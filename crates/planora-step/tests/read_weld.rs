//! STEP-Leser, Stufe 6: Zusammenziehen, Paaren und Nachziehen der Ecken (STEP-Import, Aufgabe 7).
//! `split_vertices.step` ist ein Quader 10 × 20 × 30, dessen Flächen je eigene Ecken und Kanten
//! haben (Kanteneintrag `#b+40+j` der Fläche `f` mit `b = 1000 + 100 f` läuft von Ecke `j` nach
//! `j + 1`; Nachbarn durchlaufen ihre Kopie andersherum). Grobe Fassungen erzeugt der Test aus
//! dieser Datei: Ecken je Fläche verschoben, Geraden durch die verschobenen Ecken neu gelegt.

mod common;

use common::export;
use planora_drawing::model::ViewKind;
use planora_drawing::section::{CutOutcome, SECTION_NUDGE_MM, SectionPlane, cut_body};
use planora_exchange::Progress;
use planora_kernel::{
    AnalyticKernel, BlendEdge, BlendKind, BlendRequest, BoolOp, Curve, FaceTag, Kernel, Solid,
    Surface, validate,
};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec3, Frame, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_step::read::geom::Skip;
use planora_step::read::topo::{
    BuildOptions, BuiltSolid, RawBody, RawEdge, RawFace, RawHalfEdge, RawShell, build_solid,
    read_raw,
};
use planora_step::read::units::{Units, context_units};
use planora_step::read::weld::{
    HashGrid, check_open_edges, coarse_warning, tighten_vertices, unknown_accuracy_warning,
    weld_body, weld_points,
};
use planora_step::{ReadLimits, StepFile};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

const UNKNOWN_WARNING: &str = "Genauigkeit der Datei unbekannt (angenommen 0,001 mm): Rechnen mit diesem Körper kann scheitern.";
const COARSE_WARNING: &str =
    "Genauigkeit der Datei 0,001 mm: Rechnen mit diesem Körper kann scheitern.";

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

/// Baut den Körper mit der Genauigkeit der Datei.
fn build(text: &str) -> Result<BuiltSolid, String> {
    let file = parse(text);
    let units = units_of(&file);
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    build_solid(
        &file,
        units,
        &items,
        &BuildOptions {
            weld_tol_mm: units.weld_tol_mm(),
        },
        &Progress::new(),
    )
    .map_err(|Skip(m)| m)
}

fn volume(s: &Solid) -> f64 {
    let mesh = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 0.001,
        },
    )
    .expect("Netz");
    mass_properties(&mesh).volume
}

/// Erste Zahl `#b` der Fläche `f` in `split_vertices.step`.
fn base(f: usize) -> usize {
    1000 + 100 * f
}

/// Koordinaten aus `(x,y,z)` einer Zeile `#n=CARTESIAN_POINT('',(x,y,z));`.
fn coords(line: &str) -> DVec3 {
    let inner = line
        .split_once("((")
        .or_else(|| line.split_once(",("))
        .expect("Klammer")
        .1;
    let inner = inner.split_once(')').expect("Klammer zu").0;
    let c: Vec<f64> = inner
        .split(',')
        .map(|x| x.trim().parse().expect("Zahl"))
        .collect();
    DVec3::new(c[0], c[1], c[2])
}

/// Ecken der Fläche `f` in Schleifenreihenfolge.
fn corners(text: &str, f: usize) -> [DVec3; 4] {
    std::array::from_fn(|k| {
        let head = format!("#{}=CARTESIAN_POINT(", base(f) + k);
        coords(
            text.lines()
                .find(|l| l.starts_with(&head))
                .expect("Eckpunkt"),
        )
    })
}

/// Nummer der Ecke `p` des Quaders (0 bis 7).
fn corner_index(p: DVec3) -> usize {
    usize::from(p.x > 5.0) + 2 * usize::from(p.y > 10.0) + 4 * usize::from(p.z > 15.0)
}

/// `split_vertices.step` mit verschobenen Ecken: `shift(f, k)` verschiebt Ecke `k` der Fläche `f`
/// (nur deren Kopie); jede Gerade wird durch ihre beiden (verschobenen) Ecken neu gelegt.
/// `uncertainty` ersetzt die Genauigkeit der Datei.
fn displaced(shift: &dyn Fn(usize, usize, DVec3) -> DVec3, uncertainty: &str) -> String {
    let text = data("split_vertices.step");
    let mut new: HashMap<String, String> = HashMap::new();
    for f in 0..6 {
        let old = corners(&text, f);
        let c: [DVec3; 4] = std::array::from_fn(|k| old[k] + shift(f, k, old[k]));
        let b = base(f);
        for (k, p) in c.iter().enumerate() {
            new.insert(
                format!("#{}", b + k),
                format!(
                    "#{}=CARTESIAN_POINT('',({:.12},{:.12},{:.12}));",
                    b + k,
                    p.x,
                    p.y,
                    p.z
                ),
            );
        }
        for j in 0..4 {
            let d = c[(j + 1) % 4] - c[j];
            let u = d.normalize();
            new.insert(
                format!("#{}", b + 10 + j),
                format!(
                    "#{}=DIRECTION('',({:.15},{:.15},{:.15}));",
                    b + 10 + j,
                    u.x,
                    u.y,
                    u.z
                ),
            );
            new.insert(
                format!("#{}", b + 20 + j),
                format!(
                    "#{}=VECTOR('',#{},{:.12});",
                    b + 20 + j,
                    b + 10 + j,
                    d.length()
                ),
            );
        }
    }
    let out: Vec<String> = text
        .lines()
        .map(|l| {
            let id = l.split_once('=').map(|(id, _)| id.to_string());
            match id.and_then(|id| new.get(&id)) {
                Some(n) => n.clone(),
                None => l.replace("LENGTH_MEASURE(1.E-07)", uncertainty),
            }
        })
        .collect();
    out.join("\n")
}

/// Richtung der Verschiebung von Ecke `c` des Quaders.
fn corner_dir(c: usize) -> DVec3 {
    let x = c as f64;
    DVec3::new(
        (1.7 * x + 0.3).sin(),
        (2.3 * x + 1.1).cos(),
        (0.7 * x + 2.9).sin(),
    )
    .normalize()
}

/// Grob: jede Kopie einer Ecke bis 9e-4 neben der genauen, je Fläche anders (Betrag und
/// Richtung), aber **nicht unabhängig**: Die Kopien einer Ecke streuen um eine gemeinsame
/// Richtung und liegen höchstens 1e-3 (die Genauigkeit der Datei) auseinander. Unabhängig bis
/// 9e-4 versetzte Kopien können bis 1,8e-3 auseinanderliegen; sie werden nicht zusammengezogen
/// (siehe `copies_farther_apart_than_the_accuracy_are_a_gap`).
fn coarse_shift(f: usize, _k: usize, p: DVec3) -> DVec3 {
    const SCALE: [f64; 6] = [1.0, 0.8, 0.6, 0.9, 0.7, 0.95];
    let wobble = DVec3::new(
        (1.3 * f as f64).sin(),
        (0.9 * f as f64 + 0.4).cos(),
        (2.1 * f as f64).sin(),
    );
    let dir = (corner_dir(corner_index(p)) + 0.2 * wobble).normalize();
    9e-4 * SCALE[f] * dir
}

fn coarse_box() -> String {
    displaced(&coarse_shift, "LENGTH_MEASURE(1.E-03)")
}

#[test]
fn grid_finds_neighbours_only() {
    // 1000 Punkte im Abstand 1, dazu einer 0,4e-3 neben Punkt 17.
    let mut pts: Vec<DVec3> = (0..1000)
        .map(|i| DVec3::new((i % 10) as f64, ((i / 10) % 10) as f64, (i / 100) as f64))
        .collect();
    pts.push(pts[17] + DVec3::new(0.0, 0.4e-3, 0.0));
    let rep = weld_points(&pts, 1e-3);
    assert_eq!(rep.len(), 1001);
    assert_eq!(rep[1000], 17);
    for (i, r) in rep.iter().enumerate().take(1000) {
        assert_eq!(*r, i, "Punkt {i}");
    }
    // An der Zellgrenze: 0,9e-3 über die Grenze bei x = 2e-3 (Zellgröße = Toleranz).
    let a = DVec3::new(1.9e-3, 0.5, 0.5);
    let b = DVec3::new(2.8e-3, 0.5, 0.5);
    assert_eq!(weld_points(&[a, b], 1e-3), vec![0, 0]);
    // Schräg über drei Grenzen.
    let c = a + DVec3::new(0.5e-3, 0.5e-3, 0.5e-3);
    assert_eq!(weld_points(&[a, c], 1e-3), vec![0, 0]);
    // Knapp darüber nicht.
    let d = a + DVec3::new(1.001e-3, 0.0, 0.0);
    assert_eq!(weld_points(&[a, d], 1e-3), vec![0, 1]);
    // Kette a–b–c je 0,8e-3: nicht transitiv.
    let chain = [
        DVec3::ZERO,
        DVec3::new(0.8e-3, 0.0, 0.0),
        DVec3::new(1.6e-3, 0.0, 0.0),
    ];
    assert_eq!(weld_points(&chain, 1e-3), vec![0, 0, 2]);
    // Raster: nur Punkte innerhalb des Abstands, auch über Zellgrenzen.
    let mut grid = HashGrid::new(1e-3);
    grid.insert(a, 7);
    grid.insert(DVec3::new(5.0, 5.0, 5.0), 8);
    assert_eq!(grid.near(b, 1e-3), vec![7]);
    assert!(grid.near(d + DVec3::new(0.5e-3, 0.0, 0.0), 1e-3).is_empty());
}

#[test]
fn split_vertices_box_is_welded() {
    let built = build(&data("split_vertices.step")).expect("gelesen");
    let s = &built.solid;
    assert_eq!(s.faces.len(), 6);
    assert_eq!(s.vertices.len(), 8);
    assert_eq!(s.edges.len(), 12);
    validate(s).expect("gültig");
    let v = volume(s);
    assert!((v - 6000.0).abs() <= 1e-9, "Volumen {v}");
    // Feine Datei: keine Warnung.
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
}

#[test]
fn coarse_box_is_taken_with_warning() {
    let text = coarse_box();
    // Der Testkörper ist so grob wie gewollt: jede Kopie bis 9e-4 neben der genauen Ecke, die
    // Kopien einer Ecke aber höchstens 1e-3 auseinander (nicht unabhängig versetzt).
    let exact = data("split_vertices.step");
    let mut copies: Vec<Vec<DVec3>> = vec![Vec::new(); 8];
    let mut largest: f64 = 0.0;
    for f in 0..6 {
        for (p, q) in corners(&exact, f).into_iter().zip(corners(&text, f)) {
            largest = largest.max(p.distance(q));
            assert!(p.distance(q) <= 9e-4 + 1e-12);
            copies[corner_index(p)].push(q);
        }
    }
    assert!((largest - 9e-4).abs() < 1e-9, "größter Versatz {largest}");
    for c in &copies {
        assert_eq!(c.len(), 3);
        for a in c {
            for b in c {
                assert!(a.distance(*b) <= 1e-3, "Kopien {}", a.distance(*b));
            }
        }
    }
    // Zusammenziehen: je Ecke bleibt der Punkt der ersten Kopie (Boden, Deckel, dann Vorderseite
    // …), nichts wird gemittelt.
    let file = parse(&text);
    let units = units_of(&file);
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    let mut raw = read_raw(&file, units, &items, 1e-3, &Progress::new()).expect("Rohform");
    weld_body(&mut raw, 1e-3).expect("zusammengezogen");
    assert_eq!(raw.vertices.len(), 8);
    for v in &raw.vertices {
        let first = (0..6)
            .flat_map(|f| corners(&text, f))
            .find(|q| corner_index(*q) == corner_index(*v))
            .expect("Kopie");
        assert_eq!(*v, first, "Vertreter der Ecke {}", corner_index(*v));
    }
    let built = build(&text).expect("gelesen");
    assert_eq!(built.warnings, vec![COARSE_WARNING.to_string()]);
    assert_eq!(built.solid.vertices.len(), 8);
    assert_eq!(built.solid.edges.len(), 12);
    validate(&built.solid).expect("gültig");
    // Ohne genannte Genauigkeit gilt 1e-3 und die Warnung „unbekannt“ (Schlussprüfung M1).
    let none =
        data("split_vertices.step").replace("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#103))\n  ", "");
    assert!(!none.contains("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT"));
    let built = build(&none).expect("gelesen");
    assert_eq!(built.warnings, vec![UNKNOWN_WARNING.to_string()]);
    assert_eq!(unknown_accuracy_warning(1e-3), UNKNOWN_WARNING);
    // Gröbere Genauigkeit als 1e-3 wird begrenzt; die Warnung nennt den begrenzten Wert.
    let rough =
        data("split_vertices.step").replace("LENGTH_MEASURE(1.E-07)", "LENGTH_MEASURE(1.E-02)");
    let built = build(&rough).expect("gelesen");
    assert_eq!(built.warnings, vec![COARSE_WARNING.to_string()]);
    // Text der Warnung.
    assert_eq!(coarse_warning(1e-3), COARSE_WARNING);
    assert_eq!(
        coarse_warning(2.5e-5),
        "Genauigkeit der Datei 0,000025 mm: Rechnen mit diesem Körper kann scheitern."
    );
}

#[test]
fn gap_above_tolerance_is_named() {
    // Ecke 0 des Bodens (0; 0; 0) um 2e-3 in −x: Ihre Kanten liegen 2e-3 neben denen der
    // Nachbarn (an der Ecke selbst; der größte Abstand zählt).
    let text = displaced(
        &|f, k, _| {
            if (f, k) == (0, 0) {
                DVec3::new(-2e-3, 0.0, 0.0)
            } else {
                DVec3::ZERO
            }
        },
        "LENGTH_MEASURE(1.E-03)",
    );
    assert_eq!(
        build(&text).err().as_deref(),
        Some("Lücke von 0,002 mm an der Kante bei (0,00; 10,00; 0,00).")
    );
    // Dieselbe Ecke 2e-3 in −y: Die Kante des Bodens wird länger, ihre Mitte liegt nur 1e-3
    // neben der des Nachbarn; die Lücke ist trotzdem 2e-3.
    let text = displaced(
        &|f, k, _| {
            if (f, k) == (0, 0) {
                DVec3::new(0.0, -2e-3, 0.0)
            } else {
                DVec3::ZERO
            }
        },
        "LENGTH_MEASURE(1.E-03)",
    );
    assert_eq!(
        build(&text).err().as_deref(),
        Some("Lücke von 0,002 mm an der Kante bei (0,00; 10,00; 0,00).")
    );
    // 1,23456e-3: höchstens vier gültige Stellen.
    let text = displaced(
        &|f, k, _| {
            if (f, k) == (0, 0) {
                DVec3::new(-1.23456e-3, 0.0, 0.0)
            } else {
                DVec3::ZERO
            }
        },
        "LENGTH_MEASURE(1.E-03)",
    );
    assert_eq!(
        build(&text).err().as_deref(),
        Some("Lücke von 0,001235 mm an der Kante bei (0,00; 10,00; 0,00).")
    );
    // Dieselbe Lücke unter der Genauigkeit (9e-4) wird zusammengezogen.
    let text = displaced(
        &|f, k, _| {
            if (f, k) == (0, 0) {
                DVec3::new(-9e-4, 0.0, 0.0)
            } else {
                DVec3::ZERO
            }
        },
        "LENGTH_MEASURE(1.E-03)",
    );
    let built = build(&text).expect("gelesen");
    assert_eq!(built.solid.vertices.len(), 8);
    // Genauigkeit über 1e-3 wird auf 1e-3 begrenzt: 2e-3 bleibt eine Lücke.
    let text = displaced(
        &|f, k, _| {
            if (f, k) == (0, 0) {
                DVec3::new(-2e-3, 0.0, 0.0)
            } else {
                DVec3::ZERO
            }
        },
        "LENGTH_MEASURE(1.E-02)",
    );
    assert_eq!(
        build(&text).err().as_deref(),
        Some("Lücke von 0,002 mm an der Kante bei (0,00; 10,00; 0,00).")
    );
}

#[test]
fn missing_face_is_named() {
    // Quader ohne Deckel (#1170).
    let text = data("split_vertices.step").replace("#1070,#1170,", "#1070,");
    let m = build(&text).err().unwrap_or_default();
    assert!(m.ends_with("gehört nur zu einer Fläche."), "„{m}“");
    // Die erste offene Kante ist die obere Kante der Vorderseite.
    assert_eq!(
        m,
        "Der Körper ist ungültig: Die Kante bei (5,00; 0,00; 30,00) gehört nur zu einer Fläche."
    );
}

#[test]
fn different_arcs_between_the_same_corners_stay_apart() {
    // Zylinder aus zwei Halbschalen (Ecken bei ±20 je oben und unten): die beiden Bögen eines
    // Deckels haben dieselben Ecken, sind aber verschiedene Kanten.
    for name in ["bore_halves.step", "bore_seam.step"] {
        let built = build(&data(name)).expect("gelesen");
        let v = volume(&built.solid);
        let want = PI * (20.0f64 * 20.0 - 5.0 * 5.0) * 30.0;
        assert!((v - want).abs() / want < 1e-4, "{name}: Volumen {v}");
    }
    let built = build(&data("bore_halves.step")).expect("gelesen");
    assert_eq!(built.solid.faces.len(), 6);
    assert_eq!(built.solid.edges.len(), 12);
}

/// Text eines Körpers aus ebenen Flächen (je Fläche ein Vieleck gegen den Uhrzeigersinn um die
/// Außennormale), jede Fläche mit eigenen Ecken und Kanten.
fn polyhedron_text(faces: &[Vec<DVec3>]) -> String {
    pieces_text(&[faces.to_vec()])
}

/// Wie [`polyhedron_text`], je Stück ein `MANIFOLD_SOLID_BREP` in derselben Darstellung.
fn pieces_text(pieces: &[Vec<Vec<DVec3>>]) -> String {
    let mut out = String::new();
    let mut id = 1000usize;
    let mut next = || {
        id += 1;
        id
    };
    let mut solids = Vec::new();
    let pt = |p: DVec3| format!("({:.6},{:.6},{:.6})", p.x, p.y, p.z);
    for faces in pieces {
        let mut face_ids = Vec::new();
        for poly in faces {
            let n = poly.len();
            let points: Vec<usize> = poly
                .iter()
                .map(|p| {
                    let i = next();
                    out.push_str(&format!("#{i}=CARTESIAN_POINT('',{});\n", pt(*p)));
                    i
                })
                .collect();
            let verts: Vec<usize> = points
                .iter()
                .map(|p| {
                    let i = next();
                    out.push_str(&format!("#{i}=VERTEX_POINT('',#{p});\n"));
                    i
                })
                .collect();
            let mut oes = Vec::new();
            for j in 0..n {
                let (a, b) = (poly[j], poly[(j + 1) % n]);
                let d = b - a;
                let u = d.normalize();
                let (dir, vec, line, ec, oe) = (next(), next(), next(), next(), next());
                out.push_str(&format!(
                    "#{dir}=DIRECTION('',{});\n#{vec}=VECTOR('',#{dir},{:.6});\n\
                 #{line}=LINE('',#{},#{vec});\n#{ec}=EDGE_CURVE('',#{},#{},#{line},.T.);\n\
                 #{oe}=ORIENTED_EDGE('',*,*,#{ec},.T.);\n",
                    pt(u),
                    d.length(),
                    points[j],
                    verts[j],
                    verts[(j + 1) % n],
                ));
                oes.push(format!("#{oe}"));
            }
            // Newell-Normale, Bezugsrichtung entlang der ersten Kante.
            let mut normal = DVec3::ZERO;
            for j in 0..n {
                let (a, b) = (poly[j], poly[(j + 1) % n]);
                normal += DVec3::new(
                    (a.y - b.y) * (a.z + b.z),
                    (a.z - b.z) * (a.x + b.x),
                    (a.x - b.x) * (a.y + b.y),
                );
            }
            let normal = normal.normalize();
            let x = (poly[1] - poly[0]).normalize();
            let (lp, fb, pl, ax, nd, xd, face) =
                (next(), next(), next(), next(), next(), next(), next());
            out.push_str(&format!(
                "#{lp}=EDGE_LOOP('',({}));\n#{fb}=FACE_OUTER_BOUND('',#{lp},.T.);\n\
             #{pl}=PLANE('',#{ax});\n#{ax}=AXIS2_PLACEMENT_3D('',#{},#{nd},#{xd});\n\
             #{nd}=DIRECTION('',{});\n#{xd}=DIRECTION('',{});\n\
             #{face}=ADVANCED_FACE('',(#{fb}),#{pl},.T.);\n",
                oes.join(","),
                points[0],
                pt(normal),
                pt(x),
            ));
            face_ids.push(format!("#{face}"));
        }
        let (solid, shell) = (next(), next());
        out.push_str(&format!(
            "#{solid}=MANIFOLD_SOLID_BREP('Quader',#{shell});\n#{shell}=CLOSED_SHELL('',({}));\n",
            face_ids.join(",")
        ));
        solids.push(format!("#{solid}"));
    }
    let head = data("split_vertices.step");
    let head: String = head
        .split("/* Je Fläche")
        .next()
        .expect("Kopf")
        .lines()
        .filter(|l| !l.starts_with("#20=") && !l.starts_with("#21="))
        .map(|l| format!("{l}\n"))
        .collect();
    let head = head.replace("(#11,#20)", &format!("(#11,{})", solids.join(",")));
    format!("{head}{out}ENDSEC;\nEND-ISO-10303-21;\n")
}

/// Flächen eines achsparallelen Quaders von `lo` bis `hi` (Schleifen um die Außennormale).
fn box_faces(lo: DVec3, hi: DVec3) -> Vec<Vec<DVec3>> {
    let c = |x: bool, y: bool, z: bool| {
        DVec3::new(
            if x { hi.x } else { lo.x },
            if y { hi.y } else { lo.y },
            if z { hi.z } else { lo.z },
        )
    };
    let (f, t) = (false, true);
    vec![
        vec![c(f, f, f), c(f, t, f), c(t, t, f), c(t, f, f)],
        vec![c(f, f, t), c(t, f, t), c(t, t, t), c(f, t, t)],
        vec![c(f, f, f), c(t, f, f), c(t, f, t), c(f, f, t)],
        vec![c(t, f, f), c(t, t, f), c(t, t, t), c(t, f, t)],
        vec![c(t, t, f), c(f, t, f), c(f, t, t), c(t, t, t)],
        vec![c(f, f, f), c(f, f, t), c(f, t, t), c(f, t, f)],
    ]
}

#[test]
fn touching_pieces_are_welded_each_on_its_own() {
    // Zwei Quader, die sich an der Fläche x = 10 berühren: Ecken und Kanten fallen dort
    // zusammen, gehören aber zu verschiedenen Stücken und bleiben getrennt.
    let a = box_faces(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0));
    let b = box_faces(DVec3::new(10.0, 0.0, 0.0), DVec3::new(20.0, 20.0, 30.0));
    let built = build(&pieces_text(&[a, b])).expect("gelesen");
    let s = &built.solid;
    assert_eq!(s.shells.len(), 2);
    assert_eq!(s.vertices.len(), 16);
    assert_eq!(s.edges.len(), 24);
    validate(s).expect("gültig");
    let v = volume(s);
    assert!((v - 12000.0).abs() <= 1e-9, "Volumen {v}");
}

/// Treppe aus `steps` Stufen (Grundriss in x/y, Höhe 1): `2 · steps + 2` Ecken im Grundriss.
fn staircase(steps: usize) -> Vec<Vec<DVec3>> {
    let n = steps as f64;
    let mut profile = vec![(0.0, 0.0), (n, 0.0)];
    for k in (1..=steps).rev() {
        let k = k as f64;
        profile.push((k, k));
        profile.push((k - 1.0, k));
    }
    let at = |(x, y): (f64, f64), z: f64| DVec3::new(x, y, z);
    let mut faces = Vec::new();
    faces.push(profile.iter().rev().map(|p| at(*p, 0.0)).collect());
    faces.push(profile.iter().map(|p| at(*p, 1.0)).collect());
    let m = profile.len();
    for i in 0..m {
        let (a, b) = (profile[i], profile[(i + 1) % m]);
        faces.push(vec![at(a, 0.0), at(b, 0.0), at(b, 1.0), at(a, 1.0)]);
    }
    faces
}

#[test]
fn welding_scales_linearly() {
    // Probe der Hilfe am kleinen Fall: eine Stufe ist ein gültiger Körper.
    let small = build(&polyhedron_text(&staircase(3))).expect("kleine Treppe");
    assert_eq!(small.solid.vertices.len(), 2 * (2 * 3 + 2));
    let v = volume(&small.solid);
    assert!((v - 6.0).abs() < 1e-9, "Volumen {v}");

    let steps = 2500;
    let text = polyhedron_text(&staircase(steps));
    let file = parse(&text);
    let units = units_of(&file);
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    let tol = units.weld_tol_mm();
    let mut raw = read_raw(&file, units, &items, tol, &Progress::new()).expect("Rohform");
    let corners = 2 * (2 * steps + 2);
    assert!(raw.vertices.len() > 2 * corners);
    let t = Instant::now();
    weld_body(&mut raw, tol).expect("zusammengezogen");
    check_open_edges(&raw, tol).expect("jede Kante zweimal benutzt");
    let took = t.elapsed();
    eprintln!("Zusammenziehen und Paaren: {took:?}");
    assert_eq!(raw.vertices.len(), corners);
    assert_eq!(raw.edges.len(), 3 * (2 * steps + 2));
    assert!(took.as_secs_f64() < 2.0, "{took:?}");
}

/// Schritte 1–5 des Machbarkeitstors (Aufgabe 4) für einen Quader 10 × 20 × 30 mit der Ecke im
/// Ursprung; Volumen relativ 1e-4.
fn gate_steps_for_the_box(s: &Solid) {
    let kernel = AnalyticKernel::default();
    let rel = |got: f64, want: f64| (got - want).abs() / want;
    // 1, 2
    validate(s).expect("1: gültig");
    assert!(rel(volume(s), 6000.0) < 1e-9, "2: Volumen {}", volume(s));
    // 3: Stab 100 × 4 × 4 entlang x durch die Mitte.
    let size = DVec3::new(100.0, 4.0, 4.0);
    let bar = kernel
        .make_box(size)
        .and_then(|b| {
            kernel.transform(
                &b,
                &DMat4::from_translation(DVec3::new(5.0, 10.0, 15.0) - 0.5 * size),
            )
        })
        .expect("Stab");
    let r = kernel
        .boolean(s, &bar, BoolOp::Difference)
        .expect("3: Abziehen");
    validate(&r).expect("3: gültig");
    assert!(rel(volume(&r), 6000.0 - 160.0) < 1e-4, "3: {}", volume(&r));
    // 4: Rundung Radius 1 an der Kante oben vorn (y = 0, z = 30).
    let top_front = |s: &Solid| {
        s.edges
            .iter()
            .filter(|(_, e)| {
                [0.0, 0.5, 1.0].iter().all(|k| {
                    let p = e.curve.point(e.t0 + (e.t1 - e.t0) * k);
                    p.y.abs() < 0.01 && (p.z - 30.0).abs() < 0.01
                })
            })
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
    };
    let edges = top_front(s);
    assert_eq!(edges.len(), 1);
    let b = kernel
        .blend(
            s,
            &BlendRequest {
                edges: edges
                    .into_iter()
                    .map(|edge| BlendEdge {
                        edge,
                        first_face: None,
                    })
                    .collect(),
                kind: BlendKind::Fillet { radius: 1.0 },
                tangent_chain: false,
            },
        )
        .expect("4: Rundung");
    validate(&b.solid).expect("4: gültig");
    // Die Rundung ist da: eine Zylinderfläche mit Radius 1, die scharfe Kante ist weg.
    assert_eq!(b.solid.faces.len(), 7);
    assert!(b.solid.faces.iter().any(|(_, f)| matches!(
        f.surface,
        Surface::Cylinder { radius, .. } if (radius - 1.0).abs() < 1e-9
    )));
    assert!(top_front(&b.solid).is_empty());
    let spandrel = 1.0 - PI / 4.0;
    assert!(
        rel(volume(&b.solid), 6000.0 - 10.0 * spandrel) < 1e-5,
        "4: {}",
        volume(&b.solid)
    );
    // 5: Schnitt in der Mitte, Ansicht von vorn.
    let plane = SectionPlane {
        kind: ViewKind::Front,
        offset: 10.0,
    };
    match cut_body(s, 0, &plane, &Tolerance::default()).expect("5: Schnitt") {
        CutOutcome::Cut { solid, nudged } => {
            validate(&solid).expect("5: gültig");
            let nudge = if nudged { SECTION_NUDGE_MM } else { 0.0 };
            assert!(rel(volume(&solid), 3000.0 - nudge * 300.0) < 1e-6);
        }
        other => panic!("5: {other:?} statt Cut"),
    }
}

#[test]
fn coarse_box_after_tightening_passes_the_gate_steps() {
    let built = build(&coarse_box()).expect("gelesen");
    // Jede Ecke liegt nach dem Nachziehen genau (Kernel-Genauigkeit und besser) auf ihrer Ecke
    // des Quaders.
    let exact: Vec<DVec3> = (0..8)
        .map(|c| {
            DVec3::new(
                10.0 * (c & 1) as f64,
                20.0 * ((c >> 1) & 1) as f64,
                30.0 * ((c >> 2) & 1) as f64,
            )
        })
        .collect();
    for (_, v) in built.solid.vertices.iter() {
        let d = exact[corner_index(v.point)].distance(v.point);
        assert!(d <= 1e-9, "Ecke {:?} {d:e} neben der genauen", v.point);
    }
    // Jede Kante liegt mit Anfang, Mitte und Ende genau auf beiden Flächen.
    for (_, e) in built.solid.edges.iter() {
        for k in [0.0, 0.5, 1.0] {
            let p = e.curve.point(e.t0 + (e.t1 - e.t0) * k);
            let planes = [p.x, p.x - 10.0, p.y, p.y - 20.0, p.z, p.z - 30.0]
                .iter()
                .filter(|d| d.abs() < 1e-9)
                .count();
            assert!(
                planes >= 2,
                "Kantenpunkt {p:?} liegt nicht genau auf zwei Ebenen"
            );
        }
    }
    gate_steps_for_the_box(&built.solid);
    // Gegenprobe: ohne Nachziehen (Datei genau so grob, aber die Ecken dort, wo die Datei sie
    // hat) wäre derselbe Körper für den Kernel zu ungenau – geprüft im Tor (`gate.rs`).
}

/// Zahl aus „Lücke von {d} mm …“.
fn gap_number(m: &str) -> f64 {
    let rest = m
        .strip_prefix("Lücke von ")
        .unwrap_or_else(|| panic!("keine Lücke: „{m}“"));
    rest.split_once(" mm")
        .expect("Einheit")
        .0
        .replace(',', ".")
        .parse()
        .expect("Zahl")
}

#[test]
fn copies_farther_apart_than_the_accuracy_are_a_gap() {
    // Festlegung (Ledger, Aufgabe 7): Kopien einer Ecke müssen innerhalb der Genauigkeit der
    // Datei beieinanderliegen. Hier liegen Boden- und Vorderseitenkopie der Ecke (0; 0; 0) je
    // 9e-4 neben der genauen, aber in entgegengesetzter Richtung (1,8e-3 auseinander); die
    // linke Kopie (genau) liegt nahe bei beiden. Zusammengezogen wird nicht transitiv: Die
    // Vorderseitenkopie bleibt getrennt, der Körper wird mit einer Lücke über der Genauigkeit
    // ausgelassen – nichts wird still geschlossen.
    let dir = DVec3::new(1.0, -1.0, 1.0).normalize();
    let text = displaced(
        &|f, k, _| match (f, k) {
            (0, 0) => 9e-4 * dir,
            (2, 0) => -9e-4 * dir,
            _ => DVec3::ZERO,
        },
        "LENGTH_MEASURE(1.E-03)",
    );
    let m = build(&text).err().unwrap_or_default();
    let d = gap_number(&m);
    assert!(d >= 1e-3, "„{m}“");
}

/// Rohform mit Ecken, Kanten (Kurve, t0, t1, Anfang, Ende) und Flächen (Fläche, Halbkanten), alle
/// in einer Schale.
fn raw_body(
    vertices: Vec<DVec3>,
    edges: Vec<(Curve, f64, f64, usize, usize)>,
    faces: Vec<(Surface, Vec<(usize, bool)>)>,
) -> RawBody {
    let n = faces.len();
    RawBody {
        vertices,
        edges: edges
            .into_iter()
            .map(|(curve, t0, t1, start, end)| RawEdge {
                curve,
                t0,
                t1,
                start,
                end,
                line: 0,
            })
            .collect(),
        faces: faces
            .into_iter()
            .enumerate()
            .map(|(i, (surface, hes))| RawFace {
                surface,
                same_sense: true,
                loops: vec![
                    hes.into_iter()
                        .map(|(edge, forward)| RawHalfEdge { edge, forward })
                        .collect(),
                ],
                marked_outer: Some(0),
                patch: None,
                tag: FaceTag {
                    origin: 0,
                    role: format!("STEP-Fläche {}", i + 1),
                },
                line: 0,
                pole_marks: Vec::new(),
                side_guessed: false,
            })
            .collect(),
        shells: vec![RawShell {
            faces: (0..n).collect(),
            void: false,
            owner: None,
        }],
    }
}

fn plane(origin: DVec3, x: DVec3, y: DVec3) -> Surface {
    Surface::Plane {
        frame: Frame {
            origin,
            x,
            y,
            z: x.cross(y),
        },
    }
}

/// Kubische Bézierkurve von `p` nach `q` mit den inneren Punkten `a`, `b`.
fn bezier(p: DVec3, a: DVec3, b: DVec3, q: DVec3) -> Curve {
    Curve::nurbs(
        NurbsCurve::new(
            3,
            vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            vec![p, a, b, q],
            vec![1.0; 4],
        )
        .expect("Bézier"),
    )
}

#[test]
fn gap_counts_the_unwelded_corners() {
    // Zwei Kopien derselben Kurve, eine Ecke der zweiten 2e-3 daneben (nicht zusammengezogen):
    // Die Kurvenpunkte der Kopien fallen zusammen (Parameter = Projektion auf dieselbe Kurve),
    // die Lücke ist trotzdem 2e-3 – nie eine Zahl unter der Genauigkeit oder 0.
    let circle = |origin: DVec3, x: DVec3, y: DVec3, radius: f64| Curve::Circle {
        frame: Frame {
            origin,
            x,
            y,
            z: x.cross(y),
        },
        radius,
    };
    let cases = [
        // Rand eines Zylinders R 20 oben, Ecke radial nach außen.
        (
            "Zylinder",
            circle(DVec3::new(0.0, 0.0, 30.0), DVec3::X, DVec3::Y, 20.0),
            DVec3::X,
        ),
        // Meridian einer Kugel R 5, Ecke radial.
        (
            "Kugel",
            circle(DVec3::ZERO, DVec3::X, DVec3::Z, 5.0),
            DVec3::X,
        ),
        // Rohrkreis eines Torus (Mitte bei x = 10, Rohr 3), Ecke quer zum Kreis.
        (
            "Torus",
            circle(DVec3::new(10.0, 0.0, 0.0), DVec3::X, DVec3::Z, 3.0),
            DVec3::Y,
        ),
    ];
    for (name, curve, off) in cases {
        for reversed in [false, true] {
            let a = curve.point(0.0);
            let b = curve.point(PI);
            let a2 = a + 2e-3 * off;
            let second = if reversed {
                (curve.clone(), PI, 0.0, 1, 2)
            } else {
                (curve.clone(), 0.0, PI, 2, 1)
            };
            let raw = raw_body(
                vec![a, b, a2],
                vec![(curve.clone(), 0.0, PI, 0, 1), second],
                vec![
                    (plane(DVec3::ZERO, DVec3::X, DVec3::Y), vec![(0, true)]),
                    (plane(DVec3::ZERO, DVec3::X, DVec3::Y), vec![(1, true)]),
                ],
            );
            let Skip(m) = check_open_edges(&raw, 1e-3).expect_err("offen");
            let d = gap_number(&m);
            assert!(
                (d - 2e-3).abs() < 1e-9,
                "{name}, rückwärts {reversed}: „{m}“"
            );
        }
    }
}

#[test]
fn pairing_needs_quarter_and_middle_points() {
    // Zwischen denselben Ecken: Gerade A, S-Kurve B (Mitte genau auf A, Viertel 2,8e-3 daneben),
    // Bogen C (überall 3,75e-3 … 5e-3 neben A, also unter 10 · tol), Kopie D von A (rückwärts).
    // Nur D wird mit A gepaart.
    let (p, q) = (DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0));
    let at = |x: f64, y: f64| DVec3::new(x, y, 0.0);
    let line = Curve::Line {
        origin: p,
        dir: DVec3::X,
    };
    let h = 0.01;
    let s_curve = bezier(p, at(10.0 / 3.0, h), at(20.0 / 3.0, -h), q);
    let bow = bezier(p, at(10.0 / 3.0, 6.67e-3), at(20.0 / 3.0, 6.67e-3), q);
    let pl = plane(DVec3::ZERO, DVec3::X, DVec3::Y);
    let mut raw = raw_body(
        vec![p, q],
        vec![
            (line.clone(), 0.0, 10.0, 0, 1),
            (s_curve, 0.0, 1.0, 0, 1),
            (bow, 0.0, 1.0, 0, 1),
            (line, 10.0, 0.0, 1, 0),
        ],
        vec![
            (pl.clone(), vec![(0, true)]),
            (pl.clone(), vec![(1, true)]),
            (pl.clone(), vec![(2, true)]),
            (pl, vec![(3, false)]),
        ],
    );
    weld_body(&mut raw, 1e-3).expect("zusammengezogen");
    assert_eq!(raw.edges.len(), 3);
    assert_eq!(raw.faces[3].loops[0][0].edge, 0);
    assert!(
        raw.faces[3].loops[0][0].forward,
        "Richtung der Kopie gekippt"
    );
    for f in 0..3 {
        assert_eq!(raw.faces[f].loops[0][0].edge, f);
    }
}

#[test]
fn gap_is_measured_along_the_whole_edge_to_the_nearest_partner() {
    // Offene Gerade A; daneben ein Bogen B zwischen denselben Ecken (Mitte 1,5e-3 neben A) und
    // eine Gerade E 0,05 neben A (weiter weg, aber im Suchabstand). Die Lücke kommt vom nächsten
    // Partner B und aus der Mitte (an den Enden ist sie 0): 0,0015.
    let at = |x: f64, y: f64| DVec3::new(x, y, 0.0);
    let (p, q) = (at(0.0, 0.0), at(10.0, 0.0));
    let h = 2e-3;
    let pl = plane(DVec3::ZERO, DVec3::X, DVec3::Y);
    let raw = raw_body(
        vec![p, q, at(0.0, 0.05), at(10.0, 0.05)],
        vec![
            (
                Curve::Line {
                    origin: p,
                    dir: DVec3::X,
                },
                0.0,
                10.0,
                0,
                1,
            ),
            (
                bezier(p, at(10.0 / 3.0, h), at(20.0 / 3.0, h), q),
                0.0,
                1.0,
                0,
                1,
            ),
            (
                Curve::Line {
                    origin: at(0.0, 0.05),
                    dir: DVec3::X,
                },
                0.0,
                10.0,
                2,
                3,
            ),
        ],
        vec![
            (pl.clone(), vec![(0, true)]),
            (pl.clone(), vec![(1, false)]),
            (pl, vec![(2, false)]),
        ],
    );
    assert_eq!(
        check_open_edges(&raw, 1e-3)
            .err()
            .map(|Skip(m)| m)
            .as_deref(),
        Some("Lücke von 0,0015 mm an der Kante bei (5,00; 0,00; 0,00).")
    );
}

#[test]
fn edge_used_three_times_is_left_to_validate() {
    // Dreimal benutzt ist nicht offen: Das meldet `validate` („… gehört zu 3 Flächen …“).
    let pl = plane(DVec3::ZERO, DVec3::X, DVec3::Y);
    let raw = raw_body(
        vec![DVec3::ZERO, DVec3::X],
        vec![(
            Curve::Line {
                origin: DVec3::ZERO,
                dir: DVec3::X,
            },
            0.0,
            1.0,
            0,
            1,
        )],
        vec![
            (pl.clone(), vec![(0, true)]),
            (pl.clone(), vec![(0, false)]),
            (pl, vec![(0, true)]),
        ],
    );
    assert!(check_open_edges(&raw, 1e-3).is_ok());
}

/// Ecke `corner` an drei Ebenen x = 0, y = 0, z = 0 mit Kanten entlang der Achsen; `nurbs`:
/// die Kante entlang z als Freiformkurve (Gerade 1. Grades).
fn corner_body(corner: DVec3, nurbs: bool) -> RawBody {
    let axis = |d: DVec3| Curve::Line {
        origin: DVec3::ZERO,
        dir: d,
    };
    let z_edge = if nurbs {
        (
            Curve::nurbs(
                NurbsCurve::new(
                    1,
                    vec![0.0, 0.0, 10.0, 10.0],
                    vec![DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0)],
                    vec![1.0; 2],
                )
                .expect("Gerade"),
            ),
            corner.z,
            10.0,
            0,
            3,
        )
    } else {
        (axis(DVec3::Z), corner.z, 10.0, 0, 3)
    };
    raw_body(
        vec![
            corner,
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(0.0, 10.0, 0.0),
            DVec3::new(0.0, 0.0, 10.0),
        ],
        vec![
            (axis(DVec3::X), corner.x, 10.0, 0, 1),
            (axis(DVec3::Y), corner.y, 10.0, 0, 2),
            z_edge,
        ],
        vec![
            (
                plane(DVec3::ZERO, DVec3::X, DVec3::Y),
                vec![(0, true), (1, false)],
            ),
            (
                plane(DVec3::ZERO, DVec3::Y, DVec3::Z),
                vec![(1, true), (2, false)],
            ),
            (
                plane(DVec3::ZERO, DVec3::Z, DVec3::X),
                vec![(2, true), (0, false)],
            ),
        ],
    )
}

#[test]
fn tightening_stays_within_the_accuracy_and_away_from_freeform_edges() {
    let dir = DVec3::ONE.normalize();
    // 9,9e-4 neben der Ecke: genau auf die Ecke gezogen.
    let mut raw = corner_body(9.9e-4 * dir, false);
    tighten_vertices(&mut raw, 1e-3);
    assert!(raw.vertices[0].length() < 1e-12, "{:?}", raw.vertices[0]);
    // 1,2e-3 neben der Ecke (über der Genauigkeit): bleibt, wo die Datei sie hat.
    let far = 1.2e-3 * dir;
    let mut raw = corner_body(far, false);
    tighten_vertices(&mut raw, 1e-3);
    assert_eq!(raw.vertices[0], far);
    // Eine Freiformkante an der Ecke: nicht gezogen (sonst wanderte die Ecke vom Ende der Kurve
    // weg).
    let near = 5e-4 * dir;
    let mut raw = corner_body(near, true);
    tighten_vertices(&mut raw, 1e-3);
    assert_eq!(raw.vertices[0], near);
}

/// Ecken und Kantenparameter als Bits.
fn bits(raw: &RawBody) -> Vec<u64> {
    raw.vertices
        .iter()
        .flat_map(|p| p.to_array())
        .chain(raw.edges.iter().flat_map(|e| [e.t0, e.t1]))
        .map(f64::to_bits)
        .collect()
}

#[test]
fn exact_corners_are_left_bit_for_bit() {
    // Genaue Dateien: Das Nachziehen verändert kein Bit an Ecken und Kantenparametern.
    let kernel = AnalyticKernel::default();
    let mut texts: Vec<(String, String)> = [
        "box_ap214.step",
        "bore_halves.step",
        "bore_seam.step",
        "split_vertices.step",
    ]
    .iter()
    .map(|n| (n.to_string(), data(n)))
    .collect();
    let bar = kernel
        .make_cylinder(3.0, 40.0)
        .and_then(|c| {
            kernel.transform(
                &c,
                &(DMat4::from_rotation_y(0.7)
                    * DMat4::from_translation(DVec3::new(2.0, 1.0, -20.0))),
            )
        })
        .expect("Stab");
    let block = kernel
        .make_box(DVec3::new(20.0, 20.0, 10.0))
        .and_then(|b| {
            kernel.transform(&b, &DMat4::from_translation(DVec3::new(-10.0, -10.0, -5.0)))
        })
        .expect("Block");
    let bodies: Vec<(&str, Solid)> = vec![
        (
            "Zylinder",
            kernel.make_cylinder(5.0, 12.0).expect("Zylinder"),
        ),
        ("Kegel", kernel.make_cone(4.0, 9.0).expect("Kegel")),
        ("Kugel", kernel.make_sphere(6.0).expect("Kugel")),
        (
            "Querbohrung",
            kernel
                .boolean(&block, &bar, BoolOp::Difference)
                .expect("Querbohrung"),
        ),
    ];
    for (name, body) in &bodies {
        texts.push((name.to_string(), export(&[(name, body)]).0));
    }
    for (name, text) in texts {
        let file = parse(&text);
        let units = units_of(&file);
        let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
        let tol = units.weld_tol_mm();
        let mut raw = read_raw(&file, units, &items, tol, &Progress::new()).expect("Rohform");
        weld_body(&mut raw, tol).expect("zusammengezogen");
        let before = bits(&raw);
        tighten_vertices(&mut raw, tol);
        assert!(before == bits(&raw), "{name}: Bits verändert");
    }
}

#[test]
fn far_away_bodies_are_refused_quickly() {
    // Koordinaten um 1e14 mm: Das Raster könnte dort keine Zellen mehr unterscheiden (alles in
    // einer Zelle, quadratisch). Solche Körper werden sofort abgelehnt.
    let far = DVec3::new(1e14, 0.0, 0.0);
    let faces: Vec<Vec<DVec3>> = staircase(2500)
        .into_iter()
        .map(|f| f.into_iter().map(|p| p + far).collect())
        .collect();
    let text = polyhedron_text(&faces);
    let file = parse(&text);
    let units = units_of(&file);
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    let tol = units.weld_tol_mm();
    let mut raw = read_raw(&file, units, &items, tol, &Progress::new()).expect("Rohform");
    let t = Instant::now();
    let r = weld_body(&mut raw, tol);
    assert!(t.elapsed().as_secs_f64() < 1.0, "{:?}", t.elapsed());
    assert_eq!(
        r.err().map(|Skip(m)| m).as_deref(),
        Some("Der Körper liegt weiter als 1 000 km vom Ursprung entfernt.")
    );
    let small: Vec<Vec<DVec3>> = staircase(3)
        .into_iter()
        .map(|f| f.into_iter().map(|p| p + far).collect())
        .collect();
    assert_eq!(
        build(&polyhedron_text(&small)).err().as_deref(),
        Some("Der Körper liegt weiter als 1 000 km vom Ursprung entfernt.")
    );
}
