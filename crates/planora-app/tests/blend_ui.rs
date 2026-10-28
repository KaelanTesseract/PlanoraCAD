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
    let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
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

// ---- Aufgabe 11: Formeln im Rundungsfenster ---------------------------------------------------

fn named_param(s: &mut AppState, name: &str, formula: &str, unit: planora_feature::ParamUnit) {
    let id = s.add_parameter().unwrap();
    s.edit_parameter(
        id,
        planora_feature::ParamEdit {
            name: name.into(),
            formula: formula.into(),
            unit,
            comment: String::new(),
        },
    )
    .unwrap();
}

/// Breite = 40 (Länge), w = 30 (Winkel), Quader 20 mit gewählter Kante Deckel/Wand 1.
fn blend_params_state() -> AppState {
    use planora_feature::ParamUnit;
    let mut s = AppState::new();
    named_param(&mut s, "Breite", "40", ParamUnit::Length);
    named_param(&mut s, "w", "30", ParamUnit::Angle);
    s.create_box(DVec3::splat(20.0)).unwrap();
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
    s.set_selection(Some(planora_render::Pick::Edge(planora_render::EdgeRef {
        body: 0,
        edge: e,
    })));
    s
}

#[test]
fn blend_dialog_formulas() {
    use planora_app::blend_dialog::{ArtChoice, angle_message};
    use planora_feature::{FeatureKind, FieldKey};
    // Rundung: Radius „Breite/20“ → 2, Formel am Radius.
    let mut s = blend_params_state();
    let mut d = BlendDialog::fillet();
    d.first = "Breite/20".into();
    assert!(!d.execute(&mut s), "{:?}", d.error());
    let f = s.document.features().last().unwrap();
    match &f.kind {
        FeatureKind::Verrunden { art, .. } => assert_eq!(*art, BlendArt::Rundung { radius: 2.0 }),
        k => panic!("{k:?}"),
    }
    assert_eq!(
        f.formulas().get(&FieldKey::Radius).map(String::as_str),
        Some("Breite/20")
    );

    // Fase mit Winkel: Winkel „w“ → 30°, Formel am Winkel.
    let mut s = blend_params_state();
    let mut d = BlendDialog::chamfer();
    d.set_art(ArtChoice::FaseWinkel);
    d.second = "w".into();
    let scope = s.document.param_scope();
    let (art, formulas) = d.art_formulas(&scope).unwrap();
    assert_eq!(
        art,
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 30.0
        }
    );
    assert_eq!(
        formulas.get(&FieldKey::Winkel).map(String::as_str),
        Some("w")
    );
    // `art_value` bleibt für Zahlen: ohne Parameter ist „w“ kein gültiger Winkel.
    assert_eq!(d.art_value(), Err(angle_message("w")));
    assert!(!d.execute(&mut s), "{:?}", d.error());
    let f = s.document.features().last().unwrap();
    match &f.kind {
        FeatureKind::Verrunden { art, .. } => assert_eq!(
            *art,
            BlendArt::FaseWinkel {
                abstand: 1.0,
                winkel: 30.0
            }
        ),
        k => panic!("{k:?}"),
    }
    assert_eq!(
        f.formulas().get(&FieldKey::Winkel).map(String::as_str),
        Some("w")
    );

    // Winkel „Breite“: Einheitenmeldung im Fenster, kein Feature.
    let mut s = blend_params_state();
    let count = s.document.features().len();
    let mut d = BlendDialog::chamfer();
    d.set_art(ArtChoice::FaseWinkel);
    d.second = "Breite".into();
    let msg = "Winkel: Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.";
    assert!(d.execute(&mut s), "Fenster bleibt offen");
    assert_eq!(d.error().as_deref(), Some(msg));
    assert_eq!(s.document.features().len(), count, "kein Feature");
    // Das Fenster zeigt die Meldung schon beim Tippen (Fußzeile) und sperrt „Erzeugen“.
    let ctx = context();
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(d.show(ui.ctx(), &mut s));
        });
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    assert!(text.contains(msg), "Meldung fehlt:\n{text}");
    assert_eq!(s.document.features().len(), count);
}
