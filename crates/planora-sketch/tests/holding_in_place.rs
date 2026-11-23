//! C12 (2026-11-23): `solve_holding` und `solve_holding_connected` rechnen an Ort und Stelle statt
//! auf einer Kopie der Skizze. Ergebnis, Werte, Aufbau und Stand mÃƒÂ¼ssen dem alten Weg
//! (`*_cloning`) gleichen, bei Erfolg wie bei Scheitern.

use std::time::Instant;

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternId, PatternKind, PatternPlan};
use planora_sketch::solver::SolveStatus;
use planora_sketch::{ConstraintId, ValueSnapshot};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn plan(sources: Vec<CurveId>, counts: [u32; 2], values: [f64; 2]) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources,
        counts,
        values,
        suppressed: vec![],
    }
}

/// Rechteck mit BreitenmaÃƒÅ¸; `[links unten, rechts unten, rechts oben, links oben]`, MaÃƒÅ¸.
fn measured_rect(s: &mut Sketch, x: f64) -> ([PointId; 4], ConstraintId) {
    let r = s.add_rectangle(v(x, 0.0), v(x + 10.0, 6.0)).unwrap();
    let dim = s
        .add_constraint(K::Distance(r.points[0], r.points[1], 10.0))
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    (r.points, dim)
}

/// Bits aller Werte.
fn bits(s: &Sketch) -> Vec<u64> {
    s.variables().iter().map(|x| x.to_bits()).collect()
}

/// FÃƒÂ¼hrt `solve_holding` neu und alt auf zwei Kopien aus und vergleicht alles.
fn same_as_cloning(s: &Sketch, hold: &[PointId], connected: bool, what: &str) -> bool {
    let (mut new, mut old) = (s.clone(), s.clone());
    let (rev, data) = (s.revision(), s.to_data());
    let (ok_new, ok_old) = if connected {
        (
            new.solve_holding_connected_for_test(hold),
            old.solve_holding_connected_cloning(hold),
        )
    } else {
        (new.solve_holding(hold), old.solve_holding_cloning(hold))
    };
    assert_eq!(ok_new, ok_old, "{what}: Ergebnis");
    assert_eq!(bits(&new), bits(&old), "{what}: Werte");
    assert_eq!(new.to_data(), old.to_data(), "{what}: Aufbau");
    if ok_new {
        assert_ne!(
            new.revision(),
            rev,
            "{what}: gelÃƒÂ¶st braucht einen neuen Stand"
        );
    } else {
        assert_eq!(new.revision(), rev, "{what}: gescheitert behÃƒÂ¤lt den Stand");
        assert_eq!(new.to_data(), data, "{what}: gescheitert ÃƒÂ¤ndert nichts");
    }
    // Danach lÃƒÂ¶st die Skizze wie die alte (LÃƒÂ¶sungsspeicher und Gruppen passen).
    let (rn, ro) = (new.solve(), old.solve());
    assert_eq!(rn.status, ro.status, "{what}: Status danach");
    assert_eq!(rn.dof, ro.dof, "{what}: Freiheitsgrade danach");
    assert_eq!(bits(&new), bits(&old), "{what}: Werte nach dem LÃƒÂ¶sen");
    ok_new
}

#[test]
fn holding_in_place_matches_the_copy() {
    let mut s = sketch();
    let (p, dim) = measured_rect(&mut s, 0.0);
    s.set_constraint_value(dim, 14.0).unwrap();
    // Zwei Ecken links fest: Die Breite geht.
    assert!(same_as_cloning(&s, &[p[0], p[3]], false, "links fest"));
    // Alle Ecken fest: Die Breite geht nicht.
    assert!(!same_as_cloning(&s, &p, false, "alle fest"));
    // Leere Liste: gewÃƒÂ¶hnliches LÃƒÂ¶sen.
    assert!(same_as_cloning(&s, &[], false, "ohne Halt"));
}

#[test]
fn a_refused_pin_leaves_the_sketch_as_it_was() {
    let mut s = sketch();
    let (p, dim) = measured_rect(&mut s, 0.0);
    let r = s.add_rectangle(v(30.0, 0.0), v(40.0, 6.0)).unwrap();
    let id: PatternId = s
        .add_pattern(&plan(r.lines.to_vec(), [3, 1], [15.0, 10.0]))
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let inst = s.pattern(id).unwrap().instances[1].points[0];
    s.set_constraint_value(dim, 12.0).unwrap();
    // Der erste Festpunkt geht, der zweite liegt auf einem Exemplar und wird abgelehnt: Der schon
    // angelegte erste muss wieder weg.
    assert!(!same_as_cloning(&s, &[p[0], inst], false, "Exemplar"));
    // Ein gelÃƒÂ¶schter Punkt: gescheitert, unverÃƒÂ¤ndert.
    let mut t = s.clone();
    let q = t.add_point(50.0, 50.0).unwrap();
    t.remove_point(q).unwrap();
    assert!(!same_as_cloning(&t, &[p[0], q], false, "gelÃƒÂ¶schter Punkt"));
}

#[test]
fn holding_a_3d_point_in_place_matches_the_copy() {
    let mut s = sketch();
    let a = s.add_point3(DVec3::new(0.0, 0.0, 5.0)).unwrap();
    let b = s.add_point3(DVec3::new(10.0, 0.0, 5.0)).unwrap();
    let dim = s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    s.solve();
    s.set_constraint_value(dim, 13.0).unwrap();
    assert!(same_as_cloning(&s, &[a], false, "3D"));
}

#[test]
fn holding_connected_in_place_matches_the_copy() {
    let mut s = sketch();
    let (p, dim) = measured_rect(&mut s, 0.0);
    // Ein Widerspruch an anderer Stelle zÃƒÂ¤hlt nicht.
    let (q, _) = measured_rect(&mut s, 40.0);
    let _ = s.add_constraint(K::Distance(q[0], q[1], 20.0));
    s.set_constraint_value(dim, 16.0).unwrap();
    assert!(same_as_cloning(&s, &[p[0], p[3]], true, "verbunden"));
    assert!(!same_as_cloning(&s, &p, true, "verbunden, alle fest"));
}

#[test]
fn a_value_snapshot_restores_values_and_revision() {
    let mut s = sketch();
    let (p, _) = measured_rect(&mut s, 0.0);
    let snap: ValueSnapshot = s.value_snapshot();
    let (before, rev) = (bits(&s), s.revision());
    s.set_point_xy(p[2], 30.0, 20.0).unwrap();
    s.solve();
    assert_ne!(bits(&s), before);
    assert_eq!(snap.variables().len(), before.len());
    s.restore_values(snap);
    assert_eq!(bits(&s), before);
    assert_eq!(s.revision(), rev);
}

/// Messung (C12): groÃƒÅ¸e Skizze mit Muster, ein MaÃƒÅ¸ an einem freien Rechteck, viele LÃƒÂ¤ufe mit
/// zwei gehaltenen Ecken. Druckt beide Zeiten; neu darf nicht langsamer sein.
#[test]
#[ignore]
fn holding_in_place_is_not_slower_on_a_large_sketch() {
    let mut s = sketch();
    let mut sources = Vec::new();
    for i in 0..20 {
        let x = i as f64 * 12.0;
        sources.extend(s.add_rectangle(v(x, 0.0), v(x + 10.0, 6.0)).unwrap().lines);
    }
    s.add_pattern(&plan(sources, [10, 4], [300.0, 10.0]))
        .unwrap();
    let (p, dim) = measured_rect(&mut s, -50.0);
    let run = |cloning: bool| {
        let mut t = s.clone();
        let t0 = Instant::now();
        for i in 0..50 {
            t.set_constraint_value(dim, 10.0 + (i % 5) as f64).unwrap();
            let ok = if cloning {
                t.solve_holding_cloning(&[p[0], p[3]])
            } else {
                t.solve_holding(&[p[0], p[3]])
            };
            assert!(ok);
        }
        (t0.elapsed().as_secs_f64() * 1000.0, bits(&t))
    };
    let (old_ms, old_bits) = run(true);
    let (new_ms, new_bits) = run(false);
    eprintln!("solve_holding 50 LÃƒÂ¤ufe: Kopie {old_ms:.1} ms, an Ort und Stelle {new_ms:.1} ms");
    assert_eq!(new_bits, old_bits);
    assert!(new_ms < old_ms, "{new_ms:.1} ms gegen {old_ms:.1} ms");
}
