//! Fenster „Exportieren“ mit dem achten Format „Zeichnung als PDF“ (Teilprojekt 3c-3, Aufgabe 4):
//! Formatliste, das gewählte Blatt als Datei, Blattrahmen, Zeichenprüfung mit Feldnamen,
//! verlorene Positionsnummern, Endung und Dateiname, Schreiben über die Hilfsdatei, Abbrechen
//! und Schließen, Dokumenteigenschaften und der Stand der Ergebnisse je Format.
//!
//! Die Dateien werden echt geschrieben und mit dem eigenen PDF-Leser der Zeichnungstests gelesen;
//! verglichen wird mit einem Blatt, das der Test selbst aus den Körpern berechnet.

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
