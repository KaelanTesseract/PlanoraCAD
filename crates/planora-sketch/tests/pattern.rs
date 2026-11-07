//! Muster mit gehaltenen Exemplaren (Skizzen 5, Task 3): reine Lage der Exemplare, Vorschau
//! gleich Ergebnis, Exemplare folgen der Vorlage über dichte Wertefolgen, der Löser hält die
//! Exemplare (kein Freiheitsgrad, keine Überbestimmung), Grenzlagen und alle Kurvenarten.

use planora_math::{DVec2, Frame, TAU, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::pattern::{
    CircularMode, PATTERN_MAX_INSTANCES, PatternDir, PatternId, PatternKind, PatternPlan,
    PatternProblem, PatternSlot,
};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};
use planora_sketch::transform::Similarity;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn solved(s: &mut Sketch) -> SolveReport {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.conflicting.is_empty(), "{r:?}");
    r
}

/// Rechteck 10×6 mit der Ecke unten links in `o`, ohne Maße (nur waagerecht/senkrecht).
fn rect(s: &mut Sketch, o: DVec2) -> Vec<CurveId> {
    s.add_rectangle(o, o + v(10.0, 6.0)).unwrap().lines.to_vec()
}

/// Rechteck 10×6 mit fester Ecke unten links und Maßen für Breite und Höhe: voll bestimmt.
/// Liefert Linien, Ecken und das Breitenmaß.
fn dimensioned_rect(s: &mut Sketch) -> (Vec<CurveId>, [PointId; 4], ConstraintId) {
    let ids = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let [p0, p1, _, p3] = ids.points;
    s.add_constraint(K::Fixed(p0, 0.0, 0.0)).unwrap();
    let w = s.add_constraint(K::Distance(p0, p1, 10.0)).unwrap();
    s.add_constraint(K::Distance(p0, p3, 6.0)).unwrap();
    solved(s);
    (ids.lines.to_vec(), ids.points, w)
}

fn rect_plan(sources: Vec<CurveId>, counts: [u32; 2], values: [f64; 2]) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources,
        counts,
        values,
        suppressed: Vec::new(),
    }
}

fn circ_plan(
    sources: Vec<CurveId>,
    center: PointId,
    mode: CircularMode,
    n: u32,
    angle: f64,
) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Circular { center, mode },
        sources,
        counts: [n, 1],
        values: [angle, 0.0],
        suppressed: Vec::new(),
    }
}

fn offsets(list: &[([i32; 2], Similarity)]) -> Vec<([i32; 2], DVec2)> {
    list.iter()
        .map(|(i, t)| (*i, t.apply(DVec2::ZERO)))
        .collect()
}

fn close(a: DVec2, b: DVec2, tol: f64) -> bool {
    a.distance(b) <= tol
}

/// Der Wert des Musters im Feld `slot`.
fn value_id(s: &Sketch, id: PatternId, slot: PatternSlot) -> ConstraintId {
    *s.pattern(id)
        .unwrap()
        .values
        .iter()
        .find(|&&c| matches!(s.constraint(c).unwrap().kind, K::PatternValue(_, x, _) if x == slot))
        .unwrap()
}

/// Jedes Exemplar ist die mit seiner Lage bewegte Vorlage (Punkte, Radien, Halbachsen).
fn assert_instances_follow(s: &Sketch, id: PatternId, tol: f64) {
    let data = s.pattern(id).unwrap();
    let lattice = s.pattern_transforms(&current_plan(s, id)).unwrap();
    assert_eq!(lattice.len(), data.instances.len());
    for inst in &data.instances {
        let t = lattice.iter().find(|(i, _)| *i == inst.at).unwrap().1;
        for (k, &q) in inst.points.iter().enumerate() {
            let want = t.apply(at(s, data.source_points[k]));
            assert!(
                close(at(s, q), want, tol),
                "{:?} {:?} {want:?}",
                inst.at,
                at(s, q)
            );
        }
        for (k, &c) in inst.curves.iter().enumerate() {
            let src = data.sources[k];
            if let (Some(r), Some(r0)) = (s.radius(c), s.radius(src)) {
                assert!((r - r0).abs() <= tol);
            }
            for i in 0..16 {
                let u = TAU * i as f64 / 16.0;
                if let (Some(p), Some(p0)) = (s.ellipse_point(c, u), s.ellipse_point(src, u)) {
                    assert!(close(p, t.apply(p0), tol), "Halbachsen {:?}", inst.at);
                }
            }
        }
    }
}

/// Der Plan eines angelegten Musters aus seinen Werten.
fn current_plan(s: &Sketch, id: PatternId) -> PatternPlan {
    let data = s.pattern(id).unwrap();
    let val = |slot| {
        data.values
            .iter()
            .find_map(|&c| match s.constraint(c).unwrap().kind {
                K::PatternValue(_, x, v) if x == slot => Some(v),
                _ => None,
            })
    };
    let (counts, values) = match data.kind {
        PatternKind::Rect { .. } => (
            [
                val(PatternSlot::Count1).unwrap() as u32,
                val(PatternSlot::Count2).unwrap() as u32,
            ],
            [
                val(PatternSlot::Spacing1).unwrap(),
                val(PatternSlot::Spacing2).unwrap(),
            ],
        ),
        PatternKind::Circular { .. } => (
            [val(PatternSlot::Count).unwrap() as u32, 1],
            [val(PatternSlot::Angle).unwrap_or(0.0), 0.0],
        ),
    };
    PatternPlan {
        kind: data.kind,
        sources: data.sources.clone(),
        counts,
        values,
        suppressed: data.suppressed.clone(),
    }
}

// ---- Lagen ---------------------------------------------------------------------------------

#[test]
fn rect_pattern_places_five_instances_at_the_lattice() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    let plan = rect_plan(src.clone(), [3, 2], [15.0, 10.0]);
    let got = offsets(&s.pattern_transforms(&plan).unwrap());
    let want = [
        ([1, 0], v(15.0, 0.0)),
        ([2, 0], v(30.0, 0.0)),
        ([0, 1], v(0.0, 10.0)),
        ([1, 1], v(15.0, 10.0)),
        ([2, 1], v(30.0, 10.0)),
    ];
    assert_eq!(got.len(), 5, "{got:?}");
    for (i, o) in want {
        let found = got
            .iter()
            .find(|(j, _)| *j == i)
            .unwrap_or_else(|| panic!("{i:?}"));
        assert!(close(found.1, o, 1e-12), "{i:?} {:?}", found.1);
    }
    // Angelegt: Exemplar (2,1) ist die um (30,10) verschobene Vorlage.
    let id = s.add_pattern(&plan).unwrap();
    let data = s.pattern(id).unwrap().clone();
    assert_eq!(data.instances.len(), 5);
    assert_eq!(data.sources, src);
    let inst = data.instances.iter().find(|i| i.at == [2, 1]).unwrap();
    for (k, &q) in inst.points.iter().enumerate() {
        let p = data.source_points[k];
        assert!(close(at(&s, q), at(&s, p) + v(30.0, 10.0), 1e-12));
    }
    assert_eq!(s.pattern_count(), 1);
    assert_eq!(s.patterns().count(), 1);
    assert_eq!(s.pattern_of_curve(src[0]), Some((id, None)));
    assert_eq!(s.pattern_of_curve(inst.curves[2]), Some((id, Some([2, 1]))));
}

#[test]
fn extent_symmetric_line_direction_and_negative_spacing() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    // Gesamtlänge 30 bei 3 ⇒ Schritt 15.
    let mut plan = rect_plan(src.clone(), [3, 1], [30.0, 10.0]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::SketchX, PatternDir::SketchY],
        extent: [true, false],
        symmetric: [false, false],
    };
    let got = offsets(&s.pattern_transforms(&plan).unwrap());
    assert_eq!(got.len(), 2);
    assert!(close(got[0].1, v(15.0, 0.0), 1e-12) && close(got[1].1, v(30.0, 0.0), 1e-12));

    // Symmetrisch X 3 ⇒ Indizes −2…2.
    let mut plan = rect_plan(src.clone(), [3, 1], [15.0, 10.0]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::SketchX, PatternDir::SketchY],
        extent: [false, false],
        symmetric: [true, false],
    };
    let got = offsets(&s.pattern_transforms(&plan).unwrap());
    let mut idx: Vec<i32> = got.iter().map(|(i, _)| i[0]).collect();
    idx.sort_unstable();
    assert_eq!(idx, vec![-2, -1, 1, 2]);
    for (i, o) in &got {
        assert!(close(*o, v(15.0 * i[0] as f64, 0.0), 1e-12));
    }

    // Richtung längs einer Linie unter 30°.
    let a = s.add_point(50.0, 50.0).unwrap();
    let dir = v(30f64.to_radians().cos(), 30f64.to_radians().sin());
    let b = s
        .add_point(50.0 + 20.0 * dir.x, 50.0 + 20.0 * dir.y)
        .unwrap();
    let l = s.add_line(a, b).unwrap();
    let mut plan = rect_plan(src.clone(), [3, 1], [15.0, 10.0]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::Line(l), PatternDir::SketchY],
        extent: [false, false],
        symmetric: [false, false],
    };
    for (i, o) in offsets(&s.pattern_transforms(&plan).unwrap()) {
        assert!(close(o, dir * (15.0 * i[0] as f64), 1e-12), "{i:?} {o:?}");
    }

    // Negativer Abstand kehrt um.
    let plan = rect_plan(src, [3, 1], [-15.0, 10.0]);
    let got = offsets(&s.pattern_transforms(&plan).unwrap());
    assert!(close(got[0].1, v(-15.0, 0.0), 1e-12) && close(got[1].1, v(-30.0, 0.0), 1e-12));
}

fn angles(list: &[([i32; 2], Similarity)]) -> Vec<f64> {
    let mut a: Vec<f64> = list.iter().map(|(_, t)| t.angle.to_degrees()).collect();
    a.sort_by(f64::total_cmp);
    a
}

fn assert_angles(got: Vec<f64>, want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-9, "{got:?} {want:?}");
    }
}

#[test]
fn circular_pattern_full_angle_and_symmetric() {
    let mut s = sketch();
    let src = rect(&mut s, v(20.0, 0.0));
    let c = s.add_point(0.0, 0.0).unwrap();
    let full = s
        .pattern_transforms(&circ_plan(src.clone(), c, CircularMode::Full, 6, 0.0))
        .unwrap();
    assert_angles(angles(&full), &[60.0, 120.0, 180.0, 240.0, 300.0]);
    for (i, t) in &full {
        assert!((t.angle - i[0] as f64 * TAU / 6.0).abs() < 1e-12);
        assert!(
            close(t.apply(v(0.0, 0.0)), v(0.0, 0.0), 1e-12),
            "um die Mitte"
        );
    }
    let angle = s
        .pattern_transforms(&circ_plan(
            src.clone(),
            c,
            CircularMode::Angle,
            4,
            90f64.to_radians(),
        ))
        .unwrap();
    assert_angles(angles(&angle), &[30.0, 60.0, 90.0]);
    let sym = s
        .pattern_transforms(&circ_plan(
            src,
            c,
            CircularMode::Symmetric,
            3,
            90f64.to_radians(),
        ))
        .unwrap();
    assert_angles(angles(&sym), &[-45.0, -22.5, 22.5, 45.0]);
}

// ---- Vorschau = Ergebnis -------------------------------------------------------------------

#[test]
fn preview_equals_created_instances() {
    let mut s = sketch();
    let mut src = rect(&mut s, v(0.0, 0.0));
    let m = s.add_point(5.0, 3.0).unwrap();
    src.push(s.add_circle(m, 2.0).unwrap());
    let (c, a0, a1) = (
        s.add_point(5.0, 6.0).unwrap(),
        s.add_point(8.0, 6.0).unwrap(),
        s.add_point(2.0, 6.0).unwrap(),
    );
    src.push(s.add_arc(c, a0, a1).unwrap());
    solved(&mut s);
    let plans = {
        let mut rect = rect_plan(src.clone(), [3, 2], [15.0, 12.0]);
        rect.suppressed = vec![[1, 1]];
        let mut circ = circ_plan(
            src.clone(),
            s.add_point(-20.0, -5.0).unwrap(),
            CircularMode::Angle,
            5,
            4.0,
        );
        circ.suppressed = vec![[2, 0]];
        [rect, circ]
    };
    for plan in plans {
        let mut t = s.clone();
        let preview = t.pattern_preview(&plan, 64).unwrap();
        assert!(preview.iter().all(|(i, _)| !plan.suppressed.contains(i)));
        let id = t.add_pattern(&plan).unwrap();
        let data = t.pattern(id).unwrap().clone();
        assert_eq!(preview.len(), data.instances.len());
        for (i, lines) in &preview {
            let inst = data.instances.iter().find(|x| x.at == *i).unwrap();
            assert_eq!(lines.len(), inst.curves.len());
            for (pl, &c) in lines.iter().zip(&inst.curves) {
                let got = t.curve_polyline(c, 64).unwrap();
                assert_eq!(pl.len(), got.len());
                for (p, q) in pl.iter().zip(&got) {
                    assert!(close(*p, *q, 1e-9), "{i:?} {p:?} {q:?}");
                }
            }
        }
        assert!(
            data.instances
                .iter()
                .all(|x| !plan.suppressed.contains(&x.at))
        );
        assert_eq!(data.suppressed, plan.suppressed);
    }
}

// ---- Folgen --------------------------------------------------------------------------------

#[test]
fn instances_follow_the_source_width_over_a_dense_sweep() {
    let mut s = sketch();
    let (src, pts, w) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [3, 3], [15.0, 10.0]))
        .unwrap();
    for k in 0..=400 {
        let width = 1.0 + 39.0 * k as f64 / 400.0;
        s.set_constraint_value(w, width).unwrap();
        solved(&mut s);
        // Kein Umklappen: die rechte Ecke liegt rechts.
        assert!((at(&s, pts[1]).x - width).abs() < 1e-9, "{width}");
        assert_instances_follow(&s, id, 1e-9);
    }
}

#[test]
fn instances_follow_a_turning_direction_line() {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let a = s.add_point(-50.0, -50.0).unwrap();
    let b = s.add_point(-30.0, -50.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let mut plan = rect_plan(src, [3, 1], [15.0, 10.0]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::Line(l), PatternDir::SketchY],
        extent: [false, false],
        symmetric: [false, false],
    };
    let id = s.add_pattern(&plan).unwrap();
    for deg in -180..=180 {
        let r = (deg as f64).to_radians();
        s.set_point_xy(b, -50.0 + 20.0 * r.cos(), -50.0 + 20.0 * r.sin())
            .unwrap();
        solved(&mut s);
        let data = s.pattern(id).unwrap();
        let src0 = at(&s, data.source_points[0]);
        for inst in &data.instances {
            let want = src0 + v(r.cos(), r.sin()) * (15.0 * inst.at[0] as f64);
            assert!(close(at(&s, inst.points[0]), want, 1e-9), "{deg}");
        }
    }
}

/// Fix-Runde Task 9: Richtung 2 „senkrecht zu Richtung 1“ (Fusion) folgt der drehenden Linie;
/// als erste Richtung geht sie nicht.
#[test]
fn a_perpendicular_second_direction_turns_with_the_line() {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let a = s.add_point(-50.0, -50.0).unwrap();
    let b = s.add_point(-30.0, -50.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let mut plan = rect_plan(src, [2, 3], [15.0, 10.0]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::Line(l), PatternDir::Perpendicular],
        extent: [false, false],
        symmetric: [false, false],
    };
    let id = s.add_pattern(&plan).unwrap();
    for deg in -180..=180 {
        let r = (deg as f64).to_radians();
        s.set_point_xy(b, -50.0 + 20.0 * r.cos(), -50.0 + 20.0 * r.sin())
            .unwrap();
        solved(&mut s);
        let data = s.pattern(id).unwrap();
        let src0 = at(&s, data.source_points[0]);
        let (u, n) = (v(r.cos(), r.sin()), v(-r.sin(), r.cos()));
        for inst in &data.instances {
            let want = src0 + u * (15.0 * inst.at[0] as f64) + n * (10.0 * inst.at[1] as f64);
            assert!(close(at(&s, inst.points[0]), want, 1e-9), "{deg}");
        }
    }
    // Als erste Richtung gibt es kein „senkrecht“.
    let mut bad = current_plan(&s, id);
    bad.kind = PatternKind::Rect {
        dirs: [PatternDir::Perpendicular, PatternDir::SketchY],
        extent: [false, false],
        symmetric: [false, false],
    };
    assert_eq!(
        s.pattern_transforms(&bad).unwrap_err(),
        PatternProblem::BadDirection
    );
    // Eine Datei mit „senkrecht“ als erster Richtung lädt nicht, und die Meldung sagt, warum
    // (Schlusskorrektur: nicht „fehlendes Objekt“).
    let mut data = s.to_data();
    data.patterns[id.index()].kind = bad.kind;
    let e = Sketch::from_data(data).unwrap_err();
    let text = e.to_string();
    assert!(
        text.contains(&planora_i18n::tr!("sketch-err-data-pattern-direction")),
        "{text}"
    );
}

#[test]
fn instances_follow_a_moving_center() {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let c = s.add_point(-20.0, 0.0).unwrap();
    let id = s
        .add_pattern(&circ_plan(src, c, CircularMode::Full, 6, 0.0))
        .unwrap();
    for k in 0..=200 {
        let x = -40.0 + 0.2 * k as f64;
        s.set_point_xy(c, x, -0.5 * x).unwrap();
        solved(&mut s);
        assert_instances_follow(&s, id, 1e-9);
        let data = s.pattern(id).unwrap();
        let inst = data.instances.iter().find(|i| i.at == [3, 0]).unwrap();
        // 180°: an der Mitte gespiegelt.
        let m = at(&s, c);
        let want = m * 2.0 - at(&s, data.source_points[0]);
        assert!(close(at(&s, inst.points[0]), want, 1e-9));
    }
}

#[test]
fn spacing_sweep_moves_instances_and_keeps_the_source_bit_identical() {
    let mut s = sketch();
    let (src, pts, _) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [4, 2], [15.0, 10.0]))
        .unwrap();
    let spacing = value_id(&s, id, PatternSlot::Spacing1);
    let before: Vec<u64> = pts
        .iter()
        .flat_map(|&p| {
            let q = at(&s, p);
            [q.x.to_bits(), q.y.to_bits()]
        })
        .collect();
    for k in 0..=1000 {
        let d = 0.5 + 99.5 * k as f64 / 1000.0;
        let rev = s.revision();
        s.set_constraint_value(spacing, d).unwrap();
        solved(&mut s);
        assert_ne!(s.revision(), rev);
        let data = s.pattern(id).unwrap();
        for inst in &data.instances {
            let o = v(d * inst.at[0] as f64, 10.0 * inst.at[1] as f64);
            for (j, &q) in inst.points.iter().enumerate() {
                assert!(
                    close(at(&s, q), at(&s, data.source_points[j]) + o, 1e-9),
                    "{d}"
                );
            }
        }
        let now: Vec<u64> = pts
            .iter()
            .flat_map(|&p| {
                let q = at(&s, p);
                [q.x.to_bits(), q.y.to_bits()]
            })
            .collect();
        assert_eq!(now, before, "Vorlage unverändert");
    }
}

// ---- Löser ---------------------------------------------------------------------------------

#[test]
fn creating_patterns_adds_no_freedom_and_nothing_redundant() {
    for n in [10u32, 30] {
        let mut s = sketch();
        let (src, _, _) = dimensioned_rect(&mut s);
        let before = solved(&mut s).dof;
        let id = s
            .add_pattern(&rect_plan(src, [n, n], [15.0, 10.0]))
            .unwrap();
        let r = solved(&mut s);
        assert_eq!(r.dof, before, "rel 0 bei {n}×{n}");
        assert!(r.redundant.is_empty(), "{r:?}");
        let data = s.pattern(id).unwrap();
        assert_eq!(data.instances.len(), (n * n - 1) as usize);
        for inst in &data.instances {
            assert!(inst.points.iter().all(|p| !r.free_points.contains(p)));
            assert!(inst.curves.iter().all(|c| !r.free_curves.contains(c)));
        }
    }
}

#[test]
fn instances_of_a_free_source_count_as_free() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    let before = solved(&mut s).dof;
    let id = s
        .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
        .unwrap();
    let r = solved(&mut s);
    assert_eq!(r.dof, before);
    let data = s.pattern(id).unwrap();
    assert!(
        data.instances[0]
            .points
            .iter()
            .all(|p| r.free_points.contains(p))
    );
    assert!(
        data.instances[0]
            .curves
            .iter()
            .all(|c| r.free_curves.contains(c))
    );
}

#[test]
fn a_foreign_line_bound_to_an_instance_follows() {
    let mut s = sketch();
    let (src, _, w) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [3, 1], [15.0, 10.0]))
        .unwrap();
    let inst = s.pattern(id).unwrap().instances[1].clone();
    let corner = inst.points[2];
    let f = s.add_point(60.0, 30.0).unwrap();
    let g = s.add_point(70.0, 40.0).unwrap();
    s.add_line(f, g).unwrap();
    s.add_constraint(K::Coincident(f, corner)).unwrap();
    solved(&mut s);
    assert!(close(at(&s, f), at(&s, corner), 1e-9));
    for width in [12.0, 4.0, 25.0] {
        s.set_constraint_value(w, width).unwrap();
        let r = solved(&mut s);
        assert!(r.redundant.is_empty());
        assert!(close(at(&s, f), at(&s, corner), 1e-9), "{width}");
        assert_instances_follow(&s, id, 1e-9);
    }
}

#[test]
fn dragging_an_instance_point_moves_the_source_by_the_same_offset() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    solved(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
        .unwrap();
    let data = s.pattern(id).unwrap().clone();
    let (q, p) = (data.instances[0].points[2], data.source_points[2]);
    let (q0, p0) = (at(&s, q), at(&s, p));
    let shift = v(3.0, 2.0);
    let r = s
        .drag(&[Drag {
            point: q,
            target: q0 + shift,
        }])
        .unwrap();
    assert!(!r.drag_failed, "{r:?}");
    assert!(close(at(&s, p), p0 + shift, 1e-6), "{:?}", at(&s, p));
    assert!(close(at(&s, q), q0 + shift, 1e-6), "{:?}", at(&s, q));
    // Auch im Kreismuster: die Umkehrung der Drehung.
    let mut s = sketch();
    let src = rect(&mut s, v(20.0, 0.0));
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    let id = s
        .add_pattern(&circ_plan(src, c, CircularMode::Full, 4, 0.0))
        .unwrap();
    let data = s.pattern(id).unwrap().clone();
    let inst = data.instances.iter().find(|i| i.at == [1, 0]).unwrap();
    let (q, p) = (inst.points[0], data.source_points[0]);
    let p0 = at(&s, p);
    // Exemplar um 90° gedreht: Zug nach +x am Exemplar ist Zug nach −y an der Vorlage.
    s.drag(&[Drag {
        point: q,
        target: at(&s, q) + v(2.0, 0.0),
    }])
    .unwrap();
    assert!(close(at(&s, p), p0 + v(0.0, -2.0), 1e-6), "{:?}", at(&s, p));
}

#[test]
fn a_constraint_between_source_and_own_instance_is_a_cycle() {
    let mut s = sketch();
    let (src, pts, _) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
        .unwrap();
    let inst = s.pattern(id).unwrap().instances[0].clone();
    // Ein fremder Punkt, der am Exemplar hängt und zugleich die Vorlage bindet.
    let f = s.add_point(40.0, 40.0).unwrap();
    s.add_constraint(K::Coincident(f, inst.points[1])).unwrap();
    let rev = s.revision();
    let count = s.constraints().count();
    let err = s.add_constraint(K::Distance(f, pts[2], 5.0)).unwrap_err();
    assert_eq!(err, PatternProblem::Cycle.error());
    assert_eq!(s.revision(), rev, "Skizze unverändert");
    assert_eq!(s.constraints().count(), count);
    let err = s
        .add_constraint(K::Horizontal(LineOrPoints::Points(pts[3], inst.points[3])))
        .unwrap_err();
    assert_eq!(err, PatternProblem::Cycle.error());
    // Fremde Geometrie ohne Bezug zur Vorlage darf binden.
    let g = s.add_point(80.0, 40.0).unwrap();
    s.add_constraint(K::Distance(g, inst.points[2], 5.0))
        .unwrap();
    solved(&mut s);
}

#[test]
fn pattern_values_are_dimensions_that_cannot_be_removed_or_added_by_hand() {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [3, 2], [15.0, 10.0]))
        .unwrap();
    let values = s.pattern(id).unwrap().values.clone();
    assert_eq!(values.len(), 4);
    for &c in &values {
        let kind = s.constraint(c).unwrap().kind.clone();
        assert!(kind.is_dimension() && kind.needs_format_15());
        assert!(kind.points().is_empty() && kind.curves().is_empty());
        assert_eq!(kind.length_value(), None);
        assert!(s.remove_constraint(c).is_err());
        assert!(s.set_driven(c, true).is_err());
        assert!(s.add_constraint(kind).is_err());
    }
    assert!(s.needs_format_15());
    assert_eq!(
        s.measure(&K::PatternValue(id, PatternSlot::Spacing1, 0.0)),
        Some(15.0)
    );
    assert!(
        s.set_constraint_value(value_id(&s, id, PatternSlot::Spacing2), 0.0)
            .is_err()
    );
    // Die ganze Skizze kopieren: Musterwerte werden nicht mitkopiert, das Kopieren geht.
    let set = s.whole_sketch_set();
    let mut t = Similarity::identity(DVec2::ZERO);
    t.offset = v(0.0, 100.0);
    s.copy_transformed(&set, &t).unwrap();
    assert_eq!(s.pattern_count(), 1);
}

// ---- Grenzlagen (Review-Fokus 1) -------------------------------------------------------------

#[test]
fn edge_cases_of_counts_steps_angles_and_suppression() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    let curves = s.curves().count();
    // Anzahl 1/1: kein Exemplar, Muster bleibt.
    let id = s
        .add_pattern(&rect_plan(src.clone(), [1, 1], [15.0, 10.0]))
        .unwrap();
    assert!(s.pattern(id).unwrap().instances.is_empty());
    assert_eq!(s.pattern_count(), 1);
    assert_eq!(s.curves().count(), curves);
    assert_eq!(s.pattern(id).unwrap().values.len(), 4);

    // Abstand 0, NaN ⇒ ZeroStep.
    for bad in [0.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            s.pattern_transforms(&rect_plan(src.clone(), [3, 1], [bad, 10.0])),
            Err(PatternProblem::ZeroStep)
        );
    }
    let c = s.add_point(-30.0, 0.0).unwrap();
    assert_eq!(
        s.pattern_transforms(&circ_plan(src.clone(), c, CircularMode::Angle, 3, f64::NAN)),
        Err(PatternProblem::ZeroStep)
    );
    // „Winkel“ 360° gilt wie „Voll“ (Annahme): bei 4 die Lagen 0°, 90°, 180°, 270°.
    for full in [TAU, TAU - 1e-12, -TAU] {
        let got = s
            .pattern_transforms(&circ_plan(src.clone(), c, CircularMode::Angle, 4, full))
            .unwrap();
        let mut a: Vec<f64> = got
            .iter()
            .map(|(_, t)| t.angle.to_degrees().rem_euclid(360.0))
            .collect();
        a.sort_by(f64::total_cmp);
        assert_angles(a, &[90.0, 180.0, 270.0]);
    }
    // 31×30 ⇒ TooMany; 30×30 geht.
    let too_many = rect_plan(src.clone(), [31, 30], [15.0, 10.0]);
    assert_eq!(
        s.pattern_transforms(&too_many),
        Err(PatternProblem::TooMany)
    );
    assert_eq!(
        s.add_pattern(&too_many),
        Err(PatternProblem::TooMany.error())
    );
    assert_eq!(
        s.pattern_transforms(&rect_plan(src.clone(), [30, 30], [15.0, 10.0]))
            .unwrap()
            .len(),
        PATTERN_MAX_INSTANCES - 1
    );
    // Alle abgewählt: gültig, keine Kurve.
    let mut all = rect_plan(src.clone(), [2, 2], [15.0, 10.0]);
    all.suppressed = vec![[1, 0], [0, 1], [1, 1]];
    let curves = s.curves().count();
    let id = s.add_pattern(&all).unwrap();
    assert!(s.pattern(id).unwrap().instances.is_empty());
    assert_eq!(s.curves().count(), curves);
    // Leer, unbekannte Richtung.
    assert_eq!(
        s.pattern_transforms(&rect_plan(Vec::new(), [2, 1], [15.0, 10.0])),
        Err(PatternProblem::Empty)
    );
    let mut parallel = rect_plan(src.clone(), [2, 2], [15.0, 10.0]);
    parallel.kind = PatternKind::Rect {
        dirs: [PatternDir::SketchX, PatternDir::SketchX],
        extent: [false, false],
        symmetric: [false, false],
    };
    assert_eq!(
        s.pattern_transforms(&parallel),
        Err(PatternProblem::BadDirection)
    );
    // Ein Exemplar als Vorlage.
    let id = s
        .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
        .unwrap();
    let inst = s.pattern(id).unwrap().instances[0].curves.clone();
    assert_eq!(
        s.pattern_transforms(&rect_plan(inst, [2, 1], [15.0, 10.0])),
        Err(PatternProblem::InstanceAsSource)
    );
}

#[test]
fn overlapping_instances_keep_profiles_finite() {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    s.add_pattern(&rect_plan(src, [4, 2], [3.0, 4.0])).unwrap();
    solved(&mut s);
    let found = s.profiles();
    assert!(
        found
            .outlines
            .iter()
            .flatten()
            .flatten()
            .all(|p| p.is_finite())
    );
}

// ---- Alle Kurvenarten im Kreismuster -------------------------------------------------------

#[test]
fn ellipse_elliptic_arc_and_spline_turn_with_the_circular_pattern() {
    let mut s = sketch();
    let m = s.add_point(20.0, 0.0).unwrap();
    let e = s.add_ellipse(m, v(4.0, 1.0), 2.0).unwrap();
    let em = s.add_point(30.0, 5.0).unwrap();
    let (es, ee) = (
        s.add_point(33.0, 5.0).unwrap(),
        s.add_point(30.0, 7.0).unwrap(),
    );
    let ea = s.add_elliptic_arc(em, v(3.0, 0.0), 2.0, es, ee).unwrap();
    let sp: Vec<PointId> = [(20.0, 10.0), (24.0, 13.0), (28.0, 9.0), (31.0, 12.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let spline = s.add_spline(&sp).unwrap();
    solved(&mut s);
    let c = s.add_point(0.0, 0.0).unwrap();
    let id = s
        .add_pattern(&circ_plan(
            vec![e, ea, spline],
            c,
            CircularMode::Full,
            5,
            0.0,
        ))
        .unwrap();
    solved(&mut s);
    let data = s.pattern(id).unwrap().clone();
    assert_eq!(data.instances.len(), 4);
    for inst in &data.instances {
        let turn = inst.at[0] as f64 * TAU / 5.0;
        let rot = |p: DVec2| {
            DVec2::new(
                p.x * turn.cos() - p.y * turn.sin(),
                p.x * turn.sin() + p.y * turn.cos(),
            )
        };
        for (k, &src) in data.sources.iter().enumerate() {
            let copy = inst.curves[k];
            match (&s.curve(src).unwrap().kind, &s.curve(copy).unwrap().kind) {
                (CurveKind::Ellipse { .. }, CurveKind::Ellipse { .. })
                | (CurveKind::EllipticArc { .. }, CurveKind::EllipticArc { .. }) => {
                    for t in 0..64 {
                        let t = TAU * t as f64 / 64.0;
                        let want = rot(s.ellipse_point(src, t).unwrap());
                        let got = s.ellipse_point(copy, t).unwrap();
                        assert!(close(want, got, 1e-9), "{:?}", inst.at);
                    }
                }
                (CurveKind::Spline { points: a }, CurveKind::Spline { points: b }) => {
                    assert_eq!(a.len(), b.len());
                    for (p, q) in a.iter().zip(b) {
                        assert!(close(rot(at(&s, *p)), at(&s, *q), 1e-9));
                    }
                }
                (a, b) => panic!("Art verschieden: {a:?} {b:?}"),
            }
        }
    }
    // Halbachse und Spline folgen auch, wenn sich die Vorlage bewegt.
    s.set_point_xy(sp[1], 25.0, 15.0).unwrap();
    solved(&mut s);
    assert_instances_follow(&s, id, 1e-9);
}

// ---- Korrekturrunde 1 ------------------------------------------------------------------------

#[test]
fn switching_a_constraint_back_on_cannot_close_a_cycle() {
    for via_driven in [false, true] {
        let mut s = sketch();
        let (src, pts, _) = dimensioned_rect(&mut s);
        let id = s
            .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
            .unwrap();
        let q = s.pattern(id).unwrap().instances[0].points[1];
        let f = s.add_point(40.0, 40.0).unwrap();
        let a = s.add_constraint(K::Distance(pts[0], f, 3.0)).unwrap();
        if via_driven {
            s.set_driven(a, true).unwrap();
        } else {
            s.set_enabled(a, false).unwrap();
        }
        // Ohne die abgeschaltete Bindung ist das keine Schleife.
        s.add_constraint(K::Coincident(f, q)).unwrap();
        solved(&mut s);
        let rev = s.revision();
        let err = if via_driven {
            s.set_driven(a, false).unwrap_err()
        } else {
            s.set_enabled(a, true).unwrap_err()
        };
        assert_eq!(err, PatternProblem::Cycle.error());
        assert_eq!(s.revision(), rev, "Skizze unverändert");
        let d = s.constraint(a).unwrap();
        assert!(d.driven == via_driven && d.enabled == via_driven);
        solved(&mut s);
        assert!(close(at(&s, f), at(&s, q), 1e-9));
        // Ab- und wieder einschalten ohne Schleife geht weiter.
        let g = s.add_point(90.0, 40.0).unwrap();
        let b = s.add_constraint(K::Distance(g, q, 3.0)).unwrap();
        s.set_enabled(b, false).unwrap();
        s.set_enabled(b, true).unwrap();
        s.set_driven(b, true).unwrap();
        s.set_driven(b, false).unwrap();
        solved(&mut s);
    }
}

/// `links` Linien, jede ein Muster mit einem Exemplar 10 weiter rechts; der Anfang der nächsten
/// Linie liegt auf dem Exemplar des Anfangs der vorigen. Der erste Anfang hängt über ein Maß an
/// einem festen Punkt.
fn pattern_chain(s: &mut Sketch, links: usize) -> (ConstraintId, Vec<PointId>) {
    let o = s.add_point(0.0, 0.0).unwrap();
    s.add_constraint(K::Fixed(o, 0.0, 0.0)).unwrap();
    let mut starts = Vec::new();
    let mut prev: Option<PointId> = None;
    let mut dim = None;
    for k in 0..links {
        let x = 5.0 + 10.0 * k as f64;
        let a = s.add_point(x, 0.0).unwrap();
        let b = s.add_point(x, 5.0).unwrap();
        let l = s.add_line(a, b).unwrap();
        s.add_constraint(K::Vertical(LineOrPoints::Line(l)))
            .unwrap();
        s.add_constraint(K::Distance(a, b, 5.0)).unwrap();
        match prev {
            None => {
                s.add_constraint(K::Horizontal(LineOrPoints::Points(o, a)))
                    .unwrap();
                dim = Some(s.add_constraint(K::Distance(o, a, 5.0)).unwrap());
            }
            Some(q) => {
                s.add_constraint(K::Coincident(a, q)).unwrap();
            }
        }
        let id = s
            .add_pattern(&rect_plan(vec![l], [2, 1], [10.0, 10.0]))
            .unwrap();
        let data = s.pattern(id).unwrap();
        let k0 = data.source_points.iter().position(|&p| p == a).unwrap();
        prev = Some(data.instances[0].points[k0]);
        starts.push(a);
    }
    (dim.unwrap(), starts)
}

/// Schlusskorrektur I2: Die Runden reichen für jede Kette ohne Schleife (mindestens Zahl der
/// Muster + 1); fünf Glieder löst ein Lauf.
#[test]
fn a_chain_longer_than_the_old_rounds_is_solved_in_one_run() {
    for links in [3, 5, 7] {
        let mut s = sketch();
        let (dim, starts) = pattern_chain(&mut s, links);
        solved(&mut s);
        for value in [25.0, 3.0, 40.0] {
            s.set_constraint_value(dim, value).unwrap();
            solved(&mut s);
            for (k, &a) in starts.iter().enumerate() {
                assert!(
                    close(at(&s, a), v(value + 10.0 * k as f64, 0.0), 1e-6),
                    "{links} {value} {k} {:?}",
                    at(&s, a)
                );
            }
        }
    }
}

/// Schlusskorrektur (Minor 6): Geht die Lage eines Musters nicht (die zweite Richtungslinie
/// wird parallel zur ersten), bleibt nichts still überholt: Der Lauf meldet „nicht lösbar“.
#[test]
fn a_pattern_whose_lattice_fails_is_reported_not_solved() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    let line = |s: &mut Sketch, a: DVec2, b: DVec2| {
        let (p, q) = (
            s.add_point(a.x, a.y).unwrap(),
            s.add_point(b.x, b.y).unwrap(),
        );
        s.add_line(p, q).unwrap()
    };
    let d1 = line(&mut s, v(0.0, -20.0), v(10.0, -20.0));
    let d2 = line(&mut s, v(-20.0, 0.0), v(-14.0, 10.0));
    let mut plan = rect_plan(src, [3, 2], [15.0, 10.0]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::Line(d1), PatternDir::Line(d2)],
        extent: [false, false],
        symmetric: [false, false],
    };
    s.add_pattern(&plan).unwrap();
    solved(&mut s);
    s.add_constraint(K::Parallel(d1, d2)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::NotConverged, "{r:?}");
}

#[test]
fn preview_reports_the_real_error_of_an_instance() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    // Koordinaten über MAX_COORD (1e9) beim zweiten Exemplar.
    let plan = rect_plan(src, [3, 1], [6e8, 10.0]);
    let preview = s.pattern_preview(&plan, 64).unwrap_err();
    let created = s.add_pattern(&plan).unwrap_err();
    assert_eq!(preview, created);
    assert_ne!(preview, PatternProblem::Empty.error());
    assert_eq!(s.pattern_count(), 0);
    let bad = rect_plan(Vec::new(), [3, 1], [6.0, 10.0]);
    assert_eq!(
        s.pattern_preview(&bad, 64).unwrap_err(),
        PatternProblem::Empty.error()
    );
}

#[test]
fn dragging_an_instance_point_of_a_deleted_source_point_moves_the_point_itself() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    let id = s
        .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
        .unwrap();
    let data = s.pattern(id).unwrap().clone();
    let (p, q) = (data.source_points[0], data.instances[0].points[0]);
    s.remove_point(p).unwrap();
    let target = at(&s, q) + v(1.0, 2.0);
    s.drag(&[Drag { point: q, target }]).unwrap();
    assert!(close(at(&s, q), target, 1e-9), "{:?}", at(&s, q));
}

#[test]
fn circular_angle_sweep_with_an_arc_source() {
    let mut s = sketch();
    let (m, a0, a1) = (
        s.add_point(20.0, 0.0).unwrap(),
        s.add_point(24.0, 0.0).unwrap(),
        s.add_point(20.0, 4.0).unwrap(),
    );
    let arc = s.add_arc(m, a0, a1).unwrap();
    s.add_constraint(K::Fixed(m, 20.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(a0, 24.0, 0.0)).unwrap();
    solved(&mut s);
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    let n = 4u32;
    let id = s
        .add_pattern(&circ_plan(
            vec![arc],
            c,
            CircularMode::Angle,
            n,
            90f64.to_radians(),
        ))
        .unwrap();
    let angle = value_id(&s, id, PatternSlot::Angle);
    let pts = [m, a0, a1];
    for deg in -360..=360 {
        if deg == 0 {
            assert!(s.set_constraint_value(angle, 0.0).is_err());
            continue;
        }
        let theta = (deg as f64).to_radians();
        s.set_constraint_value(angle, theta).unwrap();
        solved(&mut s);
        let step = if theta.abs() >= TAU - 1e-9 {
            theta.signum() * TAU / n as f64
        } else {
            theta / (n - 1) as f64
        };
        let data = s.pattern(id).unwrap();
        assert_eq!(data.instances.len(), (n - 1) as usize);
        for inst in &data.instances {
            let turn = step * inst.at[0] as f64;
            let rot = |p: DVec2| {
                DVec2::new(
                    p.x * turn.cos() - p.y * turn.sin(),
                    p.x * turn.sin() + p.y * turn.cos(),
                )
            };
            let copy = inst.curves[0];
            let CurveKind::Arc { center, start, end } = s.curve(copy).unwrap().kind else {
                panic!("kein Bogen");
            };
            for (src, got) in pts.iter().zip([center, start, end]) {
                assert!(
                    close(rot(at(&s, *src)), at(&s, got), 1e-9),
                    "{deg} {:?}",
                    inst.at
                );
            }
            assert!((s.radius(copy).unwrap() - 4.0).abs() < 1e-9);
            // Der Bogen klappt nicht um: derselbe Öffnungswinkel wie die Vorlage.
            let sweep = |c: CurveId| {
                let CurveKind::Arc { center, start, end } = s.curve(c).unwrap().kind else {
                    unreachable!()
                };
                let (m, a, b) = (at(&s, center), at(&s, start), at(&s, end));
                ((b - m).to_angle() - (a - m).to_angle()).rem_euclid(TAU)
            };
            assert!((sweep(copy) - sweep(arc)).abs() < 1e-9, "{deg}");
        }
    }
}

/// Exemplare eines 3×1-Musters des bemaßten Rechtecks (Abstand 15): die Ecke unten links des
/// ersten und zweiten Exemplars und die untere Linie des ersten.
fn instance_corners(s: &mut Sketch) -> (PatternId, PointId, PointId, CurveId) {
    let (src, [p0, ..], _) = dimensioned_rect(s);
    let id = s
        .add_pattern(&rect_plan(src.clone(), [3, 1], [15.0, 10.0]))
        .unwrap();
    solved(s);
    let d = s.pattern(id).unwrap().clone();
    let k = d.source_points.iter().position(|&x| x == p0).unwrap();
    let inst = |at: [i32; 2]| d.instances.iter().find(|i| i.at == at).unwrap().clone();
    let (i1, i2) = (inst([1, 0]), inst([2, 0]));
    (id, i1.points[k], i2.points[k], i1.curves[0])
}

/// Schlusskorrektur I1: Ein Maß, das nur Exemplare liest, wird anzeigend und zeigt den
/// gemessenen Wert; der getippte Wert täte nichts.
#[test]
fn a_dimension_between_two_instances_becomes_driven_with_the_real_value() {
    let mut s = sketch();
    let (_, a, b, _) = instance_corners(&mut s);
    assert!(close(at(&s, b) - at(&s, a), v(15.0, 0.0), 1e-9));
    for kind in [
        K::Distance(a, b, 50.0),
        K::DistanceX(a, b, 50.0),
        K::Distance(b, a, 3.0),
    ] {
        let id = s.add_constraint(kind).unwrap();
        let c = s.constraint(id).unwrap();
        assert!(c.driven, "{:?}", c.kind);
        assert!(
            (c.kind.value().unwrap() - 15.0).abs() < 1e-9,
            "{:?}",
            c.kind
        );
        let r = solved(&mut s);
        assert!(r.redundant.is_empty() && r.conflicting.is_empty(), "{r:?}");
        assert!(close(at(&s, b) - at(&s, a), v(15.0, 0.0), 1e-9));
        // Wieder erzwingen ginge ohne Wirkung: abgelehnt, Stand unverändert.
        let rev = s.revision();
        let e = s.set_driven(id, false).unwrap_err();
        assert_eq!(e, PatternProblem::InstanceOnly.error());
        assert_eq!(s.revision(), rev);
        assert!(s.constraint(id).unwrap().driven);
    }
}

/// Schlusskorrektur I1: Eine geometrische Abhängigkeit nur an Exemplaren wird abgelehnt; die
/// Skizze bleibt, wie sie war.
#[test]
fn a_geometric_constraint_on_instances_only_is_refused() {
    let mut s = sketch();
    let (_, a, b, line) = instance_corners(&mut s);
    let count = s.constraints().count();
    for kind in [
        K::Horizontal(LineOrPoints::Line(line)),
        K::Coincident(a, b),
        K::Fixed(a, 0.0, 0.0),
    ] {
        let rev = s.revision();
        let e = s.add_constraint(kind.clone()).unwrap_err();
        assert_eq!(e, PatternProblem::InstanceOnly.error(), "{kind:?}");
        assert_eq!(s.revision(), rev, "{kind:?}");
        assert_eq!(s.constraints().count(), count);
    }
}

/// Exemplar und freier Punkt: wie bisher ein treibendes Maß, der freie Punkt folgt.
#[test]
fn an_instance_and_a_free_point_still_drive() {
    let mut s = sketch();
    let (_, a, _, _) = instance_corners(&mut s);
    let f = s.add_point(at(&s, a).x, 30.0).unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Points(a, f)))
        .unwrap();
    let id = s.add_constraint(K::Distance(a, f, 20.0)).unwrap();
    assert!(!s.constraint(id).unwrap().driven);
    solved(&mut s);
    assert!((at(&s, f).distance(at(&s, a)) - 20.0).abs() < 1e-9);
    s.set_constraint_value(id, 25.0).unwrap();
    solved(&mut s);
    assert!((at(&s, f).distance(at(&s, a)) - 25.0).abs() < 1e-9);
}

/// Alte Dateien mit einem treibenden Maß nur an Exemplaren laden weiter; ein neuer Wert für so
/// ein Maß wird mit der Meldung abgelehnt statt still gespeichert.
#[test]
fn an_old_driving_instance_dimension_loads_but_takes_no_new_value() {
    let mut s = sketch();
    let (_, a, b, _) = instance_corners(&mut s);
    let id = s.add_constraint(K::Distance(a, b, 50.0)).unwrap();
    let mut data = s.to_data();
    let rec = &mut data.constraints[id.index()];
    rec.driven = false;
    rec.kind = K::Distance(a, b, 50.0);
    let mut old = Sketch::from_data(data).unwrap();
    assert!(!old.constraint(id).unwrap().driven);
    old.solve();
    let rev = old.revision();
    let e = old.set_constraint_value(id, 70.0).unwrap_err();
    assert_eq!(e, PatternProblem::InstanceOnly.error());
    assert_eq!(old.revision(), rev);
    assert_eq!(old.constraint(id).unwrap().kind, K::Distance(a, b, 50.0));
    // Anzeigend schalten geht und zeigt dann den gemessenen Wert.
    old.set_driven(id, true).unwrap();
    old.set_constraint_value(id, 15.0).unwrap();
}
