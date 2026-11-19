//! Slice (Skizzen 12): Schnittebene an der Skizzenebene, Seite nach der Kamera.

use glam::DVec3;
use planora_render::{SLICE_EPS, slice_cuts, slice_plane};

#[test]
fn slice_side_follows_camera() {
    // Ebene z = 2 mit Normale +z; der Punkt liegt über der Ebene.
    let origin = DVec3::new(0.0, 0.0, 2.0);
    let p = DVec3::new(3.0, -1.0, 4.0);
    let above = slice_plane(origin, DVec3::Z, DVec3::new(0.0, 0.0, 50.0));
    assert!(
        slice_cuts(above, p),
        "Kamera oben: der Teil über der Ebene fällt weg"
    );
    assert!(
        !slice_cuts(above, DVec3::new(3.0, -1.0, 0.0)),
        "der Teil hinter der Ebene bleibt"
    );
    // Kamera unter der Ebene: dieselbe Normale, die andere Seite wird geschnitten.
    let below = slice_plane(origin, DVec3::Z, DVec3::new(0.0, 0.0, -50.0));
    assert!(!slice_cuts(below, p), "Kamera unten: der obere Teil bleibt");
    assert!(slice_cuts(below, DVec3::new(3.0, -1.0, 0.0)));
    // Die Richtung der übergebenen Normalen spielt keine Rolle, nur die Kamera.
    assert_eq!(
        slice_plane(origin, -DVec3::Z, DVec3::new(0.0, 0.0, 50.0)),
        above
    );
    // Kameraseite positiv: n·eye + d > 0.
    let eye = DVec3::new(0.0, 0.0, 50.0);
    let [nx, ny, nz, d] = above;
    assert!(
        f64::from(nx) * eye.x + f64::from(ny) * eye.y + f64::from(nz) * eye.z + f64::from(d) > 0.0
    );
}

#[test]
fn slice_keeps_faces_in_the_plane() {
    // Schräge Ebene weit vom Ursprung: Punkte in der Ebene (bis knapp unter SLICE_EPS davor)
    // bleiben, damit die Fläche unter einer Skizze auf einer Körperfläche nicht verschwindet.
    let n = DVec3::new(1.0, 2.0, 2.0).normalize();
    let origin = DVec3::new(400.0, -250.0, 120.0);
    let eye = origin + n * 300.0;
    let plane = slice_plane(origin, n, eye);
    let along = DVec3::new(2.0, -1.0, 0.0).normalize();
    for k in [-300.0, 0.0, 300.0] {
        let q = origin + along * k;
        assert!(!slice_cuts(plane, q), "in der Ebene: bleibt ({k})");
        assert!(
            !slice_cuts(plane, q + n * f64::from(SLICE_EPS) * 0.5),
            "knapp davor: bleibt ({k})"
        );
        assert!(
            slice_cuts(plane, q + n * f64::from(SLICE_EPS) * 20.0),
            "deutlich davor: geschnitten ({k})"
        );
    }
    // Festgelegter Wert (siehe offene-punkte.md, Task 6).
    assert_eq!(SLICE_EPS, 1e-3, "Toleranz in mm");
}
