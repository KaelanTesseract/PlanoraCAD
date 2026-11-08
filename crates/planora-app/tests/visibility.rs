//! Handtest Skizzen 6 (2026-11-08): Jedes Objekt der 3D-Ansicht lässt sich im Browser aus- und
//! einblenden wie in Fusion: Körper, Skizzen, Konstruktionsebenen und die Grundebenen im Ordner
//! „Ursprung“ (anfangs ausgeblendet wie in Fusion), auch ganze Ordner. Das Modell bleibt;
//! ausgeblendete Objekte werden nicht gezeichnet und nicht angeklickt. Die Sichtbarkeit steht in
//! der Datei und ist ein Rückgängig-Schritt. Grundebenen lassen sich auch in der normalen
//! Auswahl wählen. Ohne
//! Fenster: Rohereignisse je Bild gegen einen egui-Kontext (1920×1080).

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2};
use glam::Vec2;
use planora_app::browser;
use planora_app::plane_dialog::PlaneClick;
use planora_app::sketch_view;
use planora_app::state::{AppState, PickFilter};
use planora_app::{fonts, theme};
use planora_feature::planes::{PlaneBase, PlaneDef};
use planora_feature::{FeatureId, SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};
use planora_render::Pick;

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};

/// Größe der 3D-Ansicht für die Treffer.
const VIEW: Vec2 = Vec2::new(1920.0, 1080.0);

struct Tree {
    ctx: egui::Context,
    time: f64,
}

impl Tree {
    fn new() -> Tree {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        Tree { ctx, time: 0.0 }
    }

    fn frame(&mut self, s: &mut AppState, events: Vec<Event>) {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| browser::show(ui, s));
        out.drop_without_applying_deltas();
    }

    fn settle(&mut self, s: &mut AppState) {
        self.frame(s, vec![]);
        self.frame(s, vec![]);
    }

    fn click(&mut self, s: &mut AppState, p: Pos2) {
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [
            vec![Event::PointerMoved(p)],
            vec![button(true)],
            vec![button(false)],
            vec![],
        ] {
            self.frame(s, events);
        }
    }

    fn eye(&self, name: &str) -> Option<Rect> {
        browser::eye_rects(&self.ctx)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| r)
    }

    /// Klickt das Auge der Zeile `name` (nach einem ruhigen Bild).
    fn toggle(&mut self, s: &mut AppState, name: &str) {
        self.settle(s);
        let eye = self
            .eye(name)
            .unwrap_or_else(|| panic!("Auge bei „{name}“ fehlt"));
        self.click(s, eye.center());
    }
}

fn sketch_with_rectangle(s: &mut AppState) -> FeatureId {
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0))
        .unwrap();
    s.close_sketch();
    id
}

fn offset_plane(s: &mut AppState, z: f64) -> FeatureId {
    s.add_plane(PlaneDef::Offset {
        base: PlaneBase::Standard(StandardPlane::XY),
        distance: z,
    })
    .unwrap()
}

#[test]
fn the_sketch_eye_hides_the_finished_sketch() {
    let mut s = AppState::new();
    let id = sketch_with_rectangle(&mut s);
    assert!(!sketch_view::finished_sketch_shapes(&s, SCREEN).is_empty());
    let selected = s.selected_feature;
    let mut t = Tree::new();
    t.toggle(&mut s, "Skizze 1");
    assert!(s.is_feature_hidden(id));
    assert!(
        sketch_view::finished_sketch_shapes(&s, SCREEN).is_empty(),
        "ausgeblendete Skizze wird nicht gezeichnet"
    );
    // Nur die Ansicht: Die Skizze bleibt im Dokument, das Auge ändert die Auswahl nicht.
    assert!(s.document.feature(id).is_some());
    assert_eq!(s.selected_feature, selected);
    t.toggle(&mut s, "Skizze 1");
    assert!(!s.is_feature_hidden(id));
    assert!(!sketch_view::finished_sketch_shapes(&s, SCREEN).is_empty());
}

#[test]
fn the_plane_eye_hides_the_plane_and_it_cannot_be_picked() {
    let mut s = AppState::new();
    let id = offset_plane(&mut s, 10.0);
    let center = VIEW / 2.0;
    assert_eq!(s.plane_rects().len(), 1);
    assert_eq!(s.pick(center, VIEW), Some(Pick::Plane(id)));
    s.set_selection(Some(Pick::Plane(id)));
    s.set_hover(Some(Pick::Plane(id)));
    let mut t = Tree::new();
    t.toggle(&mut s, "Ebene 1");
    assert!(s.is_feature_hidden(id));
    assert!(s.plane_rects().is_empty());
    assert!(sketch_view::plane_shapes(&s, SCREEN).is_empty());
    assert_eq!(s.pick(center, VIEW), None, "nicht mehr anklickbar");
    assert_eq!(s.selection, None, "Auswahl auf ihr entfällt");
    assert_eq!(s.hover, None);
    // Sie bleibt Basis für eine Skizze (Kontextmenü „Skizze erstellen“).
    assert!(s.document.plane_frame(id).is_some());
    t.toggle(&mut s, "Ebene 1");
    assert!(!s.is_feature_hidden(id));
    assert_eq!(s.pick(center, VIEW), Some(Pick::Plane(id)));
}

#[test]
fn origin_planes_start_hidden_and_the_eye_shows_them() {
    let mut s = AppState::new();
    for p in [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ] {
        assert!(!s.is_origin_plane_shown(p), "{p:?}: anfangs aus wie Fusion");
    }
    assert!(s.origin_plane_rects().is_empty());
    assert!(sketch_view::plane_shapes(&s, SCREEN).is_empty());
    // Der Ordner „Ursprung“ ist anfangs zugeklappt.
    s.tree_folders.set_open(browser::Folder::Origin, true);
    let mut t = Tree::new();
    t.toggle(&mut s, "XY-Ebene");
    assert!(s.is_origin_plane_shown(StandardPlane::XY));
    assert!(!s.is_origin_plane_shown(StandardPlane::XZ));
    let rects = s.origin_plane_rects();
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[0].0, StandardPlane::XY);
    assert!((rects[0].1.frame.z - DVec3::Z).length() < 1e-12);
    assert!(rects[0].1.frame.origin.length() < 1e-12, "im Ursprung");
    assert_eq!(sketch_view::plane_shapes(&s, SCREEN).len(), 1);
    t.toggle(&mut s, "XY-Ebene");
    assert!(s.origin_plane_rects().is_empty());
}

#[test]
fn a_shown_origin_plane_can_be_clicked_where_planes_count() {
    let mut s = AppState::new();
    let center = VIEW / 2.0;
    assert_eq!(s.plane_click_at(center, VIEW, PickFilter::BASES), None);
    s.set_origin_plane_shown(StandardPlane::XY, true);
    assert_eq!(
        s.plane_click_at(center, VIEW, PickFilter::BASES),
        Some(PlaneClick::Standard(StandardPlane::XY))
    );
    // Wo Ebenen nicht zählen (Tangentialfläche), zählt auch die Grundebene nicht.
    assert_eq!(s.plane_click_at(center, VIEW, PickFilter::FACES), None);
    // Eine Konstruktionsebene davor gewinnt (näher an der Kamera).
    let id = offset_plane(&mut s, 10.0);
    assert_eq!(
        s.plane_click_at(center, VIEW, PickFilter::BASES),
        Some(PlaneClick::View(Pick::Plane(id)))
    );
    s.set_feature_hidden(id, true);
    assert_eq!(
        s.plane_click_at(center, VIEW, PickFilter::BASES),
        Some(PlaneClick::Standard(StandardPlane::XY))
    );
}

#[test]
fn a_body_in_front_of_an_origin_plane_wins() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.set_origin_plane_shown(StandardPlane::XY, true);
    let center = VIEW / 2.0;
    // Die Kamera schaut schräg von oben: die Deckfläche (z = 10) liegt vor XY (z = 0).
    assert!(matches!(
        s.plane_click_at(center, VIEW, PickFilter::BASES),
        Some(PlaneClick::View(Pick::Face(_)))
    ));
}

#[test]
fn toggle_visibility_acts_on_the_selected_sketch_or_plane() {
    let mut s = AppState::new();
    let sk = sketch_with_rectangle(&mut s);
    let pl = offset_plane(&mut s, 5.0);
    s.selected_feature = Some(sk);
    assert!(s.toggle_selected_visibility());
    assert!(s.is_feature_hidden(sk));
    s.selected_feature = Some(pl);
    assert!(s.toggle_selected_visibility());
    assert!(s.is_feature_hidden(pl));
    assert!(s.toggle_selected_visibility());
    assert!(!s.is_feature_hidden(pl));
    s.selected_feature = None;
    assert!(!s.toggle_selected_visibility(), "nichts gewählt");
}

#[test]
fn the_open_sketch_has_no_eye() {
    let mut s = AppState::new();
    let id = sketch_with_rectangle(&mut s);
    s.open_sketch(id).unwrap();
    let mut t = Tree::new();
    t.settle(&mut s);
    assert!(
        browser::eye_rects(&t.ctx)
            .iter()
            .all(|(n, _)| !n.starts_with("Skizze 1")),
        "die geöffnete Skizze bleibt sichtbar"
    );
    // Auch über den Zustand nicht: Das Ausblenden einer offenen Skizze wird abgelehnt.
    s.set_feature_hidden(id, true);
    assert!(!s.is_feature_hidden(id));
}

/// Klick auf das Auge eines Ordners.
fn toggle_folder(t: &mut Tree, s: &mut AppState, f: browser::Folder) {
    t.toggle(s, &f.label());
}

#[test]
fn visibility_is_saved_in_the_file_and_old_files_stay_as_they_were() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let sk = sketch_with_rectangle(&mut s);
    let plain = planora_project::encode_version(&s.snapshot()).unwrap();
    assert!(!plain.contains("sichtbarkeit"), "Grundzustand: kein Feld");
    assert!(planora_project::required_format(&s.snapshot()) < 16);

    s.set_body_hidden(0, true);
    s.set_feature_hidden(sk, true);
    s.set_origin_plane_shown(StandardPlane::XZ, true);
    s.toggle_folder_hidden(browser::Folder::Construction);
    assert_eq!(planora_project::required_format(&s.snapshot()), 16);
    let text = planora_project::encode_version(&s.snapshot()).unwrap();
    let back = planora_project::decode_version(&text, "Test").unwrap();
    let l = AppState::from_snapshot(back).unwrap();
    assert!(l.is_body_hidden(0));
    assert!(l.own_feature_hidden(sk));
    assert!(l.is_origin_plane_shown(StandardPlane::XZ));
    assert!(!l.is_origin_plane_shown(StandardPlane::XY));
    assert!(l.is_folder_hidden(browser::Folder::Construction));
    assert_eq!(l.scene.face_indices.len(), 0, "der Körper bleibt aus");

    // Alles wieder an: Die Datei ist wieder die alte, Byte für Byte.
    s.set_body_hidden(0, false);
    s.set_feature_hidden(sk, false);
    s.set_origin_plane_shown(StandardPlane::XZ, false);
    s.toggle_folder_hidden(browser::Folder::Construction);
    assert_eq!(
        planora_project::encode_version(&s.snapshot()).unwrap(),
        plain
    );
}

#[test]
fn an_older_format_with_visibility_is_refused_and_unknown_references_fall_away() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.set_body_hidden(0, true);
    let text = planora_project::encode_version(&s.snapshot()).unwrap();
    let old = text.replacen("\"format\": 16", "\"format\": 15", 1);
    assert_ne!(old, text);
    assert!(planora_project::decode_version(&old, "Test").is_err());
    // Ein Verweis auf ein fehlendes Feature fällt beim Laden weg.
    let mut snap = s.snapshot();
    snap.visibility.features.insert(FeatureId(999));
    let l = AppState::from_snapshot(snap).unwrap();
    assert!(!l.own_feature_hidden(FeatureId(999)));
    assert!(l.snapshot().visibility.features.is_empty());
}

#[test]
fn hiding_is_one_undo_step_and_undo_brings_it_back() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let before = s.snapshot();
    let full = s.scene.face_indices.len();
    let edits = s.document.edit_count();
    let model = s.document.model_edit_count();
    s.set_body_hidden(0, true);
    assert!(s.document.edit_count() > edits, "gilt als Änderung");
    assert_eq!(s.document.model_edit_count(), model, "rechnet nicht neu");
    assert_eq!(s.scene.face_indices.len(), 0);
    assert!(!before.same_content(&s.snapshot()));
    assert!(before.same_model(&s.snapshot()));
    assert_eq!(
        planora_app::undo_labels::describe_change(&before, &s.snapshot()),
        "Ein- oder ausblenden"
    );
    s.restore_document(before).unwrap();
    assert!(!s.is_body_hidden(0));
    assert_eq!(s.scene.face_indices.len(), full, "die Szene folgt");
}

#[test]
fn the_folder_eye_hides_everything_inside_and_keeps_the_row_eyes() {
    use browser::Folder;
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let sk = sketch_with_rectangle(&mut s);
    let a = offset_plane(&mut s, 30.0);
    let b = offset_plane(&mut s, 40.0);
    s.set_feature_hidden(b, true);
    let mut t = Tree::new();

    toggle_folder(&mut t, &mut s, Folder::Construction);
    assert!(s.is_feature_hidden(a));
    assert!(!s.own_feature_hidden(a), "das Auge der Zeile bleibt");
    assert!(s.plane_rects().is_empty());
    toggle_folder(&mut t, &mut s, Folder::Construction);
    assert_eq!(s.plane_rects().len(), 1, "b bleibt selbst aus");
    assert!(s.own_feature_hidden(b));

    toggle_folder(&mut t, &mut s, Folder::Sketches);
    assert!(s.is_feature_hidden(sk));
    assert!(sketch_view::finished_sketch_shapes(&s, SCREEN).is_empty());
    toggle_folder(&mut t, &mut s, Folder::Sketches);
    assert!(!sketch_view::finished_sketch_shapes(&s, SCREEN).is_empty());

    let full = s.scene.face_indices.len();
    s.selected_body = Some(0);
    toggle_folder(&mut t, &mut s, Folder::Bodies);
    assert!(s.is_body_hidden(0));
    assert!(!s.own_body_hidden(0));
    assert_eq!(s.scene.face_indices.len(), 0);
    assert_eq!(s.selected_body, None, "die Wahl entfällt");
    toggle_folder(&mut t, &mut s, Folder::Bodies);
    assert_eq!(s.scene.face_indices.len(), full);

    // Ursprung: das Auge schaltet die drei Ebenen selbst.
    assert!(s.is_folder_hidden(Folder::Origin));
    toggle_folder(&mut t, &mut s, Folder::Origin);
    assert_eq!(s.origin_plane_rects().len(), 3);
    s.set_origin_plane_shown(StandardPlane::XY, false);
    assert!(!s.is_folder_hidden(Folder::Origin), "zwei sind noch an");
    toggle_folder(&mut t, &mut s, Folder::Origin);
    assert!(s.origin_plane_rects().is_empty());
}

#[test]
fn a_click_on_the_folder_eye_does_not_fold_and_drawings_have_no_eye() {
    use browser::Folder;
    let mut s = AppState::new();
    let mut t = Tree::new();
    let open = s.tree_folders.is_open(Folder::Sketches);
    toggle_folder(&mut t, &mut s, Folder::Sketches);
    assert_eq!(s.tree_folders.is_open(Folder::Sketches), open);
    assert!(t.eye(&Folder::Drawings.label()).is_none());
}

#[test]
fn an_old_version_changes_no_visibility() {
    let mut s = AppState::new();
    s.tree_folders.set_open(browser::Folder::Origin, true);
    let mut t = Tree::new();
    let run = |t: &mut Tree, s: &mut AppState, events| {
        t.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(t.time),
            events,
            ..Default::default()
        };
        let out = t.ctx.run_ui(raw, |ui| browser::show_with(ui, s, true));
        out.drop_without_applying_deltas();
    };
    run(&mut t, &mut s, vec![]);
    run(&mut t, &mut s, vec![]);
    for name in ["XY-Ebene".to_string(), browser::Folder::Origin.label()] {
        let p = t.eye(&name).unwrap().center();
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for e in [
            vec![Event::PointerMoved(p)],
            vec![button(true)],
            vec![button(false)],
            vec![],
        ] {
            run(&mut t, &mut s, e);
        }
    }
    assert!(s.document.visibility().is_default());
}

#[test]
fn a_shown_origin_plane_can_be_selected_in_the_normal_view() {
    let mut s = AppState::new();
    let center = VIEW / 2.0;
    assert_eq!(s.view_click_at(center, VIEW), None);
    s.set_origin_plane_shown(StandardPlane::XY, true);
    assert_eq!(
        s.view_click_at(center, VIEW),
        Some(PlaneClick::Standard(StandardPlane::XY))
    );
    assert_eq!(s.pick(center, VIEW), None, "Pick kennt nur das Modell");
    s.edge_picking = true;
    assert_eq!(s.view_click_at(center, VIEW), None, "beim Markieren nicht");
    s.edge_picking = false;

    s.select_origin_plane(StandardPlane::XY);
    assert_eq!(s.selected_origin_plane(), Some(StandardPlane::XY));
    assert_eq!(s.sketch_on_face_blocked(), None, "Skizze darauf geht");
    // V schaltet die gewählte Ebene um.
    assert!(s.toggle_selected_visibility());
    assert!(!s.is_origin_plane_shown(StandardPlane::XY));
    assert!(s.toggle_selected_visibility());
    assert!(s.is_origin_plane_shown(StandardPlane::XY));
    // Eine andere Wahl löst sie ab, auch ein Klick ins Leere.
    s.set_selection(None);
    assert_eq!(s.selected_origin_plane(), None);
    s.select_origin_plane(StandardPlane::XY);
    let id = offset_plane(&mut s, 5.0);
    s.selected_feature = Some(id);
    assert_eq!(s.selected_origin_plane(), None);
}

#[test]
fn a_click_on_an_origin_row_selects_it() {
    let mut s = AppState::new();
    s.tree_folders.set_open(browser::Folder::Origin, true);
    let mut t = Tree::new();
    t.settle(&mut s);
    let row = browser::row_rects(&t.ctx)
        .into_iter()
        .find(|(n, _)| n == "XZ-Ebene")
        .unwrap()
        .1;
    t.click(&mut s, row.center());
    assert_eq!(s.selected_origin_plane(), Some(StandardPlane::XZ));
    assert!(!s.is_origin_plane_shown(StandardPlane::XZ), "wählt nur");
}

/// Die Augen belegen keinen Platz im Layout: Unter dem Ordnerkopf und zwischen den Zeilen ist
/// der Abstand gleich (Kurzprüfung: je Ordner waren es 4 Pixel zu viel).
#[test]
fn the_eyes_take_no_room_in_the_layout() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    s.tree_folders.set_open(browser::Folder::Origin, true);
    for read_only in [false, true] {
        let mut t = Tree::new();
        for _ in 0..2 {
            t.time += 0.05;
            let raw = egui::RawInput {
                screen_rect: Some(SCREEN),
                time: Some(t.time),
                ..Default::default()
            };
            let out = t
                .ctx
                .run_ui(raw, |ui| browser::show_with(ui, &mut s, read_only));
            out.drop_without_applying_deltas();
        }
        let row = |name: &str| {
            browser::row_rects(&t.ctx)
                .into_iter()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("Zeile {name}"))
                .1
        };
        let folder = |f: browser::Folder| {
            browser::folder_rects(&t.ctx)
                .into_iter()
                .find(|(g, _)| *g == f)
                .unwrap()
                .1
        };
        let rows = row("Zylinder 1").top() - row("Quader 1").top();
        let bodies = row("Quader 1").top() - folder(browser::Folder::Bodies).top();
        assert_eq!(bodies, rows, "unter dem Ordnerkopf");
        let origin = row("XZ-Ebene").top() - row("XY-Ebene").top();
        assert_eq!(origin, rows, "zwischen den Grundebenen");
    }
}
