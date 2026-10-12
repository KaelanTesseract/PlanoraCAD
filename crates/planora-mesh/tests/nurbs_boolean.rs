//! Boolesche Pipeline mit einer Freiformseite (Task 9): Schnitte sammeln, Flächen teilen,
//! Einordnen gegen einen analytischen Körper. Liegt im Netz-Crate wegen der Volumenprüfung.
//! Volle Boolesche Operationen (Task 10): `nurbs_boolean_full.rs`; Belastung und dünne Gebilde:
//! `nurbs_boolean_robust.rs`.

mod nurbs_common;

use nurbs_common::*;
use planora_kernel::boolean::{
    BoolOp, BooleanError, Class, FaceWork, boolean, classify_region, imprint, solid_faces,
    split_face,
};
use planora_kernel::{Curve, Kernel, Solid, Surface, validate};
use planora_math::{DMat4, DVec2, DVec3, PI, Tolerance};

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
