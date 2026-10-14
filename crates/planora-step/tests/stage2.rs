//! Stufentest 2 (Teilprojekt 10, Task 9): Volumen und Fläche gekrümmter Körper aus den
//! gelesenen STEP-Flächen gegen die Formeln (Spec §5.4) und gegen das Netz des Kernels;
//! Körper mit Loch (Geschlecht 1).

mod common;

use common::check::measure;
use common::model::Model;
use common::{checked, export};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop, Segment, Solid};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};

/// Sehnentoleranz der Netze.
const CHORD: f64 = 0.002;
/// Volumenfehler eines Netzes gegen die Formel höchstens `SAG_FACTOR · CHORD · gekrümmte
/// Oberfläche` (ebene Flächen sind exakt): Jedes
/// Dreieck liegt bis zur Sehnentoleranz unter der Fläche, in der Mitte eines Dreiecks im
/// Breiten-/Längenraster bis zu zwei Sehnenhöhen (wie in Teilprojekt 9, Ruling R-1; gemessen
/// höchstens 1,1 bei der Kugel).
const SAG_FACTOR: f64 = 2.0;
/// Relativer Fehler der Oberfläche gegen die Formel (gemessen höchstens 5,6e-4 bei der Kugel).
const AREA_REL: f64 = 1e-3;
/// Schranke gegen das Kernel-Netz derselben Teilung: nur Rundung beim Auflegen der Punkte auf
/// die STEP-Flächen.
const MESH_MATCH_REL: f64 = 1e-9;
/// Größter Abstand eines Punkts der STEP-Fläche von seiner Fläche in mm: nur Rundung beim
/// Auflegen der Punkte (analytische Flächen, keine Fußpunktsuche).
const MAX_OFFSET: f64 = 1e-9;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn torus_ring() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(8.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(8.0, 3.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 6.0),
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

fn tube() -> Solid {
    k().extrude(
        &Profile {
            outer: ProfileLoop::circle(DVec2::ZERO, 5.0),
            holes: vec![ProfileLoop::circle(DVec2::ZERO, 3.0)],
        },
        &Frame::WORLD,
        8.0,
    )
    .unwrap()
}

fn slanted() -> Solid {
    let cylinder = k().make_cylinder(5.0, 40.0).unwrap();
    let block = k()
        .transform(
            &k().make_box(DVec3::splat(200.0)).unwrap(),
            &(DMat4::from_translation(DVec3::new(0.0, 0.0, 25.0))
                * DMat4::from_rotation_x(0.5)
                * DMat4::from_translation(DVec3::new(-100.0, -100.0, 0.0))),
        )
        .unwrap();
    k().boolean(&cylinder, &block, BoolOp::Difference).unwrap()
}

/// Quader 20 × 20 × 10 mit senkrechter Bohrung r = 4 durch die Mitte.
fn drilled() -> Solid {
    let block = k().make_box(DVec3::new(20.0, 20.0, 10.0)).unwrap();
    let pin = k()
        .transform(
            &k().make_cylinder(4.0, 30.0).unwrap(),
            &DMat4::from_translation(DVec3::new(10.0, 10.0, -10.0)),
        )
        .unwrap();
    k().boolean(&block, &pin, BoolOp::Difference).unwrap()
}

/// (Name, Körper, Volumen, Oberfläche, Geschlecht, davon gekrümmte Oberfläche) nach Formeln.
/// Ebene Flächen werden exakt vernetzt und tragen zum Volumenfehler nichts bei.
fn bodies() -> Vec<(&'static str, Solid, f64, f64, usize, f64)> {
    let (r, h) = (4.0, 9.0);
    let (rc, hc) = (6.0, 9.0);
    let rs = 6.0;
    // Ring: Zylinder r 8, h 6 plus halbe Kreisscheibe r 3 um x = 8 gedreht (Pappus).
    let disc = PI * 9.0 / 2.0;
    let disc_x = 8.0 + 4.0 * 3.0 / (3.0 * PI);
    let arc_x = 8.0 + 2.0 * 3.0 / PI;
    let ring_v = PI * 64.0 * 6.0 + 2.0 * PI * disc_x * disc;
    let ring_a = 2.0 * PI * 64.0 + 2.0 * PI * arc_x * 3.0 * PI;
    // Schräg: Schnittebene durch die Achse bei z = 25, mittlere Höhe 25.
    let slant = 0.5f64;
    let slant_a = PI * 25.0 + PI * 25.0 / slant.cos() + 2.0 * PI * 5.0 * 25.0;
    vec![
        (
            "Zylinder",
            k().make_cylinder(r, h).unwrap(),
            PI * r * r * h,
            2.0 * PI * r * (r + h),
            0,
            2.0 * PI * r * h,
        ),
        (
            "Kegel",
            k().make_cone(rc, hc).unwrap(),
            PI * rc * rc * hc / 3.0,
            PI * rc * (rc + rc.hypot(hc)),
            0,
            PI * rc * rc.hypot(hc),
        ),
        (
            "Kugel",
            k().make_sphere(rs).unwrap(),
            4.0 / 3.0 * PI * rs.powi(3),
            4.0 * PI * rs * rs,
            0,
            4.0 * PI * rs * rs,
        ),
        (
            "Ring",
            torus_ring(),
            ring_v,
            ring_a,
            0,
            ring_a - 2.0 * PI * 64.0,
        ),
        (
            "Rohr",
            tube(),
            PI * 16.0 * 8.0,
            2.0 * PI * 16.0 + 2.0 * PI * 8.0 * 8.0,
            1,
            2.0 * PI * 8.0 * 8.0,
        ),
        (
            "Schräg",
            slanted(),
            PI * 25.0 * 25.0,
            slant_a,
            0,
            2.0 * PI * 5.0 * 25.0,
        ),
        (
            "Bohrung",
            drilled(),
            4000.0 - PI * 16.0 * 10.0,
            2.0 * (400.0 - PI * 16.0) + 80.0 * 10.0 + 2.0 * PI * 4.0 * 10.0,
            1,
            2.0 * PI * 4.0 * 10.0,
        ),
    ]
}

#[test]
fn curved_bodies_keep_volume_and_area_through_the_file() {
    for (name, s, volume, area, genus, curved) in bodies() {
        let (text, _) = export(&[(name, &s)]);
        let (m, stats) = checked(&text, &[&s]);
        assert_eq!(
            stats.iter().map(|x| x.genus).sum::<usize>(),
            genus,
            "{name}"
        );
        let r = measure(&m, &[&s], CHORD).unwrap();
        assert_eq!(r.flipped, 0, "{name}");
        assert!(r.max_offset < MAX_OFFSET, "{name}: {}", r.max_offset);
        assert!(rel(r.volume, r.kernel_volume) < MESH_MATCH_REL, "{name}");
        assert!(
            (r.volume - volume).abs() <= SAG_FACTOR * CHORD * curved,
            "{name}: Volumen {} statt {volume}",
            r.volume
        );
        assert!(
            rel(r.area, area) < AREA_REL,
            "{name}: Fläche {} statt {area}",
            r.area
        );
    }
}

#[test]
fn mirrored_curved_bodies_keep_a_positive_volume() {
    for (name, s, volume, _, _, curved) in bodies() {
        let s = k()
            .transform(
                &s,
                &(DMat4::from_rotation_z(0.3) * DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0))),
            )
            .unwrap();
        let (text, _) = export(&[(name, &s)]);
        let m = Model::read(&text).unwrap();
        let r = measure(&m, &[&s], CHORD).unwrap();
        assert_eq!(r.flipped, 0, "{name}");
        assert!(r.max_offset < MAX_OFFSET, "{name}: {}", r.max_offset);
        assert!(rel(r.volume, r.kernel_volume) < MESH_MATCH_REL, "{name}");
        assert!(
            (r.volume - volume).abs() <= SAG_FACTOR * CHORD * curved,
            "{name}: {}",
            r.volume
        );
    }
}
