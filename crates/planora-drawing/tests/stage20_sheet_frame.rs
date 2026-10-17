//! Blattgeometrie nach ISO 5457/7200: Rahmen, Zentriermarken, Raster, Schriftfeld, Tabellen,
//! freier Bereich, Projektionssymbol.

use planora_drawing::sheet_frame::*;
use planora_drawing::{Paper, Projection, Sheet};
use planora_math::DVec2;

const PAPERS: [Paper; 5] = [Paper::A4, Paper::A3, Paper::A2, Paper::A1, Paper::A0];

fn sh(paper: Paper, landscape: bool) -> Sheet {
    Sheet { paper, landscape }
}

fn bx(b: Box2) -> [f64; 4] {
    [b.0.x, b.0.y, b.1.x, b.1.y]
}

fn near(a: [f64; 4], e: [f64; 4]) {
    for i in 0..4 {
        assert!((a[i] - e[i]).abs() < 1e-9, "{a:?} != {e:?}");
    }
}

/// Wie [`near`], für Werte mit sechs Nachkommastellen (skalierte Formate).
fn near6(a: [f64; 4], e: [f64; 4]) {
    for i in 0..4 {
        assert!((a[i] - e[i]).abs() < 1e-6, "{a:?} != {e:?}");
    }
}

/// Faktor des Blattzubehörs auf A4 (297 / 420).
const F4: f64 = 297.0 / 420.0;

fn seg(s: (DVec2, DVec2)) -> [f64; 4] {
    [s.0.x, s.0.y, s.1.x, s.1.y]
}

fn overlap(a: Box2, b: Box2) -> bool {
    a.0.x < b.1.x - 1e-9 && b.0.x < a.1.x - 1e-9 && a.0.y < b.1.y - 1e-9 && b.0.y < a.1.y - 1e-9
}

fn inside(inner: Box2, outer: Box2) -> bool {
    inner.0.x >= outer.0.x - 1e-9
        && inner.0.y >= outer.0.y - 1e-9
        && inner.1.x <= outer.1.x + 1e-9
        && inner.1.y <= outer.1.y + 1e-9
}

#[test]
fn scale_factor_per_paper() {
    for (p, f) in [
        (Paper::A4, F4),
        (Paper::A3, 1.0),
        (Paper::A2, 594.0 / 420.0),
        (Paper::A1, 841.0 / 420.0),
        (Paper::A0, 1189.0 / 420.0),
    ] {
        for landscape in [false, true] {
            assert_eq!(sheet_scale(sh(p, landscape)), f, "{p:?} {landscape}");
            assert_eq!(sheet_layout(sh(p, landscape), 0, 0).scale, f);
        }
    }
    assert!((sheet_scale(sh(Paper::A4, true)) - 0.707_14).abs() < 1e-5);
    assert!((sheet_scale(sh(Paper::A0, true)) - 2.8310).abs() < 1e-4);
}

#[test]
fn frame_rectangles() {
    // A3: Bezugsgröße, Ränder 20 und 10.
    near(
        bx(sheet_layout(sh(Paper::A3, true), 0, 0).frame),
        [20.0, 10.0, 410.0, 287.0],
    );
    near(
        bx(sheet_layout(sh(Paper::A3, false), 0, 0).frame),
        [20.0, 10.0, 287.0, 410.0],
    );
    // A4: Ränder 14,14 und 7,07.
    near6(
        bx(sheet_layout(sh(Paper::A4, false), 0, 0).frame),
        [14.142857, 7.071429, 202.928571, 289.928571],
    );
    near6(
        bx(sheet_layout(sh(Paper::A4, true), 0, 0).frame),
        [14.142857, 7.071429, 289.928571, 202.928571],
    );
    near6(
        bx(sheet_layout(sh(Paper::A2, true), 0, 0).frame),
        [28.285714, 14.142857, 579.857143, 405.857143],
    );
    near6(
        bx(sheet_layout(sh(Paper::A0, true), 0, 0).frame),
        [56.619048, 28.309524, 1160.690476, 812.690476],
    );
    // Linienstärken: A3-Werte, mitwachsend, nie unter 0,18.
    assert_eq!(SheetPen::Heavy.width_mm(), 0.7);
    assert_eq!(SheetPen::Medium.width_mm(), 0.5);
    assert_eq!(SheetPen::Thin.width_mm(), 0.25);
    for pen in [SheetPen::Heavy, SheetPen::Medium, SheetPen::Thin] {
        assert_eq!(pen.width_at(1.0), pen.width_mm());
    }
    assert!((SheetPen::Heavy.width_at(F4) - 0.495).abs() < 1e-12);
    assert!((SheetPen::Medium.width_at(F4) - 0.353_571_428_571).abs() < 1e-9);
    assert_eq!(SheetPen::Thin.width_at(F4), MIN_PEN_MM);
    assert_eq!(MIN_PEN_MM, 0.18);
    assert!((SheetPen::Heavy.width_at(1189.0 / 420.0) - 1.981_666_666_667).abs() < 1e-9);
    let a4 = sheet_layout(sh(Paper::A4, true), 0, 0);
    assert_eq!(a4.pen_width(SheetPen::Thin), 0.18);
}

#[test]
fn centre_marks_a4_landscape() {
    let l = sheet_layout(sh(Paper::A4, true), 0, 0);
    near6(seg(l.centre_marks[0]), [0.0, 105.0, 17.678571, 105.0]);
    near6(seg(l.centre_marks[1]), [297.0, 105.0, 286.392857, 105.0]);
    near6(seg(l.centre_marks[2]), [148.5, 0.0, 148.5, 10.607143]);
    near6(seg(l.centre_marks[3]), [148.5, 210.0, 148.5, 199.392857]);
    // A3 unverändert.
    let l = sheet_layout(sh(Paper::A3, true), 0, 0);
    near(seg(l.centre_marks[0]), [0.0, 148.5, 25.0, 148.5]);
    near(seg(l.centre_marks[1]), [420.0, 148.5, 405.0, 148.5]);
    near(seg(l.centre_marks[2]), [210.0, 0.0, 210.0, 15.0]);
    near(seg(l.centre_marks[3]), [210.0, 297.0, 210.0, 282.0]);
}

#[test]
fn grid_counts_and_labels() {
    for (len, n) in [
        (210.0, 4),
        (297.0, 6),
        (420.0, 8),
        (594.0, 12),
        (841.0, 16),
        (1189.0, 24),
    ] {
        assert_eq!(grid_count(len), n, "{len}");
    }
    // A3 quer: 8 × 6 Felder zu 50 mm.
    let l = sheet_layout(sh(Paper::A3, true), 0, 0);
    assert_eq!(l.grid_ticks.len(), 20);
    let has = |l: &SheetLayout, e: [f64; 4], tol: f64| {
        l.grid_ticks.iter().any(|t| {
            let s = seg(*t);
            (0..4).all(|i| (s[i] - e[i]).abs() < tol)
        })
    };
    for x in [60.0, 110.0, 160.0, 260.0, 310.0, 360.0] {
        assert!(has(&l, [x, 287.0, x, 292.0], 1e-9), "oben {x}");
        assert!(has(&l, [x, 10.0, x, 5.0], 1e-9), "unten {x}");
    }
    for y in [48.5, 98.5, 198.5, 248.5] {
        assert!(has(&l, [20.0, y, 15.0, y], 1e-9), "links {y}");
        assert!(has(&l, [410.0, y, 415.0, y], 1e-9), "rechts {y}");
    }
    assert_eq!(l.grid_labels.len(), 28);
    let lab = |l: &SheetLayout, t: &str, x: f64, y: f64, tol: f64| {
        l.grid_labels
            .iter()
            .any(|(p, s)| s == t && (p.x - x).abs() < tol && (p.y - y).abs() < tol)
    };
    assert!(lab(&l, "1", 40.0, 5.0, 1e-9));
    assert!(lab(&l, "1", 40.0, 292.0, 1e-9));
    assert!(lab(&l, "A", 10.0, 267.75, 1e-9));
    assert!(lab(&l, "A", 415.0, 267.75, 1e-9));
    // A4 quer: dieselben 8 × 6 Felder, Feldlänge 35,36 mm, Striche 3,54 mm.
    let l = sheet_layout(sh(Paper::A4, true), 0, 0);
    assert_eq!(l.grid_ticks.len(), 20);
    for x in [
        42.428571, 77.785714, 113.142857, 183.857143, 219.214286, 254.571429,
    ] {
        assert!(has(&l, [x, 202.928571, x, 206.464286], 1e-6), "oben {x}");
        assert!(has(&l, [x, 7.071429, x, 3.535714], 1e-6), "unten {x}");
    }
    for y in [34.285714, 69.642857, 140.357143, 175.714286] {
        assert!(has(&l, [14.142857, y, 10.607143, y], 1e-6), "links {y}");
        assert!(has(&l, [289.928571, y, 293.464286, y], 1e-6), "rechts {y}");
    }
    assert_eq!(l.grid_labels.len(), 28);
    assert!(lab(&l, "1", 28.285714, 3.535714, 1e-6));
    assert!(lab(&l, "1", 28.285714, 206.464286, 1e-6));
    assert!(lab(&l, "A", 7.071429, 189.321429, 1e-6));
    assert!(lab(&l, "A", 293.464286, 189.321429, 1e-6));
    // Jedes Format hat das Raster des vergrößerten oder verkleinerten A3-Blatts.
    for p in PAPERS {
        for landscape in [false, true] {
            let l = sheet_layout(sh(p, landscape), 0, 0);
            let (nx, ny) = if landscape { (8, 6) } else { (6, 8) };
            assert_eq!(l.grid_labels.len(), 2 * (nx + ny), "{p:?} {landscape}");
            assert_eq!(
                l.grid_ticks.len(),
                2 * (nx - 2 + ny - 2),
                "{p:?} {landscape}"
            );
        }
    }
    let a0 = sheet_layout(sh(Paper::A0, false), 0, 0);
    let rows: Vec<&String> = a0
        .grid_labels
        .iter()
        .filter(|(p, _)| (p.x - a0.mm(10.0)).abs() < 1e-9)
        .map(|(_, s)| s)
        .collect();
    assert_eq!(rows, ["H", "G", "F", "E", "D", "C", "B", "A"]);
    assert_eq!(grid_letter(0), Some('A'));
    assert_eq!(grid_letter(8), Some('J'));
    assert_eq!(grid_letter(23), Some('Z'));
    assert_eq!(grid_letter(24), None);
}

#[test]
fn title_block_and_tables() {
    // A3 hoch: Rahmenbreite 267 < 310, Hinweise über dem Schriftfeld.
    let l = sheet_layout(sh(Paper::A3, false), 0, 3);
    near(bx(l.title), [107.0, 10.0, 287.0, 66.0]);
    near(bx(l.revisions), [111.0, 403.0, 287.0, 410.0]);
    near(bx(l.notes.unwrap()), [107.0, 66.0, 287.0, 88.0]);
    let l = sheet_layout(sh(Paper::A3, false), 9, 0);
    assert!((l.revisions.0.y - 382.0).abs() < 1e-9);
    assert!(l.notes.is_none());
    let l = sheet_layout(sh(Paper::A3, true), 0, 3);
    near(bx(l.title), [230.0, 10.0, 410.0, 66.0]);
    near(bx(l.revisions), [234.0, 280.0, 410.0, 287.0]);
    near(bx(l.notes.unwrap()), [110.0, 10.0, 220.0, 32.0]);
    // A4 hoch: verkleinertes A3 hoch (Schriftfeld 127,3 × 39,6).
    let l = sheet_layout(sh(Paper::A4, false), 0, 3);
    near6(bx(l.title), [75.642857, 7.071429, 202.928571, 46.671429]);
    near6(
        bx(l.revisions),
        [78.471429, 284.978571, 202.928571, 289.928571],
    );
    near6(
        bx(l.notes.unwrap()),
        [75.642857, 46.671429, 202.928571, 62.228571],
    );
    let l = sheet_layout(sh(Paper::A4, false), 9, 0);
    assert!((l.revisions.0.y - (289.928571 - 4.0 * 7.0 * F4)).abs() < 1e-6);
    // A4 quer: Rahmenbreite 275,8 ≥ 310 · 0,707, Hinweise daneben wie auf A3 quer.
    let l = sheet_layout(sh(Paper::A4, true), 0, 3);
    near6(bx(l.title), [162.642857, 7.071429, 289.928571, 46.671429]);
    near6(
        bx(l.revisions),
        [165.471429, 197.978571, 289.928571, 202.928571],
    );
    near6(
        bx(l.notes.unwrap()),
        [77.785714, 7.071429, 155.571429, 22.628571],
    );
    // A2 quer und hoch, A0 quer.
    let l = sheet_layout(sh(Paper::A2, true), 0, 3);
    near6(bx(l.title), [325.285714, 14.142857, 579.857143, 93.342857]);
    near6(
        bx(l.revisions),
        [330.942857, 395.957143, 579.857143, 405.857143],
    );
    near6(
        bx(l.notes.unwrap()),
        [155.571429, 14.142857, 311.142857, 45.257143],
    );
    let l = sheet_layout(sh(Paper::A2, false), 0, 3);
    near6(
        bx(l.notes.unwrap()),
        [151.285714, 93.342857, 405.857143, 124.457143],
    );
    let l = sheet_layout(sh(Paper::A0, true), 0, 3);
    near6(
        bx(l.title),
        [651.119048, 28.309524, 1160.690476, 186.842857],
    );
    near6(
        bx(l.revisions),
        [662.442857, 792.873810, 1160.690476, 812.690476],
    );
    near6(
        bx(l.notes.unwrap()),
        [311.404762, 28.309524, 622.809524, 90.590476],
    );

    assert_eq!(TITLE_CELLS.len(), 17);
    let mut area = 0.0;
    for (i, a) in TITLE_CELLS.iter().enumerate() {
        area += (a.x1 - a.x0) * (a.y1 - a.y0);
        assert!(a.x0 >= 0.0 && a.x1 <= TITLE_W && a.y0 >= 0.0 && a.y1 <= TITLE_H);
        for b in &TITLE_CELLS[i + 1..] {
            let o = a.x0 < b.x1 - 1e-9
                && b.x0 < a.x1 - 1e-9
                && a.y0 < b.y1 - 1e-9
                && b.y0 < a.y1 - 1e-9;
            assert!(!o, "{} / {}", a.label, b.label);
        }
    }
    assert!((area - 180.0 * 56.0).abs() < 1e-9);
    assert!(
        TITLE_CELLS
            .iter()
            .any(|c| c.x0 == 135.0 && c.x1 == 180.0 && c.y0 == 28.0 && c.y1 == 38.0)
    );
    for s in &TITLE_SLOTS {
        let c = TITLE_CELLS
            .iter()
            .find(|c| {
                s.x0 >= c.x0
                    && s.x0 + s.width <= c.x1
                    && s.baseline >= c.y0
                    && s.baseline + s.height <= c.y1
            })
            .unwrap_or_else(|| panic!("{:?} in keiner Zelle", s.field));
        if s.field != TitleField::Subtitle {
            assert_eq!(c.label, s.field.label());
        } else {
            assert_eq!(c.label, "Titel");
        }
        // unter dem Feldnamen (Grundlinie y1 - 1 - LABEL_H)
        assert!(
            s.baseline + s.height <= c.y1 - 1.0 - LABEL_H + 1e-9,
            "{:?}",
            s.field
        );
    }
    let slot = |f| TITLE_SLOTS.iter().find(|s| s.field == f).unwrap();
    let (ti, su) = (slot(TitleField::Title), slot(TitleField::Subtitle));
    assert!(
        su.baseline + su.height <= ti.baseline,
        "Titel und Zusatztitel überlappen"
    );
    for f in TitleField::ALL {
        assert_eq!(TITLE_SLOTS.iter().filter(|s| s.field == f).count(), 1);
    }
    assert_eq!(TitleField::Title.max_chars(), 60);
    assert_eq!(TitleField::Owner.max_chars(), 80);
    assert_eq!(TitleField::Revision.label(), "Index");
    assert_eq!(Paper::A4.label(), "A4");
    assert_eq!(Paper::A0.label(), "A0");
}

#[test]
fn free_area_examples() {
    near(
        bx(sheet_layout(sh(Paper::A3, true), 0, 3).free),
        [20.0, 66.0, 410.0, 280.0],
    );
    near(
        bx(sheet_layout(sh(Paper::A3, false), 0, 3).free),
        [20.0, 88.0, 287.0, 403.0],
    );
    near6(
        bx(sheet_layout(sh(Paper::A4, false), 0, 3).free),
        [14.142857, 62.228571, 202.928571, 284.978571],
    );
    near6(
        bx(sheet_layout(sh(Paper::A4, true), 0, 3).free),
        [14.142857, 46.671429, 289.928571, 197.978571],
    );
    near6(
        bx(sheet_layout(sh(Paper::A2, true), 0, 3).free),
        [28.285714, 93.342857, 579.857143, 395.957143],
    );
    near6(
        bx(sheet_layout(sh(Paper::A0, true), 0, 3).free),
        [56.619048, 186.842857, 1160.690476, 792.873810],
    );
}

/// Jedes Format ist das A3-Blatt gleicher Lage, mit dem Faktor vergrößert oder verkleinert:
/// Ränder, Schriftfeld, Zellen, Tabellen und Hinweisfeld genau A3 mal Faktor; Rahmen und freier
/// Bereich im selben Verhältnis zum Blatt (bis auf die Abweichung der Papiergrößen von √2,
/// höchstens 0,1 %).
#[test]
fn every_paper_is_a_scaled_a3() {
    let rel = |b: Box2, w: f64, h: f64| [b.0.x / w, b.0.y / h, b.1.x / w, b.1.y / h];
    let size = |b: Box2| (b.1.x - b.0.x, b.1.y - b.0.y);
    for p in PAPERS {
        for landscape in [false, true] {
            for rev in 0..=9 {
                for notes in 0..=8 {
                    let sheet = sh(p, landscape);
                    let a3 = sheet_layout(sh(Paper::A3, landscape), rev, notes);
                    let l = sheet_layout(sheet, rev, notes);
                    let f = sheet_scale(sheet);
                    let ctx = format!("{p:?} {landscape} {rev} {notes}");
                    let (w, h) = sheet.size_mm();
                    let (w3, h3) = sh(Paper::A3, landscape).size_mm();
                    // Ränder.
                    let m = |l: &SheetLayout, w: f64, h: f64| {
                        [l.frame.0.x, l.frame.0.y, w - l.frame.1.x, h - l.frame.1.y]
                    };
                    let (ml, m3) = (m(&l, w, h), m(&a3, w3, h3));
                    for i in 0..4 {
                        assert!((ml[i] - m3[i] * f).abs() < 1e-9, "{ctx}: Rand {i}");
                    }
                    // Schriftfeld, Änderungstabelle, Hinweisfeld: gleiche Größe mal Faktor.
                    let close = |a: (f64, f64), b: (f64, f64)| {
                        (a.0 - b.0 * f).abs() < 1e-9 && (a.1 - b.1 * f).abs() < 1e-9
                    };
                    assert!(close(size(l.title), size(a3.title)), "{ctx}: Schriftfeld");
                    assert!(
                        close(size(l.revisions), size(a3.revisions)),
                        "{ctx}: Tabelle"
                    );
                    assert_eq!(l.notes.is_some(), a3.notes.is_some(), "{ctx}");
                    if let (Some(n), Some(n3)) = (l.notes, a3.notes) {
                        assert!(close(size(n), size(n3)), "{ctx}: Hinweise");
                        // Gleiche Stellung: neben oder über dem Schriftfeld wie auf A3.
                        let beside = |l: &SheetLayout, n: Box2| (n.0.y - l.title.0.y).abs() < 1e-9;
                        assert_eq!(beside(&l, n), beside(&a3, n3), "{ctx}: Stellung");
                    }
                    // Zellen und Inhaltsplätze.
                    for (c, c3) in l.title_cells().iter().zip(TITLE_CELLS.iter()) {
                        for (a, b) in [(c.x0, c3.x0), (c.x1, c3.x1), (c.y0, c3.y0), (c.y1, c3.y1)] {
                            assert!((a - b * f).abs() < 1e-9, "{ctx}: {}", c.label);
                        }
                    }
                    for (s, s3) in l.title_slots().iter().zip(TITLE_SLOTS.iter()) {
                        assert!((s.width - s3.width * f).abs() < 1e-9, "{ctx}");
                        assert!((s.height - s3.height * f).abs() < 1e-9, "{ctx}");
                    }
                    // Rahmen und freier Bereich im Verhältnis zum Blatt wie auf A3.
                    for (b, b3, what) in [(l.frame, a3.frame, "Rahmen"), (l.free, a3.free, "frei")]
                    {
                        let (r, r3) = (rel(b, w, h), rel(b3, w3, h3));
                        for i in 0..4 {
                            assert!((r[i] - r3[i]).abs() < 1e-3, "{ctx}: {what} {r:?} {r3:?}");
                        }
                    }
                    assert!(l.free.1.x - l.free.0.x > 0.0 && l.free.1.y - l.free.0.y > 0.0);
                }
            }
        }
    }
}

#[test]
fn free_area_never_empty() {
    for p in PAPERS {
        for landscape in [false, true] {
            for rev in 0..=9 {
                for notes in 0..=8 {
                    let l = sheet_layout(sh(p, landscape), rev, notes);
                    let f = l.free;
                    let ctx = format!("{p:?} {landscape} {rev} {notes}");
                    assert!(f.1.x - f.0.x > 0.0 && f.1.y - f.0.y > 0.0, "{ctx}");
                    assert!(inside(f, l.frame), "{ctx}");
                    assert!(!overlap(f, l.title) && !overlap(f, l.revisions), "{ctx}");
                    if let Some(n) = l.notes {
                        assert!(!overlap(f, n) && inside(n, l.frame), "{ctx}");
                        assert!(!overlap(n, l.title) && !overlap(n, l.revisions), "{ctx}");
                    }
                    assert!(
                        inside(l.title, l.frame) && inside(l.revisions, l.frame),
                        "{ctx}"
                    );
                    assert!(!overlap(l.revisions, l.title), "{ctx}");
                }
            }
        }
    }
}

#[test]
fn projection_symbol_sides() {
    // Zelle Projektion 135..180 x 47..56, Feldname links oben.
    let (x0, cy) = (PROJECTION_X0, PROJECTION_CENTRE_Y);
    let label_end = 135.0 + 1.0 + 6.0 * LABEL_H; // reichlich bemessene Breite von "Projektion"
    for proj in [Projection::First, Projection::Third] {
        let (lines, circles) = projection_symbol(proj, cy, x0, 1.0);
        assert_eq!(lines.len(), 4);
        assert_eq!(circles[0].1, 3.5);
        assert_eq!(circles[1].1, 1.75);
        assert_eq!(circles[0].0, circles[1].0);
        let cx = circles[0].0.x;
        let pts: Vec<DVec2> = lines.iter().flat_map(|s| [s.0, s.1]).collect();
        let (tmin, tmax) = pts
            .iter()
            .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.x), a.1.max(p.x)));
        let h_at = |x: f64| {
            let ys: Vec<f64> = pts
                .iter()
                .filter(|p| (p.x - x).abs() < 1e-9)
                .map(|p| p.y)
                .collect();
            ys.iter().cloned().fold(f64::MIN, f64::max)
                - ys.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!((h_at(tmin) - 3.5).abs() < 1e-9);
        assert!((h_at(tmax) - 7.0).abs() < 1e-9);
        assert!((tmax - tmin - 7.0).abs() < 1e-9);
        match proj {
            Projection::First => assert!((cx - 3.5 - tmax - 3.5).abs() < 1e-9),
            Projection::Third => assert!((tmin - (cx + 3.5) - 3.5).abs() < 1e-9),
        }
        let lo = tmin.min(cx - 3.5);
        let hi = tmax.max(cx + 3.5);
        assert!(lo > label_end && hi < 180.0);
        for p in &pts {
            assert!(p.y >= 47.0 && p.y <= 56.0);
        }
        assert!(circles[0].0.y - 3.5 >= 47.0 && circles[0].0.y + 3.5 <= 56.0);
        // Auf A4 dasselbe Symbol, verkleinert.
        let (small, sc) = projection_symbol(proj, cy * F4, x0 * F4, F4);
        for (a, b) in small.iter().zip(&lines) {
            assert!((a.0 - b.0 * F4).length() < 1e-9 && (a.1 - b.1 * F4).length() < 1e-9);
        }
        assert!((sc[0].1 - 3.5 * F4).abs() < 1e-12 && (sc[1].1 - 1.75 * F4).abs() < 1e-12);
    }
}

#[test]
fn centre_marks_reach_five_scaled_mm_past_the_frame_on_every_side() {
    for p in PAPERS {
        for landscape in [false, true] {
            let sheet = sh(p, landscape);
            let (w, h) = sheet.size_mm();
            let l = sheet_layout(sheet, 0, 0);
            let (f0, f1) = l.frame;
            let m = &l.centre_marks;
            let ctx = format!("{p:?} {landscape}");
            let inset = l.mm(CENTRE_MARK_INSET_MM);
            let eq = |a: (f64, f64), b: (f64, f64)| {
                assert!(
                    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9,
                    "{ctx}: {a:?} {b:?}"
                )
            };
            // links, rechts, unten, oben: Start am Blattrand, Ende 5 mm (mal Faktor) hinter dem
            // Rahmen
            eq((m[0].0.x, m[0].1.x), (0.0, f0.x + inset));
            eq((m[1].0.x, m[1].1.x), (w, f1.x - inset));
            eq((m[2].0.y, m[2].1.y), (0.0, f0.y + inset));
            eq((m[3].0.y, m[3].1.y), (h, f1.y - inset));
            assert_eq!(m[0].0.y, h / 2.0);
            assert_eq!(m[1].0.y, h / 2.0);
            assert_eq!(m[2].0.x, w / 2.0);
            assert_eq!(m[3].0.x, w / 2.0);
        }
    }
}
