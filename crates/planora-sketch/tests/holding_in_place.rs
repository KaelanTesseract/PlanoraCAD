//! C12 (2026-11-23): `solve_holding` und `solve_holding_connected` rechnen an Ort und Stelle statt
//! auf einer Kopie der Skizze. Ergebnis, Werte, Aufbau und Stand müssen dem alten Weg
//! (`*_cloning`) gleichen, bei Erfolg wie bei Scheitern.

// Zählender Speicher-Allocator für die Messung (siehe `Counting`).
#![allow(unsafe_code)]

use std::time::Instant;

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternId, PatternKind, PatternPlan};
use planora_sketch::solver::SolveStatus;
use planora_sketch::text::{HAlign, TextParams, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_sketch::{ConstraintId, ValueSnapshot};
use std::sync::Arc;

/// Zählt die Speicheranforderungen dieses Test-Programms (eigenes Programm je Testdatei), damit
/// die Messung „an Ort und Stelle gegen Kopie“ nicht an der Uhr hängt (offener Punkt C12: Die
/// Zeitmessung wackelte unter WSL um 0,2 ms).
struct Counting;

thread_local! {
    /// Angeforderte Bytes je Faden (Tests laufen nebeneinander in eigenen Fäden).
    static ALLOCATED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let _ = ALLOCATED.try_with(|a| a.set(a.get() + layout.size() as u64));
        // SAFETY: an den Systemspeicher weitergereicht, gleiche Anforderung.
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: Zeiger und Anforderung stammen aus `alloc` oben.
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Bytes, die `f` auf diesem Faden anfordert.
fn allocated_by<R>(f: impl FnOnce() -> R) -> (u64, R) {
    let before = ALLOCATED.with(|a| a.get());
    let r = f();
    (ALLOCATED.with(|a| a.get()) - before, r)
}

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

/// Rechteck mit Breitenmaß; `[links unten, rechts unten, rechts oben, links oben]`, Maß.
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

/// Führt `solve_holding` neu und alt auf zwei Kopien aus und vergleicht alles.
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
            "{what}: gelöst braucht einen neuen Stand"
        );
    } else {
        assert_eq!(new.revision(), rev, "{what}: gescheitert behält den Stand");
        assert_eq!(new.to_data(), data, "{what}: gescheitert ändert nichts");
    }
    // Danach löst die Skizze wie die alte (Lösungsspeicher und Gruppen passen).
    let (rn, ro) = (new.solve(), old.solve());
    assert_eq!(rn.status, ro.status, "{what}: Status danach");
    assert_eq!(rn.dof, ro.dof, "{what}: Freiheitsgrade danach");
    assert_eq!(bits(&new), bits(&old), "{what}: Werte nach dem Lösen");
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
    // Leere Liste: gewöhnliches Lösen.
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
    // Ein gelöschter Punkt: gescheitert, unverändert.
    let mut t = s.clone();
    let q = t.add_point(50.0, 50.0).unwrap();
    t.remove_point(q).unwrap();
    assert!(!same_as_cloning(&t, &[p[0], q], false, "gelöschter Punkt"));
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
    // Ein Widerspruch an anderer Stelle zählt nicht.
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

/// Messung (C12): große Skizze mit Muster, ein Maß an einem freien Rechteck, viele Läufe mit
/// zwei gehaltenen Ecken. Druckt Zeiten und Bytes; neu fordert weniger Speicher an.
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
    let (old_bytes, (old_ms, old_bits)) = allocated_by(|| run(true));
    let (new_bytes, (new_ms, new_bits)) = allocated_by(|| run(false));
    eprintln!(
        "solve_holding 50 Läufe: Kopie {old_ms:.1} ms / {old_bytes} B,          an Ort und Stelle {new_ms:.1} ms / {new_bytes} B"
    );
    assert_eq!(new_bits, old_bits);
    // Die Zeit wird nur ausgegeben (sie wackelt, wenn das Lösen überwiegt); entschieden wird
    // an den angeforderten Bytes: ohne die Kopie je Lauf deutlich weniger.
    assert!(new_bytes < old_bytes, "{new_bytes} B gegen {old_bytes} B");
}

/// Messung (C12): Sicherung je Bild beim Kurvenzug, Kopie der Skizze gegen Wertestand.
#[test]
#[ignore]
fn a_value_snapshot_is_cheaper_than_a_copy() {
    let mut s = sketch();
    let mut sources = Vec::new();
    for i in 0..20 {
        let x = i as f64 * 12.0;
        sources.extend(s.add_rectangle(v(x, 0.0), v(x + 10.0, 6.0)).unwrap().lines);
    }
    s.add_pattern(&plan(sources, [10, 4], [300.0, 10.0]))
        .unwrap();
    s.solve();
    let t0 = Instant::now();
    for _ in 0..200 {
        std::hint::black_box(s.clone());
    }
    let copy_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let t0 = Instant::now();
    for _ in 0..200 {
        std::hint::black_box(s.value_snapshot());
    }
    let snap_ms = t0.elapsed().as_secs_f64() * 1000.0;
    eprintln!("200 Sicherungen: Kopie {copy_ms:.2} ms, Wertestand {snap_ms:.2} ms");
    assert!(snap_ms < copy_ms, "{snap_ms:.2} ms gegen {copy_ms:.2} ms");
}

// ---- Mit Text (Prüferfund C12): Die Nachführung kann den Aufbau ändern ----------------------

struct SquareShaper;

impl TextShaper for SquareShaper {
    fn shape(&self, content: &str, _style: &TextStyle) -> Result<TextShape, TextShapeError> {
        let glyphs: Vec<Glyph> = content
            .chars()
            .enumerate()
            .map(|(i, _)| Glyph {
                origin: v(i as f64 * 0.8, 0.0),
                advance: 0.8,
                contours: vec![Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(0.6, 0.0)),
                        Seg::Line(v(0.6, 0.7)),
                        Seg::Line(v(0.0, 0.7)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                }],
            })
            .collect();
        let width = glyphs.len() as f64 * 0.8;
        Ok(TextShape {
            lines: vec![ShapedLine { glyphs, width }],
            ascender: 0.9,
            descender: -0.2,
            line_height: 1.2,
            substituted: None,
            synthetic: false,
            missing_chars: false,
        })
    }
}

#[test]
fn with_a_text_holding_takes_the_copy_and_matches_it() {
    let mut s = sketch();
    let (p, dim) = measured_rect(&mut s, 0.0);
    assert!(s.only_values_change_when_solving());
    let params = TextParams {
        content: "AB".into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    };
    let shape = Arc::new(SquareShaper.shape("AB", &params.style).unwrap());
    s.add_text_box(
        v(40.0, 0.0),
        v(70.0, 10.0),
        params,
        HAlign::Left,
        VAlign::Bottom,
        shape,
    )
    .unwrap();
    assert!(!s.only_values_change_when_solving());
    s.set_constraint_value(dim, 13.0).unwrap();
    assert!(same_as_cloning(&s, &[p[0], p[3]], false, "Text"));
    assert!(same_as_cloning(&s, &[p[0], p[3]], true, "Text, verbunden"));
    assert!(!same_as_cloning(&s, &p, false, "Text, alle fest"));
}

// ---- Wächter und Halbieren ohne Kopie (offene Punkte C12, 2026-11-24) -----------------------

/// Eine Panik mitten im Halten (Festpunkte angehängt, Toleranz verschärft, Zeilen abgeschaltet)
/// lässt die Skizze bitgleich zurück, auch im Weg über verbundene Gruppen; die Panik geht weiter.
#[test]
fn a_panic_while_holding_restores_the_sketch() {
    let mut s = sketch();
    let (p, dim) = measured_rect(&mut s, 0.0);
    measured_rect(&mut s, 40.0);
    s.set_constraint_value(dim, 14.0).unwrap();
    for connected in [false, true] {
        let (before, data, rev) = (bits(&s), s.to_data(), s.revision());
        Sketch::panic_in_next_hold_for_tests();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if connected {
                s.solve_holding_connected_for_test(&[p[0], p[3]])
            } else {
                s.solve_holding(&[p[0], p[3]])
            }
        }));
        assert!(r.is_err(), "Panik geht weiter");
        assert_eq!(bits(&s), before, "Werte");
        assert_eq!(
            s.to_data(),
            data,
            "Aufbau (Festpunkte, abgeschaltete Zeilen)"
        );
        assert_eq!(s.revision(), rev, "Stand");
        // Danach läuft alles wie vorher.
        assert!(same_as_cloning(
            &s,
            &[p[0], p[3]],
            connected,
            "nach der Panik"
        ));
    }
}

/// `solve_holding_in_steps` rechnet ohne Kopie und gleicht dem alten Weg bitgenau, bei Erfolg
/// wie bei Scheitern; gescheitert bleibt die Skizze unverändert, das Maß steht wieder auf `to`.
#[test]
fn holding_in_steps_matches_the_copy() {
    // Gelenkviereck: a, b gehalten, c und d frei, drei Seiten 10 lang, Winkel bei b als Maß.
    // Große Sprünge auf fast gestreckte Lagen scheitern direkt und gehen über das Halbieren.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let c = s.add_point(10.0, 10.0).unwrap();
    let d = s.add_point(0.0, 10.0).unwrap();
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    s.add_line(c, d).unwrap();
    s.add_line(d, a).unwrap();
    s.add_constraint(K::Distance(b, c, 10.0)).unwrap();
    s.add_constraint(K::Distance(c, d, 10.0)).unwrap();
    s.add_constraint(K::Distance(d, a, 10.0)).unwrap();
    let from = std::f64::consts::FRAC_PI_2;
    let ang = s.add_constraint(K::Angle(ab, bc, from)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let mut halved = 0;
    for to in [0.3, 2.9, 3.1, 0.05, 1.6, 2.5, 6.0, -1.0] {
        let (mut new, mut old) = (s.clone(), s.clone());
        if new.set_constraint_value(ang, to).is_err() {
            continue;
        }
        old.set_constraint_value(ang, to).unwrap();
        let (data, rev) = (new.to_data(), new.revision());
        let direct = new.clone().solve_holding(&[a, b]);
        let ok_new = new.solve_holding_in_steps(ang, &[a, b], from, to);
        let ok_old = old.solve_holding_in_steps_cloning(ang, &[a, b], from, to);
        if !direct && ok_new {
            halved += 1;
        }
        assert_eq!(ok_new, ok_old, "{to}: Ergebnis");
        assert_eq!(bits(&new), bits(&old), "{to}: Werte");
        assert_eq!(new.to_data(), old.to_data(), "{to}: Aufbau");
        if !ok_new {
            assert_eq!(new.to_data(), data, "{to}: gescheitert ändert nichts");
            assert_eq!(new.revision(), rev, "{to}: gescheitert behält den Stand");
        }
    }
    eprintln!("{halved} Sprünge gingen über das Halbieren");
}
