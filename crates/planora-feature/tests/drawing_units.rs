//! Teilprojekt Zoll, Task 7: Maßeinheit je Zeichnung im Dokument. Anlegen mit einer Einheit,
//! Umstellen als eine Änderung, Toleranz-Meldungen in der Einheit der Zeichnung (nicht in der
//! Einstellung der Oberfläche).

use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, TagData, Tolerance as DimTolerance,
};
use planora_drawing::sheet_info::Stamp;
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::*;
use planora_i18n::{DisplayUnit, german, with_lang, with_unit};
use planora_math::DVec3;

const A4: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-31".into(),
        user: "dennis".into(),
    }
}

fn tag(role: &str) -> TagData {
    TagData {
        origin: 1,
        role: role.into(),
        hint: DVec3::ZERO,
    }
}

fn edge(a: &str, b: &str) -> GeoRef {
    GeoRef::Edge(GeoRefData::Edge {
        a: tag(a),
        b: tag(b),
        hint: DVec3::new(1.0, 2.0, 3.0),
    })
}

fn spec(view: planora_drawing::ViewId, kind: DimKind, refs: Vec<GeoRef>) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind,
        refs,
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::Symmetric(1000.5),
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

#[test]
fn drawings_start_in_the_given_unit() {
    let mut d = Document::new();
    let a = d
        .add_drawing_stamped("A", A4, Projection::First, &stamp())
        .unwrap();
    let b = d
        .add_drawing_stamped_in("B", A4, Projection::First, &stamp(), DisplayUnit::Zoll)
        .unwrap();
    let c = d.add_drawing("C", A4, Projection::First).unwrap();
    assert_eq!(d.drawing(a).unwrap().unit, DisplayUnit::Millimeter);
    assert_eq!(d.drawing(b).unwrap().unit, DisplayUnit::Zoll);
    assert_eq!(d.drawing(c).unwrap().unit, DisplayUnit::Millimeter);
}

#[test]
fn changing_the_unit_is_one_edit_and_same_unit_is_none() {
    let mut d = Document::new();
    let id = d.add_drawing("A", A4, Projection::First).unwrap();
    let edits = d.edit_count();
    d.set_drawing_unit(id, DisplayUnit::Millimeter).unwrap();
    assert_eq!(d.edit_count(), edits);
    d.set_drawing_unit(id, DisplayUnit::Zoll).unwrap();
    assert_eq!(d.edit_count(), edits + 1);
    assert_eq!(d.drawing(id).unwrap().unit, DisplayUnit::Zoll);
    // Schnappschuss trägt die Einheit mit.
    let back = Document::from_snapshot(d.snapshot(), 0.05).unwrap();
    assert_eq!(back.drawing(id).unwrap().unit, DisplayUnit::Zoll);
}

fn tolerance_message(unit: DisplayUnit, global: DisplayUnit, kind: DimKind) -> String {
    let mut d = Document::new();
    let id = d.add_drawing("A", A4, Projection::First).unwrap();
    d.set_drawing_unit(id, unit).unwrap();
    let front = d
        .add_view(
            id,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 0.0, y: 0.0 },
            None,
        )
        .unwrap();
    let refs = vec![edge("Wand 1", "Wand 2"), edge("Wand 1", "Wand 4")];
    let r = with_lang(&german(), || {
        with_unit(global, || d.add_dimension(id, spec(front, kind, refs)))
    });
    match r {
        Err(FeatureError::InvalidInput(m)) => m,
        other => panic!("InvalidInput erwartet, bekam {other:?}"),
    }
}

#[test]
fn tolerance_message_uses_the_drawing_unit() {
    let mm = tolerance_message(
        DisplayUnit::Millimeter,
        DisplayUnit::Zoll,
        DimKind::Horizontal,
    );
    assert!(mm.contains("1000 mm"), "{mm}");
    let inch = tolerance_message(
        DisplayUnit::Zoll,
        DisplayUnit::Millimeter,
        DimKind::Horizontal,
    );
    assert!(inch.contains("39,370 in"), "{inch}");
    // Winkel bleiben in Grad, gleich in welcher Einheit.
    let angle = tolerance_message(DisplayUnit::Zoll, DisplayUnit::Zoll, DimKind::Angle);
    assert!(angle.contains("1000 °"), "{angle}");
    assert!(!angle.contains(" in") && !angle.contains("mm"), "{angle}");
}
