//! Befehlsfeld unten (Graphit, Etappe B, Aufgabe 5): Chips für die Booleschen Operationen und
//! Symbolknöpfe für die Erzeugen- und Bearbeiten-Aktionen des Menüs. Jede Schaltfläche muss
//! dasselbe bewirken wie ihr Menüeintrag, dieselben Sperren haben und ihre Farben aus der Palette
//! beziehen. Ohne Fenster: Rohereignisse je Bild gegen einen egui-Kontext, die Knopflagen kommen
//! aus `shell_panels::bottom_buttons`.

use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::shell::{self, SHELL_INSET};
use planora_app::state::StandardPlane;
use planora_app::workspace::{After, Prompt, Workspace};
use planora_app::{fonts, shell_panels, theme, versions_panel, viewport};
use planora_feature::SketchPlane;
use planora_kernel::BoolOp;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

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
    let root = std::env::temp_dir().join(format!("planora-cmd-{}", new_id()));
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
                f.fill == p.bg_panel
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
fn bottom_field_is_disabled_in_read_only_tabs_and_while_sketching() {
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
    // Skizzenmodus.
    let mut app = app_with(2, false);
    {
        let state = &mut app.ws_mut().active_tab_mut().state;
        let id = state
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        state.open_sketch(id).unwrap();
    }
    blocked(app, "Skizzenmodus");
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
            .any(|t| t.text == "In einer alten Version lässt sich nichts hinzufügen.")
    );
    let c = context(true);
    let mut app = app_with(2, false);
    let state = &mut app.ws_mut().active_tab_mut().state;
    let id = state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    state.open_sketch(id).unwrap();
    let p = pos_of(&c, &mut app, SIZE, "Quader …");
    let texts = hover_texts(&c, &mut app, p);
    assert!(texts.iter().any(|t| t.text == "Zuerst die Skizze beenden."));
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
        left: pick("linkes Feld", &|r| {
            near(r.left(), SHELL_INSET) && near(r.top(), SHELL_INSET)
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

/// Der Text im Statusfeld bei `size`: ungekürzt, höchstens zwei Zeilen; das Befehlsfeld (falls
/// gezeigt) steht daneben, ohne Status, freie Fläche oder andere Felder zu überdecken. Dazu, ob
/// das Befehlsfeld gezeigt wird.
fn status_at(app: &mut PlanoraApp, size: Vec2) -> (Seen, bool) {
    let c = context(true);
    let (shot, free) = settle(&c, app, size);
    let f = fields_of(&shot, true, size);
    let t = shot.texts_in(f.status);
    assert_eq!(t.len(), 1, "{t:?}");
    let seen = t[0].clone();
    assert!(!seen.elided, "{size:?}: gekürzt: {}", seen.text);
    assert!(seen.rows <= 2, "{size:?}: {} Zeilen", seen.rows);
    assert!(f.status.contains_rect(seen.rect));
    for other in [f.left, f.right, f.top, free] {
        assert!(!overlaps(f.status, other), "{:?} / {other:?}", f.status);
    }
    if let Some(bottom) = f.bottom {
        for other in [f.status, f.left, f.right, f.top, free] {
            assert!(!overlaps(bottom, other), "{bottom:?} / {other:?}");
        }
        assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(bottom));
    } else {
        assert!(shell_panels::bottom_buttons(&c).is_empty());
    }
    (seen, f.bottom.is_some())
}

/// Text, Farbe und ob das Befehlsfeld steht, bei 1400 × 900 und 1100 × 700.
fn status(app: &mut PlanoraApp) -> (String, Color32, bool, bool) {
    let (a, big) = status_at(app, vec2(1400.0, 900.0));
    let (b, small) = status_at(app, vec2(1100.0, 700.0));
    assert_eq!((&a.text, a.color), (&b.text, b.color));
    (a.text, a.color, big, small)
}

#[test]
fn status_stays_readable_with_the_real_bottom_field() {
    let p = theme::palette(true);
    let mut app = app_with(1, false);
    // Normaler Status: das Befehlsfeld steht in beiden Größen.
    assert_eq!(
        status(&mut app),
        ("1 Körper".into(), p.text_secondary, true, true)
    );

    // Skizzenhinweis mit allen Tasten.
    let state = &mut app.ws_mut().active_tab_mut().state;
    let id = state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    state.open_sketch(id).unwrap();
    let (text, color, big, small) = status(&mut app);
    assert!(big && small, "Befehlsfeld bleibt neben dem Skizzenhinweis");
    assert_eq!(
        text,
        "Skizzen-Modus: Skizze 1 – Esc bricht ab, Eingabe beendet, Entf löscht, mittlere \
         Maustaste oder Umschalt + Ziehen verschiebt"
    );
    assert_eq!(color, p.text_secondary);

    // Lange Fehlermeldung (mehr als 150 Zeichen).
    let long = "Erzeugen fehlgeschlagen: Die Fläche liegt nicht in einer Ebene, deshalb lässt \
                sich darauf keine Skizze anlegen. Bitte zuerst eine ebene Fläche des Körpers \
                wählen.";
    app.ws_mut().active_tab_mut().state.message = Some(long.into());
    // Eine lange Meldung geht vor: bei 1100 px bleibt für das Befehlsfeld kein Platz (es weicht),
    // bei 1400 px steht es schmaler daneben.
    assert_eq!(status(&mut app), (long.into(), p.error, true, false));
    // Ohne die Meldung ist das Feld wieder da.
    app.ws_mut().active_tab_mut().state.message = None;
    let (_, _, big, small) = status(&mut app);
    assert!(big && small);
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
