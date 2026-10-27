//! Sprache im Programm: Aufträge sprechen die Sprache dessen, der sie startet; Zahlen folgen
//! dem Dezimaltrenner der Sprache; die Auswahl „Sprache“ in Einstellungen → Darstellung; die
//! Sprache von Windows (bzw. `LANG`) als Vorschlag beim ersten Start; Probleme mit Sprachdateien
//! landen englisch im Protokoll; eine vorgemerkte Sprache kommt beim nächsten Schreiben mit.
//! Kein Test hier ruft `set_global` oder `Workspace::set_language` (das macht nur
//! `language_global.rs`); Englisch kommt über `with_lang`. Konfiguration nur im Temp-Ordner.

mod common;

use egui::{Pos2, Rect, vec2};
use planora_app::jobs::{Job, Poll};
use planora_app::settings_window::{self, SettingsWindow};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{
    blend_dialog, export_dialog, properties, sketch_view, step_import_dialog, system_language,
    theme, update_install,
};
use planora_feature::FeatureKind;
use planora_i18n::{LoadProblem, english, german, tr, with_lang};
use planora_math::DVec3;
use planora_project::config::Config;
use std::fs;
use std::path::PathBuf;

#[test]
fn a_job_speaks_the_language_of_its_starter() {
    let job = with_lang(&english(), || Job::spawn(|_| tr!("language-name")));
    assert_eq!(job.wait(), Poll::Ready("English".to_string()));
    // Ohne Überschreibung gilt im Test das globale Deutsch, auch im Arbeitsthread.
    let job = Job::spawn(|_| tr!("language-name"));
    assert_eq!(job.wait(), Poll::Ready("Deutsch".to_string()));
    // Die Überschreibung endet mit `with_lang`; der Auftrag behält die Sprache seines Starts.
    let job = with_lang(&english(), || {
        Job::spawn(|_| {
            std::thread::sleep(std::time::Duration::from_millis(30));
            tr!("language-name")
        })
    });
    assert_eq!(planora_i18n::current().id(), "de");
    assert_eq!(job.wait(), Poll::Ready("English".to_string()));
}

#[test]
fn numbers_in_the_properties_follow_the_language() {
    let state = AppState::new();
    let kind = FeatureKind::Box {
        size: DVec3::new(10.5, 20.0, 30.25),
        origin: DVec3::new(-1.5, 0.0, 2.0),
    };
    let rows =
        |lang: &planora_i18n::Lang| with_lang(lang, || properties::parameter_rows(&state, &kind));
    let de = rows(&german());
    assert_eq!(de[0].1, "10,50 × 20,00 × 30,25 mm");
    assert_eq!(de[1].1, "-1,50 / 0,00 / 2,00 mm");
    let en = rows(&english());
    assert_eq!(en[0].1, "10.50 × 20.00 × 30.25 mm");
    assert_eq!(en[1].1, "-1.50 / 0.00 / 2.00 mm");
}

#[test]
fn fixed_number_helpers_follow_the_language() {
    let all = || {
        vec![
            sketch_view::format_number(-0.001, 2),
            sketch_view::format_number(1.23456, 3),
            sketch_view::format_length(40.0),
            sketch_view::format_angle(std::f64::consts::FRAC_PI_2),
            blend_dialog::min_text(0.0125),
            export_dialog::mb_text(3_400_000),
            update_install::download_text(3 * 1024 * 1024 + 400 * 1024, 0),
            step_import_dialog::size_text(Some((DVec3::ZERO, DVec3::new(25.4, 50.8, 76.2)))),
        ]
    };
    let de = with_lang(&german(), all);
    assert_eq!(
        de,
        [
            "0,00",
            "1,235",
            "40,00 mm",
            "90,0°",
            "0,0125",
            "3,4 MB",
            "Wird heruntergeladen … 3,4 MB",
            "25,4 × 50,8 × 76,2 mm",
        ]
    );
    let en = with_lang(&english(), all);
    let expected: Vec<String> = de.iter().map(|s| s.replace(',', ".")).collect();
    assert_eq!(en, expected);
    // Gelesen werden Komma und Punkt in jeder Sprache.
    for lang in [german(), english()] {
        with_lang(&lang, || {
            assert_eq!(sketch_view::parse_number("2,5"), Some(2.5));
            assert_eq!(sketch_view::parse_number("2.5"), Some(2.5));
        });
    }
}

/// Zeichnet die Einstellungen einige Bilder lang und liefert die Texte des letzten Bildes.
fn settings_texts(ws: &mut Workspace) -> String {
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    theme::apply(&ctx, true);
    let mut w = SettingsWindow::default();
    w.open(false);
    let mut seen = String::new();
    for i in 0..4 {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
            time: Some(f64::from(i) * 0.05),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            w.show_in(ui.ctx(), ws);
        });
        seen = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    seen
}

fn temp_workspace(prefix: &str) -> (Workspace, PathBuf) {
    let tmp = common::temp_root(prefix);
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(None, Config::default());
    ws.config_path = tmp.join("config.json");
    assert_ne!(ws.config_path, Config::config_path(), "nie die echte Datei");
    (ws, tmp)
}

#[test]
fn the_language_choice_sits_in_appearance_and_shows_the_current_language() {
    let (mut ws, tmp) = temp_workspace("planora-sprache-auswahl");
    let de = settings_texts(&mut ws);
    let lines: Vec<&str> = de.lines().collect();
    assert!(lines.contains(&"Sprache"), "{de}");
    assert!(lines.contains(&"Deutsch"), "{de}");
    assert!(!lines.contains(&"Language"), "{de}");
    // „Sprache“ steht im Abschnitt „Darstellung“ (nach dessen Titel, vor „Updates“).
    let pos = |t: &str| lines.iter().position(|l| *l == t).unwrap();
    assert!(
        pos(settings_window::SECTION_APPEARANCE) < pos("Sprache"),
        "{de}"
    );
    let en = with_lang(&english(), || settings_texts(&mut ws));
    let lines: Vec<&str> = en.lines().collect();
    assert!(lines.contains(&"Language"), "{en}");
    assert!(lines.contains(&"English"), "{en}");
    assert!(!lines.contains(&"Sprache"), "{en}");
    assert_eq!(
        with_lang(&english(), settings_window::section_language),
        "Language"
    );
    assert_eq!(settings_window::section_language(), "Sprache");
    // Nur Zeichnen schreibt nichts.
    assert!(!ws.config_path.exists());
    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn the_saved_language_id_is_reported() {
    let (mut ws, tmp) = temp_workspace("planora-sprache-id");
    assert_eq!(ws.language_id(), None);
    ws.config.language = Some("en".into());
    assert_eq!(ws.language_id(), Some("en"));
    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn a_pending_language_is_written_with_the_next_setting() {
    let (mut ws, tmp) = temp_workspace("planora-sprache-vorgemerkt");
    ws.config.language = Some("en".into());
    ws.language_pending = true;
    assert!(ws.set_theme(false));
    let text = fs::read_to_string(&ws.config_path).unwrap();
    assert!(text.contains("\"language\": \"en\""), "{text}");
    assert!(!ws.language_pending);
    // Ohne Vormerkung schreibt ein anderes Feld die Sprache nicht.
    let (mut ws, tmp2) = temp_workspace("planora-sprache-nicht-vorgemerkt");
    ws.config.language = Some("en".into());
    assert!(ws.set_panel_transparency(10));
    assert!(
        !fs::read_to_string(&ws.config_path)
            .unwrap()
            .contains("language")
    );
    // Auch die übrigen Schreibwege nehmen eine vorgemerkte Sprache mit.
    ws.language_pending = true;
    ws.set_update_check(false);
    assert!(
        fs::read_to_string(&ws.config_path)
            .unwrap()
            .contains("\"language\": \"en\"")
    );
    assert!(!ws.language_pending);
    ws.config.language = Some("de".into());
    ws.language_pending = true;
    assert!(ws.set_panel_transparency(20));
    assert!(
        fs::read_to_string(&ws.config_path)
            .unwrap()
            .contains("\"language\": \"de\"")
    );
    ws.config.language = Some("en".into());
    ws.language_pending = true;
    assert!(ws.try_persist_config());
    assert!(
        fs::read_to_string(&ws.config_path)
            .unwrap()
            .contains("\"language\": \"en\"")
    );
    ws.config.language = Some("de".into());
    ws.language_pending = true;
    let km = ws.keymap.clone();
    assert!(ws.set_keymap(km));
    assert!(
        fs::read_to_string(&ws.config_path)
            .unwrap()
            .contains("\"language\": \"de\"")
    );
    assert!(!ws.language_pending);
    let _ = fs::remove_dir_all(tmp);
    let _ = fs::remove_dir_all(tmp2);
}

#[test]
fn posix_locales_become_language_ids() {
    let cases = [
        ("de_DE.UTF-8", Some("de-DE")),
        ("en_US", Some("en-US")),
        ("pt_BR.UTF-8@euro", Some("pt-BR")),
        ("fr", Some("fr")),
        ("C", None),
        ("POSIX", None),
        ("C.UTF-8", None),
        ("", None),
    ];
    for (raw, want) in cases {
        assert_eq!(
            system_language::from_posix_locale(raw).as_deref(),
            want,
            "{raw}"
        );
    }
    // Was das System meldet, ist nie leer.
    assert!(
        system_language::system_languages()
            .iter()
            .all(|l| !l.trim().is_empty())
    );
}

#[test]
fn problems_with_language_files_go_english_into_the_log() {
    let line = system_language::problem_line(&LoadProblem {
        file: PathBuf::from("C:/x/fr.ftl"),
        message: "syntax error (1 in total): boom".into(),
    });
    assert_eq!(
        line,
        "Language file skipped: C:/x/fr.ftl: syntax error (1 in total): boom"
    );
    // Eine kaputte Datei in einem der Ordner: übergangen, im Protokoll vermerkt, kein Absturz.
    let dir = common::temp_root("planora-sprachdateien");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("xx.ftl"), "das ist = = kein { Fluent").unwrap();
    let missing = dir.join("gibt-es-nicht");
    let lines = system_language::load_languages(&[missing, dir.clone()]);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("Language file skipped: "), "{lines:?}");
    assert!(lines[0].contains("xx.ftl"), "{lines:?}");
    assert!(planora_i18n::find("xx").is_none());
    let log = planora_app::diag::recent_lines(200);
    assert!(log.iter().any(|l| l.contains(&lines[0])), "{log:?}");
    let _ = fs::remove_dir_all(dir);
}
