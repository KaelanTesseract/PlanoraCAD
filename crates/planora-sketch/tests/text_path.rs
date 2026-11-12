//! Text auf Pfad (Skizzen 8, Task 5, Spec B7 und B8): Leserichtung, Bezugspunkt, Ausrichtung,
//! oberhalb/unterhalb, An Pfad anpassen, über das Ende hinaus, Umkehren, starre Glyphen, der
//! Text folgt dem Pfad mit gleichen Ids, letzte gute Lage bei einem Pfad ohne Länge, der Pfad
//! lässt sich nicht löschen.
//!
//! Der Test-Setzer [`BoxShaper`] liefert für jedes Zeichen ein Quadrat mit Loch (11 Punkte), so
//! prüft die Skizze ohne `planora-text` und ohne Schriften.

use planora_i18n::tr;
use planora_kernel::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign, path_content};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

/// Vorschub je Zeichen des Test-Setzers (Einheiten der Höhe).
const ADV: f64 = 0.8;
const INK_RIGHT: f64 = 0.6;
const INK_TOP: f64 = 0.7;
const ASC: f64 = 0.9;
const DESC: f64 = -0.2;
/// Höhe der Texte in mm.
const H: f64 = 5.0;
/// Akzent ohne Vorschub (kombinierender Akut), steht über dem Zeichen davor.
const MARK: char = '\u{301}';

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Die Punkte einer Glyphe des Test-Setzers in der Reihenfolge der Skizze (je Kontur der
/// Anfang, dann Kontrollpunkte und Ende, ohne das letzte Ende).
const GLYPH: [(f64, f64); 11] = [
    (0.0, 0.0),
    (INK_RIGHT, 0.0),
    (INK_RIGHT, INK_TOP),
    (0.0, INK_TOP),
    (0.2, 0.2),
    (0.4, 0.2),
    (0.5, 0.35),
    (0.4, 0.5),
    (0.35, 0.55),
    (0.25, 0.55),
    (0.2, 0.5),
];

/// Die Punkte des Akzents (ein Dreieck).
const ACCENT: [(f64, f64); 3] = [(0.2, 0.8), (0.4, 0.8), (0.3, 0.95)];

fn box_glyph() -> Vec<Contour> {
    let p = |i: usize| v(GLYPH[i].0, GLYPH[i].1);
    vec![
        Contour {
            start: p(0),
            segs: vec![
                Seg::Line(p(1)),
                Seg::Line(p(2)),
                Seg::Line(p(3)),
                Seg::Line(p(0)),
            ],
        },
        Contour {
            start: p(4),
            segs: vec![
                Seg::Line(p(5)),
                Seg::Quad(p(6), p(7)),
                Seg::Cubic(p(8), p(9), p(10)),
                Seg::Line(p(4)),
            ],
        },
    ]
}

fn accent_glyph() -> Vec<Contour> {
    let p = |i: usize| v(ACCENT[i].0, ACCENT[i].1);
    vec![Contour {
        start: p(0),
        segs: vec![Seg::Line(p(1)), Seg::Line(p(2)), Seg::Line(p(0))],
    }]
}

/// Test-Setzer: je Zeichen ein Quadrat mit Loch, Leerzeichen ohne Umriss, `\n` bricht die Zeile,
/// [`MARK`] ist ein Akzent ohne Vorschub über dem Zeichen davor (wie ein Setzer ihn liefert).
struct BoxShaper;

impl TextShaper for BoxShaper {
    fn shape(&self, content: &str, style: &TextStyle) -> Result<TextShape, TextShapeError> {
        if content.trim().is_empty() {
            return Err(TextShapeError::Empty);
        }
        let mut lines = Vec::new();
        for text in content.split('\n') {
            let chars: Vec<char> = text.chars().collect();
            let mut glyphs = Vec::new();
            let mut pen = 0.0;
            for (i, ch) in chars.iter().enumerate() {
                if *ch == MARK {
                    glyphs.push(Glyph {
                        origin: v(pen - ADV, 0.0),
                        advance: 0.0,
                        contours: accent_glyph(),
                    });
                    continue;
                }
                let contours = if ch.is_whitespace() {
                    Vec::new()
                } else {
                    box_glyph()
                };
                glyphs.push(Glyph {
                    origin: v(pen, 0.0),
                    advance: ADV,
                    contours,
                });
                pen += ADV;
                if i + 1 < chars.len() {
                    pen += ADV * style.spacing / 100.0;
                }
            }
            lines.push(ShapedLine { glyphs, width: pen });
        }
        Ok(TextShape {
            lines,
            ascender: ASC,
            descender: DESC,
            line_height: 1.2,
            substituted: None,
            synthetic: false,
            missing_chars: false,
        })
    }
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn style() -> TextStyle {
    TextStyle {
        font: "Test".into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    }
}

fn params(content: &str) -> TextParams {
    TextParams {
        content: content.into(),
        style: style(),
        height: H,
        flip_h: false,
        flip_v: false,
    }
}

/// Gesetzt wie für einen Text auf Pfad: Zeilen mit Leerzeichen verbunden.
fn shape_of(p: &TextParams) -> Arc<TextShape> {
    Arc::new(
        BoxShaper
            .shape(&path_content(&p.content), &p.style)
            .unwrap(),
    )
}

fn add(s: &mut Sketch, path: CurveId, p: TextParams, above: bool, fit: bool, h: HAlign) -> TextId {
    let shape = shape_of(&p);
    s.add_text_path(path, p, above, fit, h, shape).unwrap()
}

fn on_path(s: &mut Sketch, path: CurveId, content: &str, h: HAlign) -> TextId {
    add(s, path, params(content), true, false, h)
}

fn line(s: &mut Sketch, a: DVec2, b: DVec2) -> CurveId {
    let a = s.add_point(a.x, a.y).unwrap();
    let b = s.add_point(b.x, b.y).unwrap();
    s.add_line(a, b).unwrap()
}

fn circle(s: &mut Sketch, r: f64) -> CurveId {
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(c, r).unwrap()
}

fn positions(s: &Sketch, t: TextId) -> Vec<DVec2> {
    s.text(t)
        .unwrap()
        .points
        .iter()
        .map(|&p| s.point_xy(p).unwrap())
        .collect()
}

/// Lage einer Glyphe des Test-Setzers in der Skizze, aus ihren Punkten.
#[derive(Debug)]
struct Placed {
    /// Ursprung (Anfang des Vorschubs auf der Grundlinie).
    origin: DVec2,
    /// Richtung der x- und y-Achse der Glyphe (Länge 1).
    ex: DVec2,
    ey: DVec2,
    /// Mitte des Vorschubs auf der Grundlinie.
    mid: DVec2,
    points: Vec<DVec2>,
}

/// Die `k`-te Glyphe mit Umriss des Textes `t` (11 Punkte je Glyphe).
fn glyph(s: &Sketch, t: TextId, k: usize) -> Placed {
    let all = positions(s, t);
    let points = all[11 * k..11 * k + 11].to_vec();
    let ex = (points[1] - points[0]) / (INK_RIGHT * H);
    let ey = (points[3] - points[0]) / (INK_TOP * H);
    Placed {
        origin: points[0],
        ex,
        ey,
        mid: points[0] + ex * (0.5 * ADV * H),
        points,
    }
}

fn glyph_count(s: &Sketch, t: TextId) -> usize {
    s.text(t).unwrap().points.len() / 11
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) <= 1e-9
}

fn assert_close(a: DVec2, b: DVec2, what: &str) {
    assert!(close(a, b), "{what}: {a:?} ≠ {b:?}");
}

fn says<T: std::fmt::Debug>(r: Result<T, SketchError>, msg: String, what: &str) {
    let e = r.expect_err(what).to_string();
    assert!(e.contains(&msg), "{what}: {e}");
}

/// Abstand von `p` zum Linienzug der Kurve `c` (dicht abgetastet).
fn distance_to_curve(s: &Sketch, c: CurveId, p: DVec2) -> f64 {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let n = 20_000;
    let pts: Vec<DVec2> = (0..=n)
        .map(|k| {
            s.curve_eval(c, lo + (hi - lo) * k as f64 / n as f64)
                .unwrap()
        })
        .collect();
    pts.windows(2)
        .map(|w| {
            let d = w[1] - w[0];
            let u = if d.length_squared() > 0.0 {
                ((p - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            p.distance(w[0] + d * u)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Jede Glyphe ist starr: alle Abstände wie im Shape × Höhe, die Achsen senkrecht und Länge 1.
fn assert_rigid(s: &Sketch, t: TextId, what: &str) {
    for k in 0..glyph_count(s, t) {
        let g = glyph(s, t, k);
        assert!((g.ex.length() - 1.0).abs() < 1e-9, "{what} {k}: {g:?}");
        assert!((g.ey.length() - 1.0).abs() < 1e-9, "{what} {k}: {g:?}");
        assert!(g.ex.dot(g.ey).abs() < 1e-9, "{what} {k}: {g:?}");
        for (i, &(xi, yi)) in GLYPH.iter().enumerate() {
            for (j, &(xj, yj)) in GLYPH.iter().enumerate().take(i) {
                let want = v(xi, yi).distance(v(xj, yj)) * H;
                let got = g.points[i].distance(g.points[j]);
                assert!(
                    (got - want).abs() < 1e-9,
                    "{what} {k}: {i}-{j} {got} ≠ {want}"
                );
            }
        }
    }
}

fn all_finite(s: &Sketch, t: TextId) -> bool {
    positions(s, t).iter().all(|p| p.is_finite())
}

/// Punkt auf dem Kreis mit Radius `r` um den Ursprung, `s` Bogenlänge ab oben im Uhrzeigersinn.
fn on_circle(r: f64, s: f64) -> DVec2 {
    let a = FRAC_PI_2 - s / r;
    v(a.cos(), a.sin()) * r
}

#[test]
fn on_circle_centered_is_upright_on_top() {
    let mut s = sketch();
    let c = circle(&mut s, 20.0);
    let t = on_path(&mut s, c, "a", HAlign::Center);
    let g = glyph(&s, t, 0);
    assert_close(g.mid, v(0.0, 20.0), "Mitte der Glyphe bei 90°");
    assert_close(g.ex, v(1.0, 0.0), "im Uhrzeigersinn gelesen");
    assert_close(g.ey, v(0.0, 1.0), "die y-Achse zeigt nach außen");
    assert_close(
        g.origin,
        v(-0.5 * ADV * H, 20.0),
        "Grundlinie auf dem Kreis",
    );
    // Zwei Zeichen sitzen mittig über dem Bezugspunkt, beide aufrecht und außen.
    let mut s = sketch();
    let c = circle(&mut s, 20.0);
    let t = on_path(&mut s, c, "ab", HAlign::Center);
    let (a, b) = (glyph(&s, t, 0), glyph(&s, t, 1));
    assert!(a.mid.x < 0.0 && b.mid.x > 0.0, "{a:?} {b:?}");
    assert!((a.mid.x + b.mid.x).abs() < 1e-9 && (a.mid.y - b.mid.y).abs() < 1e-9);
    for g in [&a, &b] {
        assert!((g.mid.length() - 20.0).abs() < 1e-9, "{g:?}");
        assert_close(g.ey, g.mid / 20.0, "nach außen");
    }
    // Ein Text länger als der Umfang läuft weiter um den Kreis.
    let mut s = sketch();
    let c = circle(&mut s, 20.0);
    let t = on_path(&mut s, c, &"a".repeat(40), HAlign::Left);
    assert_close(
        glyph(&s, t, 0).mid,
        on_circle(20.0, 0.5 * ADV * H),
        "links beginnt oben",
    );
    for k in 0..40 {
        let g = glyph(&s, t, k);
        assert!((g.mid.length() - 20.0).abs() < 1e-9, "{k}: {g:?}");
        assert_close(g.ey, g.mid / 20.0, "nach außen");
    }
    assert_rigid(&s, t, "Kreis");
}

#[test]
fn on_ellipse_centered_is_upright_on_top() {
    // Ruling 5b: Ellipse und Ellipsenbogen werden wie Kreis und Bogen im Uhrzeigersinn gelesen.
    // Der Bezugspunkt der Ellipse ist ihr höchster Punkt, auch bei gedrehter Ellipse.
    let c = v(5.0, -3.0);
    for major in [v(30.0, 0.0), v(24.0, 18.0), v(-18.0, 24.0), v(0.0, -30.0)] {
        let mut s = sketch();
        let m = s.add_point(c.x, c.y).unwrap();
        let e = s.add_ellipse(m, major, 15.0).unwrap();
        let t = on_path(&mut s, e, "a", HAlign::Center);
        let g = glyph(&s, t, 0);
        let b = major.perp() / major.length() * 15.0;
        let top_t = b.y.atan2(major.y);
        let top = c + major * top_t.cos() + b * top_t.sin();
        assert!(
            (top.y - c.y - major.y.hypot(b.y)).abs() < 1e-9,
            "höchster Punkt"
        );
        assert_close(g.mid, top, "Mitte der Glyphe oben");
        assert_close(g.ex, v(1.0, 0.0), "im Uhrzeigersinn gelesen");
        assert_close(g.ey, v(0.0, 1.0), "aufrecht und außen");
        assert_rigid(&s, t, "Ellipse");
        // Ein langer Text läuft über die Naht des Parameters hinweg um die ganze Ellipse.
        let long = on_path(&mut s, e, &"a".repeat(30), HAlign::Center);
        assert!(all_finite(&s, long));
        for k in 0..30 {
            let d = distance_to_curve(&s, e, glyph(&s, long, k).mid);
            assert!(d < 1e-4, "{major:?} {k}: {d}");
        }
    }
    // Ellipsenbogen über die obere Hälfte (gegen den Uhrzeigersinn von rechts nach links
    // angelegt): gelesen von links nach rechts, mittig oben aufrecht und außen.
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(30.0, 0.0).unwrap();
    let b = s.add_point(-30.0, 0.0).unwrap();
    let arc = s.add_elliptic_arc(m, v(30.0, 0.0), 15.0, a, b).unwrap();
    let t = on_path(&mut s, arc, "a", HAlign::Center);
    let g = glyph(&s, t, 0);
    assert!(g.mid.distance(v(0.0, 15.0)) < 1e-6, "{g:?}");
    assert!(g.ex.distance(v(1.0, 0.0)) < 1e-6, "{g:?}");
    assert!(g.ey.distance(v(0.0, 1.0)) < 1e-6, "{g:?}");
    let t = on_path(&mut s, arc, "ab", HAlign::Left);
    let first = glyph(&s, t, 0);
    assert!(first.mid.x < -20.0, "links beginnt links: {first:?}");
}

/// Schräge Linie der Länge 100 von (10, 10) nach (70, 90).
fn slanted() -> (Sketch, CurveId, DVec2, DVec2) {
    let mut s = sketch();
    let (a, b) = (v(10.0, 10.0), v(70.0, 90.0));
    let l = line(&mut s, a, b);
    (s, l, a, b)
}

#[test]
fn on_line_left_starts_at_start() {
    let (mut s, l, a, _) = slanted();
    let dir = v(0.6, 0.8);
    let t = on_path(&mut s, l, "ab", HAlign::Left);
    let (g0, g1) = (glyph(&s, t, 0), glyph(&s, t, 1));
    assert_close(g0.origin, a, "erste Glyphe am Anfang");
    assert_close(g1.origin, a + dir * (ADV * H), "zweite dahinter");
    assert_close(g0.ex, dir, "Leserichtung vom Anfang zum Ende");
    assert_close(g0.ey, dir.perp(), "oberhalb heißt links der Leserichtung");
    assert_rigid(&s, t, "Linie");
}

#[test]
fn right_ends_at_end() {
    let (mut s, l, _, b) = slanted();
    let dir = v(0.6, 0.8);
    let t = on_path(&mut s, l, "ab", HAlign::Right);
    let last = glyph(&s, t, 1);
    assert_close(
        last.origin + dir * (ADV * H),
        b,
        "der Vorschub endet am Ende",
    );
    // Mit Zeichenabstand: Die Zeilenbreite zählt, der letzte Vorschub endet weiter am Ende.
    let (mut s, l, _, b) = slanted();
    let mut p = params("abc");
    p.style.spacing = 50.0;
    let t = add(&mut s, l, p, true, false, HAlign::Right);
    let (g1, g2) = (glyph(&s, t, 1), glyph(&s, t, 2));
    assert_close(g2.origin + dir * (ADV * H), b, "mit Abstand");
    assert_close(g2.origin - g1.origin, dir * (1.5 * ADV * H), "Abstand 50 %");
}

#[test]
fn centered_at_mid() {
    let (mut s, l, a, b) = slanted();
    let dir = v(0.6, 0.8);
    let t = on_path(&mut s, l, "abc", HAlign::Center);
    let (first, last) = (glyph(&s, t, 0), glyph(&s, t, 2));
    let middle = (first.origin + last.origin + dir * (ADV * H)) / 2.0;
    assert_close(middle, (a + b) / 2.0, "mittig auf der Länge");
    assert_close(glyph(&s, t, 1).mid, (a + b) / 2.0, "die mittlere Glyphe");
}

#[test]
fn below_puts_ascender_on_curve() {
    let mut s = sketch();
    let l = line(&mut s, v(0.0, 0.0), v(100.0, 0.0));
    let t = add(&mut s, l, params("ab"), false, false, HAlign::Left);
    for k in 0..2 {
        let g = glyph(&s, t, k);
        assert_close(g.ey, v(0.0, 1.0), "weiter lesbar");
        assert!((g.origin.y + ASC * H).abs() < 1e-9, "{g:?}");
        let ascender = g.origin + g.ey * (ASC * H);
        assert!(
            ascender.y.abs() < 1e-9,
            "Oberlänge auf der Kurve: {ascender:?}"
        );
        assert!(g.points.iter().all(|p| p.y < 0.0), "unterhalb: {g:?}");
    }
    assert_close(glyph(&s, t, 0).origin, v(0.0, -ASC * H), "links am Anfang");
    // Auf dem Kreis unterhalb: innen, oben aufrecht.
    let mut s = sketch();
    let c = circle(&mut s, 20.0);
    let t = add(&mut s, c, params("a"), false, false, HAlign::Center);
    let g = glyph(&s, t, 0);
    assert_close(g.ey, v(0.0, 1.0), "aufrecht");
    assert_close(
        g.mid + g.ey * (ASC * H),
        v(0.0, 20.0),
        "Oberlänge oben am Kreis",
    );
    assert!(g.points.iter().all(|p| p.length() < 20.0), "innen: {g:?}");
}

#[test]
fn fit_on_open_path_spans_ends() {
    let mut s = sketch();
    let l = line(&mut s, v(0.0, 0.0), v(100.0, 0.0));
    let mut p = params("abcd");
    // Zeichenabstand und Ausrichtung sind beim Anpassen ohne Wirkung.
    p.style.spacing = 50.0;
    let t = add(&mut s, l, p, true, true, HAlign::Right);
    let n = glyph_count(&s, t);
    assert_eq!(n, 4);
    let origins: Vec<DVec2> = (0..n).map(|k| glyph(&s, t, k).origin).collect();
    assert_close(origins[0], v(0.0, 0.0), "erste Glyphe am Anfang");
    assert_close(
        origins[3] + v(ADV * H, 0.0),
        v(100.0, 0.0),
        "letzte endet am Ende",
    );
    let gaps: Vec<f64> = origins
        .windows(2)
        .map(|w| w[1].x - w[0].x - ADV * H)
        .collect();
    for g in &gaps {
        assert!((g - gaps[0]).abs() < 1e-9, "gleiche Lücken: {gaps:?}");
    }
    assert!((gaps[0] - (100.0 - 4.0 * ADV * H) / 3.0).abs() < 1e-9);
    // Auf dem Kreis ist auch die Lücke zwischen letzter und erster Glyphe gleich: Die Mitten
    // stehen im gleichen Winkelabstand, die erste beginnt oben.
    let mut s = sketch();
    let c = circle(&mut s, 20.0);
    let t = add(&mut s, c, params("abcd"), true, true, HAlign::Center);
    assert_close(
        glyph(&s, t, 0).mid,
        on_circle(20.0, 0.5 * ADV * H),
        "beginnt oben",
    );
    let angle = |k: usize| {
        let m = glyph(&s, t, k).mid;
        m.y.atan2(m.x)
    };
    for k in 0..4 {
        let step = (angle(k) - angle((k + 1) % 4)).rem_euclid(2.0 * PI);
        assert!((step - FRAC_PI_2).abs() < 1e-9, "{k}: {step}");
    }
}

#[test]
fn fit_too_long_overlaps() {
    let mut s = sketch();
    let l = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let t = add(&mut s, l, params("abcde"), true, true, HAlign::Left);
    let origins: Vec<DVec2> = (0..5).map(|k| glyph(&s, t, k).origin).collect();
    assert_close(origins[0], v(0.0, 0.0), "Anfang");
    assert_close(origins[4] + v(ADV * H, 0.0), v(10.0, 0.0), "Ende");
    let gap = (10.0 - 5.0 * ADV * H) / 4.0;
    assert!(gap < 0.0);
    for w in origins.windows(2) {
        assert!(
            (w[1].x - w[0].x - ADV * H - gap).abs() < 1e-9,
            "negative Lücke"
        );
    }
    assert_rigid(&s, t, "überlappend");
}

#[test]
fn beyond_end_follows_tangent() {
    // Viertelbogen um den Ursprung von (20, 0) nach (0, 20); gelesen im Uhrzeigersinn, also von
    // oben nach rechts. Länge 10π ≈ 31,4, der Text ist 40 lang.
    let arc = |s: &mut Sketch| {
        let c = s.add_point(0.0, 0.0).unwrap();
        let a = s.add_point(20.0, 0.0).unwrap();
        let b = s.add_point(0.0, 20.0).unwrap();
        s.add_arc(c, a, b).unwrap()
    };
    let len = 10.0 * PI;
    let mut s = sketch();
    let c = arc(&mut s);
    let t = on_path(&mut s, c, &"a".repeat(10), HAlign::Left);
    let first = glyph(&s, t, 0);
    let m = on_circle(20.0, 0.5 * ADV * H);
    assert_close(first.mid, m, "links am Anfang der Leserichtung");
    assert_close(first.ex, v(m.y, -m.x) / 20.0, "im Uhrzeigersinn");
    assert_close(first.ey, m / 20.0, "außen");
    // Hinter dem Ende auf der Tangente nach unten weiter, die Grundlinie bei x = 20.
    let last = glyph(&s, t, 9);
    let beyond = 9.5 * ADV * H - len;
    assert!(beyond > 0.0);
    assert_close(last.mid, v(20.0, -beyond), "auf der Tangente am Ende");
    assert_close(last.ex, v(0.0, -1.0), "Richtung der Tangente");
    assert_close(last.ey, v(1.0, 0.0), "weiter außen");
    // Rechts ausgerichtet: vor dem Anfang auf der Tangente am Anfang.
    let mut s = sketch();
    let c = arc(&mut s);
    let t = on_path(&mut s, c, &"a".repeat(10), HAlign::Right);
    let first = glyph(&s, t, 0);
    let before = len - 10.0 * ADV * H + 0.5 * ADV * H;
    assert!(before < 0.0);
    assert_close(first.mid, v(before, 20.0), "auf der Tangente am Anfang");
    assert_close(first.ex, v(1.0, 0.0), "Richtung der Tangente");
    let last = glyph(&s, t, 9);
    let a = (0.5 * ADV * H) / 20.0;
    assert_close(last.mid, v(a.cos(), a.sin()) * 20.0, "endet am Ende");
    assert_rigid(&s, t, "Bogen");
}

const LINKED: ProjectStyle = ProjectStyle {
    construction: false,
};

fn nurbs(degree: usize, knots: &[f64], control: &[(f64, f64)], weights: &[f64]) -> NurbsCurve {
    NurbsCurve::new(
        degree,
        knots.to_vec(),
        control
            .iter()
            .map(|&(x, y)| DVec3::new(x, y, 0.0))
            .collect(),
        weights.to_vec(),
    )
    .unwrap()
}

/// Vollkreis als rationale NURBS um den Ursprung, Naht bei `(r, 0)`.
fn nurbs_circle(r: f64) -> NurbsCurve {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    nurbs(
        2,
        &[
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ],
        &[
            (r, 0.0),
            (r, r),
            (0.0, r),
            (-r, r),
            (-r, 0.0),
            (-r, -r),
            (0.0, -r),
            (r, -r),
            (r, 0.0),
        ],
        &[1.0, w, 1.0, w, 1.0, w, 1.0, w, 1.0],
    )
}

fn projected(s: &mut Sketch, c: NurbsCurve) -> CurveId {
    let (lo, hi) = c.domain();
    s.add_projected_curve(&Curve::Nurbs(Arc::new(c)), lo, hi, LINKED)
        .unwrap()
}

/// Je ein Pfad jeder Art außer Linie, Kreis und Bogen, mit Namen.
fn other_paths(s: &mut Sketch) -> Vec<(CurveId, &'static str)> {
    let pts = |s: &mut Sketch, xy: &[(f64, f64)]| -> Vec<_> {
        xy.iter()
            .map(|&(x, y)| s.add_point(x, y).unwrap())
            .collect()
    };
    let fit = pts(s, &[(0.0, 0.0), (20.0, 10.0), (40.0, -5.0), (60.0, 5.0)]);
    let spline = s.add_spline(&fit).unwrap();
    let ctrl = pts(
        s,
        &[
            (0.0, 30.0),
            (15.0, 45.0),
            (30.0, 25.0),
            (45.0, 40.0),
            (60.0, 30.0),
        ],
    );
    let control = s.add_control_spline(&ctrl, 3).unwrap();
    let e = s.add_point(0.0, -40.0).unwrap();
    let ellipse = s.add_ellipse(e, v(30.0, 0.0), 15.0).unwrap();
    // Obere Hälfte der Ellipse um (100, 0), gegen den Uhrzeigersinn von (130, 0) nach (70, 0).
    let ec = pts(s, &[(100.0, 0.0), (130.0, 0.0), (70.0, 0.0)]);
    let earc = s
        .add_elliptic_arc(ec[0], v(30.0, 0.0), 15.0, ec[1], ec[2])
        .unwrap();
    let cn = pts(s, &[(0.0, 80.0), (40.0, 80.0), (20.0, 100.0)]);
    let conic = s.add_conic(cn[0], cn[1], cn[2], 0.5).unwrap();
    let wavy = projected(
        s,
        nurbs(
            3,
            &[0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0],
            &[
                (0.0, 120.0),
                (20.0, 170.0),
                (50.0, 190.0),
                (90.0, 180.0),
                (120.0, 150.0),
                (140.0, 120.0),
            ],
            &[1.0; 6],
        ),
    );
    let closed = projected(s, nurbs_circle(25.0));
    vec![
        (spline, "Spline"),
        (control, "Kontrollpunkt-Spline"),
        (ellipse, "Ellipse"),
        (earc, "Ellipsenbogen"),
        (conic, "Kegelschnitt"),
        (wavy, "fester Spline"),
        (closed, "geschlossener fester Spline"),
    ]
}

#[test]
fn on_spline_ellipse_conic_fixedspline_works() {
    for fit in [false, true] {
        let mut s = sketch();
        for (c, what) in other_paths(&mut s) {
            for h in [HAlign::Left, HAlign::Center, HAlign::Right] {
                for above in [true, false] {
                    let t = add(&mut s, c, params("abc"), above, fit, h);
                    let what = format!("{what}, anpassen {fit}, {h:?}, oberhalb {above}");
                    assert!(all_finite(&s, t), "{what}: keine NaN");
                    assert_eq!(glyph_count(&s, t), 3, "{what}");
                    assert_rigid(&s, t, &what);
                    if above {
                        // Die Mitte der Grundlinie jeder Glyphe liegt auf dem Pfad (der Text ist
                        // kürzer als jeder Pfad, nichts ragt hinaus).
                        for k in 0..3 {
                            let m = glyph(&s, t, k).mid;
                            let d = distance_to_curve(&s, c, m);
                            assert!(d < 1e-4, "{what} {k}: {d}");
                        }
                    }
                    s.remove_text(t).unwrap();
                }
            }
        }
    }
}

#[test]
fn glyphs_stay_rigid() {
    let mut s = sketch();
    let c = circle(&mut s, 12.0);
    let e = s.add_point(0.0, 50.0).unwrap();
    let ellipse = s.add_ellipse(e, v(25.0, 5.0), 9.0).unwrap();
    for path in [c, ellipse] {
        for (flip_h, flip_v) in [(false, false), (true, false), (false, true), (true, true)] {
            for fit in [false, true] {
                let mut p = params("abcdefgh");
                p.flip_h = flip_h;
                p.flip_v = flip_v;
                let t = add(&mut s, path, p, true, fit, HAlign::Center);
                assert_rigid(&s, t, &format!("{path:?} {flip_h} {flip_v} {fit}"));
                s.remove_text(t).unwrap();
            }
        }
    }
}

#[test]
fn path_change_moves_text_keeps_ids() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(100.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let t = on_path(&mut s, l, "ab", HAlign::Center);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let d = s.text(t).unwrap().clone();
    let before = positions(&s, t);
    s.drag(&[Drag {
        point: b,
        target: v(80.0, 40.0),
    }])
    .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let after = s.text(t).unwrap();
    assert_eq!(after.curves, d.curves, "gleiche Kurven");
    assert_eq!(after.points, d.points, "gleiche Punkte");
    assert_ne!(positions(&s, t), before, "der Text ist gefolgt");
    let (pa, pb) = (s.point_xy(a).unwrap(), s.point_xy(b).unwrap());
    let dir = (pb - pa).normalize();
    let (g0, g1) = (glyph(&s, t, 0), glyph(&s, t, 1));
    assert_close(g0.ex, dir, "Richtung des neuen Pfades");
    assert_close(
        (g0.origin + g1.origin + dir * (ADV * H)) / 2.0,
        (pa + pb) / 2.0,
        "Mitte",
    );
    // Abhängigkeiten am Pfad gehen wie gewohnt; der Text folgt dem gelösten Pfad.
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let ex = glyph(&s, t, 0).ex;
    assert!(
        ex.y.abs() < 1e-9 && (ex.x.abs() - 1.0).abs() < 1e-9,
        "waagerecht: {ex:?}"
    );
    assert_eq!(s.text(t).unwrap().curves, d.curves);
}

#[test]
fn zero_length_path_keeps_last_good() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(50.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let t = on_path(&mut s, l, "ab", HAlign::Center);
    s.solve();
    let (curves, points) = {
        let d = s.text(t).unwrap();
        (d.curves.clone(), d.points.clone())
    };
    let good = positions(&s, t);
    // Die Linie auf Länge 0 ziehen (hier ohne Löser, wie ein Zwischenstand beim Ziehen).
    s.set_point_xy(b, 0.0, 0.0).unwrap();
    s.solve();
    assert!(all_finite(&s, t), "keine NaN");
    assert_eq!(s.text(t).unwrap().points, points, "die Punkte bleiben");
    assert_eq!(s.text(t).unwrap().curves, curves);
    assert_eq!(positions(&s, t), good, "letzte gute Lage");
    // Fast null: ebenso.
    s.set_point_xy(b, 1e-12, 0.0).unwrap();
    s.solve();
    assert!(all_finite(&s, t));
    assert_eq!(positions(&s, t), good);
    // Wieder lang: Der Text folgt mit denselben Ids.
    s.set_point_xy(b, 0.0, 80.0).unwrap();
    s.solve();
    assert_eq!(s.text(t).unwrap().points, points);
    let (g0, g1) = (glyph(&s, t, 0), glyph(&s, t, 1));
    assert_close(
        (g0.origin + g1.origin + v(0.0, ADV * H)) / 2.0,
        v(0.0, 40.0),
        "Mitte",
    );
}

#[test]
fn deleting_path_rejected() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(50.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let t = on_path(&mut s, l, "ab", HAlign::Left);
    let rev = s.revision();
    let data = s.to_data();
    says(s.remove_curve(l), tr!("sketch-text-on-path"), "Pfad");
    says(
        s.remove_point(a),
        tr!("sketch-text-on-path"),
        "Punkt des Pfades",
    );
    for &c in &s.text(t).unwrap().curves.clone() {
        says(s.remove_curve(c), tr!("sketch-text-part"), "Textkurve");
    }
    assert_eq!(s.revision(), rev);
    assert_eq!(s.to_data(), data);
    // Den Text zu löschen lässt den Pfad stehen; danach geht der Pfad.
    s.remove_text(t).unwrap();
    assert!(s.curve(l).is_some(), "ein Pfad bleibt");
    s.remove_curve(l).unwrap();
}

#[test]
fn from_data_refuses_a_frame_line_as_path() {
    // Ruling 5c: Eine Rahmenlinie ist auch aus einer Datei kein Pfad. Sonst ginge sie mit ihrem
    // Text verloren, und der Text auf ihr hinge an einer gelöschten Kurve.
    let mut s = sketch();
    let p = params("ab");
    let shape = Arc::new(BoxShaper.shape("ab", &p.style).unwrap());
    let boxed = s
        .add_text_box(
            v(0.0, 0.0),
            v(40.0, 10.0),
            p,
            HAlign::Left,
            VAlign::Bottom,
            shape,
        )
        .unwrap();
    let free = line(&mut s, v(0.0, -20.0), v(50.0, -20.0));
    on_path(&mut s, free, "cd", HAlign::Left);
    let TextPlacement::Box { lines, .. } = s.text(boxed).unwrap().placement else {
        panic!("Rahmen");
    };
    let good = s.to_data();
    assert!(
        Sketch::from_data(good.clone()).is_ok(),
        "freie Linie als Pfad"
    );
    let reason = tr!(
        "sketch-err-text-data",
        reason = planora_i18n::t("sketch-err-text-data-path")
    );
    for l in lines {
        let mut d = good.clone();
        if let TextPlacement::Path { path, .. } = &mut d.texts[1].placement {
            *path = l;
        }
        says(Sketch::from_data(d), reason.clone(), "Rahmenlinie als Pfad");
    }
    // Darum geht der Rahmen beim Löschen des Textes immer mit, ohne Rückfrage.
    let mut s = Sketch::from_data(good).unwrap();
    s.remove_text(boxed).unwrap();
    assert!(lines.iter().all(|&l| s.curve(l).is_none()));
}

#[test]
fn flip_h_reverses_reading() {
    let make = |flip_h: bool, h: HAlign| {
        let mut s = sketch();
        let l = line(&mut s, v(0.0, 0.0), v(100.0, 0.0));
        let mut p = params("ab");
        p.flip_h = flip_h;
        let t = add(&mut s, l, p, true, false, h);
        (s, t)
    };
    // Ruling 5a/5d: Die Leserichtung kehrt sich um, die Buchstaben bleiben lesbar. Auf der
    // Geraden ist das eine Drehung um 180° um die Mitte des Pfades, für jede Ausrichtung;
    // „oberhalb“ bleibt links der (neuen) Leserichtung, also unter der Linie.
    for h in [HAlign::Left, HAlign::Center, HAlign::Right] {
        let (s0, t0) = make(false, h);
        let (s1, t1) = make(true, h);
        for (p, q) in positions(&s0, t0).iter().zip(positions(&s1, t1)) {
            assert_close(q, v(100.0 - p.x, -p.y), "gedreht um die Mitte des Pfades");
        }
        for k in 0..2 {
            let (a, b) = (glyph(&s0, t0, k), glyph(&s1, t1, k));
            assert!(
                (a.ex.perp_dot(a.ey) - b.ex.perp_dot(b.ey)).abs() < 1e-9
                    && b.ex.perp_dot(b.ey) > 0.0,
                "lesbar, nicht spiegelverkehrt: {a:?} {b:?}"
            );
            assert!(
                outer_area(&a) * outer_area(&b) > 0.0,
                "gleicher Umlaufsinn der Kontur"
            );
        }
    }
    // Links ausgerichtet beginnt die Leserichtung nun am Ende, die Reihenfolge kehrt sich um.
    let (s, t) = make(true, HAlign::Left);
    let (a, b) = (glyph(&s, t, 0), glyph(&s, t, 1));
    assert_close(a.mid, v(100.0 - 0.5 * ADV * H, 0.0), "erste Glyphe am Ende");
    assert!(b.mid.x < a.mid.x, "umgekehrte Folge: {a:?} {b:?}");
    assert_close(a.ex, v(-1.0, 0.0), "Leserichtung umgekehrt");
    assert_close(a.ey, v(0.0, -1.0), "links der neuen Leserichtung");
    assert!(positions(&s, t).iter().all(|p| p.y <= 1e-9), "andere Seite");
    // Auf dem Kreis: gegen den Uhrzeigersinn ab oben, oberhalb heißt dann innen; unten steht der
    // Text aufrecht und läuft von links nach rechts.
    let mut s = sketch();
    let c = circle(&mut s, 20.0);
    let mut p = params(&"a".repeat(40));
    p.flip_h = true;
    let t = add(&mut s, c, p, true, false, HAlign::Left);
    for k in 0..40 {
        let g = glyph(&s, t, k);
        let s_k = -(k as f64 + 0.5) * ADV * H;
        assert_close(g.mid, on_circle(20.0, s_k), "gegen den Uhrzeigersinn");
        assert_close(g.ey, -g.mid / 20.0, "nach innen");
        assert!(g.ex.perp_dot(g.ey) > 0.0, "lesbar");
    }
    let bottom = glyph(&s, t, 15);
    assert!(bottom.mid.y < -19.9, "{bottom:?}");
    assert!(
        bottom.ex.x > 0.99 && bottom.ey.y > 0.99,
        "unten lesbar: {bottom:?}"
    );
    assert_rigid(&s, t, "Kreis umgekehrt");
}

/// Vorzeichenbehaftete Fläche der Außenkontur einer Glyphe des Test-Setzers.
fn outer_area(g: &Placed) -> f64 {
    let q = &g.points[..4];
    (0..4).map(|i| q[i].perp_dot(q[(i + 1) % 4])).sum::<f64>() / 2.0
}

#[test]
fn flip_v_swaps_side() {
    for above in [true, false] {
        let make = |flip_v: bool| {
            let mut s = sketch();
            let l = line(&mut s, v(0.0, 0.0), v(100.0, 0.0));
            let mut p = params("ab");
            p.flip_v = flip_v;
            let t = add(&mut s, l, p, above, false, HAlign::Center);
            (s, t)
        };
        let (s0, t0) = make(false);
        let (s1, t1) = make(true);
        for (p, q) in positions(&s0, t0).iter().zip(positions(&s1, t1)) {
            assert_close(q, v(p.x, -p.y), "gespiegelt am Pfad");
        }
        let side = |s: &Sketch, t| {
            positions(s, t)
                .iter()
                .filter(|p| p.y.abs() > 1e-9)
                .map(|p| p.y.signum())
                .sum::<f64>()
        };
        assert_eq!(side(&s0, t0), -side(&s1, t1), "andere Seite");
    }
    // Auf dem Kreis: oberhalb und umgekehrt steht der Text innen, die Grundlinie bleibt am Kreis.
    let mut s = sketch();
    let c = circle(&mut s, 20.0);
    let mut p = params("a");
    p.flip_v = true;
    let t = add(&mut s, c, p, true, false, HAlign::Center);
    let g = glyph(&s, t, 0);
    assert_close(g.origin, v(-0.5 * ADV * H, 20.0), "Grundlinie am Kreis");
    assert_close(g.ey, v(0.0, -1.0), "nach innen");
    assert!(
        g.points.iter().all(|p| p.y <= 20.0 + 1e-9),
        "unter der Tangente oben"
    );
}

#[test]
fn fit_keeps_marks_on_their_base() {
    // Ein Akzent ohne Vorschub bleibt beim Anpassen und auf gekrümmtem Pfad über seinem
    // Buchstaben, starr mit ihm.
    let content = format!("a{MARK}b");
    for fit in [false, true] {
        let mut s = sketch();
        let c = circle(&mut s, 15.0);
        let t = add(&mut s, c, params(&content), true, fit, HAlign::Center);
        let all = positions(&s, t);
        assert_eq!(all.len(), 11 + 3 + 11, "a, Akzent, b in dieser Folge");
        let ex = (all[1] - all[0]) / (INK_RIGHT * H);
        let ey = (all[3] - all[0]) / (INK_TOP * H);
        for (i, &(x, y)) in ACCENT.iter().enumerate() {
            let want = all[0] + ex * (x * H) + ey * (y * H);
            assert_close(all[11 + i], want, "Akzent über dem a");
        }
    }
}

#[test]
fn add_text_path_rejects_bad_paths_and_leaves_nothing() {
    let mut s = sketch();
    let p = params("ab");
    let shape = Arc::new(BoxShaper.shape("ab", &p.style).unwrap());
    let boxed = s
        .add_text_box(
            v(0.0, 0.0),
            v(40.0, 10.0),
            p,
            HAlign::Left,
            VAlign::Bottom,
            shape,
        )
        .unwrap();
    let glyph_curve = s.text(boxed).unwrap().curves[0];
    let TextPlacement::Box { lines, .. } = s.text(boxed).unwrap().placement else {
        panic!("Rahmen");
    };
    let sp: Vec<_> = [(0.0, 50.0), (20.0, 60.0), (40.0, 50.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let spline = s.add_spline(&sp).unwrap();
    let handle = s.activate_tangent_handle(spline, sp[1]).unwrap();
    let a = s.add_point(70.0, 0.0).unwrap();
    let b = s.add_point(80.0, 0.0).unwrap();
    let zero = s.add_line(a, b).unwrap();
    s.set_point_xy(b, 70.0, 0.0).unwrap();
    let ok = line(&mut s, v(0.0, -20.0), v(50.0, -20.0));
    let dead = line(&mut s, v(0.0, -30.0), v(50.0, -30.0));
    s.remove_curve(dead).unwrap();
    let rev = s.revision();
    let data = s.to_data();
    let try_add = |s: &mut Sketch, path: CurveId, p: TextParams| {
        let shape = shape_of(&p);
        s.add_text_path(path, p, true, false, HAlign::Left, shape)
    };
    let bad = tr!("sketch-text-bad-path");
    says(
        try_add(&mut s, glyph_curve, params("x")),
        bad.clone(),
        "Textkurve",
    );
    says(
        try_add(&mut s, lines[0], params("x")),
        bad.clone(),
        "Rahmenlinie",
    );
    says(try_add(&mut s, handle, params("x")), bad, "Griffkurve");
    assert_eq!(
        try_add(&mut s, dead, params("x")),
        Err(SketchError::UnknownObject)
    );
    says(
        try_add(&mut s, zero, params("x")),
        tr!("sketch-text-path-degenerate"),
        "Pfad ohne Länge",
    );
    let mut p = params("x");
    p.height = 0.0;
    says(try_add(&mut s, ok, p), tr!("sketch-text-height"), "Höhe 0");
    let mut p = params("x");
    p.style.spacing = 1001.0;
    says(
        try_add(&mut s, ok, p),
        tr!("sketch-text-spacing", max = "1000".to_string()),
        "Abstand",
    );
    let empty = Arc::new(TextShape {
        lines: vec![ShapedLine {
            glyphs: Vec::new(),
            width: 0.0,
        }],
        ascender: ASC,
        descender: DESC,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: true,
    });
    says(
        s.add_text_path(ok, params("x"), true, false, HAlign::Left, empty),
        tr!("sketch-text-empty"),
        "ohne Kurven",
    );
    assert_eq!(s.revision(), rev, "nichts angelegt");
    assert_eq!(s.to_data(), data);
    // Konstruktion und eine Linie geht; ebenso ein zweiter Text auf demselben Pfad.
    s.set_construction(ok, true).unwrap();
    let t1 = on_path(&mut s, ok, "ab", HAlign::Left);
    let t2 = on_path(&mut s, ok, "cd", HAlign::Right);
    assert!(s.text(t1).is_some() && s.text(t2).is_some());
    assert!(
        s.text(t1)
            .unwrap()
            .curves
            .iter()
            .all(|&c| !s.curve(c).unwrap().construction),
        "die Buchstaben bilden Profile"
    );
    assert!(matches!(
        s.text(t1).unwrap().placement,
        TextPlacement::Path { path, above: true, fit: false, h: HAlign::Left } if path == ok
    ));
    says(
        s.edit_text_box(
            t1,
            params("ab"),
            HAlign::Left,
            VAlign::Bottom,
            shape_of(&params("ab")),
        ),
        tr!("sketch-text-not-box"),
        "kein Rahmen",
    );
}

#[test]
fn several_lines_join_with_a_space() {
    assert_eq!(path_content("a\r\nb\rc\nd"), "a b c d");
    assert_eq!(path_content("ab"), "ab");
    let make = |content: &str| {
        let mut s = sketch();
        let l = line(&mut s, v(0.0, 0.0), v(100.0, 0.0));
        let t = on_path(&mut s, l, content, HAlign::Center);
        (s, t)
    };
    let (mut s, t) = make("ab\ncd");
    let (s2, t2) = make("ab cd");
    assert_eq!(positions(&s, t), positions(&s2, t2));
    assert_eq!(
        s.text(t).unwrap().content,
        "ab\ncd",
        "der Inhalt bleibt, wie er ist"
    );
    // Neusetzen setzt ebenso verbunden: dieselbe Form, nichts ändert sich.
    let rev = s.revision();
    let report = s.reshape_texts(&BoxShaper);
    assert!(!report.changed, "{report:?}");
    assert_eq!(s.revision(), rev);
}

#[test]
fn path_text_roundtrip() {
    let mut s = sketch();
    let c = circle(&mut s, 20.0);
    let mut p = params("ab\ncd");
    p.flip_h = true;
    let t = add(&mut s, c, p, false, true, HAlign::Right);
    s.solve();
    let d = s.to_data();
    let mut loaded = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(loaded.to_data(), d);
    assert!(!loaded.is_text_shaped(t));
    assert_eq!(loaded.text(t).unwrap(), s.text(t).unwrap());
    let rev = loaded.revision();
    let report = loaded.reshape_texts(&BoxShaper);
    assert!(!report.changed, "{report:?}");
    assert!(loaded.is_text_shaped(t));
    loaded.solve();
    assert_eq!(loaded.revision(), rev, "Lösen danach ändert nichts");
    assert_eq!(positions(&loaded, t), positions(&s, t));
}
