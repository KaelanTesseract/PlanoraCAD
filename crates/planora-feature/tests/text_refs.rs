//! Skizzen 11b, Task 11: Parameter im Text. `{Name}` im Inhalt eines Skizzentextes wird durch den
//! Wert des Parameters ersetzt (Länge in der Dokumenteinheit, ohne Einheitenzeichen); der Text
//! hält den Rohinhalt, die Kurven entstehen aus dem gezeigten Inhalt (`TextData::shown`, nur zur
//! Laufzeit). Ersetzt und neu gesetzt wird in [`Document::reshape_texts`]; die Neuberechnung
//! prüft nur die Verweise und warnt.
//!
//! Gesetzt wird mit den mitgelieferten Schriften ([`FontLibrary::bundled_only`]).

use planora_feature::text_refs::{apply_shown, param_refs, rename_refs, substitute};
use planora_feature::*;
use planora_i18n::{DisplayUnit, english, german, tr, with_lang, with_unit};
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{TextShaper, TextStyle};
use planora_text::FontLibrary;
use std::sync::Arc;

fn style() -> TextStyle {
    TextStyle {
        font: "Montserrat".into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    }
}

/// Text `content` (gesetzt, wie getippt) im Rahmen (0, 0)–(80, 10), Höhe 5.
fn add_text(sk: &mut Sketch, content: &str, shaper: &dyn TextShaper) -> TextId {
    let shape = shaper.shape(content, &style()).unwrap();
    sk.add_text_box(
        DVec2::new(0.0, 0.0),
        DVec2::new(80.0, 10.0),
        TextParams {
            content: content.into(),
            style: style(),
            height: 5.0,
            flip_h: false,
            flip_v: false,
        },
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
        text_formulas: Default::default(),
    })
}

fn sketch_def(d: &Document, s: FeatureId) -> &SketchFeature {
    match &d.feature(s).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

fn length(d: &mut Document, name: &str, formula: &str) -> ParamId {
    d.add_parameter(name, formula, ParamUnit::Length, "")
        .unwrap()
}

/// Dokument mit Parameter `Breite` = `breite` und einer Skizze mit dem Text `content`.
fn doc(breite: &str, content: &str, lib: &FontLibrary) -> (Document, ParamId, FeatureId, TextId) {
    let mut d = Document::new();
    let p = length(&mut d, "Breite", breite);
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let t = add_text(&mut sk, content, lib);
    let s = d.add_feature("Skizze 1", sketch_kind(sk)).unwrap();
    (d, p, s, t)
}

fn shown(d: &Document, s: FeatureId, t: TextId) -> Option<String> {
    sketch_def(d, s).sketch.text(t).unwrap().shown.clone()
}

/// Lage aller Punkte des Textes in der Definition.
fn text_points(sk: &Sketch, t: TextId) -> Vec<DVec2> {
    sk.text(t)
        .unwrap()
        .points
        .iter()
        .map(|p| sk.point_xy(*p).unwrap())
        .collect()
}

/// Die Punkte eines unmittelbar mit `content` gesetzten Textes der Höhe `height`.
fn reference_points(content: &str, height: f64, lib: &FontLibrary) -> Vec<DVec2> {
    let mut sk = Sketch::new(Frame::WORLD, "Vergleich", Tolerance::default());
    let t = add_text(&mut sk, content, lib);
    if height != 5.0 {
        sk.set_text_height(t, height).unwrap();
    }
    text_points(&sk, t)
}

fn same_points(a: &[DVec2], b: &[DVec2]) {
    assert_eq!(a.len(), b.len(), "gleich viele Punkte");
    for (x, y) in a.iter().zip(b) {
        assert!((*x - *y).length() < 1e-9, "{x:?} statt {y:?}");
    }
}

#[test]
fn param_refs_in_order() {
    assert_eq!(
        param_refs("{Breite} x {hoehe} {} {1a} {a b} {Breite}"),
        vec!["Breite".to_string(), "hoehe".into(), "Breite".into()]
    );
    assert!(param_refs("ohne Klammern").is_empty());
    assert_eq!(param_refs("{{Tiefe}}"), vec!["Tiefe".to_string()]);
}

#[test]
fn breite_mm_text() {
    let lib = FontLibrary::bundled_only();
    for (lang, want) in [(german(), "20,00 mm"), (english(), "20.00 mm")] {
        with_lang(&lang, || {
            let (mut d, _, s, t) = doc("20", "{Breite} mm", &lib);
            assert!(d.reshape_texts(&lib));
            assert_eq!(shown(&d, s, t).as_deref(), Some(want));
            let sk = &sketch_def(&d, s).sketch;
            // Der Rohinhalt bleibt; die Kurven sind die des gezeigten Inhalts.
            assert_eq!(sk.text(t).unwrap().content, "{Breite} mm");
            same_points(&text_points(sk, t), &reference_points(want, 5.0, &lib));
            assert_eq!(d.feature(s).unwrap().status, FeatureStatus::Ok);
            assert!(d.feature(s).unwrap().warning().is_none());
            // Ein zweites Neusetzen ändert nichts.
            assert!(!d.reshape_texts(&lib));
        });
    }
}

#[test]
fn inch_unit_shows_inch_value() {
    with_lang(&german(), || {
        with_unit(DisplayUnit::Zoll, || {
            let mut d = Document::new();
            length(&mut d, "Breite", "25.4");
            let scope = d.param_scope();
            assert_eq!(substitute("{Breite}", &scope), ("1,000".into(), vec![]));
        });
        // Winkel und Zahl mit zwei Nachkommastellen, ohne Einheitenzeichen.
        let mut d = Document::new();
        d.add_parameter("W", "45", ParamUnit::Angle, "").unwrap();
        d.add_parameter("N", "3", ParamUnit::Number, "").unwrap();
        let scope = d.param_scope();
        assert_eq!(
            substitute("{w}° / {N}", &scope),
            ("45,00° / 3,00".into(), vec![])
        );
    });
}

#[test]
fn unknown_name_stays_literal() {
    let lib = FontLibrary::bundled_only();
    with_lang(&german(), || {
        let (mut d, _, s, t) = doc("20", "{Breite} {Tiefe} {Hoehe}", &lib);
        let scope = d.param_scope();
        assert_eq!(
            substitute("{Breite} {Tiefe} {Hoehe} {tiefe}", &scope),
            (
                "20,00 {Tiefe} {Hoehe} {tiefe}".into(),
                vec!["Tiefe".to_string(), "Hoehe".into(), "tiefe".into()]
            )
        );
        d.reshape_texts(&lib);
        assert_eq!(shown(&d, s, t).as_deref(), Some("20,00 {Tiefe} {Hoehe}"));
        let f = d.feature(s).unwrap();
        assert_eq!(f.status, FeatureStatus::Ok);
        let want = tr!("feature-sketch-text-param-unknown", names = "Tiefe, Hoehe");
        assert_eq!(f.warning(), Some(want.as_str()));
    });
}

#[test]
fn apply_shown_without_refs_is_none() {
    let lib = FontLibrary::bundled_only();
    let (mut d, _, s, t) = doc("20", "Ohne", &lib);
    let scope = d.param_scope();
    let mut sk = sketch_def(&d, s).sketch.clone();
    assert!(!apply_shown(&mut sk, &scope));
    assert_eq!(sk.text(t).unwrap().shown, None);
    assert!(!d.reshape_texts(&lib), "gleiche Form, nichts ändert sich");
}

#[test]
fn rename_updates_text() {
    let lib = FontLibrary::bundled_only();
    assert_eq!(
        rename_refs("{breite} mm, {Breiten}, Breite, {BREITE}", "Breite", "Neu"),
        "{Neu} mm, {Breiten}, Breite, {Neu}"
    );
    with_lang(&german(), || {
        let (mut d, p, s, t) = doc("20", "{Breite} mm", &lib);
        d.reshape_texts(&lib);
        let before = text_points(&sketch_def(&d, s).sketch, t);
        let edits = d.model_edit_count();
        d.set_parameter(
            p,
            ParamEdit {
                name: "Neu".into(),
                formula: "20".into(),
                unit: ParamUnit::Length,
                comment: String::new(),
            },
        )
        .unwrap();
        assert_eq!(d.model_edit_count(), edits + 1, "ein Schritt");
        let sk = &sketch_def(&d, s).sketch;
        assert_eq!(sk.text(t).unwrap().content, "{Neu} mm");
        // Die gespeicherten Daten tragen den neuen Rohinhalt.
        assert_eq!(sk.to_data().texts[0].content, "{Neu} mm");
        assert!(!d.reshape_texts(&lib), "gleicher Wert, gleiche Kurven");
        assert_eq!(shown(&d, s, t).as_deref(), Some("20,00 mm"));
        same_points(&text_points(&sketch_def(&d, s).sketch, t), &before);
        assert!(d.feature(s).unwrap().warning().is_none());
        // Das Dateiformat prüft `format22.rs` (`rename_keeps_old_format`).
    });
}

#[test]
fn deleted_parameter_leaves_literal_and_warning() {
    let lib = FontLibrary::bundled_only();
    with_lang(&german(), || {
        let (mut d, p, s, t) = doc("20", "{Breite} mm", &lib);
        d.reshape_texts(&lib);
        let sk = &sketch_def(&d, s).sketch;
        let refs: Vec<_> = sk
            .profiles()
            .refs
            .into_iter()
            .filter(|r| sk.text_of_profile(r) == Some(t))
            .collect();
        assert!(!refs.is_empty());
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
        // Text-Verweise sperren das Ersetzen nicht (sie sind keine Formeln).
        d.replace_parameter_by_value(p).unwrap();
        d.reshape_texts(&lib);
        assert_eq!(shown(&d, s, t).as_deref(), Some("{Breite} mm"));
        let f = d.feature(s).unwrap();
        assert_eq!(f.status, FeatureStatus::Ok);
        let want = tr!("feature-sketch-text-param-unknown", names = "Breite");
        assert_eq!(f.warning(), Some(want.as_str()));
        let e = d.feature(e).unwrap();
        assert_eq!(e.status, FeatureStatus::Ok, "{:?}", e.status);
        assert!(!d.visible_bodies().is_empty());
    });
}

#[test]
fn shown_never_written() {
    let lib = FontLibrary::bundled_only();
    with_lang(&german(), || {
        let (mut d, _, s, t) = doc("20", "{Breite} mm", &lib);
        d.reshape_texts(&lib);
        assert_eq!(shown(&d, s, t).as_deref(), Some("20,00 mm"));
        let snap = d.snapshot();
        let FeatureKind::Sketch(sf) = &snap.features[0].kind else {
            panic!("keine Skizze")
        };
        let data = sf.sketch.to_data();
        assert_eq!(data.texts[0].content, "{Breite} mm");
        assert!(!format!("{data:?}").contains("20,00"));
        // Geladen ist der Text nicht gesetzt und zeigt noch nichts Ersetztes.
        let back = Sketch::from_data(data).unwrap();
        assert_eq!(back.text(t).unwrap().shown, None);
        // Die Datei selbst prüft `format22.rs` (`shown_never_in_file`).
    });
}

#[test]
fn parameter_change_with_height_formula() {
    // Texthöhe als Formel (Task 10) und Parameter im Inhalt zusammen: Die Formelrunde setzt die
    // Höhe, `reshape_texts` den Inhalt; am Ende stimmen beide.
    let lib = FontLibrary::bundled_only();
    with_lang(&german(), || {
        let mut d = Document::new();
        let b = length(&mut d, "Breite", "20");
        let h = length(&mut d, "H", "5");
        let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
        let t = add_text(&mut sk, "{Breite} mm", &lib);
        let FeatureKind::Sketch(mut sf) = sketch_kind(sk) else {
            unreachable!()
        };
        sf.text_formulas.insert(t, "H".into());
        let s = d.add_feature("Skizze 1", FeatureKind::Sketch(sf)).unwrap();
        d.reshape_texts(&lib);
        same_points(
            &text_points(&sketch_def(&d, s).sketch, t),
            &reference_points("20,00 mm", 5.0, &lib),
        );
        for (id, name, formula) in [(b, "Breite", "30"), (h, "H", "8")] {
            d.set_parameter(
                id,
                ParamEdit {
                    name: name.into(),
                    formula: formula.into(),
                    unit: ParamUnit::Length,
                    comment: String::new(),
                },
            )
            .unwrap();
        }
        assert!(d.reshape_texts(&lib));
        let sk = &sketch_def(&d, s).sketch;
        assert_eq!(sk.text(t).unwrap().height, 8.0);
        assert_eq!(shown(&d, s, t).as_deref(), Some("30,00 mm"));
        same_points(
            &text_points(sk, t),
            &reference_points("30,00 mm", 8.0, &lib),
        );
        // Nur die Höhe ändert sich: die Formelrunde allein legt den Text neu, der Inhalt bleibt.
        d.set_parameter(
            h,
            ParamEdit {
                name: "H".into(),
                formula: "6".into(),
                unit: ParamUnit::Length,
                comment: String::new(),
            },
        )
        .unwrap();
        d.reshape_texts(&lib);
        let sk = &sketch_def(&d, s).sketch;
        assert_eq!(sk.text(t).unwrap().height, 6.0);
        same_points(
            &text_points(sk, t),
            &reference_points("30,00 mm", 6.0, &lib),
        );
        assert_eq!(d.feature(s).unwrap().status, FeatureStatus::Ok);
    });
}
