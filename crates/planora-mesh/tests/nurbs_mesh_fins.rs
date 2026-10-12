//! Netze Boolescher Ergebnisse mit Freiformflächen an kurzen Bogenstücken einer ebenen
//! Schnittkante („Flosse“): Freiformmantel und Quaderebene deckten dasselbe Stück zwischen Bogen
//! und Sehne ab (Dreiecke des Mantels nur aus Randpunkten, genau in der Ebene der Nachbarfläche);
//! vier Dreiecke an einer Kante, das Netz galt als offen. Hilfsfunktionen in `nurbs_common`.

mod nurbs_common;

use nurbs_common::*;
use planora_kernel::boolean::{BoolOp, boolean};
use planora_kernel::{Kernel, Solid, validate};
use planora_math::{DMat4, DVec3, PI, TAU};
use planora_mesh::{check_face_boundaries, is_watertight, mass_properties, tessellate};

/// Einfacher Zufallsgenerator (reproduzierbar, keine Abhängigkeit; wie in
/// `nurbs_boolean_robust.rs`).
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

/// Startwert der Lagenreihe des Belastungstests (vor der Behebung zwei offene Netze unter den
/// ersten 450 Fällen).
const SEED: u64 = 0x7777;

/// Fall `case` der Lagenreihe mit Startwert `seed` (wie `run_tilts` in
/// `nurbs_boolean_robust.rs`, dort mit 0x5eed): Quader in zufälliger Größe, Lage und Drehung,
/// Operation reihum.
fn tilt_case(seed: u64, case: usize) -> (Solid, BoolOp) {
    let mut rng = Rng(seed);
    let mut out = None;
    for c in 0..=case {
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
        if c == case {
            out = Some((size, at, rot));
        }
    }
    let (size, at, rot) = out.unwrap();
    let center = at + size * 0.5;
    let place = DMat4::from_translation(center) * rot * DMat4::from_translation(-size * 0.5);
    let boxx = k().transform(&k().make_box(size).unwrap(), &place).unwrap();
    let op = [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][case % 3];
    (boxx, op)
}

/// Ergebnis von NURBS-Zylinder (r = 3, h = 10) `op` Quader des Falls `case`, oder `None` bei
/// einer benannten Meldung.
fn tilt_result(seed: u64, case: usize) -> Option<Solid> {
    let (boxx, op) = tilt_case(seed, case);
    boolean(&nurbs_cylinder_solid(3.0, 10.0), &boxx, op, &tol()).ok()
}

/// Netz bei `chord`: Randbedingung je Fläche, wasserdicht; liefert das Volumen.
fn closed_volume(s: &Solid, chord: f64, what: &str) -> f64 {
    let m = tessellate(s, &opts(chord)).unwrap_or_else(|e| panic!("{what}: {e}"));
    check_face_boundaries(s, &m).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(is_watertight(&m, 1e-6), "{what}: Netz offen");
    mass_properties(&m).volume
}

/// Vor der Behebung offene Fälle (Flosse) mit ihrem damaligen Netzvolumen (Release-Build,
/// Windows): (Startwert, Fall, Sehnentoleranz, Volumen).
const FORMER_FINS: [(u64, usize, f64, f64); 4] = [
    (0x7777, 79, 0.001, 1.892_097_011_584_643_8e2),
    (0x7777, 79, 0.002, 1.891_578_623_025_653_2e2),
    (0x5eed, 151, 0.001, 2.405_997_911_499_635e2),
    (0x5eed, 151, 0.002, 2.405_257_000_191_445_8e2),
];

#[test]
fn former_fins_are_closed_and_keep_their_volume() {
    for (seed, case, chord, before) in FORMER_FINS {
        let s = tilt_result(seed, case).expect("Ergebnis");
        validate(&s).unwrap();
        let v = closed_volume(&s, chord, &format!("Fall {case}, Sehne {chord}"));
        println!(
            "Startwert {seed:#x}, Fall {case}, Sehne {chord}: relative Änderung {:e}",
            rel(v, before)
        );
        assert!(
            rel(v, before) <= 1e-6,
            "Fall {case}, Sehne {chord}: {v} statt {before}"
        );
    }
}

/// Zählt über die Fälle `cases`: Ergebnisse, bei Sehnentoleranz 0,001 bzw. 0,002 offene Netze.
fn fin_census(cases: std::ops::Range<usize>) -> (usize, Vec<String>) {
    let (mut ok, mut open) = (0, Vec::new());
    for case in cases {
        let Some(s) = tilt_result(SEED, case) else {
            continue;
        };
        ok += 1;
        for chord in [0.001, 0.002] {
            match tessellate(&s, &opts(chord)) {
                Ok(m) => {
                    let boundary = check_face_boundaries(&s, &m);
                    if !is_watertight(&m, 1e-6) || boundary.is_err() {
                        open.push(format!(
                            "Fall {case}, Sehne {chord}: Volumen {:.17e}, Rand {boundary:?}",
                            mass_properties(&m).volume
                        ));
                    }
                }
                Err(e) => open.push(format!("Fall {case}, Sehne {chord}: {e}")),
            }
        }
    }
    println!("{ok} Ergebnisse, {} offen: {open:#?}", open.len());
    (ok, open)
}

#[test]
#[ignore = "über 600 Ergebnisse, nur mit --release --ignored (etwa 13 min)"]
fn tilted_boxes_on_a_nurbs_cylinder_mesh_closed() {
    let (ok, open) = fin_census(0..700);
    assert!(ok >= 600, "nur {ok} Ergebnisse");
    assert!(open.is_empty(), "{open:#?}");
}
