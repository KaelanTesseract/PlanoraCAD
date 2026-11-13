//! Tests des SVG-Lesers (Skizzen 9, Task 4).

use planora_exchange::import2d::{Element2d, Import2d, Unit2d};
use planora_exchange::{CHECK_EVERY, ExchangeError, MAX_IMPORT_ELEMENTS, read_svg};
use planora_math::DVec2;
use std::path::PathBuf;

const NEVER: &dyn Fn() -> bool = &|| false;
/// Millimeter je SVG-Pixel.
const MM: f64 = 25.4 / 96.0;

fn data(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/svg")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{p:?}: {e}"))
}

fn read(name: &str) -> Import2d {
    read_svg(&data(name), NEVER).unwrap()
}

fn elements(imp: &Import2d) -> &[Element2d] {
    assert_eq!(imp.layers.len(), 1);
    &imp.layers[0].elements
}

fn near(a: DVec2, x: f64, y: f64) -> bool {
    (a.x - x).abs() < 1e-4 && (a.y - y).abs() < 1e-4
}

#[test]
fn rect_100px_is_26_458mm_wide() {
    let imp = read("rect.svg");
    assert_eq!(imp.units, Some(Unit2d::Millimeter));
    assert_eq!(imp.layers[0].name, "SVG");
    let (lo, hi) = imp.bounds().unwrap();
    assert!((hi.x - lo.x - 100.0 * MM).abs() < 1e-4, "{lo:?} {hi:?}");
    assert!((hi.x - lo.x - 26.458).abs() < 1e-3);
    assert!((hi.y - lo.y - 50.0 * MM).abs() < 1e-4);
    assert!((lo.x - 10.0 * MM).abs() < 1e-4);
}

#[test]
fn width_in_mm_with_viewbox_scales() {
    let imp = read("mm_viewbox.svg");
    let (lo, hi) = imp.bounds().unwrap();
    assert!((hi.x - lo.x - 100.0).abs() < 1e-3, "{lo:?} {hi:?}");
    assert!((hi.y - lo.y - 50.0).abs() < 1e-3);
}

#[test]
fn y_axis_is_flipped() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <path d="M 0 10 L 20 10" fill="none" stroke="black"/></svg>"#;
    let imp = read_svg(svg, NEVER).unwrap();
    let Element2d::Line(a, b) = &elements(&imp)[0] else {
        panic!("keine Linie");
    };
    assert!(near(*a, 0.0, -2.6458), "{a:?}");
    assert!(near(*b, 20.0 * MM, -2.6458), "{b:?}");
}

#[test]
fn nested_group_transforms_and_viewbox_offset() {
    // (60,60) -> scale 2 -> (120,120) -> translate 10 -> (130,120) -> viewBox-Versatz -50 -> (80,70)
    let imp = read("nested.svg");
    let Element2d::Line(a, b) = &elements(&imp)[0] else {
        panic!("keine Linie");
    };
    assert!(near(*a, 80.0 * MM, -70.0 * MM), "{a:?}");
    assert!(near(*b, 100.0 * MM, -70.0 * MM), "{b:?}");
}

#[test]
fn quad_and_cubic_kept_exact() {
    let imp = read("curves.svg");
    let el = elements(&imp);
    assert_eq!(el.len(), 2, "{el:?}");
    let Element2d::Quad(q) = &el[0] else {
        panic!("keine Quad: {el:?}");
    };
    assert!(near(q[0], 0.0, 0.0));
    assert!(near(q[1], 10.0 * MM, -20.0 * MM));
    assert!(near(q[2], 20.0 * MM, 0.0));
    let Element2d::Cubic(c) = &el[1] else {
        panic!("keine Cubic: {el:?}");
    };
    assert!(near(c[0], 20.0 * MM, 0.0));
    assert!(near(c[1], 30.0 * MM, -10.0 * MM));
    assert!(near(c[2], 40.0 * MM, -10.0 * MM));
    assert!(near(c[3], 50.0 * MM, 0.0));
}

#[test]
fn subpath_close_adds_line_only_if_gap() {
    let gap = read("close_gap.svg");
    assert_eq!(elements(&gap).len(), 4, "{:?}", elements(&gap));
    let Element2d::Line(a, b) = &elements(&gap)[3] else {
        panic!("keine Linie");
    };
    assert!(near(*a, 0.0, -10.0 * MM) && near(*b, 0.0, 0.0));
    let none = read("close_nogap.svg");
    assert_eq!(elements(&none).len(), 3, "{:?}", elements(&none));
}

#[test]
fn text_element_counted_as_skipped_text() {
    let imp = read("text_image.svg");
    assert_eq!(imp.skipped.texts, 1);
    assert_eq!(imp.element_count(), 1);
}

#[test]
fn image_element_counted() {
    let imp = read("text_image.svg");
    assert_eq!(imp.skipped.images, 1);
}

#[test]
fn svgz_reads() {
    let imp = read("rect.svgz");
    let plain = read("rect.svg");
    assert_eq!(imp, plain);
    assert!(imp.element_count() > 0);
}

#[test]
fn broken_input_is_an_error() {
    let err = read_svg(b"das ist kein SVG", NEVER).unwrap_err();
    assert!(matches!(err, ExchangeError::SvgBroken(_)), "{err:?}");
    let _ = err.to_string();
}

#[test]
fn clip_paths_are_ignored() {
    // Der Ausschnitt schneidet nichts ab, er wird auch nicht als Element gelesen.
    let imp = read("mixed.svg");
    assert!(imp.element_count() > 5);
    assert_eq!(imp.skipped.texts, 1);
}

fn many_paths(n: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    for _ in 0..n {
        s.push_str(r#"<path d="M0 0 L1 1" fill="none" stroke="black"/>"#);
    }
    s.push_str("</svg>");
    s.into_bytes()
}

#[test]
fn element_limit() {
    let ok = read_svg(&many_paths(MAX_IMPORT_ELEMENTS), NEVER).unwrap();
    assert_eq!(ok.element_count(), MAX_IMPORT_ELEMENTS);
    let err = read_svg(&many_paths(MAX_IMPORT_ELEMENTS + 1), NEVER).unwrap_err();
    assert_eq!(err, ExchangeError::TooManyElements);
}

#[test]
fn cancel_gives_no_partial_result() {
    let err = read_svg(&data("mixed.svg"), &|| true).unwrap_err();
    assert_eq!(err, ExchangeError::Cancelled);
}

#[test]
fn periodic_cancel_check_stops_a_long_read() {
    // Die erste Frage am Anfang liefert false; erst später (aus der Schleife) true.
    let calls = std::cell::Cell::new(0u64);
    let cancel = || {
        calls.set(calls.get() + 1);
        calls.get() > 1
    };
    let err = read_svg(&many_paths(3 * CHECK_EVERY as usize), &cancel).unwrap_err();
    assert_eq!(err, ExchangeError::Cancelled);
    assert_eq!(calls.get(), 2, "bricht beim ersten regelmäßigen Check ab");
}

/// Kette von `levels` Gruppen, jede fügt die vorige `fanout`-mal per `<use>` ein.
fn use_bomb(levels: usize, fanout: usize) -> Vec<u8> {
    let mut s = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><defs><path id="l0" d="M0 0 L1 1"/>"#,
    );
    for k in 1..=levels {
        s.push_str(&format!(r#"<g id="l{k}">"#));
        for _ in 0..fanout {
            s.push_str(&format!(r##"<use xlink:href="#l{}"/>"##, k - 1));
        }
        s.push_str("</g>");
    }
    s.push_str(&format!(r##"</defs><use xlink:href="#l{levels}"/></svg>"##));
    s.into_bytes()
}

#[test]
fn use_chain_bomb_is_rejected_quickly() {
    let t = std::time::Instant::now();
    let err = read_svg(&use_bomb(20, 10), NEVER).unwrap_err();
    assert_eq!(err, ExchangeError::TooManyElements);
    assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
}

#[test]
fn modest_use_reuse_is_fine() {
    let imp = read_svg(&use_bomb(3, 4), NEVER).unwrap();
    assert_eq!(imp.element_count(), 64);
}

#[test]
fn xml_entity_bomb_is_rejected_quickly() {
    let svg = r#"<?xml version="1.0"?>
<!DOCTYPE svg [
 <!ENTITY a "aaaaaaaaaa">
 <!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">
 <!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">
 <!ENTITY d "&c;&c;&c;&c;&c;&c;&c;&c;&c;&c;">
 <!ENTITY e "&d;&d;&d;&d;&d;&d;&d;&d;&d;&d;">
 <!ENTITY f "&e;&e;&e;&e;&e;&e;&e;&e;&e;&e;">
]>
<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><desc>&f;&f;&f;&f;&f;&f;&f;&f;&f;&f;</desc></svg>"#;
    let t = std::time::Instant::now();
    let err = read_svg(svg.as_bytes(), NEVER).unwrap_err();
    assert!(matches!(err, ExchangeError::SvgBroken(_)), "{err:?}");
    assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
}

#[test]
fn plain_doctype_without_subset_is_allowed() {
    let svg = r#"<?xml version="1.0"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><path d="M0 0 L1 1" fill="none" stroke="black"/></svg>"#;
    assert_eq!(read_svg(svg.as_bytes(), NEVER).unwrap().element_count(), 1);
}

/// Einfacher Zufallszahlengenerator (xorshift) mit festem Startwert.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn truncated_and_flipped_svg_never_panic() {
    let names = [
        "rect.svg",
        "mm_viewbox.svg",
        "nested.svg",
        "curves.svg",
        "text_image.svg",
        "mixed.svg",
        "rect.svgz",
    ];
    let files: Vec<Vec<u8>> = names.iter().map(|n| data(n)).collect();
    for bytes in &files {
        for i in 0..50 {
            let _ = read_svg(&bytes[..bytes.len() * i / 50], NEVER);
        }
    }
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for n in 0..500 {
        let mut bytes = files[n % files.len()].clone();
        for _ in 0..1 + rng.next() % 8 {
            let at = (rng.next() % bytes.len() as u64) as usize;
            bytes[at] = rng.next() as u8;
        }
        let _ = read_svg(&bytes, NEVER);
    }
}

/// Wie `use_bomb`, aber jedes `use` hat zwei Verweise: usvg nimmt `href` vor `xlink:href`.
/// `big` ist der Verweis, den usvg wirklich nimmt; `tiny` ist der Köder.
fn decoy_bomb(big_first: bool, href: impl Fn(usize) -> String) -> Vec<u8> {
    let mut s = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><defs><path id="l0" d="M0 0 L1 1"/>"#,
    );
    for k in 1..=20 {
        s.push_str(&format!(r#"<g id="l{k}">"#));
        for _ in 0..10 {
            let big = href(k - 1);
            if big_first {
                s.push_str(&format!(r##"<use href="{big}" xlink:href="#l0"/>"##));
            } else {
                s.push_str(&format!(r##"<use xlink:href="#l0" href="{big}"/>"##));
            }
        }
        s.push_str("</g>");
    }
    s.push_str(&format!(r#"</defs><use href="{}"/></svg>"#, href(20)));
    s.into_bytes()
}

#[test]
fn use_href_takes_precedence_over_xlink_href() {
    for first in [true, false] {
        let t = std::time::Instant::now();
        let err = read_svg(&decoy_bomb(first, |k| format!("#l{k}")), NEVER).unwrap_err();
        assert_eq!(err, ExchangeError::TooManyElements);
        assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
    }
}

#[test]
fn use_href_with_spaces_resolves_like_usvg() {
    for form in [" #l{}", "#l{} ", "  #l{}  ", "\t#l{}"] {
        let t = std::time::Instant::now();
        let err = read_svg(
            &decoy_bomb(true, |k| form.replace("{}", &k.to_string())),
            NEVER,
        )
        .unwrap_err();
        assert_eq!(err, ExchangeError::TooManyElements, "Form {form:?}");
        assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
    }
}

#[test]
fn use_href_with_spaces_imports_the_right_geometry() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <defs><path id="a" d="M 0 0 L 10 0" fill="none" stroke="black"/></defs>
        <use href=" #a "/><use href="#a" transform="translate(0,20)"/></svg>"##;
    let imp = read_svg(svg, NEVER).unwrap();
    let el = elements(&imp);
    assert_eq!(el.len(), 2, "{el:?}");
    let Element2d::Line(a, b) = &el[0] else {
        panic!("keine Linie");
    };
    assert!(near(*a, 0.0, 0.0) && near(*b, 10.0 * MM, 0.0));
    let Element2d::Line(a, _) = &el[1] else {
        panic!("keine Linie");
    };
    assert!(near(*a, 0.0, -20.0 * MM), "{a:?}");
}

#[test]
fn marker_bomb_finishes_quickly_and_marker_is_skipped() {
    // Ein Pfad mit 90 000 Punkten, jeder Punkt bekäme eine Markierung mit 100 Formen.
    let mut d = String::from("M 0 0");
    for i in 1..=90_000 {
        d.push_str(&format!(" L {i} 1"));
    }
    let mut marker = String::new();
    for _ in 0..100 {
        marker.push_str(r#"<rect x="0" y="0" width="1" height="1"/>"#);
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs><marker id="m" markerWidth="2" markerHeight="2">{marker}</marker></defs><path d="{d}" fill="none" stroke="black" marker-mid="url(#m)"/></svg>"##
    );
    let t = std::time::Instant::now();
    let imp = read_svg(svg.as_bytes(), NEVER).unwrap();
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
    assert_eq!(imp.element_count(), 90_000);
    assert_eq!(imp.skipped.unknown, 1);
}

#[test]
fn arrow_marker_imports_only_the_path() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <defs><marker id="arrow" markerWidth="6" markerHeight="6" refX="3" refY="3" orient="auto">
            <path d="M 0 0 L 6 3 L 0 6 Z"/></marker></defs>
        <path d="M 10 10 L 50 10" fill="none" stroke="black" marker-end="url(#arrow)"/>
        <path d="M 10 20 L 50 20" fill="none" stroke="black" style="marker-end:url(#arrow)"/>
        </svg>"##;
    let imp = read_svg(svg, NEVER).unwrap();
    assert_eq!(imp.element_count(), 2, "{:?}", elements(&imp));
    assert_eq!(imp.skipped.unknown, 1);
}

// ---- Gesamtprüfung I4: Muster, Clip-Pfade, Masken und Filter werden ausgeschnitten ----------

/// Elemente eines einzelnen gefüllten Rechtecks (Vergleichswert).
fn per_rect() -> usize {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect x="0" y="0" width="1" height="1" fill="black"/></svg>"##;
    read_svg(svg, NEVER).unwrap().element_count()
}

/// `n` Rechtecke mit den Attributen `attrs`, nebeneinander.
fn rects(n: usize, attrs: &str) -> String {
    let mut out = String::new();
    for i in 0..n {
        out.push_str(&format!(
            r#"<rect x="{}" y="0" width="1" height="1" {attrs}/>"#,
            i * 2
        ));
    }
    out
}

#[test]
fn pattern_bomb_finishes_quickly_and_shapes_keep_their_outline() {
    // 5 000 Flächen nutzen ein Muster mit 5 000 Formen: usvg kopierte das Muster je Fläche
    // (25 Millionen Formen). Ausgeschnitten bleibt jede Fläche mit ihrem Umriss.
    let n = 5_000;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs><pattern id="p" width="1" height="1">{}</pattern></defs>{}</svg>"##,
        rects(n, r#"fill="black""#),
        rects(n, r##"fill="url(#p)""##)
    );
    let t = std::time::Instant::now();
    let imp = read_svg(svg.as_bytes(), NEVER).unwrap();
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
    assert_eq!(imp.element_count(), n * per_rect());
    assert_eq!(imp.skipped.unknown, 1);
}

#[test]
fn clip_path_and_fe_image_bomb_finishes_quickly() {
    // Clip-Pfad in `objectBoundingBox` (je Nutzer neu umgesetzt) und ein Filter mit `feImage`
    // auf eine große Gruppe (je Filter neu umgesetzt), beide von 5 000 Formen genutzt.
    let n = 5_000;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><defs>
        <clipPath id="c" clipPathUnits="objectBoundingBox">{big}</clipPath>
        <g id="big">{big}</g>
        <filter id="f" filterUnits="objectBoundingBox"><feImage xlink:href="#big"/></filter>
        </defs>{clipped}{filtered}</svg>"##,
        big = rects(n, r#"fill="black""#),
        clipped = rects(n, r##"fill="black" clip-path="url(#c)""##),
        filtered = rects(n, r##"fill="black" filter="url(#f)""##),
    );
    let t = std::time::Instant::now();
    let imp = read_svg(svg.as_bytes(), NEVER).unwrap();
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
    // Beide Gruppen behalten ihre Umrisse (ein Filter mit verlorenem Ziel ließe usvg das
    // Element ganz weg).
    assert_eq!(imp.element_count(), 2 * n * per_rect());
    assert_eq!(imp.skipped.unknown, 2);
}

#[test]
fn shapes_with_mask_filter_or_pattern_in_style_keep_their_outline() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <defs>
          <mask id="m"><rect width="100" height="100" fill="white"/></mask>
          <filter id="f"><feGaussianBlur stdDeviation="2"/></filter>
          <pattern id="p" width="4" height="4"><circle r="1"/></pattern>
          <style>.q { fill: url(#p) }</style>
        </defs>
        <rect x="0" y="0" width="10" height="10" style="mask:url(#m)" fill="black"/>
        <rect x="20" y="0" width="10" height="10" style="filter:url(#f)" fill="black"/>
        <rect x="40" y="0" width="10" height="10" style="fill:url(#p)"/>
        <rect x="60" y="0" width="10" height="10" class="q"/>
        <rect x="80" y="0" width="10" height="10" stroke="url(#p)" fill="none"/>
        </svg>"##;
    let imp = read_svg(svg, NEVER).unwrap();
    assert_eq!(imp.element_count(), 5 * per_rect(), "{:?}", elements(&imp));
    assert_eq!(imp.skipped.unknown, 3);
}
