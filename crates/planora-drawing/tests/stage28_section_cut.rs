//! Stufe 28: Kernel-Schnitt für Schnittansichten (`section.rs`, Teil 1).

use planora_drawing::ViewKind;
use planora_drawing::section::*;
use planora_kernel::{AnalyticKernel, BoolOp, Curve, Face, Id, Kernel, Loop, Solid, validate};
use planora_math::{DMat4, DVec3, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn tol() -> Tolerance {
    Tolerance::default()
}

fn moved(s: &Solid, x: f64, y: f64, z: f64) -> Solid {
    s.transformed(&DMat4::from_translation(DVec3::new(x, y, z)))
        .unwrap()
}

fn boxs(a: f64, b: f64, c: f64) -> Solid {
    let mut s = k().make_box(DVec3::new(a, b, c)).unwrap();
    s.assign_origin(7);
    s
}

fn diff(a: &Solid, b: &Solid) -> Solid {
    let mut s = k().boolean(a, b, BoolOp::Difference).unwrap();
    s.assign_origin(7);
    s
}

fn plane(kind: ViewKind, offset: f64) -> SectionPlane {
    SectionPlane { kind, offset }
}

fn cut(s: &Solid, p: &SectionPlane) -> (Solid, bool) {
    match cut_body(s, 0, p, &tol()).unwrap() {
        CutOutcome::Cut { solid, nudged } => (solid, nudged),
        other => panic!("erwartet Cut, bekommen {other:?}"),
    }
}

/// Punkte einer Schleife; gekrümmte Kanten mit `n` Punkten, Geraden nur mit Endpunkten.
fn loop_points(s: &Solid, lp: &Loop, n: usize) -> Vec<DVec3> {
    let mut out = Vec::new();
    for he in &lp.half_edges {
        let e = &s.edges[he.edge];
        let m = if matches!(e.curve, Curve::Line { .. }) {
            2
        } else {
            n
        };
        let mut pts: Vec<DVec3> = (0..m)
            .map(|i| {
                e.curve
                    .point(e.t0 + (e.t1 - e.t0) * i as f64 / (m - 1) as f64)
            })
            .collect();
        if !he.forward {
            pts.reverse();
        }
        pts.pop();
        out.extend(pts);
    }
    out
}

fn shoelace(pts: &[DVec3], u: usize, v: usize) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            p[u] * q[v] - q[u] * p[v]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

/// Querachsen zur Schnittachse: Achse der (einzigen) Schnittebene aus den Flächen bestimmen.
fn cut_axis(s: &Solid, faces: &[Id<Face>]) -> usize {
    let pts = loop_points(s, &s.faces[faces[0]].outer, 8);
    (0..3)
        .find(|&a| pts.iter().all(|p| (p[a] - pts[0][a]).abs() < 1e-6))
        .expect("Schnittfläche ist eben und achsparallel")
}

/// Fläche aller Schnittflächen: äußere Schleifen minus innere (512 Punkte je gekrümmter Kante
/// wären für das Rohr zu grob für 1e-3, deshalb 4096).
fn cut_area(s: &Solid) -> f64 {
    let faces = cut_faces(s);
    let a = cut_axis(s, &faces);
    let (u, v) = ((a + 1) % 3, (a + 2) % 3);
    faces
        .iter()
        .map(|&f| {
            let face = &s.faces[f];
            let outer = shoelace(&loop_points(s, &face.outer, 4096), u, v);
            let inner: f64 = face
                .inner
                .iter()
                .map(|l| shoelace(&loop_points(s, l, 4096), u, v))
                .sum();
            outer - inner
        })
        .sum()
}

fn all_cut_points(s: &Solid) -> Vec<DVec3> {
    let mut out = Vec::new();
    for f in cut_faces(s) {
        let face = &s.faces[f];
        for l in std::iter::once(&face.outer).chain(face.inner.iter()) {
            out.extend(loop_points(s, l, 256));
        }
    }
    out
}

fn bounds(s: &Solid) -> (DVec3, DVec3) {
    solid_bounds(s).unwrap()
}

fn near_plane(s: &Solid, a: usize, off: f64) -> bool {
    s.vertices
        .iter()
        .any(|(_, v)| (v.point[a] - off).abs() < 1e-4)
}

fn tube() -> Solid {
    let outer = k().make_cylinder(10.0, 20.0).unwrap();
    let inner = moved(&k().make_cylinder(6.0, 24.0).unwrap(), 0.0, 0.0, -2.0);
    diff(&outer, &inner)
}

#[test]
fn box_cut_in_half() {
    let s = boxs(20.0, 10.0, 5.0);
    let (out, nudged) = cut(&s, &plane(ViewKind::Front, 4.0));
    assert!(!nudged);
    let faces = cut_faces(&out);
    assert_eq!(faces.len(), 1);
    assert!((cut_area(&out) - 100.0).abs() < 1e-9);
    for p in all_cut_points(&out) {
        assert!((p.y - 4.0).abs() < 1e-9, "{p:?}");
    }
    let (lo, hi) = bounds(&out);
    assert!((lo.y - 4.0).abs() < 1e-9 && (hi.y - 10.0).abs() < 1e-9);
    validate(&out).unwrap();
    for (id, f) in out.faces.iter() {
        if !faces.contains(&id) {
            assert_eq!(f.tag.as_ref().unwrap().origin, 7);
        }
    }
}

#[test]
fn cylinder_through_axis() {
    let mut s = k().make_cylinder(5.0, 10.0).unwrap();
    s.assign_origin(7);
    let (out, nudged) = cut(&s, &plane(ViewKind::Front, 0.0));
    assert_eq!(nudged, near_plane(&s, 1, 0.0));
    assert_eq!(cut_faces(&out).len(), 1);
    assert!((cut_area(&out) - 100.0).abs() < 1e-5, "{}", cut_area(&out));
}

#[test]
fn box_with_bore() {
    let b = boxs(30.0, 20.0, 10.0);
    let drill = moved(&k().make_cylinder(4.0, 14.0).unwrap(), 15.0, 10.0, -2.0);
    let s = diff(&b, &drill);
    let (out, _) = cut(&s, &plane(ViewKind::Front, 10.0));
    assert_eq!(cut_faces(&out).len(), 2);
    assert!((cut_area(&out) - 220.0).abs() < 1e-3, "{}", cut_area(&out));
    for p in all_cut_points(&out) {
        assert!((p.x - 15.0).abs() >= 4.0 - 1e-6, "{p:?}");
    }
}

#[test]
fn tube_across() {
    let (out, _) = cut(&tube(), &plane(ViewKind::Top, 10.0));
    let faces = cut_faces(&out);
    assert_eq!(faces.len(), 1);
    assert_eq!(out.faces[faces[0]].inner.len(), 1);
    let want = 64.0 * std::f64::consts::PI;
    assert!((cut_area(&out) - want).abs() < 1e-3, "{}", cut_area(&out));
    let (_, hi) = bounds(&out);
    assert!(hi.z <= 10.0 + 1e-9);
}

#[test]
fn tube_along() {
    let (out, _) = cut(&tube(), &plane(ViewKind::Front, 0.0));
    let faces = cut_faces(&out);
    assert_eq!(faces.len(), 2);
    let all = cut_area(&out);
    assert!((all - 160.0).abs() < 2e-3, "{all}");
    for f in faces {
        let pts = loop_points(&out, &out.faces[f].outer, 16);
        let a = shoelace(&pts, 0, 2);
        assert!((a - 80.0).abs() < 1e-3, "{a}");
    }
}

#[test]
fn groove_body() {
    let plate = boxs(40.0, 20.0, 10.0);
    let groove = moved(
        &k().make_box(DVec3::new(8.0, 22.0, 7.0)).unwrap(),
        16.0,
        -1.0,
        5.0,
    );
    let s = diff(&plate, &groove);
    let (out, _) = cut(&s, &plane(ViewKind::Front, 10.0));
    let faces = cut_faces(&out);
    assert_eq!(faces.len(), 1);
    assert_eq!(out.faces[faces[0]].outer.half_edges.len(), 8);
    assert!((cut_area(&out) - 360.0).abs() < 1e-9);
}

#[test]
fn whole_side_without_kernel() {
    let s = boxs(10.0, 10.0, 10.0);
    let o = |kind, off| cut_body(&s, 0, &plane(kind, off), &tol()).unwrap();
    assert!(matches!(o(ViewKind::Front, -1.0), CutOutcome::Kept));
    assert!(matches!(o(ViewKind::Front, 0.0), CutOutcome::Kept));
    assert!(matches!(o(ViewKind::Front, 10.0), CutOutcome::Removed));
    assert!(matches!(o(ViewKind::Front, 11.0), CutOutcome::Removed));
    assert!(matches!(o(ViewKind::Back, 11.0), CutOutcome::Kept));
}

#[test]
fn plane_on_a_step_face() {
    let lower = k().make_box(DVec3::new(20.0, 10.0, 5.0)).unwrap();
    let upper = moved(
        &k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap(),
        0.0,
        0.0,
        5.0,
    );
    let mut s = k().boolean(&lower, &upper, BoolOp::Union).unwrap();
    s.assign_origin(7);
    let (out, nudged) = cut(&s, &plane(ViewKind::Top, 5.0));
    assert!(nudged);
    assert!((cut_area(&out) - 200.0).abs() < 1e-9, "{}", cut_area(&out));
}

#[test]
fn reserved_tag_is_refused() {
    let mut s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    s.assign_origin(u64::MAX - 3);
    let e = cut_body(&s, 0, &plane(ViewKind::Front, 5.0), &tol()).unwrap_err();
    assert!(!e.is_empty());
    let mut s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    s.assign_origin(SECTION_ORIGIN_MIN - 1);
    assert!(cut_body(&s, 0, &plane(ViewKind::Front, 5.0), &tol()).is_ok());
}

#[test]
fn bad_input() {
    let p = plane(ViewKind::Front, 1.0);
    assert!(cut_body(&Solid::new(), 0, &p, &tol()).is_err());
    let s = boxs(10.0, 10.0, 10.0);
    assert!(cut_body(&s, 0, &plane(ViewKind::Isometric, 1.0), &tol()).is_err());
    assert!(cut_body(&s, 0, &plane(ViewKind::Front, f64::NAN), &tol()).is_err());
}

#[test]
fn two_bodies_keep_apart() {
    let s = boxs(10.0, 10.0, 10.0);
    let p = plane(ViewKind::Front, 5.0);
    for (body, origin) in [(0usize, u64::MAX), (3, u64::MAX - 3)] {
        let CutOutcome::Cut { solid, .. } = cut_body(&s, body, &p, &tol()).unwrap() else {
            panic!("Cut erwartet");
        };
        let faces = cut_faces(&solid);
        assert_eq!(faces.len(), 1);
        assert_eq!(solid.faces[faces[0]].tag.as_ref().unwrap().origin, origin);
        assert_eq!(section_origin(body), origin);
    }
}

#[test]
fn nudge_only_for_corners_within_1e_4() {
    let s = boxs(20.0, 10.0, 5.0);
    // Ecke 5e-3 entfernt: kein Verschieben nötig.
    let (_, nudged) = cut(&s, &plane(ViewKind::Front, 5e-3));
    assert!(!nudged);
    // Ecke 5e-5 entfernt: wird verschoben.
    let (out, nudged) = cut(&s, &plane(ViewKind::Front, 5e-5));
    assert!(nudged);
    assert!((cut_area(&out) - 100.0).abs() < 1e-9);
}

#[test]
fn plane_tangent_to_a_bore_is_nudged() {
    // Bohrung längs x (Mittelachse y = 10, z = 5, r = 4); die Ebene z = 9 berührt sie.
    let b = boxs(30.0, 20.0, 10.0);
    let drill = k().make_cylinder(4.0, 34.0).unwrap();
    let rot = DMat4::from_translation(DVec3::new(-2.0, 10.0, 5.0))
        * DMat4::from_rotation_y(std::f64::consts::FRAC_PI_2);
    let s = diff(&b, &drill.transformed(&rot).unwrap());
    let (out, nudged) = cut(&s, &plane(ViewKind::Top, 9.0));
    assert!(nudged);
    assert_eq!(cut_faces(&out).len(), 2);
    validate(&out).unwrap();
}

// Gefahrenzone an der Naht einer Bohrung: Die Naht eines Zylinders liegt in der Ebene durch die
// Achse; Ebenen knapp daneben (bis etwa 0,065 · r) trafen den Bohrungskreis kurz vor seinem Ende
// und scheiterten früher im Kernel. Der Kernel sucht dort jetzt an beiden Enden.

/// Versätze der Ebene von der Bohrungsachse.
const NEAR_AXIS: [f64; 9] = [0.1, -0.1, 0.01, -0.01, 1e-4, 0.0, 0.07, 1.0, -1.0];

/// Schneidet an `p`, prüft Gültigkeit und die Schnittfläche gegen `want` (±1e-3).
fn cut_matches(s: &Solid, p: &SectionPlane, want: f64, what: &str) {
    let out = match cut_body(s, 0, p, &tol()) {
        Ok(CutOutcome::Cut { solid, .. }) => solid,
        other => panic!("{what}: Schnitt erwartet, bekommen {other:?}"),
    };
    validate(&out).unwrap_or_else(|e| panic!("{what}: {e}"));
    let got = cut_area(&out);
    assert!(
        (got - want).abs() < 1e-3,
        "{what}: Fläche {got}, erwartet {want}"
    );
}

/// Breite eines Bohrungsstreifens in der Ebene im Abstand `d` von der Achse.
fn chord(r: f64, d: f64) -> f64 {
    2.0 * (r * r - d * d).sqrt()
}

/// Bohrung längs z durch einen Quader, Achse bei (`x`, `y`), unten 2 mm überstehend.
fn bore_z(r: f64, x: f64, y: f64, h: f64) -> Solid {
    moved(&k().make_cylinder(r, h + 4.0).unwrap(), x, y, -2.0)
}

#[test]
fn bore_along_z_plane_beside_the_axis() {
    for r in [4.0, 10.0, 12.0] {
        let s = diff(&boxs(100.0, 60.0, 40.0), &bore_z(r, 50.0, 30.0, 40.0));
        for d in NEAR_AXIS {
            let want = 100.0 * 40.0 - chord(r, d) * 40.0;
            cut_matches(
                &s,
                &plane(ViewKind::Front, 30.0 + d),
                want,
                &format!("r {r} d {d}"),
            );
        }
    }
}

#[test]
fn bore_along_x_and_y_plane_beside_the_axis() {
    let block = boxs(100.0, 60.0, 40.0);
    // Längs x (Achse y = 30, z = 20), Ebene y = 30 + d.
    let along_x = k()
        .make_cylinder(10.0, 104.0)
        .unwrap()
        .transformed(
            &(DMat4::from_translation(DVec3::new(-2.0, 30.0, 20.0))
                * DMat4::from_rotation_y(std::f64::consts::FRAC_PI_2)),
        )
        .unwrap();
    let s = diff(&block, &along_x);
    for d in NEAR_AXIS {
        let want = 100.0 * 40.0 - chord(10.0, d) * 100.0;
        cut_matches(
            &s,
            &plane(ViewKind::Front, 30.0 + d),
            want,
            &format!("x d {d}"),
        );
    }
    // Längs y (Achse x = 50, z = 20), Ebene z = 20 + d.
    let along_y = k()
        .make_cylinder(10.0, 64.0)
        .unwrap()
        .transformed(
            &(DMat4::from_translation(DVec3::new(50.0, -2.0, 20.0))
                * DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2)),
        )
        .unwrap();
    let s = diff(&block, &along_y);
    for d in NEAR_AXIS {
        let want = 100.0 * 60.0 - chord(10.0, d) * 60.0;
        cut_matches(
            &s,
            &plane(ViewKind::Top, 20.0 + d),
            want,
            &format!("y d {d}"),
        );
    }
}

#[test]
fn large_block_bore_r40_plane_within_1mm_of_the_axis() {
    // Quader 400 × 300 × 200, Bohrung r 40 längs y (Achse x = 200, z = 100), Ebene z = 100 + d.
    let along_y = k()
        .make_cylinder(40.0, 304.0)
        .unwrap()
        .transformed(
            &(DMat4::from_translation(DVec3::new(200.0, -2.0, 100.0))
                * DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2)),
        )
        .unwrap();
    let s = diff(&boxs(400.0, 300.0, 200.0), &along_y);
    for d in [
        -1.0, -0.5, -0.1, -0.01, -1e-4, 0.0, 1e-4, 0.01, 0.1, 0.5, 1.0,
    ] {
        let want = 400.0 * 300.0 - chord(40.0, d) * 300.0;
        cut_matches(
            &s,
            &plane(ViewKind::Top, 100.0 + d),
            want,
            &format!("d {d}"),
        );
    }
}

#[test]
fn plate_with_two_bores_plane_beside_both_axes() {
    // Platte 100 × 60 × 10, Bohrungen r 6 bei x = 25 und r 10 bei x = 65 (beide y = 30).
    let plate = boxs(100.0, 60.0, 10.0);
    let s = diff(
        &diff(&plate, &bore_z(6.0, 25.0, 30.0, 10.0)),
        &bore_z(10.0, 65.0, 30.0, 10.0),
    );
    for d in NEAR_AXIS {
        let want = 100.0 * 10.0 - chord(6.0, d) * 10.0 - chord(10.0, d) * 10.0;
        cut_matches(
            &s,
            &plane(ViewKind::Front, 30.0 + d),
            want,
            &format!("d {d}"),
        );
    }
}
