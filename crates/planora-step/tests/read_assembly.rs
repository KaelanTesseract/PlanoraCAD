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
    unsupported_kinds,
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
        self.product_id(name, name, rep)
    }

    /// Produkt mit Kennung `id` und Namen `name`.
    fn product_id(&mut self, id: &str, name: &str, rep: u64) -> u64 {
        let p = self.add(&format!("PRODUCT('{id}','{name}','',(#8))"));
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

    /// Teil mit der Lage `axis` und weiteren Einträgen `extra` (etwa Ziel-Lagen).
    fn part_with(
        &mut self,
        name: &str,
        body: Option<&str>,
        ctx: u64,
        axis: u64,
        extra: &[u64],
    ) -> Part {
        let mut items = vec![axis];
        items.extend_from_slice(extra);
        if let Some(b) = body {
            items.push(self.solid(b));
        }
        let rep = self.rep(name, &items, ctx);
        let pd = self.product(name, rep);
        Part { pd, rep, axis }
    }

    /// `MAPPED_ITEM` der Darstellung `child` mit Ursprung `origin` (in `child`) auf `target`.
    fn mapped(&mut self, origin: u64, child: u64, target: u64) -> u64 {
        let map = self.add(&format!("REPRESENTATION_MAP(#{origin},#{child})"));
        self.add(&format!("MAPPED_ITEM('',#{map},#{target})"))
    }

    /// Beziehung ohne Vorkommen (`op`: Transformation oder keine).
    fn relation(&mut self, rep_1: u64, rep_2: u64, op: Option<u64>) -> u64 {
        match op {
            None => self.add(&format!(
                "SHAPE_REPRESENTATION_RELATIONSHIP('','',#{rep_1},#{rep_2})"
            )),
            Some(op) => self.add(&format!(
                "(REPRESENTATION_RELATIONSHIP('','',#{rep_1},#{rep_2})\
                 REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{op})SHAPE_REPRESENTATION_RELATIONSHIP())"
            )),
        }
    }

    /// Vorkommen mit schlichter `SHAPE_REPRESENTATION_RELATIONSHIP` (ohne Transformation).
    fn cdsr_plain(&mut self, definition: u64, rep_1: u64, rep_2: u64) -> u64 {
        let pds = self.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{definition})"));
        let rr = self.relation(rep_1, rep_2, None);
        self.add(&format!(
            "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{rr},#{pds})"
        ))
    }

    fn idt(&mut self, it_1: u64, it_2: u64) -> u64 {
        self.add(&format!(
            "ITEM_DEFINED_TRANSFORMATION('','',#{it_1},#{it_2})"
        ))
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
        let (a1, a2, o, a3) = self.cto_args(axis1, axis2, origin, axis3);
        self.add(&format!(
            "CARTESIAN_TRANSFORMATION_OPERATOR_3D('','',{a1},{a2},{o},{scale:?},{a3})"
        ))
    }

    /// Verweise (oder `$`) für die Achsen und den Ursprung eines Operators.
    fn cto_args(
        &mut self,
        axis1: Option<[f64; 3]>,
        axis2: Option<[f64; 3]>,
        origin: [f64; 3],
        axis3: Option<[f64; 3]>,
    ) -> (String, String, String, String) {
        let mut d = |a: Option<[f64; 3]>| match a {
            Some(v) => format!("#{}", self.dir(v)),
            None => "$".to_string(),
        };
        let (a1, a2, a3) = (d(axis1), d(axis2), d(axis3));
        let o = format!("#{}", self.point(origin));
        (a1, a2, o, a3)
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
        note("K2", "schiefe Achsen"),
        note("K3", "Spiegelung"),
        None,
        note("K5", "unbekannte Form der Lage: AXIS2_PLACEMENT_2D"),
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
    // Das Vorkommen nennt R und P, seine Beziehung fehlt: P läge still am Ursprung → Datei
    // abgelehnt.
    let mut g = Gen::new();
    let r = g.part("R", None);
    let p = g.part("P", Some("x"));
    let nauo = g.nauo(r.pd, p.pd);
    let pds = g.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{nauo})"));
    g.add(&format!(
        "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#99999,#{pds})"
    ));
    let err = bodies(&parse(&g.text())).expect_err("fehlender Verweis");
    assert!(
        matches!(err, StepReadError::MissingRef { target: 99999, .. }),
        "{err}"
    );

    // Ohne Vorkommen (Definition ist die Produktdefinition selbst) betrifft sie keinen Körper.
    let mut g = Gen::new();
    let r = g.part("R", None);
    g.part("P", Some("x"));
    let pds = g.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{})", r.pd));
    g.add(&format!(
        "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#99999,#{pds})"
    ));
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["P"]);
    assert_eq!(found[0].placement_note, None);
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

// ---------------------------------------------------------------------------------------------
// Fix-Runde 1: Lagen, die nicht vertauschbar sind, und die Befunde I1–I5, M1, M2, M4, M5.

/// Kind-Lage C (mm): bei (0, 2, 0), z = x-Achse, x = y-Achse (also y = z).
fn skew_child(g: &mut Gen) -> u64 {
    g.axis([0.0, 2.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
}

/// Ziel-Lage P: bei (`at`, 0, 0), z = z-Achse, x = y-Achse (Drehung um 90° um z).
fn skew_target(g: &mut Gen, at: f64) -> u64 {
    g.axis([at, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0])
}

/// Handwerte für `T = P · C⁻¹` mit P bei (`at` mm, 0, 0): C⁻¹(p) = ((p−(0,2,0))·y, ·z, ·x),
/// P(a, b, c) = (at, 0, 0) + a·y − b·x + c·z. Also T(0) = (at, −2, 0), T(x) = (at, −2, 1),
/// T(y) = (at, −1, 0). Falsch herum (C⁻¹ · P) gäbe T(0) = (−2·…) mit z = at.
fn skew_expected(at: f64) -> [[f64; 3]; 3] {
    [[at, -2.0, 0.0], [at, -2.0, 1.0], [at, -1.0, 0.0]]
}

/// Schiebt die Zeilen der Einträge `ids` an den Anfang des Datenteils (STEP erlaubt jede
/// Reihenfolge; so lässt sich prüfen, dass das Ergebnis nicht von ihr abhängt).
fn hoist(text: &str, ids: &[u64]) -> String {
    let starts: Vec<String> = ids.iter().map(|i| format!("#{i}=")).collect();
    let (moved, rest): (Vec<&str>, Vec<&str>) = text
        .lines()
        .partition(|l| starts.iter().any(|s| l.starts_with(s.as_str())));
    let mut out = String::new();
    for l in rest {
        out.push_str(l);
        out.push('\n');
        if l == "DATA;" {
            for m in &moved {
                out.push_str(m);
                out.push('\n');
            }
        }
    }
    out
}

#[test]
fn non_commuting_placements() {
    let mut g = Gen::new();
    // (a) Vorkommen: Kind-Lage gedreht und quer zur Drehachse verschoben.
    let target = skew_target(&mut g, 10.0);
    let r_axis = g.shift([0.0; 3]);
    let r = g.part_with("R", None, MM, r_axis, &[target]);
    let c = skew_child(&mut g);
    let k = g.part_with("K", Some("x"), MM, c, &[]);
    g.place(r, k, target);
    // (b) MAPPED_ITEM: Eltern in Zoll (Ziel bei 10 in = 254 mm), Kind in mm, Ursprung ≠ Einheit.
    let c2 = skew_child(&mut g);
    let m = g.part_with("M", Some("x"), MM, c2, &[]);
    let target2 = skew_target(&mut g, 10.0);
    let item = g.mapped(c2, m.rep, target2);
    let axis2 = g.shift([0.0; 3]);
    let rep2 = g.rep("R2", &[axis2, item], INCH);
    g.product("R2", rep2);
    // (c) Operator in einer Baugruppe in Zoll: Ursprung (1, 0, 0) in = 25,4 mm.
    let r3 = g.part_at("R3", None, INCH, [0.0; 3]);
    let k3 = g.part("K3", Some("x"));
    let op = g.cto(
        Some([0.0, 1.0, 0.0]),
        Some([-1.0, 0.0, 0.0]),
        [1.0, 0.0, 0.0],
        1.0,
        Some([0.0, 0.0, 1.0]),
    );
    g.place_op(r3, k3, op);
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["K", "M", "K3"]);
    assert!(
        found.iter().all(|b| b.placement_note.is_none()),
        "{found:?}"
    );
    check_placement(found[0].placement, skew_expected(10.0), "K");
    check_placement(found[1].placement, skew_expected(254.0), "M");
    check_placement(
        found[2].placement,
        [[25.4, 0.0, 0.0], [25.4, 1.0, 0.0], [24.4, 0.0, 0.0]],
        "K3",
    );
}

#[test]
fn operator_rules() {
    let mut g = Gen::new();
    let r = g.part("R", None);
    let x = Some([1.0, 0.0, 0.0]);
    let y = Some([0.0, 1.0, 0.0]);
    let z = Some([0.0, 0.0, 1.0]);
    let mut ops: Vec<u64> = Vec::new();
    // 1: Maßstab 0,5
    ops.push(g.cto(None, None, [0.0; 3], 0.5, None));
    // 2: schiefe axis2
    ops.push(g.cto(x, Some([0.1, 1.0, 0.0]), [0.0; 3], 1.0, z));
    // 3: Maßstab `$` gilt als 1
    let (a1, a2, o, a3) = g.cto_args(None, None, [0.0, 0.0, 4.0], None);
    ops.push(g.add(&format!(
        "CARTESIAN_TRANSFORMATION_OPERATOR_3D('','',{a1},{a2},{o},$,{a3})"
    )));
    // 4: zusammengesetzter Operator, Drehung um 90° um z, Ursprung (2, 0, 0)
    let (a1, a2, o, a3) = g.cto_args(y, Some([-1.0, 0.0, 0.0]), [2.0, 0.0, 0.0], z);
    ops.push(g.add(&format!(
        "(CARTESIAN_TRANSFORMATION_OPERATOR({a1},{a2},{o},1.)CARTESIAN_TRANSFORMATION_OPERATOR_3D({a3})\
         FUNCTIONALLY_DEFINED_TRANSFORMATION('','')GEOMETRIC_REPRESENTATION_ITEM()REPRESENTATION_ITEM(''))"
    )));
    // 5: zusammengesetzt, nicht gleichmäßig (scale2 = 2)
    let (a1, a2, o, a3) = g.cto_args(None, None, [0.0; 3], None);
    ops.push(g.add(&format!(
        "(CARTESIAN_TRANSFORMATION_OPERATOR({a1},{a2},{o},1.)CARTESIAN_TRANSFORMATION_OPERATOR_3D({a3})\
         CARTESIAN_TRANSFORMATION_OPERATOR_3D_NON_UNIFORM(2.,1.)\
         FUNCTIONALLY_DEFINED_TRANSFORMATION('','')GEOMETRIC_REPRESENTATION_ITEM()REPRESENTATION_ITEM(''))"
    )));
    // 6: einfacher nicht gleichmäßiger Operator mit lauter 1 bzw. `$`: Bewegung
    let (a1, a2, o, a3) = g.cto_args(None, None, [0.0, 6.0, 0.0], None);
    ops.push(g.add(&format!(
        "CARTESIAN_TRANSFORMATION_OPERATOR_3D_NON_UNIFORM('','',{a1},{a2},{o},1.,{a3},1.,$)"
    )));
    // 7: dto. mit scale3 = 0,5
    let (a1, a2, o, a3) = g.cto_args(None, None, [0.0; 3], None);
    ops.push(g.add(&format!(
        "CARTESIAN_TRANSFORMATION_OPERATOR_3D_NON_UNIFORM('','',{a1},{a2},{o},1.,{a3},1.,0.5)"
    )));
    // 8: ISO base_axis: axis3 = x, axis1 `$` → d1 = y (first_proj_axis), axis2 = z → d2 = z.
    ops.push(g.cto(None, z, [0.0; 3], 1.0, x));
    // 9: axis2 auf der (Vorgabe-)Achse z: ISO undefiniert
    ops.push(g.cto(x, z, [0.0; 3], 1.0, None));
    // 10: Maßstab −1 mit linkshändigen Achsen (Determinante > 0): ISO verlangt Maßstab > 0
    ops.push(g.cto(x, Some([0.0, -1.0, 0.0]), [0.0; 3], -1.0, z));
    // 11: Maßstab 1,0000001 (innerhalb 1e-6)
    ops.push(g.cto(None, None, [0.0, 0.0, 1.0], 1.000_000_1, None));
    // 12: Maßstab 1,00001
    ops.push(g.cto(None, None, [0.0; 3], 1.000_01, None));
    // 13: axis1 = −x ohne axis2: ISO nimmt y = (0, 1, 0) → linkshändig
    ops.push(g.cto(Some([-1.0, 0.0, 0.0]), None, [0.0; 3], 1.0, z));
    let parts: Vec<Part> = (1..=ops.len())
        .map(|i| g.part(&format!("C{i}"), Some("x")))
        .collect();
    for (p, op) in parts.iter().zip(&ops) {
        g.place_op(r, *p, *op);
    }
    let found = bodies(&parse(&g.text())).expect("Körper");
    let expected = [
        note("C1", "Skalierung"),
        note("C2", "schiefe Achsen"),
        None,
        None,
        note("C5", "Skalierung"),
        None,
        note("C7", "Skalierung"),
        None,
        note("C9", "schiefe Achsen"),
        note("C10", "Skalierung"),
        None,
        note("C12", "Skalierung"),
        note("C13", "Spiegelung"),
    ];
    assert_eq!(found.len(), expected.len());
    for (b, e) in found.iter().zip(&expected) {
        assert_eq!(&b.placement_note, e, "{}", b.name);
    }
    check_placement(
        found[2].placement,
        [[0.0, 0.0, 4.0], [1.0, 0.0, 4.0], [0.0, 1.0, 4.0]],
        "C3",
    );
    check_placement(
        found[3].placement,
        [[2.0, 0.0, 0.0], [2.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
        "C4",
    );
    check_placement(
        found[5].placement,
        [[0.0, 6.0, 0.0], [1.0, 6.0, 0.0], [0.0, 7.0, 0.0]],
        "C6",
    );
    check_placement(
        found[7].placement,
        [[0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        "C8",
    );
    // Maßstab innerhalb der Toleranz: genaue Bewegung (Maßstab 1).
    assert_eq!(
        found[10].placement,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 1.0))
    );
}

#[test]
fn relationship_without_occurrence() {
    let mut g = Gen::new();
    // A: rep_1 = Darstellung mit Produkt (Eltern), rep_2 = Körper ohne Produkt (Kind).
    let sr_axis = g.shift([0.0, 0.0, 5.0]);
    let sr = g.rep("Teil A", &[sr_axis], MM);
    g.product("Teil A", sr);
    let ab_axis = g.shift([0.0; 3]);
    let s = g.solid("x");
    let ab = g.rep("Volumen A", &[ab_axis, s], MM);
    let idt = g.idt(sr_axis, ab_axis);
    g.relation(sr, ab, Some(idt));
    // B: rep_1 ohne Produkt, rep_2 mit Produkt: ISO, rep_2 ist Eltern.
    let ab_axis = g.shift([0.0; 3]);
    let s = g.solid("x");
    let ab = g.rep("Volumen B", &[ab_axis, s], MM);
    let sr_axis = g.shift([0.0, 0.0, 7.0]);
    let sr = g.rep("Teil B", &[sr_axis], MM);
    g.product("Teil B", sr);
    let idt = g.idt(ab_axis, sr_axis);
    g.relation(ab, sr, Some(idt));
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["Volumen A", "Volumen B"]);
    assert!(
        found.iter().all(|b| b.placement_note.is_none()),
        "{found:?}"
    );
    // Ohne Vorkommen ist die Richtung angenommen (Fix-Runde 1 von Aufgabe 9, P3).
    assert!(found.iter().all(|b| b.placement_guessed), "{found:?}");
    let inch = bodies(&parse(&data("inch_assembly.step"))).expect("Körper");
    assert!(inch.iter().all(|b| !b.placement_guessed));
    assert_eq!(
        found[0].placement,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0))
    );
    assert_eq!(
        found[1].placement,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 7.0))
    );
}

#[test]
fn names_use_product_name_and_replace_inner_controls() {
    let mut g = Gen::new();
    let s = g.solid("x");
    let rep = g.rep("Darstellung", &[s], MM);
    g.product_id("Kennung", "Anzeige", rep);
    g.part("Rad\\X\\0Alinks", Some("x"));
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["Anzeige", "Rad links"]);
}

#[test]
fn direction_via_the_parent() {
    // Die Darstellung des Körpers gehört zu keinem Produkt; die Richtung folgt aus dem
    // Elternteil (rep_2 bzw. vertauscht rep_1 gehört zu R).
    let mut g = Gen::new();
    let t1 = g.shift([0.0, 0.0, 3.0]);
    let t2 = g.shift([0.0, 0.0, 9.0]);
    let r_axis = g.shift([0.0; 3]);
    let r = g.part_with("R", None, MM, r_axis, &[t1, t2]);
    for (i, t) in [t1, t2].into_iter().enumerate() {
        let c_axis = g.shift([0.0; 3]);
        let c = g.part_with(&format!("C{i}"), None, MM, c_axis, &[]);
        let ab_axis = g.shift([0.0; 3]);
        let s = g.solid("x");
        let ab = g.rep(&format!("V{i}"), &[ab_axis, s], MM);
        let nauo = g.nauo(r.pd, c.pd);
        if i == 0 {
            let idt = g.idt(ab_axis, t);
            g.cdsr(nauo, ab, r.rep, idt);
        } else {
            let idt = g.idt(t, ab_axis);
            g.cdsr(nauo, r.rep, ab, idt);
        }
    }
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["V0", "V1"]);
    assert!(
        found.iter().all(|b| b.placement_note.is_none()),
        "{found:?}"
    );
    assert_eq!(
        found[0].placement,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 3.0))
    );
    assert_eq!(
        found[1].placement,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 9.0))
    );
}

#[test]
fn self_loop_under_a_root() {
    let mut g = Gen::new();
    let axis = g.shift([0.0; 3]);
    let solid = g.solid("x");
    let rep_id = g.next + 2;
    let map = g.add(&format!("REPRESENTATION_MAP(#{axis},#{rep_id})"));
    let item = g.add(&format!("MAPPED_ITEM('',#{map},#{axis})"));
    let rep = g.rep("S", &[axis, solid, item], MM);
    assert_eq!(rep, rep_id);
    let pd = g.product("S", rep);
    let s = Part { pd, rep, axis };
    let r = g.part("R", None);
    let t = g.shift([1.0, 0.0, 0.0]);
    g.place(r, s, t);
    let text = g.text();
    let found = within(20, move || bodies(&parse(&text))).expect("Körper");
    assert_eq!(names_of(&found), ["S"]);
    assert_eq!(found[0].placement_note, note("S", "Verweise im Kreis"));
}

#[test]
fn below_the_depth_limit_each_part_once() {
    // Kette L0 … L32, dann L32 → M (zu tief), M enthält Y zweimal: Y genau einmal.
    let mut g = Gen::new();
    let chain: Vec<Part> = (0..=MAX_ASSEMBLY_DEPTH)
        .map(|i| g.part(&format!("L{i}"), None))
        .collect();
    for w in chain.windows(2) {
        let t = g.shift([1.0, 0.0, 0.0]);
        g.place(w[0], w[1], t);
    }
    let m = g.part("M", None);
    let y = g.part("Y", Some("x"));
    let t = g.shift([0.0; 3]);
    g.place(chain[MAX_ASSEMBLY_DEPTH], m, t);
    for _ in 0..2 {
        let t = g.shift([0.0; 3]);
        g.place(m, y, t);
    }
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["Y"]);
    assert_eq!(found[0].placement_note, note("Y", "zu tief verschachtelt"));
}

#[test]
fn broken_entries_hit_only_what_they_touch() {
    // (a) kaputte, unbeteiligte Darstellung und (b) unbeteiligte Beziehung ins Leere.
    let mut g = Gen::new();
    g.add("SHAPE_REPRESENTATION('kaputt')");
    g.add("SHAPE_REPRESENTATION_RELATIONSHIP('','',#99998,#99999)");
    // … und eine Beziehung von einer Darstellung ohne Körper ins Leere.
    let u = g.part("U", None);
    let t = g.shift([0.0; 3]);
    let idt = g.idt(u.axis, t);
    g.relation(u.rep, 99995, Some(idt));
    let r = g.part("R", None);
    let p = g.part("P", Some("x"));
    let t = g.shift([0.0, 0.0, 2.0]);
    g.place(r, p, t);
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["P"]);
    assert_eq!(found[0].placement_note, None);
    assert_eq!(
        found[0].placement,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 2.0))
    );

    // (c) Das Vorkommen hinter dem CDSR fehlt: Körper am Ursprung, Meldung.
    let mut g = Gen::new();
    let r = g.part("R", None);
    let p = g.part("P", Some("x"));
    let t = g.shift([0.0, 0.0, 2.0]);
    let idt = g.idt(p.axis, t);
    let pds = g.add("PRODUCT_DEFINITION_SHAPE('','',#99997)");
    let rr = g.relation(p.rep, r.rep, Some(idt));
    g.add(&format!(
        "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{rr},#{pds})"
    ));
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["P"]);
    let n = found[0].placement_note.clone().unwrap_or_default();
    assert!(
        n.starts_with("Die Lage von „P“ ließ sich nicht bestimmen (Richtung unklar – Zeile ")
            && n.contains("#99997")
            && n.ends_with("); der Körper liegt am Ursprung."),
        "{n}"
    );
    assert_eq!(found[0].placement, DMat4::IDENTITY);

    // (d) Beziehung eines Körpers mit fehlendem Elternteil: Datei abgelehnt.
    let mut g = Gen::new();
    let p = g.part("P", Some("x"));
    let t = g.shift([0.0; 3]);
    let idt = g.idt(p.axis, t);
    g.relation(p.rep, 99996, Some(idt));
    let err = bodies(&parse(&g.text())).expect_err("abgelehnt");
    assert!(
        matches!(err, StepReadError::MissingRef { target: 99996, .. }),
        "{err}"
    );

    // (e) Kaputte Darstellung und ein freier Volumen-Eintrag: Er könnte in ihr stehen → Meldung.
    let mut g = Gen::new();
    let bad = g.add("SHAPE_REPRESENTATION('kaputt')");
    g.solid("Frei");
    let text = g.text();
    let file = parse(&text);
    let found = bodies(&file).expect("Körper");
    assert_eq!(names_of(&found), ["Frei"]);
    let line = file.line(bad).expect("Zeile");
    assert_eq!(
        found[0].placement_note,
        note(
            "Frei",
            &format!("Zeile {line}: Erwartet wird eine Darstellung")
        )
    );
}

#[test]
fn transformation_items_follow_their_representations() {
    // Lagen gekreuzt zur Beziehung geschrieben: IDT(Ziel in der Baugruppe, Lage im Teil),
    // Beziehung (Teil, Baugruppe). Die Zugehörigkeit entscheidet.
    let mut g = Gen::new();
    let target = skew_target(&mut g, 10.0);
    let r_axis = g.shift([0.0; 3]);
    let r = g.part_with("R", None, MM, r_axis, &[target]);
    let c = skew_child(&mut g);
    let k = g.part_with("K", Some("x"), MM, c, &[]);
    let idt = g.idt(target, c);
    g.place_op(r, k, idt);
    // Widerspruch: beide Lagen gehören zum Teil.
    let c2 = g.shift([0.0; 3]);
    let other = g.shift([1.0, 0.0, 0.0]);
    let w = g.part_with("W", Some("x"), MM, c2, &[other]);
    let idt = g.idt(c2, other);
    g.place_op(r, w, idt);
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["K", "W"]);
    assert_eq!(found[0].placement_note, None);
    check_placement(found[0].placement, skew_expected(10.0), "K");
    assert_eq!(found[1].placement_note, note("W", "Richtung unklar"));
    assert_eq!(found[1].placement, DMat4::IDENTITY);
}

#[test]
fn second_representation_of_a_part_follows_the_first() {
    // Teil mit zwei SHAPE_DEFINITION_REPRESENTATION: die platzierte SR (Lage) und die ABSR mit
    // dem Körper, ohne Beziehung dazwischen.
    let mut g = Gen::new();
    let target = skew_target(&mut g, 10.0);
    let r_axis = g.shift([0.0; 3]);
    let r = g.part_with("R", None, MM, r_axis, &[target]);
    let c = skew_child(&mut g);
    let k = g.part_with("K", None, MM, c, &[]);
    let s = g.solid("x");
    let ab = g.rep("Volumen", &[s], MM);
    let pds = g.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{})", k.pd));
    g.add(&format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{ab})"));
    g.place(r, k, target);
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["K"]);
    assert_eq!(found[0].placement_note, None);
    check_placement(found[0].placement, skew_expected(10.0), "K");
}

#[test]
fn mapped_item_and_plain_occurrence_give_one_body() {
    for hoisted in [false, true] {
        let mut g = Gen::new();
        let teil = g.part("Teil", Some("x"));
        let target = g.shift([100.0, 0.0, 0.0]);
        let item = g.mapped(teil.axis, teil.rep, target);
        let b_axis = g.shift([0.0; 3]);
        let b = g.part_with("Baugruppe", None, MM, b_axis, &[item]);
        let nauo = g.nauo(b.pd, teil.pd);
        let cdsr = g.cdsr_plain(nauo, teil.rep, b.rep);
        let mut text = g.text();
        if hoisted {
            // Das Vorkommen steht vor allen anderen Einträgen.
            text = hoist(&text, &[cdsr - 2, cdsr - 1, cdsr, nauo]);
        }
        let found = bodies(&parse(&text)).expect("Körper");
        assert_eq!(names_of(&found), ["Teil"], "vorgezogen: {hoisted}");
        assert_eq!(found[0].placement_note, None);
        assert_eq!(
            found[0].placement,
            DMat4::from_translation(DVec3::new(100.0, 0.0, 0.0)),
            "vorgezogen: {hoisted}"
        );
    }
}

#[test]
fn same_solid_in_linked_representations_once() {
    let mut g = Gen::new();
    let s = g.solid("x");
    let axis = g.shift([0.0; 3]);
    let sr = g.rep("Teil", &[axis, s], MM);
    g.product("Teil", sr);
    let ab = g.rep("Volumen", &[s], MM);
    g.relation(sr, ab, None);
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["Teil"]);
    assert_eq!(found[0].items, [s]);
}

#[test]
fn two_occurrences_on_one_relationship() {
    // Zwei Vorkommen (NAUO + CDSR) teilen sich eine Beziehung: je Vorkommen ein Körper.
    let mut g = Gen::new();
    let r = g.part("R", None);
    let p = g.part("P", Some("x"));
    let t = g.shift([0.0, 0.0, 2.0]);
    let idt = g.idt(p.axis, t);
    let rr = g.relation(p.rep, r.rep, Some(idt));
    for _ in 0..2 {
        let nauo = g.nauo(r.pd, p.pd);
        let pds = g.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{nauo})"));
        g.add(&format!(
            "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{rr},#{pds})"
        ));
    }
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(names_of(&found), ["P", "P (2)"]);
    for b in &found {
        assert_eq!(
            b.placement,
            DMat4::from_translation(DVec3::new(0.0, 0.0, 2.0))
        );
    }
}

#[test]
fn unsupported_kinds_are_named() {
    let mut g = Gen::new();
    let shell = g.add("CLOSED_SHELL('',())");
    g.add(&format!("FACETED_BREP('',#{shell})"));
    g.add(&format!("FACETED_BREP('',#{shell})"));
    let open = g.add("OPEN_SHELL('',())");
    g.add(&format!("SHELL_BASED_SURFACE_MODEL('',(#{open}))"));
    g.solid("x");
    let file = parse(&g.text());
    assert_eq!(
        unsupported_kinds(&file),
        [
            "Die Datei enthält 2 Facettenkörper (FACETED_BREP); diese Art wird noch nicht \
             unterstützt und ausgelassen.",
            "Die Datei enthält 1 Flächenmodell (SHELL_BASED_SURFACE_MODEL); diese Art wird noch \
             nicht unterstützt und ausgelassen.",
        ]
    );
    assert!(unsupported_kinds(&parse(&data("box_ap214.step"))).is_empty());
}

/// Schlussprüfung M3: Nicht ganz senkrechte Achsen eines Operators projiziert ISO 10303-42
/// (`first_proj_axis`, `second_proj_axis`); bis zum Kosinus 1e-4 tun wir das auch (Dateien mit
/// sechsstelligen Richtungen), darüber Meldung „schiefe Achsen“, Körper am Ursprung.
#[test]
fn slightly_oblique_axes_are_projected() {
    let mut g = Gen::new();
    let r = g.part("R", None);
    let z = Some([0.0, 0.0, 1.0]);
    let ops = [
        g.cto(Some([1.0, 0.0, 1e-7]), None, [0.0, 0.0, 3.0], 1.0, z),
        g.cto(Some([1.0, 0.0, 1e-5]), None, [0.0, 0.0, 3.0], 1.0, z),
        g.cto(
            Some([1.0, 0.0, 0.0]),
            Some([1e-5, 1.0, 0.0]),
            [0.0, 0.0, 3.0],
            1.0,
            z,
        ),
        g.cto(Some([1.0, 0.0, 1e-3]), None, [0.0, 0.0, 3.0], 1.0, z),
        g.cto(
            Some([1.0, 0.0, 0.0]),
            Some([1e-3, 1.0, 0.0]),
            [0.0, 0.0, 3.0],
            1.0,
            z,
        ),
    ];
    let parts: Vec<Part> = (1..=ops.len())
        .map(|i| g.part(&format!("S{i}"), Some("x")))
        .collect();
    for (p, op) in parts.iter().zip(&ops) {
        g.place_op(r, *p, *op);
    }
    let found = bodies(&parse(&g.text())).expect("Körper");
    assert_eq!(found.len(), 5);
    for b in &found[..3] {
        assert_eq!(b.placement_note, None, "{}", b.name);
        check_placement(
            b.placement,
            [[0.0, 0.0, 3.0], [1.0, 0.0, 3.0], [0.0, 1.0, 3.0]],
            &b.name,
        );
    }
    assert_eq!(found[3].placement_note, note("S4", "schiefe Achsen"));
    assert_eq!(found[4].placement_note, note("S5", "schiefe Achsen"));
}
