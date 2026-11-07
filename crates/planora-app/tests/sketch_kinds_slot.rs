//! Skizzen 3, Task 10: die fünf Langloch-Arten auf der Stufenmaschine und der Falt-Wächter für
//! Längenmaße. Ohne Oberfläche über `click`/`commit_input`, eine Fahrt über Rohereignisse bei
//! 1920×1080 (`PlanoraApp::for_tests`) und das Extrudieren über `AppState`.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{ConstraintAction, Item, SketchSession, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{FeatureId, ParamEdit, ParamScope, ParamUnit, SketchFeature, SketchPlane};
use planora_i18n::t;
use planora_math::{DVec2, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::shapes::{ArcPlan, ArcSlotIds, ArcSlotPlan, ShapePlan, SlotIds, SlotPlan};
use planora_sketch::solver::{SolveReport, SolveStatus};
use std::f64::consts::{FRAC_PI_2, SQRT_2, TAU};

const R: f64 = 0.5;
const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
const STRAIGHT: [ToolKind; 3] = [
    ToolKind::SlotCenterToCenter,
    ToolKind::SlotOverall,
    ToolKind::SlotCenterPoint,
];
const ARCS: [ToolKind; 2] = [ToolKind::SlotThreePointArc, ToolKind::SlotCenterPointArc];
const KINDS: [ToolKind; 5] = [
    ToolKind::SlotCenterToCenter,
    ToolKind::SlotOverall,
    ToolKind::SlotCenterPoint,
    ToolKind::SlotThreePointArc,
    ToolKind::SlotCenterPointArc,
];

// ---- Hilfen ohne Oberfläche ---------------------------------------------------------------------

fn at(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

#[track_caller]
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} statt {b}");
}

#[track_caller]
fn near_v(a: DVec2, b: DVec2) {
    assert!(a.distance(b) < 1e-6, "{a:?} statt {b:?}");
}

/// Lage in einem um `angle` gedrehten Rahmen, weg vom Ursprung (der fängt sonst).
fn place(angle: f64, x: f64, y: f64) -> DVec2 {
    at(5.0, -3.0) + DVec2::from_angle(angle).rotate(at(x, y))
}

fn session(kind: ToolKind) -> (SketchSession, Sketch) {
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let mut se = SketchSession::new(FeatureId(0));
    se.choose_kind(&mut sk, kind);
    (se, sk)
}

fn click_at(se: &mut SketchSession, sk: &mut Sketch, p: DVec2) {
    se.click(sk, p, R, false).unwrap();
}

/// Die Klicks einer Art im Rahmen: gerade mit den Werten aus Task 1, Bögen mit R = 50.
fn clicks(kind: ToolKind, angle: f64) -> Vec<DVec2> {
    let pts: &[(f64, f64)] = match kind {
        ToolKind::SlotCenterToCenter | ToolKind::SlotOverall => {
            &[(0.0, 0.0), (40.0, 0.0), (20.0, 6.0)]
        }
        ToolKind::SlotCenterPoint => &[(0.0, 0.0), (20.0, 0.0), (5.0, 6.0)],
        // Bogen um (0,50) von (0,0) gegen den Uhrzeigersinn über (50,50) bis (0,100).
        ToolKind::SlotThreePointArc => &[(0.0, 0.0), (0.0, 100.0), (50.0, 50.0), (55.0, 50.0)],
        ToolKind::SlotCenterPointArc => &[(0.0, 0.0), (50.0, 0.0), (0.0, 50.0), (31.0, 31.0)],
        _ => unreachable!(),
    };
    pts.iter().map(|&(x, y)| place(angle, x, y)).collect()
}

fn draw(se: &mut SketchSession, sk: &mut Sketch, kind: ToolKind, angle: f64) {
    for p in clicks(kind, angle) {
        click_at(se, sk, p);
    }
}

fn bits(sk: &Sketch) -> (u64, Vec<u64>) {
    (
        sk.revision(),
        sk.variables().iter().map(|v| v.to_bits()).collect(),
    )
}

#[track_caller]
fn assert_solved(sk: &Sketch) -> SolveReport {
    let r = sk.clone().solve();
    assert_eq!(r.status, SolveStatus::Solved);
    assert!(r.redundant.is_empty(), "{:?}", r.redundant);
    r
}

fn count(sk: &Sketch, pick: impl Fn(&K) -> bool) -> usize {
    sk.constraints().filter(|(_, c)| pick(&c.kind)).count()
}

fn xy(sk: &Sketch, p: PointId) -> DVec2 {
    sk.point_xy(p).unwrap()
}

fn arc_ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2, DVec2) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Arc { center, start, end } => (xy(sk, center), xy(sk, start), xy(sk, end)),
        _ => panic!("kein Bogen"),
    }
}

fn line_ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (xy(sk, a), xy(sk, b)),
        _ => panic!("keine Linie"),
    }
}

fn curves_of(sk: &Sketch, line: bool, construction: bool) -> Vec<CurveId> {
    sk.curves()
        .filter(|(_, d)| {
            d.construction == construction && matches!(d.kind, CurveKind::Line { .. }) == line
        })
        .map(|(c, _)| c)
        .collect()
}

/// Die Ids des einzigen geraden Langlochs der Skizze, aus der Reihenfolge von `add_slot`.
fn slot_ids(sk: &Sketch) -> SlotIds {
    let axis = curves_of(sk, true, true)[0];
    let CurveKind::Line { a, b } = sk.curve(axis).unwrap().kind else {
        panic!("Achse");
    };
    let sides = curves_of(sk, true, false);
    let caps = curves_of(sk, false, false);
    let mid = sk.constraints().find_map(|(_, c)| match c.kind {
        K::Midpoint(p, l) if l == axis => Some(p),
        _ => None,
    });
    SlotIds {
        centers: [a, b],
        sides: [sides[0], sides[1]],
        caps: [caps[0], caps[1]],
        axis,
        mid,
    }
}

/// Die Ids des einzigen Bogenlanglochs der Skizze, aus der Reihenfolge von `add_arc_slot`.
fn arc_slot_ids(sk: &Sketch) -> ArcSlotIds {
    let axis = curves_of(sk, false, true)[0];
    let CurveKind::Arc { center, start, end } = sk.curve(axis).unwrap().kind else {
        panic!("Achse");
    };
    let arcs = curves_of(sk, false, false);
    ArcSlotIds {
        center,
        ends: [start, end],
        axis,
        outer: arcs[0],
        inner: arcs[1],
        caps: [arcs[2], arcs[3]],
    }
}

/// Seiten tangential und parallel zur Achse, Enden Halbkreise um die Mitten, außen herum.
#[track_caller]
fn assert_slot(sk: &Sketch, ids: &SlotIds) {
    let (a, b) = (xy(sk, ids.centers[0]), xy(sk, ids.centers[1]));
    let dir = (b - a).normalize();
    let r = sk.radius(ids.caps[0]).unwrap();
    near(sk.radius(ids.caps[1]).unwrap(), r);
    let (p1, p2) = line_ends(sk, ids.sides[0]);
    let (p3, p4) = line_ends(sk, ids.sides[1]);
    near_v(p1, a + dir.perp() * r);
    near_v(p2, b + dir.perp() * r);
    near_v(p3, b - dir.perp() * r);
    near_v(p4, a - dir.perp() * r);
    let (m0, s0, e0) = arc_ends(sk, ids.caps[0]);
    let (m1, s1, e1) = arc_ends(sk, ids.caps[1]);
    near_v(m0, a);
    near_v(m1, b);
    near_v((s0 + e0) / 2.0, a);
    near_v((s1 + e1) / 2.0, b);
    assert!((s0 - a).perp().dot(-dir) > 0.0, "E0 läuft außen");
    assert!((s1 - b).perp().dot(dir) > 0.0, "E1 läuft außen");
}

/// Außen R+h, innen R−h, alles konzentrisch, Enden Halbkreise um die Enden der Achse, die Stöße
/// auf dem Strahl durch die Mitte (Tangenten ohne Knick).
#[track_caller]
fn assert_arc_slot(sk: &Sketch, ids: &ArcSlotIds, big_r: f64, h: f64) {
    let m = xy(sk, ids.center);
    near(sk.radius(ids.axis).unwrap(), big_r);
    near(sk.radius(ids.outer).unwrap(), big_r + h);
    near(sk.radius(ids.inner).unwrap(), big_r - h);
    for arc in [ids.axis, ids.outer, ids.inner] {
        near_v(arc_ends(sk, arc).0, m);
    }
    for (cap, end) in ids.caps.iter().zip(ids.ends) {
        near(sk.radius(*cap).unwrap(), h);
        let (c, s, e) = arc_ends(sk, *cap);
        near_v(c, xy(sk, end));
        near_v((s + e) / 2.0, c);
        let radial = (c - m).normalize();
        assert!(
            (s - c).normalize().perp_dot(radial).abs() < 1e-6,
            "Stoß nicht radial: {s:?}"
        );
    }
}

/// Der Umriss aus der fertigen Geometrie, um ihn gegen die Vorschau zu halten.
fn outline_of(sk: &Sketch, kind: ToolKind) -> Vec<DVec2> {
    let plan = if ARCS.contains(&kind) {
        let ids = arc_slot_ids(sk);
        let (center, start, end) = arc_ends(sk, ids.axis);
        ShapePlan::ArcSlot(ArcSlotPlan {
            axis: ArcPlan { center, start, end },
            half_width: sk.radius(ids.caps[0]).unwrap(),
        })
    } else {
        let ids = slot_ids(sk);
        ShapePlan::Slot(SlotPlan {
            a: xy(sk, ids.centers[0]),
            b: xy(sk, ids.centers[1]),
            radius: sk.radius(ids.caps[0]).unwrap(),
        })
    };
    plan.outline(96).remove(0)
}

/// Klick wird abgelehnt: keine Vorschau, nur die Meldung, Skizze und Lagen bleiben.
#[track_caller]
fn expect_refused(se: &mut SketchSession, sk: &mut Sketch, raw: DVec2, key: &str) {
    let before = bits(sk);
    let pos = se.pending_pos.clone();
    assert!(se.kind_plan(sk, raw).is_none(), "keine Vorschau");
    let text = se.click(sk, raw, R, false).unwrap_err();
    assert_eq!(text, t(key));
    assert_eq!(se.message_text().as_deref(), Some(t(key).as_str()));
    assert_eq!(bits(sk), before, "Skizze bitgleich");
    assert_eq!(se.pending_pos, pos, "die Form läuft weiter");
    assert!(se.created.is_none());
}

// ---- Hilfen mit getippten Werten ----------------------------------------------------------------

fn rig(kind: ToolKind) -> Workspace {
    let root = common::temp_root("planora-sketch-kinds-slot");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let s = &mut w.active_tab_mut().state;
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sketch).unwrap();
    w.observe_history(false);
    let (se, sf, _) = parts(&mut w);
    se.choose_kind(&mut sf.sketch, kind);
    w
}

fn parts(w: &mut Workspace) -> (&mut SketchSession, &mut SketchFeature, ParamScope) {
    let s = &mut w.active_tab_mut().state;
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    (se, sf, scope)
}

fn commit(w: &mut Workspace, p: DVec2) -> Result<(), String> {
    let (se, sf, scope) = parts(w);
    se.commit_input(sf, p, R, false, &scope)
}

fn type_in(w: &mut Workspace, kind: F, text: &str) {
    let (se, sf, scope) = parts(w);
    se.sync_input(&sf.sketch);
    let i = se
        .input
        .fields
        .iter()
        .position(|f| f.kind == kind)
        .unwrap_or_else(|| panic!("Feld {kind:?} fehlt"));
    se.type_field(i, text, &scope);
}

fn sk(w: &Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

fn dims(sk: &Sketch) -> Vec<(ConstraintId, K)> {
    sk.constraints()
        .filter(|(_, c)| c.kind.is_dimension())
        .map(|(id, c)| (id, c.kind.clone()))
        .collect()
}

fn dim_id(sk: &Sketch, pick: impl Fn(&K) -> bool) -> ConstraintId {
    dims(sk)
        .into_iter()
        .find(|(_, k)| pick(k))
        .expect("Maß fehlt")
        .0
}

fn set(w: &mut Workspace, id: ConstraintId, text: &str) -> Result<(), String> {
    let (se, sf, scope) = parts(w);
    se.set_dimension_input(sf, id, text, &scope)
}

fn straight_with_fields(kind: ToolKind, angle: f64) -> Workspace {
    let mut w = rig(kind);
    commit(&mut w, place(angle, 0.0, 0.0)).unwrap();
    type_in(&mut w, F::Length, "40");
    commit(&mut w, place(angle, 70.0, 0.0)).unwrap();
    type_in(&mut w, F::Width, "12");
    commit(&mut w, place(angle, 20.0, 30.0)).unwrap();
    w
}

fn center_arc_with_fields(angle: f64) -> Workspace {
    let mut w = rig(ToolKind::SlotCenterPointArc);
    commit(&mut w, place(angle, 0.0, 0.0)).unwrap();
    type_in(&mut w, F::Radius, "50");
    commit(&mut w, place(angle, 80.0, 0.0)).unwrap();
    near_v(parts(&mut w).0.pending_pos[1], place(angle, 50.0, 0.0));
    type_in(&mut w, F::Sweep, "90");
    commit(&mut w, place(angle, 80.0, 80.0)).unwrap();
    near_v(parts(&mut w).0.pending_pos[2], place(angle, 0.0, 50.0));
    type_in(&mut w, F::Width, "10");
    commit(&mut w, place(angle, 60.0, 60.0)).unwrap();
    w
}

// ---- Alle fünf Arten: Kurven nach Entscheidung 2, Vorschau gleich Ergebnis ------------------------

#[test]
fn every_kind_makes_the_planned_curves_and_exactly_one_face_also_slanted() {
    for kind in KINDS {
        for angle in [0.0, 0.5] {
            let (mut se, mut sk) = session(kind);
            let fresh = bits(&sk);
            let pts = clicks(kind, angle);
            let last = *pts.last().unwrap();
            for &p in &pts[..pts.len() - 1] {
                click_at(&mut se, &mut sk, p);
            }
            assert_eq!(bits(&sk), fresh, "{kind:?}: Lagen bis zum letzten Klick");
            let preview = se.kind_preview(&sk, last).expect("Vorschau");
            click_at(&mut se, &mut sk, last);

            let r = assert_solved(&sk);
            let is_arc = ARCS.contains(&kind);
            assert_eq!(r.dof, if is_arc { 6 } else { 5 }, "{kind:?}");
            assert_eq!(
                se.last_report.as_ref().unwrap().iterations,
                0,
                "{kind:?}: keine Bewegung"
            );
            assert_eq!(sk.profiles().profiles.len(), 1, "{kind:?}: eine Fläche");
            assert!(se.pending_pos.is_empty());
            assert!(dims(&sk).is_empty());
            assert_eq!(count(&sk, |k| matches!(k, K::Tangent(..))), 4);
            assert_eq!(
                count(&sk, |k| matches!(k, K::Equal(..))),
                usize::from(!is_arc),
                "{kind:?}: Equal nur beim geraden Langloch"
            );
            assert_eq!(
                count(&sk, |k| matches!(k, K::Midpoint(..))),
                usize::from(kind == ToolKind::SlotCenterPoint)
            );
            assert_eq!(sk.curves().filter(|(_, d)| d.construction).count(), 1);

            let result = outline_of(&sk, kind);
            assert_eq!(preview.len(), 1);
            assert_eq!(preview[0].len(), result.len(), "{kind:?}");
            for (a, b) in preview[0].iter().zip(&result) {
                assert!(a.distance(*b) < 1e-6, "{kind:?} {angle}: {a:?} {b:?}");
            }
        }
    }
}

#[test]
fn clicks_of_the_task_one_values_give_their_centers() {
    let (mut se, mut sk) = session(ToolKind::SlotCenterToCenter);
    draw(&mut se, &mut sk, ToolKind::SlotCenterToCenter, 0.0);
    let ids = slot_ids(&sk);
    near_v(xy(&sk, ids.centers[0]), place(0.0, 0.0, 0.0));
    near_v(xy(&sk, ids.centers[1]), place(0.0, 40.0, 0.0));
    near(sk.radius(ids.caps[0]).unwrap(), 6.0);
    assert_eq!(ids.mid, None);
    assert_slot(&sk, &ids);

    let (mut se, mut sk) = session(ToolKind::SlotOverall);
    draw(&mut se, &mut sk, ToolKind::SlotOverall, 0.0);
    let ids = slot_ids(&sk);
    near_v(xy(&sk, ids.centers[0]), place(0.0, 6.0, 0.0));
    near_v(xy(&sk, ids.centers[1]), place(0.0, 34.0, 0.0));
    assert_eq!(ids.mid, None);
    assert_slot(&sk, &ids);

    let (mut se, mut sk) = session(ToolKind::SlotCenterPoint);
    draw(&mut se, &mut sk, ToolKind::SlotCenterPoint, 0.0);
    let ids = slot_ids(&sk);
    near_v(xy(&sk, ids.centers[0]), place(0.0, -20.0, 0.0));
    near_v(xy(&sk, ids.centers[1]), place(0.0, 20.0, 0.0));
    near_v(xy(&sk, ids.mid.unwrap()), place(0.0, 0.0, 0.0));
    assert_slot(&sk, &ids);

    for (kind, h) in [
        (ToolKind::SlotThreePointArc, 5.0),
        (ToolKind::SlotCenterPointArc, 50.0 - 31.0 * SQRT_2),
    ] {
        let (mut se, mut sk) = session(kind);
        draw(&mut se, &mut sk, kind, 0.0);
        assert_arc_slot(&sk, &arc_slot_ids(&sk), 50.0, h);
    }
}

#[test]
fn intermediate_preview_is_a_line_then_the_axis_arc() {
    for kind in STRAIGHT {
        let (mut se, mut sk) = session(kind);
        click_at(&mut se, &mut sk, at(5.0, -3.0));
        assert_eq!(
            se.kind_preview(&sk, at(25.0, 8.0)).unwrap(),
            vec![vec![at(5.0, -3.0), at(25.0, 8.0)]]
        );
        assert!(se.kind_preview(&sk, at(5.0, -3.0)).is_none());
    }
    let (mut se, mut sk) = session(ToolKind::SlotThreePointArc);
    click_at(&mut se, &mut sk, at(5.0, -3.0));
    assert_eq!(
        se.kind_preview(&sk, at(5.0, 97.0)).unwrap(),
        vec![vec![at(5.0, -3.0), at(5.0, 97.0)]]
    );
    click_at(&mut se, &mut sk, at(5.0, 97.0));
    let axis = &se.kind_preview(&sk, at(55.0, 47.0)).unwrap()[0];
    near_v(axis[0], at(5.0, -3.0));
    near_v(*axis.last().unwrap(), at(5.0, 97.0));
    for p in axis {
        near(p.distance(at(5.0, 47.0)), 50.0);
    }

    let (mut se, mut sk) = session(ToolKind::SlotCenterPointArc);
    click_at(&mut se, &mut sk, at(5.0, -3.0));
    assert_eq!(
        se.kind_preview(&sk, at(55.0, -3.0)).unwrap(),
        vec![vec![at(5.0, -3.0), at(55.0, -3.0)]]
    );
    click_at(&mut se, &mut sk, at(55.0, -3.0));
    let axis = &se.kind_preview(&sk, at(5.0, 70.0)).unwrap()[0];
    near_v(axis[0], at(55.0, -3.0));
    near_v(*axis.last().unwrap(), at(5.0, 47.0));
}

#[test]
fn a_slot_extrudes() {
    for kind in KINDS {
        let mut s = AppState::new();
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        let (se, sketch) = s.session_sketch_mut().unwrap();
        se.choose_kind(sketch, kind);
        draw(se, sketch, kind, 0.5);
        s.close_sketch();
        s.extrude_sketch(id, 0, 3.0)
            .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
    }
}

#[test]
fn no_kind_is_pending_any_more_and_the_key_is_gone() {
    for src in [
        include_str!("../../planora-i18n/languages/de.ftl"),
        include_str!("../../planora-i18n/languages/en.ftl"),
        include_str!("../src/sketch_kinds.rs"),
    ] {
        assert!(!src.contains("sketch-kind-pending"));
    }
    for key in ["sketch-slot-too-wide", "sketch-dimension-folds"] {
        for ftl in [
            include_str!("../../planora-i18n/languages/de.ftl"),
            include_str!("../../planora-i18n/languages/en.ftl"),
        ] {
            let line = format!("{key} =");
            assert!(ftl.lines().any(|l| l.starts_with(&line)), "{key}");
        }
    }
}

// ---- Felder ---------------------------------------------------------------------------------------

#[test]
fn typed_length_40_and_width_12_become_exact_dimensions() {
    for angle in [0.0, 0.5] {
        // Mitte zu Mitte; die Länge wartet bis zum Abschluss.
        let mut w = rig(ToolKind::SlotCenterToCenter);
        commit(&mut w, place(angle, 0.0, 0.0)).unwrap();
        type_in(&mut w, F::Length, "40");
        commit(&mut w, place(angle, 70.0, 0.0)).unwrap();
        near_v(parts(&mut w).0.pending_pos[1], place(angle, 40.0, 0.0));
        assert!(dims(sk(&w)).is_empty());
        let w = straight_with_fields(ToolKind::SlotCenterToCenter, angle);
        let ds = dims(sk(&w));
        assert_eq!(ds.len(), 2);
        assert!(
            matches!(ds[0].1, K::Distance(_, _, v) if v == 40.0),
            "{ds:?}"
        );
        assert!(matches!(ds[1].1, K::Diameter(_, v) if v == 12.0), "{ds:?}");
        assert_eq!(assert_solved(sk(&w)).dof, 3);

        // Gesamt: Mitten 28, Enden außen 40 auseinander. Skizzen 5 (Entscheidung 12): Die
        // Gesamtlänge ist ein eigenes Maß `SlotOverall` 40 statt des Mittenabstands 28.
        let w = straight_with_fields(ToolKind::SlotOverall, angle);
        let ds = dims(sk(&w));
        assert_eq!(ds.len(), 2);
        assert!(
            matches!(ds[0].1, K::SlotOverall(_, _, _, v) if v == 40.0),
            "{ds:?}"
        );
        assert!(matches!(ds[1].1, K::Diameter(_, v) if v == 12.0), "{ds:?}");
        assert_eq!(assert_solved(sk(&w)).dof, 3);
        let ids = slot_ids(sk(&w));
        let (a, b) = (xy(sk(&w), ids.centers[0]), xy(sk(&w), ids.centers[1]));
        near(a.distance(b), 28.0);
        near_v(a, place(angle, 6.0, 0.0));
        near_v(b, place(angle, 34.0, 0.0));
        let dir = (b - a).normalize();
        near((b + dir * 6.0).distance(a - dir * 6.0), 40.0);
        assert_slot(sk(&w), &ids);

        // Mittelpunkt: Länge ist Mitte zu Mitte, der Zeiger gibt die Hälfte.
        let w = straight_with_fields(ToolKind::SlotCenterPoint, angle);
        let ds = dims(sk(&w));
        assert!(
            matches!(ds[0].1, K::Distance(_, _, v) if v == 40.0),
            "{ds:?}"
        );
        assert!(matches!(ds[1].1, K::Diameter(_, v) if v == 12.0), "{ds:?}");
        assert_eq!(assert_solved(sk(&w)).dof, 3);
        let ids = slot_ids(sk(&w));
        let (a, b) = (xy(sk(&w), ids.centers[0]), xy(sk(&w), ids.centers[1]));
        near(a.distance(b), 40.0);
        near_v(xy(sk(&w), ids.mid.unwrap()), (a + b) / 2.0);
        near_v(xy(sk(&w), ids.mid.unwrap()), place(angle, 0.0, 0.0));
    }
}

#[test]
fn overall_length_uses_the_pointer_width_when_only_the_length_is_typed() {
    // Skizzen 5: Die getippte Gesamtlänge wird das Maß `SlotOverall` 40 (vorher Mittenabstand
    // 28); die Breite vom Zeiger gibt nur die Lage der Mitten.
    let mut w = rig(ToolKind::SlotOverall);
    commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
    type_in(&mut w, F::Length, "40");
    commit(&mut w, place(0.0, 70.0, 0.0)).unwrap();
    commit(&mut w, place(0.0, 20.0, 6.0)).unwrap();
    let ds = dims(sk(&w));
    assert_eq!(ds.len(), 1);
    assert!(
        matches!(ds[0].1, K::SlotOverall(_, _, _, v) if v == 40.0),
        "{ds:?}"
    );
    let ids = slot_ids(sk(&w));
    near(
        xy(sk(&w), ids.centers[0]).distance(xy(sk(&w), ids.centers[1])),
        28.0,
    );
    assert_solved(sk(&w));

    // Nur die Breite: kein Längenmaß, die Enden bleiben.
    let mut w = rig(ToolKind::SlotOverall);
    commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
    commit(&mut w, place(0.0, 40.0, 0.0)).unwrap();
    type_in(&mut w, F::Width, "12");
    commit(&mut w, place(0.0, 20.0, 30.0)).unwrap();
    let ds = dims(sk(&w));
    assert_eq!(ds.len(), 1);
    assert!(matches!(ds[0].1, K::Diameter(_, v) if v == 12.0), "{ds:?}");
    let ids = slot_ids(sk(&w));
    near_v(xy(sk(&w), ids.centers[0]), place(0.0, 6.0, 0.0));
    near_v(xy(sk(&w), ids.centers[1]), place(0.0, 34.0, 0.0));
}

#[test]
fn center_point_arc_takes_radius_sweep_and_width() {
    for angle in [0.0, 0.5] {
        let w = center_arc_with_fields(angle);
        let ids = arc_slot_ids(sk(&w));
        let ds = dims(sk(&w));
        assert_eq!(ds.len(), 3, "{ds:?}");
        assert!(ds.iter().any(|(_, k)| *k == K::Radius(ids.axis, 50.0)));
        assert!(ds.iter().any(|(_, k)| *k == K::Diameter(ids.caps[0], 10.0)));
        assert!(
            ds.iter()
                .any(|(_, k)| matches!(k, K::Angle(_, _, v) if (v - FRAC_PI_2).abs() < 1e-9))
        );
        assert_solved(sk(&w));
        assert_arc_slot(sk(&w), &ids, 50.0, 5.0);
        near_v(xy(sk(&w), ids.ends[1]), place(angle, 0.0, 50.0));
        // Die beiden Hilfslinien des Öffnungswinkels sind Konstruktion, also genau eine Fläche.
        assert_eq!(sk(&w).profiles().profiles.len(), 1);
    }
}

#[test]
fn a_width_typed_with_the_pointer_inside_is_the_same_width() {
    let mut w = rig(ToolKind::SlotCenterPointArc);
    commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
    commit(&mut w, place(0.0, 50.0, 0.0)).unwrap();
    commit(&mut w, place(0.0, 0.0, 50.0)).unwrap();
    type_in(&mut w, F::Width, "10");
    commit(&mut w, place(0.0, 20.0, 20.0)).unwrap();
    assert_arc_slot(sk(&w), &arc_slot_ids(sk(&w)), 50.0, 5.0);
}

#[test]
fn three_point_arc_takes_radius_or_sweep_but_not_both() {
    // Radius: Sehne 100 ⇒ bei R = 50 ein Halbkreis auf der Seite des Zeigers.
    let mut w = rig(ToolKind::SlotThreePointArc);
    commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
    commit(&mut w, place(0.0, 0.0, 100.0)).unwrap();
    type_in(&mut w, F::Radius, "50");
    commit(&mut w, place(0.0, 30.0, 50.0)).unwrap();
    type_in(&mut w, F::Width, "10");
    commit(&mut w, place(0.0, 60.0, 50.0)).unwrap();
    let ids = arc_slot_ids(sk(&w));
    assert!(
        dims(sk(&w))
            .iter()
            .any(|(_, k)| *k == K::Radius(ids.axis, 50.0))
    );
    assert_solved(sk(&w));
    assert_arc_slot(sk(&w), &ids, 50.0, 5.0);
    near_v(xy(sk(&w), ids.center), place(0.0, 0.0, 50.0));

    // Öffnungswinkel: Sehne 100, 90° ⇒ R = 100/√2.
    let mut w = rig(ToolKind::SlotThreePointArc);
    commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
    commit(&mut w, place(0.0, 0.0, 100.0)).unwrap();
    type_in(&mut w, F::Sweep, "90");
    commit(&mut w, place(0.0, 30.0, 50.0)).unwrap();
    commit(&mut w, place(0.0, 40.0, 50.0)).unwrap();
    let ids = arc_slot_ids(sk(&w));
    near(sk(&w).radius(ids.axis).unwrap(), 100.0 / SQRT_2);
    assert!(
        dims(sk(&w))
            .iter()
            .any(|(_, k)| matches!(k, K::Angle(_, _, v) if (v - FRAC_PI_2).abs() < 1e-9))
    );
    assert_solved(sk(&w));
    assert_eq!(sk(&w).profiles().profiles.len(), 1);

    // Beides: das zweite Feld nennt die Regel, der Klick lehnt ab, egal in welcher Reihenfolge.
    for (first, second) in [(F::Radius, F::Sweep), (F::Sweep, F::Radius)] {
        let mut w = rig(ToolKind::SlotThreePointArc);
        commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
        commit(&mut w, place(0.0, 0.0, 100.0)).unwrap();
        type_in(&mut w, first, if first == F::Radius { "50" } else { "90" });
        type_in(
            &mut w,
            second,
            if second == F::Radius { "50" } else { "90" },
        );
        let (se, _, _) = parts(&mut w);
        let field = se.input.fields.iter().find(|f| f.kind == second).unwrap();
        assert_eq!(
            field.error.as_deref(),
            Some(t("sketch-field-radius-or-sweep").as_str())
        );
        assert_eq!(field.locked, None);
        let before = bits(sk(&w));
        let text = commit(&mut w, place(0.0, 30.0, 50.0)).unwrap_err();
        assert_eq!(text, t("sketch-field-radius-or-sweep"));
        assert_eq!(bits(sk(&w)), before);
        assert_eq!(parts(&mut w).0.pending_pos.len(), 2);
        // Das zweite Feld wieder leeren gibt den Klick frei.
        type_in(&mut w, second, "");
        commit(&mut w, place(0.0, 30.0, 50.0)).unwrap();
        assert_eq!(parts(&mut w).0.pending_pos.len(), 3);
    }
}

#[test]
fn field_values_follow_the_pointer_before_the_first_key() {
    let mut w = rig(ToolKind::SlotCenterPoint);
    commit(&mut w, at(5.0, -3.0)).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(25.0, -3.0)), R);
    near(se.field_value(&sf.sketch, F::Length).unwrap(), 40.0);
    commit(&mut w, at(25.0, -3.0)).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(10.0, 3.0)), R);
    near(se.field_value(&sf.sketch, F::Width).unwrap(), 12.0);

    let mut w = rig(ToolKind::SlotCenterPointArc);
    commit(&mut w, at(5.0, -3.0)).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(55.0, -3.0)), R);
    near(se.field_value(&sf.sketch, F::Radius).unwrap(), 50.0);
    commit(&mut w, at(55.0, -3.0)).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(5.0, 47.0)), R);
    near(se.field_value(&sf.sketch, F::Sweep).unwrap(), FRAC_PI_2);
    commit(&mut w, at(5.0, 47.0)).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(5.0 + 33.0, -3.0 + 44.0)), R);
    near(se.field_value(&sf.sketch, F::Width).unwrap(), 10.0);
}

// ---- Maß ändern hält die Form --------------------------------------------------------------------

#[test]
fn changing_width_and_length_in_the_app_keeps_the_straight_shape() {
    for kind in STRAIGHT {
        for angle in [0.0, 0.5] {
            let mut w = straight_with_fields(kind, angle);
            let ids = slot_ids(sk(&w));
            let width = dim_id(sk(&w), |k| matches!(k, K::Diameter(..)));
            // Skizzen 5: „Gesamt“ hält die Gesamtlänge (`SlotOverall`), die anderen den
            // Mittenabstand; die Werte für „Gesamt“ bleiben über der Breite.
            let overall = kind == ToolKind::SlotOverall;
            let length = dim_id(sk(&w), |k| {
                matches!(k, K::Distance(..) | K::SlotOverall(..))
            });
            let steps: &[(ConstraintId, &str)] = if overall {
                &[
                    (width, "30"),
                    (length, "100"),
                    (width, "2"),
                    (length, "5"),
                    (width, "4"),
                    (length, "40"),
                    (width, "12"),
                ]
            } else {
                &[
                    (width, "30"),
                    (length, "100"),
                    (width, "2"),
                    (length, "5"),
                    (width, "12"),
                    (length, "40"),
                ]
            };
            for &(id, text) in steps {
                set(&mut w, id, text).unwrap_or_else(|e| panic!("{kind:?} {text}: {e}"));
                assert_solved(sk(&w));
                assert_slot(sk(&w), &ids);
                let v: f64 = text.parse().unwrap();
                let (a, b) = (xy(sk(&w), ids.centers[0]), xy(sk(&w), ids.centers[1]));
                let r = sk(&w).radius(ids.caps[0]).unwrap();
                if id == width {
                    near(r, v / 2.0);
                } else if overall {
                    near(a.distance(b) + 2.0 * r, v);
                } else {
                    near(a.distance(b), v);
                }
                if let Some(mid) = ids.mid {
                    let (a, b) = (xy(sk(&w), ids.centers[0]), xy(sk(&w), ids.centers[1]));
                    near_v(xy(sk(&w), mid), (a + b) / 2.0);
                }
                assert_eq!(sk(&w).profiles().profiles.len(), 1, "{kind:?} {text}");
            }
        }
    }
}

#[test]
fn changing_radius_sweep_and_width_in_the_app_keeps_the_arc_shape() {
    for angle in [0.0, 0.5] {
        let mut w = center_arc_with_fields(angle);
        let ids = arc_slot_ids(sk(&w));
        let width = dim_id(sk(&w), |k| matches!(k, K::Diameter(..)));
        let radius = dim_id(sk(&w), |k| matches!(k, K::Radius(..)));
        let sweep = dim_id(sk(&w), |k| matches!(k, K::Angle(..)));
        let (mut r, mut h, mut s) = (50.0, 5.0, 90.0_f64);
        for (id, value) in [
            (width, 20.0),
            (radius, 120.0),
            (width, 3.0),
            (radius, 30.0),
            (sweep, 60.0),
            (sweep, 120.0),
            (sweep, 150.0),
            (width, 10.0),
            (radius, 50.0),
            (sweep, 90.0),
        ] {
            set(&mut w, id, &value.to_string()).unwrap_or_else(|e| panic!("{value}: {e}"));
            if id == width {
                h = value / 2.0;
            } else if id == radius {
                r = value;
            } else {
                s = value;
            }
            assert_solved(sk(&w));
            assert_arc_slot(sk(&w), &ids, r, h);
            let (m, start, end) = arc_ends(sk(&w), ids.axis);
            let open = ((end - m).to_angle() - (start - m).to_angle()).rem_euclid(TAU);
            near(open, s.to_radians());
            assert_eq!(sk(&w).profiles().profiles.len(), 1, "{value}");
        }
    }
}

// ---- Review-Focus 1: unmögliche Eingaben -----------------------------------------------------------

#[test]
fn the_same_place_twice_says_so_and_the_shape_goes_on() {
    for (kinds, key) in [
        (&STRAIGHT[..], "sketch-err-line-zero"),
        (&ARCS[..], "sketch-arc-end-on-start"),
    ] {
        for &kind in kinds {
            let (mut se, mut sk) = session(kind);
            click_at(&mut se, &mut sk, at(5.0, -3.0));
            assert!(se.kind_preview(&sk, at(5.0, -3.0)).is_none(), "{kind:?}");
            let before = bits(&sk);
            let text = se.click(&mut sk, at(5.0, -3.0), R, false).unwrap_err();
            assert_eq!(text, t(key), "{kind:?}");
            assert_eq!(bits(&sk), before);
            assert_eq!(se.pending_pos.len(), 1);
        }
    }
}

#[test]
fn pointer_on_the_axis_or_too_wide_is_refused_and_the_shape_goes_on() {
    for kind in STRAIGHT {
        let (mut se, mut sk) = session(kind);
        let pts = clicks(kind, 0.0);
        click_at(&mut se, &mut sk, pts[0]);
        click_at(&mut se, &mut sk, pts[1]);
        expect_refused(
            &mut se,
            &mut sk,
            place(0.0, 7.0, 0.0),
            "sketch-slot-too-wide",
        );
        click_at(&mut se, &mut sk, pts[2]);
        assert_eq!(sk.profiles().profiles.len(), 1, "{kind:?}");
    }
    // Gesamt: Breite gleich und größer als die Länge.
    {
        let (mut se, mut sk) = session(ToolKind::SlotOverall);
        click_at(&mut se, &mut sk, place(0.0, 0.0, 0.0));
        click_at(&mut se, &mut sk, place(0.0, 40.0, 0.0));
        for y in [20.0, 30.0] {
            expect_refused(
                &mut se,
                &mut sk,
                place(0.0, 20.0, y),
                "sketch-slot-too-wide",
            );
        }
    }

    // Getippt: Breite gleich der Gesamtlänge und darüber.
    for width in ["40", "50"] {
        let mut w = rig(ToolKind::SlotOverall);
        commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
        type_in(&mut w, F::Length, "40");
        commit(&mut w, place(0.0, 70.0, 0.0)).unwrap();
        type_in(&mut w, F::Width, width);
        let before = bits(sk(&w));
        let (se, sf, _) = parts(&mut w);
        let cur = se.resolve_cursor(&sf.sketch, place(0.0, 20.0, 30.0), R);
        assert!(se.kind_plan(&sf.sketch, cur.at).is_none(), "keine Vorschau");
        let text = commit(&mut w, place(0.0, 20.0, 30.0)).unwrap_err();
        assert_eq!(text, t("sketch-slot-too-wide"));
        assert_eq!(bits(sk(&w)), before);
        assert_eq!(parts(&mut w).0.pending_pos.len(), 2);
        assert_eq!(
            parts(&mut w).0.message_text().as_deref(),
            Some(t("sketch-slot-too-wide").as_str())
        );
    }
}

#[test]
fn arc_slot_width_from_twice_the_radius_is_refused() {
    for kind in ARCS {
        for width in ["100", "110", "200"] {
            let mut w = rig(kind);
            let pts = clicks(kind, 0.0);
            for &p in &pts[..3] {
                commit(&mut w, p).unwrap();
            }
            type_in(&mut w, F::Width, width);
            let before = bits(sk(&w));
            // Auch der Zeiger innen macht aus zu breit keine schmalere Form.
            for pointer in [place(0.0, 60.0, 60.0), place(0.0, 3.0, 3.0)] {
                let text = commit(&mut w, pointer).unwrap_err();
                assert_eq!(text, t("sketch-slot-too-wide"), "{kind:?} {width}");
                assert_eq!(bits(sk(&w)), before);
                assert_eq!(parts(&mut w).0.pending_pos.len(), 3);
            }
        }
    }
    // Vom Zeiger: h ≥ R.
    let (mut se, mut sk) = session(ToolKind::SlotCenterPointArc);
    for &p in &clicks(ToolKind::SlotCenterPointArc, 0.0)[..3] {
        click_at(&mut se, &mut sk, p);
    }
    expect_refused(
        &mut se,
        &mut sk,
        place(0.0, 110.0, 0.0),
        "sketch-slot-too-wide",
    );
}

#[test]
fn a_sweep_of_350_degrees_with_a_large_width_is_refused() {
    let mut w = rig(ToolKind::SlotCenterPointArc);
    commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
    type_in(&mut w, F::Radius, "50");
    commit(&mut w, place(0.0, 80.0, 0.0)).unwrap();
    type_in(&mut w, F::Sweep, "350");
    commit(&mut w, place(0.0, 30.0, -30.0)).unwrap();
    type_in(&mut w, F::Width, "20");
    let before = bits(sk(&w));
    let text = commit(&mut w, place(0.0, -60.0, 0.0)).unwrap_err();
    assert_eq!(text, t("sketch-slot-too-wide"));
    assert_eq!(bits(sk(&w)), before);
    // Schmal genug passt es: Lücke 2·50·sin(5°) = 8,7.
    type_in(&mut w, F::Width, "8");
    commit(&mut w, place(0.0, -60.0, 0.0)).unwrap();
    assert_solved(sk(&w));
}

#[test]
fn three_collinear_points_and_a_short_radius_are_refused_at_the_axis_stage() {
    let (mut se, mut sk) = session(ToolKind::SlotThreePointArc);
    click_at(&mut se, &mut sk, at(5.0, -3.0));
    click_at(&mut se, &mut sk, at(5.0, 97.0));
    expect_refused(&mut se, &mut sk, at(5.0, 47.0), "sketch-circle-collinear");
    assert_eq!(se.kind_stage(), 2);
    assert!(se.kind_preview(&sk, at(5.0, 47.0)).is_none());

    // Radius kleiner als die halbe Sehne.
    let mut w = rig(ToolKind::SlotThreePointArc);
    commit(&mut w, at(5.0, -3.0)).unwrap();
    commit(&mut w, at(5.0, 97.0)).unwrap();
    type_in(&mut w, F::Radius, "20");
    let text = commit(&mut w, at(40.0, 47.0)).unwrap_err();
    assert_eq!(text, t("sketch-arc-radius-chord"));
    assert_eq!(parts(&mut w).0.pending_pos.len(), 2);

    // Mittelpunkt-Bogen: Ein Ende auf dem Strahl des Anfangs hat keinen Öffnungswinkel.
    let (mut se, mut sk) = session(ToolKind::SlotCenterPointArc);
    click_at(&mut se, &mut sk, at(5.0, -3.0));
    click_at(&mut se, &mut sk, at(55.0, -3.0));
    expect_refused(&mut se, &mut sk, at(95.0, -3.0), "sketch-arc-end-on-start");
}

#[test]
fn overall_length_below_the_width_by_dimension_is_refused_and_the_state_stays() {
    // Skizzen 5 (Entscheidung 12): Die Gesamtlänge ist das eigene Maß `SlotOverall` 40 bei
    // Breite 12. Gleich der Breite, darunter, null und negativ wird abgelehnt.
    let mut w = straight_with_fields(ToolKind::SlotOverall, 0.0);
    let length = dim_id(sk(&w), |k| matches!(k, K::SlotOverall(..)));
    let before = bits(sk(&w));
    for text in ["12", "8", "0", "-8"] {
        assert!(set(&mut w, length, text).is_err(), "{text}");
        assert_eq!(bits(sk(&w)), before, "{text}");
    }
    assert!(matches!(
        sk(&w).constraint(length).unwrap().kind,
        K::SlotOverall(_, _, _, v) if v == 40.0
    ));
}

// ---- Review-Focus 2: Fang ---------------------------------------------------------------------------

/// Setzt den Durchmesser am Ende `cap` (Ausgang `from`) über die App auf `to`.
fn change_width(se: &mut SketchSession, sk: &mut Sketch, cap: CurveId, from: f64, to: f64) {
    let width = sk.add_constraint(K::Diameter(cap, from)).unwrap();
    sk.solve();
    se.set_dimension_value(sk, width, to).unwrap();
    assert_solved(sk);
}

#[test]
fn ends_on_existing_points_are_coincident_and_a_width_change_keeps_them() {
    let (mut se, mut sk) = session(ToolKind::SlotCenterToCenter);
    let p = sk.add_point(5.0, -3.0).unwrap();
    let q = sk.add_point(45.0, -3.0).unwrap();
    click_at(&mut se, &mut sk, at(5.1, -2.9));
    click_at(&mut se, &mut sk, at(44.9, -3.1));
    click_at(&mut se, &mut sk, at(25.0, 3.0));
    let ids = slot_ids(&sk);
    assert_eq!(count(&sk, |k| *k == K::Coincident(ids.centers[0], p)), 1);
    assert_eq!(count(&sk, |k| *k == K::Coincident(ids.centers[1], q)), 1);
    assert_solved(&sk);
    change_width(&mut se, &mut sk, ids.caps[0], 12.0, 30.0);
    assert_slot(&sk, &ids);
    near_v(xy(&sk, ids.centers[0]), xy(&sk, p));
    near_v(xy(&sk, ids.centers[1]), xy(&sk, q));
    near(sk.radius(ids.caps[1]).unwrap(), 15.0);
}

#[test]
fn center_of_a_center_point_slot_in_the_origin_is_fixed() {
    let (mut se, mut sk) = session(ToolKind::SlotCenterPoint);
    click_at(&mut se, &mut sk, at(0.1, 0.1));
    click_at(&mut se, &mut sk, at(20.0, 3.0));
    click_at(&mut se, &mut sk, at(8.0, 9.0));
    let ids = slot_ids(&sk);
    let mid = ids.mid.unwrap();
    assert_eq!(count(&sk, |k| *k == K::Fixed(mid, 0.0, 0.0)), 1);
    assert_solved(&sk);
    let width = 2.0 * sk.radius(ids.caps[0]).unwrap();
    change_width(&mut se, &mut sk, ids.caps[0], width, 4.0);
    near_v(xy(&sk, mid), at(0.0, 0.0));
    assert_slot(&sk, &ids);
}

#[test]
fn outer_ends_of_an_overall_slot_bind_on_the_caps() {
    let (mut se, mut sk) = session(ToolKind::SlotOverall);
    let p = sk.add_point(5.0, -3.0).unwrap();
    let q = sk.add_point(45.0, -3.0).unwrap();
    click_at(&mut se, &mut sk, at(5.1, -2.9));
    click_at(&mut se, &mut sk, at(44.9, -3.1));
    click_at(&mut se, &mut sk, at(25.0, 3.0));
    let ids = slot_ids(&sk);
    assert_eq!(count(&sk, |k| *k == K::PointOnCurve(p, ids.caps[0])), 1);
    assert_eq!(count(&sk, |k| *k == K::PointOnCurve(q, ids.caps[1])), 1);
    assert_eq!(count(&sk, |k| matches!(k, K::Coincident(..))), 0);
    assert_solved(&sk);
    near_v(xy(&sk, ids.centers[0]), at(11.0, -3.0));
    near_v(xy(&sk, ids.centers[1]), at(39.0, -3.0));
    change_width(&mut se, &mut sk, ids.caps[0], 12.0, 20.0);
    assert_slot(&sk, &ids);
    near(xy(&sk, ids.centers[0]).distance(xy(&sk, p)), 10.0);
    near(xy(&sk, ids.centers[1]).distance(xy(&sk, q)), 10.0);
}

#[test]
fn an_outer_end_in_the_origin_is_fixed_on_the_cap() {
    let (mut se, mut sk) = session(ToolKind::SlotOverall);
    click_at(&mut se, &mut sk, at(0.1, 0.1));
    click_at(&mut se, &mut sk, at(40.0, 3.0));
    click_at(&mut se, &mut sk, at(20.0, 9.0));
    let ids = slot_ids(&sk);
    let on_cap = |k: &K| matches!(k, K::PointOnCurve(_, c) if *c == ids.caps[0]);
    assert_eq!(
        count(
            &sk,
            |k| matches!(k, K::Fixed(_, x, y) if *x == 0.0 && *y == 0.0)
        ),
        1
    );
    assert_eq!(count(&sk, on_cap), 1);
    assert_solved(&sk);
}

#[test]
fn arc_kinds_bind_center_and_ends_and_the_width_stage_binds_nothing() {
    let (mut se, mut sk) = session(ToolKind::SlotCenterPointArc);
    let s = sk.add_point(50.0, 0.0).unwrap();
    // Der Punkt liegt auf dem Strahl der Breite, 5 mm außerhalb des Mittelbogens.
    let q = sk.add_point(33.0, 44.0).unwrap();
    click_at(&mut se, &mut sk, at(0.1, 0.1));
    click_at(&mut se, &mut sk, at(49.9, 0.1));
    click_at(&mut se, &mut sk, at(0.0, 50.0));
    click_at(&mut se, &mut sk, at(33.05, 44.05));
    let ids = arc_slot_ids(&sk);
    assert_eq!(count(&sk, |k| *k == K::Fixed(ids.center, 0.0, 0.0)), 1);
    assert_eq!(count(&sk, |k| *k == K::Coincident(ids.ends[0], s)), 1);
    assert_eq!(
        count(&sk, |k| {
            matches!(k, K::Coincident(_, x) | K::Coincident(x, _) if *x == q)
                || matches!(k, K::PointOnCurve(x, _) if *x == q)
        }),
        0
    );
    assert_solved(&sk);
    change_width(&mut se, &mut sk, ids.caps[0], 10.0, 20.0);
    near_v(xy(&sk, ids.center), at(0.0, 0.0));
    near_v(xy(&sk, ids.ends[0]), xy(&sk, s));
    assert_arc_slot(&sk, &ids, 50.0, 10.0);
}

#[test]
fn three_point_arc_ends_on_existing_points_are_coincident() {
    let (mut se, mut sk) = session(ToolKind::SlotThreePointArc);
    let p = sk.add_point(5.0, -3.0).unwrap();
    let q = sk.add_point(5.0, 97.0).unwrap();
    click_at(&mut se, &mut sk, at(5.1, -2.9));
    click_at(&mut se, &mut sk, at(4.9, 96.9));
    // Im Uhrzeigersinn: Start und Ende der Achse sind getauscht, die Bindung nicht.
    click_at(&mut se, &mut sk, at(-45.0, 47.0));
    click_at(&mut se, &mut sk, at(-50.0, 47.0));
    let ids = arc_slot_ids(&sk);
    for x in [p, q] {
        assert_eq!(
            count(
                &sk,
                |k| matches!(k, K::Coincident(a, y) if *y == x && ids.ends.contains(a))
            ),
            1
        );
    }
    assert_solved(&sk);
    assert_arc_slot(&sk, &ids, 50.0, 5.0);
}

// ---- Trimmen und Löschen ------------------------------------------------------------------------------

#[test]
fn trimming_a_side_and_deleting_an_arc_leave_no_dead_constraints() {
    let (mut se, mut sk) = session(ToolKind::SlotCenterToCenter);
    draw(&mut se, &mut sk, ToolKind::SlotCenterToCenter, 0.0);
    let curves = sk.curves().count();
    se.set_tool(&mut sk, Tool::Trim);
    click_at(&mut se, &mut sk, place(0.0, 20.0, 6.0));
    assert_eq!(sk.curves().count(), curves - 1);
    assert_eq!(sk.clone().solve().status, SolveStatus::Solved);

    let (mut se, mut sk) = session(ToolKind::SlotCenterToCenter);
    draw(&mut se, &mut sk, ToolKind::SlotCenterToCenter, 0.0);
    let ids = slot_ids(&sk);
    se.set_tool(&mut sk, Tool::Select);
    se.selection = vec![Item::Curve(ids.caps[1])];
    se.delete_selection(&mut sk);
    assert!(sk.curve(ids.caps[1]).is_none());
    assert_eq!(sk.clone().solve().status, SolveStatus::Solved);

    let (mut se, mut sk) = session(ToolKind::SlotCenterPointArc);
    draw(&mut se, &mut sk, ToolKind::SlotCenterPointArc, 0.0);
    let ids = arc_slot_ids(&sk);
    se.set_tool(&mut sk, Tool::Select);
    se.selection = vec![Item::Curve(ids.outer)];
    se.delete_selection(&mut sk);
    assert!(sk.curve(ids.outer).is_none());
    assert_eq!(sk.clone().solve().status, SolveStatus::Solved);
}

// ---- Falt-Wächter -------------------------------------------------------------------------------------

#[test]
fn a_center_arc_keeps_its_direction_when_its_8_mm_line_goes_to_100() {
    let (mut se, mut sk) = session(ToolKind::ArcCenter);
    let a = sk.add_point(2.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    let line = sk.add_line(a, b).unwrap();
    let m = sk.add_point(10.0, 5.0).unwrap();
    let e = sk.add_point(15.0, 5.0).unwrap();
    let arc = sk.add_arc(m, b, e).unwrap();
    sk.add_constraint(K::Tangent(arc, line)).unwrap();
    let id = sk.add_constraint(K::Distance(a, b, 8.0)).unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    se.set_dimension_value(&mut sk, id, 100.0).unwrap();
    assert_solved(&sk);
    let (pa, pb) = (xy(&sk, a), xy(&sk, b));
    near(pa.distance(pb), 100.0);
    let (center, start, _) = arc_ends(&sk, arc);
    near_v(start, pb);
    let leaving = (start - center).perp().normalize();
    assert!(
        leaving.dot((pb - pa).normalize()) > 1.0 - 1e-6,
        "Spitze: {leaving:?}"
    );
}

/// Bogen-Langloch mit R = 60 und Breite 20.
fn arc_slot_r60() -> (SketchSession, Sketch, ArcSlotIds) {
    let (mut se, mut sk) = session(ToolKind::SlotCenterPointArc);
    let m = at(5.0, -3.0);
    click_at(&mut se, &mut sk, m);
    click_at(&mut se, &mut sk, m + at(60.0, 0.0));
    click_at(&mut se, &mut sk, m + at(0.0, 60.0));
    click_at(&mut se, &mut sk, m + at(42.0, 56.0));
    let ids = arc_slot_ids(&sk);
    (se, sk, ids)
}

#[test]
fn an_arc_slot_width_of_130_at_radius_60_is_refused_and_the_state_is_bit_equal() {
    let (mut se, mut sk, ids) = arc_slot_r60();
    let id = sk.add_constraint(K::Diameter(ids.caps[0], 20.0)).unwrap();
    sk.add_constraint(K::Radius(ids.axis, 60.0)).unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    let before = bits(&sk);
    // Die Tangenten haben nur noch eine Lösung: Innen kein Platz mehr ist ein echter Widerspruch.
    let text = se.set_dimension_value(&mut sk, id, 130.0).unwrap_err();
    assert_eq!(text, t("sketch-change-conflict"));
    assert_eq!(se.message_text().as_deref(), Some(text.as_str()));
    assert_eq!(bits(&sk), before);
    assert!(matches!(
        sk.constraint(id).unwrap().kind,
        K::Diameter(_, v) if v == 20.0
    ));
    assert_arc_slot(&sk, &ids, 60.0, 10.0);
    // Danach geht ein gültiger Wert wie immer.
    se.set_dimension_value(&mut sk, id, 30.0).unwrap();
    assert_arc_slot(&sk, &ids, 60.0, 15.0);
}

/// Ohne Maß am Radius klappt nichts um: Die Tangenten haben nur noch eine Lösung, also wächst der
/// Radius mit, und das Langloch bleibt eine gültige Form.
#[test]
fn an_arc_slot_with_a_free_radius_grows_with_its_width() {
    let (mut se, mut sk, ids) = arc_slot_r60();
    let id = sk.add_constraint(K::Diameter(ids.caps[0], 20.0)).unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    se.set_dimension_value(&mut sk, id, 130.0).unwrap();
    assert_solved(&sk);
    let big_r = sk.radius(ids.axis).unwrap();
    assert!(big_r > 65.0, "{big_r}");
    assert_arc_slot(&sk, &ids, big_r, 65.0);
    assert_eq!(sk.profiles().profiles.len(), 1);
}

#[test]
fn a_new_dimension_past_the_limit_is_refused_too() {
    let (mut se, mut sk, ids) = arc_slot_r60();
    sk.add_constraint(K::Radius(ids.axis, 60.0)).unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    se.set_tool(&mut sk, Tool::Select);
    se.selection = vec![Item::Curve(ids.caps[0])];
    let before = bits(&sk);
    let constraints = sk.constraints().count();
    assert!(
        se.apply(&mut sk, ConstraintAction::Diameter, Some(130.0))
            .is_err()
    );
    assert_eq!(bits(&sk), before);
    assert_eq!(sk.constraints().count(), constraints);
}

/// Fest (0,0) und (30,0), dazwischen `b` auf der x-Achse: Die Linie `b`–`q` kehrt um, sobald `b`
/// über `q` hinausläuft, und das meldet der Löser als gelöst.
fn line_past_its_fixed_end() -> (SketchSession, Sketch, PointId, PointId, CurveId) {
    let (se, mut sk) = session(ToolKind::ArcCenter);
    let a = sk.add_point(0.0, 0.0).unwrap();
    let far = sk.add_point(100.0, 0.0).unwrap();
    let axis = sk.add_line(a, far).unwrap();
    sk.set_construction(axis, true).unwrap();
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::Fixed(far, 100.0, 0.0)).unwrap();
    let b = sk.add_point(20.0, 0.0).unwrap();
    sk.add_constraint(K::PointOnCurve(b, axis)).unwrap();
    let q = sk.add_point(30.0, 0.0).unwrap();
    sk.add_constraint(K::Fixed(q, 30.0, 0.0)).unwrap();
    let line = sk.add_line(b, q).unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    (se, sk, a, b, line)
}

#[test]
fn a_length_that_reverses_a_line_is_refused_in_all_steps_and_the_state_stays() {
    let (mut se, mut sk, a, b, line) = line_past_its_fixed_end();
    let id = sk.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    let before = bits(&sk);
    let text = se.set_dimension_value(&mut sk, id, 50.0).unwrap_err();
    assert_eq!(text, t("sketch-dimension-folds"));
    assert_eq!(bits(&sk), before);
    // Bis kurz vor den festen Punkt geht es.
    se.set_dimension_value(&mut sk, id, 29.0).unwrap();
    let (from, to) = line_ends(&sk, line);
    assert!((to - from).x > 0.0, "{from:?} {to:?}");
}

#[test]
fn a_straight_slot_wider_than_its_center_distance_stays_allowed() {
    let mut w = rig(ToolKind::SlotCenterToCenter);
    commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
    type_in(&mut w, F::Length, "20");
    commit(&mut w, place(0.0, 70.0, 0.0)).unwrap();
    type_in(&mut w, F::Width, "12");
    commit(&mut w, place(0.0, 10.0, 30.0)).unwrap();
    let ids = slot_ids(sk(&w));
    let width = dim_id(sk(&w), |k| matches!(k, K::Diameter(..)));
    set(&mut w, width, "60").unwrap();
    assert_solved(sk(&w));
    assert_slot(sk(&w), &ids);
    near(sk(&w).radius(ids.caps[0]).unwrap(), 30.0);
}

#[test]
fn the_guard_does_not_look_at_an_angle_inside_its_range() {
    let (mut se, mut sk) = session(ToolKind::ArcCenter);
    let f = sk.add_point(0.0, 0.0).unwrap();
    let g = sk.add_point(10.0, 0.0).unwrap();
    let h = sk.add_point(10.0 + 8.66, 5.0).unwrap();
    let l0 = sk.add_line(f, g).unwrap();
    let l1 = sk.add_line(g, h).unwrap();
    sk.add_constraint(K::Fixed(f, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::Fixed(g, 10.0, 0.0)).unwrap();
    let id = sk
        .add_constraint(K::Angle(l0, l1, 30f64.to_radians()))
        .unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    // Ein Winkelmaß läuft gar nicht durch den Wächter (`length_value` ist `None`, siehe
    // `shapes_constraints.rs`): Der Wert geht durch, wie auch immer der Löser ihn erreicht.
    se.set_dimension_value(&mut sk, id, 170f64.to_radians())
        .unwrap();
    assert!(se.message_text().is_none());
    assert_solved(&sk);
}

#[test]
fn a_typed_dimension_that_folds_is_refused_and_nothing_stays() {
    // Der Parameter hat beim Abschluss einen anderen Wert als beim Tippen, also trifft die Länge
    // nicht mehr die Lage: Der Endpunkt müsste über den festen Punkt hinaus.
    let scope_with = |formula: &str| {
        let mut s = AppState::new();
        let id = s.add_parameter().unwrap();
        s.edit_parameter(
            id,
            ParamEdit {
                name: "Laenge".into(),
                formula: formula.into(),
                unit: ParamUnit::Length,
                comment: String::new(),
            },
        )
        .unwrap();
        s.document.param_scope()
    };
    let (typed, changed) = (scope_with("20"), scope_with("50"));
    let mut w = rig(ToolKind::SlotCenterToCenter);
    let (a, b) = {
        let (se, sf, _) = parts(&mut w);
        let (_, built, a, b, _) = line_past_its_fixed_end();
        sf.sketch = built;
        se.set_tool(&mut sf.sketch, Tool::Line);
        (a, b)
    };
    let start = xy(sk(&w), a);
    let end = xy(sk(&w), b);
    let (se, sf, _) = parts(&mut w);
    se.commit_input(sf, start + at(0.05, 0.05), R, false, &typed)
        .unwrap();
    se.sync_input(&sf.sketch);
    se.type_field(0, "Laenge", &typed);
    let before = bits(sk(&w));
    let (se, sf, _) = parts(&mut w);
    let text = se
        .commit_input(sf, end + at(0.05, 0.05), R, false, &changed)
        .unwrap_err();
    assert_eq!(text, t("sketch-dimension-folds"));
    assert_eq!(bits(sk(&w)), before);
    assert_eq!(dims(sk(&w)).len(), 0);
}

#[test]
fn a_sweep_dimension_outside_0_to_180_is_refused_and_150_goes_through() {
    let mut w = center_arc_with_fields(0.0);
    let ids = arc_slot_ids(sk(&w));
    let sweep = dim_id(sk(&w), |k| matches!(k, K::Angle(..)));
    let before = bits(sk(&w));
    for text in ["200", "270", "300", "-10"] {
        let refused = set(&mut w, sweep, text).unwrap_err();
        assert_eq!(refused, t("sketch-angle-range"), "{text}");
        assert_eq!(
            parts(&mut w).0.message_text().as_deref(),
            Some(t("sketch-angle-range").as_str())
        );
        assert_eq!(bits(sk(&w)), before, "{text}");
    }
    // Genau 180° und 0° gehen zum Löser; erreicht er sie nicht, lehnt der Widerspruchsweg ab.
    for text in ["180", "0"] {
        let refused = set(&mut w, sweep, text).unwrap_err();
        assert_ne!(refused, t("sketch-angle-range"), "{text}");
        assert_eq!(bits(sk(&w)), before, "{text}");
    }
    set(&mut w, sweep, "150").unwrap();
    assert_solved(sk(&w));
    assert_arc_slot(sk(&w), &ids, 50.0, 5.0);
    let (m, start, end) = arc_ends(sk(&w), ids.axis);
    near(
        ((end - m).to_angle() - (start - m).to_angle()).rem_euclid(TAU),
        150f64.to_radians(),
    );
}

#[test]
fn a_typed_sweep_above_180_is_correct_when_it_is_created() {
    for degrees in [200.0_f64, 270.0, 300.0] {
        let mut w = rig(ToolKind::SlotCenterPointArc);
        commit(&mut w, place(0.0, 0.0, 0.0)).unwrap();
        type_in(&mut w, F::Radius, "50");
        commit(&mut w, place(0.0, 80.0, 0.0)).unwrap();
        type_in(&mut w, F::Sweep, &degrees.to_string());
        commit(&mut w, place(0.0, 30.0, 30.0)).unwrap();
        type_in(&mut w, F::Width, "10");
        commit(&mut w, place(0.0, 60.0, 60.0)).unwrap();
        let ids = arc_slot_ids(sk(&w));
        assert_solved(sk(&w));
        assert_arc_slot(sk(&w), &ids, 50.0, 5.0);
        let (m, start, end) = arc_ends(sk(&w), ids.axis);
        near(
            ((end - m).to_angle() - (start - m).to_angle()).rem_euclid(TAU),
            degrees.to_radians(),
        );
        // Dasselbe Maß unverändert bestätigen ändert nichts, das Maß steht ja schon so da.
        let sweep = dim_id(sk(&w), |k| matches!(k, K::Angle(..)));
        set(&mut w, sweep, &degrees.to_string()).unwrap();
    }
}

// ---- Fahrt über Rohereignisse bei 1920×1080, Review-Focus 3 ---------------------------------------

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: NONE,
    }
}

struct Gui {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Gui {
    fn new() -> Gui {
        let root = common::temp_root("planora-sketch-kinds-slot-gui");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sk).unwrap();
        w.observe_history(false);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut gui = Gui {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        gui.settle();
        gui
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        out.drop_without_applying_deltas();
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn click(&mut self, x: f32, y: f32) {
        let p = pos2(x, y);
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    fn key(&mut self, k: Key) {
        self.frame(vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: NONE,
        }]);
        self.settle();
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn choose(&mut self, kind: ToolKind) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.choose_kind(sk, kind);
        self.settle();
    }

    fn sketch(&mut self) -> Sketch {
        self.st().session_sketch().unwrap().clone()
    }

    fn stage(&mut self) -> usize {
        self.st().session_sketch_mut().unwrap().0.kind_stage()
    }

    fn steps(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn undo_label(&mut self) -> Option<String> {
        self.app.ws_mut().active_tab_mut().undo_label()
    }

    fn undo(&mut self) {
        self.app.ws_mut().active_tab_mut().undo().unwrap();
        self.settle();
    }

    fn redo(&mut self) {
        self.app.ws_mut().active_tab_mut().redo().unwrap();
        self.settle();
    }
}

fn state_of(sk: &Sketch) -> (Vec<u64>, usize, usize) {
    (
        sk.variables().iter().map(|v| v.to_bits()).collect(),
        sk.curves().count(),
        sk.constraints().count(),
    )
}

/// Bildschirmlagen je Art, ohne gemeinsame Zeile oder Spalte.
fn screen_clicks(kind: ToolKind) -> Vec<(f32, f32)> {
    match kind {
        ToolKind::SlotThreePointArc => vec![
            (800.0, 300.0),
            (810.0, 700.0),
            (1005.0, 505.0),
            (1040.0, 515.0),
        ],
        ToolKind::SlotCenterPointArc => vec![
            (800.0, 500.0),
            (1000.0, 510.0),
            (810.0, 300.0),
            (960.0, 370.0),
        ],
        _ => vec![(800.0, 500.0), (1100.0, 510.0), (950.0, 440.0)],
    }
}

#[test]
fn each_kind_by_raw_events_is_one_undo_step_and_escape_at_each_stage_leaves_nothing() {
    for kind in KINDS {
        let mut gui = Gui::new();
        gui.choose(kind);
        let (before, steps) = (state_of(&gui.sketch()), gui.steps());
        let pts = screen_clicks(kind);
        for stage in 1..pts.len() {
            for &(x, y) in &pts[..stage] {
                gui.click(x, y);
            }
            assert_eq!(gui.stage(), stage, "{kind:?}");
            gui.key(Key::Escape);
            assert_eq!(state_of(&gui.sketch()), before, "{kind:?} Stufe {stage}");
            assert_eq!(gui.steps(), steps, "{kind:?} Stufe {stage}");
            assert_eq!(gui.stage(), 0, "{kind:?} Stufe {stage}");
        }
        for &(x, y) in &pts {
            gui.click(x, y);
        }
        gui.settle();
        let done = gui.sketch();
        assert_eq!(gui.stage(), 0, "{kind:?}");
        assert_eq!(done.profiles().profiles.len(), 1, "{kind:?}");
        assert_solved(&done);
        assert_eq!(gui.steps(), steps + 1, "{kind:?}: genau ein Schritt");
        assert_eq!(gui.undo_label().as_deref(), Some("Langloch zeichnen"));
        gui.undo();
        assert_eq!(state_of(&gui.sketch()), before, "{kind:?}");
        gui.redo();
        assert_eq!(state_of(&gui.sketch()), state_of(&done), "{kind:?}");
    }
}
