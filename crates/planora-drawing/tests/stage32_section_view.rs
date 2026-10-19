//! Stufe 32: Schnittansicht rechnen (`compute_section`).

use planora_drawing::section::*;
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{
    DrawBody, DrawError, ViewKind, ViewLines, Visibility, compute_section, compute_view,
};
use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, BoolOp, Curve, Kernel, Solid};
use planora_math::{DMat4, DVec3};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, x: f64, y: f64, z: f64) -> Solid {
    s.transformed(&DMat4::from_translation(DVec3::new(x, y, z)))
        .unwrap()
}

fn diff(a: &Solid, b: &Solid) -> Solid {
    let mut s = k().boolean(a, b, BoolOp::Difference).unwrap();
    s.assign_origin(7);
    s
}

fn boxs(a: f64, b: f64, c: f64) -> Solid {
    let mut s = k().make_box(DVec3::new(a, b, c)).unwrap();
    s.assign_origin(7);
    s
}

fn tube() -> Solid {
    let outer = k().make_cylinder(10.0, 20.0).unwrap();
    let inner = moved(&k().make_cylinder(6.0, 24.0).unwrap(), 0.0, 0.0, -2.0);
    diff(&outer, &inner)
}

fn bored_box() -> Solid {
    let drill = moved(&k().make_cylinder(4.0, 14.0).unwrap(), 15.0, 10.0, -2.0);
    diff(&boxs(30.0, 20.0, 10.0), &drill)
}

fn stepped() -> Solid {
    let lower = k().make_box(DVec3::new(20.0, 10.0, 5.0)).unwrap();
    let upper = moved(
        &k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap(),
        0.0,
        0.0,
        5.0,
    );
    let mut s = k().boolean(&lower, &upper, BoolOp::Union).unwrap();
    s.assign_origin(7);
    s
}

fn db<'a>(name: &str, solid: &'a Solid) -> DrawBody<'a> {
    DrawBody {
        name: name.into(),
        solid,
    }
}

fn plane(kind: ViewKind, offset: f64) -> SectionPlane {
    SectionPlane { kind, offset }
}

fn run(bodies: &[DrawBody], p: &SectionPlane) -> Result<SectionLines, DrawError> {
    compute_section(bodies, p, compute_view, &Progress::new())
}

/// Radius, wenn der Strich ein Kreis in der Höhe `z` ist.
fn circle_at(v: &ViewLines, s: usize, z: f64) -> Option<f64> {
    match &v.strokes[s].curve {
        Curve::Circle { frame, radius, .. } if (frame.origin.z - z).abs() < 1e-9 => {
            Some(radius.abs())
        }
        _ => None,
    }
}

fn body_of(o: &StrokeOrigin) -> usize {
    match o {
        StrokeOrigin::Edge { body, .. } | StrokeOrigin::Silhouette { body, .. } => *body,
    }
}

#[test]
fn tube_top_section() {
    let t = tube();
    let r = run(&[db("Rohr", &t)], &plane(ViewKind::Top, 10.0)).unwrap();
    assert_eq!(r.lines.kind, ViewKind::Top);
    let mut radii: Vec<f64> = r
        .lines
        .pieces
        .iter()
        .filter(|p| p.vis == Visibility::Visible)
        .filter_map(|p| circle_at(&r.lines, p.stroke, 10.0))
        .collect();
    radii.sort_by(f64::total_cmp);
    radii.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert_eq!(radii.len(), 2, "{radii:?}");
    assert!((radii[0] - 6.0).abs() < 1e-9 && (radii[1] - 10.0).abs() < 1e-9);
    assert_eq!(r.data.regions.len(), 1);
    assert_eq!(r.data.regions[0].loops.len(), 2);
    assert_eq!(r.data.source, vec![0]);
    assert!(!r.data.nudged);
}

#[test]
fn misses() {
    let t = tube();
    for off in [30.0, -5.0] {
        let e = run(&[db("Rohr", &t)], &plane(ViewKind::Top, off))
            .err()
            .unwrap();
        assert!(matches!(e, DrawError::SectionMisses), "{off}: {e:?}");
        assert_eq!(e.to_string(), "Die Schnittebene trifft den Körper nicht.");
    }
}

#[test]
fn one_behind_one_cut() {
    // Quader ganz hinter der Ebene (y 12..20 bei Ebene y = 0), Rohr (Achse z) geschnitten.
    let b = moved(&boxs(5.0, 8.0, 5.0), 30.0, 12.0, 0.0);
    let t = tube();
    let r = run(
        &[db("Quader", &b), db("Rohr", &t)],
        &plane(ViewKind::Front, 0.0),
    )
    .unwrap();
    assert_eq!(r.data.source, vec![0, 1]);
    assert!(!r.data.regions.is_empty());
    assert!(r.data.regions.iter().all(|g| g.body == 1));
    let box_strokes = r
        .lines
        .strokes
        .iter()
        .filter(|s| body_of(&s.origin) == 0)
        .count();
    assert_eq!(box_strokes, 8); // vier Kanten längs y sind in Vorne Punkte
    let (lo, hi) = r.lines.bbox;
    assert!(hi.0 >= 35.0 - 1e-9 && lo.0 <= -10.0 + 1e-6, "{lo:?} {hi:?}");
}

#[test]
fn removed_bodies_drop_out() {
    // Körper 0 ganz vor der Ebene (y -24..-20, Ebene y = 0), Körper 1 geschnitten.
    let front = moved(&boxs(5.0, 4.0, 5.0), 30.0, -24.0, 0.0);
    let t = tube();
    let r = run(
        &[db("Vorn", &front), db("Rohr", &t)],
        &plane(ViewKind::Front, 0.0),
    )
    .unwrap();
    assert_eq!(r.data.source, vec![1]);
    assert_eq!(r.data.solids.len(), 1);
    assert!(!r.lines.strokes.is_empty());
    assert!(r.lines.strokes.iter().all(|s| body_of(&s.origin) == 0));
    assert!(r.data.regions.iter().all(|g| g.body == 1));
}

#[test]
fn kernel_failure_names_the_body() {
    let ok = boxs(10.0, 10.0, 10.0);
    let mut bad = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    bad.assign_origin(u64::MAX - 3);
    let e = run(
        &[db("Körper 1", &ok), db("Körper 2", &bad)],
        &plane(ViewKind::Front, 5.0),
    )
    .err()
    .unwrap();
    match &e {
        DrawError::SectionFailed(n) => assert_eq!(n, "Körper 2"),
        o => panic!("{o:?}"),
    }
    assert_eq!(
        e.to_string(),
        "Bei „Körper 2“ lässt sich der Schnitt nicht berechnen."
    );
}

fn too_complex(_: &[DrawBody], _: ViewKind, _: &Progress) -> Result<ViewLines, DrawError> {
    Err(DrawError::TooComplex(5))
}

fn cancelling(b: &[DrawBody], kd: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    p.cancel();
    compute_view(b, kd, p)
}

#[test]
fn view_fn_error_passes() {
    let t = tube();
    let e = compute_section(
        &[db("Rohr", &t)],
        &plane(ViewKind::Top, 10.0),
        too_complex,
        &Progress::new(),
    )
    .err()
    .unwrap();
    assert!(matches!(e, DrawError::TooComplex(5)), "{e:?}");
}

#[test]
fn cancel() {
    let t = tube();
    let p = Progress::new();
    p.cancel();
    let e = compute_section(
        &[db("Rohr", &t)],
        &plane(ViewKind::Top, 10.0),
        compute_view,
        &p,
    )
    .err()
    .unwrap();
    assert!(matches!(e, DrawError::Cancelled), "{e:?}");
    // Der Abbruch wird je Körper geprüft: sonst käme für diesen Körper die Fehlermeldung.
    let mut bad = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    bad.assign_origin(u64::MAX - 3);
    let e = compute_section(
        &[db("Schlecht", &bad)],
        &plane(ViewKind::Front, 5.0),
        compute_view,
        &p,
    )
    .err()
    .unwrap();
    assert!(matches!(e, DrawError::Cancelled), "{e:?}");
    // Abbruch erst in der Ansichtsrechnung kommt unverändert zurück.
    let e = compute_section(
        &[db("Rohr", &t)],
        &plane(ViewKind::Top, 10.0),
        cancelling,
        &Progress::new(),
    )
    .err()
    .unwrap();
    assert!(matches!(e, DrawError::Cancelled), "{e:?}");
}

/// Sichtbarkeit der Stücke senkrechter Linien bei x = 11 oder 19 (Bohrungskanten).
fn bore_pieces(v: &ViewLines) -> Vec<Visibility> {
    v.pieces
        .iter()
        .filter(|p| {
            let s = &v.strokes[p.stroke];
            let (a, b) = (s.point2(&v.basis, p.t0), s.point2(&v.basis, p.t1));
            (a.x - b.x).abs() < 1e-6
                && [11.0, 19.0].iter().any(|x| (a.x - x).abs() < 1e-6)
                && (a.y - b.y).abs() > 1e-6
        })
        .map(|p| p.vis)
        .collect()
}

#[test]
fn bore_visible_after_cut() {
    let s = bored_box();
    let r = run(&[db("Platte", &s)], &plane(ViewKind::Front, 10.0)).unwrap();
    let cut = bore_pieces(&r.lines);
    assert!(cut.contains(&Visibility::Visible), "{cut:?}");
    let plain = compute_view(&[db("Platte", &s)], ViewKind::Front, &Progress::new()).unwrap();
    let before = bore_pieces(&plain);
    assert!(!before.is_empty());
    assert!(
        before.iter().all(|v| *v == Visibility::Hidden),
        "{before:?}"
    );
}

#[test]
fn no_bodies() {
    let e = run(&[], &plane(ViewKind::Front, 1.0)).err().unwrap();
    assert!(matches!(e, DrawError::NoBodies), "{e:?}");
    let t = tube();
    let e = run(&[db("Rohr", &t)], &plane(ViewKind::Front, f64::NAN))
        .err()
        .unwrap();
    assert!(matches!(e, DrawError::NonFinite), "{e:?}");
}

#[test]
fn nudge_reported() {
    let s = stepped();
    let r = run(&[db("Stufe", &s)], &plane(ViewKind::Top, 5.0)).unwrap();
    assert!(r.data.nudged);
    let plain = run(&[db("Stufe", &s)], &plane(ViewKind::Top, 2.5)).unwrap();
    assert!(!plain.data.nudged);
}

#[test]
fn nudge_of_an_earlier_body_is_kept() {
    // Der erste Körper wird verschoben, der zweite nicht.
    let s = stepped();
    let t = moved(&boxs(5.0, 5.0, 20.0), 40.0, 0.0, 0.0);
    let r = run(
        &[db("Stufe", &s), db("Stab", &t)],
        &plane(ViewKind::Top, 5.0),
    )
    .unwrap();
    assert!(r.data.nudged);
    assert_eq!(r.data.source, vec![0, 1]);
}

#[test]
fn source_follows_original_indices_after_a_removed_body() {
    let front = moved(&boxs(5.0, 4.0, 5.0), 30.0, -24.0, 0.0);
    let behind = moved(&boxs(5.0, 8.0, 5.0), 30.0, 12.0, 0.0);
    let t = tube();
    let r = run(
        &[db("Vorn", &front), db("Hinten", &behind), db("Rohr", &t)],
        &plane(ViewKind::Front, 0.0),
    )
    .unwrap();
    assert_eq!(r.data.source, vec![1, 2]);
    assert_eq!(r.data.solids.len(), 2);
    assert!(r.data.regions.iter().all(|g| g.body == 2));
    assert!(r.lines.strokes.iter().any(|s| body_of(&s.origin) == 1));
}

fn names_as_error(b: &[DrawBody], _: ViewKind, _: &Progress) -> Result<ViewLines, DrawError> {
    Err(DrawError::InvalidBody(
        b.iter()
            .map(|x| x.name.as_str())
            .collect::<Vec<_>>()
            .join("|"),
    ))
}

#[test]
fn view_gets_the_names_of_the_kept_bodies() {
    let front = moved(&boxs(5.0, 4.0, 5.0), 30.0, -24.0, 0.0);
    let t = tube();
    let e = compute_section(
        &[db("Vorn", &front), db("Rohr", &t)],
        &plane(ViewKind::Front, 0.0),
        names_as_error,
        &Progress::new(),
    )
    .err()
    .unwrap();
    match e {
        DrawError::InvalidBody(n) => assert_eq!(n, "Rohr"),
        o => panic!("{o:?}"),
    }
}
