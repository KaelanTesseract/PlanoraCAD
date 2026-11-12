//! Text in der Feature-Berechnung (Skizzen 8, Task 10, Spec C10, G4, G7): Extrusionen behalten
//! ihre Textprofile, eine fehlende Schrift ist eine Warnung an der Skizze und kein Fehler, ein
//! noch nicht gesetzter Text (nach dem Laden) warnt nicht, und das Neusetzen nach dem Laden ändert
//! nichts, wenn die Form gleich bleibt.
//!
//! Gesetzt wird nur mit den mitgelieferten Schriften ([`FontLibrary::bundled_only`]), nie mit
//! Systemschriften.

use planora_feature::*;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_sketch::transform::Similarity;
use planora_text::FontLibrary;
use std::sync::Arc;

/// Eine Schrift, die es nirgends gibt.
const MISSING: &str = "Gibtsnicht Grotesk";

fn style(font: &str) -> TextStyle {
    TextStyle {
        font: font.into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    }
}

fn params(content: &str, font: &str) -> TextParams {
    TextParams {
        content: content.into(),
        style: style(font),
        height: 5.0,
        flip_h: false,
        flip_v: false,
    }
}

/// Text `content` in der Schrift `font`, gesetzt mit `shaper`, im Rahmen (0, 0)–(40, 10).
fn add_text(sk: &mut Sketch, content: &str, font: &str, shaper: &dyn TextShaper) -> TextId {
    let shape = shaper.shape(content, &style(font)).unwrap();
    sk.add_text_box(
        DVec2::new(0.0, 0.0),
        DVec2::new(40.0, 10.0),
        params(content, font),
        HAlign::Left,
        VAlign::Bottom,
        Arc::new(shape),
    )
    .unwrap()
}

fn sketch_kind(sketch: Sketch) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    })
}

/// Wie beim Laden: über die Daten der Datei, die Texte sind danach nicht gesetzt.
fn loaded(sk: &Sketch) -> Sketch {
    Sketch::from_data(sk.to_data()).unwrap()
}

fn sketch_def(d: &Document, s: FeatureId) -> &SketchFeature {
    match &d.feature(s).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

fn volume(s: &Solid) -> f64 {
    let options = TessellationOptions {
        chord_tolerance: 1e-3,
    };
    mass_properties(&tessellate(s, &options).unwrap()).volume
}

/// Kleinstes x aller Eckpunkte der sichtbaren Körper.
fn min_x(d: &Document) -> f64 {
    d.visible_bodies()
        .iter()
        .flat_map(|b| b.solid.vertices.iter().map(|(_, v)| v.point.x))
        .fold(f64::INFINITY, f64::min)
}

fn total_volume(d: &Document) -> f64 {
    d.visible_bodies().iter().map(|b| volume(b.solid)).sum()
}

fn font_warning(font: &str, count: usize) -> String {
    planora_i18n::tr!(
        "feature-sketch-font-missing",
        font = font.to_string(),
        count = count
    )
}

#[test]
fn extrude_of_text_keeps_profile_after_move() {
    let lib = FontLibrary::bundled_only();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let t = add_text(&mut sk, "Ho", "Montserrat", &lib);
    let refs: Vec<_> = sk
        .profiles()
        .refs
        .into_iter()
        .filter(|r| sk.text_of_profile(r) == Some(t))
        .collect();
    assert!(refs.len() >= 2, "H und o: {} Profile", refs.len());
    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", sketch_kind(sk)).unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Refs(refs),
                height: 2.0,
            },
        )
        .unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    let bodies = d.visible_bodies().len();
    let vol = total_volume(&d);
    let x0 = min_x(&d);
    assert!(vol > 0.0);

    // Den Rahmen samt Text um 10 in x verschieben, wie „Verschieben“ in der Skizze.
    d.update(s, |k| {
        let FeatureKind::Sketch(sf) = k else {
            unreachable!()
        };
        let glyph = sf.sketch.text(t).unwrap().curves[0];
        let set = sf.sketch.transform_set(&[glyph], &[]);
        sf.sketch
            .apply_transform(
                &set,
                &Similarity {
                    base: DVec2::ZERO,
                    offset: DVec2::new(10.0, 0.0),
                    angle: 0.0,
                    factor: 1.0,
                },
            )
            .unwrap();
    })
    .unwrap();
    let f = d.feature(e).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert_eq!(d.visible_bodies().len(), bodies, "gleich viele Körper");
    let moved = total_volume(&d);
    // Die Volumen kommen aus der Tessellierung; die Bögen der Buchstaben werden an anderer Lage
    // etwas anders unterteilt.
    assert!(((moved - vol) / vol).abs() < 1e-4, "{moved} statt {vol}");
    assert!(
        (min_x(&d) - (x0 + 10.0)).abs() < 1e-6,
        "Körper mitgewandert"
    );
    d.check_invariants().unwrap();
}

#[test]
fn missing_font_gives_warning_not_error() {
    let lib = FontLibrary::bundled_only();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let t = add_text(&mut sk, "Ho", MISSING, &lib);
    assert!(sk.is_text_shaped(t));
    let refs: Vec<_> = sk.profiles().refs;
    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", sketch_kind(sk.clone())).unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Refs(refs),
                height: 2.0,
            },
        )
        .unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert_eq!(f.warning(), Some(font_warning(MISSING, 1).as_str()));
    assert!(f.warning().unwrap().contains(MISSING));
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    assert!(d.feature(e).unwrap().warning().is_none());

    // Zwei Texte derselben fehlenden Schrift: eine Zeile mit der Anzahl.
    let mut two = sk.clone();
    let shape = lib.shape("x", &style(MISSING)).unwrap();
    two.add_text_box(
        DVec2::new(0.0, 20.0),
        DVec2::new(40.0, 30.0),
        params("x", MISSING),
        HAlign::Left,
        VAlign::Bottom,
        Arc::new(shape),
    )
    .unwrap();
    let mut d2 = Document::new();
    let s2 = d2.add_feature("Skizze 1", sketch_kind(two)).unwrap();
    let f = d2.feature(s2).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok);
    assert_eq!(f.warning(), Some(font_warning(MISSING, 2).as_str()));

    // Eine vorhandene Schrift warnt nicht.
    let mut ok = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    add_text(&mut ok, "Ho", "Montserrat", &lib);
    let mut d3 = Document::new();
    let s3 = d3.add_feature("Skizze 1", sketch_kind(ok)).unwrap();
    assert!(d3.feature(s3).unwrap().warning().is_none());
}

#[test]
fn unshaped_text_gives_no_warning() {
    let lib = FontLibrary::bundled_only();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let t = add_text(&mut sk, "Ho", MISSING, &lib);
    let sk = loaded(&sk);
    assert!(!sk.is_text_shaped(t));
    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", sketch_kind(sk)).unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());

    // Erst das Neusetzen bringt die Warnung (Spec G4), auch wenn die Form gleich bleibt: Die
    // gespeicherten Kurven stammen schon aus der Ersatzschrift.
    let edits = d.edit_count();
    assert!(!d.reshape_texts(&lib), "gleiche Form");
    assert_eq!(d.edit_count(), edits, "keine Änderung");
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok);
    assert_eq!(f.warning(), Some(font_warning(MISSING, 1).as_str()));
    assert!(sketch_def(&d, s).sketch.is_text_shaped(t));
}

#[test]
fn reshape_after_load_same_font_is_unchanged() {
    let lib = FontLibrary::bundled_only();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let t = add_text(&mut sk, "Hallo", "Montserrat", &lib);
    let refs: Vec<_> = sk.profiles().refs;
    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", sketch_kind(loaded(&sk))).unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Refs(refs),
                height: 2.0,
            },
        )
        .unwrap();
    assert!(!sketch_def(&d, s).sketch.is_text_shaped(t));
    let before = d.snapshot();
    let edits = d.edit_count();
    let model = d.model_edit_count();
    let version = d.feature(s).unwrap().version();
    let data = sketch_def(&d, s).sketch.to_data();
    let revision = sketch_def(&d, s).sketch.revision();
    let vol = total_volume(&d);

    assert!(!d.reshape_texts(&lib), "dieselbe Schrift, dieselbe Form");
    assert!(sketch_def(&d, s).sketch.is_text_shaped(t));
    assert_eq!(d.edit_count(), edits, "nicht „geändert“");
    assert_eq!(d.model_edit_count(), model);
    assert_eq!(d.feature(s).unwrap().version(), version, "kein neuer Stand");
    assert!(
        sketch_def(&d, s).sketch.to_data() == data,
        "dieselben Daten"
    );
    assert_eq!(sketch_def(&d, s).sketch.revision(), revision);
    assert!(d.snapshot().same_content(&before), "gleiches Bild");
    assert!(d.feature(s).unwrap().warning().is_none());
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    assert!((total_volume(&d) - vol).abs() < 1e-9);

    // Ein zweites Neusetzen ändert ebenso nichts.
    assert!(!d.reshape_texts(&lib));
    assert_eq!(d.edit_count(), edits);
    d.check_invariants().unwrap();
}

/// Setzt jedes sichtbare Zeichen als Quadrat 3 × 3 im Vorschub 4 (eine Schrift, die nach dem
/// Laden fehlt).
struct SquareShaper;

impl TextShaper for SquareShaper {
    fn shape(&self, content: &str, _style: &TextStyle) -> Result<TextShape, TextShapeError> {
        let v = DVec2::new;
        let glyphs: Vec<Glyph> = content
            .chars()
            .enumerate()
            .map(|(i, _)| Glyph {
                origin: v(4.0 * i as f64, 0.0),
                advance: 4.0,
                contours: vec![Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(3.0, 0.0)),
                        Seg::Line(v(3.0, 3.0)),
                        Seg::Line(v(0.0, 3.0)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                }],
            })
            .collect();
        let width = 4.0 * glyphs.len() as f64;
        Ok(TextShape {
            lines: vec![ShapedLine { glyphs, width }],
            ascender: 4.0,
            descender: -1.0,
            line_height: 5.0,
            substituted: None,
            synthetic: false,
            missing_chars: false,
        })
    }
}

#[test]
fn reshape_after_load_with_missing_font_changes_shape() {
    // Gespeichert mit einer Schrift, die auf diesem Rechner fehlt: Das Neusetzen mit Montserrat
    // ändert die Form, also ist das Dokument geändert (Spec G4), und die Warnung erscheint.
    let lib = FontLibrary::bundled_only();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let t = add_text(&mut sk, "Ho", MISSING, &SquareShaper);
    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", sketch_kind(loaded(&sk))).unwrap();
    assert!(d.feature(s).unwrap().warning().is_none());
    let edits = d.edit_count();

    assert!(d.reshape_texts(&lib), "andere Form");
    assert_ne!(d.edit_count(), edits, "das Dokument gilt als geändert");
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert_eq!(f.warning(), Some(font_warning(MISSING, 1).as_str()));
    let result = d.sketch_of(s).unwrap();
    assert!(
        result
            .text(t)
            .unwrap()
            .curves
            .iter()
            .any(|c| !matches!(result.curve(*c).unwrap().kind, CurveKind::Line { .. })),
        "Montserrat hat Bögen"
    );
    assert_eq!(
        sketch_def(&d, s).sketch.text(t).unwrap().style.font,
        MISSING
    );
    d.check_invariants().unwrap();
}

/// Erste Linienkante des Körpers, verknüpft projiziert in `sk`.
fn project_some_line(sk: &mut Sketch, solid: &Solid) -> (CurveId, EdgeRef) {
    let tol = Tolerance::default();
    for (id, e) in solid.edges.iter() {
        let Ok(r) = EdgeRef::from_edge(solid, id, &tol) else {
            continue;
        };
        let mut trial = sk.clone();
        let Ok(c) = trial.add_projected_curve(
            &e.curve,
            e.t0,
            e.t1,
            ProjectStyle {
                construction: false,
            },
        ) else {
            continue;
        };
        if matches!(trial.curve(c).unwrap().kind, CurveKind::Line { .. }) {
            *sk = trial;
            return (c, r);
        }
    }
    panic!("keine Linienkante");
}

#[test]
fn font_and_projection_warnings_both_show() {
    let lib = FontLibrary::bundled_only();
    let mut d = Document::new();
    let b = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(10.0, 20.0, 5.0),
                origin: DVec3::new(0.0, 50.0, 0.0),
            },
        )
        .unwrap();
    let solid = d
        .body(BodyRef {
            feature: b,
            index: 0,
        })
        .unwrap()
        .clone();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let (curve, edge) = project_some_line(&mut sk, &solid);
    add_text(&mut sk, "Ho", MISSING, &lib);
    let s = d
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![EdgeCopy {
                    curve,
                    source: CopySource::Edge(edge),
                }],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    assert_eq!(
        d.feature(s).unwrap().warning(),
        Some(font_warning(MISSING, 1).as_str())
    );
    // Quelle unterdrückt: Der Bezug ist verloren, beide Warnungen stehen da.
    d.set_suppressed(b, true).unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    let w = f.warning().expect("Warnungen");
    assert!(
        w.contains(&planora_i18n::tr!(
            "feature-sketch-projection-lost",
            count = 1
        )),
        "{w}"
    );
    assert!(w.contains(&font_warning(MISSING, 1)), "{w}");
}

/// Der Setzer lehnt ab, weil die Ersatzschrift zu viele Kurven ergäbe (wie `planora-text`).
struct TooManyCurves;

impl TextShaper for TooManyCurves {
    fn shape(&self, _content: &str, _style: &TextStyle) -> Result<TextShape, TextShapeError> {
        Err(TextShapeError::TooManyCurves)
    }
}

fn unset_warning(count: usize) -> String {
    planora_i18n::tr!("feature-sketch-text-unset", count = count)
}

#[test]
fn text_that_cannot_be_set_keeps_curves_and_warns() {
    // Gespeichert mit einer Schrift, die hier fehlt; weder Montserrat noch Roboto haben die
    // Zeichen. Die Ersatzschrift ergäbe keine einzige Kurve: Der Text darf nicht verschwinden
    // (Anti-Ziel), er behält seine Kurven, die Extrusion bleibt, und die Skizze warnt.
    let lib = FontLibrary::bundled_only();
    let content = "漢字";
    let probe = lib.shape(content, &style(MISSING)).unwrap();
    assert_eq!(probe.segment_count(), 0, "Voraussetzung: nichts zu setzen");
    let refusing: [(&str, &dyn TextShaper); 2] = [
        ("Ersatzschrift ohne die Zeichen", &lib),
        ("zu viele Kurven", &TooManyCurves),
    ];
    for (what, shaper) in refusing {
        let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
        let t = add_text(&mut sk, content, MISSING, &SquareShaper);
        let refs: Vec<_> = sk.profiles().refs;
        assert_eq!(refs.len(), 2, "zwei Quadrate");
        let mut d = Document::new();
        let s = d.add_feature("Skizze 1", sketch_kind(loaded(&sk))).unwrap();
        let e = d
            .add_feature(
                "Extrusion 1",
                FeatureKind::Extrude {
                    sketch: s,
                    profiles: ProfileSel::Refs(refs),
                    height: 2.0,
                },
            )
            .unwrap();
        let data = sketch_def(&d, s).sketch.to_data();
        let vol = total_volume(&d);
        assert!(vol > 1.0, "{what}: {vol}");
        let edits = d.edit_count();

        assert!(!d.reshape_texts(shaper), "{what}: die Kurven bleiben");
        assert_eq!(d.edit_count(), edits, "{what}: nicht „geändert“");
        assert!(sketch_def(&d, s).sketch.to_data() == data, "{what}");
        assert!(sketch_def(&d, s).sketch.text(t).unwrap().is_unset());
        let f = d.feature(s).unwrap();
        assert_eq!(f.status, FeatureStatus::Ok, "{what}: {:?}", f.status);
        assert_eq!(f.warning(), Some(unset_warning(1).as_str()), "{what}");
        assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok, "{what}");
        assert!((total_volume(&d) - vol).abs() < 1e-9, "{what}");
        let result = d.sketch_of(s).unwrap();
        assert_eq!(result.text(t).unwrap().curves.len(), 8, "{what}");

        // Mit einer Schrift, die setzt, fällt die Warnung wieder weg.
        assert!(!d.reshape_texts(&SquareShaper), "{what}: gleiche Form");
        assert_eq!(d.feature(s).unwrap().warning(), None, "{what}");
        d.check_invariants().unwrap();
    }
}
