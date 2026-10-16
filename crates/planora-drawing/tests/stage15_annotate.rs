//! Stufe 15 (Teilprojekt 12): Maße und Mittellinien als Markierungen auf dem Blatt. Alle
//! Sollwerte sind von Hand aus den Eingabepunkten ausgerechnet (Rechenweg im Kommentar), nicht
//! mit der Bibliothek.

mod common;

use common::mark_check::*;
use common::model::{box_with_hole, cylinder_solid};
use planora_drawing::annotate::{
    DimText, Mark, MarkKind, ResolvedDims, TextAnchor, dimension_text, format_value, group_base,
    layout_centerline, layout_dimension, layout_dimension_in, layout_lost,
};
use planora_drawing::centerline::{CenterLine, CenterSource};
use planora_drawing::dimension::{
    Anchor, ChainMode, ChainSpec, DimError, DimId, DimKind, DimSpec, Measured, Tolerance,
};
use planora_drawing::model::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::sheet::{SheetModel, SheetTransform, build_sheet, build_sheet_with};
use planora_drawing::stroke::{Stroke, StrokeOrigin};
use planora_drawing::{
    Basis, DrawBody, Piece, ViewId, ViewKind, ViewLines, Visibility, compute_view,
};
use planora_exchange::Progress;
use planora_kernel::Curve;
use planora_math::{DVec2, DVec3};
use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

const TOL: f64 = 1e-9;
/// √2/2.
const H2: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn id_xf() -> SheetTransform {
    SheetTransform {
        scale: 1.0,
        tx: 0.0,
        ty: 0.0,
        bbox_center: DVec2::ZERO,
    }
}

fn spec(kind: DimKind, offset: f64) -> DimSpec {
    DimSpec {
        id: DimId(1),
        view: ViewId(1),
        kind,
        refs: Vec::new(),
        offset,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: Tolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

/// Lineares Maß zwischen zwei Punkten (Spannen je zweimal der Punkt).
fn lin(a: DVec2, b: DVec2, dir: DVec2) -> Anchor {
    Anchor::Linear {
        a,
        b,
        dir,
        a_span: (a, a),
        b_span: (b, b),
    }
}

fn meas(value: f64, anchor: Anchor, prefix: Option<char>) -> Measured {
    Measured {
        value,
        anchor,
        prefix,
    }
}

fn one_text(marks: &[Mark]) -> T {
    let t = texts(marks);
    assert_eq!(t.len(), 1, "genau ein Text erwartet: {t:?}");
    t[0].clone()
}

fn all_kind(marks: &[Mark], kind: MarkKind) {
    assert!(!marks.is_empty());
    for m in marks {
        assert_eq!(kind_of(m), kind, "{m:?}");
    }
}

fn h_marks(a: DVec2, b: DVec2, value: f64, sp: &DimSpec) -> Vec<Mark> {
    layout_dimension(sp, &meas(value, lin(a, b, v(1.0, 0.0)), None), &id_xf(), 0)
}

// (a) A=(20,50), B=(50,50), offset 10: Maßlinie y = 50 + 10 = 60; Hilfslinien von
// 50 + 1 = 51 bis 60 + 2 = 62; Länge 30 ≥ 2·3,5 + 1 → Pfeile innen, Spitzen auf den
// Hilfslinien, Richtung vom Pfeil weg zur Maßlinie (links (1,0), rechts (−1,0)); Text „30,0“
// (Breite 0,6·3,5·4 = 8,4 ≤ 30 − 7) mittig bei ((20+50)/2, 60 + 1) = (35, 61).
#[test]
fn horizontal_dimension_geometry() {
    let sp = spec(DimKind::Horizontal, 10.0);
    for (a, b, dir) in [
        (v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0)),
        (v(50.0, 50.0), v(20.0, 50.0), v(1.0, 0.0)),
        (v(20.0, 50.0), v(50.0, 50.0), v(-1.0, 0.0)),
    ] {
        let m = layout_dimension(&sp, &meas(30.0, lin(a, b, dir), None), &id_xf(), 0);
        all_kind(&m, MarkKind::Dimension);
        assert_lines(
            &m,
            &[
                (v(20.0, 60.0), v(50.0, 60.0)),
                (v(20.0, 51.0), v(20.0, 62.0)),
                (v(50.0, 51.0), v(50.0, 62.0)),
            ],
            TOL,
        );
        assert_arrows(
            &m,
            &[(v(20.0, 60.0), v(1.0, 0.0)), (v(50.0, 60.0), v(-1.0, 0.0))],
            TOL,
        );
        let t = one_text(&m);
        assert!(close(t.pos, v(35.0, 61.0), TOL), "{t:?}");
        assert_eq!(t.height, 3.5);
        assert_eq!(t.angle, 0.0);
        assert_eq!(t.anchor, TextAnchor::Middle);
        assert_eq!(t.text, "30,0");
        assert!(m.iter().all(|x| !matches!(x, Mark::Arc { .. })));
    }
}

// (b) B − A = 6 < 8: Pfeile außen (Spitzen auf den Hilfslinien, Körper nach außen), Maßlinie
// um 5 mm je Seite verlängert: 15 … 31. Text „6,0“ (6,3 > 6 − 7) rechts: Start bei
// 26 + 3,5 + 2 = 31,5. Grenze: 8 → innen.
#[test]
fn short_dimension_puts_arrows_outside() {
    let sp = spec(DimKind::Horizontal, 10.0);
    let m = h_marks(v(20.0, 50.0), v(26.0, 50.0), 6.0, &sp);
    assert_lines(
        &m,
        &[
            (v(15.0, 60.0), v(31.0, 60.0)),
            (v(20.0, 51.0), v(20.0, 62.0)),
            (v(26.0, 51.0), v(26.0, 62.0)),
        ],
        TOL,
    );
    assert_arrows(
        &m,
        &[(v(20.0, 60.0), v(-1.0, 0.0)), (v(26.0, 60.0), v(1.0, 0.0))],
        TOL,
    );
    let t = one_text(&m);
    assert_eq!(t.anchor, TextAnchor::Start);
    assert!(close(t.pos, v(31.5, 61.0), TOL), "{t:?}");
    // Genau 8 mm: innen, Maßlinie von Spitze zu Spitze.
    let m = h_marks(v(20.0, 50.0), v(28.0, 50.0), 8.0, &sp);
    assert!(has_line(&m, v(20.0, 60.0), v(28.0, 60.0), TOL));
    assert_arrows(
        &m,
        &[(v(20.0, 60.0), v(1.0, 0.0)), (v(28.0, 60.0), v(-1.0, 0.0))],
        TOL,
    );
    // 7,99 mm: außen.
    let m = h_marks(v(20.0, 50.0), v(27.99, 50.0), 7.99, &sp);
    assert!(has_line(&m, v(15.0, 60.0), v(32.99, 60.0), 1e-9));
    assert!(has_arrow(&m, v(20.0, 60.0), v(-1.0, 0.0), TOL));
}

// (c) Text „ABCDEFG“: Breite 0,6·3,5·7 = 14,7 > 11 − 7 = 4 → rechts von B: Start bei
// 31 + 5,5 = 36,5, Höhe 61. Grenzen: 16 mm mit „16,0“ (8,4 ≤ 9) mittig; 15 mm mit „15,0“
// (8,4 > 8) rechts. Toleranz zweizeilig: Breite = Haupttext + ein Zeichen Abstand + breiterer
// Teil: 8,4 + 2,1 + 0,6·2,45·5 („−10,0“) = 17,85 → passt bei 24,9 (17,9), nicht bei 24,8 (17,8).
#[test]
fn text_that_does_not_fit_moves_right() {
    let mut sp = spec(DimKind::Horizontal, 10.0);
    sp.text_override = Some("ABCDEFG".into());
    let m = h_marks(v(20.0, 50.0), v(31.0, 50.0), 11.0, &sp);
    // 11 ≥ 8: Pfeile innen.
    assert!(has_arrow(&m, v(20.0, 60.0), v(1.0, 0.0), TOL));
    let t = one_text(&m);
    assert_eq!(t.text, "ABCDEFG");
    assert_eq!(t.anchor, TextAnchor::Start);
    assert!(close(t.pos, v(36.5, 61.0), TOL), "{t:?}");

    let sp = spec(DimKind::Horizontal, 10.0);
    let t = one_text(&h_marks(v(20.0, 50.0), v(36.0, 50.0), 16.0, &sp));
    assert_eq!(t.anchor, TextAnchor::Middle);
    assert!(close(t.pos, v(28.0, 61.0), TOL));
    let t = one_text(&h_marks(v(20.0, 50.0), v(35.0, 50.0), 15.0, &sp));
    assert_eq!(t.anchor, TextAnchor::Start);
    assert!(close(t.pos, v(40.5, 61.0), TOL));

    let mut sp = spec(DimKind::Horizontal, 10.0);
    sp.tolerance = Tolerance::Limits {
        upper: 0.2,
        lower: -10.0,
    };
    let dt = dimension_text(
        &sp,
        &meas(12.5, lin(v(0.0, 0.0), v(1.0, 0.0), v(1.0, 0.0)), None),
    );
    assert!((dt.width() - 17.85).abs() < 1e-9, "{}", dt.width());
    // Passt: Haupttext beginnt bei Mitte − 17,85/2.
    let m = h_marks(v(20.0, 50.0), v(44.9, 50.0), 12.5, &sp);
    let t = texts(&m);
    assert_eq!(t.len(), 3);
    let main = t.iter().find(|x| x.text == "12,5").unwrap();
    assert_eq!(main.anchor, TextAnchor::Start);
    assert!(close(main.pos, v(32.45 - 8.925, 61.0), 1e-9), "{main:?}");
    // Passt nicht: rechts von B (44,8 + 5,5 = 50,3).
    let m = h_marks(v(20.0, 50.0), v(44.8, 50.0), 12.5, &sp);
    let main = texts(&m).into_iter().find(|x| x.text == "12,5").unwrap();
    assert_eq!(main.anchor, TextAnchor::Start);
    assert!(close(main.pos, v(50.3, 61.0), 1e-9), "{main:?}");
}

// (d) Senkrecht: A=(40,20), B=(40,60), offset 10: Querachse nach rechts (1,0) → Maßlinie bei
// x = 40 + 10 = 50; Hilfslinien von x = 41 bis 52; Text um 90° gedreht (liest von rechts),
// „über“ der Linie in Leserichtung, also links: (50 − 1, 40) = (49, 40).
#[test]
fn vertical_dimension_rotates_text_90_degrees() {
    let sp = spec(DimKind::Vertical, 10.0);
    for (a, b, dir) in [
        (v(40.0, 20.0), v(40.0, 60.0), v(0.0, 1.0)),
        (v(40.0, 60.0), v(40.0, 20.0), v(0.0, -1.0)),
    ] {
        let m = layout_dimension(&sp, &meas(40.0, lin(a, b, dir), None), &id_xf(), 0);
        assert_lines(
            &m,
            &[
                (v(50.0, 20.0), v(50.0, 60.0)),
                (v(41.0, 20.0), v(52.0, 20.0)),
                (v(41.0, 60.0), v(52.0, 60.0)),
            ],
            TOL,
        );
        assert_arrows(
            &m,
            &[(v(50.0, 20.0), v(0.0, 1.0)), (v(50.0, 60.0), v(0.0, -1.0))],
            TOL,
        );
        let t = one_text(&m);
        assert!((t.angle - FRAC_PI_2).abs() < 1e-12, "{t:?}");
        assert!(close(t.pos, v(49.0, 40.0), TOL), "{t:?}");
        assert_eq!(t.anchor, TextAnchor::Middle);
        assert_eq!(t.text, "40,0");
    }
}

// (e) 3-4-5: A=(20,20), B=(60,50), u=(0,8; 0,6), Länge 50. Querachse w = u um 90° gedreht,
// nach rechts: (0,6; −0,8). Querlage A·w = 12 − 16 = −4 = B·w. offset 8 → Linie bei −4 + 8:
// A' = A + 8w = (24,8; 13,6), B' = B + 8w = (64,8; 43,6). Hilfslinie A: A + w = (20,6; 19,2)
// bis A + 10w = (26; 12). Text mittig (44,8; 28,6) + Lesenormale (−0,6; 0,8) = (44,2; 29,4),
// Winkel atan2(0,6; 0,8).
#[test]
fn aligned_dimension_follows_the_direction() {
    let sp = spec(DimKind::Aligned, 8.0);
    for (a, b, dir) in [
        (v(20.0, 20.0), v(60.0, 50.0), v(0.8, 0.6)),
        (v(60.0, 50.0), v(20.0, 20.0), v(-0.8, -0.6)),
    ] {
        let m = layout_dimension(&sp, &meas(50.0, lin(a, b, dir), None), &id_xf(), 0);
        assert_lines(
            &m,
            &[
                (v(24.8, 13.6), v(64.8, 43.6)),
                (v(20.6, 19.2), v(26.0, 12.0)),
                (v(60.6, 49.2), v(66.0, 42.0)),
            ],
            1e-9,
        );
        // Parallel zu AB im Abstand 8.
        let (p, q) = (v(24.8, 13.6), v(64.8, 43.6));
        assert!((dist_point_line(v(20.0, 20.0), p, q) - 8.0).abs() < 1e-9);
        assert!((dist_point_line(v(60.0, 50.0), p, q) - 8.0).abs() < 1e-9);
        assert_arrows(
            &m,
            &[(v(24.8, 13.6), v(0.8, 0.6)), (v(64.8, 43.6), v(-0.8, -0.6))],
            1e-9,
        );
        let t = one_text(&m);
        assert!(close(t.pos, v(44.2, 29.4), 1e-9), "{t:?}");
        assert!((t.angle - 0.6f64.atan2(0.8)).abs() < 1e-12);
        assert_eq!(t.text, "50,0");
    }
    // Nach links oben gerichtet: lesbar gedreht. A=(20,20), B=(−10,60), dir (−0,6; 0,8) → u =
    // (0,6; −0,8), Winkel atan2(−0,8; 0,6); w = (0,8; 0,6); Querlage beider 28 → Linie bei 36:
    // A' = A + 8w = (26,4; 24,8), B' = (−3,6; 64,8); Text (11,4; 44,8) + (0,8; 0,6) = (12,2; 45,4).
    let m = layout_dimension(
        &sp,
        &meas(50.0, lin(v(20.0, 20.0), v(-10.0, 60.0), v(-0.6, 0.8)), None),
        &id_xf(),
        0,
    );
    assert!(has_line(&m, v(26.4, 24.8), v(-3.6, 64.8), 1e-9));
    let t = one_text(&m);
    assert!(close(t.pos, v(12.2, 45.4), 1e-9), "{t:?}");
    assert!((t.angle - (-0.8f64).atan2(0.6)).abs() < 1e-12);
    assert!(t.angle > -FRAC_PI_2 && t.angle <= FRAC_PI_2);
}

// (f) offset −10: Bezug ist die kleinere Querlage, Linie bei 50 − 10 = 40; Hilfslinien von
// 50 − 1 = 49 bis 40 − 2 = 38; Text über der Linie (41).
#[test]
fn negative_offset_puts_the_line_on_the_other_side() {
    let sp = spec(DimKind::Horizontal, -10.0);
    let m = h_marks(v(20.0, 50.0), v(50.0, 50.0), 30.0, &sp);
    assert_lines(
        &m,
        &[
            (v(20.0, 40.0), v(50.0, 40.0)),
            (v(20.0, 49.0), v(20.0, 38.0)),
            (v(50.0, 49.0), v(50.0, 38.0)),
        ],
        TOL,
    );
    assert!(close(one_text(&m).pos, v(35.0, 41.0), TOL));
}

/// Waagerecht mit Spannen: a: Kante x=20, y∈[40,50]; b: Kante x=50, y∈[30,45].
fn h_span_marks(offset: f64) -> Vec<Mark> {
    let anchor = Anchor::Linear {
        a: v(20.0, 50.0),
        b: v(50.0, 45.0),
        dir: v(1.0, 0.0),
        a_span: (v(20.0, 40.0), v(20.0, 50.0)),
        b_span: (v(50.0, 30.0), v(50.0, 45.0)),
    };
    layout_dimension(
        &spec(DimKind::Horizontal, offset),
        &meas(30.0, anchor, None),
        &id_xf(),
        0,
    )
}

// Spannen (Festlegung (a)): offset ≥ 0 → Bezug max(40,50,30,45) = 50, Linie 60; Hilfslinie je
// Bezug vom der Linie näheren Ende: a von 51, b von 45 + 1 = 46, beide bis 62.
// offset < 0 → Bezug min = 30, Linie 20; a vom unteren Ende 40 − 1 = 39, b von 29, bis 18.
#[test]
fn spans_horizontal_witness_lines_start_at_the_nearer_end() {
    let m = h_span_marks(10.0);
    assert_lines(
        &m,
        &[
            (v(20.0, 60.0), v(50.0, 60.0)),
            (v(20.0, 51.0), v(20.0, 62.0)),
            (v(50.0, 46.0), v(50.0, 62.0)),
        ],
        TOL,
    );
    let m = h_span_marks(-10.0);
    assert_lines(
        &m,
        &[
            (v(20.0, 20.0), v(50.0, 20.0)),
            (v(20.0, 39.0), v(20.0, 18.0)),
            (v(50.0, 29.0), v(50.0, 18.0)),
        ],
        TOL,
    );
    assert!(close(one_text(&m).pos, v(35.0, 21.0), TOL));
}

// Senkrecht mit Spannen: a: Kante y=20, x∈[10,40]; b: Kante y=60, x∈[5,30]. w = (1,0).
// offset 10 → Bezug max = 40, Linie x = 50; a von 41, b von 31, bis 52.
// offset −10 → Bezug min = 5, Linie x = −5; a von 9, b von 4, bis −7; Text (−6, 40).
#[test]
fn spans_vertical_witness_lines_start_at_the_nearer_end() {
    let anchor = Anchor::Linear {
        a: v(40.0, 20.0),
        b: v(30.0, 60.0),
        dir: v(0.0, 1.0),
        a_span: (v(10.0, 20.0), v(40.0, 20.0)),
        b_span: (v(5.0, 60.0), v(30.0, 60.0)),
    };
    let m = layout_dimension(
        &spec(DimKind::Vertical, 10.0),
        &meas(40.0, anchor.clone(), None),
        &id_xf(),
        0,
    );
    assert_lines(
        &m,
        &[
            (v(50.0, 20.0), v(50.0, 60.0)),
            (v(41.0, 20.0), v(52.0, 20.0)),
            (v(31.0, 60.0), v(52.0, 60.0)),
        ],
        TOL,
    );
    let m = layout_dimension(
        &spec(DimKind::Vertical, -10.0),
        &meas(40.0, anchor, None),
        &id_xf(),
        0,
    );
    assert_lines(
        &m,
        &[
            (v(-5.0, 20.0), v(-5.0, 60.0)),
            (v(9.0, 20.0), v(-7.0, 20.0)),
            (v(4.0, 60.0), v(-7.0, 60.0)),
        ],
        TOL,
    );
    assert!(close(one_text(&m).pos, v(-6.0, 40.0), TOL));
}

// Schräg mit Spannen: A=(20,20), B=(60,50), u=(0,8; 0,6), w=(0,6; −0,8). a: (17,24) [Quer −9]
// und (23,16) [Quer 1]; b: (58,8; 51,6) [−6] und (61,8; 47,6) [−1]. Lage entlang u: A 28, B 78.
// offset 8 → Bezug max = 1, Linie bei 9: A' = 28u + 9w = (27,8; 9,6), B' = 78u + 9w =
// (67,8; 39,6); Hilfslinie a von (23,16) + w = (23,6; 15,2) bis A' + 2w = (29; 8), b von
// (62,4; 46,8) bis (69; 38).
// offset −8 → Bezug min = −9, Linie bei −17: A' = (12,2; 30,4), B' = (52,2; 60,4); a von
// (17,24) − w = (16,4; 24,8) bis A' − 2w = (11; 32), b von (58,2; 52,4) bis (51; 62).
#[test]
fn spans_aligned_witness_lines_start_at_the_nearer_end() {
    let anchor = Anchor::Linear {
        a: v(20.0, 20.0),
        b: v(60.0, 50.0),
        dir: v(0.8, 0.6),
        a_span: (v(17.0, 24.0), v(23.0, 16.0)),
        b_span: (v(58.8, 51.6), v(61.8, 47.6)),
    };
    let m = layout_dimension(
        &spec(DimKind::Aligned, 8.0),
        &meas(50.0, anchor.clone(), None),
        &id_xf(),
        0,
    );
    assert_lines(
        &m,
        &[
            (v(27.8, 9.6), v(67.8, 39.6)),
            (v(23.6, 15.2), v(29.0, 8.0)),
            (v(62.4, 46.8), v(69.0, 38.0)),
        ],
        1e-9,
    );
    let m = layout_dimension(
        &spec(DimKind::Aligned, -8.0),
        &meas(50.0, anchor, None),
        &id_xf(),
        0,
    );
    assert_lines(
        &m,
        &[
            (v(12.2, 30.4), v(52.2, 60.4)),
            (v(16.4, 24.8), v(11.0, 32.0)),
            (v(58.2, 52.4), v(51.0, 62.0)),
        ],
        1e-9,
    );
}

fn round(center: DVec2, r: f64, prefix: char) -> Measured {
    meas(
        if prefix == 'Ø' { 2.0 * r } else { r },
        Anchor::Round {
            center,
            radius: r,
            arc: None,
        },
        Some(prefix),
    )
}

// (g) Kreis (100,100), r=10, 45°: u = (√2/2, √2/2); Spitze 100 + 10·0,70710678… =
// 107,0710678…; Pfeil zum Kreis hin (Körper nach außen, dir = u); Führungslinie bis
// 100 + 20·√2/2 = 114,1421356…; Schulter 4 mm nach rechts bis 118,1421356…; Text „Ø20,0“
// dahinter (Start) bei x + 1, Grundlinie 114,142… − 1,75 (mittig zur Schulter).
// 135°: Spitze (92,929…; 107,071…), Knick (85,858…; 114,142…), Schulter nach links bis
// 81,858…, Text End bei 80,858….
#[test]
fn diameter_leader_default_45_degrees() {
    let sp = spec(DimKind::Diameter, 10.0);
    let m = layout_dimension(&sp, &round(v(100.0, 100.0), 10.0, 'Ø'), &id_xf(), 0);
    all_kind(&m, MarkKind::Dimension);
    let tip = v(100.0 + 10.0 * H2, 100.0 + 10.0 * H2);
    assert!(close(
        tip,
        v(107.071_067_811_865_47, 107.071_067_811_865_47),
        1e-12
    ));
    let knee = v(100.0 + 20.0 * H2, 100.0 + 20.0 * H2);
    assert_arrows(&m, &[(tip, v(H2, H2))], 1e-12);
    assert_lines(&m, &[(tip, knee), (knee, v(knee.x + 4.0, knee.y))], 1e-9);
    let t = one_text(&m);
    assert_eq!(t.text, "Ø20,0");
    assert_eq!(t.anchor, TextAnchor::Start);
    assert_eq!(t.angle, 0.0);
    assert!(
        close(
            t.pos,
            v(119.142_135_623_730_95, 114.142_135_623_730_95 - 1.75),
            1e-9
        ),
        "{t:?}"
    );

    let mut sp = sp;
    sp.angle = 135.0;
    let m = layout_dimension(&sp, &round(v(100.0, 100.0), 10.0, 'Ø'), &id_xf(), 0);
    let tip = v(100.0 - 10.0 * H2, 100.0 + 10.0 * H2);
    let knee = v(100.0 - 20.0 * H2, 100.0 + 20.0 * H2);
    assert_arrows(&m, &[(tip, v(-H2, H2))], 1e-9);
    assert_lines(&m, &[(tip, knee), (knee, v(knee.x - 4.0, knee.y))], 1e-9);
    let t = one_text(&m);
    assert_eq!(t.anchor, TextAnchor::End);
    assert!(
        close(
            t.pos,
            v(80.857_864_376_269_05, 112.392_135_623_730_95),
            1e-9
        ),
        "{t:?}"
    );
}

// (h) Kreis 30 mm auf dem Blatt (r = 15): Linie durch die Mitte von (100,100) ∓ 15·u =
// (89,393…; 89,393…) bis (110,606…; 110,606…), zwei Pfeile innen, Text auf der Linie, nicht in
// der Mitte (Mittellinienkreuz), sondern bei r/2 = 7,5 entlang u (Text „Ø30,0“ 10,5 breit:
// 7,5 ≥ 5,25 + 1) plus 1 senkrecht: (100 + 7,5·√2/2 − √2/2, 100 + 7,5·√2/2 + √2/2) =
// (100 + 6,5·√2/2, 100 + 8,5·√2/2), Winkel π/4. Grenze 25 mm: r = 12,5 →
// durch die Mitte, r = 12,49 → Führungslinie. Maßstab 1:2 mit r = 25 → 25 mm → durch.
#[test]
fn large_diameter_gets_a_through_line_with_two_arrows() {
    let sp = spec(DimKind::Diameter, 10.0);
    let m = layout_dimension(&sp, &round(v(100.0, 100.0), 15.0, 'Ø'), &id_xf(), 0);
    let (p1, p2) = (
        v(100.0 - 15.0 * H2, 100.0 - 15.0 * H2),
        v(100.0 + 15.0 * H2, 100.0 + 15.0 * H2),
    );
    assert_lines(&m, &[(p1, p2)], 1e-9);
    assert_arrows(&m, &[(p2, v(-H2, -H2)), (p1, v(H2, H2))], 1e-9);
    let t = one_text(&m);
    assert_eq!(t.text, "Ø30,0");
    assert_eq!(t.anchor, TextAnchor::Middle);
    assert!((t.angle - FRAC_PI_4).abs() < 1e-12);
    assert!(
        close(t.pos, v(100.0 + 6.5 * H2, 100.0 + 8.5 * H2), 1e-9),
        "{t:?}"
    );

    let m = layout_dimension(&sp, &round(v(100.0, 100.0), 12.5, 'Ø'), &id_xf(), 0);
    assert_eq!(find_arrows(&m).len(), 2);
    let m = layout_dimension(&sp, &round(v(100.0, 100.0), 12.49, 'Ø'), &id_xf(), 0);
    assert_eq!(find_arrows(&m).len(), 1);
    // Maßstab 1:2, Radius 25 in der Ansicht = 12,5 auf dem Blatt.
    let half = SheetTransform {
        scale: 0.5,
        tx: 100.0,
        ty: 100.0,
        bbox_center: DVec2::ZERO,
    };
    let m = layout_dimension(&sp, &round(v(0.0, 0.0), 25.0, 'Ø'), &half, 0);
    assert_eq!(find_arrows(&m).len(), 2);
    assert!(has_line(
        &m,
        v(100.0 - 12.5 * H2, 100.0 - 12.5 * H2),
        v(100.0 + 12.5 * H2, 100.0 + 12.5 * H2),
        1e-9
    ));
    // Radius bekommt nie die Linie durch die Mitte.
    let m = layout_dimension(
        &spec(DimKind::Radius, 10.0),
        &round(v(100.0, 100.0), 15.0, 'R'),
        &id_xf(),
        0,
    );
    assert_eq!(find_arrows(&m).len(), 1);
}

// (i) R5 bei (0,0), 45°, offset 10: Text „R5,0“, Spitze bei 5·√2/2.
#[test]
fn radius_text_has_r_prefix() {
    let m = layout_dimension(
        &spec(DimKind::Radius, 10.0),
        &round(v(0.0, 0.0), 5.0, 'R'),
        &id_xf(),
        0,
    );
    assert_eq!(one_text(&m).text, "R5,0");
    assert_arrows(&m, &[(v(5.0 * H2, 5.0 * H2), v(H2, H2))], 1e-12);
}

// (j) Scheitel (10,10), d1=(1,0), d2=(0,1), offset 15: Bogen r=15 von 0 bis π/2; Hilfslinien
// vom Scheitel bis 15 + 2 = 17 entlang d1, d2; Pfeile tangential an den Bogenenden: bei
// (25,10) Richtung (0,1), bei (10,25) Richtung (1,0); Text „90,0°“ bei 45° außen:
// (10,10) + 16·(√2/2, √2/2) = (21,313…; 21,313…), tangential (Winkel −π/4).
// 270°-Lage: d2 = (0,−1): kürzere Richtung von −π/2 bis 0 → Bogen 3π/2 … 2π; Mitte 7π/4, Text
// tangential wäre 5π/4 → lesbar π/4 (gedreht), daher eine Texthöhe weiter außen:
// r = 15 + 1 + 3,5 = 19,5 → (10 + 19,5·√2/2, 10 − 19,5·√2/2).
#[test]
fn angle_arc_between_two_lines() {
    let sp = spec(DimKind::Angle, 15.0);
    for (d1, d2) in [(v(1.0, 0.0), v(0.0, 1.0)), (v(0.0, 1.0), v(1.0, 0.0))] {
        let m = layout_dimension(
            &sp,
            &meas(
                90.0,
                Anchor::Angle {
                    vertex: v(10.0, 10.0),
                    d1,
                    d2,
                },
                None,
            ),
            &id_xf(),
            0,
        );
        let a = arcs(&m);
        assert_eq!(a.len(), 1);
        let (c, r, s, e, _) = a[0];
        assert!(close(c, v(10.0, 10.0), TOL) && (r - 15.0).abs() < TOL);
        assert!(s.abs() < 1e-12 && (e - FRAC_PI_2).abs() < 1e-12, "{s} {e}");
        assert_lines(
            &m,
            &[
                (v(10.0, 10.0), v(27.0, 10.0)),
                (v(10.0, 10.0), v(10.0, 27.0)),
            ],
            1e-12,
        );
        assert_arrows(
            &m,
            &[(v(25.0, 10.0), v(0.0, 1.0)), (v(10.0, 25.0), v(1.0, 0.0))],
            1e-12,
        );
        let t = one_text(&m);
        assert_eq!(t.text, "90,0°");
        assert!(
            close(t.pos, v(10.0 + 16.0 * H2, 10.0 + 16.0 * H2), 1e-9),
            "{t:?}"
        );
        assert!((t.angle + FRAC_PI_4).abs() < 1e-12);
    }
    let m = layout_dimension(
        &sp,
        &meas(
            90.0,
            Anchor::Angle {
                vertex: v(10.0, 10.0),
                d1: v(1.0, 0.0),
                d2: v(0.0, -1.0),
            },
            None,
        ),
        &id_xf(),
        0,
    );
    let (_, _, s, e, _) = arcs(&m)[0];
    assert!(
        (s - 1.5 * PI).abs() < 1e-12 && (e - 2.0 * PI).abs() < 1e-12,
        "{s} {e}"
    );
    let t = one_text(&m);
    assert!((t.angle - FRAC_PI_4).abs() < 1e-12);
    assert!(
        close(t.pos, v(10.0 + 19.5 * H2, 10.0 - 19.5 * H2), 1e-9),
        "{t:?}"
    );
}

fn chained(mode: ChainMode, group: u64, order: u32, offset: f64) -> DimSpec {
    let mut s = spec(DimKind::Horizontal, offset);
    s.chain = Some(ChainSpec { group, mode, order });
    s
}

fn dim_line_y(m: &[Mark]) -> f64 {
    // Die Maßlinie ist die waagerechte Linie (Hilfslinien sind senkrecht).
    let l: Vec<_> = lines(m)
        .into_iter()
        .filter(|(a, b, _)| (a.y - b.y).abs() < 1e-12)
        .collect();
    assert_eq!(l.len(), 1, "{l:?}");
    l[0].0.y
}

// (k) Bezugsmaße offset 10, Stufe 0, 1, 2 → Linie bei 50 + 10 + 7·Stufe = 60, 67, 74. Negativer
// Abstand stapelt nach unten: 40, 33. Eine Kette verschiebt nie, ein Maß ohne Gruppe auch nicht.
#[test]
fn baseline_group_stacks_by_7_mm() {
    for (order, y) in [(0, 60.0), (1, 67.0), (2, 74.0)] {
        let sp = chained(ChainMode::Baseline, 1, order, 10.0);
        let m = layout_dimension(
            &sp,
            &meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0)), None),
            &id_xf(),
            order as usize,
        );
        assert!((dim_line_y(&m) - y).abs() < TOL, "Stufe {order}");
        // Hilfslinien reichen bis zur eigenen Linie + 2.
        assert!(has_line(&m, v(20.0, 51.0), v(20.0, y + 2.0), TOL));
    }
    for (order, y) in [(0, 40.0), (1, 33.0)] {
        let sp = chained(ChainMode::Baseline, 1, order, -10.0);
        let m = h_marks(v(20.0, 50.0), v(50.0, 50.0), 30.0, &sp);
        let m2 = layout_dimension(
            &sp,
            &meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0)), None),
            &id_xf(),
            order as usize,
        );
        assert!((dim_line_y(&m) - 40.0).abs() < TOL);
        assert!((dim_line_y(&m2) - y).abs() < TOL);
    }
    let sp = chained(ChainMode::Chain, 1, 2, 10.0);
    let m = layout_dimension(
        &sp,
        &meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0)), None),
        &id_xf(),
        2,
    );
    assert!((dim_line_y(&m) - 60.0).abs() < TOL);
    let m = layout_dimension(
        &spec(DimKind::Horizontal, 10.0),
        &meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0)), None),
        &id_xf(),
        2,
    );
    assert!((dim_line_y(&m) - 60.0).abs() < TOL);
}

// (l) Kette: Glied 1 (20,50)–(50,50), Glied 2 (50,50)–(80, Kante 40…70). Einzeln: Linie bei 60
// bzw. 70 + 10 = 80. Mit gemeinsamem Bezug der Gruppe (max über alle Spannen = 70): beide bei 80,
// Texte je Glied zwischen den eigenen Pfeilen: (35, 81) und (65, 81).
#[test]
fn chain_shares_the_dimension_line() {
    let m1 = meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0)), None);
    let m2 = meas(
        30.0,
        Anchor::Linear {
            a: v(50.0, 50.0),
            b: v(80.0, 70.0),
            dir: v(1.0, 0.0),
            a_span: (v(50.0, 50.0), v(50.0, 50.0)),
            b_span: (v(80.0, 40.0), v(80.0, 70.0)),
        },
        None,
    );
    let s1 = chained(ChainMode::Chain, 3, 0, 10.0);
    let s2 = chained(ChainMode::Chain, 3, 1, 10.0);
    assert!((dim_line_y(&layout_dimension(&s1, &m1, &id_xf(), 0)) - 60.0).abs() < TOL);
    assert!((dim_line_y(&layout_dimension(&s2, &m2, &id_xf(), 1)) - 80.0).abs() < TOL);
    let base = group_base(&[&m1, &m2], &id_xf()).unwrap();
    let a = layout_dimension_in(&s1, &m1, &id_xf(), 0, Some(&base));
    let b = layout_dimension_in(&s2, &m2, &id_xf(), 1, Some(&base));
    assert!(has_line(&a, v(20.0, 80.0), v(50.0, 80.0), TOL));
    assert!(has_line(&b, v(50.0, 80.0), v(80.0, 80.0), TOL));
    // Hilfslinie von Glied 1 reicht jetzt von 51 bis 82.
    assert!(has_line(&a, v(20.0, 51.0), v(20.0, 82.0), TOL));
    assert!(close(one_text(&a).pos, v(35.0, 81.0), TOL));
    assert!(close(one_text(&b).pos, v(65.0, 81.0), TOL));
    // Negativer Abstand: Bezug min = 40 → beide Linien bei 30.
    let s1n = chained(ChainMode::Chain, 3, 0, -10.0);
    let s2n = chained(ChainMode::Chain, 3, 1, -10.0);
    let a = layout_dimension_in(&s1n, &m1, &id_xf(), 0, Some(&base));
    let b = layout_dimension_in(&s2n, &m2, &id_xf(), 1, Some(&base));
    assert!((dim_line_y(&a) - 30.0).abs() < TOL);
    assert!((dim_line_y(&b) - 30.0).abs() < TOL);
}

// (m) Toleranztexte bei 12,5 (eine Nachkommastelle).
#[test]
fn tolerance_texts() {
    let m = meas(12.5, lin(v(0.0, 0.0), v(1.0, 0.0), v(1.0, 0.0)), None);
    let mut sp = spec(DimKind::Horizontal, 10.0);
    sp.tolerance = Tolerance::Symmetric(0.1);
    let dt = dimension_text(&sp, &m);
    assert_eq!(dt.main, "12,5 ±0,1");
    assert!(dt.stack.is_none());

    sp.tolerance = Tolerance::Limits {
        upper: 0.2,
        lower: -0.1,
    };
    let dt = dimension_text(&sp, &m);
    assert_eq!(dt.main, "12,5");
    assert_eq!(
        dt.stack,
        Some(("+0,2".to_string(), "\u{2212}0,1".to_string()))
    );
    assert!((dt.stack_height - 2.45).abs() < 1e-12);
    // Breite: 8,4 + 2,1 + 0,6·2,45·4 = 16,38.
    assert!((dt.width() - 16.38).abs() < 1e-9);
    // Auf dem Blatt (A=(20,50), B=(50,50)): passt (16,38 ≤ 23); Beginn 35 − 8,19 = 26,81;
    // Haupttext Start (26,81; 61); Toleranz ab 26,81 + 8,4 + 2,1 = 37,31: untere Zeile auf der
    // Grundlinie 61, obere 1,2·2,45 = 2,94 höher.
    let marks = h_marks(v(20.0, 50.0), v(50.0, 50.0), 12.5, &sp);
    let t = texts(&marks);
    assert_eq!(t.len(), 3);
    let find = |s: &str| t.iter().find(|x| x.text == s).unwrap().clone();
    let main = find("12,5");
    assert_eq!(main.height, 3.5);
    assert!(close(main.pos, v(26.81, 61.0), 1e-9), "{main:?}");
    let up = find("+0,2");
    let lo = find("\u{2212}0,1");
    assert!((up.height - 2.45).abs() < 1e-12 && (lo.height - 2.45).abs() < 1e-12);
    assert!(close(lo.pos, v(37.31, 61.0), 1e-9), "{lo:?}");
    assert!(close(up.pos, v(37.31, 63.94), 1e-9), "{up:?}");
    assert_eq!(up.anchor, TextAnchor::Start);
    // Untere Abweichung null: ohne Vorzeichen.
    sp.tolerance = Tolerance::Limits {
        upper: 0.1,
        lower: 0.0,
    };
    assert_eq!(
        dimension_text(&sp, &m).stack,
        Some(("+0,1".to_string(), "0,0".to_string()))
    );

    // Grenzmaße: 12,5 + 0,1 / 12,5 − 0,1, ohne Nennmaß, volle Höhe.
    sp.tolerance = Tolerance::LimitSizes {
        upper: 0.1,
        lower: -0.1,
    };
    let dt = dimension_text(&sp, &m);
    assert_eq!(dt.main, "");
    assert_eq!(dt.stack, Some(("12,6".to_string(), "12,4".to_string())));
    assert_eq!(dt.stack_height, 3.5);
    assert!((dt.width() - 8.4).abs() < 1e-9);
    let t = texts(&h_marks(v(20.0, 50.0), v(50.0, 50.0), 12.5, &sp));
    assert_eq!(t.len(), 2);

    // Passung mit Vorsatz Ø, keine Nachkommastelle.
    let mo = meas(10.0, lin(v(0.0, 0.0), v(1.0, 0.0), v(1.0, 0.0)), Some('Ø'));
    let mut sp = spec(DimKind::Diameter, 10.0);
    sp.decimals = 0;
    sp.tolerance = Tolerance::Fit("H7".into());
    assert_eq!(dimension_text(&sp, &mo).main, "Ø10 H7");

    // Eigener Text: `<>` am Anfang behält den Vorsatz.
    let mut sp = spec(DimKind::Diameter, 10.0);
    sp.text_override = Some("<> typ.".into());
    assert_eq!(dimension_text(&sp, &mo).main, "Ø10,0 typ.");
    sp.text_override = Some("ca. <>".into());
    assert_eq!(dimension_text(&sp, &mo).main, "ca. 10,0");
    sp.text_override = Some("frei".into());
    assert_eq!(dimension_text(&sp, &mo).main, "frei");
    // Toleranz bleibt hinter einem eigenen Text.
    sp.tolerance = Tolerance::Symmetric(0.05);
    sp.decimals = 2;
    assert_eq!(dimension_text(&sp, &mo).main, "frei ±0,05");

    // Winkel mit Gradzeichen.
    let ma = meas(
        45.0,
        Anchor::Angle {
            vertex: v(0.0, 0.0),
            d1: v(1.0, 0.0),
            d2: v(H2, H2),
        },
        None,
    );
    assert_eq!(
        dimension_text(&spec(DimKind::Angle, 10.0), &ma).main,
        "45,0°"
    );
    let _: DimText = dimension_text(&spec(DimKind::Angle, 10.0), &ma);
}

// (n) Rundung wie `format!` (zur nächsten geraden Ziffer nur bei exaktem Halbwert im
// Binärwert): 0,05 ist binär etwas größer → 0,1; 0,25 exakt → 0,2; 0,35 binär kleiner → 0,3;
// 2,5 → 2; 3,5 → 4. −0,04 → 0,0 ohne Minus; Minus als U+2212; Nachkommastellen 0…3.
#[test]
fn format_value_rounds_and_uses_the_german_comma() {
    assert_eq!(format_value(12.5, 1), "12,5");
    assert_eq!(format_value(0.05, 1), "0,1");
    assert_eq!(format_value(0.25, 1), "0,2");
    assert_eq!(format_value(0.35, 1), "0,3");
    assert_eq!(format_value(2.5, 0), "2");
    assert_eq!(format_value(3.5, 0), "4");
    assert_eq!(format_value(10.0, 0), "10");
    assert_eq!(format_value(-0.04, 1), "0,0");
    assert_eq!(format_value(-0.0, 1), "0,0");
    assert_eq!(format_value(-0.0004, 3), "0,000");
    assert_eq!(format_value(-0.5, 1), "\u{2212}0,5");
    assert_eq!(format_value(-1.25, 1), "\u{2212}1,2");
    assert_eq!(format_value(1234.5678, 3), "1234,568");
    assert_eq!(format_value(1234.5678, 7), "1234,568");
    assert_eq!(format_value(f64::NAN, 1), "?");
    assert_eq!(format_value(f64::INFINITY, 1), "?");
    assert!(!format_value(-3.0, 1).contains('-'));
}

// (o) Verlorenes Maß: alles rot (`Lost`), die letzte Maßlinie (20,60)–(50,60), ein Text
// „? <Grund>“ nach der Passt-Regel: „? Der Bezug wurde nicht gefunden: Kante 3“ hat 39
// Zeichen = 81,9 mm > 30 − 7 → rechts daneben, Start bei 50 + 3,5 + 2 = 55,5, Höhe 61. Kurzer
// Grund „? a“ (6,3 mm) mittig bei (35, 61). Langer Grund wird auf 40 Zeichen gekürzt („…“):
// 43 Zeichen = 90,3 mm, ebenfalls Start bei (55,5; 61), nicht quer über den Hilfslinien.
// Ohne letzte Lage: nur „?“ in der Mitte der Ansicht (tx, ty).
#[test]
fn lost_dimension_is_red_with_a_question_mark_and_reason() {
    let sp = spec(DimKind::Horizontal, 10.0);
    let last = lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0));
    let reason = DimError::Lost("Kante 3".into()).to_string();
    let m = layout_lost(&sp, Some(&last), &id_xf(), &reason);
    all_kind(&m, MarkKind::Lost);
    assert!(has_line(&m, v(20.0, 60.0), v(50.0, 60.0), TOL));
    let t = one_text(&m);
    assert_eq!(t.text, "? Der Bezug wurde nicht gefunden: Kante 3");
    assert_eq!(t.anchor, TextAnchor::Start);
    assert!(close(t.pos, v(55.5, 61.0), TOL), "{t:?}");
    let t = one_text(&layout_lost(&sp, Some(&last), &id_xf(), "a"));
    assert_eq!(t.anchor, TextAnchor::Middle);
    assert!(close(t.pos, v(35.0, 61.0), TOL));
    let long = "x".repeat(60);
    let t = one_text(&layout_lost(&sp, Some(&last), &id_xf(), &long));
    assert_eq!(t.text, format!("? {}…", "x".repeat(40)));
    assert!((text_w(&t.text) - 90.3).abs() < 1e-9);
    assert_eq!(t.anchor, TextAnchor::Start);
    assert!(close(t.pos, v(55.5, 61.0), TOL), "{t:?}");
    // Der Textkasten (Start bis Start + 90,3) beginnt rechts der rechten Hilfslinie (x = 50).
    assert!(t.pos.x > 50.0);
    // Rundes Maß verloren: Führungslinie rot.
    let m = layout_lost(
        &spec(DimKind::Diameter, 10.0),
        Some(&Anchor::Round {
            center: v(100.0, 100.0),
            radius: 10.0,
            arc: None,
        }),
        &id_xf(),
        "weg",
    );
    all_kind(&m, MarkKind::Lost);
    assert_eq!(one_text(&m).text, "? weg");
    assert_eq!(find_arrows(&m).len(), 1);
    // Ohne letzte Lage.
    let xf = SheetTransform {
        scale: 2.0,
        tx: 150.0,
        ty: 80.0,
        bbox_center: v(3.0, 4.0),
    };
    let m = layout_lost(&sp, None, &xf, &reason);
    assert_eq!(m.len(), 1);
    let t = one_text(&m);
    assert_eq!(t.kind, MarkKind::Lost);
    assert_eq!(t.text, "?");
    assert!(close(t.pos, v(150.0, 80.0), TOL));
}

// (p) Mittellinie (0,0)–(10,0) in der Ansicht, Maßstab 2, Mitte (5,0) → Blatt (100,100):
// Enden 2·(−5) + 100 = 90 und 110; Überstand 3 mm je Seite → 87 … 113. Senkrecht ebenso.
#[test]
fn centerline_is_extended_by_3_mm() {
    let xf = SheetTransform {
        scale: 2.0,
        tx: 100.0,
        ty: 100.0,
        bbox_center: v(5.0, 0.0),
    };
    let src = CenterSource::Circle { body: 0, edge: 1 };
    let m = layout_centerline(
        &CenterLine {
            a: v(0.0, 0.0),
            b: v(10.0, 0.0),
            source: src,
        },
        &xf,
    );
    assert_eq!(m.len(), 1);
    assert_eq!(
        m[0],
        Mark::Line {
            a: v(87.0, 100.0),
            b: v(113.0, 100.0),
            kind: MarkKind::CenterLine
        }
    );
    let m = layout_centerline(
        &CenterLine {
            a: v(5.0, 4.0),
            b: v(5.0, -1.0),
            source: src,
        },
        &xf,
    );
    assert_lines(&m, &[(v(100.0, 111.0), v(100.0, 95.0))], TOL);
    // Entartet oder kaputt: nichts.
    for (a, b) in [
        (v(1.0, 1.0), v(1.0, 1.0)),
        (v(f64::NAN, 0.0), v(1.0, 0.0)),
        (v(0.0, 0.0), v(f64::INFINITY, 0.0)),
    ] {
        assert!(layout_centerline(&CenterLine { a, b, source: src }, &xf).is_empty());
    }
}

/// Vergleicht zwei Markierungslisten Stück für Stück (Zahlen bis `tol`; Texte gleich, wenn
/// `same_text`).
fn same_marks(a: &[Mark], b: &[Mark], tol: f64, same_text: bool) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b) {
        match (x, y) {
            (
                Mark::Line {
                    a: p,
                    b: q,
                    kind: k,
                },
                Mark::Line {
                    a: p2,
                    b: q2,
                    kind: k2,
                },
            ) => {
                assert!(
                    close(*p, *p2, tol) && close(*q, *q2, tol) && k == k2,
                    "{x:?} {y:?}"
                )
            }
            (
                Mark::Arrow { tip, dir, kind },
                Mark::Arrow {
                    tip: t2,
                    dir: d2,
                    kind: k2,
                },
            ) => assert!(close(*tip, *t2, tol) && close(*dir, *d2, tol) && kind == k2),
            (
                Mark::Arc {
                    center,
                    r,
                    start,
                    end,
                    ..
                },
                Mark::Arc {
                    center: c2,
                    r: r2,
                    start: s2,
                    end: e2,
                    ..
                },
            ) => assert!(
                close(*center, *c2, tol)
                    && (r - r2).abs() < tol
                    && (start - s2).abs() < tol
                    && (end - e2).abs() < tol
            ),
            (
                Mark::Text {
                    pos,
                    height,
                    angle,
                    anchor,
                    text,
                    ..
                },
                Mark::Text {
                    pos: p2,
                    height: h2,
                    angle: a2,
                    anchor: an2,
                    text: t2,
                    ..
                },
            ) => {
                assert!(close(*pos, *p2, tol) && (height - h2).abs() < tol);
                assert!((angle - a2).abs() < tol && anchor == an2);
                assert_eq!(text == t2, same_text, "{text} / {t2}");
            }
            _ => panic!("verschiedene Arten: {x:?} {y:?}"),
        }
    }
}

/// Anker zum Maßstabsfaktor der Ansichtskoordinaten.
type AnchorAt = Box<dyn Fn(f64) -> Anchor>;

// (q) Gleiche Blattpunkte bei 1:1 und 1:2 (Ansichtspunkte doppelt, Mitte doppelt) → gleiche
// Marken; ein anderer Wert ändert nur den Text.
#[test]
fn scale_independence() {
    let one = SheetTransform {
        scale: 1.0,
        tx: 150.0,
        ty: 100.0,
        bbox_center: v(10.0, 5.0),
    };
    let half = SheetTransform {
        scale: 0.5,
        tx: 150.0,
        ty: 100.0,
        bbox_center: v(20.0, 10.0),
    };
    let cases: Vec<(DimSpec, AnchorAt)> = vec![
        (
            spec(DimKind::Horizontal, 10.0),
            Box::new(|k| lin(v(0.0, 0.0) * k, v(30.0, 4.0) * k, v(1.0, 0.0))),
        ),
        (
            spec(DimKind::Aligned, -8.0),
            Box::new(|k| lin(v(0.0, 0.0) * k, v(40.0, 30.0) * k, v(0.8, 0.6))),
        ),
        (
            spec(DimKind::Diameter, 10.0),
            Box::new(|k| Anchor::Round {
                center: v(3.0, 4.0) * k,
                radius: 5.0 * k,
                arc: None,
            }),
        ),
        (
            spec(DimKind::Diameter, 10.0),
            Box::new(|k| Anchor::Round {
                center: v(3.0, 4.0) * k,
                radius: 20.0 * k,
                arc: None,
            }),
        ),
        (
            spec(DimKind::Angle, 12.0),
            Box::new(|k| Anchor::Angle {
                vertex: v(1.0, 2.0) * k,
                d1: v(1.0, 0.0),
                d2: v(H2, H2),
            }),
        ),
    ];
    for (sp, anchor) in &cases {
        let prefix = (sp.kind == DimKind::Diameter).then_some('Ø');
        let a = layout_dimension(sp, &meas(30.0, anchor(1.0), prefix), &one, 0);
        let b = layout_dimension(sp, &meas(30.0, anchor(2.0), prefix), &half, 0);
        assert!(!a.is_empty());
        same_marks(&a, &b, 1e-9, true);
        let c = layout_dimension(sp, &meas(60.0, anchor(2.0), prefix), &half, 0);
        same_marks(&a, &c, 1e-9, false);
    }
}

// Kaputte Eingaben: keine Marken, keine Panik. Weit außerhalb des Blatts: trotzdem gezeichnet.
#[test]
fn non_finite_or_huge_inputs_give_no_marks_and_far_marks_are_kept() {
    let sp = spec(DimKind::Horizontal, 10.0);
    let bad = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e308];
    for x in bad {
        let m = h_marks(v(x, 50.0), v(50.0, 50.0), 30.0, &sp);
        assert!(m.is_empty() || all_finite(&m), "{x}: {m:?}");
        if !x.is_finite() {
            assert!(m.is_empty());
        }
        let mut s2 = sp.clone();
        s2.offset = x;
        let m = h_marks(v(20.0, 50.0), v(50.0, 50.0), 30.0, &s2);
        assert!(m.is_empty() || all_finite(&m));
        if !x.is_finite() {
            assert!(m.is_empty());
        }
        s2.offset = 10.0;
        s2.text_shift = (x, 0.0);
        let m = h_marks(v(20.0, 50.0), v(50.0, 50.0), 30.0, &s2);
        assert!(m.is_empty() || all_finite(&m));
        let mut s3 = spec(DimKind::Diameter, 10.0);
        s3.angle = x;
        assert!(
            layout_dimension(&s3, &round(v(0.0, 0.0), 5.0, 'Ø'), &id_xf(), 0).is_empty()
                || x == 1e308
        );
        let m = layout_dimension(
            &spec(DimKind::Diameter, 10.0),
            &round(v(0.0, 0.0), x, 'Ø'),
            &id_xf(),
            0,
        );
        assert!(m.is_empty() || all_finite(&m));
        let m = layout_dimension(
            &spec(DimKind::Angle, 10.0),
            &meas(
                90.0,
                Anchor::Angle {
                    vertex: v(0.0, x),
                    d1: v(1.0, 0.0),
                    d2: v(0.0, 1.0),
                },
                None,
            ),
            &id_xf(),
            0,
        );
        assert!(m.is_empty() || all_finite(&m));
        let xf = SheetTransform {
            scale: x,
            tx: 0.0,
            ty: 0.0,
            bbox_center: DVec2::ZERO,
        };
        let m = layout_dimension(
            &sp,
            &meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0)), None),
            &xf,
            0,
        );
        assert!(m.is_empty() || all_finite(&m));
        let m = layout_lost(
            &sp,
            Some(&lin(v(x, 0.0), v(1.0, 0.0), v(1.0, 0.0))),
            &id_xf(),
            "r",
        );
        assert!(all_finite(&m), "{m:?}");
    }
    // Richtung null oder Winkelrichtungen gleich: nichts.
    assert!(h_marks_dir(v(0.0, 0.0)).is_empty());
    // Riesige, aber endliche Lage: Überlauf zu ∞ → nichts.
    let m = h_marks(v(-1e308, 50.0), v(1e308, 50.0), 30.0, &sp);
    assert!(m.is_empty());
    // Weit außerhalb (1e5 mm): gezeichnet, an der richtigen Stelle (P12 erledigt die Oberfläche).
    let m = h_marks(v(1e5, -1e5), v(1e5 + 30.0, -1e5), 30.0, &sp);
    assert!(has_line(
        &m,
        v(1e5, -1e5 + 10.0),
        v(1e5 + 30.0, -1e5 + 10.0),
        1e-9
    ));
    let (lo, hi) = bbox_of_marks(&m);
    assert!(
        lo.x >= 1e5 - 1e-9 && hi.x <= 1e5 + 30.0 + 1e-9,
        "{lo:?} {hi:?}"
    );
    assert!(lo.y >= -1e5 && hi.y <= -1e5 + 12.0 + 1e-9);
}

fn h_marks_dir(dir: DVec2) -> Vec<Mark> {
    layout_dimension(
        &spec(DimKind::Horizontal, 10.0),
        &meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), dir), None),
        &id_xf(),
        0,
    )
}

// ---------------------------------------------------------------------------------------------
// Blattaufbau

fn v3(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

/// Linienstrich `a`–`b` mit der Herkunft Kante `edge` von Körper 0.
fn line_stroke(a: DVec3, b: DVec3, edge: usize) -> Stroke {
    let d = b - a;
    Stroke {
        curve: Curve::Line {
            origin: a,
            dir: d.normalize(),
        },
        t0: 0.0,
        t1: d.length(),
        origin: StrokeOrigin::Edge { body: 0, edge },
    }
}

/// ViewLines von Hand: Rechteck mit den Bildecken; Bild nach eigener Tabelle: Vorne (x, z),
/// Oben (x, y). Hüllbox = Rechteck.
fn rect_view(kind: ViewKind, w: f64, h: f64, centerlines: Vec<CenterLine>) -> ViewLines {
    let p = |x: f64, y: f64| match kind {
        ViewKind::Front => v3(x, 0.0, y),
        _ => v3(x, y, 0.0),
    };
    let corners = [p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)];
    let strokes: Vec<Stroke> = (0..4)
        .map(|i| line_stroke(corners[i], corners[(i + 1) % 4], i))
        .collect();
    let pieces = strokes
        .iter()
        .enumerate()
        .map(|(i, s)| Piece {
            stroke: i,
            t0: s.t0,
            t1: s.t1,
            vis: Visibility::Visible,
        })
        .collect();
    ViewLines {
        kind,
        basis: Basis::of(kind),
        extent: 100.0,
        strokes,
        pieces,
        bbox: ((0.0, 0.0), (w, h)),
        centerlines,
        dropped_splits: 0,
    }
}

fn a3_drawing() -> Drawing {
    Drawing::new(
        DrawingId(1),
        "Z".into(),
        Sheet {
            paper: Paper::A3,
            landscape: true,
        },
        Projection::First,
    )
}

/// Vorne bei (100,100) 1:1, Rechteck 40×20 → Blatt = p − (20,10) + (100,100) = p + (80,90).
/// Oben bei (300,100) 1:1, Rechteck 40×30, Mittellinien aus.
struct Fixture {
    d: Drawing,
    front: ViewId,
    top: ViewId,
    vf: ViewLines,
    vt: ViewLines,
}

fn fixture() -> Fixture {
    let mut d = a3_drawing();
    let one = Scale { num: 1, den: 1 };
    let front = d.add_view(
        ViewKind::Front,
        one,
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    let top = d.add_view(
        ViewKind::Top,
        one,
        Placement::Free { x: 300.0, y: 100.0 },
        None,
    );
    d.views[1].centerlines = false;
    let src = CenterSource::Axis { body: 0, face: 3 };
    let vf = rect_view(
        ViewKind::Front,
        40.0,
        20.0,
        vec![CenterLine {
            a: v(20.0, 0.0),
            b: v(20.0, 20.0),
            source: src,
        }],
    );
    let vt = rect_view(
        ViewKind::Top,
        40.0,
        30.0,
        vec![
            CenterLine {
                a: v(10.0, 15.0),
                b: v(30.0, 15.0),
                source: src,
            },
            CenterLine {
                a: v(20.0, 5.0),
                b: v(20.0, 25.0),
                source: src,
            },
        ],
    );
    Fixture {
        d,
        front,
        top,
        vf,
        vt,
    }
}

fn res(f: &Fixture) -> BTreeMap<ViewId, &ViewLines> {
    BTreeMap::from([(f.front, &f.vf), (f.top, &f.vt)])
}

fn of_kind(marks: &[Mark], kind: MarkKind) -> Vec<Mark> {
    marks
        .iter()
        .filter(|m| kind_of(m) == kind)
        .cloned()
        .collect()
}

fn h_spec(view: ViewId, id: u64, offset: f64) -> DimSpec {
    let mut s = spec(DimKind::Horizontal, offset);
    s.view = view;
    s.id = DimId(id);
    s
}

// (r) Vorne (Mittellinien an): Achse (20,0)–(20,20) → Blatt (100,90)–(100,110), mit Überstand
// (100,87)–(100,113). Oben (aus): keine Mittellinien, aber Maße. Maß vorne (0,20)–(40,20) →
// Blatt (80,110)–(120,110), Linie bei 120. Verlorenes Maß mit letzter Lage (0,0)–(40,0) →
// Blatt y = 90, offset −10 → Linie 80, rot mit Grund. Maß oben (0,30)–(40,30): Blatt oben
// = p − (20,15) + (300,100) = p + (280,85) → (280,115)–(320,115), Linie 125.
#[test]
fn build_sheet_with_adds_marks_only_for_views_with_centerlines_flag() {
    let f = fixture();
    let mut dims = ResolvedDims::default();
    let ok = |a: DVec2, b: DVec2| Ok(meas(40.0, lin(a, b, v(1.0, 0.0)), None));
    dims.by_view.insert(
        f.front,
        vec![
            (h_spec(f.front, 1, 10.0), ok(v(0.0, 20.0), v(40.0, 20.0))),
            (
                h_spec(f.front, 2, -10.0),
                Err(DimError::Lost("Kante 3".into())),
            ),
        ],
    );
    dims.last_known
        .insert(DimId(2), lin(v(0.0, 0.0), v(40.0, 0.0), v(1.0, 0.0)));
    dims.by_view.insert(
        f.top,
        vec![(h_spec(f.top, 3, 10.0), ok(v(0.0, 30.0), v(40.0, 30.0)))],
    );
    let m = build_sheet_with(&f.d, &res(&f), &dims);
    assert_eq!(m.views.len(), 2);
    let front = &m.views[0];
    let cl = of_kind(&front.marks, MarkKind::CenterLine);
    assert_eq!(
        cl,
        vec![Mark::Line {
            a: v(100.0, 87.0),
            b: v(100.0, 113.0),
            kind: MarkKind::CenterLine
        }]
    );
    let dm = of_kind(&front.marks, MarkKind::Dimension);
    assert!(has_line(&dm, v(80.0, 120.0), v(120.0, 120.0), TOL));
    assert_eq!(one_text(&dm).text, "40,0");
    let lost = of_kind(&front.marks, MarkKind::Lost);
    // offset −10 von y = 90 → 80.
    assert!(has_line(&lost, v(80.0, 80.0), v(120.0, 80.0), TOL));
    assert_eq!(
        one_text(&lost).text,
        "? Der Bezug wurde nicht gefunden: Kante 3"
    );
    let top = &m.views[1];
    assert!(of_kind(&top.marks, MarkKind::CenterLine).is_empty());
    let dm = of_kind(&top.marks, MarkKind::Dimension);
    assert!(has_line(&dm, v(280.0, 125.0), v(320.0, 125.0), TOL));
    // Herkunft der Linien.
    for (i, l) in front.lines.iter().enumerate() {
        assert_eq!(l.origin, StrokeOrigin::Edge { body: 0, edge: i });
    }
    // Verlorenes Maß ohne letzte Lage: „?“ in der Ansichtsmitte (100,100).
    dims.last_known.clear();
    let m = build_sheet_with(&f.d, &res(&f), &dims);
    let lost = of_kind(&m.views[0].marks, MarkKind::Lost);
    assert_eq!(lost.len(), 1);
    assert!(close(one_text(&lost).pos, v(100.0, 100.0), TOL));
}

// Bezugsmaße und Kette über `build_sheet_with`: Gruppe 7 (Bezug, Stufen 0, 1, 2; Enden auf
// y = 110 im Blatt) → Linien 120, 127, 134. Gruppe 8 (Kette, offset −10): Glied 1 mit Spanne
// (0,0)…(0,20) → Blatt 90…110, Glied 2 mit Spanne (40,−5)…(40,0) → 85…90; gemeinsamer Bezug
// min = 85 → beide bei 75 (einzeln wären es 80 und 75).
#[test]
fn build_sheet_with_stacks_baselines_and_shares_chain_lines() {
    let f = fixture();
    let mut dims = ResolvedDims::default();
    let mut list = Vec::new();
    for (order, x) in [(0u32, 10.0), (1, 25.0), (2, 40.0)] {
        let mut s = h_spec(f.front, 10 + order as u64, 10.0);
        s.chain = Some(ChainSpec {
            group: 7,
            mode: ChainMode::Baseline,
            order,
        });
        list.push((
            s,
            Ok(meas(x, lin(v(0.0, 20.0), v(x, 20.0), v(1.0, 0.0)), None)),
        ));
    }
    let mut c1 = h_spec(f.front, 20, -10.0);
    c1.chain = Some(ChainSpec {
        group: 8,
        mode: ChainMode::Chain,
        order: 0,
    });
    let mut c2 = c1.clone();
    c2.id = DimId(21);
    c2.chain = Some(ChainSpec {
        group: 8,
        mode: ChainMode::Chain,
        order: 1,
    });
    list.push((
        c1,
        Ok(meas(
            10.0,
            Anchor::Linear {
                a: v(0.0, 20.0),
                b: v(10.0, 0.0),
                dir: v(1.0, 0.0),
                a_span: (v(0.0, 0.0), v(0.0, 20.0)),
                b_span: (v(10.0, 0.0), v(10.0, 0.0)),
            },
            None,
        )),
    ));
    list.push((
        c2,
        Ok(meas(
            30.0,
            Anchor::Linear {
                a: v(10.0, 0.0),
                b: v(40.0, 0.0),
                dir: v(1.0, 0.0),
                a_span: (v(10.0, 0.0), v(10.0, 0.0)),
                b_span: (v(40.0, -5.0), v(40.0, 0.0)),
            },
            None,
        )),
    ));
    dims.by_view.insert(f.front, list);
    let m = build_sheet_with(&f.d, &res(&f), &dims);
    let dm = of_kind(&m.views[0].marks, MarkKind::Dimension);
    assert!(has_line(&dm, v(80.0, 120.0), v(90.0, 120.0), TOL));
    assert!(has_line(&dm, v(80.0, 127.0), v(105.0, 127.0), TOL));
    assert!(has_line(&dm, v(80.0, 134.0), v(120.0, 134.0), TOL));
    // Kette: Glied 1 (80…90) ist kurz (10 ≥ 8 → innen), Glied 2 (90…120).
    assert!(has_line(&dm, v(80.0, 75.0), v(90.0, 75.0), TOL), "{dm:?}");
    assert!(has_line(&dm, v(90.0, 75.0), v(120.0, 75.0), TOL));
}

// (r) zweiter Teil: ohne Maße und mit ausgeschalteten Mittellinien keine Marken; die Linien
// hängen nicht von den Schaltern ab.
#[test]
fn build_sheet_without_dims_has_no_marks_for_flagged_off_views() {
    let mut f = fixture();
    f.d.views[0].centerlines = false;
    let off = build_sheet(&f.d, &res(&f));
    assert!(off.views.iter().all(|v| v.marks.is_empty()));
    let off2 = build_sheet_with(&f.d, &res(&f), &ResolvedDims::default());
    assert!(off2.views.iter().all(|v| v.marks.is_empty()));
    f.d.views[0].centerlines = true;
    f.d.views[1].centerlines = true;
    let on = build_sheet(&f.d, &res(&f));
    assert_eq!(on.views[0].marks.len(), 1);
    // Oben: zwei Linien des Kreuzes: (10,15)–(30,15) → (290,100)–(310,100) ± 3.
    assert_eq!(on.views[1].marks.len(), 2);
    assert!(has_line(
        &on.views[1].marks,
        v(287.0, 100.0),
        v(313.0, 100.0),
        TOL
    ));
    assert!(has_line(
        &on.views[1].marks,
        v(300.0, 87.0),
        v(300.0, 113.0),
        TOL
    ));
    assert!(
        on.views
            .iter()
            .flat_map(|v| &v.marks)
            .all(|m| kind_of(m) == MarkKind::CenterLine)
    );
    for (a, b) in off.views.iter().zip(&on.views) {
        assert_eq!(a.extent_mm, b.extent_mm);
        assert_eq!(a.lines.len(), b.lines.len());
        for (x, y) in a.lines.iter().zip(&b.lines) {
            assert_eq!(x.curve, y.curve);
            assert_eq!(x.vis, y.vis);
        }
    }
}

fn golden_text(m: &SheetModel) -> String {
    let mut s = String::new();
    for v in &m.views {
        s += &format!("view {:?} {} {:?}\n", v.id, v.name, v.extent_mm);
        for l in &v.lines {
            s += &format!("{:?} {:?}\n", l.vis, l.curve);
        }
    }
    s
}

// (s) Linien und Ausdehnung wie vor der Änderung: Vergleich mit der Ausgabe des alten
// `build_sheet` (Fixtur `fixtures/stage15_sheet_lines.txt`, vor dieser Aufgabe erzeugt, Zahlen
// im Debug-Format, also bitgenau). Ohne Mittellinien-Schalter: keine Marken.
#[test]
fn old_sheet_construction_is_unchanged() {
    let solid = box_with_hole(40.0, 30.0, 20.0, 5.0, DVec3::Y);
    let cyl = cylinder_solid(8.0, 12.0);
    let bodies = [
        DrawBody {
            name: "A".into(),
            solid: &solid,
        },
        DrawBody {
            name: "B".into(),
            solid: &cyl,
        },
    ];
    let mut d = a3_drawing();
    let f = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 200.0 },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        Scale { num: 1, den: 2 },
        Placement::Aligned { distance: 90.0 },
        Some(f),
    );
    let i = d.add_view(
        ViewKind::Isometric,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 300.0, y: 150.0 },
        None,
    );
    let p = Progress::new();
    let vf = compute_view(&bodies, ViewKind::Front, &p).unwrap();
    let vt = compute_view(&bodies, ViewKind::Top, &p).unwrap();
    let vi = compute_view(&bodies, ViewKind::Isometric, &p).unwrap();
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(f, &vf), (t, &vt), (i, &vi)]);
    let golden = include_str!("fixtures/stage15_sheet_lines.txt").replace('\r', "");
    let m = build_sheet(&d, &res);
    assert_eq!(golden_text(&m), golden);
    let m2 = build_sheet_with(&d, &res, &ResolvedDims::default());
    assert_eq!(golden_text(&m2), golden);
    // Normansichten zeigen ihre Mittellinien (Vorgabe an), die Isometrie nicht.
    assert!(!m.views[0].marks.is_empty() && !m.views[1].marks.is_empty());
    assert!(m.views[2].marks.is_empty());
    for view in &mut d.views {
        view.centerlines = false;
    }
    let m3 = build_sheet(&d, &res);
    assert_eq!(golden_text(&m3), golden);
    assert!(m3.views.iter().all(|v| v.marks.is_empty()));
}

/// Eigene Textbreite: 0,6 · 3,5 · Zeichenzahl.
fn text_w(t: &str) -> f64 {
    0.6 * 3.5 * t.chars().count() as f64
}

// Fix 1: Text eines großen Ø nicht auf dem Mittellinienkreuz. Für 0°, 45°, 90°, 135°, 180°,
// 270°: Anker mindestens halbe Textbreite (5,25) von der Mitte entfernt, und der Textkasten
// (Anker ± Breite/2 in Leserichtung, 0 … 3,5 senkrecht dazu) enthält die Mitte nicht.
#[test]
fn large_diameter_text_avoids_the_centre() {
    let c = v(100.0, 100.0);
    for deg in [0.0f64, 45.0, 90.0, 135.0, 180.0, 270.0] {
        let mut sp = spec(DimKind::Diameter, 10.0);
        sp.angle = deg;
        let m = layout_dimension(&sp, &round(c, 15.0, 'Ø'), &id_xf(), 0);
        let t = one_text(&m);
        assert_eq!(t.anchor, TextAnchor::Middle);
        let w = text_w(&t.text);
        let d = t.pos - c;
        assert!(d.length() >= 0.5 * w, "{deg}: {t:?}");
        let along = v(t.angle.cos(), t.angle.sin());
        let up = v(-along.y, along.x);
        // Lage der Mitte im Textsystem (Ursprung Anker).
        let (x, y) = (-d.x * along.x - d.y * along.y, -d.x * up.x - d.y * up.y);
        let inside = x.abs() <= 0.5 * w && (0.0..=3.5).contains(&y);
        assert!(!inside, "{deg}: Mitte im Textkasten {t:?}");
        // Lesbar und auf der Linie (Abstand 1 von der Linie durch die Mitte).
        assert!(t.angle > -FRAC_PI_2 && t.angle <= FRAC_PI_2 + 1e-12);
        let th = deg.to_radians();
        let u = v(th.cos(), th.sin());
        assert!(
            (dist_point_line(t.pos, c, c + u) - 1.0).abs() < 1e-9,
            "{deg}"
        );
    }
}

// Fix 8: Führungslinie mit Abstand 0: keine Strecke der Länge null, Pfeil, Schulter und Text
// bleiben. Kreis (100,100) r 10, 45°: Spitze = Knick (107,07…), Schulter bis 111,07….
#[test]
fn leader_with_zero_offset_has_no_zero_length_line() {
    let m = layout_dimension(
        &spec(DimKind::Diameter, 0.0),
        &round(v(100.0, 100.0), 10.0, 'Ø'),
        &id_xf(),
        0,
    );
    let tip = v(100.0 + 10.0 * H2, 100.0 + 10.0 * H2);
    assert_lines(&m, &[(tip, v(tip.x + 4.0, tip.y))], 1e-9);
    for (a, b, _) in lines(&m) {
        assert!(a.distance(b) > 1e-9);
    }
    assert_eq!(find_arrows(&m).len(), 1);
    assert_eq!(one_text(&m).text, "Ø20,0");
}

// Fix 5: dieselbe Parallel-Grenze (Sinus 1e-6) für Gruppen und Bezug: ein Glied mit Richtung
// (1, 1e-7) zählt zur Gruppe (seine Spanne bis y = 90 hebt den Bezug), eines mit (1, 1e-5)
// nicht.
#[test]
fn group_parallelism_uses_the_across_tolerance() {
    let m1 = meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, 0.0)), None);
    let tall = |dy: f64| {
        meas(
            30.0,
            Anchor::Linear {
                a: v(50.0, 50.0),
                b: v(80.0, 90.0),
                dir: v(1.0, dy),
                a_span: (v(50.0, 50.0), v(50.0, 50.0)),
                b_span: (v(80.0, 40.0), v(80.0, 90.0)),
            },
            None,
        )
    };
    let near = tall(1e-7);
    let far = tall(1e-5);
    let b = group_base(&[&m1, &near], &id_xf()).unwrap();
    assert!(
        (b.max - 90.0).abs() < 1e-9 && (b.min - 40.0).abs() < 1e-9,
        "{b:?}"
    );
    let b2 = group_base(&[&m1, &far], &id_xf()).unwrap();
    assert!((b2.max - 50.0).abs() < 1e-9, "{b2:?}");
    // Der Bezug gilt für ein Maß, dessen Richtung um 1e-7 abweicht (Linie bei 100), nicht bei
    // 1e-5 (eigener Bezug 50 → Linie bei 60). Neigung nach unten rechts: die Querachse bleibt
    // oben (bei Neigung nach oben rechts zeigt sie nach der Anker-Regel `rightward` nach unten).
    let s = chained(ChainMode::Chain, 1, 0, 10.0);
    let tilt = |dy: f64| meas(30.0, lin(v(20.0, 50.0), v(50.0, 50.0), v(1.0, dy)), None);
    let m = layout_dimension_in(&s, &tilt(-1e-7), &id_xf(), 0, Some(&b));
    assert_eq!(line_through(&m, v(35.0, 100.0), 1e-4).len(), 1, "{m:?}");
    let m = layout_dimension_in(&s, &tilt(-1e-5), &id_xf(), 0, Some(&b));
    assert_eq!(line_through(&m, v(35.0, 60.0), 1e-3).len(), 1, "{m:?}");
}

// Fix 6 und 7 über `build_sheet_with` (Vorne: Blatt = p + (80,90)).
// Fix 7: Kette Gruppe 9, offset 10, Punkte (0,20), (10,20), (40,20) → Blatt x 80, 90, 120 bei
// y 110: Linie 120, Hilfslinien je Punkt einmal (90 nur einmal, obwohl beide Glieder sie
// haben): 3 senkrechte Linien von 111 bis 122, nicht 4.
// Fix 6: Kette Gruppe 8, offset −10: Glied 1 verloren (letzte Lage (0,20)–(10,0), Spanne a
// 0…20), Glied 2 messbar mit Spanne (40,−5)…(40,0) → Gruppenbezug min = −5 → Blatt 85 → beide
// Linien bei 75 (das verlorene ohne Gruppenbezug läge bei 90 − 10 = 80).
#[test]
fn chain_witness_lines_once_and_lost_member_on_the_group_line() {
    let f = fixture();
    let mut dims = ResolvedDims::default();
    let mut list = Vec::new();
    for (order, (x0, x1)) in [(0u32, (0.0, 10.0)), (1, (10.0, 40.0))] {
        let mut s = h_spec(f.front, 30 + order as u64, 10.0);
        s.chain = Some(ChainSpec {
            group: 9,
            mode: ChainMode::Chain,
            order,
        });
        list.push((
            s,
            Ok(meas(
                x1 - x0,
                lin(v(x0, 20.0), v(x1, 20.0), v(1.0, 0.0)),
                None,
            )),
        ));
    }
    let mut c1 = h_spec(f.front, 40, -10.0);
    c1.chain = Some(ChainSpec {
        group: 8,
        mode: ChainMode::Chain,
        order: 0,
    });
    let mut c2 = c1.clone();
    c2.id = DimId(41);
    c2.chain = Some(ChainSpec {
        group: 8,
        mode: ChainMode::Chain,
        order: 1,
    });
    list.push((c1, Err(DimError::Lost("Kante 1".into()))));
    dims.last_known.insert(
        DimId(40),
        Anchor::Linear {
            a: v(0.0, 20.0),
            b: v(10.0, 0.0),
            dir: v(1.0, 0.0),
            a_span: (v(0.0, 0.0), v(0.0, 20.0)),
            b_span: (v(10.0, 0.0), v(10.0, 0.0)),
        },
    );
    list.push((
        c2,
        Ok(meas(
            30.0,
            Anchor::Linear {
                a: v(10.0, 0.0),
                b: v(40.0, 0.0),
                dir: v(1.0, 0.0),
                a_span: (v(10.0, 0.0), v(10.0, 0.0)),
                b_span: (v(40.0, -5.0), v(40.0, 0.0)),
            },
            None,
        )),
    ));
    dims.by_view.insert(f.front, list);
    let m = build_sheet_with(&f.d, &res(&f), &dims);
    let dm = of_kind(&m.views[0].marks, MarkKind::Dimension);
    let up: Vec<_> = lines(&dm)
        .into_iter()
        .filter(|(a, b, _)| (a.x - b.x).abs() < 1e-12 && a.y.max(b.y) > 115.0)
        .collect();
    assert_eq!(up.len(), 3, "{up:?}");
    for x in [80.0, 90.0, 120.0] {
        assert!(has_line(&dm, v(x, 111.0), v(x, 122.0), TOL), "{x}");
    }
    assert!(has_line(&dm, v(80.0, 120.0), v(90.0, 120.0), TOL));
    assert!(has_line(&dm, v(90.0, 120.0), v(120.0, 120.0), TOL));
    let lost = of_kind(&m.views[0].marks, MarkKind::Lost);
    assert!(
        has_line(&lost, v(80.0, 75.0), v(90.0, 75.0), TOL),
        "{lost:?}"
    );
    assert!(has_line(&dm, v(90.0, 75.0), v(120.0, 75.0), TOL));
}
