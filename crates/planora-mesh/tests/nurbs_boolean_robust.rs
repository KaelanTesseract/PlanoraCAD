//! Boolesche Operationen mit Freiformflächen unter Belastung (Task 10): zufällige Lagen
//! (Brief-Test 8), dünne Gebilde unter der Knotentoleranz (Übergabe Task 9), Schnittkurven aus
//! mehreren offenen Kurven. Hilfsfunktionen in `nurbs_common`.

mod nurbs_common;

use nurbs_common::*;
use planora_kernel::boolean::{BoolOp, boolean};
use planora_kernel::{Kernel, NURBS_EDGE_TOL, Solid, validate};
use planora_math::{DMat4, DVec3, PI, TAU};
use planora_mesh::{boolean_volume_plausible, is_watertight, mass_properties, tessellate};

/// Einfacher Zufallsgenerator (reproduzierbar, keine Abhängigkeit).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Quader in zufälliger Lage und Drehung gegen den NURBS-Zylinder r = 3, h = 10 (fester Startwert
/// 0x5eed), Fälle `cases` (die Folge ist für jede Auswahl dieselbe). Jedes Ergebnis ist gültig,
/// wasserdicht, hat ein plausibles Volumen und stimmt mit dem analytischen Zwilling überein (wenn
/// der eines liefert), oder es ist ein benannter Fehler. Liefert (Ergebnisse, Meldungen).
fn run_tilts(cases: std::ops::Range<usize>) -> (usize, Vec<String>) {
    let mut rng = Rng(0x5eed);
    let cyl = nurbs_cylinder_solid(3.0, 10.0);
    let twin = k().make_cylinder(3.0, 10.0).unwrap();
    let v_cyl = PI * 9.0 * 10.0;
    let (mut ok, mut named) = (0, Vec::new());
    for case in 0..cases.end {
        let size = DVec3::new(
            2.0 + 8.0 * rng.next(),
            2.0 + 8.0 * rng.next(),
            2.0 + 8.0 * rng.next(),
        );
        let at = DVec3::new(
            -6.0 + 8.0 * rng.next(),
            -6.0 + 8.0 * rng.next(),
            -2.0 + 10.0 * rng.next(),
        );
        let rot = DMat4::from_rotation_z(TAU * rng.next())
            * DMat4::from_rotation_x(PI * rng.next())
            * DMat4::from_rotation_y(PI * rng.next());
        if !cases.contains(&case) {
            continue;
        }
        let center = at + size * 0.5;
        let place = DMat4::from_translation(center) * rot * DMat4::from_translation(-size * 0.5);
        let boxx = k().transform(&k().make_box(size).unwrap(), &place).unwrap();
        let v_box = size.x * size.y * size.z;
        let op = [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][case % 3];
        match boolean(&cyl, &boxx, op, &tol()) {
            Ok(r) => {
                let v = checked_volume(&r);
                assert!(v > 0.0, "Fall {case}: {v}");
                assert!(v <= v_cyl + v_box + 1e-6, "Fall {case}: {v}");
                assert!(
                    boolean_volume_plausible(op, v_cyl, v_box, v, 3e-3),
                    "Fall {case} {op:?}: {v}"
                );
                // Gegenprobe mit dem analytischen Zwilling (Netzabweichung beider Seiten)
                if let Ok(a) = boolean(&twin, &boxx, op, &tol()) {
                    let va = checked_volume(&a);
                    assert!(
                        (v - va).abs() <= 3e-3 * va + 1e-3,
                        "Fall {case} {op:?}: NURBS {v}, analytisch {va}"
                    );
                }
                ok += 1;
            }
            Err(e) => {
                assert!(!e.to_string().is_empty());
                named.push(format!("Fall {case} {op:?}: {e}"));
            }
        }
    }
    println!("{ok} Ergebnisse, {} Meldungen: {named:#?}", named.len());
    (ok, named)
}

#[test]
fn hostile_tilts_do_not_panic_small() {
    // Auswahl der Fälle (0–3; sechs Fälle dauerten 30 s), die im gewöhnlichen (Debug-)Testlauf mitläuft; alle 30 Fälle prüft
    // `hostile_tilts_do_not_panic` (nur mit --release --ignored).
    run_tilts(0..4);
}

#[test]
#[ignore = "30 Lagen, nur mit --release --ignored (etwa 30 s)"]
fn hostile_tilts_do_not_panic() {
    let started = std::time::Instant::now();
    let (ok, named) = run_tilts(0..30);
    assert_eq!(ok + named.len(), 30);
    assert!(started.elapsed().as_secs() < 120, "{:?}", started.elapsed());
}
// ---- Übergabe Task 9: Gebilde dünner als die Knotentoleranz ----

/// Lage eines dünnen Gebildes gegen den NURBS-Zylinder r = 3, z ∈ [−10, 10] (`placed_cylinder`).
#[derive(Clone, Copy, Debug)]
enum Thin {
    /// Quader `[3 − d, 13] × [−5, 5]²`: schneidet vom Zylinder eine Linse der Tiefe `d` ab.
    Side(f64),
    /// Quader `[−1, 10] × [−5, 5] × [0, 10 − e]`: Deckel `e` unter der oberen Mantelkante.
    Top(f64),
}

impl Thin {
    fn solid(self) -> Solid {
        match self {
            Thin::Side(d) => box_solid(
                DVec3::new(10.0 + d, 10.0, 10.0),
                DVec3::new(3.0 - d, -5.0, -5.0),
            ),
            Thin::Top(e) => box_solid(
                DVec3::new(11.0, 10.0, 10.0 - e),
                DVec3::new(-1.0, -5.0, 0.0),
            ),
        }
    }

    /// Exaktes Volumen von Zylinder ∩ Quader.
    fn common(self) -> f64 {
        let r = 3.0_f64;
        // Kreisabschnitt jenseits der Sehne im Abstand `c` von der Mitte
        let segment = |c: f64| {
            let a = (c / r).acos();
            r * r * (a - a.sin() * a.cos())
        };
        match self {
            Thin::Side(d) => segment(r - d) * 10.0,
            Thin::Top(e) => (PI * r * r - segment(1.0)) * (10.0 - e),
        }
    }

    fn box_volume(self) -> f64 {
        match self {
            Thin::Side(d) => (10.0 + d) * 100.0,
            Thin::Top(e) => 110.0 * (10.0 - e),
        }
    }

    /// Exaktes Ergebnisvolumen von `cyl op box` (`cyl_first`) bzw. `box op cyl`.
    fn expected(self, op: BoolOp, cyl_first: bool) -> f64 {
        let (vc, vb, i) = (PI * 9.0 * 20.0, self.box_volume(), self.common());
        match (op, cyl_first) {
            (BoolOp::Union, _) => vc + vb - i,
            (BoolOp::Intersection, _) => i,
            (BoolOp::Difference, true) => vc - i,
            (BoolOp::Difference, false) => vb - i,
        }
    }
}

/// Prüft `cyl op box` bzw. `box op cyl` für ein dünnes Gebilde: Ein Ergebnis ist gültig,
/// wasserdicht und hat das exakte Volumen (Linsen der Seitenlage werden fein vernetzt, sie sind
/// schmaler als die gewöhnliche Sehnentoleranz); sonst eine benannte Meldung. Liefert die Meldung.
fn check_thin(t: Thin, op: BoolOp, cyl_first: bool) -> Option<String> {
    let (cyl, boxx) = (placed_cylinder(0.0, 0.0), t.solid());
    let r = if cyl_first {
        boolean(&cyl, &boxx, op, &tol())
    } else {
        boolean(&boxx, &cyl, op, &tol())
    };
    let r = match r {
        Ok(r) => r,
        Err(e) => {
            assert!(!e.to_string().is_empty());
            return Some(e.to_string());
        }
    };
    validate(&r).unwrap_or_else(|e| panic!("{t:?} {op:?}: {e}"));
    let sliver = matches!((t, op), (Thin::Side(_), BoolOp::Intersection));
    let chord = if sliver { 1e-5 } else { 0.002 };
    let m = tessellate(&r, &opts(chord)).unwrap();
    assert!(
        is_watertight(&m, 1e-6),
        "{t:?} {op:?}: Netz nicht geschlossen"
    );
    let v = mass_properties(&m).volume;
    let expect = t.expected(op, cyl_first);
    // Die Kanten liegen nur auf `NURBS_EDGE_TOL` genau: Bei Linsen von wenigen 1e-5 Tiefe ist
    // das ein merklicher Teil des Volumens, daher zusätzlich Oberfläche · Toleranz.
    let area: f64 = m
        .indices
        .chunks(3)
        .map(|t| {
            let p = |k: usize| m.positions[t[k] as usize];
            (p(1) - p(0)).cross(p(2) - p(0)).length() / 2.0
        })
        .sum();
    let limit = 3e-3 * expect + area * NURBS_EDGE_TOL;
    assert!(
        (v - expect).abs() <= limit,
        "{t:?} {op:?} {cyl_first}: {v} statt {expect} (Grenze {limit})"
    );
    None
}

#[test]
fn thin_features_give_valid_results_or_named_errors_small() {
    // Auswahl für den gewöhnlichen Testlauf; die ganze Reihe prüft
    // `thin_features_give_valid_results_or_named_errors` (nur --release --ignored).
    for t in [Thin::Side(3e-5), Thin::Top(1e-5)] {
        for op in [BoolOp::Union, BoolOp::Difference] {
            check_thin(t, op, true);
        }
    }
    // ab 1e-4 (fünffache Knotentoleranz) wird es gelöst
    assert_eq!(check_thin(Thin::Side(1e-4), BoolOp::Difference, true), None);
}

#[test]
fn a_cap_a_few_millionths_below_the_rim_is_valid_or_named() {
    // Nachbesserung Task 10 (I2): Bei e = 1,5e-6 … 3,9e-6 entstand ein „gültiger“ Körper, dessen
    // Netz nicht schließt (die Randbögen lagen 3e-6 neben der Quaderdeckelebene, erlaubt waren
    // 4 · tol.linear). Jetzt ist es ein gültiger, geschlossener Körper mit richtigem Volumen oder
    // eine benannte Meldung.
    for e in [2e-6, 3e-6] {
        check_thin(Thin::Top(e), BoolOp::Difference, true);
    }
}

#[test]
#[ignore = "dünne Gebilde, alle Lagen, nur mit --release --ignored (etwa 3 min)"]
fn thin_features_give_valid_results_or_named_errors() {
    let mut named = Vec::new();
    let sides = [
        1e-6, 1e-5, 1.5e-5, 2e-5, 2.5e-5, 3e-5, 5e-5, 8e-5, 1e-4, 3e-4, 1e-3,
    ];
    let tops = [
        1e-6, 1.5e-6, 2e-6, 3e-6, 4e-6, 5e-6, 7e-6, 1e-5, 1.5e-5, 2e-5, 3e-5, 5e-5, 1e-4, 1e-3,
    ];
    let cases = sides
        .iter()
        .map(|d| Thin::Side(*d))
        .chain(tops.iter().map(|e| Thin::Top(*e)));
    for t in cases {
        for op in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
            for cyl_first in [true, false] {
                if let Some(msg) = check_thin(t, op, cyl_first) {
                    // ab 1e-4 (fünffache Knotentoleranz) wird jede Lage gelöst
                    let size = match t {
                        Thin::Side(d) | Thin::Top(d) => d,
                    };
                    assert!(size < 1e-4, "{t:?} {op:?} {cyl_first}: {msg}");
                    named.push(format!("{t:?} {op:?} {cyl_first}: {msg}"));
                }
            }
        }
    }
    println!("{} Meldungen: {named:#?}", named.len());
}

#[test]
fn a_cut_made_of_several_open_curves_near_the_seam_is_sewn() {
    // Fall 6 der Lagenreihe: Ein Quader schneidet den Mantel in einer kleinen Schleife über der
    // Naht, die der Rechner als mehrere offene Kurven liefert; ihre Enden fallen nur bis auf
    // etwa 2e-6 zusammen. `split_face` hielt den Treffer zweier solcher Stücke an ihrem
    // gemeinsamen Ende für eine Kreuzung im Inneren (`AmbiguousContact`); jetzt zählt mit
    // Freiformbeteiligung nur ein Treffer weiter als der Knotenabstand von den Enden.
    let (ok, named) = run_tilts(6..7);
    assert_eq!((ok, named.len()), (1, 0), "{named:?}");
}

// ---- Nachbesserung Task 10, Runde 2 ----

/// NURBS-Kugel r = 3 und Quader `[−5, 5]² × [h, h + 5]` (Deckel nahe dem Nordpol), beide mit
/// `rot` gedreht: Ergebnis gültig, wasserdicht, Volumen exakt (Kugelkappe der Höhe `3 − h`).
fn sphere_cap_case(h: f64, rot: DMat4, op: BoolOp) {
    let sphere = k().transform(&nurbs_sphere_solid(3.0), &rot).unwrap();
    let boxx = k()
        .transform(
            &box_solid(DVec3::new(10.0, 10.0, 5.0), DVec3::new(-5.0, -5.0, h)),
            &rot,
        )
        .unwrap();
    let c = 3.0 - h;
    let cap = PI * c * c * (9.0 - c) / 3.0;
    let (vs, vb) = (4.0 / 3.0 * PI * 27.0, 500.0);
    let expect = match op {
        BoolOp::Intersection => cap,
        BoolOp::Difference => vs - cap,
        BoolOp::Union => vs + vb - cap,
    };
    let r = boolean(&sphere, &boxx, op, &tol()).unwrap_or_else(|e| panic!("h {h} {op:?}: {e}"));
    validate(&r).unwrap();
    // Die Kappe allein ist klein und flach (Pfeilhöhe 3 − h): fein vernetzen, sonst kostet der
    // Sehnenfehler am Kappenrand allein schon fast 1 % ihres Volumens.
    let chord = if op == BoolOp::Intersection {
        c / 200.0
    } else {
        0.002
    };
    let m = tessellate(&r, &opts(chord)).unwrap();
    assert!(is_watertight(&m, 1e-6), "h {h} {op:?}: Netz offen");
    let v = mass_properties(&m).volume;
    let area: f64 = m
        .indices
        .chunks(3)
        .map(|t| {
            let p = |i: usize| m.positions[t[i] as usize];
            (p(1) - p(0)).cross(p(2) - p(0)).length() / 2.0
        })
        .sum();
    let limit = 3e-3 * expect + area * NURBS_EDGE_TOL;
    assert!(
        (v - expect).abs() <= limit,
        "h {h} {op:?}: {v} statt {expect}"
    );
}

/// Drehungen der Kugel-Quader-Lage: keine und zwei aus dem Startwert 0xabc8.
fn cap_rotations() -> Vec<DMat4> {
    let mut rng = Rng(0xabc8);
    let mut out = vec![DMat4::IDENTITY];
    for _ in 0..2 {
        out.push(
            DMat4::from_rotation_z(TAU * rng.next())
                * DMat4::from_rotation_x(PI * rng.next())
                * DMat4::from_rotation_y(PI * rng.next()),
        );
    }
    out
}

#[test]
fn a_box_cutting_a_nurbs_sphere_near_its_pole_gives_results() {
    // Runde 2, R1: Der Prüfpunkt des Quaderbodens liegt nahe am Pol; in Runde 1 galt von dort
    // jede Strahlrichtung als „am Pol vorbei“ (Streifschuss). Auswahl für den Debug-Lauf.
    for h in [2.95, 2.99] {
        sphere_cap_case(h, DMat4::IDENTITY, BoolOp::Intersection);
    }
    sphere_cap_case(2.97, cap_rotations()[1], BoolOp::Difference);
}

#[test]
#[ignore = "Kugel nahe dem Pol geschnitten, alle Lagen, nur mit --release --ignored (etwa 30 s)"]
fn a_box_cutting_a_nurbs_sphere_near_its_pole_gives_results_in_all_placements() {
    for h in [2.95, 2.97, 2.99, 2.995, 2.999] {
        for rot in cap_rotations() {
            for op in [BoolOp::Intersection, BoolOp::Difference, BoolOp::Union] {
                sphere_cap_case(h, rot, op);
            }
        }
    }
}
