//! Stufe 33: Schraffur, Schnittlinie und Titel als Markierungen (`section_marks.rs`).

use planora_drawing::annotate::{
    DIM_TEXT_H, Mark, MarkKind, SECTION_ARROW_HALF_W, SECTION_ARROW_LEN, SectionPen, TextAnchor,
    arrow_size,
};
use planora_drawing::hatch::MAX_HATCH_LINES;
use planora_drawing::section::{SectionData, SectionPlane, compute_section};
use planora_drawing::section_marks::*;
use planora_drawing::sheet::{SheetModel, build_sheet, view_transform};
use planora_drawing::sheet_frame::sheet_scale;
use planora_drawing::{
    DrawBody, Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
    ViewLines, compute_view,
};
use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, BoolOp, Kernel, Solid};
use planora_math::{DMat4, DVec2, DVec3};
use std::collections::BTreeMap;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, x: f64, y: f64, z: f64) -> Solid {
    s.transformed(&DMat4::from_translation(DVec3::new(x, y, z)))
        .unwrap()
}

fn diff(a: &Solid, b: &Solid) -> Solid {
    let mut s = k().boolean(a, b, BoolOp::Difference).unwrap();
    s.assign_origin(7);
    s
}

fn boxs(a: f64, b: f64, c: f64) -> Solid {
    let mut s = k().make_box(DVec3::new(a, b, c)).unwrap();
    s.assign_origin(7);
    s
}

/// Rohr: außen r 10, innen r 6, Höhe 20, Achse z durch den Ursprung.
fn tube() -> Solid {
    let outer = k().make_cylinder(10.0, 20.0).unwrap();
    let inner = moved(&k().make_cylinder(6.0, 24.0).unwrap(), 0.0, 0.0, -2.0);
    diff(&outer, &inner)
}

fn db<'a>(name: &str, solid: &'a Solid) -> DrawBody<'a> {
    DrawBody {
        name: name.into(),
        solid,
    }
}

fn sheet_of(paper: Paper) -> Sheet {
    Sheet {
        paper,
        landscape: true,
    }
}

const S11: Scale = Scale { num: 1, den: 1 };

struct Fix {
    d: Drawing,
    src: ViewId,
    sec: ViewId,
    src_lines: ViewLines,
    sec_lines: planora_drawing::section::SectionLines,
}

fn fixture(
    bodies: &[DrawBody],
    paper: Paper,
    scale: Scale,
    src_kind: ViewKind,
    sec_kind: ViewKind,
    offset: f64,
) -> Fix {
    let mut d = Drawing::new(DrawingId(1), "T".into(), sheet_of(paper), Projection::First);
    let src = d.add_view(
        src_kind,
        scale,
        Placement::Free { x: 150.0, y: 200.0 },
        None,
    );
    let sec = d.add_section(src, sec_kind, offset, 120.0).unwrap();
    let src_lines = compute_view(bodies, src_kind, &Progress::new()).unwrap();
    let sec_lines = compute_section(
        bodies,
        &SectionPlane {
            kind: sec_kind,
            offset,
        },
        compute_view,
        &Progress::new(),
    )
    .unwrap();
    Fix {
        d,
        src,
        sec,
        src_lines,
        sec_lines,
    }
}

impl Fix {
    fn results(&self) -> BTreeMap<ViewId, &ViewLines> {
        BTreeMap::from([
            (self.src, &self.src_lines),
            (self.sec, &self.sec_lines.lines),
        ])
    }
    fn sections(&self) -> BTreeMap<ViewId, &SectionData> {
        BTreeMap::from([(self.sec, &self.sec_lines.data)])
    }
    fn marks(&self) -> (Vec<SectionMarks>, Vec<String>) {
        section_marks(&self.d, &self.results(), &self.sections())
    }
    fn sheet(&self) -> SheetModel {
        build_sheet(&self.d, &self.results())
    }
    fn set_spacing(&mut self, spacing: f64) {
        for v in &mut self.d.views {
            if let Some(s) = &mut v.section {
                s.hatch.spacing = spacing;
            }
        }
    }
    fn set_hatch_on(&mut self, on: bool) {
        for v in &mut self.d.views {
            if let Some(s) = &mut v.section {
                s.hatch.on = on;
            }
        }
    }
}

fn of_view(list: &[SectionMarks], id: ViewId) -> Vec<Mark> {
    list.iter()
        .filter(|e| e.view == id)
        .flat_map(|e| e.marks.iter().cloned())
        .collect()
}

fn kind_of(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

fn hatch_segs(marks: &[Mark]) -> Vec<(DVec2, DVec2)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Line {
                a,
                b,
                kind: MarkKind::Hatch,
            } => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

fn pen_lines(marks: &[Mark], pen: SectionPen) -> Vec<(DVec2, DVec2)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Line {
                a,
                b,
                kind: MarkKind::SectionLine(p),
            } if *p == pen => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

fn arrows(marks: &[Mark]) -> Vec<(DVec2, DVec2)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Arrow { tip, dir, .. } => Some((*tip, *dir)),
            _ => None,
        })
        .collect()
}

struct T {
    pos: DVec2,
    height: f64,
    angle: f64,
    anchor: TextAnchor,
    text: String,
}

fn texts(marks: &[Mark]) -> Vec<T> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Text {
                pos,
                height,
                angle,
                anchor,
                text,
                ..
            } => Some(T {
                pos: *pos,
                height: *height,
                angle: *angle,
                anchor: *anchor,
                text: text.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn len(s: &(DVec2, DVec2)) -> f64 {
    (s.1 - s.0).length()
}

fn normal(angle_deg: f64) -> DVec2 {
    let a = angle_deg.to_radians();
    DVec2::new(-a.sin(), a.cos())
}

/// Aufsteigende, zusammengefasste Werte `n·p` aller Stücke.
fn rows_of(segs: &[(DVec2, DVec2)], n: DVec2) -> Vec<f64> {
    let mut cs: Vec<f64> = segs.iter().map(|s| n.dot(s.0)).collect();
    cs.sort_by(f64::total_cmp);
    cs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    cs
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn assert_spacing(segs: &[(DVec2, DVec2)], n: DVec2, want: f64, tol: f64) {
    let cs = rows_of(segs, n);
    assert!(cs.len() >= 3, "zu wenige Zeilen: {cs:?}");
    for w in cs.windows(2) {
        assert!(near(w[1] - w[0], want, tol), "{} statt {want}", w[1] - w[0]);
    }
}

fn tube_fix(paper: Paper, scale: Scale) -> (Solid, Paper, Scale) {
    (tube(), paper, scale)
}

fn tube_section(t: &Solid, paper: Paper, scale: Scale) -> Fix {
    fixture(
        &[db("Rohr", t)],
        paper,
        scale,
        ViewKind::Front,
        ViewKind::Top,
        10.0,
    )
}

#[test]
fn hatch_follows_sheet_not_scale() {
    let n45 = normal(45.0);
    let cases = [
        (Paper::A3, Scale { num: 1, den: 2 }, 3.0, 1e-9),
        (Paper::A3, Scale { num: 5, den: 1 }, 3.0, 1e-9),
        (Paper::A4, S11, 3.0 * 297.0 / 420.0, 1e-6),
        (Paper::A0, S11, 3.0 * 1189.0 / 420.0, 1e-6),
    ];
    for (paper, scale, want, tol) in cases {
        let (t, p, s) = tube_fix(paper, scale);
        let fx = tube_section(&t, p, s);
        let (list, errs) = fx.marks();
        assert!(errs.is_empty(), "{errs:?}");
        let own = of_view(&list, fx.sec);
        let segs = hatch_segs(&own);
        assert!(!segs.is_empty());
        assert_spacing(&segs, n45, want, tol);
        // Alle Linienstücke der Schnittansicht sind Schraffur.
        for m in &own {
            if let Mark::Line { kind, .. } = m {
                assert_eq!(*kind, MarkKind::Hatch);
            }
        }
        // Und alle liegen im Winkel 45 Grad.
        for (a, b) in &segs {
            let d = *b - *a;
            assert!(near(d.x, d.y, 1e-9 * d.length().max(1.0)), "{d:?}");
        }
    }
}

#[test]
fn spacing_and_switch() {
    let t = tube();
    let n45 = normal(45.0);
    for (paper, f) in [(Paper::A3, 1.0), (Paper::A4, 297.0 / 420.0)] {
        let mut fx = tube_section(&t, paper, S11);
        fx.set_spacing(5.0);
        let (list, _) = fx.marks();
        assert_spacing(&hatch_segs(&of_view(&list, fx.sec)), n45, 5.0 * f, 1e-6);
    }
    let mut fx = tube_section(&t, Paper::A3, S11);
    fx.set_hatch_on(false);
    let (list, errs) = fx.marks();
    assert!(errs.is_empty());
    let own = of_view(&list, fx.sec);
    assert!(own.iter().all(|m| kind_of(m) != MarkKind::Hatch));
    assert!(
        list.iter()
            .flat_map(|e| &e.marks)
            .all(|m| kind_of(m) != MarkKind::Hatch)
    );
    // Titel und Schnittlinie bleiben.
    assert_eq!(texts(&own).len(), 1);
    assert!(!pen_lines(&of_view(&list, fx.src), SectionPen::End).is_empty());
}

/// Länge des Stücks der Geraden `n·p = c` (Richtung `d`) im Rechteck `[lo, hi]`.
fn clip_len(lo: DVec2, hi: DVec2, n: DVec2, c: f64) -> f64 {
    let d = DVec2::new(n.y, -n.x);
    let p0 = n * c;
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for (p, dd, l, h) in [(p0.x, d.x, lo.x, hi.x), (p0.y, d.y, lo.y, hi.y)] {
        if dd.abs() < 1e-12 {
            if p < l || p > h {
                return 0.0;
            }
        } else {
            let (a, b) = ((l - p) / dd, (h - p) / dd);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
    }
    (t1 - t0).max(0.0)
}

fn dist_to_segment(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

#[test]
fn holes_stay_free_rows() {
    let t = tube();
    // Quer: Schnitt von oben, Blick auf die Ringfläche.
    let fx = tube_section(&t, Paper::A3, S11);
    let (list, _) = fx.marks();
    let segs = hatch_segs(&of_view(&list, fx.sec));
    let xf = view_transform(&fx.d, fx.sec, &fx.results()).unwrap();
    let centre = xf.apply(DVec2::new(0.0, 0.0));
    let n = normal(45.0);
    let mut per_row: BTreeMap<i64, usize> = BTreeMap::new();
    for s in &segs {
        *per_row
            .entry((n.dot(s.0) * 1e5).round() as i64)
            .or_default() += 1;
        for p in [s.0, s.1] {
            assert!((p - centre).length() >= 6.0 - 0.01, "{p:?}");
        }
        assert!(dist_to_segment(centre, s.0, s.1) >= 6.0 - 0.01);
    }
    assert!(per_row.values().all(|&c| c <= 2), "{per_row:?}");
    assert!(per_row.values().any(|&c| c == 2));

    // Längs: Schnitt von vorn durch die Achse; Streifen x -10..-6 und 6..10, z 0..20.
    let fx = fixture(
        &[db("Rohr", &t)],
        Paper::A3,
        S11,
        ViewKind::Top,
        ViewKind::Front,
        0.0,
    );
    let (list, errs) = fx.marks();
    assert!(errs.is_empty());
    let segs = hatch_segs(&of_view(&list, fx.sec));
    let xf = view_transform(&fx.d, fx.sec, &fx.results()).unwrap();
    let rects: Vec<(DVec2, DVec2)> = [(-10.0, -6.0), (6.0, 10.0)]
        .iter()
        .map(|&(x0, x1)| {
            (
                xf.apply(DVec2::new(x0, 0.0)),
                xf.apply(DVec2::new(x1, 20.0)),
            )
        })
        .collect();
    let c0 = n.dot(xf.apply(DVec2::new(0.0, 0.0)));
    let mut hit_rows = 0;
    for kk in -200..200 {
        let c = c0 + kk as f64 * 3.0;
        let want: f64 = rects.iter().map(|(lo, hi)| clip_len(*lo, *hi, n, c)).sum();
        let have: f64 = segs
            .iter()
            .filter(|s| near(n.dot(s.0), c, 1e-6))
            .map(len)
            .sum();
        assert!(near(want, have, 0.01), "Zeile {kk}: {have} statt {want}");
        let pieces = segs.iter().filter(|s| near(n.dot(s.0), c, 1e-6)).count();
        assert!(pieces <= 2);
        if want > 0.01 {
            hit_rows += 1;
        }
    }
    assert!(hit_rows > 8);
    // Stücke nur in den Streifen: jeder Endpunkt liegt in einem Streifen (mit Rand).
    for s in &segs {
        for p in [s.0, s.1] {
            assert!(
                rects.iter().any(|(lo, hi)| p.x >= lo.x - 0.01
                    && p.x <= hi.x + 0.01
                    && p.y >= lo.y - 0.01
                    && p.y <= hi.y + 0.01),
                "{p:?}"
            );
        }
    }
}

fn plate_with_bore() -> Solid {
    let drill = moved(&k().make_cylinder(5.0, 14.0).unwrap(), 0.0, 0.0, -2.0);
    diff(&moved(&boxs(40.0, 40.0, 10.0), -20.0, -20.0, 0.0), &drill)
}

fn bolt() -> Solid {
    let mut s = moved(&k().make_cylinder(5.0, 14.0).unwrap(), 0.0, 0.0, -2.0);
    s.assign_origin(7);
    s
}

fn check_plate_and_pin(bodies: &[DrawBody]) {
    let fx = fixture(bodies, Paper::A3, S11, ViewKind::Front, ViewKind::Top, 5.0);
    let (list, errs) = fx.marks();
    assert!(errs.is_empty());
    let segs = hatch_segs(&of_view(&list, fx.sec));
    let xf = view_transform(&fx.d, fx.sec, &fx.results()).unwrap();
    let centre = xf.apply(DVec2::new(0.0, 0.0));
    let (mut plate_n, mut pin_n) = (0, 0);
    let mut plate_closest = f64::INFINITY;
    for s in &segs {
        let d = s.1 - s.0;
        assert!(near(d.x.abs(), d.y.abs(), 1e-9 * d.length().max(1.0)));
        let rising = d.x * d.y > 0.0;
        if rising {
            // Platte, 45 Grad: kein Stück im Loch.
            plate_n += 1;
            let dist = dist_to_segment(centre, s.0, s.1);
            assert!(dist >= 5.0 - 0.01, "{s:?}");
            plate_closest = plate_closest.min(dist);
        } else {
            // Bolzen, 135 Grad: nichts außerhalb des Bolzens.
            pin_n += 1;
            for p in [s.0, s.1] {
                let r = (p - centre).length();
                // Das Stück reicht bis an den Rand (Untergrenze), nicht darüber hinaus.
                assert!((5.0 - 0.01..=5.0 + 0.01).contains(&r), "{p:?}");
            }
        }
    }
    assert!(plate_n > 4 && pin_n > 2, "{plate_n} {pin_n}");
    // Die Platte reicht bis auf einen Linienabstand an das Loch heran.
    assert!(plate_closest <= 5.0 + 3.0, "{plate_closest}");
    // Ein Spalt bleibt: jede Platten-Spitze am Loch liegt mindestens 5 - 0,005 entfernt, jede
    // Bolzen-Spitze höchstens 5; die Schar des Bolzens ist die 135-Grad-Schar.
    let n135 = normal(135.0);
    let c0 = n135.dot(xf.apply(DVec2::new(0.0, 0.0)));
    for s in segs.iter().filter(|s| (s.1 - s.0).x * (s.1 - s.0).y < 0.0) {
        let q = (n135.dot(s.0) - c0) / 3.0;
        assert!(near(q, q.round(), 1e-9), "{q}");
    }
}

#[test]
fn touching_bodies_alternate() {
    let plate = plate_with_bore();
    let pin = bolt();
    check_plate_and_pin(&[db("Platte", &plate), db("Bolzen", &pin)]);
    // Ein ungeschnittener Körper davor ändert den Rang nicht (Rang unter den Körpern mit
    // Schnittfläche, nicht der Index).
    let aside = moved(&boxs(10.0, 10.0, 10.0), 100.0, 0.0, -30.0);
    check_plate_and_pin(&[
        db("Daneben", &aside),
        db("Platte", &plate),
        db("Bolzen", &pin),
    ]);

    // Zwei Regionen desselben Körpers (U-Profil, an den Schenkeln geschnitten) teilen die Schar.
    let slot = moved(&boxs(16.0, 20.0, 14.0), 12.0, 5.0, -2.0);
    let u = diff(&boxs(40.0, 20.0, 10.0), &slot);
    let fx = fixture(
        &[db("U", &u)],
        Paper::A3,
        S11,
        ViewKind::Top,
        ViewKind::Front,
        12.0,
    );
    assert_eq!(fx.sec_lines.data.regions.len(), 2);
    let (list, errs) = fx.marks();
    assert!(errs.is_empty());
    let segs = hatch_segs(&of_view(&list, fx.sec));
    let xf = view_transform(&fx.d, fx.sec, &fx.results()).unwrap();
    let n = normal(45.0);
    let c0 = n.dot(xf.apply(DVec2::new(0.0, 0.0)));
    let mid = xf.apply(DVec2::new(20.0, 0.0)).x;
    let (mut left, mut right) = (0, 0);
    for s in &segs {
        let q = (n.dot(s.0) - c0) / 3.0;
        assert!(near(q, q.round(), 1e-9), "{q}");
        if s.0.x < mid {
            left += 1;
        } else {
            right += 1;
        }
    }
    assert!(left > 2 && right > 2);
}

#[test]
fn section_line_geometry() {
    let b = boxs(30.0, 20.0, 10.0);
    for paper in [Paper::A3, Paper::A4, Paper::A0] {
        let f = sheet_scale(sheet_of(paper));
        let fx = fixture(
            &[db("Quader", &b)],
            paper,
            S11,
            ViewKind::Front,
            ViewKind::Top,
            5.0,
        );
        let (list, errs) = fx.marks();
        assert!(errs.is_empty());
        let own = of_view(&list, fx.src);
        let sheet = fx.sheet();
        let width = sheet
            .views
            .iter()
            .find(|v| v.id == fx.src)
            .unwrap()
            .extent_mm
            .0;
        let xf = view_transform(&fx.d, fx.src, &fx.results()).unwrap();
        let y = xf.apply(DVec2::new(0.0, 5.0)).y;
        let x_lo = xf.tx - width / 2.0 - 5.0 * f;
        let x_hi = xf.tx + width / 2.0 + 5.0 * f;

        let ends = pen_lines(&own, SectionPen::End);
        assert_eq!(ends.len(), 2);
        for e in &ends {
            assert!(near(len(e), 8.0 * f, 1e-9), "{}", len(e));
            assert!(near(e.0.y, y, 1e-9) && near(e.1.y, y, 1e-9));
        }
        let mut xs: Vec<f64> = ends.iter().flat_map(|e| [e.0.x, e.1.x]).collect();
        xs.sort_by(f64::total_cmp);
        assert!(near(xs[0], x_lo, 1e-9) && near(xs[3], x_hi, 1e-9), "{xs:?}");
        assert!(near(xs[1], x_lo + 8.0 * f, 1e-9) && near(xs[2], x_hi - 8.0 * f, 1e-9));

        let chain = pen_lines(&own, SectionPen::Chain);
        let horiz: Vec<_> = chain.iter().filter(|c| near(c.0.y, c.1.y, 1e-12)).collect();
        assert_eq!(horiz.len(), 1);
        assert!(near(len(horiz[0]), width + 10.0 * f - 16.0 * f, 1e-9));
        assert!(near(horiz[0].0.y, y, 1e-9));
        // Schäfte: senkrecht, 5 f lang, von den Enden nach unten.
        let shafts: Vec<_> = chain.iter().filter(|c| near(c.0.x, c.1.x, 1e-12)).collect();
        assert_eq!(shafts.len(), 2);
        for s in &shafts {
            assert!(near(len(s), 5.0 * f, 1e-9));
            assert!(near(s.0.y, y, 1e-9) && near(s.1.y, y - 5.0 * f, 1e-9));
        }
        // Pfeile: Blick nach unten, Spitze 10 f unter dem Ende.
        let ar = arrows(&own);
        assert_eq!(ar.len(), 2);
        let mut tips: Vec<f64> = Vec::new();
        for (tip, dir) in &ar {
            assert_eq!(*dir, DVec2::new(0.0, 1.0));
            assert!(near(tip.y, y - 10.0 * f, 1e-9), "{tip:?}");
            tips.push(tip.x);
        }
        tips.sort_by(f64::total_cmp);
        assert!(near(tips[0], x_lo, 1e-9) && near(tips[1], x_hi, 1e-9));
        // Buchstaben mittig unter den Spitzen.
        let tx = texts(&own);
        assert_eq!(tx.len(), 2);
        let mut tpos: Vec<f64> = Vec::new();
        for t in &tx {
            assert_eq!(t.text, "A");
            assert!(near(t.height, DIM_TEXT_H, 1e-12));
            assert!(near(t.angle, 0.0, 1e-12));
            assert_eq!(t.anchor, TextAnchor::Middle);
            // Mitte = Spitze - (1,75 + f), Grundlinie eine halbe Höhe tiefer.
            assert!(
                near(
                    t.pos.y,
                    y - 10.0 * f - (DIM_TEXT_H / 2.0 + f) - DIM_TEXT_H / 2.0,
                    1e-9
                ),
                "{:?}",
                t.pos
            );
            tpos.push(t.pos.x);
        }
        tpos.sort_by(f64::total_cmp);
        assert!(near(tpos[0], x_lo, 1e-9) && near(tpos[1], x_hi, 1e-9));
        // Alles in der Quellansicht ist Schnittlinie.
        assert!(
            own.iter()
                .all(|m| matches!(kind_of(m), MarkKind::SectionLine(_)))
        );
    }
}

#[test]
fn section_line_directions() {
    let b = boxs(30.0, 20.0, 10.0);
    // (Art der Schnittansicht, Versatz, waagerecht?, Blickrichtung auf dem Blatt)
    let cases = [
        (ViewKind::Top, 5.0, true, DVec2::new(0.0, -1.0)),
        (ViewKind::Bottom, 5.0, true, DVec2::new(0.0, 1.0)),
        (ViewKind::Left, 12.0, false, DVec2::new(1.0, 0.0)),
        (ViewKind::Right, 12.0, false, DVec2::new(-1.0, 0.0)),
    ];
    for (kind, offset, horizontal, v) in cases {
        let fx = fixture(
            &[db("Q", &b)],
            Paper::A3,
            S11,
            ViewKind::Front,
            kind,
            offset,
        );
        let (list, _) = fx.marks();
        let own = of_view(&list, fx.src);
        let xf = view_transform(&fx.d, fx.src, &fx.results()).unwrap();
        let line = xf.apply(DVec2::new(offset, 5.0));
        let ends = pen_lines(&own, SectionPen::End);
        assert_eq!(ends.len(), 2, "{kind:?}");
        for e in &ends {
            if horizontal {
                assert!(
                    near(e.0.y, line.y, 1e-9) && near(e.1.y, line.y, 1e-9),
                    "{kind:?}"
                );
            } else {
                assert!(
                    near(e.0.x, line.x, 1e-9) && near(e.1.x, line.x, 1e-9),
                    "{kind:?}"
                );
            }
        }
        let ar = arrows(&own);
        assert_eq!(ar.len(), 2);
        for (tip, dir) in &ar {
            assert_eq!(*dir, DVec2::new(-v.x, -v.y), "{kind:?}");
            let along = if horizontal {
                tip.y - line.y
            } else {
                tip.x - line.x
            };
            assert!(near(along, 10.0 * (v.x + v.y), 1e-9), "{kind:?} {tip:?}");
        }
    }
}

#[test]
fn title() {
    let t = tube();
    let scale = Scale { num: 1, den: 2 };
    for paper in [Paper::A3, Paper::A0] {
        let f = sheet_scale(sheet_of(paper));
        let fx = tube_section(&t, paper, scale);
        let (list, _) = fx.marks();
        let own = of_view(&list, fx.sec);
        let tx = texts(&own);
        assert_eq!(tx.len(), 1);
        let t0 = &tx[0];
        assert_eq!(t0.text, "A\u{2013}A (1:2)");
        assert_eq!(section_title('A', scale), t0.text);
        assert!(near(t0.height, 5.0 * f, 1e-9));
        assert_eq!(t0.anchor, TextAnchor::Middle);
        assert!(matches!(
            own.iter()
                .find(|m| matches!(m, Mark::Text { .. }))
                .map(kind_of),
            Some(MarkKind::SectionLine(SectionPen::Chain))
        ));
        let sheet = fx.sheet();
        let sv = sheet.views.iter().find(|v| v.id == fx.sec).unwrap();
        assert_eq!(sv.name, section_view_name('A'));
        assert_eq!(sv.name, "Schnitt A\u{2013}A");
        let xf = view_transform(&fx.d, fx.sec, &fx.results()).unwrap();
        assert!(near(t0.pos.x, xf.tx, 1e-9));
        let bottom = xf.ty - sv.extent_mm.1 / 2.0;
        assert!(
            near(t0.pos.y, bottom - 5.0 * f - 5.0 * f, 1e-9),
            "{} statt {}",
            t0.pos.y,
            bottom - 10.0 * f
        );
    }
}

#[test]
fn too_fine() {
    let t = tube();
    let mut fx = tube_section(&t, Paper::A3, S11);
    let one = fx.sec_lines.data.regions.clone();
    for _ in 0..2000 {
        fx.sec_lines.data.regions.extend(one.iter().cloned());
    }
    let (list, errs) = fx.marks();
    assert_eq!(
        errs,
        vec!["Die Schraffur von „Schnitt A\u{2013}A“ ist zu fein.".to_string()]
    );
    assert!(!errs[0].chars().any(|c| c.is_ascii_digit()));
    let own = of_view(&list, fx.sec);
    assert!(own.iter().all(|m| kind_of(m) != MarkKind::Hatch));
    // Titel und Schnittlinie bleiben.
    assert_eq!(texts(&own).len(), 1);
    assert!(!of_view(&list, fx.src).is_empty());
}

#[test]
fn hatch_limit_is_exact() {
    let t = tube();
    // Abstand suchen, bei dem eine Fläche N Stücke gibt und N die 20 000 teilt.
    let mut found = None;
    let mut s = 3.0;
    while s > 0.5 {
        let mut fx = tube_section(&t, Paper::A3, S11);
        fx.set_spacing(s);
        let (list, _) = fx.marks();
        let n = hatch_segs(&of_view(&list, fx.sec)).len();
        if n > 0 && MAX_HATCH_LINES.is_multiple_of(n) {
            found = Some((s, n));
            break;
        }
        s -= 0.01;
    }
    let (s, n) = found.expect("kein passender Abstand");
    let copies = MAX_HATCH_LINES / n;
    let run = |count: usize| {
        let mut fx = tube_section(&t, Paper::A3, S11);
        fx.set_spacing(s);
        let one = fx.sec_lines.data.regions.clone();
        fx.sec_lines.data.regions.clear();
        for _ in 0..count {
            fx.sec_lines.data.regions.extend(one.iter().cloned());
        }
        let (list, errs) = fx.marks();
        (hatch_segs(&of_view(&list, fx.sec)).len(), errs)
    };
    let (have, errs) = run(copies);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(have, MAX_HATCH_LINES);
    let (have, errs) = run(copies + 1);
    assert_eq!(have, 0);
    assert_eq!(errs.len(), 1);
}

#[test]
fn missing_results() {
    let t = tube();
    let b = [db("Rohr", &t)];
    // Schnittansicht ohne Ergebnis: keine Markierungen, auch keine Schnittlinie.
    let fx = tube_section(&t, Paper::A3, S11);
    let results = BTreeMap::from([(fx.src, &fx.src_lines)]);
    let (list, errs) = section_marks(&fx.d, &results, &fx.sections());
    assert!(list.is_empty() && errs.is_empty(), "{list:?}");

    // Quelle nicht gezeichnet (Schnittansicht frei platziert): keine Schnittlinie, Titel bleibt.
    let mut fx = fixture(&b, Paper::A3, S11, ViewKind::Front, ViewKind::Top, 10.0);
    for v in &mut fx.d.views {
        if v.id == fx.sec {
            v.placement = Placement::Free { x: 150.0, y: 100.0 };
        }
    }
    let results = BTreeMap::from([(fx.sec, &fx.sec_lines.lines)]);
    let (list, _) = section_marks(&fx.d, &results, &fx.sections());
    assert!(of_view(&list, fx.src).is_empty());
    let own = of_view(&list, fx.sec);
    assert_eq!(texts(&own).len(), 1);
    assert!(!hatch_segs(&own).is_empty());

    // Verlorene Quelle (Ansicht gibt es nicht mehr): keine Schnittlinie.
    let mut fx = fixture(&b, Paper::A3, S11, ViewKind::Front, ViewKind::Top, 10.0);
    for v in &mut fx.d.views {
        if v.id == fx.sec {
            v.placement = Placement::Free { x: 150.0, y: 100.0 };
            v.parent = Some(ViewId(99));
        }
    }
    let (list, _) = section_marks(&fx.d, &fx.results(), &fx.sections());
    assert!(list.iter().all(|e| e.view == fx.sec));
    assert!(
        list.iter()
            .flat_map(|e| &e.marks)
            .all(|m| !matches!(m, Mark::Arrow { .. }))
    );

    // Ohne Daten der Schnittflächen keine Schraffur, aber Titel.
    let fx = tube_section(&t, Paper::A3, S11);
    let (list, _) = section_marks(&fx.d, &fx.results(), &BTreeMap::new());
    let own = of_view(&list, fx.sec);
    assert!(hatch_segs(&own).is_empty());
    assert_eq!(texts(&own).len(), 1);
}

#[test]
fn add_to_sheet() {
    let t = tube();
    let fx = tube_section(&t, Paper::A3, S11);
    let mut sheet = fx.sheet();
    let before: Vec<usize> = sheet.views.iter().map(|v| v.marks.len()).collect();
    let (list, _) = fx.marks();
    add_section_marks(&mut sheet, &list);
    for (i, v) in sheet.views.iter().enumerate() {
        let add = of_view(&list, v.id).len();
        assert!(add > 0);
        assert_eq!(v.marks.len(), before[i] + add);
    }
    // Unbekannte Ansichten werden übergangen.
    add_section_marks(
        &mut sheet,
        &[SectionMarks {
            view: ViewId(77),
            marks: vec![],
        }],
    );
}

#[test]
fn arrow_sizes() {
    let f = 297.0 / 420.0;
    let (l, w) = arrow_size(MarkKind::SectionLine(SectionPen::Chain), f);
    assert!(
        near(l, 3.535714, 1e-6) && near(w, 0.707143, 1e-6),
        "{l} {w}"
    );
    let (l, w) = arrow_size(MarkKind::SectionLine(SectionPen::End), 1.0);
    assert!(near(l, SECTION_ARROW_LEN, 1e-12) && near(w, SECTION_ARROW_HALF_W, 1e-12));
    assert_eq!(arrow_size(MarkKind::Dimension, f), (3.5, 0.6));
    assert_eq!(arrow_size(MarkKind::Dimension, 1.0), (3.5, 0.6));
    assert_eq!(arrow_size(MarkKind::Lost, 2.5), (3.5, 0.6));
}

#[test]
fn polygon_tolerance_is_measured_on_the_sheet() {
    // Maßstab 5:1: Der Rand weicht auf dem Blatt höchstens 0,005 mm von den Kreisen ab.
    let t = tube();
    let fx = tube_section(&t, Paper::A3, Scale { num: 5, den: 1 });
    let (list, _) = fx.marks();
    let xf = view_transform(&fx.d, fx.sec, &fx.results()).unwrap();
    let centre = xf.apply(DVec2::new(0.0, 0.0));
    let tol = 0.005 + 1e-9;
    let mut checked = 0;
    for (a, b) in hatch_segs(&of_view(&list, fx.sec)) {
        for p in [a, b] {
            let d = (p - centre).length();
            let r = if d > 40.0 { 50.0 } else { 30.0 };
            assert!(d >= r - tol && d <= r + 1e-9, "{d} bei Radius {r}");
            checked += 1;
        }
    }
    assert!(checked > 20);
}

#[test]
fn bad_spacing_gives_a_plain_message() {
    let t = tube();
    for bad in [0.0, -3.0, f64::NAN, f64::INFINITY] {
        let mut fx = tube_section(&t, Paper::A3, S11);
        fx.set_spacing(bad);
        let (list, errs) = fx.marks();
        assert_eq!(
            errs,
            vec!["Die Schraffur von „Schnitt A\u{2013}A“ lässt sich nicht berechnen.".to_string()],
            "{bad}"
        );
        let own = of_view(&list, fx.sec);
        assert!(own.iter().all(|m| kind_of(m) != MarkKind::Hatch));
        assert_eq!(texts(&own).len(), 1);
    }
}

/// Erwartete Schnittlinie, von Hand gerechnet (A3, Maßstab 1:1, Quelle frei bei (150, 200)).
struct Hand {
    src: ViewKind,
    sec: ViewKind,
    offset: f64,
    horizontal: bool,
    /// Blattkoordinate quer zur Linie (y bei waagerecht, x bei senkrecht).
    fixed: f64,
    /// Beide Enden entlang der Linie, einschließlich 5 mm Überstand.
    lo: f64,
    hi: f64,
    /// Blickrichtung auf dem Blatt.
    v: (f64, f64),
}

fn hand(
    src: ViewKind,
    sec: ViewKind,
    offset: f64,
    horizontal: bool,
    (fixed, lo, hi): (f64, f64, f64),
    v: (f64, f64),
) -> Hand {
    Hand {
        src,
        sec,
        offset,
        horizontal,
        fixed,
        lo,
        hi,
        v,
    }
}

/// Punkt: `along` entlang der Linie, `across` quer dazu.
fn at(h: &Hand, along: f64, across: f64) -> DVec2 {
    if h.horizontal {
        DVec2::new(along, h.fixed + across)
    } else {
        DVec2::new(h.fixed + across, along)
    }
}

fn norm_seg(a: DVec2, b: DVec2) -> [f64; 4] {
    if (a.x, a.y) <= (b.x, b.y) {
        [a.x, a.y, b.x, b.y]
    } else {
        [b.x, b.y, a.x, a.y]
    }
}

fn sorted_segs(v: Vec<(DVec2, DVec2)>) -> Vec<[f64; 4]> {
    let mut o: Vec<[f64; 4]> = v.into_iter().map(|(a, b)| norm_seg(a, b)).collect();
    o.sort_by(|a, b| a.partial_cmp(b).unwrap());
    o
}

fn same_segs(have: Vec<(DVec2, DVec2)>, want: Vec<(DVec2, DVec2)>, what: &str) {
    let (h, w) = (sorted_segs(have), sorted_segs(want));
    assert_eq!(h.len(), w.len(), "{what}: {h:?} statt {w:?}");
    for (a, b) in h.iter().zip(&w) {
        assert!(
            a.iter().zip(b).all(|(x, y)| near(*x, *y, 1e-9)),
            "{what}: {a:?} statt {b:?}"
        );
    }
}

fn same_points(mut have: Vec<DVec2>, mut want: Vec<DVec2>, what: &str) {
    let key = |a: &DVec2, b: &DVec2| (a.x, a.y).partial_cmp(&(b.x, b.y)).unwrap();
    have.sort_by(key);
    want.sort_by(key);
    assert_eq!(have.len(), want.len(), "{what}");
    for (a, b) in have.iter().zip(&want) {
        assert!(
            near(a.x, b.x, 1e-9) && near(a.y, b.y, 1e-9),
            "{what}: {a:?} statt {b:?}"
        );
    }
}

#[test]
fn section_line_exact_cases() {
    let b = boxs(30.0, 20.0, 10.0);
    // Quader x 0..30, y 0..20, z 0..10. Ansichtsmitten: Vorne (15, 5), Oben (15, 10),
    // Rechts (10, 5); Blattmitte (150, 200). Zahlen: (quer, von, bis) auf dem Blatt.
    let cases = [
        // Vorne, senkrecht bei x = 12: Blatt-x 147, y 195..205, mit Überstand 190..210.
        hand(
            ViewKind::Front,
            ViewKind::Left,
            12.0,
            false,
            (147.0, 190.0, 210.0),
            (1.0, 0.0),
        ),
        hand(
            ViewKind::Front,
            ViewKind::Right,
            12.0,
            false,
            (147.0, 190.0, 210.0),
            (-1.0, 0.0),
        ),
        // Oben, waagerecht bei y = 8: Blatt-y 198, x 135..165, mit Überstand 130..170.
        hand(
            ViewKind::Top,
            ViewKind::Front,
            8.0,
            true,
            (198.0, 130.0, 170.0),
            (0.0, 1.0),
        ),
        hand(
            ViewKind::Top,
            ViewKind::Back,
            8.0,
            true,
            (198.0, 130.0, 170.0),
            (0.0, -1.0),
        ),
        // Oben, senkrecht bei x = 9: Blatt-x 144, y 190..210, mit Überstand 185..215.
        hand(
            ViewKind::Top,
            ViewKind::Left,
            9.0,
            false,
            (144.0, 185.0, 215.0),
            (1.0, 0.0),
        ),
        hand(
            ViewKind::Top,
            ViewKind::Right,
            9.0,
            false,
            (144.0, 185.0, 215.0),
            (-1.0, 0.0),
        ),
        // Rechts (Ansicht y, z), waagerecht bei z = 3: Blatt-y 198, x 140..160, Überstand 135..165.
        hand(
            ViewKind::Right,
            ViewKind::Top,
            3.0,
            true,
            (198.0, 135.0, 165.0),
            (0.0, -1.0),
        ),
        hand(
            ViewKind::Right,
            ViewKind::Bottom,
            3.0,
            true,
            (198.0, 135.0, 165.0),
            (0.0, 1.0),
        ),
        // Rechts, senkrecht bei y = 6: Blatt-x 146, y 195..205, mit Überstand 190..210.
        hand(
            ViewKind::Right,
            ViewKind::Front,
            6.0,
            false,
            (146.0, 190.0, 210.0),
            (1.0, 0.0),
        ),
        hand(
            ViewKind::Right,
            ViewKind::Back,
            6.0,
            false,
            (146.0, 190.0, 210.0),
            (-1.0, 0.0),
        ),
    ];
    for h in &cases {
        let what = format!("{:?} -> {:?}", h.src, h.sec);
        let fx = fixture(&[db("Q", &b)], Paper::A3, S11, h.src, h.sec, h.offset);
        let (list, _) = fx.marks();
        let own = of_view(&list, fx.src);
        // Vorzeichen der Blickrichtung quer zur Linie.
        let s = if h.horizontal { h.v.1 } else { h.v.0 };
        same_segs(
            pen_lines(&own, SectionPen::End),
            vec![
                (at(h, h.lo, 0.0), at(h, h.lo + 8.0, 0.0)),
                (at(h, h.hi - 8.0, 0.0), at(h, h.hi, 0.0)),
            ],
            &what,
        );
        same_segs(
            pen_lines(&own, SectionPen::Chain),
            vec![
                (at(h, h.lo + 8.0, 0.0), at(h, h.hi - 8.0, 0.0)),
                (at(h, h.lo, 0.0), at(h, h.lo, 5.0 * s)),
                (at(h, h.hi, 0.0), at(h, h.hi, 5.0 * s)),
            ],
            &what,
        );
        let ar = arrows(&own);
        for (_, d) in &ar {
            assert_eq!(*d, DVec2::new(-h.v.0, -h.v.1), "{what}");
        }
        let want_tips = vec![at(h, h.lo, 10.0 * s), at(h, h.hi, 10.0 * s)];
        same_points(ar.iter().map(|a| a.0).collect(), want_tips.clone(), &what);
        // Buchstaben: Mitte 2,75 hinter der Spitze, Grundlinie 1,75 tiefer.
        let want_pos = want_tips
            .iter()
            .map(|t| DVec2::new(t.x + 2.75 * h.v.0, t.y + 2.75 * h.v.1 - 1.75))
            .collect();
        same_points(texts(&own).iter().map(|t| t.pos).collect(), want_pos, &what);
    }
}
