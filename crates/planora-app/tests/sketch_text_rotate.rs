//! Skizzen 11b, Task 12: Drehgriff am Textrahmen. Im Auswahlwerkzeug zeigt ein gewählter Text im
//! Rahmen in der Skizzenebene einen Griff über der Mitte der oberen Rahmenlinie (20 Bildpunkte
//! nach außen); Ziehen dreht den Text um Ecke 0, ein Schritt „Text drehen“. Esc und Rechtsklick
//! stellen den Startwinkel her. Kein Griff an Text auf Pfad und an 3D-Text. Hält eine Abhängigkeit
//! den Winkel, bleibt der Text, und die Meldung sagt es.
//!
//! Die Sitzung ohne Oberfläche, wie bei den Griffen der Splines.

mod common;

use planora_app::sketch_state::{Item, SketchSession};
use planora_app::sketch_view::PICK_RADIUS_PX;
use planora_app::state::{AppState, StandardPlane};
use planora_feature::SketchPlane;
use planora_i18n::tr;
use planora_math::{DVec2, DVec3};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

/// Pickradius der Sitzung in Skizzeneinheiten; daraus folgt der Maßstab Einheiten je Bildpunkt.
const R: f64 = 0.8;

fn upp() -> f64 {
    R / f64::from(PICK_RADIUS_PX)
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn square_shape() -> Arc<TextShape> {
    let square = Contour {
        start: v(0.0, 0.0),
        segs: vec![
            Seg::Line(v(0.6, 0.0)),
            Seg::Line(v(0.6, 0.7)),
            Seg::Line(v(0.0, 0.7)),
            Seg::Line(v(0.0, 0.0)),
        ],
    };
    Arc::new(TextShape {
        lines: vec![ShapedLine {
            glyphs: vec![Glyph {
                origin: v(0.0, 0.0),
                advance: 0.8,
                contours: vec![square],
            }],
            width: 0.8,
        }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    })
}

fn params() -> TextParams {
    TextParams {
        content: "A".into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 10.0,
        flip_h: false,
        flip_v: false,
    }
}

/// Skizze mit einem Text im Rahmen von (0,0) bis (40,10).
fn rig() -> (AppState, TextId) {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let t = {
        let sk = s.session_sketch_mut().unwrap().1;
        let t = sk
            .add_text_box(
                v(0.0, 0.0),
                v(40.0, 10.0),
                params(),
                HAlign::Left,
                VAlign::Top,
                square_shape(),
            )
            .unwrap();
        sk.solve();
        t
    };
    s.observe_sketch(false);
    (s, t)
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn se(s: &mut AppState) -> &mut SketchSession {
    s.sketch_session.as_mut().unwrap()
}

fn glyph_curve(s: &AppState, t: TextId) -> CurveId {
    sk(s).text(t).unwrap().curves[0]
}

fn select_text(s: &mut AppState, t: TextId) {
    let c = glyph_curve(s, t);
    se(s).selection = vec![Item::Curve(c)];
}

fn corners(s: &AppState, t: TextId) -> [DVec2; 4] {
    let TextPlacement::Box { corners, .. } = sk(s).text(t).unwrap().placement else {
        panic!("kein Rahmen");
    };
    corners.map(|p| sk(s).point_xy(p).unwrap())
}

fn grips(s: &AppState) -> Vec<(TextId, DVec2)> {
    s.sketch_session
        .as_ref()
        .unwrap()
        .text_rotate_grips(sk(s), upp())
}

fn begin(s: &mut AppState, at: DVec2) -> bool {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.begin_drag(sk, at, R)
}

fn drag(s: &mut AppState, at: DVec2) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.drag_to(sk, at);
}

fn end(s: &mut AppState) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.end_drag(sk);
    s.observe_sketch(false);
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

#[test]
fn grip_sits_20_px_above_the_middle_of_the_top_line() {
    let (mut s, t) = rig();
    assert!(grips(&s).is_empty(), "nichts gewählt, kein Griff");
    select_text(&mut s, t);
    let g = grips(&s);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].0, t);
    let c = corners(&s, t);
    let mid = (c[2] + c[3]) * 0.5;
    let out = (mid - (c[0] + c[1]) * 0.5).normalize();
    let want = mid + out * 20.0 * upp();
    assert!(g[0].1.distance(want) < 1e-9, "{:?} statt {want:?}", g[0].1);
}

#[test]
fn rotate_grip_turns_about_corner0() {
    let (mut s, t) = rig();
    select_text(&mut s, t);
    let before = corners(&s, t);
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    // Das Greifen liegt über der Mitte; um Ecke 0 gemessen etwa bei atan2(12, 20). Der Zeiger geht
    // um genau 90° um Ecke 0 weiter.
    let rel = at - before[0];
    let turned = before[0] + v(-rel.y, rel.x);
    drag(&mut s, turned);
    end(&mut s);
    let angle = sk(&s).text_angle(t).unwrap();
    assert!((angle - FRAC_PI_2).abs() < 1e-9, "{angle}");
    let after = corners(&s, t);
    assert!(after[0].distance(before[0]) < 1e-9, "Ecke 0 bleibt");
    assert!(((after[1] - after[0]).length() - (before[1] - before[0]).length()).abs() < 1e-9);
}

#[test]
fn rotate_is_one_undo_step() {
    let (mut s, t) = rig();
    select_text(&mut s, t);
    let steps = undo_len(&s);
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(-5.0, 4.0));
    drag(&mut s, at + v(-12.0, 8.0));
    drag(&mut s, at + v(-20.0, 10.0));
    end(&mut s);
    assert_eq!(undo_len(&s), steps + 1);
    assert!(sk(&s).text_angle(t).unwrap().abs() > 0.1);
    assert_eq!(
        s.sketch_undo_label().as_deref(),
        Some(tr!("undo-text-rotate").as_str())
    );
    assert!(s.undo_sketch());
    assert!(sk(&s).text_angle(t).unwrap().abs() < 1e-12);

    // Zurück auf den Anfang ist kein Schritt.
    let steps = undo_len(&s);
    select_text(&mut s, t);
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(-9.0, 3.0));
    drag(&mut s, at);
    end(&mut s);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn rotate_esc_restores() {
    let (mut s, t) = rig();
    select_text(&mut s, t);
    let steps = undo_len(&s);
    let before = sk(&s).clone();
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(-15.0, 6.0));
    assert!(sk(&s).text_angle(t).unwrap().abs() > 0.1);
    {
        let (se_, sk_) = s.session_sketch_mut().unwrap();
        se_.cancel(sk_);
        assert!(se_.box_aborted, "der Abbruch unterdrückt das Menü");
    }
    s.observe_sketch(false);
    assert_eq!(sk(&s).to_data(), before.to_data());
    assert!(sk(&s).text_angle(t).unwrap().abs() < 1e-12);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn right_click_during_the_drag_opens_no_menu_and_restores() {
    let (mut s, t) = rig();
    select_text(&mut s, t);
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(-15.0, 6.0));
    {
        let (se_, sk_) = s.session_sketch_mut().unwrap();
        se_.secondary_click(sk_, (900.0, 900.0));
        assert!(se_.curve_menu.is_none());
    }
    assert!(sk(&s).text_angle(t).unwrap().abs() < 1e-12);
}

#[test]
fn no_grip_for_path_or_3d_text() {
    let (mut s, _) = rig();
    // Text auf einem Pfad.
    let (path_text, line) = {
        let sk = s.session_sketch_mut().unwrap().1;
        let a = sk.add_point(0.0, 30.0).unwrap();
        let b = sk.add_point(40.0, 30.0).unwrap();
        let line = sk.add_line(a, b).unwrap();
        let t = sk
            .add_text_path(line, params(), true, false, HAlign::Left, square_shape())
            .unwrap();
        sk.solve();
        (t, line)
    };
    s.observe_sketch(false);
    let _ = line;
    select_text(&mut s, path_text);
    assert!(grips(&s).is_empty(), "Text auf Pfad ohne Griff");

    // Text in einer schrägen Ebene (3D).
    let t3 = {
        let sk = s.session_sketch_mut().unwrap().1;
        let t = sk
            .add_text_box3(
                [DVec3::new(0.0, 60.0, 0.0), DVec3::new(40.0, 60.0, 10.0)],
                DVec3::new(0.0, 1.0, 0.0),
                params(),
                HAlign::Left,
                VAlign::Top,
                square_shape(),
            )
            .unwrap();
        sk.solve();
        t
    };
    s.observe_sketch(false);
    select_text(&mut s, t3);
    assert!(grips(&s).is_empty(), "3D-Text ohne Griff");
}

#[test]
fn grip_needs_exactly_one_text_in_the_select_tool() {
    let (mut s, t) = rig();
    select_text(&mut s, t);
    assert_eq!(grips(&s).len(), 1);
    // Zusätzlich ein Punkt gewählt: nicht mehr genau ein Text.
    let p = {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.add_point(100.0, 100.0).unwrap()
    };
    let c = glyph_curve(&s, t);
    se(&mut s).selection = vec![Item::Curve(c), Item::Point(p)];
    assert!(grips(&s).is_empty());
    // Anderes Werkzeug: kein Griff.
    select_text(&mut s, t);
    se(&mut s).tool = planora_app::sketch_state::Tool::Line;
    assert!(grips(&s).is_empty());
}

#[test]
fn held_angle_refused_with_message() {
    let (mut s, t) = rig();
    let base = {
        let TextPlacement::Box { lines, .. } = sk(&s).text(t).unwrap().placement else {
            panic!();
        };
        lines[0]
    };
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.add_constraint(K::Horizontal(LineOrPoints::Line(base)))
            .unwrap();
        sk.solve();
    }
    s.observe_sketch(false);
    select_text(&mut s, t);
    let steps = undo_len(&s);
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(-15.0, 6.0));
    assert!(sk(&s).text_angle(t).unwrap().abs() < 1e-12, "Winkel 0");
    assert_eq!(
        se(&mut s).message_text().as_deref(),
        Some(tr!("sketch-text-angle-held").as_str())
    );
    end(&mut s);
    assert_eq!(undo_len(&s), steps, "kein Schritt");
    assert!(sk(&s).text_angle(t).unwrap().abs() < 1e-12);
}

#[test]
fn rotate_tool_switch_restores() {
    let (mut s, t) = rig();
    select_text(&mut s, t);
    let steps = undo_len(&s);
    let before = sk(&s).clone();
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(-15.0, 6.0));
    assert!(sk(&s).text_angle(t).unwrap().abs() > 0.1);
    {
        let (se_, sk_) = s.session_sketch_mut().unwrap();
        se_.set_tool(sk_, planora_app::sketch_state::Tool::Line);
    }
    s.observe_sketch(false);
    assert_eq!(sk(&s).to_data(), before.to_data());
    assert!(sk(&s).text_angle(t).unwrap().abs() < 1e-12);
    assert_eq!(undo_len(&s), steps, "kein Schritt");
}

#[test]
fn pointer_on_corner0_changes_nothing() {
    let (mut s, t) = rig();
    select_text(&mut s, t);
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(-15.0, 6.0));
    let turned = sk(&s).text_angle(t).unwrap();
    let c0 = corners(&s, t)[0];
    drag(&mut s, c0);
    assert!(
        (sk(&s).text_angle(t).unwrap() - turned).abs() < 1e-12,
        "hält"
    );
    end(&mut s);
}

#[test]
fn changing_the_selection_mid_drag_does_not_matter() {
    let (mut s, t) = rig();
    select_text(&mut s, t);
    let at = grips(&s)[0].1;
    assert!(begin(&mut s, at));
    se(&mut s).selection.clear();
    drag(&mut s, at + v(-15.0, 6.0));
    end(&mut s);
    assert!(sk(&s).text_angle(t).unwrap().abs() > 0.1);
    assert!(s.undo_sketch());
    assert!(sk(&s).text_angle(t).unwrap().abs() < 1e-12);
}
