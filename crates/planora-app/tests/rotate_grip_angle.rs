//! Skizzen 12, Task 11: Winkelanzeige an Drehgriffen. Beim Zug am Text-Drehgriff und am Drehring der
//! Triade (Verschieben/Kopieren) steht der aktuelle Winkel am Zeiger, Format über `format_angle`.
//! Der Werkzeugwechsel mitten im Zug am Rho-Griff nimmt den Zug zurück wie am Drehgriff.
//!
//! Die Sitzung ohne Oberfläche, wie bei den Griffen der Splines.

mod common;

use egui::pos2;
use planora_app::sketch_grips::Grip;
use planora_app::sketch_state::{Item, SketchSession, Tool};
use planora_app::sketch_view::{
    PICK_RADIUS_PX, format_angle, place_pointer_label, rotate_drag_angle,
};
use planora_app::state::{AppState, StandardPlane};
use planora_feature::SketchPlane;
use planora_i18n::{english, german, with_lang};
use planora_math::DVec2;
use planora_sketch::conic_shoulder;
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

const R: f64 = 0.8;

fn upp() -> f64 {
    R / f64::from(PICK_RADIUS_PX)
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn se(s: &mut AppState) -> &mut SketchSession {
    s.sketch_session.as_mut().unwrap()
}

fn open_sketch() -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    s
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

/// Skizze mit einem Text im Rahmen von (0,0) bis (40,10), der Text gewählt.
fn text_rig() -> (AppState, TextId) {
    let mut s = open_sketch();
    let t = {
        let sk = s.session_sketch_mut().unwrap().1;
        let t = sk
            .add_text_box(
                v(0.0, 0.0),
                v(40.0, 10.0),
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
                },
                HAlign::Left,
                VAlign::Top,
                square_shape(),
            )
            .unwrap();
        sk.solve();
        t
    };
    s.observe_sketch(false);
    let c = sk(&s).text(t).unwrap().curves[0];
    se(&mut s).selection = vec![Item::Curve(c)];
    (s, t)
}

fn begin(s: &mut AppState, at: DVec2) -> bool {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.begin_drag(sk, at, R)
}

fn drag(s: &mut AppState, at: DVec2) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.drag_to(sk, at);
}

fn shown(s: &AppState) -> Option<f64> {
    rotate_drag_angle(s.sketch_session.as_ref().unwrap(), sk(s))
}

fn change_tool(s: &mut AppState) {
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    se_.set_tool(sk_, Tool::Line);
    s.observe_sketch(false);
}

#[test]
fn text_rotate_shows_angle() {
    let (mut s, t) = text_rig();
    assert_eq!(shown(&s), None, "ohne Zug keine Anzeige");
    let at = s
        .sketch_session
        .as_ref()
        .unwrap()
        .text_rotate_grips(sk(&s), upp())[0]
        .1;
    assert!(begin(&mut s, at));
    // Um Ecke 0 genau 90° weiter: Der Text steht danach auf 90°.
    let turned = v(-at.y, at.x);
    drag(&mut s, turned);
    let angle = shown(&s).expect("Anzeige im Zug");
    assert!((angle - FRAC_PI_2).abs() < 1e-9, "{angle}");
    assert_eq!(Some(angle), sk(&s).text_angle(t));
    {
        let (se_, sk_) = s.session_sketch_mut().unwrap();
        se_.end_drag(sk_);
    }
    s.observe_sketch(false);
    assert_eq!(shown(&s), None, "nach dem Loslassen keine Anzeige");
}

#[test]
fn triad_rotate_shows_angle() {
    let mut s = open_sketch();
    let ids = {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.add_rectangle(v(-5.0, -5.0), v(5.0, 5.0)).unwrap().lines
    };
    s.observe_sketch(false);
    {
        let (se_, sk_) = s.session_sketch_mut().unwrap();
        se_.selection = ids.iter().map(|&c| Item::Curve(c)).collect();
        se_.set_tool(sk_, Tool::MoveCopy);
    }
    assert_eq!(shown(&s), None);
    let base = se(&mut s).transform_now().expect("Bezugspunkt");
    let center = base.base + base.offset;
    se(&mut s).begin_grip(Grip::Rotate, center + v(10.0, 0.0));
    assert_eq!(shown(&s), Some(0.0));
    se(&mut s).grip_to(center + v(0.0, 10.0));
    let angle = shown(&s).expect("Anzeige im Zug");
    assert!((angle - FRAC_PI_2).abs() < 1e-9, "{angle}");
    se(&mut s).end_grip();
    assert_eq!(shown(&s), None);
}

#[test]
fn angle_text_uses_display_units() {
    let de = with_lang(&german(), || format_angle(FRAC_PI_2));
    let en = with_lang(&english(), || format_angle(FRAC_PI_2));
    assert_eq!(de, "90,0°");
    assert_eq!(en, "90.0°");
    // Lage: neben dem Zeiger (rechts unten), solange Platz ist.
    let bounds = egui::Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    let size = egui::vec2(40.0, 16.0);
    let at = place_pointer_label(pos2(100.0, 50.0), size, &[], bounds);
    assert!(at.x > 100.0 && at.y > 50.0, "neben dem Zeiger: {at:?}");
}

fn rho_of(s: &AppState, c: CurveId) -> f64 {
    match sk(s).curve(c).unwrap().kind {
        CurveKind::Conic { rho, .. } => rho,
        _ => panic!("kein Kegelschnitt"),
    }
}

/// Wie am Drehgriff des Textes: Der Werkzeugwechsel mitten im Zug am Rho-Griff nimmt den Zug zurück,
/// kein Schritt (offener Punkt, Handtest 3 prüft es gegen Fusion).
#[test]
fn tool_change_during_rho_drag_reverts() {
    let mut s = open_sketch();
    let c = {
        let sk = s.session_sketch_mut().unwrap().1;
        let p = sk.add_point(0.0, 0.0).unwrap();
        let q = sk.add_point(20.0, 0.0).unwrap();
        let a = sk.add_point(10.0, 12.0).unwrap();
        sk.add_conic(p, q, a, 0.5).unwrap()
    };
    s.observe_sketch(false);
    se(&mut s).selection = vec![Item::Curve(c)];
    let at = conic_shoulder(v(0.0, 0.0), v(10.0, 12.0), v(20.0, 0.0), 0.5);
    let steps = s.sketch_history().unwrap().undo_len();
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(0.0, 2.4));
    assert!((rho_of(&s, c) - 0.7).abs() < 1e-9);
    change_tool(&mut s);
    assert_eq!(rho_of(&s, c), 0.5, "Zug zurückgenommen");
    assert_eq!(
        s.sketch_history().unwrap().undo_len(),
        steps,
        "kein Schritt"
    );
}

#[test]
fn label_avoids_the_input_field_and_stays_in_the_window() {
    use egui::{Rect, vec2};
    use planora_app::sketch_view::place_pointer_label;
    let bounds = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let size = vec2(60.0, 20.0);
    let overlaps = |p: egui::Pos2, f: Rect| Rect::from_min_size(p, size).intersects(f);
    // Frei: rechts unten neben dem Zeiger.
    let pointer = pos2(400.0, 300.0);
    let at = place_pointer_label(pointer, size, &[], bounds);
    assert_eq!(at, pointer + vec2(18.0, 18.0));
    // Ein Eingabefeld genau dort: das Feld weicht aus und deckt es nicht.
    let field = Rect::from_min_size(pos2(410.0, 310.0), vec2(200.0, 60.0));
    let at = place_pointer_label(pointer, size, &[field], bounds);
    assert!(!overlaps(at, field), "{at:?}");
    assert!(bounds.contains_rect(Rect::from_min_size(at, size)));
    // Am Fensterrand: bleibt im Fenster, auch mit Feld.
    for p in [pos2(995.0, 795.0), pos2(2.0, 2.0), pos2(1200.0, -50.0)] {
        let f = Rect::from_min_size(p + vec2(10.0, 10.0), vec2(300.0, 100.0));
        let at = place_pointer_label(p, size, &[f], bounds);
        assert!(
            bounds.contains_rect(Rect::from_min_size(at, size)),
            "{p:?} {at:?}"
        );
    }
    let p = pos2(995.0, 795.0);
    let f = Rect::from_min_size(pos2(900.0, 790.0), vec2(100.0, 10.0));
    let at = place_pointer_label(p, size, &[f], bounds);
    assert!(!overlaps(at, f), "{at:?}");
}
