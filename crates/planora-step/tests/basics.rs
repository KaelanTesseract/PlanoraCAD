//! Fehler, Zahlen und Zeichenketten (Teilprojekt 10, Task 1).

use planora_step::*;

/// Kleiner Zufallsgenerator mit festem Startwert (keine Abhängigkeit).
struct Lcg(u64);

impl Lcg {
    fn bits(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
}

/// Liest ein STEP-REAL so, wie es ein fremdes Programm tut: Punkt Pflicht, `E` groß.
fn read_back(s: &str) -> f64 {
    assert!(s.contains('.'), "kein Punkt in {s}");
    assert!(!s.contains('e'), "kleines e in {s}");
    let (mantissa, _) = s.split_once('E').unwrap_or((s, ""));
    let (whole, _) = mantissa.split_once('.').unwrap();
    let digits = whole.trim_start_matches('-');
    assert!(
        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
        "vor dem Punkt fehlen Ziffern: {s}"
    );
    s.replace('E', "e").parse().unwrap()
}

#[test]
fn reals_always_have_a_point_and_read_back_exactly() {
    for (x, want) in [
        (1.0, "1."),
        (0.5, "0.5"),
        (-2.25, "-2.25"),
        (0.0, "0."),
        (-0.0, "0."),
        (1e-7, "1.E-7"),
        (1.5e21, "1.5E21"),
        (1e16, "1.E16"),
        (-3e-300, "-3.E-300"),
        (123456.789, "123456.789"),
        (f64::MAX, "1.7976931348623157E308"),
    ] {
        assert_eq!(real(x).unwrap(), want, "{x}");
    }
    let mut rng = Lcg(7);
    let mut checked = 0;
    while checked < 20_000 {
        let x = f64::from_bits(rng.bits());
        if !x.is_finite() {
            continue;
        }
        let s = real(x).unwrap();
        let back = read_back(&s);
        let want = if x == 0.0 { 0.0 } else { x };
        assert_eq!(back.to_bits(), want.to_bits(), "{x:?} → {s}");
        // Gültige Stellen: Ziffern der Mantisse ohne führende Nullen.
        let digits: String = s
            .split('E')
            .next()
            .unwrap()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let significant = digits.trim_start_matches('0').len();
        assert!(significant <= 17, "{s}: zu viele Stellen");
        checked += 1;
    }
}

#[test]
fn non_finite_numbers_are_refused_and_leave_the_text_alone() {
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(real(x), Err(StepError::NotFinite));
        let mut s = String::from("(1.,");
        assert_eq!(push_real(&mut s, x), Err(StepError::NotFinite));
        assert_eq!(s, "(1.,");
    }
}

#[test]
fn strings_are_quoted_and_escaped() {
    let t = |s: &str| step_string(s).text;
    assert_eq!(t("Quader 1"), "'Quader 1'");
    assert_eq!(t(""), "''");
    assert_eq!(t("O'Brien"), "'O''Brien'");
    assert_eq!(t("a\\b"), "'a\\\\b'");
    assert_eq!(t("Würfel"), "'W\\X2\\00FC\\X0\\rfel'");
    assert_eq!(t("Maß äö"), "'Ma\\X2\\00DF\\X0\\ \\X2\\00E400F6\\X0\\'");
    assert_eq!(t("Zeile\nzwei\t\u{7f}"), "'Zeile zwei  '");
    assert_eq!(t("\u{1F600}x"), "'\\X4\\0001F600\\X0\\x'");
    assert_eq!(t("é\u{1F600}"), "'\\X2\\00E9\\X0\\\\X4\\0001F600\\X0\\'");
    for name in ["Quader 1", "O'Brien", "Würfel", "\u{1F600}", ""] {
        let s = step_string(name);
        assert!(!s.shortened);
        assert!(
            s.text.chars().all(|c| (' '..='~').contains(&c)),
            "{}",
            s.text
        );
    }
}

#[test]
fn long_names_are_shortened_to_the_limit() {
    assert_eq!(MAX_NAME_CHARS, 256);
    let exact = "x".repeat(MAX_NAME_CHARS);
    let s = step_string(&exact);
    assert!(!s.shortened);
    assert_eq!(s.text.len(), MAX_NAME_CHARS + 2);
    let long = "ä".repeat(MAX_NAME_CHARS + 1);
    let s = step_string(&long);
    assert!(s.shortened);
    assert_eq!(s.text.matches("00E4").count(), MAX_NAME_CHARS);
}

#[test]
fn error_texts_are_german() {
    assert_eq!(StepError::Cancelled.to_string(), "Abgebrochen");
    let e = StepError::InvalidBody {
        name: "Quader 1".into(),
        reason: "Die Hülle ist offen".into(),
    };
    assert_eq!(
        e.to_string(),
        "„Quader 1“ wird nicht ausgegeben: Die Hülle ist offen"
    );
    assert!(
        StepError::TooManyEntities
            .to_string()
            .contains("500 Millionen")
    );
    assert!(
        StepError::NothingToExport
            .to_string()
            .contains("keinen Körper")
    );
    assert!(
        StepError::NotFinite
            .to_string()
            .contains("keine gültige Zahl")
    );
    let io: StepError = std::io::Error::other("voll").into();
    assert!(matches!(io, StepError::Io { .. }));
}
