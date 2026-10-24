//! Verteiler der Tastenkürzel (`keymap::dispatch`): die heutigen Kürzel laufen über die
//! Tastenbelegung, mit denselben Sperren wie vorher (Tippen, Rückfrage, zweiter Durchgang; für
//! Rückgängig/Wiederholen zusätzlich offene Fenster und gedrückte Maustaste). Menüs, Tooltips
//! und Hinweise zeigen die Taste aus der Tabelle.
//!
//! Mit echten Bildern der ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`) bzw. einem
//! nackten egui-Kontext und rohen Tastenereignissen; Bibliothek und Konfiguration nur im
//! Temp-Ordner.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{self, PlanoraApp};
use planora_app::export_dialog::ExportDialog;
use planora_app::keymap::dispatch::{self, current_area};
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult, shortcut_hint};
use planora_app::state::AppState;
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, prompts, shell_panels, theme, viewport};
use planora_feature::{SketchPlane, StandardPlane};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use std::collections::BTreeMap;

const SIZE: Vec2 = vec2(1400.0, 900.0);

const SHIFT_COMMAND: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: true,
};

// ---- Hilfen ---------------------------------------------------------------------------------

fn ws() -> Workspace {
    let root = common::temp_root("planora-keymap-dispatch");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    w
}

fn chord(key: Key, ctrl: bool, shift: bool, alt: bool) -> KeyChord {
    KeyChord {
        key,
        ctrl,
        shift,
        alt,
    }
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn repeat(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: true,
        modifiers,
    }
}

fn keymap_from(entries: &[(&str, &[&str])]) -> Keymap {
    let map: BTreeMap<String, Vec<String>> = entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
        .collect();
    let (km, notes) = Keymap::from_overrides(&map);
    assert!(notes.is_empty(), "{notes:?}");
    km
}

/// Ein Bild mit nacktem Kontext: erst `before` (etwa ein fokussierter Knopf), dann der
/// Verteiler; liefert dessen Aktionen und ob danach noch Tastenereignisse übrig sind.
fn dispatch_frame(
    ctx: &egui::Context,
    km: &Keymap,
    area: Area,
    events: Vec<Event>,
    before: &mut dyn FnMut(&mut egui::Ui),
) -> (Vec<Action>, bool) {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
        events,
        ..Default::default()
    };
    let mut got = Vec::new();
    let mut left = false;
    let out = ctx.run_ui(raw, |ui| {
        before(ui);
        got = dispatch::pressed(ui.ctx(), km, area);
        left = ui.input(|i| i.events.iter().any(|e| matches!(e, Event::Key { .. })));
    });
    out.drop_without_applying_deltas();
    (got, left)
}

fn dispatch_once(km: &Keymap, area: Area, events: Vec<Event>) -> (Vec<Action>, bool) {
    dispatch_frame(&egui::Context::default(), km, area, events, &mut |_| {})
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    /// Blieben nach dem Bild Tastenereignisse übrig (nicht verbraucht)?
    left_over: bool,
    passes: usize,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            left_over: false,
            passes: 0,
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn run(&mut self, events: Vec<Event>, two_passes: bool) -> Vec<Seen> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let mut left_over = false;
        let mut passes = 0;
        let out = self.ctx.run_ui(raw, |ui| {
            passes += 1;
            if two_passes && ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("Test: zweiter Durchgang");
            }
            app.ui(ui, &mut f);
            left_over = ui.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, Event::Key { pressed: true, .. }))
            });
        });
        self.left_over = left_over;
        self.passes = passes;
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        self.run(events, false)
    }

    fn settle(&mut self) -> Vec<Seen> {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn press(&mut self, k: Key, m: Modifiers) {
        self.frame(vec![key(k, m)]);
    }

    fn tabs(&mut self) -> usize {
        self.ws().tabs.len()
    }

    fn bodies(&mut self) -> usize {
        self.st().bodies.len()
    }

    fn undo_len(&mut self) -> usize {
        self.ws().active_tab().history().undo_len()
    }

    fn redo_len(&mut self) -> usize {
        self.ws().active_tab().history().redo_len()
    }

    /// Texte, während die Maus über `p` steht (Tooltip ohne Verzögerung).
    fn hover(&mut self, p: Pos2) -> Vec<String> {
        let mut seen = vec![];
        for _ in 0..5 {
            self.time += 0.3;
            seen = self.frame(vec![Event::PointerMoved(p)]);
        }
        seen.into_iter().map(|s| s.text).collect()
    }

    /// Klick an `p` (hin, drücken, loslassen, ein Bild danach).
    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }
}

/// Ein gezeichneter Text mit seinem Rechteck.
struct Seen {
    text: String,
    rect: Rect,
}

fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        egui::epaint::Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: Rect::from_min_size(t.pos, t.galley.size()),
        }),
        _ => {}
    }
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Quader, dann Kugel, die Kugel zurückgenommen: es gibt Rückgängig und Wiederholen.
fn box_sphere_undone() -> Rig {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.observe_history(false);
    w.active_tab_mut().state.create_sphere(3.0).unwrap();
    w.observe_history(false);
    w.active_tab_mut().undo().unwrap();
    let mut rig = Rig::new(w);
    assert_eq!((rig.bodies(), rig.undo_len(), rig.redo_len()), (1, 1, 1));
    rig
}

// ---- Verteiler ohne App ---------------------------------------------------------------------

#[test]
fn default_keys_still_trigger_the_same_actions() {
    let km = Keymap::defaults();
    for (k, m, want) in [
        (Key::N, Modifiers::COMMAND, Action::New),
        (Key::O, Modifiers::COMMAND, Action::Projects),
        (Key::S, Modifiers::COMMAND, Action::Save),
        (Key::W, Modifiers::COMMAND, Action::Close),
        (Key::E, SHIFT_COMMAND, Action::Export),
        (Key::Z, Modifiers::COMMAND, Action::Undo),
        (Key::Y, Modifiers::COMMAND, Action::Redo),
        (Key::Z, SHIFT_COMMAND, Action::Redo),
    ] {
        for area in [Area::Model, Area::Sketch, Area::Drawing] {
            let (got, left) = dispatch_once(&km, area, vec![key(k, m)]);
            assert_eq!(got, [want], "{k:?} {m:?} in {area:?}");
            assert!(!left, "{k:?}: verbraucht");
        }
    }
    // Auch Strg als `ctrl` (so meldet Windows die Taste: `ctrl` und `command`).
    let ctrl = Modifiers {
        ctrl: true,
        command: true,
        ..Modifiers::NONE
    };
    assert_eq!(
        dispatch_once(&km, Area::Model, vec![key(Key::S, ctrl)]).0,
        [Action::Save]
    );
    // Ohne Strg, mit Alt dazu oder einer fremden Taste: nichts, und nichts verbraucht.
    for (k, m) in [
        (Key::S, Modifiers::NONE),
        (Key::S, Modifiers::ALT | Modifiers::COMMAND),
        (Key::Z, Modifiers::ALT | Modifiers::COMMAND),
        (Key::Q, Modifiers::COMMAND),
    ] {
        let (got, left) = dispatch_once(&km, Area::Model, vec![key(k, m)]);
        assert!(got.is_empty(), "{k:?} {m:?}: {got:?}");
        assert!(left, "{k:?} {m:?} bleibt liegen");
    }

    // Die ganze App: jedes Kürzel tut, was es vorher tat.
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    rig.press(Key::N, Modifiers::COMMAND);
    rig.settle();
    assert_eq!(rig.tabs(), 2, "Strg+N: neuer Tab");
    rig.press(Key::W, Modifiers::COMMAND);
    rig.settle();
    assert_eq!(rig.tabs(), 1, "Strg+W: Tab zu");
    rig.press(Key::E, SHIFT_COMMAND);
    rig.settle();
    assert!(
        rig.ws().active_tab().export_dialog.is_some(),
        "Strg+Umschalt+E"
    );
    rig.ws().active_tab_mut().export_dialog = None;
    rig.settle();
    rig.press(Key::S, Modifiers::COMMAND);
    assert!(
        matches!(rig.ws().prompt, Some(Prompt::SaveAs { .. })),
        "Strg+S"
    );
    rig.ws().prompt = None;
    rig.settle();
    rig.press(Key::O, Modifiers::COMMAND);
    rig.settle();
    assert!(rig.app.windows_mut().1.is_open(), "Strg+O: Projekte");
    rig.app.windows_mut().1.close();
    rig.settle();
    rig.press(Key::Z, Modifiers::COMMAND);
    assert_eq!(rig.bodies(), 0, "Strg+Z");
    rig.press(Key::Y, Modifiers::COMMAND);
    assert_eq!(rig.bodies(), 1, "Strg+Y");
    rig.press(Key::Z, Modifiers::COMMAND);
    rig.press(Key::Z, SHIFT_COMMAND);
    assert_eq!(rig.bodies(), 1, "Strg+Umschalt+Z");
}

#[test]
fn ctrl_shift_z_is_redo_and_not_undo() {
    let km = Keymap::defaults();
    let (got, left) = dispatch_once(&km, Area::Model, vec![key(Key::Z, SHIFT_COMMAND)]);
    assert_eq!(got, [Action::Redo]);
    assert!(!left);
    // Beide in einem Bild: mehr Umschalttasten zuerst, jede Aktion einmal.
    let (got, _) = dispatch_once(
        &km,
        Area::Model,
        vec![key(Key::Z, Modifiers::COMMAND), key(Key::Z, SHIFT_COMMAND)],
    );
    assert_eq!(got, [Action::Redo, Action::Undo]);
    let (got, _) = dispatch_once(
        &km,
        Area::Model,
        vec![key(Key::N, Modifiers::COMMAND), key(Key::E, SHIFT_COMMAND)],
    );
    assert_eq!(got, [Action::Export, Action::New]);
    // Hat Wiederholen Strg+Umschalt+Z nicht mehr, wird daraus kein Rückgängig.
    let km = keymap_from(&[("bearbeiten.wiederholen", &["Strg+Y"])]);
    let (got, left) = dispatch_once(&km, Area::Model, vec![key(Key::Z, SHIFT_COMMAND)]);
    assert!(got.is_empty(), "{got:?}");
    assert!(left);

    // In der App: ein Schritt vor, keiner zurück.
    let mut rig = box_sphere_undone();
    rig.press(Key::Z, SHIFT_COMMAND);
    assert_eq!((rig.bodies(), rig.undo_len(), rig.redo_len()), (2, 2, 0));
    assert!(!rig.left_over);
}

#[test]
fn a_held_key_fires_once_per_frame() {
    let km = Keymap::defaults();
    let (got, left) = dispatch_once(
        &km,
        Area::Model,
        vec![
            key(Key::N, Modifiers::COMMAND),
            repeat(Key::N, Modifiers::COMMAND),
            repeat(Key::N, Modifiers::COMMAND),
        ],
    );
    assert_eq!(got, [Action::New], "eine Aktion je Bild");
    assert!(!left, "alle Wiederholungen verbraucht");
    // Zwei Belegungen derselben Aktion in einem Bild: auch nur einmal.
    let (got, _) = dispatch_once(
        &km,
        Area::Model,
        vec![key(Key::Y, Modifiers::COMMAND), key(Key::Z, SHIFT_COMMAND)],
    );
    assert_eq!(got, [Action::Redo]);

    let mut rig = Rig::new(ws());
    rig.frame(vec![
        key(Key::N, Modifiers::COMMAND),
        repeat(Key::N, Modifiers::COMMAND),
    ]);
    assert_eq!(rig.tabs(), 2, "ein neuer Tab im Bild");
    rig.frame(vec![repeat(Key::N, Modifiers::COMMAND)]);
    assert_eq!(rig.tabs(), 3, "nächstes Bild, nächster Tab");
}

/// Ein Bedienelement mit Tastaturfokus (kein Eingabefeld) sperrt nur die Leertaste.
#[test]
fn focus_on_a_widget_blocks_only_the_space_key() {
    let km = keymap_from(&[
        ("datei.neu", &["Leertaste"]),
        ("modell.befehl_wiederholen", &[]),
        ("skizze.werkzeug_wiederholen", &[]),
    ]);
    let ctx = egui::Context::default();
    let id = egui::Id::new("knopf");
    let mut button = |ui: &mut egui::Ui| {
        let r = ui.push_id(id, |ui| ui.button("Knopf")).inner;
        r.request_focus();
    };
    for _ in 0..3 {
        dispatch_frame(&ctx, &km, Area::Model, vec![], &mut button);
    }
    assert!(
        ctx.memory(|m| m.focused()).is_some(),
        "der Knopf hat den Fokus"
    );
    let (got, left) = dispatch_frame(
        &ctx,
        &km,
        Area::Model,
        vec![key(Key::Space, Modifiers::NONE)],
        &mut button,
    );
    assert!(got.is_empty(), "{got:?}");
    assert!(left, "die Leertaste bleibt dem Knopf");
    let (got, left) = dispatch_frame(
        &ctx,
        &km,
        Area::Model,
        vec![key(Key::S, Modifiers::COMMAND)],
        &mut button,
    );
    assert_eq!(got, [Action::Save], "andere Kürzel wirken");
    assert!(!left);
    // Ohne Fokus wirkt die Leertaste.
    let (got, left) = dispatch_once(&km, Area::Model, vec![key(Key::Space, Modifiers::NONE)]);
    assert_eq!(got, [Action::New]);
    assert!(!left);
}

#[test]
fn area_follows_sketch_and_drawing() {
    let mut s = AppState::new();
    assert_eq!(current_area(&s), Area::Model);
    s.create_box(DVec3::splat(10.0)).unwrap();
    let d = s
        .document
        .add_drawing(
            "Blatt 1",
            planora_drawing::Sheet {
                paper: planora_drawing::Paper::A4,
                landscape: true,
            },
            planora_drawing::Projection::First,
        )
        .unwrap();
    s.drawing_ui.open(d);
    assert_eq!(current_area(&s), Area::Drawing);
    s.drawing_ui.area = planora_app::drawing_state::WorkArea::Model;
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    assert_eq!(current_area(&s), Area::Sketch);
}

// ---- Sperren -------------------------------------------------------------------------------

#[test]
fn no_key_fires_while_typing_or_prompt_or_second_pass() {
    // Tippen: das Namensfeld im Fenster „Umbenennen“ hat die Tastatur.
    let mut w = ws();
    let id = w
        .active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.observe_history(false);
    w.active_tab_mut().state.rename_draft = Some((id, "Teil".into()));
    let mut rig = Rig::new(w);
    let at = rig
        .settle()
        .into_iter()
        .find(|s| s.text == "Teil")
        .expect("Feld im Fenster „Umbenennen“")
        .rect
        .center();
    rig.click(at);
    assert!(viewport::typing(&rig.ctx), "das Namensfeld nimmt Tasten an");
    rig.press(Key::N, Modifiers::COMMAND);
    rig.settle();
    assert_eq!(rig.tabs(), 1, "beim Tippen kein neuer Tab");

    // Rückfrage offen.
    let mut rig = Rig::new(ws());
    rig.ws().prompt = Some(Prompt::Notice("Hinweis".into()));
    rig.settle();
    rig.press(Key::N, Modifiers::COMMAND);
    assert_eq!(rig.tabs(), 1, "bei offener Rückfrage kein neuer Tab");
    assert!(rig.left_over, "die Taste bleibt liegen");

    // Zwei Durchgänge: nur der erste löst aus.
    let mut rig = Rig::new(ws());
    rig.run(vec![key(Key::N, Modifiers::COMMAND)], true);
    assert_eq!(rig.passes, 2);
    assert_eq!(rig.tabs(), 2, "genau ein neuer Tab");
}

#[test]
fn undo_redo_stay_blocked_by_open_windows_and_pressed_mouse_button() {
    for case in 0..6 {
        let mut rig = box_sphere_undone();
        match case {
            0 => rig.ws().about_open = true,
            1 => {
                let t = rig.ws().active_tab_mut();
                t.export_dialog = Some(ExportDialog::new(&t.state));
            }
            2 => rig.app.windows_mut().2.open(false),
            3 => {
                let p = rig
                    .ws()
                    .library
                    .as_ref()
                    .unwrap()
                    .create_project("P")
                    .unwrap();
                planora_app::project_actions::request_delete(rig.app.panels_mut().1, p);
            }
            4 => {
                let p = rig
                    .ws()
                    .library
                    .as_ref()
                    .unwrap()
                    .create_project("P")
                    .unwrap();
                rig.ws().save_as(&p, "Teil", "Anfang").unwrap();
                rig.st().create_cone(1.0, 2.0).unwrap();
                rig.ws().observe_history(false);
                let (w, _, versions) = rig.app.panels_mut();
                planora_app::versions_panel::request_delete_version(w, versions, 1);
                assert!(planora_app::versions_panel::has_pending(versions));
            }
            _ => {
                let p = shell_panels::last_layout(&rig.ctx).unwrap().free.center();
                rig.frame(vec![Event::PointerMoved(p)]);
                rig.frame(vec![button(p, true)]);
            }
        }
        if case < 5 {
            rig.settle();
        }
        let steps = (rig.bodies(), rig.undo_len(), rig.redo_len());
        for (k, m) in [
            (Key::Z, Modifiers::COMMAND),
            (Key::Y, Modifiers::COMMAND),
            (Key::Z, SHIFT_COMMAND),
        ] {
            rig.press(k, m);
            assert!(rig.left_over, "Fall {case}: {k:?} bleibt unverbraucht");
            assert_eq!(
                (rig.bodies(), rig.undo_len(), rig.redo_len()),
                steps,
                "Fall {case}: {k:?} ändert nichts"
            );
        }
        // Die übrigen Kürzel sind davon nicht gesperrt (wie bisher).
        if case == 0 {
            rig.press(Key::N, Modifiers::COMMAND);
            assert_eq!(rig.tabs(), 2, "„Über“ offen: Strg+N wirkt");
        }
    }
}

// ---- Ändern wirkt sofort, Anzeige folgt -----------------------------------------------------

#[test]
fn rebinding_changes_dispatch_immediately() {
    let mut rig = Rig::new(ws());
    let mut km = rig.ws().keymap.clone();
    assert_eq!(
        km.set(
            Action::Save,
            0,
            Binding::Key(chord(Key::S, true, true, false))
        ),
        SetResult::Ok
    );
    rig.ws().keymap = km;
    rig.press(Key::S, Modifiers::COMMAND);
    assert!(rig.ws().prompt.is_none(), "Strg+S tut nichts mehr");
    assert!(rig.left_over);
    rig.settle();
    rig.press(Key::S, SHIFT_COMMAND);
    assert!(
        matches!(rig.ws().prompt, Some(Prompt::SaveAs { .. })),
        "Strg+Umschalt+S speichert"
    );
}

#[test]
fn menu_and_tooltip_text_follow_the_keymap() {
    let mut km = Keymap::defaults();
    assert_eq!(shortcut_hint(&km, Action::Save), "Strg+S");
    // Mehrere Belegungen: im Menü die erste.
    assert_eq!(shortcut_hint(&km, Action::Redo), "Strg+Y");
    km.set(
        Action::Save,
        0,
        Binding::Key(chord(Key::S, true, true, false)),
    );
    assert_eq!(shortcut_hint(&km, Action::Save), "Strg+Umschalt+S");
    // Ohne Belegung: kein Text.
    let none = keymap_from(&[("datei.speichern", &[])]);
    assert_eq!(shortcut_hint(&none, Action::Save), "");

    // Das Menü „Datei“ zeigt die neue Taste.
    let mut w = ws();
    w.keymap = km.clone();
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut texts = String::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            app::file_menu(ui, &mut w);
        });
        texts = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    let lines: Vec<&str> = texts.lines().collect();
    assert!(lines.contains(&"Strg+Umschalt+S"), "{lines:?}");
    assert!(!lines.contains(&"Strg+S"), "{lines:?}");
    assert!(lines.contains(&"Strg+N"), "{lines:?}");

    // Tooltip des Knopfs „Rückgängig“ im oberen Feld.
    let mut rig = box_sphere_undone();
    let mut km = Keymap::defaults();
    km.set(
        Action::Undo,
        0,
        Binding::Key(chord(Key::Z, true, false, true)),
    );
    rig.ws().keymap = km;
    rig.settle();
    let at = shell_panels::history_buttons_seen(&rig.ctx)
        .into_iter()
        .find(|(l, _)| l == "Rückgängig")
        .expect("Knopf „Rückgängig“")
        .1
        .center();
    let texts = rig.hover(at);
    assert!(
        texts
            .iter()
            .any(|t| t == "Rückgängig: Quader anlegen (Strg+Alt+Z)"),
        "{texts:?}"
    );

    // Keine festen Tastentexte mehr in Tabs, Feldern und Hinweisen (Kommentare ausgenommen).
    for (name, src) in [
        ("tabs.rs", include_str!("../src/tabs.rs")),
        ("shell_panels.rs", include_str!("../src/shell_panels.rs")),
        ("prompts.rs", include_str!("../src/prompts.rs")),
        ("app.rs", include_str!("../src/app.rs")),
    ] {
        for (i, line) in src.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            assert!(!code.contains("Strg+"), "{name}:{}: {line}", i + 1);
        }
    }
}

#[test]
fn prompt_hint_uses_the_current_projects_key() {
    let km = Keymap::defaults();
    assert_eq!(
        prompts::no_project_hint(&km),
        "Noch kein Projekt vorhanden. Bitte mit „Abbrechen“ schließen, dann mit Strg+O das Fenster Projekte öffnen und ein Projekt anlegen."
    );
    let km = keymap_from(&[("datei.projekte", &["Strg+P"])]);
    let hint = prompts::no_project_hint(&km);
    assert!(hint.contains("mit Strg+P das Fenster Projekte"), "{hint}");
    assert!(!hint.contains("Strg+O"), "{hint}");
    // Ohne Taste: der Weg über das Menü.
    let km = keymap_from(&[("datei.projekte", &[])]);
    let hint = prompts::no_project_hint(&km);
    assert!(!hint.contains("Strg"), "{hint}");
    assert!(hint.contains(app::PROJECTS_ENTRY), "{hint}");
}
