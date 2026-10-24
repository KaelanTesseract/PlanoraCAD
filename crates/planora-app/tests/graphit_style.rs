//! Graphit-Stil in Menüs, Tab-Leiste, Browser, Fenstern und Leisten (Etappe A, Aufgabe 5):
//! Symbole und getrennte Tastenkürzel im Menü, 24 px hohe Zeilen mit neutraler Auswahl, der
//! Graphit-Rahmen der Fenster, nichts abgeschnitten bei 1400 px und beide Themen ohne Reste
//! alter Farben.

use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, FontFamily, Pos2, Rect, vec2};
use planora_app::create_dialog::CreateDialog;
use planora_app::export_dialog::ExportDialog;
use planora_app::icons::{self, Icon};
use planora_app::shell::ShellState;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{app, browser, drawing_view, fonts, prompts, properties, sketch_panel};
use planora_app::{tabs, theme};
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::SketchPlane;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

/// Fensterbreite der Prüfung auf Abschneiden.
const WIDTH: f32 = 1400.0;
const HEIGHT: f32 = 900.0;

/// Alte Flächenfarben vor Graphit (dürfen nirgends mehr vorkommen).
const OLD_COLORS: [Color32; 2] = [
    Color32::from_rgb(0x1e, 0x21, 0x27),
    Color32::from_rgb(0x25, 0x28, 0x30),
];

/// Kontext mit Schriften; mit `Some(dark)` auch mit dem Graphit-Theme.
fn context(theme_dark: Option<bool>) -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    if let Some(dark) = theme_dark {
        theme::apply(&c, dark);
    }
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// Zeit je Bild; Klicks brauchen kurze Abstände (sonst gälte der Druck als langes Halten).
    static STEP: std::cell::Cell<f64> = const { std::cell::Cell::new(1.0) };
}

fn input(width: f32) -> egui::RawInput {
    // Jedes Bild eine Sekunde später: Einblendungen neuer Fenster sind dann abgeschlossen.
    let time = TIME.with(|t| {
        t.set(t.get() + STEP.with(|s| s.get()));
        t.get()
    });
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, HEIGHT))),
        time: Some(time),
        ..Default::default()
    }
}

fn workspace() -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-graphit-{}", new_id()));
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Ein gezeichneter Text: Inhalt, Lage, Farbe (Platzhalter aufgelöst), Familie, Ausschnitt.
#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
    family: FontFamily,
    clip: Rect,
}

/// Eine gefüllte Fläche mit Rand.
#[derive(Debug, Clone, Copy)]
struct Fill {
    rect: Rect,
    fill: Color32,
    stroke: egui::Stroke,
    radius: egui::CornerRadius,
}

fn walk(shape: &Shape, clip: Rect, texts: &mut Vec<Seen>, fills: &mut Vec<Fill>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, texts, fills)),
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |s| s.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            if let Some(c) = t.override_text_color {
                color = c;
            }
            texts.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                color,
                family: section.map_or(FontFamily::Proportional, |s| {
                    s.format.font_id.family.clone()
                }),
                clip,
            });
        }
        Shape::Rect(r) => fills.push(Fill {
            rect: r.rect,
            fill: r.fill,
            stroke: r.stroke,
            radius: r.corner_radius,
        }),
        _ => {}
    }
}

struct Shot {
    texts: Vec<Seen>,
    fills: Vec<Fill>,
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let (mut texts, mut fills) = (Vec::new(), Vec::new());
        for c in shapes {
            walk(&c.shape, c.clip_rect, &mut texts, &mut fills);
        }
        Shot { texts, fills }
    }

    fn text(&self, exact: &str) -> &Seen {
        self.texts
            .iter()
            .find(|t| t.text == exact)
            .unwrap_or_else(|| panic!("„{exact}“ fehlt in {:#?}", self.names()))
    }

    fn names(&self) -> Vec<&str> {
        self.texts.iter().map(|t| t.text.as_str()).collect()
    }
}

fn shot(c: &egui::Context, width: f32, f: impl FnMut(&mut egui::Ui)) -> Shot {
    let out = c.run_ui(input(width), f);
    let s = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    s
}

/// Zwei Texte stehen in derselben Zeile.
fn same_row(a: &Seen, b: &Seen) -> bool {
    (a.rect.center().y - b.rect.center().y).abs() < 4.0
}

// ---------------------------------------------------------------------------------------------

#[test]
fn menu_entries_have_icons_and_separate_shortcuts() {
    let c = context(Some(true));
    let mut ws = workspace();
    // Wie im Menü: Einträge untereinander über die ganze Menübreite.
    let s = shot(&c, WIDTH, |ui| {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            ui.set_width(280.0);
            theme::menu_style(ui.style_mut());
            app::file_menu(ui, &mut ws);
        });
    });
    let neu = s.text("Neu");
    let glyph = icons::glyph(Icon::New).to_string();
    let icon = s
        .texts
        .iter()
        .find(|t| t.text == glyph)
        .unwrap_or_else(|| panic!("Symbol „Neu“ fehlt: {:#?}", s.names()));
    assert_eq!(icon.family, fonts::icons(), "Symbol in der Symbolschrift");
    assert!(same_row(icon, neu) && icon.rect.right() <= neu.rect.left());
    let short = s.text("Strg+N");
    assert!(same_row(short, neu), "{short:?} / {neu:?}");
    assert!(short.rect.left() > neu.rect.right() + 20.0, "Kürzel rechts");
    // Keine aufgefüllte Spalte aus Leerzeichen, kein Kürzel im Beschriftungstext.
    for t in &s.texts {
        assert!(
            !t.text.contains("  "),
            "Leerzeichen-Kolonne in {:?}",
            t.text
        );
        assert!(
            t.text == "Strg+N" || t.text.starts_with("Strg+") || !t.text.contains("Strg"),
            "Kürzel im Text: {:?}",
            t.text
        );
    }
    // Alle Kürzel rechtsbündig.
    let rights: Vec<f32> = ["Strg+N", "Strg+O", "Strg+S", "Strg+Umschalt+E", "Strg+W"]
        .iter()
        .map(|k| s.text(k).rect.right())
        .collect();
    for r in &rights {
        assert!((r - rights[0]).abs() < 1.0, "{rights:?}");
    }
    // Jeder Eintrag hat ein Symbol links in seiner Zeile.
    for label in [
        "Projekte …",
        "Speichern",
        "Speichern unter …",
        "Exportieren …",
        "Netz importieren …",
        "STEP importieren …",
        "Schließen",
        "Einstellungen …",
        "Beenden",
    ] {
        let l = s.text(label);
        assert!(
            s.texts
                .iter()
                .any(|t| t.family == fonts::icons() && same_row(t, l)),
            "kein Symbol bei „{label}“"
        );
    }
}

#[test]
fn browser_rows_are_24_high_and_use_theme_colors() {
    for dark in [true, false] {
        let c = context(Some(dark));
        let p = theme::palette(dark);
        let mut st = AppState::new();
        st.create_box(DVec3::splat(10.0)).unwrap();
        let id = st.document.features()[0].id;
        st.selected_feature = Some(id);
        let s = shot(&c, WIDTH, |ui| browser::show(ui, &mut st));
        let rows = browser::row_rects(&c);
        assert!(rows.len() >= 2, "Feature- und Körperzeile: {rows:?}");
        for (name, r) in &rows {
            assert_eq!(r.height(), theme::ROW_H, "Zeile {name}");
        }
        let selected = rows[0].1;
        let fill = s
            .fills
            .iter()
            .find(|f| f.rect.intersects(selected) && f.fill != Color32::TRANSPARENT)
            .unwrap_or_else(|| panic!("gewählte Zeile ohne Fläche: {:?}", s.fills));
        assert_eq!(fill.fill, p.bg_active, "neutral gewählt (dunkel: {dark})");
        assert!(
            s.fills.iter().all(|f| f.fill != p.accent),
            "kein Akzent in den Zeilen"
        );
        // Gewählte Zeile in text_primary, die Körperzeile in Ruhe text_secondary.
        let names: Vec<&Seen> = s.texts.iter().filter(|t| t.text == rows[0].0).collect();
        assert_eq!(names[0].color, p.text_primary, "gewählte Zeile");
        let body = s.texts.iter().find(|t| t.text == rows[1].0).unwrap();
        let body = if names.len() > 1 { names[1] } else { body };
        assert_eq!(body.color, p.text_secondary, "Zeile in Ruhe");
        // Kopf „Feature-Baum“ auf Abschnittsfläche in text_primary.
        let head = s.text("Feature-Baum");
        assert_eq!(head.color, p.text_primary);
        assert!(
            s.fills.iter().any(|f| f.fill == p.bg_section
                && f.rect.contains_rect(head.rect)
                && (f.rect.height() - theme::HEADER_H).abs() < 0.5),
            "Kopf ohne Abschnittsfläche"
        );
    }
}

/// Zeigt `f` drei Bilder lang (neue Fenster messen sich im ersten Bild unsichtbar aus).
fn settle(c: &egui::Context, mut f: impl FnMut(&egui::Context)) -> Shot {
    let mut last = None;
    for _ in 0..3 {
        last = Some(shot(c, WIDTH, |ui| f(ui.ctx())));
    }
    last.unwrap()
}

/// Die Fensterfläche: bg_panel, 1 px border_subtle, Rundung 14.
fn has_panel_frame(s: &Shot, dark: bool, what: &str) {
    let p = theme::palette(dark);
    assert!(
        s.fills.iter().any(|f| f.fill == p.bg_panel
            && f.stroke == egui::Stroke::new(1.0, p.border_subtle)
            && f.radius == egui::CornerRadius::same(theme::RADIUS_LG)),
        "{what} (dunkel: {dark}) ohne Graphit-Rahmen: {:?}",
        s.fills
    );
}

#[test]
fn dialog_windows_use_the_panel_frame() {
    for dark in [true, false] {
        // Ohne `theme::apply`: die Fenster setzen den Rahmen selbst.
        let c = context(None);
        c.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let mut st = AppState::new();
        st.create_box(DVec3::splat(10.0)).unwrap();
        let mut create = CreateDialog::Box {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let s = settle(&c, |ctx| {
            create.show(ctx, &mut st);
        });
        has_panel_frame(&s, dark, "Quader erzeugen");

        let mut export = ExportDialog::new(&st);
        let mut dir = None;
        let s = settle(&c, |ctx| {
            export.show_in(
                ctx,
                &mut st,
                &mut dir,
                &mut planora_app::places::Places::default(),
            );
        });
        has_panel_frame(&s, dark, "Exportieren");

        let mut ws = workspace();
        ws.prompt = Some(Prompt::Notice("Etwas ist schiefgegangen".into()));
        let s = settle(&c, |ctx| prompts::show(ctx, &mut ws));
        has_panel_frame(&s, dark, "Hinweis");
        // Titel als Überschrift in text_primary.
        assert_eq!(s.text("Hinweis").color, theme::palette(dark).text_primary);
    }
}

/// Zustand mit Quader und einer Zeichnung mit gewählter Vorderansicht.
fn drawing_state() -> AppState {
    let mut st = AppState::new();
    st.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    let sheet = Sheet {
        paper: Paper::A4,
        landscape: true,
    };
    let d = st
        .document
        .add_drawing("Blatt 1", sheet, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 148.5, y: 105.0 },
            None,
        )
        .unwrap();
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    st.drawing_ui.selected_view = Some(front);
    st
}

/// Kein Text ragt aus seinem Ausschnitt (sonst wäre er abgeschnitten).
fn nothing_clipped(s: &Shot, what: &str) {
    for t in &s.texts {
        assert!(
            t.clip.expand(0.5).contains_rect(t.rect),
            "{what}: „{}“ abgeschnitten ({:?} außerhalb {:?})",
            t.text,
            t.rect,
            t.clip
        );
    }
}

#[test]
fn no_text_is_clipped_at_1400px() {
    let c = context(Some(true));
    // Menüleiste: eine Zeile, alles innerhalb der Breite.
    let mut ws = workspace();
    let mut shell = ShellState::default();
    let s = shot(&c, WIDTH, |ui| {
        egui::Panel::top("menu").show(ui, |ui| {
            app::menu_bar(ui, &mut ws, &mut shell);
        });
    });
    let first = s.text("Datei").clone();
    for m in ["Datei", "Erzeugen", "Zeichnung", "Bearbeiten", "Ansicht"] {
        let t = s.text(m);
        assert!(same_row(t, &first) && t.rect.right() < WIDTH, "Menü {m}");
    }
    nothing_clipped(&s, "Menüleiste");

    // Tab-Leiste mit fünf Tabs (einer geändert): eine Zeile.
    ws.active_tab_mut()
        .state
        .create_box(DVec3::splat(4.0))
        .unwrap();
    for _ in 0..4 {
        ws.new_tab();
    }
    let s = shot(&c, WIDTH, |ui| {
        egui::Panel::top("tabs").show(ui, |ui| tabs::show(ui, &mut ws));
    });
    let first = s.text("Unbenannt 1 ●").clone();
    for n in 2..=5 {
        let t = s.text(&format!("Unbenannt {n}"));
        assert!(same_row(t, &first) && t.rect.right() < WIDTH, "Tab {n}");
    }
    nothing_clipped(&s, "Tab-Leiste");

    // Zeichnungsfelder mit gewählter Ansicht: das Werkzeugfeld hat genau zwei Zeilen, die
    // Einstellungen der Ansicht stehen im Ansichtsfeld rechts; nichts abgeschnitten.
    let mut st = drawing_state();
    let mut s = shot(&c, WIDTH, |ui| drawing_view::show(ui, &mut st));
    for _ in 0..2 {
        s = shot(&c, WIDTH, |ui| drawing_view::show(ui, &mut st));
    }
    let bar = drawing_view::last_layout(&c).unwrap().toolbar;
    let mut rows: Vec<f32> = Vec::new();
    for t in s.texts.iter().filter(|t| bar.contains(t.rect.center())) {
        assert!(
            bar.expand(0.5).contains_rect(t.rect) && t.rect.right() <= WIDTH,
            "„{}“ ragt aus dem Werkzeugfeld {bar:?}: {:?}",
            t.text,
            t.rect
        );
        let y = t.rect.center().y;
        if !rows.iter().any(|r| (r - y).abs() < 6.0) {
            rows.push(y);
        }
    }
    assert_eq!(rows.len(), 2, "zwei Zeilen: {rows:?}");
    let fit = planora_app::icons::glyph(icons::Icon::FitSheet).to_string();
    for must in ["3D-Modell", fit.as_str(), drawing_view::SUGGEST] {
        assert!(
            bar.contains_rect(s.text(must).rect),
            "„{must}“ im Werkzeugfeld"
        );
    }
    let side = planora_app::shell_panels::field_rects(&c)
        .into_iter()
        .find(|(n, _)| *n == "Ansichtsfeld")
        .expect("Ansichtsfeld")
        .1;
    for must in [
        "Ansicht „Vorne“",
        "Verdeckte Kanten zeigen",
        drawing_view::CENTERLINES,
        "Ansicht löschen",
    ] {
        assert!(
            side.contains_rect(s.text(must).rect),
            "„{must}“ im Ansichtsfeld"
        );
    }
    nothing_clipped(&s, "Zeichnungsfelder");

    // Skizzenfelder (Leiste, Feld, Fuß): alle Texte sichtbar, keiner abgeschnitten; die
    // Werkzeugnamen stehen als Hinweistext an den Symbolen der Leiste.
    let mut st = AppState::new();
    let i = st
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    st.open_sketch(i).unwrap();
    let s = settle(&c, |ctx| sketch_panel::show(ctx, &mut st));
    for must in [
        "Abhängigkeiten",
        "Abstand Punkt–Linie …",
        "Konstruktion umschalten",
        "Skizze beenden",
    ] {
        s.text(must);
    }
    // Der Schalter „Waagerecht/senkrecht erkennen“ steht in zwei Zeilen.
    assert_eq!(sketch_panel::AUTO_AXIS, "Waagerecht/senkrecht erkennen");
    s.text(&sketch_panel::auto_axis_label());
    assert!(
        !sketch_panel::AUTO_AXIS_HINT.contains("  "),
        "Leerzeichen-Kolonne im Hinweistext"
    );
    nothing_clipped(&s, "Skizzenfelder");
    // Die Symbole der Leiste liegen ganz im Fenster (ihr Name steht im Hinweistext, siehe
    // tests/sketch_fields.rs).
    let buttons = sketch_panel::strip_buttons(&c);
    assert!(buttons.iter().any(|(l, _)| *l == "Körperkante kopieren"));
    for (label, r) in buttons {
        assert!(r.left() >= 0.0 && r.bottom() <= HEIGHT, "{label} {r:?}");
    }

    // Eigenschaften in Bereichsbreite (24 % von 1400 px).
    let mut st = AppState::new();
    st.create_box(DVec3::splat(10.0)).unwrap();
    st.selected_body = Some(0);
    let s = shot(&c, WIDTH, |ui| {
        ui.set_max_width(0.24 * WIDTH);
        properties::show(ui, &mut st);
    });
    s.text(properties::POSITION_LABEL);
    nothing_clipped(&s, "Eigenschaften");
}

#[test]
fn dark_and_light_both_render_the_panels() {
    for dark in [true, false] {
        let c = context(Some(dark));
        let p = theme::palette(dark);
        let other = theme::palette(!dark);
        let mut st = AppState::new();
        st.create_box(DVec3::splat(10.0)).unwrap();
        st.selected_body = Some(0);
        let mut dialog = CreateDialog::Sphere { radius: 5.0 };
        let mut s = None;
        for _ in 0..3 {
            s = Some(shot(&c, WIDTH, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.columns(2, |cols| {
                        browser::show(&mut cols[0], &mut st);
                        properties::show(&mut cols[1], &mut st);
                    });
                });
                dialog.show(ui.ctx(), &mut st);
            }));
        }
        let s = s.unwrap();
        let colors: Vec<Color32> = s.texts.iter().map(|t| t.color).collect();
        assert!(
            colors.contains(&p.text_primary),
            "text_primary (dunkel: {dark})"
        );
        assert!(
            colors.contains(&p.text_secondary),
            "text_secondary (dunkel: {dark})"
        );
        for t in &s.texts {
            assert!(
                t.color != other.text_primary && t.color != other.text_secondary,
                "„{}“ in der Farbe des anderen Themas (dunkel: {dark})",
                t.text
            );
        }
        // Werte in text_primary, Bezeichnungen in text_secondary.
        assert_eq!(s.text("Volumen").color, p.text_secondary);
        assert_eq!(s.text("Eigenschaften").color, p.text_primary);
        let all: Vec<Color32> = s
            .fills
            .iter()
            .flat_map(|f| [f.fill, f.stroke.color])
            .chain(colors)
            .collect();
        for old in OLD_COLORS {
            assert!(!all.contains(&old), "alte Farbe {old:?} (dunkel: {dark})");
        }
        // Fenster und Felder des Themas.
        has_panel_frame(&s, dark, "Kugel erzeugen");
    }
}

// ---------------------------------------------------------------------------------------------
// Tastaturfokus auf randlosen Knöpfen und Klicks auf die umgebauten Menüs

fn key_tab() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Bild mit Ereignissen.
fn shot_with(
    c: &egui::Context,
    events: Vec<egui::Event>,
    f: impl FnMut(&mut egui::Ui),
) -> (Shot, Vec<(Rect, egui::Stroke)>) {
    let mut raw = input(WIDTH);
    raw.events = events;
    let out = c.run_ui(raw, f);
    let s = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    let accent = theme::accent(c.global_style().visuals.dark_mode);
    let rings = s
        .fills
        .iter()
        .filter(|f| f.stroke.color == accent && f.stroke.width > 0.0)
        .map(|f| (f.rect, f.stroke))
        .collect();
    (s, rings)
}

/// `tabs`-mal Tab drücken, dann zwei ruhige Bilder; liefert das letzte Bild und seine
/// Akzent-Ringe.
fn after_tabs(
    c: &egui::Context,
    tabs: usize,
    mut f: impl FnMut(&mut egui::Ui),
) -> (Shot, Vec<(Rect, egui::Stroke)>) {
    for _ in 0..tabs {
        shot_with(c, vec![key_tab()], &mut f);
    }
    shot_with(c, vec![], &mut f);
    shot_with(c, vec![], &mut f)
}

/// Genau ein 2-px-Ring in Akzentfarbe, außen um `around`.
fn one_ring(rings: &[(Rect, egui::Stroke)], dark: bool, around: Rect, what: &str) {
    assert_eq!(rings.len(), 1, "{what} (dunkel: {dark}): {rings:?}");
    assert_eq!(
        rings[0].1,
        egui::Stroke::new(2.0, theme::accent(dark)),
        "{what}"
    );
    assert!(
        rings[0].0.contains_rect(around),
        "{what}: Ring {:?} um {around:?}",
        rings[0].0
    );
}

#[test]
fn focused_borderless_buttons_show_the_focus_ring() {
    for dark in [true, false] {
        // Symbolknopf (Skizzenwerkzeug): mit Fokus ein Ring, ohne keiner.
        for focus in [true, false] {
            let c = context(Some(dark));
            let mut first = true;
            let mut button = Rect::NOTHING;
            let (_, rings) = after_tabs(&c, 0, |ui| {
                let r = icons::source_button(
                    ui,
                    icons::IconSource::Font(Icon::ToolLine),
                    "Linie",
                    dark,
                    false,
                );
                if first && focus {
                    r.request_focus();
                }
                first = false;
                button = r.rect;
            });
            if focus {
                one_ring(&rings, dark, button, "Symbolknopf");
            } else {
                assert!(rings.is_empty(), "ohne Fokus kein Ring: {rings:?}");
            }
        }

        // Tab-Leiste: erster Tab-Druck fokussiert den Tab-Kopf, der zweite „Schließen“.
        let c = context(Some(dark));
        let mut ws = workspace();
        let (s, rings) = after_tabs(&c, 0, |ui| tabs::show(ui, &mut ws));
        assert!(rings.is_empty(), "ohne Fokus kein Ring: {rings:?}");
        let head = s.text("Unbenannt 1").rect;
        let (_, rings) = after_tabs(&c, 1, |ui| tabs::show(ui, &mut ws));
        one_ring(&rings, dark, head, "Tab-Kopf");
        let (s, rings) = after_tabs(&c, 1, |ui| tabs::show(ui, &mut ws));
        let close = s
            .texts
            .iter()
            .find(|t| t.text == icons::glyph(Icon::Close).to_string())
            .unwrap()
            .rect;
        one_ring(&rings, dark, close, "Schließen");
        // Geänderter Tab (gezeichneter Punkt) ebenso.
        let c = context(Some(dark));
        ws.active_tab_mut()
            .state
            .create_box(DVec3::splat(4.0))
            .unwrap();
        let (s, rings) = after_tabs(&c, 1, |ui| tabs::show(ui, &mut ws));
        one_ring(&rings, dark, s.text("Unbenannt 1 ●").rect, "geänderter Tab");

        // Zeichnungsleiste: der erste Knopf „3D-Modell“.
        let c = context(Some(dark));
        let mut st = drawing_state();
        st.drawing_ui.selected_view = None;
        let (s, rings) = after_tabs(&c, 0, |ui| drawing_view::show(ui, &mut st));
        assert!(rings.is_empty(), "ohne Fokus kein Ring: {rings:?}");
        let (_, rings) = after_tabs(&c, 1, |ui| drawing_view::show(ui, &mut st));
        one_ring(&rings, dark, s.text("3D-Modell").rect, "Leistenknopf");

        // Menüeintrag „Neu“.
        let c = context(Some(dark));
        let mut ws = workspace();
        let mut menu = |ui: &mut egui::Ui| {
            theme::menu_style(ui.style_mut());
            app::file_menu(ui, &mut ws);
        };
        let (s, rings) = after_tabs(&c, 0, &mut menu);
        assert!(rings.is_empty(), "ohne Fokus kein Ring: {rings:?}");
        let (_, rings) = after_tabs(&c, 1, &mut menu);
        one_ring(&rings, dark, s.text("Neu").rect, "Menüeintrag");

        // Menü „Zeichnung“: „Neue Zeichnung …“, dann „Zum 3D-Modell“.
        let c = context(Some(dark));
        let mut st = AppState::new();
        let mut dialog = None;
        let mut menu = |ui: &mut egui::Ui| {
            theme::menu_style(ui.style_mut());
            planora_app::drawing_dialog::drawing_menu(ui, &mut st, &mut dialog, false);
        };
        let (s, rings) = after_tabs(&c, 0, &mut menu);
        assert!(rings.is_empty(), "ohne Fokus kein Ring: {rings:?}");
        let (_, rings) = after_tabs(&c, 1, &mut menu);
        one_ring(
            &rings,
            dark,
            s.text("Neue Zeichnung …").rect,
            "Neue Zeichnung",
        );
        let (_, rings) = after_tabs(&c, 1, &mut menu);
        one_ring(&rings, dark, s.text("Zum 3D-Modell").rect, "Zum 3D-Modell");

        // Menüeintrag mit Schriftsymbol über `icons::source_entry` (Menü „Erzeugen“).
        let c = context(Some(dark));
        let mut menu = |ui: &mut egui::Ui| {
            theme::menu_style(ui.style_mut());
            icons::source_entry(ui, icons::IconSource::Font(Icon::Box), "Quader", true);
        };
        let (s, rings) = after_tabs(&c, 0, &mut menu);
        assert!(rings.is_empty(), "ohne Fokus kein Ring: {rings:?}");
        let (_, rings) = after_tabs(&c, 1, &mut menu);
        one_ring(&rings, dark, s.text("Quader").rect, "Schriftsymbol-Eintrag");
    }
}

/// Klick an `p` (Bewegen, Drücken, Loslassen, Nachbild); `f` liefert, ob etwas ausgelöst wurde.
fn click(c: &egui::Context, p: Pos2, mut f: impl FnMut(&mut egui::Ui) -> bool) -> bool {
    let press = |pressed| egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut any = false;
    STEP.with(|s| s.set(0.05));
    for events in [
        vec![egui::Event::PointerMoved(p)],
        vec![press(true)],
        vec![press(false)],
        vec![],
    ] {
        shot_with(c, events, |ui| any |= f(ui));
    }
    STEP.with(|s| s.set(1.0));
    any
}

#[test]
fn the_refactored_menus_and_tab_buttons_still_act() {
    let c = context(Some(true));
    // „Neu“ im Menü „Datei“ legt einen Tab an.
    let mut ws = workspace();
    let (s, _) = shot_with(&c, vec![], |ui| {
        app::file_menu(ui, &mut ws);
    });
    let neu = s.text("Neu").rect.center();
    click(&c, neu, |ui| {
        app::file_menu(ui, &mut ws) != app::MenuRequest::None
    });
    assert_eq!(ws.tabs.len(), 2, "Neu legt einen Tab an");

    // „Datei → Projekte …“ in der Menüleiste meldet: Fenster Projekte öffnen.
    let c = context(Some(true));
    let mut shell = ShellState::default();
    let mut bar =
        |ui: &mut egui::Ui| app::menu_bar(ui, &mut ws, &mut shell) == app::MenuRequest::Projects;
    let (s, _) = shot_with(&c, vec![], |ui| {
        bar(ui);
    });
    assert!(!click(&c, s.text("Datei").rect.center(), &mut bar));
    let (s, _) = shot_with(&c, vec![], |ui| {
        bar(ui);
    });
    assert!(
        click(&c, s.text("Projekte …").rect.center(), &mut bar),
        "Projekte … öffnet das Fenster Projekte"
    );

    // Symbole der Tab-Leiste: „Schließen“ am zweiten Tab, „Neu“ am Ende.
    let c = context(Some(true));
    let (s, _) = shot_with(&c, vec![], |ui| tabs::show(ui, &mut ws));
    let close = icons::glyph(Icon::Close).to_string();
    let second = s.texts.iter().filter(|t| t.text == close).nth(1).unwrap();
    click(&c, second.rect.center(), |ui| {
        tabs::show(ui, &mut ws);
        false
    });
    assert_eq!(ws.tabs.len(), 1, "Schließen schließt den unveränderten Tab");
    let (s, _) = shot_with(&c, vec![], |ui| tabs::show(ui, &mut ws));
    let plus = icons::glyph(Icon::NewTab).to_string();
    click(&c, s.text(&plus).rect.center(), |ui| {
        tabs::show(ui, &mut ws);
        false
    });
    assert_eq!(ws.tabs.len(), 2, "Neu legt einen Tab an");
}

#[test]
fn text_and_number_fields_are_26_high() {
    let c = context(Some(true));
    let (mut text, mut number) = (String::from("Name"), 12.5_f64);
    let (mut a, mut b) = (Rect::NOTHING, Rect::NOTHING);
    shot(&c, WIDTH, |ui| {
        a = theme::text_field(ui, egui::TextEdit::singleline(&mut text)).rect;
        b = theme::field(ui, egui::DragValue::new(&mut number)).rect;
    });
    assert_eq!(a.height(), theme::FIELD_H, "Textfeld {a:?}");
    assert_eq!(b.height(), theme::FIELD_H, "Zahlenfeld {b:?}");
}
