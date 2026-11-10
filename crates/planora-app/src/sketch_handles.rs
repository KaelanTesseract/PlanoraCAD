//! Griffe am Bildschirm (Skizzen 7, Spec B6, B7, D3, D4): was zu sehen ist, was sich greifen lässt,
//! der erste Zug an einem gedämpften Griff, der Rho-Griff und die Befehle aus dem Skizzenfeld. Die
//! Rechnung bleibt im Modell (`planora_sketch::spline_handles`); hier steht nur, was die Auswahl
//! braucht, denn das Skizzenmodell kennt keine Auswahl.
//!
//! Ein Griffzug läuft als [`CurveDrag`], ohne eigenen Weg durch das Ziehen: Die Ziele rechnen vom
//! Start aus, nichts springt auf den Zeiger. Dazu kommen die `anchors`, die stehen bleiben (der
//! Stützpunkt, wenn seine Spitze gezogen wird), und `before`, der Stand vor dem Einschalten, auf den
//! ein Abbruch zurückgeht.

use crate::msg::Msg;
use crate::sketch_state::{CurveDrag, Item, SketchSession, Tool, err};
use planora_math::DVec2;
use planora_sketch::SketchError;
use planora_sketch::model::{CurveId, CurveKind, HandleCurvature, PointId, Sketch, SplineHandle};
use planora_sketch::solver::{Drag, SolveReport};
use planora_sketch::{conic_rho_at, conic_rho_ok, conic_shoulder};
use std::collections::HashSet;

/// Bereich des Rho-Griffs (D4). Getippt geht mehr, siehe [`conic_rho_ok`].
const RHO_GRIP_RANGE: (f64, f64) = (0.001, 0.999);

/// Bis zu diesem Anteil des Zeigerwegs gilt ein Stützpunkt als stehen geblieben ([`carry_frame`]).
const NOT_MOVED: f64 = 1e-4;

/// Welcher Griff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlePart {
    /// Die Griff-Linie; `tip` ist ihre Spitze.
    Tangent,
    /// Der Griff-Bogen; `tip` ist sein freies Ende, gedämpft die Marke auf der Normalen.
    Curvature,
}

/// Ein Griff, der gerade zu sehen ist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandleView {
    pub spline: CurveId,
    /// Der Stützpunkt.
    pub point: PointId,
    pub part: HandlePart,
    pub at: DVec2,
    pub tip: DVec2,
    /// Gedämpft: Noch nicht eingeschaltet, der erste Zug schaltet ihn ein.
    pub ghost: bool,
}

/// Griff-Kurven und Griff-Punkte, die gerade unsichtbar sind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HiddenHandles {
    pub curves: HashSet<CurveId>,
    pub points: HashSet<PointId>,
}

/// Griff unter dem Zeiger, der noch nicht als Kurve oder Punkt in der Skizze steht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HandleGrip {
    Ghost(HandleView),
    Rho(CurveId),
}

/// Laufender Zug am Rho-Griff: Rho unter dem Zeiger beim Greifen minus Rho der Kurve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RhoGrab {
    pub(crate) curve: CurveId,
    offset: f64,
}

/// Rho des Kegelschnitts `c`.
pub(crate) fn conic_rho(sk: &Sketch, c: CurveId) -> Option<f64> {
    match sk.curve(c)?.kind {
        CurveKind::Conic { rho, .. } => Some(rho),
        _ => None,
    }
}

/// Die drei Lagen des Kegelschnitts `c` in der Folge Start, Scheitel, Ende, und sein Rho.
fn conic_frame(sk: &Sketch, c: CurveId) -> Option<(DVec2, DVec2, DVec2, f64)> {
    let CurveKind::Conic {
        start,
        end,
        apex,
        rho,
    } = sk.curve(c)?.kind
    else {
        return None;
    };
    Some((
        sk.point_xy(start).ok()?,
        sk.point_xy(apex).ok()?,
        sk.point_xy(end).ok()?,
        rho,
    ))
}

/// Ein Bild des Zugs am Rho-Griff: das Rho zum Zeiger `at`, begrenzt auf den Griffbereich, und das
/// Lösen, damit Abhängigkeiten an der Kurve folgen.
pub(crate) fn rho_frame(
    sk: &mut Sketch,
    grab: RhoGrab,
    at: DVec2,
) -> Result<SolveReport, SketchError> {
    let (p0, a, p2, _) = conic_frame(sk, grab.curve).ok_or(SketchError::UnknownObject)?;
    if let Some(raw) = conic_rho_at(p0, a, p2, at) {
        let rho = (raw - grab.offset).clamp(RHO_GRIP_RANGE.0, RHO_GRIP_RANGE.1);
        sk.set_conic_rho(grab.curve, rho)?;
    }
    Ok(sk.solve())
}

/// Ein Bild des Zugs am Stützpunkt mit Griff: Griffspitze und Krümmungsbogen gehen starr mit (Ruling
/// Task 6), aber nur um das, was der Stützpunkt wirklich schafft. Ein fester Punkt bleibt, und der Griff
/// verbiegt die Kurve nicht. Darum zuerst der Punkt allein auf einer Kopie, dann alle um seinen Weg.
/// `start[0]` ist der Stützpunkt.
pub(crate) fn carry_frame(
    sk: &mut Sketch,
    start: &[(PointId, DVec2)],
    delta: DVec2,
) -> Result<SolveReport, SketchError> {
    let (fit, from) = start[0];
    let mut trial = sk.clone();
    let probe = trial.drag(&[Drag {
        point: fit,
        target: from + delta,
    }])?;
    if probe.drag_failed {
        return Ok(probe);
    }
    let made = trial.point_xy(fit)? - from;
    // Der Probelauf hält einen Punkt, der nicht folgen kann, nur bis auf die Genauigkeit des Zugs. Das
    // gilt als „bewegt sich nicht“, sonst würde daraus eine Form- und eine Schrittänderung.
    let made = if made.length() <= NOT_MOVED * delta.length() {
        DVec2::ZERO
    } else {
        made
    };
    let drags: Vec<Drag> = start
        .iter()
        .map(|&(point, at)| Drag {
            point,
            target: at + made,
        })
        .collect();
    sk.drag(&drags)
}

/// Die Griff-Kurven eines Griffs.
fn parts_of(h: &SplineHandle) -> Vec<CurveId> {
    match h.curvature {
        HandleCurvature::Arc { arc, .. } => vec![h.line, arc],
        _ => vec![h.line],
    }
}

/// Die Punkte eines Griffs ohne den Stützpunkt.
fn points_of(h: &SplineHandle) -> Vec<PointId> {
    match h.curvature {
        HandleCurvature::Arc { center, other, .. } => vec![h.tip, center, other],
        _ => vec![h.tip],
    }
}

/// Ein Spline und was die Auswahl für seine Griffe heißt. Hier steht die eine Regel aus B6.
struct SplineView<'a> {
    spline: CurveId,
    handles: &'a [SplineHandle],
    /// Die Auswahl betrifft ihn: er selbst, ein Griff oder ein Stützpunkt.
    related: bool,
}

impl SplineView<'_> {
    /// Ist der aktive Griff `h` zu sehen?
    fn shows(&self, sk: &Sketch, h: &SplineHandle) -> bool {
        self.related || constrained(sk, h)
    }
}

/// Alle Splines der Skizze, gesehen von der Auswahl `selection`.
fn spline_views<'a>(sk: &'a Sketch, selection: &'a [Item]) -> impl Iterator<Item = SplineView<'a>> {
    sk.curves().filter_map(move |(spline, data)| {
        let CurveKind::Spline { points, handles } = &data.kind else {
            return None;
        };
        let related = selection.iter().any(|item| match *item {
            Item::Curve(c) => c == spline || handles.iter().any(|h| parts_of(h).contains(&c)),
            Item::Point(p) => {
                points.contains(&p) || handles.iter().any(|h| points_of(h).contains(&p))
            }
            Item::Constraint(_) => false,
        });
        Some(SplineView {
            spline,
            handles,
            related,
        })
    })
}

/// Hat der Griff eine Abhängigkeit oder ein Maß des Nutzers? Die interne Zeile des Bogens zählt nicht.
/// Eine ausgeschaltete oder nur anzeigende zählt mit: Ihr Zeichen steht weiter in der Zeichnung, und
/// der Griff muss zu sehen und wählbar bleiben, damit man sie wiederfindet.
fn constrained(sk: &Sketch, h: &SplineHandle) -> bool {
    let (curves, points) = (parts_of(h), points_of(h));
    sk.constraints().any(|(_, d)| {
        !d.internal
            && (d.kind.curves().iter().any(|c| curves.contains(c))
                || d.kind.points().iter().any(|p| points.contains(p)))
    })
}

/// Was bei dieser Auswahl unsichtbar ist. Eine fertige Skizze hat keine Auswahl.
pub fn hidden_handles_of(sk: &Sketch, selection: &[Item]) -> HiddenHandles {
    let mut out = HiddenHandles::default();
    for view in spline_views(sk, selection) {
        for h in view.handles.iter().filter(|h| !view.shows(sk, h)) {
            out.curves.extend(parts_of(h));
            out.points.extend(points_of(h));
        }
    }
    if !out.points.is_empty() {
        // Benutzt eine andere Kurve den Punkt, ist er ein gewöhnlicher Punkt.
        let only = sk.handle_only_points();
        out.points.retain(|p| only.contains(p));
    }
    out
}

impl SketchSession {
    /// Die Griffe, die gerade zu sehen sind (B6). Aktive: Ist der Spline, ein Griff oder sein
    /// Stützpunkt gewählt, alle seine; mit einer Abhängigkeit oder einem Maß des Nutzers immer.
    /// Gedämpfte nur im Auswahlwerkzeug, denn nur dort geht der erste Zug: an jedem Stützpunkt des
    /// gewählten Splines ohne Griff, und auf der Normalen an jedem Tangentengriff ohne Bogen.
    pub fn visible_handles(&self, sk: &Sketch) -> Vec<HandleView> {
        let mut out = Vec::new();
        let select_tool = self.tool == Tool::Select;
        for view in spline_views(sk, &self.selection) {
            let spline = view.spline;
            let xy = |p: PointId| sk.point_xy(p).ok();
            for h in view.handles.iter().filter(|h| view.shows(sk, h)) {
                let (Some(at), Some(tip)) = (xy(h.at), xy(h.tip)) else {
                    continue;
                };
                let view = |part, tip| HandleView {
                    spline,
                    point: h.at,
                    part,
                    at,
                    tip,
                    ghost: false,
                };
                out.push(view(HandlePart::Tangent, tip));
                if let HandleCurvature::Arc { other, .. } = h.curvature
                    && let Some(end) = xy(other)
                {
                    out.push(view(HandlePart::Curvature, end));
                }
            }
            if !select_tool {
                continue;
            }
            let ghost = |part, (p, tip): (PointId, DVec2)| {
                xy(p).map(|at| HandleView {
                    spline,
                    point: p,
                    part,
                    at,
                    tip,
                    ghost: true,
                })
            };
            if view.related {
                out.extend(
                    sk.handle_curvature_marks(spline)
                        .into_iter()
                        .filter_map(|m| ghost(HandlePart::Curvature, m)),
                );
            }
            if self.selection.contains(&Item::Curve(spline)) {
                out.extend(
                    sk.handle_ghost_tips(spline)
                        .into_iter()
                        .filter_map(|m| ghost(HandlePart::Tangent, m)),
                );
            }
        }
        out
    }

    /// Die Punkte der aktiven Griffe, die zu sehen sind: die Liste für den Fang.
    pub fn visible_handle_points(&self, sk: &Sketch) -> Vec<PointId> {
        let mut out = Vec::new();
        for view in spline_views(sk, &self.selection) {
            for h in view.handles.iter().filter(|h| view.shows(sk, h)) {
                out.extend(points_of(h));
            }
        }
        out
    }

    /// Was bei der aktuellen Auswahl unsichtbar ist: Es wird nicht gezeichnet und nicht getroffen.
    pub fn hidden_handles(&self, sk: &Sketch) -> HiddenHandles {
        hidden_handles_of(sk, &self.selection)
    }

    /// Der nächste Punkt im Radius, ohne unsichtbare Griffe.
    pub(crate) fn shown_point(&self, sk: &Sketch, at: DVec2, radius: f64) -> Option<PointId> {
        sk.pick_point_skipping(at, radius, &self.hidden_handles(sk).points)
    }

    /// Die nächste Kurve im Radius, ohne unsichtbare Griffe.
    pub(crate) fn shown_curve(&self, sk: &Sketch, at: DVec2, radius: f64) -> Option<CurveId> {
        sk.pick_curve_skipping(at, radius, &self.hidden_handles(sk).curves)
    }

    /// Griffe auf den Schulterpunkten der gewählten Kegelschnitte (D4), im Auswahlwerkzeug.
    pub fn rho_grips(&self, sk: &Sketch) -> Vec<(CurveId, DVec2)> {
        if self.tool != Tool::Select {
            return Vec::new();
        }
        self.selection
            .iter()
            .filter_map(|item| {
                let Item::Curve(c) = *item else {
                    return None;
                };
                let (p0, a, p2, rho) = conic_frame(sk, c)?;
                Some((c, conic_shoulder(p0, a, p2, rho)))
            })
            .collect()
    }

    /// Der gedämpfte Griff oder Rho-Griff bei `at`, wenn ihn kein Punkt verdeckt: Der nächste gewinnt,
    /// bei Gleichstand der Punkt.
    pub(crate) fn handle_grip_at(&self, sk: &Sketch, at: DVec2, radius: f64) -> Option<HandleGrip> {
        if self.tool != Tool::Select || !at.is_finite() {
            return None;
        }
        let ghosts = self.visible_handles(sk).into_iter().filter(|h| h.ghost);
        let grips = ghosts
            .map(|h| (h.tip, HandleGrip::Ghost(h)))
            .chain(
                self.rho_grips(sk)
                    .into_iter()
                    .map(|(c, q)| (q, HandleGrip::Rho(c))),
            )
            .map(|(q, grip)| (q.distance(at), grip))
            .filter(|(d, _)| *d <= radius)
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        let point = self
            .shown_point(sk, at, radius)
            .and_then(|p| sk.point_xy(p).ok())
            .map(|q| q.distance(at));
        if point.is_some_and(|d| d <= grips.0) {
            return None;
        }
        Some(grips.1)
    }

    /// Wird der Punkt `p` gerade gezogen? Beim Griffzug die bewegten Punkte, nicht die gehaltenen.
    pub fn is_dragged_point(&self, p: PointId) -> bool {
        self.dragging == Some(p)
            || self
                .curve_drag
                .as_ref()
                .is_some_and(|d| d.before.is_some() && d.start.iter().any(|&(moved, _)| moved == p))
    }

    /// Läuft ein Griffzug, den ein Abbruch ganz zurücknimmt?
    pub fn handle_drag_running(&self) -> bool {
        self.curve_drag.as_ref().is_some_and(|d| d.before.is_some())
    }

    /// Nimmt den laufenden Griffzug ganz zurück, auch das Einschalten im ersten Zug: Es entsteht kein
    /// Schritt (Esc, rechte Taste, Rückgängig; der Werkzeugwechsel nur beim ersten Zug, siehe
    /// [`set_tool`](SketchSession::set_tool)). `false`, wenn keiner läuft. Hat mitten im Zug
    /// noch etwas anderes die Skizze geändert, gibt es den Stand nicht mehr; dann endet der Zug wie
    /// jeder andere.
    pub fn abort_handle_drag(&mut self, sk: &mut Sketch) -> bool {
        let Some(mut drag) = self.curve_drag.take_if(|d| d.before.is_some()) else {
            return false;
        };
        drag.note_foreign_change(sk);
        let Some(before) = drag.before.take() else {
            self.curve_drag = Some(drag);
            return false;
        };
        *sk = before;
        self.last_report = Some(sk.clone().solve());
        self.message = None;
        self.edit_label = None;
        self.box_aborted = true;
        true
    }

    /// Greift in `begin_drag` einen gedämpften Griff oder den Rho-Griff. `false`, wenn dort keiner ist.
    pub(crate) fn begin_grip_drag(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) -> bool {
        match self.handle_grip_at(sk, at, radius) {
            None => false,
            Some(HandleGrip::Ghost(h)) => {
                self.begin_first_drag(sk, h, at);
                true
            }
            Some(HandleGrip::Rho(c)) => {
                self.begin_rho_drag(sk, c, at);
                true
            }
        }
    }

    /// Erster Zug: schaltet den Griff in einem Zug mit dem Ziehen ein. Lehnt das Modell ab, steht der
    /// Grund in `message`, und es läuft kein Zug.
    fn begin_first_drag(&mut self, sk: &mut Sketch, h: HandleView, at: DVec2) {
        let before = sk.clone();
        let done = self.atomic(sk, |se, sk, prior| {
            match h.part {
                HandlePart::Tangent => {
                    sk.activate_tangent_handle(h.spline, h.point).map_err(err)?;
                }
                HandlePart::Curvature => {
                    sk.activate_curvature_handle(h.spline, h.point)
                        .map_err(err)?;
                }
            }
            se.solve_checked(sk, prior)?;
            Ok(())
        });
        if done.is_err() {
            return;
        }
        let Some((_, active)) = sk
            .handles_at(h.point)
            .into_iter()
            .find(|(c, _)| *c == h.spline)
        else {
            return;
        };
        let (moved, anchors, label) = match (h.part, active.curvature) {
            (HandlePart::Curvature, HandleCurvature::Arc { other, .. }) => (
                other,
                vec![active.at, active.tip],
                crate::msg!("undo-handle-curvature"),
            ),
            _ => (
                active.tip,
                vec![active.at],
                crate::msg!("undo-handle-tangent"),
            ),
        };
        // An der Krümmungsmarke gilt das freie Bogenende als gegriffen: Es folgt dem Zeiger wie danach
        // beim Zug am aktiven Griff. Die Marke liegt nicht auf dem Bogenende; vom Greifpunkt aus
        // gerechnet landete das Ende um diesen Abstand neben dem Zeiger (zweiter Handtest Skizzen 7).
        // Die gedämpfte Spitze ist dagegen schon die Spitze, dort bleibt der Greifpunkt.
        let grab = match h.part {
            HandlePart::Curvature => sk.point_xy(moved).unwrap_or(at),
            HandlePart::Tangent => at,
        };
        self.start_handle_grab(sk, before, grab, &[moved], &anchors, Some(label));
        if let Some(drag) = self.curve_drag.as_mut() {
            drag.activates = true;
        }
    }

    /// Greift den Rho-Griff des Kegelschnitts `c`.
    fn begin_rho_drag(&mut self, sk: &Sketch, c: CurveId, at: DVec2) {
        let Some((p0, a, p2, rho)) = conic_frame(sk, c) else {
            return;
        };
        let Some(raw) = conic_rho_at(p0, a, p2, at) else {
            return;
        };
        let mut drag = CurveDrag::new(sk, at, Vec::new());
        drag.rho = Some(RhoGrab {
            curve: c,
            offset: raw - rho,
        });
        drag.label = Some(crate::msg!("undo-conic-rho"));
        drag.before = Some(sk.clone());
        self.curve_drag = Some(drag);
    }

    /// Zug am Punkt `p`, wenn er zu einem Griff gehört. An der Spitze, der Bogenmitte oder dem freien
    /// Bogenende bleibt stehen, was davor liegt. Am Stützpunkt gehen Griffspitze und Krümmungsbogen
    /// starr mit (Ruling Task 6). `false`: Der Punkt hat keinen Griff, es ist ein gewöhnlicher Zug.
    pub(crate) fn begin_handle_point(&mut self, sk: &Sketch, p: PointId, at: DVec2) -> bool {
        if let Some((_, h)) = sk.handle_of_point(p) {
            let anchors = if p == h.tip {
                vec![h.at]
            } else {
                vec![h.at, h.tip]
            };
            let label = Some(crate::msg!("undo-handle-drag"));
            self.start_handle_grab(sk, sk.clone(), at, &[p], &anchors, label);
            return true;
        }
        let carried: Vec<PointId> = sk
            .handles_at(p)
            .iter()
            .flat_map(|(_, h)| points_of(h))
            .collect();
        if carried.is_empty() {
            return false;
        }
        let mut moved = vec![p];
        moved.extend(carried);
        self.start_handle_grab(sk, sk.clone(), at, &moved, &[], None);
        if let Some(drag) = self.curve_drag.as_mut() {
            drag.carry = true;
        }
        true
    }

    /// Zug an einer Griff-Kurve: Die Linie zieht ihre Spitze, der Bogen sein freies Ende; der
    /// Stützpunkt bleibt. `false`: `c` ist kein Griff.
    pub(crate) fn begin_handle_curve(&mut self, sk: &Sketch, c: CurveId, at: DVec2) -> bool {
        let Some((_, h)) = sk.handle_of_curve(c) else {
            return false;
        };
        let (moved, anchors) = match h.curvature {
            HandleCurvature::Arc { arc, other, .. } if arc == c => (other, vec![h.at, h.tip]),
            _ => (h.tip, vec![h.at]),
        };
        let label = Some(crate::msg!("undo-handle-drag"));
        self.start_handle_grab(sk, sk.clone(), at, &[moved], &anchors, label);
        true
    }

    fn start_handle_grab(
        &mut self,
        sk: &Sketch,
        before: Sketch,
        at: DVec2,
        moved: &[PointId],
        anchors: &[PointId],
        label: Option<Msg>,
    ) {
        let start = moved
            .iter()
            .filter_map(|&p| sk.point_xy(p).ok().map(|q| (p, q)))
            .collect();
        let mut drag = CurveDrag::new(sk, at, start);
        drag.anchors = anchors.to_vec();
        drag.label = label;
        // Löst das Einschalten ein Muster auf, sagt es die Meldung am Ende des Zugs.
        drag.count_patterns_of(&before);
        drag.before = Some(before);
        self.curve_drag = Some(drag);
    }

    /// Spline und Stützpunkt, an denen die Griff-Knöpfe wirken: der gewählte Stützpunkt (liegt er auf
    /// mehreren Splines, zuerst ein gewählter), oder der Griff, zu dem ein gewählter Griff-Teil gehört.
    pub fn handle_target(&self, sk: &Sketch) -> Option<(CurveId, PointId)> {
        let picked = |c: &CurveId| self.selection.contains(&Item::Curve(*c));
        self.selection.iter().find_map(|item| match *item {
            Item::Point(p) => sk
                .handle_of_point(p)
                .map(|(c, h)| (c, h.at))
                .or_else(|| {
                    let mut on: Vec<CurveId> = sk
                        .curves()
                        .filter(|(_, d)| {
                            matches!(&d.kind, CurveKind::Spline { points, .. } if points.contains(&p))
                        })
                        .map(|(c, _)| c)
                        .collect();
                    on.sort_by_key(|c| !picked(c));
                    on.first().map(|&c| (c, p))
                }),
            Item::Curve(c) => sk.handle_of_curve(c).map(|(s, h)| (s, h.at)),
            Item::Constraint(_) => None,
        })
    }

    /// Ist am Griffziel der Tangentengriff und der Krümmungsgriff an?
    pub fn handle_state(&self, sk: &Sketch, c: CurveId, at: PointId) -> (bool, bool) {
        let h = sk.handles_at(at).into_iter().find(|(s, _)| *s == c);
        (
            h.is_some(),
            h.is_some_and(|(_, h)| matches!(h.curvature, HandleCurvature::Arc { .. })),
        )
    }

    fn require_handle_target(&mut self, sk: &Sketch) -> Result<(CurveId, PointId), String> {
        self.handle_target(sk).ok_or_else(|| {
            let e = crate::msg!("sketch-handle-no-point");
            let text = e.text();
            self.message = Some(e);
            text
        })
    }

    /// Knopf und Aktion „Tangentengriff“ (B7): schaltet ihn am gewählten Stützpunkt ein oder aus, ein
    /// Rückgängig-Schritt.
    pub fn toggle_tangent_handle(&mut self, sk: &mut Sketch) -> Result<(), String> {
        let (c, at) = self.require_handle_target(sk)?;
        let on = self.handle_state(sk, c, at).0;
        self.atomic(sk, |se, sk, prior| {
            if on {
                sk.deactivate_handle(c, at).map_err(err)?;
            } else {
                sk.activate_tangent_handle(c, at).map_err(err)?;
            }
            se.solve_checked(sk, prior)?;
            se.edit_label = Some(if on {
                crate::msg!("undo-handle-off")
            } else {
                crate::msg!("undo-handle-tangent")
            });
            Ok(())
        })?;
        self.forget_removed(sk);
        Ok(())
    }

    /// Knopf und Aktion „Krümmungsgriff“ (B7): schaltet ihn ein (fehlt der Tangentengriff, kommt er
    /// mit) oder nur ihn aus. Die Ablehnungen des Modells stehen als Text in `message`, und die Skizze
    /// bleibt, wie sie ist.
    pub fn toggle_curvature_handle(&mut self, sk: &mut Sketch) -> Result<(), String> {
        let (c, at) = self.require_handle_target(sk)?;
        let on = self.handle_state(sk, c, at).1;
        self.atomic(sk, |se, sk, prior| {
            if on {
                sk.deactivate_curvature_handle(c, at).map_err(err)?;
            } else {
                sk.activate_curvature_handle(c, at).map_err(err)?;
            }
            se.solve_checked(sk, prior)?;
            se.edit_label = Some(if on {
                crate::msg!("undo-handle-off")
            } else {
                crate::msg!("undo-handle-curvature")
            });
            Ok(())
        })?;
        self.forget_removed(sk);
        Ok(())
    }

    /// Rho aus dem Feld: außerhalb des Bereichs abgelehnt mit Meldung, sonst ein Rückgängig-Schritt.
    /// Gehört die Kurve zu einem Muster, löst es sich auf, und die Meldung sagt es.
    pub fn set_conic_rho(&mut self, sk: &mut Sketch, c: CurveId, rho: f64) -> Result<(), String> {
        self.atomic(sk, |se, sk, prior| {
            if !conic_rho_ok(rho) {
                return Err(crate::msg!("sketch-err-conic-rho"));
            }
            sk.set_conic_rho(c, rho).map_err(err)?;
            se.solve_checked(sk, prior)?;
            se.edit_label = Some(crate::msg!("undo-conic-rho"));
            Ok(())
        })
    }

    /// Die eine gewählte Kurve; `None` ohne oder bei mehreren.
    fn single_curve(&self) -> Option<CurveId> {
        let mut curves = self.selection.iter().filter_map(|item| match *item {
            Item::Curve(c) => Some(c),
            _ => None,
        });
        let c = curves.next()?;
        curves.next().is_none().then_some(c)
    }

    /// Der eine gewählte Spline mit Kontrollpunkten und sein wirksamer Grad.
    pub fn selected_control_spline(&self, sk: &Sketch) -> Option<(CurveId, u8)> {
        let c = self.single_curve()?;
        match sk.curve(c)?.kind {
            CurveKind::ControlSpline { degree, .. } => Some((c, degree)),
            _ => None,
        }
    }

    /// Der eine gewählte Kegelschnitt und sein Rho.
    pub fn selected_conic(&self, sk: &Sketch) -> Option<(CurveId, f64)> {
        let c = self.single_curve()?;
        conic_rho(sk, c).map(|rho| (c, rho))
    }

    /// Ändert den Grad des Splines mit Kontrollpunkten (D3), ein Rückgängig-Schritt. Was danach nicht
    /// mehr hält, entfällt mit Meldung. Derselbe wirksame Grad ändert nichts.
    pub fn set_spline_degree(
        &mut self,
        sk: &mut Sketch,
        c: CurveId,
        degree: u8,
    ) -> Result<(), String> {
        let same = matches!(
            sk.curve(c).map(|d| &d.kind),
            Some(CurveKind::ControlSpline { degree: now, .. }) if *now == degree
        );
        if same {
            return Ok(());
        }
        self.atomic(sk, |se, sk, prior| {
            let dropped = sk.set_control_spline_degree(c, degree).map_err(err)?;
            se.solve_checked(sk, prior)?;
            if dropped > 0 {
                se.add_note(crate::msg!("sketch-move-dropped", n = dropped));
            }
            se.edit_label = Some(crate::msg!("undo-spline-degree"));
            Ok(())
        })?;
        self.forget_removed(sk);
        Ok(())
    }

    /// Aktion `skizze.spline_grad`: am gewählten Spline von Grad 5 (oder wirksam 4) auf 3, sonst auf 5.
    pub fn cycle_spline_degree(&mut self, sk: &mut Sketch) -> Result<(), String> {
        let Some((c, degree)) = self.selected_control_spline(sk) else {
            let e = crate::msg!("sketch-spline-degree-no-selection");
            let text = e.text();
            self.message = Some(e);
            return Err(text);
        };
        self.set_spline_degree(sk, c, if degree >= 4 { 3 } else { 5 })
    }

    /// Klick von „Kontrollpunkt einfügen“: Der Knoten kommt an die Kurvenstelle nächst dem Klick. Nur
    /// Splines mit Kontrollpunkten zählen; daneben geschieht nichts. Die Kurve bleibt gewählt, das
    /// Werkzeug bleibt für den nächsten Punkt.
    pub(crate) fn click_insert_control_point(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        radius: f64,
        prior: &Sketch,
    ) -> Result<(), Msg> {
        let others: HashSet<CurveId> = sk
            .curves()
            .filter(|(_, d)| !matches!(d.kind, CurveKind::ControlSpline { .. }))
            .map(|(c, _)| c)
            .collect();
        let Some(c) = sk.pick_curve_skipping(at, radius, &others) else {
            return Ok(());
        };
        let (_, dropped) = sk.insert_control_point(c, at).map_err(err)?;
        self.solve_checked(sk, prior)?;
        if dropped > 0 {
            self.add_note(crate::msg!("sketch-move-dropped", n = dropped));
        }
        self.edit_label = Some(crate::msg!("undo-control-point-insert"));
        Ok(())
    }
}
