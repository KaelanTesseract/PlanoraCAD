//! Rauchtests ohne Fenster: Dialog und Eigenschaften werden mit einem egui-Kontext gezeichnet.

use planora_app::boolean_dialog::BooleanDialog;
use planora_app::state::*;
use planora_app::{fonts, properties};
use planora_kernel::*;
use planora_math::DVec3;

fn state() -> AppState {
    let mut s = AppState::new();
    let k = s.kernel;
    s.add_body(
        BodyKind::Box,
        "Quader".into(),
        k.make_box(DVec3::splat(10.0)).unwrap(),
    )
    .unwrap();
    s.add_body(
        BodyKind::Cylinder,
        "Zylinder".into(),
        k.make_cylinder(2.0, 14.0).unwrap(),
    )
    .unwrap();
    s
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

#[test]
fn the_boolean_dialog_draws_and_stays_open_without_input() {
    let ctx = context();
    let mut s = state();
    let mut dialog = BooleanDialog::new(&s);
    for _ in 0..3 {
        let mut open = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            open = dialog.show(ui.ctx(), &mut s);
        })
        .drop_without_applying_deltas();
        assert!(open);
    }
    assert_eq!(
        s.bodies.len(),
        2,
        "ohne Klick auf „Ausführen“ bleibt alles unverändert"
    );
    assert!(s.message.is_none());
}

#[test]
fn the_properties_panel_draws_the_position_without_moving_the_body() {
    let ctx = context();
    let mut s = state();
    s.selected_body = Some(1);
    let bounds = s.bodies[1].bounds;
    let version = s.scene_version;
    for _ in 0..3 {
        ctx.run_ui(egui::RawInput::default(), |ui| properties::show(ui, &mut s))
            .drop_without_applying_deltas();
    }
    assert_eq!(s.bodies[1].bounds, bounds);
    assert_eq!(s.scene_version, version);
    assert!(s.message.is_none());
}

#[test]
fn the_boolean_dialog_draws_a_disabled_run_button_and_an_error() {
    let ctx = context();
    let mut s = state();
    let mut dialog = BooleanDialog::new(&s);
    dialog.b = dialog.a;
    assert!(
        dialog.execute(&mut s),
        "Fehler bei A = B hält den Dialog offen"
    );
    for _ in 0..3 {
        let mut open = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            open = dialog.show(ui.ctx(), &mut s);
        })
        .drop_without_applying_deltas();
        assert!(open);
    }
    assert!(!dialog.can_run());
    assert!(
        dialog.error().is_some(),
        "die Meldung bleibt beim Zeichnen stehen"
    );
    assert_eq!(s.bodies.len(), 2);
}

#[test]
fn opening_a_sketch_drops_open_body_and_boolean_dialogs() {
    use planora_app::app::drop_dialogs_in_sketch_mode;
    use planora_app::create_dialog::CreateDialog;
    let mut s = state();
    let mut create = Some(CreateDialog::Box {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    });
    let mut boolean = Some(BooleanDialog::new(&s));
    drop_dialogs_in_sketch_mode(&s, &mut create, &mut boolean);
    assert!(
        create.is_some() && boolean.is_some(),
        "ohne Skizze bleiben sie offen"
    );
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.open_sketch(i).unwrap();
    drop_dialogs_in_sketch_mode(&s, &mut create, &mut boolean);
    assert!(create.is_none() && boolean.is_none());
}
