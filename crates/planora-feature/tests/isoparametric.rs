//! Skizzen 11b, Task 15: Rechnung und Nachführen „Isoparametrische Kurve“ (Spec 6,
//! Entscheidung 4). Die Parameterlinie `u = const` bzw. `v = const` einer Körperfläche, beschnitten
//! an ihrer Berandung samt Löchern, als Kurven in Skizzenkoordinaten.

use planora_feature::isoparametric::{iso_curves, iso_range, iso_value_at};
use planora_feature::surface_project::ProjectError;
use planora_feature::*;
use planora_kernel::boolean::face_geom;
use planora_kernel::{BoolOp, Solid, Surface, SurfaceKind};
use planora_math::{DVec3, Frame, PI, TAU, Tolerance};
use planora_sketch::helpers::Included;
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use std::time::Duration;

const EPS: f64 = 1e-6;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn body(d: &Document, feature: FeatureId) -> Solid {
    d.body(BodyRef { feature, index: 0 })
        .unwrap_or_else(|| panic!("kein Körper: {:?}", d.feature(feature).map(|f| &f.status)))
        .clone()
}

fn cyl_at(d: &mut Document, r: f64, h: f64, origin: DVec3) -> FeatureId {
    d.add_feature(
        "Zylinder",
        FeatureKind::Cylinder {
            radius: r,
            height: h,
            origin,
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

/// Erste Fläche der Art `kind` (bei Zylindern die mit dem Radius `radius`, wenn angegeben).
fn face_of(d: &Document, feature: FeatureId, kind: SurfaceKind, radius: Option<f64>) -> FaceRef {
    let s = body(d, feature);
    let (id, _) = s
        .faces
        .iter()
        .find(|(_, f)| {
            f.surface.kind() == kind
                && match (&f.surface, radius) {
                    (Surface::Cylinder { radius: r, .. }, Some(want)) => (r - want).abs() < 1e-9,
                    _ => true,
                }
        })
        .expect("Fläche");
    FaceRef::from_face(&s, id, &tol()).unwrap()
}

fn run(
    d: &Document,
    face: &FaceRef,
    dir: IsoDirection,
    value: f64,
) -> Result<Vec<Included>, ProjectError> {
    iso_curves(
        &DocView::new(d, d.features().len(), Frame::WORLD),
        face,
        dir,
        value,
        &tol(),
        Duration::from_secs(2),
    )
}

fn line(g: &Included) -> (DVec3, DVec3) {
    match g {
        Included::Line(a, b) => (*a, *b),
        g => panic!("keine Linie: {g:?}"),
    }
}

fn circle(g: &Included) -> (DVec3, f64, DVec3) {
    match g {
        Included::Circle { c, r, n } => (*c, *r, *n),
        g => panic!("kein Kreis: {g:?}"),
    }
}

/// Winkel des Bogens gegen den Uhrzeigersinn um `n` von `start` nach `end` (0 … 2π).
fn arc_angle(g: &Included) -> (DVec3, f64, f64) {
    match g {
        Included::Arc { c, start, end, n } => {
            let (a, b) = (*start - *c, *end - *c);
            let ang = a.cross(b).dot(n.normalize()).atan2(a.dot(b));
            (*c, a.length(), if ang <= 0.0 { ang + TAU } else { ang })
        }
        g => panic!("kein Bogen: {g:?}"),
    }
}

fn radial(p: DVec3) -> f64 {
    (p.x * p.x + p.y * p.y).sqrt()
}

#[test]
fn cylinder_u_is_line_v_is_circle() {
    let mut d = Document::new();
    let c = cyl_at(&mut d, 10.0, 30.0, DVec3::ZERO);
    let face = face_of(&d, c, SurfaceKind::Cylinder, None);
    let g = run(&d, &face, IsoDirection::U, 0.25).unwrap();
    assert_eq!(g.len(), 1, "{g:?}");
    let (a, b) = line(&g[0]);
    assert!((a.distance(b) - 30.0).abs() < EPS, "{a:?} {b:?}");
    assert!((radial(a) - 10.0).abs() < EPS && (radial(b) - 10.0).abs() < EPS);
    assert!((a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS);
    let g = run(&d, &face, IsoDirection::V, 0.5).unwrap();
    assert_eq!(g.len(), 1, "{g:?}");
    let (m, r, n) = circle(&g[0]);
    assert!(m.distance(DVec3::new(0.0, 0.0, 15.0)) < EPS, "{m:?}");
    assert!((r - 10.0).abs() < EPS && n.cross(DVec3::Z).length() < EPS);
}

#[test]
fn plane_face_with_hole() {
    let mut d = Document::new();
    let bx = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(20.0, 20.0, 5.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let hole = cyl_at(&mut d, 3.0, 9.0, DVec3::new(10.0, 10.0, -2.0));
    let cut = boolean(&mut d, bx, hole, BoolOp::Difference);
    let s = body(&d, cut);
    let (top, _) = s
        .faces
        .iter()
        .find(|(f, _)| {
            planora_sketch::plane_of_face(&s, *f)
                .is_some_and(|fr| fr.z.z > 0.5 && (fr.origin.z - 5.0).abs() < 1e-9)
        })
        .unwrap();
    let top = FaceRef::from_face(&s, top, &tol()).unwrap();
    for dir in [IsoDirection::U, IsoDirection::V] {
        let g = run(&d, &top, dir, 0.5).unwrap();
        assert_eq!(g.len(), 2, "{g:?}");
        for piece in &g {
            let (a, b) = line(piece);
            assert!((a.distance(b) - 7.0).abs() < EPS, "{a:?} {b:?}");
            assert!((a.z - 5.0).abs() < EPS && (b.z - 5.0).abs() < EPS);
            // Durch die Mitte der Fläche (x = 10 oder y = 10).
            assert!(
                ((a.x - 10.0).abs() < EPS && (b.x - 10.0).abs() < EPS)
                    || ((a.y - 10.0).abs() < EPS && (b.y - 10.0).abs() < EPS),
                "{a:?} {b:?}"
            );
        }
    }
}

/// Zylinder R 10 H 30 um Z mit einem durchgehenden Querloch entlang `axis` (X oder Y):
/// quadratischer Querschnitt 6 × 6 um z = 15. Der Kernel schneidet Zylinder noch nicht mit
/// Zylindern; ein ebenwandiges Loch gibt dieselben Schnittwinkel (2 · asin 0,3 je Loch auf dem
/// Kreis bei z = 15) und dieselben Lochhöhen.
fn cross_bored(d: &mut Document, axis: DVec3) -> FeatureId {
    let main = cyl_at(d, 10.0, 30.0, DVec3::ZERO);
    let (size, origin) = if axis == DVec3::X {
        (DVec3::new(30.0, 6.0, 6.0), DVec3::new(-15.0, -3.0, 12.0))
    } else {
        (DVec3::new(6.0, 30.0, 6.0), DVec3::new(-3.0, -15.0, 12.0))
    };
    let bore = d
        .add_feature("Loch", FeatureKind::Box { size, origin })
        .unwrap();
    boolean(d, main, bore, BoolOp::Difference)
}

/// Prüf-Schwerpunkt 5: Zwei Löcher teilen den v-Kreis in genau zwei Bögen; keiner läuft durch
/// ein Loch, keiner bricht an der Naht ab (Bohrung entlang Y: Naht mitten in einem Bogen;
/// entlang X: ein Loch über der Naht).
#[test]
fn cylinder_with_hole_splits_iso_line() {
    let hole = 2.0 * (0.3f64).asin();
    for axis in [DVec3::Y, DVec3::X] {
        let mut d = Document::new();
        let b = cross_bored(&mut d, axis);
        let face = face_of(&d, b, SurfaceKind::Cylinder, Some(10.0));
        let g = run(&d, &face, IsoDirection::V, 0.5).unwrap();
        assert_eq!(g.len(), 2, "Achse {axis:?}: {g:?}");
        let mut sum = 0.0;
        for piece in &g {
            let (c, r, ang) = arc_angle(piece);
            assert!(c.distance(DVec3::new(0.0, 0.0, 15.0)) < EPS, "{c:?}");
            assert!((r - 10.0).abs() < EPS);
            assert!((ang - (PI - hole)).abs() < 1e-6, "Achse {axis:?}: {ang}");
            sum += ang;
            // Die Bogenmitte liegt nicht in einem Loch (abseits der Bohrachse).
            let Included::Arc { start, end, .. } = piece else {
                unreachable!()
            };
            let mid = ((*start + *end) * 0.5 - c).normalize();
            assert!(mid.dot(axis).abs() < 0.5, "Achse {axis:?}: {mid:?}");
        }
        assert!((sum - (TAU - 2.0 * hole)).abs() < 1e-6, "{sum}");
        // u-Linie durch ein Loch: zwei Linien, oben und unten.
        let at = iso_value_at(
            &face_geom(
                &body(&d, b),
                planora_feature::refs::resolve_face(&[&body(&d, b)], &face, &tol())
                    .unwrap()
                    .face,
            ),
            DVec3::new(10.0 * axis.x, 10.0 * axis.y, 15.0),
            IsoDirection::U,
        );
        let g = run(&d, &face, IsoDirection::U, at).unwrap();
        assert_eq!(g.len(), 2, "Achse {axis:?}: {g:?}");
        let mut len: Vec<f64> = g
            .iter()
            .map(|p| {
                let (a, b) = line(p);
                a.distance(b)
            })
            .collect();
        len.sort_by(f64::total_cmp);
        assert!(
            (len[0] - 12.0).abs() < 1e-6 && (len[1] - 12.0).abs() < 1e-6,
            "{len:?}"
        );
    }
}

#[test]
fn sphere_and_torus_arcs() {
    let mut d = Document::new();
    let s = d
        .add_feature(
            "Kugel",
            FeatureKind::Sphere {
                radius: 8.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let face = face_of(&d, s, SurfaceKind::Sphere, None);
    // Meridian: halber Kreis; Breitenkreis bei der Mitte: Äquator.
    let g = run(&d, &face, IsoDirection::U, 0.3).unwrap();
    assert_eq!(g.len(), 1, "{g:?}");
    let (c, r, ang) = arc_angle(&g[0]);
    assert!(c.length() < EPS && (r - 8.0).abs() < EPS && (ang - PI).abs() < 1e-9);
    let g = run(&d, &face, IsoDirection::V, 0.5).unwrap();
    let (c, r, _) = circle(&g[0]);
    assert!(c.length() < EPS && (r - 8.0).abs() < EPS, "{c:?} {r}");

    // Torus: obere Kreiskante eines Zylinders R 10 H 30 mit R 2 verrundet (Torus R 8, r 2 um
    // Z, ein Viertel des Rohrs).
    let mut d = Document::new();
    let c = cyl_at(&mut d, 10.0, 30.0, DVec3::ZERO);
    let s = body(&d, c);
    let (id, _) = s
        .edges
        .iter()
        .find(|(_, e)| {
            matches!(e.curve, planora_kernel::Curve::Circle { .. })
                && s.vertices[e.start].point.z > 15.0
        })
        .expect("obere Kreiskante");
    let edge = EdgeRef::from_edge(&s, id, &tol()).unwrap();
    let t = d
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
    let face = face_of(&d, t, SurfaceKind::Torus, None);
    // u = const: Viertelbogen des Rohrs (r 2 um einen Punkt auf dem Kreis R 8, z 28).
    let g = run(&d, &face, IsoDirection::U, 0.4).unwrap();
    assert_eq!(g.len(), 1, "{g:?}");
    let (c, r, ang) = arc_angle(&g[0]);
    assert!(
        (radial(c) - 8.0).abs() < EPS && (c.z - 28.0).abs() < EPS,
        "{c:?}"
    );
    assert!(
        (r - 2.0).abs() < EPS && (ang - PI / 2.0).abs() < 1e-6,
        "{r} {ang}"
    );
    // v = const in der Mitte: voller Kreis um Z bei 45° des Rohrs.
    let g = run(&d, &face, IsoDirection::V, 0.5).unwrap();
    assert_eq!(g.len(), 1, "{g:?}");
    let (c, r, n) = circle(&g[0]);
    let q = std::f64::consts::FRAC_PI_4;
    assert!(radial(c) < EPS && n.cross(DVec3::Z).length() < EPS, "{c:?}");
    assert!((c.z - (28.0 + 2.0 * q.sin())).abs() < 1e-6, "{c:?}");
    assert!((r - (8.0 + 2.0 * q.cos())).abs() < 1e-6, "{r}");
}

/// Drehkörper aus einem Spline-Profil (Freiformfläche), wie in den Tests zu „Auf Fläche
/// projizieren“.
fn spline_revolve(d: &mut Document) -> FeatureId {
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
    let s = d
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
    d.add_feature(
        "Rotation",
        FeatureKind::Revolve {
            sketch: s,
            profiles: ProfileSel::Index(0),
            axis,
        },
    )
    .unwrap()
}

#[test]
fn nurbs_face_gives_fixed_spline() {
    let mut d = Document::new();
    let rev = spline_revolve(&mut d);
    let solid = body(&d, rev);
    let face = face_of(&d, rev, SurfaceKind::Nurbs, None);
    let surface = &solid
        .faces
        .iter()
        .find(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs)
        .unwrap()
        .1
        .surface;
    for dir in [IsoDirection::U, IsoDirection::V] {
        let g = run(&d, &face, dir, 0.5).unwrap();
        assert!(!g.is_empty());
        for piece in &g {
            let Included::Spline { curve } = piece else {
                panic!("kein Spline: {piece:?}");
            };
            let (t0, t1) = curve.domain();
            for k in 0..=50 {
                let p = curve.point(t0 + (t1 - t0) * k as f64 / 50.0);
                let gap = surface.implicit(p).abs();
                assert!(gap <= 1e-6, "{dir:?}: Abstand {gap} bei {p:?}");
            }
            let mut sk = Sketch::new(Frame::WORLD, "Ziel", Tolerance::default());
            let ids = sk.add_included(piece).unwrap();
            assert!(matches!(
                sk.curve(ids.curves[0]).unwrap().kind,
                CurveKind::FixedSpline { .. }
            ));
        }
    }
}

#[test]
fn value_is_fraction_of_range() {
    // Viertelzylinder 0 … 90° (Zylinder ∩ Quader im ersten Quadranten).
    let mut d = Document::new();
    let c = cyl_at(&mut d, 10.0, 30.0, DVec3::ZERO);
    let bx = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(20.0, 20.0, 40.0),
                origin: DVec3::new(0.0, 0.0, -5.0),
            },
        )
        .unwrap();
    let q = boolean(&mut d, c, bx, BoolOp::Intersection);
    let face = face_of(&d, q, SurfaceKind::Cylinder, None);
    let g = run(&d, &face, IsoDirection::U, 0.25).unwrap();
    assert_eq!(g.len(), 1, "{g:?}");
    let (a, b) = line(&g[0]);
    let want = 22.5f64.to_radians();
    for p in [a, b] {
        assert!(
            (p.y.atan2(p.x) - want).abs() < 1e-9 && (radial(p) - 10.0).abs() < EPS,
            "{p:?}"
        );
    }
    assert!((a.distance(b) - 30.0).abs() < EPS);
    // Der Bereich ist das Hüllrechteck der Berandung (kein voller Umlauf).
    let s = body(&d, q);
    let found = planora_feature::refs::resolve_face(&[&s], &face, &tol()).unwrap();
    let geom = face_geom(&s, found.face);
    let r = iso_range(&geom);
    assert!((r.u.1 - r.u.0 - PI / 2.0).abs() < 1e-9, "{r:?}");
    assert!((r.v.1 - r.v.0 - 30.0).abs() < 1e-9, "{r:?}");
    let p = DVec3::new(10.0 * want.cos(), 10.0 * want.sin(), 7.5);
    assert!((iso_value_at(&geom, p, IsoDirection::U) - 0.25).abs() < 1e-9);
    assert!((iso_value_at(&geom, p, IsoDirection::V) - 0.25).abs() < 1e-9);
}

/// Zielskizze auf XY mit den Kopien der Isokurven `sources` (je eine Gruppe).
fn follow_doc(d: &mut Document, sources: &[CopySource]) -> (FeatureId, Vec<CurveId>) {
    let mut sk = Sketch::new(Frame::WORLD, "Ziel", Tolerance::default());
    let mut copies = Vec::new();
    let mut curves = Vec::new();
    for source in sources {
        let CopySource::Isoparametric {
            face,
            direction,
            value,
        } = source
        else {
            panic!("{source:?}");
        };
        let doc_tol = d.kernel().tolerance;
        let pieces = iso_curves(
            &DocView::new(d, d.features().len(), Frame::WORLD),
            face,
            *direction,
            *value,
            &doc_tol,
            Duration::from_secs(2),
        )
        .unwrap();
        for (k, g) in pieces.iter().enumerate() {
            let ids = sk.add_included(g).unwrap();
            curves.push(ids.curves[0]);
            copies.push(EdgeCopy {
                target: CopyTarget::Curve(ids.curves[0]),
                source: source.clone(),
                piece: k as u32,
                of: pieces.len() as u32,
            });
        }
    }
    let t = d
        .add_feature(
            "Ziel",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies,
                formulas: Default::default(),
                text_formulas: Default::default(),
            }),
        )
        .unwrap();
    (t, curves)
}

fn iso(face: &FaceRef, direction: IsoDirection, value: f64) -> CopySource {
    CopySource::Isoparametric {
        face: face.clone(),
        direction,
        value,
    }
}

fn warning_of(d: &Document, s: FeatureId) -> String {
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    f.warning().unwrap_or_default().to_string()
}

/// Weltlage der Kurve `c` der berechneten Skizze `s`: Anfang, Ende und (bei Kreisen) Mitte.
fn world(d: &Document, s: FeatureId, c: CurveId) -> (DVec3, DVec3, Option<DVec3>) {
    let sk = d.sketch_of(s).unwrap();
    let (curve, t0, t1) = sk.curve_world(c).unwrap();
    let center = match &curve {
        planora_kernel::Curve::Circle { frame, .. } => Some(frame.origin),
        _ => None,
    };
    (curve.point(t0), curve.point(t1), center)
}

#[test]
fn follows_face_change() {
    let mut d = Document::new();
    let c = cyl_at(&mut d, 10.0, 30.0, DVec3::ZERO);
    let face = face_of(&d, c, SurfaceKind::Cylinder, None);
    let (t, curves) = follow_doc(
        &mut d,
        &[
            iso(&face, IsoDirection::U, 0.25),
            iso(&face, IsoDirection::V, 0.5),
        ],
    );
    assert_eq!(warning_of(&d, t), "");
    let (a0, b0, _) = world(&d, t, curves[0]);
    assert!((a0.distance(b0) - 30.0).abs() < EPS);
    let (_, _, m) = world(&d, t, curves[1]);
    assert!((m.unwrap().z - 15.0).abs() < EPS);
    d.update(c, |k| {
        if let FeatureKind::Cylinder { height, .. } = k {
            *height = 40.0;
        }
    })
    .unwrap();
    assert_eq!(warning_of(&d, t), "");
    let (a1, b1, _) = world(&d, t, curves[0]);
    assert!((a1.distance(b1) - 40.0).abs() < EPS, "{a1:?} {b1:?}");
    // Dieselbe Stelle am Umfang.
    assert!((a1.x - a0.x).abs() < EPS && (a1.y - a0.y).abs() < EPS);
    let (_, _, m) = world(&d, t, curves[1]);
    assert!((m.unwrap().z - 20.0).abs() < EPS, "{m:?}");
    assert_eq!(d.feature(t).unwrap().kind.weak_dependencies(), vec![c]);
    d.check_invariants().unwrap();
}

#[test]
fn lost_face_is_warning() {
    let mut d = Document::new();
    let c = cyl_at(&mut d, 10.0, 30.0, DVec3::ZERO);
    let face = face_of(&d, c, SurfaceKind::Cylinder, None);
    let (t, curves) = follow_doc(&mut d, &[iso(&face, IsoDirection::U, 0.25)]);
    assert_eq!(d.feature(t).unwrap().kind.weak_dependencies(), vec![c]);
    let before = world(&d, t, curves[0]);
    d.delete(c).unwrap();
    assert_eq!(d.feature(t).unwrap().status, FeatureStatus::Ok);
    assert!(!warning_of(&d, t).is_empty());
    assert_eq!(d.feature(t).unwrap().lost_projections().len(), curves.len());
    let after = world(&d, t, curves[0]);
    assert!(before.0.distance(after.0) < EPS && before.1.distance(after.1) < EPS);
    d.check_invariants().unwrap();
}
