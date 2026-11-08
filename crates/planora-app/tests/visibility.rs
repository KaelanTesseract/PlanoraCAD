//! Handtest Skizzen 6 (2026-11-08): Jedes Objekt der 3D-Ansicht lässt sich im Browser aus- und
//! einblenden wie in Fusion: Körper (schon vorher), Skizzen, Konstruktionsebenen und die
//! Grundebenen im Ordner „Ursprung“ (anfangs ausgeblendet wie in Fusion). Nur die Ansicht
//! ändert sich; ausgeblendete Objekte werden nicht gezeichnet und nicht angeklickt. Ohne
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
