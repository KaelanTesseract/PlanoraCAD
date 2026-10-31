//! STEP-Leser, Stufe 5: Nähte, Pole, Entartungen und Hohlräume (STEP-Import, Aufgabe 6). Die
//! Dateien unter `tests/data` sind von Hand geschrieben; die Erwartungen sind Handrechnungen.

mod common;

use common::export;
use planora_exchange::Progress;
use planora_kernel::boolean::{BoolOp, face_probe_point};
use planora_kernel::{AnalyticKernel, Curve, Face, FaceTag, Kernel, Solid, Surface, validate};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec3, Frame, PI, TAU, Tolerance};
use planora_mesh::{Mesh, TessellationOptions, mass_properties, tessellate};
use planora_step::read::geom::{Resolver, Skip, SurfaceGeom};
use planora_step::read::seam::drop_degenerate;
use planora_step::read::spline::{finish_cone, revolution};
use planora_step::read::topo::{
    BuildOptions, BuiltSolid, RawBody, RawEdge, RawFace, RawHalfEdge, STEP_FACE_ROLE, build_solid,
    derive_patch_sensed, read_raw,
};
use planora_step::read::units::{Units, context_units};
use planora_step::{ReadLimits, StepFile};
use std::f64::consts::FRAC_PI_2;
use std::path::PathBuf;

fn data(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn parse(text: &str) -> StepFile {
    StepFile::parse(
        text.as_bytes().to_vec(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
    .expect("Datei lädt")
}

/// Einheiten aus dem Kontext der Darstellung `rep`.
fn units_of(file: &StepFile, rep: u64) -> Units {
    let e = file.entity(rep).expect("Darstellung");
    let ctx = e
        .arg("ADVANCED_BREP_SHAPE_REPRESENTATION", 2)
        .expect("Kontext")
        .reference()
        .expect("Verweis");
    context_units(file, ctx).expect("Einheiten")
}

/// Darstellung mit dem Namen `name` und ihre Volumen-Einträge.
fn representation(file: &StepFile, name: &str) -> (u64, Vec<u64>) {
    for &rep in file.ids_of("ADVANCED_BREP_SHAPE_REPRESENTATION") {
        let e = file.entity(rep).expect("Darstellung");
        const R: &str = "ADVANCED_BREP_SHAPE_REPRESENTATION";
        if e.arg(R, 0).ok().and_then(|v| v.text().ok()) != Some(name) {
            continue;
        }
        let items = e
            .arg(R, 1)
            .expect("Liste")
            .refs()
            .expect("Verweise")
            .into_iter()
            .filter(|id| {
                let it = file.entity(*id).expect("Eintrag");
                it.is("MANIFOLD_SOLID_BREP") || it.is("BREP_WITH_VOIDS")
            })
            .collect();
        return (rep, items);
    }
    panic!("Darstellung {name} fehlt")
}

/// Liest den Körper der Darstellung `name`.
fn body_of(text: &str, name: &str) -> Result<BuiltSolid, Skip> {
    let file = parse(text);
    let (rep, items) = representation(&file, name);
    let units = units_of(&file, rep);
    build_solid(
        &file,
        units,
        &items,
        &BuildOptions {
            weld_tol_mm: units.weld_tol_mm(),
        },
        &Progress::new(),
    )
}

fn body(text: &str, name: &str) -> Solid {
    let b = body_of(text, name).unwrap_or_else(|Skip(s)| panic!("{name} ausgelassen: {s}"));
    validate(&b.solid).unwrap_or_else(|e| panic!("{name} ungültig: {e}"));
    b.solid
}

fn skip_of(text: &str, name: &str) -> String {
    match body_of(text, name) {
        Ok(_) => panic!("{name} sollte ausgelassen werden"),
        Err(Skip(s)) => s,
    }
}

fn swap(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from} nicht genau einmal");
    text.replacen(from, to, 1)
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

fn volume(solid: &Solid) -> f64 {
    mass_properties(&mesh(solid, 0.001)).volume
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

/// Volumen des eingelesenen Körpers selbst (wie in `read_splines.rs`): Gaußscher Satz, ebene
/// Flächen über ihre Ränder, gekrümmte über ihren Parameterbereich (die Probekörper füllen ihn
/// ganz). Ein Netz mit Sehne 0,001 wäre nur auf etwa 4·Sehne/(3·Radius) genau.
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

/// Volumen aus dem Körper selbst relativ 1e-6, das Netz (Sehne 0,001) bis auf seinen
/// Sehnenfehler (1e-3).
fn assert_volume(solid: &Solid, want: f64, what: &str) {
    assert_rel(brep_volume(solid), want, 1e-6, what);
    assert_rel(volume(solid), want, 1e-3, what);
}

/// Die Fläche mit der Marke „STEP-Fläche n“.
fn face_n(solid: &Solid, n: usize) -> &Face {
    let role = format!("{STEP_FACE_ROLE} {n}");
    solid
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role))
        .map(|(_, f)| f)
        .unwrap_or_else(|| panic!("keine Fläche {role}"))
}

/// Außennormale der Fläche in `p`.
fn outward(face: &Face, p: DVec3) -> DVec3 {
    let (u, v) = face.surface.project(p);
    let n = face.surface.normal(u, v);
    if face.same_sense { n } else { -n }
}

/// Länge einer Kante (über 64 Abschnitte).
fn edge_length(solid: &Solid, he: &planora_kernel::HalfEdge) -> f64 {
    let e = &solid.edges[he.edge];
    (0..64)
        .map(|k| {
            let a = e.t0 + (e.t1 - e.t0) * k as f64 / 64.0;
            let b = e.t0 + (e.t1 - e.t0) * (k + 1) as f64 / 64.0;
            e.curve.point(a).distance(e.curve.point(b))
        })
        .sum()
}

/// Eine Schleife `[Naht, Naht rückwärts]`: dieselbe Kante in beiden Richtungen.
fn is_seam_pair(face: &Face) -> bool {
    let h = &face.outer.half_edges;
    h.len() == 2 && h[0].edge == h[1].edge && h[0].forward != h[1].forward
}

#[test]
fn sphere_with_vertex_loops() {
    let text = data("sphere_vertex_loop.step");
    // Kugel: eine Fläche, Schleife [Naht, Naht rückwärts], die beiden Pole als Ecken.
    let s = body(&text, "Kugel");
    assert_eq!(s.faces.len(), 1);
    let f = face_n(&s, 1);
    assert!(is_seam_pair(f), "Schleife {:?}", f.outer);
    assert!(f.inner.is_empty(), "VERTEX_LOOP verworfen");
    assert_eq!(s.vertices.len(), 2);
    let mut zs: Vec<f64> = s.vertices.iter().map(|(_, v)| v.point.z).collect();
    zs.sort_by(f64::total_cmp);
    assert_eq!(zs, vec![-10.0, 10.0]);
    let p = f.patch.expect("Bereich");
    assert!((p.v0 + FRAC_PI_2).abs() <= 1e-12 && (p.v1 - FRAC_PI_2).abs() <= 1e-12);
    assert!((p.u1 - p.u0 - TAU).abs() <= 1e-12, "{p:?}");
    assert_volume(&s, 4.0 / 3.0 * PI * 1000.0, "Kugel");
    // Kuppel: Die Kante der Länge null am Pol ist entfernt (Kreise unten und oben, Naht des
    // Zylinders, Meridian), die Halbkugel hat die Schleife [Äquator, Meridian, Meridian zurück].
    let s = body(&text, "Kuppel");
    assert_eq!(s.edges.len(), 4);
    assert_eq!(s.vertices.len(), 3);
    let dome = face_n(&s, 3);
    assert_eq!(dome.outer.half_edges.len(), 3);
    assert_volume(&s, 5000.0 * PI / 3.0, "Kuppel");
    // Die Kante der Länge null zwischen zwei Ecken am selben Ort: Die Ecken werden eine.
    let twin = swap(
        &text,
        "#197=EDGE_CURVE('',#73,#73,#95,.T.);",
        "#197=EDGE_CURVE('',#73,#74,#95,.T.);\n#74=VERTEX_POINT('',#65);",
    );
    let s = body(&twin, "Kuppel");
    assert_eq!((s.edges.len(), s.vertices.len()), (4, 3));
    let dome = face_n(&s, 3);
    assert_eq!(dome.outer.half_edges.len(), 3);
    assert_volume(&s, 5000.0 * PI / 3.0, "Kuppel");
}

#[test]
fn drop_degenerate_joins_corners_and_keeps_full_circles() {
    let line = |a: DVec3, b: DVec3, s: usize, e: usize| RawEdge {
        curve: Curve::Line {
            origin: a,
            dir: (b - a).normalize_or(DVec3::X),
        },
        t0: 0.0,
        t1: (b - a).length(),
        start: s,
        end: e,
        line: 7,
    };
    let p = [
        DVec3::ZERO,
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(10.0, 10.0, 0.0),
        DVec3::new(0.0, 10.0, 0.0),
        DVec3::new(0.0, 10.0, 0.0),
    ];
    let circle = RawEdge {
        curve: Curve::Circle {
            frame: Frame::WORLD,
            radius: 2.0,
        },
        t0: 0.0,
        t1: TAU,
        start: 5,
        end: 5,
        line: 8,
    };
    let he = |edge: usize| RawHalfEdge {
        edge,
        forward: true,
    };
    let mut raw = RawBody {
        vertices: p
            .iter()
            .copied()
            .chain([DVec3::new(2.0, 0.0, 0.0)])
            .collect(),
        edges: vec![
            line(p[0], p[1], 0, 1),
            line(p[1], p[2], 1, 2),
            line(p[2], p[3], 2, 3),
            // Länge null zwischen zwei Ecken am selben Ort.
            line(p[3], p[4], 3, 4),
            line(p[4], p[0], 4, 0),
            circle,
            // Länge null an einer Ecke (eigene Schleife, verschwindet wie ein VERTEX_LOOP).
            line(p[1], p[1], 1, 1),
        ],
        faces: vec![RawFace {
            surface: Surface::Plane {
                frame: Frame::WORLD,
            },
            same_sense: true,
            loops: vec![
                vec![he(6)],
                vec![he(0), he(1), he(2), he(3), he(4)],
                vec![RawHalfEdge {
                    edge: 5,
                    forward: false,
                }],
            ],
            marked_outer: Some(1),
            patch: None,
            tag: FaceTag {
                origin: 0,
                role: "STEP-Fläche 1".into(),
            },
            line: 9,
            pole_marks: Vec::new(),
            side_guessed: false,
        }],
        shells: Vec::new(),
    };
    drop_degenerate(&mut raw, 1e-6).expect("bereinigt");
    let f = &raw.faces[0];
    assert_eq!(f.loops.len(), 2, "{:?}", f.loops);
    assert_eq!(f.marked_outer, Some(0));
    assert_eq!(f.loops[0], vec![he(0), he(1), he(2), he(4)]);
    // Die Ecken der entfernten Kante sind eine: Der Rand schließt sich.
    assert_eq!(raw.he_end(he(2)), raw.he_start(he(4)));
    // Der volle Kreis (gleiche Ecken, Länge 4π) bleibt.
    assert_eq!(f.loops[1].len(), 1);
    // Eine Gerade zwischen gleichen Ecken (Länge null) lässt den Rand offen: Meldung mit Zeile.
    let mut broken = raw.clone();
    broken.edges[1].end = 1;
    broken.edges[1].t1 = 0.0;
    let Err(Skip(why)) = drop_degenerate(&mut broken, 1e-6) else {
        panic!("offener Rand");
    };
    assert_eq!(why, "Zeile 7: Eine Kante hat die Länge null.");
}

#[test]
fn seams_are_inserted() {
    let text = data("seamless.step");
    // Zylinder: Naht beim Winkel der ersten Ecke des unteren Rands (60 Grad), der obere Kreis
    // ist dort geteilt (4 Kreisstücke + Naht), der Deckel benutzt dieselben Teilkanten.
    let s = body(&text, "Zylinder");
    assert_eq!(s.edges.len(), 5);
    let mantle = face_n(&s, 3);
    assert!(mantle.inner.is_empty());
    let seams: Vec<_> = mantle
        .outer
        .half_edges
        .iter()
        .filter(|he| matches!(s.edges[he.edge].curve, Curve::Line { .. }))
        .collect();
    assert_eq!(seams.len(), 2, "Naht hin und zurück");
    assert_eq!(seams[0].edge, seams[1].edge);
    assert!((edge_length(&s, seams[0]) - 10.0).abs() <= 1e-12);
    let seam = &s.edges[seams[0].edge];
    let a = DVec3::new(2.5, 4.330_127_018_922_193, 0.0);
    for p in [seam.curve.point(seam.t0), seam.curve.point(seam.t1)] {
        let flat = DVec3::new(p.x, p.y, 0.0);
        assert!(flat.distance(a) <= 1e-12, "Naht bei {p}");
    }
    let lid = face_n(&s, 2);
    assert_eq!(lid.outer.half_edges.len(), 2, "Deckel mit geteiltem Kreis");
    for he in &lid.outer.half_edges {
        assert!(
            mantle.outer.half_edges.iter().any(|m| m.edge == he.edge),
            "Teilkante fehlt im Mantel"
        );
    }
    let p = mantle.patch.expect("Bereich");
    assert!((p.u1 - p.u0 - TAU).abs() <= 1e-12 && (p.v1 - p.v0 - 10.0).abs() <= 1e-12);
    assert_volume(&s, PI * 25.0 * 10.0, "Zylinder");
    // Derselbe Zylinder mit gekipptem Sinn des oberen Kreises bleibt abgelehnt.
    let flipped = swap(
        &text,
        "#63=EDGE_CURVE('',#43,#43,#57,.T.);",
        "#63=EDGE_CURVE('',#43,#43,#57,.F.);",
    );
    let why = skip_of(&flipped, "Zylinder");
    assert!(why.contains("schließt sich nicht"), "{why}");
    // Kugel nur mit VERTEX_LOOPs: Naht von Pol zu Pol eingefügt.
    let s = body(&text, "Kugel");
    assert!(is_seam_pair(face_n(&s, 1)));
    assert_eq!(s.vertices.len(), 2);
    assert_volume(&s, 4.0 / 3.0 * PI * 64.0, "Kugel");
    // Kegel aus einer Drehfläche mit VERTEX_LOOP an der Spitze: Naht vom Kreis zur Spitze.
    let s = body(&text, "Kegel");
    assert_eq!(face_n(&s, 2).outer.half_edges.len(), 3);
    assert!(
        s.vertices
            .iter()
            .any(|(_, v)| v.point.distance(DVec3::new(40.0, 0.0, 20.0)) <= 1e-12),
        "Spitze als Ecke"
    );
    assert_volume(&s, 2000.0 * PI / 3.0, "Kegel");
    // Kugelschale nur mit dem Äquator: Naht zum Südpol.
    let s = body(&text, "Kappe");
    assert_eq!(face_n(&s, 2).outer.half_edges.len(), 3);
    assert!(
        s.vertices
            .iter()
            .any(|(_, v)| v.point.distance(DVec3::new(60.0, 0.0, -6.0)) <= 1e-12),
        "Südpol als Ecke"
    );
    assert_volume(&s, 144.0 * PI, "Kappe");
    // Eine Ebene nur mit einem VERTEX_LOOP hat keinen Rand (erst nach dem Einfügen der Nähte).
    let lidless = swap(
        &text,
        "#75=FACE_OUTER_BOUND('',#78,.T.);",
        "#75=FACE_OUTER_BOUND('',#99,.T.);\n#99=VERTEX_LOOP('',#43);",
    );
    let why = skip_of(&lidless, "Zylinder");
    assert!(why.ends_with(": Eine Fläche hat keinen Rand."), "{why}");
}

/// Torusfläche des Körpers und ein Probepunkt auf ihr.
fn torus_probe(s: &Solid) -> (&Face, DVec3) {
    let (id, face) = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Torus { .. }))
        .expect("Torusfläche");
    let p = face_probe_point(s, id, &Tolerance::default()).expect("Probepunkt");
    (face, p)
}

#[test]
fn seamless_torus_ring_gets_two_seams() {
    // Torus 10/3 ganz ohne Rand: zwei Nähte durch eine Ecke, Volumen 2 π² R r².
    let s = body(&data("seamless.step"), "Ring");
    assert_eq!(s.faces.len(), 1);
    assert_eq!(s.vertices.len(), 1);
    let (_, ring) = s.faces.iter().next().expect("Fläche");
    assert_eq!(ring.outer.half_edges.len(), 4);
    assert_volume(&s, 2.0 * PI * PI * 10.0 * 9.0, "Ring");
}

#[test]
fn torus_band_around_axis_gets_a_meridian_seam() {
    let text = data("seamless_torus.step");
    // Welle: Zylinder R 10 × 38, Scheibe R 8 × 2 und die Viertelkreisfläche r 2 um (8; 38)
    // gedreht (Pappus: Schwerpunkt 8 + 8/(3π)).
    let s = body(&text, "Welle");
    let want = 3800.0 * PI + 128.0 * PI + 16.0 * PI * PI + 16.0 * PI / 3.0;
    assert_volume(&s, want, "Welle");
    let (face, p) = torus_probe(&s);
    let (u, _) = face.surface.project(p);
    let tube = DVec3::new(8.0 * u.cos(), 8.0 * u.sin(), 38.0);
    assert!(outward(face, p).dot((p - tube).normalize()) > 1.0 - 1e-9, "Rundung nach außen");
    // Kehle: Innenwand über v = π; Ring R 10…20 × 10 und die linke Hälfte des Rohrkreises r 5
    // um (10; 0) gedreht (Schwerpunkt 10 − 20/(3π)).
    let s = body(&text, "Kehle");
    let want = 3000.0 * PI + 250.0 * PI * PI - 500.0 * PI / 3.0;
    assert_volume(&s, want, "Kehle");
    let (face, p) = torus_probe(&s);
    let (u, _) = face.surface.project(p);
    let tube = DVec3::new(10.0 * u.cos(), 10.0 * u.sin(), 0.0);
    assert!(outward(face, p).dot((p - tube).normalize()) > 1.0 - 1e-9, "Wand zur Achse");
}

#[test]
fn torus_elbow_gets_a_latitude_seam() {
    // Rohr r 5 um R 20 über 90 Grad: π r² · R · π/2.
    let text = data("seamless_torus.step");
    // Ecke des zweiten Rohrkreises auch bei v = π/2: Er wird bei v = 0 geteilt.
    let split = swap(
        &text,
        "#435=CARTESIAN_POINT('',(0.,25.,0.));",
        "#435=CARTESIAN_POINT('',(0.,20.,5.));",
    );
    for (text, edges) in [(&text, 3), (&split, 4)] {
        let s = body(text, "Bogen");
        assert_eq!(s.edges.len(), edges);
        assert_volume(&s, 250.0 * PI * PI, "Bogen");
        let (face, p) = torus_probe(&s);
        let (u, _) = face.surface.project(p);
        let tube = DVec3::new(20.0 * u.cos(), 20.0 * u.sin(), 0.0);
        assert!(outward(face, p).dot((p - tube).normalize()) > 1.0 - 1e-9, "Rohr nach außen");
    }
}

#[test]
fn torus_band_with_a_wrong_circle_is_skipped() {
    let text = data("seamless_torus.step");
    let wrong = swap(
        &text,
        "#92=ORIENTED_EDGE('',*,*,#63,.F.);",
        "#92=ORIENTED_EDGE('',*,*,#63,.T.);",
    );
    let why = skip_of(&wrong, "Welle");
    assert!(why.contains("schließt sich nicht"), "{why}");
    let wrong = swap(
        &text,
        "#484=ORIENTED_EDGE('',*,*,#461,.F.);",
        "#484=ORIENTED_EDGE('',*,*,#461,.T.);",
    );
    let why = skip_of(&wrong, "Bogen");
    assert!(why.contains("schließt sich nicht"), "{why}");
}

/// Prüft den Hohlquader: zwei Schalen, Volumen, Außennormale am Hohlraum zur Kugelmitte.
fn check_hollow_box(s: &Solid) {
    assert_eq!(s.shells.len(), 2);
    assert_volume(s, 8000.0 - 4.0 / 3.0 * PI * 125.0, "Hohlquader");
    let (id, ball) = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Sphere { .. }))
        .expect("Kugelfläche");
    let p = face_probe_point(s, id, &Tolerance::default()).expect("Probepunkt");
    let n = outward(ball, p);
    let to_centre = (DVec3::splat(10.0) - p).normalize();
    assert!(n.dot(to_centre) > 1.0 - 1e-9, "Normale {n} bei {p}");
}

#[test]
fn voids_and_lumps() {
    let text = data("void_box.step");
    check_hollow_box(&body(&text, "Hohlquader"));
    // Zwei getrennte Quader derselben Darstellung: ein Körper mit zwei Schalen.
    let s = body(&text, "Zwei Quader");
    assert_eq!(s.shells.len(), 2);
    assert_eq!(s.faces.len(), 12);
    assert!((volume(&s) - 2000.0).abs() <= 1e-9, "{}", volume(&s));
    // Quader 30 mit Torus-Hohlraum 8/3 (eine Torusfläche mit zwei Nähten); auch mit der Höhle
    // falsch als `.T.` geschrieben.
    let want = 27_000.0 - 2.0 * PI * PI * 8.0 * 9.0;
    let s = body(&text, "Ringhöhle");
    assert_eq!(s.shells.len(), 2);
    assert_volume(&s, want, "Ringhöhle");
    let (id, ring) = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Torus { .. }))
        .expect("Torusfläche");
    let p = face_probe_point(&s, id, &Tolerance::default()).expect("Probepunkt");
    let (u, _) = ring.surface.project(p);
    let tube = DVec3::new(115.0, 15.0, 15.0) + DVec3::new(u.cos(), u.sin(), 0.0) * 8.0;
    assert!(outward(ring, p).dot((tube - p).normalize()) > 1.0 - 1e-9);
    let wrong = swap(
        &text,
        "#51=ORIENTED_CLOSED_SHELL('',*,#52,.F.);",
        "#51=ORIENTED_CLOSED_SHELL('',*,#52,.T.);",
    );
    assert_volume(&body(&wrong, "Ringhöhle"), want, "Ringhöhle .T.");
}

#[test]
fn void_written_without_flip_is_turned() {
    let text = data("void_box.step");
    let wrong = swap(
        &text,
        "#22=ORIENTED_CLOSED_SHELL('',*,#23,.F.);",
        "#22=ORIENTED_CLOSED_SHELL('',*,#23,.T.);",
    );
    check_hollow_box(&body(&wrong, "Hohlquader"));
    // Die Richtung der Schale wird gelesen (nicht nur über das Vorzeichen korrigiert): in der
    // Rohform zeigt die Kugel des Hohlraums mit `.F.` zur Mitte, mit `.T.` nach außen.
    let sphere_sense = |t: &str| {
        let file = parse(t);
        let (rep, items) = representation(&file, "Hohlquader");
        let units = units_of(&file, rep);
        let raw = read_raw(&file, units, &items, units.weld_tol_mm(), &Progress::new())
            .unwrap_or_else(|Skip(s)| panic!("{s}"));
        assert_eq!(raw.shells.len(), 2);
        assert_eq!((raw.shells[0].void, raw.shells[1].void), (false, true));
        let f = raw
            .faces
            .iter()
            .find(|f| matches!(f.surface, Surface::Sphere { .. }))
            .expect("Kugel");
        f.same_sense
    };
    assert!(!sphere_sense(&text));
    assert!(sphere_sense(&wrong));
}

// --- Körper des eigenen Schreibers mit Hohlräumen und Stücken ---

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

/// Liest alle Volumen-Einträge der ersten Darstellung eines Texts des eigenen Schreibers.
fn read_written(text: &str) -> Solid {
    let file = parse(text);
    let rep = file.ids_of("ADVANCED_BREP_SHAPE_REPRESENTATION")[0];
    let e = file.entity(rep).expect("Darstellung");
    let items: Vec<u64> = e
        .arg("ADVANCED_BREP_SHAPE_REPRESENTATION", 1)
        .expect("Liste")
        .refs()
        .expect("Verweise")
        .into_iter()
        .filter(|id| {
            let it = file.entity(*id).expect("Eintrag");
            it.is("MANIFOLD_SOLID_BREP") || it.is("BREP_WITH_VOIDS")
        })
        .collect();
    let units = units_of(&file, rep);
    let b = build_solid(
        &file,
        units,
        &items,
        &BuildOptions {
            weld_tol_mm: units.weld_tol_mm(),
        },
        &Progress::new(),
    )
    .unwrap_or_else(|Skip(s)| panic!("ausgelassen: {s}"));
    validate(&b.solid).expect("gültig");
    b.solid
}

/// Text mit jedem Hohlraum als `ORIENTED_CLOSED_SHELL(…,.T.)` (falsch: nicht umgekehrt).
fn void_as_written(text: &str) -> String {
    let out: Vec<String> = text
        .lines()
        .map(|l| {
            if l.contains("ORIENTED_CLOSED_SHELL(") {
                l.replace(",.F.)", ",.T.)")
            } else {
                l.to_string()
            }
        })
        .collect();
    assert_ne!(out.join("\n"), text.trim_end(), "kein Hohlraum");
    out.join("\n")
}

#[test]
fn written_cavities_and_lumps_come_back() {
    let cube = |a: f64| k().make_box(DVec3::splat(a)).unwrap();
    // Würfel 10 mit Kugelhöhle r 2.
    let ball = moved(&k().make_sphere(2.0).unwrap(), DVec3::splat(5.0));
    let hollow = k().boolean(&cube(10.0), &ball, BoolOp::Difference).unwrap();
    let (text, _) = export(&[("Hohl", &hollow)]);
    assert!(text.contains("BREP_WITH_VOIDS"));
    let s = read_written(&text);
    assert_eq!(s.shells.len(), 2);
    assert_volume(&s, 1000.0 - 32.0 / 3.0 * PI, "Kugelhöhle");
    // Dieselbe Datei mit der Höhle als `.T.` (falsch): gleiches Volumen.
    let wrong = void_as_written(&text);
    assert_rel(volume(&read_written(&wrong)), volume(&s), 1e-9, "umgedreht");
    // Zwei getrennte Würfel.
    let two = k()
        .boolean(
            &cube(4.0),
            &moved(&cube(3.0), DVec3::X * 10.0),
            BoolOp::Union,
        )
        .unwrap();
    let (text, _) = export(&[("Zwei", &two)]);
    let s = read_written(&text);
    assert_eq!(s.shells.len(), 2);
    assert!((volume(&s) - 91.0).abs() <= 1e-9);
}

// --- Pole ---

/// Kugel aus `sphere_vertex_loop.step` mit Polen `off` neben der Achse und der Genauigkeit
/// `uncertainty`.
fn coarse_sphere(off: f64, uncertainty: &str) -> String {
    let text = data("sphere_vertex_loop.step");
    let text = swap(
        &text,
        "#44=CARTESIAN_POINT('',(0.,0.,-10.));",
        &format!("#44=CARTESIAN_POINT('',(0.,{off:e},-10.));"),
    );
    let text = swap(
        &text,
        "#45=CARTESIAN_POINT('',(0.,0.,10.));",
        &format!("#45=CARTESIAN_POINT('',({off:e},0.,10.));"),
    );
    swap(
        &text,
        "LENGTH_MEASURE(1.E-06)",
        &format!("LENGTH_MEASURE({uncertainty})"),
    )
}

#[test]
fn coarse_pole_is_recognised_by_its_place() {
    // Pole 7e-4 neben der Achse, Genauigkeit 1e-3: Die Ecken liegen danach genau auf den Polen,
    // das Volumen stimmt.
    let s = body(&coarse_sphere(7e-4, "1.E-3"), "Kugel");
    for (_, v) in s.vertices.iter() {
        assert!(
            v.point.x.abs() <= 1e-12 && v.point.y.abs() <= 1e-12,
            "Pol {}",
            v.point
        );
    }
    assert_volume(&s, 4.0 / 3.0 * PI * 1000.0, "grobe Kugel");
    // Genaue Datei: Der Pol bleibt 7e-4 neben der Naht, der Rand schließt sich nicht.
    let why = skip_of(&coarse_sphere(7e-4, "1.E-6"), "Kugel");
    assert_eq!(
        why,
        "Der Rand der Fläche „STEP-Fläche 1“ schließt sich nicht: Eine Kante läuft falsch herum."
    );
    // Läuft die Naht selbst neben dem Pol vorbei, wird der Körper ausgelassen (nie ein falsches
    // Volumen).
    let text = swap(
        &coarse_sphere(0.0, "1.E-3"),
        "#46=AXIS2_PLACEMENT_3D('',#47,#905,#901);",
        "#46=AXIS2_PLACEMENT_3D('',#49,#905,#901);\n#49=CARTESIAN_POINT('',(0.,5.E-4,0.));",
    );
    let why = skip_of(&text, "Kugel");
    assert_eq!(
        why,
        "Der Rand der Fläche „STEP-Fläche 1“ läuft nicht genau durch einen Pol."
    );
}

/// Punkte eines Meridians der Kugel r 2 bei der Länge `u` von `b0` nach `b1`.
fn meridian(u: f64, b0: f64, b1: f64) -> Vec<DVec3> {
    (0..=16)
        .map(|k| {
            let b = b0 + (b1 - b0) * k as f64 / 16.0;
            let r = 2.0 * b.cos();
            DVec3::new(r * u.cos(), r * u.sin(), 2.0 * b.sin())
        })
        .collect()
}

#[test]
fn pole_jump_rule_for_the_patch() {
    let sphere = Surface::Sphere {
        frame: Frame::WORLD,
        radius: 2.0,
    };
    // Kugel nur aus einer Naht: ein voller Umlauf.
    let mut lp = meridian(0.5, -FRAC_PI_2, FRAC_PI_2);
    lp.extend(meridian(0.5, FRAC_PI_2, -FRAC_PI_2));
    for sense in [true, false] {
        let p = derive_patch_sensed(&sphere, sense, &[lp.clone()], 0.0).expect("Bereich");
        assert!((p.u1 - p.u0 - TAU).abs() <= 1e-12, "{p:?}");
        assert!((p.v0 + FRAC_PI_2).abs() <= 1e-12 && (p.v1 - FRAC_PI_2).abs() <= 1e-12);
    }
    // Ausschnitt über 270 Grad: Äquator von 0 bis 3π/2, Meridian hoch, am Pol zurück, Meridian
    // bei 0 herunter. Ohne Polsprung läge u nach dem Pol bei 2π statt bei 0.
    let mut lp: Vec<DVec3> = (0..=48)
        .map(|k| {
            let u = 1.5 * PI * k as f64 / 48.0;
            DVec3::new(2.0 * u.cos(), 2.0 * u.sin(), 0.0)
        })
        .collect();
    lp.extend(meridian(1.5 * PI, 0.0, FRAC_PI_2));
    lp.extend(meridian(0.0, FRAC_PI_2, 0.0));
    let p = derive_patch_sensed(&sphere, true, &[lp], 0.0).expect("Ausschnitt");
    assert!(
        p.u0.abs() <= 1e-12 && (p.u1 - 1.5 * PI).abs() <= 1e-12,
        "{p:?}"
    );
    assert!(p.v0.abs() <= 1e-12 && (p.v1 - FRAC_PI_2).abs() <= 1e-12);
    // Geometrisch erkannt: Ein Randpunkt 5e-4 neben dem Pol zählt bei Genauigkeit 1e-3 nicht in
    // u (bei 1e-6 schon).
    let mut lp = meridian(0.25 * PI, -FRAC_PI_2, FRAC_PI_2);
    lp.extend(meridian(0.75 * PI, FRAC_PI_2, -FRAC_PI_2));
    lp.push(DVec3::new(5e-4, 0.0, (4.0f64 - 25e-8).sqrt()));
    let p = derive_patch_sensed(&sphere, false, &[lp.clone()], 1e-3).expect("grob");
    assert!((p.u0 - 0.25 * PI).abs() <= 1e-12, "{p:?}");
    let r = derive_patch_sensed(&sphere, false, &[lp], 1e-6);
    assert!(
        !matches!(r, Ok(p) if (p.u0 - 0.25 * PI).abs() <= 1e-12),
        "{r:?}"
    );
}

// --- Drehflächen: Trimmung, Toleranzen, Kegelspitze ---

/// Kuppel mit der Halbkugel als Drehfläche des getrimmten Profils `profile` (Eintrag #700) und
/// dem Flächensinn `sense`.
fn revolved_dome(profile: &str, sense: &str) -> String {
    let text = data("sphere_vertex_loop.step");
    let text = swap(
        &text,
        "#113=SPHERICAL_SURFACE('',#82,10.);",
        &format!(
            "#113=SURFACE_OF_REVOLUTION('',#700,#701);\n#701=AXIS1_PLACEMENT('',#62,#903);\n\
             #702=CARTESIAN_POINT('',(-10.,0.,10.));\n{profile}"
        ),
    );
    swap(
        &text,
        "#123=ADVANCED_FACE('',(#133),#113,.T.);",
        &format!("#123=ADVANCED_FACE('',(#133),#113,{sense});"),
    )
}

#[test]
fn trimmed_profiles_are_revolved_over_their_trim() {
    let want = 5000.0 * PI / 3.0;
    // Die andere Hälfte des Meridiankreises (x < 0), vom Äquator zum Pol (Sinn .F.): Normale
    // nach außen.
    let text = revolved_dome(
        "#700=TRIMMED_CURVE('',#94,(PARAMETER_VALUE(3.14159265358979)),\
         (PARAMETER_VALUE(1.5707963267949)),.F.,.PARAMETER.);\n",
        ".T.",
    );
    let s = body(&text, "Kuppel");
    assert!(matches!(face_n(&s, 3).surface, Surface::Sphere { .. }));
    assert_volume(&s, want, "andere Hälfte");
    // Dieselbe Hälfte vom Pol zum Äquator (Sinn .T., Trimmung über Punkte): Normale nach innen,
    // die Fläche steht auf .F.
    let text = revolved_dome(
        "#700=TRIMMED_CURVE('',#94,(#65),(#702),.T.,.CARTESIAN.);\n",
        ".F.",
    );
    assert_volume(&body(&text, "Kuppel"), want, "über Punkte");
    // Falscher Flächensinn bleibt abgelehnt.
    let text = revolved_dome(
        "#700=TRIMMED_CURVE('',#94,(#65),(#702),.T.,.CARTESIAN.);\n",
        ".T.",
    );
    let why = skip_of(&text, "Kuppel");
    assert!(why.contains("verdreht"), "{why}");
    // Ellipsoidboden: Halbachsen 10 und 5, getrimmt vom Äquator zum Pol.
    let text = revolved_dome(
        "#700=TRIMMED_CURVE('',#703,(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(1.5707963267949)),\
         .T.,.PARAMETER.);\n#703=ELLIPSE('',#83,10.,5.);\n",
        ".T.",
    );
    let text = swap(
        &text,
        "#94=CIRCLE('',#83,10.);",
        "#94=ELLIPSE('',#83,10.,5.);",
    );
    let text = swap(
        &text,
        "#65=CARTESIAN_POINT('',(0.,0.,20.));",
        "#65=CARTESIAN_POINT('',(0.,0.,15.));",
    );
    let s = body(&text, "Kuppel");
    assert_volume(
        &s,
        1000.0 * PI + 2.0 / 3.0 * PI * 100.0 * 5.0,
        "Ellipsoidboden",
    );
}

/// Rationaler Viertelkreis r 10 in der xz-Ebene von (10; 0; 0) nach (`end_x`; 0; 10).
fn quarter(end_x: f64) -> Curve {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    Curve::nurbs(
        NurbsCurve::new(
            2,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![
                DVec3::new(10.0, 0.0, 0.0),
                DVec3::new(10.0, 0.0, 10.0),
                DVec3::new(end_x, 0.0, 10.0),
            ],
            vec![1.0, w, 1.0],
        )
        .expect("Viertelkreis"),
    )
}

#[test]
fn revolution_tolerances_follow_the_file() {
    let o = DVec3::ZERO;
    // Freiformprofil, das genau auf der Achse endet (Halbkugel aus einem B-Spline): gelesen.
    let r = revolution(&quarter(0.0), o, DVec3::Z, 1e-6);
    assert!(matches!(r, Ok(SurfaceGeom::Ready { .. })), "{r:?}");
    // 5e-8 jenseits der Achse: innerhalb der Genauigkeit 1e-6, gelesen; 1e-4 jenseits nicht.
    let r = revolution(&quarter(-5e-8), o, DVec3::Z, 1e-6);
    assert!(matches!(r, Ok(SurfaceGeom::Ready { .. })), "{r:?}");
    let Err(Skip(why)) = revolution(&quarter(-1e-4), o, DVec3::Z, 1e-6) else {
        panic!("Profil schneidet die Achse");
    };
    assert_eq!(why, "Das Profil einer Drehfläche schneidet die Drehachse.");
    // Kreis mit Mitte 1e-8 neben der Achse: Kugel (nicht die Torus-Meldung).
    let circle = |cx: f64| Curve::Circle {
        frame: Frame {
            origin: DVec3::new(cx, 0.0, 0.0),
            x: DVec3::X,
            y: DVec3::Z,
            z: -DVec3::Y,
        },
        radius: 5.0,
    };
    match revolution(&circle(1e-8), o, DVec3::Z, 1e-6) {
        Ok(SurfaceGeom::Ready {
            surface: Surface::Sphere { radius, .. },
            ..
        }) => assert_eq!(radius, 5.0),
        other => panic!("Kugel erwartet: {other:?}"),
    }
    // 1e-5 daneben bei Genauigkeit 1e-6: ein Torus mit kleinem Radius größer als dem großen.
    let Err(Skip(why)) = revolution(&circle(1e-5), o, DVec3::Z, 1e-6) else {
        panic!("Spindeltorus");
    };
    assert!(why.starts_with("Eine Torusfläche"), "{why}");
}

#[test]
fn pointed_cone_takes_its_reference_away_from_the_apex() {
    let circle: Vec<DVec3> = (0..=32)
        .map(|k| {
            let a = TAU * k as f64 / 32.0;
            DVec3::new(10.0 * a.cos(), 10.0 * a.sin(), 0.0)
        })
        .collect();
    let apex = DVec3::new(0.0, 0.0, 20.0);
    // Randpunkte bis in die Spitze (Naht von Kreis zur Spitze), Gerade in beiden Richtungen.
    let loops = vec![circle, vec![DVec3::new(10.0, 0.0, 0.0), apex]];
    let down = (DVec3::new(10.0, 0.0, 0.0) - apex).normalize();
    for (origin, dir) in [(apex, down), (DVec3::new(10.0, 0.0, 0.0), -down)] {
        let (surface, _) = finish_cone(origin, dir, DVec3::ZERO, DVec3::Z, &loops).expect("Kegel");
        let Surface::Cone {
            frame,
            radius,
            slope,
        } = surface
        else {
            panic!("Kegel erwartet");
        };
        assert!((radius - 10.0).abs() <= 1e-9, "Radius {radius}");
        assert!(frame.origin.distance(DVec3::ZERO) <= 1e-9, "{frame:?}");
        assert!((slope - 0.5).abs() <= 1e-12);
    }
}

// --- Nacharbeit 1 (Review Aufgabe 6) ---

#[test]
fn flipped_seamless_cap_is_never_the_other_cap() {
    let text = data("seamless.step");
    // Richtig geschrieben: gelesen (Kappe nach unten, Haube nach oben, Schale mit Südpol).
    assert_volume(&body(&text, "Kappe"), 144.0 * PI, "Kappe");
    let s = body(&text, "Haube");
    assert_volume(&s, 144.0 * PI, "Haube");
    // Die Naht der Haube läuft von der Ecke bei 45 Grad zum Nordpol.
    let dome = face_n(&s, 2);
    let seam = dome
        .outer
        .half_edges
        .iter()
        .map(|he| &s.edges[he.edge])
        .find(|e| e.start != e.end)
        .expect("Naht");
    let ends = [seam.curve.point(seam.t0), seam.curve.point(seam.t1)];
    let corner = DVec3::new(80.0 + 6.0 * (PI / 4.0).cos(), 6.0 * (PI / 4.0).sin(), 0.0);
    assert!(ends.iter().any(|p| p.distance(corner) <= 1e-9), "{ends:?}");
    assert!(
        ends.iter()
            .any(|p| p.distance(DVec3::new(80.0, 0.0, 6.0)) <= 1e-9),
        "{ends:?}"
    );
    assert_volume(&body(&text, "Schale"), 144.0 * PI, "Schale");
    // Ein falsches `same_sense` der Kugelfläche ergäbe still die Gegenkappe: abgelehnt.
    for (name, from, to) in [
        (
            "Kappe",
            "#562=ADVANCED_FACE('',(#572),#552,.T.);",
            "#562=ADVANCED_FACE('',(#572),#552,.F.);",
        ),
        (
            "Haube",
            "#662=ADVANCED_FACE('',(#672),#652,.T.);",
            "#662=ADVANCED_FACE('',(#672),#652,.F.);",
        ),
    ] {
        let why = skip_of(&swap(&text, from, to), name);
        assert_eq!(
            why,
            "Die Seite der Fläche „STEP-Fläche 2“ ist nicht eindeutig: Sie hat weder Naht noch \
             Pol, und der Körper müsste umgedreht werden.",
            "{name}"
        );
    }
    // Mit `VERTEX_LOOP` am Südpol: Die Naht muss dorthin; derselbe Bitfehler → verdreht.
    let why = skip_of(
        &swap(
            &text,
            "#762=ADVANCED_FACE('',(#772,#773),#752,.T.);",
            "#762=ADVANCED_FACE('',(#772,#773),#752,.F.);",
        ),
        "Schale",
    );
    assert_eq!(
        why,
        "Die Fläche „STEP-Fläche 2“ ist verdreht: Rand und Außenseite passen nicht zusammen."
    );
    // Die ganze Schale umgekehrt (beide Flächen und beide Ränder) mit dem Pol bleibt richtig.
    let mut inv = text.clone();
    for (from, to) in [
        (
            "#761=ADVANCED_FACE('',(#771),#751,.T.);",
            "#761=ADVANCED_FACE('',(#771),#751,.F.);",
        ),
        (
            "#762=ADVANCED_FACE('',(#772,#773),#752,.T.);",
            "#762=ADVANCED_FACE('',(#772,#773),#752,.F.);",
        ),
        (
            "#771=FACE_OUTER_BOUND('',#781,.T.);",
            "#771=FACE_OUTER_BOUND('',#781,.F.);",
        ),
        (
            "#772=FACE_BOUND('',#782,.T.);",
            "#772=FACE_BOUND('',#782,.F.);",
        ),
    ] {
        inv = swap(&inv, from, to);
    }
    let s = body(&inv, "Schale");
    assert_volume(&s, 144.0 * PI, "Schale umgekehrt");
    let zmin = s
        .vertices
        .iter()
        .map(|(_, v)| v.point.z)
        .fold(f64::INFINITY, f64::min);
    assert!((zmin + 6.0).abs() <= 1e-9, "Südpol {zmin}");
}

#[test]
fn seamless_freeform_cap_is_named_by_p9() {
    // Ellipsoidboden als Drehfläche nur mit dem Äquator (kein Meridian): P9.
    let text = revolved_dome(
        "#700=TRIMMED_CURVE('',#703,(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(1.5707963267949)),\
         .T.,.PARAMETER.);\n#703=ELLIPSE('',#83,10.,5.);\n",
        ".T.",
    );
    let text = swap(
        &text,
        "#143=EDGE_LOOP('',(#156,#157,#158,#159));",
        "#143=EDGE_LOOP('',(#156));",
    );
    assert_eq!(
        skip_of(&text, "Kuppel"),
        "Fläche vom Typ Freiformfläche ohne Naht wird noch nicht unterstützt."
    );
}

/// Text mit den Punkten der Kennungen `ids` um `shift` verschoben.
fn shifted(text: &str, ids: std::ops::RangeInclusive<u64>, shift: DVec3) -> String {
    text.lines()
        .map(|l| {
            let Some((head, rest)) = l.split_once("=CARTESIAN_POINT('',(") else {
                return l.to_string();
            };
            let id: u64 = head.trim_start_matches('#').parse().unwrap_or(0);
            if !ids.contains(&id) {
                return l.to_string();
            }
            let nums: Vec<f64> = rest
                .trim_end_matches("));")
                .split(',')
                .map(|x| x.parse().expect("Zahl"))
                .collect();
            let p = DVec3::new(nums[0], nums[1], nums[2]) + shift;
            format!(
                "{head}=CARTESIAN_POINT('',({:?},{:?},{:?}));",
                p.x, p.y, p.z
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn broken_shells_are_named() {
    let text = data("void_box.step");
    // Hohlraum ragt aus dem Quader (Kugel um (10; 10; 18) r 5, Quader bis 20).
    let out = shifted(&text, 44..=47, DVec3::new(0.0, 0.0, 8.0));
    assert_eq!(
        skip_of(&out, "Hohlquader"),
        "Ein Hohlraum liegt nicht ganz im Körper."
    );
    // Zweites Stück im ersten (2 … 8 im Würfel 0 … 10), auch innen-außen geschrieben.
    let inner = text
        .lines()
        .map(|l| {
            let Some((head, rest)) = l.split_once("=CARTESIAN_POINT('',(") else {
                return l.to_string();
            };
            let id: u64 = head.trim_start_matches('#').parse().unwrap_or(0);
            if !(3001..=3008).contains(&id) {
                return l.to_string();
            }
            let map = |x: &str| match x {
                "20." | "0." => "2.",
                "30." | "10." => "8.",
                other => panic!("{other}"),
            };
            let c: Vec<&str> = rest.trim_end_matches("));").split(',').map(map).collect();
            format!("{head}=CARTESIAN_POINT('',({}));", c.join(","))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let why = "Zwei Stücke des Körpers überschneiden sich.";
    assert_eq!(skip_of(&inner, "Zwei Quader"), why);
    let inside_out = swap(
        &inner,
        "#31=MANIFOLD_SOLID_BREP('Stück 2',#3140);",
        "#31=MANIFOLD_SOLID_BREP('Stück 2',#3141);\n#3141=ORIENTED_CLOSED_SHELL('',*,#3140,.F.);",
    );
    assert_eq!(skip_of(&inside_out, "Zwei Quader"), why);
    // Überlappende Stücke (zweiter Würfel bei x 5 … 15).
    let overlap = shifted(&text, 3001..=3008, DVec3::new(-15.0, 0.0, 0.0));
    assert_eq!(skip_of(&overlap, "Zwei Quader"), why);
    // Berührende Stücke (gemeinsame Seitenfläche bei x = 10) bleiben erlaubt.
    let touch = shifted(&text, 3001..=3008, DVec3::new(-10.0, 0.0, 0.0));
    assert!((volume(&body(&touch, "Zwei Quader")) - 2000.0).abs() <= 1e-9);
}

#[test]
fn open_torus_band_gets_its_seam() {
    // Torusfläche zwischen zwei Meridiankreisen (windet sich in v), ohne Deckel: Sie bekommt
    // eine Naht (nicht „schließt sich nicht“, das wäre ein falsch laufender Rand), der Körper
    // bleibt aber offen.
    let band = "#20=MANIFOLD_SOLID_BREP('Ring',#21);\n#21=CLOSED_SHELL('',(#30));\n\
        #30=ADVANCED_FACE('',(#31,#32),#40,.T.);\n\
        #31=FACE_BOUND('',#33,.T.);\n#32=FACE_BOUND('',#34,.T.);\n\
        #33=EDGE_LOOP('',(#35));\n#34=EDGE_LOOP('',(#36));\n\
        #35=ORIENTED_EDGE('',*,*,#37,.F.);\n#36=ORIENTED_EDGE('',*,*,#38,.T.);\n\
        #37=EDGE_CURVE('',#51,#51,#61,.T.);\n#38=EDGE_CURVE('',#52,#52,#62,.T.);\n\
        #51=VERTEX_POINT('',#71);\n#52=VERTEX_POINT('',#72);\n\
        #71=CARTESIAN_POINT('',(13.,0.,0.));\n#72=CARTESIAN_POINT('',(0.,13.,0.));\n\
        #61=CIRCLE('',#81,3.);\n#62=CIRCLE('',#82,3.);\n\
        #81=AXIS2_PLACEMENT_3D('',#73,#93,#91);\n#82=AXIS2_PLACEMENT_3D('',#74,#94,#92);\n\
        #73=CARTESIAN_POINT('',(10.,0.,0.));\n#74=CARTESIAN_POINT('',(0.,10.,0.));\n\
        #91=DIRECTION('',(1.,0.,0.));\n#92=DIRECTION('',(0.,1.,0.));\n\
        #93=DIRECTION('',(0.,-1.,0.));\n#94=DIRECTION('',(1.,0.,0.));\n\
        #40=TOROIDAL_SURFACE('',#83,10.,3.);\n#83=AXIS2_PLACEMENT_3D('',#75,$,$);\n\
        #75=CARTESIAN_POINT('',(0.,0.,0.));\n";
    let text = format!(
        "ISO-10303-21;\n/* Selbst geschrieben für die Tests von PlanoraCAD (GPL-3.0-only). */\n\
         HEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
         FILE_NAME('x.step','2026-10-21T00:00:00',(''),(''),'','','');\n\
         FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n\
         #9=ADVANCED_BREP_SHAPE_REPRESENTATION('Ring',(#20),#105);\n\
         #100=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
         #105=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#100))\
         REPRESENTATION_CONTEXT('',''));\n{band}ENDSEC;\nEND-ISO-10303-21;\n"
    );
    // Das Band bekommt seine Naht; der Körper ist ohne Deckel aber offen.
    let why = skip_of(&text, "Ring");
    assert!(why.ends_with("gehört nur zu einer Fläche."), "{why}");
}

#[test]
fn coarse_pole_corners_are_pulled_before_short_edges_go() {
    // Kuppel mit zwei Ecken am Pol, 1,2e-3 auseinander (je 6e-4 neben dem Pol), dazwischen die
    // Kante „Länge null“; Genauigkeit 1e-3: erst auf den Pol ziehen, dann ist die Kante kurz.
    let text = data("sphere_vertex_loop.step");
    let text = swap(
        &text,
        "#197=EDGE_CURVE('',#73,#73,#95,.T.);",
        "#197=EDGE_CURVE('',#73,#74,#95,.T.);\n#74=VERTEX_POINT('',#66);\n\
         #66=CARTESIAN_POINT('',(-6.E-4,0.,20.));",
    );
    let text = swap(
        &text,
        "#65=CARTESIAN_POINT('',(0.,0.,20.));",
        "#65=CARTESIAN_POINT('',(6.E-4,0.,20.));",
    );
    let text = swap(&text, "LENGTH_MEASURE(1.E-06)", "LENGTH_MEASURE(1.E-3)");
    let s = body(&text, "Kuppel");
    assert_eq!((s.edges.len(), s.vertices.len()), (4, 3));
    assert_volume(&s, 5000.0 * PI / 3.0, "Kuppel grob");
}

/// Rationaler Halbkreis r 10 in der xz-Ebene um (0; 0; 10), von (10; 0; 10) über den Pol nach
/// (−10; 0; 10), Parameter 0 … 1 (Knoten 0,5 am Pol), als Eintrag #704.
const SEMICIRCLE: &str = "#704=(BOUNDED_CURVE()B_SPLINE_CURVE(2,(#64,#705,#65,#706,#702),\
    .CIRCULAR_ARC.,.F.,.F.)B_SPLINE_CURVE_WITH_KNOTS((3,2,3),(0.,0.5,1.),.UNSPECIFIED.)CURVE()\
    GEOMETRIC_REPRESENTATION_ITEM()RATIONAL_B_SPLINE_CURVE((1.,0.7071067811865476,1.,\
    0.7071067811865476,1.))REPRESENTATION_ITEM(''));\n\
    #705=CARTESIAN_POINT('',(10.,0.,20.));\n#706=CARTESIAN_POINT('',(-10.,0.,20.));\n";

#[test]
fn trim_paths_are_exact() {
    let want = 5000.0 * PI / 3.0;
    // Freiformprofil, Sinn .F.: vom Ende (1, bei x < 0) zurück zum Pol (0,5). Die Spiegelung
    // des Bereichs bei der Umkehrung muss stimmen, sonst wird die Hälfte bei x > 0 gemessen.
    let text = revolved_dome(
        &format!(
            "#700=TRIMMED_CURVE('',#704,(PARAMETER_VALUE(1.)),(PARAMETER_VALUE(0.5)),.F.,\
             .PARAMETER.);\n{SEMICIRCLE}"
        ),
        ".T.",
    );
    assert_volume(&body(&text, "Kuppel"), want, "Freiform .F.");
    // Dieselbe Hälfte vorwärts vom Pol (Knoten 0,5) zum Ende der Kurve (Rand des Bereichs).
    let text = revolved_dome(
        &format!(
            "#700=TRIMMED_CURVE('',#704,(PARAMETER_VALUE(0.5)),(PARAMETER_VALUE(1.)),.T.,\
             .PARAMETER.);\n{SEMICIRCLE}"
        ),
        ".F.",
    );
    assert_volume(&body(&text, "Kuppel"), want, "Freiform .T.");
    // Punkte und (falsche) Parameter mit `.CARTESIAN.`: Die Punkte gelten.
    let text = revolved_dome(
        "#700=TRIMMED_CURVE('',#94,(#65,PARAMETER_VALUE(0.)),(#702,PARAMETER_VALUE(0.5)),.T.,\
         .CARTESIAN.);\n",
        ".F.",
    );
    assert_volume(&body(&text, "Kuppel"), want, "CARTESIAN");
}

#[test]
fn trim_over_a_full_turn_is_the_whole_curve() {
    // Gleiche Grenzen bei einem Kreis: ein voller Umlauf (Torus, nicht „keine Ausdehnung“).
    let text = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
         FILE_NAME('x.step','',(''),(''),'','','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\n\
         ENDSEC;\nDATA;\n\
         #1=SURFACE_OF_REVOLUTION('',#2,#3);\n\
         #2=TRIMMED_CURVE('',#4,(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(0.)),.T.,.PARAMETER.);\n\
         #3=AXIS1_PLACEMENT('',#5,#6);\n#4=CIRCLE('',#7,3.);\n\
         #5=CARTESIAN_POINT('',(0.,0.,0.));\n#6=DIRECTION('',(0.,0.,1.));\n\
         #7=AXIS2_PLACEMENT_3D('',#8,#9,#10);\n#8=CARTESIAN_POINT('',(10.,0.,0.));\n\
         #9=DIRECTION('',(0.,-1.,0.));\n#10=DIRECTION('',(1.,0.,0.));\n\
         ENDSEC;\nEND-ISO-10303-21;\n";
    let file = parse(text);
    let mut res = Resolver::new(&file, Units::MM);
    match res.surface(1) {
        Ok(SurfaceGeom::Ready {
            surface: Surface::Torus { major, minor, .. },
            ..
        }) => assert_eq!((major, minor), (10.0, 3.0)),
        other => panic!("Torus erwartet: {other:?}"),
    }
}
