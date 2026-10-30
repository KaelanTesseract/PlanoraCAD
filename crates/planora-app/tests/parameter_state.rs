//! Parameter im App-Zustand (Aufgabe 8): Parameter-Aktionen mit Sperre bei offener Skizze und
//! Meldung, ein Schritt je Parameteränderung, Rückgängig stellt Tabelle, Felder und Körper her,
//! Verschieben mit Formeln (P11), Formeln im Skizzenentwurf, Anlegen mit Formeln, Formelfelder
//! des Netz-Körpers und die geschätzte Verlaufsgröße.

mod common;

use planora_app::history::{sketch_bytes, snapshot_bytes};
use planora_app::state::*;
use planora_app::workspace::*;
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::{
    BlendArt, BodyRef, FeatureId, FeatureKind, FieldKey, Formulas, ParamEdit, ParamId, ParamUnit,
    Parameter, SketchFeature, SketchPlane,
};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DQuat, DVec2, DVec3, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::{ConstraintId, ConstraintKind, Sketch};
use std::sync::Arc;

// ---- Hilfen ---------------------------------------------------------------------------------

fn ws() -> Workspace {
    let root = common::temp_root("planora-parameter-state");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn st(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
}

fn tab(w: &mut Workspace) -> &mut DocTab {
    w.active_tab_mut()
}

fn observe(w: &mut Workspace) {
    w.observe_history(false);
}

fn undo_len(w: &Workspace) -> usize {
    w.active_tab().history().undo_len()
}

fn formulas(pairs: &[(FieldKey, &str)]) -> Formulas {
    pairs.iter().map(|(k, t)| (*k, t.to_string())).collect()
}

/// Neuer Parameter (über die App) mit Name und Formel als Länge.
fn param(s: &mut AppState, name: &str, formula: &str) -> ParamId {
    let id = s.add_parameter().unwrap();
    set_formula_named(s, id, name, formula);
    id
}

fn set_formula_named(s: &mut AppState, id: ParamId, name: &str, formula: &str) {
    let p = s.document.parameter(id).unwrap().clone();
    s.edit_parameter(
        id,
        ParamEdit {
            name: name.into(),
            formula: formula.into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
    .unwrap();
}

fn set_formula(s: &mut AppState, id: ParamId, formula: &str) -> Result<(), String> {
    let p = s.document.parameter(id).unwrap().clone();
    s.edit_parameter(
        id,
        ParamEdit {
            name: p.name,
            formula: formula.into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
}

fn kind(s: &AppState, id: FeatureId) -> FeatureKind {
    s.document.feature(id).unwrap().kind.clone()
}

fn feature_formulas(s: &AppState, id: FeatureId) -> Formulas {
    s.document.feature(id).unwrap().formulas().clone()
}

fn box_size(s: &AppState, id: FeatureId) -> DVec3 {
    match kind(s, id) {
        FeatureKind::Box { size, .. } => size,
        k => panic!("kein Quader: {k:?}"),
    }
}

fn move_delta(s: &AppState, id: FeatureId) -> DVec3 {
    match kind(s, id) {
        FeatureKind::Move { delta, .. } => delta,
        k => panic!("kein Verschieben: {k:?}"),
    }
}

fn width(s: &AppState, body: usize) -> f64 {
    let (lo, hi) = s.bodies[body].bounds;
    hi.x - lo.x
}

fn sketch_def(s: &AppState, id: FeatureId) -> SketchFeature {
    match kind(s, id) {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

/// Würfel als Netz (wie in den Netz-Tests).
fn cube() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
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
    })
}

/// Skizze „Skizze 1“ mit Rechteck 40 × 20 (Ecke im Ursprung fest) und einem Abstandsmaß der
/// Unterseite mit der Formel „Breite“ (Breite = 40); geschlossen. Liefert Skizze und Maß.
fn sketch_with_formula(s: &mut AppState) -> (FeatureId, ConstraintId) {
    let sk = s
        .add_sketch(SketchPlane::Standard(planora_feature::StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    let c = {
        let sketch = s.session_sketch_mut().unwrap().1;
        let r = sketch
            .add_rectangle(DVec2::ZERO, DVec2::new(40.0, 20.0))
            .unwrap();
        let p = r.points;
        sketch
            .add_constraint(ConstraintKind::Fixed(p[0], 0.0, 0.0))
            .unwrap();
        sketch
            .add_constraint(ConstraintKind::Distance(p[0], p[1], 40.0))
            .unwrap()
    };
    s.session_draft_mut()
        .unwrap()
        .1
        .formulas
        .insert(c, "Breite".into());
    s.try_close_sketch().unwrap();
    assert_eq!(
        sketch_def(s, sk).formulas.get(&c).map(String::as_str),
        Some("Breite")
    );
    (sk, c)
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

// ---- (b) Verlaufsgröße ------------------------------------------------------------------------

#[test]
fn bytes_grow() {
    let empty = AppState::new().snapshot();
    let mut one = empty.clone();
    one.parameters.push(Parameter {
        id: ParamId(1),
        name: "Breite".into(),
        formula: "40".into(),
        unit: ParamUnit::Length,
        comment: String::new(),
    });
    one.next_param = 2;
    assert!(snapshot_bytes(&one) > snapshot_bytes(&empty));
    // Name, Formel und Kommentar zählen mit ihrer Länge.
    for change in [
        |p: &mut Parameter| p.comment = "Außenmaß des Rahmens".into(),
        |p: &mut Parameter| p.formula = "40 + 2*Wand".into(),
        |p: &mut Parameter| p.name = "Rahmenbreite".into(),
    ] {
        let mut longer = one.clone();
        change(&mut longer.parameters[0]);
        assert!(snapshot_bytes(&longer) > snapshot_bytes(&one));
    }

    // Feldformeln eines Features.
    let mut s = AppState::new();
    s.create_box(DVec3::new(40.0, 20.0, 10.0)).unwrap();
    let plain = s.snapshot();
    let mut with = plain.clone();
    with.features[0].formulas = formulas(&[(FieldKey::Breite, "40")]);
    assert!(snapshot_bytes(&with) > snapshot_bytes(&plain));
    let mut longer = with.clone();
    longer.features[0].formulas = formulas(&[(FieldKey::Breite, "40 + 0")]);
    assert!(snapshot_bytes(&longer) > snapshot_bytes(&with));

    // Skizzenformeln: im Skizzenentwurf und im Bild des Dokuments.
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    let c = sk
        .add_constraint(ConstraintKind::Distance(a, b, 5.0))
        .unwrap();
    let sf = SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(planora_feature::StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    };
    let mut sf_with = sf.clone();
    sf_with.formulas.insert(c, "5".into());
    assert!(sketch_bytes(&sf_with) > sketch_bytes(&sf));
    let mut sk_snap = AppState::new().snapshot();
    sk_snap.features.push(planora_feature::SnapshotFeature {
        id: FeatureId(1),
        name: "Skizze 1".into(),
        suppressed: false,
        kind: FeatureKind::Sketch(sf.clone()),
        formulas: Formulas::new(),
    });
    let mut sk_snap_with = sk_snap.clone();
    sk_snap_with.features[0].kind = FeatureKind::Sketch(sf_with);
    assert!(snapshot_bytes(&sk_snap_with) > snapshot_bytes(&sk_snap));
}

// ---- (c) Rückgängig stellt Tabelle, Felder und Körper her ---------------------------------------

#[test]
fn undo_restores_table_and_fields() {
    let mut w = ws();
    let breite = param(st(&mut w), "Breite", "40");
    observe(&mut w);
    let q = st(&mut w)
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite")]),
        )
        .unwrap();
    observe(&mut w);
    let steps = undo_len(&w);
    assert_eq!(tab(&mut w).undo_label(), Some("Quader anlegen".to_string()));

    set_formula(st(&mut w), breite, "60").unwrap();
    observe(&mut w);
    assert_eq!(undo_len(&w), steps + 1, "genau ein Schritt");
    assert_eq!(
        tab(&mut w).undo_label(),
        Some("Parameter ändern".to_string())
    );
    assert_eq!(box_size(st(&mut w), q).x, 60.0);
    assert!((width(st(&mut w), 0) - 60.0).abs() < 1e-6);

    tab(&mut w).undo().unwrap();
    observe(&mut w);
    let s = st(&mut w);
    assert_eq!(s.document.parameter(breite).unwrap().formula, "40");
    assert_eq!(s.document.parameter_value(breite), Some(40.0));
    assert_eq!(box_size(s, q).x, 40.0);
    assert_eq!(
        feature_formulas(s, q),
        formulas(&[(FieldKey::Breite, "Breite")])
    );
    assert!((width(s, 0) - 40.0).abs() < 1e-6);
    assert_eq!(undo_len(&w), steps);

    tab(&mut w).redo().unwrap();
    observe(&mut w);
    let s = st(&mut w);
    assert_eq!(s.document.parameter(breite).unwrap().formula, "60");
    assert_eq!(box_size(s, q).x, 60.0);
    assert!((width(s, 0) - 60.0).abs() < 1e-6);
}

#[test]
fn parameter_actions_are_one_step_each() {
    let mut w = ws();
    let a = st(&mut w).add_parameter().unwrap();
    observe(&mut w);
    assert_eq!(
        tab(&mut w).undo_label(),
        Some("Parameter anlegen".to_string())
    );
    assert_eq!(st(&mut w).document.parameter(a).unwrap().name, "Parameter1");
    let breite = param(st(&mut w), "Breite", "40");
    observe(&mut w);
    // Anlegen und Ändern im selben Bild: ein Schritt (Anlegen).
    assert_eq!(
        tab(&mut w).undo_label(),
        Some("Parameter anlegen".to_string())
    );
    let q = st(&mut w)
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite")]),
        )
        .unwrap();
    observe(&mut w);
    st(&mut w).delete_parameter(a).unwrap();
    observe(&mut w);
    assert_eq!(
        tab(&mut w).undo_label(),
        Some("Parameter löschen".to_string())
    );
    let steps = undo_len(&w);
    st(&mut w).replace_parameter(breite).unwrap();
    observe(&mut w);
    assert_eq!(undo_len(&w), steps + 1);
    assert_eq!(
        tab(&mut w).undo_label(),
        Some("Parameter löschen".to_string())
    );
    assert!(st(&mut w).document.parameters().is_empty());
    assert!(feature_formulas(st(&mut w), q).is_empty());
    assert_eq!(box_size(st(&mut w), q).x, 40.0);
    // Rückgängig bringt Parameter und Formel zurück.
    tab(&mut w).undo().unwrap();
    observe(&mut w);
    assert_eq!(
        st(&mut w).document.parameter(breite).unwrap().name,
        "Breite"
    );
    assert_eq!(
        feature_formulas(st(&mut w), q),
        formulas(&[(FieldKey::Breite, "Breite")])
    );
}

#[test]
fn rejected_parameter_change_changes_nothing() {
    let mut s = AppState::new();
    let breite = param(&mut s, "Breite", "40");
    let before = s.snapshot();
    let count = s.document.edit_count();
    let e = set_formula(&mut s, breite, "Breit").unwrap_err();
    assert_eq!(s.message.as_deref(), Some(e.as_str()));
    assert!(e.contains("Breit"), "{e}");
    assert!(s.snapshot().same_content(&before));
    assert_eq!(s.document.edit_count(), count);
    // Benutzter Parameter: Löschen gesperrt mit Nennung der Benutzer.
    s.create_box_with(
        DVec3::new(40.0, 20.0, 10.0),
        formulas(&[(FieldKey::Breite, "Breite")]),
    )
    .unwrap();
    let count = s.document.edit_count();
    assert_eq!(
        s.delete_parameter(breite),
        Err("Breite wird in Quader 1 benutzt.".to_string())
    );
    assert_eq!(s.document.edit_count(), count);
    assert_eq!(
        s.delete_parameter(ParamId(99)),
        Err("Ungültige Eingabe: Den Parameter gibt es in diesem Dokument nicht".to_string())
    );
}

// ---- (d) Sperre bei offener Skizze ------------------------------------------------------------

#[test]
fn sketch_open_locks_parameters() {
    let mut s = AppState::new();
    let breite = param(&mut s, "Breite", "40");
    let sk = s
        .add_sketch(SketchPlane::Standard(planora_feature::StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    let before = s.snapshot();
    let count = s.document.edit_count();
    let lock = Err(sketch_open_lock().to_string());
    assert_eq!(s.add_parameter().map(|_| ()), lock);
    assert_eq!(s.message, Some(sketch_open_lock()));
    s.message = None;
    assert_eq!(set_formula(&mut s, breite, "60"), lock);
    assert_eq!(s.message, Some(sketch_open_lock()));
    assert_eq!(s.delete_parameter(breite), lock);
    assert_eq!(s.replace_parameter(breite), lock);
    assert_eq!(s.document.edit_count(), count);
    assert!(s.snapshot().same_content(&before));
    // Die Skizze bleibt offen.
    assert_eq!(s.open_sketch_id(), Some(sk));
    s.close_sketch();
    s.delete_parameter(breite).unwrap();
}

// ---- (e) Meldung nach einer Änderung, die ein Feature scheitern lässt ---------------------------

#[test]
fn message_after_failing_change() {
    let mut s = AppState::new();
    let breite = param(&mut s, "Breite", "50");
    let q = s
        .create_box_with(
            DVec3::new(10.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite-40")]),
        )
        .unwrap();
    s.selected_body = Some(0);
    assert_eq!(box_size(&s, q).x, 10.0);
    s.message = None;
    set_formula(&mut s, breite, "40").unwrap();
    assert_eq!(
        s.message.as_deref(),
        Some("„Quader 1“: Breite ergibt 0 mm, es muss größer als 0 sein")
    );
    // Der Körper ist weg, die Wahl damit auch (wie bei jeder Änderung).
    assert!(s.bodies.is_empty());
    assert_eq!(s.selected_body, None);
    // Zurück auf 50: der Körper kommt wieder.
    s.message = None;
    set_formula(&mut s, breite, "50").unwrap();
    assert_eq!(s.message, None);
    assert_eq!(s.bodies.len(), 1);
}

#[test]
fn message_names_the_newly_failing_feature() {
    let mut s = AppState::new();
    let breite = param(&mut s, "Breite", "50");
    let tiefe = param(&mut s, "Tiefe", "50");
    // Quader 1 scheitert schon vorher (0 mm), Quader 2 erst nach der Änderung.
    let q1 = s
        .create_box_with(
            DVec3::new(10.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Tiefe-50")]),
        )
        .unwrap();
    assert!(matches!(
        s.document.feature(q1).unwrap().status,
        planora_feature::FeatureStatus::Error(_)
    ));
    s.create_box_with(
        DVec3::new(10.0, 20.0, 10.0),
        formulas(&[(FieldKey::Tiefe, "Breite-40")]),
    )
    .unwrap();
    s.message = None;
    set_formula(&mut s, breite, "40").unwrap();
    assert_eq!(
        s.message.as_deref(),
        Some("„Quader 2“: Tiefe ergibt 0 mm, es muss größer als 0 sein")
    );
    // Eine Änderung, nach der nichts neu scheitert: keine Meldung.
    s.message = None;
    set_formula(&mut s, tiefe, "60").unwrap();
    assert_eq!(s.message, None);
}

// ---- (f) Verschieben mit Formeln (P11) --------------------------------------------------------

#[test]
fn move_with_formulas_not_extended() {
    let mut s = AppState::new();
    param(&mut s, "a", "10");
    s.create_box(DVec3::splat(10.0)).unwrap();
    let body = s.bodies[0].source;
    let m = s
        .document
        .add_feature_with_formulas(
            "Verschieben 1",
            FeatureKind::Move {
                body,
                delta: DVec3::ZERO,
            },
            formulas(&[(FieldKey::X, "a")]),
        )
        .unwrap();
    s.refresh();
    assert_eq!(move_delta(&s, m), DVec3::new(10.0, 0.0, 0.0));
    let count = s.document.features().len();

    s.move_body(0, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    assert_eq!(
        s.document.features().len(),
        count + 1,
        "ein neues Verschieben"
    );
    assert_eq!(move_delta(&s, m), DVec3::new(10.0, 0.0, 0.0));
    assert_eq!(feature_formulas(&s, m), formulas(&[(FieldKey::X, "a")]));
    let newer = s.document.features().last().unwrap().id;
    assert_eq!(move_delta(&s, newer), DVec3::new(5.0, 0.0, 0.0));
    assert_eq!(s.bodies[0].source.feature, newer);

    // Ohne Formeln wird wie bisher erweitert.
    s.move_body(0, DVec3::new(0.0, 3.0, 0.0)).unwrap();
    assert_eq!(s.document.features().len(), count + 1);
    assert_eq!(move_delta(&s, newer), DVec3::new(5.0, 3.0, 0.0));
}

#[test]
fn drag_never_extends_move_with_formulas() {
    let mut s = AppState::new();
    param(&mut s, "a", "10");
    s.create_box(DVec3::splat(10.0)).unwrap();
    let body = s.bodies[0].source;
    let m = s
        .document
        .add_feature_with_formulas(
            "Verschieben 1",
            FeatureKind::Move {
                body,
                delta: DVec3::ZERO,
            },
            formulas(&[(FieldKey::Y, "a")]),
        )
        .unwrap();
    s.refresh();
    let count = s.document.features().len();
    s.move_body_dragging(0, DVec3::new(1.0, 0.0, 0.0)).unwrap();
    s.move_body_dragging(0, DVec3::new(1.0, 0.0, 0.0)).unwrap();
    s.end_drag();
    // Ein neues Verschieben für den ganzen Zug, das alte bleibt.
    assert_eq!(s.document.features().len(), count + 1);
    assert_eq!(move_delta(&s, m), DVec3::new(0.0, 10.0, 0.0));
    assert_eq!(feature_formulas(&s, m), formulas(&[(FieldKey::Y, "a")]));
    let newer = s.document.features().last().unwrap().id;
    assert_eq!(move_delta(&s, newer), DVec3::new(2.0, 0.0, 0.0));
}

// ---- (g)/(h) Formeln im Skizzenentwurf --------------------------------------------------------

#[test]
fn formula_only_change_commits() {
    let mut w = ws();
    param(st(&mut w), "Breite", "40");
    let (sk, c) = sketch_with_formula(st(&mut w));
    observe(&mut w);
    let steps = undo_len(&w);

    st(&mut w).open_sketch(sk).unwrap();
    st(&mut w)
        .session_draft_mut()
        .unwrap()
        .1
        .formulas
        .insert(c, "breite".into());
    observe(&mut w);
    assert_eq!(tab(&mut w).undo_label(), Some("Formel ändern".to_string()));
    st(&mut w).try_close_sketch().unwrap();
    observe(&mut w);
    let s = st(&mut w);
    assert_eq!(
        sketch_def(s, sk).formulas.get(&c).map(String::as_str),
        Some("breite")
    );
    // Der Wert bleibt gleich.
    match sketch_def(s, sk).sketch.constraint(c).unwrap().kind {
        ConstraintKind::Distance(_, _, v) => assert_eq!(v, 40.0),
        ref k => panic!("kein Abstand: {k:?}"),
    }
    assert_eq!(undo_len(&w), steps + 1);
    assert_eq!(
        tab(&mut w).undo_label(),
        Some("Skizze bearbeiten".to_string())
    );

    // Ein unveränderter Entwurf bleibt ohne Schritt.
    let count = st(&mut w).document.edit_count();
    st(&mut w).open_sketch(sk).unwrap();
    st(&mut w).try_close_sketch().unwrap();
    observe(&mut w);
    assert_eq!(st(&mut w).document.edit_count(), count);
    assert_eq!(undo_len(&w), steps + 1);
}

#[test]
fn observe_prunes() {
    let mut s = AppState::new();
    param(&mut s, "Breite", "40");
    let (sk, c) = sketch_with_formula(&mut s);
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .remove_constraint(c)
        .unwrap();
    // Während einer Geste bleibt der Verlauf, wie er ist.
    s.observe_sketch(true);
    s.observe_sketch(false);
    let (_, draft) = s.session_draft_mut().unwrap();
    assert!(draft.formulas.is_empty());
    let h = s.sketch_history().unwrap();
    assert!(h.current().formulas.is_empty());
    assert_eq!(h.undo_label(), Some("Löschen"));
    s.try_close_sketch().unwrap();
    assert!(sketch_def(&s, sk).formulas.is_empty());
}

#[test]
fn parameter_change_follows_into_sketch() {
    let mut s = AppState::new();
    let breite = param(&mut s, "Breite", "40");
    let (sk, c) = sketch_with_formula(&mut s);
    set_formula(&mut s, breite, "50").unwrap();
    let def = sketch_def(&s, sk);
    match def.sketch.constraint(c).unwrap().kind {
        ConstraintKind::Distance(_, _, v) => assert_eq!(v, 50.0),
        ref k => panic!("kein Abstand: {k:?}"),
    }
}

// ---- (i) Anlegen mit Formeln, Netz-Körper -----------------------------------------------------

#[test]
fn create_with_formulas() {
    let mut w = ws();
    param(st(&mut w), "Breite", "40");
    observe(&mut w);
    let steps = undo_len(&w);
    let q = st(&mut w)
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite")]),
        )
        .unwrap();
    observe(&mut w);
    assert_eq!(undo_len(&w), steps + 1);
    assert_eq!(tab(&mut w).undo_label(), Some("Quader anlegen".to_string()));
    let s = st(&mut w);
    assert_eq!(
        feature_formulas(s, q),
        formulas(&[(FieldKey::Breite, "Breite")])
    );
    assert_eq!(box_size(s, q), DVec3::new(40.0, 20.0, 10.0));

    // Ohne Formeln wie bisher (create_box ruft create_box_with mit leerer Tabelle).
    let plain = s.create_box(DVec3::splat(5.0)).unwrap();
    assert!(feature_formulas(s, plain).is_empty());

    // Zylinder, Kugel, Kegel.
    let z = s
        .create_cylinder_with(5.0, 10.0, formulas(&[(FieldKey::Hoehe, "Breite/4")]))
        .unwrap();
    assert!(matches!(kind(s, z), FeatureKind::Cylinder { height, .. } if height == 10.0));
    assert_eq!(feature_formulas(s, z).len(), 1);
    let k = s
        .create_sphere_with(5.0, formulas(&[(FieldKey::Radius, "Breite/8")]))
        .unwrap();
    assert!(matches!(kind(s, k), FeatureKind::Sphere { radius, .. } if radius == 5.0));
    let c = s
        .create_cone_with(5.0, 20.0, formulas(&[(FieldKey::Hoehe, "Breite/2")]))
        .unwrap();
    assert!(matches!(kind(s, c), FeatureKind::Cone { height, .. } if height == 20.0));

    // Eine kaputte Formel wird abgelehnt, nichts entsteht.
    let count = s.document.features().len();
    let e = s
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[(FieldKey::Tiefe, "Breit")]),
        )
        .unwrap_err();
    assert!(e.contains("Breit"), "{e}");
    assert_eq!(s.message.as_deref(), Some(e.as_str()));
    assert_eq!(s.document.features().len(), count);
}

#[test]
fn extrude_and_blend_with_formulas() {
    let mut s = AppState::new();
    param(&mut s, "Breite", "40");
    let (sk, _) = sketch_with_formula(&mut s);
    let e = s
        .extrude_sketch_with(sk, 0, 5.0, formulas(&[(FieldKey::Hoehe, "Breite/8")]))
        .unwrap();
    assert!(matches!(kind(&s, e), FeatureKind::Extrude { height, .. } if height == 5.0));
    assert_eq!(
        feature_formulas(&s, e),
        formulas(&[(FieldKey::Hoehe, "Breite/8")])
    );
    // Ohne Formeln wie bisher.
    let plain = s.extrude_sketch(sk, 0, 3.0).unwrap();
    assert!(feature_formulas(&s, plain).is_empty());

    let mut s = AppState::new();
    param(&mut s, "Breite", "40");
    s.create_box(DVec3::splat(20.0)).unwrap();
    let edge = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    let r = s
        .add_blend_with(
            0,
            &[edge],
            BlendArt::Rundung { radius: 2.0 },
            false,
            None,
            formulas(&[(FieldKey::Radius, "Breite/20")]),
        )
        .unwrap();
    assert_eq!(
        feature_formulas(&s, r),
        formulas(&[(FieldKey::Radius, "Breite/20")])
    );
    assert_eq!(
        s.bodies[s.selected_body.unwrap()].source,
        BodyRef {
            feature: r,
            index: 0
        }
    );
}

#[test]
fn mesh_formula_fields() {
    let mut s = AppState::new();
    param(&mut s, "a", "12");
    let n = s.import_mesh("Netz", cube(), 1.0).unwrap();
    let i = s
        .body_index(BodyRef {
            feature: n,
            index: 0,
        })
        .unwrap();
    s.set_mesh_formula(i, FieldKey::X, "a").unwrap();
    assert_eq!(feature_formulas(&s, n), formulas(&[(FieldKey::X, "a")]));
    assert_eq!(s.mesh_transform(i).unwrap().offset.x, 12.0);
    s.set_mesh_formula(i, FieldKey::Y, "a/2").unwrap();
    assert_eq!(s.mesh_transform(i).unwrap().offset.y, 6.0);

    // Eine getippte Zahl ersetzt nur die Formel dieses Feldes.
    s.set_mesh_formula(i, FieldKey::X, "7").unwrap();
    assert_eq!(feature_formulas(&s, n), formulas(&[(FieldKey::Y, "a/2")]));
    assert_eq!(s.mesh_transform(i).unwrap().offset.x, 7.0);
    s.set_mesh_formula(i, FieldKey::Massstab, "2,5").unwrap();
    assert_eq!(s.mesh_transform(i).unwrap().scale, 2.5);

    // Abgelehnt (kaputte Formel, Länge im Maßstab, Wert außerhalb): nichts ändert sich.
    let before = s.snapshot();
    for (key, text) in [
        (FieldKey::X, "b"),
        (FieldKey::Massstab, "a"),
        (FieldKey::Massstab, "0"),
        (FieldKey::X, "1e300"),
        (FieldKey::X, "  "),
    ] {
        assert!(s.set_mesh_formula(i, key, text).is_err(), "{text}");
        assert!(s.snapshot().same_content(&before), "{text}");
    }
    assert_eq!(feature_formulas(&s, n), formulas(&[(FieldKey::Y, "a/2")]));
    // Ein Feld, das der Netz-Körper nicht hat.
    assert!(s.set_mesh_formula(i, FieldKey::Breite, "a").is_err());
    // Kein Netz-Körper.
    s.create_box(DVec3::splat(5.0)).unwrap();
    let q = s
        .bodies
        .iter()
        .position(|b| b.kind == BodyKind::Box)
        .unwrap();
    assert!(s.set_mesh_formula(q, FieldKey::X, "1").is_err());
}

#[test]
fn mesh_transform_keeps_other_formulas_and_restores_on_error() {
    let mut s = AppState::new();
    param(&mut s, "a", "12");
    let n = s.import_mesh("Netz", cube(), 1.0).unwrap();
    let i = s
        .body_index(BodyRef {
            feature: n,
            index: 0,
        })
        .unwrap();
    s.set_mesh_formula(i, FieldKey::X, "a").unwrap();
    s.set_mesh_formula(i, FieldKey::Massstab, "a/a*2").unwrap();
    let t = s.mesh_transform(i).unwrap();

    // Nur die Drehung ändert sich: alle Formeln bleiben.
    let turned = MeshTransform {
        rotation: DQuat::from_rotation_z(0.5),
        ..t
    };
    s.set_mesh_transform(i, turned).unwrap();
    let i = s
        .body_index(BodyRef {
            feature: n,
            index: 0,
        })
        .unwrap();
    assert_eq!(feature_formulas(&s, n).len(), 2);

    // Lage außerhalb des Bereichs: alte Lage samt Formeln zurück.
    let before = s.snapshot();
    let far = MeshTransform {
        offset: DVec3::new(12.0, 1e300, 0.0),
        ..turned
    };
    assert!(s.set_mesh_transform(i, far).is_err());
    assert!(s.snapshot().features == before.features);
    assert_eq!(
        feature_formulas(&s, n),
        formulas(&[(FieldKey::X, "a"), (FieldKey::Massstab, "a/a*2")])
    );

    // Verschieben des Netz-Körpers ersetzt nur die Formel der bewegten Richtung (P2).
    let i = s
        .body_index(BodyRef {
            feature: n,
            index: 0,
        })
        .unwrap();
    s.move_body(i, DVec3::new(0.0, 4.0, 0.0)).unwrap();
    assert_eq!(feature_formulas(&s, n).len(), 2);
    let i = s
        .body_index(BodyRef {
            feature: n,
            index: 0,
        })
        .unwrap();
    s.move_body(i, DVec3::new(3.0, 0.0, 0.0)).unwrap();
    assert_eq!(
        feature_formulas(&s, n),
        formulas(&[(FieldKey::Massstab, "a/a*2")])
    );
    let i = s
        .body_index(BodyRef {
            feature: n,
            index: 0,
        })
        .unwrap();
    assert_eq!(s.mesh_transform(i).unwrap().offset.x, 15.0);
}
