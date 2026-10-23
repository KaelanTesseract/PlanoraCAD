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
            assert!(d.show_in(
                ui.ctx(),
                s,
                &mut dir,
                &mut planora_app::places::Places::default()
            ));
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
    // Mechanisch angepasst (Teilprojekt 11, Aufgabe 17; 3c-3: PDF): drei Zeichnungsformate dahinter.
    assert_eq!(ExportFormat::ALL.len(), 8);
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
        (rect.width() - 720.0).abs() <= 1.0 && (rect.center().x - 600.0).abs() <= 1.0,
        "720 breit und mittig: {rect:?}"
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

/// Hält das STEP-Schreiben an, bis abgebrochen wird, und schreibt dann mit dem echten
/// Schreiber weiter (der den Abbruch meldet). Ohne Abbruch nach 30 s ebenfalls echt.
fn held_step_writer(
    path: &std::path::Path,
    prepared: &StepPrepared,
    p: &Progress,
) -> Result<u64, planora_step::StepError> {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !p.is_cancelled() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let r = write_step_bodies(path, prepared, p);
    HELD_DONE.lock().unwrap().push(path.to_path_buf());
    r
}

/// Ziele, deren angehaltenes Schreiben ([`held_step_writer`]) zu Ende ist (auch abgebrochen).
static HELD_DONE: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

fn held_write_finished(path: &std::path::Path) -> bool {
    HELD_DONE.lock().unwrap().iter().any(|p| p == path)
}

/// Drei Quader, nichts gewählt (für Tests, deren Schreiben über [`held_step_writer`] läuft).
fn small_state() -> AppState {
    let mut s = AppState::new();
    for _ in 0..3 {
        s.create_box(DVec3::splat(3.0)).unwrap();
    }
    s.selected_body = None;
    s
}

#[test]
fn the_window_stays_open_with_a_hint_while_step_is_written() {
    // Das Schreiben wartet auf den Abbruch; früher entschieden 300 Quader und die Dauer der
    // ersten Bilder, ob es beim Zeichnen noch lief (unzuverlässig).
    let mut s = small_state();
    let dir = temp_dir("hinweis");
    let path = dir.join("k.step");
    let mut d = step_dialog(&s);
    d.set_step_writer(held_step_writer);
    d.write_to(path.clone(), &s);
    assert!(!d.request_close());
    let ctx = context();
    let mut last = None;
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(
                d.show_in(
                    ui.ctx(),
                    &mut s,
                    &mut last,
                    &mut planora_app::places::Places::default()
                ),
                "Fenster bleibt offen"
            );
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
    // Das Schreiben wartet auf den Abbruch (früher 200 Quader: ein Wettlauf zwischen Schreiben
    // und Abbrechen).
    let s = small_state();
    let dir = temp_dir("abbruch-fenster");
    let path = dir.join("alt.step");
    fs::write(&path, "alt").unwrap();
    let mut d = step_dialog(&s);
    d.set_step_writer(held_step_writer);
    d.write_to(path.clone(), &s);
    assert!(d.is_writing_step());
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

/// Öffnet im aktiven Tab ein STEP-Schreiben mit `boxes` Quadern und holt es nie ab. Mit `hold`
/// wartet das Schreiben auf den Abbruch ([`held_step_writer`]).
fn start_writing_in_active_tab(
    w: &mut planora_app::workspace::Workspace,
    boxes: usize,
    to: &std::path::Path,
    hold: bool,
) {
    let tab = w.active_tab_mut();
    for _ in 0..boxes {
        tab.state.create_box(DVec3::splat(3.0)).unwrap();
    }
    tab.state.selected_body = None;
    let mut d = step_dialog(&tab.state);
    if hold {
        d.set_step_writer(held_step_writer);
    }
    d.write_to(to.to_path_buf(), &tab.state);
    tab.export_dialog = Some(d);
}

/// Wartet (höchstens 30 s), bis `done` stimmt.
fn wait_until(mut done: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !done() && start.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn closing_tabs_and_quitting_work_with_an_unfetched_step_write_in_a_background_tab() {
    use planora_app::workspace::Workspace;
    use planora_project::config::Config;
    let mut w = Workspace::new(None, Config::default());
    let dir = temp_dir("tab");
    let target = dir.join("t.step");
    start_writing_in_active_tab(&mut w, 1, &target, false);
    // Das Schreiben ist fertig, wurde aber nie abgeholt (Hintergrund-Tab ohne `tick`). Warten,
    // bis die Datei da ist, statt einer festen Pause (1,5 s reichten auf einem belasteten
    // Rechner nicht sicher).
    wait_until(|| target.exists() && temp_files(&dir) == 0);
    assert!(target.exists(), "das Schreiben endet");
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
    // Das Schreiben wartet auf den Abbruch und läuft deshalb sicher noch (früher 300 Quader:
    // dann war nur „vermutlich noch nicht fertig“).
    start_writing_in_active_tab(&mut w, 3, &target, true);
    assert!(w.tabs[0].export_dialog.as_ref().unwrap().is_writing_step());
    assert!(!w.request_close(0));
    w.discard_and_continue();
    assert_eq!(w.tabs.len(), 1, "der Tab ist weg, ein frischer ersetzt ihn");
    assert!(w.tabs[0].export_dialog.is_none());
    // Der abgebrochene Arbeitsthread räumt auf: keine Hilfsdatei, die alte Datei bleibt.
    wait_until(|| held_write_finished(&target));
    assert!(
        held_write_finished(&target),
        "das Schreiben endet nach dem Abbruch"
    );
    assert_eq!(temp_files(&dir), 0);
    assert_eq!(fs::read_to_string(&target).unwrap(), "alt");
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

/// Viele Quader (eine längere Vernetzung; der Test darunter hängt nicht an ihrer Dauer).
fn big_state() -> AppState {
    let mut s = AppState::new();
    for _ in 0..300 {
        s.create_box(DVec3::splat(3.0)).unwrap();
    }
    s.selected_body = None;
    s
}

/// Zeichnet `n` Bilder und liefert die Texte des letzten.
fn draw_frames(d: &mut ExportDialog, s: &mut AppState, n: usize) -> String {
    let ctx = context();
    let mut last = None;
    let mut text = String::new();
    for _ in 0..n {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show_in(
                ui.ctx(),
                s,
                &mut last,
                &mut planora_app::places::Places::default()
            ));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

#[test]
fn the_cancel_button_belongs_to_the_running_write_not_to_the_chosen_format() {
    // Das Schreiben wartet auf den Abbruch; mit 300 Quadern und echtem Schreiber entschied die
    // Dauer der ersten Bilder, ob es beim Zeichnen noch lief.
    let mut s = small_state();
    let dir = temp_dir("abbrechen-format");
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    d.format = ExportFormat::Step;
    d.tick(&s, true);
    d.set_step_writer(held_step_writer);
    d.write_to(dir.join("k.step"), &s);
    d.format = ExportFormat::Mesh(Format::Stl);
    let text = draw_frames(&mut d, &mut s, 2);
    assert!(d.is_writing_step(), "das Schreiben muss noch laufen");
    assert!(text.contains("Die Datei wird geschrieben"), "{text}");
    assert!(text.contains("Abbrechen"), "{text}");
    d.cancel_write();
    d.tick(&s, true);
    d.format = ExportFormat::Step;
    assert_eq!(d.write_state(), WriteState::Failed("Abgebrochen".into()));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_result_arriving_after_a_format_change_is_not_shown_in_the_other_format() {
    let mut s = mixed_state();
    let dir = temp_dir("anderes-format");
    let mut d = step_dialog(&s);
    d.write_to(dir.join("k.step"), &s);
    d.format = ExportFormat::Mesh(Format::Stl);
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Idle);
    let text = draw(&mut d, &mut s);
    assert!(!text.contains("Fertig"), "{text}");
    d.format = ExportFormat::Step;
    assert!(matches!(d.write_state(), WriteState::Done(..)));
    let _ = fs::remove_dir_all(&dir);
}

/// Treiber: ein Fenster, steigende Zeit; liefert die Texte und die Mitte des Textes `find`.
struct Clicker {
    ctx: egui::Context,
    time: f64,
}

impl Clicker {
    fn frame(
        &mut self,
        d: &mut ExportDialog,
        s: &mut AppState,
        events: Vec<egui::Event>,
        find: &str,
    ) -> (String, Option<egui::Pos2>) {
        self.time += 0.05;
        let mut input = wide();
        input.events = events;
        input.time = Some(self.time);
        let mut dir = None;
        let out = self.ctx.run_ui(input, |ui| {
            d.show_in(
                ui.ctx(),
                s,
                &mut dir,
                &mut planora_app::places::Places::default(),
            );
        });
        let mut found = None;
        fn walk(sh: &egui::epaint::Shape, find: &str, found: &mut Option<egui::Pos2>) {
            match sh {
                egui::epaint::Shape::Text(t) if t.galley.text().contains(find) => {
                    *found = Some(t.pos + egui::vec2(8.0, t.galley.size().y / 2.0));
                }
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|x| walk(x, find, found)),
                _ => {}
            }
        }
        for c in &out.shapes {
            walk(&c.shape, find, &mut found);
        }
        let text = collect_texts(&out);
        out.drop_without_applying_deltas();
        (text, found)
    }

    /// Klickt auf den Text `find` und liefert die Texte des letzten Bildes.
    fn click(&mut self, d: &mut ExportDialog, s: &mut AppState, find: &str) -> String {
        self.frame(d, s, vec![], find);
        let (_, pos) = self.frame(d, s, vec![], find);
        let p = pos.unwrap_or_else(|| panic!("„{find}“ nicht sichtbar"));
        let button = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for ev in [egui::Event::PointerMoved(p), button(true), button(false)] {
            self.frame(d, s, vec![ev], find);
        }
        self.frame(d, s, vec![], find);
        self.frame(d, s, vec![], find).0
    }
}

#[test]
fn clicking_another_format_removes_the_done_line() {
    let mut s = mixed_state();
    let dir = temp_dir("klick");
    let mut d = step_dialog(&s);
    d.write_to(dir.join("k.step"), &s);
    d.tick(&s, true);
    let mut c = Clicker {
        ctx: context(),
        time: 0.0,
    };
    c.frame(&mut d, &mut s, vec![], "x");
    let text = c.frame(&mut d, &mut s, vec![], "x").0;
    assert!(text.contains("Fertig"), "{text}");
    let text = c.click(&mut d, &mut s, "STL – für den 3D-Druck");
    assert_eq!(d.format, ExportFormat::Mesh(Format::Stl));
    assert!(!text.contains("Fertig"), "{text}");
    let text = c.click(&mut d, &mut s, STEP_LABEL);
    assert_eq!(d.format, ExportFormat::Step);
    assert!(
        !text.contains("Fertig"),
        "die Meldung gehört nicht mehr dazu:\n{text}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_and_recomputing_in_step_leave_the_mesh_result_alone() {
    let s = big_state();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, false);
    d.format = ExportFormat::Step;
    d.cancel();
    d.format = ExportFormat::Mesh(Format::Stl);
    d.tick(&s, true);
    assert!(
        matches!(d.status(&s), ExportStatus::Ready { .. }),
        "{:?}",
        d.status(&s)
    );
    d.format = ExportFormat::Step;
    d.recompute();
    d.format = ExportFormat::Mesh(Format::Stl);
    assert!(
        matches!(d.status(&s), ExportStatus::Ready { .. }),
        "das Netz rechnet nicht neu"
    );
}

#[test]
fn a_typed_stp_ending_becomes_step() {
    let s = mixed_state();
    let dir = temp_dir("stp");
    let mut d = step_dialog(&s);
    d.write_to(dir.join("x.STP"), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Done(p, _) if p == dir.join("x.step")));
    assert!(dir.join("x.step").exists());
    assert!(!dir.join("x.STP.step").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_stp_name_asks_before_replacing_the_existing_step_file() {
    let s = mixed_state();
    let dir = temp_dir("stp-ersetzen");
    fs::write(dir.join("x.step"), "alt").unwrap();
    let mut d = step_dialog(&s);
    d.begin_save(dir.clone(), &s);
    d.accept_chosen(dir.join("x.stp"), &s);
    assert!(d.chooser().is_some_and(|c| c.asks_to_replace()));
    assert_eq!(d.write_state(), WriteState::Idle);
    assert_eq!(fs::read_to_string(dir.join("x.step")).unwrap(), "alt");
    d.accept_chosen(dir.join("x.step"), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Done(..)));
    assert!(d.chooser().is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_disabled_export_button_says_why_when_nothing_can_be_written() {
    let mut s = AppState::new();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    let d = step_dialog(&s);
    assert_eq!(disabled_hint(&d.status(&s)), STEP_NOTHING);
    assert_eq!(
        disabled_hint(&ExportStatus::Computing(0.5)),
        "Erst möglich, wenn die Berechnung fertig ist."
    );
}

#[test]
fn long_names_are_shortened_in_lists_and_reported_after_writing() {
    let long = "A".repeat(300);
    let mut s = mixed_state();
    s.bodies[0].name = long.clone();
    let dir = temp_dir("lang");
    let mut d = step_dialog(&s);
    assert_eq!(d.shortened_names(&s), vec![format!("{}…", "A".repeat(40))]);
    d.write_to(dir.join("k.step"), &s);
    d.tick(&s, true);
    let text = draw(&mut d, &mut s);
    assert!(
        text.contains("Name gekürzt (höchstens 256 Zeichen in der Datei)"),
        "{text}"
    );
    assert!(text.contains(&format!("„{}…“", "A".repeat(40))), "{text}");
    assert!(!text.contains(&"A".repeat(41)), "{text}");
    let file = fs::read_to_string(dir.join("k.step")).unwrap();
    let a256 = "A".repeat(256);
    assert!(file.contains(&format!("PRODUCT('{a256}',")));
    assert!(!file.contains(&"A".repeat(257)));
    let mut open = AnalyticKernel::default()
        .make_box(DVec3::splat(1.0))
        .unwrap();
    let f = open.faces.iter().next().unwrap().0;
    open.faces[f].outer.half_edges.pop();
    let items = vec![
        ExportItem {
            name: "B".repeat(100),
            source: ExportSource::Solid(open),
        },
        ExportItem {
            name: "C".repeat(100),
            source: ExportSource::Mesh(
                Arc::new(cube()),
                planora_exchange::MeshTransform::default(),
            ),
        },
    ];
    let p = prepare_step(&items, &Progress::new()).unwrap();
    let b40 = "B".repeat(40);
    assert!(p.rejected[0].starts_with(&format!("„{b40}…“ wird nicht ausgegeben: ")));
    assert_eq!(p.skipped_meshes, vec![format!("{}…", "C".repeat(40))]);
    let _ = fs::remove_dir_all(&dir);
}
