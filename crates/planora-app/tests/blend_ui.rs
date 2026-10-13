//! Fenster ohne Bildschirm (Teilprojekt 8, Task 8).

use planora_app::blend_dialog::BlendDialog;
use planora_app::feature_dialog::FeatureDialog;
use planora_app::state::*;
use planora_app::{fonts, properties};
use planora_feature::BlendArt;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_math::DVec3;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

#[test]
fn the_blend_dialog_draws_and_changes_nothing_without_input() {
    let ctx = context();
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    for mut d in [BlendDialog::fillet(), BlendDialog::chamfer()] {
        for _ in 0..3 {
            let mut open = false;
            ctx.run_ui(egui::RawInput::default(), |ui| {
                open = d.show(ui.ctx(), &mut s)
            })
            .drop_without_applying_deltas();
            assert!(open);
        }
    }
    assert_eq!(s.document.features().len(), 1);
}

fn blended_state(art: BlendArt) -> (AppState, planora_feature::FeatureId) {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let solid = &s.bodies[0].solid;
    let e = solid
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(solid, id)
                .iter()
                .map(|u| face_name(solid, u.face))
                .collect();
            n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
        })
        .unwrap();
    let id = s.add_blend(0, &[e], art, false).unwrap();
    (s, id)
}

#[test]
fn the_properties_show_the_blend_parameters() {
    let ctx = context();
    let (mut s, id) = blended_state(BlendArt::FaseWinkel {
        abstand: 1.0,
        winkel: 30.0,
    });
    let kind = s.document.feature(id).unwrap().kind.clone();
    let rows = properties::parameter_rows(&s, &kind);
    let labels: Vec<&str> = rows.iter().map(|(l, _)| *l).collect();
    assert_eq!(
        labels,
        [
            "Körper",
            "Kanten",
            "Art",
            "Abstand",
            "Winkel",
            "Tangentenkette"
        ]
    );
    s.selected_body = None;
    s.selected_feature = Some(id);
    ctx.run_ui(egui::RawInput::default(), |ui| properties::show(ui, &mut s))
        .drop_without_applying_deltas();
}

#[test]
fn the_feature_dialog_of_a_blend_shows_its_body() {
    let ctx = context();
    let (mut s, id) = blended_state(BlendArt::Rundung { radius: 1.0 });
    let mut d = FeatureDialog::new(&s, id).expect("Dialog für Rundung");
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(d.show(ui.ctx(), &mut s));
        });
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    for must in ["Körper: Quader 1", "Radius", "Übernehmen"] {
        assert!(text.contains(must), "„{must}“ fehlt:\n{text}");
    }
}
