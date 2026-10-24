//! Modell der Verlaufsleiste (ohne Oberfläche): Schritte eines Körpers, Marker je Körper,
//! Körperfarbe, Sperre und der gewählte Körper der Leiste.

use planora_app::history_bar::{self, LOCK_HINT, StepState};
use planora_app::icons::FeatureIcon;
use planora_app::state::*;
use planora_app::theme;
use planora_feature::{FeatureId, FeatureKind, SketchPlane};
use planora_kernel::boolean::BoolOp;
use planora_math::{DVec2, DVec3};

const BOX: DVec3 = DVec3::new(10.0, 20.0, 30.0);

/// Index des sichtbaren Körpers, den `feature` erzeugt.
fn body_of(s: &AppState, feature: FeatureId) -> usize {
    s.bodies
        .iter()
        .position(|b| b.source.feature == feature)
        .expect("Körper sichtbar")
}

/// Geschlossene Rechteckskizze auf XY.
fn rect_sketch(s: &mut AppState) -> FeatureId {
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(5.0, 10.0))
        .unwrap();
    s.close_sketch();
    sk
}

/// Skizze, Extrusion, Verschieben: eine Kette aus drei Schritten. Liefert `[s, e, m]`.
fn chain_model() -> (AppState, [FeatureId; 3]) {
    let mut s = AppState::new();
    let sk = rect_sketch(&mut s);
    let e = s.extrude_sketch(sk, 0, 2.0).unwrap();
    s.move_body(body_of(&s, e), DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    assert!(matches!(
        s.document.feature(m).unwrap().kind,
        FeatureKind::Move { .. }
    ));
    assert_eq!(s.document.body_tips(), vec![m]);
    (s, [sk, e, m])
}

fn ids(s: &AppState, tip: FeatureId) -> Vec<FeatureId> {
    history_bar::steps(s, tip).iter().map(|v| v.id).collect()
}

#[test]
fn steps_lists_chain_in_order_with_states() {
    let (mut s, [sk, e, m]) = chain_model();
    let other = s.create_box(BOX).unwrap();
    let steps = history_bar::steps(&s, m);
    assert_eq!(ids(&s, m), vec![sk, e, m], "nur die eigene Kette");
    for v in &steps {
        let f = s.document.feature(v.id).unwrap();
        assert_eq!(v.name, f.name);
        assert_eq!(v.icon, FeatureIcon::of(&f.kind));
        assert_eq!(v.state, StepState::Active);
        assert!(!v.shared);
    }
    assert_eq!(steps[0].icon, FeatureIcon::Sketch);
    assert_eq!(ids(&s, other), vec![other]);
    // Hinter dem Marker abgeblendet.
    history_bar::move_marker(&mut s, m, 1).unwrap();
    let states: Vec<StepState> = history_bar::steps(&s, m)
        .into_iter()
        .map(|v| v.state)
        .collect();
    assert_eq!(
        states,
        vec![StepState::Active, StepState::Inactive, StepState::Inactive]
    );
    // Fehler eines Schritts aus seinem Status.
    let (mut s, [_, e, m]) = chain_model();
    s.document
        .update(e, |k| {
            if let FeatureKind::Extrude { profile, .. } = k {
                *profile = 7;
            }
        })
        .unwrap();
    s.refresh();
    let v = &history_bar::steps(&s, m)[1];
    assert!(
        matches!(&v.state, StepState::Error(t) if !t.is_empty()),
        "{:?}",
        v.state
    );
    // Unbekannter Körper: keine Schritte.
    assert!(history_bar::steps(&s, FeatureId(999)).is_empty());
}

#[test]
fn move_marker_back_sets_rollback_and_forward_clears_it() {
    let (mut s, [_, e, m]) = chain_model();
    assert_eq!(history_bar::marker_position(&s, m), 3);
    history_bar::move_marker(&mut s, m, 1).unwrap();
    assert_eq!(s.document.rollbacks(), &[e]);
    assert_eq!(history_bar::marker_position(&s, m), 1);
    assert!(s.bodies.is_empty(), "Körper zurückgesetzt");
    history_bar::move_marker(&mut s, m, 2).unwrap();
    assert_eq!(s.document.rollbacks(), &[m]);
    assert_eq!(history_bar::marker_position(&s, m), 2);
    assert_eq!(s.bodies.len(), 1, "Extrusion sichtbar");
    assert_eq!(s.bodies[0].source.feature, e);
    // Außerhalb der Kette: Fehler, nichts geändert.
    assert!(history_bar::move_marker(&mut s, m, 4).is_err());
    assert!(history_bar::move_marker(&mut s, FeatureId(999), 0).is_err());
    assert!(s.message.is_some());
    assert_eq!(s.document.rollbacks(), &[m]);
}

#[test]
fn move_marker_to_the_end_clears() {
    let (mut s, [sk, _, m]) = chain_model();
    history_bar::move_marker(&mut s, m, 0).unwrap();
    assert_eq!(s.document.rollbacks(), &[sk]);
    assert_eq!(history_bar::marker_position(&s, m), 0);
    history_bar::move_marker(&mut s, m, 3).unwrap();
    assert!(s.document.rollbacks().is_empty());
    assert_eq!(history_bar::marker_position(&s, m), 3);
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.bodies[0].source.feature, m);
}

#[test]
fn step_forward_and_back_walks_one_step() {
    let (mut s, [_, e, m]) = chain_model();
    let pos = |s: &AppState| history_bar::marker_position(s, m);
    history_bar::step(&mut s, m, false).unwrap();
    assert_eq!(pos(&s), 2);
    history_bar::step(&mut s, m, false).unwrap();
    assert_eq!(pos(&s), 1);
    assert_eq!(s.document.rollbacks(), &[e]);
    history_bar::step(&mut s, m, false).unwrap();
    history_bar::step(&mut s, m, false).unwrap();
    assert_eq!(pos(&s), 0, "am Anfang bleibt es dort");
    history_bar::step(&mut s, m, true).unwrap();
    assert_eq!(pos(&s), 1);
    history_bar::to_end(&mut s, m).unwrap();
    assert_eq!(pos(&s), 3);
    assert!(s.document.rollbacks().is_empty());
    history_bar::step(&mut s, m, true).unwrap();
    assert_eq!(pos(&s), 3, "am Ende bleibt es dort");
    history_bar::to_start(&mut s, m).unwrap();
    assert_eq!(pos(&s), 0);
}

#[test]
fn marker_in_a_union_chain_follows_list_order() {
    // Quader A, Verschieben, Quader B, Vereinigen: ein Schritt nach dem anderen.
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(10.0)).unwrap();
    let b = s.create_box(DVec3::splat(10.0)).unwrap();
    let (pa, pb) = (s.bodies[0].bounds.0, s.bodies[1].bounds.0);
    s.move_body(body_of(&s, b), pa - pb + DVec3::new(5.0, 0.0, 0.0))
        .unwrap();
    let mb = s.document.features().last().unwrap().id;
    let u = s
        .add_boolean(body_of(&s, a), body_of(&s, mb), BoolOp::Union)
        .unwrap();
    assert_eq!(ids(&s, u), vec![a, b, mb, u]);
    // Zwei Stellen in der Kette (je Eingangskette eine): der Marker steht vor der vordersten.
    s.document.set_rollback(u, mb).unwrap();
    s.document.set_rollback(u, a).unwrap();
    s.refresh();
    assert_eq!(s.document.rollbacks(), &[a, mb]);
    assert_eq!(history_bar::marker_position(&s, u), 0);
    // Vorwärts: die Stelle wandert, bis alles aktiv ist.
    for want in 1..=4 {
        history_bar::step(&mut s, u, true).unwrap();
        assert_eq!(history_bar::marker_position(&s, u), want);
        s.document.check_invariants().unwrap();
    }
    assert!(s.document.rollbacks().is_empty());
}

#[test]
fn shared_step_is_flagged_for_both_bodies() {
    // Eine Skizze, zweimal extrudiert; die erste Extrusion mit einem Quader vereinigt.
    let mut s = AppState::new();
    let sk = rect_sketch(&mut s);
    let e1 = s.extrude_sketch(sk, 0, 2.0).unwrap();
    let e2 = s.extrude_sketch(sk, 0, 4.0).unwrap();
    let q = s.create_box(DVec3::splat(3.0)).unwrap();
    let (pe, pq) = (
        s.bodies[body_of(&s, e1)].bounds.0,
        s.bodies[body_of(&s, q)].bounds.0,
    );
    s.move_body(body_of(&s, q), pe - pq).unwrap();
    let mq = s.document.features().last().unwrap().id;
    let u = s
        .add_boolean(body_of(&s, e1), body_of(&s, mq), BoolOp::Union)
        .unwrap();
    assert_eq!(s.document.body_tips(), vec![e2, u]);
    let flags = |tip: FeatureId| -> Vec<(FeatureId, bool)> {
        history_bar::steps(&s, tip)
            .into_iter()
            .map(|v| (v.id, v.shared))
            .collect()
    };
    assert_eq!(flags(e2), vec![(sk, true), (e2, false)]);
    assert_eq!(
        flags(u),
        vec![(sk, true), (e1, false), (q, false), (mq, false), (u, false)]
    );
}

#[test]
fn body_color_is_stable_when_other_bodies_are_deleted() {
    let mut s = AppState::new();
    let a = s.create_box(BOX).unwrap();
    let b = s.create_box(BOX).unwrap();
    let c = s.create_box(BOX).unwrap();
    let colors: Vec<_> = [a, b, c]
        .iter()
        .map(|&t| history_bar::body_color(&s, t))
        .collect();
    assert_ne!(colors[0], colors[1]);
    assert_ne!(colors[1], colors[2]);
    s.delete_feature(b).unwrap();
    assert_eq!(history_bar::body_color(&s, c), colors[2]);
    assert_eq!(history_bar::body_color(&s, a), colors[0]);
    // Ein Verschieben behält die Farbe des ersten Schritts.
    s.move_body(body_of(&s, c), DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    assert_eq!(history_bar::body_color(&s, m), colors[2]);
    // Die Reihe stammt aus dem Theme: kleinste Kennung modulo Länge.
    let n = theme::BODY_COLORS.len();
    assert_eq!(n, 8);
    assert_eq!(colors[2], theme::BODY_COLORS[(c.0 as usize) % n]);
}

#[test]
fn body_colors_are_distinct_and_legible_on_panels() {
    for (i, c) in theme::BODY_COLORS.iter().enumerate() {
        for dark in [true, false] {
            let bg = theme::palette(dark).bg_panel;
            let r = theme::contrast_ratio(c.get(dark), bg);
            assert!(r >= 3.0, "Farbe {i} ({dark}): {r}");
        }
        for (j, d) in theme::BODY_COLORS.iter().enumerate() {
            if i != j {
                assert_ne!(c.dark, d.dark);
                assert_ne!(c.light, d.light);
            }
        }
    }
}

#[test]
fn locked_body_reports_hint() {
    let (mut s, [.., m]) = chain_model();
    let other = s.create_box(BOX).unwrap();
    assert!(!history_bar::is_locked(&s, m));
    history_bar::move_marker(&mut s, m, 2).unwrap();
    assert!(history_bar::is_locked(&s, m));
    assert!(!history_bar::is_locked(&s, other), "andere Körper nicht");
    assert_eq!(
        LOCK_HINT,
        "Dieser Körper ist zurückgesetzt. Springe zuerst zum Ende des Verlaufs."
    );
    history_bar::to_end(&mut s, m).unwrap();
    assert!(!history_bar::is_locked(&s, m));
}

#[test]
fn selected_body_falls_back_when_body_is_deleted() {
    let mut s = AppState::new();
    assert_eq!(s.history_body(), None, "leeres Dokument");
    let a = s.create_box(BOX).unwrap();
    let b = s.create_box(BOX).unwrap();
    assert_eq!(s.history_body(), Some(b), "Standard: der letzte Körper");
    s.select_body(Some(a));
    assert_eq!(s.history_body(), Some(a));
    s.delete_feature(a).unwrap();
    assert_eq!(s.history_body(), Some(b), "gelöscht: neu gewählt");
    s.select_body(None);
    assert_eq!(s.history_body(), Some(b));
    // Geht der Körper in einem späteren Schritt auf, gilt dessen Ergebnis.
    let c = s.create_box(BOX).unwrap();
    s.select_body(Some(b));
    s.move_body(body_of(&s, b), DVec3::X).unwrap();
    let mb = s.document.features().last().unwrap().id;
    assert_eq!(s.history_body(), Some(mb));
    assert_eq!(s.document.body_tips(), vec![c, mb]);
    // Neuer Tab: eigener Zustand mit eigener Wahl.
    assert_eq!(AppState::new().history_body(), None);
    s.delete_feature_with_dependents(b).unwrap();
    s.delete_feature(c).unwrap();
    assert_eq!(s.history_body(), None);
}

#[test]
fn move_marker_ends_an_open_sketch() {
    let mut s = AppState::new();
    let a = s.create_box(BOX).unwrap();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    assert_eq!(s.open_sketch_id(), Some(sk));
    history_bar::move_marker(&mut s, a, 0).unwrap();
    assert_eq!(s.open_sketch_id(), None, "Skizze beendet");
    assert_eq!(s.document.rollbacks(), &[a]);
}
