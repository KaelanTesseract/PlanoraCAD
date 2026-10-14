//! 3MF: Paket, wohlgeformtes XML, Objekte, Punkte und Dreiecke (Teilprojekt 9, Task 7).

mod common;

use common::{XmlElement, parse_xml, read_zip};
use planora_exchange::*;

/// Einheitswürfel, um `shift` in x verschoben.
fn cube(shift: f32) -> ExchangeMesh {
    let mut m = ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    };
    for p in &mut m.positions {
        p[0] += shift;
    }
    m
}

fn bodies() -> [ExportBody; 2] {
    [
        ExportBody {
            name: "Würfel <1> & \"Co\"".into(),
            mesh: cube(0.0),
        },
        ExportBody {
            name: "Zweiter".into(),
            mesh: cube(5.0),
        },
    ]
}

fn write(bodies: &[ExportBody]) -> Vec<u8> {
    let mut out = Vec::new();
    write_3mf(&mut out, bodies).unwrap();
    out
}

fn model_of(bytes: &[u8]) -> Vec<XmlElement> {
    let entries = read_zip(bytes).unwrap();
    let model = entries
        .into_iter()
        .find(|e| e.name == "3D/3dmodel.model")
        .unwrap();
    parse_xml(&String::from_utf8(model.data).unwrap()).unwrap()
}

fn attr<'a>(e: &'a XmlElement, key: &str) -> &'a str {
    &e.attrs.iter().find(|(k, _)| k == key).unwrap().1
}

#[test]
fn the_package_has_the_three_parts_and_valid_xml() {
    let out = write(&bodies());
    let entries = read_zip(&out).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"]
    );
    for e in &entries {
        parse_xml(std::str::from_utf8(&e.data).unwrap())
            .unwrap_or_else(|m| panic!("{}: {m}", e.name));
    }
    let rels = parse_xml(std::str::from_utf8(&entries[1].data).unwrap()).unwrap();
    let rel = rels.iter().find(|x| x.name == "Relationship").unwrap();
    assert_eq!(attr(rel, "Target"), "/3D/3dmodel.model");
    let types = parse_xml(std::str::from_utf8(&entries[0].data).unwrap()).unwrap();
    assert!(
        types
            .iter()
            .any(|x| x.name == "Default" && attr(x, "Extension") == "model")
    );
}

#[test]
fn objects_items_points_and_triangles_match_the_bodies() {
    let xml = model_of(&write(&bodies()));
    let root = &xml[0];
    assert_eq!(root.name, "model");
    assert_eq!(attr(root, "unit"), "millimeter");
    let objects: Vec<&XmlElement> = xml.iter().filter(|e| e.name == "object").collect();
    assert_eq!(objects.len(), 2);
    assert_eq!(attr(objects[0], "type"), "model");
    assert_eq!(attr(objects[0], "name"), "Würfel <1> & \"Co\"");
    let items: Vec<&str> = xml
        .iter()
        .filter(|e| e.name == "item")
        .map(|e| attr(e, "objectid"))
        .collect();
    assert_eq!(items, ["1", "2"]);
    let vertices: Vec<&XmlElement> = xml.iter().filter(|e| e.name == "vertex").collect();
    assert_eq!(vertices.len(), 16);
    assert_eq!(xml.iter().filter(|e| e.name == "triangle").count(), 24);
    // zweiter Körper verschoben; Punkte mit sechs Nachkommastellen
    assert_eq!(attr(vertices[9], "x"), "6.000000");
    for v in &vertices {
        for k in ["x", "y", "z"] {
            let s = attr(v, k);
            assert_eq!(s.len() - s.find('.').unwrap() - 1, 6, "{s}");
        }
    }
    for t in xml.iter().filter(|e| e.name == "triangle") {
        for k in ["v1", "v2", "v3"] {
            assert!(attr(t, k).parse::<u32>().unwrap() < 8);
        }
    }
}

#[test]
fn empty_bodies_are_left_out_and_nothing_is_an_error() {
    let mut out = Vec::new();
    assert_eq!(write_3mf(&mut out, &[]), Err(ExchangeError::Empty));
    let b = [
        ExportBody {
            name: "leer".into(),
            mesh: ExchangeMesh::default(),
        },
        ExportBody {
            name: "voll".into(),
            mesh: cube(0.0),
        },
    ];
    let xml = model_of(&write(&b));
    assert_eq!(xml.iter().filter(|e| e.name == "object").count(), 1);
    assert_eq!(xml.iter().filter(|e| e.name == "item").count(), 1);
}

#[test]
fn control_characters_in_names_do_not_break_the_xml() {
    let b = [ExportBody {
        name: "a\u{1}b\nc".into(),
        mesh: cube(0.0),
    }];
    let xml = model_of(&write(&b));
    let object = xml.iter().find(|e| e.name == "object").unwrap();
    assert_eq!(attr(object, "name"), "a b c");
}
