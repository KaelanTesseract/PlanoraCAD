//! Machbarkeitstor (STEP-Import, Aufgabe 4): Eingelesene Körper müssen im Kernel genauso
//! weiterarbeiten wie selbst gebaute. Je Körper fünf Schritte: (1) `validate`, (2) Volumen gegen
//! die Handrechnung, (3) Abziehen eines Stabs 100 × 4 × 4 quer durch die Mitte, (4) Rundung
//! Radius 1 an der oberen äußeren Kante, (5) Schnitt in der Mitte (Ansicht von vorn).
//!
//! Genaue Fälle sind die Dateien unter `tests/data`; die groben Fassungen (Messfälle) erzeugt der
//! Test: jede Ecke um 9e-4 mm in einer festen Richtung je Ecke verschoben, Genauigkeit der Datei
//! `1.E-3`; Flächen, Kreise und Geraden bleiben. Ein Schritt gilt nur dann als bestanden, wenn
//! auch das Volumen stimmt (genau: relativ 1e-4, grob: 1e-3; Sehne 0,001) – nie „Ok“ mit
//! falschem Körper.

use planora_drawing::model::ViewKind;
use planora_drawing::section::{CutOutcome, SectionPlane, cut_body};
use planora_exchange::Progress;
use planora_kernel::{
    AnalyticKernel, BlendEdge, BlendKind, BlendRequest, BoolOp, Edge, Id, Kernel, Solid, Surface,
    validate,
};
use planora_math::{DMat4, DVec3, PI, TAU, Tolerance};
use planora_mesh::{TessellationOptions, boolean_volume_plausible, mass_properties, tessellate};
use planora_step::read::geom::Skip;
use planora_step::read::topo::{BuildOptions, STEP_FACE_ROLE, build_solid, read_raw};
use planora_step::read::units::{Units, context_units};
use planora_step::{ReadLimits, StepFile};
use std::path::PathBuf;

/// Sehne der Volumenrechnung (mm).
const CHORD: f64 = 0.001;
/// Erlaubte Volumenabweichung (relativ) genauer Körper.
const EXACT_REL: f64 = 1e-4;
/// Erlaubte Volumenabweichung (relativ) grober Körper: nie „Ok“ mit mehr.
const COARSE_REL: f64 = 1e-3;
/// Versatz jeder Ecke in den Messfällen (mm).
const COARSE_SHIFT: f64 = 9e-4;
/// Radius der Rundung (mm).
const FILLET: f64 = 1.0;
/// Ring: Außenradius, Bohrung, Höhe (mm).
const RING_R: f64 = 20.0;
const RING_BORE: f64 = 5.0;
const RING_H: f64 = 30.0;
/// Halbe Breite des Stabs quer zu seiner Länge (mm); Länge 100.
const BAR_HALF: f64 = 2.0;
const BAR_LEN: f64 = 100.0;

const STEPS: [&str; 5] = ["validate", "Volumen", "Abziehen", "Rundung", "Schnitt"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Ok,
    Fails,
}

use Outcome::{Fails, Ok as Pass};

/// Gemessene Ergebnisse der groben Fassungen (Schritte wie [`STEPS`]).
///
/// Messung 2026-10-21 (Aufgabe 4, Basis e54ae6a, ohne Nachziehen): Einlesen, `validate` und
/// Volumen gelangen; Abziehen, Rundung und Schnitt lehnte der Kernel ab, weil die Ecken weiter als
/// seine Toleranz (1e-6) neben den Kanten lagen („… liegen zu knapp beieinander …“, „Eine Kante
/// der Fläche „STEP-Fläche 3“ passt nicht zur Rundung“, „Eine Aussparung oder Kante … liegt im
/// Bereich der Rundung …“) – nie mit falschem Körper. Damit galt P16.
///
/// Messung 2026-10-23 (Aufgabe 7, mit Nachziehen der Ecken auf den Schnitt ihrer Flächen und
/// Kanten): alle fünf Schritte bestehen, die Rundung auch im Bau (siehe [`rounded`]).
const EXPECTED_COARSE: [(&str, [Outcome; 5]); 3] = [
    ("box_ap214.step", [Pass, Pass, Pass, Pass, Pass]),
    ("bore_halves.step", [Pass, Pass, Pass, Pass, Pass]),
    ("bore_seam.step", [Pass, Pass, Pass, Pass, Pass]),
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// Quader 10 × 20 × 30, Ecke im Ursprung.
    Box,
    /// Zylinder R 20, h 30 mit Bohrung r 5, Achse z durch den Ursprung.
    Ring,
}

fn kind_of(file: &str) -> Kind {
    if file.starts_with("box") {
        Kind::Box
    } else {
        Kind::Ring
    }
}

/// Querschnitt eines Kreises vom Radius `r` im Streifen `|y| ≤ a`.
fn strip(r: f64, a: f64) -> f64 {
    2.0 * (a * (r * r - a * a).sqrt() + r * r * (a / r).asin())
}

/// Fläche zwischen Ecke und Viertelkreis (Rundung) und Abstand ihres Schwerpunkts von der Ecke
/// je Achse.
fn spandrel(r: f64) -> (f64, f64) {
    (
        r * r * (1.0 - PI / 4.0),
        r * (10.0 - 3.0 * PI) / (12.0 - 3.0 * PI),
    )
}

impl Kind {
    /// Mitte des Körpers.
    fn center(self) -> DVec3 {
        match self {
            Kind::Box => DVec3::new(5.0, 10.0, 15.0),
            Kind::Ring => DVec3::new(0.0, 0.0, 0.5 * RING_H),
        }
    }

    fn volume(self) -> f64 {
        match self {
            Kind::Box => 6000.0,
            Kind::Ring => PI * (RING_R * RING_R - RING_BORE * RING_BORE) * RING_H,
        }
    }

    /// Volumen, das der Stab entlang x durch die Mitte wegnimmt.
    fn bar_removed(self) -> f64 {
        let side = 2.0 * BAR_HALF;
        match self {
            Kind::Box => 10.0 * side * side,
            // Zwei Wände (x > 0, x < 0) über die Höhe des Stabs.
            Kind::Ring => side * (strip(RING_R, BAR_HALF) - strip(RING_BORE, BAR_HALF)),
        }
    }

    /// Volumen, das die Rundung wegnimmt (Pappus beim Kreis).
    fn blend_removed(self) -> f64 {
        let (area, c) = spandrel(FILLET);
        match self {
            Kind::Box => area * 10.0,
            Kind::Ring => area * TAU * (RING_R - c),
        }
    }

    /// Offenbleibendes Volumen nach dem Schnitt (Ansicht von vorn behält `y > Ebene`).
    fn cut_volume(self, nudged: bool) -> f64 {
        let nudge = if nudged {
            planora_drawing::section::SECTION_NUDGE_MM
        } else {
            0.0
        };
        match self {
            Kind::Box => 6000.0 / 2.0 - nudge * 10.0 * 30.0,
            Kind::Ring => self.volume() / 2.0 - nudge * 2.0 * (RING_R - RING_BORE) * RING_H,
        }
    }

    /// Die obere äußere Kante: beim Quader oben vorn (y = 0, z = 30), beim Ring der Kreis R 20
    /// oben (bei Halbschalen zwei Bögen).
    fn top_edges(self, s: &Solid) -> Vec<Id<Edge>> {
        let on = |p: DVec3| match self {
            Kind::Box => p.y.abs() < 0.01 && (p.z - 30.0).abs() < 0.01,
            Kind::Ring => {
                (p.z - RING_H).abs() < 0.01 && (p.truncate().length() - RING_R).abs() < 0.01
            }
        };
        s.edges
            .iter()
            .filter(|(_, e)| {
                [0.0, 0.25, 0.5, 0.75, 1.0]
                    .iter()
                    .all(|k| on(e.curve.point(e.t0 + (e.t1 - e.t0) * k)))
            })
            .map(|(id, _)| id)
            .collect()
    }

    fn top_edge_count(self, file: &str) -> usize {
        if file.starts_with("bore_halves") {
            2
        } else {
            1
        }
    }
}

fn data(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn units_of(file: &StepFile) -> Result<Units, String> {
    let rep = *file
        .ids_of("ADVANCED_BREP_SHAPE_REPRESENTATION")
        .first()
        .ok_or("keine Darstellung")?;
    let e = file.entity(rep).map_err(|e| format!("{e:?}"))?;
    let ctx = e
        .arg("ADVANCED_BREP_SHAPE_REPRESENTATION", 2)
        .and_then(|v| v.reference())
        .map_err(|e| e.to_string())?;
    context_units(file, ctx).map_err(|e| format!("{e:?}"))
}

/// Ecken der Rohform, wie die Datei sie nennt (vor Zusammenziehen und Nachziehen).
fn raw_vertices(text: &str) -> Vec<DVec3> {
    let file = StepFile::parse(
        text.as_bytes().to_vec(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
    .expect("Datei lädt");
    let units = units_of(&file).expect("Einheiten");
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    read_raw(&file, units, &items, units.weld_tol_mm(), &Progress::new())
        .map_err(|Skip(m)| m)
        .expect("Rohform")
        .vertices
}

/// Liest den Körper der Datei; liefert auch die Genauigkeit, mit der gebaut wurde.
fn read(text: &str) -> Result<(Solid, f64), String> {
    let file = StepFile::parse(
        text.as_bytes().to_vec(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let units = units_of(&file)?;
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    let weld = units.weld_tol_mm();
    build_solid(
        &file,
        units,
        &items,
        &BuildOptions { weld_tol_mm: weld },
        &Progress::new(),
    )
    .map(|b| (b.solid, weld))
    .map_err(|Skip(m)| m)
}

fn volume(s: &Solid) -> Result<f64, String> {
    let mesh = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: CHORD,
        },
    )
    .map_err(|e| format!("Netz: {e}"))?;
    Ok(mass_properties(&mesh).volume)
}

fn check_volume(what: &str, got: f64, want: f64, rel: f64) -> Result<(), String> {
    let d = (got - want).abs() / want.abs();
    if d <= rel {
        Ok(())
    } else {
        Err(format!(
            "{what}: Volumen {got:.6} statt {want:.6} (relativ {d:.2e})"
        ))
    }
}

fn valid(what: &str, s: &Solid) -> Result<(), String> {
    validate(s)
        .map(|_| ())
        .map_err(|e| format!("{what}: validate: {e}"))
}

/// Jede Marke „STEP-Fläche 1“ … „STEP-Fläche n“ kommt in `s` vor.
fn tags_kept(what: &str, s: &Solid, n: usize) -> Result<(), String> {
    for k in 1..=n {
        let role = format!("{STEP_FACE_ROLE} {k}");
        if !s
            .faces
            .iter()
            .any(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role))
        {
            return Err(format!("{what}: Marke „{role}“ fehlt"));
        }
    }
    Ok(())
}

/// Spielraum (Parameter: rad bzw. mm), mit dem die Randpunkte einer Fläche in ihrem
/// Parameterbereich liegen müssen (grobe Ecken liegen bis 9e-4 mm daneben).
const PATCH_SLACK: f64 = 2e-3;

/// Liegt `x` (bei Periode `per` um Vielfache verschoben) in `[a, b]`?
fn in_range(x: f64, a: f64, b: f64, per: Option<f64>) -> bool {
    let x = match per {
        Some(per) => x - per * ((x - (a - PATCH_SLACK)) / per).floor(),
        None => x,
    };
    x >= a - PATCH_SLACK && x <= b + PATCH_SLACK
}

/// Der Parameterbereich jeder gekrümmten Fläche enthält ihren Rand (sonst rechnet der Kernel
/// auf einem anderen Stück der Trägerfläche).
fn patches_fit(what: &str, s: &Solid) -> Result<(), String> {
    for (_, f) in s.faces.iter() {
        let Some(p) = f.patch else { continue };
        let role = f.tag.as_ref().map_or("?", |t| t.role.as_str());
        for lp in std::iter::once(&f.outer).chain(f.inner.iter()) {
            for he in &lp.half_edges {
                let e = &s.edges[he.edge];
                for k in 0..=8 {
                    let q = e.curve.point(e.t0 + (e.t1 - e.t0) * k as f64 / 8.0);
                    let (u, v) = f.surface.project(q);
                    if !(in_range(u, p.u0, p.u1, f.surface.u_period())
                        && in_range(v, p.v0, p.v1, f.surface.v_period()))
                    {
                        return Err(format!(
                            "{what}: Randpunkt ({u:.4}; {v:.4}) von „{role}“ außerhalb {p:?}"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Die Rundung ist gebaut: mehr Flächen als vorher, darunter eine Rundungsfläche mit Radius
/// [`FILLET`] (Quader: Zylinder, Ring: Torus mit diesem Rohrradius), und keine Kante läuft mehr
/// entlang der scharfen oberen Kante.
fn rounded(what: &str, kind: Kind, s: &Solid, faces_before: usize) -> Result<(), String> {
    if s.faces.len() <= faces_before {
        return Err(format!(
            "{what}: {} Flächen, vorher {faces_before}",
            s.faces.len()
        ));
    }
    let fillet_face = s.faces.iter().any(|(_, f)| match (kind, &f.surface) {
        (Kind::Box, Surface::Cylinder { radius, .. }) => (radius - FILLET).abs() < 1e-9,
        (Kind::Ring, Surface::Torus { minor, .. }) => (minor - FILLET).abs() < 1e-9,
        _ => false,
    });
    if !fillet_face {
        return Err(format!("{what}: keine Rundungsfläche mit Radius {FILLET}"));
    }
    let left = kind.top_edges(s).len();
    if left > 0 {
        return Err(format!("{what}: {left} scharfe obere Kanten übrig"));
    }
    Ok(())
}

/// Ergebnis der fünf Schritte für die Datei `name` mit dem Text `text`.
fn run(name: &str, text: &str, rel: f64) -> [Result<(), String>; 5] {
    let kind = kind_of(name);
    let (s, _) = match read(text) {
        Ok(s) => s,
        Err(m) => {
            let m = format!("Leser: {m}");
            return std::array::from_fn(|_| Err(m.clone()));
        }
    };
    let kernel = AnalyticKernel::default();
    let faces = s.faces.len();
    let v = volume(&s);

    let step1 = valid("eingelesen", &s)
        .and_then(|()| tags_kept("eingelesen", &s, faces))
        .and_then(|()| patches_fit("eingelesen", &s));

    let step2 = v
        .clone()
        .and_then(|v| check_volume("eingelesen", v, kind.volume(), rel));

    let step3 = (|| {
        let va = v.clone()?;
        let size = DVec3::new(BAR_LEN, 2.0 * BAR_HALF, 2.0 * BAR_HALF);
        let bar = kernel
            .make_box(size)
            .and_then(|b| {
                kernel.transform(&b, &DMat4::from_translation(kind.center() - 0.5 * size))
            })
            .map_err(|e| format!("Stab: {e}"))?;
        let vb = volume(&bar)?;
        let r = kernel
            .boolean(&s, &bar, BoolOp::Difference)
            .map_err(|e| format!("boolean: {e}"))?;
        valid("abgezogen", &r)?;
        let vr = volume(&r)?;
        if !boolean_volume_plausible(BoolOp::Difference, va, vb, vr, COARSE_REL) {
            return Err(format!(
                "boolean_volume_plausible: {va:.3} − {vb:.3} → {vr:.3}"
            ));
        }
        check_volume("abgezogen", vr, kind.volume() - kind.bar_removed(), rel)
    })();

    let step4 = (|| {
        let edges = kind.top_edges(&s);
        if edges.len() != kind.top_edge_count(name) {
            return Err(format!("{} obere Kanten gefunden", edges.len()));
        }
        let b = kernel
            .blend(
                &s,
                &BlendRequest {
                    edges: edges
                        .into_iter()
                        .map(|edge| BlendEdge {
                            edge,
                            first_face: None,
                        })
                        .collect(),
                    kind: BlendKind::Fillet { radius: FILLET },
                    tangent_chain: false,
                },
            )
            .map_err(|e| format!("blend: {e}"))?;
        valid("gerundet", &b.solid)?;
        tags_kept("gerundet", &b.solid, faces)?;
        // Das Volumen ändert sich durch die Rundung nur um 3,6e-4 (Quader) bzw. 7,5e-4 (Ring)
        // relativ – bei groben Körpern (1e-3) zu wenig als Beweis. Darum auch der Bau: eine neue
        // Rundungsfläche mit Radius 1, die scharfe Kante ist weg.
        rounded("gerundet", kind, &b.solid, faces)?;
        check_volume(
            "gerundet",
            volume(&b.solid)?,
            kind.volume() - kind.blend_removed(),
            rel,
        )
    })();

    let step5 = (|| {
        let plane = SectionPlane {
            kind: ViewKind::Front,
            offset: kind.center().y,
        };
        match cut_body(&s, 0, &plane, &Tolerance::default())
            .map_err(|e| format!("cut_body: {e}"))?
        {
            CutOutcome::Cut { solid, nudged } => {
                valid("geschnitten", &solid)?;
                check_volume("geschnitten", volume(&solid)?, kind.cut_volume(nudged), rel)
            }
            other => Err(format!("cut_body: {other:?} statt Cut")),
        }
    })();

    [step1, step2, step3, step4, step5]
}

/// Grobe Fassung: jede `VERTEX_POINT` bekommt einen eigenen, um `shift` mm in einer festen
/// Richtung je Ecke verschobenen Punkt (Geraden, Kreise und Flächen behalten ihre Punkte);
/// Genauigkeit der Datei `1.E-03`.
fn coarse(text: &str, shift: f64) -> String {
    let mut points: Vec<(String, DVec3)> = Vec::new();
    for line in text.lines() {
        if let Some((id, rest)) = line.split_once("=CARTESIAN_POINT('',(")
            && let Some(nums) = rest.strip_suffix("));")
        {
            let c: Vec<f64> = nums
                .split(',')
                .map(|x| x.trim().parse().expect("Zahl"))
                .collect();
            if c.len() == 3 {
                points.push((id.to_string(), DVec3::new(c[0], c[1], c[2])));
            }
        }
    }
    let mut out = Vec::new();
    let mut added = Vec::new();
    for line in text.lines() {
        if let Some((id, rest)) = line.split_once("=VERTEX_POINT('',")
            && let Some(p) = rest.strip_suffix(");")
        {
            let k = added.len();
            let q = points
                .iter()
                .find(|(i, _)| i == p)
                .map(|(_, q)| *q)
                .expect("Punkt der Ecke");
            let x = k as f64;
            let dir = DVec3::new(
                (1.7 * x + 0.3).sin(),
                (2.3 * x + 1.1).cos(),
                (0.7 * x + 2.9).sin(),
            )
            .normalize();
            let q = q + shift * dir;
            let nid = format!("#{}", 9000 + k);
            added.push(format!(
                "{nid}=CARTESIAN_POINT('',({:.9},{:.9},{:.9}));",
                q.x, q.y, q.z
            ));
            out.push(format!("{id}=VERTEX_POINT('',{nid});"));
        } else if line.contains("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(") {
            let (head, tail) = line.split_once("LENGTH_MEASURE(").expect("Genauigkeit");
            let (_, tail) = tail.split_once(')').expect("Klammer");
            out.push(format!("{head}LENGTH_MEASURE(1.E-03){tail}"));
        } else if line == "ENDSEC;" && out.iter().any(|l| l == "DATA;") {
            // Ende des Datenteils: die neuen Punkte davor.
            out.append(&mut added);
            out.push(line.to_string());
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

const FILES: [&str; 3] = ["box_ap214.step", "bore_halves.step", "bore_seam.step"];

#[test]
fn exact_bodies_pass_every_step() {
    let mut failures = Vec::new();
    for name in FILES {
        let res = run(name, &data(name), EXACT_REL);
        for (step, r) in STEPS.iter().zip(res) {
            match r {
                Ok(()) => eprintln!("{name} | {step} | Ok"),
                Err(m) => {
                    eprintln!("{name} | {step} | FEHLER: {m}");
                    failures.push(format!("{name}, Schritt {step}: {m}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn coarse_bodies_are_measured() {
    let mut mismatches = Vec::new();
    for (name, expected) in EXPECTED_COARSE {
        let exact = data(name);
        let text = coarse(&exact, COARSE_SHIFT);
        // Die grobe Fassung ist wirklich grob: jede Ecke 9e-4 mm neben der genauen, gebaut mit
        // der Genauigkeit 1e-3.
        let (e, _) = read(&exact).unwrap_or_else(|m| panic!("{name}: {m}"));
        let (c, weld) = read(&text).unwrap_or_else(|m| panic!("{name} (grob): {m}"));
        assert_eq!(weld, 1e-3, "{name}: Genauigkeit");
        // In der Datei (Rohform vor dem Bereinigen) liegt jede Ecke 9e-4 neben der genauen …
        let (raw_c, raw_e) = (raw_vertices(&text), raw_vertices(&exact));
        assert_eq!(raw_c.len(), raw_e.len(), "{name}: Ecken der Rohform");
        for (a, b) in raw_c.iter().zip(&raw_e) {
            let d = a.distance(*b);
            assert!((d - COARSE_SHIFT).abs() < 1e-8, "{name}: Versatz {d}");
        }
        // … und im Körper nach dem Nachziehen (P16) genau auf ihr (Kernel: 1e-6; hier 1e-9).
        assert_eq!(c.vertices.len(), e.vertices.len(), "{name}: Ecken");
        for (_, a) in c.vertices.iter() {
            let d = e
                .vertices
                .iter()
                .map(|(_, b)| a.point.distance(b.point))
                .fold(f64::INFINITY, f64::min);
            assert!(
                d <= 1e-9,
                "{name}: Ecke {:?} {d:e} neben der genauen",
                a.point
            );
        }
        let res = run(name, &text, COARSE_REL);
        for ((step, want), r) in STEPS.iter().zip(expected).zip(res) {
            let got = if r.is_ok() { Pass } else { Fails };
            eprintln!(
                "{name} (grob) | {step} | {got:?} | {}",
                r.as_ref().err().map_or("", |m| m.as_str())
            );
            if got != want {
                mismatches.push(format!(
                    "{name} (grob), {step}: {got:?} statt {want:?} ({r:?})"
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

/// Gegenprobe zu den Messfällen: Was weiter als die Genauigkeit der Datei (1e-3) neben seinem
/// Träger liegt, wird nicht übernommen – sonst wäre ein „Ok“ der Messfälle wertlos.
#[test]
fn beyond_the_file_tolerance_is_refused() {
    for name in FILES {
        // Ecken 2e-3 mm neben ihren Kanten.
        let m = read(&coarse(&data(name), 2e-3)).err().unwrap_or_default();
        eprintln!("{name}, Ecken 2e-3: {m}");
        // Beim Ring mit Naht fällt es schon am Parameterbereich auf (Mantel mehr als ein Umlauf).
        let want = if name == "bore_seam.step" {
            "reicht über mehr als einen Umlauf."
        } else {
            "liegt nicht auf ihrer Kante."
        };
        assert!(m.ends_with(want), "{name}: „{m}“");
    }
    for name in ["bore_halves.step", "bore_seam.step"] {
        // Oberer Außenkreis 2e-3 mm neben dem Mantel (Ecken mitgezogen, damit nur die Kante
        // falsch liegt).
        let text = coarse(&data(name), 0.0)
            .replace("CIRCLE('',#532,20.)", "CIRCLE('',#532,20.002)")
            .replace("(20.000000000,0.000000000,30.000000000)", "(20.002,0.,30.)")
            .replace(
                "(-20.000000000,0.000000000,30.000000000)",
                "(-20.002,0.,30.)",
            );
        let m = read(&text).err().unwrap_or_default();
        eprintln!("{name}, Kreis 2e-3: {m}");
        assert!(
            m.ends_with("liegt nicht auf ihren Flächen."),
            "{name}: „{m}“"
        );
    }
}
