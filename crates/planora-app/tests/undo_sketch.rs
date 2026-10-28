//! Rückgängig in der offenen Skizze (Aufgabe 7, R10): Solange eine Skizze offen ist, wirken
//! Rückgängig und Wiederholen nur auf ihren eigenen Verlauf; der erste Klick eines Werkzeugs
//! gehört zum Element; Ziehen eines Punktes ist ein Schritt; „Skizze beenden“ ist genau ein
//! Dokumentschritt; der Skizzenverlauf endet mit der Sitzung.
//!
//! Über `Workspace`/`DocTab` (`observe_history` wie am Bildende) und `AppState` direkt.

mod common;

use planora_app::history::MAX_STEPS;
use planora_app::sketch_state::{ConstraintAction, Item, SketchSession, Tool};
use planora_app::state::*;
use planora_app::workspace::*;
use planora_feature::{FeatureId, SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::Sketch;

const R: f64 = 0.5;

// ---- Hilfen ---------------------------------------------------------------------------------

fn ws() -> Workspace {
    let root = common::temp_root("planora-undo-sketch");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn st(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
}

fn tab(w: &mut Workspace) -> &mut DocTab {
    w.active_tab_mut()
}

/// Wie am Ende eines Bildes ohne Geste.
fn observe(w: &mut Workspace) {
    w.observe_history(false);
}

fn doc_undo_len(w: &Workspace) -> usize {
    w.active_tab().history().undo_len()
}

fn sk_undo_len(w: &Workspace) -> usize {
    w.active_tab().state.sketch_history().unwrap().undo_len()
}

fn sk_redo_len(w: &Workspace) -> usize {
    w.active_tab().state.sketch_history().unwrap().redo_len()
}

fn sess(w: &mut Workspace) -> (&mut SketchSession, &mut Sketch) {
    st(w).session_sketch_mut().unwrap()
}

fn draft(w: &Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

fn tool(w: &mut Workspace, t: Tool) {
    let (se, sk) = sess(w);
    se.set_tool(sk, t);
}

fn click(w: &mut Workspace, x: f64, y: f64) {
    let (se, sk) = sess(w);
    se.click(sk, DVec2::new(x, y), R, false).unwrap();
}

fn curves(w: &Workspace) -> usize {
    draft(w).curves().count()
}

/// Neue Skizze auf XY anlegen (ein Dokumentschritt) und öffnen.
fn open_new_sketch(w: &mut Workspace) -> FeatureId {
    let id = st(w)
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    st(w).open_sketch(id).unwrap();
    observe(w);
    id
}

/// Rechteck mit zwei Klicks, danach ein Bild.
fn rectangle(w: &mut Workspace, a: (f64, f64), b: (f64, f64)) {
    tool(w, Tool::Rectangle);
    click(w, a.0, a.1);
    observe(w);
    click(w, b.0, b.1);
    observe(w);
}

// ---- (a) Rückgängig trifft nur die Skizze ---------------------------------------------------

#[test]
fn undo_in_sketch_touches_only_the_sketch() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::splat(5.0)).unwrap();
    observe(&mut w);
    open_new_sketch(&mut w);
    let edits = st(&mut w).document.edit_count();
    let doc_steps = doc_undo_len(&w);
    let doc_before = st(&mut w).snapshot();

    tool(&mut w, Tool::Line);
    click(&mut w, 0.0, 0.0);
    observe(&mut w);
    click(&mut w, 10.0, 4.0);
    observe(&mut w);
    click(&mut w, 3.0, 12.0);
    observe(&mut w);
    assert_eq!(curves(&w), 2);
    assert_eq!(sk_undo_len(&w), 2, "zwei Linien, zwei Schritte");
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Linie zeichnen".to_string())
    );
    assert_eq!(
        w.active_tab().state.sketch_undo_label(),
        Some("Linie zeichnen".to_string())
    );

    tab(&mut w).undo().unwrap();
    assert_eq!(curves(&w), 1, "eine Linie zurückgenommen");
    assert_eq!(draft(&w).points().count(), 2);
    assert_eq!(
        st(&mut w).document.edit_count(),
        edits,
        "Dokument unberührt"
    );
    assert_eq!(doc_undo_len(&w), doc_steps, "Dokumentverlauf ruht");
    assert_eq!(w.active_tab().history().redo_len(), 0);
    assert!(st(&mut w).snapshot().same_content(&doc_before));
    assert!(st(&mut w).sketch_session.is_some(), "Skizze bleibt offen");
    assert_eq!(st(&mut w).bodies.len(), 1, "der Quader bleibt");
    assert_eq!(
        w.active_tab().redo_label(),
        Some("Linie zeichnen".to_string())
    );

    tab(&mut w).redo().unwrap();
    assert_eq!(curves(&w), 2, "wiederholt");
    assert_eq!(sk_undo_len(&w), 2);
    assert_eq!(sk_redo_len(&w), 0);
    assert_eq!(st(&mut w).document.edit_count(), edits);

    // Zweimal zurück, dann eine neue Aktion: die Wiederholen-Liste ist leer.
    tab(&mut w).undo().unwrap();
    tab(&mut w).undo().unwrap();
    assert_eq!(curves(&w), 0);
    assert_eq!(sk_redo_len(&w), 2);
    assert!(tab(&mut w).undo().is_err(), "der Anfang der Skizze");
    assert_eq!(st(&mut w).document.edit_count(), edits);
    tool(&mut w, Tool::Point);
    click(&mut w, 7.0, 7.0);
    observe(&mut w);
    assert_eq!(sk_redo_len(&w), 0);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Punkt setzen".to_string())
    );
    assert_eq!(
        w.active_tab().redo_blocked(),
        Some(nothing_to_redo()),
        "nichts mehr zu wiederholen"
    );
    // Die offene Skizze bleibt „ungespeichert“, auch am Anfang ihres Verlaufs.
    tab(&mut w).undo().unwrap();
    assert!(w.active_tab().is_dirty());
}

/// Eine abgelehnte Änderung (Widerspruch) lässt den Entwurf unverändert: kein Schritt.
#[test]
fn rejected_change_is_no_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Line);
    click(&mut w, 0.0, 0.0);
    click(&mut w, 10.0, 0.1); // fast waagerecht: „Waagerecht“ kommt dazu
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 1);
    tool(&mut w, Tool::Select);
    let line = draft(&w).curves().next().unwrap().0;
    let (se, sk) = sess(&mut w);
    se.selection = vec![Item::Curve(line)];
    assert!(se.apply(sk, ConstraintAction::Vertical, None).is_err());
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 1, "Widerspruch abgelehnt: kein Schritt");
    assert_eq!(
        w.active_tab().state.sketch_undo_label(),
        Some("Linie zeichnen".to_string())
    );
}

// ---- (b) Der erste Klick gehört zum Element --------------------------------------------------

#[test]
fn first_click_belongs_to_the_line() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Line);
    click(&mut w, 0.0, 0.0);
    observe(&mut w);
    assert_eq!(draft(&w).points().count(), 1, "der Punkt steht schon");
    assert_eq!(sk_undo_len(&w), 0, "erster Klick: kein Schritt");
    // Rückgängig nähme nur den offenen Punkt zurück (wie Esc); Wiederholen gibt es nicht.
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Punkt setzen".to_string())
    );
    assert_eq!(w.active_tab().undo_blocked(), None);
    assert_eq!(w.active_tab().redo_label(), None);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 0, "auch nicht im nächsten Bild");

    click(&mut w, 10.0, 5.0);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 1, "zweiter Klick: genau ein Schritt");
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Linie zeichnen".to_string())
    );
    // Linienzug weiter: der dritte Klick ist der zweite Schritt.
    click(&mut w, 2.0, 9.0);
    observe(&mut w);
    assert_eq!(
        sk_undo_len(&w),
        2,
        "Linienzug mit drei Klicks: zwei Schritte"
    );
    // Ein Schritt zurück nimmt die zweite Linie, nicht den offenen Endpunkt allein.
    tab(&mut w).undo().unwrap();
    assert_eq!(curves(&w), 1);

    // Rechteck: ein Schritt.
    let n = sk_undo_len(&w);
    rectangle(&mut w, (20.0, 0.0), (30.0, 6.0));
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Rechteck zeichnen".to_string())
    );

    // Kreis, Mittelpunkt auf einer Linie gefangen (Punkt samt Bindung): ein Schritt.
    tool(&mut w, Tool::Circle);
    let n = sk_undo_len(&w);
    click(&mut w, 25.0, 0.0);
    observe(&mut w);
    assert_eq!(
        sk_undo_len(&w),
        n,
        "Mittelpunkt auf der Kante: noch kein Schritt"
    );
    click(&mut w, 27.0, 0.0);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Kreis zeichnen".to_string())
    );

    // Bogen: drei Klicks, ein Schritt.
    tool(&mut w, Tool::Arc);
    let n = sk_undo_len(&w);
    for (x, y) in [(50.0, 0.0), (55.0, 0.0), (50.0, 5.0)] {
        click(&mut w, x, y);
        observe(&mut w);
    }
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Bogen zeichnen".to_string())
    );

    // Spline: drei Punkte und Abschließen, ein Schritt.
    tool(&mut w, Tool::Spline);
    let n = sk_undo_len(&w);
    for (x, y) in [(60.0, 0.0), (65.0, 4.0), (70.0, 0.0)] {
        click(&mut w, x, y);
        observe(&mut w);
    }
    assert_eq!(sk_undo_len(&w), n, "Splinepunkte allein: kein Schritt");
    let (se, sk) = sess(&mut w);
    se.finish_tool(sk).unwrap();
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Spline zeichnen".to_string())
    );

    // Ein abgebrochener erster Klick hinterlässt einen Punkt: der ist ein eigener Schritt.
    tool(&mut w, Tool::Line);
    let n = sk_undo_len(&w);
    click(&mut w, 80.0, 0.0);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n);
    let (se, sk) = sess(&mut w);
    se.cancel(sk);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Punkt setzen".to_string())
    );
}

/// Offener erster Klick und Rückgängig: Zuerst geht nur der offene Punkt weg (wie Esc), der
/// vorige Schritt bleibt; das nächste Rückgängig nimmt ihn zurück.
#[test]
fn undo_with_open_first_click_cancels_it_first() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, 0.0, 0.0);
    observe(&mut w);
    tool(&mut w, Tool::Line);
    click(&mut w, 10.0, 10.0);
    observe(&mut w);
    assert_eq!(draft(&w).points().count(), 2);
    assert_eq!(sk_undo_len(&w), 1);
    tab(&mut w).undo().unwrap();
    assert_eq!(
        draft(&w).points().count(),
        1,
        "nur der offene Punkt ist weg"
    );
    assert_eq!(sk_undo_len(&w), 1, "der vorige Schritt bleibt");
    assert_eq!(sk_redo_len(&w), 0, "nichts zu wiederholen");
    let se = st(&mut w).sketch_session.as_ref().unwrap();
    assert!(se.pending.is_empty());
    assert_eq!(se.tool, Tool::Line);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 1, "kein neuer Schritt");
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Punkt setzen".to_string())
    );
    tab(&mut w).undo().unwrap();
    assert_eq!(
        draft(&w).points().count(),
        0,
        "zweites Rückgängig: der Schritt"
    );
    assert_eq!(sk_undo_len(&w), 0);
    tab(&mut w).redo().unwrap();
    assert_eq!(draft(&w).points().count(), 1);
    // Ohne vorigen Schritt: der offene Punkt allein.
    tab(&mut w).undo().unwrap();
    click(&mut w, 5.0, 5.0);
    observe(&mut w);
    assert_eq!(tab(&mut w).redo().unwrap_err(), nothing_to_redo());
    tab(&mut w).undo().unwrap();
    assert_eq!(draft(&w).points().count(), 0);
    assert_eq!(sk_redo_len(&w), 1, "die Wiederholen-Liste bleibt");
}

// ---- (c) Ziehen ist ein Schritt ---------------------------------------------------------------

#[test]
fn point_drag_is_one_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Line);
    click(&mut w, 0.0, 0.0);
    click(&mut w, 10.0, 6.0);
    observe(&mut w);
    tool(&mut w, Tool::Select);
    let n = sk_undo_len(&w);
    let p = draft(&w).pick_point(DVec2::new(10.0, 6.0), R).unwrap();
    let start = draft(&w).point_xy(p).unwrap();
    let (se, sk) = sess(&mut w);
    assert!(se.begin_drag(sk, DVec2::new(10.0, 6.0), R));
    for i in 1..=20 {
        let (se, sk) = sess(&mut w);
        se.drag_to(sk, DVec2::new(10.0 + i as f64 * 0.5, 6.0 - i as f64 * 0.2));
        w.observe_history(true);
        assert_eq!(sk_undo_len(&w), n, "während des Ziehens kein Schritt");
    }
    let (se, sk) = sess(&mut w);
    se.end_drag(sk);
    let end = draft(&w).point_xy(p).unwrap();
    assert!(end.distance(start) > 5.0, "der Punkt ist gewandert");
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1, "Ziehen: genau ein Schritt");
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Skizze ändern".to_string())
    );

    tab(&mut w).undo().unwrap();
    let back = draft(&w).point_xy(p).unwrap();
    assert_eq!(back.x.to_bits(), start.x.to_bits(), "exakt die alte Stelle");
    assert_eq!(back.y.to_bits(), start.y.to_bits());
    assert_eq!(st(&mut w).sketch_session.as_ref().unwrap().dragging, None);
    tab(&mut w).redo().unwrap();
    let again = draft(&w).point_xy(p).unwrap();
    assert_eq!(
        (again.x.to_bits(), again.y.to_bits()),
        (end.x.to_bits(), end.y.to_bits())
    );
}

// ---- (d) Zurückladen leert die Sitzung --------------------------------------------------------

#[test]
fn jump_clears_session_bits() {
    let mut w = ws();
    open_new_sketch(&mut w);
    rectangle(&mut w, (0.0, 0.0), (10.0, 5.0));
    rectangle(&mut w, (20.0, 0.0), (30.0, 5.0));
    let s = &w.active_tab().state;
    assert_eq!(
        s.profile_cache
            .get(s.session_sketch().unwrap())
            .profiles
            .len(),
        2
    );
    let computed = s.profile_cache.computations();

    // Laufendes Werkzeug, Auswahl, Meldung, Ziehen, kein Bericht.
    tool(&mut w, Tool::Ellipse);
    click(&mut w, 50.0, 50.0);
    click(&mut w, 55.0, 50.0);
    let c = draft(&w).curves().next().unwrap().0;
    let p = draft(&w).points().next().unwrap().0;
    {
        let (se, _) = sess(&mut w);
        assert!(!se.pending.is_empty() && !se.pending_pos.is_empty());
        se.selection = vec![Item::Curve(c)];
        se.message = Some("alt".into());
        se.dragging = Some(p);
        se.last_report = None;
    }
    let cleared = |w: &Workspace| {
        let s = &w.active_tab().state;
        let se = s.sketch_session.as_ref().unwrap();
        assert!(se.pending.is_empty());
        assert!(se.pending_pos.is_empty());
        assert!(se.selection.is_empty());
        assert_eq!(se.message, None);
        assert_eq!(se.dragging, None);
        assert_eq!(se.tool, Tool::Ellipse, "das Werkzeug bleibt");
        let report = se.last_report.as_ref().expect("Bericht neu bestimmt");
        let fresh = s.session_sketch().unwrap().clone().solve();
        assert_eq!(report.status, fresh.status);
        assert_eq!(report.dof, fresh.dof);
        assert_eq!(report.free_points, fresh.free_points);
    };
    // Erst nur der offene Klick der Ellipse (wie Esc).
    assert!(st(&mut w).undo_sketch());
    cleared(&w);
    assert_eq!(curves(&w), 8);
    assert_eq!(sk_undo_len(&w), 2);
    // Dann der letzte Schritt.
    {
        let (se, _) = sess(&mut w);
        se.selection = vec![Item::Curve(c)];
        se.message = Some("alt".into());
        se.dragging = Some(p);
        se.last_report = None;
    }
    assert!(st(&mut w).undo_sketch());
    cleared(&w);
    let s = &w.active_tab().state;
    // Ein Rechteck weniger.
    assert_eq!(s.session_sketch().unwrap().curves().count(), 4);
    // Der Entwurf hat genau den Stand des Bildes; die Profile sind die des zurückgeholten Stands.
    let h = s.sketch_history().unwrap();
    assert_eq!(
        s.session_sketch().unwrap().revision(),
        h.current().sketch.revision()
    );
    assert_eq!(
        s.profile_cache
            .get(s.session_sketch().unwrap())
            .profiles
            .len(),
        1
    );
    assert_eq!(s.profile_cache.computations(), computed + 1);
    // Nach dem Sprung ist nichts offen: kein Schritt beim nächsten Bild.
    let n = sk_undo_len(&w);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n);

    // Wiederholen ebenso.
    {
        let (se, _) = sess(&mut w);
        se.selection = vec![Item::Point(p)];
        se.message = Some("alt".into());
        se.last_report = None;
    }
    assert!(st(&mut w).redo_sketch());
    let se = w.active_tab().state.sketch_session.as_ref().unwrap();
    assert!(se.selection.is_empty());
    assert_eq!(se.message, None);
    assert!(se.last_report.is_some());
    assert_eq!(
        w.active_tab()
            .state
            .profile_cache
            .get(draft(&w))
            .profiles
            .len(),
        2
    );
}

/// Ein Widerspruch im zurückgeholten Stand wird im Bericht gezeigt (Färbung, Freiheitsgrade).
#[test]
fn jump_reports_the_restored_solver_state() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Line);
    click(&mut w, 0.0, 0.0);
    click(&mut w, 10.0, 4.0);
    observe(&mut w);
    let free = w
        .active_tab()
        .state
        .sketch_session
        .as_ref()
        .unwrap()
        .last_report
        .as_ref()
        .unwrap()
        .dof;
    assert!(free > 0);
    tool(&mut w, Tool::Select);
    let line = draft(&w).curves().next().unwrap().0;
    let (se, sk) = sess(&mut w);
    se.selection = vec![Item::Curve(line)];
    se.apply(sk, ConstraintAction::Horizontal, None).unwrap();
    observe(&mut w);
    let dof_after = |w: &Workspace| {
        w.active_tab()
            .state
            .sketch_session
            .as_ref()
            .unwrap()
            .last_report
            .as_ref()
            .unwrap()
            .dof
    };
    assert_eq!(dof_after(&w), free - 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Abhängigkeit setzen".to_string())
    );
    tab(&mut w).undo().unwrap();
    assert_eq!(
        dof_after(&w),
        free,
        "Freiheitsgrade des zurückgeholten Stands"
    );
    tab(&mut w).redo().unwrap();
    assert_eq!(dof_after(&w), free - 1);
}

// ---- (e) „Skizze beenden“ ist ein Dokumentschritt --------------------------------------------

#[test]
fn end_sketch_is_one_document_step() {
    let mut w = ws();
    let id = open_new_sketch(&mut w);
    assert_eq!(doc_undo_len(&w), 1);
    assert_eq!(
        w.active_tab().history().undo_label(),
        Some("Skizze anlegen")
    );
    let created = st(&mut w).snapshot();

    rectangle(&mut w, (0.0, 0.0), (10.0, 5.0));
    tool(&mut w, Tool::Line);
    click(&mut w, 20.0, 0.0);
    click(&mut w, 25.0, 9.0);
    observe(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, 40.0, 40.0);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 3, "drei Elemente");
    assert_eq!(doc_undo_len(&w), 1, "der Dokumentverlauf ruht");

    st(&mut w).close_sketch();
    observe(&mut w);
    assert_eq!(doc_undo_len(&w), 2, "genau ein Dokumentschritt");
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Skizze bearbeiten".to_string())
    );
    assert_eq!(
        st(&mut w).document.sketch_of(id).unwrap().curves().count(),
        5
    );

    tab(&mut w).undo().unwrap();
    let sk = st(&mut w).document.sketch_of(id).unwrap();
    assert_eq!(sk.curves().count(), 0, "die ganze Skizze zurück");
    assert_eq!(sk.points().count(), 0);
    assert!(
        st(&mut w).snapshot().same_content(&created),
        "wie nach dem Anlegen"
    );
    tab(&mut w).redo().unwrap();
    assert_eq!(
        st(&mut w).document.sketch_of(id).unwrap().curves().count(),
        5
    );

    // Unverändert öffnen und schließen: kein Schritt.
    let n = doc_undo_len(&w);
    st(&mut w).open_sketch(id).unwrap();
    observe(&mut w);
    st(&mut w).close_sketch();
    observe(&mut w);
    assert_eq!(doc_undo_len(&w), n, "unverändert: kein Schritt");

    // Alle Skizzenschritte zurückgenommen und geschlossen: kein Dokumentschritt.
    st(&mut w).open_sketch(id).unwrap();
    observe(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, -5.0, -5.0);
    observe(&mut w);
    click(&mut w, -8.0, -5.0);
    observe(&mut w);
    while tab(&mut w).undo().is_ok() {}
    let edits = st(&mut w).document.edit_count();
    st(&mut w).close_sketch();
    observe(&mut w);
    assert_eq!(doc_undo_len(&w), n, "alles zurückgenommen: kein Schritt");
    assert_eq!(
        st(&mut w).document.edit_count(),
        edits,
        "nicht einmal neu berechnet"
    );
}

// ---- (f) Der Skizzenverlauf endet mit der Sitzung --------------------------------------------

#[test]
fn sketch_history_ends_with_session() {
    let mut w = ws();
    let id = open_new_sketch(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, 1.0, 1.0);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 1);
    st(&mut w).close_sketch();
    observe(&mut w);
    assert!(st(&mut w).sketch_history().is_none());
    st(&mut w).open_sketch(id).unwrap();
    observe(&mut w);
    assert_eq!(
        w.active_tab().state.sketch_undo_label(),
        None,
        "neu geöffnet: leer"
    );
    assert_eq!(sk_undo_len(&w), 0);
    assert_eq!(w.active_tab().undo_label(), None);

    // Wechsel zu einer anderen Skizze: deren eigener, leerer Verlauf.
    tool(&mut w, Tool::Point);
    click(&mut w, 2.0, 2.0);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 1);
    let other = st(&mut w)
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    st(&mut w).open_sketch(other).unwrap();
    observe(&mut w);
    assert_eq!(st(&mut w).open_sketch_id(), Some(other));
    assert_eq!(sk_undo_len(&w), 0, "Wechsel: neuer Verlauf");
    assert_eq!(w.active_tab().state.sketch_undo_label(), None);
    assert_eq!(
        st(&mut w).document.sketch_of(id).unwrap().points().count(),
        2,
        "die alte Skizze ist übernommen"
    );

    // Die offene Skizze im Baum löschen: Verlauf weg, Dokumentschritt „Feature löschen“.
    tool(&mut w, Tool::Point);
    click(&mut w, 3.0, 3.0);
    observe(&mut w);
    st(&mut w).delete_feature(other).unwrap();
    assert!(st(&mut w).sketch_session.is_none());
    assert!(st(&mut w).sketch_history().is_none());
    observe(&mut w);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Feature löschen".to_string())
    );

    // Zurückladen des Dokuments verwirft eine offene Skizze samt Verlauf.
    st(&mut w).open_sketch(id).unwrap();
    click(&mut w, 4.0, 4.0);
    observe(&mut w);
    let snap = st(&mut w).snapshot();
    st(&mut w).restore_document(snap).unwrap();
    assert!(st(&mut w).sketch_history().is_none());
    st(&mut w).open_sketch(id).unwrap();
    assert_eq!(sk_undo_len(&w), 0);
}

/// Grenze: höchstens 100 Schritte, die ältesten fallen heraus.
#[test]
fn sketch_history_keeps_at_most_max_steps() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Point);
    for i in 0..MAX_STEPS + 3 {
        click(&mut w, i as f64 * 2.0, 0.0);
        observe(&mut w);
    }
    assert_eq!(sk_undo_len(&w), MAX_STEPS);
    while tab(&mut w).undo().is_ok() {}
    assert_eq!(
        draft(&w).points().count(),
        3,
        "die ersten drei sind verdrängt"
    );
}

// ---- (g) Namen und Wege je nach Modus --------------------------------------------------------

#[test]
fn labels_route_by_mode() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::splat(2.0)).unwrap();
    observe(&mut w);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Quader anlegen".to_string())
    );
    let id = open_new_sketch(&mut w);
    // Offen, ohne Skizzenschritt: kein Name, Rückgängig ändert das Dokument nicht.
    assert_eq!(w.active_tab().undo_label(), None);
    assert_eq!(w.active_tab().redo_label(), None);
    assert_eq!(w.active_tab().undo_blocked(), Some(nothing_to_undo()));
    assert_eq!(w.active_tab().redo_blocked(), Some(nothing_to_redo()));
    let edits = st(&mut w).document.edit_count();
    let features = st(&mut w).document.features().len();
    assert_eq!(tab(&mut w).undo().unwrap_err(), nothing_to_undo());
    assert_eq!(tab(&mut w).redo().unwrap_err(), nothing_to_redo());
    assert_eq!(st(&mut w).document.edit_count(), edits);
    assert_eq!(st(&mut w).document.features().len(), features);
    assert_eq!(
        st(&mut w).open_sketch_id(),
        Some(id),
        "die Skizze bleibt offen"
    );
    assert_eq!(doc_undo_len(&w), 2);
    assert_eq!(w.active_tab().history().redo_len(), 0);

    // Mit Skizzenschritt: dessen Name, auch noch vor dem Bildende.
    tool(&mut w, Tool::Line);
    click(&mut w, 0.0, 0.0);
    click(&mut w, 4.0, 3.0);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Linie zeichnen".to_string()),
        "offen"
    );
    assert_eq!(w.active_tab().undo_blocked(), None);
    observe(&mut w);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Linie zeichnen".to_string())
    );
    tab(&mut w).undo().unwrap();
    assert_eq!(
        w.active_tab().redo_label(),
        Some("Linie zeichnen".to_string())
    );
    assert_eq!(w.active_tab().redo_blocked(), None);
    assert_eq!(w.active_tab().undo_blocked(), Some(nothing_to_undo()));
    tab(&mut w).redo().unwrap();

    // Geschlossen: wieder der Dokumentverlauf.
    st(&mut w).close_sketch();
    observe(&mut w);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Skizze bearbeiten".to_string())
    );
    tab(&mut w).undo().unwrap();
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Skizze anlegen".to_string())
    );
    assert!(matches!(
        st(&mut w)
            .document
            .sketch_of(id)
            .map(|s| s.curves().count()),
        Some(0)
    ));
}

/// „Kante übernehmen“: Kurve und Verweis (Kopie) sind zusammen ein Schritt; Rückgängig nimmt
/// beide zurück, „Skizze beenden“ schreibt den zurückgeholten Stand.
#[test]
fn projected_edge_is_a_step_with_its_copy() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    observe(&mut w);
    let id = open_new_sketch(&mut w);
    let solid = st(&mut w).bodies[0].solid.clone();
    let mut done = 0;
    for (e, _) in solid.edges.iter() {
        if done < 2 && st(&mut w).project_edge_into_sketch(0, e).is_ok() {
            done += 1;
            observe(&mut w);
        }
    }
    assert_eq!(done, 2);
    assert_eq!(sk_undo_len(&w), 2);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Kante übernehmen".to_string())
    );
    let copies = |w: &Workspace| {
        w.active_tab()
            .state
            .sketch_draft
            .as_ref()
            .unwrap()
            .copies
            .len()
    };
    assert_eq!(copies(&w), 2);
    tab(&mut w).undo().unwrap();
    assert_eq!(copies(&w), 1, "der Verweis geht mit zurück");
    assert_eq!(curves(&w), 1);
    tab(&mut w).redo().unwrap();
    assert_eq!(copies(&w), 2);
    tab(&mut w).undo().unwrap();
    st(&mut w).close_sketch();
    observe(&mut w);
    let planora_feature::FeatureKind::Sketch(def) = &st(&mut w).document.feature(id).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(def.copies.len(), 1);
}

/// Eine Abhängigkeit, während ein Werkzeug offene Punkte hat, ist ein eigener Schritt (nur
/// neue Punkte samt Fangbindung werden aufgeschoben).
#[test]
fn constraint_while_tool_open_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, 5.0, 5.0);
    observe(&mut w);
    let p = draft(&w).points().next().unwrap().0;
    tool(&mut w, Tool::Line);
    click(&mut w, 0.0, 0.0);
    observe(&mut w);
    let n = sk_undo_len(&w);
    let (se, sk) = sess(&mut w);
    assert!(!se.pending.is_empty());
    se.selection = vec![Item::Point(p)];
    se.apply(sk, ConstraintAction::Fixed, None).unwrap();
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
}

// ---- Nachbesserung: Aufschub nur für reine Punkte, Lücken ------------------------------------

/// Linie (0,0)–(10,4) mit Längenmaß 10 (ein Schritt je Aktion); danach Linienwerkzeug mit
/// offenem erstem Klick. Liefert Linie und Maß.
fn line_with_dimension_and_open_click(
    w: &mut Workspace,
) -> (
    planora_sketch::model::CurveId,
    planora_sketch::constraint::ConstraintId,
) {
    tool(w, Tool::Line);
    click(w, 0.0, 0.0);
    click(w, 10.0, 4.0);
    observe(w);
    tool(w, Tool::Select);
    let line = draft(w).curves().next().unwrap().0;
    let (se, sk) = sess(w);
    se.selection = vec![Item::Curve(line)];
    let dim = se
        .apply(sk, ConstraintAction::Distance, Some(10.0))
        .unwrap();
    observe(w);
    assert_eq!(w.active_tab().undo_label(), Some("Maß setzen".to_string()));
    tool(w, Tool::Line);
    click(w, 30.0, 30.0);
    observe(w);
    assert!(!sess(w).0.pending.is_empty());
    (line, dim)
}

/// I1: Maßwert ändern bei offenem erstem Klick ist ein eigener Schritt; die Linie danach auch.
#[test]
fn dimension_value_with_open_click_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    let (_, dim) = line_with_dimension_and_open_click(&mut w);
    let n = sk_undo_len(&w);
    let (se, sk) = sess(&mut w);
    se.set_dimension_value(sk, dim, 20.0).unwrap();
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1, "Maß 10 → 20: eigener Schritt");
    // Der offene Punkt benennt den Schritt nicht.
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Skizze ändern".to_string())
    );
    click(&mut w, 40.0, 33.0);
    observe(&mut w);
    assert_eq!(
        sk_undo_len(&w),
        n + 2,
        "die Linie bleibt ein eigener Schritt"
    );
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Linie zeichnen".to_string())
    );
    tab(&mut w).undo().unwrap();
    tab(&mut w).undo().unwrap();
    let v = |w: &Workspace| match draft(w).constraint(dim).unwrap().kind {
        planora_sketch::constraint::ConstraintKind::Distance(_, _, v) => v,
        _ => unreachable!(),
    };
    assert_eq!(v(&w), 10.0, "zwei Schritte zurück: Maß wieder 10");
}

/// I1: eine neue Abhängigkeit bei offenem erstem Klick ist ein eigener Schritt mit ihrem Namen.
#[test]
fn new_constraint_with_open_click_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    let (line, _) = line_with_dimension_and_open_click(&mut w);
    let n = sk_undo_len(&w);
    let (se, sk) = sess(&mut w);
    se.selection = vec![Item::Curve(line)];
    se.apply(sk, ConstraintAction::Horizontal, None).unwrap();
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Abhängigkeit setzen".to_string())
    );
}

/// I1: „anzeigend“ umschalten bei offenem erstem Klick ist ein eigener Schritt.
#[test]
fn driven_toggle_with_open_click_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    let (_, dim) = line_with_dimension_and_open_click(&mut w);
    let n = sk_undo_len(&w);
    let (se, sk) = sess(&mut w);
    se.set_driven(sk, dim, true).unwrap();
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    tab(&mut w).undo().unwrap();
    assert!(!draft(&w).constraint(dim).unwrap().driven);
}

/// I1: Hilfslinie umschalten bei offenem erstem Klick ist ein eigener Schritt.
#[test]
fn construction_toggle_with_open_click_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    let (line, _) = line_with_dimension_and_open_click(&mut w);
    let n = sk_undo_len(&w);
    let (se, sk) = sess(&mut w);
    se.selection = vec![Item::Curve(line)];
    se.toggle_construction(sk);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Hilfslinie umschalten".to_string())
    );
}

/// Abhängigkeit löschen bei offenem erstem Klick: eigener Schritt.
#[test]
fn deleting_a_constraint_with_open_click_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    let (_, dim) = line_with_dimension_and_open_click(&mut w);
    let n = sk_undo_len(&w);
    let (se, sk) = sess(&mut w);
    se.selection = vec![Item::Constraint(dim)];
    se.delete_selection(sk);
    assert!(!se.pending.is_empty(), "der offene Punkt bleibt");
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(w.active_tab().undo_label(), Some("Löschen".to_string()));
}

/// Einen freien Punkt löschen bei offenem erstem Klick: eigener Schritt.
#[test]
fn deleting_a_point_with_open_click_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, -20.0, -20.0);
    observe(&mut w);
    let lone = draft(&w).points().next().unwrap().0;
    tool(&mut w, Tool::Line);
    click(&mut w, 30.0, 30.0);
    observe(&mut w);
    let n = sk_undo_len(&w);
    let (se, sk) = sess(&mut w);
    se.selection = vec![Item::Point(lone)];
    se.delete_selection(sk);
    assert!(!se.pending.is_empty());
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(w.active_tab().undo_label(), Some("Löschen".to_string()));
}

/// Rückgängig ohne vorheriges Bildende: die noch nicht erfasste Änderung wird ein Schritt und
/// genau dieser geht zurück (über `AppState` und über den Tab, wie die Tastenkürzel).
#[test]
fn undo_takes_an_unobserved_change_as_a_step() {
    for via_tab in [false, true] {
        let mut w = ws();
        open_new_sketch(&mut w);
        tool(&mut w, Tool::Point);
        click(&mut w, 1.0, 1.0);
        observe(&mut w);
        click(&mut w, 2.0, 2.0); // ohne Bildende
        if via_tab {
            tab(&mut w).undo().unwrap();
        } else {
            assert!(st(&mut w).undo_sketch());
        }
        assert_eq!(draft(&w).points().count(), 1, "nur der neue Punkt geht");
        assert_eq!(sk_redo_len(&w), 1);
        if via_tab {
            tab(&mut w).redo().unwrap();
        } else {
            assert!(st(&mut w).redo_sketch());
        }
        assert_eq!(draft(&w).points().count(), 2, "und kommt wieder");
    }
}

/// Lehnt das Dokument den Entwurf beim Beenden ab, bleibt die Skizze offen samt Verlauf.
#[test]
fn rejected_commit_keeps_the_sketch_history() {
    let mut w = ws();
    let id = st(&mut w)
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    st(&mut w).create_box(DVec3::splat(4.0)).unwrap(); // entsteht nach der Skizze
    observe(&mut w);
    st(&mut w).open_sketch(id).unwrap();
    tool(&mut w, Tool::Point);
    click(&mut w, 1.0, 1.0);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), 1);
    let solid = st(&mut w).bodies[0].solid.clone();
    let tol = st(&mut w).kernel.tolerance;
    let edge = solid
        .edges
        .iter()
        .find_map(|(e, _)| planora_feature::EdgeRef::from_edge(&solid, e, &tol).ok())
        .unwrap();
    let curve = {
        let sk = sess(&mut w).1;
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(1.0, 0.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    st(&mut w)
        .sketch_draft
        .as_mut()
        .unwrap()
        .copies
        .push(planora_feature::EdgeCopy { curve, edge });
    assert!(st(&mut w).try_close_sketch().is_err());
    assert!(st(&mut w).sketch_session.is_some());
    assert_eq!(sk_undo_len(&w), 1, "der Verlauf bleibt");
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Kante übernehmen".to_string())
    );
    tab(&mut w).undo().unwrap();
    tab(&mut w).undo().unwrap();
    assert_eq!(draft(&w).points().count(), 0);
}

/// Wiederholen-Name bei offener Änderung: keiner (auch beim offenen ersten Klick).
#[test]
fn redo_label_is_none_with_an_open_change() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, 1.0, 1.0);
    observe(&mut w);
    tab(&mut w).undo().unwrap();
    assert_eq!(
        w.active_tab().state.sketch_redo_label(),
        Some("Punkt setzen".to_string())
    );
    click(&mut w, 2.0, 2.0); // noch kein Schritt
    assert_eq!(w.active_tab().state.sketch_redo_label(), None);
    assert_eq!(w.active_tab().redo_blocked(), Some(nothing_to_redo()));
    tab(&mut w).undo().unwrap();
    assert_eq!(
        w.active_tab().state.sketch_redo_label(),
        Some("Punkt setzen".to_string())
    );
    tool(&mut w, Tool::Line);
    click(&mut w, 3.0, 3.0); // offener erster Klick
    observe(&mut w);
    assert_eq!(w.active_tab().state.sketch_redo_label(), None);
}

/// M3: Ein offenes Maßfeld schließt nach Rückgängig, auch wenn das Maß noch da ist und die
/// Auswahl wieder dieselbe ist.
#[test]
fn dimension_field_closes_after_a_jump() {
    let c = egui::Context::default();
    c.set_fonts(planora_app::fonts::definitions());
    planora_app::theme::apply(&c, true);
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Line);
    click(&mut w, 0.0, 0.0);
    click(&mut w, 10.0, 4.0);
    observe(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, 30.0, 30.0);
    observe(&mut w);
    tool(&mut w, Tool::Select);
    let line = draft(&w).curves().next().unwrap().0;
    let frame = |c: &egui::Context, w: &mut Workspace| {
        let out = c.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| planora_app::sketch_panel::show(ui.ctx(), st(w)),
        );
        out.drop_without_applying_deltas();
    };
    sess(&mut w).0.selection = vec![Item::Curve(line)];
    planora_app::sketch_panel::open_dimension_editor(&c, st(&mut w), ConstraintAction::Distance);
    frame(&c, &mut w);
    assert!(planora_app::sketch_panel::dimension_editor_open(&c));
    tab(&mut w).undo().unwrap(); // nimmt den Punkt, die Linie bleibt
    assert!(draft(&w).curve(line).is_some());
    sess(&mut w).0.selection = vec![Item::Curve(line)];
    frame(&c, &mut w);
    assert!(
        !planora_app::sketch_panel::dimension_editor_open(&c),
        "das Feld gehört zum alten Stand"
    );
}

/// I1: Verschiebt sich ein vorhandener Punkt bei offenem erstem Klick (etwa durch den Solver),
/// ist das ein eigener Schritt.
#[test]
fn moved_point_with_open_click_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Point);
    click(&mut w, -20.0, -20.0);
    observe(&mut w);
    let lone = draft(&w).points().next().unwrap().0;
    tool(&mut w, Tool::Line);
    click(&mut w, 30.0, 30.0);
    observe(&mut w);
    let n = sk_undo_len(&w);
    sess(&mut w).1.set_point_xy(lone, -21.0, -20.0).unwrap();
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Skizze ändern".to_string())
    );
}

/// I1: Häkchen „anzeigend“ direkt an der Skizze (Wert unverändert) bei offenem erstem Klick:
/// eigener Schritt.
#[test]
fn flag_change_with_open_click_is_its_own_step() {
    let mut w = ws();
    open_new_sketch(&mut w);
    let (_, dim) = line_with_dimension_and_open_click(&mut w);
    let n = sk_undo_len(&w);
    sess(&mut w).1.set_driven(dim, true).unwrap();
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
}

/// Erster Klick auf den Rand eines Kreises und eines Bogens (Punkt samt Bindung, der Solver
/// prüft die Gruppe): bleibt aufgeschoben, das Element ist ein Schritt.
#[test]
fn first_click_snapped_on_a_circle_is_deferred() {
    let mut w = ws();
    open_new_sketch(&mut w);
    tool(&mut w, Tool::Circle);
    click(&mut w, 0.0, 0.0);
    click(&mut w, 7.3, 0.0);
    observe(&mut w);
    let n = sk_undo_len(&w);
    tool(&mut w, Tool::Line);
    let at = 7.3 / 2f64.sqrt();
    click(&mut w, at + 0.1, at - 0.05);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n, "Fang auf dem Kreis: noch kein Schritt");
    click(&mut w, 20.0, 15.0);
    observe(&mut w);
    assert_eq!(sk_undo_len(&w), n + 1);
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Linie zeichnen".to_string())
    );
}
