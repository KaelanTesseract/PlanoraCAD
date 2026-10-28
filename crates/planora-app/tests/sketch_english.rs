//! Skizzen auf Englisch (Sprachplan, Task 8): Werkzeugleiste, Skizzenfeld, Fußfeld, Maßfeld,
//! Meldungen und Beschriftungen in der Ansicht, unter `with_lang(&english(), …)` gezeichnet, ohne
//! Fenster. Kein Test ruft `set_language` oder `set_global` (Ruling 4): die Sprache gilt nur für
//! den Thread des Tests.
//!
//! Fehlertexte der Bibliothek `planora-sketch` bleiben bis Task 11 deutsch; sie kommen in diesen
//! Tests nicht vor.

mod common;

use common::looks_like_key;
use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_state::{ConstraintAction, Item};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, sketch_panel, sketch_view, theme};
use planora_feature::SketchPlane;
use planora_i18n::{english, german, with_lang};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::CurveId;

const SIZE: Vec2 = vec2(1920.0, 1080.0);

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
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

/// App mit offener, leerer Skizze auf XY.
fn sketching() -> PlanoraApp {
    let root = common::temp_root("planora-sketch-english");
    let ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let mut app = PlanoraApp::for_tests(ws);
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    app
}

/// Waagerechte Linie (−10,−10)–(10,−10), ausgewählt.
fn with_selected_line(app: &mut PlanoraApp) -> CurveId {
    let (se, sk) = state(app).session_sketch_mut().unwrap();
    let a = sk.add_point(-10.0, -10.0).unwrap();
    let b = sk.add_point(10.0, -10.0).unwrap();
    let line = sk.add_line(a, b).unwrap();
    se.selection = vec![Item::Curve(line)];
    line
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

    fn has(&self, exact: &str) -> bool {
        self.texts.iter().any(|(t, _)| t == exact)
    }

    fn at(&self, exact: &str) -> Rect {
        self.texts
            .iter()
            .find(|(t, _)| t == exact)
            .unwrap_or_else(|| panic!("„{exact}“ fehlt in {:?}", self.all()))
            .1
    }

    /// Wie [`at`](Self::at), aber nur im Rechteck `area`.
    fn at_in(&self, area: Rect, exact: &str) -> Rect {
        self.texts
            .iter()
            .find(|(t, r)| t == exact && area.contains(r.center()))
            .unwrap_or_else(|| panic!("„{exact}“ fehlt in {area:?}: {:?}", self.all()))
            .1
    }

    fn all(&self) -> Vec<&str> {
        self.texts.iter().map(|(t, _)| t.as_str()).collect()
    }

    /// Kein deutscher Buchstabe, keine Schlüssel-Id in den gezeichneten Texten.
    fn assert_english(&self, what: &str) {
        for (text, _) in &self.texts {
            for line in text.lines() {
                assert!(
                    !line.chars().any(|c| "äöüßÄÖÜ„".contains(c)),
                    "{what}: deutscher Text „{line}“ in {:?}",
                    self.all()
                );
                assert!(
                    !looks_like_key(line),
                    "{what}: Schlüssel-Id „{line}“ sichtbar in {:?}",
                    self.all()
                );
            }
        }
    }

    fn assert_has(&self, what: &str, wanted: &[&str]) {
        for w in wanted {
            assert!(self.has(w), "{what}: „{w}“ fehlt in {:?}", self.all());
        }
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

fn moved(p: Pos2) -> Event {
    Event::PointerMoved(p)
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
    frame(c, app, vec![moved(p)]);
    frame(c, app, vec![moved(p), button(p, true)]);
    frame(c, app, vec![button(p, false)]);
    frame(c, app, vec![])
}

fn side(c: &egui::Context) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == "Skizze")
        .expect("Skizzenfeld")
        .1
}

#[test]
fn sketch_speaks_english() {
    with_lang(&english(), || {
        let c = context();
        let mut app = sketching();
        with_selected_line(&mut app);
        let shot = settle(&c, &mut app);
        shot.assert_english("Skizzenfelder");
        shot.assert_has(
            "Skizzenfelder",
            &[
                "Constraints",
                "Coincident",
                "Horizontal",
                "Vertical",
                "Parallel",
                "Perpendicular",
                "Tangent",
                "Equal",
                "Fixed",
                "Midpoint",
                "Symmetric",
                "Point on curve",
                "Distance …",
                "Point–line distance …",
                "Radius …",
                "Diameter …",
                "Angle …",
                "Edit",
                "Mirror …",
                "Pattern …",
                "Delete",
                "Detect\nhorizontal/vertical",
                "Sketch constraints",
                "No constraints yet.",
                "Finish sketch",
            ],
        );
        shot.assert_has("Zustand", &["Fully constrained"]);

        // Werkzeugleiste: Namen (für Bildschirmleser) und Hinweise beim Überfahren.
        let tools = sketch_panel::tools();
        let names: Vec<&str> = tools.iter().map(|(_, l, _)| l.as_str()).collect();
        assert_eq!(
            names,
            [
                "Select",
                "Point",
                "Line",
                "Rectangle",
                "Circle",
                "Arc",
                "Ellipse",
                "Spline",
                "Copy body edge"
            ]
        );
        let buttons = sketch_panel::strip_buttons(&c);
        assert_eq!(buttons.len(), 9);
        for (tool, label, hint) in tools {
            let (_, r) = buttons.iter().find(|(l, _)| *l == label).unwrap();
            frame(&c, &mut app, vec![moved(r.center())]);
            frame(&c, &mut app, vec![moved(r.center())]);
            let tip = frame(&c, &mut app, vec![]);
            assert!(tip.has(&hint), "Hinweis zu {tool:?}: {:?}", tip.all());
            tip.assert_english(&format!("Hinweis {tool:?}"));
        }
    });
}

#[test]
fn sketch_editing_fields_speak_english() {
    with_lang(&english(), || {
        let c = context();
        let mut app = sketching();
        with_selected_line(&mut app);
        settle(&c, &mut app);

        // Maßfeld für ein neues Maß, mit der Zahl in englischer Schreibweise.
        sketch_panel::open_dimension_editor(&c, state(&mut app), ConstraintAction::Distance);
        let shot = settle(&c, &mut app);
        shot.assert_english("Maßfeld");
        shot.assert_has("Maßfeld", &["Distance:", "OK", "Cancel", "20.00"]);

        // Abbrechen, dann „Pattern …“ und „Mirror …“.
        let r = shot.at_in(side(&c), "Cancel");
        let shot = click(&c, &mut app, r.center());
        shot.assert_english("nach Abbrechen");
        let r = shot.at_in(side(&c), "Pattern …");
        let shot = click(&c, &mut app, r.center());
        shot.assert_english("Muster");
        shot.assert_has(
            "Muster",
            &[
                "Offset x",
                "Offset y",
                "Number of copies",
                "Create",
                "Cancel",
            ],
        );

        // Falsche Eingabe im Feld „Offset x“: die Meldung steht englisch im Skizzenfeld.
        let label = shot.at("Offset x");
        let field = Pos2::new(label.right() + 40.0, label.center().y);
        click(&c, &mut app, field);
        frame(
            &c,
            &mut app,
            vec![Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }],
        );
        frame(&c, &mut app, vec![Event::Text("abc".into())]);
        let shot = settle(&c, &mut app);
        let create = shot.at_in(side(&c), "Create");
        let shot = click(&c, &mut app, create.center());
        shot.assert_english("falsche Zahl");
        shot.assert_has("falsche Zahl", &["Please enter a number"]);
        // Die gespeicherte Meldung wechselt mit der Sprache.
        let de = with_lang(&german(), || settle(&c, &mut app));
        assert!(de.has("Bitte eine Zahl eingeben"), "{:?}", de.all());

        // Abbrechen, „Mirror …“ braucht zwei Kurven und eine Achse: gesperrt, aber sichtbar.
        let cancel = shot.at_in(side(&c), "Cancel");
        click(&c, &mut app, cancel.center());
        let _ = side(&c);
    });
}

#[test]
fn constraint_list_and_stored_messages_speak_english() {
    let c = context();
    let mut app = sketching();
    with_selected_line(&mut app);
    // Unter Deutsch entstehen eine Abhängigkeit, ein Maß und zwei Meldungen.
    with_lang(&german(), || {
        let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
        se.apply(sk, ConstraintAction::Horizontal, None).unwrap();
        se.apply(sk, ConstraintAction::Distance, Some(10.0))
            .unwrap();
        let err = se
            .apply(sk, ConstraintAction::Distance, Some(20.0))
            .unwrap_err();
        assert_eq!(
            err,
            "Widerspruch: „Abstand“ widerspricht vorhandenen Abhängigkeiten"
        );
    });
    with_lang(&english(), || {
        let shot = settle(&c, &mut app);
        shot.assert_english("Liste und Meldung");
        shot.assert_has(
            "Liste und Meldung",
            &[
                "Horizontal",
                "Distance 10.00 mm",
                "driven",
                "Contradiction: \"Distance\" conflicts with existing constraints",
            ],
        );
        let se = state(&mut app).sketch_session.as_ref().unwrap();
        assert_eq!(
            se.message_text().as_deref(),
            Some("Contradiction: \"Distance\" conflicts with existing constraints")
        );
    });
    // Eine Meldung ohne Wert und eine mit Zahl, unter Deutsch gespeichert, englisch gezeichnet.
    let mut app = sketching();
    with_lang(&german(), || {
        let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
        assert!(se.apply(sk, ConstraintAction::Horizontal, None).is_err());
        assert_eq!(
            se.message_text().as_deref(),
            Some("Es ist nichts ausgewählt")
        );
    });
    with_lang(&english(), || {
        let shot = settle(&c, &mut app);
        shot.assert_english("Meldung ohne Auswahl");
        shot.assert_has("Meldung ohne Auswahl", &["Nothing is selected"]);
    });
    let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
    se.tool = planora_app::sketch_state::Tool::Spline;
    for i in 0..planora_sketch::model::MAX_SPLINE_POINTS {
        se.click(
            sk,
            planora_math::DVec2::new(i as f64 * 5.0, 3.0),
            0.1,
            false,
        )
        .unwrap();
    }
    with_lang(&german(), || {
        let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
        let e = se
            .click(sk, planora_math::DVec2::new(500.0, 3.0), 0.1, false)
            .unwrap_err();
        assert!(e.starts_with("Ein Spline hat höchstens"), "{e}");
    });
    with_lang(&english(), || {
        let text = state(&mut app)
            .sketch_session
            .as_ref()
            .unwrap()
            .message_text()
            .unwrap();
        assert!(text.starts_with("A spline has at most"), "{text}");
        assert!(text.ends_with("points; please finish it"), "{text}");
    });
}

#[test]
fn status_and_view_labels_speak_english() {
    with_lang(&english(), || {
        let c = context();
        let mut app = sketching();
        with_selected_line(&mut app);
        let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
        // Zustand: Freiheitsgrade, überzählig.
        se.apply(sk, ConstraintAction::Horizontal, None).unwrap();
        let (text, _) = se.status(sk);
        assert!(
            text.ends_with("degrees of freedom") || text == "1 degree of freedom",
            "{text}"
        );
        se.apply(sk, ConstraintAction::Horizontal, None).unwrap();
        let (text, _) = se.status(sk);
        assert!(text.ends_with(", 1 redundant"), "{text}");

        // Beschriftungen in der Ansicht: Wörter statt deutscher Kürzel.
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(1.0, 1.0).unwrap();
        let line = sk.add_line(a, b).unwrap();
        for (kind, want) in [
            (K::Midpoint(a, line), "Mid"),
            (K::Symmetric(a, b, line), "Sym"),
            (K::PointOnCurve(a, line), "on"),
        ] {
            assert_eq!(sketch_view::overlay_text(&c, sk, &kind, false), want);
        }
        with_lang(&german(), || {
            assert_eq!(
                sketch_view::overlay_text(&c, sk, &K::Midpoint(a, line), false),
                "Mitte"
            );
        });
        assert_eq!(sketch_view::kind_label(&K::Tangent(line, line)), "Tangent");
        assert_eq!(
            sketch_view::kind_label(&K::EllipseMajor(line, 1.0)),
            "Major semi-axis"
        );
    });
}
