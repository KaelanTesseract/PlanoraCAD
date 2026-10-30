//! Protokoll, letzte Schritte, Absturzbericht und `rueckmeldung.txt` sind immer englisch, auch
//! wenn das Programm deutsch spricht; nur der Text des Benutzers bleibt, wie er ist. Das
//! Rückmelde-Fenster und die Absturz-Rückfrage folgen dagegen der Sprache des Programms.

mod common;

use chrono::NaiveDate;
use common::{Screen, assert_english, assert_has, find};
use planora_app::build_info::{BuildInfo, GpuInfo};
use planora_app::commands;
use planora_app::crash::{self, ReportInput, StartFinding};
use planora_app::crash_dialog::{self, CrashNotice};
use planora_app::diag::{self, Step};
use planora_app::feedback::{self, Kind, Part, Personal};
use planora_app::feedback_dialog::{self, FeedbackContext, FeedbackDialog};
use planora_app::keymap::Action;
use planora_app::workspace::Workspace;
use planora_i18n::{english, german, with_lang};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use std::path::Path;

const GERMAN: &str = "äöüßÄÖÜ„";

fn assert_no_german(what: &str, text: &str) {
    assert!(
        !text.chars().any(|c| GERMAN.contains(c)),
        "{what}: deutscher Text in\n{text}"
    );
}

fn report_input(steps: Vec<Step>, log: Vec<String>) -> ReportInput {
    ReportInput {
        version: "0.2.0-dev.1".into(),
        message: "index out of bounds".into(),
        location: "crates/planora-app/src/app.rs:10:5".into(),
        thread: "main".into(),
        backtrace: "0: planora_app::app::run".into(),
        addresses: None,
        steps,
        log,
        document: Some(r"C:\Projekte\Halter.pcad".into()),
        time: NaiveDate::from_ymd_opt(2026, 10, 26)
            .and_then(|d| d.and_hms_opt(14, 3, 22))
            .unwrap(),
    }
}

fn context(data: &Path, crash_report: Option<std::path::PathBuf>) -> FeedbackContext {
    FeedbackContext {
        info: BuildInfo {
            version: planora_app::running_version(),
            os: "Windows 11 (Build 26100)".into(),
            gpu: Some(GpuInfo {
                name: "Testkarte 3000".into(),
                // Treiber unbekannt: der Ersatztext kommt aus den Sprachdateien.
                driver: String::new(),
                backend: "Vulkan".into(),
            }),
        },
        screen: Some((1920, 1080)),
        steps: diag::recent_steps(),
        log: diag::recent_lines(100),
        personal: Personal::default(),
        data_dir: data.to_path_buf(),
        from_crash: crash_report.is_some(),
        crash_report,
    }
}

#[test]
fn log_steps_crash_report_and_feedback_file_are_english_under_german() {
    with_lang(&german(), || {
        // Arbeit im Programm: Befehle, Rückgängig-Schritte, Speichern, Öffnen.
        let root = common::temp_root("planora-berichte-englisch");
        diag::set_verbose(true);
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        commands::run_action(&mut w, Action::New);
        w.active_tab_mut()
            .state
            .create_box(DVec3::splat(10.0))
            .unwrap();
        w.observe_history(false);
        commands::run_action(&mut w, Action::Undo);
        commands::run_action(&mut w, Action::Redo);
        let p = w.library.as_ref().unwrap().create_project("P").unwrap();
        let path = w.save_as(&p, "Teil", "").unwrap();
        w.close_tab(0);
        w.open_document(&path, planora_project::docstore::LockMode::Acquire)
            .unwrap();
        diag::set_verbose(false);

        let steps = diag::recent_steps();
        let log = diag::recent_lines(500);
        assert!(
            steps.iter().any(|s| s.label.contains("Create box")),
            "{steps:?}"
        );
        assert!(
            log.iter().any(|l| l.contains("DETAIL Command Undo:")),
            "{log:?}"
        );
        assert_no_german("letzte Schritte", &diag::steps_text(&steps));
        assert_no_german("Protokoll", &log.join("\n"));

        // Absturzbericht.
        let text = crash::report_text(&report_input(steps.clone(), log.clone()));
        assert_no_german("Absturzbericht", &text);
        assert!(text.contains("Last steps:") && text.contains("Document: Halter.pcad"));

        // rueckmeldung.txt mit allen Bestandteilen; die Beschreibung bleibt deutsch.
        let data = root.join("daten");
        let written = crash::write_report(
            &data.join(crash::CRASH_DIR),
            &report_input(steps, log),
            &crash::UserMask::none(),
        )
        .unwrap();
        let mut d = FeedbackDialog::default();
        d.open_fresh(context(&data, Some(written)));
        for p in d.shown_parts() {
            d.set_checked(p, true);
        }
        d.kind = Kind::Wunsch;
        d.area = Some("Tastenkürzel");
        let desc = "Übergröße „Halter“ lässt sich nicht öffnen.";
        d.description = desc.into();
        let full = feedback::full_text(&d.report());
        assert!(full.contains(desc), "{full}");
        assert!(full.contains("Kind: Wish\nArea: Shortcuts\n"), "{full}");
        assert!(full.contains("== Window picture ==\n"), "{full}");
        assert_no_german("rueckmeldung.txt", &full.replace(desc, ""));
        // Die Angaben sind auch in der Vorschau englisch (so, wie sie mitgehen).
        assert_no_german("Grafikkarte", &d.preview_text(Part::Grafik).unwrap());
        assert_eq!(
            d.preview_text(Part::System).unwrap(),
            "Windows 11 (Build 26100)\nScreen: 1920 × 1080"
        );
    });
}

/// Ein Bild mit dem Fenster.
fn frame_feedback(s: &mut Screen, d: &mut FeedbackDialog) -> Vec<common::Seen> {
    s.frame(vec![], &mut |ctx| {
        let _ = feedback_dialog::show(ctx, d, true);
    })
}

#[test]
fn feedback_window_speaks_english() {
    with_lang(&english(), || {
        let root = common::temp_root("planora-rueckmeldung-englisch");
        diag::step("Extrude sketch");
        diag::info("Start");
        // Mit Absturzbericht: Er geht nur als Datei mit, also entsteht der Ordner.
        let report = crash::write_report(
            &root.join(crash::CRASH_DIR),
            &report_input(vec![], vec![]),
            &crash::UserMask::none(),
        )
        .unwrap();
        let mut d = FeedbackDialog::default();
        d.open_fresh(context(&root, Some(report)));
        d.area = Some("Zeichnung");
        let mut s = Screen::new(1920.0, 1080.0);
        let seen = (0..3)
            .map(|_| frame_feedback(&mut s, &mut d))
            .last()
            .unwrap();
        assert_english("Rückmeldung (zu kurz)", &seen, &[], &[]);
        assert_has(
            "Rückmeldung",
            &seen,
            &[
                "Feedback",
                "What is it about?",
                "Drawing",
                "This goes along",
                "Last steps",
            ],
        );
        // Lang genug: abschicken; danach Hinweise mit Ordner.
        d.description = "Extrude does nothing at all".into();
        let seen = frame_feedback(&mut s, &mut d);
        let at = find(&seen, &feedback_dialog::open_browser())
            .expect("Knopf")
            .rect
            .center();
        s.click(at, &mut |ctx| {
            let _ = feedback_dialog::show(ctx, &mut d, true);
        });
        assert!(d.sent.is_some(), "abgeschickt");
        let seen = frame_feedback(&mut s, &mut d);
        assert_english("Rückmeldung (geschickt)", &seen, &[], &[]);
        assert!(
            seen.iter()
                .any(|t| t.text.starts_with("Drag “rueckmeldung.txt”")),
            "{}",
            common::join(&seen)
        );
        assert_has("Rückmeldung (geschickt)", &seen, &["Copy text", "Close"]);
    });
}

#[test]
fn feedback_hints_follow_the_language_when_drawn() {
    // Unter Deutsch geöffnet, unter Englisch gezeichnet: Hinweise im Fenster englisch.
    let root = common::temp_root("planora-rueckmeldung-wechsel");
    let mut d = with_lang(&german(), || {
        let mut d = FeedbackDialog::default();
        d.open_fresh(context(&root, None));
        d
    });
    assert_eq!(
        with_lang(&english(), || d.preview_text(Part::Bild)).unwrap(),
        with_lang(&english(), feedback_dialog::shot_not_yet)
    );
    with_lang(&english(), || {
        let mut s = Screen::new(1920.0, 1080.0);
        let seen = (0..3)
            .map(|_| frame_feedback(&mut s, &mut d))
            .last()
            .unwrap();
        assert_english("Rückmeldung", &seen, &[], &[]);
    });
}

fn frame_crash(s: &mut Screen, n: &mut CrashNotice) -> Vec<common::Seen> {
    s.frame(vec![], &mut |ctx| {
        let _ = crash_dialog::show(ctx, n, true);
    })
}

#[test]
fn crash_prompt_speaks_english_and_its_summary_follows_the_language() {
    let root = common::temp_root("planora-absturz-englisch");
    let dir = root.join(crash::CRASH_DIR);
    let a = crash::write_report(
        &dir,
        &report_input(vec![], vec![]),
        &crash::UserMask::none(),
    )
    .unwrap();
    // Ein älterer, deutscher Bericht daneben.
    let old = dir.join("absturz-2026-10-01_10-00-00.txt");
    std::fs::write(&old, "Zeit: 01.10.2026, 10:00:00\n\nVersion: 0.1.0\n").unwrap();
    // Unter Deutsch angelegt …
    let mut n = with_lang(&german(), || {
        CrashNotice::from_finding(&StartFinding::Crashed(vec![a, old]), dir.clone()).unwrap()
    });
    n.open = true;
    let mut s = Screen::new(1920.0, 1080.0);
    let seen = (0..3).map(|_| frame_crash(&mut s, &mut n)).last().unwrap();
    assert_has(
        "Rückfrage deutsch",
        &seen,
        &["Bericht vom 26.10.2026, 14:03:22, Version 0.2.0-dev.1 (dazu 1 ältere)"],
    );
    // … unter Englisch gezeichnet: alles englisch, auch die Zeile mit Zeit und Version.
    with_lang(&english(), || {
        let seen = (0..3).map(|_| frame_crash(&mut s, &mut n)).last().unwrap();
        assert_english("Absturz-Rückfrage", &seen, &[], &[]);
        assert_has(
            "Absturz-Rückfrage",
            &seen,
            &[
                "PlanoraCAD crashed last time",
                "Report from 2026-10-26 14:03:22, version 0.2.0-dev.1 (plus 1 older)",
                "View report",
                "Open as feedback",
                "Not now",
            ],
        );
        // Bericht ansehen: der Bericht selbst ist englisch.
        n.viewing = true;
        let seen = (0..3).map(|_| frame_crash(&mut s, &mut n)).last().unwrap();
        assert_english("Absturz-Rückfrage (Bericht)", &seen, &[], &[]);
        assert_has("Absturz-Rückfrage (Bericht)", &seen, &["Hide report"]);
        // Ende ohne Bericht.
        let mut u = CrashNotice::from_finding(&StartFinding::Unexpected, dir.clone()).unwrap();
        u.open = true;
        let seen = (0..3).map(|_| frame_crash(&mut s, &mut u)).last().unwrap();
        assert_english("Unerwartetes Ende", &seen, &[], &[]);
        assert_has(
            "Unerwartetes Ende",
            &seen,
            &["PlanoraCAD closed unexpectedly last time"],
        );
    });
}
