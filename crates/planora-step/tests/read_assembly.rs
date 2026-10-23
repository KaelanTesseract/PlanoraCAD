//! STEP-Leser, Stufe 7: Körper, Namen und Baugruppen-Lagen (STEP-Import, Aufgabe 8).
//! `inch_assembly.step` (Zoll, Grad): Baugruppe „Gestell“ mit dem Block zweimal (Einheitslage;
//! um 90° um z gedreht und um (4 in, 0, 0) verschoben) und dem Stift bei (0, 0, 3 in), dessen
//! `rep_1`/`rep_2` vertauscht sind. `mapped_item.step` (mm): Teil über `MAPPED_ITEM` um
//! (0, 0, 50) versetzt und ein zweites Mal mit Maßstab −1. Weitere Baugruppen erzeugt der Test
//! ([`Gen`]); deren Volumen-Einträge haben leere Schalen (gebaut wird dort nichts).

use planora_exchange::Progress;
use planora_math::{DMat4, DVec3};
use planora_step::read::assembly::{
    FoundBody, MAX_ASSEMBLY_DEPTH, MAX_BODY_NAME_CHARS, find_bodies, placement_note,
};
use planora_step::read::geom::Skip;
use planora_step::read::topo::{BuildOptions, build_solid};
use planora_step::read::units::{Units, context_units};
use planora_step::{ReadLimits, StepFile, StepReadError};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

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

/// `find_bodies` mit den Einheiten der Datei (ohne Kontext: Millimeter).
fn bodies(file: &StepFile) -> Result<Vec<FoundBody>, StepReadError> {
    find_bodies(file, &|c| match c {
        Some(c) => context_units(file, c),
        None => Ok(Units::MM),
    })
}

fn names_of(found: &[FoundBody]) -> Vec<&str> {
    found.iter().map(|b| b.name.as_str()).collect()
}

/// Baut den Körper (Aufgaben 3–7), wendet die Lage an und liefert die Hülle seiner Ecken (mm).
fn bounds(file: &StepFile, body: &FoundBody) -> (DVec3, DVec3) {
    let units = match body.context {
        Some(c) => context_units(file, c).expect("Einheiten"),
        None => Units::MM,
    };
    let built = build_solid(
        file,
        units,
        &body.items,
        &BuildOptions {
            weld_tol_mm: units.weld_tol_mm(),
        },
        &Progress::new(),
    )
    .unwrap_or_else(|Skip(m)| panic!("{}: {m}", body.name));
    let solid = built.solid.transformed(&body.placement).expect("Lage");
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in solid.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    (lo, hi)
}

fn close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= 1e-9, "{what}: {a} statt {b}");
}

fn close_point(a: DVec3, b: [f64; 3], what: &str) {
    assert!(
        a.distance(DVec3::from_array(b)) <= 1e-9,
        "{what}: {a} statt {b:?}"
    );
}

/// Prüft eine Lage an drei Punkten (Ursprung und zwei Achsen) gegen Handwerte.
fn check_placement(m: DMat4, at: [[f64; 3]; 3], what: &str) {
    close_point(m.transform_point3(DVec3::ZERO), at[0], what);
    close_point(m.transform_point3(DVec3::X), at[1], what);
    close_point(m.transform_point3(DVec3::Y), at[2], what);
}

/// Führt `f` in einem eigenen Faden aus; nach `secs` Sekunden scheitert der Test (Endlosschleife).
fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(secs))
        .expect("Zeitgrenze überschritten")
}

fn note(name: &str, reason: &str) -> Option<String> {
    Some(placement_note(name, reason))
}

// ---------------------------------------------------------------------------------------------
// Erzeuger kleiner Baugruppen: Kontext #3 in mm, #6 in Zoll.

const MM: u64 = 3;
const INCH: u64 = 6;

#[derive(Clone, Copy)]
struct Part {
    pd: u64,
    rep: u64,
    /// Lage im Teil (in seiner Darstellung), Gegenstück der Ziel-Lage beim Einbau.
    axis: u64,
}

struct Gen {
    data: String,
    next: u64,
}

impl Gen {
    fn new() -> Gen {
        let data = "#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
             #2=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n\
             #3=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#1,#2))REPRESENTATION_CONTEXT('',''));\n\
             #4=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#1);\n\
             #5=(CONVERSION_BASED_UNIT('INCH',#4)LENGTH_UNIT()NAMED_UNIT(*));\n\
             #6=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#5,#2))REPRESENTATION_CONTEXT('',''));\n\
             #7=APPLICATION_CONTEXT('');\n\
             #8=PRODUCT_CONTEXT('',#7,'mechanical');\n\
             #9=PRODUCT_DEFINITION_CONTEXT('',#7,'design');\n"
            .to_string();
        Gen { data, next: 10 }
    }

    fn add(&mut self, entity: &str) -> u64 {
        let id = self.next;
        self.next += 1;
        let _ = writeln!(self.data, "#{id}={entity};");
        id
    }

    fn triple(v: [f64; 3]) -> String {
        format!("({:?},{:?},{:?})", v[0], v[1], v[2])
    }

    fn point(&mut self, p: [f64; 3]) -> u64 {
        self.add(&format!("CARTESIAN_POINT('',{})", Gen::triple(p)))
    }

    fn dir(&mut self, d: [f64; 3]) -> u64 {
        self.add(&format!("DIRECTION('',{})", Gen::triple(d)))
    }

    /// `AXIS2_PLACEMENT_3D` bei `o` mit Achse `z` und Bezugsrichtung `x`.
    fn axis(&mut self, o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> u64 {
        let p = self.point(o);
        let z = self.dir(z);
        let x = self.dir(x);
        self.add(&format!("AXIS2_PLACEMENT_3D('',#{p},#{z},#{x})"))
    }

    fn shift(&mut self, o: [f64; 3]) -> u64 {
        self.axis(o, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])
    }

    /// Volumen-Eintrag mit leerer Schale (nur für die Suche, nicht zum Bauen).
    fn solid(&mut self, name: &str) -> u64 {
        let shell = self.add("CLOSED_SHELL('',())");
        self.add(&format!("MANIFOLD_SOLID_BREP('{name}',#{shell})"))
    }

    fn rep(&mut self, name: &str, items: &[u64], ctx: u64) -> u64 {
        let list: Vec<String> = items.iter().map(|i| format!("#{i}")).collect();
        self.add(&format!(
            "SHAPE_REPRESENTATION('{name}',({}),#{ctx})",
            list.join(",")
        ))
    }

    /// Produkt mit Definition und `SHAPE_DEFINITION_REPRESENTATION` auf `rep`; liefert die
    /// `PRODUCT_DEFINITION`.
    fn product(&mut self, name: &str, rep: u64) -> u64 {
        let p = self.add(&format!("PRODUCT('{name}','{name}','',(#8))"));
        let f = self.add(&format!("PRODUCT_DEFINITION_FORMATION('','',#{p})"));
        let pd = self.add(&format!("PRODUCT_DEFINITION('design','',#{f},#9)"));
        let pds = self.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{pd})"));
        self.add(&format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})"));
        pd
    }

    /// Teil mit eigener Lage bei `origin`, optional mit einem Volumen-Eintrag `body`.
    fn part_at(&mut self, name: &str, body: Option<&str>, ctx: u64, origin: [f64; 3]) -> Part {
        let axis = self.shift(origin);
        let mut items = vec![axis];
        if let Some(b) = body {
            items.push(self.solid(b));
        }
        let rep = self.rep(name, &items, ctx);
        let pd = self.product(name, rep);
        Part { pd, rep, axis }
    }

    fn part(&mut self, name: &str, body: Option<&str>) -> Part {
        self.part_at(name, body, MM, [0.0; 3])
    }

    fn nauo(&mut self, parent_pd: u64, child_pd: u64) -> u64 {
        self.add(&format!(
            "NEXT_ASSEMBLY_USAGE_OCCURRENCE('','','',#{parent_pd},#{child_pd},$)"
        ))
    }

    /// Vorkommen über `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` mit `definition` (NAUO oder
    /// anderes) und der Beziehung `rep_1`, `rep_2`, Transformation `op`.
    fn cdsr(&mut self, definition: u64, rep_1: u64, rep_2: u64, op: u64) -> u64 {
        let pds = self.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{definition})"));
        let rr = self.add(&format!(
            "(REPRESENTATION_RELATIONSHIP('','',#{rep_1},#{rep_2})\
             REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{op})SHAPE_REPRESENTATION_RELATIONSHIP())"
        ));
        self.add(&format!(
            "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{rr},#{pds})"
        ))
    }

    /// `child` in `parent`: Lage `child.axis` auf `target` (in `parent`).
    fn place(&mut self, parent: Part, child: Part, target: u64) {
        let idt = self.add(&format!(
            "ITEM_DEFINED_TRANSFORMATION('','',#{},#{target})",
            child.axis
        ));
        let nauo = self.nauo(parent.pd, child.pd);
        self.cdsr(nauo, child.rep, parent.rep, idt);
    }

    /// `child` in `parent` mit einer Transformation `op`, Richtung über das NAUO.
    fn place_op(&mut self, parent: Part, child: Part, op: u64) {
        let nauo = self.nauo(parent.pd, child.pd);
        self.cdsr(nauo, child.rep, parent.rep, op);
    }

    /// `CARTESIAN_TRANSFORMATION_OPERATOR_3D` (Achsen `None` → `$`).
    fn cto(
        &mut self,
        axis1: Option<[f64; 3]>,
        axis2: Option<[f64; 3]>,
        origin: [f64; 3],
        scale: f64,
        axis3: Option<[f64; 3]>,
    ) -> u64 {
        let mut d = |a: Option<[f64; 3]>| match a {
            Some(v) => format!("#{}", self.dir(v)),
            None => "$".to_string(),
        };
        let (a1, a2, a3) = (d(axis1), d(axis2), d(axis3));
        let o = self.point(origin);
        self.add(&format!(
            "CARTESIAN_TRANSFORMATION_OPERATOR_3D('','',{a1},{a2},#{o},{scale:?},{a3})"
        ))
    }

    fn text(&self) -> String {
        format!(
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
             FILE_NAME('t.step','2026-10-23T00:00:00',(''),(''),'','','');\n\
             FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n{}ENDSEC;\nEND-ISO-10303-21;\n",
            self.data
        )
    }
}

// ---------------------------------------------------------------------------------------------

#[test]
fn assembly_places_each_occurrence() {
    let file = parse(&data("inch_assembly.step"));
    let found = bodies(&file).expect("Körper");
    assert_eq!(names_of(&found), ["Block", "Block (2)", "Stift"]);
    assert!(
        found.iter().all(|b| b.placement_note.is_none()),
        "{found:?}"
    );
    assert_eq!(found[0].items, [1020]);
    assert_eq!(found[1].items, [1020]);
    assert_eq!(found[2].items, [2000]);
    assert!(found.iter().all(|b| b.context == Some(110)));
    // Handwerte der Lagen (mm): Block 2 dreht x nach y und liegt um 4 in = 101,6 mm verschoben.
    check_placement(
        found[0].placement,
        [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        "Block",
    );
    check_placement(
        found[1].placement,
        [[101.6, 0.0, 0.0], [101.6, 1.0, 0.0], [100.6, 0.0, 0.0]],
        "Block (2)",
    );
    // Stift: Lage im Teil um 90° gedreht, Ziel bei (0, 0, 3 in): T = A(Ziel) · A(Teil)⁻¹.
    check_placement(
        found[2].placement,
        [[0.0, 0.0, 76.2], [0.0, -1.0, 76.2], [1.0, 0.0, 76.2]],
        "Stift",
    );

    let (lo, hi) = bounds(&file, &found[0]);
    close_point(lo, [0.0; 3], "Block unten");
    close_point(hi, [25.4, 50.8, 76.2], "Block oben");
    let (lo, hi) = bounds(&file, &found[1]);
    close_point(lo, [50.8, 0.0, 0.0], "Block (2) unten");
    close_point(hi, [101.6, 25.4, 76.2], "Block (2) oben");
    let (lo, hi) = bounds(&file, &found[2]);
    close(lo.z, 76.2, "Stift unten");
    close(hi.z, 127.0, "Stift oben");

    // Bestimmt: derselbe Aufruf liefert dasselbe.
    assert_eq!(bodies(&file).expect("Körper"), found);
}

#[test]
fn mapped_items_and_bad_transforms() {
    let file = parse(&data("mapped_item.step"));
    let found = bodies(&file).expect("Körper");
    assert_eq!(names_of(&found), ["Teil", "Teil (2)"]);
    assert_eq!(found[0].placement_note, None);
    assert_eq!(
        found[0].placement,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 50.0))
    );
    let (lo, hi) = bounds(&file, &found[0]);
    close_point(lo, [0.0, 0.0, 50.0], "Teil unten");
    close_point(hi, [10.0, 20.0, 80.0], "Teil oben");
    assert_eq!(
        found[1].placement_note.as_deref(),
        Some(
            "Die Lage von „Teil (2)“ ließ sich nicht bestimmen (Spiegelung); der Körper liegt am Ursprung."
        )
    );
    assert_eq!(found[1].placement, DMat4::IDENTITY);
    assert_eq!(found[1].items, [1020]);
}

#[test]
fn nested_placements_multiply_to_the_root() {
    // R (mm) enthält S (Zoll) gedreht und verschoben, S enthält P (mm); R enthält P zusätzlich
    // direkt. Lage von S im Teil bei (0, 0, 1 in): Verschiebungen je in der Einheit ihrer
    // Darstellung.
    let mut g = Gen::new();
    let r = g.part("R", None);
    let s = g.part_at("S", None, INCH, [0.0, 0.0, 1.0]);
    let p = g.part("P", Some("Volumen P"));
    let t_rs = g.axis([10.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
    g.place(r, s, t_rs);
    let t_rp = g.shift([0.0, 0.0, 7.0]);
    g.place(r, p, t_rp);
    let t_sp = g.shift([0.0, 1.0, 0.0]); // in S: Zoll
    g.place(s, p, t_sp);
    let file = parse(&g.text());
    let found = bodies(&file).expect("Körper");
    assert_eq!(names_of(&found), ["P", "P (2)"]);
    assert!(found.iter().all(|b| b.placement_note.is_none()));
    // P über S: M = T_RS · T_SP; T_SP verschiebt um (0, 25,4, 0); T_RS(q) = D90(q − (0,0,25,4))
    // + (10, 0, 0). M(0) = D90(0, 25,4, −25,4) + (10, 0, 0) = (−15,4, 0, −25,4).
    check_placement(
        found[0].placement,
        [
            [-15.4, 0.0, -25.4],
            [-15.4, 1.0, -25.4],
            [-16.4, 0.0, -25.4],
        ],
        "P über S",
    );
    check_placement(
        found[1].placement,
        [[0.0, 0.0, 7.0], [1.0, 0.0, 7.0], [0.0, 1.0, 7.0]],
        "P direkt",
    );
}

#[test]
fn transform_kinds() {
    let mut g = Gen::new();
    let r = g.part("R", None);
    let k: Vec<Part> = (1..=8)
        .map(|i| g.part(&format!("K{i}"), Some("")))
        .collect();
    let z = Some([0.0, 0.0, 1.0]);
    // 1: Maßstab 2
    let op = g.cto(None, None, [0.0; 3], 2.0, None);
    g.place_op(r, k[0], op);
    // 2: schiefe Achsen
    let op = g.cto(Some([1.0, 0.0, 0.1]), None, [0.0; 3], 1.0, z);
    g.place_op(r, k[1], op);
    // 3: linkshändig (Spiegelung über die y-Achse)
    let op = g.cto(
        Some([1.0, 0.0, 0.0]),
        Some([0.0, -1.0, 0.0]),
        [0.0; 3],
        1.0,
        z,
    );
    g.place_op(r, k[2], op);
    // 4: gültige Drehung um 90° um z, verschoben um (5, 0, 0)
    let op = g.cto(
        Some([0.0, 1.0, 0.0]),
        Some([-1.0, 0.0, 0.0]),
        [5.0, 0.0, 0.0],
        1.0,
        z,
    );
    g.place_op(r, k[3], op);
    // 5: Lage als AXIS2_PLACEMENT_2D
    let p2 = g.add("CARTESIAN_POINT('',(0.,0.))");
    let a2 = g.add(&format!("AXIS2_PLACEMENT_2D('',#{p2},$)"));
    let target = g.shift([0.0; 3]);
    let idt = g.add(&format!(
        "ITEM_DEFINED_TRANSFORMATION('','',#{a2},#{target})"
    ));
    g.place_op(r, k[4], idt);
    // 6: NAUO zwischen fremden Teilen: keine Darstellung gehört zu ihnen
    let u1 = g.part("U1", None);
    let u2 = g.part("U2", None);
    let nauo = g.nauo(u1.pd, u2.pd);
    let t = g.shift([1.0, 0.0, 0.0]);
    let idt = g.add(&format!(
        "ITEM_DEFINED_TRANSFORMATION('','',#{},#{t})",
        k[5].axis
    ));
    g.cdsr(nauo, k[5].rep, r.rep, idt);
    // 7: Definition ist kein Vorkommen (die PRODUCT_DEFINITION selbst)
    let t = g.shift([1.0, 0.0, 0.0]);
    let idt = g.add(&format!(
        "ITEM_DEFINED_TRANSFORMATION('','',#{},#{t})",
        k[6].axis
    ));
    g.cdsr(r.pd, k[6].rep, r.rep, idt);
    // 8: vertauscht mit Operator: Er bildet rep_1 (R) auf rep_2 (K8) ab, also gilt sein Kehrwert.
    let op = g.cto(None, None, [0.0, 0.0, -9.0], 1.0, None);
    let nauo = g.nauo(r.pd, k[7].pd);
    g.cdsr(nauo, r.rep, k[7].rep, op);
    let file = parse(&g.text());
    let found = bodies(&file).expect("Körper");
    let n: Vec<String> = (1..=8).map(|i| format!("K{i}")).collect();
    assert_eq!(names_of(&found), n);
    let expected = [
        note("K1", "Skalierung"),
        note("K2", "Skalierung"),
        note("K3", "Spiegelung"),
        None,
        note("K5", "unbekannte Art AXIS2_PLACEMENT_2D"),
        note("K6", "Richtung unklar"),
        note("K7", "Richtung unklar"),
        None,
    ];
    for (b, e) in found.iter().zip(&expected) {
        assert_eq!(&b.placement_note, e, "{}", b.name);
        if e.is_some() {
            assert_eq!(b.placement, DMat4::IDENTITY, "{}", b.name);
        }
    }
    check_placement(
        found[3].placement,
        [[5.0, 0.0, 0.0], [5.0, 1.0, 0.0], [4.0, 0.0, 0.0]],
        "K4",
    );
    check_placement(
        found[7].placement,
        [[0.0, 0.0, 9.0], [1.0, 0.0, 9.0], [0.0, 1.0, 9.0]],
        "K8",
    );
}

#[test]
fn bad_placement_reaches_the_whole_subtree() {
    // R → S gespiegelt, S → P: P liegt mit demselben Grund am Ursprung.
    let mut g = Gen::new();
    let r = g.part("R", None);
    let s = g.part("S", None);
    let p = g.part("P", Some("x"));
    let op = g.cto(None, None, [3.0, 0.0, 0.0], -1.0, None);
    g.place_op(r, s, op);
    let t = g.shift([0.0, 4.0, 0.0]);
    g.place(s, p, t);
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["P"]);
    assert_eq!(found[0].placement_note, note("P", "Spiegelung"));
    assert_eq!(found[0].placement, DMat4::IDENTITY);
}

#[test]
fn missing_relations_fail_with_line() {
    let mut g = Gen::new();
    let r = g.part("R", None);
    g.part("P", Some("x"));
    let pds = g.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{})", r.pd));
    g.add(&format!(
        "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#99999,#{pds})"
    ));
    let err = bodies(&parse(&g.text())).expect_err("fehlender Verweis");
    assert!(
        matches!(err, StepReadError::MissingRef { target: 99999, .. }),
        "{err}"
    );
}

#[test]
fn cycles_and_depth() {
    // A enthält B, B enthält A; keine Wurzel.
    let mut g = Gen::new();
    let a = g.part("A", Some("x"));
    let b = g.part("B", Some("x"));
    let t = g.shift([1.0, 0.0, 0.0]);
    g.place(a, b, t);
    let t = g.shift([0.0, 1.0, 0.0]);
    g.place(b, a, t);
    let text = g.text();
    let found = within(20, move || bodies(&parse(&text))).expect("Körper");
    assert_eq!(names_of(&found), ["A", "B"]);
    assert_eq!(found[0].placement_note, note("A", "Verweise im Kreis"));
    assert_eq!(found[1].placement_note, note("B", "Verweise im Kreis"));
    assert!(found.iter().all(|b| b.placement == DMat4::IDENTITY));

    // R enthält A und P; A ⇄ B. P liegt richtig, A und B einmal mit Grund.
    let mut g = Gen::new();
    let r = g.part("R", None);
    let a = g.part("A", Some("x"));
    let b = g.part("B", Some("x"));
    let p = g.part("P", Some("x"));
    let t = g.shift([5.0, 0.0, 0.0]);
    g.place(r, a, t);
    let t = g.shift([1.0, 0.0, 0.0]);
    g.place(a, b, t);
    let t = g.shift([0.0, 1.0, 0.0]);
    g.place(b, a, t);
    let t = g.shift([0.0, 0.0, 2.0]);
    g.place(r, p, t);
    let text = g.text();
    let found = within(20, move || bodies(&parse(&text))).expect("Körper");
    assert_eq!(names_of(&found), ["P", "A", "B"]);
    assert_eq!(found[0].placement_note, None);
    assert_eq!(
        found[0].placement,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 2.0))
    );
    assert_eq!(found[1].placement_note, note("A", "Verweise im Kreis"));
    assert_eq!(found[2].placement_note, note("B", "Verweise im Kreis"));

    // Darstellung, die sich selbst als MAPPED_ITEM enthält.
    let mut g = Gen::new();
    let axis = g.shift([0.0; 3]);
    let solid = g.solid("x");
    let rep_id = g.next + 2;
    let map = g.add(&format!("REPRESENTATION_MAP(#{axis},#{rep_id})"));
    let item = g.add(&format!("MAPPED_ITEM('',#{map},#{axis})"));
    let rep = g.rep("Selbst", &[axis, solid, item], MM);
    assert_eq!(rep, rep_id);
    let text = g.text();
    let found = within(20, move || bodies(&parse(&text))).expect("Körper");
    assert_eq!(names_of(&found), ["Selbst"]);
    assert_eq!(found[0].placement_note, note("Selbst", "Verweise im Kreis"));

    // Ketten: 32 Ebenen gehen, 33 sind zu tief.
    for (levels, ok) in [(MAX_ASSEMBLY_DEPTH, true), (MAX_ASSEMBLY_DEPTH + 1, false)] {
        let mut g = Gen::new();
        let chain: Vec<Part> = (0..=levels)
            .map(|i| {
                let body = (i == levels).then_some("x");
                g.part(&format!("L{i}"), body)
            })
            .collect();
        for w in chain.windows(2) {
            let t = g.shift([1.0, 0.0, 0.0]);
            g.place(w[0], w[1], t);
        }
        let text = g.text();
        let found = within(20, move || bodies(&parse(&text))).expect("Körper");
        let name = format!("L{levels}");
        assert_eq!(names_of(&found), [name.as_str()]);
        if ok {
            assert_eq!(found[0].placement_note, None);
            check_placement(
                found[0].placement,
                [
                    [levels as f64, 0.0, 0.0],
                    [levels as f64 + 1.0, 0.0, 0.0],
                    [levels as f64, 1.0, 0.0],
                ],
                "Kette",
            );
        } else {
            assert_eq!(
                found[0].placement_note,
                note(&name, "zu tief verschachtelt")
            );
            assert_eq!(found[0].placement, DMat4::IDENTITY);
        }
    }
}

#[test]
fn names() {
    let mut g = Gen::new();
    // Freier Volumen-Eintrag (in keiner Darstellung) steht zuerst in der Datei, kommt aber zuletzt.
    g.solid("Frei");
    let umlaut80 = "\\X2\\00C4\\X0\\".repeat(80);
    g.part(&umlaut80, Some("x"));
    g.part("  Rad\\X\\09links \\X\\0A ", Some("x"));
    let s = g.solid("Volumen");
    g.rep("Darstellung", &[s], MM);
    let s = g.solid("Volumen");
    g.rep("", &[s], MM);
    let s = g.solid("");
    g.rep("", &[s], MM);
    let s = g.solid("");
    g.rep(" ", &[s], MM);
    let s = g.solid("x");
    let rep = g.rep("Ersatz", &[s], MM);
    g.product("", rep);
    g.part("Teil", Some("x"));
    g.part("Teil", Some("x"));
    g.part("Teil (2)", Some("x"));
    g.part(&umlaut80, Some("x"));
    let found = bodies(&parse(&g.text())).expect("Körper");
    let a64 = "Ä".repeat(MAX_BODY_NAME_CHARS);
    let a60 = format!("{} (2)", "Ä".repeat(MAX_BODY_NAME_CHARS - 4));
    assert_eq!(
        names_of(&found),
        [
            a64.as_str(),
            "Rad links",
            "Darstellung",
            "Volumen",
            "",
            "",
            "Ersatz",
            "Teil",
            "Teil (2)",
            "Teil (2) (2)",
            a60.as_str(),
            "Frei",
        ]
    );
    assert_eq!(found[0].name.chars().count(), 64);
    assert_eq!(found[10].name.chars().count(), 64);
    assert_eq!(found[11].context, None);
    assert!(found.iter().all(|b| b.placement_note.is_none()));
}

#[test]
fn limits() {
    for (n, ok) in [(50usize, true), (51, false)] {
        let mut g = Gen::new();
        let r = g.part("R", None);
        let p = g.part("P", Some("x"));
        for i in 0..n {
            let t = g.shift([i as f64, 0.0, 0.0]);
            g.place(r, p, t);
        }
        let result = bodies(&parse(&g.text()));
        if ok {
            let found = result.expect("50 Körper");
            assert_eq!(found.len(), 50);
            assert_eq!(found[49].name, "P (50)");
            assert_eq!(
                found[49].placement,
                DMat4::from_translation(DVec3::new(49.0, 0.0, 0.0))
            );
        } else {
            let err = result.expect_err("zu viele");
            assert_eq!(err, StepReadError::TooManyBodies);
            assert_eq!(err.to_string(), "Die Datei enthält mehr als 50 Körper.");
        }
    }
    // 51 Körper als freie Volumen-Einträge zählen ebenso.
    let mut g = Gen::new();
    for _ in 0..51 {
        g.solid("x");
    }
    assert_eq!(bodies(&parse(&g.text())), Err(StepReadError::TooManyBodies));

    assert_eq!(
        bodies(&parse(&Gen::new().text())),
        Err(StepReadError::NoBodies)
    );

    let file = parse(&data("box_ap214.step"));
    let found = bodies(&file).expect("Körper");
    assert_eq!(names_of(&found), ["Würfel"]);
    assert_eq!(found[0].placement, DMat4::IDENTITY);
    assert_eq!(found[0].placement_note, None);
    assert_eq!(found[0].items, [20]);
    assert_eq!(found[0].context, Some(105));
}

#[test]
fn many_empty_branches_stay_fast() {
    // 30 Ebenen, je Ebene zwei Vorkommen der nächsten (2^30 Wege), der Körper hängt an der
    // Wurzel: nichts darf alle Wege ablaufen.
    let mut g = Gen::new();
    let levels: Vec<Part> = (0..30)
        .map(|i| g.part(&format!("E{i}"), (i == 0).then_some("x")))
        .collect();
    for w in levels.windows(2) {
        for _ in 0..2 {
            let t = g.shift([1.0, 0.0, 0.0]);
            g.place(w[0], w[1], t);
        }
    }
    let text = g.text();
    let found = within(20, move || bodies(&parse(&text))).expect("Körper");
    assert_eq!(names_of(&found), ["E0"]);
}
