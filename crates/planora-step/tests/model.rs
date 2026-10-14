//! Modell des eigenen Lesers: Formeln für Kurven und Flächen (mit bekannten Formen gegengeprüft)
//! und Aufbau aus einer geschriebenen Datei (Teilprojekt 10, Task 5).

mod common;

use common::export;
use common::model::*;
use common::reader::Value;
use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop, Segment};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU};
use planora_step::text::step_string;
use std::sync::Arc;

/// Schranke der Formeltests dieser Datei (Rundung bei Größen um 1–10).
const EPS: f64 = 1e-12;
/// Schranke für Ergebnisse der Näherungssuchen (Differenzenquotient, Intervallschachtelung).
const SEARCH_EPS: f64 = 1e-7;

fn world() -> Axis {
    Axis {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    }
}

/// Viertelkreis (Radius 1, xy-Ebene) als rationaler B-Spline vom Grad 2.
fn quarter_circle() -> Spline1 {
    Spline1 {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control: vec![DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y],
        weights: vec![1.0, std::f64::consts::FRAC_1_SQRT_2, 1.0],
        closed: false,
    }
}

#[test]
fn a_rational_quarter_circle_stays_on_the_circle() {
    let s = quarter_circle();
    assert_eq!(s.domain(), (0.0, 1.0));
    for i in 0..=20 {
        let p = s.point(f64::from(i) / 20.0);
        assert!((p.length() - 1.0).abs() < EPS, "{p}");
        assert!(p.z.abs() < EPS);
    }
    assert!((s.point(0.5) - DVec3::new(1.0, 1.0, 0.0).normalize()).length() < EPS);
    let c = Crv::Spline(s);
    let q = DVec3::new(0.6, 0.8, 0.0) * 1.5;
    let t = c.param(q);
    assert!((c.point(t) - DVec3::new(0.6, 0.8, 0.0)).length() < SEARCH_EPS);
    assert!((c.distance(q) - 0.5).abs() < SEARCH_EPS);
    let tan = c.tangent(0.0);
    assert!(tan.normalize().dot(DVec3::Y) > 1.0 - SEARCH_EPS, "{tan}");
}

#[test]
fn knots_with_inner_multiplicity_give_a_polyline() {
    // Grad 1 mit Knoten 0,0,1,2,2: Streckenzug durch drei Punkte.
    let s = Spline1 {
        degree: 1,
        knots: vec![0.0, 0.0, 1.0, 2.0, 2.0],
        control: vec![DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0)],
        weights: vec![1.0; 3],
        closed: false,
    };
    assert!((s.point(0.5) - DVec3::new(0.5, 0.0, 0.0)).length() < EPS);
    assert!((s.point(1.5) - DVec3::new(1.0, 0.5, 0.0)).length() < EPS);
    assert!((s.point(2.0) - DVec3::new(1.0, 1.0, 0.0)).length() < EPS);
    assert!(
        (s.point(5.0) - DVec3::new(1.0, 1.0, 0.0)).length() < EPS,
        "begrenzt"
    );
}

#[test]
fn a_rational_quarter_cylinder_surface_has_radius_one() {
    let row = |z: f64| {
        vec![
            DVec3::new(1.0, 0.0, z),
            DVec3::new(1.0, 1.0, z),
            DVec3::new(0.0, 1.0, z),
        ]
    };
    let w = std::f64::consts::FRAC_1_SQRT_2;
    // control[i][j]: i in u (Grad 1, Höhe), j in v (Grad 2, Bogen).
    let s = Spline2 {
        du: 1,
        dv: 2,
        ku: vec![0.0, 0.0, 1.0, 1.0],
        kv: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control: vec![row(0.0), row(3.0)],
        weights: vec![vec![1.0, w, 1.0], vec![1.0, w, 1.0]],
        u_closed: false,
        v_closed: false,
    };
    for (u, v) in [(0.0, 0.0), (0.3, 0.7), (1.0, 0.5), (0.5, 1.0)] {
        let p = s.point(u, v);
        assert!((p.x.hypot(p.y) - 1.0).abs() < EPS, "{p}");
        assert!((p.z - 3.0 * u).abs() < EPS);
    }
    let surf = Surf::Spline(s.clone());
    let q = DVec3::new(1.2, 1.2, 1.0);
    let foot = DVec3::new(FRAC, FRAC, 1.0);
    assert!((surf.project(q) - foot).length() < SEARCH_EPS);
    assert!((surf.distance(q) - (q - foot).length()).abs() < SEARCH_EPS);
    // S_u = +z, S_v läuft gegen den Uhrzeigersinn: S_u × S_v zeigt nach innen.
    let n = surf.normal_at(foot);
    assert!(
        n.dot(-foot.with_z(0.0).normalize()) > 1.0 - SEARCH_EPS,
        "{n}"
    );
}

/// cos 45°.
const FRAC: f64 = std::f64::consts::FRAC_1_SQRT_2;

#[test]
fn analytic_surfaces_measure_distance_and_natural_normals() {
    let a = Axis {
        origin: DVec3::new(1.0, 2.0, 3.0),
        ..world()
    };
    let p = a.origin + DVec3::new(3.0, 4.0, 2.0);
    let plane = Surf::Plane(a);
    assert!((plane.distance(p) - 2.0).abs() < EPS);
    assert_eq!(plane.normal_at(p), DVec3::Z);
    let cyl = Surf::Cylinder { a, r: 2.0 };
    assert!((cyl.distance(p) - 3.0).abs() < EPS);
    assert!((cyl.normal_at(p) - DVec3::new(0.6, 0.8, 0.0)).length() < EPS);
    let sph = Surf::Sphere { a, r: 1.0 };
    let len = 29f64.sqrt();
    assert!((sph.distance(p) - (len - 1.0)).abs() < EPS);
    let tor = Surf::Torus {
        a,
        major: 5.0,
        minor: 1.0,
    };
    assert!(
        (tor.distance(p) - 1.0).abs() < EPS,
        "Rohrmitte im Abstand 2"
    );
    assert!((tor.normal_at(p) - DVec3::Z).length() < EPS);
    // Kegel mit Radius 1 am Ursprung, 45°: Radius 1 + z.
    let cone = Surf::Cone {
        a: world(),
        r: 1.0,
        semi: PI / 4.0,
    };
    let on = DVec3::new(3.0, 0.0, 2.0);
    assert!(cone.distance(on) < EPS);
    assert!((cone.project(on + DVec3::new(1.0, 0.0, -1.0)) - on).length() < EPS);
    let n = cone.normal_at(on);
    assert!((n - DVec3::new(FRAC, 0.0, -FRAC)).length() < EPS, "{n}");
    for s in [&cyl, &sph, &tor, &cone] {
        let q = s.project(p);
        assert!(s.distance(q) < 1e-9, "{s:?}");
    }
}

#[test]
fn analytic_curves_have_points_tangents_and_parameters() {
    let a = Axis {
        origin: DVec3::new(0.0, 0.0, 1.0),
        ..world()
    };
    let c = Crv::Circle { a, r: 2.0 };
    assert!((c.point(PI / 2.0) - DVec3::new(0.0, 2.0, 1.0)).length() < EPS);
    assert!((c.tangent(0.0) - DVec3::new(0.0, 2.0, 0.0)).length() < EPS);
    assert!((c.param(DVec3::new(-5.0, 0.0, 1.0)) - PI).abs() < EPS);
    assert!((c.distance(DVec3::new(0.0, 0.0, 1.0)) - 2.0).abs() < EPS);
    assert_eq!(c.period(), Some(TAU));
    let e = Crv::Ellipse {
        a,
        r1: 3.0,
        r2: 1.0,
    };
    let q = e.point(1.0);
    assert!((e.param(q) - 1.0).abs() < EPS);
    assert!(e.distance(q) < EPS);
    let l = Crv::Line {
        p: DVec3::X,
        d: DVec3::Y * 2.0,
    };
    assert!((l.param(DVec3::new(5.0, 4.0, 0.0)) - 2.0).abs() < EPS);
    assert_eq!(l.period(), None);
}

#[test]
fn the_model_of_a_written_cube_has_one_part_and_all_counts() {
    let s = AnalyticKernel::default()
        .make_box(DVec3::new(1.0, 2.0, 3.0))
        .unwrap();
    let (text, _) = export(&[("Quader 1", &s)]);
    let m = Model::read(&text).unwrap();
    assert_eq!(m.uncertainty, 1e-6);
    assert_eq!(m.products.len(), 1);
    let p = &m.products[0];
    assert_eq!(p.name, "Quader 1");
    assert_eq!(p.solids.len(), 1);
    assert_eq!(p.solids[0].name, "Quader 1");
    assert!(p.solids[0].voids.is_empty());
    assert_eq!(p.solids[0].outer.faces.len(), 6);
    assert_eq!((m.edges.len(), m.vertices.len()), (12, 8));
    assert!(
        p.solids[0]
            .outer
            .faces
            .iter()
            .all(|f| matches!(f.surface, Surf::Plane(_)) && f.bounds.len() == 1)
    );
    assert_eq!(m.shells().len(), 1);
}

#[test]
fn entity_numbers_of_a_written_file_run_from_one_without_gaps() {
    let s = AnalyticKernel::default()
        .make_box(DVec3::new(1.0, 2.0, 3.0))
        .unwrap();
    let (text, report) = export(&[("Quader 1", &s)]);
    let m = Model::read(&text).unwrap();
    let keys: Vec<u64> = m.file.entities.keys().copied().collect();
    let n = keys.len() as u64;
    assert_eq!(keys, (1..=n).collect::<Vec<u64>>());
    assert_eq!(n, report.entities);
}

#[test]
fn names_written_by_the_writer_are_read_back_unchanged() {
    for name in ["Würfel", "Maß äö", "\u{1F600}", "é\u{1F600}", "O'Neil\\x"] {
        let literal = step_string(name).text;
        let text = format!(
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('x'),'2;1');\n\
             FILE_NAME('a.step','2026-10-14T00:00:00',(''),(''),'P','P','');\n\
             FILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 1 1 1 1 }}'));\nENDSEC;\n\
             DATA;\n#1=FOO({literal});\nENDSEC;\nEND-ISO-10303-21;\n"
        );
        assert!(literal.is_ascii(), "{literal}");
        let f = common::reader::parse(&text).unwrap();
        let a = f.simple(1, "FOO").unwrap();
        assert!(matches!(&a[0], Value::Str(_)), "{name}");
        assert_eq!(a[0].text().unwrap(), name, "{literal}");
    }
}

#[test]
fn the_model_refuses_wrong_units_and_skewed_axes() {
    let s = AnalyticKernel::default()
        .make_box(DVec3::splat(1.0))
        .unwrap();
    let (text, _) = export(&[("Q", &s)]);
    // Erstes Koordinatensystem mit der Achse als Bezugsrichtung (schief).
    let line = text
        .lines()
        .find(|l| l.contains("AXIS2_PLACEMENT_3D("))
        .unwrap();
    let args: Vec<&str> = line.trim_end_matches(");").split(',').collect();
    let skewed = format!("{},{},{},{});", args[0], args[1], args[2], args[2]);
    for (from, to, want) in [
        (
            "SI_UNIT(.MILLI.,.METRE.)",
            "SI_UNIT($,.METRE.)",
            "Millimetern",
        ),
        (
            "DIRECTION('',(0.,0.,1.))",
            "DIRECTION('',(0.,0.,2.))",
            "nicht normiert",
        ),
        (line, skewed.as_str(), "nicht senkrecht"),
        ("MANIFOLD_SOLID_BREP('Q'", "MANIFOLD_SOLID_BREP('X'", ""),
    ] {
        assert!(text.contains(from), "{from}");
        let broken = text.replacen(from, to, 1);
        match Model::read(&broken) {
            Err(e) => assert!(e.contains(want), "{from}: {e}"),
            Ok(m) => assert_eq!(m.products[0].solids[0].name, "X", "{from}"),
        }
    }
}

#[test]
fn the_model_names_a_knot_type_other_than_unspecified() {
    let k = AnalyticKernel::default();
    // Offene Kurve (einfacher Eintrag) und rationale Ellipse (zusammengesetzter Eintrag).
    let open = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(12.0, 5.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ],
        &[0.0, 0.5, 1.0],
    )
    .unwrap();
    let ellipse =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 6.0, DVec3::Y * 3.0, 0.0, TAU).unwrap();
    let square = |side: Segment| ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            side,
            Segment::Line {
                to: DVec2::new(0.0, 10.0),
            },
            Segment::Line { to: DVec2::ZERO },
        ],
    };
    let (mut text, mut tried) = (String::new(), 0);
    for outer in [
        square(Segment::Nurbs {
            curve: Arc::new(open),
        }),
        ProfileLoop::nurbs_loop(ellipse).unwrap(),
    ] {
        let solid = k
            .extrude(
                &Profile {
                    outer,
                    holes: vec![],
                },
                &Frame::WORLD,
                5.0,
            )
            .unwrap();
        text = export(&[("K", &solid)]).0;
        assert!(Model::read(&text).is_ok());
        for entity in ["B_SPLINE_CURVE_WITH_KNOTS(", "B_SPLINE_SURFACE_WITH_KNOTS("] {
            // Das Kennzeichen steht am Ende des Eintrags (Zeilen sind umbrochen, daher bis `;`).
            let from = text.find(entity).expect("Eintrag vorhanden");
            let end = from + text[from..].find(';').unwrap();
            let at = from + text[from..end].rfind(".UNSPECIFIED.").unwrap();
            let broken = format!(
                "{}.PIECEWISE_BEZIER_KNOTS.{}",
                &text[..at],
                &text[at + ".UNSPECIFIED.".len()..]
            );
            let Err(e) = Model::read(&broken) else {
                panic!("{entity}: Knotenart nicht abgelehnt");
            };
            assert!(e.contains("Knotenart"), "{entity}: {e}");
            tried += 1;
        }
    }
    assert_eq!(tried, 4, "Kurve und Fläche, einfach und zusammengesetzt");
    assert!(
        text.contains("RATIONAL_B_SPLINE_CURVE("),
        "zusammengesetzte Form geprüft"
    );
}

#[test]
fn the_distance_to_a_very_slender_ellipse_is_not_amplified_by_its_shape() {
    // Halbachsen 2080 und 3,2 (Verhältnis über 600, wie bei einem streifenden Schnitt): Ein
    // Punkt 1e-12 neben der Kurve muss auch diesen Abstand ergeben, nicht das Vielfache.
    const OFF: f64 = 1e-12;
    let c = Crv::Ellipse {
        a: Axis {
            origin: DVec3::new(1000.0, 17.0, -16.0),
            ..world()
        },
        r1: 2080.0,
        r2: 3.2,
    };
    for t in [2.6, 3.0, 3.1, 3.3, 4.0, 5.5] {
        let n = c.tangent(t).cross(DVec3::Z).normalize();
        let q = c.point(t) + n * OFF;
        let d = c.distance(q);
        assert!((d - OFF).abs() < OFF / 10.0, "t = {t}: {d}");
    }
}
