//! Stufe 10: SVG-Schreiber. Gelesen wird mit dem eigenen Leser (`common/svg_read.rs`), die
//! Sollwerte kommen aus Formeln und von Hand gerechneten Zahlen, nicht aus der Bibliothek.

mod common;

use common::model::*;
use common::svg_read::{self, P, ReadPath, ReadSvg, Seg, dist};
use planora_drawing::model::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::{SheetLine, SheetModel, SheetView, build_sheet};
use planora_drawing::stroke::{Stroke, StrokeOrigin};
use planora_drawing::{
    Basis, DrawBody, DrawError, Piece, ViewId, ViewKind, ViewLines, Visibility, compute_view,
    svg_string, write_svg,
};
use planora_exchange::Progress;
use planora_kernel::{Curve, Solid};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU};
use std::collections::BTreeMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------------------------
// Eigene Hilfen
// ---------------------------------------------------------------------------------------------

fn v3(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn cross(a: DVec3, b: DVec3) -> DVec3 {
    v3(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Blattachsen `(ex, ey)` der Ansicht, von Hand: Blick `d`, „oben“ `ey`, `ex = d × ey`.
fn axes(kind: ViewKind) -> (DVec3, DVec3) {
    let (d, ey) = match kind {
        ViewKind::Front => (v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Back => (v3(0.0, -1.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Left => (v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Right => (v3(-1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Top => (v3(0.0, 0.0, -1.0), v3(0.0, 1.0, 0.0)),
        ViewKind::Bottom => (v3(0.0, 0.0, 1.0), v3(0.0, -1.0, 0.0)),
        ViewKind::Isometric => {
            let (s3, s6) = (3f64.sqrt(), 6f64.sqrt());
            (
                v3(-1.0 / s3, 1.0 / s3, -1.0 / s3),
                v3(-1.0 / s6, 1.0 / s6, 2.0 / s6),
            )
        }
    };
    (cross(d, ey), ey)
}

fn img(p: DVec3, kind: ViewKind) -> P {
    let (ex, ey) = axes(kind);
    (p.dot(ex), p.dot(ey))
}

fn frame(origin: DVec3, x: DVec3, y: DVec3) -> Frame {
    Frame {
        origin,
        x,
        y,
        z: cross(x, y),
    }
}

fn sheet_of(paper: Paper, landscape: bool) -> Sheet {
    Sheet { paper, landscape }
}

/// Blattmodell von Hand: Ansichten mit Namen und Linien (sichtbar/verdeckt), Rahmen 10 mm.
fn manual(sheet: Sheet, views: Vec<(&str, Vec<(Visibility, Out2d)>)>) -> SheetModel {
    let (w, h) = sheet.size_mm();
    SheetModel {
        sheet,
        frame: (DVec2::new(10.0, 10.0), DVec2::new(w - 10.0, h - 10.0)),
        views: views
            .into_iter()
            .enumerate()
            .map(|(i, (name, lines))| SheetView {
                id: ViewId(i as u64 + 1),
                name: name.to_string(),
                lines: lines
                    .into_iter()
                    .map(|(vis, curve)| SheetLine { vis, curve })
                    .collect(),
                extent_mm: (0.0, 0.0),
            })
            .collect(),
    }
}

fn one_view(lines: Vec<(Visibility, Out2d)>) -> SheetModel {
    manual(sheet_of(Paper::A3, true), vec![("Vorne", lines)])
}

fn line(a: (f64, f64), b: (f64, f64)) -> Out2d {
    Out2d::Line {
        a: DVec2::new(a.0, a.1),
        b: DVec2::new(b.0, b.1),
    }
}

fn arc(c: (f64, f64), r: f64, start: f64, end: f64) -> Out2d {
    Out2d::Arc {
        c: DVec2::new(c.0, c.1),
        r,
        start,
        end,
    }
}

fn svg_of(m: &SheetModel) -> String {
    svg_string(m).expect("SVG sollte sich schreiben lassen")
}

fn read_of(m: &SheetModel) -> (String, ReadSvg) {
    let s = svg_of(m);
    let r = svg_read::read(&s);
    (s, r)
}

/// Modell eines Körpers in einer Ansicht, Mitte der Ansicht bei (200, 150) auf A3 quer.
fn solid_model(solid: &Solid, kind: ViewKind, show_hidden: bool) -> SheetModel {
    let body = DrawBody {
        name: "K".into(),
        solid,
    };
    let vl = compute_view(&[body], kind, &Progress::new()).expect("Ansicht");
    let mut d = Drawing::new(
        DrawingId(1),
        "Z".into(),
        sheet_of(Paper::A3, true),
        Projection::First,
    );
    let id = d.add_view(
        kind,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 200.0, y: 150.0 },
        None,
    );
    d.views[0].show_hidden = show_hidden;
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(id, &vl)]);
    build_sheet(&d, &res)
}

/// Eine Ansicht aus Strichen von Hand: jeder Eintrag `(Strich, Stück-Beginn, Stück-Ende)`,
/// alles sichtbar; die Hüllbox wird vorgegeben (Mitte → bekannte Verschiebung auf das Blatt).
fn hand_model(
    kind: ViewKind,
    items: Vec<(Stroke, f64, f64)>,
    bbox: ((f64, f64), (f64, f64)),
    center: (f64, f64),
) -> SheetModel {
    let mut strokes = Vec::new();
    let mut pieces = Vec::new();
    for (i, (s, t0, t1)) in items.into_iter().enumerate() {
        pieces.push(Piece {
            stroke: i,
            t0,
            t1,
            vis: Visibility::Visible,
        });
        strokes.push(s);
    }
    let vl = ViewLines {
        kind,
        basis: Basis::of(kind),
        extent: 100.0,
        strokes,
        pieces,
        bbox,
        centerlines: Vec::new(),
        dropped_splits: 0,
    };
    let mut d = Drawing::new(
        DrawingId(1),
        "Z".into(),
        sheet_of(Paper::A3, true),
        Projection::First,
    );
    let id = d.add_view(
        kind,
        Scale { num: 1, den: 1 },
        Placement::Free {
            x: center.0,
            y: center.1,
        },
        None,
    );
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(id, &vl)]);
    build_sheet(&d, &res)
}

fn stroke(curve: Curve, t0: f64, t1: f64) -> Stroke {
    Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    }
}

/// Formelkurve mit Stützpunkten: Abstände werden am nächsten Stützpunkt grob gesucht und auf
/// der Formel selbst (Goldener Schnitt) verfeinert, damit die Sehnenfehler der Stützpunkte
/// (bei großen Kurven über 1e-6) nicht in die Messung eingehen.
struct Curve2<'a> {
    f: &'a dyn Fn(f64) -> P,
    t0: f64,
    t1: f64,
    pts: Vec<P>,
}

/// Formelkurve über `[t0, t1]` mit `n + 1` Stützpunkten.
fn poly<'a>(f: &'a dyn Fn(f64) -> P, t0: f64, t1: f64, n: usize) -> Curve2<'a> {
    Curve2 {
        f,
        t0,
        t1,
        pts: (0..=n)
            .map(|i| f(t0 + (t1 - t0) * i as f64 / n as f64))
            .collect(),
    }
}

/// Abstand eines Punkts zur Formelkurve (bei einer geschlossenen Kurve sind Anfang und Ende
/// derselbe Stützpunkt: dann werden beide Nachbarschaften verfeinert).
fn dist_poly(p: P, c: &Curve2) -> f64 {
    let n = c.pts.len() - 1;
    let (mut bi, mut bd) = (0usize, f64::INFINITY);
    for (i, q) in c.pts.iter().enumerate() {
        let d = dist(p, *q);
        if d < bd {
            (bi, bd) = (i, d);
        }
    }
    let at = |i: usize| c.t0 + (c.t1 - c.t0) * i as f64 / n as f64;
    let mut cands = vec![bi];
    if bi == 0 && dist(c.pts[0], c.pts[n]) < 1e-9 {
        cands.push(n);
    } else if bi == n && dist(c.pts[0], c.pts[n]) < 1e-9 {
        cands.push(0);
    }
    let g = 0.5 * (5f64.sqrt() - 1.0);
    let d = |t: f64| dist(p, (c.f)(t));
    let mut best = bd;
    for bi in cands {
        let (mut lo, mut hi) = (at(bi.saturating_sub(1)), at((bi + 1).min(n)));
        for _ in 0..80 {
            let (m1, m2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            if d(m1) < d(m2) {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        best = best.min(d(0.5 * (lo + hi)));
    }
    best
}

/// Punkte der gelesenen Stücke (`per_seg` je Stück).
fn read_points(paths: &[&ReadPath], per_seg: usize) -> Vec<P> {
    let mut v = Vec::new();
    for p in paths {
        for s in &p.segments {
            for i in 0..=per_seg {
                v.push(s.point_at(i as f64 / per_seg as f64));
            }
        }
    }
    v
}

/// Größte Abweichung in beiden Richtungen: gelesene Punkte zur Formelkurve und
/// Formelpunkte zu den gelesenen Pfaden.
fn two_way_deviation(paths: &[&ReadPath], formula: &Curve2, formula_checks: usize) -> f64 {
    let mut dev = 0.0f64;
    for p in read_points(paths, 64) {
        dev = dev.max(dist_poly(p, formula));
    }
    let step = (formula.pts.len() / formula_checks).max(1);
    for q in formula.pts.iter().step_by(step) {
        let d = paths
            .iter()
            .map(|p| p.distance_to(*q))
            .fold(f64::INFINITY, f64::min);
        dev = dev.max(d);
    }
    dev
}

fn close(a: P, b: P, tol: f64) -> bool {
    dist(a, b) <= tol
}

// ---------------------------------------------------------------------------------------------
// Eigene Splineauswertung (Cox–de Boor), unabhängig von der Bibliothek
// ---------------------------------------------------------------------------------------------

fn basis_fn(i: usize, p: usize, u: f64, k: &[f64], end: f64) -> f64 {
    if p == 0 {
        let inside = k[i] <= u && u < k[i + 1];
        let at_end = u == end && k[i] < k[i + 1] && k[i + 1] == end;
        return f64::from(inside || at_end);
    }
    let mut v = 0.0;
    let d1 = k[i + p] - k[i];
    if d1 > 0.0 {
        v += (u - k[i]) / d1 * basis_fn(i, p - 1, u, k, end);
    }
    let d2 = k[i + p + 1] - k[i + 1];
    if d2 > 0.0 {
        v += (k[i + p + 1] - u) / d2 * basis_fn(i + 1, p - 1, u, k, end);
    }
    v
}

fn nurbs_eval(p: usize, k: &[f64], ctrl: &[P], w: &[f64], u: f64) -> P {
    let end = k[ctrl.len()];
    let (mut x, mut y, mut s) = (0.0, 0.0, 0.0);
    for (i, c) in ctrl.iter().enumerate() {
        let b = basis_fn(i, p, u, k, end) * w[i];
        x += b * c.0;
        y += b * c.1;
        s += b;
    }
    (x / s, y / s)
}

fn domain(p: usize, k: &[f64], n: usize) -> (f64, f64) {
    (k[p], k[n])
}

fn spline(p: usize, knots: Vec<f64>, ctrl: &[P], weights: Vec<f64>) -> Out2d {
    Out2d::Spline {
        degree: p,
        knots,
        ctrl: ctrl.iter().map(|c| DVec2::new(c.0, c.1)).collect(),
        weights,
    }
}

/// Geklemmter gleichmäßiger Knotenvektor für `n` Kontrollpunkte und Grad `p`.
fn clamped(p: usize, n: usize) -> Vec<f64> {
    let mut k = vec![0.0; p + 1];
    let inner = n - p - 1;
    for j in 1..=inner {
        k.push(j as f64);
    }
    k.extend(vec![(inner + 1) as f64; p + 1]);
    k
}

/// Roundtrip eines Splines durch den Schreiber: größte Abweichung (beide Richtungen) und
/// die Befehlsfolge.
fn spline_roundtrip(curve: &Out2d) -> (f64, String) {
    let Out2d::Spline {
        degree,
        knots,
        ctrl,
        weights,
    } = curve
    else {
        panic!("Spline erwartet")
    };
    let cp: Vec<P> = ctrl.iter().map(|c| (c.x, c.y)).collect();
    let m = one_view(vec![(Visibility::Visible, curve.clone())]);
    let (_, r) = read_of(&m);
    assert_eq!(r.paths.len(), 1);
    let (a, b) = domain(*degree, knots, cp.len());
    let f = |u: f64| nurbs_eval(*degree, knots, &cp, weights, u);
    let formula = poly(&f, a, b, 4_000);
    let dev = two_way_deviation(&[&r.paths[0]], &formula, 1500);
    (dev, r.paths[0].kinds())
}

fn pts(v: &[(f64, f64)]) -> Vec<P> {
    v.to_vec()
}

// ---------------------------------------------------------------------------------------------
// (a) Kopf
// ---------------------------------------------------------------------------------------------

#[test]
fn header_and_size() {
    let m = manual(sheet_of(Paper::A3, true), vec![]);
    let s = svg_of(&m);
    assert!(
        s.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"420mm\" height=\"297mm\" viewBox=\"0 0 420 297\">"),
        "{s}"
    );
    let r = svg_read::read(&s);
    assert_eq!(r.width_attr, "420mm");
    assert_eq!(r.height_attr, "297mm");
    assert_eq!(r.viewbox, [0.0, 0.0, 420.0, 297.0]);
    let rect = r.rect.expect("Innenrahmen");
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (10.0, 10.0, 400.0, 277.0)
    );
    assert_eq!(rect.stroke_width, 0.5);
    assert_eq!(rect.fill, "none");
    assert!(s.contains("<rect x=\"10\" y=\"10\" width=\"400\" height=\"277\" fill=\"none\" stroke=\"#000\" stroke-width=\"0.5\"/>"));
    // Hochformat A0: 841 × 1189.
    let s = svg_of(&manual(sheet_of(Paper::A0, false), vec![]));
    assert!(s.contains("width=\"841mm\" height=\"1189mm\" viewBox=\"0 0 841 1189\""));
    assert!(s.contains("width=\"821\" height=\"1169\""));
    // Kein Pfad, keine Ansicht: nur die Wurzelgruppe.
    assert!(!s.contains("<path"));
}

#[test]
fn numbers_are_short_and_plain() {
    // Gerundet auf sechs Nachkommastellen, ohne Exponent, ohne nachgestellte Nullen, „-0“ → „0“.
    let m = one_view(vec![
        (
            Visibility::Visible,
            line((0.123_456_789, -0.000_000_1), (1e-9, 123_456.5)),
        ),
        (
            Visibility::Visible,
            line((-1e-7, 2.0), (1_000.100_000_4, 7.0)),
        ),
    ]);
    let s = svg_of(&m);
    assert!(
        s.contains("d=\"M 0.123457 0 L 0 123456.5 M 0 2 L 1000.1 7\""),
        "{s}"
    );
    assert!(!s.contains("-0 ") && !s.contains("-0\""));
}

// ---------------------------------------------------------------------------------------------
// (b) Quader von vorne
// ---------------------------------------------------------------------------------------------

#[test]
fn box_front_view_roundtrips() {
    // Quader 25 × 10 × 15, Vorderansicht: Rechteck 25 × 15, Umfang 80, Mitte bei (200, 150).
    let m = solid_model(&box_solid(25.0, 10.0, 15.0), ViewKind::Front, false);
    let (s, r) = read_of(&m);
    assert_eq!(r.group_ids, ["Vorne"]);
    let vis = r.visible("Vorne");
    assert_eq!(vis.len(), 1, "genau ein sichtbarer Pfad");
    assert!(r.hidden("Vorne").is_empty());
    let p = vis[0];
    assert_eq!(p.kinds(), "LLLL", "vier Strecken");
    assert_eq!(p.subpaths, 4);
    assert_eq!(p.dasharray, None);
    assert!(!s.contains("stroke-dasharray"), "sichtbar ohne Strichelung");
    assert_eq!(
        (
            p.stroke.as_str(),
            p.fill.as_str(),
            p.linecap.as_str(),
            p.width
        ),
        ("#000", "none", "round", 0.5)
    );
    assert!((p.length() - 80.0).abs() < 1e-6, "{}", p.length());
    // Ecken von Hand: Mitte (200, 150) ± (12,5 | 7,5).
    let corners = [
        (187.5, 142.5),
        (212.5, 142.5),
        (212.5, 157.5),
        (187.5, 157.5),
    ];
    for c in corners {
        let n = p
            .segments
            .iter()
            .filter(|sg| close(sg.start(), c, 1e-6))
            .count()
            + p.segments
                .iter()
                .filter(|sg| close(sg.end(), c, 1e-6))
                .count();
        assert_eq!(n, 2, "Ecke {c:?} gehört zu zwei Strecken");
    }
}

// ---------------------------------------------------------------------------------------------
// (c) Zylinder von oben = Kreis
// ---------------------------------------------------------------------------------------------

#[test]
fn cylinder_top_roundtrips_to_circle() {
    let r0 = 7.0;
    let m = solid_model(&cylinder_solid(r0, 20.0), ViewKind::Top, false);
    let (_, r) = read_of(&m);
    let vis = r.visible("Oben");
    assert_eq!(vis.len(), 1);
    assert!(r.hidden("Oben").is_empty());
    let p = vis[0];
    assert!(p.kinds().chars().all(|c| c == 'A'), "{}", p.kinds());
    assert!((p.length() - TAU * r0).abs() < 1e-6, "{}", p.length());
    let center = (200.0, 150.0);
    for sg in &p.segments {
        let Seg::Arc { c, u, v, .. } = sg else {
            panic!("Bogen erwartet")
        };
        assert!(close(*c, center, 1e-6), "Mittelpunkt {c:?}");
        assert!((dist((0.0, 0.0), *u) - r0).abs() < 1e-6);
        assert!((dist((0.0, 0.0), *v) - r0).abs() < 1e-6);
    }
    for q in read_points(&[p], 200) {
        assert!((dist(q, center) - r0).abs() < 1e-6, "{q:?}");
    }
    // Geschlossen und lückenlos.
    let n = p.segments.len();
    for i in 0..n {
        assert!(close(
            p.segments[i].end(),
            p.segments[(i + 1) % n].start(),
            1e-6
        ));
    }
    // Mittelpunkt und Radius auch aus drei Punkten der Datei (Umkreis), nicht aus dem Leser.
    let (a, b, c) = (
        p.segments[0].point_at(0.0),
        p.segments[1].point_at(0.5),
        p.segments[2].point_at(0.3),
    );
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    let ux = ((a.0 * a.0 + a.1 * a.1) * (b.1 - c.1)
        + (b.0 * b.0 + b.1 * b.1) * (c.1 - a.1)
        + (c.0 * c.0 + c.1 * c.1) * (a.1 - b.1))
        / d;
    let uy = ((a.0 * a.0 + a.1 * a.1) * (c.0 - b.0)
        + (b.0 * b.0 + b.1 * b.1) * (a.0 - c.0)
        + (c.0 * c.0 + c.1 * c.1) * (b.0 - a.0))
        / d;
    assert!(close((ux, uy), center, 1e-6));
    assert!((dist((ux, uy), a) - r0).abs() < 1e-6);
}

// ---------------------------------------------------------------------------------------------
// (d) Zylinder isometrisch: Ellipsen
// ---------------------------------------------------------------------------------------------

#[test]
fn iso_cylinder_ellipse_roundtrips() {
    let (r0, h0) = (10.0, 30.0);
    let kind = ViewKind::Isometric;
    let m = solid_model(&cylinder_solid(r0, h0), kind, true);
    let (_, r) = read_of(&m);
    // Hüllbox der Ansicht von Hand: x ∈ [−r, r]; y von −r/√3 (unten) bis 2h/√6 + r/√3;
    // Mitte (0, h/√6) → Verschiebung auf das Blatt (200, 150) − (0, h/√6).
    let t = (200.0, 150.0 - h0 / 6f64.sqrt());
    let ring = |z: f64| {
        move |th: f64| {
            let q = img(v3(r0 * th.cos(), r0 * th.sin(), z), kind);
            (q.0 + t.0, q.1 + t.1)
        }
    };
    let (ring_top, ring_bottom) = (ring(h0), ring(0.0));
    let top = poly(&ring_top, 0.0, TAU, 40_000);
    let bottom = poly(&ring_bottom, 0.0, TAU, 40_000);
    let visible = r.visible("Isometrisch");
    let hidden = r.hidden("Isometrisch");
    assert_eq!(visible.len(), 1);
    assert_eq!(hidden.len(), 1);
    // Jeder gelesene Bogenpunkt liegt auf der oberen oder der unteren Ellipse.
    let mut top_len = 0.0;
    for p in visible.iter().chain(hidden.iter()) {
        for sg in &p.segments {
            if let Seg::Arc { .. } = sg {
                let pts_: Vec<P> = (0..=64).map(|i| sg.point_at(i as f64 / 64.0)).collect();
                let on_top = pts_.iter().all(|&q| dist_poly(q, &top) < 1e-6);
                let on_bottom = pts_.iter().all(|&q| dist_poly(q, &bottom) < 1e-6);
                assert!(on_top || on_bottom, "Bogen liegt auf keiner Ellipse");
                if on_top {
                    top_len += sg.length();
                }
            }
        }
    }
    // Umfang der oberen Ellipse aus der Formel (Polygonzug, 40000 Sehnen).
    let formula_len: f64 = top.pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    assert!(
        (top_len - formula_len).abs() < 1e-5,
        "{top_len} vs {formula_len}"
    );
    // Obere Ellipse ganz sichtbar.
    for q in top.pts.iter().step_by(400) {
        let d = visible
            .iter()
            .map(|p| p.distance_to(*q))
            .fold(f64::INFINITY, f64::min);
        assert!(d < 1e-6, "obere Ellipse {q:?}: {d}");
    }
    // Untere Ellipse: sichtbar, wo n·d < 0 mit n = (cos, sin, 0), d = (−1, 1, −1)/√3,
    // also sin θ < cos θ; sonst verdeckt (Streifen um die Wechselstellen ausgenommen).
    let mut vis_seen = 0;
    let mut hid_seen = 0;
    for i in (0..40_000).step_by(100) {
        let th = TAU * i as f64 / 40_000.0;
        let g = th.sin() - th.cos();
        if g.abs() < 0.05 {
            continue;
        }
        let q = bottom.pts[i];
        let dv = visible
            .iter()
            .map(|p| p.distance_to(q))
            .fold(f64::INFINITY, f64::min);
        let dh = hidden
            .iter()
            .map(|p| p.distance_to(q))
            .fold(f64::INFINITY, f64::min);
        if g < 0.0 {
            assert!(dv < 1e-6, "sichtbare Hälfte θ = {th}: {dv}");
            vis_seen += 1;
        } else {
            assert!(dh < 1e-6, "verdeckte Hälfte θ = {th}: {dh}");
            assert!(dv > 1e-3, "verdeckte Hälfte nicht sichtbar θ = {th}: {dv}");
            hid_seen += 1;
        }
    }
    assert!(vis_seen > 100 && hid_seen > 100);
    // Alle Bögen tragen die Drehung der Hauptachse: Hauptachse liegt in x-Richtung des
    // Blatts (ex = (1,1,0)/√2), also ist eine Halbachse des Leser-Bogens (u oder v) etwa (r, 0)
    // bzw. (0, r/√3) — unabhängig davon, in welchem Viertel der Bogen liegt.
    let mut saw = false;
    for p in visible.iter().chain(hidden.iter()) {
        for sg in &p.segments {
            if let Seg::Arc { c, u, v, .. } = sg {
                let _ = c;
                let (lu, lv) = (dist((0.0, 0.0), *u), dist((0.0, 0.0), *v));
                let (big, small) = (lu.max(lv), lu.min(lv));
                assert!((big - r0).abs() < 1e-5, "große Halbachse {big}");
                assert!(
                    (small - r0 / 3f64.sqrt()).abs() < 1e-5,
                    "kleine Halbachse {small}"
                );
                saw = true;
            }
        }
    }
    assert!(saw);
}

// ---------------------------------------------------------------------------------------------
// (e) Splines
// ---------------------------------------------------------------------------------------------

fn cubic_ctrl() -> Vec<P> {
    pts(&[
        (0.0, 1.0),
        (2.0, 4.0),
        (5.0, -2.0),
        (7.0, 3.0),
        (9.0, 0.0),
        (12.0, 5.0),
    ])
}

#[test]
fn spline_roundtrips_within_1e6() {
    // NURBS-Kante (Grad 3, innere Knoten 0,3 und 0,6) in der Draufsicht (x, y), Mitte der
    // Hüllbox (6, 1,5) → Verschiebung (200, 150) − (6, 1,5).
    let ctrl3: Vec<DVec3> = cubic_ctrl().iter().map(|c| v3(c.0, c.1, 0.0)).collect();
    let knots = vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0];
    let c = NurbsCurve::new(3, knots.clone(), ctrl3, vec![1.0; 6]).unwrap();
    let m = hand_model(
        ViewKind::Top,
        vec![(stroke(Curve::Nurbs(Arc::new(c)), 0.0, 1.0), 0.0, 1.0)],
        ((0.0, -3.0), (12.0, 6.0)),
        (200.0, 150.0),
    );
    let (_, r) = read_of(&m);
    let vis = r.visible("Oben");
    assert_eq!(vis.len(), 1);
    assert_eq!(vis[0].kinds(), "CCC", "drei Bézier-Stücke");
    let t = (200.0 - 6.0, 150.0 - 1.5);
    let ctrl = cubic_ctrl();
    let f = |u: f64| {
        let q = nurbs_eval(3, &knots, &ctrl, &[1.0; 6], u);
        (q.0 + t.0, q.1 + t.1)
    };
    let formula = poly(&f, 0.0, 1.0, 4_000);
    let dev = two_way_deviation(&[vis[0]], &formula, 2000);
    eprintln!("Spline Grad 3: größte Abweichung {dev:e}");
    assert!(dev < 1e-6, "{dev}");
    // Auch die Länge stimmt mit dem Polygonzug der Formel überein.
    let flen: f64 = formula.pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    assert!(
        (vis[0].length() - flen).abs() < 1e-5,
        "{} vs {flen}",
        vis[0].length()
    );
}

#[test]
fn spline_degrees_roundtrip() {
    // Nicht rationale Splines der Grade 1 bis 6 mit inneren Knoten (zum Teil doppelt);
    // Grad 1 → L, Grad 2 → Q, Grad 3 → C, höher: kubische Näherung.
    let ctrl: Vec<P> = (0..9)
        .map(|i| {
            let x = i as f64 * 6.0 + [0.0, 2.0, -3.0, 1.0, 4.0, -2.0, 3.0, 0.5, -1.0][i];
            let y = [0.0, 9.0, -4.0, 7.0, -8.0, 5.0, 2.0, -6.0, 3.0][i];
            (x + 20.0, y + 40.0)
        })
        .collect();
    let mut worst = 0.0f64;
    for p in 1..=6usize {
        let n = 9usize;
        let kn = clamped(p, n);
        let curve = spline(p, kn, &ctrl, vec![1.0; n]);
        let (dev, kinds) = spline_roundtrip(&curve);
        eprintln!("Grad {p}: Abweichung {dev:e}, Befehle {kinds}");
        assert!(dev < 1e-6, "Grad {p}: {dev}");
        match p {
            1 => assert!(kinds.chars().all(|c| c == 'L')),
            2 => assert!(kinds.chars().all(|c| c == 'Q')),
            3 => assert!(kinds.chars().all(|c| c == 'C')),
            _ => assert!(kinds.chars().all(|c| c == 'C')),
        }
        worst = worst.max(dev);
    }
    // Doppelter innerer Knoten (Knick) bei Grad 3: ein Absatz C0 ohne Sprung.
    let kn = vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 3.0, 4.0, 4.0, 4.0, 4.0];
    let curve = spline(3, kn, &ctrl[..8], vec![1.0; 8]);
    let (dev, kinds) = spline_roundtrip(&curve);
    eprintln!("Grad 3 mit Doppelknoten: {dev:e} {kinds}");
    assert!(dev < 1e-6);
    eprintln!("größte Abweichung nicht rationaler Splines: {worst:e}");
}

#[test]
fn rational_splines_roundtrip_within_1e6() {
    // Viertelkreis als rationale Kurve Grad 2: Radius 10 um (50, 60).
    let w = 0.5f64.sqrt();
    let quarter = spline(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[(60.0, 60.0), (60.0, 70.0), (50.0, 70.0)],
        vec![1.0, w, 1.0],
    );
    let (dev, kinds) = spline_roundtrip(&quarter);
    eprintln!("Viertelkreis rational: {dev:e} {kinds}");
    assert!(dev < 1e-6, "{dev}");
    // Alle Punkte der Datei liegen auf dem Kreis (Formel, nicht Kurvenauswertung).
    let m = one_view(vec![(Visibility::Visible, quarter)]);
    let (_, r) = read_of(&m);
    for q in read_points(&[&r.paths[0]], 100) {
        assert!((dist(q, (50.0, 60.0)) - 10.0).abs() < 1e-6, "{q:?}");
    }
    // Vollkreis aus neun Kontrollpunkten, Radius 120 (große Koordinaten).
    let (cx, cy, rr) = (300.0, 200.0, 120.0);
    let cp = pts(&[
        (cx + rr, cy),
        (cx + rr, cy + rr),
        (cx, cy + rr),
        (cx - rr, cy + rr),
        (cx - rr, cy),
        (cx - rr, cy - rr),
        (cx, cy - rr),
        (cx + rr, cy - rr),
        (cx + rr, cy),
    ]);
    let ww: Vec<f64> = (0..9).map(|i| if i % 2 == 0 { 1.0 } else { w }).collect();
    let full = spline(
        2,
        vec![
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ],
        &cp,
        ww,
    );
    let (dev, kinds) = spline_roundtrip(&full);
    eprintln!("Vollkreis rational R=120: {dev:e} {kinds}");
    assert!(dev < 1e-6, "{dev}");
    let m = one_view(vec![(Visibility::Visible, full)]);
    let (_, r) = read_of(&m);
    for q in read_points(&[&r.paths[0]], 100) {
        assert!((dist(q, (cx, cy)) - rr).abs() < 1e-6, "{q:?}");
    }
    // Wilde Gewichte, Grad 2 und Grad 3 (rational), eigene Auswertung als Wahrheit.
    let ctrl = pts(&[
        (20.0, 30.0),
        (35.0, 80.0),
        (60.0, 10.0),
        (90.0, 70.0),
        (120.0, 25.0),
    ]);
    let c2 = spline(
        2,
        vec![0.0, 0.0, 0.0, 0.4, 0.7, 1.0, 1.0, 1.0],
        &ctrl,
        vec![1.0, 2.0, 0.5, 3.0, 1.0],
    );
    let (dev, kinds) = spline_roundtrip(&c2);
    eprintln!("rational Grad 2 wilde Gewichte: {dev:e} {kinds}");
    assert!(dev < 1e-6, "{dev}");
    let c3 = spline(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
        &ctrl,
        vec![1.0, 2.0, 0.5, 3.0, 1.0],
    );
    let (dev, kinds) = spline_roundtrip(&c3);
    eprintln!("rational Grad 3: {dev:e} {kinds}");
    assert!(dev < 1e-6, "{dev}");
}

#[test]
fn closed_nurbs_seam_pieces_roundtrip() {
    // Geschlossene Freiformkurve; ein Stück über die Naht [0,7; 1,3] ergibt zwei Ausgabekurven
    // (zwei SheetLines, zwei Teilpfade). Draufsicht (x, y), Hüllbox (−13,−13)–(13,13) → Mitte 0.
    let c = closed_nurbs_loop();
    let ctrl = pts(&[
        (10.0, 0.0),
        (10.0, 10.0),
        (-10.0, 10.0),
        (-10.0, -10.0),
        (10.0, -10.0),
        (10.0, -5.0),
        (10.0, 0.0),
    ]);
    let knots = vec![0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0];
    let m = hand_model(
        ViewKind::Top,
        vec![(stroke(Curve::Nurbs(Arc::new(c)), 0.0, 1.0), 0.7, 1.3)],
        ((-13.0, -13.0), (13.0, 13.0)),
        (200.0, 150.0),
    );
    assert_eq!(m.views[0].lines.len(), 2, "zwei Stücke über die Naht");
    let (_, r) = read_of(&m);
    let p = r.visible("Oben")[0];
    assert_eq!(p.subpaths, 2);
    let f = |u: f64| {
        let q = nurbs_eval(3, &knots, &ctrl, &[1.0; 7], u);
        (q.0 + 200.0, q.1 + 150.0)
    };
    // Wahrheit: u ∈ [0,7; 1] und [0; 0,3] getrennt (zwei Polygonzüge).
    let a = poly(&f, 0.7, 1.0, 2_000);
    let b = poly(&f, 0.0, 0.3, 2_000);
    let mut dev = 0.0f64;
    for q in read_points(&[p], 64) {
        dev = dev.max(dist_poly(q, &a).min(dist_poly(q, &b)));
    }
    for q in a.pts.iter().step_by(10).chain(b.pts.iter().step_by(10)) {
        dev = dev.max(p.distance_to(*q));
    }
    eprintln!("Naht: größte Abweichung {dev:e}");
    assert!(dev < 1e-6, "{dev}");
    // Beide Teilpfade beginnen/enden an der Naht (10, 0) + Verschiebung.
    let seam = (210.0, 150.0);
    let starts: Vec<P> = p.segments.iter().map(Seg::start).collect();
    let ends: Vec<P> = p.segments.iter().map(Seg::end).collect();
    assert!(ends.iter().any(|&e| close(e, seam, 1e-6)));
    assert!(starts.iter().any(|&s| close(s, seam, 1e-6)));
}

// ---------------------------------------------------------------------------------------------
// (f) Verdeckte Linien gestrichelt
// ---------------------------------------------------------------------------------------------

#[test]
fn hidden_lines_are_dashed() {
    // Quader 30 × 20 × 20 mit Bohrung (r = 4) längs X; von vorne sind die zwei Mantellinien der
    // Bohrung bei z = 10 ± 4 verdeckt.
    let m = solid_model(
        &box_with_hole(30.0, 20.0, 20.0, 4.0, v3(1.0, 0.0, 0.0)),
        ViewKind::Front,
        true,
    );
    let (s, r) = read_of(&m);
    let hid = r.hidden("Vorne");
    assert_eq!(hid.len(), 1, "genau ein verdeckter Pfad");
    let h = hid[0];
    assert!(s.contains("stroke-dasharray=\"3 1\""));
    assert_eq!(h.dasharray.as_deref(), Some("3 1"));
    assert_eq!(h.width, 0.25);
    assert_eq!(h.linecap, "round");
    assert_eq!(h.kinds(), "LL");
    assert!((h.length() - 60.0).abs() < 1e-6, "{}", h.length());
    // Blatt-y: Mitte 150 ± 4; x von 185 bis 215.
    let mut ys: Vec<f64> = h.segments.iter().map(|sg| sg.start().1).collect();
    ys.sort_by(f64::total_cmp);
    assert!(
        (ys[0] - 146.0).abs() < 1e-6 && (ys[1] - 154.0).abs() < 1e-6,
        "{ys:?}"
    );
    for sg in &h.segments {
        let (a, b) = (sg.start(), sg.end());
        assert!((a.1 - b.1).abs() < 1e-9);
        assert!((a.0.min(b.0) - 185.0).abs() < 1e-6 && (a.0.max(b.0) - 215.0).abs() < 1e-6);
    }
    // Die sichtbaren Linien sind durchgezogen, 0,5 breit.
    let vis = r.visible("Vorne");
    assert_eq!(vis.len(), 1);
    assert_eq!(vis[0].dasharray, None);
    assert_eq!(vis[0].width, 0.5);
    assert!(
        (vis[0].length() - 100.0).abs() < 1e-6,
        "{}",
        vis[0].length()
    );
    // Ohne show_hidden entfällt der gestrichelte Pfad ganz (keine leere Gruppe).
    let m = solid_model(
        &box_with_hole(30.0, 20.0, 20.0, 4.0, v3(1.0, 0.0, 0.0)),
        ViewKind::Front,
        false,
    );
    let s = svg_of(&m);
    assert!(!s.contains("stroke-dasharray"));
    assert_eq!(s.matches("<path").count(), 1);
}

// ---------------------------------------------------------------------------------------------
// (g) y-Achse einmal gespiegelt
// ---------------------------------------------------------------------------------------------

#[test]
fn y_axis_is_flipped_once() {
    let m = manual(
        sheet_of(Paper::A4, true),
        vec![(
            "Vorne",
            vec![(Visibility::Visible, line((20.0, 10.0), (50.0, 10.0)))],
        )],
    );
    let (s, r) = read_of(&m);
    // Genau eine Umrechnung, und zwar auf der Wurzelgruppe (A4 quer: Höhe 210).
    assert_eq!(s.matches("transform=").count(), 1, "{s}");
    assert!(
        s.contains("<g transform=\"matrix(1 0 0 -1 0 210)\">"),
        "{s}"
    );
    assert_eq!(r.transform_count, 1);
    // Die Zahlen der Datei bleiben Blattkoordinaten.
    assert!(s.contains("d=\"M 20 10 L 50 10\""), "{s}");
    // Zurückgerechnet liegt die Linie bei Blatt-y = 10; im Bild (Rohkoordinate) bei H − 10.
    let p = r.visible("Vorne")[0];
    assert!(close(p.segments[0].start(), (20.0, 10.0), 1e-12));
    assert!(close(p.segments[0].end(), (50.0, 10.0), 1e-12));
    assert!(((210.0 - p.segments[0].start().1) - 200.0).abs() < 1e-12);
    // Der Innenrahmen liegt außerhalb der Gruppe in Rohkoordinaten (symmetrisch: 10, 10).
    let rect = r.rect.unwrap();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (10.0, 10.0, 277.0, 190.0)
    );
}

// ---------------------------------------------------------------------------------------------
// (h) Nicht endliche Zahlen
// ---------------------------------------------------------------------------------------------

fn refused(bad: Out2d) {
    let m = one_view(vec![
        (Visibility::Visible, line((0.0, 0.0), (1.0, 1.0))),
        (Visibility::Hidden, bad.clone()),
    ]);
    assert!(
        matches!(svg_string(&m), Err(DrawError::NonFinite)),
        "{bad:?}"
    );
    let mut sink: Vec<u8> = Vec::new();
    assert!(matches!(
        write_svg(&m, &mut sink),
        Err(DrawError::NonFinite)
    ));
    assert!(sink.is_empty(), "bei einem Fehler wird nichts geschrieben");
}

#[test]
fn non_finite_is_refused() {
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    refused(line((nan, 0.0), (1.0, 1.0)));
    refused(line((0.0, 0.0), (1.0, inf)));
    refused(line((0.0, 0.0), (-inf, 1.0)));
    refused(arc((0.0, 0.0), nan, 0.0, 1.0));
    refused(arc((0.0, 0.0), 1.0, inf, 2.0));
    refused(arc((nan, 0.0), 1.0, 0.0, 2.0));
    refused(Out2d::Ellipse {
        c: DVec2::ZERO,
        major: DVec2::new(1.0, 0.0),
        ratio: nan,
        start: 0.0,
        end: 1.0,
    });
    refused(Out2d::Ellipse {
        c: DVec2::ZERO,
        major: DVec2::new(inf, 0.0),
        ratio: 0.5,
        start: 0.0,
        end: 1.0,
    });
    let kn = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let cp = [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)];
    refused(spline(2, kn.clone(), &cp, vec![1.0, nan, 1.0]));
    refused(spline(
        2,
        kn.clone(),
        &[(0.0, 0.0), (inf, 1.0), (2.0, 0.0)],
        vec![1.0; 3],
    ));
    refused(spline(
        2,
        vec![0.0, 0.0, nan, 1.0, 1.0, 1.0],
        &cp,
        vec![1.0; 3],
    ));
    // Ungültig aufgebaut (leer, Knotenzahl falsch, Gewicht ≤ 0): ebenfalls Fehler, keine Panik.
    refused(spline(3, vec![], &[], vec![]));
    refused(spline(2, vec![0.0, 1.0], &cp, vec![1.0; 3]));
    refused(spline(2, kn, &cp, vec![1.0, -1.0, 1.0]));
    // Rahmen mit nicht endlicher Zahl.
    let mut m = one_view(vec![]);
    m.frame.1.x = nan;
    assert!(matches!(svg_string(&m), Err(DrawError::NonFinite)));
}

// ---------------------------------------------------------------------------------------------
// (i) Ansichtsnamen
// ---------------------------------------------------------------------------------------------

#[test]
fn view_group_ids_are_names_and_escaped() {
    let l = || vec![(Visibility::Visible, line((20.0, 20.0), (30.0, 30.0)))];
    let m = manual(
        sheet_of(Paper::A3, true),
        vec![
            ("A<&\">'", l()),
            ("Vorne", l()),
            ("Vorne", l()),
            ("Vorne 2", l()),
            ("Vorne", l()),
            ("", l()),
            ("Ü\tx\n\u{1}", l()),
        ],
    );
    let (s, r) = read_of(&m);
    assert!(s.contains("<g id=\"A&lt;&amp;&quot;&gt;&apos;\">"), "{s}");
    assert_eq!(
        r.group_ids,
        [
            "A<&\">'",
            "Vorne",
            "Vorne 2",
            "Vorne 2 2",
            "Vorne 3",
            "Ansicht",
            "Ü\tx\n "
        ]
    );
    // Der Leser prüft Eindeutigkeit der Kennungen und die Verschachtelung selbst (Absturz sonst);
    // jede Gruppe enthält ihren Pfad.
    for id in &r.group_ids {
        assert_eq!(r.paths_of(id).count(), 1, "{id}");
    }
    // Keine rohen Sonderzeichen im Attribut.
    assert!(!s.contains("id=\"A<"));
    assert!(s.contains("&#9;") && s.contains("&#10;"));
}

#[test]
fn names_from_build_sheet_are_unique_and_plain() {
    // Zwei Vorderansichten derselben Zeichnung heißen „Vorne“ und „Vorne 2“.
    let solid = box_solid(10.0, 10.0, 10.0);
    let body = DrawBody {
        name: "K".into(),
        solid: &solid,
    };
    let vl = compute_view(&[body], ViewKind::Front, &Progress::new()).unwrap();
    let mut d = Drawing::new(
        DrawingId(1),
        "Z".into(),
        sheet_of(Paper::A3, true),
        Projection::First,
    );
    let a = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    let b = d.add_view(
        ViewKind::Front,
        Scale { num: 2, den: 1 },
        Placement::Free { x: 300.0, y: 100.0 },
        None,
    );
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(a, &vl), (b, &vl)]);
    let (_, r) = read_of(&build_sheet(&d, &res));
    assert_eq!(r.group_ids, ["Vorne", "Vorne 2"]);
    assert_eq!(
        r.visible("Vorne")[0].length(),
        r.visible("Vorne")[0].length()
    );
    assert!((r.visible("Vorne")[0].length() - 40.0).abs() < 1e-6);
    assert!((r.visible("Vorne 2")[0].length() - 80.0).abs() < 1e-6);
}

// ---------------------------------------------------------------------------------------------
// Bogenrichtung (Spiegelung und Sweep-Flag)
// ---------------------------------------------------------------------------------------------

#[test]
fn arc_direction_hand_check() {
    // Viertelbogen gegen den Uhrzeigersinn (y oben) um (20, 30), r = 10, von 0 bis 90°:
    // Anfang (30, 30), Ende (20, 40). In der gespiegelten Gruppe bleiben die Zahlen Blattzahlen,
    // und „wachsender Winkel“ heißt sweep-flag = 1.
    let m = one_view(vec![(
        Visibility::Visible,
        arc((20.0, 30.0), 10.0, 0.0, PI / 2.0),
    )]);
    let (s, r) = read_of(&m);
    assert!(s.contains("d=\"M 30 30 A 10 10 0 0 1 20 40\""), "{s}");
    // Der Bogen läuft nach oben rechts (weg vom Mittelpunkt): Mitte bei c + 10·(cos 45°, sin 45°).
    let p = r.visible("Vorne")[0];
    let mid = p.segments[0].point_at(0.5);
    let want = (20.0 + 10.0 * 0.5f64.sqrt(), 30.0 + 10.0 * 0.5f64.sqrt());
    assert!(close(mid, want, 1e-9), "{mid:?} vs {want:?}");
    // Zur Gegenprobe: die Bildkoordinate (nach Gruppenmatrix) der Mitte liegt bei y = 210 − …;
    // der Mittelpunkt (20, 30) liegt unter dem Bogen, also im Bild darüber (kleineres y).
    let screen = |q: P| (q.0, 297.0 - q.1);
    assert!(screen((20.0, 30.0)).1 > screen(mid).1);
}

#[test]
fn arc_angles_negative_and_beyond_tau() {
    let c = (60.0, 70.0);
    let r0 = 12.0;
    let cases: &[(f64, f64)] = &[
        (-7.0, -5.0),       // unter −TAU
        (-0.5, 2.0),        // über null hinweg
        (20.0, 22.5),       // über TAU
        (6.0, 6.0 + 1.0),   // über die Nahtstelle 2π
        (-1.0, -1.0 + TAU), // Vollkreis ab −1
        (3.0, 3.0 + 4.9),   // knapp ¾ Kreis mit Anfang über π/2
        (0.1, 0.1 + 0.001), // winziger Bogen
    ];
    for &(a, b) in cases {
        let m = one_view(vec![(Visibility::Visible, arc(c, r0, a, b))]);
        let (_, r) = read_of(&m);
        let p = r.visible("Vorne")[0];
        let f = |t: f64| (c.0 + r0 * t.cos(), c.1 + r0 * t.sin());
        let formula = poly(&f, a, b, 4_000);
        let dev = two_way_deviation(&[p], &formula, 400);
        assert!(dev < 1e-6, "[{a}, {b}]: {dev}");
        // Die Länge hängt an den auf 6 Stellen gerundeten Endpunkten (vier Stücke, bis etwa 5e-6).
        assert!(
            (p.length() - r0 * (b - a)).abs() < 5e-6,
            "[{a}, {b}] Länge {} vs {}",
            p.length(),
            r0 * (b - a)
        );
        // Anfangspunkt (Winkel a) und Endpunkt (Winkel b), Richtung gegen den Uhrzeigersinn.
        assert!(
            close(p.segments[0].start(), f(a), 1e-6),
            "[{a}, {b}] Anfang"
        );
        assert!(
            close(p.segments.last().unwrap().end(), f(b), 1e-6),
            "[{a}, {b}] Ende"
        );
    }
    // Öffnungswinkel 0: nichts zu zeichnen.
    let m = one_view(vec![(Visibility::Visible, arc(c, r0, 1.0, 1.0))]);
    assert!(!svg_of(&m).contains("<path"));
}

#[test]
fn mirrored_views_keep_arc_orientation() {
    // Unterseite: (x, y, z) → (x, −y), also gespiegelt. Teilkreis um (3, 4, 5), r = 6,
    // Parameter 0,3 bis 2,2 (gegen den Uhrzeigersinn im Raum, im Bild im Uhrzeigersinn).
    let kind = ViewKind::Bottom;
    let r0 = 6.0;
    let s = stroke(
        Curve::Circle {
            frame: frame(v3(3.0, 4.0, 5.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0)),
            radius: r0,
        },
        0.3,
        2.2,
    );
    // Hüllbox (−10, −15)–(20, 5): Mitte (5, −5) → Verschiebung (195, 155).
    let m = hand_model(
        kind,
        vec![(s, 0.3, 2.2)],
        ((-10.0, -15.0), (20.0, 5.0)),
        (200.0, 150.0),
    );
    let (_, r) = read_of(&m);
    let p = r.visible("Unten")[0];
    let f = |t: f64| {
        let q = img(v3(3.0 + r0 * t.cos(), 4.0 + r0 * t.sin(), 5.0), kind);
        (q.0 + 195.0, q.1 + 155.0)
    };
    let formula = poly(&f, 0.3, 2.2, 4_000);
    let dev = two_way_deviation(&[p], &formula, 400);
    assert!(dev < 1e-6, "Unterseite: {dev}");
    assert!((p.length() - r0 * 1.9).abs() < 1e-6);
    // Anfang und Ende in der Richtung des Parameters: die Spiegelung kehrt die Reihenfolge um,
    // also steht P(2,2) am Anfang des Pfads (der Bogen läuft im Bild gegen den Uhrzeigersinn).
    let (e0, e1) = (p.segments[0].start(), p.segments.last().unwrap().end());
    assert!(
        close(e0, f(2.2), 1e-6) && close(e1, f(0.3), 1e-6),
        "{e0:?} {e1:?}"
    );

    // Rückansicht: (x, y, z) → (−x, z), gespiegelt; Kreis in der XZ-Ebene und ein gekippter
    // Kreis (Ellipse im Bild), beide Teilbögen über mehr als π.
    let kind = ViewKind::Back;
    for (fy, name) in [(v3(0.0, 0.0, 1.0), "XZ"), (v3(0.0, 0.6, 0.8), "gekippt")] {
        let s = stroke(
            Curve::Circle {
                frame: frame(v3(2.0, 0.0, 3.0), v3(1.0, 0.0, 0.0), fy),
                radius: 5.0,
            },
            -1.0,
            4.0,
        );
        let m = hand_model(
            kind,
            vec![(s, -1.0, 4.0)],
            ((-20.0, -10.0), (20.0, 10.0)),
            (200.0, 150.0),
        );
        let (_, r) = read_of(&m);
        let p = r.visible("Hinten")[0];
        let f = |t: f64| {
            let (ct, st) = (t.cos(), t.sin());
            let pnt = v3(2.0, 0.0, 3.0) + v3(1.0, 0.0, 0.0) * (5.0 * ct) + fy * (5.0 * st);
            let q = img(pnt, kind);
            (q.0 + 200.0, q.1 + 150.0)
        };
        let formula = poly(&f, -1.0, 4.0, 4_000);
        let dev = two_way_deviation(&[p], &formula, 400);
        assert!(dev < 1e-6, "Rückansicht {name}: {dev}");
    }
    // Isometrische Ellipse (Curve::Ellipse), Parameter über TAU hinaus.
    let kind = ViewKind::Isometric;
    let fr = frame(v3(1.0, 2.0, 3.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0));
    let s = stroke(
        Curve::Ellipse {
            frame: fr,
            major: 9.0,
            minor: 4.0,
        },
        5.0,
        8.5,
    );
    let m = hand_model(
        kind,
        vec![(s, 5.0, 8.5)],
        ((-20.0, -20.0), (20.0, 20.0)),
        (200.0, 150.0),
    );
    let (_, r) = read_of(&m);
    let p = r.visible("Isometrisch")[0];
    let f = |t: f64| {
        let q = img(v3(1.0 + 9.0 * t.cos(), 2.0 + 4.0 * t.sin(), 3.0), kind);
        (q.0 + 200.0, q.1 + 150.0)
    };
    let formula = poly(&f, 5.0, 8.5, 4_000);
    let dev = two_way_deviation(&[p], &formula, 400);
    assert!(dev < 1e-6, "Isometrische Ellipse: {dev}");
}

#[test]
fn ellipse_arcs_with_rotation_roundtrip() {
    // Ellipse mit Hauptachse unter 33° (a = 40, Verhältnis 0,35), Parameter −2 … 5 (über π).
    let (c, a, ratio) = ((150.0, 120.0), 40.0, 0.35);
    let phi = 33f64.to_radians();
    let major = DVec2::new(a * phi.cos(), a * phi.sin());
    let curve = Out2d::Ellipse {
        c: DVec2::new(c.0, c.1),
        major,
        ratio,
        start: -2.0,
        end: 5.0,
    };
    let m = one_view(vec![(Visibility::Visible, curve)]);
    let (s, r) = read_of(&m);
    assert!(
        s.contains(" A 40 14 33 0 1 "),
        "rx, ry, Drehung, large=0, sweep=1: {s}"
    );
    let p = r.visible("Vorne")[0];
    let f = |t: f64| {
        let (u, v) = (a * t.cos(), a * ratio * t.sin());
        (
            c.0 + u * phi.cos() - v * phi.sin(),
            c.1 + u * phi.sin() + v * phi.cos(),
        )
    };
    let formula = poly(&f, -2.0, 5.0, 8_000);
    let dev = two_way_deviation(&[p], &formula, 400);
    assert!(dev < 1e-6, "{dev}");
    // Volle Ellipse: vier Teilbögen, geschlossen.
    let full = Out2d::Ellipse {
        c: DVec2::new(c.0, c.1),
        major,
        ratio,
        start: 0.0,
        end: TAU,
    };
    let (s, r) = read_of(&one_view(vec![(Visibility::Visible, full)]));
    assert_eq!(s.matches(" A ").count(), 4);
    let p = r.visible("Vorne")[0];
    let formula = poly(&f, 0.0, TAU, 8_000);
    assert!(two_way_deviation(&[p], &formula, 400) < 1e-6);
}

#[test]
fn large_coordinates_on_a0_roundtrip() {
    // A0 quer (1189 × 841): Elemente nahe der oberen rechten Ecke.
    let sheet = sheet_of(Paper::A0, true);
    let (c, r0) = ((900.5, 500.25), 300.0);
    let ell_c = (600.123, 400.456);
    let ell_a = 150.0;
    let phi = 17f64.to_radians();
    let ctrl = pts(&[
        (300.0, 300.0),
        (500.5, 780.0),
        (800.0, 350.0),
        (1100.0, 800.25),
        (1150.0, 600.0),
    ]);
    let kn = vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0];
    let spl = spline(3, kn.clone(), &ctrl, vec![1.0; 5]);
    let m = manual(
        sheet,
        vec![(
            "Oben",
            vec![
                (Visibility::Visible, arc(c, r0, 0.0, TAU)),
                (Visibility::Visible, line((11.0, 11.0), (1178.0, 830.0))),
                (
                    Visibility::Hidden,
                    Out2d::Ellipse {
                        c: DVec2::new(ell_c.0, ell_c.1),
                        major: DVec2::new(ell_a * phi.cos(), ell_a * phi.sin()),
                        ratio: 0.5,
                        start: 0.2,
                        end: 4.0,
                    },
                ),
                (Visibility::Visible, spl),
            ],
        )],
    );
    let (s, r) = read_of(&m);
    assert!(s.contains("width=\"1189mm\" height=\"841mm\""));
    assert!(s.contains("matrix(1 0 0 -1 0 841)"));
    let vis = r.visible("Oben");
    let hid = r.hidden("Oben");
    assert_eq!((vis.len(), hid.len()), (1, 1));
    // Kreis: die ersten vier Teilpfade-Stücke 'A' (Vollkreis), dann Linie, dann Spline.
    let kinds = vis[0].kinds();
    assert!(kinds.starts_with("AAAAL"), "{kinds}");
    let circle_pts: Vec<P> = vis[0].segments[..4]
        .iter()
        .flat_map(|sg| (0..=50).map(move |i| sg.point_at(i as f64 / 50.0)))
        .collect();
    for q in circle_pts {
        assert!((dist(q, c) - r0).abs() < 1e-6, "{q:?}");
    }
    let l = &vis[0].segments[4];
    assert!(close(l.start(), (11.0, 11.0), 1e-9) && close(l.end(), (1178.0, 830.0), 1e-9));
    // Spline (Grad 3 → C).
    let sp_segs: Vec<&Seg> = vis[0].segments[5..].iter().collect();
    assert!(sp_segs.iter().all(|sg| sg.kind() == 'C'));
    let f = |u: f64| nurbs_eval(3, &kn, &ctrl, &[1.0; 5], u);
    let formula = poly(&f, 0.0, 1.0, 4_000);
    let dev_spline = read_points(
        &[&ReadPath {
            segments: vis[0].segments[5..].to_vec(),
            ..vis[0].clone()
        }],
        64,
    )
    .iter()
    .map(|&q| dist_poly(q, &formula))
    .fold(0.0, f64::max);
    eprintln!("A0 Spline: {dev_spline:e}");
    assert!(dev_spline < 1e-6);
    // Ellipse (verdeckt): Drehung ist auf 6 Nachkommastellen (Grad) gerundet; bei a = 150 mm
    // wirkt das mit höchstens 150 · 5e-7° ≈ 1,3e-6 mm; deshalb hier die Grenze 3e-6.
    let g = |t: f64| {
        let (u, v) = (ell_a * t.cos(), 0.5 * ell_a * t.sin());
        (
            ell_c.0 + u * phi.cos() - v * phi.sin(),
            ell_c.1 + u * phi.sin() + v * phi.cos(),
        )
    };
    let formula = poly(&g, 0.2, 4.0, 8_000);
    let dev = two_way_deviation(&[hid[0]], &formula, 400);
    eprintln!("A0 Ellipse a=150: {dev:e}");
    assert!(dev < 3e-6, "{dev}");
}

// ---------------------------------------------------------------------------------------------
// Verhalten der Schnittstelle
// ---------------------------------------------------------------------------------------------

#[test]
fn write_matches_string_and_reports_io_errors() {
    let m = manual(
        sheet_of(Paper::A4, false),
        vec![(
            "Vorne",
            vec![(Visibility::Hidden, line((20.0, 20.0), (40.0, 50.0)))],
        )],
    );
    let mut buf: Vec<u8> = Vec::new();
    write_svg(&m, &mut buf).unwrap();
    assert_eq!(String::from_utf8(buf).unwrap(), svg_of(&m));

    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("voll"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(write_svg(&m, &mut Broken), Err(DrawError::Io(_))));
}

#[test]
fn empty_groups_are_not_written() {
    let m = manual(
        sheet_of(Paper::A4, true),
        vec![
            ("Leer", vec![]),
            (
                "Nur verdeckt",
                vec![(Visibility::Hidden, line((20.0, 20.0), (40.0, 50.0)))],
            ),
            (
                "Nur sichtbar",
                vec![(Visibility::Visible, line((20.0, 20.0), (40.0, 50.0)))],
            ),
        ],
    );
    let (s, r) = read_of(&m);
    assert_eq!(r.group_ids, ["Leer", "Nur verdeckt", "Nur sichtbar"]);
    assert_eq!(r.paths_of("Leer").count(), 0);
    assert_eq!(r.hidden("Nur verdeckt").len(), 1);
    assert!(r.visible("Nur verdeckt").is_empty());
    assert_eq!(r.visible("Nur sichtbar").len(), 1);
    assert!(r.hidden("Nur sichtbar").is_empty());
    assert_eq!(s.matches("<path").count(), 2);
}
