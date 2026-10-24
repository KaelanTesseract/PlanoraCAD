//! Rückrollstellen je Körper: inaktive Schritte, Vorwärtsspringen aus dem Zwischenspeicher,
//! unabhängige Ketten, Zusammenspiel mit dem globalen Marker und dem Löschen.

use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{BoolOp, Edge, Id, Solid};
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::Sketch;

fn volume(s: &Solid) -> f64 {
    mass_properties(
        &tessellate(
            s,
            &TessellationOptions {
                chord_tolerance: 1e-3,
            },
        )
        .unwrap(),
    )
    .volume
}

fn body(feature: FeatureId) -> BodyRef {
    BodyRef { feature, index: 0 }
}

fn status(d: &Document, id: FeatureId) -> FeatureStatus {
    d.feature(id).unwrap().status.clone()
}

fn visible(d: &Document) -> Vec<FeatureId> {
    d.visible_bodies().iter().map(|v| v.body.feature).collect()
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
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn boxed(d: &mut Document, name: &str, size: f64) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Box {
            size: DVec3::splat(size),
            origin: DVec3::ZERO,
        },
    )
    .unwrap()
}

fn moved(d: &mut Document, name: &str, of: FeatureId, delta: DVec3) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Move {
            body: body(of),
            delta,
        },
    )
    .unwrap()
}

/// Verdrängtes Volumen der Rundung (Radius 2) an einer Kante der Länge `len`.
fn spandrel(len: f64) -> f64 {
    (1.0 - PI / 4.0) * 4.0 * len
}

/// Quader A (10 mm), Verschieben(A), Rundung (Radius 2) an einer Kante des verschobenen Körpers.
fn fillet_model() -> (Document, FeatureId, FeatureId, FeatureId) {
    let mut d = Document::new();
    let a = boxed(&mut d, "Quader 1", 10.0);
    let m = moved(&mut d, "Verschieben 1", a, DVec3::new(5.0, 0.0, 0.0));
    let s = d.body(body(m)).unwrap();
    let edge = EdgeRef::from_edge(
        s,
        edge_between(s, "Deckel", "Wand 1"),
        &Tolerance::default(),
    )
    .unwrap();
    let f = d
        .add_feature(
            "Rundung 1",
            FeatureKind::Verrunden {
                body: body(m),
                edges: vec![edge],
                art: BlendArt::Rundung { radius: 2.0 },
                tangentenkette: true,
            },
        )
        .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    (d, a, m, f)
}

/// Quader A, Verschieben(A), Zylinder B, Verschieben(B), Vereinigen der beiden verschobenen
/// Körper. Liefert `[a, ma, b, mb, u]`.
fn union_model() -> (Document, [FeatureId; 5]) {
    let mut d = Document::new();
    let a = boxed(&mut d, "Quader 1", 4.0);
    let ma = moved(&mut d, "Verschieben 1", a, DVec3::new(1.0, 0.0, 0.0));
    let b = d
        .add_feature(
            "Zylinder 1",
            FeatureKind::Cylinder {
                radius: 1.0,
                height: 6.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let mb = moved(&mut d, "Verschieben 2", b, DVec3::new(0.0, 1.0, 0.0));
    let u = d
        .add_feature(
            "Vereinigen 1",
            FeatureKind::Boolean {
                target: body(ma),
                tool: body(mb),
                op: BoolOp::Union,
            },
        )
        .unwrap();
    assert_eq!(status(&d, u), FeatureStatus::Ok);
    (d, [a, ma, b, mb, u])
}

/// Rechteckskizze auf XY und ihre Extrusion (10 mm).
fn sketch_model() -> (Document, FeatureId, FeatureId) {
    let mut d = Document::new();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(40.0, 30.0))
        .unwrap();
    let s = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 10.0,
            },
        )
        .unwrap();
    (d, s, e)
}

#[test]
fn rollback_marks_step_and_dependents_not_computed() {
    let (mut d, a, m, f) = fillet_model();
    let edits = d.edit_count();
    d.set_rollback(f, m).unwrap();
    assert_eq!(d.edit_count(), edits + 1, "zählt als Änderung");
    assert_eq!(status(&d, a), FeatureStatus::Ok);
    assert_eq!(status(&d, m), FeatureStatus::NotComputed);
    assert_eq!(status(&d, f), FeatureStatus::NotComputed);
    assert_eq!(visible(&d), vec![a], "sichtbar ist der Quader");
    assert_eq!(d.rollbacks(), &[m]);
    assert_eq!(d.rollback_in_chain(f), Some(m));
    assert!(d.is_active(a));
    assert!(!d.is_active(m));
    assert!(!d.is_active(f));
    assert_eq!(d.marker(), 3, "der globale Marker bleibt");
    d.check_invariants().unwrap();
}

#[test]
fn moving_forward_restores_identical_geometry() {
    let (mut d, a, m, f) = fillet_model();
    let before = d.visible_bodies()[0].solid;
    let (vol, corners) = (volume(before), before.vertices.len());
    d.set_rollback(f, m).unwrap();
    let report = d.clear_rollbacks_with_report(f).unwrap();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    for id in [a, m, f] {
        assert!(report.reused.contains(&id), "{id:?} in {:?}", report.reused);
    }
    assert!(d.rollbacks().is_empty());
    assert_eq!(visible(&d), vec![f]);
    let after = d.visible_bodies()[0].solid;
    assert_eq!(volume(after).to_bits(), vol.to_bits());
    assert_eq!(after.vertices.len(), corners);
    d.check_invariants().unwrap();
}

#[test]
fn rollback_before_a_boolean_shows_both_inputs_separately() {
    let (mut d, [a, ma, b, mb, u]) = union_model();
    d.set_rollback(u, ma).unwrap();
    assert_eq!(status(&d, ma), FeatureStatus::NotComputed);
    assert_eq!(status(&d, u), FeatureStatus::NotComputed);
    assert_eq!(status(&d, a), FeatureStatus::Ok);
    assert_eq!(status(&d, b), FeatureStatus::Ok);
    assert_eq!(
        status(&d, mb),
        FeatureStatus::Ok,
        "Kette von B bleibt aktiv"
    );
    assert_eq!(visible(&d), vec![a, mb], "beide Eingangskörper einzeln");
    d.check_invariants().unwrap();
    // Zweite Stelle in der Kette von B: die Stelle von A bleibt.
    d.set_rollback(mb, mb).unwrap();
    assert_eq!(d.rollbacks(), &[ma, mb]);
    assert_eq!(d.rollback_in_chain(ma), Some(ma));
    assert_eq!(d.rollback_in_chain(mb), Some(mb));
    assert_eq!(status(&d, mb), FeatureStatus::NotComputed);
    assert_eq!(visible(&d), vec![a, b]);
    d.check_invariants().unwrap();
    // B wieder vor: A bleibt zurückgesetzt, das Vereinigen auch.
    d.clear_rollbacks(mb).unwrap();
    assert_eq!(d.rollbacks(), &[ma]);
    assert_eq!(status(&d, u), FeatureStatus::NotComputed);
    d.clear_all_rollbacks();
    assert!(d.rollbacks().is_empty());
    assert_eq!(status(&d, u), FeatureStatus::Ok);
    assert_eq!(visible(&d), vec![u]);
    d.check_invariants().unwrap();
}

#[test]
fn set_rollback_replaces_the_previous_one_in_the_same_chain() {
    let (mut d, a, m, f) = fillet_model();
    d.set_rollback(f, f).unwrap();
    assert_eq!(d.rollbacks(), &[f]);
    assert_eq!(visible(&d), vec![m]);
    d.set_rollback(f, m).unwrap();
    assert_eq!(d.rollbacks(), &[m]);
    assert_eq!(visible(&d), vec![a]);
    // gleiche Stelle noch einmal: keine Änderung
    let edits = d.edit_count();
    d.set_rollback(f, m).unwrap();
    assert_eq!(d.edit_count(), edits);
    // Im Vereinigen-Modell ersetzt eine Stelle nur die vergleichbare (hier die hinter `b` in
    // derselben Eingangskette); die Stelle in der Kette von A bleibt.
    let (mut d, [_, ma, b, mb, u]) = union_model();
    d.set_rollback(ma, ma).unwrap();
    d.set_rollback(mb, mb).unwrap();
    d.set_rollback(u, b).unwrap();
    assert_eq!(d.rollbacks(), &[ma, b]);
    d.check_invariants().unwrap();
    // Eine Stelle hinter dem neuen Schritt (außerhalb der Kette des gewählten Spitzen-Schritts)
    // entfällt ebenfalls: Abhängige Schritte haben keine eigene Stelle.
    let (mut d, [a, ma, ..]) = union_model();
    d.set_rollback(ma, ma).unwrap();
    d.set_rollback(a, a).unwrap();
    assert_eq!(d.rollbacks(), &[a]);
    d.check_invariants().unwrap();
}

#[test]
fn set_rollback_rejects_a_step_outside_the_chain() {
    let (mut d, [a, ma, b, mb, _]) = union_model();
    d.set_rollback(ma, a).unwrap();
    let edits = d.edit_count();
    let e = d.set_rollback(mb, ma).unwrap_err();
    assert!(matches!(e, FeatureError::InvalidInput(_)), "{e:?}");
    assert!(matches!(
        d.set_rollback(FeatureId(999), b),
        Err(FeatureError::UnknownFeature)
    ));
    assert!(matches!(
        d.clear_rollbacks(FeatureId(999)),
        Err(FeatureError::UnknownFeature)
    ));
    assert_eq!(d.rollbacks(), &[a], "nichts geändert");
    assert_eq!(d.edit_count(), edits);
    assert_eq!(status(&d, mb), FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

#[test]
fn sketch_rollback_rolls_back_the_extrude() {
    let (mut d, s, e) = sketch_model();
    d.set_rollback(e, s).unwrap();
    assert!(d.visible_bodies().is_empty(), "Körper weg");
    assert!(d.feature(s).is_some(), "Skizze bleibt in der Liste");
    assert_eq!(status(&d, s), FeatureStatus::NotComputed);
    assert_eq!(status(&d, e), FeatureStatus::NotComputed);
    d.check_invariants().unwrap();
    d.clear_rollbacks(e).unwrap();
    assert!((volume(d.visible_bodies()[0].solid) - 12000.0).abs() < 1e-3);
    d.check_invariants().unwrap();
}

#[test]
fn global_marker_and_rollback_combine() {
    let (mut d, a, m, f) = fillet_model();
    d.set_marker(1).unwrap();
    let statuses: Vec<FeatureStatus> = [a, m, f].iter().map(|&i| status(&d, i)).collect();
    d.set_rollback(f, f).unwrap();
    let now: Vec<FeatureStatus> = [a, m, f].iter().map(|&i| status(&d, i)).collect();
    assert_eq!(
        now, statuses,
        "Marker vor der Stelle: die Stelle ändert nichts"
    );
    assert_eq!(visible(&d), vec![a]);
    d.check_invariants().unwrap();
    d.set_marker(3).unwrap();
    assert_eq!(status(&d, m), FeatureStatus::Ok);
    assert_eq!(
        status(&d, f),
        FeatureStatus::NotComputed,
        "jetzt wirkt die Stelle"
    );
    assert_eq!(visible(&d), vec![m]);
    assert!(d.is_active(m));
    assert!(!d.is_active(f));
    d.check_invariants().unwrap();
}

#[test]
fn deleting_a_step_removes_orphan_rollbacks() {
    let (mut d, a, m, f) = fillet_model();
    d.set_rollback(f, m).unwrap();
    // Schritt hinter der Stelle löschen: die Stelle bleibt.
    assert_eq!(d.delete_with_dependents(f).unwrap(), vec![f]);
    assert_eq!(d.rollbacks(), &[m]);
    d.check_invariants().unwrap();
    // Die Stelle selbst löschen: sie verschwindet.
    assert_eq!(d.delete_with_dependents(m).unwrap(), vec![m]);
    assert!(d.rollbacks().is_empty());
    assert_eq!(status(&d, a), FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

#[test]
fn editing_a_rolled_back_step_recomputes_after_moving_forward() {
    let (mut d, a, m, f) = fillet_model();
    d.set_rollback(f, a).unwrap();
    assert!(d.visible_bodies().is_empty());
    d.update(a, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::splat(12.0);
        }
    })
    .unwrap();
    assert_eq!(status(&d, a), FeatureStatus::NotComputed);
    d.check_invariants().unwrap();
    let report = d.clear_rollbacks_with_report(f).unwrap();
    assert_eq!(report.computed, vec![a, m, f]);
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - (1728.0 - spandrel(12.0))).abs() < 0.05, "{v}");
    d.check_invariants().unwrap();
}

#[test]
fn body_tips_and_chain() {
    let (mut d, a, m, f) = fillet_model();
    assert_eq!(d.body_tips(), vec![f]);
    assert_eq!(d.chain(f), vec![a, m, f]);
    assert_eq!(d.chain(m), vec![a, m]);
    assert_eq!(d.chain(a), vec![a]);
    assert!(d.chain(FeatureId(999)).is_empty());
    assert_eq!(d.rollback_in_chain(f), None);
    // Die Spitzen hängen nur von den Arten ab, nicht vom Status.
    d.set_rollback(f, a).unwrap();
    assert_eq!(d.body_tips(), vec![f]);
    assert_eq!(d.chain(f), vec![a, m, f]);
    assert_eq!(d.rollback_in_chain(m), Some(a));

    let (d, [a, ma, b, mb, u]) = union_model();
    assert_eq!(d.body_tips(), vec![u]);
    assert_eq!(d.chain(u), vec![a, ma, b, mb, u]);
    assert_eq!(d.chain(mb), vec![b, mb]);

    let (d, s, e) = sketch_model();
    assert_eq!(d.body_tips(), vec![e], "Skizzen sind keine Körper");
    assert_eq!(d.chain(e), vec![s, e]);

    let mut d = Document::new();
    let a = boxed(&mut d, "A", 1.0);
    let b = boxed(&mut d, "B", 2.0);
    assert_eq!(d.body_tips(), vec![a, b]);
}

#[test]
fn set_rollback_on_a_union_tip_keeps_the_other_input_chain() {
    let (mut d, [a, ma, b, mb, u]) = union_model();
    assert_eq!(d.body_tips(), vec![u]);
    d.set_rollback(u, ma).unwrap();
    d.set_rollback(u, mb).unwrap();
    assert_eq!(d.rollbacks(), &[ma, mb], "Stellen in beiden Eingangsketten");
    assert_eq!(visible(&d), vec![a, b]);
    d.check_invariants().unwrap();
    // Leiste von A über die Spitze U: nur die Stelle von A wandert, B bleibt zurückgesetzt.
    d.set_rollback(u, a).unwrap();
    assert_eq!(d.rollbacks(), &[a, mb]);
    assert_eq!(status(&d, mb), FeatureStatus::NotComputed);
    assert_eq!(visible(&d), vec![b]);
    d.check_invariants().unwrap();
    // Die Stelle am Vereinigen selbst ersetzt beide (beide liegen in seiner Kette).
    d.set_rollback(u, u).unwrap();
    assert_eq!(d.rollbacks(), &[u]);
    assert_eq!(visible(&d), vec![ma, mb]);
    d.check_invariants().unwrap();
}

#[test]
fn rollback_in_chain_with_two_rollbacks_in_a_union_chain() {
    let (mut d, [_, ma, _, mb, u]) = union_model();
    d.set_rollback(u, mb).unwrap();
    d.set_rollback(u, ma).unwrap();
    assert_eq!(d.rollbacks(), &[ma, mb]);
    // Die Kette des Vereinigens enthält beide; geliefert wird die vorderste in Listenordnung.
    assert_eq!(d.rollback_in_chain(u), Some(ma));
    assert_eq!(d.rollback_in_chain(ma), Some(ma));
    assert_eq!(d.rollback_in_chain(mb), Some(mb));
}

#[test]
fn update_pushing_one_rollback_behind_another_is_pruned() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 2.0);
    let m = moved(&mut d, "Verschieben", a, DVec3::X);
    d.set_rollback(m, m).unwrap();
    d.set_rollback(b, b).unwrap();
    assert_eq!(d.rollbacks(), &[b, m], "unabhängige Ketten");
    // Verschieben verweist jetzt auf B: seine Stelle läge hinter der von B und entfällt.
    d.update(m, |k| {
        if let FeatureKind::Move { body: r, .. } = k {
            *r = body(b);
        }
    })
    .unwrap();
    assert_eq!(d.rollbacks(), &[b]);
    assert_eq!(
        status(&d, m),
        FeatureStatus::NotComputed,
        "abgeleitet inaktiv"
    );
    assert_eq!(visible(&d), vec![a]);
    d.check_invariants().unwrap();
}

/// Ein unbekanntes Feature ist nicht aktiv (es gibt nichts zu berechnen oder zu zeigen).
#[test]
fn is_active_on_unknown_id() {
    let (mut d, a, ..) = fillet_model();
    assert!(!d.is_active(FeatureId(999)));
    assert!(!d.is_active(FeatureId(0)));
    d.delete_with_dependents(a).unwrap();
    assert!(!d.is_active(a), "gelöscht");
}

// ---- Schnappschuss (Rückgängig, Wiederholen, Laden) --------------------------------------

fn statuses(d: &Document) -> Vec<(FeatureId, FeatureStatus)> {
    d.features()
        .iter()
        .map(|f| (f.id, f.status.clone()))
        .collect()
}

#[test]
fn snapshot_restore_brings_rollbacks_back() {
    let (mut d, [_a, ma, b, mb, u]) = union_model();
    d.set_rollback(u, ma).unwrap();
    d.set_rollback(u, mb).unwrap();
    let rollbacks = d.rollbacks().to_vec();
    assert_eq!(rollbacks, vec![ma, mb]);
    let states = statuses(&d);
    let shown = visible(&d);
    let snap = d.snapshot();
    assert_eq!(snap.rollbacks, rollbacks);

    d.clear_all_rollbacks();
    assert!(d.rollbacks().is_empty());
    // Nur die Stellen unterscheiden sich: kein gleiches Modell, Wiederherstellen rechnet neu.
    assert!(!d.snapshot().same_model(&snap));
    assert_eq!(d.restore(snap.clone()).unwrap(), RestoreKind::Full);
    assert_eq!(d.rollbacks(), rollbacks.as_slice());
    assert_eq!(statuses(&d), states);
    assert_eq!(visible(&d), shown);
    assert!(!d.is_active(u) && d.is_active(b));
    d.check_invariants().unwrap();

    // Laden aus dem Bild bringt die Stellen ebenso mit.
    let loaded = Document::from_snapshot(snap, 0.05).unwrap();
    assert_eq!(loaded.rollbacks(), rollbacks.as_slice());
    assert_eq!(statuses(&loaded), states);
}

#[test]
fn snapshot_with_invalid_rollbacks_is_rejected() {
    let (mut d, [a, ma, _b, _mb, u]) = union_model();
    d.set_rollback(u, ma).unwrap();
    let good = d.snapshot();
    let msg = |s: Snapshot| match Document::from_snapshot(s, 0.05) {
        Err(FeatureError::InvalidInput(m)) => m,
        Err(e) => panic!("anderer Fehler: {e:?}"),
        Ok(_) => panic!("Dokument hätte abgelehnt werden müssen"),
    };

    let mut unknown = good.clone();
    unknown.rollbacks = vec![FeatureId(999)];
    assert!(
        msg(unknown).contains("unbekannten Schritt (Kennung 999)"),
        "unbekannt"
    );

    let mut twice = good.clone();
    twice.rollbacks = vec![ma, ma];
    assert!(msg(twice).contains("doppelt"));

    // Zwei Stellen im Verlauf desselben Körpers (Quader 1 liegt in der Kette von Verschieben 1).
    let mut same_chain = good.clone();
    same_chain.rollbacks = vec![a, ma];
    let m = msg(same_chain.clone());
    assert!(
        m.contains("„Quader 1“")
            && m.contains("„Verschieben 1“")
            && m.contains("desselben Körpers"),
        "{m}"
    );
    // Auch der Rückgängig-Weg lehnt ab und lässt das Dokument unverändert.
    let before = d.snapshot();
    assert!(d.restore(same_chain).is_err());
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.rollbacks(), &[ma]);

    // Ungeordnet, aber gültig: wird in Listenordnung übernommen.
    let (mut d2, [_a, ma2, _b, mb2, u2]) = union_model();
    d2.set_rollback(u2, ma2).unwrap();
    d2.set_rollback(u2, mb2).unwrap();
    let mut swapped = d2.snapshot();
    swapped.rollbacks.reverse();
    let loaded = Document::from_snapshot(swapped, 0.05).unwrap();
    assert_eq!(loaded.rollbacks(), &[ma2, mb2]);
}
