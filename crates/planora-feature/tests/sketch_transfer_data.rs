//! Neue Daten aus Skizzen 11b (Task 7): Skizzengruppen im Snapshot, Höhenformeln der Texte,
//! `{Name}`-Verweise und die Herkünfte Drehprofil und Isokurve.

use planora_feature::planes::{AxisRef, StdAxis};
use planora_feature::*;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use planora_sketch::{CurveId, Sketch};
use std::sync::Arc;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn shape(content: &str) -> Arc<TextShape> {
    let glyphs: Vec<Glyph> = content
        .chars()
        .enumerate()
        .map(|(i, _)| Glyph {
            origin: v(i as f64 * 0.8, 0.0),
            advance: 0.8,
            contours: vec![Contour {
                start: v(0.0, 0.0),
                segs: vec![
                    Seg::Line(v(0.6, 0.0)),
                    Seg::Line(v(0.6, 0.7)),
                    Seg::Line(v(0.0, 0.7)),
                    Seg::Line(v(0.0, 0.0)),
                ],
            }],
        })
        .collect();
    let width = content.chars().count() as f64 * 0.8;
    Arc::new(TextShape {
        lines: vec![ShapedLine { glyphs, width }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    })
}

fn text_sketch(contents: &[&str]) -> (Sketch, Vec<TextId>) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let ids = contents
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let y = i as f64 * 20.0;
            sk.add_text_box(
                v(0.0, y),
                v(40.0, y + 10.0),
                TextParams {
                    content: (*c).into(),
                    style: TextStyle {
                        font: "Gibt Es Nicht Sans".into(),
                        bold: false,
                        italic: false,
                        spacing: 0.0,
                    },
                    height: 5.0,
                    flip_h: false,
                    flip_v: false,
                },
                HAlign::Left,
                VAlign::Bottom,
                shape(c),
            )
            .unwrap()
        })
        .collect();
    (sk, ids)
}

fn feature(sketch: Sketch) -> SketchFeature {
    SketchFeature {
        sketch,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
        text_formulas: TextFormulas::new(),
    }
}

fn face(origin: u64) -> FaceRef {
    FaceRef {
        origin,
        role: "Mantel".into(),
        hint: DVec3::X,
    }
}

fn edge(origin: u64) -> EdgeRef {
    EdgeRef {
        a: face(origin),
        b: face(origin),
        hint: DVec3::Y,
    }
}

#[test]
fn param_ref_needs_a_valid_name_in_braces() {
    for yes in ["{Breite}", "{Breite} mm", "A {b_2} c", "{x}{y}", "{ä}"] {
        assert!(text_refs::has_param_ref(yes), "{yes}");
    }
    for no in [
        "",
        "Breite",
        "{}",
        "{ Breite }",
        "{1a}",
        "{Breite",
        "Breite}",
        "}{",
        "{a b}",
        "{pi}",
    ] {
        assert!(!text_refs::has_param_ref(no), "{no}");
    }
}

#[test]
fn prune_formulas_drops_formulas_of_dead_texts() {
    let (mut sk, ids) = text_sketch(&["A", "B"]);
    sk.remove_text(ids[1]).unwrap();
    let mut sf = feature(sk);
    sf.text_formulas.insert(ids[0], "Breite".into());
    sf.text_formulas.insert(ids[1], "Hoehe".into());
    assert!(sf.prune_formulas());
    assert_eq!(sf.text_formulas.len(), 1);
    assert!(sf.text_formulas.contains_key(&ids[0]));
    assert!(!sf.prune_formulas());
}

#[test]
fn text_formulas_count_for_equality() {
    let (sk, ids) = text_sketch(&["A"]);
    let a = feature(sk);
    let mut b = a.clone();
    assert_eq!(a, b);
    b.text_formulas.insert(ids[0], "2 * Breite".into());
    assert_ne!(a, b);
}

#[test]
fn spun_profile_origins() {
    let spec = SpunSpec {
        selection: SpunSelection::Elements(vec![
            CurveSource::Edge(edge(4)),
            CurveSource::SketchCurve {
                sketch: FeatureId(7),
                curve: CurveId::from_index(1),
            },
        ]),
        axis: SpunAxis::Ref(AxisRef::Edge(edge(5))),
        tolerance: 0.01,
        flip: false,
    };
    let s = CopySource::SpunProfile(spec.clone());
    assert_eq!(
        s.origins(),
        vec![FeatureId(4), FeatureId(4), FeatureId(5), FeatureId(5)]
    );
    assert_eq!(s.loose_origins(), vec![FeatureId(7)]);
    assert!(s.is_3d_kind());
    let bodies = CopySource::SpunProfile(SpunSpec {
        selection: SpunSelection::Bodies(vec![BodyRef {
            feature: FeatureId(3),
            index: 0,
        }]),
        axis: SpunAxis::Ref(AxisRef::SketchLine {
            sketch: FeatureId(8),
            curve: CurveId::from_index(0),
        }),
        tolerance: 0.01,
        flip: true,
    });
    assert_eq!(bodies.origins(), vec![FeatureId(3)]);
    assert_eq!(bodies.loose_origins(), vec![FeatureId(8)]);
    let own = CopySource::SpunProfile(SpunSpec {
        selection: SpunSelection::Bodies(vec![]),
        axis: SpunAxis::Own(CurveId::from_index(0)),
        tolerance: 0.01,
        flip: false,
    });
    assert!(own.origins().is_empty() && own.loose_origins().is_empty());
    let std_axis = CopySource::SpunProfile(SpunSpec {
        axis: SpunAxis::Ref(AxisRef::Standard(StdAxis::Z)),
        ..spec
    });
    assert!(s.same_family(&bodies) && s.same_family(&std_axis));
    assert!(!s.same_family(&CopySource::Edge(edge(4))));
}

#[test]
fn isoparametric_origins() {
    let s = CopySource::Isoparametric {
        face: face(9),
        direction: IsoDirection::V,
        value: 0.5,
    };
    assert_eq!(s.origins(), vec![FeatureId(9)]);
    assert!(s.loose_origins().is_empty());
    assert!(s.is_3d_kind());
    let u = CopySource::Isoparametric {
        face: face(2),
        direction: IsoDirection::U,
        value: 0.0,
    };
    assert!(s.same_family(&u));
    let spun = CopySource::SpunProfile(SpunSpec {
        selection: SpunSelection::Bodies(vec![]),
        axis: SpunAxis::Own(CurveId::from_index(0)),
        tolerance: 0.01,
        flip: false,
    });
    assert!(!s.same_family(&spun));
}

/// Skizzengruppen gehen über den Snapshot: Laden und Rückgängig geben sie zurück.
#[test]
fn sketch_groups_live_in_the_snapshot() {
    let mut d = Document::new();
    let a = d
        .add_feature(
            "Skizze1",
            FeatureKind::Sketch(feature(Sketch::new(
                Frame::WORLD,
                "Skizze1",
                Tolerance::default(),
            ))),
        )
        .unwrap();
    assert!(d.sketch_groups().is_empty());
    let empty = d.snapshot();
    let mut s = d.snapshot();
    let mut g = SketchGroups::default();
    g.groups.push(SketchGroup {
        id: SketchGroupId(1),
        name: "Gruppe".into(),
        parent: None,
        hidden: true,
    });
    g.member.insert(a, SketchGroupId(1));
    g.next = 2;
    assert!(!g.is_empty());
    s.sketch_groups = g.clone();
    assert!(!s.same_content(&empty));
    let loaded = Document::from_snapshot(s.clone(), 0.05).unwrap();
    assert_eq!(loaded.sketch_groups(), &g);
    d.restore(s).unwrap();
    assert_eq!(d.sketch_groups(), &g);
    d.restore(empty).unwrap();
    assert!(d.sketch_groups().is_empty());
}
