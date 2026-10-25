//! Gemeinsame Hilfen der Oberflächentests.

use std::cell::RefCell;
use std::path::PathBuf;

/// Löscht den Bibliotheksordner eines Tests, wenn der Test (genauer: sein Thread) endet.
struct TempRoots(Vec<PathBuf>);

impl Drop for TempRoots {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

thread_local! {
    static ROOTS: RefCell<TempRoots> = const { RefCell::new(TempRoots(Vec::new())) };
}

/// Neuer Ordner `%TEMP%\<prefix>-<id>` für die Bibliothek eines Tests; er wird nach dem Test
/// gelöscht, damit Läufe keine Ordner ansammeln. (Der Ordner selbst wird erst von
/// `Library::open` angelegt.)
///
/// `%TEMP%` wird vorher in die lange Schreibweise gebracht (ohne `\\?\`): Auf manchen Rechnern
/// (etwa den Windows-Runnern von GitHub) steht dort ein 8.3-Kurzname wie `RUNNER~1`. Die Tests
/// vergleichen den übergebenen Pfad mit dem kanonischen der Bibliothek; ein Kurzname ist eine
/// andere Schreibweise, die kein Vergleich ohne Dateisystemzugriff erkennen kann.
#[allow(dead_code)]
pub fn temp_root(prefix: &str) -> PathBuf {
    let root = long_temp_dir().join(format!("{prefix}-{}", planora_project::fsutil::new_id()));
    ROOTS.with(|r| r.borrow_mut().0.push(root.clone()));
    root
}

/// `std::env::temp_dir()` in langer Schreibweise, ohne Präfix `\\?\`.
#[allow(dead_code)]
fn long_temp_dir() -> PathBuf {
    let tmp = std::env::temp_dir();
    match std::fs::canonicalize(&tmp) {
        Ok(c) => PathBuf::from(planora_app::file_chooser::display_path(&c)),
        Err(_) => tmp,
    }
}

/// Ein Text im Bild mit seinem Rechteck und dem Ausschnitt, in dem er gezeichnet wird.
#[allow(dead_code)]
pub struct Seen {
    pub text: String,
    pub rect: egui::Rect,
    pub clip: egui::Rect,
}

fn walk_shape(shape: &egui::epaint::Shape, clip: egui::Rect, out: &mut Vec<Seen>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk_shape(s, clip, out)),
        egui::epaint::Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: egui::Rect::from_min_size(t.pos, t.galley.size()),
            clip,
        }),
        _ => {}
    }
}

/// Fenster für Layouttests: eigener Kontext, Bilder in 0,05-s-Schritten (Klicks und Mausrad
/// brauchen kurze Abstände).
#[allow(dead_code)]
pub struct Screen {
    pub ctx: egui::Context,
    pub size: egui::Vec2,
    /// Gewünschter Abstand bis zum nächsten Bild nach dem letzten Bild (`Duration::MAX` = keins).
    pub repaint_delay: std::time::Duration,
    time: f64,
}

#[allow(dead_code)]
impl Screen {
    pub fn new(w: f32, h: f32) -> Screen {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        Screen {
            ctx,
            size: egui::vec2(w, h),
            repaint_delay: std::time::Duration::MAX,
            time: 0.0,
        }
    }

    /// Ein Bild mit den Ereignissen; liefert alle gezeichneten Texte.
    pub fn frame(
        &mut self,
        events: Vec<egui::Event>,
        f: &mut dyn FnMut(&egui::Context),
    ) -> Vec<Seen> {
        self.frame_shapes(events, f).0
    }

    /// Wie [`Screen::frame`], dazu alle gezeichneten Formen des Bilds.
    pub fn frame_shapes(
        &mut self,
        events: Vec<egui::Event>,
        f: &mut dyn FnMut(&egui::Context),
    ) -> (Vec<Seen>, Vec<egui::epaint::ClippedShape>) {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(raw, |ui| f(ui.ctx()));
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk_shape(&c.shape, c.clip_rect, &mut seen);
        }
        let shapes = std::mem::take(&mut out.shapes);
        self.repaint_delay = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        out.drop_without_applying_deltas();
        (seen, shapes)
    }

    /// `n` Bilder ohne Ereignisse; liefert die Texte des letzten.
    pub fn settle(&mut self, n: usize, f: &mut dyn FnMut(&egui::Context)) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![], f);
        }
        seen
    }

    /// Mausrad um `dy` Punkte (negativ = nach unten blättern) an der Stelle `at`.
    pub fn wheel(&mut self, at: egui::Pos2, dy: f32, f: &mut dyn FnMut(&egui::Context)) {
        self.frame(vec![egui::Event::PointerMoved(at)], f);
        self.frame(
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            }],
            f,
        );
        self.settle(3, f);
    }

    /// Linksklick an der Stelle `at`.
    pub fn click(&mut self, at: egui::Pos2, f: &mut dyn FnMut(&egui::Context)) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![egui::Event::PointerMoved(at)], f);
        self.frame(vec![button(true)], f);
        self.frame(vec![button(false)], f);
        self.settle(2, f);
    }
}

#[allow(dead_code)]
pub fn find<'a>(seen: &'a [Seen], text: &str) -> Option<&'a Seen> {
    seen.iter().find(|s| s.text == text)
}

/// Prüft die Kartenreihe in einem Dialog: gleiche Oberkante, gleiche Höhe, jede Karte im
/// Dialogrechteck, kein Text ragt aus seiner Karte.
#[allow(dead_code)]
pub fn assert_cards_inside(rect: egui::Rect, cards: &[egui::Rect], seen: &[Seen]) {
    assert!(cards.len() >= 2, "{cards:?}");
    for c in cards {
        assert!(
            (c.top() - cards[0].top()).abs() <= 0.5,
            "Oberkante {cards:?}"
        );
        assert!(
            (c.height() - cards[0].height()).abs() <= 0.5,
            "Höhe {cards:?}"
        );
        assert!(
            rect.expand(0.5).contains_rect(*c),
            "Karte {c:?} im Dialog {rect:?}"
        );
    }
    for pair in cards.windows(2) {
        assert!(pair[1].min.x >= pair[0].max.x, "nebeneinander {cards:?}");
    }
    for s in seen {
        if let Some(card) = cards.iter().find(|c| c.contains(s.rect.center())) {
            assert!(
                card.expand(0.5).contains_rect(s.rect),
                "Text „{}“ {:?} ragt aus seiner Karte {card:?}",
                s.text,
                s.rect
            );
        }
    }
}
