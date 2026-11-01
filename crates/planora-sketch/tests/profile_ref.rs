//! Profil-Verweise wiederfinden (Entscheidung 4), Flächen zusammenfassen und alte Profilnummern
//! umrechnen (Entscheidung 5).

use std::f64::consts::PI;

use planora_kernel::{AnalyticKernel, Kernel, Profile, Solid};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Rechteck mit den Linien unten, rechts, oben, links (in dieser Reihenfolge).
fn rect(s: &mut Sketch, x0: f64, y0: f64, x1: f64, y1: f64) -> (Vec<PointId>, Vec<CurveId>) {
    let p = vec![
        s.add_point(x0, y0).unwrap(),
        s.add_point(x1, y0).unwrap(),
        s.add_point(x1, y1).unwrap(),
        s.add_point(x0, y1).unwrap(),
    ];
    let c = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    (p, c)
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let a = s.add_point(a.0, a.1).unwrap();
    let b = s.add_point(b.0, b.1).unwrap();
    s.add_line(a, b).unwrap()
}

// ---- find -------------------------------------------------------------------------------

#[test]
fn a_moved_or_enlarged_rectangle_is_found_again() {
    let mut s = sketch();
    let (p, _) = rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    let r = s.profiles().refs[0].clone();
    for (i, &q) in p.iter().enumerate() {
        let v = s.point_xy(q).unwrap();
        s.set_point_xy(q, v.x + 5.0, v.y + 3.0).unwrap();
        // auch zwischendurch (verzerrtes Viereck) bleibt es dieselbe Fläche
        assert_eq!(s.profiles().find(&r), Some(0), "nach Punkt {i}");
    }
    // vergrößern: rechte Kante von x = 25 nach x = 35
    s.set_point_xy(p[1], 35.0, 3.0).unwrap();
    s.set_point_xy(p[2], 35.0, 13.0).unwrap();
    let e = s.profiles();
    assert_eq!(e.find(&r), Some(0));
    assert!((e.profiles[0].outer.signed_area().abs() - 300.0).abs() < 1e-9);
}

#[test]
fn a_chamfered_corner_is_found_again() {
    let mut s = sketch();
    let (p, c) = rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    let r = s.profiles().refs[0].clone();
    // Fase an der Ecke (20, 10): rechte Linie endet bei (20, 8), obere beginnt bei (18, 10),
    // dazwischen die Fasenlinie (gleichwertig zu einer gestutzten Ecke, Kurven-Ids bleiben).
    let q = s.add_point(18.0, 10.0).unwrap();
    s.set_point_xy(p[2], 20.0, 8.0).unwrap();
    let mut d = s.to_data();
    d.curves[c[2].index()].kind = CurveKind::Line { a: q, b: p[3] };
    let mut s = Sketch::from_data(d).unwrap();
    s.add_line(p[2], q).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.profiles[0].outer.segments.len(), 5);
    assert_eq!(e.find(&r), Some(0));
}

#[test]
fn a_rectangle_split_in_half_is_lost() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    let r = s.profiles().refs[0].clone();
    // Mittellinie: beide Hälften haben J = 30 / 70 ≈ 0,43 < 0,5
    line(&mut s, (10.0, 0.0), (10.0, 10.0));
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2);
    assert_eq!(e.find(&r), None);
}

#[test]
fn a_circle_added_inside_keeps_the_ring() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    let r = s.profiles().refs[0].clone();
    let m = s.add_point(10.0, 10.0).unwrap();
    s.add_circle(m, 4.0).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2);
    let ring = e.find(&r).expect("Ring gefunden");
    assert_eq!(e.profiles[ring].holes.len(), 1);
}

#[test]
fn a_deleted_curve_loses_the_profile() {
    let mut s = sketch();
    let (_, c) = rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    let r = s.profiles().refs[0].clone();
    s.remove_curve(c[1]).unwrap();
    assert_eq!(s.profiles().find(&r), None);
}

#[test]
fn the_inner_disk_is_found_by_its_circle() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    let m = s.add_point(10.0, 10.0).unwrap();
    s.add_circle(m, 4.0).unwrap();
    let e = s.profiles();
    let disk = (0..2).find(|&i| e.profiles[i].holes.is_empty()).unwrap();
    let r = e.refs[disk].clone();
    // Kreis verschoben
    s.set_point_xy(m, 11.0, 9.0).unwrap();
    assert_eq!(s.profiles().find(&r), Some(disk));
}

// ---- resolve ----------------------------------------------------------------------------

#[test]
fn resolve_maps_refs_and_names_the_lost_one() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 3.0, 3.0);
    rect(&mut s, 10.0, 0.0, 13.0, 3.0);
    let e = s.profiles();
    assert_eq!(e.resolve(&e.refs), Ok(vec![0, 1]));
    assert_eq!(
        e.resolve(&[e.refs[1].clone(), e.refs[0].clone()]),
        Ok(vec![1, 0])
    );
    assert_eq!(e.resolve(&[e.refs[0].clone(), e.refs[0].clone()]), Err(1));
    let mut gone = e.refs[1].clone();
    for piece in &mut gone.outer {
        piece.curve = e.refs[0].outer[0].curve;
        piece.left = !piece.left;
    }
    assert_eq!(e.resolve(&[e.refs[0].clone(), gone]), Err(1));
    assert_eq!(e.resolve(&[]), Ok(vec![]));
}

// ---- merged -----------------------------------------------------------------------------

#[test]
fn two_halves_merge_into_one_profile() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    line(&mut s, (10.0, 0.0), (10.0, 10.0));
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2);
    let m = e.merged(&[0, 1]);
    assert_eq!(m.len(), 1);
    assert!(m[0].holes.is_empty());
    assert!((m[0].outer.signed_area().abs() - 200.0).abs() < 1e-9);
    // geteilte Kanten werden wieder eine Kante: Rechteck mit vier Segmenten
    assert_eq!(m[0].outer.segments.len(), 4);
    assert!(m[0].outer.is_closed(s.tolerance()));
    let k = AnalyticKernel::default();
    let solid = k.extrude(&m[0], s.frame(), 1.0).unwrap();
    k.validate(&solid).unwrap();
    assert_eq!(solid.faces.len(), 6);
}

#[test]
fn separate_rectangles_stay_separate_and_a_single_face_is_unchanged() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 3.0, 3.0);
    rect(&mut s, 10.0, 0.0, 13.0, 3.0);
    let e = s.profiles();
    let m = e.merged(&[1, 0]);
    assert_eq!(m.len(), 2);
    assert_eq!(m[0], e.profiles[0]);
    assert_eq!(m[1], e.profiles[1]);
    assert_eq!(e.merged(&[1]), vec![e.profiles[1].clone()]);
    assert_eq!(e.merged(&[1, 1]), vec![e.profiles[1].clone()]);
    assert!(e.merged(&[]).is_empty());
}

#[test]
fn ring_and_disk_merge_into_the_full_rectangle() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    let m = s.add_point(10.0, 10.0).unwrap();
    s.add_circle(m, 4.0).unwrap();
    let e = s.profiles();
    let merged = e.merged(&[0, 1]);
    assert_eq!(merged.len(), 1);
    assert!(merged[0].holes.is_empty());
    assert_eq!(merged[0].outer.segments.len(), 4);
    assert!((merged[0].outer.signed_area().abs() - 400.0).abs() < 1e-9);
}

#[test]
fn merging_keeps_holes_of_the_merged_region() {
    // Rechteck mit Kreisloch rechts, Mittellinie links davon: beide Hälften zusammen behalten das
    // Loch.
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 30.0, 10.0);
    line(&mut s, (10.0, 0.0), (10.0, 10.0));
    let m = s.add_point(20.0, 5.0).unwrap();
    s.add_circle(m, 2.0).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 3);
    let halves: Vec<usize> = (0..3)
        .filter(|&i| e.profiles[i].outer.signed_area().abs() > 50.0)
        .collect();
    let merged = e.merged(&halves);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].holes.len(), 1);
    assert_eq!(merged[0].outer.segments.len(), 4);
    let k = AnalyticKernel::default();
    let solid = k.extrude(&merged[0], s.frame(), 1.0).unwrap();
    k.validate(&solid).unwrap();
}

#[test]
fn a_circle_cut_by_a_line_merges_back_into_a_full_circle() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(m, 5.0).unwrap();
    line(&mut s, (-1.0, -6.0), (2.0, 6.0));
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2);
    let merged = e.merged(&[0, 1]);
    assert_eq!(merged.len(), 1);
    assert_eq!(
        merged[0].outer,
        planora_kernel::ProfileLoop::circle(DVec2::ZERO, 5.0)
    );
}

// ---- Übergang alter Profilnummern --------------------------------------------------------

fn tangent_hole(scale: f64) -> Sketch {
    let mut s = sketch();
    let (_, c) = rect(&mut s, 0.0, 0.0, 20.0 * scale, 10.0 * scale);
    let m = s.add_point(10.0 * scale, 3.1 * scale).unwrap();
    let k = s.add_circle(m, 3.0 * scale).unwrap();
    s.add_constraint(ConstraintKind::Tangent(c[0], k)).unwrap();
    s.solve();
    s
}

/// Die Skizzen der Tests in `tests/profile.rs` plus Rechteck mit offener Linie quer durch und
/// weitere; `true`: die alten Profile lassen sich nicht umrechnen (heutige Flächen abgelehnt).
fn scenarios() -> Vec<(&'static str, Sketch, bool)> {
    let mut out = Vec::new();
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 3.0, 4.0);
    out.push(("Rechteck", s, false));

    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    let c = s.add_point(10.0, 10.0).unwrap();
    s.add_circle(c, 4.0).unwrap();
    out.push(("Kreis im Rechteck", s, false));

    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    for r in [10.0, 6.0, 2.0] {
        s.add_circle(c, r).unwrap();
    }
    out.push(("Schachtelung", s, false));

    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 3.0, 3.0);
    rect(&mut s, 10.0, 0.0, 13.0, 3.0);
    out.push(("zwei Rechtecke", s, false));

    let mut s = sketch();
    let (p, _) = rect(&mut s, 0.0, 0.0, 3.0, 3.0);
    let diag = s.add_line(p[0], p[2]).unwrap();
    s.set_construction(diag, true).unwrap();
    out.push(("Konstruktion", s, false));

    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    let c = s.add_point(5.0, 5.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, c).unwrap();
    out.push(("offene Kette", s, false));

    let mut s = sketch();
    let (p, _) = rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    let extra = s.add_point(9.0, 9.0).unwrap();
    s.add_line(p[0], extra).unwrap();
    out.push(("Diagonale mit Stummel", s, false));

    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    line(&mut s, (1.0, 0.0), (1.0, 4.0));
    out.push(("T-Stoß", s, false));

    let mut s = sketch();
    let p = [
        s.add_point(0.0, 0.0).unwrap(),
        s.add_point(4.0, 4.0).unwrap(),
        s.add_point(4.0, 0.0).unwrap(),
        s.add_point(0.0, 4.0).unwrap(),
    ];
    for i in 0..4 {
        s.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    out.push(("Schleife", s, false));

    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 6.0, 6.0);
    rect(&mut s, 3.0, 3.0, 9.0, 9.0);
    out.push(("kreuzende Quadrate", s, false));

    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let l = s.add_point(-1.0, 0.0).unwrap();
    let r = s.add_point(1.0, 0.0).unwrap();
    s.add_line(l, r).unwrap();
    s.add_arc(c, r, l).unwrap();
    out.push(("Halbscheibe", s, false));

    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    let c = s.add_point(4.0, 4.0).unwrap();
    let c2 = s.add_point(4.0 + 1e-8, 4.0).unwrap();
    let d = s.add_point(0.0, 4.0).unwrap();
    let d2 = s.add_point(1e-8, 4.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, c).unwrap();
    s.add_line(c2, d).unwrap();
    s.add_line(d2, a).unwrap();
    out.push(("fast berührende Enden", s, false));

    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, a).unwrap();
    out.push(("Hin und zurück", s, false));

    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(2.0, 0.0).unwrap();
    let b = s.add_point(0.0, 2.0).unwrap();
    s.add_line(a, c).unwrap();
    s.add_line(c, b).unwrap();
    s.add_arc(c, a, b).unwrap();
    out.push(("Viertelkreis rückwärts", s, false));

    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 30.0, 20.0);
    rect(&mut s, 2.0, 2.0, 12.0, 18.0);
    let c = s.add_point(22.0, 10.0).unwrap();
    s.add_circle(c, 5.0).unwrap();
    rect(&mut s, 4.0, 4.0, 8.0, 8.0);
    out.push(("zwei Löcher und Insel", s, false));

    for line_first in [false, true] {
        let mut s = sketch();
        let c = s.add_point(0.0, 0.0).unwrap();
        let a = s.add_point(1.0, 0.0).unwrap();
        let b = s.add_point(-2.0, 0.0).unwrap();
        if line_first {
            s.add_line(b, a).unwrap();
            s.add_arc(c, a, b).unwrap();
        } else {
            s.add_arc(c, a, b).unwrap();
            s.add_line(b, a).unwrap();
        }
        out.push(("Bogenende neben dem Kreis", s, false));
    }

    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(1.0e4, 0.0).unwrap();
    let b = s.add_point(1.0e4, 5.0e-6).unwrap();
    let t = s.add_point(0.0, 1.0).unwrap();
    s.add_arc(c, a, b).unwrap();
    s.add_line(b, t).unwrap();
    s.add_line(t, a).unwrap();
    out.push(("Bogen ohne Winkel", s, false));

    out.push(("tangentiales Loch", tangent_hole(1.0), false));
    out.push(("tangentiales Loch groß", tangent_hole(1e5), false));

    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(a, 10.0).unwrap();
    let b = s.add_point(0.0, 4.0).unwrap();
    s.add_circle(b, 6.0).unwrap();
    out.push(("innen tangentiale Kreise", s, false));

    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 40.0, 30.0);
    let m = s.add_point(20.0, 11.0).unwrap();
    s.add_circle(m, 10.0).unwrap();
    out.push(("Loch mit kleinem Spalt", s, false));

    out.push(("offene Linie quer durch", crossed_rectangle(), false));

    // zusätzlich (nicht aus `tests/profile.rs`): gekrümmte Ränder, von offenen Linien geteilt
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(m, 5.0).unwrap();
    line(&mut s, (-1.0, -6.0), (2.0, 6.0));
    out.push(("Kreis mit offener Linie", s, false));

    let mut s = sketch();
    let m = s.add_point(1.0, 2.0).unwrap();
    s.add_ellipse(m, DVec2::new(8.0, 3.0), 4.0).unwrap();
    line(&mut s, (-10.0, 1.0), (12.0, 3.0));
    line(&mut s, (0.0, -10.0), (1.0, 12.0));
    out.push(("Ellipse mit zwei offenen Linien", s, false));

    let mut s = sketch();
    let (p, _) = rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    let q = s.add_point(10.0, 14.0).unwrap();
    let top = s
        .curves()
        .find(|(_, c)| c.kind.points() == vec![p[2], p[3]])
        .unwrap()
        .0;
    s.remove_curve(top).unwrap();
    s.add_spline(&[p[2], q, p[3]]).unwrap();
    line(&mut s, (5.0, -3.0), (6.0, 16.0));
    out.push(("Spline-Kante mit offener Linie", s, false));

    // Fix-Runde 1: Die offene Linie berührt das Loch; die Fläche darunter ist eingeklemmt und
    // abgelehnt (Entscheidung 3). Ohne sie wäre der Körper kleiner: verloren.
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    let m = s.add_point(10.0, 5.0).unwrap();
    s.add_circle(m, 3.0).unwrap();
    line(&mut s, (-5.0, 8.0), (25.0, 8.0));
    out.push(("offene Linie berührt das Loch", s, true));
    out
}

/// Review-Fokus 4: Rechteck, quer durch eine offene Linie, die über beide Seiten hinausragt.
fn crossed_rectangle() -> Sketch {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    line(&mut s, (-5.0, 4.0), (25.0, 6.0));
    s
}

struct Body {
    volume: f64,
    hull: (DVec3, DVec3),
    faces: usize,
    edges: usize,
}

fn body(s: &Sketch, p: &Profile) -> Body {
    let k = AnalyticKernel::default();
    let solid: Solid = k.extrude(p, s.frame(), 1.5).unwrap();
    k.validate(&solid).unwrap();
    let mesh = tessellate(
        &solid,
        &TessellationOptions {
            chord_tolerance: 1e-3,
        },
    )
    .unwrap();
    let hull = mesh
        .positions
        .iter()
        .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), q| {
            (lo.min(*q), hi.max(*q))
        });
    Body {
        volume: mass_properties(&mesh).volume,
        hull,
        faces: solid.faces.len(),
        edges: solid.edges.len(),
    }
}

#[test]
fn old_profile_numbers_give_the_same_bodies() {
    let (mut checked, mut several, mut lost_checked) = (0, 0, 0);
    for (name, s, lost) in scenarios() {
        let legacy = s.legacy_profiles();
        let now = s.profiles();
        if lost {
            assert!(!legacy.profiles.is_empty(), "{name}");
            for n in 0..legacy.profiles.len() {
                assert_eq!(s.migrate_profile_index(n), None, "{name}: Profil {n}");
                lost_checked += 1;
            }
            continue;
        }
        for (n, old) in legacy.profiles.iter().enumerate() {
            let refs = s
                .migrate_profile_index(n)
                .unwrap_or_else(|| panic!("{name}: altes Profil {n} ohne Flächen"));
            let sel = now
                .resolve(&refs)
                .unwrap_or_else(|i| panic!("{name}: Verweis {i} von Profil {n} verloren"));
            several += usize::from(sel.len() > 1);
            let merged = now.merged(&sel);
            assert_eq!(merged.len(), 1, "{name}: Profil {n}");
            let (a, b) = (body(&s, old), body(&s, &merged[0]));
            assert!(
                ((a.volume - b.volume) / a.volume).abs() < 1e-9,
                "{name}: Profil {n}: Volumen {} statt {}",
                b.volume,
                a.volume
            );
            assert!(
                a.hull.0.distance(b.hull.0) < 1e-9 && a.hull.1.distance(b.hull.1) < 1e-9,
                "{name}: Profil {n}: Hülle {:?} statt {:?}",
                b.hull,
                a.hull
            );
            assert_eq!(a.faces, b.faces, "{name}: Profil {n}: Flächen");
            assert_eq!(a.edges, b.edges, "{name}: Profil {n}: Kanten");
            assert_eq!(s.legacy_index_of(&refs), Some(n), "{name}: Profil {n}");
            checked += 1;
        }
        assert_eq!(
            s.migrate_profile_index(legacy.profiles.len()),
            None,
            "{name}"
        );
    }
    assert!(checked >= 18, "{checked}");
    // T-Stoß (früher offene Linie) und offene Linien quer durch Rechteck, Kreis, Ellipse und
    // Spline-Kante
    assert_eq!(several, 5);
    assert_eq!(lost_checked, 1);
}

#[test]
fn an_open_line_across_an_old_profile_gives_two_faces_and_the_same_body() {
    let s = crossed_rectangle();
    assert_eq!(s.legacy_profiles().profiles.len(), 1);
    assert_eq!(s.profiles().profiles.len(), 2);
    let refs = s.migrate_profile_index(0).unwrap();
    assert_eq!(refs.len(), 2);
    let now = s.profiles();
    let merged = now.merged(&now.resolve(&refs).unwrap());
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0], s.legacy_profiles().profiles[0]);
    assert_eq!(s.legacy_index_of(&refs), Some(0));
    assert_eq!(s.legacy_index_of(&refs[..1]), None);
}

/// Verweise aus der berechneten Skizze weichen nach Formeln oder erneutem Lösen in den letzten
/// Stellen ab: `legacy_index_of` vergleicht die gewählten Flächen, nicht die Zahlen.
#[test]
fn legacy_index_of_compares_the_selected_faces() {
    let s = crossed_rectangle();
    let refs = s.migrate_profile_index(0).unwrap();
    let mut nudged = refs.clone();
    for r in &mut nudged {
        for p in r.outer.iter_mut().chain(&mut r.holes) {
            p.u0 += 1e-12;
            p.u1 -= 1e-12;
            p.len *= 1.0 + 1e-12;
        }
    }
    assert_ne!(nudged, refs);
    assert_eq!(s.legacy_index_of(&nudged), Some(0));
    // Reihenfolge egal, ein Teil oder doppelte Flächen nicht
    let reversed: Vec<_> = refs.iter().rev().cloned().collect();
    assert_eq!(s.legacy_index_of(&reversed), Some(0));
    assert_eq!(s.legacy_index_of(&nudged[..1]), None);
    let twice = vec![refs[0].clone(), refs[0].clone(), refs[1].clone()];
    assert_eq!(s.legacy_index_of(&twice), None);
    assert_eq!(s.legacy_index_of(&[]), None);
}

/// Einmal berechnet, für viele Features derselben Skizze: dasselbe Ergebnis wie die
/// Einzelaufrufe.
#[test]
fn the_migration_of_a_sketch_matches_the_single_calls() {
    for (name, s, _) in scenarios() {
        let m = s.legacy_migration();
        let legacy = s.legacy_profiles().profiles.len();
        for n in 0..=legacy {
            let one = s.migrate_profile_index(n);
            assert_eq!(m.migrate(n), one.as_deref(), "{name}: Profil {n}");
            let resolves = one
                .as_ref()
                .is_some_and(|r| s.profiles().resolve(r).is_ok());
            assert_eq!(m.resolvable(n).is_some(), resolves, "{name}: Profil {n}");
            if let Some(refs) = one {
                assert_eq!(m.legacy_index_of(&refs), Some(n), "{name}: Profil {n}");
            }
        }
    }
}

// ---- Verweise nach dem Stutzen (Gesamtprüfung) --------------------------------------------

/// Index des Profils, dessen Außenrand im Mittel links (`left`) bzw. rechts von x = 20 liegt
/// und dessen Fläche `area` ± 2 ist.
fn face_at(e: &planora_sketch::ExtractedProfiles, left: bool, area: f64) -> usize {
    (0..e.profiles.len())
        .find(|&i| {
            let o = &e.outlines[i][0];
            let c = o.iter().fold(DVec2::ZERO, |a, p| a + *p) / o.len() as f64;
            (c.x < 20.0) == left && (e.profiles[i].outer.signed_area().abs() - area).abs() < 2.0
        })
        .unwrap_or_else(|| panic!("keine Fläche links={left} mit {area}"))
}

/// Rechteck (0,0)–(20,10) mit einer Kurve um (20, 5), die die rechte Kante kreuzt; der äußere
/// Teil wird gestutzt. Die innere Hälfte und die restliche Fläche des Rechtecks bleiben
/// dieselben Flächen.
fn refs_survive_trimming_the_outer_part(
    add: impl FnOnce(&mut Sketch, PointId) -> CurveId,
    half: f64,
) {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 10.0);
    let m = s.add_point(20.0, 5.0).unwrap();
    let c = add(&mut s, m);
    let before = s.profiles();
    assert_eq!(before.profiles.len(), 3);
    let inner = before.refs[face_at(&before, true, half)].clone();
    let rest = before.refs[face_at(&before, true, 200.0 - half)].clone();
    s.trim(c, DVec2::new(22.9, 5.0)).unwrap();
    let after = s.profiles();
    assert_eq!(after.profiles.len(), 2);
    assert_eq!(after.find(&inner), Some(face_at(&after, true, half)));
    assert_eq!(after.find(&rest), Some(face_at(&after, true, 200.0 - half)));
}

#[test]
fn refs_survive_trimming_a_circle_to_an_arc() {
    refs_survive_trimming_the_outer_part(|s, m| s.add_circle(m, 3.0).unwrap(), PI * 4.5);
}

#[test]
fn refs_survive_trimming_an_ellipse_to_an_elliptic_arc() {
    // Halbachsen 3 (waagrecht) und 2: halbe Fläche π · 3 · 2 / 2
    refs_survive_trimming_the_outer_part(
        |s, m| s.add_ellipse(m, DVec2::new(3.0, 0.0), 2.0).unwrap(),
        PI * 3.0,
    );
}
