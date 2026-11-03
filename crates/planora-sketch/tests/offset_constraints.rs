//! Versatz mit Abhängigkeiten (Skizzen 4, Task 4): Freiheitsgrade wie gemessen, die Vorlage bleibt
//! stehen, die Kopie folgt ihr, dichte Wertefolgen statt Stichproben.

use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DVec2, DVec3, Frame, TAU, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::offset::{OFFSET_TIED_MAX, OffsetGeom, OffsetIds, OffsetPlan};
use planora_sketch::shapes::folded_over;
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn dof(s: &Sketch) -> usize {
    s.clone().solve().dof
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

/// `Solved`, nichts überzählig, kein Widerspruch.
fn assert_clean(r: &SolveReport) {
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.redundant.is_empty(), "überzählig: {r:?}");
    assert!(r.conflicting.is_empty(), "Widerspruch: {r:?}");
}

/// Wie [`assert_clean`], und beim Lösen bewegt sich nichts.
fn assert_still(s: &Sketch) -> SolveReport {
    let mut solved = s.clone();
    let r = solved.solve();
    assert_clean(&r);
    assert_eq!(r.iterations, 0, "{r:?}");
    assert_eq!(solved.variables(), s.variables());
    r
}

/// Keine Abhängigkeit zeigt auf eine tote Kurve, keine lebende Kurve auf einen toten Punkt.
fn assert_no_dead_refs(s: &Sketch) {
    for (id, d) in s.constraints().chain(s.internal_constraints()) {
        for c in d.kind.curves() {
            assert!(s.curve(c).is_some(), "{id:?}: {:?}", d.kind);
        }
        for p in d.kind.points() {
            assert!(s.point_xy(p).is_ok(), "{id:?}: {:?}", d.kind);
        }
    }
    for (_, c) in s.curves() {
        for p in c.kind.points() {
            assert!(s.point_xy(p).is_ok(), "{:?}", c.kind);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Formen
// ---------------------------------------------------------------------------------------------

fn single_line() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let (a, b) = (s.add_point(0., 0.).unwrap(), s.add_point(40., 0.).unwrap());
    let l = s.add_line(a, b).unwrap();
    (s, vec![l])
}

fn single_arc() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let m = s.add_point(0., 0.).unwrap();
    let a = s.add_point(20., 0.).unwrap();
    let b = s.add_point(0., 20.).unwrap();
    let c = s.add_arc(m, a, b).unwrap();
    (s, vec![c])
}

fn single_circle() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let m = s.add_point(3., 4.).unwrap();
    let c = s.add_circle(m, 20.).unwrap();
    (s, vec![c])
}

fn polygon(n: usize, big: f64) -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let ids: Vec<_> = (0..n)
        .map(|k| {
            let a = TAU * k as f64 / n as f64 + 0.1;
            s.add_point(big * a.cos(), big * a.sin()).unwrap()
        })
        .collect();
    let curves = (0..n)
        .map(|i| s.add_line(ids[i], ids[(i + 1) % n]).unwrap())
        .collect();
    (s, curves)
}

/// Vieleck, die Ecken mit Radius `rr[k % rr.len()]` gerundet (0: scharf). Gegen den Uhrzeigersinn
/// angegeben, konkave Ecken bekommen einen gegenläufig durchlaufenen Bogen.
fn rounded(pts: &[(f64, f64)], rr: &[f64]) -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let n = pts.len();
    let at = |k: usize| DVec2::new(pts[k % n].0, pts[k % n].1);
    let mut curves = Vec::new();
    let (mut first, mut last) = (None, None);
    for k in 0..n {
        let r = rr[k % rr.len()];
        let (p0, p1, p2) = (at(k + n - 1), at(k), at(k + 1));
        let (u, w) = ((p1 - p0).normalize(), (p2 - p1).normalize());
        let turn = u.perp_dot(w).atan2(u.dot(w));
        let inset = r * (turn.abs() / 2.0).tan();
        let (a, b) = (p1 - u * inset, p1 + w * inset);
        let ia = s.add_point(a.x, a.y).unwrap();
        let ib = if r > 0.0 {
            s.add_point(b.x, b.y).unwrap()
        } else {
            ia
        };
        if let Some(prev) = last {
            curves.push(s.add_line(prev, ia).unwrap());
        } else {
            first = Some(ia);
        }
        if r > 0.0 {
            let left = turn > 0.0;
            let m = a + u.perp() * if left { r } else { -r };
            let ic = s.add_point(m.x, m.y).unwrap();
            curves.push(if left {
                s.add_arc(ic, ia, ib).unwrap()
            } else {
                s.add_arc(ic, ib, ia).unwrap()
            });
        }
        last = Some(ib);
    }
    curves.push(s.add_line(last.unwrap(), first.unwrap()).unwrap());
    (s, curves)
}

fn rounded_rect() -> (Sketch, Vec<CurveId>) {
    rounded(&[(0., 0.), (80., 0.), (80., 50.), (0., 50.)], &[8.0])
}

/// L-Form mit zwei gerundeten Ecken, einer erhabenen und einer hohlen, je r = 6.
fn l_form() -> (Sketch, Vec<CurveId>) {
    rounded(
        &[
            (0., 0.),
            (100., 0.),
            (100., 40.),
            (40., 40.),
            (40., 100.),
            (0., 100.),
        ],
        &[0., 0., 6., 6., 0., 0.],
    )
}

/// Die Vorlage selbst trägt ihre Tangenten, wie nach einer Rundung.
fn tied((mut s, cs): (Sketch, Vec<CurveId>)) -> (Sketch, Vec<CurveId>) {
    let arc = |s: &Sketch, c: CurveId| matches!(s.curve(c).unwrap().kind, CurveKind::Arc { .. });
    for i in 0..cs.len() {
        let (x, y) = (cs[i], cs[(i + 1) % cs.len()]);
        let shared = s
            .curve(x)
            .unwrap()
            .kind
            .points()
            .into_iter()
            .any(|p| s.curve(y).unwrap().kind.points().contains(&p));
        if shared && (arc(&s, x) || arc(&s, y)) {
            s.add_constraint(ConstraintKind::Tangent(x, y)).unwrap();
        }
    }
    assert_clean(&s.solve());
    (s, cs)
}

fn open_chain() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let (a, b) = (s.add_point(0., 0.).unwrap(), s.add_point(50., 0.).unwrap());
    let (c, m) = (
        s.add_point(60., 10.).unwrap(),
        s.add_point(50., 10.).unwrap(),
    );
    let d = s.add_point(60., 50.).unwrap();
    let e = s.add_point(90., 70.).unwrap();
    let curves = vec![
        s.add_line(a, b).unwrap(),
        s.add_arc(m, b, c).unwrap(),
        s.add_line(c, d).unwrap(),
        s.add_line(d, e).unwrap(),
    ];
    (s, curves)
}

// ---------------------------------------------------------------------------------------------
// Prüfstand
// ---------------------------------------------------------------------------------------------

/// Eine Skizze mit Versatz und was es zum Prüfen braucht.
struct Bench {
    s: Sketch,
    /// `(Vorlage, Stück der Kopie)`.
    pairs: Vec<(CurveId, CurveId)>,
    sources: Vec<CurveId>,
    ids: OffsetIds,
    /// Vorzeichen der Lage je Paar, vom Anlegen.
    sides: Vec<f64>,
    d: f64,
}

fn plan_of(s: &Sketch, curves: &[CurveId], d: f64) -> OffsetPlan {
    let chains = s.ordered_chains(curves).unwrap();
    s.offset_plan(&chains, d).unwrap()
}

/// Abstand mit Vorzeichen des Punktes zum Träger der Vorlage: Linie links positiv, runde Vorlage
/// außerhalb positiv.
fn to_carrier(s: &Sketch, source: CurveId, p: DVec2) -> f64 {
    match s.curve(source).unwrap().kind {
        CurveKind::Line { a, b } => {
            let (a, b) = (at(s, a), at(s, b));
            (b - a).normalize().perp_dot(p - a)
        }
        CurveKind::Arc { center, .. } | CurveKind::Circle { center, .. } => {
            p.distance(at(s, center)) - s.radius(source).unwrap()
        }
        ref k => panic!("keine exakte Vorlage: {k:?}"),
    }
}

/// Stellen auf einem Stück der Kopie: Enden und Mitte.
fn samples(s: &Sketch, c: CurveId) -> Vec<DVec2> {
    match s.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => {
            let (a, b) = (at(s, a), at(s, b));
            vec![a, (a + b) * 0.5, b]
        }
        CurveKind::Arc { center, start, end } => {
            let (m, a, b) = (at(s, center), at(s, start), at(s, end));
            let r = (a - m).length();
            let mid = if (a - m + (b - m)).length() > 1e-9 {
                m + (a - m + (b - m)).normalize() * r
            } else {
                m + (a - m).perp().normalize() * r
            };
            vec![a, mid, b]
        }
        CurveKind::Circle { center, .. } => {
            let (m, r) = (at(s, center), s.radius(c).unwrap());
            (0..4)
                .map(|k| m + DVec2::from_angle(k as f64 * TAU / 4.0) * r)
                .collect()
        }
        ref k => panic!("kein exaktes Stück: {k:?}"),
    }
}

impl Bench {
    fn new((mut s, sources): (Sketch, Vec<CurveId>), d: f64) -> Bench {
        let plan = plan_of(&s, &sources, d);
        let ids = s.add_offset(&plan).unwrap();
        let sources_of: Vec<CurveId> = plan
            .loops
            .iter()
            .flat_map(|l| l.pieces.iter().map(|p| p.source))
            .collect();
        let pairs: Vec<(CurveId, CurveId)> =
            sources_of.into_iter().zip(ids.curves.clone()).collect();
        let sides = pairs
            .iter()
            .map(|&(q, k)| to_carrier(&s, q, samples(&s, k)[1]).signum())
            .collect();
        Bench {
            s,
            pairs,
            sources,
            ids,
            sides,
            d: d.abs(),
        }
    }

    fn dim(&self) -> ConstraintId {
        self.ids.distance_dim.expect("Maß")
    }

    fn hold(&self) -> Vec<PointId> {
        self.s.offset_hold_points(self.dim()).expect("Haltepunkte")
    }

    /// Alle Stücke im Abstand `d` auf ihrer Seite, Fehler unter `limit`.
    fn check_distance(&self, s: &Sketch, d: f64, limit: f64, what: &str) {
        for (i, &(q, k)) in self.pairs.iter().enumerate() {
            for p in samples(s, k) {
                let e = to_carrier(s, q, p);
                assert!(
                    (e - self.sides[i] * d).abs() < limit,
                    "{what}: Stück {i}, Abstand {e} statt {d}"
                );
            }
        }
    }

    /// Variablen aller Punkte und Radien der Vorlage.
    fn source_state(&self, s: &Sketch) -> Vec<(u64, u64)> {
        let mut out = Vec::new();
        for &c in &self.sources {
            for p in s.curve(c).unwrap().kind.points() {
                let v = at(s, p);
                out.push((v.x.to_bits(), v.y.to_bits()));
            }
            if let Some(r) = s.radius(c) {
                out.push((r.to_bits(), 0));
            }
        }
        out
    }

    /// Das Maß läuft durch `values`: gelöst, Vorlage bitgleich, Kopie stimmt, nie umgeklappt.
    fn sweep_held(&mut self, values: impl Iterator<Item = f64>, what: &str) {
        let (dim, hold) = (self.dim(), self.hold());
        let state = self.source_state(&self.s);
        let mut s = self.s.clone();
        for v in values {
            let before = s.clone();
            s.set_constraint_value(dim, v).unwrap();
            assert!(s.solve_holding(&hold), "{what}: d = {v} nicht gelöst");
            assert!(!folded_over(&before, &s), "{what}: d = {v} umgeklappt");
            assert_eq!(self.source_state(&s), state, "{what}: d = {v} Vorlage");
            self.check_distance(&s, v, 1e-6, what);
        }
        self.s = s;
    }
}

/// `from..=to` in `steps` gleichen Schritten.
fn values(from: f64, to: f64, steps: usize) -> impl Iterator<Item = f64> {
    (0..=steps).map(move |k| from + (to - from) * k as f64 / steps as f64)
}

// ---------------------------------------------------------------------------------------------
// Tabelle aus Entscheidung 3
// ---------------------------------------------------------------------------------------------

fn count(s: &Sketch, f: impl Fn(&ConstraintKind) -> bool) -> usize {
    s.constraints().filter(|(_, d)| f(&d.kind)).count()
}

fn is_parallel(k: &ConstraintKind) -> bool {
    matches!(k, ConstraintKind::Parallel(..))
}

fn is_equal(k: &ConstraintKind) -> bool {
    matches!(k, ConstraintKind::Equal(..))
}

fn is_radius(k: &ConstraintKind) -> bool {
    matches!(k, ConstraintKind::Radius(..))
}

/// Vorlage ohne jede Bewegung, `Solved`, nichts überzählig, keine Schritte, rel 0.
fn check_table(shape: (Sketch, Vec<CurveId>), d: f64, what: &str) -> Bench {
    let before = shape.0.clone();
    let dof_before = dof(&before);
    let bench = Bench::new(shape, d);
    assert_still(&bench.s);
    assert_eq!(dof(&bench.s), dof_before, "{what}: rel");
    assert!(bench.ids.tied, "{what}");
    for (i, v) in before.variables().iter().enumerate() {
        assert_eq!(v.to_bits(), bench.s.variables()[i].to_bits(), "{what}: {i}");
    }
    bench.check_distance(&bench.s, d.abs(), 1e-9, what);
    for &c in &bench.ids.rollers {
        assert!(bench.s.curve(c).unwrap().construction, "{what}");
    }
    bench
}

#[test]
fn the_table_of_decision_3_holds() {
    check_table(single_line(), 5.0, "Linie links");
    check_table(single_line(), -5.0, "Linie rechts");
    check_table(single_arc(), 5.0, "Bogen innen");
    check_table(single_arc(), -5.0, "Bogen außen");
    check_table(single_circle(), 5.0, "Kreis innen");
    check_table(single_circle(), -5.0, "Kreis außen");
    for n in [3, 4, 5, 8, 64] {
        check_table(polygon(n, 200.0), 5.0, &format!("n = {n} innen"));
        check_table(polygon(n, 200.0), -5.0, &format!("n = {n} außen"));
    }
    check_table(rounded_rect(), 3.0, "abgerundet innen");
    check_table(rounded_rect(), -3.0, "abgerundet außen");
    check_table(tied(rounded_rect()), -3.0, "abgerundet, Vorlage tangential");
    check_table(l_form(), 3.0, "L innen");
    check_table(l_form(), -3.0, "L außen");
    check_table(open_chain(), 3.0, "offen links");
    check_table(open_chain(), -3.0, "offen rechts");
}

#[test]
fn a_chain_that_is_tangent_all_round_has_one_roller_and_one_parallel_less() {
    let b = check_table(rounded_rect(), -3.0, "abgerundet");
    assert_eq!(b.ids.rollers.len(), 1);
    assert_eq!(count(&b.s, is_parallel), 3, "eine weniger als Linien");
    assert_eq!(count(&b.s, is_radius), 1);
    assert_eq!(count(&b.s, is_equal), 0);
}

#[test]
fn a_square_has_four_rollers_one_radius_three_equal() {
    let b = check_table(polygon(4, 50.0), -5.0, "Quadrat");
    assert_eq!(b.ids.rollers.len(), 4);
    assert_eq!(count(&b.s, is_parallel), 4);
    assert_eq!(count(&b.s, is_radius), 1);
    assert_eq!(count(&b.s, is_equal), 3);
    assert_eq!(b.ids.curves.len(), 4);
    assert!(matches!(
        b.s.constraint(b.dim()).unwrap().kind,
        ConstraintKind::Radius(c, v) if c == b.ids.rollers[0] && v == 5.0
    ));
}

#[test]
fn curves_of_all_arcs_tangent_all_round_are_clean() {
    // Oval aus vier Bögen: Hier ist eine Tangente überzählig, keine Parallele.
    let mut s = sketch();
    let p: Vec<_> = [(39., -12.), (39., 12.), (-39., 12.), (-39., -12.)]
        .iter()
        .map(|q| s.add_point(q.0, q.1).unwrap())
        .collect();
    let c: Vec<_> = [(30., 0.), (0., -40.), (-30., 0.), (0., 40.)]
        .iter()
        .map(|q| s.add_point(q.0, q.1).unwrap())
        .collect();
    let cs: Vec<_> = (0..4)
        .map(|k| s.add_arc(c[k], p[k], p[(k + 1) % 4]).unwrap())
        .collect();
    let b = check_table((s, cs), -2.0, "Oval");
    assert_eq!(b.ids.rollers.len(), 1);
}

#[test]
fn copy_arcs_share_the_center_of_the_source_and_style_follows() {
    let (mut s, cs) = single_arc();
    s.set_construction(cs[0], true).unwrap();
    let b = Bench::new((s, cs.clone()), -5.0);
    let center_of = |c: CurveId| match b.s.curve(c).unwrap().kind {
        CurveKind::Arc { center, .. } => center,
        ref k => panic!("{k:?}"),
    };
    assert_eq!(center_of(b.ids.curves[0]), center_of(cs[0]));
    assert!(b.s.curve(b.ids.curves[0]).unwrap().construction);

    let (mut s, cs) = single_line();
    s.set_construction(cs[0], true).unwrap();
    s.set_centerline(cs[0], true).unwrap();
    let b = Bench::new((s, cs), 5.0);
    assert!(b.s.curve(b.ids.curves[0]).unwrap().centerline);
}

#[test]
fn a_plan_that_no_longer_fits_is_an_error_and_changes_nothing() {
    let (mut s, cs) = single_arc();
    let plan = plan_of(&s, &cs, 5.0);
    s.remove_curve(cs[0]).unwrap();
    let rev = s.revision();
    assert!(s.add_offset(&plan).is_err());
    assert_eq!(s.revision(), rev);
}

// ---------------------------------------------------------------------------------------------
// Dichte Folgen des Maßes
// ---------------------------------------------------------------------------------------------

#[test]
fn distance_sweep_on_all_polygons() {
    for n in [3, 4, 5, 6, 7, 8, 12, 16] {
        for sign in [1.0, -1.0] {
            let mut b = Bench::new(polygon(n, 200.0), sign * 5.0);
            b.sweep_held(values(0.5, 50.0, 100), &format!("n = {n}, {sign}"));
        }
    }
}

/// Die ganze Folge mit 100 Schritten läuft in `offset_perf` im Release-Build.
#[test]
fn distance_sweep_on_a_polygon_of_64() {
    let mut b = Bench::new(polygon(64, 200.0), -5.0);
    b.sweep_held(values(0.5, 50.0, 6), "n = 64");
}

#[test]
fn distance_sweep_on_circle_and_arc() {
    for shape in [single_circle, single_arc] {
        let mut out = Bench::new(shape(), -5.0);
        out.sweep_held(values(0.1, 50.0, 500), "außen");
        let mut inner = Bench::new(shape(), 5.0);
        inner.sweep_held(values(0.1, 19.0, 190), "innen");
        // Bis an die Grenze und dahinter: abgelehnt, Stand bleibt.
        let (dim, hold) = (inner.dim(), inner.hold());
        for v in [20.0, 20.5, 40.0] {
            let mut t = inner.s.clone();
            t.set_constraint_value(dim, v).unwrap();
            let vars = t.variables().to_vec();
            assert!(!t.solve_holding(&hold), "d = {v}");
            assert_eq!(t.variables(), vars.as_slice(), "d = {v}: Stand bleibt");
        }
    }
}

#[test]
fn distance_sweep_on_the_l_form_stops_at_the_radius() {
    for sign in [1.0, -1.0] {
        let mut b = Bench::new(l_form(), sign * 3.0);
        b.sweep_held(values(0.05, 5.95, 118), &format!("L, {sign}"));
        let (dim, hold) = (b.dim(), b.hold());
        for v in values(6.0, 12.0, 24) {
            let mut t = b.s.clone();
            t.set_constraint_value(dim, v).unwrap();
            let vars = t.variables().to_vec();
            assert!(!t.solve_holding(&hold), "L, {sign}: d = {v}");
            assert_eq!(t.variables(), vars.as_slice(), "d = {v}: Stand bleibt");
        }
    }
}

#[test]
fn distance_sweep_on_open_chain_and_rounded_rect() {
    let mut b = Bench::new(rounded_rect(), -3.0);
    b.sweep_held(values(0.1, 50.0, 200), "abgerundet außen");
    let mut b = Bench::new(rounded_rect(), 3.0);
    b.sweep_held(values(0.1, 7.9, 100), "abgerundet innen");
    let mut b = Bench::new(open_chain(), 3.0);
    b.sweep_held(values(0.1, 9.9, 100), "offen links");
    let mut b = Bench::new(open_chain(), -3.0);
    b.sweep_held(values(0.1, 30.0, 200), "offen rechts");
}

/// Der Radius der Vorlage läuft mit dem Maß `Radius(q, r)` von 0,25 bis 100.
fn source_radius_sweep(shape: fn() -> (Sketch, Vec<CurveId>), d: f64, inward: bool) {
    let mut b = Bench::new(shape(), d);
    let q = b.sources[0];
    let r0 = b.s.radius(q).unwrap();
    let dim = b.s.add_constraint(ConstraintKind::Radius(q, r0)).unwrap();
    assert_clean(&b.s.solve());
    let mut s = b.s.clone();
    let (mut solved, mut rejected) = (0, 0);
    for k in 1..=400 {
        let r = 0.25 * k as f64;
        let before = s.clone();
        let mut t = s.clone();
        t.set_constraint_value(dim, r).unwrap();
        let report = t.solve();
        assert!(!folded_over(&before, &t), "r = {r}: umgeklappt");
        if report.status == SolveStatus::Solved {
            assert!(!(inward && r <= d.abs()), "r = {r}: müsste abgelehnt sein");
            b.check_distance(&t, d.abs(), 1e-6, &format!("r = {r}"));
            assert!((t.radius(q).unwrap() - r).abs() < 1e-6);
            s = t;
            solved += 1;
        } else {
            assert!(inward && r <= d.abs(), "r = {r}: {report:?}");
            rejected += 1;
        }
    }
    let want = if inward { (d.abs() / 0.25) as usize } else { 0 };
    assert_eq!(rejected, want);
    assert_eq!(solved, 400 - want);
    b.s = s;
}

#[test]
fn source_radius_sweep_on_circle() {
    source_radius_sweep(single_circle, -5.0, false);
    source_radius_sweep(single_circle, 5.0, true);
}

#[test]
fn source_radius_sweep_on_arc() {
    source_radius_sweep(single_arc, -5.0, false);
    source_radius_sweep(single_arc, 5.0, true);
}

// ---------------------------------------------------------------------------------------------
// Die Vorlage bleibt
// ---------------------------------------------------------------------------------------------

/// Alle Punkte der Vorlagekurven sind drin; von der Kopie nur, was sie mit der Vorlage teilt.
fn check_hold_cover(b: &Bench, what: &str) {
    let hold = b.hold();
    let mut source_points = Vec::new();
    for &c in &b.sources {
        let points = b.s.curve(c).unwrap().kind.points();
        for p in &points {
            assert!(hold.contains(p), "{what}: Vorlagepunkt {p:?} fehlt");
        }
        source_points.extend(points);
    }
    for &k in &b.ids.curves {
        for p in b.s.curve(k).unwrap().kind.points() {
            assert!(
                source_points.contains(&p) || !hold.contains(&p),
                "{what}: Punkt der Kopie {p:?} wird gehalten"
            );
        }
    }
}

#[test]
fn hold_points_cover_the_source_not_the_copy() {
    check_hold_cover(&Bench::new(single_line(), 5.0), "Linie");
    check_hold_cover(&Bench::new(single_arc(), 5.0), "Bogen");
    check_hold_cover(&Bench::new(single_circle(), 5.0), "Kreis");
    check_hold_cover(&Bench::new(polygon(5, 50.0), -5.0), "Fünfeck");
    check_hold_cover(&Bench::new(rounded_rect(), -3.0), "abgerundet");
    check_hold_cover(&Bench::new(l_form(), 3.0), "L");
    check_hold_cover(&Bench::new(open_chain(), 3.0), "offen");
}

#[test]
fn hold_points_of_a_construction_source_do_not_cross_to_the_copy() {
    for shape in [single_line, single_arc, open_chain] {
        let (mut s, cs) = shape();
        for &c in &cs {
            s.set_construction(c, true).unwrap();
        }
        let mut b = Bench::new((s, cs), 3.0);
        check_hold_cover(&b, "Konstruktion");
        b.sweep_held(values(0.5, 9.0, 40), "Konstruktion");
    }
}

#[test]
fn only_a_line_stays_when_the_distance_changes() {
    let mut b = Bench::new(single_line(), 5.0);
    // Ohne Halten geht die Vorlage mit (gemessen 3,5 mm).
    let mut free = b.s.clone();
    let (dim, hold) = (b.dim(), b.hold());
    let a = match free.curve(b.sources[0]).unwrap().kind {
        CurveKind::Line { a, .. } => a,
        _ => unreachable!(),
    };
    free.set_constraint_value(dim, 12.0).unwrap();
    free.solve();
    assert!(
        at(&free, a).distance(DVec2::ZERO) > 1.0,
        "{:?}",
        at(&free, a)
    );

    let state = b.source_state(&b.s);
    let rev = b.s.revision();
    b.s.set_constraint_value(dim, 12.0).unwrap();
    assert!(b.s.solve_holding(&hold));
    assert_ne!(b.s.revision(), rev);
    assert_eq!(b.source_state(&b.s), state);
    b.check_distance(&b.s, 12.0, 1e-6, "Linie");
    // Das nächste gewöhnliche Lösen bewegt nichts mehr.
    assert_still(&b.s);
}

#[test]
fn a_failed_hold_changes_nothing() {
    let b = Bench::new(single_circle(), 5.0);
    let (dim, hold) = (b.dim(), b.hold());
    let mut t = b.s.clone();
    t.set_constraint_value(dim, 30.0).unwrap();
    let (vars, rev) = (t.variables().to_vec(), t.revision());
    assert!(!t.solve_holding(&hold));
    assert_eq!(t.variables(), vars.as_slice());
    assert_eq!(t.revision(), rev);
}

#[test]
fn hold_points_are_none_for_ordinary_radii() {
    // Gewöhnlicher Radius eines Bogens.
    let (mut s, cs) = single_arc();
    let dim = s
        .add_constraint(ConstraintKind::Radius(cs[0], 20.0))
        .unwrap();
    assert_eq!(s.offset_hold_points(dim), None);

    // Von Hand gezeichneter Kreis, tangential an eine Linie.
    let (mut s, cs) = single_line();
    let m = s.add_point(10., 5.).unwrap();
    let c = s.add_circle(m, 5.).unwrap();
    s.add_constraint(ConstraintKind::Tangent(cs[0], c)).unwrap();
    let dim = s.add_constraint(ConstraintKind::Radius(c, 5.0)).unwrap();
    assert_eq!(s.offset_hold_points(dim), None);
    // Auch als Konstruktionskreis, solange seine Mitte zu keiner Kurve gehört.
    s.set_construction(c, true).unwrap();
    assert_eq!(s.offset_hold_points(dim), None);

    // Ein anderes Maß, eine tote Abhängigkeit.
    let (mut s, cs) = single_line();
    let (a, b) = match s.curve(cs[0]).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        _ => unreachable!(),
    };
    let dim = s
        .add_constraint(ConstraintKind::Distance(a, b, 40.0))
        .unwrap();
    assert_eq!(s.offset_hold_points(dim), None);
    s.remove_constraint(dim).unwrap();
    assert_eq!(s.offset_hold_points(dim), None);
}

// ---------------------------------------------------------------------------------------------
// Die Kopie folgt
// ---------------------------------------------------------------------------------------------

#[test]
fn dragging_a_source_corner_keeps_the_distance() {
    for (shape, d) in [
        (polygon(4, 50.0), -5.0),
        (polygon(4, 50.0), 5.0),
        (polygon(5, 50.0), -5.0),
        (tied(rounded_rect()), -3.0),
        (tied(rounded_rect()), 3.0),
        (tied(l_form()), 3.0),
        (tied(l_form()), -3.0),
        (tied(open_chain()), 3.0),
    ] {
        let mut b = Bench::new(shape, d);
        let corner = b
            .sources
            .iter()
            .find_map(|&c| match b.s.curve(c).unwrap().kind {
                CurveKind::Line { a, .. } => Some(a),
                _ => None,
            })
            .unwrap();
        let start = at(&b.s, corner);
        for k in 1..=40 {
            let target = start + DVec2::new(0.1, 0.05) * k as f64;
            let before = b.s.clone();
            let r =
                b.s.drag(&[Drag {
                    point: corner,
                    target,
                }])
                .unwrap();
            assert!(!r.drag_failed, "{r:?}");
            assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
            assert!(!folded_over(&before, &b.s), "k = {k}");
            let s = b.s.clone();
            for (i, &(q, k2)) in b.pairs.iter().enumerate() {
                for p in samples(&s, k2) {
                    let e = (to_carrier(&s, q, p) - b.sides[i] * b.d).abs();
                    assert!(e < 1e-12, "Stück {i}, k = {k}: Fehler {e}");
                }
            }
        }
    }
}

/// Grenze, die im Bericht steht: Ohne Tangenten an der Vorlage geht beim Ziehen die Berührung der
/// Vorlage selbst verloren. Die Kopie bleibt rundum tangential, kann also nicht überall stimmen,
/// aber nichts scheitert oder klappt um.
#[test]
fn dragging_a_source_without_tangents_stays_solved() {
    let mut b = Bench::new(rounded_rect(), -3.0);
    let corner = b
        .sources
        .iter()
        .find_map(|&c| match b.s.curve(c).unwrap().kind {
            CurveKind::Line { a, .. } => Some(a),
            _ => None,
        })
        .unwrap();
    let start = at(&b.s, corner);
    for k in 1..=40 {
        let before = b.s.clone();
        let target = start + DVec2::new(0.1, 0.05) * k as f64;
        let r =
            b.s.drag(&[Drag {
                point: corner,
                target,
            }])
            .unwrap();
        assert!(!r.drag_failed, "{r:?}");
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        assert!(!folded_over(&before, &b.s), "k = {k}");
    }
}

#[test]
fn a_dimension_on_the_source_line_is_followed() {
    let mut b = Bench::new(single_line(), 5.0);
    let (q, k) = b.pairs[0];
    let (a, c) = match b.s.curve(q).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        _ => unreachable!(),
    };
    let dim =
        b.s.add_constraint(ConstraintKind::Distance(a, c, 40.0))
            .unwrap();
    assert_clean(&b.s.solve());
    let mut s = b.s.clone();
    for v in values(0.5, 150.0, 300) {
        let before = s.clone();
        s.set_constraint_value(dim, v).unwrap();
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "L = {v}: {r:?}");
        assert!(!folded_over(&before, &s), "L = {v}");
        b.check_distance(&s, 5.0, 1e-6, "Linie");
        assert!((s.line_length(k).unwrap() - v).abs() < 1e-6, "L = {v}");
        let dir = |c: CurveId| match s.curve(c).unwrap().kind {
            CurveKind::Line { a, b } => (at(&s, b) - at(&s, a)).normalize(),
            _ => unreachable!(),
        };
        assert!(dir(q).perp_dot(dir(k)).abs() < 1e-6, "L = {v}");
    }
}

#[test]
fn a_fillet_radius_on_the_source_changes_the_copy() {
    use planora_sketch::fillet::CurvePick;
    let mut s = sketch();
    let (a, p, c) = (
        s.add_point(0., 0.).unwrap(),
        s.add_point(60., 0.).unwrap(),
        s.add_point(60., 60.).unwrap(),
    );
    let (l1, l2) = (s.add_line(a, p).unwrap(), s.add_line(p, c).unwrap());
    let pick = |curve: CurveId, x: f64, y: f64| CurvePick {
        curve,
        at: DVec2::new(x, y),
    };
    let plan = s
        .fillet_plan(pick(l1, 40., 0.), pick(l2, 60., 20.), 10.0)
        .unwrap();
    let fillet = s.add_fillet(&plan, None).unwrap();
    assert_clean(&s.solve());
    let radius_dim = fillet.radius_dim.unwrap();
    let mut b = Bench::new((s, vec![l1, fillet.arc, l2]), 3.0);
    let copy_arc = b
        .pairs
        .iter()
        .find(|p| p.0 == fillet.arc)
        .map(|p| p.1)
        .unwrap();
    assert_still(&b.s);
    let mut s = b.s.clone();
    for r in values(10.0, 25.0, 60).chain(values(25.0, 4.0, 84)) {
        let before = s.clone();
        s.prepare_dimension_change(radius_dim, r);
        s.set_constraint_value(radius_dim, r).unwrap();
        let report = s.solve();
        assert_eq!(report.status, SolveStatus::Solved, "r = {r}: {report:?}");
        assert!(!folded_over(&before, &s), "r = {r}");
        assert!((s.radius(fillet.arc).unwrap() - r).abs() < 1e-6, "r = {r}");
        assert!(
            (s.radius(copy_arc).unwrap() - (r - 3.0)).abs() < 1e-6,
            "r = {r}"
        );
        b.check_distance(&s, 3.0, 1e-6, "Rundung");
    }
    b.s = s;
}

// ---------------------------------------------------------------------------------------------
// Lange Ketten
// ---------------------------------------------------------------------------------------------

#[test]
fn a_chain_over_the_limit_is_plain_geometry() {
    assert_eq!(OFFSET_TIED_MAX, 64);
    let (mut s, cs) = polygon(65, 400.0);
    let plan = plan_of(&s, &cs, -5.0);
    let constraints = s.constraints().count();
    let vars = s.variables().to_vec();
    let ids = s.add_offset(&plan).unwrap();
    assert!(!ids.tied);
    assert_eq!(ids.distance_dim, None);
    assert!(ids.rollers.is_empty());
    assert_eq!(ids.curves.len(), 65);
    assert_eq!(s.constraints().count(), constraints);
    assert_eq!(count(&s, is_parallel), 0);
    assert_eq!(&s.variables()[..vars.len()], vars.as_slice());
    // Die Punkte teilen die Enden.
    let ends = |c: CurveId| match s.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        ref k => panic!("{k:?}"),
    };
    for i in 0..65 {
        assert_eq!(ends(ids.curves[i]).1, ends(ids.curves[(i + 1) % 65]).0);
    }
    assert_clean(&s.solve());
}

#[test]
fn arcs_over_the_limit_do_not_share_the_center() {
    // 33 Ecken gerundet: 66 Stücke.
    let pts: Vec<(f64, f64)> = (0..33)
        .map(|k| {
            let a = TAU * k as f64 / 33.0;
            (400.0 * a.cos(), 400.0 * a.sin())
        })
        .collect();
    let (mut s, cs) = rounded(&pts, &[5.0]);
    let plan = plan_of(&s, &cs, -3.0);
    assert_eq!(plan.piece_count(), 66);
    let ids = s.add_offset(&plan).unwrap();
    assert!(!ids.tied);
    let center_of = |c: CurveId| match s.curve(c).unwrap().kind {
        CurveKind::Arc { center, .. } => Some(center),
        _ => None,
    };
    let sources: Vec<_> = cs.iter().filter_map(|&c| center_of(c)).collect();
    for &c in &ids.curves {
        if let Some(m) = center_of(c) {
            assert!(!sources.contains(&m));
        }
    }
    assert_clean(&s.solve());
}

#[test]
fn the_limit_itself_is_still_tied() {
    let b = check_table(polygon(64, 400.0), -5.0, "n = 64");
    assert_eq!(b.ids.rollers.len(), 64);
}

// ---------------------------------------------------------------------------------------------
// Hilfsgeometrie im Weg (Review-Focus 2)
// ---------------------------------------------------------------------------------------------

/// Löst, die Vorlage bleibt bitgleich, nichts Totes bleibt zurück.
fn check_aftermath(b: &Bench, s: &mut Sketch, state: &[(u64, u64)]) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.conflicting.is_empty(), "{r:?}");
    assert_eq!(b.source_state(s), state);
    assert_no_dead_refs(s);
}

#[test]
fn deleting_a_roller_frees_the_copy_only() {
    let b = Bench::new(polygon(4, 50.0), -5.0);
    let state = b.source_state(&b.s);
    let mut s = b.s.clone();
    let free_before = dof(&s);
    s.remove_curve(b.ids.rollers[2]).unwrap();
    check_aftermath(&b, &mut s, &state);
    assert!(dof(&s) > free_before, "die Kopie wird frei");
    // Das Maß wirkt weiter auf den Rest.
    assert!(s.offset_hold_points(b.dim()).is_some());
}

#[test]
fn deleting_the_first_roller_with_the_dimension_frees_the_copy() {
    let b = Bench::new(polygon(4, 50.0), -5.0);
    let state = b.source_state(&b.s);
    let mut s = b.s.clone();
    s.remove_curve(b.ids.rollers[0]).unwrap();
    check_aftermath(&b, &mut s, &state);
    assert!(s.constraint(b.dim()).is_none());
}

#[test]
fn deleting_a_cross_line_is_harmless() {
    let b = Bench::new(single_line(), 5.0);
    let state = b.source_state(&b.s);
    let cross: Vec<CurveId> =
        b.s.curves()
            .filter(|(c, d)| {
                d.construction && matches!(d.kind, CurveKind::Line { .. }) && !b.sources.contains(c)
            })
            .map(|(c, _)| c)
            .collect();
    assert_eq!(cross.len(), 2);
    let mut s = b.s.clone();
    s.remove_curve(cross[0]).unwrap();
    check_aftermath(&b, &mut s, &state);
    // Die Kopie lässt sich nun am freien Ende ziehen; die Vorlage bleibt.
    assert_eq!(dof(&s), dof(&b.s) + 1);
}

#[test]
fn trimming_a_copy_line_leaves_no_dead_constraints() {
    let b = Bench::new(polygon(4, 50.0), -5.0);
    let state = b.source_state(&b.s);
    let mut s = b.s.clone();
    let k = b.ids.curves[0];
    let (a, c) = match s.curve(k).unwrap().kind {
        CurveKind::Line { a, b } => (at(&s, a), at(&s, b)),
        _ => unreachable!(),
    };
    s.trim(k, (a + c) * 0.5).unwrap();
    check_aftermath(&b, &mut s, &state);
}

#[test]
fn deleting_the_copy_leaves_rollers_without_dead_references() {
    for (shape, d) in [(polygon(4, 50.0), -5.0), (rounded_rect(), -3.0)] {
        let b = Bench::new(shape, d);
        let state = b.source_state(&b.s);
        let mut s = b.s.clone();
        for &k in &b.ids.curves {
            s.remove_curve(k).unwrap();
        }
        check_aftermath(&b, &mut s, &state);
        // Nichts mehr berührt den Rollkreis: Das Maß ist kein Versatzmaß mehr.
        assert_eq!(s.offset_hold_points(b.dim()), None);
    }
}

// ---------------------------------------------------------------------------------------------
// Feste Geometrie (Review-Focus 4)
// ---------------------------------------------------------------------------------------------

#[test]
fn the_offset_of_a_projected_line_is_normal_and_follows() {
    let k = AnalyticKernel::default();
    let body = k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let mut s = sketch();
    let mut copied = None;
    for (id, _) in body.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&body, id)
            && (s.line_length(c).unwrap_or(0.0) - 10.0).abs() < 1e-9
        {
            copied = Some((c, id));
            break;
        }
    }
    let (q, edge) = copied.expect("eine Kante der Länge 10");
    let b = Bench::new((s, vec![q]), 4.0);
    assert_still(&b.s);
    let copy = b.ids.curves[0];
    assert!(!b.s.curve(copy).unwrap().construction, "Kopie normal");
    assert!(b.s.offset_hold_points(b.dim()).is_some());

    let mut s = b.s.clone();
    let moved = k.make_box(DVec3::new(12.0, 20.0, 5.0)).unwrap();
    s.reproject_edge(q, &moved, edge).unwrap();
    assert_clean(&s.solve());
    assert!((s.line_length(copy).unwrap() - 12.0).abs() < 1e-7);
    b.check_distance(&s, 4.0, 1e-7, "Kante");
}

#[test]
fn the_offset_of_a_projected_circle_follows_the_radius() {
    let k = AnalyticKernel::default();
    let body = k.make_cylinder(3.0, 8.0).unwrap();
    let mut s = sketch();
    let mut copied = None;
    for (id, _) in body.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&body, id)
            && matches!(s.curve(c).unwrap().kind, CurveKind::Circle { .. })
        {
            copied = Some((c, id));
            break;
        }
    }
    let (q, edge) = copied.expect("ein Kreis");
    let mut b = Bench::new((s, vec![q]), -2.0);
    assert_still(&b.s);
    let copy = b.ids.curves[0];
    assert!(!b.s.curve(copy).unwrap().construction, "Kopie normal");
    b.sweep_held(values(0.5, 10.0, 40), "Kreis fest");

    let mut s = b.s.clone();
    let dim = b.dim();
    s.set_constraint_value(dim, 2.0).unwrap();
    assert!(s.solve_holding(&b.hold()));
    let moved = k.make_cylinder(5.0, 8.0).unwrap();
    s.reproject_edge(q, &moved, edge).unwrap();
    assert_clean(&s.solve());
    assert!((s.radius(copy).unwrap() - 7.0).abs() < 1e-7);
}

// ---------------------------------------------------------------------------------------------
// Profile, Näherung, Rundlauf
// ---------------------------------------------------------------------------------------------

#[test]
fn a_square_with_an_outer_offset_has_two_faces() {
    let b = Bench::new(polygon(4, 50.0), -5.0);
    let e = b.s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 2, "Ring und Innenfläche");
    assert!(e.profile_at(DVec2::ZERO).is_some());
    for l in &e.curve_loops {
        for c in l {
            assert!(!b.ids.rollers.contains(c), "Rollkreise teilen nichts");
            assert!(!b.s.curve(*c).unwrap().construction);
        }
    }
    // Hinter der Kopie ist nichts mehr.
    assert!(e.profile_at(DVec2::new(100.0, 100.0)).is_none());
}

fn ellipse() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let m = s.add_point(0., 0.).unwrap();
    let e = s.add_ellipse(m, DVec2::new(40.0, 0.0), 20.0).unwrap();
    (s, vec![e])
}

#[test]
fn approximate_pieces_carry_nothing_and_close_a_profile() {
    let (mut s, cs) = ellipse();
    let plan = plan_of(&s, &cs, -5.0);
    assert!(plan.loops.iter().all(|l| {
        l.pieces
            .iter()
            .all(|p| matches!(p.geom, OffsetGeom::Approx { .. }))
    }));
    let vars = s.variables().to_vec();
    let ids = s.add_offset(&plan).unwrap();
    assert!(!ids.tied, "Nichts bindet die Kopie an die Vorlage");
    assert_eq!(ids.distance_dim, None);
    assert!(ids.rollers.is_empty());
    assert_eq!(s.constraints().count(), 0);
    assert_eq!(&s.variables()[..vars.len()], vars.as_slice());
    assert_eq!(ids.curves.len(), 2);
    assert_eq!(s.clone().solve().status, SolveStatus::Solved);

    // Ohne die Vorlage bleibt genau die Fläche der Kopie.
    s.remove_curve(cs[0]).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
    assert!(e.profile_at(DVec2::ZERO).is_some());
}

#[test]
fn a_line_next_to_an_approximation_carries_its_own_constraints() {
    let mut s = sketch();
    let (a, b) = (s.add_point(0., 0.).unwrap(), s.add_point(40., 0.).unwrap());
    let (c, d) = (
        s.add_point(60., 10.).unwrap(),
        s.add_point(80., 30.).unwrap(),
    );
    let l = s.add_line(a, b).unwrap();
    let sp = s.add_spline(&[b, c, d]).unwrap();
    let plan = plan_of(&s, &[l, sp], 3.0);
    let vars = s.variables().to_vec();
    let ids = s.add_offset(&plan).unwrap();
    assert_eq!(ids.rollers.len(), 1);
    assert_eq!(count(&s, is_parallel), 1);
    assert!(ids.tied);
    let mut solved = s.clone();
    let r = solved.solve();
    assert_clean(&r);
    assert_eq!(&solved.variables()[..vars.len()], vars.as_slice());
}

#[test]
fn offsets_survive_the_round_trip() {
    for (shape, d) in [
        (polygon(4, 50.0), -5.0),
        (rounded_rect(), -3.0),
        (single_circle(), 5.0),
        (open_chain(), 3.0),
        (ellipse(), -5.0),
    ] {
        let (mut s, cs) = shape;
        let plan = plan_of(&s, &cs, d);
        let ids = s.add_offset(&plan).unwrap();
        let data = s.to_data();
        let back = Sketch::from_data(data.clone()).unwrap();
        assert_eq!(back.to_data(), data);
        assert!(!s.has_centerline_entry(), "kein neues Format");
        if let Some(dim) = ids.distance_dim {
            assert_eq!(back.offset_hold_points(dim), s.offset_hold_points(dim));
        }
        let mut t = back.clone();
        let r = t.solve();
        assert_clean(&r);
        assert_eq!(r.iterations, 0);
    }
}

// ---------------------------------------------------------------------------------------------
// Stücke und Vorlagen sind nicht eins zu eins
// ---------------------------------------------------------------------------------------------

fn keyhole() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let x = 30.0 - (225.0f64 - 16.0).sqrt();
    let c = s.add_point(30.0, 0.0).unwrap();
    let p: Vec<_> = [
        (-60.0, -30.0),
        (-10.0, -30.0),
        (-10.0, -4.0),
        (x, -4.0),
        (x, 4.0),
        (-10.0, 4.0),
        (-10.0, 30.0),
        (-60.0, 30.0),
    ]
    .iter()
    .map(|q| s.add_point(q.0, q.1).unwrap())
    .collect();
    let mut q = Vec::new();
    for k in 0..3 {
        q.push(s.add_line(p[k], p[k + 1]).unwrap());
    }
    q.push(s.add_arc(c, p[3], p[4]).unwrap());
    for k in 4..8 {
        q.push(s.add_line(p[k], p[(k + 1) % 8]).unwrap());
    }
    (s, q)
}

fn plain_polygon(pts: &[(f64, f64)]) -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let ids: Vec<_> = pts.iter().map(|p| s.add_point(p.0, p.1).unwrap()).collect();
    let n = ids.len();
    let curves = (0..n)
        .map(|i| s.add_line(ids[i], ids[(i + 1) % n]).unwrap())
        .collect();
    (s, curves)
}

#[test]
fn a_circle_piece_from_an_arc_is_tied_like_a_circle() {
    let mut b = check_table(keyhole(), 8.0, "Schlüsselloch");
    let circle = b
        .pairs
        .iter()
        .find(|&&(_, k)| matches!(b.s.curve(k).unwrap().kind, CurveKind::Circle { .. }))
        .expect("Kreis aus dem Bogen");
    assert!(matches!(
        b.s.curve(circle.0).unwrap().kind,
        CurveKind::Arc { .. }
    ));
    check_hold_cover(&b, "Schlüsselloch");
    b.sweep_held(values(8.0, 14.9, 69), "Schlüsselloch");
}

#[test]
fn a_dumbbell_ties_both_loops_to_one_dimension() {
    let pts = [
        (0.0, 0.0),
        (40.0, 0.0),
        (40.0, 17.0),
        (60.0, 17.0),
        (60.0, 0.0),
        (100.0, 0.0),
        (100.0, 40.0),
        (60.0, 40.0),
        (60.0, 23.0),
        (40.0, 23.0),
        (40.0, 40.0),
        (0.0, 40.0),
    ];
    let mut b = check_table(plain_polygon(&pts), 5.0, "Hantel");
    assert_eq!(count(&b.s, is_radius), 1);
    assert_eq!(
        count(&b.s, is_equal),
        b.ids.rollers.len() - 1 + 2,
        "die zwei Fugen im Steg"
    );
    check_hold_cover(&b, "Hantel");
    b.sweep_held(values(3.5, 19.0, 62), "Hantel");
}

#[test]
fn one_source_may_carry_two_pieces() {
    // Die Zunge teilt die Fläche, und die untere Kante gibt zwei Stücke in zwei Schleifen.
    let pts = [
        (0.0, 0.0),
        (100.0, 0.0),
        (100.0, 40.0),
        (55.0, 40.0),
        (55.0, 8.0),
        (45.0, 8.0),
        (45.0, 40.0),
        (0.0, 40.0),
    ];
    let (s, cs) = plain_polygon(&pts);
    let plan = plan_of(&s, &cs, 5.0);
    assert_eq!(plan.loops.len(), 2);
    let on_bottom = plan
        .loops
        .iter()
        .flat_map(|l| &l.pieces)
        .filter(|p| p.source == cs[0])
        .count();
    assert_eq!(on_bottom, 2);
    let mut b = check_table((s, cs), 5.0, "Zunge");
    check_hold_cover(&b, "Zunge");
    b.sweep_held(values(4.5, 15.0, 42), "Zunge");
}

#[test]
fn two_half_arcs_share_both_ends() {
    let mut s = sketch();
    let m = s.add_point(0., 0.).unwrap();
    let (p, q) = (
        s.add_point(20., 0.).unwrap(),
        s.add_point(-20., 0.).unwrap(),
    );
    let cs = vec![s.add_arc(m, p, q).unwrap(), s.add_arc(m, q, p).unwrap()];
    let mut b = check_table((s, cs), -5.0, "zwei Hälften");
    assert_eq!(b.ids.rollers.len(), 1);
    b.sweep_held(values(0.5, 50.0, 100), "zwei Hälften");
}

#[test]
fn collinear_neighbours_are_one_run() {
    let pts = [(0., 0.), (40., 0.), (100., 0.), (100., 50.), (0., 50.)];
    let mut b = check_table(plain_polygon(&pts), -5.0, "auf einer Geraden");
    assert_eq!(b.ids.rollers.len(), 4, "je scharfe Fuge einer");
    assert_eq!(count(&b.s, is_parallel), 5);
    b.sweep_held(values(0.5, 50.0, 100), "auf einer Geraden");
}

#[test]
fn an_open_end_cut_by_the_trimming_stays_free() {
    let mut s = sketch();
    let p: Vec<_> = [(0., 0.), (100., 0.), (100., 3.)]
        .iter()
        .map(|q| s.add_point(q.0, q.1).unwrap())
        .collect();
    let cs = vec![
        s.add_line(p[0], p[1]).unwrap(),
        s.add_line(p[1], p[2]).unwrap(),
    ];
    let dof_before = dof(&s);
    let plan = plan_of(&s, &cs, 5.0);
    assert_eq!(plan.piece_count(), 1);
    let vars = s.variables().to_vec();
    let ids = s.add_offset(&plan).unwrap();
    assert_eq!(&s.variables()[..vars.len()], vars.as_slice());
    assert_still(&s);
    assert_eq!(dof(&s), dof_before + 1, "das gekürzte Ende ist frei");
    assert_eq!(ids.rollers.len(), 1);
}
