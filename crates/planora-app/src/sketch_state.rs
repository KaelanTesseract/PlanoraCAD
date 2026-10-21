//! Skizzen-Modus ohne Oberfläche: Werkzeuge, Auswahl, Ziehen und Abhängigkeiten.
//!
//! Alle Methoden, die die Skizze ändern, sind atomar: Bei einem Fehler bleiben Skizze und
//! Sitzung (laufendes Werkzeug, Auswahl) unverändert, und `message` erklärt den Fehler.

use crate::formula_field::resolve;
use planora_feature::{FeatureId, ParamScope, ParamUnit, SketchFeature, constraint_unit};
use planora_kernel::{Edge, Id, Solid};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, MAX_SPLINE_POINTS, PointId, Sketch};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};

/// Winkel zur Achse, unter dem eine Linie beim Zeichnen als waagerecht/senkrecht gilt.
const AUTO_AXIS_ANGLE: f64 = 3.0 * std::f64::consts::PI / 180.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Point,
    Line,
    Rectangle,
    Circle,
    Arc,
    Ellipse,
    Spline,
    ProjectEdge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Point(PointId),
    Curve(CurveId),
    Constraint(ConstraintId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    None,
    Point(PointId),
    Curve(CurveId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintAction {
    Coincident,
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Tangent,
    Equal,
    Fixed,
    Midpoint,
    Symmetric,
    PointOnCurve,
    Distance,
    DistancePointLine,
    Radius,
    Diameter,
    Angle,
}

impl ConstraintAction {
    pub fn label(self) -> &'static str {
        use ConstraintAction::*;
        match self {
            Coincident => "Deckungsgleich",
            Horizontal => "Waagerecht",
            Vertical => "Senkrecht",
            Parallel => "Parallel",
            Perpendicular => "Rechtwinklig",
            Tangent => "Tangential",
            Equal => "Gleich",
            Fixed => "Fest",
            Midpoint => "Mittelpunkt",
            Symmetric => "Symmetrisch",
            PointOnCurve => "Punkt auf Kurve",
            Distance => "Abstand",
            DistancePointLine => "Abstand Punkt–Linie",
            Radius => "Radius",
            Diameter => "Durchmesser",
            Angle => "Winkel",
        }
    }

    /// Ist das ein Maß (mit Zahlenwert)?
    pub fn is_dimension(self) -> bool {
        use ConstraintAction::*;
        matches!(
            self,
            Distance | DistancePointLine | Radius | Diameter | Angle
        )
    }

    /// Einheit, die eine Formel für dieses Maß ergeben muss (Winkel in Grad, sonst Länge).
    pub fn unit(self) -> ParamUnit {
        if self == ConstraintAction::Angle {
            ParamUnit::Angle
        } else {
            ParamUnit::Length
        }
    }
}

/// Meldung, wenn man in ein anzeigendes Maß eine Formel tippt.
pub const DRIVEN_NO_FORMULA: &str =
    "Ein anzeigendes Maß bestimmt nichts und kann keine Formel haben.";

/// Zustand einer Skizze mit Formeln, die sich nicht lösen lässt (Plan P12): derselbe Text wie
/// der Fehlerstatus, den die Skizze beim Beenden bekommt. `None` ohne Formeln oder gelöst.
pub fn formula_problem(sf: &SketchFeature, report: Option<&SolveReport>) -> Option<&'static str> {
    if sf.formulas.is_empty() {
        return None;
    }
    match report?.status {
        SolveStatus::Solved => None,
        SolveStatus::Conflicting => {
            Some("Mit den Werten der Formeln widersprechen sich Abhängigkeiten")
        }
        SolveStatus::NotConverged => {
            Some("Mit den Werten der Formeln lässt sich die Skizze nicht lösen")
        }
    }
}

/// Wert für die Skizze (Winkel in Bogenmaß) und Formel aus dem Text eines Maßfelds.
fn dimension_input(
    text: &str,
    unit: ParamUnit,
    scope: &ParamScope,
) -> Result<(f64, Option<String>), String> {
    let (v, formula) = resolve(text, unit, scope)?;
    let v = if unit == ParamUnit::Angle {
        v.to_radians()
    } else {
        v
    };
    Ok((v, formula))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Zustand des geöffneten Skizzen-Modus.
#[derive(Clone, Debug)]
pub struct SketchSession {
    /// Das bearbeitete Skizzen-Feature (die Sitzung arbeitet auf seinem Entwurf).
    pub sketch: FeatureId,
    pub tool: Tool,
    pub selection: Vec<Item>,
    /// Punkte des laufenden Werkzeugs.
    pub pending: Vec<PointId>,
    /// Noch nicht angelegte Eckpunkte (Rechteck, Ellipse).
    pub pending_pos: Vec<DVec2>,
    pub cursor: Option<DVec2>,
    /// Beim Zeichnen von Linien automatisch waagerecht/senkrecht ergänzen.
    pub auto_constrain: bool,
    pub last_report: Option<SolveReport>,
    pub message: Option<String>,
    pub dragging: Option<PointId>,
    /// Zählt die Sprünge durch Rückgängig/Wiederholen ([`SketchSession::after_history_jump`]);
    /// Eingabefelder der Oberfläche, die zu einem älteren Zählerstand gehören, schließen.
    pub history_jumps: u64,
}

/// Was vor einer Änderung gesichert wird, um sie bei einem Fehler zurückzunehmen.
struct Backup {
    sketch: Sketch,
    pending: Vec<PointId>,
    pending_pos: Vec<DVec2>,
    selection: Vec<Item>,
    last_report: Option<SolveReport>,
}

/// Auswahl nach Art getrennt (jeweils in Auswahlreihenfolge).
#[derive(Default)]
struct Sorted {
    points: Vec<PointId>,
    lines: Vec<CurveId>,
    /// Kreise und Bögen
    rounds: Vec<CurveId>,
    /// Ellipsen und Splines
    others: Vec<CurveId>,
    /// Kurven aller Arten in Auswahlreihenfolge.
    curves: Vec<CurveId>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn status_ok(r: &SolveReport) -> bool {
    r.status == SolveStatus::Solved
}

/// Hinweis, wenn die Skizze nach einer erlaubten Änderung weiterhin nicht gelöst ist.
fn unsolved_hint(report: Option<&SolveReport>) -> Option<&'static str> {
    match report.map(|r| r.status) {
        Some(SolveStatus::Conflicting) => Some("Die Skizze enthält weiterhin Widersprüche"),
        Some(SolveStatus::NotConverged) => {
            Some("Die Skizze lässt sich weiterhin nicht vollständig lösen")
        }
        _ => None,
    }
}

impl SketchSession {
    pub fn new(sketch: FeatureId) -> SketchSession {
        SketchSession {
            sketch,
            tool: Tool::Select,
            selection: Vec::new(),
            pending: Vec::new(),
            pending_pos: Vec::new(),
            cursor: None,
            auto_constrain: true,
            last_report: None,
            message: None,
            dragging: None,
            history_jumps: 0,
        }
    }

    /// Wechselt das Werkzeug; ein laufendes Werkzeug wird abgebrochen, ein laufendes Ziehen
    /// wie mit [`end_drag`](Self::end_drag) beendet.
    pub fn set_tool(&mut self, sk: &mut Sketch, tool: Tool) {
        self.end_drag(sk);
        self.tool = tool;
        self.pending.clear();
        self.pending_pos.clear();
    }

    /// Nach Rückgängig/Wiederholen in der Skizze (R10): `sk` ist der zurückgeholte Entwurf.
    /// Laufendes Werkzeug (offene Punkte und Ecken), Auswahl, Ziehen und Meldung gehören zum
    /// alten Stand und entfallen, ebenso ein offenes Maßfeld (über `history_jumps`).
    /// Freiheitsgrade und Färbung kommen aus einem frischen Lösen einer Kopie, damit der Entwurf
    /// selbst (und sein Stand) unverändert bleibt. Werkzeug und Mauszeiger bleiben.
    pub fn after_history_jump(&mut self, sk: &Sketch) {
        self.pending.clear();
        self.pending_pos.clear();
        self.selection.clear();
        self.dragging = None;
        self.message = None;
        self.last_report = Some(sk.clone().solve());
        self.history_jumps += 1;
    }

    /// Fang an der Stelle `at`: Punkte vor Kurven.
    pub fn snap(&self, sk: &Sketch, at: DVec2, radius: f64) -> Snap {
        if let Some(p) = sk.pick_point(at, radius) {
            Snap::Point(p)
        } else if let Some(c) = sk.pick_curve(at, radius) {
            Snap::Curve(c)
        } else {
            Snap::None
        }
    }

    // ---- Atomarität ---------------------------------------------------------------------

    fn backup(&self, sk: &Sketch) -> Backup {
        Backup {
            sketch: sk.clone(),
            pending: self.pending.clone(),
            pending_pos: self.pending_pos.clone(),
            selection: self.selection.clone(),
            last_report: self.last_report.clone(),
        }
    }

    fn restore(&mut self, sk: &mut Sketch, b: Backup) {
        *sk = b.sketch;
        self.pending = b.pending;
        self.pending_pos = b.pending_pos;
        self.selection = b.selection;
        self.last_report = b.last_report;
    }

    /// Führt `f` atomar aus: Bei einem Fehler wird alles zurückgesetzt und die Meldung in
    /// `message` abgelegt. Vorher wird `message` geleert.
    fn atomic<T>(
        &mut self,
        sk: &mut Sketch,
        f: impl FnOnce(&mut Self, &mut Sketch, &Sketch) -> Result<T, String>,
    ) -> Result<T, String> {
        self.message = None;
        let b = self.backup(sk);
        match f(self, sk, &b.sketch) {
            Ok(v) => Ok(v),
            Err(e) => {
                self.restore(sk, b);
                self.message = Some(e.clone());
                Err(e)
            }
        }
    }

    /// Löst die Skizze und merkt sich den Bericht. Macht die Änderung die Skizze widersprüchlich
    /// oder unlösbar, ist das ein Fehler (siehe [`check_change`]).
    fn solve_checked(&mut self, sk: &mut Sketch, prior: &Sketch) -> Result<SolveReport, String> {
        let before = self.last_report.take();
        let report = sk.solve();
        self.last_report = Some(report.clone());
        if let Some(problem) = check_change(&report, before.as_ref(), prior, None) {
            return Err(match problem {
                SolveStatus::Conflicting => {
                    "Widerspruch: Die Änderung widerspricht vorhandenen Abhängigkeiten".to_string()
                }
                _ => "Die Skizze ließ sich nach der Änderung nicht lösen".to_string(),
            });
        }
        self.warn_if_unsolved();
        Ok(report)
    }

    /// Ist die Skizze nach einer erlaubten Änderung weiterhin nicht gelöst (der Fehler bestand
    /// schon vorher), wird darauf hingewiesen, sofern keine andere Meldung ansteht.
    fn warn_if_unsolved(&mut self) {
        if self.message.is_some() {
            return;
        }
        self.message = unsolved_hint(self.last_report.as_ref()).map(str::to_string);
    }

    // ---- Werkzeuge ----------------------------------------------------------------------

    /// Vorhandener Punkt unter `at` oder ein neuer; auf einer Kurve wird der neue Punkt auf die
    /// Kurve gesetzt und mit `PointOnCurve` daran gebunden.
    fn point_at(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) -> Result<PointId, String> {
        match self.snap(sk, at, radius) {
            Snap::Point(p) => Ok(p),
            Snap::Curve(c) => {
                let q = sk.nearest_on_curve(c, at).unwrap_or(at);
                let p = sk.add_point(q.x, q.y).map_err(err)?;
                sk.add_constraint(ConstraintKind::PointOnCurve(p, c))
                    .map_err(err)?;
                Ok(p)
            }
            Snap::None => sk.add_point(at.x, at.y).map_err(err),
        }
    }

    /// Klick mit dem aktiven Werkzeug an der Stelle `at` (Skizzeneinheiten); `radius` ist der
    /// Fangradius, `extend` erweitert bzw. schaltet die Auswahl um.
    pub fn click(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        extend: bool,
    ) -> Result<(), String> {
        if !at.is_finite() || !radius.is_finite() || radius < 0.0 {
            let e = "Ungültige Position".to_string();
            self.message = Some(e.clone());
            return Err(e);
        }
        self.cursor = Some(at);
        self.atomic(sk, |se, sk, prior| match se.tool {
            Tool::Select => {
                se.select_at(sk, at, radius, extend);
                Ok(())
            }
            Tool::Point => se.click_point(sk, at, radius, prior),
            Tool::Line => se.click_line(sk, at, radius, prior),
            Tool::Rectangle => se.click_rectangle(sk, at, prior),
            Tool::Circle => se.click_circle(sk, at, radius, prior),
            Tool::Arc => se.click_arc(sk, at, radius, prior),
            Tool::Ellipse => se.click_ellipse(sk, at, radius, prior),
            Tool::Spline => se.click_spline(sk, at, radius, prior),
            // Wird von der Oberfläche über `project_edge` gesteuert.
            Tool::ProjectEdge => Ok(()),
        })
    }

    fn select_at(&mut self, sk: &Sketch, at: DVec2, radius: f64, extend: bool) {
        let item = match self.snap(sk, at, radius) {
            Snap::Point(p) => Some(Item::Point(p)),
            Snap::Curve(c) => Some(Item::Curve(c)),
            Snap::None => None,
        };
        match (item, extend) {
            (Some(i), true) => {
                if let Some(k) = self.selection.iter().position(|&s| s == i) {
                    self.selection.remove(k);
                } else {
                    self.selection.push(i);
                }
            }
            (Some(i), false) => self.selection = vec![i],
            (None, true) => {}
            (None, false) => self.selection.clear(),
        }
    }

    fn click_point(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), String> {
        if let Snap::Point(_) = self.snap(sk, at, radius) {
            return Ok(());
        }
        self.point_at(sk, at, radius)?;
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_line(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), String> {
        let p = self.point_at(sk, at, radius)?;
        let Some(&start) = self.pending.first() else {
            self.pending = vec![p];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let line = sk.add_line(start, p).map_err(err)?;
        self.solve_checked(sk, prior)?;
        if self.auto_constrain {
            self.add_auto_axis(sk, line);
        }
        self.pending = vec![p];
        Ok(())
    }

    /// Ergänzt bei fast waagerechten/senkrechten Linien `Horizontal`/`Vertical`. Ist die
    /// Abhängigkeit überzählig oder lässt sich die Skizze damit nicht lösen, entfällt sie.
    fn add_auto_axis(&mut self, sk: &mut Sketch, line: CurveId) {
        let Some(CurveKind::Line { a, b }) = sk.curve(line).map(|c| c.kind.clone()) else {
            return;
        };
        let (Ok(pa), Ok(pb)) = (sk.point_xy(a), sk.point_xy(b)) else {
            return;
        };
        let d = pb - pa;
        let angle = d.y.abs().atan2(d.x.abs());
        let kind = if angle < AUTO_AXIS_ANGLE {
            ConstraintKind::Horizontal(LineOrPoints::Line(line))
        } else if std::f64::consts::FRAC_PI_2 - angle < AUTO_AXIS_ANGLE {
            ConstraintKind::Vertical(LineOrPoints::Line(line))
        } else {
            return;
        };
        let before = sk.clone();
        let before_report = self.last_report.clone();
        let Ok(id) = sk.add_constraint(kind) else {
            return;
        };
        let report = sk.solve();
        if status_ok(&report) && !report.redundant.contains(&id) {
            self.last_report = Some(report);
        } else {
            *sk = before;
            self.last_report = before_report;
        }
    }

    fn click_rectangle(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        prior: &Sketch,
    ) -> Result<(), String> {
        let Some(&corner) = self.pending_pos.first() else {
            self.pending_pos = vec![at];
            return Ok(());
        };
        sk.add_rectangle(corner, at).map_err(err)?;
        self.pending_pos.clear();
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_circle(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), String> {
        let Some(&center) = self.pending.first() else {
            let c = self.point_at(sk, at, radius)?;
            self.pending = vec![c];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let r = sk.point_xy(center).map_err(err)?.distance(at);
        if r <= sk.tolerance().linear {
            return Err("Der Radius muss größer als null sein".into());
        }
        sk.add_circle(center, r).map_err(err)?;
        self.pending.clear();
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_arc(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), String> {
        let tol = sk.tolerance().linear;
        match *self.pending.as_slice() {
            [] => {
                let c = self.point_at(sk, at, radius)?;
                self.pending = vec![c];
            }
            [center] => {
                let s = self.point_at(sk, at, radius)?;
                if s == center
                    || sk
                        .point_xy(s)
                        .map_err(err)?
                        .distance(sk.point_xy(center).map_err(err)?)
                        <= tol
                {
                    return Err("Der Startpunkt darf nicht in der Mitte liegen".into());
                }
                self.pending.push(s);
            }
            [center, start, ..] => {
                let m = sk.point_xy(center).map_err(err)?;
                let s = sk.point_xy(start).map_err(err)?;
                let r = s.distance(m);
                let dir = at - m;
                if dir.length() <= tol {
                    return Err("Das Bogenende darf nicht in der Mitte liegen".into());
                }
                let on_start = "Das Ende fällt auf den Start; der Bogen hätte keine Länge";
                let end = match self.snap(sk, at, radius) {
                    Snap::Point(p) if p == start => return Err(on_start.into()),
                    Snap::Point(p) if p != center => p,
                    _ => {
                        let q = m + dir.normalize() * r;
                        if q.distance(s) <= tol {
                            return Err(on_start.into());
                        }
                        sk.add_point(q.x, q.y).map_err(err)?
                    }
                };
                if sk.point_xy(end).map_err(err)?.distance(s) <= tol {
                    return Err(on_start.into());
                }
                sk.add_arc(center, start, end).map_err(err)?;
                self.pending.clear();
            }
        }
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_ellipse(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), String> {
        let tol = sk.tolerance().linear;
        let Some(&center) = self.pending.first() else {
            let c = self.point_at(sk, at, radius)?;
            self.pending = vec![c];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let m = sk.point_xy(center).map_err(err)?;
        let Some(&major_end) = self.pending_pos.first() else {
            if (at - m).length() <= tol {
                return Err("Die große Halbachse muss länger als null sein".into());
            }
            self.pending_pos = vec![at];
            return Ok(());
        };
        let major = major_end - m;
        let len = major.length();
        if len <= tol {
            return Err("Die große Halbachse muss länger als null sein".into());
        }
        let minor = (major / len).perp_dot(at - m).abs().min(len);
        if minor <= tol {
            return Err("Die kleine Halbachse muss größer als null sein".into());
        }
        sk.add_ellipse(center, major, minor).map_err(err)?;
        self.pending.clear();
        self.pending_pos.clear();
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_spline(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), String> {
        if self.pending.len() >= MAX_SPLINE_POINTS {
            return Err(format!(
                "Ein Spline hat höchstens {MAX_SPLINE_POINTS} Punkte; bitte abschließen"
            ));
        }
        let p = self.point_at(sk, at, radius)?;
        if self.pending.contains(&p) {
            return Err("Dieser Punkt gehört schon zum Spline".into());
        }
        self.pending.push(p);
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    /// Schließt das laufende Werkzeug ab: legt den Spline an bzw. beendet den Linienzug.
    pub fn finish_tool(&mut self, sk: &mut Sketch) -> Result<(), String> {
        if self.tool != Tool::Spline {
            self.message = None;
            self.pending.clear();
            self.pending_pos.clear();
            return Ok(());
        }
        self.atomic(sk, |se, sk, prior| {
            if se.pending.len() < 2 {
                return Err("Ein Spline braucht mindestens 2 Punkte".into());
            }
            sk.add_spline(&se.pending).map_err(err)?;
            se.pending.clear();
            se.solve_checked(sk, prior)?;
            Ok(())
        })
    }

    /// Bricht das laufende Werkzeug ab; im Auswahlwerkzeug wird die Auswahl geleert. Ein
    /// laufendes Ziehen wird wie mit [`end_drag`](Self::end_drag) beendet (der Punkt bleibt,
    /// wo er ist).
    pub fn cancel(&mut self, sk: &mut Sketch) {
        self.end_drag(sk);
        self.pending.clear();
        self.pending_pos.clear();
        if self.tool == Tool::Select {
            self.selection.clear();
        }
    }

    // ---- Ziehen -------------------------------------------------------------------------

    /// Beginnt im Auswahlwerkzeug das Ziehen des Punktes unter `at`.
    pub fn begin_drag(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) -> bool {
        if self.tool != Tool::Select || !at.is_finite() {
            return false;
        }
        self.dragging = sk.pick_point(at, radius);
        self.dragging.is_some()
    }

    /// Zieht den gegriffenen Punkt nach `at`. Scheitert der Solver, bleibt die Skizze, und
    /// `message` erklärt es.
    pub fn drag_to(&mut self, sk: &mut Sketch, at: DVec2) {
        let Some(point) = self.dragging else {
            return;
        };
        self.cursor = Some(at);
        match sk.drag(&[Drag { point, target: at }]) {
            Ok(report) => {
                self.message = (!status_ok(&report)).then(|| {
                    "Der Punkt lässt sich wegen der Abhängigkeiten nicht dorthin ziehen".to_string()
                });
                self.last_report = Some(report);
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    /// Beendet das Ziehen. Ein volles Lösen holt dabei die Diagnose nach, die während des
    /// Ziehens für die gezogene Gruppe entfällt (unveränderte Gruppen kommen aus dem
    /// Zwischenspeicher des Solvers und kosten nichts).
    pub fn end_drag(&mut self, sk: &mut Sketch) {
        if self.dragging.take().is_some() {
            self.last_report = Some(sk.solve());
        }
    }

    // ---- Abhängigkeiten -----------------------------------------------------------------

    fn sorted(&self, sk: &Sketch) -> Option<Sorted> {
        let mut s = Sorted::default();
        for item in &self.selection {
            match *item {
                Item::Point(p) => {
                    sk.point_xy(p).ok()?;
                    s.points.push(p);
                }
                Item::Curve(c) => {
                    match sk.curve(c)?.kind {
                        CurveKind::Line { .. } => s.lines.push(c),
                        CurveKind::Circle { .. } | CurveKind::Arc { .. } => s.rounds.push(c),
                        CurveKind::Ellipse { .. } | CurveKind::Spline { .. } => s.others.push(c),
                    }
                    s.curves.push(c);
                }
                Item::Constraint(_) => return None,
            }
        }
        Some(s)
    }

    /// Abhängigkeiten, die zur aktuellen Auswahl passen.
    pub fn available_actions(&self, sk: &Sketch) -> Vec<ConstraintAction> {
        use ConstraintAction::*;
        let Some(s) = self.sorted(sk) else {
            return Vec::new();
        };
        match (
            s.points.len(),
            s.lines.len(),
            s.rounds.len(),
            s.others.len(),
        ) {
            (1, 0, 0, 0) => vec![Fixed],
            (2, 0, 0, 0) => vec![Coincident, Horizontal, Vertical, Distance],
            (0, 1, 0, 0) => vec![Horizontal, Vertical, Distance],
            (0, 2, 0, 0) => vec![Parallel, Perpendicular, Equal, Angle],
            (0, 1, 1, 0) | (0, 1, 0, 1) => vec![Tangent],
            (0, 0, 2, 0) => vec![Tangent, Equal],
            (0, 0, 1, 0) => vec![Radius, Diameter],
            (1, 1, 0, 0) => vec![PointOnCurve, Midpoint, DistancePointLine],
            (1, 0, 1, 0) | (1, 0, 0, 1) => vec![PointOnCurve],
            (2, 1, 0, 0) => vec![Symmetric],
            _ => Vec::new(),
        }
    }

    /// Baut die Abhängigkeit aus der Auswahl (Maße mit Platzhalterwert 0).
    fn build(&self, sk: &Sketch, action: ConstraintAction) -> Result<ConstraintKind, String> {
        use ConstraintAction as A;
        use ConstraintKind as K;
        if self.selection.is_empty() {
            return Err("Es ist nichts ausgewählt".into());
        }
        if !self.available_actions(sk).contains(&action) {
            return Err(format!(
                "„{}“ passt nicht zur aktuellen Auswahl",
                action.label()
            ));
        }
        let s = self
            .sorted(sk)
            .ok_or_else(|| "Die Auswahl ist ungültig".to_string())?;
        let p = &s.points;
        let c = &s.curves;
        Ok(match action {
            A::Fixed => {
                let xy = sk.point_xy(p[0]).map_err(err)?;
                K::Fixed(p[0], xy.x, xy.y)
            }
            A::Coincident => K::Coincident(p[0], p[1]),
            A::Horizontal | A::Vertical => {
                let lp = if p.len() == 2 {
                    LineOrPoints::Points(p[0], p[1])
                } else {
                    LineOrPoints::Line(c[0])
                };
                if action == A::Horizontal {
                    K::Horizontal(lp)
                } else {
                    K::Vertical(lp)
                }
            }
            A::Distance => {
                if p.len() == 2 {
                    K::Distance(p[0], p[1], 0.0)
                } else {
                    let Some(CurveKind::Line { a, b }) = sk.curve(c[0]).map(|d| d.kind.clone())
                    else {
                        return Err("Die Länge gibt es nur für Linien".into());
                    };
                    K::Distance(a, b, 0.0)
                }
            }
            A::Parallel => K::Parallel(c[0], c[1]),
            A::Perpendicular => K::Perpendicular(c[0], c[1]),
            A::Tangent => K::Tangent(c[0], c[1]),
            A::Equal => K::Equal(c[0], c[1]),
            A::Angle => K::Angle(c[0], c[1], 0.0),
            A::Radius => K::Radius(c[0], 0.0),
            A::Diameter => K::Diameter(c[0], 0.0),
            A::PointOnCurve => K::PointOnCurve(p[0], c[0]),
            A::Midpoint => K::Midpoint(p[0], c[0]),
            A::DistancePointLine => K::DistancePointLine(p[0], c[0], 0.0),
            A::Symmetric => K::Symmetric(p[0], p[1], c[0]),
        })
    }

    /// Legt die Abhängigkeit `action` für die Auswahl an und löst. Maße ohne `value` erhalten
    /// den aktuellen Wert. Widersprüchliche oder unlösbare Abhängigkeiten werden verworfen
    /// (Skizze unverändert), überzählige Maße werden nur anzeigend.
    pub fn apply(
        &mut self,
        sk: &mut Sketch,
        action: ConstraintAction,
        value: Option<f64>,
    ) -> Result<ConstraintId, String> {
        self.atomic(sk, |se, sk, prior| {
            let mut kind = se.build(sk, action)?;
            if action.is_dimension() {
                let v = match value {
                    Some(v) => v,
                    None => sk
                        .measure(&kind)
                        .ok_or_else(|| "Das Maß lässt sich nicht messen".to_string())?,
                };
                kind = with_checked_value(sk, &kind, v)?;
            }
            let id = sk.add_constraint(kind).map_err(err)?;
            let before = se.last_report.take();
            let report = sk.solve();
            se.last_report = Some(report.clone());
            if let Some(problem) = check_change(&report, before.as_ref(), prior, Some(id)) {
                return Err(match problem {
                    SolveStatus::Conflicting => format!(
                        "Widerspruch: „{}“ widerspricht vorhandenen Abhängigkeiten",
                        action.label()
                    ),
                    _ => format!("„{}“ ließ sich nicht lösen", action.label()),
                });
            }
            if report.redundant.contains(&id) {
                if action.is_dimension() {
                    sk.set_driven(id, true).map_err(err)?;
                    if let Some(v) = sk.constraint(id).and_then(|c| sk.measure(&c.kind)) {
                        sk.set_constraint_value(id, v).map_err(err)?;
                    }
                    se.last_report = Some(sk.solve());
                    se.message = Some(
                        "Dieses Maß ist bereits durch andere Abhängigkeiten bestimmt und wird \
                         nur angezeigt"
                            .into(),
                    );
                } else {
                    se.message = Some("Diese Abhängigkeit ist überzählig".into());
                }
            }
            se.warn_if_unsolved();
            Ok(id)
        })
    }

    /// Aktueller Wert des Maßes `action` für die Auswahl (Vorgabe des Zahlenfelds); `None`, wenn
    /// das kein Maß ist oder es nicht zur Auswahl passt.
    pub fn measure_action(&self, sk: &Sketch, action: ConstraintAction) -> Option<f64> {
        if !action.is_dimension() {
            return None;
        }
        let kind = self.build(sk, action).ok()?;
        sk.measure(&kind)
    }

    /// Ändert den Wert des Maßes `id` und löst. Ungültige Werte und Werte, die die Skizze
    /// widersprüchlich oder unlösbar machen, werden abgelehnt (Skizze unverändert).
    pub fn set_dimension_value(
        &mut self,
        sk: &mut Sketch,
        id: ConstraintId,
        value: f64,
    ) -> Result<(), String> {
        self.atomic(sk, |se, sk, prior| {
            let kind = sk
                .constraint(id)
                .map(|c| c.kind.clone())
                .ok_or_else(|| "Abhängigkeit nicht gefunden".to_string())?;
            with_checked_value(sk, &kind, value)?;
            sk.set_constraint_value(id, value).map_err(err)?;
            se.solve_checked(sk, prior)?;
            Ok(())
        })
    }

    /// Zahl oder Formel `text` für das bestehende Maß `id` des Entwurfs `sf`: auswerten mit den
    /// Parametern `scope` (Winkel in Grad, gespeichert in Bogenmaß), Wert setzen und lösen wie
    /// [`set_dimension_value`](Self::set_dimension_value), erst danach die Formel setzen bzw.
    /// bei einer Zahl nur die Formel dieses Maßes entfernen (Plan P2). Atomar: Bei einem Fehler
    /// bleiben Skizze und Formeln unverändert, `message` erklärt ihn. Ein anzeigendes Maß nimmt
    /// nur Zahlen.
    pub fn set_dimension_input(
        &mut self,
        sf: &mut SketchFeature,
        id: ConstraintId,
        text: &str,
        scope: &ParamScope,
    ) -> Result<(), String> {
        self.message = None;
        let read = match sf.sketch.constraint(id) {
            None => Err("Abhängigkeit nicht gefunden".to_string()),
            Some(c) if !c.kind.is_dimension() => Err("Das ist kein Maß".to_string()),
            Some(c) => {
                let driven = c.driven;
                dimension_input(text, constraint_unit(&c.kind), scope).and_then(|(v, f)| {
                    if driven && f.is_some() {
                        Err(DRIVEN_NO_FORMULA.to_string())
                    } else {
                        Ok((v, f))
                    }
                })
            }
        };
        let (value, formula) = match read {
            Ok(r) => r,
            Err(e) => {
                self.message = Some(e.clone());
                return Err(e);
            }
        };
        self.set_dimension_value(&mut sf.sketch, id, value)?;
        match formula {
            Some(f) => {
                sf.formulas.insert(id, f);
            }
            None => {
                sf.formulas.remove(&id);
            }
        }
        self.note_formula_problem(sf);
        Ok(())
    }

    /// Neues Maß `action` aus der Auswahl mit Zahl oder Formel `text` (wie
    /// [`apply`](Self::apply)); die Formel kommt erst nach dem erfolgreichen Lösen dazu. Wird
    /// das Maß anzeigend (überzählig), entfällt die Formel, und die Meldung von `apply` steht da.
    pub fn apply_dimension_input(
        &mut self,
        sf: &mut SketchFeature,
        action: ConstraintAction,
        text: &str,
        scope: &ParamScope,
    ) -> Result<ConstraintId, String> {
        self.message = None;
        let read = if action.is_dimension() {
            dimension_input(text, action.unit(), scope)
        } else {
            Err("Das ist kein Maß".to_string())
        };
        let (value, formula) = match read {
            Ok(r) => r,
            Err(e) => {
                self.message = Some(e.clone());
                return Err(e);
            }
        };
        let id = self.apply(&mut sf.sketch, action, Some(value))?;
        let driven = sf.sketch.constraint(id).is_none_or(|c| c.driven);
        match formula {
            Some(f) if !driven => {
                sf.formulas.insert(id, f);
            }
            _ => {
                sf.formulas.remove(&id);
            }
        }
        self.note_formula_problem(sf);
        Ok(id)
    }

    /// Lässt sich eine Skizze mit Formeln nach einer erlaubten Änderung nicht lösen (der Fehler
    /// bestand schon vorher), steht der Fehler, den sie beim Beenden bekommt (Plan P12), als
    /// Meldung da, sofern keine andere ansteht oder nur der allgemeine Hinweis.
    fn note_formula_problem(&mut self, sf: &SketchFeature) {
        if let Some(p) = formula_problem(sf, self.last_report.as_ref())
            && (self.message.is_none()
                || self.message.as_deref() == unsolved_hint(self.last_report.as_ref()))
        {
            self.message = Some(p.to_string());
        }
    }

    /// Schaltet das Maß `id` auf „nur anzeigend“ (`driven`) oder zurück auf bestimmend. Beim
    /// Zurückschalten übernimmt es den aktuellen Wert; wird die Skizze dadurch widersprüchlich,
    /// bleibt alles unverändert.
    pub fn set_driven(
        &mut self,
        sk: &mut Sketch,
        id: ConstraintId,
        driven: bool,
    ) -> Result<(), String> {
        self.atomic(sk, |se, sk, prior| {
            let kind = sk
                .constraint(id)
                .map(|c| c.kind.clone())
                .ok_or_else(|| "Abhängigkeit nicht gefunden".to_string())?;
            if !kind.is_dimension() {
                return Err("Nur Maße können nur anzeigend sein".into());
            }
            if let Some(v) = sk.measure(&kind) {
                sk.set_constraint_value(id, v).map_err(err)?;
            }
            sk.set_driven(id, driven).map_err(err)?;
            let before = se.last_report.take();
            let report = sk.solve();
            se.last_report = Some(report.clone());
            if let Some(problem) =
                check_change(&report, before.as_ref(), prior, (!driven).then_some(id))
            {
                return Err(match problem {
                    SolveStatus::Conflicting => {
                        "Widerspruch: Das Maß widerspricht vorhandenen Abhängigkeiten".to_string()
                    }
                    _ => "Die Skizze ließ sich nach der Änderung nicht lösen".to_string(),
                });
            }
            if !driven && report.redundant.contains(&id) {
                return Err(
                    "Dieses Maß ist bereits durch andere Abhängigkeiten bestimmt und bleibt nur \
                     anzeigend"
                        .into(),
                );
            }
            se.warn_if_unsolved();
            Ok(())
        })
    }

    // ---- Bearbeiten ---------------------------------------------------------------------

    /// Löscht die Auswahl samt abhängiger Kurven und Abhängigkeiten. Punkte gelöschter (auch
    /// mitgelöschter) Kurven, die danach nichts mehr benutzt, werden mit entfernt; andere freie
    /// Punkte bleiben.
    pub fn delete_selection(&mut self, sk: &mut Sketch) {
        self.message = None;
        let mut orphans: Vec<PointId> = Vec::new();
        for item in std::mem::take(&mut self.selection) {
            match item {
                Item::Constraint(c) => {
                    let _ = sk.remove_constraint(c);
                }
                Item::Curve(c) => {
                    if let Some(d) = sk.curve(c) {
                        orphans.extend(d.kind.points());
                    }
                    let _ = sk.remove_curve(c);
                }
                Item::Point(p) => {
                    // Andere Punkte der mitgelöschten Kurven werden ebenso geprüft.
                    orphans.extend(
                        sk.curves()
                            .map(|(_, c)| c.kind.points())
                            .filter(|pts| pts.contains(&p))
                            .flatten(),
                    );
                    let _ = sk.remove_point(p);
                }
            }
        }
        sk.prune_points(&orphans);
        self.pending.retain(|&p| sk.point_xy(p).is_ok());
        if self.dragging.is_some_and(|p| sk.point_xy(p).is_err()) {
            self.dragging = None;
        }
        self.last_report = Some(sk.solve());
    }

    /// Schaltet bei den ausgewählten Kurven die Konstruktionseigenschaft um.
    pub fn toggle_construction(&mut self, sk: &mut Sketch) {
        for item in &self.selection {
            if let Item::Curve(c) = *item
                && let Some(construction) = sk.curve(c).map(|d| d.construction)
            {
                let _ = sk.set_construction(c, !construction);
            }
        }
    }

    /// Statuszeile: Freiheitsgrade, Widerspruch, überzählige Abhängigkeiten.
    pub fn status(&self, sk: &Sketch) -> (String, Severity) {
        let report = match &self.last_report {
            Some(r) => r.clone(),
            None => sk.clone().solve(),
        };
        match report.status {
            SolveStatus::Conflicting => (
                format!(
                    "Widerspruch zwischen {} Abhängigkeiten",
                    report.conflicting.len()
                ),
                Severity::Error,
            ),
            SolveStatus::NotConverged => ("Nicht lösbar".into(), Severity::Error),
            SolveStatus::Solved => {
                let mut text = match report.dof {
                    0 => "Vollständig bestimmt".to_string(),
                    1 => "1 Freiheitsgrad".to_string(),
                    n => format!("{n} Freiheitsgrade"),
                };
                if report.redundant.is_empty() {
                    (text, Severity::Info)
                } else {
                    text.push_str(&format!(", {} überzählig", report.redundant.len()));
                    (text, Severity::Warning)
                }
            }
        }
    }

    fn selected_curves(&self) -> Vec<CurveId> {
        self.selection
            .iter()
            .filter_map(|i| match i {
                Item::Curve(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// Spiegelt die ausgewählten Kurven an der Linie `axis`.
    pub fn mirror_selection(&mut self, sk: &mut Sketch, axis: CurveId) -> Result<(), String> {
        let curves: Vec<CurveId> = self
            .selected_curves()
            .into_iter()
            .filter(|&c| c != axis)
            .collect();
        self.atomic(sk, |se, sk, prior| {
            sk.add_mirrored(&curves, axis).map_err(err)?;
            se.solve_checked(sk, prior)?;
            Ok(())
        })
    }

    /// Lineares Muster der ausgewählten Kurven: `count` Kopien im Abstand `offset`.
    pub fn pattern_selection(
        &mut self,
        sk: &mut Sketch,
        offset: DVec2,
        count: usize,
    ) -> Result<(), String> {
        let curves = self.selected_curves();
        self.atomic(sk, |se, sk, prior| {
            sk.add_linear_pattern(&curves, offset, count).map_err(err)?;
            se.solve_checked(sk, prior)?;
            Ok(())
        })
    }

    /// Kopiert eine Körperkante als feste Konstruktionsgeometrie in die Skizze.
    pub fn project_edge(
        &mut self,
        sk: &mut Sketch,
        solid: &Solid,
        edge: Id<Edge>,
    ) -> Result<CurveId, String> {
        self.atomic(sk, |se, sk, prior| {
            let c = sk.add_projected_edge(solid, edge).map_err(err)?;
            se.solve_checked(sk, prior)?;
            Ok(c)
        })
    }
}

/// Prüft den Bericht nach einer Änderung gegen den Zustand davor. Liefert den Grund
/// (`Conflicting`/`NotConverged`), wenn die Änderung zurückgenommen werden muss:
/// - die Skizze ist nicht gelöst, war es aber vorher;
/// - die neue Abhängigkeit `new` gehört zu den widersprüchlichen;
/// - die Skizze war schon vorher kaputt, jetzt widersprechen sich aber Abhängigkeiten, die es
///   vorher nicht taten, oder sie wurde von widersprüchlich zu nicht lösbar.
///
/// Der Zustand davor ist `before` (der letzte Bericht der Sitzung), sonst wird `prior` gelöst.
///
/// Grenze: Der Bericht nennt bei nicht lösbaren Gruppen nur Abhängigkeiten mit zu großem
/// Residuum; eine neue nicht lösbare Gruppe neben einem schon bestehenden Widerspruch wird nur
/// erkannt, wenn sie dabei neue Abhängigkeiten nennt.
fn check_change(
    report: &SolveReport,
    before: Option<&SolveReport>,
    prior: &Sketch,
    new: Option<ConstraintId>,
) -> Option<SolveStatus> {
    if status_ok(report) {
        return None;
    }
    let bad = Some(report.status);
    if new.is_some_and(|id| report.conflicting.contains(&id)) {
        return bad;
    }
    let solved_prior;
    let before = match before {
        Some(b) => b,
        None => {
            solved_prior = prior.clone().solve();
            &solved_prior
        }
    };
    if status_ok(before) {
        return bad;
    }
    let grown = report
        .conflicting
        .iter()
        .any(|c| !before.conflicting.contains(c));
    let worse =
        before.status == SolveStatus::Conflicting && report.status == SolveStatus::NotConverged;
    (grown || worse).then_some(report.status)
}

/// Das Maß `kind` mit dem Wert `value`, geprüft nach der gemeinsamen Regel der Skizze
/// ([`Sketch::check_constraint_value`]). Nicht endliche Eingaben ergeben die Meldung des
/// Zahlenfelds.
pub fn with_checked_value(
    sk: &Sketch,
    kind: &ConstraintKind,
    value: f64,
) -> Result<ConstraintKind, String> {
    if !value.is_finite() {
        return Err("Bitte eine Zahl eingeben".into());
    }
    let changed = kind
        .with_value(value)
        .ok_or_else(|| "Das ist kein Maß".to_string())?;
    sk.check_constraint_value(&changed).map_err(err)?;
    Ok(changed)
}
