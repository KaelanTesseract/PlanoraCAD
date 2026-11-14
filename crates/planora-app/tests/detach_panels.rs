//! Abgedockte Felder (Baum, rechte Spalte, Ansichtsfeld der Zeichnung, Skizzenfeld) in der
//! ganzen App mit echten Kindfenstern (`Screen::with_viewports`), dazu Unterdialoge aus dem
//! abgedockten Baum.

mod common;

use common::{Screen, Seen, find, join};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::detach::{self, Place};
use planora_app::icons::{self, Icon};
use planora_app::shell_panels;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::{SketchPlane, StandardPlane};
use planora_math::DVec3;
use planora_project::config::{Config, DetachedWindow};
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1920.0, 1080.0);

/// Lage der abgedockten Felder auf dem Hauptmonitor (unter Windows auf einem vorhandenen
/// Monitor, sonst bliebe das Feld angedockt).
fn window_at(x: f32, y: f32) -> DetachedWindow {
    DetachedWindow {
        x,
        y,
        width: 320.0,
        height: 700.0,
    }
}

fn glyph(icon: Icon) -> String {
    icons::glyph(icon).to_string()
}

/// Die ganze App im Harness mit Kindfenstern.
struct Rig {
    screen: Screen,
    app: PlanoraApp,
}

impl Rig {
    /// App mit einem Quader; `detached` liegt schon im eigenen Fenster.
    fn new(detached: &[(Place, DetachedWindow)]) -> Rig {
        let screen = Screen::with_viewports(SIZE.x, SIZE.y);
        screen.ctx.set_fonts(fonts::definitions());
        theme::apply(&screen.ctx, true);
        PlanoraApp::setup_context(&screen.ctx);
        let root = common::temp_root("planora-detach-panels");
        let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        // Nie die echte Konfiguration schreiben.
        ws.config_path = root.join("config.json");
        ws.active_tab_mut()
            .state
            .create_box(DVec3::new(100.0, 60.0, 40.0))
            .unwrap();
        for (p, w) in detached {
            ws.detach.set(*p, *w);
        }
        let mut rig = Rig {
            screen,
            app: PlanoraApp::for_tests(ws),
        };
        rig.settle(4);
        rig
    }

    fn state(&mut self) -> &mut planora_app::state::AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        let app = &mut self.app;
        self.screen.frame_ui(events, &mut |ui| {
            let mut f = eframe::Frame::_new_kittest();
            eframe::App::ui(app, ui, &mut f);
        })
    }

    fn settle(&mut self, n: usize) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![]);
        }
        seen
    }

    /// Ein Bild mit Ereignissen im Kindfenster `id`; liefert dessen Texte.
    fn frame_child(&mut self, id: egui::ViewportId, events: Vec<Event>) -> Vec<Seen> {
        self.screen.queue_child(id, events);
        self.frame(vec![]);
        self.screen.child_seen(id)
    }

    /// Klick mit `button` im Kindfenster `id` an `at`, danach zwei Bilder.
    fn click_child(&mut self, id: egui::ViewportId, at: Pos2, button: PointerButton) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame_child(id, vec![Event::PointerMoved(at)]);
        self.frame_child(id, vec![press(true)]);
        self.frame_child(id, vec![press(false)]);
        self.settle(2);
    }

    /// Linksklick im Hauptfenster an `at`, danach zwei Bilder.
    fn click(&mut self, at: Pos2) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![press(true)]);
        self.frame(vec![press(false)]);
        self.settle(2);
    }

    fn fields(&self) -> Vec<&'static str> {
        shell_panels::field_rects(&self.screen.ctx)
            .into_iter()
            .map(|(n, _)| n)
            .collect()
    }

    fn layout(&self) -> planora_app::shell::ShellRects {
        shell_panels::last_layout(&self.screen.ctx).expect("Layout")
    }
}

fn center_of(seen: &[Seen], text: &str) -> Pos2 {
    find(seen, text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(seen)))
        .rect
        .center()
}

/// Mitte des Symbols „Abdocken“ im Feld der Spalte `column`: oben rechts im Kopf des Felds.
fn undock_in(seen: &[Seen], column: Rect) -> Pos2 {
    let at = seen
        .iter()
        .filter(|s| s.text == glyph(Icon::Undock))
        .map(|s| s.rect.center())
        .find(|p| column.contains(*p))
        .unwrap_or_else(|| panic!("Abdocken in {column:?}: {}", join(seen)));
    assert!(at.x > column.center().x, "rechts: {at:?} in {column:?}");
    assert!(at.y < column.top() + theme::HEADER_H + 16.0, "oben: {at:?}");
    at
}

#[test]
fn tree_undocked_draws_tree_in_child_and_frees_column() {
    let docked = Rig::new(&[]).layout();
    let mut rig = Rig::new(&[(Place::Tree, window_at(100.0, 100.0))]);
    let main = rig.settle(2);
    let id = detach::viewport_id(Place::Tree);
    let child = rig.screen.child_seen(id);
    assert!(find(&child, "Feature-Baum").is_some(), "{}", join(&child));
    assert!(
        find(&child, &glyph(Icon::Dock)).is_some(),
        "{}",
        join(&child)
    );
    assert!(find(&main, "Feature-Baum").is_none(), "{}", join(&main));
    assert!(!rig.fields().contains(&"links"), "{:?}", rig.fields());
    let l = rig.layout();
    assert_eq!(l.left.width(), 0.0);
    assert!(l.free.left() < docked.free.left(), "{l:?} / {docked:?}");
    let b = rig.screen.child_builder(id).expect("Builder");
    assert_eq!(b.title.as_deref(), Some("PlanoraCAD – Feature-Baum"));
    assert_eq!(b.resizable, Some(true));
    assert_eq!(
        b.active,
        Some(false),
        "nimmt dem Hauptfenster den Fokus nicht"
    );
    assert_eq!(b.inner_size, Some(vec2(320.0, 700.0)));
    assert_eq!(b.position, Some(pos2(100.0, 100.0)));

    // „Andocken“: zurück in die Spalte, der Eintrag ist weg.
    let dock = center_of(&child, &glyph(Icon::Dock));
    rig.click_child(id, dock, PointerButton::Primary);
    assert!(rig.app.ws_mut().detach.get(Place::Tree).is_none());
    let main = rig.settle(2);
    assert!(rig.fields().contains(&"links"), "{:?}", rig.fields());
    assert!(find(&main, "Feature-Baum").is_some(), "{}", join(&main));
    let runs = rig.screen.child_runs(id);
    rig.settle(2);
    assert_eq!(rig.screen.child_runs(id), runs, "kein Fenster mehr");

    // „Abdocken“ im Feld: an der Stelle der Spalte heraus.
    let main = rig.settle(1);
    let column = rig.layout().left;
    let undock = undock_in(&main, column);
    rig.click(undock);
    let w = rig.app.ws_mut().detach.get(Place::Tree).expect("abgedockt");
    assert_eq!((w.x, w.y), (column.min.x, column.min.y));
    assert_eq!((w.width, w.height), (column.width(), column.height()));
    rig.settle(2);
    assert!(!rig.fields().contains(&"links"), "{:?}", rig.fields());
    assert_eq!(
        rig.screen.child_builder(id).expect("Builder").inner_size,
        Some(column.size()),
        "Startgröße = Spalte"
    );
}

#[test]
fn right_undocked_shows_properties_in_child() {
    let mut rig = Rig::new(&[(Place::Right, window_at(1500.0, 100.0))]);
    let main = rig.settle(2);
    let id = detach::viewport_id(Place::Right);
    let child = rig.screen.child_seen(id);
    assert!(find(&child, "Eigenschaften").is_some(), "{}", join(&child));
    assert!(find(&main, "Eigenschaften").is_none(), "{}", join(&main));
    assert!(!rig.fields().contains(&"rechts"), "{:?}", rig.fields());
    assert!(rig.fields().contains(&"links"), "Baum bleibt");
    assert_eq!(rig.layout().right.width(), 0.0);
    let b = rig.screen.child_builder(id).expect("Builder");
    assert_eq!(b.title.as_deref(), Some("PlanoraCAD – Eigenschaften"));
    assert_eq!(b.resizable, Some(true));
    // Alt+F4 dockt an, statt etwas zu schließen.
    rig.screen
        .child_info(id, |i| i.events = vec![egui::ViewportEvent::Close]);
    rig.settle(3);
    assert!(rig.app.ws_mut().detach.get(Place::Right).is_none());
    assert!(rig.fields().contains(&"rechts"), "{:?}", rig.fields());
}

/// Zeichnung „Blatt 1“ mit Vorderansicht und davon abgeleiteter Draufsicht, geöffnet; die
/// Vorderansicht ist gewählt.
fn open_drawing(rig: &mut Rig) {
    let s = rig.state();
    let sheet = Sheet {
        paper: Paper::A3,
        landscape: true,
    };
    let d = s
        .document
        .add_drawing("Blatt 1", sheet, Projection::First)
        .unwrap();
    let scale = Scale { num: 1, den: 1 };
    let front = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            scale,
            Placement::Free { x: 200.0, y: 150.0 },
            None,
        )
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Top,
            scale,
            Placement::Aligned { distance: 70.0 },
            Some(front),
        )
        .unwrap();
    s.drawing_ui.open(d);
    s.drawing_ui.selected_view = Some(front);
}

#[test]
fn drawing_view_field_follows_right_place() {
    let mut rig = Rig::new(&[(Place::Right, window_at(1500.0, 100.0))]);
    open_drawing(&mut rig);
    let main = rig.settle(4);
    let id = detach::viewport_id(Place::Right);
    let child = rig.screen.child_seen(id);
    assert!(
        find(&child, "Ansicht löschen").is_some(),
        "Ansichtsfeld im Fenster: {}",
        join(&child)
    );
    assert!(find(&main, "Ansicht löschen").is_none(), "{}", join(&main));
    let names = rig.fields();
    assert!(!names.contains(&"Ansichtsfeld"), "{names:?}");
    assert!(names.contains(&"Werkzeugfeld"), "{names:?}");
    assert_eq!(rig.layout().right.width(), 0.0);
    let b = rig.screen.child_builder(id).expect("Builder");
    assert_eq!(b.title.as_deref(), Some("PlanoraCAD – Ansicht"));

    // Andocken: das Ansichtsfeld steht wieder rechts im Hauptfenster.
    let dock = center_of(&child, &glyph(Icon::Dock));
    rig.click_child(id, dock, PointerButton::Primary);
    let main = rig.settle(2);
    assert!(rig.fields().contains(&"Ansichtsfeld"), "{:?}", rig.fields());
    assert!(find(&main, "Ansicht löschen").is_some(), "{}", join(&main));

    // Abdocken aus dem Ansichtsfeld.
    let undock = undock_in(&main, rig.layout().right);
    rig.click(undock);
    assert!(rig.app.ws_mut().detach.get(Place::Right).is_some());
    let main = rig.settle(3);
    assert!(find(&main, "Ansicht löschen").is_none(), "{}", join(&main));
    let child = rig.screen.child_seen(id);
    assert!(
        find(&child, "Ansicht löschen").is_some(),
        "{}",
        join(&child)
    );
}

/// Neue Skizze auf XY im aktiven Tab, offen.
fn open_sketch(rig: &mut Rig) -> planora_feature::FeatureId {
    let s = rig.state();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    id
}

#[test]
fn sketch_side_window_appears_only_in_sketch() {
    let mut rig = Rig::new(&[(Place::SketchSide, window_at(1500.0, 100.0))]);
    let id = detach::viewport_id(Place::SketchSide);
    rig.settle(2);
    assert_eq!(rig.screen.child_runs(id), 0, "im Modell kein Fenster");
    assert!(rig.fields().contains(&"rechts"), "Modell: Spalte rechts");

    // Skizze beginnen: Fenster da, rechts keine Spalte.
    open_sketch(&mut rig);
    rig.frame(vec![]);
    assert!(rig.screen.child_runs(id) > 0, "Fenster mit der Skizze");
    let child = rig.screen.child_seen(id);
    assert!(
        find(&child, "Profil anzeigen").is_some(),
        "{}",
        join(&child)
    );
    assert_eq!(
        rig.screen
            .child_builder(id)
            .expect("Builder")
            .title
            .as_deref(),
        Some("PlanoraCAD – Skizzenfeld")
    );
    let names = rig.fields();
    assert!(!names.contains(&"Skizze"), "{names:?}");
    assert!(names.contains(&"Leiste"), "{names:?}");
    assert_eq!(rig.layout().right.width(), 0.0);

    // Skizze beenden: im nächsten Bild kein Kindfenster, Layout mit rechter Spalte.
    rig.state().close_sketch();
    let runs = rig.screen.child_runs(id);
    rig.frame(vec![]);
    assert_eq!(rig.screen.child_runs(id), runs, "kein Fenster mehr");
    let l = rig.layout();
    assert!(l.strip.is_none());
    assert!(l.right.width() > 0.0, "{l:?}");
    assert!(rig.fields().contains(&"rechts"), "{:?}", rig.fields());
    assert!(
        rig.app.ws_mut().detach.get(Place::SketchSide).is_some(),
        "bleibt für die nächste Skizze abgedockt"
    );
}

#[test]
fn undo_back_into_sketch_with_undocked_side() {
    let mut rig = Rig::new(&[(Place::SketchSide, window_at(1500.0, 100.0))]);
    let id = detach::viewport_id(Place::SketchSide);
    let sketch = open_sketch(&mut rig);
    rig.settle(2);
    let runs = rig.screen.child_runs(id);
    assert!(runs > 0);

    // Rückgängig in der Skizze (nichts zu tun): die Skizze und ihr Fenster bleiben.
    let _ = rig.app.ws_mut().active_tab_mut().undo();
    rig.settle(2);
    assert_eq!(rig.state().open_sketch_id(), Some(sketch));
    assert!(rig.screen.child_runs(id) > runs, "Fenster bleibt");

    // Beenden, dann Rückgängig (nimmt die Skizze weg) und Wiederholen: kein Fenster.
    rig.state().close_sketch();
    rig.settle(2);
    let runs = rig.screen.child_runs(id);
    rig.app.ws_mut().active_tab_mut().undo().unwrap();
    rig.settle(2);
    assert!(rig.state().document.feature(sketch).is_none());
    rig.app.ws_mut().active_tab_mut().redo().unwrap();
    rig.settle(2);
    assert_eq!(rig.screen.child_runs(id), runs, "kein verwaistes Fenster");
    assert!(rig.fields().contains(&"rechts"), "{:?}", rig.fields());

    // Wieder in die Skizze: Fenster und Layout schon im ersten Bild.
    rig.state().open_sketch(sketch).unwrap();
    rig.frame(vec![]);
    assert!(rig.screen.child_runs(id) > runs, "Fenster wieder da");
    assert_eq!(rig.layout().right.width(), 0.0);
    assert!(!rig.fields().contains(&"Skizze"), "{:?}", rig.fields());
    assert!(rig.fields().contains(&"Leiste"), "{:?}", rig.fields());
}

#[test]
fn rename_from_undocked_tree_opens_over_tree_window() {
    let mut rig = Rig::new(&[(Place::Tree, window_at(300.0, 100.0))]);
    let tree = detach::viewport_id(Place::Tree);
    // Das Betriebssystem meldet das Baumfenster etwas verschoben (vom Benutzer gezogen).
    let host = Rect::from_min_size(pos2(340.0, 120.0), vec2(320.0, 700.0));
    rig.screen.child_info(tree, |i| {
        i.outer_rect = Some(host);
        i.inner_rect = Some(host);
    });
    rig.settle(2);
    let child = rig.screen.child_seen(tree);
    let row = center_of(&child, "Quader 1");
    rig.click_child(tree, row, PointerButton::Secondary);
    let child = rig.screen.child_seen(tree);
    let entry = center_of(&child, "Umbenennen");
    rig.click_child(tree, entry, PointerButton::Primary);
    assert!(rig.state().rename_draft.is_some(), "Umbenennen offen");
    let main = rig.settle(3);

    let sub = detach::sub_viewport_id(Place::Tree, egui::Id::new("Umbenennen"));
    assert!(rig.screen.child_runs(sub) > 0, "eigenes Fenster");
    let seen = rig.screen.child_seen(sub);
    assert!(find(&seen, "Umbenennen").is_some(), "{}", join(&seen));
    assert!(find(&seen, "Übernehmen").is_some(), "{}", join(&seen));
    assert!(
        find(&seen, &glyph(Icon::Undock)).is_none(),
        "kein Abdocken: {}",
        join(&seen)
    );
    assert!(
        find(&main, "Übernehmen").is_none(),
        "Haupt: {}",
        join(&main)
    );
    let child = rig.screen.child_seen(tree);
    assert!(
        find(&child, "Übernehmen").is_none(),
        "nicht im Baumfenster: {}",
        join(&child)
    );
    let b = rig.screen.child_builder(sub).expect("Builder");
    let size = b.inner_size.expect("Größe");
    let pos = b.position.expect("Lage");
    assert!(
        (pos + size / 2.0 - host.center()).length() < 1.0,
        "mittig über dem Baumfenster: {pos:?} {size:?} / {host:?}"
    );
    assert_eq!(b.resizable, Some(false));
    assert_eq!(b.active, Some(true), "nimmt den Fokus");
    assert!(
        detach::sub_dialog_open(&rig.screen.ctx, Place::Tree),
        "Baumfenster gesperrt"
    );
    assert!(
        rig.app
            .ws_mut()
            .detach
            .get(Place::Dialog("tree_rename"))
            .is_none(),
        "nichts gemerkt"
    );

    // Abbrechen im Unterdialog schließt ihn; sein Fenster geht.
    let cancel = center_of(&seen, "Abbrechen");
    rig.click_child(sub, cancel, PointerButton::Primary);
    assert!(rig.state().rename_draft.is_none());
    let runs = rig.screen.child_runs(sub);
    rig.settle(2);
    assert_eq!(rig.screen.child_runs(sub), runs);
    assert!(!detach::sub_dialog_open(&rig.screen.ctx, Place::Tree));
}

#[test]
fn delete_prompt_from_drawing_view_stays_in_main() {
    let mut rig = Rig::new(&[(Place::Tree, window_at(100.0, 100.0))]);
    open_drawing(&mut rig);
    let main = rig.settle(4);
    // „Ansicht löschen“ im (angedockten) Ansichtsfeld: die Draufsicht hängt an der Ansicht.
    let delete = center_of(&main, "Ansicht löschen");
    rig.click(delete);
    assert!(rig.state().pending_delete_drawing.is_some(), "Rückfrage");
    let main = rig.settle(3);
    assert!(
        find(&main, "Rückfrage").is_some() && find(&main, "Löschen").is_some(),
        "im Hauptfenster: {}",
        join(&main)
    );
    let tree = detach::viewport_id(Place::Tree);
    let child = rig.screen.child_seen(tree);
    assert!(find(&child, "Rückfrage").is_none(), "{}", join(&child));
    let sub = detach::sub_viewport_id(Place::Tree, egui::Id::new("planora-delete-drawing"));
    assert_eq!(rig.screen.child_runs(sub), 0, "kein Unterdialog");
    assert!(rig.screen.child_runs(tree) > 0, "Baum bleibt abgedockt");
}
