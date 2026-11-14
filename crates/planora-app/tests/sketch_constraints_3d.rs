//! Abhängigkeiten, Maße, Fang und Ziehen an 3D-Geometrie in der App (Skizzen 10, Task 14): H/V an
//! einer 3D-Linie wählt die nächste Achse, gedämpfte Knöpfe ohne 3D-Fassung, Bemaßung an
//! 3D-Geometrie (ausgerichtet, waagerecht/senkrecht gesperrt), Ziehen parallel zum Bildschirm, Fang
//! an 3D-Punkten und Mitten samt Bindung, Zeichen an ihrer Lage im Raum, Schnittfang ohne
//! 3D-Kurven, Randbindung von Kreis und Ellipse, Ecken des 3D-Textrahmens. Bei 1920 × 1080.

mod common;

use eframe::App as _;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use glam::Vec3;
use planora_app::app::PlanoraApp;
use planora_app::keymap::defaults::Action;
use planora_app::keymap::{Area, Binding, ModKey};
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{ConstraintAction, Item, Tool, TriadPlane};
use planora_app::sketch_triad::{TriadPart, triad_hit};
use planora_app::sketch_view::{self, Mapper};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::SketchPlane;
use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::OrbitCamera;
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::dimension::{DimMode, DimPick};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::snap::{Snap, SnapIndex};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(W, H),
};

// ---- Prüfstand mit der Ansicht allein ----------------------------------------------------------

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    ctx
}

/// Schräger Blick, aus dem alle drei Ebenen gut zu sehen sind.
fn oblique() -> OrbitCamera {
    let mut cam = OrbitCamera {
        target: Vec3::new(0.0, 20.0, 15.0),
        distance: 220.0,
        ..Default::default()
    };
    cam.look_along(Vec3::new(1.0, -0.6, 0.8), Vec3::Z);
    cam.set_viewport(W, H);
    cam
}

/// Offene 3D-Skizze auf XY mit dem Werkzeug `tool`.
fn sketch_3d(tool: Tool) -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_three_d(sk, true);
        se.choose_tool(sk, tool);
    }
    s.observe_sketch(false);
    s.camera = oblique();
    s
}

struct View {
    ctx: egui::Context,
    s: AppState,
    time: f64,
}

impl View {
    fn new(tool: Tool) -> View {
        let mut v = View {
            ctx: context(),
            s: sketch_3d(tool),
            time: 0.0,
        };
        for _ in 0..3 {
            v.frame(vec![]);
        }
        v
    }

    fn with_kind(kind: ToolKind) -> View {
        let mut v = View::new(kind.tool());
        let (se, sk) = v.se_sk();
        se.choose_kind(sk, kind);
        v.frame(vec![]);
        v
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let s = &mut self.s;
        let mut gesture = false;
        let out = self.ctx.run_ui(raw, |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
            gesture = ui.input(|i| i.pointer.any_down());
        });
        out.drop_without_applying_deltas();
        self.s.observe_sketch(gesture);
    }

    fn map(&self) -> Mapper {
        let mut cam = self.s.camera;
        cam.set_viewport(W, H);
        Mapper {
            camera: cam,
            frame: *self.s.session_sketch().unwrap().frame(),
            rect: SCREEN,
        }
    }

    fn at3(&self, p: DVec3) -> Pos2 {
        self.map().to_screen3(p).expect("vor der Kamera")
    }

    fn hover(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p)]);
    }

    /// Klick an die Bildschirmstelle `p`; sie darf keinen bedienbaren Teil der Triade treffen.
    fn click(&mut self, p: Pos2) {
        self.time += 1.0;
        self.hover(p);
        if let Some(l) = self
            .s
            .sketch_session
            .as_ref()
            .unwrap()
            .triad_ui
            .layout
            .clone()
        {
            assert!(
                !matches!(
                    triad_hit(&l, p),
                    Some(TriadPart::Plane(_) | TriadPart::Handle(_))
                ),
                "der Klick träfe die Triade: {p:?}"
            );
        }
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    fn click3(&mut self, p: DVec3) {
        let at = self.at3(p);
        self.click(at);
    }

    /// Zug mit der linken Taste von `from` nach `to` in einigen Schritten.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.time += 1.0;
        self.hover(from);
        self.frame(vec![button(from, true)]);
        for k in 1..=8 {
            let t = k as f32 / 8.0;
            self.frame(vec![Event::PointerMoved(from + (to - from) * t)]);
        }
        self.frame(vec![Event::PointerMoved(to)]);
        self.frame(vec![button(to, false)]);
        self.frame(vec![]);
    }

    fn sk(&self) -> &Sketch {
        self.s.session_sketch().unwrap()
    }

    fn se(&self) -> &planora_app::sketch_state::SketchSession {
        self.s.sketch_session.as_ref().unwrap()
    }

    fn se_sk(&mut self) -> (&mut planora_app::sketch_state::SketchSession, &mut Sketch) {
        self.s.session_sketch_mut().unwrap()
    }

    fn set_plane(&mut self, plane: TriadPlane) {
        let (se, _) = self.se_sk();
        se.triad.active = plane;
        self.frame(vec![]);
    }

    fn xyz(&self, p: PointId) -> DVec3 {
        self.sk().point_xyz(p).unwrap()
    }

    /// Legt einen 3D-Punkt an (außerhalb jedes Werkzeugs, als Ausgangslage).
    fn point3(&mut self, p: DVec3) -> PointId {
        let id = self.se_sk().1.add_point3(p).unwrap();
        self.s.observe_sketch(false);
        self.frame(vec![]);
        id
    }

    /// Legt eine Linie zwischen zwei neuen 3D-Punkten an.
    fn line3(&mut self, a: DVec3, b: DVec3) -> (CurveId, PointId, PointId) {
        let (_, sk) = self.se_sk();
        let pa = sk.add_point3(a).unwrap();
        let pb = sk.add_point3(b).unwrap();
        let l = sk.add_line(pa, pb).unwrap();
        self.s.observe_sketch(false);
        self.frame(vec![]);
        (l, pa, pb)
    }
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn new_sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn near(a: DVec3, b: DVec3, tol: f64) -> bool {
    (a - b).length() < tol
}

fn has(sk: &Sketch, f: impl Fn(&ConstraintKind) -> bool) -> bool {
    sk.constraints().any(|(_, c)| f(&c.kind))
}

// ---- Abhängigkeiten ----------------------------------------------------------------------------

#[test]
fn hv_button_on_3d_line_picks_nearest_axis() {
    let mut s = sketch_3d(Tool::Select);
    let (se, sk) = s.session_sketch_mut().unwrap();
    // Fast entlang Z: Der Knopf „Waagerecht“ ergibt `AlongZ`.
    let a = sk.add_point3(DVec3::new(0.0, 0.0, 5.0)).unwrap();
    let b = sk.add_point3(DVec3::new(1.0, 2.0, 40.0)).unwrap();
    let z_line = sk.add_line(a, b).unwrap();
    se.selection = vec![Item::Curve(z_line)];
    se.constraint_button(sk, ConstraintAction::Horizontal)
        .unwrap();
    assert!(
        has(sk, |k| *k
            == ConstraintKind::AlongZ(LineOrPoints::Line(z_line))),
        "entlang Z"
    );
    let (pa, pb) = (sk.point_xyz(a).unwrap(), sk.point_xyz(b).unwrap());
    assert!((pa.x - pb.x).abs() < 1e-6 && (pa.y - pb.y).abs() < 1e-6);
    // Fast entlang Y: Auch „Waagerecht“ ergibt `Vertical`.
    let c = sk.add_point3(DVec3::new(10.0, 0.0, 10.0)).unwrap();
    let d = sk.add_point3(DVec3::new(12.0, 30.0, 12.0)).unwrap();
    let y_line = sk.add_line(c, d).unwrap();
    se.selection = vec![Item::Curve(y_line)];
    se.constraint_button(sk, ConstraintAction::Horizontal)
        .unwrap();
    assert!(has(sk, |k| *k
        == ConstraintKind::Vertical(LineOrPoints::Line(y_line))));
    // Zwei 3D-Punkte fast entlang X: „Senkrecht“ ergibt `Horizontal` zwischen den Punkten.
    let e = sk.add_point3(DVec3::new(0.0, 50.0, 20.0)).unwrap();
    let f = sk.add_point3(DVec3::new(30.0, 52.0, 21.0)).unwrap();
    se.selection = vec![Item::Point(e), Item::Point(f)];
    se.constraint_button(sk, ConstraintAction::Vertical)
        .unwrap();
    assert!(has(sk, |k| *k
        == ConstraintKind::Horizontal(LineOrPoints::Points(e, f))));
    // In 2D bleibt alles wie bisher.
    let g = sk.add_point(0.0, 80.0).unwrap();
    let h = sk.add_point(1.0, 120.0).unwrap();
    let flat = sk.add_line(g, h).unwrap();
    se.selection = vec![Item::Curve(flat)];
    se.constraint_button(sk, ConstraintAction::Horizontal)
        .unwrap();
    assert!(has(sk, |k| *k
        == ConstraintKind::Horizontal(LineOrPoints::Line(flat))));
}

#[test]
fn symmetric_button_dimmed_for_3d_selection() {
    for three_d in [true, false] {
        let mut r = Rig::new();
        let (p, q, l) = {
            let (_, sk) = r.st().session_sketch_mut().unwrap();
            let (p, q, a, b) = if three_d {
                (
                    sk.add_point3(DVec3::new(-10.0, 5.0, 10.0)).unwrap(),
                    sk.add_point3(DVec3::new(10.0, 5.0, 10.0)).unwrap(),
                    sk.add_point3(DVec3::new(0.0, 0.0, 10.0)).unwrap(),
                    sk.add_point3(DVec3::new(0.0, 20.0, 10.0)).unwrap(),
                )
            } else {
                (
                    sk.add_point(-10.0, 5.0).unwrap(),
                    sk.add_point(10.0, 5.0).unwrap(),
                    sk.add_point(0.0, 0.0).unwrap(),
                    sk.add_point(0.0, 20.0).unwrap(),
                )
            };
            (p, q, sk.add_line(a, b).unwrap())
        };
        r.st().observe_sketch(false);
        r.st().sketch_session.as_mut().unwrap().selection =
            vec![Item::Point(p), Item::Point(q), Item::Curve(l)];
        r.frame(vec![]);
        let nodes = r.frame(vec![]);
        let label = ConstraintAction::Symmetric.label();
        let dimmed: Vec<bool> = nodes
            .iter()
            .filter(|(n, _)| *n == label)
            .map(|(_, d)| *d)
            .collect();
        assert_eq!(dimmed, [three_d], "Symmetrisch gedämpft bei 3D: {three_d}");
        let st = r.st();
        let (se, sk) = st.session_sketch_mut().unwrap();
        assert_eq!(se.blocked_in_3d(sk, ConstraintAction::Symmetric), three_d);
        assert_eq!(
            se.available_actions(sk)
                .contains(&ConstraintAction::Symmetric),
            !three_d
        );
        // Parallel u. a. bleiben an 3D-Geometrie offen.
        assert!(!se.blocked_in_3d(sk, ConstraintAction::Parallel));
        if three_d {
            // Über eine Taste: nichts entsteht, der Hinweis nennt den Grund.
            let before = sk.constraints().count();
            let _ = se.constraint_button(sk, ConstraintAction::Symmetric);
            assert_eq!(sk.constraints().count(), before);
            assert_eq!(se.message_text(), Some(tr!("sketch-not-in-3d-hint")));
        }
    }
}

/// Der Abhängigkeitsbefehl „erst Befehl, dann Auswahl“ wählt 3D-Kurven über den
/// Bildschirmabstand.
#[test]
fn constraint_command_picks_a_3d_line() {
    let mut v = View::new(Tool::Select);
    let (l, ..) = v.line3(DVec3::new(10.0, 10.0, 5.0), DVec3::new(11.0, 12.0, 45.0));
    {
        let (se, sk) = v.se_sk();
        se.constraint_button(sk, ConstraintAction::Vertical)
            .unwrap();
        assert_eq!(se.tool, Tool::Constraint(ConstraintAction::Vertical));
    }
    v.frame(vec![]);
    v.click3(DVec3::new(10.5, 11.0, 25.0));
    assert!(
        has(v.sk(), |k| *k
            == ConstraintKind::AlongZ(LineOrPoints::Line(l))),
        "{:?}",
        v.sk()
            .constraints()
            .map(|(_, c)| c.kind.clone())
            .collect::<Vec<_>>()
    );
}

// ---- Maße --------------------------------------------------------------------------------------

#[test]
fn dimension_tool_on_3d_line_gives_distance() {
    let mut v = View::new(Tool::Dimension);
    let (a3, b3) = (DVec3::new(10.0, 10.0, 20.0), DVec3::new(40.0, 30.0, 50.0));
    let (l, a, b) = v.line3(a3, b3);
    let (sa, sb) = (v.at3(a3), v.at3(b3));
    let dir = (sb - sa).normalized();
    let mid = v.at3((a3 + b3) * 0.5);
    v.click(mid);
    assert_eq!(v.se().dim.picks.len(), 1, "die 3D-Linie ist gewählt");
    assert!(matches!(v.se().dim.picks[0], DimPick::Curve(c, _) if c == l));
    v.click(mid + vec2(-dir.y, dir.x) * 80.0);
    let plan = v.se().dim.plan.clone().expect("gesetztes Maß");
    let len = (b3 - a3).length();
    match plan.kind {
        ConstraintKind::Distance(p, q, value) => {
            assert_eq!((p, q), (a, b));
            assert!((value - len).abs() < 1e-6, "{value} gegen {len}");
        }
        k => panic!("Abstand im Raum erwartet: {k:?}"),
    }
    assert_eq!(plan.mode, DimMode::Aligned);
    let kind = plan.kind.clone();
    {
        let (se, sf) = v.s.session_draft_mut().unwrap();
        se.apply_planned_value(sf, kind, 50.0).unwrap();
    }
    v.frame(vec![]);
    assert!(((v.xyz(b) - v.xyz(a)).length() - 50.0).abs() < 1e-6);
    assert!(has(v.sk(), |k| matches!(k, ConstraintKind::Distance(..))));
    // Das Maß steht im Raum an der Linie: Sein Text liegt nahe am Bild der Linienmitte.
    let m = v.at3((v.xyz(a) + v.xyz(b)) * 0.5);
    let hits = sketch_view::label_hits(&v.ctx, v.se().sketch);
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0].1.min.distance(m + vec2(6.0, -18.0)) < 30.0,
        "{hits:?} gegen {m:?}"
    );
}

#[test]
fn dimension_horizontal_blocked_on_3d() {
    // Im Modell: Die Zeigerregel ergäbe waagerecht; an 3D-Punkten bleibt es ausgerichtet.
    let mut sk = new_sketch();
    let p = sk.add_point3(DVec3::new(0.0, 0.0, 10.0)).unwrap();
    let q = sk.add_point3(DVec3::new(30.0, 5.0, 10.0)).unwrap();
    let picks = [DimPick::Point(p), DimPick::Point(q)];
    let pointer = DVec2::new(15.0, 40.0);
    let plan = sk.dimension_plan(&picks, pointer).expect("Maß");
    assert_eq!(plan.mode, DimMode::Aligned);
    assert!(matches!(plan.kind, ConstraintKind::Distance(..)));
    assert!(sk.dimension_axis_blocked(&picks, pointer));
    assert!(
        sk.add_constraint(ConstraintKind::DistanceX(p, q, 30.0))
            .is_err()
    );
    // In 2D gilt die Zeigerregel wie bisher.
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(30.0, 5.0).unwrap();
    let flat = [DimPick::Point(a), DimPick::Point(b)];
    let plan = sk.dimension_plan(&flat, pointer).unwrap();
    assert_eq!(plan.mode, DimMode::Horizontal);
    assert!(!sk.dimension_axis_blocked(&flat, pointer));

    // In der App: Der Hinweis nennt den Grund.
    let mut v = View::new(Tool::Dimension);
    let (p3, q3) = (DVec3::new(0.0, 0.0, 10.0), DVec3::new(30.0, 5.0, 10.0));
    v.point3(p3);
    v.point3(q3);
    v.click3(p3);
    v.click3(q3);
    assert_eq!(v.se().dim.picks.len(), 2);
    v.click3(DVec3::new(15.0, 40.0, 0.0));
    let plan = v.se().dim.plan.clone().expect("gesetztes Maß");
    assert!(
        matches!(plan.kind, ConstraintKind::Distance(..)),
        "{plan:?}"
    );
    assert_eq!(
        v.se().message_text(),
        Some(tr!("sketch-dimension-3d-aligned"))
    );
}

// ---- Ziehen ------------------------------------------------------------------------------------

#[test]
fn drag_3d_point_moves_parallel_to_screen() {
    let mut v = View::new(Tool::Select);
    let old = DVec3::new(20.0, 20.0, 30.0);
    let p = v.point3(old);
    let n = v.map().view_normal().expect("Blickrichtung");
    let undo = v.s.sketch_history().unwrap().undo_len();
    let from = v.at3(old);
    let to = from + vec2(80.0, -40.0);
    v.drag(from, to);
    assert!(!v.se().is_dragging());
    let new = v.xyz(p);
    assert!(new.distance(old) > 1.0, "der Punkt zieht: {new:?}");
    assert!(
        (new - old).dot(n).abs() < 1e-6,
        "in der Ebene parallel zum Bildschirm: {new:?}"
    );
    assert!(v.at3(new).distance(to) < 1.0, "unter dem Zeiger");
    assert_eq!(
        v.s.sketch_history().unwrap().undo_len(),
        undo + 1,
        "ein Schritt"
    );

    // Eine 3D-Linie zieht ganz mit, beide Enden um dieselbe Verschiebung.
    let (a3, b3) = (DVec3::new(-20.0, 40.0, 10.0), DVec3::new(10.0, 60.0, 40.0));
    let (_, a, b) = v.line3(a3, b3);
    let from = v.at3((a3 + b3) * 0.5);
    v.drag(from, from + vec2(-60.0, 30.0));
    let (da, db) = (v.xyz(a) - a3, v.xyz(b) - b3);
    assert!(da.length() > 1.0, "{da:?}");
    assert!(near(da, db, 1e-6), "{da:?} gegen {db:?}");
    assert!(da.dot(n).abs() < 1e-6);
}

// ---- Fang --------------------------------------------------------------------------------------

#[test]
fn snap_to_3d_point_and_midpoint() {
    let mut v = View::new(Tool::Line);
    v.set_plane(TriadPlane::Yz);
    let p3 = DVec3::new(0.0, -40.0, 50.0);
    let p = v.point3(p3);
    let (a3, b3) = (DVec3::new(-40.0, 90.0, 10.0), DVec3::new(0.0, 90.0, 50.0));
    let (l, ..) = v.line3(a3, b3);
    let m3 = (a3 + b3) * 0.5;
    let at = v.at3(p3);
    v.hover(at);
    let cur = v.se().cursor3.expect("Zeiger in 3D");
    assert_eq!(cur.snap, Some(p), "Fang am 3D-Punkt");
    assert!(near(cur.at, p3, 1e-9));
    let at = v.at3(m3);
    v.hover(at);
    let cur = v.se().cursor3.expect("Zeiger in 3D");
    assert_eq!(cur.mid, Some(l), "Fang an der Mitte");
    assert!(near(cur.at, m3, 1e-9));

    v.click3(p3);
    v.click3(m3);
    let sk = v.sk();
    let line = sk
        .curves()
        .filter_map(|(c, d)| match d.kind {
            CurveKind::Line { a, b } if c != l => Some((a, b)),
            _ => None,
        })
        .next()
        .expect("neue Linie");
    assert_eq!(line.0, p, "beginnt am gefangenen Punkt");
    assert!(near(v.xyz(line.1), m3, 1e-6));
    assert!(has(sk, |k| *k == ConstraintKind::Midpoint(line.1, l)));
}

#[test]
fn snap_index_skips_3d_curves_in_intersections() {
    for three_d in [true, false] {
        let mut sk = new_sketch();
        let a = sk.add_point(-10.0, 3.0).unwrap();
        let b = sk.add_point(10.0, 3.0).unwrap();
        sk.add_line(a, b).unwrap();
        let (c, d) = if three_d {
            (
                sk.add_point3(DVec3::new(2.0, -10.0, 5.0)).unwrap(),
                sk.add_point3(DVec3::new(2.0, 10.0, 5.0)).unwrap(),
            )
        } else {
            (
                sk.add_point(2.0, -10.0).unwrap(),
                sk.add_point(2.0, 10.0).unwrap(),
            )
        };
        sk.add_line(c, d).unwrap();
        let hit = SnapIndex::build(&sk)
            .find(DVec2::new(2.0, 3.0), 0.5, &[])
            .expect("Fang");
        assert_eq!(
            matches!(hit.snap, Snap::Intersection(..)),
            !three_d,
            "{hit:?}"
        );
    }
}

#[test]
fn circle_2p_rim_binds_snapped_3d_point() {
    let mut v = View::with_kind(ToolKind::Circle2P);
    v.set_plane(TriadPlane::Yz);
    let q3 = DVec3::new(0.0, 10.0, 20.0);
    let q = v.point3(q3);
    v.click3(q3);
    v.click3(DVec3::new(0.0, 40.0, 20.0));
    let sk = v.sk();
    let (c, _) = sk
        .curves()
        .find(|(_, d)| matches!(d.kind, CurveKind::Circle { .. }))
        .expect("Kreis");
    assert!(has(sk, |k| *k == ConstraintKind::PointOnCurve(q, c)));
}

#[test]
fn ellipse_rim_binds_snapped_3d_point() {
    let mut v = View::new(Tool::Ellipse);
    v.set_plane(TriadPlane::Yz);
    let q3 = DVec3::new(0.0, 70.0, 20.0);
    let q = v.point3(q3);
    v.click3(DVec3::new(0.0, 30.0, 20.0));
    v.click3(q3);
    v.click3(DVec3::new(0.0, 30.0, 35.0));
    let sk = v.sk();
    let (e, _) = sk
        .curves()
        .find(|(_, d)| matches!(d.kind, CurveKind::Ellipse { .. }))
        .expect("Ellipse");
    assert!(has(sk, |k| *k == ConstraintKind::PointOnCurve(q, e)));
}

/// Tippt `content` und drückt OK (mit den mitgelieferten Schriften).
fn type_and_ok(v: &mut View, content: &str) {
    let lib = planora_text::FontLibrary::bundled_only();
    let (se, sf) = v.s.session_draft_mut().unwrap();
    se.text.as_mut().expect("Werkzeug Text").params.content = content.into();
    se.confirm_text(sf, Some(&lib)).unwrap();
    v.frame(vec![]);
}

#[test]
fn text_3d_corner_snaps_bind_and_glyph_points_are_not_picked() {
    let mut v = View::new(Tool::Text);
    v.set_plane(TriadPlane::Yz);
    let q3 = DVec3::new(0.0, 40.0, 10.0);
    let q = v.point3(q3);
    v.click3(q3);
    v.click3(DVec3::new(0.0, 80.0, 30.0));
    type_and_ok(&mut v, "Hi");
    let sk = v.sk();
    let (_, d) = sk.texts().next().expect("Text");
    let corners = match d.placement {
        planora_sketch::text::TextPlacement::Box { corners, .. } => corners,
        _ => panic!("Rahmen erwartet"),
    };
    let corner = corners
        .iter()
        .copied()
        .find(|&c| near(sk.point_xyz(c).unwrap(), q3, 1e-6))
        .expect("Ecke am Punkt");
    assert!(
        has(sk, |k| *k == ConstraintKind::Coincident(corner, q)
            || *k == ConstraintKind::Coincident(q, corner)),
        "Ecke an den gefangenen Punkt gebunden"
    );

    // Die Punkte der Buchstaben trifft die Wahl in 3D nicht (wie in 2D).
    v.s.sketch_session.as_mut().unwrap().tool = Tool::Select;
    v.frame(vec![]);
    let glyphs = v.sk().glyph_points();
    assert!(!glyphs.is_empty());
    let map = v.map();
    for &g in &glyphs {
        let at = v.at3(v.xyz(g));
        let hit = sketch_view::pick_3d(v.se(), v.sk(), &map, at);
        assert!(
            !matches!(hit, Some(Item::Point(p)) if glyphs.contains(&p)),
            "Buchstabenpunkt getroffen: {hit:?}"
        );
    }
}

/// Review I1: Im Zeichenweg in der Ebene (Triade auf der Skizzenebene) fängt ein hoher 3D-Punkt
/// nicht an seiner Lage in der Draufsicht, wo auf dem Bildschirm nichts ist. Die Linie bleibt 2D,
/// das Profil bleibt. Sein sichtbares Bild fängt dagegen.
#[test]
fn flat_drawing_ignores_the_top_view_of_3d_points() {
    let mut v = View::new(Tool::Line);
    assert!(!v.se().draws_in_3d(v.sk()), "Zeichnen in der Ebene");
    let high = DVec3::new(30.0, 30.0, 80.0);
    let h = v.point3(high);
    let below = DVec3::new(30.0, 30.0, 0.0);
    assert!(
        v.at3(below).distance(v.at3(high)) > 100.0,
        "auf dem Bildschirm weit weg"
    );
    v.hover(v.at3(below));
    let cur = v.se().draw_cursor.clone().expect("Zeiger");
    assert_ne!(cur.snap, Snap::Point(h), "kein Fang in der Draufsicht");
    for p in [
        below,
        DVec3::new(70.0, 30.0, 0.0),
        DVec3::new(50.0, 60.0, 0.0),
        below,
    ] {
        v.click3(p);
    }
    let sk = v.sk();
    let lines: Vec<(PointId, PointId)> = sk
        .curves()
        .filter_map(|(_, d)| match d.kind {
            CurveKind::Line { a, b } => Some((a, b)),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(
        lines
            .iter()
            .all(|&(a, b)| !sk.is_3d_point(a) && !sk.is_3d_point(b) && a != h && b != h),
        "die Linien bleiben 2D"
    );
    assert_eq!(sk.profiles().profiles.len(), 1, "das Profil bleibt");

    // Das sichtbare Bild des 3D-Punkts fängt.
    let at = v.at3(high);
    v.hover(at);
    let cur = v.se().draw_cursor.clone().expect("Zeiger");
    assert_eq!(cur.snap, Snap::Point(h), "Fang am sichtbaren 3D-Punkt");
}

/// Review M1: Trifft der Sehstrahl die Skizzenebene nicht, wählt der Klick die 3D-Geometrie allein,
/// ohne eine Lage (0, 0) anzunehmen; ein 2D-Punkt im Ursprung schlägt sie nicht.
#[test]
fn a_click_off_the_sketch_plane_picks_only_3d_geometry() {
    let mut v = View::new(Tool::Dimension);
    {
        let (_, sk) = v.se_sk();
        sk.add_point(0.0, 0.0).unwrap();
    }
    let (a3, b3) = (DVec3::new(-20.0, 0.0, 70.0), DVec3::new(20.0, 0.0, 90.0));
    let (l, ..) = v.line3(a3, b3);
    // Waagerechter Blick entlang +Y von unterhalb der Linie: Über dem Horizont trifft kein
    // Sehstrahl die Ebene z = 0.
    let mut cam = OrbitCamera {
        target: Vec3::new(0.0, 0.0, 50.0),
        distance: 200.0,
        ..Default::default()
    };
    cam.look_along(Vec3::Y, Vec3::Z);
    cam.set_viewport(W, H);
    v.s.camera = cam;
    v.frame(vec![]);
    let mid = v.at3((a3 + b3) * 0.5);
    let map = v.map();
    assert!(
        sketch_view::screen_to_sketch(&map.camera, &map.frame, egui_vec(SIZE), glam_vec(mid))
            .is_none(),
        "die Linie liegt über dem Horizont"
    );
    v.click(mid);
    assert_eq!(v.se().dim.picks.len(), 1, "{:?}", v.se().dim.picks);
    assert!(
        matches!(v.se().dim.picks[0], DimPick::Curve(c, _) if c == l),
        "die 3D-Linie, nicht der Punkt im Ursprung: {:?}",
        v.se().dim.picks
    );
    // Ein zweiter Klick über dem Horizont neben der Linie setzt das Maß im Raum.
    v.click(mid + vec2(0.0, -80.0));
    let plan = v.se().dim.plan.clone();
    assert!(
        plan.is_none(),
        "ohne Treffer setzt der Klick ohne Ebene nichts: {plan:?}"
    );
    // Auf der Ebene (unter dem Horizont) setzt er es.
    v.click(pos2(W / 2.0, H - 40.0));
    let plan = v.se().dim.plan.clone().expect("gesetztes Maß");
    assert!(
        matches!(plan.kind, ConstraintKind::Distance(..)),
        "{plan:?}"
    );

    // Abhängigkeitsbefehl ebenso: H an der Linie über dem Horizont.
    let mut v2 = View::new(Tool::Select);
    {
        let (_, sk) = v2.se_sk();
        sk.add_point(0.0, 0.0).unwrap();
    }
    let (l2, ..) = v2.line3(a3, b3);
    v2.s.camera = cam;
    {
        let (se, sk) = v2.se_sk();
        se.constraint_button(sk, ConstraintAction::Horizontal)
            .unwrap();
    }
    v2.frame(vec![]);
    let mid = v2.at3((a3 + b3) * 0.5);
    v2.click(mid);
    assert!(
        has(v2.sk(), |k| *k
            == ConstraintKind::Horizontal(LineOrPoints::Line(l2))),
        "{:?}",
        v2.sk()
            .constraints()
            .map(|(_, c)| c.kind.clone())
            .collect::<Vec<_>>()
    );
}

fn egui_vec(v: egui::Vec2) -> glam::Vec2 {
    glam::Vec2::new(v.x, v.y)
}

fn glam_vec(p: Pos2) -> glam::Vec2 {
    glam::Vec2::new(p.x, p.y)
}

// ---- Zeichen im Raum ---------------------------------------------------------------------------

#[test]
fn annotation_anchors_of_3d_geometry_lie_in_space() {
    let mut sk = new_sketch();
    let a = sk.add_point3(DVec3::new(0.0, 0.0, 10.0)).unwrap();
    let b = sk.add_point3(DVec3::new(0.0, 0.0, 50.0)).unwrap();
    let l = sk.add_line(a, b).unwrap();
    let at = sketch_view::constraint_anchors(&sk, &ConstraintKind::AlongZ(LineOrPoints::Line(l)));
    assert_eq!(at.len(), 1);
    assert!(near(at[0], DVec3::new(0.0, 0.0, 30.0), 1e-9), "{at:?}");
    let at = sketch_view::constraint_anchors(&sk, &ConstraintKind::Distance(a, b, 40.0));
    assert!(near(at[0], DVec3::new(0.0, 0.0, 30.0), 1e-9), "{at:?}");
    let at = sketch_view::constraint_anchors(&sk, &ConstraintKind::Fixed3(b, 0.0, 0.0, 50.0));
    assert!(near(at[0], DVec3::new(0.0, 0.0, 50.0), 1e-9), "{at:?}");
}

// ---- Tasten ------------------------------------------------------------------------------------

/// Carried item 7: Alt allein und Pfeil hoch belegen im Skizzenbereich (und überall) nur die
/// Aktionen der Triade.
#[test]
fn only_triad_actions_use_alt_alone_and_arrow_up() {
    let triad = [
        Action::SketchTriadCyclePlane,
        Action::SketchTriadCycleHandle,
        Action::SketchLockZ,
    ];
    for &action in Action::ALL {
        if !matches!(action.area(), Area::Sketch | Area::All) || triad.contains(&action) {
            continue;
        }
        for b in action.default_bindings() {
            let clash = match b {
                Binding::Modifier(ModKey::Alt) => true,
                Binding::Key(k) => k.key == egui::Key::ArrowUp && !(k.ctrl || k.shift || k.alt),
                _ => false,
            };
            assert!(!clash, "{action:?} belegt {b:?}");
        }
    }
}

// ---- In der App (Skizzenfeld) ------------------------------------------------------------------

const SIZE: egui::Vec2 = vec2(1920.0, 1080.0);

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new() -> Rig {
        let root = common::temp_root("planora-sketch-constraints-3d");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let ctx = context();
        PlanoraApp::setup_context(&ctx);
        ctx.enable_accesskit();
        let mut app = PlanoraApp::for_tests(w);
        {
            let s = &mut app.ws_mut().active_tab_mut().state;
            let id = s
                .add_sketch(SketchPlane::Standard(StandardPlane::XY))
                .unwrap();
            s.open_sketch(id).unwrap();
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.set_three_d(sk, true);
        }
        let mut r = Rig {
            ctx,
            app,
            time: 0.0,
        };
        for _ in 0..3 {
            r.frame(vec![]);
        }
        r
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    /// Ein Bild; liefert Beschriftung und „gesperrt“ der Knoten für Bildschirmleser.
    fn frame(&mut self, events: Vec<Event>) -> Vec<(String, bool)> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut nodes = Vec::new();
        if let Some(update) = out.platform_output.accesskit_update.take() {
            for (_, node) in &update.nodes {
                if let Some(label) = node.label() {
                    nodes.push((label.to_string(), node.is_disabled()));
                }
            }
        }
        out.drop_without_applying_deltas();
        nodes
    }
}
