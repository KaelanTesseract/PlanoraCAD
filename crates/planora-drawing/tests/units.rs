//! Teilprojekt Zoll, Task 7: Jede Zeichnung hat ihre eigene Maßeinheit. Maßzahlen werden mit der
//! Einheit der Zeichnung umgerechnet (Winkel nicht), Toleranzen ebenso; eine Zeichnung in Zoll
//! trägt im Schriftfeld den Vermerk „Maße in Zoll“. Zeichnungen in mm bleiben unverändert. Die
//! globale Einheit spielt dabei keine Rolle.

use planora_drawing::annotate::{
    Mark, ResolvedDims, default_decimals, dimension_text, dimension_text_in,
};
use planora_drawing::dimension::{Anchor, DimId, DimKind, DimSpec, Measured, Tolerance};
use planora_drawing::model::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::sheet::build_sheet_with;
use planora_drawing::sheet_frame::{PROJECTION_X0, TITLE_W, sheet_layout};
use planora_drawing::sheet_info::{
    ProjectInfo, SheetContext, SheetInfo, Stamp, default_notes, default_notes_in, resolve_texts,
};
use planora_drawing::sheet_marks::{sheet_frame, sheet_frame_dxf, sheet_text_width};
use planora_drawing::{Basis, ViewId, ViewKind, ViewLines};
use planora_i18n::{DisplayUnit, english, german, with_lang, with_unit};
use planora_math::DVec2;
use std::collections::BTreeMap;

const IN: DisplayUnit = DisplayUnit::Zoll;
const MM: DisplayUnit = DisplayUnit::Millimeter;

fn spec(kind: DimKind, decimals: u8) -> DimSpec {
    DimSpec {
        id: DimId(1),
        view: ViewId(1),
        kind,
        refs: Vec::new(),
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: Tolerance::None,
        decimals,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn linear(value: f64) -> Measured {
    let (a, b) = (DVec2::new(0.0, 0.0), DVec2::new(value, 0.0));
    Measured {
        value,
        anchor: Anchor::Linear {
            a,
            b,
            dir: DVec2::new(1.0, 0.0),
            a_span: (a, a),
            b_span: (b, b),
        },
        prefix: None,
    }
}

fn de<R>(f: impl FnOnce() -> R) -> R {
    with_lang(&german(), f)
}

fn a3() -> Sheet {
    Sheet {
        paper: Paper::A3,
        landscape: true,
    }
}

fn drawing(unit: DisplayUnit) -> Drawing {
    let mut d = Drawing::new(DrawingId(1), "Halter".into(), a3(), Projection::First);
    d.info = Some(SheetInfo::new(&stamp()));
    d.unit = unit;
    d
}

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-31".into(),
        user: "dennis".into(),
    }
}

fn ctx() -> SheetContext {
    SheetContext {
        doc_name: "Teil".into(),
        index: 1,
        count: 1,
    }
}

#[test]
fn new_drawing_is_in_millimeters() {
    let d = Drawing::new(DrawingId(1), "A".into(), a3(), Projection::First);
    assert_eq!(d.unit, MM);
}

#[test]
fn inch_drawing_converts_lengths_with_its_own_decimals() {
    de(|| {
        let t = dimension_text_in(&spec(DimKind::Horizontal, 2), &linear(25.4), IN);
        assert_eq!(t.main, "1,00");
        assert_eq!(t.stack, None);
        // Durchmesser mit Vorsatz, ohne Einheitenzeichen.
        let mut m = linear(12.7);
        m.prefix = Some('Ø');
        assert_eq!(
            dimension_text_in(&spec(DimKind::Diameter, 3), &m, IN).main,
            "Ø0,500"
        );
    });
}

#[test]
fn millimeter_drawing_text_is_unchanged() {
    de(|| {
        let s = spec(DimKind::Horizontal, 1);
        let m = linear(25.4);
        assert_eq!(dimension_text_in(&s, &m, MM), dimension_text(&s, &m));
        assert_eq!(dimension_text_in(&s, &m, MM).main, "25,4");
    });
}

#[test]
fn the_global_unit_does_not_change_drawing_text() {
    de(|| {
        let s = spec(DimKind::Horizontal, 1);
        let m = linear(25.4);
        assert_eq!(with_unit(IN, || dimension_text(&s, &m).main), "25,4");
        assert_eq!(
            with_unit(MM, || dimension_text_in(
                &spec(DimKind::Horizontal, 2),
                &m,
                IN
            )
            .main),
            "1,00"
        );
    });
}

#[test]
fn angles_stay_in_degrees() {
    de(|| {
        let m = Measured {
            value: 45.0,
            anchor: Anchor::Angle {
                vertex: DVec2::ZERO,
                d1: DVec2::new(1.0, 0.0),
                d2: DVec2::new(1.0, 1.0),
            },
            prefix: None,
        };
        let mut s = spec(DimKind::Angle, 1);
        s.tolerance = Tolerance::Symmetric(0.5);
        assert_eq!(dimension_text_in(&s, &m, IN).main, "45,0° ±0,5");
    });
}

#[test]
fn tolerances_follow_the_drawing_unit() {
    de(|| {
        let m = linear(25.4);
        let mut s = spec(DimKind::Horizontal, 3);
        s.tolerance = Tolerance::Symmetric(0.254);
        assert_eq!(dimension_text_in(&s, &m, IN).main, "1,000 ±0,010");
        s.tolerance = Tolerance::Limits {
            upper: 0.127,
            lower: -0.0254,
        };
        let t = dimension_text_in(&s, &m, IN);
        assert_eq!(t.stack, Some(("+0,005".into(), "\u{2212}0,001".into())));
        s.tolerance = Tolerance::LimitSizes {
            upper: 2.54,
            lower: -2.54,
        };
        let t = dimension_text_in(&s, &m, IN);
        assert_eq!(t.stack, Some(("1,100".into(), "0,900".into())));
    });
}

#[test]
fn english_inch_text_uses_the_point() {
    let t = with_lang(&english(), || {
        dimension_text_in(&spec(DimKind::Horizontal, 2), &linear(25.4), IN)
    });
    assert_eq!(t.main, "1.00");
}

#[test]
fn new_dimensions_get_two_decimals_in_inch_drawings() {
    assert_eq!(default_decimals(DimKind::Horizontal, MM), 1);
    assert_eq!(default_decimals(DimKind::Diameter, MM), 1);
    assert_eq!(default_decimals(DimKind::Angle, MM), 1);
    assert_eq!(default_decimals(DimKind::Horizontal, IN), 2);
    assert_eq!(default_decimals(DimKind::Radius, IN), 2);
    // Winkel bleiben in Grad und behalten ihre Stellen.
    assert_eq!(default_decimals(DimKind::Angle, IN), 1);
}

fn empty_view(kind: ViewKind, w: f64, h: f64) -> ViewLines {
    ViewLines {
        kind,
        basis: Basis::of(kind),
        extent: 100.0,
        strokes: Vec::new(),
        pieces: Vec::new(),
        bbox: ((0.0, 0.0), (w, h)),
        centerlines: Vec::new(),
        dropped_splits: 0,
    }
}

fn sheet_texts_of(marks: &[Mark]) -> Vec<String> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Das Blatt (Bildschirm, SVG, DXF, PDF entstehen alle daraus) rechnet mit der Einheit der
/// Zeichnung, nicht mit der globalen.
#[test]
fn sheet_marks_use_the_drawing_unit() {
    let build = |unit: DisplayUnit, global: DisplayUnit| {
        let mut d = drawing(unit);
        let front = d.add_view(
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        );
        let vf = empty_view(ViewKind::Front, 50.8, 20.0);
        let results = BTreeMap::from([(front, &vf)]);
        let mut dims = ResolvedDims::default();
        let mut s = spec(DimKind::Horizontal, 2);
        s.view = front;
        dims.by_view.insert(front, vec![(s, Ok(linear(50.8)))]);
        de(|| {
            with_unit(global, || {
                let m = build_sheet_with(&d, &results, &dims);
                sheet_texts_of(&m.views[0].marks)
            })
        })
    };
    assert_eq!(build(IN, MM), vec!["2,00".to_string()]);
    assert_eq!(build(MM, IN), vec!["50,80".to_string()]);
}

#[test]
fn edge_note_is_converted_only_in_inch_drawings() {
    de(|| {
        assert_eq!(
            default_notes_in("ISO 2768-m", MM),
            default_notes("ISO 2768-m")
        );
        let notes = default_notes_in("ISO 2768-m", IN);
        assert_eq!(notes[1], "Kanten gebrochen 0,012 in");
        assert_eq!(notes[0], default_notes("ISO 2768-m")[0]);
        assert_eq!(notes[2], default_notes("ISO 2768-m")[2]);
    });
    let en = with_lang(&english(), || default_notes_in("ISO 2768-m", IN));
    assert_eq!(en[1], "Break sharp edges 0.012 in");
}

#[test]
fn title_texts_carry_the_unit_note_only_for_inch() {
    let p = ProjectInfo::new(&stamp());
    de(|| {
        let mm = resolve_texts(&drawing(MM), Some(&p), &ctx(), None);
        assert_eq!(mm.units_note, None);
        assert_eq!(mm.notes[1], "Kanten gebrochen 0,3 mm");
        let inch = resolve_texts(&drawing(IN), Some(&p), &ctx(), None);
        assert_eq!(inch.units_note.as_deref(), Some("Maße in Zoll"));
        assert_eq!(inch.notes[1], "Kanten gebrochen 0,012 in");
        // Die globale Einheit ändert nichts.
        let mm2 = with_unit(IN, || resolve_texts(&drawing(MM), Some(&p), &ctx(), None));
        assert_eq!(mm2, mm);
    });
    let en = with_lang(&english(), || {
        resolve_texts(&drawing(IN), Some(&p), &ctx(), None)
    });
    assert_eq!(en.units_note.as_deref(), Some("Dimensions in inches"));
}

#[test]
fn title_block_shows_the_unit_note_in_the_projection_cell() {
    let p = ProjectInfo::new(&stamp());
    let layout = sheet_layout(a3(), 0, 3);
    for unit in [MM, IN] {
        let texts = de(|| resolve_texts(&drawing(unit), Some(&p), &ctx(), None));
        for frame in [
            de(|| sheet_frame(&layout, &texts, Projection::First)),
            de(|| sheet_frame_dxf(&layout, &texts, Projection::First)),
        ] {
            let notes: Vec<&Mark> = frame
                .marks
                .iter()
                .filter(|m| matches!(m, Mark::Text { text, .. } if text.contains("Zoll")))
                .collect();
            if unit == MM {
                assert!(notes.is_empty(), "mm-Zeichnung ohne Vermerk");
                continue;
            }
            assert_eq!(notes.len(), 1);
            let Mark::Text {
                pos, height, text, ..
            } = notes[0]
            else {
                unreachable!()
            };
            assert_eq!(text, "Maße in Zoll");
            // Links neben dem Projektionssymbol, unter der Feldbezeichnung „Projektion“.
            let t0 = layout.title.0;
            let x = pos.x - t0.x;
            let y = pos.y - t0.y;
            assert!((135.0..PROJECTION_X0).contains(&x), "x = {x}");
            assert!((47.0..52.0).contains(&y), "y = {y}");
            assert!(y + height <= 52.8, "unter der Bezeichnung");
            assert!(x + sheet_text_width(text, *height) <= PROJECTION_X0 - 0.5);
            assert!(x < TITLE_W);
        }
    }
}

#[test]
fn mm_title_block_is_unchanged() {
    let p = ProjectInfo::new(&stamp());
    let layout = sheet_layout(a3(), 0, 3);
    let (mm, inch) = de(|| {
        (
            resolve_texts(&drawing(MM), Some(&p), &ctx(), None),
            resolve_texts(&drawing(IN), Some(&p), &ctx(), None),
        )
    });
    let mut plain = inch.clone();
    plain.units_note = None;
    plain.notes = mm.notes.clone();
    assert_eq!(plain, mm);
    let a = de(|| sheet_frame(&layout, &mm, Projection::First)).marks;
    let b = de(|| sheet_frame(&layout, &inch, Projection::First)).marks;
    assert_eq!(b.len(), a.len() + 1);
}
