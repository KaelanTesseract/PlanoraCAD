//! Skizzen 11a, Schlussprüfung I-1: Löschen einer Kurve nimmt ihre freien Enden samt Maßen mit
//! (Nacharbeit H2), aber keine geschützten Punkte: eingezogene Punkte (Ziele einer Kopie) bleiben
//! mit Fixierung und Verknüpfung, ebenso Punkte, die noch eine andere Kurve benutzt, und die Mitte
//! eines Kreismusters. Entf in der Ansicht und das X der Liste „Unterbestimmte finden“ verhalten
//! sich gleich. Bei 1920 × 1080, ohne Fenster.

use egui::{Event, Key, Modifiers, Rect, pos2};
use planora_app::sketch_state::Item;
use planora_app::sketch_view;
use planora_app::state::{AppState, IncludePick, StandardPlane};
use planora_app::{fonts, theme};
use planora_feature::{CopyTarget, SketchPlane};
use planora_math::DVec2;
use planora_sketch::check::CheckItem;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{CircularMode, PatternKind, PatternPlan};

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};

/// Offene, leere Skizze auf XY mit leerem Verlauf.
fn open_sketch() -> AppState {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    s
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

/// Skizze mit eingezogenem Ursprung (Punkt mit Kopie und Fixierung) und einer Linie des Nutzers
/// von dort nach (30, 10), an deren freiem Ende ein Maß hängt. Liefert Ursprung, Linie, freies Ende.
fn included_origin_with_line() -> (AppState, PointId, CurveId, PointId) {
    let mut s = open_sketch();
    s.include_into_sketch(IncludePick::Origin).unwrap();
    let copy = s.sketch_draft.as_ref().unwrap().copies[0].clone();
    let CopyTarget::Point(o) = copy.target else {
        panic!("Ursprung wird ein Punkt: {copy:?}");
    };
    let (se, sk) = s.session_sketch_mut().unwrap();
    let q = sk.add_point(30.0, 10.0).unwrap();
    let line = sk.add_line(o, q).unwrap();
    sk.add_constraint(K::DistanceX(o, q, 30.0)).unwrap();
    se.last_report = Some(sk.solve());
    s.observe_sketch(false);
    (s, o, line, q)
}

/// Ein Bild der Skizzenansicht mit den Ereignissen `events`.
fn frame(ctx: &egui::Context, s: &mut AppState, time: &mut f64, events: Vec<Event>) {
    *time += 1.0 / 60.0;
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        time: Some(*time),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| {
        sketch_view::show_overlay(ui, SCREEN, s);
    });
    out.drop_without_applying_deltas();
    s.observe_sketch(false);
}

/// Wählt `item` und drückt Entf in der Ansicht.
fn press_delete(s: &mut AppState, item: Item) {
    s.sketch_session.as_mut().unwrap().selection = vec![item];
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    let mut time = 0.0;
    frame(&ctx, s, &mut time, vec![]);
    let key = |pressed| Event::Key {
        key: Key::Delete,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, s, &mut time, vec![key(true)]);
    frame(&ctx, s, &mut time, vec![key(false)]);
}

/// Das X der Liste „Unterbestimmte finden“ an `it`.
fn press_x(s: &mut AppState, it: CheckItem) {
    let (se, sf) = s.session_draft_mut().unwrap();
    se.last_report = Some(sf.sketch.solve());
    se.open_under_list();
    assert!(se.delete_under_item_in(sf, it), "X muss löschen");
    s.observe_sketch(false);
}

/// Prüft den Stand nach dem Löschen der Linie am eingezogenen Ursprung.
fn assert_origin_kept(s: &AppState, o: PointId, line: CurveId, q: PointId, how: &str) {
    let sk = sk(s);
    assert!(sk.curve(line).is_none(), "{how}: Linie fort");
    assert!(sk.point_xy(q).is_err(), "{how}: freies Ende fort");
    assert!(sk.point_xy(o).is_ok(), "{how}: eingezogener Punkt bleibt");
    let kinds: Vec<K> = sk.constraints().map(|(_, k)| k.kind.clone()).collect();
    assert!(
        kinds.iter().any(|k| k.fixed_point() == Some(o)),
        "{how}: Fixierung bleibt: {kinds:?}"
    );
    assert!(
        !kinds.iter().any(|k| matches!(k, K::DistanceX(..))),
        "{how}: Maß am freien Ende geht mit: {kinds:?}"
    );
    let copies = &s.sketch_draft.as_ref().unwrap().copies;
    assert!(
        copies.iter().any(|c| c.target == CopyTarget::Point(o)),
        "{how}: Verknüpfung bleibt: {copies:?}"
    );
}

#[test]
fn delete_key_keeps_included_point_linked() {
    let (mut s, o, line, q) = included_origin_with_line();
    let undo = s.sketch_history().unwrap().undo_len();
    press_delete(&mut s, Item::Curve(line));
    assert_origin_kept(&s, o, line, q, "Entf");
    assert_eq!(s.sketch_history().unwrap().undo_len(), undo + 1);
}

#[test]
fn x_in_list_keeps_included_point_linked() {
    let (mut s, o, line, q) = included_origin_with_line();
    let undo = s.sketch_history().unwrap().undo_len();
    press_x(&mut s, CheckItem::Curve(line));
    assert_origin_kept(&s, o, line, q, "X");
    assert_eq!(s.sketch_history().unwrap().undo_len(), undo + 1);
}

/// Entf und X ergeben dieselbe Skizze und dieselben Kopien.
#[test]
fn delete_key_and_x_give_the_same_result() {
    let (mut a, _, line, _) = included_origin_with_line();
    let (mut b, _, _, _) = included_origin_with_line();
    press_delete(&mut a, Item::Curve(line));
    press_x(&mut b, CheckItem::Curve(line));
    let (da, db) = (
        a.sketch_draft.as_ref().unwrap(),
        b.sketch_draft.as_ref().unwrap(),
    );
    assert_eq!(da.copies, db.copies);
    let pts = |sk: &Sketch| sk.points().map(|p| p.0).collect::<Vec<_>>();
    let cons = |sk: &Sketch| {
        sk.constraints()
            .map(|(_, k)| k.kind.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(pts(&da.sketch), pts(&db.sketch));
    assert_eq!(cons(&da.sketch), cons(&db.sketch));
}

/// Zwei Linien teilen einen Punkt: Löschen der einen lässt ihn samt seinem Maß stehen, das freie
/// Ende der gelöschten Linie geht mit seinem Maß.
#[test]
fn shared_point_stays_with_its_dimension() {
    for how in ["Entf", "X"] {
        let mut s = open_sketch();
        let (gone_line, shared, far, other_end) = {
            let (se, sk) = s.session_sketch_mut().unwrap();
            let a = sk.add_point(0.0, 0.0).unwrap();
            let shared = sk.add_point(10.0, 0.0).unwrap();
            let c = sk.add_point(10.0, 8.0).unwrap();
            let gone_line = sk.add_line(a, shared).unwrap();
            sk.add_line(shared, c).unwrap();
            sk.add_constraint(K::DistanceY(shared, c, 8.0)).unwrap();
            sk.add_constraint(K::DistanceX(a, shared, 10.0)).unwrap();
            se.last_report = Some(sk.solve());
            (gone_line, shared, a, c)
        };
        s.observe_sketch(false);
        match how {
            "Entf" => press_delete(&mut s, Item::Curve(gone_line)),
            _ => press_x(&mut s, CheckItem::Curve(gone_line)),
        }
        let sk = sk(&s);
        assert!(sk.point_xy(shared).is_ok(), "{how}: geteilter Punkt bleibt");
        assert!(sk.point_xy(other_end).is_ok(), "{how}");
        assert!(sk.point_xy(far).is_err(), "{how}: freies Ende fort");
        let kinds: Vec<K> = sk.constraints().map(|(_, k)| k.kind.clone()).collect();
        assert_eq!(kinds, vec![K::DistanceY(shared, other_end, 8.0)], "{how}");
    }
}

/// Die Mitte eines Kreismusters ist zugleich Ende einer Linie: Löschen der Linie lässt die Mitte
/// stehen, das Muster bleibt.
#[test]
fn circular_pattern_center_stays() {
    for how in ["Entf", "X"] {
        let mut s = open_sketch();
        let (line, center) = {
            let (se, sk) = s.session_sketch_mut().unwrap();
            let center = sk.add_point(0.0, 0.0).unwrap();
            let e = sk.add_point(-15.0, 3.0).unwrap();
            let line = sk.add_line(center, e).unwrap();
            let p = sk.add_point(20.0, 0.0).unwrap();
            let r = sk.add_point(25.0, 0.0).unwrap();
            let src = sk.add_line(p, r).unwrap();
            sk.add_pattern(&PatternPlan {
                kind: PatternKind::Circular {
                    center,
                    mode: CircularMode::Full,
                },
                sources: vec![src],
                counts: [4, 1],
                values: [0.0, 0.0],
                suppressed: vec![],
            })
            .unwrap();
            se.last_report = Some(sk.solve());
            (line, center)
        };
        s.observe_sketch(false);
        match how {
            "Entf" => press_delete(&mut s, Item::Curve(line)),
            _ => press_x(&mut s, CheckItem::Curve(line)),
        }
        let sk = sk(&s);
        assert!(sk.curve(line).is_none(), "{how}: Linie fort");
        assert!(sk.point_xy(center).is_ok(), "{how}: Mitte bleibt");
        let centers: Vec<_> = sk
            .patterns()
            .filter_map(|(_, d)| match d.kind {
                planora_sketch::pattern::PatternKind::Circular { center, .. } => Some(center),
                _ => None,
            })
            .collect();
        assert_eq!(centers, vec![center], "{how}: Muster bleibt");
        assert_eq!(
            sk.point_xy(center).unwrap(),
            DVec2::ZERO,
            "{how}: Mitte unverändert"
        );
    }
}
