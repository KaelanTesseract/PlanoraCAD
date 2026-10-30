//! Fenster „Exportieren“ auf Englisch (Sprachplan, Task 10): Netzformate, STEP und Zeichnungen
//! (SVG, DXF, PDF) mit Karten, Angaben, Fortschritt, Hinweisen, Fehlern und Schreibergebnissen,
//! unter `with_lang(&english(), …)` gezeichnet, ohne Fenster. Kein Test ruft `set_language` oder
//! `set_global` (Ruling 4).
//!
//! Meldungen, die das Fenster in seinem Zustand behält, werden unter Deutsch angelegt und im
//! nächsten Bild unter Englisch gezeichnet. Texte der Bibliotheken (`planora-drawing`,
//! `planora-step`, `planora-exchange`) bleiben bis Task 12/14 deutsch; sie sind unten in `LATER`
//! aufgeführt und nur von der Deutsch-Prüfung ausgenommen.

mod common;

use common::{Seen, assert_english, assert_has};
use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_drawing::dimension::DimError;
use planora_drawing::sheet_info::TitleTexts;
use planora_drawing::{
    DrawBody, DrawError, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale, Sheet,
    TitleField, ViewKind, ViewLines, compute_view,
};
use planora_exchange::{ExchangeMesh, Format, Progress};
use planora_i18n::{english, german, with_lang};
use planora_math::DVec3;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SVG: ExportFormat = ExportFormat::Drawing(DrawingFormat::Svg);
const DXF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Dxf);
const PDF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Pdf);

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

/// Texte der Bibliotheken, die bis Task 12/14 deutsch bleiben und einen deutschen Buchstaben
/// enthalten: der Grund einer gescheiterten Ansicht („Der Körper ist für diese Ansicht zu
/// aufwendig …“).
const LATER: [&str; 1] = ["Der Körper ist für diese Ansicht"];

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
    let d = std::env::temp_dir().join(format!("planora-export-en-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Fenster in Full-HD-Größe (1920 × 1080).
fn wide() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    }
}

/// Zeichnet das Fenster dreimal (Fenster und Layout setzen sich) und liefert die Texte des
/// letzten Bilds.
fn draw(ctx: &egui::Context, d: &mut ExportDialog, s: &mut AppState) -> Vec<Seen> {
    let mut dir = None;
    let mut seen = Vec::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show_in(
                ui.ctx(),
                s,
                &mut dir,
                &mut planora_app::places::Places::default()
            ));
        });
        seen = common::collect_seen(&out.shapes);
        out.drop_without_applying_deltas();
    }
    seen
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn box_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    s.selected_body = None;
    s
}

/// Quader, Kugel und ein Netz-Körper „Cube“.
fn mixed_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_sphere(5.0).unwrap();
    s.import_mesh("Cube", Arc::new(cube()), 1.0).unwrap();
    s.selected_body = None;
    s
}

/// Zeichnung `name` mit Vorne (frei), Oben und Links (abgeleitet).
fn add_drawing(s: &mut AppState, name: &str) -> DrawingId {
    let one = Scale { num: 1, den: 1 };
    let d = s
        .document
        .add_drawing(name, A3_QUER, Projection::First)
        .unwrap();
    let front = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            one,
            Placement::Free { x: 120.0, y: 200.0 },
            None,
        )
        .unwrap();
    for (kind, distance) in [(ViewKind::Top, 70.0), (ViewKind::Left, 100.0)] {
        s.document
            .add_view(d, kind, one, Placement::Aligned { distance }, Some(front))
            .unwrap();
    }
    d
}

fn endless_step_check(_: &[ExportItem], p: &Progress) -> Result<StepPrepared, String> {
    loop {
        p.check().map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn held_step_writer(
    _: &Path,
    _: &StepPrepared,
    p: &Progress,
) -> Result<u64, planora_step::StepError> {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !p.is_cancelled() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    Err(planora_step::StepError::Cancelled)
}

fn fail_top(b: &[DrawBody], k: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    if k == ViewKind::Top {
        Err(DrawError::TooComplex(123))
    } else {
        compute_view(b, k, p)
    }
}

fn check(what: &str, seen: &[Seen]) {
    assert_english(what, seen, &[], &LATER);
}

fn has_prefix(seen: &[Seen], prefix: &str) -> bool {
    seen.iter().any(|s| s.text.starts_with(prefix))
}

#[test]
fn export_speaks_english() {
    with_lang(&english(), || {
        // Netzformat: Karten, Formate, Qualität, Umfang, Ergebnis, Knöpfe.
        let ctx = context();
        let mut s = box_state();
        let mut d = ExportDialog::new(&s);
        d.tick(&s, true);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has(
            "Netzformat",
            &seen,
            &[
                "Format",
                "Quality",
                "Scope",
                "STL – for 3D printing",
                "OBJ – for many 3D programs",
                "3MF – for 3D printing, with names",
                "glTF (.glb) – for viewing and showing",
                "STEP – for other CAD programs (exact)",
                "Drawing as SVG",
                "Drawing as DXF",
                "Drawing as PDF",
                "Coarse (up to 0.1 mm deviation)",
                "Normal (up to 0.01 mm)",
                "Fine (up to 0.001 mm)",
                "Finer means more triangles and larger files.",
                "Selected body",
                "All visible bodies",
                "Suppressed steps are not included.",
                "12 triangles, under 0.1 MB",
                "Export …",
                "Close",
            ],
        );
        check("Netzformat", &seen);

        // Speichern unter (Dateiauswahl in der Fußzeile).
        d.begin_save(temp_dir("chooser"), &s);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has("Speichern", &seen, &["Save as"]);
        check("Speichern", &seen);

        // Schreiben: Fertig, veraltet, nicht berechnet.
        let dir = temp_dir("mesh");
        let mut d = ExportDialog::new(&s);
        d.tick(&s, true);
        d.write_to(dir.join("box.stl"), &s);
        d.tick(&s, true);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has(
            "Fertig",
            &seen,
            &["Done: \"box.stl\" written (under 0.1 MB)."],
        );
        check("Fertig", &seen);
        s.create_box(DVec3::splat(3.0)).unwrap();
        d.write_to(dir.join("box2.stl"), &s);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has(
            "Veraltet",
            &seen,
            &["Not written: The selection has changed, please save again."],
        );
        check("Veraltet", &seen);
        d.recompute();
        d.write_to(dir.join("box3.stl"), &s);
        let mut d2 = ExportDialog::new(&s);
        d2.write_to(dir.join("box3.stl"), &s);
        let seen = draw(&ctx, &mut d2, &mut s);
        assert_has(
            "Nicht berechnet",
            &seen,
            &["Not written: Nothing has been calculated yet. Please wait or recalculate."],
        );
        check("Nicht berechnet", &seen);
        let _ = fs::remove_dir_all(&dir);

        // STEP: Angaben, ausgelassene Netz-Körper, Rechnen, Abbrechen, nichts auszugeben.
        let mut s = mixed_state();
        let mut d = ExportDialog::new(&s);
        d.format = ExportFormat::Step;
        d.tick(&s, true);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has(
            "STEP",
            &seen,
            &[
                "Fillets and holes stay exact.",
                "Mesh bodies are not written as STEP: \"Cube\". Choose STL, OBJ, 3MF or glTF for them.",
            ],
        );
        assert!(
            has_prefix(&seen, "Bodies: 2, faces: "),
            "{}",
            common::join(&seen)
        );
        assert!(
            !seen.iter().any(|t| t.text == "Quality"),
            "bei STEP keine Karte Qualität"
        );
        check("STEP", &seen);

        let mut d = ExportDialog::new(&s);
        d.format = ExportFormat::Step;
        d.set_step_prepare(endless_step_check);
        d.tick(&s, false);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has("STEP rechnet", &seen, &["Checking the bodies …", "Cancel"]);
        check("STEP rechnet", &seen);
        d.cancel();
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has("STEP abgebrochen", &seen, &["Cancelled", "Recalculate"]);
        check("STEP abgebrochen", &seen);

        let mut only_mesh = AppState::new();
        only_mesh
            .import_mesh("Cube", Arc::new(cube()), 1.0)
            .unwrap();
        let mut d = ExportDialog::new(&only_mesh);
        d.format = ExportFormat::Step;
        d.tick(&only_mesh, true);
        let seen = draw(&ctx, &mut d, &mut only_mesh);
        assert_has(
            "STEP leer",
            &seen,
            &["There is no body that can be written as a STEP file."],
        );
        check("STEP leer", &seen);

        // STEP schreiben: Hinweis beim Schließen, Fortschritt, Abbrechen.
        let mut s = box_state();
        let dir = temp_dir("step");
        let mut d = ExportDialog::new(&s);
        d.format = ExportFormat::Step;
        d.tick(&s, true);
        d.set_step_writer(held_step_writer);
        d.write_to(dir.join("k.step"), &s);
        assert!(!d.request_close());
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has(
            "STEP schreibt",
            &seen,
            &[
                "Writing the file …",
                "Cancel",
                "Writing is still in progress. Please wait or cancel first.",
            ],
        );
        check("STEP schreibt", &seen);
        d.cancel_write();
        d.tick(&s, true);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has("STEP Abbruch", &seen, &["Not written: Cancelled"]);
        let _ = fs::remove_dir_all(&dir);

        // Zeichnungen: ohne Zeichnung, SVG/DXF mit Angaben, Netz-Körper, PDF mit Blattwahl.
        let mut none = box_state();
        let mut d = ExportDialog::new(&none);
        d.format = SVG;
        d.tick(&none, true);
        let seen = draw(&ctx, &mut d, &mut none);
        assert_has(
            "Keine Zeichnung",
            &seen,
            &[
                "No drawing available.",
                "There is no drawing yet. You can create one under \"Drawing\" in the menu.",
                "Include sheet frame",
            ],
        );
        check("Keine Zeichnung", &seen);

        let mut s = mixed_state();
        add_drawing(&mut s, "Sheet 1");
        for format in [SVG, DXF] {
            let mut d = ExportDialog::new(&s);
            d.format = format;
            d.tick(&s, true);
            let seen = draw(&ctx, &mut d, &mut s);
            assert_has(
                "Zeichnung",
                &seen,
                &[
                    "Drawing",
                    "Include sheet frame",
                    "Views: 3",
                    "Sheet: A3 landscape",
                    "Dimensions: 0",
                    "Centerlines: yes",
                    "All views are calculated.",
                    "Mesh bodies are not included in the drawing: \"Cube\".",
                ],
            );
            assert!(has_prefix(&seen, "Lines: "), "{}", common::join(&seen));
            assert!(
                !has_prefix(&seen, "Lines: calculating"),
                "{}",
                common::join(&seen)
            );
            check("Zeichnung", &seen);
        }

        let mut d = ExportDialog::new(&s);
        d.format = PDF;
        d.tick(&s, true);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has(
            "PDF",
            &seen,
            &["Selected sheet", "All sheets", "All views are calculated."],
        );
        check("PDF", &seen);
        d.pdf_pages = PdfPages::All;
        d.tick(&s, true);
        let seen = draw(&ctx, &mut d, &mut s);
        assert_has(
            "PDF alle Blätter",
            &seen,
            &[
                "With \"All sheets\", all drawings go into the PDF one after another.",
                "Sheets: 1",
                "All sheets are calculated.",
            ],
        );
        check("PDF alle Blätter", &seen);

        // Eine Ansicht scheitert: Meldung mit Ansichtsname, Zeile „Linien“.
        let mut s = box_state();
        add_drawing(&mut s, "Sheet 1");
        let mut d = ExportDialog::new(&s);
        d.set_compute(fail_top);
        d.format = SVG;
        d.tick(&s, true);
        let seen = draw(&ctx, &mut d, &mut s);
        assert!(
            has_prefix(&seen, "The view \"Top\" cannot be calculated: "),
            "{}",
            common::join(&seen)
        );
        assert_has(
            "Ansicht scheitert",
            &seen,
            &["Lines: not calculated", "Recalculate"],
        );
        check("Ansicht scheitert", &seen);
    });
}

/// Meldungen, die das Fenster in seinem Zustand behält, werden unter Deutsch angelegt und im
/// nächsten Bild unter Englisch gezeichnet.
#[test]
fn export_stored_messages_follow_the_language() {
    let ctx = context();

    // Abbrechen (Ergebnis des Rechnens) und Fehler einer Ansicht.
    let mut s = mixed_state();
    let mut d = ExportDialog::new(&s);
    d.format = ExportFormat::Step;
    d.set_step_prepare(endless_step_check);
    d.tick(&s, false);
    d.cancel();
    assert_eq!(d.status(&s), ExportStatus::Failed("Abgebrochen".into()));
    let seen = with_lang(&german(), || draw(&ctx, &mut d, &mut s));
    assert_has("Deutsch", &seen, &["Abgebrochen"]);
    let seen = with_lang(&english(), || draw(&ctx, &mut d, &mut s));
    assert_has("Abbruch", &seen, &["Cancelled"]);
    check("Abbruch", &seen);

    let mut s = box_state();
    add_drawing(&mut s, "Sheet 1");
    let mut d = ExportDialog::new(&s);
    d.set_compute(fail_top);
    d.format = SVG;
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Failed(m) if m.starts_with("Die Ansicht „Oben“")));
    let seen = with_lang(&english(), || draw(&ctx, &mut d, &mut s));
    assert!(
        has_prefix(&seen, "The view \"Top\" cannot be calculated: "),
        "{}",
        common::join(&seen)
    );
    check("Ansicht", &seen);

    // Schreibergebnis „veraltet“ und „nicht berechnet“.
    let mut s = box_state();
    let dir = temp_dir("stored");
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    s.create_box(DVec3::splat(3.0)).unwrap();
    d.write_to(dir.join("a.stl"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(stale()));
    let seen = with_lang(&english(), || draw(&ctx, &mut d, &mut s));
    assert_has(
        "veraltet",
        &seen,
        &["Not written: The selection has changed, please save again."],
    );
    let mut d = ExportDialog::new(&s);
    d.write_to(dir.join("b.stl"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(not_computed()));
    let seen = with_lang(&english(), || draw(&ctx, &mut d, &mut s));
    assert_has(
        "nicht berechnet",
        &seen,
        &["Not written: Nothing has been calculated yet. Please wait or recalculate."],
    );

    // Zeichnung ohne Ansicht: der gesperrte Knopf nennt den Grund auch nach dem Wechsel.
    let mut s = box_state();
    s.document
        .add_drawing("Empty", A3_QUER, Projection::First)
        .unwrap();
    let mut d = ExportDialog::new(&s);
    d.format = SVG;
    d.tick(&s, true);
    d.write_to(dir.join("c.svg"), &s);
    assert_eq!(
        d.write_state(),
        WriteState::Failed("Die Zeichnung „Empty“ hat noch keine Ansicht.".into())
    );
    let seen = with_lang(&english(), || draw(&ctx, &mut d, &mut s));
    assert_has(
        "ohne Ansicht",
        &seen,
        &[
            "Not written: The drawing \"Empty\" has no view yet.",
            "The drawing \"Empty\" has no view yet.",
        ],
    );
    check("ohne Ansicht", &seen);
    let _ = fs::remove_dir_all(&dir);
}

/// Texte der Hilfsfunktionen, Zahlen und die Meldungen zu Zeichen, Maßen und Blättern.
#[test]
fn export_message_texts_speak_english() {
    with_lang(&english(), || {
        // Zahlen.
        assert_eq!(group_digits(12_345), "12,345");
        assert_eq!(group_digits(12), "12");
        assert_eq!(mb_text(1_260_000), "1.3 MB");
        assert_eq!(mb_text(50_000), "under 0.1 MB");
        assert_eq!(estimate_text(1_260_000), "approx. 1.3 MB");
        assert_eq!(estimate_text(684), "under 0.1 MB");
        assert_eq!(
            lines_counts_text(12_345, 3),
            "Lines: 12,345 visible, 3 hidden"
        );
        assert_eq!(
            sheet_label(A3_QUER),
            "A3 landscape",
            "Blatt quer in der Sprache des Fensters"
        );

        // Blätter und Maße.
        assert_eq!(
            outside_text(1),
            "1 dimension lies outside the sheet and is still written."
        );
        assert_eq!(
            outside_text(2),
            "2 dimensions lie outside the sheet and are still written."
        );
        assert_eq!(pages_label(1), "1 sheet");
        assert_eq!(pages_label(3), "3 sheets");
        assert_eq!(sheet_progress_text(2, 5), "Calculating sheet 2 of 5 …");
        assert_eq!(
            in_sheet("Sheet 2", "Cancelled"),
            "In sheet \"Sheet 2\": Cancelled"
        );
        assert_eq!(
            lost_dimension_text("Diameter", "Front", &DimError::Lost("gone.".into())),
            "The dimension \"Diameter\" in the view \"Front\" has lost its reference: gone. Please delete the dimension and place it again."
        );
        assert_eq!(
            lost_balloon_text("Front"),
            "The balloon in the view \"Front\" belongs to a part that no longer exists. Please delete the balloon and place it again."
        );
        assert_eq!(
            lost_advice(),
            "Please delete the dimension and place it again."
        );
        let many = too_many_pages_text(300);
        assert!(many.starts_with("A PDF can hold at most "), "{many}");
        assert!(
            many.ends_with(", the document has 300. Please export single sheets."),
            "{many}"
        );

        // Felder des Blattrahmens in Meldungen zu DXF und PDF.
        let mut t = TitleTexts {
            fields: BTreeMap::new(),
            revisions: Vec::new(),
            notes: vec!["ok".into(), "x😀".into()],
            parts: Vec::new(),
        };
        t.fields.insert(TitleField::Owner, "Owner Ltd".into());
        assert_eq!(
            dxf_sheet_texts_ok(&t),
            Err("The character U+1F600 in the field \"Note 2\" cannot be written to DXF. Please change it.".into())
        );
        assert_eq!(
            pdf_sheet_texts_ok(&t),
            Err("The character \"😀\" in the field \"Note 2\" cannot be written to a PDF.".into())
        );
        t.notes = vec!["Ä".repeat(60)];
        assert_eq!(
            dxf_sheet_texts_ok(&t),
            Err("The text in the field \"Note 1\" is too long for DXF. Please shorten it.".into())
        );
        t.notes.clear();
        t.fields.insert(TitleField::Title, "a\u{200B}b".into());
        assert_eq!(
            pdf_sheet_texts_ok(&t),
            Err("The character U+200B in the field \"Title\" cannot be written to a PDF.".into())
        );
        assert_eq!(
            pdf_char_text('😀', "Material"),
            "The character \"😀\" in the field \"Material\" cannot be written to a PDF."
        );

        // Hinweise und Namen.
        let names = vec!["Cube".to_string(), "Ball".to_string()];
        assert_eq!(
            skipped_text(&names),
            "Mesh bodies are not written as STEP: \"Cube\", \"Ball\". Choose STL, OBJ, 3MF or glTF for them."
        );
        assert_eq!(
            drawing_skipped_text(&names),
            "Mesh bodies are not included in the drawing: \"Cube\", \"Ball\"."
        );
        assert!(shortened_text(&names).starts_with("Name shortened (at most "));
        assert_eq!(
            computing_text(ExportFormat::Mesh(Format::Stl), false),
            "Calculating the mesh …"
        );
        assert_eq!(computing_text(SVG, true), "Calculating the parts list …");
        assert_eq!(centerline_use_label(), ["yes", "no", "partly"]);
        assert_eq!(
            ExportFormat::Step.label(),
            "STEP – for other CAD programs (exact)"
        );
        for q in Quality::ALL {
            let l = q.label();
            assert!(l.contains("up to 0."), "{l}");
        }
    });
}

fn centerline_use_label() -> [String; 3] {
    [
        CenterlineUse::Yes.label(),
        CenterlineUse::No.label(),
        CenterlineUse::Partly.label(),
    ]
}
