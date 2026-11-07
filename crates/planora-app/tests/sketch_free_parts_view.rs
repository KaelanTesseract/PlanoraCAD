//! Handtest Skizzen 5, Korrektur 5, Bildschirmseite: Die Lage eines freien Bogens auf dem
//! Bildschirm bleibt, wenn anderswo ein Glatt gesetzt oder ein Maß geändert wird. Die App wird
//! ohne Fenster bei 1920×1080 gezeichnet (Skizze auf XZ wie im Handtest).

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_state::Item;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_view, theme};
use planora_feature::SketchPlane;
use planora_i18n::{german, with_lang};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{PointId, Sketch};
use planora_sketch::shapes::arc_through;

const SIZE: Vec2 = vec2(1920.0, 1080.0);

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(events: Vec<Event>) -> egui::RawInput {
    let time = TIME.with(|t| {
        t.set(t.get() + 1.0 / 60.0);
        t.get()
    });
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
        time: Some(time),
        events,
        ..Default::default()
    }
}

fn state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

fn sk(app: &mut PlanoraApp) -> &mut Sketch {
    state(app).session_sketch_mut().unwrap().1
}

fn sketching() -> PlanoraApp {
    let root = common::temp_root("planora-sketch-free-parts-view");
    let ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let mut app = PlanoraApp::for_tests(ws);
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    s.open_sketch(id).unwrap();
    app
}

struct Shot {
    texts: Vec<(String, Rect)>,
}

fn walk(shape: &Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
        Shape::Text(t) => out.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        _ => {}
    }
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let mut texts = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut texts);
        }
        Shot { texts }
    }

    fn at(&self, exact: &str) -> Rect {
        self.texts
            .iter()
            .filter(|(t, _)| t == exact)
            .min_by(|a, b| a.1.top().total_cmp(&b.1.top()))
            .unwrap_or_else(|| panic!("„{exact}“ fehlt"))
            .1
    }
}

fn frame(c: &egui::Context, app: &mut PlanoraApp, events: Vec<Event>) -> Shot {
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(events), |ui| app.ui(ui, &mut f));
    let shot = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    shot
}

fn settle(c: &egui::Context, app: &mut PlanoraApp) -> Shot {
    for _ in 0..8 {
        frame(c, app, vec![]);
    }
    frame(c, app, vec![])
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn click(c: &egui::Context, app: &mut PlanoraApp, p: Pos2) -> Shot {
    frame(c, app, vec![Event::PointerMoved(p)]);
    frame(c, app, vec![Event::PointerMoved(p), button(p, true)]);
    frame(c, app, vec![button(p, false)]);
    settle(c, app)
}

fn press(c: &egui::Context, app: &mut PlanoraApp, label: &str) -> Shot {
    let shot = settle(c, app);
    let at = shot.at(label).center();
    click(c, app, at)
}

fn pt(app: &mut PlanoraApp, p: DVec2) -> PointId {
    sk(app).add_point(p.x, p.y).unwrap()
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Bildschirmlage der Punkte (Pixel).
fn on_screen(app: &mut PlanoraApp, points: &[PointId]) -> Vec<Vec2> {
    let st = state(app);
    let cam = st.camera;
    let sk = st.session_sketch().unwrap();
    let frame = *sk.frame();
    let viewport = glam::Vec2::new(SIZE.x, SIZE.y);
    points
        .iter()
        .map(|&p| {
            let s = sketch_view::sketch_to_screen(&cam, &frame, viewport, sk.point_xy(p).unwrap())
                .unwrap();
            vec2(s.x, s.y)
        })
        .collect()
}

#[test]
fn a_free_arc_keeps_its_place_on_screen_when_smooth_is_added_elsewhere() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let plan = arc_through(v(50.3, 7.3), v(78.7, 11.9), v(67.2, 22.6), 1e-6).unwrap();
        let arc = [
            pt(&mut app, plan.center),
            pt(&mut app, plan.start),
            pt(&mut app, plan.end),
        ];
        sk(&mut app).add_arc(arc[0], arc[1], arc[2]).unwrap();
        let a = pt(&mut app, v(-10.0, 0.0));
        let joint = pt(&mut app, v(0.0, 0.0));
        let l = sk(&mut app).add_line(a, joint).unwrap();
        let q1 = pt(&mut app, v(5.0, 3.0));
        let q2 = pt(&mut app, v(10.0, 0.0));
        let sp = sk(&mut app).add_spline(&[joint, q1, q2]).unwrap();
        sk(&mut app)
            .add_constraint(K::Fixed(a, -10.0, 0.0))
            .unwrap();
        settle(&c, &mut app);
        let cam_before = state(&mut app).camera;
        let before = on_screen(&mut app, &arc);
        state(&mut app).sketch_session.as_mut().unwrap().selection =
            vec![Item::Curve(l), Item::Curve(sp)];
        press(&c, &mut app, "Glatt");
        assert!(matches!(
            sk(&mut app).constraints().last(),
            Some((_, d)) if matches!(d.kind, K::Smooth(..))
        ));
        let after = on_screen(&mut app, &arc);
        let cam_after = state(&mut app).camera;
        for (b, a) in before.iter().zip(&after) {
            assert!(
                (*b - *a).length() < 0.01,
                "Bogen rutscht auf dem Bildschirm: {b:?} → {a:?}; Kamera {:?} → {:?}",
                (cam_before.ndc_offset, cam_before.visible_height_fraction),
                (cam_after.ndc_offset, cam_after.visible_height_fraction)
            );
        }
    });
}
