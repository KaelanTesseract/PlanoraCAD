//! Skizzenstatus oben im Skizzenfeld (Skizzen 11a, Task 15, A4).
//! Ohne Fenster bei 1920 × 1080.

use planora_app::sketch_panel::{self, StatusTone};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, theme};
use planora_feature::SketchPlane;
use planora_math::{Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::Sketch;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn status_line_texts() {
    // Unterbestimmt: freie Linie.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 1.0).unwrap();
    s.add_line(a, b).unwrap();
    let r = s.solve();
    assert_eq!(
        sketch_panel::status_line(&s, &r),
        ("Status: Unterbestimmt".to_string(), StatusTone::Normal)
    );
    // Bestimmt.
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Points(a, b)))
        .unwrap();
    s.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    let r = s.solve();
    assert_eq!(
        sketch_panel::status_line(&s, &r),
        ("Status: Bestimmt".to_string(), StatusTone::Normal)
    );
    // Widerspruch (rot).
    s.add_constraint(K::Distance(a, b, 9.0)).unwrap();
    let r = s.solve();
    assert_eq!(
        sketch_panel::status_line(&s, &r),
        ("Status: Widerspruch".to_string(), StatusTone::Error)
    );
    // Nicht gelöst (orange): Linie auf null Länge gesetzt.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    let c = s.add_point(9.0, 3.0).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 5.0, 0.0)).unwrap();
    s.add_line(b, c).unwrap();
    s.set_point_xy(c, 5.0, 0.0).unwrap();
    let r = s.solve();
    assert_eq!(
        sketch_panel::status_line(&s, &r),
        ("Status: nicht gelöst".to_string(), StatusTone::Warning)
    );
}

fn frame_text(ctx: &egui::Context, s: &mut AppState) -> String {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(raw.clone(), |ui| sketch_panel::show(ui.ctx(), s));
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

#[test]
fn status_follows_live_edit() {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (a, b) = {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(5.0, 0.0).unwrap();
        sk.add_line(a, b).unwrap();
        sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        sk.add_constraint(K::Horizontal(LineOrPoints::Points(a, b)))
            .unwrap();
        se.last_report = Some(sk.solve());
        (a, b)
    };
    let text = frame_text(&ctx, &mut s);
    assert!(text.contains("Status: Unterbestimmt"), "{text}");
    // Maß setzen: im selben Bild „Bestimmt“.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        sk.add_constraint(K::Distance(a, b, 5.0)).unwrap();
        se.last_report = Some(sk.solve());
    }
    let text = frame_text(&ctx, &mut s);
    assert!(text.contains("Status: Bestimmt"), "{text}");
    assert!(!text.contains("Status: Unterbestimmt"), "{text}");
}
