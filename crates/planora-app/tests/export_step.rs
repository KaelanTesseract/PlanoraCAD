//! Fenster „Exportieren“ mit STEP (Teilprojekt 10, Task 13): fünftes Format, Karte statt
//! Qualität, Körper und Flächen, Netz-Körper ausgelassen, abgelehnte Körper, Schreiben mit
//! Hilfsdatei, veraltete Ergebnisse, Abbrechen, volle Breite und gleich hohe Karten.

use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_exchange::{ExchangeMesh, Format, Progress};
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::DVec3;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-step-ui-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn wide() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    }
}

/// Zeichnet das Fenster dreimal und liefert die Texte des letzten Bildes.
fn draw(d: &mut ExportDialog, s: &mut AppState) -> String {
    let ctx = context();
    let mut dir = None;
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show(ui.ctx(), s, &mut dir));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

/// Quader, Kugel und ein Netz-Körper „Würfel“; nichts gewählt.
fn mixed_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_sphere(5.0).unwrap();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    s.selected_body = None;
    s
}

fn step_dialog(s: &AppState) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.format = ExportFormat::Step;
    d.tick(s, true);
    d
}

#[test]
fn step_is_the_fifth_format_with_its_own_extension() {
    assert_eq!(ExportFormat::ALL.len(), 5);
    assert_eq!(ExportFormat::ALL[4], ExportFormat::Step);
    assert_eq!(ExportFormat::Step.extension(), "step");
    assert_eq!(ExportFormat::Step.label(), STEP_LABEL);
    assert_eq!(STEP_LABEL, "STEP – für andere CAD-Programme (exakt)");
    assert_eq!(ExportFormat::Mesh(Format::Stl).extension(), "stl");
    assert!(ExportFormat::Step.is_step() && !ExportFormat::Mesh(Format::Obj).is_step());
}

#[test]
fn step_counts_bodies_and_faces_and_names_the_mesh_bodies() {
    let s = mixed_state();
    let d = step_dialog(&s);
    assert_eq!(
        d.status(&s),
        ExportStatus::StepReady {
            bodies: 2,
            faces: 7,
            rejected: vec![],
            skipped_meshes: vec!["Würfel".into()],
        }
    );
}

#[test]
fn the_window_shows_step_texts_without_the_quality_card() {
    let mut s = mixed_state();
    let mut d = step_dialog(&s);
    let text = draw(&mut d, &mut s);
    for word in [
        STEP_LABEL,
        STEP_EXACT,
        "2 Körper, 7 Flächen",
        "Netz-Körper werden als STEP nicht ausgegeben: „Würfel“",
        "STL – für den 3D-Druck",
        "Umfang",
        "Exportieren …",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    for word in ["Qualität", "Grob", "Dreiecke"] {
        assert!(
            !text.contains(word),
            "„{word}“ gehört nicht zu STEP:\n{text}"
        );
    }
    let rect = d.last_rect.unwrap();
    assert!(
        rect.width() >= 1200.0 - 2.0 * DIALOG_MARGIN - 1.0,
        "{rect:?}"
    );
    assert_eq!(d.last_cards.len(), 3);
    assert!(
        d.last_cards
            .windows(2)
            .all(|w| (w[0].height() - w[1].height()).abs() < 0.5),
        "{:?}",
        d.last_cards
    );
}

#[test]
fn writing_step_goes_to_the_chosen_file() {
    let s = mixed_state();
    let dir = temp_dir("schreiben");
    let mut d = step_dialog(&s);
    d.begin_save(dir.clone(), &s);
    assert_eq!(d.chooser().unwrap().target(), Some(dir.join("Körper.step")));
    let path = dir.join("Körper.step");
    d.write_to(path.clone(), &s);
    d.tick(&s, true);
    let bytes = match d.write_state() {
        WriteState::Done(p, n) => {
            assert_eq!(p, path);
            n
        }
        other => panic!("{other:?}"),
    };
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(bytes, text.len() as u64);
    assert!(text.starts_with("ISO-10303-21;"));
    assert!(text.contains("FILE_NAME('K\\X2\\00F6\\X0\\rper.step'"));
    assert_eq!(text.matches("PRODUCT('").count(), 2);
    assert!(text.contains("PRODUCT('Quader 1','Quader 1'"));
    assert!(
        !text.contains("W\\X2\\00FC\\X0\\rfel"),
        "Netz-Körper nicht in der Datei"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_stale_step_result_is_never_written() {
    let mut s = mixed_state();
    s.selected_body = Some(0);
    let dir = temp_dir("veraltet");
    let mut d = step_dialog(&s);
    assert_eq!(d.scope, Scope::Selected);
    s.selected_body = Some(1);
    let path = dir.join("x.step");
    d.write_to(path.clone(), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_mesh_bodies_leave_nothing_to_write() {
    let mut s = AppState::new();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    let mut d = step_dialog(&s);
    assert!(matches!(
        d.status(&s),
        ExportStatus::StepReady { bodies: 0, .. }
    ));
    let text = draw(&mut d, &mut s);
    assert!(text.contains(STEP_NOTHING), "{text}");
    let dir = temp_dir("nichts");
    d.write_to(dir.join("x.step"), &s);
    assert!(matches!(d.write_state(), WriteState::Failed(m) if m.contains("keinen Körper")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rejected_bodies_are_listed_with_their_reason() {
    let mut open = AnalyticKernel::default()
        .make_box(DVec3::splat(1.0))
        .unwrap();
    let f = open.faces.iter().next().unwrap().0;
    open.faces[f].outer.half_edges.pop();
    let good = AnalyticKernel::default()
        .make_box(DVec3::splat(1.0))
        .unwrap();
    let items = vec![
        ExportItem {
            name: "Offen".into(),
            source: ExportSource::Solid(open),
        },
        ExportItem {
            name: "Gut".into(),
            source: ExportSource::Solid(good),
        },
    ];
    let p = prepare_step(&items, &Progress::new()).unwrap();
    assert_eq!(p.bodies.len(), 1);
    assert_eq!(p.faces, 6);
    assert_eq!(p.rejected.len(), 1);
    assert!(
        p.rejected[0].starts_with("„Offen“ wird nicht ausgegeben: "),
        "{}",
        p.rejected[0]
    );
    let cancelled = Progress::new();
    cancelled.cancel();
    assert_eq!(prepare_step(&items, &cancelled).unwrap_err(), "Abgebrochen");
}

#[test]
fn cancelling_step_writing_keeps_the_old_file() {
    let s = mixed_state();
    let items = step_dialog(&s).items(&s);
    let prepared = prepare_step(&items, &Progress::new()).unwrap();
    let dir = temp_dir("abbrechen");
    let path = dir.join("alt.step");
    fs::write(&path, "alt").unwrap();
    let p = Progress::new();
    p.cancel();
    assert_eq!(
        write_step_bodies(&path, &prepared, &p)
            .unwrap_err()
            .to_string(),
        "Abgebrochen"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "alt");
    let n = write_step_bodies(&path, &prepared, &Progress::new()).unwrap();
    assert_eq!(n, fs::metadata(&path).unwrap().len());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_the_step_check_says_so_and_switching_back_keeps_the_mesh_result() {
    let s = mixed_state();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
    d.format = ExportFormat::Step;
    d.tick(&s, false);
    d.cancel();
    assert_eq!(d.status(&s), ExportStatus::Failed("Abgebrochen".into()));
    d.format = ExportFormat::Mesh(Format::ThreeMf);
    assert!(
        matches!(d.status(&s), ExportStatus::Ready { .. }),
        "das Netz bleibt gültig"
    );
    d.format = ExportFormat::Step;
    d.recompute();
    d.tick(&s, true);
    assert!(matches!(
        d.status(&s),
        ExportStatus::StepReady { bodies: 2, .. }
    ));
}

#[test]
fn closing_is_refused_while_step_is_written_and_the_file_is_finished() {
    let s = mixed_state();
    let dir = temp_dir("schliessen");
    let mut d = step_dialog(&s);
    let path = dir.join("k.step");
    d.write_to(path.clone(), &s);
    assert!(d.is_writing_step());
    assert!(!d.request_close(), "Schließen muss abgewiesen werden");
    assert_eq!(d.write_state(), WriteState::Writing);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Done(..)));
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .starts_with("ISO-10303-21;")
    );
    assert!(d.request_close());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_window_stays_open_with_a_hint_while_step_is_written() {
    // Viele Körper, damit der Arbeitsthread beim Zeichnen sicher noch läuft.
    let mut s = AppState::new();
    for _ in 0..300 {
        s.create_box(DVec3::splat(3.0)).unwrap();
    }
    s.selected_body = None;
    let dir = temp_dir("hinweis");
    let path = dir.join("k.step");
    let mut d = step_dialog(&s);
    d.write_to(path.clone(), &s);
    assert!(!d.request_close());
    let ctx = context();
    let mut last = None;
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show(ui.ctx(), &mut s, &mut last), "Fenster bleibt offen");
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    assert!(d.is_writing_step(), "das Schreiben muss noch laufen");
    assert!(text.contains(WRITING_HINT), "{text}");
    d.cancel_write();
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed("Abgebrochen".into()));
    assert!(!path.exists());
    assert!(d.request_close());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_while_writing_says_so_and_keeps_the_old_file() {
    let mut s = AppState::new();
    for _ in 0..200 {
        s.create_box(DVec3::splat(3.0)).unwrap();
    }
    s.selected_body = None;
    let dir = temp_dir("abbruch-fenster");
    let path = dir.join("alt.step");
    fs::write(&path, "alt").unwrap();
    let mut d = step_dialog(&s);
    d.write_to(path.clone(), &s);
    d.cancel_write();
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed("Abgebrochen".into()));
    assert_eq!(fs::read_to_string(&path).unwrap(), "alt");
    let _ = fs::remove_dir_all(&dir);
}

/// Zahl der Hilfsdateien („.…planora-tmp“) im Ordner.
fn temp_files(dir: &std::path::Path) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with("planora-tmp")
        })
        .count()
}

/// Öffnet im aktiven Tab ein STEP-Schreiben mit `boxes` Quadern und holt es nie ab.
fn start_writing_in_active_tab(
    w: &mut planora_app::workspace::Workspace,
    boxes: usize,
    to: &std::path::Path,
) {
    let tab = w.active_tab_mut();
    for _ in 0..boxes {
        tab.state.create_box(DVec3::splat(3.0)).unwrap();
    }
    tab.state.selected_body = None;
    let mut d = step_dialog(&tab.state);
    d.write_to(to.to_path_buf(), &tab.state);
    tab.export_dialog = Some(d);
}

#[test]
fn closing_tabs_and_quitting_work_with_an_unfetched_step_write_in_a_background_tab() {
    use planora_app::workspace::Workspace;
    use planora_project::config::Config;
    let mut w = Workspace::new(None, Config::default());
    let dir = temp_dir("tab");
    let target = dir.join("t.step");
    start_writing_in_active_tab(&mut w, 1, &target);
    // Das Schreiben ist längst fertig, wurde aber nie abgeholt (Hintergrund-Tab ohne `tick`).
    std::thread::sleep(std::time::Duration::from_millis(1500));
    w.new_tab();
    // Der erste Tab ist geändert: es gibt die gewohnte Rückfrage, keine Sperre.
    assert!(!w.request_quit());
    assert!(w.prompt.is_some(), "die übliche Rückfrage „Speichern?“");
    w.discard_and_continue();
    assert!(w.quit, "Beenden geht trotz nicht abgeholtem Schreiben");
    assert!(target.exists());
    assert_eq!(temp_files(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn closing_a_tab_during_a_running_step_write_cancels_it_and_leaves_no_helper_file() {
    use planora_app::workspace::Workspace;
    use planora_project::config::Config;
    let mut w = Workspace::new(None, Config::default());
    let dir = temp_dir("tab-laeuft");
    let target = dir.join("alt.step");
    fs::write(&target, "alt").unwrap();
    start_writing_in_active_tab(&mut w, 300, &target);
    assert!(!w.request_close(0));
    w.discard_and_continue();
    assert_eq!(w.tabs.len(), 1, "der Tab ist weg, ein frischer ersetzt ihn");
    assert!(w.tabs[0].export_dialog.is_none());
    // Der abgebrochene Arbeitsthread räumt auf: keine Hilfsdatei, die Zieldatei ist ganz.
    let start = std::time::Instant::now();
    while temp_files(&dir) > 0 && start.elapsed().as_secs() < 10 {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(temp_files(&dir), 0);
    let text = fs::read_to_string(&target).unwrap();
    assert!(text == "alt" || text.starts_with("ISO-10303-21;"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_body_name_becomes_a_numbered_body_in_messages() {
    let mut open = AnalyticKernel::default()
        .make_box(DVec3::splat(1.0))
        .unwrap();
    let f = open.faces.iter().next().unwrap().0;
    open.faces[f].outer.half_edges.pop();
    let items = vec![
        ExportItem {
            name: "  ".into(),
            source: ExportSource::Solid(open),
        },
        ExportItem {
            name: "".into(),
            source: ExportSource::Mesh(
                Arc::new(cube()),
                planora_exchange::MeshTransform::default(),
            ),
        },
    ];
    let p = prepare_step(&items, &Progress::new()).unwrap();
    assert!(p.rejected[0].starts_with("„Körper 1“ wird nicht ausgegeben: "));
    assert_eq!(p.skipped_meshes, vec!["Körper 2".to_string()]);
}
