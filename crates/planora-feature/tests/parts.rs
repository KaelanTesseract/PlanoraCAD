//! Stückliste im Dokument: stabile Körperkennung, Zeilen, Zelltexte, eigene Zeilentexte
//! (Teilprojekt 3c-2, Task 2).

use planora_drawing::sheet_info::{FieldValue, NotesValue, ProjectInfo};
use planora_drawing::{BodyKey, PartsRow, RowOverride};
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{BoolOp, Edge, Id, Solid};
use planora_math::{DVec3, Tolerance};
use std::sync::Arc;

fn bx_at(size: DVec3, origin: DVec3) -> FeatureKind {
    FeatureKind::Box { size, origin }
}

fn cube(d: &mut Document, name: &str) -> FeatureId {
    d.add_feature(name, bx_at(DVec3::splat(10.0), DVec3::ZERO))
        .unwrap()
}

fn body(f: FeatureId) -> BodyRef {
    BodyRef {
        feature: f,
        index: 0,
    }
}

fn key(f: FeatureId) -> BodyKey {
    BodyKey {
        feature: f.0,
        index: 0,
    }
}

fn row(pos: u32, name: &str, qty: u32, keys: Vec<BodyKey>) -> PartsRow {
    PartsRow {
        pos,
        name: name.to_string(),
        qty,
        keys,
    }
}

fn name_override(t: &str) -> RowOverride {
    RowOverride {
        name: FieldValue::Text(t.to_string()),
        ..RowOverride::default()
    }
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn edge_ref(doc: &Document, r: BodyRef, a: &str, b: &str) -> EdgeRef {
    let s = doc.body(r).unwrap();
    EdgeRef::from_edge(s, edge_between(s, a, b), &Tolerance::default()).unwrap()
}

fn project_with_material(m: FieldValue) -> ProjectInfo {
    ProjectInfo {
        login: "dennis".into(),
        owner: FieldValue::Auto,
        creator: FieldValue::Auto,
        checked: FieldValue::Auto,
        approved: FieldValue::Auto,
        material: m,
        tolerance: FieldValue::Auto,
        notes: NotesValue::Auto,
    }
}

#[test]
fn identity_survives_move_fillet_boolean() {
    let mut d = Document::new();
    let a = cube(&mut d, "Platte");
    let b = d
        .add_feature(
            "Stift",
            bx_at(DVec3::new(2.0, 2.0, 2.0), DVec3::new(30.0, 0.0, 0.0)),
        )
        .unwrap();
    let moved = d
        .add_feature(
            "Verschieben",
            FeatureKind::Move {
                body: body(a),
                delta: DVec3::new(5.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let edge = edge_ref(&d, body(moved), "Deckel", "Wand 1");
    let rounded = d
        .add_feature(
            "Rundung",
            FeatureKind::Verrunden {
                body: body(moved),
                edges: vec![edge],
                art: BlendArt::Rundung { radius: 1.0 },
                tangentenkette: true,
            },
        )
        .unwrap();
    let joined = d
        .add_feature(
            "Vereinigung",
            FeatureKind::Boolean {
                target: body(rounded),
                tool: body(b),
                op: BoolOp::Union,
            },
        )
        .unwrap();
    assert_eq!(d.feature(joined).unwrap().status, FeatureStatus::Ok);
    let result = body(joined);
    assert_eq!(
        d.body_key(result),
        BodyKey {
            feature: a.0,
            index: 0
        }
    );
    // Auch jede Zwischenstufe führt zum Ursprung.
    assert_eq!(d.body_key(body(moved)), key(a));
    assert_eq!(d.body_key(body(rounded)), key(a));
    // Verschieben und Verrunden benennen den Körper nicht um; die Boolesche Operation schon.
    assert_eq!(d.body_display_name(body(moved)).as_deref(), Some("Platte"));
    assert_eq!(
        d.body_display_name(body(rounded)).as_deref(),
        Some("Platte")
    );
    assert_eq!(d.body_display_name(result).as_deref(), Some("Vereinigung"));
    // Das Werkzeug ist nach der Operation kein eigener Körper mehr.
    assert_eq!(d.row_of(key(b)), None);
    assert_eq!(
        d.parts_rows(),
        vec![row(
            1,
            &d.body_display_name(result).unwrap(),
            1,
            vec![key(a)]
        )]
    );
}

#[test]
fn rename_and_reorder_keep_keys() {
    let mut d = Document::new();
    let f1 = cube(&mut d, "Eins");
    let f2 = cube(&mut d, "Zwei");
    let f3 = cube(&mut d, "Drei");
    assert_eq!(d.row_of(key(f3)), Some(3));
    d.rename(f2, "Mitte").unwrap();
    assert_eq!(d.body_key(body(f2)), key(f2));
    assert_eq!(
        d.parts_rows(),
        vec![
            row(1, "Eins", 1, vec![key(f1)]),
            row(2, "Mitte", 1, vec![key(f2)]),
            row(3, "Drei", 1, vec![key(f3)]),
        ]
    );
    d.delete(f1).unwrap();
    assert_eq!(d.body_key(body(f3)), key(f3));
    assert_eq!(d.row_of(key(f3)), Some(2));
    assert_eq!(d.row_of(key(f1)), None);
}

#[test]
fn grouping_and_quantity() {
    let mut d = Document::new();
    let k1 = cube(&mut d, "Bolzen");
    let k2 = cube(&mut d, "Platte");
    let k3 = cube(&mut d, "Bolzen");
    assert_eq!(
        d.parts_rows(),
        vec![
            row(1, "Bolzen", 2, vec![key(k1), key(k3)]),
            row(2, "Platte", 1, vec![key(k2)]),
        ]
    );
    assert_eq!(d.row_of(key(k3)), Some(1));
    assert_eq!(d.row_of(key(k2)), Some(2));
    // Menge steht in den Zellen.
    let cells = d.parts_cells();
    assert_eq!(cells[0][2], "2");
    assert_eq!(cells[1][2], "1");
}

#[test]
fn meshes_and_suppressed_are_not_rows() {
    let mut d = Document::new();
    let a = cube(&mut d, "Sichtbar");
    let s = cube(&mut d, "Weg");
    let mesh = Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    });
    let m = d
        .add_mesh_body("Netz", mesh, MeshTransform::IDENTITY)
        .unwrap();
    d.set_suppressed(s, true).unwrap();
    assert_eq!(d.parts_rows(), vec![row(1, "Sichtbar", 1, vec![key(a)])]);
    assert_eq!(d.row_of(key(m)), None);
    assert_eq!(d.row_of(key(s)), None);
}

#[test]
fn overrides_by_key() {
    let mut d = Document::new();
    let a = cube(&mut d, "Platte");
    let model_edits = d.model_edit_count();
    assert_eq!(d.edit_count(), d.model_edit_count());
    let before = d.edit_count();

    d.set_parts_override(key(a), name_override("Grundplatte"))
        .unwrap();
    assert_eq!(d.edit_count(), before + 1);
    assert_eq!(d.model_edit_count(), model_edits);
    assert_eq!(d.parts_cells()[0][1], "Grundplatte");
    // Gleicher Wert zählt nicht.
    d.set_parts_override(key(a), name_override("Grundplatte"))
        .unwrap();
    assert_eq!(d.edit_count(), before + 1);
    // Der Text hängt an der Kennung, nicht am Namen.
    d.rename(a, "Anders").unwrap();
    assert_eq!(d.parts_cells()[0][1], "Grundplatte");
    assert_eq!(d.parts_rows()[0].name, "Anders");
    // Neuer Wert zählt.
    let edits = d.edit_count();
    d.set_parts_override(key(a), name_override("Sockel"))
        .unwrap();
    assert_eq!(d.edit_count(), edits + 1);
    assert_eq!(d.parts_cells()[0][1], "Sockel");
    // Alles automatisch: Eintrag fällt weg.
    let edits = d.edit_count();
    d.set_parts_override(key(a), RowOverride::default())
        .unwrap();
    assert!(d.parts_overrides().is_empty());
    assert_eq!(d.edit_count(), edits + 1);
    assert_eq!(d.parts_cells()[0][1], "Anders");
    // Nochmals automatisch ohne Eintrag: keine Änderung.
    d.set_parts_override(key(a), RowOverride::default())
        .unwrap();
    assert_eq!(d.edit_count(), edits + 1);

    // Zu lang: Fehler, Dokument unverändert.
    d.set_parts_override(key(a), name_override("Alu")).unwrap();
    let edits = d.edit_count();
    let overrides = d.parts_overrides().clone();
    let long = "x".repeat(61);
    assert!(d.set_parts_override(key(a), name_override(&long)).is_err());
    assert_eq!(d.edit_count(), edits);
    assert_eq!(d.parts_overrides(), &overrides);
    let ok60 = "x".repeat(60);
    d.set_parts_override(key(a), name_override(&ok60)).unwrap();

    // Höchstens 1000 Schlüssel.
    let mut d = Document::new();
    for i in 0..1000u64 {
        d.set_parts_override(
            BodyKey {
                feature: i + 1,
                index: 0,
            },
            name_override("A"),
        )
        .unwrap();
    }
    let edits = d.edit_count();
    let extra = BodyKey {
        feature: 5000,
        index: 0,
    };
    assert!(d.set_parts_override(extra, name_override("A")).is_err());
    assert_eq!(d.parts_overrides().len(), 1000);
    assert_eq!(d.edit_count(), edits);
    // Ein vorhandener Schlüssel lässt sich weiter ändern.
    d.set_parts_override(
        BodyKey {
            feature: 1,
            index: 0,
        },
        name_override("B"),
    )
    .unwrap();
}

#[test]
fn cells_defaults() {
    let mut d = Document::new();
    let a = cube(&mut d, "Platte");
    assert_eq!(
        d.parts_cells(),
        vec![["1", "Platte", "1", "–", ""].map(String::from)]
    );
    d.set_project_info(project_with_material(FieldValue::Text("S235".into())))
        .unwrap();
    assert_eq!(d.parts_cells()[0][3], "S235");
    // Projektangabe automatisch: weiter „–“.
    d.set_project_info(project_with_material(FieldValue::Auto))
        .unwrap();
    assert_eq!(d.parts_cells()[0][3], "–");
    d.set_project_info(project_with_material(FieldValue::Text("S235".into())))
        .unwrap();
    d.set_parts_override(
        key(a),
        RowOverride {
            material: FieldValue::Text("Alu".into()),
            note: FieldValue::Text("lackiert".into()),
            qty: FieldValue::Text("2 Stk.".into()),
            ..RowOverride::default()
        },
    )
    .unwrap();
    assert_eq!(
        d.parts_cells(),
        vec![["1", "Platte", "2 Stk.", "Alu", "lackiert"].map(String::from)]
    );
}

#[test]
fn boolean_split_has_own_key() {
    // Der Kern liefert heute je Boolescher Operation einen Körper; die Kennung muss aber auch
    // für weitere Ergebnisse (Index >= 1) eindeutig sein, ohne dass sie schon vorkommen.
    let mut d = Document::new();
    let plate = cube(&mut d, "Platte");
    let tool = d
        .add_feature(
            "Nut",
            bx_at(DVec3::new(4.0, 30.0, 10.0), DVec3::new(8.0, -5.0, -2.0)),
        )
        .unwrap();
    let cut = d
        .add_feature(
            "Schnitt",
            FeatureKind::Boolean {
                target: body(plate),
                tool: body(tool),
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    assert_eq!(
        d.body_key(BodyRef {
            feature: cut,
            index: 1
        }),
        BodyKey {
            feature: cut.0,
            index: 1
        }
    );
    assert_eq!(
        d.body_key(BodyRef {
            feature: cut,
            index: 2
        }),
        BodyKey {
            feature: cut.0,
            index: 2
        }
    );
    // Index 0 folgt dem Ziel.
    assert_eq!(d.body_key(body(cut)), key(plate));
}

#[test]
fn snapshot_round_trip_keeps_overrides() {
    let mut d = Document::new();
    let a = cube(&mut d, "Platte");
    d.set_parts_override(key(a), name_override("Grundplatte"))
        .unwrap();
    let snap = d.snapshot();
    assert_eq!(snap.parts_overrides.len(), 1);
    let loaded = Document::from_snapshot(snap.clone(), 0.05).unwrap();
    assert_eq!(loaded.edit_count(), 0);
    assert_eq!(loaded.parts_overrides(), d.parts_overrides());
    assert_eq!(loaded.parts_cells(), d.parts_cells());

    let mut bad = snap.clone();
    bad.parts_overrides
        .insert(key(a), name_override(&"x".repeat(61)));
    assert!(Document::from_snapshot(bad, 0.05).is_err());

    let mut too_many = snap;
    for i in 0..1001u64 {
        too_many.parts_overrides.insert(
            BodyKey {
                feature: 100 + i,
                index: 0,
            },
            name_override("A"),
        );
    }
    assert!(Document::from_snapshot(too_many, 0.05).is_err());
}

#[test]
fn suppress_and_restore() {
    let mut d = Document::new();
    let a = cube(&mut d, "Platte");
    let b = cube(&mut d, "Zweite");
    d.set_parts_override(key(a), name_override("Grundplatte"))
        .unwrap();
    assert_eq!(d.parts_cells()[0][1], "Grundplatte");
    d.set_suppressed(a, true).unwrap();
    assert_eq!(d.row_of(key(a)), None);
    assert_eq!(d.row_of(key(b)), Some(1));
    assert_eq!(d.parts_overrides().len(), 1, "Eintrag bleibt");
    d.set_suppressed(a, false).unwrap();
    assert_eq!(d.row_of(key(a)), Some(1));
    assert_eq!(d.parts_cells()[0][1], "Grundplatte");
}
