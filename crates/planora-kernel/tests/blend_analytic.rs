//! Analytische Rundungs- und Fasenflächen (Teilprojekt 8, Task 3).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::analytic::*;
use planora_kernel::blend::section::ChamferSpec;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn tol() -> Tolerance {
    Tolerance::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn geom(s: &Solid, a: &str, b: &str, kind: SectionKind) -> Result<Option<EdgeGeom>, BlendError> {
    let e = edge_between(s, a, b);
    let first = edge_uses(s, e)
        .into_iter()
        .find(|u| face_name(s, u.face) == a)
        .map(|u| u.face);
    let sides = edge_sides(s, e, first).unwrap();
    let angle = edge_angle(s, e, &sides).unwrap();
    analytic_geom(s, e, &sides, angle.convex, kind, &tol())
}

/// Querschnittsfläche zwischen Kante (Innenwinkel 90°) und Viertelkreis: (1 − π/4)·r².
fn spandrel90(r: f64) -> f64 {
    (1.0 - PI / 4.0) * r * r
}

/// Abstand des Schwerpunkts dieser Fläche von jeder der beiden Seiten (rechter Winkel).
fn spandrel90_offset(r: f64) -> f64 {
    r * (5.0 / 6.0 - PI / 4.0) / (1.0 - PI / 4.0)
}

#[test]
fn a_box_edge_fillet_is_a_cylinder_along_the_edge() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let g = geom(&s, "Deckel", "Wand 1", SectionKind::Fillet { radius: 2.0 })
        .unwrap()
        .unwrap();
    match &g.surface {
        Surface::Cylinder { frame, radius } => {
            assert!((radius - 2.0).abs() < 1e-12);
            assert!(frame.z.cross(DVec3::X).length() < 1e-12);
            assert!((frame.origin.y - 2.0).abs() < 1e-12 && (frame.origin.z - 8.0).abs() < 1e-12);
        }
        other => panic!("{other:?}"),
    }
    assert!(g.same_sense && g.convex && !g.closed && g.analytic);
    // Berührkurven liegen auf ihren Flächen und laufen über die ganze Kante.
    let (t0, t1) = g.range;
    assert!(((t1 - t0).abs() - 10.0).abs() < 1e-12);
    for f in [0.0, 0.3, 1.0] {
        let t = t0 + (t1 - t0) * f;
        let (pa, pb) = (g.contact_a.point(t), g.contact_b.point(t));
        assert!(
            (pa.z - 10.0).abs() < 1e-12 && (pa.y - 2.0).abs() < 1e-12,
            "{pa:?}"
        );
        assert!(pb.y.abs() < 1e-12 && (pb.z - 8.0).abs() < 1e-12, "{pb:?}");
    }
    assert!(g.section_start.start().distance(g.contact_a.point(t0)) < 1e-12);
    assert!(g.section_start.end().distance(g.contact_b.point(t0)) < 1e-12);
    assert!(g.section_end.start().distance(g.contact_a.point(t1)) < 1e-12);
    match g.volume {
        VolumeModel::Prism { area } => {
            assert!((area - spandrel90(2.0)).abs() < 1e-3 * spandrel90(2.0))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_concave_fillet_faces_the_other_way() {
    let s = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(10.0, 0.0),
                    DVec2::new(10.0, 4.0),
                    DVec2::new(4.0, 4.0),
                    DVec2::new(4.0, 10.0),
                    DVec2::new(0.0, 10.0),
                ])
                .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            5.0,
        )
        .unwrap();
    let g = geom(&s, "Wand 3", "Wand 4", SectionKind::Fillet { radius: 1.0 })
        .unwrap()
        .unwrap();
    assert!(!g.convex && !g.same_sense);
    match &g.surface {
        Surface::Cylinder { frame, .. } => {
            assert!(frame.origin.truncate().distance(DVec2::new(5.0, 5.0)) < 1e-12)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_cylinder_top_edge_gives_a_closed_torus_and_a_cone_chamfer() {
    let s = k().make_cylinder(5.0, 8.0).unwrap();
    let g = geom(&s, "Wand 1", "Deckel", SectionKind::Fillet { radius: 1.0 })
        .unwrap()
        .unwrap();
    match &g.surface {
        Surface::Torus {
            major,
            minor,
            frame,
        } => {
            assert!((major - 4.0).abs() < 1e-10 && (minor - 1.0).abs() < 1e-12);
            assert!((frame.origin.z - 7.0).abs() < 1e-10);
        }
        other => panic!("{other:?}"),
    }
    assert!(g.closed);
    assert!(((g.range.1 - g.range.0).abs() - TAU).abs() < 1e-12);
    assert!(g.section_start.start().distance(g.section_end.start()) < 1e-10);
    match g.volume {
        VolumeModel::Pappus {
            area,
            centroid_radius,
        } => {
            assert!((area - spandrel90(1.0)).abs() < 1e-3 * spandrel90(1.0));
            assert!((centroid_radius - (5.0 - spandrel90_offset(1.0))).abs() < 1e-3);
        }
        other => panic!("{other:?}"),
    }
    let c = geom(
        &s,
        "Wand 1",
        "Deckel",
        SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 1.0 }),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(c.surface, Surface::Cone { .. }), "{:?}", c.surface);
    match c.volume {
        VolumeModel::Pappus {
            area,
            centroid_radius,
        } => {
            assert!((area - 0.5).abs() < 1e-12);
            assert!((centroid_radius - (5.0 - 1.0 / 3.0)).abs() < 1e-12);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn chamfers_on_a_box_edge_honour_face_one() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let two = geom(
        &s,
        "Deckel",
        "Wand 1",
        SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 2.5 }),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(two.surface, Surface::Plane { .. }));
    let p = two.contact_a.point(two.range.0);
    assert!(
        (p.y - 1.0).abs() < 1e-12 && (p.z - 10.0).abs() < 1e-12,
        "Fläche 1 = Deckel: {p:?}"
    );
    let q = two.contact_b.point(two.range.0);
    assert!((q.z - 7.5).abs() < 1e-12, "{q:?}");
    let angle = geom(
        &s,
        "Deckel",
        "Wand 1",
        SectionKind::Chamfer(ChamferSpec::DistanceAngle {
            distance: 1.0,
            angle: 30f64.to_radians(),
        }),
    )
    .unwrap()
    .unwrap();
    let q = angle.contact_b.point(angle.range.0);
    assert!(
        (10.0 - q.z - 30f64.to_radians().tan()).abs() < 1e-10,
        "{q:?}"
    );
}

#[test]
fn hemisphere_and_cone_base_edges_are_tori() {
    let r_big = 5.0;
    let hemi = k()
        .revolve(
            &ProfileLoop {
                start: DVec2::ZERO,
                segments: vec![
                    Segment::Line {
                        to: DVec2::new(r_big, 0.0),
                    },
                    Segment::Arc {
                        center: DVec2::ZERO,
                        sweep: PI / 2.0,
                    },
                ],
            },
            &Frame::WORLD,
        )
        .unwrap();
    let g = geom(
        &hemi,
        "Fläche 1",
        "Fläche 2",
        SectionKind::Fillet { radius: 1.0 },
    )
    .unwrap()
    .unwrap();
    match &g.surface {
        Surface::Torus { major, .. } => assert!((major - 15f64.sqrt()).abs() < 1e-9, "{major}"),
        other => panic!("{other:?}"),
    }
    let cone = k().make_cone(4.0, 6.0).unwrap();
    let g = geom(
        &cone,
        "Fläche 1",
        "Fläche 2",
        SectionKind::Fillet { radius: 0.5 },
    )
    .unwrap()
    .unwrap();
    assert!(matches!(g.surface, Surface::Torus { .. }));
    // Berührpunkt auf dem Kegelmantel liegt auf dem Mantel.
    let fb = &cone.faces[g.sides.b.face];
    let p = g.contact_b.point(g.range.0);
    assert!(fb.surface.implicit(p).abs() < 1e-9);
}

#[test]
fn curvature_and_axis_limits_are_named() {
    let thin = k().make_cylinder(1.0, 5.0).unwrap();
    let r = geom(
        &thin,
        "Wand 1",
        "Deckel",
        SectionKind::Fillet { radius: 1.5 },
    );
    assert!(
        matches!(r, Err(BlendError::CurvatureTooSmall { .. })),
        "{r:?}"
    );
    // Zylinder r = 2, Rundung 1,5: die Kugelbahn (ρ = 0,5) läge näher an der Achse als der Radius.
    let mid = k().make_cylinder(2.0, 5.0).unwrap();
    let r = geom(
        &mid,
        "Wand 1",
        "Deckel",
        SectionKind::Fillet { radius: 1.5 },
    );
    assert!(
        matches!(&r, Err(BlendError::Unsupported(m)) if m.contains("Drehachse")),
        "{r:?}"
    );
}

#[test]
fn freeform_edges_are_not_analytic() {
    let ellipse = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 3.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let s = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::nurbs_loop(ellipse).unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            4.0,
        )
        .unwrap();
    let g = geom(&s, "Wand 1", "Deckel", SectionKind::Fillet { radius: 0.5 }).unwrap();
    assert!(g.is_none());
}

/// Alle Zahlen einer gelieferten Geometrie sind endlich.
fn assert_finite(g: &EdgeGeom) {
    let (t0, t1) = g.range;
    for f in [0.0, 0.5, 1.0] {
        let t = t0 + (t1 - t0) * f;
        assert!(
            g.contact_a.point(t).is_finite() && g.contact_b.point(t).is_finite(),
            "{g:?}"
        );
        assert!(
            g.section_start.at(f).is_finite() && g.section_end.at(f).is_finite(),
            "{g:?}"
        );
    }
    assert!(g.reach.is_finite(), "{g:?}");
    let v = match g.volume {
        VolumeModel::Prism { area } => area,
        VolumeModel::Pappus {
            area,
            centroid_radius,
        } => area + centroid_radius,
        VolumeModel::Total { volume } => volume,
    };
    assert!(v.is_finite(), "{g:?}");
}

/// Feindliche Zahlen direkt an `analytic_geom`: prüft nur „kein Panic“ und „jede gelieferte
/// Geometrie ist endlich“. Maße außerhalb des erlaubten Bereichs, die hier noch eine endliche
/// Geometrie ergeben können (etwa Radius 1e6, Fasenwinkel −1 oder 1e6), weist erst
/// `check_request` (Task 5) ab, nicht diese Ebene.
#[test]
fn hostile_sizes_never_panic_or_give_nan() {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = k().make_cylinder(5.0, 8.0).unwrap();
    let bad = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -1.0,
        1e-300,
        1e300,
    ];
    for s in [&cube, &cyl] {
        let (a, b) = if std::ptr::eq(s, &cube) {
            ("Deckel", "Wand 1")
        } else {
            ("Wand 1", "Deckel")
        };
        for &x in &bad {
            let kinds = [
                SectionKind::Fillet { radius: x },
                SectionKind::Chamfer(ChamferSpec::Distances { a: x, b: 1.0 }),
                SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: x }),
                SectionKind::Chamfer(ChamferSpec::DistanceAngle {
                    distance: x,
                    angle: 0.5,
                }),
                SectionKind::Chamfer(ChamferSpec::DistanceAngle {
                    distance: 1.0,
                    angle: x,
                }),
            ];
            for kind in kinds {
                match geom(s, a, b, kind) {
                    Ok(Some(g)) => assert_finite(&g),
                    Ok(None) => panic!("analytische Kante erwartet ({kind:?})"),
                    Err(_) => {}
                }
            }
        }
    }
}

#[test]
fn polygon_area_and_centroid_of_a_unit_square() {
    let pts = [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y];
    let (a, c) = polygon_area_centroid(&pts);
    assert!((a - 1.0).abs() < 1e-15);
    assert!(c.distance(DVec3::new(0.5, 0.5, 0.0)) < 1e-15);
}

// ---------------------------------------------------------------------------------------------
// Rollkugelprobe mit festem Startwert (Lage, G1-Übergang und Außenrichtung aller Flächen).
// ---------------------------------------------------------------------------------------------

/// Fester Pseudozufall (wie in `nurbs_geometry.rs`).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

/// Abstand eines Punkts zu einer Fläche, über dem er nicht mehr auf ihr liegt.
const ON_SURFACE: f64 = 1e-8;
/// Abweichung zwischen den beiden Kugelmittelpunkten bzw. zwischen Normalen.
const SAME_CENTER: f64 = 1e-7;
/// Stellen entlang der Kante (Bruchteile des Bereichs).
const STATIONS: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// Außennormale der neuen Fläche an einem Punkt auf ihr.
fn blend_out(g: &EdgeGeom, p: DVec3) -> DVec3 {
    let (u, v) = g.surface.project(p);
    let n = g.surface.normal(u, v);
    if g.same_sense { n } else { -n }
}

/// Kantenpunkt und Einheitstangente (Kantenrichtung) am Bruchteil `f`.
fn edge_station(s: &Solid, g: &EdgeGeom, f: f64) -> (DVec3, DVec3) {
    let e = &s.edges[g.edge];
    let t = e.t0 + (e.t1 - e.t0) * f;
    (
        e.curve.point(t),
        (e.curve.tangent(t) * (e.t1 - e.t0).signum()).normalize(),
    )
}

/// Berührpunkte am Bruchteil `f` des Bereichs.
fn contacts(g: &EdgeGeom, f: f64) -> (DVec3, DVec3) {
    let t = g.range.0 + (g.range.1 - g.range.0) * f;
    (g.contact_a.point(t), g.contact_b.point(t))
}

/// Gemeinsame Prüfungen: Berührpunkte auf ihren Flächen und auf der neuen Fläche, in der
/// Querschnittsebene des passenden Kantenpunkts; Querschnitte beginnen und enden dort.
fn check_contacts_common(s: &Solid, g: &EdgeGeom, what: &str) {
    let (fa, fb) = (&s.faces[g.sides.a.face], &s.faces[g.sides.b.face]);
    for f in STATIONS {
        let (pa, pb) = contacts(g, f);
        let (o, t) = edge_station(s, g, f);
        assert!(
            fa.surface.implicit(pa).abs() < ON_SURFACE,
            "{what} f={f}: A {pa:?}"
        );
        assert!(
            fb.surface.implicit(pb).abs() < ON_SURFACE,
            "{what} f={f}: B {pb:?}"
        );
        assert!(
            g.surface.implicit(pa).abs() < ON_SURFACE,
            "{what} f={f}: neue Fläche an A"
        );
        assert!(
            g.surface.implicit(pb).abs() < ON_SURFACE,
            "{what} f={f}: neue Fläche an B"
        );
        assert!(
            (pa - o).dot(t).abs() < ON_SURFACE,
            "{what} f={f}: A nicht im Querschnitt"
        );
        assert!(
            (pb - o).dot(t).abs() < ON_SURFACE,
            "{what} f={f}: B nicht im Querschnitt"
        );
    }
    for (seg, f) in [(&g.section_start, 0.0), (&g.section_end, 1.0)] {
        let (pa, pb) = contacts(g, f);
        assert!(
            seg.start().distance(pa) < SAME_CENTER,
            "{what}: Querschnitt {f} Anfang"
        );
        assert!(
            seg.end().distance(pb) < SAME_CENTER,
            "{what}: Querschnitt {f} Ende"
        );
    }
}

/// Rundung: gleiche Kugel aus beiden Berührpunkten, G1 mit beiden Nachbarn (Außenrichtung
/// gleich), Bogenmitte auf der Kugel und auf der kurzen Seite (zur Kante hin).
fn check_fillet(s: &Solid, g: &EdgeGeom, r: f64, what: &str) {
    check_contacts_common(s, g, what);
    let (fa, fb) = (&s.faces[g.sides.a.face], &s.faces[g.sides.b.face]);
    let into_ball = if g.convex { -r } else { r };
    let center = |f: f64| {
        let (pa, pb) = contacts(g, f);
        let ca = pa + outward_normal(fa, pa) * into_ball;
        let cb = pb + outward_normal(fb, pb) * into_ball;
        assert!(
            ca.distance(cb) < SAME_CENTER,
            "{what} f={f}: Kugeln {ca:?} {cb:?}"
        );
        ca
    };
    for f in STATIONS {
        let (pa, pb) = contacts(g, f);
        center(f);
        let (na, nb) = (outward_normal(fa, pa), outward_normal(fb, pb));
        assert!(
            blend_out(g, pa).distance(na) < SAME_CENTER,
            "{what} f={f}: G1/Richtung an A"
        );
        assert!(
            blend_out(g, pb).distance(nb) < SAME_CENTER,
            "{what} f={f}: G1/Richtung an B"
        );
    }
    for (seg, f) in [(&g.section_start, 0.0), (&g.section_end, 1.0)] {
        let (o, _) = edge_station(s, g, f);
        let c = center(f);
        let m = seg.at(0.5);
        assert!(
            (m.distance(c) - r).abs() < SAME_CENTER,
            "{what}: Bogenmitte nicht auf der Kugel"
        );
        assert!(
            m.distance(o) < c.distance(o),
            "{what}: Bogen auf der langen Seite"
        );
        assert!(
            g.surface.implicit(m).abs() < ON_SURFACE,
            "{what}: Bogenmitte neben der Fläche"
        );
    }
}

/// Fase: Außenrichtung aus dem Umlaufsinn des Randes im Querschnitt. An `p` läuft der Rand aus
/// der Nachbarfläche (örtliche Flächentangente `w`, von der Kante weg) über `p` in die Fase; die
/// Außennormale entsteht aus der Laufrichtung immer durch dieselbe Vierteldrehung.
fn check_chamfer(s: &Solid, g: &EdgeGeom, what: &str) {
    check_contacts_common(s, g, what);
    let (fa, fb) = (&s.faces[g.sides.a.face], &s.faces[g.sides.b.face]);
    for f in STATIONS {
        let (pa, pb) = contacts(g, f);
        let (o, t) = edge_station(s, g, f);
        for (p, q, face) in [(pa, pb, fa), (pb, pa, fb)] {
            let n = outward_normal(face, p);
            let mut w = n.cross(t).normalize();
            if w.dot(p - o) < 0.0 {
                w = -w;
            }
            // Laufrichtung vor `p` ist −w; `turn` dreht sie in die Außennormale `n`.
            let turn = (-w).cross(n);
            let expected = turn.cross((q - p).normalize());
            let got = blend_out(g, 0.5 * (p + q));
            assert!(
                got.distance(expected) < SAME_CENTER,
                "{what} f={f}: Fase zeigt {got:?} statt {expected:?}"
            );
        }
    }
}

#[derive(Default, Debug)]
struct Tally {
    ok: usize,
    refused: usize,
    concave_fillets: usize,
    torus_fillets: usize,
    torus_neighbours: usize,
    negative_sweeps: usize,
    reversed_faces: usize,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.ok += o.ok;
        self.refused += o.refused;
        self.concave_fillets += o.concave_fillets;
        self.torus_fillets += o.torus_fillets;
        self.torus_neighbours += o.torus_neighbours;
        self.negative_sweeps += o.negative_sweeps;
        self.reversed_faces += o.reversed_faces;
    }
}

/// Rundung, Fase mit zwei Abständen und Fase mit Abstand und Winkel an jeder Kante.
fn probe_body(s: &Solid, rng: &mut Lcg, what: &str, tally: &mut Tally) {
    for (e, _) in s.edges.iter() {
        let Ok(sides) = edge_sides(s, e, None) else {
            continue;
        };
        let Ok(angle) = edge_angle(s, e, &sides) else {
            continue;
        };
        let kinds = [
            SectionKind::Fillet {
                radius: rng.range(0.3, 0.8),
            },
            SectionKind::Chamfer(ChamferSpec::Distances {
                a: rng.range(0.3, 0.8),
                b: rng.range(0.3, 0.8),
            }),
            SectionKind::Chamfer(ChamferSpec::DistanceAngle {
                distance: rng.range(0.3, 0.8),
                angle: rng.range(20.0, 70.0).to_radians(),
            }),
        ];
        for kind in kinds {
            let label = format!("{what} Kante {e:?} {kind:?}");
            match analytic_geom(s, e, &sides, angle.convex, kind, &tol()) {
                Ok(Some(g)) => {
                    tally.ok += 1;
                    let faces = [&s.faces[sides.a.face], &s.faces[sides.b.face]];
                    if faces.iter().any(|f| !f.same_sense) {
                        tally.reversed_faces += 1;
                    }
                    if g.range.1 < g.range.0 {
                        tally.negative_sweeps += 1;
                    }
                    match kind {
                        SectionKind::Fillet { radius } => {
                            check_fillet(s, &g, radius, &label);
                            if !g.convex {
                                tally.concave_fillets += 1;
                            }
                            if matches!(g.surface, Surface::Torus { .. }) {
                                tally.torus_fillets += 1;
                            }
                            if faces
                                .iter()
                                .any(|f| matches!(f.surface, Surface::Torus { .. }))
                            {
                                tally.torus_neighbours += 1;
                            }
                        }
                        SectionKind::Chamfer(_) => check_chamfer(s, &g, &label),
                    }
                }
                Ok(None) => {}
                Err(_) => tally.refused += 1,
            }
        }
    }
}

fn extrusion(outer: ProfileLoop, h: f64) -> Solid {
    k().extrude(
        &Profile {
            outer,
            holes: vec![],
        },
        &Frame::WORLD,
        h,
    )
    .unwrap()
}

fn line(x: f64, y: f64) -> Segment {
    Segment::Line {
        to: DVec2::new(x, y),
    }
}

fn arc(cx: f64, cy: f64, sweep: f64) -> Segment {
    Segment::Arc {
        center: DVec2::new(cx, cy),
        sweep,
    }
}

/// Testkörperfamilien: gerade und Kreiskanten, konvex und konkav, Kegel und Torus.
fn families() -> Vec<(&'static str, Solid)> {
    let l_profile = ProfileLoop::polygon(&[
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 4.0),
        DVec2::new(4.0, 4.0),
        DVec2::new(4.0, 10.0),
        DVec2::new(0.0, 10.0),
    ])
    .unwrap();
    // Gerundete Außenecke (gegen den Uhrzeigersinn, konvex) und eine Kerbe im Uhrzeigersinn
    // (konkav).
    let arcs = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            line(12.0, 0.0),
            line(12.0, 5.0),
            arc(9.0, 5.0, PI / 2.0),
            line(7.0, 8.0),
            arc(5.0, 8.0, -PI),
            line(0.0, 8.0),
            line(0.0, 0.0),
        ],
    };
    // Drehkörper mit Rille (Torus nach innen) und gerundeter Schulter (Torus nach außen).
    let groove = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            line(6.0, 0.0),
            line(6.0, 3.0),
            arc(6.0, 5.0, -PI),
            line(6.0, 10.0),
            line(4.0, 10.0),
            arc(4.0, 12.0, PI / 2.0),
            line(0.0, 14.0),
        ],
    };
    // Abgestufter Drehkörper: konkave Kreiskante an der Stufe.
    let stepped = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            line(6.0, 0.0),
            line(6.0, 3.0),
            line(3.0, 3.0),
            line(3.0, 7.0),
            line(0.0, 7.0),
        ],
    };
    vec![
        ("Quader", k().make_box(DVec3::new(10.0, 8.0, 6.0)).unwrap()),
        ("Zylinder", k().make_cylinder(5.0, 8.0).unwrap()),
        ("Kegel", k().make_cone(4.0, 6.0).unwrap()),
        ("L-Profil", extrusion(l_profile, 5.0)),
        ("Bögen", extrusion(arcs, 4.0)),
        ("Rille", k().revolve(&groove, &Frame::WORLD).unwrap()),
        ("Stufe", k().revolve(&stepped, &Frame::WORLD).unwrap()),
    ]
}

#[test]
fn rolling_ball_probe_over_body_families() {
    let mut rng = Lcg(0x5EED_0003);
    let mut total = Tally::default();
    for (name, base) in families() {
        let axis = DVec3::new(
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
            rng.range(0.2, 1.0),
        )
        .normalize();
        let rotation = DMat4::from_translation(DVec3::new(3.0, -2.0, 1.5))
            * DMat4::from_axis_angle(axis, rng.range(0.3, 2.5));
        let mirror = DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0));
        let variants = [
            ("", DMat4::IDENTITY),
            (" gespiegelt", mirror),
            (" gedreht", rotation),
            (" gespiegelt und gedreht", rotation * mirror),
        ];
        let mut family = Tally::default();
        for (suffix, m) in variants {
            let s = k().transform(&base, &m).unwrap();
            assert!(validate(&s).is_ok(), "{name}{suffix}");
            probe_body(&s, &mut rng, &format!("{name}{suffix}"), &mut family);
        }
        assert!(
            family.ok > 0,
            "{name}: keine analytische Geometrie ({family:?})"
        );
        total.add(&family);
    }
    // Die Probe deckt die verlangten Fälle wirklich ab.
    assert!(total.concave_fillets > 0, "{total:?}");
    assert!(total.torus_fillets > 0, "{total:?}");
    assert!(total.torus_neighbours > 0, "{total:?}");
    assert!(total.negative_sweeps > 0, "{total:?}");
    assert!(total.reversed_faces > 0, "{total:?}");
}

/// Kreiskanten mit linkshändigem Rahmen (`z = −(x × y)`) und mit umgekehrtem Parameter
/// (`t1 < t0`): Die Berührkreise laufen trotzdem in Kantenrichtung.
#[test]
fn circle_edges_with_left_handed_or_reversed_parametrisation() {
    let base = k().make_cylinder(5.0, 8.0).unwrap();
    let mut left = base.clone();
    let mut reversed = base.clone();
    let ids: Vec<Id<Edge>> = base.edges.iter().map(|(id, _)| id).collect();
    for id in ids {
        if let Curve::Circle { frame, .. } = &mut left.edges[id].curve {
            frame.z = -frame.z;
        }
        let e = &mut reversed.edges[id];
        if let Curve::Circle { frame, .. } = &mut e.curve {
            frame.y = -frame.y;
            frame.z = -frame.z;
            e.t0 = -e.t0;
            e.t1 = -e.t1;
        }
    }
    let mut rng = Lcg(7);
    for (what, s) in [("linkshändig", &left), ("umgekehrt", &reversed)] {
        assert!(validate(s).is_ok(), "{what}");
        let mut tally = Tally::default();
        probe_body(s, &mut rng, what, &mut tally);
        assert!(tally.ok > 0, "{what}: {tally:?}");
        let g = geom(s, "Wand 1", "Deckel", SectionKind::Fillet { radius: 1.0 })
            .unwrap()
            .unwrap();
        let e = &s.edges[g.edge];
        let along = e.curve.point(e.t0 + 0.1 * (e.t1 - e.t0).signum()) - e.curve.point(e.t0);
        let dt = 0.1 * (g.range.1 - g.range.0).signum();
        let contact = g.contact_a.point(g.range.0 + dt) - g.contact_a.point(g.range.0);
        assert!(
            along.dot(contact) > 0.0,
            "{what}: Berührkreis läuft gegen die Kante"
        );
    }
}
