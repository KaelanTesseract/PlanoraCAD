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
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, PointId, Sketch};
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
        // Liste und Ansicht nennen es „Konzentrisch“ bzw. zeigen ◎.
        let s = sk(&mut app).clone();
        assert_eq!(
            sketch_view::constraint_name(&s, &kind, false),
            "Konzentrisch"
        );
        assert!(shot.count("Konzentrisch") >= 2, "Knopf und Listeneintrag");
        let mark = sketch_view::overlay_text(&c, &s, &kind, false);
        assert!(mark == "◎" || mark == "(o)", "{mark}");
        assert!(shot.count(&mark) >= 1, "Zeichen in der Ansicht");
        // Ein gewöhnliches Deckungsgleich bleibt es.
        let p = pt(&mut app, 1.0, 1.0);
        let q = pt(&mut app, 2.0, 2.0);
        let s = sk(&mut app).clone();
        assert!(!sketch_view::is_concentric(&s, &K::Coincident(p, q)));
        assert!(!sketch_view::is_concentric(&s, &K::Coincident(m1, p)));
        settle(&c, &mut app);
        undo(&mut app);
        // Ein Rückgängig nimmt Konzentrisch zurück.
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
        // Kreis und Linie: Knopf aus, ein Klick ändert nichts.
        select(&mut app, vec![Item::Curve(c1), Item::Curve(l)]);
        assert!(!available(&mut app).contains(&ConstraintAction::Concentric));
        let rev = sk(&mut app).revision();
        press(&c, &mut app, "Konzentrisch");
        assert_eq!(sk(&mut app).revision(), rev);
        // Über die Aktion (Taste) sagt die Meldung, warum.
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
