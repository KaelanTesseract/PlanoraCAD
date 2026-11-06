//! Dateiformat 15 (Teilprojekt Skizzen 5, Task 2): die neuen Abhängigkeitsarten. Nur eine Skizze
//! mit einem Eintrag dieser Arten (auch einem gelöschten) hebt die Formatnummer auf 15; ohne sie
//! bleibt der Text bytegleich wie bisher. Task 5: Muster (auch aufgelöste) ebenso.

use planora_feature::*;
use planora_math::{DVec2, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::pattern::{
    CircularMode, PatternDir, PatternId, PatternKind, PatternPlan, PatternSlot,
};
use planora_sketch::shapes::{SlotPlan, SlotStyle};
use planora_sketch::{ConstraintId, CurveId, CurveKind, PointId, Sketch, SketchData};

fn new_sketch() -> Sketch {
    Sketch::new(StandardPlane::XY.frame(), "Skizze 1", Tolerance::default())
}

fn document(sk: Sketch) -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Skizze 1",
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    d
}

fn load(text: &str) -> Result<Document, ProjectError> {
    let s = decode_version(text, "x.json")?;
    Document::from_snapshot(s, 0.05).map_err(|e| ProjectError::Invalid(e.to_string()))
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> (CurveId, PointId, PointId) {
    let (pa, pb) = (
        s.add_point(a.0, a.1).unwrap(),
        s.add_point(b.0, b.1).unwrap(),
    );
    (s.add_line(pa, pb).unwrap(), pa, pb)
}

/// Legt eine Abhängigkeit der neuen Art Nummer `k` (0 bis 5) samt ihrer Geometrie an.
fn add_kind(s: &mut Sketch, k: usize) -> ConstraintId {
    match k {
        0 => {
            let (l1, _, _) = line(s, (0.0, 0.0), (10.0, 0.0));
            let (l2, _, _) = line(s, (15.0, 0.1), (25.0, 0.0));
            s.add_constraint(K::Collinear(l1, l2)).unwrap()
        }
        1 => {
            let (l3, _, joint) = line(s, (-10.0, 20.0), (0.0, 20.0));
            let q = [
                s.add_point(5.0, 23.0).unwrap(),
                s.add_point(10.0, 20.0).unwrap(),
            ];
            let sp = s.add_spline(&[joint, q[0], q[1]]).unwrap();
            s.add_constraint(K::Smooth(l3, joint, sp, joint)).unwrap()
        }
        2 | 3 => {
            let (p, q) = (
                s.add_point(30.0, 30.0 + 10.0 * k as f64).unwrap(),
                s.add_point(34.0, 37.0 + 10.0 * k as f64).unwrap(),
            );
            if k == 2 {
                s.add_constraint(K::DistanceX(p, q, 4.0)).unwrap()
            } else {
                s.add_constraint(K::DistanceY(p, q, 7.0)).unwrap()
            }
        }
        4 => {
            let (axis, _, _) = line(s, (0.0, -30.0), (10.0, -30.0));
            s.set_centerline(axis, true).unwrap();
            let r = s.add_point(3.0, -34.0).unwrap();
            s.add_constraint(K::CenterlineDiameter(r, axis, 8.0))
                .unwrap()
        }
        _ => {
            let slot = s
                .add_slot(
                    &SlotPlan {
                        a: DVec2::new(0.0, -60.0),
                        b: DVec2::new(20.0, -60.0),
                        radius: 3.0,
                    },
                    SlotStyle::CenterToCenter,
                )
                .unwrap();
            s.add_constraint(K::SlotOverall(
                slot.centers[0],
                slot.centers[1],
                slot.caps[0],
                26.0,
            ))
            .unwrap()
        }
    }
}

/// Skizze mit jeweils einer Abhängigkeit der sechs neuen Arten; liefert auch die Ids.
fn all_kinds() -> (Sketch, Vec<ConstraintId>) {
    let mut s = new_sketch();
    let ids = (0..6).map(|k| add_kind(&mut s, k)).collect();
    (s, ids)
}

#[test]
fn format_version_is_15() {
    assert_eq!(FORMAT_VERSION, 15);
}

#[test]
fn every_new_kind_round_trips_through_the_data_and_the_project_format() {
    let (s, _) = all_kinds();
    let data = s.to_data();
    // Seite (branch) je Eintrag einmal anders setzen, damit sie nachweislich erhalten bleibt.
    let mut data2 = data.clone();
    for (i, c) in data2.constraints.iter_mut().enumerate() {
        if matches!(
            c.kind,
            K::DistanceX(..) | K::DistanceY(..) | K::CenterlineDiameter(..)
        ) {
            c.branch = if i % 2 == 0 { -1 } else { 1 };
        }
    }
    for d in [data, data2] {
        let sk = Sketch::from_data(d.clone()).unwrap();
        assert_eq!(sk.to_data(), d);
        let doc = document(sk);
        let snap = doc.snapshot();
        assert_eq!(required_format(&snap), 15);
        let text = encode_version(&snap).unwrap();
        assert!(text.contains("\"format\": 15"), "{text}");
        for name in [
            "kollinear",
            "glatt",
            "abstand_waagerecht",
            "abstand_senkrecht",
            "durchmesser_zur_achse",
            "langloch_gesamt",
        ] {
            assert!(text.contains(&format!("\"typ\": \"{name}\"")), "{name}");
        }
        let back = load(&text).unwrap();
        assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
        let FeatureKind::Sketch(sf) = &back.features()[0].kind else {
            panic!("keine Skizze")
        };
        assert_eq!(sf.sketch.to_data(), d);
    }
}

#[test]
fn every_new_kind_alone_needs_format_15_even_when_removed() {
    let (s, _) = all_kinds();
    let kinds: Vec<K> = s.constraints().map(|(_, c)| c.kind.clone()).collect();
    assert_eq!(kinds.iter().filter(|k| k.needs_format_15()).count(), 6);
    for k in 0..6 {
        // Eine Skizze mit genau dieser einen neuen Art: lebend 15, gelöscht weiter 15, denn der
        // Eintrag bleibt in der Datei; keine andere neue Art hilft dabei mit.
        let mut sk = new_sketch();
        let id = add_kind(&mut sk, k);
        let new_kinds = |sk: &Sketch| {
            sk.constraints()
                .filter(|(_, c)| c.kind.needs_format_15())
                .count()
        };
        assert_eq!(new_kinds(&sk), 1, "Art {k}");
        assert_eq!(required_format(&document(sk.clone()).snapshot()), 15);
        sk.remove_constraint(id).unwrap();
        assert_eq!(new_kinds(&sk), 0, "Art {k}");
        assert!(sk.needs_format_15(), "Art {k}");
        let doc = document(sk.clone());
        assert_eq!(required_format(&doc.snapshot()), 15, "Art {k}");
        // Datei-Rundlauf mit nur dem gelöschten Eintrag.
        let text = encode_version(&doc.snapshot()).unwrap();
        assert!(text.contains("\"format\": 15"), "Art {k}");
        let back = load(&text).unwrap();
        assert_eq!(encode_version(&back.snapshot()).unwrap(), text, "Art {k}");
        let FeatureKind::Sketch(sf) = &back.features()[0].kind else {
            panic!("keine Skizze")
        };
        assert_eq!(sf.sketch.to_data(), sk.to_data(), "Art {k}");
    }
}

#[test]
fn required_format_is_unchanged_without_new_entries_and_15_with_collinear() {
    let mut s = new_sketch();
    let (l1, _, _) = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let (l2, _, _) = line(&mut s, (15.0, 0.1), (25.0, 0.0));
    s.add_constraint(K::Parallel(l1, l2)).unwrap();
    assert!(!s.needs_format_15());
    assert_eq!(required_format(&document(s.clone()).snapshot()), 1);
    s.set_centerline(l1, true).unwrap();
    assert_eq!(required_format(&document(s.clone()).snapshot()), 14);

    let mut live = s.clone();
    let c = live.add_constraint(K::Collinear(l1, l2)).unwrap();
    assert!(live.needs_format_15());
    assert_eq!(required_format(&document(live.clone()).snapshot()), 15);
    live.remove_constraint(c).unwrap();
    assert!(live.needs_format_15());
    assert_eq!(required_format(&document(live).snapshot()), 15);
}

#[test]
fn every_fixture_loads_and_writes_the_same_bytes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let doc = load(&text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let out = encode_version(&doc.snapshot()).unwrap();
        let head = |t: &str| {
            t.lines()
                .find(|l| l.contains("\"format\""))
                .unwrap()
                .trim()
                .to_string()
        };
        assert_eq!(head(&out), head(&text), "{name}: Formatnummer");
        assert_eq!(out, text, "{name}: Bytes");
        n += 1;
    }
    assert!(n >= 8, "{n}");
}

#[test]
fn a_head_newer_than_the_known_format_is_too_new() {
    let (s, _) = all_kinds();
    let text = encode_version(&document(s).snapshot()).unwrap();
    assert!(load(&text).is_ok());
    let newer = text.replacen("\"format\": 15", "\"format\": 16", 1);
    let e = load(&newer).unwrap_err();
    assert!(matches!(e, ProjectError::TooNew { found: 16, .. }), "{e:?}");
}

// ---------------------------------------------------------------------------------------------
// Muster (Task 5)
// ---------------------------------------------------------------------------------------------

fn document_with(sk: Sketch, formulas: SketchFormulas) -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Skizze 1",
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas,
        }),
    )
    .unwrap();
    d
}

/// Der Musterwert im Feld `slot`.
fn value_of(s: &Sketch, id: PatternId, slot: PatternSlot) -> ConstraintId {
    *s.pattern(id)
        .unwrap()
        .values
        .iter()
        .find(|&&c| matches!(s.constraint(c).unwrap().kind, K::PatternValue(_, x, _) if x == slot))
        .unwrap()
}

/// Ids der Muster in [`pattern_sketch`].
struct Patterns {
    rect: PatternId,
    circular: PatternId,
    gone: PatternId,
    spacing: ConstraintId,
    width: ConstraintId,
    circle: CurveId,
}

/// Bemaßtes Rechteck als Vorlage eines Rechteckmusters 3×2 mit einer abgewählten Lage; ein
/// freier Kreis als Vorlage eines vollen Kreismusters um eine feste Mitte mit Abwahl; ein drittes
/// Muster, schon aufgelöst (gelöschter Eintrag). Danach gelöst.
fn pattern_sketch() -> (Sketch, Patterns) {
    let mut s = new_sketch();
    let r = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
        .unwrap();
    let [p0, p1, _, p3] = r.points;
    s.add_constraint(K::Fixed(p0, 0.0, 0.0)).unwrap();
    let width = s.add_constraint(K::Distance(p0, p1, 10.0)).unwrap();
    s.add_constraint(K::Distance(p0, p3, 6.0)).unwrap();
    let rect = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Rect {
                dirs: [PatternDir::SketchX, PatternDir::SketchY],
                extent: [false, false],
                symmetric: [false, false],
            },
            sources: r.lines.to_vec(),
            counts: [3, 2],
            values: [15.0, 10.0],
            suppressed: vec![[1, 1]],
        })
        .unwrap();
    let center = s.add_point(100.0, 0.0).unwrap();
    s.add_constraint(K::Fixed(center, 100.0, 0.0)).unwrap();
    let m = s.add_point(120.0, 0.0).unwrap();
    let circle = s.add_circle(m, 3.0).unwrap();
    let circular = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Circular {
                center,
                mode: CircularMode::Full,
            },
            sources: vec![circle],
            counts: [6, 1],
            values: [0.0, 0.0],
            suppressed: vec![[2, 0]],
        })
        .unwrap();
    let (l, _, _) = line(&mut s, (0.0, -50.0), (10.0, -50.0));
    let gone = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Rect {
                dirs: [PatternDir::SketchY, PatternDir::SketchX],
                extent: [true, false],
                symmetric: [false, true],
            },
            sources: vec![l],
            counts: [2, 1],
            values: [-8.0, 5.0],
            suppressed: vec![],
        })
        .unwrap();
    s.dissolve_pattern(gone).unwrap();
    let report = s.solve();
    assert!(report.conflicting.is_empty(), "{report:?}");
    let spacing = value_of(&s, rect, PatternSlot::Spacing1);
    (
        s,
        Patterns {
            rect,
            circular,
            gone,
            spacing,
            width,
            circle,
        },
    )
}

fn bits(s: &Sketch) -> Vec<u64> {
    s.to_data().vars.iter().map(|v| v.to_bits()).collect()
}

#[test]
fn patterns_round_trip_through_the_data_and_the_project_format() {
    let (s, ids) = pattern_sketch();
    assert_eq!(s.pattern_count(), 2);
    assert!(s.pattern(ids.gone).is_none());
    let data = s.to_data();
    assert_eq!(data.patterns.len(), 3);
    assert!(data.patterns[ids.rect.index()].alive);
    assert!(!data.patterns[ids.gone.index()].alive);
    assert_eq!(data.patterns[ids.rect.index()].suppressed, vec![[1, 1]]);
    let back = Sketch::from_data(data.clone()).unwrap();
    assert_eq!(back.to_data(), data);

    let mut formulas = SketchFormulas::new();
    formulas.insert(ids.spacing, "10+5".to_string());
    let doc = document_with(s.clone(), formulas);
    let snap = doc.snapshot();
    assert_eq!(required_format(&snap), 15);
    let text = encode_version(&snap).unwrap();
    assert!(text.contains("\"format\": 15"));
    assert!(text.contains("\"muster\""), "{text}");
    assert!(text.contains("\"typ\": \"muster_wert\""));
    let loaded = load(&text).unwrap();
    assert_eq!(encode_version(&loaded.snapshot()).unwrap(), text);
    let FeatureKind::Sketch(sf) = &loaded.features()[0].kind else {
        panic!("keine Skizze")
    };
    assert_eq!(sf.sketch.to_data(), data);
    assert_eq!(
        sf.formulas.get(&ids.spacing).map(String::as_str),
        Some("10+5")
    );
    let mut sk = sf.sketch.clone();
    assert_eq!(sk.pattern_count(), 2);
    let before = bits(&sk);
    let r = sk.solve();
    assert_eq!(r.iterations, 0, "{r:?}");
    assert!(r.conflicting.is_empty(), "{r:?}");
    assert_eq!(bits(&sk), before, "Lagen bitgleich");
    // Das geladene Muster lebt weiter: Abstand, Vorlagenmaß und Anzahl ändern.
    sk.set_constraint_value(ids.spacing, 20.0).unwrap();
    sk.set_constraint_value(ids.width, 12.0).unwrap();
    sk.solve();
    let instances = |sk: &Sketch, id| sk.pattern(id).unwrap().instances.len();
    assert_eq!(instances(&sk, ids.rect), 4);
    assert_eq!(instances(&sk, ids.circular), 4);
    let m = sk.pattern(ids.rect).unwrap();
    let inst = m.instances.iter().find(|i| i.at == [1, 0]).unwrap();
    for (&q, &p) in inst.points.iter().zip(&m.source_points) {
        let want = sk.point_xy(p).unwrap() + DVec2::new(20.0, 0.0);
        assert!(sk.point_xy(q).unwrap().distance(want) < 1e-9);
    }
    let corner = sk.point_xy(m.source_points[1]).unwrap();
    assert!(corner.distance(DVec2::new(12.0, 0.0)) < 1e-6, "{corner:?}");
    let count = value_of(&sk, ids.circular, PatternSlot::Count);
    sk.set_constraint_value(count, 8.0).unwrap();
    assert_eq!(instances(&sk, ids.circular), 6);
    assert!(sk.pattern_of_curve(ids.circle).is_some());
}

#[test]
fn old_sketches_without_patterns_write_no_pattern_list() {
    let (s, _) = all_kinds();
    assert!(s.to_data().patterns.is_empty());
    let mut s1 = new_sketch();
    line(&mut s1, (0.0, 0.0), (10.0, 0.0));
    let doc = document(s1);
    assert_eq!(required_format(&doc.snapshot()), 1);
    let text = encode_version(&doc.snapshot()).unwrap();
    assert!(!text.contains("\"muster\""), "{text}");
}

#[test]
fn a_dissolved_pattern_alone_needs_format_15() {
    let mut s = new_sketch();
    let (l, _, _) = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let id = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Rect {
                dirs: [PatternDir::SketchY, PatternDir::SketchX],
                extent: [false, false],
                symmetric: [false, false],
            },
            sources: vec![l],
            counts: [3, 1],
            values: [5.0, 5.0],
            suppressed: vec![],
        })
        .unwrap();
    assert!(s.needs_format_15());
    s.dissolve_pattern(id).unwrap();
    assert!(s.constraints().all(|(_, c)| !c.kind.needs_format_15()));
    assert!(s.needs_format_15());
    let doc = document(s.clone());
    assert_eq!(required_format(&doc.snapshot()), 15);
    let text = encode_version(&doc.snapshot()).unwrap();
    let back = load(&text).unwrap();
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);

    // Der Mustereintrag allein, ohne einen Musterwert in der Datei: Die gelöschten Werte werden
    // zu gelöschten alten Einträgen, das aufgelöste Muster führt keine Werte mehr.
    let (a, b) = match s.curve(l).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        _ => panic!("keine Linie"),
    };
    let mut d = s.to_data();
    for c in &mut d.constraints {
        if matches!(c.kind, K::PatternValue(..)) {
            c.kind = K::Coincident(a, b);
        }
    }
    d.patterns[id.index()].values.clear();
    let only = Sketch::from_data(d).unwrap();
    assert!(only.constraints().all(|(_, c)| !c.kind.needs_format_15()));
    assert!(only.needs_format_15());
    assert_eq!(required_format(&document(only).snapshot()), 15);
}

/// `from_data` mit veränderten Rohdaten: liefert die Fehlermeldung.
fn broken(f: impl FnOnce(&mut SketchData, &Patterns)) -> String {
    let (s, ids) = pattern_sketch();
    let mut d = s.to_data();
    f(&mut d, &ids);
    Sketch::from_data(d).unwrap_err().to_string()
}

#[test]
fn from_data_refuses_broken_patterns() {
    // Exemplar mit falscher Punkt- oder Kurvenzahl.
    let m = broken(|d, ids| {
        d.patterns[ids.rect.index()].instances[0].points.pop();
    });
    assert!(m.contains("Exemplar"), "{m}");
    let m = broken(|d, ids| {
        d.patterns[ids.circular.index()].instances[1].curves.clear();
    });
    assert!(m.contains("Exemplar"), "{m}");
    // Wert-Id, die kein Musterwert ist; eine eines anderen Musters; ein fehlendes Feld.
    let m = broken(|d, ids| {
        d.patterns[ids.rect.index()].values[1] = ids.width;
    });
    assert!(m.contains("Musterwert"), "{m}");
    let m = broken(|d, ids| {
        let other = d.patterns[ids.circular.index()].values[0];
        d.patterns[ids.rect.index()].values[0] = other;
    });
    assert!(m.contains("Musterwert"), "{m}");
    let m = broken(|d, ids| {
        d.patterns[ids.rect.index()].values.pop();
    });
    assert!(m.contains("Musterwert"), "{m}");
    let m = broken(|d, ids| {
        d.patterns[ids.rect.index()].values[0] = ConstraintId::from_index(100_000);
    });
    assert!(m.contains("fehlend"), "{m}");
    // Vorlage gelöscht bei lebendem Muster (der Kreis hat keine Abhängigkeiten).
    let m = broken(|d, ids| {
        d.curves[ids.circle.index()].alive = false;
    });
    assert!(m.contains("Muster") && m.contains("gelöscht"), "{m}");
    // Exemplar zugleich Vorlage: eine Kurve als Vorlage und als Exemplar, ein Exemplarpunkt als
    // Mitte eines anderen Musters.
    let m = broken(|d, ids| {
        let r = &mut d.patterns[ids.rect.index()];
        r.instances[0].curves[0] = r.sources[0];
    });
    assert!(m.contains("keine Vorlage"), "{m}");
    let m = broken(|d, ids| {
        let q = d.patterns[ids.rect.index()].instances[0].points[0];
        let PatternKind::Circular { center, .. } = &mut d.patterns[ids.circular.index()].kind
        else {
            panic!("kein Kreismuster")
        };
        *center = q;
    });
    assert!(m.contains("keine Vorlage"), "{m}");
    // Ein Index außerhalb der Liste.
    let m = broken(|d, ids| {
        d.patterns[ids.rect.index()].sources[0] = CurveId::from_index(100_000);
    });
    assert!(m.contains("fehlend"), "{m}");
    // Ein lebender Musterwert eines aufgelösten Musters.
    let m = broken(|d, ids| {
        let c = d.patterns[ids.gone.index()].values[0];
        d.constraints[c.index()].alive = true;
    });
    assert!(m.contains("Musterwert"), "{m}");
}

// ---------------------------------------------------------------------------------------------
// Korrektur 1: Rundläufe nach `edit_pattern`, alle Richtungs- und Kreisarten, Schleifen
// ---------------------------------------------------------------------------------------------

/// Datei-Rundlauf: Format 15, gleiche Bytes, gleiche Rohdaten; danach lösen ohne Schritt,
/// Lagen bitgleich. Liefert die geladene Skizze.
fn file_round_trip(s: &Sketch, tag: &str) -> Sketch {
    let doc = document(s.clone());
    let text = encode_version(&doc.snapshot()).unwrap();
    assert!(text.contains("\"format\": 15"), "{tag}");
    let back = load(&text).unwrap_or_else(|e| panic!("{tag}: {e:?}"));
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text, "{tag}");
    let FeatureKind::Sketch(sf) = &back.features()[0].kind else {
        panic!("keine Skizze")
    };
    assert_eq!(sf.sketch.to_data(), s.to_data(), "{tag}");
    let mut sk = sf.sketch.clone();
    let before = bits(&sk);
    let r = sk.solve();
    assert_eq!(r.iterations, 0, "{tag}: {r:?}");
    assert_eq!(bits(&sk), before, "{tag}");
    sk
}

fn circular(
    sources: Vec<CurveId>,
    center: PointId,
    mode: CircularMode,
    n: u32,
    angle: f64,
) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Circular { center, mode },
        sources,
        counts: [n, 1],
        values: [angle, 0.0],
        suppressed: vec![],
    }
}

fn rect_plan(sources: Vec<CurveId>, dirs: [PatternDir; 2]) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Rect {
            dirs,
            extent: [false, false],
            symmetric: [false, false],
        },
        sources,
        counts: [3, 2],
        values: [15.0, 10.0],
        suppressed: vec![],
    }
}

#[test]
fn circular_modes_angle_full_symmetric_round_trip_after_edits() {
    let mut s = new_sketch();
    let c = s.add_point(100.0, 0.0).unwrap();
    let m = s.add_point(120.0, 0.0).unwrap();
    let circle = s.add_circle(m, 3.0).unwrap();
    let mut plan = circular(vec![circle], c, CircularMode::Angle, 5, 1.5);
    let id = s.add_pattern(&plan).unwrap();
    let text = encode_version(&document(s.clone()).snapshot()).unwrap();
    assert!(text.contains("\"art\": \"winkel\""), "{text}");
    file_round_trip(&s, "Winkel");
    // Winkel -> Voll: Der Winkelwert bleibt als gelöschter Eintrag mit einem Feld, das nicht mehr
    // zur Art passt.
    plan.kind = PatternKind::Circular {
        center: c,
        mode: CircularMode::Full,
    };
    s.edit_pattern(id, &plan).unwrap();
    assert!(
        s.to_data()
            .constraints
            .iter()
            .any(|r| !r.alive && matches!(r.kind, K::PatternValue(_, PatternSlot::Angle, _)))
    );
    file_round_trip(&s, "Voll nach Winkel");
    plan.kind = PatternKind::Circular {
        center: c,
        mode: CircularMode::Symmetric,
    };
    plan.values = [1.0, 0.0];
    s.edit_pattern(id, &plan).unwrap();
    let text = encode_version(&document(s.clone()).snapshot()).unwrap();
    assert!(text.contains("\"art\": \"symmetrisch\""), "{text}");
    s.suppress_instance(id, [1, 0], true).unwrap();
    let mut back = file_round_trip(&s, "Symmetrisch mit Abwahl");
    back.suppress_instance(id, [1, 0], false).unwrap();
    file_round_trip(&back, "wieder angewählt nach dem Laden");
}

#[test]
fn kind_and_source_changes_round_trip_after_edits() {
    let mut s = new_sketch();
    let r = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
        .unwrap();
    let src = r.lines.to_vec();
    let id = s
        .add_pattern(&rect_plan(
            src.clone(),
            [PatternDir::SketchX, PatternDir::SketchY],
        ))
        .unwrap();
    let c = s.add_point(100.0, 0.0).unwrap();
    // Rechteck -> Kreis: die Werte des Rechtecks bleiben gelöscht mit Rechteckfeldern.
    s.edit_pattern(id, &circular(src.clone(), c, CircularMode::Full, 4, 0.0))
        .unwrap();
    file_round_trip(&s, "Rechteck -> Kreis");
    // Kreis -> Rechteck.
    s.edit_pattern(
        id,
        &rect_plan(src.clone(), [PatternDir::SketchX, PatternDir::SketchY]),
    )
    .unwrap();
    file_round_trip(&s, "Kreis -> Rechteck");
    // Andere Vorlagen: alle Exemplare neu.
    let other = s
        .add_rectangle(DVec2::new(0.0, 30.0), DVec2::new(4.0, 34.0))
        .unwrap();
    s.edit_pattern(
        id,
        &rect_plan(
            other.lines.to_vec(),
            [PatternDir::SketchX, PatternDir::SketchY],
        ),
    )
    .unwrap();
    assert_eq!(s.pattern(id).unwrap().sources, other.lines.to_vec());
    file_round_trip(&s, "andere Vorlagen");
}

#[test]
fn a_line_direction_round_trips() {
    let mut s = new_sketch();
    let (dir, _, _) = line(&mut s, (0.0, -20.0), (10.0, -15.0));
    let r = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
        .unwrap();
    s.add_pattern(&rect_plan(
        r.lines.to_vec(),
        [PatternDir::Line(dir), PatternDir::SketchY],
    ))
    .unwrap();
    let text = encode_version(&document(s.clone()).snapshot()).unwrap();
    assert!(text.contains("\"typ\": \"linie\""), "{text}");
    file_round_trip(&s, "Richtungslinie");
}

#[test]
fn an_arc_or_elliptic_arc_closing_a_pattern_loop_is_refused_and_files_stay_loadable() {
    let mut s = new_sketch();
    let r = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
        .unwrap();
    let id = s
        .add_pattern(&rect_plan(
            r.lines.to_vec(),
            [PatternDir::SketchX, PatternDir::SketchY],
        ))
        .unwrap();
    let inst = s.pattern(id).unwrap().instances[0].points.clone();
    let before = s.to_data();
    // Bogen um einen Vorlagenpunkt mit Enden auf einem Exemplar (der Fang bietet das an).
    let e = s.add_arc(r.points[0], inst[0], inst[1]).unwrap_err();
    assert!(e.to_string().contains("eigenen Exemplar"), "{e}");
    let e = s
        .add_elliptic_arc(r.points[0], DVec2::new(20.0, 0.0), 5.0, inst[0], inst[1])
        .unwrap_err();
    assert!(e.to_string().contains("eigenen Exemplar"), "{e}");
    assert_eq!(s.to_data(), before, "nichts angelegt");
    // Ein Bogen, der nur Exemplare liest, geht und wird gespeichert.
    let free = s.add_point(30.0, 30.0).unwrap();
    let arc = s.add_arc(free, inst[0], inst[1]).unwrap();
    s.solve();
    file_round_trip(&s, "Bogen an Exemplaren");

    // Eine Schleife allein über eine interne Abhängigkeit (etwa aus Stutzen oder Brechen) lädt;
    // dieselbe Schleife über eine Abhängigkeit des Benutzers nicht.
    let mut d = s.to_data();
    let CurveKind::Arc { start, end, .. } = d.curves[arc.index()].kind else {
        panic!("kein Bogen")
    };
    d.curves[arc.index()].kind = CurveKind::Arc {
        center: r.points[0],
        start,
        end,
    };
    assert!(Sketch::from_data(d.clone()).is_ok());
    let mut user = s.clone();
    user.add_constraint(K::Distance(free, inst[0], 5.0))
        .unwrap();
    let mut d = user.to_data();
    let k = d
        .constraints
        .iter()
        .position(|c| matches!(c.kind, K::Distance(a, _, _) if a == free))
        .unwrap();
    d.constraints[k].kind = K::Distance(r.points[0], inst[0], 5.0);
    let e = Sketch::from_data(d).unwrap_err();
    assert!(e.to_string().contains("eigenen Exemplar"), "{e}");
}
