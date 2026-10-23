//! STEP-Import, Aufgabe 9: Pipeline (`import_bodies`) und Normalisieren (`normalize`,
//! `read_normal_body`, `check_normal_header`). Eigene Testdateien unter `tests/data`; weitere
//! Dateien erzeugt der Test (Prismen über einem Vieleck: Quader und „Treppe“ mit wählbar vielen
//! Flächen).

use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, Kernel, Solid};
use planora_math::{DMat4, DVec3};
use planora_step::read::geom::Resolver;
use planora_step::read::units::Units;
use planora_step::{
    ImportOptions, ImportedBody, LengthUnit, MAX_STEP_TEXT_BYTES, NORMAL_FILE_NAME, ReadLimits,
    StepFile, StepImport, StepReadError, check_normal_header, import_bodies, normalize,
    read_normal_body,
};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TORUS: &str = "Eine Torusfläche mit kleinem Radius größer oder gleich dem großen wird noch nicht unterstützt.";

fn data(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn parse_with(text: &str, limits: &ReadLimits) -> StepFile {
    StepFile::parse(text.as_bytes().to_vec(), limits, &Progress::new()).expect("Datei lädt")
}

fn parse(text: &str) -> StepFile {
    parse_with(text, &ReadLimits::DEFAULT)
}

fn import_with(text: &str, opts: &ImportOptions) -> StepImport {
    import_bodies(&parse(text), opts, &Progress::new()).expect("Import")
}

fn import(text: &str) -> StepImport {
    import_with(text, &ImportOptions::default())
}

fn ok(b: &ImportedBody) -> &planora_step::NormalBody {
    b.outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("„{}“ ausgelassen: {e}", b.name))
}

fn err(b: &ImportedBody) -> &str {
    match &b.outcome {
        Ok(_) => panic!("„{}“ wurde übernommen", b.name),
        Err(e) => e,
    }
}

fn roles(s: &Solid) -> Vec<String> {
    s.faces
        .iter()
        .map(|(_, f)| f.tag.as_ref().map_or(String::new(), |t| t.role.clone()))
        .collect()
}

/// Hülle der Ecken.
fn corners(s: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in s.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    (lo, hi)
}

fn near(a: DVec3, b: DVec3, tol: f64, what: &str) {
    assert!((a - b).abs().max_element() <= tol, "{what}: {a} statt {b}");
}

/// Zwei Körper sind gleich: Flächenzahl, Marken, Ecken bitgleich, Kantenparameter bitgleich und
/// alles Übrige (die `Debug`-Ausgabe schreibt jede Zahl so, dass sie bitgleich zurückkommt).
fn same_solid(a: &Solid, b: &Solid, what: &str) {
    assert_eq!(a.faces.len(), b.faces.len(), "{what}: Flächenzahl");
    assert_eq!(roles(a), roles(b), "{what}: Marken");
    let pa: Vec<[u64; 3]> = a
        .vertices
        .iter()
        .map(|(_, v)| v.point.to_array().map(f64::to_bits))
        .collect();
    let pb: Vec<[u64; 3]> = b
        .vertices
        .iter()
        .map(|(_, v)| v.point.to_array().map(f64::to_bits))
        .collect();
    assert_eq!(pa, pb, "{what}: Ecken");
    let ea: Vec<(u64, u64)> = a
        .edges
        .iter()
        .map(|(_, e)| (e.t0.to_bits(), e.t1.to_bits()))
        .collect();
    let eb: Vec<(u64, u64)> = b
        .edges
        .iter()
        .map(|(_, e)| (e.t0.to_bits(), e.t1.to_bits()))
        .collect();
    assert_eq!(ea, eb, "{what}: Kantenparameter");
    assert_eq!(format!("{a:?}"), format!("{b:?}"), "{what}: Körper");
}

/// Prüft „Sitzung = Neuladen“ für jeden übernommenen Körper und gibt deren Zahl zurück.
fn check_reload(imp: &StepImport, file: &str) -> usize {
    let mut n = 0;
    for b in &imp.bodies {
        let Ok(nb) = &b.outcome else { continue };
        let what = format!("{file} „{}“", b.name);
        let reloaded = read_normal_body(&nb.text).unwrap_or_else(|e| panic!("{what}: {e}"));
        same_solid(&reloaded, &nb.solid, &what);
        let k = nb.solid.faces.len();
        let expected: Vec<String> = (1..=k).map(|i| format!("STEP-Fläche {i}")).collect();
        assert_eq!(roles(&nb.solid), expected, "{what}: Marken nach P7");
        assert!(
            nb.solid
                .faces
                .iter()
                .all(|(_, f)| f.tag.as_ref().is_some_and(|t| t.origin == 0)),
            "{what}: Herkunft 0"
        );
        assert_eq!(b.faces, k, "{what}: Flächenzahl");
        let again = normalize(&nb.solid, &b.name).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_eq!(again.text, nb.text, "{what}: Normalisieren ist ruhend");
        check_normal_header(&nb.text).unwrap_or_else(|e| panic!("{what}: {e}"));
        n += 1;
    }
    n
}

// ---------------------------------------------------------------------------------------------
// Erzeugte Dateien: Prismen über Vielecken als freie Volumen-Einträge.

/// Schreibt Einträge ab Nummer 1.
struct Gen {
    out: String,
    next: u64,
}

impl Gen {
    fn new() -> Gen {
        Gen {
            out: String::new(),
            next: 1,
        }
    }

    fn add(&mut self, body: &str) -> u64 {
        let id = self.next;
        self.next += 1;
        let _ = writeln!(self.out, "#{id}={body};");
        id
    }

    fn point(&mut self, p: DVec3) -> u64 {
        self.add(&format!(
            "CARTESIAN_POINT('',({:?},{:?},{:?}))",
            p.x, p.y, p.z
        ))
    }

    fn dir(&mut self, d: DVec3) -> u64 {
        self.add(&format!("DIRECTION('',({:?},{:?},{:?}))", d.x, d.y, d.z))
    }

    fn placement(&mut self, at: DVec3, z: DVec3, x: DVec3) -> u64 {
        let (p, z, x) = (self.point(at), self.dir(z), self.dir(x));
        self.add(&format!("AXIS2_PLACEMENT_3D('',#{p},#{z},#{x})"))
    }

    /// Kontext in mm (`inch`: Zoll) mit Unsicherheit 1e-7 in dieser Einheit.
    fn context(&mut self, inch: bool) -> u64 {
        let mm = self.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))");
        let length = if inch {
            let m = self.add(&format!(
                "LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#{mm})"
            ));
            let d = self.add("DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.)");
            self.add(&format!(
                "(CONVERSION_BASED_UNIT('INCH',#{m})LENGTH_UNIT()NAMED_UNIT(#{d}))"
            ))
        } else {
            mm
        };
        let rad = self.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))");
        let sr = self.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())");
        let u = self.add(&format!(
            "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),#{length},'distance_accuracy_value','')"
        ));
        self.add(&format!(
            "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{u}))\
             GLOBAL_UNIT_ASSIGNED_CONTEXT((#{length},#{rad},#{sr}))REPRESENTATION_CONTEXT('',''))"
        ))
    }

    fn line_edge(&mut self, a: (u64, DVec3), b: (u64, DVec3)) -> u64 {
        let d = b.1 - a.1;
        let p = self.point(a.1);
        let dir = self.dir(d / d.length());
        let v = self.add(&format!("VECTOR('',#{dir},{:?})", d.length()));
        let l = self.add(&format!("LINE('',#{p},#{v})"));
        self.add(&format!("EDGE_CURVE('',#{},#{},#{l},.T.)", a.0, b.0))
    }

    fn face(&mut self, edges: &[(u64, bool)], plane: u64, same: bool) -> u64 {
        let oe: Vec<String> = edges
            .iter()
            .map(|&(e, f)| {
                let id = self.add(&format!(
                    "ORIENTED_EDGE('',*,*,#{e},{})",
                    if f { ".T." } else { ".F." }
                ));
                format!("#{id}")
            })
            .collect();
        let lp = self.add(&format!("EDGE_LOOP('',({}))", oe.join(",")));
        let b = self.add(&format!("FACE_OUTER_BOUND('',#{lp},.T.)"));
        let pl = self.add(&format!("PLANE('',#{plane})"));
        self.add(&format!(
            "ADVANCED_FACE('',(#{b}),#{pl},{})",
            if same { ".T." } else { ".F." }
        ))
    }

    /// Freier `MANIFOLD_SOLID_BREP` `name`: Prisma über dem Vieleck `pts` (gegen den
    /// Uhrzeigersinn in z = 0) mit Höhe `h`; `2 + pts.len()` Flächen.
    fn prism(&mut self, name: &str, pts: &[(f64, f64)], h: f64) -> u64 {
        let n = pts.len();
        let lo: Vec<DVec3> = pts.iter().map(|&(x, y)| DVec3::new(x, y, 0.0)).collect();
        let hi: Vec<DVec3> = lo.iter().map(|p| *p + DVec3::Z * h).collect();
        let mut vlo = Vec::new();
        let mut vhi = Vec::new();
        for i in 0..n {
            let p = self.point(lo[i]);
            vlo.push((self.add(&format!("VERTEX_POINT('',#{p})")), lo[i]));
            let p = self.point(hi[i]);
            vhi.push((self.add(&format!("VERTEX_POINT('',#{p})")), hi[i]));
        }
        let bottom: Vec<u64> = (0..n)
            .map(|i| self.line_edge(vlo[i], vlo[(i + 1) % n]))
            .collect();
        let top: Vec<u64> = (0..n)
            .map(|i| self.line_edge(vhi[i], vhi[(i + 1) % n]))
            .collect();
        let up: Vec<u64> = (0..n).map(|i| self.line_edge(vlo[i], vhi[i])).collect();
        let mut faces = Vec::new();
        let base = self.placement(DVec3::ZERO, DVec3::Z, DVec3::X);
        let rev: Vec<(u64, bool)> = bottom.iter().rev().map(|&e| (e, false)).collect();
        faces.push(self.face(&rev, base, false));
        let lid = self.placement(DVec3::Z * h, DVec3::Z, DVec3::X);
        let fwd: Vec<(u64, bool)> = top.iter().map(|&e| (e, true)).collect();
        faces.push(self.face(&fwd, lid, true));
        for i in 0..n {
            let j = (i + 1) % n;
            let d = (lo[j] - lo[i]).normalize();
            let normal = DVec3::new(d.y, -d.x, 0.0);
            let pl = self.placement(lo[i], normal, d);
            faces.push(self.face(
                &[
                    (bottom[i], true),
                    (up[j], true),
                    (top[i], false),
                    (up[i], false),
                ],
                pl,
                true,
            ));
        }
        let list: Vec<String> = faces.iter().map(|f| format!("#{f}")).collect();
        let shell = self.add(&format!("CLOSED_SHELL('',({}))", list.join(",")));
        self.add(&format!("MANIFOLD_SOLID_BREP('{name}',#{shell})"))
    }

    fn file(&self) -> String {
        format!(
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
             FILE_NAME('probe.step','2026-10-23T00:00:00',(''),(''),'','','');\n\
             FILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 1 1 1 1 }}'));\nENDSEC;\nDATA;\n{}\
             ENDSEC;\nEND-ISO-10303-21;\n",
            self.out
        )
    }
}

const BOX: [(f64, f64); 4] = [(0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (0.0, 20.0)];

/// Treppe mit `k` Stufen und einer gefasten Ecke: `2k + 3` Ecken, also `2k + 5` Flächen.
fn stairs(k: usize) -> Vec<(f64, f64)> {
    let k = k as f64;
    let mut pts = vec![(0.0, 0.0), (k - 0.5, 0.0), (k, 0.5), (k, k)];
    let mut i = 1.0;
    while i < k {
        pts.push((k - i, k - i + 1.0));
        pts.push((k - i, k - i));
        i += 1.0;
    }
    pts.push((0.0, 1.0));
    pts
}

/// Quader und Treppe mit `k` Stufen als freie Volumen-Einträge in einer mm-Datei.
fn box_and_stairs(k: usize) -> String {
    let mut g = Gen::new();
    g.context(false);
    g.prism("Quader", &BOX, 30.0);
    g.prism("Treppe", &stairs(k), 1.0);
    g.file()
}

/// Ersetzt jede Nummer `#n` durch `#(n + off)`.
fn shift_ids(text: &str, off: u64) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '#' {
            let mut digits = String::new();
            while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                digits.push(*d);
                chars.next();
            }
            let n: u64 = digits.parse().expect("Nummer");
            let _ = write!(out, "{}", n + off);
        }
    }
    out
}

/// Datenteil von `box_ap214.step` (ohne Kommentare).
fn box_data() -> String {
    let text = data("box_ap214.step");
    let start = text.find("DATA;").expect("DATA") + "DATA;".len();
    let end = text.rfind("ENDSEC;").expect("ENDSEC");
    text[start..end]
        .lines()
        .filter(|l| !l.trim_start().starts_with("/*"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn wrap(data: &str) -> String {
    format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
         FILE_NAME('drei.step','2026-10-23T00:00:00',(''),(''),'','','');\n\
         FILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 1 1 1 1 }}'));\nENDSEC;\nDATA;\n{data}\n\
         ENDSEC;\nEND-ISO-10303-21;\n"
    )
}

/// Drei Quader in einer Datei: Quader, Quader ohne Deckel (`#31` fehlt in der Schale), Quader
/// mit einer Torusfläche r ≥ R statt des Bodens.
fn three_bodies() -> String {
    let one = box_data();
    let lidless = shift_ids(&one, 1000).replace("(#1030,#1031,#1032,", "(#1030,#1032,");
    assert!(lidless.contains("(#1030,#1032,"));
    let torus = shift_ids(&one, 2000).replace(
        "#2060=PLANE('',#2310);",
        "#2060=TOROIDAL_SURFACE('',#2310,5.,10.);",
    );
    assert!(torus.contains("TOROIDAL_SURFACE"));
    wrap(&format!("{one}\n{lidless}\n{torus}"))
}

// ---------------------------------------------------------------------------------------------

#[test]
fn session_equals_reload() {
    let mut total = 0;
    for file in [
        "box_ap214.step",
        "bore_halves.step",
        "inch_assembly.step",
        "bore_seam.step",
        "rational_bspline.step",
        "revolution_extrusion.step",
        "seamless.step",
        "sphere_vertex_loop.step",
        "split_vertices.step",
        "void_box.step",
        "mapped_item.step",
    ] {
        let imp = import(&data(file));
        total += check_reload(&imp, file);
    }
    assert!(total >= 14, "nur {total} Körper übernommen");
    // Eigene Körper des Kernels, auch Kegel und Kugel.
    let k = AnalyticKernel::default();
    let bodies = [
        ("Quader", k.make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap()),
        ("Zylinder", k.make_cylinder(5.0, 10.0).unwrap()),
        ("Kegel", k.make_cone(5.0, 10.0).unwrap()),
        ("Kugel", k.make_sphere(7.0).unwrap()),
    ];
    // Dazu je schräg gedreht und verschoben (Richtungen mit „krummen“ letzten Stellen).
    let pose = DMat4::from_translation(DVec3::new(3.25, -7.5, 1.125))
        * DMat4::from_axis_angle(DVec3::new(1.0, 2.0, 3.0).normalize(), 0.7);
    for (name, solid) in &bodies {
        let turned = k.transform(solid, &pose).expect("Lage");
        for (what, s) in [
            (name.to_string(), solid),
            (format!("{name} gedreht"), &turned),
        ] {
            let n = normalize(s, &what).unwrap_or_else(|e| panic!("{what}: {e}"));
            let reloaded = read_normal_body(&n.text).unwrap_or_else(|e| panic!("{what}: {e}"));
            same_solid(&reloaded, &n.solid, &what);
            let again = normalize(&n.solid, &what).expect("zweites Mal");
            assert_eq!(again.text, n.text, "{what}: Normalisieren ist ruhend");
            assert_eq!(n.solid.faces.len(), s.faces.len(), "{what}: Flächen");
        }
    }
}

#[test]
fn unit_directions_are_kept_bit_for_bit() {
    // Schon normierte Richtungen und senkrechte Bezugsrichtungen bleiben bitgleich (sonst käme
    // unser eigener Text nach Schreiben und Lesen nicht gleich zurück).
    let (a, b) = (-0.447_213_595_499_958_04_f64, 0.894_427_190_999_916_1_f64);
    let text = wrap(&format!(
        "{}\n#9001=DIRECTION('',({a:?},0.,{b:?}));\n#9002=DIRECTION('',({b:?},0.,{:?}));\n\
         #9003=AXIS2_PLACEMENT_3D('',#151,#9001,#9002);\n#9004=DIRECTION('',(3.,0.,4.));",
        box_data(),
        -a
    ));
    let file = parse(&text);
    let mut res = Resolver::new(&file, Units::MM);
    let d = res.direction(9001).expect("Richtung");
    assert_eq!(
        d.to_array().map(f64::to_bits),
        [a, 0.0, b].map(f64::to_bits)
    );
    let f = res.placement(9003).expect("Lage");
    assert_eq!(
        f.z.to_array().map(f64::to_bits),
        [a, 0.0, b].map(f64::to_bits)
    );
    assert_eq!(
        f.x.to_array().map(f64::to_bits),
        [b, 0.0, -a].map(f64::to_bits)
    );
    // Nicht normierte werden normiert.
    let d = res.direction(9004).expect("Richtung");
    assert!((d - DVec3::new(0.6, 0.0, 0.8)).length() < 1e-15);
}

#[test]
fn normal_text_is_fixed() {
    let text = data("inch_assembly.step");
    let a = import(&text);
    let b = import(&text);
    assert_eq!(format!("{a:?}"), format!("{b:?}"), "zwei Läufe gleich");
    for body in &a.bodies {
        let n = ok(body);
        assert!(n.text.contains("'1970-01-01T00:00:00'"), "Zeitstempel");
        assert!(
            n.text.contains(&format!("'{NORMAL_FILE_NAME}'")),
            "Dateiname"
        );
        assert_eq!(NORMAL_FILE_NAME, "planora-step-koerper.step");
        assert!(n.text.is_ascii());
    }
    // Die Lage steckt im Text (vor dem Normalisieren angewandt): Block (2) liegt bei x ≥ 50,8.
    let moved = a
        .bodies
        .iter()
        .find(|b| b.name == "Block (2)")
        .expect("Block (2)");
    let reloaded = read_normal_body(&ok(moved).text).expect("liest");
    let (lo, hi) = corners(&reloaded);
    near(lo, DVec3::new(50.8, 0.0, 0.0), 1e-9, "Block (2) unten");
    near(hi, DVec3::new(101.6, 25.4, 76.2), 1e-9, "Block (2) oben");
    // Gleicher Körper, anderer Name → anderer Text, gleicher Körper.
    let other = normalize(&ok(moved).solid, "Anders").expect("Text");
    assert_ne!(other.text, ok(moved).text);
}

#[test]
fn bad_bodies_are_left_out_others_kept() {
    let imp = import(&three_bodies());
    assert_eq!(imp.bodies.len(), 3);
    let names: Vec<&str> = imp.bodies.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Würfel", "Würfel (2)", "Würfel (3)"]);
    let first = ok(&imp.bodies[0]);
    assert_eq!(first.solid.faces.len(), 6);
    let lidless = err(&imp.bodies[1]);
    assert!(
        lidless.starts_with("Der Körper ist ungültig: Die Kante bei (")
            && lidless.ends_with("gehört nur zu einer Fläche."),
        "{lidless}"
    );
    assert_eq!(err(&imp.bodies[2]), TORUS);
    assert_eq!(imp.bodies[1].faces, 5);
    assert_eq!(imp.bodies[2].faces, 6);
    assert!(imp.bodies[1].bounds.is_none());
    // Reihenfolge der Datei, auch wenn der fehlerhafte Körper zuerst kommt.
    let one = box_data();
    let torus = shift_ids(&one, 2000).replace(
        "#2060=PLANE('',#2310);",
        "#2060=TOROIDAL_SURFACE('',#2310,5.,10.);",
    );
    let imp = import(&wrap(&format!("{torus}\n{one}")));
    assert_eq!(err(&imp.bodies[0]), TORUS);
    assert_eq!(ok(&imp.bodies[1]).solid.faces.len(), 6);
    // Nur fehlerhafte Körper: der Aufruf gelingt trotzdem.
    let imp = import(&wrap(&torus));
    assert_eq!(imp.bodies.len(), 1);
    assert_eq!(err(&imp.bodies[0]), TORUS);
}

#[test]
fn units_override_scales_everything() {
    let text = data("inch_assembly.step");
    let plain = import(&text);
    assert_eq!(plain.unit, LengthUnit::Zoll);
    assert!(plain.unit_known);
    let block = &plain.bodies[0];
    assert_eq!(block.name, "Block");
    let (lo, hi) = block.bounds.expect("Hülle");
    near(lo, DVec3::ZERO, 1e-12, "Block unten");
    near(hi, DVec3::new(25.4, 50.8, 76.2), 1e-12, "Block oben");
    assert!(
        (block.tolerance_mm - 2.54e-6).abs() < 1e-18,
        "{}",
        block.tolerance_mm
    );
    assert!(block.warnings.is_empty(), "{:?}", block.warnings);

    let mm = import_with(
        &text,
        &ImportOptions {
            unit_override: Some(LengthUnit::Millimeter),
        },
    );
    assert_eq!(mm.unit, LengthUnit::Millimeter);
    assert!(mm.unit_known);
    assert_eq!(mm.bodies.len(), 3);
    let block = &mm.bodies[0];
    let (lo, hi) = block.bounds.expect("Hülle");
    near(lo, DVec3::ZERO, 1e-12, "Block unten (mm)");
    near(hi, DVec3::new(1.0, 2.0, 3.0), 1e-12, "Block oben (mm)");
    // 1e-7 in → 1e-7 mm, begrenzt auf 1e-6 mm.
    assert_eq!(block.tolerance_mm, 1e-6);
    let second = mm
        .bodies
        .iter()
        .find(|b| b.name == "Block (2)")
        .expect("Block (2)");
    let (lo, hi) = second.bounds.expect("Hülle");
    near(lo, DVec3::new(2.0, 0.0, 0.0), 1e-12, "Block (2) unten (mm)");
    near(hi, DVec3::new(4.0, 1.0, 3.0), 1e-12, "Block (2) oben (mm)");
    let (lo, hi) = corners(&ok(second).solid);
    near(lo, DVec3::new(2.0, 0.0, 0.0), 1e-12, "Block (2) Ecken");
    near(hi, DVec3::new(4.0, 1.0, 3.0), 1e-12, "Block (2) Ecken");
    // Der Stift (Zylinder r 0,25, Länge 2): die Hülle folgt aus dem Netz, nicht nur aus den Ecken.
    let pin = mm.bodies.iter().find(|b| b.name == "Stift").expect("Stift");
    let (lo, hi) = pin.bounds.expect("Hülle");
    let size = hi - lo;
    let mut s = size.to_array();
    s.sort_by(f64::total_cmp);
    assert!(
        (s[0] - 0.5).abs() < 0.01 && (s[1] - 0.5).abs() < 0.01,
        "{size}"
    );
    assert!((s[2] - 2.0).abs() < 1e-9, "{size}");
}

#[test]
fn cancel_and_progress() {
    let text = data("inch_assembly.step");
    let file = parse(&text);
    let p = Progress::new();
    p.cancel();
    assert_eq!(
        import_bodies(&file, &ImportOptions::default(), &p).unwrap_err(),
        StepReadError::Cancelled
    );
    let p = Progress::new();
    import_bodies(&file, &ImportOptions::default(), &p).expect("Import");
    assert_eq!(p.fraction(), 1.0);

    // Treppe mit 20 001 Flächen neben einem Quader.
    assert_eq!(stairs(9998).len() + 2, 20_001);
    let big = box_and_stairs(9998);
    let limits = ReadLimits {
        max_faces: 20_000,
        ..ReadLimits::DEFAULT
    };
    let file = parse_with(&big, &limits);
    let p = Progress::new();
    let imp = import_bodies(&file, &ImportOptions::default(), &p).expect("Import");
    assert_eq!(imp.bodies.len(), 2);
    assert_eq!(ok(&imp.bodies[0]).solid.faces.len(), 6);
    assert_eq!(
        err(&imp.bodies[1]),
        "Der Körper hat mehr als 20 000 Flächen."
    );
    assert_eq!(imp.bodies[1].faces, 20_001);
    assert_eq!(p.fraction(), 1.0);
    // Gezählt wurden nur die 6 Flächen des Quaders.
    p.set_total(1_000_000);
    assert!((p.fraction() * 1e6 - 6.0).abs() < 0.01, "{}", p.fraction());
}

#[test]
fn more_than_fifty_bodies_refuse_the_file() {
    let file = |n: usize| {
        let mut g = Gen::new();
        g.context(false);
        for i in 0..n {
            g.prism(&format!("Q{i}"), &BOX, 30.0);
        }
        g.file()
    };
    let e = import_bodies(
        &parse(&file(51)),
        &ImportOptions::default(),
        &Progress::new(),
    )
    .unwrap_err();
    assert_eq!(e, StepReadError::TooManyBodies);
    assert_eq!(e.to_string(), "Die Datei enthält mehr als 50 Körper.");
    let p = Progress::new();
    let imp = import_bodies(&parse(&file(50)), &ImportOptions::default(), &p).expect("50 gehen");
    assert_eq!(imp.bodies.len(), 50);
    assert!(imp.bodies.iter().all(|b| b.outcome.is_ok()));
    assert_eq!(p.fraction(), 1.0);
}

#[test]
fn small_stairs_are_a_valid_body_and_limit_counts_faces() {
    let text = box_and_stairs(3);
    let imp = import(&text);
    assert_eq!(imp.bodies.len(), 2);
    assert_eq!(imp.bodies[1].name, "Treppe");
    assert_eq!(ok(&imp.bodies[1]).solid.faces.len(), 11);
    assert_eq!(imp.unit, LengthUnit::Millimeter);
    assert!(imp.unit_known);
    check_reload(&imp, "Treppe");
    // Grenze 8 Flächen: der Text nennt die Grenze des Programms, nicht die der Probe.
    let limits = ReadLimits {
        max_faces: 8,
        ..ReadLimits::DEFAULT
    };
    let p = Progress::new();
    let imp =
        import_bodies(&parse_with(&text, &limits), &ImportOptions::default(), &p).expect("Import");
    assert_eq!(ok(&imp.bodies[0]).solid.faces.len(), 6);
    assert_eq!(
        err(&imp.bodies[1]),
        "Der Körper hat mehr als 20 000 Flächen."
    );
    assert_eq!(imp.bodies[1].faces, 11);
    assert_eq!(p.fraction(), 1.0);
    // Ein Körper, der mitten in den Flächen scheitert, zählt trotzdem ganz.
    let broken = text.replacen("PLANE(", "TOROIDAL_SURFACE_X(", 1);
    let p = Progress::new();
    let imp = import_bodies(&parse(&broken), &ImportOptions::default(), &p).expect("Import");
    assert!(err(&imp.bodies[0]).contains("TOROIDAL_SURFACE_X"));
    assert_eq!(p.fraction(), 1.0);
    p.set_total(1_000_000);
    assert!((p.fraction() * 1e6 - 17.0).abs() < 0.01, "{}", p.fraction());
}

#[test]
fn cancel_during_the_build_gives_no_result() {
    let text = box_and_stairs(1500);
    let file = parse(&text);
    let p = Progress::new();
    let start = Instant::now();
    let res = std::thread::scope(|s| {
        s.spawn(|| {
            let until = Instant::now() + Duration::from_secs(120);
            while p.fraction() == 0.0 && Instant::now() < until {
                std::thread::yield_now();
            }
            p.cancel();
        });
        import_bodies(&file, &ImportOptions::default(), &p)
    });
    assert_eq!(res.unwrap_err(), StepReadError::Cancelled);
    assert!(
        start.elapsed() < Duration::from_secs(60),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn header_check_is_cheap_and_strict() {
    let end = "END-ISO-10303-21;\n";
    assert!(check_normal_header("ISO-10303-21;\nHEADER;\nENDSEC;\n").is_err());
    assert!(check_normal_header(&format!("ISO-10303-21;\n/* ä */\n{end}")).is_err());
    assert!(check_normal_header(&format!("HEADER;\n{end}")).is_err());
    assert!(check_normal_header("").is_err());
    // Billig: der Inhalt wird nicht gelesen.
    check_normal_header(&format!("ISO-10303-21;\nkaputt(\n{end}")).expect("nur der Rahmen");
    let start = "ISO-10303-21;\n";
    let fill = MAX_STEP_TEXT_BYTES - start.len() - end.len();
    let mut big = String::with_capacity(MAX_STEP_TEXT_BYTES + 1);
    big.push_str(start);
    big.extend(std::iter::repeat_n(' ', fill));
    big.push_str(end);
    assert_eq!(big.len(), MAX_STEP_TEXT_BYTES);
    let t = Instant::now();
    check_normal_header(&big).expect("genau an der Grenze");
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    big.insert(start.len(), ' ');
    let e = check_normal_header(&big).unwrap_err();
    assert!(e.contains("64 MB"), "{e}");
    drop(big);
    // Jeder Text aus `normalize` geht durch.
    let k = AnalyticKernel::default();
    for solid in [
        k.make_box(DVec3::ONE).unwrap(),
        k.make_sphere(3.0).unwrap(),
        k.make_cone(2.0, 4.0).unwrap(),
    ] {
        let n = normalize(&solid, "Ä \u{1F600}").expect("normalisiert");
        check_normal_header(&n.text).expect("Kopf");
    }
    // Nur genau ein Körper in Millimeter, an der Einheitslage.
    let e = read_normal_body(&data("inch_assembly.step")).unwrap_err();
    assert!(!e.is_empty());
    let three = three_bodies();
    assert!(three.is_ascii());
    assert!(read_normal_body(&three).is_err(), "drei Körper");
    let one = wrap(&box_data());
    read_normal_body(&one).expect("ein Quader in mm");
    let cm = one.replace("SI_UNIT(.MILLI.,.METRE.)", "SI_UNIT(.CENTI.,.METRE.)");
    assert!(read_normal_body(&cm).is_err(), "Zentimeter");
    assert!(read_normal_body(&format!("ISO-10303-21;\nkaputt(\n{end}")).is_err());
}

#[test]
fn free_solids_take_the_unit_of_the_file() {
    // Freie Volumen-Einträge in einer Zoll-Datei: Zoll, nicht still Millimeter.
    let mut g = Gen::new();
    g.context(true);
    g.prism("Quader", &BOX, 30.0);
    let imp = import(&g.file());
    assert_eq!(imp.unit, LengthUnit::Zoll);
    let (lo, hi) = imp.bodies[0].bounds.expect("Hülle");
    near(lo, DVec3::ZERO, 1e-12, "unten");
    near(hi, DVec3::new(254.0, 508.0, 762.0), 1e-9, "oben");
    assert!((imp.bodies[0].tolerance_mm - 2.54e-6).abs() < 1e-18);
    // Umgestellt auf mm.
    let imp = import_with(
        &g.file(),
        &ImportOptions {
            unit_override: Some(LengthUnit::Millimeter),
        },
    );
    let (_, hi) = imp.bodies[0].bounds.expect("Hülle");
    near(hi, DVec3::new(10.0, 20.0, 30.0), 1e-12, "mm");
    // Zwei Kontexte mit verschiedenen Einheiten: unklar → ausgelassen mit Grund.
    let mut g = Gen::new();
    g.context(false);
    g.context(true);
    g.prism("Quader", &BOX, 30.0);
    g.prism("Treppe", &stairs(2), 1.0);
    let p = Progress::new();
    let imp = import_bodies(&parse(&g.file()), &ImportOptions::default(), &p).expect("Import");
    assert_eq!(imp.bodies.len(), 2);
    // Nicht gebaut, aber im Fortschritt gezählt: 6 + 9 Flächen.
    assert_eq!(p.fraction(), 1.0);
    p.set_total(1_000_000);
    assert!((p.fraction() * 1e6 - 15.0).abs() < 0.01, "{}", p.fraction());
    assert_eq!(err(&imp.bodies[1]), err(&imp.bodies[0]));
    assert_eq!(
        err(&imp.bodies[0]),
        "Die Einheit ist unklar: Der Körper steht in keiner Darstellung, und die Datei nennt \
         verschiedene Einheiten."
    );
    // Mit gewählter Einheit ist sie klar.
    let imp = import_with(
        &g.file(),
        &ImportOptions {
            unit_override: Some(LengthUnit::Meter),
        },
    );
    let (_, hi) = imp.bodies[0].bounds.expect("Hülle");
    near(hi, DVec3::new(10_000.0, 20_000.0, 30_000.0), 1e-6, "m");
    // Ohne Kontext: Millimeter, aber als unbekannt gemeldet.
    let mut g = Gen::new();
    g.prism("Quader", &BOX, 30.0);
    let imp = import(&g.file());
    assert_eq!(imp.unit, LengthUnit::Millimeter);
    assert!(!imp.unit_known);
    let (_, hi) = imp.bodies[0].bounds.expect("Hülle");
    near(hi, DVec3::new(10.0, 20.0, 30.0), 1e-12, "ohne Kontext");
    // Ohne Genauigkeit: grob, mit Warnung.
    assert_eq!(
        imp.bodies[0].warnings,
        ["Genauigkeit der Datei 0,001 mm: Rechnen mit diesem Körper kann scheitern."]
    );
    assert_eq!(imp.bodies[0].tolerance_mm, 1e-3);
}

#[test]
fn notes_and_messages() {
    // Nicht unterstützte Körperarten werden gemeldet.
    let text = wrap(&format!(
        "{}\n#5000=FACETED_BREP('',#21);\n#5001=SHELL_BASED_SURFACE_MODEL('',(#21));\n\
         #5002=SHELL_BASED_SURFACE_MODEL('',(#21));",
        box_data()
    ));
    let imp = import(&text);
    assert_eq!(
        imp.messages,
        [
            "Die Datei enthält 1 Facettenkörper (FACETED_BREP); diese Art wird noch nicht \
             unterstützt und ausgelassen.",
            "Die Datei enthält 2 Flächenmodelle (SHELL_BASED_SURFACE_MODEL); diese Art wird noch \
             nicht unterstützt und ausgelassen."
        ]
    );
    assert_eq!(imp.bodies.len(), 1);
    assert!(import(&data("box_ap214.step")).messages.is_empty());
    assert_eq!(
        import(&data("box_ap214.step")).header.file_name,
        "box_ap214.step"
    );

    // Lage-Meldung bei leerem Namen: mit „STEP-Körper“, neu gebildet mit dem Namen der App.
    let mut g = Gen::new();
    let ctx = g.context(false);
    g.prism("", &BOX, 30.0);
    g.add(&format!("SHAPE_REPRESENTATION('',5,#{ctx})"));
    let imp = import(&g.file());
    let b = &imp.bodies[0];
    assert_eq!(b.name, "");
    ok(b);
    assert_eq!(b.warnings.len(), 1, "{:?}", b.warnings);
    let w = &b.warnings[0];
    assert!(
        w.starts_with("Die Lage von „STEP-Körper“ ließ sich nicht bestimmen (Zeile ")
            && w.ends_with("); der Körper liegt am Ursprung."),
        "{w}"
    );
    let named = b.warnings_named("STEP-Körper 4");
    assert_eq!(named.len(), 1);
    assert_eq!(
        named[0],
        w.replace("„STEP-Körper“", "„STEP-Körper 4“"),
        "neu gebildet"
    );
    // Mit Namen: Meldung wie aus der Datei; spiegelnde Lage → Körper am Ursprung, übernommen.
    let imp = import(&data("mapped_item.step"));
    let mirrored: Vec<&ImportedBody> = imp
        .bodies
        .iter()
        .filter(|b| !b.warnings.is_empty())
        .collect();
    assert_eq!(mirrored.len(), 1);
    let m = mirrored[0];
    ok(m);
    assert_eq!(
        m.warnings,
        [format!(
            "Die Lage von „{}“ ließ sich nicht bestimmen (Spiegelung); der Körper liegt am \
             Ursprung.",
            m.name
        )]
    );
    assert_eq!(m.warnings_named(&m.name), m.warnings);
}
