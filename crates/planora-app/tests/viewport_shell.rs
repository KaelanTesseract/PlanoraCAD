//! 3D-Bereich im ganzen Fenster mit freiem Rechteck: Würfel und Bildmitte im freien Rechteck,
//! Zeiger über schwebenden Feldern (egui-Areas) dreht, wählt und zoomt nicht, Dialoge bleiben
//! über den Feldern. Ohne Fenster: Rohereignisse je Frame gegen einen egui-Kontext.

use egui::{Event, Modifiers, PointerButton, Rect, pos2, vec2};
use glam::Vec2;
use planora_app::dialog::{Dialog, DialogSize};
use planora_app::state::AppState;
use planora_app::{fonts, theme, viewport};
use planora_math::DVec3;

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(800.0, 600.0),
};

/// Freies Rechteck, absichtlich nicht mittig (links ein Feld, oben ein Feld).
const FREE: Rect = Rect {
    min: pos2(278.0, 90.0),
    max: pos2(700.0, 500.0),
};

/// Schwebendes Feld über der Mitte des freien Rechtecks.
const FIELD: Rect = Rect {
    min: pos2(429.0, 255.0),
    max: pos2(549.0, 335.0),
};

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn input(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

fn moved(x: f32, y: f32) -> Event {
    Event::PointerMoved(pos2(x, y))
}

fn button(x: f32, y: f32, pressed: bool) -> Event {
    Event::PointerButton {
        pos: pos2(x, y),
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn wheel(dy: f32) -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    }
}

fn escape() -> Event {
    Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Ein Quader 100 mm, eingepasst ins freie Rechteck.
fn cube_state(ctx: &egui::Context) -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(100.0)).unwrap();
    frame(ctx, &mut s, vec![], false);
    s.frame_all();
    s
}

/// Ein Frame wie in der App: 3D-Bereich über das ganze Fenster mit freiem Rechteck, darüber ein
/// schwebendes Feld (`Order::Middle`, nicht verschiebbar) mit einem Knopf, auf Wunsch einem
/// Textfeld; am Ende merkt sich die Ansicht den Tastaturfokus.
fn frame(ctx: &egui::Context, s: &mut AppState, events: Vec<Event>, text: bool) {
    let mut name = String::from("Feld");
    ctx.run_ui(input(events), |ui| {
        viewport::show_in(ui, s, Some(FREE));
        egui::Area::new(egui::Id::new("test-field"))
            .order(egui::Order::Middle)
            .movable(false)
            .fixed_pos(FIELD.min)
            .show(ui.ctx(), |ui| {
                ui.set_min_size(FIELD.size());
                ui.set_max_size(FIELD.size());
                let _ = ui.button("Knopf");
                if text {
                    let r = theme::text_field(ui, egui::TextEdit::singleline(&mut name));
                    // Fokus nur in Frames ohne Ereignisse holen, damit Escape ihn nehmen kann.
                    if !r.has_focus() && ui.input(|i| i.events.is_empty()) {
                        r.request_focus();
                    }
                }
            });
        viewport::remember_keyboard_focus(ui.ctx());
    })
    .drop_without_applying_deltas();
}

/// Klick (Drücken und Loslassen an einer Stelle) mit Leerlauf vorher und nachher.
fn click(ctx: &egui::Context, s: &mut AppState, x: f32, y: f32) {
    for events in [
        vec![moved(x, y)],
        vec![moved(x, y), button(x, y, true)],
        vec![button(x, y, false)],
        vec![],
    ] {
        frame(ctx, s, events, false);
    }
}

/// Ziehen von `from` nach `to` in `steps` Schritten; liefert `yaw` nach jedem Schritt.
fn drag(ctx: &egui::Context, s: &mut AppState, from: (f32, f32), to: (f32, f32)) -> Vec<f32> {
    frame(ctx, s, vec![moved(from.0, from.1)], false);
    frame(
        ctx,
        s,
        vec![moved(from.0, from.1), button(from.0, from.1, true)],
        false,
    );
    let mut yaws = Vec::new();
    let steps = 6;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let p = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        frame(ctx, s, vec![moved(p.0, p.1)], false);
        yaws.push(s.camera.yaw);
    }
    frame(ctx, s, vec![button(to.0, to.1, false)], false);
    yaws
}

#[test]
fn cube_sits_in_the_free_rect_corner() {
    let cube = viewport::cube_rect(FREE);
    assert_eq!(
        cube,
        Rect::from_min_size(
            pos2(FREE.right() - 84.0 - 12.0, FREE.top() + 12.0),
            vec2(84.0, 84.0)
        )
    );
    // `show_in` legt den Würfel dorthin: ein Klick auf seine untere Fläche dreht die Ansicht
    // (an der Fensterecke läge er außerhalb dieses Punkts).
    let ctx = context();
    let mut s = AppState::new();
    let (yaw, pitch) = (s.camera.yaw, s.camera.pitch);
    let p = cube.center() + vec2(0.0, 20.0);
    click(&ctx, &mut s, p.x, p.y);
    assert_ne!(
        (s.camera.yaw, s.camera.pitch),
        (yaw, pitch),
        "Würfel im freien Rechteck angeklickt"
    );
}

#[test]
fn camera_gets_the_visible_rect_each_frame() {
    let ctx = context();
    let mut s = AppState::new();
    for _ in 0..2 {
        frame(&ctx, &mut s, vec![], false);
    }
    let full = Vec2::new(SCREEN.width(), SCREEN.height());
    let px = s.camera.project(s.camera.target, full).unwrap();
    let want = Vec2::new(FREE.center().x, FREE.center().y);
    assert!((px - want).length() < 0.5, "{px:?} statt {want:?}");

    // Ohne freies Rechteck (wie bisher `show`) liegt das Ziel wieder in der Fenstermitte.
    ctx.run_ui(input(vec![]), |ui| viewport::show(ui, &mut s))
        .drop_without_applying_deltas();
    let px = s.camera.project(s.camera.target, full).unwrap();
    assert!((px - Vec2::new(400.0, 300.0)).length() < 0.5, "{px:?}");
}

#[test]
fn pointer_over_a_floating_area_does_not_orbit_or_select() {
    let ctx = context();
    let mut s = cube_state(&ctx);
    let (yaw, pitch) = (s.camera.yaw, s.camera.pitch);

    // Klick auf das Feld über der Körpermitte wählt nichts.
    click(&ctx, &mut s, FIELD.center().x, FIELD.center().y);
    assert_eq!(s.selection, None, "Klick auf dem Feld wählt nichts");
    // Ziehen auf dem Feld dreht nicht.
    drag(&ctx, &mut s, (440.0, 265.0), (540.0, 325.0));
    assert_eq!((s.camera.yaw, s.camera.pitch), (yaw, pitch), "kein Orbit");

    // Daneben (unterhalb des Feldes, noch auf dem Körper) wählt der Klick eine Fläche …
    click(&ctx, &mut s, FIELD.center().x, FIELD.bottom() + 25.0);
    assert!(s.selection.is_some(), "Klick neben dem Feld wählt");
    // … und das gleiche Ziehen dreht.
    drag(&ctx, &mut s, (200.0, 450.0), (300.0, 510.0));
    assert_ne!(
        (s.camera.yaw, s.camera.pitch),
        (yaw, pitch),
        "Orbit daneben"
    );
}

#[test]
fn drag_started_in_the_viewport_continues_over_a_field() {
    let ctx = context();
    let mut s = cube_state(&ctx);
    let yaws = drag(
        &ctx,
        &mut s,
        (300.0, 300.0),
        (FIELD.right() - 10.0, FIELD.center().y),
    );
    // Schritt i (1…6) liegt bei x = 300 + (539 − 300)·i/6: die Schritte 4 bis 6 (x ≥ 459) liegen
    // über dem Feld (x ≥ 429), der Orbit läuft dort weiter.
    assert!(300.0 + (FIELD.right() - 10.0 - 300.0) * 4.0 / 6.0 > FIELD.left());
    assert_ne!(yaws[4], yaws[3], "Orbit über dem Feld: {yaws:?}");
    assert_ne!(yaws[5], yaws[4], "Orbit bis zum Ende: {yaws:?}");
}

#[test]
fn mouse_wheel_over_a_field_does_not_zoom() {
    let scroll_at = |x: f32, y: f32| {
        let ctx = context();
        let mut s = cube_state(&ctx);
        let before = s.camera.distance;
        frame(&ctx, &mut s, vec![moved(x, y)], false);
        frame(&ctx, &mut s, vec![moved(x, y), wheel(120.0)], false);
        for _ in 0..20 {
            frame(&ctx, &mut s, vec![], false);
        }
        (before, s.camera.distance)
    };
    let (before, after) = scroll_at(FIELD.center().x, FIELD.center().y);
    assert_eq!(after, before, "Mausrad über dem Feld zoomt nicht");
    let (before, after) = scroll_at(FIELD.center().x, FIELD.bottom() + 25.0);
    assert!(after < before, "Mausrad daneben zoomt: {before} → {after}");
}

#[test]
fn dialogs_stay_above_fields() {
    let ctx = context();
    let field = Rect::from_min_size(pos2(300.0, 200.0), vec2(300.0, 200.0));
    let probe = pos2(400.0, 300.0);
    let mut window_layer = None;
    let mut frames = vec![vec![], vec![]];
    // Klick auf einen freien Teil des Feldes (außerhalb des Dialogs) hebt das Feld an.
    frames.push(vec![moved(320.0, 220.0), button(320.0, 220.0, true)]);
    frames.push(vec![button(320.0, 220.0, false)]);
    frames.push(vec![]);
    for (i, events) in frames.into_iter().enumerate() {
        ctx.run_ui(input(events), |ui| {
            egui::Area::new(egui::Id::new("test-field"))
                .order(egui::Order::Middle)
                .movable(false)
                .fixed_pos(field.min)
                .show(ui.ctx(), |ui| {
                    ui.set_min_size(field.size());
                    let _ = ui.label("Feld");
                });
            let shown = Dialog::new("Dialog", DialogSize::Normal).show(ui.ctx(), |ui| {
                ui.label("Ein Dialog über dem Feld");
            });
            window_layer = shown.map(|r| r.response.layer_id);
        })
        .drop_without_applying_deltas();
        if i >= 1 {
            assert_eq!(
                ctx.layer_id_at(probe),
                window_layer,
                "Frame {i}: der Dialog liegt über dem Feld"
            );
        }
    }
}

/// Dialog mit einer Auswahlliste; liefert das Rechteck der Liste und die Ebene des Dialogs.
fn combo_dialog(
    ctx: &egui::Context,
    events: Vec<Event>,
    choice: &mut i32,
) -> (Option<Rect>, Option<egui::LayerId>) {
    let mut combo = None;
    let mut layer = None;
    ctx.run_ui(input(events), |ui| {
        let shown = Dialog::new("Dialog", DialogSize::Normal).show(ui.ctx(), |ui| {
            let r = egui::ComboBox::from_id_salt("test-combo")
                .selected_text(format!("Wert {choice}"))
                .show_ui(ui, |ui| {
                    for v in 0..5 {
                        ui.selectable_value(choice, v, format!("Wert {v}"));
                    }
                });
            combo = Some(r.response.rect);
        });
        layer = shown.map(|r| r.response.layer_id);
    })
    .drop_without_applying_deltas();
    (combo, layer)
}

#[test]
fn popups_of_a_dialog_open_above_it() {
    let ctx = context();
    let mut choice = 0;
    combo_dialog(&ctx, vec![], &mut choice);
    let (combo, window_layer) = combo_dialog(&ctx, vec![], &mut choice);
    let c = combo.unwrap().center();
    for events in [
        vec![moved(c.x, c.y), button(c.x, c.y, true)],
        vec![button(c.x, c.y, false)],
        vec![],
        vec![],
    ] {
        combo_dialog(&ctx, events, &mut choice);
    }
    // Die Liste klappt unter dem Feld auf und liegt über dem Dialog (beide `Foreground`).
    let below = pos2(c.x, c.y + 30.0);
    let top = ctx.layer_id_at(below);
    assert!(top.is_some(), "Liste offen");
    assert_ne!(top, window_layer, "Liste liegt über dem Dialog");
    assert_eq!(top.unwrap().order, egui::Order::Foreground);
}

#[test]
fn escape_keeps_working_and_typing_in_a_field_blocks_it() {
    let ctx = context();
    let mut s = cube_state(&ctx);
    let below = (FIELD.center().x, FIELD.bottom() + 25.0);

    // Escape in der Ansicht hebt die Auswahl auf.
    click(&ctx, &mut s, below.0, below.1);
    assert!(s.selection.is_some());
    frame(&ctx, &mut s, vec![escape()], false);
    assert_eq!(s.selection, None, "Escape hebt die Auswahl auf");

    // Mit dem Fokus in einem Textfeld des Feldes bleibt die Auswahl.
    click(&ctx, &mut s, below.0, below.1);
    assert!(s.selection.is_some());
    for _ in 0..3 {
        frame(&ctx, &mut s, vec![], true);
    }
    assert!(ctx.egui_wants_keyboard_input(), "Textfeld hat den Fokus");
    frame(&ctx, &mut s, vec![escape()], true);
    assert!(s.selection.is_some(), "Escape im Textfeld wählt nicht ab");
}
