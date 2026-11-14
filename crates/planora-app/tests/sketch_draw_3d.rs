//! Zeichnen in 3D (Skizzen 10, Task 11): Linie, Spline und Punkt in der Ebene der Triade, Achsfang,
//! Z-Sperre, getippte Länge und Winkel, Esc und Rückgängig, gedämpfte 2D-Arten in der Liste der
//! Arten sowie die Korrekturen an der Triade (Klick auf einen streifenden Drehbogen, Winkelfeld
//! ohne Fokusraub, Esc und Rückgängig beenden den Zug). Bei 1920 × 1080.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use glam::Vec3;
use planora_app::app::PlanoraApp;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_panel;
use planora_app::sketch_state::{Tool, TriadAxis, TriadPlane};
use planora_app::sketch_triad::{ARC_FRAC, TriadPart, triad_hit};
use planora_app::sketch_view::{self, Mapper};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::SketchPlane;
use planora_math::{DQuat, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::OrbitCamera;
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

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

/// Schräger Blick, aus dem alle drei Ebenen der Triade gut zu sehen sind.
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

    /// Bildschirmlage eines Punkts in Skizzenkoordinaten.
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
        if let Some(l) = self.layout() {
            assert!(
                !matches!(
                    triad_hit(&l, p),
                    Some(TriadPart::Plane(_) | TriadPart::Handle(_))
                ),
                "der Klick träfe die Triade: {p:?}"
            );
        }
        self.frame(vec![button(p, PointerButton::Primary, true)]);
        self.frame(vec![button(p, PointerButton::Primary, false)]);
        self.frame(vec![]);
    }

    fn click3(&mut self, p: DVec3) {
        let at = self.at3(p);
        self.click(at);
    }

    fn key(&mut self, k: Key) {
        self.frame(vec![key_down(k, Modifiers::NONE)]);
        self.frame(vec![key_up(k)]);
        self.frame(vec![]);
    }

    fn layout(&self) -> Option<planora_app::sketch_triad::TriadLayout> {
        self.s.sketch_session.as_ref()?.triad_ui.layout.clone()
    }

    fn sk(&self) -> &Sketch {
        self.s.session_sketch().unwrap()
    }

    fn se_sk(&mut self) -> (&mut planora_app::sketch_state::SketchSession, &mut Sketch) {
        self.s.session_sketch_mut().unwrap()
    }

    fn set_plane(&mut self, plane: TriadPlane) {
        let (se, _) = self.se_sk();
        se.triad.active = plane;
        self.frame(vec![]);
    }

    fn set_lock(&mut self, on: bool) {
        self.s.sketch_session.as_mut().unwrap().triad.lock_z = on;
    }

    fn points(&self) -> Vec<PointId> {
        self.sk().points().map(|(p, _)| p).collect()
    }

    fn lines(&self) -> Vec<(CurveId, PointId, PointId)> {
        self.sk()
            .curves()
            .filter(|(_, d)| !d.construction)
            .filter_map(|(c, d)| match d.kind {
                CurveKind::Line { a, b } => Some((c, a, b)),
                _ => None,
            })
            .collect()
    }

    fn xyz(&self, p: PointId) -> DVec3 {
        self.sk().point_xyz(p).unwrap()
    }
}

fn button(p: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn key_down(k: Key, mods: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: mods,
    }
}

fn key_up(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn near(a: DVec3, b: DVec3, tol: f64) -> bool {
    (a - b).length() < tol
}

fn has(sk: &Sketch, f: impl Fn(&ConstraintKind) -> bool) -> bool {
    sk.constraints().any(|(_, c)| f(&c.kind))
}

// ---- Linie --------------------------------------------------------------------------------------

#[test]
fn line_in_yz_plane_makes_3d_points() {
    let mut v = View::new(Tool::Line);
    v.set_plane(TriadPlane::Yz);
    let (a, b) = (DVec3::new(0.0, 10.0, 20.0), DVec3::new(0.0, 32.0, 6.0));
    v.click3(a);
    v.click3(b);
    let lines = v.lines();
    assert_eq!(lines.len(), 1, "eine Linie");
    let (_, pa, pb) = lines[0];
    for (p, want) in [(pa, a), (pb, b)] {
        assert!(v.sk().is_3d_point(p), "3D-Punkt");
        let got = v.xyz(p);
        assert!(got.x.abs() < 1e-9, "in der Ebene YZ: {got:?}");
        assert!(near(got, want, 0.05), "{got:?} statt {want:?}");
    }
    assert_eq!(v.points().len(), 2);
}

#[test]
fn line_on_sketch_plane_stays_2d() {
    let mut v = View::new(Tool::Line);
    let (a, b) = (DVec3::new(40.0, 10.0, 0.0), DVec3::new(70.0, 30.0, 0.0));
    v.click3(a);
    v.click3(b);
    let lines = v.lines();
    assert_eq!(lines.len(), 1);
    let (_, pa, pb) = lines[0];
    for (p, want) in [(pa, a), (pb, b)] {
        assert!(!v.sk().is_3d_point(p), "2D-Punkt");
        assert!(near(v.xyz(p), want, 0.05));
    }
    // Die Klicklage als 3D-Punkt gegeben: auf der Skizzenebene mit unverdrehter Triade 2D.
    let (se, sk) = v.se_sk();
    se.choose_tool(sk, Tool::Point);
    let p = se
        .place_point_3d(sk, DVec3::new(-7.0, 3.0, 0.0), None)
        .unwrap();
    assert!(!sk.is_3d_point(p));
    // Neben der Skizzenebene oder bei gedrehter Triade ein 3D-Punkt.
    let q = se
        .place_point_3d(sk, DVec3::new(-7.0, 9.0, 4.0), None)
        .unwrap();
    assert!(sk.is_3d_point(q));
    // Punkte freier Kurven: nur die Lage zählt, nicht die Triade (Fix-Runde 1).
    se.triad.active = TriadPlane::Yz;
    let r = se
        .place_point_3d(sk, DVec3::new(0.0, 12.0, 0.0), None)
        .unwrap();
    assert!(!sk.is_3d_point(r));
}

/// Fix-Runde 1: Bei gedrehter Triade bleiben Linien mit beiden Enden auf z = 0 2D und ergeben ein
/// Profil.
#[test]
fn rotated_triad_lines_on_sketch_plane_stay_2d_and_make_a_profile() {
    let mut v = View::new(Tool::Line);
    {
        let (se, _) = v.se_sk();
        se.triad.rotate(TriadAxis::Z, 0.4);
        assert!(!se.triad.is_sketch_plane());
    }
    v.frame(vec![]);
    for p in [
        DVec3::new(40.0, 10.0, 0.0),
        DVec3::new(75.0, 25.0, 0.0),
        DVec3::new(50.0, 50.0, 0.0),
    ] {
        v.click3(p);
        assert!(v.s.sketch_session.as_ref().unwrap().draw3, "3D-Weg");
    }
    // Zurück auf den ersten Punkt: der Fang schließt das Dreieck.
    let first = v.xyz(v.points()[0]);
    v.click3(first);
    v.key(Key::Escape);
    assert_eq!(v.lines().len(), 3);
    assert_eq!(v.points().len(), 3);
    for p in v.points() {
        assert!(!v.sk().is_3d_point(p), "2D-Punkt");
    }
    assert_eq!(
        v.sk().profiles().outlines.len(),
        1,
        "das Dreieck ist ein Profil"
    );
}

/// Fix-Runde 1: Wechselt die Linie zwischen Ebene und 3D, gilt ein gesperrter Winkel nicht weiter.
#[test]
fn switching_between_2d_and_3d_clears_locked_fields() {
    use planora_app::sketch_input::FieldKind;
    let mut v = View::new(Tool::Line);
    v.click3(DVec3::new(40.0, 10.0, 0.0));
    let aim = v.at3(DVec3::new(60.0, 25.0, 0.0));
    v.hover(aim);
    v.frame(vec![key_down(Key::Tab, Modifiers::NONE)]);
    v.frame(vec![key_up(Key::Tab)]);
    v.frame(vec![Event::Text("30".into())]);
    v.hover(aim);
    let angle = |v: &View| {
        v.s.sketch_session
            .as_ref()
            .unwrap()
            .input
            .fields
            .iter()
            .find(|f| f.kind == FieldKind::Angle)
            .and_then(|f| f.locked)
    };
    assert!(angle(&v).is_some(), "in der Ebene gesperrt");
    assert!(!v.s.sketch_session.as_ref().unwrap().draw3);
    v.set_plane(TriadPlane::Yz);
    v.hover(aim);
    assert!(v.s.sketch_session.as_ref().unwrap().draw3);
    assert_eq!(angle(&v), None, "in 3D nicht mehr gesperrt");
}

#[test]
fn triad_follows_last_point() {
    let mut v = View::new(Tool::Line);
    v.set_plane(TriadPlane::Yz);
    let a = DVec3::new(0.0, 10.0, 20.0);
    v.click3(a);
    let p = v.points()[0];
    let t = v.s.sketch_session.as_ref().unwrap().triad;
    assert_eq!(t.origin, v.xyz(p));
    // Auch auf der Skizzenebene (2D-Weg) springt sie mit.
    let mut v = View::new(Tool::Point);
    v.click3(DVec3::new(40.0, -8.0, 0.0));
    let p = v.points()[0];
    let t = v.s.sketch_session.as_ref().unwrap().triad;
    assert_eq!(t.origin, v.xyz(p));
    assert!(t.is_sketch_plane());
}

#[test]
fn axis_snap_adds_along_z() {
    let mut v = View::new(Tool::Line);
    v.set_plane(TriadPlane::Yz);
    let a = DVec3::new(0.0, 10.0, 20.0);
    v.click3(a);
    let start = v.xyz(v.points()[0]);
    // 4 Bildpunkte neben dem Bild der Z-Achse durch den Anfang.
    let along = v.at3(start + DVec3::new(0.0, 0.0, 25.0)) + vec2(4.0, 0.0);
    v.click(along);
    let lines = v.lines();
    assert_eq!(lines.len(), 1);
    let (line, pa, pb) = lines[0];
    let (s, e) = (v.xyz(pa), v.xyz(pb));
    assert_eq!((e.x, e.y), (s.x, s.y), "genau über dem Anfang: {s:?} {e:?}");
    assert!(e.z > s.z + 10.0);
    assert!(
        has(v.sk(), |k| *k
            == ConstraintKind::AlongZ(LineOrPoints::Line(line))),
        "AlongZ an der Linie"
    );
    // Die Y-Achse der Triade wird waagerecht? Nein: Y der Skizze heißt senkrecht.
    let y = v.at3(e + DVec3::new(0.0, 20.0, 0.0)) + vec2(0.0, 4.0);
    v.click(y);
    let second = v
        .lines()
        .into_iter()
        .find(|l| l.0 != line)
        .expect("zweite Linie")
        .0;
    assert!(has(v.sk(), |k| *k
        == ConstraintKind::Vertical(LineOrPoints::Line(second))));
    // Gedrehte Triade (nicht um Vielfache von 90°): kein Bezug zu einer Skizzenachse.
    let (se, sk) = v.se_sk();
    se.cancel(sk);
    se.triad.rotate(TriadAxis::X, 0.3);
    let n = sk.constraints().count();
    let o = se.triad.origin;
    let z = se.triad.axis(TriadAxis::Z);
    se.place_point_3d(sk, o, None).unwrap();
    se.place_point_3d(sk, o + z * 15.0, Some(TriadAxis::Z))
        .unwrap();
    let added: Vec<_> = sk
        .constraints()
        .skip(n)
        .map(|(_, c)| c.kind.clone())
        .collect();
    assert!(
        added.iter().all(|k| !matches!(
            k,
            ConstraintKind::AlongZ(_) | ConstraintKind::Horizontal(_) | ConstraintKind::Vertical(_)
        )),
        "{added:?}"
    );
}

#[test]
fn lock_z_line_end_above_start() {
    let mut v = View::new(Tool::Line);
    let a = DVec3::new(10.0, 12.0, 0.0);
    v.click3(a);
    let start = v.points()[0];
    assert!(!v.sk().is_3d_point(start));
    // Pfeil hoch über dem Anfang, dann nach oben.
    let at = v.at3(v.xyz(start));
    v.hover(at);
    v.set_lock(true);
    let up = v.at3(v.xyz(start) + DVec3::new(0.0, 0.0, 30.0)) + vec2(6.0, 0.0);
    v.hover(up);
    // Die Höhe zeigt ein Feld.
    let fields: Vec<_> =
        v.s.sketch_session
            .as_ref()
            .unwrap()
            .input
            .fields
            .iter()
            .map(|f| f.kind)
            .collect();
    assert_eq!(
        fields,
        vec![planora_app::sketch_input::FieldKind::ZDistance]
    );
    v.click(up);
    v.set_lock(false);
    v.frame(vec![]);
    let lines = v.lines();
    assert_eq!(lines.len(), 1);
    let (line, pa, pb) = lines[0];
    let (s, e) = (v.xyz(pa), v.xyz(pb));
    assert_eq!((e.x, e.y), (s.x, s.y), "genau über dem Anfang");
    assert!((e.z - 30.0).abs() < 1.0, "{e:?}");
    assert!(v.sk().is_3d_point(pb));
    assert!(has(v.sk(), |k| *k
        == ConstraintKind::AlongZ(LineOrPoints::Line(line))));
}

// ---- Spline -------------------------------------------------------------------------------------

#[test]
fn spline_fit_and_control_3d() {
    for kind in [ToolKind::SplineFit, ToolKind::SplineControl] {
        let mut v = View::new(Tool::Spline);
        {
            let (se, sk) = v.se_sk();
            se.choose_kind(sk, kind);
        }
        v.set_plane(TriadPlane::Yz);
        for p in [
            DVec3::new(0.0, 8.0, 22.0),
            DVec3::new(0.0, 26.0, 30.0),
            DVec3::new(0.0, 40.0, 12.0),
        ] {
            v.click3(p);
        }
        v.key(Key::Enter);
        let curves: Vec<_> = v.sk().curves().map(|(_, d)| d.kind.clone()).collect();
        assert_eq!(curves.len(), 1, "{kind:?}: {curves:?}");
        let pts = match (&curves[0], kind) {
            (CurveKind::Spline { points, .. }, ToolKind::SplineFit) => points.clone(),
            (CurveKind::ControlSpline { points, .. }, ToolKind::SplineControl) => points.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(pts.len(), 3);
        assert!(pts.iter().all(|&p| v.sk().is_3d_point(p)));
        assert!(pts.iter().all(|&p| v.xyz(p).x.abs() < 1e-9));
    }
}

// ---- Abbruch und Rückgängig ---------------------------------------------------------------------

#[test]
fn esc_leaves_nothing() {
    let mut v = View::new(Tool::Line);
    v.set_plane(TriadPlane::Yz);
    let count = |sk: &Sketch| {
        (
            sk.points().count(),
            sk.curves().count(),
            sk.constraints().count(),
        )
    };
    let before = count(v.sk());
    v.click3(DVec3::new(0.0, 10.0, 20.0));
    assert_eq!(v.points().len(), 1);
    v.key(Key::Escape);
    assert_eq!(v.points().len(), 0);
    assert_eq!(count(v.sk()), before);
    // Spline mit zwei Punkten: Esc lässt nichts.
    let (se, sk) = v.se_sk();
    se.choose_tool(sk, Tool::Spline);
    v.frame(vec![]);
    v.click3(DVec3::new(0.0, 8.0, 22.0));
    v.click3(DVec3::new(0.0, 26.0, 30.0));
    assert_eq!(v.points().len(), 2);
    v.key(Key::Escape);
    assert_eq!(v.points().len(), 0);
    assert_eq!(v.sk().curves().count(), 0);
    // Der Verlauf hat nichts davon gesehen.
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), 1, "nur der Haken");
}

#[test]
fn one_line_is_one_undo_step() {
    let mut v = View::new(Tool::Line);
    v.set_plane(TriadPlane::Yz);
    let n = v.s.sketch_history().unwrap().undo_len();
    v.click3(DVec3::new(0.0, 10.0, 20.0));
    assert_eq!(
        v.s.sketch_history().unwrap().undo_len(),
        n,
        "der erste Klick wartet"
    );
    v.click3(DVec3::new(0.0, 32.0, 6.0));
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), n + 1);
    v.key(Key::Escape);
    assert!(v.s.undo_sketch());
    assert_eq!(v.points().len(), 0);
    assert_eq!(v.lines().len(), 0);
}

// ---- Felder -------------------------------------------------------------------------------------

#[test]
fn typed_length_and_angle_make_3d_dimensions() {
    let mut v = View::new(Tool::Line);
    v.set_plane(TriadPlane::Yz);
    v.click3(DVec3::new(0.0, 10.0, 20.0));
    let start = v.xyz(v.points()[0]);
    // Zeiger schräg nach oben rechts in der Ebene YZ.
    let aim = v.at3(start + DVec3::new(0.0, 20.0, 12.0));
    v.hover(aim);
    let fields: Vec<_> =
        v.s.sketch_session
            .as_ref()
            .unwrap()
            .input
            .fields
            .iter()
            .map(|f| f.kind)
            .collect();
    use planora_app::sketch_input::FieldKind;
    assert_eq!(fields, vec![FieldKind::Length, FieldKind::Angle]);
    v.frame(vec![Event::Text("30".into())]);
    v.frame(vec![key_down(Key::Tab, Modifiers::NONE)]);
    v.frame(vec![key_up(Key::Tab)]);
    v.frame(vec![Event::Text("45".into())]);
    v.hover(aim);
    v.frame(vec![key_down(Key::Enter, Modifiers::NONE)]);
    v.frame(vec![key_up(Key::Enter)]);
    v.frame(vec![]);
    let lines = v.lines();
    assert_eq!(
        lines.len(),
        1,
        "{:?}",
        v.s.sketch_session.as_ref().unwrap().message
    );
    let (line, pa, pb) = lines[0];
    let (s, e) = (v.xyz(pa), v.xyz(pb));
    let d = e - s;
    assert!(e.x.abs() < 1e-9 && v.sk().is_3d_point(pb), "{e:?}");
    assert!((d.length() - 30.0).abs() < 1e-6, "{d:?}");
    let angle = d.y.atan2(d.z).to_degrees();
    assert!((d.z.atan2(d.y).to_degrees() - 45.0).abs() < 1e-6, "{angle}");
    assert!(has(v.sk(), |k| matches!(k,
        ConstraintKind::Distance(x, y, l) if [*x, *y] == [pa, pb] && (*l - 30.0).abs() < 1e-9)));
    assert!(has(v.sk(), |k| matches!(k,
        ConstraintKind::Angle(_, l, a) if *l == line && (a.to_degrees() - 45.0).abs() < 1e-9)));
    assert_eq!(
        v.sk().clone().solve().status,
        planora_sketch::solver::SolveStatus::Solved
    );
}

// ---- In der App ---------------------------------------------------------------------------------

const SIZE: egui::Vec2 = vec2(1920.0, 1080.0);

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new(three_d: bool, tool: Tool) -> Rig {
        let root = common::temp_root("planora-sketch-draw-3d");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let ctx = context();
        PlanoraApp::setup_context(&ctx);
        let mut app = PlanoraApp::for_tests(w);
        {
            let s = &mut app.ws_mut().active_tab_mut().state;
            let id = s
                .add_sketch(SketchPlane::Standard(StandardPlane::XY))
                .unwrap();
            s.open_sketch(id).unwrap();
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.set_three_d(sk, three_d);
            se.choose_tool(sk, tool);
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

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        self.ctx
            .run_ui(raw, |ui| app.ui(ui, &mut f))
            .drop_without_applying_deltas();
    }

    fn click(&mut self, at: Pos2, b: PointerButton) {
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![Event::PointerMoved(at), button(at, b, true)]);
        self.frame(vec![button(at, b, false)]);
        self.frame(vec![]);
    }

    fn layout(&mut self) -> planora_app::sketch_triad::TriadLayout {
        self.st()
            .sketch_session
            .as_ref()
            .unwrap()
            .triad_ui
            .layout
            .clone()
            .expect("Triade zu sehen")
    }

    fn points(&mut self) -> usize {
        self.st().session_sketch().unwrap().points().count()
    }

    fn strip(&self, label: &str) -> Pos2 {
        sketch_panel::strip_buttons(&self.ctx)
            .into_iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("Werkzeug „{label}“ fehlt"))
            .1
            .center()
    }
}

/// Alle Texte des letzten Bilds mit ihren Farben (Galley-Abschnitte und Ersatzfarbe).
fn texts_with_colors(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Vec<egui::Color32>)> {
    fn walk(shape: &egui::epaint::Shape, out: &mut Vec<(String, Vec<egui::Color32>)>) {
        match shape {
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
            egui::epaint::Shape::Text(t) => {
                let mut colors: Vec<egui::Color32> = t
                    .galley
                    .job
                    .sections
                    .iter()
                    .map(|s| s.format.color)
                    .collect();
                colors.push(t.fallback_color);
                colors.extend(t.override_text_color);
                out.push((t.galley.text().to_string(), colors));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for s in shapes {
        walk(&s.shape, &mut out);
    }
    out
}

/// Bei sichtbarer Triade ist kein Eintrag in den Listen der Arten gedämpft: Alle Arten zeichnen
/// in der Ebene der Triade (Task H2).
#[test]
fn no_kind_dimmed_when_triad_visible() {
    let mut r = Rig::new(true, Tool::Line);
    for tool in [
        Tool::Arc,
        Tool::Rectangle,
        Tool::Circle,
        Tool::Polygon,
        Tool::Slot,
        Tool::Spline,
    ] {
        let at = r.strip(&sketch_panel::tool_label(tool));
        r.click(at, PointerButton::Secondary);
        assert!(sketch_panel::flyout_open(&r.ctx), "{tool:?}");
        r.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(r.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut r.app;
        let out = r.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let texts = texts_with_colors(&out.shapes);
        out.drop_without_applying_deltas();
        let weak = r.ctx.global_style().visuals.weak_text_color();
        for kind in ToolKind::of(tool) {
            let label = kind.label();
            let found: Vec<&Vec<egui::Color32>> = texts
                .iter()
                .filter(|(t, _)| *t == label)
                .map(|(_, c)| c)
                .collect();
            assert!(!found.is_empty(), "{kind:?}: Eintrag fehlt");
            for colors in found {
                assert!(!colors.contains(&weak), "{kind:?} gedämpft");
            }
        }
        r.frame(vec![key_down(Key::Escape, Modifiers::NONE)]);
        r.frame(vec![key_up(Key::Escape)]);
        r.frame(vec![]);
    }
}

/// Korrektur Task 10 (a): Ein Klick auf einen Drehbogen, dessen Ebene auf der Kante steht, gehört
/// der Triade und setzt keinen Punkt.
#[test]
fn a_click_on_an_edge_on_rotation_arc_sets_no_point() {
    let mut r = Rig::new(true, Tool::Line);
    {
        let s = r.st();
        // Blick entlang +Y: Die Ebene XY (Drehbogen um Z) steht auf der Kante, ZX ist offen.
        s.camera.look_along(-Vec3::Y, Vec3::Z);
        s.sketch_session.as_mut().unwrap().triad.active = TriadPlane::Zx;
    }
    r.frame(vec![]);
    let l = r.layout();
    let at = l.center + (l.tips[0].unwrap() - l.center) * 0.6;
    assert_eq!(triad_hit(&l, at), Some(TriadPart::Handle(TriadAxis::Z)));
    r.click(at, PointerButton::Primary);
    assert_eq!(r.points(), 0, "der Klick auf die Triade setzt keinen Punkt");
    assert_eq!(
        r.st().sketch_session.as_ref().unwrap().triad.rot,
        DQuat::IDENTITY
    );
}

/// Korrektur Task 10 (b): Das Winkelfeld am Drehgriff nimmt die Tastatur erst beim Tippen einer
/// Zahl; bis dahin wirken Pfeil hoch und Alt.
#[test]
fn the_angle_field_takes_the_keyboard_only_when_typing() {
    let mut r = Rig::new(true, Tool::Line);
    r.frame(vec![key_down(Key::Tab, Modifiers::NONE)]);
    r.frame(vec![key_up(Key::Tab)]);
    r.frame(vec![]);
    assert_eq!(
        r.st().sketch_session.as_ref().unwrap().triad.handle,
        Some(TriadAxis::X)
    );
    assert!(
        r.ctx.memory(|m| m.focused()).is_none(),
        "das Feld nimmt die Tastatur nicht von selbst"
    );
    // Pfeil hoch sperrt weiter.
    r.frame(vec![key_down(Key::ArrowUp, Modifiers::NONE)]);
    r.frame(vec![]);
    assert!(r.st().sketch_session.as_ref().unwrap().triad.lock_z);
    r.frame(vec![key_up(Key::ArrowUp)]);
    r.frame(vec![]);
    assert!(!r.st().sketch_session.as_ref().unwrap().triad.lock_z);
    // Eine Zahl öffnet das Feld, Eingabe dreht.
    r.frame(vec![Event::Text("90".into())]);
    r.frame(vec![key_down(Key::Enter, Modifiers::NONE)]);
    r.frame(vec![key_up(Key::Enter)]);
    let t = r.st().sketch_session.as_ref().unwrap().triad;
    assert!(near(t.axes()[1], DVec3::Z, 1e-9), "{:?}", t.axes());
}

/// Korrektur Task 10 (c): Esc (auch nach Tippen ins Winkelfeld) und ein Sprung im Verlauf beenden
/// den Zug am Drehbogen und lassen nichts zurück.
#[test]
fn esc_and_undo_end_an_arc_drag() {
    for how in ["esc", "undo"] {
        let mut r = Rig::new(true, Tool::Line);
        // Ein Schritt im Verlauf, zu dem Rückgängig springen kann.
        {
            let (se, sk) = r.st().session_sketch_mut().unwrap();
            se.choose_tool(sk, Tool::Point);
            se.place_point_3d(sk, DVec3::new(30.0, 30.0, 0.0), None)
                .unwrap();
            se.choose_tool(sk, Tool::Line);
            se.triad.origin = DVec3::ZERO;
        }
        r.frame(vec![]);
        r.frame(vec![]);
        // Tab und eine Zahl: das Winkelfeld hat die Tastatur.
        r.frame(vec![key_down(Key::Tab, Modifiers::NONE)]);
        r.frame(vec![key_up(Key::Tab)]);
        r.frame(vec![Event::Text("1".into())]);
        r.frame(vec![]);
        let l = r.layout();
        let (ex, ey) = (l.tips[0].unwrap() - l.center, l.tips[1].unwrap() - l.center);
        let on = |deg: f32| {
            let a = deg.to_radians();
            l.center + (ex * a.cos() + ey * a.sin()) * ARC_FRAC
        };
        let (press, to) = (on(45.0), on(80.0));
        assert_eq!(triad_hit(&l, press), Some(TriadPart::Handle(TriadAxis::Z)));
        r.frame(vec![Event::PointerMoved(press)]);
        r.frame(vec![
            Event::PointerMoved(press),
            button(press, PointerButton::Primary, true),
        ]);
        for k in 1..=4 {
            r.frame(vec![Event::PointerMoved(
                press + (to - press) * (k as f32 / 4.0),
            )]);
        }
        assert!(
            r.st()
                .sketch_session
                .as_ref()
                .unwrap()
                .triad_ui
                .drag
                .is_some()
        );
        let n = r.st().sketch_history().unwrap().undo_len();
        if how == "esc" {
            r.frame(vec![key_down(Key::Escape, Modifiers::NONE)]);
            r.frame(vec![key_up(Key::Escape)]);
        } else {
            assert!(r.st().undo_sketch());
        }
        r.frame(vec![Event::PointerMoved(to)]);
        let se = r.st().sketch_session.as_ref().unwrap();
        assert!(se.triad_ui.drag.is_none(), "{how}: der Zug ist zu Ende");
        assert_eq!(
            se.triad.rot,
            DQuat::IDENTITY,
            "{how}: die Drehung ist zurück"
        );
        r.frame(vec![button(to, PointerButton::Primary, false)]);
        r.frame(vec![]);
        let se = r.st().sketch_session.as_ref().unwrap();
        assert_eq!(se.triad.rot, DQuat::IDENTITY, "{how}");
        assert_eq!(se.tool, Tool::Line, "{how}");
        let want = if how == "esc" { 1 } else { 0 };
        assert_eq!(r.points(), want, "{how}: das Loslassen setzt keinen Punkt");
        if how == "esc" {
            assert_eq!(r.st().sketch_history().unwrap().undo_len(), n);
        }
    }
}

// ---- Ebene Kurven (Task 12) ----------------------------------------------------------------------

impl View {
    /// Ansicht mit dem Werkzeug der Art `kind` in dieser Art.
    fn with_kind(kind: ToolKind) -> View {
        let mut v = View::new(kind.tool());
        let (se, sk) = v.se_sk();
        se.choose_kind(sk, kind);
        v.frame(vec![]);
        v
    }

    /// Die normalen Kurven (ohne Hilfslinien).
    fn curves(&self) -> Vec<(CurveId, CurveKind)> {
        self.sk()
            .curves()
            .filter(|(_, d)| !d.construction)
            .map(|(c, d)| (c, d.kind.clone()))
            .collect()
    }

    /// Abgetastete Punkte der Kurve `c` im Raum der Skizze.
    fn samples(&self, c: CurveId) -> Vec<DVec3> {
        let sk = self.sk();
        let (lo, hi) = sk.curve_domain3(c).expect("Bereich");
        (0..=32)
            .map(|k| {
                sk.curve_point3(c, lo + (hi - lo) * f64::from(k) / 32.0)
                    .expect("Punkt")
            })
            .collect()
    }

    /// Prüft, dass die Kurve `c` ganz in der Ebene durch `o` mit der Normale `n` liegt.
    fn assert_in_plane(&self, c: CurveId, o: DVec3, n: DVec3) {
        for p in self.samples(c) {
            assert!((p - o).dot(n).abs() < 1e-6, "{p:?} nicht in der Ebene");
        }
    }

    fn triad(&self) -> planora_app::sketch_triad::Triad {
        self.s.sketch_session.as_ref().unwrap().triad
    }
}

/// Liegen die Richtungen `a` und `b` auf einer Geraden?
fn same_axis(a: DVec3, b: DVec3) -> bool {
    a.normalize().dot(b.normalize()).abs() > 1.0 - 1e-6
}

#[test]
fn circle_2p_in_triad_plane_gets_normal() {
    let mut v = View::with_kind(ToolKind::Circle2P);
    v.set_plane(TriadPlane::Yz);
    v.click3(DVec3::new(0.0, 10.0, 20.0));
    assert!(v.curves().is_empty(), "der erste Klick legt nichts an");
    v.click3(DVec3::new(0.0, 40.0, 20.0));
    let curves = v.curves();
    assert_eq!(curves.len(), 1, "ein Kreis");
    let (c, CurveKind::Circle { center, .. }) = curves[0].clone() else {
        panic!("kein Kreis: {curves:?}");
    };
    assert!(v.sk().is_3d_curve(c));
    assert!(same_axis(v.sk().curve_normal(c), DVec3::X), "Normale X");
    assert!(v.sk().is_3d_point(center));
    assert!(near(v.xyz(center), DVec3::new(0.0, 25.0, 20.0), 0.1));
    assert!((v.sk().radius(c).unwrap() - 15.0).abs() < 0.1);
    v.assert_in_plane(c, DVec3::ZERO, DVec3::X);
    // Nur der Mittelpunkt, keine weiteren Punkte.
    assert_eq!(v.points().len(), 1);
}

#[test]
fn arc_3p_in_rotated_triad() {
    let mut v = View::with_kind(ToolKind::Arc3P);
    {
        let (se, _) = v.se_sk();
        se.triad.rotate(TriadAxis::X, 0.5);
    }
    v.frame(vec![]);
    let t = v.triad();
    let (u, w) = (t.axis(TriadAxis::X), t.axis(TriadAxis::Y));
    let n = t.axis(TriadAxis::Z);
    let at = |a: f64, b: f64| u * a + w * b;
    let (start, end, through) = (at(20.0, 10.0), at(60.0, 10.0), at(40.0, 25.0));
    v.click3(start);
    v.click3(end);
    assert!(v.curves().is_empty());
    v.click3(through);
    let curves = v.curves();
    assert_eq!(curves.len(), 1, "ein Bogen: {curves:?}");
    let (
        c,
        CurveKind::Arc {
            center,
            start: s,
            end: e,
        },
    ) = curves[0].clone()
    else {
        panic!("kein Bogen: {curves:?}");
    };
    assert!(
        same_axis(v.sk().curve_normal(c), n),
        "Normale der Triadenebene"
    );
    for p in [center, s, e] {
        assert!(v.sk().is_3d_point(p), "3D-Punkt");
    }
    let ends = [v.xyz(s), v.xyz(e)];
    for want in [start, end] {
        assert!(
            ends.iter().any(|&q| near(q, want, 0.1)),
            "{want:?} fehlt: {ends:?}"
        );
    }
    v.assert_in_plane(c, DVec3::ZERO, n);
    // Der Bogen läuft durch den dritten Klick.
    let m = v.xyz(center);
    let r = v.sk().radius(c).unwrap();
    assert!(((through - m).length() - r).abs() < 0.1);
    assert!(
        v.samples(c).iter().any(|&p| near(p, through, 1.5)),
        "durch den dritten Punkt"
    );
}

#[test]
fn rect_2p_in_zx_is_four_3d_lines_with_perpendicular() {
    // Gedreht um die Normale der Ebene ZX: Die Achsen sind keine Achsen der Skizze.
    let mut v = View::with_kind(ToolKind::Rect2P);
    v.set_plane(TriadPlane::Zx);
    {
        let (se, _) = v.se_sk();
        se.triad.rotate(TriadAxis::Y, 0.3);
    }
    v.frame(vec![]);
    let t = v.triad();
    let (a, b) = (t.axis(TriadAxis::Z), t.axis(TriadAxis::X));
    let (c0, c2) = (a * 20.0 + b * 25.0, a * 50.0 + b * 60.0);
    v.click3(c0);
    assert!(v.lines().is_empty());
    v.click3(c2);
    let lines = v.lines();
    assert_eq!(lines.len(), 4, "vier Linien");
    assert_eq!(v.points().len(), 4, "vier Ecken");
    for p in v.points() {
        assert!(v.sk().is_3d_point(p), "3D-Ecke");
        assert!(v.xyz(p).y.abs() < 1e-6, "in der Ebene ZX: {:?}", v.xyz(p));
    }
    let corners: Vec<DVec3> = v.points().iter().map(|&p| v.xyz(p)).collect();
    for want in [c0, c2, a * 20.0 + b * 60.0, a * 50.0 + b * 25.0] {
        assert!(
            corners.iter().any(|&q| near(q, want, 0.1)),
            "Ecke {want:?} fehlt: {corners:?}"
        );
    }
    assert!(has(v.sk(), |k| matches!(
        k,
        ConstraintKind::Perpendicular(..)
    )));
    assert!(has(v.sk(), |k| matches!(k, ConstraintKind::Parallel(..))));
    assert!(!has(v.sk(), |k| matches!(
        k,
        ConstraintKind::Horizontal(_) | ConstraintKind::Vertical(_) | ConstraintKind::AlongZ(_)
    )));
    for (_, pa, pb) in &lines {
        let d = v.xyz(*pb) - v.xyz(*pa);
        assert!(
            same_axis(d, a) || same_axis(d, b),
            "Kante entlang der Triade: {d:?}"
        );
    }

    // Unverdreht liegen die Achsen auf Z und X: dann Horizontal und Entlang Z wie in 2D.
    let mut v = View::with_kind(ToolKind::Rect2P);
    v.set_plane(TriadPlane::Zx);
    v.click3(DVec3::new(50.0, 0.0, 10.0));
    v.click3(DVec3::new(80.0, 0.0, 40.0));
    assert_eq!(v.lines().len(), 4);
    let count = |v: &View, f: fn(&ConstraintKind) -> bool| {
        v.sk().constraints().filter(|(_, c)| f(&c.kind)).count()
    };
    assert_eq!(count(&v, |k| matches!(k, ConstraintKind::Horizontal(_))), 2);
    assert_eq!(count(&v, |k| matches!(k, ConstraintKind::AlongZ(_))), 2);
    assert_eq!(
        count(&v, |k| matches!(k, ConstraintKind::Perpendicular(..))),
        0
    );
    for p in v.points() {
        assert!(v.sk().is_3d_point(p));
        assert!(v.xyz(p).y.abs() < 1e-6);
    }
}

#[test]
fn ellipse_in_yz() {
    let mut v = View::new(Tool::Ellipse);
    v.set_plane(TriadPlane::Yz);
    let m = DVec3::new(0.0, 20.0, 20.0);
    v.click3(m);
    v.click3(DVec3::new(0.0, 50.0, 20.0));
    assert!(v.curves().is_empty());
    v.click3(DVec3::new(0.0, 25.0, 32.0));
    let curves = v.curves();
    assert_eq!(curves.len(), 1, "eine Ellipse: {curves:?}");
    let (c, CurveKind::Ellipse { center, .. }) = curves[0].clone() else {
        panic!("keine Ellipse: {curves:?}");
    };
    assert!(same_axis(v.sk().curve_normal(c), DVec3::X), "Normale X");
    assert!(v.sk().is_3d_point(center));
    assert!(near(v.xyz(center), m, 0.1));
    v.assert_in_plane(c, DVec3::ZERO, DVec3::X);
    // Große Halbachse 30 entlang Y, kleine 12.
    let dist: Vec<f64> = v.samples(c).iter().map(|p| (*p - m).length()).collect();
    let far = dist.iter().copied().fold(0.0, f64::max);
    let close = dist.iter().copied().fold(f64::MAX, f64::min);
    assert!((far - 30.0).abs() < 0.2, "große Halbachse {far}");
    assert!((close - 12.0).abs() < 0.3, "kleine Halbachse {close}");
}

#[test]
fn conic_in_triad_plane() {
    let mut v = View::with_kind(ToolKind::Conic);
    v.set_plane(TriadPlane::Yz);
    let (start, end, apex) = (
        DVec3::new(0.0, 10.0, 10.0),
        DVec3::new(0.0, 50.0, 10.0),
        DVec3::new(0.0, 30.0, 40.0),
    );
    v.click3(start);
    v.click3(end);
    v.click3(apex);
    assert!(v.curves().is_empty(), "das Rho fehlt noch");
    // Der vierte Klick bestätigt das Rho.
    v.click3(DVec3::new(0.0, -30.0, 60.0));
    let curves = v.curves();
    assert_eq!(curves.len(), 1, "ein Kegelschnitt: {curves:?}");
    let (
        c,
        CurveKind::Conic {
            start: s,
            end: e,
            apex: a,
            ..
        },
    ) = curves[0].clone()
    else {
        panic!("kein Kegelschnitt: {curves:?}");
    };
    for (p, want) in [(s, start), (e, end), (a, apex)] {
        assert!(v.sk().is_3d_point(p), "3D-Punkt");
        assert!(near(v.xyz(p), want, 0.1), "{:?} statt {want:?}", v.xyz(p));
    }
    v.assert_in_plane(c, DVec3::ZERO, DVec3::X);
}

#[test]
fn circle_on_sketch_plane_is_2d() {
    for turn in [0.0, 0.4] {
        let mut v = View::with_kind(ToolKind::Circle2P);
        {
            let (se, _) = v.se_sk();
            se.triad.rotate(TriadAxis::Z, turn);
        }
        v.frame(vec![]);
        v.click3(DVec3::new(30.0, 10.0, 0.0));
        v.click3(DVec3::new(60.0, 30.0, 0.0));
        let curves = v.curves();
        assert_eq!(curves.len(), 1, "ein Kreis ({turn})");
        let (c, CurveKind::Circle { center, .. }) = curves[0].clone() else {
            panic!("kein Kreis");
        };
        assert!(!v.sk().is_3d_curve(c), "2D-Kreis ({turn})");
        assert!(!v.sk().is_3d_point(center), "2D-Mitte ({turn})");
        assert!(near(v.xyz(center), DVec3::new(45.0, 20.0, 0.0), 0.1));
        assert_eq!(v.sk().profiles().outlines.len(), 1, "ein Profil ({turn})");
    }
}

/// Esc nach den ersten Klicks lässt nichts, eine fertige Form ist ein Rückgängig-Schritt.
#[test]
fn planar_3d_shapes_esc_and_undo() {
    let mut v = View::with_kind(ToolKind::Arc3P);
    v.set_plane(TriadPlane::Yz);
    let n = v.s.sketch_history().unwrap().undo_len();
    v.click3(DVec3::new(0.0, 10.0, 20.0));
    v.click3(DVec3::new(0.0, 40.0, 20.0));
    v.key(Key::Escape);
    assert!(v.points().is_empty() && v.curves().is_empty());
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), n);
    v.click3(DVec3::new(0.0, 10.0, 20.0));
    v.click3(DVec3::new(0.0, 40.0, 20.0));
    v.click3(DVec3::new(0.0, 25.0, 35.0));
    assert_eq!(v.curves().len(), 1);
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), n + 1);
    v.key(Key::Escape);
    assert!(v.s.undo_sketch());
    assert!(v.points().is_empty() && v.curves().is_empty());
}

/// Nach dem Scheitel zeigt der Kegelschnitt in 3D das Feld Rho; ein getipptes Rho gilt mit
/// Eingabe.
#[test]
fn conic_in_3d_takes_a_typed_rho() {
    use planora_app::sketch_input::FieldKind;
    let mut v = View::with_kind(ToolKind::Conic);
    v.set_plane(TriadPlane::Yz);
    v.click3(DVec3::new(0.0, 10.0, 10.0));
    v.click3(DVec3::new(0.0, 50.0, 10.0));
    v.click3(DVec3::new(0.0, 30.0, 40.0));
    let aim = v.at3(DVec3::new(0.0, -30.0, 60.0));
    v.hover(aim);
    let fields: Vec<FieldKind> =
        v.s.sketch_session
            .as_ref()
            .unwrap()
            .input
            .fields
            .iter()
            .map(|f| f.kind)
            .collect();
    assert_eq!(fields, vec![FieldKind::Rho]);
    v.frame(vec![Event::Text("0.7".into())]);
    v.hover(aim);
    v.frame(vec![key_down(Key::Enter, Modifiers::NONE)]);
    v.frame(vec![key_up(Key::Enter)]);
    v.frame(vec![]);
    let rho: Vec<f64> = v
        .curves()
        .iter()
        .filter_map(|(_, k)| match k {
            CurveKind::Conic { rho, .. } => Some(*rho),
            _ => None,
        })
        .collect();
    assert_eq!(
        rho.len(),
        1,
        "{:?}",
        v.s.sketch_session.as_ref().unwrap().message
    );
    assert!((rho[0] - 0.7).abs() < 1e-9, "{rho:?}");
}

// ---- Text (Task 13) -----------------------------------------------------------------------------

/// Die Ecken eines Textes im Rahmen.
fn text_corners(sk: &Sketch) -> Vec<DVec3> {
    let (_, d) = sk.texts().next().expect("ein Text");
    match d.placement {
        planora_sketch::text::TextPlacement::Box { corners, .. } => {
            corners.iter().map(|&p| sk.point_xyz(p).unwrap()).collect()
        }
        planora_sketch::text::TextPlacement::Path { .. } => panic!("kein Rahmen"),
    }
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
fn text_frame_in_yz_plane() {
    let mut v = View::new(Tool::Text);
    v.set_plane(TriadPlane::Yz);
    let n = v.s.sketch_history().unwrap().undo_len();
    let (c0, c2) = (DVec3::new(0.0, 40.0, 10.0), DVec3::new(0.0, 80.0, 30.0));
    v.click3(c0);
    v.click3(c2);
    {
        let t = v.s.sketch_session.as_ref().unwrap().text.as_ref().unwrap();
        assert_eq!(t.corners.len(), 2, "zwei Ecken");
        assert!(t.plane3.is_some(), "Rahmen in 3D");
    }
    assert_eq!(v.sk().texts().count(), 0, "bis OK nichts in der Skizze");
    type_and_ok(&mut v, "Hi");
    assert_eq!(
        v.s.sketch_history().unwrap().undo_len(),
        n + 1,
        "ein Schritt"
    );
    let sk = v.sk();
    let (_, d) = sk.texts().next().unwrap();
    assert!(
        same_axis(d.normal.expect("Normale"), DVec3::X),
        "{:?}",
        d.normal
    );
    let corners = text_corners(sk);
    for q in &corners {
        assert!(q.x.abs() < 1e-6, "Ecke in der Ebene YZ: {q:?}");
    }
    for want in [
        c0,
        c2,
        DVec3::new(0.0, 80.0, 10.0),
        DVec3::new(0.0, 40.0, 30.0),
    ] {
        assert!(
            corners.iter().any(|&q| near(q, want, 0.1)),
            "Ecke {want:?} fehlt: {corners:?}"
        );
    }
    // Grundlinie entlang Y, oben ist Z: Ecke 0 unten links.
    assert!(near(corners[0], c0, 0.1), "{corners:?}");
    assert!(!d.points.is_empty());
    for &p in &d.points {
        assert!(sk.is_3d_point(p));
        assert!(
            sk.point_xyz(p).unwrap().x.abs() < 1e-6,
            "Buchstabe in der Ebene"
        );
    }
    assert!(sk.profiles().profiles.is_empty(), "3D-Text ohne Profil");

    // Ändern: Abschnitt wie in 2D, die Vorschau in der Ebene, OK bleibt in der Ebene.
    let (t, curves) = (sk.texts().next().unwrap().0, d.curves.len());
    {
        let (se, sk) = v.se_sk();
        assert!(se.begin_text_edit(sk, t));
        let tool = se.text.as_ref().unwrap();
        assert!(tool.plane3.is_some(), "Ändern in der Ebene des Textes");
        let frame = tool.frame_corners().unwrap();
        assert!(
            frame[0].length() < 1e-9,
            "Ecke 0 im Ursprung der Ebene: {frame:?}"
        );
    }
    type_and_ok(&mut v, "Hi Hi");
    let sk = v.sk();
    let d = sk.text(t).unwrap();
    assert!(d.curves.len() > curves, "mehr Buchstaben");
    for &p in &d.points {
        assert!(
            sk.point_xyz(p).unwrap().x.abs() < 1e-6,
            "nach dem Ändern in der Ebene"
        );
    }

    // Auf der Skizzenebene bleibt es der gewöhnliche 2D-Text.
    let mut v = View::new(Tool::Text);
    assert!(!v.s.sketch_session.as_ref().unwrap().draws_in_3d(v.sk()));
    v.click3(DVec3::new(40.0, 40.0, 0.0));
    v.click3(DVec3::new(80.0, 60.0, 0.0));
    assert!(
        v.s.sketch_session
            .as_ref()
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .plane3
            .is_none()
    );
    type_and_ok(&mut v, "Hi");
    let sk = v.sk();
    let (_, d) = sk.texts().next().unwrap();
    assert!(d.normal.is_none());
    assert!(text_corners(sk).iter().all(|q| q.z == 0.0));
    assert!(d.points.iter().all(|&p| !sk.is_3d_point(p)));
}

// ---- Namenszeile der Palette (Task H1) -----------------------------------------------------------

/// Der Skizzenname „Skizze 12“ steht in der Zeile „Name | 3D-Skizze | Profil anzeigen“
/// ungekürzt (bei 1920 × 1080).
#[test]
fn sketch_name_is_not_cut_in_the_palette() {
    // Ein sehr langer Name kürzt sich weiter (die Zeile bleibt eine Zeile).
    for (name, want_cut) in [("Skizze 12", false), ("Eine sehr lange Skizze 12", true)] {
        let mut r = Rig::new(true, Tool::Line);
        r.st().session_sketch_mut().unwrap().1.name = name.to_string();
        r.frame(vec![]);
        r.frame(vec![]);
        let (cut, rect) = sketch_panel::sketch_name_label(&r.ctx).expect("Namenszeile");
        assert_eq!(cut, want_cut, "„{name}“, {} px breit", rect.width());
    }
}

// ---- Alle Arten in der Ebene der Triade (Task H2) -----------------------------------------------

/// Zeichnet die Art `kind` in der Ebene `plane` mit den Klicks `clicks`: Vor dem letzten Klick
/// bleibt die Skizze, wie sie war (kein Rückgängig-Schritt, keine neue Kurve), der letzte legt die
/// Form als einen Rückgängig-Schritt an.
fn draw_kind(v: &mut View, kind: ToolKind, plane: TriadPlane, clicks: &[DVec3]) {
    {
        let (se, sk) = v.se_sk();
        se.choose_kind(sk, kind);
    }
    v.frame(vec![]);
    v.set_plane(plane);
    let n = v.s.sketch_history().unwrap().undo_len();
    let curves = v.curves().len();
    let points = v.points().len();
    let (last, first) = clicks.split_last().unwrap();
    for &p in first {
        v.click3(p);
        let msg = v.s.sketch_session.as_ref().unwrap().message.clone();
        assert_eq!(v.curves().len(), curves, "{kind:?}: noch keine Kurve");
        assert_eq!(v.points().len(), points, "{kind:?}: noch kein Punkt");
        assert_eq!(v.s.sketch_history().unwrap().undo_len(), n, "{kind:?}");
        assert!(msg.is_none(), "{kind:?}: {msg:?}");
    }
    v.click3(*last);
    let msg = v.s.sketch_session.as_ref().unwrap().message.clone();
    assert!(v.curves().len() > curves, "{kind:?}: keine Form ({msg:?})");
    assert_eq!(
        v.s.sketch_history().unwrap().undo_len(),
        n + 1,
        "{kind:?}: ein Rückgängig-Schritt"
    );
}

/// Neue normale Kurven seit `before`.
fn new_curves(v: &View, before: &[(CurveId, CurveKind)]) -> Vec<(CurveId, CurveKind)> {
    v.curves()
        .into_iter()
        .filter(|(c, _)| before.iter().all(|(b, _)| b != c))
        .collect()
}

/// Alle Punkte der Skizze liegen in der Ebene x = 0 und sind 3D.
fn assert_points_in_yz(v: &View) {
    for p in v.points() {
        assert!(v.sk().is_3d_point(p), "3D-Punkt");
        assert!(v.xyz(p).x.abs() < 1e-6, "in YZ: {:?}", v.xyz(p));
    }
}

/// Jede Kurve der Form liegt in YZ; Kreise und Bögen haben die Normale X.
fn assert_shape_in_yz(v: &View, curves: &[(CurveId, CurveKind)]) {
    for (c, k) in curves {
        v.assert_in_plane(*c, DVec3::ZERO, DVec3::X);
        if matches!(k, CurveKind::Circle { .. } | CurveKind::Arc { .. }) {
            assert!(v.sk().is_3d_curve(*c));
            assert!(same_axis(v.sk().curve_normal(*c), DVec3::X), "Normale X");
        }
    }
}

/// Ein 3D-Linienstück von `a` nach `b`, direkt in der Skizze angelegt.
fn add_line3(v: &mut View, a: DVec3, b: DVec3) -> CurveId {
    let (_, sk) = v.se_sk();
    let pa = sk.add_point3(a).unwrap();
    let pb = sk.add_point3(b).unwrap();
    let l = sk.add_line(pa, pb).unwrap();
    v.frame(vec![]);
    l
}

/// Abstand von `p` zur Geraden durch `a` und `b`.
fn line_distance(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    (p - a).cross((b - a).normalize()).length()
}

#[test]
fn center_circle_in_yz_plane() {
    let mut v = View::new(Tool::Circle);
    // Mittelpunkt-Radius ist die Vorgabe des Kreises.
    assert_eq!(
        v.s.sketch_session.as_ref().unwrap().kind_of(Tool::Circle),
        Some(ToolKind::CircleCenter)
    );
    let m = DVec3::new(0.0, 20.0, 20.0);
    draw_kind(
        &mut v,
        ToolKind::CircleCenter,
        TriadPlane::Yz,
        &[m, DVec3::new(0.0, 35.0, 20.0)],
    );
    let curves = v.curves();
    let (c, CurveKind::Circle { center, .. }) = curves[0].clone() else {
        panic!("kein Kreis: {curves:?}");
    };
    assert_eq!(curves.len(), 1);
    assert!(near(v.xyz(center), m, 0.1));
    assert!((v.sk().radius(c).unwrap() - 15.0).abs() < 0.1);
    assert_shape_in_yz(&v, &curves);
    assert_points_in_yz(&v);
}

#[test]
fn circle_3p_in_yz() {
    let mut v = View::new(Tool::Circle);
    draw_kind(
        &mut v,
        ToolKind::Circle3P,
        TriadPlane::Yz,
        &[
            DVec3::new(0.0, 10.0, 10.0),
            DVec3::new(0.0, 30.0, 10.0),
            DVec3::new(0.0, 20.0, 20.0),
        ],
    );
    let curves = v.curves();
    let (c, CurveKind::Circle { center, .. }) = curves[0].clone() else {
        panic!("kein Kreis: {curves:?}");
    };
    assert!(near(v.xyz(center), DVec3::new(0.0, 20.0, 10.0), 0.1));
    assert!((v.sk().radius(c).unwrap() - 10.0).abs() < 0.1);
    assert_shape_in_yz(&v, &curves);
}

#[test]
fn circle_2t_and_3t_in_yz() {
    let (a, b, c) = (
        DVec3::new(0.0, 10.0, 10.0),
        DVec3::new(0.0, 70.0, 10.0),
        DVec3::new(0.0, 10.0, 70.0),
    );
    // Zwei Tangenten: zwei Linien wählen, dann die Lage.
    let mut v = View::new(Tool::Circle);
    let l0 = add_line3(&mut v, a, b);
    let l1 = add_line3(&mut v, a, c);
    let before = v.curves();
    draw_kind(
        &mut v,
        ToolKind::Circle2T,
        TriadPlane::Yz,
        &[(a + b) * 0.5, (a + c) * 0.5, DVec3::new(0.0, 25.0, 25.0)],
    );
    let made = new_curves(&v, &before);
    assert_eq!(made.len(), 1, "{made:?}");
    let (circle, CurveKind::Circle { center, .. }) = made[0].clone() else {
        panic!("kein Kreis: {made:?}");
    };
    let (m, r) = (v.xyz(center), v.sk().radius(circle).unwrap());
    assert!((line_distance(m, a, b) - r).abs() < 0.01);
    assert!((line_distance(m, a, c) - r).abs() < 0.01);
    assert_shape_in_yz(&v, &made);
    for l in [l0, l1] {
        assert!(has(v.sk(), |k| matches!(k, ConstraintKind::Tangent(x, y)
            if *x == l && *y == circle)));
    }

    // Drei Tangenten: die drei Seiten eines Dreiecks.
    let mut v = View::new(Tool::Circle);
    add_line3(&mut v, a, b);
    add_line3(&mut v, a, c);
    add_line3(&mut v, b, c);
    let before = v.curves();
    draw_kind(
        &mut v,
        ToolKind::Circle3T,
        TriadPlane::Yz,
        &[(a + b) * 0.5, (a + c) * 0.5, (b + c) * 0.5],
    );
    let made = new_curves(&v, &before);
    let (circle, CurveKind::Circle { center, .. }) = made[0].clone() else {
        panic!("kein Kreis: {made:?}");
    };
    let (m, r) = (v.xyz(center), v.sk().radius(circle).unwrap());
    for (p, q) in [(a, b), (a, c), (b, c)] {
        assert!((line_distance(m, p, q) - r).abs() < 0.01, "tangential");
    }
    assert_shape_in_yz(&v, &made);
}

/// Tangentenkreise nur an Kurven in der Ebene der Triade: sonst eine Meldung, und nichts entsteht.
#[test]
fn tangent_circle_refuses_curves_outside_the_plane() {
    let mut v = View::new(Tool::Circle);
    // In XY, nicht in YZ.
    add_line3(
        &mut v,
        DVec3::new(10.0, 10.0, 0.0),
        DVec3::new(60.0, 10.0, 0.0),
    );
    {
        let (se, sk) = v.se_sk();
        se.choose_kind(sk, ToolKind::Circle2T);
    }
    v.set_plane(TriadPlane::Yz);
    let n = v.s.sketch_history().unwrap().undo_len();
    v.click3(DVec3::new(35.0, 10.0, 0.0));
    let se = v.s.sketch_session.as_ref().unwrap();
    assert!(se.pending_curves.is_empty(), "nicht gewählt");
    let text = se.message.as_ref().map(|m| m.text()).unwrap_or_default();
    assert!(text.contains("Triade"), "Hinweis: {text}");
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), n);
}

#[test]
fn arc_center_and_tangent_in_yz() {
    // Mittelpunkt: Mitte, Start, Richtung des Endes.
    let mut v = View::new(Tool::Arc);
    let m = DVec3::new(0.0, 20.0, 20.0);
    draw_kind(
        &mut v,
        ToolKind::ArcCenter,
        TriadPlane::Yz,
        &[m, DVec3::new(0.0, 35.0, 20.0), DVec3::new(0.0, 20.0, 40.0)],
    );
    let curves = v.curves();
    let (arc, CurveKind::Arc { center, start, end }) = curves[0].clone() else {
        panic!("kein Bogen: {curves:?}");
    };
    assert!(near(v.xyz(center), m, 0.1));
    assert!((v.sk().radius(arc).unwrap() - 15.0).abs() < 0.1);
    let ends = [v.xyz(start), v.xyz(end)];
    for want in [DVec3::new(0.0, 35.0, 20.0), DVec3::new(0.0, 20.0, 35.0)] {
        assert!(ends.iter().any(|&q| near(q, want, 0.1)), "{ends:?}");
    }
    assert_shape_in_yz(&v, &curves);
    assert_points_in_yz(&v);

    // Tangential: an einer Linie in YZ, vom Ende der Linie aus.
    let mut v = View::new(Tool::Arc);
    let (a, b) = (DVec3::new(0.0, 10.0, 10.0), DVec3::new(0.0, 40.0, 10.0));
    let line = add_line3(&mut v, a, b);
    let before = v.curves();
    draw_kind(
        &mut v,
        ToolKind::ArcTangent,
        TriadPlane::Yz,
        &[b, DVec3::new(0.0, 40.0, 40.0)],
    );
    let made = new_curves(&v, &before);
    let (arc, CurveKind::Arc { center, .. }) = made[0].clone() else {
        panic!("kein Bogen: {made:?}");
    };
    assert!(near(v.xyz(center), DVec3::new(0.0, 40.0, 25.0), 0.1));
    assert!((v.sk().radius(arc).unwrap() - 15.0).abs() < 0.1);
    assert!(has(v.sk(), |k| matches!(k, ConstraintKind::Tangent(x, y)
        if *x == arc && *y == line)));
    assert_shape_in_yz(&v, &made);
}

/// Tangential nur an Kurven in der Ebene der Triade.
#[test]
fn tangent_arc_refuses_a_line_outside_the_plane() {
    let mut v = View::new(Tool::Arc);
    let b = DVec3::new(0.0, 40.0, 10.0);
    add_line3(&mut v, DVec3::new(20.0, 40.0, 10.0), b);
    {
        let (se, sk) = v.se_sk();
        se.choose_kind(sk, ToolKind::ArcTangent);
    }
    v.set_plane(TriadPlane::Yz);
    v.click3(b);
    let se = v.s.sketch_session.as_ref().unwrap();
    assert!(se.pending3.is_empty(), "kein Anfang");
    let text = se.message.as_ref().map(|m| m.text()).unwrap_or_default();
    assert!(text.contains("Triade"), "Hinweis: {text}");
}

#[test]
fn rect_3p_and_center_in_yz() {
    let mut v = View::new(Tool::Rectangle);
    draw_kind(
        &mut v,
        ToolKind::Rect3P,
        TriadPlane::Yz,
        &[
            DVec3::new(0.0, 10.0, 10.0),
            DVec3::new(0.0, 40.0, 10.0),
            DVec3::new(0.0, 25.0, 30.0),
        ],
    );
    assert_eq!(v.lines().len(), 4);
    assert_points_in_yz(&v);
    let corners: Vec<DVec3> = v.points().iter().map(|&p| v.xyz(p)).collect();
    for (y, z) in [(10.0, 10.0), (40.0, 10.0), (40.0, 30.0), (10.0, 30.0)] {
        let want = DVec3::new(0.0, y, z);
        assert!(corners.iter().any(|&q| near(q, want, 0.1)), "{corners:?}");
    }
    assert!(has(v.sk(), |k| matches!(
        k,
        ConstraintKind::Perpendicular(..)
    )));

    // Mittelpunkt: Die Achsen von YZ sind Y und Z, also Vertikal und Entlang Z wie in 2D.
    let mut v = View::new(Tool::Rectangle);
    draw_kind(
        &mut v,
        ToolKind::RectCenter,
        TriadPlane::Yz,
        &[DVec3::new(0.0, 30.0, 30.0), DVec3::new(0.0, 40.0, 45.0)],
    );
    assert_eq!(v.lines().len(), 4);
    assert_eq!(v.points().len(), 5, "vier Ecken und die Mitte");
    assert_points_in_yz(&v);
    let corners: Vec<DVec3> = v.points().iter().map(|&p| v.xyz(p)).collect();
    for (y, z) in [(20.0, 15.0), (40.0, 15.0), (40.0, 45.0), (20.0, 45.0)] {
        let want = DVec3::new(0.0, y, z);
        assert!(corners.iter().any(|&q| near(q, want, 0.1)), "{corners:?}");
    }
    assert!(has(v.sk(), |k| matches!(k, ConstraintKind::Vertical(_))));
    assert!(has(v.sk(), |k| matches!(k, ConstraintKind::AlongZ(_))));
    assert!(!has(v.sk(), |k| matches!(k, ConstraintKind::Horizontal(_))));
    assert_eq!(
        v.sk().clone().solve().status,
        planora_sketch::solver::SolveStatus::Solved
    );
}

/// Gedreht um die Normale liegen die Achsen der Ebene nicht auf Achsen der Skizze: Senkrecht statt
/// Vertikal und Entlang Z.
#[test]
fn rect_center_in_a_rotated_plane_uses_perpendicular() {
    let mut v = View::new(Tool::Rectangle);
    {
        let (se, _) = v.se_sk();
        se.triad.rotate(TriadAxis::X, 0.4);
    }
    v.frame(vec![]);
    let t = v.triad();
    let (u, w) = (t.axis(TriadAxis::Y), t.axis(TriadAxis::Z));
    draw_kind(
        &mut v,
        ToolKind::RectCenter,
        TriadPlane::Yz,
        &[u * 30.0 + w * 30.0, u * 40.0 + w * 45.0],
    );
    assert_eq!(v.lines().len(), 4);
    assert!(has(v.sk(), |k| matches!(
        k,
        ConstraintKind::Perpendicular(..)
    )));
    assert!(!has(v.sk(), |k| matches!(
        k,
        ConstraintKind::Horizontal(_) | ConstraintKind::Vertical(_) | ConstraintKind::AlongZ(_)
    )));
    assert_points_in_yz(&v);
}

#[test]
fn polygons_in_yz() {
    let m = DVec3::new(0.0, 30.0, 30.0);
    for (kind, clicks) in [
        (
            ToolKind::PolygonCircumscribed,
            vec![m, DVec3::new(0.0, 30.0, 45.0)],
        ),
        (
            ToolKind::PolygonInscribed,
            vec![m, DVec3::new(0.0, 30.0, 45.0)],
        ),
        (
            ToolKind::PolygonEdge,
            vec![
                DVec3::new(0.0, 20.0, 20.0),
                DVec3::new(0.0, 40.0, 20.0),
                DVec3::new(0.0, 30.0, 40.0),
            ],
        ),
    ] {
        let mut v = View::new(Tool::Polygon);
        draw_kind(&mut v, kind, TriadPlane::Yz, &clicks);
        assert_eq!(v.lines().len(), 6, "{kind:?}: sechs Seiten");
        assert_points_in_yz(&v);
        let lens: Vec<f64> = v
            .lines()
            .iter()
            .map(|(_, a, b)| (v.xyz(*a) - v.xyz(*b)).length())
            .collect();
        for l in &lens {
            assert!((l - lens[0]).abs() < 0.01, "{kind:?}: gleich lang {lens:?}");
        }
        let all = v.curves();
        assert_shape_in_yz(&v, &all);
        if kind == ToolKind::PolygonInscribed {
            // Der Zeiger liegt auf einer Ecke: Umkreis 15, beim Sechseck auch die Seite.
            assert!((lens[0] - 15.0).abs() < 0.01, "{lens:?}");
        }
        assert_eq!(
            v.sk().clone().solve().status,
            planora_sketch::solver::SolveStatus::Solved,
            "{kind:?}"
        );
    }
}

fn kind_is_arc_slot(kind: ToolKind) -> bool {
    matches!(
        kind,
        ToolKind::SlotThreePointArc | ToolKind::SlotCenterPointArc
    )
}

#[test]
fn slots_in_yz() {
    let straight = [
        DVec3::new(0.0, 10.0, 20.0),
        DVec3::new(0.0, 50.0, 20.0),
        DVec3::new(0.0, 30.0, 28.0),
    ];
    for (kind, clicks) in [
        (ToolKind::SlotCenterToCenter, straight.to_vec()),
        (ToolKind::SlotOverall, straight.to_vec()),
        (
            ToolKind::SlotCenterPoint,
            vec![
                DVec3::new(0.0, 30.0, 20.0),
                DVec3::new(0.0, 50.0, 20.0),
                DVec3::new(0.0, 30.0, 28.0),
            ],
        ),
        (
            ToolKind::SlotThreePointArc,
            vec![
                DVec3::new(0.0, 10.0, 20.0),
                DVec3::new(0.0, 50.0, 20.0),
                DVec3::new(0.0, 30.0, 32.0),
                DVec3::new(0.0, 30.0, 36.0),
            ],
        ),
        (
            ToolKind::SlotCenterPointArc,
            vec![
                DVec3::new(0.0, 30.0, 20.0),
                DVec3::new(0.0, 50.0, 20.0),
                DVec3::new(0.0, 30.0, 40.0),
                DVec3::new(0.0, 54.0, 20.0),
            ],
        ),
    ] {
        let mut v = View::new(Tool::Slot);
        draw_kind(&mut v, kind, TriadPlane::Yz, &clicks);
        let all = v.curves();
        assert_eq!(all.len(), 4, "{kind:?}: vier Kurven {all:?}");
        assert_shape_in_yz(&v, &all);
        assert_points_in_yz(&v);
        // Die Enden: beim geraden Langloch die beiden Bögen, beim Bogenlangloch die kleinen.
        let caps: Vec<f64> = all
            .iter()
            .filter(|(_, k)| matches!(k, CurveKind::Arc { .. }))
            .map(|(c, _)| v.sk().radius(*c).unwrap())
            .filter(|&r| !kind_is_arc_slot(kind) || r < 5.0)
            .collect();
        assert_eq!(caps.len(), 2, "{kind:?}: zwei Enden {caps:?}");
        let want = if kind_is_arc_slot(kind) { 4.0 } else { 8.0 };
        for r in caps {
            assert!((r - want).abs() < 0.05, "{kind:?}: halbe Breite {r}");
        }
        assert_eq!(
            v.sk().clone().solve().status,
            planora_sketch::solver::SolveStatus::Solved,
            "{kind:?}"
        );
    }
}

/// Esc mitten in einer Form (auch nach gewählten Kurven) hinterlässt nichts.
#[test]
fn esc_in_the_new_kinds_leaves_nothing() {
    let mut v = View::new(Tool::Rectangle);
    {
        let (se, sk) = v.se_sk();
        se.choose_kind(sk, ToolKind::Rect3P);
    }
    v.set_plane(TriadPlane::Yz);
    let n = v.s.sketch_history().unwrap().undo_len();
    v.click3(DVec3::new(0.0, 10.0, 10.0));
    v.click3(DVec3::new(0.0, 40.0, 10.0));
    v.key(Key::Escape);
    assert!(v.points().is_empty() && v.curves().is_empty());
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), n);
    let se = v.s.sketch_session.as_ref().unwrap();
    assert!(se.pending3.is_empty() && se.plane3.is_none());

    let mut v = View::new(Tool::Circle);
    let (a, b) = (DVec3::new(0.0, 10.0, 10.0), DVec3::new(0.0, 70.0, 10.0));
    add_line3(&mut v, a, b);
    {
        let (se, sk) = v.se_sk();
        se.choose_kind(sk, ToolKind::Circle2T);
    }
    v.set_plane(TriadPlane::Yz);
    let n = v.s.sketch_history().unwrap().undo_len();
    v.click3((a + b) * 0.5);
    assert_eq!(
        v.s.sketch_session.as_ref().unwrap().pending_curves.len(),
        1,
        "gewählt"
    );
    v.key(Key::Escape);
    let se = v.s.sketch_session.as_ref().unwrap();
    assert!(se.pending_curves.is_empty() && se.plane3.is_none());
    assert_eq!(v.curves().len(), 1);
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), n);
}

/// Liegt die Ebene der Triade in der Skizzenebene (auch um Z gedreht), entsteht 2D-Geometrie.
#[test]
fn new_kinds_on_the_sketch_plane_stay_2d() {
    let mut v = View::new(Tool::Polygon);
    {
        let (se, _) = v.se_sk();
        se.triad.rotate(TriadAxis::Z, 0.4);
    }
    v.frame(vec![]);
    draw_kind(
        &mut v,
        ToolKind::PolygonInscribed,
        TriadPlane::Xy,
        &[DVec3::new(30.0, 30.0, 0.0), DVec3::new(45.0, 30.0, 0.0)],
    );
    assert_eq!(v.lines().len(), 6);
    for p in v.points() {
        assert!(!v.sk().is_3d_point(p), "2D-Punkt");
    }
    for (c, _) in v.curves() {
        assert!(!v.sk().is_3d_curve(c), "2D-Kurve");
    }
    assert_eq!(v.sk().profiles().outlines.len(), 1, "ein Profil");
}

/// Mit dem Haken zeigt jedes Werkzeug mit Arten die Triade, auch Polygon und Langloch.
#[test]
fn triad_is_visible_for_every_kind() {
    for kind in ToolKind::ALL.iter().filter(|k| {
        matches!(
            k.tool(),
            Tool::Circle | Tool::Arc | Tool::Rectangle | Tool::Polygon | Tool::Slot | Tool::Spline
        )
    }) {
        let v = View::with_kind(*kind);
        assert!(v.layout().is_some(), "{kind:?}: Triade fehlt");
    }
}

// ---- Fix-Runde 1 (Task H) ------------------------------------------------------------------------

/// Länge der Kurve `c` aus ihren Abtastpunkten.
fn curve_length(v: &View, c: CurveId) -> f64 {
    v.samples(c)
        .windows(2)
        .map(|w| (w[1] - w[0]).length())
        .sum()
}

/// Bogen „Mittelpunkt“ in 3D folgt dem Zug wie in Fusion: im und gegen den Uhrzeigersinn gezogen
/// entsteht jeweils der Viertelbogen, nicht der Ergänzungsbogen.
#[test]
fn arc_center_in_3d_follows_the_drag() {
    let m = DVec3::new(0.0, 20.0, 20.0);
    let start = DVec3::new(0.0, 35.0, 20.0);
    // In YZ ist die Normale X: Gegen den Uhrzeigersinn geht es von +Y nach +Z.
    for (via, end) in [
        (DVec3::new(0.0, 30.0, 30.0), DVec3::new(0.0, 20.0, 35.0)),
        (DVec3::new(0.0, 30.0, 10.0), DVec3::new(0.0, 20.0, 5.0)),
    ] {
        let mut v = View::new(Tool::Arc);
        {
            let (se, sk) = v.se_sk();
            se.choose_kind(sk, ToolKind::ArcCenter);
        }
        v.set_plane(TriadPlane::Yz);
        v.click3(m);
        v.click3(start);
        let at = v.at3(via);
        v.hover(at);
        v.click3(end);
        let curves = v.curves();
        assert_eq!(curves.len(), 1, "{end:?}: {curves:?}");
        let (
            arc,
            CurveKind::Arc {
                start: s, end: e, ..
            },
        ) = curves[0].clone()
        else {
            panic!("kein Bogen: {curves:?}");
        };
        let quarter = 15.0 * std::f64::consts::FRAC_PI_2;
        let len = curve_length(&v, arc);
        assert!((len - quarter).abs() < 0.2, "{end:?}: Bogenlänge {len}");
        let ends = [v.xyz(s), v.xyz(e)];
        for want in [start, end] {
            assert!(ends.iter().any(|&q| near(q, want, 0.1)), "{ends:?}");
        }
        assert_shape_in_yz(&v, &curves);
    }
}

/// Tangential in 3D: Enden an einem Punkt eine Linie in der Ebene und eine jüngere außerhalb, gilt
/// die in der Ebene (kein falscher Hinweis).
#[test]
fn tangent_arc_prefers_the_curve_in_the_plane() {
    let mut v = View::new(Tool::Arc);
    let (a, b) = (DVec3::new(0.0, 10.0, 10.0), DVec3::new(0.0, 40.0, 10.0));
    let line = add_line3(&mut v, a, b);
    // Jünger, am selben Punkt, aber entlang X (nicht in YZ).
    {
        let (_, sk) = v.se_sk();
        let end = match sk.curve(line).unwrap().kind {
            CurveKind::Line { b, .. } => b,
            _ => unreachable!(),
        };
        let off = sk.add_point3(DVec3::new(20.0, 40.0, 10.0)).unwrap();
        sk.add_line(end, off).unwrap();
    }
    v.frame(vec![]);
    let before = v.curves();
    draw_kind(
        &mut v,
        ToolKind::ArcTangent,
        TriadPlane::Yz,
        &[b, DVec3::new(0.0, 40.0, 40.0)],
    );
    let made = new_curves(&v, &before);
    let (arc, CurveKind::Arc { .. }) = made[0].clone() else {
        panic!("kein Bogen: {made:?}");
    };
    assert!(has(v.sk(), |k| matches!(k, ConstraintKind::Tangent(x, y)
        if *x == arc && *y == line)));
    assert_shape_in_yz(&v, &made);
}
