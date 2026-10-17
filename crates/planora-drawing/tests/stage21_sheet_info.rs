//! Blattangaben: Vorbelegung, Änderungsbuchstaben, Auflösung zu Texten, Grenzen.

use planora_drawing::model::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::{SheetLine, SheetModel, SheetView};
use planora_drawing::sheet_frame::TitleField;
use planora_drawing::sheet_info::*;
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{ViewId, ViewKind, Visibility};
use planora_math::DVec2;

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-17".into(),
        user: "dennis".into(),
    }
}

fn drawing() -> Drawing {
    let mut d = Drawing::new(
        DrawingId(1),
        "Halter".into(),
        Sheet {
            paper: Paper::A3,
            landscape: true,
        },
        Projection::First,
    );
    d.info = Some(SheetInfo::new(&stamp()));
    d
}

fn ctx() -> SheetContext {
    SheetContext {
        doc_name: "Werkstück".into(),
        index: 2,
        count: 3,
    }
}

const HALF: Scale = Scale { num: 1, den: 2 };

fn texts(d: &Drawing, p: Option<&ProjectInfo>) -> TitleTexts {
    resolve_texts(d, p, &ctx(), Some(HALF))
}

fn info(d: &mut Drawing) -> &mut SheetInfo {
    d.info.as_mut().unwrap()
}

#[test]
fn revision_letters() {
    assert_eq!(revision_letter(0), Some('A'));
    assert_eq!(revision_letter(7), Some('H'));
    assert_eq!(revision_letter(8), Some('J'));
    assert_eq!(revision_letter(13), Some('P'));
    assert_eq!(revision_letter(14), Some('R'));
    assert_eq!(revision_letter(22), Some('Z'));
    assert_eq!(revision_letter(23), None);
    for i in 0..23 {
        let c = revision_letter(i).unwrap();
        assert!(!matches!(c, 'I' | 'O' | 'Q'), "{c}");
    }
}

#[test]
fn utc_dates() {
    assert_eq!(utc_date(0), "1970-01-01");
    assert_eq!(utc_date(1_792_195_200), "2026-10-17");
    assert_eq!(utc_date(951_868_799), "2000-02-29");
}

#[test]
fn defaults_of_a_new_sheet() {
    let d = drawing();
    let p = ProjectInfo::new(&stamp());
    let t = texts(&d, Some(&p));
    let get = |f| t.get(f);
    assert_eq!(get(TitleField::Number), "Werkstück-02");
    assert_eq!(get(TitleField::Title), "Halter");
    assert_eq!(get(TitleField::Subtitle), "");
    assert_eq!(get(TitleField::DocType), "Fertigungszeichnung");
    assert_eq!(get(TitleField::Status), "in Arbeit");
    assert_eq!(get(TitleField::Issued), "2026-10-17");
    assert_eq!(get(TitleField::Revision), "");
    assert_eq!(get(TitleField::SheetOf), "2 von 3");
    assert_eq!(get(TitleField::Format), "A3");
    assert_eq!(get(TitleField::Scale), "1:2");
    assert_eq!(get(TitleField::Creator), "dennis");
    assert_eq!(get(TitleField::Material), "–");
    assert_eq!(get(TitleField::Weight), "–");
    assert_eq!(get(TitleField::Language), "de");
    assert_eq!(get(TitleField::Owner), "");
    assert_eq!(get(TitleField::Checked), "");
    assert_eq!(get(TitleField::Approved), "");
    assert_eq!(
        t.notes,
        vec![
            "Allgemeintoleranzen ISO 2768-m".to_string(),
            "Kanten gebrochen 0,3 mm".to_string(),
            "Oberflächen Ra 3,2 (soweit nicht anders angegeben)".to_string(),
        ]
    );
    assert!(t.revisions.is_empty());
}

#[test]
fn custom_text_wins_and_reset_restores() {
    let mut d = drawing();
    let p = ProjectInfo::new(&stamp());
    info(&mut d).title = FieldValue::Text("Lager".into());
    info(&mut d).status = FieldValue::Text("freigegeben".into());
    let t = texts(&d, Some(&p));
    assert_eq!(t.get(TitleField::Title), "Lager");
    assert_eq!(t.get(TitleField::Status), "freigegeben");
    info(&mut d).status = FieldValue::Auto;
    assert_eq!(texts(&d, Some(&p)).get(TitleField::Status), "in Arbeit");
    // Leere Eingabe heißt automatisch.
    info(&mut d).title = FieldValue::from_input("  ");
    assert_eq!(info(&mut d).title, FieldValue::Auto);
    assert_eq!(texts(&d, Some(&p)).get(TitleField::Title), "Halter");
    assert_eq!(FieldValue::from_input(""), FieldValue::Auto);
    assert_eq!(
        FieldValue::from_input(" x "),
        FieldValue::Text(" x ".into())
    );
    // Neuer Kontext: Automatisches folgt, eigener Text bleibt.
    info(&mut d).subtitle = FieldValue::Text("Rev. B".into());
    let c = SheetContext {
        doc_name: "Neu".into(),
        index: 1,
        count: 1,
    };
    let t = resolve_texts(&d, Some(&p), &c, Some(HALF));
    assert_eq!(t.get(TitleField::Number), "Neu-01");
    assert_eq!(t.get(TitleField::SheetOf), "1 von 1");
    assert_eq!(t.get(TitleField::Subtitle), "Rev. B");
}

#[test]
fn old_drawing_without_info() {
    let mut d = drawing();
    d.info = None;
    let t = texts(&d, None);
    for f in [
        TitleField::Issued,
        TitleField::Owner,
        TitleField::Creator,
        TitleField::Checked,
        TitleField::Approved,
        TitleField::Material,
        TitleField::Weight,
    ] {
        assert_eq!(t.get(f), "–", "{f:?}");
    }
    assert_eq!(t.get(TitleField::Number), "Werkstück-02");
    assert_eq!(t.get(TitleField::Title), "Halter");
    assert_eq!(t.get(TitleField::DocType), "Fertigungszeichnung");
    assert_eq!(t.get(TitleField::Status), "in Arbeit");
    assert_eq!(t.get(TitleField::Revision), "");
    assert_eq!(t.get(TitleField::SheetOf), "2 von 3");
    assert_eq!(t.get(TitleField::Format), "A3");
    assert_eq!(t.get(TitleField::Scale), "1:2");
    assert_eq!(t.get(TitleField::Language), "de");
    assert_eq!(t.notes, default_notes(DEFAULT_TOLERANCE));
    // Ohne Maßstab ein Strich.
    assert_eq!(
        resolve_texts(&d, None, &ctx(), None).get(TitleField::Scale),
        "–"
    );
}

#[test]
fn revisions_and_state() {
    let mut d = drawing();
    for k in 0..23 {
        info(&mut d).add_revision("2026-10-17", "dg").unwrap();
        let t = texts(&d, None);
        assert_eq!(
            t.get(TitleField::Revision),
            revision_letter(k).unwrap().to_string()
        );
    }
    let err = info(&mut d).add_revision("2026-10-17", "dg").unwrap_err();
    assert_eq!(err, TOO_MANY_REVISIONS);
    assert!(err.contains("höchstens 23 Änderungen"));
    assert_eq!(info(&mut d).revisions.len(), 23);
    let t = texts(&d, None);
    assert_eq!(t.get(TitleField::Revision), "Z");
    assert_eq!(t.revisions.len(), 23);
    let vis = t.visible_revisions();
    assert_eq!(vis.len(), 6);
    assert_eq!(vis[5][0], "Z");
    assert_eq!(vis[0][0], "U");
    // Eigener Buchstabe an der letzten Änderung.
    info(&mut d).revisions[22].letter = FieldValue::Text("X1".into());
    assert_eq!(texts(&d, None).get(TitleField::Revision), "X1");
    // Weniger als sechs: alle.
    let mut few = drawing();
    info(&mut few).add_revision("d", "n").unwrap();
    assert_eq!(texts(&few, None).visible_revisions().len(), 1);
    assert!(texts(&drawing(), None).visible_revisions().is_empty());
}

#[test]
fn notes_rules() {
    let mut d = drawing();
    let mut p = ProjectInfo::new(&stamp());
    p.tolerance = FieldValue::Text("ISO 2768-f".into());
    assert_eq!(
        texts(&d, Some(&p)).notes[0],
        "Allgemeintoleranzen ISO 2768-f"
    );
    p.notes = NotesValue::Lines(vec!["P".into()]);
    assert_eq!(texts(&d, Some(&p)).notes, vec!["P".to_string()]);
    info(&mut d).notes = NotesValue::Lines(vec!["A".into()]);
    assert_eq!(texts(&d, Some(&p)).notes, vec!["A".to_string()]);
    info(&mut d).notes = NotesValue::Lines(vec![]);
    assert!(texts(&d, Some(&p)).notes.is_empty());
}

fn sheet_view(id: u64, lines: usize) -> SheetView {
    let line = SheetLine {
        vis: Visibility::Visible,
        curve: Out2d::Line {
            a: DVec2::ZERO,
            b: DVec2::X,
        },
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    };
    SheetView {
        id: ViewId(id),
        name: format!("V{id}"),
        lines: vec![line; lines],
        extent_mm: (1.0, 1.0),
        marks: vec![],
    }
}

fn model(d: &Drawing, views: Vec<SheetView>) -> SheetModel {
    SheetModel {
        sheet: d.sheet,
        frame: (DVec2::ZERO, DVec2::ONE),
        views,
    }
}

#[test]
fn main_scale_rule() {
    let mut d = drawing();
    let free = Placement::Free { x: 0.0, y: 0.0 };
    let a = d.add_view(ViewKind::Front, HALF, free, None);
    let b = d.add_view(ViewKind::Top, Scale { num: 1, den: 5 }, free, None);
    // Mehr Linien gewinnen.
    let m = model(&d, vec![sheet_view(a.0, 10), sheet_view(b.0, 30)]);
    assert_eq!(main_scale(&d, &m), Some(Scale { num: 1, den: 5 }));
    // Gleichstand: die erste.
    let m = model(&d, vec![sheet_view(a.0, 10), sheet_view(b.0, 10)]);
    assert_eq!(main_scale(&d, &m), Some(HALF));
    // Keine berechnet: erste Ansicht der Zeichnung.
    let m = model(&d, vec![]);
    assert_eq!(main_scale(&d, &m), Some(HALF));
    // Leer.
    let e = drawing();
    assert_eq!(main_scale(&e, &model(&e, vec![])), None);
}

fn long(n: usize) -> String {
    "x".repeat(n)
}

#[test]
fn validation_limits() {
    let ok = || SheetInfo::new(&stamp());
    assert!(validate_sheet_info(&ok()).is_ok());
    let err = |i: SheetInfo| validate_sheet_info(&i).unwrap_err();

    let mut i = ok();
    i.title = FieldValue::Text(long(60));
    assert!(validate_sheet_info(&i).is_ok());
    i.title = FieldValue::Text(long(61));
    assert!(err(i).contains("zu lang"));

    let mut i = ok();
    i.subtitle = FieldValue::Text(long(80));
    assert!(validate_sheet_info(&i).is_ok());
    i.subtitle = FieldValue::Text(long(81));
    assert!(err(i).contains("zu lang"));

    let mut i = ok();
    i.notes = NotesValue::Lines(vec![long(100)]);
    assert!(validate_sheet_info(&i).is_ok());
    i.notes = NotesValue::Lines(vec![long(101)]);
    assert!(err(i).contains("zu lang"));

    let mut i = ok();
    i.add_revision("d", "n").unwrap();
    i.revisions[0].description = long(120);
    assert!(validate_sheet_info(&i).is_ok());
    i.revisions[0].description = long(121);
    assert!(err(i).contains("zu lang"));

    let mut i = ok();
    i.notes = NotesValue::Lines(vec!["a".into(); 6]);
    assert!(validate_sheet_info(&i).is_ok());
    i.notes = NotesValue::Lines(vec!["a".into(); 7]);
    assert!(err(i).contains("höchstens 6 Hinweiszeilen"));

    let mut i = ok();
    for _ in 0..23 {
        i.add_revision("d", "n").unwrap();
    }
    assert!(validate_sheet_info(&i).is_ok());
    i.revisions.push(i.revisions[0].clone());
    assert!(err(i).contains("höchstens 23 Änderungen"));

    let mut i = ok();
    i.title = FieldValue::Text("a\nb".into());
    assert!(err(i).contains("Steuerzeichen"));
    let mut i = ok();
    i.notes = NotesValue::Lines(vec!["a\tb".into()]);
    assert!(err(i).contains("Steuerzeichen"));

    // Zeichen, nicht Bytes.
    let mut i = ok();
    i.title = FieldValue::Text("Ä".repeat(60));
    assert!(validate_sheet_info(&i).is_ok());

    // Projektangaben.
    let mut p = ProjectInfo::new(&stamp());
    assert!(validate_project_info(&p).is_ok());
    p.owner = FieldValue::Text(long(81));
    assert!(validate_project_info(&p).is_err());
    let mut p = ProjectInfo::new(&stamp());
    p.notes = NotesValue::Lines(vec!["a".into(); 7]);
    assert!(validate_project_info(&p).is_err());
    let mut p = ProjectInfo::new(&stamp());
    p.checked = FieldValue::Text("a\nb".into());
    assert!(validate_project_info(&p).is_err());
}
