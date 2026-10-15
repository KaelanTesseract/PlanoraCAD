//! Stufe 14 (Teilprojekt 12): Mittellinien aus dem Modell. Sollwerte von Hand aus den Maßen der
//! Prüfkörper; Bildpunkte nach der von Hand aufgeschriebenen Achsentabelle der Normansichten
//! (`common::dim_model::image`), nicht über `Basis::project`.

mod common;

use common::dim_model::{NORMAL_VIEWS, image};
use common::model::*;
use planora_drawing::basis::Basis;
use planora_drawing::centerline::{CenterLine, CenterSource, centerlines};
use planora_drawing::limits::geom_tol;
use planora_drawing::model::ViewKind;
use planora_drawing::{DrawBody, DrawError, compute_view};
use planora_exchange::Progress;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3};

const ALL_VIEWS: [ViewKind; 7] = [
    ViewKind::Front,
    ViewKind::Back,
    ViewKind::Left,
    ViewKind::Right,
    ViewKind::Top,
    ViewKind::Bottom,
    ViewKind::Isometric,
];

/// Blickrichtung der Normansichten, von Hand: Vorne +Y, Hinten −Y, Links +X, Rechts −X,
/// Oben −Z, Unten +Z (Weltachse 0/1/2 und Vorzeichen).
fn depth_axis(kind: ViewKind) -> usize {
    match kind {
        ViewKind::Front | ViewKind::Back => 1,
        ViewKind::Left | ViewKind::Right => 0,
        ViewKind::Top | ViewKind::Bottom => 2,
        ViewKind::Isometric => panic!("keine Normansicht"),
    }
}

/// Eigene Ausdehnung (größte Kante der Hüllbox der Eckpunkte und Kantenpunkte), nur als
/// Maßstab der Toleranz.
fn own_extent(solids: &[&Solid]) -> f64 {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for s in solids {
        for (_, e) in s.edges.iter() {
            for i in 0..=64 {
                let p = e.curve.point(e.t0 + (e.t1 - e.t0) * i as f64 / 64.0);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    (hi - lo).max_element().max(1e-9)
}

fn lines_of(solids: &[&Solid], kind: ViewKind) -> Vec<CenterLine> {
    let bodies: Vec<DrawBody> = solids
        .iter()
        .enumerate()
        .map(|(i, s)| DrawBody {
            name: format!("K{i}"),
            solid: s,
        })
        .collect();
    centerlines(
        &bodies,
        &Basis::of(kind),
        own_extent(solids),
        &Progress::new(),
    )
    .unwrap_or_else(|e| panic!("{kind:?}: {e}"))
}

fn lines(s: &Solid, kind: ViewKind) -> Vec<CenterLine> {
    lines_of(&[s], kind)
}

fn near2(a: DVec2, b: DVec2, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol
}

/// Gibt es eine Linie mit den Enden `a`, `b` (Reihenfolge beliebig)?
fn has_segment(ls: &[CenterLine], a: DVec2, b: DVec2, tol: f64) -> Option<CenterLine> {
    ls.iter()
        .find(|l| {
            (near2(l.a, a, tol) && near2(l.b, b, tol)) || (near2(l.a, b, tol) && near2(l.b, a, tol))
        })
        .copied()
}

/// Achsenkreuz um `c` mit Halblänge `half`: eine waagrechte und eine senkrechte Linie.
#[track_caller]
fn assert_cross(ls: &[CenterLine], c: DVec2, half: f64, tol: f64) -> (CenterSource, CenterSource) {
    let h = has_segment(
        ls,
        c - DVec2::new(half, 0.0),
        c + DVec2::new(half, 0.0),
        tol,
    )
    .unwrap_or_else(|| panic!("waagrechte Linie des Kreuzes um {c:?} (±{half}) fehlt: {ls:?}"));
    let v = has_segment(
        ls,
        c - DVec2::new(0.0, half),
        c + DVec2::new(0.0, half),
        tol,
    )
    .unwrap_or_else(|| panic!("senkrechte Linie des Kreuzes um {c:?} (±{half}) fehlt: {ls:?}"));
    (h.source, v.source)
}

#[track_caller]
fn assert_axis(ls: &[CenterLine], a: DVec2, b: DVec2, tol: f64) -> CenterSource {
    has_segment(ls, a, b, tol)
        .unwrap_or_else(|| panic!("Achsenlinie {a:?} – {b:?} fehlt: {ls:?}"))
        .source
}

fn v2(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

const TOL: f64 = 1e-12;

#[test]
fn cylinder_top_view_has_one_cross_at_the_center() {
    // Zylinder r = 5, h = 12: Boden- und Deckelkreis liegen von oben übereinander → ein Kreuz.
    let s = cylinder_solid(5.0, 12.0);
    for kind in [ViewKind::Top, ViewKind::Bottom] {
        let ls = lines(&s, kind);
        assert_eq!(ls.len(), 2, "{kind:?}: {ls:?}");
        let (h, v) = assert_cross(&ls, v2(0.0, 0.0), 5.0, TOL);
        assert!(matches!(h, CenterSource::Circle { body: 0, .. }), "{h:?}");
        assert_eq!(h, v);
    }
}

#[test]
fn cylinder_front_view_has_one_axis_line() {
    // Achse Z ⟂ Blickrichtung: eine Linie bei x = 0 von z = 0 bis z = 12 (Länge 12), in allen
    // vier Seitenansichten.
    let s = cylinder_solid(5.0, 12.0);
    for kind in [
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
    ] {
        let ls = lines(&s, kind);
        assert_eq!(ls.len(), 1, "{kind:?}: {ls:?}");
        let src = assert_axis(&ls, v2(0.0, 0.0), v2(0.0, 12.0), TOL);
        assert!(matches!(src, CenterSource::Axis { body: 0, .. }), "{src:?}");
        assert!((ls[0].a.distance(ls[0].b) - 12.0).abs() <= TOL);
    }
}

#[test]
fn cone_and_frustum_side_views_have_one_axis_line() {
    // Kegelstumpf r0 = 6, r1 = 3, h = 10 und spitzer Kegel r0 = 6, h = 10: von der Seite eine
    // Achsenlinie (0,0)–(0,10); von oben ein Kreuz mit Halblänge 6 (größter Kreis).
    for s in [cone_solid(6.0, 3.0, 10.0), cone_solid(6.0, 0.0, 10.0)] {
        let ls = lines(&s, ViewKind::Front);
        assert_eq!(ls.len(), 1, "{ls:?}");
        assert_axis(&ls, v2(0.0, 0.0), v2(0.0, 10.0), TOL);
        let ls = lines(&s, ViewKind::Top);
        assert_eq!(ls.len(), 2, "{ls:?}");
        assert_cross(&ls, v2(0.0, 0.0), 6.0, TOL);
    }
}

#[test]
fn sphere_has_a_cross_in_every_normal_view() {
    // Kugel r = 4 um (3, −2, 5): in jeder Normansicht ein Kreuz um das Bild der Mitte, Halblänge 4.
    let c = DVec3::new(3.0, -2.0, 5.0);
    let s = translated(&sphere_solid(4.0), c);
    for kind in NORMAL_VIEWS {
        let ls = lines(&s, kind);
        assert_eq!(ls.len(), 2, "{kind:?}: {ls:?}");
        let (h, _) = assert_cross(&ls, image(kind, c), 4.0, TOL);
        assert!(matches!(h, CenterSource::Sphere { body: 0, .. }), "{h:?}");
    }
}

#[test]
fn box_has_no_centerlines() {
    let s = box_solid(40.0, 20.0, 30.0);
    for kind in ALL_VIEWS {
        assert!(lines(&s, kind).is_empty(), "{kind:?}");
    }
}

#[test]
fn hole_along_y_front_view_has_a_cross_at_the_hole_center() {
    // Quader 40 × 20 × 30 mit Ø10-Loch längs Y durch die Mitte (20, 10, 15).
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, DVec3::Y);
    let ls = lines(&s, ViewKind::Front);
    assert_eq!(ls.len(), 2, "{ls:?}");
    let (h, _) = assert_cross(&ls, v2(20.0, 15.0), 5.0, TOL);
    assert!(matches!(h, CenterSource::Circle { .. }), "{h:?}");
    // Hinten: x gespiegelt.
    let ls = lines(&s, ViewKind::Back);
    assert_eq!(ls.len(), 2, "{ls:?}");
    assert_cross(&ls, v2(-20.0, 15.0), 5.0, TOL);
    // Oben: Achsenlinie des Lochs bei x = 20 von y = 0 bis y = 20 (Länge 20).
    let ls = lines(&s, ViewKind::Top);
    assert_eq!(ls.len(), 1, "{ls:?}");
    let src = assert_axis(&ls, v2(20.0, 0.0), v2(20.0, 20.0), TOL);
    assert!(matches!(src, CenterSource::Axis { .. }), "{src:?}");
    // Links (Bild (−y, z)): waagrecht bei z = 15 von −20 bis 0.
    let ls = lines(&s, ViewKind::Left);
    assert_eq!(ls.len(), 1, "{ls:?}");
    assert_axis(&ls, v2(-20.0, 15.0), v2(0.0, 15.0), TOL);
}

#[test]
fn concentric_circles_give_one_cross() {
    // Zylinder r = 8, h = 20 mit Sackbohrung r = 3, Tiefe 12 auf derselben Achse: vier
    // konzentrische Kreise → ein Kreuz (Halblänge = größter Radius 8); von vorn liegen die Achsen
    // von Mantel [0, 20] und Bohrung [8, 20] aufeinander → eine Linie (0,0)–(0,20).
    let s = cylinder_with_blind_hole(8.0, 20.0, 3.0, 12.0);
    for kind in [ViewKind::Top, ViewKind::Bottom] {
        let ls = lines(&s, kind);
        assert_eq!(ls.len(), 2, "{kind:?}: {ls:?}");
        assert_cross(&ls, v2(0.0, 0.0), 8.0, TOL);
    }
    let ls = lines(&s, ViewKind::Front);
    assert_eq!(ls.len(), 1, "{ls:?}");
    assert_axis(&ls, v2(0.0, 0.0), v2(0.0, 20.0), TOL);
}

#[test]
fn stacked_bodies_share_one_cross_and_one_axis_line() {
    // Zwei Körper auf derselben Achse: Zylinder r = 4, z ∈ [0, 5] und r = 2, z ∈ [5, 10].
    let a = cylinder_solid(4.0, 5.0);
    let b = translated(&cylinder_solid(2.0, 5.0), DVec3::new(0.0, 0.0, 5.0));
    let ls = lines_of(&[&a, &b], ViewKind::Top);
    assert_eq!(ls.len(), 2, "{ls:?}");
    assert_cross(&ls, v2(0.0, 0.0), 4.0, TOL);
    let ls = lines_of(&[&a, &b], ViewKind::Front);
    assert_eq!(ls.len(), 1, "{ls:?}");
    assert_axis(&ls, v2(0.0, 0.0), v2(0.0, 10.0), TOL);
}

#[test]
fn two_separate_bodies_each_get_their_own() {
    // Zylinder r = 2, h = 6 bei x = 0 und bei x = 10.
    let a = cylinder_solid(2.0, 6.0);
    let b = translated(&cylinder_solid(2.0, 6.0), DVec3::new(10.0, 0.0, 0.0));
    let ls = lines_of(&[&a, &b], ViewKind::Top);
    assert_eq!(ls.len(), 4, "{ls:?}");
    let (s0, _) = assert_cross(&ls, v2(0.0, 0.0), 2.0, TOL);
    let (s1, _) = assert_cross(&ls, v2(10.0, 0.0), 2.0, TOL);
    assert!(matches!(s0, CenterSource::Circle { body: 0, .. }), "{s0:?}");
    assert!(matches!(s1, CenterSource::Circle { body: 1, .. }), "{s1:?}");
    let ls = lines_of(&[&a, &b], ViewKind::Front);
    assert_eq!(ls.len(), 2, "{ls:?}");
    let s0 = assert_axis(&ls, v2(0.0, 0.0), v2(0.0, 6.0), TOL);
    let s1 = assert_axis(&ls, v2(10.0, 0.0), v2(10.0, 6.0), TOL);
    assert!(matches!(s0, CenterSource::Axis { body: 0, .. }), "{s0:?}");
    assert!(matches!(s1, CenterSource::Axis { body: 1, .. }), "{s1:?}");
    // Zwei Kreise mit 1e-3 Abstand der Mitten sind nicht konzentrisch: zwei Kreuze.
    let c = translated(&cylinder_solid(2.0, 6.0), DVec3::new(1e-3, 0.0, 0.0));
    let ls = lines_of(&[&a, &c], ViewKind::Top);
    assert_eq!(ls.len(), 4, "{ls:?}");
}

#[test]
fn torus_top_view_cross_half_length_is_major_plus_minor() {
    // Ring R = 10, r = 3, Achse Z: von oben und unten Kreuz mit Halblänge 13; von der Seite nichts
    // (die Nähte sind keine Kanten, der Torus hat nur das Kreuz entlang der Achse).
    let s = torus_solid(10.0, 3.0);
    for kind in [ViewKind::Top, ViewKind::Bottom] {
        let ls = lines(&s, kind);
        assert_eq!(ls.len(), 2, "{kind:?}: {ls:?}");
        let (h, _) = assert_cross(&ls, v2(0.0, 0.0), 13.0, TOL);
        assert!(matches!(h, CenterSource::Torus { body: 0, .. }), "{h:?}");
    }
    for kind in [
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
    ] {
        assert!(lines(&s, kind).is_empty(), "{kind:?}");
    }
}

#[test]
fn isometric_has_centerlines_computed_but_flagged_off_in_the_sheet() {
    // Regel: In der Isometrie stehen Achsen und Kreise schräg; die Berechnung liefert dort
    // keine Mittellinien (auch nicht für Kugeln). Die Isometrie hat zudem `centerlines = false`.
    for s in [
        cylinder_solid(5.0, 12.0),
        sphere_solid(4.0),
        torus_solid(10.0, 3.0),
        box_with_hole(40.0, 20.0, 30.0, 5.0, DVec3::Y),
    ] {
        assert!(lines(&s, ViewKind::Isometric).is_empty());
    }
    let s = cylinder_solid(5.0, 12.0);
    let body = DrawBody {
        name: "Z".into(),
        solid: &s,
    };
    let vl = compute_view(&[body], ViewKind::Isometric, &Progress::new()).unwrap();
    assert!(vl.centerlines.is_empty());
}

#[test]
fn half_cylinder_and_arcs_use_the_arc_center() {
    // Halbzylinder r = 5, h = 8 (Bögen um den Ursprung): von oben Kreuz um (0,0), Halblänge 5;
    // von vorn die Achse (0,0)–(0,8).
    let s = half_cylinder_solid(5.0, 8.0);
    let ls = lines(&s, ViewKind::Top);
    assert_eq!(ls.len(), 2, "{ls:?}");
    let (h, _) = assert_cross(&ls, v2(0.0, 0.0), 5.0, TOL);
    assert!(matches!(h, CenterSource::Circle { .. }), "{h:?}");
    let ls = lines(&s, ViewKind::Front);
    assert_eq!(ls.len(), 1, "{ls:?}");
    assert_axis(&ls, v2(0.0, 0.0), v2(0.0, 8.0), TOL);
    // Quader 30 × 20 × 10 mit vier senkrechten Rundungen r = 4: Viertelkreisbögen um
    // (4, 4), (26, 4), (4, 16), (26, 16) → vier Kreuze mit Halblänge 4.
    let s = rounded_box(30.0, 20.0, 10.0, 4.0);
    let ls = lines(&s, ViewKind::Top);
    assert_eq!(ls.len(), 8, "{ls:?}");
    for c in [v2(4.0, 4.0), v2(26.0, 4.0), v2(4.0, 16.0), v2(26.0, 16.0)] {
        assert_cross(&ls, c, 4.0, 1e-9);
    }
    // Von vorn liegen vordere und hintere Rundung auf derselben Linie: zwei Achsenlinien
    // bei x = 4 und x = 26, z ∈ [0, 10].
    let ls = lines(&s, ViewKind::Front);
    assert_eq!(ls.len(), 2, "{ls:?}");
    assert_axis(&ls, v2(4.0, 0.0), v2(4.0, 10.0), 1e-9);
    assert_axis(&ls, v2(26.0, 0.0), v2(26.0, 10.0), 1e-9);
}

#[test]
fn values_scale_with_the_body_and_survive_far_translation() {
    // Zylinder r = 5, h = 12, um den Faktor f skaliert und um shift·(1,1,1) verschoben:
    // oben Kreuz um (shift, shift) mit Halblänge 5f; vorn Achse (shift, shift)–(shift, shift+12f).
    for (f, shift) in [(1e5, 0.0), (1e-1, 0.0), (1.0, 1e6), (1e-1, 1e6), (1e5, 1e6)] {
        let s = transformed(&cylinder_solid(5.0, 12.0), f, shift);
        let tol = 1e-12 * (shift.abs() + 12.0 * f);
        let ls = lines(&s, ViewKind::Top);
        assert_eq!(ls.len(), 2, "f = {f}, shift = {shift}: {ls:?}");
        assert_cross(&ls, v2(shift, shift), 5.0 * f, tol);
        let ls = lines(&s, ViewKind::Front);
        assert_eq!(ls.len(), 1, "f = {f}, shift = {shift}: {ls:?}");
        assert_axis(&ls, v2(shift, shift), v2(shift, shift + 12.0 * f), tol);
    }
    // Weit draußen auf zwei Wegen auf dieselbe Achse gesetzt: A um 1e7 + 0,3 verschoben, B (auf
    // A gestapelt) erst um 1e7 + 0,1, dann um 0,2. Die Mitten unterscheiden sich um eine
    // Rundungsstufe (10000000,3 gegen 10000000,299999999, ≈ 1,9e-9 – mehr als die kleinste
    // Toleranz 1e-9 und mehr als 64·ε·24): sie gelten trotzdem als eine Achse (ein Kreuz, eine
    // Linie), weil die Toleranz mit der Lage wächst.
    let far = 1e7;
    let a = translated(&cylinder_solid(4.0, 12.0), DVec3::new(far + 0.3, 0.0, 0.0));
    let b = translated(
        &translated(&cylinder_solid(2.0, 12.0), DVec3::new(far + 0.1, 0.0, 12.0)),
        DVec3::new(0.2, 0.0, 0.0),
    );
    let (xa, xb) = (far + 0.3, far + 0.1 + 0.2);
    assert!((xa - xb).abs() > 1e-9, "{xa} {xb}");
    let ls = lines_of(&[&a, &b], ViewKind::Top);
    assert_eq!(ls.len(), 2, "{ls:?}");
    assert_cross(&ls, v2(xa, 0.0), 4.0, 1e-8);
    let ls = lines_of(&[&a, &b], ViewKind::Front);
    assert_eq!(ls.len(), 1, "{ls:?}");
    assert_axis(&ls, v2(xa, 0.0), v2(xa, 24.0), 1e-8);
}

/// Liegt der Punkt `q` (Bild) auf dem Bild einer bekannten Mitte (Achse ∥ Blickrichtung oder
/// Kugelmitte)?
fn on_known_center(q: DVec2, centers: &[PrimCenter], kind: ViewKind, tol: f64) -> bool {
    centers.iter().any(|c| match *c {
        PrimCenter::Axis { base, axis, .. } => {
            axis == depth_axis(kind) && image(kind, base).distance(q) <= tol
        }
        PrimCenter::Point { c } => image(kind, c).distance(q) <= tol,
    })
}

/// Liegt die Strecke `a b` auf dem Bild einer bekannten Achse quer zur Blickrichtung, innerhalb
/// ihres Höhenbereichs?
fn on_known_axis(a: DVec2, b: DVec2, centers: &[PrimCenter], kind: ViewKind, tol: f64) -> bool {
    centers.iter().any(|c| match *c {
        PrimCenter::Axis { base, axis, h } if axis != depth_axis(kind) => {
            let mut top = base;
            top[axis] += h;
            let (p, q) = (image(kind, base), image(kind, top));
            let u = (q - p) / h;
            let within = |x: DVec2| {
                let s = (x - p).dot(u);
                let off = (x - p).perp_dot(u).abs();
                off <= tol && s >= -tol && s <= h + tol
            };
            within(a) && within(b)
        }
        _ => false,
    })
}

#[test]
fn series_of_random_bodies_no_panic_and_centerlines_lie_on_the_axes() {
    let mut total = 0usize;
    let mut crosses = 0usize;
    let mut axes = 0usize;
    for seed in 0..200u64 {
        let (s, csg) = random_body_csg(seed);
        let centers = prim_centers(&csg);
        let tol = geom_tol(own_extent(&[&s]));
        for kind in ALL_VIEWS {
            let ls = lines(&s, kind);
            if kind == ViewKind::Isometric {
                assert!(ls.is_empty(), "Startwert {seed}: Isometrie {ls:?}");
                continue;
            }
            total += ls.len();
            for l in &ls {
                assert!(
                    l.a.is_finite() && l.b.is_finite() && l.a.distance(l.b) > tol,
                    "Startwert {seed}, {kind:?}: {l:?}"
                );
                match l.source {
                    CenterSource::Circle { .. } | CenterSource::Sphere { .. } => {
                        crosses += 1;
                        // Waagrecht oder senkrecht; die Mitte liegt auf einer bekannten Mitte.
                        let mid = 0.5 * (l.a + l.b);
                        let d = l.b - l.a;
                        assert!(
                            d.x.abs() <= tol || d.y.abs() <= tol,
                            "Startwert {seed}, {kind:?}: Kreuzlinie schräg {l:?}"
                        );
                        assert!(
                            on_known_center(mid, &centers, kind, tol),
                            "Startwert {seed}, {kind:?}: Kreuz {l:?} nicht auf einer Mitte"
                        );
                    }
                    CenterSource::Axis { .. } => {
                        axes += 1;
                        assert!(
                            on_known_axis(l.a, l.b, &centers, kind, tol),
                            "Startwert {seed}, {kind:?}: Achse {l:?} nicht auf einer Achse"
                        );
                    }
                    CenterSource::Torus { .. } => {
                        panic!("Startwert {seed}: kein Torus in der Reihe")
                    }
                }
            }
        }
    }
    // Die Reihe prüft tatsächlich etwas.
    assert!(crosses > 100 && axes > 50, "{total} {crosses} {axes}");
}

#[test]
fn compute_view_bbox_ignores_centerlines() {
    // Untere Halbkugel r = 5 (z ≤ 0) von vorn: das Kugelkreuz reicht bis z = +5, die Hüllbox der
    // Linien nur bis z = 0.
    let s = lower_hemisphere_solid(5.0);
    let body = DrawBody {
        name: "H".into(),
        solid: &s,
    };
    let vl = compute_view(&[body], ViewKind::Front, &Progress::new()).unwrap();
    assert_eq!(vl.centerlines.len(), 2, "{:?}", vl.centerlines);
    assert_cross(&vl.centerlines, v2(0.0, 0.0), 5.0, TOL);
    let ((x0, y0), (x1, y1)) = vl.bbox;
    assert!(
        (x0 + 5.0).abs() < 1e-9 && (x1 - 5.0).abs() < 1e-9,
        "{:?}",
        vl.bbox
    );
    assert!((y0 + 5.0).abs() < 1e-9 && y1.abs() < 1e-9, "{:?}", vl.bbox);
}

#[test]
fn degenerate_numbers_give_no_centerlines_and_no_panic() {
    for (edge_r, face_r) in [
        (0.0, 0.0),
        (f64::NAN, f64::NAN),
        (-5.0, -5.0),
        (f64::INFINITY, 0.0),
    ] {
        let s = cylinder_with_bad_numbers(edge_r, face_r);
        let body = DrawBody {
            name: "Z".into(),
            solid: &s,
        };
        for kind in ALL_VIEWS {
            // Ausdehnung fest (die eigene Messung wäre mit diesen Zahlen selbst kaputt).
            let ls = centerlines(
                std::slice::from_ref(&body),
                &Basis::of(kind),
                12.0,
                &Progress::new(),
            )
            .unwrap_or_else(|e| panic!("{edge_r} {kind:?}: {e}"));
            let crosses = ls
                .iter()
                .filter(|l| matches!(l.source, CenterSource::Circle { .. }))
                .count();
            assert_eq!(crosses, 0, "{edge_r} {kind:?}: {ls:?}");
            for l in &ls {
                assert!(l.a.is_finite() && l.b.is_finite(), "{l:?}");
            }
        }
    }
}

#[test]
fn cancelling_stops_the_computation() {
    let s = cylinder_solid(5.0, 12.0);
    let body = DrawBody {
        name: "Z".into(),
        solid: &s,
    };
    let p = Progress::new();
    p.cancel();
    let r = centerlines(&[body], &Basis::of(ViewKind::Top), 12.0, &p);
    assert!(matches!(r, Err(DrawError::Cancelled)), "{r:?}");
}
