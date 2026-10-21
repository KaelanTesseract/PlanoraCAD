//! STEP-Leser, Stufe 3: Topologie, Grundweg (STEP-Import, Aufgabe 3). Die Dateien unter
//! `tests/data` sind von Hand geschrieben; die Erwartungen sind Handrechnungen und Körper, die der
//! Kernel selbst baut (Fläche für Fläche: Außennormale, Laufrichtung der Ränder, Volumen, Fläche).

mod common;

use planora_exchange::Progress;
use planora_kernel::boolean::{face_probe_point, oriented_area, points_to_uv};
use planora_kernel::primitives::{make_box, make_cone, make_cylinder};
use planora_kernel::{
    AnalyticKernel, BlendEdge, BlendKind, BlendRequest, BoolOp, Curve, Face, Id, Kernel, Profile,
    ProfileLoop, Solid, Surface, extrude, validate,
};
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};
use planora_mesh::{Mesh, TessellationOptions, mass_properties, tessellate};
use planora_step::read::geom::Skip;
use planora_step::read::topo::{
    BuildOptions, BuiltSolid, STEP_FACE_ROLE, build_solid, derive_patch_sensed, edge_params,
};
use planora_step::read::units::{Units, context_units};
use planora_step::{MAX_STEP_FACES, ReadLimits, StepFile};
use std::path::PathBuf;

fn data(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn parse_with(text: &str, limits: &ReadLimits) -> StepFile {
    StepFile::parse(text.as_bytes().to_vec(), limits, &Progress::new()).expect("Datei lädt")
}

/// Liest alle `MANIFOLD_SOLID_BREP` der Datei als einen Körper; Einheiten aus dem Kontext der
/// (ersten) Darstellung.
fn build_text(text: &str, limits: &ReadLimits) -> Result<BuiltSolid, Skip> {
    let file = parse_with(text, limits);
    let units = units_of(&file);
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    assert!(!items.is_empty(), "Datei ohne Körper");
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

fn build(text: &str) -> BuiltSolid {
    build_text(text, &ReadLimits::DEFAULT).unwrap_or_else(|Skip(s)| panic!("ausgelassen: {s}"))
}

fn skip_of(text: &str, limits: &ReadLimits) -> String {
    match build_text(text, limits) {
        Ok(_) => panic!("sollte ausgelassen werden"),
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

/// Fläche der Dreiecke von Fläche `face`.
fn face_area(m: &Mesh, face: Id<Face>) -> f64 {
    let r = m
        .face_ranges
        .iter()
        .find(|r| r.face == face)
        .expect("Fläche im Netz");
    let start = r.first_index as usize;
    let idx = &m.indices[start..start + r.index_count as usize];
    idx.as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            0.5 * (b - a).cross(c - a).length()
        })
        .sum()
}

/// Außennormale der Fläche in `p`.
fn outward(face: &Face, p: DVec3) -> DVec3 {
    let (u, v) = face.surface.project(p);
    let n = face.surface.normal(u, v);
    if face.same_sense { n } else { -n }
}

/// Punkte der äußeren Schleife in Laufrichtung (je Kante 8 Abschnitte, ohne Endpunkt).
fn outer_points(solid: &Solid, face: &Face) -> Vec<DVec3> {
    let mut pts = Vec::new();
    for he in &face.outer.half_edges {
        let e = &solid.edges[he.edge];
        for k in 0..8 {
            let s = k as f64 / 8.0;
            let s = if he.forward { s } else { 1.0 - s };
            pts.push(e.curve.point(e.t0 + (e.t1 - e.t0) * s));
        }
    }
    pts
}

/// Umlaufsinn der äußeren Schleife um die Außennormale: positiv = gegen den Uhrzeigersinn (die
/// Regel des Kernels für äußere Schleifen).
fn outer_turn(solid: &Solid, face: &Face) -> f64 {
    let poly: Vec<DVec2> = points_to_uv(&face.surface, face.same_sense, &outer_points(solid, face));
    oriented_area(&poly, face.same_sense)
}

fn bbox(solid: &Solid) -> (DVec3, DVec3) {
    solid.vertices.iter().fold(
        (DVec3::INFINITY, DVec3::NEG_INFINITY),
        |(lo, hi), (_, v)| (lo.min(v.point), hi.max(v.point)),
    )
}

fn assert_close(a: DVec3, b: DVec3, eps: f64) {
    assert!((a - b).length() <= eps, "{a:?} != {b:?} (±{eps})");
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn roles(solid: &Solid) -> Vec<String> {
    solid
        .faces
        .iter()
        .map(|(_, f)| {
            let tag = f.tag.as_ref().expect("Marke");
            assert_eq!(tag.origin, 0, "Herkunft noch nicht vergeben");
            tag.role.clone()
        })
        .collect()
}

/// Die Fläche mit der Marke „STEP-Fläche n“.
fn face_n(solid: &Solid, n: usize) -> &Face {
    let role = format!("{STEP_FACE_ROLE} {n}");
    let mut it = solid
        .faces
        .iter()
        .filter(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role));
    let (_, f) = it.next().unwrap_or_else(|| panic!("keine Fläche {role}"));
    assert!(it.next().is_none(), "{role} doppelt");
    f
}

#[test]
fn box_reads_with_exact_measures() {
    let built = build(&data("box_ap214.step"));
    let s = &built.solid;
    assert_eq!(built.faces, 6);
    assert_eq!(s.faces.len(), 6);
    assert_eq!(s.edges.len(), 12);
    assert_eq!(s.vertices.len(), 8);
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    let report = validate(s).expect("gültig");
    assert_eq!(report.genus, 0);
    let mp = mass_properties(&mesh(s, 0.001));
    assert!((mp.volume - 6000.0).abs() <= 1e-9, "Volumen {}", mp.volume);
    assert!((mp.surface_area - 2200.0).abs() <= 1e-9);
    let (lo, hi) = bbox(s);
    assert_close(lo, DVec3::ZERO, 1e-12);
    assert_close(hi, DVec3::new(10.0, 20.0, 30.0), 1e-12);
    let expected: Vec<String> = (1..=6).map(|n| format!("STEP-Fläche {n}")).collect();
    assert_eq!(roles(s), expected);
    // Dateireihenfolge: Boden, Deckel, vorn, rechts, hinten, links (CLOSED_SHELL #21).
    let normals = [
        DVec3::NEG_Z,
        DVec3::Z,
        DVec3::NEG_Y,
        DVec3::X,
        DVec3::Y,
        DVec3::NEG_X,
    ];
    let tol = Tolerance::default();
    for (n, want) in normals.iter().enumerate() {
        let f = face_n(s, n + 1);
        let id = s
            .faces
            .iter()
            .find(|(_, g)| std::ptr::eq(*g, f))
            .map(|(id, _)| id)
            .expect("Id");
        let p = face_probe_point(s, id, &tol).expect("Probepunkt");
        assert_close(outward(f, p), *want, 1e-15);
        assert!(
            outer_turn(s, f) > 0.0,
            "Fläche {} läuft falsch herum",
            n + 1
        );
    }
}

/// Der gelesene Quader gleicht Fläche für Fläche dem Quader des Kernels: gleiche Außennormale,
/// gleicher Umlaufsinn der Ränder um sie, gleicher Flächeninhalt und gleiche Eckpunkte.
#[test]
fn box_matches_the_kernel_box_face_by_face() {
    let read = build(&data("box_ap214.step")).solid;
    let kernel = make_box(DVec3::new(10.0, 20.0, 30.0), &Tolerance::default()).expect("Quader");
    let (mr, mk) = (mesh(&read, 0.01), mesh(&kernel, 0.01));
    let tol = Tolerance::default();
    for (rid, rf) in read.faces.iter() {
        let p = face_probe_point(&read, rid, &tol).expect("Probepunkt");
        let n = outward(rf, p);
        let (kid, kf) = kernel
            .faces
            .iter()
            .find(|(kid, kf)| {
                let q = face_probe_point(&kernel, *kid, &tol).expect("Probepunkt");
                outward(kf, q).distance(n) < 1e-12
            })
            .expect("Fläche des Kernels mit derselben Außennormale");
        assert!(kf.surface.implicit(p).abs() < 1e-12, "andere Ebene");
        assert!(outer_turn(&read, rf) > 0.0 && outer_turn(&kernel, kf) > 0.0);
        assert!((face_area(&mr, rid) - face_area(&mk, kid)).abs() < 1e-9);
        let corners = |s: &Solid, f: &Face| {
            let mut c: Vec<[i64; 3]> = f
                .outer
                .half_edges
                .iter()
                .map(|he| {
                    let p = s.vertices[s.half_edge_start(*he)].point;
                    [p.x, p.y, p.z].map(|x| (x * 1e6).round() as i64)
                })
                .collect();
            c.sort();
            c
        };
        assert_eq!(corners(&read, rf), corners(&kernel, kf));
    }
    let (vr, vk) = (mass_properties(&mr), mass_properties(&mk));
    assert!((vr.volume - vk.volume).abs() < 1e-9);
    assert!((vr.surface_area - vk.surface_area).abs() < 1e-9);
}

/// Ring R 20 / r 5, Höhe 30, vom Kernel extrudiert.
fn kernel_ring() -> Solid {
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, 20.0),
        holes: vec![ProfileLoop::circle(DVec2::ZERO, 5.0)],
    };
    extrude(&profile, &Frame::WORLD, 30.0, &Tolerance::default()).expect("Ring")
}

fn check_ring(built: &BuiltSolid, faces: usize, width: f64) -> (f64, f64) {
    let s = &built.solid;
    assert_eq!(built.faces, faces);
    validate(s).expect("gültig");
    let mp = mass_properties(&mesh(s, 0.001));
    let want = 11_250.0 * PI;
    assert!(
        rel(mp.volume, want) <= 1e-4,
        "Volumen {} statt {want}",
        mp.volume
    );
    let tol = Tolerance::default();
    let mut cylinders = 0;
    for (id, f) in s.faces.iter() {
        let Surface::Cylinder { radius, .. } = f.surface else {
            assert!(f.patch.is_none(), "Ebenen ohne Parameterbereich");
            continue;
        };
        cylinders += 1;
        let p = f.patch.expect("Parameterbereich");
        assert!(
            (p.u1 - p.u0 - width).abs() <= 1e-12,
            "Breite {}",
            p.u1 - p.u0
        );
        assert!((0.0..TAU).contains(&p.u0), "u0 = {}", p.u0);
        assert!(p.v0.abs() <= 1e-12 && (p.v1 - 30.0).abs() <= 1e-12);
        let q = face_probe_point(s, id, &tol).expect("Probepunkt");
        let radial = DVec3::new(q.x, q.y, 0.0).normalize();
        let n = outward(f, q);
        // Außen zeigt die Außennormale radial nach außen, in der Bohrung zur Achse.
        let want = if radius > 10.0 { radial } else { -radial };
        assert_close(n, want, 1e-12);
        assert!(outer_turn(s, f) > 0.0);
    }
    assert_eq!(cylinders, if width < 4.0 { 4 } else { 2 });
    for (_, f) in s.faces.iter() {
        assert!(outer_turn(s, f) > 0.0);
    }
    (mp.volume, mp.surface_area)
}

#[test]
fn cylinder_halves_and_seam_give_the_same_body() {
    let halves = build(&data("bore_halves.step"));
    let seam = build(&data("bore_seam.step"));
    let (vh, ah) = check_ring(&halves, 6, PI);
    let (vs, a_s) = check_ring(&seam, 4, TAU);
    assert!(rel(vh, vs) <= 1e-4);
    assert!(rel(ah, a_s) <= 1e-4);
    let want_area = 2.0 * PI * (400.0 - 25.0) + 2.0 * PI * 25.0 * 30.0;
    assert!(
        rel(ah, want_area) <= 1e-4,
        "Oberfläche {ah} statt {want_area}"
    );
    // Gleich dem Ring des Kernels.
    let k = mass_properties(&mesh(&kernel_ring(), 0.001));
    assert!(rel(vs, k.volume) <= 1e-4 && rel(a_s, k.surface_area) <= 1e-4);
    // Volle Kreise mit Achse −z und `same_sense` .F.: Kante läuft gegen den Uhrzeigersinn um +z.
    let top_hole = face_n(&seam.solid, 2)
        .inner
        .first()
        .expect("Loch im Deckel")
        .half_edges[0];
    let e = &seam.solid.edges[top_hole.edge];
    assert!(
        (e.t1 - e.t0 + TAU).abs() <= 1e-12,
        "t0 {} t1 {}",
        e.t0,
        e.t1
    );
}

#[test]
fn edge_params_follow_the_sense() {
    let circle = Curve::Circle {
        frame: Frame::WORLD,
        radius: 5.0,
    };
    let (a, b) = (DVec3::new(5.0, 0.0, 0.0), DVec3::new(-5.0, 0.0, 0.0));
    let (t0, t1) = edge_params(&circle, a, b, true).expect("Halbkreis");
    assert!(
        t0.abs() <= 1e-15 && (t1 - PI).abs() <= 1e-12,
        "({t0}, {t1})"
    );
    let (t0, t1) = edge_params(&circle, a, b, false).expect("Halbkreis rückwärts");
    assert!(
        t0.abs() <= 1e-15 && (t1 + PI).abs() <= 1e-12,
        "({t0}, {t1})"
    );
    assert!(t1 < t0);
    // Der Bogen über −y von (−5,0,0) zu (5,0,0): mit dem Sinn über −y, gegen ihn über +y.
    let (t0, t1) = edge_params(&circle, b, a, true).expect("untere Hälfte");
    assert!((t1 - t0 - PI).abs() <= 1e-12);
    assert!(circle.point(0.5 * (t0 + t1)).y < -4.9);
    let (t0, t1) = edge_params(&circle, b, a, false).expect("obere Hälfte");
    assert!((t0 - t1 - PI).abs() <= 1e-12);
    assert!(circle.point(0.5 * (t0 + t1)).y > 4.9);
    // Voller Kreis: ± eine Periode.
    let p = DVec3::new(0.0, 5.0, 0.0);
    let (t0, t1) = edge_params(&circle, p, p, true).expect("voller Kreis");
    assert!((t0 - PI / 2.0).abs() <= 1e-15 && (t1 - t0 - TAU).abs() <= 1e-12);
    let (t0, t1) = edge_params(&circle, p, p, false).expect("voller Kreis rückwärts");
    assert!((t1 - t0 + TAU).abs() <= 1e-12);
    // Ellipse: Viertelbogen.
    let ellipse = Curve::Ellipse {
        frame: Frame::WORLD,
        major: 4.0,
        minor: 2.0,
    };
    let (t0, t1) = edge_params(
        &ellipse,
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(0.0, 2.0, 0.0),
        true,
    )
    .expect("Ellipse");
    assert!(t0.abs() <= 1e-15 && (t1 - PI / 2.0).abs() <= 1e-12);
    let (_, t1) = edge_params(
        &ellipse,
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(0.0, 2.0, 0.0),
        false,
    )
    .expect("Ellipse rückwärts");
    assert!((t1 + 1.5 * PI).abs() <= 1e-12);
    // Gerade vorwärts und rückwärts.
    let line = Curve::Line {
        origin: DVec3::new(1.0, 0.0, 0.0),
        dir: DVec3::X,
    };
    let (a, b) = (DVec3::new(3.0, 0.0, 0.0), DVec3::new(8.0, 0.0, 0.0));
    assert_eq!(edge_params(&line, a, b, true), Ok((2.0, 7.0)));
    let (t0, t1) = edge_params(&line, b, a, false).expect("Gerade rückwärts");
    assert_eq!((t0, t1), (7.0, 2.0));
    assert!(t1 < t0);
}

#[test]
fn patch_is_unwrapped_along_the_loop() {
    let cyl = Surface::Cylinder {
        frame: Frame::WORLD,
        radius: 2.0,
    };
    let arc = |a0: f64, a1: f64, z: f64| -> Vec<DVec3> {
        (0..=16)
            .map(|k| {
                let a = a0 + (a1 - a0) * k as f64 / 16.0;
                DVec3::new(2.0 * a.cos(), 2.0 * a.sin(), z)
            })
            .collect()
    };
    // Bogen über die Naht bei ±π: von 3π/4 bis 5π/4, dann zurück in der Höhe 1.
    let mut lp = arc(0.75 * PI, 1.25 * PI, 0.0);
    lp.extend(arc(1.25 * PI, 0.75 * PI, 1.0));
    let p = derive_patch_sensed(&cyl, true, &[lp], 0.0).expect("Bereich");
    assert!((p.u0 - 0.75 * PI).abs() <= 1e-12 && (p.u1 - 1.25 * PI).abs() <= 1e-12);
    assert!(p.v0.abs() <= 1e-12 && (p.v1 - 1.0).abs() <= 1e-12);
    // Ganz um die Achse: Breite 2π, u0 in [0, 2π).
    let mut lp = arc(-0.5 * PI, 1.5 * PI, 0.0);
    lp.extend(arc(1.5 * PI, -0.5 * PI, 3.0));
    let p = derive_patch_sensed(&cyl, true, &[lp], 0.0).expect("voller Umlauf");
    assert!((p.u1 - p.u0 - TAU).abs() <= 1e-12);
    assert!((p.u0 - 1.5 * PI).abs() <= 1e-12, "u0 = {}", p.u0);
    // Mehr als ein Umlauf ist kein Parameterbereich.
    let mut lp = arc(0.0, 2.5 * PI, 0.0);
    lp.extend(arc(2.5 * PI, 0.0, 1.0));
    let Err(Skip(s)) = derive_patch_sensed(&cyl, true, &[lp], 0.0) else {
        panic!("mehr als ein Umlauf");
    };
    assert_eq!(
        s,
        "Der Parameterbereich einer Fläche reicht über mehr als einen Umlauf."
    );
    // Kugel nur aus einer Naht von Pol zu Pol (hin und zurück): Mit dem Polsprung des Kernels
    // ein voller Umlauf (Aufgabe 6).
    let sphere = Surface::Sphere {
        frame: Frame::WORLD,
        radius: 2.0,
    };
    let meridian = |a0: f64, a1: f64| -> Vec<DVec3> {
        (0..=16)
            .map(|k| {
                let b = a0 + (a1 - a0) * k as f64 / 16.0;
                DVec3::new(2.0 * b.cos(), 0.0, 2.0 * b.sin())
            })
            .collect()
    };
    let mut lp = meridian(-0.5 * PI, 0.5 * PI);
    lp.extend(meridian(0.5 * PI, -0.5 * PI));
    let p = derive_patch_sensed(&sphere, true, &[lp], 0.0).expect("Kugel");
    assert!((p.u1 - p.u0 - TAU).abs() <= 1e-12, "{p:?}");
    // Ein Kugelstück mit Pol im Rand und echter Breite (Viertel von Pol zu Pol zwischen den
    // Längen π/4 und 3π/4, im Parameterraum im Uhrzeigersinn: Sinn der Fläche falsch): Die Pole
    // selbst (genau auf der Achse, dort ist `u` beliebig, die Rückrechnung liefert 0) zählen nur
    // in v.
    let turn = |phi: f64, pts: Vec<DVec3>| -> Vec<DVec3> {
        let n = pts.len();
        pts.into_iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 || i + 1 == n {
                    DVec3::new(0.0, 0.0, p.z.signum() * 2.0)
                } else {
                    DVec3::new(p.x * phi.cos(), p.x * phi.sin(), p.z)
                }
            })
            .collect()
    };
    let mut lp = turn(0.25 * PI, meridian(-0.5 * PI, 0.5 * PI));
    lp.extend(turn(0.75 * PI, meridian(0.5 * PI, -0.5 * PI)));
    let p = derive_patch_sensed(&sphere, false, &[lp], 0.0).expect("Viertel");
    assert!(
        (p.u0 - 0.25 * PI).abs() <= 1e-12 && (p.u1 - 0.75 * PI).abs() <= 1e-12,
        "{p:?}"
    );
    assert!((p.v0 + 0.5 * PI).abs() <= 1e-12 && (p.v1 - 0.5 * PI).abs() <= 1e-12);
    // Polgrenze wie im Kernel (`points_to_uv`): |v − Pol| < 1e-7 · (1 + |Pol|). Ein Punkt
    // 1e-7 neben dem Pol bei u = 0 zählt nicht in u; 1e-6 daneben schon.
    let near_pole = |off: f64| -> Vec<DVec3> {
        let mut lp = turn(0.25 * PI, meridian(-0.5 * PI, 0.5 * PI));
        lp.extend(turn(0.75 * PI, meridian(0.5 * PI, -0.5 * PI)));
        let b = 0.5 * PI - off;
        lp.push(DVec3::new(2.0 * b.cos(), 0.0, 2.0 * b.sin()));
        lp
    };
    let p = derive_patch_sensed(&sphere, false, &[near_pole(1e-7)], 0.0).expect("am Pol");
    assert!((p.u0 - 0.25 * PI).abs() <= 1e-12, "{p:?}");
    // 1e-6 daneben ist kein Pol mehr: Der Punkt zählt in u (hier ergibt das keinen Bereich mehr
    // zwischen π/4 und 3π/4).
    let r = derive_patch_sensed(&sphere, false, &[near_pole(1e-6)], 0.0);
    assert!(
        !matches!(r, Ok(p) if (p.u0 - 0.25 * PI).abs() <= 1e-12),
        "{r:?}"
    );
}

/// Alle `same_sense` der Flächen und alle Richtungen der Ränder umgedreht.
fn inside_out(text: &str) -> String {
    let swap = |line: &str| {
        if let Some(head) = line.strip_suffix(".T.);") {
            format!("{head}.F.);")
        } else if let Some(head) = line.strip_suffix(".F.);") {
            format!("{head}.T.);")
        } else {
            line.to_string()
        }
    };
    text.lines()
        .map(|l| {
            if l.contains("ADVANCED_FACE(")
                || l.contains("FACE_BOUND(")
                || l.contains("FACE_OUTER_BOUND(")
            {
                swap(l)
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn inside_out_file_is_turned() {
    let text = inside_out(&data("box_ap214.step"));
    assert_ne!(text, data("box_ap214.step"));
    let built = build(&text);
    let s = &built.solid;
    validate(s).expect("gültig");
    let mp = mass_properties(&mesh(s, 0.01));
    assert!((mp.volume - 6000.0).abs() <= 1e-9, "Volumen {}", mp.volume);
    for (_, f) in s.faces.iter() {
        assert!(outer_turn(s, f) > 0.0);
    }
    // Flächen bleiben in der Dateireihenfolge benannt; der Boden zeigt wieder nach unten.
    let f = face_n(s, 1);
    let q = f.surface.point(5.0, 5.0);
    assert_close(outward(f, q), DVec3::NEG_Z, 1e-15);
    // Ebenso der Ring.
    let ring = build(&inside_out(&data("bore_halves.step")));
    let mp = mass_properties(&mesh(&ring.solid, 0.01));
    assert!(mp.volume > 0.0 && rel(mp.volume, 11_250.0 * PI) <= 1e-2);
    // Die Schale als ORIENTED_CLOSED_SHELL(.F.): kehrt Flächen und Ränder gemeinsam um (sonst
    // passten Rand und Außenseite nicht zusammen); einmal und zweimal umgekehrt.
    let base = data("box_ap214.step");
    for (once, twice) in [(".F.", ".T."), (".F.", ".F."), (".T.", ".T.")] {
        let wrapped = base.replace(
            "#20=MANIFOLD_SOLID_BREP('W\\X2\\00FC\\X0\\rfel',#21);",
            &format!(
                "#20=MANIFOLD_SOLID_BREP('W\\X2\\00FC\\X0\\rfel',#23);\n\
                 #22=ORIENTED_CLOSED_SHELL('',*,#21,{once});\n\
                 #23=ORIENTED_CLOSED_SHELL('',*,#22,{twice});"
            ),
        );
        assert_ne!(wrapped, base);
        let s = build(&wrapped).solid;
        let mp = mass_properties(&mesh(&s, 0.01));
        assert!((mp.volume - 6000.0).abs() <= 1e-9, "Volumen {}", mp.volume);
        for (_, f) in s.faces.iter() {
            assert!(outer_turn(&s, f) > 0.0);
        }
    }
}

#[test]
fn broken_topology_is_skipped_with_reason() {
    let text = data("box_ap214.step");
    // Fläche #35 fehlt in der Schale.
    let missing = text.replace("#33,#34,#35));", "#33,#34));");
    assert_ne!(missing, text);
    let s = skip_of(&missing, &ReadLimits::DEFAULT);
    // Ohne Kernel-Nummern: die erste Kante der linken Fläche (E4, Mitte bei (0; 10; 0)).
    assert_eq!(
        s,
        "Der Körper ist ungültig: Die Kante bei (0,00; 10,00; 0,00) gehört nur zu einer Fläche."
    );
    // Ecke V8 um 0,5 mm in x verschoben: die Gerade von E8 geht durch sie und liegt nicht mehr
    // auf der linken Fläche.
    let moved = text.replace(
        "#158=CARTESIAN_POINT('',(0.,20.,30.));",
        "#158=CARTESIAN_POINT('',(0.5,20.,30.));",
    );
    assert_ne!(moved, text);
    assert_eq!(
        skip_of(&moved, &ReadLimits::DEFAULT),
        "Die Kante bei (0,50; 10,00; 30,00) liegt nicht auf ihren Flächen."
    );
    // Flächengrenze: Text aus der Konstante, nicht aus dem Testlimit.
    assert_eq!(MAX_STEP_FACES, 20_000);
    let limits = ReadLimits {
        max_faces: 5,
        ..ReadLimits::DEFAULT
    };
    assert_eq!(
        skip_of(&text, &limits),
        "Der Körper hat mehr als 20 000 Flächen."
    );
    // Die Grenze greift vor dem Aufbau: keine Fläche wurde gelesen.
    let file = parse_with(&text, &limits);
    let progress = Progress::new();
    progress.set_total(6);
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    let opts = BuildOptions { weld_tol_mm: 1e-6 };
    assert!(build_solid(&file, units_of(&file), &items, &opts, &progress).is_err());
    assert_eq!(progress.fraction(), 0.0);
    let limits = ReadLimits {
        max_faces: 6,
        ..ReadLimits::DEFAULT
    };
    assert!(build_text(&text, &limits).is_ok());
}

#[test]
fn inconsistent_face_and_vertex_are_skipped() {
    let text = data("box_ap214.step");
    // Nur `same_sense` des Bodens gekippt: Rand und Außenseite passen nicht mehr zusammen.
    let flipped = text.replace(
        "#30=ADVANCED_FACE('',(#40),#60,.F.);",
        "#30=ADVANCED_FACE('',(#40),#60,.T.);",
    );
    assert_ne!(flipped, text);
    assert_eq!(
        skip_of(&flipped, &ReadLimits::DEFAULT),
        "Die Fläche „STEP-Fläche 1“ ist verdreht: Rand und Außenseite passen nicht zusammen."
    );
    // Eine Ecke neben ihrer Kante (die Geraden bleiben richtig).
    let off = text.replace(
        "#143=VERTEX_POINT('',#153);",
        "#143=VERTEX_POINT('',#159);\n#159=CARTESIAN_POINT('',(10.,20.,0.25));",
    );
    assert_ne!(off, text);
    assert_eq!(
        skip_of(&off, &ReadLimits::DEFAULT),
        "Die Ecke bei (10,00; 20,00; 0,25) liegt nicht auf ihrer Kante."
    );
}

#[test]
fn cancel_and_progress() {
    let text = data("bore_seam.step");
    let file = parse_with(&text, &ReadLimits::DEFAULT);
    let units = units_of(&file);
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    let opts = BuildOptions { weld_tol_mm: 1e-6 };
    let progress = Progress::new();
    progress.set_total(4);
    build_solid(&file, units, &items, &opts, &progress).expect("gelesen");
    assert_eq!(progress.fraction(), 1.0);
    let cancelled = Progress::new();
    cancelled.cancel();
    assert_eq!(
        build_solid(&file, units, &items, &opts, &cancelled).map(|_| ()),
        Err(Skip("Abgebrochen".into()))
    );
}

/// Körper des Kernels → eigener Schreiber → Leser: Fläche für Fläche dieselbe Fläche, dieselbe
/// Außennormale, derselbe Umlaufsinn, derselbe Flächeninhalt; gleiches Volumen.
#[test]
fn kernel_bodies_survive_write_and_read() {
    let tol = Tolerance::default();
    let bodies: Vec<(&str, Solid)> = vec![
        (
            "Quader",
            make_box(DVec3::new(3.0, 4.0, 5.0), &tol).expect("Quader"),
        ),
        ("Zylinder", make_cylinder(2.0, 7.0, &tol).expect("Zylinder")),
        ("Ring", kernel_ring()),
        ("Kegel", make_cone(3.0, 4.0, &tol).expect("Kegel")),
    ];
    for (name, kernel) in &bodies {
        let (text, _) = common::export(&[(name, kernel)]);
        let read = build(&text).solid;
        validate(&read).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(read.faces.len(), kernel.faces.len(), "{name}");
        let (mr, mk) = (mesh(&read, 0.001), mesh(kernel, 0.001));
        for ((rid, rf), (kid, kf)) in read.faces.iter().zip(kernel.faces.iter()) {
            assert_eq!(rf.surface.kind(), kf.surface.kind(), "{name}");
            let p = face_probe_point(kernel, kid, &tol).expect("Probepunkt");
            assert!(rf.surface.implicit(p).abs() < 1e-9, "{name}: andere Fläche");
            assert_close(outward(rf, p), outward(kf, p), 1e-9);
            assert!(outer_turn(&read, rf) > 0.0, "{name}: Umlaufsinn");
            assert_eq!(rf.inner.len(), kf.inner.len());
            let (ar, ak) = (face_area(&mr, rid), face_area(&mk, kid));
            assert!(rel(ar, ak) <= 1e-4, "{name}: Fläche {ar} statt {ak}");
        }
        let (vr, vk) = (mass_properties(&mr), mass_properties(&mk));
        assert!(
            rel(vr.volume, vk.volume) <= 1e-4,
            "{name}: {} / {}",
            vr.volume,
            vk.volume
        );
    }
}

// --- Nacharbeit 1 (Review Task 3) ---

/// Ersetzt genau einen Text (muss genau einmal vorkommen).
fn swap_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "„{from}“ nicht genau einmal");
    text.replace(from, to)
}

#[test]
fn hole_direction_is_checked() {
    // Bohrungsfläche in sich umgekehrt und beide Lochränder der Deckel gekippt: Kanten paaren,
    // Volumen positiv, aber die Bohrung zeigt ins Material (Review: V = 38 482 statt 35 343).
    let mut t = data("bore_seam.step");
    for (from, to) in [
        (
            "#605=ADVANCED_FACE('',(#617),#624,.F.);",
            "#605=ADVANCED_FACE('',(#617),#624,.T.);",
        ),
        (
            "#617=FACE_OUTER_BOUND('',#637,.T.);",
            "#617=FACE_OUTER_BOUND('',#637,.F.);",
        ),
        (
            "#612=FACE_BOUND('',#632,.T.);",
            "#612=FACE_BOUND('',#632,.F.);",
        ),
        (
            "#614=FACE_BOUND('',#634,.T.);",
            "#614=FACE_BOUND('',#634,.F.);",
        ),
    ] {
        t = swap_once(&t, from, to);
    }
    assert_eq!(
        skip_of(&t, &ReadLimits::DEFAULT),
        "Die Fläche „STEP-Fläche 1“ ist verdreht: Rand und Außenseite passen nicht zusammen."
    );
}

const NOT_CLOSED: &str = "schließt sich nicht: Eine Kante läuft falsch herum.";

#[test]
fn arc_with_wrong_sense_is_rejected() {
    // Halbkreis A1 (#561) mit falschem Sinn läuft über −y: Die Schleife der Halbschale
    // (Fläche 3) windet sich einmal um die Achse.
    let t = swap_once(
        &data("bore_halves.step"),
        "#561=EDGE_CURVE('',#511,#512,#541,.T.);",
        "#561=EDGE_CURVE('',#511,#512,#541,.F.);",
    );
    assert_eq!(
        skip_of(&t, &ReadLimits::DEFAULT),
        format!("Der Rand der Fläche „STEP-Fläche 3“ {NOT_CLOSED}")
    );
    // Quader mit Rundung (Kernel → eigener Schreiber): jeder Kreisbogen einzeln mit falschem
    // Sinn wird abgelehnt, nie still mit anderem Volumen übernommen (Review: 947,6 statt 991,4).
    let tol = Tolerance::default();
    let b = make_box(DVec3::new(10.0, 10.0, 10.0), &tol).expect("Quader");
    let edge = b.edges.iter().map(|(id, _)| id).next().expect("Kante");
    let blended = AnalyticKernel::default()
        .blend(
            &b,
            &BlendRequest {
                edges: vec![BlendEdge {
                    edge,
                    first_face: None,
                }],
                kind: BlendKind::Fillet { radius: 2.0 },
                tangent_chain: false,
            },
        )
        .expect("Rundung");
    let (text, _) = common::export(&[("Rundung", &blended.solid)]);
    build(&text);
    let circles: Vec<String> = text
        .lines()
        .filter(|l| l.contains("=CIRCLE("))
        .map(|l| l[..l.find('=').expect("=")].to_string())
        .collect();
    assert!(!circles.is_empty());
    let mut not_closed = 0;
    let mut arcs = 0;
    for line in text.lines().filter(|l| l.contains("=EDGE_CURVE(")) {
        if !circles.iter().any(|c| line.contains(&format!(",{c},"))) {
            continue;
        }
        arcs += 1;
        let flipped = if let Some(h) = line.strip_suffix(".T.);") {
            format!("{h}.F.);")
        } else {
            format!("{}.T.);", line.strip_suffix(".F.);").expect("Sinn"))
        };
        let s = skip_of(&swap_once(&text, line, &flipped), &ReadLimits::DEFAULT);
        if s.starts_with("Der Rand der Fläche „STEP-Fläche ") && s.ends_with(NOT_CLOSED) {
            not_closed += 1;
        }
    }
    assert!(arcs >= 2, "Bögen: {arcs}");
    assert!(not_closed >= 1);
}

#[test]
fn holes_near_the_seam_keep_their_place() {
    let cyl = Surface::Cylinder {
        frame: Frame::WORLD,
        radius: 2.0,
    };
    let at = |u: f64, v: f64| DVec3::new(2.0 * u.cos(), 2.0 * u.sin(), v);
    let rect = |u0: f64, u1: f64| -> Vec<DVec3> {
        let mut pts = Vec::new();
        for k in 0..=32 {
            pts.push(at(u0 + (u1 - u0) * k as f64 / 32.0, 0.0));
        }
        for k in 0..=32 {
            pts.push(at(u1 - (u1 - u0) * k as f64 / 32.0, 10.0));
        }
        pts
    };
    let hole = |uc: f64| -> Vec<DVec3> {
        (0..=32)
            .map(|k| {
                let s = -TAU * k as f64 / 32.0;
                at(uc + 0.2 * s.cos(), 5.0 + 2.0 * s.sin())
            })
            .collect()
    };
    // Voller Mantel, Naht bei u = 0, Loch bei u = 5,28.
    let p =
        derive_patch_sensed(&cyl, true, &[rect(0.0, TAU), hole(5.28)], 0.0).expect("voller Mantel");
    assert!(p.u0.abs() <= 1e-12 && (p.u1 - TAU).abs() <= 1e-12, "{p:?}");
    assert!(p.v0.abs() <= 1e-12 && (p.v1 - 10.0).abs() <= 1e-12);
    // Mantelstück u 2,5 … 4,0 (über ±π), Loch bei u = 3,6.
    let p = derive_patch_sensed(&cyl, true, &[rect(2.5, 4.0), hole(3.6)], 0.0).expect("Stück");
    assert!(
        (p.u0 - 2.5).abs() <= 1e-12 && (p.u1 - 4.0).abs() <= 1e-12,
        "{p:?}"
    );
}

#[test]
fn curved_face_outer_loop_by_turn() {
    // Tasche bei u ≈ π/2 und nahe der Naht bei u ≈ −0,35 (≡ 5,93): Mit dem Loch zuerst an
    // `derive_patch_sensed` würde der Mantel dort über mehr als einen Umlauf entfaltet.
    hole_listed_first(DVec2::new(-2.0, 8.0), DVec2::new(2.0, 12.0));
    let c = DVec2::new(10.0 * (-0.35f64).cos(), 10.0 * (-0.35f64).sin());
    hole_listed_first(c - DVec2::splat(2.0), c + DVec2::splat(2.0));
}

/// Zylinder R 10 mit Tasche (Rechteck `lo`–`hi` in xy, z 8…12) durch die Wand: Der Mantel hat
/// ein Loch. In der Datei steht das Loch zuerst, und beide Ränder sind FACE_BOUND.
fn hole_listed_first(lo: DVec2, hi: DVec2) {
    let tol = Tolerance::default();
    let cyl = make_cylinder(10.0, 20.0, &tol).expect("Zylinder");
    let pocket = extrude(
        &Profile {
            outer: ProfileLoop::rectangle(lo, hi),
            holes: vec![],
        },
        &Frame {
            origin: DVec3::new(0.0, 0.0, 8.0),
            ..Frame::WORLD
        },
        4.0,
        &tol,
    )
    .expect("Tasche");
    let body = AnalyticKernel::default()
        .boolean(&cyl, &pocket, BoolOp::Difference)
        .expect("Abziehen");
    let (text, _) = common::export(&[("Tasche", &body)]);
    let reference = build(&text).solid;
    let cyl_ids: Vec<String> = text
        .lines()
        .filter(|l| l.contains("=CYLINDRICAL_SURFACE("))
        .map(|l| l[..l.find('=').expect("=")].to_string())
        .collect();
    let face_line = text
        .lines()
        .find(|l| {
            l.contains("=ADVANCED_FACE('',(#")
                && cyl_ids.iter().any(|c| l.contains(&format!("),{c},")))
                && l.split("),#").next().is_some_and(|b| b.contains(','))
        })
        .expect("Mantel mit Loch")
        .to_string();
    let open = face_line.find("(#").expect("Liste") + 1;
    let close = face_line[open..].find(')').expect("Ende") + open;
    let bounds: Vec<&str> = face_line[open..close].split(',').collect();
    assert_eq!(bounds.len(), 2, "{face_line}");
    let swapped = format!(
        "{}{},{}{}",
        &face_line[..open],
        bounds[1],
        bounds[0],
        &face_line[close..]
    );
    let mut t = swap_once(&text, &face_line, &swapped);
    t = swap_once(
        &t,
        &format!("{}=FACE_OUTER_BOUND(", bounds[0]),
        &format!("{}=FACE_BOUND(", bounds[0]),
    );
    let read = build(&t).solid;
    let (a, b) = (
        mass_properties(&mesh(&reference, 0.01)),
        mass_properties(&mesh(&read, 0.01)),
    );
    assert!((a.volume - b.volume).abs() <= 1e-9 * a.volume);
    // Kennungen der Kanten folgen der Lesereihenfolge; verglichen werden die Ecken der Ränder.
    let corners = |s: &Solid, lp: &planora_kernel::Loop| -> Vec<DVec3> {
        lp.half_edges
            .iter()
            .map(|he| s.vertices[s.half_edge_start(*he)].point)
            .collect()
    };
    for ((_, f), (_, g)) in reference.faces.iter().zip(read.faces.iter()) {
        assert_eq!(corners(&reference, &f.outer), corners(&read, &g.outer));
        assert_eq!(f.inner.len(), g.inner.len());
        for (a, b) in f.inner.iter().zip(&g.inner) {
            assert_eq!(corners(&reference, a), corners(&read, b));
        }
    }
}

/// Kleiner Körper mit Kontext und Darstellung (Kontext #105, Körper #20).
fn small_body(entities: &str) -> String {
    format!(
        "ISO-10303-21;\n/* Selbst geschrieben für die Tests von PlanoraCAD (GPL-3.0-only). */\n\
         HEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
         FILE_NAME('x.step','2026-10-21T00:00:00',(''),(''),'','','');\n\
         FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n\
         #9=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#20),#105);\n\
         #100=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
         #105=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#100))\
         REPRESENTATION_CONTEXT('',''));\n{entities}ENDSEC;\nEND-ISO-10303-21;\n"
    )
}

#[test]
fn hostile_topology_is_named() {
    let text = data("box_ap214.step");
    let skip = |t: &str| skip_of(t, &ReadLimits::DEFAULT);
    // Fläche ohne Rand: Meldung, keine Panik.
    let s = skip(&swap_once(
        &text,
        "#30=ADVANCED_FACE('',(#40),#60,.F.);",
        "#30=ADVANCED_FACE('',(),#60,.F.);",
    ));
    assert!(s.ends_with(": Eine Fläche hat keinen Rand."), "{s}");
    // Rand ohne Kanten.
    let s = skip(&swap_once(
        &text,
        "#51=EDGE_LOOP('',(#204,#205,#206,#207));",
        "#51=EDGE_LOOP('',());",
    ));
    assert!(s.ends_with(": Ein Rand hat keine Kanten."), "{s}");
    // Zwei äußere Ränder.
    let s = skip(&swap_once(
        &text,
        "#30=ADVANCED_FACE('',(#40),#60,.F.);",
        "#30=ADVANCED_FACE('',(#40,#40),#60,.F.);",
    ));
    assert!(
        s.ends_with(": Eine Fläche hat mehr als einen äußeren Rand."),
        "{s}"
    );
    // Kante der Länge null.
    let s = skip(&swap_once(
        &text,
        "#121=EDGE_CURVE('',#141,#142,#161,.T.);",
        "#121=EDGE_CURVE('',#141,#141,#161,.T.);",
    ));
    assert!(s.ends_with(": Eine Kante hat die Länge null."), "{s}");
    // Leere Schale.
    let empty = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("(#30") && !l.trim_start().starts_with("#33,"))
        .map(|l| {
            if l.starts_with("#21=CLOSED_SHELL(") {
                "#21=CLOSED_SHELL('',());"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(empty.contains("#21=CLOSED_SHELL('',());"));
    assert_eq!(skip(&empty), "Der Körper hat keine Flächen.");
    // Ecke nur am Ende ihrer Kanten neben ihnen (E5 umgedreht, damit V5 nirgends Anfang ist);
    // x = −0,001 erscheint als „0,00“, nie als „-0,00“.
    let mut t = text.clone();
    for (from, to) in [
        (
            "#125=EDGE_CURVE('',#145,#146,#165,.T.);",
            "#125=EDGE_CURVE('',#146,#145,#165,.F.);",
        ),
        (
            "#207=ORIENTED_EDGE('',*,*,#125,.F.);",
            "#207=ORIENTED_EDGE('',*,*,#125,.T.);",
        ),
        (
            "#210=ORIENTED_EDGE('',*,*,#125,.F.);",
            "#210=ORIENTED_EDGE('',*,*,#125,.T.);",
        ),
        (
            "#145=VERTEX_POINT('',#155);",
            "#145=VERTEX_POINT('',#159);\n#159=CARTESIAN_POINT('',(-0.001,0.,30.25));",
        ),
    ] {
        t = swap_once(&t, from, to);
    }
    assert_eq!(
        skip(&t),
        "Die Ecke bei (0,00; 0,00; 30,25) liegt nicht auf ihrer Kante."
    );
    // Flacher Körper: zwei deckungsgleiche Flächen, Volumen genau null.
    let flat = small_body(
        "#20=MANIFOLD_SOLID_BREP('',#21);\n#21=CLOSED_SHELL('',(#30,#31));\n\
         #30=ADVANCED_FACE('',(#40),#60,.T.);\n#31=ADVANCED_FACE('',(#41),#60,.F.);\n\
         #40=FACE_OUTER_BOUND('',#50,.T.);\n#41=FACE_OUTER_BOUND('',#50,.F.);\n\
         #50=EDGE_LOOP('',(#71,#72,#73,#74));\n\
         #71=ORIENTED_EDGE('',*,*,#81,.T.);\n#72=ORIENTED_EDGE('',*,*,#82,.T.);\n\
         #73=ORIENTED_EDGE('',*,*,#83,.T.);\n#74=ORIENTED_EDGE('',*,*,#84,.T.);\n\
         #81=EDGE_CURVE('',#91,#92,#111,.T.);\n#82=EDGE_CURVE('',#92,#93,#112,.T.);\n\
         #83=EDGE_CURVE('',#93,#94,#113,.T.);\n#84=EDGE_CURVE('',#94,#91,#114,.T.);\n\
         #91=VERTEX_POINT('',#121);\n#92=VERTEX_POINT('',#122);\n\
         #93=VERTEX_POINT('',#123);\n#94=VERTEX_POINT('',#124);\n\
         #111=LINE('',#121,#131);\n#112=LINE('',#122,#132);\n\
         #113=LINE('',#123,#133);\n#114=LINE('',#124,#134);\n\
         #121=CARTESIAN_POINT('',(0.,0.,0.));\n#122=CARTESIAN_POINT('',(10.,0.,0.));\n\
         #123=CARTESIAN_POINT('',(10.,10.,0.));\n#124=CARTESIAN_POINT('',(0.,10.,0.));\n\
         #131=VECTOR('',#141,1.);\n#132=VECTOR('',#142,1.);\n\
         #133=VECTOR('',#143,1.);\n#134=VECTOR('',#144,1.);\n\
         #141=DIRECTION('',(1.,0.,0.));\n#142=DIRECTION('',(0.,1.,0.));\n\
         #143=DIRECTION('',(-1.,0.,0.));\n#144=DIRECTION('',(0.,-1.,0.));\n\
         #60=PLANE('',#61);\n#61=AXIS2_PLACEMENT_3D('',#121,$,$);\n",
    );
    assert_eq!(skip(&flat), "Der Körper hat kein Volumen.");
}

// --- Nacharbeit 2 (Re-Review Task 3) ---

/// `bore_seam.step` mit Ecken, die um 5e-4 mm in Umfangsrichtung neben der Naht liegen, und der
/// Genauigkeit `uncertainty` (Text der Datei).
fn coarse_bore_seam(uncertainty: &str) -> String {
    let mut t = data("bore_seam.step");
    for (from, to) in [
        (
            "#501=CARTESIAN_POINT('',(20.,0.,0.));",
            "#501=CARTESIAN_POINT('',(20.,0.0005,0.));",
        ),
        (
            "#507=CARTESIAN_POINT('',(5.,0.,30.));",
            "#507=CARTESIAN_POINT('',(5.,-0.0005,30.));",
        ),
        (
            "LENGTH_MEASURE(1.E-06)",
            &format!("LENGTH_MEASURE({uncertainty})"),
        ),
    ] {
        t = swap_once(&t, from, to);
    }
    t
}

#[test]
fn coarse_seam_is_taken_within_the_file_tolerance() {
    // Genauigkeit 1e-3: Der Mantel überspannt eine Periode plus den Versatz an der Naht
    // (etwa 2,5e-5 bzw. 1e-4 rad); der Bereich wird auf genau eine Periode gekappt.
    let built = build(&coarse_bore_seam("1.E-03"));
    validate(&built.solid).expect("gültig");
    for (_, f) in built.solid.faces.iter() {
        if let Some(p) = f.patch {
            assert!((p.u1 - p.u0 - TAU).abs() <= 1e-12, "{p:?}");
        }
    }
    // Gegen den genauen Ring bei gleicher Sehne (das grobe Netz hält den Test schnell); der
    // Versatz von 5e-4 mm ändert das Volumen um weit weniger als 1e-4.
    let mp = mass_properties(&mesh(&built.solid, 0.01));
    let exact = mass_properties(&mesh(&build(&data("bore_seam.step")).solid, 0.01));
    assert!(
        rel(mp.volume, exact.volume) <= 1e-4,
        "Volumen {}",
        mp.volume
    );
    assert!(
        rel(mp.volume, 11_250.0 * PI) <= 1e-3,
        "Volumen {}",
        mp.volume
    );
    // Genauigkeit 1e-6: Der Versatz ist größer als die Datei erlaubt.
    let s = skip_of(&coarse_bore_seam("1.E-06"), &ReadLimits::DEFAULT);
    assert!(s.ends_with("reicht über mehr als einen Umlauf."), "{s}");
}

#[test]
fn too_wide_range_stays_rejected() {
    let cyl = Surface::Cylinder {
        frame: Frame::WORLD,
        radius: 20.0,
    };
    let arc = |a0: f64, a1: f64, z: f64| -> Vec<DVec3> {
        (0..=64)
            .map(|k| {
                let a = a0 + (a1 - a0) * k as f64 / 64.0;
                DVec3::new(20.0 * a.cos(), 20.0 * a.sin(), z)
            })
            .collect()
    };
    let lp = |w: f64| {
        let mut lp = arc(0.0, w, 0.0);
        lp.extend(arc(w, 0.0, 1.0));
        lp
    };
    // 1,1 Perioden: auch mit grober Genauigkeit abgelehnt.
    for tol in [0.0, 1e-3] {
        let Err(Skip(s)) = derive_patch_sensed(&cyl, true, &[lp(1.1 * TAU)], tol) else {
            panic!("1,1 Perioden bei {tol}");
        };
        assert!(s.ends_with("reicht über mehr als einen Umlauf."), "{s}");
    }
    // Eine Periode plus 4e-5 rad (8e-4 mm am Umfang): bei 1e-3 gekappt, bei 1e-6 abgelehnt.
    let p = derive_patch_sensed(&cyl, true, &[lp(TAU + 4e-5)], 1e-3).expect("grob");
    assert!((p.u1 - p.u0 - TAU).abs() <= 1e-12, "{p:?}");
    assert!(derive_patch_sensed(&cyl, true, &[lp(TAU + 4e-5)], 1e-6).is_err());
    // Dünner Zylinder (R 0,1): 2 · 1e-3 / 0,1 = 0,02 rad wäre zu großzügig; der Schlupf ist
    // auf 1e-3 der Periode (0,0063 rad) begrenzt.
    let thin = Surface::Cylinder {
        frame: Frame::WORLD,
        radius: 0.1,
    };
    let thin_lp = |w: f64| -> Vec<DVec3> {
        let pt = |a: f64, z: f64| DVec3::new(0.1 * a.cos(), 0.1 * a.sin(), z);
        let mut lp: Vec<DVec3> = (0..=64).map(|k| pt(w * k as f64 / 64.0, 0.0)).collect();
        lp.extend((0..=64).map(|k| pt(w * (64 - k) as f64 / 64.0, 1.0)));
        lp
    };
    assert!(derive_patch_sensed(&thin, true, &[thin_lp(TAU + 0.005)], 1e-3).is_ok());
    assert!(derive_patch_sensed(&thin, true, &[thin_lp(TAU + 0.01)], 1e-3).is_err());
    // Kegel (Radius 5 bei v = 0, Spitze bei v = 5) mit zwei Randpunkten 1e-4 mm vor der Spitze:
    // Punkte näher als 10 · Genauigkeit an der Achse bestimmen den Schlupf nicht (sonst wäre er
    // bis zur Obergrenze groß); 0,004 rad Überstand bleiben bei 1e-3 abgelehnt (2e-3/5 = 4e-4).
    let cone = Surface::Cone {
        frame: Frame::WORLD,
        radius: 5.0,
        slope: -1.0,
    };
    let on = |a: f64, v: f64| DVec3::new((5.0 - v) * a.cos(), (5.0 - v) * a.sin(), v);
    let w = TAU + 0.004;
    let mut cone_lp: Vec<DVec3> = (0..=64).map(|k| on(w * k as f64 / 64.0, 0.0)).collect();
    cone_lp.push(on(w, 5.0 - 1e-4));
    cone_lp.push(on(0.0, 5.0 - 1e-4));
    assert!(derive_patch_sensed(&cone, true, &[cone_lp], 1e-3).is_err());
    // Der Sinn der Fläche ändert ohne Pol nichts.
    assert_eq!(
        derive_patch_sensed(&cyl, true, &[lp(TAU)], 0.0).map(|p| p.u1 - p.u0),
        derive_patch_sensed(&cyl, false, &[lp(TAU)], 0.0).map(|p| p.u1 - p.u0)
    );
}

/// Ring-Torus R 20, r 5 um z aus vier Viertelflächen (u und v je in zwei Hälften; Meridiane bei
/// u = 0 und π, Parallelkreise außen und innen); `m0a` ist der Sinn des Meridianbogens bei u = 0
/// von v = 0 nach π.
fn quartered_torus(m0a: &str) -> String {
    small_body(&format!(
        "#20=MANIFOLD_SOLID_BREP('',#21);\n#21=CLOSED_SHELL('',(#31,#32,#33,#34));\n\
         #31=ADVANCED_FACE('',(#41),#60,.T.);\n#32=ADVANCED_FACE('',(#42),#60,.T.);\n\
         #33=ADVANCED_FACE('',(#43),#60,.T.);\n#34=ADVANCED_FACE('',(#44),#60,.T.);\n\
         #41=FACE_OUTER_BOUND('',#51,.T.);\n#42=FACE_OUTER_BOUND('',#52,.T.);\n\
         #43=FACE_OUTER_BOUND('',#53,.T.);\n#44=FACE_OUTER_BOUND('',#54,.T.);\n\
         #51=EDGE_LOOP('',(#201,#202,#203,#204));\n#52=EDGE_LOOP('',(#205,#206,#207,#208));\n\
         #53=EDGE_LOOP('',(#209,#210,#211,#212));\n#54=EDGE_LOOP('',(#213,#214,#215,#216));\n\
         #201=ORIENTED_EDGE('',*,*,#81,.T.);\n#202=ORIENTED_EDGE('',*,*,#87,.T.);\n\
         #203=ORIENTED_EDGE('',*,*,#83,.F.);\n#204=ORIENTED_EDGE('',*,*,#85,.F.);\n\
         #205=ORIENTED_EDGE('',*,*,#82,.T.);\n#206=ORIENTED_EDGE('',*,*,#85,.T.);\n\
         #207=ORIENTED_EDGE('',*,*,#84,.F.);\n#208=ORIENTED_EDGE('',*,*,#87,.F.);\n\
         #209=ORIENTED_EDGE('',*,*,#83,.T.);\n#210=ORIENTED_EDGE('',*,*,#88,.T.);\n\
         #211=ORIENTED_EDGE('',*,*,#81,.F.);\n#212=ORIENTED_EDGE('',*,*,#86,.F.);\n\
         #213=ORIENTED_EDGE('',*,*,#84,.T.);\n#214=ORIENTED_EDGE('',*,*,#86,.T.);\n\
         #215=ORIENTED_EDGE('',*,*,#82,.F.);\n#216=ORIENTED_EDGE('',*,*,#88,.F.);\n\
         #81=EDGE_CURVE('',#91,#93,#101,.T.);\n#82=EDGE_CURVE('',#93,#91,#101,.T.);\n\
         #83=EDGE_CURVE('',#92,#94,#102,.T.);\n#84=EDGE_CURVE('',#94,#92,#102,.T.);\n\
         #85=EDGE_CURVE('',#91,#92,#103,{m0a});\n#86=EDGE_CURVE('',#92,#91,#103,.T.);\n\
         #87=EDGE_CURVE('',#93,#94,#104,.T.);\n#88=EDGE_CURVE('',#94,#93,#104,.T.);\n\
         #91=VERTEX_POINT('',#121);\n#92=VERTEX_POINT('',#122);\n\
         #93=VERTEX_POINT('',#123);\n#94=VERTEX_POINT('',#124);\n\
         #121=CARTESIAN_POINT('',(25.,0.,0.));\n#122=CARTESIAN_POINT('',(15.,0.,0.));\n\
         #123=CARTESIAN_POINT('',(-25.,0.,0.));\n#124=CARTESIAN_POINT('',(-15.,0.,0.));\n\
         #101=CIRCLE('',#131,25.);\n#102=CIRCLE('',#131,15.);\n\
         #103=CIRCLE('',#132,5.);\n#104=CIRCLE('',#133,5.);\n\
         #131=AXIS2_PLACEMENT_3D('',#125,#141,#142);\n\
         #132=AXIS2_PLACEMENT_3D('',#126,#143,#142);\n\
         #133=AXIS2_PLACEMENT_3D('',#127,#144,#145);\n\
         #125=CARTESIAN_POINT('',(0.,0.,0.));\n#126=CARTESIAN_POINT('',(20.,0.,0.));\n\
         #127=CARTESIAN_POINT('',(-20.,0.,0.));\n\
         #141=DIRECTION('',(0.,0.,1.));\n#142=DIRECTION('',(1.,0.,0.));\n\
         #143=DIRECTION('',(0.,-1.,0.));\n#144=DIRECTION('',(0.,1.,0.));\n\
         #145=DIRECTION('',(-1.,0.,0.));\n\
         #60=TOROIDAL_SURFACE('',#131,20.,5.);\n"
    ))
}

#[test]
fn torus_quarters_close_in_v() {
    let built = build(&quartered_torus(".T."));
    let s = &built.solid;
    validate(s).expect("gültig");
    let mp = mass_properties(&mesh(s, 0.001));
    let want = 2.0 * PI * PI * 20.0 * 25.0;
    assert!(
        rel(mp.volume, want) <= 5e-4,
        "Volumen {} statt {want}",
        mp.volume
    );
    for (_, f) in s.faces.iter() {
        let p = f.patch.expect("Bereich");
        assert!((p.u1 - p.u0 - PI).abs() <= 1e-12 && (p.v1 - p.v0 - PI).abs() <= 1e-12);
        assert!((0.0..TAU).contains(&p.v0), "{p:?}");
    }
    // Meridianbogen mit falschem Sinn: Die Viertelfläche windet sich einmal in v.
    assert_eq!(
        skip_of(&quartered_torus(".F."), &ReadLimits::DEFAULT),
        format!("Der Rand der Fläche „STEP-Fläche 1“ {NOT_CLOSED}")
    );
}
