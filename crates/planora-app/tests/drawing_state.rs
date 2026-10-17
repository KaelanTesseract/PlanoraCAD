//! Zeichnungsstand der App mit Berechnung im Hintergrund (Teilprojekt 11, Aufgabe 14):
//! Schlüssel je Ansichtsart und Modellstand, Abbrechen veralteter Berechnungen, Meldungen,
//! Maßstabsvorschlag und Abstand abgeleiteter Ansichten.

use planora_app::drawing_state::*;
use planora_app::state::*;
use planora_drawing::{
    DrawBody, DrawError, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
    ViewLines,
};
use planora_exchange::{ExchangeMesh, Progress};
use planora_feature::{FeatureId, FeatureKind};
use planora_math::DVec3;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Einheitswürfel als Netz (Dreiecke von außen gegen den Uhrzeigersinn).
fn cube_mesh() -> ExchangeMesh {
    ExchangeMesh {
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

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

fn s(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

fn free(x: f64, y: f64) -> Placement {
    Placement::Free { x, y }
}

/// Zustand mit einem Quader der Größe `size`; liefert auch das Feature.
fn box_state(size: DVec3) -> (AppState, FeatureId) {
    let mut st = AppState::new();
    let id = st.create_box(size).unwrap();
    (st, id)
}

fn new_drawing(st: &mut AppState, name: &str) -> DrawingId {
    st.document
        .add_drawing(name, A4_QUER, Projection::First)
        .unwrap()
}

fn add(st: &mut AppState, d: DrawingId, kind: ViewKind, scale: Scale) -> ViewId {
    st.document
        .add_view(d, kind, scale, free(100.0, 100.0), None)
        .unwrap()
}

fn tick(ui: &mut DrawingUi, st: &AppState, block: bool) {
    ui.tick(&st.bodies, &st.document, block);
}

fn ready(ui: &DrawingUi, st: &AppState, kind: ViewKind) -> Arc<ViewLines> {
    match ui.result(kind, st.document.model_edit_count()) {
        ViewResult::Ready(v) => v,
        other => panic!("{kind:?}: {other:?}"),
    }
}

fn failed(ui: &DrawingUi, st: &AppState, kind: ViewKind) -> String {
    match ui.result(kind, st.document.model_edit_count()) {
        ViewResult::Failed(m) => m,
        other => panic!("{kind:?}: {other:?}"),
    }
}

fn width(v: &ViewLines) -> f64 {
    v.bbox.1.0 - v.bbox.0.0
}

/// Ändert die Kantenlänge des Quaders `id` auf `edge` (alle drei Kanten).
fn resize_box(st: &mut AppState, id: FeatureId, edge: f64) {
    st.document
        .update(id, |k| {
            if let FeatureKind::Box { size, .. } = k {
                *size = DVec3::splat(edge);
            }
        })
        .unwrap();
    st.refresh();
}

/// Rechnung für Tests: wartet, bis abgebrochen wird (meldet vorher ein Viertel Fortschritt).
fn wait_for_cancel(_: &[DrawBody], _: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    p.set_total(4);
    p.advance(1);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        p.check()?;
        std::thread::sleep(Duration::from_millis(2));
    }
    Err(DrawError::Cancelled)
}

fn panics(_: &[DrawBody], _: ViewKind, _: &Progress) -> Result<ViewLines, DrawError> {
    panic!("absichtlicher Absturz im Test");
}

fn too_complex(_: &[DrawBody], _: ViewKind, _: &Progress) -> Result<ViewLines, DrawError> {
    Err(DrawError::TooComplex(5))
}

fn open(ui: &mut DrawingUi, d: DrawingId) {
    ui.open(d);
}

// (a)
#[test]
fn opening_a_drawing_computes_each_kind_once() {
    let (mut st, _) = box_state(DVec3::splat(10.0));
    let d = new_drawing(&mut st, "Blatt 1");
    let front = add(&mut st, d, ViewKind::Front, s(1, 1));
    let front2 = add(&mut st, d, ViewKind::Front, s(1, 2));
    let top = st
        .document
        .add_view(
            d,
            ViewKind::Top,
            s(1, 1),
            Placement::Aligned { distance: 40.0 },
            Some(front),
        )
        .unwrap();
    let mut ui = DrawingUi::default();
    assert_eq!(ui.area, WorkArea::Model);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 0, "Modellbereich: nichts rechnen");
    open(&mut ui, d);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 2, "Vorne und Oben je einmal");
    let f = ready(&ui, &st, ViewKind::Front);
    let t = ready(&ui, &st, ViewKind::Top);
    assert!((width(&f) - 10.0).abs() < 1e-6, "{:?}", f.bbox);
    assert!((width(&t) - 10.0).abs() < 1e-6, "{:?}", t.bbox);
    let drawing = st.document.drawing(d).unwrap().clone();
    let r = ui.results_for(&drawing, st.document.model_edit_count());
    assert_eq!(
        r.keys().copied().collect::<Vec<_>>(),
        vec![front, front2, top]
    );
    assert!(
        Arc::ptr_eq(&r[&front], &r[&front2]),
        "eine Rechnung für beide"
    );
    // Weitere Bilder ohne Änderung starten nichts.
    for _ in 0..5 {
        tick(&mut ui, &st, false);
    }
    assert_eq!(ui.spawned(), 2);
}

// (b)
#[test]
fn moving_or_scaling_a_view_does_not_recompute() {
    let (mut st, _) = box_state(DVec3::splat(10.0));
    let d = new_drawing(&mut st, "Blatt 1");
    let front = add(&mut st, d, ViewKind::Front, s(1, 1));
    let mut ui = DrawingUi::default();
    open(&mut ui, d);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 1);
    let (edits, model) = (st.document.edit_count(), st.document.model_edit_count());
    st.document
        .update_drawing(d, |dr| {
            dr.views[0].scale = s(2, 1);
            dr.views[0].placement = free(50.0, 60.0);
        })
        .unwrap();
    st.document
        .add_view(d, ViewKind::Front, s(1, 5), free(200.0, 100.0), None)
        .unwrap();
    st.document.rename_drawing(d, "Blatt A").unwrap();
    assert_eq!(
        st.document.edit_count(),
        edits + 3,
        "Zeichnungsänderungen zählen"
    );
    assert_eq!(
        st.document.model_edit_count(),
        model,
        "das Modell bleibt gleich"
    );
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 1, "Lage und Maßstab rechnen nicht neu");
    let drawing = st.document.drawing(d).unwrap().clone();
    assert_eq!(
        ui.results_for(&drawing, st.document.model_edit_count())
            .len(),
        2
    );
    assert!(
        ui.results_for(&drawing, st.document.model_edit_count())
            .contains_key(&front)
    );
}

// (c)
#[test]
fn editing_the_model_recomputes_and_drops_old_lines() {
    let (mut st, id) = box_state(DVec3::splat(10.0));
    let d = new_drawing(&mut st, "Blatt 1");
    add(&mut st, d, ViewKind::Front, s(1, 1));
    let mut ui = DrawingUi::default();
    open(&mut ui, d);
    tick(&mut ui, &st, true);
    let old = ready(&ui, &st, ViewKind::Front);
    assert!((width(&old) - 10.0).abs() < 1e-6, "{:?}", old.bbox);
    resize_box(&mut st, id, 20.0);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 2);
    let new = ready(&ui, &st, ViewKind::Front);
    assert!((width(&new) - 20.0).abs() < 1e-6, "{:?}", new.bbox);
    let h = new.bbox.1.1 - new.bbox.0.1;
    assert!((h - 20.0).abs() < 1e-6, "{:?}", new.bbox);
    assert!(!Arc::ptr_eq(&old, &new));
}

// (d)
#[test]
fn a_running_job_is_cancelled_on_model_change() {
    let (mut st, id) = box_state(DVec3::splat(10.0));
    let d = new_drawing(&mut st, "Blatt 1");
    add(&mut st, d, ViewKind::Front, s(1, 1));
    let mut ui = DrawingUi::default();
    ui.set_compute(wait_for_cancel);
    open(&mut ui, d);
    tick(&mut ui, &st, false);
    assert_eq!(ui.spawned(), 1);
    let first = ui.progress_of(ViewKind::Front).expect("läuft");
    assert!(!first.is_cancelled());
    // Fortschritt aus `Job::fraction`.
    let start = Instant::now();
    loop {
        tick(&mut ui, &st, false);
        match ui.result(ViewKind::Front, st.document.model_edit_count()) {
            ViewResult::Pending { fraction } if fraction > 0.0 => {
                assert!((fraction - 0.25).abs() < 1e-6, "{fraction}");
                break;
            }
            ViewResult::Pending { .. } => {}
            other => panic!("{other:?}"),
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
    resize_box(&mut st, id, 20.0);
    tick(&mut ui, &st, false);
    assert!(first.is_cancelled(), "die alte Berechnung ist abgebrochen");
    assert_eq!(ui.spawned(), 2, "eine neue Berechnung läuft");
    let second = ui.progress_of(ViewKind::Front).expect("läuft");
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(!second.is_cancelled());
    // Fallenlassen des Zustands bricht auch die neue ab.
    drop(ui);
    assert!(second.is_cancelled());
}

// (e)
#[test]
fn no_bodies_and_mesh_only_give_named_failures() {
    // Keine Körper
    let mut st = AppState::new();
    let d = new_drawing(&mut st, "Blatt 1");
    add(&mut st, d, ViewKind::Front, s(1, 1));
    let mut ui = DrawingUi::default();
    open(&mut ui, d);
    tick(&mut ui, &st, true);
    assert_eq!(
        failed(&ui, &st, ViewKind::Front),
        "Es gibt keinen Körper, der sich zeichnen lässt."
    );
    assert_eq!(
        ui.note(&st.bodies).as_deref(),
        Some("Es gibt keinen Körper, der sich zeichnen lässt.")
    );

    // Nur Netz-Körper
    st.import_mesh("Würfel", Arc::new(cube_mesh()), 1.0)
        .unwrap();
    tick(&mut ui, &st, true);
    assert_eq!(
        failed(&ui, &st, ViewKind::Front),
        "Alle Körper sind Netze; eine Zeichnung braucht exakte Körper."
    );
    assert_eq!(
        ui.note(&st.bodies).as_deref(),
        Some("Alle Körper sind Netze; eine Zeichnung braucht exakte Körper.")
    );

    // Gemischt: exakter Quader und zwei Netz-Körper (ein langer Name wird gekürzt).
    st.import_mesh(&"N".repeat(60), Arc::new(cube_mesh()), 1.0)
        .unwrap();
    st.create_box(DVec3::splat(10.0)).unwrap();
    tick(&mut ui, &st, true);
    assert!(matches!(
        ui.result(ViewKind::Front, st.document.model_edit_count()),
        ViewResult::Ready(_)
    ));
    assert_eq!(
        ui.note(&st.bodies),
        Some(format!(
            "Netz-Körper werden nicht gezeichnet: Würfel, {}….",
            "N".repeat(40)
        ))
    );

    // Nur exakte Körper: kein Hinweis.
    let (only, _) = box_state(DVec3::splat(5.0));
    assert_eq!(ui.note(&only.bodies), None);
}

#[test]
fn drawing_errors_and_crashed_workers_become_failures() {
    let (mut st, _) = box_state(DVec3::splat(10.0));
    let d = new_drawing(&mut st, "Blatt 1");
    add(&mut st, d, ViewKind::Front, s(1, 1));
    let mut ui = DrawingUi::default();
    ui.set_compute(too_complex);
    open(&mut ui, d);
    tick(&mut ui, &st, true);
    assert_eq!(
        failed(&ui, &st, ViewKind::Front),
        "Der Körper ist für diese Ansicht zu aufwendig (5 Arbeitsschritte)."
    );

    let mut ui = DrawingUi::default();
    ui.set_compute(panics);
    open(&mut ui, d);
    tick(&mut ui, &st, true);
    assert_eq!(
        failed(&ui, &st, ViewKind::Front),
        "Die Berechnung wurde unerwartet beendet."
    );
    // Auch ohne Warten: der verlorene Thread hängt nicht.
    let mut ui = DrawingUi::default();
    ui.set_compute(panics);
    open(&mut ui, d);
    let start = Instant::now();
    loop {
        tick(&mut ui, &st, false);
        if let ViewResult::Failed(m) = ui.result(ViewKind::Front, st.document.model_edit_count()) {
            assert_eq!(m, "Die Berechnung wurde unerwartet beendet.");
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "hängt");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        ui.spawned(),
        1,
        "ein Fehler wird nicht ständig neu versucht"
    );
}

// (f)
#[test]
fn failed_view_keeps_the_drawing() {
    let mut st = AppState::new();
    st.import_mesh("Würfel", Arc::new(cube_mesh()), 1.0)
        .unwrap();
    let d = new_drawing(&mut st, "Blatt 1");
    let v = add(&mut st, d, ViewKind::Front, s(1, 1));
    let before = st.document.drawing(d).unwrap().clone();
    let mut ui = DrawingUi::default();
    open(&mut ui, d);
    ui.selected_view = Some(v);
    tick(&mut ui, &st, true);
    assert!(matches!(
        ui.result(ViewKind::Front, st.document.model_edit_count()),
        ViewResult::Failed(_)
    ));
    assert_eq!(st.document.drawings().len(), 1);
    assert_eq!(st.document.drawing(d), Some(&before));
    assert_eq!(
        ui.area,
        WorkArea::Drawing(d),
        "der Zeichnungsbereich bleibt offen"
    );
    assert_eq!(ui.selected_view, Some(v));
    assert!(
        ui.results_for(&before, st.document.model_edit_count())
            .is_empty()
    );
}

// (g)
#[test]
fn deleting_the_open_drawing_returns_to_model_area() {
    let (mut st, _) = box_state(DVec3::splat(10.0));
    let d = new_drawing(&mut st, "Blatt 1");
    let v = add(&mut st, d, ViewKind::Front, s(1, 1));
    let mut ui = DrawingUi::default();
    open(&mut ui, d);
    ui.selected_view = Some(v);
    tick(&mut ui, &st, true);
    st.document.delete_drawing(d).unwrap();
    tick(&mut ui, &st, true);
    assert_eq!(ui.area, WorkArea::Model);
    assert_eq!(ui.selected_view, None);
    // Eine Zeichnung, die es nie gab (etwa aus einem anderen Tab), ebenso.
    ui.area = WorkArea::Drawing(DrawingId(999));
    tick(&mut ui, &st, true);
    assert_eq!(ui.area, WorkArea::Model);
    assert_eq!(ui.spawned(), 1);
}

#[test]
fn kinds_no_longer_needed_are_cancelled() {
    let (mut st, _) = box_state(DVec3::splat(10.0));
    let a = new_drawing(&mut st, "A");
    add(&mut st, a, ViewKind::Front, s(1, 1));
    add(&mut st, a, ViewKind::Top, s(1, 1));
    let b = new_drawing(&mut st, "B");
    add(&mut st, b, ViewKind::Front, s(1, 1));
    let mut ui = DrawingUi::default();
    ui.set_compute(wait_for_cancel);
    open(&mut ui, a);
    tick(&mut ui, &st, false);
    assert_eq!(ui.spawned(), 2);
    let front = ui.progress_of(ViewKind::Front).unwrap();
    let top = ui.progress_of(ViewKind::Top).unwrap();
    open(&mut ui, b);
    tick(&mut ui, &st, false);
    assert!(top.is_cancelled(), "„Oben“ braucht B nicht");
    assert!(!front.is_cancelled(), "„Vorne“ braucht B auch");
    assert!(ui.progress_of(ViewKind::Top).is_none());
    assert_eq!(
        ui.result(ViewKind::Top, st.document.model_edit_count()),
        ViewResult::Pending { fraction: 0.0 }
    );
    assert_eq!(ui.spawned(), 2);
}

// (h)
#[test]
fn suggest_scale_fits_the_three_views() {
    // Quader x = 100, y = 60, z = 40 auf A4 quer (297 × 210; das Blattzubehör ist das von A3
    // mal 0,707, freier Bereich 275,79 × 151,31 über dem Schriftfeld), Erstwinkel: Vorne
    // 100 × 40, Links 60 × 40, Oben 100 × 60.
    // Breite: 100·f + 20 + 60·f ≤ 275,79 → f ≤ 1,6; Höhe: 40·f + 20 + 60·f ≤ 151,31 → f ≤ 1,31.
    // Größte Reihenstufe: 1:1 (2:1 gäbe 220 > 151,31).
    let (st, _) = box_state(DVec3::new(100.0, 60.0, 40.0));
    for p in [Projection::First, Projection::Third] {
        assert_eq!(
            suggest_scale(&st.bodies, A4_QUER, p, ViewKind::Front),
            s(1, 1)
        );
    }
    let fits = |f: f64| 160.0 * f + 20.0 <= 275.79 && 100.0 * f + 20.0 <= 151.31;
    assert!(fits(1.0) && !fits(2.0), "Gegenprobe");

    // Zehnfach: Breite 1600·f + 20 ≤ 275,79 → f ≤ 0,160; Höhe 1000·f + 20 ≤ 151,31 →
    // f ≤ 0,131 → 1:10 (120 ≤ 151,31).
    let (big, _) = box_state(DVec3::new(1000.0, 600.0, 400.0));
    assert_eq!(
        suggest_scale(&big.bodies, A4_QUER, Projection::First, ViewKind::Front),
        s(1, 10)
    );
    // Ein Zehntel: Breite 16·f + 20 ≤ 275,79 → f ≤ 16,0; Höhe 10·f + 20 ≤ 151,31 → f ≤ 13,1
    // → 10:1 (120 ≤ 151,31).
    let (small, _) = box_state(DVec3::new(10.0, 6.0, 4.0));
    assert_eq!(
        suggest_scale(&small.bodies, A4_QUER, Projection::First, ViewKind::Front),
        s(10, 1)
    );
    // A4 hoch (freier Bereich 188,79 × 222,75), Quader 300 × 60 × 40: Breite 360·f + 20 ≤
    // 188,79 → f ≤ 0,469; 1:2 gäbe 200 > 188,79, also 1:5 (92 ≤ 188,79).
    let (wide, _) = box_state(DVec3::new(300.0, 60.0, 40.0));
    let hoch = Sheet {
        paper: Paper::A4,
        landscape: false,
    };
    assert_eq!(
        suggest_scale(&wide.bodies, hoch, Projection::First, ViewKind::Front),
        s(1, 5)
    );
    // Nichts passt (selbst 1:100 nicht): kleinste Stufe 1:100.
    let (huge, _) = box_state(DVec3::new(50_000.0, 30_000.0, 20_000.0));
    assert_eq!(
        suggest_scale(&huge.bodies, A4_QUER, Projection::First, ViewKind::Front),
        s(1, 100)
    );
}

#[test]
fn view_size_comes_from_the_projected_bounds() {
    let (st, _) = box_state(DVec3::new(100.0, 60.0, 40.0));
    let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
    assert!(close(
        estimate_view_size(&st.bodies, ViewKind::Front),
        (100.0, 40.0)
    ));
    assert!(close(
        estimate_view_size(&st.bodies, ViewKind::Back),
        (100.0, 40.0)
    ));
    assert!(close(
        estimate_view_size(&st.bodies, ViewKind::Left),
        (60.0, 40.0)
    ));
    assert!(close(
        estimate_view_size(&st.bodies, ViewKind::Right),
        (60.0, 40.0)
    ));
    assert!(close(
        estimate_view_size(&st.bodies, ViewKind::Top),
        (100.0, 60.0)
    ));
    assert!(close(
        estimate_view_size(&st.bodies, ViewKind::Bottom),
        (100.0, 60.0)
    ));
    // Isometrisch, von Hand: ex = (1, 1, 0)/√2, ey = (−1, 1, 2)/√6.
    // Breite (100 + 60)/√2; Höhe (100 + 60 + 2·40)/√6.
    let iso = estimate_view_size(&st.bodies, ViewKind::Isometric);
    assert!(
        close(iso, (160.0 / 2f64.sqrt(), 240.0 / 6f64.sqrt())),
        "{iso:?}"
    );
    // Netz-Körper zählen nicht mit.
    let mut mixed = AppState::new();
    mixed
        .import_mesh("Groß", Arc::new(cube_mesh()), 500.0)
        .unwrap();
    mixed.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    assert!(close(
        estimate_view_size(&mixed.bodies, ViewKind::Front),
        (100.0, 40.0)
    ));
    assert_eq!(estimate_view_size(&[], ViewKind::Front), (0.0, 0.0));
}

// (i)
#[test]
fn default_distance_matches_half_extents_plus_gap() {
    let (st, _) = box_state(DVec3::new(100.0, 60.0, 40.0));
    let b = &st.bodies;
    // Oben unter/über Vorne (senkrecht): Höhe Vorne 40 bei 1:1, Höhe Oben 60 bei 1:2.
    // 40/2 + 20 + 30/2 = 55.
    let dist = default_distance(b, ViewKind::Front, ViewKind::Top, s(1, 1), s(1, 2));
    assert!((dist - 55.0).abs() < 1e-9, "{dist}");
    // Links neben Vorne (waagerecht): Breite Vorne 100 bei 1:1, Breite Links 60 bei 2:1.
    // 100/2 + 20 + 120/2 = 130.
    let dist = default_distance(b, ViewKind::Front, ViewKind::Left, s(1, 1), s(2, 1));
    assert!((dist - 130.0).abs() < 1e-9, "{dist}");
    // Rechts neben Oben (waagerecht), Elternmaßstab 1:2: Breite Oben 100·0,5, Breite Rechts
    // (y-Ausdehnung 60) bei 1:1 → 25 + 20 + 30 = 75.
    let dist = default_distance(b, ViewKind::Top, ViewKind::Right, s(1, 2), s(1, 1));
    assert!((dist - 75.0).abs() < 1e-9, "{dist}");
    // Unten unter Vorne: Höhen 40 und 60 → 20 + 20 + 30 = 70.
    let dist = default_distance(b, ViewKind::Front, ViewKind::Bottom, s(1, 1), s(1, 1));
    assert!((dist - 70.0).abs() < 1e-9, "{dist}");
}

// (j)
#[test]
fn shared_cache_between_drawings() {
    let (mut st, id) = box_state(DVec3::splat(10.0));
    let a = new_drawing(&mut st, "A");
    add(&mut st, a, ViewKind::Front, s(1, 1));
    add(&mut st, a, ViewKind::Top, s(1, 1));
    let b = new_drawing(&mut st, "B");
    add(&mut st, b, ViewKind::Front, s(1, 2));
    let mut ui = DrawingUi::default();
    open(&mut ui, a);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 2);
    open(&mut ui, b);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 2, "Vorne liegt schon vor");
    open(&mut ui, a);
    tick(&mut ui, &st, true);
    assert_eq!(
        ui.spawned(),
        2,
        "fertiges „Oben“ bleibt beim Wechsel erhalten"
    );
    assert!(MAX_CACHED >= ViewKind::ALL.len());
    // Alle sieben Arten: jede einmal, alle bleiben (MAX_CACHED = 7).
    let c = new_drawing(&mut st, "C");
    for k in ViewKind::ALL {
        add(&mut st, c, k, s(1, 1));
    }
    open(&mut ui, c);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 7);
    open(&mut ui, b);
    tick(&mut ui, &st, true);
    open(&mut ui, c);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 7);
    // Modelländerung: alte Linien verschwinden auch für nicht gebrauchte Arten.
    resize_box(&mut st, id, 20.0);
    open(&mut ui, b);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 8, "nur „Vorne“ für B");
    assert_eq!(
        ui.result(ViewKind::Top, st.document.model_edit_count()),
        ViewResult::Pending { fraction: 0.0 }
    );
    open(&mut ui, a);
    tick(&mut ui, &st, true);
    assert_eq!(ui.spawned(), 9);
    assert!((width(&ready(&ui, &st, ViewKind::Top)) - 20.0).abs() < 1e-6);
}

#[test]
fn the_app_state_has_a_drawing_ui_in_the_model_area() {
    let st = AppState::new();
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    assert_eq!(st.drawing_ui.selected_view, None);
    assert_eq!(st.drawing_ui.zoom, 1.0);
    assert_eq!(st.drawing_ui.pan, egui::Vec2::ZERO);
    assert_eq!(st.drawing_ui.spawned(), 0);
}

#[test]
fn old_lines_are_never_read_as_current_without_a_tick() {
    let (mut st, id) = box_state(DVec3::splat(10.0));
    let d = new_drawing(&mut st, "Blatt 1");
    let v = add(&mut st, d, ViewKind::Front, s(1, 1));
    let mut ui = DrawingUi::default();
    open(&mut ui, d);
    tick(&mut ui, &st, true);
    let drawing = st.document.drawing(d).unwrap().clone();
    assert!(
        ui.results_for(&drawing, st.document.model_edit_count())
            .contains_key(&v)
    );
    resize_box(&mut st, id, 20.0);
    // Ohne `tick`: der alte Stand gilt nicht mehr.
    let model = st.document.model_edit_count();
    assert!(ui.results_for(&drawing, model).is_empty());
    assert_eq!(
        ui.result(ViewKind::Front, model),
        ViewResult::Pending { fraction: 0.0 }
    );
    // Auch im Modellbereich (dort kehrt `tick` sofort zurück).
    ui.area = WorkArea::Model;
    tick(&mut ui, &st, true);
    assert!(ui.results_for(&drawing, model).is_empty());
}

#[test]
fn switching_drawings_clears_the_selection_and_the_sheet_view() {
    let (mut st, _) = box_state(DVec3::splat(10.0));
    let a = new_drawing(&mut st, "A");
    let va = add(&mut st, a, ViewKind::Front, s(1, 1));
    let b = new_drawing(&mut st, "B");
    let vb = add(&mut st, b, ViewKind::Top, s(1, 1));
    // Kennungen werden je Zeichnung gezählt: dieselbe Zahl, eine andere Ansicht.
    assert_eq!(va, vb);
    let mut ui = DrawingUi::default();
    ui.open(a);
    assert_eq!(ui.area, WorkArea::Drawing(a));
    ui.selected_view = Some(va);
    ui.zoom = 3.0;
    ui.pan = egui::vec2(5.0, 6.0);
    tick(&mut ui, &st, true);
    assert_eq!(
        ui.selected_view,
        Some(va),
        "dieselbe Zeichnung behält die Wahl"
    );
    assert_eq!(ui.zoom, 3.0);
    // Über `open`.
    ui.open(b);
    assert_eq!(ui.area, WorkArea::Drawing(b));
    assert_eq!(ui.selected_view, None);
    assert_eq!(ui.zoom, 1.0);
    assert_eq!(ui.pan, egui::Vec2::ZERO);
    tick(&mut ui, &st, true);
    // Sicherheitsnetz: Bereich direkt gesetzt, `tick` merkt den Wechsel.
    ui.selected_view = Some(vb);
    ui.zoom = 2.0;
    ui.pan = egui::vec2(1.0, 1.0);
    ui.area = WorkArea::Drawing(a);
    tick(&mut ui, &st, true);
    assert_eq!(ui.selected_view, None);
    assert_eq!(ui.zoom, 1.0);
    assert_eq!(ui.pan, egui::Vec2::ZERO);
    // Zurück ins Modell und wieder in dieselbe Zeichnung: Wahl bleibt.
    ui.selected_view = Some(va);
    ui.area = WorkArea::Model;
    tick(&mut ui, &st, true);
    ui.area = WorkArea::Drawing(a);
    tick(&mut ui, &st, true);
    assert_eq!(ui.selected_view, Some(va));
}

#[test]
fn suggest_scale_without_exact_bodies_is_one_to_one() {
    assert_eq!(
        suggest_scale(&[], A4_QUER, Projection::First, ViewKind::Front),
        s(1, 1)
    );
    let mut st = AppState::new();
    st.import_mesh("Würfel", Arc::new(cube_mesh()), 1.0)
        .unwrap();
    assert_eq!(
        suggest_scale(&st.bodies, A4_QUER, Projection::Third, ViewKind::Front),
        s(1, 1)
    );
}
