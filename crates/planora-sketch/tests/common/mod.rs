//! Gemeinsame Aufbauten für die Löser-Messtests (Skizzen 11a).
#![allow(dead_code)]

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{PointId, Sketch};

/// Rechteck aus vier Linien mit gemeinsamen Eckpunkten, leicht schief gestartet, mit zwei Maßen
/// (40 × 30); die Verschiebung bleibt frei.
pub fn rect_with_dims() -> Sketch {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let p = [
        s.add_point(1.0, 2.0).unwrap(),
        s.add_point(37.0, 3.0).unwrap(),
        s.add_point(41.0, 33.0).unwrap(),
        s.add_point(-2.0, 29.0).unwrap(),
    ];
    let l: Vec<_> = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l[0])))
        .unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(l[1])))
        .unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l[2])))
        .unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(l[3])))
        .unwrap();
    s.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(p[1], p[2], 30.0)).unwrap();
    s
}

/// Alle Punkte als Bitmuster ihrer Koordinaten, zum bitgenauen Vergleich zweier Skizzen.
pub fn values_bits(s: &Sketch) -> Vec<(u64, u64)> {
    s.points()
        .map(|(id, _)| {
            let p = s.point_xy(id).unwrap();
            (p.x.to_bits(), p.y.to_bits())
        })
        .collect()
}

/// Dieselbe Kette wie [`chain_3d`], aber in der Skizzenebene: Die Richtung wechselt x → y → −x → −y,
/// die Bögen drehen um ein Viertel gegen den Uhrzeigersinn (`add_arc`). Coincident und Tangent wie
/// dort, die gleiche Störung in x und y.
pub fn chain_2d(s: &mut Sketch, segments: usize) -> (PointId, usize) {
    let dirs = [DVec2::X, DVec2::Y, -DVec2::X, -DVec2::Y];
    let noise = |i: usize, k: usize| {
        let f = (i * 7 + k * 13) as f64;
        DVec2::new(f.sin(), (f * 1.7).cos()) * 1e-3
    };
    let mut pos = DVec2::ZERO;
    let mut prev_end: Option<PointId> = None;
    let mut prev_arc = None;
    let mut last = None;
    let mut curves = 0;
    let mut k = 0;
    let mut p2 = |s: &mut Sketch, p: DVec2| {
        k += 1;
        let q = p + noise(k, k / 3);
        s.add_point(q.x, q.y).unwrap()
    };
    for i in 0..segments {
        let h = dirs[i % 4];
        let h2 = dirs[(i + 1) % 4];
        let a = p2(s, pos);
        let b_pos = pos + 2.0 * h;
        let b = p2(s, b_pos);
        let line = s.add_line(a, b).unwrap();
        curves += 1;
        if let (Some(e), Some(arc)) = (prev_end, prev_arc) {
            s.add_constraint(K::Coincident(e, a)).unwrap();
            s.add_constraint(K::Tangent(arc, line)).unwrap();
        }
        // Viertelbogen von b: Mitte b + h2, Ende b + h + h2.
        let st = p2(s, b_pos);
        let m = p2(s, b_pos + h2);
        let e = p2(s, b_pos + h + h2);
        let arc = s.add_arc(m, st, e).unwrap();
        curves += 1;
        s.add_constraint(K::Coincident(b, st)).unwrap();
        s.add_constraint(K::Tangent(line, arc)).unwrap();
        prev_end = Some(e);
        prev_arc = Some(arc);
        last = Some(e);
        pos = b_pos + h + h2;
    }
    (last.unwrap(), curves)
}

/// Kette aus `segments` Viertelkreis-Stücken im Raum: Linie (Länge 2), dann ein Viertelbogen
/// (Radius 1), der die Richtung x → y → z → x → … dreht. Linien und Bögen haben getrennte
/// Punkte, die mit `Coincident` verbunden sind; an jedem Übergang gilt `Tangent`. Die Punkte
/// werden leicht verstellt, damit der Löser wirklich rechnen muss.
pub fn chain_3d(s: &mut Sketch, segments: usize) -> (planora_sketch::model::PointId, usize) {
    let axes = [DVec3::X, DVec3::Y, DVec3::Z];
    let noise = |i: usize, k: usize| {
        let f = (i * 7 + k * 13) as f64;
        DVec3::new(f.sin(), (f * 1.7).cos(), (f * 2.3).sin()) * 1e-3
    };
    let mut pos = DVec3::ZERO;
    let mut prev_end: Option<planora_sketch::model::PointId> = None;
    let mut prev_arc = None;
    let mut last = None;
    let mut curves = 0;
    let mut k = 0;
    let mut p3 = |s: &mut Sketch, p: DVec3| {
        k += 1;
        s.add_point3(p + noise(k, k / 3)).unwrap()
    };
    for i in 0..segments {
        let h = axes[i % 3];
        let h2 = axes[(i + 1) % 3];
        // Linie von `pos` in Richtung h.
        let a = p3(s, pos);
        let b_pos = pos + 2.0 * h;
        let b = p3(s, b_pos);
        let line = s.add_line(a, b).unwrap();
        curves += 1;
        if let (Some(e), Some(arc)) = (prev_end, prev_arc) {
            s.add_constraint(K::Coincident(e, a)).unwrap();
            s.add_constraint(K::Tangent(arc, line)).unwrap();
        }
        // Viertelbogen von b: Mitte b + h2, Ende b + h + h2.
        let st = p3(s, b_pos);
        let m = p3(s, b_pos + h2);
        let e = p3(s, b_pos + h + h2);
        let arc = s.add_arc3(m, st, e, h.cross(h2)).unwrap();
        curves += 1;
        s.add_constraint(K::Coincident(b, st)).unwrap();
        s.add_constraint(K::Tangent(line, arc)).unwrap();
        prev_end = Some(e);
        prev_arc = Some(arc);
        last = Some(e);
        pos = b_pos + h + h2;
    }
    (last.unwrap(), curves)
}
