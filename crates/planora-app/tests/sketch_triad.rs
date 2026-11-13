//! Haken „3D-Skizze“ und die Tastenaktionen der Triade (Skizzen 10, Task 9): der Haken ist ein
//! Rückgängig-Schritt, Alt allein, Tab und Pfeil hoch (gehalten) sind belegbar, Alt mit
//! Mausziehen (Kamera) löst nichts aus. Die Triade selbst zeichnet erst Task 10; hier prüfen
//! wir Zustand, Aktionen und Einstellungen.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands::run_action;
use planora_app::keymap::dispatch::{self, ModTap};
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, ModKey, SetResult};
use planora_app::sketch_state::{TriadAxis, TriadPlane};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::SketchPlane;
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1920.0, 1080.0);

fn ws() -> Workspace {
    let root = common::temp_root("planora-sketch-triad");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    w
}

fn open_sketch(w: &mut Workspace) {
    let s = &mut w.active_tab_mut().state;
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
}

fn st(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
}

fn chord(key: Key) -> KeyChord {
    KeyChord {
        key,
        ctrl: false,
        shift: false,
        alt: false,
    }
}

/// Ein Bild mit nacktem Kontext und den gehaltenen Umschalttasten `mods`; `tap` ist der
/// Verfolger über die Bilder.
fn tap_frame(
    ctx: &egui::Context,
    tap: &mut ModTap,
    mods: Modifiers,
    mut events: Vec<Event>,
) -> Option<ModKey> {
    events.insert(0, Event::ModifiersChanged(mods));
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
        events,
        ..Default::default()
    };
    let mut got = None;
    let out = ctx.run_ui(raw, |ui| got = ui.input(|i| tap.step(i)));
    out.drop_without_applying_deltas();
    got
}

fn press(button: PointerButton, pressed: bool, mods: Modifiers) -> Event {
    Event::PointerButton {
        pos: Pos2::new(100.0, 100.0),
        button,
        pressed,
        modifiers: mods,
    }
}

// ---- Haken ------------------------------------------------------------------------------------

#[test]
fn three_d_checkbox_is_one_undo_step() {
    let mut w = ws();
    open_sketch(&mut w);
    w.observe_history(false);
    let n = st(&mut w).sketch_history().unwrap().undo_len();
    {
        let (se, sk) = st(&mut w).session_sketch_mut().unwrap();
        assert!(!sk.three_d);
        se.set_three_d(sk, true);
        assert!(sk.three_d);
    }
    w.observe_history(false);
    assert_eq!(st(&mut w).sketch_history().unwrap().undo_len(), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("3D-Skizze umschalten".to_string())
    );
    w.active_tab_mut().undo().unwrap();
    assert!(!st(&mut w).session_sketch().unwrap().three_d);
    // Denselben Zustand noch einmal setzen ist kein Schritt.
    {
        let (se, sk) = st(&mut w).session_sketch_mut().unwrap();
        se.set_three_d(sk, false);
    }
    w.observe_history(false);
    assert_eq!(st(&mut w).sketch_history().unwrap().undo_len(), n);
}

fn collect_text(shape: &egui::epaint::Shape, out: &mut Vec<(String, Pos2)>) {
    use egui::epaint::Shape;
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| collect_text(x, out)),
        Shape::Text(t) => {
            let r = Rect::from_min_size(t.pos, t.galley.size());
            out.push((t.galley.text().to_string(), r.center()));
        }
        _ => {}
    }
}

#[test]
fn the_panel_checkbox_toggles_the_sketch() {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    let mut app = PlanoraApp::for_tests(ws());
    open_sketch(app.ws_mut());
    let mut time = 0.0;
    let mut frame = |app: &mut PlanoraApp, events: Vec<Event>| {
        time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let out = c.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut found = Vec::new();
        for s in &out.shapes {
            collect_text(&s.shape, &mut found);
        }
        out.drop_without_applying_deltas();
        found
    };
    frame(&mut app, vec![]);
    frame(&mut app, vec![]);
    let t = frame(&mut app, vec![]);
    let at = t
        .iter()
        .find(|(s, _)| s == "3D-Skizze")
        .expect("Haken „3D-Skizze“ fehlt")
        .1;
    let ev = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(&mut app, vec![Event::PointerMoved(at)]);
    frame(&mut app, vec![Event::PointerMoved(at), ev(true)]);
    frame(&mut app, vec![ev(false)]);
    frame(&mut app, vec![]);
    assert!(st(app.ws_mut()).session_sketch().unwrap().three_d);
}

// ---- Belegung ---------------------------------------------------------------------------------

#[test]
fn triad_actions_are_rebindable() {
    assert_eq!(Action::SketchTriadCyclePlane.id(), "skizze.triade_ebene");
    assert_eq!(Action::SketchTriadCycleHandle.id(), "skizze.triade_griff");
    assert_eq!(Action::SketchLockZ.id(), "skizze.z_sperren");
    for a in [
        Action::SketchTriadCyclePlane,
        Action::SketchTriadCycleHandle,
        Action::SketchLockZ,
    ] {
        assert_eq!(a.area(), Area::Sketch);
    }
    let mut km = Keymap::defaults();
    assert_eq!(
        km.bindings(Action::SketchTriadCyclePlane),
        &[Binding::Modifier(ModKey::Alt)]
    );
    assert_eq!(
        km.bindings(Action::SketchTriadCycleHandle),
        &[Binding::Key(chord(Key::Tab))]
    );
    assert_eq!(
        km.bindings(Action::SketchLockZ),
        &[Binding::Key(chord(Key::ArrowUp))]
    );
    for (a, new) in [
        (Action::SketchTriadCyclePlane, Binding::Key(chord(Key::F9))),
        (Action::SketchTriadCycleHandle, Binding::Key(chord(Key::F8))),
        (Action::SketchLockZ, Binding::Modifier(ModKey::Ctrl)),
    ] {
        assert_eq!(km.set(a, 0, new), SetResult::Ok);
        assert_eq!(km.bindings(a), &[new]);
        assert!(!km.is_default(a));
        // Über die Konfiguration: gespeichert und wieder gelesen.
        let (back, notes) = Keymap::from_overrides(&km.to_overrides());
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(back.bindings(a), &[new]);
        km.reset(a);
        assert!(km.is_default(a));
    }
    // Alt allein ist belegbar und nicht doppelt belegbar.
    assert_eq!(
        km.conflict_for(
            Action::SketchTriadCycleHandle,
            &Binding::Modifier(ModKey::Alt)
        ),
        Some(Action::SketchTriadCyclePlane)
    );
    // Tab bleibt für die Aktion belegbar (Zurücksetzen), sonst ist er fest.
    assert!(
        Keymap::fixed_key_text(
            Action::SketchTriadCycleHandle,
            &Binding::Key(chord(Key::Tab))
        )
        .is_none()
    );
    assert!(Keymap::fixed_key_text(Action::SketchLine, &Binding::Key(chord(Key::Tab))).is_some());
    // Anzeige.
    assert_eq!(km.text(Action::SketchTriadCyclePlane), "Alt allein");
}

#[test]
fn alt_alone_fires_cycle_but_alt_drag_does_not() {
    let ctx = egui::Context::default();
    let mut tap = ModTap::default();
    // Alt drücken, halten, loslassen: genau beim Loslassen ein Tipp.
    assert_eq!(tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]), None);
    assert_eq!(tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]), None);
    assert_eq!(
        tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]),
        Some(ModKey::Alt)
    );
    assert_eq!(tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]), None);

    // Alt + Mausziehen (Kamera drehen): kein Tipp.
    for button in [PointerButton::Primary, PointerButton::Middle] {
        let mut tap = ModTap::default();
        tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
        tap_frame(
            &ctx,
            &mut tap,
            Modifiers::ALT,
            vec![press(button, true, Modifiers::ALT)],
        );
        tap_frame(
            &ctx,
            &mut tap,
            Modifiers::ALT,
            vec![Event::PointerMoved(Pos2::new(160.0, 120.0))],
        );
        tap_frame(
            &ctx,
            &mut tap,
            Modifiers::ALT,
            vec![press(button, false, Modifiers::ALT)],
        );
        assert_eq!(
            tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]),
            None,
            "{button:?}"
        );
    }
    // Alt + Taste (Alt+Tab, Alt+F4 …): kein Tipp.
    let mut tap = ModTap::default();
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    tap_frame(
        &ctx,
        &mut tap,
        Modifiers::ALT,
        vec![Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::ALT,
        }],
    );
    assert_eq!(tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]), None);
    // Alt zusammen mit Strg: kein Tipp von Alt.
    let mut tap = ModTap::default();
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    tap_frame(&ctx, &mut tap, Modifiers::ALT | Modifiers::CTRL, vec![]);
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    assert_eq!(tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]), None);
    // Die Aktion zum Tipp kommt aus der Belegung, nur im Bereich der Skizze.
    let km = Keymap::defaults();
    assert_eq!(
        dispatch::tap_actions(&km, Area::Sketch, ModKey::Alt),
        vec![Action::SketchTriadCyclePlane]
    );
    assert!(dispatch::tap_actions(&km, Area::Model, ModKey::Alt).is_empty());
}

#[test]
fn alt_tap_in_the_app_cycles_the_plane_only_in_a_3d_sketch() {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    let mut app = PlanoraApp::for_tests(ws());
    open_sketch(app.ws_mut());
    let mut time = 0.0;
    let mut frame = |app: &mut PlanoraApp, mods: Modifiers, mut events: Vec<Event>| {
        time += 1.0 / 60.0;
        events.insert(0, Event::ModifiersChanged(mods));
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        c.run_ui(raw, |ui| app.ui(ui, &mut f))
            .drop_without_applying_deltas();
    };
    let plane = |app: &mut PlanoraApp| {
        st(app.ws_mut())
            .sketch_session
            .as_ref()
            .unwrap()
            .triad_plane()
    };
    for _ in 0..3 {
        frame(&mut app, Modifiers::NONE, vec![]);
    }
    // Ohne den Haken tut Alt nichts.
    frame(&mut app, Modifiers::ALT, vec![]);
    frame(&mut app, Modifiers::NONE, vec![]);
    assert_eq!(plane(&mut app), TriadPlane::Xy);
    {
        let (se, sk) = st(app.ws_mut()).session_sketch_mut().unwrap();
        se.set_three_d(sk, true);
    }
    frame(&mut app, Modifiers::ALT, vec![]);
    frame(&mut app, Modifiers::NONE, vec![]);
    assert_eq!(plane(&mut app), TriadPlane::Yz);
    // Alt mit Ziehen: bleibt.
    frame(&mut app, Modifiers::ALT, vec![]);
    let p = Pos2::new(900.0, 500.0);
    frame(
        &mut app,
        Modifiers::ALT,
        vec![
            Event::PointerMoved(p),
            press(PointerButton::Middle, true, Modifiers::ALT),
        ],
    );
    frame(
        &mut app,
        Modifiers::ALT,
        vec![press(PointerButton::Middle, false, Modifiers::ALT)],
    );
    frame(&mut app, Modifiers::NONE, vec![]);
    assert_eq!(plane(&mut app), TriadPlane::Yz);
    // Aktion direkt: reihum bis zum Anfang.
    run_action(app.ws_mut(), Action::SketchTriadCyclePlane);
    assert_eq!(plane(&mut app), TriadPlane::Zx);
    run_action(app.ws_mut(), Action::SketchTriadCyclePlane);
    assert_eq!(plane(&mut app), TriadPlane::Xy);
    // Tab: Drehgriff reihum.
    let handle = |app: &mut PlanoraApp| {
        st(app.ws_mut())
            .sketch_session
            .as_ref()
            .unwrap()
            .triad_handle()
    };
    assert_eq!(handle(&mut app), None);
    run_action(app.ws_mut(), Action::SketchTriadCycleHandle);
    assert_eq!(handle(&mut app), Some(TriadAxis::X));
    run_action(app.ws_mut(), Action::SketchTriadCycleHandle);
    run_action(app.ws_mut(), Action::SketchTriadCycleHandle);
    assert_eq!(handle(&mut app), Some(TriadAxis::Z));
    run_action(app.ws_mut(), Action::SketchTriadCycleHandle);
    assert_eq!(handle(&mut app), Some(TriadAxis::X));
}

#[test]
fn arrow_up_held_locks_z_while_down() {
    let ctx = egui::Context::default();
    let km = Keymap::defaults();
    let frame = |events: Vec<Event>| {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            events,
            ..Default::default()
        };
        let mut got = false;
        ctx.run_ui(raw, |ui| {
            got = dispatch::held(ui.ctx(), &km, Area::Sketch, Action::SketchLockZ)
        })
        .drop_without_applying_deltas();
        got
    };
    let key = |pressed| Event::Key {
        key: Key::ArrowUp,
        physical_key: Some(Key::ArrowUp),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    assert!(!frame(vec![]));
    assert!(frame(vec![key(true)]));
    assert!(frame(vec![]), "gehalten bleibt es");
    assert!(!frame(vec![key(false)]));
}

// ---- Einstellungen ----------------------------------------------------------------------------

#[test]
fn triad_view_settings_default_on_and_persist() {
    let mut w = ws();
    assert!(w.config.show_triad_planes);
    assert!(w.config.show_triad_handles);
    assert!(w.set_triad_view(false, true));
    let c = Config::read_from(&w.config_path).unwrap();
    assert!(!c.show_triad_planes);
    assert!(c.show_triad_handles);
    assert!(w.set_triad_view(true, true));
    let text = std::fs::read_to_string(&w.config_path).unwrap();
    assert!(!text.contains("show_triad"), "{text}");
}
