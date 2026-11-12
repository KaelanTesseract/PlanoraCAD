//! Tests des DXF-Lesers für die Grundelemente (Skizzen 9). Die Dateien sind selbst geschrieben.

use planora_exchange::import2d::{Element2d, Import2d, Unit2d};
use planora_exchange::read_dxf;
use planora_math::DVec2;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

const NEVER: &dyn Fn() -> bool = &|| false;

/// Baut eine Text-DXF aus Kopfvariablen und Entitäten. Jede Zeile ist „Code Wert“.
fn dxf(header: &str, entities: &str) -> Vec<u8> {
    fn pairs(s: &str, out: &mut String) {
        for line in s.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let (code, value) = line.split_once(' ').unwrap_or((line, ""));
            out.push_str(&format!("{code:>3}\n{}\n", value.trim()));
        }
    }
    let mut out = String::new();
    if !header.is_empty() {
        out.push_str("  0\nSECTION\n  2\nHEADER\n");
        pairs(header, &mut out);
        out.push_str("  0\nENDSEC\n");
    }
    out.push_str("  0\nSECTION\n  2\nENTITIES\n");
    pairs(entities, &mut out);
    out.push_str("  0\nENDSEC\n  0\nEOF\n");
    out.into_bytes()
}

fn read(header: &str, entities: &str) -> Import2d {
    read_dxf(&dxf(header, entities), NEVER).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn only(imp: &Import2d) -> &Element2d {
    assert_eq!(imp.element_count(), 1, "{imp:?}");
    &imp.layers[0].elements[0]
}

#[test]
fn line_circle_arc_in_layers() {
    let imp = read_dxf(include_bytes!("data/dxf/basic_r12.dxf"), NEVER).unwrap();
    assert_eq!(imp.layers.len(), 2);
    assert_eq!(imp.layers[0].name, "Wand");
    assert_eq!(imp.layers[1].name, "Tuer");
    assert_eq!(
        imp.layers[0].elements[0],
        Element2d::Line(DVec2::ZERO, DVec2::new(10.0, 0.0))
    );
    assert_eq!(
        imp.layers[0].elements[1],
        Element2d::Circle {
            c: DVec2::new(20.0, 0.0),
            r: 5.0
        }
    );
    match &imp.layers[1].elements[0] {
        Element2d::Arc { c, r, start, end } => {
            assert_eq!((*c, *r), (DVec2::ZERO, 3.0));
            assert!(
                close(*start, 0.0) && close(*end, FRAC_PI_2),
                "{start} {end}"
            );
        }
        e => panic!("kein Bogen: {e:?}"),
    }
    assert_eq!(imp.units, None);
}

#[test]
fn insunits_4_is_mm_and_missing_is_none() {
    let imp = read_dxf(include_bytes!("data/dxf/basic_r2000.dxf"), NEVER).unwrap();
    assert_eq!(imp.units, Some(Unit2d::Millimeter));
    assert_eq!(read("", "0 POINT\n10 1\n20 2").units, None);
    let inch = read("9 $INSUNITS\n70 1", "0 POINT\n10 1\n20 2");
    assert_eq!(inch.units, Some(Unit2d::Inch));
}

#[test]
fn lwpolyline_bulge_becomes_arc() {
    // Bulge 1 zwischen (0,0) und (10,0): Halbkreis gegen den Uhrzeigersinn, unterhalb der Sehne.
    let open = "0 LWPOLYLINE\n90 2\n70 0\n10 0\n20 0\n42 1\n10 10\n20 0";
    let imp = read("", open);
    match only(&imp) {
        Element2d::Arc { c, r, start, end } => {
            assert!(
                close(c.x, 5.0) && close(c.y, 0.0) && close(*r, 5.0),
                "{c:?} {r}"
            );
            assert!(same_angle(*start, PI), "start {start}");
            assert!(close((*end - *start).rem_euclid(TAU), PI), "end {end}");
        }
        e => panic!("kein Bogen: {e:?}"),
    }
    // Negativer Bulge: Halbkreis im Uhrzeigersinn, als Bogen mit vertauschten Enden.
    let neg = "0 LWPOLYLINE\n90 2\n70 0\n10 0\n20 0\n42 -1\n10 10\n20 0";
    let imp = read("", neg);
    match only(&imp) {
        Element2d::Arc { c, r, start, end } => {
            assert!(close(c.x, 5.0) && close(c.y, 0.0) && close(*r, 5.0));
            // Gegen den Uhrzeigersinn von (10,0) nach (0,0): Winkel 0 bis PI (oben).
            assert!(same_angle(*start, 0.0), "start {start}");
            assert!(close((*end - *start).rem_euclid(TAU), PI), "end {end}");
        }
        e => panic!("kein Bogen: {e:?}"),
    }
}

#[test]
fn closed_polyline_with_repeated_last_point_has_no_zero_segment() {
    let ent =
        "0 LWPOLYLINE\n90 5\n70 1\n10 0\n20 0\n10 4\n20 0\n10 4\n20 3\n10 0\n20 3\n10 0\n20 0";
    let imp = read("", ent);
    assert_eq!(imp.element_count(), 4, "{imp:?}");
    assert_eq!(imp.skipped.degenerate, 0);
    for e in &imp.layers[0].elements {
        match e {
            Element2d::Line(a, b) => assert!(a.distance(*b) > 1e-9, "{e:?}"),
            e => panic!("keine Linie: {e:?}"),
        }
    }
    // Ohne Wiederholung schließt die Kontur ebenfalls mit vier Linien.
    let plain = "0 LWPOLYLINE\n90 4\n70 1\n10 0\n20 0\n10 4\n20 0\n10 4\n20 3\n10 0\n20 3";
    assert_eq!(read("", plain).element_count(), 4);
    // Offen bleibt es bei drei.
    let open = "0 LWPOLYLINE\n90 4\n70 0\n10 0\n20 0\n10 4\n20 0\n10 4\n20 3\n10 0\n20 3";
    assert_eq!(read("", open).element_count(), 3);
}

#[test]
fn old_style_polyline_with_vertices() {
    let ent = "0 POLYLINE\n8 P\n66 1\n70 1\n0 VERTEX\n10 0\n20 0\n0 VERTEX\n10 2\n20 0\n0 VERTEX\n10 2\n20 2\n0 SEQEND";
    let imp = read("", ent);
    assert_eq!(imp.element_count(), 3, "{imp:?}");
    let mesh =
        "0 POLYLINE\n66 1\n70 16\n0 VERTEX\n10 0\n20 0\n30 0\n0 VERTEX\n10 1\n20 0\n30 1\n0 SEQEND";
    let imp = read("", mesh);
    assert_eq!((imp.element_count(), imp.skipped.three_d), (0, 1));
}

#[test]
fn ellipse_partial_keeps_params() {
    let ent = "0 ELLIPSE\n10 1\n20 2\n30 0\n11 4\n21 0\n31 0\n40 0.5\n41 0.5\n42 2.5";
    let imp = read("", ent);
    match only(&imp) {
        Element2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            assert_eq!(
                (*c, *major, *ratio),
                (DVec2::new(1.0, 2.0), DVec2::new(4.0, 0.0), 0.5)
            );
            assert!(close(*start, 0.5) && close(*end, 2.5));
        }
        e => panic!("keine Ellipse: {e:?}"),
    }
    let full = "0 ELLIPSE\n10 0\n20 0\n30 0\n11 4\n21 0\n31 0\n40 0.5\n41 0\n42 6.283185307179586";
    match only(&read("", full)) {
        Element2d::Ellipse { start, end, .. } => assert!(*start == 0.0 && close(*end, TAU)),
        e => panic!("keine Ellipse: {e:?}"),
    }
}

#[test]
fn spline_with_control_points_and_weights() {
    // Grad 2, drei Kontrollpunkte, Knoten 0 0 0 1 1 1, mittleres Gewicht 2.
    let ent = "0 SPLINE\n70 8\n71 2\n72 6\n73 3\n40 0\n40 0\n40 0\n40 1\n40 1\n40 1\n\
               41 1\n41 2\n41 1\n10 0\n20 0\n30 0\n10 1\n20 1\n30 0\n10 2\n20 0\n30 0";
    let imp = read("", ent);
    match only(&imp) {
        Element2d::Nurbs {
            degree,
            knots,
            points,
            weights,
        } => {
            assert_eq!(*degree, 2);
            assert_eq!(knots, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
            assert_eq!(
                points,
                &[DVec2::ZERO, DVec2::new(1.0, 1.0), DVec2::new(2.0, 0.0)]
            );
            assert_eq!(weights.as_deref(), Some(&[1.0, 2.0, 1.0][..]));
        }
        e => panic!("kein NURBS: {e:?}"),
    }
    // Ohne Gewichte: keine Gewichte.
    let plain = "0 SPLINE\n70 8\n71 2\n72 6\n73 3\n40 0\n40 0\n40 0\n40 1\n40 1\n40 1\n\
                 10 0\n20 0\n30 0\n10 1\n20 1\n30 0\n10 2\n20 0\n30 0";
    match only(&read("", plain)) {
        Element2d::Nurbs { weights, .. } => assert_eq!(*weights, None),
        e => panic!("kein NURBS: {e:?}"),
    }
}

#[test]
fn spline_fit_points_only_becomes_fit_spline() {
    let ent = "0 SPLINE\n70 1064\n71 3\n74 3\n11 0\n21 0\n31 0\n11 1\n21 2\n31 0\n11 3\n21 0\n31 0";
    match only(&read("", ent)) {
        Element2d::FitSpline(p) => assert_eq!(
            p,
            &[DVec2::ZERO, DVec2::new(1.0, 2.0), DVec2::new(3.0, 0.0)]
        ),
        e => panic!("kein Stützpunkt-Spline: {e:?}"),
    }
}

#[test]
fn point_entity() {
    let imp = read("", "0 POINT\n8 Pkt\n10 3\n20 4\n30 0");
    assert_eq!(imp.layers[0].name, "Pkt");
    assert_eq!(only(&imp), &Element2d::Point(DVec2::new(3.0, 4.0)));
}

#[test]
fn extrusion_minus_z_is_mirrored() {
    let ent = "0 CIRCLE\n10 3\n20 1\n30 0\n40 2\n210 0\n220 0\n230 -1";
    assert_eq!(
        only(&read("", ent)),
        &Element2d::Circle {
            c: DVec2::new(-3.0, 1.0),
            r: 2.0
        }
    );
    // Bogen 0 bis 90 Grad, gespiegelt: wird zu 90 bis 180 Grad.
    let arc = "0 ARC\n10 0\n20 0\n30 0\n40 1\n50 0\n51 90\n210 0\n220 0\n230 -1";
    match only(&read("", arc)) {
        Element2d::Arc { start, end, .. } => {
            assert!(close(*start, FRAC_PI_2) && close(*end, PI), "{start} {end}");
        }
        e => panic!("kein Bogen: {e:?}"),
    }
    // Schräge Ebene: übersprungen.
    let tilted = "0 CIRCLE\n10 0\n20 0\n30 0\n40 2\n210 1\n220 0\n230 0";
    let imp = read("", tilted);
    assert_eq!((imp.element_count(), imp.skipped.three_d), (0, 1));
}

#[test]
fn out_of_plane_line_is_skipped() {
    let imp = read("", "0 LINE\n10 0\n20 0\n30 0\n11 10\n21 0\n31 5");
    assert_eq!(imp.element_count(), 0);
    assert_eq!(imp.skipped.three_d, 1);
    let face = read(
        "",
        "0 3DFACE\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0\n12 1\n22 1\n32 0",
    );
    assert_eq!(face.skipped.three_d, 1);
}

#[test]
fn dimension_and_hatch_are_counted() {
    let ent = "0 DIMENSION\n8 M\n2 *D1\n10 0\n20 0\n30 0\n11 0\n21 0\n31 0\n70 0\n13 0\n23 0\n33 0\n14 5\n24 0\n34 0\n\
               0 HATCH\n8 S\n10 0\n20 0\n30 0\n210 0\n220 0\n230 1\n2 SOLID\n70 1\n91 0\n\
               0 HATCH\n8 S\n67 1\n2 SOLID\n\
               0 LINE\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0";
    let imp = read("", ent);
    assert_eq!(imp.skipped.dimensions, 1, "{:?}", imp.skipped);
    assert_eq!(imp.skipped.hatches, 1, "{:?}", imp.skipped);
    assert_eq!(imp.element_count(), 1);
}

#[test]
fn paper_space_is_skipped() {
    let ent = "0 LINE\n67 1\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0\n\
               0 CIRCLE\n10 0\n20 0\n30 0\n40 1";
    let imp = read("", ent);
    assert_eq!(imp.element_count(), 1);
    assert!(matches!(only(&imp), Element2d::Circle { .. }));
}

#[test]
fn nan_and_zero_radius_are_degenerate() {
    let ent = "0 CIRCLE\n10 0\n20 0\n30 0\n40 0\n\
               0 LINE\n10 1\n20 1\n30 0\n11 1\n21 1\n31 0\n\
               0 ELLIPSE\n10 0\n20 0\n30 0\n11 0\n21 0\n31 0\n40 0.5\n41 0\n42 6.28\n\
               0 CIRCLE\n10 0\n20 0\n30 0\n40 1";
    let imp = read("", ent);
    assert_eq!(imp.element_count(), 1, "{imp:?}");
    assert_eq!(imp.skipped.degenerate, 3);
    // „nan“ im Text: entweder ein Lesefehler oder ein entartetes Element, nie ein Absturz.
    let nan = "0 CIRCLE\n10 nan\n20 0\n30 0\n40 1\n0 CIRCLE\n10 0\n20 0\n30 0\n40 1";
    match read_dxf(&dxf("", nan), NEVER) {
        Ok(imp) => assert_eq!(imp.element_count() + imp.skipped.degenerate, 2),
        Err(e) => assert!(!e.to_string().is_empty()),
    }
}

#[test]
fn broken_file_is_an_error_and_cancel_works() {
    let e = read_dxf(b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nabc\n", NEVER).unwrap_err();
    assert!(!e.to_string().is_empty());
    let lines = "0 POINT\n10 1\n20 1\n".repeat(5000);
    let r = read_dxf(&dxf("", &lines), &|| true);
    assert_eq!(r.unwrap_err(), planora_exchange::ExchangeError::Cancelled);
}

/// Gleicher Winkel bis auf ganze Umdrehungen.
fn same_angle(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(TAU);
    d < 1e-9 || TAU - d < 1e-9
}
