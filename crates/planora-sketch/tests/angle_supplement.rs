//! Winkelmaß mit Gegenwinkel (Skizzen 11a, Task 6, A7): Messwert `π − θ`, Treiben der Geometrie,
//! Wahl durch den Zeiger, nur an `Angle`, in 3D, und das alte Winkelmaß bleibt bitgleich.

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::dimension::{DimPick, DimPlan};
use planora_sketch::model::{CurveId, PointId, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn line(s: &mut Sketch, a: DVec2, b: DVec2) -> (CurveId, PointId, PointId) {
    let (pa, pb) = (
        s.add_point(a.x, a.y).unwrap(),
        s.add_point(b.x, b.y).unwrap(),
    );
    (s.add_line(pa, pb).unwrap(), pa, pb)
}

fn line3(s: &mut Sketch, a: DVec3, b: DVec3) -> CurveId {
    let (pa, pb) = (s.add_point3(a).unwrap(), s.add_point3(b).unwrap());
    s.add_line(pa, pb).unwrap()
}

/// Linien durch den Ursprung: eine längs x, eine unter 60°.
fn lines_0_and_60() -> (Sketch, CurveId, CurveId) {
    let mut s = sketch();
    let (l1, ..) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let (l2, ..) = line(&mut s, v(0.0, 0.0), v(5.0, 5.0 * 3f64.sqrt()));
    (s, l1, l2)
}

#[test]
fn supplement_measures_pi_minus_theta() {
    let (s, l1, l2) = lines_0_and_60();
    let theta = s.measure_angle(l1, l2, false).unwrap();
    assert!((theta - 60f64.to_radians()).abs() < 1e-12);
    let sup = s.measure_angle(l1, l2, true).unwrap();
    assert!((sup - 120f64.to_radians()).abs() < 1e-12, "{sup}");
    // `measure` misst wie bisher ohne Gegenwinkel; `measure_constraint` beachtet das Feld.
    assert_eq!(
        s.measure(&K::Angle(l1, l2, 0.0)).unwrap().to_bits(),
        theta.to_bits()
    );
    let mut t = s.clone();
    let id = t
        .add_constraint_supplement(K::Angle(l1, l2, 0.0), true)
        .unwrap();
    assert!(t.constraint(id).unwrap().supplement());
    assert!((t.measure_constraint(id).unwrap() - sup).abs() < 1e-12);
    let plain = t.add_constraint(K::Angle(l1, l2, 0.0)).unwrap();
    assert!(!t.constraint(plain).unwrap().supplement());
}

#[test]
fn supplement_drives_geometry() {
    let (mut s, l1, l2) = lines_0_and_60();
    let id = s
        .add_constraint_supplement(K::Angle(l1, l2, 60f64.to_radians()), true)
        .unwrap();
    // Gemessen ist 120°, der Wert 60° treibt sie also um; zurück auf 100°.
    s.set_constraint_value(id, 100f64.to_radians()).unwrap();
    assert!(s.constraint(id).unwrap().supplement());
    let r = s.solve();
    assert!(r.conflicting.is_empty(), "{r:?}");
    let plain = s.measure(&K::Angle(l1, l2, 0.0)).unwrap();
    assert!((plain - 80f64.to_radians()).abs() < 1e-9, "{plain}");
    assert!((s.measure_constraint(id).unwrap() - 100f64.to_radians()).abs() < 1e-9);
    // Rückgängig über die Rohdaten behält das Feld.
    let back = Sketch::from_data_counted(s.to_data()).unwrap().0;
    assert!(back.constraint(id).unwrap().supplement());
}

fn plan(s: &Sketch, l1: CurveId, l2: CurveId, pointer: DVec2) -> DimPlan {
    s.dimension_plan(
        &[
            DimPick::Curve(l1, v(5.0, 0.0)),
            DimPick::Curve(l2, v(2.0, 2.0)),
        ],
        pointer,
    )
    .unwrap()
}

#[test]
fn plan_picks_supplement_by_pointer() {
    let mut s = sketch();
    let (l1, ..) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let (l2, ..) = line(&mut s, v(0.0, 0.0), v(0.0, 10.0));
    let got: Vec<bool> = [v(5.0, 5.0), v(-5.0, 5.0), v(-5.0, -5.0), v(5.0, -5.0)]
        .iter()
        .map(|&p| plan(&s, l1, l2, p).supplement)
        .collect();
    assert_eq!(got, [false, true, false, true]);
    // Schiefe Linien (60°): Feld zwischen +d1 und +d2 ist der 60°-Keil.
    let (s, l1, l2) = lines_0_and_60();
    let inside = plan(&s, l1, l2, v(8.0, 5.0));
    assert!(!inside.supplement);
    let outside = plan(&s, l1, l2, v(-8.0, 5.0));
    assert!(outside.supplement);
    let K::Angle(a, b, val) = outside.kind else {
        panic!("{:?}", outside.kind)
    };
    assert!((val - 120f64.to_radians()).abs() < 1e-12);
    assert!((s.measure_angle(a, b, true).unwrap() - val).abs() < 1e-12);
    // Das Anlegen mit dem Plan bewegt nichts.
    let mut c = s.clone();
    let before = c.variables().to_vec();
    c.add_constraint_supplement(outside.kind.clone(), outside.supplement)
        .unwrap();
    let r = c.solve();
    assert_eq!(r.iterations, 0, "{r:?}");
    assert_eq!(c.variables(), &before[..]);
}

#[test]
fn plan_on_line_takes_angle() {
    let mut s = sketch();
    let (l1, ..) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let (l2, ..) = line(&mut s, v(0.0, 0.0), v(0.0, 10.0));
    for p in [v(5.0, 0.0), v(-5.0, 0.0), v(0.0, 5.0), v(0.0, 0.0)] {
        assert!(!plan(&s, l1, l2, p).supplement, "{p:?}");
    }
}

#[test]
fn supplement_only_on_angle() {
    let mut s = sketch();
    let (l1, a, b) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let rev = s.revision();
    assert!(
        s.add_constraint_supplement(K::Distance(a, b, 10.0), true)
            .is_err()
    );
    assert!(
        s.add_constraint_supplement(
            K::Horizontal(planora_sketch::constraint::LineOrPoints::Line(l1)),
            true
        )
        .is_err()
    );
    assert_eq!(s.revision(), rev);
    assert!(
        s.add_constraint_supplement(K::Distance(a, b, 10.0), false)
            .is_ok()
    );
}

#[test]
fn supplement_3d_lines() {
    let mut s = sketch();
    let (c, sn) = (50f64.to_radians().cos(), 50f64.to_radians().sin());
    let l1 = line3(
        &mut s,
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(10.0, 0.0, 1.0),
    );
    let l2 = line3(
        &mut s,
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(10.0 * c, 10.0 * sn, 1.0),
    );
    assert!((s.measure_angle(l1, l2, false).unwrap() - 50f64.to_radians()).abs() < 1e-12);
    assert!((s.measure_angle(l1, l2, true).unwrap() - 130f64.to_radians()).abs() < 1e-12);
    let id = s
        .add_constraint_supplement(K::Angle(l1, l2, 130f64.to_radians()), true)
        .unwrap();
    s.set_constraint_value(id, 110f64.to_radians()).unwrap();
    let r = s.solve();
    assert!(r.conflicting.is_empty(), "{r:?}");
    assert!((s.measure_angle(l1, l2, false).unwrap() - 70f64.to_radians()).abs() < 1e-9);
    assert!((s.measure_constraint(id).unwrap() - 110f64.to_radians()).abs() < 1e-9);
}

/// Das Winkelmaß ohne Gegenwinkel ist bitgleich zum Stand vor der Aufgabe. Die festen Werte
/// wurden vor der Änderung der Auswertung gemessen (Ruling F14): Residuum bei v = 0,5 und
/// Messwert zweier schiefer Linien, 2D und 3D.
#[test]
fn old_angle_unchanged() {
    let mut s = sketch();
    let (l1, ..) = line(&mut s, v(1.0, 2.0), v(11.0, 3.5));
    let (l2, ..) = line(&mut s, v(-2.0, 0.5), v(4.0, 8.0));
    let id = s.add_constraint(K::Angle(l1, l2, 0.5)).unwrap();
    let r = s.evaluate(id, s.variables()).unwrap().residuals[0].v;
    let m = s.measure(&K::Angle(l1, l2, 0.0)).unwrap();
    let mc = s.measure_constraint(id).unwrap();
    assert_eq!(m.to_bits(), mc.to_bits());
    let mut t = sketch();
    let k1 = line3(
        &mut t,
        DVec3::new(1.0, 2.0, 0.5),
        DVec3::new(11.0, 3.5, 2.0),
    );
    let k2 = line3(
        &mut t,
        DVec3::new(-2.0, 0.5, 1.0),
        DVec3::new(4.0, 8.0, -3.0),
    );
    let id3 = t.add_constraint(K::Angle(k1, k2, 0.5)).unwrap();
    let r3 = t.evaluate(id3, t.variables()).unwrap().residuals[0].v;
    let m3 = t.measure(&K::Angle(k1, k2, 0.0)).unwrap();
    assert_eq!(
        [r.to_bits(), m.to_bits(), r3.to_bits(), m3.to_bits()],
        [
            0x3fcf50e7b046caac,
            0x3fe7e8c77d8e7c5d,
            0x3fd987a8233202c6,
            0x3fed215c170b118c
        ]
    );
}

/// Die Zeigerregel gilt auch an Linien im Raum (Lage des Zeigers in der Skizzenebene).
#[test]
fn plan_picks_supplement_by_pointer_3d() {
    let mut s = sketch();
    let l1 = line3(
        &mut s,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(10.0, 0.0, 2.0),
    );
    let l2 = line3(
        &mut s,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(0.0, 10.0, 2.0),
    );
    let got: Vec<bool> = [v(5.0, 5.0), v(-5.0, 5.0), v(-5.0, -5.0), v(5.0, -5.0)]
        .iter()
        .map(|&p| plan(&s, l1, l2, p).supplement)
        .collect();
    assert_eq!(got, [false, true, false, true]);
}
