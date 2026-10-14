//! Rundungen und Fasen, Stufe 1: geschlossene Formeln, geschlossenes Netz, Folgeoperationen
//! (Teilprojekt 8, Task 6). Netzvolumen mit Sehnentoleranz 1e-3 und gemeinsamer Teilung.

mod blend_common;

use blend_common::*;
use planora_kernel::boolean::{Location, solid_contains, solid_faces};
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, PI};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

const CHORD: f64 = 1e-3;
/// Erlaubter relativer Fehler der Volumenänderung gegen die Formel.
const REL: f64 = 5e-3;

fn box10() -> Solid {
    k().make_box(DVec3::splat(10.0)).unwrap()
}

#[test]
fn a_box_edge_fillet_removes_the_spandrel_prism() {
    let s = box10();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: 2.0 }, false).unwrap();
    let want = -(1.0 - PI / 4.0) * 4.0 * 10.0;
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Rundung");
    assert_rel(b.volume_change, want, 1e-3, "Schätzung");
    assert!(plausible(&s, &b, 0.05));
}

#[test]
fn box_chamfers_match_their_triangles() {
    let s = box10();
    let e = edge_between(&s, "Deckel", "Wand 1");
    for (kind, want) in [
        (
            BlendKind::ChamferEqual { distance: 1.5 },
            -0.5 * 1.5 * 1.5 * 10.0,
        ),
        (
            BlendKind::ChamferTwo {
                distance1: 1.0,
                distance2: 2.5,
            },
            -0.5 * 2.5 * 10.0,
        ),
        (
            BlendKind::ChamferAngle {
                distance: 1.0,
                angle: 30f64.to_radians(),
            },
            -0.5 * 30f64.to_radians().tan() * 10.0,
        ),
    ] {
        let b = blend_edges(&s, &[e], kind.clone(), false).unwrap();
        assert_rel(
            delta_volume(&s, &b.solid, CHORD),
            want,
            1e-9,
            &format!("{kind:?}"),
        );
        assert_rel(b.volume_change, want, 1e-9, "Schätzung");
    }
}

#[test]
fn cylinder_top_fillet_and_chamfer_follow_pappus() {
    let (big, h, r) = (5.0, 8.0, 1.0);
    let s = k().make_cylinder(big, h).unwrap();
    let e = edge_between(&s, "Wand 1", "Deckel");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, false).unwrap();
    let (area, dist) = spandrel(PI / 2.0, r);
    let want = -area * 2.0 * PI * (big - dist / 2f64.sqrt());
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Torus");
    assert!(plausible(&s, &b, 0.05));
    let c = blend_edges(&s, &[e], BlendKind::ChamferEqual { distance: r }, false).unwrap();
    let want = -0.5 * r * r * 2.0 * PI * (big - r / 3.0);
    assert_rel(delta_volume(&s, &c.solid, CHORD), want, REL, "Kegelfase");
}

#[test]
fn cone_base_fillet_matches_its_spandrel() {
    let (big, h, r) = (4.0, 6.0, 0.5);
    let s = k().make_cone(big, h).unwrap();
    let e = edge_between(&s, "Fläche 1", "Fläche 2");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, false).unwrap();
    let u1 = DVec2::new(-1.0, 0.0);
    let u2 = DVec2::new(-big, h).normalize();
    let theta = u1.dot(u2).acos();
    let (area, dist) = spandrel(theta, r);
    let bis = (u1 + u2).normalize();
    let want = -area * 2.0 * PI * (big + bis.x * dist);
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Kegel");
}

#[test]
fn hemisphere_base_fillet_matches_a_fine_reference() {
    let (big, r) = (5.0, 1.0);
    let s = hemisphere(big);
    let e = edge_between(&s, "Fläche 1", "Fläche 2");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, false).unwrap();
    let (area, centroid) = plane_circle_spandrel(big, r);
    let want = -area * 2.0 * PI * centroid.x;
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Halbkugel");
}

#[test]
fn extrusions_concave_sixty_degrees_and_plane_cylinder_edges() {
    // Konkave Kante des L-Profils: Material kommt hinzu.
    let l = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    );
    let e = edge_between(&l, "Wand 3", "Wand 4");
    let b = blend_edges(&l, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    assert_rel(
        delta_volume(&l, &b.solid, CHORD),
        (1.0 - PI / 4.0) * 5.0,
        REL,
        "konkav",
    );
    assert!(b.volume_change > 0.0);
    // Gleichseitiges Dreieck: Innenwinkel 60°.
    let tri = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(5.0, 75f64.sqrt()),
        ],
        4.0,
    );
    let e = edge_between(&tri, "Wand 1", "Wand 2");
    let b = blend_edges(&tri, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    let want = -(3f64.sqrt() - PI / 3.0) * 4.0;
    assert_rel(delta_volume(&tri, &b.solid, CHORD), want, REL, "60°");
    // Viertelscheibe: senkrechte Kante zwischen Ebene und Zylinder (gerade Kante, Zylinderrundung).
    let quarter = k()
        .extrude(
            &Profile {
                outer: ProfileLoop {
                    start: DVec2::ZERO,
                    segments: vec![
                        Segment::Line {
                            to: DVec2::new(6.0, 0.0),
                        },
                        Segment::Arc {
                            center: DVec2::ZERO,
                            sweep: PI / 2.0,
                        },
                        Segment::Line { to: DVec2::ZERO },
                    ],
                },
                holes: vec![],
            },
            &planora_math::Frame::WORLD,
            3.0,
        )
        .unwrap();
    let e = edge_between(&quarter, "Wand 1", "Wand 2");
    let b = blend_edges(&quarter, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    let (area, _) = plane_circle_spandrel(6.0, 1.0);
    assert_rel(
        delta_volume(&quarter, &b.solid, CHORD),
        -area * 3.0,
        REL,
        "Ebene–Zylinder",
    );
}

#[test]
fn follow_up_operations_on_filleted_bodies() {
    let s = box10();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let one = blend_edges(&s, &[e], BlendKind::Fillet { radius: 2.0 }, false)
        .unwrap()
        .solid;
    let spandrel_prism = (1.0 - PI / 4.0) * 4.0 * 10.0;
    // Boolesche Operation durch die Rundungsfläche hindurch.
    let cutter = k()
        .transform(
            &k().make_box(DVec3::new(4.0, 3.0, 12.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(3.0, -1.0, -1.0)),
        )
        .unwrap();
    let cut = k().boolean(&one, &cutter, BoolOp::Difference).unwrap();
    validate(&cut).unwrap();
    let v = mass_properties(
        &tessellate(
            &cut,
            &TessellationOptions {
                chord_tolerance: CHORD,
            },
        )
        .unwrap(),
    )
    .volume;
    // Weg fällt der Quaderteil y ∈ [0, 2] × x ∈ [3, 7] × z ∈ [0, 10] ohne den schon fehlenden
    // Zwickel auf diesen 4 mm.
    let want = 1000.0 - spandrel_prism - (4.0 * 2.0 * 10.0 - (1.0 - PI / 4.0) * 4.0 * 4.0);
    assert_rel(v, want, 1e-4, "Abziehen durch die Rundung");
    // Zweite Rundung an einer anderen Kante: die Änderungen addieren sich.
    let e2 = edge_between(&one, "Boden", "Wand 3");
    let two = blend_edges(&one, &[e2], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    assert_rel(
        delta_volume(&one, &two.solid, CHORD),
        -(1.0 - PI / 4.0) * 10.0,
        REL,
        "zweite Rundung",
    );
    // Rundung an der Kante zwischen Rundungsfläche und Deckel: tangential, benannt.
    let tangent = edge_between(&one, "Rundung 1", "Deckel");
    assert!(matches!(
        blend_edges(&one, &[tangent], BlendKind::Fillet { radius: 0.5 }, false),
        Err(BlendError::NearlyTangent { .. })
    ));
}

#[test]
fn robust_points_lie_on_the_right_side() {
    let s = box10();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: 2.0 }, false).unwrap();
    let faces = solid_faces(&b.solid);
    let at = |p: DVec3| solid_contains(&faces, p, &tol()).unwrap();
    // Im abgetragenen Zwickel (0,3 von der alten Kante längs der Winkelhalbierenden).
    assert_eq!(at(DVec3::new(5.0, 0.2, 9.8)), Location::Outside);
    // Kugelmittelpunkt und Mitte des Körpers: innen.
    assert_eq!(at(DVec3::new(5.0, 2.0, 8.0)), Location::Inside);
    assert_eq!(at(DVec3::new(5.0, 5.0, 5.0)), Location::Inside);
    // Konkav: Punkt im neuen Material.
    let l = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    );
    let e = edge_between(&l, "Wand 3", "Wand 4");
    let b = blend_edges(&l, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    let faces = solid_faces(&b.solid);
    assert_eq!(
        solid_contains(&faces, DVec3::new(4.1, 4.1, 2.5), &tol()).unwrap(),
        Location::Inside
    );
    assert_eq!(
        solid_contains(&faces, DVec3::new(4.9, 4.9, 2.5), &tol()).unwrap(),
        Location::Outside
    );
}
