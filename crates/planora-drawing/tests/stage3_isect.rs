//! Stufe 3: 2D-Schnitte der Striche und Teilpunkte.

mod common;
use common::model::*;
use planora_drawing::{DrawError, basis::Basis, isect::*, model::ViewKind, stroke::*};
use planora_exchange::Progress;
use planora_kernel::Curve;
use planora_math::{DVec3, Frame};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

fn line(o: (f64, f64, f64), d: (f64, f64, f64), t1: f64, i: usize) -> Stroke {
    Stroke {
        curve: Curve::Line {
            origin: DVec3::new(o.0, o.1, o.2),
            dir: DVec3::new(d.0, d.1, d.2).normalize(),
        },
        t0: 0.0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: i },
    }
}

fn circle(center: (f64, f64, f64), r: f64, t0: f64, t1: f64, i: usize) -> Stroke {
    Stroke {
        curve: Curve::Circle {
            frame: Frame {
                origin: DVec3::new(center.0, center.1, center.2),
                ..Frame::WORLD
            },
            radius: r,
        },
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: i },
    }
}

#[test]
fn two_crossing_lines_split_both() {
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 1.0), 20.0, 0);
    let b = line((10.0, 0.0, 0.0), (-1.0, 0.0, 1.0), 20.0, 1);
    let basis = Basis::of(ViewKind::Front);
    let c = stroke_cross(&a, &b, &basis, 30.0).unwrap();
    assert_eq!(c.len(), 1);
    assert!((c[0].p.x - 5.0).abs() < 1e-9 && (c[0].p.y - 5.0).abs() < 1e-9);
    assert!(c[0].transversal);
}

#[test]
fn lines_with_different_depth_still_cross_in_2d() {
    // y-Tiefe verschieden, 2D-Kreuzung bleibt
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 1.0), 20.0, 0);
    let b = line((10.0, 7.0, 0.0), (-1.0, 0.0, 1.0), 20.0, 1);
    assert_eq!(
        stroke_cross(&a, &b, &Basis::of(ViewKind::Front), 30.0)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn line_through_circle_crosses_twice() {
    let circ = Stroke {
        curve: Curve::Circle {
            frame: Frame::WORLD,
            radius: 5.0,
        },
        t0: 0.0,
        t1: TAU,
        origin: StrokeOrigin::Edge { body: 0, edge: 2 },
    };
    // Kreis liegt in der XY-Ebene; von vorne (x, z) ist er die Strecke x ∈ [-5, 5] bei z = 0
    let a = line((-10.0, 0.0, -3.0), (1.0, 0.0, 0.0), 20.0, 3); // parallel, keine Kreuzung
    assert!(
        stroke_cross(&a, &circ, &Basis::of(ViewKind::Front), 20.0)
            .unwrap()
            .is_empty()
    );
    // von oben (x, y) ist der Kreis ein Kreis; eine Gerade durch die Mitte kreuzt zweimal
    let l = line((-10.0, 0.0, 0.0), (1.0, 0.0, 0.0), 20.0, 4);
    let c = stroke_cross(&l, &circ, &Basis::of(ViewKind::Top), 20.0).unwrap();
    assert_eq!(c.len(), 2);
}

#[test]
fn tangent_line_to_circle_does_not_split() {
    let circ = Stroke {
        curve: Curve::Circle {
            frame: Frame::WORLD,
            radius: 5.0,
        },
        t0: 0.0,
        t1: TAU,
        origin: StrokeOrigin::Edge { body: 0, edge: 5 },
    };
    let l = line((-10.0, 5.0, 0.0), (1.0, 0.0, 0.0), 20.0, 6);
    let basis = Basis::of(ViewKind::Top);
    let strokes = vec![l, circ];
    let sp = split_params(&strokes, &basis, 20.0, &Progress::new()).unwrap();
    assert!(sp[0].is_empty() && sp[1].is_empty(), "{sp:?}");
}

#[test]
fn endpoint_on_other_stroke_splits_it() {
    // T-Anschluss: Gerade b endet mitten auf a
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 20.0, 0);
    let b = line((8.0, 0.0, 0.0), (0.0, 0.0, 1.0), 10.0, 1);
    let sp = split_params(
        &[a.clone(), b],
        &Basis::of(ViewKind::Front),
        20.0,
        &Progress::new(),
    )
    .unwrap();
    assert_eq!(sp[0].len(), 1);
    assert!((sp[0][0] - 8.0).abs() < 1e-9);
    assert!(sp[1].is_empty());
    // Zusatz: Viertelbogen (Mitte (8, 5) in x/z, r = 5), der in (8, 0) tangential auf a endet
    // (wie eine Rundung an einer längeren Kante). Dort ist keine transversale Kreuzung – a wird
    // nur über den Endpunkt geteilt.
    let arc = Stroke {
        curve: Curve::Circle {
            frame: Frame {
                origin: DVec3::new(8.0, 0.0, 5.0),
                x: DVec3::X,
                y: DVec3::Z,
                z: -DVec3::Y,
            },
            radius: 5.0,
        },
        t0: -FRAC_PI_2,
        t1: 0.0,
        origin: StrokeOrigin::Edge { body: 0, edge: 2 },
    };
    let basis = Basis::of(ViewKind::Front);
    assert!(
        stroke_cross(&a, &arc, &basis, 20.0)
            .unwrap()
            .iter()
            .all(|c| !c.transversal)
    );
    let sp = split_params(&[a, arc], &basis, 20.0, &Progress::new()).unwrap();
    assert_eq!(sp[0].len(), 1, "{sp:?}");
    assert!((sp[0][0] - 8.0).abs() < 1e-9, "{sp:?}");
    assert!(sp[1].is_empty(), "{sp:?}");
}

#[test]
fn ellipse_ellipse_crossings_match_brute_force() {
    // Zwei verdrehte Kreise, von schräg (Isometrisch) projiziert: Ellipsen. Gegenprüfung: dichte
    // Abtastung zählt die Schnitte der Polygonzüge.
    let c1 = Stroke {
        curve: Curve::Circle {
            frame: Frame::WORLD,
            radius: 6.0,
        },
        t0: 0.0,
        t1: TAU,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    };
    let f2 = Frame {
        origin: DVec3::new(4.0, 0.0, 0.0),
        ..Frame::WORLD
    };
    let c2 = Stroke {
        curve: Curve::Circle {
            frame: f2,
            radius: 6.0,
        },
        t0: 0.0,
        t1: TAU,
        origin: StrokeOrigin::Edge { body: 0, edge: 1 },
    };
    let basis = Basis::of(ViewKind::Isometric);
    let got = stroke_cross(&c1, &c2, &basis, 20.0)
        .unwrap()
        .iter()
        .filter(|c| c.transversal)
        .count();
    let want = brute_force_cross_count(&c1, &c2, &basis, 20_000); // in tests/common/model.rs
    assert_eq!(got, want);
}

#[test]
fn spline_crossing_matches_brute_force() {
    // Kubische Kurve durch (0,0,0), (5,0,12), (10,0,-4), (20,0,10) (Kontrollpunkte, offener
    // Knotenvektor, Grad 3) gegen die Gerade z = 3 (x von -5 bis 25)
    let curve = Curve::nurbs(cubic_through_controls(&[
        (0.0, 0.0, 0.0),
        (5.0, 0.0, 12.0),
        (10.0, 0.0, -4.0),
        (20.0, 0.0, 10.0),
    ]));
    let (t0, t1) = curve.domain().unwrap();
    let a = Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 7 },
    };
    let b = line((-5.0, 0.0, 3.0), (1.0, 0.0, 0.0), 30.0, 8);
    let basis = Basis::of(ViewKind::Front);
    let got = stroke_cross(&a, &b, &basis, 30.0)
        .unwrap()
        .iter()
        .filter(|c| c.transversal)
        .count();
    assert_eq!(got, brute_force_cross_count(&a, &b, &basis, 20_000));
    assert!(got >= 2);
}

#[test]
fn cost_limit_reports_too_complex() {
    // Mehr als MAX_STROKES (20 000) Striche → TooComplex.
    let strokes: Vec<Stroke> = (0..25_001)
        .map(|i| line((i as f64, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, i))
        .collect();
    let err = split_params(
        &strokes,
        &Basis::of(ViewKind::Front),
        30_000.0,
        &Progress::new(),
    )
    .unwrap_err();
    assert!(matches!(err, DrawError::TooComplex(_)));
}

// ---- Zusätzliche Fälle: Naht periodischer Parameter, Überdeckung, Entartung, Abbruch ----

/// Gerade durch (5, 0) und (0, 5) (von oben gesehen): trifft den Kreis r = 5 um den Ursprung
/// genau an seiner Naht (t = 0 ≡ 2π) und bei t = π/2.
fn seam_line() -> Stroke {
    line((10.0, -5.0, 0.0), (-1.0, 1.0, 0.0), 20.0, 9)
}

#[test]
fn full_circle_seam_crossing_reported_once() {
    let circ = circle((0.0, 0.0, 0.0), 5.0, 0.0, TAU, 10);
    let basis = Basis::of(ViewKind::Top);
    for (a, b, swapped) in [
        (seam_line(), circ.clone(), false),
        (circ, seam_line(), true),
    ] {
        let c = stroke_cross(&a, &b, &basis, 20.0).unwrap();
        assert_eq!(c.len(), 2, "{c:?}");
        assert!(c.iter().all(|c| c.transversal));
        let at_seam = c
            .iter()
            .filter(|c| (c.p.x - 5.0).abs() < 1e-9 && c.p.y.abs() < 1e-9)
            .count();
        assert_eq!(at_seam, 1, "{c:?}");
        for x in &c {
            let tc = if swapped { x.ta } else { x.tb };
            let tl = if swapped { x.tb } else { x.ta };
            assert!((0.0..=TAU).contains(&tc), "{tc}");
            assert!((0.0..=20.0).contains(&tl), "{tl}");
        }
    }
}

#[test]
fn arc_across_seam_is_found_by_line_and_by_circle() {
    // Bogen von t = 5 bis t = 7 (über 2π hinweg); der Nahtpunkt (5, 0) liegt bei t = 2π.
    let arc = circle((0.0, 0.0, 0.0), 5.0, 5.0, 7.0, 11);
    let basis = Basis::of(ViewKind::Top);
    let c = stroke_cross(&seam_line(), &arc, &basis, 20.0).unwrap();
    assert_eq!(c.len(), 1, "{c:?}");
    assert!((c[0].tb - TAU).abs() < 1e-9, "{c:?}");
    assert!((c[0].p.x - 5.0).abs() < 1e-9 && c[0].p.y.abs() < 1e-9);
    // Kreis um (5, -5) mit r = 5 geht durch (5, 0) (dort waagrecht) und (0, -5).
    let other = circle((5.0, -5.0, 0.0), 5.0, 0.0, TAU, 12);
    let c = stroke_cross(&arc, &other, &basis, 20.0).unwrap();
    assert_eq!(c.len(), 1, "{c:?}");
    assert!((c[0].ta - TAU).abs() < 1e-9, "{c:?}");
    assert!((c[0].tb - FRAC_PI_2).abs() < 1e-9, "{c:?}");
    assert!(c[0].transversal);
}

#[test]
fn circle_circle_crossing_at_seam_reported_once() {
    let c1 = circle((0.0, 0.0, 0.0), 5.0, 0.0, TAU, 13);
    let c2 = circle((5.0, -5.0, 0.0), 5.0, 0.0, TAU, 14);
    let basis = Basis::of(ViewKind::Top);
    let c = stroke_cross(&c1, &c2, &basis, 20.0).unwrap();
    assert_eq!(c.len(), 2, "{c:?}");
    assert!(c.iter().all(|c| c.transversal));
    let mut pts: Vec<(f64, f64)> = c.iter().map(|c| (c.p.x, c.p.y)).collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(
        pts[0].0.abs() < 1e-9 && (pts[0].1 + 5.0).abs() < 1e-9,
        "{pts:?}"
    );
    assert!(
        (pts[1].0 - 5.0).abs() < 1e-9 && pts[1].1.abs() < 1e-9,
        "{pts:?}"
    );
    assert_eq!(brute_force_cross_count(&c1, &c2, &basis, 20_000), 2);
    // Teilpunkte: c1 nur bei 3π/2 (die Naht ist Bereichsrand), c2 bei π/2 und π.
    let sp = split_params(&[c1, c2], &basis, 20.0, &Progress::new()).unwrap();
    assert_eq!(sp[0].len(), 1, "{sp:?}");
    assert!((sp[0][0] - 1.5 * PI).abs() < 1e-9, "{sp:?}");
    assert_eq!(sp[1].len(), 2, "{sp:?}");
    assert!((sp[1][0] - FRAC_PI_2).abs() < 1e-9 && (sp[1][1] - PI).abs() < 1e-9);
}

#[test]
fn closed_nurbs_seam_crossing_reported_once() {
    let curve = Curve::nurbs(closed_nurbs_loop());
    assert!(curve.period().is_some(), "Prüfkurve muss geschlossen sein");
    let (t0, t1) = curve.domain().unwrap();
    let a = Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 15 },
    };
    // Waagrechte Gerade y = 0, x von 0 bis 20: trifft die Kurve nur in ihrem Nahtpunkt (10, 0).
    let b = line((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 20.0, 16);
    let basis = Basis::of(ViewKind::Top);
    let c = stroke_cross(&a, &b, &basis, 20.0).unwrap();
    assert_eq!(c.len(), 1, "{c:?}");
    assert!(c[0].transversal);
    assert!(
        (c[0].p.x - 10.0).abs() < 1e-9 && c[0].p.y.abs() < 1e-9,
        "{c:?}"
    );
    assert!((c[0].tb - 10.0).abs() < 1e-9);
    let sp = split_params(&[a, b], &basis, 20.0, &Progress::new()).unwrap();
    assert!(sp[0].is_empty(), "{sp:?}");
    assert_eq!(sp[1].len(), 1, "{sp:?}");
    assert!((sp[1][0] - 10.0).abs() < 1e-9);
}

#[test]
fn closed_nurbs_crossings_match_brute_force() {
    let curve = Curve::nurbs(closed_nurbs_loop());
    let (t0, t1) = curve.domain().unwrap();
    let a = Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 17 },
    };
    // Schräge Gerade quer durch die Schleife, nicht durch die Naht.
    let b = line((-20.0, -3.0, 0.0), (1.0, 0.3, 0.0), 45.0, 18);
    let basis = Basis::of(ViewKind::Top);
    let got = stroke_cross(&a, &b, &basis, 20.0)
        .unwrap()
        .iter()
        .filter(|c| c.transversal)
        .count();
    assert_eq!(got, brute_force_cross_count(&a, &b, &basis, 20_000));
    assert!(got >= 2);
}

#[test]
fn parallel_and_collinear_lines_have_no_roots_overlap_split_by_endpoints() {
    let basis = Basis::of(ViewKind::Front);
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 20.0, 0);
    let par = line((0.0, 0.0, 1.0), (1.0, 0.0, 0.0), 20.0, 1);
    assert!(stroke_cross(&a, &par, &basis, 30.0).unwrap().is_empty());
    // kollinear, überlappend auf x ∈ [10, 20] (b läuft von x = 10 bis 30)
    let b = line((10.0, 5.0, 0.0), (1.0, 0.0, 0.0), 20.0, 2);
    assert!(stroke_cross(&a, &b, &basis, 30.0).unwrap().is_empty());
    let sp = split_params(&[a, b], &basis, 30.0, &Progress::new()).unwrap();
    assert_eq!(sp[0].len(), 1, "{sp:?}");
    assert!((sp[0][0] - 10.0).abs() < 1e-9);
    assert_eq!(sp[1].len(), 1, "{sp:?}");
    assert!((sp[1][0] - 10.0).abs() < 1e-9);
}

#[test]
fn overlapping_arcs_of_one_circle_split_by_endpoints_only() {
    let basis = Basis::of(ViewKind::Top);
    let a = circle((0.0, 0.0, 0.0), 5.0, 0.0, PI, 0);
    let b = circle((0.0, 0.0, 0.0), 5.0, FRAC_PI_2, 1.5 * PI, 1);
    let c = stroke_cross(&a, &b, &basis, 20.0).unwrap();
    assert!(c.iter().all(|c| !c.transversal), "{c:?}");
    let sp = split_params(&[a, b], &basis, 20.0, &Progress::new()).unwrap();
    assert_eq!(sp[0].len(), 1, "{sp:?}");
    assert!((sp[0][0] - FRAC_PI_2).abs() < 1e-9);
    assert_eq!(sp[1].len(), 1, "{sp:?}");
    assert!((sp[1][0] - PI).abs() < 1e-9);
    // zwei deckungsgleiche Vollkreise: keine Teilung
    let f1 = circle((0.0, 0.0, 0.0), 5.0, 0.0, TAU, 2);
    let f2 = circle((0.0, 0.0, 3.0), 5.0, 0.0, TAU, 3);
    let sp = split_params(&[f1, f2], &basis, 20.0, &Progress::new()).unwrap();
    assert!(sp[0].is_empty() && sp[1].is_empty(), "{sp:?}");
}

#[test]
fn edge_on_circle_is_crossed_twice_at_one_point() {
    // Von vorne ist der Kreis die doppelt durchlaufene Strecke x ∈ [-5, 5]; die senkrechte
    // Gerade x = 3 trifft beide Durchläufe (t = ±acos(3/5)).
    let circ = circle((0.0, 0.0, 0.0), 5.0, 0.0, TAU, 0);
    let l = line((3.0, 0.0, -4.0), (0.0, 0.0, 1.0), 8.0, 1);
    let basis = Basis::of(ViewKind::Front);
    let c = stroke_cross(&l, &circ, &basis, 20.0).unwrap();
    assert_eq!(c.len(), 2, "{c:?}");
    let mut tb: Vec<f64> = c.iter().map(|c| c.tb).collect();
    tb.sort_by(f64::total_cmp);
    let t = (0.6f64).acos();
    assert!(
        (tb[0] - t).abs() < 1e-9 && (tb[1] - (TAU - t)).abs() < 1e-9,
        "{tb:?}"
    );
}

#[test]
fn degenerate_projection_has_no_roots() {
    // Gerade längs der Blickrichtung (Front: y) fällt zu einem Punkt zusammen.
    let basis = Basis::of(ViewKind::Front);
    let dot = line((5.0, 0.0, 0.0), (0.0, 1.0, 0.0), 10.0, 0);
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 20.0, 1);
    let circ = circle((0.0, 0.0, 0.0), 5.0, 0.0, TAU, 2);
    assert!(stroke_cross(&dot, &a, &basis, 20.0).unwrap().is_empty());
    assert!(stroke_cross(&a, &dot, &basis, 20.0).unwrap().is_empty());
    assert!(stroke_cross(&dot, &circ, &basis, 20.0).unwrap().is_empty());
}

#[test]
fn cancelled_progress_stops_split_params() {
    let p = Progress::new();
    p.cancel();
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 1.0), 20.0, 0);
    let b = line((10.0, 0.0, 0.0), (-1.0, 0.0, 1.0), 20.0, 1);
    let err = split_params(&[a, b], &Basis::of(ViewKind::Front), 30.0, &p).unwrap_err();
    assert!(matches!(err, DrawError::Cancelled));
}

#[test]
fn split_params_are_sorted_unique_and_interior() {
    // Gitter aus 5 waagrechten und 5 senkrechten Geraden, alle kreuzen sich; dazu ein Kreis.
    let mut strokes = Vec::new();
    for k in 0..5 {
        let v = 2.0 + 4.0 * k as f64;
        strokes.push(line((0.0, 0.0, v), (1.0, 0.0, 0.0), 20.0, k));
        strokes.push(line((v, 0.0, 0.0), (0.0, 0.0, 1.0), 20.0, 10 + k));
    }
    strokes.push(Stroke {
        curve: Curve::Circle {
            frame: Frame {
                origin: DVec3::new(10.0, 0.0, 10.0),
                x: DVec3::X,
                y: DVec3::Z,
                z: -DVec3::Y,
            },
            radius: 7.0,
        },
        t0: 0.0,
        t1: TAU,
        origin: StrokeOrigin::Edge { body: 0, edge: 99 },
    });
    let basis = Basis::of(ViewKind::Front);
    let sp = split_params(&strokes, &basis, 20.0, &Progress::new()).unwrap();
    for (s, ts) in strokes.iter().zip(&sp) {
        let w = (s.t1 - s.t0).abs();
        for pair in ts.windows(2) {
            assert!(pair[1] - pair[0] >= 1e-9 * w, "{ts:?}");
        }
        for &t in ts {
            assert!(t > s.t0 + 1e-9 * w && t < s.t1 - 1e-9 * w, "{ts:?}");
        }
    }
    // Jede Gerade wird von den 5 Geraden der anderen Richtung gekreuzt; der Kreis (r = 7 um
    // (10, 10)) trifft die Geraden bei 6, 10, 14 je zweimal (die bei 2 und 18 liegen 8 entfernt):
    // 12 Kreuzungen, davon eine genau in seiner Naht (17, 10) – die ist Bereichsrand: 11.
    for (k, ts) in sp[..10].iter().enumerate() {
        let v = 2.0 + 4.0 * (k / 2) as f64;
        let want = if (v - 10.0).abs() < 5.0 { 7 } else { 5 };
        assert_eq!(ts.len(), want, "{k}: {ts:?}");
    }
    assert_eq!(sp[10].len(), 11, "{:?}", sp[10]);
}

#[test]
fn spline_with_literal_control_points_matches_brute_force() {
    // Dieselben vier Punkte als Kontrollpunkte (nicht als Durchgangspunkte): die Kurve sinkt
    // nach dem ersten Anstieg nur bis z ≈ 3,89 und kreuzt z = 3 einmal.
    let curve = Curve::nurbs(cubic_from_controls(&[
        (0.0, 0.0, 0.0),
        (5.0, 0.0, 12.0),
        (10.0, 0.0, -4.0),
        (20.0, 0.0, 10.0),
    ]));
    let (t0, t1) = curve.domain().unwrap();
    let a = Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 19 },
    };
    let b = line((-5.0, 0.0, 3.0), (1.0, 0.0, 0.0), 30.0, 20);
    let basis = Basis::of(ViewKind::Front);
    let got = stroke_cross(&a, &b, &basis, 30.0)
        .unwrap()
        .iter()
        .filter(|c| c.transversal)
        .count();
    assert_eq!(got, brute_force_cross_count(&a, &b, &basis, 20_000));
    assert_eq!(got, 1);
}

#[test]
fn tangent_circles_do_not_split_also_at_seam() {
    let basis = Basis::of(ViewKind::Top);
    let c1 = circle((0.0, 0.0, 0.0), 5.0, 0.0, TAU, 0);
    // außen berührend in (5, 0) = Naht von c1, und in (0, 5) mitten auf c1
    let diag = 50f64.sqrt();
    for center in [(10.0, 0.0, 0.0), (0.0, 10.0, 0.0), (diag, diag, 0.0)] {
        let c2 = circle(center, 5.0, 0.0, TAU, 1);
        let c = stroke_cross(&c1, &c2, &basis, 20.0).unwrap();
        assert!(c.iter().all(|c| !c.transversal), "{center:?}: {c:?}");
        let sp = split_params(&[c1.clone(), c2], &basis, 20.0, &Progress::new()).unwrap();
        assert!(sp[0].is_empty() && sp[1].is_empty(), "{center:?}: {sp:?}");
    }
}

#[test]
fn line_tangent_to_spline_does_not_split() {
    // Bézierkurve mit Kontrollpunkten (0,0), (5,8), (15,8), (20,0) (x, z): höchster Punkt bei
    // u = 1/2 mit z = 3/4·8 = 6, dort waagrecht. Die Gerade z = 6 berührt sie nur.
    let curve = Curve::nurbs(cubic_from_controls(&[
        (0.0, 0.0, 0.0),
        (5.0, 0.0, 8.0),
        (15.0, 0.0, 8.0),
        (20.0, 0.0, 0.0),
    ]));
    let (t0, t1) = curve.domain().unwrap();
    let a = Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 21 },
    };
    let b = line((-5.0, 0.0, 6.0), (1.0, 0.0, 0.0), 30.0, 22);
    let basis = Basis::of(ViewKind::Front);
    let c = stroke_cross(&a, &b, &basis, 30.0).unwrap();
    assert!(c.iter().all(|c| !c.transversal), "{c:?}");
    let sp = split_params(&[a, b], &basis, 30.0, &Progress::new()).unwrap();
    assert!(sp[0].is_empty() && sp[1].is_empty(), "{sp:?}");
}

#[test]
fn far_from_origin_gives_same_result() {
    // Dieselben Fälle um (1e5, 1e5, 0) verschoben; die Ausdehnung (Kantenlänge der Hüllbox)
    // bleibt 20. Ergebnis gleich: Berührungen teilen nicht, Kreuzungen an denselben Parametern.
    let basis = Basis::of(ViewKind::Top);
    let off = (1e5, 1e5, 0.0);
    let at = |x: f64, y: f64| (x + off.0, y + off.1, off.2);
    let c1 = circle(at(0.0, 0.0), 5.0, 0.0, TAU, 0);
    // Nur exakt darstellbare Mittelpunkte: 1e5 + 7,07… würde schon beim Eingeben um ~1e-11
    // gerundet, und die Kreise schnitten sich dann wirklich (unter ~2e-6 rad).
    for (cx, cy) in [(10.0, 0.0), (0.0, 10.0), (-10.0, 0.0)] {
        let c2 = circle(at(cx, cy), 5.0, 0.0, TAU, 1);
        let c = stroke_cross(&c1, &c2, &basis, 20.0).unwrap();
        assert!(c.iter().all(|c| !c.transversal), "({cx}, {cy}): {c:?}");
        let sp = split_params(&[c1.clone(), c2], &basis, 20.0, &Progress::new()).unwrap();
        assert!(sp[0].is_empty() && sp[1].is_empty(), "({cx}, {cy}): {sp:?}");
    }
    let near = stroke_cross(
        &circle((0.0, 0.0, 0.0), 5.0, 0.0, TAU, 0),
        &circle((4.0, 3.0, 0.0), 5.0, 0.0, TAU, 1),
        &basis,
        20.0,
    )
    .unwrap();
    let far = stroke_cross(&c1, &circle(at(4.0, 3.0), 5.0, 0.0, TAU, 1), &basis, 20.0).unwrap();
    assert_eq!(near.len(), 2);
    assert_eq!(far.len(), 2, "{far:?}");
    for (n, f) in near.iter().zip(&far) {
        assert!(
            (n.ta - f.ta).abs() < 1e-9 && (n.tb - f.tb).abs() < 1e-9,
            "{n:?} {f:?}"
        );
        assert!(f.transversal);
    }
    // Freiformkurve weit draußen, von einer Geraden berührt (wie line_tangent_to_spline…).
    let front = Basis::of(ViewKind::Front);
    let curve = Curve::nurbs(cubic_from_controls(&[
        (1e5, 0.0, 1e5),
        (1e5 + 5.0, 0.0, 1e5 + 8.0),
        (1e5 + 15.0, 0.0, 1e5 + 8.0),
        (1e5 + 20.0, 0.0, 1e5),
    ]));
    let (t0, t1) = curve.domain().unwrap();
    let a = Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 2 },
    };
    let b = line((1e5 - 5.0, 0.0, 1e5 + 6.0), (1.0, 0.0, 0.0), 30.0, 3);
    let sp = split_params(&[a, b], &front, 30.0, &Progress::new()).unwrap();
    assert!(sp[0].is_empty() && sp[1].is_empty(), "{sp:?}");
}

/// Kubische Freiformkurve in der xz-Ebene (Vorderansicht: Bild = (x, z)) mit einer schmalen
/// Spitze: gleichmäßige Knoten 0,1 … 0,9 und dazwischen ein Knotenhaufen 0,5297 … 0,5303; der
/// einzige Kontrollpunkt mit z ≠ 0 (z = 5) wirkt nur über diesem Haufen. Die Spitze (Breite
/// 6e-4 im Parameter, etwa 6 µm in x) liegt zwischen zwei Probestellen jeder Teilbox.
fn spiky_spline() -> Stroke {
    let mut knots: Vec<f64> = vec![0.0; 4];
    knots.extend([0.1, 0.2, 0.3, 0.4, 0.5]);
    knots.extend([0.5297, 0.52985, 0.53, 0.53015, 0.5303]);
    knots.extend([0.6, 0.7, 0.8, 0.9]);
    knots.extend([1.0; 4]);
    let n = knots.len() - 4;
    // Kontrollpunkte an den Greville-Stellen (x = 10 · Mittel von 3 Knoten): x(t) ≈ 10·t.
    let spike = (0..n)
        .position(|i| (knots[i] - 0.5297).abs() < 1e-12 && (knots[i + 4] - 0.5303).abs() < 1e-12)
        .expect("Kontrollpunkt über dem Knotenhaufen");
    let pts: Vec<DVec3> = (0..n)
        .map(|i| {
            let x = 10.0 * (knots[i + 1] + knots[i + 2] + knots[i + 3]) / 3.0;
            DVec3::new(x, 0.0, if i == spike { 5.0 } else { 0.0 })
        })
        .collect();
    let c = planora_math::nurbs::NurbsCurve::new(3, knots, pts, vec![1.0; n]).unwrap();
    Stroke {
        curve: Curve::nurbs(c),
        t0: 0.0,
        t1: 1.0,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    }
}

#[test]
fn a_crossing_between_the_samples_of_a_sharp_spline_is_found() {
    // Waagerechte Linie z = 1 quer durch die Spitze: zwei Kreuzungen. Die Unterteilung prüfte
    // Teilbereiche früher nur an 9 Probestellen; die Spitze liegt zwischen ihnen, ihre Box war zu
    // klein und das Paar fiel weg. Eigene Zählung: brute_force_cross_count (dichte Polygone).
    let spline = spiky_spline();
    let peak = (0..=200_000)
        .map(|i| spline.point(i as f64 / 200_000.0).z)
        .fold(0.0, f64::max);
    assert!(peak > 1.5, "Spitze reicht über die Linie: {peak}");
    let l = line((0.0, 0.0, 1.0), (1.0, 0.0, 0.0), 10.0, 1);
    let basis = Basis::of(ViewKind::Front);
    let want = brute_force_cross_count(&spline, &l, &basis, 200_000);
    assert_eq!(want, 2, "eigene Zählung");
    let got: Vec<_> = stroke_cross(&spline, &l, &basis, 10.0)
        .unwrap()
        .into_iter()
        .filter(|c| c.transversal)
        .collect();
    assert_eq!(got.len(), want, "{got:?}");
    for c in &got {
        assert!((spline.point(c.ta).z - 1.0).abs() < 1e-9 && (c.ta - 0.53).abs() < 4e-4);
    }
    let splits = split_params(&[spline, l], &basis, 10.0, &Progress::new()).unwrap();
    assert_eq!(splits[0].len(), 2);
    assert_eq!(splits[1].len(), 2);
}

#[test]
fn strokes_without_width_are_rejected() {
    let a = circle((0.0, 0.0, 0.0), 5.0, 1.0, 1.0, 0);
    let b = circle((1.0, 0.0, 0.0), 5.0, 0.0, TAU, 1);
    let basis = Basis::of(ViewKind::Top);
    assert!(matches!(
        stroke_cross(&a, &b, &basis, 10.0),
        Err(DrawError::InvalidBody(ref m)) if m == ZERO_WIDTH
    ));
    assert!(matches!(
        split_params(&[b, a], &basis, 10.0, &Progress::new()),
        Err(DrawError::InvalidBody(ref m)) if m == ZERO_WIDTH
    ));
}
