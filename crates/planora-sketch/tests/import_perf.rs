//! Messtest: 10 000 Linien einfügen (Skizzen 9, Spec B „Leistung“).
//! `cargo test -p planora-sketch --release --test import_perf -- --ignored --nocapture`

use planora_exchange::import2d::{Element2d, Import2d, Layer2d};
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::import::{InsertOptions, Placement};
use planora_sketch::model::Sketch;
use planora_sketch::text_shape::{TextShape, TextShapeError, TextShaper, TextStyle};
use std::time::Instant;

struct NoText;

impl TextShaper for NoText {
    fn shape(&self, _: &str, _: &TextStyle) -> Result<TextShape, TextShapeError> {
        Err(TextShapeError::Empty)
    }
}

/// 2500 Quadrate (50 × 50) aus je vier Linien: 10 000 Linien, 2500 Profile.
fn squares() -> Import2d {
    let mut elements = Vec::with_capacity(10_000);
    for i in 0..50 {
        for j in 0..50 {
            let o = DVec2::new(i as f64 * 3.0, j as f64 * 3.0);
            let p = [
                o,
                o + DVec2::new(2.0, 0.0),
                o + DVec2::new(2.0, 2.0),
                o + DVec2::new(0.0, 2.0),
            ];
            for k in 0..4 {
                elements.push(Element2d::Line(p[k], p[(k + 1) % 4]));
            }
        }
    }
    Import2d {
        layers: vec![Layer2d {
            name: "0".into(),
            elements,
        }],
        ..Default::default()
    }
}

fn inserted(fixed: bool) -> (Sketch, f64) {
    let imp = squares();
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let opt = InsertOptions {
        fixed,
        ..Default::default()
    };
    let t = Instant::now();
    let r = s
        .insert_geometry(&imp, &Placement::default(), &opt, &NoText)
        .unwrap();
    let secs = t.elapsed().as_secs_f64();
    assert_eq!(r.curves, 10_000);
    assert_eq!(r.points, 10_000);
    (s, secs)
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn ten_thousand_lines_insert_under_2s() {
    for fixed in [false, true] {
        let (_, secs) = inserted(fixed);
        println!("10 000 Linien einfügen (fest: {fixed}): {:.3} s", secs);
        assert!(secs < 2.0, "{secs} s");
    }
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn ten_thousand_lines_profiles_measured() {
    let (s, _) = inserted(false);
    let t = Instant::now();
    let p = s.profiles();
    let secs = t.elapsed().as_secs_f64();
    println!(
        "Profile von 10 000 Linien: {} Profile in {:.3} s",
        p.profiles.len(),
        secs
    );
    assert_eq!(p.profiles.len(), 2500);
}

// ---- Gesamtprüfung I7: Messungen an der Grenze von 100 000 Elementen -------------------------

/// `n` Quadrate aus je vier Linien in einem Raster (4·n Linien).
fn many_squares(n: usize) -> Import2d {
    let side = (n as f64).sqrt().ceil() as usize;
    let mut elements = Vec::with_capacity(4 * n);
    for k in 0..n {
        let o = DVec2::new((k % side) as f64 * 3.0, (k / side) as f64 * 3.0);
        let p = [
            o,
            o + DVec2::new(2.0, 0.0),
            o + DVec2::new(2.0, 2.0),
            o + DVec2::new(0.0, 2.0),
        ];
        for i in 0..4 {
            elements.push(Element2d::Line(p[i], p[(i + 1) % 4]));
        }
    }
    one(elements)
}

fn one(elements: Vec<Element2d>) -> Import2d {
    Import2d {
        layers: vec![Layer2d {
            name: "0".into(),
            elements,
        }],
        ..Default::default()
    }
}

/// Eine Kette aus `n` Halbkreisen (wie eine Polylinie mit Bulge): ein einziger Zusammenhang.
fn arc_chain(n: usize) -> Import2d {
    let elements = (0..n)
        .map(|i| Element2d::Arc {
            c: DVec2::new(2.0 * i as f64 + 1.0, 0.0),
            r: 1.0,
            start: std::f64::consts::PI,
            end: std::f64::consts::TAU,
        })
        .collect();
    one(elements)
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn hundred_thousand_lines_at_the_limit() {
    let imp = many_squares(25_000);
    assert_eq!(imp.element_count(), 100_000);
    let t = Instant::now();
    let lines = planora_sketch::import::preview_polylines(&imp, &Placement::default(), &[], 0.01);
    println!(
        "Vorschau 100 000 Linien: {} Züge in {:.3} s",
        lines.len(),
        t.elapsed().as_secs_f64()
    );
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let t = Instant::now();
    let r = s
        .insert_geometry(
            &imp,
            &Placement::default(),
            &InsertOptions::default(),
            &NoText,
        )
        .unwrap();
    println!(
        "100 000 Linien einfügen: {} Kurven in {:.3} s",
        r.curves,
        t.elapsed().as_secs_f64()
    );
    let t = Instant::now();
    let p = s.profiles();
    println!(
        "Profile von 100 000 Linien: {} Profile in {:.3} s",
        p.profiles.len(),
        t.elapsed().as_secs_f64()
    );
    assert_eq!(p.profiles.len(), 25_000);
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn hundred_thousand_cubics_preview() {
    let elements = (0..100_000)
        .map(|i| {
            let o = DVec2::new((i % 300) as f64 * 3.0, (i / 300) as f64 * 3.0);
            Element2d::Cubic([
                o,
                o + DVec2::new(0.5, 1.0),
                o + DVec2::new(1.5, -1.0),
                o + DVec2::new(2.0, 0.0),
            ])
        })
        .collect();
    let imp = one(elements);
    let t = Instant::now();
    let lines = planora_sketch::import::preview_polylines(&imp, &Placement::default(), &[], 0.01);
    println!(
        "Vorschau 100 000 Kubiken: {} Züge in {:.3} s",
        lines.len(),
        t.elapsed().as_secs_f64()
    );
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn chained_bulge_arcs_insert_and_drag() {
    // Eine Kette ist eine einzige Lösergruppe (dichte Matrizen): Die Zeit wächst stark mit der
    // Länge. Gemessen wird steigend, bis ein Schritt länger als 30 s dauert.
    for n in [100, 250, 500, 1_000, 2_500] {
        let imp = arc_chain(n);
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let t = Instant::now();
        let r = s
            .insert_geometry(
                &imp,
                &Placement::default(),
                &InsertOptions::default(),
                &NoText,
            )
            .unwrap();
        let insert = t.elapsed().as_secs_f64();
        assert_eq!(r.curves, n);
        // Ein Zugschritt am Ende der Kette.
        let (last, _) = s.points().last().expect("Punkte");
        let at = s.point_xy(last).unwrap();
        let t = Instant::now();
        let rep = s.drag(&[planora_sketch::solver::Drag {
            point: last,
            target: (at + DVec2::new(0.1, 0.1)).extend(0.0),
            plane: None,
        }]);
        let drag = t.elapsed().as_secs_f64();
        println!(
            "{n} verkettete Bögen: einfügen {insert:.3} s, ein Zugschritt {drag:.3} s ({:?})",
            rep.map(|r| format!("{r:?}").chars().take(60).collect::<String>())
        );
        if insert.max(drag) > 30.0 {
            println!("Abbruch der Reihe: über 30 s");
            break;
        }
    }
}
