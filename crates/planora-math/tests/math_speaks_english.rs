//! Alle Fehlermeldungen von planora-math erscheinen unter Englisch ohne deutsche Zeichen und
//! ohne Schlüssel-Ids. Dieser Test ruft nie `set_global`.

use planora_i18n::{english, german, with_lang};
use planora_math::nurbs::{KnotVector, NurbsCurve, NurbsError, skin};
use planora_math::{DVec3, RadiusLaw, RadiusLawError};

fn looks_like_key(s: &str) -> bool {
    s.contains('-')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && s.starts_with(|c: char| c.is_ascii_lowercase())
}

fn assert_english(what: &str, text: &str) {
    assert!(!text.is_empty(), "{what}: leer");
    assert!(
        !text.chars().any(|c| "äöüßÄÖÜ„".contains(c)),
        "{what}: deutsche Zeichen in \"{text}\""
    );
    assert!(
        !looks_like_key(text),
        "{what}: sieht wie eine Id aus: {text}"
    );
}

#[test]
fn every_variant_speaks_english() {
    let errors = vec![
        NurbsError::Degree(0),
        NurbsError::Knots("x".into()),
        NurbsError::Control("x".into()),
        NurbsError::Weights("x".into()),
        NurbsError::Fit("x".into()),
        NurbsError::Input("x".into()),
        NurbsError::TooLarge("x".into()),
    ];
    for e in &errors {
        let text = with_lang(&english(), || e.to_string());
        assert_english("NurbsError", &text);
        assert!(
            text.contains('x') || matches!(e, NurbsError::Degree(_)),
            "{text}"
        );
    }
    for e in [
        RadiusLawError::TooFewPoints,
        RadiusLawError::Positions,
        RadiusLawError::Radii,
    ] {
        let text = with_lang(&english(), || e.to_string());
        assert_english("RadiusLawError", &text);
    }
}

/// Meldungen aus echten Fehlerfällen der Konstruktoren (die Texte entstehen tief in den
/// Prüfungen, nicht nur in den Fehlertypen).
/// Die Texte entstehen beim Anlegen des Fehlers, also in der Sprache, die dann gilt.
fn real_failures() -> Vec<NurbsError> {
    let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
    let nan = f64::NAN;
    let mut errors: Vec<NurbsError> = Vec::new();
    let mut push = |r: Result<(), NurbsError>| {
        if let Err(e) = r {
            errors.push(e);
        }
    };
    push(KnotVector::new(0, vec![0.0, 1.0]).map(|_| ()));
    push(KnotVector::new(2, vec![0.0]).map(|_| ()));
    push(KnotVector::new(1, vec![0.0, nan, 1.0, 1.0]).map(|_| ()));
    push(KnotVector::new(1, vec![0.0, 0.0, 2.0, 1.0, 1.0, 1.0]).map(|_| ()));
    push(KnotVector::new(1, vec![0.0, 0.5, 1.0, 1.0]).map(|_| ()));
    push(KnotVector::new(1, vec![1.0, 1.0, 1.0, 1.0]).map(|_| ()));
    push(KnotVector::new(1, vec![0.0, 0.0, 1e12, 1e12]).map(|_| ()));
    push(KnotVector::new(1, vec![0.0, 0.0, 1e-9, 1e-9]).map(|_| ()));
    push(
        NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![p(0.0, 0.0, 0.0)],
            vec![1.0],
        )
        .map(|_| ()),
    );
    push(
        NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0)],
            vec![1.0],
        )
        .map(|_| ()),
    );
    push(
        NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0)],
            vec![1.0, 1e9],
        )
        .map(|_| ()),
    );
    push(
        NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![p(0.0, 0.0, 0.0), p(1e12, 0.0, 0.0)],
            vec![1.0, 1.0],
        )
        .map(|_| ()),
    );
    push(NurbsCurve::fit(&[p(0.0, 0.0, 0.0)], 1e-3).map(|_| ()));
    push(NurbsCurve::fit(&[p(0.0, 0.0, 0.0), p(0.0, 0.0, 0.0)], 1e-3).map(|_| ()));
    push(NurbsCurve::fit(&[p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0)], nan).map(|_| ()));
    push(NurbsCurve::fit(&[p(0.0, 0.0, 0.0), p(1e12, 0.0, 0.0)], 1e-3).map(|_| ()));
    push(
        NurbsCurve::elliptic_arc(
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            0.0,
            1.0,
        )
        .map(|_| ()),
    );
    push(
        NurbsCurve::elliptic_arc(
            p(nan, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            0.0,
            1.0,
        )
        .map(|_| ()),
    );
    push(
        NurbsCurve::elliptic_arc(
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            0.0,
            1e-12,
        )
        .map(|_| ()),
    );
    push(skin(&[], &[], &[]).map(|_| ()));
    errors
}

#[test]
fn real_failures_speak_english_and_german() {
    let english_errors = with_lang(&english(), real_failures);
    let german_errors = with_lang(&german(), real_failures);
    assert!(
        english_errors.len() >= 15,
        "zu wenige Fehlerfälle: {}",
        english_errors.len()
    );
    assert_eq!(english_errors.len(), german_errors.len());
    for (e, g) in english_errors.iter().zip(&german_errors) {
        let en = with_lang(&english(), || e.to_string());
        assert_english("Fehlerfall", &en);
        // Deutsch bleibt deutsch (nicht Id, nicht leer) und unterscheidet sich vom Englischen.
        let de = with_lang(&german(), || g.to_string());
        assert!(!de.is_empty() && !looks_like_key(&de), "{de}");
        assert_ne!(en, de, "{en}");
    }
}

#[test]
fn radius_law_failures_speak_english() {
    let cases: [&[(f64, f64)]; 3] = [
        &[(0.0, 1.0)],
        &[(0.0, 1.0), (0.0, 2.0)],
        &[(0.0, 1.0), (1.0, -2.0)],
    ];
    for pts in cases {
        let e = RadiusLaw::new(pts).unwrap_err();
        let en = with_lang(&english(), || e.to_string());
        assert_english("RadiusLaw", &en);
    }
}
