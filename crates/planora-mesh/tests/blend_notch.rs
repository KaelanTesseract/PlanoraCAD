//! Aussparungen in den Kantenflächen im Bereich der Rundung (Teilprojekt 8, Review Task 11,
//! Fix-Runde 1): Ein Sackloch von unten, das die Wand anschneidet, darf nie einen „gültigen“, aber
//! selbst durchdringenden Körper ergeben. Jedes `Ok` ist dicht vernetzbar und hat das Volumen nach
//! Formel; sonst eine benannte Ablehnung. Liegt hier, weil der Kernel nicht vernetzen kann.

use planora_kernel::*;
use planora_math::{DMat4, DVec3, PI};
use planora_mesh::{
    BLEND_SLACK_FACTOR, Discretizer, is_watertight, mass_properties, tessellate_with,
};

/// Sehnentoleranz der Volumenprobe (gröber als in `blend_corner_volume.rs`: das dichte Netz der
/// angeschnittenen Wand dauert im Debug-Lauf bei 1e-4 fast 40 s).
const CHORD: f64 = 1e-3;

/// Erlaubte Abweichung der Volumenänderung (mm³) nach P2: Sehnentoleranz mal
/// `BLEND_SLACK_FACTOR` mal gekrümmter Fläche der Rundung (Viertelzylinder, Länge `len`).
fn slack(r: f64, len: f64) -> f64 {
    BLEND_SLACK_FACTOR * CHORD * (PI / 2.0 * r * len)
}
/// Schranke der Dichtheitsprüfung des Netzes.
const WATERTIGHT_TOL: f64 = 1e-9;
/// Sehnentoleranz der schnellen Dichtheitsprobe in der Zufallsreihe.
const SWEEP_CHORD: f64 = 1e-2;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Würfel 10 minus Zylinder (Mitte `c`, Radius `r`) von z = −1 bis `top`.
fn notched(c: (f64, f64), r: f64, top: f64) -> Solid {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let tool = k()
        .transform(
            &k().make_cylinder(r, top + 1.0).unwrap(),
            &DMat4::from_translation(DVec3::new(c.0, c.1, -1.0)),
        )
        .unwrap();
    k().boolean(&cube, &tool, BoolOp::Difference).unwrap()
}

fn edge_at(s: &Solid, p0: DVec3, p1: DVec3) -> Id<Edge> {
    let near = |v: Id<Vertex>, p: DVec3| s.vertices[v].point.distance(p) < 1e-9;
    s.edges
        .iter()
        .find(|(_, e)| {
            (near(e.start, p0) && near(e.end, p1)) || (near(e.start, p1) && near(e.end, p0))
        })
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("keine Kante {p0} – {p1}"))
}

fn fillet(s: &Solid, edges: &[Id<Edge>], r: f64) -> Result<Blended, BlendError> {
    k().blend(
        s,
        &BlendRequest {
            edges: edges
                .iter()
                .map(|&edge| BlendEdge {
                    edge,
                    first_face: None,
                })
                .collect(),
            kind: BlendKind::Fillet { radius: r },
            tangent_chain: false,
        },
    )
}

fn front_edge(s: &Solid) -> Id<Edge> {
    edge_at(s, DVec3::new(0.0, 0.0, 10.0), DVec3::new(10.0, 0.0, 10.0))
}

fn left_edge(s: &Solid) -> Id<Edge> {
    edge_at(s, DVec3::new(0.0, 0.0, 10.0), DVec3::new(0.0, 10.0, 10.0))
}

/// Benannt abgelehnt: die Aussparung in Wand 1 liegt im Bereich der Rundung.
fn assert_named(r: Result<Blended, BlendError>, what: &str) {
    match r {
        Err(BlendError::Unsupported(m)) => {
            assert!(
                m.contains("Wand 1") && m.contains("Bereich der Rundung"),
                "{what}: {m}"
            )
        }
        Err(e) => panic!("{what}: andere Ablehnung {e:?}"),
        Ok(b) => panic!(
            "{what}: Ok mit {} Flächen, vernetzbar: {}",
            b.solid.faces.len(),
            tessellate_with(&b.solid, &Discretizer::new(&b.solid, SWEEP_CHORD)).is_ok()
        ),
    }
}

#[test]
fn a_blind_hole_notching_the_wall_below_the_edge_is_refused() {
    // Review-Repro: Das Loch schneidet die Wand 1 von unten bis z ≈ 9,53 an; die Berührkurve auf
    // Wand 1 (z = 10 − r) läuft durch die Aussparung, zwischen zwei Stichproben hindurch.
    let s = notched(
        (1.8305139638093673, 0.3021497648354126),
        0.6483770760742794,
        9.529838477300427,
    );
    for r in [2.5, 2.8, 2.8851, 3.0, 3.5] {
        assert_named(fillet(&s, &[front_edge(&s)], r), &format!("r {r}"));
    }
    // Gehrung an der Ecke (0, 0, 10) mit einem kleinen Loch nahe der Ecke.
    let s = notched(
        (0.4860760141418993, 0.22046619031904158),
        0.30337190150437787,
        9.47983333137078,
    );
    assert_named(
        fillet(&s, &[front_edge(&s), left_edge(&s)], 2.0217),
        "Gehrung",
    );
}

/// Vernetzte Volumenänderung bei gemeinsamer Teilung; das Ergebnis ist gültig und dicht.
fn delta_volume(s: &Solid, r: &Solid) -> f64 {
    validate(r).unwrap();
    let d = Discretizer::for_solids(&[s, r], CHORD);
    let (m0, m1) = (
        tessellate_with(s, &d).unwrap(),
        tessellate_with(r, &d).unwrap(),
    );
    assert!(is_watertight(&m1, WATERTIGHT_TOL), "Netz nicht dicht");
    mass_properties(&m1).volume - mass_properties(&m0).volume
}

#[test]
fn a_notch_below_the_contact_curve_keeps_the_fillet() {
    // Dieselbe Aussparung, aber nur bis z = 6: Die Berührkurve z = 7,5 (r 2,5) bleibt darüber.
    let s = notched(
        (1.8305139638093673, 0.3021497648354126),
        0.6483770760742794,
        6.0,
    );
    let r = 2.5;
    let b = fillet(&s, &[front_edge(&s)], r).unwrap();
    let want = -(1.0 - PI / 4.0) * r * r * 10.0;
    let got = delta_volume(&s, &b.solid);
    assert!((got - want).abs() <= slack(r, 10.0), "{got} statt {want}");
    // Gehrung ebenso mit flacher Aussparung (bis z = 6).
    let s = notched(
        (0.4860760141418993, 0.22046619031904158),
        0.30337190150437787,
        6.0,
    );
    let r = 2.0217;
    let b = fillet(&s, &[front_edge(&s), left_edge(&s)], r).unwrap();
    let (a, m) = ((1.0 - PI / 4.0) * r * r, (5.0 / 6.0 - PI / 4.0) * r * r * r);
    let want = -2.0 * (a * 10.0 - m);
    let got = delta_volume(&s, &b.solid);
    assert!(
        (got - want).abs() <= slack(r, 20.0),
        "Gehrung: {got} statt {want}"
    );
}

/// Zufallszahlen mit festem Startwert (wie in `planora-kernel/tests/nurbs_geometry.rs`).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

/// Zufällige Sacklöcher und Taschen (Zylinder bzw. Quader von unten, teils die Wand 1 und Wand 4
/// anschneidend) nahe der Deckelkante; gerundet wird die vordere Kante oder beide Kanten an der
/// Ecke (Gehrung). Jedes `Ok` muss gültig und dicht vernetzbar sein, jede Ablehnung eine Meldung.
fn sweep(cases: usize, seed: u64) -> [usize; 3] {
    let mut rng = Lcg(seed);
    let mut counts = [0usize; 3]; // gültig, abgelehnt, davon Aussparung
    for case in 0..cases {
        let cube = k().make_box(DVec3::splat(10.0)).unwrap();
        let top = rng.range(5.0, 9.9);
        let tool = if case % 2 == 0 {
            let r = rng.range(0.2, 1.5);
            let c = (rng.range(-0.5, 9.0), rng.range(-0.8 * r, 3.0));
            k().transform(
                &k().make_cylinder(r, top + 1.0).unwrap(),
                &DMat4::from_translation(DVec3::new(c.0, c.1, -1.0)),
            )
            .unwrap()
        } else {
            let size = DVec3::new(rng.range(0.3, 2.0), rng.range(0.3, 2.0), top + 1.0);
            let at = DVec3::new(rng.range(-0.5, 8.0), rng.range(-0.5, 3.0), -1.0);
            k().transform(&k().make_box(size).unwrap(), &DMat4::from_translation(at))
                .unwrap()
        };
        let Ok(s) = k().boolean(&cube, &tool, BoolOp::Difference) else {
            continue;
        };
        if s.shells.iter().filter(|sh| !sh.faces.is_empty()).count() > 1 {
            continue;
        }
        let r = rng.range(0.3, 4.0);
        let miter = rng.next() < 0.3;
        let (Some(front), Some(left)) = (
            find_edge(&s, DVec3::new(10.0, 0.0, 10.0)),
            find_edge(&s, DVec3::new(0.0, 10.0, 10.0)),
        ) else {
            continue;
        };
        let edges = if miter {
            vec![front, left]
        } else {
            vec![front]
        };
        let what = format!("Fall {case}: r {r}, Gehrung {miter}");
        match fillet(&s, &edges, r) {
            Ok(b) => {
                validate(&b.solid).unwrap_or_else(|e| panic!("{what}: {e}"));
                let m = tessellate_with(&b.solid, &Discretizer::new(&b.solid, SWEEP_CHORD))
                    .unwrap_or_else(|e| panic!("{what}: nicht vernetzbar: {e}"));
                assert!(
                    is_watertight(&m, WATERTIGHT_TOL),
                    "{what}: Netz nicht dicht"
                );
                counts[0] += 1;
            }
            Err(e) => {
                assert!(!e.to_string().is_empty(), "{what}");
                counts[1] += 1;
                if e.to_string().contains("Aussparung") {
                    counts[2] += 1;
                }
            }
        }
    }
    counts
}

/// Kante von (0, 0, 10) nach `p`, falls ungeteilt vorhanden.
fn find_edge(s: &Solid, p: DVec3) -> Option<Id<Edge>> {
    let near = |v: Id<Vertex>, q: DVec3| s.vertices[v].point.distance(q) < 1e-9;
    let o = DVec3::new(0.0, 0.0, 10.0);
    s.edges
        .iter()
        .find(|(_, e)| (near(e.start, o) && near(e.end, p)) || (near(e.start, p) && near(e.end, o)))
        .map(|(id, _)| id)
}

#[test]
fn random_notches_near_a_fillet_never_give_a_broken_body() {
    // Kurze Reihe für den Debug-Lauf (die Booleschen Operationen sind dort langsam).
    let counts = sweep(8, 0x0A07_C401);
    assert!(counts[0] > 0, "{counts:?}");
}

#[test]
#[ignore = "lange Reihe; nur mit --release --ignored (etwa 15 s)"]
fn many_random_notches_near_a_fillet_never_give_a_broken_body() {
    let counts = sweep(1000, 0x0A07_C402);
    eprintln!("gültig, abgelehnt, davon Aussparung: {counts:?}");
    assert!(counts.iter().all(|&c| c > 0), "{counts:?}");
}
