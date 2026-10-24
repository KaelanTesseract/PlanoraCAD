//! Befehlsfeld unten (Graphit, Etappe B, Aufgabe 5): Chips für die Booleschen Operationen und
//! Symbolknöpfe für die Erzeugen- und Bearbeiten-Aktionen des Menüs. Jede Schaltfläche muss
//! dasselbe bewirken wie ihr Menüeintrag, dieselben Sperren haben und ihre Farben aus der Palette
//! beziehen. Ohne Fenster: Rohereignisse je Bild gegen einen egui-Kontext, die Knopflagen kommen
//! aus `shell_panels::bottom_buttons`.

mod common;

use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::shell::{self, SHELL_INSET};
use planora_app::state::StandardPlane;
use planora_app::workspace::{After, Prompt, Workspace};
use planora_app::{fonts, shell_panels, sketch_panel, theme, versions_panel, viewport};
use planora_feature::SketchPlane;
use planora_kernel::BoolOp;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;

/// Fläche des Feature-Baums bei der voreingestellten Durchsichtigkeit.
fn tree_fill(dark: bool) -> Color32 {
    theme::panel_frame_translucent(dark, planora_project::config::DEFAULT_PANEL_TRANSPARENCY).fill
}

fn context(dark: bool) -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, dark);
    // Hinweistexte sofort zeigen.
    c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(size: Vec2, events: Vec<Event>) -> egui::RawInput {
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
    let root = common::temp_root("planora-cmd");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// App mit `n` Quadern im aktiven Tab und, auf Wunsch, einer Skizze (für Extrudieren und Drehen).
fn app_with(n: usize, sketch: bool) -> PlanoraApp {
    let mut app = PlanoraApp::for_tests(library_ws());
    let state = &mut app.ws_mut().active_tab_mut().state;
    for _ in 0..n {
        state.create_box(DVec3::splat(100.0)).unwrap();
    }
    if sketch {
        state
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
    }
    app
}

fn read_only_ws() -> Workspace {
    let mut w = library_ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    for _ in 0..2 {
        w.active_tab_mut()
            .state
            .create_box(DVec3::splat(3.0))
            .unwrap();
    }
    w.save_active("Stand 2", None).unwrap();
    versions_panel::open_version(&mut w, 1);
    assert!(w.active_tab().read_only);
    w
}

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
    elided: bool,
    rows: usize,
}

#[derive(Debug, Clone, Copy)]
struct Fill {
    rect: Rect,
    fill: Color32,
    stroke: egui::Stroke,
    radius: egui::CornerRadius,
}

#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
    fills: Vec<Fill>,
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
                elided: t.galley.elided,
                rows: t.galley.rows.len(),
            });
        }
        Shape::Rect(r) => s.fills.push(Fill {
            rect: r.rect,
            fill: r.fill,
            stroke: r.stroke,
            radius: r.corner_radius,
        }),
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

    /// Rahmen der schwebenden Felder (bg_panel, 1 px border_subtle, Rundung 14).
    fn fields(&self, dark: bool) -> Vec<Rect> {
        let p = theme::palette(dark);
        self.fills
            .iter()
            .filter(|f| {
                // Der Feature-Baum ist durchsichtig (Voreinstellung 40 %), die anderen deckend.
                (f.fill == p.bg_panel || f.fill == tree_fill(dark))
                    && f.stroke == egui::Stroke::new(1.0, p.border_subtle)
                    && f.radius == egui::CornerRadius::same(theme::RADIUS_LG)
            })
            .map(|f| f.rect)
            .collect()
    }

    fn texts_in(&self, r: Rect) -> Vec<&Seen> {
        self.texts
            .iter()
            .filter(|t| r.expand(0.5).contains(t.rect.center()))
            .collect()
    }

    fn find(&self, r: Rect, exact: &str) -> &Seen {
        self.texts
            .iter()
            .find(|t| t.text == exact && r.expand(0.5).contains(t.rect.center()))
            .unwrap_or_else(|| {
                panic!(
                    "„{exact}“ fehlt in {r:?}: {:?}",
                    self.texts_in(r).iter().map(|t| &t.text).collect::<Vec<_>>()
                )
            })
    }
}

fn run(
    c: &egui::Context,
    app: &mut PlanoraApp,
    size: Vec2,
    events: Vec<Event>,
    with_viewport: bool,
) -> (Shot, Rect) {
    let mut free = Rect::NOTHING;
    let out = c.run_ui(input(size, events), |ui| {
        free = shell_panels::show_fields(ui.ctx(), app);
        if with_viewport {
            viewport::show_in(ui, &mut app.ws_mut().active_tab_mut().state, Some(free));
        }
        // Im Skizzenmodus: Leiste, Skizzenfeld und Fuß wie in der App.
        sketch_panel::show(ui.ctx(), &mut app.ws_mut().active_tab_mut().state);
    });
    let shot = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    (shot, free)
}

fn settle(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) -> (Shot, Rect) {
    let mut last = run(c, app, size, vec![], false);
    for _ in 0..2 {
        last = run(c, app, size, vec![], false);
    }
    last
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

fn click(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, p: Pos2, with_viewport: bool) {
    for events in [
        vec![moved(p)],
        vec![moved(p), button(p, true)],
        vec![button(p, false)],
        vec![],
    ] {
        run(c, app, size, events, with_viewport);
    }
}

const SIZE: Vec2 = vec2(1400.0, 900.0);

/// Lage des Befehlsknopfs `label` (nach einigen Bildern, damit die Lagen stehen).
fn pos_of(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, label: &str) -> Pos2 {
    settle(c, app, size);
    shell_panels::bottom_buttons(c)
        .into_iter()
        .find(|(l, _)| l == label)
        .unwrap_or_else(|| panic!("Knopf „{label}“ fehlt"))
        .1
        .center()
}

/// Klick auf den Befehlsknopf `label`.
fn press(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, label: &str) {
    let p = pos_of(c, app, size, label);
    click(c, app, size, p, false);
}

/// Klick auf den Eintrag `entry` im Menü `menu` des oberen Felds.
fn menu_entry(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, menu: &str, entry: &str) {
    let (shot, _) = settle(c, app, size);
    let top = shot
        .fields(true)
        .into_iter()
        .find(|r| (r.top() - SHELL_INSET).abs() < 0.5)
        .expect("oberes Feld");
    let m = shot.find(top, menu).rect.center();
    click(c, app, size, m, false);
    let (shot, _) = settle(c, app, size);
    let e = shot
        .texts
        .iter()
        .find(|t| t.text == entry && t.rect.top() > top.bottom())
        .unwrap_or_else(|| panic!("Menüeintrag „{entry}“ fehlt"))
        .rect
        .center();
    click(c, app, size, e, false);
}

fn dialogs(app: &mut PlanoraApp) -> String {
    let t = app.ws_mut().active_tab_mut();
    format!(
        "{:?} | {:?} | {:?}",
        t.dialog, t.boolean_dialog, t.blend_dialog
    )
}

const NONE: &str = "None | None | None";

/// Alle Knöpfe des Befehlsfelds in der Reihenfolge der Anzeige.
const ALL_LABELS: [&str; 11] = [
    "Vereinigen",
    "Abziehen",
    "Schneiden",
    "Quader …",
    "Zylinder …",
    "Kugel …",
    "Kegel …",
    "Skizze extrudieren …",
    "Skizze drehen …",
    "Verrunden …",
    "Fase …",
];

// ---------------------------------------------------------------------------------------------

#[test]
fn every_bottom_action_matches_a_menu_action() {
    // (Knopf, Menü, Menüeintrag)
    let cases: [(&str, &str, &str); 11] = [
        ("Quader …", "Erzeugen", "Quader …"),
        ("Zylinder …", "Erzeugen", "Zylinder …"),
        ("Kugel …", "Erzeugen", "Kugel …"),
        ("Kegel …", "Erzeugen", "Kegel …"),
        ("Skizze extrudieren …", "Erzeugen", "Skizze extrudieren …"),
        ("Skizze drehen …", "Erzeugen", "Skizze drehen …"),
        ("Verrunden …", "Bearbeiten", "Verrunden …"),
        ("Fase …", "Bearbeiten", "Fase …"),
        ("Vereinigen", "Bearbeiten", "Vereinigen …"),
        ("Abziehen", "Bearbeiten", "Abziehen …"),
        ("Schneiden", "Bearbeiten", "Schneiden …"),
    ];
    for (field_label, menu, entry) in cases {
        let c = context(true);
        let mut via_field = app_with(2, true);
        press(&c, &mut via_field, SIZE, field_label);
        let field = dialogs(&mut via_field);

        let c = context(true);
        let mut via_menu = app_with(2, true);
        menu_entry(&c, &mut via_menu, SIZE, menu, entry);
        let from_menu = dialogs(&mut via_menu);

        assert_eq!(field, from_menu, "{field_label}");
        assert_ne!(field, NONE, "{field_label} öffnet einen Dialog");
    }
}

#[test]
fn chips_open_the_boolean_dialog_with_the_operation() {
    // Der Dialog nimmt die Operation vorgewählt an (`BooleanDialog::with_op`).
    for (label, op) in [
        ("Vereinigen", BoolOp::Union),
        ("Abziehen", BoolOp::Difference),
        ("Schneiden", BoolOp::Intersection),
    ] {
        let c = context(true);
        let mut app = app_with(2, false);
        press(&c, &mut app, SIZE, label);
        let d = app
            .ws_mut()
            .active_tab_mut()
            .boolean_dialog
            .clone()
            .unwrap_or_else(|| panic!("{label}: kein Dialog"));
        assert_eq!(d.op, op, "{label}");
    }
}

#[test]
fn bottom_field_is_disabled_in_read_only_tabs_and_during_a_prompt() {
    let blocked = |mut app: PlanoraApp, what: &str| {
        let c = context(true);
        for label in ALL_LABELS {
            press(&c, &mut app, SIZE, label);
            assert_eq!(
                dialogs(&mut app),
                NONE,
                "{what}: „{label}“ wirkt trotz Sperre"
            );
        }
    };
    // Schreibgeschützter Tab (alte Version).
    blocked(PlanoraApp::for_tests(read_only_ws()), "schreibgeschützt");
    // Offene Rückfrage.
    let mut app = app_with(2, true);
    let tab = app.ws_mut().active_tab().id;
    app.ws_mut().prompt = Some(Prompt::SaveChanges {
        tab,
        then: After::CloseTab,
    });
    blocked(app, "offene Rückfrage");
    // Gegenprobe: ohne Sperre wirkt derselbe Klick.
    let c = context(true);
    let mut app = app_with(2, true);
    press(&c, &mut app, SIZE, "Quader …");
    assert_ne!(dialogs(&mut app), NONE);
}

#[test]
fn chip_colours_come_from_the_palette_in_both_themes() {
    for dark in [true, false] {
        let c = context(dark);
        let mut app = app_with(2, false);
        app.ws_mut().active_tab_mut().state.dark = dark;
        let p = theme::palette(dark);
        let (shot, _) = settle(&c, &mut app, SIZE);
        let buttons = shell_panels::bottom_buttons(&c);
        for (label, (fill, text)) in [
            ("Vereinigen", p.chip_bridge),
            ("Abziehen", p.chip_split),
            ("Schneiden", p.chip_boolean),
        ] {
            let rect = buttons.iter().find(|(l, _)| l == label).unwrap().1;
            let seen = shot
                .texts
                .iter()
                .find(|t| t.text == label && rect.contains(t.rect.center()))
                .unwrap_or_else(|| panic!("{label} nicht gezeichnet"));
            assert_eq!(seen.color, text, "{label} dark={dark}: Textfarbe");
            assert!(
                shot.fills
                    .iter()
                    .any(|f| f.fill == fill && f.rect.contains(seen.rect.center())),
                "{label} dark={dark}: Fläche {fill:?}"
            );
        }
    }
}

/// Texte eines Bildes, nachdem der Zeiger einige Bilder auf `p` stand (Hinweistexte erscheinen).
fn hover_texts(c: &egui::Context, app: &mut PlanoraApp, p: Pos2) -> Vec<Seen> {
    let mut texts = vec![];
    for _ in 0..4 {
        texts = run(c, app, SIZE, vec![moved(p)], false).0.texts;
    }
    texts
}

#[test]
fn tooltips_name_the_action_in_german() {
    for label in ALL_LABELS {
        let c = context(true);
        let mut app = app_with(2, true);
        let p = pos_of(&c, &mut app, SIZE, label);
        // Der Hinweis heißt wie der Menüeintrag und steht außerhalb des Knopfs.
        let want = match label {
            "Vereinigen" | "Abziehen" | "Schneiden" => format!("{label} …"),
            other => other.to_string(),
        };
        let texts = hover_texts(&c, &mut app, p);
        assert!(
            texts
                .iter()
                .any(|t| t.text == want && (t.rect.center() - p).length() > 1.0),
            "Hinweistext „{want}“ fehlt"
        );
    }
}

#[test]
fn a_blocked_button_explains_why_on_hover() {
    // Ohne Skizze: Extrudieren nennt den Grund wie das Menü.
    let c = context(true);
    let mut app = app_with(2, false);
    let p = pos_of(&c, &mut app, SIZE, "Skizze extrudieren …");
    let texts = hover_texts(&c, &mut app, p);
    assert!(
        texts
            .iter()
            .any(|t| t.text == "Zuerst eine Skizze anlegen.")
    );
    // Schreibgeschützt und im Skizzenmodus: ein Satz zum Grund.
    let c = context(true);
    let mut app = PlanoraApp::for_tests(read_only_ws());
    let p = pos_of(&c, &mut app, SIZE, "Quader …");
    let texts = hover_texts(&c, &mut app, p);
    assert!(
        texts
            .iter()
            .any(|t| t.text == "In einer alten Version lässt sich nichts ändern.")
    );
}

// ---- Lage, Status und Zeigerrouting mit dem echten Befehlsfeld --------------------------------

struct Fields {
    top: Rect,
    left: Rect,
    right: Rect,
    status: Rect,
    /// `None`, wenn das Feld einer langen Meldung weicht.
    bottom: Option<Rect>,
}

fn overlaps(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 0.5 && i.height() > 0.5
}

fn fields_of(shot: &Shot, dark: bool, size: Vec2) -> Fields {
    let all = shot.fields(dark);
    let near = |a: f32, b: f32| (a - b).abs() < 0.5;
    let pick = |what: &str, f: &dyn Fn(&Rect) -> bool| -> Rect {
        let hits: Vec<_> = all.iter().filter(|r| f(r)).collect();
        assert_eq!(hits.len(), 1, "{what}: {hits:?} in {all:?}");
        *hits[0]
    };
    let w = size.x;
    Fields {
        top: pick("oberes Feld", &|r| {
            near(r.top(), SHELL_INSET) && near(r.center().x, w / 2.0)
        }),
        // Im Skizzenmodus steht links die Werkzeugleiste.
        left: pick("linkes Feld", &|r| {
            near(r.left(), SHELL_INSET)
                && (near(r.top(), SHELL_INSET) || near(r.width(), shell::STRIP_W))
        }),
        right: pick("rechtes Feld", &|r| {
            near(r.right(), w - SHELL_INSET) && near(r.top(), SHELL_INSET)
        }),
        status: pick("Statusfeld", &|r| {
            near(r.left(), SHELL_INSET) && near(r.bottom(), size.y - SHELL_INSET)
        }),
        bottom: {
            let hits: Vec<_> = all
                .iter()
                .filter(|r| {
                    near(r.bottom(), size.y - SHELL_INSET)
                        && near(r.center().x, w / 2.0)
                        && near(r.height(), shell::BOTTOM_H)
                })
                .collect();
            assert!(hits.len() <= 1, "Befehlsfeld: {hits:?}");
            hits.first().map(|r| **r)
        },
    }
}

/// Was das Statusfeld bei `size` zeigt, samt Lage des Befehlsfelds und der Knopflagen.
struct StatusView {
    seen: Seen,
    status: Rect,
    bottom: Rect,
}

/// Status und Befehlsfeld bei `size`: das Befehlsfeld steht immer da, nichts überdeckt die freie
/// Fläche oder ein anderes Feld.
fn status_at(app: &mut PlanoraApp, size: Vec2) -> StatusView {
    let c = context(true);
    let (shot, free) = settle(&c, app, size);
    let f = fields_of(&shot, true, size);
    let bottom = f.bottom.unwrap_or_else(|| {
        panic!(
            "Befehlsfeld fehlt: {:?} {:?}",
            shot.fields(true),
            shell_panels::last_layout(&c).map(|l| l.bottom)
        )
    });
    let t = shot.texts_in(f.status);
    assert_eq!(t.len(), 1, "{t:?}");
    let seen = t[0].clone();
    assert!(seen.rows <= 3, "{size:?}: {} Zeilen", seen.rows);
    assert!(f.status.contains_rect(seen.rect));
    for other in [f.left, f.right, f.top, free, bottom] {
        assert!(!overlaps(f.status, other), "{:?} / {other:?}", f.status);
    }
    for other in [f.left, f.right, f.top, free] {
        assert!(!overlaps(bottom, other), "{bottom:?} / {other:?}");
    }
    assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(bottom));
    StatusView {
        seen,
        status: f.status,
        bottom,
    }
}

/// Mindestbreite des Befehlsfelds: die drei Chips mit Trennstrich und Rand.
const MIN_BOTTOM: f32 = 250.0;

#[test]
fn status_stays_readable_with_the_real_bottom_field() {
    let p = theme::palette(true);
    let big = vec2(1400.0, 900.0);
    let small = vec2(1100.0, 700.0);
    let mut app = app_with(1, false);
    for size in [big, small] {
        let v = status_at(&mut app, size);
        assert_eq!(
            (v.seen.text.as_str(), v.seen.color),
            ("1 Körper", p.text_secondary)
        );
        assert!(!v.seen.elided && v.seen.rows == 1);
        assert!(v.bottom.width() >= MIN_BOTTOM, "{size:?}: {:?}", v.bottom);
    }

    // Skizzenstatus (die Tasten stehen im Hover): bei 1400 und 1100 ganz zu lesen; unten steht dann das
    // Fußfeld der Skizze statt des Befehlsfelds.
    let state = &mut app.ws_mut().active_tab_mut().state;
    let id = state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    state.open_sketch(id).unwrap();
    let hint = "Skizzen-Modus: Skizze 1";
    for size in [big, small] {
        let v = status_at(&mut app, size);
        assert_eq!(v.seen.text, hint, "{size:?}");
        assert!(!v.seen.elided, "{size:?}: Hinweis gekürzt");
        assert!(v.bottom.width() >= MIN_BOTTOM, "{size:?}: {:?}", v.bottom);
    }

    // Lange Fehlermeldung (mehr als 150 Zeichen).
    let long = "Erzeugen fehlgeschlagen: Die Fläche liegt nicht in einer Ebene, deshalb lässt sich darauf keine Skizze anlegen. Bitte zuerst eine ebene Fläche des Körpers wählen.";
    app.ws_mut().active_tab_mut().state.message = Some(long.into());
    let v = status_at(&mut app, big);
    assert_eq!((v.seen.text.as_str(), v.seen.color), (long, p.error));
    assert!(!v.seen.elided, "1400: Meldung ganz sichtbar");
    assert!(v.bottom.width() >= MIN_BOTTOM);
    // Bei 1100 px bleibt neben dem Befehlsfeld weniger Platz: die Meldung bricht in drei
    // Zeilen um und ist trotzdem ganz zu lesen (das Feld steht auch dann mit Mindestbreite da).
    let v = status_at(&mut app, small);
    assert_eq!(v.seen.text, long);
    assert!(!v.seen.elided, "1100: Meldung ganz sichtbar");
    assert_eq!(v.seen.rows, 3);
    assert!(v.bottom.width() >= MIN_BOTTOM, "{:?}", v.bottom);
    // Eine noch längere Meldung endet mit „…“ und zeigt sich beim Überfahren ganz.
    let huge = "Sehr lange Meldung. ".repeat(40);
    app.ws_mut().active_tab_mut().state.message = Some(huge.clone());
    let v = status_at(&mut app, small);
    assert!(v.seen.elided && v.seen.rows == 3);
    assert!(v.bottom.width() >= MIN_BOTTOM);
    let c = context(true);
    settle(&c, &mut app, small);
    let hover = v.status.center();
    let shot = (0..4)
        .map(|_| run(&c, &mut app, small, vec![moved(hover)], false).0)
        .last()
        .unwrap();
    assert!(
        shot.texts.iter().any(|t| t.text == huge && !t.elided),
        "voller Text beim Überfahren"
    );
}

/// Bei 800 × 600 steht das Befehlsfeld mit der ganzen verfügbaren Breite zwischen den Spalten
/// (die Lage der oberen Leiste ist dort die bekannte Grenze von Aufgabe 2).
#[test]
fn the_bottom_field_is_present_at_800x600() {
    let size = vec2(800.0, 600.0);
    let c = context(true);
    let mut app = app_with(2, false);
    let (shot, free) = settle(&c, &mut app, size);
    let bottom = shot
        .fields(true)
        .into_iter()
        .find(|r| {
            (r.bottom() - (size.y - SHELL_INSET)).abs() < 0.5 && r.height() == shell::BOTTOM_H
        })
        .expect("Befehlsfeld bei 800 x 600");
    assert!(
        bottom.width() >= MIN_BOTTOM.min(size.x - 2.0 * (theme::PANEL_WIDTH + 28.0)),
        "{bottom:?}"
    );
    assert!(!overlaps(bottom, free), "{bottom:?} / {free:?}");
}

#[test]
fn all_buttons_sit_inside_the_bottom_field_at_normal_sizes() {
    for size in [vec2(1400.0, 900.0), vec2(1920.0, 1080.0)] {
        let c = context(true);
        let mut app = app_with(2, true);
        let (shot, _) = settle(&c, &mut app, size);
        let f = fields_of(&shot, true, size);
        let buttons = shell_panels::bottom_buttons(&c);
        assert_eq!(buttons.len(), ALL_LABELS.len());
        for (label, r) in buttons {
            let bottom = f.bottom.expect("Befehlsfeld");
            assert!(bottom.contains_rect(r), "{label}: {r:?} in {bottom:?}");
        }
    }
}

/// Klick und Ziehen auf einem echten Knopf des Befehlsfelds dreht und wählt im 3D-Bereich nichts.
#[test]
fn clicks_on_a_real_bottom_button_do_not_reach_the_viewport() {
    let size = vec2(1400.0, 900.0);
    let c = context(true);
    let mut app = app_with(1, false);
    run(&c, &mut app, size, vec![], true);
    {
        let state = &mut app.ws_mut().active_tab_mut().state;
        state.frame_all();
        // Der Quader füllt das ganze Fenster, auch unter dem Befehlsfeld.
        state.camera.distance *= 0.25;
    }
    for _ in 0..3 {
        run(&c, &mut app, size, vec![], true);
    }
    let cam = |app: &mut PlanoraApp| {
        let s = &app.ws_mut().active_tab_mut().state;
        (s.camera.yaw, s.camera.pitch)
    };
    let before = cam(&mut app);
    // Knopf „Quader …“: der Klick wirkt aufs Feld (Dialog), nicht auf die Ansicht.
    let p = pos_of(&c, &mut app, size, "Quader …");
    let layer = c.layer_id_at(p).expect("Ebene unter dem Zeiger");
    assert_eq!(layer.order, egui::Order::Middle);
    click(&c, &mut app, size, p, true);
    assert_eq!(app.ws_mut().active_tab_mut().state.selection, None);
    assert!(app.ws_mut().active_tab_mut().dialog.is_some());
    // Ziehen von einem Knopf aus dreht nichts.
    let p = pos_of(&c, &mut app, size, "Kegel …");
    let to = p + vec2(80.0, 20.0);
    let mut events = vec![vec![moved(p)], vec![moved(p), button(p, true)]];
    for i in 1..=6 {
        events.push(vec![moved(p + (to - p) * (i as f32 / 6.0))]);
    }
    events.push(vec![button(to, false)]);
    for e in events {
        run(&c, &mut app, size, e, true);
    }
    assert_eq!(
        cam(&mut app),
        before,
        "Ziehen auf dem Befehlsfeld dreht nicht"
    );
    // Auch ein Klick auf eine freie Stelle des Felds (Rand) wählt nichts.
    let (shot, _) = run(&c, &mut app, size, vec![], true);
    let f = fields_of(&shot, true, size);
    let bottom = f.bottom.expect("Befehlsfeld");
    let edge = pos2(bottom.left() + 3.0, bottom.center().y);
    click(&c, &mut app, size, edge, true);
    assert_eq!(app.ws_mut().active_tab_mut().state.selection, None);
    assert_eq!(cam(&mut app), before);
}

// ---- Fix-Runde 1 ------------------------------------------------------------------------------

// Im Skizzenmodus gibt es das Befehlsfeld nicht mehr (Etappe C, Aufgabe 3); dass das Menü dort
// die Skizze beendet und die Dialoge öffnet, prüft tests/sketch_fields.rs
// (`menu_still_ends_the_sketch_and_opens_dialogs`).

/// Fläche des Chips `label` im letzten Bild.
fn chip_fill_at(shot: &Shot, rect: Rect) -> Color32 {
    let near = |a: Rect, b: Rect| {
        (a.min - b.min).abs().max_elem() < 0.6 && (a.max - b.max).abs().max_elem() < 0.6
    };
    shot.fills
        .iter()
        .find(|f| near(f.rect, rect))
        .unwrap_or_else(|| panic!("keine Fläche in {rect:?}"))
        .fill
}

#[test]
fn chips_show_hover_and_pressed_feedback_from_the_palette() {
    for dark in [true, false] {
        let c = context(dark);
        let mut app = app_with(2, false);
        app.ws_mut().active_tab_mut().state.dark = dark;
        let p = theme::palette(dark);
        let (rest, _) = settle(&c, &mut app, SIZE);
        let rect = shell_panels::bottom_buttons(&c)
            .into_iter()
            .find(|(l, _)| l == "Abziehen")
            .unwrap()
            .1;
        let (fill, ink) = p.chip_split;
        assert_eq!(chip_fill_at(&rest, rect), fill, "Ruhe");
        let at = rect.center();
        let (hover, _) = run(&c, &mut app, SIZE, vec![moved(at)], false);
        let hovered = chip_fill_at(&hover, rect);
        assert_eq!(hovered, theme::chip_fill(fill, ink, true, false));
        assert_ne!(hovered, fill, "Überfahren ändert die Fläche");
        let (down, _) = run(&c, &mut app, SIZE, vec![moved(at), button(at, true)], false);
        let pressed = chip_fill_at(&down, rect);
        assert_eq!(pressed, theme::chip_fill(fill, ink, true, true));
        assert!(
            pressed != hovered && pressed != fill,
            "Drücken ist ein dritter Zustand"
        );
    }
}

/// Bei 1100 × 700 sind alle Knöpfe sichtbar; bei 800 × 600 lassen sich die hinteren per Mausrad
/// in die Sicht rollen.
#[test]
fn every_button_is_reachable_at_narrow_widths() {
    let size = vec2(1100.0, 700.0);
    let c = context(true);
    let mut app = app_with(2, true);
    let (shot, _) = settle(&c, &mut app, size);
    let bottom = fields_of(&shot, true, size).bottom.expect("Befehlsfeld");
    for (label, r) in shell_panels::bottom_buttons(&c) {
        assert!(
            bottom.contains_rect(r),
            "1100: {label}: {r:?} in {bottom:?}"
        );
    }

    let size = vec2(800.0, 600.0);
    let c = context(true);
    let mut app = app_with(2, true);
    let (shot, _) = settle(&c, &mut app, size);
    let bottom = shot
        .fields(true)
        .into_iter()
        .find(|r| r.height() == shell::BOTTOM_H && r.bottom() > size.y - 20.0 && r.left() > 100.0)
        .expect("Befehlsfeld");
    let last = |c: &egui::Context| {
        shell_panels::bottom_buttons(c)
            .into_iter()
            .find(|(l, _)| l == "Fase …")
            .unwrap()
            .1
    };
    assert!(
        !bottom.contains_rect(last(&c)),
        "800: Fase zunächst verdeckt"
    );
    let at = bottom.center();
    run(&c, &mut app, size, vec![moved(at)], false);
    for _ in 0..30 {
        run(
            &c,
            &mut app,
            size,
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(-40.0, 0.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            }],
            false,
        );
    }
    for _ in 0..10 {
        run(&c, &mut app, size, vec![], false);
    }
    assert!(
        bottom.contains_rect(last(&c)),
        "800: Fase nach dem Rollen {:?}",
        last(&c)
    );
}

/// Gegenprobe zum Routing: dieselbe Klick- und Ziehgeste in der freien Mitte wählt und dreht, auf
/// dem Befehlsfeld nicht (der vom ersten Klick geöffnete Dialog wird vor dem Ziehen geschlossen).
#[test]
fn drag_from_a_button_does_not_rotate_but_the_same_drag_in_free_does() {
    let size = vec2(1400.0, 900.0);
    let c = context(true);
    let mut app = app_with(1, false);
    run(&c, &mut app, size, vec![], true);
    {
        let state = &mut app.ws_mut().active_tab_mut().state;
        state.frame_all();
        state.camera.distance *= 0.25;
    }
    for _ in 0..3 {
        run(&c, &mut app, size, vec![], true);
    }
    let cam = |app: &mut PlanoraApp| {
        let s = &app.ws_mut().active_tab_mut().state;
        (s.camera.yaw, s.camera.pitch)
    };
    let drag = |c: &egui::Context, app: &mut PlanoraApp, from: Pos2| {
        let to = from + vec2(80.0, 30.0);
        let mut events = vec![vec![moved(from)], vec![moved(from), button(from, true)]];
        for i in 1..=6 {
            events.push(vec![moved(from + (to - from) * (i as f32 / 6.0))]);
        }
        events.push(vec![button(to, false)]);
        for e in events {
            run(c, app, size, e, true);
        }
    };
    let before = cam(&mut app);
    // Klick auf einen Knopf öffnet den Dialog; er wird geschlossen, bevor gezogen wird.
    let p = pos_of(&c, &mut app, size, "Kegel …");
    click(&c, &mut app, size, p, true);
    assert!(app.ws_mut().active_tab_mut().dialog.is_some());
    app.ws_mut().active_tab_mut().dialog = None;
    for _ in 0..3 {
        run(&c, &mut app, size, vec![], true);
    }
    let from = pos_of(&c, &mut app, size, "Kegel …");
    drag(&c, &mut app, from);
    assert_eq!(cam(&mut app), before, "Ziehen vom Knopf dreht nicht");
    // Positive Kontrolle: in der freien Mitte wählt der Klick und das Ziehen dreht.
    let (_, free) = run(&c, &mut app, size, vec![], true);
    let mid = free.center();
    click(&c, &mut app, size, mid, true);
    assert!(
        app.ws_mut().active_tab_mut().state.selection.is_some(),
        "Klick in free wählt"
    );
    drag(&c, &mut app, mid);
    assert_ne!(cam(&mut app), before, "Ziehen in free dreht");
}
