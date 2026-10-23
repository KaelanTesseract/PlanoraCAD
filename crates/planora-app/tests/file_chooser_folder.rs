//! Dateiauswahl im Ordner-Modus (Projektverzeichnis wählen): nur Ordner, Klick markiert,
//! Doppelklick geht hinein, Ergebnis ist der markierte oder der aktuelle Ordner. Alles nur in
//! eigenen Temp-Ordnern; die Schnellzugriffe ohne Laufwerkssuche.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::file_chooser::*;
use planora_app::places::{Place, PlaceKind, Places};
use std::fs;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-ordnerwahl-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    // Wie die Auswahl selbst (absolute Pfade), damit Vergleiche passen.
    std::path::absolute(&d).unwrap()
}

/// Temp-Ordner mit `a/`, `b/`, `teil.stl`, `x.step`.
fn populated(name: &str) -> PathBuf {
    let d = temp_dir(name);
    fs::create_dir_all(d.join("a")).unwrap();
    fs::create_dir_all(d.join("b")).unwrap();
    fs::write(d.join("teil.stl"), b"x").unwrap();
    fs::write(d.join("x.step"), b"x").unwrap();
    d
}

fn shared() -> Places {
    Places {
        drives: Some(vec![]),
        ..Places::default()
    }
}

fn names(c: &FileChooser) -> Vec<&str> {
    c.entries().iter().map(|e| e.name.as_str()).collect()
}

/// Ein gezeichneter Text mit Rechteck.
struct Seen {
    text: String,
    rect: Rect,
}

fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        egui::epaint::Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: Rect::from_min_size(t.pos, t.galley.size()),
        }),
        _ => {}
    }
}

/// Fenster ohne Bildschirm: zeichnet die ganze Auswahl über die volle Breite.
struct Harness {
    ctx: egui::Context,
    time: f64,
    places: Places,
    /// Letzte Aktion ungleich `None`.
    action: ChooserAction,
}

impl Harness {
    fn new() -> Harness {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        Harness {
            ctx,
            time: 0.0,
            places: shared(),
            action: ChooserAction::None,
        }
    }

    fn frame(&mut self, c: &mut FileChooser, events: Vec<Event>) -> Vec<Seen> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 700.0))),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let places = &mut self.places;
        let mut action = ChooserAction::None;
        let out = self.ctx.run_ui(raw, |ui| action = c.show_with(ui, places));
        if action != ChooserAction::None {
            self.action = action;
        }
        let mut seen = Vec::new();
        for s in &out.shapes {
            walk(&s.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self, c: &mut FileChooser) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..3 {
            seen = self.frame(c, vec![]);
        }
        seen
    }

    fn at(&mut self, c: &mut FileChooser, text: &str) -> Pos2 {
        let seen = self.settle(c);
        seen.iter()
            .find(|s| s.text == text)
            .unwrap_or_else(|| panic!("„{text}“ nicht gezeichnet"))
            .rect
            .center()
    }

    /// `n` Klicks hintereinander (2 = Doppelklick); davor eine Pause, damit frühere Klicks
    /// nicht mitzählen.
    fn clicks(&mut self, c: &mut FileChooser, text: &str, n: usize) {
        self.time += 1.0;
        let at = self.at(c, text);
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(c, vec![Event::PointerMoved(at)]);
        for _ in 0..n {
            self.frame(c, vec![button(true)]);
            self.frame(c, vec![button(false)]);
        }
        self.settle(c);
    }
}

fn texts(seen: &[Seen]) -> Vec<&str> {
    seen.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn folder_mode_lists_only_folders() {
    let dir = populated("nur-ordner");
    let c = FileChooser::folder(dir.clone());
    assert_eq!(c.mode, ChooserMode::Folder);
    assert_eq!(names(&c), ["a", "b"]);
    assert!(c.entries().iter().all(|e| e.is_dir));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn without_mark_the_current_folder_is_chosen() {
    let dir = populated("ohne-marke");
    let mut c = FileChooser::folder(dir.clone());
    assert_eq!(c.marked(), None);
    assert_eq!(c.target(), Some(dir.clone()));
    assert_eq!(c.confirm(), ChooserAction::Chosen(dir.clone()));
    assert!(!c.asks_to_replace(), "ein Ordner wird nie ersetzt");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_marked_folder_is_chosen_and_entering_clears_the_mark() {
    let dir = populated("marke");
    let mut c = FileChooser::folder(dir.clone());
    c.mark("b");
    assert_eq!(c.marked(), Some("b"));
    assert_eq!(c.target(), Some(dir.join("b")));
    assert_eq!(c.confirm(), ChooserAction::Chosen(dir.join("b")));
    c.enter("a");
    assert_eq!(c.marked(), None);
    assert_eq!(c.target(), Some(dir.join("a")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn up_and_places_clear_the_mark() {
    let dir = populated("hoch");
    let mut c = FileChooser::folder(dir.join("a"));
    fs::create_dir_all(dir.join("a").join("innen")).unwrap();
    c.reload();
    c.mark("innen");
    assert_eq!(c.marked(), Some("innen"));
    c.up();
    assert_eq!(c.dir(), dir.as_path());
    assert_eq!(c.marked(), None, "„Ordner höher“ löscht die Marke");
    assert_eq!(c.target(), Some(dir.clone()));
    c.mark("b");
    let place = Place {
        kind: PlaceKind::Favorite,
        label: "a".into(),
        path: dir.join("a"),
    };
    assert!(c.go_to_place(&place));
    assert_eq!(c.marked(), None, "Schnellzugriff löscht die Marke");
    assert_eq!(c.target(), Some(dir.join("a")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_vanished_folder_cannot_be_chosen() {
    let dir = populated("weg");
    let mut c = FileChooser::folder(dir.join("a"));
    fs::remove_dir_all(dir.join("a")).unwrap();
    assert_eq!(c.target(), None);
    assert_eq!(c.confirm(), ChooserAction::None);
    assert_eq!(c.error.as_deref(), Some(NO_FOLDER));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn click_marks_and_double_click_enters_in_the_list() {
    let dir = populated("klick");
    let mut c = FileChooser::folder(dir.clone());
    let mut h = Harness::new();
    h.clicks(&mut c, "b", 1);
    assert_eq!(c.marked(), Some("b"), "Klick markiert");
    assert_eq!(c.dir(), dir.as_path(), "Klick geht nicht hinein");
    h.clicks(&mut c, "a", 2);
    assert!(
        c.dir().ends_with("a"),
        "Doppelklick geht hinein: {:?}",
        c.dir()
    );
    assert_eq!(c.marked(), None);
    assert_eq!(h.action, ChooserAction::None, "Doppelklick wählt nicht");
    let _ = fs::remove_dir_all(&dir);
}

/// Flächen in der Farbe der Auswahl, die den Text `text` enthalten.
fn highlighted(h: &mut Harness, c: &mut FileChooser, text: &str) -> bool {
    let fill = h.ctx.global_style().visuals.selection.bg_fill;
    let seen = h.settle(c);
    let label = seen.iter().find(|s| s.text == text).expect("Text").rect;
    h.time += 0.05;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 700.0))),
        time: Some(h.time),
        ..Default::default()
    };
    let places = &mut h.places;
    let out = h.ctx.run_ui(raw, |ui| {
        c.show_with(ui, places);
    });
    fn rects(shape: &egui::epaint::Shape, fill: egui::Color32, out: &mut Vec<Rect>) {
        match shape {
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| rects(s, fill, out)),
            egui::epaint::Shape::Rect(r) if r.fill == fill => out.push(r.rect),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for s in &out.shapes {
        rects(&s.shape, fill, &mut found);
    }
    out.drop_without_applying_deltas();
    found.iter().any(|r| r.contains_rect(label))
}

#[test]
fn the_marked_folder_is_highlighted_like_a_chosen_file() {
    let dir = populated("hervor");
    let mut c = FileChooser::folder(dir.clone());
    let mut h = Harness::new();
    assert!(!highlighted(&mut h, &mut c, "b"));
    c.mark("b");
    assert!(
        highlighted(&mut h, &mut c, "b"),
        "markierter Ordner hervorgehoben"
    );
    assert!(!highlighted(&mut h, &mut c, "a"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn footer_says_choose_this_folder_and_cancel_closes() {
    let dir = populated("fuss");
    let mut c = FileChooser::folder(dir.clone());
    let mut h = Harness::new();
    let seen = h.settle(&mut c);
    let t = texts(&seen);
    assert!(t.contains(&CHOOSE_FOLDER), "{t:?}");
    assert!(t.contains(&"Abbrechen"), "{t:?}");
    for absent in ["Dateiname", "Öffnen", "Speichern", "teil.stl", "x.step"] {
        assert!(!t.contains(&absent), "kein „{absent}“: {t:?}");
    }
    h.clicks(&mut c, CHOOSE_FOLDER, 1);
    assert_eq!(h.action, ChooserAction::Chosen(dir.clone()));
    h.action = ChooserAction::None;
    h.clicks(&mut c, "Abbrechen", 1);
    assert_eq!(h.action, ChooserAction::Cancel);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_chosen_line_shows_the_target() {
    let dir = populated("zeile");
    let mut c = FileChooser::folder(dir.clone());
    let mut h = Harness::new();
    let line = |seen: &[Seen]| {
        seen.iter()
            .find(|s| s.text.starts_with(CHOSEN_PREFIX))
            .map(|s| s.text.clone())
            .expect("Zeile „Gewählt:“")
    };
    let before = line(&h.settle(&mut c));
    assert!(before.contains(&display_path(&dir)), "{before}");
    c.mark("b");
    let after = line(&h.settle(&mut c));
    assert!(after.contains(&display_path(&dir.join("b"))), "{after}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_folder_shows_no_subfolders_hint() {
    let dir = temp_dir("leer");
    fs::write(dir.join("teil.stl"), b"x").unwrap();
    let mut c = FileChooser::folder(dir.clone());
    assert!(c.entries().is_empty());
    let mut h = Harness::new();
    let seen = h.settle(&mut c);
    let t = texts(&seen);
    assert!(t.contains(&NO_SUBFOLDERS), "{t:?}");
    assert!(
        !t.contains(&"Hier gibt es keine passenden Dateien."),
        "{t:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn save_and_open_modes_are_unchanged() {
    let dir = populated("unveraendert");
    let mut c = FileChooser::open(dir.clone(), vec!["stl"]);
    assert_eq!(names(&c), ["a", "b", "teil.stl"]);
    c.mark("b");
    assert_eq!(c.marked(), None, "nur im Ordner-Modus");
    let mut h = Harness::new();
    let t = h.settle(&mut c);
    let t = texts(&t);
    assert!(t.contains(&"Öffnen") && t.contains(&"Dateiname"), "{t:?}");
    assert!(!t.contains(&CHOOSE_FOLDER), "{t:?}");
    assert!(!t.iter().any(|s| s.starts_with(CHOSEN_PREFIX)), "{t:?}");
    h.clicks(&mut c, "b", 1);
    assert_eq!(
        c.marked(),
        None,
        "Klick auf Ordner markiert beim Öffnen nichts"
    );
    assert_eq!(c.dir(), dir.as_path());
    let mut s = FileChooser::save(dir.clone(), "teil".into(), "stl");
    assert_eq!(names(&s), ["a", "b", "teil.stl"]);
    assert_eq!(
        s.confirm(),
        ChooserAction::None,
        "Ersetzen erst nach Rückfrage"
    );
    assert!(s.asks_to_replace());
    let t = h.settle(&mut s);
    assert!(texts(&t).contains(&"Ersetzen"));
    let _ = fs::remove_dir_all(&dir);
}
