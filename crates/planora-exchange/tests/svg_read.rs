//! Tests des SVG-Lesers (Skizzen 9, Task 4).

use planora_exchange::import2d::{Element2d, Import2d, Unit2d};
use planora_exchange::{ExchangeError, MAX_IMPORT_ELEMENTS, read_svg};
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
