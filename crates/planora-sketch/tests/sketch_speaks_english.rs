//! Alle Fehlermeldungen von planora-sketch erscheinen unter Englisch ohne deutsche Zeichen und
//! ohne Schlüssel-Ids. Dieser Test ruft nie `set_global`.

use planora_i18n::{english, german, with_lang};
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::{Drag, Sketch, SketchError};

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
    let errors = [
        SketchError::InvalidInput("x".into()),
        SketchError::UnknownObject,
        SketchError::Degenerate("x".into()),
        SketchError::WrongType("x".into()),
        SketchError::OpenLoop,
        SketchError::AmbiguousTopology("x".into()),
        SketchError::SelfIntersecting,
        SketchError::AmbiguousNesting,
        SketchError::UnsupportedForSolid("x".into()),
        SketchError::UnsupportedProjection("x".into()),
        SketchError::Kernel("x".into()),
    ];
    for e in &errors {
        let text = with_lang(&english(), || e.to_string());
        assert_english("SketchError", &text);
        if !matches!(
            e,
            SketchError::UnknownObject
                | SketchError::OpenLoop
                | SketchError::SelfIntersecting
                | SketchError::AmbiguousNesting
        ) {
            assert!(text.contains('x'), "{text}");
        }
    }
}

/// Fehler aus echten Aufrufen; die Texte entstehen beim Anlegen, also in der Sprache, die dann
/// gilt.
fn real_failures() -> Vec<SketchError> {
    let mut errors: Vec<SketchError> = Vec::new();
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let nan = f64::NAN;
    let mut note = |r: Result<(), SketchError>| {
        if let Err(e) = r {
            errors.push(e);
        }
    };
    note(s.add_point(nan, 0.0).map(|_| ()));
    note(s.add_point(0.0, 1e12).map(|_| ()));
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let c = s.add_point(10.0, 10.0).unwrap();
    let d = s.add_point(0.0, 10.0).unwrap();
    note(s.add_line(a, a).map(|_| ()));
    let l = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    let l3 = s.add_line(c, d).unwrap();
    let circle = s.add_circle(a, 3.0).unwrap();
    note(s.add_circle(a, 0.0).map(|_| ()));
    note(s.add_circle(a, nan).map(|_| ()));
    note(s.add_arc(a, a, b).map(|_| ()));
    note(s.add_ellipse(a, DVec2::new(0.0, 0.0), 1.0).map(|_| ()));
    note(s.add_ellipse(a, DVec2::new(5.0, 0.0), 9.0).map(|_| ()));
    note(s.add_spline(&[a]).map(|_| ()));
    note(s.add_spline(&[a, a]).map(|_| ()));
    note(s.add_constraint(K::Coincident(a, a)).map(|_| ()));
    note(
        s.add_constraint(K::Horizontal(LineOrPoints::Line(circle)))
            .map(|_| ()),
    );
    note(s.add_constraint(K::Parallel(l, l)).map(|_| ()));
    note(s.add_constraint(K::Parallel(l, circle)).map(|_| ()));
    note(s.add_constraint(K::Equal(l, circle)).map(|_| ()));
    note(s.add_constraint(K::Symmetric(a, b, circle)).map(|_| ()));
    note(s.add_constraint(K::Distance(a, a, 1.0)).map(|_| ()));
    note(s.add_constraint(K::Distance(a, b, -1.0)).map(|_| ()));
    note(s.add_constraint(K::Distance(a, b, nan)).map(|_| ()));
    note(s.add_constraint(K::Radius(l, 1.0)).map(|_| ()));
    note(s.add_constraint(K::Radius(circle, 0.0)).map(|_| ()));
    note(s.add_constraint(K::Angle(l, l2, nan)).map(|_| ()));
    note(s.add_constraint(K::EllipseMajor(l, 1.0)).map(|_| ()));
    note(s.add_constraint(K::Tangent(l, l2)).map(|_| ()));
    note(s.add_mirrored(&[], l).map(|_| ()));
    note(s.add_mirrored(&[l, l], l3).map(|_| ()));
    note(s.add_mirrored(&[l2], circle).map(|_| ()));
    note(
        s.add_linear_pattern(&[l], DVec2::new(1.0, 0.0), 0)
            .map(|_| ()),
    );
    note(
        s.add_linear_pattern(&[l], DVec2::new(0.0, 0.0), 2)
            .map(|_| ()),
    );
    note(
        s.add_linear_pattern(&[l], DVec2::new(nan, 0.0), 2)
            .map(|_| ()),
    );
    note(
        s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(0.0, 5.0))
            .map(|_| ()),
    );
    note(s.set_point_xy(a, nan, 0.0));
    note(
        s.drag(&[Drag {
            point: a,
            target: DVec2::new(nan, 0.0),
        }])
        .map(|_| ()),
    );
    note(s.set_frame(Frame::WORLD).map(|_| ()));
    // Beschädigte Rohdaten.
    let good = s.to_data();
    let mut broken = good.clone();
    broken.tolerance.linear = nan;
    note(Sketch::from_data(broken).map(|_| ()));
    let mut broken = good.clone();
    broken.vars[0] = nan;
    note(Sketch::from_data(broken).map(|_| ()));
    let mut broken = good.clone();
    broken.points[0].x = 10_000;
    note(Sketch::from_data(broken).map(|_| ()));
    let mut broken = good.clone();
    broken.points[1].alive = false;
    note(Sketch::from_data(broken).map(|_| ()));
    // Skizze zur Prüfung der Profile: offene Kette.
    let open = s.profiles();
    for r in open.rejected {
        errors.push(r.reason);
    }
    errors
}

#[test]
fn real_failures_speak_english_and_german() {
    let english_errors = with_lang(&english(), real_failures);
    let german_errors = with_lang(&german(), real_failures);
    assert!(
        english_errors.len() >= 25,
        "zu wenige Fehlerfälle: {}",
        english_errors.len()
    );
    assert_eq!(english_errors.len(), german_errors.len());
    for (e, g) in english_errors.iter().zip(&german_errors) {
        let en = with_lang(&english(), || e.to_string());
        assert_english("Fehlerfall", &en);
        let de = with_lang(&german(), || g.to_string());
        assert!(!de.is_empty() && !looks_like_key(&de), "{de}");
        assert_ne!(en, de, "{en}");
    }
}
