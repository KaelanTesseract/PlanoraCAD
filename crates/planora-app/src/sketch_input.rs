//! Getippte Werte beim Zeichnen, ohne Oberfläche: Felder je Stufe des Werkzeugs, gesperrte Größen
//! als letzter Schritt des aufgelösten Zeigers und die Maße, die beim Abschluss daraus werden.
//! Nicht Getipptes bleibt frei und ergibt kein Maß.

use crate::formula_field::resolve;
use crate::msg::Msg;
use crate::sketch_cursor::{DrawCursor, leaving_direction};
use crate::sketch_kinds::ToolKind;
use crate::sketch_state::{
    SketchSession, Tool, TransformStage, check_change, checked_value, dimension_input, err,
};
use planora_feature::{ParamScope, ParamUnit, SketchFeature};
use planora_i18n::tr;
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind};
use planora_sketch::fillet::{ChamferIds, FilletIds};
use planora_sketch::helpers::RectIds;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::offset::OffsetIds;
use planora_sketch::shapes::{ArcSlotIds, PolygonIds, SlotIds};

/// Länge der Achshilfe (Skizzen-x-Achse) für Winkel ohne vorige Linie.
const AXIS_HELPER_LENGTH: f64 = 10.0;

/// Art eines Felds beim Zeichnen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Length,
    Angle,
    Width,
    Height,
    Diameter,
    Radius,
    Sweep,
    MajorAxis,
    MinorAxis,
    /// Tiefe (3-Punkt-Rechteck).
    Depth,
    /// Seitenzahl eines Polygons: ganzzahlig 3 bis 64, wird nie ein Maß.
    Sides,
    /// Abstand der Fase (Skizzen 4).
    Distance,
    /// Zweiter Abstand der Fase.
    Distance2,
    /// Versatz: Vorzeichen erlaubt (wechselt die Seite), nie null.
    OffsetDistance,
    /// Verschiebung in x: Vorzeichen und null erlaubt.
    MoveX,
    /// Verschiebung in y: Vorzeichen und null erlaubt.
    MoveY,
    /// Drehung in Grad: Vorzeichen erlaubt.
    MoveAngle,
    /// Faktor des Maßstabs: größer als null, ohne Einheit.
    ScaleFactor,
}

impl FieldKind {
    /// Einheit, in der das Feld gelesen wird (Winkel in Grad, Seitenzahl ohne, sonst Länge).
    pub fn unit(self) -> ParamUnit {
        match self {
            FieldKind::Angle | FieldKind::Sweep | FieldKind::MoveAngle => ParamUnit::Angle,
            FieldKind::Sides | FieldKind::ScaleFactor => ParamUnit::Number,
            _ => ParamUnit::Length,
        }
    }

    /// Name des Felds in der aktuellen Sprache.
    pub fn label(self) -> String {
        match self {
            FieldKind::Length => tr!("sketch-field-length"),
            FieldKind::Angle => tr!("sketch-field-angle"),
            FieldKind::Width => tr!("sketch-field-width"),
            FieldKind::Height => tr!("sketch-field-height"),
            FieldKind::Diameter => tr!("sketch-field-diameter"),
            FieldKind::Radius => tr!("sketch-field-radius"),
            FieldKind::Sweep => tr!("sketch-field-sweep"),
            FieldKind::MajorAxis => tr!("sketch-field-major"),
            FieldKind::MinorAxis => tr!("sketch-field-minor"),
            FieldKind::Depth => tr!("sketch-field-depth"),
            FieldKind::Sides => tr!("sketch-field-sides"),
            FieldKind::Distance => tr!("sketch-field-distance"),
            FieldKind::Distance2 => tr!("sketch-field-distance2"),
            FieldKind::OffsetDistance => tr!("sketch-field-offset"),
            FieldKind::MoveX => tr!("sketch-field-move-x"),
            FieldKind::MoveY => tr!("sketch-field-move-y"),
            FieldKind::MoveAngle => tr!("sketch-field-move-angle"),
            FieldKind::ScaleFactor => tr!("sketch-field-scale-factor"),
        }
    }
}

/// Ein Feld der aktuellen Stufe.
#[derive(Clone, Debug)]
pub struct DrawField {
    pub kind: FieldKind,
    pub text: String,
    /// Wurde schon getippt? Vorher zeigt die Oberfläche den aktuellen Wert.
    pub touched: bool,
    /// Gesperrter Wert in mm oder Bogenmaß.
    pub locked: Option<f64>,
    pub error: Option<String>,
}

/// Felder der Stufe des laufenden Werkzeugs.
#[derive(Clone, Debug, Default)]
pub struct DrawInput {
    pub fields: Vec<DrawField>,
    pub active: usize,
    /// Getippte Seitenzahl des laufenden Polygons; gilt über die Stufen bis zum Abschluss.
    pub sides: Option<usize>,
    /// Werkzeug, Art und Stand (Punkte, Lagen, Kurven, Bogenzug), zu dem die Felder gehören;
    /// ändert er sich, sind die Felder leer.
    key: (
        Option<Tool>,
        Option<ToolKind>,
        Vec<PointId>,
        usize,
        usize,
        bool,
        TransformStage,
    ),
    /// Getippte Werte früherer Stufen (Bogenradius, große Achse) bis zum Abschluss.
    carried: Vec<(FieldKind, String)>,
    /// Größte kleine Halbachse dieser Stufe (die große Halbachse mit Toleranz).
    minor_limit: Option<f64>,
}

impl DrawInput {
    pub(crate) fn locked(&self, kind: FieldKind) -> Option<f64> {
        self.fields
            .iter()
            .find(|f| f.kind == kind)
            .and_then(|f| f.locked)
    }
}

/// Was der letzte Klick fertig angelegt hat; daran hängen die Maße.
#[derive(Clone, Debug)]
pub enum Created {
    Line(CurveId),
    Rectangle(RectIds),
    Circle(CurveId),
    Arc(CurveId),
    Ellipse(CurveId),
    Polygon { ids: PolygonIds, inscribed: bool },
    Slot(SlotIds),
    ArcSlot(ArcSlotIds),
    Fillet(FilletIds),
    Chamfer(ChamferIds),
    Offset(OffsetIds),
}

impl Created {
    pub(crate) fn undo_label(&self) -> Msg {
        match self {
            Created::Line(_) => crate::msg!("undo-line"),
            Created::Rectangle(_) => crate::msg!("undo-rectangle"),
            Created::Circle(_) => crate::msg!("undo-circle"),
            Created::Arc(_) => crate::msg!("undo-arc"),
            Created::Ellipse(_) => crate::msg!("undo-ellipse"),
            Created::Polygon { .. } => crate::msg!("undo-polygon"),
            Created::Slot(_) | Created::ArcSlot(_) => crate::msg!("undo-slot"),
            Created::Fillet(_) => crate::msg!("undo-fillet"),
            Created::Chamfer(_) => crate::msg!("undo-chamfer"),
            Created::Offset(_) => crate::msg!("undo-offset"),
        }
    }

    /// Das schon angelegte Maß zum Feld `kind`: Rundung und Fase legen ihre Maße selbst an.
    fn existing_dimension(&self, kind: FieldKind) -> Option<ConstraintId> {
        match (self, kind) {
            (Created::Fillet(ids), FieldKind::Radius) => ids.radius_dim,
            (Created::Chamfer(ids), FieldKind::Distance) => ids.dims.first().copied(),
            (Created::Chamfer(ids), FieldKind::Distance2 | FieldKind::Angle) => {
                ids.dims.get(1).copied()
            }
            _ => None,
        }
    }
}

/// `d` um `angle` gegen den Uhrzeigersinn gedreht.
fn rotated(d: DVec2, angle: f64) -> DVec2 {
    DVec2::from_angle(angle).rotate(d)
}

/// Richtung von `from` zum Zeiger, bei gleicher Lage entlang der x-Achse. Das Raster zählt dabei
/// nicht: Die Richtung liefe sonst in groben Sprüngen von Rasterpunkt zu Rasterpunkt.
pub(crate) fn toward_pointer(cur: &DrawCursor, from: DVec2) -> DVec2 {
    let aim = if matches!(cur.snap, crate::sketch_state::Snap::Grid) && cur.autos.is_empty() {
        cur.raw
    } else {
        cur.at
    };
    (aim - from)
        .try_normalize()
        .or((cur.raw - from).try_normalize())
        .unwrap_or(DVec2::X)
}

/// Ende auf dem Strahl ab `from` im Winkel `angle` zu `basis`; der Zeiger wählt die Seite und, ohne
/// `len`, die Länge.
pub(crate) fn locked_ray(
    from: DVec2,
    basis: DVec2,
    cur: &DrawCursor,
    len: Option<f64>,
    angle: f64,
) -> DVec2 {
    let side = if basis.perp_dot(cur.raw - from) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let ray = rotated(basis, side * angle);
    let projected = (cur.raw - from).dot(ray);
    // Auf der Rückseite des Strahls (stumpfer Winkel) hätte die Linie sonst keine Länge; dann zählt
    // der Abstand des Zeigers.
    let free = if projected > 0.0 {
        projected
    } else {
        cur.raw.distance(from)
    };
    from + ray * len.unwrap_or(free)
}

/// Der Wert eines getippten Textes (mm oder Bogenmaß), geprüft; leer ergibt `None`.
fn read_field(
    kind: FieldKind,
    text: &str,
    scope: &ParamScope,
    minor_limit: Option<f64>,
) -> Result<Option<f64>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    // Bei der Seitenzahl ist jeder unlesbare Text (auch ein unbekannter Name) ein Bereichsfehler.
    let (v, _) = match resolve(text, kind.unit(), scope) {
        Err(_) if kind == FieldKind::Sides => return Err(tr!("sketch-field-sides-range")),
        found => found?,
    };
    match kind {
        FieldKind::Sides if !(3.0..=64.0).contains(&v) || v.fract() != 0.0 => {
            Err(tr!("sketch-field-sides-range"))
        }
        FieldKind::Angle if !(0.0..=180.0).contains(&v) => Err(tr!("sketch-field-angle-range")),
        FieldKind::Sweep if !(v > 0.0 && v < 360.0) => Err(tr!("sketch-field-sweep-range")),
        FieldKind::Angle | FieldKind::Sweep => Ok(Some(v.to_radians())),
        // Skizzen 4: Vorzeichen erlaubt, nur endlich (und der Versatz nicht null).
        FieldKind::OffsetDistance if !v.is_finite() || v == 0.0 => {
            Err(tr!("sketch-field-offset-nonzero"))
        }
        FieldKind::MoveX | FieldKind::MoveY if !v.is_finite() => {
            Err(tr!("sketch-err-value-finite"))
        }
        FieldKind::MoveAngle if !v.is_finite() => Err(tr!("sketch-err-value-finite")),
        FieldKind::MoveAngle => Ok(Some(v.to_radians())),
        FieldKind::ScaleFactor if !v.is_finite() || v <= 0.0 => {
            Err(tr!("sketch-field-factor-positive"))
        }
        FieldKind::OffsetDistance | FieldKind::MoveX | FieldKind::MoveY => Ok(Some(v)),
        _ if !v.is_finite() || v <= 0.0 => Err(tr!("sketch-field-positive")),
        FieldKind::MinorAxis if minor_limit.is_some_and(|l| v > l) => {
            Err(tr!("sketch-field-minor-too-big"))
        }
        _ => Ok(Some(v)),
    }
}

/// Der getippte Versatz als Text des Maßes: Ein Maß hat kein Vorzeichen, das Minus hat schon die
/// Seite gewechselt. Eine Formel bleibt eine Formel, nur ohne das Minus.
fn magnitude_text(text: &str, scope: &ParamScope) -> Result<String, Msg> {
    let unit = FieldKind::OffsetDistance.unit();
    let (value, _) = resolve(text, unit, scope)?;
    if value >= 0.0 {
        return Ok(text.to_string());
    }
    // „-a + b“ ohne Minus wäre etwas anderes; darum zählt der nachgerechnete Wert.
    let bare = text.trim().trim_start_matches(['-', '\u{2212}']).trim();
    [bare.to_string(), format!("-({text})")]
        .into_iter()
        .find(|t| resolve(t, unit, scope).is_ok_and(|(v, _)| v == -value))
        .ok_or_else(|| crate::msg!("sketch-field-rejected"))
}

/// Die Hilfslinie entlang der Skizzen-x-Achse: einmal je Skizze eine Konstruktionslinie
/// (0,0)-(10,0) mit festen Enden, also ohne Freiheitsgrad, weil `Angle` zwei Linien braucht und
/// das Dateiformat keine Achse kennt.
pub(crate) fn axis_reference(sk: &mut Sketch) -> Result<CurveId, Msg> {
    use ConstraintKind as K;
    let fixed_at = |p: PointId, x: f64, y: f64| {
        sk.constraints().any(|(_, c)| {
            c.enabled
                && !c.driven
                && matches!(c.kind, K::Fixed(q, a, b) if q == p && a == x && b == y)
        })
    };
    let found = sk.curves().find_map(|(c, data)| match data.kind {
        CurveKind::Line { a, b }
            if data.construction
                && fixed_at(a, 0.0, 0.0)
                && fixed_at(b, AXIS_HELPER_LENGTH, 0.0) =>
        {
            Some(c)
        }
        _ => None,
    });
    if let Some(c) = found {
        return Ok(c);
    }
    let a = sk.add_point(0.0, 0.0).map_err(err)?;
    let b = sk.add_point(AXIS_HELPER_LENGTH, 0.0).map_err(err)?;
    let line = sk.add_line(a, b).map_err(err)?;
    sk.set_construction(line, true).map_err(err)?;
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).map_err(err)?;
    sk.add_constraint(K::Fixed(b, AXIS_HELPER_LENGTH, 0.0))
        .map_err(err)?;
    Ok(line)
}

/// Winkelbezug einer Linie ab `start`: die vorige Linie der Kette in Vorwärtsrichtung. Ein Bogen ist
/// keiner, denn ein Winkelmaß gibt es nur zwischen Linien.
fn chain_basis(sk: &Sketch, chain: Option<CurveId>, start: PointId) -> Option<(CurveId, DVec2)> {
    let c =
        chain.filter(|&c| matches!(sk.curve(c).map(|d| &d.kind), Some(CurveKind::Line { .. })))?;
    Some((c, leaving_direction(sk, c, start)?))
}

/// Bezugsrichtung für den Winkel einer Linie ab `start`: vorige Linie, sonst die x-Achse.
fn basis_direction(sk: &Sketch, chain: Option<CurveId>, start: PointId) -> DVec2 {
    chain_basis(sk, chain, start).map_or(DVec2::X, |(_, d)| d)
}

impl SketchSession {
    /// Felder der aktuellen Stufe; nur bei geändertem Werkzeug oder Stand neu (dann leer).
    pub fn sync_input(&mut self, sk: &Sketch) {
        let key = (
            Some(self.tool),
            self.planned_kind(),
            self.pending.clone(),
            self.pending_pos.len(),
            self.pending_curves.len(),
            self.line_arc.is_some(),
            self.transform.stage,
        );
        if self.input.key == key {
            return;
        }
        // Ohne offene Punkte läuft kein Werkzeug mehr; was es getippt hatte, gilt nicht mehr.
        if self.pending.is_empty() && self.pending_pos.is_empty() && self.pending_curves.is_empty()
        {
            self.input.carried.clear();
        }
        self.input.fields = self
            .stage_fields()
            .iter()
            .map(|&kind| DrawField {
                kind,
                text: String::new(),
                touched: false,
                locked: None,
                error: None,
            })
            .collect();
        self.input.active = 0;
        self.input.key = key;
        self.input.minor_limit = self.minor_limit(sk);
    }

    /// Felder je Stufe: Linie Länge und Winkel, Rechteck Breite und Höhe, Kreis Durchmesser, Bogen
    /// erst Radius, dann Öffnungswinkel, Ellipse erst große, dann kleine Halbachse.
    fn stage_fields(&self) -> &'static [FieldKind] {
        use FieldKind as F;
        if self.line_arc.is_some() {
            return &[];
        }
        if let Some(kind) = self.planned_kind() {
            return kind.stages().get(self.kind_stage()).copied().unwrap_or(&[]);
        }
        let (pts, pos) = (self.pending.len(), self.pending_pos.len());
        match self.tool {
            Tool::Offset if !self.pending_curves.is_empty() => &[F::OffsetDistance],
            Tool::MoveCopy if self.transform.stage == TransformStage::Active => {
                &[F::MoveX, F::MoveY, F::MoveAngle]
            }
            Tool::Scale if self.transform.stage == TransformStage::Active => &[F::ScaleFactor],
            Tool::Line if pts > 0 => &[F::Length, F::Angle],
            Tool::Rectangle if pos > 0 => &[F::Width, F::Height],
            Tool::Circle if pts > 0 => &[F::Diameter],
            Tool::Arc if pts == 1 => &[F::Radius],
            Tool::Arc if pts > 1 => &[F::Sweep],
            Tool::Ellipse if pts > 0 && pos == 0 => &[F::MajorAxis],
            Tool::Ellipse if pts > 0 => &[F::MinorAxis],
            _ => &[],
        }
    }

    fn minor_limit(&self, sk: &Sketch) -> Option<f64> {
        if self.tool != Tool::Ellipse {
            return None;
        }
        let m = sk.point_xy(*self.pending.first()?).ok()?;
        Some(self.pending_pos.first()?.distance(m) + sk.tolerance().linear)
    }

    /// Text des Felds `i`: Gültiges sperrt sofort, leer gibt frei, Ungültiges gibt frei und zeigt
    /// die Meldung (der Abschluss wird dann abgelehnt).
    pub fn type_field(&mut self, i: usize, text: &str, scope: &ParamScope) {
        let limit = self.input.minor_limit;
        let Some(field) = self.input.fields.get_mut(i) else {
            return;
        };
        field.text = text.to_string();
        field.touched = true;
        match read_field(field.kind, text, scope, limit) {
            Ok(locked) => {
                field.locked = locked;
                field.error = None;
            }
            Err(e) => {
                field.locked = None;
                field.error = Some(e);
            }
        }
        // Solange die Seitenzahl im Fehler steht, bleibt die letzte gültige (keine springende Vorschau).
        if field.kind == FieldKind::Sides && (field.locked.is_some() || text.trim().is_empty()) {
            self.input.sides = field.locked.map(|v| v as usize);
        }
        self.limit_radius_or_sweep(i);
    }

    /// Start und Ende legen die Sehne fest, also bestimmt beim 3-Punkt-Bogen-Langloch Radius oder
    /// Öffnungswinkel den Bogen: Das zweite getippte Feld `i` nennt die Regel.
    fn limit_radius_or_sweep(&mut self, i: usize) {
        if self.planned_kind() != Some(ToolKind::SlotThreePointArc) {
            return;
        }
        let both = self.input.locked(FieldKind::Radius).is_some()
            && self.input.locked(FieldKind::Sweep).is_some();
        if both && let Some(field) = self.input.fields.get_mut(i) {
            field.locked = None;
            field.error = Some(tr!("sketch-field-radius-or-sweep"));
        }
    }

    /// Nächstes oder (`back`) voriges Feld; die Sperren bleiben.
    pub fn next_field(&mut self, back: bool) {
        let n = self.input.fields.len();
        if n > 0 {
            let step = if back { n - 1 } else { 1 };
            self.input.active = (self.input.active + step) % n;
        }
    }

    /// Aktueller Wert des Felds der Art `kind` (mm oder Bogenmaß): die Sperre, sonst aus der
    /// Vorschau dieses Bilds. `None`, wenn die Stufe das Feld nicht hat oder kein Zeiger da ist.
    pub fn field_value(&self, sk: &Sketch, kind: FieldKind) -> Option<f64> {
        use FieldKind as F;
        let field = self.input.fields.iter().find(|f| f.kind == kind)?;
        if field.locked.is_some() {
            return field.locked;
        }
        if self.planned_kind().is_some() {
            return self.kind_field_value(sk, kind);
        }
        if matches!(kind, F::MoveX | F::MoveY | F::MoveAngle | F::ScaleFactor) {
            return self.transform_field_value(kind);
        }
        let at = self.draw_cursor.as_ref()?.at;
        if kind == F::OffsetDistance {
            return self.offset_pointer_distance(sk, at);
        }
        if matches!(kind, F::Width | F::Height) {
            let size = at - *self.pending_pos.first()?;
            return Some(if kind == F::Width { size.x } else { size.y }.abs());
        }
        let &first = self.pending.first()?;
        let from = sk.point_xy(first).ok()?;
        let along = at - from;
        Some(match kind {
            F::Length | F::Radius | F::MajorAxis => along.length(),
            F::Diameter => 2.0 * along.length(),
            F::Angle => {
                let basis = basis_direction(sk, self.line_chain, first);
                basis.perp_dot(along).abs().atan2(basis.dot(along))
            }
            F::Sweep => {
                let start = sk.point_xy(*self.pending.get(1)?).ok()? - from;
                start
                    .perp_dot(along)
                    .atan2(start.dot(along))
                    .rem_euclid(std::f64::consts::TAU)
            }
            F::MinorAxis => {
                let major = (*self.pending_pos.first()? - from).try_normalize()?;
                major.perp_dot(along).abs()
            }
            // Die Felder von Skizzen 4 zeigen ihre Vorschau selbst.
            F::Width
            | F::Height
            | F::Depth
            | F::Sides
            | F::Distance
            | F::Distance2
            | F::OffsetDistance
            | F::MoveX
            | F::MoveY
            | F::MoveAngle
            | F::ScaleFactor => return None,
        })
    }

    /// Letzter Schritt des aufgelösten Zeigers: Gesperrte Größen gewinnen immer. Trifft die
    /// Endlage dann das Fangziel oder die Hilfslinie nicht mehr, entfallen sie.
    pub(crate) fn apply_locks(&mut self, sk: &Sketch, cur: &mut DrawCursor) {
        self.sync_input(sk);
        let Some(at) = self.locked_position(sk, cur) else {
            return;
        };
        let tol = sk.tolerance().linear;
        cur.at = at;
        if cur.snap_at.is_some_and(|s| s.distance(at) > tol) {
            cur.snap = crate::sketch_state::Snap::None;
            cur.snap_at = None;
        }
        if cur.guides.x.is_some_and(|(_, x)| (at.x - x).abs() > tol) {
            cur.guides.x = None;
        }
        if cur.guides.y.is_some_and(|(_, y)| (at.y - y).abs() > tol) {
            cur.guides.y = None;
        }
        // Die Richtung steht jetzt fest, und ein Bogenanfang abseits des Punktes läuft nicht
        // tangential an ihm weiter.
        let angle = self.input.locked(FieldKind::Angle).is_some();
        let arc_start = self.tool == Tool::Arc && self.pending.len() == 1;
        if angle || (arc_start && cur.point(tol).is_none()) {
            cur.autos.clear();
        }
    }

    /// Lage mit allen gesperrten Größen; `None` ohne Sperre.
    fn locked_position(&self, sk: &Sketch, cur: &DrawCursor) -> Option<DVec2> {
        use FieldKind as F;
        if self.planned_kind().is_some() {
            return self.kind_locked_position(sk, cur);
        }
        let input = &self.input;
        let first = self
            .pending
            .first()
            .and_then(|&p| sk.point_xy(p).ok().map(|xy| (p, xy)));
        let toward = |from: DVec2| toward_pointer(cur, from);
        match self.tool {
            Tool::Line => {
                let (start, from) = first?;
                let (len, angle) = (input.locked(F::Length), input.locked(F::Angle));
                let Some(angle) = angle else {
                    return Some(from + toward(from) * len?);
                };
                let basis = basis_direction(sk, self.line_chain, start);
                Some(locked_ray(from, basis, cur, len, angle))
            }
            Tool::Rectangle => {
                let &corner = self.pending_pos.first()?;
                let (w, h) = (input.locked(F::Width), input.locked(F::Height));
                if w.is_none() && h.is_none() {
                    return None;
                }
                let size = cur.at - corner;
                Some(DVec2::new(
                    w.map_or(cur.at.x, |w| corner.x + size.x.signum() * w),
                    h.map_or(cur.at.y, |h| corner.y + size.y.signum() * h),
                ))
            }
            Tool::Circle => {
                let (_, m) = first?;
                Some(m + toward(m) * (input.locked(F::Diameter)? / 2.0))
            }
            Tool::Arc => {
                let (_, m) = first?;
                match self.pending.get(1) {
                    None => Some(m + toward(m) * input.locked(F::Radius)?),
                    Some(&start) => {
                        let s = sk.point_xy(start).ok()?;
                        Some(m + rotated(s - m, input.locked(F::Sweep)?))
                    }
                }
            }
            Tool::Ellipse => {
                let (_, m) = first?;
                match self.pending_pos.first() {
                    None => Some(m + toward(m) * input.locked(F::MajorAxis)?),
                    Some(&end) => {
                        let normal = (end - m).try_normalize()?.perp();
                        let side = normal.dot(cur.at - m).signum();
                        Some(m + normal * side * input.locked(F::MinorAxis)?)
                    }
                }
            }
            _ => None,
        }
    }

    /// Setzt den Punkt an der aufgelösten Lage; getippte Werte werden beim Abschluss der Kurve zu
    /// Maßen, die einer früheren Stufe warten bis dahin. Ohne getippte Werte wie
    /// [`click`](Self::click). Atomar: Ein ungültiges Feld oder ein widersprüchliches Maß lässt
    /// Entwurf und Sitzung, wie sie waren, nur `message` erklärt es. Ein überzähliges Maß wird
    /// anzeigend.
    pub fn commit_input(
        &mut self,
        sf: &mut SketchFeature,
        raw: DVec2,
        radius: f64,
        extend: bool,
        scope: &ParamScope,
    ) -> Result<(), String> {
        self.sync_input(&sf.sketch);
        if let Some(e) = self.input.fields.iter().find_map(|f| f.error.clone()) {
            self.message = Some(Msg::Raw(e.clone()));
            return Err(e);
        }
        let typed = self.input.fields.iter().any(|f| f.locked.is_some());
        if !typed && self.input.carried.is_empty() {
            let result = self.click(&mut sf.sketch, raw, radius, extend);
            self.sync_input(&sf.sketch);
            return result;
        }
        let (saved_session, saved_draft) = (self.clone(), sf.clone());
        match self.commit_typed(sf, raw, radius, extend, scope) {
            Ok(()) => Ok(()),
            Err(e) => {
                let text = e.text();
                *self = saved_session;
                *sf = saved_draft;
                self.message = Some(e);
                Err(text)
            }
        }
    }

    /// Eingabe: [`commit_input`](Self::commit_input) dort, wo die Vorschau steht; ohne Zeiger nichts.
    pub fn commit_at_cursor(
        &mut self,
        sf: &mut SketchFeature,
        radius: f64,
        scope: &ParamScope,
    ) -> Result<(), String> {
        let raw = self.draw_cursor.as_ref().map(|c| c.raw).or(self.cursor);
        match raw {
            Some(raw) => self.commit_input(sf, raw, radius, false, scope),
            None => Ok(()),
        }
    }

    fn commit_typed(
        &mut self,
        sf: &mut SketchFeature,
        raw: DVec2,
        radius: f64,
        extend: bool,
        scope: &ParamScope,
    ) -> Result<(), Msg> {
        let key = self.input.key.clone();
        let chain = self.line_chain;
        let mut typed = self.input.carried.clone();
        typed.extend(
            self.input
                .fields
                .iter()
                // Die Seitenzahl ist kein Maß; sie steht in `DrawInput::sides`. Verschieben und
                // Maßstab lesen ihre Felder selbst.
                .filter(|f| {
                    f.locked.is_some()
                        && !matches!(
                            f.kind,
                            FieldKind::Sides
                                | FieldKind::MoveX
                                | FieldKind::MoveY
                                | FieldKind::MoveAngle
                                | FieldKind::ScaleFactor
                        )
                })
                .map(|f| (f.kind, f.text.clone())),
        );
        if let Err(text) = self.click(&mut sf.sketch, raw, radius, extend) {
            return Err(self.message.take().unwrap_or(Msg::Raw(text)));
        }
        let Some(created) = self.created.take() else {
            // Nächste Stufe: Die Werte warten; ohne Fortschritt bleibt alles, wie es war.
            self.sync_input(&sf.sketch);
            if self.input.key != key {
                self.input.carried = typed;
            }
            return Ok(());
        };
        // Der Versatz legt sein Maß selbst an (oder keins): Der getippte Text wird dessen Wert und
        // Formel, und was der Klick zu sagen hatte, bleibt stehen.
        if let Created::Offset(ids) = &created {
            let note = self.message.take();
            let text = typed.iter().find(|(k, _)| *k == FieldKind::OffsetDistance);
            if let (Some(id), Some((_, text))) = (ids.distance_dim, text) {
                let text = magnitude_text(text, scope)?;
                if let Err(text) = self.set_dimension_input(sf, id, &text, scope) {
                    return Err(self.message.take().unwrap_or(Msg::Raw(text)));
                }
            }
            if note.is_some() {
                self.message = note;
            }
            self.edit_label = Some(created.undo_label());
            self.sync_input(&sf.sketch);
            return Ok(());
        }
        let mut note = None;
        for (kind, text) in &typed {
            // Rundung und Fase: Wert und Formel gehören an das Maß, das schon da ist.
            if let Some(id) = created.existing_dimension(*kind) {
                if let Err(text) = self.set_dimension_input(sf, id, text, scope) {
                    return Err(self.message.take().unwrap_or(Msg::Raw(text)));
                }
                continue;
            }
            let overall = self.overall_center_distance(sf, &created, *kind, &typed, scope)?;
            note = self
                .add_typed_dimension(sf, &created, *kind, text, overall, chain, scope)?
                .or(note);
        }
        if note.is_some() {
            self.message = note;
        }
        self.edit_label = Some(created.undo_label());
        self.sync_input(&sf.sketch);
        Ok(())
    }

    /// Der Mittenabstand `L − W` zur getippten Gesamtlänge eines Langlochs „Gesamt“ (Ruling Task 2:
    /// Die Gesamtlänge ist kein eigenes Maß); `W` ist die getippte Breite oder die vom Zeiger. `None`
    /// für jedes andere Maß.
    fn overall_center_distance(
        &self,
        sf: &SketchFeature,
        created: &Created,
        kind: FieldKind,
        typed: &[(FieldKind, String)],
        scope: &ParamScope,
    ) -> Result<Option<f64>, Msg> {
        let (Some(ToolKind::SlotOverall), Created::Slot(ids), FieldKind::Length) =
            (self.planned_kind(), created, kind)
        else {
            return Ok(None);
        };
        let value_of = |wanted: FieldKind| {
            typed
                .iter()
                .find(|(k, _)| *k == wanted)
                .map(|(k, text)| dimension_input(text, k.unit(), scope).map(|(v, _)| v))
                .transpose()
        };
        let length =
            value_of(FieldKind::Length)?.ok_or_else(|| crate::msg!("sketch-field-rejected"))?;
        let width = match value_of(FieldKind::Width)? {
            Some(w) => w,
            None => {
                2.0 * sf
                    .sketch
                    .radius(ids.caps[0])
                    .ok_or_else(|| crate::msg!("sketch-field-rejected"))?
            }
        };
        Ok(Some(length - width))
    }

    /// Legt das Maß eines getippten Werts an. Widerspruch ist ein Fehler; `Some`: das Maß war
    /// überzählig und ist anzeigend, das ist die Meldung dazu. Ein überzähliger Öffnungswinkel
    /// entfällt samt seinen beiden Hilfslinien, die sonst keinen Zweck hätten. `computed` ersetzt den
    /// getippten Wert; das Maß bekommt dann weder Formel noch Speichertext.
    #[allow(clippy::too_many_arguments)]
    fn add_typed_dimension(
        &mut self,
        sf: &mut SketchFeature,
        created: &Created,
        kind: FieldKind,
        text: &str,
        computed: Option<f64>,
        chain: Option<CurveId>,
        scope: &ParamScope,
    ) -> Result<Option<Msg>, Msg> {
        let (typed, _) = dimension_input(text, kind.unit(), scope)?;
        let value = computed.unwrap_or(typed);
        let prior = sf.sketch.clone();
        let dim = dimension_kind(&mut sf.sketch, created, kind, chain)?;
        let dim = checked_value(&sf.sketch, &dim, value)?;
        let old = sf.sketch.measure(&dim);
        let id = sf.sketch.add_constraint(dim).map_err(err)?;
        let unsolved = sf.sketch.clone();
        let before = self.last_report.take();
        let report = sf.sketch.solve();
        let direct = match check_change(&report, before.as_ref(), &prior, Some(id)) {
            Some(_) => Err(crate::msg!("sketch-field-rejected")),
            None => Ok(()),
        };
        let redundant = report.redundant.contains(&id);
        if direct.is_ok() && redundant && kind == FieldKind::Sweep {
            sf.sketch = prior;
            self.last_report = before;
            return Ok(Some(crate::msg!("sketch-field-sweep-redundant")));
        }
        self.last_report = Some(report);
        self.guard_length(&mut sf.sketch, id, old, &unsolved, direct)?;
        if redundant {
            self.make_driven(&mut sf.sketch, id)?;
            return Ok(self.message.take());
        }
        if computed.is_some() {
            return Ok(None);
        }
        if let Err(text) = self.set_dimension_input(sf, id, text, scope) {
            return Err(self.message.take().unwrap_or(Msg::Raw(text)));
        }
        Ok(None)
    }
}

/// Das Maß der Art `kind` an der fertigen Kurve, noch ohne Wert. Winkel: `Angle(bezug, linie)`
/// oder `Angle(linie, bezug)`, je nachdem, zu welcher Seite die Linie abgeht, damit der Wert
/// positiv bleibt. Öffnungswinkel: zwei Konstruktionslinien Mitte→Start und Mitte→Ende.
fn dimension_kind(
    sk: &mut Sketch,
    created: &Created,
    kind: FieldKind,
    chain: Option<CurveId>,
) -> Result<ConstraintKind, Msg> {
    use ConstraintKind as K;
    use FieldKind as F;
    Ok(match (created, kind) {
        (Created::Line(line), F::Length) => {
            let (a, b) = line_ends(sk, *line)?;
            K::Distance(a, b, 0.0)
        }
        (Created::Line(line), F::Angle) => line_angle(sk, *line, chain)?,
        (Created::Rectangle(r), F::Width) => K::Distance(r.points[0], r.points[1], 0.0),
        (Created::Rectangle(r), F::Height | F::Depth) => K::Distance(r.points[1], r.points[2], 0.0),
        (Created::Rectangle(r), F::Angle) => line_angle(sk, r.lines[0], chain)?,
        (Created::Circle(c), F::Diameter) => K::Diameter(*c, 0.0),
        (Created::Arc(c), F::Radius) => K::Radius(*c, 0.0),
        (Created::Arc(c), F::Sweep) => sweep_dimension(sk, *c)?,
        // Umbeschrieben gibt die Größe den Abstand Mitte–Seite an, einbeschrieben den Radius.
        (
            Created::Polygon {
                ids,
                inscribed: true,
            },
            F::Radius,
        ) => K::Radius(ids.circle, 0.0),
        (Created::Polygon { ids, .. }, F::Radius) => {
            K::DistancePointLine(ids.center, ids.lines[0], 0.0)
        }
        (Created::Polygon { ids, .. }, F::Length) => K::Distance(ids.points[0], ids.points[1], 0.0),
        (Created::Slot(ids), F::Length) => K::Distance(ids.centers[0], ids.centers[1], 0.0),
        (Created::Slot(ids), F::Width) => K::Diameter(ids.caps[0], 0.0),
        (Created::ArcSlot(ids), F::Radius) => K::Radius(ids.axis, 0.0),
        (Created::ArcSlot(ids), F::Sweep) => sweep_dimension(sk, ids.axis)?,
        (Created::ArcSlot(ids), F::Width) => K::Diameter(ids.caps[0], 0.0),
        (Created::Ellipse(c), F::MajorAxis) => K::EllipseMajor(*c, 0.0),
        (Created::Ellipse(c), F::MinorAxis) => K::EllipseMinor(*c, 0.0),
        _ => return Err(crate::msg!("sketch-field-rejected")),
    })
}

/// Der Öffnungswinkel des Bogens `arc`: über zwei neue Konstruktionslinien Mitte→Start und Mitte→Ende.
fn sweep_dimension(sk: &mut Sketch, arc: CurveId) -> Result<ConstraintKind, Msg> {
    let Some(CurveKind::Arc { center, start, end }) = sk.curve(arc).map(|d| d.kind.clone()) else {
        return Err(crate::msg!("sketch-field-rejected"));
    };
    let first = sk.add_line(center, start).map_err(err)?;
    let second = sk.add_line(center, end).map_err(err)?;
    sk.set_construction(first, true).map_err(err)?;
    sk.set_construction(second, true).map_err(err)?;
    Ok(ConstraintKind::Angle(first, second, 0.0))
}

/// Der Winkel der Linie zur vorigen Linie der Kette, sonst zur Achshilfe.
fn line_angle(
    sk: &mut Sketch,
    line: CurveId,
    chain: Option<CurveId>,
) -> Result<ConstraintKind, Msg> {
    let (a, b) = line_ends(sk, line)?;
    let (from, to) = (sk.point_xy(a).map_err(err)?, sk.point_xy(b).map_err(err)?);
    if from.distance(to) <= sk.tolerance().linear {
        return Err(crate::msg!("sketch-field-positive"));
    }
    let (basis, dir) = match chain_basis(sk, chain, a) {
        Some(found) => found,
        None => (axis_reference(sk)?, DVec2::X),
    };
    Ok(if dir.perp_dot(to - from) < 0.0 {
        ConstraintKind::Angle(line, basis, 0.0)
    } else {
        ConstraintKind::Angle(basis, line, 0.0)
    })
}

fn line_ends(sk: &Sketch, line: CurveId) -> Result<(PointId, PointId), Msg> {
    match sk.curve(line).map(|d| &d.kind) {
        Some(CurveKind::Line { a, b }) => Ok((*a, *b)),
        _ => Err(crate::msg!("sketch-field-rejected")),
    }
}
