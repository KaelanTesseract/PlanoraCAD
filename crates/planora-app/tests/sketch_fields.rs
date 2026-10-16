//! Skizzenmodus mit schwebenden Feldern (Graphit, Etappe C, Aufgabe 3): Werkzeugleiste links,
//! Skizzenfeld rechts, Fußfeld unten statt des Fensters „Skizze“. Ohne Fenster: die ganze App
//! (`eframe::App::ui`) Bild für Bild gegen einen egui-Kontext; geprüft werden die gemalten
//! Formen, die Felder (`shell_panels::field_rects`) und der Zustand der Skizze.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::shell::{self, STRIP_BUTTON};
use planora_app::sketch_state::{ConstraintAction, Item, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, sketch_panel, theme};
use planora_feature::SketchPlane;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::CurveId;

const SIZE: Vec2 = vec2(1400.0, 900.0);
const SMALL: Vec2 = vec2(800.0, 600.0);

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    // Hinweistexte sofort zeigen.
    c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(size: Vec2, events: Vec<Event>) -> egui::RawInput {
    // Kurze Bildabstände: Klicks gelten nicht als langes Halten, zwei Klicks als Doppelklick.
    let time = TIME.with(|t| {
        t.set(t.get() + 1.0 / 60.0);
        t.get()
    });
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(time),
        events,
        ..Default::default()
    }
}

fn library_ws() -> Workspace {
    let root = common::temp_root("planora-sketch-fields");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

/// App mit `bodies` Quadern und einer offenen, leeren Skizze auf XY.
fn sketching(bodies: usize) -> PlanoraApp {
    let mut app = PlanoraApp::for_tests(library_ws());
    let s = state(&mut app);
    for _ in 0..bodies {
        s.create_box(DVec3::splat(100.0)).unwrap();
    }
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    app
}

/// Skizze mit einer waagerechten Linie (−10,−10)–(10,−10), ausgewählt; liefert die Linie.
fn with_selected_line(app: &mut PlanoraApp) -> CurveId {
    let (se, sk) = state(app).session_sketch_mut().unwrap();
    let a = sk.add_point(-10.0, -10.0).unwrap();
    let b = sk.add_point(10.0, -10.0).unwrap();
    let line = sk.add_line(a, b).unwrap();
    se.selection = vec![Item::Curve(line)];
    line
}

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
}

#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
    fills: Vec<(Rect, Color32)>,
}

fn walk(shape: &Shape, s: &mut Shot) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |x| x.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            if let Some(c) = t.override_text_color {
                color = c;
            }
            s.texts.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                color,
            });
        }
        Shape::Rect(r) => s.fills.push((r.rect, r.fill)),
        _ => {}
    }
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let mut s = Shot::default();
        for c in shapes {
            walk(&c.shape, &mut s);
        }
        s
    }

    fn find_in(&self, r: Rect, exact: &str) -> &Seen {
        self.texts
            .iter()
            .find(|t| t.text == exact && r.expand(0.5).contains(t.rect.center()))
            .unwrap_or_else(|| {
                panic!(
                    "„{exact}“ fehlt in {r:?}: {:?}",
                    self.texts
                        .iter()
                        .filter(|t| r.contains(t.rect.center()))
                        .map(|t| &t.text)
                        .collect::<Vec<_>>()
                )
            })
    }

    fn has(&self, exact: &str) -> bool {
        self.texts.iter().any(|t| t.text == exact)
    }
}

fn frame(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, events: Vec<Event>) -> Shot {
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(size, events), |ui| app.ui(ui, &mut f));
    let shot = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    shot
}

fn settle(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) -> Shot {
    frame(c, app, size, vec![]);
    frame(c, app, size, vec![]);
    frame(c, app, size, vec![])
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

fn wheel(dy: f32) -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    }
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Klick an `p`; liefert das Bild danach.
fn click(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, p: Pos2) -> Shot {
    frame(c, app, size, vec![moved(p)]);
    frame(c, app, size, vec![moved(p), button(p, true)]);
    frame(c, app, size, vec![button(p, false)]);
    frame(c, app, size, vec![])
}

fn double_click(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, p: Pos2) {
    frame(c, app, size, vec![moved(p)]);
    frame(c, app, size, vec![moved(p), button(p, true)]);
    frame(c, app, size, vec![button(p, false)]);
    frame(c, app, size, vec![button(p, true)]);
    frame(c, app, size, vec![button(p, false)]);
    frame(c, app, size, vec![]);
}

fn field(c: &egui::Context, name: &str) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("Feld {name} fehlt: {:?}", shell_panels::field_rects(c)))
        .1
}

fn names(c: &egui::Context) -> Vec<&'static str> {
    shell_panels::field_rects(c)
        .into_iter()
        .map(|(n, _)| n)
        .collect()
}

/// Klick auf den Text `label` im Skizzenfeld; liefert das Bild danach.
fn press_in_side(c: &egui::Context, app: &mut PlanoraApp, label: &str) -> Shot {
    let shot = settle(c, app, SIZE);
    let p = shot.find_in(field(c, "Skizze"), label).rect.center();
    click(c, app, SIZE, p)
}

// (d) ------------------------------------------------------------------------------------------

#[test]
fn all_nine_tools_are_buttons_with_tooltips_and_widget_info() {
    use egui::accesskit::{Role, Toggled};
    let c = context();
    c.enable_accesskit();
    let mut app = sketching(0);
    settle(&c, &mut app, SIZE);
    let strip = field(&c, "Leiste");
    let buttons = sketch_panel::strip_buttons(&c);
    let labels: Vec<&str> = sketch_panel::TOOLS.iter().map(|(_, l, _)| *l).collect();
    assert_eq!(
        buttons.iter().map(|(l, _)| *l).collect::<Vec<_>>(),
        labels,
        "alle neun Werkzeuge in der Reihenfolge der Liste"
    );
    for (label, r) in &buttons {
        assert!(
            strip.contains_rect(*r),
            "{label} {r:?} in der Leiste {strip:?}"
        );
        assert_eq!(r.size(), vec2(STRIP_BUTTON, STRIP_BUTTON), "{label}");
    }

    // Für Bildschirmleser: Knopf mit Namen, gewählt nur das aktuelle Werkzeug (Auswahl).
    let mut f = eframe::Frame::_new_kittest();
    let mut out = c.run_ui(input(SIZE, vec![]), |ui| app.ui(ui, &mut f));
    let update = out
        .platform_output
        .accesskit_update
        .take()
        .expect("AccessKit-Baum");
    out.drop_without_applying_deltas();
    for (tool, label, _) in sketch_panel::TOOLS {
        let node = update
            .nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| n.label() == Some(label))
            .unwrap_or_else(|| panic!("kein Knoten „{label}“"));
        assert_eq!(node.role(), Role::Button, "{label}");
        let want = if tool == Tool::Select {
            Toggled::True
        } else {
            Toggled::False
        };
        assert_eq!(node.toggled(), Some(want), "{label}");
    }

    let at = |c: &egui::Context, label: &str| {
        sketch_panel::strip_buttons(c)
            .into_iter()
            .find(|(l, _)| *l == label)
            .unwrap()
            .1
    };
    // Überfahren zeigt den Namen und den Hinweis als Hinweistext (vor jedem Klick: egui zeigt
    // kurz nach einem Klick keine Hinweistexte).
    for (_, label, hint) in sketch_panel::TOOLS {
        let r = at(&c, label);
        frame(&c, &mut app, SIZE, vec![moved(r.center())]);
        frame(&c, &mut app, SIZE, vec![moved(r.center())]);
        let shot = frame(&c, &mut app, SIZE, vec![]);
        assert!(shot.has(label), "Hinweistext „{label}“ fehlt");
        assert!(shot.has(hint), "Hinweis zu „{label}“ fehlt");
    }
    let p = theme::palette(true);
    for (tool, label, _) in sketch_panel::TOOLS {
        let r = at(&c, label);
        // Klick wählt das Werkzeug.
        let shot = click(&c, &mut app, SIZE, r.center());
        assert_eq!(state(&mut app).sketch_session.as_ref().unwrap().tool, tool);
        // Gewählt: neutrale Fläche bg_active, nicht die Akzentfarbe.
        assert!(
            shot.fills
                .iter()
                .any(|(f, c)| *c == p.bg_active && (f.center() - r.center()).length() < 0.6_f32),
            "{label}: gewählt ohne bg_active-Fläche"
        );
        assert!(
            !shot
                .fills
                .iter()
                .any(|(f, c)| *c == p.accent && strip.contains_rect(*f)),
            "{label}: Akzentfarbe in der Leiste"
        );
    }
}

// (e) ------------------------------------------------------------------------------------------

#[test]
fn constraint_actions_and_edit_buttons_work_like_before() {
    let c = context();
    let mut app = sketching(0);
    let line = with_selected_line(&mut app);
    let kinds = |app: &mut PlanoraApp| -> Vec<K> {
        state(app)
            .session_sketch()
            .unwrap()
            .constraints()
            .map(|(_, c)| c.kind.clone())
            .collect()
    };

    // Abhängigkeit ohne Zahl: sofort angewandt.
    press_in_side(&c, &mut app, "Waagerecht");
    assert!(
        matches!(kinds(&mut app).as_slice(), [K::Horizontal(..)]),
        "{:?}",
        kinds(&mut app)
    );

    // Maß: das Zahlenfeld öffnet sich, Zahl eingeben und Eingabe übernimmt sie.
    state(&mut app).sketch_session.as_mut().unwrap().selection = vec![Item::Curve(line)];
    press_in_side(&c, &mut app, "Abstand …");
    assert!(sketch_panel::dimension_editor_open(&c), "Zahlenfeld offen");
    frame(&c, &mut app, SIZE, vec![key(Key::A, Modifiers::COMMAND)]);
    frame(&c, &mut app, SIZE, vec![Event::Text("25".into())]);
    frame(&c, &mut app, SIZE, vec![key(Key::Enter, Modifiers::NONE)]);
    settle(&c, &mut app, SIZE);
    assert!(!sketch_panel::dimension_editor_open(&c), "nach Eingabe zu");
    assert!(
        kinds(&mut app)
            .iter()
            .any(|k| matches!(k, K::Distance(_, _, v) if (*v - 25.0).abs() < 1e-9)),
        "{:?}",
        kinds(&mut app)
    );
    let len = state(&mut app)
        .session_sketch()
        .unwrap()
        .line_length(line)
        .unwrap();
    assert!((len - 25.0).abs() < 1e-6, "Linie jetzt 25 lang: {len}");

    // Liste: Klick wählt die Abhängigkeit, Doppelklick auf das Maß öffnet das Zahlenfeld.
    let shot = settle(&c, &mut app, SIZE);
    let side = field(&c, "Skizze");
    let row = shot
        .texts
        .iter()
        .find(|t| t.text.starts_with("Abstand 25") && side.contains(t.rect.center()))
        .expect("Maß in der Liste")
        .rect
        .center();
    click(&c, &mut app, SIZE, row);
    let sel = state(&mut app)
        .sketch_session
        .as_ref()
        .unwrap()
        .selection
        .clone();
    assert!(matches!(sel.as_slice(), [Item::Constraint(_)]), "{sel:?}");
    // Etwas warten (sonst zählte der Klick eben mit), dann doppelt auf dieselbe Zeile.
    for _ in 0..30 {
        frame(&c, &mut app, SIZE, vec![]);
    }
    let shot = settle(&c, &mut app, SIZE);
    let row = shot
        .texts
        .iter()
        .find(|t| t.text.starts_with("Abstand 25") && side.contains(t.rect.center()))
        .expect("Maß in der Liste")
        .rect
        .center();
    double_click(&c, &mut app, SIZE, row);
    assert!(
        sketch_panel::dimension_editor_open(&c),
        "Doppelklick öffnet das Zahlenfeld"
    );
    press_in_side(&c, &mut app, "Abbrechen");
    assert!(!sketch_panel::dimension_editor_open(&c));

    // Konstruktion umschalten.
    state(&mut app).sketch_session.as_mut().unwrap().selection = vec![Item::Curve(line)];
    press_in_side(&c, &mut app, "Konstruktion umschalten");
    let construction = |app: &mut PlanoraApp| {
        state(app)
            .session_sketch()
            .unwrap()
            .curve(line)
            .unwrap()
            .construction
    };
    assert!(construction(&mut app));
    press_in_side(&c, &mut app, "Konstruktion umschalten");
    assert!(!construction(&mut app));

    // Muster: drei Kopien der Linie.
    let curves = |app: &mut PlanoraApp| state(app).session_sketch().unwrap().curves().count();
    press_in_side(&c, &mut app, "Muster …");
    press_in_side(&c, &mut app, "Erzeugen");
    assert_eq!(curves(&mut app), 4, "Linie und drei Kopien");

    // Spiegeln: eine Kurve und zuletzt die Achse.
    let axis = {
        let sk = state(&mut app).session_sketch_mut().unwrap().1;
        let a = sk.add_point(-30.0, 0.0).unwrap();
        let b = sk.add_point(-30.0, 20.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    state(&mut app).sketch_session.as_mut().unwrap().selection =
        vec![Item::Curve(line), Item::Curve(axis)];
    let before = curves(&mut app);
    press_in_side(&c, &mut app, "Spiegeln …");
    press_in_side(&c, &mut app, "Spiegeln");
    assert_eq!(curves(&mut app), before + 1, "gespiegelte Linie");

    // Löschen.
    state(&mut app).sketch_session.as_mut().unwrap().selection = vec![Item::Curve(axis)];
    let before = curves(&mut app);
    press_in_side(&c, &mut app, "Löschen");
    assert_eq!(curves(&mut app), before - 1);
}

// (f) ------------------------------------------------------------------------------------------

/// Skizze mit Linie (ausgewählt) und laufendem Linienwerkzeug, Zahlenfeld „Abstand“ offen.
fn with_editor(c: &egui::Context, size: Vec2) -> PlanoraApp {
    let mut app = sketching(0);
    let line = with_selected_line(&mut app);
    let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
    se.set_tool(sk, Tool::Line);
    let a = match sk.curve(line).unwrap().kind {
        planora_sketch::model::CurveKind::Line { a, .. } => a,
        _ => unreachable!(),
    };
    se.pending = vec![a];
    se.selection = vec![Item::Curve(line)];
    sketch_panel::open_dimension_editor(c, state(&mut app), ConstraintAction::Distance);
    settle(c, &mut app, size);
    app
}

#[test]
fn value_editor_fits_250px_and_takes_keyboard_focus() {
    for size in [SIZE, SMALL] {
        let c = context();
        let mut app = with_editor(&c, size);
        // Das Skizzenfeld rollt das Zahlenfeld ins Bild (mit kurzer Bewegung).
        for _ in 0..30 {
            frame(&c, &mut app, size, vec![]);
        }
        let shot = settle(&c, &mut app, size);
        let side = field(&c, "Skizze");
        assert_eq!(side.width(), theme::PANEL_WIDTH, "{size:?}");
        let inner = side.shrink(theme::SPACE_3);
        let ok = shot.find_in(side, "OK").rect;
        let cancel = shot.find_in(side, "Abbrechen").rect;
        let value = shot.find_in(side, "20,00").rect;
        for (what, r) in [("OK", ok), ("Abbrechen", cancel), ("Zahl", value)] {
            assert!(
                inner.expand(0.5).contains_rect(r),
                "{size:?}: {what} {r:?} ragt aus dem Feld {inner:?}"
            );
        }
        // Gestapelt: OK und Abbrechen unter dem Zahlenfeld.
        assert!(ok.top() > value.bottom(), "OK unter dem Feld");
        assert!(cancel.top() > value.bottom(), "Abbrechen unter dem Feld");
        // Das Zahlenfeld hat die Tastatur.
        assert!(
            c.egui_wants_keyboard_input(),
            "{size:?}: Zahlenfeld ohne Fokus"
        );
    }

    // Esc, Eingabe und Entf erreichen die Skizze nicht, solange das Zahlenfeld Fokus hat.
    for k in [Key::Escape, Key::Enter, Key::Delete] {
        let c = context();
        let mut app = with_editor(&c, SIZE);
        let before = state(&mut app).sketch_session.clone().unwrap();
        frame(&c, &mut app, SIZE, vec![key(k, Modifiers::NONE)]);
        settle(&c, &mut app, SIZE);
        let se = state(&mut app).sketch_session.clone().unwrap();
        assert_eq!(se.pending, before.pending, "{k:?}: Werkzeug läuft weiter");
        assert_eq!(se.selection, before.selection, "{k:?}: Auswahl unverändert");
        assert_eq!(
            state(&mut app).session_sketch().unwrap().curves().count(),
            1,
            "{k:?}: nichts gelöscht"
        );
    }
}

// (g) ------------------------------------------------------------------------------------------

#[test]
fn foot_shows_status_and_end_sketch() {
    let p = theme::palette(true);
    let c = context();
    let mut app = sketching(0);
    // Leere Skizze, gelöst: vollständig bestimmt (grün).
    {
        let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
        se.last_report = Some(sk.clone().solve());
    }
    let shot = settle(&c, &mut app, SIZE);
    let foot = field(&c, "Fuß");
    let t = shot.find_in(foot, "Vollständig bestimmt");
    assert_eq!(t.color, p.ok);
    // Ein freier Punkt: zwei Freiheitsgrade, neutral.
    {
        let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
        sk.add_point(5.0, 5.0).unwrap();
        se.last_report = Some(sk.clone().solve());
    }
    let shot = settle(&c, &mut app, SIZE);
    let t = shot.find_in(foot, "2 Freiheitsgrade");
    assert_eq!(t.color, p.text_secondary);
    // Der Hinweis zum Werkzeug steht ebenfalls im Fuß.
    let hint = sketch_panel::TOOLS[0].2;
    assert!(
        shot.texts
            .iter()
            .any(|t| foot.contains(t.rect.center())
                && hint.starts_with(t.text.trim_end_matches('…'))),
        "Werkzeughinweis im Fuß"
    );
    // „Skizze beenden“ schließt die Skizze.
    let end = shot.find_in(foot, "Skizze beenden").rect.center();
    click(&c, &mut app, SIZE, end);
    assert!(state(&mut app).sketch_session.is_none(), "Skizze beendet");
    let shot = settle(&c, &mut app, SIZE);
    assert!(!shot.has("Skizze beenden"));
    let n = names(&c);
    for gone in ["Leiste", "Skizze", "Fuß"] {
        assert!(!n.contains(&gone), "{gone} nach der Skizze: {n:?}");
    }
    for back in ["links", "rechts", "unten"] {
        assert!(n.contains(&back), "{back} wieder da: {n:?}");
    }
}

// (h) ------------------------------------------------------------------------------------------

#[test]
fn pointer_on_sketch_fields_does_not_reach_the_sketch() {
    for size in [SIZE, SMALL] {
        let c = context();
        let mut app = sketching(0);
        {
            let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
            se.set_tool(sk, Tool::Point);
        }
        settle(&c, &mut app, size);
        let points = |app: &mut PlanoraApp| state(app).session_sketch().unwrap().points().count();
        let n = names(&c);
        for gone in ["links", "rechts", "unten"] {
            assert!(
                !n.contains(&gone),
                "{size:?}: {gone} im Skizzenmodus: {n:?}"
            );
        }
        // Auf dem Knopf „Punkt“ (bleibt das Werkzeug), mitten im Feld, links im Fuß (Status).
        let punkt = sketch_panel::strip_buttons(&c)
            .into_iter()
            .find(|(l, _)| *l == "Punkt")
            .unwrap()
            .1;
        let foot = field(&c, "Fuß");
        for (name, p) in [
            ("Leiste", punkt.center()),
            ("Skizze", field(&c, "Skizze").center()),
            ("Fuß", foot.left_center() + vec2(6.0, 0.0)),
            ("oben", field(&c, "oben").left_center() + vec2(6.0, 0.0)),
            ("Status", field(&c, "Status").center()),
        ] {
            assert_eq!(
                c.layer_id_at(p).map(|l| l.order),
                Some(egui::Order::Middle),
                "{size:?}: {name}"
            );
            click(&c, &mut app, size, p);
            assert_eq!(
                points(&mut app),
                0,
                "{size:?}: Klick auf {name} setzt keinen Punkt"
            );
            let d = state(&mut app).camera.distance;
            frame(&c, &mut app, size, vec![moved(p), wheel(120.0)]);
            frame(&c, &mut app, size, vec![]);
            assert_eq!(
                state(&mut app).camera.distance,
                d,
                "{size:?}: Mausrad auf {name} zoomt nicht"
            );
        }
        assert_eq!(
            state(&mut app).sketch_session.as_ref().unwrap().tool,
            Tool::Point
        );

        // Gegenprobe in der freien Fläche (links unten, neben der Leiste, abseits des Würfels).
        let free = shell_panels::last_layout(&c).unwrap().free;
        let p = pos2(free.left() + 40.0, free.bottom() - 40.0);
        assert_eq!(
            c.layer_id_at(p).map(|l| l.order),
            Some(egui::Order::Background)
        );
        let d = state(&mut app).camera.distance;
        frame(&c, &mut app, size, vec![moved(p), wheel(120.0)]);
        frame(&c, &mut app, size, vec![]);
        assert_ne!(
            state(&mut app).camera.distance,
            d,
            "{size:?}: Mausrad in der freien Fläche zoomt"
        );
        click(&c, &mut app, size, p);
        assert_eq!(
            points(&mut app),
            1,
            "{size:?}: Klick in der freien Fläche setzt einen Punkt"
        );
    }
}

// (i) ------------------------------------------------------------------------------------------

#[test]
fn no_sketch_window_remains() {
    let c = context();
    let mut app = sketching(0);
    settle(&c, &mut app, SIZE);
    // Kein Fenster: keine sichtbare Ebene im Vordergrund, solange kein Dialog offen ist.
    let foreground: Vec<egui::LayerId> = c.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|l| l.order == egui::Order::Foreground)
            .collect()
    });
    assert!(
        foreground.is_empty(),
        "Fenster im Skizzenmodus: {foreground:?}"
    );
    let n = names(&c);
    for want in ["Leiste", "Skizze", "Fuß", "oben", "Status"] {
        assert!(n.contains(&want), "{want} fehlt: {n:?}");
    }
    // Im Quelltext gibt es das Fenster nicht mehr.
    let src = include_str!("../src/sketch_panel.rs");
    assert!(
        !src.contains("theme::window"),
        "sketch_panel ruft theme::window"
    );
    assert!(
        !src.contains("egui::Window"),
        "sketch_panel baut ein egui::Window"
    );
    // Ohne Shell (Tests): dieselben drei Felder an den Stellen aus `layout_in_mode`.
    let c = context();
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    for _ in 0..3 {
        c.run_ui(input(SIZE, vec![]), |ui| {
            sketch_panel::show(ui.ctx(), &mut s)
        })
        .drop_without_applying_deltas();
    }
    let r = shell::layout_in_mode(
        Rect::from_min_size(Pos2::ZERO, SIZE),
        shell::ShellMode::Sketch,
        &shell::ShellState::default(),
        (SIZE.x, 0.0),
        0.0,
        0.0,
        false,
    );
    assert_eq!(field(&c, "Leiste").min, r.strip.unwrap().min);
    assert_eq!(field(&c, "Skizze").min, r.right.min);
    assert_eq!(field(&c, "Fuß").top(), r.bottom.top());
}

// (j) ------------------------------------------------------------------------------------------

/// Klick auf den Eintrag `entry` im Menü `menu` des oberen Felds.
fn menu_entry(c: &egui::Context, app: &mut PlanoraApp, menu: &str, entry: &str) {
    let shot = settle(c, app, SIZE);
    let top = field(c, "oben");
    let m = shot.find_in(top, menu).rect.center();
    let shot = click(c, app, SIZE, m);
    let e = shot
        .texts
        .iter()
        .find(|t| t.text == entry && t.rect.top() > top.bottom())
        .unwrap_or_else(|| panic!("Menüeintrag „{entry}“ fehlt"))
        .rect
        .center();
    click(c, app, SIZE, e);
}

#[test]
fn menu_still_ends_the_sketch_and_opens_dialogs() {
    for (menu, entry) in [
        ("Erzeugen", "Quader …"),
        ("Erzeugen", "Skizze extrudieren …"),
        ("Bearbeiten", "Vereinigen …"),
        ("Bearbeiten", "Fase …"),
    ] {
        let c = context();
        let mut app = sketching(2);
        settle(&c, &mut app, SIZE);
        // Das Befehlsfeld gibt es im Skizzenmodus nicht; Befehle laufen über das Menü.
        assert!(!names(&c).contains(&"unten"));
        assert!(shell_panels::bottom_buttons(&c).is_empty());
        menu_entry(&c, &mut app, menu, entry);
        let t = app.ws_mut().active_tab_mut();
        let opened = t.dialog.is_some() || t.boolean_dialog.is_some() || t.blend_dialog.is_some();
        assert!(opened, "{entry}: Dialog geöffnet");
        assert!(t.state.sketch_session.is_none(), "{entry}: Skizze beendet");
    }
}

// Etappe-B-Übergabe: Würfel und Home im Skizzenmodus -------------------------------------------

#[test]
fn cube_and_home_work_in_sketch_mode_without_adding_points() {
    use planora_app::viewport;
    use planora_render::{ViewFace, current_region};
    let c = context();
    let mut app = sketching(0);
    {
        let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Point);
    }
    settle(&c, &mut app, SIZE);
    let free = shell_panels::last_layout(&c).unwrap().free;
    let cube = viewport::cube_rect(free);
    // Klick auf die Mitte des Würfels (Fläche zur Kamera): Ansicht auf diese Fläche, kein Punkt.
    click(&c, &mut app, SIZE, cube.center());
    let points = |app: &mut PlanoraApp| state(app).session_sketch().unwrap().points().count();
    assert_eq!(
        points(&mut app),
        0,
        "Klick auf den Würfel setzt keinen Punkt"
    );
    assert_eq!(
        current_region(&state(&mut app).camera, 2.0),
        Some(planora_render::CubeRegion::Face(ViewFace::Top)),
        "Skizze auf XY: Blick von oben, Klick auf die Mitte bleibt oben"
    );
    // Home: Grundansicht, kein Punkt.
    click(&c, &mut app, SIZE, viewport::home_rect(cube).center());
    assert_eq!(points(&mut app), 0, "Home setzt keinen Punkt");
    assert!(
        matches!(
            current_region(&state(&mut app).camera, 2.0),
            Some(planora_render::CubeRegion::Corner { .. })
        ),
        "Home zeigt die Isometrie"
    );
    assert!(
        state(&mut app).sketch_session.is_some(),
        "Skizze bleibt offen"
    );
}

// Kleine Fenster: die gezeichneten Felder überlappen sich nicht -------------------------------

#[test]
fn painted_sketch_fields_do_not_overlap_at_five_sizes() {
    for size in [
        vec2(800.0, 600.0),
        vec2(1100.0, 700.0),
        vec2(1400.0, 900.0),
        vec2(1920.0, 1080.0),
        vec2(2560.0, 1440.0),
    ] {
        let c = context();
        let mut app = sketching(0);
        settle(&c, &mut app, size);
        let layout = shell_panels::last_layout(&c).unwrap();
        let free = layout.free;
        // Jedes Skizzenfeld bleibt in seinem Rechteck (der Inhalt drückt es nicht breiter).
        for (name, want) in [
            ("Leiste", layout.strip.unwrap()),
            ("Skizze", layout.right),
            ("Fuß", layout.bottom),
        ] {
            let got = field(&c, name);
            assert!(
                want.expand(0.5).contains_rect(got),
                "{size:?}: {name} {got:?} ragt aus {want:?}"
            );
        }
        let mut all = shell_panels::field_rects(&c);
        all.push(("frei", free));
        let window = Rect::from_min_size(Pos2::ZERO, size);
        for (name, r) in &all {
            assert!(
                window.contains_rect(*r),
                "{size:?}: {name} {r:?} ragt aus dem Fenster"
            );
        }
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                let o = all[i].1.intersect(all[j].1);
                assert!(
                    o.width() <= 0.5 || o.height() <= 0.5,
                    "{size:?}: {} und {} überlappen: {:?} / {:?}",
                    all[i].0,
                    all[j].0,
                    all[i].1,
                    all[j].1
                );
            }
        }
    }
}
