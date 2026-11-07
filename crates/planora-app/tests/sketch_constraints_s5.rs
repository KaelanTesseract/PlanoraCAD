//! Skizzen 5, Task 7: Konzentrisch, Kollinear und Glatt im Skizzenfeld. Die App wird ohne
//! Fenster bei 1920×1080 gezeichnet, die Knöpfe werden über ihre Beschriftung geklickt.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::keymap::Action;
use planora_app::sketch_state::{ConstraintAction, Item};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_view, theme};
use planora_feature::SketchPlane;
use planora_i18n::{english, german, tr, with_lang};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

const SIZE: Vec2 = vec2(1920.0, 1080.0);

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(events: Vec<Event>) -> egui::RawInput {
    let time = TIME.with(|t| {
        t.set(t.get() + 1.0 / 60.0);
        t.get()
    });
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
        time: Some(time),
        events,
        ..Default::default()
    }
}

fn state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

fn sk(app: &mut PlanoraApp) -> &mut Sketch {
    state(app).session_sketch_mut().unwrap().1
}

/// App mit offener, leerer Skizze auf XY.
fn sketching() -> PlanoraApp {
    let root = common::temp_root("planora-sketch-constraints-s5");
    let ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let mut app = PlanoraApp::for_tests(ws);
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    app
}

struct Shot {
    texts: Vec<(String, Rect)>,
}

fn walk(shape: &Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
        Shape::Text(t) => out.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        _ => {}
    }
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let mut texts = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut texts);
        }
        Shot { texts }
    }

    fn count(&self, exact: &str) -> usize {
        self.texts.iter().filter(|(t, _)| t == exact).count()
    }

    /// Erste Stelle des Texts (der Knopf steht über der Liste).
    fn at(&self, exact: &str) -> Rect {
        self.texts
            .iter()
            .filter(|(t, _)| t == exact)
            .min_by(|a, b| a.1.top().total_cmp(&b.1.top()))
            .unwrap_or_else(|| {
                panic!(
                    "„{exact}“ fehlt in {:?}",
                    self.texts.iter().map(|t| &t.0).collect::<Vec<_>>()
                )
            })
            .1
    }
}

fn frame(c: &egui::Context, app: &mut PlanoraApp, events: Vec<Event>) -> Shot {
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(events), |ui| app.ui(ui, &mut f));
    let shot = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    shot
}

fn settle(c: &egui::Context, app: &mut PlanoraApp) -> Shot {
    frame(c, app, vec![]);
    frame(c, app, vec![]);
    frame(c, app, vec![])
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn click(c: &egui::Context, app: &mut PlanoraApp, p: Pos2) -> Shot {
    frame(c, app, vec![Event::PointerMoved(p)]);
    frame(c, app, vec![Event::PointerMoved(p), button(p, true)]);
    frame(c, app, vec![button(p, false)]);
    settle(c, app)
}

/// Klickt den Knopf mit der Beschriftung `label` im Skizzenfeld.
fn press(c: &egui::Context, app: &mut PlanoraApp, label: &str) -> Shot {
    let shot = settle(c, app);
    let at = shot.at(label).center();
    click(c, app, at)
}

fn select(app: &mut PlanoraApp, items: Vec<Item>) {
    state(app).sketch_session.as_mut().unwrap().selection = items;
}

fn available(app: &mut PlanoraApp) -> Vec<ConstraintAction> {
    let (se, sk) = state(app).session_sketch_mut().unwrap();
    se.available_actions(sk)
}

fn message(app: &mut PlanoraApp) -> Option<String> {
    state(app).sketch_session.as_ref().unwrap().message_text()
}

fn live(app: &mut PlanoraApp) -> Vec<K> {
    sk(app).constraints().map(|(_, c)| c.kind.clone()).collect()
}

fn dof(app: &mut PlanoraApp) -> usize {
    sk(app).clone().solve().dof
}

fn pt(app: &mut PlanoraApp, x: f64, y: f64) -> PointId {
    sk(app).add_point(x, y).unwrap()
}

fn circle(app: &mut PlanoraApp, x: f64, y: f64, r: f64) -> (CurveId, PointId) {
    let m = pt(app, x, y);
    (sk(app).add_circle(m, r).unwrap(), m)
}

fn line(app: &mut PlanoraApp, a: (f64, f64), b: (f64, f64)) -> (CurveId, PointId, PointId) {
    let (pa, pb) = (pt(app, a.0, a.1), pt(app, b.0, b.1));
    (sk(app).add_line(pa, pb).unwrap(), pa, pb)
}

fn undo(app: &mut PlanoraApp) {
    assert!(state(app).undo_sketch());
}

#[test]
fn concentric_ties_two_circles_and_is_one_undo_step() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (c1, m1) = circle(&mut app, 0.0, 0.0, 5.0);
        let (c2, m2) = circle(&mut app, 30.0, 10.0, 8.0);
        settle(&c, &mut app);
        let before = live(&mut app).len();
        let dof_before = dof(&mut app);
        select(&mut app, vec![Item::Curve(c1), Item::Curve(c2)]);
        assert!(available(&mut app).contains(&ConstraintAction::Concentric));
        let shot = press(&c, &mut app, "Konzentrisch");
        let now = live(&mut app);
        assert_eq!(now.len(), before + 1, "{:?}", message(&mut app));
        let kind = now.last().unwrap().clone();
        assert!(
            matches!(kind, K::Coincident(a, b) if [a, b] == [m1, m2] || [a, b] == [m2, m1]),
            "{kind:?}"
        );
        let s = sk(&mut app);
        assert!(s.point_xy(m1).unwrap().distance(s.point_xy(m2).unwrap()) < 1e-9);
        assert!(sketch_view::is_concentric(s, &kind));
        assert_eq!(s.clone().solve().status, SolveStatus::Solved);
        assert_eq!(dof(&mut app), dof_before - 2);
        // Die Liste nennt es „Konzentrisch“, die Ansicht zeichnet zwei Kreise (kein Schriftzeichen,
        // Korrektur 1: kein Ersatz „(o)“ mehr).
        let s = sk(&mut app).clone();
        assert_eq!(
            sketch_view::constraint_name(&s, &kind, false),
            "Konzentrisch"
        );
        assert!(shot.count("Konzentrisch") >= 2, "Knopf und Listeneintrag");
        let centers = sketch_view::round_centers(&s);
        assert_eq!(
            sketch_view::overlay_mark(&centers, &kind),
            Some(sketch_view::PaintedMark::Concentric)
        );
        assert_eq!(sketch_view::overlay_text(&c, &s, &kind, false), "");
        assert_eq!(shot.count("(o)") + shot.count("◎"), 0);
        // Ein gewöhnliches Deckungsgleich bleibt es.
        let p = pt(&mut app, 1.0, 1.0);
        let q = pt(&mut app, 2.0, 2.0);
        let s = sk(&mut app).clone();
        assert!(!sketch_view::is_concentric(&s, &K::Coincident(p, q)));
        assert!(!sketch_view::is_concentric(&s, &K::Coincident(m1, p)));
        settle(&c, &mut app);
        // Das erste Rückgängig nimmt die zwei Punkte eben, das zweite Konzentrisch.
        undo(&mut app);
        undo(&mut app);
        assert_eq!(live(&mut app).len(), before);
        let s = sk(&mut app);
        assert!(s.point_xy(m1).unwrap().distance(s.point_xy(m2).unwrap()) > 1.0);
    });
}

#[test]
fn concentric_needs_round_curves_and_says_when_the_centre_is_shared() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (c1, m) = circle(&mut app, 0.0, 0.0, 5.0);
        let (l, ..) = line(&mut app, (20.0, 0.0), (40.0, 5.0));
        settle(&c, &mut app);
        // Kreis und Linie: Der Klick legt nichts an; die Meldung sagt, warum, und der Befehl
        // startet ohne diese Auswahl (Handtest Korrektur 4: erst Befehl, dann Auswahl).
        select(&mut app, vec![Item::Curve(c1), Item::Curve(l)]);
        assert!(!available(&mut app).contains(&ConstraintAction::Concentric));
        let rev = sk(&mut app).revision();
        press(&c, &mut app, "Konzentrisch");
        assert_eq!(sk(&mut app).revision(), rev);
        assert_eq!(message(&mut app), Some(tr!("sketch-concentric-kinds")));
        {
            let se = state(&mut app).sketch_session.as_ref().unwrap();
            assert_eq!(
                se.tool,
                planora_app::sketch_state::Tool::Constraint(ConstraintAction::Concentric)
            );
            assert!(se.selection.is_empty(), "die unpassende Auswahl fällt weg");
        }
        // Esc beendet den Befehl; die Prüfungen unten gelten dem Sofort-Weg aus dem Auswählen.
        {
            let (se, s) = state(&mut app).session_sketch_mut().unwrap();
            se.cancel(s);
            assert_eq!(se.tool, planora_app::sketch_state::Tool::Select);
        }
        // Über `apply` sagt die Meldung dasselbe.
        select(&mut app, vec![Item::Curve(c1), Item::Curve(l)]);
        {
            let (se, s) = state(&mut app).session_sketch_mut().unwrap();
            assert!(se.apply(s, ConstraintAction::Concentric, None).is_err());
        }
        assert_eq!(message(&mut app), Some(tr!("sketch-concentric-kinds")));
        assert_eq!(sk(&mut app).revision(), rev);

        // Gemeinsame Mitte: Meldung, nichts angelegt.
        let c2 = sk(&mut app).add_circle(m, 9.0).unwrap();
        settle(&c, &mut app);
        let before = live(&mut app).len();
        let rev = sk(&mut app).revision();
        select(&mut app, vec![Item::Curve(c1), Item::Curve(c2)]);
        press(&c, &mut app, "Konzentrisch");
        assert_eq!(message(&mut app), Some(tr!("sketch-concentric-already")));
        assert_eq!(sk(&mut app).revision(), rev);
        assert_eq!(live(&mut app).len(), before);

        // Bogen und Ellipse gehen auch.
        let (am, a0, a1) = (
            pt(&mut app, 60.0, 0.0),
            pt(&mut app, 66.0, 0.0),
            pt(&mut app, 60.0, 6.0),
        );
        let arc = sk(&mut app).add_arc(am, a0, a1).unwrap();
        let em = pt(&mut app, 90.0, 5.0);
        let e = sk(&mut app)
            .add_ellipse(em, planora_math::DVec2::new(8.0, 0.0), 4.0)
            .unwrap();
        settle(&c, &mut app);
        select(&mut app, vec![Item::Curve(arc), Item::Curve(e)]);
        assert!(available(&mut app).contains(&ConstraintAction::Concentric));
        let dof_before = dof(&mut app);
        press(&c, &mut app, "Konzentrisch");
        assert_eq!(dof(&mut app), dof_before - 2, "{:?}", message(&mut app));
        assert_eq!(
            state(&mut app).sketch_session.as_ref().unwrap().tool,
            planora_app::sketch_state::Tool::Select,
            "mit passender Auswahl kein Befehl"
        );
    });
}

#[test]
fn collinear_takes_two_and_is_named_like_fusion() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (l1, ..) = line(&mut app, (0.0, 0.0), (10.0, 1.0));
        let (l2, ..) = line(&mut app, (20.0, 5.0), (30.0, 9.0));
        settle(&c, &mut app);
        select(&mut app, vec![Item::Curve(l1), Item::Curve(l2)]);
        assert!(available(&mut app).contains(&ConstraintAction::Collinear));
        assert!(!available(&mut app).contains(&ConstraintAction::Concentric));
        assert!(!available(&mut app).contains(&ConstraintAction::Smooth));
        let dof_before = dof(&mut app);
        let before = live(&mut app).len();
        let shot = press(&c, &mut app, "Kollinear");
        assert_eq!(live(&mut app).len(), before + 1);
        assert!(matches!(live(&mut app).last(), Some(K::Collinear(..))));
        assert_eq!(dof(&mut app), dof_before - 2);
        assert_eq!(message(&mut app), None);
        assert!(shot.count("Kollinear") >= 2, "Knopf und Listeneintrag");
        undo(&mut app);
        assert_eq!(live(&mut app).len(), before);
    });
}

#[test]
fn smooth_on_an_arc_says_g1_and_on_a_line_says_nothing() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        // Spline und Bogen am gemeinsamen Ende.
        let (m, st, en) = (
            pt(&mut app, 0.0, 0.0),
            pt(&mut app, 5.0, 0.0),
            pt(&mut app, 0.0, 5.0),
        );
        let arc = sk(&mut app).add_arc(m, st, en).unwrap();
        let q = [pt(&mut app, -5.0, 8.0), pt(&mut app, -10.0, 6.0)];
        let sp = sk(&mut app).add_spline(&[en, q[0], q[1]]).unwrap();
        settle(&c, &mut app);
        select(&mut app, vec![Item::Curve(sp), Item::Curve(arc)]);
        assert!(available(&mut app).contains(&ConstraintAction::Smooth));
        assert!(!available(&mut app).contains(&ConstraintAction::Concentric));
        let before = live(&mut app).len();
        press(&c, &mut app, "Glatt");
        assert_eq!(live(&mut app).len(), before + 1);
        assert!(matches!(live(&mut app).last(), Some(K::Smooth(..))));
        assert_eq!(message(&mut app), Some(tr!("sketch-smooth-arc-g1")));

        // Spline und Linie am gemeinsamen Ende: keine Meldung.
        let (l, _, joint) = line(&mut app, (40.0, 0.0), (50.0, 0.0));
        let r = [pt(&mut app, 55.0, 3.0), pt(&mut app, 60.0, 0.0)];
        let sp2 = sk(&mut app).add_spline(&[joint, r[0], r[1]]).unwrap();
        settle(&c, &mut app);
        select(&mut app, vec![Item::Curve(l), Item::Curve(sp2)]);
        let dof_before = dof(&mut app);
        let before = live(&mut app).len();
        press(&c, &mut app, "Glatt");
        assert_eq!(live(&mut app).len(), before + 1);
        assert_eq!(message(&mut app), None);
        assert_eq!(dof(&mut app), dof_before - 1);
        assert_eq!(sk(&mut app).clone().solve().status, SolveStatus::Solved);

        // Spline und Kreis oder Ellipse: kein Glatt.
        let (ci, _) = circle(&mut app, 0.0, 40.0, 3.0);
        select(&mut app, vec![Item::Curve(ci), Item::Curve(sp2)]);
        assert!(!available(&mut app).contains(&ConstraintAction::Smooth));
        {
            let (se, s) = state(&mut app).session_sketch_mut().unwrap();
            assert!(se.apply(s, ConstraintAction::Smooth, None).is_err());
        }
        assert_eq!(message(&mut app), Some(tr!("sketch-smooth-kinds")));
    });
}

#[test]
fn smooth_joins_separate_ends_in_one_undo_step() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (l, _, b) = line(&mut app, (-10.0, 0.0), (0.0, 0.0));
        let s0 = pt(&mut app, 2.0, 1.0);
        let r = [pt(&mut app, 6.0, 4.0), pt(&mut app, 12.0, 0.0)];
        let sp = sk(&mut app).add_spline(&[s0, r[0], r[1]]).unwrap();
        settle(&c, &mut app);
        let before = live(&mut app).len();
        select(&mut app, vec![Item::Curve(l), Item::Curve(sp)]);
        press(&c, &mut app, "Glatt");
        let now = live(&mut app);
        assert_eq!(now.len(), before + 2, "{:?}", message(&mut app));
        assert!(now.iter().any(
            |k| matches!(k, K::Coincident(x, y) if [*x, *y] == [b, s0] || [*x, *y] == [s0, b])
        ));
        assert!(now.iter().any(|k| matches!(k, K::Smooth(..))));
        let s = sk(&mut app);
        assert!(s.point_xy(b).unwrap().distance(s.point_xy(s0).unwrap()) < 1e-8);
        assert_eq!(s.clone().solve().status, SolveStatus::Solved);
        // Ein Rückgängig nimmt beides.
        undo(&mut app);
        assert_eq!(live(&mut app).len(), before);
        let s = sk(&mut app);
        assert!(s.point_xy(b).unwrap().distance(s.point_xy(s0).unwrap()) > 1.0);
    });
}

#[test]
fn redundant_constraints_keep_the_sketch_solved_and_say_so_truthfully() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        // Ganz überzählig: drei Kreise, die vierte (transitive) Konzentrisch-Bindung.
        let (a, _) = circle(&mut app, 0.0, 0.0, 3.0);
        let (b, _) = circle(&mut app, 10.0, 0.0, 5.0);
        let (d, _) = circle(&mut app, 20.0, 0.0, 7.0);
        settle(&c, &mut app);
        for pair in [[a, b], [b, d]] {
            select(&mut app, pair.map(Item::Curve).to_vec());
            press(&c, &mut app, "Konzentrisch");
            assert_eq!(message(&mut app), None);
        }
        let dof_before = dof(&mut app);
        let before = live(&mut app).len();
        select(&mut app, vec![Item::Curve(a), Item::Curve(d)]);
        press(&c, &mut app, "Konzentrisch");
        assert_eq!(live(&mut app).len(), before + 1);
        assert_eq!(message(&mut app), Some(tr!("sketch-redundant")));
        assert_eq!(dof(&mut app), dof_before);
        let report = sk(&mut app).clone().solve();
        assert_eq!(report.status, SolveStatus::Solved);
        assert!(report.conflicting.is_empty());

        // Teilweise überzählig: zwei waagerechte Linien, dann Kollinear. Die Winkelzeile ist
        // schon erfüllt, der Abstand nicht: Ein Freiheitsgrad fällt weg, die Meldung darf nicht
        // sagen, dass sich nichts geändert hätte.
        let (l1, ..) = line(&mut app, (0.0, 20.0), (10.0, 20.0));
        let (l2, ..) = line(&mut app, (20.0, 25.0), (30.0, 25.0));
        for l in [l1, l2] {
            select(&mut app, vec![Item::Curve(l)]);
            press(&c, &mut app, "Waagerecht");
        }
        let dof_before = dof(&mut app);
        select(&mut app, vec![Item::Curve(l1), Item::Curve(l2)]);
        press(&c, &mut app, "Kollinear");
        assert_eq!(dof(&mut app), dof_before - 1);
        assert!(matches!(live(&mut app).last(), Some(K::Collinear(..))));
        let report = sk(&mut app).clone().solve();
        assert_eq!(report.status, SolveStatus::Solved);
        assert!(report.conflicting.is_empty());
        // Der Löser meldet Kollinear als überzählig (die Winkelzeile), die Meldung sagt „zum Teil“.
        let id = sk(&mut app).constraints().last().unwrap().0;
        assert!(report.redundant.contains(&id), "{:?}", report.redundant);
        assert_eq!(message(&mut app), Some(tr!("sketch-redundant-partly")));
    });
}

#[test]
fn the_three_actions_are_bindable_and_apply_to_the_selection() {
    use planora_app::keymap::Area;
    let actions = [
        (
            Action::SketchConcentric,
            "skizze.konzentrisch",
            "Konzentrisch",
            "Concentric",
        ),
        (
            Action::SketchCollinear,
            "skizze.kollinear",
            "Kollinear",
            "Collinear",
        ),
        (Action::SketchSmooth, "skizze.glatt", "Glatt", "Smooth"),
    ];
    for (a, id, de, en) in actions {
        assert_eq!(a.id(), id);
        assert_eq!(Action::from_id(id), Some(a));
        assert_eq!(a.area(), Area::Sketch);
        assert!(a.default_bindings().is_empty(), "{a:?}");
        assert!(!a.repeatable(), "{a:?}");
        with_lang(&german(), || assert_eq!(a.label(), de));
        with_lang(&english(), || assert_eq!(a.label(), en));
    }
    with_lang(&german(), || {
        let mut app = sketching();
        let (l1, ..) = line(&mut app, (0.0, 0.0), (10.0, 1.0));
        let (l2, ..) = line(&mut app, (20.0, 5.0), (30.0, 9.0));
        select(&mut app, vec![Item::Curve(l1), Item::Curve(l2)]);
        planora_app::commands::run_action(app.ws_mut(), Action::SketchCollinear);
        assert!(matches!(live(&mut app).last(), Some(K::Collinear(..))));
    });
}

#[test]
fn the_new_buttons_speak_english() {
    with_lang(&english(), || {
        let c = context();
        let mut app = sketching();
        let (c1, _) = circle(&mut app, 0.0, 0.0, 5.0);
        let (c2, _) = circle(&mut app, 30.0, 10.0, 8.0);
        select(&mut app, vec![Item::Curve(c1), Item::Curve(c2)]);
        let shot = settle(&c, &mut app);
        for label in ["Concentric", "Collinear", "Smooth"] {
            assert!(shot.count(label) >= 1, "{label}");
        }
        let shot = press(&c, &mut app, "Concentric");
        assert!(shot.count("Concentric") >= 2, "Knopf und Listeneintrag");
        assert_eq!(
            ConstraintAction::Smooth.label(),
            "Smooth",
            "wie der Name in der Liste"
        );
    });
}

// ---- Korrektur 1 ------------------------------------------------------------------------------

/// Wählt die Kurven mit Klicks im Auswahlwerkzeug an den Stellen `at` (wie die Maus).
fn click_curves(app: &mut PlanoraApp, at: &[DVec2]) {
    let (se, s) = state(app).session_sketch_mut().unwrap();
    se.set_tool(s, planora_app::sketch_state::Tool::Select);
    for (i, &p) in at.iter().enumerate() {
        se.click(s, p, 0.3, i > 0).unwrap();
    }
}

/// Punkt auf der Kurve `c` beim Anteil `t` ihres Linienzugs.
fn on_curve(app: &mut PlanoraApp, c: CurveId, t: f64) -> DVec2 {
    let pl = sk(app).curve_polyline(c, 256).unwrap();
    pl[((pl.len() - 1) as f64 * t).round() as usize]
}

/// Bogen und Spline, die beide Enden teilen (geschlossene Schleife).
fn closed_loop(app: &mut PlanoraApp) -> (CurveId, CurveId, PointId, PointId) {
    let (m, st, en) = (pt(app, 0.0, 0.0), pt(app, 5.0, 0.0), pt(app, 0.0, 5.0));
    let arc = sk(app).add_arc(m, st, en).unwrap();
    let mid = pt(app, -6.0, -6.0);
    let sp = sk(app).add_spline(&[en, mid, st]).unwrap();
    (arc, sp, st, en)
}

#[test]
fn smooth_in_a_closed_loop_takes_the_clicked_end_in_any_order() {
    with_lang(&german(), || {
        let c = context();
        for near_end in [true, false] {
            for arc_first in [true, false] {
                let mut app = sketching();
                let (arc, sp, st, en) = closed_loop(&mut app);
                settle(&c, &mut app);
                // Klick auf beide Kurven nahe dem gewünschten Ende (nicht auf dem Punkt selbst).
                let (ta, ts) = if near_end { (0.8, 0.15) } else { (0.2, 0.85) };
                let on_arc = on_curve(&mut app, arc, ta);
                let on_sp = on_curve(&mut app, sp, ts);
                let clicks = if arc_first {
                    [on_arc, on_sp]
                } else {
                    [on_sp, on_arc]
                };
                click_curves(&mut app, &clicks);
                let want = if near_end { en } else { st };
                let picked = state(&mut app)
                    .sketch_session
                    .as_ref()
                    .unwrap()
                    .selection
                    .clone();
                assert_eq!(picked.len(), 2, "{picked:?}");
                press(&c, &mut app, "Glatt");
                let Some(K::Smooth(_, pa, _, pb)) = live(&mut app).last().cloned() else {
                    panic!("kein Glatt: {:?}", message(&mut app));
                };
                assert_eq!(
                    (pa, pb),
                    (want, want),
                    "Ende {near_end}, Bogen zuerst {arc_first}"
                );
                assert!(
                    !live(&mut app)
                        .iter()
                        .any(|k| matches!(k, K::Coincident(..))),
                    "gemeinsames Ende: kein Deckungsgleich"
                );
            }
        }
    });
}

#[test]
fn smooth_joins_far_apart_curves_at_the_clicked_ends_in_one_undo_step() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (_, a, _) = line(&mut app, (-10.0, 0.0), (0.0, 0.0));
        let s0 = pt(&mut app, 200.0, 150.0);
        let r = [pt(&mut app, 206.0, 154.0), pt(&mut app, 212.0, 150.0)];
        let sp = sk(&mut app).add_spline(&[s0, r[0], r[1]]).unwrap();
        settle(&c, &mut app);
        let before = live(&mut app).len();
        // Klick auf die Linie nahe ihrem Anfang, auf den Spline nahe seinem Ende.
        let on_sp = on_curve(&mut app, sp, 0.9);
        click_curves(&mut app, &[DVec2::new(-8.0, 0.0), on_sp]);
        press(&c, &mut app, "Glatt");
        let now = live(&mut app);
        assert_eq!(now.len(), before + 2, "{:?}", message(&mut app));
        let end = r[1];
        assert!(now.iter().any(
            |k| matches!(k, K::Coincident(x, y) if [*x, *y] == [a, end] || [*x, *y] == [end, a])
        ));
        assert!(now.iter().any(|k| matches!(k, K::Smooth(..))));
        let s = sk(&mut app);
        assert!(s.point_xy(a).unwrap().distance(s.point_xy(end).unwrap()) < 1e-8);
        assert_eq!(s.clone().solve().status, SolveStatus::Solved);
        // Ein Rückgängig nimmt Deckungsgleich und Glatt zusammen.
        undo(&mut app);
        assert_eq!(live(&mut app).len(), before);
        let s = sk(&mut app);
        assert!(s.point_xy(a).unwrap().distance(s.point_xy(end).unwrap()) > 100.0);
    });
}

#[test]
fn a_refused_concentric_leaves_no_undo_step_and_the_revision() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (c1, m1) = circle(&mut app, 0.0, 0.0, 5.0);
        let (c2, m2) = circle(&mut app, 30.0, 10.0, 8.0);
        sk(&mut app).add_constraint(K::Fixed(m1, 0.0, 0.0)).unwrap();
        sk(&mut app)
            .add_constraint(K::Fixed(m2, 30.0, 10.0))
            .unwrap();
        settle(&c, &mut app);
        let n = live(&mut app).len();
        let rev = sk(&mut app).revision();
        select(&mut app, vec![Item::Curve(c1), Item::Curve(c2)]);
        press(&c, &mut app, "Konzentrisch");
        assert!(message(&mut app).is_some());
        assert_eq!(sk(&mut app).revision(), rev);
        assert_eq!(live(&mut app).len(), n);
        // Rückgängig nimmt den letzten echten Schritt (die Kreise samt Festsetzen), keinen leeren.
        undo(&mut app);
        assert!(live(&mut app).len() < n);
    });
}

#[test]
fn concentric_between_a_pattern_source_and_its_instance_is_refused() {
    use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan};
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (src, _) = circle(&mut app, 0.0, 0.0, 3.0);
        let pid = sk(&mut app)
            .add_pattern(&PatternPlan {
                kind: PatternKind::Rect {
                    dirs: [PatternDir::SketchX, PatternDir::SketchY],
                    extent: [false, false],
                    symmetric: [false, false],
                },
                sources: vec![src],
                counts: [2, 1],
                values: [20.0, 10.0],
                suppressed: vec![],
            })
            .unwrap();
        sk(&mut app).solve();
        let inst = sk(&mut app).pattern(pid).unwrap().instances[0].curves[0];
        settle(&c, &mut app);
        let n = live(&mut app).len();
        let rev = sk(&mut app).revision();
        select(&mut app, vec![Item::Curve(src), Item::Curve(inst)]);
        press(&c, &mut app, "Konzentrisch");
        // Der Text kommt aus der Bibliothek (mit „Ungültige Eingabe: …“ davor).
        let text = message(&mut app).unwrap_or_default();
        assert!(text.contains(&tr!("sketch-pattern-cycle")), "{text}");
        assert_eq!(sk(&mut app).revision(), rev);
        assert_eq!(live(&mut app).len(), n);
    });
}

#[test]
fn collinear_works_with_a_centerline() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (l1, ..) = line(&mut app, (0.0, 0.0), (10.0, 1.0));
        let (axis, ..) = line(&mut app, (20.0, 5.0), (30.0, 9.0));
        sk(&mut app).set_centerline(axis, true).unwrap();
        settle(&c, &mut app);
        select(&mut app, vec![Item::Curve(l1), Item::Curve(axis)]);
        assert!(available(&mut app).contains(&ConstraintAction::Collinear));
        let dof_before = dof(&mut app);
        press(&c, &mut app, "Kollinear");
        assert!(matches!(live(&mut app).last(), Some(K::Collinear(..))));
        assert_eq!(dof(&mut app), dof_before - 2);
        assert_eq!(message(&mut app), None);
        let s = sk(&mut app).clone();
        let centers = sketch_view::round_centers(&s);
        let kind = live(&mut app).last().cloned().unwrap();
        assert_eq!(
            sketch_view::overlay_mark(&centers, &kind),
            Some(sketch_view::PaintedMark::Collinear)
        );
    });
}

#[test]
fn smooth_without_clicks_does_not_depend_on_the_selection_order() {
    with_lang(&german(), || {
        let c = context();
        let mut joints = Vec::new();
        for arc_first in [true, false] {
            let mut app = sketching();
            let (arc, sp, ..) = closed_loop(&mut app);
            settle(&c, &mut app);
            let order = if arc_first { [arc, sp] } else { [sp, arc] };
            // Ohne Klicklage, wie nach einer Rahmenauswahl.
            select(&mut app, order.map(Item::Curve).to_vec());
            press(&c, &mut app, "Glatt");
            let Some(K::Smooth(_, pa, _, pb)) = live(&mut app).last().cloned() else {
                panic!("kein Glatt: {:?}", message(&mut app));
            };
            assert_eq!(pa, pb);
            joints.push(pa);
        }
        assert_eq!(joints[0], joints[1]);
    });
}

/// Kreise ohne Füllung im Bild (die gezeichneten Zeichen; Punkte sind gefüllt).
fn open_circles(c: &egui::Context, app: &mut PlanoraApp) -> usize {
    fn walk(shape: &Shape, n: &mut usize) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, n)),
            Shape::Circle(k) if k.fill == egui::Color32::TRANSPARENT => *n += 1,
            _ => {}
        }
    }
    settle(c, app);
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(vec![]), |ui| app.ui(ui, &mut f));
    let mut n = 0;
    for s in &out.shapes {
        walk(&s.shape, &mut n);
    }
    out.drop_without_applying_deltas();
    n
}

#[test]
fn the_concentric_mark_is_painted_in_the_view() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (c1, _) = circle(&mut app, 0.0, 0.0, 5.0);
        let (c2, _) = circle(&mut app, 30.0, 10.0, 8.0);
        let before = open_circles(&c, &mut app);
        select(&mut app, vec![Item::Curve(c1), Item::Curve(c2)]);
        press(&c, &mut app, "Konzentrisch");
        select(&mut app, vec![]);
        let after = open_circles(&c, &mut app);
        assert_eq!(after, before + 2, "zwei Kreise des Zeichens");
    });
}

// ---- Korrektur 2 ------------------------------------------------------------------------------

/// Die Enden des zuletzt angelegten Glatt.
fn last_joint(app: &mut PlanoraApp) -> (PointId, PointId) {
    live(app)
        .iter()
        .rev()
        .find_map(|k| match k {
            K::Smooth(_, a, _, b) => Some((*a, *b)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("kein Glatt: {:?}", message(app)))
}

/// Wie sich die Auswahl nach den Klicks ändert (nicht durch einen Klick im Auswählen).
#[derive(Clone, Copy, Debug)]
enum Reselect {
    /// Nur den Bogen anklicken, dann beide direkt setzen (wie Liste oder Code ohne Klick).
    Direct,
    /// Über die Kette (Doppelklick).
    Chain,
    /// Über einen Rahmen um beide Kurven.
    Box,
    /// Werkzeugwechsel und zurück, dann neu gesetzt.
    ToolChange,
}

#[test]
fn a_click_counts_only_for_the_selection_it_made() {
    with_lang(&german(), || {
        let c = context();
        // Ohne Klick: der Ersatz (kleinste Lücke, bei Gleichstand nach Punktnummer).
        let mut app = sketching();
        let (arc, sp, st, en) = closed_loop(&mut app);
        settle(&c, &mut app);
        select(&mut app, vec![Item::Curve(arc), Item::Curve(sp)]);
        press(&c, &mut app, "Glatt");
        let fallback = last_joint(&mut app);
        // Die Klicks unten liegen am anderen Ende, sonst bewiese der Test nichts.
        let clicked = if fallback.0 == en { st } else { en };
        let (ta, ts) = if clicked == en {
            (0.8, 0.15)
        } else {
            (0.2, 0.85)
        };

        for how in [
            Reselect::Direct,
            Reselect::Chain,
            Reselect::Box,
            Reselect::ToolChange,
        ] {
            let mut app = sketching();
            let (arc, sp, ..) = closed_loop(&mut app);
            settle(&c, &mut app);
            let clicks = [on_curve(&mut app, arc, ta), on_curve(&mut app, sp, ts)];
            match how {
                Reselect::Direct => click_curves(&mut app, &clicks[..1]),
                _ => click_curves(&mut app, &clicks),
            }
            {
                let (se, s) = state(&mut app).session_sketch_mut().unwrap();
                match how {
                    Reselect::Direct => se.selection = vec![Item::Curve(arc), Item::Curve(sp)],
                    Reselect::Chain => se.select_chain(s, arc, false),
                    Reselect::Box => se.select_rect(
                        s,
                        DVec2::new(-20.0, -20.0),
                        DVec2::new(20.0, 20.0),
                        false,
                        &|p| Some(p),
                    ),
                    // Die Auswahl bleibt beim Werkzeugwechsel, die Klicklagen nicht.
                    Reselect::ToolChange => {
                        se.set_tool(s, planora_app::sketch_state::Tool::Line);
                        se.set_tool(s, planora_app::sketch_state::Tool::Select);
                    }
                }
                let mut picked = se.selection.clone();
                picked.sort_by_key(|i| format!("{i:?}"));
                let mut want = vec![Item::Curve(arc), Item::Curve(sp)];
                want.sort_by_key(|i| format!("{i:?}"));
                assert_eq!(picked, want, "{how:?}");
            }
            press(&c, &mut app, "Glatt");
            assert_eq!(last_joint(&mut app), fallback, "{how:?}");
        }

        // Gegenprobe: Ohne Umweg gilt der Klick.
        let mut app = sketching();
        let (arc, sp, ..) = closed_loop(&mut app);
        settle(&c, &mut app);
        let clicks = [on_curve(&mut app, arc, ta), on_curve(&mut app, sp, ts)];
        click_curves(&mut app, &clicks);
        press(&c, &mut app, "Glatt");
        assert_eq!(last_joint(&mut app), (clicked, clicked));
    });
}

#[test]
fn a_curve_clicked_off_and_on_again_takes_the_new_click() {
    with_lang(&german(), || {
        let c = context();
        let mut app = sketching();
        let (arc, sp, st, _) = closed_loop(&mut app);
        settle(&c, &mut app);
        let (a_en, a_st) = (on_curve(&mut app, arc, 0.8), on_curve(&mut app, arc, 0.2));
        let s_st = on_curve(&mut app, sp, 0.85);
        // Bogen nahe en, Spline nahe st; dann den Bogen ab- und nahe st wieder anwählen.
        click_curves(&mut app, &[a_en, s_st]);
        {
            let (se, s) = state(&mut app).session_sketch_mut().unwrap();
            se.click(s, a_en, 0.3, true).unwrap();
            se.click(s, a_st, 0.3, true).unwrap();
        }
        press(&c, &mut app, "Glatt");
        assert_eq!(last_joint(&mut app), (st, st));
    });
}

#[test]
fn smooth_prefers_a_shared_point_over_coincident_twins() {
    with_lang(&german(), || {
        let c = context();
        // Geschlossener Spline mit zwei deckenden Enden (Deckungsgleich), die Linie endet am
        // zweiten: Glatt nimmt genau diesen gemeinsamen Punkt.
        let mut app = sketching();
        let a0 = pt(&mut app, 0.0, 0.0);
        let q1 = pt(&mut app, 5.0, 5.0);
        let q2 = pt(&mut app, 10.0, 0.0);
        let a1 = pt(&mut app, 0.0, 0.0);
        let sp = sk(&mut app).add_spline(&[a0, q1, q2, a1]).unwrap();
        sk(&mut app).add_constraint(K::Coincident(a0, a1)).unwrap();
        let e = pt(&mut app, -10.0, 0.0);
        let l = sk(&mut app).add_line(e, a1).unwrap();
        settle(&c, &mut app);
        let before = live(&mut app).len();
        for order in [[sp, l], [l, sp]] {
            select(&mut app, order.map(Item::Curve).to_vec());
            press(&c, &mut app, "Glatt");
            assert_eq!(last_joint(&mut app), (a1, a1), "{order:?}");
            assert_eq!(
                live(&mut app).len(),
                before + 1,
                "kein neues Deckungsgleich"
            );
            undo(&mut app);
        }
    });
}

/// Schlusskorrektur I1: Waagerecht nur an einer Exemplarlinie hätte keine Wirkung; der Knopf
/// lehnt mit der Meldung ab, die Skizze bleibt (auch die Revision).
#[test]
fn horizontal_on_an_instance_line_is_refused_with_a_message() {
    use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan};
    for lang in [german(), english()] {
        with_lang(&lang, || {
            let c = context();
            let mut app = sketching();
            let (src, _, _) = line(&mut app, (0.0, 0.0), (10.0, 4.0));
            let pid = sk(&mut app)
                .add_pattern(&PatternPlan {
                    kind: PatternKind::Rect {
                        dirs: [PatternDir::SketchX, PatternDir::SketchY],
                        extent: [false, false],
                        symmetric: [false, false],
                    },
                    sources: vec![src],
                    counts: [2, 1],
                    values: [20.0, 10.0],
                    suppressed: vec![],
                })
                .unwrap();
            sk(&mut app).solve();
            let inst = sk(&mut app).pattern(pid).unwrap().instances[0].curves[0];
            settle(&c, &mut app);
            let n = live(&mut app).len();
            let rev = sk(&mut app).revision();
            select(&mut app, vec![Item::Curve(inst)]);
            press(&c, &mut app, &tr!("sketch-constraint-horizontal"));
            let text = message(&mut app).unwrap_or_default();
            assert!(
                text.contains(&tr!("sketch-pattern-instance-only")),
                "{text}"
            );
            assert_eq!(sk(&mut app).revision(), rev);
            assert_eq!(live(&mut app).len(), n);
            // An der Vorlage geht es, und das Exemplar folgt.
            select(&mut app, vec![Item::Curve(src)]);
            press(&c, &mut app, &tr!("sketch-constraint-horizontal"));
            assert_eq!(live(&mut app).len(), n + 1);
            let s = sk(&mut app);
            let CurveKind::Line { a, b } = s.curve(inst).unwrap().kind else {
                panic!("Linie")
            };
            let (pa, pb) = (s.point_xy(a).unwrap(), s.point_xy(b).unwrap());
            assert!((pa.y - pb.y).abs() < 1e-9, "{pa:?} {pb:?}");
        });
    }
}

/// Schlusskorrektur (Minor 7): Löschen ist eine Einheit. Lehnt die Skizze das Abwählen eines
/// Exemplars ab (es brächte ein anderes Muster in eine Schleife), bleibt alles stehen, auch die
/// anderen gewählten Kurven; die Meldung nennt den Grund.
#[test]
fn delete_is_one_unit_when_a_suppression_is_refused() {
    use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan};
    let plan = |sources: Vec<CurveId>, spacing: f64| PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources,
        counts: [3, 1],
        values: [spacing, 10.0],
        suppressed: vec![],
    };
    with_lang(&german(), || {
        let mut app = sketching();
        let s = sk(&mut app);
        let rq = s
            .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
            .unwrap();
        let q = s.add_pattern(&plan(rq.lines.to_vec(), 20.0)).unwrap();
        let corner = |s: &Sketch, id, src: PointId, at: [i32; 2]| {
            let d = s.pattern(id).unwrap();
            let k = d.source_points.iter().position(|&x| x == src).unwrap();
            d.instances
                .iter()
                .find(|i| i.at == at)
                .unwrap()
                .clone()
                .points[k]
        };
        let src = rq.points[0];
        let (qi1, qi2) = (corner(s, q, src, [1, 0]), corner(s, q, src, [2, 0]));
        let rp = s
            .add_rectangle(DVec2::new(10.0, 50.0), DVec2::new(14.0, 54.0))
            .unwrap();
        let p = s.add_pattern(&plan(rp.lines.to_vec(), 10.0)).unwrap();
        let vpt = corner(s, p, rp.points[0], [2, 0]);
        s.add_arc(vpt, qi1, qi2).unwrap();
        let e = s.add_point(-30.0, 50.0).unwrap();
        s.add_arc(src, vpt, e).unwrap();
        s.solve();
        let inst = s
            .pattern(p)
            .unwrap()
            .instances
            .iter()
            .find(|i| i.at == [2, 0])
            .unwrap()
            .curves[0];
        let (free, _, _) = line(&mut app, (-50.0, -50.0), (-40.0, -50.0));
        let rev = sk(&mut app).revision();
        let curves = sk(&mut app).curves().count();
        select(&mut app, vec![Item::Curve(free), Item::Curve(inst)]);
        {
            let (se, s) = state(&mut app).session_sketch_mut().unwrap();
            se.delete_selection(s);
        }
        let text = message(&mut app).unwrap_or_default();
        assert!(text.contains(&tr!("sketch-pattern-cycle")), "{text}");
        assert_eq!(sk(&mut app).revision(), rev, "nichts gelöscht");
        assert_eq!(sk(&mut app).curves().count(), curves);
        assert!(sk(&mut app).curve(free).is_some());
        assert_eq!(sk(&mut app).pattern(p).unwrap().instances.len(), 2);
        // Ohne das Exemplar geht das Löschen der Linie.
        select(&mut app, vec![Item::Curve(free)]);
        let (se, s) = state(&mut app).session_sketch_mut().unwrap();
        se.delete_selection(s);
        assert!(s.curve(free).is_none());
    });
}
