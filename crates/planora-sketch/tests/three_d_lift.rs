//! Anheben vorhandener 2D-Punkte durch ebene Kurven in 3D und Rundlauf der Datei (Skizzen 10,
//! Gesamtprüfung C1, I1, I2). Übernommen aus den Prüfdateien der Gesamtprüfung (r.rs, r2.rs):
//! Nach jeder öffentlichen 3D-Operation muss `Sketch::from_data(s.to_data())` gelingen.

use planora_math::{DQuat, DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::helpers::Included;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::text::{HAlign, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use planora_sketch::transform::{Rigid3, TransformSet};
use std::sync::Arc;

fn sketch() -> Sketch {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    s.set_three_d(true);
    s
}

fn load(s: &Sketch) -> Result<(), String> {
    Sketch::from_data(s.to_data())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn bound_2d() -> String {
    planora_i18n::tr!("sketch-err-point-bound-2d")
}

fn in_plane_count(s: &Sketch, c: CurveId) -> usize {
    s.constraint_entries()
        .filter(|k| **k == ConstraintKind::InPlane(c))
        .count()
}

/// S1: Kreis in gekippter Ebene um einen vorhandenen 2D-Punkt mit waagerechtem Maß. Das Maß gibt
/// es an 3D-Geometrie nicht: abgelehnt wie über „Deckungsgleich“, nichts angelegt.
#[test]
fn circle3_on_dimensioned_point_is_refused() {
    let mut s = sketch();
    let p = s.add_point(0.0, 0.0).unwrap();
    let q = s.add_point(10.0, 0.0).unwrap();
    s.add_line(p, q).unwrap();
    s.add_constraint(ConstraintKind::DistanceX(p, q, 10.0))
        .unwrap();
    let before = s.curves().count();
    let e = s.add_circle3(p, 3.0, DVec3::X).unwrap_err();
    assert!(e.to_string().contains(&bound_2d()), "{e}");
    assert!(!s.is_3d_point(p));
    assert_eq!(s.curves().count(), before);
    assert_eq!(load(&s), Ok(()));
}

/// S1b: Bogen in der YZ-Ebene, Enden an Rechteckecken mit Punkt-Linie-Maß: abgelehnt, das Profil
/// bleibt, die Datei lädt.
#[test]
fn arc3_on_point_with_point_line_dim_is_refused() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(0.0, 10.0).unwrap();
    let c = s.add_point(20.0, 10.0).unwrap();
    let d = s.add_point(20.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, c).unwrap();
    let l2 = s.add_line(c, d).unwrap();
    s.add_line(d, a).unwrap();
    s.add_constraint(ConstraintKind::DistancePointLine(a, l2, 20.0))
        .unwrap();
    assert_eq!(s.profiles().profiles.len(), 1);
    let m = s.add_point3(DVec3::new(0.0, 5.0, 0.0)).unwrap();
    let e = s.add_arc3(m, b, a, DVec3::X).unwrap_err();
    assert!(e.to_string().contains(&bound_2d()), "{e}");
    assert!(!s.is_3d_point(a) && !s.is_3d_point(b));
    assert_eq!(s.profiles().profiles.len(), 1);
    assert_eq!(load(&s), Ok(()));
}

/// T10: 2D-Bogenwerkzeug mit einem gefangenen 3D-Punkt; der andere, bemaßte 2D-Punkt dürfte
/// nicht 3D werden: abgelehnt.
#[test]
fn flat_arc_at_3d_point_with_dimensioned_end_is_refused() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let q = s.add_point(9.0, 0.0).unwrap();
    s.add_constraint(ConstraintKind::DistanceX(a, q, 4.0))
        .unwrap();
    let b = s.add_point3(DVec3::new(0.0, 5.0, 0.0)).unwrap();
    let e = s.add_arc(m, a, b).unwrap_err();
    assert!(e.to_string().contains(&bound_2d()), "{e}");
    assert!(!s.is_3d_point(a) && !s.is_3d_point(m));
    assert_eq!(s.curves().count(), 0);
    assert_eq!(load(&s), Ok(()));
}

/// Kreis in gekippter Ebene um die Mitte eines 2D-Kreises: Der 2D-Kreis bekommt mit der Mitte
/// seine Ebene (Normale der Skizze), beide laden.
#[test]
fn circle3_at_center_of_flat_circle_lifts_it() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let flat = s.add_circle(m, 5.0).unwrap();
    let tilted = s.add_circle3(m, 3.0, DVec3::X).unwrap();
    assert!(s.is_3d_point(m));
    assert_eq!(s.curve(flat).unwrap().normal, Some(DVec3::Z));
    assert_eq!(s.curve(tilted).unwrap().normal, Some(DVec3::X));
    assert_eq!(load(&s), Ok(()));
}

/// S2 (I1): 2D-Bogen, dessen Startpunkt durch einen gekippten Kreis um ihn 3D wird. Der Bogen
/// bekommt Normale und `InPlane` wie über „Deckungsgleich“, und nach einem Zug beginnt er weiter
/// an seinem Punkt.
#[test]
fn flat_arc_at_promoted_point_gets_its_plane() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 5.0).unwrap();
    let arc = s.add_arc(m, a, b).unwrap();
    s.add_circle3(a, 2.0, DVec3::X).unwrap();
    assert_eq!(s.curve(arc).unwrap().normal, Some(DVec3::Z));
    assert!(s.is_3d_point(m) && s.is_3d_point(a) && s.is_3d_point(b));
    assert_eq!(in_plane_count(&s, arc), 1);
    assert_eq!(load(&s), Ok(()));
    let at = s.point_xyz(a).unwrap();
    let r = s
        .drag(&[Drag {
            point: a,
            target: at + DVec3::new(0.0, 0.0, 4.0),
            plane: Some((at, DVec3::X)),
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved);
    let (lo, hi) = s.curve_domain3(arc).unwrap();
    let pa = s.point_xyz(a).unwrap();
    let pb = s.point_xyz(b).unwrap();
    let start = s.curve_point3(arc, lo).unwrap();
    let end = s.curve_point3(arc, hi).unwrap();
    assert!(start.distance(pa) < 1e-6, "Anfang {start:?} gegen a {pa:?}");
    assert!(end.distance(pb) < 1e-6, "Ende {end:?} gegen b {pb:?}");
    assert_eq!(load(&s), Ok(()));
}

/// Laden (Gesamtprüfung, nach C1/I1): Ein lebender Bogen ohne Normale an einem 3D-Punkt ist
/// beschädigt; der Bogen löste sich sonst von seinem Punkt.
#[test]
fn loading_refuses_flat_arc_at_3d_point() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 5.0).unwrap();
    s.add_arc(m, a, b).unwrap();
    let mut d = s.to_data();
    d.vars.push(2.0);
    d.points[a.index()].z = Some(d.vars.len() - 1);
    let e = Sketch::from_data(d).unwrap_err().to_string();
    assert!(
        e.contains(&planora_i18n::tr!("sketch-err-data-curve-normal")),
        "{e}"
    );
}

fn shape() -> Arc<TextShape> {
    let v = DVec2::new;
    Arc::new(TextShape {
        lines: vec![ShapedLine {
            glyphs: vec![Glyph {
                origin: v(0.0, 0.0),
                advance: 0.8,
                contours: vec![Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(0.6, 0.0)),
                        Seg::Line(v(0.6, 0.7)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                }],
            }],
            width: 0.8,
        }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    })
}

fn params() -> TextParams {
    TextParams {
        content: "A".into(),
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

fn text_box(s: &mut Sketch) -> planora_sketch::text::TextId {
    s.add_text_box(
        DVec2::new(0.0, 0.0),
        DVec2::new(40.0, 10.0),
        params(),
        HAlign::Left,
        VAlign::Bottom,
        shape(),
    )
    .unwrap()
}

/// S3 (I2): Text ohne gesetzte Form (geladen, Schrift fehlt) aus der Ebene schieben: Die
/// Buchstabenpunkte gehen starr mit, die Datei lädt.
#[test]
fn unshaped_text_moved_out_of_plane_takes_its_glyphs() {
    let mut s = sketch();
    let t = text_box(&mut s);
    let mut s = Sketch::from_data(s.to_data()).unwrap(); // Form fehlt nun
    let before: Vec<DVec3> = s
        .text(t)
        .unwrap()
        .points
        .iter()
        .map(|&p| s.point_xyz(p).unwrap())
        .collect();
    let TextPlacement::Box { lines, .. } = s.text(t).unwrap().placement else {
        panic!("Rahmentext erwartet")
    };
    let set = TransformSet {
        points: vec![],
        curves: lines.to_vec(),
    };
    let shift = DVec3::new(0.0, 0.0, 7.0);
    let m = Rigid3 {
        rot: DQuat::IDENTITY,
        shift,
        pivot: DVec3::ZERO,
    };
    s.move_rigid3(&set, &m, false).unwrap();
    let d = s.text(t).unwrap();
    assert!(d.normal.is_some());
    for (&p, from) in d.points.iter().zip(&before) {
        assert!(s.is_3d_point(p));
        assert!(s.point_xyz(p).unwrap().distance(*from + shift) < 1e-9);
    }
    assert_eq!(load(&s), Ok(()));
    // Gedreht ebenso.
    let turn = Rigid3 {
        rot: DQuat::from_rotation_x(0.7),
        shift: DVec3::ZERO,
        pivot: DVec3::ZERO,
    };
    s.move_rigid3(&set, &turn, false).unwrap();
    assert_eq!(load(&s), Ok(()));
}

fn tilt() -> Rigid3 {
    Rigid3 {
        rot: DQuat::from_rotation_x(0.7) * DQuat::from_rotation_y(-0.4),
        shift: DVec3::new(1.0, 2.0, 3.0),
        pivot: DVec3::new(1.0, 1.0, 0.0),
    }
}

fn flat_shift() -> Rigid3 {
    Rigid3 {
        rot: DQuat::from_rotation_z(0.3),
        shift: DVec3::new(30.0, 5.0, 0.0),
        pivot: DVec3::ZERO,
    }
}

fn planar_set(s: &mut Sketch) -> Vec<CurveId> {
    let m = s.add_point(0.0, 0.0).unwrap();
    let c = s.add_circle(m, 4.0).unwrap();
    let m2 = s.add_point(20.0, 0.0).unwrap();
    let a = s.add_point(25.0, 0.0).unwrap();
    let b = s.add_point(20.0, 5.0).unwrap();
    let arc = s.add_arc(m2, a, b).unwrap();
    let m3 = s.add_point(40.0, 0.0).unwrap();
    let e = s.add_ellipse(m3, DVec2::new(6.0, 1.0), 2.0).unwrap();
    let m4 = s.add_point(60.0, 0.0).unwrap();
    let ea_s = s.add_point(66.0, 0.0).unwrap();
    let ea_e = s.add_point(60.0, 3.0).unwrap();
    let ea = s
        .add_elliptic_arc(m4, DVec2::new(6.0, 0.0), 3.0, ea_s, ea_e)
        .unwrap();
    let l = s.add_line(a, ea_s).unwrap();
    vec![c, arc, e, ea, l]
}

/// T6/T7: 2D-Geometrie aus der Ebene drehen, dann kopieren (eben und im Raum), jeweils Rundlauf.
#[test]
fn moved_and_copied_curves_roundtrip() {
    let mut s = sketch();
    let curves = planar_set(&mut s);
    s.solve();
    let set = TransformSet {
        points: vec![],
        curves,
    };
    s.move_rigid3(&set, &tilt(), false).unwrap();
    assert_eq!(load(&s), Ok(()));
    s.solve();
    assert_eq!(load(&s), Ok(()));
    assert!(
        s.move_rigid3(&set, &flat_shift(), true)
            .unwrap()
            .copy
            .is_some()
    );
    assert_eq!(load(&s), Ok(()));
    s.solve();
    assert!(s.move_rigid3(&set, &tilt(), true).unwrap().copy.is_some());
    assert_eq!(load(&s), Ok(()));
    s.solve();
    assert_eq!(load(&s), Ok(()));
}

/// T5/T9: Text in 3D kippen, lösen, an einer Ecke ziehen, kopieren; Rundlauf.
#[test]
fn text_in_space_roundtrips() {
    let mut s = sketch();
    let t = text_box(&mut s);
    let TextPlacement::Box { lines, corners, .. } = s.text(t).unwrap().placement else {
        panic!("Rahmentext erwartet")
    };
    let set = TransformSet {
        points: vec![],
        curves: lines.to_vec(),
    };
    s.move_rigid3(&set, &tilt(), false).unwrap();
    assert_eq!(load(&s), Ok(()));
    s.solve();
    assert_eq!(load(&s), Ok(()));
    let at = s.point_xyz(corners[2]).unwrap();
    for k in 0..5 {
        let target = at + DVec3::new(3.0 + k as f64, 2.0, 1.5 * k as f64);
        s.drag(&[Drag {
            point: corners[2],
            target,
            plane: Some((at, DVec3::new(0.3, -0.5, 0.8))),
        }])
        .unwrap();
        assert_eq!(load(&s), Ok(()), "Zug {k}");
    }
    assert!(
        s.move_rigid3(&set, &flat_shift(), true)
            .unwrap()
            .copy
            .is_some()
    );
    assert_eq!(load(&s), Ok(()));
    assert!(s.move_rigid3(&set, &tilt(), true).unwrap().copy.is_some());
    assert_eq!(load(&s), Ok(()));
    s.solve();
    assert_eq!(load(&s), Ok(()));
}

/// T8: Einbeziehen aller Arten, nachführen (flach ↔ Raum, beide Richtungen); Rundlauf.
#[test]
fn included_geometry_roundtrips() {
    let n = DVec3::new(0.0, -0.6, 0.8);
    let u = DVec3::X;
    let w = n.cross(u);
    let c = DVec3::new(1.0, 2.0, 3.0);
    let space = [
        Included::Line(DVec3::new(0.0, 0.0, 1.0), DVec3::new(5.0, 0.0, 2.0)),
        Included::Circle { c, r: 4.0, n },
        Included::Arc {
            c,
            start: c + u * 4.0,
            end: c + w * 4.0,
            n,
        },
        Included::Ellipse {
            c,
            major: u * 6.0,
            minor: 2.0,
            n,
        },
        Included::EllipticArc {
            c,
            major: u * 6.0,
            minor: 2.0,
            start: c + u * 6.0,
            end: c + w * 2.0,
            n,
        },
    ];
    let z = DVec3::Z;
    let flat = [
        Included::Line(DVec3::ZERO, DVec3::new(5.0, 0.0, 0.0)),
        Included::Circle {
            c: DVec3::ZERO,
            r: 4.0,
            n: z,
        },
        Included::Arc {
            c: DVec3::ZERO,
            start: DVec3::X * 4.0,
            end: DVec3::Y * 4.0,
            n: z,
        },
        Included::Ellipse {
            c: DVec3::ZERO,
            major: DVec3::X * 6.0,
            minor: 2.0,
            n: z,
        },
        Included::EllipticArc {
            c: DVec3::ZERO,
            major: DVec3::X * 6.0,
            minor: 2.0,
            start: DVec3::X * 6.0,
            end: DVec3::Y * 2.0,
            n: z,
        },
    ];
    for (g3, g2) in space.iter().zip(&flat) {
        for (first, second) in [(g3, g2), (g2, g3)] {
            let mut s = sketch();
            let cv = s.add_included(first).unwrap().curves[0];
            assert_eq!(load(&s), Ok(()));
            s.update_included(cv, second).unwrap();
            assert_eq!(load(&s), Ok(()));
            s.solve();
            s.update_included(cv, first).unwrap();
            assert_eq!(load(&s), Ok(()));
            s.solve();
            assert_eq!(load(&s), Ok(()));
        }
    }
}

/// T10: Fester 2D-Punkt wird über „Deckungsgleich“ 3D; Rundlauf.
#[test]
fn fixed_point_lifted_by_coincident_roundtrips() {
    let mut s = sketch();
    let p = s.add_point(1.0, 2.0).unwrap();
    s.add_constraint(ConstraintKind::Fixed(p, 1.0, 2.0))
        .unwrap();
    let h = s.add_point3(DVec3::new(1.0, 2.0, 0.0)).unwrap();
    s.add_constraint(ConstraintKind::Coincident(p, h)).unwrap();
    assert_eq!(load(&s), Ok(()));
    s.solve();
    assert_eq!(load(&s), Ok(()));
}
