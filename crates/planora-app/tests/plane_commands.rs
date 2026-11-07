//! Skizzen 6, Task 12: Menü „Konstruieren“ mit den fünf Konstruktionsebenen (Fenster mit
//! Auswahlfeldern, Formelfeld und Vorschau), Ordner „Konstruktion“ im Browser mit Kontextmenü
//! und die belegbaren Aktionen. Ohne Fenster: Zustand, `Workspace` und egui-Bilder (1920×1080).

mod common;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2};
use planora_app::browser;
use planora_app::commands;
use planora_app::keymap::{Action, Area, Keymap, shortcut_hint};
use planora_app::plane_dialog::{PlaneClick, PlaneCommand, PlaneDialog, SlotValue};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{app, fonts};
use planora_feature::planes::{PlaneBase, PlaneDef, PointRef};
use planora_feature::{FeatureId, FeatureKind, FeatureStatus, SketchPlane, StandardPlane};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::Pick;
use std::collections::BTreeMap;

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};

fn ws() -> Workspace {
    let root = common::temp_root("planora-plane-commands");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn undo_len(w: &Workspace) -> usize {
    w.active_tab().history().undo_len()
}

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn raw(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

fn click(pos: Pos2, button: PointerButton) -> Vec<Event> {
    let press = |pressed| Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::default(),
    };
    vec![Event::PointerMoved(pos), press(true), press(false)]
}

/// Mitte des ersten Textes, der mit `prefix` beginnt.
fn find_text(out: &egui::FullOutput, prefix: &str) -> Option<Pos2> {
    fn walk(shape: &egui::epaint::Shape, prefix: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text().starts_with(prefix) => {
                Some(t.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, prefix)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|c| walk(&c.shape, prefix))
}

/// Ein Bild des Baums; liefert alle Texte und die Lage von `prefix`.
fn tree_frame(
    c: &egui::Context,
    s: &mut AppState,
    events: Vec<Event>,
    prefix: &str,
) -> (String, Option<Pos2>) {
    let out = c.run_ui(raw(events), |ui| browser::show(ui, s));
    let r = (
        planora_app::testing::collect_texts(&out),
        find_text(&out, prefix),
    );
    out.drop_without_applying_deltas();
    r
}

/// Rechtsklick auf die Zeile `row`, dann Klick auf den Menüeintrag `entry`; liefert die Texte
/// des offenen Menüs.
fn context_menu(s: &mut AppState, row: &str, entry: &str) -> String {
    let c = ctx();
    let (_, pos) = tree_frame(&c, s, vec![], row);
    let pos = pos.unwrap_or_else(|| panic!("Zeile {row}"));
    tree_frame(&c, s, click(pos, PointerButton::Secondary), "");
    let (menu, item) = tree_frame(&c, s, vec![], entry);
    let item = item.unwrap_or_else(|| panic!("Menüeintrag {entry}:\n{menu}"));
    tree_frame(&c, s, click(item, PointerButton::Primary), "");
    menu
}

/// Versatzebene 10 mm über XY, angelegt über den Zustand.
fn offset_plane(s: &mut AppState, z: f64) -> FeatureId {
    s.add_plane(PlaneDef::Offset {
        base: PlaneBase::Standard(StandardPlane::XY),
        distance: z,
    })
    .unwrap()
}

#[test]
fn offset_plane_dialog_ok_adds_one_step() {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(20.0))
        .unwrap();
    w.observe_history(false);
    let before = undo_len(&w);
    // Menü „Konstruieren“ → „Versatzebene“ öffnet das Fenster.
    commands::run_action(&mut w, Action::PlaneOffset);
    let tab = w.active_tab_mut();
    let mut d = tab.plane_dialog.take().expect("Fenster offen");
    assert_eq!(d.command(), PlaneCommand::Offset);
    assert_eq!(d.title(), "Versatzebene");
    assert_eq!(d.number_label().as_deref(), Some("Abstand"));
    // Die Deckfläche des Quaders als Basis, angeklickt in der Ansicht (Filter mit Ebenen).
    let s = &mut tab.state;
    let top = s.bodies[0]
        .solid
        .faces
        .iter()
        .find(|(id, _)| {
            planora_sketch::plane_of_face(&s.bodies[0].solid, *id)
                .is_some_and(|f| f.z.z > 0.5 && f.origin.z > 19.0)
        })
        .map(|(id, _)| id)
        .unwrap();
    s.plane_pick = None;
    let _ = d.sync(s);
    assert_eq!(
        s.plane_pick.map(|p| p.filter),
        Some(planora_app::state::PickFilter::SELECT)
    );
    s.plane_pick.as_mut().unwrap().clicked =
        Some(PlaneClick::View(Pick::Face(planora_render::FaceRef {
            body: 0,
            face: top,
        })));
    d.set_text("5 + 5");
    let edits = s.document.edit_count();
    let _ = d.sync(s);
    assert!(matches!(
        d.slot(0),
        Some(SlotValue::Base(PlaneBase::Face(_)))
    ));
    // Vorschau 10 mm über der Deckfläche, ohne das Dokument zu ändern.
    let preview = s
        .plane_pick
        .and_then(|p| p.preview)
        .unwrap_or_else(|| panic!("Vorschau: {:?}", d.preview(s).err().map(|m| m.text())));
    assert!((preview.frame.origin.z - 30.0).abs() < 1e-9, "{preview:?}");
    assert_eq!(s.document.edit_count(), edits);
    assert_eq!(s.document.features().len(), 1);
    // „OK“: genau ein Schritt mit dem Namen des Befehls; die Formel bleibt.
    let id = d.ok(s).unwrap();
    let f = s.document.feature(id).unwrap();
    assert_eq!(f.name, "Ebene 1");
    assert_eq!(f.status, FeatureStatus::Ok);
    assert!(
        f.formulas().values().any(|t| t.contains('5')),
        "{:?}",
        f.formulas()
    );
    let frame = s.document.plane_frame(id).unwrap();
    assert!((frame.origin.z - 30.0).abs() < 1e-9);
    w.observe_history(false);
    assert_eq!(undo_len(&w), before + 1);
    assert_eq!(
        w.active_tab().undo_label().as_deref(),
        Some("Versatzebene anlegen")
    );
    // „Bearbeiten“ öffnet dasselbe Fenster mit den Werten; OK ist wieder ein Schritt.
    let tab = w.active_tab_mut();
    tab.open_edit(id);
    let mut e = tab.plane_dialog.take().expect("Fenster der Ebene");
    assert!(tab.feature_dialog.is_none());
    assert_eq!(e.editing(), Some(id));
    assert!(e.text().contains('5'), "{}", e.text());
    e.set_text("-4");
    e.ok(&mut tab.state).unwrap();
    let frame = tab.state.document.plane_frame(id).unwrap();
    assert!((frame.origin.z - 16.0).abs() < 1e-9);
    w.observe_history(false);
    assert_eq!(undo_len(&w), before + 2);
    assert_eq!(w.active_tab().undo_label().as_deref(), Some("Ebene ändern"));
}

#[test]
fn cancel_leaves_nothing() {
    let mut w = ws();
    w.observe_history(false);
    let before = undo_len(&w);
    for cmd in PlaneCommand::ALL {
        app::open_plane(&mut w, cmd);
        let tab = w.active_tab_mut();
        let mut d = tab.plane_dialog.take().unwrap();
        let s = &mut tab.state;
        let edits = s.document.edit_count();
        // Ein Feld füllen (wo es geht), dann Esc: Das Fenster schließt, nichts bleibt.
        if cmd == PlaneCommand::Offset || cmd == PlaneCommand::Mid {
            assert!(d.take_click(s, PlaneClick::Standard(StandardPlane::XZ)));
        }
        let c = ctx();
        let out = c.run_ui(raw(vec![]), |ui| {
            d.show(ui.ctx(), s);
        });
        out.drop_without_applying_deltas();
        assert!(s.plane_pick.is_some(), "{cmd:?}: Auswahl aktiv");
        let mut open = true;
        let out = c.run_ui(
            raw(vec![Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }]),
            |ui| open = d.show(ui.ctx(), s),
        );
        out.drop_without_applying_deltas();
        assert!(!open, "{cmd:?}: Esc schließt");
        assert!(s.plane_pick.is_none(), "{cmd:?}");
        assert_eq!(s.document.edit_count(), edits, "{cmd:?}");
        assert!(s.document.features().is_empty(), "{cmd:?}");
    }
    // „Abbrechen“ über das Schließen des Tabs-Fensters: das Fenster ist weg, die Auswahl auch.
    app::open_plane(&mut w, PlaneCommand::Angle);
    w.active_tab_mut().close_dialogs();
    assert!(w.active_tab().plane_dialog.is_none());
    assert!(w.active_tab().state.plane_pick.is_none());
    w.observe_history(false);
    assert_eq!(undo_len(&w), before);
}

#[test]
fn browser_lists_planes_in_construction_folder() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    let c = ctx();
    // Ohne Ebene kein Ordner „Konstruktion“ (wie Fusion).
    let (text, _) = tree_frame(&c, &mut s, vec![], "");
    assert!(!text.contains("Konstruktion"), "{text}");
    let first = offset_plane(&mut s, 10.0);
    // Zwei zusammenfallende Ebenen: Fehler, der im Baum und in den Eigenschaften steht.
    let bad = s
        .add_plane(PlaneDef::Mid {
            a: PlaneBase::Standard(StandardPlane::XY),
            b: PlaneBase::Standard(StandardPlane::XY),
        })
        .unwrap();
    assert!(matches!(
        s.document.feature(bad).unwrap().status,
        FeatureStatus::Error(_)
    ));
    let (text, folder) = tree_frame(&c, &mut s, vec![], "Konstruktion");
    let (_, one) = tree_frame(&c, &mut s, vec![], "Ebene 1");
    let (_, two) = tree_frame(&c, &mut s, vec![], "Ebene 2");
    let (folder, one, two) = (folder.unwrap(), one.unwrap(), two.unwrap());
    assert!(folder.y < one.y && one.y < two.y, "{text}");
    assert!(text.contains("Fehler"), "{text}");
    let status = browser::status_text(&s).unwrap();
    assert!(status.contains("Ebene 2"), "{status}");
    // Skizzen stehen darüber, Zeichnungen darunter.
    let (_, sketches) = tree_frame(&c, &mut s, vec![], "Skizzen");
    let (_, drawings) = tree_frame(&c, &mut s, vec![], "Zeichnungen");
    assert!(sketches.unwrap().y < folder.y && folder.y < drawings.unwrap().y);
    // Eigenschaften: Befehl, Bezug und Abstand.
    let kind = s.document.feature(first).unwrap().kind.clone();
    let rows = planora_app::properties::parameter_rows(&s, &kind);
    let joined = format!("{rows:?}");
    for must in ["Versatzebene", "XY-Ebene", "Abstand"] {
        assert!(joined.contains(must), "{must}: {joined}");
    }
    // Ein Klick auf die Zeile wählt die Ebene (hervorgehoben in der Ansicht).
    tree_frame(&c, &mut s, click(one, PointerButton::Primary), "");
    assert_eq!(s.selected_feature, Some(first));
    assert_eq!(s.selected_plane(), Some(first));
    assert_eq!(
        s.sketch_on_face_blocked(),
        None,
        "„Skizze auf Fläche“ nimmt die Ebene"
    );
}

#[test]
fn sketch_from_plane_context_menu() {
    let mut s = AppState::new();
    let plane = offset_plane(&mut s, 10.0);
    let menu = context_menu(&mut s, "Ebene 1", "Skizze erstellen");
    for must in [
        "Bearbeiten",
        "Skizze erstellen",
        "Umbenennen",
        "Unterdrücken",
        "Löschen",
    ] {
        assert!(menu.contains(must), "„{must}“ fehlt:\n{menu}");
    }
    let sketch = s.open_sketch_id().expect("Skizze offen");
    match &s.document.feature(sketch).unwrap().kind {
        FeatureKind::Sketch(sk) => assert_eq!(sk.plane, SketchPlane::OnPlane(plane)),
        k => panic!("{k:?}"),
    }
    s.close_sketch();
    // „Bearbeiten“ aus dem Menü fordert das Fenster der Ebene an.
    context_menu(&mut s, "Ebene 1", "Bearbeiten");
    assert_eq!(s.edit_request, Some(plane));
    let d = PlaneDialog::edit(&s, plane).unwrap();
    assert_eq!(d.command(), PlaneCommand::Offset);
    assert_eq!(
        d.slot(0),
        Some(&SlotValue::Base(PlaneBase::Standard(StandardPlane::XY)))
    );
    // Unterdrücken aus dem Menü.
    context_menu(&mut s, "Ebene 1", "Unterdrücken");
    assert!(s.document.feature(plane).unwrap().suppressed);
}

#[test]
fn all_plane_actions_are_bindable() {
    let ids = [
        ("konstruieren.versatzebene", PlaneCommand::Offset),
        ("konstruieren.winkelebene", PlaneCommand::Angle),
        ("konstruieren.mittelebene", PlaneCommand::Mid),
        ("konstruieren.drei_punkte", PlaneCommand::ThreePoints),
        ("konstruieren.tangentialebene", PlaneCommand::Tangent),
    ];
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (k, (id, cmd)) in ids.iter().enumerate() {
        let a = Action::from_id(id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(a, cmd.action());
        assert_eq!(a.area(), Area::Model);
        // Ohne Vorgabetaste wie Fusion, Name wie der Befehl.
        assert!(a.default_bindings().is_empty(), "{id}");
        assert_eq!(a.label(), cmd.label());
        assert!(a.repeatable(), "{id}");
        map.insert(id.to_string(), vec![format!("Strg+Alt+{}", k + 1)]);
    }
    let (km, notes) = Keymap::from_overrides(&map);
    assert!(notes.is_empty(), "{notes:?}");
    for (k, (_, cmd)) in ids.iter().enumerate() {
        assert_eq!(
            shortcut_hint(&km, cmd.action()),
            format!("Strg+Alt+{}", k + 1)
        );
        // Über die Taste öffnet sich das Fenster des Befehls.
        let mut w = ws();
        commands::run_key(&mut w, cmd.action());
        let d = w.active_tab().plane_dialog.as_ref().expect("Fenster");
        assert_eq!(d.command(), *cmd);
    }
    // Ebene durch drei Punkte: der Ursprung lässt sich ohne Klick in die Ansicht wählen.
    let mut s = AppState::new();
    let mut d = PlaneDialog::new(PlaneCommand::ThreePoints);
    d.fill(SlotValue::Point(PointRef::Origin));
    assert_eq!(d.active_slot(), 1);
    assert!(!d.take_click(&s, PlaneClick::Standard(StandardPlane::XY)));
    assert!(d.error().is_some());
    assert!(d.ok(&mut s).is_err(), "leere Felder");
    assert!(s.document.features().is_empty());
}
