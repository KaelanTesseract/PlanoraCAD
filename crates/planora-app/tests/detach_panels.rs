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

/// Lage der abgedockten Felder auf dem nachgebildeten Hauptmonitor (`Screen::with_viewports`),
/// sonst bliebe das Feld angedockt.
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
        find(&child, "Unterbestimmte finden").is_some(),
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

    let sub = detach::sub_viewport_id(Place::Tree, egui::Id::unique("Umbenennen"));
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
    let sub = detach::sub_viewport_id(Place::Tree, egui::Id::unique("planora-delete-drawing"));
    assert_eq!(rig.screen.child_runs(sub), 0, "kein Unterdialog");
    assert!(rig.screen.child_runs(tree) > 0, "Baum bleibt abgedockt");
}

fn press_at(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Ziehen am Kopf des Felds im Kindfenster `id` (das Betriebssystem verschiebt das Fenster um
/// 30/10 Punkte und ändert seine Größe um `grow` Punkte, wie beim Ziehen am Rand); `last` wie bei
/// [`Screen::apply_child_size`].
fn drag_field(
    rig: &mut Rig,
    id: egui::ViewportId,
    last: &mut (Option<Vec2>, usize),
    head: &str,
    grow: Vec2,
) {
    let mut step = |rig: &mut Rig, events: Vec<Event>| {
        rig.frame_child(id, events);
        rig.screen.apply_child_size(id, last);
    };
    let at = center_of(&rig.screen.child_seen(id), head);
    let to = at + vec2(30.0, 10.0);
    step(rig, vec![Event::PointerMoved(at)]);
    step(rig, vec![press_at(at, true)]);
    step(rig, vec![Event::PointerMoved(to)]);
    rig.screen.child_info(id, |i| {
        i.outer_rect = i
            .outer_rect
            .map(|r| Rect::from_min_size(r.min + vec2(30.0, 10.0), r.size() + grow));
        i.inner_rect = i.outer_rect;
    });
    step(rig, vec![press_at(to, false)]);
    for _ in 0..3 {
        step(rig, vec![]);
    }
}

#[test]
fn undocked_field_keeps_its_size_on_a_monitor_with_other_scale() {
    // Hauptfenster 100 %, Baumfenster auf einem Monitor mit 125 %.
    let mut rig = Rig::new(&[]);
    let id = detach::viewport_id(Place::Tree);
    rig.screen.child_info(id, |i| {
        i.native_pixels_per_point = Some(1.25);
        i.outer_rect = Some(Rect::from_min_size(pos2(80.0, 80.0), Vec2::ZERO));
    });
    let saved = window_at(100.0, 100.0);
    rig.app.ws_mut().detach.set(Place::Tree, saved);
    let mut last = (None, 0);
    for _ in 0..4 {
        rig.frame(vec![]);
        rig.screen.apply_child_size(id, &mut last);
    }
    let size = |rig: &mut Rig| {
        let w = rig.app.ws_mut().detach.get(Place::Tree).expect("abgedockt");
        vec2(w.width, w.height)
    };
    // Ziehen ändert die Größe nicht; einmal am Rand 40 Punkte breiter (50 px), danach bleibt
    // sie bei jedem weiteren Ziehen.
    let mut want = vec2(saved.width, saved.height);
    for (n, grow) in [Vec2::ZERO, vec2(40.0, 0.0), Vec2::ZERO, Vec2::ZERO]
        .into_iter()
        .enumerate()
    {
        drag_field(&mut rig, id, &mut last, "Feature-Baum", grow);
        want += grow * 1.25;
        let got = size(&mut rig);
        assert!(
            (got - want).length() < 1.0,
            "Vorgang {n}: {got:?} statt {want:?}"
        );
    }
}

#[test]
fn tab_switch_with_undocked_sketch_side() {
    let mut rig = Rig::new(&[(Place::SketchSide, window_at(1500.0, 100.0))]);
    let side = detach::viewport_id(Place::SketchSide);
    // Tab A: Skizze offen, Skizzenfeld im eigenen Fenster.
    open_sketch(&mut rig);
    rig.settle(2);
    assert!(rig.screen.child_runs(side) > 0);

    // Tab B (Modell): kein Skizzenfenster mehr, rechte Spalte da – schon im ersten Bild.
    rig.app.ws_mut().new_tab();
    let runs = rig.screen.child_runs(side);
    rig.frame(vec![]);
    assert_eq!(rig.screen.child_runs(side), runs, "kein verwaistes Fenster");
    let l = rig.layout();
    assert!(l.strip.is_none() && l.right.width() > 0.0, "{l:?}");
    assert!(rig.fields().contains(&"rechts"), "{:?}", rig.fields());
    rig.settle(2);
    assert_eq!(rig.screen.child_runs(side), runs);

    // Zurück zu A: Fenster und Layout ohne Spalte im ersten Bild.
    rig.app.ws_mut().activate(0);
    rig.frame(vec![]);
    assert!(rig.screen.child_runs(side) > runs, "Fenster wieder da");
    let l = rig.layout();
    assert!(l.strip.is_some() && l.right.width() == 0.0, "{l:?}");
    assert!(!rig.fields().contains(&"Skizze"), "{:?}", rig.fields());
    assert!(rig.fields().contains(&"Leiste"), "{:?}", rig.fields());
}

#[test]
fn tab_switch_model_and_drawing_keeps_the_right_window() {
    let mut rig = Rig::new(&[(Place::Right, window_at(1500.0, 100.0))]);
    let right = detach::viewport_id(Place::Right);
    let title = |rig: &Rig| {
        rig.screen
            .child_builder(right)
            .and_then(|b| b.title)
            .unwrap_or_default()
    };
    assert_eq!(title(&rig), "PlanoraCAD – Eigenschaften");
    // Tab B mit offener Zeichnung: dasselbe Fenster zeigt das Ansichtsfeld.
    rig.app.ws_mut().new_tab();
    open_drawing(&mut rig);
    rig.settle(3);
    assert_eq!(title(&rig), "PlanoraCAD – Ansicht");
    let child = rig.screen.child_seen(right);
    assert!(
        find(&child, "Ansicht löschen").is_some(),
        "{}",
        join(&child)
    );
    assert!(
        !rig.fields().contains(&"Ansichtsfeld"),
        "{:?}",
        rig.fields()
    );
    // Zurück zu A (Modell): wieder die Eigenschaften im selben Fenster.
    rig.app.ws_mut().activate(0);
    rig.frame(vec![]);
    assert_eq!(title(&rig), "PlanoraCAD – Eigenschaften");
    let child = rig.screen.child_seen(right);
    assert!(find(&child, "Eigenschaften").is_some(), "{}", join(&child));
    assert!(
        find(&child, "Ansicht löschen").is_none(),
        "{}",
        join(&child)
    );
    assert!(!rig.fields().contains(&"rechts"), "{:?}", rig.fields());
}

#[test]
fn rename_from_tree_window_survives_a_tab_switch() {
    let mut rig = Rig::new(&[(Place::Tree, window_at(300.0, 100.0))]);
    let tree = detach::viewport_id(Place::Tree);
    let row = center_of(&rig.screen.child_seen(tree), "Quader 1");
    rig.click_child(tree, row, PointerButton::Secondary);
    let entry = center_of(&rig.screen.child_seen(tree), "Umbenennen");
    rig.click_child(tree, entry, PointerButton::Primary);
    assert!(rig.state().rename_draft.is_some());
    let sub = detach::sub_viewport_id(Place::Tree, egui::Id::unique("Umbenennen"));
    rig.settle(2);
    assert!(rig.screen.child_runs(sub) > 0);

    // Tab B und zurück: das offene Umbenennen bleibt über dem Baumfenster.
    rig.app.ws_mut().new_tab();
    rig.settle(3);
    rig.app.ws_mut().activate(0);
    assert!(rig.state().rename_draft.is_some(), "noch offen");
    let runs = rig.screen.child_runs(sub);
    let main = rig.settle(3);
    assert!(rig.screen.child_runs(sub) > runs, "wieder als Unterdialog");
    assert!(
        find(&main, "Übernehmen").is_none(),
        "nicht im Hauptfenster: {}",
        join(&main)
    );
}

#[cfg(windows)]
#[test]
fn undocked_tree_stays_out_when_it_lands_off_screen() {
    // Review M3: Der Monitor wird nicht laufend geprüft. Landet das abgedockte Baumfenster
    // außerhalb aller Monitore (gezogen; Ende des Ziehens merkt die Lage), bleibt es abgedockt.
    // Erst beim nächsten Erscheinen des Platzes (nach der Skizze) steht es angedockt, und der
    // Eintrag bleibt.
    let mut rig = Rig::new(&[(Place::Tree, window_at(100.0, 100.0))]);
    let tree = detach::viewport_id(Place::Tree);
    assert!(rig.screen.child_runs(tree) > 0, "abgedockt");
    let far = DetachedWindow {
        x: -100000.0,
        y: -100000.0,
        ..window_at(0.0, 0.0)
    };
    rig.app.ws_mut().detach.set(Place::Tree, far);
    let runs = rig.screen.child_runs(tree);
    rig.settle(3);
    assert!(rig.screen.child_runs(tree) > runs, "bleibt abgedockt");
    assert_eq!(rig.layout().left.width(), 0.0, "Spalte bleibt frei");

    open_sketch(&mut rig);
    rig.settle(2);
    rig.state().close_sketch();
    rig.settle(3);
    assert!(rig.layout().left.width() > 0.0, "jetzt angedockt");
    assert_eq!(
        rig.app.ws_mut().detach.get(Place::Tree),
        Some(far),
        "Eintrag bleibt"
    );
}

// ---- Namenszeile der Skizzenpalette mit Abdock-Symbol (Task H3) ---------------------------------

/// Mit eigenen Fenstern (Abdock-Symbol in der Ecke) steht „Skizze 12“ in der Namenszeile der
/// Palette ungekürzt, angedockt und abgedockt (so breit wie die Spalte); ein sehr langer Name
/// kürzt sich weiter.
#[test]
fn sketch_name_is_not_cut_next_to_the_dock_icon() {
    let column = DetachedWindow {
        width: theme::PANEL_WIDTH,
        ..window_at(1500.0, 100.0)
    };
    for detached in [vec![], vec![(Place::SketchSide, column)]] {
        for (name, want_cut) in [("Skizze 12", false), ("Eine sehr lange Skizze 12", true)] {
            let mut rig = Rig::new(&detached);
            open_sketch(&mut rig);
            rig.state().session_sketch_mut().unwrap().1.name = name.to_string();
            rig.settle(3);
            let (cut, rect) =
                planora_app::sketch_panel::sketch_name_label(&rig.screen.ctx).expect("Name");
            assert_eq!(
                cut,
                want_cut,
                "„{name}“, {} px breit, abgedockt: {}",
                rect.width(),
                !detached.is_empty()
            );
        }
    }
}

// ---- Aufklappfeld „Anzeigeoptionen“ bei abgedocktem Skizzenfeld (Skizzen 12, Fix-Runde 1) --------

fn detached_sketch_rig() -> Rig {
    let column = DetachedWindow {
        width: theme::PANEL_WIDTH,
        ..window_at(1500.0, 100.0)
    };
    let mut rig = Rig::new(&[(Place::SketchSide, column)]);
    open_sketch(&mut rig);
    rig.settle(4);
    rig
}

fn point_count(rig: &mut Rig) -> usize {
    rig.state().session_sketch().unwrap().points().count()
}

#[test]
fn undocked_display_menu_opens_by_click_and_by_command_and_closes_with_the_sketch() {
    let mut rig = Rig::new(&[(
        Place::SketchSide,
        DetachedWindow {
            width: theme::PANEL_WIDTH,
            ..window_at(1500.0, 100.0)
        },
    )]);
    let id = detach::viewport_id(Place::SketchSide);
    let sketch = open_sketch(&mut rig);
    rig.settle(4);
    let ctx = rig.screen.ctx.clone();
    assert!(!planora_app::sketch_panel::display_menu_open(&ctx));
    // Befehl: öffnet das Feld im Kindfenster.
    planora_app::commands::run_action(
        rig.app.ws_mut(),
        planora_app::keymap::Action::SketchDisplayMenu,
    );
    rig.settle(3);
    assert!(
        planora_app::sketch_panel::display_menu_open(&ctx),
        "Befehl öffnet"
    );
    let child = rig.screen.child_seen(id);
    assert!(find(&child, "Fang").is_some(), "{}", join(&child));
    // Zweiter Aufruf schließt.
    planora_app::commands::run_action(
        rig.app.ws_mut(),
        planora_app::keymap::Action::SketchDisplayMenu,
    );
    rig.settle(3);
    assert!(!planora_app::sketch_panel::display_menu_open(&ctx));
    // Skizzenende schließt. (Klick auf das Symbol im Kindfenster: siehe
    // `undocked_sketch_header_buttons_take_clicks`.)
    planora_app::commands::run_action(
        rig.app.ws_mut(),
        planora_app::keymap::Action::SketchDisplayMenu,
    );
    rig.settle(3);
    assert!(planora_app::sketch_panel::display_menu_open(&ctx));
    rig.state().close_sketch();
    rig.settle(3);
    assert!(
        !planora_app::sketch_panel::display_menu_open(&ctx),
        "Skizzenende"
    );
    rig.state().open_sketch(sketch).unwrap();
    rig.settle(3);
    assert!(!planora_app::sketch_panel::display_menu_open(&ctx));
}

#[test]
fn undocked_display_menu_open_eats_the_click_into_the_drawing() {
    let mut rig = detached_sketch_rig();
    {
        let (se, sk) = rig.state().session_sketch_mut().unwrap();
        se.set_tool(sk, planora_app::sketch_state::Tool::Point);
    }
    let ctx = rig.screen.ctx.clone();
    let at = pos2(700.0, 500.0);
    // Ohne Feld setzt der Klick einen Punkt.
    let before = point_count(&mut rig);
    rig.click(at);
    assert_eq!(
        point_count(&mut rig),
        before + 1,
        "Gegenprobe: Punkt gesetzt"
    );
    // Mit offenem Feld schließt der Klick nur das Feld.
    planora_app::commands::run_action(
        rig.app.ws_mut(),
        planora_app::keymap::Action::SketchDisplayMenu,
    );
    rig.settle(3);
    assert!(planora_app::sketch_panel::display_menu_open(&ctx));
    let before = point_count(&mut rig);
    rig.click(pos2(900.0, 300.0));
    assert_eq!(point_count(&mut rig), before, "kein Punkt bei offenem Feld");
    assert!(!planora_app::sketch_panel::display_menu_open(&ctx), "zu");
}

#[test]
fn undocked_display_menu_closes_with_escape_in_main_without_ending_the_tool() {
    let mut rig = detached_sketch_rig();
    {
        let (se, sk) = rig.state().session_sketch_mut().unwrap();
        se.set_tool(sk, planora_app::sketch_state::Tool::Line);
    }
    let ctx = rig.screen.ctx.clone();
    planora_app::commands::run_action(
        rig.app.ws_mut(),
        planora_app::keymap::Action::SketchDisplayMenu,
    );
    rig.settle(3);
    assert!(planora_app::sketch_panel::display_menu_open(&ctx));
    let esc = || Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    rig.frame(vec![esc()]);
    rig.settle(2);
    assert!(
        !planora_app::sketch_panel::display_menu_open(&ctx),
        "Esc im Hauptfenster schließt das Feld"
    );
    assert_eq!(
        rig.state().sketch_session.as_ref().unwrap().tool,
        planora_app::sketch_state::Tool::Line,
        "das Werkzeug bleibt"
    );
}

// ---- Kopfzeile abgedockter Felder: Knöpfe klickbar, Rest zieht das Fenster -------------------

#[test]
fn undocked_sketch_header_buttons_take_clicks() {
    let mut rig = detached_sketch_rig();
    let id = detach::viewport_id(Place::SketchSide);
    let ctx = rig.screen.ctx.clone();
    // Symbol „Anzeigeoptionen“ öffnet das Feld.
    let icon = center_of(&rig.screen.child_seen(id), &glyph(Icon::Display));
    assert!(!planora_app::sketch_panel::display_menu_open(&ctx));
    rig.click_child(id, icon, PointerButton::Primary);
    assert!(
        planora_app::sketch_panel::display_menu_open(&ctx),
        "Klick auf „Anzeigeoptionen“ im Kindfenster öffnet das Feld"
    );
    // Zweiter Klick schließt es wieder.
    rig.click_child(id, icon, PointerButton::Primary);
    assert!(!planora_app::sketch_panel::display_menu_open(&ctx));
    // Haken „3D-Skizze“ schaltet die Skizze um.
    assert!(!rig.state().session_sketch().unwrap().three_d);
    let three_d = center_of(&rig.screen.child_seen(id), "3D-Skizze");
    rig.click_child(id, three_d, PointerButton::Primary);
    assert!(
        rig.state().session_sketch().unwrap().three_d,
        "Klick auf „3D-Skizze“ im Kindfenster"
    );
    // Andocken liegt weiter ganz rechts darüber und dockt an.
    let dock = center_of(&rig.screen.child_seen(id), &glyph(Icon::Dock));
    rig.click_child(id, dock, PointerButton::Primary);
    rig.settle(2);
    assert!(rig.app.ws_mut().detach.get(Place::SketchSide).is_none());
}

/// Drückt in der Kopfzeile des Kindfensters `id` an `at` und zieht; `true`, wenn das Fenster
/// dabei gezogen wird (`StartDrag`).
fn drag_starts_window(rig: &mut Rig, id: egui::ViewportId, at: Pos2) -> bool {
    let before = rig.screen.child_commands(id).len();
    rig.frame_child(id, vec![Event::PointerMoved(at)]);
    rig.frame_child(id, vec![press_at(at, true)]);
    rig.frame_child(id, vec![Event::PointerMoved(at + vec2(30.0, 10.0))]);
    let started =
        rig.screen.child_commands(id)[before..].contains(&egui::ViewportCommand::StartDrag);
    rig.frame_child(id, vec![press_at(at + vec2(30.0, 10.0), false)]);
    rig.settle(2);
    started
}

#[test]
fn undocked_sketch_header_still_drags_the_window() {
    let mut rig = detached_sketch_rig();
    let id = detach::viewport_id(Place::SketchSide);
    let ctx = rig.screen.ctx.clone();
    let (_, name) = planora_app::sketch_panel::sketch_name_label(&ctx).expect("Name");
    // Auf dem Namen und auf der freien Fläche zwischen Name und „3D-Skizze“.
    let three_d = find(&rig.screen.child_seen(id), "3D-Skizze")
        .expect("3D-Skizze")
        .rect;
    // Der Haken beginnt links mit seinem Kästchen (etwa 20 Punkte vor dem Text); die Lücke liegt
    // mittig zwischen Name und Kästchen, weit genug von beiden (egui gibt das Ziehen sonst dem
    // nahen Namen, der Text auswählen kann).
    let gap = pos2((name.max.x + three_d.min.x - 20.0) / 2.0, name.center().y);
    assert!(
        gap.x > name.max.x + 8.0 && gap.x < three_d.min.x - 28.0,
        "Lücke {gap:?} / Name {name:?} / {three_d:?}"
    );
    assert!(drag_starts_window(&mut rig, id, gap), "freie Fläche");
    assert!(drag_starts_window(&mut rig, id, name.center()), "Name");
    assert!(!planora_app::sketch_panel::display_menu_open(&ctx));
    assert!(!rig.state().session_sketch().unwrap().three_d);
}

#[test]
fn undocked_right_switch_takes_clicks() {
    let mut rig = Rig::new(&[(Place::Right, window_at(1500.0, 100.0))]);
    rig.app.shell_mut().set_versions_visible(true);
    rig.settle(3);
    let id = detach::viewport_id(Place::Right);
    let versions = center_of(&rig.screen.child_seen(id), "Versionen");
    rig.click_child(id, versions, PointerButton::Primary);
    assert_eq!(
        rig.app.shell_mut().effective_right_tab(),
        planora_app::shell::RightTab::Versions,
        "Umschalter „Versionen“ im Kindfenster"
    );
}
