//! Fenster-Baustein für abgedockte Inhalte (`detach::show_window`) im Harness mit echten
//! Kindfenstern: eigener Inhalt, Andocken, Schließen, Ziehen und Lage-Korrektur.

mod common;

use common::{Screen, find, join};
use egui::{Event, PointerButton, ViewportCommand, ViewportEvent, pos2, vec2};
use planora_app::detach::{self, DetachState, Place, WindowEvent, WindowSpec};
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
    }
}

/// Zeigt den Baum als eigenes Fenster und sammelt die Ereignisse.
fn tree_window<'a>(
    state: &'a RefCell<DetachState>,
    events: &'a RefCell<Vec<WindowEvent>>,
) -> impl FnMut(&egui::Context) + 'a {
    move |ctx| {
        egui::Area::new(egui::Id::new("haupt")).show(ctx, |ui| {
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
