//! Fenster der Konstruktionsebenen (Skizzen 6, Menü „Konstruieren“): Versatzebene, Ebene unter
//! Winkel, Mittelebene, Ebene durch drei Punkte und Tangentialebene wie in Autodesk Fusion.
//!
//! Jedes Fenster hat Auswahlfelder (wie das Kantenfeld von „Verrunden“): Das aktive Feld nimmt
//! den nächsten Klick in der 3D-Ansicht ([`AppState::plane_pick`], mit dem passenden
//! [`PickFilter`]) oder im Ordner „Ursprung“ des Browsers. Abstand bzw. Winkel stehen in einem
//! Formelfeld. Die Vorschau zeigt die Ebene in der Hervorhebungsfarbe, berechnet mit
//! [`compute_plane`], ohne das Dokument zu ändern. „OK“ ist genau ein Rückgängig-Schritt,
//! „Abbrechen“, ✕ und Esc hinterlassen nichts. „Bearbeiten“ öffnet dasselbe Fenster mit den
//! Werten einer bestehenden Ebene.

use crate::browser::error_color;
use crate::dialog::{Dialog, DialogSize};
use crate::formula_field::{FormulaField, names_hint, resolve_kept, stored_text};
use crate::keymap::Action;
use crate::msg::{Msg, MsgArg};
use crate::state::{AppState, MAX_COORD_MM, PickFilter};
use planora_feature::planes::{
    AxisRef, PlaneBase, PlaneDef, PlaneFeature, PointRef, StdAxis, compute_plane,
};
use planora_feature::{
    EdgeRef, FaceRef, FeatureId, FeatureKind, FieldKey, Formulas, ParamUnit, StandardPlane,
    field_label,
};
use planora_i18n::tr;
use planora_kernel::Solid;
use planora_math::Frame;
use planora_render::{Pick, PlaneRect};
use planora_sketch::plane_of_face;

/// Die fünf Befehle des Menüs „Konstruieren“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneCommand {
    Offset,
    Angle,
    Mid,
    ThreePoints,
    Tangent,
}

impl PlaneCommand {
    /// Reihenfolge im Menü (wie Fusion).
    pub const ALL: [PlaneCommand; 5] = [
        PlaneCommand::Offset,
        PlaneCommand::Angle,
        PlaneCommand::Tangent,
        PlaneCommand::Mid,
        PlaneCommand::ThreePoints,
    ];

    /// Name des Befehls (Menü, Fenstertitel).
    pub fn label(self) -> String {
        match self {
            PlaneCommand::Offset => tr!("plane-cmd-offset"),
            PlaneCommand::Angle => tr!("plane-cmd-angle"),
            PlaneCommand::Mid => tr!("plane-cmd-mid"),
            PlaneCommand::ThreePoints => tr!("plane-cmd-three-points"),
            PlaneCommand::Tangent => tr!("plane-cmd-tangent"),
        }
    }

    /// Aktion der Tastenbelegung (ohne Vorgabetaste, belegbar).
    pub fn action(self) -> Action {
        match self {
            PlaneCommand::Offset => Action::PlaneOffset,
            PlaneCommand::Angle => Action::PlaneAngle,
            PlaneCommand::Mid => Action::PlaneMid,
            PlaneCommand::ThreePoints => Action::PlaneThreePoints,
            PlaneCommand::Tangent => Action::PlaneTangent,
        }
    }

    /// Der Befehl zu einer Aktion.
    pub fn for_action(a: Action) -> Option<PlaneCommand> {
        PlaneCommand::ALL.into_iter().find(|c| c.action() == a)
    }

    /// Der Befehl, der eine Ebene mit dieser Definition anlegt.
    pub fn of(def: &PlaneDef) -> PlaneCommand {
        match def {
            PlaneDef::Offset { .. } => PlaneCommand::Offset,
            PlaneDef::Angle { .. } => PlaneCommand::Angle,
            PlaneDef::Mid { .. } => PlaneCommand::Mid,
            PlaneDef::ThreePoints(_) => PlaneCommand::ThreePoints,
            PlaneDef::Tangent { .. } => PlaneCommand::Tangent,
        }
    }

    /// Fester englischer Name für das Protokoll.
    pub fn command_name(self) -> &'static str {
        match self {
            PlaneCommand::Offset => "Offset plane",
            PlaneCommand::Angle => "Plane at angle",
            PlaneCommand::Mid => "Midplane",
            PlaneCommand::ThreePoints => "Plane through three points",
            PlaneCommand::Tangent => "Tangent plane",
        }
    }

    /// Die Auswahlfelder in der Reihenfolge des Fensters.
    pub fn slots(self) -> &'static [SlotKind] {
        match self {
            PlaneCommand::Offset => &[SlotKind::Base],
            PlaneCommand::Angle => &[SlotKind::Axis],
            PlaneCommand::Mid => &[SlotKind::Base, SlotKind::Base],
            PlaneCommand::ThreePoints => &[SlotKind::Point, SlotKind::Point, SlotKind::Point],
            PlaneCommand::Tangent => &[SlotKind::Face, SlotKind::Base],
        }
    }

    /// Das Zahlenfeld (Abstand in mm oder Winkel in Grad), falls es eines gibt.
    fn number(self) -> Option<(FieldKey, ParamUnit)> {
        match self {
            PlaneCommand::Offset => Some((FieldKey::Abstand, ParamUnit::Length)),
            PlaneCommand::Angle | PlaneCommand::Tangent => {
                Some((FieldKey::Winkel, ParamUnit::Angle))
            }
            PlaneCommand::Mid | PlaneCommand::ThreePoints => None,
        }
    }

    /// Bezeichnung eines Auswahlfelds (`i` ab 0).
    fn slot_label(self, i: usize) -> String {
        match (self, i) {
            (PlaneCommand::Offset, _) => tr!("plane-slot-plane"),
            (PlaneCommand::Angle, _) => tr!("plane-slot-line"),
            (PlaneCommand::Mid, 0) => tr!("plane-slot-first-plane"),
            (PlaneCommand::Mid, _) => tr!("plane-slot-second-plane"),
            (PlaneCommand::ThreePoints, n) => tr!("plane-slot-point-n", n = n + 1),
            (PlaneCommand::Tangent, 0) => tr!("plane-slot-face"),
            (PlaneCommand::Tangent, _) => tr!("plane-slot-reference"),
        }
    }
}

/// Was ein Auswahlfeld nimmt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    /// Ebene, ebene Fläche oder Grundebene.
    Base,
    /// Gerade Kante oder Ursprungsachse.
    Axis,
    /// Eckpunkt eines Körpers oder der Ursprung.
    Point,
    /// Zylinder-, Kegel- oder Kugelfläche.
    Face,
}

impl SlotKind {
    /// Was ein Klick in der 3D-Ansicht für dieses Feld treffen kann.
    pub fn filter(self) -> PickFilter {
        match self {
            SlotKind::Base => PickFilter::SELECT,
            SlotKind::Axis | SlotKind::Face => PickFilter::GEOMETRY,
            SlotKind::Point => PickFilter::POINTS,
        }
    }

    /// Hinweis im leeren Feld.
    fn prompt(self) -> String {
        match self {
            SlotKind::Base => tr!("plane-pick-base"),
            SlotKind::Axis => tr!("plane-pick-axis"),
            SlotKind::Point => tr!("plane-pick-point"),
            SlotKind::Face => tr!("plane-pick-face"),
        }
    }
}

/// Inhalt eines Auswahlfelds.
#[derive(Clone, Debug, PartialEq)]
pub enum SlotValue {
    Base(PlaneBase),
    Axis(AxisRef),
    Point(PointRef),
    Face(FaceRef),
}

/// Ein Klick, den das aktive Auswahlfeld bekommt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaneClick {
    /// In der 3D-Ansicht (oder eine Ebene im Browser).
    View(Pick),
    /// Eine Grundebene im Ordner „Ursprung“.
    Standard(StandardPlane),
}

/// Zustand eines offenen Ebenen-Fensters, den Ansicht und Browser sehen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanePick {
    /// Was ein Klick in der Ansicht für das aktive Feld treffen kann.
    pub filter: PickFilter,
    /// Der letzte Klick, den das Fenster noch nicht abgeholt hat.
    pub clicked: Option<PlaneClick>,
    /// Vorschau der Ebene (in der Hervorhebungsfarbe gezeichnet).
    pub preview: Option<PlaneRect>,
}

/// Die bearbeitete Ebene.
#[derive(Clone, Debug)]
struct Editing {
    id: FeatureId,
    name: String,
    /// Stand des Features beim Öffnen ([`planora_feature::Feature::version`]).
    version: u64,
}

/// Offenes Fenster einer Konstruktionsebene.
#[derive(Clone, Debug)]
pub struct PlaneDialog {
    command: PlaneCommand,
    editing: Option<Editing>,
    slots: Vec<Option<SlotValue>>,
    /// Feld, das den nächsten Klick bekommt.
    active: usize,
    /// Text des Zahlenfelds (Zahl oder Formel, in der Anzeigeeinheit).
    text: String,
    /// Gespeicherter Wert und Formel, aus denen der Text vorbelegt wurde: Bleibt der Text
    /// unverändert, bleiben sie es auch.
    value: f64,
    formula: Option<String>,
    error: Option<Msg>,
}

/// Name eines Features für die Anzeige im Feld.
fn feature_name(state: &AppState, id: FeatureId) -> String {
    state
        .document
        .feature(id)
        .map_or_else(|| tr!("state-feature-n", n = id.0), |f| f.name.clone())
}

fn standard_label(p: StandardPlane) -> String {
    match p {
        StandardPlane::XY => tr!("tree-plane-xy"),
        StandardPlane::XZ => tr!("tree-plane-xz"),
        StandardPlane::YZ => tr!("tree-plane-yz"),
    }
}

fn axis_label(a: StdAxis) -> String {
    match a {
        StdAxis::X => tr!("plane-axis-x"),
        StdAxis::Y => tr!("plane-axis-y"),
        StdAxis::Z => tr!("plane-axis-z"),
    }
}

/// Text eines gefüllten Felds („XY-Ebene“, „Fläche von Quader 1“, „Ebene 2“ …).
pub fn describe(state: &AppState, v: &SlotValue) -> String {
    match v {
        SlotValue::Base(PlaneBase::Standard(p)) => standard_label(*p),
        SlotValue::Base(PlaneBase::Plane(id)) => feature_name(state, *id),
        SlotValue::Base(PlaneBase::Face(r)) | SlotValue::Face(r) => tr!(
            "plane-value-face",
            name = feature_name(state, FeatureId(r.origin))
        ),
        SlotValue::Axis(AxisRef::Standard(a)) => axis_label(*a),
        SlotValue::Axis(AxisRef::Edge(e)) => tr!(
            "plane-value-edge",
            name = feature_name(state, FeatureId(e.a.origin))
        ),
        SlotValue::Axis(AxisRef::SketchLine { sketch, .. }) => tr!(
            "plane-value-sketch-line",
            name = feature_name(state, *sketch)
        ),
        SlotValue::Point(PointRef::Origin) => tr!("plane-value-origin"),
        SlotValue::Point(PointRef::EdgeEnd { edge, .. }) => tr!(
            "plane-value-vertex",
            name = feature_name(state, FeatureId(edge.a.origin))
        ),
        SlotValue::Point(PointRef::SketchPoint { sketch, .. }) => tr!(
            "plane-value-sketch-point",
            name = feature_name(state, *sketch)
        ),
    }
}

/// Inhalte der Auswahlfelder und Abstand bzw. Winkel (sonst 0) einer Definition.
fn slots_of(def: &PlaneDef) -> (Vec<SlotValue>, f64) {
    match def {
        PlaneDef::Offset { base, distance } => (vec![SlotValue::Base(base.clone())], *distance),
        PlaneDef::Angle { axis, angle } => (vec![SlotValue::Axis(axis.clone())], *angle),
        PlaneDef::Mid { a, b } => (
            vec![SlotValue::Base(a.clone()), SlotValue::Base(b.clone())],
            0.0,
        ),
        PlaneDef::ThreePoints(p) => (p.iter().cloned().map(SlotValue::Point).collect(), 0.0),
        PlaneDef::Tangent { face, base, angle } => (
            vec![SlotValue::Face(face.clone()), SlotValue::Base(base.clone())],
            *angle,
        ),
    }
}

/// Zeilen des Eigenschaftenfensters einer Konstruktionsebene: Befehl, die Bezüge mit den
/// Bezeichnungen des Fensters und Abstand bzw. Winkel.
pub fn property_rows(state: &AppState, def: &PlaneDef) -> Vec<(String, String)> {
    let command = PlaneCommand::of(def);
    let (slots, value) = slots_of(def);
    let mut rows = vec![(tr!("props-plane-command"), command.label())];
    for (i, v) in slots.iter().enumerate() {
        rows.push((command.slot_label(i), describe(state, v)));
    }
    if let (Some(label), Some((_, unit))) =
        (PlaneDialog::new(command).number_label(), command.number())
    {
        rows.push((label, crate::formula_field::unit_value(value, unit)));
    }
    rows
}

/// Die festen Körper der Ansicht (ohne Netz-Körper), wie sie die Berechnung sieht.
fn solids(state: &AppState) -> Vec<&Solid> {
    (0..state.bodies.len())
        .filter(|j| !state.is_mesh(*j))
        .map(|j| &state.bodies[j].solid)
        .collect()
}

impl PlaneDialog {
    /// Neues, leeres Fenster für `command`; Abstand und Winkel beginnen bei 0 (wie Fusion).
    pub fn new(command: PlaneCommand) -> PlaneDialog {
        let unit = command.number().map_or(ParamUnit::Length, |(_, u)| u);
        PlaneDialog {
            command,
            editing: None,
            slots: vec![None; command.slots().len()],
            active: 0,
            text: stored_text(0.0, None, unit, &Default::default()),
            value: 0.0,
            formula: None,
            error: None,
        }
    }

    /// Fenster „Bearbeiten“ für die Konstruktionsebene `id`, gefüllt mit ihren Werten; `None`
    /// für andere Features.
    pub fn edit(state: &AppState, id: FeatureId) -> Option<PlaneDialog> {
        let f = state.document.feature(id)?;
        let FeatureKind::Plane(pf) = &f.kind else {
            return None;
        };
        let command = PlaneCommand::of(&pf.def);
        let (slots, value) = slots_of(&pf.def);
        let (formula, unit) = match command.number() {
            Some((key, unit)) => (f.formulas().get(&key).cloned(), unit),
            None => (None, ParamUnit::Length),
        };
        let scope = state.document.param_scope();
        Some(PlaneDialog {
            command,
            editing: Some(Editing {
                id,
                name: f.name.clone(),
                version: f.version(),
            }),
            slots: slots.into_iter().map(Some).collect(),
            active: 0,
            text: stored_text(value, formula.as_deref(), unit, &scope),
            value,
            formula,
            error: None,
        })
    }

    /// Der Befehl des Fensters.
    pub fn command(&self) -> PlaneCommand {
        self.command
    }

    /// Die bearbeitete Ebene (`None` beim Anlegen).
    pub fn editing(&self) -> Option<FeatureId> {
        self.editing.as_ref().map(|e| e.id)
    }

    /// Titel des Fensters.
    pub fn title(&self) -> String {
        match &self.editing {
            Some(e) => tr!("edit-title", name = e.name.as_str()),
            None => self.command.label(),
        }
    }

    /// Das aktive Auswahlfeld.
    pub fn active_slot(&self) -> usize {
        self.active
    }

    /// Macht das Feld `i` aktiv (Klick auf das Feld).
    pub fn set_active(&mut self, i: usize) {
        if i < self.slots.len() {
            self.active = i;
        }
    }

    /// Inhalt des Felds `i`.
    pub fn slot(&self, i: usize) -> Option<&SlotValue> {
        self.slots.get(i)?.as_ref()
    }

    /// Füllt das aktive Feld mit `v` und macht das nächste leere Feld aktiv.
    pub fn fill(&mut self, v: SlotValue) {
        self.slots[self.active] = Some(v);
        self.error = None;
        let n = self.slots.len();
        if let Some(next) = (1..n)
            .map(|k| (self.active + k) % n)
            .find(|k| self.slots[*k].is_none())
        {
            self.active = next;
        }
    }

    /// Text des Zahlenfelds.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Setzt den Text des Zahlenfelds (Zahl oder Formel).
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
    }

    /// Bezeichnung des Zahlenfelds („Abstand“, „Winkel“), falls es eines gibt.
    pub fn number_label(&self) -> Option<String> {
        let (key, _) = self.command.number()?;
        let placeholder = FeatureKind::Plane(PlaneFeature {
            def: match self.command {
                PlaneCommand::Offset => PlaneDef::Offset {
                    base: PlaneBase::Standard(StandardPlane::XY),
                    distance: 0.0,
                },
                _ => PlaneDef::Angle {
                    axis: AxisRef::Standard(StdAxis::X),
                    angle: 0.0,
                },
            },
        });
        Some(field_label(&placeholder, key))
    }

    /// Aktuelle Meldung des Fensters in der aktuellen Sprache.
    pub fn error(&self) -> Option<String> {
        self.error.as_ref().map(Msg::text)
    }

    /// Nimmt einen Klick für das aktive Feld; ein unpassender Klick lässt das Feld, wie es ist,
    /// und hinterlässt eine Meldung. `true`, wenn das Feld gefüllt wurde.
    pub fn take_click(&mut self, state: &AppState, click: PlaneClick) -> bool {
        match self.convert(state, click) {
            Ok(v) => {
                self.fill(v);
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    /// Wandelt einen Klick in den Inhalt des aktiven Felds.
    fn convert(&self, state: &AppState, click: PlaneClick) -> Result<SlotValue, Msg> {
        let kind = self.command.slots()[self.active];
        let tol = state.kernel.tolerance;
        let solid_of = |body: usize| -> Result<&Solid, Msg> {
            if state.is_mesh(body) {
                return Err(crate::msg!("plane-mesh"));
            }
            state
                .bodies
                .get(body)
                .map(|b| &b.solid)
                .ok_or_else(|| crate::msg!("state-body-not-found"))
        };
        let wrong = || crate::msg!("plane-wrong-pick", what = kind.prompt());
        match (kind, click) {
            (SlotKind::Base, PlaneClick::Standard(p)) => {
                Ok(SlotValue::Base(PlaneBase::Standard(p)))
            }
            (SlotKind::Base, PlaneClick::View(Pick::Plane(id))) => {
                if self.is_later(state, id) {
                    return Err(crate::msg!("plane-later-plane"));
                }
                Ok(SlotValue::Base(PlaneBase::Plane(id)))
            }
            (SlotKind::Base, PlaneClick::View(Pick::Face(f))) => {
                let solid = solid_of(f.body)?;
                if plane_of_face(solid, f.face).is_none() {
                    return Err(crate::msg!("plane-face-not-flat"));
                }
                FaceRef::from_face(solid, f.face, &tol)
                    .map(|r| SlotValue::Base(PlaneBase::Face(r)))
                    .map_err(|e| Msg::Raw(e.to_string()))
            }
            (SlotKind::Face, PlaneClick::View(Pick::Face(f))) => {
                let solid = solid_of(f.body)?;
                FaceRef::from_face(solid, f.face, &tol)
                    .map(SlotValue::Face)
                    .map_err(|e| Msg::Raw(e.to_string()))
            }
            (SlotKind::Axis, PlaneClick::View(Pick::Edge(e))) => {
                let solid = solid_of(e.body)?;
                EdgeRef::from_edge(solid, e.edge, &tol)
                    .map(|r| SlotValue::Axis(AxisRef::Edge(r)))
                    .map_err(|e| Msg::Raw(e.to_string()))
            }
            (SlotKind::Point, PlaneClick::View(Pick::Vertex { body, edge, end })) => {
                let solid = solid_of(body)?;
                EdgeRef::from_edge(solid, edge, &tol)
                    .map(|r| SlotValue::Point(PointRef::EdgeEnd { edge: r, end }))
                    .map_err(|e| Msg::Raw(e.to_string()))
            }
            _ => Err(wrong()),
        }
    }

    /// Liegt die Ebene `id` beim Bearbeiten nicht vor der bearbeiteten (sie selbst oder eine
    /// spätere)? Eine Ebene kann nur auf früheren aufbauen.
    fn is_later(&self, state: &AppState, id: FeatureId) -> bool {
        let Some(e) = &self.editing else {
            return false;
        };
        let doc = &state.document;
        match (doc.index_of(id), doc.index_of(e.id)) {
            (Some(a), Some(b)) => a >= b,
            _ => true,
        }
    }

    /// Definition und Formeln aus den Feldern; ein leeres Feld oder eine ungültige Zahl ergibt
    /// eine Meldung.
    pub fn definition(&self, state: &AppState) -> Result<(PlaneDef, Formulas), Msg> {
        let scope = state.document.param_scope();
        let mut formulas = Formulas::new();
        let number = match self.command.number() {
            Some((key, unit)) => {
                let label = self.number_label().unwrap_or_default();
                let (v, formula) = resolve_kept(
                    &self.text,
                    unit,
                    &scope,
                    self.value,
                    self.formula.as_deref(),
                )
                .map_err(|e| crate::msg!("field-error", label = label.as_str(), error = e))?;
                if v.abs() > MAX_COORD_MM {
                    return Err(crate::msg!(
                        "edit-too-big",
                        label = label.as_str(),
                        max = MsgArg::LenAs(MAX_COORD_MM, format!("{MAX_COORD_MM:e}")),
                        unit = MsgArg::Unit
                    ));
                }
                if let Some(f) = formula {
                    formulas.insert(key, f);
                }
                v
            }
            None => 0.0,
        };
        if let Some(i) = self.slots.iter().position(Option::is_none) {
            return Err(crate::msg!(
                "plane-slot-empty",
                slot = self.command.slot_label(i)
            ));
        }
        let s: Vec<SlotValue> = self.slots.iter().flatten().cloned().collect();
        let base = |v: &SlotValue| match v {
            SlotValue::Base(b) => Some(b.clone()),
            _ => None,
        };
        let point = |v: &SlotValue| match v {
            SlotValue::Point(p) => Some(p.clone()),
            _ => None,
        };
        let def = match (self.command, s.as_slice()) {
            (PlaneCommand::Offset, [a]) => base(a).map(|base| PlaneDef::Offset {
                base,
                distance: number,
            }),
            (PlaneCommand::Angle, [SlotValue::Axis(axis)]) => Some(PlaneDef::Angle {
                axis: axis.clone(),
                angle: number,
            }),
            (PlaneCommand::Mid, [a, b]) => {
                base(a).zip(base(b)).map(|(a, b)| PlaneDef::Mid { a, b })
            }
            (PlaneCommand::ThreePoints, [a, b, c]) => match (point(a), point(b), point(c)) {
                (Some(a), Some(b), Some(c)) => Some(PlaneDef::ThreePoints([a, b, c])),
                _ => None,
            },
            (PlaneCommand::Tangent, [SlotValue::Face(face), b]) => {
                base(b).map(|base| PlaneDef::Tangent {
                    face: face.clone(),
                    base,
                    angle: number,
                })
            }
            _ => None,
        };
        def.map(|d| (d, formulas))
            .ok_or_else(|| crate::msg!("plane-slot-empty", slot = self.command.slot_label(0)))
    }

    /// Lage der Ebene aus den Feldern, rein berechnet (das Dokument bleibt unverändert).
    pub fn preview(&self, state: &AppState) -> Result<(PlaneDef, Frame), Msg> {
        let (def, _) = self.definition(state)?;
        let pf = PlaneFeature { def };
        let frame = compute_plane(
            &state.document,
            &pf,
            &solids(state),
            &state.kernel.tolerance,
        )
        .map_err(|e| Msg::Raw(e.to_string()))?;
        Ok((pf.def, frame))
    }

    /// „OK“: legt die Ebene an bzw. ändert sie (ein Schritt). Lässt sie sich nicht berechnen,
    /// bleibt das Dokument unverändert und die Meldung steht im Fenster.
    pub fn ok(&mut self, state: &mut AppState) -> Result<FeatureId, String> {
        let result = self.ok_msg(state);
        if let Err(e) = &result {
            state.message = Some(e.text());
            self.error = Some(e.clone());
        }
        result.map_err(|m| m.text())
    }

    fn ok_msg(&self, state: &mut AppState) -> Result<FeatureId, Msg> {
        let (def, formulas) = self.definition(state)?;
        self.preview(state)?;
        match &self.editing {
            None => state.add_plane_with(def, formulas).map_err(Msg::Raw),
            Some(e) => {
                let current = state.document.feature(e.id).map(|f| f.version());
                if current != Some(e.version) {
                    return Err(crate::feature_dialog::stale_message());
                }
                state
                    .update_plane(e.id, def, formulas)
                    .map(|()| e.id)
                    .map_err(Msg::Raw)
            }
        }
    }

    /// Beendet die Auswahl (Fenster geschlossen): Ansicht und Browser wählen wieder normal.
    pub fn finish(state: &mut AppState) {
        state.plane_pick = None;
        state.set_hover(None);
    }

    /// Holt einen Klick ab und gibt Filter und Vorschau an Ansicht und Browser; liefert die
    /// Vorschau (oder warum es keine gibt).
    pub fn sync(&mut self, state: &mut AppState) -> Result<(PlaneDef, Frame), Msg> {
        if let Some(click) = state.plane_pick.as_mut().and_then(|p| p.clicked.take()) {
            self.take_click(state, click);
        }
        let result = self.preview(state);
        let preview = result
            .as_ref()
            .ok()
            .and_then(|(def, frame)| state.plane_rect_for(def, *frame));
        state.plane_pick = Some(PlanePick {
            filter: self.command.slots()[self.active].filter(),
            clicked: None,
            preview,
        });
        result
    }

    /// Zeigt das Fenster; `true`, solange es offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let preview = self.sync(state);
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        let scope = state.document.param_scope();
        let mut field_error = None;
        let id = egui::Id::new(("plane_dialog", self.editing()));
        Dialog::new(self.title(), DialogSize::Normal)
            .id(id)
            .open(&mut open)
            .show(ctx, |ui| {
                if self.editing.is_some() {
                    ui.weak(self.command.label());
                    ui.add_space(4.0);
                }
                egui::Grid::new(("plane_slots", self.editing()))
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (i, kind) in self.command.slots().iter().enumerate() {
                            ui.label(self.command.slot_label(i));
                            ui.horizontal(|ui| {
                                let text = self.slots[i]
                                    .as_ref()
                                    .map_or_else(|| kind.prompt(), |v| describe(state, v));
                                if ui.selectable_label(self.active == i, text).clicked() {
                                    self.active = i;
                                }
                                self.extra_choices(ui, i, *kind);
                            });
                            ui.end_row();
                        }
                        if let (Some(label), Some((_, unit))) =
                            (self.number_label(), self.command.number())
                        {
                            ui.label(label.as_str());
                            let out = FormulaField::new(
                                ("plane-field", self.editing()),
                                &mut self.text,
                                unit,
                            )
                            .error_below(false)
                            .label(label.as_str())
                            .show(ui, &scope);
                            if let Some(e) = out.error {
                                field_error =
                                    Some(tr!("field-error", label = label.as_str(), error = e));
                            }
                            ui.end_row();
                        }
                    });
                if self.command.number().is_some()
                    && let Some(hint) = names_hint(&scope)
                {
                    ui.weak(hint);
                }
                let dark = ui.visuals().dark_mode;
                // Warum die Vorschau fehlt (nur wenn alle Felder gefüllt sind).
                let reason = match (&preview, self.slots.iter().all(Option::is_some)) {
                    (Err(e), true) => Some(e.text()),
                    _ => None,
                };
                for e in field_error.iter().chain(&self.error()).chain(&reason) {
                    ui.add_space(4.0);
                    ui.colored_label(error_color(dark), e);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let ready = preview.is_ok() && field_error.is_none();
                    if ui
                        .add_enabled(ready, egui::Button::new(tr!("common-ok")))
                        .clicked()
                    {
                        ok = true;
                    }
                    if ui.button(tr!("common-cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        // Esc bricht ab (wie „Abbrechen“), solange kein Feld tippt.
        if !crate::viewport::typing(ctx) && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        let keep = if ok {
            self.ok(state).is_err()
        } else {
            open && !cancel
                && self
                    .editing
                    .as_ref()
                    .is_none_or(|e| state.document.feature(e.id).is_some())
        };
        if !keep {
            PlaneDialog::finish(state);
        }
        keep
    }

    /// Wahl ohne Klick in die Ansicht: Grundebenen, Ursprungsachsen, Ursprung.
    fn extra_choices(&mut self, ui: &mut egui::Ui, i: usize, kind: SlotKind) {
        let mut chosen = None;
        match kind {
            SlotKind::Base => {
                ui.menu_button(tr!("plane-origin-planes"), |ui| {
                    for p in [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ] {
                        if ui.button(standard_label(p)).clicked() {
                            chosen = Some(SlotValue::Base(PlaneBase::Standard(p)));
                            ui.close();
                        }
                    }
                });
            }
            SlotKind::Axis => {
                ui.menu_button(tr!("plane-origin-axes"), |ui| {
                    for a in [StdAxis::X, StdAxis::Y, StdAxis::Z] {
                        if ui.button(axis_label(a)).clicked() {
                            chosen = Some(SlotValue::Axis(AxisRef::Standard(a)));
                            ui.close();
                        }
                    }
                });
            }
            SlotKind::Point => {
                if ui.small_button(tr!("plane-value-origin")).clicked() {
                    chosen = Some(SlotValue::Point(PointRef::Origin));
                }
            }
            SlotKind::Face => {}
        }
        if let Some(v) = chosen {
            self.active = i;
            self.fill(v);
        }
    }
}
