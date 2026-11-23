//! Fenster-Baustein für abgedockte Inhalte (`detach::show_window`) im Harness mit echten
//! Kindfenstern: eigener Inhalt, Andocken, Schließen, Ziehen und Lage-Korrektur; dazu abdockbare
//! Dialoge (`Dialog::dock_key`).

mod common;

use common::{Screen, find, join};
use egui::{Event, PointerButton, ViewportCommand, ViewportEvent, pos2, vec2};
use planora_app::detach::{self, DetachState, Head, Place, WindowEvent, WindowSpec};
use planora_app::dialog::{Dialog, DialogSize};
use planora_app::icons::{self, Icon};
use planora_project::config::DetachedWindow;
use std::cell::RefCell;

const CONTENT: &str = "Inhalt des Baums";

fn spec() -> WindowSpec<'static> {
    WindowSpec {
        place: Place::Tree,
        title: "Browser",
        inner_size: vec2(320.0, 480.0),
        resizable: true,
        focus_on_open: false,
        closable: false,
        head: Head::Title,
    }
}

/// Zeigt den Baum als eigenes Fenster und sammelt die Ereignisse.
fn tree_window<'a>(
    state: &'a RefCell<DetachState>,
    events: &'a RefCell<Vec<WindowEvent>>,
) -> impl FnMut(&egui::Context) + 'a {
    move |ctx| {
        egui::Area::new(egui::Id::unique("haupt")).show(ctx, |ui| {
            ui.label("Hauptfenster");
        });
        let (r, ev) = detach::show_window(ctx, &mut state.borrow_mut(), spec(), |ui| {
            ui.label(CONTENT);
            7
        });
        assert_eq!(r, Some(7), "Inhalt lief");
        if ev != WindowEvent::None {
            events.borrow_mut().push(ev);
        }
    }
}

fn press(at: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn window_shows_contents_in_child_viewport() {
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let state = RefCell::new(DetachState::default());
    let events = RefCell::new(Vec::new());
    let mut f = tree_window(&state, &events);
    let main = s.settle(3, &mut f);
    let id = detach::viewport_id(Place::Tree);
    let child = s.child_seen(id);
    assert!(find(&child, CONTENT).is_some(), "Kind: {}", join(&child));
    assert!(find(&child, "Browser").is_some(), "Titel: {}", join(&child));
    assert!(find(&main, CONTENT).is_none(), "Haupt: {}", join(&main));
    let b = s.child_builder(id).expect("Builder");
    assert_eq!(b.title.as_deref(), Some("PlanoraCAD – Browser"));
    assert_eq!(b.decorations, Some(false));
    assert_eq!(b.taskbar, Some(false));
    assert_eq!(b.resizable, Some(true));
    assert_eq!(b.active, Some(false));
    assert_eq!(b.inner_size, Some(vec2(320.0, 480.0)));
    assert_eq!(b.position, None, "ohne gemerkte Lage keine Lage");
    assert!(events.borrow().is_empty());
}

#[test]
fn dock_click_reports_dock() {
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let state = RefCell::new(DetachState::default());
    let events = RefCell::new(Vec::new());
    let mut f = tree_window(&state, &events);
    s.settle(3, &mut f);
    let id = detach::viewport_id(Place::Tree);
    let child = s.child_seen(id);
    let dock = find(&child, &icons::glyph(Icon::Dock).to_string())
        .unwrap_or_else(|| panic!("Symbol Andocken fehlt: {}", join(&child)))
        .rect
        .center();
    s.click_child(id, dock, &mut f);
    s.settle(2, &mut f);
    assert_eq!(*events.borrow(), vec![WindowEvent::DockClicked]);

    // Doppelklick auf den Titel dockt ebenfalls an. Vorher warten, damit egui ihn nicht mit dem
    // Klick oben zu einem Dreifachklick zählt.
    s.settle(15, &mut f);
    events.borrow_mut().clear();
    let title = find(&child, "Browser").expect("Titel").rect.center();
    s.frame_child(id, vec![Event::PointerMoved(title)], &mut f);
    for _ in 0..2 {
        s.frame_child(id, vec![press(title, true)], &mut f);
        s.frame_child(id, vec![press(title, false)], &mut f);
    }
    s.settle(2, &mut f);
    assert!(
        events.borrow().contains(&WindowEvent::DockClicked),
        "{:?}",
        events.borrow()
    );
    assert!(
        !s.child_commands(id).contains(&ViewportCommand::StartDrag),
        "Klicken ist kein Ziehen"
    );
}

#[test]
fn close_request_is_cancelled_and_reported() {
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let state = RefCell::new(DetachState::default());
    let events = RefCell::new(Vec::new());
    let mut f = tree_window(&state, &events);
    s.settle(2, &mut f);
    let id = detach::viewport_id(Place::Tree);
    assert!(!s.child_commands(id).contains(&ViewportCommand::CancelClose));
    s.child_info(id, |i| i.events = vec![ViewportEvent::Close]);
    s.frame(vec![], &mut f);
    assert!(
        s.child_commands(id).contains(&ViewportCommand::CancelClose),
        "{:?}",
        s.child_commands(id)
    );
    assert_eq!(*events.borrow(), vec![WindowEvent::CloseRequested]);
    s.settle(2, &mut f);
    assert_eq!(events.borrow().len(), 1, "nur einmal gemeldet");
}

#[test]
fn drag_end_stores_physical_rect() {
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let ppp = 1.25;
    let start = egui::Rect::from_min_size(pos2(1600.0, 40.0), vec2(320.0, 480.0));
    let state = RefCell::new(DetachState::default());
    state
        .borrow_mut()
        .set(Place::Tree, detach::to_physical(start, ppp));
    state.borrow_mut().take_dirty();
    let events = RefCell::new(Vec::new());
    let id = detach::viewport_id(Place::Tree);
    s.child_info(id, |i| {
        i.native_pixels_per_point = Some(ppp);
        i.outer_rect = Some(start);
        i.inner_rect = Some(start);
    });
    let mut f = tree_window(&state, &events);
    s.settle(3, &mut f);
    assert!(!state.borrow_mut().take_dirty(), "Öffnen ändert nichts");
    let title = find(&s.child_seen(id), "Browser")
        .expect("Titel")
        .rect
        .center();
    s.frame_child(id, vec![Event::PointerMoved(title)], &mut f);
    s.frame_child(id, vec![press(title, true)], &mut f);
    s.frame_child(
        id,
        vec![Event::PointerMoved(title + vec2(30.0, 10.0))],
        &mut f,
    );
    assert!(
        s.child_commands(id).contains(&ViewportCommand::StartDrag),
        "{:?}",
        s.child_commands(id)
    );
    // Das Betriebssystem hat das Fenster verschoben; während die Maus noch gedrückt ist, wird
    // nichts gemerkt.
    let moved = start.translate(vec2(-900.0, 120.0));
    s.child_info(id, |i| {
        i.outer_rect = Some(moved);
        i.inner_rect = Some(moved);
    });
    s.frame(vec![], &mut f);
    assert!(
        !state.borrow_mut().take_dirty(),
        "nicht bei gedrückter Maus"
    );
    s.frame_child(id, vec![press(title + vec2(30.0, 10.0), false)], &mut f);
    s.settle(2, &mut f);
    let want = detach::to_physical(moved, ppp);
    assert_eq!(state.borrow().get(Place::Tree), Some(want));
    assert_eq!(
        want,
        DetachedWindow {
            x: 875.0,
            y: 200.0,
            width: 400.0,
            height: 600.0
        }
    );
    assert!(state.borrow_mut().take_dirty());
    assert!(events.borrow().is_empty(), "{:?}", events.borrow());
}

#[test]
fn second_frame_corrects_position_once() {
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    // Gewollt: physisch (3000, 100) auf einem Monitor mit 125 %. Der Builder setzt die Lage
    // logisch mit dem Faktor des Hauptfensters (1,0); winit rechnet sie mit dem Faktor des
    // neuen Fensters um und landet bei physisch (3750, 125).
    let wanted = DetachedWindow {
        x: 3000.0,
        y: 100.0,
        width: 400.0,
        height: 600.0,
    };
    let state = RefCell::new(DetachState::default());
    state.borrow_mut().set(Place::Tree, wanted);
    let events = RefCell::new(Vec::new());
    let id = detach::viewport_id(Place::Tree);
    let wrong = egui::Rect::from_min_size(pos2(3000.0, 100.0), vec2(320.0, 480.0));
    s.child_info(id, |i| {
        i.native_pixels_per_point = Some(1.25);
        i.outer_rect = Some(wrong);
        i.inner_rect = Some(wrong);
    });
    let mut f = tree_window(&state, &events);
    s.settle(5, &mut f);
    let moves: Vec<_> = s
        .child_commands(id)
        .into_iter()
        .filter(|c| matches!(c, ViewportCommand::OuterPosition(_)))
        .collect();
    assert_eq!(
        moves,
        vec![ViewportCommand::OuterPosition(pos2(2400.0, 80.0))]
    );
    assert_eq!(
        s.child_builder(id).expect("Builder").position,
        Some(pos2(3000.0, 100.0)),
        "Lage aus der gemerkten (Punkte des Hauptfensters)"
    );
    assert_eq!(
        state.borrow().get(Place::Tree),
        Some(wanted),
        "nichts überschrieben"
    );
}

#[test]
fn matching_position_is_not_corrected() {
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let at = egui::Rect::from_min_size(pos2(-1500.0, 40.0), vec2(320.0, 480.0));
    let state = RefCell::new(DetachState::default());
    state
        .borrow_mut()
        .set(Place::Tree, detach::to_physical(at, 1.0));
    let events = RefCell::new(Vec::new());
    let id = detach::viewport_id(Place::Tree);
    s.child_info(id, |i| {
        i.native_pixels_per_point = Some(1.0);
        i.outer_rect = Some(at.translate(vec2(1.5, -1.5)));
    });
    let mut f = tree_window(&state, &events);
    s.settle(5, &mut f);
    assert!(
        !s.child_commands(id)
            .iter()
            .any(|c| matches!(c, ViewportCommand::OuterPosition(_))),
        "{:?}",
        s.child_commands(id)
    );
}

#[test]
fn embed_viewports_never_opens_window() {
    let mut s = Screen::new(1920.0, 1080.0);
    assert!(!detach::can_detach(&s.ctx));
    let mut state = DetachState::default();
    let mut result = None;
    let seen = s.frame(vec![], &mut |ctx| {
        result = Some(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || {
                detach::show_window(ctx, &mut state, spec(), |ui| {
                    ui.label(CONTENT);
                })
            },
        )));
    });
    let result = result.expect("lief");
    if cfg!(debug_assertions) {
        assert!(result.is_err(), "debug_assert schlägt an");
    } else {
        let (r, ev) = result.expect("kein Abbruch");
        assert!(r.is_none());
        assert_eq!(ev, WindowEvent::None);
    }
    assert!(find(&seen, CONTENT).is_none(), "{}", join(&seen));
    assert!(find(&seen, "Browser").is_none(), "{}", join(&seen));
}

#[test]
fn harness_with_viewports_can_detach() {
    let s = Screen::with_viewports(1920.0, 1080.0);
    assert!(detach::can_detach(&s.ctx));
}

// ---------------------------------------------------------------------------------------------
// Abdockbare Dialoge
// ---------------------------------------------------------------------------------------------

const BLEND_TITLE: &str = "Verrunden";
const BLEND_CONTENT: &str = "Radius der Rundung";
const BLEND_KEY: &str = "fillet";
const BLEND: Place = Place::Dialog(BLEND_KEY);
const FOOTER: &str = "Fußzeile OK";

/// Zustand eines Dialogtests: Abdock-Zustand, offen, Zeilen des Inhalts.
struct DialogTest {
    state: RefCell<DetachState>,
    open: RefCell<bool>,
    lines: RefCell<usize>,
    /// Im ersten Durchgang jedes Bilds verwerfen (mehrere Durchgänge je Bild).
    discard_first_pass: bool,
}

impl DialogTest {
    fn new(saved: Option<DetachedWindow>) -> DialogTest {
        let mut state = DetachState::default();
        if let Some(w) = saved {
            state.set(BLEND, w);
        }
        state.take_dirty();
        DialogTest {
            state: RefCell::new(state),
            open: RefCell::new(true),
            lines: RefCell::new(1),
            discard_first_pass: false,
        }
    }

    /// Ein Bild wie in `PlanoraApp::ui`: Zustand in `ctx.data`, Dialog, Zustand zurück.
    fn frame(&self) -> impl FnMut(&egui::Context) + '_ {
        move |ctx| {
            if self.discard_first_pass && ctx.current_pass_index() == 0 {
                ctx.request_discard("Test: zweiter Durchgang");
            }
            detach::install(ctx, &mut self.state.borrow_mut());
            let mut open = *self.open.borrow();
            let lines = *self.lines.borrow();
            Dialog::new(BLEND_TITLE, DialogSize::Normal)
                .dock_key(BLEND_KEY)
                .open(&mut open)
                .show_with_footer(
                    ctx,
                    |ui| {
                        for i in 0..lines {
                            ui.label(format!("{BLEND_CONTENT} {i}"));
                        }
                    },
                    |ui| {
                        ui.label(FOOTER);
                    },
                );
            *self.open.borrow_mut() = open;
            detach::uninstall(ctx, &mut self.state.borrow_mut());
        }
    }
}

/// Gemerkte Lage auf dem nachgebildeten Hauptmonitor (`Screen::with_viewports`).
fn on_main_monitor() -> DetachedWindow {
    DetachedWindow {
        x: 200.0,
        y: 150.0,
        width: 480.0,
        height: 300.0,
    }
}

fn glyph(icon: Icon) -> String {
    icons::glyph(icon).to_string()
}

#[test]
fn undock_symbol_only_with_viewports() {
    // Eingebettete Viewports (alle übrigen Tests, Plattformen ohne Fenster): kein Symbol.
    let t = DialogTest::new(None);
    let mut s = Screen::new(1920.0, 1080.0);
    let mut f = t.frame();
    let seen = s.settle(3, &mut f);
    assert!(find(&seen, BLEND_TITLE).is_some(), "{}", join(&seen));
    assert!(
        find(&seen, &glyph(Icon::Undock)).is_none(),
        "{}",
        join(&seen)
    );
    assert!(find(&seen, "Abdocken").is_none(), "{}", join(&seen));
    let close = find(&seen, &glyph(Icon::Close))
        .expect("Schließen")
        .rect
        .center();
    s.frame(vec![Event::PointerMoved(close)], &mut f);
    let seen = s.settle(30, &mut f);
    assert!(
        find(&seen, "Abdocken").is_none(),
        "Tooltip: {}",
        join(&seen)
    );
    drop(f);

    // Ohne Abdock-Zustand in `ctx.data` (anderer Zusammenhang): ebenfalls kein Symbol.
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let mut plain = |ctx: &egui::Context| {
        Dialog::new(BLEND_TITLE, DialogSize::Normal)
            .dock_key(BLEND_KEY)
            .show(ctx, |ui| ui.label(BLEND_CONTENT));
    };
    let seen = s.settle(3, &mut plain);
    assert!(find(&seen, BLEND_TITLE).is_some(), "{}", join(&seen));
    assert!(
        find(&seen, &glyph(Icon::Undock)).is_none(),
        "{}",
        join(&seen)
    );

    // Mit Fenstern und Zustand: Symbol links neben dem Schließen-Symbol.
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let mut f = t.frame();
    let seen = s.settle(3, &mut f);
    let undock = find(&seen, &glyph(Icon::Undock)).expect("Symbol Abdocken");
    let close = find(&seen, &glyph(Icon::Close)).expect("Schließen");
    assert!(undock.rect.max.x <= close.rect.min.x, "links vom Schließen");
    assert!((undock.rect.center().y - close.rect.center().y).abs() < 1.0);
}

#[test]
fn undock_click_detaches_at_current_place() {
    let t = DialogTest::new(None);
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let mut f = t.frame();
    let seen = s.settle(3, &mut f);
    let title = find(&seen, BLEND_TITLE).expect("Titel").rect;
    let undock = find(&seen, &glyph(Icon::Undock))
        .expect("Symbol Abdocken")
        .rect
        .center();
    s.click(undock, &mut f);
    let w = t.state.borrow().get(BLEND).expect("abgedockt gemerkt");
    // Lage und Größe des Dialogs im Hauptfenster (Faktor 1, Hauptfenster bei 0/0).
    assert!(
        w.x < title.min.x && title.min.x - w.x < 40.0,
        "{w:?} / {title:?}"
    );
    assert!(
        w.y < title.min.y && title.min.y - w.y < 40.0,
        "{w:?} / {title:?}"
    );
    assert!((w.width - DialogSize::Normal.width()).abs() < 0.5, "{w:?}");
    assert!(t.state.borrow_mut().take_dirty(), "zu speichern");
    let main = s.settle(3, &mut f);
    let id = detach::viewport_id(BLEND);
    let child = s.child_seen(id);
    assert!(
        find(&child, BLEND_TITLE).is_some(),
        "Kind: {}",
        join(&child)
    );
    assert!(find(&main, BLEND_TITLE).is_none(), "Haupt: {}", join(&main));
    let b = s.child_builder(id).expect("Builder");
    assert_eq!(
        b.position,
        Some(pos2(w.x, w.y)),
        "springt an der Stelle heraus"
    );
    assert_eq!(b.active, Some(true), "Dialog nimmt den Fokus");
    assert_eq!(b.resizable, Some(false));
}

#[test]
fn undocked_dialog_renders_in_child() {
    let t = DialogTest::new(Some(on_main_monitor()));
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let mut f = t.frame();
    let main = s.settle(4, &mut f);
    let id = detach::viewport_id(BLEND);
    let child = s.child_seen(id);
    assert!(
        find(&child, BLEND_TITLE).is_some(),
        "Kind: {}",
        join(&child)
    );
    assert!(
        find(&child, &format!("{BLEND_CONTENT} 0")).is_some(),
        "Inhalt: {}",
        join(&child)
    );
    assert!(find(&child, FOOTER).is_some(), "{}", join(&child));
    assert!(
        find(&child, &glyph(Icon::Dock)).is_some(),
        "{}",
        join(&child)
    );
    assert!(
        find(&child, &glyph(Icon::Close)).is_some(),
        "{}",
        join(&child)
    );
    assert!(find(&main, BLEND_TITLE).is_none(), "Haupt: {}", join(&main));
    assert!(
        find(&main, &format!("{BLEND_CONTENT} 0")).is_none(),
        "Haupt: {}",
        join(&main)
    );
    let b = s.child_builder(id).expect("Builder");
    assert_eq!(b.title.as_deref(), Some("PlanoraCAD – Verrunden"));
    assert_eq!(b.position, Some(pos2(200.0, 150.0)));
    let size = b.inner_size.expect("Größe");
    assert_eq!(size.x, DialogSize::Normal.width(), "Breite der Klasse");
    assert!(!t.state.borrow_mut().take_dirty(), "Öffnen ändert nichts");
}

#[test]
fn undocked_dialog_height_follows_content() {
    let t = DialogTest::new(Some(on_main_monitor()));
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let id = detach::viewport_id(BLEND);
    s.child_info(id, |i| i.monitor_size = Some(vec2(1920.0, 1080.0)));
    let mut f = t.frame();
    s.settle(5, &mut f);
    let small = s.child_builder(id).unwrap().inner_size.unwrap().y;
    let child = s.child_seen(id);
    let footer = find(&child, FOOTER).expect("Fußzeile").rect;
    assert!(
        footer.max.y <= small,
        "Fußzeile im Fenster: {footer:?} / {small}"
    );
    assert!(
        small - footer.max.y < 40.0,
        "kein großer Leerraum: {footer:?} / {small}"
    );
    *t.lines.borrow_mut() = 8;
    s.settle(5, &mut f);
    let big = s.child_builder(id).unwrap().inner_size.unwrap().y;
    assert!(
        big > small + 7.0 * 10.0,
        "wächst mit dem Inhalt: {small} → {big}"
    );
    // eframe gleicht den Builder mit dem Fenster ab (`ViewportBuilder::patch`) und schickt die
    // neue Größe als `ViewportCommand::InnerSize`.
    // Höher als der Monitor (abzüglich Rand) wird das Fenster nie; der Inhalt rollt dann.
    *t.lines.borrow_mut() = 400;
    s.settle(5, &mut f);
    let capped = s.child_builder(id).unwrap().inner_size.unwrap().y;
    assert!(capped < 1080.0 && capped > 900.0, "{capped}");
    let child = s.child_seen(id);
    assert!(
        find(&child, FOOTER).is_some(),
        "Fußzeile bleibt: {}",
        join(&child)
    );
    assert!(find(&child, &format!("{BLEND_CONTENT} 399")).is_none());
}

#[cfg(windows)]
#[test]
fn missing_monitor_opens_docked_and_keeps_entry() {
    let gone = DetachedWindow {
        x: -100000.0,
        y: 40.0,
        width: 480.0,
        height: 300.0,
    };
    let t = DialogTest::new(Some(gone));
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let mut f = t.frame();
    let main = s.settle(4, &mut f);
    let id = detach::viewport_id(BLEND);
    assert!(
        find(&main, BLEND_TITLE).is_some(),
        "im Hauptfenster: {}",
        join(&main)
    );
    assert!(
        find(&main, &glyph(Icon::Undock)).is_some(),
        "mit Symbol Abdocken"
    );
    assert_eq!(s.child_runs(id), 0, "kein eigenes Fenster");
    assert_eq!(t.state.borrow().get(BLEND), Some(gone), "Eintrag bleibt");
    assert!(!t.state.borrow_mut().take_dirty());
}

#[test]
fn dock_click_returns_to_main_window() {
    let t = DialogTest::new(Some(on_main_monitor()));
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let mut f = t.frame();
    s.settle(3, &mut f);
    let id = detach::viewport_id(BLEND);
    let child = s.child_seen(id);
    let dock = find(&child, &glyph(Icon::Dock))
        .unwrap_or_else(|| panic!("Symbol Andocken: {}", join(&child)))
        .rect
        .center();
    s.click_child(id, dock, &mut f);
    assert_eq!(t.state.borrow().get(BLEND), None, "Eintrag gelöscht");
    assert!(t.state.borrow_mut().take_dirty());
    let runs = s.child_runs(id);
    let main = s.settle(3, &mut f);
    assert!(
        find(&main, BLEND_TITLE).is_some(),
        "zurück: {}",
        join(&main)
    );
    assert!(
        find(&main, &glyph(Icon::Undock)).is_some(),
        "{}",
        join(&main)
    );
    assert_eq!(s.child_runs(id), runs, "Kindfenster zu");
    assert!(*t.open.borrow(), "Andocken schließt nicht");
}

#[test]
fn close_in_undocked_dialog_closes_it() {
    // Schließen-Symbol im abgedockten Dialog: wie heute `*open = false`.
    let t = DialogTest::new(Some(on_main_monitor()));
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let mut f = t.frame();
    s.settle(3, &mut f);
    let id = detach::viewport_id(BLEND);
    let child = s.child_seen(id);
    let close = find(&child, &glyph(Icon::Close))
        .expect("Schließen")
        .rect
        .center();
    s.click_child(id, close, &mut f);
    assert!(!*t.open.borrow(), "geschlossen");
    assert_eq!(
        t.state.borrow().get(BLEND),
        Some(on_main_monitor()),
        "bleibt abgedockt gemerkt"
    );
    drop(f);

    // Alt+F4 im abgedockten Dialog: wie Abbrechen, das Fenster bricht das Schließen ab.
    let t = DialogTest::new(Some(on_main_monitor()));
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let mut f = t.frame();
    s.settle(3, &mut f);
    s.child_info(id, |i| i.events = vec![ViewportEvent::Close]);
    s.frame(vec![], &mut f);
    assert!(!*t.open.borrow(), "geschlossen");
    assert!(s.child_commands(id).contains(&ViewportCommand::CancelClose));
}

#[test]
fn multi_pass_frame_handles_close_and_drag_once() {
    // Bilder mit mehreren Durchgängen (`request_discard`) rufen das Kindfenster mehrmals auf;
    // Schließen abbrechen und Ziehen beginnen gehen trotzdem nur einmal an das Fenster.
    let mut t = DialogTest::new(Some(on_main_monitor()));
    t.discard_first_pass = true;
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let id = detach::viewport_id(BLEND);
    let mut f = t.frame();
    s.settle(3, &mut f);
    let runs = s.child_runs(id);
    s.frame(vec![], &mut f);
    assert_eq!(s.child_runs(id), runs + 2, "zwei Durchgänge je Bild");

    let title = find(&s.child_seen(id), BLEND_TITLE)
        .expect("Titel")
        .rect
        .center();
    s.frame_child(id, vec![Event::PointerMoved(title)], &mut f);
    s.frame_child(id, vec![press(title, true)], &mut f);
    s.frame_child(
        id,
        vec![Event::PointerMoved(title + vec2(30.0, 10.0))],
        &mut f,
    );
    s.frame_child(id, vec![press(title + vec2(30.0, 10.0), false)], &mut f);
    let drags = s
        .child_commands(id)
        .iter()
        .filter(|c| **c == ViewportCommand::StartDrag)
        .count();
    assert_eq!(drags, 1, "{:?}", s.child_commands(id));

    s.child_info(id, |i| i.events = vec![ViewportEvent::Close]);
    s.frame(vec![], &mut f);
    let cancels = s
        .child_commands(id)
        .iter()
        .filter(|c| **c == ViewportCommand::CancelClose)
        .count();
    assert_eq!(cancels, 1, "{:?}", s.child_commands(id));
    assert!(!*t.open.borrow());
}

#[test]
fn undocked_dialog_size_is_stable_on_a_monitor_with_other_scale() {
    // Hauptfenster 100 %, Dialog auf einem Monitor mit 125 %: Ziehen ändert seine Größe nicht
    // (das Harness wendet Builder-Größe und `InnerSize` wie winit mit dem Faktor des Kinds an).
    let t = DialogTest::new(Some(on_main_monitor()));
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let id = detach::viewport_id(BLEND);
    s.child_info(id, |i| {
        i.native_pixels_per_point = Some(1.25);
        i.outer_rect = Some(egui::Rect::from_min_size(
            pos2(160.0, 120.0),
            egui::Vec2::ZERO,
        ));
        i.monitor_size = Some(vec2(1536.0, 864.0));
    });
    let mut f = t.frame();
    let mut last = (None, 0);
    for _ in 0..6 {
        s.frame(vec![], &mut f);
        s.apply_child_size(id, &mut last);
    }
    let built = s.child_builder(id).expect("Builder").inner_size;
    for n in 0..3 {
        let title = find(&s.child_seen(id), BLEND_TITLE)
            .expect("Titel")
            .rect
            .center();
        let to = title + vec2(30.0, 10.0);
        for events in [
            vec![Event::PointerMoved(title)],
            vec![press(title, true)],
            vec![Event::PointerMoved(to)],
        ] {
            s.frame_child(id, events, &mut f);
            s.apply_child_size(id, &mut last);
        }
        s.child_info(id, |i| {
            i.outer_rect = i.outer_rect.map(|r| r.translate(vec2(30.0, 10.0)));
        });
        s.frame_child(id, vec![press(to, false)], &mut f);
        for _ in 0..4 {
            s.frame(vec![], &mut f);
            s.apply_child_size(id, &mut last);
        }
        assert!(t.state.borrow().get(BLEND).is_some());
        assert_eq!(
            s.child_builder(id).expect("Builder").inner_size,
            built,
            "Ziehen {n}: Größe bleibt"
        );
    }
}

#[test]
fn system_move_without_drag_is_stored() {
    // Review M2: Verschiebt das Betriebssystem das Fenster ohne Ziehen an Kopfzeile oder Rand
    // (Win+Umschalt+Pfeil, Einrasten, DPI-Wechsel), wird die neue Lage gemerkt.
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let start = egui::Rect::from_min_size(pos2(100.0, 100.0), vec2(320.0, 480.0));
    let state = RefCell::new(DetachState::default());
    state
        .borrow_mut()
        .set(Place::Tree, detach::to_physical(start, 1.0));
    state.borrow_mut().take_dirty();
    let events = RefCell::new(Vec::new());
    let id = detach::viewport_id(Place::Tree);
    s.child_info(id, |i| {
        i.native_pixels_per_point = Some(1.0);
        i.outer_rect = Some(start);
        i.inner_rect = Some(start);
    });
    let mut f = tree_window(&state, &events);
    s.settle(3, &mut f);
    assert!(!state.borrow_mut().take_dirty(), "Öffnen ändert nichts");
    let moved = egui::Rect::from_min_size(pos2(400.0, 150.0), vec2(360.0, 500.0));
    s.child_info(id, |i| {
        i.outer_rect = Some(moved);
        i.inner_rect = Some(moved);
    });
    s.settle(2, &mut f);
    assert_eq!(
        state.borrow().get(Place::Tree),
        Some(detach::to_physical(moved, 1.0))
    );
    assert!(state.borrow_mut().take_dirty());
}

#[cfg(windows)]
#[test]
fn system_move_keeps_entry_when_its_monitor_is_missing() {
    // Review M2 / Spec Ziel 3: Fehlt der Monitor der gemerkten Lage (Windows hat das Fenster
    // verschoben), bleibt der Eintrag.
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let far = egui::Rect::from_min_size(pos2(-100000.0, -100000.0), vec2(320.0, 480.0));
    let state = RefCell::new(DetachState::default());
    let saved = detach::to_physical(far, 1.0);
    state.borrow_mut().set(Place::Tree, saved);
    state.borrow_mut().take_dirty();
    let events = RefCell::new(Vec::new());
    let id = detach::viewport_id(Place::Tree);
    s.child_info(id, |i| {
        i.native_pixels_per_point = Some(1.0);
        i.outer_rect = Some(far);
        i.inner_rect = Some(far);
    });
    let mut f = tree_window(&state, &events);
    s.settle(3, &mut f);
    let moved = egui::Rect::from_min_size(pos2(100.0, 100.0), vec2(320.0, 480.0));
    s.child_info(id, |i| {
        i.outer_rect = Some(moved);
        i.inner_rect = Some(moved);
    });
    s.settle(3, &mut f);
    assert_eq!(state.borrow().get(Place::Tree), Some(saved));
    assert!(!state.borrow_mut().take_dirty());
}

#[test]
fn alt_f4_on_fixed_prompt_over_field_reports_cancel() {
    // Review M4: Feste Rückfragen (ohne Schließen-Symbol, etwa der Versionsspalte) über einem
    // abgedockten Feld: Alt+F4 auf ihrem Fenster meldet `close_request` (der Aufrufer bricht ab)
    // und schließt nicht das Programm.
    let mut s = Screen::with_viewports(1920.0, 1080.0);
    let state = RefCell::new(DetachState::default());
    let closed = RefCell::new(false);
    let mut f = |ctx: &egui::Context| {
        let (r, _) = detach::show_window(ctx, &mut state.borrow_mut(), spec(), |ui| {
            let mut c = false;
            Dialog::new("Frage", DialogSize::Prompt)
                .fixed()
                .close_request(&mut c)
                .show(ui.ctx(), |ui| {
                    ui.label("Wirklich löschen?");
                });
            c
        });
        if r == Some(true) {
            *closed.borrow_mut() = true;
        }
    };
    s.settle(3, &mut f);
    let sub = detach::sub_viewport_id(Place::Tree, egui::Id::unique("Frage"));
    assert!(s.child_runs(sub) > 0, "Unterdialog im eigenen Fenster");
    assert!(
        find(&s.child_seen(sub), &glyph(Icon::Close)).is_none(),
        "kein Schließen-Symbol"
    );
    assert!(!*closed.borrow());
    s.child_info(sub, |i| i.events = vec![ViewportEvent::Close]);
    s.frame(vec![], &mut f);
    assert!(*closed.borrow(), "Alt+F4 gemeldet");
    assert!(
        s.child_commands(sub)
            .contains(&ViewportCommand::CancelClose)
    );
}
