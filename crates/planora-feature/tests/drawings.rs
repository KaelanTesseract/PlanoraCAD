use planora_drawing::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
};
use planora_feature::*;
use planora_math::DVec3;

const A4: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};
const S11: Scale = Scale { num: 1, den: 1 };
const FREE: Placement = Placement::Free { x: 0.0, y: 0.0 };
const ALIGNED: Placement = Placement::Aligned { distance: 50.0 };

fn doc_with_drawing() -> (Document, DrawingId) {
    let mut d = Document::new();
    let id = d.add_drawing("Blatt", A4, Projection::First).unwrap();
    (d, id)
}

fn invalid(r: Result<impl std::fmt::Debug, FeatureError>) -> String {
    match r {
        Err(FeatureError::InvalidInput(m)) => m,
        other => panic!("InvalidInput erwartet, bekam {other:?}"),
    }
}

/// Eine gültige Zeichnung für Snapshot-Tests: Vorne (Grundansicht) + Oben abgeleitet.
fn good_drawing(id: u64, name: &str) -> Drawing {
    let mut d = Drawing::new(DrawingId(id), name.to_string(), A4, Projection::First);
    let front = d.add_view(ViewKind::Front, S11, FREE, None);
    d.add_view(ViewKind::Top, S11, ALIGNED, Some(front));
    d
}

fn snapshot_with(drawings: Vec<Drawing>, next_drawing: u64) -> Snapshot {
    Snapshot {
        features: vec![],
        marker: 0,
        next_id: 1,
        meshes: Default::default(),
        drawings,
        next_drawing,
        project_info: None,
        parts_overrides: Default::default(),
        parameters: Vec::new(),
        next_param: 1,
    }
}

#[test]
fn add_rename_delete_roundtrip() {
    let mut d = Document::new();
    assert_eq!(d.edit_count(), 0);
    let a = d.add_drawing("Erste", A4, Projection::First).unwrap();
    assert_eq!(d.edit_count(), 1);
    let b = d.add_drawing("Zweite", A4, Projection::Third).unwrap();
    assert_eq!((a, b), (DrawingId(1), DrawingId(2)));
    assert_eq!(d.edit_count(), 2);
    d.rename_drawing(a, "  Neu  ").unwrap();
    assert_eq!(d.edit_count(), 3);
    assert_eq!(d.drawing(a).unwrap().name, "Neu");
    d.delete_drawing(a).unwrap();
    assert_eq!(d.edit_count(), 4);
    assert!(d.drawing(a).is_none());
    assert_eq!(d.drawings().len(), 1);
    // Kennungen werden nie wieder vergeben.
    let c = d.add_drawing("Dritte", A4, Projection::First).unwrap();
    assert_eq!(c, DrawingId(3));
    assert_eq!(d.edit_count(), 5);
    assert_eq!(d.model_edit_count(), 0);
}

/// Führt die Änderung aus; sie muss mit `InvalidInput` scheitern und darf nichts verändern.
fn refused_view(
    d: &mut Document,
    id: DrawingId,
    f: impl FnOnce(&mut Document) -> Result<ViewId, FeatureError>,
) {
    let before = d.drawing(id).unwrap().clone();
    let edits = d.edit_count();
    invalid(f(d));
    assert_eq!(d.drawing(id).unwrap(), &before);
    assert_eq!(d.edit_count(), edits);
}

#[test]
fn invalid_drawings_are_refused_and_leave_the_document_unchanged() {
    let (mut d, id) = doc_with_drawing();
    let front = d.add_view(id, ViewKind::Front, S11, FREE, None).unwrap();
    let iso = d
        .add_view(id, ViewKind::Isometric, S11, FREE, None)
        .unwrap();
    let before = d.drawing(id).unwrap().clone();
    let edits = d.edit_count();
    assert_eq!(edits, 3);

    // leerer Name beim Anlegen und Umbenennen
    invalid(d.add_drawing("   ", A4, Projection::First));
    assert_eq!(d.drawings().len(), 1);
    invalid(d.rename_drawing(id, " 	"));
    // Name mit 201 Zeichen zu lang
    invalid(d.rename_drawing(id, &"x".repeat(201)));
    assert_eq!(d.drawing(id).unwrap(), &before);
    assert_eq!(d.edit_count(), edits);

    // Maßstab 0:1, 1:0, 1001:1, 1:1001
    for s in [
        Scale { num: 0, den: 1 },
        Scale { num: 1, den: 0 },
        Scale { num: 1001, den: 1 },
        Scale { num: 1, den: 1001 },
    ] {
        refused_view(&mut d, id, |d| {
            d.add_view(id, ViewKind::Front, s, FREE, None)
        });
    }
    // Eltern: unbekannt, isometrisch
    refused_view(&mut d, id, |d| {
        d.add_view(id, ViewKind::Top, S11, ALIGNED, Some(ViewId(77)))
    });
    refused_view(&mut d, id, |d| {
        d.add_view(id, ViewKind::Top, S11, ALIGNED, Some(iso))
    });
    // Aligned ohne Eltern, Free mit Eltern, Aligned für isometrische Ansicht
    refused_view(&mut d, id, |d| {
        d.add_view(id, ViewKind::Top, S11, ALIGNED, None)
    });
    refused_view(&mut d, id, |d| {
        d.add_view(id, ViewKind::Top, S11, FREE, Some(front))
    });
    refused_view(&mut d, id, |d| {
        d.add_view(id, ViewKind::Isometric, S11, ALIGNED, Some(front))
    });
    // Positionen NaN, unendlich, 1e9, knapp über 1e5
    for p in [
        Placement::Free {
            x: f64::NAN,
            y: 0.0,
        },
        Placement::Free {
            x: 0.0,
            y: f64::INFINITY,
        },
        Placement::Free { x: 1.0e9, y: 0.0 },
        Placement::Free {
            x: 0.0,
            y: -1.0e5 - 1.0,
        },
    ] {
        refused_view(&mut d, id, |d| {
            d.add_view(id, ViewKind::Front, S11, p, None)
        });
    }
    for dist in [f64::NAN, 1.0e9, -1.0e5 - 1.0] {
        refused_view(&mut d, id, |d| {
            d.add_view(
                id,
                ViewKind::Top,
                S11,
                Placement::Aligned { distance: dist },
                Some(front),
            )
        });
    }
    // Die Grenzen selbst sind erlaubt: Maßstab 1000:1, 1:1000, Lage ±1e5.
    d.add_view(
        id,
        ViewKind::Front,
        Scale { num: 1000, den: 1 },
        Placement::Free {
            x: 1.0e5,
            y: -1.0e5,
        },
        None,
    )
    .unwrap();
    d.add_view(id, ViewKind::Front, Scale { num: 1, den: 1000 }, FREE, None)
        .unwrap();
    // 200 Zeichen (Umlaute zählen als je ein Zeichen) sind erlaubt.
    d.rename_drawing(id, &"ä".repeat(200)).unwrap();
}

#[test]
fn view_limit_is_64() {
    let (mut d, id) = doc_with_drawing();
    for _ in 0..64 {
        d.add_view(id, ViewKind::Front, S11, FREE, None).unwrap();
    }
    let edits = d.edit_count();
    let before = d.drawing(id).unwrap().clone();
    let m = invalid(d.add_view(id, ViewKind::Front, S11, FREE, None));
    assert!(m.contains("64"), "{m}");
    assert_eq!(d.drawing(id).unwrap(), &before);
    assert_eq!(d.drawing(id).unwrap().views.len(), 64);
    assert_eq!(d.edit_count(), edits);
}

#[test]
fn drawing_limit_is_1000() {
    let mut d = Document::new();
    for i in 0..1000 {
        d.add_drawing(&format!("Z{i}"), A4, Projection::First)
            .unwrap();
    }
    let m = invalid(d.add_drawing("Zu viel", A4, Projection::First));
    assert!(m.contains("1000"), "{m}");
    assert_eq!(d.drawings().len(), 1000);
    // Snapshot mit 1000 ist gültig, mit 1001 nicht.
    let s = d.snapshot();
    assert_eq!(s.drawings.len(), 1000);
    assert_eq!(s.next_drawing, 1001);
    assert!(Document::from_snapshot(s.clone(), 0.05).is_ok());
    let mut s = s;
    s.drawings.push(good_drawing(1001, "Z1000"));
    s.next_drawing = 1002;
    invalid(Document::from_snapshot(s, 0.05));
}

#[test]
fn duplicate_names_are_refused_case_insensitively() {
    let (mut d, a) = doc_with_drawing(); // "Blatt"
    invalid(d.add_drawing("BLATT", A4, Projection::First));
    invalid(d.add_drawing("  blatt  ", A4, Projection::First));
    assert_eq!(d.drawings().len(), 1);
    let b = d.add_drawing("Zweites", A4, Projection::First).unwrap();
    let edits = d.edit_count();
    // Umbenennen auf einen vorhandenen Namen, der sich nur in der Schreibweise unterscheidet.
    invalid(d.rename_drawing(b, "bLaTT"));
    assert_eq!(d.drawing(b).unwrap().name, "Zweites");
    // update_drawing prüft ebenfalls.
    invalid(d.update_drawing(b, |dr| dr.name = " BLATT ".to_string()));
    assert_eq!(d.drawing(b).unwrap().name, "Zweites");
    assert_eq!(d.edit_count(), edits);
    // Die eigene Zeichnung darf ihre Schreibweise ändern.
    d.rename_drawing(a, "BLATT").unwrap();
    assert_eq!(d.drawing(a).unwrap().name, "BLATT");
    // Nach dem Löschen ist der Name wieder frei.
    d.delete_drawing(a).unwrap();
    d.add_drawing("blatt", A4, Projection::First).unwrap();
}

#[test]
fn unknown_drawing_is_an_error_and_counts_nothing() {
    let (mut d, _id) = doc_with_drawing();
    let edits = d.edit_count();
    let ghost = DrawingId(99);
    assert_eq!(
        invalid(d.delete_drawing(ghost)),
        "Die Zeichnung gibt es nicht"
    );
    invalid(d.rename_drawing(ghost, "X"));
    invalid(d.update_drawing(ghost, |_| {}));
    invalid(d.add_view(ghost, ViewKind::Front, S11, FREE, None));
    assert!(d.drawing(ghost).is_none());
    assert_eq!(d.edit_count(), edits);
}

#[test]
fn update_that_changes_nothing_does_not_count() {
    let (mut d, id) = doc_with_drawing();
    let e = d.edit_count();
    d.update_drawing(id, |_| {}).unwrap();
    assert_eq!(d.edit_count(), e);
    d.rename_drawing(id, "Blatt").unwrap(); // gleicher Name
    assert_eq!(d.edit_count(), e);
    d.update_drawing(id, |dr| dr.sheet.landscape = true)
        .unwrap();
    assert_eq!(d.edit_count(), e + 1);
    assert!(d.drawing(id).unwrap().sheet.landscape);
}

#[test]
fn update_cannot_change_the_id() {
    let (mut d, id) = doc_with_drawing();
    invalid(d.update_drawing(id, |dr| dr.id = DrawingId(7)));
    assert_eq!(d.drawing(id).unwrap().id, id);
}

#[test]
fn remove_view_removes_children_via_update() {
    let (mut d, id) = doc_with_drawing();
    let front = d.add_view(id, ViewKind::Front, S11, FREE, None).unwrap();
    let top = d
        .add_view(id, ViewKind::Top, S11, ALIGNED, Some(front))
        .unwrap();
    let right = d
        .add_view(id, ViewKind::Right, S11, ALIGNED, Some(front))
        .unwrap();
    let _under = d
        .add_view(id, ViewKind::Left, S11, ALIGNED, Some(top))
        .unwrap();
    let other = d.add_view(id, ViewKind::Back, S11, FREE, None).unwrap();
    assert_eq!((front.0, top.0, right.0, other.0), (1, 2, 3, 5));
    let e = d.edit_count();
    let mut removed = vec![];
    d.update_drawing(id, |dr| removed = dr.remove_view(top))
        .unwrap();
    assert_eq!(removed, vec![top, ViewId(4)]);
    let ids: Vec<u64> = d
        .drawing(id)
        .unwrap()
        .views
        .iter()
        .map(|v| v.id.0)
        .collect();
    assert_eq!(ids, vec![1, 3, 5]);
    assert_eq!(d.edit_count(), e + 1);
    // Die Grundansicht samt aller Ableitungen.
    d.update_drawing(id, |dr| {
        dr.remove_view(front);
    })
    .unwrap();
    let ids: Vec<u64> = d
        .drawing(id)
        .unwrap()
        .views
        .iter()
        .map(|v| v.id.0)
        .collect();
    assert_eq!(ids, vec![5]);
}

#[test]
fn snapshot_roundtrip_keeps_drawings() {
    let (mut d, id) = doc_with_drawing();
    let front = d.add_view(id, ViewKind::Front, S11, FREE, None).unwrap();
    d.add_view(
        id,
        ViewKind::Top,
        Scale { num: 1, den: 2 },
        Placement::Aligned { distance: 75.5 },
        Some(front),
    )
    .unwrap();
    let id2 = d.add_drawing("Zweite", A4, Projection::Third).unwrap();
    d.delete_drawing(id2).unwrap();
    let e = Document::from_snapshot(d.snapshot(), 0.05).unwrap();
    assert_eq!(e.drawings(), d.drawings());
    assert_eq!(e.edit_count(), 0);
    assert_eq!(e.model_edit_count(), 0);
    // Zähler bleiben erhalten: nächste Kennung ist 3, nicht 2.
    let mut e = e;
    assert_eq!(
        e.add_drawing("Dritte", A4, Projection::First).unwrap(),
        DrawingId(3)
    );
}

#[test]
fn snapshot_with_bad_data_is_refused() {
    // Ausgangspunkt gültig
    assert!(
        Document::from_snapshot(
            snapshot_with(vec![good_drawing(1, "A"), good_drawing(2, "B")], 3),
            0.05
        )
        .is_ok()
    );
    let refused = |s: Snapshot| {
        invalid(Document::from_snapshot(s, 0.05));
    };
    // doppelte Kennung
    refused(snapshot_with(
        vec![good_drawing(1, "A"), good_drawing(1, "B")],
        3,
    ));
    // Kennung >= next, Kennung == next, Kennung 0
    refused(snapshot_with(vec![good_drawing(5, "A")], 3));
    refused(snapshot_with(vec![good_drawing(3, "A")], 3));
    refused(snapshot_with(vec![good_drawing(0, "A")], 3));
    // next_drawing: 0 und u64::MAX
    refused(snapshot_with(vec![], 0));
    refused(snapshot_with(vec![], u64::MAX));
    // Namen doppelt (auch nur durch Schreibweise/Leerraum)
    refused(snapshot_with(
        vec![good_drawing(1, "Blatt"), good_drawing(2, "blatt")],
        3,
    ));
    refused(snapshot_with(
        vec![good_drawing(1, "Blatt"), good_drawing(2, " BLATT ")],
        3,
    ));
    // leerer Name
    refused(snapshot_with(vec![good_drawing(1, "  ")], 2));
    // Zyklus: Ansicht 1 hängt an 2, Ansicht 2 an 1
    let mut cyc = good_drawing(1, "A");
    cyc.views[0].parent = Some(ViewId(2));
    cyc.views[0].placement = ALIGNED;
    cyc.views[1].parent = Some(ViewId(1));
    refused(snapshot_with(vec![cyc], 2));
    // Ansicht mit sich selbst als Eltern
    let mut own = good_drawing(1, "A");
    own.views[1].parent = Some(ViewId(2));
    refused(snapshot_with(vec![own], 2));
    // Aligned mit Elternart Isometrisch
    let mut iso = Drawing::new(DrawingId(1), "A".into(), A4, Projection::First);
    let p = iso.add_view(ViewKind::Isometric, S11, FREE, None);
    iso.add_view(ViewKind::Front, S11, ALIGNED, Some(p));
    refused(snapshot_with(vec![iso], 2));
    // Ansichtskennung doppelt / >= next_view
    let mut dup = good_drawing(1, "A");
    dup.views[1].id = ViewId(1);
    refused(snapshot_with(vec![dup], 2));
    let mut hi = good_drawing(1, "A");
    hi.views[1].id = ViewId(9);
    refused(snapshot_with(vec![hi], 2));
    // NaN-Position
    let mut nan = good_drawing(1, "A");
    nan.views[0].placement = Placement::Free {
        x: f64::NAN,
        y: 0.0,
    };
    refused(snapshot_with(vec![nan], 2));
    // Maßstab 0
    let mut sc = good_drawing(1, "A");
    sc.views[0].scale = Scale { num: 0, den: 1 };
    refused(snapshot_with(vec![sc], 2));
    // 65 Ansichten
    let mut many = Drawing::new(DrawingId(1), "A".into(), A4, Projection::First);
    for _ in 0..65 {
        many.add_view(ViewKind::Front, S11, FREE, None);
    }
    refused(snapshot_with(vec![many], 2));
    // Fehler in einer Zeichnung darf nicht nennenswert halb geladen werden: Fehlermeldung nennt sie
    let mut named = good_drawing(1, "Meine");
    named.views[0].scale = Scale { num: 0, den: 1 };
    let m = invalid(Document::from_snapshot(snapshot_with(vec![named], 2), 0.05));
    assert!(m.contains("Meine"), "{m}");
}

#[test]
fn feature_edit_changes_model_count_drawing_edit_does_not() {
    let (mut d, id) = doc_with_drawing();
    assert_eq!((d.edit_count(), d.model_edit_count()), (1, 0));
    d.add_view(id, ViewKind::Front, S11, FREE, None).unwrap();
    d.rename_drawing(id, "Anders").unwrap();
    assert_eq!((d.edit_count(), d.model_edit_count()), (3, 0));
    let f = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(10.0, 10.0, 10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let m = d.model_edit_count();
    assert!(m > 0);
    assert_eq!(d.edit_count(), 3 + m);
    d.rename_drawing(id, "Nochmal").unwrap();
    assert_eq!(d.model_edit_count(), m);
    assert_eq!(d.edit_count(), 4 + m);
    d.set_suppressed(f, true).unwrap();
    assert!(d.model_edit_count() > m);
}

#[test]
fn old_snapshot_without_drawings_loads() {
    let d = Document::from_snapshot(snapshot_with(vec![], 1), 0.05).unwrap();
    assert!(d.drawings().is_empty());
    assert_eq!(d.edit_count(), 0);
    let s = d.snapshot();
    assert!(s.drawings.is_empty());
    assert_eq!(s.next_drawing, 1);
    let mut d = d;
    assert_eq!(
        d.add_drawing("Erste", A4, Projection::First).unwrap(),
        DrawingId(1)
    );
}

#[test]
fn update_drawing_rejects_invalid_sheet_info() {
    use planora_drawing::sheet_info::{FieldValue, SheetInfo, Stamp};
    let (mut d, id) = doc_with_drawing();
    let stamp = Stamp {
        date: "2026-10-17".into(),
        user: "dennis".into(),
    };
    d.update_drawing(id, |dr| dr.info = Some(SheetInfo::new(&stamp)))
        .unwrap();
    let before = d.drawing(id).unwrap().clone();
    let edits = d.edit_count();
    let m = invalid(d.update_drawing(id, |dr| {
        dr.info.as_mut().unwrap().title = FieldValue::Text("x".repeat(61));
    }));
    assert!(m.contains("zu lang"), "{m}");
    assert_eq!(d.drawing(id).unwrap(), &before);
    assert_eq!(d.edit_count(), edits);
}

#[test]
fn a_drawing_without_info_is_valid() {
    let d = Drawing::new(DrawingId(1), "Z".into(), A4, Projection::First);
    assert!(d.info.is_none());
    assert!(planora_feature::validate_drawing(&d).is_ok());
}

// ---- Schnittansichten (Teilprojekt 3d, Aufgabe 4) ----------------------------------------

mod sections {
    use super::*;
    use planora_drawing::{MAX_SECTIONS, SectionSpec};

    const A3: Sheet = Sheet {
        paper: Paper::A3,
        landscape: true,
    };

    /// Zeichnung mit Vorne (Kennung 1) und einem Schnitt A–A nach Oben (Kennung 2).
    fn with_section() -> Drawing {
        let mut d = Drawing::new(DrawingId(1), "Blatt".into(), A3, Projection::First);
        let front = d.add_view(ViewKind::Front, S11, FREE, None);
        d.add_section(front, ViewKind::Top, 5.0, 80.0).unwrap();
        d
    }

    fn sec(d: &mut Drawing, f: impl FnOnce(&mut SectionSpec)) {
        f(d.views[1].section.as_mut().unwrap());
    }

    fn err(d: &Drawing) -> String {
        validate_drawing(d).unwrap_err()
    }

    #[test]
    fn a_valid_section_passes() {
        let d = with_section();
        assert_eq!(validate_drawing(&d), Ok(()));
    }

    #[test]
    fn source_must_be_a_straight_view_without_section() {
        let msg = "Ein Schnitt geht nur aus einer geraden Ansicht.";
        // Quelle isometrisch.
        let mut d = with_section();
        d.views[0].kind = ViewKind::Isometric;
        assert_eq!(err(&d), msg);
        // Quelle ist selbst ein Schnitt.
        let mut d = with_section();
        let s = d.views[1].id;
        d.add_view(ViewKind::Left, S11, ALIGNED, Some(s));
        d.views[2].section = Some(SectionSpec {
            offset: 0.0,
            letter: 'B',
            hatch: Default::default(),
        });
        assert_eq!(err(&d), msg);
        // Frei platziert oder ohne Quelle.
        let mut d = with_section();
        d.views[1].placement = FREE;
        assert_eq!(err(&d), msg);
        let mut d = with_section();
        d.views[1].parent = None;
        assert_eq!(err(&d), msg);
    }

    #[test]
    fn plane_must_fit_the_source() {
        let msg = "Die Schnittebene passt nicht zur Ausgangsansicht.";
        // Hinten blickt entlang derselben Achse wie Vorne.
        let mut d = with_section();
        d.views[1].kind = ViewKind::Back;
        assert_eq!(err(&d), msg);
        // Isometrisch hat keine Schnittebene.
        let mut d = with_section();
        d.views[1].kind = ViewKind::Isometric;
        assert_eq!(err(&d), msg);
    }

    #[test]
    fn scale_follows_the_source() {
        let mut d = with_section();
        d.views[1].scale = Scale { num: 2, den: 1 };
        assert_eq!(
            err(&d),
            "Eine Schnittansicht hat den Maßstab ihrer Ausgangsansicht."
        );
    }

    #[test]
    fn letters_must_be_valid_and_unique() {
        let msg = "Ein Schnittbuchstabe ist ungültig oder doppelt.";
        for bad in ['I', 'O', 'Q', 'a', '1', ' ', 'Ä'] {
            let mut d = with_section();
            sec(&mut d, |s| s.letter = bad);
            assert_eq!(err(&d), msg, "{bad:?}");
        }
        let mut d = with_section();
        let front = d.views[0].id;
        d.add_section(front, ViewKind::Left, 0.0, 80.0).unwrap();
        d.views[2].section.as_mut().unwrap().letter = 'A';
        assert_eq!(err(&d), msg);
        d.views[2].section.as_mut().unwrap().letter = 'B';
        assert_eq!(validate_drawing(&d), Ok(()));
    }

    #[test]
    fn at_most_twelve_sections() {
        let mut d = with_section();
        let front = d.views[0].id;
        for i in 1..MAX_SECTIONS {
            d.add_section(front, ViewKind::Top, i as f64, 80.0).unwrap();
        }
        assert_eq!(validate_drawing(&d), Ok(()));
        let mut extra = d.views[1].clone();
        extra.id = ViewId(d.next_view);
        d.next_view += 1;
        extra.section.as_mut().unwrap().letter = 'Z';
        d.views.push(extra);
        assert_eq!(err(&d), "Es sind höchstens 12 Schnitte je Blatt möglich.");
    }

    #[test]
    fn offset_must_be_finite_and_in_range() {
        let msg = "Die Lage der Schnittebene ist ungültig.";
        for bad in [f64::NAN, f64::INFINITY, 1.0e5 + 1.0, -1.0e5 - 1.0] {
            let mut d = with_section();
            sec(&mut d, |s| s.offset = bad);
            assert_eq!(err(&d), msg, "{bad}");
        }
        for ok in [1.0e5, -1.0e5, 0.0] {
            let mut d = with_section();
            sec(&mut d, |s| s.offset = ok);
            assert_eq!(validate_drawing(&d), Ok(()), "{ok}");
        }
    }

    #[test]
    fn hatch_spacing_is_checked_for_the_sheet() {
        let mut d = with_section();
        sec(&mut d, |s| s.hatch.spacing = 0.7);
        assert_eq!(validate_drawing(&d), Ok(()));
        d.sheet = A4;
        assert_eq!(
            err(&d),
            "Der Schraffurabstand von „Schnitt A\u{2013}A“ wäre auf diesem Blatt kleiner als 0,5 mm."
        );
        let mut d = with_section();
        sec(&mut d, |s| s.hatch.spacing = 50.5);
        assert_eq!(
            err(&d),
            "Der Schraffurabstand ist zu groß (höchstens 50 mm)."
        );
    }

    #[test]
    fn changing_the_sheet_is_refused_and_leaves_the_document_alone() {
        let mut doc = Document::new();
        let id = doc.add_drawing("Blatt", A3, Projection::First).unwrap();
        let front = doc.add_view(id, ViewKind::Front, S11, FREE, None).unwrap();
        doc.update_drawing(id, |d| {
            d.add_section(front, ViewKind::Top, 0.0, 80.0).unwrap();
            d.views[1].section.as_mut().unwrap().hatch.spacing = 0.7;
        })
        .unwrap();
        let before = doc.drawing(id).unwrap().clone();
        let edits = doc.edit_count();
        let m = invalid(doc.update_drawing(id, |d| d.sheet = A4));
        assert!(
            m.contains("Schnitt A\u{2013}A") && m.contains("0,5 mm"),
            "{m}"
        );
        assert_eq!(doc.drawing(id).unwrap(), &before);
        assert_eq!(doc.edit_count(), edits);
        // Auf ein größeres Blatt geht es.
        doc.update_drawing(id, |d| {
            d.sheet = Sheet {
                paper: Paper::A2,
                landscape: true,
            }
        })
        .unwrap();
    }

    #[test]
    fn snapshots_with_bad_sections_are_refused_with_the_drawing_name() {
        let mut d = with_section();
        sec(&mut d, |s| s.letter = 'I');
        let e = Document::from_snapshot(snapshot_with(vec![d], 2), 0.1).unwrap_err();
        assert!(format!("{e:?}").contains("Blatt"), "{e:?}");
    }
}
