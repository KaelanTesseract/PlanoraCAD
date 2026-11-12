//! Skizzen 8, Task 7 (Korrekturrunde 1): Buchstaben eines Textes bieten beim Zeichnen keinen Fang
//! (Spec B9), und Werkzeuge, die Kurven wählen, nehmen keine Buchstaben (Spec E). Klicks über die
//! Sitzung (`commit_input`) wie in `sketch_fillet.rs`, ohne Fenster bei 1920 × 1080.

use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, StandardPlane};
use planora_feature::SketchPlane;
use planora_i18n::t;
use planora_math::DVec2;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use std::collections::HashSet;
use std::sync::Arc;

const PICK: f64 = 0.5;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Ein Zeichen: Quadrat (0, 0)–(0,6, 0,7) in Einheiten der Höhe, ohne Schrift.
fn shape() -> Arc<TextShape> {
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

/// Offene Skizze auf XY mit Text „a“ mittig im Rahmen (0, 0)–(40, 10): Das Quadrat liegt bei
/// x = 18 … 21, y = 1 … 4,5.
fn with_text() -> (AppState, TextId) {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let t = {
        let sk = s.session_sketch_mut().unwrap().1;
        let params = TextParams {
            content: "a".into(),
            style: TextStyle {
                font: "Test".into(),
                bold: false,
                italic: false,
                spacing: 0.0,
            },
            height: 5.0,
            flip_h: false,
            flip_v: false,
        };
        let t = sk
            .add_text_box(
                v(0.0, 0.0),
                v(40.0, 10.0),
                params,
                HAlign::Center,
                VAlign::Bottom,
                shape(),
            )
            .unwrap();
        sk.solve();
        t
    };
    s.observe_sketch(false);
    (s, t)
}

fn sketch(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
}

fn choose(s: &mut AppState, kind: ToolKind) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.choose_kind(sk, kind);
}

/// Klick über `commit_input` wie die Oberfläche, danach der Verlauf.
fn click(s: &mut AppState, at: DVec2) -> Result<(), String> {
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    se.update_cursor(&sf.sketch, Some(at), PICK);
    let result = se.commit_input(sf, at, PICK, false, &scope);
    s.observe_sketch(false);
    result
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn glyphs(s: &AppState, t: TextId) -> HashSet<CurveId> {
    sketch(s).text(t).unwrap().curves.iter().copied().collect()
}

/// Keine Abhängigkeit an Buchstaben, und keine an `p`.
fn free_of(s: &AppState, t: TextId, p: PointId) {
    let own = glyphs(s, t);
    for (_, k) in sketch(s).constraints() {
        assert!(
            !k.kind.curves().iter().any(|c| own.contains(c)),
            "an Buchstaben: {:?}",
            k.kind
        );
        assert!(!k.kind.points().contains(&p), "am Ende: {:?}", k.kind);
    }
}

#[test]
fn a_line_ending_on_a_glyph_has_a_free_end() {
    let (mut s, t) = with_text();
    let steps = undo_len(&s);
    let curves = sketch(&s).curves().count();
    set_tool(&mut s, Tool::Line);
    click(&mut s, v(10.0, 20.0)).unwrap();
    // Genau auf der linken Kante des Buchstabens.
    click(&mut s, v(18.0, 2.7)).expect("Linie bis an den Buchstaben");
    set_tool(&mut s, Tool::Select);
    s.observe_sketch(false);
    let sk = sketch(&s);
    assert_eq!(sk.curves().count(), curves + 1, "eine Linie");
    let (line, data) = sk.curves().last().unwrap();
    assert!(!glyphs(&s, t).contains(&line));
    let CurveKind::Line { b, .. } = data.kind else {
        panic!("Linie erwartet");
    };
    free_of(&s, t, b);
    assert_eq!(undo_len(&s), steps + 1, "ein Rückgängig-Schritt");
}

#[test]
fn fillet_on_a_glyph_shows_the_text_message() {
    let (mut s, _) = with_text();
    let before = sketch(&s).to_data();
    let steps = undo_len(&s);
    choose(&mut s, ToolKind::Fillet);
    let e = click(&mut s, v(18.0, 2.7)).expect_err("Buchstabe abgelehnt");
    assert!(e.contains(&t("sketch-text-no-fillet")), "{e}");
    assert!(s.sketch_session.as_ref().unwrap().pending_curves.is_empty());
    assert!(sketch(&s).to_data() == before);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn circle_3t_on_a_glyph_is_refused() {
    let (mut s, _) = with_text();
    let before = sketch(&s).to_data();
    choose(&mut s, ToolKind::Circle3T);
    let e = click(&mut s, v(18.0, 2.7)).expect_err("Buchstabe abgelehnt");
    assert!(e.contains(&t("sketch-text-no-dimension")), "{e}");
    assert!(s.sketch_session.as_ref().unwrap().pending_curves.is_empty());
    assert!(sketch(&s).to_data() == before);
}

#[test]
fn offset_on_a_glyph_is_refused() {
    let (mut s, _) = with_text();
    let before = sketch(&s).to_data();
    set_tool(&mut s, Tool::Offset);
    let e = click(&mut s, v(18.0, 2.7)).expect_err("Buchstabe abgelehnt");
    assert!(e.contains(&t("sketch-text-no-offset")), "{e}");
    assert!(s.sketch_session.as_ref().unwrap().pending_curves.is_empty());
    assert!(sketch(&s).to_data() == before);
}
