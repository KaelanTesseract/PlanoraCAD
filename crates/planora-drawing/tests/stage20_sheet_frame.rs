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
fn frame_rectangles() {
    near(
        bx(sheet_layout(sh(Paper::A4, false), 0, 0).frame),
        [20.0, 10.0, 200.0, 287.0],
    );
    near(
        bx(sheet_layout(sh(Paper::A4, true), 0, 0).frame),
        [20.0, 10.0, 287.0, 200.0],
    );
    near(
        bx(sheet_layout(sh(Paper::A3, true), 0, 0).frame),
        [20.0, 10.0, 410.0, 287.0],
    );
    near(
        bx(sheet_layout(sh(Paper::A0, true), 0, 0).frame),
        [20.0, 10.0, 1179.0, 831.0],
    );
    assert_eq!(SheetPen::Heavy.width_mm(), 0.7);
    assert_eq!(SheetPen::Medium.width_mm(), 0.5);
    assert_eq!(SheetPen::Thin.width_mm(), 0.25);
}

#[test]
fn centre_marks_a4_landscape() {
    let l = sheet_layout(sh(Paper::A4, true), 0, 0);
    near(seg(l.centre_marks[0]), [0.0, 105.0, 25.0, 105.0]);
    near(seg(l.centre_marks[1]), [297.0, 105.0, 282.0, 105.0]);
    near(seg(l.centre_marks[2]), [148.5, 0.0, 148.5, 15.0]);
    near(seg(l.centre_marks[3]), [148.5, 210.0, 148.5, 195.0]);
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
    let l = sheet_layout(sh(Paper::A4, true), 0, 0);
    assert_eq!(l.grid_ticks.len(), 12);
    let has = |e: [f64; 4]| {
        l.grid_ticks.iter().any(|t| {
            let s = seg(*t);
            (0..4).all(|i| (s[i] - e[i]).abs() < 1e-9)
        })
    };
    for x in [48.5, 98.5, 198.5, 248.5] {
        assert!(has([x, 200.0, x, 205.0]), "oben {x}");
        assert!(has([x, 10.0, x, 5.0]), "unten {x}");
    }
    for y in [55.0, 155.0] {
        assert!(has([20.0, y, 15.0, y]), "links {y}");
        assert!(has([287.0, y, 292.0, y]), "rechts {y}");
    }
    assert_eq!(l.grid_labels.len(), 20);
    let lab = |t: &str, x: f64, y: f64| {
        l.grid_labels
            .iter()
            .any(|(p, s)| s == t && (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9)
    };
    assert!(lab("1", 34.25, 5.0));
    assert!(lab("1", 34.25, 205.0));
    assert!(lab("A", 10.0, 177.5));
    assert!(lab("A", 292.0, 177.5));
    let a0 = sheet_layout(sh(Paper::A0, false), 0, 0);
    let rows: Vec<&String> = a0
        .grid_labels
        .iter()
        .filter(|(p, _)| p.x == 10.0)
        .map(|(_, s)| s)
        .collect();
    assert_eq!(rows.len(), 24);
    assert!(rows.iter().all(|s| *s != "I" && *s != "O"));
    assert_eq!(grid_letter(0), Some('A'));
    assert_eq!(grid_letter(8), Some('J'));
    assert_eq!(grid_letter(23), Some('Z'));
    assert_eq!(grid_letter(24), None);
}

#[test]
fn title_block_and_tables() {
    let l = sheet_layout(sh(Paper::A4, false), 0, 3);
    near(bx(l.title), [20.0, 10.0, 200.0, 66.0]);
    near(bx(l.revisions), [24.0, 280.0, 200.0, 287.0]);
    near(bx(l.notes.unwrap()), [20.0, 66.0, 200.0, 88.0]);
    let l = sheet_layout(sh(Paper::A4, false), 9, 0);
    assert!((l.revisions.0.y - 238.0).abs() < 1e-9);
    assert!(l.notes.is_none());
    let l = sheet_layout(sh(Paper::A3, true), 0, 3);
    near(bx(l.notes.unwrap()), [110.0, 10.0, 220.0, 32.0]);
    // A2 hoch: Rahmenbreite 400, Hinweise daneben (F9).
    let l = sheet_layout(sh(Paper::A2, false), 0, 3);
    let n = l.notes.unwrap();
    assert!((n.0.y - l.title.0.y).abs() < 1e-9 && n.1.x <= l.title.0.x - 10.0 + 1e-9);

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
    assert_eq!(TitleField::Revision.label(), "Änderung");
    assert_eq!(Paper::A4.label(), "A4");
    assert_eq!(Paper::A0.label(), "A0");
}

#[test]
fn free_area_examples() {
    near(
        bx(sheet_layout(sh(Paper::A4, false), 0, 3).free),
        [20.0, 88.0, 200.0, 280.0],
    );
    near(
        bx(sheet_layout(sh(Paper::A4, true), 0, 3).free),
        [20.0, 88.0, 287.0, 193.0],
    );
    near(
        bx(sheet_layout(sh(Paper::A3, true), 0, 3).free),
        [20.0, 66.0, 410.0, 280.0],
    );
    near(
        bx(sheet_layout(sh(Paper::A0, true), 0, 3).free),
        [20.0, 66.0, 1179.0, 824.0],
    );
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
        let (lines, circles) = projection_symbol(proj, cy, x0);
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
    }
}

#[test]
fn centre_marks_reach_five_mm_past_the_frame_on_every_side() {
    for p in PAPERS {
        for landscape in [false, true] {
            let sheet = sh(p, landscape);
            let (w, h) = sheet.size_mm();
            let l = sheet_layout(sheet, 0, 0);
            let (f0, f1) = l.frame;
            let m = &l.centre_marks;
            let ctx = format!("{p:?} {landscape}");
            // links, rechts, unten, oben: Start am Blattrand, Ende 5 mm hinter dem Rahmen
            assert_eq!(
                (m[0].0.x, m[0].1.x),
                (0.0, f0.x + CENTRE_MARK_INSET_MM),
                "{ctx}"
            );
            assert_eq!(
                (m[1].0.x, m[1].1.x),
                (w, f1.x - CENTRE_MARK_INSET_MM),
                "{ctx}"
            );
            assert_eq!(
                (m[2].0.y, m[2].1.y),
                (0.0, f0.y + CENTRE_MARK_INSET_MM),
                "{ctx}"
            );
            assert_eq!(
                (m[3].0.y, m[3].1.y),
                (h, f1.y - CENTRE_MARK_INSET_MM),
                "{ctx}"
            );
            assert_eq!(m[0].0.y, h / 2.0);
            assert_eq!(m[1].0.y, h / 2.0);
            assert_eq!(m[2].0.x, w / 2.0);
            assert_eq!(m[3].0.x, w / 2.0);
        }
    }
}
