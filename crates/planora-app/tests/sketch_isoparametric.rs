//! Skizzen 11b, Task 16: „Isoparametrische Kurve“ in der App (Spec 6, wie Fusion „Isoparametric
//! Curve“): Dialog am Knopf „Projizieren/Einbeziehen“, Fläche und Punkt per Klick, Richtung U, V
//! oder beide, Einzeln oder Verteilt; OK legt die Kurven als ein Schritt an, die Skizze wird zur
//! 3D-Skizze; Abbrechen hinterlässt nichts; eine verlorene Fläche lässt sich über die Flächenwahl
//! neu verknüpfen. Bei 1920 × 1080.

mod common;

use common::Screen;
use egui::{Event, Modifiers};
use planora_app::detach::DIALOG_KEYS;
use planora_app::isoparametric_dialog::{IsoDialog, IsoDirections, IsoField, IsoMode, iso_values};
use planora_app::keymap::Action;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, IncludePick, ProjectTarget, StandardPlane};
use planora_app::surface_project_dialog::{self as spd, DialogOutcome, SurfaceDialog};
use planora_feature::{
    BodyRef, CopySource, FeatureId, FeatureKind, IsoDirection, ProfileSel, SketchFeature,
    SketchPlane,
};
use planora_kernel::{BoolOp, SurfaceKind};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::model::{CurveKind, Sketch};

const W: f32 = 1920.0;
const H: f32 = 1080.0;

// ---- Hilfen ---------------------------------------------------------------------------------------

fn cylinder(s: &mut AppState, r: f64, h: f64, origin: DVec3) -> FeatureId {
    let id = s
        .document
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: r,
                height: h,
                origin,
            },
        )
        .unwrap();
    s.refresh();
    id
}

/// Kugelkappe: Kugel R 10 um den Ursprung ohne die untere Hälfte (Quader darunter abgezogen).
fn dome(s: &mut AppState) -> FeatureId {
    let k = s
        .document
        .add_feature(
            "Kugel",
            FeatureKind::Sphere {
                radius: 10.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let q = s
        .document
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(40.0, 40.0, 20.0),
                origin: DVec3::new(-20.0, -20.0, -20.0),
            },
        )
        .unwrap();
    let d = s
        .document
        .add_feature(
            "Kappe",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: k,
                    index: 0,
                },
                tool: BodyRef {
                    feature: q,
                    index: 0,
                },
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    s.refresh();
    d
}

/// Drehkörper aus einem Spline-Profil (Freiformfläche), wie in den Tests von Task 15.
fn spline_revolve(s: &mut AppState) -> FeatureId {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let ap = sk.add_point(0.0, -2.0).unwrap();
    let aq = sk.add_point(0.0, 8.0).unwrap();
    let axis = sk.add_line(ap, aq).unwrap();
    sk.set_construction(axis, true).unwrap();
    let p: Vec<_> = [
        (0.0, 0.0),
        (0.0, 5.0),
        (2.0, 5.0),
        (3.0, 3.0),
        (2.5, 1.0),
        (2.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| sk.add_point(x, y).unwrap())
    .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..6]).unwrap();
    sk.add_line(p[5], p[0]).unwrap();
    let sketch = s
        .document
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
                text_formulas: Default::default(),
            }),
        )
        .unwrap();
    let rev = s
        .document
        .add_feature(
            "Rotation",
            FeatureKind::Revolve {
                sketch,
                profiles: ProfileSel::Index(0),
                axis,
            },
        )
        .unwrap();
    s.refresh();
    rev
}

fn body_index(s: &AppState, feature: FeatureId) -> usize {
    s.bodies
        .iter()
        .position(|b| b.source.feature == feature)
        .expect("Körper sichtbar")
}

/// Die erste Fläche der Art `kind` des Körpers `body`.
fn face(s: &AppState, body: usize, kind: SurfaceKind) -> IncludePick {
    let (id, _) = s.bodies[body]
        .solid
        .faces
        .iter()
        .find(|(_, f)| f.surface.kind() == kind)
        .expect("Fläche");
    IncludePick::Face(body, id)
}

/// Neue Skizze auf `plane`, geöffnet und mit leerem Verlauf.
fn open_new(s: &mut AppState, plane: StandardPlane) -> FeatureId {
    let id = s.add_sketch(SketchPlane::Standard(plane)).unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    id
}

/// Wählt die Art „Isoparametrische Kurve“ am Knopf „Projizieren/Einbeziehen“; der Dialog öffnet.
fn choose(s: &mut AppState) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.choose_kind(sk, ToolKind::Isoparametric);
    spd::sync(s);
}

fn dialog(s: &mut AppState) -> &mut IsoDialog {
    match s.surface_dialog.as_mut() {
        Some(SurfaceDialog::Iso(d)) => d,
        other => panic!("Dialog „Isoparametrische Kurve“ erwartet: {other:?}"),
    }
}

/// Rechnet die Vorschau des offenen Dialogs neu (wie nach einer Änderung im Dialog).
fn refresh(s: &mut AppState) {
    let Some(SurfaceDialog::Iso(mut d)) = s.surface_dialog.take() else {
        panic!("Dialog");
    };
    d.refresh(s);
    s.surface_dialog = Some(SurfaceDialog::Iso(d));
}

/// Zylinder R 10 H 30 um Z, offene Skizze auf XY, Dialog offen; liefert Zustand und Körper.
fn cylinder_rig() -> (AppState, usize) {
    let mut s = AppState::new();
    let c = cylinder(&mut s, 10.0, 30.0, DVec3::ZERO);
    open_new(&mut s, StandardPlane::XY);
    let bi = body_index(&s, c);
    choose(&mut s);
    (s, bi)
}

fn draft(s: &AppState) -> &SketchFeature {
    s.sketch_draft.as_ref().unwrap()
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn curve_count(s: &AppState) -> usize {
    sk(s).curves().count()
}

fn key(k: egui::Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Ein Bild mit dem offenen Dialog und den Ereignissen; liefert das letzte Ergebnis.
fn frame(screen: &mut Screen, s: &mut AppState, events: Vec<Event>) -> Option<DialogOutcome> {
    let mut out = None;
    screen.frame(events, &mut |ctx| {
        if let Some(o) = spd::show_open(ctx, s) {
            out = Some(o);
        }
    });
    out
}

/// Die verschiedenen Herkünfte der Kopien des Entwurfs (je Gruppe eine).
fn groups(s: &AppState) -> Vec<CopySource> {
    let mut out: Vec<CopySource> = Vec::new();
    for c in &draft(s).copies {
        if !out.contains(&c.source) {
            out.push(c.source.clone());
        }
    }
    out
}

/// Kein `Fixed`/`Fixed3` außer an den Enden fester Splines.
fn assert_free(sk: &Sketch) {
    let mut spline_ends = Vec::new();
    for (_, c) in sk.curves() {
        if let CurveKind::FixedSpline { start, end, .. } = c.kind {
            spline_ends.push(start);
            spline_ends.push(end);
        }
    }
    for (_, c) in sk.constraints() {
        if let Some(p) = c.kind.fixed_point() {
            assert!(
                spline_ends.contains(&p),
                "Fixierung außerhalb eines festen Splines: {:?}",
                c.kind
            );
        }
    }
}

// ---- Tests ----------------------------------------------------------------------------------------

#[test]
fn iso_values_distributed_without_edges() {
    // Offene Richtung: k / (count + 1), ohne die Ränder.
    let v = iso_values(IsoMode::Distributed, 0.9, 3, false);
    assert_eq!(v.len(), 3);
    for (got, want) in v.iter().zip([0.25, 0.5, 0.75]) {
        assert!((got - want).abs() < 1e-12, "{v:?}");
    }
    // Geschlossene Richtung (M4): (k + 0,5) / count, gleiche Abstände, keine auf der Naht.
    let v = iso_values(IsoMode::Distributed, 0.9, 4, true);
    assert_eq!(v.len(), 4);
    for (got, want) in v.iter().zip([0.125, 0.375, 0.625, 0.875]) {
        assert!((got - want).abs() < 1e-12, "{v:?}");
    }
    assert_eq!(iso_values(IsoMode::Single, 0.3, 7, false), vec![0.3]);
    assert_eq!(iso_values(IsoMode::Single, 0.3, 7, true), vec![0.3]);
    // Grenzen: Entfernung 0–1, Anzahl 1–50.
    assert_eq!(iso_values(IsoMode::Single, 1.7, 1, false), vec![1.0]);
    assert_eq!(iso_values(IsoMode::Single, -0.2, 1, false), vec![0.0]);
    assert_eq!(iso_values(IsoMode::Distributed, 0.5, 0, false), vec![0.5]);
    assert_eq!(iso_values(IsoMode::Distributed, 0.5, 0, true), vec![0.5]);
    assert_eq!(iso_values(IsoMode::Distributed, 0.5, 80, false).len(), 50);
}

#[test]
fn u_and_v_on_dome_one_step() {
    let mut s = AppState::new();
    let d = dome(&mut s);
    open_new(&mut s, StandardPlane::XY);
    let bi = body_index(&s, d);
    choose(&mut s);
    let f = face(&s, bi, SurfaceKind::Sphere);
    assert!(spd::deliver(&mut s, f));
    {
        let d = dialog(&mut s);
        assert!(d.face.is_some());
        d.directions = IsoDirections::Both;
        d.mode = IsoMode::Distributed;
        d.count = 3;
    }
    refresh(&mut s);
    {
        let d = dialog(&mut s);
        assert!(d.ok_enabled(), "{:?}", d.error());
        assert!(
            !d.preview_lines().is_empty(),
            "die Vorschau zeichnet die Kurven"
        );
    }
    assert!(!sk(&s).three_d);
    let n = undo_len(&s);
    let mut screen = Screen::new(W, H);
    let seen = screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    for label in [
        "Isoparametrische Kurve",
        "Fläche",
        "Punkt",
        "Richtung",
        "Einzeln",
        "Verteilt",
        "Anzahl",
        "Projektionsverknüpfung",
    ] {
        assert!(common::find(&seen, label).is_some(), "„{label}“ fehlt");
    }
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    assert_eq!(out, Some(DialogOutcome::Applied));
    assert!(s.surface_dialog.is_none(), "der Dialog ist zu");
    assert_eq!(s.sketch_session.as_ref().unwrap().tool, Tool::Select);
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n + 1, "ein OK, ein Schritt");
    assert_eq!(
        s.sketch_undo_label().as_deref(),
        Some("Isoparametrische Kurve")
    );
    assert!(sk(&s).three_d, "die Skizze ist eine 3D-Skizze");
    let g = groups(&s);
    assert_eq!(g.len(), 6, "je Richtung und Wert eine Gruppe: {g:?}");
    for dir in [IsoDirection::U, IsoDirection::V] {
        let mut values: Vec<f64> = g
            .iter()
            .filter_map(|c| match c {
                CopySource::Isoparametric {
                    direction, value, ..
                } if *direction == dir => Some(*value),
                _ => None,
            })
            .collect();
        values.sort_by(f64::total_cmp);
        assert_eq!(values.len(), 3, "{dir:?}: {values:?}");
        // U läuft auf der Kappe ganz herum (geschlossen, M4), V vom Äquator zum Pol (offen).
        let want = match dir {
            IsoDirection::U => [1.0 / 6.0, 0.5, 5.0 / 6.0],
            IsoDirection::V => [0.25, 0.5, 0.75],
        };
        for (got, want) in values.iter().zip(want) {
            assert!((got - want).abs() < 1e-12, "{dir:?}: {values:?}");
        }
    }
    assert_eq!(draft(&s).copies.len(), curve_count(&s), "alles verknüpft");
    // Die Kurven liegen auf der Kugel.
    for (c, _) in sk(&s).curves() {
        let (k, t0, t1) = sk(&s).curve_world(c).unwrap();
        for i in 0..=4 {
            let p = k.point(t0 + (t1 - t0) * f64::from(i) / 4.0);
            assert!((p.length() - 10.0).abs() < 1e-6, "{p:?}");
            assert!(p.z >= -1e-6, "{p:?}");
        }
    }
    // Rückgängig nimmt alles, auch den Haken „3D-Skizze“.
    assert!(s.undo_sketch());
    assert_eq!(curve_count(&s), 0);
    assert!(draft(&s).copies.is_empty());
    assert!(!sk(&s).three_d);
}

#[test]
fn point_sets_distance() {
    let (mut s, bi) = cylinder_rig();
    let f = face(&s, bi, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut s, f.clone()));
    let IncludePick::Face(body, id) = f else {
        unreachable!()
    };
    {
        let d = dialog(&mut s);
        assert_eq!(d.focus, IsoField::Point, "nach der Fläche kommt der Punkt");
        d.directions = IsoDirections::V;
    }
    // Ein Punkt auf der Mantelfläche in Höhe 9 (von 30): Anteil 0,3 in v.
    let at = DVec3::new(10.0 * 0.6, 10.0 * 0.8, 9.0);
    assert!(planora_app::isoparametric_dialog::deliver_point(
        &mut s, body, id, at
    ));
    {
        let d = dialog(&mut s);
        assert!((d.distance - 0.3).abs() < 1e-6, "{}", d.distance);
        assert_eq!(d.mode, IsoMode::Single);
        assert!(d.ok_enabled(), "{:?}", d.error());
    }
    // Ein Punkt auf einer anderen Fläche wechselt die Fläche und setzt den Punkt (M5, wie
    // Fusion); zurück auf den Mantel ebenso.
    let mantle = dialog(&mut s).face.clone();
    let other = face(&s, bi, SurfaceKind::Plane);
    let IncludePick::Face(_, top) = other else {
        unreachable!()
    };
    assert!(planora_app::isoparametric_dialog::deliver_point(
        &mut s,
        body,
        top,
        DVec3::new(0.0, 0.0, 30.0)
    ));
    {
        let d = dialog(&mut s);
        assert_ne!(d.face, mantle, "Fläche gewechselt");
        assert!(d.error().is_none());
    }
    assert!(planora_app::isoparametric_dialog::deliver_point(
        &mut s, body, id, at
    ));
    {
        let d = dialog(&mut s);
        assert_eq!(d.face, mantle);
        assert!((d.distance - 0.3).abs() < 1e-6);
    }
    // Ein Klick auf etwas, das keine Fläche vor der Skizze ist, wird abgelehnt: Die Lage bleibt,
    // OK bleibt frei.
    assert!(!planora_app::isoparametric_dialog::deliver_point(
        &mut s,
        99,
        top,
        DVec3::new(0.0, 0.0, 30.0)
    ));
    {
        let d = dialog(&mut s);
        assert!((d.distance - 0.3).abs() < 1e-6);
        assert!(d.error().is_some(), "Hinweis");
        assert!(d.ok_enabled(), "ein abgelehnter Klick sperrt OK nicht");
    }
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    // Ein Kreis R 10 in Höhe 9.
    let (c, _) = sk(&s).curves().next().expect("Kurve");
    let (k, t0, t1) = sk(&s).curve_world(c).unwrap();
    for i in 0..=3 {
        let p = k.point(t0 + (t1 - t0) * f64::from(i) / 3.0);
        assert!((p.z - 9.0).abs() < 1e-6, "{p:?}");
        assert!((p.truncate().length() - 10.0).abs() < 1e-6, "{p:?}");
    }
}

#[test]
fn unlinked_gives_free_curves() {
    // Zylinder, U und V verteilt: Linien und Kreise, ohne Verknüpfung.
    let (mut s, bi) = cylinder_rig();
    let f = face(&s, bi, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut s, f));
    {
        let d = dialog(&mut s);
        d.directions = IsoDirections::Both;
        d.mode = IsoMode::Distributed;
        d.count = 2;
        d.linked = false;
    }
    refresh(&mut s);
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    assert_eq!(curve_count(&s), 4);
    assert!(draft(&s).copies.is_empty(), "keine Herkunft");
    assert_free(sk(&s));
    assert!(sk(&s).three_d);

    // Freiformfläche: feste Splines behalten ihre End-Fixierungen, sonst nichts fest.
    let mut s = AppState::new();
    let rev = spline_revolve(&mut s);
    open_new(&mut s, StandardPlane::XZ);
    let bi = body_index(&s, rev);
    choose(&mut s);
    let f = face(&s, bi, SurfaceKind::Nurbs);
    assert!(spd::deliver(&mut s, f));
    {
        let d = dialog(&mut s);
        d.directions = IsoDirections::Both;
        d.mode = IsoMode::Distributed;
        d.count = 2;
        d.linked = false;
    }
    refresh(&mut s);
    assert!(dialog(&mut s).ok_enabled(), "{:?}", dialog(&mut s).error());
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    assert!(draft(&s).copies.is_empty());
    assert!(
        sk(&s)
            .curves()
            .any(|(_, c)| matches!(c.kind, CurveKind::FixedSpline { .. })),
        "ein fester Spline"
    );
    assert_free(sk(&s));

    // Mit Verknüpfung dagegen fixiert.
    let (mut t, bi) = cylinder_rig();
    let f = face(&t, bi, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut t, f));
    dialog(&mut t).directions = IsoDirections::U;
    refresh(&mut t);
    let mut screen = Screen::new(W, H);
    frame(&mut screen, &mut t, vec![key(egui::Key::Enter)]);
    assert!(
        sk(&t)
            .constraints()
            .any(|(_, c)| c.kind.fixed_point().is_some())
    );
}

#[test]
fn cancel_leaves_nothing() {
    let (mut s, bi) = cylinder_rig();
    let f = face(&s, bi, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut s, f));
    assert!(dialog(&mut s).ok_enabled());
    let (steps, curves) = (undo_len(&s), curve_count(&s));
    let mut screen = Screen::new(W, H);
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Escape)]);
    assert_eq!(out, Some(DialogOutcome::Cancelled));
    assert!(s.surface_dialog.is_none());
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps);
    assert_eq!(curve_count(&s), curves);
    assert!(draft(&s).copies.is_empty());
    assert!(!sk(&s).three_d);
    assert!(s.dialog_marks.is_empty(), "Markierungen weg");
}

#[test]
fn ok_needs_a_face() {
    let (mut s, _) = cylinder_rig();
    {
        let d = dialog(&mut s);
        assert!(d.face.is_none());
        assert!(!d.ok_enabled());
    }
    let steps = undo_len(&s);
    let mut screen = Screen::new(W, H);
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    assert_eq!(out, Some(DialogOutcome::Open), "Enter tut nichts");
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps);
    // Eine Achse ist keine Fläche: Hinweis, nichts gewählt.
    assert!(!spd::deliver(
        &mut s,
        IncludePick::Axis(planora_feature::AxisId::Z)
    ));
    let d = dialog(&mut s);
    assert!(d.face.is_none());
    assert!(d.error().is_some());
}

#[test]
fn relink_to_other_face() {
    let mut s = AppState::new();
    let a = cylinder(&mut s, 10.0, 30.0, DVec3::ZERO);
    let b = cylinder(&mut s, 5.0, 20.0, DVec3::new(40.0, 0.0, 0.0));
    let sketch = open_new(&mut s, StandardPlane::XY);
    choose(&mut s);
    let ai = body_index(&s, a);
    let f = face(&s, ai, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut s, f));
    dialog(&mut s).directions = IsoDirections::V;
    refresh(&mut s);
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    s.close_sketch();
    // Zylinder A verschwindet: die Isokurve verliert ihre Fläche (Warnung, die Kurve bleibt).
    s.delete_feature(a).unwrap();
    let lost = s.document.feature(sketch).unwrap().lost_projections();
    assert_eq!(lost.len(), 1, "eine verlorene Kurve");
    let target = lost[0];
    assert!(s.open_lost_projections(sketch));
    let rows = s.lost_dialog.clone().unwrap().rows(&s);
    assert!(rows.iter().all(|r| r.relinkable), "Neu verknüpfen geht");
    s.start_relink(sketch, target).unwrap();
    // Neu verknüpfen auf die Mantelfläche von B; Richtung und Wert bleiben.
    let bi = body_index(&s, b);
    let IncludePick::Face(_, fb) = face(&s, bi, SurfaceKind::Cylinder) else {
        unreachable!()
    };
    let steps = undo_len(&s);
    s.relink_pick(ProjectTarget::Face(bi, fb)).unwrap();
    assert!(s.sketch_session.as_ref().unwrap().relink.is_none());
    assert_eq!(undo_len(&s), steps + 1, "ein Schritt");
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Neu verknüpfen"));
    match &draft(&s).copies[0].source {
        CopySource::Isoparametric {
            direction, value, ..
        } => {
            assert_eq!(*direction, IsoDirection::V);
            assert!((value - 0.5).abs() < 1e-12);
        }
        other => panic!("Isokurve erwartet: {other:?}"),
    }
    // Die Kurve folgt: Kreis R 5 um (40, 0) in Höhe 10.
    let (c, _) = sk(&s).curves().next().unwrap();
    let (k, t0, t1) = sk(&s).curve_world(c).unwrap();
    for i in 0..=3 {
        let p = k.point(t0 + (t1 - t0) * f64::from(i) / 3.0);
        assert!((p.z - 10.0).abs() < 1e-6, "{p:?}");
        assert!(
            ((p - DVec3::new(40.0, 0.0, p.z)).length() - 5.0).abs() < 1e-6,
            "{p:?}"
        );
    }
    s.close_sketch();
    assert!(
        s.document
            .feature(sketch)
            .unwrap()
            .lost_projections()
            .is_empty(),
        "Warnung weg"
    );
}

#[test]
fn dialog_has_dock_key() {
    assert!(DIALOG_KEYS.contains(&"sketch_isoparametric"));
    let d = IsoDialog::default();
    assert_eq!(d.directions, IsoDirections::U);
    assert_eq!(d.mode, IsoMode::Single);
    assert!((d.distance - 0.5).abs() < 1e-12);
    assert_eq!(d.count, 3);
    assert!(d.linked && d.face.is_none());
    assert_eq!(d.focus, IsoField::Face);
    // Taste und Name wie Fusion; ohne Vorbelegung, belegbar.
    assert_eq!(
        ToolKind::Isoparametric.action(),
        Action::SketchIsoparametric
    );
    assert_eq!(Action::SketchIsoparametric.id(), "skizze.isokurve");
    assert!(Action::SketchIsoparametric.default_bindings().is_empty());
    assert_eq!(ToolKind::Isoparametric.label(), "Isoparametrische Kurve");
    assert_eq!(ToolKind::Isoparametric.tool(), Tool::ProjectEdge);
    assert!(ToolKind::of(Tool::ProjectEdge).contains(&ToolKind::Isoparametric));
    let en = planora_i18n::with_lang(&planora_i18n::english(), || {
        (
            ToolKind::Isoparametric.label(),
            planora_i18n::tr!("iso-mode-single"),
            planora_i18n::tr!("iso-mode-distributed"),
            planora_i18n::tr!("iso-distance"),
            planora_i18n::tr!("iso-count"),
        )
    });
    assert_eq!(
        en,
        (
            "Isoparametric Curve".to_string(),
            "Single".to_string(),
            "Distributed".to_string(),
            "Distance".to_string(),
            "Count".to_string()
        )
    );
}

/// Abstand des Weltpunkts `p` von der Kurve `c` der Skizze (dichte Abtastung; reicht für Linie
/// und Kreis).
fn distance_to_curve(sk: &Sketch, c: planora_sketch::model::CurveId, p: DVec3) -> f64 {
    let (k, t0, t1) = sk.curve_world(c).unwrap();
    let n = 20_000;
    (0..=n)
        .map(|i| {
            k.point(t0 + (t1 - t0) * f64::from(i) / f64::from(n))
                .distance(p)
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn both_directions_pass_through_the_point() {
    let (mut s, bi) = cylinder_rig();
    let f = face(&s, bi, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut s, f.clone()));
    let IncludePick::Face(body, id) = f else {
        unreachable!()
    };
    dialog(&mut s).directions = IsoDirections::Both;
    let a = 0.9_f64;
    let at = DVec3::new(10.0 * a.cos(), 10.0 * a.sin(), 9.0);
    assert!(planora_app::isoparametric_dialog::deliver_point(
        &mut s, body, id, at
    ));
    assert!(dialog(&mut s).ok_enabled());
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    let curves: Vec<_> = sk(&s).curves().map(|(c, _)| c).collect();
    assert_eq!(curves.len(), 2, "eine U-Linie und ein V-Kreis");
    assert!(
        sk(&s)
            .curves()
            .any(|(_, c)| matches!(c.kind, CurveKind::Line { .. }))
    );
    for c in curves {
        let d = distance_to_curve(sk(&s), c, at);
        assert!(d < 1e-3, "die Kurve geht durch den Punkt: Abstand {d}");
    }
    // Der v-Anteil kommt aus der Höhe des Punkts (9 von 30).
    let v_share = draft(&s).copies.iter().find_map(|c| match c.source {
        CopySource::Isoparametric {
            direction: IsoDirection::V,
            value,
            ..
        } => Some(value),
        _ => None,
    });
    assert!((v_share.unwrap() - 0.3).abs() < 1e-6);
}

#[test]
fn direction_change_recomputes_share() {
    let (mut s, bi) = cylinder_rig();
    let f = face(&s, bi, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut s, f.clone()));
    let IncludePick::Face(body, id) = f else {
        unreachable!()
    };
    let at = DVec3::new(10.0 * 0.6, 10.0 * 0.8, 9.0);
    assert!(planora_app::isoparametric_dialog::deliver_point(
        &mut s, body, id, at
    ));
    let u_share = dialog(&mut s).distance;
    assert!(
        (u_share - 0.3).abs() > 1e-3,
        "u-Anteil ist nicht der v-Anteil: {u_share}"
    );
    dialog(&mut s).directions = IsoDirections::V;
    refresh(&mut s);
    let v = dialog(&mut s).distance;
    assert!((v - 0.3).abs() < 1e-6, "{v}");
    dialog(&mut s).directions = IsoDirections::U;
    refresh(&mut s);
    assert!((dialog(&mut s).distance - u_share).abs() < 1e-9);
}

#[test]
fn distributed_closed_and_open_directions() {
    // Zylinder: U (um den Umfang, geschlossen) mit 4 gleichen Abständen, keine auf der Naht;
    // V (über die Höhe, offen) ohne die Ränder.
    let (mut s, bi) = cylinder_rig();
    let f = face(&s, bi, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut s, f));
    {
        let d = dialog(&mut s);
        d.directions = IsoDirections::Both;
        d.mode = IsoMode::Distributed;
        d.count = 4;
    }
    refresh(&mut s);
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    let mut u = Vec::new();
    let mut v = Vec::new();
    for g in groups(&s) {
        if let CopySource::Isoparametric {
            direction, value, ..
        } = g
        {
            match direction {
                IsoDirection::U => u.push(value),
                IsoDirection::V => v.push(value),
            }
        }
    }
    u.sort_by(f64::total_cmp);
    v.sort_by(f64::total_cmp);
    assert_eq!(u, vec![0.125, 0.375, 0.625, 0.875]);
    assert_eq!(v, vec![0.2, 0.4, 0.6, 0.8]);
    // Die U-Linien stehen im Abstand 90°.
    let mut angles: Vec<f64> = sk(&s)
        .curves()
        .filter(|(_, c)| matches!(c.kind, CurveKind::Line { .. }))
        .map(|(c, _)| {
            let (k, t0, _) = sk(&s).curve_world(c).unwrap();
            let p = k.point(t0);
            p.y.atan2(p.x).rem_euclid(std::f64::consts::TAU)
        })
        .collect();
    angles.sort_by(f64::total_cmp);
    assert_eq!(angles.len(), 4);
    for w in angles.windows(2) {
        assert!(
            (w[1] - w[0] - std::f64::consts::FRAC_PI_2).abs() < 1e-6,
            "{angles:?}"
        );
    }
}

#[test]
fn dome_u_distributed_unlinked_meets_at_the_pole() {
    use planora_sketch::constraint::ConstraintKind;
    let mut s = AppState::new();
    let d = dome(&mut s);
    open_new(&mut s, StandardPlane::XY);
    let bi = body_index(&s, d);
    choose(&mut s);
    let f = face(&s, bi, SurfaceKind::Sphere);
    assert!(spd::deliver(&mut s, f));
    {
        let d = dialog(&mut s);
        d.directions = IsoDirections::U;
        d.mode = IsoMode::Distributed;
        d.count = 4;
        d.linked = false;
    }
    refresh(&mut s);
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    assert_eq!(curve_count(&s), 4, "vier Meridiane");
    assert!(draft(&s).copies.is_empty());
    assert!(
        !sk(&s)
            .constraints()
            .any(|(_, c)| c.kind.fixed_point().is_some()),
        "keine Fixierung"
    );
    // Am Pol hängen die vier Enden zusammen.
    let pole = DVec3::new(0.0, 0.0, 10.0);
    let near = |p| sk(&s).point_xyz(p).is_ok_and(|q| q.distance(pole) < 1e-6);
    let joins = sk(&s)
        .constraints()
        .filter(|(_, c)| matches!(c.kind, ConstraintKind::Coincident(a, b) if near(a) && near(b)))
        .count();
    assert_eq!(joins, 3, "drei Coincident binden die vier Pol-Enden");
}

#[test]
fn relink_to_face_without_hit_keeps_source() {
    // Isokurve V beim Anteil 1 (oberer Rand des Zylinders); als neue Fläche eine Kugel: dort ist
    // v = 1 der Pol, kein Stück.
    let mut s = AppState::new();
    let a = cylinder(&mut s, 10.0, 30.0, DVec3::ZERO);
    let k = s
        .document
        .add_feature(
            "Kugel",
            FeatureKind::Sphere {
                radius: 5.0,
                origin: DVec3::new(40.0, 0.0, 0.0),
            },
        )
        .unwrap();
    s.refresh();
    let sketch = open_new(&mut s, StandardPlane::XY);
    choose(&mut s);
    let ai = body_index(&s, a);
    let f = face(&s, ai, SurfaceKind::Cylinder);
    assert!(spd::deliver(&mut s, f));
    {
        let d = dialog(&mut s);
        d.directions = IsoDirections::V;
        d.distance = 1.0;
    }
    refresh(&mut s);
    assert!(dialog(&mut s).ok_enabled(), "{:?}", dialog(&mut s).error());
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    let target = draft(&s).copies[0].target;
    let before = draft(&s).copies[0].source.clone();
    s.start_relink(sketch, target).unwrap();
    let ki = body_index(&s, k);
    let IncludePick::Face(_, fk) = face(&s, ki, SurfaceKind::Sphere) else {
        unreachable!()
    };
    let steps = undo_len(&s);
    assert!(s.relink_pick(ProjectTarget::Face(ki, fk)).is_err());
    let message = s
        .sketch_session
        .as_ref()
        .unwrap()
        .message
        .as_ref()
        .map(|m| m.text());
    assert_eq!(message, Some(planora_i18n::tr!("lost-relink-failed")));
    assert_eq!(draft(&s).copies[0].source, before, "die Herkunft bleibt");
    assert_eq!(undo_len(&s), steps, "kein Schritt");
    assert!(
        s.sketch_session.as_ref().unwrap().relink.is_some(),
        "die Wahl bleibt"
    );
}

#[test]
fn single_at_zero_and_one() {
    // Offene Richtung (V am Zylinder): 0 und 1 sind die Randkreise unten und oben.
    for (share, z) in [(0.0, 0.0), (1.0, 30.0)] {
        let (mut s, bi) = cylinder_rig();
        let f = face(&s, bi, SurfaceKind::Cylinder);
        assert!(spd::deliver(&mut s, f));
        {
            let d = dialog(&mut s);
            d.directions = IsoDirections::V;
            d.distance = share;
        }
        refresh(&mut s);
        assert!(dialog(&mut s).ok_enabled(), "{:?}", dialog(&mut s).error());
        let mut screen = Screen::new(W, H);
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
        let (c, _) = sk(&s).curves().next().expect("Kurve");
        let (k, t0, _) = sk(&s).curve_world(c).unwrap();
        assert!((k.point(t0).z - z).abs() < 1e-6);
    }
    // Offene Richtung ohne Stück (V an der Kugel: 0 und 1 sind die Pole): OK gesperrt.
    for share in [0.0, 1.0] {
        let mut s = AppState::new();
        let k = s
            .document
            .add_feature(
                "Kugel",
                FeatureKind::Sphere {
                    radius: 5.0,
                    origin: DVec3::ZERO,
                },
            )
            .unwrap();
        s.refresh();
        open_new(&mut s, StandardPlane::XY);
        let ki = body_index(&s, k);
        choose(&mut s);
        let f = face(&s, ki, SurfaceKind::Sphere);
        assert!(spd::deliver(&mut s, f));
        {
            let d = dialog(&mut s);
            d.directions = IsoDirections::V;
            d.distance = share;
        }
        refresh(&mut s);
        let d = dialog(&mut s);
        assert!(!d.ok_enabled(), "Pol: kein Stück");
        assert_eq!(d.error(), Some(planora_i18n::tr!("iso-no-hit")));
    }
    // Geschlossene Richtung (U am Zylinder): 0 und 1 sind beide die Linie auf der Naht.
    let mut at = Vec::new();
    for share in [0.0, 1.0] {
        let (mut s, bi) = cylinder_rig();
        let f = face(&s, bi, SurfaceKind::Cylinder);
        assert!(spd::deliver(&mut s, f));
        dialog(&mut s).distance = share;
        refresh(&mut s);
        assert!(dialog(&mut s).ok_enabled(), "{:?}", dialog(&mut s).error());
        let mut screen = Screen::new(W, H);
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
        assert_eq!(curve_count(&s), 1);
        let (c, _) = sk(&s).curves().next().unwrap();
        let (k, t0, _) = sk(&s).curve_world(c).unwrap();
        let p = k.point(t0);
        at.push(DVec3::new(p.x, p.y, 0.0));
    }
    assert!(at[0].distance(at[1]) < 1e-6, "beide auf der Naht: {at:?}");
}
