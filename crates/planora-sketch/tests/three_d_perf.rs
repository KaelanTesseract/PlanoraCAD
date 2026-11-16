//! Messtests Skizzen 10 (3D-Skizze): Die 2D-Rechnung darf durch die 3D-Fassung nicht langsamer
//! werden, und eine lange 3D-Kette muss bedienbar bleiben.
//! `cargo test -p planora-sketch --release --test three_d_perf -- --ignored --nocapture`

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::import::{InsertOptions, Placement};
use planora_sketch::model::Sketch;
use planora_sketch::solver::{Drag, SolveStatus};
use std::time::Instant;

mod common;
use common::{NoText, arc_chain, chain_2d, chain_3d};

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

/// Die Aufteilung einer Zeit als Text: Jacobi / Normalgleichungen / Cholesky / Rang / Rest.
fn split(t: &planora_sketch::solver::SolveTimings) -> String {
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1e3;
    let rest = ms(t.total) - ms(t.assemble) - ms(t.normal) - ms(t.cholesky) - ms(t.rank);
    format!(
        "Jacobi {:.0} ms / Normalgl. {:.0} ms / Cholesky {:.0} ms / Rang {:.0} ms / Rest {rest:.0} ms",
        ms(t.assemble),
        ms(t.normal),
        ms(t.cholesky),
        ms(t.rank),
    )
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
    let (r, solve_split) = s.solve_timed();
    let solve = t.elapsed().as_secs_f64();
    let at = s.point_xyz(last).unwrap();
    let t = Instant::now();
    let (rep, drag_split) = s
        .drag_timed(&[Drag {
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
    println!("PERF3D Aufteilung lösen: {}", split(&solve_split));
    println!("PERF3D Aufteilung Zugschritt: {}", split(&drag_split));
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    // Messziele A10 (Skizzen 11a, Task 3).
    assert!(solve <= 0.5, "{solve}");
    assert!(drag <= 0.05, "{drag}");
}

/// 200 verkettete ebene Linien und Bögen (100 + 100) mit Coincident und Tangent: dieselbe Kette
/// wie in 3D, aber in der Ebene. Aufbauen, lösen, ein Zugschritt `+(0.1, 0.1)`.
#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn perf_200_chained_2d() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let t = Instant::now();
    let (last, curves) = chain_2d(&mut s, 100);
    let build = t.elapsed().as_secs_f64();
    assert_eq!(curves, 200);
    let t = Instant::now();
    let (r, solve_split) = s.solve_timed();
    let solve = t.elapsed().as_secs_f64();
    let at = s.point_xy(last).unwrap();
    let t = Instant::now();
    let (rep, drag_split) = s
        .drag_timed(&[Drag {
            point: last,
            target: DVec3::new(at.x + 0.1, at.y + 0.1, 0.0),
            plane: None,
        }])
        .unwrap();
    let drag = t.elapsed().as_secs_f64();
    println!(
        "PERF2D-KETTE 200 verkettete ebene Kurven: aufbauen {build:.3} s, lösen {solve:.3} s \
         ({:?}), ein Zugschritt {drag:.3} s ({:?})",
        r.status, rep.status
    );
    println!("PERF2D-KETTE Aufteilung lösen: {}", split(&solve_split));
    println!("PERF2D-KETTE Aufteilung Zugschritt: {}", split(&drag_split));
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
}

/// Schwelle `SPARSE_MIN_VARS` (Skizzen 11a, Task 3): `chain_2d` mit 1 bis 16 Stücken (je 10
/// Variablen in einer Gruppe) einmal dicht und einmal dünn lösen und danach einen Zugschritt
/// rechnen, je der Median aus 21 Läufen. Die Schwelle ist die kleinste Größe, ab der der dünne
/// Weg beim Lösen und beim Zugschritt schneller ist, auf die nächste Zehnerzahl aufgerundet.
#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn sparse_threshold_crossover() {
    use planora_sketch::sparse::SolverPath;
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2] * 1e3
    };
    let run = |segments: usize, path: SolverPath| {
        let (mut solve, mut drag) = (Vec::new(), Vec::new());
        for _ in 0..21 {
            let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
            let (last, _) = chain_2d(&mut s, segments);
            s.set_solver_path(path);
            let t = Instant::now();
            s.solve();
            solve.push(t.elapsed().as_secs_f64());
            let at = s.point_xy(last).unwrap();
            let t = Instant::now();
            s.drag(&[Drag {
                point: last,
                target: DVec3::new(at.x + 0.1, at.y + 0.1, 0.0),
                plane: None,
            }])
            .unwrap();
            drag.push(t.elapsed().as_secs_f64());
        }
        (median(solve), median(drag))
    };
    for segments in 1..=16 {
        let (ds, dd) = run(segments, SolverPath::Dense);
        let (ss, sd) = run(segments, SolverPath::Sparse);
        println!(
            "SCHWELLE {:3} Variablen: lösen dicht {ds:7.3} / dünn {ss:7.3} ms, \
             Zug dicht {dd:7.3} / dünn {sd:7.3} ms{}",
            segments * 10,
            if ss < ds && sd < dd {
                "  dünn schneller"
            } else {
                ""
            }
        );
    }
}

/// Teurer Fall der dünnen Freiheitsprüfung (`column_free`): `chain_2d(100)`, danach erst ein
/// `Fixed` am Anfang und 50 Längenmaße an den ersten Linien. Die bestimmenden Zeilen kommen also
/// spät; jede bestimmte Spalte rechnet bis weit ans Ende der Hülle. Lösen dicht gegen dünn, je
/// der Median aus 3 Läufen.
#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn sparse_free_check_with_late_dimensions() {
    use planora_sketch::constraint::ConstraintKind as K;
    use planora_sketch::model::CurveKind;
    use planora_sketch::sparse::SolverPath;
    let build = || {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        chain_2d(&mut s, 100);
        let (first, _) = s.points().next().unwrap();
        let at = s.point_xy(first).unwrap();
        s.add_constraint(K::Fixed(first, at.x, at.y)).unwrap();
        let lines: Vec<_> = s
            .curves()
            .filter_map(|(_, c)| match c.kind {
                CurveKind::Line { a, b } => Some((a, b)),
                _ => None,
            })
            .take(50)
            .collect();
        for (a, b) in lines {
            s.add_constraint(K::Distance(a, b, 2.0)).unwrap();
        }
        s
    };
    let run = |path: SolverPath| {
        let mut out: Vec<(f64, usize)> = (0..3)
            .map(|_| {
                let mut s = build();
                s.set_solver_path(path);
                let t = Instant::now();
                let (r, _) = s.solve_timed();
                assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
                (t.elapsed().as_secs_f64(), r.dof)
            })
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out[1]
    };
    let mut s = build();
    s.set_solver_path(SolverPath::Sparse);
    let (_, t) = s.solve_timed();
    let (dense, dd) = run(SolverPath::Dense);
    let (sparse, sd) = run(SolverPath::Sparse);
    assert_eq!(dd, sd);
    println!(
        "FREIPRUEFUNG chain_2d(100) + Fixed + 50 Maße: lösen dicht {:.3} s, dünn {:.3} s \
         (dof {sd}; Rang dünn {:.0} ms)",
        dense,
        sparse,
        t.rank.as_secs_f64() * 1e3
    );
}
