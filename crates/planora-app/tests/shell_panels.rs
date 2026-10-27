//! Schwebende Felder (Graphit, Etappe B, Aufgabe 4): oben Menü und Tabs, links Projekte und
//! Feature-Baum, rechts Eigenschaften | Versionen, links unten der Status und bei alten Versionen
//! der Hinweis. Geprüft werden Lage (im Fenster, ohne Überlappung, ohne Sprung im ersten Bild),
//! Inhalte, Farben, Rollen und dass Zeigereingaben auf einem echten Feld den 3D-Bereich nicht
//! erreichen. Ohne Fenster: Rohereignisse je Bild gegen einen egui-Kontext.

mod common;

use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::shell::{self, SHELL_INSET};
use planora_app::sketch_state::Tool;
use planora_app::state::StandardPlane;
use planora_app::workspace::Workspace;
use planora_app::{
    fonts, shell_panels, sketch_panel, sketch_view, theme, versions_panel, viewport,
};
use planora_feature::SketchPlane;
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
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(size: Vec2, events: Vec<Event>) -> egui::RawInput {
    // Kurze Bildabstände: Klicks gelten nicht als langes Halten.
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
    let root = common::temp_root("planora-shell");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// App mit einem Quader im aktiven Tab.
fn app_with_box() -> PlanoraApp {
    let mut app = PlanoraApp::for_tests(library_ws());
    app.ws_mut()
        .active_tab_mut()
        .state
        .create_box(DVec3::splat(100.0))
        .unwrap();
    app
}

/// Ein gezeichneter Text: Inhalt, Lage, Farbe.
#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
    /// Mit „…“ gekürzt.
    elided: bool,
    rows: usize,
}

/// Eine Fläche mit Rand.
#[derive(Debug, Clone, Copy)]
struct Fill {
    rect: Rect,
    fill: Color32,
    stroke: egui::Stroke,
    radius: egui::CornerRadius,
    blur: f32,
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
            blur: r.blur_width,
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

    /// Texte innerhalb eines Rechtecks.
    fn texts_in(&self, r: Rect) -> Vec<&Seen> {
        self.texts
            .iter()
            .filter(|t| r.expand(0.5).contains(t.rect.center()))
            .collect()
    }

    fn has_in(&self, r: Rect, exact: &str) -> bool {
        self.texts_in(r).iter().any(|t| t.text == exact)
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

/// Ein Bild: Felder (und auf Wunsch der 3D-Bereich in der freien Fläche darunter).
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

/// Mehrere Bilder ohne Eingaben; liefert das letzte.
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

fn wheel(dy: f32) -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
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

/// Die vier Felder eines Bildes nach Lage: oben, links, rechts, Status.
struct Fields {
    top: Rect,
    left: Rect,
    right: Rect,
    status: Rect,
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

/// Unter dieser Fensterbreite passt das obere Feld (Menü bis „Hilfe“, Knöpfe, ein Tab) nicht
/// zwischen die Spalten; sie beginnen dann unter ihm (gemessen: ab 1120,7 px passt es).
const COLUMNS_BELOW_TOP_UNDER: f32 = 1121.0;

fn fields_of(shot: &Shot, dark: bool, size: Vec2) -> Fields {
    let all = shot.fields(dark);
    let pick = |what: &str, f: &dyn Fn(&Rect) -> bool| -> Rect {
        let hits: Vec<_> = all.iter().filter(|r| f(r)).collect();
        assert_eq!(hits.len(), 1, "{what}: {hits:?} in {all:?}");
        *hits[0]
    };
    let w = size.x;
    // Die Spalten beginnen oben; nur in Fenstern schmaler als [`COLUMNS_BELOW_TOP_UNDER`] (dort
    // passt das obere Feld nicht zwischen sie) beginnen sie unter dem oberen Feld.
    let col_top = |y: f32| {
        if w < COLUMNS_BELOW_TOP_UNDER {
            near(y, SHELL_INSET + shell::TOP_H + shell::GAP)
        } else {
            near(y, SHELL_INSET)
        }
    };
    Fields {
        top: pick("oberes Feld", &|r| {
            near(r.top(), SHELL_INSET) && near(r.center().x, w / 2.0)
        }),
        // Im Skizzenmodus steht links die Werkzeugleiste.
        left: pick("linkes Feld", &|r| {
            near(r.left(), SHELL_INSET)
                && r.width() <= theme::PANEL_WIDTH + 0.5
                && (col_top(r.top()) || near(r.width(), shell::STRIP_W))
        }),
        right: pick("rechtes Feld", &|r| {
            near(r.right(), w - SHELL_INSET)
                && r.width() <= theme::PANEL_WIDTH + 0.5
                && col_top(r.top())
        }),
        status: pick("Statusfeld", &|r| {
            near(r.left(), SHELL_INSET) && near(r.bottom(), size.y - SHELL_INSET)
        }),
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 0.5 && i.height() > 0.5
}

// ---------------------------------------------------------------------------------------------

#[test]
fn fields_draw_inside_the_window_at_1400x900_and_1100x700() {
    for size in [vec2(1400.0, 900.0), vec2(1100.0, 700.0)] {
        let c = context(true);
        let mut app = app_with_box();
        let (shot, free) = settle(&c, &mut app, size);
        let content = Rect::from_min_size(Pos2::ZERO, size);
        let all = shot.fields(true);
        // Fünf Felder und die Verlaufsleiste über dem Befehlsfeld.
        assert_eq!(all.len(), 6, "{size:?}: sechs Felder: {all:?}");
        let f = fields_of(&shot, true, size);
        assert!(near(f.left.width(), theme::PANEL_WIDTH), "{:?}", f.left);
        assert!(near(f.right.width(), theme::PANEL_WIDTH), "{:?}", f.right);
        assert!(near(f.top.height(), shell::TOP_H), "{:?}", f.top);
        assert!(near(f.status.height(), shell::STATUS_H), "{:?}", f.status);
        for (i, a) in all.iter().enumerate() {
            assert!(content.contains_rect(*a), "{size:?}: {a:?} außerhalb");
            assert!(
                !overlaps(*a, free),
                "{size:?}: {a:?} über der freien Fläche {free:?}"
            );
            for b in &all[i + 1..] {
                assert!(!overlaps(*a, *b), "{size:?}: {a:?} überlappt {b:?}");
            }
        }
    }
}

#[test]
fn top_field_contains_menu_and_tabs() {
    let c = context(true);
    let mut app = app_with_box();
    app.ws_mut().new_tab();
    let titles: Vec<String> = app.ws_mut().tabs.iter().map(|t| t.title()).collect();
    let size = vec2(1400.0, 900.0);
    let (shot, _) = settle(&c, &mut app, size);
    let top = fields_of(&shot, true, size).top;
    for must in ["Datei", "Erzeugen", "Zeichnung", "Bearbeiten", "Ansicht"] {
        shot.find(top, must);
    }
    // Tab-Köpfe („Name“, geänderte mit „ ●“ dahinter).
    let tab_head = |title: &str| {
        shot.texts_in(top)
            .into_iter()
            .find(|t| t.text == title || t.text == format!("{title} ●"))
            .unwrap_or_else(|| panic!("Tab „{title}“ fehlt"))
            .rect
    };
    for t in &titles {
        tab_head(t);
    }
    // Menü und Tabs stehen in einer Zeile, die Tabs rechts vom Menü.
    let datei = shot.find(top, "Datei").rect;
    let tab = tab_head(&titles[0]);
    assert!((datei.center().y - tab.center().y).abs() < 4.0);
    assert!(tab.left() > shot.find(top, "Ansicht").rect.right());

    // Viele Tabs: das Feld wird nicht breiter als der Platz zwischen den Spalten, die Tabs rollen.
    for _ in 0..25 {
        app.ws_mut().new_tab();
    }
    let (shot, free) = settle(&c, &mut app, size);
    let f = fields_of(&shot, true, size);
    let between = size.x - 2.0 * (SHELL_INSET + theme::PANEL_WIDTH + shell::GAP);
    assert!(near(f.top.width(), between), "{:?}", f.top);
    assert!(
        near(f.top.height(), shell::TOP_H),
        "eine Zeile: {:?}",
        f.top
    );
    for side in [f.left, f.right] {
        assert!(!overlaps(f.top, side), "{:?} über {side:?}", f.top);
    }
    assert!(!overlaps(f.top, free));
}

#[test]
fn view_menu_toggles_versions_in_the_shell() {
    let c = context(true);
    let mut app = app_with_box();
    let size = vec2(1400.0, 900.0);
    let (shot, _) = settle(&c, &mut app, size);
    let top = fields_of(&shot, true, size).top;
    let ansicht = shot.find(top, "Ansicht").rect.center();
    assert!(
        app.shell_mut().versions_visible,
        "Versionen anfangs sichtbar"
    );
    click(&c, &mut app, size, ansicht, false);
    let (shot, _) = settle(&c, &mut app, size);
    // Das Menü hat keinen Eintrag „Projekte“ mehr (das Feld gibt es nicht).
    assert!(
        !shot
            .texts
            .iter()
            .any(|t| t.text == "Projekte" && t.rect.top() > top.bottom()),
        "kein Eintrag Projekte"
    );
    // Eintrag im aufgeklappten Menü (unter dem oberen Feld).
    let entry = shot
        .texts
        .iter()
        .find(|t| t.text == "Versionen" && t.rect.top() > top.bottom())
        .unwrap_or_else(|| panic!("Menüeintrag „Versionen“ fehlt"))
        .rect
        .center();
    click(&c, &mut app, size, entry, false);
    assert!(!app.shell_mut().versions_visible, "Versionen ausgeblendet");
    // Menü schließen.
    run(
        &c,
        &mut app,
        size,
        vec![Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        false,
    );
    settle(&c, &mut app, size);
}

#[test]
fn left_field_shows_only_the_tree() {
    let c = context(true);
    let mut app = app_with_box();
    let size = vec2(1400.0, 900.0);
    let (shot, _) = settle(&c, &mut app, size);
    let left = fields_of(&shot, true, size).left;
    let texts = shot.texts_in(left);
    assert!(!texts.iter().any(|t| t.text == "Projekte"), "{texts:?}");
    assert!(!texts.iter().any(|t| t.text.contains("Noch keine Projekte")));
    // Der Baum steht oben im Feld (kein Abschnitt mehr darüber).
    let tree = shot.find(left, "Feature-Baum").rect;
    assert!(
        tree.top() < left.top() + 40.0,
        "Baum oben im Feld: {tree:?} in {left:?}"
    );
    // Ein langer Baum bekommt die ganze Spalte (den Platz des früheren Felds Projekte).
    for i in 0..40 {
        app.ws_mut()
            .active_tab_mut()
            .state
            .create_box(DVec3::splat(5.0 + i as f64))
            .unwrap();
    }
    let (shot, _) = settle(&c, &mut app, size);
    let column = shell_panels::last_layout(&c).unwrap().left;
    let left = fields_of(&shot, true, size).left;
    assert!(
        (left.height() - column.height()).abs() < 1.0,
        "Feld {left:?} füllt die Spalte {column:?}"
    );
}

#[test]
fn right_field_switches_between_properties_and_versions() {
    let c = context(true);
    let mut app = app_with_box();
    let size = vec2(1400.0, 900.0);
    let unsaved = "Dieses Dokument ist noch nicht gespeichert.";
    let (shot, _) = settle(&c, &mut app, size);
    let right = fields_of(&shot, true, size).right;
    assert!(!shot.has_in(right, unsaved));
    let versions = shot.find(right, "Versionen").rect.center();
    click(&c, &mut app, size, versions, false);
    let (shot, _) = settle(&c, &mut app, size);
    let right = fields_of(&shot, true, size).right;
    assert!(shot.has_in(right, unsaved), "Versionsliste nach Klick");

    let props = shot.find(right, "Eigenschaften").rect.center();
    click(&c, &mut app, size, props, false);
    let (shot, _) = settle(&c, &mut app, size);
    let right = fields_of(&shot, true, size).right;
    assert!(!shot.has_in(right, unsaved), "zurück zu Eigenschaften");

    // Ohne Versionen (Ansicht-Menü) gibt es keinen Umschalter, nur Eigenschaften.
    app.shell_mut().right_tab = shell::RightTab::Versions;
    app.shell_mut().versions_visible = false;
    let (shot, _) = settle(&c, &mut app, size);
    let right = fields_of(&shot, true, size).right;
    assert!(!shot.has_in(right, "Versionen"));
    assert!(!shot.has_in(right, unsaved));
    shot.find(right, "Eigenschaften");
}

/// Arbeitsbereich mit gespeichertem Dokument (zwei Versionen); aktiv ist die Ansicht von v1.
fn read_only_ws() -> Workspace {
    let mut w = library_ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(3.0))
        .unwrap();
    w.save_active("Stand 2", None).unwrap();
    versions_panel::open_version(&mut w, 1);
    assert!(w.active_tab().read_only);
    w
}

#[test]
fn read_only_banner_text_and_colour() {
    let text = "Alte Version v1 – schreibgeschützt. Im Bereich „Versionen“ lässt sie sich \
                wiederherstellen oder abzweigen.";
    for (dark, size, rows) in [
        (true, vec2(1400.0, 900.0), 1),
        (false, vec2(1400.0, 900.0), 1),
        (true, vec2(1100.0, 700.0), 2),
    ] {
        let c = context(dark);
        let mut app = PlanoraApp::for_tests(read_only_ws());
        app.ws_mut().set_dark(dark);
        let (shot, free) = settle(&c, &mut app, size);
        let f = fields_of(&shot, dark, size);
        let all = shot.fields(dark);
        assert_eq!(all.len(), 7, "mit Hinweisfeld und Verlaufsleiste: {all:?}");
        // Unter dem oberen Feld (stehen die Spalten darunter, beginnen sie auf gleicher Höhe).
        let banner = *all
            .iter()
            .find(|r| near(r.top(), f.top.bottom() + shell::GAP) && **r != f.left && **r != f.right)
            .expect("Hinweisfeld unter dem oberen Feld");
        // Ganze Breite zwischen den Spalten.
        assert!(
            near(banner.left(), f.left.right() + shell::GAP),
            "{banner:?}"
        );
        assert!(
            near(banner.right(), f.right.left() - shell::GAP),
            "{banner:?}"
        );
        if rows == 1 {
            assert!(near(banner.height(), shell::BANNER_LINE_H), "{banner:?}");
        }
        assert!(banner.height() <= shell::BANNER_H + 0.5, "{banner:?}");
        // Der ganze Text ist zu sehen (ungekürzt), bei 1100 px in zwei Zeilen.
        let seen = shot.find(banner, text);
        assert!(!seen.elided, "{size:?}: Hinweis gekürzt");
        assert_eq!(seen.rows, rows, "{size:?}");
        assert!(banner.contains_rect(seen.rect), "{seen:?} in {banner:?}");
        assert_eq!(seen.color, theme::palette(dark).warning);
        assert!(!overlaps(banner, free), "{banner:?} über {free:?}");
        for other in [f.left, f.right, f.top] {
            assert!(!overlaps(banner, other));
        }
        // Bei ausgeblendeten Versionen bleibt der Hinweis (Versionen über das Menü Ansicht).
        app.shell_mut().set_versions_visible(false);
        let (shot, _) = settle(&c, &mut app, size);
        assert!(shot.texts.iter().any(|t| t.text == text));
    }
}

/// Der Text im Statusfeld bei `size`. Er ist ungekürzt, hat höchstens zwei Zeilen und das Feld
/// überdeckt weder die freie Fläche noch die anderen Felder.
fn status_at(app: &mut PlanoraApp, size: Vec2) -> Seen {
    let c = context(true);
    let (shot, free) = settle(&c, app, size);
    let f = fields_of(&shot, true, size);
    let t = shot.texts_in(f.status);
    assert_eq!(t.len(), 1, "{t:?}");
    let seen = t[0].clone();
    assert!(!seen.elided, "{size:?}: gekürzt: {}", seen.text);
    assert!(seen.rows <= 3, "{size:?}: {} Zeilen", seen.rows);
    assert!(
        f.status.contains_rect(seen.rect),
        "{seen:?} in {:?}",
        f.status
    );
    assert!(!overlaps(f.status, free), "{:?} über {free:?}", f.status);
    for other in [f.left, f.right, f.top] {
        assert!(!overlaps(f.status, other), "{:?} / {other:?}", f.status);
    }
    seen
}

/// Text und Farbe im Statusfeld bei 1400 × 900 und 1100 × 700 (an beiden gleich).
fn status(app: &mut PlanoraApp) -> (String, Color32) {
    let a = status_at(app, vec2(1400.0, 900.0));
    let b = status_at(app, vec2(1100.0, 700.0));
    assert_eq!((&a.text, a.color), (&b.text, b.color));
    (a.text, a.color)
}

#[test]
fn status_field_shows_message_with_error_colour_and_counts_bodies() {
    let p = theme::palette(true);
    // Körperzahl: eine Zeile, Feld 32 hoch.
    let mut app = app_with_box();
    assert_eq!(status(&mut app), ("1 Körper".into(), p.text_secondary));
    assert_eq!(status_at(&mut app, vec2(1400.0, 900.0)).rows, 1);

    // Fehler im Baum.
    let state = &mut app.ws_mut().active_tab_mut().state;
    state
        .add_feature(
            None,
            planora_feature::FeatureKind::Cylinder {
                radius: -1.0,
                height: 2.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    // Die Meldung des Fehlschlags ginge vor; ohne sie zeigt der Status den Fehler im Baum.
    state.message = None;
    let (text, color) = status(&mut app);
    assert!(text.contains("Zylinder 1"), "{text}");
    assert_eq!(color, p.error);

    // Skizzen-Modus geht vor.
    let state = &mut app.ws_mut().active_tab_mut().state;
    let id = state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    state.open_sketch(id).unwrap();
    // Kurz; die Tasten stehen im Hover der Statuszeile.
    let (text, color) = status(&mut app);
    assert_eq!(text, "Skizzen-Modus: Skizze 1");
    assert_eq!(color, p.text_secondary);

    // Eine Meldung geht allem vor, in der Fehlerfarbe.
    app.ws_mut().active_tab_mut().state.message = Some("Etwas ging schief".into());
    assert_eq!(status(&mut app), ("Etwas ging schief".into(), p.error));

    // Auch eine lange Meldung (150 Zeichen) ist ganz zu lesen, in höchstens zwei Zeilen.
    let long = "Erzeugen fehlgeschlagen: Die Fläche liegt nicht in einer Ebene, deshalb lässt \
                sich darauf keine Skizze anlegen. Bitte zuerst eine ebene Fläche des Körpers \
                wählen.";
    assert!(long.chars().count() >= 150, "{}", long.chars().count());
    app.ws_mut().active_tab_mut().state.message = Some(long.into());
    assert_eq!(status(&mut app), (long.into(), p.error));
    assert_eq!(status_at(&mut app, vec2(1100.0, 700.0)).rows, 3);
}

#[test]
fn status_beyond_two_lines_is_shortened_and_stays_clear_of_the_free_rect() {
    let mut app = app_with_box();
    let huge = "Sehr lange Meldung. ".repeat(40);
    app.ws_mut().active_tab_mut().state.message = Some(huge.clone());
    let c = context(true);
    let size = vec2(1100.0, 700.0);
    let (shot, free) = settle(&c, &mut app, size);
    let f = fields_of(&shot, true, size);
    let seen = shot.texts_in(f.status)[0].clone();
    assert!(seen.elided, "gekürzt");
    assert_eq!(seen.rows, 3);
    assert!(!overlaps(f.status, free));
    assert!(!overlaps(f.status, f.left));
}

#[test]
fn first_frame_has_no_jump() {
    for read_only in [false, true] {
        let c = context(true);
        let mut app = if read_only {
            PlanoraApp::for_tests(read_only_ws())
        } else {
            app_with_box()
        };
        let size = vec2(1400.0, 900.0);
        let (first, _) = run(&c, &mut app, size, vec![], false);
        let (second, _) = run(&c, &mut app, size, vec![], false);
        let (a, b) = (first.fields(true), second.fields(true));
        assert_eq!(a.len(), if read_only { 7 } else { 6 }, "erstes Bild: {a:?}");
        assert_eq!(a, b, "erstes und zweites Bild gleich");
        // Auch die Texte stehen schon im ersten Bild an ihrem Platz.
        let datei = |s: &Shot| s.texts.iter().find(|t| t.text == "Datei").map(|t| t.rect);
        assert_eq!(datei(&first), datei(&second));
        assert!(datei(&first).is_some());

        // Ein neuer Tab verbreitert das obere Feld schon im selben Bild (kein Bild mit alter
        // Breite und rollender Tab-Leiste).
        app.ws_mut().new_tab();
        let (changed, _) = run(&c, &mut app, size, vec![], false);
        let (next, _) = run(&c, &mut app, size, vec![], false);
        let top = |s: &Shot| fields_of(s, true, size).top;
        assert_eq!(top(&changed), top(&next));
        assert!(top(&changed).width() > top(&second).width());
    }
}

#[test]
fn side_columns_scroll_when_content_is_taller_than_the_window() {
    let c = context(true);
    let mut app = PlanoraApp::for_tests(library_ws());
    for i in 0..40 {
        app.ws_mut()
            .active_tab_mut()
            .state
            .create_box(DVec3::splat(5.0 + i as f64))
            .unwrap();
    }
    let size = vec2(1100.0, 500.0);
    let rects = shell::layout(
        Rect::from_min_size(Pos2::ZERO, size),
        &shell::ShellState::default(),
        600.0,
        0.0,
        100.0,
        false,
    );
    let (shot, _) = settle(&c, &mut app, size);
    let f = fields_of(&shot, true, size);
    assert!(
        f.left.height() <= rects.left.height() + 0.5,
        "{:?} höher als {:?}",
        f.left,
        rects.left
    );
    assert!(near(f.left.width(), theme::PANEL_WIDTH));
    // Der Inhalt ist länger als das Feld …
    let tree = shot.find(f.left, "Feature-Baum").rect;
    // … und rollt mit dem Mausrad.
    let p = f.left.center();
    run(&c, &mut app, size, vec![moved(p)], false);
    run(&c, &mut app, size, vec![moved(p), wheel(-400.0)], false);
    let mut shot = Shot::default();
    for _ in 0..30 {
        shot = run(&c, &mut app, size, vec![], false).0;
    }
    let f2 = fields_of(&shot, true, size);
    assert_eq!(f2.left, f.left, "Feld bleibt gleich groß");
    let after = shot
        .texts
        .iter()
        .find(|t| t.text == "Feature-Baum")
        .map(|t| t.rect);
    assert!(
        after.is_none_or(|r| r.top() < tree.top() - 50.0),
        "Inhalt gerollt: {tree:?} → {after:?}"
    );
}

#[test]
fn dark_and_light_frames() {
    for dark in [true, false] {
        let c = context(dark);
        let mut app = app_with_box();
        app.ws_mut().set_dark(dark);
        let size = vec2(1400.0, 900.0);
        let (shot, _) = settle(&c, &mut app, size);
        assert_eq!(shot.fields(dark).len(), 6, "dunkel {dark}");
        // Das andere Thema kommt nicht vor.
        assert!(shot.fields(!dark).is_empty());
        assert!(shot.fills.iter().all(|f| f.blur == 0.0), "kein Schatten");
        let p = theme::palette(dark);
        for f in shot
            .fills
            .iter()
            .filter(|f| f.radius == egui::CornerRadius::same(theme::RADIUS_LG))
        {
            assert_eq!(f.stroke.color, p.border_subtle, "{f:?}");
            assert!(
                f.fill == p.bg_panel || f.fill == tree_fill(dark),
                "kein Verlauf, Fläche bg_panel: {f:?}"
            );
        }
        // Nur der Feature-Baum ist durchsichtig, und zwar das linke Feld.
        let translucent: Vec<_> = shot
            .fills
            .iter()
            .filter(|f| {
                f.fill == tree_fill(dark) && f.radius == egui::CornerRadius::same(theme::RADIUS_LG)
            })
            .collect();
        assert_eq!(translucent.len(), 1, "dunkel {dark}: {translucent:?}");
        assert_eq!(
            translucent[0].rect,
            fields_of(&shot, dark, size).left,
            "dunkel {dark}"
        );
    }
}

// ---- Zeigerrouting über den echten Feldern ------------------------------------------------------

/// App mit einem Quader, der das ganze Fenster füllt (Kamera nah), damit unter jedem Feld Körper
/// liegt. Liefert die Felder.
fn routing_app(c: &egui::Context, size: Vec2) -> (PlanoraApp, Fields) {
    let mut app = app_with_box();
    run(c, &mut app, size, vec![], true);
    let state = &mut app.ws_mut().active_tab_mut().state;
    state.frame_all();
    state.camera.distance *= 0.25;
    let (shot, _) = run(c, &mut app, size, vec![], true);
    let f = fields_of(&shot, true, size);
    (app, f)
}

/// Geklickt und gezogen wird knapp unter der Oberkante jedes Felds (dort liegen nur Kopf,
/// Beschriftung oder Rand, kein Knopf).
#[test]
fn clicks_and_drags_on_a_real_field_do_not_reach_the_viewport() {
    let size = vec2(1100.0, 700.0);
    let c = context(true);
    let (mut app, f) = routing_app(&c, size);
    let cam = |app: &mut PlanoraApp| {
        let s = &app.ws_mut().active_tab_mut().state;
        (s.camera.yaw, s.camera.pitch)
    };
    let before = cam(&mut app);
    for (what, field) in [("rechts", f.right), ("Status", f.status), ("links", f.left)] {
        let p = pos2(field.center().x, field.top() + 6.0);
        // Das Feld nimmt Zeigereingaben an (interaktive Ebene `Middle`).
        let layer = c.layer_id_at(p).expect("Ebene unter dem Zeiger");
        assert_eq!(layer.order, egui::Order::Middle, "{what}");
        click(&c, &mut app, size, p, true);
        assert_eq!(
            app.ws_mut().active_tab_mut().state.selection,
            None,
            "Klick {what} wählt nichts"
        );
        let to = p + vec2(80.0, 0.0);
        let mut events = vec![vec![moved(p)], vec![moved(p), button(p, true)]];
        for i in 1..=6 {
            events.push(vec![moved(p + (to - p) * (i as f32 / 6.0))]);
        }
        events.push(vec![button(to, false)]);
        for e in events {
            run(&c, &mut app, size, e, true);
        }
        assert_eq!(cam(&mut app), before, "Ziehen {what} dreht nicht");
    }
    // Gegenprobe: in der freien Mitte wählt der Klick und Ziehen dreht.
    let mid = pos2(size.x / 2.0, size.y / 2.0);
    click(&c, &mut app, size, mid, true);
    assert!(app.ws_mut().active_tab_mut().state.selection.is_some());
    let to = mid + vec2(80.0, 30.0);
    let mut events = vec![vec![moved(mid)], vec![moved(mid), button(mid, true)]];
    for i in 1..=6 {
        events.push(vec![moved(mid + (to - mid) * (i as f32 / 6.0))]);
    }
    events.push(vec![button(to, false)]);
    for e in events {
        run(&c, &mut app, size, e, true);
    }
    assert_ne!(cam(&mut app), before, "Orbit in der freien Mitte");
}

#[test]
fn mouse_wheel_on_a_real_field_does_not_zoom() {
    let size = vec2(1100.0, 700.0);
    let scroll_at = |pick: &dyn Fn(&Fields) -> Pos2| {
        let c = context(true);
        let (mut app, f) = routing_app(&c, size);
        let p = pick(&f);
        let before = app.ws_mut().active_tab_mut().state.camera.distance;
        run(&c, &mut app, size, vec![moved(p)], true);
        run(&c, &mut app, size, vec![moved(p), wheel(120.0)], true);
        for _ in 0..20 {
            run(&c, &mut app, size, vec![], true);
        }
        (before, app.ws_mut().active_tab_mut().state.camera.distance)
    };
    for (what, pick) in [
        (
            "oben",
            &(|f: &Fields| f.top.center()) as &dyn Fn(&Fields) -> Pos2,
        ),
        ("links", &|f: &Fields| f.left.center()),
        ("rechts", &|f: &Fields| f.right.center()),
        ("Status", &|f: &Fields| f.status.center()),
    ] {
        let (before, after) = scroll_at(pick);
        assert_eq!(after, before, "Mausrad {what} zoomt nicht");
    }
    let (before, after) = scroll_at(&|_| pos2(size.x / 2.0, size.y / 2.0));
    assert!(
        after < before,
        "Mausrad in der Mitte zoomt: {before} → {after}"
    );
}

// ---- Fix-Runde 1 ---------------------------------------------------------------------------------

fn count_in(shot: &Shot, r: Rect, exact: &str) -> usize {
    shot.texts_in(r).iter().filter(|t| t.text == exact).count()
}

#[test]
fn the_switch_replaces_the_headings_of_properties_and_versions() {
    let c = context(true);
    // Gespeichertes Dokument: der Versionsbereich zeigt die Liste (sonst nur einen Satz).
    let mut w = library_ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    let mut app = PlanoraApp::for_tests(w);
    let size = vec2(1400.0, 900.0);
    let (shot, _) = settle(&c, &mut app, size);
    let right = fields_of(&shot, true, size).right;
    assert_eq!(
        count_in(&shot, right, "Eigenschaften"),
        1,
        "nur im Umschalter"
    );
    app.shell_mut().right_tab = shell::RightTab::Versions;
    let (shot, _) = settle(&c, &mut app, size);
    let right = fields_of(&shot, true, size).right;
    assert!(
        shot.texts_in(right)
            .iter()
            .any(|t| t.text.contains("Anfang")),
        "Versionsliste"
    );
    assert_eq!(count_in(&shot, right, "Versionen"), 1, "nur im Umschalter");
    // Ohne Umschalter behält der Inhalt seinen Kopf.
    app.shell_mut().set_versions_visible(false);
    let (shot, _) = settle(&c, &mut app, size);
    let right = fields_of(&shot, true, size).right;
    assert_eq!(
        count_in(&shot, right, "Eigenschaften"),
        1,
        "Kopf ohne Umschalter"
    );
}

#[test]
fn a_denied_discard_still_asks_for_a_new_frame() {
    let c = context(true);
    // Nur ein Durchgang je Bild: jedes Verwerfen wird abgelehnt.
    c.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
    let mut app = app_with_box();
    let size = vec2(1400.0, 900.0);
    let repaint = |c: &egui::Context, app: &mut PlanoraApp| {
        let out = c.run_ui(input(size, vec![]), |ui| {
            shell_panels::show_fields(ui.ctx(), app);
        });
        let delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        out.drop_without_applying_deltas();
        delay
    };
    for _ in 0..5 {
        repaint(&c, &mut app);
    }
    assert!(
        repaint(&c, &mut app) > std::time::Duration::ZERO,
        "Ruhe ohne Änderung"
    );
    // Ein längerer Tabname ändert die Breite des oberen Felds (ohne Rollen der Tab-Leiste): das
    // Bild muss sofort wiederholt werden.
    app.ws_mut().active_tab_mut().name = "Ein deutlich längerer Name".into();
    assert_eq!(repaint(&c, &mut app), std::time::Duration::ZERO);

    // Unmittelbar: ein abgelehntes Verwerfen fordert selbst ein neues Bild an.
    let delay = |discard: bool| {
        let out = c.run_ui(input(size, vec![]), |ui| {
            if discard {
                shell_panels::discard(ui.ctx(), "Test");
            }
        });
        let d = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        out.drop_without_applying_deltas();
        d
    };
    for _ in 0..3 {
        delay(false);
    }
    assert!(delay(false) > std::time::Duration::ZERO);
    assert_eq!(delay(true), std::time::Duration::ZERO);
}

#[test]
fn the_active_tab_is_scrolled_into_view() {
    let c = context(true);
    let mut app = app_with_box();
    for _ in 0..25 {
        app.ws_mut().new_tab();
    }
    let size = vec2(1100.0, 700.0);
    let head = |shot: &Shot, title: &str| {
        shot.texts
            .iter()
            .find(|t| t.text == title || t.text == format!("{title} ●"))
            .map(|t| t.rect)
            .unwrap_or_else(|| {
                panic!(
                    "Tab „{title}“ fehlt: {:?}",
                    shot.texts
                        .iter()
                        .map(|t| (&t.text, t.rect.left()))
                        .collect::<Vec<_>>()
                )
            })
    };
    let last = app.ws_mut().active_tab().title();
    let mut shot = settle(&c, &mut app, size).0;
    for _ in 0..30 {
        shot = run(&c, &mut app, size, vec![], false).0;
    }
    let top = fields_of(&shot, true, size).top;
    assert!(
        top.contains_rect(head(&shot, &last)),
        "neuer Tab sichtbar: {top:?}"
    );
    // Zurück zum ersten Tab: die Leiste rollt nach links.
    let first = app.ws_mut().tabs[0].title();
    app.ws_mut().activate(0);
    for _ in 0..30 {
        shot = run(&c, &mut app, size, vec![], false).0;
    }
    assert!(
        top.contains_rect(head(&shot, &first)),
        "erster Tab sichtbar"
    );
    // Außerhalb des sichtbaren Bereichs wird der letzte Tab gar nicht gezeichnet.
    assert!(
        !shot
            .texts
            .iter()
            .any(|t| t.text.starts_with(&last) && top.contains_rect(t.rect)),
        "letzter Tab weggerollt"
    );
}

#[test]
fn tabs_can_be_activated_added_and_closed_in_the_top_field() {
    let c = context(true);
    // Unveränderte Tabs: Schließen braucht keine Rückfrage.
    let mut app = PlanoraApp::for_tests(library_ws());
    app.ws_mut().new_tab();
    let size = vec2(1400.0, 900.0);
    let glyph = |icon| planora_app::icons::glyph(icon).to_string();
    let (shot, _) = settle(&c, &mut app, size);
    let top = fields_of(&shot, true, size).top;
    let first = app.ws_mut().tabs[0].title();
    assert_eq!(app.ws_mut().active, 1);
    // Klick auf den ersten Tab aktiviert ihn.
    click(
        &c,
        &mut app,
        size,
        shot.find(top, &first).rect.center(),
        false,
    );
    assert_eq!(app.ws_mut().active, 0);
    // „+“ legt einen neuen an.
    let (shot, _) = settle(&c, &mut app, size);
    let plus = shot
        .find(top, &glyph(planora_app::icons::Icon::NewTab))
        .rect
        .center();
    click(&c, &mut app, size, plus, false);
    assert_eq!(app.ws_mut().tabs.len(), 3);
    // „×“ hinter dem ersten Tab schließt ihn.
    let (shot, _) = settle(&c, &mut app, size);
    let top = fields_of(&shot, true, size).top;
    let close = shot
        .texts_in(top)
        .into_iter()
        .filter(|t| t.text == glyph(planora_app::icons::Icon::Close))
        .map(|t| t.rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("Schließen-Symbol");
    click(&c, &mut app, size, close.center(), false);
    assert_eq!(app.ws_mut().tabs.len(), 2);
    assert!(app.ws_mut().tabs.iter().all(|t| t.title() != first));
}

/// Sichtbare, schmale waagerechte Flächen im oberen Feld (der Griff des Rollbalkens).
fn tab_bar_handles(shot: &Shot, top: Rect) -> Vec<Fill> {
    let bar = theme::scroll_style().bar_width;
    shot.fills
        .iter()
        .filter(|f| {
            f.fill.a() > 0
                && f.rect.height() <= bar + 0.5
                && f.rect.width() >= 16.0
                && top.contains_rect(f.rect)
        })
        .copied()
        .collect()
}

#[test]
fn the_tab_bar_scroll_bar_is_thin_hidden_at_rest_and_below_the_tab_names() {
    for dark in [true, false] {
        let c = context(dark);
        let mut app = app_with_box();
        app.ws_mut().set_dark(dark);
        for _ in 0..12 {
            app.ws_mut().new_tab();
        }
        let size = vec2(1100.0, 700.0);
        let mut shot = settle(&c, &mut app, size).0;
        for _ in 0..20 {
            shot = run(&c, &mut app, size, vec![], false).0;
        }
        let top = fields_of(&shot, dark, size).top;
        // In Ruhe: kein Balken.
        let rest = tab_bar_handles(&shot, top);
        assert!(
            rest.is_empty(),
            "dunkel {dark}: in Ruhe unsichtbar: {rest:?}"
        );
        // Zeiger über den Tabs: ein schmaler Griff in der Palettenfarbe, unter den Namen.
        let tab = shot
            .texts
            .iter()
            .find(|t| t.text.starts_with("Unbenannt") && top.contains_rect(t.rect))
            .expect("ein Tab")
            .rect;
        for _ in 0..30 {
            shot = run(&c, &mut app, size, vec![moved(tab.center())], false).0;
        }
        let handles = tab_bar_handles(&shot, top);
        assert_eq!(handles.len(), 1, "dunkel {dark}: {handles:?}");
        let h = handles[0];
        let s = theme::scroll_style();
        assert!(h.rect.height() <= s.floating_width + 0.5, "schmal: {h:?}");
        let (rest_colour, _) = theme::scroll_handle_colors(dark);
        assert_eq!(h.fill, rest_colour.gamma_multiply(s.active_handle_opacity));
        for t in shot.texts.iter().filter(|t| top.contains_rect(t.rect)) {
            assert!(
                !overlaps(t.rect, h.rect),
                "dunkel {dark}: Balken {:?} über „{}“ {:?}",
                h.rect,
                t.text,
                t.rect
            );
        }
        // Das Feld bleibt so hoch wie vorher.
        assert!(near(
            fields_of(&shot, dark, size).top.height(),
            shell::TOP_H
        ));
    }
}

/// Im Skizzenmodus ist der Status kurz; die Tasten stehen im Hover der Statuszeile und sonst
/// nirgends im Bild (der Hinweis steht höchstens einmal sichtbar).
#[test]
fn sketch_status_is_short_and_keys_are_in_the_hover() {
    let c = context(true);
    c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    let mut app = PlanoraApp::for_tests(library_ws());
    let state = &mut app.ws_mut().active_tab_mut().state;
    let id = state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    state.open_sketch(id).unwrap();
    let size = vec2(1400.0, 900.0);
    let (shot, _) = settle(&c, &mut app, size);
    let status = shell_panels::field_rects(&c)
        .into_iter()
        .find(|(n, _)| *n == "Status")
        .unwrap()
        .1;
    assert!(shot.has_in(status, "Skizzen-Modus: Skizze 1"));
    let count = |shot: &Shot| {
        shot.texts
            .iter()
            .filter(|t| t.text.contains("Esc bricht ab"))
            .count()
    };
    assert_eq!(count(&shot), 0, "ohne Hover steht der Hinweis nicht da");

    // Hover auf den Text der Statuszeile.
    let p = shot.find(status, "Skizzen-Modus: Skizze 1").rect.center();
    run(&c, &mut app, size, vec![moved(p)], false);
    let (shot, _) = run(&c, &mut app, size, vec![moved(p)], false);
    assert!(
        shot.texts
            .iter()
            .any(|t| t.text == shell_panels::sketch_keys_hint()),
        "Hover zeigt den Tastenhinweis"
    );
    assert_eq!(count(&shot), 1, "genau einmal sichtbar");

    // Hover auf den Rand des Felds (links neben dem Text) zeigt ihn ebenfalls, einmal.
    let c2 = context(true);
    c2.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    let mut app2 = PlanoraApp::for_tests(library_ws());
    let state = &mut app2.ws_mut().active_tab_mut().state;
    let id = state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    state.open_sketch(id).unwrap();
    let (_, _) = settle(&c2, &mut app2, size);
    let edge = pos2(status.left() + 3.0, status.center().y);
    run(&c2, &mut app2, size, vec![moved(edge)], false);
    let (shot, _) = run(&c2, &mut app2, size, vec![moved(edge)], false);
    assert_eq!(count(&shot), 1, "Hover am Feldrand zeigt den Hinweis");
}

/// Ein Bild wie in der App: Felder, Skizzenansicht in der freien Fläche, Tastenfokus merken.
fn sketch_frame(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, events: Vec<Event>) -> Shot {
    let out = c.run_ui(input(size, events), |ui| {
        let free = shell_panels::show_fields(ui.ctx(), app);
        sketch_view::show_overlay(ui, free, &mut app.ws_mut().active_tab_mut().state);
        viewport::remember_keyboard_focus(ui.ctx());
    });
    let shot = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    shot
}

/// Offene Punkte des laufenden Skizzenwerkzeugs.
fn pending(app: &mut PlanoraApp) -> usize {
    app.ws_mut()
        .active_tab()
        .state
        .sketch_session
        .as_ref()
        .unwrap()
        .pending
        .len()
}

/// Abschluss-Nachbesserung 4: hat die Statuszeile den Tastaturfokus (Tab), zeigt sie den
/// Fokusrahmen, und Esc bricht das laufende Skizzenwerkzeug trotzdem ab.
#[test]
fn focused_status_field_shows_a_ring_and_keeps_escape_working() {
    for dark in [false, true] {
        let c = context(dark);
        let mut app = PlanoraApp::for_tests(library_ws());
        let state = &mut app.ws_mut().active_tab_mut().state;
        let id = state
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        state.open_sketch(id).unwrap();
        {
            let (se, sk) = state.session_sketch_mut().unwrap();
            let a = sk.add_point(-10.0, -10.0).unwrap();
            se.set_tool(sk, Tool::Line);
            se.pending = vec![a];
        }
        let size = vec2(1400.0, 900.0);
        for _ in 0..3 {
            sketch_frame(&c, &mut app, size, vec![]);
        }
        let status = shell_panels::field_rects(&c)
            .into_iter()
            .find(|(n, _)| *n == "Status")
            .unwrap()
            .1;
        let ring = |shot: &Shot| {
            shot.fills.iter().any(|f| {
                f.stroke.color == theme::accent(dark)
                    && f.rect.expand(2.0).contains_rect(status)
                    && status.expand(4.0).contains_rect(f.rect)
            })
        };
        let shot = sketch_frame(&c, &mut app, size, vec![]);
        assert!(!ring(&shot), "dark={dark}: Fokusrahmen ohne Fokus");

        let sid = shell_panels::status_text_id();
        c.memory_mut(|m| m.request_focus(sid));
        let shot = sketch_frame(&c, &mut app, size, vec![]);
        assert_eq!(c.memory(|m| m.focused()), Some(sid), "dark={dark}");
        assert!(ring(&shot), "dark={dark}: kein Fokusrahmen um {status:?}");
        assert!(
            !viewport::typing(&c),
            "dark={dark}: Fokus auf der Statuszeile gilt als Eingabe"
        );
        assert_eq!(pending(&mut app), 1);
        sketch_frame(
            &c,
            &mut app,
            size,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        sketch_frame(&c, &mut app, size, vec![]);
        assert_eq!(
            pending(&mut app),
            0,
            "dark={dark}: Esc bricht bei fokussierter Statuszeile nicht ab"
        );
    }
}

/// Der Tastenhinweis steht im Namen der Statuszeile für Bildschirmleser.
#[test]
fn sketch_status_label_for_screen_readers_contains_the_hint() {
    let c = context(true);
    c.enable_accesskit();
    let mut app = PlanoraApp::for_tests(library_ws());
    let state = &mut app.ws_mut().active_tab_mut().state;
    let id = state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    state.open_sketch(id).unwrap();
    let size = vec2(1400.0, 900.0);
    let mut labels = Vec::new();
    for _ in 0..3 {
        let mut out = c.run_ui(input(size, vec![]), |ui| {
            shell_panels::show_fields(ui.ctx(), &mut app);
        });
        if let Some(u) = out.platform_output.accesskit_update.take() {
            for (_, n) in &u.nodes {
                // Beschriftungen von Textzeilen (Role::Label) meldet egui als Wert.
                for l in [n.label(), n.value()].into_iter().flatten() {
                    labels.push(l.to_string());
                }
            }
        }
        out.drop_without_applying_deltas();
    }
    let want = format!(
        "Skizzen-Modus: Skizze 1. {}",
        shell_panels::sketch_keys_hint()
    );
    assert!(labels.contains(&want), "{labels:?}");
}
