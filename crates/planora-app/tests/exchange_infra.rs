//! Arbeitsthreads, eigene Dateiauswahl und gleich hohe Karten (Teilprojekt 9, Task 15).

use planora_app::cards::card_row;
use planora_app::file_chooser::*;
use planora_app::fonts;
use planora_app::jobs::{Job, Poll};
use planora_exchange::{ExchangeError, Progress};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier, mpsc};

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-infra-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_job_delivers_its_result() {
    let job = Job::spawn(|p| {
        p.set_total(4);
        p.advance(4);
        42
    });
    assert_eq!(job.wait(), Poll::Ready(42));
}

#[test]
fn a_cancelled_job_sees_the_cancel_flag() {
    let progress = Arc::new(Progress::new());
    progress.cancel();
    let job = Job::spawn_with(progress, |p| p.check());
    assert_eq!(job.wait(), Poll::Ready(Err(ExchangeError::Cancelled)));
}

#[test]
fn a_panicking_job_is_lost_not_a_crash() {
    let job: Job<u8> = Job::spawn(|_| panic!("Absicht: Test für einen abgestürzten Arbeitsthread"));
    assert_eq!(job.wait(), Poll::Lost);
}

#[test]
fn dropping_a_job_cancels_it() {
    let (tx, rx) = mpsc::channel();
    let gate = Arc::new(Barrier::new(2));
    let g = Arc::clone(&gate);
    let job = Job::spawn(move |p: &Progress| {
        g.wait();
        g.wait();
        let _ = tx.send(p.is_cancelled());
    });
    gate.wait();
    drop(job);
    gate.wait();
    assert!(rx.recv().unwrap());
}

#[test]
fn the_chooser_lists_folders_first_and_only_matching_files() {
    let d = temp_dir("liste");
    fs::create_dir_all(d.join("Unterordner")).unwrap();
    for f in ["b.STL", "a.obj", "c.txt", "z.stl"] {
        fs::write(d.join(f), b"x").unwrap();
    }
    let c = FileChooser::open(d.clone(), vec!["stl", "obj"]);
    let names: Vec<(&str, bool)> = c
        .entries()
        .iter()
        .map(|e| (e.name.as_str(), e.is_dir))
        .collect();
    assert_eq!(
        names,
        [
            ("Unterordner", true),
            ("a.obj", false),
            ("b.STL", false),
            ("z.stl", false)
        ]
    );
    assert!(!c.truncated && c.error.is_none());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn saving_adds_the_ending_and_asks_before_replacing() {
    let d = temp_dir("speichern");
    let mut c = FileChooser::save(d.clone(), "Teil".into(), "3mf");
    assert_eq!(c.confirm(), ChooserAction::Chosen(d.join("Teil.3mf")));
    fs::write(d.join("Teil.3mf"), b"alt").unwrap();
    assert_eq!(c.confirm(), ChooserAction::None);
    assert!(c.asks_to_replace());
    assert_eq!(c.confirm(), ChooserAction::Chosen(d.join("Teil.3mf")));
    c.file_name = "Teil.3mf".into();
    assert_eq!(c.target(), Some(d.join("Teil.3mf")), "Endung nicht doppelt");
    c.file_name = "  ".into();
    assert_eq!(c.confirm(), ChooserAction::None);
    assert!(c.error.is_some());
    c.file_name = "../weg".into();
    assert_eq!(c.target(), None);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn navigation_moves_between_folders() {
    let d = temp_dir("wandern");
    fs::create_dir_all(d.join("innen")).unwrap();
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    c.enter("innen");
    assert_eq!(c.dir(), d.join("innen").as_path());
    c.up();
    assert_eq!(c.dir(), d.as_path());
    c.dir_text = d.join("gibt-es-nicht").display().to_string();
    assert!(!c.go_to_text());
    assert_eq!(c.dir(), d.as_path());
    assert!(c.error.is_some());
    fs::write(d.join("x.stl"), b"x").unwrap();
    c.reload();
    c.select("x.stl");
    assert_eq!(c.confirm(), ChooserAction::Chosen(d.join("x.stl")));
    c.select("fehlt.stl");
    assert_eq!(c.confirm(), ChooserAction::None);
    assert!(default_dir().is_dir() || default_dir().as_os_str().is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_chooser_draws_without_input() {
    let d = temp_dir("zeichnen");
    let mut c = FileChooser::save(d.clone(), "Teil".into(), "stl");
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    for _ in 0..2 {
        let mut action = ChooserAction::None;
        ctx.run_ui(egui::RawInput::default(), |ui| action = c.show(ui))
            .drop_without_applying_deltas();
        assert_eq!(action, ChooserAction::None);
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn cards_in_a_row_get_the_same_height_over_the_full_width() {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 600.0),
        )),
        ..Default::default()
    };
    let mut rects = Vec::new();
    for _ in 0..3 {
        ctx.run_ui(input(), |ui| {
            let mut a = |ui: &mut egui::Ui| {
                ui.label("Eine Zeile");
            };
            let mut b = |ui: &mut egui::Ui| {
                for k in 0..5 {
                    ui.label(format!("Zeile {k}"));
                }
            };
            let row: &mut [&mut dyn FnMut(&mut egui::Ui)] = &mut [&mut a, &mut b];
            rects = card_row(ui, "test", row);
        })
        .drop_without_applying_deltas();
    }
    assert_eq!(rects.len(), 2);
    assert!(
        (rects[0].height() - rects[1].height()).abs() < 0.5,
        "{rects:?}"
    );
    assert!(rects[1].min.x > rects[0].max.x, "nebeneinander");
    assert!(
        rects[1].max.x - rects[0].min.x > 800.0,
        "über die ganze Breite: {rects:?}"
    );
}

fn click(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn frame(
    ctx: &egui::Context,
    c: &mut FileChooser,
    events: Vec<egui::Event>,
) -> (ChooserAction, egui::Pos2) {
    let mut action = ChooserAction::None;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 600.0),
        )),
        events,
        time: Some({
            static T: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
            f64::from(T.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) * 0.01
        }),
        ..Default::default()
    };
    ctx.run_ui(input, |ui| action = c.show(ui))
        .drop_without_applying_deltas();
    (action, egui::Pos2::ZERO)
}

#[test]
fn a_double_click_on_a_folder_enters_it_without_choosing_a_file_inside() {
    let d = temp_dir("doppelklick");
    fs::create_dir_all(d.join("A")).unwrap();
    fs::write(d.join("A").join("inner.stl"), b"x").unwrap();
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    frame(&ctx, &mut c, vec![]);
    frame(&ctx, &mut c, vec![]);
    // Erste Listenzeile liegt unter Ordnerleiste; Zeile suchen.
    let pos = egui::pos2(60.0, 0.0);
    let mut chosen = ChooserAction::None;
    let mut entered = false;
    {
        let y = 31.0_f32;
        // Zeile des Ordners „A“ liegt bei y = 22..40.
        let mut c2 = FileChooser::open(d.clone(), vec!["stl"]);
        frame(&ctx, &mut c2, vec![]);
        let p = egui::pos2(pos.x, y);
        frame(&ctx, &mut c2, vec![egui::Event::PointerMoved(p)]);
        let evs = vec![
            click(p, true),
            click(p, false),
            click(p, true),
            click(p, false),
        ];
        let (a, _) = frame(&ctx, &mut c2, evs);
        let (b, _) = frame(&ctx, &mut c2, vec![]);
        if matches!(a, ChooserAction::Chosen(_)) || matches!(b, ChooserAction::Chosen(_)) {
            chosen = ChooserAction::Chosen(PathBuf::new());
        }
        if c2.dir() == d.join("A").as_path() {
            entered = true;
        }
    }
    assert_eq!(
        chosen,
        ChooserAction::None,
        "keine Datei im neuen Ordner gewählt"
    );
    assert!(entered, "Doppelklick öffnet den Ordner");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn file_names_with_forbidden_characters_are_rejected() {
    let d = temp_dir("namen");
    let mut c = FileChooser::save(d.clone(), "x".into(), "stl");
    for bad in ["C:boese", "a<b", "a|b", "q?", "a*b", "..", "."] {
        c.file_name = bad.into();
        assert_eq!(c.target(), None, "{bad}");
    }
    c.file_name = "Teil.".into();
    assert_eq!(c.target(), Some(d.join("Teil.stl")));
    c.file_name = "a<b".into();
    assert_eq!(c.confirm(), ChooserAction::None);
    assert!(c.error.is_some());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn relative_folders_become_absolute_and_errors_are_plain_german() {
    let mut c = FileChooser::open(default_dir(), vec!["stl"]);
    c.dir_text = ".".into();
    assert!(c.go_to_text());
    assert!(c.dir().is_absolute());
    c.up();
    assert!(c.error.is_none() && !c.dir().as_os_str().is_empty());
    c.dir_text = std::env::temp_dir()
        .join("planora-gibt-es-nicht-x")
        .display()
        .to_string();
    assert!(!c.go_to_text());
    let msg = c.error.clone().unwrap();
    assert!(!msg.contains("os error"), "{msg}");
    let c2 = FileChooser::open(
        std::env::temp_dir().join("planora-gibt-es-nicht-y"),
        vec!["stl"],
    );
    assert!(!c2.error.clone().unwrap_or_default().contains("os error"));
}
