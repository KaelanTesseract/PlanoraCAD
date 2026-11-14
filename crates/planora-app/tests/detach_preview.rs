//! Vorschau in der 3D-Ansicht zieht bei Eingaben im abgedockten Dialog nach (Spec „Ziele“ 5,
//! Anti-Ziel „Vorschau erst nach Mausbewegung“). Die ganze App mit echten Kindfenstern.

mod common;

use common::{Screen, Seen, find, join};
use egui::{Event, Modifiers, PointerButton, Pos2, Vec2, vec2};
use planora_app::app::{self, PlanoraApp};
use planora_app::detach::{self, Place};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_math::DVec3;
use planora_project::config::{Config, DetachedWindow};
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1920.0, 1080.0);

fn frame(screen: &mut Screen, app: &mut PlanoraApp, events: Vec<Event>) -> Vec<Seen> {
    screen.frame_ui(events, &mut |ui| {
        let mut f = eframe::Frame::_new_kittest();
        eframe::App::ui(app, ui, &mut f);
    })
}

fn center_of(seen: &[Seen], text: &str) -> Pos2 {
    find(seen, text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(seen)))
        .rect
        .center()
}

/// Ein Klick im Kindfenster (Bewegen, Drücken, Loslassen) in drei Bildern; liefert die
/// Wartezeit des Hauptfensters nach dem Bild mit dem Loslassen.
fn click_in(
    screen: &mut Screen,
    app: &mut PlanoraApp,
    id: egui::ViewportId,
    at: Pos2,
) -> std::time::Duration {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for e in [Event::PointerMoved(at), button(true)] {
        screen.queue_child(id, vec![e]);
        frame(screen, app, vec![]);
    }
    screen.queue_child(id, vec![button(false)]);
    frame(screen, app, vec![]);
    screen.repaint_delay
}

#[test]
fn blend_option_in_undocked_dialog_updates_preview_next_frame() {
    let fillet = Place::Dialog("fillet");
    let mut screen = Screen::with_viewports(SIZE.x, SIZE.y);
    screen.ctx.set_fonts(fonts::definitions());
    theme::apply(&screen.ctx, true);
    PlanoraApp::setup_context(&screen.ctx);
    let root = common::temp_root("planora-detach-preview");
    let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    ws.config_path = root.join("config.json");
    ws.active_tab_mut()
        .state
        .create_box(DVec3::new(100.0, 60.0, 40.0))
        .unwrap();
    ws.detach.set(
        fillet,
        DetachedWindow {
            x: 200.0,
            y: 100.0,
            width: 420.0,
            height: 600.0,
        },
    );
    let mut app = PlanoraApp::for_tests(ws);
    for _ in 0..4 {
        frame(&mut screen, &mut app, vec![]);
    }
    app::open_blend(app.ws_mut(), false);
    for _ in 0..4 {
        frame(&mut screen, &mut app, vec![]);
    }
    let child = detach::viewport_id(fillet);
    assert!(screen.child_runs(child) > 0, "Dialog abgedockt");
    assert!(app.ws_mut().active_tab().state.chain_marking);

    // Die Tangentenkette ist der Zustand, den die Vorschau (Markierung) liest; der Dialog setzt
    // ihn erst nach der 3D-Ansicht dieses Bilds.
    let seen = screen.child_seen(child);
    let at = center_of(
        &seen,
        "Tangentenkette (glatt anschließende Kanten mitnehmen)",
    );
    let delay = click_in(&mut screen, &mut app, child, at);
    assert_eq!(
        delay,
        std::time::Duration::ZERO,
        "Hauptfenster wird nach der Eingabe im Kindfenster neu gezeichnet"
    );
    // Das nächste Hauptbild (ohne jede Eingabe, durch die Anforderung ausgelöst) gibt den neuen
    // Zustand an die Ansicht weiter.
    frame(&mut screen, &mut app, vec![]);
    assert!(
        !app.ws_mut().active_tab().state.chain_marking,
        "Vorschau-Zustand im nächsten Hauptbild ohne Eingabe"
    );
}
