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

#[test]
fn isometric_plane_is_unsupported() {
    let t = tube();
    let e = run(&[db("Rohr", &t)], &plane(ViewKind::Isometric, 10.0))
        .err()
        .unwrap();
    assert!(matches!(e, DrawError::Unsupported(_)), "{e:?}");
}

/// Schnittfläche des Körpers `body` aus den Randschleifen (äußere minus innere).
fn region_area(r: &SectionLines, body: usize, kind: ViewKind) -> f64 {
    let b = planora_drawing::Basis::of(kind);
    r.data
        .regions
        .iter()
        .filter(|g| g.body == body)
        .map(|g| {
            let mut a: Vec<f64> = g
                .loops
                .iter()
                .map(|l| polygon_area(&loop_polygon(l, &b, 1e-5)).abs())
                .collect();
            let outer = a.remove(0);
            outer - a.iter().sum::<f64>()
        })
        .sum()
}

#[test]
fn grazed_large_cylinder_in_an_assembly_is_not_lost() {
    // Zylinder r 100 und ein Quader daneben; die Ebene liegt 0,01 bzw. 0,001 mm innerhalb des
    // Zylinderrands. Früher verschwand der Zylinder still (Hüllbox zu klein) oder blieb
    // ungeschnitten, nur der Quader war schraffiert. Jetzt: richtige Fläche oder eine Meldung,
    // die den Zylinder nennt (bei 0,001 mm liegen Schnittkante und Umriss in der Ansicht fast
    // aufeinander), nie ein Ergebnis ohne ihn.
    let mut cyl = k().make_cylinder(100.0, 50.0).unwrap();
    cyl.assign_origin(7);
    let block = moved(&boxs(20.0, 100.0, 50.0), 150.0, 50.0, 0.0);
    for eps in [0.01_f64, 0.001] {
        let want = 2.0 * (100.0_f64.powi(2) - (100.0 - eps).powi(2)).sqrt() * 50.0;
        for kind in [ViewKind::Front, ViewKind::Back] {
            let what = format!("{kind:?} ε {eps}");
            match run(
                &[db("Rad", &cyl), db("Klotz", &block)],
                &plane(kind, 100.0 - eps),
            ) {
                Ok(r) => {
                    assert_eq!(r.data.source, vec![0, 1], "{what}");
                    let got = region_area(&r, 0, kind);
                    assert!((got - want).abs() < 1e-3, "{what}: {got} statt {want}");
                    assert!(
                        (region_area(&r, 1, kind) - 20.0 * 50.0).abs() < 1e-9,
                        "{what}"
                    );
                }
                Err(DrawError::AmbiguousVisibility(n)) => {
                    assert!(eps < 0.005 && n.starts_with("Rad"), "{what}: {n}")
                }
                Err(e) => panic!("{what}: {e:?}"),
            }
        }
    }
}

/// Quader 400 × 300 × 200 mit Bohrung r 75 längs z (Achse x = 200, y = 150).
fn big_bored_block() -> Solid {
    let drill = moved(&k().make_cylinder(75.0, 204.0).unwrap(), 200.0, 150.0, -2.0);
    diff(&boxs(400.0, 300.0, 200.0), &drill)
}

#[test]
fn plane_a_hair_beside_a_bore_axis_gives_a_view() {
    // 5e-5 bis 3e-4 mm neben der Achse: Kante der Schnittfläche und Umriss der Bohrung liegen
    // in der Ansicht fast aufeinander; die Ansicht war unsicher und meldete einen Fehler. Jetzt
    // wird einmal um 0,001 mm verschoben (Hinweis wie im Tangentialfall).
    let s = big_bored_block();
    for (kind, d) in [
        (ViewKind::Right, 5e-5),
        (ViewKind::Right, 1.5e-4),
        (ViewKind::Right, 3e-4),
        (ViewKind::Left, -1.5e-4),
        (ViewKind::Front, -1.5e-4),
        (ViewKind::Back, 3e-4),
    ] {
        let center = if matches!(kind, ViewKind::Left | ViewKind::Right) {
            200.0
        } else {
            150.0
        };
        let r = run(&[db("Block", &s)], &plane(kind, center + d))
            .unwrap_or_else(|e| panic!("{kind:?} d {d}: {e}"));
        assert!(r.data.nudged, "{kind:?} d {d}");
        let h = if matches!(kind, ViewKind::Left | ViewKind::Right) {
            300.0
        } else {
            400.0
        };
        let (_, da) = plane(kind, 0.0).axis().unwrap();
        let shifted = d + da * SECTION_NUDGE_MM;
        let want = (h - 2.0 * (75.0_f64 * 75.0 - shifted * shifted).sqrt()) * 200.0;
        let got = region_area(&r, 0, kind);
        assert!(
            (got - want).abs() < 1e-3,
            "{kind:?} d {d}: {got} statt {want}"
        );
    }
    // Weiter weg bleibt alles wie bisher: kein Verschieben, Fläche an der eigenen Ebene.
    for d in [0.3, 2.0] {
        let r = run(&[db("Block", &s)], &plane(ViewKind::Right, 200.0 + d)).unwrap();
        assert!(!r.data.nudged, "d {d}");
        let want = (300.0 - 2.0 * (75.0_f64 * 75.0 - d * d).sqrt()) * 200.0;
        assert!(
            (region_area(&r, 0, ViewKind::Right) - want).abs() < 1e-3,
            "d {d}"
        );
    }
}

static AMBIGUOUS_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Meldet immer Unsicherheit, beim ersten Aufruf mit „erste“, danach mit „zweite“.
fn always_ambiguous(_: &[DrawBody], _: ViewKind, _: &Progress) -> Result<ViewLines, DrawError> {
    let n = AMBIGUOUS_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Err(DrawError::AmbiguousVisibility(
        if n == 0 { "erste" } else { "zweite" }.into(),
    ))
}

#[test]
fn second_ambiguity_keeps_the_first_message() {
    let s = bored_box();
    let e = compute_section(
        &[db("Block", &s)],
        &plane(ViewKind::Front, 7.0),
        always_ambiguous,
        &Progress::new(),
    )
    .err()
    .unwrap();
    assert!(
        matches!(e, DrawError::AmbiguousVisibility(ref n) if n == "erste"),
        "{e:?}"
    );
    assert_eq!(
        AMBIGUOUS_CALLS.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "genau ein Zweitversuch"
    );
}
