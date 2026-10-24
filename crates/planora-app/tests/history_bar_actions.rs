//! Verlaufsleiste: Doppelklick auf einen Schritt öffnet seine Einstellungen (auch bei
//! zurückgesetzten Schritten), und neue Aktionen an einem zurückgesetzten Körper sind gesperrt.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::{self, CreateKind, PlanoraApp};
use planora_app::browser;
use planora_app::history_bar::{self, LOCK_HINT};
use planora_app::shell_panels;
use planora_app::state::*;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{BlendArt, FeatureId, FeatureKind, FeatureStatus, SketchPlane};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{Edge, Face, Id};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_render::{FaceRef, Pick};

const SIZE: Vec2 = vec2(1400.0, 900.0);

/// Index des sichtbaren Körpers, den `feature` erzeugt.
fn body_of(s: &AppState, feature: FeatureId) -> usize {
    s.bodies
        .iter()
        .position(|b| b.source.feature == feature)
        .expect("Körper sichtbar")
}

/// Skizze, Extrusion (Höhe 2), Verschieben im Tab eines Arbeitsbereichs ohne Bibliothek.
/// Liefert `[s, e, m]`.
fn chain_ws() -> (Workspace, [FeatureId; 3]) {
    let mut ws = Workspace::new(None, Config::default());
    let s = &mut ws.active_tab_mut().state;
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(5.0, 10.0))
        .unwrap();
    s.close_sketch();
    let e = s.extrude_sketch(sk, 0, 2.0).unwrap();
    s.move_body(body_of(s, e), DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    (ws, [sk, e, m])
}

/// Höhe (z-Ausdehnung) des sichtbaren Körpers von `feature`.
fn height(s: &AppState, feature: FeatureId) -> f64 {
    let b = &s.bodies[body_of(s, feature)];
    b.bounds.1.z - b.bounds.0.z
}

/// Quader und Verschieben; der Marker steht vor dem Verschieben (Körper zurückgesetzt, sichtbar
/// ist der Quader). Liefert `[b, m]`.
fn locked_box() -> (AppState, [FeatureId; 2]) {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(10.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    history_bar::move_marker(&mut s, m, 1).unwrap();
    assert!(history_bar::is_locked(&s, m));
    assert_eq!(s.bodies[body_of(&s, b)].source.feature, b);
    (s, [b, m])
}

fn any_edge(s: &AppState, i: usize) -> Id<Edge> {
    s.bodies[i].solid.edges.iter().next().unwrap().0
}

fn top_face(s: &AppState, i: usize) -> Id<Face> {
    let solid = &s.bodies[i].solid;
    let top_z = s.bodies[i].bounds.1.z;
    solid
        .faces
        .iter()
        .find(|(id, _)| {
            planora_sketch::plane_of_face(solid, *id)
                .is_some_and(|f| f.z.z > 0.5 && (f.origin.z - top_z).abs() < 1e-9)
        })
        .map(|(id, _)| id)
        .expect("Deckfläche")
}

// ---- Doppelklick -------------------------------------------------------------------------------

#[test]
fn double_click_on_extrude_opens_dialog_and_new_height_recomputes_dependents() {
    let (mut ws, [_, e, m]) = chain_ws();
    assert!((height(&ws.active_tab().state, m) - 2.0).abs() < 1e-9);
    history_bar::open_step(&mut ws, e);
    let tab = ws.active_tab_mut();
    assert_eq!(tab.state.selected_feature, Some(e));
    let mut dialog = tab.feature_dialog.take().expect("Dialog der Extrusion");
    assert_eq!(dialog.feature(), e);
    assert!(dialog.set_text("Höhe", "5"));
    dialog.apply(&mut tab.state).unwrap();
    let s = &tab.state;
    // Das spätere Verschieben rechnet mit der neuen Höhe.
    assert!((height(s, m) - 5.0).abs() < 1e-9, "{}", height(s, m));
    for id in [e, m] {
        assert_eq!(s.document.feature(id).unwrap().status, FeatureStatus::Ok);
    }
}

#[test]
fn double_click_on_a_rolled_back_step_edits_it_without_moving_the_marker() {
    let (mut ws, [_, e, m]) = chain_ws();
    history_bar::move_marker(&mut ws.active_tab_mut().state, m, 1).unwrap();
    assert_eq!(ws.active_tab().state.document.rollbacks(), &[e]);
    history_bar::open_step(&mut ws, e);
    let tab = ws.active_tab_mut();
    let mut dialog = tab
        .feature_dialog
        .take()
        .expect("Dialog auch zurückgesetzt");
    assert!(dialog.set_text("Höhe", "7"), "Feld Höhe gibt es");
    dialog.apply(&mut tab.state).unwrap();
    let s = &mut tab.state;
    assert_eq!(s.document.rollbacks(), &[e], "der Marker bleibt");
    assert_eq!(history_bar::marker_position(s, m), 1);
    assert!(s.bodies.is_empty(), "weiter zurückgesetzt");
    history_bar::to_end(s, m).unwrap();
    assert!((height(s, m) - 7.0).abs() < 1e-9, "{}", height(s, m));
}

#[test]
fn double_click_on_sketch_opens_the_sketch() {
    let (mut ws, [sk, ..]) = chain_ws();
    history_bar::open_step(&mut ws, sk);
    let tab = ws.active_tab();
    assert_eq!(tab.state.open_sketch_id(), Some(sk));
    assert!(tab.feature_dialog.is_none());
}

/// Fenster nur mit der Leiste; Rohereignisse je Bild.
struct Bar {
    ctx: egui::Context,
    time: f64,
}

impl Bar {
    fn new() -> Bar {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        Bar { ctx, time: 0.0 }
    }

    fn frame(&mut self, s: &mut AppState, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let out = self
            .ctx
            .run_ui(raw, |ui| history_bar::show(ui.ctx(), &mut *s));
        out.drop_without_applying_deltas();
    }

    fn settle(&mut self, s: &mut AppState) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
    }

    /// `clicks` Klicks bei `p`; vorher eine Sekunde Pause, damit egui frühere Klicks nicht
    /// mitzählt.
    fn press(&mut self, s: &mut AppState, p: Pos2, clicks: usize) {
        self.time += 1.0;
        self.frame(s, vec![Event::PointerMoved(p)]);
        for _ in 0..clicks {
            self.frame(s, vec![button(p, true)]);
            self.frame(s, vec![button(p, false)]);
        }
        self.frame(s, vec![]);
    }
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn double_click_in_the_bar_requests_the_dialog_or_opens_the_sketch() {
    let (mut ws, [sk, e, _]) = chain_ws();
    let s = &mut ws.active_tab_mut().state;
    let mut bar = Bar::new();
    bar.settle(s);
    let steps = history_bar::bar_rects(&bar.ctx).steps;
    bar.press(s, steps[1].1.center(), 2);
    assert_eq!(s.edit_request, Some(e), "Dialog angefordert");
    assert_eq!(s.selected_feature, Some(e));
    s.edit_request = None;
    bar.press(s, steps[0].1.center(), 2);
    assert_eq!(s.open_sketch_id(), Some(sk));
}

#[test]
fn clicking_at_a_steps_left_edge_selects_the_step_not_the_marker() {
    let (mut ws, [_, e, m]) = chain_ws();
    let s = &mut ws.active_tab_mut().state;
    // Der Marker steht in der Lücke vor der Extrusion.
    history_bar::move_marker(s, m, 1).unwrap();
    let mut bar = Bar::new();
    bar.settle(s);
    let r = history_bar::bar_rects(&bar.ctx);
    let step = r.steps[1].1;
    assert!(r.marker.center().x < step.left());
    bar.press(s, pos2(step.left() + 1.0, step.center().y), 1);
    assert_eq!(s.selected_feature, Some(e));
    assert_eq!(history_bar::marker_position(s, m), 1);
}

#[test]
fn playback_does_not_resume_after_the_bar_was_hidden() {
    let (mut ws, [_, _, m]) = chain_ws();
    let s = &mut ws.active_tab_mut().state;
    // Eine freie Skizze zum Wechseln in den Skizzen-Modus.
    let free = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    s.select_body(Some(m));
    history_bar::to_start(s, m).unwrap();
    let mut app = PlanoraApp::for_tests(ws);
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    let mut time = 0.0;
    let mut frame = |app: &mut PlanoraApp, events: Vec<Event>, dt: f64| {
        time += dt;
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                shell_panels::show_fields(ui.ctx(), app);
            },
        );
        out.drop_without_applying_deltas();
    };
    for _ in 0..3 {
        frame(&mut app, vec![], 1.0 / 60.0);
    }
    let play = history_bar::bar_rects(&ctx)
        .buttons
        .iter()
        .find(|(n, ..)| n == "Abspielen")
        .expect("Knopf Abspielen")
        .1
        .center();
    for events in [
        vec![Event::PointerMoved(play)],
        vec![button(play, true)],
        vec![button(play, false)],
    ] {
        frame(&mut app, events, 1.0 / 60.0);
    }
    let state = |app: &mut PlanoraApp| app.ws_mut().active_tab().state.document.rollbacks().len();
    assert_eq!(state(&mut app), 1, "läuft, noch am Anfang");
    // Skizzen-Modus: die Leiste fehlt; danach zurück ins Modell.
    app.ws_mut()
        .active_tab_mut()
        .state
        .open_sketch(free)
        .unwrap();
    frame(&mut app, vec![], 1.0 / 60.0);
    app.ws_mut().active_tab_mut().state.close_sketch();
    let before = history_bar::marker_position(&app.ws_mut().active_tab().state, m);
    for _ in 0..10 {
        frame(&mut app, vec![], 0.2);
    }
    let after = history_bar::marker_position(&app.ws_mut().active_tab().state, m);
    assert_eq!(after, before, "Abspielen geht nicht von selbst weiter");
}

// ---- Sperre ------------------------------------------------------------------------------------

#[test]
fn new_feature_on_rolled_back_body_is_refused_with_hint() {
    let (mut s, [b, m]) = locked_box();
    let count = s.document.features().len();
    let i = body_of(&s, b);
    let other = s.create_box(DVec3::splat(4.0)).unwrap();
    let count = count + 1;
    let j = body_of(&s, other);

    let refused = |s: &mut AppState, r: Result<(), String>, what: &str| {
        assert_eq!(r, Err(LOCK_HINT.to_string()), "{what}");
        assert_eq!(s.message.as_deref(), Some(LOCK_HINT), "{what}");
        s.message = None;
    };
    let r = s.move_body(i, DVec3::Y);
    refused(&mut s, r, "Verschieben");
    let edge = any_edge(&s, i);
    let r = s
        .add_blend(i, &[edge], BlendArt::Rundung { radius: 1.0 }, false)
        .map(|_| ());
    refused(&mut s, r, "Verrunden");
    let r = s.add_boolean(i, j, BoolOp::Union).map(|_| ());
    refused(&mut s, r, "Vereinigen (Ziel)");
    let r = s.add_boolean(j, i, BoolOp::Difference).map(|_| ());
    refused(&mut s, r, "Abziehen (Werkzeug)");
    let face = top_face(&s, i);
    let r = s.new_sketch_on_face(i, face).map(|_| ());
    refused(&mut s, r, "Skizze auf Fläche");
    let r = s
        .add_feature(
            None,
            FeatureKind::Move {
                body: s.bodies[i].source,
                delta: DVec3::Z,
            },
        )
        .map(|_| ());
    refused(&mut s, r, "Feature direkt");
    assert_eq!(s.document.features().len(), count, "nichts angelegt");
    assert!(history_bar::is_locked(&s, m));

    // Menüs und Knöpfe: gesperrt mit Hinweis, wenn der gewählte Körper zurückgesetzt ist.
    s.selected_body = Some(i);
    assert_eq!(s.boolean_blocked(), Some(LOCK_HINT));
    assert_eq!(s.blend_blocked(), Some(LOCK_HINT));
    s.set_selection(Some(Pick::Face(FaceRef { body: i, face })));
    assert_eq!(s.sketch_on_face_blocked(), Some(LOCK_HINT));
    assert_eq!(s.body_lock(i), Some(LOCK_HINT));
    assert_eq!(s.body_lock(j), None);
    // Neue freistehende Körper gehen weiter.
    s.create_cylinder(2.0, 3.0).unwrap();
    // Am Ende des Verlaufs ist die Sperre weg.
    history_bar::to_end(&mut s, m).unwrap();
    let i = body_of(&s, m);
    s.move_body(i, DVec3::Y).unwrap();
}

#[test]
fn extruding_a_sketch_of_a_rolled_back_body_is_refused() {
    let (mut ws, [sk, e, m]) = chain_ws();
    history_bar::move_marker(&mut ws.active_tab_mut().state, m, 2).unwrap();
    let s = &mut ws.active_tab_mut().state;
    assert_eq!(s.document.rollbacks(), &[m]);
    let r = s.extrude_sketch(sk, 0, 3.0).map(|_| ());
    assert_eq!(r, Err(LOCK_HINT.to_string()));
    s.message = None;
    s.selected_feature = Some(sk);
    assert_eq!(
        app::create_blocked(&ws, CreateKind::Extrusion),
        Some(LOCK_HINT)
    );
    assert_eq!(
        app::create_blocked(&ws, CreateKind::Revolution),
        Some(LOCK_HINT)
    );
    assert_eq!(app::create_blocked(&ws, CreateKind::Box), None);
    // Der Körper der Extrusion selbst ist sichtbar und gesperrt.
    let s = &ws.active_tab().state;
    assert_eq!(s.body_lock(body_of(s, e)), Some(LOCK_HINT));
}

#[test]
fn delete_hide_and_rename_still_work_on_rolled_back_body() {
    let (mut s, [b, m]) = locked_box();
    s.rename_feature(b, "Grundplatte").unwrap();
    assert_eq!(s.document.feature(b).unwrap().name, "Grundplatte");
    browser::toggle_suppress(&mut s, b).unwrap();
    assert!(s.document.feature(b).unwrap().suppressed);
    browser::toggle_suppress(&mut s, b).unwrap();
    assert!(!s.document.feature(b).unwrap().suppressed);
    // Löschen des zurückgesetzten Schritts.
    s.delete_feature(m).unwrap();
    assert!(s.document.feature(m).is_none());
    assert!(!history_bar::is_locked(&s, b));
}

#[test]
fn other_bodies_are_not_locked() {
    let (mut s, [b, m]) = locked_box();
    let other = s.create_box(DVec3::splat(4.0)).unwrap();
    let j = body_of(&s, other);
    s.selected_body = Some(j);
    assert_eq!(s.blend_blocked(), None);
    assert_eq!(s.boolean_blocked(), None);
    assert_eq!(s.body_lock(j), None);
    s.move_body(j, DVec3::Y).unwrap();
    let j = s.bodies.iter().position(|x| x.source.feature != b).unwrap();
    let edge = any_edge(&s, j);
    s.add_blend(j, &[edge], BlendArt::Rundung { radius: 0.5 }, false)
        .unwrap();
    assert!(history_bar::is_locked(&s, m), "der andere bleibt gesperrt");
}

#[test]
fn union_is_refused_when_the_rolled_back_body_is_the_newer_of_the_inputs() {
    // Der gesperrte Körper ist hier der jüngere Eingang (`dependencies()` sortiert nach Kennung,
    // der ältere steht vorn): Ziel und Werkzeug werden beide geprüft, nicht nur der erste.
    let mut s = AppState::new();
    let older = s.create_box(DVec3::splat(4.0)).unwrap();
    let newer = s.create_box(DVec3::splat(3.0)).unwrap();
    let j = body_of(&s, newer);
    s.move_body(j, DVec3::X * 10.0).unwrap();
    let m = s.document.features().last().unwrap().id;
    history_bar::move_marker(&mut s, m, 1).unwrap();
    assert!(history_bar::is_locked(&s, m));
    let (io, jn) = (body_of(&s, older), body_of(&s, newer));
    let count = s.document.features().len();
    for (target, tool, what) in [
        (io, jn, "älterer als Ziel"),
        (jn, io, "älterer als Werkzeug"),
    ] {
        let r = s.add_boolean(target, tool, BoolOp::Union).map(|_| ());
        assert_eq!(r, Err(LOCK_HINT.to_string()), "{what}");
        s.message = None;
    }
    assert_eq!(s.document.features().len(), count, "nichts angelegt");
    assert_eq!(s.body_lock(io), None, "der ältere Körper selbst ist frei");
}
