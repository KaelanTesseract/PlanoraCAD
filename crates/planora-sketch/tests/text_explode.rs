//! Text auflösen (Skizzen 8, Task 8, Spec F): Die erzeugten Kurven bleiben mit Ids und Punkten
//! als freie Kurven, jeder Punkt bekommt `Fixed`, Rahmen und `TextData` gehen, ein Pfad bleibt.
//!
//! Der Test-Setzer [`BoxShaper`] liefert je Zeichen ein Quadrat mit Loch wie in
//! `text_dependents.rs`; das Zeichen „d“ ist eine entartete Glyphe (Scheitel auf der Sehne,
//! zusammenfallende Kontrollpunkte).

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const ADV: f64 = 0.8;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Quadrat (0, 0)–(0,6, 0,7) mit Loch aus Strecken, quadratischer und kubischer Bézierkurve.
fn box_glyph() -> Vec<Contour> {
    vec![
        Contour {
            start: v(0.0, 0.0),
            segs: vec![
                Seg::Line(v(0.6, 0.0)),
                Seg::Line(v(0.6, 0.7)),
                Seg::Line(v(0.0, 0.7)),
                Seg::Line(v(0.0, 0.0)),
            ],
        },
        Contour {
            start: v(0.2, 0.2),
            segs: vec![
                Seg::Line(v(0.4, 0.2)),
                Seg::Quad(v(0.5, 0.35), v(0.4, 0.5)),
                Seg::Cubic(v(0.35, 0.55), v(0.25, 0.55), v(0.2, 0.5)),
                Seg::Line(v(0.2, 0.2)),
            ],
        },
    ]
}

/// Entartete Glyphe: Scheitel des Kegelschnitts auf der Sehne, Kontrollpunkte des Splines fallen
/// mit ihren Nachbarn zusammen.
fn odd_glyph() -> Vec<Contour> {
    vec![Contour {
        start: v(0.0, 0.0),
        segs: vec![
            Seg::Line(v(0.6, 0.0)),
            Seg::Quad(v(0.6, 0.35), v(0.6, 0.7)),
            Seg::Cubic(v(0.6, 0.7), v(0.0, 0.7), v(0.0, 0.7)),
            Seg::Line(v(0.0, 0.0)),
        ],
    }]
}

struct BoxShaper;

impl TextShaper for BoxShaper {
    fn shape(&self, content: &str, _style: &TextStyle) -> Result<TextShape, TextShapeError> {
        if content.trim().is_empty() {
            return Err(TextShapeError::Empty);
        }
        let mut lines = Vec::new();
        for text in content.split('\n') {
            let mut glyphs = Vec::new();
            let mut pen = 0.0;
            for ch in text.chars() {
                let contours = match ch {
                    c if c.is_whitespace() => Vec::new(),
                    'd' => odd_glyph(),
                    _ => box_glyph(),
                };
                glyphs.push(Glyph {
                    origin: v(pen, 0.0),
                    advance: ADV,
                    contours,
                });
                pen += ADV;
            }
            lines.push(ShapedLine { glyphs, width: pen });
        }
        Ok(TextShape {
            lines,
            ascender: 0.9,
            descender: -0.2,
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

fn params(content: &str) -> TextParams {
    TextParams {
        content: content.into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    }
}

fn shape_of(content: &str) -> Arc<TextShape> {
    Arc::new(BoxShaper.shape(content, &params(content).style).unwrap())
}

/// Text `content` im Rahmen (x0, 0)–(x0 + 40, 10).
fn box_text(s: &mut Sketch, content: &str, x0: f64) -> TextId {
    s.add_text_box(
        v(x0, 0.0),
        v(x0 + 40.0, 10.0),
        params(content),
        HAlign::Center,
        VAlign::Bottom,
        shape_of(content),
    )
    .unwrap()
}

fn line(s: &mut Sketch, a: DVec2, b: DVec2) -> CurveId {
    let p = s.add_point(a.x, a.y).unwrap();
    let q = s.add_point(b.x, b.y).unwrap();
    s.add_line(p, q).unwrap()
}

fn frame(s: &Sketch, t: TextId) -> ([CurveId; 4], [PointId; 4]) {
    match s.text(t).unwrap().placement {
        TextPlacement::Box { lines, corners, .. } => (lines, corners),
        TextPlacement::Path { .. } => panic!("kein Rahmen"),
    }
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

/// Kurven, Art je Kurve und Lage jedes Punktes eines Textes.
struct Snapshot {
    curves: Vec<CurveId>,
    kinds: Vec<CurveKind>,
    points: Vec<PointId>,
    xy: Vec<DVec2>,
}

fn snapshot(s: &Sketch, t: TextId) -> Snapshot {
    let d = s.text(t).unwrap();
    Snapshot {
        curves: d.curves.clone(),
        kinds: d
            .curves
            .iter()
            .map(|&c| s.curve(c).unwrap().kind.clone())
            .collect(),
        points: d.points.clone(),
        xy: d.points.iter().map(|&p| at(s, p)).collect(),
    }
}

/// Die Kurven aus `snap` stehen unverändert als freie Kurven da.
fn kept(s: &Sketch, snap: &Snapshot) {
    for (c, kind) in snap.curves.iter().zip(&snap.kinds) {
        let d = s.curve(*c).expect("Kurve bleibt");
        assert_eq!(&d.kind, kind, "Art und Punkte bleiben");
        assert!(!d.construction);
        assert_eq!(s.text_of_curve(*c), None, "keine Textkurve mehr");
    }
    for (p, xy) in snap.points.iter().zip(&snap.xy) {
        assert!(
            at(s, *p).distance(*xy) <= 1e-12,
            "Punkt bleibt an seiner Stelle"
        );
    }
}

/// `Fixed` je Punkt des Benutzers, mit der Lage.
fn fixed(s: &Sketch) -> HashMap<PointId, DVec2> {
    s.constraints()
        .filter_map(|(_, k)| match k.kind {
            K::Fixed(p, x, y) => Some((p, v(x, y))),
            _ => None,
        })
        .collect()
}

fn solved(s: &mut Sketch) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
}

fn unchanged(s: &Sketch, before: &Sketch, what: &str) {
    assert!(s.to_data() == before.to_data(), "{what}: Skizze verändert");
    assert_eq!(s.revision(), before.revision(), "{what}: Stand verändert");
}

// ---- Tests --------------------------------------------------------------------------------------

#[test]
fn explode_keeps_curve_ids_and_points() {
    let mut s = sketch();
    let t = box_text(&mut s, "ab", 0.0);
    let snap = snapshot(&s, t);
    let rev = s.revision();
    let out = s.explode_texts(&[t]).unwrap();
    assert_eq!(
        out, snap.curves,
        "die erzeugten Kurven in ihrer Reihenfolge"
    );
    assert_ne!(s.revision(), rev, "neuer Stand");
    kept(&s, &snap);
    assert!(s.glyph_curves().is_empty());
    assert!(s.glyph_points().is_empty());
    // Nach dem Lösen ist alles noch da, wo es war.
    solved(&mut s);
    kept(&s, &snap);
}

#[test]
fn explode_fixes_every_point() {
    let mut s = sketch();
    let t = box_text(&mut s, "ad", 0.0);
    let snap = snapshot(&s, t);
    assert!(
        snap.kinds
            .iter()
            .any(|k| matches!(k, CurveKind::Conic { .. }))
            && snap
                .kinds
                .iter()
                .any(|k| matches!(k, CurveKind::ControlSpline { .. })),
        "Kegelschnitt und Spline im Text"
    );
    s.explode_texts(&[t]).unwrap();
    let fixed = fixed(&s);
    // Jeder Punkt jeder Kurve, auch Scheitel und Kontrollpunkte, genau einmal gehalten.
    let all: HashSet<PointId> = snap.kinds.iter().flat_map(|k| k.points()).collect();
    assert_eq!(all, snap.points.iter().copied().collect::<HashSet<_>>());
    assert_eq!(fixed.len(), all.len());
    let count = s
        .constraints()
        .filter(|(_, k)| matches!(k.kind, K::Fixed(..)))
        .count();
    assert_eq!(count, all.len(), "je Punkt ein Fixed");
    for p in &all {
        let xy = fixed.get(p).expect("Punkt gehalten");
        assert!(xy.distance(at(&s, *p)) <= 1e-12, "an der aktuellen Stelle");
    }
    // Einzeln lösbar: Ohne sein Fixed lässt sich ein Punkt setzen.
    let p = snap.points[0];
    let k = s
        .constraints()
        .find(|(_, k)| matches!(k.kind, K::Fixed(q, ..) if q == p))
        .map(|(k, _)| k)
        .unwrap();
    s.remove_constraint(k).unwrap();
    s.set_point_xy(p, 30.0, 30.0).unwrap();
    assert_eq!(at(&s, p), v(30.0, 30.0));
    solved(&mut s);
}

#[test]
fn explode_removes_frame_and_text() {
    let mut s = sketch();
    let t = box_text(&mut s, "a", 0.0);
    let (lines, corners) = frame(&s, t);
    // Ein Maß am Rahmen und eine Linie, die an Ecke 2 hängt.
    s.add_constraint(K::Distance(corners[0], corners[1], 40.0))
        .unwrap();
    let other_end = s.add_point(60.0, 30.0).unwrap();
    let other = s.add_line(corners[2], other_end).unwrap();
    let gap = at(&s, corners[2]).distance(at(&s, other_end));
    let held = s
        .add_constraint(K::Distance(corners[2], other_end, gap))
        .unwrap();
    s.explode_texts(&[t]).unwrap();
    assert!(s.text(t).is_none());
    assert_eq!(s.texts().count(), 0);
    for l in lines {
        assert!(s.curve(l).is_none(), "Rahmenlinie gelöscht");
    }
    for &p in &corners[..2] {
        assert!(s.point_xy(p).is_err(), "Ecke gelöscht");
    }
    assert!(s.point_xy(corners[3]).is_err(), "Ecke gelöscht");
    // Die Ecke an der anderen Linie bleibt als gewöhnlicher Punkt, mit ihrer Abhängigkeit.
    assert!(s.point_xy(corners[2]).is_ok());
    assert!(s.curve(other).is_some());
    assert!(s.constraint(held).is_some());
    // Keine Abhängigkeit (auch keine interne) mehr an Rahmenlinien oder gelöschten Ecken.
    let gone: HashSet<PointId> = [corners[0], corners[1], corners[3]].into();
    for (_, k) in s.constraints().chain(s.internal_constraints()) {
        assert!(!k.kind.curves().iter().any(|c| lines.contains(c)), "{k:?}");
        assert!(!k.kind.points().iter().any(|p| gone.contains(p)), "{k:?}");
    }
    solved(&mut s);
}

#[test]
fn explode_keeps_path() {
    let mut s = sketch();
    let path = line(&mut s, v(0.0, 0.0), v(50.0, 0.0));
    let h = s
        .add_constraint(K::Horizontal(LineOrPoints::Line(path)))
        .unwrap();
    let t = s
        .add_text_path(
            path,
            params("ab"),
            true,
            false,
            HAlign::Left,
            shape_of("ab"),
        )
        .unwrap();
    let snap = snapshot(&s, t);
    let path_kind = s.curve(path).unwrap().kind.clone();
    assert_eq!(s.explode_texts(&[t]).unwrap(), snap.curves);
    assert!(s.text(t).is_none());
    assert_eq!(s.curve(path).unwrap().kind, path_kind, "Pfad bleibt");
    assert!(s.constraint(h).is_some(), "Abhängigkeit am Pfad bleibt");
    kept(&s, &snap);
    // Der Pfad trägt keinen Text mehr: Er folgt nicht mehr und lässt sich löschen.
    let CurveKind::Line { b, .. } = path_kind else {
        unreachable!()
    };
    s.set_point_xy(b, 80.0, 0.0).unwrap();
    solved(&mut s);
    kept(&s, &snap);
    s.remove_curve(path).unwrap();
}

#[test]
fn exploded_curves_are_editable() {
    let mut s = sketch();
    let t = box_text(&mut s, "a", 0.0);
    let snap = snapshot(&s, t);
    s.explode_texts(&[t]).unwrap();
    // Die untere Kante des Quadrats, von einer senkrechten Linie gekreuzt.
    let (g, a, b) = snap
        .kinds
        .iter()
        .zip(&snap.curves)
        .find_map(|(k, &c)| match *k {
            CurveKind::Line { a, b } if (at(&s, a).y - at(&s, b).y).abs() <= 1e-9 => {
                Some((c, at(&s, a), at(&s, b)))
            }
            _ => None,
        })
        .unwrap();
    let mid = (a + b) * 0.5;
    line(&mut s, mid - v(0.0, 1.0), mid + v(0.0, 1.0));
    let left = a.lerp(mid, 0.5);
    s.trim(g, left).unwrap();
    // Maße an früheren Buchstaben gehen.
    let p = snap.points[0];
    let q = s.add_point(-5.0, -5.0).unwrap();
    s.add_constraint(K::Coincident(p, q)).unwrap();
    // Konstruktion schaltet die Kurve einzeln um.
    let other = snap.curves[1];
    s.set_construction(other, true).unwrap();
    assert!(s.curve(other).unwrap().construction);
    assert!(!s.curve(snap.curves[2]).unwrap().construction);
    // Einzeln löschen geht.
    s.remove_curve(snap.curves[2]).unwrap();
}

#[test]
fn profiles_same_before_and_after_explode() {
    let mut s = sketch();
    let t = box_text(&mut s, "ad", 0.0);
    let before = s.profiles();
    assert!(!before.refs.is_empty());
    s.explode_texts(&[t]).unwrap();
    let after = s.profiles();
    assert_eq!(after.refs, before.refs);
    assert_eq!(after.profiles.len(), before.profiles.len());
}

#[test]
fn explode_two_texts_at_once() {
    let mut s = sketch();
    let t1 = box_text(&mut s, "ab", 0.0);
    let t2 = box_text(&mut s, "d", 100.0);
    let keep = box_text(&mut s, "c", 200.0);
    let (a, b) = (snapshot(&s, t1), snapshot(&s, t2));
    let out = s.explode_texts(&[t2, t1]).unwrap();
    let mut want = b.curves.clone();
    want.extend(&a.curves);
    assert_eq!(out, want, "in der Reihenfolge der Texte");
    kept(&s, &a);
    kept(&s, &b);
    assert_eq!(
        s.texts().map(|(t, _)| t).collect::<Vec<_>>(),
        vec![keep],
        "der dritte Text bleibt"
    );
    assert_eq!(fixed(&s).len(), a.points.len() + b.points.len());
    solved(&mut s);
}

#[test]
fn explode_is_atomic() {
    let mut s = sketch();
    let t1 = box_text(&mut s, "ab", 0.0);
    let t2 = box_text(&mut s, "a", 100.0);
    s.remove_text(t2).unwrap();
    let before = s.clone();
    // Ein gelöschter Text: Fehler, auch der erste bleibt Text.
    assert_eq!(
        s.explode_texts(&[t1, t2]).err(),
        Some(SketchError::UnknownObject)
    );
    unchanged(&s, &before, "gelöschter Text");
    // Nichts gewählt: nichts geschieht.
    assert_eq!(s.explode_texts(&[]).unwrap(), Vec::new());
    unchanged(&s, &before, "leer");
    // Doppelt genannt: einmal aufgelöst.
    let snap = snapshot(&s, t1);
    assert_eq!(s.explode_texts(&[t1, t1]).unwrap(), snap.curves);
    assert_eq!(fixed(&s).len(), snap.points.len());
}

#[test]
fn exploded_sketch_round_trips() {
    let mut s = sketch();
    let t1 = box_text(&mut s, "ad", 0.0);
    let path = line(&mut s, v(0.0, -20.0), v(50.0, -20.0));
    let t2 = s
        .add_text_path(
            path,
            params("ab"),
            false,
            false,
            HAlign::Left,
            shape_of("ab"),
        )
        .unwrap();
    s.explode_texts(&[t1, t2]).unwrap();
    solved(&mut s);
    let d = s.to_data();
    assert_eq!(d.texts.len(), 2, "gelöschte Texte bleiben in der Datei");
    assert!(d.texts.iter().all(|t| !t.alive));
    let back = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(back.to_data(), d);
    assert_eq!(back.texts().count(), 0);
    let mut back = back;
    solved(&mut back);
    assert_eq!(back.to_data().vars, d.vars, "Lage bleibt nach dem Lösen");
}
