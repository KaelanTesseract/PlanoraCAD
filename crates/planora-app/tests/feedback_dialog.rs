//! Fenster „Rückmeldung“, Menü „Hilfe“ und Tastenkürzel Strg+Umschalt+R: Einträge im Menü
//! „Hilfe“ (Rückmeldung mit Kürzel, Häufige Probleme, Über PlanoraCAD; „Über“ nicht mehr im
//! Menü „Datei“), Häkchen und bereinigte Vorschau, Mindestlänge, „Im Browser öffnen“ mit und
//! ohne Rückmeldungsordner, „Text kopieren“, Esc und „Abbrechen“, Fensterregeln wie bei der
//! Übersicht der Tastenkürzel, Größe bei 1920 × 1080 und 800 × 600.
//!
//! Mit echten Bildern der ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`); Bibliothek,
//! Konfiguration und Rückmeldungsordner nur im Temp-Ordner. Der Dateimanager ist eine
//! Nachbildung, der Browser wird nie geöffnet (nur die Ausgabe `OutputCommand::OpenUrl`).

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, OutputCommand, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{
    MenuRequest, PlanoraApp, faq_entry, feedback_entry, help_menu_title, settings_entry,
    view_shortcuts_entry,
};
use planora_app::build_info::{BuildInfo, GpuInfo};
use planora_app::create_dialog::CreateDialog;
use planora_app::diag::Step;
use planora_app::dialog::DialogSize;
use planora_app::feedback::{
    self, ATTACH_HINT, ATTACH_HINT_PICTURE, FEEDBACK_DIR, FEEDBACK_FILE, Part, Personal,
    SystemOpen, percent_encode,
};
use planora_app::feedback_dialog::*;
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult};
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{about_dialog, fonts, theme};
use planora_project::config::Config;
use planora_project::library::Library;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const SMALL: Vec2 = vec2(800.0, 600.0);
const NONE: Modifiers = Modifiers::NONE;
const CTRL_SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: true,
    mac_cmd: false,
    command: true,
};
const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};
const REPO_URL: &str = "https://github.com/KaelanTesseract/PlanoraCAD";

// ---- Hilfen ---------------------------------------------------------------------------------

/// Nachbildung des Dateimanagers: merkt sich die geöffneten Ordner.
struct FakeOpen(Arc<Mutex<Vec<PathBuf>>>);

impl SystemOpen for FakeOpen {
    fn open_folder(&self, dir: &Path) -> bool {
        self.0.lock().unwrap().push(dir.to_path_buf());
        true
    }
}

struct Seen {
    text: String,
    rect: Rect,
    color: egui::Color32,
}

fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        egui::epaint::Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: Rect::from_min_size(t.pos, t.galley.size()),
            color: t.override_text_color.unwrap_or_else(|| {
                t.galley
                    .job
                    .sections
                    .first()
                    .map_or(t.fallback_color, |s| s.format.color)
            }),
        }),
        _ => {}
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    size: Vec2,
    time: f64,
    seen: Vec<Seen>,
    /// Alle Ausgaben (Browser, Zwischenablage) seit dem Start.
    commands: Vec<OutputCommand>,
    /// Alle Befehle an das Hauptfenster (z. B. Bild anfordern) seit dem Start.
    viewport: Vec<egui::ViewportCommand>,
    /// Von der Nachbildung geöffnete Ordner.
    opened: Arc<Mutex<Vec<PathBuf>>>,
    /// Datenordner (Ordner der Konfigurationsdatei) im Temp-Ordner.
    data: PathBuf,
}

impl Rig {
    fn new(prefix: &str) -> Rig {
        Rig::with_size(prefix, SIZE)
    }

    fn with_size(prefix: &str, size: Vec2) -> Rig {
        let data = common::temp_root(prefix);
        std::fs::create_dir_all(&data).unwrap();
        let mut ws = Workspace::new(
            Some(Library::open(&data.join("Projekte")).unwrap()),
            Config::default(),
        );
        ws.config_path = data.join("config.json");
        assert_ne!(ws.config_path, Config::config_path(), "nie die echte Datei");
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let opened = Arc::new(Mutex::new(Vec::new()));
        let mut app = PlanoraApp::for_tests(ws);
        app.set_system_open(Box::new(FakeOpen(opened.clone())));
        let mut rig = Rig {
            ctx,
            app,
            size,
            time: 0.0,
            seen: Vec::new(),
            commands: Vec::new(),
            viewport: Vec::new(),
            opened,
            data,
        };
        rig.settle();
        rig
    }

    fn frame_with(&mut self, events: Vec<Event>, modifiers: Modifiers) {
        self.time += 1.0 / 60.0;
        let mut all = vec![Event::ModifiersChanged(modifiers)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events: all,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        self.commands
            .extend(out.platform_output.commands.iter().cloned());
        for v in out.viewport_output.values() {
            self.viewport.extend(v.commands.iter().cloned());
        }
        out.drop_without_applying_deltas();
        self.seen = seen;
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.frame_with(events, NONE);
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            self.frame(vec![]);
        }
    }

    fn texts(&self) -> Vec<&str> {
        self.seen.iter().map(|s| s.text.as_str()).collect()
    }

    fn sees(&self, text: &str) -> bool {
        self.seen.iter().any(|s| s.text == text)
    }

    /// Rechteck des untersten sichtbaren Texts `text`.
    fn find(&self, text: &str) -> Rect {
        self.seen
            .iter()
            .filter(|s| s.text == text)
            .max_by(|a, b| a.rect.top().partial_cmp(&b.rect.top()).unwrap())
            .unwrap_or_else(|| panic!("„{text}“ nicht zu sehen: {:?}", self.texts()))
            .rect
    }

    fn click_at(&mut self, p: Pos2) {
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: NONE,
        };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle();
    }

    fn click(&mut self, text: &str) {
        self.settle();
        let p = self.find(text).center();
        self.click_at(p);
    }

    /// Ein Menü der Leiste öffnen (der oberste Text mit diesem Namen).
    fn open_menu(&mut self, name: &str) {
        self.settle();
        let p = self
            .seen
            .iter()
            .filter(|s| s.text == name)
            .min_by(|a, b| a.rect.top().partial_cmp(&b.rect.top()).unwrap())
            .unwrap_or_else(|| panic!("Menü „{name}“ fehlt: {:?}", self.texts()))
            .rect
            .center();
        self.click_at(p);
    }

    fn press(&mut self, k: Key, m: Modifiers) {
        let ev = |pressed| Event::Key {
            key: k,
            physical_key: Some(k),
            pressed,
            repeat: false,
            modifiers: m,
        };
        self.frame_with(vec![ev(true)], m);
        self.frame_with(vec![ev(false)], NONE);
        self.settle();
    }

    fn type_text(&mut self, s: &str) {
        self.frame(vec![Event::Text(s.into())]);
        self.settle();
    }

    fn fb(&mut self) -> &mut FeedbackDialog {
        self.app.feedback_mut()
    }

    /// Öffnet das Fenster mit den Angaben aus [`context`].
    fn open_with_context(&mut self) {
        let c = context(&self.data);
        self.fb().open_fresh(c);
        self.settle();
    }

    fn urls(&self) -> Vec<String> {
        self.commands
            .iter()
            .filter_map(|c| match c {
                OutputCommand::OpenUrl(u) => Some(u.url.clone()),
                _ => None,
            })
            .collect()
    }

    fn copies(&self) -> Vec<String> {
        self.commands
            .iter()
            .filter_map(|c| match c {
                OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    fn opened(&self) -> Vec<PathBuf> {
        self.opened.lock().unwrap().clone()
    }

    fn bundle_root(&self) -> PathBuf {
        self.data.join(FEEDBACK_DIR)
    }

    fn window_rect(&self) -> Rect {
        self.ctx
            .memory(|m| m.area_rect(window_id()))
            .expect("Fenster Rückmeldung gezeigt")
    }
}

/// Angaben wie beim Öffnen, mit einem Benutzernamen, der nirgends erscheinen darf.
fn context(data: &Path) -> FeedbackContext {
    FeedbackContext {
        info: BuildInfo {
            version: planora_app::running_version(),
            os: "Windows 11 (Build 26100)".into(),
            gpu: Some(GpuInfo {
                name: "Testkarte 3000".into(),
                driver: r"31.0 C:\Users\Max Müller\treiber".into(),
                backend: "Vulkan".into(),
            }),
        },
        screen: Some((1920, 1080)),
        steps: vec![
            Step {
                time: "14:03:22".into(),
                label: "Skizze extrudieren".into(),
            },
            Step {
                time: "14:03:30".into(),
                label: "Speichern".into(),
            },
        ],
        log: vec![
            "2026-10-26T14:03:20Z INFO Start PlanoraCAD".into(),
            r"2026-10-26T14:03:21Z INFO Geöffnet C:\Users\Max Müller\Kunden\Halter.pcad".into(),
            "2026-10-26T14:03:22Z WARN Mail an max.mueller@example.org".into(),
        ],
        personal: Personal {
            user_names: vec!["Max Müller".into()],
            home_dirs: vec![r"C:\Users\Max Müller".into()],
            project_paths: vec![],
        },
        data_dir: data.to_path_buf(),
        crash_report: None,
        from_crash: false,
    }
}

/// Die Felder der Adresse, dekodiert: (Name, Inhalt) in ihrer Reihenfolge.
fn url_fields(url: &str) -> Vec<(String, String)> {
    let query = url.split_once('?').expect("Adresse mit Feldern").1;
    query
        .split('&')
        .map(|pair| {
            let (name, value) = pair.split_once('=').expect("Feld mit Wert");
            (name.to_string(), percent_decode(value))
        })
        .collect()
}

/// Umkehrung von `feedback::percent_encode` (UTF-8).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap();
            out.push(u8::from_str_radix(hex, 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("gültiges UTF-8")
}

/// Inhalt des Felds `name` der Adresse (dekodiert).
fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

fn chord(key: Key, ctrl: bool, shift: bool) -> KeyChord {
    KeyChord {
        key,
        ctrl,
        shift,
        alt: false,
    }
}

const ALL_PARTS: [Part; 8] = [
    Part::Version,
    Part::System,
    Part::Grafik,
    Part::Schritte,
    Part::Protokoll,
    Part::Bild,
    Part::Projektdatei,
    Part::Absturzbericht,
];

/// Klappt die Vorschau aller Bestandteile auf.
fn open_previews(ctx: &egui::Context) {
    for p in ALL_PARTS {
        let mut s = egui::collapsing_header::CollapsingState::load_with_default_open(
            ctx,
            preview_id(p),
            false,
        );
        s.set_open(true);
        s.store(ctx);
    }
}

// ---- Menü „Hilfe“ ---------------------------------------------------------------------------

#[test]
fn help_menu_has_feedback_faq_and_about_and_file_menu_no_longer_has_about() {
    assert_eq!(help_menu_title(), "Hilfe");
    assert_eq!(feedback_entry(), "Rückmeldung …");
    assert_eq!(faq_entry(), "Häufige Probleme");
    let mut r = Rig::new("planora-fb-menu");
    // „Hilfe“ steht in der Leiste rechts neben „Ansicht“.
    let ansicht = r.find("Ansicht");
    let hilfe = r.find(&help_menu_title());
    assert!(
        (hilfe.center().y - ansicht.center().y).abs() < 2.0 && hilfe.left() > ansicht.right(),
        "Hilfe nach Ansicht: {ansicht:?} {hilfe:?}"
    );

    r.open_menu(&help_menu_title());
    for t in [
        feedback_entry().as_str(),
        faq_entry().as_str(),
        about_dialog::title().as_str(),
        "Strg+Umschalt+R",
    ] {
        assert!(r.sees(t), "„{t}“ fehlt: {:?}", r.texts());
    }
    let (fb, faq, about) = (
        r.find(&feedback_entry()),
        r.find(&faq_entry()),
        r.find(about_dialog::title().as_str()),
    );
    assert!(
        fb.top() < faq.top() && faq.top() < about.top(),
        "Reihenfolge"
    );
    let keys = r.find("Strg+Umschalt+R");
    assert!(
        (keys.center().y - fb.center().y).abs() < 4.0 && keys.left() > fb.right(),
        "Kürzel in der Zeile von „{}“",
        feedback_entry()
    );

    // Häufige Probleme öffnet die Seite im Browser.
    r.click(&faq_entry());
    assert_eq!(r.urls(), [feedback::faq_url()]);
    assert!(!r.fb().open);

    // Rückmeldung … öffnet das Fenster.
    r.open_menu(&help_menu_title());
    r.click(&feedback_entry());
    assert!(r.fb().open, "Rückmeldung offen");
    assert!(r.sees(&title()), "{:?}", r.texts());
    r.click(&cancel());
    assert!(!r.fb().open);

    // Über PlanoraCAD öffnet den Über-Dialog.
    r.open_menu(&help_menu_title());
    r.click(about_dialog::title().as_str());
    assert!(r.app.ws_mut().about_open, "Über offen");
    r.app.ws_mut().about_open = false;
    r.settle();

    // Im Menü „Datei“ steht „Über“ nicht mehr.
    r.open_menu("Datei");
    assert!(r.sees(&settings_entry()), "Datei offen: {:?}", r.texts());
    assert!(
        !r.sees(about_dialog::title().as_str()),
        "kein Über im Menü Datei"
    );
    assert_eq!(r.urls().len(), 1, "sonst nichts im Browser");
}

#[test]
fn faq_and_guide_urls_point_to_master_like_the_forms() {
    assert_eq!(
        feedback::faq_url(),
        format!("{REPO_URL}/blob/master/docs/haeufige-probleme.md")
    );
    assert_eq!(
        feedback::guide_url(),
        format!("{REPO_URL}/blob/master/docs/so-meldest-du-etwas.md")
    );
    let config = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/ISSUE_TEMPLATE/config.yml"),
    )
    .unwrap();
    assert!(config.contains(&format!("url: {}", feedback::faq_url())));
    assert!(config.contains(&format!("url: {}", feedback::guide_url())));
}

// ---- Tastenkürzel ---------------------------------------------------------------------------

#[test]
fn ctrl_shift_r_opens_the_window_even_with_an_open_tab_dialog() {
    let mut r = Rig::new("planora-fb-taste");
    r.app.ws_mut().active_tab_mut().dialog = Some(CreateDialog::Box {
        x: 10.0,
        y: 20.0,
        z: 30.0,
    });
    r.settle();
    assert!(r.sees("Quader erzeugen"), "{:?}", r.texts());
    r.press(Key::R, CTRL_SHIFT);
    assert!(r.fb().open, "Strg+Umschalt+R öffnet die Rückmeldung");
    assert!(r.sees(&title()), "{:?}", r.texts());
    assert!(
        r.app.ws_mut().active_tab().dialog.is_some(),
        "das Fenster des Tabs bleibt"
    );
    // Strg+R allein (ohne Umschalt) öffnet nichts.
    r.click(&cancel());
    assert!(!r.fb().open);
    r.press(Key::R, CTRL);
    assert!(!r.fb().open, "Strg+R ist nicht das Kürzel");
}

#[test]
fn shortcut_is_listed_and_rebindable() {
    let a = Action::FeedbackOpen;
    assert_eq!(Action::from_id("hilfe.rueckmeldung"), Some(a));
    assert!(Action::ALL.contains(&a));
    assert_eq!(a.id(), "hilfe.rueckmeldung");
    assert_eq!(a.label(), "Rückmeldung geben");
    assert_eq!(a.area(), Area::All);
    assert_eq!(
        a.default_bindings(),
        &[Binding::Key(chord(Key::R, true, true))]
    );
    // Nach „Exportieren“ und vor „Rückgängig“ (Reihenfolge der Übersicht).
    let pos = |x: Action| Action::ALL.iter().position(|&y| y == x).unwrap();
    assert_eq!(pos(a), pos(Action::Export) + 1);
    assert_eq!(pos(Action::Undo), pos(a) + 1);
    assert_eq!(Keymap::defaults().text(a), "Strg+Umschalt+R");

    // Die Übersicht zeigt die Aktion.
    let mut r = Rig::new("planora-fb-uebersicht");
    r.open_menu("Ansicht");
    r.click(&view_shortcuts_entry());
    assert!(r.app.overview_mut().open);
    r.app.overview_mut().filter = "Rückmeldung".into();
    r.settle();
    assert!(r.sees("Rückmeldung geben"), "{:?}", r.texts());
    assert!(r.sees("Strg+Umschalt+R"), "{:?}", r.texts());
    r.app.overview_mut().close();
    r.settle();

    // Geänderte Belegung wirkt, die alte nicht mehr; das Menü nennt die neue.
    let new = Binding::Key(chord(Key::K, true, true));
    let mut km = r.app.ws_mut().keymap.clone();
    assert_eq!(km.set(a, 0, new), SetResult::Ok);
    assert!(r.app.ws_mut().set_keymap(km));
    r.settle();
    r.press(Key::R, CTRL_SHIFT);
    assert!(!r.fb().open, "alte Belegung wirkt nicht mehr");
    r.press(Key::K, CTRL_SHIFT);
    assert!(r.fb().open, "neue Belegung öffnet");
    r.click(&cancel());
    r.open_menu(&help_menu_title());
    assert!(r.sees("Strg+Umschalt+K"), "{:?}", r.texts());
    // Gespeichert unter der festen Id.
    let saved = Config::load_from(&r.data.join("config.json"));
    assert_eq!(
        saved.keys.get("hilfe.rueckmeldung").cloned(),
        Some(vec!["Strg+Umschalt+K".to_string()])
    );
}

#[test]
fn no_default_shortcut_conflicts() {
    let km = Keymap::defaults();
    for &a in Action::ALL {
        for b in km.bindings(a) {
            assert_eq!(km.conflict_for(a, b), None, "{a:?} {b:?}");
        }
    }
    // Strg+Umschalt+R gehört überall der Rückmeldung; R allein bleibt das Rechteck der Skizze.
    for area in [Area::All, Area::Model, Area::Sketch, Area::Drawing] {
        assert_eq!(
            km.action_for_key(area, &chord(Key::R, true, true)),
            Some(Action::FeedbackOpen),
            "{area:?}"
        );
    }
    assert_eq!(
        km.action_for_key(Area::Sketch, &chord(Key::R, false, false)),
        Some(Action::SketchRectangle)
    );
}

// ---- Inhalt ---------------------------------------------------------------------------------

#[test]
fn default_checkboxes() {
    let tmp = common::temp_root("planora-fb-haekchen");
    std::fs::create_dir_all(&tmp).unwrap();
    let mut d = FeedbackDialog::default();
    assert!(!d.open);
    d.open_fresh(context(&tmp));
    assert!(d.open);
    for (p, on) in [
        (Part::Version, true),
        (Part::System, true),
        (Part::Grafik, true),
        (Part::Schritte, true),
        (Part::Protokoll, false),
        (Part::Bild, false),
        (Part::Projektdatei, false),
        (Part::Absturzbericht, false),
    ] {
        assert_eq!(d.is_checked(p), on, "{p:?}");
    }
    assert_eq!(
        d.shown_parts(),
        [
            Part::Version,
            Part::System,
            Part::Grafik,
            Part::Schritte,
            Part::Protokoll,
            Part::Bild
        ]
    );
    assert_eq!(d.preview_text(Part::Bild), Some(shot_not_yet()));
    // Die Adresse enthält nur die angehakten Teile.
    d.description = "Beim Extrudieren passiert nichts.".into();
    let url = d.built().url;
    for f in ["version", "system", "grafik", "schritte"] {
        assert!(url.contains(&format!("&{f}=")), "{f}: {url}");
    }
    assert!(!url.contains("&protokoll="), "{url}");
    // System mit Bildschirmgröße.
    assert_eq!(
        d.preview_text(Part::System),
        Some("Windows 11 (Build 26100)\nScreen: 1920 × 1080".to_string())
    );
    assert_eq!(screen_line(1920, 1080), "Screen: 1920 × 1080");

    // Mit Absturzbericht (aus „Hilfe“ geöffnet): eigener Bestandteil, bereinigt, aber nicht
    // angehakt (an sind nur Version, System, Grafikkarte und letzte Schritte).
    let report = tmp.join("absturz.txt");
    std::fs::write(&report, r"Panik in C:\Users\Max Müller\x.rs").unwrap();
    let mut c = context(&tmp);
    c.crash_report = Some(report);
    d.open_fresh(c);
    assert!(!d.is_checked(Part::Absturzbericht));
    assert!(d.shown_parts().contains(&Part::Absturzbericht));
    let text = d.preview_text(Part::Absturzbericht).unwrap();
    assert!(
        text.contains("<user>") && !text.contains("Müller"),
        "{text}"
    );

    // Im Fenster stehen die Häkchen mit ihren Namen.
    let mut r = Rig::new("planora-fb-haekchen-bild");
    r.open_with_context();
    for p in [
        Part::Version,
        Part::System,
        Part::Grafik,
        Part::Schritte,
        Part::Protokoll,
    ] {
        assert!(r.sees(&p.label()), "{p:?}: {:?}", r.texts());
    }
    for t in [
        kind_label(),
        bug_label(),
        preview_heading(),
        privacy_line(),
        show_address(),
        open_browser(),
        cancel(),
    ] {
        assert!(r.sees(&t), "„{t}“ fehlt: {:?}", r.texts());
    }
}

#[test]
fn crash_report_is_off_unless_opened_from_the_crash_prompt() {
    let tmp = common::temp_root("planora-fb-absturz");
    std::fs::create_dir_all(&tmp).unwrap();
    let report = tmp.join("absturz.txt");
    std::fs::write(&report, "Panik in main.rs:3").unwrap();
    for (from_crash, on) in [(false, false), (true, true)] {
        let mut c = context(&tmp);
        c.crash_report = Some(report.clone());
        c.from_crash = from_crash;
        let mut d = FeedbackDialog::default();
        d.open_fresh(c);
        assert!(
            d.shown_parts().contains(&Part::Absturzbericht),
            "{from_crash}"
        );
        assert_eq!(d.is_checked(Part::Absturzbericht), on, "{from_crash}");
        // Die übrigen Häkchen stehen in beiden Fällen gleich.
        for (p, want) in [
            (Part::Version, true),
            (Part::System, true),
            (Part::Grafik, true),
            (Part::Schritte, true),
            (Part::Protokoll, false),
        ] {
            assert_eq!(d.is_checked(p), want, "{from_crash}: {p:?}");
        }
        d.description = "Es ist abgestürzt.".into();
        let fields = url_fields(&d.built().url);
        let protokoll = field(&fields, feedback::FIELD_PROTOKOLL);
        assert_eq!(
            protokoll.is_some_and(|t| t.contains("Panik in main.rs:3")),
            on,
            "{from_crash}: {protokoll:?}"
        );
    }
    assert!(!Part::Absturzbericht.default_on());
}

#[test]
fn preview_is_exactly_what_the_address_carries() {
    let tmp = common::temp_root("planora-fb-gleich");
    std::fs::create_dir_all(&tmp).unwrap();
    let report = tmp.join("absturz.txt");
    std::fs::write(&report, r"Panik in C:\Users\Max Müller\x.rs & 100 %").unwrap();
    let mut c = context(&tmp);
    c.crash_report = Some(report);
    c.from_crash = true;
    let mut d = FeedbackDialog::default();
    d.open_fresh(c);
    d.set_checked(Part::Protokoll, true);
    d.description = "Beim Speichern & Laden: ä/ö/ü # 50 % + 👍".into();

    // Fehler: Version, System, Grafikkarte und Schritte je in ihrem Feld, genau der Text der
    // Vorschau; Protokoll und Absturzbericht gemeinsam im Feld „protokoll“, je mit Überschrift.
    let built = d.built();
    assert!(built.dropped.is_empty() && !built.steps_cut && !built.description_cut);
    let fields = url_fields(&built.url);
    let checked: Vec<Part> = d
        .shown_parts()
        .into_iter()
        .filter(|&p| d.is_checked(p))
        .collect();
    assert_eq!(
        checked,
        [
            Part::Version,
            Part::System,
            Part::Grafik,
            Part::Schritte,
            Part::Protokoll,
            Part::Absturzbericht
        ]
    );
    for (p, name) in [
        (Part::Version, feedback::FIELD_VERSION),
        (Part::System, feedback::FIELD_SYSTEM),
        (Part::Grafik, feedback::FIELD_GRAFIK),
        (Part::Schritte, feedback::FIELD_SCHRITTE),
    ] {
        assert_eq!(field(&fields, name), d.preview_text(p).as_deref(), "{p:?}");
    }
    let log = format!(
        "{}:\n{}\n\n{}:\n{}",
        Part::Protokoll.heading(),
        d.preview_text(Part::Protokoll).unwrap(),
        Part::Absturzbericht.heading(),
        d.preview_text(Part::Absturzbericht).unwrap()
    );
    assert_eq!(
        field(&fields, feedback::FIELD_PROTOKOLL),
        Some(log.as_str())
    );
    assert_eq!(
        field(&fields, feedback::FIELD_PASSIERT),
        Some(format!("{}\n\n{}", d.description, ATTACH_HINT).as_str())
    );

    // Wunsch: alle angehakten Teile mit Überschrift gesammelt im Feld „angaben“.
    d.kind = feedback::Kind::Wunsch;
    let fields = url_fields(&d.built().url);
    let blocks: Vec<String> = checked
        .iter()
        .map(|&p| format!("{}:\n{}", p.heading(), d.preview_text(p).unwrap()))
        .collect();
    assert_eq!(
        field(&fields, feedback::FIELD_ANGABEN),
        Some(blocks.join("\n\n").as_str())
    );
    // Nicht angehakt: auch nicht in der Adresse.
    d.set_checked(Part::Grafik, false);
    let fields = url_fields(&d.built().url);
    let angaben = field(&fields, feedback::FIELD_ANGABEN).unwrap();
    assert!(!angaben.contains(d.preview_text(Part::Grafik).unwrap().as_str()));
}

#[test]
fn area_none_means_no_area_line() {
    assert_eq!(area_none(), "Keine Angabe");
    let mut r = Rig::new("planora-fb-bereich");
    r.open_with_context();
    r.fb().description = "Die Skizze springt beim Bemaßen.".into();
    r.settle();
    // Am Anfang: keine Angabe.
    assert_eq!(r.fb().area, None);
    assert!(r.sees(&area_none()), "{:?}", r.texts());
    let url = r.fb().built().url;
    assert!(!url.contains(&percent_encode("Bereich:")), "{url}");
    // Klappliste: „Skizze“ wählen.
    r.click(&area_none());
    r.click(feedback::AREAS[0]);
    assert_eq!(r.fb().area, Some("Skizze"));
    let fields = url_fields(&r.fb().built().url);
    assert!(
        field(&fields, feedback::FIELD_PASSIERT)
            .is_some_and(|t| t.starts_with("Bereich: Skizze\n")),
        "{fields:?}"
    );
    // Zurück auf „Keine Angabe“: kein Bereich, keine Zeile „Bereich: …“.
    r.click("Skizze");
    r.click(&area_none());
    assert_eq!(r.fb().area, None);
    let url = r.fb().built().url;
    assert!(!url.contains(&percent_encode("Bereich:")), "{url}");
    r.click(&open_browser());
    let sent = r.urls().pop().unwrap();
    assert!(!sent.contains(&percent_encode("Bereich:")), "{sent}");
    assert!(!sent.contains("bereich="), "nie als Parameter: {sent}");
}

#[test]
fn preview_shows_sanitized_text() {
    let mut r = Rig::new("planora-fb-vorschau");
    r.open_with_context();
    r.fb().set_checked(Part::Protokoll, true);
    r.fb().description = "Das Speichern hängt seit heute.".into();
    open_previews(&r.ctx);
    r.settle();
    let all = r.texts().join("\n");
    assert!(all.contains("<user>"), "Vorschau zu sehen: {all}");
    assert!(all.contains("<email>"), "{all}");
    assert!(all.contains("Halter.pcad"), "{all}");
    for secret in ["Max Müller", "Müller", "max.mueller", "Kunden"] {
        assert!(!all.contains(secret), "„{secret}“ in der Vorschau: {all}");
    }
    // Ebenso in der Adresse und in den Teilen der Meldung.
    let built = r.fb().built();
    for secret in ["Max Müller", "Müller", "max.mueller", "Kunden"] {
        assert!(!built.url.contains(&percent_encode(secret)), "{secret}");
        for (p, t) in r.fb().report().parts {
            assert!(!t.contains(secret), "{p:?}: {t}");
        }
    }
    // Die Beschreibung bleibt, wie sie geschrieben wurde.
    assert_eq!(
        r.fb().report().description,
        "Das Speichern hängt seit heute."
    );
}

#[test]
fn button_disabled_below_10_chars_with_hint() {
    assert_eq!(MIN_CHARS, 10);
    assert_eq!(too_short(), "Bitte schreib mindestens 10 Zeichen.");
    let mut r = Rig::new("planora-fb-kurz");
    r.open_with_context();
    assert!(r.sees(&too_short()), "Hinweis am Anfang: {:?}", r.texts());
    // Leerzeichen zählen nicht.
    r.fb().description = "   kurz      ".into();
    r.click(&open_browser());
    assert!(r.urls().is_empty(), "gesperrt");
    assert!(r.fb().sent.is_none());
    assert!(r.sees(&too_short()));
    r.fb().description = "123456789".into();
    r.click(&open_browser());
    assert!(r.urls().is_empty(), "9 Zeichen reichen nicht");
    r.fb().description = "1234567890".into();
    r.settle();
    assert!(!r.sees(&too_short()), "Hinweis weg ab 10 Zeichen");
    r.click(&open_browser());
    assert_eq!(r.urls().len(), 1, "10 Zeichen reichen");
}

#[test]
fn open_browser_emits_the_issue_url_and_shows_copy_hint() {
    let mut r = Rig::new("planora-fb-browser");
    r.press(Key::R, CTRL_SHIFT);
    assert!(r.fb().open);
    // Das Beschreibungsfeld hat die Tastatur.
    r.type_text("Beim Extrudieren stürzt es ab & mehr");
    assert_eq!(r.fb().description, "Beim Extrudieren stürzt es ab & mehr");
    r.fb().area = Some("3D");
    r.click(&open_browser());
    let urls = r.urls();
    assert_eq!(urls.len(), 1, "{urls:?}");
    let url = &urls[0];
    assert!(
        url.starts_with(&format!("{REPO_URL}/issues/new?template=fehler.yml")),
        "{url}"
    );
    assert!(
        url.contains(&percent_encode("Beim Extrudieren stürzt es ab & mehr")),
        "{url}"
    );
    assert!(url.contains(&percent_encode("Bereich: 3D")), "{url}");
    assert!(url.len() < 2000);
    // Kurz: kein Ordner, keine Datei.
    assert!(r.opened().is_empty());
    assert!(!r.bundle_root().exists());
    assert!(!url.contains(&percent_encode(ATTACH_HINT)));
    // Das Fenster bleibt offen und hilft weiter.
    assert!(r.fb().open);
    for t in [browser_hint(), copy_text(), close()] {
        assert!(r.sees(&t), "„{t}“ fehlt: {:?}", r.texts());
    }
    assert!(!r.sees(&open_browser()), "nicht zweimal schicken");
    assert_eq!(r.fb().sent.as_ref().unwrap().url, *url);
    r.click(&close());
    assert!(!r.fb().open);
    assert_eq!(r.urls().len(), 1);
}

#[test]
fn wish_and_question_go_to_the_wish_form() {
    let mut r = Rig::new("planora-fb-wunsch");
    r.open_with_context();
    r.click(&feedback::Kind::Frage.label());
    assert_eq!(r.fb().kind, feedback::Kind::Frage);
    r.fb().description = "Wie spiegle ich eine Skizze?".into();
    r.click(&open_browser());
    let url = r.urls().pop().unwrap();
    assert!(url.contains("template=wunsch.yml"), "{url}");
    assert!(url.contains(&percent_encode("Frage: ")), "{url}");
    assert!(url.contains("&angaben="), "{url}");
}

#[test]
fn copy_puts_the_url_into_the_clipboard() {
    let mut r = Rig::new("planora-fb-kopieren");
    r.open_with_context();
    r.fb().description = "Die Bemaßung springt weg.".into();
    // Vor dem Schicken: unter „Adresse anzeigen“.
    r.click(&show_address());
    let url = r.fb().built().url;
    assert!(r.sees(&url), "Adresse als Text: {:?}", r.texts());
    r.click(&copy_text());
    assert_eq!(r.copies(), std::slice::from_ref(&url));
    assert!(r.sees(&copied()), "{:?}", r.texts());
    assert!(r.urls().is_empty(), "Kopieren öffnet keinen Browser");
    // Nach dem Schicken: neben dem Hinweis.
    r.click(&open_browser());
    assert!(
        !r.sees(&copied()),
        "Hinweis „Kopiert.“ gilt nur bis zum Schicken"
    );
    r.click(&copy_text());
    assert_eq!(r.copies(), [url.clone(), url.clone()]);
    assert_eq!(r.urls(), [url]);
    assert!(r.sees(&copied()));
}

#[test]
fn long_input_writes_bundle_and_opens_folder() {
    let mut r = Rig::new("planora-fb-lang");
    r.open_with_context();
    r.fb().set_checked(Part::Protokoll, true);
    let long = "Erst skizziert, dann extrudiert, dann gespeichert. ".repeat(80);
    r.fb().description = long.clone();
    r.click(&open_browser());
    let opened = r.opened();
    assert_eq!(opened.len(), 1, "Ordner geöffnet: {opened:?}");
    let dir = &opened[0];
    assert!(dir.starts_with(r.bundle_root()), "{dir:?}");
    let file = dir.join(FEEDBACK_FILE);
    let text = std::fs::read_to_string(&file).expect("rueckmeldung.txt angelegt");
    assert!(
        text.contains(long.trim_end()),
        "ganze Beschreibung in der Datei"
    );
    assert!(text.contains("<user>") && !text.contains("Müller"));
    let url = r.urls().pop().expect("Browser");
    assert!(url.len() < 2000, "{}", url.len());
    assert!(url.contains(&percent_encode(ATTACH_HINT)), "{url}");
    assert_eq!(FEEDBACK_FILE, "rueckmeldung.txt");
    assert!(r.sees(&folder_hint(&[FEEDBACK_FILE])), "{:?}", r.texts());
    assert!(!r.sees(&no_file()));
    assert_eq!(r.fb().sent.as_ref().unwrap().folder.as_ref(), Some(dir));
}

#[test]
fn unwritable_data_dir_still_opens_browser_with_no_file_hint() {
    let mut r = Rig::new("planora-fb-gesperrt");
    // Wo der Ordner hin soll, liegt eine Datei: anlegen geht nicht.
    std::fs::write(r.bundle_root(), "kein Ordner").unwrap();
    r.open_with_context();
    r.fb().description = "Sehr lange Beschreibung. ".repeat(120);
    r.click(&open_browser());
    assert_eq!(r.urls().len(), 1, "der Browser öffnet trotzdem");
    assert!(r.opened().is_empty(), "kein Ordner");
    assert!(r.sees(&no_file()), "{:?}", r.texts());
    assert!(!r.sees(&folder_hint(&[FEEDBACK_FILE])));
    assert!(r.sees(&browser_hint()));
    assert!(r.fb().sent.as_ref().unwrap().file_failed);

    // Kurz mit Absturzbericht: ohne Datei auch kein Hinweis „Datei hier anhängen“.
    let crash = r.data.join("absturz.txt");
    std::fs::write(&crash, "Panik").unwrap();
    let mut c = context(&r.data);
    c.crash_report = Some(crash);
    c.from_crash = true;
    r.fb().open_fresh(c);
    r.fb().description = "Es ist abgestürzt.".into();
    r.click(&open_browser());
    let url = r.urls().pop().unwrap();
    assert!(!url.contains(&percent_encode(ATTACH_HINT)), "{url}");
    assert!(r.sees(&no_file()));
}

#[test]
fn unchecked_parts_are_not_in_the_url() {
    let mut r = Rig::new("planora-fb-abgewaehlt");
    r.open_with_context();
    r.fb().description = "Der Kreis wird zur Ellipse.".into();
    // Ein Klick auf das Häkchen nimmt es weg.
    r.click(&Part::Version.label());
    assert!(!r.fb().is_checked(Part::Version));
    r.fb().set_checked(Part::System, false);
    r.click(&open_browser());
    let url = r.urls().pop().unwrap();
    assert!(!url.contains("&version="), "{url}");
    assert!(!url.contains("&system="), "{url}");
    assert!(!url.contains("&protokoll="), "{url}");
    assert!(url.contains("&grafik="), "{url}");
    assert!(url.contains("&schritte="), "{url}");
    assert!(!url.contains("Windows"), "{url}");
}

#[test]
fn esc_and_cancel_close_without_side_effects() {
    let mut r = Rig::new("planora-fb-esc");
    r.press(Key::R, CTRL_SHIFT);
    assert!(r.fb().open);
    r.type_text("Etwas ganz Langes, das nicht weggeht.");
    // Erstes Esc: das Feld gibt die Tastatur ab; zweites Esc: das Fenster schließt.
    r.press(Key::Escape, NONE);
    r.press(Key::Escape, NONE);
    assert!(!r.fb().open, "Esc schließt");
    assert!(!r.sees(&title()));

    r.open_with_context();
    r.fb().description = "Etwas ganz Langes, das nicht weggeht.".into();
    r.click(&cancel());
    assert!(!r.fb().open, "Abbrechen schließt");

    assert!(r.urls().is_empty(), "kein Browser");
    assert!(r.copies().is_empty(), "nichts kopiert");
    assert!(r.opened().is_empty(), "kein Ordner");
    assert!(!r.bundle_root().exists(), "keine Datei");
}

#[test]
fn window_rules_like_the_overview() {
    let mut r = Rig::new("planora-fb-regeln");
    // Öffnet sich über „Über“ und schließt es.
    r.app.ws_mut().about_open = true;
    r.settle();
    r.press(Key::R, CTRL_SHIFT);
    assert!(r.fb().open);
    assert!(!r.app.ws_mut().about_open, "Über schließt");
    // „Über“ öffnet sich über der Rückmeldung (das zuletzt geöffnete Fenster gewinnt).
    r.open_menu(&help_menu_title());
    r.click(about_dialog::title().as_str());
    assert!(r.app.ws_mut().about_open);
    assert!(!r.fb().open, "Rückmeldung tritt zurück");
    r.app.ws_mut().about_open = false;
    r.settle();

    // Über die Übersicht der Tastenkürzel.
    r.open_menu("Ansicht");
    r.click(&view_shortcuts_entry());
    assert!(r.app.overview_mut().open);
    r.open_menu(&help_menu_title());
    r.click(&feedback_entry());
    assert!(r.fb().open);
    assert!(!r.app.overview_mut().open, "Übersicht schließt");
    // Die Übersicht danach schließt die Rückmeldung.
    r.open_menu("Ansicht");
    r.click(&view_shortcuts_entry());
    assert!(r.app.overview_mut().open);
    assert!(!r.fb().open);
    r.app.overview_mut().close();
    r.settle();

    // Einstellungen schließen sie ebenso.
    r.open_with_context();
    r.open_menu("Datei");
    r.click(&settings_entry());
    assert!(r.app.windows_mut().2.is_open());
    assert!(!r.fb().open, "Einstellungen gewinnen");
    r.app.windows_mut().2.close();
    r.settle();

    // Ein zweites Öffnen behält den Text.
    r.open_with_context();
    r.fb().description = "Bleibt stehen.".into();
    r.app
        .apply_menu_request(&r.ctx.clone(), MenuRequest::Feedback);
    r.settle();
    assert_eq!(r.fb().description, "Bleibt stehen.");

    // Die Rückmeldung zählt als offenes Fenster: Strg+Z ist gesperrt.
    r.press(Key::Escape, NONE); // das Feld gibt die Tastatur ab
    assert!(r.fb().open);
    r.app
        .ws_mut()
        .active_tab_mut()
        .state
        .create_sphere(5.0)
        .unwrap();
    r.app.ws_mut().observe_history(false);
    let bodies = r.app.ws_mut().active_tab().state.bodies.len();
    r.press(Key::Z, CTRL);
    assert_eq!(
        r.app.ws_mut().active_tab().state.bodies.len(),
        bodies,
        "Strg+Z wirkt nicht bei offener Rückmeldung"
    );
    r.click(&cancel());
    r.press(Key::Z, CTRL);
    assert_eq!(
        r.app.ws_mut().active_tab().state.bodies.len(),
        bodies - 1,
        "danach wieder"
    );

    // Während einer Rückfrage öffnet sie nicht (weder Taste noch Menüwunsch).
    r.app.ws_mut().prompt = Some(Prompt::Notice("Hinweis".into()));
    r.settle();
    r.press(Key::R, CTRL_SHIFT);
    assert!(!r.fb().open, "nicht während einer Rückfrage");
    r.app
        .apply_menu_request(&r.ctx.clone(), MenuRequest::Feedback);
    r.settle();
    assert!(!r.fb().open, "auch nicht über den Menüwunsch");
}

// ---- Größe ----------------------------------------------------------------------------------

#[test]
fn fits_at_1920x1080_and_does_not_panic_at_800x600() {
    let mut r = Rig::new("planora-fb-gross");
    r.open_with_context();
    let rect = r.window_rect();
    let screen = Rect::from_min_size(Pos2::ZERO, SIZE);
    assert!(screen.contains_rect(rect), "{rect:?}");
    assert!(
        (rect.width() - DialogSize::Wide.width()).abs() < 0.5,
        "breit: {rect:?}"
    );
    assert!(
        (rect.center().x - screen.center().x).abs() < 1.0,
        "mittig: {rect:?}"
    );
    for t in [open_browser(), cancel(), privacy_line()] {
        assert!(rect.contains_rect(r.find(&t)), "{t} im Fenster");
    }

    // Klein, mit allem aufgeklappt und langem Protokoll: läuft durch, bleibt im Fenster.
    let mut r = Rig::with_size("planora-fb-klein", SMALL);
    let mut c = context(&r.data);
    c.log = (0..100)
        .map(|i| {
            format!(
                "2026-10-26T14:03:{:02}Z INFO Zeile {i} mit etwas Text",
                i % 60
            )
        })
        .collect();
    r.fb().open_fresh(c);
    r.fb().set_checked(Part::Protokoll, true);
    r.fb().description = "Lang. ".repeat(400);
    open_previews(&r.ctx);
    r.settle();
    let rect = r.window_rect();
    let screen = Rect::from_min_size(Pos2::ZERO, SMALL);
    assert!(screen.contains_rect(rect), "{rect:?}");
    // Der Inhalt rollt (die Fußzeile bleibt): bis „Adresse anzeigen“ zu sehen ist, aufklappen,
    // weiter rollen.
    let wheel = |r: &mut Rig| {
        let at = r.window_rect().center();
        r.frame(vec![
            Event::PointerMoved(at),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -200.0),
                phase: egui::TouchPhase::Move,
                modifiers: NONE,
            },
        ]);
        r.settle();
    };
    for _ in 0..40 {
        if r.sees(&show_address()) {
            break;
        }
        wheel(&mut r);
    }
    assert!(r.sees(&open_browser()), "Fußzeile bleibt: {:?}", r.texts());
    r.click(&show_address());
    for _ in 0..5 {
        wheel(&mut r);
    }
    assert!(screen.contains_rect(r.window_rect()));
    r.click(&open_browser());
    assert_eq!(r.urls().len(), 1);
    r.click(&close());
    assert!(!r.fb().open);
}

// ---- Texte ----------------------------------------------------------------------------------

#[test]
fn texts_are_plain_language() {
    assert_eq!(title(), "Rückmeldung");
    assert_eq!(kind_label(), "Worum geht es?");
    assert_eq!(
        bug_label(),
        "Beschreib kurz, was du gemacht hast und was passiert ist."
    );
    assert_eq!(preview_heading(), "Das geht mit");
    assert_eq!(open_browser(), "Im Browser öffnen");
    assert_eq!(cancel(), "Abbrechen");
    assert_eq!(show_address(), "Adresse anzeigen");
    assert_eq!(copy_text(), "Text kopieren");
    assert_eq!(
        browser_hint(),
        "Hat sich kein Browser geöffnet? Kopier die Adresse und füg sie im Browser ein."
    );
    assert_eq!(copied(), "Kopiert.");
    assert_eq!(
        no_file(),
        "Die Datei ließ sich nicht anlegen. Die Meldung geht trotzdem."
    );
    assert_eq!(
        privacy_line(),
        "Die Angaben gehen erst mit dem Öffnen im Browser an GitHub oder das Formular. Gespeichert und gemeldet wird erst, wenn du dort absendest."
    );
    assert!(folder_hint(&["rueckmeldung.txt"]).contains("„rueckmeldung.txt“"));
    let all = [
        title().to_string(),
        kind_label(),
        area_label(),
        area_none(),
        bug_label(),
        too_short(),
        preview_heading(),
        open_browser(),
        cancel(),
        close(),
        show_address(),
        copy_text(),
        browser_hint(),
        copied(),
        no_file(),
        privacy_line(),
        folder_hint(&["rueckmeldung.txt"]),
        screen_line(1920, 1080),
    ];
    for t in &all {
        // Der Datenschutz-Satz ist ehrlich und darum länger; er bricht im Fenster um.
        let max = if *t == privacy_line() { 140 } else { 90 };
        assert!(t.chars().count() <= max, "kurz genug: {t}");
        for tech in ["URL", "Issue", "Parameter", "::", "Sie ", "Ihre"] {
            assert!(!t.contains(tech), "{t} enthält {tech}");
        }
    }
    // Die Häkchen tragen die Namen der Bestandteile.
    assert_eq!(Part::Grafik.label(), "Grafikkarte");
    assert_eq!(Part::Schritte.label(), "Letzte Schritte");
}

// ---- Bild vom Fenster ----------------------------------------------------------------------

fn shot_requests(r: &Rig) -> usize {
    r.viewport
        .iter()
        .filter(|c| {
            matches!(c, egui::ViewportCommand::Screenshot(d)
                if planora_app::feedback_shot::is_our_shot(d))
        })
        .count()
}

fn screenshot_event(w: usize, h: usize, data: egui::UserData) -> Event {
    Event::Screenshot {
        viewport_id: egui::ViewportId::ROOT,
        user_data: data,
        image: Arc::new(egui::ColorImage::new(
            [w, h],
            vec![egui::Color32::from_rgb(10, 20, 30); w * h],
        )),
    }
}

#[test]
fn app_sends_viewport_screenshot_command_after_one_hidden_frame() {
    use planora_app::feedback_shot::{SHOT_FILE, preview_line, shot_user_data};
    let mut r = Rig::new("planora-fb-bild");
    r.open_with_context();
    assert!(r.sees(&title()));
    assert_eq!(shot_requests(&r), 0);
    open_previews(&r.ctx);
    r.settle();
    // Häkchen „Bild vom Fenster“ setzen: das Fenster verschwindet ein Bild lang, dann kommt
    // die Anforderung (das Bild davor zeigt kein Rückmeldungsfenster).
    let p = r.find(&Part::Bild.label()).center();
    r.frame(vec![Event::PointerMoved(p)]);
    r.frame(vec![Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: NONE,
    }]);
    r.frame(vec![Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: NONE,
    }]);
    // Spätestens zwei Bilder danach ist das Bild angefordert, und zwar in einem Bild ohne
    // Rückmeldungsfenster.
    let mut frames = 0;
    while shot_requests(&r) == 0 {
        assert!(frames < 2, "keine Anforderung");
        r.frame(vec![]);
        frames += 1;
    }
    assert!(
        !r.sees(&title()),
        "Fenster beim Anfordern verborgen: {:?}",
        r.texts()
    );
    assert_eq!(shot_requests(&r), 1);
    r.frame(vec![]);
    assert!(!r.sees(&title()), "verborgen, solange gewartet wird");
    // Ein fremdes Bild ändert nichts.
    r.frame(vec![screenshot_event(5, 5, egui::UserData::new("fremd"))]);
    assert!(!r.sees(&title()));
    // Das eigene Bild kommt: Fenster wieder da, Vorschau mit Größe, Häkchen gesetzt.
    r.frame(vec![screenshot_event(64, 48, shot_user_data())]);
    r.settle();
    assert!(r.sees(&title()), "wieder gezeigt");
    assert!(r.fb().is_checked(Part::Bild));
    let line = preview_line(&egui::ColorImage::new(
        [64, 48],
        vec![egui::Color32::BLACK; 64 * 48],
    ));
    assert_eq!(r.fb().preview_text(Part::Bild), Some(line));
    assert_eq!(shot_requests(&r), 1, "nur einmal angefordert");

    // Schicken: das Bild liegt im Rückmeldungsordner, die Adresse sagt „anhängen“.
    r.fb().description = "Die Linie springt beim Ziehen weg.".into();
    r.click(&open_browser());
    let opened = r.opened();
    assert_eq!(opened.len(), 1, "{opened:?}");
    let png = opened[0].join(SHOT_FILE);
    let bytes = std::fs::read(&png).expect("bild.png angelegt");
    assert_eq!(&bytes[..4], b"\x89PNG");
    let url = r.urls().pop().unwrap();
    assert!(url.contains(&percent_encode(ATTACH_HINT_PICTURE)), "{url}");
    assert!(
        r.sees(&folder_hint(&[FEEDBACK_FILE, SHOT_FILE])),
        "{:?}",
        r.texts()
    );
}

#[test]
fn picture_is_named_in_the_window_hint_and_the_web_address() {
    use planora_app::feedback_shot::SHOT_FILE;
    let mut r = Rig::new("planora-fb-bild-web");
    r.open_with_context();
    r.fb().description = "Die Linie springt beim Ziehen weg.".into();
    r.fb().set_checked(Part::Bild, true);
    let t0 = std::time::Instant::now();
    r.fb().shot_tick(t0);
    r.fb()
        .shot_received(std::sync::Arc::new(egui::ColorImage::new(
            [8, 8],
            vec![egui::Color32::BLACK; 64],
        )));
    r.settle();
    assert!(r.fb().is_checked(Part::Bild));
    r.click(&webform_button());
    let url = r.urls().pop().unwrap();
    assert!(url.starts_with(planora_app::feedback::WEBFORM_URL), "{url}");
    assert!(url.contains(&percent_encode(ATTACH_HINT_PICTURE)), "{url}");
    assert!(
        r.sees(&folder_hint(&[FEEDBACK_FILE, SHOT_FILE])),
        "{:?}",
        r.texts()
    );
    assert!(!r.sees(&folder_hint(&[FEEDBACK_FILE])));
}

#[test]
fn shot_timeout_unchecks_picture_and_shows_hint() {
    use planora_app::feedback_shot::{SHOT_TIMEOUT, ShotStep, shot_failed};
    let tmp = common::temp_root("planora-fb-bild-zeit");
    std::fs::create_dir_all(&tmp).unwrap();
    let mut d = FeedbackDialog::default();
    d.open_fresh(context(&tmp));
    d.set_checked(Part::Bild, true);
    assert!(d.hides_for_shot());
    let t0 = std::time::Instant::now();
    assert_eq!(d.shot_tick(t0), ShotStep::RequestScreenshot);
    assert_eq!(d.shot_tick(t0 + SHOT_TIMEOUT), ShotStep::ShowDialogAgain);
    assert!(!d.hides_for_shot());
    assert!(!d.is_checked(Part::Bild), "ohne Bild kein Häkchen");
    assert_eq!(d.preview_text(Part::Bild), Some(shot_failed()));
    // Erneut anhaken versucht es noch einmal.
    d.set_checked(Part::Bild, true);
    assert!(d.hides_for_shot());
    // Abwählen bricht ab und vergisst das Bild.
    d.set_checked(Part::Bild, false);
    assert!(!d.hides_for_shot());
    assert_eq!(d.preview_text(Part::Bild), Some(shot_not_yet()));
}

#[test]
fn webform_button_is_locked_below_10_chars() {
    assert_eq!(webform_button(), "Ohne GitHub-Konto senden");
    let mut r = Rig::new("planora-fb-web-kurz");
    r.open_with_context();
    assert!(
        r.sees(&webform_button()) && r.sees(&webform_hint()),
        "{:?}",
        r.texts()
    );
    r.fb().description = "123456789".into();
    r.click(&webform_button());
    assert!(r.urls().is_empty(), "gesperrt");
    assert!(r.fb().sent.is_none());
    r.fb().description = "1234567890".into();
    r.settle();
    r.click(&webform_button());
    assert_eq!(r.urls().len(), 1);
}

#[test]
fn webform_button_opens_the_web_address_and_shows_copy_and_mail_hint() {
    let mut r = Rig::new("planora-fb-web");
    r.open_with_context();
    r.fb().area = Some("3D");
    r.fb().description = "Beim Extrudieren stürzt es ab & mehr".into();
    r.click(&webform_button());
    let urls = r.urls();
    assert_eq!(urls.len(), 1, "{urls:?}");
    assert!(
        urls[0].starts_with(planora_app::feedback::WEBFORM_URL),
        "{}",
        urls[0]
    );
    assert!(urls[0].contains("art=fehler") && urls[0].contains("bereich=3D"));
    assert!(r.opened().is_empty(), "kurz: kein Ordner");
    assert!(r.sees(&copy_text()) && r.sees(&browser_hint()));
    assert!(r.sees(&mail_fallback()), "{:?}", r.texts());
    assert_eq!(
        mail_fallback(),
        "Klappt beides nicht? Schreib an info@dennis-guse.de."
    );
}

#[test]
fn webform_button_opens_folder_when_needed() {
    let mut r = Rig::new("planora-fb-web-lang");
    r.open_with_context();
    r.fb().description = "Erst skizziert, dann extrudiert, dann gespeichert. ".repeat(80);
    r.click(&webform_button());
    let opened = r.opened();
    assert_eq!(opened.len(), 1, "{opened:?}");
    assert!(opened[0].join(FEEDBACK_FILE).exists());
    let url = r.urls().pop().expect("Browser");
    assert!(url.starts_with(planora_app::feedback::WEBFORM_URL));
    assert!(url.len() < 2000);
    assert!(url.contains(&percent_encode(ATTACH_HINT)), "{url}");
    assert!(r.sees(&folder_hint(&[FEEDBACK_FILE])));
}

// ---- Sichtprüfung Etappe 2 ------------------------------------------------------------------

#[test]
fn reopening_collapses_address_and_previews() {
    let mut r = Rig::new("planora-fb-collapse");
    r.open_with_context();
    open_previews(&r.ctx);
    r.settle();
    r.click(&show_address());
    let shows_address = |r: &Rig| {
        r.seen
            .iter()
            .any(|s| s.text.contains("template=fehler.yml"))
    };
    assert!(shows_address(&r), "Adresse sollte aufgeklappt sein");
    r.fb().close();
    r.settle();
    r.open_with_context();
    r.settle();
    assert!(
        !shows_address(&r),
        "Adresse muss eingeklappt sein {:?}",
        r.texts()
    );
    let shown = r.fb().shown_parts();
    assert!(!shown.is_empty());
    for p in shown {
        let s = egui::collapsing_header::CollapsingState::load_with_default_open(
            &r.ctx,
            preview_id(p),
            false,
        );
        assert!(!s.is_open(), "{p:?} muss eingeklappt sein");
    }
}

#[test]
fn description_text_uses_the_primary_text_colour() {
    for dark in [true, false] {
        let mut r = Rig::new("planora-fb-colour");
        theme::apply(&r.ctx, dark);
        r.open_with_context();
        r.fb().description = "Beim Extrudieren stürzt es ab".into();
        r.settle();
        let seen = r
            .seen
            .iter()
            .find(|s| s.text == "Beim Extrudieren stürzt es ab")
            .expect("Beschreibung zu sehen");
        assert_eq!(seen.color, theme::palette(dark).text_primary, "dark={dark}");
    }
}

#[test]
fn log_tells_github_from_webform() {
    let mut r = Rig::new("planora-fb-log");
    r.open_with_context();
    r.fb().description = "Beim Extrudieren stürzt es ab".into();
    r.click(&open_browser());
    let lines = planora_app::diag::recent_lines(500);
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with("Feedback (Bug) opened via GitHub")),
        "{lines:?}"
    );
    let mut r = Rig::new("planora-fb-log2");
    r.open_with_context();
    r.fb().description = "Beim Extrudieren stürzt es ab".into();
    r.click(&webform_button());
    let lines = planora_app::diag::recent_lines(500);
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with("Feedback (Bug) opened via the web form")),
        "{lines:?}"
    );
}

#[test]
fn description_prompt_follows_the_kind() {
    assert_eq!(wish_label(), "Was möchtest du tun können?");
    assert_eq!(question_label(), "Was möchtest du wissen?");
    let mut r = Rig::new("planora-fb-aufforderung");
    r.open_with_context();
    for (kind, text, others) in [
        (
            feedback::Kind::Wunsch,
            wish_label(),
            [bug_label(), question_label()],
        ),
        (
            feedback::Kind::Frage,
            question_label(),
            [bug_label(), wish_label()],
        ),
        (
            feedback::Kind::Fehler,
            bug_label(),
            [wish_label(), question_label()],
        ),
    ] {
        r.click(&kind.label());
        assert_eq!(description_label(kind), text);
        assert!(r.sees(&text), "{kind:?}: {:?}", r.texts());
        for o in others {
            assert!(!r.sees(&o), "{kind:?}: „{o}“ zu sehen");
        }
    }
}
