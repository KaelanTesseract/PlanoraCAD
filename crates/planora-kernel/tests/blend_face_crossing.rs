//! Durchdringung im Flächeninneren (Teilprojekt 8, Korrekturwelle nach der Gesamtprüfung, K1):
//! Eine behaltene Fläche, deren Inneres die neue Rundungsfläche schneidet, deren Kanten aber
//! nicht, muss erkannt werden. Früher lieferte `blend` hier einen „gültigen“ Körper, dessen
//! Rundungsfläche durch den Hohlraum einer Kugeltasche lief.

use planora_kernel::blend::BlendError;
use planora_kernel::*;
use planora_math::{DMat4, DVec3, PI};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_at(s: &Solid, p0: DVec3, p1: DVec3) -> Id<Edge> {
    let near = |v: Id<Vertex>, p: DVec3| s.vertices[v].point.distance(p) < 1e-9;
    s.edges
        .iter()
        .find(|(_, e)| {
            (near(e.start, p0) && near(e.end, p1)) || (near(e.start, p1) && near(e.end, p0))
        })
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("keine Kante {p0} – {p1}"))
}

fn fillet(s: &Solid, e: Id<Edge>, radius: f64) -> Result<Blended, BlendError> {
    k().blend(
        s,
        &BlendRequest {
            edges: vec![BlendEdge {
                edge: e,
                first_face: None,
            }],
            kind: BlendKind::Fillet { radius },
            tangent_chain: true,
        },
    )
}

/// Würfel 10 minus Kugel `radius` um `center`; `turn` dreht die Kugel vorher (andere Lage von
/// Naht und Polen).
fn pocket(center: DVec3, radius: f64, turn: DMat4) -> Solid {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let ball = k().make_sphere(radius).unwrap();
    let ball = k()
        .transform(&ball, &(DMat4::from_translation(center) * turn))
        .unwrap();
    let s = k().boolean(&cube, &ball, BoolOp::Difference).unwrap();
    assert_eq!(
        s.shells.iter().filter(|sh| !sh.faces.is_empty()).count(),
        1,
        "die Tasche soll zum Rand offen sein"
    );
    s
}

/// Abgelehnt mit einer benannten Meldung (nie ein Körper).
fn assert_refused(r: Result<Blended, BlendError>, what: &str) {
    match r {
        Err(BlendError::Unsupported(m)) => {
            eprintln!("{what}: {m}");
            assert!(m.contains("Fläche"), "{what}: {m}")
        }
        Err(e) => panic!("{what}: unerwartete Meldung {e}"),
        Ok(b) => panic!(
            "{what}: Körper mit {} Flächen geliefert, die Rundungsfläche läuft durch die Tasche",
            b.solid.faces.len()
        ),
    }
}

#[test]
fn a_pocket_crossing_the_fillet_interior_is_refused() {
    // Kugel r = 1,2 im Abstand 1 von der Achse der Rundung (2, 2, z), bricht oben durch den
    // Deckel. Der Punkt (2 − √2; 2 − √2; 9,2) liegt auf der Rundungsfläche, aber nur 1,0 vom
    // Kugelmittelpunkt entfernt – also im Hohlraum. Keine Kante der Kugeltasche trifft die
    // Rundungsfläche.
    let d = 1.0 / 2f64.sqrt();
    let s = pocket(DVec3::new(2.0 - d, 2.0 - d, 9.2), 1.2, DMat4::IDENTITY);
    let e = edge_at(&s, DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0));
    assert_refused(fillet(&s, e, 2.0), "senkrechte Kante");
}

#[test]
fn a_turned_pocket_in_the_end_wall_is_refused() {
    // Kante oben (0,0,10)–(0,10,10), Achse der Rundung (2, y, 8); gedrehte Kugel r = 1,2 im
    // Abstand 1 von der Achse, bricht durch die Stirnwand y = 0 (Abschlussfläche der Rundung).
    let d = 1.0 / 2f64.sqrt();
    let s = pocket(
        DVec3::new(2.0 - d, 0.8, 8.0 + d),
        1.2,
        DMat4::from_rotation_x(PI / 2.0),
    );
    let e = edge_at(&s, DVec3::new(0.0, 0.0, 10.0), DVec3::new(0.0, 10.0, 10.0));
    assert_refused(fillet(&s, e, 2.0), "Kante oben");
}

#[test]
fn pockets_clear_of_the_fillet_still_work() {
    // Kugel auf der Achse der Rundung, r = 1,9 < 2: berührt die Rundungsfläche nicht
    // (exakt prüfbares Paar Kugel/Zylinder gleicher Achse).
    let s = pocket(DVec3::new(2.0, 2.0, 9.2), 1.9, DMat4::IDENTITY);
    let e = edge_at(&s, DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0));
    let b = fillet(&s, e, 2.0).unwrap();
    k().validate(&b.solid).unwrap();
    // Kugel weit weg von der Rundung (andere Ecke des Deckels).
    let s = pocket(DVec3::new(8.0, 8.0, 9.2), 1.2, DMat4::IDENTITY);
    let e = edge_at(&s, DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0));
    let b = fillet(&s, e, 2.0).unwrap();
    k().validate(&b.solid).unwrap();
}

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

/// Stichproben im Inneren einer Fläche (Raster über einen weiten Parameterbereich).
fn face_samples(s: &Solid, face: Id<Face>, grid: usize) -> Vec<DVec3> {
    use planora_kernel::boolean::{Containment, face_geom, point_in_face};
    let g = face_geom(s, face);
    let (u0, u1, v0, v1) = match g.surface {
        Surface::Plane { .. } => (-20.0, 20.0, -20.0, 20.0),
        Surface::Sphere { .. } => (-PI, PI, -PI / 2.0, PI / 2.0),
        Surface::Torus { .. } => (-PI, PI, -PI, PI),
        _ => (-PI, PI, -20.0, 20.0),
    };
    let tol = planora_math::Tolerance::default();
    let mut out = Vec::new();
    for a in 0..=grid {
        for c in 0..=grid {
            let u = u0 + (u1 - u0) * a as f64 / grid as f64;
            let v = v0 + (v1 - v0) * c as f64 / grid as f64;
            let p = g.surface.point(u, v);
            if point_in_face(&g, p, &tol) == Containment::Inside {
                out.push(p);
            }
        }
    }
    out
}

/// Zufällige Kugeltaschen nahe der senkrechten Kante (0,0)–(0,10), zum Deckel offen; Rundung oder
/// Fase an der Kante. Jedes Ok-Ergebnis wird gegen die Geometrie geprüft: Kein Punkt der neuen
/// Fläche liegt im Hohlraum der Kugel, kein Punkt der Kugelfläche im abgetragenen Bereich.
/// Liefert (Ok, abgelehnt, davon „nicht sicher prüfbar“).
fn random_pockets(cases: usize, seed: u64, grid: usize) -> (usize, usize, usize) {
    let mut rng = Lcg(seed);
    let (mut ok, mut refused, mut unknown) = (0, 0, 0);
    for case in 0..cases {
        let radius = rng.range(0.3, 1.6);
        let center = DVec3::new(
            rng.range(0.4, 3.5),
            rng.range(0.4, 3.5),
            rng.range(10.2 - radius, 9.9 + 0.5 * radius),
        );
        let size = rng.range(0.5, 3.0);
        let chamfer = case % 3 == 2;
        let cube = k().make_box(DVec3::splat(10.0)).unwrap();
        let ball = k()
            .transform(
                &k().make_sphere(radius).unwrap(),
                &DMat4::from_translation(center),
            )
            .unwrap();
        let Ok(s) = k().boolean(&cube, &ball, BoolOp::Difference) else {
            continue;
        };
        let Some(e) = s.edges.iter().map(|(id, _)| id).find(|&id| {
            let ed = &s.edges[id];
            let (a, b) = (s.vertices[ed.start].point, s.vertices[ed.end].point);
            a.x.abs() < 1e-9 && a.y.abs() < 1e-9 && b.x.abs() < 1e-9 && b.y.abs() < 1e-9
        }) else {
            continue;
        };
        let kind = if chamfer {
            BlendKind::ChamferEqual { distance: size }
        } else {
            BlendKind::Fillet { radius: size }
        };
        let r = k().blend(
            &s,
            &BlendRequest {
                edges: vec![BlendEdge {
                    edge: e,
                    first_face: None,
                }],
                kind,
                tangent_chain: true,
            },
        );
        let b = match r {
            Ok(b) => b,
            Err(e) => {
                refused += 1;
                if e.to_string().contains("nicht sicher prüfen") {
                    unknown += 1;
                }
                continue;
            }
        };
        ok += 1;
        let out = &b.solid;
        k().validate(out).unwrap();
        // Abgetragener Bereich an der Kante (x, y < size).
        let removed = |p: DVec3| {
            p.x < size - 1e-6
                && p.y < size - 1e-6
                && if chamfer {
                    p.x + p.y < size - 1e-6
                } else {
                    p.truncate().distance(planora_math::DVec2::splat(size)) > size + 1e-6
                }
        };
        for (id, f) in out.faces.iter() {
            let role = f.tag.as_ref().map(|t| t.role.as_str()).unwrap_or("");
            if role.starts_with("Rundung") || role.starts_with("Fase") {
                for p in face_samples(out, id, grid) {
                    assert!(
                        p.distance(center) >= radius - 1e-6,
                        "Fall {case}: {p} der neuen Fläche liegt im Hohlraum (Kugel {center} r {radius}, Maß {size})"
                    );
                }
            } else if matches!(f.surface, Surface::Sphere { .. }) {
                for p in face_samples(out, id, grid) {
                    assert!(
                        !removed(p),
                        "Fall {case}: {p} der Kugelfläche liegt im abgetragenen Bereich (Kugel {center} r {radius}, Maß {size})"
                    );
                }
            }
        }
    }
    (ok, refused, unknown)
}

#[test]
fn seeded_pockets_small() {
    let (ok, refused, unknown) = random_pockets(12, 0x5eed_0b01, 24);
    eprintln!("Kugeltaschen: {ok} Ok, {refused} abgelehnt, davon {unknown} nicht prüfbar");
    assert!(ok > 0 && refused > 0, "{ok} / {refused}");
}

#[test]
#[ignore = "300 zufällige Kugeltaschen, nur mit --release --ignored (etwa 15 s)"]
fn seeded_pockets_300() {
    let (ok, refused, unknown) = random_pockets(300, 0x5eed_0b02, 60);
    eprintln!("Kugeltaschen: {ok} Ok, {refused} abgelehnt, davon {unknown} nicht prüfbar");
}
