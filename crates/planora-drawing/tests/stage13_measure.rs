//! Stufe 13 (Teilprojekt 12): Messung aller Maßarten mit Ankern. Sollwerte von Hand aus den
//! Maßen der Prüfkörper (`common::dim_model`), Bildpunkte nach der von Hand aufgeschriebenen
//! Achsentabelle der Normansichten, nicht über `Basis::project`.

mod common;

use common::dim_model::*;
use planora_drawing::basis::Basis;
use planora_drawing::dimension::{Anchor, DimError, DimKind, Geo3, Measured, measure};
use planora_drawing::model::ViewKind;
use planora_math::{DVec2, DVec3};

const SQRT_HALF: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn run(kind: DimKind, geos: &[Geo3], view: ViewKind) -> Result<Measured, DimError> {
    measure(kind, geos, &Basis::of(view), view, 100.0)
}

fn ok(kind: DimKind, geos: &[Geo3], view: ViewKind) -> Measured {
    run(kind, geos, view).unwrap_or_else(|e| panic!("{kind:?} in {view:?}: {e}"))
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[track_caller]
fn assert_close(got: f64, want: f64, tol: f64) {
    assert!(near(got, want, tol), "{got} ≠ {want} (±{tol})");
}

#[track_caller]
fn assert_pt(got: DVec2, want: (f64, f64)) {
    assert!(
        near(got.x, want.0, 1e-12) && near(got.y, want.1, 1e-12),
        "{got:?} ≠ {want:?}"
    );
}

#[track_caller]
fn linear(m: &Measured) -> (DVec2, DVec2, DVec2) {
    match m.anchor {
        Anchor::Linear { a, b, dir, .. } => (a, b, dir),
        ref other => panic!("kein lineares Maß: {other:?}"),
    }
}

type Span = (DVec2, DVec2);

#[track_caller]
fn spans(m: &Measured) -> (Span, Span) {
    match m.anchor {
        Anchor::Linear { a_span, b_span, .. } => (a_span, b_span),
        ref other => panic!("kein lineares Maß: {other:?}"),
    }
}

/// Spanne `(kleinere, größere Querlage)` genau wie erwartet.
#[track_caller]
fn assert_span(got: Span, want: ((f64, f64), (f64, f64))) {
    assert_pt(got.0, want.0);
    assert_pt(got.1, want.1);
}

#[track_caller]
fn round(m: &Measured) -> (DVec2, f64) {
    match m.anchor {
        Anchor::Round { center, radius } => (center, radius),
        ref other => panic!("kein Kreismaß: {other:?}"),
    }
}

#[track_caller]
fn angle(m: &Measured) -> (DVec2, DVec2, DVec2) {
    match m.anchor {
        Anchor::Angle { vertex, d1, d2 } => (vertex, d1, d2),
        ref other => panic!("kein Winkelmaß: {other:?}"),
    }
}

fn invalid(text: &str) -> Result<Measured, DimError> {
    Err(DimError::Invalid(text.into()))
}

// Quader 10×20×30, Ecke im Ursprung: Kanten von Hand.
fn box_10_20_30() -> Vec<BoxEdge> {
    box_edges(DVec3::ZERO, 10.0, 20.0, 30.0)
}

/// (a) Vorne: senkrechte Kanten (entlang z) bei x = 0 und x = 10 → 10. Anker: je Kante das Ende
/// mit der größeren Querlage (hier oben, z = 30): (0, 30) und (10, 30); Richtung (1, 0).
#[test]
fn horizontal_between_two_vertical_edges_front_view() {
    let e = box_10_20_30();
    let left = line(DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 30.0));
    let right = line(DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 30.0));
    assert_eq!(left.clone(), edge_at(&e, Ax::Z, Ax::X, 0.0));
    let m = ok(
        DimKind::Horizontal,
        &[left.clone(), right.clone()],
        ViewKind::Front,
    );
    assert_eq!(m.value, 10.0);
    assert_eq!(m.prefix, None);
    let (a, b, dir) = linear(&m);
    assert_pt(a, (0.0, 30.0));
    assert_pt(b, (10.0, 30.0));
    assert_pt(dir, (1.0, 0.0));
    // Spannen quer zur Messrichtung (w = (0, 1)): unten, oben.
    let (sa, sb) = spans(&m);
    assert_span(sa, ((0.0, 0.0), (0.0, 30.0)));
    assert_span(sb, ((10.0, 0.0), (10.0, 30.0)));
    // Reihenfolge der Bezüge vertauscht: gleicher Wert, Anker vertauscht, Richtung bleibt.
    let m = ok(DimKind::Horizontal, &[right, left], ViewKind::Front);
    assert_eq!(m.value, 10.0);
    let (a, b, dir) = linear(&m);
    assert_pt(a, (10.0, 30.0));
    assert_pt(b, (0.0, 30.0));
    assert_pt(dir, (1.0, 0.0));
    // Kante rückwärts angegeben (Ende vor Anfang): gleicher Anker.
    let rev = line(DVec3::new(0.0, 0.0, 30.0), DVec3::new(0.0, 0.0, 0.0));
    let m = ok(
        DimKind::Horizontal,
        &[rev, edge_at(&e, Ax::Z, Ax::X, 10.0)],
        ViewKind::Front,
    );
    assert_pt(linear(&m).0, (0.0, 30.0));
    assert_span(spans(&m).0, ((0.0, 0.0), (0.0, 30.0)));
}

/// (b) Vorne: waagerechte Kanten (entlang x) bei z = 0 und z = 30 → 30. Anker: Ende mit der
/// größeren Querlage x = 10: (10, 0), (10, 30); Richtung (0, 1).
#[test]
fn vertical_between_two_horizontal_edges_front_view() {
    let e = box_10_20_30();
    let bottom = edge_at(&e, Ax::X, Ax::Z, 0.0);
    let top = edge_at(&e, Ax::X, Ax::Z, 30.0);
    let m = ok(DimKind::Vertical, &[bottom, top], ViewKind::Front);
    assert_eq!(m.value, 30.0);
    let (a, b, dir) = linear(&m);
    assert_pt(a, (10.0, 0.0));
    assert_pt(b, (10.0, 30.0));
    assert_pt(dir, (0.0, 1.0));
    // Spannen quer zur Messrichtung (w = (1, 0)): links, rechts.
    let (sa, sb) = spans(&m);
    assert_span(sa, ((0.0, 0.0), (10.0, 0.0)));
    assert_span(sb, ((0.0, 30.0), (10.0, 30.0)));
}

/// (c) Oben (Blick −z, ey = +y): Kanten entlang x bei y = 0 und y = 20 → 20.
#[test]
fn depth_in_top_view_uses_the_y_axis() {
    let e = box_10_20_30();
    let near_edge = line(DVec3::new(0.0, 0.0, 30.0), DVec3::new(10.0, 0.0, 30.0));
    let far_edge = line(DVec3::new(0.0, 20.0, 30.0), DVec3::new(10.0, 20.0, 30.0));
    assert!(
        e.iter()
            .any(|k| matches!(&k.geo, g if *g == near_edge || *g == far_edge))
    );
    let m = ok(DimKind::Vertical, &[near_edge, far_edge], ViewKind::Top);
    assert_eq!(m.value, 20.0);
    let (a, b, dir) = linear(&m);
    assert_pt(a, (10.0, 0.0));
    assert_pt(b, (10.0, 20.0));
    assert_pt(dir, (0.0, 1.0));
    // Unten (ey = −y): gleicher Wert, Bildlagen y = 0 und −20.
    let m = ok(
        DimKind::Vertical,
        &[
            line(DVec3::new(0.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 0.0)),
            line(DVec3::new(0.0, 20.0, 0.0), DVec3::new(10.0, 20.0, 0.0)),
        ],
        ViewKind::Bottom,
    );
    assert_eq!(m.value, 20.0);
    let (a, b, _) = linear(&m);
    assert_pt(a, (10.0, 0.0));
    assert_pt(b, (10.0, -20.0));
}

/// (d) Keil: Schräge Kante (10,0,0)–(0,0,30) erscheint vorne schräg (Bildrichtung (−10, 30)),
/// steht also weder quer zu ex noch zu ey → `NotAcross`.
#[test]
fn horizontal_between_a_vertical_and_a_slanted_edge_is_not_across() {
    let vertical = line(xz(0.0, 0.0), xz(0.0, 30.0));
    let slanted = line(xz(10.0, 0.0), xz(0.0, 30.0));
    assert_eq!(
        run(
            DimKind::Horizontal,
            &[vertical.clone(), slanted.clone()],
            ViewKind::Front
        ),
        Err(DimError::NotAcross)
    );
    assert_eq!(
        run(
            DimKind::Vertical,
            &[slanted.clone(), vertical.clone()],
            ViewKind::Front
        ),
        Err(DimError::NotAcross)
    );
    // Eine waagerechte Kante steht nicht quer zu einem waagerechten Maß.
    assert_eq!(
        run(
            DimKind::Horizontal,
            &[vertical, line(xz(0.0, 0.0), xz(10.0, 0.0))],
            ViewKind::Front
        ),
        Err(DimError::NotAcross)
    );
    // Fast quer: Abweichung 1e-7 der Richtung (unter 1e-6) gilt als quer, 1e-5 nicht.
    let tilt = |t: f64| line(xz(10.0, 0.0), xz(10.0 + t * 30.0, 30.0));
    let m = ok(
        DimKind::Horizontal,
        &[line(xz(0.0, 0.0), xz(0.0, 30.0)), tilt(1e-7)],
        ViewKind::Front,
    );
    assert_close(m.value, 10.0 + 15.0 * 1e-7, 1e-12);
    assert_eq!(
        run(
            DimKind::Horizontal,
            &[line(xz(0.0, 0.0), xz(0.0, 30.0)), tilt(1e-5)],
            ViewKind::Front
        ),
        Err(DimError::NotAcross)
    );
}

/// (e) Schräg zwischen zwei Punkten: 3-4-5 → 5, Richtung (0,6; 0,8).
#[test]
fn aligned_between_two_points_is_euclidean() {
    let m = ok(
        DimKind::Aligned,
        &[Geo3::Point(xz(0.0, 0.0)), Geo3::Point(xz(3.0, 4.0))],
        ViewKind::Front,
    );
    assert_close(m.value, 5.0, 1e-12);
    let (a, b, dir) = linear(&m);
    assert_pt(a, (0.0, 0.0));
    assert_pt(b, (3.0, 4.0));
    assert_pt(dir, (0.6, 0.8));
    // Punkte: Spannen je zweimal der Punkt.
    let (sa, sb) = spans(&m);
    assert_span(sa, ((0.0, 0.0), (0.0, 0.0)));
    assert_span(sb, ((3.0, 4.0), (3.0, 4.0)));
    // Tiefe (y) zählt nicht: Zeichnungen zeigen Abstände im Bild (Punkte verschiedener Tiefe).
    let m = ok(
        DimKind::Aligned,
        &[
            Geo3::Point(DVec3::new(1.0, -7.0, 1.0)),
            Geo3::Point(DVec3::new(4.0, 50.0, 5.0)),
        ],
        ViewKind::Front,
    );
    assert_close(m.value, 5.0, 1e-12);
}

/// Schräg zwischen zwei parallelen Kanten: (0,0)–(10,10) und (10,0)–(20,10) vorne; Abstand der
/// Geraden x − z = 0 und x − z = 10 ist 10/√2. Lagen entlang e = (1,1)/√2: Kante 1 [0, 10√2],
/// Kante 2 [5√2, 15√2], Überlappung [5√2, 10√2], Mitte 7,5√2 → a = (7,5; 7,5) auf Kante 1,
/// Fußpunkt b = (12,5; 2,5) auf Kante 2; Richtung (1, −1)/√2. Spannen (Querachse
/// w = (1, 1)/√2): die Kantenenden.
/// Ohne Überlappung: (0,0)–(10,10) und (30,20)–(40,30): Kante 2 bei [25√2, 35√2] hinter Kante 1
/// [0, 10√2] → nächste Enden a = (10, 10), b = (30, 20); Wert |(20, 10)·(−1, 1)/√2| = 10/√2.
#[test]
fn aligned_between_two_parallel_edges_is_their_distance() {
    let m = ok(
        DimKind::Aligned,
        &[
            line(xz(0.0, 0.0), xz(10.0, 10.0)),
            line(xz(10.0, 0.0), xz(20.0, 10.0)),
        ],
        ViewKind::Front,
    );
    assert_close(m.value, 10.0 * SQRT_HALF, 1e-12);
    let (a, b, dir) = linear(&m);
    assert_pt(a, (7.5, 7.5));
    assert_pt(b, (12.5, 2.5));
    assert_pt(dir, (SQRT_HALF, -SQRT_HALF));
    let (sa, sb) = spans(&m);
    assert_span(sa, ((0.0, 0.0), (10.0, 10.0)));
    assert_span(sb, ((10.0, 0.0), (20.0, 10.0)));
    // Ohne Überlappung (beide Reihenfolgen).
    let k1 = line(xz(0.0, 0.0), xz(10.0, 10.0));
    let k2 = line(xz(30.0, 20.0), xz(40.0, 30.0));
    let m = ok(DimKind::Aligned, &[k1.clone(), k2.clone()], ViewKind::Front);
    assert_close(m.value, 10.0 * SQRT_HALF, 1e-12);
    let (a, b, dir) = linear(&m);
    assert_pt(a, (10.0, 10.0));
    assert_pt(b, (30.0, 20.0));
    assert_pt(dir, (SQRT_HALF, -SQRT_HALF));
    assert_close((b - a).dot(dir), 10.0 * SQRT_HALF, 1e-12);
    let m = ok(DimKind::Aligned, &[k2, k1], ViewKind::Front);
    assert_close(m.value, 10.0 * SQRT_HALF, 1e-12);
    let (a, b, dir) = linear(&m);
    assert_pt(a, (30.0, 20.0));
    assert_pt(b, (10.0, 10.0));
    assert_pt(dir, (-SQRT_HALF, SQRT_HALF));
    // Nicht parallel → ungültig; Punkt und Kante gemischt → ungültig.
    assert!(matches!(
        run(
            DimKind::Aligned,
            &[
                line(xz(0.0, 0.0), xz(10.0, 10.0)),
                line(xz(10.0, 0.0), xz(20.0, 11.0)),
            ],
            ViewKind::Front
        ),
        Err(DimError::Invalid(_))
    ));
    assert!(matches!(
        run(
            DimKind::Aligned,
            &[
                Geo3::Point(xz(0.0, 0.0)),
                line(xz(10.0, 0.0), xz(20.0, 10.0))
            ],
            ViewKind::Front
        ),
        Err(DimError::Invalid(_))
    ));
}

/// (f) Oben, Zylinder r = 5 h = 12: Deckelkreis (Mitte (0,0,12), Normale z) mit Blick entlang
/// der Achse → Ø 10, `Round` mit Mitte (0, 0) und Radius 5.
#[test]
fn diameter_of_a_circle_seen_along_its_axis_is_a_round_anchor() {
    let (bottom, top) = cylinder_circles(5.0, 12.0);
    for c in [bottom, top] {
        for view in [ViewKind::Top, ViewKind::Bottom] {
            let m = ok(DimKind::Diameter, std::slice::from_ref(&c), view);
            assert_eq!(m.value, 10.0);
            assert_eq!(m.prefix, Some('Ø'));
            let (center, radius) = round(&m);
            assert_pt(center, (0.0, 0.0));
            assert_eq!(radius, 5.0);
        }
    }
    // Mitte versetzt: Oben (x, y), Unten (x, −y).
    let c = Geo3::Circle {
        center: DVec3::new(3.0, 4.0, 12.0),
        normal: DVec3::NEG_Z,
        radius: 5.0,
    };
    assert_pt(
        round(&ok(
            DimKind::Diameter,
            std::slice::from_ref(&c),
            ViewKind::Top,
        ))
        .0,
        (3.0, 4.0),
    );
    assert_pt(
        round(&ok(DimKind::Diameter, &[c], ViewKind::Bottom)).0,
        (3.0, -4.0),
    );
}

/// (g) Radius desselben Kreises: 5, Vorsatz R.
#[test]
fn radius_of_a_circle_seen_along_its_axis() {
    let (_, top) = cylinder_circles(5.0, 12.0);
    let m = ok(DimKind::Radius, &[top], ViewKind::Top);
    assert_eq!(m.value, 5.0);
    assert_eq!(m.prefix, Some('R'));
    let (center, radius) = round(&m);
    assert_pt(center, (0.0, 0.0));
    assert_eq!(radius, 5.0);
}

/// (h) Vorne, Zylinderfläche (Achse z durch den Ursprung, r = 5, Fläche z = 0…12): Achse im Bild
/// senkrecht, Ø quer dazu, Richtung (1, 0), Wert 10. Umrisslinien x = ±5 von z = 0 bis 12:
/// Spannen ((−5, 0), (−5, 12)) und ((5, 0), (5, 12)); a/b = obere Enden (−5, 12), (5, 12).
#[test]
fn diameter_of_a_cylinder_axis_across_view_is_linear_across_the_axis() {
    let m = ok(
        DimKind::Diameter,
        &[cylinder_axis(5.0, 12.0)],
        ViewKind::Front,
    );
    assert_eq!(m.value, 10.0);
    assert_eq!(m.prefix, Some('Ø'));
    let (a, b, dir) = linear(&m);
    assert_pt(a, (-5.0, 12.0));
    assert_pt(b, (5.0, 12.0));
    assert_pt(dir, (1.0, 0.0));
    let (sa, sb) = spans(&m);
    assert_span(sa, ((-5.0, 0.0), (-5.0, 12.0)));
    assert_span(sb, ((5.0, 0.0), (5.0, 12.0)));
    // Achse entlang −x durch (0, 3, 7), Bereich t ∈ [−4, 6]: Enden (4, 3, 7) und (−6, 3, 7), im
    // Bild (4, 7) und (−6, 7). Ø senkrecht (Richtung (0, 1)), Querachse w = (1, 0): Umrisslinien
    // z = 2 und z = 12 von x = −6 bis 4; a/b = rechte Enden (4, 2), (4, 12).
    let ax = Geo3::Axis {
        origin: DVec3::new(0.0, 3.0, 7.0),
        dir: DVec3::NEG_X,
        radius: 5.0,
        t0: -4.0,
        t1: 6.0,
    };
    let m = ok(
        DimKind::Diameter,
        std::slice::from_ref(&ax),
        ViewKind::Front,
    );
    let (a, b, dir) = linear(&m);
    assert_pt(a, (4.0, 2.0));
    assert_pt(b, (4.0, 12.0));
    assert_pt(dir, (0.0, 1.0));
    let (sa, sb) = spans(&m);
    assert_span(sa, ((-6.0, 2.0), (4.0, 2.0)));
    assert_span(sb, ((-6.0, 12.0), (4.0, 12.0)));
    // Radius einer Zylinderfläche: nicht vorgesehen (quer und mit Blick entlang der Achse).
    let radius_axis = invalid("Der Radius ist nur an Kreisen, Bögen und Kugeln möglich.");
    assert_eq!(run(DimKind::Radius, &[ax], ViewKind::Front), radius_axis);
    assert_eq!(
        run(DimKind::Radius, &[cylinder_axis(5.0, 12.0)], ViewKind::Top),
        radius_axis
    );
    // Zylinderfläche mit Blick entlang der Achse: erscheint als Kreis → Round, Ø 10.
    let m = ok(
        DimKind::Diameter,
        &[cylinder_axis(5.0, 12.0)],
        ViewKind::Top,
    );
    assert_eq!(m.value, 10.0);
    assert_eq!(round(&m), (DVec2::ZERO, 5.0));
    // Bereich verkehrt (t1 < t0) → ungültig; leerer Bereich (t0 = t1) ist erlaubt.
    let bad = Geo3::Axis {
        origin: DVec3::ZERO,
        dir: DVec3::Z,
        radius: 5.0,
        t0: 12.0,
        t1: 0.0,
    };
    assert_eq!(
        run(DimKind::Diameter, &[bad], ViewKind::Front),
        invalid("Der Bereich der Zylinderfläche ist ungültig.")
    );
    let flat = Geo3::Axis {
        origin: DVec3::ZERO,
        dir: DVec3::Z,
        radius: 5.0,
        t0: 3.0,
        t1: 3.0,
    };
    let (a, b, _) = linear(&ok(DimKind::Diameter, &[flat], ViewKind::Front));
    assert_pt(a, (-5.0, 3.0));
    assert_pt(b, (5.0, 3.0));
}

/// (i) Vorne: Deckelkreis von der Seite (Normale z ⟂ Blick y) erscheint als Strecke von (−5, 12)
/// bis (5, 12) → Ø 10 linear (Spannen je zweimal der Endpunkt); Radius → ungültig.
#[test]
fn circle_seen_from_the_side_gives_diameter_but_not_radius() {
    let (_, top) = cylinder_circles(5.0, 12.0);
    let m = ok(
        DimKind::Diameter,
        std::slice::from_ref(&top),
        ViewKind::Front,
    );
    assert_eq!(m.value, 10.0);
    assert_eq!(m.prefix, Some('Ø'));
    let (a, b, dir) = linear(&m);
    assert_pt(a, (-5.0, 12.0));
    assert_pt(b, (5.0, 12.0));
    assert_pt(dir, (1.0, 0.0));
    let (sa, sb) = spans(&m);
    assert_span(sa, ((-5.0, 12.0), (-5.0, 12.0)));
    assert_span(sb, ((5.0, 12.0), (5.0, 12.0)));
    assert_eq!(
        run(DimKind::Radius, std::slice::from_ref(&top), ViewKind::Front),
        invalid("Der Radius ist nur mit Blick auf den Kreis möglich.")
    );
    // Links (ex = −y): gleiche Strecke im Bild bei y = 12, Richtung (1, 0).
    let (a, b, dir) = linear(&ok(DimKind::Diameter, &[top], ViewKind::Left));
    assert_pt(a, (-5.0, 12.0));
    assert_pt(b, (5.0, 12.0));
    assert_pt(dir, (1.0, 0.0));
}

/// (j) Kreis mit Normale (0, 1, 1)/√2 erscheint vorne als Ellipse → ungültig (Ø und R).
#[test]
fn oblique_circle_is_refused() {
    let c = Geo3::Circle {
        center: DVec3::ZERO,
        normal: DVec3::new(0.0, 1.0, 1.0).normalize(),
        radius: 5.0,
    };
    for kind in [DimKind::Diameter, DimKind::Radius] {
        assert_eq!(
            run(kind, std::slice::from_ref(&c), ViewKind::Front),
            invalid("Der Kreis erscheint schräg.")
        );
    }
    // Zylinderachse schräg: ebenso.
    let ax = Geo3::Axis {
        origin: DVec3::ZERO,
        dir: DVec3::new(1.0, 1.0, 0.0),
        radius: 5.0,
        t0: 0.0,
        t1: 10.0,
    };
    assert_eq!(
        run(DimKind::Diameter, &[ax], ViewKind::Front),
        invalid("Der Kreis erscheint schräg.")
    );
}

/// (k) Kugel r = 7 um (1, 2, 3): in jeder Normansicht Ø 14 und R 7, Mitte nach der
/// Achsentabelle: Vorne (1, 3), Hinten (−1, 3), Links (−2, 3), Rechts (2, 3), Oben (1, 2),
/// Unten (1, −2).
#[test]
fn sphere_diameter_in_any_normal_view() {
    let s = sphere(DVec3::new(1.0, 2.0, 3.0), 7.0);
    let want = [
        (ViewKind::Front, (1.0, 3.0)),
        (ViewKind::Back, (-1.0, 3.0)),
        (ViewKind::Left, (-2.0, 3.0)),
        (ViewKind::Right, (2.0, 3.0)),
        (ViewKind::Top, (1.0, 2.0)),
        (ViewKind::Bottom, (1.0, -2.0)),
    ];
    for (view, c) in want {
        let m = ok(DimKind::Diameter, std::slice::from_ref(&s), view);
        assert_eq!((m.value, m.prefix), (14.0, Some('Ø')), "{view:?}");
        let (center, radius) = round(&m);
        assert_pt(center, c);
        assert_eq!(radius, 7.0);
        let m = ok(DimKind::Radius, std::slice::from_ref(&s), view);
        assert_eq!((m.value, m.prefix), (7.0, Some('R')), "{view:?}");
    }
}

/// (l) Winkelprofil: Unterkante und linke Kante treffen sich in (0, 0) → 90°, Richtungen (1, 0)
/// und (0, 1). 45°-Fase (40,25)–(35,30) gegen die Unterkante z = 0: Geraden x + z = 65 und
/// z = 0 schneiden sich in (65, 0); vom Scheitel zur Unterkante (−1, 0), zur Fase
/// (−1, 1)/√2 → 45°. Fase gegen Oberkante: Scheitel (35, 30), (1, −1)/√2 und (−1, 0) → 135°.
#[test]
fn angle_between_two_edges_of_an_l_profile() {
    let (bottom, left) = l_profile_corner_edges();
    let m = ok(DimKind::Angle, &[bottom, left], ViewKind::Front);
    assert_close(m.value, 90.0, 1e-9);
    assert_eq!(m.prefix, None);
    let (v, d1, d2) = angle(&m);
    assert_pt(v, (0.0, 0.0));
    assert_pt(d1, (1.0, 0.0));
    assert_pt(d2, (0.0, 1.0));

    let (chamfer, top, base) = chamfer_plate_edges();
    let m = ok(DimKind::Angle, &[chamfer.clone(), base], ViewKind::Front);
    assert_close(m.value, 45.0, 1e-9);
    let (v, d1, d2) = angle(&m);
    assert_pt(v, (65.0, 0.0));
    assert_pt(d1, (-SQRT_HALF, SQRT_HALF));
    assert_pt(d2, (-1.0, 0.0));

    let m = ok(DimKind::Angle, &[chamfer, top], ViewKind::Front);
    assert_close(m.value, 135.0, 1e-9);
    let (v, d1, d2) = angle(&m);
    assert_pt(v, (35.0, 30.0));
    assert_pt(d1, (SQRT_HALF, -SQRT_HALF));
    assert_pt(d2, (-1.0, 0.0));
}

/// Scheitel innerhalb einer Kante: Kante 1 (3,0)–(10,0) endet im Scheitel (3, 0) → Richtung zur
/// Kante (1, 0). Kante 2 (−6,−9)–(4,1) geht durch (3, 0); näheres Ende (4, 1) (Abstand √2 statt
/// 9√2) → Richtung (1, 1)/√2 → 45° (mit dem ferneren Ende wären es 135°).
#[test]
fn angle_uses_the_end_nearer_to_the_vertex() {
    let m = ok(
        DimKind::Angle,
        &[
            line(xz(3.0, 0.0), xz(10.0, 0.0)),
            line(xz(-6.0, -9.0), xz(4.0, 1.0)),
        ],
        ViewKind::Front,
    );
    assert_close(m.value, 45.0, 1e-9);
    let (v, d1, d2) = angle(&m);
    assert!(near(v.x, 3.0, 1e-12) && near(v.y, 0.0, 1e-12), "{v:?}");
    assert_pt(d1, (1.0, 0.0));
    assert!(
        near(d2.x, SQRT_HALF, 1e-12) && near(d2.y, SQRT_HALF, 1e-12),
        "{d2:?}"
    );
    // Kante 2 rückwärts angegeben ((4,1) zuerst, Scheitel bei τ = 0,1): gleiches Ergebnis.
    let m = ok(
        DimKind::Angle,
        &[
            line(xz(3.0, 0.0), xz(10.0, 0.0)),
            line(xz(4.0, 1.0), xz(-6.0, -9.0)),
        ],
        ViewKind::Front,
    );
    assert_close(m.value, 45.0, 1e-9);
    let (_, _, d2) = angle(&m);
    assert!(
        near(d2.x, SQRT_HALF, 1e-12) && near(d2.y, SQRT_HALF, 1e-12),
        "{d2:?}"
    );
}

/// (m) Parallele Kanten haben keinen Scheitel → ungültig; ebenso eine Kante, die als Punkt
/// erscheint, und ein Bezug, der keine Kante ist.
#[test]
fn parallel_lines_for_an_angle_are_invalid() {
    let e = box_10_20_30();
    let r = run(
        DimKind::Angle,
        &[
            edge_at(&e, Ax::X, Ax::Z, 0.0),
            edge_at(&e, Ax::X, Ax::Z, 30.0),
        ],
        ViewKind::Front,
    );
    assert!(matches!(r, Err(DimError::Invalid(_))), "{r:?}");
    // Kante entlang y erscheint vorne als Punkt.
    let r = run(
        DimKind::Angle,
        &[
            edge_at(&e, Ax::X, Ax::Z, 0.0),
            edge_at(&e, Ax::Y, Ax::X, 10.0),
        ],
        ViewKind::Front,
    );
    assert!(matches!(r, Err(DimError::Invalid(_))), "{r:?}");
    let r = run(
        DimKind::Angle,
        &[edge_at(&e, Ax::X, Ax::Z, 0.0), Geo3::Point(xz(1.0, 1.0))],
        ViewKind::Front,
    );
    assert!(matches!(r, Err(DimError::Invalid(_))), "{r:?}");
}

/// Winkel nur zwischen Kanten parallel zur Bildebene: Kante (0,0,0)–(1,1,1) liegt vorne schräg
/// zur Ansicht (|e·d| = 1/√3) → ungültig; Kanten in der Ebene y = 5 (parallel zur Bildebene,
/// andere Tiefe) sind erlaubt.
#[test]
fn angle_refuses_edges_oblique_to_the_view() {
    let x_edge = line(DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0));
    let oblique = line(DVec3::ZERO, DVec3::new(1.0, 1.0, 1.0));
    let text = invalid("Die Kante liegt schräg zur Ansicht; der Winkel würde verzerrt gezeigt.");
    assert_eq!(
        run(
            DimKind::Angle,
            &[x_edge.clone(), oblique.clone()],
            ViewKind::Front
        ),
        text
    );
    assert_eq!(
        run(DimKind::Angle, &[oblique, x_edge.clone()], ViewKind::Front),
        text
    );
    let m = ok(
        DimKind::Angle,
        &[
            x_edge,
            line(DVec3::new(0.0, 5.0, 0.0), DVec3::new(10.0, 5.0, 10.0)),
        ],
        ViewKind::Front,
    );
    assert_close(m.value, 45.0, 1e-9);
}

/// (n) Vorne: senkrechte Kante vorn (0,0,·) und hinten (0,20,·) liegen im Bild aufeinander →
/// Länge null; ebenso zwei gleiche Punkte (schräg).
#[test]
fn zero_length_is_invalid() {
    let e = box_10_20_30();
    let front = line(DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 30.0));
    let back = line(DVec3::new(0.0, 20.0, 0.0), DVec3::new(0.0, 20.0, 30.0));
    assert!(e.iter().any(|k| k.geo == back));
    assert_eq!(
        run(DimKind::Horizontal, &[front, back], ViewKind::Front),
        invalid("Das Maß hat die Länge null.")
    );
    let p = Geo3::Point(DVec3::new(1.0, 2.0, 3.0));
    let q = Geo3::Point(DVec3::new(1.0, 9.0, 3.0));
    for kind in [DimKind::Horizontal, DimKind::Vertical, DimKind::Aligned] {
        assert_eq!(
            run(kind, &[p.clone(), q.clone()], ViewKind::Front),
            invalid("Das Maß hat die Länge null."),
            "{kind:?}"
        );
    }
}

/// (o) Isometrie: jede Maßart wird abgelehnt, auch mit sonst gültigen Bezügen.
#[test]
fn isometric_is_refused_for_every_kind() {
    let iso = Basis::of(ViewKind::Isometric);
    let (bottom, left) = l_profile_corner_edges();
    let (_, top) = cylinder_circles(5.0, 12.0);
    let pts = [Geo3::Point(xz(0.0, 0.0)), Geo3::Point(xz(3.0, 4.0))];
    let cases: [(DimKind, Vec<Geo3>); 6] = [
        (DimKind::Horizontal, pts.to_vec()),
        (DimKind::Vertical, pts.to_vec()),
        (DimKind::Aligned, pts.to_vec()),
        (DimKind::Diameter, vec![top.clone()]),
        (DimKind::Radius, vec![top]),
        (DimKind::Angle, vec![bottom, left]),
    ];
    for (kind, geos) in &cases {
        assert_eq!(
            measure(*kind, geos, &iso, ViewKind::Isometric, 100.0),
            Err(DimError::NotAllowedInIsometric),
            "{kind:?}"
        );
        // Auch mit der Basis einer Normansicht: die Ansichtsart entscheidet.
        assert_eq!(
            measure(
                *kind,
                geos,
                &Basis::of(ViewKind::Front),
                ViewKind::Isometric,
                100.0
            ),
            Err(DimError::NotAllowedInIsometric),
            "{kind:?}"
        );
    }
}

/// (p) Quader in allen sechs Normansichten: Waagerecht/Senkrecht zwischen den Kanten am Rand
/// (quer stehend und als Punkt gesehen), zwischen Ecken, und schräg zwischen gegenüberliegenden
/// Ecken; 200 Zufallsquader im 0,5-mm-Raster, je ohne Verschiebung, um 1e6 verschoben und mit
/// 1e5 bzw. 1e-1 skaliert. Sollwerte: Eingabemaß × Maßstab (relativ 1e-9).
#[test]
fn all_normal_views_of_a_box() {
    let mut rng = DimRng::new(13);
    let mut checked = 0usize;
    for _ in 0..200 {
        let (a0, b0, c0) = (
            rng.half_steps(0.5, 100.0),
            rng.half_steps(0.5, 100.0),
            rng.half_steps(0.5, 100.0),
        );
        for (scale, o) in [
            (1.0, DVec3::ZERO),
            (1.0, DVec3::new(1e6, -1e6, 1e6)),
            (1e5, DVec3::ZERO),
            (1e-1, DVec3::ZERO),
        ] {
            let (a, b, c) = (a0 * scale, b0 * scale, c0 * scale);
            let dims = box_dims(a, b, c);
            let edges = box_edges(o, a, b, c);
            let corners = box_corners(o, a, b, c);
            let extent = a.max(b).max(c);
            for view in NORMAL_VIEWS {
                let basis = Basis::of(view);
                let ((hx, _), (vx, _)) = view_axes(view);
                let depth = [Ax::X, Ax::Y, Ax::Z]
                    .into_iter()
                    .find(|x| *x != hx && *x != vx)
                    .unwrap();
                let (dh, dv) = (dim_along(dims, hx), dim_along(dims, vx));
                let rel = |got: f64, want: f64, what: &str| {
                    assert!(
                        (got - want).abs() <= 1e-9 * want,
                        "{view:?} {what}: {got} ≠ {want} (Quader {a}×{b}×{c}, o = {o:?})"
                    );
                };
                let lo_h = comp(o, hx);
                let lo_v = comp(o, vx);
                for (kind, along, at, lo, want) in [
                    // Waagerecht: Kanten parallel zur Bild-Senkrechten, am Rand der Waagerechten.
                    (DimKind::Horizontal, vx, hx, lo_h, dh),
                    // Waagerecht: Kanten entlang der Blickrichtung (als Punkte gesehen).
                    (DimKind::Horizontal, depth, hx, lo_h, dh),
                    (DimKind::Vertical, hx, vx, lo_v, dv),
                    (DimKind::Vertical, depth, vx, lo_v, dv),
                ] {
                    let g1 = edge_at(&edges, along, at, lo);
                    let g2 = edge_at(&edges, along, at, lo + dim_along(dims, at));
                    let m = measure(kind, &[g1, g2], &basis, view, extent)
                        .unwrap_or_else(|e| panic!("{view:?} {kind:?}: {e}"));
                    rel(m.value, want, "Kanten");
                }
                // Ecken: niedrigste und höchste Ecke.
                let p0 = Geo3::Point(corners[0]);
                let p1 = Geo3::Point(corners[7]);
                for (kind, want) in [
                    (DimKind::Horizontal, dh),
                    (DimKind::Vertical, dv),
                    (DimKind::Aligned, (dh * dh + dv * dv).sqrt()),
                ] {
                    let m = measure(kind, &[p0.clone(), p1.clone()], &basis, view, extent)
                        .unwrap_or_else(|e| panic!("{view:?} {kind:?}: {e}"));
                    rel(m.value, want, "Ecken");
                    checked += 1;
                }
                // Anker der Ecken nach der Achsentabelle (ungeskalierte Bildpunkte).
                let m = measure(DimKind::Aligned, &[p0, p1], &basis, view, extent).unwrap();
                let (pa, pb, _) = linear(&m);
                let (ia, ib) = (image(view, corners[0]), image(view, corners[7]));
                let tol = 1e-12 * (1.0 + o.abs().max_element() + extent);
                assert!(pa.distance(ia) <= tol && pb.distance(ib) <= tol, "{view:?}");
            }
        }
    }
    assert_eq!(checked, 200 * 4 * 6 * 3);
}

/// (q) Der Wert hängt nicht vom Blattmaßstab ab: `measure` bekommt keinen Maßstab, nur die Basis
/// (Typ-Prüfung der Signatur) und liefert Modell-mm.
#[test]
fn value_does_not_depend_on_the_sheet_scale() {
    type MeasureFn = fn(DimKind, &[Geo3], &Basis, ViewKind, f64) -> Result<Measured, DimError>;
    let f: MeasureFn = measure;
    let e = box_10_20_30();
    let m = f(
        DimKind::Horizontal,
        &[
            edge_at(&e, Ax::Z, Ax::X, 0.0),
            edge_at(&e, Ax::Z, Ax::X, 10.0),
        ],
        &Basis::of(ViewKind::Front),
        ViewKind::Front,
        30.0,
    )
    .unwrap();
    assert_eq!(m.value, 10.0);
}

/// Quader 10×20×30 um 30° um z gedreht, vorne: Senkrechte Kanten bleiben senkrecht. Ecken
/// (10,0) → (10·cos30, 10·sin30) = (8,660…, 5) und (0,20) → (−20·sin30, 20·cos30) = (−10, 17,32…);
/// Waagerecht zwischen den senkrechten Kanten dort: 8,660254… + 10 = 18,660254…
/// Die gedrehten Unterkanten erscheinen waagerecht (Bildrichtung (cos30, 0)): für Waagerecht
/// nicht quer, für Senkrecht quer (Unter- zu Oberkante 30).
#[test]
fn rotated_box_about_z_seen_from_the_front() {
    let rz = |x: f64, y: f64, z: f64| rot_z(DVec3::new(x, y, z), 30.0);
    let e1 = line(rz(10.0, 0.0, 0.0), rz(10.0, 0.0, 30.0));
    let e2 = line(rz(0.0, 20.0, 0.0), rz(0.0, 20.0, 30.0));
    let m = ok(DimKind::Horizontal, &[e2, e1], ViewKind::Front);
    assert_close(m.value, 10.0 * 3f64.sqrt() / 2.0 + 10.0, 1e-12);
    assert_close(m.value, 18.660_254_037_844_386, 1e-12);
    let (a, b, _) = linear(&m);
    assert_close(a.x, -10.0, 1e-12);
    assert_close(b.x, 8.660_254_037_844_386, 1e-12);
    assert_close(a.y, 30.0, 1e-12);

    let low = line(rz(0.0, 0.0, 0.0), rz(10.0, 0.0, 0.0));
    let high = line(rz(0.0, 0.0, 30.0), rz(10.0, 0.0, 30.0));
    let vert = line(rz(0.0, 0.0, 0.0), rz(0.0, 0.0, 30.0));
    assert_eq!(
        run(DimKind::Horizontal, &[low.clone(), vert], ViewKind::Front),
        Err(DimError::NotAcross)
    );
    let m = ok(
        DimKind::Vertical,
        &[low.clone(), high.clone()],
        ViewKind::Front,
    );
    assert_close(m.value, 30.0, 1e-12);
    // Von oben gesehen erscheint dieselbe Kante um 30° schräg: weder waagerecht noch senkrecht.
    for kind in [DimKind::Horizontal, DimKind::Vertical] {
        assert_eq!(
            run(kind, &[low.clone(), high.clone()], ViewKind::Top),
            Err(DimError::NotAcross),
            "{kind:?}"
        );
    }
    // Winkel zwischen der gedrehten Unterkante und der x-Achse (Kante (0,0,0)–(−10,0,0)) von
    // oben: Scheitel (0, 0), Richtungen (cos30, sin30) und (−1, 0) → 150°.
    let m = ok(
        DimKind::Angle,
        &[
            low,
            line(DVec3::new(0.0, 0.0, 0.0), DVec3::new(-10.0, 0.0, 0.0)),
        ],
        ViewKind::Top,
    );
    assert_close(m.value, 150.0, 1e-9);
}

/// Quader um 30° um y (die Blickachse der Vorderansicht) gedreht: die Kante (0,0,0)–(10,0,0)
/// erscheint vorne schräg (Bildrichtung (cos30, −sin30)) → für Waagerecht und Senkrecht
/// `NotAcross`.
#[test]
fn rotated_box_about_y_has_slanted_edges_in_the_front_view() {
    let ry = |x: f64, y: f64, z: f64| rot_y(DVec3::new(x, y, z), 30.0);
    let e = line(ry(0.0, 0.0, 0.0), ry(10.0, 0.0, 0.0));
    let f = line(ry(0.0, 0.0, 30.0), ry(10.0, 0.0, 30.0));
    for kind in [DimKind::Horizontal, DimKind::Vertical] {
        assert_eq!(
            run(kind, &[e.clone(), f.clone()], ViewKind::Front),
            Err(DimError::NotAcross),
            "{kind:?}"
        );
    }
    // Schräg zwischen den beiden parallelen Kanten: ihr Abstand ist die Höhe 30.
    let m = ok(DimKind::Aligned, &[e, f], ViewKind::Front);
    assert_close(m.value, 30.0, 1e-12);
}

/// Mischformen für Waagerecht/Senkrecht: Kreis und Kugel zählen mit ihrer Mitte, eine
/// Zylinderachse mit ihrer Lage im Bild (muss quer stehen), eine Achse entlang der
/// Blickrichtung als Punkt.
#[test]
fn circles_spheres_and_axes_as_references_of_linear_dimensions() {
    let edge = line(xz(-10.0, 0.0), xz(-10.0, 30.0));
    let ax = Geo3::Axis {
        origin: DVec3::new(20.0, 5.0, 0.0),
        dir: DVec3::Z,
        radius: 4.0,
        t0: 0.0,
        t1: 12.0,
    };
    let m = ok(DimKind::Horizontal, &[edge.clone(), ax], ViewKind::Front);
    assert_eq!(m.value, 30.0);
    // Achse von (20, 0) bis (20, 12) im Bild: oberes Ende, Spanne beide Enden.
    let (_, b, _) = linear(&m);
    assert_pt(b, (20.0, 12.0));
    assert_span(spans(&m).1, ((20.0, 0.0), (20.0, 12.0)));
    assert_span(spans(&m).0, ((-10.0, 0.0), (-10.0, 30.0)));
    // Achse entlang y erscheint vorne als Punkt (20, 7).
    let ax_y = Geo3::Axis {
        origin: DVec3::new(20.0, 5.0, 7.0),
        dir: DVec3::Y,
        radius: 4.0,
        t0: -5.0,
        t1: 5.0,
    };
    let m = ok(
        DimKind::Horizontal,
        &[edge.clone(), ax_y.clone()],
        ViewKind::Front,
    );
    assert_eq!(m.value, 30.0);
    assert_pt(linear(&m).1, (20.0, 7.0));
    assert_span(spans(&m).1, ((20.0, 7.0), (20.0, 7.0)));
    let m = ok(
        DimKind::Vertical,
        &[line(xz(0.0, 0.0), xz(5.0, 0.0)), ax_y],
        ViewKind::Front,
    );
    assert_eq!(m.value, 7.0);
    // Achse waagerecht im Bild: nicht quer zu einem waagerechten Maß.
    let ax_x = Geo3::Axis {
        origin: DVec3::new(20.0, 5.0, 7.0),
        dir: DVec3::X,
        radius: 4.0,
        t0: 0.0,
        t1: 10.0,
    };
    assert_eq!(
        run(DimKind::Horizontal, &[edge.clone(), ax_x], ViewKind::Front),
        Err(DimError::NotAcross)
    );
    // Kreis (Mitte (2, 0, 12)) und Kugel (Mitte (−3, 1, 4)).
    let (_, top) = cylinder_circles(5.0, 12.0);
    let m = ok(
        DimKind::Vertical,
        &[top, sphere(DVec3::new(-3.0, 1.0, 4.0), 2.0)],
        ViewKind::Front,
    );
    assert_eq!(m.value, 8.0);
    let (a, b, _) = linear(&m);
    assert_pt(a, (0.0, 12.0));
    assert_pt(b, (-3.0, 4.0));
    assert_span(spans(&m).0, ((0.0, 12.0), (0.0, 12.0)));
}

/// Nicht endliche Eingaben (NaN, ±∞, Überlauf) und falsche Bezugszahlen: benannter Fehler, nie
/// ein Absturz.
#[test]
fn non_finite_inputs_are_invalid() {
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    let good = line(xz(0.0, 0.0), xz(0.0, 30.0));
    let bad_geos = [
        Geo3::Point(DVec3::new(nan, 0.0, 0.0)),
        Geo3::Point(DVec3::new(inf, 0.0, 0.0)),
        line(xz(nan, 0.0), xz(nan, 30.0)),
        line(xz(10.0, 0.0), xz(10.0, inf)),
        line(xz(1e308, 0.0), xz(-1e308, 30.0)),
        Geo3::Circle {
            center: DVec3::ZERO,
            normal: DVec3::new(0.0, 0.0, nan),
            radius: 5.0,
        },
        Geo3::Circle {
            center: DVec3::ZERO,
            normal: DVec3::Z,
            radius: nan,
        },
        Geo3::Axis {
            origin: DVec3::ZERO,
            dir: DVec3::new(0.0, 0.0, inf),
            radius: 5.0,
            t0: 0.0,
            t1: 1.0,
        },
        Geo3::Axis {
            origin: DVec3::ZERO,
            dir: DVec3::Z,
            radius: 5.0,
            t0: nan,
            t1: 1.0,
        },
        Geo3::Axis {
            origin: DVec3::ZERO,
            dir: DVec3::Z,
            radius: 5.0,
            t0: 0.0,
            t1: inf,
        },
        // Endlich, aber das Ende origin + t1·dir läuft über.
        Geo3::Axis {
            origin: DVec3::new(0.0, 0.0, 1e308),
            dir: DVec3::Z,
            radius: 5.0,
            t0: 0.0,
            t1: 1e308,
        },
        sphere(DVec3::ZERO, -inf),
    ];
    for g in &bad_geos {
        for kind in [DimKind::Horizontal, DimKind::Vertical, DimKind::Aligned] {
            let r = run(kind, &[good.clone(), g.clone()], ViewKind::Front);
            assert!(
                matches!(r, Err(DimError::Invalid(_))),
                "{kind:?} {g:?}: {r:?}"
            );
        }
        for kind in [DimKind::Diameter, DimKind::Radius] {
            let r = run(kind, std::slice::from_ref(g), ViewKind::Front);
            assert!(
                matches!(r, Err(DimError::Invalid(_) | DimError::NotAcross)),
                "{kind:?} {g:?}: {r:?}"
            );
        }
        let r = run(DimKind::Angle, &[g.clone(), good.clone()], ViewKind::Front);
        assert!(matches!(r, Err(DimError::Invalid(_))), "{g:?}: {r:?}");
    }
    // Überlauf trotz endlicher Eingaben: Punkte bei ±1e308 schräg → Abstand ∞.
    let r = run(
        DimKind::Aligned,
        &[
            Geo3::Point(xz(-1e308, -1e308)),
            Geo3::Point(xz(1e308, 1e308)),
        ],
        ViewKind::Front,
    );
    assert!(matches!(r, Err(DimError::Invalid(_))), "{r:?}");
    // Kreise ohne Ausdehnung oder ohne Normale.
    for c in [
        Geo3::Circle {
            center: DVec3::ZERO,
            normal: DVec3::Z,
            radius: 0.0,
        },
        Geo3::Circle {
            center: DVec3::ZERO,
            normal: DVec3::ZERO,
            radius: 5.0,
        },
        Geo3::Axis {
            origin: DVec3::ZERO,
            dir: DVec3::ZERO,
            radius: 5.0,
            t0: 0.0,
            t1: 1.0,
        },
    ] {
        let r = run(DimKind::Diameter, &[c], ViewKind::Top);
        assert!(matches!(r, Err(DimError::Invalid(_))), "{r:?}");
    }
    // Basis oder Ausdehnung nicht endlich.
    let mut b = Basis::of(ViewKind::Front);
    b.ex.x = nan;
    let pts = [Geo3::Point(xz(0.0, 0.0)), Geo3::Point(xz(3.0, 4.0))];
    let r = measure(DimKind::Aligned, &pts, &b, ViewKind::Front, 10.0);
    assert!(matches!(r, Err(DimError::Invalid(_))), "{r:?}");
    for extent in [nan, inf, -1.0] {
        let r = measure(
            DimKind::Aligned,
            &pts,
            &Basis::of(ViewKind::Front),
            ViewKind::Front,
            extent,
        );
        assert!(matches!(r, Err(DimError::Invalid(_))), "{extent}: {r:?}");
    }
    // Falsche Zahl von Bezügen.
    for (kind, n) in [
        (DimKind::Horizontal, 1),
        (DimKind::Horizontal, 3),
        (DimKind::Angle, 0),
        (DimKind::Diameter, 2),
        (DimKind::Radius, 0),
    ] {
        let geos = vec![Geo3::Point(xz(1.0, 2.0)); n];
        let r = run(kind, &geos, ViewKind::Front);
        assert!(
            matches!(r, Err(DimError::Invalid(_))),
            "{kind:?} {n}: {r:?}"
        );
    }
    // Falsche Bezugsarten: Punkt für Ø, Kreis für schräg.
    let r = run(
        DimKind::Diameter,
        &[Geo3::Point(xz(1.0, 2.0))],
        ViewKind::Front,
    );
    assert!(matches!(r, Err(DimError::Invalid(_))), "{r:?}");
    let (bottom, top) = cylinder_circles(5.0, 12.0);
    let r = run(DimKind::Aligned, &[bottom, top], ViewKind::Front);
    assert!(matches!(r, Err(DimError::Invalid(_))), "{r:?}");
}
