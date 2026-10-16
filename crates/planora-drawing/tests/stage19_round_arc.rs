//! Stufe 19 (Nachbesserung der Maße): Die Pfeilspitze eines R- oder Ø-Maßes mit Führungslinie
//! sitzt immer auf dem Bogen, nicht nur auf dem vollen Kreis. Handtest „Rundung_Masse“: Bei
//! einem Quader mit vier senkrechten Rundungen R4 zeigte die Führungslinie unter 45° bei drei
//! von vier Ecken auf einen Punkt im Körper (dort gibt es nur einen Viertelkreis).
//!
//! Sollwerte von Hand: Die Viertelkreise des Quaders 30 × 20 × 15 mit R4 liegen in der
//! Draufsicht (x nach rechts, y nach oben) um die Mitten (4, 4) unten links (180°…270°),
//! (26, 4) unten rechts (270°…360°), (26, 16) oben rechts (0°…90°) und (4, 16) oben links
//! (90°…180°). Liegt der gewünschte Winkel (45°) nicht im Bogen, zeigt die Führungslinie von
//! der Mitte aus durch die Bogenmitte (225°, 315°, 135°); oben rechts bleibt 45°.

mod common;

use common::mark_check::*;
use common::model::{box_with_hole, rounded_box};
use planora_drawing::annotate::{
    Mark, MarkKind, clamp_leader_deg, layout_dimension, layout_lost, leader_angle,
};
use planora_drawing::dimension::{
    Anchor, Arc3, ArcSpan, DimId, DimKind, DimSpec, Geo3, Measured, Tolerance, measure,
};
use planora_drawing::sheet::SheetTransform;
use planora_drawing::suggest::circle_edge_arc;
use planora_drawing::{Basis, DrawBody, ViewId, ViewKind, ViewLines, ViewSpec, compute_view};
use planora_drawing::{Placement, Scale, suggest};
use planora_exchange::Progress;
use planora_kernel::{Curve, Edge, Id};
use planora_math::{DVec2, DVec3};
use std::f64::consts::{FRAC_1_SQRT_2 as H2, FRAC_PI_2, PI};

fn xf(scale: f64) -> SheetTransform {
    SheetTransform {
        scale,
        tx: 0.0,
        ty: 0.0,
        bbox_center: DVec2::ZERO,
    }
}

fn spec(kind: DimKind, angle: f64, offset: f64) -> DimSpec {
    DimSpec {
        id: DimId(1),
        view: ViewId(1),
        kind,
        refs: Vec::new(),
        offset,
        angle,
        text_shift: (0.0, 0.0),
        tolerance: Tolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn p3(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

/// Punkt auf dem Kreis um `c` (Normale z) mit Radius `r` unter `deg` Grad, in der Ebene von `c`.
fn on(c: DVec3, r: f64, deg: f64) -> DVec3 {
    let t = deg.to_radians();
    c + DVec3::new(t.cos(), t.sin(), 0.0) * r
}

/// Kreisbogen um `c` (Normale z) von `a` bis `b` Grad (gegen den Uhrzeigersinn von oben).
fn arc_z(c: DVec3, r: f64, a: f64, b: f64) -> Geo3 {
    Geo3::Circle {
        center: c,
        normal: DVec3::Z,
        radius: r,
        arc: Some(Arc3 {
            start: on(c, r, a),
            mid: on(c, r, 0.5 * (a + b)),
            end: on(c, r, b),
        }),
    }
}

fn full_z(c: DVec3, r: f64) -> Geo3 {
    Geo3::Circle {
        center: c,
        normal: DVec3::Z,
        radius: r,
        arc: None,
    }
}

fn measured(kind: DimKind, g: &Geo3, view: ViewKind) -> Measured {
    measure(kind, std::slice::from_ref(g), &Basis::of(view), view, 100.0).unwrap()
}

/// Die eine Pfeilspitze einer Führungslinie.
fn tip_of(marks: &[Mark]) -> DVec2 {
    let a = find_arrows(marks);
    assert_eq!(a.len(), 1, "genau ein Pfeil: {marks:?}");
    a[0].0
}

/// Pfeilspitze des R-Maßes am Bogen `g` (Ansicht `view`) mit dem Winkel `deg`, Maßstab 1.
fn r_tip(g: &Geo3, view: ViewKind, deg: f64) -> DVec2 {
    let m = measured(DimKind::Radius, g, view);
    tip_of(&layout_dimension(
        &spec(DimKind::Radius, deg, 10.0),
        &m,
        &xf(1.0),
        0,
    ))
}

fn polar(c: DVec2, r: f64, deg: f64) -> DVec2 {
    let t = deg.to_radians();
    c + DVec2::new(t.cos(), t.sin()) * r
}

fn assert_close(got: DVec2, want: DVec2, what: &str) {
    assert!(close(got, want, 1e-9), "{what}: {got:?} statt {want:?}");
}

// ---------------------------------------------------------------------------------------------
// Bogen im Bild

#[test]
fn measure_gives_the_arc_span_in_the_image() {
    // Unten links, Draufsicht: 180°…270°, also Anfang π, Überstrich π/2.
    let m = measured(
        DimKind::Radius,
        &arc_z(p3(4., 4., 15.), 4.0, 180., 270.),
        ViewKind::Top,
    );
    let Anchor::Round {
        center,
        radius,
        arc: Some(s),
    } = m.anchor
    else {
        panic!("Bogen erwartet: {m:?}");
    };
    assert_close(center, DVec2::new(4.0, 4.0), "Mitte");
    assert_eq!(radius, 4.0);
    assert!((s.start - PI).abs() < 1e-12, "{s:?}");
    assert!((s.sweep - FRAC_PI_2).abs() < 1e-12, "{s:?}");
    // Dieselben Punkte in umgekehrter Reihenfolge: derselbe Bogen.
    let rev = Geo3::Circle {
        center: p3(4., 4., 15.),
        normal: DVec3::Z,
        radius: 4.0,
        arc: Some(Arc3 {
            start: on(p3(4., 4., 15.), 4.0, 270.0),
            mid: on(p3(4., 4., 15.), 4.0, 225.0),
            end: on(p3(4., 4., 15.), 4.0, 180.0),
        }),
    };
    let m2 = measured(DimKind::Radius, &rev, ViewKind::Top);
    assert_eq!(m2.anchor, m.anchor);
    // Normale nach unten (−z) ändert nichts am Bild.
    let down = match rev {
        Geo3::Circle {
            center,
            radius,
            arc,
            ..
        } => Geo3::Circle {
            center,
            normal: -DVec3::Z,
            radius,
            arc,
        },
        _ => unreachable!(),
    };
    assert_eq!(
        measured(DimKind::Radius, &down, ViewKind::Top).anchor,
        m.anchor
    );
}

#[test]
fn full_circles_have_no_arc() {
    let m = measured(
        DimKind::Diameter,
        &full_z(p3(0., 0., 0.), 5.0),
        ViewKind::Top,
    );
    assert!(matches!(m.anchor, Anchor::Round { arc: None, .. }), "{m:?}");
    // Ein „Bogen“ mit Anfang = Ende ist ein voller Kreis.
    let closed = Geo3::Circle {
        center: p3(0., 0., 0.),
        normal: DVec3::Z,
        radius: 5.0,
        arc: Some(Arc3 {
            start: on(p3(0., 0., 0.), 5.0, 30.0),
            mid: on(p3(0., 0., 0.), 5.0, 210.0),
            end: on(p3(0., 0., 0.), 5.0, 30.0),
        }),
    };
    let m = measured(DimKind::Diameter, &closed, ViewKind::Top);
    assert!(matches!(m.anchor, Anchor::Round { arc: None, .. }), "{m:?}");
}

#[test]
fn non_finite_arc_points_are_invalid() {
    let g = Geo3::Circle {
        center: p3(0., 0., 0.),
        normal: DVec3::Z,
        radius: 5.0,
        arc: Some(Arc3 {
            start: p3(f64::NAN, 0., 0.),
            mid: p3(0., 5., 0.),
            end: p3(-5., 0., 0.),
        }),
    };
    assert!(
        measure(
            DimKind::Radius,
            &[g],
            &Basis::of(ViewKind::Top),
            ViewKind::Top,
            10.0
        )
        .is_err()
    );
}

// ---------------------------------------------------------------------------------------------
// Pfeilspitze auf dem Bogen

#[test]
fn hand_set_radius_on_each_fillet_quarter_touches_its_arc() {
    // (Mitte, Bogen von, bis, Spitze bei 45° gewünscht → tatsächlicher Winkel)
    let corners = [
        ((4.0, 4.0), 180.0, 270.0, 225.0),
        ((26.0, 4.0), 270.0, 360.0, 315.0),
        ((26.0, 16.0), 0.0, 90.0, 45.0),
        ((4.0, 16.0), 90.0, 180.0, 135.0),
    ];
    for ((x, y), a, b, want) in corners {
        let g = arc_z(p3(x, y, 15.), 4.0, a, b);
        let tip = r_tip(&g, ViewKind::Top, 45.0);
        assert_close(tip, polar(DVec2::new(x, y), 4.0, want), "Ecke");
    }
    // Ein Winkel im Bogen bleibt: unten links 200° → Spitze unter 200°.
    let g = arc_z(p3(4., 4., 15.), 4.0, 180.0, 270.0);
    assert_close(
        r_tip(&g, ViewKind::Top, 200.0),
        polar(DVec2::new(4.0, 4.0), 4.0, 200.0),
        "200°",
    );
    // Genau am Bogenende (270°) bleibt der Winkel ebenfalls.
    assert_close(
        r_tip(&g, ViewKind::Top, 270.0),
        polar(DVec2::new(4.0, 4.0), 4.0, 270.0),
        "270°",
    );
    // −90° ist dasselbe wie 270°.
    assert_close(
        r_tip(&g, ViewKind::Top, -90.0),
        polar(DVec2::new(4.0, 4.0), 4.0, 270.0),
        "−90°",
    );
}

#[test]
fn half_circle_slot_end_and_arcs_across_zero() {
    let c = DVec2::ZERO;
    // Langloch-Ende links: 90°…270° (Bogenmitte 180°).
    let slot = arc_z(DVec3::ZERO, 5.0, 90.0, 270.0);
    assert_close(
        r_tip(&slot, ViewKind::Top, 0.0),
        polar(c, 5.0, 180.0),
        "0° → 180°",
    );
    assert_close(
        r_tip(&slot, ViewKind::Top, 100.0),
        polar(c, 5.0, 100.0),
        "100° bleibt",
    );
    // Bogen über 0°: 300°…420° (= 60°), Bogenmitte 0°.
    let across = arc_z(DVec3::ZERO, 5.0, 300.0, 420.0);
    assert_close(
        r_tip(&across, ViewKind::Top, 30.0),
        polar(c, 5.0, 30.0),
        "30° bleibt",
    );
    assert_close(
        r_tip(&across, ViewKind::Top, 330.0),
        polar(c, 5.0, 330.0),
        "330° bleibt",
    );
    assert_close(
        r_tip(&across, ViewKind::Top, 90.0),
        polar(c, 5.0, 0.0),
        "90° → 0°",
    );
    // Großer Bogen 0°…270° (Mitte 135°).
    let big = arc_z(DVec3::ZERO, 5.0, 0.0, 270.0);
    assert_close(
        r_tip(&big, ViewKind::Top, 200.0),
        polar(c, 5.0, 200.0),
        "200° bleibt",
    );
    assert_close(
        r_tip(&big, ViewKind::Top, 300.0),
        polar(c, 5.0, 135.0),
        "300° → 135°",
    );
    // Voller Kreis: jeder Winkel bleibt.
    let full = full_z(DVec3::ZERO, 5.0);
    for deg in [0.0, 45.0, 200.0, 300.0] {
        assert_close(
            r_tip(&full, ViewKind::Top, deg),
            polar(c, 5.0, deg),
            "Vollkreis",
        );
    }
}

#[test]
fn arc_seen_from_below_is_mirrored() {
    // Viertelkreis 0°…90° um den Ursprung; von unten (x nach rechts, −y nach oben) liegt er bei
    // 270°…360°: 45° liegt nicht darauf → Bogenmitte 315°, Spitze (5·√2/2, −5·√2/2).
    let g = arc_z(DVec3::ZERO, 5.0, 0.0, 90.0);
    assert_close(
        r_tip(&g, ViewKind::Bottom, 45.0),
        DVec2::new(5.0 * H2, -5.0 * H2),
        "von unten",
    );
    assert_close(
        r_tip(&g, ViewKind::Bottom, 300.0),
        polar(DVec2::ZERO, 5.0, 300.0),
        "300° bleibt",
    );
}

#[test]
fn small_diameter_leader_on_an_arc_touches_it_large_diameter_is_unchanged() {
    // Ø am Viertelkreis unten links (r 4, Maßstab 1: 8 < 25 → Führungslinie).
    let g = arc_z(p3(4., 4., 15.), 4.0, 180.0, 270.0);
    let m = measured(DimKind::Diameter, &g, ViewKind::Top);
    let tip = tip_of(&layout_dimension(
        &spec(DimKind::Diameter, 45.0, 10.0),
        &m,
        &xf(1.0),
        0,
    ));
    assert_close(tip, polar(DVec2::new(4.0, 4.0), 4.0, 225.0), "Ø klein");
    // Großer Ø (r 20 → 40 ≥ 25) bleibt die Linie durch die Mitte unter dem gewünschten Winkel.
    let big = arc_z(DVec3::ZERO, 20.0, 180.0, 270.0);
    let m = measured(DimKind::Diameter, &big, ViewKind::Top);
    let marks = layout_dimension(&spec(DimKind::Diameter, 45.0, 10.0), &m, &xf(1.0), 0);
    let tips: Vec<DVec2> = find_arrows(&marks).iter().map(|a| a.0).collect();
    assert_eq!(tips.len(), 2);
    assert_close(tips[0], polar(DVec2::ZERO, 20.0, 45.0), "Ø groß, Ende 1");
    assert_close(tips[1], polar(DVec2::ZERO, 20.0, 225.0), "Ø groß, Ende 2");
}

#[test]
fn lost_dimension_at_its_last_place_points_onto_the_arc() {
    let g = arc_z(p3(4., 4., 15.), 4.0, 180.0, 270.0);
    let m = measured(DimKind::Radius, &g, ViewKind::Top);
    let marks = layout_lost(
        &spec(DimKind::Radius, 45.0, 10.0),
        Some(&m.anchor),
        &xf(2.0),
        "Die Kante gibt es nicht mehr.",
    );
    assert!(marks.iter().all(|k| kind_of(k) == MarkKind::Lost));
    assert_close(
        tip_of(&marks),
        polar(DVec2::new(8.0, 8.0), 8.0, 225.0),
        "verloren",
    );
}

#[test]
fn leader_angle_and_clamp() {
    let s = ArcSpan {
        start: PI,
        sweep: FRAC_PI_2,
    };
    let deg = |x: f64| x.to_radians();
    // Im Bogen: unverändert; außerhalb: Bogenmitte.
    assert!((leader_angle(Some(&s), 200.0) - deg(200.0)).abs() < 1e-12);
    assert!((leader_angle(Some(&s), 45.0) - deg(225.0)).abs() < 1e-12);
    assert!((leader_angle(None, 45.0) - deg(45.0)).abs() < 1e-12);
    // Klemmen auf das nähere Bogenende (in Grad).
    assert_eq!(clamp_leader_deg(Some(&s), 200.0), 200.0);
    assert!((clamp_leader_deg(Some(&s), 170.0) - 180.0).abs() < 1e-9);
    assert!((clamp_leader_deg(Some(&s), 300.0) - 270.0).abs() < 1e-9);
    assert!((clamp_leader_deg(Some(&s), 30.0) - 270.0).abs() < 1e-9);
    assert!((clamp_leader_deg(Some(&s), 100.0) - 180.0).abs() < 1e-9);
    assert_eq!(clamp_leader_deg(None, 30.0), 30.0);
    // Bogen über 0°.
    let z = ArcSpan {
        start: deg(300.0),
        sweep: deg(120.0),
    };
    assert!(z.contains(deg(10.0)) && z.contains(deg(-30.0)) && !z.contains(deg(90.0)));
    assert!((z.mid() - 0.0).abs() < 1e-12 || (z.mid() - 2.0 * PI).abs() < 1e-12);
}

// ---------------------------------------------------------------------------------------------
// Echte Körper: Vorschlag und Bogen der Kante

/// Draufsicht des Quaders 30 × 20 × 15 mit R4.
fn fillet_top() -> (planora_kernel::Solid, ViewLines) {
    let s = rounded_box(30.0, 20.0, 15.0, 4.0);
    let lines = compute_view(
        &[DrawBody {
            name: "R".into(),
            solid: &s,
        }],
        ViewKind::Top,
        &Progress::new(),
    )
    .unwrap();
    (s, lines)
}

fn top_spec() -> ViewSpec {
    ViewSpec {
        id: ViewId(1),
        kind: ViewKind::Top,
        scale: Scale { num: 2, den: 1 },
        placement: Placement::Free { x: 100.0, y: 100.0 },
        parent: None,
        show_hidden: true,
        centerlines: true,
    }
}

#[test]
fn suggested_radius_leaders_of_a_fillet_box_touch_their_arcs() {
    let (s, lines) = fillet_top();
    let body = [DrawBody {
        name: "R".into(),
        solid: &s,
    }];
    let cands = suggest(&body, &[(top_spec(), &lines)], &[]);
    let radii: Vec<_> = cands.iter().filter(|c| c.kind == DimKind::Radius).collect();
    assert_eq!(radii.len(), 4, "{cands:?}");
    let basis = Basis::of(ViewKind::Top);
    let mut seen = Vec::new();
    for c in radii {
        let m = measure(c.kind, &c.geos, &basis, ViewKind::Top, lines.extent).unwrap();
        let Anchor::Round { center, .. } = m.anchor else {
            panic!("{m:?}");
        };
        // Wie im Programm: 45°, Abstand 10, Maßstab 2.
        let tip = tip_of(&layout_dimension(
            &spec(DimKind::Radius, 45.0, 10.0),
            &m,
            &xf(2.0),
            0,
        ));
        let want = match (center.x.round() as i64, center.y.round() as i64) {
            (4, 4) => 225.0,
            (26, 4) => 315.0,
            (26, 16) => 45.0,
            (4, 16) => 135.0,
            other => panic!("unerwartete Mitte {other:?}"),
        };
        assert_close(tip, polar(center * 2.0, 8.0, want), "Vorschlag");
        seen.push(want as i64);
    }
    seen.sort();
    assert_eq!(seen, vec![45, 135, 225, 315]);
}

#[test]
fn circle_edge_arc_of_fillet_quarters_and_full_holes() {
    let (s, _) = fillet_top();
    let mut quarters = 0;
    for (_, e) in s.edges.iter() {
        let Curve::Circle { frame, radius } = &e.curve else {
            continue;
        };
        let arc = circle_edge_arc(&s, e).expect("Viertelkreis");
        // Anfang, Mitte und Ende liegen auf dem Kreis, Mitte zwischen den Enden.
        for p in [arc.start, arc.mid, arc.end] {
            assert!(((p - frame.origin).length() - radius).abs() < 1e-9);
        }
        let ang = |p: DVec3| (p - frame.origin).angle_between(arc.mid - frame.origin);
        assert!((ang(arc.start) - PI / 4.0).abs() < 1e-9, "{arc:?}");
        assert!((ang(arc.end) - PI / 4.0).abs() < 1e-9, "{arc:?}");
        quarters += 1;
    }
    assert_eq!(quarters, 8, "vier Rundungen oben und unten");
    // Bohrung: volle Kreise (auch wenn aus mehreren Kanten gebaut) haben keinen Bogen.
    let h = box_with_hole(40.0, 30.0, 12.0, 5.0, DVec3::Z);
    let mut circles = 0;
    for (_, e) in h.edges.iter() {
        if matches!(e.curve, Curve::Circle { .. }) {
            assert_eq!(circle_edge_arc(&h, e), None);
            circles += 1;
        }
    }
    assert!(circles >= 2);
    // Eine Kante, die kein Kreis ist: kein Bogen.
    let line: &Edge = h
        .edges
        .iter()
        .map(|(_, e)| e)
        .find(|e| matches!(e.curve, Curve::Line { .. }))
        .unwrap();
    assert_eq!(circle_edge_arc(&h, line), None);
    let _ = Id::<Edge>::from_index(0);
}
