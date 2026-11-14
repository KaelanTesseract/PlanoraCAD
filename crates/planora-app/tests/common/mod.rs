//! Gemeinsame Hilfen der Oberflächentests.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

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

/// Alle Texte der Formen eines Bilds (mit Rechteck und Ausschnitt).
#[allow(dead_code)]
pub fn collect_seen(shapes: &[egui::epaint::ClippedShape]) -> Vec<Seen> {
    let mut seen = Vec::new();
    for c in shapes {
        walk_shape(&c.shape, c.clip_rect, &mut seen);
    }
    seen
}

/// Die Texte `seen` mit " | " verbunden (für Fehlermeldungen).
#[allow(dead_code)]
pub fn join(seen: &[Seen]) -> String {
    seen.iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Sieht die Zeile wie eine Schlüssel-Id der Sprachdateien aus (`^[a-z0-9]+(-[a-z0-9]+)+$`)?
#[allow(dead_code)]
pub fn looks_like_key(line: &str) -> bool {
    let parts: Vec<&str> = line.split('-').collect();
    parts.len() > 1
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// Kein deutscher Buchstabe (`äöüßÄÖÜ„`) und keine Schlüssel-Id in den Texten. Texte in einem der
/// Rechtecke `skip` (Bereiche späterer Tasks) und Zeilen, die eines der Stücke `later`
/// enthalten, werden übergangen.
#[allow(dead_code)]
pub fn assert_english(what: &str, seen: &[Seen], skip: &[egui::Rect], later: &[&str]) {
    for s in seen {
        if skip.iter().any(|r| r.contains_rect(s.rect)) {
            continue;
        }
        for line in s.text.lines() {
            if later.iter().any(|l| line.contains(l)) {
                continue;
            }
            assert!(
                !line.chars().any(|c| "äöüßÄÖÜ„".contains(c)),
                "{what}: deutscher Text „{line}“ in {}",
                join(seen)
            );
            assert!(
                !looks_like_key(line),
                "{what}: Schlüssel-Id „{line}“ sichtbar in {}",
                join(seen)
            );
        }
    }
}

/// Jeder Text von `wanted` steht ganz (als ein Text) im Bild.
#[allow(dead_code)]
pub fn assert_has(what: &str, seen: &[Seen], wanted: &[&str]) {
    for w in wanted {
        assert!(
            seen.iter().any(|s| s.text == *w),
            "{what}: „{w}“ fehlt in {}",
            join(seen)
        );
    }
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
    /// Kindfenster (nur bei [`Screen::with_viewports`]), geteilt mit dem Renderer.
    children: Option<Rc<RefCell<Children>>>,
}

/// Zustand der Kindfenster im Harness: was ins nächste Bild eines Kindfensters geht und was es
/// zuletzt gezeichnet bzw. befohlen hat.
#[allow(dead_code)]
#[derive(Default)]
pub struct Children {
    /// Ereignisse je Kindfenster für dessen nächstes Bild (danach geleert).
    pub child_events: HashMap<egui::ViewportId, Vec<egui::Event>>,
    /// Fensterangaben je Kindfenster (Lage, Faktor, `events` wie `Close`); `events` werden nach
    /// dem Bild geleert, der Rest bleibt.
    pub info: HashMap<egui::ViewportId, egui::ViewportInfo>,
    /// Texte des letzten Bilds je Kindfenster.
    pub seen: HashMap<egui::ViewportId, Vec<Seen>>,
    /// Alle Befehle an das Kindfenster seit dem Anlegen des Harness.
    pub commands: HashMap<egui::ViewportId, Vec<egui::ViewportCommand>>,
    /// Builder des letzten Aufrufs je Kindfenster.
    pub builders: HashMap<egui::ViewportId, egui::ViewportBuilder>,
    /// Wie oft der Renderer je Kindfenster lief.
    pub runs: HashMap<egui::ViewportId, usize>,
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
            children: None,
        }
    }

    /// Wie [`Screen::new`], aber mit echten Kindfenstern: `embed_viewports` aus und ein eigener
    /// Renderer für immediate-Viewports, der jedes Kindfenster mit **eigener** `RawInput` (aus
    /// [`Children::child_events`] und [`Children::info`]) ausführt und seine Texte sammelt.
    /// Der Renderer gilt je Thread (jeder Test läuft in einem eigenen).
    pub fn with_viewports(w: f32, h: f32) -> Screen {
        let mut s = Screen::new(w, h);
        s.ctx.set_embed_viewports(false);
        let children = Rc::new(RefCell::new(Children::default()));
        let shared = Rc::clone(&children);
        egui::Context::set_immediate_viewport_renderer(move |ctx, viewport| {
            let egui::ImmediateViewport {
                ids,
                builder,
                mut viewport_ui_cb,
            } = viewport;
            let id = ids.this;
            // Die Leihe endet vor `run_ui`: Kindfenster können selbst Kindfenster öffnen.
            let raw = {
                let mut c = shared.borrow_mut();
                let events = c.child_events.remove(&id).unwrap_or_default();
                let time = c.time;
                let info = c.info.entry(id).or_default();
                info.parent = Some(ids.parent);
                let size = info
                    .inner_rect
                    .map(|r| r.size())
                    .or(builder.inner_size)
                    .unwrap_or(egui::vec2(400.0, 300.0));
                let mut viewports = egui::ViewportIdMap::default();
                viewports.insert(id, info.clone());
                info.events.clear();
                *c.runs.entry(id).or_default() += 1;
                c.builders.insert(id, builder);
                egui::RawInput {
                    viewport_id: id,
                    viewports,
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    time: Some(time),
                    ..Default::default()
                }
            };
            let mut out = ctx.run_ui(raw, |ui| viewport_ui_cb(ui));
            let mut seen = Vec::new();
            for c in &out.shapes {
                walk_shape(&c.shape, c.clip_rect, &mut seen);
            }
            out.shapes.clear();
            out.drop_without_applying_deltas();
            shared.borrow_mut().seen.insert(id, seen);
        });
        s.children = Some(children);
        s
    }

    fn children(&self) -> std::cell::RefMut<'_, Children> {
        self.children
            .as_ref()
            .expect("Screen::with_viewports")
            .borrow_mut()
    }

    /// Legt `events` ins nächste Bild des Kindfensters `id`, führt ein Bild aus und liefert die
    /// Texte des Kindfensters.
    pub fn frame_child(
        &mut self,
        id: egui::ViewportId,
        events: Vec<egui::Event>,
        f: &mut dyn FnMut(&egui::Context),
    ) -> Vec<Seen> {
        self.children()
            .child_events
            .entry(id)
            .or_default()
            .extend(events);
        self.frame(vec![], f);
        self.child_seen(id)
    }

    /// Linksklick im Kindfenster `id` an der Stelle `at` (Punkte des Kindfensters).
    pub fn click_child(
        &mut self,
        id: egui::ViewportId,
        at: egui::Pos2,
        f: &mut dyn FnMut(&egui::Context),
    ) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame_child(id, vec![egui::Event::PointerMoved(at)], f);
        self.frame_child(id, vec![button(true)], f);
        self.frame_child(id, vec![button(false)], f);
    }

    /// Ändert die Fensterangaben des Kindfensters `id` für die nächsten Bilder.
    pub fn child_info(
        &mut self,
        id: egui::ViewportId,
        change: impl FnOnce(&mut egui::ViewportInfo),
    ) {
        change(self.children().info.entry(id).or_default());
    }

    /// Texte des letzten Bilds im Kindfenster `id` (leer, wenn es nie lief).
    pub fn child_seen(&self, id: egui::ViewportId) -> Vec<Seen> {
        self.children()
            .seen
            .get(&id)
            .map(|v| {
                v.iter()
                    .map(|s| Seen {
                        text: s.text.clone(),
                        rect: s.rect,
                        clip: s.clip,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Alle Befehle an das Kindfenster `id` bisher.
    pub fn child_commands(&self, id: egui::ViewportId) -> Vec<egui::ViewportCommand> {
        self.children()
            .commands
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    /// Builder des letzten Aufrufs für das Kindfenster `id`.
    pub fn child_builder(&self, id: egui::ViewportId) -> Option<egui::ViewportBuilder> {
        self.children().builders.get(&id).cloned()
    }

    /// Spielt das Betriebssystem für die Größe des Kindfensters `id`: Eine neue Größe im Builder
    /// (beim Öffnen und bei jeder Änderung) und jeder neue `InnerSize`-Befehl gelten als Punkte
    /// des Kindfensters (wie winit mit dessen Faktor). `last` merkt, was schon angewandt ist.
    pub fn apply_child_size(
        &mut self,
        id: egui::ViewportId,
        last: &mut (Option<egui::Vec2>, usize),
    ) {
        let built = self.child_builder(id).and_then(|b| b.inner_size);
        let commands = self.child_commands(id);
        let mut size = None;
        if built != last.0 {
            size = built;
            last.0 = built;
        }
        for c in &commands[last.1.min(commands.len())..] {
            if let egui::ViewportCommand::InnerSize(s) = c {
                size = Some(*s);
            }
        }
        last.1 = commands.len();
        if let Some(s) = size {
            self.child_info(id, |i| {
                let min = i.outer_rect.map_or(egui::Pos2::ZERO, |r| r.min);
                i.outer_rect = Some(egui::Rect::from_min_size(min, s));
                i.inner_rect = i.outer_rect;
            });
        }
    }

    /// Wie oft das Kindfenster `id` gezeichnet wurde.
    pub fn child_runs(&self, id: egui::ViewportId) -> usize {
        self.children().runs.get(&id).copied().unwrap_or(0)
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
        self.frame_ui_shapes(events, &mut |ui| f(ui.ctx()))
    }

    /// Wie [`Screen::frame`], aber mit der obersten `Ui` des Bilds (etwa für `PlanoraApp::ui`).
    pub fn frame_ui(
        &mut self,
        events: Vec<egui::Event>,
        f: &mut dyn FnMut(&mut egui::Ui),
    ) -> Vec<Seen> {
        self.frame_ui_shapes(events, f).0
    }

    /// Legt `events` ins nächste Bild des Kindfensters `id` (ohne ein Bild auszuführen).
    pub fn queue_child(&mut self, id: egui::ViewportId, events: Vec<egui::Event>) {
        self.children()
            .child_events
            .entry(id)
            .or_default()
            .extend(events);
    }

    /// Wie [`Screen::frame_shapes`] mit der obersten `Ui`.
    pub fn frame_ui_shapes(
        &mut self,
        events: Vec<egui::Event>,
        f: &mut dyn FnMut(&mut egui::Ui),
    ) -> (Vec<Seen>, Vec<egui::epaint::ClippedShape>) {
        self.time += 0.05;
        if let Some(c) = &self.children {
            c.borrow_mut().time = self.time;
        }
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(raw, |ui| f(ui));
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk_shape(&c.shape, c.clip_rect, &mut seen);
        }
        let shapes = std::mem::take(&mut out.shapes);
        self.repaint_delay = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        // Befehle an Kindfenster gibt egui erst mit dem Bild des Hauptfensters aus.
        if let Some(c) = &self.children {
            let mut c = c.borrow_mut();
            for (id, v) in &mut out.viewport_output {
                if *id != egui::ViewportId::ROOT && !v.commands.is_empty() {
                    c.commands.entry(*id).or_default().append(&mut v.commands);
                }
            }
        }
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

/// Ein Eintrag einer ZIP-Datei, unabhängig vom Programmcode gelesen.
#[allow(dead_code)]
#[derive(Debug)]
pub struct ZipEntry {
    pub name: String,
    pub flags: u16,
    pub method: u16,
    pub data: Vec<u8>,
}

/// Bitweise CRC-32 (IEEE), bewusst ohne die Bibliothek des Programms.
#[allow(dead_code)]
pub fn crc32_plain(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Liest das Inhaltsverzeichnis (Central Directory) einer ZIP-Datei von Hand und prüft je
/// Eintrag Kopf, Größen und Prüfsumme gegen den lokalen Kopf.
#[allow(dead_code)]
pub fn zip_entries(z: &[u8]) -> Vec<ZipEntry> {
    let u16_at = |i: usize| u16::from_le_bytes([z[i], z[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([z[i], z[i + 1], z[i + 2], z[i + 3]]);
    let eocd = (0..=z.len() - 22)
        .rev()
        .find(|&i| u32_at(i) == 0x0605_4b50)
        .expect("Ende des Inhaltsverzeichnisses");
    let count = usize::from(u16_at(eocd + 10));
    assert_eq!(u16_at(eocd + 8), u16_at(eocd + 10));
    let cd_size = u32_at(eocd + 12) as usize;
    let mut at = u32_at(eocd + 16) as usize;
    assert_eq!(at + cd_size, eocd, "Verzeichnis direkt vor dem Ende");
    let mut out = Vec::new();
    for _ in 0..count {
        assert_eq!(u32_at(at), 0x0201_4b50, "Eintrag im Verzeichnis");
        let flags = u16_at(at + 8);
        let method = u16_at(at + 10);
        let crc = u32_at(at + 16);
        let csize = u32_at(at + 20) as usize;
        let usize_ = u32_at(at + 24) as usize;
        let nlen = usize::from(u16_at(at + 28));
        let elen = usize::from(u16_at(at + 30));
        let clen = usize::from(u16_at(at + 32));
        let local = u32_at(at + 42) as usize;
        let name = String::from_utf8(z[at + 46..at + 46 + nlen].to_vec()).expect("UTF-8-Name");
        assert_eq!(u32_at(local), 0x0403_4b50, "lokaler Kopf");
        assert_eq!(u16_at(local + 6), flags);
        assert_eq!(u32_at(local + 14), crc);
        assert_eq!(u32_at(local + 18) as usize, csize);
        let lnlen = usize::from(u16_at(local + 26));
        let lelen = usize::from(u16_at(local + 28));
        assert_eq!(&z[local + 30..local + 30 + lnlen], name.as_bytes());
        let start = local + 30 + lnlen + lelen;
        let data = z[start..start + csize].to_vec();
        assert_eq!(csize, usize_, "gespeichert, nicht gepackt");
        assert_eq!(crc32_plain(&data), crc, "Prüfsumme von {name}");
        out.push(ZipEntry {
            name,
            flags,
            method,
            data,
        });
        at += 46 + nlen + elen + clen;
    }
    out
}

/// Nur die Namen der Einträge (siehe [`zip_entries`]).
#[allow(dead_code)]
pub fn zip_names(z: &[u8]) -> Vec<String> {
    zip_entries(z).into_iter().map(|e| e.name).collect()
}
