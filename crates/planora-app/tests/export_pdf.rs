//! Fenster „Exportieren“ mit dem achten Format „Zeichnung als PDF“ (Teilprojekt 3c-3, Aufgabe 4):
//! Formatliste, das gewählte Blatt als Datei, Blattrahmen, Zeichenprüfung mit Feldnamen,
//! verlorene Positionsnummern, Endung und Dateiname, Schreiben über die Hilfsdatei, Abbrechen
//! und Schließen, Dokumenteigenschaften und der Stand der Ergebnisse je Format.
//!
//! Die Dateien werden echt geschrieben und mit dem eigenen PDF-Leser der Zeichnungstests gelesen;
//! verglichen wird mit einem Blatt, das der Test selbst aus den Körpern berechnet.
//!
//! Aufgabe 5: „Alle Blätter“ in einer PDF (Reihenfolge, Sperre je Blatt nach F2, Grenze von 200
//! Blättern, Stand des Ergebnisses, Fortschritt „Blatt n von N“, Abbrechen und Schließen).

mod common;
#[path = "../../planora-drawing/tests/common/pdf_read.rs"]
mod pdf_read;

use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_drawing::dimension::{DimId, DimKind};
use planora_drawing::pdf::PdfMeta;
use planora_drawing::sheet::{SheetModel, build_sheet};
use planora_drawing::sheet_info::{FieldValue, ProjectInfo, SheetInfo, Stamp, TitleTexts};
use planora_drawing::{
    DrawBody, DrawError, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale, Sheet,
    TitleField, ViewId, ViewKind, ViewLines, Visibility, compute_view, pdf_bytes, svg_string,
};
use planora_exchange::{Format, Progress};
use planora_math::DVec3;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const SVG: ExportFormat = ExportFormat::Drawing(DrawingFormat::Svg);
const DXF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Dxf);
const PDF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Pdf);

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};
const A4_HOCH: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-pdf-ui-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Zahl der Hilfsdateien („….planora-tmp“) im Ordner.
fn temp_files(dir: &Path) -> usize {
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

/// Namen aller Dateien im Ordner.
fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
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

/// Quader 100 × 60 × 40 mm.
fn box_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    s.selected_body = None;
    s
}

/// `n` Quader mit den Namen „Teil 1“ … „Teil n“.
fn named_boxes(n: usize) -> (AppState, Vec<planora_feature::FeatureId>) {
    let mut s = AppState::new();
    let ids: Vec<_> = (0..n)
        .map(|i| {
            let f = s
                .create_box(DVec3::new(40.0, 30.0, 20.0 + i as f64))
                .unwrap();
            s.rename_feature(f, &format!("Teil {}", i + 1)).unwrap();
            f
        })
        .collect();
    s.selected_body = None;
    (s, ids)
}

fn one(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

/// Zeichnung `name` (A3 quer, Erstwinkel): Vorne (frei), Oben und Links (abgeleitet) und eine
/// isometrische Ansicht mit verdeckten Kanten.
fn add_drawing(s: &mut AppState, name: &str) -> DrawingId {
    let d = s
        .document
        .add_drawing(name, A3_QUER, Projection::First)
        .unwrap();
    let front = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            one(1, 1),
            Placement::Free { x: 120.0, y: 200.0 },
            None,
        )
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Top,
            one(1, 1),
            Placement::Aligned { distance: 70.0 },
            Some(front),
        )
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Left,
            one(1, 1),
            Placement::Aligned { distance: 100.0 },
            Some(front),
        )
        .unwrap();
    let iso = s
        .document
        .add_view(
            d,
            ViewKind::Isometric,
            one(1, 2),
            Placement::Free { x: 330.0, y: 90.0 },
            None,
        )
        .unwrap();
    s.document
        .update_drawing(d, |dr| {
            dr.views
                .iter_mut()
                .find(|v| v.id == iso)
                .unwrap()
                .show_hidden = true;
        })
        .unwrap();
    d
}

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-18".into(),
        user: "dennis".into(),
    }
}

/// Setzt die Blattangaben; `f` ändert die Vorgabe (alles automatisch).
fn set_info(s: &mut AppState, id: DrawingId, f: impl FnOnce(&mut SheetInfo)) {
    let mut info = s
        .document
        .drawing(id)
        .unwrap()
        .info
        .clone()
        .unwrap_or_else(|| SheetInfo::new(&stamp()));
    f(&mut info);
    s.document.set_sheet_info(id, info).unwrap();
}

fn set_title(s: &mut AppState, id: DrawingId, title: &str) {
    set_info(s, id, |i| i.title = FieldValue::Text(title.to_string()));
}

fn set_parts_switch(s: &mut AppState, id: DrawingId, on: bool) {
    set_info(s, id, |i| i.parts_list = on);
}

fn set_creator(s: &mut AppState, creator: FieldValue) {
    let mut p = s
        .document
        .project_info()
        .cloned()
        .unwrap_or_else(|| ProjectInfo::new(&stamp()));
    p.creator = creator;
    s.document.set_project_info(p).unwrap();
}

fn body_key(f: planora_feature::FeatureId) -> planora_drawing::BodyKey {
    planora_drawing::BodyKey {
        feature: f.0,
        index: 0,
    }
}

fn set_name_override(s: &mut AppState, f: planora_feature::FeatureId, name: &str) {
    s.document
        .set_parts_override(
            body_key(f),
            planora_drawing::RowOverride {
                name: FieldValue::Text(name.to_string()),
                ..Default::default()
            },
        )
        .unwrap();
}

fn front_of(s: &AppState, id: DrawingId) -> ViewId {
    s.document.drawing(id).unwrap().views[0].id
}

fn dialog_with(s: &AppState, format: ExportFormat, frame: bool, id: DrawingId) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.with_frame = frame;
    d.format = format;
    d.drawing = Some(id);
    d.tick(s, true);
    d
}

/// Fenster ohne Blattrahmen für die Zeichnung `id`.
fn dialog(s: &AppState, format: ExportFormat, id: DrawingId) -> ExportDialog {
    dialog_with(s, format, false, id)
}

/// Blatt, das der Test selbst aus den exakten Körpern und der Zeichnung berechnet.
fn own_sheet(s: &AppState, id: DrawingId) -> SheetModel {
    let bodies: Vec<DrawBody> = s
        .bodies
        .iter()
        .filter(|b| b.kind != BodyKind::Mesh)
        .map(|b| DrawBody {
            name: b.name.clone(),
            solid: &b.solid,
        })
        .collect();
    let drawing = s.document.drawing(id).unwrap();
    let lines: Vec<(ViewId, ViewLines)> = drawing
        .views
        .iter()
        .map(|v| {
            (
                v.id,
                compute_view(&bodies, v.kind, &Progress::new()).unwrap(),
            )
        })
        .collect();
    let map: BTreeMap<ViewId, &ViewLines> = lines.iter().map(|(i, l)| (*i, l)).collect();
    build_sheet(drawing, &map)
}

fn count(m: &SheetModel, vis: Visibility) -> usize {
    m.views
        .iter()
        .flat_map(|v| &v.lines)
        .filter(|l| l.vis == vis)
        .count()
}

fn done(d: &ExportDialog) -> (PathBuf, u64) {
    match d.write_state() {
        WriteState::Done(p, n) => (p, n),
        other => panic!("{other:?}"),
    }
}

/// Schreibt mit dem Fenster nach `path` und liefert die Bytes der Datei.
fn written(d: &mut ExportDialog, s: &AppState, path: &Path) -> Vec<u8> {
    d.tick(s, true);
    d.write_to(path.to_path_buf(), s);
    d.tick(s, true);
    let (p, n) = done(d);
    assert_eq!(p, path.with_extension("pdf").as_path());
    let bytes = fs::read(&p).unwrap();
    assert_eq!(n, bytes.len() as u64);
    bytes
}

fn read(bytes: &[u8]) -> pdf_read::Pdf {
    pdf_read::read(bytes).unwrap_or_else(|e| panic!("kein gültiges PDF: {e}"))
}

/// Alle Texte der Seite `page`.
fn texts(pdf: &pdf_read::Pdf, page: usize) -> Vec<String> {
    pdf.pages[page]
        .items
        .iter()
        .filter_map(|i| match i {
            pdf_read::Item::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Breiten (pt) aller Striche der Seite `page`.
fn stroke_widths(pdf: &pdf_read::Pdf, page: usize) -> Vec<f64> {
    pdf.pages[page]
        .items
        .iter()
        .filter_map(|i| match i {
            pdf_read::Item::Stroke { width, .. } => Some(*width),
            _ => None,
        })
        .collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

fn info_text(pdf: &pdf_read::Pdf, key: &str) -> Option<String> {
    pdf.info.get(key).map(pdf_read::utf16_text)
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn pdf_is_the_eighth_format() {
    assert_eq!(ExportFormat::ALL.len(), 8);
    assert_eq!(ExportFormat::ALL[7], PDF);
    assert_eq!(PDF.label(), "Zeichnung als PDF");
    assert_eq!(PDF.extension(), "pdf");
    assert!(PDF.is_drawing() && !PDF.is_step());
    assert_eq!(PDF.drawing_format(), Some(DrawingFormat::Pdf));
    // Die ersten sieben Einträge behalten ihre Stelle.
    assert_eq!(ExportFormat::ALL[5], SVG);
    assert_eq!(ExportFormat::ALL[6], DXF);

    let mut s = box_state();
    add_drawing(&mut s, "Blatt 1");
    for format in [ExportFormat::Mesh(Format::Stl), PDF] {
        let mut d = ExportDialog::new(&s);
        d.format = format;
        let text = draw(&mut d, &mut s);
        for word in [
            "Zeichnung als SVG",
            "Zeichnung als DXF",
            "Zeichnung als PDF",
            "STEP – für andere CAD-Programme (exakt)",
        ] {
            assert!(text.contains(word), "{format:?}: „{word}“ fehlt:\n{text}");
        }
        let rect = d.last_rect.unwrap();
        assert!(
            (rect.width() - 720.0).abs() <= 1.0 && (rect.center().x - 600.0).abs() <= 1.0,
            "720 breit und mittig ({format:?}): {rect:?}"
        );
        assert!(d.last_cards.len() >= 2);
        assert!(
            d.last_cards
                .windows(2)
                .all(|w| (w[0].height() - w[1].height()).abs() < 0.5),
            "Karten gleich hoch ({format:?}): {:?}",
            d.last_cards
        );
    }
}

// (b) ------------------------------------------------------------------------------------------

#[test]
fn selected_sheet_is_written() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    set_info(&mut s, id, |i| {
        i.issued = FieldValue::Text("2026-01-02".into());
    });
    let own = own_sheet(&s, id);
    let (visible, hidden) = (
        count(&own, Visibility::Visible),
        count(&own, Visibility::Hidden),
    );
    assert_eq!((visible, hidden), (21, 3));
    let dir = temp_dir("schreiben");
    let mut d = dialog(&s, PDF, id);
    assert_eq!(
        d.status(&s),
        ExportStatus::DrawingReady {
            views: 4,
            sheet: "A3 quer".into(),
            visible,
            hidden,
            skipped_meshes: vec![],
            dimensions: 0,
            outside: 0,
            centerlines: CenterlineUse::Yes,
        }
    );
    d.begin_save(dir.clone(), &s);
    let path = dir.join("Blatt 1.pdf");
    assert_eq!(d.chooser().unwrap().target(), Some(path.clone()));
    d.accept_chosen(path.clone(), &s);
    d.tick(&s, true);
    let (p, n) = done(&d);
    assert_eq!(p, path);
    let bytes = fs::read(&path).unwrap();
    assert_eq!(n, bytes.len() as u64);
    let meta = pdf_meta(&s, PdfPages::Selected, Some(id), "2099-12-31");
    assert_eq!(
        bytes,
        pdf_bytes(&[&own], &meta).unwrap(),
        "Datei = eigenes Blatt mit den Eigenschaften"
    );
    let pdf = read(&bytes);
    assert_eq!(pdf.pages.len(), 1);
    let mb = pdf.pages[0].media_box;
    assert!(
        mb[0] == 0.0 && mb[1] == 0.0 && near(mb[2], 1190.5512) && near(mb[3], 841.8898),
        "{mb:?}"
    );
    assert_eq!(info_text(&pdf, "Title").as_deref(), Some("Blatt 1"));
    assert_eq!(
        pdf.info["CreationDate"],
        pdf_read::Value::Str(b"D:20260102000000Z".to_vec()),
        "Datum aus dem Ausgabedatum des Blatts"
    );
    assert!(
        !pdf.info.contains_key("Author"),
        "kein Ersteller eingetragen"
    );
    // Ohne Rahmen nur Linien.
    assert!(texts(&pdf, 0).is_empty());
    let ui = draw(&mut d, &mut s);
    let line = format!("Fertig: „Blatt 1.pdf“ geschrieben ({}).", mb_text(n));
    assert!(ui.contains(&line), "„{line}“ fehlt:\n{ui}");
    // Angaben wie bei SVG.
    for word in [
        "Ansichten: 4",
        "Blatt: A3 quer",
        "Linien: 21 sichtbar, 3 verdeckt",
        "Maße: 0",
        "Mittellinien: ja",
        "Alle Ansichten sind berechnet.",
        "Zeichnung als PDF",
    ] {
        assert!(ui.contains(word), "„{word}“ fehlt:\n{ui}");
    }
    for word in ["Qualität", "Umfang", "Dreiecke"] {
        assert!(!ui.contains(word), "„{word}“ gehört nicht zu PDF:\n{ui}");
    }
    assert_eq!(temp_files(&dir), 0);
    assert_eq!(names(&dir), vec!["Blatt 1.pdf".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pdf_status_equals_svg_status() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let svg = dialog(&s, SVG, id).status(&s);
    let pdf = dialog(&s, PDF, id).status(&s);
    assert!(matches!(pdf, ExportStatus::DrawingReady { .. }));
    assert_eq!(svg, pdf);
}

// (c) ------------------------------------------------------------------------------------------

#[test]
fn frame_switch_applies() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    set_info(&mut s, id, |i| {
        i.issued = FieldValue::Text("2026-01-02".into());
    });
    let dir = temp_dir("rahmen");

    // Der Schalter steht in der Karte der Zeichnungsformate, auch bei PDF.
    let mut d = ExportDialog::new(&s);
    d.format = PDF;
    assert!(draw(&mut d, &mut s).contains(FRAME_SWITCH));

    // An: Rahmen mit Texten und kräftigem Randstrich (0,7 mm = 1,9843 pt).
    let mut d = dialog_with(&s, PDF, true, id);
    let on = read(&written(&mut d, &s, &dir.join("an.pdf")));
    let t = texts(&on, 0);
    assert!(t.iter().any(|x| x == "Blatt 1"), "Titel fehlt: {t:?}");
    assert!(t.iter().any(|x| x == "2026-01-02"), "Datum fehlt: {t:?}");
    assert!(t.iter().any(|x| x == "1 von 1"), "Blattangabe fehlt: {t:?}");
    assert!(
        stroke_widths(&on, 0).iter().any(|w| near(*w, 1.9843)),
        "{:?}",
        stroke_widths(&on, 0)
    );

    // Aus: keine Rahmentexte, nur Ansichtsstriche; Umschalten rechnet neu.
    d.with_frame = false;
    assert!(
        matches!(d.status(&s), ExportStatus::Computing(_)),
        "Umschalten startet eine neue Berechnung"
    );
    let off = read(&written(&mut d, &s, &dir.join("aus.pdf")));
    assert!(texts(&off, 0).is_empty(), "{:?}", texts(&off, 0));
    assert!(
        !stroke_widths(&off, 0).iter().any(|w| near(*w, 1.9843)),
        "{:?}",
        stroke_widths(&off, 0)
    );
    assert!(off.pages[0].items.len() < on.pages[0].items.len());
    // Wieder an: wieder neu und mit Rahmen.
    d.with_frame = true;
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    let again = read(&written(&mut d, &s, &dir.join("wieder.pdf")));
    assert!(texts(&again, 0).iter().any(|x| x == "Blatt 1"));

    // Stücklistenblatt ohne Rahmen: nur die Tabelle.
    let (mut s, _) = named_boxes(2);
    let first = add_drawing(&mut s, "Blatt 1");
    s.drawing_ui.open(first);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    let mut d = dialog_with(&s, PDF, false, p1);
    let table = read(&written(&mut d, &s, &dir.join("liste.pdf")));
    let t = texts(&table, 0);
    for want in ["Benennung", "Teil 1", "Teil 2"] {
        assert!(t.iter().any(|x| x == want), "{want} fehlt: {t:?}");
    }
    assert!(
        !t.iter().any(|x| x == "Fertigungszeichnung"),
        "kein Schriftfeld ohne Rahmen: {t:?}"
    );
    assert!(!stroke_widths(&table, 0).iter().any(|w| near(*w, 1.9843)));
    let _ = fs::remove_dir_all(&dir);
}

// (d) ------------------------------------------------------------------------------------------

/// Sieben Dinge, die in WinAnsi stehen oder auf `-` abgebildet werden.
const KEEP: &str = "Platte −0,1 … – Ø ± ° µ €";

#[test]
fn char_lock_names_the_field() {
    assert_eq!(
        pdf_char_text('😀', "Titel"),
        "Das Zeichen „😀“ im Feld „Titel“ lässt sich in einer PDF nicht ausgeben."
    );
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("zeichen");

    // Titel mit Emoji: PDF meldet das Feld und schreibt nichts; SVG derselben Zeichnung geht.
    set_title(&mut s, id, "Halter 😀");
    let mut d = dialog_with(&s, PDF, true, id);
    let msg = pdf_char_text('😀', "Titel");
    assert_eq!(d.status(&s), ExportStatus::Failed(msg.clone()));
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains(&msg), "Meldung im Fenster: {ui}");
    let target = dir.join("e.pdf");
    d.write_to(target.clone(), &s);
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed(msg));
    assert!(!target.exists(), "keine Datei");
    assert_eq!(temp_files(&dir), 0);
    let mut svg = dialog_with(&s, SVG, true, id);
    svg.write_to(dir.join("e.svg"), &s);
    svg.tick(&s, true);
    done(&svg);
    assert!(
        fs::read_to_string(dir.join("e.svg"))
            .unwrap()
            .contains("Halter 😀")
    );
    // Ohne Rahmen gibt es das Feld nicht, also auch keine Meldung.
    assert!(matches!(
        dialog_with(&s, PDF, false, id).status(&s),
        ExportStatus::DrawingReady { .. }
    ));

    // Zulässig: Minuszeichen, Auslassungspunkte, Gedankenstrich, Ø ± ° µ €; im Inhalt `-0,1`.
    set_title(&mut s, id, KEEP);
    let mut d = dialog_with(&s, PDF, true, id);
    let pdf = read(&written(&mut d, &s, &dir.join("ok.pdf")));
    let want = KEEP.replace('−', "-");
    assert!(
        texts(&pdf, 0).contains(&want),
        "{want:?} fehlt: {:?}",
        texts(&pdf, 0)
    );
    assert!(want.contains("-0,1"));

    // Ein Titel, der als DXF zu lang ist, geht als PDF (kein Längenlimit).
    set_title(&mut s, id, &"Ä".repeat(60));
    assert!(matches!(
        dialog_with(&s, DXF, true, id).status(&s),
        ExportStatus::Failed(_)
    ));
    assert!(matches!(
        dialog_with(&s, PDF, true, id).status(&s),
        ExportStatus::DrawingReady { .. }
    ));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn char_lock_names_row_and_column_of_the_parts_list() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    set_parts_switch(&mut s, id, true);
    set_name_override(&mut s, f[0], "Ā");
    let dir = temp_dir("zelle");
    let named = pdf_char_text('Ā', "Stückliste, Pos. 1, Benennung");
    assert_eq!(
        named,
        "Das Zeichen „Ā“ im Feld „Stückliste, Pos. 1, Benennung“ lässt sich in einer PDF nicht ausgeben."
    );
    // Zeichnungsblatt mit Rahmen: PDF nennt Zeile und Spalte wie DXF, SVG wird geschrieben.
    let d = dialog_with(&s, PDF, true, id);
    assert_eq!(d.status(&s), ExportStatus::Failed(named.clone()));
    let mut svg = dialog_with(&s, SVG, true, id);
    svg.write_to(dir.join("z.svg"), &s);
    svg.tick(&s, true);
    done(&svg);
    // Ohne Rahmen steht die Tabelle nicht in der Datei, also gibt es keine Meldung.
    assert!(matches!(
        dialog_with(&s, PDF, false, id).status(&s),
        ExportStatus::DrawingReady { .. }
    ));
    // Stücklistenblatt: auch ohne Rahmen.
    s.drawing_ui.open(id);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    for frame in [false, true] {
        let d = dialog_with(&s, PDF, frame, p1);
        assert_eq!(d.status(&s), ExportStatus::Failed(named.clone()), "{frame}");
        // Gleiche Stelle wie bei DXF (dort ein anderes Zeichen, derselbe Feldname).
        assert!(named.contains("Stückliste, Pos. 1, Benennung"));
    }
    // Zeile 2: ein Zeichen in der Bemerkung wird dort benannt.
    set_name_override(&mut s, f[0], "Welle");
    s.document
        .set_parts_override(
            body_key(f[1]),
            planora_drawing::RowOverride {
                note: FieldValue::Text("Ā".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let d = dialog_with(&s, PDF, false, p1);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(pdf_char_text('Ā', "Stückliste, Pos. 2, Bemerkung"))
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn char_lock_names_the_sheet_fields_like_dxf() {
    let mut t = TitleTexts {
        fields: BTreeMap::new(),
        revisions: Vec::new(),
        notes: vec!["gut".into(), "x😀".into()],
        parts: Vec::new(),
    };
    t.fields.insert(TitleField::Owner, "Eigentümer GmbH".into());
    assert_eq!(
        pdf_sheet_texts_ok(&t),
        Err("Das Zeichen „😀“ im Feld „Hinweis 2“ lässt sich in einer PDF nicht ausgeben.".into())
    );
    // Dieselbe Stelle, die DXF nennt.
    assert_eq!(
        dxf_sheet_texts_ok(&t),
        Err(
            "Das Zeichen U+1F600 im Feld „Hinweis 2“ lässt sich als DXF nicht ausgeben. Bitte ändern."
                .into()
        )
    );
    t.notes = vec!["Ä".repeat(200)];
    assert_eq!(pdf_sheet_texts_ok(&t), Ok(()), "keine Längengrenze");
    assert_eq!(
        dxf_sheet_texts_ok(&t),
        Err("Der Text im Feld „Hinweis 1“ ist für DXF zu lang. Bitte kürzen.".into())
    );
    t.notes = vec![];
    t.revisions = vec![[
        "B".into(),
        "Beschreibung Ā".into(),
        "2026-10-17".into(),
        "dennis".into(),
    ]];
    assert_eq!(
        pdf_sheet_texts_ok(&t),
        Err("Das Zeichen „Ā“ im Feld „Änderung B“ lässt sich in einer PDF nicht ausgeben.".into())
    );
    t.revisions.clear();
    t.fields.insert(TitleField::Title, KEEP.into());
    assert_eq!(pdf_sheet_texts_ok(&t), Ok(()));
    // Das Feld mit dem Zeichen wird an seinem Namen erkannt (Titel, nicht Eigentümer).
    t.fields.insert(TitleField::Title, "A😀".into());
    assert_eq!(
        pdf_sheet_texts_ok(&t),
        Err(pdf_char_text('😀', "Titel")),
        "{:?}",
        t.fields
    );
}

fn suggest(s: &mut AppState, d: DrawingId) {
    use planora_app::dimension_state::{SuggestOutcome, suggest_for};
    s.drawing_ui.open(d);
    s.drawing_ui.tick(&s.bodies, &s.document, true);
    match suggest_for(s, d, None) {
        SuggestOutcome::Added(n) => assert!(n >= 3),
        other => panic!("{other:?}"),
    }
}

fn dim_of(s: &AppState, id: DrawingId, kind: ViewKind, dim: DimKind) -> DimId {
    let d = s.document.drawing(id).unwrap();
    let view = d.views.iter().find(|v| v.kind == kind).unwrap().id;
    d.dimensions
        .iter()
        .find(|m| m.view == view && m.kind == dim)
        .unwrap_or_else(|| panic!("kein Maß {dim:?} in {kind:?}"))
        .id
}

#[test]
fn char_lock_names_the_view_of_a_dimension_text() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    suggest(&mut s, id);
    let width = dim_of(&s, id, ViewKind::Front, DimKind::Horizontal);
    s.document
        .update_dimension(id, width, |m| m.text_override = Some("Breite 😀".into()))
        .unwrap();
    let msg = "Das Zeichen „😀“ in einem Maßtext der Ansicht „Vorne“ lässt sich in einer PDF nicht ausgeben.";
    let dir = temp_dir("mass");
    for frame in [false, true] {
        let d = dialog_with(&s, PDF, frame, id);
        assert_eq!(d.status(&s), ExportStatus::Failed(msg.into()), "{frame}");
        let mut d = d;
        d.write_to(dir.join("m.pdf"), &s);
        d.tick(&s, true);
        assert_eq!(d.write_state(), WriteState::Failed(msg.into()));
    }
    assert_eq!(names(&dir), Vec::<String>::new());
    // Die Prüfung allein: ein Text in einer Ansicht des Blatts mit Maßen.
    let mut sheet = own_sheet(&s, id);
    assert_eq!(pdf_model_texts_ok(&sheet), Ok(()));
    sheet.views[1]
        .marks
        .push(planora_drawing::annotate::Mark::Text {
            pos: planora_math::DVec2::ZERO,
            height: 3.5,
            angle: 0.0,
            anchor: planora_drawing::annotate::TextAnchor::Start,
            text: "ok Ā".into(),
            kind: planora_drawing::annotate::MarkKind::Dimension,
            fit_width: None,
        });
    let name = sheet.views[1].name.clone();
    assert_eq!(
        pdf_model_texts_ok(&sheet),
        Err(format!(
            "Das Zeichen „Ā“ in einem Maßtext der Ansicht „{name}“ lässt sich in einer PDF nicht ausgeben."
        ))
    );
    assert_ne!(name, "Vorne", "die zweite Ansicht heißt anders");
    // Mit Rahmen kommt ein Feldfehler vor dem Maßtext.
    set_title(&mut s, id, "Halter 😀");
    let d = dialog_with(&s, PDF, true, id);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(pdf_char_text('😀', "Titel"))
    );
    // Ein zulässiger eigener Text (Minuszeichen, Ø, °) wird geschrieben.
    set_title(&mut s, id, "Halter");
    s.document
        .update_dimension(id, width, |m| {
            m.text_override = Some("−0,1 Ø ± ° µ €".into())
        })
        .unwrap();
    let mut d = dialog_with(&s, PDF, false, id);
    let pdf = read(&written(&mut d, &s, &dir.join("ok.pdf")));
    assert!(
        texts(&pdf, 0).iter().any(|t| t == "-0,1 Ø ± ° µ €"),
        "{:?}",
        texts(&pdf, 0)
    );
    let _ = fs::remove_dir_all(&dir);
}

// (e) ------------------------------------------------------------------------------------------

#[test]
fn lost_balloon_blocks_pdf() {
    let lost = lost_balloon_text("Vorne");
    assert_eq!(
        lost,
        "Die Positionsnummer in der Ansicht „Vorne“ gehört zu einem Teil, das es nicht mehr gibt. Bitte die Positionsnummer löschen und neu setzen."
    );
    let dir = temp_dir("verloren");
    for frame in [false, true] {
        let (mut s, f) = named_boxes(2);
        let id = add_drawing(&mut s, "Blatt 1");
        let front = front_of(&s, id);
        s.document
            .add_balloon(id, front, body_key(f[0]), (0.5, 0.5), (-15.0, 10.0))
            .unwrap();
        let b1 = s
            .document
            .add_balloon(id, front, body_key(f[1]), (0.5, 0.5), (-25.0, 10.0))
            .unwrap();
        let mut d = dialog_with(&s, PDF, frame, id);
        assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
        // Die Zahlen „1“ und „2“ im Raster des Rahmens zählen nicht als Positionsnummern.
        let (base1, base2) = {
            let mut zero = box_state();
            let zid = add_drawing(&mut zero, "Blatt 1");
            let mut z = dialog_with(&zero, PDF, frame, zid);
            let pdf = read(&written(&mut z, &zero, &dir.join("basis.pdf")));
            let t = texts(&pdf, 0);
            let _ = fs::remove_file(dir.join("basis.pdf"));
            (
                t.iter().filter(|x| *x == "1").count(),
                t.iter().filter(|x| *x == "2").count(),
            )
        };
        // Der zweite Körper verschwindet: Kreis 2 ist verloren.
        s.delete_body(1).unwrap();
        let target = dir.join("v.pdf");
        d.tick(&s, true);
        assert_eq!(d.status(&s), ExportStatus::Failed(lost.clone()), "{frame}");
        let ui = draw(&mut d, &mut s);
        assert!(ui.contains(&lost), "Meldung im Fenster: {ui}");
        d.write_to(target.clone(), &s);
        assert_eq!(d.write_state(), WriteState::Failed(lost.clone()), "{frame}");
        assert!(!target.exists(), "keine Datei: {frame}");
        // Kreis 2 löschen: der Export gelingt, mit Nummer 1.
        s.document
            .update_drawing(id, |dr| {
                dr.remove_balloon(b1);
            })
            .unwrap();
        let pdf = read(&written(&mut d, &s, &target));
        let t = texts(&pdf, 0);
        assert_eq!(t.iter().filter(|x| *x == "1").count(), base1 + 1, "{t:?}");
        assert_eq!(t.iter().filter(|x| *x == "2").count(), base2, "{t:?}");
        let _ = fs::remove_file(&target);
    }
    let _ = fs::remove_dir_all(&dir);
}

// (f) ------------------------------------------------------------------------------------------

#[test]
fn extension_and_name() {
    let p = |s: &str| PathBuf::from(s);
    assert_eq!(normalize_target(PDF, p("x.PDF")), p("x.pdf"));
    assert_eq!(normalize_target(PDF, p("x.Pdf")), p("x.pdf"));
    assert_eq!(normalize_target(PDF, p("x.pdf")), p("x.pdf"));
    assert_eq!(normalize_target(PDF, p("x")), p("x.pdf"));
    assert_eq!(normalize_target(PDF, p("x.svg")), p("x.svg.pdf"));
    assert_eq!(normalize_target(PDF, p("x.dxf")), p("x.dxf.pdf"));
    // Die anderen Zeichnungsformate behalten ihre Endung.
    assert_eq!(normalize_target(SVG, p("x.PDF")), p("x.PDF.svg"));
    assert_eq!(normalize_target(SVG, p("x.SVG")), p("x.svg"));

    let mut s = box_state();
    let id = add_drawing(&mut s, "A/B:C*D?E\"F<G>H|I\\J");
    let dir = temp_dir("name");
    let mut d = dialog(&s, PDF, id);
    d.begin_save(dir.clone(), &s);
    assert_eq!(
        d.chooser().unwrap().target(),
        Some(dir.join("A_B_C_D_E_F_G_H_I_J.pdf"))
    );
    // Eingabe in beliebiger Schreibweise: die Datei heißt klein „.pdf“.
    d.accept_chosen(dir.join("y.PDF"), &s);
    d.tick(&s, true);
    done(&d);
    assert_eq!(names(&dir), vec!["y.pdf".to_string()]);
    // Fremde Endung wird nicht ersetzt, sondern ergänzt.
    let mut d = dialog(&s, PDF, id);
    d.accept_chosen(dir.join("z.svg"), &s);
    d.tick(&s, true);
    let (p, _) = done(&d);
    assert_eq!(p, dir.join("z.svg.pdf"));
    assert_eq!(
        names(&dir),
        vec!["y.pdf".to_string(), "z.svg.pdf".to_string()]
    );
    assert!(read(&fs::read(&p).unwrap()).pages.len() == 1);
    let _ = fs::remove_dir_all(&dir);
}

// (g) ------------------------------------------------------------------------------------------

fn slow_pdf_writer(
    _: &Path,
    _: &[&SheetModel],
    _: &PdfMeta,
    p: &Progress,
) -> Result<u64, DrawError> {
    loop {
        p.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn existing_file_stays_when_cancelled_and_window_stays_open_while_writing() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("abbruch");
    let path = dir.join("z.pdf");
    fs::write(&path, "alt").unwrap();
    let mut d = dialog(&s, PDF, id);
    d.set_pdf_writer(slow_pdf_writer);
    d.write_to(path.clone(), &s);
    assert!(d.is_writing_file());
    assert!(!d.is_writing_step(), "STEP-Name bleibt bei STEP");
    assert!(!d.request_close(), "Schließen muss abgewiesen werden");
    assert_eq!(d.write_state(), WriteState::Writing);
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
    assert!(d.is_writing_file(), "das Schreiben muss noch laufen");
    assert!(text.contains(WRITING_HINT), "{text}");
    assert!(text.contains("Die Datei wird geschrieben"), "{text}");
    assert!(text.contains("Abbrechen"), "{text}");
    d.cancel_write();
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed("Abgebrochen".into()));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "alt",
        "alte Datei bleibt"
    );
    assert!(d.request_close());
    assert_eq!(temp_files(&dir), 0);
    // Mit dem echten Schreiber: während des Schreibens nicht schließbar, danach fertig.
    let mut d = dialog(&s, PDF, id);
    d.write_to(path.clone(), &s);
    assert!(!d.request_close());
    d.tick(&s, true);
    done(&d);
    assert!(d.request_close());
    assert!(fs::read(&path).unwrap().starts_with(b"%PDF-1.4\n"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn existing_file_stays_when_writing_fails() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("fehler");
    // Zielordner gibt es nicht: Fehlermeldung, nichts angelegt.
    let mut d = dialog(&s, PDF, id);
    d.write_to(dir.join("gibt-es-nicht").join("x.pdf"), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Failed(_)));
    assert_eq!(names(&dir), Vec::<String>::new());
    // Schreibgeschützte Zieldatei: bleibt unverändert (Windows ersetzt sie nicht).
    let path = dir.join("fest.pdf");
    fs::write(&path, "alt").unwrap();
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&path, perm.clone()).unwrap();
    d.write_to(path.clone(), &s);
    d.tick(&s, true);
    let state = d.write_state();
    let bytes = fs::read(&path).unwrap();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    fs::set_permissions(&path, perm).unwrap();
    if cfg!(windows) {
        assert!(matches!(state, WriteState::Failed(_)), "{state:?}");
        assert_eq!(bytes, b"alt");
    } else {
        assert!(
            matches!(state, WriteState::Failed(_)) && bytes == b"alt" || bytes.starts_with(b"%PDF")
        );
    }
    assert_eq!(temp_files(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn closing_a_tab_while_a_pdf_is_written_leaves_no_helper_file() {
    use planora_app::workspace::Workspace;
    use planora_project::config::Config;
    let mut w = Workspace::new(None, Config::default());
    let dir = temp_dir("tab");
    let target = dir.join("alt.pdf");
    fs::write(&target, "alt").unwrap();
    let tab = w.active_tab_mut();
    tab.state.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    let id = add_drawing(&mut tab.state, "Blatt 1");
    let mut d = dialog(&tab.state, PDF, id);
    d.write_to(target.clone(), &tab.state);
    tab.export_dialog = Some(d);
    // Erst warten, bis die Hilfsdatei da ist, oder bis das Schreiben schon fertig ist.
    let start = std::time::Instant::now();
    let finished = |t: &std::io::Result<Vec<u8>>| t.as_ref().is_ok_and(|t| t != b"alt");
    while temp_files(&dir) == 0 && !finished(&fs::read(&target)) && start.elapsed().as_secs() < 10 {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        temp_files(&dir) > 0 || finished(&fs::read(&target)),
        "weder Hilfsdatei noch fertige Datei gesehen"
    );
    assert!(!w.request_close(0));
    w.discard_and_continue();
    assert!(w.tabs[0].export_dialog.is_none());
    let start = std::time::Instant::now();
    while temp_files(&dir) > 0 && start.elapsed().as_secs() < 10 {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(temp_files(&dir), 0);
    // Die Zieldatei ist alt oder fertig; beim Ersetzen kann ein Lesen kurz scheitern.
    let mut bytes = fs::read(&target);
    for _ in 0..100 {
        if bytes.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        bytes = fs::read(&target);
    }
    let bytes = bytes.unwrap();
    assert!(bytes == b"alt" || read(&bytes).pages.len() == 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn read_only_tabs_export_like_svg() {
    use planora_app::workspace::Workspace;
    use planora_project::config::Config;
    let mut w = Workspace::new(None, Config::default());
    let dir = temp_dir("alt-stand");
    let tab = w.active_tab_mut();
    tab.state.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    tab.state.selected_body = None;
    let id = add_drawing(&mut tab.state, "Blatt 1");
    tab.read_only = true;
    for (format, name) in [(SVG, "a.svg"), (PDF, "a.pdf")] {
        let mut d = dialog(&tab.state, format, id);
        assert!(matches!(
            d.status(&tab.state),
            ExportStatus::DrawingReady { .. }
        ));
        d.write_to(dir.join(name), &tab.state);
        d.tick(&tab.state, true);
        done(&d);
        assert!(dir.join(name).exists(), "{name}");
    }
    assert!(read(&fs::read(dir.join("a.pdf")).unwrap()).pages.len() == 1);
    let _ = fs::remove_dir_all(&dir);
}

// (h) ------------------------------------------------------------------------------------------

#[test]
fn pdf_meta_rules() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let today = "2026-05-06";

    // Ersteller: nur ein ausdrücklich eingetragener Text, nie der Anmeldename.
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, Some(id), today).author,
        None
    );
    set_creator(&mut s, FieldValue::Auto);
    assert_eq!(
        s.document.project_info().unwrap().login,
        "dennis",
        "Testvoraussetzung: Anmeldename ist bekannt"
    );
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, Some(id), today).author,
        None
    );
    set_creator(&mut s, FieldValue::Text("Erika".into()));
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, Some(id), today).author,
        Some("Erika".into())
    );
    set_creator(&mut s, FieldValue::Text("   ".into()));
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, Some(id), today).author,
        None
    );
    set_creator(&mut s, FieldValue::Text("Erika".into()));

    // Titel: Name des Blatts; Datum: nichts gesetzt (keine Blattangaben) → heute.
    let m = pdf_meta(&s, PdfPages::Selected, Some(id), today);
    assert_eq!(m.title, "Blatt 1");
    assert_eq!(m.date.as_deref(), Some(today), "Zeichnung ohne Satz");
    // Ohne Angabe der Zeichnung gilt die Vorgabe.
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, None, today).title,
        "Blatt 1"
    );

    // Eigenes Ausgabedatum, wenn gültig.
    set_info(&mut s, id, |i| {
        i.issued = FieldValue::Text("2026-01-02".into());
    });
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, Some(id), today)
            .date
            .as_deref(),
        Some("2026-01-02")
    );
    // Ungültig („Anfang Mai“, 30. Februar): das Erstellungsdatum des Blatts.
    for bad in ["Anfang Mai", "2026-02-30", "", "2026-1-2"] {
        set_info(&mut s, id, |i| {
            i.issued = FieldValue::Text(bad.into());
        });
        assert_eq!(
            pdf_meta(&s, PdfPages::Selected, Some(id), today)
                .date
                .as_deref(),
            Some("2026-10-18"),
            "{bad:?}"
        );
    }
    // Automatisch: ebenfalls das Erstellungsdatum.
    set_info(&mut s, id, |i| i.issued = FieldValue::Auto);
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, Some(id), today)
            .date
            .as_deref(),
        Some("2026-10-18")
    );
    // Auch das ungültig: heute.
    set_info(&mut s, id, |i| i.created = "gestern".into());
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, Some(id), today)
            .date
            .as_deref(),
        Some(today)
    );

    // Mehrere Blätter: das gewählte zählt bei „gewähltes Blatt“, das erste bei „alle“.
    let second = add_drawing(&mut s, "Blatt 2");
    set_info(&mut s, id, |i| {
        i.created = "2026-03-04".into();
        i.issued = FieldValue::Auto;
    });
    set_info(&mut s, second, |i| {
        i.issued = FieldValue::Text("2027-07-08".into());
    });
    s.document_name = "Mein Projekt".into();
    let sel = pdf_meta(&s, PdfPages::Selected, Some(second), today);
    assert_eq!(
        (sel.title.as_str(), sel.date.as_deref()),
        ("Blatt 2", Some("2027-07-08"))
    );
    let all = pdf_meta(&s, PdfPages::All, Some(second), today);
    assert_eq!(
        (
            all.title.as_str(),
            all.date.as_deref(),
            all.author.as_deref()
        ),
        ("Mein Projekt", Some("2026-03-04"), Some("Erika"))
    );
}

#[test]
fn written_file_carries_the_meta() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    set_creator(&mut s, FieldValue::Text("Erika Ä".into()));
    set_info(&mut s, id, |i| {
        i.issued = FieldValue::Text("Anfang Mai".into());
        i.created = "2026-03-04".into();
    });
    let dir = temp_dir("meta");
    let mut d = dialog(&s, PDF, id);
    let pdf = read(&written(&mut d, &s, &dir.join("m.pdf")));
    assert_eq!(info_text(&pdf, "Title").as_deref(), Some("Blatt 1"));
    assert_eq!(info_text(&pdf, "Author").as_deref(), Some("Erika Ä"));
    assert_eq!(
        pdf.info["CreationDate"],
        pdf_read::Value::Str(b"D:20260304000000Z".to_vec())
    );
    // Ohne eigenen Ersteller (nur der Anmeldename im Schriftfeld) fehlt der Autor.
    set_creator(&mut s, FieldValue::Auto);
    let mut d = dialog(&s, PDF, id);
    let pdf = read(&written(&mut d, &s, &dir.join("n.pdf")));
    assert!(!pdf.info.contains_key("Author"));
    // Ohne gültiges Datum im Blatt: heute (UTC) aus der Systemzeit.
    set_info(&mut s, id, |i| {
        i.issued = FieldValue::Auto;
        i.created = "gestern".into();
    });
    let mut d = dialog(&s, PDF, id);
    let pdf = read(&written(&mut d, &s, &dir.join("o.pdf")));
    let today = Stamp::now().date.replace('-', "");
    let pdf_read::Value::Str(date) = &pdf.info["CreationDate"] else {
        panic!("{:?}", pdf.info);
    };
    let date = String::from_utf8(date.clone()).unwrap();
    // Um Mitternacht kann der Tag zwischen zwei Aufrufen wechseln.
    assert!(
        date == format!("D:{today}000000Z") || date.len() == 17,
        "{date}"
    );
    assert_ne!(date, "D:20260304000000Z");
    let _ = fs::remove_dir_all(&dir);
}

// (i) ------------------------------------------------------------------------------------------

static COUNTED: AtomicUsize = AtomicUsize::new(0);

fn counting_compute(
    bodies: &[DrawBody],
    kind: ViewKind,
    p: &Progress,
) -> Result<ViewLines, DrawError> {
    COUNTED.fetch_add(1, Ordering::SeqCst);
    compute_view(bodies, kind, p)
}

#[test]
fn formats_keep_their_own_results() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    // Als DXF zu lang, als SVG und PDF in Ordnung: nur DXF trägt den Zuschlag (`dxf: true`).
    set_title(&mut s, id, &"Ä".repeat(60));
    let mut d = ExportDialog::new(&s);
    d.set_compute(counting_compute);
    d.drawing = Some(id);
    d.format = SVG;
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    let after_svg = COUNTED.load(Ordering::SeqCst);
    assert_eq!(after_svg, 4, "vier Ansichtsarten");
    // PDF rechnet ein eigenes Ergebnis.
    d.format = PDF;
    assert!(
        matches!(d.status(&s), ExportStatus::Computing(_)),
        "anderes Format, anderer Schlüssel"
    );
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    assert_eq!(COUNTED.load(Ordering::SeqCst), after_svg + 4);
    // DXF mit demselben Titel scheitert (eigenes Ergebnis mit Zuschlag), PDF und SVG nicht.
    d.format = DXF;
    d.tick(&s, true);
    assert!(
        matches!(d.status(&s), ExportStatus::Failed(m) if m == "Der Text im Feld „Titel“ ist für DXF zu lang. Bitte kürzen.")
    );
    let after_dxf = COUNTED.load(Ordering::SeqCst);
    // Zurück: keine Neuberechnung.
    for f in [SVG, PDF, SVG, PDF] {
        d.format = f;
        d.tick(&s, true);
        assert!(
            matches!(d.status(&s), ExportStatus::DrawingReady { .. }),
            "{f:?}"
        );
    }
    assert_eq!(
        COUNTED.load(Ordering::SeqCst),
        after_dxf,
        "keine Neuberechnung"
    );
    // Der Rahmen wird für PDF ohne den DXF-Zuschlag gebaut: gleiche Texthöhe wie SVG.
    let dir = temp_dir("ergebnisse");
    d.with_frame = true;
    d.format = PDF;
    let pdf = read(&written(&mut d, &s, &dir.join("e.pdf")));
    let title = "Ä".repeat(60);
    let at = pdf.pages[0]
        .items
        .iter()
        .find_map(|i| match i {
            pdf_read::Item::Text { text, size, .. } if *text == title => Some(*size),
            _ => None,
        })
        .expect("Titel in der PDF");
    let mut d = ExportDialog::new(&s);
    d.drawing = Some(id);
    d.format = SVG;
    d.tick(&s, true);
    d.write_to(dir.join("e.svg"), &s);
    d.tick(&s, true);
    done(&d);
    let svg = fs::read_to_string(dir.join("e.svg")).unwrap();
    let ats = svg.find(&format!(">{title}</text>")).unwrap();
    let element = &svg[svg[..ats].rfind("<text").unwrap()..ats];
    let svg_h: f64 = element
        .split("font-size=\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .and_then(|v| v.parse().ok())
        .unwrap();
    assert!(
        (pdf_read::mm(at) - svg_h).abs() < 1e-3,
        "PDF {} mm, SVG {svg_h} mm",
        pdf_read::mm(at)
    );
    let _ = (svg_string, fs::remove_dir_all(&dir));
}

#[test]
fn key_covers_format_and_meta() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("schluessel");
    let mut d = dialog(&s, PDF, id);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    // Ein anderer Ersteller ändert die Eigenschaften: das Ergebnis gilt nicht mehr.
    set_creator(&mut s, FieldValue::Text("Erika".into()));
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    d.write_to(dir.join("1.pdf"), &s);
    assert_eq!(
        d.write_state(),
        WriteState::Failed(STALE.into()),
        "Ersteller"
    );
    let pdf = read(&written(&mut d, &s, &dir.join("1.pdf")));
    assert_eq!(info_text(&pdf, "Author").as_deref(), Some("Erika"));
    // Neuer Name des Blatts: Titel der PDF.
    s.document
        .update_drawing(id, |dr| dr.name = "Anders".into())
        .unwrap();
    d.write_to(dir.join("2.pdf"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()), "Name");
    let pdf = read(&written(&mut d, &s, &dir.join("2.pdf")));
    assert_eq!(info_text(&pdf, "Title").as_deref(), Some("Anders"));
    // Neues Ausgabedatum: Datum der PDF.
    set_info(&mut s, id, |i| {
        i.issued = FieldValue::Text("2031-12-24".into());
    });
    d.write_to(dir.join("3.pdf"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()), "Datum");
    let pdf = read(&written(&mut d, &s, &dir.join("3.pdf")));
    assert_eq!(
        pdf.info["CreationDate"],
        pdf_read::Value::Str(b"D:20311224000000Z".to_vec())
    );
    // Anderes Format: anderer Stand; das PDF-Ergebnis zählt für SVG nicht.
    d.format = SVG;
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    d.write_to(dir.join("4.svg"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(NOT_COMPUTED.into()));
    assert!(!dir.join("4.svg").exists());
    // Das PDF-Ergebnis bleibt stehen, bis es selbst veraltet.
    d.format = PDF;
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================================
// Aufgabe 5: alle Blätter in einer PDF
// =============================================================================================

const PDF_DRAWING: DrawingFormat = DrawingFormat::Pdf;

/// Rechnet die Ansichten nur aus dem ersten Körper (viele Körper braucht nur die Stückliste).
fn first_body(bodies: &[DrawBody], kind: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    compute_view(&bodies[..1], kind, p)
}

/// Zeichnung `name` auf `sheet` mit nur einer Vorderansicht (passt auch auf A4 hoch).
fn add_small(s: &mut AppState, name: &str, sheet: Sheet) -> DrawingId {
    let d = s
        .document
        .add_drawing(name, sheet, Projection::First)
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Front,
            one(1, 1),
            Placement::Free { x: 100.0, y: 150.0 },
            None,
        )
        .unwrap();
    d
}

/// Fügt Quader „Teil {from}“ … „Teil {to}“ hinzu.
fn more_boxes(s: &mut AppState, f: &mut Vec<planora_feature::FeatureId>, from: usize, to: usize) {
    for i in from..=to {
        let id = s
            .create_box(DVec3::new(40.0, 30.0, 20.0 + i as f64))
            .unwrap();
        s.rename_feature(id, &format!("Teil {i}")).unwrap();
        f.push(id);
    }
    s.selected_body = None;
}

/// Dokument „Getriebe“: „Halter“ (A3 quer), Stücklistenblatt (A4 hoch) mit 45 Zeilen auf zwei
/// Seiten, „Deckel“ (A4 hoch). Das Folgeblatt entsteht erst nach „Deckel“ (größere Kennung) und
/// steht doch hinter Seite 1: Dokumentreihenfolge ≠ Reihenfolge der Kennungen.
fn getriebe() -> (AppState, Vec<planora_feature::FeatureId>, Vec<DrawingId>) {
    let (mut s, mut f) = named_boxes(3);
    let halter = add_drawing(&mut s, "Halter");
    s.drawing_ui.open(halter);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    let deckel = add_small(&mut s, "Deckel", A4_HOCH);
    more_boxes(&mut s, &mut f, 4, 45);
    s.document_name = "Getriebe".into();
    let ids: Vec<DrawingId> = s.document.drawings().iter().map(|d| d.id).collect();
    let names: Vec<&str> = s
        .document
        .drawings()
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(names, ["Halter", "Stückliste", "Stückliste (2)", "Deckel"]);
    assert_eq!((ids[0], ids[1], ids[3]), (halter, p1, deckel));
    assert!(ids[2].0 > deckel.0, "Folgeblatt jünger als „Deckel“");
    (s, f, ids)
}

/// Fenster für „Alle Blätter“ (PDF) mit Rechnung `compute`; wartet auf das Ergebnis.
fn all_dialog(
    s: &AppState,
    frame: bool,
    compute: planora_app::drawing_state::ComputeFn,
) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.set_compute(compute);
    d.format = PDF;
    d.pdf_pages = PdfPages::All;
    d.with_frame = frame;
    d.tick(s, true);
    d
}

/// Seitengröße in pt.
fn page_size(pdf: &pdf_read::Pdf, page: usize) -> (f64, f64) {
    let mb = pdf.pages[page].media_box;
    (mb[2], mb[3])
}

/// Die einseitige PDF des gewählten Blatts `id` (gleiche Rechnung, gleicher Rahmen).
fn selected_pdf(
    s: &AppState,
    id: DrawingId,
    frame: bool,
    compute: planora_app::drawing_state::ComputeFn,
    path: &Path,
) -> pdf_read::Pdf {
    let mut d = ExportDialog::new(s);
    d.set_compute(compute);
    d.format = PDF;
    d.drawing = Some(id);
    d.with_frame = frame;
    let pdf = read(&written(&mut d, s, path));
    assert_eq!(pdf.pages.len(), 1);
    pdf
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn all_sheets_in_document_order() {
    let (mut s, _, ids) = getriebe();
    let dir = temp_dir("alle");
    let mut d = all_dialog(&s, true, first_body);
    assert!(
        matches!(&d.status(&s), ExportStatus::DrawingReady { sheet, .. } if sheet == "4 Blätter"),
        "{:?}",
        d.status(&s)
    );
    // Vorgeschlagener Name: der Dokumentname.
    d.begin_save(dir.clone(), &s);
    let path = dir.join("Getriebe.pdf");
    assert_eq!(d.chooser().unwrap().target(), Some(path.clone()));
    d.accept_chosen(path.clone(), &s);
    d.tick(&s, true);
    assert_eq!(done(&d).0, path);
    let pdf = read(&fs::read(&path).unwrap());
    assert_eq!(pdf.pages.len(), 4);
    // Seitengrößen je Blatt: A3 quer, dreimal A4 hoch.
    assert!(near(page_size(&pdf, 0).0, 1190.5512) && near(page_size(&pdf, 0).1, 841.8898));
    for i in 1..4 {
        assert!(
            near(page_size(&pdf, i).0, 595.2756) && near(page_size(&pdf, i).1, 841.8898),
            "Seite {i}: {:?}",
            page_size(&pdf, i)
        );
    }
    assert_eq!(info_text(&pdf, "Title").as_deref(), Some("Getriebe"));
    // Blattangabe im Rahmen: n von 4, in Dokumentreihenfolge.
    for i in 0..4 {
        let want = format!("{} von 4", i + 1);
        assert!(
            texts(&pdf, i).contains(&want),
            "Seite {i}: {want} fehlt: {:?}",
            texts(&pdf, i)
        );
    }
    // Titel der Blätter in Dokumentreihenfolge.
    assert!(texts(&pdf, 0).iter().any(|t| t == "Halter"));
    assert!(texts(&pdf, 3).iter().any(|t| t == "Deckel"));
    // Die Stückliste: Seite 2 mit Pos. 1 … 44, das Folgeblatt (Seite 3) mit Pos. 45.
    assert!(texts(&pdf, 1).iter().any(|t| t == "44"));
    assert!(!texts(&pdf, 1).iter().any(|t| t == "45"));
    assert!(
        texts(&pdf, 2).iter().any(|t| t == "45"),
        "{:?}",
        texts(&pdf, 2)
    );
    // Jede Seite ist das eigene Blatt dieses Dokuments (wie beim gewählten Blatt).
    for (i, id) in ids.iter().enumerate() {
        let one = selected_pdf(&s, *id, true, first_body, &dir.join(format!("s{i}.pdf")));
        assert_eq!(pdf.pages[i].media_box, one.pages[0].media_box, "Seite {i}");
        assert_eq!(pdf.pages[i].ops, one.pages[0].ops, "Seite {i}");
    }
    // Das gewählte Blatt bleibt eine Seite mit dem Namen der Zeichnung.
    let mut sel = ExportDialog::new(&s);
    sel.set_compute(first_body);
    sel.format = PDF;
    sel.drawing = Some(ids[3]);
    sel.tick(&s, true);
    sel.begin_save(dir.clone(), &s);
    assert_eq!(
        sel.chooser().unwrap().target(),
        Some(dir.join("Deckel.pdf"))
    );
    // Angaben im Fenster.
    let ui = draw(&mut d, &mut s);
    for want in ["Blätter: 4", "Alle Blätter sind berechnet."] {
        assert!(ui.contains(want), "„{want}“ fehlt:\n{ui}");
    }
    assert_eq!(temp_files(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn all_with_one_sheet_equals_the_selected_sheet() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    // Gleicher Name von Dokument und Blatt: dann gleicht die Datei Byte für Byte.
    s.document_name = "Blatt 1".into();
    set_info(&mut s, id, |i| {
        i.issued = FieldValue::Text("2026-01-02".into())
    });
    let dir = temp_dir("eins");
    for frame in [false, true] {
        let mut all = all_dialog(&s, frame, compute_view);
        let a = written(&mut all, &s, &dir.join("a.pdf"));
        let mut sel = dialog_with(&s, PDF, frame, id);
        let b = written(&mut sel, &s, &dir.join("b.pdf"));
        assert_eq!(a, b, "{frame}");
    }
    // Mit anderem Dokumentnamen unterscheidet sich nur der Titel.
    s.document_name = "Projekt".into();
    let mut all = all_dialog(&s, true, compute_view);
    let pdf = read(&written(&mut all, &s, &dir.join("c.pdf")));
    assert_eq!(info_text(&pdf, "Title").as_deref(), Some("Projekt"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mesh_hint_appears_once() {
    let mut s = box_state();
    s.import_mesh("Würfel", std::sync::Arc::new(cube()), 1.0)
        .unwrap();
    s.selected_body = None;
    add_drawing(&mut s, "Blatt 1");
    add_drawing(&mut s, "Blatt 2");
    let mut d = all_dialog(&s, false, compute_view);
    assert!(
        matches!(&d.status(&s), ExportStatus::DrawingReady { skipped_meshes, .. } if *skipped_meshes == vec!["Würfel".to_string()]),
        "{:?}",
        d.status(&s)
    );
    let ui = draw(&mut d, &mut s);
    assert_eq!(
        ui.matches("Netz-Körper sind in der Zeichnung nicht enthalten: „Würfel“.")
            .count(),
        1,
        "{ui}"
    );
}

#[test]
fn all_shows_sums_and_centerlines() {
    let (mut s, _) = named_boxes(2);
    let a = add_drawing(&mut s, "Blatt A");
    let b = add_small(&mut s, "Blatt B", A4_HOCH);
    suggest(&mut s, a);
    let ready = |d: &ExportDialog, s: &AppState| match d.status(s) {
        ExportStatus::DrawingReady {
            views,
            visible,
            hidden,
            dimensions,
            centerlines,
            ..
        } => (views, visible, hidden, dimensions, centerlines),
        other => panic!("{other:?}"),
    };
    // Summen = Summe der einzelnen Blätter.
    let one_a = ready(&dialog(&s, PDF, a), &s);
    let one_b = ready(&dialog(&s, PDF, b), &s);
    let all = ready(&all_dialog(&s, false, compute_view), &s);
    assert_eq!(
        (all.0, all.1, all.2, all.3),
        (
            one_a.0 + one_b.0,
            one_a.1 + one_b.1,
            one_a.2 + one_b.2,
            one_a.3 + one_b.3
        )
    );
    assert!(all.3 >= 3, "Maße auf Blatt A");
    assert_eq!(all.4, CenterlineUse::Yes);
    // Blatt B ohne Mittellinien: über alle Blätter „teilweise“.
    s.document
        .update_drawing(b, |dr| {
            for v in &mut dr.views {
                v.centerlines = false;
            }
        })
        .unwrap();
    let mut d = all_dialog(&s, false, compute_view);
    assert_eq!(ready(&d, &s).4, CenterlineUse::Partly);
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains("Mittellinien: teilweise"), "{ui}");
    // Ein Stücklistenblatt (ohne Ansichten) zählt nicht mit.
    s.document
        .update_drawing(b, |dr| {
            for v in &mut dr.views {
                v.centerlines = true;
            }
        })
        .unwrap();
    s.drawing_ui.open(a);
    planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    let d = all_dialog(&s, false, compute_view);
    assert_eq!(ready(&d, &s).4, CenterlineUse::Yes);
}

fn cube() -> planora_exchange::ExchangeMesh {
    planora_exchange::ExchangeMesh {
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

// (b) ------------------------------------------------------------------------------------------

#[test]
fn choice_ui() {
    use common::{Screen, assert_cards_inside, find};
    assert_eq!(PdfPages::default(), PdfPages::Selected);
    assert_eq!(PDF_SELECTED, "Gewähltes Blatt");
    assert_eq!(PDF_ALL, "Alle Blätter");
    assert_eq!(
        PDF_ALL_HINT,
        "Bei „Alle Blätter“ kommen alle Zeichnungen der Reihe nach in die PDF."
    );
    let mut s = box_state();
    let a = add_drawing(&mut s, "Blatt A");
    add_small(&mut s, "Blatt B", A4_HOCH);
    add_small(&mut s, "Blatt C", A4_HOCH);
    add_small(&mut s, "Blatt D", A4_HOCH);
    let fresh = ExportDialog::new(&s);
    assert_eq!(fresh.pdf_pages, PdfPages::Selected, "Vorgabe");

    // SVG und DXF zeigen keine Wahl.
    for format in [SVG, DXF] {
        let mut d = ExportDialog::new(&s);
        d.format = format;
        let ui = draw(&mut d, &mut s);
        assert!(
            !ui.contains(PDF_ALL) && !ui.contains(PDF_SELECTED),
            "{format:?}:\n{ui}"
        );
    }
    // PDF: die Wahl erscheint, Vorgabe „Gewähltes Blatt“, Zeichnungsauswahl offen.
    let mut d = ExportDialog::new(&s);
    d.format = PDF;
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains(PDF_SELECTED) && ui.contains(PDF_ALL), "{ui}");
    assert!(
        !ui.contains(PDF_ALL_HINT) && !ui.contains("Blätter:"),
        "{ui}"
    );
    assert_eq!(d.pdf_pages, PdfPages::Selected);
    assert_eq!(d.drawing, Some(a));

    // Bei 800 × 600: Karten gleich hoch, nichts ragt hinaus; Klick auf „Alle Blätter“ wählt sie.
    let mut screen = Screen::new(800.0, 600.0);
    let mut last = None;
    let seen = screen.settle(8, &mut |ctx| {
        assert!(d.show(ctx, &mut s, &mut last));
    });
    assert_cards_inside(d.last_rect.unwrap(), &d.last_cards, &seen);
    // Gewähltes Blatt: ein Klick auf die Auswahl öffnet die Liste der Zeichnungen.
    let combo = find(&seen, "Blatt A").expect("Auswahl").rect.center();
    let after = {
        let mut f = |ctx: &egui::Context| {
            assert!(d.show(ctx, &mut s, &mut last));
        };
        screen.click(combo, &mut f);
        screen.settle(2, &mut f)
    };
    assert!(find(&after, "Blatt C").is_some(), "Liste offen");
    // Liste wieder schließen (zweiter Klick auf die Auswahl).
    let after = {
        let mut f = |ctx: &egui::Context| {
            assert!(d.show(ctx, &mut s, &mut last));
        };
        screen.click(combo, &mut f);
        screen.settle(3, &mut f)
    };
    assert!(find(&after, "Blatt C").is_none(), "Liste zu");
    assert_eq!(d.drawing, Some(a), "Auswahl unverändert");
    let all_button = find(&after, PDF_ALL)
        .expect("Wahl „Alle Blätter“")
        .rect
        .center();
    let seen = {
        let mut f = |ctx: &egui::Context| {
            assert!(d.show(ctx, &mut s, &mut last));
        };
        screen.click(all_button, &mut f);
        screen.settle(8, &mut f)
    };
    assert_eq!(d.pdf_pages, PdfPages::All, "Klick wählt alle Blätter");
    assert_cards_inside(d.last_rect.unwrap(), &d.last_cards, &seen);
    assert!(d.last_rect.is_some_and(|r| {
        egui::Rect::from_min_size(egui::Pos2::ZERO, screen.size).contains_rect(r)
    }));
    let words: Vec<&str> = seen.iter().map(|x| x.text.as_str()).collect();
    for want in [PDF_ALL_HINT, "Blätter: 4"] {
        assert!(
            seen.iter().any(|x| x.text == want),
            "„{want}“ fehlt: {words:?}"
        );
    }
    // Bei „Alle Blätter“ ist die Auswahl gesperrt: ein Klick öffnet keine Liste.
    let combo = find(&seen, "Blatt A").expect("Auswahl").rect.center();
    let after = {
        let mut f = |ctx: &egui::Context| {
            assert!(d.show(ctx, &mut s, &mut last));
        };
        screen.click(combo, &mut f);
        screen.settle(2, &mut f)
    };
    assert!(find(&after, "Blatt C").is_none(), "Auswahl gesperrt");
    // Angaben: Summen über alle Blätter.
    d.tick(&s, true);
    let ui = draw(&mut d, &mut s);
    for want in ["Blätter: 4", "Ansichten: 7", "Alle Blätter sind berechnet."] {
        assert!(ui.contains(want), "„{want}“ fehlt:\n{ui}");
    }
    assert!(!ui.contains("Blatt: A3 quer"), "{ui}");
    // Andere Formate zeigen die Wahl auch dann nicht, wenn „Alle Blätter“ gewählt ist.
    d.format = SVG;
    let ui = draw(&mut d, &mut s);
    assert!(!ui.contains(PDF_ALL) && !ui.contains(PDF_ALL_HINT), "{ui}");
    assert!(
        ui.contains("Blatt: A3 quer"),
        "SVG gilt dem gewählten Blatt:\n{ui}"
    );
}

// (c) ------------------------------------------------------------------------------------------

/// Fünf kleine Blätter „Blatt 1“ … „Blatt 5“ mit Vorderansicht; Blatt 4 zusätzlich „Oben“.
fn five_sheets() -> (AppState, Vec<planora_feature::FeatureId>, Vec<DrawingId>) {
    let (mut s, f) = named_boxes(3);
    let ids: Vec<DrawingId> = (1..=5)
        .map(|i| add_small(&mut s, &format!("Blatt {i}"), A4_HOCH))
        .collect();
    let front = front_of(&s, ids[3]);
    s.document
        .add_view(
            ids[3],
            ViewKind::Top,
            one(1, 1),
            Placement::Aligned { distance: 60.0 },
            Some(front),
        )
        .unwrap();
    s.document_name = "Fünf".into();
    (s, f, ids)
}

fn fail_top(b: &[DrawBody], k: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    if k == ViewKind::Top {
        Err(DrawError::TooComplex(123))
    } else {
        first_body(b, k, p)
    }
}

/// Verweisdaten der geraden Kante des Körpers `body`, deren beide Eckpunkte `pred` erfüllen.
fn edge_where(
    s: &AppState,
    body: usize,
    pred: impl Fn(DVec3) -> bool,
) -> planora_drawing::dimension::GeoRefData {
    let solid = &s.bodies[body].solid;
    let (id, _) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && pred(a) && pred(b)
        })
        .expect("Kante nicht gefunden");
    planora_feature::geo_ref_for_edge(solid, id, &planora_math::Tolerance::default()).unwrap()
}

/// Waagerechtes Maß über die Breite des Körpers `body` in der Ansicht `view`.
fn width_dim(s: &AppState, body: usize, view: ViewId) -> planora_drawing::dimension::DimSpec {
    use planora_drawing::dimension::{DimSpec, GeoRef, Tolerance as DimTolerance};
    let (lo, hi) = s.bodies[body].bounds;
    let n = |a: f64, b: f64| (a - b).abs() < 1e-6;
    DimSpec {
        id: DimId(0),
        view,
        kind: DimKind::Horizontal,
        refs: vec![
            GeoRef::Edge(edge_where(s, body, |p| n(p.x, lo.x) && n(p.y, lo.y))),
            GeoRef::Edge(edge_where(s, body, |p| n(p.x, hi.x) && n(p.y, lo.y))),
        ],
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

/// Status und Schreibversuch ergeben `msg`; keine Datei, keine Hilfsdatei.
fn locked(d: &mut ExportDialog, s: &mut AppState, msg: &str, dir: &Path) {
    d.tick(s, true);
    assert_eq!(d.status(s), ExportStatus::Failed(msg.to_string()));
    let target = dir.join("gesperrt.pdf");
    d.write_to(target.clone(), s);
    d.tick(s, true);
    assert_eq!(d.write_state(), WriteState::Failed(msg.to_string()));
    assert!(!target.exists(), "keine Datei");
    assert_eq!(temp_files(dir), 0);
    let ui = draw(d, s);
    assert!(ui.contains(msg), "Meldung im Fenster:\n{ui}");
}

#[test]
fn one_broken_sheet_locks_everything() {
    assert_eq!(in_sheet("Blatt 3", "X."), "Im Blatt „Blatt 3“: X.");
    let (mut s, f, ids) = five_sheets();
    let dir = temp_dir("kaputt");
    let mut d = all_dialog(&s, true, first_body);
    assert!(
        matches!(d.status(&s), ExportStatus::DrawingReady { .. }),
        "{:?}",
        d.status(&s)
    );

    // Verlorene Positionsnummer auf Blatt 3 von 5.
    let front3 = front_of(&s, ids[2]);
    let b = s
        .document
        .add_balloon(ids[2], front3, body_key(f[2]), (0.5, 0.5), (-15.0, 10.0))
        .unwrap();
    s.delete_feature(f[2]).unwrap();
    s.selected_body = None;
    let lost = in_sheet("Blatt 3", &lost_balloon_text("Vorne"));
    locked(&mut d, &mut s, &lost, &dir);
    // Dazu ein Emoji auf Blatt 5: weiterhin die Meldung von Blatt 3 (erster Fehler).
    set_title(&mut s, ids[4], "Deckel 😀");
    locked(&mut d, &mut s, &lost, &dir);
    // Kreis gelöscht: jetzt meldet Blatt 5 sein Zeichen.
    s.document
        .update_drawing(ids[2], |dr| {
            dr.remove_balloon(b);
        })
        .unwrap();
    locked(
        &mut d,
        &mut s,
        &in_sheet("Blatt 5", &pdf_char_text('😀', "Titel")),
        &dir,
    );
    set_title(&mut s, ids[4], "Deckel");

    // Verlorenes Maß auf Blatt 2 (am Körper „Teil 2“, der dann verschwindet).
    let front2 = front_of(&s, ids[1]);
    let spec = width_dim(&s, 1, front2);
    let dim = s.document.add_dimension(ids[1], spec).unwrap();
    d.tick(&s, true);
    assert!(
        matches!(d.status(&s), ExportStatus::DrawingReady { .. }),
        "{:?}",
        d.status(&s)
    );
    s.delete_feature(f[1]).unwrap();
    s.selected_body = None;
    let reason = "Die Kante, an der das Maß hing, gibt es nicht mehr";
    let msg = format!(
        "Das Maß „Waagerecht“ in der Ansicht „Vorne“ hat seinen Bezug verloren: {reason}. Bitte das Maß löschen und neu setzen."
    );
    locked(&mut d, &mut s, &in_sheet("Blatt 2", &msg), &dir);
    s.document.delete_dimension(ids[1], dim).unwrap();

    // Nicht berechenbare Ansicht auf Blatt 4.
    let mut bad = all_dialog(&s, true, fail_top);
    let view = format!(
        "Die Ansicht „Oben“ lässt sich nicht berechnen: {}",
        DrawError::TooComplex(123)
    );
    locked(&mut bad, &mut s, &in_sheet("Blatt 4", &view), &dir);

    // Alles behoben: eine PDF mit fünf Seiten.
    let pdf = read(&written(&mut d, &s, &dir.join("ok.pdf")));
    assert_eq!(pdf.pages.len(), 5);
    for i in 0..5 {
        for want in [format!("Fünf-0{}", i + 1), format!("{} von 5", i + 1)] {
            assert!(
                texts(&pdf, i).contains(&want),
                "Seite {i}: {want}: {:?}",
                texts(&pdf, i)
            );
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn char_in_the_follow_up_parts_sheet_names_that_sheet() {
    let (s0, f, ids) = getriebe();
    let mut s = s0;
    let dir = temp_dir("folgeblatt");
    // Zeile 45 steht auf dem Folgeblatt „Stückliste (2)“.
    set_name_override(&mut s, f[44], "Ā");
    let cell = pdf_char_text('Ā', "Stückliste, Pos. 45, Benennung");
    let follow = s.document.drawing(ids[2]).unwrap().name.clone();
    assert_eq!(follow, "Stückliste (2)");
    for frame in [true, false] {
        let mut d = all_dialog(&s, frame, first_body);
        locked(&mut d, &mut s, &in_sheet(&follow, &cell), &dir);
    }
    let _ = fs::remove_dir_all(&dir);
}

// (d) ------------------------------------------------------------------------------------------

/// Rechnet jede Ansichtsart nur einmal (viele gleiche Blätter für die Grenze von 200 Seiten).
fn cached(bodies: &[DrawBody], kind: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    static CACHE: std::sync::Mutex<Vec<(ViewKind, ViewLines)>> = std::sync::Mutex::new(Vec::new());
    let mut c = CACHE.lock().unwrap();
    if let Some((_, l)) = c.iter().find(|(k, _)| *k == kind) {
        return Ok(l.clone());
    }
    let l = compute_view(&bodies[..1], kind, p)?;
    c.push((kind, l.clone()));
    Ok(l)
}

#[test]
fn blocked_before_computing() {
    assert_eq!(
        too_many_pages_text(201),
        "Eine PDF kann höchstens 200 Blätter enthalten, das Dokument hat 201. Bitte einzelne Blätter ausgeben."
    );
    assert_eq!(pages_label(1), "1 Blatt");
    assert_eq!(pages_label(3), "3 Blätter");
    // Ansichtenzeichnung ohne Ansicht an Stelle 2: gesperrt mit der vorhandenen Meldung.
    let mut s = box_state();
    let first = add_drawing(&mut s, "Blatt 1");
    s.document
        .add_drawing("Leer", A4_HOCH, Projection::First)
        .unwrap();
    add_small(&mut s, "Blatt 3", A4_HOCH);
    let empty = "Die Zeichnung „Leer“ hat noch keine Ansicht.";
    let dir = temp_dir("sperre");
    let mut d = all_dialog(&s, false, compute_view);
    assert_eq!(d.status(&s), ExportStatus::Unavailable(empty.into()));
    d.write_to(dir.join("x.pdf"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(empty.into()));
    // Das gewählte Blatt 1 geht.
    let sel = dialog(&s, PDF, first);
    assert!(matches!(sel.status(&s), ExportStatus::DrawingReady { .. }));

    // 200 Blätter gehen (eine PDF mit 200 Seiten), 201 sind gesperrt.
    let mut s = box_state();
    for i in 1..=200 {
        add_small(&mut s, &format!("B{i}"), A4_HOCH);
    }
    let mut d = all_dialog(&s, false, cached);
    assert!(
        matches!(&d.status(&s), ExportStatus::DrawingReady { sheet, .. } if sheet == "200 Blätter"),
        "{:?}",
        d.status(&s)
    );
    let pdf = read(&written(&mut d, &s, &dir.join("200.pdf")));
    assert_eq!(pdf.pages.len(), 200);
    let extra = add_small(&mut s, "B201", A4_HOCH);
    let too_many = too_many_pages_text(201);
    d.tick(&s, true);
    assert_eq!(d.status(&s), ExportStatus::Unavailable(too_many.clone()));
    d.write_to(dir.join("201.pdf"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(too_many.clone()));
    assert!(!dir.join("201.pdf").exists());
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains(&too_many), "{ui}");
    // Das gewählte Blatt bleibt möglich.
    let sel = dialog(&s, PDF, extra);
    assert!(matches!(sel.status(&s), ExportStatus::DrawingReady { .. }));
    // Wieder 200 (eine ohne Ansicht dazu, eine andere weg): nicht die Grenze, sondern die Ansicht.
    s.document.delete_drawing(extra).unwrap();
    let ids: Vec<DrawingId> = s.document.drawings().iter().map(|d| d.id).collect();
    s.document.delete_drawing(ids[0]).unwrap();
    s.document
        .add_drawing("Ohne", A4_HOCH, Projection::First)
        .unwrap();
    assert_eq!(s.document.drawings().len(), 200);
    d.tick(&s, true);
    assert_eq!(
        d.status(&s),
        ExportStatus::Unavailable("Die Zeichnung „Ohne“ hat noch keine Ansicht.".into())
    );
    let _ = fs::remove_dir_all(&dir);
}

// (e) ------------------------------------------------------------------------------------------

static KEY_COUNT: AtomicUsize = AtomicUsize::new(0);

fn key_counting(b: &[DrawBody], kind: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    KEY_COUNT.fetch_add(1, Ordering::SeqCst);
    first_body(b, kind, p)
}

/// Titel (Feld „Titel“ = Name des Blatts) je Seite.
fn page_titles(pdf: &pdf_read::Pdf, names: &[&str]) -> Vec<String> {
    (0..pdf.pages.len())
        .map(|i| {
            texts(pdf, i)
                .into_iter()
                .find(|t| names.contains(&t.as_str()))
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn stale_and_key_coverage() {
    let (mut s, _) = named_boxes(1);
    let a = add_small(&mut s, "Alpha", A4_HOCH);
    let b = add_small(&mut s, "Beta", A4_HOCH);
    add_small(&mut s, "Gamma", A4_HOCH);
    s.document_name = "Dok".into();
    let names = ["Alpha", "Beta", "Gamma", "Delta", "Neu"];
    let dir = temp_dir("stand");
    let mut d = all_dialog(&s, true, key_counting);
    let pdf = read(&written(&mut d, &s, &dir.join("0.pdf")));
    assert_eq!(page_titles(&pdf, &names), ["Alpha", "Beta", "Gamma"]);
    let first = fs::read(dir.join("0.pdf")).unwrap();

    // Je eine Änderung: das alte Ergebnis wird nie geschrieben, die neue Datei passt.
    let stale = |d: &mut ExportDialog, s: &AppState, what: &str| {
        d.write_to(dir.join("stale.pdf"), s);
        assert_eq!(d.write_state(), WriteState::Failed(STALE.into()), "{what}");
        assert!(!dir.join("stale.pdf").exists(), "{what}");
        read(&written(d, s, &dir.join(format!("{what}.pdf"))))
    };
    let dd = add_small(&mut s, "Delta", A4_HOCH);
    let pdf = stale(&mut d, &s, "hinzu");
    assert_eq!(
        page_titles(&pdf, &names),
        ["Alpha", "Beta", "Gamma", "Delta"]
    );
    assert!(texts(&pdf, 3).contains(&"4 von 4".to_string()));
    s.document.delete_drawing(b).unwrap();
    let pdf = stale(&mut d, &s, "weg");
    assert_eq!(page_titles(&pdf, &names), ["Alpha", "Gamma", "Delta"]);
    s.document.rename_drawing(dd, "Neu").unwrap();
    let pdf = stale(&mut d, &s, "umbenannt");
    assert_eq!(page_titles(&pdf, &names), ["Alpha", "Gamma", "Neu"]);
    s.document_name = "Anderes".into();
    let pdf = stale(&mut d, &s, "dokument");
    assert_eq!(info_text(&pdf, "Title").as_deref(), Some("Anderes"));
    assert!(
        texts(&pdf, 0).contains(&"Anderes-01".to_string()),
        "{:?}",
        texts(&pdf, 0)
    );
    // Ein einzelnes Blatt geändert (Blattangaben des letzten).
    set_info(&mut s, dd, |i| {
        i.subtitle = FieldValue::Text("Zusatz".into())
    });
    let pdf = stale(&mut d, &s, "blatt");
    assert!(texts(&pdf, 2).contains(&"Zusatz".to_string()));
    // Rahmen-Schalter.
    d.with_frame = false;
    let pdf = stale(&mut d, &s, "rahmen");
    assert!(texts(&pdf, 0).is_empty());
    assert_eq!(pdf.pages.len(), 3);
    d.with_frame = true;
    let pdf = stale(&mut d, &s, "rahmen2");
    assert_eq!(page_titles(&pdf, &names), ["Alpha", "Gamma", "Neu"]);
    // Gewählt ↔ alle.
    d.pdf_pages = PdfPages::Selected;
    d.drawing = Some(a);
    let pdf = stale(&mut d, &s, "gewaehlt");
    assert_eq!(pdf.pages.len(), 1);
    assert_eq!(info_text(&pdf, "Title").as_deref(), Some("Alpha"));
    d.pdf_pages = PdfPages::All;
    let pdf = stale(&mut d, &s, "alle");
    assert_eq!(pdf.pages.len(), 3);
    // Eine andere Zeichnung in der Auswahl rechnet bei „Alle Blätter“ nicht neu: das alte
    // Ergebnis gilt weiter.
    let before = KEY_COUNT.load(Ordering::SeqCst);
    let old = fs::read(dir.join("alle.pdf")).unwrap();
    d.drawing = Some(dd);
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    let again = written(&mut d, &s, &dir.join("auswahl.pdf"));
    assert_eq!(
        KEY_COUNT.load(Ordering::SeqCst),
        before,
        "keine Neuberechnung"
    );
    // Gleiche Datei (bis auf ein mögliches neues Tagesdatum, das hier nicht vorkommt: Datum
    // aus dem ersten Blatt).
    assert_eq!(again, old);
    assert_ne!(again, first);
    let _ = fs::remove_dir_all(&dir);
}

// (f) ------------------------------------------------------------------------------------------

static GATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static GATE_OPEN: AtomicUsize = AtomicUsize::new(0);
static GATE_EXITS: AtomicUsize = AtomicUsize::new(0);

/// Jede Rechnung wartet, bis sie freigegeben ist (die k-te ab `GATE_OPEN > k`), oder bis
/// abgebrochen wird.
fn gated(b: &[DrawBody], kind: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    let me = GATE_CALLS.fetch_add(1, Ordering::SeqCst);
    loop {
        if GATE_OPEN.load(Ordering::SeqCst) > me {
            return first_body(b, kind, p);
        }
        if p.is_cancelled() {
            GATE_EXITS.fetch_add(1, Ordering::SeqCst);
            return Err(DrawError::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !f() {
        assert!(start.elapsed().as_secs() < 30, "{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn progress_names_the_sheet() {
    use common::{Screen, find};
    assert_eq!(sheet_progress_text(2, 3), "Blatt 2 von 3 wird berechnet …");
    let (mut s, _) = named_boxes(1);
    for i in 1..=3 {
        add_small(&mut s, &format!("Blatt {i}"), A4_HOCH);
    }
    let dir = temp_dir("fortschritt");
    let mut d = ExportDialog::new(&s);
    d.set_compute(gated);
    d.format = PDF;
    d.pdf_pages = PdfPages::All;
    d.tick(&s, false);
    assert_eq!(d.computing_sheet(), Some((1, 3)));
    wait_for("Blatt 1 rechnet", || GATE_CALLS.load(Ordering::SeqCst) >= 1);
    assert_eq!(d.computing_sheet(), Some((1, 3)));
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    // Blatt 1 fertig: Blatt 2 von 3, der Balken steht bei mindestens einem Drittel.
    GATE_OPEN.store(1, Ordering::SeqCst);
    wait_for("Blatt 2 rechnet", || GATE_CALLS.load(Ordering::SeqCst) >= 2);
    assert_eq!(d.computing_sheet(), Some((2, 3)));
    wait_for(
        "Balken",
        || matches!(d.status(&s), ExportStatus::Computing(f) if f >= 1.0 / 3.0 - 1e-3),
    );
    let ExportStatus::Computing(f) = d.status(&s) else {
        panic!("{:?}", d.status(&s));
    };
    assert!(f < 2.0 / 3.0, "{f}");
    let mut screen = Screen::new(1200.0, 800.0);
    let mut last = None;
    let seen = screen.settle(3, &mut |ctx| {
        assert!(d.show(ctx, &mut s, &mut last));
    });
    let words: Vec<&str> = seen.iter().map(|x| x.text.as_str()).collect();
    assert!(
        find(&seen, &sheet_progress_text(2, 3)).is_some(),
        "{words:?}"
    );
    // „Exportieren …“ ist gesperrt: ein Klick öffnet keine Dateiauswahl.
    let export = find(&seen, "Exportieren …").expect("Knopf").rect.center();
    let mut f = |ctx: &egui::Context| {
        assert!(d.show(ctx, &mut s, &mut last));
    };
    screen.click(export, &mut f);
    drop(f);
    assert!(d.chooser().is_none(), "gesperrt während der Berechnung");
    // „Abbrechen“: abgebrochen, keine Datei; der Auftrag endet.
    let seen = screen.settle(1, &mut |ctx| {
        assert!(d.show(ctx, &mut s, &mut last));
    });
    let cancel = find(&seen, "Abbrechen").expect("Abbrechen").rect.center();
    let mut f = |ctx: &egui::Context| {
        assert!(d.show(ctx, &mut s, &mut last));
    };
    screen.click(cancel, &mut f);
    drop(f);
    assert_eq!(d.status(&s), ExportStatus::Failed("Abgebrochen".into()));
    assert_eq!(d.computing_sheet(), None);
    wait_for("Auftrag endet", || GATE_EXITS.load(Ordering::SeqCst) >= 1);
    d.write_to(dir.join("a.pdf"), &s);
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed("Abgebrochen".into()));
    assert_eq!(names(&dir), Vec::<String>::new());
    // Nach „Neu berechnen“ und Freigabe aller Rechnungen: drei Seiten.
    GATE_OPEN.store(usize::MAX, Ordering::SeqCst);
    d.recompute();
    let pdf = read(&written(&mut d, &s, &dir.join("b.pdf")));
    assert_eq!(pdf.pages.len(), 3);
    let _ = fs::remove_dir_all(&dir);
}

fn says_cancelled(_: &[DrawBody], _: ViewKind, _: &Progress) -> Result<ViewLines, DrawError> {
    Err(DrawError::Cancelled)
}

#[test]
fn prepare_all_sheets_counts_and_cancels() {
    let (s, _, ids) = five_sheets();
    let jobs: Vec<SheetJob> = ids
        .iter()
        .map(|id| {
            let d = s.document.drawing(*id).unwrap().clone();
            SheetJob {
                parts: parts_input(&s, &d),
                drawing: d,
                frame: None,
            }
        })
        .collect();
    let solids: Vec<(String, planora_kernel::Solid)> = s
        .bodies
        .iter()
        .map(|b| (b.name.clone(), b.solid.clone()))
        .collect();
    let counter = AtomicUsize::new(0);
    let p = Progress::new();
    let r = prepare_all_sheets(
        &jobs,
        &solids,
        first_body,
        resolve_dimensions_in_worker,
        &counter,
        &p,
    )
    .unwrap();
    assert_eq!(r.len(), 5);
    assert_eq!(counter.load(Ordering::SeqCst), 5);
    assert!((p.fraction() - 1.0).abs() < 1e-6);
    // Reihenfolge = Reihenfolge der Aufträge.
    assert_eq!(r[3].views, 2);
    assert!(
        r.iter()
            .enumerate()
            .all(|(i, x)| (x.views == 2) == (i == 3))
    );
    // Abgebrochen (auch mitten in einem Blatt): „Abgebrochen“ ohne Blattnamen.
    let counter = AtomicUsize::new(0);
    assert_eq!(
        prepare_all_sheets(
            &jobs,
            &solids,
            says_cancelled,
            resolve_dimensions_in_worker,
            &counter,
            &Progress::new()
        )
        .unwrap_err(),
        "Abgebrochen"
    );
    assert_eq!(counter.load(Ordering::SeqCst), 1, "beim ersten Blatt");
    let p = Progress::new();
    p.cancel();
    let counter = AtomicUsize::new(0);
    assert_eq!(
        prepare_all_sheets(
            &jobs,
            &solids,
            first_body,
            resolve_dimensions_in_worker,
            &counter,
            &p
        )
        .unwrap_err(),
        "Abgebrochen"
    );
}

// (g) ------------------------------------------------------------------------------------------

static STUCK_STARTED: AtomicUsize = AtomicUsize::new(0);
static STUCK_EXITS: AtomicUsize = AtomicUsize::new(0);

fn stuck(_: &[DrawBody], _: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    STUCK_STARTED.fetch_add(1, Ordering::SeqCst);
    loop {
        if p.is_cancelled() {
            STUCK_EXITS.fetch_add(1, Ordering::SeqCst);
            return Err(DrawError::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn tab_close_during_all() {
    use planora_app::workspace::Workspace;
    use planora_project::config::Config;
    // Während des Rechnens: der Auftrag endet.
    let mut w = Workspace::new(None, Config::default());
    let tab = w.active_tab_mut();
    tab.state.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    add_small(&mut tab.state, "Blatt 1", A4_HOCH);
    add_small(&mut tab.state, "Blatt 2", A4_HOCH);
    let mut d = ExportDialog::new(&tab.state);
    d.set_compute(stuck);
    d.format = PDF;
    d.pdf_pages = PdfPages::All;
    d.tick(&tab.state, false);
    tab.export_dialog = Some(d);
    wait_for("Rechnung läuft", || {
        STUCK_STARTED.load(Ordering::SeqCst) >= 1
    });
    assert_eq!(STUCK_EXITS.load(Ordering::SeqCst), 0);
    assert!(!w.request_close(0));
    w.discard_and_continue();
    assert!(w.tabs[0].export_dialog.is_none());
    wait_for("Auftrag endet", || STUCK_EXITS.load(Ordering::SeqCst) >= 1);
    // Nur ein Blatt wurde angefangen (Blatt 2 nie).
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(STUCK_STARTED.load(Ordering::SeqCst), 1);

    // Während des Schreibens: keine Hilfsdatei bleibt.
    let mut w = Workspace::new(None, Config::default());
    let dir = temp_dir("tab-alle");
    let target = dir.join("alt.pdf");
    fs::write(&target, "alt").unwrap();
    let tab = w.active_tab_mut();
    tab.state.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    for i in 1..=3 {
        add_drawing(&mut tab.state, &format!("Blatt {i}"));
    }
    let mut d = all_dialog(&tab.state, true, compute_view);
    d.write_to(target.clone(), &tab.state);
    assert!(d.is_writing_file());
    tab.export_dialog = Some(d);
    let start = std::time::Instant::now();
    let finished = |t: &std::io::Result<Vec<u8>>| t.as_ref().is_ok_and(|t| t != b"alt");
    while temp_files(&dir) == 0 && !finished(&fs::read(&target)) && start.elapsed().as_secs() < 10 {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!w.request_close(0));
    w.discard_and_continue();
    assert!(w.tabs[0].export_dialog.is_none());
    wait_for("Hilfsdatei weg", || temp_files(&dir) == 0);
    let mut bytes = fs::read(&target);
    for _ in 0..100 {
        if bytes.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        bytes = fs::read(&target);
    }
    let bytes = bytes.unwrap();
    assert!(bytes == b"alt" || read(&bytes).pages.len() == 3);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancel_while_writing_all_keeps_the_old_file() {
    let mut s = box_state();
    add_drawing(&mut s, "Blatt 1");
    add_drawing(&mut s, "Blatt 2");
    let dir = temp_dir("abbruch-alle");
    let path = dir.join("z.pdf");
    fs::write(&path, "alt").unwrap();
    let mut d = all_dialog(&s, false, compute_view);
    d.set_pdf_writer(slow_pdf_writer);
    d.write_to(path.clone(), &s);
    assert!(!d.request_close());
    d.cancel_write();
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed("Abgebrochen".into()));
    assert_eq!(fs::read_to_string(&path).unwrap(), "alt");
    assert_eq!(temp_files(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

// Aus der Prüfung von Aufgabe 4 -------------------------------------------------------------

#[test]
fn selected_non_default_sheet_names_the_pdf() {
    let mut s = box_state();
    let first = add_drawing(&mut s, "Blatt 1");
    let second = add_drawing(&mut s, "Blatt 2");
    s.document_name = "Projekt X".into();
    set_title(&mut s, second, "Anderer Titel");
    // Vorgabe ist das erste Blatt; gewählt ist das zweite.
    assert_eq!(ExportDialog::new(&s).drawing, Some(first));
    let dir = temp_dir("zweites");
    let mut d = dialog_with(&s, PDF, true, second);
    let pdf = read(&written(&mut d, &s, &dir.join("z.pdf")));
    assert_eq!(info_text(&pdf, "Title").as_deref(), Some("Blatt 2"));
    assert!(texts(&pdf, 0).contains(&"Anderer Titel".to_string()));
    assert!(texts(&pdf, 0).contains(&"2 von 2".to_string()));
    let _ = (fs::remove_dir_all(&dir), PDF_DRAWING);
}

#[test]
fn char_lock_single_word_fields_and_quotes() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("einwort");
    // Werkstoff aus den Projektangaben.
    let mut p = s
        .document
        .project_info()
        .cloned()
        .unwrap_or_else(|| ProjectInfo::new(&stamp()));
    p.material = FieldValue::Text("S235😀".into());
    s.document.set_project_info(p.clone()).unwrap();
    let d = dialog_with(&s, PDF, true, id);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(pdf_char_text('😀', "Werkstoff"))
    );
    p.material = FieldValue::Text("S235".into());
    s.document.set_project_info(p).unwrap();
    // Zeichnungsnummer aus dem Dokumentnamen.
    s.document_name = "Getriebe😀".into();
    let d = dialog_with(&s, PDF, true, id);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(pdf_char_text('😀', "Zeichnungsnummer"))
    );
    let mut all = all_dialog(&s, true, compute_view);
    locked(
        &mut all,
        &mut s,
        &in_sheet("Blatt 1", &pdf_char_text('😀', "Zeichnungsnummer")),
        &dir,
    );
    s.document_name = "Getriebe".into();
    // Typografische Anführungszeichen gehen.
    let quoted = "„Halter“ ‘A’";
    set_title(&mut s, id, quoted);
    let mut d = dialog_with(&s, PDF, true, id);
    let pdf = read(&written(&mut d, &s, &dir.join("q.pdf")));
    assert!(
        texts(&pdf, 0).contains(&quoted.to_string()),
        "{:?}",
        texts(&pdf, 0)
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn author_is_trimmed() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    set_creator(&mut s, FieldValue::Text(" Erika ".into()));
    assert_eq!(
        pdf_meta(&s, PdfPages::Selected, Some(id), "2026-05-06").author,
        Some("Erika".into())
    );
    let dir = temp_dir("autor");
    let mut d = dialog(&s, PDF, id);
    let pdf = read(&written(&mut d, &s, &dir.join("a.pdf")));
    assert_eq!(info_text(&pdf, "Author").as_deref(), Some("Erika"));
    let _ = fs::remove_dir_all(&dir);
}

// Nachbesserung nach der Prüfung von Aufgabe 5 ------------------------------------------------

/// Status-Summen und Fenstertext über alle Blätter: das zweite Blatt trägt verdeckte Linien,
/// Maße und ein Maß außerhalb des Blatts bei (das erste keines außerhalb).
#[test]
fn all_sums_cover_every_sheet() {
    let (mut s, _) = named_boxes(2);
    let a = add_drawing(&mut s, "Blatt A");
    let b = add_small(&mut s, "Blatt B", A4_HOCH);
    let iso = s
        .document
        .add_view(
            b,
            ViewKind::Isometric,
            one(1, 2),
            Placement::Free { x: 150.0, y: 60.0 },
            None,
        )
        .unwrap();
    s.document
        .update_drawing(b, |dr| {
            dr.views
                .iter_mut()
                .find(|v| v.id == iso)
                .unwrap()
                .show_hidden = true;
        })
        .unwrap();
    let front = front_of(&s, b);
    s.document
        .add_dimension(b, width_dim(&s, 0, front))
        .unwrap();
    let mut far = width_dim(&s, 0, front);
    far.offset = 400.0;
    s.document.add_dimension(b, far).unwrap();
    type Sums = (usize, usize, usize, usize, usize);
    let sums = |st: ExportStatus| -> Sums {
        match st {
            ExportStatus::DrawingReady {
                views,
                visible,
                hidden,
                dimensions,
                outside,
                ..
            } => (views, visible, hidden, dimensions, outside),
            other => panic!("{other:?}"),
        }
    };
    let sa = sums(dialog(&s, PDF, a).status(&s));
    let sb = sums(dialog(&s, PDF, b).status(&s));
    assert_eq!(sa.4, 0, "Blatt A ohne Maß außerhalb");
    assert!(sb.2 > 0 && sb.3 == 2 && sb.4 >= 1, "Blatt B: {sb:?}");
    let mut d = all_dialog(&s, false, compute_view);
    let all = sums(d.status(&s));
    assert_eq!(
        all,
        (
            sa.0 + sb.0,
            sa.1 + sb.1,
            sa.2 + sb.2,
            sa.3 + sb.3,
            sa.4 + sb.4
        )
    );
    let ui = draw(&mut d, &mut s);
    for want in [
        format!("Ansichten: {}", all.0),
        format!(
            "Linien: {} sichtbar, {} verdeckt",
            group_digits(all.1 as u64),
            group_digits(all.2 as u64)
        ),
        format!("Maße: {}", all.3),
        format!("Maße außerhalb des Blatts: {}", all.4),
        outside_text(all.4),
    ] {
        assert!(ui.contains(&want), "„{want}“ fehlt:\n{ui}");
    }
}

fn panic_right(b: &[DrawBody], k: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    if k == ViewKind::Right {
        panic!("Testabsturz in der Ansicht");
    }
    first_body(b, k, p)
}

fn panic_resolve_blatt_2(
    d: &planora_drawing::Drawing,
    solids: &[&planora_kernel::Solid],
    cut: &ViewSolids,
    p: &Progress,
) -> Result<planora_drawing::annotate::ResolvedDims, DrawError> {
    if d.name == "Blatt 2" {
        panic!("Testabsturz beim Auflösen");
    }
    resolve_dimensions_in_worker(d, solids, cut, p)
}

/// Stürzt die Rechnung eines Blatts ab, wird nichts ausgelassen: die Meldung nennt das Blatt (F2).
#[test]
fn crashing_sheet_is_named_and_never_skipped() {
    let (mut s, _) = named_boxes(1);
    let ids: Vec<DrawingId> = (1..=3)
        .map(|i| add_small(&mut s, &format!("Blatt {i}"), A4_HOCH))
        .collect();
    let dir = temp_dir("absturz");
    // Absturz außerhalb der Ansichtsrechnung (beim Auflösen der Maße von Blatt 2).
    let front = front_of(&s, ids[1]);
    let dim = s
        .document
        .add_dimension(ids[1], width_dim(&s, 0, front))
        .unwrap();
    let mut d = ExportDialog::new(&s);
    d.set_compute(first_body);
    d.set_resolve(panic_resolve_blatt_2);
    d.format = PDF;
    d.pdf_pages = PdfPages::All;
    locked(&mut d, &mut s, &in_sheet("Blatt 2", LOST), &dir);
    s.document.delete_dimension(ids[1], dim).unwrap();
    // Absturz in einer Ansicht von Blatt 2.
    let front = front_of(&s, ids[1]);
    s.document
        .add_view(
            ids[1],
            ViewKind::Right,
            one(1, 1),
            Placement::Aligned { distance: 60.0 },
            Some(front),
        )
        .unwrap();
    let mut d = all_dialog(&s, false, panic_right);
    locked(&mut d, &mut s, &in_sheet("Blatt 2", LOST), &dir);
    let _ = fs::remove_dir_all(&dir);
}

/// Mehrere Ansichtenzeichnungen ohne Ansicht: gemeldet wird die erste.
#[test]
fn no_view_lock_names_the_first_such_sheet() {
    let mut s = box_state();
    add_small(&mut s, "Blatt 1", A4_HOCH);
    for name in ["Leer A", "Blatt 3", "Leer B"] {
        if name.starts_with("Leer") {
            s.document
                .add_drawing(name, A4_HOCH, Projection::First)
                .unwrap();
        } else {
            add_small(&mut s, name, A4_HOCH);
        }
    }
    let d = all_dialog(&s, false, compute_view);
    assert_eq!(
        d.status(&s),
        ExportStatus::Unavailable("Die Zeichnung „Leer A“ hat noch keine Ansicht.".into())
    );
}

/// Ein Wechsel der Wahl „Gewähltes Blatt“/„Alle Blätter“ im Fenster löscht die Meldung „Fertig“.
#[test]
fn page_choice_clears_the_done_message() {
    use common::{Screen, find};
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    add_small(&mut s, "Blatt 2", A4_HOCH);
    let dir = temp_dir("fertig");
    let mut d = dialog(&s, PDF, id);
    written(&mut d, &s, &dir.join("f.pdf"));
    let mut screen = Screen::new(1200.0, 800.0);
    let mut last = None;
    let seen = screen.settle(3, &mut |ctx| {
        assert!(d.show(ctx, &mut s, &mut last));
    });
    let done_text = |seen: &[common::Seen]| seen.iter().any(|x| x.text.starts_with("Fertig:"));
    assert!(done_text(&seen), "Fertig vor dem Wechsel");
    let all = find(&seen, PDF_ALL).expect("Wahl").rect.center();
    let seen = {
        let mut f = |ctx: &egui::Context| {
            assert!(d.show(ctx, &mut s, &mut last));
        };
        screen.click(all, &mut f);
        screen.settle(2, &mut f)
    };
    assert_eq!(d.pdf_pages, PdfPages::All);
    assert!(!done_text(&seen), "Fertig nach dem Wechsel gelöscht");
    assert_eq!(d.write_state(), WriteState::Idle);
    let _ = fs::remove_dir_all(&dir);
}

/// Regel für den Blattnamen in „Im Blatt …“: wie überall in Meldungen höchstens 40 Zeichen, dann
/// „…“ (`shorten_for_display`).
#[test]
fn long_sheet_name_is_shortened_in_the_message() {
    let mut s = box_state();
    add_small(&mut s, "Blatt 1", A4_HOCH);
    let long = format!("Halter {}", "x".repeat(60));
    let id = add_small(&mut s, &long, A4_HOCH);
    set_title(&mut s, id, "Titel 😀");
    let shown = format!("{}…", long.chars().take(40).collect::<String>());
    assert_eq!(shorten_for_display(&long), shown);
    let d = all_dialog(&s, true, compute_view);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(in_sheet(&shown, &pdf_char_text('😀', "Titel")))
    );
}

/// Stücklistenblätter ohne Rahmen werden bei „Alle Blätter“ ohne DXF-Zuschlag gebaut: lange
/// Zellen stehen genau wie beim gewählten Blatt in der Datei.
#[test]
fn parts_sheets_in_all_have_no_dxf_widening() {
    let (mut s, f, ids) = getriebe();
    let long = "Lagerdeckel mit Dichtring und Sicherungsblech links";
    set_name_override(&mut s, f[0], long);
    set_name_override(&mut s, f[44], long);
    let dir = temp_dir("ohne-zuschlag");
    let mut d = all_dialog(&s, false, first_body);
    let pdf = read(&written(&mut d, &s, &dir.join("alle.pdf")));
    assert_eq!(pdf.pages.len(), 4);
    for (i, id) in ids.iter().enumerate().skip(1).take(2) {
        let one = selected_pdf(&s, *id, false, first_body, &dir.join(format!("s{i}.pdf")));
        assert_eq!(pdf.pages[i].ops, one.pages[0].ops, "Seite {i}");
    }
    let _ = fs::remove_dir_all(&dir);
}
