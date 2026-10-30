//! Stücklistenblätter über Sprachen hinweg (Sprachplan, Task 14): Ein Stücklistenblatt erkennt
//! man an seiner Art (`DrawingKind::PartsList`), nicht am Namen. Der Name entsteht beim Anlegen in
//! der damaligen Sprache und ist danach Benutzerdaten; ein Dokument aus einer Sprache lädt in
//! jeder anderen (auch ohne deren Sprachdatei) ohne Namenskonflikt. Kein Test ruft
//! `set_global` (Ruling 4).

use planora_drawing::sheet_info::Stamp;
use planora_drawing::{DrawingKind, Paper, Projection, Sheet};
use planora_feature::*;
use planora_i18n::{english, german, with_lang};
use planora_math::DVec3;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-30".into(),
        user: "dennis".into(),
    }
}

fn cube(d: &mut Document, name: &str) {
    d.add_feature(
        name,
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
}

/// Dokument mit `n` verschiedenen Teilen und Stückliste (A4 quer: 26 Zeilen je Blatt).
fn doc_with(n: usize) -> Document {
    let mut d = Document::new();
    for i in 1..=n {
        cube(&mut d, &format!("Teil {i}"));
    }
    d.add_parts_sheets(A4_QUER, &stamp()).unwrap();
    d
}

/// Namen der Stücklistenblätter nach Seite.
fn sheet_names(d: &Document) -> Vec<String> {
    d.parts_sheets()
        .iter()
        .map(|id| d.drawing(*id).unwrap().name.clone())
        .collect()
}

#[test]
fn a_german_document_opens_in_english_without_a_name_conflict() {
    let d = with_lang(&german(), || doc_with(27));
    assert_eq!(sheet_names(&d), ["Stückliste", "Stückliste (2)"]);
    let snap = d.snapshot();
    with_lang(&english(), || {
        let mut loaded = Document::from_snapshot(snap.clone(), 0.05)
            .expect("deutsches Dokument lädt unter Englisch");
        // Gespeicherte Namen sind Benutzerdaten: nichts wird übersetzt.
        assert_eq!(sheet_names(&loaded), ["Stückliste", "Stückliste (2)"]);
        // Neue Folgeblätter heißen wie Seite 1, nicht in der Sprache des Programms.
        for i in 28..=53 {
            cube(&mut loaded, &format!("Teil {i}"));
        }
        assert_eq!(
            sheet_names(&loaded),
            ["Stückliste", "Stückliste (2)", "Stückliste (3)"]
        );
        // Eine andere Zeichnung nimmt keinen Seitennamen an, in keiner Sprache.
        let halter = loaded
            .add_drawing("Halter", A4_QUER, Projection::First)
            .unwrap();
        assert!(loaded.rename_drawing(halter, "Stückliste (4)").is_err());
        assert!(loaded.rename_drawing(halter, "Parts list (4)").is_err());
        assert!(loaded.rename_drawing(halter, "parts list").is_err());
        // Wieder gespeichert und geladen, auch zurück unter Deutsch.
        let again = loaded.snapshot();
        assert!(Document::from_snapshot(again.clone(), 0.05).is_ok());
        assert!(with_lang(&german(), || Document::from_snapshot(again, 0.05)).is_ok());
    });
}

#[test]
fn an_english_document_keeps_its_names_in_german() {
    let d = with_lang(&english(), || doc_with(27));
    assert_eq!(sheet_names(&d), ["Parts list", "Parts list (2)"]);
    let snap = d.snapshot();
    with_lang(&german(), || {
        let mut loaded = Document::from_snapshot(snap.clone(), 0.05)
            .expect("englisches Dokument lädt unter Deutsch");
        assert_eq!(sheet_names(&loaded), ["Parts list", "Parts list (2)"]);
        for i in 28..=53 {
            cube(&mut loaded, &format!("Teil {i}"));
        }
        assert_eq!(
            sheet_names(&loaded),
            ["Parts list", "Parts list (2)", "Parts list (3)"]
        );
    });
}

#[test]
fn parts_sheet_names_are_user_data_when_loading() {
    // Ein Dokument aus einer Übersetzersprache (hier „Teileliste“), deren Datei fehlt: Das Blatt
    // ist an seiner Art erkannt, der Name zählt beim Laden nicht.
    let d = with_lang(&german(), || doc_with(27));
    let mut snap = d.snapshot();
    for x in snap.drawings.iter_mut() {
        match x.kind {
            DrawingKind::PartsList { page: 1 } => x.name = "Teileliste".into(),
            DrawingKind::PartsList { page } => x.name = format!("Teileliste ({page})"),
            DrawingKind::Views => {}
        }
    }
    for lang in [german(), english()] {
        let loaded = with_lang(&lang, || Document::from_snapshot(snap.clone(), 0.05))
            .expect("Name eines Stücklistenblatts ist Benutzerdaten");
        assert_eq!(sheet_names(&loaded), ["Teileliste", "Teileliste (2)"]);
    }
    // Lücken in den Seiten und doppelte Namen bleiben verboten.
    let mut gap = snap.clone();
    gap.drawings
        .retain(|x| x.kind != DrawingKind::PartsList { page: 1 });
    assert!(Document::from_snapshot(gap, 0.05).is_err());
    let mut twice = snap;
    for x in twice.drawings.iter_mut() {
        if x.kind == (DrawingKind::PartsList { page: 2 }) {
            x.name = "teileliste".into();
        }
    }
    assert!(Document::from_snapshot(twice, 0.05).is_err());
}
