//! Verschieben/Kopieren in 3D (Skizzen 10, Task 20, Spec Entwurf 8): Bewegte Punkte werden
//! 3D-Punkte, Kreise, Bögen, Ellipsen und Text bekommen eine gedrehte Normale, 2D-only-
//! Abhängigkeiten entfallen mit ihrer Zahl, 2D-Maße mit 3D-Fassung werden 3D. Eine Bewegung ohne
//! z-Anteil und ohne Drehung um X oder Y geht den bisherigen 2D-Weg, Bit für Bit.

use planora_math::{DQuat, DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;
use planora_sketch::text::{HAlign, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_sketch::transform::{Rigid3, Similarity, TransformSet};
use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

const TOL: f64 = 1e-9;

fn sketch() -> Sketch {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    s.set_three_d(true);
    s
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn xyz(s: &Sketch, p: PointId) -> DVec3 {
    s.point_xyz(p).unwrap()
}

fn set_of(curves: &[CurveId]) -> TransformSet {
    TransformSet {
        curves: curves.to_vec(),
        points: Vec::new(),
    }
}

fn lift(dz: f64) -> Rigid3 {
    Rigid3 {
        rot: DQuat::IDENTITY,
        shift: DVec3::new(0.0, 0.0, dz),
        pivot: DVec3::ZERO,
    }
}

fn turn_x(pivot: DVec3, angle: f64) -> Rigid3 {
    Rigid3 {
        rot: DQuat::from_rotation_x(angle),
        shift: DVec3::ZERO,
        pivot,
    }
}

fn close3(a: DVec3, b: DVec3) -> bool {
    a.distance(b) <= 1e-7
}

fn line_ends(s: &Sketch, l: CurveId) -> (PointId, PointId) {
    match s.curve(l).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

fn alive_kinds(s: &Sketch) -> Vec<K> {
    s.constraints().map(|(_, d)| d.kind.clone()).collect()
}

#[test]
fn move_z_makes_points_3d_and_removes_profile() {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_eq!(s.profiles().profiles.len(), 1, "Vorbedingung: ein Profil");
    let before = alive_kinds(&s).len();
    let report = s.move_rigid3(&set_of(&r.lines), &lift(5.0), false).unwrap();
    assert_eq!(report.removed_constraints, 0);
    assert_eq!(
        alive_kinds(&s).len(),
        before,
        "Waagerecht/Senkrecht bleiben"
    );
    for p in r.points {
        assert!(s.is_3d_point(p), "{p:?} ist 3D");
        assert!((xyz(&s, p).z - 5.0).abs() <= TOL);
    }
    assert!(
        s.profiles().profiles.is_empty(),
        "3D-Geometrie ergibt kein Profil"
    );
    assert_eq!(s.solve().status, SolveStatus::Solved);
    for p in r.points {
        assert!((xyz(&s, p).z - 5.0).abs() <= 1e-7, "Lösen hält die Höhe");
    }
}

#[test]
fn rotate_x_turns_circle_normal() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let circle = s.add_circle(c, 5.0).unwrap();
    let ac = s.add_point(20.0, 0.0).unwrap();
    let a0 = s.add_point(25.0, 0.0).unwrap();
    let a1 = s.add_point(20.0, 5.0).unwrap();
    let arc = s.add_arc(ac, a0, a1).unwrap();
    let ec = s.add_point(0.0, 20.0).unwrap();
    let ellipse = s.add_ellipse(ec, v(6.0, 0.0), 3.0).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let set = set_of(&[circle, arc, ellipse]);
    s.move_rigid3(&set, &turn_x(DVec3::ZERO, FRAC_PI_2), false)
        .unwrap();
    // Um X um 90°: z wird −y, y wird z.
    let down = DVec3::new(0.0, -1.0, 0.0);
    for c in [circle, arc, ellipse] {
        assert!(
            close3(s.curve_normal(c), down),
            "{c:?}: {:?}",
            s.curve_normal(c)
        );
        assert!(s.is_3d_curve(c));
    }
    assert!(close3(xyz(&s, a0), DVec3::new(25.0, 0.0, 0.0)));
    assert!(close3(xyz(&s, a1), DVec3::new(20.0, 0.0, 5.0)));
    assert!(close3(xyz(&s, ec), DVec3::new(0.0, 0.0, 20.0)));
    // Die große Halbachse liegt auf X, sie bleibt; Kurven liegen in ihrer neuen Ebene.
    let major = match s.curve(ellipse).unwrap().kind {
        CurveKind::Ellipse { ax, ay, az, .. } => {
            let vars = s.variables();
            DVec3::new(vars[ax], vars[ay], az.map_or(0.0, |i| vars[i]))
        }
        ref k => panic!("{k:?}"),
    };
    assert!(close3(major, DVec3::new(6.0, 0.0, 0.0)), "{major:?}");
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(
        close3(xyz(&s, a1), DVec3::new(20.0, 0.0, 5.0)),
        "Lösen hält die Lage"
    );
}

/// Mitgebrachter Punkt 1: Drehen um Z (2D-Weg) dreht vorhandene Normalen von Bögen und Ellipsen
/// mit, auch beim Kopieren.
#[test]
fn rotate_z_turns_existing_normals() {
    let mut s = sketch();
    let ac = s.add_point(0.0, 0.0).unwrap();
    let a0 = s.add_point(5.0, 0.0).unwrap();
    let a1 = s.add_point(0.0, 5.0).unwrap();
    let arc = s.add_arc(ac, a0, a1).unwrap();
    let ec = s.add_point(20.0, 0.0).unwrap();
    let ellipse = s.add_ellipse(ec, v(6.0, 0.0), 3.0).unwrap();
    let set = set_of(&[arc, ellipse]);
    // Erst um X kippen: Die Normalen zeigen nach −y.
    s.move_rigid3(&set, &turn_x(DVec3::ZERO, FRAC_PI_2), false)
        .unwrap();
    let quarter = Rigid3 {
        rot: DQuat::from_rotation_z(FRAC_PI_2),
        shift: DVec3::ZERO,
        pivot: DVec3::ZERO,
    };
    assert!(quarter.planar(&Tolerance::default()).is_some(), "2D-Weg");
    let mut copy = s.clone();
    s.move_rigid3(&set, &quarter, false).unwrap();
    // −y um Z um 90° gedreht ist +x.
    for c in [arc, ellipse] {
        assert!(
            close3(s.curve_normal(c), DVec3::X),
            "{:?}",
            s.curve_normal(c)
        );
    }
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let report = copy.move_rigid3(&set, &quarter, true).unwrap();
    let map = report.copy.expect("Kopie");
    for c in [arc, ellipse] {
        let new = map.curves[&c];
        assert!(close3(copy.curve_normal(new), DVec3::X), "Kopie {c:?}");
        assert!(close3(copy.curve_normal(c), -DVec3::Y), "Original bleibt");
    }
    assert_eq!(copy.solve().status, SolveStatus::Solved);
}

#[test]
fn symmetric_removed_count_reported() {
    let mut s = sketch();
    let a = s.add_point(-5.0, 2.0).unwrap();
    let b = s.add_point(5.0, 2.0).unwrap();
    let pa = s.add_point(0.0, -5.0).unwrap();
    let pb = s.add_point(0.0, 5.0).unwrap();
    let axis = s.add_line(pa, pb).unwrap();
    let ab = s.add_line(a, b).unwrap();
    s.add_constraint(K::Symmetric(a, b, axis)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let report = s
        .move_rigid3(&set_of(&[ab, axis]), &lift(3.0), false)
        .unwrap();
    assert_eq!(report.removed_constraints, 1);
    assert!(
        !alive_kinds(&s)
            .iter()
            .any(|k| matches!(k, K::Symmetric(..))),
        "Symmetrisch entfällt"
    );
}

#[test]
fn distance_x_dropped_distance_kept_3d() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    s.add_constraint(K::DistanceX(a, b, 10.0)).unwrap();
    let d = s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(!s.constraint_is_3d(&s.constraint(d).unwrap().kind));
    let report = s.move_rigid3(&set_of(&[line]), &lift(4.0), false).unwrap();
    assert_eq!(report.removed_constraints, 1, "waagerechtes Maß entfällt");
    assert_eq!(report.converted_dimensions, 1, "Abstand wird 3D");
    let kept = s.constraint(d).expect("Abstand bleibt");
    assert!(s.constraint_is_3d(&kept.kind));
    assert!(
        !alive_kinds(&s)
            .iter()
            .any(|k| matches!(k, K::DistanceX(..))),
        "DistanceX entfällt"
    );
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!((xyz(&s, a).distance(xyz(&s, b)) - 10.0).abs() <= 1e-7);
}

#[test]
fn copy_3d_keeps_inner_allowed_constraints() {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
    let (a, b) = line_ends(&s, r.lines[0]);
    s.add_constraint(K::DistanceX(a, b, 20.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let original: Vec<K> = alive_kinds(&s);
    let report = s.move_rigid3(&set_of(&r.lines), &lift(8.0), true).unwrap();
    let map = report.copy.expect("Kopie");
    // Das Original bleibt, wie es war, in der Ebene.
    for p in r.points {
        assert!(!s.is_3d_point(p));
    }
    let new_points: Vec<PointId> = r.points.iter().map(|p| map.points[p]).collect();
    for &p in &new_points {
        assert!(s.is_3d_point(p));
        assert!((xyz(&s, p).z - 8.0).abs() <= TOL);
    }
    let on_copy = |k: &K| {
        k.points().iter().all(|p| new_points.contains(p))
            && k.curves()
                .iter()
                .all(|c| map.curves.values().any(|x| x == c))
    };
    let copied: Vec<K> = alive_kinds(&s).into_iter().filter(on_copy).collect();
    let count = |list: &[K], f: fn(&K) -> bool| list.iter().filter(|k| f(k)).count();
    let hv = |k: &K| matches!(k, K::Horizontal(_) | K::Vertical(_));
    assert_eq!(
        count(&copied, hv),
        count(&original, hv),
        "Waagerecht/Senkrecht kommen mit"
    );
    assert_eq!(count(&copied, |k| matches!(k, K::Distance(..))), 1);
    assert_eq!(count(&copied, |k| matches!(k, K::DistanceX(..))), 0);
    assert_eq!(
        report.removed_constraints, 1,
        "das waagerechte Maß der Kopie entfällt"
    );
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn move_back_to_plane_stays_3d() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    s.move_rigid3(&set_of(&[line]), &lift(5.0), false).unwrap();
    s.move_rigid3(&set_of(&[line]), &lift(-5.0), false).unwrap();
    for p in [a, b] {
        assert!(s.is_3d_point(p), "bleibt 3D");
        assert!(xyz(&s, p).z.abs() <= TOL);
    }
    assert!(s.is_3d_curve(line));
}

/// Ohne z-Anteil und ohne Drehung um X/Y: genau der bisherige Weg, Bit für Bit.
#[test]
fn plain_2d_move_unchanged() {
    let build = || {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let r = s.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
        let (a, b) = line_ends(&s, r.lines[0]);
        s.add_constraint(K::DistanceX(a, b, 20.0)).unwrap();
        let c = s.add_point(40.0, 0.0).unwrap();
        let circle = s.add_circle(c, 3.0).unwrap();
        s.add_constraint(K::Horizontal(LineOrPoints::Points(a, c)))
            .unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        let mut lines = r.lines.to_vec();
        lines.push(circle);
        (s, set_of(&lines))
    };
    let cases = [
        (DVec2::new(3.0, -4.0), 0.0),
        (DVec2::new(1.5, 2.0), 0.7),
        (DVec2::ZERO, -1.2),
    ];
    for (offset, angle) in cases {
        for copy in [false, true] {
            let (mut old, set) = build();
            let (mut new, _) = build();
            let s = Similarity {
                base: DVec2::new(10.0, 5.0),
                offset,
                angle,
                factor: 1.0,
            };
            let old_map = if copy {
                Some(old.copy_transformed(&set, &s).unwrap())
            } else {
                old.apply_transform(&set, &s).unwrap();
                None
            };
            let m = Rigid3 {
                rot: DQuat::from_rotation_z(angle),
                shift: offset.extend(0.0),
                pivot: DVec3::new(10.0, 5.0, 0.0),
            };
            let report = new.move_rigid3(&set, &m, copy).unwrap();
            assert_eq!(report.copy, old_map);
            if angle == 0.0 {
                assert_eq!(new.to_data(), old.to_data(), "{offset:?} {copy}");
            } else {
                // Der Winkel kommt über die Drehung zurück: höchstens Rundung im letzten Bit.
                let (a, b) = (new.variables(), old.variables());
                assert_eq!(a.len(), b.len());
                for (x, y) in a.iter().zip(b) {
                    assert!((x - y).abs() <= 1e-12, "{x} {y}");
                }
                assert_eq!(new.to_data().constraints, old.to_data().constraints);
            }
        }
    }
}

/// Mitgebrachter Punkt 2: Eine Tangente zweier Kreise ohne gemeinsames Ende gilt in 3D nur bei
/// parallelen Ebenen; kippt eine Bewegung eine Ebene weg, entfällt sie und zählt mit, auch wenn
/// der Abstand der Mitten noch stimmt.
#[test]
fn tangent_without_shared_end_removed_when_planes_tilt() {
    let mut s = sketch();
    let c1 = s.add_point(0.0, 0.0).unwrap();
    let k1 = s.add_circle(c1, 5.0).unwrap();
    let c2 = s.add_point(10.0, 0.0).unwrap();
    let k2 = s.add_circle(c2, 5.0).unwrap();
    let t = s.add_constraint(K::Tangent(k1, k2)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    // Um die X-Achse durch beide Mitten: Der Berührpunkt (5, 0, 0) bleibt auf beiden Kreisen.
    let report = s
        .move_rigid3(
            &set_of(&[k2]),
            &turn_x(DVec3::new(10.0, 0.0, 0.0), 0.5),
            false,
        )
        .unwrap();
    assert!(s.constraint(t).is_none(), "Tangente entfällt");
    assert_eq!(report.removed_constraints, 1);
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

/// Gleich gekippte Ebenen behalten ihre Tangente.
#[test]
fn tangent_kept_when_both_planes_turn() {
    let mut s = sketch();
    let c1 = s.add_point(0.0, 0.0).unwrap();
    let k1 = s.add_circle(c1, 5.0).unwrap();
    let c2 = s.add_point(10.0, 0.0).unwrap();
    let k2 = s.add_circle(c2, 5.0).unwrap();
    let t = s.add_constraint(K::Tangent(k1, k2)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let report = s
        .move_rigid3(&set_of(&[k1, k2]), &turn_x(DVec3::ZERO, 0.5), false)
        .unwrap();
    assert!(s.constraint(t).is_some());
    assert_eq!(report.removed_constraints, 0);
}

#[test]
fn fixed_point_refuses_lift() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    let before = s.to_data();
    assert!(s.move_rigid3(&set_of(&[line]), &lift(2.0), false).is_err());
    assert_eq!(s.to_data(), before, "bei einem Fehler bleibt alles");
}

#[test]
fn rotate_x_turns_text_into_its_plane() {
    let mut s = sketch();
    let p = params("AB");
    let shape = Arc::new(BoxShaper.shape(&p.content, &p.style).unwrap());
    let t = s
        .add_text_box(
            v(0.0, 0.0),
            v(20.0, 8.0),
            p,
            HAlign::Left,
            VAlign::Bottom,
            shape,
        )
        .unwrap();
    let d = s.text(t).unwrap().clone();
    let TextPlacement::Box { lines, corners, .. } = d.placement else {
        panic!("Rahmen")
    };
    s.move_rigid3(&set_of(&lines), &turn_x(DVec3::ZERO, FRAC_PI_2), false)
        .unwrap();
    let d = s.text(t).unwrap();
    let n = d.normal.expect("Text in 3D");
    assert!(close3(n, -DVec3::Y), "{n:?}");
    for &q in corners.iter().chain(&d.points) {
        assert!(s.is_3d_point(q));
        assert!(
            xyz(&s, q).y.abs() <= 1e-7,
            "in der Ebene y = 0: {:?}",
            xyz(&s, q)
        );
    }
    assert!(close3(xyz(&s, corners[2]), DVec3::new(20.0, 0.0, 8.0)));
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

// ---- Test-Setzer wie in text_3d.rs -----------------------------------------------------------

const ADV: f64 = 0.8;

fn box_glyph() -> Vec<Contour> {
    vec![Contour {
        start: v(0.0, 0.0),
        segs: vec![
            Seg::Line(v(0.6, 0.0)),
            Seg::Line(v(0.6, 0.7)),
            Seg::Cubic(v(0.4, 0.8), v(0.2, 0.8), v(0.0, 0.7)),
            Seg::Line(v(0.0, 0.0)),
        ],
    }]
}

struct BoxShaper;

impl TextShaper for BoxShaper {
    fn shape(&self, content: &str, _style: &TextStyle) -> Result<TextShape, TextShapeError> {
        let mut glyphs = Vec::new();
        let mut pen = 0.0;
        for _ in content.chars() {
            glyphs.push(Glyph {
                origin: v(pen, 0.0),
                advance: ADV,
                contours: box_glyph(),
            });
            pen += ADV;
        }
        Ok(TextShape {
            lines: vec![ShapedLine { glyphs, width: pen }],
            ascender: 0.9,
            descender: -0.2,
            line_height: 1.2,
            substituted: None,
            synthetic: false,
            missing_chars: false,
        })
    }
}

fn params(content: &str) -> TextParams {
    TextParams {
        content: content.into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    }
}

/// Mitgebrachter Punkt 2 auch im 2D-Weg: Zwei gekippte Kreise (Normale −y) berühren sich; dreht
/// einer um z, stehen die Ebenen schräg zueinander, und die Tangente entfällt.
#[test]
fn tangent_dropped_on_z_turn_of_tilted_circle() {
    let mut s = sketch();
    let c1 = s.add_point(0.0, 0.0).unwrap();
    let k1 = s.add_circle(c1, 5.0).unwrap();
    let c2 = s.add_point(10.0, 0.0).unwrap();
    let k2 = s.add_circle(c2, 5.0).unwrap();
    let t = s.add_constraint(K::Tangent(k1, k2)).unwrap();
    s.move_rigid3(&set_of(&[k1, k2]), &turn_x(DVec3::ZERO, FRAC_PI_2), false)
        .unwrap();
    assert!(s.constraint(t).is_some(), "Vorbedingung: parallele Ebenen");
    let spin = Rigid3 {
        rot: DQuat::from_rotation_z(0.5),
        shift: DVec3::ZERO,
        pivot: DVec3::new(10.0, 0.0, 0.0),
    };
    assert!(spin.planar(&Tolerance::default()).is_some(), "2D-Weg");
    let report = s.move_rigid3(&set_of(&[k2]), &spin, false).unwrap();
    assert!(s.constraint(t).is_none(), "Tangente entfällt");
    assert_eq!(report.removed_constraints, 1);
}
