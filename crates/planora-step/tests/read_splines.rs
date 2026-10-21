//! STEP-Leser, Stufe 4: Freiformkurven und -flächen, Dreh- und Strangflächen (STEP-Import,
//! Aufgabe 5). Die Dateien unter `tests/data` sind von Hand geschrieben; die Erwartungen sind
//! Handrechnungen (Volumen, Radien), eine eigene De-Boor-Auswertung und Simpson-Integrale.

use planora_exchange::Progress;
use planora_kernel::boolean::face_probe_point;
use planora_kernel::{Curve, Face, Solid, Surface, validate};
use planora_math::nurbs::{KnotVector, NurbsCurve, NurbsSurface};
use planora_math::{DVec3, PI, TAU, Tolerance};
use planora_mesh::{Mesh, TessellationOptions, mass_properties, tessellate};
use planora_step::read::geom::{Skip, SurfaceGeom};
use planora_step::read::spline::{
    check_knots, clamp_knots, expand_knots, finish_nurbs_surface, implicit_knots,
    normalize_weights, revolution,
};
use planora_step::read::topo::{BuildOptions, BuiltSolid, build_solid};
use planora_step::read::units::{Units, context_units};
use planora_step::{ReadLimits, StepFile};
use std::ops::{Add, Mul};
use std::path::PathBuf;

fn data(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn units_of(file: &StepFile) -> Units {
    let rep = file.ids_of("ADVANCED_BREP_SHAPE_REPRESENTATION")[0];
    let e = file.entity(rep).expect("Darstellung");
    let ctx = e
        .arg("ADVANCED_BREP_SHAPE_REPRESENTATION", 2)
        .expect("Kontext")
        .reference()
        .expect("Verweis");
    context_units(file, ctx).expect("Einheiten")
}

/// Liest den Körper `name` (Name des `MANIFOLD_SOLID_BREP`).
fn body_of(text: &str, name: &str) -> Result<BuiltSolid, Skip> {
    let file = StepFile::parse(
        text.as_bytes().to_vec(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
    .expect("Datei lädt");
    let units = units_of(&file);
    let item = file
        .ids_of("MANIFOLD_SOLID_BREP")
        .iter()
        .copied()
        .find(|id| {
            let e = file.entity(*id).expect("Eintrag");
            e.arg("MANIFOLD_SOLID_BREP", 0)
                .ok()
                .and_then(|v| v.text().ok())
                == Some(name)
        })
        .unwrap_or_else(|| panic!("Körper {name} fehlt"));
    build_solid(
        &file,
        units,
        &[item],
        &BuildOptions {
            weld_tol_mm: units.weld_tol_mm(),
        },
        &Progress::new(),
    )
}

fn body(text: &str, name: &str) -> Solid {
    let b = body_of(text, name).unwrap_or_else(|Skip(s)| panic!("{name} ausgelassen: {s}"));
    validate(&b.solid).expect("gültig");
    b.solid
}

fn skip_of(text: &str, name: &str) -> String {
    match body_of(text, name) {
        Ok(_) => panic!("{name} sollte ausgelassen werden"),
        Err(Skip(s)) => s,
    }
}

fn mesh(solid: &Solid, chord: f64) -> Mesh {
    tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: chord,
        },
    )
    .expect("Netz")
}

fn volume(solid: &Solid, chord: f64) -> f64 {
    mass_properties(&mesh(solid, chord)).volume
}

fn assert_rel(got: f64, want: f64, rel: f64, what: &str) {
    assert!(
        ((got - want) / want).abs() <= rel,
        "{what}: {got} statt {want} (relativ {})",
        ((got - want) / want).abs()
    );
}

/// Zusammengesetzte Gauß-Legendre-Regel (5 Punkte je Stück).
fn gauss(a: f64, b: f64, pieces: usize, f: impl Fn(f64) -> f64) -> f64 {
    const X: [f64; 5] = [
        0.0,
        -0.538_469_310_105_683_1,
        0.538_469_310_105_683_1,
        -0.906_179_845_938_664,
        0.906_179_845_938_664,
    ];
    const W: [f64; 5] = [
        0.568_888_888_888_888_9,
        0.478_628_670_499_366_5,
        0.478_628_670_499_366_5,
        0.236_926_885_056_189_1,
        0.236_926_885_056_189_1,
    ];
    let h = (b - a) / pieces as f64;
    let mut s = 0.0;
    for k in 0..pieces {
        let m = a + h * (k as f64 + 0.5);
        for (x, w) in X.iter().zip(W) {
            s += w * f(m + 0.5 * h * x);
        }
    }
    s * 0.5 * h
}

/// Volumen des eingelesenen Körpers selbst (nicht seines Netzes), Gaußscher Satz:
/// V = ⅓ Σ ∫∫ p · n dA. Ebene Flächen: ⅓ (Ursprung · n) · Inhalt, der Inhalt aus den Rändern
/// (½ ∮ (p × dp) · n). Gekrümmte Flächen der Probekörper füllen ihren Parameterbereich ganz
/// (die Ränder laufen auf seinem Rand); dort ∫∫ p · (S_u × S_v) du dv mit zentralen Differenzen.
/// Ein Netz mit Sehne 0,001 wäre nur auf etwa 4·Sehne/(3·Radius) ≈ 1e-4 genau.
fn brep_volume(solid: &Solid) -> f64 {
    let mut total = 0.0;
    for (_, f) in solid.faces.iter() {
        let sign = if f.same_sense { 1.0 } else { -1.0 };
        if let Surface::Plane { frame } = &f.surface {
            let n = frame.z * sign;
            let mut area = 0.0;
            for lp in std::iter::once(&f.outer).chain(&f.inner) {
                for he in &lp.half_edges {
                    let e = &solid.edges[he.edge];
                    let (a, b) = if he.forward {
                        (e.t0, e.t1)
                    } else {
                        (e.t1, e.t0)
                    };
                    area += 0.5
                        * gauss(a, b, 400, |t| {
                            e.curve.point(t).cross(e.curve.tangent(t)).dot(n)
                        });
                }
            }
            total += frame.origin.dot(n) * area / 3.0;
            continue;
        }
        let r = f.patch.expect("Bereich");
        let (hu, hv) = (1e-6 * (r.u1 - r.u0), 1e-6 * (r.v1 - r.v0));
        let s = &f.surface;
        let flux = gauss(r.u0, r.u1, 24, |u| {
            gauss(r.v0, r.v1, 24, |v| {
                let su = (s.point(u + hu, v) - s.point(u - hu, v)) / (2.0 * hu);
                let sv = (s.point(u, v + hv) - s.point(u, v - hv)) / (2.0 * hv);
                s.point(u, v).dot(su.cross(sv))
            })
        });
        total += sign * flux / 3.0;
    }
    total
}

/// Volumen des Körpers relativ 1e-6 (aus dem Körper selbst) und das Netz (Sehne 0,001) dazu
/// passend bis auf seinen Sehnenfehler.
fn assert_volume(solid: &Solid, want: f64, what: &str) {
    assert_rel(brep_volume(solid), want, 1e-6, what);
    assert_rel(volume(solid, 0.001), want, 1e-3, what);
}

/// Umlaufzahl des Netzes um `q` (Summe der Raumwinkel / 4π): innen 1, außen 0.
fn winding(m: &Mesh, q: DVec3) -> f64 {
    let mut total = 0.0;
    for t in m.indices.chunks(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.positions[i as usize] - q);
        let (la, lb, lc) = (a.length(), b.length(), c.length());
        let num = a.dot(b.cross(c));
        let den = la * lb * lc + a.dot(b) * lc + b.dot(c) * la + c.dot(a) * lb;
        total += 2.0 * num.atan2(den);
    }
    total / (4.0 * PI)
}

/// Außennormale der Fläche in `p`.
fn outward(face: &Face, p: DVec3) -> DVec3 {
    let (u, v) = face.surface.project(p);
    let n = face.surface.normal(u, v);
    if face.same_sense { n } else { -n }
}

/// Jede Außennormale zeigt nach außen: Probepunkt + 0,1 · Normale liegt außerhalb, − 0,1 innen.
fn assert_normals_point_out(solid: &Solid, what: &str) {
    let m = mesh(solid, 0.001);
    let tol = Tolerance::default();
    for (id, f) in solid.faces.iter() {
        let p = face_probe_point(solid, id, &tol).expect("Probepunkt");
        let n = outward(f, p);
        assert!((n.length() - 1.0).abs() < 1e-9, "{what}: Normale {n}");
        let out = winding(&m, p + n * 0.1);
        let inside = winding(&m, p - n * 0.1);
        assert!(
            out < 0.5 && inside > 0.5,
            "{what}, Fläche {:?}: außen {out}, innen {inside}",
            f.tag
        );
    }
}

fn faces_of<'a>(
    solid: &'a Solid,
    pred: impl Fn(&Surface) -> bool + 'a,
) -> impl Iterator<Item = &'a Face> + 'a {
    solid
        .faces
        .iter()
        .map(|(_, f)| f)
        .filter(move |f| pred(&f.surface))
}

// ---------------------------------------------------------------------------------------------
// (a) Klemmen
// ---------------------------------------------------------------------------------------------

/// Eigene De-Boor-Auswertung (homogen) für beliebige, auch ungeklemmte Knoten.
fn de_boor<V>(p: usize, k: &[f64], c: &[V], u: f64) -> V
where
    V: Copy + Add<Output = V> + Mul<f64, Output = V>,
{
    let n = c.len();
    let mut s = p;
    while s + 1 < n && k[s + 1] <= u {
        s += 1;
    }
    let mut d: Vec<V> = (0..=p).map(|j| c[j + s - p]).collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = j + s - p;
            let a = (u - k[i]) / (k[i + p + 1 - r] - k[i]);
            d[j] = d[j - 1] * (1.0 - a) + d[j] * a;
        }
    }
    d[p]
}

fn check_clamp(weights: &[f64]) {
    let pts = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(2.0, 5.0, 1.0),
        DVec3::new(4.0, -1.0, 3.0),
        DVec3::new(7.0, 4.0, -2.0),
        DVec3::new(9.0, 0.5, 0.0),
        DVec3::new(12.0, 6.0, 2.5),
        DVec3::new(14.0, -3.0, 1.0),
    ];
    let knots: Vec<f64> = (0..=10).map(f64::from).collect();
    let hom: Vec<_> = pts
        .iter()
        .zip(weights)
        .map(|(p, w)| (*p * *w).extend(*w))
        .collect();
    let (k2, h2) = clamp_knots(3, &knots, &hom).expect("klemmbar");
    assert!(k2[..4].iter().all(|k| *k == 3.0), "Anfang {k2:?}");
    assert!(k2[k2.len() - 4..].iter().all(|k| *k == 7.0), "Ende {k2:?}");
    assert_eq!(k2.len(), h2.len() + 4);
    KnotVector::new(3, k2.clone()).expect("Kernel nimmt den Vektor an");
    let (control, w): (Vec<DVec3>, Vec<f64>) = h2.iter().map(|h| (h.truncate() / h.w, h.w)).unzip();
    let curve = NurbsCurve::new(3, k2.clone(), control, w).expect("Kurve");
    for i in 0..50 {
        let u = 3.0 + 4.0 * i as f64 / 49.0;
        let h = de_boor(3, &knots, &hom, u);
        let want = h.truncate() / h.w;
        let got = curve.point(u);
        assert!(
            got.distance(want) <= 1e-12,
            "u = {u}: {got} statt {want} (Gewichte {weights:?})"
        );
        let h2u = de_boor(3, &k2, &h2, u);
        assert!((h2u.truncate() / h2u.w).distance(want) <= 1e-12);
    }
}

#[test]
fn clamping_keeps_the_curve() {
    check_clamp(&[1.0; 7]);
    check_clamp(&[1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0]);
    // Schon geklemmt: unverändert.
    let k = vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0];
    let c: Vec<_> = (0..4)
        .map(|i| DVec3::new(i as f64, 0.0, 0.0).extend(1.0))
        .collect();
    let (k2, c2) = clamp_knots(2, &k, &c).unwrap();
    assert_eq!(k2, k);
    assert_eq!(c2, c);
    // Nur am Ende ungeklemmt (Anfang bleibt, Ende wird eingefügt).
    let k = vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0];
    let (k2, c2) = clamp_knots(2, &k, &c).unwrap();
    assert_eq!(k2, vec![0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0]);
    assert_eq!(c2.len(), 4);
    for i in 0..20 {
        let u = 2.0 * i as f64 / 19.0;
        let a = de_boor(2, &k, &c, u);
        let b = de_boor(2, &k2, &c2, u);
        assert!((a.truncate() / a.w).distance(b.truncate() / b.w) <= 1e-12);
    }
}

// ---------------------------------------------------------------------------------------------
// (b) Knotenformen
// ---------------------------------------------------------------------------------------------

#[test]
fn knot_forms() {
    assert_eq!(
        expand_knots(&[3, 1, 3], &[0., 0.5, 1.]).unwrap(),
        vec![0., 0., 0., 0.5, 1., 1., 1.]
    );
    assert_eq!(
        implicit_knots("QUASI_UNIFORM_KNOTS", 2, 4).unwrap(),
        vec![0., 0., 0., 1., 2., 2., 2.]
    );
    assert_eq!(
        implicit_knots("BEZIER_KNOTS", 3, 4).unwrap(),
        vec![0., 0., 0., 0., 1., 1., 1., 1.]
    );
    assert_eq!(
        implicit_knots("PIECEWISE_BEZIER_KNOTS", 2, 5).unwrap(),
        vec![0., 0., 0., 1., 1., 2., 2., 2.]
    );
    assert_eq!(
        implicit_knots("UNIFORM_KNOTS", 3, 11).unwrap(),
        (-3..=11).map(f64::from).collect::<Vec<_>>()
    );
    // Fallende Werte, zu viele Knoten, Vielfachheit 0, Bézier-Zahl passt nicht.
    assert!(expand_knots(&[2, 2], &[1.0, 0.0]).is_err());
    assert!(expand_knots(&[5000, 5001], &[0.0, 1.0]).is_err());
    assert!(expand_knots(&[5000, 5000], &[0.0, 1.0]).is_ok());
    assert!(expand_knots(&[2, 0, 2], &[0.0, 0.5, 1.0]).is_err());
    assert!(implicit_knots("BEZIER_KNOTS", 3, 5).is_err());
    let Skip(m) = expand_knots(&[2, 2], &[1.0, 0.0]).unwrap_err();
    assert_eq!(m, "Die Knoten fallen.");
    // Innerer Knoten öfter als der Grad, Knoten zu dicht, Zahl passt nicht, Grad 10.
    let reason = |d: usize, k: &[f64], n: usize| check_knots(d, k, n).unwrap_err().0;
    assert_eq!(
        reason(1, &[0.0, 0.0, 0.5, 0.5, 1.0, 1.0], 4),
        "Ein innerer Knoten kommt öfter vor, als der Grad erlaubt."
    );
    assert!(check_knots(2, &[0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0], 5).is_ok());
    assert_eq!(
        reason(1, &[0.0, 0.0, 1e-10, 1.0, 1.0], 3),
        "Zwei Knoten liegen zu nah beieinander."
    );
    assert_eq!(
        reason(1, &[0.0, 0.0, 1.0, 1.0], 3),
        "Die Zahl der Knoten passt nicht zu Grad und Kontrollpunkten."
    );
    assert_eq!(
        reason(10, &[0.0; 22], 11),
        "Der Grad 10 ist nicht erlaubt (1 bis 9)."
    );
}

// ---------------------------------------------------------------------------------------------
// (c) Rationale Kreise, rationaler Viertelzylinder, ungeklemmte periodische Kante
// ---------------------------------------------------------------------------------------------

#[test]
fn rational_circle_and_cylinder() {
    let text = data("rational_bspline.step");
    let s = body(&text, "Viertelzylinder");
    assert_eq!(s.faces.len(), 5);
    assert_volume(&s, PI * 100.0 / 4.0 * 5.0, "Viertelzylinder");
    // Die Bögen liegen auf dem Kreis R 10; ihre Gewichte sind normiert.
    let mut arcs = 0;
    for (_, e) in s.edges.iter() {
        let Curve::Nurbs(c) = &e.curve else { continue };
        arcs += 1;
        assert_eq!(c.weights().iter().copied().fold(0.0, f64::max), 1.0);
        for i in 0..=50 {
            let p = e.curve.point(e.t0 + (e.t1 - e.t0) * i as f64 / 50.0);
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!((r - 10.0).abs() <= 1e-12, "Bogenpunkt {p}: r = {r}");
        }
    }
    assert_eq!(arcs, 2);
    let mantle: Vec<&Face> = faces_of(&s, |f| matches!(f, Surface::Nurbs(_))).collect();
    assert_eq!(mantle.len(), 1);
    let Surface::Nurbs(d) = &mantle[0].surface else {
        unreachable!()
    };
    let ns = d.surface();
    assert_eq!(ns.counts(), (3, 2));
    let wmax = (0..3)
        .flat_map(|i| (0..2).map(move |j| (i, j)))
        .map(|(i, j)| ns.weight_at(i, j))
        .fold(0.0, f64::max);
    assert_eq!(wmax, 1.0);
    assert!((ns.weight_at(1, 0) - 0.5f64.sqrt()).abs() < 1e-15);
    assert!(!d.is_periodic_u() && !d.is_periodic_v());
    assert_normals_point_out(&s, "Viertelzylinder");

    // Flansch: ungeklemmte, periodische Kanten (UNIFORM_CURVE, einfach und zusammengesetzt).
    let f = body(&text, "Flansch");
    assert_eq!(f.faces.len(), 3);
    for (_, e) in f.edges.iter() {
        if let Curve::Nurbs(c) = &e.curve {
            assert_eq!(c.domain(), (0.0, 8.0));
            assert_eq!(e.curve.period(), Some(8.0));
            assert!((e.t1 - e.t0).abs() == 8.0);
            let start = f.vertices[e.start].point;
            assert!(e.curve.point(e.t0).distance(start) < 1e-12);
            assert!((start.x - 18.0).abs() < 1e-12 && start.y.abs() < 1e-12);
        }
    }
    let side: Vec<&Face> = faces_of(&f, |s| matches!(s, Surface::Nurbs(_))).collect();
    assert_eq!(side.len(), 1);
    let Surface::Nurbs(d) = &side[0].surface else {
        unreachable!()
    };
    assert!(d.is_periodic_u(), "Mantel des Flanschs geschlossen");
    assert_normals_point_out(&f, "Flansch");
    // Fläche unter der Kurve (Green): ∮ x dy über den geschlossenen Spline, mal Höhe 8.
    let Some(c) = f.edges.iter().find_map(|(_, e)| match &e.curve {
        Curve::Nurbs(c) => Some(c.clone()),
        _ => None,
    }) else {
        panic!("keine Freiformkante")
    };
    let area = simpson(0.0, 8.0, 80_000, |t| {
        let d = c.derivs(t, 1);
        d[0].x * d[1].y
    });
    assert_volume(&f, area * 8.0, "Flansch");

    // normalize_weights: durch das größte geteilt; zu verschieden → Skip.
    let mut w = [2.0, 1.0, 4.0];
    normalize_weights(&mut w).unwrap();
    assert_eq!(w, [0.5, 0.25, 1.0]);
    assert!(normalize_weights(&mut [1.0, 1e-7]).is_err());
    assert!(normalize_weights(&mut [1.0, 0.0]).is_err());
    assert!(normalize_weights(&mut [1.0, -1.0]).is_err());
}

#[test]
fn closed_curve_with_small_gap() {
    // Das Ende der oberen Kurve liegt 3,3e-6 neben ihrem Anfang (eine gemeinsame Ecke).
    let text = data("rational_bspline.step")
        .replace(
            "#1410,#1411,#1412),.UNSPECIFIED.,.T.,.F.);",
            "#1410,#1411,#1419),.UNSPECIFIED.,.T.,.F.);",
        )
        .replace(
            "ENDSEC;\nEND-ISO",
            "#1419=CARTESIAN_POINT('',(14.,14.00002,8.));\nENDSEC;\nEND-ISO",
        );
    assert!(text.contains("#1419=") && text.contains("#1411,#1419)"));
    // Genaue Datei (1e-6): Die Ecke liegt nicht auf beiden Enden – nicht „Länge null“.
    assert_eq!(
        skip_of(&text, "Flansch"),
        "Die Ecke bei (18,00; 0,00; 8,00) liegt nicht auf ihrer Kante."
    );
    // Grobe Datei (1e-3): Die Kurve wird innerhalb der Genauigkeit geschlossen.
    let coarse = text.replace("LENGTH_MEASURE(1.E-06)", "LENGTH_MEASURE(1.E-03)");
    let f = body(&coarse, "Flansch");
    let closed = f
        .edges
        .iter()
        .filter(|(_, e)| matches!(e.curve, Curve::Nurbs(_)))
        .all(|(_, e)| e.curve.period() == Some(8.0));
    assert!(closed);
}

fn simpson(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let n = n + n % 2;
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        let w = if i % 2 == 1 { 4.0 } else { 2.0 };
        s += w * f(a + h * i as f64);
    }
    s * h / 3.0
}

// ---------------------------------------------------------------------------------------------
// (d), (f) Drehflächen
// ---------------------------------------------------------------------------------------------

#[test]
fn revolution_becomes_analytic_where_exact() {
    let text = data("revolution_extrusion.step");
    let w = body(&text, "Welle");
    let cyl: Vec<&Face> = faces_of(&w, |s| matches!(s, Surface::Cylinder { .. })).collect();
    assert_eq!(cyl.len(), 1);
    let Surface::Cylinder { radius, frame } = &cyl[0].surface else {
        unreachable!()
    };
    assert!((radius - 8.0).abs() <= 1e-12, "Radius {radius}");
    assert!(frame.z.cross(DVec3::Z).length() < 1e-12);
    assert_volume(&w, PI * 64.0 * 20.0, "Welle");
    assert_normals_point_out(&w, "Welle");

    let k = body(&text, "Kegelstumpf");
    let cones: Vec<&Face> = faces_of(&k, |s| matches!(s, Surface::Cone { .. })).collect();
    assert_eq!(cones.len(), 1);
    let Surface::Cone { slope, .. } = &cones[0].surface else {
        unreachable!()
    };
    assert!((slope - 0.5).abs() <= 1e-12, "Steigung {slope}");
    assert_volume(&k, PI * 10.0 / 3.0 * (100.0 + 50.0 + 25.0), "Kegelstumpf");
    assert_normals_point_out(&k, "Kegelstumpf");
}

#[test]
fn flipped_is_measured_not_assumed() {
    let text = data("revolution_extrusion.step");
    let axis = "#1524=DIRECTION('',(0.,0.,1.));";
    let face = "#1603=ADVANCED_FACE('',(#1613),#1623,.F.);";
    assert!(text.contains(axis) && text.contains(face));
    let reference = brep_volume(&body(&text, "Welle"));
    // Achse umgekehrt: Die Normale der Drehfläche zeigt jetzt nach außen, also .T. für dieselbe
    // Außenseite – derselbe Körper.
    let turned = text
        .replace(axis, "#1524=DIRECTION('',(0.,0.,-1.));")
        .replace(face, "#1603=ADVANCED_FACE('',(#1613),#1623,.T.);");
    let w = body(&turned, "Welle");
    let v = brep_volume(&w);
    assert!(v > 0.0);
    assert_rel(v, reference, 1e-9, "umgekehrte Achse");
    assert_normals_point_out(&w, "Welle, umgekehrte Achse");
    // Nur die Achse umgekehrt: Außenseite und Rand passen nicht mehr – nie still übernommen.
    let wrong = text.replace(axis, "#1524=DIRECTION('',(0.,0.,-1.));");
    let m = skip_of(&wrong, "Welle");
    assert!(m.contains("verdreht"), "{m}");
    // Ebenso beim Kegel: .T. statt .F. passt nicht zur gemessenen Normale.
    let cone_wrong = text.replace(
        "#2603=ADVANCED_FACE('',(#2613),#2623,.F.);",
        "#2603=ADVANCED_FACE('',(#2613),#2623,.T.);",
    );
    let m = skip_of(&cone_wrong, "Kegelstumpf");
    assert!(m.contains("verdreht"), "{m}");
}

// ---------------------------------------------------------------------------------------------
// (e) Strangfläche einer B-Spline-Kurve
// ---------------------------------------------------------------------------------------------

fn bulge() -> NurbsCurve {
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0, 1.0],
        vec![
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(13.0, 2.0, 0.0),
            DVec3::new(11.0, 5.0, 0.0),
            DVec3::new(14.0, 8.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ],
        vec![1.0; 5],
    )
    .unwrap()
}

/// Fläche zwischen Spline und der Linie x = 10 (wie `freeform.rs::bulge_area`).
fn bulge_area() -> f64 {
    let c = bulge();
    simpson(0.0, 0.4, 4000, |t| {
        let d = c.derivs(t, 1);
        (d[0].x - 10.0) * d[1].y
    }) + simpson(0.4, 1.0, 6000, |t| {
        let d = c.derivs(t, 1);
        (d[0].x - 10.0) * d[1].y
    })
}

#[test]
fn extrusion_of_a_spline_is_bounded_by_its_loops() {
    let text = data("revolution_extrusion.step");
    let p = body(&text, "Profil");
    assert_eq!(p.faces.len(), 6);
    let side: Vec<&Face> = faces_of(&p, |s| matches!(s, Surface::Nurbs(_))).collect();
    assert_eq!(side.len(), 1);
    let f = side[0];
    let patch = f.patch.expect("Bereich");
    let um = 0.5 * (patch.u0 + patch.u1);
    let z0 = f.surface.point(um, patch.v0).z;
    let z1 = f.surface.point(um, patch.v1).z;
    assert!(z0.abs() <= 1e-6 && (z1 - 12.0).abs() <= 1e-6, "{z0} … {z1}");
    // Die Fläche selbst reicht kaum über den Rand hinaus (Rand 1e-6 relativ).
    let Surface::Nurbs(d) = &f.surface else {
        unreachable!()
    };
    let (_, (v0, v1)) = d.surface().domain();
    let (b0, b1) = (f.surface.point(um, v0).z, f.surface.point(um, v1).z);
    assert!(
        (-1e-4..=0.0).contains(&b0) && (12.0..=12.0 + 1e-4).contains(&b1),
        "{b0} … {b1}"
    );
    assert_volume(&p, (100.0 + bulge_area()) * 12.0, "Profil");
    assert_normals_point_out(&p, "Profil");
}

// ---------------------------------------------------------------------------------------------
// (g) Nicht unterstützte Fälle und Pole
// ---------------------------------------------------------------------------------------------

/// Tetraeder O, A = 10x, B = 10y, D = 10z; die schräge Fläche A–B–D ist eine bilineare
/// B-Spline-Fläche, deren erste Spalte (u = 0) im Punkt D zusammenfällt.
fn tetra() -> String {
    let head = data("rational_bspline.step");
    let (head, _) = head.split_once("/* ===== Viertelzylinder").expect("Kopf");
    let head = head
        .replace(
            "#9=ADVANCED_BREP_SHAPE_REPRESENTATION('Viertelzylinder',(#20),#105);\n",
            "#9=ADVANCED_BREP_SHAPE_REPRESENTATION('Tetraeder',(#20),#105);\n",
        )
        .replace(
            "#19=ADVANCED_BREP_SHAPE_REPRESENTATION('Flansch',(#30),#105);\n",
            "",
        );
    format!(
        "{head}#20=MANIFOLD_SOLID_BREP('Tetraeder',#21);
#21=CLOSED_SHELL('',(#601,#602,#603,#604));
#601=ADVANCED_FACE('',(#611),#621,.F.);
#611=FACE_OUTER_BOUND('',#631,.T.);
#631=EDGE_LOOP('',(#701,#702,#703));
#701=ORIENTED_EDGE('',*,*,#562,.T.);
#702=ORIENTED_EDGE('',*,*,#564,.F.);
#703=ORIENTED_EDGE('',*,*,#561,.F.);
#602=ADVANCED_FACE('',(#612),#622,.F.);
#612=FACE_OUTER_BOUND('',#632,.T.);
#632=EDGE_LOOP('',(#704,#705,#706));
#704=ORIENTED_EDGE('',*,*,#561,.T.);
#705=ORIENTED_EDGE('',*,*,#566,.F.);
#706=ORIENTED_EDGE('',*,*,#563,.F.);
#603=ADVANCED_FACE('',(#613),#623,.F.);
#613=FACE_OUTER_BOUND('',#633,.T.);
#633=EDGE_LOOP('',(#707,#708,#709));
#707=ORIENTED_EDGE('',*,*,#563,.T.);
#708=ORIENTED_EDGE('',*,*,#565,.F.);
#709=ORIENTED_EDGE('',*,*,#562,.F.);
#604=ADVANCED_FACE('',(#614),#591,.T.);
#614=FACE_OUTER_BOUND('',#634,.T.);
#634=EDGE_LOOP('',(#710,#711,#712));
#710=ORIENTED_EDGE('',*,*,#564,.T.);
#711=ORIENTED_EDGE('',*,*,#565,.T.);
#712=ORIENTED_EDGE('',*,*,#566,.T.);
#621=PLANE('',#531);
#622=PLANE('',#533);
#623=PLANE('',#534);
#591=B_SPLINE_SURFACE_WITH_KNOTS('',1,1,((#504,#504),(#502,#503)),.UNSPECIFIED.,.F.,.F.,.F.,
  (2,2),(2,2),(0.,1.),(0.,1.),.UNSPECIFIED.);
#561=EDGE_CURVE('',#511,#512,#551,.T.);
#562=EDGE_CURVE('',#511,#513,#552,.T.);
#563=EDGE_CURVE('',#511,#514,#553,.T.);
#564=EDGE_CURVE('',#512,#513,#554,.T.);
#565=EDGE_CURVE('',#513,#514,#555,.T.);
#566=EDGE_CURVE('',#514,#512,#556,.T.);
#551=LINE('',#501,#541);
#552=LINE('',#501,#542);
#553=LINE('',#501,#543);
#554=LINE('',#502,#544);
#555=LINE('',#503,#545);
#556=LINE('',#504,#546);
#541=VECTOR('',#521,1.);
#542=VECTOR('',#522,1.);
#543=VECTOR('',#523,1.);
#544=VECTOR('',#524,1.);
#545=VECTOR('',#525,1.);
#546=VECTOR('',#526,1.);
#511=VERTEX_POINT('',#501);
#512=VERTEX_POINT('',#502);
#513=VERTEX_POINT('',#503);
#514=VERTEX_POINT('',#504);
#501=CARTESIAN_POINT('',(0.,0.,0.));
#502=CARTESIAN_POINT('',(10.,0.,0.));
#503=CARTESIAN_POINT('',(0.,10.,0.));
#504=CARTESIAN_POINT('',(0.,0.,10.));
#521=DIRECTION('',(1.,0.,0.));
#522=DIRECTION('',(0.,1.,0.));
#523=DIRECTION('',(0.,0.,1.));
#524=DIRECTION('',(-1.,1.,0.));
#525=DIRECTION('',(0.,-1.,1.));
#526=DIRECTION('',(1.,0.,-1.));
#531=AXIS2_PLACEMENT_3D('',#501,#523,#521);
#533=AXIS2_PLACEMENT_3D('',#501,#522,#521);
#534=AXIS2_PLACEMENT_3D('',#501,#521,#522);
ENDSEC;
END-ISO-10303-21;
"
    )
}

#[test]
fn unsupported_spline_cases_are_named() {
    let text = data("rational_bspline.step");
    // Grad 10 der Fläche bzw. einer Kurve.
    let m = skip_of(
        &text.replace("B_SPLINE_SURFACE(2,1,", "B_SPLINE_SURFACE(10,1,"),
        "Viertelzylinder",
    );
    assert!(
        m.contains("Eine Freiformfläche lässt sich nicht übernehmen: Der Grad 10"),
        "{m}"
    );
    let m = skip_of(
        &text.replace("B_SPLINE_CURVE(2,(#502", "B_SPLINE_CURVE(10,(#502"),
        "Viertelzylinder",
    );
    assert!(
        m.contains("Eine Freiformkurve lässt sich nicht übernehmen: Der Grad 10"),
        "{m}"
    );
    // Gewicht 0.
    let m = skip_of(
        &text.replace("(((2.,2.),(1.4142", "(((0.,2.),(1.4142"),
        "Viertelzylinder",
    );
    assert!(m.contains("Freiformfläche") && m.contains("Gewicht"), "{m}");
    // Fallende Knoten.
    let falling = text.replace(
        "B_SPLINE_SURFACE_WITH_KNOTS((3,3),(2,2),(0.,1.),(0.,1.),",
        "B_SPLINE_SURFACE_WITH_KNOTS((3,3),(2,2),(0.,1.),(1.,0.),",
    );
    assert_ne!(falling, text);
    let m = skip_of(&falling, "Viertelzylinder");
    assert!(
        m.ends_with("Eine Freiformfläche lässt sich nicht übernehmen: Die Knoten fallen."),
        "{m}"
    );
    // Das Kennzeichen „geschlossen in u“ zählt nicht: entscheidend sind die Kontrollreihen.
    let flag = text.replace(
        ".UNSPECIFIED.,.F.,.F.,.F.)\n  B_SPLINE_SURFACE_WITH_KNOTS",
        ".UNSPECIFIED.,.T.,.F.,.F.)\n  B_SPLINE_SURFACE_WITH_KNOTS",
    );
    assert_ne!(flag, text);
    let s = body(&flag, "Viertelzylinder");
    let nurbs = faces_of(&s, |f| matches!(f, Surface::Nurbs(d) if !d.is_periodic_u())).count();
    assert_eq!(nurbs, 1);

    // Pol in u-Richtung: gelesen mit getauschten Richtungen, Außennormale nach außen.
    let t = tetra();
    let s = body(&t, "Tetraeder");
    assert_volume(&s, 1000.0 / 6.0, "Tetraeder");
    let face: Vec<&Face> = faces_of(&s, |f| matches!(f, Surface::Nurbs(_))).collect();
    assert_eq!(face.len(), 1);
    let Surface::Nurbs(d) = &face[0].surface else {
        unreachable!()
    };
    assert_eq!(d.poles_v().len(), 1, "Pol nach dem Tausch in v");
    let p = DVec3::new(10.0, 10.0, 10.0) / 3.0;
    let n = outward(face[0], p);
    assert!(
        n.distance(DVec3::ONE.normalize()) < 1e-9,
        "Außennormale {n}"
    );
    assert_normals_point_out(&s, "Tetraeder");
}

#[test]
fn periodic_rows_follow_the_geometry() {
    // Voller Zylindermantel als Drehfläche einer Strecke: erste und letzte Reihe gleich.
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 3.0)],
        vec![1.0, 1.0],
    )
    .unwrap();
    let ns = NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let (s, swapped) = finish_nurbs_surface(ns.clone(), 1e-3).unwrap();
    assert!(!swapped);
    let Surface::Nurbs(d) = &s else { panic!() };
    assert!(d.is_periodic_u() && !d.is_periodic_v());

    // Letzte Reihe verschoben: bis zur Genauigkeit gemittelt und geschlossen, darüber offen.
    let moved = |by: f64| {
        let (nu, nv) = ns.counts();
        let mut c = Vec::new();
        let mut w = Vec::new();
        for i in 0..nu {
            for j in 0..nv {
                let shift = if i == nu - 1 {
                    DVec3::Y * by
                } else {
                    DVec3::ZERO
                };
                c.push(ns.control_at(i, j) + shift);
                w.push(ns.weight_at(i, j));
            }
        }
        NurbsSurface::new(
            2,
            1,
            ns.knots_u().knots().to_vec(),
            ns.knots_v().knots().to_vec(),
            nu,
            nv,
            c,
            w,
        )
        .unwrap()
    };
    let (s, _) = finish_nurbs_surface(moved(1e-5), 1e-3).unwrap();
    let Surface::Nurbs(d) = &s else { panic!() };
    assert!(d.is_periodic_u());
    let g = d.surface();
    let nu = g.counts().0;
    assert_eq!(g.control_at(0, 0), g.control_at(nu - 1, 0));
    assert!((g.control_at(0, 0).y - 5e-6).abs() < 1e-12);
    let (s, _) = finish_nurbs_surface(moved(2e-3), 1e-3).unwrap();
    let Surface::Nurbs(d) = &s else { panic!() };
    assert!(!d.is_periodic_u());
    let (s, _) = finish_nurbs_surface(moved(1e-5), 1e-6).unwrap();
    let Surface::Nurbs(d) = &s else { panic!() };
    assert!(!d.is_periodic_u());

    // Spalte zusammengefallen (Pol in u-Richtung): getauscht.
    let tri = NurbsSurface::new(
        1,
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![0.0, 0.0, 1.0, 1.0],
        2,
        2,
        vec![DVec3::Z, DVec3::Z, DVec3::X, DVec3::Y],
        vec![1.0; 4],
    )
    .unwrap();
    let (s, swapped) = finish_nurbs_surface(tri, 1e-6).unwrap();
    assert!(swapped);
    let Surface::Nurbs(d) = &s else { panic!() };
    assert_eq!(d.poles_v(), &[0.0]);
    // Pol in beiden Richtungen → Meldung des Kernels.
    let both = NurbsSurface::new(
        1,
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![0.0, 0.0, 1.0, 1.0],
        2,
        2,
        vec![DVec3::Z, DVec3::Z, DVec3::Z, DVec3::Y],
        vec![1.0; 4],
    )
    .unwrap();
    let Err(Skip(m)) = finish_nurbs_surface(both, 1e-6) else {
        panic!("sollte scheitern")
    };
    assert!(m.contains("Pol"), "{m}");
}

#[test]
fn revolution_of_a_freeform_profile_is_nurbs() {
    // Ellipse (Halbachsen 3 in x, 2 in z) um (10; 0; 0) in der x-z-Ebene, um die z-Achse gedreht:
    // kein analytischer Fall. STEP-Normale (Kurve × Drehung) am äußersten Punkt: zur Achse hin.
    let ellipse = Curve::Ellipse {
        frame: planora_math::Frame {
            origin: DVec3::new(10.0, 0.0, 0.0),
            x: DVec3::X,
            y: DVec3::Z,
            z: DVec3::NEG_Y,
        },
        major: 3.0,
        minor: 2.0,
    };
    let g = revolution(&ellipse, DVec3::ZERO, DVec3::Z, 1e-6).expect("Drehfläche");
    let SurfaceGeom::Ready { surface, flipped } = g else {
        panic!("fertige Fläche erwartet")
    };
    let Surface::Nurbs(d) = &surface else {
        panic!("Freiformfläche erwartet")
    };
    assert!(d.is_periodic_u() && d.is_periodic_v());
    for (p, inward) in [
        (DVec3::new(13.0, 0.0, 0.0), DVec3::NEG_X),
        (DVec3::new(0.0, 7.0, 0.0), DVec3::Y),
        (DVec3::new(-10.0, 0.0, 2.0), DVec3::NEG_Z),
    ] {
        let (u, v) = surface.project(p);
        assert!(surface.point(u, v).distance(p) < 1e-9);
        let n = surface.normal(u, v) * if flipped { -1.0 } else { 1.0 };
        assert!(n.dot(inward) > 0.99, "{p}: {n}");
    }
    // Dieselbe Ellipse andersherum durchlaufen: die STEP-Normale zeigt nach außen.
    let reversed = Curve::Ellipse {
        frame: planora_math::Frame {
            origin: DVec3::new(10.0, 0.0, 0.0),
            x: DVec3::X,
            y: DVec3::NEG_Z,
            z: DVec3::Y,
        },
        major: 3.0,
        minor: 2.0,
    };
    let SurfaceGeom::Ready { surface, flipped } =
        revolution(&reversed, DVec3::ZERO, DVec3::Z, 1e-6).expect("Drehfläche")
    else {
        panic!("fertige Fläche erwartet")
    };
    let p = DVec3::new(13.0, 0.0, 0.0);
    let (u, v) = surface.project(p);
    let n = surface.normal(u, v) * if flipped { -1.0 } else { 1.0 };
    assert!(n.dot(DVec3::X) > 0.99, "{n}");
}
