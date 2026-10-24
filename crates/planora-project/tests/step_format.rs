//! Dateiformat 10: STEP-Körper samt normalisiertem STEP-Text in der Versionsdatei
//! (Teilprojekt STEP-Import, Aufgabe 12). Format 10 nur mit STEP-Körpern, alte Dateien Byte für
//! Byte wie bisher, Rundreise mit Text und Genauigkeit, Skizze auf einer importierten Fläche
//! nach dem Öffnen, feindliche Daten mit deutscher Meldung, ein kaputter Körpertext lädt (P14).

use planora_feature::*;
use planora_kernel::{BoolOp, Solid};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_project::docstore::{DocStore, LockMode, VERSIONS_DIR};
use planora_project::fsutil::new_id;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::Sketch;
use serde_json::{Value, json};
use std::sync::Arc;

// ---- Bausteine ----------------------------------------------------------------------------

/// Zahl in STEP-Schreibweise (immer mit Punkt).
fn real(x: f64) -> String {
    let s = format!("{x:?}");
    if s.contains('.') || s.contains('e') {
        s
    } else {
        format!("{s}.")
    }
}

/// Selbst geschriebener STEP-Text eines achsparallelen Quaders (Millimeter, Einheitslage, ein
/// Körper): Ecke `lo`, Kantenlängen `size`. Flächen in dieser Reihenfolge (Marken „STEP-Fläche
/// n“): 1 Boden, 2 Deckel, 3 vorn (y = lo), 4 rechts (x = hi), 5 hinten (y = hi), 6 links.
fn box_text(name: &str, lo: [f64; 3], size: [f64; 3]) -> String {
    let [x0, y0, z0] = lo;
    let [sx, sy, sz] = size;
    let (x1, y1, z1) = (x0 + sx, y0 + sy, z0 + sz);
    let p = |x: f64, y: f64, z: f64| format!("({},{},{})", real(x), real(y), real(z));
    let (a, b, c) = (real(sx), real(sy), real(sz));
    format!(
        "ISO-10303-21;
/* Selbst geschrieben fuer die Tests von PlanoraCAD (GPL-3.0-only). */
HEADER;
FILE_DESCRIPTION(('Quader'),'2;1');
FILE_NAME('quader.step','2026-10-21T00:00:00',(''),(''),'von Hand','von Hand','');
FILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 1 1 1 1 }}'));
ENDSEC;
DATA;
#1=APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#2);
#2=APPLICATION_CONTEXT('core data for automotive mechanical design processes');
#3=SHAPE_DEFINITION_REPRESENTATION(#4,#10);
#4=PRODUCT_DEFINITION_SHAPE('','',#5);
#5=PRODUCT_DEFINITION('design','',#6,#9);
#6=PRODUCT_DEFINITION_FORMATION('','',#7);
#7=PRODUCT('{name}','{name}','',(#8));
#8=PRODUCT_CONTEXT('',#2,'mechanical');
#9=PRODUCT_DEFINITION_CONTEXT('part definition',#2,'design');
#10=ADVANCED_BREP_SHAPE_REPRESENTATION('{name}',(#11,#20),#105);
#11=AXIS2_PLACEMENT_3D('',#150,#301,#302);
#20=MANIFOLD_SOLID_BREP('{name}',#21);
#21=CLOSED_SHELL('',(#30,#31,#32,#33,#34,#35));
#30=ADVANCED_FACE('',(#40),#60,.F.);
#31=ADVANCED_FACE('',(#41),#61,.T.);
#32=ADVANCED_FACE('',(#42),#62,.T.);
#33=ADVANCED_FACE('',(#43),#63,.T.);
#34=ADVANCED_FACE('',(#44),#64,.T.);
#35=ADVANCED_FACE('',(#45),#65,.F.);
#40=FACE_OUTER_BOUND('',#50,.T.);
#41=FACE_OUTER_BOUND('',#51,.F.);
#42=FACE_OUTER_BOUND('',#52,.T.);
#43=FACE_OUTER_BOUND('',#53,.T.);
#44=FACE_OUTER_BOUND('',#54,.T.);
#45=FACE_OUTER_BOUND('',#55,.T.);
#50=EDGE_LOOP('',(#200,#201,#202,#203));
#51=EDGE_LOOP('',(#204,#205,#206,#207));
#52=EDGE_LOOP('',(#208,#209,#210,#211));
#53=EDGE_LOOP('',(#212,#213,#214,#215));
#54=EDGE_LOOP('',(#216,#217,#218,#219));
#55=EDGE_LOOP('',(#220,#221,#222,#223));
#60=PLANE('',#310);
#61=PLANE('',#311);
#62=PLANE('',#312);
#63=PLANE('',#313);
#64=PLANE('',#314);
#65=PLANE('',#315);
#100=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));
#101=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));
#102=(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT());
#103=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),#100,'distance_accuracy_value','');
#105=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#103))GLOBAL_UNIT_ASSIGNED_CONTEXT((#100,#101,#102))REPRESENTATION_CONTEXT('',''));
#121=EDGE_CURVE('',#141,#142,#161,.T.);
#122=EDGE_CURVE('',#142,#143,#162,.T.);
#123=EDGE_CURVE('',#143,#144,#163,.F.);
#124=EDGE_CURVE('',#144,#141,#164,.T.);
#125=EDGE_CURVE('',#145,#146,#165,.T.);
#126=EDGE_CURVE('',#146,#147,#166,.T.);
#127=EDGE_CURVE('',#147,#148,#167,.T.);
#128=EDGE_CURVE('',#148,#145,#168,.T.);
#129=EDGE_CURVE('',#141,#145,#169,.T.);
#130=EDGE_CURVE('',#142,#146,#170,.T.);
#131=EDGE_CURVE('',#143,#147,#171,.T.);
#132=EDGE_CURVE('',#144,#148,#172,.T.);
#141=VERTEX_POINT('',#151);
#142=VERTEX_POINT('',#152);
#143=VERTEX_POINT('',#153);
#144=VERTEX_POINT('',#154);
#145=VERTEX_POINT('',#155);
#146=VERTEX_POINT('',#156);
#147=VERTEX_POINT('',#157);
#148=VERTEX_POINT('',#158);
#150=CARTESIAN_POINT('',(0.,0.,0.));
#151=CARTESIAN_POINT('',{p1});
#152=CARTESIAN_POINT('',{p2});
#153=CARTESIAN_POINT('',{p3});
#154=CARTESIAN_POINT('',{p4});
#155=CARTESIAN_POINT('',{p5});
#156=CARTESIAN_POINT('',{p6});
#157=CARTESIAN_POINT('',{p7});
#158=CARTESIAN_POINT('',{p8});
#161=LINE('',#151,#181);
#162=LINE('',#152,#182);
#163=LINE('',#154,#183);
#164=LINE('',#154,#184);
#165=LINE('',#155,#185);
#166=LINE('',#156,#186);
#167=LINE('',#157,#187);
#168=LINE('',#158,#188);
#169=LINE('',#151,#189);
#170=LINE('',#152,#190);
#171=LINE('',#153,#191);
#172=LINE('',#154,#192);
#181=VECTOR('',#302,{a});
#182=VECTOR('',#303,{b});
#183=VECTOR('',#302,{a});
#184=VECTOR('',#305,{b});
#185=VECTOR('',#302,{a});
#186=VECTOR('',#303,{b});
#187=VECTOR('',#304,{a});
#188=VECTOR('',#305,{b});
#189=VECTOR('',#301,{c});
#190=VECTOR('',#301,{c});
#191=VECTOR('',#301,{c});
#192=VECTOR('',#301,{c});
#200=ORIENTED_EDGE('',*,*,#124,.F.);
#201=ORIENTED_EDGE('',*,*,#123,.F.);
#202=ORIENTED_EDGE('',*,*,#122,.F.);
#203=ORIENTED_EDGE('',*,*,#121,.F.);
#204=ORIENTED_EDGE('',*,*,#128,.F.);
#205=ORIENTED_EDGE('',*,*,#127,.F.);
#206=ORIENTED_EDGE('',*,*,#126,.F.);
#207=ORIENTED_EDGE('',*,*,#125,.F.);
#208=ORIENTED_EDGE('',*,*,#121,.T.);
#209=ORIENTED_EDGE('',*,*,#130,.T.);
#210=ORIENTED_EDGE('',*,*,#125,.F.);
#211=ORIENTED_EDGE('',*,*,#129,.F.);
#212=ORIENTED_EDGE('',*,*,#122,.T.);
#213=ORIENTED_EDGE('',*,*,#131,.T.);
#214=ORIENTED_EDGE('',*,*,#126,.F.);
#215=ORIENTED_EDGE('',*,*,#130,.F.);
#216=ORIENTED_EDGE('',*,*,#123,.T.);
#217=ORIENTED_EDGE('',*,*,#132,.T.);
#218=ORIENTED_EDGE('',*,*,#127,.F.);
#219=ORIENTED_EDGE('',*,*,#131,.F.);
#220=ORIENTED_EDGE('',*,*,#124,.T.);
#221=ORIENTED_EDGE('',*,*,#129,.T.);
#222=ORIENTED_EDGE('',*,*,#128,.F.);
#223=ORIENTED_EDGE('',*,*,#132,.F.);
#301=DIRECTION('',(0.,0.,1.));
#302=DIRECTION('',(1.,0.,0.));
#303=DIRECTION('',(0.,1.,0.));
#304=DIRECTION('',(-1.,0.,0.));
#305=DIRECTION('',(0.,-1.,0.));
#310=AXIS2_PLACEMENT_3D('',#151,#301,#302);
#311=AXIS2_PLACEMENT_3D('',#155,#301,#302);
#312=AXIS2_PLACEMENT_3D('',#151,#305,#302);
#313=AXIS2_PLACEMENT_3D('',#152,#302,#303);
#314=AXIS2_PLACEMENT_3D('',#154,#303,#304);
#315=AXIS2_PLACEMENT_3D('',#151,#302,#301);
ENDSEC;
END-ISO-10303-21;
",
        p1 = p(x0, y0, z0),
        p2 = p(x1, y0, z0),
        p3 = p(x1, y1, z0),
        p4 = p(x0, y1, z0),
        p5 = p(x0, y0, z1),
        p6 = p(x1, y0, z1),
        p7 = p(x1, y1, z1),
        p8 = p(x0, y1, z1),
    )
}

/// Quader 10 × 20 × 30 am Ursprung.
fn quader_text() -> String {
    box_text("Quader", [0.0, 0.0, 0.0], [10.0, 20.0, 30.0])
}

/// Würfel 5 × 5 × 5 bei (50, 0, 0).
fn wuerfel_text() -> String {
    box_text("Wuerfel", [50.0, 0.0, 0.0], [5.0, 5.0, 5.0])
}

/// Text mit gültigem Kopf und Ende, aber ohne Körper.
fn broken_text() -> String {
    let text = quader_text();
    let head = &text[..text.find("DATA;").unwrap()];
    format!("{head}DATA;\nENDSEC;\nEND-ISO-10303-21;\n")
}

/// Form wie nach dem Laden: nur der Text, gebaut wird beim Rechnen.
fn shape(text: String, tol: f64) -> Arc<StepShape> {
    Arc::new(StepShape::new(text, tol).unwrap())
}

fn body(name: &str, shape: Arc<StepShape>) -> NewStepBody {
    NewStepBody {
        name: name.into(),
        shape,
    }
}

fn first(id: FeatureId) -> BodyRef {
    BodyRef {
        feature: id,
        index: 0,
    }
}

fn status(d: &Document, id: FeatureId) -> FeatureStatus {
    d.feature(id).unwrap().status.clone()
}

fn volume(s: &Solid) -> f64 {
    let mesh = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 1e-3,
        },
    )
    .unwrap();
    mass_properties(&mesh).volume
}

fn bounds(s: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in s.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    (lo, hi)
}

/// Marken (Herkunft, Rolle) aller Flächen, sortiert.
fn tags(s: &Solid) -> Vec<(u64, String)> {
    let mut t: Vec<(u64, String)> = s
        .faces
        .iter()
        .map(|(_, f)| {
            let tag = f.tag.as_ref().expect("Fläche ohne Marke");
            (tag.origin, tag.role.clone())
        })
        .collect();
    t.sort();
    t
}

fn face_ref(s: &Solid, role: &str) -> FaceRef {
    let (id, _) = s
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role))
        .unwrap_or_else(|| panic!("Fläche „{role}“ fehlt"));
    FaceRef::from_face(s, id, &Tolerance::default()).unwrap()
}

fn edge_ref(s: &Solid, a: &str, b: &str) -> EdgeRef {
    let tol = Tolerance::default();
    s.edges
        .iter()
        .filter_map(|(e, _)| EdgeRef::from_edge(s, e, &tol).ok())
        .find(|r| (r.a.role == a && r.b.role == b) || (r.a.role == b && r.b.role == a))
        .unwrap_or_else(|| panic!("keine Kante zwischen „{a}“ und „{b}“"))
}

fn step_shape_of(d: &Document, id: FeatureId) -> StepShapeId {
    match &d.feature(id).unwrap().kind {
        FeatureKind::StepBody { shape } => *shape,
        other => panic!("kein STEP-Körper: {other:?}"),
    }
}

fn bad(t: &str) -> ProjectError {
    decode_version(t, "x.json").unwrap_err()
}

fn msg(t: &str) -> String {
    let e = bad(t);
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    e.to_string()
}

fn edited(text: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

/// Lesen wie in der Anwendung: Datei lesen, dann `Document::from_snapshot`.
fn load(t: &str) -> Document {
    Document::from_snapshot(decode_version(t, "x.json").unwrap(), 0.05).unwrap()
}

/// Dokument mit zwei STEP-Körpern: „Quader“ (Genauigkeit 1e-6) und „Würfel“ (grob, 1e-3).
fn two_bodies() -> (Document, [FeatureId; 2]) {
    let mut d = Document::new();
    let ids = d
        .add_step_bodies(vec![
            body("Quader", shape(quader_text(), 1e-6)),
            body("Würfel", shape(wuerfel_text(), 1e-3)),
        ])
        .unwrap();
    for id in &ids {
        assert_eq!(status(&d, *id), FeatureStatus::Ok, "{:?}", status(&d, *id));
    }
    (d, [ids[0], ids[1]])
}

const NEW_KEYS: [&str; 2] = ["\"step_formen\"", "step_koerper"];

// ---- (a) Rundreise ------------------------------------------------------------------------

#[test]
fn roundtrip_keeps_text_and_tolerance() {
    assert_eq!(FORMAT_VERSION, 11);
    let (d, [q, w]) = two_bodies();
    let snap = d.snapshot();
    assert_eq!(required_format(&snap), 10);
    let text = encode_version(&snap).unwrap();
    assert!(text.contains("\"format\": 10"), "{}", &text[..40]);
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(back.step_shapes.len(), 2);
    for (id, s) in &snap.step_shapes {
        let b = &back.step_shapes[id];
        assert_eq!(b.text(), s.text(), "Text Zeichen für Zeichen");
        assert_eq!(b.tolerance_mm(), s.tolerance_mm());
        assert_eq!(b.is_coarse(), s.is_coarse());
    }
    assert_eq!(back.step_shapes[&StepShapeId(1)].text(), quader_text());
    assert_eq!(back.step_shapes[&StepShapeId(2)].text(), wuerfel_text());
    assert!(back.step_shapes[&StepShapeId(2)].is_coarse());
    assert!(!back.step_shapes[&StepShapeId(1)].is_coarse());
    for (a, b) in snap.features.iter().zip(&back.features) {
        assert_eq!((a.id, &a.name, &a.kind), (b.id, &b.name, &b.kind));
    }
    assert_eq!((back.marker, back.next_id), (snap.marker, snap.next_id));

    // Über das Dokument: gleiche Marken und Volumina, alle Status Ok, zeichengleich gespeichert.
    let e = Document::from_snapshot(back, 0.05).unwrap();
    for id in [q, w] {
        assert_eq!(status(&e, id), FeatureStatus::Ok);
        let (a, b) = (d.body(first(id)).unwrap(), e.body(first(id)).unwrap());
        assert_eq!(tags(a), tags(b));
        assert_eq!(volume(a), volume(b));
        assert_eq!(bounds(a), bounds(b));
    }
    assert!((volume(e.body(first(q)).unwrap()) - 6000.0).abs() < 1e-6);
    assert!((volume(e.body(first(w)).unwrap()) - 125.0).abs() < 1e-6);
    assert_eq!(step_shape_of(&e, w), StepShapeId(2));
    assert!(e.step_body(w).unwrap().is_coarse());
    assert_eq!(encode_version(&e.snapshot()).unwrap(), text);

    // Gestalt der Datei.
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["features"][1]["art"],
        json!({"typ": "step_koerper", "form": 2})
    );
    assert_eq!(v["step_formen"][0]["id"], 1);
    assert_eq!(v["step_formen"][0]["genauigkeit"], 1e-6);
    assert_eq!(v["step_formen"][1]["genauigkeit"], 1e-3);
    assert_eq!(v["step_formen"][1]["text"], wuerfel_text());
    let keys: Vec<&String> = v["step_formen"][0].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["genauigkeit", "id", "text"]);
}

/// Mehrere Körper aus einer Form (dasselbe `Arc` in zwei Einträgen): der Text steht einmal in
/// der Datei, beide Körper zeigen darauf und laden.
#[test]
fn shared_shape_is_written_once() {
    let mut d = Document::new();
    let s = shape(quader_text(), 1e-6);
    let ids = d
        .add_step_bodies(vec![
            body("A", Arc::clone(&s)),
            body("B", Arc::clone(&s)),
            body("C", shape(wuerfel_text(), 1e-6)),
        ])
        .unwrap();
    let snap = d.snapshot();
    assert_eq!(snap.step_shapes.len(), 3, "drei Kennungen im Dokument");
    let text = encode_version(&snap).unwrap();
    assert_eq!(text.matches("PRODUCT('Quader'").count(), 1, "Text einmal");
    let v: Value = serde_json::from_str(&text).unwrap();
    let forms: Vec<u64> = v["step_formen"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_u64().unwrap())
        .collect();
    assert_eq!(forms, [1, 3]);
    let used: Vec<u64> = v["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["art"]["form"].as_u64().unwrap())
        .collect();
    assert_eq!(used, [1, 1, 3]);
    let e = load(&text);
    for id in &ids {
        assert_eq!(status(&e, *id), FeatureStatus::Ok);
        assert_eq!(
            tags(e.body(first(*id)).unwrap()),
            tags(d.body(first(*id)).unwrap())
        );
    }
    assert!(Arc::ptr_eq(
        e.step_body(ids[0]).unwrap(),
        e.step_body(ids[1]).unwrap()
    ));
    // Erneut geschrieben: zeichengleich.
    assert_eq!(encode_version(&e.snapshot()).unwrap(), text);
}

/// STEP-Körper in Ketten mit Boolean, Verrunden und Formeln; Parameter bleiben; Regel 10 vor 9.
#[test]
fn chains_parameters_and_rule_order() {
    let (mut d, [q, _]) = two_bodies();
    d.add_parameter("r", "2", ParamUnit::Length, "").unwrap();
    let z = d
        .add_feature_with_formulas(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 2.0,
                height: 40.0,
                origin: DVec3::new(5.0, 10.0, -5.0),
            },
            [(FieldKey::Radius, "r".to_string())].into_iter().collect(),
        )
        .unwrap();
    let b = d
        .add_feature(
            "Bohrung",
            FeatureKind::Boolean {
                target: first(q),
                tool: first(z),
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    assert_eq!(status(&d, b), FeatureStatus::Ok, "{:?}", status(&d, b));
    let edge = edge_ref(d.body(first(b)).unwrap(), "STEP-Fläche 2", "STEP-Fläche 3");
    let r = d
        .add_feature(
            "Rundung",
            FeatureKind::Verrunden {
                body: first(b),
                edges: vec![edge],
                art: BlendArt::Rundung { radius: 1.0 },
                tangentenkette: false,
            },
        )
        .unwrap();
    assert_eq!(status(&d, r), FeatureStatus::Ok, "{:?}", status(&d, r));
    let snap = d.snapshot();
    assert_eq!(required_format(&snap), 10, "Regel 10 vor Regel 9");
    let text = encode_version(&snap).unwrap();
    assert!(text.contains("\"format\": 10") && text.contains("\"parameter\""));
    let e = load(&text);
    assert_eq!(e.parameters(), d.parameters());
    for (a, b) in d.features().iter().zip(e.features()) {
        assert_eq!(a.status, b.status, "{}", a.name);
        assert_eq!(a.status, FeatureStatus::Ok, "{}", a.name);
        assert_eq!(a.formulas(), b.formulas(), "{}", a.name);
    }
    let (x, y) = (d.body(first(r)).unwrap(), e.body(first(r)).unwrap());
    assert_eq!(tags(x), tags(y));
    assert_eq!(volume(x), volume(y));
    assert_eq!(encode_version(&e.snapshot()).unwrap(), text);

    // Nur Parameter, kein STEP-Körper: Format 9; nur ein STEP-Körper: Format 10.
    let mut p = Document::new();
    p.add_parameter("r", "2", ParamUnit::Length, "").unwrap();
    assert_eq!(required_format(&p.snapshot()), 9);
    let mut only = Document::new();
    only.add_step_bodies(vec![body("Q", shape(quader_text(), 1e-6))])
        .unwrap();
    assert_eq!(required_format(&only.snapshot()), 10, "nur ein STEP-Körper");
    // Unterdrückt zählt er auch (der Text muss in der Datei stehen).
    let id = only.features()[0].id;
    only.set_suppressed(id, true).unwrap();
    assert_eq!(required_format(&only.snapshot()), 10);
    let text = encode_version(&only.snapshot()).unwrap();
    assert!(text.contains("\"step_formen\""));
    let e = load(&text);
    assert!(e.feature(id).unwrap().suppressed);
}

/// Versionskette, Autosicherung und Wiederherstellen: der Text steht in jeder Version erneut
/// (Spec §4b.5), alte Versionen ohne STEP-Körper bleiben Format 1.
#[test]
fn versions_and_autosave_keep_step_bodies() {
    let p = std::env::temp_dir().join(format!("planora-step-format-{}.pcad", new_id()));
    let (d, _) = two_bodies();
    let text = encode_version(&d.snapshot()).unwrap();
    let mut st = DocStore::create(&p, &Document::new().snapshot(), "").unwrap();
    let v2 = st.save(Some(1), &d.snapshot(), "Import", None).unwrap();
    let v3 = st
        .save(Some(v2), &d.snapshot(), "unverändert", None)
        .unwrap();
    assert_eq!(encode_version(&st.load_version(v2).unwrap()).unwrap(), text);
    for v in [v2, v3] {
        let file = p.join(VERSIONS_DIR).join(format!("{v:04}.json"));
        let on_disk = std::fs::read_to_string(file).unwrap();
        assert_eq!(on_disk, text, "Version {v}: Text je Version erneut");
    }
    st.write_autosave(Some(v3), &d.snapshot()).unwrap();
    let auto = st.read_autosave().unwrap().unwrap();
    assert_eq!(encode_version(&auto.snapshot).unwrap(), text);
    let old = encode_version(&st.load_version(1).unwrap()).unwrap();
    assert!(old.contains("\"format\": 1") && !old.contains("step"));
    let branch = st.info().active_branch.clone();
    let v4 = st.restore(v2, &branch).unwrap();
    assert_eq!(encode_version(&st.load_version(v4).unwrap()).unwrap(), text);
    drop(st);
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    let e = Document::from_snapshot(st.load_version(v3).unwrap(), 0.05).unwrap();
    assert!(e.features().iter().all(|f| f.status == FeatureStatus::Ok));
    assert_eq!(encode_version(&e.snapshot()).unwrap(), text);
    drop(st);
    std::fs::remove_dir_all(&p).unwrap();
}

// ---- (b) Alte Dateien ---------------------------------------------------------------------

#[test]
fn old_files_unchanged() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut n = 0;
    for sub in ["data", "fixtures"] {
        for entry in std::fs::read_dir(root.join(sub)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            let s = decode_version(&text, &name).unwrap();
            assert!(s.step_shapes.is_empty(), "{name}");
            assert!(required_format(&s) <= 9, "{name}");
            let again = encode_version(&s).unwrap();
            assert_eq!(
                again.replace("\r\n", "\n"),
                text.replace("\r\n", "\n"),
                "{name}"
            );
            let doc = Document::from_snapshot(s, 0.05).unwrap();
            assert_eq!(
                encode_version(&doc.snapshot())
                    .unwrap()
                    .replace("\r\n", "\n"),
                text.replace("\r\n", "\n"),
                "{name}"
            );
            for k in NEW_KEYS {
                assert!(!again.contains(k), "{name}: {k}");
            }
            n += 1;
        }
    }
    assert_eq!(n, 11, "alle Golden-Dateien");

    // Ein Dokument ohne STEP-Körper, aber mit Parametern: Format 9 ohne neue Felder.
    let mut d = Document::new();
    d.add_parameter("Breite", "40", ParamUnit::Length, "")
        .unwrap();
    d.add_feature(
        "Quader",
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    assert_eq!(required_format(&d.snapshot()), 9);
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"format\": 9"), "{text}");
    for k in NEW_KEYS {
        assert!(!text.contains(k), "{k}");
    }
    // Eine Datei mit Format 10, aber ohne STEP-Körper, lädt (sie ist nicht „zu neu“).
    let ten = text.replacen("\"format\": 9", "\"format\": 10", 1);
    let s = decode_version(&ten, "x.json").unwrap();
    assert!(s.step_shapes.is_empty());
    assert!(matches!(
        bad(&text.replacen("\"format\": 9", "\"format\": 12", 1)),
        ProjectError::TooNew { found: 12, .. }
    ));
}

// ---- (c) Skizze auf einer importierten Fläche nach dem Öffnen -----------------------------

#[test]
fn reopen_keeps_sketch_on_imported_face() {
    let mut d = Document::new();
    let q = d
        .add_step_bodies(vec![body("Quader", shape(quader_text(), 1e-6))])
        .unwrap()[0];
    let solid = d.body(first(q)).unwrap().clone();
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    sk.add_rectangle(DVec2::new(1.0, 1.0), DVec2::new(3.0, 4.0))
        .unwrap();
    let s = d
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::OnFace(face_ref(&solid, "STEP-Fläche 2")),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let e = d
        .add_feature(
            "Extrusion",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 5.0,
            },
        )
        .unwrap();
    for f in d.features() {
        assert_eq!(f.status, FeatureStatus::Ok, "{}", f.name);
    }
    let hull = bounds(d.body(first(e)).unwrap());
    // Die Extrusion sitzt auf dem Deckel (z = 30) und reicht 5 mm hinaus.
    assert!((hull.0.z - 30.0).abs() < 1e-9 && (hull.1.z - 35.0).abs() < 1e-9);

    let text = encode_version(&d.snapshot()).unwrap();
    let p = std::env::temp_dir().join(format!("planora-step-sketch-{}.pcad", new_id()));
    let st = DocStore::create(&p, &decode_version(&text, "x.json").unwrap(), "").unwrap();
    drop(st);
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    let r = Document::from_snapshot(st.load_version(1).unwrap(), 0.05).unwrap();
    drop(st);
    std::fs::remove_dir_all(&p).unwrap();
    for f in r.features() {
        assert_eq!(f.status, FeatureStatus::Ok, "{}", f.name);
    }
    let again = bounds(r.body(first(e)).unwrap());
    assert!((again.0 - hull.0).abs().max_element() <= 1e-9, "{again:?}");
    assert!((again.1 - hull.1).abs().max_element() <= 1e-9, "{again:?}");
    assert_eq!(
        tags(r.body(first(q)).unwrap()),
        tags(d.body(first(q)).unwrap())
    );
    match &r.feature(s).unwrap().kind {
        FeatureKind::Sketch(sf) => match &sf.plane {
            SketchPlane::OnFace(f) => {
                assert_eq!((f.origin, f.role.as_str()), (q.0, "STEP-Fläche 2"))
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

// ---- (d) Feindliche Daten -----------------------------------------------------------------

/// Gültige Datei mit einem STEP-Körper „Q“ (Form 1).
fn one_body_text() -> String {
    let mut d = Document::new();
    d.add_step_bodies(vec![body("Q", shape(quader_text(), 1e-6))])
        .unwrap();
    encode_version(&d.snapshot()).unwrap()
}

#[test]
fn hostile_step_data() {
    let good = one_body_text();
    const OLD: &str = "STEP-Körper gibt es erst ab Dateiformat 10";
    // Format 9 mit Formen, mit einem STEP-Körper, mit beidem.
    let nine = |f: &dyn Fn(&mut Value)| {
        edited(&good, |v| {
            v["format"] = json!(9);
            f(v);
        })
    };
    let m = msg(&nine(&|_| {}));
    assert!(m.contains(OLD), "{m}");
    let m = msg(&nine(&|v| {
        v["features"] = json!([]);
    }));
    assert!(m.contains(OLD), "nur step_formen: {m}");
    let m = msg(&nine(&|v| {
        v.as_object_mut().unwrap().remove("step_formen");
    }));
    assert!(m.contains(OLD), "nur der Körper: {m}");
    let m = msg(&edited(&good, |v| v["format"] = json!(3)));
    assert!(m.contains(OLD), "{m}");

    // Text „hallo“: Kopf ungültig.
    let m = msg(&edited(&good, |v| {
        v["step_formen"][0]["text"] = json!("hallo")
    }));
    assert!(
        m.contains("STEP-Form 1: Der Text hat nicht Anfang und Ende einer STEP-Datei."),
        "{m}"
    );
    // Abgeschnitten: Ende fehlt.
    let cut = quader_text()[..quader_text().len() / 2].to_string();
    let m = msg(&edited(&good, |v| v["step_formen"][0]["text"] = json!(cut)));
    assert!(m.contains("STEP-Form 1: Der Text hat nicht Anfang"), "{m}");
    // Zeichen außerhalb von ASCII.
    let umlaut = quader_text().replace("'Quader'", "'Würfel'");
    let m = msg(&edited(&good, |v| {
        v["step_formen"][0]["text"] = json!(umlaut)
    }));
    assert!(
        m.contains("STEP-Form 1: Der Text enthält Zeichen außerhalb von ASCII."),
        "{m}"
    );
    // Doppelte Kennung.
    let m = msg(&edited(&good, |v| {
        let f = v["step_formen"][0].clone();
        v["step_formen"].as_array_mut().unwrap().push(f);
    }));
    assert!(
        m.contains("STEP-Form 1: Die Kennung kommt doppelt vor"),
        "{m}"
    );
    // Doppelte Kennung mit anderem Text: nie still überschrieben.
    let m = msg(&edited(&good, |v| {
        let mut f = v["step_formen"][0].clone();
        f["text"] = json!(wuerfel_text());
        v["step_formen"].as_array_mut().unwrap().insert(0, f);
    }));
    assert!(m.contains("Die Kennung kommt doppelt vor"), "{m}");
    // Kennung 0.
    let m = msg(&edited(&good, |v| {
        v["step_formen"][0]["id"] = json!(0);
        v["features"][0]["art"]["form"] = json!(0);
    }));
    assert!(m.contains("STEP-Form 0: Die Kennung ist ungültig"), "{m}");
    // Fehlende Form.
    let m = msg(&edited(&good, |v| {
        v["features"][0]["art"]["form"] = json!(7)
    }));
    assert!(m.contains("„Q“: Die STEP-Form 7 fehlt in der Datei"), "{m}");
    let m = msg(&edited(&good, |v| {
        v.as_object_mut().unwrap().remove("step_formen");
    }));
    assert!(m.contains("„Q“: Die STEP-Form 1 fehlt in der Datei"), "{m}");
    // Genauigkeit außerhalb 1e-6 … 1e-3, falscher Typ.
    for g in [json!(-1.0), json!(0.0), json!(2e-3), json!(1e-7)] {
        let m = msg(&edited(&good, |v| {
            v["step_formen"][0]["genauigkeit"] = g.clone()
        }));
        assert!(
            m.contains("STEP-Form 1: Die Genauigkeit muss zwischen 0,000001 und 0,001 mm liegen."),
            "{g}: {m}"
        );
    }
    for g in [json!(null), json!("0.001")] {
        msg(&edited(&good, |v| {
            v["step_formen"][0]["genauigkeit"] = g.clone()
        }));
    }
    msg(&edited(&good, |v| {
        v["step_formen"][0].as_object_mut().unwrap().remove("text");
    }));
    msg(&edited(&good, |v| {
        v["features"][0]["art"]
            .as_object_mut()
            .unwrap()
            .remove("form");
    }));
    msg(&edited(&good, |v| v["step_formen"] = json!("ganz anders")));
    // Zu neu.
    let e = bad(&good.replacen("\"format\": 10", "\"format\": 12", 1));
    assert!(matches!(e, ProjectError::TooNew { found: 12, .. }), "{e:?}");

    // Gültiger Kopf, kaputter Körper: die Datei lädt, nur der STEP-Körper hat den Fehler (P14).
    let broken = edited(&good, |v| {
        v["step_formen"][0]["text"] = json!(broken_text())
    });
    let d = load(&broken);
    let q = d.features()[0].id;
    match status(&d, q) {
        FeatureStatus::Error(m) => assert!(
            m.starts_with("Die gespeicherte STEP-Form lässt sich nicht lesen: "),
            "{m}"
        ),
        other => panic!("Fehlerstatus erwartet: {other:?}"),
    }
    // Verstümmelte Mitte (Kopf und Ende heil): lädt ebenfalls mit Fehlerstatus.
    let mut garbled = quader_text();
    let at = garbled.find("#121=").unwrap();
    garbled.replace_range(at..at + 200, &"x".repeat(200));
    let d = load(&edited(&good, |v| {
        v["step_formen"][0]["text"] = json!(garbled)
    }));
    assert!(matches!(status(&d, q), FeatureStatus::Error(_)));
    // Gespeichert wird derselbe (kaputte) Text wieder, nichts geht verloren.
    let d = load(&broken);
    let again = decode_version(&encode_version(&d.snapshot()).unwrap(), "x.json").unwrap();
    assert_eq!(again.step_shapes[&StepShapeId(1)].text(), broken_text());
    // Eine unbenutzte Form in der Datei stört nicht; geschrieben wird sie nicht wieder.
    let extra = edited(&good, |v| {
        let mut f = v["step_formen"][0].clone();
        f["id"] = json!(5);
        f["text"] = json!(wuerfel_text());
        v["step_formen"].as_array_mut().unwrap().push(f);
    });
    let d = load(&extra);
    assert_eq!(status(&d, q), FeatureStatus::Ok);
    assert_eq!(encode_version(&d.snapshot()).unwrap(), good);
}

/// Summe der Textlängen über 200 MiB: abgelehnt, bevor eine Form angelegt wird.
#[test]
fn step_texts_are_limited_in_sum() {
    const MIB: usize = 1024 * 1024;
    let filler = |n: usize| {
        let head = "ISO-10303-21;\n";
        let end = "END-ISO-10303-21;\n";
        let mut t = String::with_capacity(n);
        t.push_str(head);
        t.push_str(&" ".repeat(n - head.len() - end.len()));
        t.push_str(end);
        assert_eq!(t.len(), n);
        t
    };
    let file = |sizes: &[usize]| {
        let forms: Vec<String> = sizes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                format!(
                    "{{\"id\": {}, \"genauigkeit\": 0.000001, \"text\": {}}}",
                    i + 1,
                    serde_json::to_string(&filler(*n)).unwrap()
                )
            })
            .collect();
        let features: Vec<String> = (1..=sizes.len())
            .map(|i| {
                format!(
                    "{{\"id\": {i}, \"name\": \"K{i}\", \"unterdrueckt\": false, \
                     \"art\": {{\"typ\": \"step_koerper\", \"form\": {i}}}}}"
                )
            })
            .collect();
        format!(
            "{{\"format\": 10, \"marker\": {}, \"naechste_id\": {}, \"features\": [{}], \
             \"step_formen\": [{}]}}",
            sizes.len(),
            sizes.len() + 1,
            features.join(","),
            forms.join(",")
        )
    };
    // Genau 200 MiB in vier Formen: gelesen (gebaut wird beim Lesen nicht).
    let at = file(&[50 * MIB, 50 * MIB, 50 * MIB, 50 * MIB]);
    let s = decode_version(&at, "x.json").unwrap();
    assert_eq!(s.step_shapes.len(), 4);
    drop(s);
    drop(at);
    // Ein Byte mehr: abgelehnt.
    let over = file(&[50 * MIB, 50 * MIB, 50 * MIB, 50 * MIB + 1]);
    let m = msg(&over);
    assert!(m.contains("Die STEP-Formen sind zu groß"), "{m}");
}

// ---- (e) Schreiben baut nicht -------------------------------------------------------------

#[test]
fn encode_does_not_build() {
    let (d, [q, _]) = two_bodies();
    let mut snap = d.snapshot();
    // Formen frisch aus dem Text (noch nicht gebaut), dazu eine unbenutzte Form.
    for s in snap.step_shapes.values_mut() {
        *s = shape(s.text().to_string(), s.tolerance_mm());
    }
    snap.step_shapes.insert(
        StepShapeId(9),
        shape(box_text("X", [0.0; 3], [1.0; 3]), 1e-6),
    );
    let builds = step_builds();
    let text = encode_version(&snap).unwrap();
    assert_eq!(step_builds(), builds, "Schreiben baut keinen Körper");
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(step_builds(), builds, "Lesen baut keinen Körper");
    assert_eq!(
        back.step_shapes.keys().copied().collect::<Vec<_>>(),
        [StepShapeId(1), StepShapeId(2)],
        "unbenutzte Form nicht geschrieben"
    );
    assert!(!text.contains("PRODUCT('X'"));
    assert_eq!(encode_version(&d.snapshot()).unwrap(), text);
    // Laden baut erst beim Rechnen (`from_snapshot`), einmal je Form.
    let e = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(step_builds(), builds + 2);
    assert_eq!(status(&e, q), FeatureStatus::Ok);
}

// ---- Fix-Runde 1: Summengrenze schon beim Import, Genauigkeit je Form, Goldene Datei ------

/// Quader 5 × 5 × 5 bei (x, 0, 0) als Text von genau `len` Bytes (mit Leerzeichen-Kommentar
/// nach der ersten Zeile aufgefüllt).
fn padded_box(name: &str, x: f64, len: usize) -> String {
    let t = box_text(name, [x, 0.0, 0.0], [5.0, 5.0, 5.0]);
    let head = "ISO-10303-21;\n";
    let pad = len - t.len();
    assert!(pad >= 5, "zu kurz: {len}");
    let out = format!("{head}/*{}*/\n{}", " ".repeat(pad - 5), &t[head.len()..]);
    assert_eq!(out.len(), len);
    out
}

/// Vier Formen von genau `n` Bytes, Grenze `4 n` (über `limit`): Import bis zur Grenze geht,
/// darüber wird er ganz abgelehnt; ein Dokument an der Grenze speichert und lädt.
fn import_and_save_at_limit(n: usize, limit: usize) {
    assert_eq!(step_text_sum_limit(), limit);
    let forms: Vec<Arc<StepShape>> = (0..4)
        .map(|i| shape(padded_box(&format!("K{i}"), 10.0 * i as f64, n), 1e-6))
        .collect();
    let mut d = Document::new();
    d.add_step_bodies(
        (0..3)
            .map(|i| body(&format!("K{i}"), Arc::clone(&forms[i])))
            .collect(),
    )
    .unwrap();
    let (before, edits, next) = (d.snapshot(), d.edit_count(), d.snapshot().next_id);
    // Über der Grenze: ganz abgelehnt, das Dokument bleibt unverändert.
    let e = d
        .add_step_bodies(vec![
            body("K3", Arc::clone(&forms[3])),
            body("X", shape(quader_text(), 1e-6)),
        ])
        .unwrap_err();
    let m = e.to_string();
    assert!(
        m.starts_with("Die STEP-Formen im Projekt sind zusammen zu groß (")
            && m.ends_with("). Bitte weniger oder kleinere Körper importieren."),
        "{m}"
    );
    assert!(m.contains(", erlaubt "), "{m}");
    assert!(d.snapshot().same_content(&before));
    assert_eq!((d.edit_count(), d.snapshot().next_id), (edits, next));
    assert_eq!(d.snapshot().step_shapes.len(), 3);
    // Genau an der Grenze: angenommen; dieselbe Form noch einmal zählt nicht doppelt.
    d.add_step_bodies(vec![body("K3", Arc::clone(&forms[3]))])
        .unwrap();
    d.add_step_bodies(vec![body("K3b", Arc::clone(&forms[3]))])
        .unwrap();
    assert!(d.features().iter().all(|f| f.status == FeatureStatus::Ok));
    // Das Dokument an der Grenze speichert und lädt.
    let text = encode_version(&d.snapshot()).unwrap();
    let e = load(&text);
    assert_eq!(e.features().len(), 5);
    assert!(e.features().iter().all(|f| f.status == FeatureStatus::Ok));
    assert_eq!(encode_version(&e.snapshot()).unwrap(), text);
    // Lesen benutzt dieselbe Grenze: ein Byte weniger erlaubt, und die Datei ist zu groß.
    {
        let _smaller = limit_step_text_sum_for_tests(limit - 1);
        let m = msg(&text);
        assert!(m.contains("Die STEP-Formen sind zu groß"), "{m}");
    }
    assert_eq!(step_text_sum_limit(), limit);

    // Ein Bild über der Grenze (am Import vorbei): eigene, verständliche Meldung beim Speichern.
    let mut s = d.snapshot();
    s.step_shapes
        .insert(StepShapeId(99), shape(quader_text(), 1e-6));
    s.features.push(SnapshotFeature {
        id: FeatureId(99),
        name: "Zu viel".into(),
        suppressed: false,
        kind: FeatureKind::StepBody {
            shape: StepShapeId(99),
        },
        formulas: Default::default(),
    });
    s.next_id = 100;
    let m = encode_version(&s).unwrap_err().to_string();
    assert!(
        m.starts_with(
            "Das Projekt lässt sich nicht speichern: Die STEP-Formen sind zusammen zu groß ("
        ) && m.ends_with("). Bitte STEP-Körper löschen oder durch kleinere ersetzen."),
        "{m}"
    );
    assert!(!m.contains("ungültige Werte"), "{m}");
}

#[test]
fn import_refuses_more_step_text_than_can_be_saved() {
    let n = 16 * 1024;
    let _limit = limit_step_text_sum_for_tests(4 * n);
    import_and_save_at_limit(n, 4 * n);
}

/// Dasselbe mit der echten Grenze von 200 MiB (vier Formen à 50 MiB); nur im Release-Build.
#[test]
#[ignore = "groß: cargo test --release -p planora-project --test step_format -- --ignored"]
fn real_step_text_limit() {
    assert_eq!(MAX_STEP_TEXT_SUM_BYTES, 200 * 1024 * 1024);
    import_and_save_at_limit(50 * 1024 * 1024, MAX_STEP_TEXT_SUM_BYTES);
    let mut d = Document::new();
    let big = shape(padded_box("G", 0.0, 50 * 1024 * 1024), 1e-6);
    let small = shape(quader_text(), 1e-6);
    let e = d
        .add_step_bodies(vec![
            body("A", Arc::clone(&big)),
            body("B", shape(padded_box("H", 10.0, 50 * 1024 * 1024), 1e-6)),
            body("C", shape(padded_box("I", 20.0, 50 * 1024 * 1024), 1e-6)),
            body("D", shape(padded_box("J", 30.0, 50 * 1024 * 1024), 1e-6)),
            body("E", small),
        ])
        .unwrap_err();
    assert!(
        e.to_string().starts_with(
            "Die STEP-Formen im Projekt sind zusammen zu groß (200,1 MB, erlaubt 200 MB)."
        ),
        "{e}"
    );
    assert!(d.features().is_empty());
}

/// Zwei getrennte Importe mit gleichem Text, aber anderer Genauigkeit bleiben zwei Formen;
/// beide Genauigkeiten (und damit die Warnung „grob“) überstehen Speichern und Laden.
#[test]
fn equal_text_with_other_accuracy_stays_apart() {
    let mut d = Document::new();
    let a = d
        .add_step_bodies(vec![body("Fein", shape(quader_text(), 1e-6))])
        .unwrap()[0];
    let b = d
        .add_step_bodies(vec![body("Grob", shape(quader_text(), 1e-3))])
        .unwrap()[0];
    let text = encode_version(&d.snapshot()).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["step_formen"].as_array().unwrap().len(), 2);
    let e = load(&text);
    assert_eq!(e.step_body(a).unwrap().tolerance_mm(), 1e-6);
    assert_eq!(e.step_body(b).unwrap().tolerance_mm(), 1e-3);
    assert!(!e.step_body(a).unwrap().is_coarse());
    assert!(e.step_body(b).unwrap().is_coarse());
    assert_eq!(encode_version(&e.snapshot()).unwrap(), text);
}

/// Mehr als 1 000 000 Formen: abgelehnt, bevor eine einzelne Form geprüft wird.
#[test]
fn too_many_step_forms() {
    let n = 1_000_001;
    let mut forms = String::with_capacity(n * 48);
    for i in 1..=n {
        if i > 1 {
            forms.push(',');
        }
        forms.push_str(&format!(
            "{{\"id\":{i},\"genauigkeit\":0.000001,\"text\":\"\"}}"
        ));
    }
    let file = format!(
        "{{\"format\": 10, \"marker\": 0, \"naechste_id\": 1, \"features\": [], \
         \"step_formen\": [{forms}]}}"
    );
    let m = msg(&file);
    assert!(m.contains("STEP-Formen: zu viele Einträge"), "{m}");
}

/// Eingefrorene Format-10-Datei (die Formate 8 und 9 haben keine eigene): ein grober
/// STEP-Quader, verschoben. Liest sich, rechnet und wird Byte für Byte gleich geschrieben.
#[test]
fn format10_golden_file() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("step_fixtures")
        .join("format10_step.json");
    let text = std::fs::read_to_string(path).unwrap();
    let s = decode_version(&text, "format10_step.json").unwrap();
    assert_eq!(required_format(&s), 10);
    assert_eq!(s.step_shapes.len(), 1);
    let form = &s.step_shapes[&StepShapeId(1)];
    assert_eq!(form.text(), quader_text());
    assert_eq!(form.tolerance_mm(), 1e-3);
    assert_eq!(
        encode_version(&s).unwrap().replace("\r\n", "\n"),
        text.replace("\r\n", "\n")
    );
    let d = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(d.features().len(), 2);
    assert!(d.features().iter().all(|f| f.status == FeatureStatus::Ok));
    let moved = d.body(first(d.features()[1].id)).unwrap();
    let (lo, hi) = bounds(moved);
    assert!((lo - DVec3::new(1.0, 0.0, 0.0)).abs().max_element() < 1e-9);
    assert!((hi - DVec3::new(11.0, 20.0, 30.0)).abs().max_element() < 1e-9);
    assert_eq!(
        encode_version(&d.snapshot()).unwrap().replace("\r\n", "\n"),
        text.replace("\r\n", "\n")
    );
}

/// Schlussprüfung M1: „Genauigkeit unbekannt“ bleibt nach Speichern und Laden; das Feld steht
/// nur in der Datei, wenn es zutrifft (bekannte Genauigkeit: Datei wie bisher).
#[test]
fn unknown_accuracy_survives_save_and_load() {
    let mut d = Document::new();
    let unknown = Arc::new(
        StepShape::new(quader_text(), 1e-3)
            .unwrap()
            .accuracy_unknown(),
    );
    d.add_step_bodies(vec![
        body("Quader", unknown),
        body("Würfel", shape(wuerfel_text(), 1e-3)),
    ])
    .unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["step_formen"][0]["genauigkeit_unbekannt"], true);
    let keys: Vec<&String> = v["step_formen"][1].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["genauigkeit", "id", "text"]);
    let back = decode_version(&text, "x.json").unwrap();
    assert!(!back.step_shapes[&StepShapeId(1)].tolerance_known());
    assert!(back.step_shapes[&StepShapeId(1)].is_coarse());
    assert!(back.step_shapes[&StepShapeId(2)].tolerance_known());
    assert_eq!(encode_version(&back).unwrap(), text);
}
