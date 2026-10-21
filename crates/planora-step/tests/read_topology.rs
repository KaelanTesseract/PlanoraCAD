//! STEP-Leser, Stufe 3: Topologie, Grundweg (STEP-Import, Aufgabe 3). Die Dateien unter
//! `tests/data` sind von Hand geschrieben; die Erwartungen sind Handrechnungen und Körper, die der
//! Kernel selbst baut (Fläche für Fläche: Außennormale, Laufrichtung der Ränder, Volumen, Fläche).

mod common;

use planora_exchange::Progress;
use planora_kernel::boolean::{face_probe_point, oriented_area, points_to_uv};
use planora_kernel::primitives::{make_box, make_cone, make_cylinder};
use planora_kernel::{Curve, Face, Id, Profile, ProfileLoop, Solid, Surface, extrude, validate};
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};
use planora_mesh::{Mesh, TessellationOptions, mass_properties, tessellate};
use planora_step::read::geom::Skip;
use planora_step::read::topo::{
    BuildOptions, BuiltSolid, STEP_FACE_ROLE, build_solid, derive_patch, edge_params,
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
    let p = derive_patch(&cyl, &[lp]).expect("Bereich");
    assert!((p.u0 - 0.75 * PI).abs() <= 1e-12 && (p.u1 - 1.25 * PI).abs() <= 1e-12);
    assert!(p.v0.abs() <= 1e-12 && (p.v1 - 1.0).abs() <= 1e-12);
    // Ganz um die Achse: Breite 2π, u0 in [0, 2π).
    let mut lp = arc(-0.5 * PI, 1.5 * PI, 0.0);
    lp.extend(arc(1.5 * PI, -0.5 * PI, 3.0));
    let p = derive_patch(&cyl, &[lp]).expect("voller Umlauf");
    assert!((p.u1 - p.u0 - TAU).abs() <= 1e-12);
    assert!((p.u0 - 1.5 * PI).abs() <= 1e-12, "u0 = {}", p.u0);
    // Mehr als ein Umlauf ist kein Parameterbereich.
    let mut lp = arc(0.0, 2.5 * PI, 0.0);
    lp.extend(arc(2.5 * PI, 0.0, 1.0));
    let Err(Skip(s)) = derive_patch(&cyl, &[lp]) else {
        panic!("mehr als ein Umlauf");
    };
    assert_eq!(
        s,
        "Der Parameterbereich einer Fläche reicht über mehr als einen Umlauf."
    );
    // Kugel nur aus einer Naht von Pol zu Pol (hin und zurück): `u` hat ohne Polsprung keine
    // Breite; das ist (noch) kein Bereich statt eines leeren.
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
    let Err(Skip(s)) = derive_patch(&sphere, &[lp]) else {
        panic!("Kugel ohne Polsprung");
    };
    assert_eq!(
        s,
        "Eine Fläche, deren Rand durch einen Pol läuft, wird noch nicht unterstützt."
    );
    // Ein Kugelstück mit Pol im Rand und echter Breite (Viertel von Pol zu Pol zwischen den
    // Längen π/4 und 3π/4): Die Pole selbst (genau auf der Achse, dort ist `u` beliebig, die
    // Rückrechnung liefert 0) zählen nur in v.
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
    let p = derive_patch(&sphere, &[lp]).expect("Viertel");
    assert!(
        (p.u0 - 0.25 * PI).abs() <= 1e-12 && (p.u1 - 0.75 * PI).abs() <= 1e-12,
        "{p:?}"
    );
    assert!((p.v0 + 0.5 * PI).abs() <= 1e-12 && (p.v1 - 0.5 * PI).abs() <= 1e-12);
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
    assert!(s.starts_with("Der Körper ist ungültig:"), "{s}");
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
