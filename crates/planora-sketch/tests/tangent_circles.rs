//! Tangentialkreise (Skizzen 3, Task 3): Der Zeiger wählt die Lösung, Linien rechnen analytisch,
//! alles andere über den Solver in einer Wegwerfskizze. Der Solver der echten Skizze nimmt danach
//! nie still eine andere Lösung.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::ConstraintKind as K;
use planora_sketch::tangent_circles::{
    is_tangent_candidate, tangent_circle_three, tangent_circle_two_lines,
};
use planora_sketch::{CurveId, CurveKind, Sketch, SolveStatus};

const TOL: f64 = 1e-6;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let (pa, pb) = (
        s.add_point(a.0, a.1).unwrap(),
        s.add_point(b.0, b.1).unwrap(),
    );
    s.add_line(pa, pb).unwrap()
}

fn circle(s: &mut Sketch, m: (f64, f64), r: f64) -> CurveId {
    let c = s.add_point(m.0, m.1).unwrap();
    s.add_circle(c, r).unwrap()
}

/// Bogen um `m` von Winkel `a0` nach `a1` (Bogenmaß).
fn arc(s: &mut Sketch, m: (f64, f64), r: f64, a0: f64, a1: f64) -> CurveId {
    let c = s.add_point(m.0, m.1).unwrap();
    let st = s.add_point(m.0 + r * a0.cos(), m.1 + r * a0.sin()).unwrap();
    let en = s.add_point(m.0 + r * a1.cos(), m.1 + r * a1.sin()).unwrap();
    s.add_arc(c, st, en).unwrap()
}

fn assert_circle(got: Option<(DVec2, f64)>, c: (f64, f64), r: f64) {
    let (m, rr) = got.expect("eine Lösung");
    assert!(
        m.distance(v(c.0, c.1)) < 1e-9 && (rr - r).abs() < 1e-9,
        "erwartet {c:?} r {r}, bekommen {m:?} r {rr}"
    );
}

/// Abstand der Kreislinie von der unbegrenzten Linie bzw. dem ganzen Kreis der Kurve.
fn tangent_error(s: &Sketch, c: CurveId, m: DVec2, r: f64) -> f64 {
    match s.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => {
            let (a, b) = (s.point_xy(a).unwrap(), s.point_xy(b).unwrap());
            let d = (b - a).normalize();
            (d.perp_dot(m - a).abs() - r).abs()
        }
        CurveKind::Circle { center, .. } | CurveKind::Arc { center, .. } => {
            let (o, ro) = (s.point_xy(center).unwrap(), s.radius(c).unwrap());
            let d = o.distance(m);
            (d - (r + ro))
                .abs()
                .min((d - (r - ro)).abs())
                .min((d - (ro - r)).abs())
        }
        ref k => panic!("keine Tangentialkurve: {k:?}"),
    }
}

/// Dreieck mit den Ecken (0,0), (3,0), (0,4).
fn triangle(s: &mut Sketch) -> [CurveId; 3] {
    [
        line(s, (0.0, 0.0), (3.0, 0.0)),
        line(s, (3.0, 0.0), (0.0, 4.0)),
        line(s, (0.0, 4.0), (0.0, 0.0)),
    ]
}

/// Werte aus dem Probelauf der Planung.
fn mixed(s: &mut Sketch) -> [CurveId; 3] {
    [
        line(s, (-50.0, 0.0), (50.0, 0.0)),
        circle(s, (0.0, 30.0), 10.0),
        arc(s, (30.0, 30.0), 8.0, 2.0, 5.0),
    ]
}

/// Legt das Ergebnis als Kreis mit drei `Tangent` in eine Kopie von `s`, löst und prüft: Der Kreis
/// wandert unter `10·tol`, nichts ist überzählig, er bringt keinen Freiheitsgrad mit, und die
/// vorhandenen (freien) Werte bleiben bitgleich.
fn assert_stays(s: &Sketch, curves: [CurveId; 3], m: DVec2, r: f64, what: &str) {
    let mut real = s.clone();
    let before_dof = real.solve().dof;
    let old: Vec<f64> = real.variables().to_vec();
    let c = real.add_point(m.x, m.y).unwrap();
    let k = real.add_circle(c, r).unwrap();
    for g in curves {
        real.add_constraint(K::Tangent(g, k)).unwrap();
    }
    let rep = real.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{what}: {rep:?}");
    assert!(rep.redundant.is_empty(), "{what}: {rep:?}");
    assert_eq!(rep.dof, before_dof, "{what}: Freiheitsgrade {rep:?}");
    let moved = real.point_xy(c).unwrap().distance(m);
    assert!(moved < 10.0 * TOL, "{what}: Mitte wandert um {moved}");
    let dr = (real.radius(k).unwrap() - r).abs();
    assert!(dr < 10.0 * TOL, "{what}: Radius wandert um {dr}");
    for (i, (a, b)) in old.iter().zip(real.variables()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "{what}: Wert {i} der Skizze bewegt"
        );
    }
}

/// Dasselbe mit festgehaltenen Kurven: Der neue Kreis selbst ist dann voll bestimmt.
fn assert_fully_defined(s: &Sketch, curves: [CurveId; 3], m: DVec2, r: f64, what: &str) {
    let mut real = s.clone();
    for g in curves {
        let kind = real.curve(g).unwrap().kind.clone();
        let (points, radius) = match kind {
            CurveKind::Line { a, b } => (vec![a, b], None),
            CurveKind::Circle { center, .. } => (vec![center], Some(g)),
            // Das Bogenende folgt aus dem internen `ArcRadius` und darf nicht zusätzlich fest sein.
            CurveKind::Arc { center, start, .. } => (vec![center, start], None),
            k => panic!("{k:?}"),
        };
        for p in points {
            let at = real.point_xy(p).unwrap();
            real.add_constraint(K::Fixed(p, at.x, at.y)).unwrap();
        }
        if let Some(c) = radius {
            let rr = real.radius(c).unwrap();
            real.add_constraint(K::Radius(c, rr)).unwrap();
        }
    }
    let c = real.add_point(m.x, m.y).unwrap();
    let k = real.add_circle(c, r).unwrap();
    for g in curves {
        real.add_constraint(K::Tangent(g, k)).unwrap();
    }
    let rep = real.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{what}: {rep:?}");
    assert!(rep.redundant.is_empty(), "{what}: {rep:?}");
    assert!(!rep.free_curves.contains(&k), "{what}: Kreis frei {rep:?}");
    assert!(!rep.free_points.contains(&c), "{what}: Mitte frei {rep:?}");
}

// ---------- Zwei Linien ----------

#[test]
fn two_lines_pointer_chooses_the_sector_and_projects_onto_the_bisector() {
    let mut s = sketch();
    let x = line(&mut s, (-20.0, 0.0), (20.0, 0.0));
    let y = line(&mut s, (0.0, -20.0), (0.0, 20.0));
    for (p, c) in [
        ((3.0, 5.0), (4.0, 4.0)),
        ((-3.0, 5.0), (-4.0, 4.0)),
        ((-3.0, -5.0), (-4.0, -4.0)),
        ((3.0, -5.0), (4.0, -4.0)),
    ] {
        assert_circle(
            tangent_circle_two_lines(&s, x, y, v(p.0, p.1), None),
            c,
            4.0,
        );
        // Die Reihenfolge der Linien ändert nichts.
        assert_circle(
            tangent_circle_two_lines(&s, y, x, v(p.0, p.1), None),
            c,
            4.0,
        );
    }
}

#[test]
fn two_lines_at_an_angle_touch_both_inside_the_pointers_sector() {
    let mut s = sketch();
    let a = line(&mut s, (1.0, 1.0), (11.0, 3.0));
    let b = line(&mut s, (2.0, -5.0), (-3.0, 9.0));
    let side = |c: CurveId, p: DVec2| {
        let CurveKind::Line { a, b } = s.curve(c).unwrap().kind else {
            panic!()
        };
        let (a, b) = (s.point_xy(a).unwrap(), s.point_xy(b).unwrap());
        (b - a).perp_dot(p - a).signum()
    };
    for p in [v(6.0, 7.0), v(-6.0, 7.0), v(6.0, -7.0), v(-6.0, -7.0)] {
        let (m, r) = tangent_circle_two_lines(&s, a, b, p, None).unwrap();
        assert!(tangent_error(&s, a, m, r) < 1e-9);
        assert!(tangent_error(&s, b, m, r) < 1e-9);
        assert_eq!(side(a, m), side(a, p));
        assert_eq!(side(b, m), side(b, p));
    }
}

#[test]
fn two_lines_typed_diameter_sets_the_radius_exactly() {
    let mut s = sketch();
    let x = line(&mut s, (-20.0, 0.0), (20.0, 0.0));
    let y = line(&mut s, (0.0, -20.0), (0.0, 20.0));
    let (m, r) = tangent_circle_two_lines(&s, x, y, v(3.0, 5.0), Some(10.0)).unwrap();
    assert_eq!(r, 5.0);
    assert!(m.distance(v(5.0, 5.0)) < 1e-12, "{m:?}");
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            tangent_circle_two_lines(&s, x, y, v(3.0, 5.0), Some(bad)),
            None
        );
    }
}

#[test]
fn two_parallel_lines_give_half_the_gap_on_the_center_line() {
    let mut s = sketch();
    let a = line(&mut s, (0.0, 0.0), (20.0, 0.0));
    // Gegenläufig: Das darf nichts verwechseln.
    let b = line(&mut s, (20.0, 10.0), (0.0, 10.0));
    assert_circle(
        tangent_circle_two_lines(&s, a, b, v(7.0, 2.0), None),
        (7.0, 5.0),
        5.0,
    );
    assert_circle(
        tangent_circle_two_lines(&s, b, a, v(7.0, 2.0), None),
        (7.0, 5.0),
        5.0,
    );
    // Passender Durchmesser geht, abweichender wird abgelehnt.
    assert_circle(
        tangent_circle_two_lines(&s, a, b, v(7.0, 2.0), Some(10.0)),
        (7.0, 5.0),
        5.0,
    );
    assert_eq!(
        tangent_circle_two_lines(&s, a, b, v(7.0, 2.0), Some(8.0)),
        None
    );
}

#[test]
fn two_lines_reject_pointer_on_a_line_and_wrong_input() {
    let mut s = sketch();
    let x = line(&mut s, (-20.0, 0.0), (20.0, 0.0));
    let y = line(&mut s, (0.0, -20.0), (0.0, 20.0));
    let top = line(&mut s, (0.0, 10.0), (5.0, 10.0));
    let same = line(&mut s, (1.0, 0.0), (5.0, 0.0));
    let c = circle(&mut s, (3.0, 3.0), 1.0);
    for (a, b, p) in [
        // Zeiger auf einer Linie, auch jenseits ihrer Enden.
        (x, y, v(30.0, 0.0)),
        (x, y, v(0.0, 3.0)),
        (x, top, v(3.0, 0.0)),
        (x, top, v(30.0, 10.0)),
        // Dieselbe Linie, zwei Linien auf derselben Geraden, keine Linie.
        (x, x, v(3.0, 5.0)),
        (x, same, v(3.0, 5.0)),
        (x, c, v(3.0, 5.0)),
        // Kein endlicher Zeiger.
        (x, y, v(f64::NAN, 5.0)),
        (x, y, v(f64::INFINITY, 5.0)),
    ] {
        assert_eq!(tangent_circle_two_lines(&s, a, b, p, None), None, "{p:?}");
    }
}

// ---------- Drei Linien ----------

#[test]
fn triangle_pointer_inside_gives_the_incircle() {
    let mut s = sketch();
    let t = triangle(&mut s);
    assert_circle(tangent_circle_three(&s, t, v(1.0, 1.0)), (1.0, 1.0), 1.0);
    assert_circle(tangent_circle_three(&s, t, v(0.7, 1.2)), (1.0, 1.0), 1.0);
}

#[test]
fn triangle_pointer_outside_gives_the_excircle_of_that_side() {
    let mut s = sketch();
    let t = triangle(&mut s);
    // Hinter der Seite auf der x-Achse, der schrägen Seite und der Seite auf der y-Achse.
    assert_circle(tangent_circle_three(&s, t, v(1.5, -1.0)), (2.0, -2.0), 2.0);
    assert_circle(tangent_circle_three(&s, t, v(4.0, 4.0)), (6.0, 6.0), 6.0);
    assert_circle(tangent_circle_three(&s, t, v(-1.0, 1.5)), (-3.0, 3.0), 3.0);
}

#[test]
fn equal_rim_distance_prefers_the_nearer_center() {
    let mut s = sketch();
    let t = triangle(&mut s);
    // Auf der Verbindung zwischen Inkreis (1,1) und Ankreis (6,6), außerhalb beider Kreise mit
    // gleichem Abstand zur Kreislinie.
    let along = 3.5 - 5.0 / (2.0 * 2f64.sqrt());
    let p = v(along, along);
    assert_circle(tangent_circle_three(&s, t, p), (1.0, 1.0), 1.0);
    // Die Reihenfolge der Kurven entscheidet nichts.
    assert_circle(
        tangent_circle_three(&s, [t[2], t[0], t[1]], p),
        (1.0, 1.0),
        1.0,
    );
}

#[test]
fn two_parallel_and_a_crossing_line_have_two_solutions() {
    let mut s = sketch();
    let t = [
        line(&mut s, (0.0, 0.0), (20.0, 0.0)),
        line(&mut s, (0.0, 10.0), (20.0, 10.0)),
        line(&mut s, (0.0, 0.0), (0.0, 10.0)),
    ];
    assert_circle(tangent_circle_three(&s, t, v(3.0, 5.0)), (5.0, 5.0), 5.0);
    assert_circle(tangent_circle_three(&s, t, v(-3.0, 5.0)), (-5.0, 5.0), 5.0);
}

#[test]
fn three_lines_without_a_circle_give_none() {
    let mut s = sketch();
    let parallel = [
        line(&mut s, (0.0, 0.0), (20.0, 0.0)),
        line(&mut s, (0.0, 5.0), (20.0, 5.0)),
        line(&mut s, (20.0, 10.0), (0.0, 10.0)),
    ];
    assert_eq!(tangent_circle_three(&s, parallel, v(3.0, 2.0)), None);
    // Durch einen Punkt: nur Radius null.
    let through = [
        line(&mut s, (-5.0, 0.0), (5.0, 0.0)),
        line(&mut s, (0.0, -5.0), (0.0, 5.0)),
        line(&mut s, (-5.0, -5.0), (5.0, 5.0)),
    ];
    assert_eq!(tangent_circle_three(&s, through, v(3.0, 1.0)), None);
}

// ---------- Gemischt, über den Solver ----------

#[test]
fn mixed_solution_touches_all_three_and_follows_the_pointer() {
    let mut s = sketch();
    let t = mixed(&mut s);
    let (m, r) = tangent_circle_three(&s, t, v(15.0, 10.0)).expect("Lösung");
    for c in t {
        let e = tangent_error(&s, c, m, r);
        assert!(e < 1e-6, "Abweichung {e}");
    }
    // Außen an beiden runden Kurven; von Hand aus den drei Gleichungen: r ≈ 13,4 auf der Seite des
    // Zeigers. Die Lösung, die beide Rundungen umschließt (r ≈ 24,9), liegt weiter weg.
    assert!(m.distance(v(16.49, 13.4)) < 0.02, "{m:?}");
    assert!((r - 13.4).abs() < 0.02, "{r}");
}

#[test]
fn an_arc_counts_as_its_whole_circle() {
    // Grenze aus Entscheidung 1: Die Enden des Bogens ändern das Ergebnis nicht.
    let mut a = sketch();
    let ta = mixed(&mut a);
    let mut b = sketch();
    let tb = [
        line(&mut b, (-50.0, 0.0), (50.0, 0.0)),
        circle(&mut b, (0.0, 30.0), 10.0),
        arc(&mut b, (30.0, 30.0), 8.0, 0.1, 0.5),
    ];
    let (ma, ra) = tangent_circle_three(&a, ta, v(15.0, 10.0)).unwrap();
    let (mb, rb) = tangent_circle_three(&b, tb, v(15.0, 10.0)).unwrap();
    assert!(ma.distance(mb) < 1e-6 && (ra - rb).abs() < 1e-6);
}

#[test]
fn three_concentric_circles_have_no_solution() {
    let mut s = sketch();
    let t = [
        circle(&mut s, (0.0, 0.0), 5.0),
        circle(&mut s, (0.0, 0.0), 10.0),
        circle(&mut s, (0.0, 0.0), 20.0),
    ];
    assert_eq!(tangent_circle_three(&s, t, v(12.0, 3.0)), None);
    assert_eq!(tangent_circle_three(&s, t, v(1.0, 1.0)), None);
}

#[test]
fn two_identical_circles_leave_it_undetermined_and_give_none() {
    // Der Kreis bliebe frei beweglich: Das ist keine Lösung.
    let mut s = sketch();
    let t = [
        circle(&mut s, (0.0, 0.0), 5.0),
        circle(&mut s, (0.0, 0.0), 5.0),
        line(&mut s, (-20.0, 8.0), (20.0, 8.0)),
    ];
    assert_eq!(tangent_circle_three(&s, t, v(3.0, 3.0)), None);
}

#[test]
fn a_given_circle_is_never_returned_as_its_own_tangent_circle() {
    // Zwei Kreise liegen auf der Linie und berühren sich: Jeder von ihnen erfüllt die drei
    // Gleichungen scheinbar selbst.
    let mut s = sketch();
    let left = circle(&mut s, (0.0, 10.0), 10.0);
    let right = circle(&mut s, (20.0, 10.0), 10.0);
    let base = line(&mut s, (-50.0, 0.0), (50.0, 0.0));
    for p in [
        v(-10.0, 10.0),
        v(0.0, 10.0),
        v(10.0, 2.0),
        v(10.0, 15.0),
        v(20.0, 10.0),
        v(30.0, 10.0),
    ] {
        if let Some((m, r)) = tangent_circle_three(&s, [left, right, base], p) {
            for c in [v(0.0, 10.0), v(20.0, 10.0)] {
                assert!(
                    m.distance(c) > 1e-3 || (r - 10.0).abs() > 1e-3,
                    "Zeiger {p:?}: {m:?} r {r}"
                );
            }
        }
    }
}

// ---------- Keine stille andere Lösung ----------

/// Zeiger auf einem Raster um die Kurven.
fn pointers() -> Vec<DVec2> {
    let mut out = Vec::new();
    for i in -6..=14 {
        for j in -6..=14 {
            out.push(v(f64::from(i) * 4.0 + 0.37, f64::from(j) * 4.0 + 0.11));
        }
    }
    out
}

/// Prüft jede Lösung über das Raster und gibt zurück, wie viele Zeiger eine bekamen.
fn check_grid(s: &Sketch, t: [CurveId; 3], scale: f64) -> usize {
    let mut seen = 0;
    for p in pointers() {
        let p = p * scale;
        if let Some((m, r)) = tangent_circle_three(s, t, p) {
            seen += 1;
            let what = format!("Zeiger {p:?}, Kreis {m:?} r {r}");
            for c in t {
                let e = tangent_error(s, c, m, r);
                assert!(e < 1e-6, "{what}: Abweichung {e}");
            }
            assert_stays(s, t, m, r, &what);
            assert_fully_defined(s, t, m, r, &what);
        }
    }
    seen
}

#[test]
fn result_stays_put_in_the_real_sketch_three_lines() {
    let mut s = sketch();
    let t = triangle(&mut s);
    assert_eq!(check_grid(&s, t, 0.2), pointers().len(), "immer lösbar");
}

#[test]
fn result_stays_put_in_the_real_sketch_parallel_and_crossing() {
    let mut s = sketch();
    let t = [
        line(&mut s, (0.0, 0.0), (20.0, 0.0)),
        line(&mut s, (0.0, 10.0), (20.0, 10.0)),
        line(&mut s, (0.0, 0.0), (0.0, 10.0)),
    ];
    assert_eq!(check_grid(&s, t, 1.0), pointers().len(), "immer lösbar");
}

#[test]
fn result_stays_put_in_the_real_sketch_mixed() {
    let mut s = sketch();
    let t = mixed(&mut s);
    let seen = check_grid(&s, t, 1.0);
    assert!(seen > pointers().len() / 2, "nur {seen} Lösungen");
}

#[test]
fn result_stays_put_in_the_real_sketch_circles_and_arcs_only() {
    let mut s = sketch();
    let t = [
        circle(&mut s, (0.0, 0.0), 6.0),
        arc(&mut s, (25.0, 3.0), 5.0, 1.0, 4.0),
        arc(&mut s, (10.0, 28.0), 7.0, 3.0, 6.0),
    ];
    let seen = check_grid(&s, t, 1.0);
    assert!(seen > pointers().len() / 2, "nur {seen} Lösungen");
}

#[test]
fn result_stays_put_far_from_the_origin() {
    // 100 m vom Ursprung: Die strengere Toleranz der Wegwerfskizze darf hier nicht scheitern.
    let o = 1e5;
    let mut s = sketch();
    let t = [
        line(&mut s, (o - 50.0, o), (o + 50.0, o)),
        circle(&mut s, (o, o + 30.0), 10.0),
        arc(&mut s, (o + 30.0, o + 30.0), 8.0, 2.0, 5.0),
    ];
    let (m, r) = tangent_circle_three(&s, t, v(o + 15.0, o + 10.0)).expect("Lösung");
    assert!(m.distance(v(o + 16.49, o + 13.4)) < 0.02, "{m:?}");
    assert_stays(&s, t, m, r, "weit draußen");
}

// ---------- Rein und reproduzierbar ----------

#[test]
fn same_input_gives_bit_identical_output_and_leaves_the_sketch_alone() {
    let bits =
        |r: Option<(DVec2, f64)>| r.map(|(m, r)| (m.x.to_bits(), m.y.to_bits(), r.to_bits()));
    let mut s = sketch();
    let t = mixed(&mut s);
    let rev = s.revision();
    let vars: Vec<u64> = s.variables().iter().map(|x| x.to_bits()).collect();
    let first = bits(tangent_circle_three(&s, t, v(15.0, 10.0)));
    assert!(first.is_some());
    for _ in 0..20 {
        assert_eq!(bits(tangent_circle_three(&s, t, v(15.0, 10.0))), first);
    }
    assert_eq!(s.revision(), rev);
    let now: Vec<u64> = s.variables().iter().map(|x| x.to_bits()).collect();
    assert_eq!(now, vars);

    let lines = triangle(&mut s);
    let rev = s.revision();
    let a = bits(tangent_circle_three(&s, lines, v(1.0, 1.0)));
    assert_eq!(bits(tangent_circle_three(&s, lines, v(1.0, 1.0))), a);
    let two = |s: &Sketch| {
        bits(tangent_circle_two_lines(
            s,
            lines[0],
            lines[2],
            v(1.0, 2.0),
            None,
        ))
    };
    assert_eq!(two(&s), two(&s));
    assert_eq!(s.revision(), rev);
}

// ---------- Auswahl ----------

#[test]
fn only_lines_circles_and_arcs_are_candidates() {
    let mut s = sketch();
    let l = line(&mut s, (0.0, 0.0), (1.0, 0.0));
    let c = circle(&mut s, (5.0, 5.0), 1.0);
    let a = arc(&mut s, (9.0, 5.0), 1.0, 0.0, 1.0);
    let m = s.add_point(0.0, 9.0).unwrap();
    let e = s.add_ellipse(m, v(3.0, 0.0), 1.0).unwrap();
    let (p0, p1) = (
        s.add_point(0.0, 20.0).unwrap(),
        s.add_point(3.0, 21.0).unwrap(),
    );
    let sp = s.add_spline(&[p0, p1]).unwrap();
    let gone = line(&mut s, (0.0, 30.0), (1.0, 30.0));
    s.remove_curve(gone).unwrap();

    assert!(is_tangent_candidate(&s, l));
    assert!(is_tangent_candidate(&s, c));
    assert!(is_tangent_candidate(&s, a));
    assert!(!is_tangent_candidate(&s, e));
    assert!(!is_tangent_candidate(&s, sp));
    assert!(!is_tangent_candidate(&s, gone));

    let p = v(3.0, 3.0);
    assert_eq!(tangent_circle_three(&s, [l, c, e], p), None);
    assert_eq!(tangent_circle_three(&s, [l, c, gone], p), None);
    assert_eq!(tangent_circle_three(&s, [l, c, c], p), None);
    assert_eq!(tangent_circle_three(&s, [l, c, a], v(f64::NAN, 0.0)), None);
    assert_eq!(tangent_circle_two_lines(&s, l, gone, p, None), None);
}
