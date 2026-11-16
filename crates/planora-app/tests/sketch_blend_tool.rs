//! Übergangskurve als Werkzeug (Skizzen 11a, Task 10): zwei Klicks, ein Rückgängig-Schritt, Art
//! Tangential/Krümmung, Wahl aufheben, Vorschau beim Überfahren, Meldungen am Zeiger, Esc.
//! Ohne Fenster.

mod common;

use planora_app::keymap::{Action, Keymap};
use planora_app::sketch_state::{BlendState, SketchSession, Tool};
use planora_app::workspace::*;
use planora_feature::{ParamScope, SketchFeature, SketchPlane, StandardPlane};
use planora_i18n::{english, german, t, tr, with_lang};
use planora_kernel::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::blend::BlendKind;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use std::sync::Arc;

/// Fangradius in Skizzeneinheiten.
const R: f64 = 0.5;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Offene Skizze auf XY mit dem Werkzeug Übergangskurve. Mit `lines`: zwei freie Linien, Ende
/// (10, 0) der ersten und Ende (20, 5) der zweiten liegen einander gegenüber.
fn rig(lines: bool) -> Workspace {
    let root = common::temp_root("planora-sketch-blend-tool");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let s = &mut w.active_tab_mut().state;
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.sketch_snap_grid = false;
    if lines {
        let sk = s.session_sketch_mut().unwrap().1;
        line(sk, (0.0, 0.0), (10.0, 0.0));
        line(sk, (20.0, 5.0), (30.0, 5.0));
        sk.solve();
    }
    w.observe_history(false);
    let (se, sf, _) = parts(&mut w);
    se.set_tool(&mut sf.sketch, Tool::BlendCurve);
    se.pick_radius = R;
    w
}

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = sk.add_point(a.0, a.1).unwrap();
    let q = sk.add_point(b.0, b.1).unwrap();
    sk.add_line(p, q).unwrap()
}

fn parts(w: &mut Workspace) -> (&mut SketchSession, &mut SketchFeature, ParamScope) {
    let s = &mut w.active_tab_mut().state;
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    (se, sf, scope)
}

fn sk(w: &Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

fn undo_len(w: &mut Workspace) -> usize {
    w.observe_history(false);
    w.active_tab().state.sketch_history().unwrap().undo_len()
}

/// Ein dichter Stand, mit dem sich „bitgleich“ vergleichen lässt.
fn fingerprint(sk: &Sketch) -> (usize, usize, usize, Vec<u64>) {
    (
        sk.points().count(),
        sk.curves().count(),
        sk.constraints().count(),
        sk.variables().iter().map(|v| v.to_bits()).collect(),
    )
}

fn click(w: &mut Workspace, at: DVec2) -> Result<Option<CurveId>, String> {
    let (se, sf, _) = parts(w);
    se.click_blend(&mut sf.sketch, at).map_err(|m| m.text())
}

fn hover(w: &mut Workspace, at: DVec2) -> Option<Vec<DVec2>> {
    let (se, sf, _) = parts(w);
    se.blend_hover(&sf.sketch, at)
}

fn spline_degree(s: &Sketch, c: CurveId) -> u8 {
    match &s.curve(c).unwrap().kind {
        CurveKind::ControlSpline { degree, .. } => *degree,
        k => panic!("kein Kontrollpunkt-Spline: {k:?}"),
    }
}

#[test]
fn two_clicks_make_one_undo_step() {
    let mut w = rig(true);
    let before = fingerprint(sk(&w));
    let steps = undo_len(&mut w);
    assert_eq!(click(&mut w, v(9.5, 0.0)), Ok(None), "erster Klick wählt");
    assert!(parts(&mut w).0.blend.first.is_some());
    assert_eq!(undo_len(&mut w), steps, "die Wahl ist noch kein Schritt");
    let made = click(&mut w, v(20.5, 5.0))
        .unwrap()
        .expect("zweiter Klick legt an");
    assert_eq!(sk(&w).curves().count(), 3, "Kurvenzahl + 1");
    assert!(sk(&w).curve(made).is_some());
    assert!(
        parts(&mut w).0.blend.first.is_none(),
        "die Wahl ist verbraucht"
    );
    assert_eq!(undo_len(&mut w), steps + 1, "ein Rückgängig-Schritt");
    assert!(w.active_tab_mut().state.undo_sketch());
    assert_eq!(
        fingerprint(sk(&w)),
        before,
        "Rückgängig stellt bitgleich her"
    );
}

#[test]
fn default_kind_is_g2() {
    assert_eq!(BlendState::default().kind, BlendKind::G2);
    let mut w = rig(true);
    assert_eq!(parts(&mut w).0.blend.kind, BlendKind::G2);
    click(&mut w, v(9.5, 0.0)).unwrap();
    let c = click(&mut w, v(20.5, 5.0)).unwrap().unwrap();
    assert_eq!(spline_degree(sk(&w), c), 5);
    assert_eq!(
        sk(&w)
            .constraints()
            .filter(|(_, d)| matches!(d.kind, K::Smooth(..)))
            .count(),
        2
    );
}

#[test]
fn panel_switch_to_g1() {
    let mut w = rig(true);
    let (se, _, _) = parts(&mut w);
    se.choose_blend_kind(BlendKind::G1);
    assert_eq!(se.blend.kind, BlendKind::G1);
    click(&mut w, v(9.5, 0.0)).unwrap();
    let c = click(&mut w, v(20.5, 5.0)).unwrap().unwrap();
    assert_eq!(spline_degree(sk(&w), c), 3, "G1: Grad 3");
    assert_eq!(
        sk(&w)
            .constraints()
            .filter(|(_, d)| matches!(d.kind, K::EndTangent(..)))
            .count(),
        2
    );
}

#[test]
fn second_click_on_first_deselects() {
    let mut w = rig(true);
    let before = fingerprint(sk(&w));
    let steps = undo_len(&mut w);
    click(&mut w, v(9.5, 0.0)).unwrap();
    assert_eq!(click(&mut w, v(5.0, 0.0)), Ok(None));
    assert!(parts(&mut w).0.blend.first.is_none(), "Wahl aufgehoben");
    assert_eq!(fingerprint(sk(&w)), before, "Skizze unverändert");
    assert_eq!(undo_len(&mut w), steps);
}

#[test]
fn hover_shows_preview_after_first() {
    let mut w = rig(true);
    assert_eq!(
        hover(&mut w, v(20.5, 5.0)),
        None,
        "ohne erste Wahl keine Vorschau"
    );
    assert!(
        parts(&mut w).0.blend.hover.is_some(),
        "aber das Ziel leuchtet"
    );
    click(&mut w, v(9.5, 0.0)).unwrap();
    let pl = hover(&mut w, v(20.5, 5.0)).expect("Vorschau");
    assert!(pl.len() >= 2);
    assert!(pl[0].distance(v(10.0, 0.0)) < 1e-6, "{:?}", pl[0]);
    assert!(pl[pl.len() - 1].distance(v(20.0, 5.0)) < 1e-6);
    // Auf leerer Fläche: kein Ziel, keine Vorschau.
    assert_eq!(hover(&mut w, v(15.0, 40.0)), None);
    assert!(parts(&mut w).0.blend.hover.is_none());
}

#[test]
fn leaving_the_canvas_clears_the_hover_ring_and_preview() {
    let mut w = rig(true);
    click(&mut w, v(9.5, 0.0)).unwrap();
    {
        let (se, sf, _) = parts(&mut w);
        se.update_hover(&sf.sketch, Some(v(20.5, 5.0)), R);
        assert!(se.blend.hover.is_some(), "Ring am Ziel");
        assert!(se.hover_preview.is_some(), "Vorschau da");
        // Der Zeiger verlässt die Zeichenfläche.
        se.update_hover(&sf.sketch, None, R);
        assert!(se.blend.hover.is_none(), "Ring klebt nicht");
        assert!(se.hover_preview.is_none(), "Vorschau weg");
    }
}

#[test]
fn hover_over_circle_gives_no_target() {
    let mut w = rig(true);
    {
        let (_, sf, _) = parts(&mut w);
        let m = sf.sketch.add_point(0.0, 20.0).unwrap();
        sf.sketch.add_circle(m, 5.0).unwrap();
    }
    assert_eq!(hover(&mut w, v(5.0, 20.0)), None);
    assert!(parts(&mut w).0.blend.hover.is_none(), "Kreis ist kein Ziel");
    click(&mut w, v(9.5, 0.0)).unwrap();
    assert_eq!(hover(&mut w, v(5.0, 20.0)), None);
    assert!(parts(&mut w).0.blend.hover.is_none());
    // Ein Klick auf den Kreis wählt nichts und meldet es.
    let err = click(&mut w, v(5.0, 20.0)).unwrap_err();
    assert_eq!(err, tr!("sketch-blend-closed"));
    assert_eq!(parts(&mut w).0.message_text(), Some(err));
}

/// Handtest 11a, Fund 4a (Ruling H1-1): In einer 3D-Skizze wählt das Werkzeug auch eine Linie auf
/// z = 0 nicht und meldet, dass 3D-Kurven keine Quelle sind.
#[test]
fn line_in_3d_sketch_is_refused_with_message() {
    let mut w = rig(true);
    {
        let (se, sf, _) = parts(&mut w);
        se.set_three_d(&mut sf.sketch, true);
        se.set_tool(&mut sf.sketch, Tool::BlendCurve);
    }
    let before = fingerprint(sk(&w));
    assert_eq!(hover(&mut w, v(9.5, 0.0)), None);
    assert!(parts(&mut w).0.blend.hover.is_none(), "keine Quelle");
    let err = click(&mut w, v(9.5, 0.0)).unwrap_err();
    assert_eq!(err, tr!("sketch-blend-three-d"));
    assert_eq!(parts(&mut w).0.message_text(), Some(err));
    assert!(parts(&mut w).0.blend.first.is_none(), "nichts gewählt");
    assert_eq!(fingerprint(sk(&w)), before);
}

#[test]
fn esc_leaves_nothing() {
    let mut w = rig(true);
    let before = fingerprint(sk(&w));
    let steps = undo_len(&mut w);
    click(&mut w, v(9.5, 0.0)).unwrap();
    {
        let (se, sf, _) = parts(&mut w);
        se.cancel(&mut sf.sketch);
        assert!(se.blend.first.is_none());
        assert!(se.blend.hover.is_none());
        assert_eq!(se.tool, Tool::Select, "Esc beendet das Werkzeug");
    }
    assert_eq!(fingerprint(sk(&w)), before);
    assert_eq!(undo_len(&mut w), steps);
    // Werkzeugwechsel mitten in der Wahl: dasselbe.
    let (se, sf, _) = parts(&mut w);
    se.set_tool(&mut sf.sketch, Tool::BlendCurve);
    se.pick_radius = R;
    click(&mut w, v(9.5, 0.0)).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.set_tool(&mut sf.sketch, Tool::Line);
    assert!(se.blend.first.is_none());
    assert_eq!(fingerprint(sk(&w)), before);
}

#[test]
fn unsolvable_shows_message() {
    let mut w = rig(false);
    {
        // Wie im Modelltest: ein Spline mit festen Punkten, dessen Anfang doppelt liegt, hat am
        // Ende keine Richtung; die Zeilen der Übergangskurve lassen sich nicht erfüllen.
        let (_, sf, _) = parts(&mut w);
        let s = &mut sf.sketch;
        let q: Vec<_> = [(0.0, 0.0), (-5.0, 0.0), (-10.0, 5.0), (-15.0, 5.0)]
            .iter()
            .map(|&(x, y)| s.add_point(x, y).unwrap())
            .collect();
        s.add_control_spline(&q, 3).unwrap();
        for (p, (x, y)) in q
            .iter()
            .zip([(0.0, 0.0), (0.0, 0.0), (-10.0, 5.0), (-15.0, 5.0)])
        {
            s.add_constraint(K::Fixed(*p, x, y)).unwrap();
        }
        let l0 = s.add_point(20.0, 5.0).unwrap();
        let l1 = s.add_point(30.0, 5.0).unwrap();
        s.add_line(l0, l1).unwrap();
        s.add_constraint(K::Fixed(l0, 20.0, 5.0)).unwrap();
        s.add_constraint(K::Fixed(l1, 30.0, 5.0)).unwrap();
        // Absichtlich ungelöst: Die Lagen des Splines behalten ihre Richtung am Anfang.
    }
    w.observe_history(false);
    let before = fingerprint(sk(&w));
    let steps = undo_len(&mut w);
    assert_eq!(click(&mut w, v(0.0, 0.0)), Ok(None));
    let err = click(&mut w, v(20.0, 5.0)).unwrap_err();
    assert_eq!(err, tr!("sketch-blend-unsolvable"));
    let (se, _, _) = parts(&mut w);
    assert_eq!(se.message_text(), Some(tr!("sketch-blend-unsolvable")));
    assert_eq!(fingerprint(sk(&w)), before, "nichts angelegt");
    assert_eq!(undo_len(&mut w), steps);
}

#[test]
fn projected_edge_is_a_source() {
    let mut w = rig(false);
    let edge = {
        let n = NurbsCurve::new(
            3,
            vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0],
            [
                (0.0, 0.0),
                (2.0, 5.0),
                (5.0, 7.0),
                (9.0, 6.0),
                (12.0, 3.0),
                (14.0, 0.0),
            ]
            .iter()
            .map(|&(x, y)| DVec3::new(x, y, 0.0))
            .collect(),
            vec![1.0; 6],
        )
        .unwrap();
        Curve::Nurbs(Arc::new(n))
    };
    let c = {
        let (_, sf, _) = parts(&mut w);
        sf.sketch
            .add_projected_curve(
                &edge,
                0.0,
                3.0,
                ProjectStyle {
                    construction: false,
                },
            )
            .unwrap()
    };
    assert!(matches!(
        sk(&w).curve(c).unwrap().kind,
        CurveKind::FixedSpline { .. }
    ));
    assert_eq!(click(&mut w, v(13.9, 0.1)), Ok(None), "Quelle angenommen");
    let first = parts(&mut w).0.blend.first.expect("erstes Ende gewählt");
    assert_eq!(first.curve, c);
    assert!(parts(&mut w).0.message_text().is_none());
}

#[test]
fn action_is_assignable_unbound_and_named_like_fusion() {
    assert_eq!(Action::SketchBlendCurve.id(), "skizze.uebergangskurve");
    assert!(Action::ALL.contains(&Action::SketchBlendCurve));
    assert!(
        Keymap::defaults()
            .bindings(Action::SketchBlendCurve)
            .is_empty()
    );
    assert_eq!(
        Action::SketchBlendCurve.sketch_tool(),
        Some(Tool::BlendCurve)
    );
    assert!(Tool::BlendCurve.is_modify());
    assert_eq!(
        with_lang(&german(), || t("action-sketch-blend-curve")),
        "Übergangskurve"
    );
    assert_eq!(
        with_lang(&english(), || t("action-sketch-blend-curve")),
        "Blend Curve"
    );
    assert_eq!(
        with_lang(&german(), || t("sketch-blend-kind-g1")),
        "Tangential"
    );
    assert_eq!(
        with_lang(&german(), || t("sketch-blend-kind-g2")),
        "Krümmung"
    );
    assert_eq!(
        with_lang(&english(), || t("sketch-blend-kind-g1")),
        "Tangent"
    );
    assert_eq!(
        with_lang(&english(), || t("sketch-blend-kind-g2")),
        "Curvature"
    );
    // Der Befehl wählt das Werkzeug.
    let mut w = rig(true);
    let (se, sf, _) = parts(&mut w);
    se.set_tool(&mut sf.sketch, Tool::Select);
    planora_app::commands::run_action(&mut w, Action::SketchBlendCurve);
    assert_eq!(parts(&mut w).0.tool, Tool::BlendCurve);
}
