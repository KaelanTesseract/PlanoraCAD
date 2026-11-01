//! Rahmenauswahl und Kettenwahl (Skizzen 2, Task 7): `select_rect` und `select_chain` direkt und
//! über Rohereignisse gegen das Overlay bei 1920 × 1080.

use planora_app::sketch_state::{ConstraintAction, Item, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, sketch_view, theme};
use planora_feature::SketchPlane;
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(W, H),
};
const LEFT: egui::PointerButton = egui::PointerButton::Primary;
const RIGHT: egui::PointerButton = egui::PointerButton::Secondary;
const MIDDLE: egui::PointerButton = egui::PointerButton::Middle;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn sketch_with(build: impl FnOnce(&mut Sketch)) -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        build(sk);
        sk.solve();
    }
    s.observe_sketch(false);
    s
}

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = sk.add_point(a.0, a.1).unwrap();
    let q = sk.add_point(b.0, b.1).unwrap();
    sk.add_line(p, q).unwrap()
}

fn ends(sk: &Sketch, c: CurveId) -> (PointId, PointId) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        _ => panic!("keine Linie"),
    }
}

fn curves(s: &AppState) -> Vec<CurveId> {
    s.session_sketch()
        .unwrap()
        .curves()
        .map(|(c, _)| c)
        .collect()
}

fn selection(s: &AppState) -> Vec<Item> {
    s.sketch_session.as_ref().unwrap().selection.clone()
}

fn select(s: &mut AppState, items: &[Item]) {
    s.sketch_session.as_mut().unwrap().selection = items.to_vec();
}

/// Dieselben Elemente, gleich welche Reihenfolge.
#[track_caller]
fn assert_selected(s: &AppState, expected: &[Item]) {
    let got = selection(s);
    assert_eq!(got.len(), expected.len(), "{got:?} statt {expected:?}");
    for item in expected {
        assert!(got.contains(item), "{item:?} fehlt in {got:?}");
    }
}

fn curve_items(c: &[CurveId]) -> Vec<Item> {
    c.iter().map(|&c| Item::Curve(c)).collect()
}

/// `select_rect` mit Skizzenkoordinaten als Bildschirmlagen.
fn rect(s: &mut AppState, a: (f64, f64), b: (f64, f64), extend: bool) {
    rect_through(s, a, b, extend, &|p| Some(p));
}

fn rect_through(
    s: &mut AppState,
    a: (f64, f64),
    b: (f64, f64),
    extend: bool,
    project: &dyn Fn(DVec2) -> Option<DVec2>,
) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.select_rect(
        sk,
        DVec2::new(a.0, a.1),
        DVec2::new(b.0, b.1),
        extend,
        project,
    );
}

// ---- Rahmen ohne Oberfläche -----------------------------------------------------------------

#[test]
fn left_to_right_takes_what_lies_fully_inside() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (10.0, 0.0));
        line(sk, (0.0, 5.0), (30.0, 5.0));
    });
    let c = curves(&s);
    rect(&mut s, (-1.0, -1.0), (20.0, 10.0), false);
    assert_selected(&s, &curve_items(&c[..1]));
}

#[test]
fn right_to_left_also_takes_what_it_touches() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (10.0, 0.0));
        line(sk, (0.0, 5.0), (30.0, 5.0));
        line(sk, (50.0, 50.0), (60.0, 60.0));
        // Läuft quer durch den Rahmen, beide Enden draußen.
        line(sk, (5.0, -40.0), (5.0, 40.0));
    });
    let c = curves(&s);
    rect(&mut s, (20.0, 10.0), (-1.0, -1.0), false);
    assert_selected(&s, &curve_items(&[c[0], c[1], c[3]]));
}

#[test]
fn crossing_finds_a_circle_without_any_sample_inside() {
    let mut s = sketch_with(|sk| {
        let m = sk.add_point(0.0, 0.0).unwrap();
        sk.add_circle(m, 100.0).unwrap();
    });
    let c = curves(&s);
    // Mitte der ersten Sehne des Linienzugs (96 Schritte je Vollkreis): die Stützpunkte liegen
    // über 3 Einheiten weg.
    let half = std::f64::consts::TAU / 192.0;
    let mid = DVec2::new(half.cos(), half.sin()) * 100.0 * half.cos();
    let (lo, hi) = ((mid.x - 0.1, mid.y - 0.1), (mid.x + 0.1, mid.y + 0.1));
    rect(&mut s, hi, lo, false);
    assert_selected(&s, &curve_items(&c));
    rect(&mut s, lo, hi, false);
    assert_selected(&s, &[]);
}

#[test]
fn screen_direction_counts_also_seen_from_behind() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (10.0, 0.0));
        line(sk, (0.0, 5.0), (30.0, 5.0));
    });
    let c = curves(&s);
    let behind = |p: DVec2| Some(DVec2::new(-p.x, p.y));
    // Auf dem Bildschirm von links nach rechts (in der Skizze umgekehrt): bleibt Fenster.
    rect_through(&mut s, (-11.0, -1.0), (1.0, 6.0), false, &behind);
    assert_selected(&s, &curve_items(&c[..1]));
    rect_through(&mut s, (1.0, 6.0), (-11.0, -1.0), false, &behind);
    assert_selected(&s, &curve_items(&c));
}

#[test]
fn shift_adds_without_toggling_a_plain_box_replaces() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (10.0, 0.0));
        line(sk, (0.0, 20.0), (10.0, 20.0));
    });
    let c = curves(&s);
    rect(&mut s, (-1.0, -1.0), (11.0, 1.0), false);
    assert_selected(&s, &curve_items(&c[..1]));
    rect(&mut s, (-1.0, 19.0), (11.0, 21.0), true);
    assert_selected(&s, &curve_items(&c));
    // Noch einmal über das Gewählte: nichts wird abgewählt oder doppelt.
    rect(&mut s, (-1.0, 19.0), (11.0, 21.0), true);
    assert_selected(&s, &curve_items(&c));
    // Leer mit Umschalt: bleibt; gewöhnlich: ersetzt; leer: leert.
    rect(&mut s, (100.0, 100.0), (110.0, 110.0), true);
    assert_selected(&s, &curve_items(&c));
    rect(&mut s, (-1.0, 19.0), (11.0, 21.0), false);
    assert_selected(&s, &curve_items(&c[1..]));
    rect(&mut s, (100.0, 100.0), (110.0, 110.0), false);
    assert_selected(&s, &[]);
}

#[test]
fn only_free_points_are_taken() {
    let mut free = None;
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (10.0, 0.0));
        free = Some(sk.add_point(5.0, 5.0).unwrap());
        let m = sk.add_point(30.0, 0.0).unwrap();
        sk.add_circle(m, 2.0).unwrap();
    });
    let c = curves(&s);
    rect(&mut s, (-1.0, -3.0), (40.0, 10.0), false);
    assert_selected(
        &s,
        &[
            Item::Curve(c[0]),
            Item::Curve(c[1]),
            Item::Point(free.unwrap()),
        ],
    );
    // Nur Endpunkt im Rahmen: nichts, auch nicht von rechts nach links um die Mitte des Kreises.
    rect(&mut s, (-1.0, -1.0), (1.0, 1.0), false);
    assert_selected(&s, &[]);
    rect(&mut s, (31.0, 1.0), (29.0, -1.0), false);
    assert_selected(&s, &[]);
}

#[test]
fn the_constraint_actions_follow_a_box_around_two_lines() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (10.0, 0.0));
        line(sk, (0.0, 5.0), (10.0, 7.0));
    });
    rect(&mut s, (-1.0, -1.0), (11.0, 8.0), false);
    let (se, sk) = s.session_sketch_mut().unwrap();
    let actions = se.available_actions(sk);
    assert!(actions.contains(&ConstraintAction::Parallel), "{actions:?}");
    assert!(actions.contains(&ConstraintAction::Angle), "{actions:?}");
}

#[test]
fn what_falls_behind_the_camera_is_not_taken() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (10.0, 0.0));
        let m = sk.add_point(30.0, 0.0).unwrap();
        sk.add_circle(m, 5.0).unwrap();
    });
    let c = curves(&s);
    rect_through(&mut s, (100.0, 100.0), (-100.0, -100.0), false, &|_| None);
    assert_selected(&s, &[]);
    // Zum Teil hinter der Kamera: Das Fenster enthält keine der Kurven ganz …
    let front = |p: DVec2| (p.x >= 5.0 && p.x < 31.0).then_some(p);
    rect_through(&mut s, (-100.0, -100.0), (100.0, 100.0), false, &front);
    assert_selected(&s, &[]);
    // … von der Linie liegt keine ganze Strecke davor; der Kreis berührt den Rahmen im
    // sichtbaren Teil.
    rect_through(&mut s, (100.0, 100.0), (-100.0, -100.0), false, &front);
    assert_selected(&s, &curve_items(&c[1..]));
}

// ---- Kette ohne Oberfläche ------------------------------------------------------------------

#[test]
fn select_chain_replaces_or_adds() {
    let mut s = sketch_with(|sk| {
        sk.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(4.0, 2.0))
            .unwrap();
        line(sk, (10.0, 0.0), (12.0, 0.0));
    });
    let c = curves(&s);
    select(&mut s, &[Item::Curve(c[4])]);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.select_chain(sk, c[1], false);
    }
    assert_selected(&s, &curve_items(&c[..4]));
    select(&mut s, &[Item::Curve(c[4]), Item::Curve(c[0])]);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.select_chain(sk, c[1], true);
    }
    assert_selected(&s, &curve_items(&c));
}

// ---- Rohereignisse --------------------------------------------------------------------------

/// Ein Bild: Umschalt gehalten und die Ereignisse.
type Frame = (bool, Vec<egui::Event>);

fn screen(s: &AppState, p: (f64, f64)) -> egui::Pos2 {
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let v =
        sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), DVec2::new(p.0, p.1))
            .unwrap();
    egui::pos2(v.x, v.y)
}

fn moved(p: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(p)
}

fn mods(shift: bool) -> egui::Modifiers {
    egui::Modifiers {
        shift,
        ..Default::default()
    }
}

fn button(p: egui::Pos2, button: egui::PointerButton, pressed: bool, shift: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button,
        pressed,
        modifiers: mods(shift),
    }
}

fn esc() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

/// Spielt je Eintrag ein Bild ab und schreibt danach den Verlauf fort wie die App. Ein leeres Bild
/// vorweg: egui ordnet Zeiger-Ereignisse den Flächen des vorigen Bilds zu.
fn play(ctx: &egui::Context, s: &mut AppState, frames: Vec<Frame>) {
    for (shift, events) in std::iter::once((false, vec![])).chain(frames) {
        let mut all = vec![egui::Event::ModifiersChanged(mods(shift))];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            events: all,
            ..Default::default()
        };
        let mut gesture = false;
        ctx.run_ui(raw, |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
            gesture = ui.input(|i| i.pointer.any_down());
        })
        .drop_without_applying_deltas();
        s.observe_sketch(gesture);
    }
}

/// Zug mit der Taste `b`, ohne das Loslassen.
fn drag_frames(
    from: egui::Pos2,
    to: egui::Pos2,
    b: egui::PointerButton,
    shift: bool,
) -> Vec<Frame> {
    vec![
        (shift, vec![moved(from)]),
        (shift, vec![button(from, b, true, shift)]),
        (shift, vec![moved(from.lerp(to, 0.5))]),
        (shift, vec![moved(to)]),
    ]
}

/// Zug mit Loslassen.
fn full_drag(from: egui::Pos2, to: egui::Pos2, b: egui::PointerButton, shift: bool) -> Vec<Frame> {
    let mut frames = drag_frames(from, to, b, shift);
    frames.push((shift, vec![button(to, b, false, shift)]));
    frames
}

fn click_frames(p: egui::Pos2) -> Vec<Frame> {
    vec![
        (false, vec![moved(p)]),
        (false, vec![button(p, LEFT, true, false)]),
        (false, vec![button(p, LEFT, false, false)]),
    ]
}

fn double_click_frames(p: egui::Pos2) -> Vec<Frame> {
    let mut frames = click_frames(p);
    frames.push((false, vec![button(p, LEFT, true, false)]));
    frames.push((false, vec![button(p, LEFT, false, false)]));
    frames
}

fn box_active(s: &AppState) -> bool {
    s.sketch_session.as_ref().unwrap().box_select.is_some()
}

/// Zwei waagerechte Linien und viel freie Fläche; jede Lage ist auf dem Bildschirm gut getrennt.
fn two_lines() -> AppState {
    let s = sketch_with(|sk| {
        line(sk, (-10.0, 0.0), (10.0, 0.0));
        line(sk, (-10.0, 20.0), (40.0, 20.0));
    });
    assert!(
        (screen(&s, (10.0, 0.0)).x - screen(&s, (-10.0, 0.0)).x).abs() > 60.0,
        "Ansicht zu klein für den Test"
    );
    s
}

#[test]
fn a_drag_on_free_area_draws_a_box_and_selects_in_both_directions() {
    let ctx = context();
    let mut s = two_lines();
    let c = curves(&s);
    let steps = s.sketch_history().unwrap().undo_len();
    let (a, b) = (screen(&s, (-15.0, -8.0)), screen(&s, (20.0, 28.0)));

    // Von links nach rechts: nur die erste Linie liegt ganz drin.
    play(&ctx, &mut s, drag_frames(a, b, LEFT, false));
    let frame = s.sketch_session.as_ref().unwrap().box_select.unwrap();
    assert!(!frame.extend);
    assert_eq!(frame.start, DVec2::new(a.x as f64, a.y as f64));
    assert_eq!(frame.current, DVec2::new(b.x as f64, b.y as f64));
    play(
        &ctx,
        &mut s,
        vec![(false, vec![button(b, LEFT, false, false)])],
    );
    assert!(!box_active(&s));
    assert_selected(&s, &curve_items(&c[..1]));

    // Von rechts nach links: auch die zweite.
    play(&ctx, &mut s, full_drag(b, a, LEFT, false));
    assert_selected(&s, &curve_items(&c));
    assert_eq!(
        s.sketch_history().unwrap().undo_len(),
        steps,
        "Auswählen ist kein Rückgängig-Schritt"
    );
}

#[test]
fn a_drag_on_a_line_or_a_point_starts_no_box() {
    let ctx = context();
    let mut s = two_lines();
    let c = curves(&s);
    select(&mut s, &[Item::Curve(c[1])]);
    let (on_line, to) = (screen(&s, (0.0, 0.0)), screen(&s, (20.0, 40.0)));
    play(&ctx, &mut s, drag_frames(on_line, to, LEFT, false));
    assert!(!box_active(&s), "Zug auf der Linie");
    play(
        &ctx,
        &mut s,
        vec![(false, vec![button(to, LEFT, false, false)])],
    );
    assert_selected(&s, &curve_items(&c[1..]));

    let on_end = screen(&s, (10.0, 0.0));
    play(&ctx, &mut s, drag_frames(on_end, to, LEFT, false));
    assert!(!box_active(&s), "Zug am Punkt");
    assert!(s.sketch_session.as_ref().unwrap().dragging.is_some());
    play(
        &ctx,
        &mut s,
        vec![(false, vec![button(to, LEFT, false, false)])],
    );
}

#[test]
fn only_the_select_tool_draws_a_box() {
    let ctx = context();
    let mut s = two_lines();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Trim);
    }
    let (a, b) = (screen(&s, (-15.0, -8.0)), screen(&s, (-15.0, 40.0)));
    play(&ctx, &mut s, drag_frames(a, b, LEFT, false));
    assert!(!box_active(&s));
    play(
        &ctx,
        &mut s,
        vec![(false, vec![button(b, LEFT, false, false)])],
    );
}

#[test]
fn right_click_and_esc_abort_the_box_and_keep_the_selection() {
    let ctx = context();
    let mut s = two_lines();
    let c = curves(&s);
    let old = vec![Item::Curve(c[1])];
    let (a, b) = (screen(&s, (-15.0, -8.0)), screen(&s, (20.0, 10.0)));

    // Rechte Taste in eigenen Bildern, dann die linke los.
    select(&mut s, &old);
    play(&ctx, &mut s, drag_frames(a, b, LEFT, false));
    assert!(box_active(&s));
    play(
        &ctx,
        &mut s,
        vec![
            (false, vec![button(b, RIGHT, true, false)]),
            (false, vec![button(b, RIGHT, false, false)]),
            (false, vec![moved(a)]),
            (false, vec![button(a, LEFT, false, false)]),
            (false, vec![]),
        ],
    );
    assert!(!box_active(&s));
    assert_selected(&s, &old);

    // Rechte Taste gedrückt und losgelassen im selben Bild.
    play(&ctx, &mut s, drag_frames(a, b, LEFT, false));
    assert!(box_active(&s));
    play(
        &ctx,
        &mut s,
        vec![
            (
                false,
                vec![
                    button(b, RIGHT, true, false),
                    button(b, RIGHT, false, false),
                ],
            ),
            (false, vec![button(b, LEFT, false, false)]),
        ],
    );
    assert!(!box_active(&s));
    assert_selected(&s, &old);

    // Linke Taste los im selben Bild wie der Rechtsklick an einer Stelle ohne Bewegung danach.
    play(&ctx, &mut s, drag_frames(a, b, LEFT, false));
    play(
        &ctx,
        &mut s,
        vec![
            (false, vec![button(b, RIGHT, true, false)]),
            (false, vec![button(b, LEFT, false, false)]),
            (false, vec![button(b, RIGHT, false, false)]),
        ],
    );
    assert!(!box_active(&s));
    assert_selected(&s, &old);

    // Esc, dann weiterziehen und loslassen: Es kommt kein neuer Rahmen.
    play(&ctx, &mut s, drag_frames(a, b, LEFT, false));
    assert!(box_active(&s));
    play(
        &ctx,
        &mut s,
        vec![
            (false, vec![esc()]),
            (false, vec![moved(a)]),
            (false, vec![button(a, LEFT, false, false)]),
        ],
    );
    assert!(!box_active(&s));
    assert_selected(&s, &old);

    // Danach wirkt der Rechtsklick wieder wie sonst: Er leert die Auswahl.
    play(
        &ctx,
        &mut s,
        vec![
            (false, vec![moved(a)]),
            (false, vec![button(a, RIGHT, true, false)]),
            (false, vec![button(a, RIGHT, false, false)]),
        ],
    );
    assert_selected(&s, &[]);
}

#[test]
fn shift_drag_on_free_area_adds_and_does_not_pan() {
    let ctx = context();
    let mut s = two_lines();
    let c = curves(&s);
    select(&mut s, &[Item::Curve(c[1])]);
    let target = s.camera.target;
    let (a, b) = (screen(&s, (-15.0, -8.0)), screen(&s, (15.0, 10.0)));
    play(&ctx, &mut s, full_drag(a, b, LEFT, true));
    assert_eq!(s.camera.target, target, "Umschalt-Rahmen schwenkt nicht");
    assert_selected(&s, &curve_items(&c));
}

#[test]
fn pressing_shift_during_a_box_does_not_pan_and_the_start_decides() {
    let ctx = context();
    let mut s = two_lines();
    let c = curves(&s);
    select(&mut s, &[Item::Curve(c[1])]);
    let target = s.camera.target;
    let (a, b) = (screen(&s, (-15.0, -8.0)), screen(&s, (15.0, 10.0)));
    play(&ctx, &mut s, drag_frames(a, b, LEFT, false));
    let end = b + egui::vec2(20.0, 0.0);
    play(
        &ctx,
        &mut s,
        vec![
            (true, vec![moved(b + egui::vec2(10.0, 0.0))]),
            (true, vec![moved(end)]),
            (true, vec![button(end, LEFT, false, true)]),
        ],
    );
    assert_eq!(s.camera.target, target);
    assert_selected(&s, &curve_items(&c[..1]));
}

#[test]
fn panning_stays_on_its_buttons() {
    let ctx = context();
    let mut s = two_lines();
    let c = curves(&s);
    select(&mut s, &[Item::Curve(c[1])]);
    let home = s.camera;
    let (free, to) = (screen(&s, (-15.0, -8.0)), screen(&s, (15.0, 10.0)));

    // Mitte schwenkt, auch auf freier Fläche.
    let target = s.camera.target;
    play(&ctx, &mut s, full_drag(free, to, MIDDLE, false));
    assert_ne!(s.camera.target, target, "Mitte schwenkt");
    assert!(!box_active(&s));

    // Umschalt+Mitte gehört zum Drehen, das in der Skizze gesperrt ist: wie vorher kein Rahmen,
    // keine Bewegung und keine Änderung der Auswahl.
    s.camera = home;
    let (yaw, pitch, target) = (s.camera.yaw, s.camera.pitch, s.camera.target);
    play(&ctx, &mut s, full_drag(free, to, MIDDLE, true));
    assert_eq!((s.camera.yaw, s.camera.pitch), (yaw, pitch));
    assert_eq!(s.camera.target, target);
    assert!(!box_active(&s));
    assert_selected(&s, &curve_items(&c[1..]));

    // Umschalt+links auf einer Linie bleibt Schwenken, auch im Auswahlwerkzeug.
    let target = s.camera.target;
    let (on_line, to) = (screen(&s, (0.0, 0.0)), screen(&s, (15.0, 10.0)));
    play(&ctx, &mut s, full_drag(on_line, to, LEFT, true));
    assert_ne!(s.camera.target, target, "Umschalt+links auf der Linie");
    assert_selected(&s, &curve_items(&c[1..]));

    // In einem anderen Werkzeug schwenkt Umschalt+links auch auf freier Fläche.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Rectangle);
    }
    s.camera = home;
    let target = s.camera.target;
    play(&ctx, &mut s, full_drag(free, to, LEFT, true));
    assert_ne!(s.camera.target, target, "Rechteck: Umschalt+links");
    assert!(!box_active(&s));
}

#[test]
fn the_frame_is_solid_for_window_and_dashed_for_crossing() {
    let mut s = two_lines();
    let col = theme::sketch_colors(s.dark);
    let set = |s: &mut AppState, a: (f64, f64), b: (f64, f64)| {
        let (se, _) = s.session_sketch_mut().unwrap();
        se.begin_box(DVec2::new(a.0, a.1), false);
        se.box_to(DVec2::new(b.0, b.1));
    };
    set(&mut s, (100.0, 100.0), (300.0, 200.0));
    let window = sketch_view::box_select_shapes(&s);
    set(&mut s, (300.0, 100.0), (100.0, 200.0));
    let crossing = sketch_view::box_select_shapes(&s);
    assert_eq!(window.len(), 2, "Fläche und durchgezogener Rand");
    assert!(crossing.len() > 2, "gestrichelt zerfällt der Rand");
    for shapes in [&window, &crossing] {
        assert!(matches!(&shapes[0], egui::Shape::Rect(r) if r.fill == col.box_fill));
        let color = match &shapes[1] {
            egui::Shape::Path(p) => match p.stroke.color {
                egui::epaint::ColorMode::Solid(c) => c,
                _ => panic!("einfarbig erwartet"),
            },
            egui::Shape::LineSegment { stroke, .. } => stroke.color,
            other => panic!("unerwartet: {other:?}"),
        };
        assert_eq!(color, col.selected);
    }
    s.sketch_session.as_mut().unwrap().box_select = None;
    assert!(sketch_view::box_select_shapes(&s).is_empty());
}

// ---- Doppelklick ----------------------------------------------------------------------------

#[test]
fn double_click_selects_the_chain() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        sk.add_rectangle(DVec2::new(-20.0, -10.0), DVec2::new(20.0, 10.0))
            .unwrap();
        // zwei Linien über `Coincident`
        let l1 = line(sk, (40.0, 0.0), (50.0, 0.0));
        let l2 = line(sk, (50.0, 0.0), (60.0, 5.0));
        let (_, e) = ends(sk, l1);
        let (st, _) = ends(sk, l2);
        sk.add_constraint(ConstraintKind::Coincident(e, st))
            .unwrap();
        // alleinstehende Linie
        line(sk, (30.0, 15.0), (50.0, 15.0));
    });
    let c = curves(&s);
    let steps = s.sketch_history().unwrap().undo_len();

    let side = screen(&s, (0.0, -10.0));
    play(&ctx, &mut s, double_click_frames(side));
    assert_selected(&s, &curve_items(&c[..4]));

    let coincident = screen(&s, (45.0, 0.0));
    play(&ctx, &mut s, double_click_frames(coincident));
    assert_selected(&s, &curve_items(&c[4..6]));

    let alone = screen(&s, (40.0, 15.0));
    play(&ctx, &mut s, double_click_frames(alone));
    assert_selected(&s, &curve_items(&c[6..]));

    assert_eq!(s.sketch_history().unwrap().undo_len(), steps);
}

#[test]
fn a_single_click_still_selects_one_curve() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        sk.add_rectangle(DVec2::new(-20.0, -10.0), DVec2::new(20.0, 10.0))
            .unwrap();
    });
    let side = screen(&s, (0.0, -10.0));
    play(&ctx, &mut s, click_frames(side));
    assert_eq!(selection(&s).len(), 1);
}

#[test]
fn double_click_on_a_t_joint_takes_only_that_line_and_a_corner_stays_a_point() {
    let ctx = context();
    let mut foot = None;
    let mut s = sketch_with(|sk| {
        let bar = line(sk, (-20.0, 0.0), (20.0, 0.0));
        let stem = line(sk, (0.0, 0.0), (0.0, 20.0));
        let (f, _) = ends(sk, stem);
        foot = Some(f);
        sk.add_constraint(ConstraintKind::PointOnCurve(f, bar))
            .unwrap();
    });
    let c = curves(&s);
    let on_bar = screen(&s, (10.0, 0.0));
    play(&ctx, &mut s, double_click_frames(on_bar));
    assert_selected(&s, &curve_items(&c[..1]));
    // Punkt vor Kurve: Doppelklick auf den Punkt wählt wie der Klick nur den Punkt.
    let at_foot = screen(&s, (0.0, 0.0));
    play(&ctx, &mut s, double_click_frames(at_foot));
    assert_selected(&s, &[Item::Point(foot.unwrap())]);
}

#[test]
fn double_click_in_the_line_tool_still_finishes() {
    let ctx = context();
    let mut s = sketch_with(|_| {});
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    let (a, b) = (screen(&s, (0.0, 0.0)), screen(&s, (20.0, 0.0)));
    play(&ctx, &mut s, click_frames(a));
    assert!(!s.sketch_session.as_ref().unwrap().pending.is_empty());
    play(&ctx, &mut s, double_click_frames(b));
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.tool, Tool::Line);
    assert!(se.pending.is_empty(), "Doppelklick beendet den Linienzug");
    assert!(se.selection.is_empty());
}
