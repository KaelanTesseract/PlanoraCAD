//! Zeichen der Abhängigkeiten in der Skizzenansicht (Skizzen 3): je Kurve jede Art höchstens
//! einmal, kein „auf“ an Hilfskurven. Ganze App bei 1920×1080, die Zeichen werden aus den
//! gemalten Texten gelesen.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_kinds::ToolKind;
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{SketchPlane, StandardPlane};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::CurveKind;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const R: f64 = 0.5;

fn walk(shape: &Shape, out: &mut Vec<String>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
        Shape::Text(t) => out.push(t.galley.text().to_string()),
        _ => {}
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new() -> Rig {
        let root = common::temp_root("planora-sketch-labels");
        let w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        let id = rig
            .st()
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        rig.st().open_sketch(id).unwrap();
        rig.shown();
        rig
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    /// Alle gemalten Texte nach ein paar Bildern.
    fn shown(&mut self) -> Vec<String> {
        let mut last = Vec::new();
        for _ in 0..3 {
            self.time += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                time: Some(self.time),
                ..Default::default()
            };
            let mut f = eframe::Frame::_new_kittest();
            let app = &mut self.app;
            let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
            last.clear();
            for c in &out.shapes {
                walk(&c.shape, &mut last);
            }
            out.drop_without_applying_deltas();
        }
        last
    }

    /// Zeichnet eine Art mit den gegebenen Klicks; `sides` stellt die Eckenzahl vorher ein.
    fn draw(&mut self, kind: ToolKind, sides: Option<usize>, clicks: &[(f64, f64)]) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        if let Some(n) = sides {
            se.polygon_sides = n;
        }
        se.choose_kind(sk, kind);
        for &(x, y) in clicks {
            se.click(sk, DVec2::new(x, y), R, false).unwrap();
        }
    }
}

fn count(texts: &[String], what: &str) -> usize {
    texts.iter().filter(|t| t.as_str() == what).count()
}

#[test]
fn an_eight_gon_shows_one_equal_per_line_and_no_on_curve() {
    let mut rig = Rig::new();
    let before = rig.shown();
    rig.draw(
        ToolKind::PolygonInscribed,
        Some(8),
        &[(5.0, -3.0), (15.0, -3.0)],
    );
    let (lines, constraints) = {
        let sk = rig.st().session_sketch().unwrap();
        let lines = sk
            .curves()
            .filter(|(_, d)| !d.construction && matches!(d.kind, CurveKind::Line { .. }))
            .count();
        (lines, sk.constraints().count())
    };
    assert_eq!(lines, 8);
    let after = rig.shown();
    // Skizzen 3: eines je Linie, nicht n-1 an der ersten.
    assert_eq!(
        count(&after, "=") - count(&before, "="),
        8,
        "ein „=“ je Linie"
    );
    assert_eq!(count(&after, "auf"), 0, "kein „auf“ an den Ecken");
    // Die Abhängigkeiten selbst bleiben: mehrere Gleich-Abhängigkeiten, auf-Kurve an den Ecken.
    let sk = rig.st().session_sketch().unwrap();
    assert_eq!(sk.constraints().count(), constraints);
    assert!(
        sk.constraints()
            .any(|(_, c)| matches!(c.kind, K::PointOnCurve(..)))
    );
    assert!(
        sk.constraints()
            .filter(|(_, c)| matches!(c.kind, K::Equal(..)))
            .count()
            >= 7
    );
}

#[test]
fn a_slot_shows_one_tangent_per_curve() {
    let mut rig = Rig::new();
    let before = rig.shown();
    rig.draw(
        ToolKind::SlotCenterToCenter,
        None,
        &[(5.0, -3.0), (45.0, -3.0), (25.0, 3.0)],
    );
    let curves = rig
        .st()
        .session_sketch()
        .unwrap()
        .curves()
        .filter(|(_, d)| !d.construction)
        .count();
    let after = rig.shown();
    assert_eq!(curves, 4, "zwei Linien und zwei Kappen");
    assert_eq!(
        count(&after, "T") - count(&before, "T"),
        4,
        "ein „T“ je Kurve"
    );
}

#[test]
fn a_point_on_a_normal_curve_still_shows_on_curve() {
    let mut rig = Rig::new();
    let before = rig.shown();
    {
        let (_, sk) = rig.st().session_sketch_mut().unwrap();
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(20.0, 0.0).unwrap();
        let line = sk.add_line(a, b).unwrap();
        let p = sk.add_point(10.0, 0.0).unwrap();
        sk.add_constraint(K::PointOnCurve(p, line)).unwrap();
    }
    let after = rig.shown();
    assert_eq!(count(&after, "auf") - count(&before, "auf"), 1);
}
