//! Tests des DXF-Lesers für die Grundelemente (Skizzen 9). Die Dateien sind selbst geschrieben.

use planora_exchange::import2d::{Element2d, HAlign2d, Import2d, Unit2d, VAlign2d};
use planora_exchange::limits::{MAX_BLOCK_DEPTH, MAX_IMPORT_ELEMENTS};
use planora_exchange::{ExchangeError, read_dxf};
use planora_math::DVec2;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

const NEVER: &dyn Fn() -> bool = &|| false;

/// Baut eine Text-DXF aus Kopfvariablen und Entitäten. Jede Zeile ist „Code Wert“.
fn dxf(header: &str, entities: &str) -> Vec<u8> {
    dxf_blocks(header, "", entities)
}

/// Wie [`dxf`], mit einem BLOCKS-Abschnitt (fertige BLOCK-Texte, siehe [`block`]).
fn dxf_blocks(header: &str, blocks: &str, entities: &str) -> Vec<u8> {
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
    if !blocks.is_empty() {
        out.push_str("  0\nSECTION\n  2\nBLOCKS\n");
        pairs(blocks, &mut out);
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

/// Ein BLOCK mit Basispunkt (bx, by) und Inhalt.
fn block(name: &str, base: (f64, f64), body: &str) -> String {
    format!(
        "0 BLOCK\n8 0\n2 {name}\n70 0\n10 {}\n20 {}\n30 0\n3 {name}\n{body}\n0 ENDBLK\n8 0\n",
        base.0, base.1
    )
}

fn read_b(blocks: &str, entities: &str) -> Import2d {
    read_dxf(&dxf_blocks("", blocks, entities), NEVER).unwrap()
}

/// Ein INSERT mit Lage, Maßstab und Drehwinkel in Grad.
fn insert(name: &str, layer: &str, at: (f64, f64), scale: (f64, f64), rot: f64) -> String {
    format!(
        "0 INSERT\n8 {layer}\n2 {name}\n10 {}\n20 {}\n30 0\n41 {}\n42 {}\n43 1\n50 {rot}\n",
        at.0, at.1, scale.0, scale.1
    )
}

fn line_ends(e: &Element2d) -> (DVec2, DVec2) {
    match e {
        Element2d::Line(a, b) => (*a, *b),
        e => panic!("keine Linie: {e:?}"),
    }
}

fn near(a: DVec2, b: DVec2) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
}

/// Punkt der Ellipse bei Parameter t.
fn ellipse_at(c: DVec2, major: DVec2, ratio: f64, t: f64) -> DVec2 {
    c + major * t.cos() + DVec2::new(-major.y, major.x) * ratio * t.sin()
}

/// Ein Bogen mit normierten Winkeln: Anfang in [0, 2pi), Ende in (Anfang, Anfang + 2pi].
fn assert_normal_arc(e: &Element2d) {
    match e {
        Element2d::Arc { start, end, .. } => {
            assert!((0.0..TAU).contains(start), "start {start}");
            assert!(
                *end > *start && *end <= *start + TAU + 1e-12,
                "{start}..{end}"
            );
        }
        e => panic!("kein Bogen: {e:?}"),
    }
}

#[test]
fn insert_places_rotates_scales_block() {
    let blocks = block(
        "B",
        (0.0, 0.0),
        "0 LINE\n8 0\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0",
    );
    // Lage (10,20), Maßstab 2, 90 Grad: (0,0)-(1,0) wird zu (10,20)-(10,22).
    let imp = read_b(
        &blocks,
        &insert("B", "Teile", (10.0, 20.0), (2.0, 2.0), 90.0),
    );
    assert_eq!(imp.layers.len(), 1, "{imp:?}");
    // Layer „0“ im Block übernimmt den Layer der INSERT-Zeile.
    assert_eq!(imp.layers[0].name, "Teile");
    let (a, b) = line_ends(only(&imp));
    assert!(
        near(a, DVec2::new(10.0, 20.0)) && near(b, DVec2::new(10.0, 22.0)),
        "{a:?} {b:?}"
    );
    // Der Basispunkt des Blocks liegt auf der Einfügestelle.
    let blocks = block(
        "B",
        (1.0, 0.0),
        "0 LINE\n8 0\n10 1\n20 0\n30 0\n11 2\n21 0\n31 0",
    );
    let imp = read_b(&blocks, &insert("B", "0", (5.0, 5.0), (1.0, 1.0), 0.0));
    let (a, b) = line_ends(only(&imp));
    assert!(
        near(a, DVec2::new(5.0, 5.0)) && near(b, DVec2::new(6.0, 5.0)),
        "{a:?} {b:?}"
    );
    // Eigener Layer im Block bleibt; Blocknamen gelten ohne Rücksicht auf die Schreibweise.
    let blocks = block("Bl", (0.0, 0.0), "0 POINT\n8 Eigen\n10 1\n20 1\n30 0");
    let imp = read_b(&blocks, &insert("BL", "Teile", (0.0, 0.0), (1.0, 1.0), 0.0));
    assert_eq!(imp.layers[0].name, "Eigen");
    // Unbekannter Block: gezählt, kein Fehler.
    let imp = read_b(&blocks, &insert("fehlt", "0", (0.0, 0.0), (1.0, 1.0), 0.0));
    assert_eq!((imp.element_count(), imp.skipped.unknown), (0, 1));
}

#[test]
fn nested_insert_depth_17_is_cut_and_counted() {
    // B1 enthält eine Linie und fügt B2 ein ... B17 enthält nur noch eine Linie.
    let mut blocks = String::new();
    for k in 1..=17 {
        let mut body = String::from("0 LINE\n8 0\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0\n");
        if k < 17 {
            body.push_str(&insert(
                &format!("B{}", k + 1),
                "0",
                (1.0, 0.0),
                (1.0, 1.0),
                0.0,
            ));
        }
        blocks.push_str(&block(&format!("B{k}"), (0.0, 0.0), &body));
    }
    let imp = read_b(&blocks, &insert("B1", "0", (0.0, 0.0), (1.0, 1.0), 0.0));
    // B1 bis B16 werden geöffnet (Tiefe 16); das Einfügen von B17 wird abgeschnitten.
    assert_eq!(MAX_BLOCK_DEPTH, 16);
    assert_eq!(imp.element_count(), 16, "{imp:?}");
    assert_eq!(imp.skipped.unknown, 1);
    // Die Linie der Tiefe k liegt um k - 1 nach rechts verschoben.
    let (a, _) = line_ends(&imp.layers[0].elements[15]);
    assert!(near(a, DVec2::new(15.0, 0.0)), "{a:?}");
    // Ein Block, der sich selbst einfügt, endet ebenfalls.
    let blocks = block(
        "S",
        (0.0, 0.0),
        &insert("S", "0", (1.0, 0.0), (1.0, 1.0), 0.0),
    );
    let imp = read_b(&blocks, &insert("S", "0", (0.0, 0.0), (1.0, 1.0), 0.0));
    assert_eq!((imp.element_count(), imp.skipped.unknown), (0, 1));
}

/// Alle Punkte der ersten Ebene, auf ganze Zahlen gerundet und sortiert.
fn rounded_points(imp: &Import2d) -> Vec<(i64, i64)> {
    let mut pts: Vec<(i64, i64)> = imp.layers[0]
        .elements
        .iter()
        .map(|e| match e {
            Element2d::Point(p) => (p.x.round() as i64, p.y.round() as i64),
            e => panic!("kein Punkt: {e:?}"),
        })
        .collect();
    pts.sort();
    pts
}

#[test]
fn insert_array_rows_columns() {
    let blocks = block("P", (0.0, 0.0), "0 POINT\n8 0\n10 0\n20 0\n30 0");
    // 3 Spalten (Abstand 5), 2 Reihen (Abstand 10).
    let ent = format!(
        "{}70 3\n71 2\n44 5\n45 10\n",
        insert("P", "0", (1.0, 1.0), (1.0, 1.0), 0.0)
    );
    let imp = read_b(&blocks, &ent);
    assert_eq!(
        rounded_points(&imp),
        [(1, 1), (1, 11), (6, 1), (6, 11), (11, 1), (11, 11)]
    );
    // Um 90 Grad gedreht liegen die Spalten nach oben, die Reihen nach links.
    let ent = format!(
        "{}70 2\n71 2\n44 5\n45 10\n",
        insert("P", "0", (0.0, 0.0), (1.0, 1.0), 90.0)
    );
    let imp = read_b(&blocks, &ent);
    assert_eq!(rounded_points(&imp), [(-10, 0), (-10, 5), (0, 0), (0, 5)]);
}

#[test]
fn mirrored_insert_keeps_arc_direction() {
    // Bogen 0 bis 90 Grad um (0,0); Maßstab x = -1 spiegelt ihn zu 90 bis 180 Grad.
    let blocks = block(
        "A",
        (0.0, 0.0),
        "0 ARC\n8 0\n10 0\n20 0\n30 0\n40 1\n50 0\n51 90",
    );
    let imp = read_b(&blocks, &insert("A", "0", (0.0, 0.0), (-1.0, 1.0), 0.0));
    match only(&imp) {
        Element2d::Arc { c, r, start, end } => {
            assert!(near(*c, DVec2::ZERO) && close(*r, 1.0));
            assert!(close(*start, FRAC_PI_2) && close(*end, PI), "{start} {end}");
        }
        e => panic!("kein Bogen: {e:?}"),
    }
    // Maßstab y = -1: 0 bis 90 Grad wird zu 270 bis 360 Grad.
    let imp = read_b(&blocks, &insert("A", "0", (0.0, 0.0), (1.0, -1.0), 0.0));
    match only(&imp) {
        Element2d::Arc { start, end, .. } => {
            assert!(close(*start, 1.5 * PI) && close(*end, TAU), "{start} {end}");
        }
        e => panic!("kein Bogen: {e:?}"),
    }
}

#[test]
fn nonuniform_scale_turns_circle_into_ellipse() {
    let blocks = block("K", (0.0, 0.0), "0 CIRCLE\n8 0\n10 1\n20 0\n30 0\n40 2");
    let imp = read_b(&blocks, &insert("K", "0", (0.0, 0.0), (2.0, 1.0), 0.0));
    match only(&imp) {
        Element2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            assert!(near(*c, DVec2::new(2.0, 0.0)), "{c:?}");
            assert!(near(*major, DVec2::new(4.0, 0.0)), "{major:?}");
            assert!(close(*ratio, 0.5));
            assert!(*start == 0.0 && *end == TAU, "{start} {end}");
        }
        e => panic!("keine Ellipse: {e:?}"),
    }
    // Gedreht um 90 Grad zeigt die große Halbachse nach oben.
    let imp = read_b(&blocks, &insert("K", "0", (0.0, 0.0), (2.0, 1.0), 90.0));
    match only(&imp) {
        Element2d::Ellipse { major, ratio, .. } => {
            assert!(
                near(*major, DVec2::new(0.0, 4.0)) && close(*ratio, 0.5),
                "{major:?}"
            );
        }
        e => panic!("keine Ellipse: {e:?}"),
    }
    // Gleicher Maßstab bleibt ein Kreis.
    let imp = read_b(&blocks, &insert("K", "0", (0.0, 0.0), (3.0, 3.0), 0.0));
    assert_eq!(
        only(&imp),
        &Element2d::Circle {
            c: DVec2::new(3.0, 0.0),
            r: 6.0
        }
    );
    // Ein Viertelbogen unter ungleichem Maßstab: Die Enden bleiben an den richtigen Punkten,
    // auch gespiegelt (Drehsinn umgekehrt: der Bogen läuft dann von (0,1) nach (-2,0)).
    let blocks = block(
        "Q",
        (0.0, 0.0),
        "0 ARC\n8 0\n10 0\n20 0\n30 0\n40 1\n50 0\n51 90",
    );
    let imp = read_b(&blocks, &insert("Q", "0", (0.0, 0.0), (2.0, 1.0), 0.0));
    match only(&imp) {
        Element2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            assert!(near(
                ellipse_at(*c, *major, *ratio, *start),
                DVec2::new(2.0, 0.0)
            ));
            assert!(near(
                ellipse_at(*c, *major, *ratio, *end),
                DVec2::new(0.0, 1.0)
            ));
            assert!(close(*end - *start, FRAC_PI_2));
        }
        e => panic!("keine Ellipse: {e:?}"),
    }
    let imp = read_b(&blocks, &insert("Q", "0", (0.0, 0.0), (-2.0, 1.0), 0.0));
    match only(&imp) {
        Element2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            assert!(near(
                ellipse_at(*c, *major, *ratio, *start),
                DVec2::new(0.0, 1.0)
            ));
            assert!(near(
                ellipse_at(*c, *major, *ratio, *end),
                DVec2::new(-2.0, 0.0)
            ));
            assert!(close(*end - *start, FRAC_PI_2));
            assert!((0.0..TAU).contains(start));
        }
        e => panic!("keine Ellipse: {e:?}"),
    }
}

#[test]
fn text_and_mtext_become_text2d() {
    let ent = "0 TEXT\n8 T\n10 1\n20 2\n30 0\n40 3\n1 Platte %%c25\n50 90\n7 Standard\n\
               0 MTEXT\n8 M\n10 5\n20 6\n30 0\n40 2.5\n71 5\n1 Zeile eins\\Pzeile zwei\n\
               0 MTEXT\n8 M\n10 0\n20 0\n30 0\n40 2\n71 1\n1 {\\fArial|b1;Hi}";
    let imp = read("", ent);
    assert_eq!(imp.element_count(), 3, "{imp:?}");
    let text = |layer: usize, i: usize| match &imp.layers[layer].elements[i] {
        Element2d::Text(t) => t.clone(),
        e => panic!("kein Text: {e:?}"),
    };
    let t = text(0, 0);
    assert_eq!(t.text, "Platte Ø25");
    assert!(near(t.pos, DVec2::new(1.0, 2.0)) && close(t.height, 3.0) && close(t.angle, FRAC_PI_2));
    assert_eq!((t.halign, t.valign), (HAlign2d::Left, VAlign2d::Bottom));
    let m = text(1, 0);
    assert_eq!(m.text, "Zeile eins\nzeile zwei");
    assert_eq!((m.halign, m.valign), (HAlign2d::Center, VAlign2d::Middle));
    assert!(near(m.pos, DVec2::new(5.0, 6.0)) && close(m.height, 2.5));
    let h = text(1, 1);
    assert_eq!((h.text.as_str(), h.font.as_str()), ("Hi", "Arial"));
    assert_eq!((h.halign, h.valign), (HAlign2d::Left, VAlign2d::Top));
    // Text im Block wird mit dem Block verschoben, gedreht und skaliert.
    let blocks = block(
        "T",
        (0.0, 0.0),
        "0 TEXT\n8 0\n10 1\n20 0\n30 0\n40 2\n1 Hallo",
    );
    let imp = read_b(&blocks, &insert("T", "0", (10.0, 0.0), (3.0, 3.0), 90.0));
    match only(&imp) {
        Element2d::Text(t) => {
            assert!(near(t.pos, DVec2::new(10.0, 3.0)), "{:?}", t.pos);
            assert!(close(t.height, 6.0) && close(t.angle, FRAC_PI_2), "{t:?}");
        }
        e => panic!("kein Text: {e:?}"),
    }
}

#[test]
fn element_limit_stops_with_error() {
    let points = |n: usize| "0 POINT\n10 1\n20 1\n".repeat(n);
    // Genau an der Grenze geht es noch.
    let imp = read_dxf(&dxf("", &points(MAX_IMPORT_ELEMENTS)), NEVER).unwrap();
    assert_eq!(imp.element_count(), MAX_IMPORT_ELEMENTS);
    // Ein Element mehr ist ein Fehler, kein Teilergebnis.
    let e = read_dxf(&dxf("", &points(MAX_IMPORT_ELEMENTS + 1)), NEVER).unwrap_err();
    assert_eq!(e, ExchangeError::TooManyElements);
    assert!(!e.to_string().is_empty());
    // Gezählt wird, was entsteht: Ein Linienzug mit 60 000 Eckpunkten ergibt 59 999 Elemente.
    let long = |n: usize| {
        let mut s = format!("0 LWPOLYLINE\n90 {n}\n70 0\n");
        for i in 0..n {
            s.push_str(&format!("10 {i}\n20 {}\n", i % 2));
        }
        s
    };
    let one = read_dxf(&dxf("", &long(60_000)), NEVER).unwrap();
    assert_eq!(one.element_count(), 59_999);
    let two = format!("{}{}", long(60_000), long(60_000));
    assert_eq!(
        read_dxf(&dxf("", &two), NEVER).unwrap_err(),
        ExchangeError::TooManyElements
    );
    // Auch ein Feld aus Blöcken zählt: 400 x 300 Punkte.
    let blocks = block("P", (0.0, 0.0), "0 POINT\n8 0\n10 0\n20 0\n30 0");
    let ent = format!(
        "{}70 400\n71 300\n44 1\n45 1\n",
        insert("P", "0", (0.0, 0.0), (1.0, 1.0), 0.0)
    );
    let e = read_dxf(&dxf_blocks("", &blocks, &ent), NEVER).unwrap_err();
    assert_eq!(e, ExchangeError::TooManyElements);
}

#[test]
fn cancel_returns_cancelled() {
    use std::cell::Cell;
    // Die Abfrage vorab sagt "weiter", erst ein späterer Aufruf bricht ab: Das trifft den
    // regelmäßigen Takt innerhalb der Schleife.
    let calls = Cell::new(0);
    let cancel = || {
        calls.set(calls.get() + 1);
        calls.get() >= 2
    };
    let lines = "0 POINT\n10 1\n20 1\n".repeat(5000);
    let r = read_dxf(&dxf("", &lines), &cancel);
    assert_eq!(r.unwrap_err(), ExchangeError::Cancelled);
    assert_eq!(calls.get(), 2);
    // Dasselbe beim Auflösen eines großen Blockfelds.
    let calls = Cell::new(0);
    let cancel = || {
        calls.set(calls.get() + 1);
        calls.get() >= 2
    };
    let blocks = block("P", (0.0, 0.0), "0 POINT\n8 0\n10 0\n20 0\n30 0");
    let ent = format!(
        "{}70 100\n71 100\n44 1\n45 1\n",
        insert("P", "0", (0.0, 0.0), (1.0, 1.0), 0.0)
    );
    let r = read_dxf(&dxf_blocks("", &blocks, &ent), &cancel);
    assert_eq!(r.unwrap_err(), ExchangeError::Cancelled);
    assert_eq!(calls.get(), 2);
    // Gleich zu Beginn abbrechen.
    let r = read_dxf(&dxf("", "0 POINT\n10 1\n20 1"), &|| true);
    assert_eq!(r.unwrap_err(), ExchangeError::Cancelled);
}

#[test]
fn binary_dxf_reads() {
    // Die Binärdatei entsteht im Test aus einem selbst geschriebenen Text-DXF: laden und mit
    // `save_binary` der Bibliothek wieder schreiben.
    let text = dxf(
        "9 $ACADVER\n1 AC1015\n9 $INSUNITS\n70 4",
        "0 LINE\n8 Wand\n10 0\n20 0\n30 0\n11 10\n21 0\n31 0\n\
         0 CIRCLE\n8 Wand\n10 5\n20 5\n30 0\n40 2\n\
         0 ARC\n8 Tuer\n10 0\n20 0\n30 0\n40 3\n50 0\n51 90",
    );
    let drawing = dxf::Drawing::load(&mut std::io::Cursor::new(&text)).unwrap();
    let mut binary = Vec::new();
    drawing.save_binary(&mut binary).unwrap();
    assert!(binary.starts_with(b"AutoCAD Binary DXF"));
    let from_binary = read_dxf(&binary, NEVER).unwrap();
    let from_text = read_dxf(&text, NEVER).unwrap();
    assert_eq!(from_binary.element_count(), 3);
    assert_eq!(from_binary.layers, from_text.layers);
    assert_eq!(from_binary.units, Some(Unit2d::Millimeter));
}

#[test]
fn hand_written_block_file_reads() {
    let imp = read_dxf(include_bytes!("data/dxf/blocks_r2000.dxf"), NEVER).unwrap();
    // 2 x 2 Einfügungen mit je Linie, Bogen, Ellipse, MTEXT, dazu TEXT und Spline.
    assert_eq!(imp.element_count(), 4 * 4 + 2, "{:?}", imp.skipped);
    assert_eq!(imp.units, Some(Unit2d::Millimeter));
    let names: Vec<&str> = imp.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Teile", "Bogen", "Beschriftung", "Kurve"]);
}

#[test]
fn emitted_angles_are_normalised() {
    // Bulge-Bögen tragen rohe atan2-Winkel; ausgegeben wird normiert.
    let neg = "0 LWPOLYLINE\n90 3\n70 0\n10 0\n20 0\n42 1\n10 10\n20 0\n42 -0.5\n10 10\n20 10";
    for e in &read("", neg).layers[0].elements {
        assert_normal_arc(e);
    }
    // Bogen über die Nullstelle (350 bis 10 Grad), ein voller Bogen (0 bis 360 Grad) und
    // ein gespiegelter Bogen.
    let ent = "0 ARC\n10 0\n20 0\n30 0\n40 1\n50 350\n51 10\n\
               0 ARC\n10 0\n20 0\n30 0\n40 1\n50 0\n51 360\n\
               0 ARC\n10 0\n20 0\n30 0\n40 1\n50 30\n51 90\n210 0\n220 0\n230 -1";
    let imp = read("", ent);
    for e in &imp.layers[0].elements {
        assert_normal_arc(e);
    }
    match (&imp.layers[0].elements[0], &imp.layers[0].elements[1]) {
        (
            Element2d::Arc { start, end, .. },
            Element2d::Arc {
                start: s2, end: e2, ..
            },
        ) => {
            assert!(close(*start, 350f64.to_radians()) && close(*end - *start, 20f64.to_radians()));
            assert!(*s2 == 0.0 && close(*e2, TAU), "{s2} {e2}");
        }
        e => panic!("keine Bögen: {e:?}"),
    }
    // Eine volle Ellipse mit Ebene -Z ist genau 0..2pi (nicht -2pi..0).
    let full = "0 ELLIPSE\n10 0\n20 0\n30 0\n11 4\n21 0\n31 0\n40 0.5\n41 0\n42 6.283185307179586\n\
                210 0\n220 0\n230 -1";
    match only(&read("", full)) {
        Element2d::Ellipse { start, end, .. } => {
            assert!(*start == 0.0 && *end == TAU, "{start} {end}")
        }
        e => panic!("keine Ellipse: {e:?}"),
    }
}

#[test]
fn ellipse_mirrored_under_extrusion_keeps_its_points() {
    // Teilstück t = 0.5 bis 2.5 mit Richtung -Z: Der Parameter läuft von oben gesehen
    // rückwärts; als Bogen gegen den Uhrzeigersinn geht es von t = 2.5 nach t = 0.5.
    let ent = "0 ELLIPSE\n10 1\n20 2\n30 0\n11 4\n21 0\n31 0\n40 0.5\n41 0.5\n42 2.5\n\
               210 0\n220 0\n230 -1";
    match only(&read("", ent)) {
        Element2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            assert!((0.0..TAU).contains(start) && *end > *start && *end <= *start + TAU);
            assert!(close(*end - *start, 2.0));
            // Punkt der Datei bei t: c + major*cos t - Lotrechte*ratio*sin t.
            let p = |t: f64| {
                DVec2::new(1.0, 2.0) + DVec2::new(4.0, 0.0) * t.cos()
                    - DVec2::new(0.0, 4.0) * 0.5 * t.sin()
            };
            assert!(near(ellipse_at(*c, *major, *ratio, *start), p(2.5)));
            assert!(near(ellipse_at(*c, *major, *ratio, *end), p(0.5)));
        }
        e => panic!("keine Ellipse: {e:?}"),
    }
}

#[test]
fn lwpolyline_bulge_mirrored_under_extrusion() {
    // Halbkreis von (0,0) nach (10,0) unterhalb der Sehne; gespiegelt (-Z) liegt er um den
    // Mittelpunkt (-5,0), immer noch unterhalb: Bogen von pi bis 2pi.
    let ent = "0 LWPOLYLINE\n90 2\n70 0\n210 0\n220 0\n230 -1\n10 0\n20 0\n42 1\n10 10\n20 0";
    match only(&read("", ent)) {
        Element2d::Arc { c, r, start, end } => {
            assert!(
                near(*c, DVec2::new(-5.0, 0.0)) && close(*r, 5.0),
                "{c:?} {r}"
            );
            assert!(close(*start, PI) && close(*end, TAU), "{start} {end}");
        }
        e => panic!("kein Bogen: {e:?}"),
    }
}

#[test]
fn closed_two_point_polyline_drops_only_duplicate_segments() {
    // Zwei Punkte, geschlossen, beide Segmente gerade: nur eine Linie, nicht zwei übereinander.
    let two = "0 LWPOLYLINE\n90 2\n70 1\n10 0\n20 0\n10 10\n20 0";
    assert_eq!(read("", two).element_count(), 1);
    // Ebenso, wenn der Zug auf zwei Punkte schrumpft, weil der erste am Ende wiederkehrt.
    let back = "0 LWPOLYLINE\n90 3\n70 1\n10 0\n20 0\n10 10\n20 0\n10 0\n20 0";
    assert_eq!(read("", back).element_count(), 1);
    // Bulge nur am ersten Punkt: Bogen plus schließende Sehne (Halbscheibe).
    let half = "0 LWPOLYLINE\n90 2\n70 1\n10 0\n20 0\n42 1\n10 10\n20 0";
    let imp = read("", half);
    assert_eq!(imp.element_count(), 2, "{imp:?}");
    assert!(matches!(imp.layers[0].elements[0], Element2d::Arc { .. }));
    let (a, b) = line_ends(&imp.layers[0].elements[1]);
    assert!(
        near(a, DVec2::new(10.0, 0.0)) && near(b, DVec2::ZERO),
        "{a:?} {b:?}"
    );
    // Bulge nur am zweiten Punkt: Sehne plus Bogen zurück.
    let half2 = "0 LWPOLYLINE\n90 2\n70 1\n10 0\n20 0\n10 10\n20 0\n42 1";
    let imp = read("", half2);
    assert_eq!(imp.element_count(), 2, "{imp:?}");
    // Mit Bulge an beiden Punkten sind es zwei Bögen (Linse).
    let lens = "0 LWPOLYLINE\n90 2\n70 1\n10 0\n20 0\n42 1\n10 10\n20 0\n42 1";
    let imp = read("", lens);
    assert_eq!(imp.element_count(), 2);
    for e in &imp.layers[0].elements {
        assert_normal_arc(e);
    }
}

#[test]
fn layer_names_are_case_insensitive() {
    let ent = "0 LINE\n8 Wand\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0\n\
               0 LINE\n8 WAND\n10 0\n20 1\n30 0\n11 1\n21 1\n31 0\n\
               0 LINE\n8 Tuer\n10 0\n20 2\n30 0\n11 1\n21 2\n31 0\n\
               0 LINE\n8 wand\n10 0\n20 3\n30 0\n11 1\n21 3\n31 0";
    let imp = read("", ent);
    assert_eq!(imp.layers.len(), 2, "{imp:?}");
    assert_eq!(imp.layers[0].name, "Wand");
    assert_eq!(imp.layers[0].elements.len(), 3);
    assert_eq!(imp.layers[1].name, "Tuer");
}

#[test]
fn tilted_spline_and_point_are_three_d() {
    let spline = "0 SPLINE\n70 1064\n71 3\n74 2\n210 1\n220 0\n230 0\n\
                  11 0\n21 0\n31 0\n11 1\n21 2\n31 0";
    let imp = read("", spline);
    assert_eq!(
        (imp.element_count(), imp.skipped.three_d),
        (0, 1),
        "{imp:?}"
    );
    let point = "0 POINT\n10 1\n20 2\n30 0\n210 1\n220 0\n230 0";
    let imp = read("", point);
    assert_eq!(
        (imp.element_count(), imp.skipped.three_d),
        (0, 1),
        "{imp:?}"
    );
    // Mit Richtung -Z werden beide gespiegelt übernommen.
    let point = "0 POINT\n10 3\n20 4\n30 0\n210 0\n220 0\n230 -1";
    assert_eq!(
        only(&read("", point)),
        &Element2d::Point(DVec2::new(-3.0, 4.0))
    );
    let fit = "0 SPLINE\n70 1064\n71 3\n74 2\n210 0\n220 0\n230 -1\n\
               11 1\n21 0\n31 0\n11 2\n21 1\n31 0";
    match only(&read("", fit)) {
        Element2d::FitSpline(p) => {
            assert_eq!(p, &[DVec2::new(-1.0, 0.0), DVec2::new(-2.0, 1.0)])
        }
        e => panic!("kein Spline: {e:?}"),
    }
}

#[test]
fn mtext_paragraph_format_is_stripped_and_column_break_is_a_newline() {
    // `\pxi...;` ist ein Absatzformat und fällt weg; `\N` trennt Spalten und wird zum Umbruch.
    let ent = "0 MTEXT\n10 0\n20 0\n30 0\n40 2\n1 \\pxi-3,l3;Eins\\NZwei\\PDrei";
    match only(&read("", ent)) {
        Element2d::Text(t) => assert_eq!(t.text, "Eins\nZwei\nDrei"),
        e => panic!("kein Text: {e:?}"),
    }
}

/// Gesamtprüfung I3: Jede Längeneinheit aus `$INSUNITS` kommt mit ihrem mm-Faktor an; nur 0
/// („ohne Einheit“) ergibt `None`.
#[test]
fn every_insunits_length_unit_has_its_factor() {
    let cases: [(u32, Unit2d, f64); 24] = [
        (1, Unit2d::Inch, 25.4),
        (2, Unit2d::Foot, 304.8),
        (3, Unit2d::Mile, 1_609_344.0),
        (4, Unit2d::Millimeter, 1.0),
        (5, Unit2d::Centimeter, 10.0),
        (6, Unit2d::Meter, 1000.0),
        (7, Unit2d::Kilometer, 1e6),
        (8, Unit2d::Microinch, 2.54e-5),
        (9, Unit2d::Mil, 0.0254),
        (10, Unit2d::Yard, 914.4),
        (11, Unit2d::Angstrom, 1e-7),
        (12, Unit2d::Nanometer, 1e-6),
        (13, Unit2d::Micron, 1e-3),
        (14, Unit2d::Decimeter, 100.0),
        (15, Unit2d::Decameter, 1e4),
        (16, Unit2d::Hectometer, 1e5),
        (17, Unit2d::Gigameter, 1e12),
        (18, Unit2d::AstronomicalUnit, 1.495_978_707e14),
        (19, Unit2d::LightYear, 9.460_730_472_580_8e18),
        (20, Unit2d::Parsec, 3.085_677_581_491_367e19),
        (21, Unit2d::UsSurveyFoot, 304.800_609_6),
        (22, Unit2d::UsSurveyInch, 25.400_050_8),
        (23, Unit2d::UsSurveyYard, 914.401_828_8),
        (24, Unit2d::UsSurveyMile, 1_609_347.218_694),
    ];
    for (code, unit, mm) in cases {
        let imp = read(&format!("9 $INSUNITS\n70 {code}"), "0 POINT\n10 1\n20 2");
        assert_eq!(imp.units, Some(unit), "$INSUNITS {code}");
        let f = unit.mm();
        assert!((f - mm).abs() <= mm * 1e-9, "{code}: {f} statt {mm}");
    }
    assert_eq!(read("9 $INSUNITS\n70 0", "0 POINT\n10 1\n20 2").units, None);
}
