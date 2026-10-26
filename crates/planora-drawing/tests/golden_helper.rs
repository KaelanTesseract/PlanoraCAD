//! Prüfung des Fixturen-Vergleichs `common::golden`: Er nimmt nur Rundungsrauschen der
//! Systembibliothek hin (Werte aus der echten Linux-Ausgabe der CI), keine echte Abweichung.

mod common;

use common::golden::{same_up_to_last_digit, without_pdf_offsets};

fn same(a: &str, b: &str) -> bool {
    same_up_to_last_digit(a, b).is_ok() && same_up_to_last_digit(b, a).is_ok()
}

#[test]
fn rounding_noise_of_the_system_library_passes() {
    // stage15 unter Linux: Ellipse der Isometrie.
    assert!(same(
        "major: DVec2(8.0, 0.0), ratio: 0.5773502691896258",
        "major: DVec2(7.999999999999999, 0.0), ratio: 0.577350269189626"
    ));
    // stage34 unter Linux: Schraffurlinie im DXF (9 Stellen, Nullen abgeschnitten).
    assert!(same(
        " 21\r\n67.707442081\r\n 31",
        " 21\r\n67.70744208\r\n 31"
    ));
    assert!(same("x 8\r\n", "x 7.999999999\r\n"));
    assert!(same("gleich 1 F1 abc", "gleich 1 F1 abc"));
}

#[test]
fn real_differences_fail() {
    for (a, b) in [
        ("8.0", "8.1"),
        ("67.5", "67.6"),
        ("100.0", "100.1"),
        ("3", "3.1"),
        ("8", "7.9999999"),
        // Vorletzte Stelle einer langen Zahl.
        ("0.5773502691896258", "0.5773502691896248"),
        ("1", "2"),
        ("F1", "F2"),
        ("x 1", "x a"),
        ("0.5", "-0.5"),
        ("1e-7", "1"),
        ("1.5e-7", "1.5e-6"),
        ("1 2", "1 2 3"),
        ("1 2", "1 2 "),
    ] {
        assert!(!same(a, b), "{a:?} gegen {b:?} müsste auffallen");
    }
}

#[test]
fn pdf_offsets_are_masked_but_structure_is_not() {
    let pdf = "%PDF-1.4\n1 0 obj\n<< /Type /Page >>\nendobj\n2 0 obj\n<< /Length 12 >>\n\
               stream\n0 0 m 1.5 2 l\nendstream\nendobj\nxref\n0 3\n0000000000 65535 f \n\
               0000000009 00000 n \n0000000045 00000 n \ntrailer\n<< /Size 3 /Root 1 0 R >>\n\
               startxref\n123\n%%EOF\n";
    let shifted = pdf
        .replace("/Length 12", "/Length 13")
        .replace("0000000045 00000 n", "0000000046 00000 n")
        .replace("startxref\n123\n", "startxref\n124\n");
    assert!(same(
        &without_pdf_offsets(pdf),
        &without_pdf_offsets(&shifted)
    ));
    for changed in [
        pdf.replace("/Type /Page", "/Type /Pages"),
        pdf.replace("/Size 3", "/Size 4"),
        pdf.replace("1.5 2 l", "1.6 2 l"),
        pdf.replace("0 3\n", "0 4\n"),
    ] {
        assert!(
            !same(&without_pdf_offsets(pdf), &without_pdf_offsets(&changed)),
            "{changed}"
        );
    }
}
