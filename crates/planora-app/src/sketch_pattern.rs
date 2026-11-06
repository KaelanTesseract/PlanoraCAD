//! Rechteckige und Kreisförmige Anordnung (Skizzen 5, Fusion „Rechteckige Anordnung“ und
//! „Kreisförmige Anordnung“). Die Stufen gehen wie bei Verschieben/Kopieren: `Picking` wählt die
//! Vorlage (Klick, Rahmen und Vorauswahl gelten), das Kreismuster wartet danach in `SetBase` auf
//! den Klick auf seine Mitte, `Active` zeigt Felder, Pfeilgriffe und die Vorschau. Ein Klick auf ein
//! Exemplar der Vorschau wählt es ab oder wieder an, nach „Linie wählen“ setzt ein Klick auf eine
//! Linie die Richtung. Eingabe oder OK legt das Muster an bzw. schreibt ein bestehendes neu, als
//! **ein** Rückgängig-Schritt ([`SketchSession::finish_pattern`]); Esc, Rechtsklick und ein
//! anderes Werkzeug lassen nichts zurück. Vorschau und Anlegen rechnen mit derselben reinen Lage
//! ([`Sketch::pattern_preview`] und [`Sketch::add_pattern`] über `pattern_transforms`).

use crate::msg::Msg;
use crate::sketch_cursor::{DrawCursor, attach_point};
use crate::sketch_grips::unwrapped_angle;
use crate::sketch_grips::{GRIP_ARROW_PX, GRIP_HIT_PX};
use crate::sketch_input::FieldKind;
use crate::sketch_kinds::{PREVIEW_STEPS_PER_TURN, ToolKind};
use crate::sketch_state::{Item, SketchSession, Snap, Tool, TransformStage, err};
use egui::{Color32, Pos2, Shape, Stroke, Vec2};
use planora_feature::SketchFeature;
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::pattern::{
    CircularMode, PatternDir, PatternId, PatternKind, PatternPlan, PatternPreview, PatternSlot,
};
use planora_sketch::snap::SnapHit;
use std::borrow::Cow;
use std::collections::HashSet;
use std::f64::consts::TAU;

/// Vorgabe des Abstands: so viel mal die Ausdehnung der Vorlage in der Richtung (Entscheidung 15).
pub const PATTERN_SPACING_FACTOR: f64 = 1.5;
/// Abstand, wenn die Vorlage in keiner Richtung Ausdehnung hat (ein Punkt gibt keine Vorlage).
const PATTERN_FALLBACK_SPACING: f64 = 10.0;
/// Vorgabe der Anzahl je Richtung beim Rechteckmuster (**Annahme**: Fusion 3 × 1).
pub const PATTERN_RECT_COUNTS: [u32; 2] = [3, 1];
/// Vorgabe der Anzahl beim Kreismuster (**Annahme**: Fusion 6, voll).
pub const PATTERN_CIRCULAR_COUNT: u32 = 6;
/// Vorschau aller Lagen: Lage, abgewählt, Linienzüge (siehe
/// [`SketchSession::pattern_preview_all`]).
pub type PatternPreviewAll = Vec<([i32; 2], bool, Vec<Vec<DVec2>>)>;
/// Felder des Rechteckmusters und des Kreismusters (voll bzw. mit Winkel).
const RECT_FIELDS: &[FieldKind] = &[
    FieldKind::PatternCount1,
    FieldKind::PatternSpacing1,
    FieldKind::PatternCount2,
    FieldKind::PatternSpacing2,
];
const CIRCULAR_FULL_FIELDS: &[FieldKind] = &[FieldKind::PatternCount];
const CIRCULAR_ANGLE_FIELDS: &[FieldKind] = &[FieldKind::PatternCount, FieldKind::PatternAngle];
/// Länge und halbe Breite der Pfeilspitze in Bildpunkten.
const HEAD_PX: (f32, f32) = (12.0, 5.0);
/// Strichstärke der Griffe, normal und unter dem Zeiger.
const GRIP_WIDTH: (f32, f32) = (1.5, 2.5);

/// Ein Griff des Musters: der Pfeil einer Richtung (am letzten Exemplar) oder der Winkel des
/// Kreismusters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternGrip {
    Arrow(usize),
    Angle,
}

/// Lage der Griffe auf dem Bildschirm: `origin` ist die Mitte der Vorlage, `tips` die Spitzen der
/// Pfeile je Richtung, `angle_tip` der Griff am Ende des Bogens (Kreismuster).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternGripLayout {
    pub origin: Pos2,
    pub tips: [Option<Pos2>; 2],
    pub angle_tip: Option<Pos2>,
}

pub fn pattern_layout(
    origin: Pos2,
    tips: [Option<Pos2>; 2],
    angle_tip: Option<Pos2>,
) -> PatternGripLayout {
    PatternGripLayout {
        origin,
        tips,
        angle_tip,
    }
}

/// Fuß des Pfeils mit der Spitze `tip`: eine Pfeillänge zurück zur Mitte der Vorlage.
fn arrow_root(l: &PatternGripLayout, tip: Pos2) -> Pos2 {
    let back = tip - l.origin;
    if back.length() < 1.0 {
        return tip;
    }
    let along = back.normalized();
    tip - along * GRIP_ARROW_PX.min(back.length())
}

/// Abstand des Punkts `p` zur Strecke `a`–`b`.
fn to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t).distance(p)
}

/// Der Griff unter `p`: erst der Winkelgriff, dann die Pfeile.
pub fn pattern_hit(l: &PatternGripLayout, p: Pos2) -> Option<PatternGrip> {
    if l.angle_tip.is_some_and(|t| t.distance(p) <= GRIP_HIT_PX) {
        return Some(PatternGrip::Angle);
    }
    (0..2).find_map(|k| {
        let tip = l.tips[k]?;
        (to_segment(p, arrow_root(l, tip), tip) <= GRIP_HIT_PX).then_some(PatternGrip::Arrow(k))
    })
}

/// Zeichnet die Griffe; `hot` hebt einen hervor. Die Farben kommen vom Aufrufer aus dem Thema.
pub fn paint_pattern_grips(
    painter: &egui::Painter,
    l: &PatternGripLayout,
    hot: Option<PatternGrip>,
    color: Color32,
    hot_color: Color32,
) {
    let stroke = |grip: PatternGrip| {
        if hot == Some(grip) {
            Stroke::new(GRIP_WIDTH.1, hot_color)
        } else {
            Stroke::new(GRIP_WIDTH.0, color)
        }
    };
    for k in 0..2 {
        let Some(tip) = l.tips[k] else {
            continue;
        };
        let root = arrow_root(l, tip);
        if root == tip {
            continue;
        }
        let s = stroke(PatternGrip::Arrow(k));
        painter.line_segment([root, tip], s);
        let along = (tip - root).normalized();
        let side = along.rot90() * HEAD_PX.1;
        let base = tip - along * HEAD_PX.0;
        painter.add(Shape::convex_polygon(
            vec![tip, base + side, base - side],
            s.color,
            Stroke::NONE,
        ));
    }
    if let Some(t) = l.angle_tip {
        let s = stroke(PatternGrip::Angle);
        painter.circle_filled(t, 5.0, s.color);
    }
}

/// Laufender Zug an einem Griff: Stelle des Drückens, Wert davor, Teiler (Zahl der Schritte bis
/// zum letzten Exemplar, beim Abstand `n − 1`, bei der Gesamtlänge 1) und Mitte des Kreismusters.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternGripDrag {
    pub grip: PatternGrip,
    pub press: DVec2,
    pub start: f64,
    pub divisor: f64,
    pub center: DVec2,
    /// Das Feld dieses Werts vor dem Zug (samt Formel): Abbrechen stellt es wieder her.
    pub saved: Option<SavedField>,
}

/// Stand eines Felds der Anordnung vor einem Griffzug.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedField {
    pub kind: FieldKind,
    pub text: String,
    pub touched: bool,
    pub locked: Option<f64>,
    pub error: Option<String>,
    /// Die gemerkte Formel des Felds (`None`: keine gemerkt).
    pub formula: Option<(PatternSlot, Option<String>)>,
}

/// Neuer Wert des Griffs für den Zeiger `now`, rein: Der Pfeil folgt der Projektion des Zugs auf
/// die Richtung `dir` (das letzte Exemplar geht mit dem Zeiger, darum geteilt durch die Zahl der
/// Schritte), der Winkelgriff dem überstrichenen Winkel um die Mitte. Ein Zeiger außerhalb der
/// Zahlen lässt den Startwert.
pub fn pattern_motion(d: &PatternGripDrag, dir: DVec2, now: DVec2) -> f64 {
    if !(now.is_finite() && dir.is_finite()) || !(d.divisor.is_finite() && d.divisor > 0.0) {
        return d.start;
    }
    match d.grip {
        PatternGrip::Arrow(_) => d.start + (now - d.press).dot(dir) / d.divisor,
        PatternGrip::Angle => {
            let (from, to) = (d.press - d.center, now - d.center);
            if from.length() > 0.0 && to.length() > 0.0 {
                d.start + from.perp_dot(to).atan2(from.dot(to)) / d.divisor
            } else {
                d.start
            }
        }
    }
}

/// Zustand der Anordnungen ([`crate::sketch_pattern`]).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PatternToolState {
    pub stage: TransformStage,
    /// Der Plan der Vorschau (ohne die getippten Felder, siehe
    /// [`SketchSession::pattern_now`]).
    pub plan: Option<PatternPlan>,
    /// Das bearbeitete Muster (Doppelklick im Auswahlwerkzeug).
    pub editing: Option<PatternId>,
    /// Die Auswahl vor dem Befehl: Abbrechen stellt sie wieder her.
    pub selection_before: Vec<Item>,
    /// „Linie wählen“ läuft: Der nächste Klick auf eine Linie setzt die Richtung
    /// [`direction_slot`](Self::direction_slot).
    pub picking_direction: bool,
    /// Die Richtung (0 oder 1), die „Linie wählen“ setzt.
    pub direction_slot: usize,
    /// Laufender Zug an einem Griff.
    pub drag: Option<PatternGripDrag>,
    /// Formeln der getippten Felder je Musterwert (in Zoll auch „1 in“).
    pub formulas: Vec<(PatternSlot, Option<String>)>,
    /// `message` sagt gerade, dass die Vorschau nicht geht ([`SketchSession::update_pattern_note`]).
    pub blocked_note: bool,
    /// Plan, zu dem die Meldung zuletzt geprüft wurde.
    noted: Option<PatternPlan>,
    /// Die Mitte des Kreismusters, die erst das Anlegen als Punkt setzt (Klick neben jeden Punkt:
    /// Raster, Kurve, Ursprung oder frei), mit ihrer Anbindung. Bis dahin rechnen Vorschau und
    /// Griffe in einer Kopie der Skizze mit diesem Punkt ([`SketchSession::pattern_sketch`]).
    pub(crate) new_center: Option<SnapHit>,
    /// Beim Winkelgriff der bisher überstrichene Winkel um die Mitte, ungekappt: So läuft der Zug
    /// über ±180° hinweg weiter, auch wenn der Wert schon an ±360° anliegt.
    grip_turned: f64,
}

impl PatternToolState {
    /// Merkt sich die Formel des getippten Felds `slot` (`None`: eine reine Zahl).
    pub(crate) fn typed_formula(&mut self, slot: PatternSlot, formula: Option<String>) {
        self.formulas.retain(|(s, _)| *s != slot);
        self.formulas.push((slot, formula));
    }

    /// Nach Rückgängig/Wiederholen gehören Plan und Wahl zum alten Stand: Das Werkzeug wählt neu.
    pub(crate) fn reset_after_jump(&mut self) {
        *self = PatternToolState::default();
    }
}

/// Das Exemplar (Muster und Lage), zu dem eine gewählte Kurve oder ein gewählter Punkt gehört.
pub(crate) fn instance_of_item(sk: &Sketch, item: Item) -> Option<(PatternId, [i32; 2])> {
    match item {
        Item::Curve(c) => match sk.pattern_of_curve(c)? {
            (id, Some(at)) => Some((id, at)),
            _ => None,
        },
        Item::Point(p) => sk.patterns().find_map(|(id, d)| {
            d.instances
                .iter()
                .find(|i| i.points.contains(&p))
                .map(|i| (id, i.at))
        }),
        Item::Constraint(_) => None,
    }
}

/// Einheitsvektor der Richtung `k` (eine Linie zum Zeitpunkt der Rechnung; „senkrecht“ ist die
/// erste Richtung um 90° gedreht, wie `pattern_lattice`).
fn dir_unit(sk: &Sketch, dirs: [PatternDir; 2], k: usize) -> Option<DVec2> {
    match (k, dirs[k.min(1)]) {
        (1, PatternDir::Perpendicular) => Some(axis_unit(sk, dirs[0])?.perp()),
        (_, dir) => axis_unit(sk, dir),
    }
}

/// Einheitsvektor einer Achse oder Linie; „senkrecht“ allein hat keinen.
fn axis_unit(sk: &Sketch, dir: PatternDir) -> Option<DVec2> {
    match dir {
        PatternDir::SketchX => Some(DVec2::X),
        PatternDir::SketchY => Some(DVec2::Y),
        PatternDir::Line(l) => match sk.curve(l)?.kind {
            CurveKind::Line { a, b } => {
                (sk.point_xy(b).ok()? - sk.point_xy(a).ok()?).try_normalize()
            }
            _ => None,
        },
        PatternDir::Perpendicular => None,
    }
}

/// Kleinste und größte Lage der Linienzüge der Kurven.
fn bounds(sk: &Sketch, curves: &[CurveId]) -> Option<(DVec2, DVec2)> {
    let mut out: Option<(DVec2, DVec2)> = None;
    for &c in curves {
        for q in sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN)? {
            out = Some(match out {
                None => (q, q),
                Some((lo, hi)) => (lo.min(q), hi.max(q)),
            });
        }
    }
    out
}

/// Vorgabe des Abstands einer Richtung: 1,5 × Ausdehnung `own`, sonst der anderen, sonst fest.
fn default_spacing(own: f64, other: f64, tol: f64) -> f64 {
    let size = if own > tol {
        own
    } else if other > tol {
        other
    } else {
        PATTERN_FALLBACK_SPACING / PATTERN_SPACING_FACTOR
    };
    size * PATTERN_SPACING_FACTOR
}

/// Das Rechteckmuster der Vorgabe: Richtungen x und y, 3 × 1, Abstand 1,5 × Ausdehnung.
fn default_rect_plan(sk: &Sketch, sources: Vec<CurveId>) -> Option<PatternPlan> {
    let (lo, hi) = bounds(sk, &sources)?;
    let size = hi - lo;
    let tol = sk.tolerance().linear;
    Some(PatternPlan {
        kind: PatternKind::Rect {
            // Wie Fusion: Die zweite Richtung steht senkrecht zur ersten (hier zunächst y).
            dirs: [PatternDir::SketchX, PatternDir::Perpendicular],
            extent: [false; 2],
            symmetric: [false; 2],
        },
        sources,
        counts: PATTERN_RECT_COUNTS,
        values: [
            default_spacing(size.x, size.y, tol),
            default_spacing(size.y, size.x, tol),
        ],
        suppressed: Vec::new(),
    })
}

/// Der Wert des Plans im Feld `slot`.
fn slot_value(plan: &PatternPlan, slot: PatternSlot) -> f64 {
    match slot {
        PatternSlot::Count1 | PatternSlot::Count => f64::from(plan.counts[0]),
        PatternSlot::Count2 => f64::from(plan.counts[1]),
        PatternSlot::Spacing1 | PatternSlot::Angle => plan.values[0],
        PatternSlot::Spacing2 => plan.values[1],
    }
}

/// Griffpunkte in der Skizze: Mitte der Vorlage, je Richtung das letzte Exemplar und die Richtung
/// nach außen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternHandles {
    pub origin: DVec2,
    pub arrows: [Option<(DVec2, DVec2)>; 2],
}

impl SketchSession {
    /// Läuft eine Anordnung?
    pub fn runs_pattern(&self) -> bool {
        self.tool == Tool::Pattern
    }

    /// Wird für die Anordnung gerade gewählt? Dann gilt der Rahmen wie im Auswahlwerkzeug.
    pub fn pattern_picking(&self) -> bool {
        self.runs_pattern() && self.pattern.stage == TransformStage::Picking
    }

    /// Die gewählten, lebenden Kurven: die Vorlage.
    fn picked_sources(&self, sk: &Sketch) -> Vec<CurveId> {
        self.selection
            .iter()
            .filter_map(|i| match *i {
                Item::Curve(c) if sk.curve(c).is_some() => Some(c),
                _ => None,
            })
            .collect()
    }

    /// Mit gewählten Kurven geht es weiter: Rechteckmuster gleich zu Feldern und Griffen,
    /// Kreismuster zum Klick auf die Mitte. Ohne bleibt die Stufe beim Wählen. Ein Exemplar kann
    /// keine Vorlage sein (Meldung).
    pub fn begin_pattern(&mut self, sk: &Sketch) {
        let sources = self.picked_sources(sk);
        self.pattern.plan = None;
        self.pattern.stage = TransformStage::Picking;
        if sources.is_empty() {
            return;
        }
        if sources
            .iter()
            .any(|&c| matches!(sk.pattern_of_curve(c), Some((_, Some(_)))))
        {
            self.message = Some(crate::msg!("sketch-pattern-instance-source"));
            return;
        }
        match self.kinds.pattern {
            ToolKind::PatternCircular => self.pattern.stage = TransformStage::SetBase,
            _ => {
                let Some(plan) = default_rect_plan(sk, sources) else {
                    return;
                };
                self.pattern.plan = Some(plan);
                self.pattern.stage = TransformStage::Active;
            }
        }
    }

    /// Eingabe beim Wählen: mit Auswahl weiter, sonst die Meldung, dass nichts gewählt ist.
    pub(crate) fn leave_pattern_picking(&mut self, sk: &Sketch) {
        self.begin_pattern(sk);
        if self.pattern.stage == TransformStage::Picking && self.message.is_none() {
            self.message = Some(crate::msg!("sketch-nothing-selected"));
        }
    }

    /// Öffnet das Werkzeug mit dem bestehenden Muster `id` in der aktiven Stufe (Doppelklick auf ein
    /// Exemplar oder eine Vorlage). `false`, wenn es das Muster nicht gibt.
    pub fn begin_pattern_edit(&mut self, sk: &mut Sketch, id: PatternId) -> bool {
        let Some(mut plan) = sk.pattern_plan(id) else {
            return false;
        };
        // „Voll“ speichert keinen Winkel; wechselt die Art, beginnt der Winkel wie beim Anlegen
        // bei 360°.
        if let PatternKind::Circular {
            mode: CircularMode::Full,
            ..
        } = plan.kind
        {
            plan.values[0] = TAU;
        }
        let before = std::mem::take(&mut self.selection);
        self.drop_curve_picks();
        self.kinds.pattern = match plan.kind {
            PatternKind::Rect { .. } => ToolKind::PatternRect,
            PatternKind::Circular { .. } => ToolKind::PatternCircular,
        };
        self.set_tool(sk, Tool::Pattern);
        self.message = None;
        self.pattern.plan = Some(plan);
        self.pattern.editing = Some(id);
        self.pattern.stage = TransformStage::Active;
        self.pattern.selection_before = before;
        true
    }

    /// Doppelklick auf die Kurve `c` im Auswahlwerkzeug: Gehört sie zu einem Muster, öffnet das
    /// Werkzeug es zum Bearbeiten (`true`).
    pub fn begin_pattern_edit_at(&mut self, sk: &mut Sketch, c: CurveId) -> bool {
        self.begin_pattern_edit_item(sk, Item::Curve(c))
    }

    /// Wie [`begin_pattern_edit_at`](Self::begin_pattern_edit_at) für Kurve oder Punkt (ein
    /// Doppelklick nahe einer Ecke fängt den Punkt): Punkte eines Exemplars oder der Vorlage
    /// öffnen ihr Muster.
    pub fn begin_pattern_edit_item(&mut self, sk: &mut Sketch, item: Item) -> bool {
        if self.tool != Tool::Select {
            return false;
        }
        let id = match item {
            Item::Curve(c) => sk.pattern_of_curve(c).map(|(id, _)| id),
            Item::Point(p) => instance_of_item(sk, item).map(|(id, _)| id).or_else(|| {
                sk.patterns()
                    .find(|(_, d)| d.source_points.contains(&p))
                    .map(|(id, _)| id)
            }),
            Item::Constraint(_) => None,
        };
        id.is_some_and(|id| self.begin_pattern_edit(sk, id))
    }

    /// Klick der Anordnung: Beim Wählen schaltet er Kurve oder Punkt unter dem Zeiger, beim
    /// Kreismuster setzt er danach die Mitte (ein Punkt); aktiv setzt er nach „Linie wählen“ die
    /// Richtung, sonst wählt er das Exemplar der Vorschau unter dem Zeiger ab oder wieder an.
    pub fn click_pattern(&mut self, sk: &Sketch, cur: &DrawCursor) {
        let radius = self.pick_radius;
        match self.pattern.stage {
            TransformStage::Picking => {
                let item = match self.snap(sk, cur.raw, radius) {
                    Snap::Point(p) => Item::Point(p),
                    Snap::Curve(c) => Item::Curve(c),
                    _ => return,
                };
                self.drop_curve_picks();
                match self.selection.iter().position(|&s| s == item) {
                    Some(k) => {
                        self.selection.remove(k);
                    }
                    None => self.selection.push(item),
                }
            }
            TransformStage::SetBase => {
                // Wie ein gezeichneter Punkt: auf einem Punkt bindet die Mitte an ihn, sonst
                // entsteht beim Anlegen ein neuer an der gefangenen Stelle (Raster, Strg).
                let tol = sk.tolerance().linear;
                let (center, new_center) = match cur.point(tol) {
                    Some(p) => (p, None),
                    None => {
                        let hit = cur.remember(tol);
                        let mut staged = sk.clone();
                        match attach_point(&mut staged, None, &DrawCursor::remembered(hit, sk)) {
                            Ok(p) => (p, Some(hit)),
                            Err(e) => {
                                self.message = Some(e);
                                return;
                            }
                        }
                    }
                };
                if new_center.is_none() && instance_of_item(sk, Item::Point(center)).is_some() {
                    self.message = Some(crate::msg!("sketch-pattern-instance-source"));
                    return;
                }
                let sources = self.picked_sources(sk);
                self.message = None;
                self.pattern.new_center = new_center;
                self.pattern.plan = Some(PatternPlan {
                    kind: PatternKind::Circular {
                        center,
                        mode: CircularMode::Full,
                    },
                    sources,
                    counts: [PATTERN_CIRCULAR_COUNT, 1],
                    values: [std::f64::consts::TAU, 0.0],
                    suppressed: Vec::new(),
                });
                self.pattern.stage = TransformStage::Active;
            }
            TransformStage::Active if self.pattern.picking_direction => {
                let line = match self.snap(sk, cur.raw, radius) {
                    Snap::Curve(c) => Some(c).filter(|&c| {
                        matches!(sk.curve(c).map(|d| &d.kind), Some(CurveKind::Line { .. }))
                    }),
                    _ => None,
                };
                match line {
                    Some(c) => {
                        self.set_pattern_direction(
                            self.pattern.direction_slot,
                            PatternDir::Line(c),
                        );
                        self.pattern.picking_direction = false;
                        self.message = None;
                    }
                    None => self.message = Some(crate::msg!("sketch-pattern-direction-line")),
                }
            }
            TransformStage::Active => {
                let Some(now) = self.pattern_now() else {
                    return;
                };
                let staged = self.pattern_sketch(sk);
                let Some(at) = staged.pattern_instance_at(&now, cur.raw, radius) else {
                    return;
                };
                if let Some(plan) = self.pattern.plan.as_mut() {
                    match plan.suppressed.iter().position(|x| *x == at) {
                        Some(k) => {
                            plan.suppressed.remove(k);
                        }
                        None => plan.suppressed.push(at),
                    }
                }
            }
        }
    }

    /// Setzt die Richtung `slot` des Rechteckmusters. Bekommt die erste Richtung eine Linie und
    /// war die zweite keine eigene Linie, steht die zweite senkrecht zur ersten (wie Fusion) und
    /// folgt ihr, auch wenn die Linie sich später dreht.
    pub fn set_pattern_direction(&mut self, slot: usize, dir: PatternDir) {
        let Some(plan) = self.pattern.plan.as_mut() else {
            return;
        };
        let PatternKind::Rect { dirs, .. } = &mut plan.kind else {
            return;
        };
        if slot > 1 {
            return;
        }
        if slot == 0 && dir == PatternDir::Perpendicular {
            return;
        }
        dirs[slot] = dir;
        if slot == 0 && !matches!(dirs[1], PatternDir::Line(_)) {
            dirs[1] = PatternDir::Perpendicular;
        }
    }

    /// „Linie wählen“ für die Richtung `slot`: Der nächste Klick auf eine Linie setzt sie.
    pub fn pick_pattern_direction(&mut self, slot: usize) {
        if self.pattern.stage == TransformStage::Active && slot < 2 {
            self.pattern.picking_direction = true;
            self.pattern.direction_slot = slot;
        }
    }

    /// Der Plan dieses Bilds: getippte Felder gehen vor dem Plan.
    pub fn pattern_now(&self) -> Option<PatternPlan> {
        let mut plan = self.pattern.plan.clone()?;
        if self.tool != Tool::Pattern {
            return None;
        }
        for f in &self.input.fields {
            if let (Some(slot), Some(v)) = (f.kind.pattern_slot(), f.locked) {
                plan = plan.with_value(slot, v);
            }
        }
        Some(plan)
    }

    /// Wert eines ungetippten Felds: der Wert des Plans.
    pub(crate) fn pattern_slot_value(&self, slot: PatternSlot) -> Option<f64> {
        Some(slot_value(self.pattern.plan.as_ref()?, slot))
    }

    /// Felder der aktiven Stufe: Rechteck Anzahl und Abstand je Richtung, Kreis die Anzahl und,
    /// außer bei „Voll“, den Winkel.
    pub(crate) fn pattern_fields(&self) -> &'static [FieldKind] {
        if self.pattern.stage != TransformStage::Active {
            return &[];
        }
        match self.pattern.plan.as_ref().map(|p| p.kind) {
            Some(PatternKind::Rect { .. }) => RECT_FIELDS,
            Some(PatternKind::Circular {
                mode: CircularMode::Full,
                ..
            }) => CIRCULAR_FULL_FIELDS,
            Some(PatternKind::Circular { .. }) => CIRCULAR_ANGLE_FIELDS,
            None => &[],
        }
    }

    /// Hinweis der laufenden Anordnung, wenn er vom Hinweis der Art abweicht.
    pub(crate) fn pattern_hint(&self) -> Option<String> {
        if !self.runs_pattern() {
            return None;
        }
        if self.pattern.picking_direction {
            return Some(planora_i18n::tr!("sketch-pattern-pick-direction-hint"));
        }
        (self.pattern.stage == TransformStage::SetBase)
            .then(|| planora_i18n::tr!("sketch-pattern-pick-center-hint"))
    }

    /// Linienzüge der Exemplare, die Eingabe anlegen würde (ohne abgewählte), aus derselben Rechnung
    /// wie das Anlegen ([`Sketch::pattern_preview`]). `None`, solange nichts zu zeigen ist oder das
    /// Muster so nicht geht.
    pub fn pattern_preview(&self, sk: &Sketch) -> Option<PatternPreview> {
        if self.pattern.stage != TransformStage::Active {
            return None;
        }
        self.pattern_sketch(sk)
            .pattern_preview(&self.pattern_now()?, PREVIEW_STEPS_PER_TURN)
            .ok()
    }

    /// Vorschau aller Lagen, abgewählte mit `true` (die Ansicht zeichnet sie gestrichelt).
    pub fn pattern_preview_all(&self, sk: &Sketch) -> Option<PatternPreviewAll> {
        if self.pattern.stage != TransformStage::Active {
            return None;
        }
        let now = self.pattern_now()?;
        let mut all = now.clone();
        all.suppressed.clear();
        let lines = self
            .pattern_sketch(sk)
            .pattern_preview(&all, PREVIEW_STEPS_PER_TURN)
            .ok()?;
        Some(
            lines
                .into_iter()
                .map(|(at, l)| (at, now.suppressed.contains(&at), l))
                .collect(),
        )
    }

    /// Kurven und Punkte, die die Ansicht nicht zeichnet: beim Bearbeiten die Exemplare des
    /// Musters (an ihrer Stelle steht die Vorschau).
    pub fn pattern_hidden(&self, sk: &Sketch) -> (HashSet<CurveId>, HashSet<PointId>) {
        let Some(d) = self
            .pattern
            .editing
            .filter(|_| self.runs_pattern())
            .and_then(|id| sk.pattern(id))
        else {
            return Default::default();
        };
        (
            d.instances
                .iter()
                .flat_map(|i| i.curves.iter().copied())
                .collect(),
            d.instances
                .iter()
                .flat_map(|i| i.points.iter().copied())
                .collect(),
        )
    }

    /// Geht das Muster nicht (zu viele Exemplare, Abstand null, Richtung fehlt …), sagt `message`
    /// das schon, solange die Vorschau fehlt; geht es wieder, verschwindet die Meldung. Geprüft wird
    /// nur, wenn sich der Plan geändert hat.
    pub fn update_pattern_note(&mut self, sk: &Sketch) {
        if !self.runs_pattern() || self.pattern.stage != TransformStage::Active {
            return;
        }
        let now = self.pattern_now();
        if now == self.pattern.noted {
            return;
        }
        let staged = self.pattern_sketch(sk);
        let problem = now
            .as_ref()
            .and_then(|p| staged.pattern_preview(p, PREVIEW_STEPS_PER_TURN).err());
        self.pattern.noted = now;
        match problem {
            None => {
                if std::mem::take(&mut self.pattern.blocked_note) {
                    self.message = None;
                }
            }
            Some(e) => {
                self.message = Some(err(e));
                self.pattern.blocked_note = true;
            }
        }
    }

    /// Die Skizze, in der der Plan gilt: mit der neuen Mitte des Kreismusters, die erst das
    /// Anlegen setzt (dann eine Kopie), sonst `sk` selbst.
    pub(crate) fn pattern_sketch<'a>(&self, sk: &'a Sketch) -> Cow<'a, Sketch> {
        let Some(hit) = self.pattern.new_center else {
            return Cow::Borrowed(sk);
        };
        let mut staged = sk.clone();
        // Ging das nicht, hätte der Klick die Mitte nicht gesetzt.
        let r = attach_point(&mut staged, None, &DrawCursor::remembered(hit, sk));
        debug_assert!(r.is_ok(), "{r:?}");
        Cow::Owned(staged)
    }

    /// Lage der Mitte des Kreismusters (auch die neue, noch nicht angelegte).
    fn pattern_center_xy(&self, sk: &Sketch, center: PointId) -> Option<DVec2> {
        match self.pattern.new_center {
            Some(hit) => Some(hit.at),
            None => sk.point_xy(center).ok(),
        }
    }

    /// Winkelgriff des Kreismusters in der Skizze: Mitte und Griffpunkt am Ende des Bogens (die
    /// Mitte der Vorlage um den Winkel gedreht, symmetrisch um den halben). Bei „Voll“ und ohne
    /// Abstand der Vorlage zur Mitte keiner.
    pub(crate) fn pattern_angle_handle(&self, sk: &Sketch) -> Option<(DVec2, DVec2)> {
        if self.pattern.stage != TransformStage::Active {
            return None;
        }
        let plan = self.pattern_now()?;
        let PatternKind::Circular { center, mode } = plan.kind else {
            return None;
        };
        let theta = match mode {
            CircularMode::Full => return None,
            CircularMode::Angle => plan.values[0],
            CircularMode::Symmetric => 0.5 * plan.values[0],
        };
        let c = self.pattern_center_xy(sk, center)?;
        let (lo, hi) = bounds(sk, &plan.sources)?;
        let arm = (lo + hi) * 0.5 - c;
        if arm.length() <= sk.tolerance().linear || !theta.is_finite() {
            return None;
        }
        let (sin, cos) = theta.sin_cos();
        let tip = c + DVec2::new(arm.x * cos - arm.y * sin, arm.x * sin + arm.y * cos);
        tip.is_finite().then_some((c, tip))
    }

    /// Griffpunkte in der Skizze (nur Rechteckmuster): die Mitte der Vorlage und je Richtung das
    /// letzte Exemplar mit der Richtung nach außen.
    pub fn pattern_handles(&self, sk: &Sketch) -> Option<PatternHandles> {
        if self.pattern.stage != TransformStage::Active || self.pattern.picking_direction {
            return None;
        }
        let plan = self.pattern_now()?;
        let PatternKind::Rect { dirs, extent, .. } = plan.kind else {
            return None;
        };
        let (lo, hi) = bounds(sk, &plan.sources)?;
        let origin = (lo + hi) * 0.5;
        let mut arrows = [None; 2];
        for k in 0..2 {
            let Some(unit) = dir_unit(sk, dirs, k) else {
                continue;
            };
            let n = plan.counts[k].max(1);
            let v = plan.values[k];
            let step = if extent[k] {
                if n > 1 { v / f64::from(n - 1) } else { 0.0 }
            } else {
                v
            };
            let last = origin + unit * step * f64::from(n - 1);
            let out = if v < 0.0 { -unit } else { unit };
            if last.is_finite() {
                arrows[k] = Some((last, out));
            }
        }
        Some(PatternHandles { origin, arrows })
    }

    /// Beginnt den Zug am Griff `grip` bei `at` (Skizzeneinheiten). Ein getippter Abstand dieser
    /// Richtung wird dabei zum Wert des Plans; sein Feld folgt danach dem Griff.
    pub fn begin_pattern_grip(&mut self, sk: &Sketch, grip: PatternGrip, at: DVec2) {
        if self.pattern.stage != TransformStage::Active || !at.is_finite() {
            return;
        }
        let Some(now) = self.pattern_now() else {
            return;
        };
        let (slot, field) = match grip {
            PatternGrip::Arrow(0) => (PatternSlot::Spacing1, FieldKind::PatternSpacing1),
            PatternGrip::Arrow(_) => (PatternSlot::Spacing2, FieldKind::PatternSpacing2),
            PatternGrip::Angle => (PatternSlot::Angle, FieldKind::PatternAngle),
        };
        let start = slot_value(&now, slot);
        let saved = self
            .input
            .fields
            .iter()
            .find(|f| f.kind == field)
            .map(|f| SavedField {
                kind: f.kind,
                text: f.text.clone(),
                touched: f.touched,
                locked: f.locked,
                error: f.error.clone(),
                formula: self
                    .pattern
                    .formulas
                    .iter()
                    .find(|(s, _)| *s == slot)
                    .cloned(),
            });
        if let Some(plan) = self.pattern.plan.as_mut() {
            *plan = plan.clone().with_value(slot, start);
        }
        self.unlock_field(field, slot);
        let (divisor, center) = match (grip, now.kind) {
            (PatternGrip::Arrow(k), PatternKind::Rect { extent, .. }) => {
                let n = now.counts[k.min(1)];
                let steps = if extent[k.min(1)] || n <= 1 {
                    1.0
                } else {
                    f64::from(n - 1)
                };
                (steps, DVec2::ZERO)
            }
            (PatternGrip::Angle, PatternKind::Circular { center, mode }) => {
                let Some(c) = self.pattern_center_xy(sk, center) else {
                    return;
                };
                (
                    if mode == CircularMode::Symmetric {
                        0.5
                    } else {
                        1.0
                    },
                    c,
                )
            }
            _ => return,
        };
        self.pattern.grip_turned = 0.0;
        self.pattern.drag = Some(PatternGripDrag {
            grip,
            press: at,
            start,
            divisor,
            center,
            saved,
        });
    }

    /// Gibt das Feld `kind` frei (der Plan hat seinen Wert), samt Formel.
    fn unlock_field(&mut self, kind: FieldKind, slot: PatternSlot) {
        if let Some(f) = self.input.fields.iter_mut().find(|f| f.kind == kind) {
            f.locked = None;
            f.touched = false;
            f.text.clear();
            f.error = None;
        }
        self.pattern.formulas.retain(|(s, _)| *s != slot);
    }

    /// Führt den Griff zum Zeiger `at`: Der Abstand folgt der Projektion des Zugs auf die Richtung.
    pub fn pattern_grip_to(&mut self, sk: &Sketch, at: DVec2) {
        let Some(d) = self.pattern.drag.clone() else {
            return;
        };
        let Some(plan) = self.pattern.plan.as_mut() else {
            return;
        };
        match (d.grip, plan.kind) {
            (PatternGrip::Arrow(k), PatternKind::Rect { dirs, .. }) => {
                let Some(dir) = dir_unit(sk, dirs, k.min(1)) else {
                    return;
                };
                plan.values[k.min(1)] = pattern_motion(&d, dir, at);
            }
            (PatternGrip::Angle, PatternKind::Circular { .. }) => {
                // Der Zug zählt den überstrichenen Winkel über ±180° hinweg weiter
                // (`unwrapped_angle` wie beim Drehgriff); mehr als ein Umlauf geht nicht.
                let raw = (pattern_motion(&d, DVec2::ZERO, at) - d.start) * d.divisor;
                let turned = unwrapped_angle(self.pattern.grip_turned, raw);
                self.pattern.grip_turned = turned;
                plan.values[0] = (d.start + turned / d.divisor).clamp(-TAU, TAU);
            }
            _ => {}
        }
    }

    /// Beendet den Griff; der Wert bleibt im Plan.
    pub fn end_pattern_grip(&mut self) {
        self.pattern.drag = None;
    }

    /// Nimmt nur den laufenden Griff zurück (Rechtsklick oder Esc im Zug); bis alle Tasten los
    /// sind, ist nichts ein Klick.
    pub fn abort_pattern_grip(&mut self) {
        let Some(d) = self.pattern.drag.take() else {
            return;
        };
        if let Some(plan) = self.pattern.plan.as_mut() {
            let slot = match d.grip {
                PatternGrip::Arrow(0) => PatternSlot::Spacing1,
                PatternGrip::Arrow(_) => PatternSlot::Spacing2,
                PatternGrip::Angle => PatternSlot::Angle,
            };
            *plan = plan.clone().with_value(slot, d.start);
        }
        // Das Feld wie vor dem Zug, samt getippter Formel.
        if let Some(saved) = d.saved {
            if let Some(f) = self.input.fields.iter_mut().find(|f| f.kind == saved.kind) {
                f.text = saved.text;
                f.touched = saved.touched;
                f.locked = saved.locked;
                f.error = saved.error;
            }
            if let Some((slot, formula)) = saved.formula {
                self.pattern.typed_formula(slot, formula);
            }
        }
        self.box_aborted = true;
    }

    /// Eingabe oder OK: beim Wählen weiter zu Feldern und Griffen, aktiv anlegen.
    pub fn confirm_pattern(&mut self, sf: &mut SketchFeature) -> Result<(), String> {
        if self.in_gesture() {
            return Ok(());
        }
        match self.pattern.stage {
            TransformStage::Active => self.finish_pattern(sf),
            _ => self.finish_tool(&mut sf.sketch),
        }
    }

    /// Legt das Muster an (oder schreibt das bearbeitete neu) und löst, als ein Schritt; die
    /// getippten Felder geben ihren Musterwerten die Formel (in Zoll auch „1 in“). Danach geht das
    /// Werkzeug zurück zur Auswahl. Was nicht geht, meldet `message`, und alles bleibt, wie es war.
    pub fn finish_pattern(&mut self, sf: &mut SketchFeature) -> Result<(), String> {
        self.sync_input(&sf.sketch);
        if let Some(e) = self.input.fields.iter().find_map(|f| f.error.clone()) {
            self.message = Some(Msg::Raw(e.clone()));
            return Err(e);
        }
        let Some(plan) = self
            .pattern_now()
            .filter(|_| self.pattern.stage == TransformStage::Active)
        else {
            let e = crate::msg!("sketch-nothing-selected");
            let text = e.text();
            self.message = Some(e);
            return Err(text);
        };
        // Getippte Felder: ihre Formel (oder keine) kommt an den Musterwert.
        let typed: Vec<(PatternSlot, Option<String>)> = self
            .input
            .fields
            .iter()
            .filter(|f| f.locked.is_some())
            .filter_map(|f| f.kind.pattern_slot())
            .map(|slot| {
                let formula = self
                    .pattern
                    .formulas
                    .iter()
                    .find(|(s, _)| *s == slot)
                    .and_then(|(_, f)| f.clone());
                (slot, formula)
            })
            .collect();
        let editing = self.pattern.editing;
        let before = editing.and_then(|id| sf.sketch.pattern_plan(id));
        // Beim Bearbeiten: der Entwurf davor, damit ein unverändertes Bestätigen keinen
        // Schrittnamen setzt.
        let untouched = editing.map(|_| (sf.sketch.to_data(), sf.formulas.clone()));
        let circular = matches!(plan.kind, PatternKind::Circular { .. });
        let new_center = self.pattern.new_center;
        let mut plan = plan;
        let id = self.atomic(&mut sf.sketch, |se, sk, prior| {
            // Die neue Mitte entsteht im selben Schritt wie das Muster.
            if let (Some(hit), PatternKind::Circular { mode, .. }) = (new_center, plan.kind) {
                let center = attach_point(sk, None, &DrawCursor::remembered(hit, prior))?;
                plan.kind = PatternKind::Circular { center, mode };
            }
            let id = match editing {
                Some(id) => {
                    let edit = sk.edit_pattern(id, &plan).map_err(err)?;
                    se.solve_checked(sk, prior)?;
                    if edit.dropped_constraints > 0 {
                        se.add_note(crate::msg!(
                            "sketch-pattern-dropped",
                            n = edit.dropped_constraints
                        ));
                    }
                    id
                }
                None => {
                    let id = sk.add_pattern(&plan).map_err(err)?;
                    se.solve_checked(sk, prior)?;
                    id
                }
            };
            se.edit_label = Some(match (editing, circular) {
                (Some(_), _) => crate::msg!("undo-pattern-edit"),
                (None, true) => crate::msg!("undo-pattern-circular"),
                (None, false) => crate::msg!("undo-pattern-rect"),
            });
            Ok(id)
        })?;
        // Formeln: getippt ⇒ die neue (oder keine); sonst bleibt sie, solange der Wert gleich ist.
        let values: Vec<(PatternSlot, planora_sketch::ConstraintId, f64)> = sf
            .sketch
            .pattern(id)
            .map(|d| {
                d.values
                    .iter()
                    .filter_map(|&c| match sf.sketch.constraint(c)?.kind {
                        ConstraintKind::PatternValue(_, slot, v) => Some((slot, c, v)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (slot, c, v) in values {
            match typed.iter().find(|(s, _)| *s == slot) {
                Some((_, Some(f))) => {
                    sf.formulas.insert(c, f.clone());
                }
                Some((_, None)) => {
                    sf.formulas.remove(&c);
                }
                None => {
                    let old = before.as_ref().map(|p| slot_value(p, slot));
                    if old.is_some_and(|o| o.to_bits() != v.to_bits()) {
                        sf.formulas.remove(&c);
                    }
                }
            }
        }
        sf.prune_formulas();
        if untouched
            .is_some_and(|(data, formulas)| data == sf.sketch.to_data() && formulas == sf.formulas)
        {
            self.edit_label = None;
        }
        self.note_formula_problem(sf);
        let note = self.message.take();
        self.pattern.blocked_note = false;
        self.selection.clear();
        self.drop_curve_picks();
        self.set_tool(&mut sf.sketch, Tool::Select);
        self.message = note;
        Ok(())
    }
}

/// Bildschirmrichtung des Pfeils: das Bild der Richtung `out` an der Stelle `at`.
pub fn screen_arrow(
    to_screen: impl Fn(DVec2) -> Option<Pos2>,
    at: DVec2,
    out: DVec2,
    step: f64,
) -> Option<Pos2> {
    let a = to_screen(at)?;
    let b = to_screen(at + out * step)?;
    let d: Vec2 = b - a;
    (d.length() > 0.0).then(|| a + d.normalized() * GRIP_ARROW_PX)
}
