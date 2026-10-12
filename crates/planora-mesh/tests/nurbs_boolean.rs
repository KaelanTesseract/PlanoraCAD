//! Boolesche Pipeline mit einer Freiformseite (Task 9): Schnitte sammeln, Flächen teilen,
//! Einordnen gegen einen analytischen Körper. Liegt im Netz-Crate wegen der Volumenprüfung.

use planora_kernel::boolean::{
    BoolOp, BooleanError, Class, FaceWork, boolean, classify_region, imprint, solid_faces,
    split_face,
};
use planora_kernel::{
    AnalyticKernel, Curve, Kernel, NURBS_EDGE_TOL, Profile, ProfileLoop, Segment, Solid, Surface,
    UvRect, validate,
};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU, Tolerance};
use planora_mesh::{
    TessellationOptions, boolean_volume_plausible, is_watertight, mass_properties, tessellate,
};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn tol() -> Tolerance {
    Tolerance::default()
}

fn opts(c: f64) -> TessellationOptions {
    TessellationOptions { chord_tolerance: c }
}

/// Zylinder um die z-Achse (Boden bei z = 0), dessen Mantel eine NURBS-Drehfläche ist
/// (wie in `nurbs_mesh.rs`, Task 5).
fn nurbs_cylinder_solid(r: f64, h: f64) -> Solid {
    nurbs_cylinder_solid_from(r, h, false)
}

/// Wie [`nurbs_cylinder_solid`]; mit `at_seam_edge` beginnt die Drehfläche (u = u0) genau an der
/// Nahtkante des extrudierten Kreises, sonst bei (r, 0).
fn nurbs_cylinder_solid_from(r: f64, h: f64, at_seam_edge: bool) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, r),
        holes: vec![],
    };
    let mut solid = k()
        .extrude(
            &profile,
            &Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
            h,
        )
        .unwrap();
    let foot = if at_seam_edge {
        let seams: Vec<DVec3> = solid
            .edges
            .iter()
            .filter(|(_, e)| matches!(e.curve, Curve::Line { .. }))
            .map(|(_, e)| e.curve.point(e.t0.min(e.t1)))
            .collect();
        assert_eq!(seams.len(), 1, "eine Nahtkante");
        DVec3::new(seams[0].x, seams[0].y, 0.0)
    } else {
        DVec3::new(r, 0.0, 0.0)
    };
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![foot, foot + DVec3::Z * h],
        vec![1.0; 2],
    )
    .unwrap();
    let nurbs = Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let ids: Vec<_> = solid
        .faces
        .iter()
        .filter(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 1);
    solid.faces[ids[0]].surface = nurbs;
    solid.faces[ids[0]].patch = Some(UvRect {
        u0: 0.0,
        u1: TAU,
        v0: 0.0,
        v1: 1.0,
    });
    solid
}

/// Analytischer Quader der Größe `size` mit der Ecke `origin`.
fn box_solid(size: DVec3, origin: DVec3) -> Solid {
    k().transform(
        &k().make_box(size).unwrap(),
        &DMat4::from_translation(origin),
    )
    .unwrap()
}

fn volume(s: &Solid) -> f64 {
    mass_properties(&tessellate(s, &opts(0.002)).unwrap()).volume
}

/// Quader 20 × 20 × 10 um den Ursprung (z ∈ [−5, 5]).
fn centered_box() -> Solid {
    box_solid(DVec3::new(20.0, 20.0, 10.0), DVec3::new(-10.0, -10.0, -5.0))
}

/// NURBS-Zylinder r = 3, h = 20 ab z = −10, um `angle` um die z-Achse gedreht und um `dx` in x
/// verschoben.
fn placed_cylinder(angle: f64, dx: f64) -> Solid {
    k().transform(
        &nurbs_cylinder_solid(3.0, 20.0),
        &(DMat4::from_translation(DVec3::new(dx, 0.0, -10.0)) * DMat4::from_rotation_z(angle)),
    )
    .unwrap()
}

fn is_nurbs(s: &Surface) -> bool {
    matches!(s, Surface::Nurbs(_))
}

/// Ebene Fläche des Quaders mit der Außennormale `±z` (Deckel: `up`).
fn cap(works: &[FaceWork], up: bool) -> &FaceWork {
    works
        .iter()
        .find(|w| match &w.geom.surface {
            Surface::Plane { frame } => {
                let outward = if w.geom.same_sense { frame.z } else { -frame.z };
                (outward.z > 0.9 && up) || (outward.z < -0.9 && !up)
            }
            _ => false,
        })
        .expect("Deckel/Boden des Quaders")
}

/// Prüft die Schnitte und die Teilung für Quader und Zylinder (Erwartungen der Tests 1 und 2).
/// Ein Fehler der Pipeline wird zurückgegeben (Test 3 lässt dort nur benannte Meldungen zu).
fn check_box_and_cylinder(
    boxx: &Solid,
    cyl: &Solid,
    axis: DVec2,
    t: &Tolerance,
) -> Result<(), BooleanError> {
    let (wa, wb) = imprint(boxx, cyl, t)?;
    for (up, z) in [(true, 5.0), (false, -5.0)] {
        let w = cap(&wa, up);
        assert_eq!(w.sections.len(), 1, "ein Schnitt in der Ebene z = {z}");
        let s = &w.sections[0];
        assert!(matches!(s.curve, Curve::Nurbs(_)), "Freiformkurve");
        assert!(s.start().distance(s.end()) < 1e-9, "geschlossen");
        for i in 0..50 {
            let p = s.at(i as f64 / 50.0);
            let r = DVec2::new(p.x - axis.x, p.y - axis.y).length();
            assert!((r - 3.0).abs() < 2e-5, "r = {r}");
            assert!((p.z - z).abs() < 2e-5, "z = {}", p.z);
        }
    }
    // die übrigen Flächen des Quaders bekommen nichts
    let total: usize = wa.iter().map(|w| w.sections.len()).sum();
    assert_eq!(total, 2);
    let mantle = wb
        .iter()
        .find(|w| is_nurbs(&w.geom.surface))
        .expect("NURBS-Mantel");
    assert_eq!(mantle.sections.len(), 2);
    let mut zs: Vec<f64> = mantle.sections.iter().map(|s| s.at(0.5).z).collect();
    zs.sort_by(|a, b| a.total_cmp(b));
    assert!(
        (zs[0] + 5.0).abs() < 2e-5 && (zs[1] - 5.0).abs() < 2e-5,
        "{zs:?}"
    );
    let total: usize = wb.iter().map(|w| w.sections.len()).sum();
    assert_eq!(total, 2, "die Deckel des Zylinders bekommen nichts");

    // Teilung: Mantel in drei Stücke, Deckel und Boden des Quaders in je zwei
    let regions = split_face(&mantle.geom, &mantle.sections, t)?;
    assert_eq!(regions.len(), 3);
    for up in [true, false] {
        let w = cap(&wa, up);
        let regions = split_face(&w.geom, &w.sections, t)?;
        assert_eq!(regions.len(), 2);
        assert_eq!(
            regions.iter().filter(|r| r.holes.len() == 1).count(),
            1,
            "außen mit Loch"
        );
        assert_eq!(
            regions.iter().filter(|r| r.holes.is_empty()).count(),
            1,
            "innen"
        );
    }
    Ok(())
}

#[test]
fn imprint_of_a_box_and_a_nurbs_cylinder_finds_two_closed_cuts() {
    let (wa, wb) = imprint(&centered_box(), &placed_cylinder(0.0, 0.0), &tol()).unwrap();
    for up in [true, false] {
        let w = cap(&wa, up);
        assert_eq!(w.sections.len(), 1);
        assert!(matches!(w.sections[0].curve, Curve::Nurbs(_)));
    }
    let mantle = wb.iter().find(|w| is_nurbs(&w.geom.surface)).unwrap();
    assert_eq!(mantle.sections.len(), 2);
    // ausführliche Prüfung (Punkte auf Kreis und Ebene, Teilung)
    check_box_and_cylinder(
        &centered_box(),
        &placed_cylinder(0.0, 0.0),
        DVec2::ZERO,
        &tol(),
    )
    .unwrap();
}

#[test]
fn split_face_cuts_the_nurbs_cylinder_into_three_regions() {
    let boxx = centered_box();
    let cyl = placed_cylinder(0.0, 0.0);
    let (wa, wb) = imprint(&boxx, &cyl, &tol()).unwrap();
    let mantle = wb.iter().find(|w| is_nurbs(&w.geom.surface)).unwrap();
    let regions = split_face(&mantle.geom, &mantle.sections, &tol()).unwrap();
    assert_eq!(regions.len(), 3);
    // Klassifizierung gegen den analytischen Quader: das mittlere Stück innen, die Enden außen
    let box_faces = solid_faces(&boxx);
    let mut classes: Vec<(f64, Class)> = regions
        .iter()
        .map(|r| {
            let z = r
                .outer
                .iter()
                .map(|s| s.at(0.5).z)
                .fold(0.0, |a: f64, b| a + b)
                / r.outer.len() as f64;
            (
                z,
                classify_region(&mantle.geom, r, &box_faces, &tol()).unwrap(),
            )
        })
        .collect();
    classes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let classes: Vec<Class> = classes.into_iter().map(|c| c.1).collect();
    assert_eq!(classes, [Class::Outside, Class::Inside, Class::Outside]);
    for up in [true, false] {
        let w = cap(&wa, up);
        let regions = split_face(&w.geom, &w.sections, &tol()).unwrap();
        assert_eq!(regions.len(), 2);
    }
}

#[test]
fn the_seam_may_lie_on_a_box_edge_without_panic() {
    // Die Naht des Mantels wandert; die Schnittkreise beginnen und enden auf ihr. Die Vorgabe
    // ließe auch eine benannte Meldung zu; alle sechs Lagen werden aber gelöst (Stand Task 9),
    // daher hier die strengere Erwartung.
    let boxx = centered_box();
    for dx in [0.0, 1e-7] {
        for deg in [0.0, 45.0, 90.0] {
            let cyl = placed_cylinder(deg * PI / 180.0, dx);
            if let Err(e) = check_box_and_cylinder(&boxx, &cyl, DVec2::new(dx, 0.0), &tol()) {
                panic!("dx = {dx}, {deg}°: {e:?}");
            }
        }
    }
}

#[test]
fn a_box_side_along_the_seam_line_is_cut_without_panic() {
    // Ein Quader mit einer Kante auf der Zylinderachse. Seine Seitenfläche y = 0 schneidet den
    // Mantel längs x = 3, y = 0: dort liegt die Parameternaht u = u0 der Drehfläche (die
    // Hilfsfunktion dreht die Linie bei (r, 0)), aber keine Kante des Körpers. Die Seitenfläche
    // x = 0 schneidet längs x = 0, y = 3: dort liegt die Nahtkante der Fläche (aus dem
    // extrudierten Kreis). Den Fall „Parameternaht = Nahtkante“ prüft
    // `a_box_side_along_the_seam_edge_matches_the_analytic_twin`.
    let corner = box_solid(DVec3::new(10.0, 10.0, 10.0), DVec3::new(0.0, 0.0, -5.0));
    let cyl = placed_cylinder(0.0, 0.0);
    let (wa, wb) = imprint(&corner, &cyl, &tol()).unwrap();
    // Deckel, Boden und die Seiten x = 0, y = 0 des Quaders: je ein Schnitt, zwei Stücke
    let cut: Vec<&FaceWork> = wa.iter().filter(|w| !w.sections.is_empty()).collect();
    assert_eq!(cut.len(), 4);
    for w in cut {
        assert_eq!(w.sections.len(), 1);
        assert_eq!(split_face(&w.geom, &w.sections, &tol()).unwrap().len(), 2);
    }
    // Mantel: Viertelbögen bei z = ±5 und zwei Strecken (eine davon auf der Naht); das
    // Viertelstück und der Rest
    let mantle = wb.iter().find(|w| is_nurbs(&w.geom.surface)).unwrap();
    assert_eq!(mantle.sections.len(), 4);
    assert_eq!(
        split_face(&mantle.geom, &mantle.sections, &tol())
            .unwrap()
            .len(),
        2
    );
}

/// Schnitt- und Stückzahlen je Fläche beider Körper (Reihenfolge der Flächen).
fn cut_counts(a: &Solid, b: &Solid) -> Vec<(usize, usize)> {
    let (wa, wb) = imprint(a, b, &tol()).unwrap();
    wa.iter()
        .chain(&wb)
        .map(|w| {
            let regions = split_face(&w.geom, &w.sections, &tol()).unwrap();
            (w.sections.len(), regions.len())
        })
        .collect()
}

#[test]
fn a_box_side_along_the_seam_edge_matches_the_analytic_twin() {
    // Häufiger Fall: Die Parameternaht u = u0 der Freiformfläche ist zugleich die Nahtkante der
    // Fläche. Eine Quaderseite schneidet genau längs dieser Naht; Schnitte und Stücke müssen
    // dieselben sein wie beim analytischen Zwilling.
    let nurbs = k()
        .transform(
            &nurbs_cylinder_solid_from(3.0, 20.0, true),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0)),
        )
        .unwrap();
    let analytic = k()
        .transform(
            &k().make_cylinder(3.0, 20.0).unwrap(),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0)),
        )
        .unwrap();
    // Nahtkante des Mantels (Gerade): ihre Lage bestimmt den Quader
    let seam = nurbs
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Line { .. }))
        .map(|(_, e)| e.curve.point(e.t0))
        .unwrap();
    // Quader [0, 10]² × [−5, 5] mit einer Kante auf der Achse: Liegt die Naht bei (3, 0) oder
    // (0, 3), enthält seine Seite y = 0 bzw. x = 0 die Nahtkante.
    assert!(
        (seam.x.abs() < 1e-9 && seam.y > 0.0) || (seam.y.abs() < 1e-9 && seam.x > 0.0),
        "Naht auf einer positiven Achse: {seam:?}"
    );
    let corner = box_solid(DVec3::new(10.0, 10.0, 10.0), DVec3::new(0.0, 0.0, -5.0));
    let n = cut_counts(&corner, &nurbs);
    let a = cut_counts(&corner, &analytic);
    assert_eq!(n, a);
    // Plausibilität: der Mantel bekommt Schnitte und wird geteilt
    assert!(n.iter().any(|&(s, r)| s >= 3 && r == 2), "{n:?}");
}

/// NURBS-Zylinder r = 3, h = 20 ab z = −10 und sein analytischer Zwilling.
fn congruent_pair() -> (Solid, Solid) {
    let analytic = k()
        .transform(
            &k().make_cylinder(3.0, 20.0).unwrap(),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0)),
        )
        .unwrap();
    (placed_cylinder(0.0, 0.0), analytic)
}

#[test]
fn congruent_bodies_need_no_ray_and_give_results() {
    // Deckungsgleiche Körper: Jedes Flächenstück liegt auf einer Fläche des anderen Körpers
    // (OnSame), das Einordnen braucht keinen Strahl gegen die Freiformfläche. Vereinigung und
    // Schnitt ergeben den Zylinder selbst, die Differenz ist leer – in beiden Reihenfolgen,
    // mit sich selbst und mit dem analytischen Zwilling.
    let (nurbs, analytic) = congruent_pair();
    let expect = PI * 9.0 * 20.0;
    for (a, b) in [(&nurbs, &nurbs), (&nurbs, &analytic), (&analytic, &nurbs)] {
        for op in [BoolOp::Union, BoolOp::Intersection] {
            let r = boolean(a, b, op, &tol()).unwrap_or_else(|e| panic!("{op:?}: {e:?}"));
            let rep = validate(&r).unwrap();
            assert_eq!((rep.faces, rep.edges, rep.vertices), (3, 3, 2), "{op:?}");
            let v = volume(&r);
            assert!(((v - expect) / expect).abs() < 0.005, "{op:?}: {v}");
        }
        assert_eq!(
            boolean(a, b, BoolOp::Difference, &tol()).map(|_| ()),
            Err(BooleanError::EmptyResult)
        );
    }
}

#[test]
fn a_nurbs_surface_with_a_far_edge_reports_a_named_error() {
    let mut cyl = placed_cylinder(0.0, 0.0);
    // Die obere Randkante des Mantels (Kreis bei z = 10) wird künstlich um 0,01 nach außen
    // verschoben: sie liegt nicht mehr auf der Freiformfläche.
    let id = cyl
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Circle { .. }) && e.curve.point(e.t0).z > 9.0)
        .map(|(id, _)| id)
        .expect("obere Kreiskante");
    if let Curve::Circle { radius, .. } = &mut cyl.edges[id].curve {
        *radius += 0.01;
    }
    match imprint(&centered_box(), &cyl, &tol()) {
        Err(BooleanError::Unsupported(msg)) => assert!(msg.contains("Freiformfläche"), "{msg}"),
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn unsupported_pairs_give_a_named_message() {
    // zwei gleich große NURBS-Zylinder, die sich rechtwinklig kreuzen: die Schnittkurven
    // verzweigen sich an zwei tangentialen Stellen
    let a = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 10.0),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0)),
        )
        .unwrap();
    let b = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 10.0),
            &(DMat4::from_rotation_y(PI / 2.0)
                * DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0))),
        )
        .unwrap();
    match boolean(&a, &b, BoolOp::Union, &tol()) {
        Err(BooleanError::Unsupported(msg)) => {
            assert!(
                msg.contains("Freiformfläche") && msg.contains("Schnitt"),
                "{msg}"
            )
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn analytic_only_results_are_unchanged() {
    // wie `box_minus_cylinder_through` in `trimmed.rs`
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = k()
        .transform(
            &k().make_cylinder(2.0, 14.0).unwrap(),
            &DMat4::from_translation(DVec3::new(5.0, 5.0, -2.0)),
        )
        .unwrap();
    let d = boolean(&a, &cyl, BoolOp::Difference, &tol()).unwrap();
    let report = validate(&d).unwrap();
    // Werte vor Task 9 (Stand 9283fa2) mit diesem Test gemessen: 7 Flächen, 15 Kanten,
    // 10 Ecken, Geschlecht 1, Netzvolumen 874.5002514717166.
    assert_eq!(
        (report.faces, report.edges, report.vertices, report.genus),
        (7, 15, 10, 1)
    );
    let v = volume(&d);
    assert!((v - 874.500_251_471_716_6).abs() < 1e-9, "{v}");
    let expect = 1000.0 - PI * 4.0 * 10.0;
    assert!(((v - expect) / expect).abs() < 0.005, "{v}");
}

// ---- Task 10: volle Boolesche Operationen mit Freiformflächen ----

/// Prüft ein Ergebnis: `validate` ok, Netz wasserdicht; liefert das Netzvolumen.
fn checked_volume(s: &Solid) -> f64 {
    validate(s).unwrap_or_else(|e| panic!("validate: {e}"));
    let m = tessellate(s, &opts(0.002)).unwrap();
    assert!(is_watertight(&m, 1e-6), "Netz nicht geschlossen");
    mass_properties(&m).volume
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn count_nurbs_faces(s: &Solid) -> usize {
    s.faces.iter().filter(|(_, f)| is_nurbs(&f.surface)).count()
}

#[test]
fn box_minus_nurbs_cylinder_through_hole() {
    let r = boolean(
        &centered_box(),
        &placed_cylinder(0.0, 0.0),
        BoolOp::Difference,
        &tol(),
    )
    .unwrap();
    let rep = validate(&r).unwrap();
    // sechs Quaderflächen (Deckel und Boden mit Loch) und der NURBS-Mantel
    assert_eq!(rep.faces, 7);
    assert_eq!(count_nurbs_faces(&r), 1);
    let holed = r.faces.iter().filter(|(_, f)| f.inner.len() == 1).count();
    assert_eq!(holed, 2);
    assert_eq!(rep.genus, 1);
    let v = checked_volume(&r);
    let expect = 4000.0 - PI * 9.0 * 10.0;
    assert!(rel(v, expect) < 3e-3, "{v} statt {expect}");
}

#[test]
fn nurbs_cylinder_minus_box_slab_removes_the_middle() {
    // Der Quader umschließt z ∈ [3, 6] des Zylinders vollständig: zwei getrennte Stücke bleiben.
    // Erwartet wird dasselbe Verhalten wie beim analytischen Zwilling.
    let slab = box_solid(DVec3::new(10.0, 10.0, 3.0), DVec3::new(-5.0, -5.0, 3.0));
    let nurbs = nurbs_cylinder_solid(3.0, 10.0);
    let analytic = k().make_cylinder(3.0, 10.0).unwrap();
    let ra = boolean(&analytic, &slab, BoolOp::Difference, &tol());
    let rn = boolean(&nurbs, &slab, BoolOp::Difference, &tol());
    let expect = PI * 9.0 * (3.0 + 4.0);
    match (ra, rn) {
        (Ok(a), Ok(n)) => {
            assert_eq!(a.shells.len(), n.shells.len(), "Schalen");
            assert_eq!(n.shells.len(), 2, "zwei getrennte Stücke");
            let (va, vn) = (checked_volume(&a), checked_volume(&n));
            assert!(rel(va, expect) < 3e-3, "analytisch {va}");
            assert!(rel(vn, expect) < 3e-3, "NURBS {vn}");
            assert_eq!(validate(&a).unwrap().faces, validate(&n).unwrap().faces);
        }
        (Err(a), Err(n)) => assert_eq!(
            std::mem::discriminant(&a),
            std::mem::discriminant(&n),
            "{a:?} / {n:?}"
        ),
        (a, n) => panic!(
            "verschieden: analytisch {:?}, NURBS {:?}",
            a.map(|_| ()),
            n.map(|_| ())
        ),
    }
}

#[test]
fn union_and_intersection_follow_inclusion_exclusion() {
    // A = Quader [0, 10]³, B = NURBS-Zylinder r = 4, Achse z durch (10, 5), z ∈ [0, 10]: Deckel
    // und Boden liegen koplanar in den Ebenen des Quaders (Ebene–Ebene, schon unterstützt).
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = k()
        .transform(
            &nurbs_cylinder_solid(4.0, 10.0),
            &DMat4::from_translation(DVec3::new(10.0, 5.0, 0.0)),
        )
        .unwrap();
    let u = boolean(&a, &b, BoolOp::Union, &tol()).unwrap();
    let i = boolean(&a, &b, BoolOp::Intersection, &tol()).unwrap();
    let (vu, vi) = (checked_volume(&u), checked_volume(&i));
    let (va, vb) = (1000.0, PI * 16.0 * 10.0);
    assert!(rel(vu + vi, va + vb) < 3e-3, "{vu} + {vi} vs {}", va + vb);
    // Halber Zylinder liegt im Quader
    assert!(rel(vi, vb / 2.0) < 3e-3, "{vi}");
    assert!(boolean_volume_plausible(BoolOp::Union, va, vb, vu, 3e-3));
    assert!(boolean_volume_plausible(
        BoolOp::Intersection,
        va,
        vb,
        vi,
        3e-3
    ));
    assert!(count_nurbs_faces(&u) >= 1 && count_nurbs_faces(&i) >= 1);
}

/// Profil der Spline-Extrusion (wie in `nurbs_extrude.rs`): unten eine Gerade, rechts hoch,
/// oben ein Spline zurück, links herunter.
fn spline_profile() -> (Profile, NurbsCurve) {
    let spline = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 6.0, 0.0),
            DVec3::new(7.0, 8.0, 0.0),
            DVec3::new(3.0, 5.0, 0.0),
            DVec3::new(0.0, 7.0, 0.0),
        ],
        &[0.0, 1.0, 2.0, 3.0],
    )
    .unwrap();
    let lp = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(10.0, 6.0),
            },
            Segment::Nurbs {
                curve: std::sync::Arc::new(spline.clone()),
            },
            Segment::Line {
                to: DVec2::new(0.0, 0.0),
            },
        ],
    };
    (
        Profile {
            outer: lp,
            holes: vec![],
        },
        spline,
    )
}

/// Fläche eines Polygons links von `x = cut` (Sutherland–Hodgman an der Halbebene `x ≤ cut`).
fn area_left_of(poly: &[DVec2], cut: f64) -> f64 {
    let mut out: Vec<DVec2> = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (ina, inb) = (a.x <= cut, b.x <= cut);
        if ina {
            out.push(a);
        }
        if ina != inb {
            let t = (cut - a.x) / (b.x - a.x);
            out.push(a + (b - a) * t);
        }
    }
    let n = out.len();
    ((0..n)
        .map(|i| out[i].perp_dot(out[(i + 1) % n]))
        .sum::<f64>()
        / 2.0)
        .abs()
}

#[test]
fn spline_extrusion_cut_by_a_box() {
    let (profile, spline) = spline_profile();
    let h = 3.0;
    // Weltachsen: Profil-x = Welt-x (`from_origin_normal` wählt die x-Achse selbst)
    let body = k().extrude(&profile, &Frame::WORLD, h).unwrap();
    assert!(
        body.vertices
            .iter()
            .any(|(_, v)| v.point.distance(DVec3::new(10.0, 6.0, 0.0)) < 1e-9),
        "Profil in Weltlage"
    );
    // Halbraum x < 5, groß genug
    let half = box_solid(
        DVec3::new(55.0, 100.0, 100.0),
        DVec3::new(-50.0, -50.0, -50.0),
    );
    let r = boolean(&body, &half, BoolOp::Intersection, &tol()).unwrap();
    let v = checked_volume(&r);
    // Profil fein abgetastet (4000 Punkte auf dem Spline)
    let (t0, t1) = spline.domain();
    let mut poly = vec![
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 6.0),
    ];
    let n = 4000;
    for i in 1..n {
        let p = spline.point(t0 + (t1 - t0) * i as f64 / n as f64);
        poly.push(DVec2::new(p.x, p.y));
    }
    poly.push(DVec2::new(0.0, 7.0));
    let expect = area_left_of(&poly, 5.0) * h;
    assert!(rel(v, expect) < 3e-3, "{v} statt {expect}");
    assert_eq!(count_nurbs_faces(&r), 1, "Spline-Wand bleibt (geschnitten)");
}

#[test]
fn ellipse_extrusion_minus_circle_extrusion() {
    let ellipse = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 3.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop::nurbs_loop(ellipse).unwrap(),
        holes: vec![],
    };
    let body = k()
        .extrude(
            &profile,
            &Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
            4.0,
        )
        .unwrap();
    // durchgehender analytischer Zylinder r = 1,5 (berührt den Ellipsenmantel nicht)
    let drill = k()
        .transform(
            &k().make_cylinder(1.5, 8.0).unwrap(),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -2.0)),
        )
        .unwrap();
    let r = boolean(&body, &drill, BoolOp::Difference, &tol()).unwrap();
    let rep = validate(&r).unwrap();
    assert_eq!(rep.genus, 1);
    assert_eq!(count_nurbs_faces(&r), 1);
    let v = checked_volume(&r);
    let expect = PI * 15.0 * 4.0 - PI * 2.25 * 4.0;
    assert!(rel(v, expect) < 3e-3, "{v} statt {expect}");
}

/// Punkt im Zylinder um die z-Achse (r) bzw. um die x-Achse.
fn in_z_cyl(p: DVec3, r: f64) -> bool {
    p.x * p.x + p.y * p.y <= r * r
}

fn in_x_cyl(p: DVec3, r: f64) -> bool {
    p.y * p.y + p.z * p.z <= r * r
}

#[test]
fn nurbs_cylinder_minus_crossing_nurbs_cylinder_of_other_radius() {
    // A: r = 2, Achse z, z ∈ [−10, 10]; B: r = 1, Achse x, x ∈ [−10, 10] (durchbohrt A).
    let a = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 20.0),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0)),
        )
        .unwrap();
    let b = k()
        .transform(
            &nurbs_cylinder_solid(1.0, 20.0),
            &(DMat4::from_rotation_y(PI / 2.0)
                * DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0))),
        )
        .unwrap();
    let r = boolean(&a, &b, BoolOp::Difference, &tol()).unwrap_or_else(|e| panic!("{e}"));
    let v = checked_volume(&r);
    // Voxelzählung 150³ über [−2, 2]³: entferntes Volumen = A ∩ B (B ragt nur dort in A)
    let n = 150;
    let h = 4.0 / n as f64;
    let mut removed = 0usize;
    for i in 0..n {
        for j in 0..n {
            for l in 0..n {
                let p = DVec3::new(
                    -2.0 + (i as f64 + 0.5) * h,
                    -2.0 + (j as f64 + 0.5) * h,
                    -2.0 + (l as f64 + 0.5) * h,
                );
                if in_z_cyl(p, 2.0) && in_x_cyl(p, 1.0) {
                    removed += 1;
                }
            }
        }
    }
    let removed = removed as f64 * h * h * h;
    let expect = PI * 4.0 * 20.0 - removed;
    assert!(rel(v, expect) < 0.01, "{v} statt {expect}");
    assert!(
        rel(PI * 4.0 * 20.0 - v, removed) < 0.03,
        "entfernt {} statt {removed}",
        PI * 4.0 * 20.0 - v
    );
}

#[test]
fn equal_radius_crossing_reports_a_named_error_and_keeps_the_inputs() {
    let a = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 10.0),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0)),
        )
        .unwrap();
    let b = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 10.0),
            &(DMat4::from_rotation_y(PI / 2.0)
                * DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0))),
        )
        .unwrap();
    let before = |s: &Solid| (s.faces.len(), s.edges.len(), volume(s));
    let (ba, bb) = (before(&a), before(&b));
    for op in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
        match boolean(&a, &b, op, &tol()) {
            Err(e) => assert!(e.to_string().contains("Freiformfläche"), "{op:?}: {e}"),
            Ok(_) => panic!("{op:?}: Ergebnis statt Meldung"),
        }
    }
    assert_eq!(before(&a), ba);
    assert_eq!(before(&b), bb);
}

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
    // Auswahl der Fälle, die im gewöhnlichen (Debug-)Testlauf mitläuft; alle 30 Fälle prüft
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
#[ignore = "dünne Gebilde, alle Lagen, nur mit --release --ignored (etwa 3 min)"]
fn thin_features_give_valid_results_or_named_errors() {
    let mut named = Vec::new();
    let sides = [
        1e-6, 1e-5, 1.5e-5, 2e-5, 2.5e-5, 3e-5, 5e-5, 8e-5, 1e-4, 3e-4, 1e-3,
    ];
    let tops = [1e-6, 1e-5, 1.5e-5, 2e-5, 3e-5, 5e-5, 1e-4, 1e-3];
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

/// Scheibe mit gerundetem Rand (Kette in der (r, z)-Halbebene um die z-Achse gedreht, wie in
/// `revolve.rs`); mit `nurbs` wird das Torusstück zu einer rationalen Drehfläche eines
/// NURBS-Viertelkreises (rational in beiden Richtungen, in u geschlossen).
fn rounded_puck(nurbs: bool) -> Solid {
    let chain = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(3.0, 0.0),
            },
            Segment::Arc {
                center: DVec2::new(3.0, 1.0),
                sweep: PI / 2.0,
            },
            Segment::Line {
                to: DVec2::new(0.0, 1.0),
            },
        ],
    };
    let mut s = k().revolve(&chain, &Frame::WORLD).unwrap();
    if !nurbs {
        return s;
    }
    let id = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Torus { .. }))
        .map(|(id, _)| id)
        .unwrap();
    // Viertelkreis von (3, 0, 0) nach (4, 0, 1) um (3, 0, 1)
    let arc = NurbsCurve::elliptic_arc(
        DVec3::new(3.0, 0.0, 1.0),
        -DVec3::Z,
        DVec3::X,
        0.0,
        PI / 2.0,
    )
    .unwrap();
    let ns = NurbsSurface::revolve(&arc, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, u1), (v0, v1)) = ns.domain();
    let surface = Surface::nurbs(ns, true, false).unwrap();
    // Außenrichtung wie beim analytischen Torusstück
    let old = &s.faces[id];
    let p = DVec3::new(3.0 + 0.5_f64.sqrt(), 0.0, 1.0 - 0.5_f64.sqrt());
    let (ou, ov) = old.surface.project(p);
    let outward = old.surface.normal(ou, ov) * if old.same_sense { 1.0 } else { -1.0 };
    let (nu, nv) = surface.project(p);
    let same_sense = surface.normal(nu, nv).dot(outward) > 0.0;
    let f = &mut s.faces[id];
    f.surface = surface;
    f.same_sense = same_sense;
    f.patch = Some(UvRect { u0, u1, v0, v1 });
    s
}

/// Außenradius der gerundeten Scheibe in der Höhe `z ∈ [0, 1]`.
fn puck_radius(z: f64) -> f64 {
    3.0 + (1.0 - (1.0 - z).powi(2)).max(0.0).sqrt()
}

/// Mittelpunktregel mit `n` Stützstellen auf `[a, b]`.
fn integrate(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (b - a) / n as f64;
    (0..n).map(|i| f(a + (i as f64 + 0.5) * h)).sum::<f64>() * h
}

#[test]
fn a_rounded_puck_with_a_nurbs_torus_patch_is_cut_by_a_box() {
    // Das analytische Torusstück kann gegen Ebenen nicht geschnitten werden
    // („Der Schnitt von Torus mit Ebene wird noch nicht unterstützt“); als Freiformfläche geht
    // es. Bezugsvolumen aus der Profilgleichung (numerische Integration).
    let (n, a) = (rounded_puck(true), rounded_puck(false));
    validate(&n).unwrap();
    let v_puck = integrate(0.0, 1.0, 4000, |z| PI * puck_radius(z).powi(2));
    let (vn, va) = (checked_volume(&n), checked_volume(&a));
    assert!(
        rel(vn, v_puck) < 3e-3 && rel(va, v_puck) < 3e-3,
        "{vn} / {va} / {v_puck}"
    );
    // Quader x ∈ [2,5; 10,5], y ∈ [−1, 1], z ∈ [−0,8; 0,6] durch den gerundeten Rand
    let boxx = box_solid(DVec3::new(8.0, 2.0, 1.4), DVec3::new(2.5, -1.0, -0.8));
    assert!(matches!(
        boolean(&a, &boxx, BoolOp::Difference, &tol()),
        Err(BooleanError::UnsupportedIntersection { .. })
    ));
    let common = integrate(0.0, 0.6, 600, |z| {
        integrate(-1.0, 1.0, 600, |y| {
            let x = (puck_radius(z).powi(2) - y * y).max(0.0).sqrt();
            (x - 2.5).max(0.0)
        })
    });
    let v_box = 8.0 * 2.0 * 1.4;
    for (op, expect) in [
        (BoolOp::Difference, v_puck - common),
        (BoolOp::Union, v_puck + v_box - common),
        (BoolOp::Intersection, common),
    ] {
        let r = boolean(&n, &boxx, op, &tol()).unwrap_or_else(|e| panic!("{op:?}: {e}"));
        let v = checked_volume(&r);
        assert!(rel(v, expect) < 3e-3, "{op:?}: {v} statt {expect}");
        assert!(count_nurbs_faces(&r) >= 1, "{op:?}");
    }
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
