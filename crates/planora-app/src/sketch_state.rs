//! Skizzen-Modus ohne Oberfläche: Werkzeuge, Auswahl, Ziehen und Abhängigkeiten.
//!
//! Alle Methoden, die die Skizze ändern, sind atomar: Bei einem Fehler bleiben Skizze und
//! Sitzung (laufendes Werkzeug, Auswahl) unverändert, und `message` erklärt den Fehler.

use crate::formula_field::resolve;
use crate::msg::Msg;
use crate::sketch_cursor::{CursorOptions, DrawCursor};
use planora_feature::{FeatureId, ParamScope, ParamUnit, SketchFeature, constraint_unit};
use planora_i18n::tr;
use planora_kernel::{Edge, Id, Solid};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, MAX_SPLINE_POINTS, PointId, Sketch};
use planora_sketch::snap::{SnapHit, SnapIndex};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};
use std::sync::Arc;

pub use planora_sketch::snap::Snap;

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
    /// Fusion „Stutzen“: Klick oder Ziehen löscht das Kurvenstück zwischen den nächsten
    /// Schnittpunkten.
    Trim,
    /// Fusion „Dehnen“: verlängert ein offenes Ende bis zur nächsten Kurve.
    Extend,
    /// Fusion „Brechen“: teilt eine Kurve an den Schnittpunkten um die Klickstelle.
    Break,
}

impl Tool {
    /// Werkzeuge unter „Ändern“ (Stutzen, Dehnen, Brechen): ohne offene Punkte, Esc beendet
    /// sie.
    pub fn is_modify(self) -> bool {
        matches!(self, Tool::Trim | Tool::Extend | Tool::Break)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Point(PointId),
    Curve(CurveId),
    Constraint(ConstraintId),
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
    /// Name als gespeicherte Meldung (für Meldungen, die mit der Sprache wechseln).
    pub fn label_msg(self) -> Msg {
        use ConstraintAction::*;
        match self {
            Coincident => crate::msg!("sketch-constraint-coincident"),
            Horizontal => crate::msg!("sketch-constraint-horizontal"),
            Vertical => crate::msg!("sketch-constraint-vertical"),
            Parallel => crate::msg!("sketch-constraint-parallel"),
            Perpendicular => crate::msg!("sketch-constraint-perpendicular"),
            Tangent => crate::msg!("sketch-constraint-tangent"),
            Equal => crate::msg!("sketch-constraint-equal"),
            Fixed => crate::msg!("sketch-constraint-fixed"),
            Midpoint => crate::msg!("sketch-constraint-midpoint"),
            Symmetric => crate::msg!("sketch-constraint-symmetric"),
            PointOnCurve => crate::msg!("sketch-constraint-point-on-curve"),
            Distance => crate::msg!("sketch-constraint-distance"),
            DistancePointLine => crate::msg!("sketch-constraint-distance-point-line"),
            Radius => crate::msg!("sketch-constraint-radius"),
            Diameter => crate::msg!("sketch-constraint-diameter"),
            Angle => crate::msg!("sketch-constraint-angle"),
        }
    }

    /// Name in der aktuellen Sprache.
    pub fn label(self) -> String {
        self.label_msg().text()
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
pub fn driven_no_formula() -> String {
    tr!("sketch-driven-no-formula")
}

/// Zustand einer Skizze mit Formeln, die sich nicht lösen lässt (Plan P12): derselbe Text wie
/// der Fehlerstatus, den die Skizze beim Beenden bekommt. `None` ohne Formeln oder gelöst.
pub fn formula_problem(sf: &SketchFeature, report: Option<&SolveReport>) -> Option<Msg> {
    if sf.formulas.is_empty() {
        return None;
    }
    match report?.status {
        SolveStatus::Solved => None,
        SolveStatus::Conflicting => Some(crate::msg!("sketch-formula-conflict")),
        SolveStatus::NotConverged => Some(crate::msg!("sketch-formula-unsolvable")),
    }
}

/// Wert für die Skizze (Winkel in Bogenmaß) und Formel aus dem Text eines Maßfelds.
fn dimension_input(
    text: &str,
    unit: ParamUnit,
    scope: &ParamScope,
) -> Result<(f64, Option<String>), Msg> {
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
    /// Fang der noch nicht angelegten Eckpunkte in `pending_pos` (Rechteck: erste Ecke, Ellipse:
    /// Ende der großen Achse); die Anbindung folgt beim Abschluss. Bogen: Fang der Mitte (nur eine
    /// frei gesetzte Mitte rückt für die Tangente zurecht).
    pub pending_snaps: Vec<SnapHit>,
    pub cursor: Option<DVec2>,
    /// Strg und Raster für den Zeiger ([`resolve_cursor`](Self::resolve_cursor)).
    pub cursor_opts: CursorOptions,
    /// Aufgelöster Zeiger dieses Bilds ([`update_cursor`](Self::update_cursor)).
    pub draw_cursor: Option<DrawCursor>,
    /// Fang-Index zum letzten gesehenen Stand der Skizze (nur in Zeichenwerkzeugen gebaut).
    pub snap_index: Option<Arc<SnapIndex>>,
    /// Beim Zeichnen automatisch waagerecht, senkrecht, parallel, rechtwinklig und tangential
    /// ergänzen (die Anbindung an Fangziele bleibt auch ohne).
    pub auto_constrain: bool,
    /// Letzte Linie des laufenden Linienzugs; was mit ihr zusammenhängt, ist kein Richtungsbezug.
    pub line_chain: Option<CurveId>,
    /// Bogen: Kurve, an der der gesetzte Anfang tangential weiterläuft; entsteht beim Abschluss.
    pub auto_tangent: Option<CurveId>,
    pub last_report: Option<SolveReport>,
    /// Meldung zum letzten Schritt; gespeichert als Grund und erst beim Zeichnen übersetzt
    /// ([`message_text`](Self::message_text)).
    pub message: Option<Msg>,
    pub dragging: Option<PointId>,
    /// Zählt die Sprünge durch Rückgängig/Wiederholen ([`SketchSession::after_history_jump`]);
    /// Eingabefelder der Oberfläche, die zu einem älteren Zählerstand gehören, schließen.
    pub history_jumps: u64,
    /// Vorschau unter dem Mauszeiger (Stutzen und Brechen: das Stück, Dehnen: der Weg zum neuen
    /// Ende), je Bild neu bestimmt ([`update_hover`](Self::update_hover)).
    pub hover_preview: Option<Vec<DVec2>>,
    /// Name des nächsten Schritts für Rückgängig, wenn ein Werkzeug ihn besser kennt als der
    /// Vergleich der Stände (Stutzen, Dehnen, Brechen); der Verlauf nimmt und leert ihn.
    pub edit_label: Option<Msg>,
    /// Letzte Stelle beim Stutzen durch Ziehen (`None`: kein Zug läuft).
    pub trim_drag: Option<DVec2>,
    /// Zählt die gelungenen Änderungen durch Stutzen, Dehnen und Brechen, mit dem Werkzeug der
    /// letzten (für „Letztes Skizzenwerkzeug wiederholen“, auch wenn Esc es im selben Bild
    /// beendet).
    pub modify_edits: (u64, Option<Tool>),
}

/// Was vor einer Änderung gesichert wird, um sie bei einem Fehler zurückzunehmen.
struct Backup {
    sketch: Sketch,
    pending: Vec<PointId>,
    pending_pos: Vec<DVec2>,
    pending_snaps: Vec<SnapHit>,
    line_chain: Option<CurveId>,
    auto_tangent: Option<CurveId>,
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

/// Fehler einer Bibliothek als Meldung (der Text kommt von dort, bis die Bibliotheken übersetzt
/// sind).
pub(crate) fn err(e: impl std::fmt::Display) -> Msg {
    Msg::Raw(e.to_string())
}

fn status_ok(r: &SolveReport) -> bool {
    r.status == SolveStatus::Solved
}

/// Hinweis, wenn die Skizze nach einer erlaubten Änderung weiterhin nicht gelöst ist.
fn unsolved_hint(report: Option<&SolveReport>) -> Option<Msg> {
    match report.map(|r| r.status) {
        Some(SolveStatus::Conflicting) => Some(crate::msg!("sketch-still-conflicting")),
        Some(SolveStatus::NotConverged) => Some(crate::msg!("sketch-still-unsolved")),
        _ => None,
    }
}

impl SketchSession {
    /// Die Meldung in der aktuellen Sprache.
    pub fn message_text(&self) -> Option<String> {
        self.message.as_ref().map(Msg::text)
    }

    pub fn new(sketch: FeatureId) -> SketchSession {
        SketchSession {
            sketch,
            tool: Tool::Select,
            selection: Vec::new(),
            pending: Vec::new(),
            pending_pos: Vec::new(),
            pending_snaps: Vec::new(),
            cursor: None,
            cursor_opts: CursorOptions::default(),
            draw_cursor: None,
            snap_index: None,
            auto_constrain: true,
            line_chain: None,
            auto_tangent: None,
            last_report: None,
            message: None,
            dragging: None,
            history_jumps: 0,
            hover_preview: None,
            edit_label: None,
            trim_drag: None,
            modify_edits: (0, None),
        }
    }

    /// Wechselt das Werkzeug; ein laufendes Werkzeug wird abgebrochen, ein laufendes Ziehen
    /// wie mit [`end_drag`](Self::end_drag) beendet.
    pub fn set_tool(&mut self, sk: &mut Sketch, tool: Tool) {
        self.end_drag(sk);
        self.trim_drag = None;
        self.hover_preview = None;
        self.tool = tool;
        self.draw_cursor = None;
        self.clear_pending();
    }

    /// Vergisst, was das laufende Werkzeug schon gesetzt hat.
    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_pos.clear();
        self.pending_snaps.clear();
        self.line_chain = None;
        self.auto_tangent = None;
    }

    /// Wählt `tool` wie der Klick auf seinen Knopf in der Leiste (und sein Tastenkürzel): ist
    /// es schon gewählt, bleibt alles (auch ein laufender Linienzug), sonst wie
    /// [`set_tool`](Self::set_tool).
    pub fn choose_tool(&mut self, sk: &mut Sketch, tool: Tool) {
        if self.tool != tool {
            self.set_tool(sk, tool);
        }
    }

    /// Nach Rückgängig/Wiederholen in der Skizze (R10): `sk` ist der zurückgeholte Entwurf.
    /// Laufendes Werkzeug (offene Punkte und Ecken), Auswahl, Ziehen und Meldung gehören zum
    /// alten Stand und entfallen, ebenso ein offenes Maßfeld (über `history_jumps`).
    /// Freiheitsgrade und Färbung kommen aus einem frischen Lösen einer Kopie, damit der Entwurf
    /// selbst (und sein Stand) unverändert bleibt. Werkzeug und Mauszeiger bleiben.
    pub fn after_history_jump(&mut self, sk: &Sketch) {
        self.clear_pending();
        self.draw_cursor = None;
        self.selection.clear();
        self.dragging = None;
        self.trim_drag = None;
        self.hover_preview = None;
        self.edit_label = None;
        self.message = None;
        self.last_report = Some(sk.clone().solve());
        self.history_jumps += 1;
    }

    /// Fang für die Auswahl an der Stelle `at`: Punkte vor Kurven (nur [`Snap::Point`],
    /// [`Snap::Curve`] und [`Snap::None`]; beim Zeichnen gilt
    /// [`resolve_cursor`](Self::resolve_cursor)).
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
            pending_snaps: self.pending_snaps.clone(),
            line_chain: self.line_chain,
            auto_tangent: self.auto_tangent,
            selection: self.selection.clone(),
            last_report: self.last_report.clone(),
        }
    }

    fn restore(&mut self, sk: &mut Sketch, b: Backup) {
        *sk = b.sketch;
        self.pending = b.pending;
        self.pending_pos = b.pending_pos;
        self.pending_snaps = b.pending_snaps;
        self.line_chain = b.line_chain;
        self.auto_tangent = b.auto_tangent;
        self.selection = b.selection;
        self.last_report = b.last_report;
    }

    /// Führt `f` atomar aus: Bei einem Fehler wird alles zurückgesetzt und die Meldung in
    /// `message` abgelegt. Vorher wird `message` geleert.
    fn atomic<T>(
        &mut self,
        sk: &mut Sketch,
        f: impl FnOnce(&mut Self, &mut Sketch, &Sketch) -> Result<T, Msg>,
    ) -> Result<T, String> {
        self.message = None;
        let b = self.backup(sk);
        match f(self, sk, &b.sketch) {
            Ok(v) => Ok(v),
            Err(e) => {
                self.restore(sk, b);
                let text = e.text();
                self.message = Some(e);
                Err(text)
            }
        }
    }

    /// Löst die Skizze und merkt sich den Bericht. Macht die Änderung die Skizze widersprüchlich
    /// oder unlösbar, ist das ein Fehler (siehe [`check_change`]).
    fn solve_checked(&mut self, sk: &mut Sketch, prior: &Sketch) -> Result<SolveReport, Msg> {
        let before = self.last_report.take();
        let report = sk.solve();
        self.last_report = Some(report.clone());
        if let Some(problem) = check_change(&report, before.as_ref(), prior, None) {
            return Err(match problem {
                SolveStatus::Conflicting => crate::msg!("sketch-change-conflict"),
                _ => crate::msg!("sketch-change-unsolved"),
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
        self.message = unsolved_hint(self.last_report.as_ref());
    }

    // ---- Werkzeuge ----------------------------------------------------------------------

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
            let e = crate::msg!("sketch-invalid-position");
            let text = e.text();
            self.message = Some(e);
            return Err(text);
        }
        self.cursor = Some(at);
        let cur = self.resolve_cursor(sk, at, radius);
        self.atomic(sk, |se, sk, prior| match se.tool {
            Tool::Select => {
                se.select_at(sk, at, radius, extend);
                Ok(())
            }
            Tool::Point => se.click_point(sk, &cur, prior),
            Tool::Line => se.click_line(sk, &cur, prior),
            Tool::Rectangle => se.click_rectangle(sk, &cur, prior),
            Tool::Circle => se.click_circle(sk, &cur, prior),
            Tool::Arc => se.click_arc(sk, &cur, prior),
            Tool::Ellipse => se.click_ellipse(sk, &cur, prior),
            Tool::Spline => se.click_spline(sk, &cur, prior),
            // Wird von der Oberfläche über `project_edge` gesteuert.
            Tool::ProjectEdge => Ok(()),
            Tool::Trim | Tool::Extend | Tool::Break => se.click_modify(sk, at, radius, prior),
        })
    }

    /// Stutzen, Dehnen oder Brechen der Kurve unter `at`. Ohne Kurve dort passiert nichts;
    /// Dehnen ohne Ziel meldet `sketch-extend-no-hit` und ändert nichts.
    fn click_modify(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let Some(c) = sk.pick_curve(at, radius) else {
            return Ok(());
        };
        let label = match self.tool {
            Tool::Trim => {
                sk.trim(c, at).map_err(err)?;
                crate::msg!("undo-trim")
            }
            Tool::Extend => {
                if !sk.extend(c, at).map_err(err)? {
                    return Err(crate::msg!("sketch-extend-no-hit"));
                }
                crate::msg!("undo-extend")
            }
            Tool::Break => {
                if sk.break_curve(c, at).map_err(err)?.len() < 2 {
                    return Ok(());
                }
                crate::msg!("undo-break")
            }
            _ => return Ok(()),
        };
        self.solve_checked(sk, prior)?;
        self.forget_removed(sk);
        self.hover_preview = None;
        self.edit_label = Some(label);
        self.modify_edits = (self.modify_edits.0 + 1, Some(self.tool));
        Ok(())
    }

    /// Auswahl und offene Punkte ohne die Elemente, die es nicht mehr gibt.
    fn forget_removed(&mut self, sk: &Sketch) {
        self.selection.retain(|i| match *i {
            Item::Point(p) => sk.point_xy(p).is_ok(),
            Item::Curve(c) => sk.curve(c).is_some(),
            Item::Constraint(k) => sk.constraint(k).is_some(),
        });
        self.pending.retain(|&p| sk.point_xy(p).is_ok());
    }

    /// Vorschau unter dem Mauszeiger `at` (siehe [`hover_preview`](Self::hover_preview)):
    /// nur bei Stutzen, Dehnen und Brechen.
    pub fn update_hover(&mut self, sk: &Sketch, at: Option<DVec2>, radius: f64) {
        self.hover_preview = at
            .filter(|a| a.is_finite() && self.tool.is_modify())
            .and_then(|at| {
                let c = sk.pick_curve(at, radius)?;
                match self.tool {
                    Tool::Extend => sk.extend_preview(c, at),
                    _ => Some(sk.trim_piece(c, at)?.polyline),
                }
            })
            .filter(|pl| pl.len() >= 2);
    }

    /// Stutzen durch Ziehen: beginnt bei `at` (die Stelle des Drückens) und stutzt dort.
    pub fn begin_trim_drag(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) {
        if self.tool != Tool::Trim || !at.is_finite() {
            return;
        }
        self.trim_drag = Some(at);
        self.trim_along(sk, at, at, radius);
    }

    /// Stutzen durch Ziehen bis `at`: jede Kurve, die der Weg seit der letzten Stelle überfährt,
    /// wird an der überfahrenen Stelle gestutzt. Alles zusammen wird ein Schritt, weil der
    /// Verlauf erst nach dem Loslassen der Maustaste fortgeschrieben wird.
    pub fn trim_drag_to(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) {
        let Some(from) = self.trim_drag else {
            return;
        };
        if self.tool != Tool::Trim || !at.is_finite() {
            self.trim_drag = None;
            return;
        }
        self.trim_along(sk, from, at, radius);
        self.trim_drag = Some(at);
    }

    /// Beendet das Stutzen durch Ziehen.
    pub fn end_trim_drag(&mut self) {
        self.trim_drag = None;
    }

    /// Stutzt jede Kurve, die die Strecke `from`–`to` berührt (Proben im Abstand von höchstens
    /// `radius`). `from` selbst wird nur geprüft, wenn `from == to` (Beginn des Zugs); sonst
    /// war es schon die letzte Probe des vorigen Bilds, und eine zweite Probe dort träfe nach
    /// dem Stutzen das nächstgelegene andere Stück, das der Zug nie überfahren hat. Scheitert
    /// ein Stück, steht der Grund in `message`; die übrigen werden trotzdem gestutzt.
    fn trim_along(&mut self, sk: &mut Sketch, from: DVec2, to: DVec2, radius: f64) {
        /// Höchstens so viele Proben je Bild (ein sehr schneller Zug über eine kleine Skizze).
        const MAX_SAMPLES: usize = 400;
        let len = from.distance(to);
        let (first, n) = if len == 0.0 {
            (0, 0)
        } else if radius > 0.0 {
            (1, ((len / radius).ceil() as usize).clamp(1, MAX_SAMPLES))
        } else {
            (1, 1)
        };
        let mut failed = None;
        for k in first..=n {
            let p = if n == 0 {
                from
            } else {
                from.lerp(to, k as f64 / n as f64)
            };
            if sk.pick_curve(p, radius).is_none() {
                continue;
            }
            if self.click(sk, p, radius, false).is_err() {
                failed = self.message.take();
            }
        }
        if failed.is_some() {
            self.message = failed;
        }
        self.cursor = Some(to);
    }

    fn select_at(&mut self, sk: &Sketch, at: DVec2, radius: f64, extend: bool) {
        let item = match self.snap(sk, at, radius) {
            Snap::Point(p) => Some(Item::Point(p)),
            Snap::Curve(c) => Some(Item::Curve(c)),
            _ => None,
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
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        if cur.point(sk.tolerance().linear).is_some() {
            return Ok(());
        }
        self.attach(sk, None, cur)?;
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_line(&mut self, sk: &mut Sketch, cur: &DrawCursor, prior: &Sketch) -> Result<(), Msg> {
        let p = self.attach(sk, None, cur)?;
        let Some(&start) = self.pending.first() else {
            self.pending = vec![p];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let line = sk.add_line(start, p).map_err(err)?;
        self.solve_checked(sk, prior)?;
        for auto in &cur.autos {
            self.try_auto(sk, auto.kind(line));
        }
        self.line_chain = Some(line);
        self.pending = vec![p];
        Ok(())
    }

    /// Versucht eine automatische Abhängigkeit an einer Kopie: Sie entfällt still (`false`), wenn
    /// die Skizze vorher nicht gelöst war, danach nicht gelöst ist, die neue überzählig wäre oder
    /// ein Punkt sich um mehr als `10·tol.linear` bewegen würde.
    pub fn try_auto(&mut self, sk: &mut Sketch, kind: ConstraintKind) -> bool {
        if !self.last_report.as_ref().is_some_and(status_ok) {
            return false;
        }
        let mut attempt = sk.clone();
        let Ok(id) = attempt.add_constraint(kind) else {
            return false;
        };
        let report = attempt.solve();
        let limit = 10.0 * sk.tolerance().linear;
        let moved = sk
            .points()
            .any(|(p, _)| match (sk.point_xy(p), attempt.point_xy(p)) {
                (Ok(a), Ok(b)) => a.distance(b) > limit,
                _ => true,
            });
        if !status_ok(&report) || report.redundant.contains(&id) || moved {
            return false;
        }
        *sk = attempt;
        self.last_report = Some(report);
        true
    }

    fn click_rectangle(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let Some(&corner) = self.pending_pos.first() else {
            self.pending_pos = vec![cur.at];
            self.pending_snaps = vec![cur.remember(tol)];
            return Ok(());
        };
        let first = self
            .pending_snaps
            .first()
            .map(|&hit| DrawCursor::remembered(hit, sk));
        let rect = sk.add_rectangle(corner, cur.at).map_err(err)?;
        self.pending_pos.clear();
        self.pending_snaps.clear();
        // Das Rechteck legt eigene Ecken an (sortiert); gebunden wird die Ecke an der Lage.
        for c in first.iter().chain([cur]) {
            let own = rect
                .points
                .iter()
                .copied()
                .find(|&p| sk.point_xy(p).is_ok_and(|q| q.distance(c.at) <= tol));
            if own.is_some() {
                self.attach(sk, own, c)?;
            }
        }
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_circle(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let Some(&center) = self.pending.first() else {
            let c = self.attach(sk, None, cur)?;
            self.pending = vec![c];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let r = sk.point_xy(center).map_err(err)?.distance(cur.at);
        if r <= sk.tolerance().linear {
            return Err(crate::msg!("sketch-radius-positive"));
        }
        let circle = sk.add_circle(center, r).map_err(err)?;
        self.bind_rim(sk, circle, cur)?;
        self.pending.clear();
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_arc(&mut self, sk: &mut Sketch, cur: &DrawCursor, prior: &Sketch) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        match *self.pending.as_slice() {
            [] => {
                let c = self.attach(sk, None, cur)?;
                self.pending = vec![c];
                self.pending_snaps = vec![cur.remember(tol)];
            }
            [center] => {
                let s = self.attach(sk, None, cur)?;
                if s == center
                    || sk
                        .point_xy(s)
                        .map_err(err)?
                        .distance(sk.point_xy(center).map_err(err)?)
                        <= tol
                {
                    return Err(crate::msg!("sketch-arc-start-center"));
                }
                self.pending.push(s);
                self.plan_arc_tangent(sk, center, s, cur)?;
            }
            [center, start, ..] => {
                let m = sk.point_xy(center).map_err(err)?;
                let s = sk.point_xy(start).map_err(err)?;
                let r = s.distance(m);
                let dir = cur.at - m;
                if dir.length() <= tol {
                    return Err(crate::msg!("sketch-arc-end-center"));
                }
                let on_start = || crate::msg!("sketch-arc-end-on-start");
                let end = match cur.point(tol) {
                    Some(p) if p == start => return Err(on_start()),
                    Some(p) if p != center => p,
                    _ => {
                        // Das Ende liegt auf dem Radius; es bindet nur, wenn es dort das
                        // Fangziel trifft.
                        let q = m + dir.normalize() * r;
                        if q.distance(s) <= tol {
                            return Err(on_start());
                        }
                        let on_radius = DrawCursor {
                            at: q,
                            ..cur.clone()
                        };
                        self.attach(sk, None, &on_radius)?
                    }
                };
                if sk.point_xy(end).map_err(err)?.distance(s) <= tol {
                    return Err(on_start());
                }
                let arc = sk.add_arc(center, start, end).map_err(err)?;
                self.clear_pending();
                self.solve_checked(sk, prior)?;
                for auto in &cur.autos {
                    self.try_auto(sk, auto.kind(arc));
                }
                return Ok(());
            }
        }
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_ellipse(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let Some(&center) = self.pending.first() else {
            let c = self.attach(sk, None, cur)?;
            self.pending = vec![c];
            self.solve_checked(sk, prior)?;
            return Ok(());
        };
        let m = sk.point_xy(center).map_err(err)?;
        let Some(&major_end) = self.pending_pos.first() else {
            if (cur.at - m).length() <= tol {
                return Err(crate::msg!("sketch-major-axis-positive"));
            }
            self.pending_pos = vec![cur.at];
            self.pending_snaps = vec![cur.remember(tol)];
            return Ok(());
        };
        let major = major_end - m;
        let len = major.length();
        if len <= tol {
            return Err(crate::msg!("sketch-major-axis-positive"));
        }
        let minor = (major / len).perp_dot(cur.at - m).abs().min(len);
        if minor <= tol {
            return Err(crate::msg!("sketch-minor-axis-positive"));
        }
        let first = self
            .pending_snaps
            .first()
            .map(|&hit| DrawCursor::remembered(hit, sk));
        let ellipse = sk.add_ellipse(center, major, minor).map_err(err)?;
        for c in first.iter().chain([cur]) {
            self.bind_rim(sk, ellipse, c)?;
        }
        self.clear_pending();
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    fn click_spline(
        &mut self,
        sk: &mut Sketch,
        cur: &DrawCursor,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        if self.pending.len() >= MAX_SPLINE_POINTS {
            return Err(crate::msg!("sketch-spline-max", max = MAX_SPLINE_POINTS));
        }
        let p = self.attach(sk, None, cur)?;
        if self.pending.contains(&p) {
            return Err(crate::msg!("sketch-spline-point-twice"));
        }
        self.pending.push(p);
        self.solve_checked(sk, prior)?;
        Ok(())
    }

    /// Schließt das laufende Werkzeug ab: legt den Spline an bzw. beendet den Linienzug.
    pub fn finish_tool(&mut self, sk: &mut Sketch) -> Result<(), String> {
        if self.tool != Tool::Spline {
            self.message = None;
            self.clear_pending();
            return Ok(());
        }
        self.atomic(sk, |se, sk, prior| {
            if se.pending.len() < 2 {
                return Err(crate::msg!("sketch-spline-min"));
            }
            sk.add_spline(&se.pending).map_err(err)?;
            se.pending.clear();
            se.solve_checked(sk, prior)?;
            Ok(())
        })
    }

    /// Bricht das laufende Werkzeug ab; im Auswahlwerkzeug wird die Auswahl geleert. Ein
    /// laufendes Ziehen wird wie mit [`end_drag`](Self::end_drag) beendet (der Punkt bleibt,
    /// wo er ist). Stutzen, Dehnen und Brechen enden ganz (zurück zur Auswahl).
    pub fn cancel(&mut self, sk: &mut Sketch) {
        self.end_drag(sk);
        self.clear_pending();
        if self.tool == Tool::Select {
            self.selection.clear();
        }
        // Stutzen, Dehnen und Brechen haben nichts Offenes: Esc beendet sie wie in Fusion (auch
        // mitten im Ziehen; was schon gestutzt ist, bleibt).
        if self.tool.is_modify() {
            self.set_tool(sk, Tool::Select);
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
                self.message = (!status_ok(&report)).then(|| crate::msg!("sketch-drag-blocked"));
                self.last_report = Some(report);
            }
            Err(e) => self.message = Some(err(e)),
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
                        CurveKind::Ellipse { .. }
                        | CurveKind::EllipticArc { .. }
                        | CurveKind::Spline { .. } => s.others.push(c),
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
    fn build(&self, sk: &Sketch, action: ConstraintAction) -> Result<ConstraintKind, Msg> {
        use ConstraintAction as A;
        use ConstraintKind as K;
        if self.selection.is_empty() {
            return Err(crate::msg!("sketch-nothing-selected"));
        }
        if !self.available_actions(sk).contains(&action) {
            return Err(crate::msg!(
                "sketch-action-mismatch",
                action = action.label_msg()
            ));
        }
        let s = self
            .sorted(sk)
            .ok_or_else(|| crate::msg!("sketch-selection-invalid"))?;
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
                        return Err(crate::msg!("sketch-length-lines-only"));
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
                        .ok_or_else(|| crate::msg!("sketch-cannot-measure"))?,
                };
                kind = checked_value(sk, &kind, v)?;
            }
            let id = sk.add_constraint(kind).map_err(err)?;
            let before = se.last_report.take();
            let report = sk.solve();
            se.last_report = Some(report.clone());
            if let Some(problem) = check_change(&report, before.as_ref(), prior, Some(id)) {
                return Err(match problem {
                    SolveStatus::Conflicting => {
                        crate::msg!("sketch-apply-conflict", action = action.label_msg())
                    }
                    _ => crate::msg!("sketch-apply-unsolved", action = action.label_msg()),
                });
            }
            if report.redundant.contains(&id) {
                if action.is_dimension() {
                    sk.set_driven(id, true).map_err(err)?;
                    if let Some(v) = sk.constraint(id).and_then(|c| sk.measure(&c.kind)) {
                        sk.set_constraint_value(id, v).map_err(err)?;
                    }
                    se.last_report = Some(sk.solve());
                    se.message = Some(crate::msg!("sketch-became-driven"));
                } else {
                    se.message = Some(crate::msg!("sketch-redundant"));
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
                .ok_or_else(|| crate::msg!("sketch-constraint-missing"))?;
            checked_value(sk, &kind, value)?;
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
            None => Err(crate::msg!("sketch-constraint-missing")),
            Some(c) if !c.kind.is_dimension() => Err(crate::msg!("sketch-not-dimension")),
            Some(c) => {
                let driven = c.driven;
                // Ein anzeigendes Maß nimmt nur Zahlen; in Zoll hat auch eine Zahl einen
                // Speichertext („1 in“), der entfällt.
                let formula = matches!(
                    crate::formula_field::classify(text),
                    Some(crate::formula_field::FieldInput::Formula(_))
                );
                dimension_input(text, constraint_unit(&c.kind), scope).and_then(|(v, f)| {
                    if driven && formula {
                        Err(crate::msg!("sketch-driven-no-formula"))
                    } else if driven {
                        Ok((v, None))
                    } else {
                        Ok((v, f))
                    }
                })
            }
        };
        let (value, formula) = match read {
            Ok(r) => r,
            Err(e) => {
                let text = e.text();
                self.message = Some(e);
                return Err(text);
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
            Err(crate::msg!("sketch-not-dimension"))
        };
        let (value, formula) = match read {
            Ok(r) => r,
            Err(e) => {
                let text = e.text();
                self.message = Some(e);
                return Err(text);
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

    /// Neues Maß `action` aus der Auswahl mit dem Wert `value` (Skizzeneinheit, ohne Formel),
    /// sonst wie [`apply_dimension_input`](Self::apply_dimension_input). Für ein unverändert
    /// bestätigtes Maßfeld, dessen Text gerundet ist.
    pub fn apply_dimension_value(
        &mut self,
        sf: &mut SketchFeature,
        action: ConstraintAction,
        value: f64,
    ) -> Result<ConstraintId, String> {
        self.message = None;
        if !action.is_dimension() {
            let e = crate::msg!("sketch-not-dimension");
            let text = e.text();
            self.message = Some(e);
            return Err(text);
        }
        let id = self.apply(&mut sf.sketch, action, Some(value))?;
        sf.formulas.remove(&id);
        self.note_formula_problem(sf);
        Ok(id)
    }

    /// Lässt sich eine Skizze mit Formeln nach einer erlaubten Änderung nicht lösen (der Fehler
    /// bestand schon vorher), steht der Fehler, den sie beim Beenden bekommt (Plan P12), als
    /// Meldung da, sofern keine andere ansteht oder nur der allgemeine Hinweis.
    fn note_formula_problem(&mut self, sf: &SketchFeature) {
        if let Some(p) = formula_problem(sf, self.last_report.as_ref())
            && (self.message.is_none() || self.message == unsolved_hint(self.last_report.as_ref()))
        {
            self.message = Some(p);
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
                .ok_or_else(|| crate::msg!("sketch-constraint-missing"))?;
            if !kind.is_dimension() {
                return Err(crate::msg!("sketch-only-dimension-driven"));
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
                    SolveStatus::Conflicting => crate::msg!("sketch-change-conflict"),
                    _ => crate::msg!("sketch-change-unsolved"),
                });
            }
            if !driven && report.redundant.contains(&id) {
                return Err(crate::msg!("sketch-stays-driven"));
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

    /// Lässt sich „Konstruktion umschalten“ anwenden (mindestens eine Kurve ausgewählt)? Gilt
    /// für den Knopf im Skizzenfeld und seine Taste.
    pub fn can_toggle_construction(&self) -> bool {
        self.selection.iter().any(|i| matches!(i, Item::Curve(_)))
    }

    /// „Konstruktion umschalten“ wie Knopf und Taste: nur, wenn
    /// [`can_toggle_construction`](Self::can_toggle_construction) gilt, sonst nichts.
    pub fn toggle_construction_if_possible(&mut self, sk: &mut Sketch) {
        if self.can_toggle_construction() {
            self.toggle_construction(sk);
        }
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
                tr!("sketch-status-conflict", n = report.conflicting.len()),
                Severity::Error,
            ),
            SolveStatus::NotConverged => (tr!("sketch-status-unsolvable"), Severity::Error),
            SolveStatus::Solved => {
                let mut text = match report.dof {
                    0 => tr!("sketch-status-solved"),
                    1 => tr!("sketch-status-dof-one"),
                    n => tr!("sketch-status-dof", n = n),
                };
                if report.redundant.is_empty() {
                    (text, Severity::Info)
                } else {
                    text.push_str(&tr!("sketch-status-redundant", n = report.redundant.len()));
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
    checked_value(sk, kind, value).map_err(|m| m.text())
}

/// Wie [`with_checked_value`], mit der Meldung als Grund (zum Speichern).
fn checked_value(sk: &Sketch, kind: &ConstraintKind, value: f64) -> Result<ConstraintKind, Msg> {
    if !value.is_finite() {
        return Err(crate::msg!("sketch-enter-number"));
    }
    let changed = kind
        .with_value(value)
        .ok_or_else(|| crate::msg!("sketch-not-dimension"))?;
    sk.check_constraint_value(&changed).map_err(err)?;
    Ok(changed)
}
