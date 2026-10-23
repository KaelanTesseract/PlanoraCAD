//! STEP-Leser, Stufe 6: Zusammenziehen, Paaren und Nachziehen der Ecken (STEP-Import, Aufgabe 7).
//! `split_vertices.step` ist ein Quader 10 × 20 × 30, dessen Flächen je eigene Ecken und Kanten
//! haben (Kanteneintrag `#b+40+j` der Fläche `f` mit `b = 1000 + 100 f` läuft von Ecke `j` nach
//! `j + 1`; Nachbarn durchlaufen ihre Kopie andersherum). Grobe Fassungen erzeugt der Test aus
//! dieser Datei: Ecken je Fläche verschoben, Geraden durch die verschobenen Ecken neu gelegt.

use planora_drawing::model::ViewKind;
use planora_drawing::section::{CutOutcome, SECTION_NUDGE_MM, SectionPlane, cut_body};
use planora_exchange::Progress;
use planora_kernel::{
    AnalyticKernel, BlendEdge, BlendKind, BlendRequest, BoolOp, Kernel, Solid, Surface, validate,
};
use planora_math::{DMat4, DVec3, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_step::read::geom::Skip;
use planora_step::read::topo::{BuildOptions, BuiltSolid, build_solid, read_raw};
use planora_step::read::units::{Units, context_units};
use planora_step::read::weld::{
    HashGrid, check_open_edges, coarse_warning, weld_body, weld_points,
};
use planora_step::{ReadLimits, StepFile};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

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
/// Richtung), die Kopien einer Ecke höchstens 1e-3 auseinander.
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
    // Kopien einer Ecke höchstens 1e-3 auseinander.
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
    weld_body(&mut raw, 1e-3);
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
    // Ohne genannte Genauigkeit gilt 1e-3 und ebenfalls die Warnung.
    let none =
        data("split_vertices.step").replace("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#103))\n  ", "");
    assert!(!none.contains("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT"));
    let built = build(&none).expect("gelesen");
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
    weld_body(&mut raw, tol);
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
