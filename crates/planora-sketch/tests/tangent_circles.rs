//! Tangentialkreise (Skizzen 3, Task 3): Der Zeiger wählt per Wertung unter allen Lösungen,
//! gerechnet wird analytisch. Der Solver der echten Skizze nimmt danach nie still eine andere Lösung.

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
    assert_circle(tangent_circle_three(&s, t, v(5.0, 2.5)), (6.0, 6.0), 6.0);
    assert_circle(tangent_circle_three(&s, t, v(-1.0, 1.5)), (-3.0, 3.0), 3.0);
}

#[test]
fn equal_score_prefers_the_nearer_center() {
    // Zwei Lösungen mit gleichem Radius, (5,5) und (−5,5). Zwischen ihnen hat jeder Zeiger auf
    // y = 5 denselben Abstand zu beiden Kreislinien, die Wertung ist also gleich. Beide Seiten
    // zusammen schließen aus, dass nur die Reihenfolge der Lösungen entscheidet.
    let mut s = sketch();
    let t = [
        line(&mut s, (0.0, 0.0), (20.0, 0.0)),
        line(&mut s, (0.0, 10.0), (20.0, 10.0)),
        line(&mut s, (0.0, 0.0), (0.0, 10.0)),
    ];
    assert_circle(tangent_circle_three(&s, t, v(2.0, 5.0)), (5.0, 5.0), 5.0);
    assert_circle(tangent_circle_three(&s, t, v(-2.0, 5.0)), (-5.0, 5.0), 5.0);
    // Die Reihenfolge der Kurven entscheidet nichts.
    assert_circle(
        tangent_circle_three(&s, [t[2], t[0], t[1]], v(2.0, 5.0)),
        (5.0, 5.0),
        5.0,
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
    assert_closest((m, r), v(15.0, 10.0), &by_hand(0.0));
    // Von Hand: Diese Lösung umschließt den Kreis und liegt außen am Bogen.
    assert!(m.distance(v(27.114, 19.189)) < 0.01, "{m:?}");
    assert!((r - 19.189).abs() < 0.01, "{r}");
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
    assert_eq!(check_grid(&s, t, 1.0), pointers().len(), "immer lösbar");
}

#[test]
fn result_stays_put_in_the_real_sketch_circles_and_arcs_only() {
    let mut s = sketch();
    let t = [
        circle(&mut s, (0.0, 0.0), 6.0),
        arc(&mut s, (25.0, 3.0), 5.0, 1.0, 4.0),
        arc(&mut s, (10.0, 28.0), 7.0, 3.0, 6.0),
    ];
    assert_eq!(check_grid(&s, t, 1.0), pointers().len(), "immer lösbar");
}

#[test]
fn result_stays_put_far_from_the_origin() {
    // 100 m vom Ursprung: Auch dort reicht die Genauigkeit für jede der acht Lösungen.
    let o = 1e5;
    let mut s = sketch();
    let t = [
        line(&mut s, (o - 50.0, o), (o + 50.0, o)),
        circle(&mut s, (o, o + 30.0), 10.0),
        arc(&mut s, (o + 30.0, o + 30.0), 8.0, 2.0, 5.0),
    ];
    let all = by_hand(o);
    let got = tangent_circle_three(&s, t, v(o + 15.0, o + 10.0)).expect("Lösung");
    assert_closest(got, v(o + 15.0, o + 10.0), &all);
    for &(m, r) in &all {
        let p = m + v(0.0, r);
        let got = tangent_circle_three(&s, t, p).expect("Lösung");
        assert_closest(got, p, &all);
        assert!(got.0.distance(m) < 1e-6, "{got:?} statt {m:?}");
        assert_stays(&s, t, got.0, got.1, &format!("weit draußen, r {r}"));
    }
}

// ---------- Alle Lösungen gegen die Rechnung von Hand ----------

/// Alle Lösungen des Probelauf-Beispiels, um `o` in beide Richtungen verschoben, unabhängig vom
/// Modul aus den acht Vorzeichen: Die Linie gibt `y = s·r`, die Differenz der Kreisgleichungen
/// `x` linear in `r`, der Rest eine quadratische Gleichung in `r`.
fn by_hand(o: f64) -> Vec<(DVec2, f64)> {
    let mut out: Vec<(DVec2, f64)> = Vec::new();
    for s in [1.0, -1.0] {
        for s1 in [1.0, -1.0] {
            for s2 in [1.0, -1.0] {
                // x² + (s·r − 30)² = (r + 10·s1)² und (x − 30)² + (s·r − 30)² = (r + 8·s2)²
                let (p, q) = (936.0 / 60.0, (20.0 * s1 - 16.0 * s2) / 60.0);
                let (a, b, c) = (q * q, 2.0 * p * q - 60.0 * s - 20.0 * s1, p * p + 800.0);
                let disc: f64 = b * b - 4.0 * a * c;
                if disc < 0.0 {
                    continue;
                }
                for r in [
                    (-b + disc.sqrt()) / (2.0 * a),
                    (-b - disc.sqrt()) / (2.0 * a),
                ] {
                    if r > TOL {
                        out.push((v(o + p + q * r, o + s * r), r));
                    }
                }
            }
        }
    }
    out
}

/// Die Wertung aus Entscheidung 8, hier zur Probe von Hand über alle Lösungen.
fn assert_closest(got: (DVec2, f64), p: DVec2, all: &[(DVec2, f64)]) {
    let score = |&(m, r): &(DVec2, f64)| (m.distance(p) - r).abs() + 0.05 * r;
    let best_score = all.iter().map(score).fold(f64::INFINITY, f64::min);
    let best = all
        .iter()
        .filter(|c| score(c) <= best_score + TOL)
        .min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p)))
        .unwrap();
    assert!(
        got.0.distance(best.0) < 1e-6 && (got.1 - best.1).abs() < 1e-6,
        "Zeiger {p:?}: bekommen {got:?}, am besten liegt {best:?}"
    );
}

#[test]
fn the_example_has_eight_solutions_by_hand() {
    let all = by_hand(0.0);
    assert_eq!(all.len(), 8, "{all:?}");
    let mut s = sketch();
    let t = mixed(&mut s);
    for &(m, r) in &all {
        for c in t {
            let e = tangent_error(&s, c, m, r);
            assert!(e < 1e-6, "{m:?} r {r}: Abweichung {e}");
        }
    }
}

#[test]
fn pointer_on_a_ring_gets_the_best_score_of_all_solutions() {
    let all = by_hand(0.0);
    let mut s = sketch();
    let t = mixed(&mut s);
    for radius in [5.0, 25.0, 60.0] {
        for k in 0..24 {
            let a = f64::from(k) * std::f64::consts::TAU / 24.0;
            let p = v(15.0, 20.0) + v(a.cos(), a.sin()) * radius;
            let got = tangent_circle_three(&s, t, p).expect("Lösung");
            assert_closest(got, p, &all);
        }
    }
}

#[test]
fn pointer_on_the_rim_of_any_solution_gets_the_best_score() {
    let all = by_hand(0.0);
    let mut s = sketch();
    let t = mixed(&mut s);
    for &(m, r) in &all {
        for k in 0..24 {
            let a = f64::from(k) * std::f64::consts::TAU / 24.0;
            let p = m + v(a.cos(), a.sin()) * r;
            let got = tangent_circle_three(&s, t, p).expect("Lösung");
            assert_closest(got, p, &all);
        }
    }
}

#[test]
fn every_solution_can_still_be_reached_with_the_pointer() {
    // Auch die sehr großen: ganz oben auf ihrer Kreislinie, also weit weg von den übrigen.
    let all = by_hand(0.0);
    let mut s = sketch();
    let t = mixed(&mut s);
    for &(m, r) in &all {
        let got = tangent_circle_three(&s, t, m + v(0.0, r)).expect("Lösung");
        assert!(
            got.0.distance(m) < 1e-6 && (got.1 - r).abs() < 1e-6,
            "oben auf {m:?} r {r}: bekommen {got:?}"
        );
    }
}

#[test]
fn near_the_curves_the_winner_is_not_a_huge_circle() {
    let all = by_hand(0.0);
    let smallest = all.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let mut s = sketch();
    let t = mixed(&mut s);
    let (mut near, mut over_100, mut over_1000, mut largest) = (0, 0, 0, 0.0f64);
    for i in -40..=70 {
        for j in -60..=80 {
            let p = v(f64::from(i) + 0.37, f64::from(j) + 0.11);
            let to_curves = t
                .iter()
                .map(|&c| s.distance_to_curve(c, p).unwrap())
                .fold(f64::INFINITY, f64::min);
            if to_curves >= 15.0 {
                continue;
            }
            near += 1;
            let (_, r) = tangent_circle_three(&s, t, p).expect("Lösung");
            largest = largest.max(r);
            over_100 += usize::from(r >= 100.0);
            over_1000 += usize::from(r > 1000.0);
        }
    }
    assert_eq!(near, 5514);
    assert!(smallest < 100.0);
    assert_eq!(over_1000, 0, "größtes r {largest}");
    // Das Gewicht 0,05 lässt r ≈ 143 und r ≈ 151 an 685 Lagen gewinnen, vor allem neben den Enden
    // der Linie; unter 100 bleibt es hier erst ab etwa 0,3. Festgehalten, damit es auffällt.
    assert_eq!(over_100, 685);
    assert!(largest < 200.0, "größtes r {largest}");
}

#[test]
fn only_huge_solutions_still_come_back() {
    // Zwei parallele Linien im Abstand 20 m und eine Querlinie: Es gibt nur Kreise mit r = 10 m.
    let mut s = sketch();
    let t = [
        line(&mut s, (0.0, 0.0), (20.0, 0.0)),
        line(&mut s, (0.0, 20000.0), (20.0, 20000.0)),
        line(&mut s, (0.0, 0.0), (0.0, 10.0)),
    ];
    assert_circle(
        tangent_circle_three(&s, t, v(3.0, 5.0)),
        (10000.0, 10000.0),
        10000.0,
    );
}

#[test]
fn every_branch_of_the_example_is_held_by_the_real_solver() {
    let mut s = sketch();
    let t = mixed(&mut s);
    for (m, r) in by_hand(0.0) {
        // Die Werte, die die Funktion liefert, nicht die der Probe: ganz oben auf der Kreislinie
        // gewinnt jede Lösung.
        let got = tangent_circle_three(&s, t, m + v(0.0, r)).expect("Lösung");
        assert!(got.0.distance(m) < 1e-6, "{got:?} statt {m:?}");
        let what = format!("Lösung {m:?} r {r}");
        assert_stays(&s, t, got.0, got.1, &what);
        assert_fully_defined(&s, t, got.0, got.1, &what);
    }
}

#[test]
fn the_example_always_has_a_result() {
    let mut s = sketch();
    let t = mixed(&mut s);
    let mut none = 0;
    let mut total = 0;
    for i in -40..=70 {
        for j in -60..=80 {
            total += 1;
            let p = v(f64::from(i) + 0.37, f64::from(j) + 0.11);
            if tangent_circle_three(&s, t, p).is_none() {
                none += 1;
            }
        }
    }
    assert_eq!(none, 0, "{none} von {total} Zeigerlagen ohne Ergebnis");
}

#[test]
fn degenerate_arrangements_still_give_their_solutions() {
    // Alle Mitten auf einer Geraden: gespiegelte Lösungen oben und unten.
    let mut s = sketch();
    let t = [
        circle(&mut s, (0.0, 0.0), 2.0),
        circle(&mut s, (10.0, 0.0), 2.0),
        circle(&mut s, (20.0, 0.0), 2.0),
    ];
    for sign in [1.0, -1.0] {
        let p = v(5.0, sign * 6.0);
        let (m, r) = tangent_circle_three(&s, t, p).expect("Lösung");
        for c in t {
            let e = tangent_error(&s, c, m, r);
            assert!(e < 1e-9, "{m:?} r {r}: Abweichung {e}");
        }
        assert_stays(&s, t, m, r, "Mitten auf einer Geraden");
    }
    // Zwei parallele Linien und ein Kreis dazwischen: r = halber Abstand; der Zeiger wählt die Lösung außen am Kreis.
    let mut s = sketch();
    let t = [
        line(&mut s, (-50.0, 0.0), (50.0, 0.0)),
        line(&mut s, (50.0, 10.0), (-50.0, 10.0)),
        circle(&mut s, (0.0, 5.0), 2.0),
    ];
    let x = 7.0;
    assert_circle(tangent_circle_three(&s, t, v(13.0, 5.0)), (x, 5.0), 5.0);
    assert_circle(tangent_circle_three(&s, t, v(-13.0, 5.0)), (-x, 5.0), 5.0);
    assert_stays(&s, t, v(x, 5.0), 5.0, "parallel und Kreis");
    // Der Kreis liegt weit außerhalb des Streifens: Es gibt keine Lösung.
    let mut s = sketch();
    let t = [
        line(&mut s, (-50.0, 0.0), (50.0, 0.0)),
        line(&mut s, (-50.0, 10.0), (50.0, 10.0)),
        circle(&mut s, (0.0, 40.0), 2.0),
    ];
    assert_eq!(tangent_circle_three(&s, t, v(3.0, 5.0)), None);
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
