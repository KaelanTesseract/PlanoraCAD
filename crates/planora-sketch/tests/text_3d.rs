//! Text in 3D (Skizzen 10, Task 13, Spec Entwurf 4): Der Rahmen liegt in einer eigenen Ebene,
//! die Buchstaben entstehen im 2D-Satz und werden in die Rahmenebene gelegt (3D-Punkte). Ändern und
//! Nachführen bleiben in der Ebene, ein 3D-Text ergibt kein Profil, und „Text auf Pfad“ gibt es nur
//! in 2D.
//!
//! Der Test-Setzer [`BoxShaper`] liefert für jedes Zeichen ein Quadrat mit Loch (Strecken, eine
//! quadratische und eine kubische Bézierkurve), so prüft die Skizze ohne Schriften.

use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;
use planora_sketch::text::{HAlign, TextId, TextKindEdit, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use std::sync::Arc;

const ADV: f64 = 0.8;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

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
                glyphs.push(Glyph {
                    origin: v(pen, 0.0),
                    advance: ADV,
                    contours: if ch.is_whitespace() {
                        Vec::new()
                    } else {
                        box_glyph()
                    },
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
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    s.set_three_d(true);
    s
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

fn shape_of(p: &TextParams) -> Arc<TextShape> {
    Arc::new(BoxShaper.shape(&p.content, &p.style).unwrap())
}

/// Text im Rahmen `a`–`b` der Ebene mit der Normale `n`, links unten.
fn text3(s: &mut Sketch, a: DVec3, b: DVec3, n: DVec3, content: &str) -> TextId {
    let p = params(content);
    let shape = shape_of(&p);
    s.add_text_box3([a, b], n, p, HAlign::Left, VAlign::Bottom, shape)
        .unwrap()
}

fn corners(s: &Sketch, t: TextId) -> [PointId; 4] {
    match s.text(t).unwrap().placement {
        TextPlacement::Box { corners, .. } => corners,
        TextPlacement::Path { .. } => panic!("kein Rahmen"),
    }
}

fn xyz(s: &Sketch, p: PointId) -> DVec3 {
    s.point_xyz(p).unwrap()
}

/// Alle Ecken und erzeugten Punkte des Textes sind 3D und liegen in der Ebene durch Ecke 0 mit
/// der Normale `n`.
fn assert_in_plane(s: &Sketch, t: TextId, n: DVec3) {
    let n = n.normalize();
    let c = corners(s, t);
    let o = xyz(s, c[0]);
    let d = s.text(t).unwrap();
    assert!(!d.points.is_empty());
    for &p in c.iter().chain(&d.points) {
        assert!(s.is_3d_point(p), "3D-Punkt {p:?}");
        let off = (xyz(s, p) - o).dot(n);
        assert!(off.abs() < 1e-9, "außerhalb der Ebene um {off}");
    }
    let stored = d.normal.expect("Normale am Text");
    assert!(
        stored.cross(n).length() < 1e-9,
        "Normale {stored:?} statt {n:?}"
    );
}

#[test]
fn text_box3_curves_lie_in_plane() {
    // Ebene YZ bei x = 10: Grundlinie entlang Y, oben ist Z.
    let mut s = sketch();
    let t = text3(
        &mut s,
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(10.0, 40.0, 10.0),
        DVec3::X,
        "AB",
    );
    assert_in_plane(&s, t, DVec3::X);
    let d = s.text(t).unwrap().clone();
    assert_eq!(d.curves.len(), 16, "zwei Zeichen mit je acht Kurven");
    for &c in &d.curves {
        let kind = &s.curve(c).unwrap().kind;
        assert!(
            matches!(
                kind,
                CurveKind::Line { .. } | CurveKind::Conic { .. } | CurveKind::ControlSpline { .. }
            ),
            "{kind:?}"
        );
        assert!(s.is_3d_curve(c));
    }
    // Dieselbe Lage wie der 2D-Text im Rahmen (0, 0)–(40, 10), mit (x, y) → (10, x, y).
    let mut flat = Sketch::new(Frame::WORLD, "F", Tolerance::default());
    let p = params("AB");
    let shape = shape_of(&p);
    let t2 = flat
        .add_text_box(
            v(0.0, 0.0),
            v(40.0, 10.0),
            p,
            HAlign::Left,
            VAlign::Bottom,
            shape,
        )
        .unwrap();
    let want: Vec<DVec2> = flat
        .text(t2)
        .unwrap()
        .points
        .iter()
        .map(|&p| flat.point_xy(p).unwrap())
        .collect();
    let got: Vec<DVec3> = d.points.iter().map(|&p| xyz(&s, p)).collect();
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(&want) {
        assert!(
            (*g - DVec3::new(10.0, w.x, w.y)).length() < 1e-9,
            "{g:?} statt {w:?}"
        );
    }
    let c = corners(&s, t);
    let at: Vec<DVec3> = c.iter().map(|&p| xyz(&s, p)).collect();
    assert!((at[1] - DVec3::new(10.0, 40.0, 0.0)).length() < 1e-9);
    assert!((at[3] - DVec3::new(10.0, 0.0, 10.0)).length() < 1e-9);

    // Lösen hält alles in der Ebene; der 3D-Text ergibt kein Profil.
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_in_plane(&s, t, DVec3::X);
    assert!(s.profiles().profiles.is_empty(), "kein Profil aus 3D-Text");

    // Schräge Ebene.
    let n = DVec3::new(1.0, 1.0, 1.0);
    let t = text3(
        &mut s,
        DVec3::new(0.0, 0.0, 20.0),
        DVec3::new(30.0, 10.0, 20.0),
        n,
        "C",
    );
    assert_in_plane(&s, t, n);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_in_plane(&s, t, n);

    // In der Skizzenebene bleibt es ein gewöhnlicher 2D-Text.
    let t = text3(
        &mut s,
        DVec3::new(0.0, -50.0, 0.0),
        DVec3::new(30.0, -40.0, 0.0),
        DVec3::Z,
        "D",
    );
    let d = s.text(t).unwrap();
    assert!(d.normal.is_none());
    assert!(d.points.iter().all(|&p| !s.is_3d_point(p)));

    // Rundlauf über die Rohdaten.
    let back = Sketch::from_data(s.to_data()).unwrap();
    assert_eq!(back.to_data(), s.to_data());
}

#[test]
fn text3_edit_refreshes_in_plane() {
    let mut s = sketch();
    let n = DVec3::X;
    let t = text3(
        &mut s,
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(10.0, 40.0, 10.0),
        n,
        "A",
    );
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let before: Vec<CurveId> = s.text(t).unwrap().curves.clone();

    // Mehr Zeichen: die ersten Kurven bleiben, neue kommen in der Ebene dazu.
    let p = params("AAA");
    let shape = shape_of(&p);
    s.edit_text(
        t,
        p,
        TextKindEdit::Box {
            h: HAlign::Center,
            v: VAlign::Middle,
        },
        shape,
    )
    .unwrap();
    let d = s.text(t).unwrap();
    assert_eq!(d.curves.len(), 24);
    assert_eq!(&d.curves[..8], &before[..]);
    assert_in_plane(&s, t, n);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_in_plane(&s, t, n);

    // Den Rahmen in der Ebene verschieben: die Buchstaben folgen beim Lösen.
    let c = corners(&s, t);
    let first = xyz(&s, s.text(t).unwrap().points[0]);
    let c0 = xyz(&s, c[0]);
    s.set_point_xyz(c[0], c0 + DVec3::new(0.0, 5.0, 3.0))
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_in_plane(&s, t, n);
    let moved = xyz(&s, s.text(t).unwrap().points[0]);
    assert!(moved.distance(first) > 1.0, "Buchstaben folgen dem Rahmen");

    // Winkel in der Ebene: um Ecke 0, die Ebene bleibt.
    s.set_text_angle(t, 0.5).unwrap();
    assert!((s.text_angle(t).unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_in_plane(&s, t, n);

    // Neu setzen (wie nach dem Laden) bleibt in der Ebene und ändert nichts.
    let mut loaded = Sketch::from_data(s.to_data()).unwrap();
    let report = loaded.reshape_texts(&BoxShaper);
    assert!(!report.changed, "gleiche Form, gleiche Lage");
    assert_in_plane(&loaded, t, n);

    // Auflösen hält jeden Punkt im Raum fest.
    let points = s.text(t).unwrap().points.clone();
    s.explode_texts(&[t]).unwrap();
    for p in points {
        let at = xyz(&s, p);
        assert!(s.constraints().any(|(_, k)| matches!(
            k.kind,
            planora_sketch::constraint::ConstraintKind::Fixed3(q, x, y, z)
                if q == p && DVec3::new(x, y, z).distance(at) < 1e-9
        )));
    }
}

#[test]
fn text_path_on_3d_curve_rejected() {
    let mut s = sketch();
    let a = s.add_point3(DVec3::new(0.0, 0.0, 0.0)).unwrap();
    let b = s.add_point3(DVec3::new(0.0, 40.0, 10.0)).unwrap();
    let path = s.add_line(a, b).unwrap();
    let p = params("A");
    let shape = shape_of(&p);
    let before = s.to_data();
    let e = s
        .add_text_path(path, p.clone(), true, false, HAlign::Left, shape.clone())
        .unwrap_err();
    assert_eq!(
        e,
        SketchError::InvalidInput(tr!("sketch-err-text-path-3d")),
        "{e}"
    );
    assert_eq!(s.to_data(), before);

    // Ein 2D-Text wechselt nicht auf einen 3D-Pfad, ein 3D-Text gar nicht auf einen Pfad.
    let fa = s.add_point(0.0, -20.0).unwrap();
    let fb = s.add_point(40.0, -20.0).unwrap();
    let flat_path = s.add_line(fa, fb).unwrap();
    let t2 = s
        .add_text_path(
            flat_path,
            p.clone(),
            true,
            false,
            HAlign::Left,
            shape.clone(),
        )
        .unwrap();
    let to = |path| TextKindEdit::Path {
        path,
        above: true,
        fit: false,
        h: HAlign::Left,
    };
    let before = s.to_data();
    let e = s
        .edit_text(t2, p.clone(), to(path), shape.clone())
        .unwrap_err();
    assert_eq!(e, SketchError::InvalidInput(tr!("sketch-err-text-path-3d")));
    let t3 = text3(
        &mut s,
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(10.0, 40.0, 10.0),
        DVec3::X,
        "A",
    );
    let before3 = s.to_data();
    let e = s.edit_text(t3, p, to(flat_path), shape).unwrap_err();
    assert_eq!(e, SketchError::InvalidInput(tr!("sketch-err-text-path-3d")));
    assert_eq!(s.to_data(), before3);
    assert_ne!(before, before3);
}
