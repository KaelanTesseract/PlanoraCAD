//! Skizzen 11b, Task 13: Rechnung und Nachführen „Drehprofil“ (Spec 5, Entscheidung 6). Ein
//! Drehkörper oder bestimmte Elemente werden um die Achse in die Halbebene der Skizze gedreht;
//! das Ergebnis steht in Skizzenkoordinaten (z = 0).

use planora_feature::planes::{AxisRef, PlaneBase, PlaneDef, PlaneFeature, StdAxis};
use planora_feature::spun_profile::{SpunError, spun_profile};
use planora_feature::*;
use planora_i18n::{english, german, with_lang};
use planora_kernel::{BoolOp, Curve, Solid};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::helpers::Included;
use planora_sketch::model::{CurveId, Sketch};

const EPS: f64 = 1e-6;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn xz() -> Frame {
    StandardPlane::XZ.frame()
}

fn cyl(d: &mut Document, r: f64, h: f64, z0: f64) -> FeatureId {
    d.add_feature(
        "Zylinder",
        FeatureKind::Cylinder {
            radius: r,
            height: h,
            origin: DVec3::new(0.0, 0.0, z0),
        },
    )
    .unwrap()
}

fn boolean(d: &mut Document, a: FeatureId, b: FeatureId, op: BoolOp) -> FeatureId {
    d.add_feature(
        "Bool",
        FeatureKind::Boolean {
            target: BodyRef {
                feature: a,
                index: 0,
            },
            tool: BodyRef {
                feature: b,
                index: 0,
            },
            op,
        },
    )
    .unwrap()
}

fn body_spec(feature: FeatureId, axis: SpunAxis) -> SpunSpec {
    SpunSpec {
        selection: SpunSelection::Bodies(vec![BodyRef { feature, index: 0 }]),
        axis,
        tolerance: 0.01,
        flip: false,
    }
}

fn z_axis() -> SpunAxis {
    SpunAxis::Ref(AxisRef::Standard(StdAxis::Z))
}

fn run_on(
    d: &Document,
    frame: Frame,
    spec: &SpunSpec,
    own: Option<(DVec3, DVec3)>,
) -> Result<Vec<Included>, SpunError> {
    spun_profile(
        &DocView::new(d, d.features().len(), frame),
        spec,
        own,
        &tol(),
    )
}

fn run(d: &Document, spec: &SpunSpec) -> Result<Vec<Included>, SpunError> {
    run_on(d, xz(), spec, None)
}

/// Welle aus zwei Zylindern um Z: R 10 von z 0 bis 30, R 5 von z 25 bis 50 (überlappend,
/// damit die Vereinigung keine gemeinsame Stirnfläche braucht).
fn shaft(d: &mut Document, big: f64) -> (FeatureId, FeatureId) {
    let a = cyl(d, big, 30.0, 0.0);
    let b = cyl(d, 5.0, 25.0, 25.0);
    (a, boolean(d, a, b, BoolOp::Union))
}

fn lines(g: &[Included]) -> Vec<(DVec3, DVec3)> {
    g.iter()
        .map(|g| match g {
            Included::Line(a, b) => (*a, *b),
            g => panic!("keine Linie: {g:?}"),
        })
        .collect()
}

fn all_flat(g: &[Included]) {
    for g in g {
        let pts: Vec<DVec3> = match g {
            Included::Point(p) => vec![*p],
            Included::Line(a, b) => vec![*a, *b],
            Included::Arc { c, start, end, .. } => vec![*c, *start, *end],
            Included::Spline { curve } => curve.control_points().to_vec(),
            g => panic!("unerwartet: {g:?}"),
        };
        assert!(
            pts.iter().all(|p| p.z.abs() < EPS),
            "nicht in der Ebene: {g:?}"
        );
    }
}

/// Senkrechte Linie (entlang der Achse, Skizzen-y) bei |x| = r von y0 bis y1?
fn has_axial(ls: &[(DVec3, DVec3)], r: f64, y0: f64, y1: f64) -> bool {
    ls.iter().any(|(a, b)| {
        (a.x.abs() - r).abs() < EPS
            && (b.x.abs() - r).abs() < EPS
            && (a.y.min(b.y) - y0).abs() < EPS
            && (a.y.max(b.y) - y1).abs() < EPS
    })
}

/// Waagerechte Linie (Stirnlinie) bei y von |x| = r0 bis r1?
fn has_radial(ls: &[(DVec3, DVec3)], y: f64, r0: f64, r1: f64) -> bool {
    ls.iter().any(|(a, b)| {
        (a.y - y).abs() < EPS
            && (b.y - y).abs() < EPS
            && (a.x.abs().min(b.x.abs()) - r0).abs() < EPS
            && (a.x.abs().max(b.x.abs()) - r1).abs() < EPS
    })
}

#[test]
fn stepped_shaft_outline() {
    let mut d = Document::new();
    let (_, s) = shaft(&mut d, 10.0);
    let g = run(&d, &body_spec(s, z_axis())).unwrap();
    all_flat(&g);
    let ls = lines(&g);
    assert_eq!(ls.len(), 5, "{ls:?}");
    assert!(has_axial(&ls, 10.0, 0.0, 30.0), "{ls:?}");
    assert!(has_axial(&ls, 5.0, 30.0, 50.0), "{ls:?}");
    assert!(has_radial(&ls, 0.0, 0.0, 10.0), "{ls:?}");
    assert!(has_radial(&ls, 30.0, 5.0, 10.0), "{ls:?}");
    assert!(has_radial(&ls, 50.0, 0.0, 5.0), "{ls:?}");
    // Keine Linie auf der Achse.
    assert!(
        !ls.iter().any(|(a, b)| a.x.abs() < EPS && b.x.abs() < EPS),
        "{ls:?}"
    );
    // Ohne Umkehren liegt alles auf einer Seite.
    let side = ls[0].0.x + ls[0].1.x;
    assert!(
        ls.iter()
            .all(|(a, b)| a.x * side >= -EPS && b.x * side >= -EPS)
    );
}

#[test]
fn tube_has_inner_line() {
    let mut d = Document::new();
    let outer = cyl(&mut d, 10.0, 30.0, 0.0);
    let inner = cyl(&mut d, 6.0, 40.0, -5.0);
    let t = boolean(&mut d, outer, inner, BoolOp::Difference);
    let g = run(&d, &body_spec(t, z_axis())).unwrap();
    all_flat(&g);
    let ls = lines(&g);
    assert_eq!(ls.len(), 4, "{ls:?}");
    assert!(has_axial(&ls, 10.0, 0.0, 30.0), "{ls:?}");
    assert!(has_axial(&ls, 6.0, 0.0, 30.0), "{ls:?}");
    assert!(has_radial(&ls, 0.0, 6.0, 10.0), "{ls:?}");
    assert!(has_radial(&ls, 30.0, 6.0, 10.0), "{ls:?}");
}

fn sphere(d: &mut Document, r: f64) -> FeatureId {
    d.add_feature(
        "Kugel",
        FeatureKind::Sphere {
            radius: r,
            origin: DVec3::ZERO,
        },
    )
    .unwrap()
}

/// Bögen des Ergebnisses als (Mitte, Radius, Anfang, Ende).
fn arcs(g: &[Included]) -> Vec<(DVec3, f64, DVec3, DVec3)> {
    g.iter()
        .map(|g| match g {
            Included::Arc { c, start, end, .. } => (*c, c.distance(*start), *start, *end),
            g => panic!("kein Bogen: {g:?}"),
        })
        .collect()
}

#[test]
fn sphere_gives_half_circle_arc() {
    let mut d = Document::new();
    let k = sphere(&mut d, 10.0);
    let g = run(&d, &body_spec(k, z_axis())).unwrap();
    all_flat(&g);
    let a = arcs(&g);
    assert_eq!(a.len(), 1, "{g:?}");
    let (c, r, s, e) = a[0];
    assert!(c.length() < EPS);
    assert!((r - 10.0).abs() < EPS);
    // Halbkreis von Pol zu Pol.
    assert!(s.x.abs() < EPS && e.x.abs() < EPS, "{s} {e}");
    assert!((s.y + e.y).abs() < EPS && (s.y.abs() - 10.0).abs() < EPS);
    // Seite der Halbebene: n = z x d = (-Y) x Z = -X, also Skizzen-x < 0 (XZ: x = Welt-X).
    let m = arc_mid(&g[0]);
    assert!((m.x + 10.0).abs() < EPS && m.y.abs() < EPS, "{m}");
}

/// Mittelpunkt eines Bogens (gegen den Uhrzeigersinn um +z von `start` nach `end`).
fn arc_mid(g: &Included) -> DVec3 {
    let Included::Arc { c, start, end, n } = g else {
        panic!("{g:?}")
    };
    assert!((n.z - 1.0).abs() < EPS);
    let (u, v) = (*start - *c, *end - *c);
    let h = u.y.atan2(u.x) + sweep(u, v) / 2.0;
    *c + DVec3::new(h.cos(), h.sin(), 0.0) * u.length()
}

/// Winkel gegen den Uhrzeigersinn von `u` nach `v` in (0, 2π].
fn sweep(u: DVec3, v: DVec3) -> f64 {
    let a = v.y.atan2(v.x) - u.y.atan2(u.x);
    if a <= 0.0 {
        a + std::f64::consts::TAU
    } else {
        a
    }
}

#[test]
fn flip_moves_arc_to_other_side() {
    let mut d = Document::new();
    let k = sphere(&mut d, 10.0);
    let spec = SpunSpec {
        flip: true,
        ..body_spec(k, z_axis())
    };
    let g = run(&d, &spec).unwrap();
    assert_eq!(arcs(&g).len(), 1, "{g:?}");
    let m = arc_mid(&g[0]);
    assert!((m.x - 10.0).abs() < EPS && m.y.abs() < EPS, "{m}");
}

/// Achse der ersten Fläche der Art `kind` (Ursprung, z) des Körpers.
fn face_axis(s: &Solid, kind: planora_kernel::SurfaceKind) -> (DVec3, DVec3) {
    let f = s
        .faces
        .iter()
        .find(|(_, f)| f.surface.kind() == kind)
        .expect("Fläche")
        .1;
    let fr = f.surface.frame();
    (fr.origin, fr.z)
}

/// Eine Skizzenebene, die die Achsrichtung `d` enthält.
fn frame_for(d: DVec3) -> Frame {
    if d.cross(DVec3::Z).length() < 1e-9 {
        xz()
    } else {
        Frame::WORLD
    }
}

#[test]
fn cone_gives_slanted_line() {
    let mut d = Document::new();
    let c = d
        .add_feature(
            "Kegel",
            FeatureKind::Cone {
                radius: 6.0,
                height: 8.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let (o, z) = face_axis(&solid(&d, c), planora_kernel::SurfaceKind::Cone);
    let spec = body_spec(c, SpunAxis::Own(CurveId::from_index(0)));
    let g = run_on(&d, frame_for(z), &spec, Some((o, z))).unwrap();
    all_flat(&g);
    let ls = lines(&g);
    // Grundlinie (r 0 bis 6) und Mantellinie (Länge 10), nichts auf der Achse.
    assert_eq!(ls.len(), 2, "{ls:?}");
    let mut len: Vec<f64> = ls.iter().map(|(a, b)| a.distance(*b)).collect();
    len.sort_by(f64::total_cmp);
    assert!(
        (len[0] - 6.0).abs() < EPS && (len[1] - 10.0).abs() < EPS,
        "{len:?}"
    );
}

#[test]
fn torus_fillet_gives_quarter_arc() {
    // Zylinder R 10 H 30, obere Kreiskante mit R 2 verrundet: Torusfläche koaxial zu Z.
    let mut d = Document::new();
    let c = cyl(&mut d, 10.0, 30.0, 0.0);
    let s = solid(&d, c);
    let (id, _) = s
        .edges
        .iter()
        .find(|(_, e)| {
            matches!(e.curve, Curve::Circle { .. }) && s.vertices[e.start].point.z > 15.0
        })
        .expect("obere Kreiskante");
    let edge = EdgeRef::from_edge(&s, id, &tol()).unwrap();
    let f = d
        .add_feature(
            "Rundung",
            FeatureKind::Verrunden {
                body: BodyRef {
                    feature: c,
                    index: 0,
                },
                edges: vec![edge],
                art: BlendArt::Rundung { radius: 2.0 },
                tangentenkette: true,
            },
        )
        .unwrap();
    assert_eq!(d.feature(f).unwrap().status, FeatureStatus::Ok);
    assert!(
        solid(&d, f)
            .faces
            .iter()
            .any(|(_, f)| f.surface.kind() == planora_kernel::SurfaceKind::Torus)
    );
    let g = run(&d, &body_spec(f, z_axis())).unwrap();
    all_flat(&g);
    // Drei Linien (Grund, Mantel bis z 28, Deckel bis r 8) und ein Viertelbogen um (8, 28).
    let (mut ls, mut arcs_) = (Vec::new(), Vec::new());
    for g in &g {
        match g {
            Included::Line(a, b) => ls.push((*a, *b)),
            Included::Arc { c, start, end, .. } => arcs_.push((*c, *start, *end)),
            g => panic!("{g:?}"),
        }
    }
    assert_eq!((ls.len(), arcs_.len()), (3, 1), "{g:?}");
    assert!(has_axial(&ls, 10.0, 0.0, 28.0), "{ls:?}");
    assert!(has_radial(&ls, 0.0, 0.0, 10.0), "{ls:?}");
    assert!(has_radial(&ls, 30.0, 0.0, 8.0), "{ls:?}");
    let (m, a, b) = arcs_[0];
    assert!(
        (m.x.abs() - 8.0).abs() < EPS && (m.y - 28.0).abs() < EPS,
        "{m}"
    );
    assert!((m.distance(a) - 2.0).abs() < EPS);
    assert!((sweep(a - m, b - m) - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
}

#[test]
fn empty_selection_is_nothing() {
    let mut d = Document::new();
    let _ = shaft(&mut d, 10.0);
    for sel in [
        SpunSelection::Bodies(vec![]),
        SpunSelection::Elements(vec![]),
    ] {
        let spec = SpunSpec {
            selection: sel,
            axis: z_axis(),
            tolerance: 0.01,
            flip: false,
        };
        assert_eq!(run(&d, &spec), Err(SpunError::Nothing));
    }
}

/// Achse um 5e-9 gestört (Rest des Skizzenlösers, wie bei einer Achse aus einer Skizzenlinie):
/// Die Welle geht trotzdem durch; um 1e-3 schief bleibt `NotRevolved` (Korrektur I1).
#[test]
fn slightly_perturbed_axis_still_revolved() {
    let mut d = Document::new();
    let (_, s) = shaft(&mut d, 10.0);
    let spec = body_spec(s, SpunAxis::Own(CurveId::from_index(0)));
    let dir = DVec3::new(5e-9, 0.0, 1.0).normalize();
    let g = run_on(&d, xz(), &spec, Some((DVec3::ZERO, dir))).unwrap();
    let ls = lines(&g);
    assert_eq!(ls.len(), 5, "{ls:?}");
    let at_r = |r: f64| {
        ls.iter()
            .any(|(a, b)| (a.x.abs() - r).abs() < 1e-5 && (b.x.abs() - r).abs() < 1e-5)
    };
    assert!(at_r(10.0) && at_r(5.0), "{ls:?}");
    let tilted = DVec3::new(1e-3, 0.0, 1.0).normalize();
    assert_eq!(
        run_on(&d, xz(), &spec, Some((DVec3::ZERO, tilted))),
        Err(SpunError::NotRevolved)
    );
}

/// Eine gekrümmte Kante, die die Achse kreuzt (Knick in r), fällt nicht weg (Korrektur M1).
#[test]
fn curve_crossing_axis_is_kept() {
    let mut d = Document::new();
    let mut sk = Sketch::new(xz(), "Quelle", Tolerance::default());
    let m = sk.add_point(2.0, 10.0).unwrap();
    let circle = sk.add_circle(m, 5.0).unwrap();
    let src = d
        .add_feature(
            "Quelle",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XZ),
                copies: vec![],
                formulas: Default::default(),
                text_formulas: Default::default(),
            }),
        )
        .unwrap();
    let (c, t0, t1) = d.sketch_of(src).unwrap().curve_world(circle).unwrap();
    let spec = elements_spec(vec![CurveSource::SketchCurve {
        sketch: src,
        curve: circle,
    }]);
    let g = run(&d, &spec).unwrap();
    assert!(g.len() >= 2, "an der Achse geteilt: {g:?}");
    let curves: Vec<_> = g
        .iter()
        .map(|g| match g {
            Included::Spline { curve } => curve.clone(),
            g => panic!("{g:?}"),
        })
        .collect();
    let (u0, u1) = curves[0].domain();
    let side = curves[0].point(0.5 * (u0 + u1)).x.signum();
    for i in 0..50 {
        let (r, s) = image_xz(c.point(t0 + (t1 - t0) * i as f64 / 50.0));
        let at = DVec3::new(side * r, s, 0.0);
        let dist = curves
            .iter()
            .map(|k| k.distance_to(at))
            .fold(f64::INFINITY, f64::min);
        assert!(dist <= spec.tolerance, "Punkt {i}: {dist}");
    }
}

#[test]
fn closed_cavity_dropped() {
    let mut d = Document::new();
    let outer = sphere(&mut d, 10.0);
    let inner = sphere(&mut d, 5.0);
    let h = boolean(&mut d, outer, inner, BoolOp::Difference);
    // Der Körper hat wirklich einen Hohlraum (zwei Kugelflächen).
    let spheres = solid(&d, h)
        .faces
        .iter()
        .filter(|(_, f)| f.surface.kind() == planora_kernel::SurfaceKind::Sphere)
        .count();
    assert_eq!(spheres, 2);
    let g = run(&d, &body_spec(h, z_axis())).unwrap();
    let a = arcs(&g);
    assert_eq!(a.len(), 1, "{g:?}");
    assert!((a[0].1 - 10.0).abs() < EPS);
}

#[test]
fn flip_puts_result_other_side() {
    let mut d = Document::new();
    let (_, s) = shaft(&mut d, 10.0);
    let spec = body_spec(s, z_axis());
    let flipped = SpunSpec {
        flip: true,
        ..spec.clone()
    };
    let a = lines(&run(&d, &spec).unwrap());
    let b = lines(&run(&d, &flipped).unwrap());
    let side = |ls: &[(DVec3, DVec3)]| ls.iter().map(|(p, q)| p.x + q.x).sum::<f64>();
    assert!(side(&a) * side(&b) < 0.0, "{a:?} {b:?}");
    assert!((side(&a) + side(&b)).abs() < EPS);
}

#[test]
fn axis_off_plane_refused() {
    let mut d = Document::new();
    let (_, s) = shaft(&mut d, 10.0);
    let e = run_on(&d, Frame::WORLD, &body_spec(s, z_axis()), None).unwrap_err();
    assert_eq!(e, SpunError::AxisNotInPlane);
    assert_eq!(
        with_lang(&german(), || e.message()),
        "Die Achse muss in der Skizzenebene liegen oder parallel zu ihr sein."
    );
    assert_eq!(
        with_lang(&english(), || e.message()),
        "The axis must lie in the sketch plane or parallel to it."
    );
}

#[test]
fn axis_parallel_offset_ok() {
    // Zylinder R 3 um die Achse x = 0, y = 5 (parallel zur XZ-Ebene, 5 mm davor).
    let mut d = Document::new();
    let c = d
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 3.0,
                height: 10.0,
                origin: DVec3::new(0.0, 5.0, 0.0),
            },
        )
        .unwrap();
    let spec = body_spec(c, SpunAxis::Own(CurveId::from_index(0)));
    let g = run_on(&d, xz(), &spec, Some((DVec3::new(0.0, 5.0, 0.0), DVec3::Z))).unwrap();
    all_flat(&g);
    let ls = lines(&g);
    assert_eq!(ls.len(), 3, "{ls:?}");
    assert!(has_axial(&ls, 3.0, 0.0, 10.0), "{ls:?}");
    assert!(has_radial(&ls, 0.0, 0.0, 3.0), "{ls:?}");
    assert!(has_radial(&ls, 10.0, 0.0, 3.0), "{ls:?}");
}

#[test]
fn box_not_revolved() {
    let mut d = Document::new();
    let b = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::splat(10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let e = run(&d, &body_spec(b, z_axis())).unwrap_err();
    assert_eq!(e, SpunError::NotRevolved);
    assert_eq!(
        with_lang(&german(), || e.message()),
        "Drehprofil geht in dieser Fassung nur für Drehkörper um die gewählte Achse."
    );
    assert_eq!(
        with_lang(&english(), || e.message()),
        "Spun Profile currently works only for bodies of revolution about the chosen axis."
    );
}

fn solid(d: &Document, feature: FeatureId) -> Solid {
    d.body(BodyRef { feature, index: 0 }).unwrap().clone()
}

fn elements_spec(items: Vec<CurveSource>) -> SpunSpec {
    SpunSpec {
        selection: SpunSelection::Elements(items),
        axis: z_axis(),
        tolerance: 0.01,
        flip: false,
    }
}

#[test]
fn coaxial_circle_edge_is_point() {
    let mut d = Document::new();
    let c = cyl(&mut d, 10.0, 30.0, 0.0);
    let s = solid(&d, c);
    let (id, _) = s
        .edges
        .iter()
        .find(|(_, e)| {
            matches!(e.curve, Curve::Circle { .. }) && s.vertices[e.start].point.z > 15.0
        })
        .expect("obere Kreiskante");
    let e = EdgeRef::from_edge(&s, id, &tol()).unwrap();
    let g = run(&d, &elements_spec(vec![CurveSource::Edge(e)])).unwrap();
    assert_eq!(g.len(), 1, "{g:?}");
    let Included::Point(p) = g[0] else {
        panic!("{g:?}")
    };
    assert!((p.x.abs() - 10.0).abs() < EPS && (p.y - 30.0).abs() < EPS && p.z.abs() < EPS);
}

/// Bild eines Punkts wie in der Spec: Lage entlang der Z-Achse bleibt (Skizzen-y), Abstand zur
/// Achse wird zu |x|.
fn image_xz(p: DVec3) -> (f64, f64) {
    (p.truncate().length(), p.z)
}

#[test]
fn skew_line_becomes_spline_within_tolerance() {
    // Ebene x = 5 (YZ um 5 versetzt), darauf eine schräge Linie: windschief zur Z-Achse.
    let mut d = Document::new();
    let plane = d
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Offset {
                    base: PlaneBase::Standard(StandardPlane::YZ),
                    distance: 5.0,
                },
            }),
        )
        .unwrap();
    let frame = d.plane_frame(plane).unwrap();
    let mut sk = Sketch::new(frame, "Quelle", Tolerance::default());
    let p = sk.add_point(-10.0, 0.0).unwrap();
    let q = sk.add_point(10.0, 20.0).unwrap();
    let line = sk.add_line(p, q).unwrap();
    let src = d
        .add_feature(
            "Quelle",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::OnPlane(plane),
                copies: vec![],
                formulas: Default::default(),
                text_formulas: Default::default(),
            }),
        )
        .unwrap();
    let (a, b) = {
        let sk = d.sketch_of(src).unwrap();
        let (c, t0, t1) = sk.curve_world(line).unwrap();
        (c.point(t0), c.point(t1))
    };
    let mut spec = elements_spec(vec![CurveSource::SketchCurve {
        sketch: src,
        curve: line,
    }]);
    spec.tolerance = 0.005;
    let g = run(&d, &spec).unwrap();
    assert_eq!(g.len(), 1, "{g:?}");
    let Included::Spline { curve } = &g[0] else {
        panic!("{g:?}")
    };
    assert!(curve.control_points().iter().all(|p| p.z.abs() < EPS));
    for i in 0..50 {
        let w = a.lerp(b, i as f64 / 49.0);
        let (r, s) = image_xz(w);
        // Die Seite ist fest; beide Seiten prüfen und die nähere nehmen wäre zu nachsichtig.
        let side = curve.point(curve.domain().0).x.signum();
        let at = DVec3::new(side * r, s, 0.0);
        let dist = curve.distance_to(at);
        assert!(dist <= spec.tolerance, "Punkt {i}: {dist}");
    }
}

/// Zielskizze auf XZ mit den Kopien des Drehprofils `spec` (eine Gruppe).
fn follow_doc(d: &mut Document, spec: &SpunSpec) -> (FeatureId, Vec<CurveId>) {
    let g = run(d, spec).unwrap();
    let mut sk = Sketch::new(xz(), "Ziel", Tolerance::default());
    let mut copies = Vec::new();
    let mut curves = Vec::new();
    for (k, g) in g.iter().enumerate() {
        let ids = sk.add_included(g).unwrap();
        curves.push(ids.curves[0]);
        copies.push(EdgeCopy {
            target: CopyTarget::Curve(ids.curves[0]),
            source: CopySource::SpunProfile(spec.clone()),
            piece: k as u32,
            of: 0,
        });
    }
    let n = copies.len() as u32;
    for c in &mut copies {
        c.of = n;
    }
    let t = d
        .add_feature(
            "Ziel",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XZ),
                copies,
                formulas: Default::default(),
                text_formulas: Default::default(),
            }),
        )
        .unwrap();
    (t, curves)
}

/// Linien der berechneten Skizze `s` (Kurven `curves`) in Skizzenkoordinaten.
fn sketch_lines(d: &Document, s: FeatureId, curves: &[CurveId]) -> Vec<(DVec3, DVec3)> {
    let sk = d.sketch_of(s).unwrap();
    curves
        .iter()
        .map(|c| {
            let (curve, t0, t1) = sk.curve_world(*c).unwrap();
            (
                xz().to_local(curve.point(t0)),
                xz().to_local(curve.point(t1)),
            )
        })
        .collect()
}

fn warning_of(d: &Document, s: FeatureId) -> String {
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    f.warning().unwrap_or_default().to_string()
}

#[test]
fn shaft_change_follows() {
    let mut d = Document::new();
    let (big, s) = shaft(&mut d, 10.0);
    let (t, curves) = follow_doc(&mut d, &body_spec(s, z_axis()));
    assert_eq!(warning_of(&d, t), "");
    assert!(has_axial(&sketch_lines(&d, t, &curves), 10.0, 0.0, 30.0));
    d.update(big, |k| {
        if let FeatureKind::Cylinder { radius, .. } = k {
            *radius = 12.0;
        }
    })
    .unwrap();
    assert_eq!(warning_of(&d, t), "");
    let ls = sketch_lines(&d, t, &curves);
    assert!(has_axial(&ls, 12.0, 0.0, 30.0), "{ls:?}");
    assert!(has_radial(&ls, 30.0, 5.0, 12.0), "{ls:?}");
    assert_eq!(d.feature(t).unwrap().kind.weak_dependencies(), vec![s]);
    d.check_invariants().unwrap();
}

#[test]
fn deleted_body_is_warning() {
    let mut d = Document::new();
    let (_, s) = shaft(&mut d, 10.0);
    let (t, curves) = follow_doc(&mut d, &body_spec(s, z_axis()));
    let before = sketch_lines(&d, t, &curves);
    d.delete(s).unwrap();
    assert_eq!(d.feature(t).unwrap().status, FeatureStatus::Ok);
    assert!(!warning_of(&d, t).is_empty());
    assert_eq!(d.feature(t).unwrap().lost_projections().len(), curves.len());
    let after = sketch_lines(&d, t, &curves);
    for (x, y) in before.iter().zip(&after) {
        assert!(x.0.distance(y.0) < EPS && x.1.distance(y.1) < EPS);
    }
    d.check_invariants().unwrap();
}

/// Prüfauftrag aus Task 7: Eine eigene Achse, die auf eine gelöschte Kurve oder eine Nicht-Linie
/// zeigt, ist ein verlorener Bezug (Warnung, Kurven bleiben), kein Absturz und kein Fehler. Eine
/// lebende Linie auf der Z-Achse rechnet ohne Warnung.
#[test]
fn own_axis_must_be_live_line() {
    for bad in ["Linie", "gelöscht", "Kreis"] {
        let mut d = Document::new();
        let (_, s) = shaft(&mut d, 10.0);
        let g = run(&d, &body_spec(s, z_axis())).unwrap();
        let mut sk = Sketch::new(xz(), "Ziel", Tolerance::default());
        let m = sk.add_point(0.0, 60.0).unwrap();
        let axis = match bad {
            "Linie" => {
                let o = sk.add_point(0.0, 0.0).unwrap();
                sk.add_line(o, m).unwrap()
            }
            "Kreis" => sk.add_circle(m, 2.0).unwrap(),
            _ => CurveId::from_index(999),
        };
        let spec = body_spec(s, SpunAxis::Own(axis));
        let mut copies = Vec::new();
        let mut curves = Vec::new();
        for (k, g) in g.iter().enumerate() {
            let ids = sk.add_included(g).unwrap();
            curves.push(ids.curves[0]);
            copies.push(EdgeCopy {
                target: CopyTarget::Curve(ids.curves[0]),
                source: CopySource::SpunProfile(spec.clone()),
                piece: k as u32,
                of: 0,
            });
        }
        let n = copies.len() as u32;
        for c in &mut copies {
            c.of = n;
        }
        let t = d
            .add_feature(
                "Ziel",
                FeatureKind::Sketch(SketchFeature {
                    sketch: sk,
                    plane: SketchPlane::Standard(StandardPlane::XZ),
                    copies,
                    formulas: Default::default(),
                    text_formulas: Default::default(),
                }),
            )
            .unwrap();
        let lost = if bad == "Linie" { 0 } else { curves.len() };
        assert_eq!(warning_of(&d, t).is_empty(), lost == 0, "{bad}");
        assert_eq!(
            d.feature(t).unwrap().lost_projections().len(),
            lost,
            "{bad}"
        );
        assert!(has_axial(&sketch_lines(&d, t, &curves), 10.0, 0.0, 30.0));
    }
}
