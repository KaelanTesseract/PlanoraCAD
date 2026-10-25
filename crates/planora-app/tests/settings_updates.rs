//! Einstellungen → Updates: laufende Version mit Kanal, Auswahl „Update-Kanal“ (heute nur Dev),
//! Schalter „Beim Start nach Updates suchen“ mit Hinweis zum Datenschutz, Knopf „Jetzt nach
//! Updates suchen“ (meldet den Wunsch an die App, Ergebnis daneben) und „Letzte Prüfung“. Gespeichert wird sofort über die
//! Wege des Arbeitsbereichs; misslingt das, gilt der Wert für die Sitzung (mit Hinweis) und
//! kommt beim nächsten gelungenen Schreiben mit. Konfiguration nur im Temp-Ordner.

mod common;

use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::keymap::Keymap;
use planora_app::settings_window::*;
use planora_app::workspace::Workspace;
use planora_app::{about_dialog, local_time, theme};
use planora_project::config::{Config, PendingUpdate, UPDATE_CHANNEL_IDS};
use planora_update::{Channel, Version};
use std::fs;
use std::path::PathBuf;

struct Env {
    ws: Workspace,
    tmp: PathBuf,
    cfg: PathBuf,
}

fn env(prefix: &str) -> Env {
    let tmp = common::temp_root(prefix);
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(None, Config::default());
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
    Env { ws, tmp, cfg }
}

/// Lenkt das Schreiben auf einen Pfad unter einer Datei um (Schreiben scheitert).
fn block(e: &mut Env) -> PathBuf {
    let blocker = e.tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    e.ws.config_path = blocker.join("config.json");
    blocker
}

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

#[derive(Clone, Debug)]
struct Shot {
    text: String,
    rect: Rect,
    clip: Rect,
    color: Color32,
}

fn walk(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<Shot>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
        egui::epaint::Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |s| s.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            out.push(Shot {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                clip,
                color,
            })
        }
        _ => {}
    }
}

/// Das Fenster ohne Bildschirm, wie in der App über `show_in`.
struct App {
    ctx: egui::Context,
    size: egui::Vec2,
    time: f64,
    w: SettingsWindow,
    e: Env,
}

impl App {
    fn new(e: Env, dark: bool, w: f32, h: f32) -> App {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        theme::apply(&ctx, dark);
        App {
            ctx,
            size: vec2(w, h),
            time: 0.0,
            w: SettingsWindow::default(),
            e,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Shot> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let (w, ws) = (&mut self.w, &mut self.e.ws);
        let out = self.ctx.run_ui(raw, |ui| {
            w.show_in(ui.ctx(), ws);
        });
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, c.clip_rect, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self, n: usize) -> Vec<Shot> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![]);
        }
        seen
    }

    /// Klick auf den sichtbaren Text `text`; bei mehreren der unterste.
    fn click(&mut self, text: &str) {
        self.time += 1.0;
        let seen = self.settle(4);
        let s = seen
            .iter()
            .filter(|s| s.text == text && s.clip.contains(s.rect.center()))
            .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar: {:?}", texts(&seen)))
            .clone();
        let at = s.rect.center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle(3);
    }
}

fn texts(seen: &[Shot]) -> Vec<&str> {
    seen.iter().map(|s| s.text.as_str()).collect()
}

fn find<'a>(seen: &'a [Shot], t: &str) -> &'a Shot {
    seen.iter()
        .find(|s| s.text == t)
        .unwrap_or_else(|| panic!("„{t}“ fehlt: {:?}", texts(seen)))
}

#[test]
fn running_version_comes_from_cargo() {
    // Der Build-Fehler (ungeeignete Versionsnummer) fällt hier auf, nicht erst beim Start.
    let parsed = Version::from_cargo(env!("CARGO_PKG_VERSION"));
    assert_eq!(planora_app::running_version(), parsed);
    assert_eq!(planora_app::RUNNING_VERSION, env!("CARGO_PKG_VERSION"));
    assert_eq!(parsed.to_string(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn section_shows_version_channel_switch_and_note() {
    for dark in [false, true] {
        let e = env("planora-updates-abschnitt");
        let mut a = App::new(e, dark, 1920.0, 1080.0);
        a.w.open(false);
        let seen = a.settle(6);
        assert_eq!(SECTION_UPDATES, "Updates");
        assert_eq!(CHANNEL_LABEL, "Update-Kanal");
        assert_eq!(CHECK_ON_START_LABEL, "Beim Start nach Updates suchen");
        assert_eq!(
            PRIVACY_NOTE,
            "Beim Suchen fragt das Programm bei GitHub nach; dabei sieht GitHub deine IP-Adresse."
        );
        assert_eq!(CHECK_NOW, "Jetzt nach Updates suchen");
        assert_eq!(CHECK_RUNNING, "Suche läuft …");
        let running = planora_app::running_version();
        let version_line = running_version_line();
        assert_eq!(
            version_line,
            format!(
                "Laufende Version: {} ({})",
                running,
                running.channel().label()
            )
        );
        assert_eq!(last_check_line(None), "Letzte Prüfung: noch nie");
        let never = last_check_line(None);
        for t in [
            SECTION_APPEARANCE,
            SECTION_UPDATES,
            version_line.as_str(),
            CHANNEL_LABEL,
            Channel::Dev.label(),
            CHECK_ON_START_LABEL,
            PRIVACY_NOTE,
            CHECK_NOW,
            never.as_str(),
            SECTION_PROJECTS,
            "Schließen",
        ] {
            find(&seen, t);
        }
        // Reihenfolge: Darstellung, Updates (Version und letzte Prüfung, Kanal und Schalter,
        // Hinweis, Knopf), Projekte.
        let order = [
            SECTION_APPEARANCE,
            SECTION_UPDATES,
            version_line.as_str(),
            CHANNEL_LABEL,
            PRIVACY_NOTE,
            CHECK_NOW,
            SECTION_PROJECTS,
        ];
        for pair in order.windows(2) {
            let (a, b) = (find(&seen, pair[0]), find(&seen, pair[1]));
            assert!(
                a.rect.bottom() <= b.rect.top(),
                "dunkel {dark}: „{}“ über „{}“",
                pair[0],
                pair[1]
            );
        }
        // Je Zeile zwei Angaben, nebeneinander.
        for (left, right) in [
            (version_line.as_str(), never.as_str()),
            (CHANNEL_LABEL, CHECK_ON_START_LABEL),
        ] {
            let (l, r) = (find(&seen, left), find(&seen, right));
            assert!(
                l.rect.right() < r.rect.left()
                    && (l.rect.center().y - r.rect.center().y).abs() < 4.0,
                "dunkel {dark}: „{left}“ links neben „{right}“"
            );
        }
        // Das Fenster lässt bei 1920 × 1080 die Menüleiste frei (sie bleibt bedienbar).
        let menu_bottom = 50.0;
        let window = a.w.last_rect.expect("Fenster gezeichnet");
        assert!(
            window.top() > menu_bottom,
            "dunkel {dark}: Fenster {window:?} über der Menüleiste"
        );
        // Der Hinweis zum Datenschutz ist zurückhaltend (schwache Schrift).
        let note = find(&seen, PRIVACY_NOTE);
        let label = find(&seen, CHECK_ON_START_LABEL);
        assert!(
            note.rect.height() <= label.rect.height() + 0.5,
            "kleine Schrift"
        );
        assert_eq!(
            note.color,
            theme::palette(dark).text_tertiary,
            "dunkel {dark}"
        );
        // Fußzeile bleibt im Fenster.
        let close = find(&seen, "Schließen");
        let rect = a.w.last_rect.expect("Fenster gezeichnet");
        assert!(
            rect.contains_rect(close.rect),
            "Schließen {:?} in {rect:?}",
            close.rect
        );
        assert!(close.clip.contains_rect(close.rect));
        // Die Texte sind deutsch und einfach.
        for t in [
            CHANNEL_LABEL,
            CHECK_ON_START_LABEL,
            PRIVACY_NOTE,
            CHECK_NOW,
            CHECK_RUNNING,
            UP_TO_DATE,
            CHECK_FAILED,
            UPDATES_NOT_SAVED,
        ] {
            for tech in ["API", "HTTP", "Cargo", "::", "config", "Release"] {
                assert!(!t.contains(tech), "{t} enthält {tech}");
            }
        }
    }
}

#[test]
fn the_check_now_button_asks_the_app_and_shows_the_hint() {
    for dark in [false, true] {
        let e = env("planora-updates-knopf");
        let mut a = App::new(e, dark, 1920.0, 1080.0);
        a.w.open(false);
        assert!(!a.w.take_check_request());
        a.click(CHECK_NOW);
        // Das Fenster sucht nicht selbst: es meldet den Wunsch (einmal) an die App.
        assert!(a.w.is_open());
        assert!(a.w.take_check_request());
        assert!(!a.w.take_check_request(), "abgeholt");
        assert_eq!(a.e.ws.last_update_check(), None);
        assert!(!a.e.cfg.exists(), "nichts geschrieben");
        // Läuft die Suche, ist der Knopf aus.
        a.w.set_check_hint(Some(CheckHint::Running));
        let seen = a.settle(3);
        find(&seen, CHECK_RUNNING);
        a.click(CHECK_NOW);
        assert!(!a.w.take_check_request(), "Knopf aus");
        // Ergebnisse neben dem Knopf, der Fehler in Warnfarbe; das Fenster lässt bei
        // 1920 × 1080 die Menüleiste frei.
        let warning = theme::palette(dark).warning;
        for (hint, warns) in [
            (CheckHint::UpToDate, false),
            (CheckHint::Found(v("0.2.0-dev.9")), false),
            (CheckHint::Failed, true),
        ] {
            a.w.set_check_hint(Some(hint.clone()));
            let seen = a.settle(3);
            let (button, shown) = (find(&seen, CHECK_NOW), find(&seen, &hint.text()));
            assert!(
                button.rect.right() < shown.rect.left()
                    && (button.rect.center().y - shown.rect.center().y).abs() < 4.0,
                "dunkel {dark}: {hint:?} neben dem Knopf"
            );
            assert_eq!(shown.color == warning, warns, "dunkel {dark}: {hint:?}");
            let window = a.w.last_rect.expect("Fenster gezeichnet");
            assert!(window.top() > 50.0, "dunkel {dark}: {window:?}");
        }
        // Neu öffnen leert ein Ergebnis, nicht aber „Suche läuft …“.
        a.w.close();
        a.w.open(false);
        assert_eq!(a.w.check_hint(), None);
        a.w.set_check_hint(Some(CheckHint::Running));
        a.w.close();
        a.w.open(false);
        assert_eq!(a.w.check_hint(), Some(&CheckHint::Running));
    }
}

#[test]
fn last_check_line_uses_local_time() {
    let t = 1_761_400_000u64;
    assert_eq!(
        last_check_line(Some(t)),
        format!("Letzte Prüfung: {}", local_time::unix_time_text(t as i64))
    );
    // Feste Zeitzone: 25.10.2025, 13:46:40 UTC → 15:46 bei +2 Stunden.
    assert_eq!(
        local_time::unix_time_text_with(t as i64, |_| Some(7200)),
        "25.10.2025, 15:46"
    );
    assert_eq!(
        local_time::unix_time_text_with(t as i64, |_| None),
        "25.10.2025, 13:46 UTC"
    );
    // Unsinnig große Werte stürzen nicht ab.
    let _ = last_check_line(Some(u64::MAX));
    // Im Fenster.
    let mut e = env("planora-updates-letzte");
    assert!(e.ws.set_last_update_check(t));
    let mut a = App::new(e, false, 1920.0, 1080.0);
    a.w.open(false);
    let seen = a.settle(6);
    find(&seen, &last_check_line(Some(t)));
}

#[test]
fn channel_choice_is_saved_and_dev_is_the_only_option() {
    assert_eq!(AVAILABLE_CHANNELS, &[Channel::Dev]);
    for c in AVAILABLE_CHANNELS {
        assert!(Channel::ALL.contains(c));
    }
    // Die Kennungen der Konfiguration sind die der Kanäle.
    assert_eq!(UPDATE_CHANNEL_IDS, Channel::ALL.map(Channel::id));

    let e = env("planora-updates-kanal");
    Config {
        root: Some(e.tmp.join("Projekte")),
        update_channel: "beta".into(),
        ..Config::default()
    }
    .save_to(&e.cfg)
    .unwrap();
    let mut e = e;
    e.ws.config = Config::read_from(&e.cfg).unwrap();
    assert_eq!(e.ws.update_channel(), Channel::Beta);
    let mut a = App::new(e, true, 1920.0, 1080.0);
    a.w.open(false);
    let seen = a.settle(6);
    // Die Auswahl zeigt den gespeicherten Kanal.
    find(&seen, Channel::Beta.label());
    a.click(Channel::Beta.label());
    let seen = a.settle(3);
    // Aufgeklappt: nur „Dev“ steht zur Wahl.
    assert!(
        texts(&seen).contains(&Channel::Dev.label()),
        "Dev wählbar: {:?}",
        texts(&seen)
    );
    assert!(
        !texts(&seen).contains(&Channel::Stable.label()),
        "Stabil nicht wählbar: {:?}",
        texts(&seen)
    );
    assert_eq!(
        seen.iter()
            .filter(|s| s.text == Channel::Beta.label())
            .count(),
        1,
        "Beta nur als Anzeige des gespeicherten Kanals"
    );
    a.click(Channel::Dev.label());
    assert_eq!(a.e.ws.update_channel(), Channel::Dev);
    let disk = Config::read_from(&a.e.cfg).unwrap();
    assert_eq!(disk.update_channel, "dev", "gespeichert");
    assert_eq!(
        disk.root,
        Some(a.e.tmp.join("Projekte")),
        "Verzeichnis bleibt"
    );
    assert_eq!(a.w.updates_message(), None);

    // Unbekannte Kennung im Speicher: Dev.
    a.e.ws.config.update_channel = "nightly".into();
    assert_eq!(a.e.ws.update_channel(), Channel::Dev);
}

#[test]
fn switch_is_saved_and_failure_keeps_value_for_the_session_with_hint() {
    for dark in [false, true] {
        let e = env("planora-updates-schalter");
        let mut a = App::new(e, dark, 1920.0, 1080.0);
        a.w.open(false);
        assert!(a.e.ws.update_check_enabled(), "Voreinstellung an");
        a.click(CHECK_ON_START_LABEL);
        assert!(!a.e.ws.update_check_enabled(), "aus");
        assert!(
            !Config::read_from(&a.e.cfg).unwrap().update_check,
            "gespeichert"
        );
        assert_eq!(a.w.updates_message(), None);
        a.click(CHECK_ON_START_LABEL);
        assert!(a.e.ws.update_check_enabled(), "wieder an");
        let text = fs::read_to_string(&a.e.cfg).unwrap();
        assert!(
            !text.contains("update_check"),
            "Voreinstellung nicht geschrieben: {text}"
        );

        // Schreiben scheitert: der Wert gilt für die Sitzung, Hinweis in Warnfarbe.
        let blocker = block(&mut a.e);
        a.click(CHECK_ON_START_LABEL);
        assert!(!a.e.ws.update_check_enabled(), "gilt trotzdem");
        assert_eq!(fs::read(&blocker).unwrap(), b"x");
        assert_eq!(
            a.w.updates_message(),
            Some(&RootMessage::Warning(UPDATES_NOT_SAVED.into()))
        );
        assert_eq!(
            UPDATES_NOT_SAVED,
            "Die Update-Einstellung gilt nur bis zum Programmende: sie konnte nicht gespeichert werden."
        );
        let seen = a.settle(3);
        assert_eq!(
            find(&seen, UPDATES_NOT_SAVED).color,
            theme::palette(dark).warning
        );
        // Nächstes gelungenes Schreiben (hier das Projektverzeichnis) nimmt den Wert mit.
        a.e.ws.config_path = a.e.cfg.clone();
        assert!(a.e.ws.try_persist_config());
        assert!(!Config::read_from(&a.e.cfg).unwrap().update_check);
        // Neu öffnen leert den Hinweis.
        a.w.close();
        a.w.open(false);
        assert_eq!(a.w.updates_message(), None);
    }
}

#[test]
fn workspace_accessors_roundtrip() {
    let mut e = env("planora-updates-zugriffe");
    let ws = &mut e.ws;
    assert_eq!(ws.update_channel(), Channel::Dev);
    assert!(ws.update_check_enabled());
    assert_eq!(ws.last_update_check(), None);
    assert_eq!(ws.skipped_version(), None);
    assert_eq!(ws.pending_update(), None);

    assert!(ws.set_update_channel(Channel::Stable));
    assert!(ws.set_update_check(false));
    assert!(ws.set_last_update_check(1_761_400_000));
    assert!(ws.set_skipped_version(Some(v("0.2.0-dev.4"))));
    assert!(ws.set_pending_update(Some((v("0.2.0-dev.1"), v("0.2.0-dev.2")))));
    let disk = Config::read_from(&e.cfg).unwrap();
    assert_eq!(disk.update_channel, "stable");
    assert!(!disk.update_check);
    assert_eq!(disk.last_update_check, Some(1_761_400_000));
    assert_eq!(disk.skipped_version.as_deref(), Some("0.2.0-dev.4"));
    assert_eq!(
        disk.update_pending,
        Some(PendingUpdate {
            from: "0.2.0-dev.1".into(),
            to: "0.2.0-dev.2".into()
        })
    );
    // Frisch geladen liest der Arbeitsbereich dasselbe.
    let mut again = Workspace::new(None, disk);
    again.config_path = e.cfg.clone();
    assert_eq!(again.update_channel(), Channel::Stable);
    assert!(!again.update_check_enabled());
    assert_eq!(again.last_update_check(), Some(1_761_400_000));
    assert_eq!(again.skipped_version(), Some(v("0.2.0-dev.4")));
    assert_eq!(
        again.pending_update(),
        Some((v("0.2.0-dev.1"), v("0.2.0-dev.2")))
    );
    // Zurücksetzen entfernt die Felder.
    assert!(again.set_skipped_version(None));
    assert!(again.set_pending_update(None));
    let text = fs::read_to_string(&e.cfg).unwrap();
    assert!(!text.contains("skipped_version"), "{text}");
    assert!(!text.contains("update_pending"), "{text}");
    // Unlesbare Versionen im Speicher: keine.
    again.config.skipped_version = Some("irgendwas".into());
    again.config.update_pending = Some(PendingUpdate {
        from: "0.1.0".into(),
        to: "kaputt".into(),
    });
    assert_eq!(again.skipped_version(), None);
    assert_eq!(again.pending_update(), None);
}

#[test]
fn pending_update_settings_ride_along_on_every_write_path() {
    // Jeder der vier Wege nimmt vorgemerkte Update-Felder mit.
    for path in ["verzeichnis", "durchsicht", "tasten", "favoriten"] {
        let mut e = env(&format!("planora-updates-mit-{path}"));
        let blocker = block(&mut e);
        assert!(!e.ws.set_update_check(false), "{path}");
        assert!(!e.ws.set_last_update_check(42), "{path}");
        assert_eq!(fs::read(&blocker).unwrap(), b"x");
        e.ws.config_path = e.cfg.clone();
        let written = match path {
            "verzeichnis" => e.ws.try_persist_config(),
            "durchsicht" => e.ws.set_panel_transparency(20),
            "tasten" => e.ws.set_keymap(Keymap::default()),
            _ => {
                let mut places = e.ws.places();
                places.add_favorite(&e.tmp.join("Teile")).unwrap();
                e.ws.save_places(places)
            }
        };
        assert!(written, "{path}");
        let disk = Config::read_from(&e.cfg).unwrap();
        assert!(!disk.update_check, "{path}");
        assert_eq!(disk.last_update_check, Some(42), "{path}");
        // Danach nicht mehr vorgemerkt: der Wert auf der Platte gehört wieder der Datei.
        Config::update_at(&e.cfg, |c| c.last_update_check = Some(99)).unwrap();
        assert!(e.ws.set_panel_transparency(30), "{path}");
        assert_eq!(
            Config::read_from(&e.cfg).unwrap().last_update_check,
            Some(99),
            "{path}"
        );
    }
}

#[test]
fn a_failed_update_write_never_clobbers_other_settings() {
    let mut e = env("planora-updates-nichts-verlieren");
    Config {
        root: Some(e.tmp.join("Projekte")),
        favorites: vec![e.tmp.join("Fav")],
        panel_transparency: 25,
        ..Config::default()
    }
    .save_to(&e.cfg)
    .unwrap();
    // Unlesbare Datei: unangetastet, der Wert gilt für die Sitzung.
    let good = fs::read(&e.cfg).unwrap();
    fs::write(&e.cfg, b"{kaputt").unwrap();
    assert!(!e.ws.set_update_channel(Channel::Beta));
    assert_eq!(e.ws.update_channel(), Channel::Beta);
    assert_eq!(fs::read(&e.cfg).unwrap(), b"{kaputt");
    // Datei wieder lesbar: ein Update-Feld schreibt nur Update-Felder (die anderen bleiben
    // so, wie eine andere Instanz sie hinterlassen hat), die vorgemerkten kommen mit.
    fs::write(&e.cfg, good).unwrap();
    assert!(e.ws.set_update_check(false));
    let disk = Config::read_from(&e.cfg).unwrap();
    assert_eq!(disk.update_channel, "beta");
    assert!(!disk.update_check);
    assert_eq!(disk.root, Some(e.tmp.join("Projekte")));
    assert_eq!(disk.favorites, [e.tmp.join("Fav")]);
    assert_eq!(disk.panel_transparency, 25);
    // Eine vorgemerkte Durchsichtigkeit kommt beim Schreiben eines Update-Felds mit.
    block(&mut e);
    assert!(!e.ws.set_panel_transparency(70));
    e.ws.config_path = e.cfg.clone();
    assert!(e.ws.set_last_update_check(7));
    let disk = Config::read_from(&e.cfg).unwrap();
    assert_eq!(disk.panel_transparency, 70);
    assert_eq!(disk.last_update_check, Some(7));
    assert!(!e.ws.panel_transparency_pending);
}

#[test]
fn about_dialog_shows_version_with_channel() {
    let running = planora_app::running_version();
    assert_eq!(
        about_dialog::version_line(),
        format!("Version {} ({})", running, running.channel().label())
    );
    assert_eq!(
        about_dialog::version_line_for(&v("0.2.0-dev.1")),
        "Version 0.2.0-dev.1 (Dev)"
    );
    assert_eq!(
        about_dialog::version_line_for(&v("1.0.0")),
        "Version 1.0.0 (Stabil)"
    );
    assert_eq!(
        running_version_line_for(&v("0.2.0-dev.1")),
        "Laufende Version: 0.2.0-dev.1 (Dev)"
    );
}

#[test]
fn keys_table_height_follows_the_window_and_the_menu_stays_free() {
    use planora_app::keymap::settings_ui::{
        TABLE_MAX_H, TABLE_MIN_H, TABLE_RESERVED_H, table_max_height,
    };
    assert_eq!(table_max_height(1080.0), 1080.0 - TABLE_RESERVED_H);
    assert_eq!(table_max_height(1080.0), 300.0);
    assert_eq!(table_max_height(600.0), TABLE_MIN_H);
    assert_eq!(table_max_height(4000.0), TABLE_MAX_H);
    let mut heights = Vec::new();
    for h in [900.0, 1080.0, 1440.0] {
        let e = env("planora-updates-tabellenhoehe");
        let mut a = App::new(e, true, 1920.0, h);
        a.w.open(false);
        // Mit Hinweiszeilen im Abschnitt „Updates“ (Ergebnis und Speicherfehler).
        a.w.set_check_hint(Some(CheckHint::Failed));
        block(&mut a.e);
        a.click(CHECK_ON_START_LABEL);
        let seen = a.settle(6);
        find(&seen, UPDATES_NOT_SAVED);
        let window = a.w.last_rect.expect("Fenster gezeichnet");
        assert!(
            window.top() > 50.0,
            "Höhe {h}: {window:?} über der Menüleiste"
        );
        assert!(window.bottom() < h - 50.0, "Höhe {h}: {window:?}");
        heights.push(window.height());
    }
    // Größere Fenster: die Tabelle nutzt den Platz (das Fenster wächst mit).
    assert!(
        heights.windows(2).all(|w| w[1] > w[0] + 100.0),
        "{heights:?}"
    );
}
