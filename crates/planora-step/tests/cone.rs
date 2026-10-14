//! Kegel mit allen Vorzeichen (Radius am Ursprung negativ, null oder positiv, Steigung steigend
//! oder fallend), Spitze als Eckpunkt, gespiegelt und gedreht (Teilprojekt 10, Task 8).

mod common;

use common::check::geom_tol;
use common::model::Surf;
use common::{checked, export};
use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, Kernel, ProfileLoop, Segment, Solid, Surface};
use planora_math::{DMat4, DVec2, DVec3, Frame};
use planora_step::{StepBody, StepError, StepOptions, check_body};

/// Abweichung (rad), bis zu der der gelesene halbe Öffnungswinkel `atan(|slope|)` entsprechen
/// muss; der Winkel wird verlustfrei geschrieben, es bleibt nur der Rest der Rundung.
const ANGLE_TOL: f64 = 1e-15;
/// Verschiebung des Radius im Test „jenseits der Spitze“ (mm): macht den Radius in der Mitte des
/// Bereichs (z = 11, Radius 6) negativ.
const RADIUS_SHIFT_BEYOND_APEX: f64 = 7.0;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Drehkörper aus einer Kette über die Punkte (r, z), die auf der Achse beginnt und endet.
fn revolved(points: &[(f64, f64)]) -> Solid {
    let pts: Vec<DVec2> = points.iter().map(|&(r, z)| DVec2::new(r, z)).collect();
    k().revolve(
        &ProfileLoop {
            start: pts[0],
            segments: pts[1..].iter().map(|&to| Segment::Line { to }).collect(),
        },
        &Frame::WORLD,
    )
    .unwrap()
}

/// (Name, Körper, Vorzeichen von Radius am Ursprung und Steigung im Kernel).
fn cones() -> Vec<(&'static str, Solid, f64, f64)> {
    vec![
        // Spitze oben, Radius fällt: radius 6 > 0, slope < 0.
        ("Kegel", k().make_cone(6.0, 9.0).unwrap(), 1.0, -1.0),
        // Kegelstumpf über dem Ursprung, Radius wächst: radius = 2 − 4 · 10 < 0, slope 4.
        (
            "Stumpf steigend",
            revolved(&[(0.0, 10.0), (2.0, 10.0), (10.0, 12.0), (0.0, 12.0)]),
            -1.0,
            1.0,
        ),
        // Unter dem Ursprung, Radius fällt nach oben: radius = 10 − 20 < 0, slope −1.
        (
            "Stumpf unten",
            revolved(&[(0.0, -20.0), (10.0, -20.0), (5.0, -15.0), (0.0, -15.0)]),
            -1.0,
            -1.0,
        ),
        // Spitze genau im Ursprung des Kernels: radius 0, slope 1.
        (
            "Trichter",
            revolved(&[(0.0, 0.0), (5.0, 5.0), (0.0, 5.0)]),
            0.0,
            1.0,
        ),
    ]
}

fn poses() -> Vec<(&'static str, DMat4)> {
    vec![
        ("wie gebaut", DMat4::IDENTITY),
        (
            "gedreht",
            DMat4::from_translation(DVec3::new(-4.0, 2.0, 1.0)) * DMat4::from_rotation_y(2.2),
        ),
        ("gespiegelt", DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0))),
        (
            "gespiegelt und gedreht",
            DMat4::from_rotation_z(0.8) * DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)),
        ),
    ]
}

fn kernel_cone(s: &Solid) -> (f64, f64) {
    s.faces
        .iter()
        .find_map(|(_, f)| match f.surface {
            Surface::Cone { radius, slope, .. } => Some((radius, slope)),
            _ => None,
        })
        .unwrap()
}

#[test]
fn the_test_bodies_cover_every_sign_of_radius_and_slope() {
    for (name, s, r_sign, s_sign) in cones() {
        let (radius, slope) = kernel_cone(&s);
        let sign = |x: f64| if x == 0.0 { 0.0 } else { x.signum() };
        assert_eq!((sign(radius), sign(slope)), (r_sign, s_sign), "{name}");
    }
}

#[test]
fn cones_of_every_sign_pass_all_checks_in_every_pose() {
    for (name, body, _, _) in cones() {
        for (pose, m) in poses() {
            let s = k().transform(&body, &m).unwrap();
            let (text, _) = export(&[(name, &s)]);
            let (model, _) = std::panic::catch_unwind(|| checked(&text, &[&s]))
                .unwrap_or_else(|_| panic!("{name}, {pose}"));
            let (_, slope) = kernel_cone(&s);
            for f in model.shells().iter().flat_map(|sh| sh.faces.iter()) {
                if let Surf::Cone { r, semi, .. } = f.surface {
                    assert!(r > 0.0, "{name}, {pose}: Radius {r}");
                    assert!(
                        (semi - slope.abs().atan()).abs() < ANGLE_TOL,
                        "{name}, {pose}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_apex_of_a_cone_is_a_vertex() {
    let s = k().make_cone(6.0, 9.0).unwrap();
    let (text, _) = export(&[("Kegel", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(
        (stats[0].faces, stats[0].edges, stats[0].vertices),
        (2, 2, 2)
    );
    let tol = geom_tol(&m);
    assert!(
        m.vertices
            .values()
            .any(|p| p.distance(DVec3::new(0.0, 0.0, 9.0)) < tol),
        "Spitze als Eckpunkt"
    );
}

#[test]
fn a_flat_cone_is_written_as_a_cylinder() {
    let mut s = k().make_cylinder(3.0, 5.0).unwrap();
    let id = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .unwrap()
        .0;
    if let Surface::Cylinder { frame, radius } = s.faces[id].surface {
        s.faces[id].surface = Surface::Cone {
            frame,
            radius,
            slope: 0.0,
        };
    }
    let (text, _) = export(&[("Flach", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let cylinders = m
        .shells()
        .iter()
        .flat_map(|sh| sh.faces.iter())
        .filter(|f| matches!(f.surface, Surf::Cylinder { r, .. } if r == 3.0))
        .count();
    assert_eq!(cylinders, 1);
}

#[test]
fn a_cone_face_beyond_its_apex_is_refused_by_name() {
    let mut s = revolved(&[(0.0, 10.0), (2.0, 10.0), (10.0, 12.0), (0.0, 12.0)]);
    let id = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cone { .. }))
        .unwrap()
        .0;
    if let Surface::Cone {
        frame,
        radius,
        slope,
    } = s.faces[id].surface
    {
        // Radius in der Mitte des Bereichs (z = 11) jetzt −1: die Fläche läge jenseits der
        // Spitze.
        s.faces[id].surface = Surface::Cone {
            frame,
            radius: radius - RADIUS_SHIFT_BEYOND_APEX,
            slope,
        };
    }
    let err = check_body(
        &StepBody {
            name: "Jenseits".into(),
            solid: &s,
        },
        &StepOptions::new("kegel.step"),
        &Progress::new(),
    )
    .unwrap_err();
    match err {
        StepError::InvalidBody { name, reason } => {
            assert_eq!(name, "Jenseits");
            assert!(reason.contains("Spitze"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

/// Kantenmaß der Quader für Senkung und Vereinigung (mm).
const BLOCK: DVec3 = DVec3::new(20.0, 20.0, 12.0);
/// Mitte des Quaders in x und y, dort sitzt die Kegelachse.
const BLOCK_CENTER: (f64, f64) = (10.0, 10.0);
/// Maß der Fase und der Rundung am Kegelfuß (mm).
const BLEND_SIZE: f64 = 1.0;
/// Radius des Kegelfußes in diesen Körpern (mm), zum Finden der Kante.
const BASE_RADIUS: f64 = 6.0;

/// Spitze von Körpern in schrägen Drehungen und Spiegelungen: Die Spitze liegt durch Rundung
/// neben der Achse und darf die Normalenprüfung nicht stören.
fn skewed_poses() -> Vec<(&'static str, DMat4)> {
    vec![
        (
            "verschoben, x-, z-gedreht",
            DMat4::from_translation(DVec3::new(3.0, -1.0, 2.0))
                * DMat4::from_rotation_x(0.7)
                * DMat4::from_rotation_z(1.1),
        ),
        (
            "y-gedreht und in y gespiegelt",
            DMat4::from_rotation_y(2.0) * DMat4::from_scale(DVec3::new(1.0, -1.0, 1.0)),
        ),
        (
            "weit verschoben, x-gedreht",
            DMat4::from_translation(DVec3::new(2e5, 0.0, 0.0)) * DMat4::from_rotation_x(0.9),
        ),
    ]
}

fn base_circle(s: &Solid) -> planora_kernel::Id<planora_kernel::Edge> {
    s.edges
        .iter()
        .find(|(_, e)| {
            matches!(e.curve, planora_kernel::Curve::Circle { radius, .. } if radius == BASE_RADIUS)
        })
        .unwrap()
        .0
}

fn blended(kind: planora_kernel::BlendKind) -> Solid {
    let cone = k().make_cone(BASE_RADIUS, 9.0).unwrap();
    let request = planora_kernel::BlendRequest {
        edges: vec![planora_kernel::BlendEdge {
            edge: base_circle(&cone),
            first_face: None,
        }],
        kind,
        tangent_chain: false,
    };
    k().blend(&cone, &request).unwrap().solid
}

fn at_block_center(s: &Solid, z: f64) -> Solid {
    k().transform(
        s,
        &DMat4::from_translation(DVec3::new(BLOCK_CENTER.0, BLOCK_CENTER.1, z)),
    )
    .unwrap()
}

fn apex_bodies() -> Vec<(&'static str, Solid)> {
    let block = k().make_box(BLOCK).unwrap();
    let cone = k().make_cone(BASE_RADIUS, 9.0).unwrap();
    let upside_down = k()
        .transform(&cone, &DMat4::from_rotation_x(std::f64::consts::PI))
        .unwrap();
    let below = at_block_center(&cone, -1.0);
    let above = at_block_center(&upside_down, BLOCK.z + 1.0);
    let sunk = at_block_center(&cone, BLOCK.z - 1.0);
    vec![
        ("Kegel", cone.clone()),
        (
            "Senkung unten",
            k().boolean(&block, &below, planora_kernel::BoolOp::Difference)
                .unwrap(),
        ),
        (
            "Senkung oben",
            k().boolean(&block, &above, planora_kernel::BoolOp::Difference)
                .unwrap(),
        ),
        ("Trichter", revolved(&[(0.0, 0.0), (5.0, 5.0), (0.0, 5.0)])),
        (
            "Kegel mit Fase",
            blended(planora_kernel::BlendKind::ChamferEqual {
                distance: BLEND_SIZE,
            }),
        ),
        (
            "Kegel mit Rundung",
            blended(planora_kernel::BlendKind::Fillet { radius: BLEND_SIZE }),
        ),
        (
            "Quader und Kegel",
            k().boolean(&block, &sunk, planora_kernel::BoolOp::Union)
                .unwrap(),
        ),
    ]
}

#[test]
fn an_apex_beside_the_axis_by_rounding_passes_the_normal_check() {
    for (name, body) in apex_bodies() {
        for (pose, m) in skewed_poses() {
            let s = k().transform(&body, &m).unwrap();
            let (text, _) = export(&[(name, &s)]);
            std::panic::catch_unwind(|| checked(&text, &[&s]))
                .unwrap_or_else(|_| panic!("{name}, {pose}"));
        }
    }
}
