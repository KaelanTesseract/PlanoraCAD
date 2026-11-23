//! Ruling H1-5 (a): Fokus-Schutz am Bildende. egui 0.36 gibt den Fokus ungefiltert an accesskit
//! weiter; zeigt er auf ein Widget, das im Bild nicht gezeichnet wurde, bricht accesskit_consumer im
//! Fenster ab. [`viewport::remember_keyboard_focus`] (am Ende jedes Bilds, Hauptfenster und
//! abgedockte Fenster) gibt einen solchen Fokus ab und reicht ihn an das Widget weiter, sobald es
//! gezeichnet wird.
//!
//! egui 0.37 (C13): egui filtert den accesskit-Fokus selbst; der Schutz bleibt, weil sonst ein nicht
//! gezeichnetes Feld die Tastatur hielte (die Prüfungen auf `memory.focused()` unten).

use egui::{Id, Rect, pos2};
use planora_app::viewport;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx
}

/// Ein Bild: `draw` baut die Oberfläche, danach wie in der App `remember_keyboard_focus`. Prüft
/// wie accesskit_consumer, dass der Fokus auf einem Knoten des Bilds steht.
fn frame(ctx: &egui::Context, draw: impl FnMut(&mut egui::Ui)) {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1920.0, 1080.0))),
        ..Default::default()
    };
    let mut draw = draw;
    let out = ctx.run_ui(raw, |ui| {
        draw(ui);
        viewport::remember_keyboard_focus(ui.ctx());
    });
    let update = out.platform_output.accesskit_update.as_ref().unwrap();
    assert!(
        update.nodes.iter().any(|(id, _)| *id == update.focus),
        "Fokus {:?} steht auf keinem Knoten dieses Bilds",
        update.focus
    );
    out.drop_without_applying_deltas();
}

#[test]
fn focus_on_an_undrawn_widget_is_released_at_frame_end() {
    let ctx = context();
    frame(&ctx, |_| {});
    let ghost = Id::unique("nie gezeichnet");
    frame(&ctx, |ui| {
        ui.label("etwas");
        ui.ctx().memory_mut(|m| m.request_focus(ghost));
    });
    assert_eq!(ctx.memory(|m| m.focused()), None, "Fokus am Bildende weg");
    frame(&ctx, |_| {});
    frame(&ctx, |_| {});
    assert_eq!(ctx.memory(|m| m.focused()), None, "kommt nicht zurück");
}

/// Wer den Fokus für ein Feld anfordert, das erst im nächsten Bild erscheint (etwa ein Dialog, der
/// gerade aufgeht), bekommt ihn dort trotzdem.
#[test]
fn focus_for_a_widget_of_the_next_frame_arrives_there() {
    let ctx = context();
    let mut text = String::new();
    let mut field_id = None;
    // Das Feld einmal zeichnen, um seine Kennung zu kennen, dann ein Bild ohne es.
    frame(&ctx, |ui| {
        field_id = Some(ui.text_edit_singleline(&mut text).id);
    });
    let field_id = field_id.unwrap();
    frame(&ctx, |_| {});
    frame(&ctx, |ui| {
        ui.ctx().memory_mut(|m| m.request_focus(field_id))
    });
    assert_eq!(ctx.memory(|m| m.focused()), None);
    for _ in 0..2 {
        frame(&ctx, |ui| {
            ui.text_edit_singleline(&mut text);
        });
    }
    assert_eq!(ctx.memory(|m| m.focused()), Some(field_id));
}

/// Ein gezeichnetes Feld behält seinen Fokus.
#[test]
fn focus_on_a_drawn_widget_stays() {
    let ctx = context();
    let mut text = String::new();
    let mut field_id = None;
    for _ in 0..3 {
        frame(&ctx, |ui| {
            let r = ui.text_edit_singleline(&mut text);
            if field_id.is_none() {
                r.request_focus();
            }
            field_id = Some(r.id);
        });
    }
    assert_eq!(ctx.memory(|m| m.focused()), field_id);
}
