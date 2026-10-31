//! Teilprojekt Zoll, Task 7: Maßeinheit je Zeichnung in der Oberfläche. Eine neue Zeichnung
//! übernimmt die Einstellung beim Anlegen; in den Blattangaben lässt sie sich umstellen (ein
//! Schritt zum Rückgängigmachen); „Maß bearbeiten“ zeigt und nimmt Toleranzen in der Einheit der
//! Zeichnung; der Maßtext im Browser folgt der Zeichnung, nicht der Einstellung.

mod common;

use planora_app::dimension_browser::row_text_in;
use planora_app::dimension_dialog::{DimensionDialog, ToleranceKind, tolerance_title};
use planora_app::dimension_state::resolve_in;
use planora_app::drawing_dialog::NewDrawingDialog;
use planora_app::sheet_dialog::{self, SheetInfoDialog, unit_name};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_drawing::dimension::{DimId, DimKind, DimSpec, GeoRef, Tolerance as DimTolerance};
use planora_drawing::sheet_info::{FieldValue, Stamp};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_feature::geo_ref_for_edge;
use planora_i18n::{DisplayUnit, german, with_lang, with_unit};
use planora_math::{DVec3, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-31".into(),
        user: "dennis".into(),
    }
}

fn ws() -> Workspace {
    let root = common::temp_root("planora-drawing-units");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn st(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
}

#[test]
fn a_new_drawing_takes_the_unit_of_the_setting() {
    for unit in [DisplayUnit::Millimeter, DisplayUnit::Zoll] {
        let mut s = AppState::new();
        s.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
        let dialog = NewDrawingDialog::new(&s);
        let id = with_unit(unit, || dialog.create(&mut s)).unwrap();
        assert_eq!(s.document.drawing(id).unwrap().unit, unit);
        // Die Einstellung später umzustellen ändert die Zeichnung nicht.
        let other = match unit {
            DisplayUnit::Millimeter => DisplayUnit::Zoll,
            DisplayUnit::Zoll => DisplayUnit::Millimeter,
        };
        with_unit(other, || {
            assert_eq!(s.document.drawing(id).unwrap().unit, unit);
        });
    }
}

#[test]
fn sheet_dialog_changes_the_unit_in_one_undo_step() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    w.observe_history(false);
    let d = st(&mut w)
        .document
        .add_drawing_stamped("Blatt 1", A4_QUER, Projection::First, &stamp())
        .unwrap();
    w.observe_history(false);
    let steps = w.active_tab().history().undo_len();

    let mut dialog = SheetInfoDialog::new(&w.active_tab().state, d, &stamp());
    assert_eq!(dialog.unit, DisplayUnit::Millimeter);
    // Unverändert übernommen: kein Schritt.
    dialog.apply(st(&mut w)).unwrap();
    w.observe_history(false);
    assert_eq!(w.active_tab().history().undo_len(), steps);

    dialog.unit = DisplayUnit::Zoll;
    dialog.sheet.title = FieldValue::Text("Halter".into());
    dialog.apply(st(&mut w)).unwrap();
    w.observe_history(false);
    assert_eq!(w.active_tab().history().undo_len(), steps + 1);
    let drawing = st(&mut w).document.drawing(d).unwrap().clone();
    assert_eq!(drawing.unit, DisplayUnit::Zoll);

    w.active_tab_mut().undo().unwrap();
    let back = st(&mut w).document.drawing(d).unwrap().clone();
    assert_eq!(back.unit, DisplayUnit::Millimeter);
    assert_eq!(back.info.unwrap().title, FieldValue::Auto);

    // Der Entwurf eines neu geöffneten Fensters zeigt die Einheit der Zeichnung.
    w.active_tab_mut().redo().unwrap();
    sheet_dialog::open(st(&mut w), d);
    let open = st(&mut w).sheet_dialog.as_ref().unwrap();
    assert_eq!(open.unit, DisplayUnit::Zoll);
}

#[test]
fn sheet_dialog_shows_the_unit_choice() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let d = s
        .document
        .add_drawing_stamped("Blatt 1", A4_QUER, Projection::First, &stamp())
        .unwrap();
    let mut dialog = SheetInfoDialog::new(&s, d, &stamp());
    // Standardauflösung 1920 × 1080: Auswahl ganz zu sehen (im Bild, nicht abgeschnitten).
    let mut screen = common::Screen::new(1920.0, 1080.0);
    let seen = with_lang(&german(), || {
        screen.settle(6, &mut |ctx| {
            dialog.show(ctx, &mut s);
        })
    });
    let text = common::join(&seen);
    let full = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0));
    let visible = |t: &str| {
        seen.iter().any(|x| {
            x.text == t && full.contains_rect(x.rect) && x.clip.expand(0.5).contains_rect(x.rect)
        })
    };
    assert!(visible("Maßeinheit"), "{text}");
    let mm = with_lang(&german(), || unit_name(DisplayUnit::Millimeter));
    assert_eq!(mm, "Millimeter (mm)");
    assert!(visible(&mm), "{text}");
}

// ---- Maße in einer Zeichnung in Zoll --------------------------------------------------------

const BOX: DVec3 = DVec3::new(25.4, 20.0, 30.0);

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn edge_where(st: &AppState, body: usize, pred: impl Fn(DVec3) -> bool) -> GeoRef {
    let solid = &st.bodies[body].solid;
    let id = solid
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let e = solid.edges.get(id).unwrap();
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && pred(a) && pred(b)
        })
        .expect("Kante nicht gefunden");
    GeoRef::Edge(geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap())
}

/// Quader 25,4 breit, Zeichnung in Zoll mit Vorderansicht und Breitenmaß (2 Stellen).
fn inch_state(tolerance: DimTolerance) -> (AppState, DrawingId, ViewId, DimId) {
    let mut s = AppState::new();
    s.create_box(BOX).unwrap();
    let d = s
        .document
        .add_drawing_stamped_in(
            "Blatt 1",
            A4_QUER,
            Projection::First,
            &stamp(),
            DisplayUnit::Zoll,
        )
        .unwrap();
    let front = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        )
        .unwrap();
    let (lo, hi) = s.bodies[0].bounds;
    let refs = vec![
        edge_where(&s, 0, |p| near(p.x, lo.x) && near(p.y, lo.y)),
        edge_where(&s, 0, |p| near(p.x, hi.x) && near(p.y, lo.y)),
    ];
    let spec = DimSpec {
        id: DimId(0),
        view: front,
        kind: DimKind::Horizontal,
        refs,
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance,
        decimals: 2,
        text_override: None,
        chain: None,
        suggested: false,
    };
    let id = s.document.add_dimension(d, spec).unwrap();
    s.drawing_ui.open(d);
    s.drawing_ui.tick(&s.bodies, &s.document, true);
    (s, d, front, id)
}

#[test]
fn browser_row_follows_the_drawing_unit_not_the_setting() {
    let (mut s, d, _, id) = inch_state(DimTolerance::Symmetric(0.254));
    let dims = resolve_in(&mut s, d);
    let (spec, r) = dims
        .by_view
        .values()
        .flatten()
        .find(|(x, _)| x.id == id)
        .unwrap();
    let unit = s.document.drawing(d).unwrap().unit;
    let row = with_lang(&german(), || {
        with_unit(DisplayUnit::Millimeter, || row_text_in(spec, r, unit))
    });
    assert_eq!(row, "1,00 ±0,01");
}

#[test]
fn dimension_dialog_edits_tolerances_in_inches() {
    let (mut s, d, _, id) = inch_state(DimTolerance::Symmetric(0.254));
    planora_app::dimension_dialog::open(&mut s, d, id, false);
    let mut dialog = s.dimension_dialog.take().unwrap();
    assert_eq!(dialog.unit, DisplayUnit::Zoll);
    with_lang(&german(), || {
        assert_eq!(dialog.symmetric, "0,01");
        assert_eq!(tolerance_title(dialog.unit), "Toleranz (in)");
        assert_eq!(tolerance_title(DisplayUnit::Millimeter), "Toleranz");
    });
    // Unverändert bestätigt: der gespeicherte Wert bleibt bitgenau.
    let (_, tol, _) = with_lang(&german(), || dialog.build()).unwrap();
    assert_eq!(tol, DimTolerance::Symmetric(0.254));
    // Neue Eingabe in Zoll → mm.
    dialog.symmetric = "0,02".into();
    let (_, tol, _) = with_lang(&german(), || dialog.build()).unwrap();
    let DimTolerance::Symmetric(v) = tol else {
        panic!("{tol:?}")
    };
    assert!((v - 0.508).abs() < 1e-12, "{v}");
    dialog.apply(&mut s).unwrap();
    // Die Grenze gilt in Zoll (1000 mm ≈ 39,37 in).
    dialog.symmetric = "40".into();
    let errors = with_lang(&german(), || dialog.errors());
    assert!(errors.iter().any(|e| e.contains("39,370079")), "{errors:?}");
    // In einer Zeichnung in mm bleibt alles wie bisher.
    let spec = s
        .document
        .drawing(d)
        .unwrap()
        .dimension(id)
        .unwrap()
        .clone();
    let mm = DimensionDialog::new(d, &spec, false);
    assert_eq!(mm.unit, DisplayUnit::Millimeter);
    assert!(mm.symmetric.starts_with("0,50"), "{}", mm.symmetric);
}

/// Zoll-Eingabe, dann die Zeichnung zurück auf mm: das Feld zeigt gerundet „0,508“, und
/// unverändert übernommen bleibt der gespeicherte Wert bitgenau.
#[test]
fn switching_back_to_mm_shows_rounded_tolerances_and_keeps_the_value() {
    let (mut s, d, _, id) = inch_state(DimTolerance::None);
    planora_app::dimension_dialog::open(&mut s, d, id, false);
    let mut dialog = s.dimension_dialog.take().unwrap();
    dialog.kind = ToleranceKind::Symmetric;
    dialog.symmetric = "0,02".into();
    with_lang(&german(), || dialog.apply(&mut s)).unwrap();
    let stored = match s
        .document
        .drawing(d)
        .unwrap()
        .dimension(id)
        .unwrap()
        .tolerance
    {
        DimTolerance::Symmetric(v) => v,
        ref t => panic!("{t:?}"),
    };
    assert!((stored - 0.508).abs() < 1e-12, "{stored}");
    s.document
        .set_drawing_unit(d, DisplayUnit::Millimeter)
        .unwrap();
    planora_app::dimension_dialog::open(&mut s, d, id, false);
    let dialog = s.dimension_dialog.take().unwrap();
    assert_eq!(dialog.unit, DisplayUnit::Millimeter);
    with_lang(&german(), || {
        assert_eq!(dialog.symmetric, "0,508");
        assert_eq!(tolerance_title(dialog.unit), "Toleranz");
    });
    let (_, tol, _) = with_lang(&german(), || dialog.build()).unwrap();
    match tol {
        DimTolerance::Symmetric(v) => assert_eq!(v.to_bits(), stored.to_bits()),
        t => panic!("{t:?}"),
    }
    let edits = s.document.edit_count();
    with_lang(&german(), || dialog.apply(&mut s)).unwrap();
    assert_eq!(
        s.document.edit_count(),
        edits,
        "unverändert: keine Änderung"
    );
}

#[test]
fn dimension_dialog_shows_the_value_in_the_drawing_unit() {
    let (mut s, d, _, id) = inch_state(DimTolerance::None);
    planora_app::dimension_dialog::open(&mut s, d, id, false);
    let mut dialog = s.dimension_dialog.take().unwrap();
    let mut screen = common::Screen::new(1920.0, 1080.0);
    let seen = with_lang(&german(), || {
        with_unit(DisplayUnit::Millimeter, || {
            screen.settle(6, &mut |ctx| {
                dialog.show(ctx, &mut s);
            })
        })
    });
    let text = common::join(&seen);
    assert!(common::find(&seen, "1,00").is_some(), "{text}");
    assert!(common::find(&seen, "25,40").is_none(), "{text}");
    assert!(common::find(&seen, "Toleranz (in)").is_some(), "{text}");
}
