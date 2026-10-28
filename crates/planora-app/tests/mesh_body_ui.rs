//! Netz-Körper im Fenster: Eigenschaften, gesperrte Menüpunkte, Körperwahl, Symbol
//! (Teilprojekt 9, Task 13).

use planora_app::boolean_dialog::BooleanDialog;
use planora_app::icons::FeatureIcon;
use planora_app::state::*;
use planora_app::{fonts, properties};
use planora_exchange::ExchangeMesh;
use planora_kernel::boolean::BoolOp;
use planora_math::DVec3;
use std::sync::Arc;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
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
    }
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

#[test]
fn the_properties_of_a_mesh_body_show_measures_transform_and_report() {
    let ctx = context();
    let mut s = AppState::new();
    let mut open = cube();
    open.indices.truncate(33);
    s.import_mesh("Offen", Arc::new(open), 1.0).unwrap();
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| properties::show(ui, &mut s));
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    for word in [
        "Netz-Körper",
        "Volumen",
        "Abmessungen",
        "Position",
        "Drehung",
        "Maßstab",
        "Dreiecke",
        "Geschlossen",
        "nein",
        "3 offene Kanten",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    assert_eq!(s.document.features().len(), 1, "Zeichnen ändert nichts");
    // Feature gewählt: Parameter des Netz-Körpers
    let id = s.document.features()[0].id;
    let kind = s.document.feature(id).unwrap().kind.clone();
    let rows = properties::parameter_rows(&s, &kind);
    let labels: Vec<&str> = rows.iter().map(|(l, _)| *l).collect();
    assert_eq!(labels, ["Netz", "Position", "Drehung", "Maßstab"]);
    assert_eq!(rows[0].1, "11 Dreiecke");
    assert_eq!(FeatureIcon::of(&kind), FeatureIcon::Body(BodyKind::Mesh));
}

#[test]
fn the_edit_menu_rules_name_the_lock() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_box(DVec3::splat(5.0)).unwrap();
    assert_eq!(s.boolean_blocked(), None);
    s.import_mesh("N", Arc::new(cube()), 1.0).unwrap(); // jetzt gewählt
    assert_eq!(s.boolean_blocked(), Some(MESH_LOCKED_HINT.to_string()));
    assert_eq!(s.blend_blocked(), Some(MESH_LOCKED_HINT.to_string()));
    assert_eq!(
        s.sketch_on_face_blocked(),
        Some(MESH_LOCKED_HINT.to_string())
    );
    s.selected_body = Some(0);
    assert_eq!(s.boolean_blocked(), None);
    assert_eq!(s.blend_blocked(), None);
    assert!(s.sketch_on_face_blocked().unwrap().contains("ebene Fläche"));
    let mut only_mesh = AppState::new();
    only_mesh.import_mesh("N", Arc::new(cube()), 1.0).unwrap();
    only_mesh.selected_body = None;
    assert!(only_mesh.boolean_blocked().unwrap().contains("zwei Körper"));
    assert!(only_mesh.blend_blocked().unwrap().contains("ein Körper"));
}

#[test]
fn the_boolean_dialog_offers_only_ordinary_bodies() {
    let mut s = AppState::new();
    s.import_mesh("N", Arc::new(cube()), 1.0).unwrap();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_box(DVec3::splat(5.0)).unwrap();
    s.selected_body = s.bodies.iter().position(|b| b.kind == BodyKind::Mesh);
    let d = BooleanDialog::with_op(&s, BoolOp::Union);
    assert!(
        !s.is_mesh(d.a) && !s.is_mesh(d.b) && d.a != d.b,
        "{} {}",
        d.a,
        d.b
    );
    let ctx = context();
    let mut d = d;
    ctx.run_ui(egui::RawInput::default(), |ui| {
        assert!(d.show(ui.ctx(), &mut s));
    })
    .drop_without_applying_deltas();
}
