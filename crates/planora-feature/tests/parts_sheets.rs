//! Stücklistenblätter als Zeichnungen mit automatischen Folgeblättern (3c-2, Aufgabe 3).

use planora_drawing::sheet_info::{NotesValue, Stamp};
use planora_drawing::{
    DrawingId, DrawingKind, Paper, Placement, Projection, Scale, Sheet, ViewKind,
};
use planora_feature::*;
use planora_math::DVec3;

const A4_HOCH: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};
const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};
const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-18".into(),
        user: "dennis".into(),
    }
}

fn cube(d: &mut Document, name: &str) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap()
}

/// `n` Quader mit verschiedenen Namen („Teil 1“ …): jede Zeile der Stückliste ein Körper.
fn cubes(d: &mut Document, n: usize) -> Vec<FeatureId> {
    (1..=n).map(|i| cube(d, &format!("Teil {i}"))).collect()
}

fn doc_with(n: usize, sheet: Sheet) -> (Document, Vec<FeatureId>) {
    let mut d = Document::new();
    let ids = cubes(&mut d, n);
    d.add_parts_sheets(sheet, &stamp()).unwrap();
    (d, ids)
}

fn names(d: &Document) -> Vec<String> {
    d.drawings().iter().map(|x| x.name.clone()).collect()
}

fn pages(d: &Document) -> Vec<u32> {
    d.parts_sheets()
        .iter()
        .map(|id| match d.drawing(*id).unwrap().kind {
            DrawingKind::PartsList { page } => page,
            DrawingKind::Views => panic!("kein Stücklistenblatt"),
        })
        .collect()
}

fn msg(e: FeatureError) -> String {
    match e {
        FeatureError::InvalidInput(m) => m,
        other => panic!("unerwarteter Fehler: {other:?}"),
    }
}

#[test]
fn create_first_page() {
    let mut d = Document::new();
    cubes(&mut d, 3);
    let before = d.edit_count();
    let id = d.add_parts_sheets(A4_HOCH, &stamp()).unwrap();
    assert_eq!(d.edit_count(), before + 1);
    assert_eq!(d.drawings().len(), 1);
    let x = d.drawing(id).unwrap();
    assert_eq!(x.name, "Stückliste");
    assert_eq!(x.kind, DrawingKind::PartsList { page: 1 });
    assert_eq!(x.sheet, A4_HOCH);
    assert_eq!(x.projection, Projection::First);
    assert!(x.views.is_empty());
    let info = x.info.as_ref().unwrap();
    assert_eq!(info.notes, NotesValue::Lines(vec![]));
    assert!(!info.parts_list);
    assert_eq!(info.created, "2026-10-18");
    assert_eq!(d.project_info().unwrap().login, "dennis");

    let count = d.edit_count();
    let e = d.add_parts_sheets(A4_HOCH, &stamp()).unwrap_err();
    assert_eq!(msg(e), PARTS_SHEET_EXISTS);
    assert_eq!(d.edit_count(), count);
    assert_eq!(d.drawings().len(), 1);

    let mut other = Document::new();
    other
        .add_drawing("stückliste", A4_HOCH, Projection::First)
        .unwrap();
    let count = other.edit_count();
    let e = other.add_parts_sheets(A4_HOCH, &stamp()).unwrap_err();
    assert_eq!(msg(e), PARTS_NAME_TAKEN);
    assert_eq!(other.edit_count(), count);
    assert_eq!(other.drawings().len(), 1);
    assert!(other.project_info().is_none());
}

#[test]
fn pages_follow_row_count() {
    // A4 quer: 26 Zeilen je Blatt.
    let (mut d, ids) = doc_with(26, A4_QUER);
    assert_eq!(names(&d), ["Stückliste"]);
    let c = d.edit_count();
    let m = d.model_edit_count();
    let extra = cube(&mut d, "Teil 27");
    assert_eq!(names(&d), ["Stückliste", "Stückliste (2)"]);
    assert_eq!(pages(&d), [1, 2]);
    // Modelländerung + Abgleich; Zeichnungsänderungen zählen nicht zur Modellversion.
    assert_eq!(d.edit_count(), c + 2);
    assert_eq!(d.model_edit_count(), m + 1);
    let second = d.drawing(d.parts_sheets()[1]).unwrap();
    assert_eq!(second.sheet, A4_QUER);
    assert_eq!(second.projection, Projection::First);
    assert_eq!(
        second.info.as_ref().unwrap().notes,
        NotesValue::Lines(vec![])
    );
    assert_eq!(second.info.as_ref().unwrap().created, "2026-10-18");

    // Ein erneutes Berechnen ohne Änderung bleibt ohne Wirkung.
    let c = d.edit_count();
    d.recompute();
    assert_eq!(d.edit_count(), c);
    assert!(!d.sync_parts_sheets());

    // Einen löschen: wieder ein Blatt.
    d.delete(extra).unwrap();
    assert_eq!(names(&d), ["Stückliste"]);

    // 52 Zeilen = 2 Blätter, 53 = 3 Blätter.
    for i in 27..=52 {
        cube(&mut d, &format!("Teil {i}"));
    }
    assert_eq!(pages(&d), [1, 2]);
    cube(&mut d, "Teil 53");
    assert_eq!(pages(&d), [1, 2, 3]);
    assert_eq!(
        names(&d),
        ["Stückliste", "Stückliste (2)", "Stückliste (3)"]
    );

    // Blattformat von Seite 1 gilt für die Folgeblätter.
    let first = d.parts_sheets()[0];
    d.update_drawing(first, |x| x.sheet = A3_QUER).unwrap();
    for id in d.parts_sheets() {
        assert_eq!(d.drawing(id).unwrap().sheet, A3_QUER);
    }
    // Ein Folgeblatt allein lässt sich nicht abweichend einstellen.
    let third = d.parts_sheets()[2];
    d.update_drawing(third, |x| x.sheet = A4_HOCH).unwrap();
    assert_eq!(d.drawing(third).unwrap().sheet, A3_QUER);
    drop(ids);
}

#[test]
fn pages_per_paper_and_limit() {
    // A4 hoch: 44 Zeilen je Blatt.
    let (mut d, _) = doc_with(44, A4_HOCH);
    assert_eq!(pages(&d), [1]);
    cube(&mut d, "Teil 45");
    assert_eq!(pages(&d), [1, 2]);

    // Höchstens 200 Zeilen zählen: 201 Zeilen auf A4 quer = 8 Blätter (200 / 26 aufgerundet).
    let (mut d, _) = doc_with(200, A4_QUER);
    assert_eq!(pages(&d), [1, 2, 3, 4, 5, 6, 7, 8]);
    cube(&mut d, "Teil 201");
    assert_eq!(pages(&d).len(), 8);
}

#[test]
fn sheets_follow_suppress_marker_and_delete() {
    let (mut d, ids) = doc_with(27, A4_QUER);
    assert_eq!(pages(&d), [1, 2]);
    d.set_suppressed(ids[0], true).unwrap();
    assert_eq!(pages(&d), [1]);
    d.set_suppressed(ids[0], false).unwrap();
    assert_eq!(pages(&d), [1, 2]);
    d.set_marker(26).unwrap();
    assert_eq!(pages(&d), [1]);
    d.set_marker(27).unwrap();
    assert_eq!(pages(&d), [1, 2]);
    d.delete(ids[26]).unwrap();
    assert_eq!(pages(&d), [1]);
}

#[test]
fn pages_stay_together() {
    let mut d = Document::new();
    cubes(&mut d, 26);
    d.add_drawing("Davor", A4_QUER, Projection::First).unwrap();
    d.add_parts_sheets(A4_QUER, &stamp()).unwrap();
    d.add_drawing("Halter", A4_QUER, Projection::First).unwrap();
    assert_eq!(names(&d), ["Davor", "Stückliste", "Halter"]);
    cube(&mut d, "Teil 27");
    assert_eq!(
        names(&d),
        ["Davor", "Stückliste", "Stückliste (2)", "Halter"]
    );
    for i in 28..=53 {
        cube(&mut d, &format!("Teil {i}"));
    }
    assert_eq!(
        names(&d),
        [
            "Davor",
            "Stückliste",
            "Stückliste (2)",
            "Stückliste (3)",
            "Halter"
        ]
    );
}

#[test]
fn delete_and_rename_rules() {
    let (mut d, ids) = doc_with(53, A4_QUER);
    let halter = d.add_drawing("Halter", A4_QUER, Projection::First).unwrap();
    let sheets = d.parts_sheets();
    assert_eq!(sheets.len(), 3);

    // Folgeblatt löschen: abgelehnt, nichts geändert.
    let snap_names = names(&d);
    let c = d.edit_count();
    for s in &sheets[1..] {
        let e = d.delete_drawing(*s).unwrap_err();
        assert_eq!(msg(e), FOLLOW_PAGE_DELETE);
    }
    assert_eq!(names(&d), snap_names);
    assert_eq!(d.edit_count(), c);

    // Umbenennen: jedes Blatt abgelehnt, auch über update_drawing; Namen bleiben.
    for s in &sheets {
        let e = d.rename_drawing(*s, "Teile").unwrap_err();
        assert_eq!(msg(e), PARTS_SHEET_RENAME);
        let e = d
            .update_drawing(*s, |x| x.name = "Teile".into())
            .unwrap_err();
        assert_eq!(msg(e), PARTS_SHEET_RENAME);
    }
    assert_eq!(names(&d), snap_names);
    assert_eq!(d.edit_count(), c);

    // Die Art einer Zeichnung lässt sich nicht ändern.
    assert!(
        d.update_drawing(halter, |x| x.kind = DrawingKind::PartsList { page: 9 })
            .is_err()
    );
    assert!(
        d.update_drawing(sheets[0], |x| x.kind = DrawingKind::Views)
            .is_err()
    );
    assert_eq!(d.edit_count(), c);

    // Eine andere Zeichnung nimmt keinen Seitennamen an.
    assert!(d.rename_drawing(halter, "Stückliste (4)").is_err());
    assert!(d.rename_drawing(halter, "stückliste (2)").is_err());
    assert!(
        d.add_drawing("Stückliste (5)", A4_QUER, Projection::First)
            .is_err()
    );
    assert_eq!(d.edit_count(), c);
    d.rename_drawing(halter, "Gehäuse").unwrap();

    // Seite 1 löschen: alle Blätter weg, eine Änderung.
    d.delete_drawing(sheets[0]).unwrap();
    assert_eq!(names(&d), ["Gehäuse"]);
    assert_eq!(d.edit_count(), c + 1 + 1);
    // Danach hält nichts den Seitennamen mehr, und es bleibt auch kein Abgleich übrig.
    assert!(!d.sync_parts_sheets());
    d.rename_drawing(halter, "Stückliste (2)").unwrap();
    drop(ids);
}

#[test]
fn no_views_on_parts_sheets() {
    let (mut d, _) = doc_with(3, A4_QUER);
    let s = d.parts_sheets()[0];
    let c = d.edit_count();
    assert!(
        d.add_view(
            s,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 0.0, y: 0.0 },
            None
        )
        .is_err()
    );
    assert_eq!(d.edit_count(), c);
    assert!(d.drawing(s).unwrap().views.is_empty());

    let base = d.drawing(s).unwrap().clone();

    let mut with_view = base.clone();
    with_view.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 0.0, y: 0.0 },
        None,
    );
    let err = validate_drawing(&with_view).unwrap_err();
    assert!(err.contains("Stücklistenblatt"), "{err}");

    // Maße: aus einer normalen Zeichnung mit Maß, ohne ihre Ansicht, auf ein Stücklistenblatt.
    let mut norm = base.clone();
    norm.kind = DrawingKind::Views;
    let v = norm.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 0.0, y: 0.0 },
        None,
    );
    norm.dimensions.push(planora_drawing::dimension::DimSpec {
        id: planora_drawing::dimension::DimId(1),
        view: v,
        kind: planora_drawing::dimension::DimKind::Horizontal,
        refs: vec![],
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: planora_drawing::dimension::Tolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    });
    let mut with_dim = base.clone();
    with_dim.dimensions = norm.dimensions.clone();
    let err = validate_drawing(&with_dim).unwrap_err();
    assert!(err.contains("Stücklistenblatt"), "{err}");

    let mut page0 = base.clone();
    page0.kind = DrawingKind::PartsList { page: 0 };
    assert!(validate_drawing(&page0).is_err());
    let mut page1 = base;
    page1.kind = DrawingKind::PartsList { page: 1 };
    assert!(validate_drawing(&page1).is_ok());
}

#[test]
fn loading_does_not_sync() {
    let (d, _) = doc_with(27, A4_QUER);
    assert_eq!(pages(&d), [1, 2]);

    // Snapshot mit nur einem Blatt, aber 27 Zeilen: Laden behält es.
    let mut s = d.snapshot();
    s.drawings
        .retain(|x| x.kind != DrawingKind::PartsList { page: 2 });
    let mut loaded = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(pages(&loaded), [1]);
    assert_eq!(loaded.edit_count(), 0);
    // Die nächste Modelländerung gleicht ab.
    cube(&mut loaded, "Teil 28");
    assert_eq!(pages(&loaded), [1, 2]);

    // Umgekehrt: zwei Blätter, aber nur noch eine Zeile.
    let (d, ids) = doc_with(27, A4_QUER);
    let mut s = d.snapshot();
    s.features.truncate(1);
    s.marker = 1;
    let mut loaded = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(pages(&loaded), [1, 2]);
    assert_eq!(loaded.edit_count(), 0);
    loaded.rename(ids[0], "Anders").unwrap();
    assert_eq!(pages(&loaded), [1]);

    // Lücke in den Seiten oder falscher Name: abgelehnt.
    let mut s = d.snapshot();
    s.drawings
        .retain(|x| x.kind != DrawingKind::PartsList { page: 1 });
    assert!(Document::from_snapshot(s, 0.05).is_err());
    let mut s = d.snapshot();
    for x in s.drawings.iter_mut() {
        if x.kind == (DrawingKind::PartsList { page: 2 }) {
            x.name = "Blatt zwei".into();
        }
    }
    assert!(Document::from_snapshot(s, 0.05).is_err());
}

#[test]
fn stamped_switch_default() {
    let mut d = Document::new();
    let a = d
        .add_drawing_stamped("A", A4_HOCH, Projection::First, &stamp())
        .unwrap();
    assert!(!d.drawing(a).unwrap().info.as_ref().unwrap().parts_list);
    let f1 = cube(&mut d, "Eins");
    let b = d
        .add_drawing_stamped("B", A4_HOCH, Projection::First, &stamp())
        .unwrap();
    assert!(!d.drawing(b).unwrap().info.as_ref().unwrap().parts_list);
    let f2 = cube(&mut d, "Zwei");
    let c = d
        .add_drawing_stamped("C", A4_HOCH, Projection::First, &stamp())
        .unwrap();
    assert!(d.drawing(c).unwrap().info.as_ref().unwrap().parts_list);

    // Körper hinzufügen oder löschen ändert die Schalter nicht.
    cube(&mut d, "Drei");
    d.delete(f2).unwrap();
    d.delete(f1).unwrap();
    assert!(!d.drawing(b).unwrap().info.as_ref().unwrap().parts_list);
    assert!(d.drawing(c).unwrap().info.as_ref().unwrap().parts_list);
    let e = d
        .add_drawing_stamped("E", A4_HOCH, Projection::First, &stamp())
        .unwrap();
    assert!(!d.drawing(e).unwrap().info.as_ref().unwrap().parts_list);

    // Ohne Stempel: keine Blattangaben.
    let p = d.add_drawing("P", A4_HOCH, Projection::First).unwrap();
    assert!(d.drawing(p).unwrap().info.is_none());
}

#[test]
fn stamped_switch_counts_visible_bodies_only() {
    let mut d = Document::new();
    let a = cube(&mut d, "Eins");
    cube(&mut d, "Zwei");
    d.set_suppressed(a, true).unwrap();
    let x = d
        .add_drawing_stamped("X", A4_HOCH, Projection::First, &stamp())
        .unwrap();
    assert!(!d.drawing(x).unwrap().info.as_ref().unwrap().parts_list);
}

#[test]
fn rename_regroups_pages() {
    let (mut d, ids) = doc_with(27, A4_QUER);
    assert_eq!(pages(&d), [1, 2]);
    let c = d.edit_count();
    // „Teil 27“ heißt wie „Teil 1“: 26 Zeilen, ein Blatt.
    d.rename(ids[26], "Teil 1").unwrap();
    assert_eq!(pages(&d), [1]);
    assert_eq!(names(&d), ["Stückliste"]);
    assert_eq!(d.edit_count(), c + 2);
    // Wieder trennen: das Folgeblatt kehrt zurück.
    d.rename(ids[26], "Teil 27").unwrap();
    assert_eq!(pages(&d), [1, 2]);
    // Gleicher Name ändert nichts.
    let c = d.edit_count();
    d.rename(ids[26], "Teil 27").unwrap();
    assert_eq!(d.edit_count(), c);
}

#[test]
fn follow_up_name_clash_is_not_overwritten() {
    // Ein Dokument aus einer Datei hat „Stückliste (2)“ schon als Zeichnung ohne Stücklistenblätter.
    let mut d = Document::new();
    cubes(&mut d, 26);
    d.add_parts_sheets(A4_QUER, &stamp()).unwrap();
    let mut s = d.snapshot();
    let mut x = planora_drawing::Drawing::new(
        DrawingId(s.next_drawing),
        "Stückliste (2)".into(),
        A4_QUER,
        Projection::First,
    );
    x.info = None;
    s.next_drawing += 1;
    s.drawings.push(x);
    let mut loaded = Document::from_snapshot(s, 0.05).unwrap();
    cube(&mut loaded, "Teil 27");
    // Kein zweites „Stückliste (2)“, kein Absturz.
    assert_eq!(pages(&loaded), [1]);
    let count = loaded
        .drawings()
        .iter()
        .filter(|x| x.name == "Stückliste (2)")
        .count();
    assert_eq!(count, 1);
}

#[test]
fn parts_sheets_are_listed_by_page_not_by_position() {
    let (d, _) = doc_with(27, A4_QUER);
    let mut s = d.snapshot();
    // In der Datei steht Seite 2 vor Seite 1.
    s.drawings.reverse();
    assert_eq!(s.drawings[0].kind, DrawingKind::PartsList { page: 2 });
    let mut loaded = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(pages(&loaded), [1, 2]);
    // Die Seite 1 bestimmt das Blattformat der Folgeblätter, nicht die erste in der Liste.
    let first = loaded.parts_sheets()[0];
    loaded.update_drawing(first, |x| x.sheet = A3_QUER).unwrap();
    let second = loaded.parts_sheets()[1];
    assert_eq!(loaded.drawing(second).unwrap().sheet, A3_QUER);
}
