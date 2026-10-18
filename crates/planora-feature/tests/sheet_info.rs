use planora_drawing::sheet_info::{FieldValue, NotesValue, ProjectInfo, SheetInfo, Stamp};
use planora_drawing::{DrawingId, Paper, Projection, Sheet};
use planora_feature::*;

const A4: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};

fn stamp(date: &str, user: &str) -> Stamp {
    Stamp {
        date: date.to_string(),
        user: user.to_string(),
    }
}

#[test]
fn stamped_drawing_has_info_and_project() {
    let mut d = Document::new();
    let a = d
        .add_drawing_stamped(
            "Erste",
            A4,
            Projection::First,
            &stamp("2026-10-17", "dennis"),
        )
        .unwrap();
    assert_eq!(d.edit_count(), 1);
    assert_eq!(
        d.drawing(a).unwrap().info.as_ref().unwrap().created,
        "2026-10-17"
    );
    assert_eq!(d.project_info().unwrap().login, "dennis");

    let b = d
        .add_drawing_stamped(
            "Zweite",
            A4,
            Projection::First,
            &stamp("2026-10-18", "anna"),
        )
        .unwrap();
    assert_eq!(d.edit_count(), 2);
    assert_eq!(
        d.drawing(b).unwrap().info.as_ref().unwrap().created,
        "2026-10-18"
    );
    assert_eq!(d.project_info().unwrap().login, "dennis");

    let c = d.add_drawing("Dritte", A4, Projection::First).unwrap();
    assert_eq!(d.edit_count(), 3);
    assert!(d.drawing(c).unwrap().info.is_none());

    let mut plain = Document::new();
    plain.add_drawing("Blatt", A4, Projection::First).unwrap();
    assert!(plain.project_info().is_none());
}

#[test]
fn stamped_drawing_failure_changes_nothing() {
    let mut d = Document::new();
    d.add_drawing("Name", A4, Projection::First).unwrap();
    let before = d.edit_count();
    assert!(
        d.add_drawing_stamped(
            "name",
            A4,
            Projection::First,
            &stamp("2026-10-17", "dennis")
        )
        .is_err()
    );
    assert!(
        d.add_drawing_stamped(
            "Neu",
            A4,
            Projection::First,
            &stamp("2026-10-17", &"x".repeat(81))
        )
        .is_err()
    );
    assert_eq!(d.edit_count(), before);
    assert_eq!(d.drawings().len(), 1);
    assert!(d.project_info().is_none());
}

#[test]
fn set_infos_validate_and_count() {
    let mut d = Document::new();
    let id = d
        .add_drawing_stamped(
            "Blatt",
            A4,
            Projection::First,
            &stamp("2026-10-17", "dennis"),
        )
        .unwrap();
    let model = d.model_edit_count();
    let base = d.edit_count();

    let mut info = d.drawing(id).unwrap().info.clone().unwrap();
    info.title = FieldValue::Text("Welle".into());
    d.set_sheet_info(id, info.clone()).unwrap();
    assert_eq!(d.edit_count(), base + 1);
    d.set_sheet_info(id, info.clone()).unwrap();
    assert_eq!(d.edit_count(), base + 1, "gleiche Werte zählen nicht");

    let mut bad = info.clone();
    bad.title = FieldValue::Text("x".repeat(61));
    assert!(d.set_sheet_info(id, bad).is_err());
    assert_eq!(d.drawing(id).unwrap().info.as_ref(), Some(&info));
    assert_eq!(d.edit_count(), base + 1);

    let mut p = d.project_info().unwrap().clone();
    p.owner = FieldValue::Text("Firma".into());
    d.set_project_info(p.clone()).unwrap();
    assert_eq!(d.edit_count(), base + 2);
    d.set_project_info(p.clone()).unwrap();
    assert_eq!(d.edit_count(), base + 2);
    let mut bad = p.clone();
    bad.notes = NotesValue::Lines(vec!["a".into(); 7]);
    assert!(d.set_project_info(bad).is_err());
    assert_eq!(d.project_info(), Some(&p));
    assert_eq!(d.edit_count(), base + 2);
    assert_eq!(d.model_edit_count(), model);

    assert!(d.set_sheet_info(DrawingId(99), info).is_err());
}

#[test]
fn project_info_without_drawing_and_delete() {
    let mut d = Document::new();
    d.set_project_info(ProjectInfo::new(&stamp("2026-10-17", "dennis")))
        .unwrap();
    assert_eq!(d.edit_count(), 1);
    let id = d.add_drawing("Blatt", A4, Projection::First).unwrap();
    d.delete_drawing(id).unwrap();
    assert!(d.project_info().is_some(), "Projektangaben bleiben stehen");
}

#[test]
fn snapshot_round_trip() {
    let mut d = Document::new();
    let id = d
        .add_drawing_stamped(
            "Blatt",
            A4,
            Projection::First,
            &stamp("2026-10-17", "dennis"),
        )
        .unwrap();
    let mut info: SheetInfo = d.drawing(id).unwrap().info.clone().unwrap();
    info.number = FieldValue::Text("ZN-1".into());
    d.set_sheet_info(id, info).unwrap();
    let s = d.snapshot();
    assert!(s.project_info.is_some());
    let back = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(back.drawings(), d.drawings());
    assert_eq!(back.project_info(), d.project_info());
    assert_eq!(back.edit_count(), 0);
}

#[test]
fn from_snapshot_rejects_bad_project_info() {
    let mut s = Document::new().snapshot();
    let mut p = ProjectInfo::new(&stamp("2026-10-17", "dennis"));
    p.login = "x".repeat(81);
    s.project_info = Some(p);
    assert!(Document::from_snapshot(s, 0.05).is_err());
}
