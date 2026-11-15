//! Messtests Skizzen 10 (3D-Skizze): Die 2D-Rechnung darf durch die 3D-Fassung nicht langsamer
//! werden, und eine lange 3D-Kette muss bedienbar bleiben.
//! `cargo test -p planora-sketch --release --test three_d_perf -- --ignored --nocapture`

use planora_exchange::import2d::{Element2d, Import2d, Layer2d};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::import::{InsertOptions, Placement};
use planora_sketch::model::Sketch;
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::text_shape::{TextShape, TextShapeError, TextShaper, TextStyle};
use std::time::Instant;

struct NoText;

impl TextShaper for NoText {
    fn shape(&self, _: &str, _: &TextStyle) -> Result<TextShape, TextShapeError> {
        Err(TextShapeError::Empty)
    }
}

/// Eine Kette aus `n` Halbkreisen (wie eine Polylinie mit Bulge): ein einziger Zusammenhang.
/// Dieselbe Skizze wie `chained_bulge_arcs_insert_and_drag` in `import_perf.rs`.
fn arc_chain(n: usize) -> Import2d {
    let elements = (0..n)
        .map(|i| Element2d::Arc {
            c: DVec2::new(2.0 * i as f64 + 1.0, 0.0),
            r: 1.0,
            start: std::f64::consts::PI,
            end: std::f64::consts::TAU,
        })
        .collect();
    Import2d {
        layers: vec![Layer2d {
            name: "0".into(),
            elements,
        }],
        ..Default::default()
    }
}

/// Reine 2D-Skizze: 500 verkettete Bögen einfügen und einen Zugschritt am Ende rechnen.
/// Vergleichswert (Skizzen 9, `offene-punkte.md`): 10,9 s einfügen, 16,9 s Zugschritt, jeweils
/// auf der damaligen Maschine; maßgeblich ist der Lauf desselben Tests auf dem Stand vor dem
/// Zweig (höchstens +5 %).
#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn perf_2d_unchanged() {
    let imp = arc_chain(500);
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
    assert_eq!(r.curves, 500);
    let (last, _) = s.points().last().expect("Punkte");
    let at = s.point_xy(last).unwrap();
    let t = Instant::now();
    let rep = s
        .drag(&[Drag {
            point: last,
            target: (at + DVec2::new(0.1, 0.1)).extend(0.0),
            plane: None,
        }])
        .unwrap();
    let drag = t.elapsed().as_secs_f64();
    println!(
        "PERF2D 500 verkettete Bögen: einfügen {insert:.3} s, ein Zugschritt {drag:.3} s ({:?})",
        rep.status
    );
}

/// Kette aus `segments` Viertelkreis-Stücken im Raum: Linie (Länge 2), dann ein Viertelbogen
/// (Radius 1), der die Richtung x → y → z → x → … dreht. Linien und Bögen haben getrennte
/// Punkte, die mit `Coincident` verbunden sind; an jedem Übergang gilt `Tangent`. Die Punkte
/// werden leicht verstellt, damit der Löser wirklich rechnen muss.
fn chain_3d(s: &mut Sketch, segments: usize) -> (planora_sketch::model::PointId, usize) {
    let axes = [DVec3::X, DVec3::Y, DVec3::Z];
    let noise = |i: usize, k: usize| {
        let f = (i * 7 + k * 13) as f64;
        DVec3::new(f.sin(), (f * 1.7).cos(), (f * 2.3).sin()) * 1e-3
    };
    let mut pos = DVec3::ZERO;
    let mut prev_end: Option<planora_sketch::model::PointId> = None;
    let mut prev_arc = None;
    let mut last = None;
    let mut curves = 0;
    let mut k = 0;
    let mut p3 = |s: &mut Sketch, p: DVec3| {
        k += 1;
        s.add_point3(p + noise(k, k / 3)).unwrap()
    };
    for i in 0..segments {
        let h = axes[i % 3];
        let h2 = axes[(i + 1) % 3];
        // Linie von `pos` in Richtung h.
        let a = p3(s, pos);
        let b_pos = pos + 2.0 * h;
        let b = p3(s, b_pos);
        let line = s.add_line(a, b).unwrap();
        curves += 1;
        if let (Some(e), Some(arc)) = (prev_end, prev_arc) {
            s.add_constraint(K::Coincident(e, a)).unwrap();
            s.add_constraint(K::Tangent(arc, line)).unwrap();
        }
        // Viertelbogen von b: Mitte b + h2, Ende b + h + h2.
        let st = p3(s, b_pos);
        let m = p3(s, b_pos + h2);
        let e = p3(s, b_pos + h + h2);
        let arc = s.add_arc3(m, st, e, h.cross(h2)).unwrap();
        curves += 1;
        s.add_constraint(K::Coincident(b, st)).unwrap();
        s.add_constraint(K::Tangent(line, arc)).unwrap();
        prev_end = Some(e);
        prev_arc = Some(arc);
        last = Some(e);
        pos = b_pos + h + h2;
    }
    (last.unwrap(), curves)
}

/// 200 verkettete 3D-Linien und -Bögen (100 + 100) mit Coincident und Tangent: aufbauen, lösen und
/// einen Zugschritt in der Zugebene rechnen. Die Zeiten stehen in der Notiz.
#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn perf_200_chained_3d() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let t = Instant::now();
    let (last, curves) = chain_3d(&mut s, 100);
    let build = t.elapsed().as_secs_f64();
    assert_eq!(curves, 200);
    let t = Instant::now();
    let r = s.solve();
    let solve = t.elapsed().as_secs_f64();
    let at = s.point_xyz(last).unwrap();
    let t = Instant::now();
    let rep = s
        .drag(&[Drag {
            point: last,
            target: at + DVec3::new(0.1, 0.1, 0.1),
            plane: Some((at, DVec3::Z)),
        }])
        .unwrap();
    let drag = t.elapsed().as_secs_f64();
    println!(
        "PERF3D 200 verkettete 3D-Kurven: aufbauen {build:.3} s, lösen {solve:.3} s ({:?}), \
         ein Zugschritt {drag:.3} s ({:?})",
        r.status, rep.status
    );
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
}
