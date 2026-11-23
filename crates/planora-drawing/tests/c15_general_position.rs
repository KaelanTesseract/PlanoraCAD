//! C15b (2026-11-23): Zeichnungen der sieben Fusion-Fälle in allgemeiner Lage (schräge Zylinder,
//! schräge Bohrung im Rohr, Zylinder und Ebene schräg durch einen Ring mit Torus, Ebene durch
//! einen Kegel, Bohrung neben der Kugelmitte, Kegel gegen schrägen Zylinder), je Vereinigung,
//! Differenz und Schnitt in allen Ansichten. Jede Ansicht gelingt (keine benannte Meldung) und
//! besteht alle Prüfungen von `check_lines`; weicht die Netzreferenz in (e) ab, entscheidet ein
//! exakter Richter am Formelmodell (wie in der Zufallsreihe): Hauchdünne Stellen (etwa 9e-5 mm
//! Material knapp unter dem Scheitel des Torus) löst das Netz nicht auf.

mod common;

use common::check::check_lines;
use common::model::CsgJudge;
use planora_drawing::{DrawBody, compute_view, model::ViewKind};
use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, BoolOp, Kernel, ProfileLoop, Segment, Solid};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};
use std::rc::Rc;

type Sdf = Rc<dyn Fn(DVec3) -> f64>;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Körper und seine Abstandsschätzung (innen negativ).
#[derive(Clone)]
struct Part {
    solid: Solid,
    f: Sdf,
}

fn moved(p: &Part, m: DMat4) -> Part {
    let inv = m.inverse();
    let f = p.f.clone();
    Part {
        solid: k().transform(&p.solid, &m).unwrap(),
        f: Rc::new(move |q| f(inv.transform_point3(q))),
    }
}

/// Zylinder Radius `r`, Länge `h`, mittig um den Ursprung entlang z.
fn cylinder(r: f64, h: f64) -> Part {
    let base = Part {
        solid: k().make_cylinder(r, h).unwrap(),
        f: Rc::new(move |q: DVec3| (q.x.hypot(q.y) - r).max(-q.z).max(q.z - h)),
    };
    moved(
        &base,
        DMat4::from_translation(DVec3::new(0.0, 0.0, -h / 2.0)),
    )
}

/// Kegel: Grundkreis Radius `r` bei z = 0, Spitze bei z = `h`.
fn cone(r: f64, h: f64) -> Part {
    let n = (1.0 + (r / h) * (r / h)).sqrt();
    Part {
        solid: k().make_cone(r, h).unwrap(),
        f: Rc::new(move |q: DVec3| {
            ((q.x.hypot(q.y) - r * (1.0 - q.z / h)) / n)
                .max(-q.z)
                .max(q.z - h)
        }),
    }
}

fn sphere(r: f64) -> Part {
    Part {
        solid: k().make_sphere(r).unwrap(),
        f: Rc::new(move |q: DVec3| q.length() - r),
    }
}

/// Quader `size³` ab `o`.
fn cube(size: f64, o: DVec3) -> Part {
    let base = Part {
        solid: k().make_box(DVec3::splat(size)).unwrap(),
        f: Rc::new(move |q: DVec3| {
            (-q.x)
                .max(q.x - size)
                .max(-q.y)
                .max(q.y - size)
                .max(-q.z)
                .max(q.z - size)
        }),
    };
    moved(&base, DMat4::from_translation(o))
}

/// Scheibe der Dicke 6 bis zum Radius 10 mit halbem Torus außen (großer Radius 10, Rohr 3).
fn ring() -> Part {
    let chain = ProfileLoop {
        start: DVec2::new(0.0, -3.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, -3.0),
            },
            Segment::Arc {
                center: DVec2::new(10.0, 0.0),
                sweep: PI,
            },
            Segment::Line {
                to: DVec2::new(0.0, 3.0),
            },
        ],
    };
    Part {
        solid: k().revolve(&chain, &Frame::WORLD).unwrap(),
        f: Rc::new(|q: DVec3| {
            let rho = q.x.hypot(q.y);
            (rho - 10.0)
                .max(q.z.abs() - 3.0)
                .min((rho - 10.0).hypot(q.z) - 3.0)
        }),
    }
}

fn op(a: &Part, b: &Part, o: BoolOp) -> Part {
    let (fa, fb) = (a.f.clone(), b.f.clone());
    Part {
        solid: k().boolean(&a.solid, &b.solid, o).unwrap(),
        f: match o {
            BoolOp::Union => Rc::new(move |q| fa(q).min(fb(q))),
            BoolOp::Difference => Rc::new(move |q| fa(q).max(-fb(q))),
            BoolOp::Intersection => Rc::new(move |q| fa(q).max(fb(q))),
        },
    }
}

/// Die sieben Fälle (Name, A, B), wie in `planora-mesh/tests/general_position.rs`.
fn cases() -> Vec<(&'static str, Part, Part)> {
    let slab = || cube(40.0, DVec3::new(-20.0, -20.0, -40.0));
    vec![
        (
            "schräge Zylinder",
            cylinder(5.0, 30.0),
            moved(
                &cylinder(2.0, 40.0),
                DMat4::from_translation(DVec3::new(1.0, 0.0, 2.0))
                    * DMat4::from_rotation_x(PI / 3.0),
            ),
        ),
        (
            "Bohrung im Rohr",
            op(
                &cylinder(10.0, 40.0),
                &cylinder(8.0, 50.0),
                BoolOp::Difference,
            ),
            moved(&cylinder(3.0, 60.0), DMat4::from_rotation_y(PI / 3.0)),
        ),
        (
            "Zylinder durch Ring",
            ring(),
            moved(
                &cylinder(1.5, 30.0),
                DMat4::from_translation(DVec3::new(11.5, 0.5, 0.0))
                    * DMat4::from_rotation_x(PI / 6.0),
            ),
        ),
        (
            "Ebene durch Ring",
            ring(),
            moved(
                &slab(),
                DMat4::from_translation(DVec3::new(0.0, 0.0, 1.0))
                    * DMat4::from_rotation_x(0.35)
                    * DMat4::from_rotation_y(0.2),
            ),
        ),
        (
            "Ebene durch Kegel",
            cone(6.0, 12.0),
            moved(
                &slab(),
                DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0))
                    * DMat4::from_rotation_x(0.4)
                    * DMat4::from_rotation_y(0.25),
            ),
        ),
        (
            "Kugel mit Bohrung",
            sphere(8.0),
            moved(
                &cylinder(2.0, 30.0),
                DMat4::from_translation(DVec3::new(3.0, 1.0, 0.0)) * DMat4::from_rotation_x(0.3),
            ),
        ),
        (
            "Kegel gegen Zylinder",
            cone(6.0, 12.0),
            moved(
                &cylinder(1.5, 40.0),
                DMat4::from_translation(DVec3::new(0.5, 0.0, 4.0))
                    * DMat4::from_rotation_y(PI / 2.5),
            ),
        ),
    ]
}

/// Alle Ansichten eines Körpers prüfen; Fehler gesammelt.
fn check_part(name: &str, p: &Part, failures: &mut Vec<String>) {
    let f = p.f.clone();
    let judge = CsgJudge::from_fn(move |q| f(q), &p.solid);
    for kind in ViewKind::ALL {
        let v = match compute_view(
            &[DrawBody {
                name: "K".into(),
                solid: &p.solid,
            }],
            kind,
            &Progress::new(),
        ) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{name} {kind:?}: {e}"));
                continue;
            }
        };
        let r = check_lines(&[&p.solid], kind, &v);
        let basis = planora_drawing::basis::Basis::of(kind);
        let mut resolved = vec![false; r.errors.len()];
        for &(i, x, lib) in &r.vis_diffs {
            if judge.visible(x, &basis).is_none_or(|exact| exact == lib) {
                resolved[i] = true;
            }
        }
        for (e, done) in r.errors.iter().zip(&resolved) {
            if !done {
                failures.push(format!("{name} {kind:?}: {e}"));
            }
        }
    }
}

fn run(index: usize) {
    let (name, a, b) = cases().swap_remove(index);
    let mut failures = Vec::new();
    for o in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
        check_part(&format!("{name} {o:?}"), &op(&a, &b, o), &mut failures);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn skew_cylinders_draw_cleanly() {
    run(0);
}

#[test]
fn an_oblique_hole_through_a_tube_draws_cleanly() {
    run(1);
}

#[test]
fn a_cylinder_through_a_ring_draws_cleanly() {
    run(2);
}

#[test]
fn a_tilted_plane_through_a_ring_draws_cleanly() {
    run(3);
}

#[test]
fn a_tilted_plane_through_a_cone_draws_cleanly() {
    run(4);
}

#[test]
fn an_off_center_hole_through_a_sphere_draws_cleanly() {
    run(5);
}

#[test]
fn a_cone_against_a_skew_cylinder_draws_cleanly() {
    run(6);
}
