//! Skizzen 11b, Task 9: Skizzengruppen im Browser-Ordner „Skizzen“ – Baum (erst Gruppen, dann
//! Skizzen), Zuklappen, Strg-Klick-Mehrfachwahl mit „Gruppe erstellen“, Auge der Gruppe, Ziehen
//! in eine Gruppe und heraus, Umbenennen-Dialog und Auflösen; je Änderung ein Rückgängig-Schritt.
//! Ohne Fenster bei 1920 × 1080.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2};
use planora_app::browser::{self, Folder};
use planora_app::detach::DIALOG_KEYS;
use planora_app::sketch_tree::{drop_target_ok, sketch_tree};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{FeatureId, GroupItem, SketchGroupId, SketchPlane, StandardPlane};
use planora_project::config::Config;
use std::collections::BTreeSet;

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};

const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

/// Ein Tab mit `n` Skizzen („Skizze 1“ …) auf XY und ein egui-Kontext für den Baum.
struct Rig {
    w: Workspace,
    c: egui::Context,
    time: f64,
    sketches: Vec<FeatureId>,
}

fn rig(n: usize) -> Rig {
    let mut w = Workspace::new(None, Config::default());
    let s = &mut w.active_tab_mut().state;
    let sketches = (0..n)
        .map(|_| {
            s.add_sketch(SketchPlane::Standard(StandardPlane::XY))
                .unwrap()
        })
        .collect();
    w.observe_history(false);
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    let mut r = Rig {
        w,
        c,
        time: 0.0,
        sketches,
    };
    r.frame(vec![], Modifiers::NONE);
    r.frame(vec![], Modifiers::NONE);
    r
}

/// Texte eines Bilds mit ihrer Mitte.
fn texts(out: &egui::FullOutput) -> Vec<(String, Pos2)> {
    fn walk(shape: &egui::epaint::Shape, into: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::epaint::Shape::Text(t) => into.push((
                t.galley.text().to_string(),
                t.visual_bounding_rect().center(),
            )),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, into)),
            _ => {}
        }
    }
    let mut v = Vec::new();
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut v));
    v
}

impl Rig {
    fn s(&mut self) -> &mut AppState {
        &mut self.w.active_tab_mut().state
    }

    fn undo_len(&self) -> usize {
        self.w.active_tab().history().undo_len()
    }

    fn label(&self) -> Option<String> {
        self.w.active_tab().undo_label()
    }

    /// Ein Bild des Baums, danach wie die App: Änderungen werden Schritte.
    fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) -> Vec<(String, Pos2)> {
        // Kurze Bilder (ein Klick bleibt ein Klick); vor jedem Drücken eine Pause, damit zwei
        // Klicks nacheinander kein Doppelklick werden.
        let pause = events
            .iter()
            .any(|e| matches!(e, Event::PointerButton { pressed: true, .. }));
        self.time += if pause { 1.0 } else { 0.05 };
        let mut events = events;
        events.insert(0, Event::ModifiersChanged(modifiers));
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let c = self.c.clone();
        let out = c.run_ui(raw, |ui| browser::show(ui, self.s()));
        let t = texts(&out);
        out.drop_without_applying_deltas();
        self.w.observe_history(false);
        t
    }

    fn settle(&mut self) -> Vec<(String, Pos2)> {
        self.frame(vec![], Modifiers::NONE);
        self.frame(vec![], Modifiers::NONE)
    }

    fn row(&self, name: &str) -> Rect {
        browser::row_rects(&self.c)
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("Zeile „{name}“ fehlt"))
            .1
    }

    fn has_row(&self, name: &str) -> bool {
        browser::row_rects(&self.c).iter().any(|(n, _)| n == name)
    }

    fn eye(&self, name: &str) -> Rect {
        browser::eye_rects(&self.c)
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("Auge bei „{name}“ fehlt"))
            .1
    }

    fn folder(&self, f: Folder) -> Rect {
        browser::folder_rects(&self.c)
            .into_iter()
            .find(|(g, _)| *g == f)
            .unwrap()
            .1
    }

    fn click_with(&mut self, p: Pos2, button: PointerButton, modifiers: Modifiers) {
        let b = |pressed| Event::PointerButton {
            pos: p,
            button,
            pressed,
            modifiers,
        };
        self.frame(vec![Event::PointerMoved(p)], modifiers);
        self.frame(vec![b(true)], modifiers);
        self.frame(vec![b(false)], modifiers);
        self.frame(vec![], Modifiers::NONE);
    }

    fn click(&mut self, p: Pos2) {
        self.click_with(p, PointerButton::Primary, Modifiers::NONE);
    }

    /// Rechtsklick auf die Zeile `row`, dann Klick auf den Menüeintrag `entry`.
    fn menu(&mut self, row: &str, entry: &str) {
        let p = self.row(row).center();
        self.click_with(p, PointerButton::Secondary, Modifiers::NONE);
        let t = self.frame(vec![], Modifiers::NONE);
        let at = t
            .iter()
            .find(|(s, _)| s == entry)
            .unwrap_or_else(|| panic!("Eintrag „{entry}“ fehlt: {t:?}"))
            .1;
        self.click(at);
    }

    /// Zieht mit der linken Taste von `from` nach `to`.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        let b = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(from)], Modifiers::NONE);
        self.frame(vec![b(from, true)], Modifiers::NONE);
        for i in 1..=8 {
            let p = from + (to - from) * (i as f32 / 8.0);
            self.frame(vec![Event::PointerMoved(p)], Modifiers::NONE);
        }
        self.frame(vec![b(to, false)], Modifiers::NONE);
        self.settle();
    }

    fn group_of(&mut self, id: FeatureId) -> Option<SketchGroupId> {
        self.s().document.sketch_groups().member.get(&id).copied()
    }

    /// Legt eine Gruppe aus `items` an (ohne Baum), klappt sie auf und macht einen Schritt.
    fn group(&mut self, items: &[GroupItem]) -> SketchGroupId {
        let g = self.s().document.create_sketch_group(items).unwrap();
        self.s().tree_groups_open.insert(g);
        self.w.observe_history(false);
        self.settle();
        g
    }
}

#[test]
fn tree_order_groups_then_sketches() {
    let mut r = rig(4);
    let [a, b, c, d] = [r.sketches[0], r.sketches[1], r.sketches[2], r.sketches[3]];
    let g1 = r.group(&[GroupItem::Sketch(c)]);
    let g2 = r.group(&[GroupItem::Sketch(b)]);
    let open: BTreeSet<SketchGroupId> = [g1, g2].into();
    let rows = sketch_tree(&r.s().document, &open);
    let items: Vec<(GroupItem, usize)> = rows.iter().map(|t| (t.item, t.depth)).collect();
    assert_eq!(
        items,
        vec![
            (GroupItem::Group(g1), 0),
            (GroupItem::Sketch(c), 1),
            (GroupItem::Group(g2), 0),
            (GroupItem::Sketch(b), 1),
            (GroupItem::Sketch(a), 0),
            (GroupItem::Sketch(d), 0),
        ]
    );
    assert_eq!(rows[0].name, "Gruppe1");
    assert_eq!(rows[1].name, "Skizze 3");
    // Der Baum zeichnet in derselben Reihenfolge.
    let y = |r: &Rig, n: &str| r.row(n).center().y;
    let order = [
        "Gruppe1", "Skizze 3", "Gruppe2", "Skizze 2", "Skizze 1", "Skizze 4",
    ];
    for w in order.windows(2) {
        assert!(y(&r, w[0]) < y(&r, w[1]), "{} vor {}", w[0], w[1]);
    }
}

#[test]
fn collapsed_group_hides_rows() {
    let mut r = rig(3);
    let [a, b, c] = [r.sketches[0], r.sketches[1], r.sketches[2]];
    let outer = r.group(&[GroupItem::Sketch(a)]);
    let inner = r.group(&[GroupItem::Sketch(b)]);
    r.s()
        .document
        .move_to_group(GroupItem::Group(inner), Some(outer))
        .unwrap();
    // Nur die äußere offen: die innere Gruppe steht da, ihr Inhalt fehlt.
    let rows = sketch_tree(&r.s().document, &[outer].into());
    let items: Vec<GroupItem> = rows.iter().map(|t| t.item).collect();
    assert_eq!(
        items,
        vec![
            GroupItem::Group(outer),
            GroupItem::Group(inner),
            GroupItem::Sketch(a),
            GroupItem::Sketch(c),
        ]
    );
    assert_eq!(rows[1].depth, 1);
    // Alles zu: nur die äußere Gruppe und die freie Skizze.
    let rows = sketch_tree(&r.s().document, &BTreeSet::new());
    let items: Vec<GroupItem> = rows.iter().map(|t| t.item).collect();
    assert_eq!(items, vec![GroupItem::Group(outer), GroupItem::Sketch(c)]);
    // Im Baum: Klick auf den Pfeil der äußeren Gruppe klappt sie zu (nur die Ansicht).
    r.settle();
    assert!(r.has_row("Skizze 1"));
    let edits = r.s().document.edit_count();
    let row = r.row("Gruppe1");
    let caret = pos2(row.min.x + 6.0, row.center().y);
    r.click(caret);
    r.settle();
    assert!(!r.s().tree_groups_open.contains(&outer));
    assert!(!r.has_row("Skizze 1") && !r.has_row("Gruppe2"));
    assert!(r.has_row("Skizze 3"));
    assert_eq!(
        r.s().document.edit_count(),
        edits,
        "Zuklappen ändert nichts"
    );
}

#[test]
fn ctrl_click_then_create_group() {
    let mut r = rig(3);
    let [a, b, c] = [r.sketches[0], r.sketches[1], r.sketches[2]];
    let before = r.undo_len();
    let p1 = r.row("Skizze 1").center();
    let p2 = r.row("Skizze 2").center();
    r.click_with(p1, PointerButton::Primary, CTRL);
    r.click_with(p2, PointerButton::Primary, CTRL);
    assert_eq!(r.s().browser_sketches, vec![a, b]);
    r.menu("Skizze 2", "Gruppe erstellen");
    r.settle();
    assert_eq!(r.undo_len(), before + 1, "genau ein Schritt");
    assert_eq!(r.label().as_deref(), Some("Gruppe erstellen"));
    let g = r.group_of(a).expect("Skizze 1 in der Gruppe");
    assert_eq!(r.group_of(b), Some(g));
    assert_eq!(r.group_of(c), None);
    assert!(r.has_row("Gruppe1"));
    // Ein einfacher Klick wählt nur diese eine Skizze.
    let p3 = r.row("Skizze 3").center();
    r.click(p3);
    assert_eq!(r.s().browser_sketches, vec![c]);
    assert_eq!(r.s().selected_feature, Some(c));
}

#[test]
fn group_eye_hides_sketches_in_view() {
    let mut r = rig(3);
    let [a, b, c] = [r.sketches[0], r.sketches[1], r.sketches[2]];
    let g = r.group(&[GroupItem::Sketch(a), GroupItem::Sketch(b)]);
    let before = r.undo_len();
    let eye = r.eye("Gruppe1").center();
    r.click(eye);
    r.settle();
    assert!(r.s().document.sketch_groups().group(g).unwrap().hidden);
    for id in [a, b] {
        assert!(r.s().is_feature_hidden(id), "Mitglied ausgeblendet");
        assert!(!r.s().own_feature_hidden(id), "eigenes Auge unverändert");
    }
    assert!(!r.s().is_feature_hidden(c));
    assert_eq!(r.undo_len(), before + 1);
    assert_eq!(r.label().as_deref(), Some("Gruppe ein-/ausblenden"));
    // Wieder an: alles sichtbar, ein weiterer Schritt.
    let eye = r.eye("Gruppe1").center();
    r.click(eye);
    r.settle();
    assert!(!r.s().is_feature_hidden(a) && !r.s().is_feature_hidden(b));
    assert_eq!(r.undo_len(), before + 2);
}

#[test]
fn drop_into_and_out_of_group() {
    let mut r = rig(3);
    let [a, _, c] = [r.sketches[0], r.sketches[1], r.sketches[2]];
    let g = r.group(&[GroupItem::Sketch(a)]);
    let before = r.undo_len();
    // Skizze 3 auf die Gruppenzeile: hinein.
    let from = r.row("Skizze 3").center();
    let to = r.row("Gruppe1").center();
    r.drag(from, to);
    assert_eq!(r.group_of(c), Some(g));
    assert_eq!(r.undo_len(), before + 1, "ein Schritt");
    assert_eq!(r.label().as_deref(), Some("In Gruppe verschieben"));
    // Skizze 3 auf den Ordner „Skizzen“: wieder nach oben.
    let from = r.row("Skizze 3").center();
    let to = r.folder(Folder::Sketches).center();
    r.drag(from, to);
    assert_eq!(r.group_of(c), None);
    assert_eq!(r.undo_len(), before + 2, "noch ein Schritt");
    assert_eq!(r.label().as_deref(), Some("In Gruppe verschieben"));
    // Ziehen ohne Ziel (auf eine Skizzenzeile) ändert nichts.
    let from = r.row("Skizze 3").center();
    let to = r.row("Skizze 2").center();
    r.drag(from, to);
    assert_eq!(r.group_of(c), None);
    assert_eq!(r.undo_len(), before + 2);
}

#[test]
fn drop_group_on_own_child_refused() {
    let mut r = rig(2);
    let [a, b] = [r.sketches[0], r.sketches[1]];
    let outer = r.group(&[GroupItem::Sketch(a)]);
    let inner = r.group(&[GroupItem::Sketch(b)]);
    r.s()
        .document
        .move_to_group(GroupItem::Group(inner), Some(outer))
        .unwrap();
    r.w.observe_history(false);
    r.settle();
    let doc = &r.s().document;
    assert!(!drop_target_ok(doc, GroupItem::Group(outer), Some(inner)));
    assert!(!drop_target_ok(doc, GroupItem::Group(outer), Some(outer)));
    assert!(
        !drop_target_ok(doc, GroupItem::Group(inner), Some(outer)),
        "liegt schon dort"
    );
    assert!(drop_target_ok(doc, GroupItem::Group(inner), None));
    assert!(drop_target_ok(doc, GroupItem::Sketch(a), Some(inner)));
    assert!(!drop_target_ok(
        doc,
        GroupItem::Sketch(a),
        Some(SketchGroupId(99))
    ));
    // Im Baum: die äußere Gruppe auf die innere ziehen bleibt ohne Wirkung.
    let groups = r.s().document.sketch_groups().clone();
    let before = r.undo_len();
    let from = r.row("Gruppe1").center();
    let to = r.row("Gruppe2").center();
    r.drag(from, to);
    assert_eq!(*r.s().document.sketch_groups(), groups, "nichts geändert");
    assert_eq!(r.undo_len(), before);
}

#[test]
fn rename_dialog_has_dock_key() {
    assert!(DIALOG_KEYS.contains(&"tree_group_rename"));
    let src = include_str!("../src/browser.rs");
    assert!(src.contains(".dock_key(\"tree_group_rename\")"));
    let mut r = rig(1);
    let a = r.sketches[0];
    let g = r.group(&[GroupItem::Sketch(a)]);
    let before = r.undo_len();
    r.menu("Gruppe1", "Umbenennen");
    assert_eq!(
        r.s().group_rename_draft,
        Some((g, "Gruppe1".to_string())),
        "Fenster offen mit dem alten Namen"
    );
    r.s().group_rename_draft = Some((g, "  Halter  ".to_string()));
    let t = r.frame(vec![], Modifiers::NONE);
    let apply = t.iter().find(|(s, _)| s == "Übernehmen").unwrap().1;
    r.click(apply);
    r.settle();
    assert_eq!(r.s().group_rename_draft, None);
    assert_eq!(
        r.s().document.sketch_groups().group(g).unwrap().name,
        "Halter"
    );
    assert_eq!(r.undo_len(), before + 1);
    assert_eq!(r.label().as_deref(), Some("Gruppe umbenennen"));
    assert!(r.has_row("Halter"));
}

#[test]
fn undo_ungroup_restores_group() {
    let mut r = rig(3);
    let [a, b, c] = [r.sketches[0], r.sketches[1], r.sketches[2]];
    let g = r.group(&[GroupItem::Sketch(a), GroupItem::Sketch(b)]);
    let before = r.undo_len();
    r.menu("Gruppe1", "Gruppe auflösen");
    r.settle();
    assert!(r.s().document.sketch_groups().group(g).is_none());
    assert_eq!(r.group_of(a), None);
    assert_eq!(r.undo_len(), before + 1);
    assert_eq!(r.label().as_deref(), Some("Gruppe auflösen"));
    assert_eq!(r.s().document.features().len(), 3, "keine Skizze gelöscht");
    r.w.active_tab_mut().undo().unwrap();
    r.settle();
    assert!(r.s().document.sketch_groups().group(g).is_some());
    assert_eq!(r.group_of(a), Some(g));
    assert_eq!(r.group_of(b), Some(g));
    // „Löschen“ im Gruppenmenü löst ebenfalls auf und löscht keine Skizze.
    r.menu("Gruppe1", "Löschen");
    r.settle();
    assert!(r.s().document.sketch_groups().group(g).is_none());
    assert_eq!(r.label().as_deref(), Some("Gruppe auflösen"));
    for id in [a, b, c] {
        assert!(r.s().document.feature(id).is_some());
    }
}

// ---- Korrekturrunde 1 ----------------------------------------------------------------------

#[test]
fn a_3d_pick_after_a_group_leaves_the_group_alone_on_delete() {
    use planora_app::commands::run_action;
    use planora_app::keymap::Action;
    use planora_math::DVec3;
    use planora_render::{FaceRef, Pick};
    let mut r = rig(2);
    let a = r.sketches[0];
    let quader = r.s().create_box(DVec3::splat(4.0)).unwrap();
    r.w.observe_history(false);
    let g = r.group(&[GroupItem::Sketch(a)]);
    let p = r.row("Gruppe1").center();
    r.click(p);
    assert_eq!(r.s().selected_sketch_group(), Some(g), "Gruppe gewählt");
    // Klick in 3D auf eine Fläche: die Gruppe ist nicht mehr gewählt.
    let face = r.s().bodies[0].mesh.face_ranges[0].face;
    r.s()
        .set_selection(Some(Pick::Face(FaceRef { body: 0, face })));
    assert_eq!(r.s().browser_group, None);
    assert_eq!(r.s().selected_sketch_group(), None);
    run_action(&mut r.w, Action::DeleteSelection);
    r.w.observe_history(false);
    assert!(
        r.s().document.sketch_groups().group(g).is_some(),
        "Entf löst die alte Gruppe nicht auf"
    );
    // Auch ein über die Verlaufsleiste gewählter Körper verdrängt die Gruppe.
    r.click(p);
    assert_eq!(r.s().selected_sketch_group(), Some(g));
    r.s().select_body(Some(quader));
    assert_eq!(r.s().selected_sketch_group(), None);
}

#[test]
fn group_commands_are_one_step_each() {
    use planora_app::commands::run_action;
    use planora_app::keymap::Action;
    let mut r = rig(3);
    let [a, b] = [r.sketches[0], r.sketches[1]];
    let p1 = r.row("Skizze 1").center();
    let p2 = r.row("Skizze 2").center();
    r.click_with(p1, PointerButton::Primary, CTRL);
    r.click_with(p2, PointerButton::Primary, CTRL);
    let before = r.undo_len();
    run_action(&mut r.w, Action::SketchGroupCreate);
    r.settle();
    assert_eq!(r.undo_len(), before + 1, "genau ein Schritt");
    assert_eq!(r.label().as_deref(), Some("Gruppe erstellen"));
    let g = r.group_of(a).expect("in der Gruppe");
    assert_eq!(r.group_of(b), Some(g));
    let p = r.row("Gruppe1").center();
    r.click(p);
    run_action(&mut r.w, Action::SketchGroupUngroup);
    r.settle();
    assert_eq!(r.undo_len(), before + 2, "genau ein weiterer Schritt");
    assert_eq!(r.label().as_deref(), Some("Gruppe auflösen"));
    assert!(r.s().document.sketch_groups().group(g).is_none());
    assert_eq!(r.s().browser_group, None);
}

#[test]
fn undo_that_removes_the_chosen_group_clears_the_choice() {
    let mut r = rig(2);
    let a = r.sketches[0];
    let p = r.row("Skizze 1").center();
    r.click(p);
    r.menu("Skizze 1", "Gruppe erstellen");
    r.settle();
    let g = r.group_of(a).unwrap();
    let row = r.row("Gruppe1").center();
    r.click(row);
    assert_eq!(r.s().browser_group, Some(g));
    r.w.active_tab_mut().undo().unwrap();
    r.settle();
    assert!(r.s().document.sketch_groups().group(g).is_none());
    assert_eq!(r.s().browser_group, None);
}

#[test]
fn a_long_group_name_is_shortened_with_an_ellipsis() {
    let mut r = rig(1);
    let a = r.sketches[0];
    let g = r.group(&[GroupItem::Sketch(a)]);
    let long = "Sehr langer Gruppenname ".repeat(30).trim().to_string();
    r.s().document.rename_sketch_group(g, &long).unwrap();
    r.settle();
    // Gezeichnet werden nur die Zeichen bis „…“ (der Text des Auftrags bleibt der volle Name).
    fn shown(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(t) => out.push(
                t.galley
                    .rows
                    .iter()
                    .flat_map(|r| r.row.glyphs.iter().map(|g| g.chr))
                    .collect(),
            ),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| shown(s, out)),
            _ => {}
        }
    }
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        time: Some(r.time + 1.0),
        ..Default::default()
    };
    let c = r.c.clone();
    let out = c.run_ui(raw, |ui| browser::show(ui, r.s()));
    let mut drawn = Vec::new();
    out.shapes.iter().for_each(|c| shown(&c.shape, &mut drawn));
    out.drop_without_applying_deltas();
    assert!(
        drawn
            .iter()
            .any(|x| x.ends_with('…') && long.starts_with(x.trim_end_matches('…'))),
        "gekürzt mit …: {drawn:?}"
    );
    assert!(!drawn.contains(&long), "nicht in voller Länge");
}
