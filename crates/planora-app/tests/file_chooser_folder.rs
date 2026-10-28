//! Dateiauswahl im Ordner-Modus (Projektverzeichnis wählen): nur Ordner, Klick markiert,
//! Doppelklick geht hinein, Ergebnis ist der markierte oder der aktuelle Ordner. Alles nur in
//! eigenen Temp-Ordnern; die Schnellzugriffe ohne Laufwerkssuche.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::file_chooser::*;
use planora_app::places::{Place, PlaceKind, Places};
use std::fs;
use std::path::PathBuf;

/// Temp-Ordner eines Tests; wird beim Verlassen des Tests gelöscht, auch wenn er fehlschlägt.
struct TempDir(PathBuf);

impl std::ops::Deref for TempDir {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("planora-ordnerwahl-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    // Wie die Auswahl selbst (absolute Pfade), damit Vergleiche passen.
    TempDir(std::path::absolute(&d).unwrap())
}

/// Temp-Ordner mit `a/`, `b/`, `teil.stl`, `x.step`.
fn populated(name: &str) -> TempDir {
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
}

#[test]
fn without_mark_the_current_folder_is_chosen() {
    let dir = populated("ohne-marke");
    let mut c = FileChooser::folder(dir.clone());
    assert_eq!(c.marked(), None);
    assert_eq!(c.target(), Some(dir.clone()));
    assert_eq!(c.confirm(), ChooserAction::Chosen(dir.clone()));
    assert!(!c.asks_to_replace(), "ein Ordner wird nie ersetzt");
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
}

#[test]
fn a_vanished_folder_cannot_be_chosen() {
    let dir = populated("weg");
    let mut c = FileChooser::folder(dir.join("a"));
    fs::remove_dir_all(dir.join("a")).unwrap();
    assert_eq!(c.target(), None);
    assert_eq!(c.confirm(), ChooserAction::None);
    assert_eq!(c.error_text().as_deref(), Some(no_folder().as_str()));
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
}

#[test]
fn footer_says_choose_this_folder_and_cancel_closes() {
    let dir = populated("fuss");
    let mut c = FileChooser::folder(dir.clone());
    let mut h = Harness::new();
    let seen = h.settle(&mut c);
    let t = texts(&seen);
    assert!(t.iter().any(|s| *s == choose_folder()), "{t:?}");
    assert!(t.contains(&"Abbrechen"), "{t:?}");
    for absent in ["Dateiname", "Öffnen", "Speichern", "teil.stl", "x.step"] {
        assert!(!t.contains(&absent), "kein „{absent}“: {t:?}");
    }
    h.clicks(&mut c, &choose_folder(), 1);
    assert_eq!(h.action, ChooserAction::Chosen(dir.clone()));
    h.action = ChooserAction::None;
    h.clicks(&mut c, "Abbrechen", 1);
    assert_eq!(h.action, ChooserAction::Cancel);
}

#[test]
fn the_chosen_line_shows_the_target() {
    let dir = populated("zeile");
    let mut c = FileChooser::folder(dir.clone());
    let mut h = Harness::new();
    let line = |seen: &[Seen]| {
        seen.iter()
            .find(|s| s.text.starts_with(&chosen_prefix()))
            .map(|s| s.text.clone())
            .expect("Zeile „Gewählt:“")
    };
    let before = line(&h.settle(&mut c));
    assert!(before.contains(&display_path(&dir)), "{before}");
    c.mark("b");
    let after = line(&h.settle(&mut c));
    assert!(after.contains(&display_path(&dir.join("b"))), "{after}");
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
    assert!(t.iter().any(|s| *s == no_subfolders()), "{t:?}");
    assert!(
        !t.contains(&"Hier gibt es keine passenden Dateien."),
        "{t:?}"
    );
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
    assert!(!t.iter().any(|s| *s == choose_folder()), "{t:?}");
    assert!(!t.iter().any(|s| s.starts_with(&chosen_prefix())), "{t:?}");
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
}

// --- Nachbesserung 1 ---------------------------------------------------------------------------

#[test]
fn a_vanished_marked_folder_is_not_replaced_by_the_current_one() {
    let dir = populated("marke-weg");
    let mut c = FileChooser::folder(dir.clone());
    c.mark("b");
    fs::remove_dir_all(dir.join("b")).unwrap();
    assert_eq!(c.target(), None, "kein Rückfall auf den aktuellen Ordner");
    assert_eq!(c.confirm(), ChooserAction::None);
    assert_eq!(c.error_text().as_deref(), Some(no_folder().as_str()));
    assert_eq!(c.marked(), None, "Marke gelöscht");
    assert_eq!(names(&c), ["a"], "Liste neu gelesen");
    // Danach entscheidet der Nutzer neu: ohne Marke der aktuelle Ordner.
    assert_eq!(c.confirm(), ChooserAction::Chosen(dir.clone()));
    // Auch eine Datei gleichen Namens ist kein Ordner.
    c.mark("a");
    fs::remove_dir_all(dir.join("a")).unwrap();
    fs::write(dir.join("a"), b"x").unwrap();
    assert_eq!(c.target(), None);
    assert_eq!(c.confirm(), ChooserAction::None);
    assert_eq!(c.error_text().as_deref(), Some(no_folder().as_str()));
    assert_eq!(c.marked(), None);
}

#[test]
fn only_folders_of_the_list_can_be_marked() {
    let dir = populated("nur-liste");
    let mut c = FileChooser::folder(dir.join("a"));
    fs::create_dir_all(dir.join("a").join("innen")).unwrap();
    c.reload();
    let outside = dir.join("b").display().to_string();
    for bad in [
        "..",
        ".",
        "",
        outside.as_str(),
        "innen/x",
        "innen\\x",
        "INNEN ",
        "fehlt",
    ] {
        c.mark(bad);
        assert_eq!(c.marked(), None, "„{bad}“");
    }
    let mut c = FileChooser::folder(dir.clone());
    c.mark("teil.stl");
    assert_eq!(c.marked(), None, "Datei");
    c.mark("b");
    c.mark("..");
    assert_eq!(c.marked(), Some("b"), "Ungültiges ändert die Marke nicht");
}

#[test]
fn a_typed_folder_is_applied_before_choosing() {
    let dir = populated("getippt");
    let mut c = FileChooser::folder(dir.clone());
    let mut h = Harness::new();
    c.mark("b");
    c.dir_text = dir.join("a").display().to_string();
    h.clicks(&mut c, &choose_folder(), 1);
    assert_eq!(h.action, ChooserAction::Chosen(dir.join("a")));
    assert_eq!(c.dir(), dir.join("a").as_path());
    h.action = ChooserAction::None;
    c.dir_text = dir.join("gibt-es-nicht").display().to_string();
    h.clicks(&mut c, &choose_folder(), 1);
    assert_eq!(h.action, ChooserAction::None, "nichts gewählt");
    assert_eq!(
        c.error_text().as_deref(),
        Some("Diesen Ordner gibt es nicht.")
    );
    let seen = h.settle(&mut c);
    assert!(texts(&seen).contains(&"Diesen Ordner gibt es nicht."));
    assert_eq!(c.dir(), dir.join("a").as_path(), "Ordner bleibt");
}

#[test]
fn long_folders_are_cut_off_and_files_do_not_count_in_folder_mode() {
    let dir = temp_dir("viele");
    for i in 0..MAX_ENTRIES - 1 {
        fs::create_dir(dir.join(format!("o{i:05}"))).unwrap();
    }
    for i in 0..3 {
        fs::write(dir.join(format!("t{i}.stl")), b"x").unwrap();
    }
    // Ordner-Modus: nur die Ordner zählen, nichts gekürzt.
    let c = FileChooser::folder(dir.clone());
    assert_eq!(c.entries().len(), MAX_ENTRIES - 1);
    assert!(!c.truncated);
    // Öffnen: Ordner und Dateien zählen, gekürzt.
    let o = FileChooser::open(dir.clone(), vec!["stl"]);
    assert_eq!(o.entries().len(), MAX_ENTRIES);
    assert!(o.truncated);
    assert_eq!(o.entries().iter().filter(|e| !e.is_dir).count(), 1);
    // Ordner-Modus mit mehr Ordnern als erlaubt: gekürzt, mit Hinweis.
    fs::create_dir(dir.join("o99998")).unwrap();
    fs::create_dir(dir.join("o99999")).unwrap();
    let c = FileChooser::folder(dir.clone());
    assert_eq!(c.entries().len(), MAX_ENTRIES);
    assert!(c.truncated);
    assert!(c.entries().iter().all(|e| e.is_dir));
    assert_eq!(
        c.entries().last().unwrap().name,
        "o99998",
        "die ersten nach Namen"
    );
}

/// Verbindung (Junction) `link` → `target`; `false`, wenn sie sich nicht anlegen lässt.
#[cfg(windows)]
fn junction(link: &std::path::Path, target: &std::path::Path) -> bool {
    std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .is_ok_and(|o| o.status.success())
        && link.is_dir()
}

#[cfg(windows)]
#[test]
fn junctions_are_listed_and_can_be_entered_and_chosen() {
    let dir = populated("verbindung");
    let elsewhere = temp_dir("verbindung-ziel");
    fs::create_dir_all(elsewhere.join("drin")).unwrap();
    if !junction(&dir.join("link"), &elsewhere) {
        eprintln!("Verbindung ließ sich nicht anlegen, Test übersprungen");
        return;
    }
    let mut c = FileChooser::folder(dir.clone());
    assert_eq!(names(&c), ["a", "b", "link"]);
    c.mark("link");
    assert_eq!(c.target(), Some(dir.join("link")));
    c.enter("link");
    assert_eq!(names(&c), ["drin"]);
    assert_eq!(c.target(), Some(dir.join("link")));
    // Die Verbindung zuerst entfernen (nicht durch sie hindurch löschen).
    let _ = fs::remove_dir(dir.join("link"));
}
