//! Haken „3D-Skizze“ und die Tastenaktionen der Triade (Skizzen 10, Task 9): der Haken ist ein
//! Rückgängig-Schritt, Alt allein, Tab und Pfeil hoch (gehalten) sind belegbar, Alt mit
//! Mausziehen (Kamera) löst nichts aus. Ab Task 10 auch die Triade selbst: Lage, Drehung, Treffer,
//! Achsfang, Z-Sperre, Ziehen am Drehgriff, Rechtsklickmenü und das Winkelfeld.

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

/// Gesamtprüfung (M1): Der Haken bei offenem Punkt eines Linienzugs ist ein eigener Schritt; er
/// verschmilzt nicht mit der Linie, die danach entsteht.
#[test]
fn three_d_checkbox_with_an_open_point_stays_its_own_step() {
    let mut w = ws();
    open_sketch(&mut w);
    w.observe_history(false);
    let n = st(&mut w).sketch_history().unwrap().undo_len();
    {
        let (se, sk) = st(&mut w).session_sketch_mut().unwrap();
        se.choose_tool(sk, Tool::Line);
        se.click(sk, planora_math::DVec2::new(0.0, 0.0), 0.5, false)
            .unwrap();
    }
    w.observe_history(false);
    assert_eq!(
        st(&mut w).sketch_history().unwrap().undo_len(),
        n,
        "der offene Punkt gehört zur Linie"
    );
    {
        let (se, sk) = st(&mut w).session_sketch_mut().unwrap();
        se.set_three_d(sk, true);
    }
    w.observe_history(false);
    assert_eq!(st(&mut w).sketch_history().unwrap().undo_len(), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("3D-Skizze umschalten".to_string())
    );
    {
        let (se, sk) = st(&mut w).session_sketch_mut().unwrap();
        se.click(sk, planora_math::DVec2::new(10.0, 0.0), 0.5, false)
            .unwrap();
    }
    w.observe_history(false);
    assert_eq!(st(&mut w).sketch_history().unwrap().undo_len(), n + 2);
    let lines = |w: &mut Workspace| st(w).session_sketch().unwrap().curves().count();
    assert_eq!(lines(&mut w), 1);
    w.active_tab_mut().undo().unwrap();
    assert_eq!(lines(&mut w), 0, "erst die Linie");
    assert!(st(&mut w).session_sketch().unwrap().three_d);
    w.active_tab_mut().undo().unwrap();
    assert!(
        !st(&mut w).session_sketch().unwrap().three_d,
        "dann der Haken"
    );
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
        // Die Triade gehört zu den Zeichenwerkzeugen (Gesamtprüfung).
        se.choose_tool(sk, Tool::Line);
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

// ---- Fix-Runde 1: Belegungen wirken wirklich ----------------------------------------------------

/// Die ganze App mit offener Skizze, Bild für Bild.
struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    /// Blieb im letzten Bild ein Tab-Druck unverbraucht?
    tab_left: bool,
}

impl Rig {
    fn new(three_d: bool) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        let mut app = PlanoraApp::for_tests(ws());
        open_sketch(app.ws_mut());
        if three_d {
            let (se, sk) = st(app.ws_mut()).session_sketch_mut().unwrap();
            se.set_three_d(sk, true);
        }
        let mut rig = Rig {
            ctx,
            app,
            time: 0.0,
            tab_left: false,
        };
        for _ in 0..3 {
            rig.frame(Modifiers::NONE, vec![]);
        }
        rig
    }

    fn frame(&mut self, mods: Modifiers, mut events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        events.insert(0, Event::ModifiersChanged(mods));
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let mut left = false;
        self.ctx
            .run_ui(raw, |ui| {
                app.ui(ui, &mut f);
                left = ui.input(|i| {
                    i.events.iter().any(|e| {
                        matches!(
                            e,
                            Event::Key {
                                key: Key::Tab,
                                pressed: true,
                                ..
                            }
                        )
                    })
                });
            })
            .drop_without_applying_deltas();
        self.tab_left = left;
    }

    fn plane(&mut self) -> TriadPlane {
        st(self.app.ws_mut())
            .sketch_session
            .as_ref()
            .unwrap()
            .triad_plane()
    }

    fn handle(&mut self) -> Option<TriadAxis> {
        st(self.app.ws_mut())
            .sketch_session
            .as_ref()
            .unwrap()
            .triad_handle()
    }

    fn z_lock(&mut self) -> bool {
        st(self.app.ws_mut())
            .sketch_session
            .as_ref()
            .unwrap()
            .triad
            .lock_z
    }

    fn bind(&mut self, a: Action, b: Binding) {
        let mut km = self.app.ws_mut().keymap.clone();
        km.swap(a, 0, b);
        self.app.ws_mut().keymap = km;
    }
}

fn key_down(k: Key, mods: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: mods,
    }
}

fn key_up(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn rebound_triad_keys_really_work() {
    let mut r = Rig::new(true);
    r.tool(Tool::Line);
    // Ebene auf F9: die Taste wirkt, Alt allein nicht mehr.
    r.bind(Action::SketchTriadCyclePlane, Binding::Key(chord(Key::F9)));
    r.frame(Modifiers::NONE, vec![key_down(Key::F9, Modifiers::NONE)]);
    r.frame(Modifiers::NONE, vec![key_up(Key::F9)]);
    assert_eq!(r.plane(), TriadPlane::Yz);
    r.frame(Modifiers::ALT, vec![]);
    r.frame(Modifiers::NONE, vec![]);
    assert_eq!(
        r.plane(),
        TriadPlane::Yz,
        "Alt allein ist nicht mehr belegt"
    );
    // Griff auf Umschalt allein: der Tipp wirkt (und Tab nicht mehr).
    r.bind(
        Action::SketchTriadCycleHandle,
        Binding::Modifier(ModKey::Shift),
    );
    r.frame(Modifiers::SHIFT, vec![]);
    r.frame(Modifiers::NONE, vec![]);
    assert_eq!(r.handle(), Some(TriadAxis::X));
    // „Z sperren“ auf Strg allein: gehalten wirkt, losgelassen nicht; mit Umschalt dazu nicht.
    r.bind(Action::SketchLockZ, Binding::Modifier(ModKey::Ctrl));
    assert!(!r.z_lock());
    r.frame(Modifiers::CTRL, vec![]);
    assert!(r.z_lock(), "Strg gehalten sperrt Z");
    r.frame(Modifiers::CTRL, vec![]);
    assert!(r.z_lock());
    r.frame(Modifiers::NONE, vec![]);
    assert!(!r.z_lock(), "losgelassen: frei");
    r.frame(Modifiers::CTRL | Modifiers::SHIFT, vec![]);
    assert!(!r.z_lock(), "mit Umschalt dazu ist es nicht Strg allein");
    r.frame(Modifiers::NONE, vec![]);
    // Und der Standard: Pfeil hoch gehalten.
    r.bind(Action::SketchLockZ, Binding::Key(chord(Key::ArrowUp)));
    r.frame(
        Modifiers::NONE,
        vec![key_down(Key::ArrowUp, Modifiers::NONE)],
    );
    assert!(r.z_lock());
    r.frame(Modifiers::NONE, vec![key_up(Key::ArrowUp)]);
    assert!(!r.z_lock());
    // Tab ist frei geworden (zuletzt: ein unverbrauchtes Tab springt im Fokus weiter).
    r.frame(Modifiers::NONE, vec![key_down(Key::Tab, Modifiers::NONE)]);
    assert_eq!(r.handle(), Some(TriadAxis::X), "Tab ist frei geworden");
}

#[test]
fn modifier_alone_is_refused_where_nothing_reads_it() {
    for a in Action::ALL {
        let ok = matches!(
            a,
            Action::SketchTriadCyclePlane | Action::SketchTriadCycleHandle | Action::SketchLockZ
        );
        assert_eq!(a.accepts_modifier(), ok, "{a:?}");
    }
    let b = Binding::Modifier(ModKey::Alt);
    assert!(Keymap::unsupported_text(Action::Save, &b).is_some());
    assert!(Keymap::unsupported_text(Action::SketchLockZ, &b).is_none());
    // Aus der Konfiguration: übergangen, die Voreinstellung bleibt.
    let map = std::collections::BTreeMap::from([(
        "datei.speichern".to_string(),
        vec!["Alt allein".to_string()],
    )]);
    let (km, notes) = Keymap::from_overrides(&map);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(km.is_default(Action::Save));
}

#[test]
fn a_focus_change_cancels_the_modifier_tap() {
    let ctx = egui::Context::default();
    // Alt+Tab: Alt drücken, Fokus weg (Umschalttasten werden zurückgesetzt), kein Tipp.
    let mut tap = ModTap::default();
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    assert_eq!(
        tap_frame(
            &ctx,
            &mut tap,
            Modifiers::NONE,
            vec![Event::WindowFocused(false)]
        ),
        None
    );
    // Zurück ins Fenster mit noch gehaltenem Alt, dann loslassen: auch kein Tipp.
    tap_frame(
        &ctx,
        &mut tap,
        Modifiers::NONE,
        vec![Event::WindowFocused(true)],
    );
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    assert_eq!(tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]), None);
    // Danach funktioniert ein reiner Tipp wieder.
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    assert_eq!(
        tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]),
        Some(ModKey::Alt)
    );
}

#[test]
fn alt_wheel_and_alt_after_a_held_button_are_no_taps() {
    let ctx = egui::Context::default();
    let mut tap = ModTap::default();
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    tap_frame(
        &ctx,
        &mut tap,
        Modifiers::ALT,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 1.0),
            modifiers: Modifiers::ALT,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
        }],
    );
    assert_eq!(tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]), None);
    // Maustaste schon gedrückt, dann erst Alt: kein Tipp.
    let mut tap = ModTap::default();
    tap_frame(
        &ctx,
        &mut tap,
        Modifiers::NONE,
        vec![press(PointerButton::Primary, true, Modifiers::NONE)],
    );
    tap_frame(&ctx, &mut tap, Modifiers::ALT, vec![]);
    tap_frame(
        &ctx,
        &mut tap,
        Modifiers::ALT,
        vec![press(PointerButton::Primary, false, Modifiers::ALT)],
    );
    assert_eq!(tap_frame(&ctx, &mut tap, Modifiers::NONE, vec![]), None);
}

#[test]
fn tab_belongs_to_the_triad_only_in_a_3d_sketch() {
    // 2D-Skizze: Tab bleibt unverbraucht und wechselt nichts.
    let mut r = Rig::new(false);
    r.frame(Modifiers::NONE, vec![key_down(Key::Tab, Modifiers::NONE)]);
    assert!(r.tab_left, "Tab bleibt in einer 2D-Skizze liegen");
    assert_eq!(r.handle(), None);
    // 3D-Skizze mit Zeichenwerkzeug: ein echtes Tab-Ereignis wechselt den Griff und wird
    // verbraucht.
    let mut r = Rig::new(true);
    r.tool(Tool::Line);
    r.frame(Modifiers::NONE, vec![key_down(Key::Tab, Modifiers::NONE)]);
    assert!(!r.tab_left);
    assert_eq!(r.handle(), Some(TriadAxis::X));
}

/// Gesamtprüfung (M2/M5): In einer 3D-Skizze ohne sichtbare Triade (Auswahlwerkzeug) gehören
/// Tab und Alt nicht der Triade. Tab bleibt liegen, Alt schaltet keine Ebene; unsichtbar ändert
/// sich nichts. Mit einem Zeichenwerkzeug wirkt Alt wieder.
#[test]
fn tab_and_alt_leave_a_hidden_triad_alone() {
    let mut r = Rig::new(true);
    assert!(r.layout().is_none(), "Auswahlwerkzeug: keine Triade");
    r.frame(Modifiers::ALT, vec![]);
    r.frame(Modifiers::NONE, vec![]);
    assert_eq!(r.plane(), TriadPlane::Xy);
    run_action(r.app.ws_mut(), Action::SketchTriadCyclePlane);
    run_action(r.app.ws_mut(), Action::SketchTriadCycleHandle);
    assert_eq!(r.plane(), TriadPlane::Xy);
    assert_eq!(r.handle(), None);
    // Mit einem Zeichenwerkzeug wirkt Alt wieder.
    r.tool(Tool::Line);
    r.frame(Modifiers::NONE, vec![]);
    r.frame(Modifiers::ALT, vec![]);
    r.frame(Modifiers::NONE, vec![]);
    assert_eq!(r.plane(), TriadPlane::Yz);
    // Zurück im Auswahlwerkzeug bleibt Tab liegen (Fokuswechsel wie in 2D).
    r.tool(Tool::Select);
    r.frame(Modifiers::NONE, vec![]);
    r.frame(Modifiers::NONE, vec![key_down(Key::Tab, Modifiers::NONE)]);
    assert!(r.tab_left, "Tab bleibt liegen");
    assert_eq!(r.handle(), None);
}

// ---- Task 10: die Triade selbst -----------------------------------------------------------------

use glam::Vec3;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::Tool;
use planora_app::sketch_triad::{
    ARC_FRAC, Triad, TriadLayout, TriadPart, snap_angle, triad_hit, triad_point,
};
use planora_app::sketch_view::sketch3_to_screen;
use planora_math::{DQuat, DVec3, Frame};
use planora_render::OrbitCamera;
use planora_sketch::model::MAX_COORD;
use std::f64::consts::{FRAC_PI_2, PI};

const W: f32 = 1920.0;
const H: f32 = 1080.0;

fn camera(target: Vec3, distance: f32, to_camera: Vec3, up: Vec3) -> OrbitCamera {
    let mut cam = OrbitCamera {
        target,
        distance,
        ..Default::default()
    };
    cam.look_along(to_camera, up);
    cam.set_viewport(W, H);
    cam
}

fn view() -> glam::Vec2 {
    glam::Vec2::new(W, H)
}

fn near(a: DVec3, b: DVec3, tol: f64) -> bool {
    (a - b).length() < tol
}

#[test]
fn default_triad_is_sketch_plane() {
    let t = Triad::default();
    assert!(t.is_sketch_plane());
    assert_eq!(t.origin, DVec3::ZERO);
    assert_eq!(t.rot, DQuat::IDENTITY);
    assert_eq!(t.active, TriadPlane::Xy);
    assert_eq!(t.handle, None);
    assert!(!t.lock_z);
    assert_eq!(t.plane(), (DVec3::ZERO, DVec3::Z));
    assert_eq!(t.axes(), [DVec3::X, DVec3::Y, DVec3::Z]);
    // Hochgeschoben ist es nicht mehr die Skizzenebene, in der Ebene verschoben schon.
    let mut u = t;
    u.moved_to(DVec3::new(1.0, 2.0, 3.0));
    assert!(!u.is_sketch_plane());
    assert_eq!(u.plane().0, DVec3::new(1.0, 2.0, 3.0));
    u.moved_to(DVec3::new(1.0, 2.0, 1e-12));
    assert!(u.is_sketch_plane());
    u.cycle_plane();
    assert!(!u.is_sketch_plane(), "YZ ist nicht die Skizzenebene");
}

#[test]
fn cycle_plane_xy_yz_zx_xy() {
    let mut t = Triad::default();
    let mut seen = vec![(t.active, t.plane().1)];
    for _ in 0..3 {
        t.cycle_plane();
        seen.push((t.active, t.plane().1));
    }
    assert_eq!(
        seen,
        vec![
            (TriadPlane::Xy, DVec3::Z),
            (TriadPlane::Yz, DVec3::X),
            (TriadPlane::Zx, DVec3::Y),
            (TriadPlane::Xy, DVec3::Z),
        ]
    );
    let mut handles = Vec::new();
    for _ in 0..4 {
        t.cycle_handle();
        handles.push(t.handle);
    }
    assert_eq!(
        handles,
        vec![
            Some(TriadAxis::X),
            Some(TriadAxis::Y),
            Some(TriadAxis::Z),
            Some(TriadAxis::X)
        ]
    );
}

#[test]
fn rotate_x_90_turns_xy_into_xz() {
    let mut t = Triad::default();
    t.moved_to(DVec3::new(2.0, 0.0, 0.0));
    t.rotate(TriadAxis::X, FRAC_PI_2);
    let (p, n) = t.plane();
    assert_eq!(p, DVec3::new(2.0, 0.0, 0.0));
    // Die XY-Ebene der Triade steht jetzt in der XZ-Ebene der Skizze.
    assert!(near(n.abs(), DVec3::Y, 1e-12), "{n:?}");
    let [x, y, z] = t.axes();
    assert!(near(x, DVec3::X, 1e-12));
    assert!(near(y, DVec3::Z, 1e-12), "{y:?}");
    assert!(near(z, -DVec3::Y, 1e-12), "{z:?}");
    assert!(!t.is_sketch_plane());
    // Zurückgedreht ist sie wieder die Skizzenebene (z = 0 vorausgesetzt).
    t.moved_to(DVec3::new(2.0, 0.0, 0.0));
    t.rotate(TriadAxis::X, -FRAC_PI_2);
    assert!(t.is_sketch_plane());
    // Gedreht wird um die eigene (schon gedrehte) Achse der Triade.
    let mut t = Triad::default();
    t.rotate(TriadAxis::Z, FRAC_PI_2);
    t.rotate(TriadAxis::X, FRAC_PI_2);
    assert!(near(t.axes()[0], DVec3::Y, 1e-12));
    assert!(near(t.plane().1, DVec3::X, 1e-12), "{:?}", t.plane().1);
}

#[test]
fn reset_keeps_origin_drops_rotation() {
    let mut t = Triad::default();
    t.moved_to(DVec3::new(4.0, -1.0, 6.0));
    t.rotate(TriadAxis::Y, PI / 6.0);
    t.cycle_plane();
    t.cycle_handle();
    t.reset_orientation();
    assert_eq!(t.origin, DVec3::new(4.0, -1.0, 6.0));
    assert_eq!(t.rot, DQuat::IDENTITY);
    assert_eq!(t.axes(), [DVec3::X, DVec3::Y, DVec3::Z]);
    assert_eq!(t.active, TriadPlane::Yz, "die aktive Ebene bleibt");
    assert_eq!(t.handle, Some(TriadAxis::X));
}

#[test]
fn axis_snap_within_8px_returns_axis() {
    // Von oben auf die Skizzenebene.
    let cam = camera(Vec3::ZERO, 100.0, Vec3::Z, Vec3::Y);
    let f = Frame::WORLD;
    let screen = |p: DVec3| sketch3_to_screen(&cam, &f, view(), p).unwrap();
    let t = Triad::default();
    // 5 Bildpunkte neben der X-Achse: rastet ein, der Punkt liegt genau auf der Achse.
    let px = screen(DVec3::new(10.0, 0.0, 0.0)) + glam::Vec2::new(0.0, 5.0);
    let (p, axis) = triad_point(&t, &cam, &f, view(), px, None).unwrap();
    assert_eq!(axis, Some(TriadAxis::X));
    assert!(near(p, DVec3::new(10.0, 0.0, 0.0), 1e-3), "{p:?}");
    assert_eq!((p.y, p.z), (0.0, 0.0));
    // 12 Bildpunkte daneben: frei auf der Ebene.
    let px = screen(DVec3::new(10.0, 0.0, 0.0)) + glam::Vec2::new(0.0, 12.0);
    let (p, axis) = triad_point(&t, &cam, &f, view(), px, None).unwrap();
    assert_eq!(axis, None);
    assert!(p.y < -0.1 && p.z.abs() < 1e-9, "{p:?}");
    // Y-Achse ebenso; die Triade woanders.
    let mut t = Triad::default();
    t.moved_to(DVec3::new(5.0, 5.0, 0.0));
    let px = screen(DVec3::new(5.0, 20.0, 0.0)) + glam::Vec2::new(-7.0, 0.0);
    let (p, axis) = triad_point(&t, &cam, &f, view(), px, None).unwrap();
    assert_eq!(axis, Some(TriadAxis::Y));
    assert!(near(p, DVec3::new(5.0, 20.0, 0.0), 1e-3), "{p:?}");
    // Auf der Ebene YZ (von vorn gesehen) rastet die Z-Achse ein.
    let cam = camera(Vec3::ZERO, 100.0, Vec3::X, Vec3::Z);
    let screen = |p: DVec3| sketch3_to_screen(&cam, &f, view(), p).unwrap();
    let mut t = Triad::default();
    t.cycle_plane();
    let px = screen(DVec3::new(0.0, 0.0, 15.0)) + glam::Vec2::new(4.0, 0.0);
    let (p, axis) = triad_point(&t, &cam, &f, view(), px, None).unwrap();
    assert_eq!(axis, Some(TriadAxis::Z));
    assert!(near(p, DVec3::new(0.0, 0.0, 15.0), 1e-3), "{p:?}");
}

#[test]
fn lock_z_moves_only_along_normal() {
    // Von der Seite (Blick entlang +Y): die Höhe ist gut zu sehen.
    let cam = camera(Vec3::ZERO, 100.0, -Vec3::Y, Vec3::Z);
    let f = Frame::WORLD;
    let screen = |p: DVec3| sketch3_to_screen(&cam, &f, view(), p).unwrap();
    let t = Triad::default();
    let base = DVec3::new(3.0, 4.0, 0.0);
    let px = screen(DVec3::new(3.0, 4.0, 7.0));
    let (p, axis) = triad_point(&t, &cam, &f, view(), px, Some(base)).unwrap();
    assert!(near(p, DVec3::new(3.0, 4.0, 7.0), 1e-3), "{p:?}");
    assert_eq!(axis, None);
    // Seitlich daneben: x und y bleiben genau, nur die Höhe folgt.
    let (p, _) = triad_point(
        &t,
        &cam,
        &f,
        view(),
        px + glam::Vec2::new(40.0, 0.0),
        Some(base),
    )
    .unwrap();
    assert_eq!((p.x, p.y), (3.0, 4.0));
    // In der Perspektive liegt die nächste Stelle zum Strahl daneben etwas anders.
    assert!((p.z - 7.0).abs() < 0.05, "{p:?}");
    // Ohne Sperre ist diese Ebene streifend gesehen: kein Punkt (Review Focus 3).
    assert!(triad_point(&t, &cam, &f, view(), px, None).is_none());
    // Mit Sperre, aber Blick entlang der Normale (von oben): keine Höhe ablesbar.
    let top = camera(Vec3::ZERO, 100.0, Vec3::Z, Vec3::Y);
    let px = sketch3_to_screen(&top, &f, view(), base).unwrap();
    assert!(triad_point(&t, &top, &f, view(), px, Some(base)).is_none());
    // Fast streifend (6° über der Ebene): nie ein Punkt über MAX_COORD.
    let flat = camera(
        Vec3::ZERO,
        100.0,
        Vec3::new(0.0, -1.0, (6.0f32).to_radians().tan()),
        Vec3::Z,
    );
    for i in 0..=24 {
        for j in 0..=24 {
            let px = glam::Vec2::new(W * i as f32 / 24.0, H * j as f32 / 24.0);
            for lock in [None, Some(base)] {
                if let Some((p, _)) = triad_point(&t, &flat, &f, view(), px, lock) {
                    assert!(p.is_finite() && p.abs().max_element() <= MAX_COORD, "{p:?}");
                }
            }
        }
    }
}

fn hand_layout() -> TriadLayout {
    let c = Pos2::new(100.0, 100.0);
    TriadLayout {
        center: c,
        tips: [
            Some(Pos2::new(180.0, 100.0)),
            Some(Pos2::new(100.0, 20.0)),
            Some(Pos2::new(40.0, 140.0)),
        ],
        // Das Quadrat der XY-Ebene liegt hier dicht an der X-Achse.
        squares: [
            Some([
                Pos2::new(105.0, 99.0),
                Pos2::new(130.0, 99.0),
                Pos2::new(130.0, 72.0),
                Pos2::new(105.0, 72.0),
            ]),
            None,
            None,
        ],
        arcs: [
            vec![Pos2::new(100.0, 36.0), Pos2::new(70.0, 50.0)],
            Vec::new(),
            vec![Pos2::new(164.0, 100.0), Pos2::new(145.0, 55.0)],
        ],
        show_planes: true,
        show_handles: true,
    }
}

#[test]
fn hit_plane_square_before_axis() {
    let mut l = hand_layout();
    // Im Quadrat und zugleich 2 Bildpunkte neben der X-Achse: die Ebene gewinnt.
    let p = Pos2::new(115.0, 98.0);
    assert_eq!(triad_hit(&l, p), Some(TriadPart::Plane(TriadPlane::Xy)));
    assert_eq!(
        triad_hit(&l, Pos2::new(120.0, 80.0)),
        Some(TriadPart::Plane(TriadPlane::Xy))
    );
    assert_eq!(
        triad_hit(&l, Pos2::new(175.0, 103.0)),
        Some(TriadPart::Axis(TriadAxis::X))
    );
    assert_eq!(
        triad_hit(&l, Pos2::new(155.0, 76.0)),
        Some(TriadPart::Handle(TriadAxis::Z))
    );
    assert_eq!(triad_hit(&l, Pos2::new(400.0, 400.0)), None);
    // Ohne Ebenen und Drehgriffe zählen nur die Achsen.
    l.show_planes = false;
    l.show_handles = false;
    assert_eq!(triad_hit(&l, p), Some(TriadPart::Axis(TriadAxis::X)));
    assert_eq!(triad_hit(&l, Pos2::new(155.0, 76.0)), None);
}

#[test]
fn angle_snap_catches_15_degree_steps() {
    let d = |x: f64| x.to_radians();
    assert!((snap_angle(d(44.0), false) - d(45.0)).abs() < 1e-12);
    assert!((snap_angle(d(-91.5), false) - d(-90.0)).abs() < 1e-12);
    // Zwischen den Stufen ganze Grad.
    assert!((snap_angle(d(37.4), false) - d(37.0)).abs() < 1e-12);
    // Mit Strg frei.
    assert_eq!(snap_angle(d(44.0), true), d(44.0));
}

// ---- Task 10: in der App ------------------------------------------------------------------------

impl Rig {
    fn tool(&mut self, tool: Tool) {
        let (se, sk) = st(self.app.ws_mut()).session_sketch_mut().unwrap();
        se.choose_tool(sk, tool);
    }

    fn kind(&mut self, kind: ToolKind) {
        let (se, sk) = st(self.app.ws_mut()).session_sketch_mut().unwrap();
        se.choose_kind(sk, kind);
    }

    fn triad(&mut self) -> Triad {
        st(self.app.ws_mut()).sketch_session.as_ref().unwrap().triad
    }

    fn layout(&mut self) -> Option<TriadLayout> {
        st(self.app.ws_mut())
            .sketch_session
            .as_ref()
            .unwrap()
            .triad_ui
            .layout
            .clone()
    }

    /// Ein Bild wie [`Rig::frame`], liefert die gezeichneten Texte mit ihrer Mitte.
    fn texts(&mut self, events: Vec<Event>) -> Vec<(String, Pos2)> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut found = Vec::new();
        for s in &out.shapes {
            collect_text(&s.shape, &mut found);
        }
        out.drop_without_applying_deltas();
        found
    }

    fn click(&mut self, at: Pos2, button: PointerButton) {
        let ev = |pressed| Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(Modifiers::NONE, vec![Event::PointerMoved(at)]);
        self.frame(Modifiers::NONE, vec![Event::PointerMoved(at), ev(true)]);
        self.frame(Modifiers::NONE, vec![ev(false)]);
        self.frame(Modifiers::NONE, vec![]);
    }

    fn points(&mut self) -> usize {
        st(self.app.ws_mut())
            .session_sketch()
            .unwrap()
            .points()
            .count()
    }
}

fn centroid(q: [Pos2; 4]) -> Pos2 {
    let s = q.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2());
    (s / 4.0).to_pos2()
}

fn button(at: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn triad_shows_only_with_a_3d_tool_in_a_3d_sketch() {
    let mut r = Rig::new(true);
    assert!(r.layout().is_none(), "Auswahlwerkzeug: keine Triade");
    for tool in [
        Tool::Line,
        Tool::Point,
        Tool::Ellipse,
        Tool::Text,
        Tool::Spline,
        Tool::Polygon,
        Tool::Slot,
    ] {
        r.tool(tool);
        r.frame(Modifiers::NONE, vec![]);
        assert!(r.layout().is_some(), "{tool:?}");
    }
    // Alle Arten zeichnen in 3D (Task H).
    for (kind, shown) in [
        (ToolKind::Arc3P, true),
        (ToolKind::ArcCenter, true),
        (ToolKind::Rect2P, true),
        (ToolKind::Rect3P, true),
        (ToolKind::Circle2P, true),
        (ToolKind::CircleCenter, true),
        (ToolKind::Conic, true),
        (ToolKind::SplineControl, true),
    ] {
        r.kind(kind);
        r.frame(Modifiers::NONE, vec![]);
        assert_eq!(r.layout().is_some(), shown, "{kind:?}");
    }
    for tool in [Tool::Trim, Tool::Dimension, Tool::Select] {
        r.tool(tool);
        r.frame(Modifiers::NONE, vec![]);
        assert!(r.layout().is_none(), "{tool:?}");
    }
    // Ohne den Haken nie.
    let mut r = Rig::new(false);
    r.tool(Tool::Line);
    r.frame(Modifiers::NONE, vec![]);
    assert!(r.layout().is_none());
}

#[test]
fn click_on_a_plane_square_activates_it_and_sets_no_point() {
    let mut r = Rig::new(true);
    r.tool(Tool::Line);
    // Von oben stehen YZ und ZX auf der Kante; schräg gesehen sind alle drei Quadrate offen.
    st(r.app.ws_mut()).camera.orbit(60.0, 40.0);
    r.frame(Modifiers::NONE, vec![]);
    let l = r.layout().unwrap();
    let n = r.points();
    let at = centroid(l.squares[1].expect("Quadrat YZ"));
    assert_eq!(triad_hit(&l, at), Some(TriadPart::Plane(TriadPlane::Yz)));
    r.click(at, PointerButton::Primary);
    assert_eq!(r.triad().active, TriadPlane::Yz);
    assert_eq!(r.points(), n, "der Klick auf die Triade setzt keinen Punkt");
    // Ohne „Ebenen anzeigen“ ist da keine Ebene zu treffen.
    assert!(r.app.ws_mut().set_triad_view(false, true));
    // Der Tab übernimmt den Stand am Ende eines Bilds, das nächste zeichnet danach.
    r.frame(Modifiers::NONE, vec![]);
    r.frame(Modifiers::NONE, vec![]);
    let l = r.layout().unwrap();
    assert!(!l.show_planes);
    assert!(triad_hit(&l, at).is_none_or(|p| !matches!(p, TriadPart::Plane(_))));
}

#[test]
fn dragging_a_handle_rotates_with_angle_snap() {
    let mut r = Rig::new(true);
    r.tool(Tool::Line);
    r.frame(Modifiers::NONE, vec![]);
    let l = r.layout().unwrap();
    // Die Skizze ist von oben zu sehen: Punkte der Ebene XY um die Mitte über die Achsspitzen.
    let (ex, ey) = (l.tips[0].unwrap() - l.center, l.tips[1].unwrap() - l.center);
    let on = |deg: f32, rho: f32| {
        let a = deg.to_radians();
        l.center + (ex * a.cos() + ey * a.sin()) * rho
    };
    let press = on(45.0, ARC_FRAC);
    assert_eq!(triad_hit(&l, press), Some(TriadPart::Handle(TriadAxis::Z)));
    let to = on(89.0, ARC_FRAC);
    let drag = |r: &mut Rig| {
        r.frame(Modifiers::NONE, vec![Event::PointerMoved(press)]);
        r.frame(
            Modifiers::NONE,
            vec![
                Event::PointerMoved(press),
                button(press, PointerButton::Primary, true),
            ],
        );
        for k in 1..=6 {
            let p = press + (to - press) * (k as f32 / 6.0);
            r.frame(Modifiers::NONE, vec![Event::PointerMoved(p)]);
        }
    };
    // Vom Griff (45°) um 44° weiter: gefangen auf 45°.
    drag(&mut r);
    let texts = r.texts(vec![Event::PointerMoved(to)]);
    assert!(
        texts.iter().any(|(s, _)| s.contains("45")),
        "ein Feld zeigt den Winkel: {:?}",
        texts.iter().map(|t| &t.0).collect::<Vec<_>>()
    );
    let half = 0.5f64.sqrt();
    let x = r.triad().axes()[0];
    assert!(near(x, DVec3::new(half, half, 0.0), 1e-9), "{x:?}");
    // Rechtsklick bricht den Zug ab: die Drehung ist zurück, das Werkzeug bleibt.
    r.frame(
        Modifiers::NONE,
        vec![button(to, PointerButton::Secondary, true)],
    );
    assert_eq!(r.triad().rot, DQuat::IDENTITY);
    r.frame(
        Modifiers::NONE,
        vec![
            button(to, PointerButton::Primary, false),
            button(to, PointerButton::Secondary, false),
        ],
    );
    r.frame(Modifiers::NONE, vec![]);
    // Noch einmal, diesmal losgelassen: bleibt gedreht.
    drag(&mut r);
    r.frame(
        Modifiers::NONE,
        vec![button(to, PointerButton::Primary, false)],
    );
    r.frame(Modifiers::NONE, vec![]);
    let x = r.triad().axes()[0];
    assert!(near(x, DVec3::new(half, half, 0.0), 1e-9), "{x:?}");
    assert_eq!(r.points(), 0, "der Zug setzt keinen Punkt");
    let se = st(r.app.ws_mut()).sketch_session.as_ref().unwrap();
    assert_eq!(se.tool, Tool::Line);
    assert!(se.triad_ui.drag.is_none());
}

#[test]
fn right_click_on_the_triad_opens_its_menu() {
    // Im Text-Werkzeug beendet ein Rechtsklick in die Ansicht das Werkzeug; auf der Triade nicht.
    let mut r = Rig::new(true);
    r.tool(Tool::Text);
    r.frame(Modifiers::NONE, vec![]);
    // Erst etwas drehen, damit „zurücksetzen“ etwas tut.
    st(r.app.ws_mut())
        .sketch_session
        .as_mut()
        .unwrap()
        .triad
        .rotate(TriadAxis::Y, 0.3);
    r.frame(Modifiers::NONE, vec![]);
    let l2 = r.layout().unwrap();
    let at = l2.center + (l2.tips[0].unwrap() - l2.center) * 0.9;
    let open = |r: &mut Rig| {
        r.click(at, PointerButton::Secondary);
        r.texts(vec![])
    };
    let find = |texts: &[(String, Pos2)], s: &str| {
        texts
            .iter()
            .find(|(t, _)| t == s)
            .map(|t| t.1)
            .unwrap_or_else(|| panic!("„{s}“ fehlt"))
    };
    let texts = open(&mut r);
    for s in [
        "Triadenausrichtung zurücksetzen",
        "Drehgriffe anzeigen",
        "Ebenen anzeigen",
    ] {
        find(&texts, s);
    }
    assert_eq!(
        st(r.app.ws_mut()).sketch_session.as_ref().unwrap().tool,
        Tool::Text,
        "der Rechtsklick auf die Triade bricht das Werkzeug nicht ab"
    );
    r.click(find(&texts, "Ebenen anzeigen"), PointerButton::Primary);
    assert!(!r.app.ws_mut().config.show_triad_planes);
    let path = r.app.ws_mut().config_path.clone();
    assert!(!Config::read_from(&path).unwrap().show_triad_planes);
    assert!(!r.layout().unwrap().show_planes);
    // Wieder öffnen, zurücksetzen.
    let texts = open(&mut r);
    r.click(
        find(&texts, "Triadenausrichtung zurücksetzen"),
        PointerButton::Primary,
    );
    assert_eq!(r.triad().rot, DQuat::IDENTITY);
    // Drehgriffe aus.
    let texts = open(&mut r);
    r.click(find(&texts, "Drehgriffe anzeigen"), PointerButton::Primary);
    assert!(!r.app.ws_mut().config.show_triad_handles);
    assert!(!r.layout().unwrap().show_handles);
    assert_eq!(r.points(), 0);
    // Gegenprobe: neben der Triade ist der Rechtsklick wie sonst ein Abbruch.
    r.click(l2.center + vec2(400.0, 0.0), PointerButton::Secondary);
    assert_eq!(
        st(r.app.ws_mut()).sketch_session.as_ref().unwrap().tool,
        Tool::Select
    );
}

#[test]
fn tab_handle_takes_a_typed_angle() {
    let mut r = Rig::new(true);
    r.tool(Tool::Line);
    r.frame(Modifiers::NONE, vec![]);
    r.frame(Modifiers::NONE, vec![key_down(Key::Tab, Modifiers::NONE)]);
    assert_eq!(r.handle(), Some(TriadAxis::X));
    let texts = r.texts(vec![]);
    assert!(texts.iter().any(|(s, _)| s == "X-Winkel"), "Feld fehlt");
    r.frame(Modifiers::NONE, vec![Event::Text("90".into())]);
    r.frame(Modifiers::NONE, vec![key_down(Key::Enter, Modifiers::NONE)]);
    r.frame(Modifiers::NONE, vec![key_up(Key::Enter)]);
    let t = r.triad();
    assert!(near(t.axes()[1], DVec3::Z, 1e-9), "{:?}", t.axes());
    assert_eq!(r.points(), 0);
}
