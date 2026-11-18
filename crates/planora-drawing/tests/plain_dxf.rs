//! Aufgabe 4 (Skizzen 11b): DXF-Schreiber für ebene Modelle. Gelesen wird mit dem Leser des
//! Austauschpakets (`planora_exchange::read_dxf`), die Sollwerte stehen von Hand fest.

use planora_drawing::model::{Paper, Sheet};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::{SheetLine, SheetModel, SheetView};
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{ViewId, Visibility, dxf_string};
use planora_math::DVec2;

fn p(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Blatt mit zwei Ansichten, sichtbaren und verdeckten Linien und allen Kurvenarten; Grundlage
/// des Bytevergleichs mit der Ausgabe vor dem Umbau (`tests/data/sheet_before.dxf`).
fn sheet_model() -> SheetModel {
    let line = |vis, curve| SheetLine {
        vis,
        curve,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    };
    let view = |i: u64, name: &str, lines: Vec<SheetLine>| SheetView {
        id: ViewId(i),
        name: name.to_string(),
        lines,
        extent_mm: (0.0, 0.0),
        marks: Vec::new(),
    };
    SheetModel {
        sheet: Sheet {
            paper: Paper::A3,
            landscape: true,
        },
        frame: None,
        views: vec![
            view(
                1,
                "Vorne",
                vec![
                    line(
                        Visibility::Visible,
                        Out2d::Line {
                            a: p(10.0, 20.0),
                            b: p(60.5, 20.25),
                        },
                    ),
                    line(
                        Visibility::Hidden,
                        Out2d::Line {
                            a: p(10.0, 30.0),
                            b: p(60.0, 30.0),
                        },
                    ),
                    line(
                        Visibility::Visible,
                        Out2d::Arc {
                            c: p(100.0, 100.0),
                            r: 12.5,
                            start: 0.25,
                            end: 2.0,
                        },
                    ),
                ],
            ),
            view(
                2,
                "Oben",
                vec![
                    line(
                        Visibility::Visible,
                        Out2d::Ellipse {
                            c: p(200.0, 150.0),
                            major: p(20.0, 0.0),
                            ratio: 0.5,
                            start: 0.0,
                            end: std::f64::consts::TAU,
                        },
                    ),
                    line(
                        Visibility::Visible,
                        Out2d::Spline {
                            degree: 3,
                            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
                            ctrl: vec![p(0.0, 0.0), p(10.0, 30.0), p(30.0, 30.0), p(40.0, 0.0)],
                            weights: vec![1.0; 4],
                        },
                    ),
                ],
            ),
        ],
    }
}

/// Einmalig vor dem Umbau: `cargo test -p planora-drawing --test plain_dxf -- --ignored bless`.
#[test]
#[ignore]
fn bless_sheet_before() {
    let text = dxf_string(&sheet_model()).unwrap();
    std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/sheet_before.dxf"),
        text,
    )
    .unwrap();
}

#[test]
fn sheet_dxf_unchanged() {
    let want = include_bytes!("data/sheet_before.dxf");
    let got = dxf_string(&sheet_model()).unwrap();
    assert_eq!(got.as_bytes(), want.as_slice());
}

// ---------------------------------------------------------------------------------------------
// Ebene Modelle
// ---------------------------------------------------------------------------------------------

use planora_drawing::DrawError;
use planora_drawing::plain_dxf::{
    DxfUnit, PlainDxf, PlainEntity, PlainLayer, plain_dxf_string, write_plain_dxf,
};
use planora_exchange::import2d::{Element2d, Import2d, Unit2d};
use planora_exchange::read_dxf;

fn layer(name: &str) -> PlainLayer {
    PlainLayer {
        name: name.to_string(),
        color: 7,
    }
}

fn model(unit: DxfUnit, entities: Vec<(usize, PlainEntity)>) -> PlainDxf {
    PlainDxf {
        unit,
        layers: vec![layer("0")],
        entities,
    }
}

fn read(m: &PlainDxf) -> (String, Import2d) {
    let text = plain_dxf_string(m).expect("DXF sollte sich schreiben lassen");
    let imp = read_dxf(text.as_bytes(), &|| false).expect("Leser sollte die Datei annehmen");
    (text, imp)
}

fn all(imp: &Import2d) -> Vec<&Element2d> {
    imp.layers.iter().flat_map(|l| &l.elements).collect()
}

fn near(a: DVec2, b: DVec2) -> bool {
    (a - b).length() < 1e-9
}

fn unit_line(unit: DxfUnit) -> PlainDxf {
    model(
        unit,
        vec![(
            0,
            PlainEntity::Curve(Out2d::Line {
                a: p(0.0, 0.0),
                b: p(1.0, 0.0),
            }),
        )],
    )
}

#[test]
fn inch_file_reads_back_in_inch() {
    let (_, imp) = read(&unit_line(DxfUnit::Inch));
    assert_eq!(imp.units, Some(Unit2d::Inch));
    let els = all(&imp);
    assert_eq!(els.len(), 1);
    let Element2d::Line(a, b) = els[0] else {
        panic!("Linie erwartet: {:?}", els[0])
    };
    assert!(((*b - *a).length() - 1.0).abs() < 1e-9);
}

#[test]
fn mm_cm_m_ft_insunits() {
    for (unit, code, want) in [
        (DxfUnit::Millimeter, 4, Unit2d::Millimeter),
        (DxfUnit::Centimeter, 5, Unit2d::Centimeter),
        (DxfUnit::Meter, 6, Unit2d::Meter),
        (DxfUnit::Foot, 2, Unit2d::Foot),
    ] {
        let (text, imp) = read(&unit_line(unit));
        assert!(
            text.contains(&format!("$INSUNITS\r\n 70\r\n{code}\r\n")),
            "{unit:?}"
        );
        assert_eq!(imp.units, Some(want), "{unit:?}");
        assert_eq!(unit.insunits(), code);
    }
    assert_eq!(DxfUnit::Inch.insunits(), 1);
    let mm = [1.0, 10.0, 1000.0, 25.4, 304.8];
    let units = [
        DxfUnit::Millimeter,
        DxfUnit::Centimeter,
        DxfUnit::Meter,
        DxfUnit::Inch,
        DxfUnit::Foot,
    ];
    for (u, f) in units.into_iter().zip(mm) {
        assert_eq!(u.mm_per_unit(), f, "{u:?}");
    }
    // $MEASUREMENT: metrisch 1, Zoll und Fuß 0.
    for (u, m) in [
        (DxfUnit::Millimeter, 1),
        (DxfUnit::Centimeter, 1),
        (DxfUnit::Meter, 1),
        (DxfUnit::Inch, 0),
        (DxfUnit::Foot, 0),
    ] {
        let text = plain_dxf_string(&unit_line(u)).unwrap();
        assert!(
            text.contains(&format!("$MEASUREMENT\r\n 70\r\n{m}\r\n")),
            "{u:?}"
        );
        assert_eq!(u.metric(), m == 1);
    }
}

#[test]
fn point_arc_ellipse_spline_roundtrip() {
    let arc = Out2d::Arc {
        c: p(5.0, -3.0),
        r: 2.5,
        start: 0.5,
        end: 2.25,
    };
    let ellipse = Out2d::Ellipse {
        c: p(-4.0, 7.0),
        major: p(3.0, 4.0),
        ratio: 0.5,
        start: 0.0,
        end: std::f64::consts::TAU,
    };
    let knots = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
    let ctrl = vec![p(0.0, 0.0), p(10.0, 30.0), p(30.0, 30.0), p(40.0, 0.0)];
    let spline = Out2d::Spline {
        degree: 3,
        knots: knots.clone(),
        ctrl: ctrl.clone(),
        weights: vec![1.0; 4],
    };
    let m = model(
        DxfUnit::Millimeter,
        vec![
            (0, PlainEntity::Point(p(1.25, 2.5))),
            (0, PlainEntity::Curve(arc)),
            (0, PlainEntity::Curve(ellipse)),
            (0, PlainEntity::Curve(spline)),
        ],
    );
    let (text, imp) = read(&m);
    assert!(text.contains("  0\r\nPOINT\r\n"));
    assert!(text.contains("AcDbPoint"));
    let els = all(&imp);
    assert_eq!(els.len(), 4, "{els:?}");
    let mut seen = [false; 4];
    for e in els {
        match e {
            Element2d::Point(q) => {
                assert!(near(*q, p(1.25, 2.5)));
                seen[0] = true;
            }
            Element2d::Arc { c, r, start, end } => {
                assert!(near(*c, p(5.0, -3.0)) && (r - 2.5).abs() < 1e-9);
                assert!((start - 0.5).abs() < 1e-9 && (end - 2.25).abs() < 1e-9);
                seen[1] = true;
            }
            Element2d::Ellipse {
                c,
                major,
                ratio,
                start,
                end,
            } => {
                assert!(near(*c, p(-4.0, 7.0)) && near(*major, p(3.0, 4.0)));
                assert!((ratio - 0.5).abs() < 1e-9);
                assert!(start.abs() < 1e-9 && (end - std::f64::consts::TAU).abs() < 1e-9);
                seen[2] = true;
            }
            Element2d::Nurbs {
                degree,
                knots: k,
                points,
                ..
            } => {
                assert_eq!(*degree, 3);
                assert_eq!(k.len(), knots.len());
                assert!(k.iter().zip(&knots).all(|(a, b)| (a - b).abs() < 1e-9));
                assert!(points.iter().zip(&ctrl).all(|(a, b)| near(*a, *b)));
                seen[3] = true;
            }
            other => panic!("unerwartet: {other:?}"),
        }
    }
    assert_eq!(seen, [true; 4]);
}

#[test]
fn layers_by_name() {
    let m = PlainDxf {
        unit: DxfUnit::Millimeter,
        layers: vec![layer("0"), layer("Konstruktion")],
        entities: vec![
            (
                0,
                PlainEntity::Curve(Out2d::Line {
                    a: p(0.0, 0.0),
                    b: p(1.0, 0.0),
                }),
            ),
            (1, PlainEntity::Point(p(2.0, 3.0))),
        ],
    };
    let (_, imp) = read(&m);
    let find = |n: &str| imp.layers.iter().find(|l| l.name == n).expect(n);
    assert!(matches!(find("0").elements[..], [Element2d::Line(..)]));
    assert!(matches!(
        find("Konstruktion").elements[..],
        [Element2d::Point(_)]
    ));
}

#[test]
fn extents_come_from_the_objects() {
    let m = model(
        DxfUnit::Centimeter,
        vec![(
            0,
            PlainEntity::Curve(Out2d::Line {
                a: p(1.0, 2.0),
                b: p(4.0, 6.0),
            }),
        )],
    );
    let text = plain_dxf_string(&m).unwrap();
    assert!(text.contains("$EXTMIN\r\n 10\r\n1\r\n 20\r\n2\r\n 30\r\n0\r\n"));
    assert!(text.contains("$EXTMAX\r\n 10\r\n4\r\n 20\r\n6\r\n 30\r\n0\r\n"));
}

#[test]
fn empty_model_is_error() {
    let m = model(DxfUnit::Millimeter, vec![]);
    assert!(plain_dxf_string(&m).is_err());
    let mut sink = Vec::new();
    assert!(write_plain_dxf(&m, &mut sink).is_err());
    assert!(sink.is_empty(), "bei einem Fehler wird nichts geschrieben");
}

#[test]
fn bad_input_is_refused() {
    // Nicht endliche Zahl.
    let m = model(
        DxfUnit::Millimeter,
        vec![(0, PlainEntity::Point(p(f64::NAN, 0.0)))],
    );
    assert!(matches!(plain_dxf_string(&m), Err(DrawError::NonFinite)));
    // Ebenenindex außerhalb.
    let m = model(
        DxfUnit::Millimeter,
        vec![(5, PlainEntity::Point(p(0.0, 0.0)))],
    );
    assert!(plain_dxf_string(&m).is_err());
}

/// Die Namen der LAYER-Tabelle in der Reihenfolge der Datei.
fn layer_table_names(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.split("\r\n").collect();
    let start = lines
        .windows(2)
        .position(|w| w[0].trim() == "2" && w[1] == "LAYER")
        .expect("LAYER-Tabelle");
    let mut names = Vec::new();
    // Gruppencode und Wert wechseln sich ab.
    let mut i = start + 2;
    while i + 1 < lines.len() && lines[i + 1] != "ENDTAB" {
        if lines[i].trim() == "2" {
            names.push(lines[i + 1].to_string());
        }
        i += 2;
    }
    names
}

fn one_point(layers: Vec<PlainLayer>, layer: usize) -> PlainDxf {
    PlainDxf {
        unit: DxfUnit::Millimeter,
        layers,
        entities: vec![(layer, PlainEntity::Point(p(1.0, 2.0)))],
    }
}

#[test]
fn layer_color_outside_1_to_255_is_refused() {
    for color in [-7, 0, 256, i16::MAX, i16::MIN] {
        let m = one_point(
            vec![PlainLayer {
                name: "A".into(),
                color,
            }],
            0,
        );
        match plain_dxf_string(&m) {
            Err(DrawError::Unsupported(msg)) => {
                assert_eq!(msg, planora_i18n::tr!("drawing-err-plain-color"), "{color}")
            }
            other => panic!("Farbe {color} sollte abgelehnt werden: {other:?}"),
        }
    }
    // Die Grenzen selbst sind gültig.
    for color in [1, 7, 255] {
        let m = one_point(
            vec![PlainLayer {
                name: "A".into(),
                color,
            }],
            0,
        );
        plain_dxf_string(&m).unwrap_or_else(|e| panic!("Farbe {color}: {e:?}"));
    }
}

#[test]
fn layer_zero_is_always_first_in_the_layer_table() {
    // Der Aufrufer nennt „0“ erst an dritter Stelle.
    let m = one_point(vec![layer("A"), layer("B"), layer("0")], 2);
    let text = plain_dxf_string(&m).unwrap();
    assert_eq!(layer_table_names(&text), ["0", "A", "B"]);
    // Auch bei Groß-/Kleinschreibung eindeutig und ohne „0“ vorn ergänzt.
    let m = one_point(vec![layer("A")], 0);
    let text = plain_dxf_string(&m).unwrap();
    assert_eq!(layer_table_names(&text), ["0", "A"]);
    // Das Objekt auf der späten Ebene „0“ landet auch auf „0“.
    let m = one_point(vec![layer("A"), layer("0")], 1);
    let (_, imp) = read(&m);
    let zero = imp.layers.iter().find(|l| l.name == "0").unwrap();
    assert!(matches!(zero.elements[..], [Element2d::Point(_)]));
}
