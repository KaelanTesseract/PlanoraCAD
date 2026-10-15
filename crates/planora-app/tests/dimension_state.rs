//! Maße in der App (Teilprojekt 12, Aufgabe 10): Auflösen mit Zwischenspeicher, letzte
//! bekannte Lage verlorener Maße, Hinweise, Vorschlag der Grundmaße und das Blatt mit Marken.
//!
//! Sollwerte aus den Eingabemaßen der Körper: Quader 10 × 20 × 30 (x, y, z) → Vorderansicht
//! Breite 10, Höhe 30; Draufsicht Tiefe 20; Zylinder r = 5, h = 12 → Ø10, Höhe 12.

use egui::{Pos2, Rect, vec2};
use planora_app::dimension_state::*;
use planora_app::drawing_view;
use planora_app::fonts;
use planora_app::state::*;
use planora_drawing::annotate::{Mark, MarkKind, ResolvedDims, TextAnchor};
use planora_drawing::dimension::{
    ChainMode, ChainSpec, DimError, DimId, DimKind, DimSpec, GeoRef, GeoRefData,
    Tolerance as DimTolerance,
};
use planora_drawing::limits::MAX_DIMENSIONS;
use planora_drawing::sheet::build_sheet_with;
use planora_drawing::{
    Anchor, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewLines,
};
use planora_exchange::ExchangeMesh;
use planora_feature::{FeatureId, FeatureKind, geo_ref_for_edge};
use planora_kernel::boolean::BoolOp;
use planora_math::{DVec3, Tolerance};
use std::collections::BTreeMap;
use std::sync::Arc;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

const BOX: DVec3 = DVec3::new(10.0, 20.0, 30.0);

fn s11() -> Scale {
    Scale { num: 1, den: 1 }
}

fn new_drawing(st: &mut AppState) -> DrawingId {
    st.document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap()
}

fn add_view(st: &mut AppState, d: DrawingId, kind: ViewKind, x: f64, y: f64) -> ViewId {
    st.document
        .add_view(d, kind, s11(), Placement::Free { x, y }, None)
        .unwrap()
}

/// Öffnet die Zeichnung und wartet, bis alle Ansichten berechnet sind.
fn open_and_compute(st: &mut AppState, d: DrawingId) {
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
}

fn compute(st: &mut AppState) {
    st.drawing_ui.tick(&st.bodies, &st.document, true);
}

/// Quader 10 × 20 × 30 mit Zeichnung und Vorderansicht in (100, 100).
fn box_drawing() -> (AppState, FeatureId, DrawingId, ViewId) {
    let mut st = AppState::new();
    let f = st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    (st, f, d, front)
}

/// Bounds des Körpers `body`.
fn bounds(st: &AppState, body: usize) -> (DVec3, DVec3) {
    st.bodies[body].bounds
}

/// Verweisdaten der geraden Kante des Körpers `body`, deren beide Eckpunkte `pred` erfüllen.
fn edge_where(st: &AppState, body: usize, pred: impl Fn(DVec3) -> bool) -> GeoRefData {
    let solid = &st.bodies[body].solid;
    let (id, _) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && pred(a) && pred(b)
        })
        .expect("Kante nicht gefunden");
    geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Linke und rechte senkrechte Vorderkante (Breite in der Vorderansicht).
fn width_refs(st: &AppState, body: usize) -> Vec<GeoRef> {
    let (lo, hi) = bounds(st, body);
    vec![
        GeoRef::Edge(edge_where(st, body, |p| near(p.x, lo.x) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, body, |p| near(p.x, hi.x) && near(p.y, lo.y))),
    ]
}

/// Untere und obere waagerechte Vorderkante (Höhe in der Vorderansicht).
fn height_refs(st: &AppState, body: usize) -> Vec<GeoRef> {
    let (lo, hi) = bounds(st, body);
    vec![
        GeoRef::Edge(edge_where(st, body, |p| near(p.z, lo.z) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, body, |p| near(p.z, hi.z) && near(p.y, lo.y))),
    ]
}

fn spec(view: ViewId, kind: DimKind, refs: Vec<GeoRef>, offset: f64) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind,
        refs,
        offset,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn add_width(st: &mut AppState, d: DrawingId, view: ViewId, offset: f64) -> DimId {
    let refs = width_refs(st, 0);
    st.document
        .add_dimension(d, spec(view, DimKind::Horizontal, refs, offset))
        .unwrap()
}

fn add_height(st: &mut AppState, d: DrawingId, view: ViewId, offset: f64) -> DimId {
    let refs = height_refs(st, 0);
    st.document
        .add_dimension(d, spec(view, DimKind::Vertical, refs, offset))
        .unwrap()
}

/// Messwert des Maßes `id` (Fehler → Panik mit Text).
fn value(r: &ResolvedDims, id: DimId) -> f64 {
    let (_, res) = r
        .by_view
        .values()
        .flatten()
        .find(|(s, _)| s.id == id)
        .unwrap_or_else(|| panic!("Maß {id:?} fehlt"));
    res.as_ref().unwrap_or_else(|e| panic!("{e}")).value
}

fn entry(r: &ResolvedDims, id: DimId) -> (DimSpec, Result<planora_drawing::Measured, DimError>) {
    r.by_view
        .values()
        .flatten()
        .find(|(s, _)| s.id == id)
        .cloned()
        .unwrap_or_else(|| panic!("Maß {id:?} fehlt"))
}

fn resize_box(st: &mut AppState, f: FeatureId, size: DVec3) {
    st.document
        .update(f, |k| {
            if let FeatureKind::Box { size: s, .. } = k {
                *s = size;
            }
        })
        .unwrap();
    st.refresh();
}

fn dims_of(st: &AppState, d: DrawingId) -> Vec<DimSpec> {
    st.document.drawing(d).unwrap().dimensions.clone()
}

// (a)
#[test]
fn resolve_gives_values_for_a_box_after_adding_overall_dimensions() {
    let (mut st, _, d, front) = box_drawing();
    let w = add_width(&mut st, d, front, 10.0);
    let h = add_height(&mut st, d, front, 10.0);
    let mut ui = DimensionUi::default();
    let r = ui.resolve(&st, d);
    assert!(near(value(&r, w), BOX.x), "Breite {}", value(&r, w));
    assert!(near(value(&r, h), BOX.z), "Höhe {}", value(&r, h));
    assert_eq!(r.by_view.len(), 1);
    assert_eq!(r.by_view[&front].len(), 2);
    assert_eq!(
        entry(&r, w).0,
        st.document
            .drawing(d)
            .unwrap()
            .dimension(w)
            .unwrap()
            .clone()
    );
    // Letzte Lagen sind gemerkt.
    assert!(r.last_known.contains_key(&w) && r.last_known.contains_key(&h));
    // Netz-Körper zählen nicht: ein Netz daneben ändert nichts an den Werten.
    st.import_mesh("Würfel", Arc::new(cube_mesh()), 1.0)
        .unwrap();
    let r = ui.resolve(&st, d);
    assert!(near(value(&r, w), BOX.x));
    // Unbekannte Zeichnung: leer, kein Absturz.
    assert!(ui.resolve(&st, DrawingId(99)).by_view.is_empty());
}

// (b)
#[test]
fn model_edit_updates_values_without_recomputing_views() {
    let (mut st, f, d, front) = box_drawing();
    open_and_compute(&mut st, d);
    let s0 = st.drawing_ui.spawned();
    assert_eq!(s0, 1, "eine Ansichtsart");
    let mut ui = DimensionUi::default();
    assert!(ui.resolve(&st, d).by_view.is_empty());
    // Zeichnungsänderung (neues Maß) bei gleichem Modellstand: wird aufgelöst.
    let w = add_width(&mut st, d, front, 10.0);
    assert!(near(value(&ui.resolve(&st, d), w), 10.0));
    resize_box(&mut st, f, DVec3::new(20.0, 20.0, 30.0));
    let r = ui.resolve(&st, d);
    assert!(
        near(value(&r, w), 20.0),
        "Wert folgt dem Modell: {}",
        value(&r, w)
    );
    assert_eq!(
        st.drawing_ui.spawned(),
        s0,
        "Auflösen rechnet keine Ansicht"
    );
    compute(&mut st);
    assert_eq!(st.drawing_ui.spawned(), s0 + 1, "nur die Ansicht selbst");
    assert!(Arc::ptr_eq(&r, &ui.resolve(&st, d)));
}

// (c)
#[test]
fn deleted_reference_becomes_lost_with_reason_and_keeps_last_anchor() {
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let b = st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 120.0, 100.0);
    // Breite des zweiten Quaders (Körper 1).
    let refs = width_refs(&st, 1);
    let id = st
        .document
        .add_dimension(d, spec(front, DimKind::Horizontal, refs, 10.0))
        .unwrap();
    open_and_compute(&mut st, d);
    let mut ui = DimensionUi::default();
    let before = ui.resolve(&st, d);
    assert!(near(value(&before, id), BOX.x));
    let anchor: Anchor = before.last_known[&id].clone();

    st.delete_feature(b).unwrap();
    compute(&mut st);
    let after = ui.resolve(&st, d);
    let (_, res) = entry(&after, id);
    match res {
        Err(DimError::Lost(reason)) => {
            assert!(reason.contains("nicht mehr vorhanden"), "Grund: {reason}")
        }
        other => panic!("verloren erwartet: {other:?}"),
    }
    assert_eq!(after.last_known.get(&id), Some(&anchor), "alte Lage bleibt");
    let notes = ui.notes(&st, d, &after);
    assert_eq!(notes.lost, 1);
    assert_eq!(
        notes.texts,
        vec!["1 Maß hat seinen Bezug verloren.".to_string()]
    );
    // Auf dem Blatt rot an der alten Stelle: die Maßlinie liegt wie vorher 10 mm über der
    // Oberkante (Ansicht 30 hoch um y 100 → Linie y 125) und ist 10 mm lang. (Die Ansicht ist
    // jetzt schmaler, deshalb wird nur die Höhe und die Länge verglichen.)
    let marks = sheet_marks(&st, d, front, &after);
    assert!(!marks.is_empty());
    assert!(
        marks.iter().all(|m| kind_of(m) == MarkKind::Lost),
        "{marks:?}"
    );
    assert!(
        marks.iter().any(|m| matches!(m, Mark::Line { a, b, .. }
            if near(a.y, 125.0) && near(b.y, 125.0) && near((a.x - b.x).abs(), 10.0))),
        "{marks:?}"
    );
    assert!(
        marks
            .iter()
            .any(|m| matches!(m, Mark::Text { text, .. } if text.starts_with("? ")))
    );

    // Maß gelöscht → Eintrag verfällt.
    st.document.delete_dimension(d, id).unwrap();
    let gone = ui.resolve(&st, d);
    assert!(gone.by_view.is_empty());
    assert!(!gone.last_known.contains_key(&id));
}

// (d)
#[test]
fn cached_refuses_stale_states() {
    let (mut st, f, d, front) = box_drawing();
    let w = add_width(&mut st, d, front, 10.0);
    let mut ui = DimensionUi::default();
    assert!(
        ui.cached(d, st.document.model_edit_count(), st.document.edit_count())
            .is_none()
    );
    let r = ui.resolve(&st, d);
    let c = ui
        .cached(d, st.document.model_edit_count(), st.document.edit_count())
        .unwrap();
    assert!(Arc::ptr_eq(&r, &c));
    assert!(
        ui.cached(
            DrawingId(d.0 + 1),
            st.document.model_edit_count(),
            st.document.edit_count()
        )
        .is_none()
    );
    // Modelländerung ohne `resolve`: nichts.
    resize_box(&mut st, f, DVec3::new(20.0, 20.0, 30.0));
    assert!(
        ui.cached(d, st.document.model_edit_count(), st.document.edit_count())
            .is_none()
    );
    let r = ui.resolve(&st, d);
    assert!(near(value(&r, w), 20.0));
    // Nur Zeichnungsänderung ohne `resolve`: ebenfalls nichts.
    st.document
        .update_dimension(d, w, |m| m.offset = 17.0)
        .unwrap();
    assert!(
        ui.cached(d, st.document.model_edit_count(), st.document.edit_count())
            .is_none()
    );
}

/// Vorschläge als (Art, Wert) sortiert.
fn kinds_and_values(st: &AppState, d: DrawingId) -> Vec<(String, f64)> {
    let mut ui = DimensionUi::default();
    let r = ui.resolve(st, d);
    let mut v: Vec<(String, f64)> = r
        .by_view
        .values()
        .flatten()
        .map(|(s, res)| {
            let m = res.as_ref().unwrap_or_else(|e| panic!("{e}"));
            (format!("{:?}", s.kind), (m.value * 1e6).round() / 1e6)
        })
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

// (e)
#[test]
fn suggest_adds_overall_and_diameter_dimensions_once() {
    // Quader: Breite und Höhe vorne, Tiefe oben (die Breite oben ist dieselbe).
    let (mut st, _, d, front) = box_drawing();
    let top = add_view(&mut st, d, ViewKind::Top, 100.0, 200.0);
    open_and_compute(&mut st, d);
    let edits = st.document.edit_count();
    let out = suggest_for(&mut st, d, None);
    assert_eq!(out, SuggestOutcome::Added(3), "{}", out.message());
    assert_eq!(out.message(), "3 Maße wurden hinzugefügt.");
    assert_eq!(st.document.edit_count(), edits + 3, "je Maß eine Änderung");
    let dims = dims_of(&st, d);
    let mut seen: Vec<(ViewId, DimKind)> = dims.iter().map(|m| (m.view, m.kind)).collect();
    seen.sort_by_key(|(v, k)| (v.0, format!("{k:?}")));
    assert_eq!(
        seen,
        vec![
            (front, DimKind::Horizontal),
            (front, DimKind::Vertical),
            (top, DimKind::Vertical)
        ]
    );
    for m in &dims {
        assert!(m.suggested);
        assert_eq!(m.decimals, 1);
        assert_eq!(m.offset, 10.0, "erste Maßlinie je Ansicht und Richtung");
        assert_eq!(m.chain, None);
    }
    assert_eq!(
        kinds_and_values(&st, d),
        vec![
            ("Horizontal".into(), BOX.x),
            ("Vertical".into(), BOX.y),
            ("Vertical".into(), BOX.z)
        ]
    );
    // Zweites Drücken: nichts Neues.
    let edits = st.document.edit_count();
    assert_eq!(suggest_for(&mut st, d, None), SuggestOutcome::NothingNew);
    assert_eq!(dims_of(&st, d).len(), 3);
    assert_eq!(st.document.edit_count(), edits);

    // Zylinder r = 5, h = 12: Ø oben (Kreis als Kreis), Höhe vorne.
    let mut st = AppState::new();
    st.create_cylinder(5.0, 12.0).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let top = add_view(&mut st, d, ViewKind::Top, 100.0, 160.0);
    open_and_compute(&mut st, d);
    let out = suggest_for(&mut st, d, None);
    assert_eq!(out, SuggestOutcome::Added(2), "{}", out.message());
    let dims = dims_of(&st, d);
    assert!(
        dims.iter()
            .any(|m| m.view == top && m.kind == DimKind::Diameter)
    );
    assert!(
        dims.iter()
            .any(|m| m.view == front && m.kind == DimKind::Vertical)
    );
    assert_eq!(
        kinds_and_values(&st, d),
        vec![("Diameter".into(), 10.0), ("Vertical".into(), 12.0)]
    );
    assert_eq!(suggest_for(&mut st, d, None), SuggestOutcome::NothingNew);
    assert_eq!(dims_of(&st, d).len(), 2);
}

#[test]
fn suggest_stacks_by_direction_and_gives_drawing_unique_groups() {
    // Platte 60 × 40 × 10 mit zwei Bohrungen r = 4 (Mitten x 15 / 45, y 20).
    let mut st = AppState::new();
    st.create_box(DVec3::new(60.0, 40.0, 10.0)).unwrap();
    let (lo, _) = bounds(&st, 0);
    for x in [15.0, 45.0] {
        st.add_feature(
            None,
            FeatureKind::Cylinder {
                radius: 4.0,
                height: 10.0,
                origin: lo + DVec3::new(x, 20.0, 0.0),
            },
        )
        .unwrap();
        let (c0, c1) = bounds(&st, 1);
        assert!(
            near(c1.z - c0.z, 10.0) && near(c1.x - c0.x, 8.0),
            "Zylinder steht in z"
        );
        st.add_boolean(0, 1, BoolOp::Difference).unwrap();
    }
    assert_eq!(st.bodies.len(), 1);
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 80.0, 60.0);
    let top = add_view(&mut st, d, ViewKind::Top, 80.0, 140.0);
    // Ein vorhandenes Bezugsmaß mit Gruppe 1 in der Vorderansicht.
    let mut pre = spec(front, DimKind::Horizontal, width_refs(&st, 0), 30.0);
    pre.chain = Some(ChainSpec {
        group: 1,
        mode: ChainMode::Baseline,
        order: 0,
    });
    st.document.add_dimension(d, pre).unwrap();
    open_and_compute(&mut st, d);
    let out = suggest_for(&mut st, d, Some(top));
    let SuggestOutcome::Added(n) = out else {
        panic!("{}", out.message());
    };
    let dims = dims_of(&st, d);
    assert_eq!(dims.len(), n + 1);
    assert!(
        dims.iter().skip(1).all(|m| m.view == top),
        "nur die gewählte Ansicht"
    );
    // Gruppen: neu nummeriert (nie 1), je Gruppe eine Richtung, Stufen 0, 1, … ohne Lücke.
    let mut groups: BTreeMap<u64, Vec<&DimSpec>> = BTreeMap::new();
    for m in dims.iter().skip(1) {
        if let Some(c) = m.chain {
            groups.entry(c.group).or_default().push(m);
        }
    }
    assert!(!groups.is_empty(), "Bohrungsmitten als Bezugsmaße");
    for (g, members) in &groups {
        assert!(*g >= 2, "Gruppe {g} kollidiert mit der vorhandenen");
        let orders: Vec<u32> = members.iter().map(|m| m.chain.unwrap().order).collect();
        assert_eq!(orders, (0..members.len() as u32).collect::<Vec<_>>());
        assert!(
            members
                .iter()
                .all(|m| m.kind == members[0].kind && m.offset == members[0].offset)
        );
    }
    // Stapeln nach P3: je Richtung 10, 17, 24 … (eine Gruppe belegt so viele Stufen wie Glieder).
    let mut ui = DimensionUi::default();
    let r = ui.resolve(&st, d);
    for (h, kind) in [(true, DimKind::Horizontal), (false, DimKind::Vertical)] {
        let mut levels: Vec<f64> = Vec::new();
        for (s, res) in &r.by_view[&top] {
            let m = res.as_ref().unwrap_or_else(|e| panic!("{e}"));
            let Anchor::Linear { dir, .. } = m.anchor else {
                continue;
            };
            if (dir.x.abs() >= dir.y.abs()) != h {
                continue;
            }
            let order = s.chain.map_or(0, |c| c.order) as f64;
            levels.push(s.offset + 7.0 * order);
        }
        levels.sort_by(f64::total_cmp);
        let expected: Vec<f64> = (0..levels.len()).map(|i| 10.0 + 7.0 * i as f64).collect();
        assert_eq!(levels, expected, "{kind:?}");
    }
    assert!(r.by_view.values().flatten().all(|(_, x)| x.is_ok()));
    assert_eq!(
        suggest_for(&mut st, d, Some(top)),
        SuggestOutcome::NothingNew
    );
}

// (f)
#[test]
fn suggest_waits_for_views() {
    let (mut st, _, d, _) = box_drawing();
    // Nicht geöffnet / noch nicht berechnet.
    assert_eq!(
        suggest_for(&mut st, d, None),
        SuggestOutcome::Unavailable("Die Ansichten werden noch berechnet.".into())
    );
    assert!(dims_of(&st, d).is_empty());
    // Berechnet, dann Modelländerung: die alten Linien zählen nicht.
    open_and_compute(&mut st, d);
    st.create_box(BOX).unwrap();
    assert_eq!(
        suggest_for(&mut st, d, None),
        SuggestOutcome::Unavailable(VIEWS_PENDING.into())
    );
}

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

// (g)
#[test]
fn suggest_on_mesh_only_or_empty_document_is_named() {
    // Leeres Dokument.
    let mut st = AppState::new();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    open_and_compute(&mut st, d);
    assert_eq!(
        suggest_for(&mut st, d, None),
        SuggestOutcome::Unavailable("Es gibt keinen Körper, der sich zeichnen lässt.".into())
    );
    assert!(dims_of(&st, d).is_empty());
    // Nur Netz-Körper.
    st.import_mesh("Würfel", Arc::new(cube_mesh()), 10.0)
        .unwrap();
    compute(&mut st);
    assert_eq!(
        suggest_for(&mut st, d, Some(front)),
        SuggestOutcome::Unavailable(
            "Alle Körper sind Netze; eine Zeichnung braucht exakte Körper.".into()
        )
    );
    // Isometrie, Zeichnung ohne Normansicht, unbekannte Ansicht und Zeichnung.
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let iso = add_view(&mut st, d, ViewKind::Isometric, 100.0, 100.0);
    open_and_compute(&mut st, d);
    assert_eq!(
        suggest_for(&mut st, d, Some(iso)).message(),
        "In der isometrischen Ansicht gibt es keine Maße."
    );
    assert_eq!(
        suggest_for(&mut st, d, None).message(),
        "Die Zeichnung hat noch keine Ansicht, in der sich Maße vorschlagen lassen."
    );
    assert_eq!(
        suggest_for(&mut st, d, Some(ViewId(77))).message(),
        "Die Ansicht gibt es nicht."
    );
    assert_eq!(
        suggest_for(&mut st, DrawingId(77), None).message(),
        "Die Zeichnung gibt es nicht."
    );
    assert!(dims_of(&st, d).is_empty());
}

// (h)
#[test]
fn suggest_respects_max_dimensions() {
    let (mut st, _, d, front) = box_drawing();
    add_view(&mut st, d, ViewKind::Top, 100.0, 200.0);
    let width = spec(front, DimKind::Horizontal, width_refs(&st, 0), 10.0);
    st.document
        .update_drawing(d, |dr| {
            for _ in 0..MAX_DIMENSIONS - 1 {
                dr.add_dimension(width.clone());
            }
        })
        .unwrap();
    open_and_compute(&mut st, d);
    // Platz für eines: Höhe vorne und Tiefe oben wären neu, nur eines passt.
    let out = suggest_for(&mut st, d, None);
    assert_eq!(
        out,
        SuggestOutcome::Partly {
            added: 1,
            skipped: vec![
                "Die Zeichnung hat schon die Höchstzahl von 2000 Maßen. 1 weiteres Maß wurde nicht gesetzt."
                    .into()
            ]
        }
    );
    assert_eq!(dims_of(&st, d).len(), MAX_DIMENSIONS);
    let out = suggest_for(&mut st, d, None);
    assert_eq!(
        out,
        SuggestOutcome::Unavailable(
            "Die Zeichnung hat schon die Höchstzahl von 2000 Maßen.".into()
        )
    );
    assert_eq!(dims_of(&st, d).len(), MAX_DIMENSIONS);
}

/// Markierungen der Ansicht `view` im Blattmodell mit den Maßen `dims`.
fn sheet_marks(st: &AppState, d: DrawingId, view: ViewId, dims: &ResolvedDims) -> Vec<Mark> {
    let drawing = st.document.drawing(d).unwrap();
    let owned = st
        .drawing_ui
        .results_for(drawing, st.document.model_edit_count());
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, v)| (*k, v.as_ref())).collect();
    build_sheet_with(drawing, &refs, dims)
        .views
        .into_iter()
        .find(|v| v.id == view)
        .map(|v| {
            v.marks
                .into_iter()
                .filter(|m| kind_of(m) != MarkKind::CenterLine)
                .collect()
        })
        .unwrap_or_default()
}

fn kind_of(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

// (i)
#[test]
fn notes_count_lost_and_outside_the_sheet() {
    let (mut st, _, d, front) = box_drawing();
    add_width(&mut st, d, front, 10.0);
    let far = add_height(&mut st, d, front, 5000.0);
    open_and_compute(&mut st, d);
    let mut ui = DimensionUi::default();
    let r = ui.resolve(&st, d);
    let notes = ui.notes(&st, d, &r);
    assert_eq!(notes.lost, 0);
    assert_eq!(notes.outside, 1);
    assert_eq!(
        notes.texts,
        vec!["1 Maß liegt außerhalb des Blatts.".to_string()]
    );
    // Das Maß wird trotzdem gezeichnet.
    let marks = sheet_marks(&st, d, front, &r);
    assert!(
        marks
            .iter()
            .any(|m| matches!(m, Mark::Line { a, .. } if a.x > 5000.0))
    );
    // Zurückgeholt: kein Hinweis mehr.
    st.document
        .update_dimension(d, far, |m| m.offset = 10.0)
        .unwrap();
    let r = ui.resolve(&st, d);
    assert_eq!(ui.notes(&st, d, &r), DimNotes::default());
    // Ohne Ansichtsergebnis (Modell geändert, noch nicht berechnet) zählt nichts als außerhalb.
    st.document
        .update_dimension(d, far, |m| m.offset = 5000.0)
        .unwrap();
    st.create_box(BOX).unwrap();
    let r = ui.resolve(&st, d);
    assert_eq!(ui.notes(&st, d, &r).outside, 0);
    compute(&mut st);
    assert_eq!(ui.notes(&st, d, &r).outside, 1);
    // Zwei Maße außerhalb, eines verloren (Mehrzahl).
    let refs = vec![GeoRef::Edge(GeoRefData::Edge {
        a: planora_drawing::dimension::TagData {
            origin: 999,
            role: "weg".into(),
            hint: DVec3::ZERO,
        },
        b: planora_drawing::dimension::TagData {
            origin: 999,
            role: "auch weg".into(),
            hint: DVec3::ZERO,
        },
        hint: DVec3::ZERO,
    })];
    let mut lost = spec(front, DimKind::Horizontal, refs.clone(), 10.0);
    lost.refs.push(refs[0].clone());
    st.document.add_dimension(d, lost.clone()).unwrap();
    st.document.add_dimension(d, lost).unwrap();
    add_width(&mut st, d, front, -6000.0);
    let r = ui.resolve(&st, d);
    let notes = ui.notes(&st, d, &r);
    assert_eq!(notes.lost, 2);
    assert_eq!(notes.outside, 2);
    assert_eq!(
        notes.texts,
        vec![
            "2 Maße haben ihren Bezug verloren.".to_string(),
            "2 Maße liegen außerhalb des Blatts.".to_string()
        ]
    );
}

// (j)
#[test]
fn resolve_is_cheap_when_nothing_changed() {
    let (mut st, _, d, front) = box_drawing();
    let w = add_width(&mut st, d, front, 10.0);
    let mut ui = DimensionUi::default();
    let a = ui.resolve(&st, d);
    let b = ui.resolve(&st, d);
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(ui.resolutions(), 1);
    // Ansicht verschoben: neuer Stand, aber keine neue Messung (Bezüge unverändert).
    st.document
        .update_drawing(d, |dr| {
            dr.views[0].placement = Placement::Free { x: 150.0, y: 90.0 }
        })
        .unwrap();
    let c = ui.resolve(&st, d);
    assert!(!Arc::ptr_eq(&a, &c));
    assert_eq!(ui.resolutions(), 1);
    assert!(near(value(&c, w), BOX.x));
    // Neues Maß: wird gemessen.
    add_height(&mut st, d, front, 10.0);
    ui.resolve(&st, d);
    assert_eq!(ui.resolutions(), 2);
    // Bezüge eines Maßes geändert (gleiche Art): wird neu gemessen, nicht übernommen.
    let (lo, hi) = bounds(&st, 0);
    let refs = vec![
        GeoRef::Edge(edge_where(&st, 0, |p| near(p.x, lo.x) && near(p.z, lo.z))),
        GeoRef::Edge(edge_where(&st, 0, |p| near(p.x, hi.x) && near(p.z, lo.z))),
    ];
    st.document
        .update_dimension(d, w, |m| m.refs = refs)
        .unwrap();
    let r = ui.resolve(&st, d);
    assert_eq!(ui.resolutions(), 3);
    assert!(near(value(&r, w), BOX.x));
    let refs = height_refs(&st, 0);
    st.document
        .update_dimension(d, w, |m| {
            m.kind = DimKind::Vertical;
            m.refs = refs;
        })
        .unwrap();
    assert!(near(value(&ui.resolve(&st, d), w), BOX.z));
    assert_eq!(ui.resolutions(), 4);
}

// (k)
#[test]
fn drag_edits_do_not_trigger_view_recompute() {
    let (mut st, _, d, front) = box_drawing();
    let w = add_width(&mut st, d, front, 10.0);
    open_and_compute(&mut st, d);
    let s0 = st.drawing_ui.spawned();
    let mut ui = DimensionUi::default();
    let before = ui.resolve(&st, d);
    let model = st.document.model_edit_count();
    st.document
        .update_dimension(d, w, |m| {
            m.offset = 24.0;
            m.text_shift = (3.0, -1.0);
        })
        .unwrap();
    assert_eq!(st.document.model_edit_count(), model);
    compute(&mut st);
    assert_eq!(st.drawing_ui.spawned(), s0, "Ansichten bleiben");
    let after = ui.resolve(&st, d);
    let (s, res) = entry(&after, w);
    assert_eq!(s.offset, 24.0, "neuer Abstand");
    assert_eq!(s.text_shift, (3.0, -1.0));
    assert_eq!(
        res.unwrap(),
        entry(&before, w).1.unwrap(),
        "gleiche Messung"
    );
    assert_eq!(ui.resolutions(), 1, "Ziehen misst nicht neu");
    // Die Marken folgen: Maßlinie jetzt 24 mm über der Oberkante (Blatt y 115 + 24).
    let marks = sheet_marks(&st, d, front, &after);
    assert!(
        marks
            .iter()
            .any(|m| matches!(m, Mark::Line { a, b, .. } if near(a.y, 139.0) && near(b.y, 139.0))),
        "{marks:?}"
    );
}

// (l)
#[test]
fn drawing_area_uses_the_sheet_with_marks() {
    let (mut st, _, d, front) = box_drawing();
    add_width(&mut st, d, front, 10.0);
    open_and_compute(&mut st, d);
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
        time: Some(0.1),
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| drawing_view::show(ui, &mut st));
    out.drop_without_applying_deltas();
    let sheet = drawing_view::last_sheet(&ctx).expect("kein Blatt gezeichnet");
    let view = sheet.views.iter().find(|v| v.id == front).unwrap();
    // Vorderansicht 10 × 30 in (100, 100): Oberkante y 115, Maßlinie 10 mm darüber, Pfeile
    // innen an x 95 und 105. Der Text „10,0“ (8,4 mm breit) passt nicht zwischen die Pfeile
    // (Regel aus Aufgabe 4: erst ab etwa 16 mm): er steht rechts, Anfang 5,5 mm hinter der
    // rechten Hilfslinie (x 110,5), 1 mm über der Linie.
    let dims: Vec<&Mark> = view
        .marks
        .iter()
        .filter(|m| kind_of(m) == MarkKind::Dimension)
        .collect();
    assert!(
        dims.iter().any(|m| matches!(m, Mark::Line { a, b, .. }
            if near(a.y, 125.0) && near(b.y, 125.0)
            && near(a.x.min(b.x), 95.0) && near(a.x.max(b.x), 105.0))),
        "{dims:?}"
    );
    assert!(
        dims.iter()
            .any(|m| matches!(m, Mark::Text { pos, text, anchor, .. }
        if text == "10,0" && *anchor == TextAnchor::Start
        && near(pos.x, 110.5) && near(pos.y, 126.0))),
        "{dims:?}"
    );
    // Dasselbe Blatt wie mit den aufgelösten Maßen gebaut.
    let r = resolve_in(&mut st, d);
    let expected = sheet_marks(&st, d, front, &r);
    let got: Vec<Mark> = view
        .marks
        .iter()
        .filter(|m| kind_of(m) != MarkKind::CenterLine)
        .cloned()
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(got, expected);
}
