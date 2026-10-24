//! Verlaufsleiste am unteren Rand (Oberfläche): ein Symbol je Schritt des gewählten Körpers,
//! Marker ziehen oder mit den Pfeiltasten bewegen, Abspielen, Hinweise und die Lage zwischen den
//! anderen Feldern. Ohne Fenster: Rohereignisse je Bild gegen einen egui-Kontext.

mod common;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::history_bar::{self, BarRects};
use planora_app::state::*;
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, theme};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1400.0, 900.0);

/// Fenster mit eigener Zeit (Bildabstand `step` Sekunden).
struct Bar {
    ctx: egui::Context,
    size: Vec2,
    time: f64,
    step: f64,
}

impl Bar {
    fn new(size: Vec2) -> Bar {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        Bar {
            ctx,
            size,
            time: 0.0,
            step: 1.0 / 60.0,
        }
    }

    /// Ein Bild nur mit der Leiste; liefert alle gezeichneten Texte.
    fn frame(&mut self, s: &mut AppState, events: Vec<Event>) -> Vec<String> {
        self.time += self.step;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let out = self
            .ctx
            .run_ui(raw, |ui| history_bar::show(ui.ctx(), &mut *s));
        let texts = planora_app::testing::collect_texts(&out)
            .lines()
            .map(str::to_owned)
            .collect();
        out.drop_without_applying_deltas();
        texts
    }

    fn settle(&mut self, s: &mut AppState) -> Vec<String> {
        let mut t = Vec::new();
        for _ in 0..3 {
            t = self.frame(s, vec![]);
        }
        t
    }

    fn rects(&self) -> BarRects {
        history_bar::bar_rects(&self.ctx)
    }

    fn click(&mut self, s: &mut AppState, p: Pos2) {
        for events in [
            vec![Event::PointerMoved(p)],
            vec![button(p, true)],
            vec![button(p, false)],
            vec![],
        ] {
            self.frame(s, events);
        }
    }

    fn key(&mut self, s: &mut AppState, key: Key) {
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(s, vec![ev(true), ev(false)]);
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

/// Index des sichtbaren Körpers, den `feature` erzeugt.
fn body_of(s: &AppState, feature: FeatureId) -> usize {
    s.bodies
        .iter()
        .position(|b| b.source.feature == feature)
        .expect("Körper sichtbar")
}

/// Skizze, Extrusion, Verschieben: eine Kette aus drei Schritten. Liefert `[s, e, m]`.
fn chain_model() -> (AppState, [FeatureId; 3]) {
    let mut s = AppState::new();
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
    s.move_body(body_of(&s, e), DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    (s, [sk, e, m])
}

fn label<'a>(r: &'a BarRects, name: &str) -> &'a (String, Rect, bool) {
    r.buttons
        .iter()
        .find(|(n, ..)| n == name)
        .unwrap_or_else(|| panic!("Knopf „{name}“ fehlt: {:?}", r.buttons))
}

#[test]
fn bar_shows_one_icon_per_step_of_selected_body() {
    let (mut s, [sk, e, m]) = chain_model();
    let other = s.create_box(DVec3::splat(4.0)).unwrap();
    s.select_body(Some(m));
    let mut bar = Bar::new(SIZE);
    let texts = bar.settle(&mut s);
    let r = bar.rects();
    let ids: Vec<FeatureId> = r.steps.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        ids,
        vec![sk, e, m],
        "nur die Schritte des gewählten Körpers"
    );
    for pair in r.steps.windows(2) {
        assert!(pair[0].1.right() < pair[1].1.left(), "{:?}", r.steps);
    }
    for (_, rect) in &r.steps {
        assert!((rect.width() - rect.height()).abs() < 0.5, "quadratisch");
        assert!(r.bar.contains_rect(*rect), "{rect:?} in {:?}", r.bar);
    }
    assert!(r.dimmed.is_empty());
    assert!(texts.iter().any(|t| t == "Schritt 3 von 3"), "{texts:?}");
    // Der Marker steht hinter dem letzten Schritt.
    assert!(r.marker.center().x > r.steps[2].1.right());
    // Ein anderer Körper: seine Leiste.
    s.select_body(Some(other));
    bar.settle(&mut s);
    let r = bar.rects();
    assert_eq!(r.steps.len(), 1);
    assert_eq!(r.steps[0].0, other);
}

#[test]
fn clicking_a_step_selects_it() {
    let (mut s, [_, e, _]) = chain_model();
    let mut bar = Bar::new(SIZE);
    bar.settle(&mut s);
    let r = bar.rects();
    bar.click(&mut s, r.steps[1].1.center());
    assert_eq!(s.selected_feature, Some(e));
    assert!(
        s.document.rollbacks().is_empty(),
        "Klick bewegt den Marker nicht"
    );
}

#[test]
fn dragging_the_marker_to_the_second_step_rolls_back() {
    let (mut s, [_, e, m]) = chain_model();
    let mut bar = Bar::new(SIZE);
    bar.settle(&mut s);
    let r = bar.rects();
    let from = r.marker.center();
    let to = pos2(r.steps[1].1.left() + 2.0, from.y);
    let mid = pos2((from.x + to.x) / 2.0, from.y);
    for events in [
        vec![Event::PointerMoved(from)],
        vec![button(from, true)],
        vec![Event::PointerMoved(mid)],
        vec![Event::PointerMoved(to)],
        vec![button(to, false)],
        vec![],
    ] {
        bar.frame(&mut s, events);
    }
    bar.settle(&mut s);
    assert_eq!(s.document.rollbacks(), &[e]);
    assert_eq!(history_bar::marker_position(&s, m), 1);
    let r = bar.rects();
    assert!(
        (r.marker.center().x - r.steps[1].1.left()).abs() < 6.0,
        "Marker vor dem zweiten Schritt: {:?} {:?}",
        r.marker,
        r.steps[1].1
    );
}

#[test]
fn arrow_keys_move_marker_when_bar_has_focus() {
    let (mut s, [_, _, m]) = chain_model();
    let mut bar = Bar::new(SIZE);
    bar.settle(&mut s);
    // Ohne Fokus bleibt alles.
    bar.key(&mut s, Key::ArrowLeft);
    assert_eq!(history_bar::marker_position(&s, m), 3);
    let marker = bar.rects().marker.center();
    bar.click(&mut s, marker);
    bar.key(&mut s, Key::ArrowLeft);
    assert_eq!(history_bar::marker_position(&s, m), 2);
    bar.key(&mut s, Key::ArrowLeft);
    assert_eq!(history_bar::marker_position(&s, m), 1);
    bar.key(&mut s, Key::ArrowRight);
    assert_eq!(history_bar::marker_position(&s, m), 2);
    bar.key(&mut s, Key::ArrowRight);
    assert_eq!(history_bar::marker_position(&s, m), 3);
    assert!(s.document.rollbacks().is_empty());
}

#[test]
fn playback_buttons_move_the_marker() {
    let (mut s, [_, _, m]) = chain_model();
    let mut bar = Bar::new(SIZE);
    bar.settle(&mut s);
    let r = bar.rects();
    assert!(!label(&r, "Ein Schritt vor").2, "am Ende gesperrt");
    bar.click(&mut s, label(&r, "Zum Anfang").1.center());
    assert_eq!(history_bar::marker_position(&s, m), 0);
    let r = bar.rects();
    assert!(!label(&r, "Ein Schritt zurück").2, "am Anfang gesperrt");
    bar.click(&mut s, label(&r, "Ein Schritt vor").1.center());
    assert_eq!(history_bar::marker_position(&s, m), 1);
    bar.click(&mut s, label(&r, "Zum Ende").1.center());
    assert_eq!(history_bar::marker_position(&s, m), 3);
    bar.click(&mut s, label(&r, "Ein Schritt zurück").1.center());
    assert_eq!(history_bar::marker_position(&s, m), 2);
}

#[test]
fn play_moves_one_step_every_600_ms_and_stops_at_the_end() {
    let (mut s, [_, _, m]) = chain_model();
    history_bar::to_start(&mut s, m).unwrap();
    let mut bar = Bar::new(SIZE);
    bar.settle(&mut s);
    let play = label(&bar.rects(), "Abspielen").1.center();
    bar.click(&mut s, play);
    assert!(
        bar.rects().buttons.iter().any(|(n, ..)| n == "Anhalten"),
        "läuft"
    );
    let start = history_bar::marker_position(&s, m);
    assert!(start <= 1, "{start}");
    // 0,1 s je Bild: nach 0,6 s ein Schritt weiter.
    bar.step = 0.1;
    let mut seen = vec![start];
    for _ in 0..40 {
        bar.frame(&mut s, vec![]);
        let p = history_bar::marker_position(&s, m);
        if seen.last() != Some(&p) {
            seen.push(p);
        }
    }
    assert_eq!(*seen.last().unwrap(), 3, "{seen:?}");
    for pair in seen.windows(2) {
        assert_eq!(
            pair[1],
            pair[0] + 1,
            "ein Schritt nach dem anderen: {seen:?}"
        );
    }
    let r = bar.rects();
    assert!(
        r.buttons.iter().any(|(n, ..)| n == "Abspielen"),
        "angehalten: {:?}",
        r.buttons
    );
    // Angehalten: kein Neuzeichnen mehr angefordert.
    bar.time += 1.0;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
        time: Some(bar.time),
        ..Default::default()
    };
    let out = bar
        .ctx
        .run_ui(raw, |ui| history_bar::show(ui.ctx(), &mut s));
    let delay = out
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
    out.drop_without_applying_deltas();
    assert!(delay > std::time::Duration::from_secs(10), "{delay:?}");
}

#[test]
fn no_selected_body_shows_hint_and_disabled_buttons() {
    let mut s = AppState::new();
    let mut bar = Bar::new(SIZE);
    let texts = bar.settle(&mut s);
    assert!(texts.iter().any(|t| t == "Körper wählen"), "{texts:?}");
    let r = bar.rects();
    assert!(r.steps.is_empty());
    assert_eq!(r.buttons.len(), 5, "{:?}", r.buttons);
    assert!(r.buttons.iter().all(|(.., on)| !on), "{:?}", r.buttons);
    assert!(r.bar.width() > 100.0);
}

#[test]
fn inactive_steps_are_dimmed() {
    let (mut s, [_, e, m]) = chain_model();
    history_bar::move_marker(&mut s, m, 1).unwrap();
    let mut bar = Bar::new(SIZE);
    let texts = bar.settle(&mut s);
    let r = bar.rects();
    assert_eq!(r.dimmed, vec![e, m]);
    assert!(texts.iter().any(|t| t == "Schritt 1 von 3"), "{texts:?}");
}

#[test]
fn global_marker_offers_activate_all() {
    let (mut s, [_, e, m]) = chain_model();
    // Der globale Marker steht hinter der Extrusion: das Verschieben ist zurückgesetzt.
    s.document.set_marker(2).unwrap();
    s.refresh();
    let mut bar = Bar::new(SIZE);
    let texts = bar.settle(&mut s);
    assert!(
        texts.iter().any(|t| t == "Gesamter Verlauf zurückgesetzt"),
        "{texts:?}"
    );
    let r = bar.rects();
    assert_eq!(r.dimmed, vec![m]);
    // Vorwärts geht es nicht über den globalen Marker hinaus.
    assert!(!label(&r, "Ein Schritt vor").2);
    assert!(!label(&r, "Zum Ende").2);
    // Ein Schritt zurück und wieder vor: keine Stelle bleibt (der Körper ist nicht gesperrt).
    bar.click(&mut s, label(&r, "Ein Schritt zurück").1.center());
    assert_eq!(s.document.rollbacks(), &[e]);
    bar.click(&mut s, label(&r, "Ein Schritt vor").1.center());
    assert!(
        s.document.rollbacks().is_empty(),
        "{:?}",
        s.document.rollbacks()
    );
    assert!(!history_bar::is_locked(&s, m));
    let all = label(&bar.rects(), "Alles aktivieren").1;
    bar.click(&mut s, all.center());
    assert_eq!(s.document.marker(), s.document.features().len());
    let texts = bar.settle(&mut s);
    assert!(!texts.iter().any(|t| t == "Gesamter Verlauf zurückgesetzt"));
    assert!(bar.rects().dimmed.is_empty());
}

#[test]
fn long_chain_scrolls_and_marker_stays_reachable() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(4.0)).unwrap();
    // Jedes Verschieben ein eigener Schritt (`move_body` erhöhte das letzte).
    for _ in 0..59 {
        let body = s.bodies[0].source;
        s.add_feature(
            None,
            planora_feature::FeatureKind::Move {
                body,
                delta: DVec3::X,
            },
        )
        .unwrap();
    }
    let tip = s.history_body().unwrap();
    assert_eq!(history_bar::steps(&s, tip).len(), 60);
    let mut bar = Bar::new(vec2(1100.0, 700.0));
    bar.settle(&mut s);
    let r = bar.rects();
    assert_eq!(r.steps.len(), 60);
    let content: f32 = r.steps.iter().map(|(_, x)| x.width()).sum();
    assert!(content > r.strip.width(), "Leiste schmaler als die Kette");
    assert!(r.bar.contains_rect(r.strip));
    // Am Ende: der Marker ist sichtbar.
    assert!(
        r.strip.contains(r.marker.center()),
        "{:?} in {:?}",
        r.marker,
        r.strip
    );
    // Zum Anfang: die Leiste rollt mit.
    bar.click(&mut s, label(&r, "Zum Anfang").1.center());
    bar.settle(&mut s);
    assert_eq!(history_bar::marker_position(&s, tip), 0);
    assert_eq!(s.document.rollbacks(), &[b]);
    let r = bar.rects();
    assert!(
        r.strip.contains(r.marker.center()),
        "{:?} in {:?}",
        r.marker,
        r.strip
    );
    assert!(
        r.strip.contains(r.steps[0].1.center()),
        "erster Schritt sichtbar"
    );
}

// ---- In der App -------------------------------------------------------------------------------

fn library_ws() -> Workspace {
    let root = common::temp_root("planora-history-bar");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn overlaps(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 0.5 && i.height() > 0.5
}

#[test]
fn bar_does_not_overlap_toolbar() {
    for size in [vec2(1400.0, 900.0), vec2(1100.0, 700.0)] {
        let mut app = PlanoraApp::for_tests(library_ws());
        let state = &mut app.ws_mut().active_tab_mut().state;
        state.create_box(DVec3::splat(100.0)).unwrap();
        let c = egui::Context::default();
        c.set_fonts(fonts::definitions());
        theme::apply(&c, true);
        let mut free = Rect::NOTHING;
        for i in 0..4 {
            let out = c.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    time: Some(f64::from(i) / 60.0),
                    ..Default::default()
                },
                |ui| free = shell_panels::show_fields(ui.ctx(), &mut app),
            );
            out.drop_without_applying_deltas();
        }
        let bar = history_bar::bar_rects(&c).bar;
        assert!(bar.width() > 100.0, "{size:?}: Leiste fehlt {bar:?}");
        let fields = shell_panels::field_rects(&c);
        let names: Vec<&str> = fields.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"Verlauf"), "{names:?}");
        for (name, rect) in &fields {
            if *name != "Verlauf" {
                assert!(
                    !overlaps(bar, *rect),
                    "{size:?}: Leiste {bar:?} überlappt {name} {rect:?}"
                );
            }
        }
        let bottom = fields.iter().find(|(n, _)| *n == "unten").unwrap().1;
        assert!(bar.bottom() < bottom.top(), "über dem Befehlsfeld");
        assert!(
            !overlaps(bar, free),
            "{size:?}: über der freien Fläche {free:?}"
        );
        assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(bar));
    }
}

#[test]
fn bar_is_locked_while_a_prompt_is_open() {
    let (mut s, [_, _, m]) = chain_model();
    let mut bar = Bar::new(SIZE);
    let rect = history_bar::default_rect(Rect::from_min_size(Pos2::ZERO, SIZE));
    for _ in 0..3 {
        bar.time += bar.step;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(bar.time),
            ..Default::default()
        };
        let out = bar.ctx.run_ui(raw, |ui| {
            history_bar::show_with(
                ui.ctx(),
                &mut s,
                rect,
                Some("Zuerst die Rückfrage beantworten."),
            )
        });
        out.drop_without_applying_deltas();
    }
    let r = bar.rects();
    assert!(r.buttons.iter().all(|(.., on)| !on), "{:?}", r.buttons);
    assert_eq!(history_bar::marker_position(&s, m), 3);
}
