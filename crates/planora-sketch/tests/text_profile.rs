//! Profile aus Text ohne Schriften (Skizzen 8, Task 9): leere Innenflächen nach der Füllregel
//! „nicht null“ und [`Sketch::text_of_profile`]. Die Prüfung mit echten Glyphen steht in
//! `planora-text/tests/real_glyph_profiles.rs`.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use std::sync::Arc;

/// Vorschub je Zeichen (Einheiten der Höhe).
const ADV: f64 = 1.0;
const H: f64 = 5.0;
const DESC: f64 = -0.2;
/// Unten ausgerichtet steht die Grundlinie um die Unterlänge über der Grundlinie des Rahmens.
const BASE: f64 = -DESC * H;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Quadrat von `lo` bis `hi`, gegen den Uhrzeigersinn oder (`cw`) im Uhrzeigersinn.
fn square(lo: DVec2, hi: DVec2, cw: bool) -> Contour {
    let mut pts = vec![v(hi.x, lo.y), hi, v(lo.x, hi.y), lo];
    if cw {
        pts = vec![v(lo.x, hi.y), hi, v(hi.x, lo.y), lo];
    }
    Contour {
        start: lo,
        segs: pts.into_iter().map(Seg::Line).collect(),
    }
}

/// Je Zeichen ein Quadrat mit innerem Quadrat: bei „o“ wie in einer Schrift gegenläufig (ein
/// Loch), bei „x“ gleichläufig (nach „nicht null“ Tinte, keine leere Innenfläche).
fn shape(content: &str) -> TextShape {
    let glyphs: Vec<Glyph> = content
        .chars()
        .enumerate()
        .map(|(i, ch)| Glyph {
            origin: v(i as f64 * ADV, 0.0),
            advance: ADV,
            contours: vec![
                square(v(0.0, 0.0), v(0.8, 0.8), false),
                square(v(0.2, 0.2), v(0.6, 0.6), ch == 'o'),
            ],
        })
        .collect();
    TextShape {
        lines: vec![ShapedLine {
            glyphs,
            width: content.chars().count() as f64 * ADV,
        }],
        ascender: 0.9,
        descender: DESC,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    }
}

fn text(s: &mut Sketch, content: &str, at: DVec2) -> TextId {
    let p = TextParams {
        content: content.into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: H,
        flip_h: false,
        flip_v: false,
    };
    let sh = Arc::new(shape(content));
    s.add_text_box(at, at + v(20.0, 10.0), p, HAlign::Left, VAlign::Bottom, sh)
        .unwrap()
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Mitte des Innenquadrats der Glyphe `k` eines Textes am Ursprung.
fn inner_mid(k: usize) -> DVec2 {
    v((k as f64 * ADV + 0.4) * H, 0.4 * H + BASE)
}

#[test]
fn reversed_inner_contour_is_a_hole_not_a_profile() {
    let mut s = sketch();
    let t = text(&mut s, "oo", DVec2::ZERO);
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    // Nur die Ringe, das Innere ist leer wie in einer Schrift.
    assert_eq!(e.profiles.len(), 2);
    assert!(e.profiles.iter().all(|p| p.holes.len() == 1));
    assert!(e.refs.iter().all(|r| s.text_of_profile(r) == Some(t)));
    assert_eq!(e.profile_at(inner_mid(0)), None);
    assert!(e.profile_at(v(0.1 * H, 0.1 * H + BASE)).is_some());
}

#[test]
fn same_direction_inner_contour_is_ink() {
    let mut s = sketch();
    let t = text(&mut s, "x", DVec2::ZERO);
    let e = s.profiles();
    // „nicht null“: Umlaufzahl 2 im Inneren, also Tinte und ein eigenes Profil.
    assert_eq!(e.profiles.len(), 2);
    assert!(e.refs.iter().all(|r| s.text_of_profile(r) == Some(t)));
    assert!(e.profile_at(inner_mid(0)).is_some());
}

#[test]
fn mirrored_text_keeps_its_holes() {
    // Umkehren dreht den Umlaufsinn aller Konturen; das Innere bleibt leer.
    for (flip_h, flip_v) in [(true, false), (false, true), (true, true)] {
        let mut s = sketch();
        let t = text(&mut s, "o", DVec2::ZERO);
        let p = TextParams {
            content: "o".into(),
            style: s.text(t).unwrap().style.clone(),
            height: H,
            flip_h,
            flip_v,
        };
        let sh = Arc::new(shape("o"));
        s.edit_text_box(t, p, HAlign::Left, VAlign::Bottom, sh)
            .unwrap();
        let e = s.profiles();
        assert_eq!(e.profiles.len(), 1, "Umkehren {flip_h} {flip_v}");
        assert_eq!(e.profiles[0].holes.len(), 1);
    }
}

#[test]
fn foreign_curve_in_hole_keeps_the_rest_as_profile() {
    let mut s = sketch();
    let t = text(&mut s, "o", DVec2::ZERO);
    // Ein Kreis im leeren Inneren: Die Fläche zwischen Loch und Kreis grenzt an eine fremde
    // Kurve und bleibt ein Profil wie bisher, aber kein Textprofil.
    let m = inner_mid(0);
    let c = s.add_point(m.x, m.y).unwrap();
    let circle = s.add_circle(c, 0.5).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(
        e.profiles.len(),
        3,
        "Ring, Inneres mit Kreisloch, Kreisfläche"
    );
    let texts: Vec<Option<TextId>> = e.refs.iter().map(|r| s.text_of_profile(r)).collect();
    assert_eq!(texts.iter().filter(|x| **x == Some(t)).count(), 1);
    let around = e.profile_at(m + v(0.7, 0.0)).expect("Inneres ohne Kreis");
    assert_eq!(s.text_of_profile(&e.refs[around]), None);
    assert!(e.refs[around].holes.iter().all(|p| p.curve == circle));
}

#[test]
fn two_texts_overlapping_keep_mixed_faces() {
    let mut s = sketch();
    // Zweiter Text um eine halbe Glyphe schräg versetzt: Teilflächen, die an beide Texte grenzen,
    // sind keine Textprofile, und keine fällt als leeres Inneres weg.
    let a = text(&mut s, "o", DVec2::ZERO);
    let b = text(&mut s, "o", v(0.5 * H, 0.25 * H));
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert!(!e.profiles.is_empty());
    for r in &e.refs {
        let t = s.text_of_profile(r);
        assert!(t.is_none() || t == Some(a) || t == Some(b));
    }
    assert!(e.refs.iter().any(|r| s.text_of_profile(r).is_none()));
}
