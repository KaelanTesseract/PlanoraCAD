use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn total(w: &[FaceWork]) -> usize {
    w.iter().map(|f| f.sections.len()).sum()
}

#[test]
fn overlapping_boxes_get_six_sections_each() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = moved(&a, DVec3::splat(5.0));
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    assert_eq!(fa.len(), 6);
    assert_eq!(total(&fa), 6);
    assert_eq!(total(&fb), 6);
}

#[test]
fn disjoint_boxes_get_no_sections() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = moved(&a, DVec3::new(30.0, 0.0, 0.0));
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    assert_eq!(total(&fa) + total(&fb), 0);
}

#[test]
fn a_cylinder_through_a_box() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = k().make_cylinder(2.0, 14.0).unwrap();
    let b = moved(&cyl, DVec3::new(5.0, 5.0, -2.0));
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    // Boden und Deckel des Quaders bekommen je einen Kreis, sonst nichts
    assert_eq!(total(&fa), 2);
    // die Zylinderwand bekommt zwei Kreise (z = 0 und z = 10), die Kappen nichts
    assert_eq!(total(&fb), 2);
    let wall = fb
        .iter()
        .find(|f| matches!(f.geom.surface, Surface::Cylinder { .. }))
        .unwrap();
    assert_eq!(wall.sections.len(), 2);
}

#[test]
fn a_box_stacked_flush_on_a_box_imprints_its_footprint() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let small = k().make_box(DVec3::new(6.0, 6.0, 10.0)).unwrap();
    let b = moved(&small, DVec3::new(2.0, 2.0, 10.0)); // steht bündig auf der Deckfläche
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    // die Deckfläche von A bekommt die 4 Randkanten des B-Bodens
    let top = fa
        .iter()
        .find(|f| matches!(&f.geom.surface, Surface::Plane { frame } if (frame.origin.z - 10.0).abs() < 1e-9 && f.geom.same_sense == (frame.z.z > 0.0)))
        .unwrap();
    assert_eq!(top.sections.len(), 4);
    assert_eq!(total(&fb), 0);
}

#[test]
fn perpendicular_cylinders_are_unsupported() {
    let a = k().make_cylinder(2.0, 10.0).unwrap();
    let rot =
        DMat4::from_translation(DVec3::new(-5.0, 0.0, 5.0)) * DMat4::from_rotation_y(PI / 2.0);
    let b = k()
        .transform(&k().make_cylinder(2.0, 10.0).unwrap(), &rot)
        .unwrap();
    match imprint(&a, &b, &tol()) {
        Err(BooleanError::UnsupportedIntersection { a, b }) => {
            assert_eq!((a, b), ("Zylinder", "Zylinder"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_order_of_faces_matches_the_solid() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = moved(&a, DVec3::splat(5.0));
    let (fa, _) = imprint(&a, &b, &tol()).unwrap();
    for (i, (id, face)) in a.faces.iter().enumerate() {
        assert_eq!(fa[i].geom.same_sense, face.same_sense, "Fläche {id:?}");
    }
}

// --- Zusätzliche Fälle: Nähte, Pole und Kegelspitze ---

fn boxed(min: DVec3, size: DVec3) -> Solid {
    moved(&k().make_box(size).unwrap(), min)
}

fn curved(w: &[FaceWork]) -> &FaceWork {
    w.iter()
        .find(|f| !matches!(f.geom.surface, Surface::Plane { .. }))
        .unwrap()
}

/// Die Fläche x = 0 schneidet die Kugel in einem Großkreis durch beide Pole: Er wird an den Polen
/// geteilt, und die Stücke teilen die Kugelfläche in zwei Hälften.
#[test]
fn a_great_circle_through_the_poles_is_split_at_the_poles() {
    let s = k().make_sphere(5.0).unwrap();
    let b = boxed(DVec3::new(0.0, -10.0, -10.0), DVec3::new(10.0, 20.0, 20.0));
    let (fs, fb) = imprint(&s, &b, &tol()).unwrap();
    let sphere = curved(&fs);
    assert_eq!(sphere.sections.len(), 2);
    for sec in &sphere.sections {
        for p in [sec.start(), sec.end()] {
            assert!((p.z.abs() - 5.0).abs() < 1e-6, "Ende nicht am Pol: {p:?}");
        }
    }
    assert_eq!(total(&fb), 2);
    let regions = split_face(&sphere.geom, &sphere.sections, &tol()).unwrap();
    assert_eq!(regions.len(), 2);
}

/// Die Fläche y = 0 enthält die Nahtmeridiane der Kugel: Das Stück auf der Naht gehört trotzdem
/// zur Kugelfläche (die Naht ist Inneres) und wird dort zum echten Schnitt.
#[test]
fn a_cut_along_the_sphere_seam_is_kept() {
    let s = k().make_sphere(5.0).unwrap();
    let b = boxed(DVec3::new(-10.0, 0.0, -10.0), DVec3::new(20.0, 10.0, 20.0));
    let (fs, _) = imprint(&s, &b, &tol()).unwrap();
    let sphere = curved(&fs);
    assert_eq!(sphere.sections.len(), 2);
    let regions = split_face(&sphere.geom, &sphere.sections, &tol()).unwrap();
    assert_eq!(regions.len(), 2);
}

/// Ein Breitenkreis kreuzt die Naht: Er wird dort geteilt und bleibt ein Stück.
#[test]
fn a_latitude_circle_is_split_at_the_seam() {
    let s = k().make_sphere(5.0).unwrap();
    let b = boxed(DVec3::new(-10.0, -10.0, 2.0), DVec3::splat(20.0));
    let (fs, fb) = imprint(&s, &b, &tol()).unwrap();
    let sphere = curved(&fs);
    assert_eq!(sphere.sections.len(), 1);
    let sec = &sphere.sections[0];
    // Anfang und Ende liegen auf der Naht (y = 0, x > 0)
    for p in [sec.start(), sec.end()] {
        assert!(p.y.abs() < 1e-6 && p.x > 0.0, "nicht auf der Naht: {p:?}");
    }
    assert_eq!(total(&fb), 1);
    assert_eq!(
        split_face(&sphere.geom, &sphere.sections, &tol())
            .unwrap()
            .len(),
        2
    );
}

/// Eine Ebene durch die Kegelachse: zwei Mantellinien von der Grundkante bis zur Spitze.
#[test]
fn a_plane_through_the_cone_axis_ends_at_the_apex() {
    let c = k().make_cone(3.0, 6.0).unwrap();
    let b = boxed(DVec3::new(0.0, -10.0, -1.0), DVec3::new(10.0, 20.0, 10.0));
    let (fc, _) = imprint(&c, &b, &tol()).unwrap();
    let cone = curved(&fc);
    assert_eq!(cone.sections.len(), 2);
    let apex = DVec3::new(0.0, 0.0, 6.0);
    for sec in &cone.sections {
        assert!(
            sec.start().distance(apex) < 1e-6 || sec.end().distance(apex) < 1e-6,
            "Mantellinie endet nicht an der Spitze"
        );
    }
    assert_eq!(
        split_face(&cone.geom, &cone.sections, &tol())
            .unwrap()
            .len(),
        2
    );
}

// --- Nachbesserung Runde 1: Nähte zusammenfallender Flächen sind keine Schnitte ---

fn walls(w: &[FaceWork]) -> Vec<&FaceWork> {
    w.iter()
        .filter(|f| !matches!(f.geom.surface, Surface::Plane { .. }))
        .collect()
}

#[test]
fn identical_cylinders_get_no_sections() {
    let a = k().make_cylinder(2.0, 10.0).unwrap();
    let (fa, fb) = imprint(&a, &a.clone(), &tol()).unwrap();
    assert_eq!(total(&fa) + total(&fb), 0);
}

#[test]
fn identical_spheres_get_no_sections() {
    let a = k().make_sphere(5.0).unwrap();
    let (fa, fb) = imprint(&a, &a.clone(), &tol()).unwrap();
    assert_eq!(total(&fa) + total(&fb), 0);
}

/// Koaxiale Zylinder gleichen Radius, B um 90° gedreht (seine Naht liegt in A): Jede Wand bekommt
/// genau den Randkreis der anderen, keinen Schlitz entlang der fremden Naht.
#[test]
fn coaxial_equal_cylinders_imprint_only_the_rim_circles() {
    let a = k().make_cylinder(2.0, 10.0).unwrap();
    let m = DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0)) * DMat4::from_rotation_z(PI / 2.0);
    let b = k().transform(&a, &m).unwrap();
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    for (w, z) in [(walls(&fa), 5.0), (walls(&fb), 10.0)] {
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].sections.len(), 1, "{:?}", w[0].sections);
        let s = &w[0].sections[0];
        assert!(matches!(s.curve, Curve::Circle { .. }));
        assert!((s.at(0.5).z - z).abs() < 1e-6);
    }
}
