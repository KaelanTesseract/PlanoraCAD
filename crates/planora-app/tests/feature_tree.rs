//! Feature-Baum als Körper-Übersicht (Verlaufsleiste, Aufgabe 6): Dokument, Ordner „Körper“,
//! „Skizzen“, „Zeichnungen“, „Ursprung“; je Körper ein Auge und ein Quadrat in der Körperfarbe;
//! keine Schrittzeilen und keine Rückroll-Linie mehr. Ohne Fenster: Rohereignisse je Bild gegen
//! einen egui-Kontext.

use egui::epaint::Shape;
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use planora_app::browser::{self, Folder};
use planora_app::state::AppState;
use planora_app::{fonts, history_bar, theme};
use planora_drawing::{Paper, Projection, Sheet};
use planora_feature::{FeatureId, SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(400.0, 800.0),
};

/// Ein egui-Kontext mit Schriften und Thema.
struct Tree {
    ctx: egui::Context,
    time: f64,
    dark: bool,
}

/// Was ein Bild gezeichnet hat.
struct Shot {
    texts: Vec<(String, Rect, Color32)>,
    fills: Vec<(Rect, Color32)>,
}

impl Shot {
    fn all_text(&self) -> String {
        self.texts
            .iter()
            .map(|t| t.0.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|t| t.0 == text)
    }

    fn text(&self, text: &str) -> &(String, Rect, Color32) {
        self.texts
            .iter()
            .find(|t| t.0 == text)
            .unwrap_or_else(|| panic!("„{text}“ fehlt:\n{}", self.all_text()))
    }
}

fn walk(shape: &Shape, shot: &mut Shot) {
    match shape {
        Shape::Text(t) => shot.texts.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
            t.galley
                .job
                .sections
                .first()
                .map_or(t.fallback_color, |s| s.format.color),
        )),
        Shape::Rect(r) => shot.fills.push((r.rect, r.fill)),
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, shot)),
        _ => {}
    }
}

impl Tree {
    fn new(dark: bool) -> Tree {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, dark);
        Tree {
            ctx,
            time: 0.0,
            dark,
        }
    }

    fn frame(&mut self, s: &mut AppState, events: Vec<Event>) -> Shot {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| browser::show(ui, s));
        let mut shot = Shot {
            texts: Vec::new(),
            fills: Vec::new(),
        };
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut shot));
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self, s: &mut AppState) -> Shot {
        self.frame(s, vec![]);
        self.frame(s, vec![])
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

    /// Rechteck der Zeile `name` (Körper, Skizzen, Zeichnungen, Ebenen).
    fn row(&self, name: &str) -> Option<Rect> {
        browser::row_rects(&self.ctx)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| r)
    }

    fn names(&self) -> Vec<String> {
        browser::row_rects(&self.ctx)
            .into_iter()
            .map(|(n, _)| n)
            .collect()
    }

    fn folder(&self, f: Folder) -> Rect {
        browser::folder_rects(&self.ctx)
            .into_iter()
            .find(|(g, _)| *g == f)
            .unwrap_or_else(|| panic!("Ordner {f:?} fehlt"))
            .1
    }

    fn eye(&self, name: &str) -> Rect {
        browser::eye_rects(&self.ctx)
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("Auge bei „{name}“ fehlt"))
            .1
    }
}

fn two_bodies() -> (AppState, [FeatureId; 2]) {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    let b = s.create_cylinder(1.0, 4.0).unwrap();
    (s, [a, b])
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

#[test]
fn tree_lists_bodies_under_the_bodies_folder_with_colors() {
    for dark in [true, false] {
        let (mut s, [a, b]) = two_bodies();
        let mut t = Tree::new(dark);
        let shot = t.settle(&mut s);
        let folder = t.folder(Folder::Bodies);
        assert!(shot.has("Körper"), "{}", shot.all_text());
        let colors: Vec<Color32> = [a, b]
            .iter()
            .map(|tip| history_bar::body_color(&s, *tip).get(t.dark))
            .collect();
        assert_ne!(colors[0], colors[1], "zwei Körper, zwei Farben");
        for (name, color) in ["Quader 1", "Zylinder 1"].into_iter().zip(colors) {
            let row = t.row(name).unwrap_or_else(|| panic!("Zeile {name}"));
            assert!(row.top() >= folder.bottom() - 0.5, "{name} unter „Körper“");
            assert!(
                shot.fills
                    .iter()
                    .any(|(r, c)| *c == color && row.contains_rect(*r) && r.width() <= 14.0),
                "{name}: kein Quadrat in {color:?} (dunkel: {dark}): {:?}",
                shot.fills
            );
        }
    }
}

#[test]
fn clicking_a_body_selects_it_for_the_bar() {
    let (mut s, [a, b]) = two_bodies();
    assert_eq!(s.history_body(), Some(b), "Standard: der letzte Körper");
    let mut t = Tree::new(true);
    t.settle(&mut s);
    let row = t.row("Quader 1").unwrap();
    t.click(&mut s, row.center());
    assert_eq!(s.history_body(), Some(a));
    // Die bisherige Auswahl für Eigenschaften und Ansicht bleibt erhalten.
    assert_eq!(s.selected_body, Some(0));
    assert_eq!(s.selected_feature, None);
}

#[test]
fn eye_toggles_visibility_of_the_body() {
    let (mut s, _) = two_bodies();
    let full = s.scene.face_indices.len();
    let mut t = Tree::new(true);
    t.settle(&mut s);
    let eye = t.eye("Quader 1");
    t.click(&mut s, eye.center());
    assert!(s.is_body_hidden(0), "Quader ausgeblendet");
    assert!(!s.is_body_hidden(1));
    assert_eq!(s.bodies.len(), 2, "der Körper bleibt im Modell");
    assert!(
        s.scene.face_indices.len() < full,
        "die Ansicht zeigt ihn nicht"
    );
    // Das Auge wählt nichts aus.
    assert_eq!(s.selected_body, None);
    t.settle(&mut s);
    let eye = t.eye("Quader 1");
    t.click(&mut s, eye.center());
    assert!(!s.is_body_hidden(0));
    assert_eq!(s.scene.face_indices.len(), full);
}

#[test]
fn hidden_bodies_cannot_be_picked() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let size = glam::Vec2::new(800.0, 600.0);
    let center = size / 2.0;
    assert!(
        s.pick(center, size).is_some(),
        "der Quader liegt in der Mitte"
    );
    s.set_body_hidden(0, true);
    assert_eq!(s.pick(center, size), None);
    s.set_body_hidden(0, false);
    assert!(s.pick(center, size).is_some());
}

#[test]
fn steps_are_not_listed_in_the_tree() {
    let mut s = AppState::new();
    let sk = sketch_with_rectangle(&mut s);
    let e = s.extrude_sketch(sk, 0, 2.0).unwrap();
    let j = s.bodies.iter().position(|b| b.source.feature == e).unwrap();
    s.move_body(j, DVec3::X).unwrap();
    let mut t = Tree::new(true);
    let shot = t.settle(&mut s);
    let names = t.names();
    let body = s.document.feature(e).unwrap().name.clone();
    assert!(names.contains(&body), "{names:?}");
    assert!(names.contains(&"Skizze 1".to_string()), "{names:?}");
    assert!(!shot.has("Verschieben 1"), "{}", shot.all_text());
    assert_eq!(
        names.iter().filter(|n| **n == body).count(),
        1,
        "ein Körper, eine Zeile: {names:?}"
    );
    // Auch mit zurückgestelltem globalem Marker keine Rückroll-Linie.
    s.document.set_marker(2).unwrap();
    s.refresh();
    let shot = t.settle(&mut s);
    assert!(!shot.has("Berechnung endet hier"), "{}", shot.all_text());
}

#[test]
fn a_union_shows_one_body_and_its_inputs_no_longer_separately() {
    let (mut s, _) = two_bodies();
    s.add_boolean(0, 1, planora_kernel::BoolOp::Union).unwrap();
    let mut t = Tree::new(true);
    t.settle(&mut s);
    let names = t.names();
    let union_name = s.bodies[0].name.clone();
    assert_eq!(
        names.iter().filter(|n| **n == union_name).count(),
        1,
        "{names:?}"
    );
    assert!(!names.contains(&"Quader 1".to_string()), "{names:?}");
    assert!(!names.contains(&"Zylinder 1".to_string()), "{names:?}");
}

#[test]
fn sketches_and_drawings_have_their_own_folders() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    sketch_with_rectangle(&mut s);
    s.document
        .add_drawing(
            "Blatt 1",
            Sheet {
                paper: Paper::A3,
                landscape: true,
            },
            Projection::First,
        )
        .unwrap();
    let mut t = Tree::new(true);
    let shot = t.settle(&mut s);
    let y = |r: Rect| r.center().y;
    let bodies = t.folder(Folder::Bodies);
    let sketches = t.folder(Folder::Sketches);
    let drawings = t.folder(Folder::Drawings);
    let origin = t.folder(Folder::Origin);
    let quader = t.row("Quader 1").unwrap();
    let skizze = t.row("Skizze 1").unwrap();
    let blatt = t.row("Blatt 1").unwrap();
    assert!(y(bodies) < y(quader) && y(quader) < y(sketches));
    assert!(y(sketches) < y(skizze) && y(skizze) < y(drawings));
    assert!(y(drawings) < y(blatt) && y(blatt) < y(origin));
    // Das Dokument steht ganz oben, die Ordner sind darunter eingerückt.
    let doc = shot.text(&s.document_name.clone()).1;
    assert!(doc.center().y < y(bodies));
    for name in ["Körper", "Skizzen", "Zeichnungen", "Ursprung"] {
        assert!(shot.has(name), "„{name}“ fehlt:\n{}", shot.all_text());
    }
    // Skizzen behalten ihre Befehle (Doppelklick öffnet sie).
    assert!(t.row("Skizze 1").is_some());
}

#[test]
fn folders_collapse_and_expand_and_remember_state_per_tab() {
    let mut one = AppState::new();
    one.create_box(DVec3::splat(4.0)).unwrap();
    let mut two = AppState::new();
    two.create_box(DVec3::splat(4.0)).unwrap();
    let mut t = Tree::new(true);
    // Voreinstellung: alles offen außer „Ursprung“.
    t.settle(&mut one);
    assert!(t.row("Quader 1").is_some());
    assert!(t.row("XY-Ebene").is_none(), "Ursprung ist zugeklappt");
    for f in [Folder::Bodies, Folder::Sketches, Folder::Drawings] {
        assert!(one.tree_folders.is_open(f), "{f:?}");
    }
    assert!(!one.tree_folders.is_open(Folder::Origin));
    // „Körper“ im ersten Tab zuklappen.
    let header = t.folder(Folder::Bodies);
    t.click(&mut one, header.center());
    assert!(!one.tree_folders.is_open(Folder::Bodies));
    t.settle(&mut one);
    assert!(t.row("Quader 1").is_none(), "zugeklappt");
    // Das zweite Tab hat seinen eigenen Zustand.
    t.settle(&mut two);
    assert!(t.row("Quader 1").is_some(), "im anderen Tab offen");
    // Zurück im ersten Tab bleibt es zu; „Ursprung“ aufklappen zeigt die Ebenen.
    t.settle(&mut one);
    assert!(t.row("Quader 1").is_none());
    let origin = t.folder(Folder::Origin);
    t.click(&mut one, origin.center());
    t.settle(&mut one);
    for plane in ["XY-Ebene", "XZ-Ebene", "YZ-Ebene"] {
        assert!(t.row(plane).is_some(), "{plane}");
    }
    let header = t.folder(Folder::Bodies);
    t.click(&mut one, header.center());
    t.settle(&mut one);
    assert!(t.row("Quader 1").is_some(), "wieder aufgeklappt");
}

#[test]
fn empty_document_shows_invitation_text() {
    let mut s = AppState::new();
    let mut t = Tree::new(true);
    let shot = t.settle(&mut s);
    assert!(shot.has(browser::EMPTY_TEXT), "{}", shot.all_text());
    assert!(browser::EMPTY_TEXT.contains("„Erzeugen“"));
    // Mit einem Körper verschwindet die Einladung.
    s.create_box(DVec3::splat(4.0)).unwrap();
    let shot = t.settle(&mut s);
    assert!(!shot.has(browser::EMPTY_TEXT));
}

#[test]
fn rolled_back_body_is_dimmed_but_listed() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    history_bar::to_start(&mut s, m).unwrap();
    assert!(history_bar::is_locked(&s, m));
    assert!(s.bodies.is_empty(), "ganz zurückgesetzt: nichts sichtbar");
    // Ein späterer, nicht zurückgesetzter Körper: Er soll nach dem Abwählen die Wahl der Leiste
    // sein, damit der Klick unten wirklich etwas ändern muss.
    let later = s.create_box(DVec3::splat(2.0)).unwrap();
    let mut t = Tree::new(true);
    let shot = t.settle(&mut s);
    let row = t
        .row("Quader 1")
        .expect("zurückgesetzter Körper bleibt gelistet");
    let (_, rect, color) = shot.text("Quader 1");
    assert!(row.contains_rect(*rect));
    // Abgeblendet: schwache Schrift statt der Farbe einer Zeile in Ruhe.
    let weak = t.ctx.style_of(egui::Theme::Dark).visuals.weak_text_color();
    assert_eq!(*color, weak);
    assert_ne!(*color, planora_app::icons::stroke_color(true, false));
    assert!(shot.has(browser::ROLLED_BACK), "{}", shot.all_text());
    // Ein Klick wählt ihn für die Leiste (dort lässt er sich wieder vorspulen).
    s.select_body(Some(later));
    assert_eq!(s.history_body(), Some(later));
    t.click(&mut s, row.center());
    assert_eq!(s.history_body(), Some(m));
    let _ = b;
}

#[test]
fn the_tree_frame_is_the_panel_frame() {
    // Bei 0 % Durchsichtigkeit genau das deckende Panel (mehr in tests/transparency.rs).
    for dark in [true, false] {
        assert_eq!(browser::tree_frame(dark, 0), theme::panel_frame(dark));
    }
}

#[test]
fn the_body_row_menu_acts_on_the_body() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let mut t = Tree::new(true);
    t.settle(&mut s);
    let row = t.row("Quader 1").unwrap();
    // Rechtsklick, dann „Unterdrücken“: unterdrückt wird der Ursprung des Körpers.
    let p = row.center();
    let press = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    t.frame(
        &mut s,
        vec![Event::PointerMoved(p), press(true), press(false)],
    );
    let shot = t.frame(&mut s, vec![]);
    let item = shot.text("Unterdrücken").1.center();
    t.click(&mut s, item);
    assert!(s.document.feature(b).unwrap().suppressed);
    let shot = t.settle(&mut s);
    assert!(t.row("Quader 1").is_some(), "unterdrückt, aber gelistet");
    assert!(shot.has("unterdrückt"), "{}", shot.all_text());
}

/// Ein Bild nur mit der Verlaufsleiste (1400 × 900); liefert die gezeichneten Texte.
fn bar_frame(ctx: &egui::Context, s: &mut AppState, time: &mut f64, events: Vec<Event>) -> Shot {
    *time += 0.05;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
        time: Some(*time),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| history_bar::show(ui.ctx(), &mut *s));
    let mut shot = Shot {
        texts: Vec::new(),
        fills: Vec::new(),
    };
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut shot));
    out.drop_without_applying_deltas();
    shot
}

/// Rechtsklick auf den Schritt `id` der Leiste, dann Klick auf `entry`.
fn bar_menu(ctx: &egui::Context, s: &mut AppState, time: &mut f64, id: FeatureId, entry: &str) {
    for _ in 0..3 {
        bar_frame(ctx, s, time, vec![]);
    }
    let rect = history_bar::bar_rects(ctx)
        .steps
        .into_iter()
        .find(|(f, _)| *f == id)
        .expect("Schritt in der Leiste")
        .1;
    let p = rect.center();
    let press = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    bar_frame(
        ctx,
        s,
        time,
        vec![Event::PointerMoved(p), press(true), press(false)],
    );
    let shot = bar_frame(ctx, s, time, vec![]);
    let item = shot.text(entry).1.center();
    let click = |pressed| Event::PointerButton {
        pos: item,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [
        vec![Event::PointerMoved(item)],
        vec![click(true)],
        vec![click(false)],
        vec![],
    ] {
        bar_frame(ctx, s, time, events);
    }
}

#[test]
fn steps_keep_suppress_rename_and_delete_in_the_bar() {
    // Die Schritte stehen nicht mehr im Baum; ihre Befehle bietet das Kontextmenü der Leiste.
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    let mut time = 0.0;
    bar_menu(&ctx, &mut s, &mut time, m, "Unterdrücken");
    assert!(s.document.feature(m).unwrap().suppressed);
    bar_menu(&ctx, &mut s, &mut time, m, "Aktivieren");
    assert!(!s.document.feature(m).unwrap().suppressed);
    bar_menu(&ctx, &mut s, &mut time, m, "Umbenennen");
    assert_eq!(s.rename_draft.as_ref().map(|r| r.0), Some(m));
    s.rename_draft = None;
    bar_menu(&ctx, &mut s, &mut time, m, "Löschen");
    assert!(
        s.document.feature(m).is_none(),
        "ohne Folgen sofort gelöscht"
    );
    assert!(s.document.feature(b).is_some());
}

// ---- Fenstertest: Befunde ---------------------------------------------------------------------

/// Text `text` mit seinem sichtbaren Ausschnitt (Rechteck des Textes ∩ Schnittrechteck).
fn clipped_text(out: &egui::FullOutput, text: &str) -> Option<(Rect, Rect)> {
    fn walk(shape: &Shape, clip: Rect, text: &str) -> Option<(Rect, Rect)> {
        match shape {
            Shape::Text(t) if t.galley.text() == text => {
                Some((Rect::from_min_size(t.pos, t.galley.size()), clip))
            }
            Shape::Vec(v) => v.iter().find_map(|s| walk(s, clip, text)),
            _ => None,
        }
    }
    out.shapes
        .iter()
        .find_map(|c| walk(&c.shape, c.clip_rect, text))
}

/// Baum in einem 250 px breiten Feld (wie die linke Spalte); liefert den Ausschnitt des Namens.
fn name_in_narrow_panel(s: &mut AppState, name: &str) -> Option<(Rect, Rect)> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    let mut found = None;
    for i in 0..3 {
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(f64::from(i) * 0.05),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            ui.allocate_ui(vec2(theme::PANEL_WIDTH, 700.0), |ui| {
                ui.set_width(theme::PANEL_WIDTH);
                browser::show(ui, s);
            });
        });
        found = clipped_text(&out, name);
        out.drop_without_applying_deltas();
    }
    found
}

#[test]
fn body_names_are_not_cut_short_by_a_status_column() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    for name in ["Quader 1", "Zylinder 1"] {
        let (text, clip) = name_in_narrow_panel(&mut s, name).expect(name);
        assert!(
            clip.contains_rect(text),
            "{name} abgeschnitten: Text {text:?}, sichtbar {clip:?}"
        );
    }
}

#[test]
fn rolled_back_body_keeps_its_full_name_visible() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    history_bar::to_start(&mut s, m).unwrap();
    let (text, clip) = name_in_narrow_panel(&mut s, "Quader 1").expect("Name");
    assert!(clip.contains_rect(text), "Text {text:?}, sichtbar {clip:?}");
}

#[test]
fn a_long_name_is_shortened_with_an_ellipsis_and_the_status_moves_to_the_hint() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(4.0)).unwrap();
    let long = "Grundplatte mit sehr langem Namen für die Werkstatt";
    s.rename_feature(b, long).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    history_bar::to_start(&mut s, m).unwrap();
    let mut t = Tree::new(true);
    let shot = t.settle(&mut s);
    // Gezeichnet werden nur die Zeichen bis „…“ (der Text des Auftrags bleibt der volle Name).
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        time: Some(100.0),
        ..Default::default()
    };
    let out = t.ctx.run_ui(raw, |ui| browser::show(ui, &mut s));
    fn shown(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Text(t) => out.push(
                t.galley
                    .rows
                    .iter()
                    .flat_map(|r| r.row.glyphs.iter().map(|g| g.chr))
                    .collect(),
            ),
            Shape::Vec(v) => v.iter().for_each(|s| shown(s, out)),
            _ => {}
        }
    }
    let mut drawn = Vec::new();
    out.shapes.iter().for_each(|c| shown(&c.shape, &mut drawn));
    out.drop_without_applying_deltas();
    assert!(
        drawn
            .iter()
            .any(|x| x.ends_with('…') && long.starts_with(x.trim_end_matches('…'))),
        "gekürzt mit …: {drawn:?}"
    );
    assert!(!drawn.iter().any(|x| x == long), "nicht in voller Länge");
    // Der Status passt nicht mehr daneben: er steht nur im Hinweis.
    assert!(!shot.has(browser::ROLLED_BACK), "{}", shot.all_text());
    let row = t.row(long).expect("Zeile");
    let mut text = String::new();
    for i in 0..6 {
        let events = if i == 0 {
            vec![Event::PointerMoved(row.center())]
        } else {
            vec![]
        };
        t.time += 0.5;
        text = t.frame(&mut s, events).all_text();
    }
    assert!(text.contains(browser::ROLLED_BACK), "Hinweis:\n{text}");
}

#[test]
fn the_empty_hints_sit_inside_their_folders() {
    let mut s = AppState::new();
    let mut t = Tree::new(true);
    t.settle(&mut s);
    let shot = t.settle(&mut s);
    let bodies = t.folder(Folder::Bodies);
    let sketches = t.folder(Folder::Sketches);
    let hint = shot.text(browser::EMPTY_TEXT).1;
    assert!(
        hint.top() >= bodies.bottom() - 0.5 && hint.bottom() <= sketches.top() + 0.5,
        "Hinweis im Ordner „Körper“: {hint:?}, Körper {bodies:?}, Skizzen {sketches:?}"
    );
    assert!(shot.has(browser::NO_SKETCHES), "{}", shot.all_text());
    let sk = shot.text(browser::NO_SKETCHES).1;
    assert!(sk.top() >= sketches.bottom() - 0.5);
    // Zugeklappt: kein Hinweis.
    s.tree_folders.set_open(Folder::Bodies, false);
    s.tree_folders.set_open(Folder::Sketches, false);
    let shot = t.settle(&mut s);
    assert!(!shot.has(browser::EMPTY_TEXT), "{}", shot.all_text());
    assert!(!shot.has(browser::NO_SKETCHES), "{}", shot.all_text());
    // Mit einem Körper, aber ohne Skizze: nur noch der Hinweis bei den Skizzen.
    s.tree_folders = Default::default();
    s.create_box(DVec3::splat(4.0)).unwrap();
    let shot = t.settle(&mut s);
    assert!(!shot.has(browser::EMPTY_TEXT));
    assert!(shot.has(browser::NO_SKETCHES));
}

#[test]
fn body_rows_keep_their_order_after_a_move() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    let mut t = Tree::new(true);
    t.settle(&mut s);
    let order = |t: &Tree| -> Vec<String> {
        t.names()
            .into_iter()
            .filter(|n| n == "Quader 1" || n == "Zylinder 1")
            .collect()
    };
    assert_eq!(order(&t), ["Quader 1", "Zylinder 1"]);
    let j = s.bodies.iter().position(|b| b.name == "Quader 1").unwrap();
    s.move_body(j, DVec3::X).unwrap();
    t.settle(&mut s);
    assert_eq!(
        order(&t),
        ["Quader 1", "Zylinder 1"],
        "Verschieben ordnet nicht um"
    );
}
