//! Programmlogo und Dialog „Über PlanoraCAD“: eingebettete Bilder lesbar, Fenstersymbol fällt bei
//! einem Fehler still weg, Menüeintrag im Menü „Datei“, Inhalt des Dialogs, kleines Zeichen im
//! oberen Feld (Größe, Lage, Feldbreite bei 800 × 600 und 1100 × 700). Ohne Fenster:
//! Rohereignisse je Bild gegen einen egui-Kontext.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::dialog::{CLOSE_LABEL, DialogSize};
use planora_app::icons::{self, Icon};
use planora_app::workspace::Workspace;
use planora_app::{about_dialog, fonts, logo, shell_panels, theme};
use planora_project::config::Config;
use planora_project::library::Library;

const BIG: Vec2 = vec2(1400.0, 900.0);
const MID: Vec2 = vec2(1100.0, 700.0);
const SMALL: Vec2 = vec2(800.0, 600.0);

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
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

fn app() -> PlanoraApp {
    let root = common::temp_root("planora-about");
    PlanoraApp::for_tests(Workspace::new(
        Some(Library::open(&root).unwrap()),
        Config::default(),
    ))
}

/// Was ein Bild zeigt: Texte und die Rechtecke aller Bilder (Netze mit Textur).
#[derive(Default)]
struct Shot {
    texts: Vec<(String, Rect)>,
    images: Vec<Rect>,
}

impl Shot {
    fn of(out: &egui::FullOutput) -> Shot {
        fn walk(shape: &Shape, s: &mut Shot) {
            match shape {
                Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
                Shape::Text(t) => s.texts.push((
                    t.galley.text().to_string(),
                    Rect::from_min_size(t.pos, t.galley.size()),
                )),
                Shape::Rect(r) if r.fill_texture_id() != egui::TextureId::default() => {
                    s.images.push(r.rect);
                }
                Shape::Mesh(m) if m.texture_id != egui::TextureId::default() => {
                    s.images.push(m.calc_bounds());
                }
                _ => {}
            }
        }
        let mut s = Shot::default();
        for c in &out.shapes {
            walk(&c.shape, &mut s);
        }
        s
    }

    fn rect(&self, text: &str) -> Option<Rect> {
        self.texts.iter().find(|t| t.0 == text).map(|t| t.1)
    }

    fn has(&self, text: &str) -> bool {
        self.rect(text).is_some()
    }
}

fn frame(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, events: Vec<Event>) -> Shot {
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(size, events), |ui| app.ui(ui, &mut f));
    let shot = Shot::of(&out);
    out.drop_without_applying_deltas();
    shot
}

fn settle(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) -> Shot {
    frame(c, app, size, vec![]);
    frame(c, app, size, vec![]);
    frame(c, app, size, vec![])
}

fn click(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, p: Pos2) -> Shot {
    let button = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(c, app, size, vec![Event::PointerMoved(p), button(true)]);
    frame(c, app, size, vec![button(false)]);
    frame(c, app, size, vec![])
}

fn field(c: &egui::Context, name: &str) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("Feld {name} fehlt"))
        .1
}

/// Menü „Datei“ öffnen und „Über PlanoraCAD“ wählen; liefert das Bild danach.
fn open_about(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) -> Shot {
    let shot = settle(c, app, size);
    let datei = shot.rect("Datei").expect("Menü Datei");
    let shot = click(c, app, size, datei.center());
    let entry = shot
        .rect(about_dialog::TITLE)
        .expect("Eintrag im Menü Datei");
    click(c, app, size, entry.center())
}

#[test]
fn embedded_icons_decode_to_rgba() {
    for (bytes, edge) in [(logo::ICON_256, 256u32), (logo::ICON_64, 64)] {
        let icon = logo::decode(bytes).expect("eingebettetes Bild lesbar");
        assert_eq!((icon.width, icon.height), (edge, edge));
        assert_eq!(icon.rgba.len(), (edge * edge * 4) as usize);
        assert!(
            icon.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
            "Bild ist nicht ganz durchsichtig"
        );
        assert!(
            icon.rgba.iter().any(|&b| b != 0),
            "Bild ist nicht ganz schwarz"
        );
    }
    let window = logo::window_icon().expect("Fenstersymbol");
    assert_eq!((window.width, window.height), (256, 256));
}

#[test]
fn unreadable_icon_data_falls_back_without_panic() {
    assert!(logo::decode(&[]).is_none());
    assert!(logo::window_icon_from(b"kein PNG").is_none());
    let truncated = &logo::ICON_64[..logo::ICON_64.len() / 2];
    assert!(logo::window_icon_from(truncated).is_none());
    // Auch das Bild für die Oberfläche fehlt dann nur.
    let c = context();
    let mut shown = true;
    let out = c.run_ui(input(SMALL, vec![]), |ui| {
        shown = logo::image(ui.ctx(), "defekt", b"kein PNG", 24.0).is_some();
    });
    out.drop_without_applying_deltas();
    assert!(!shown);
}

#[test]
fn datei_menu_opens_the_about_dialog_and_it_closes() {
    let c = context();
    let mut app = app();
    let shot = open_about(&c, &mut app, BIG);
    assert!(app.ws_mut().about_open, "Dialog ist offen");

    let shot = if shot.has(about_dialog::NAME) {
        shot
    } else {
        settle(&c, &mut app, BIG)
    };
    assert!(shot.has(about_dialog::NAME), "Name");
    assert!(
        shot.has(&format!("Version {}", env!("CARGO_PKG_VERSION"))),
        "Version"
    );
    assert!(shot.has("Lizenz: GPL-3.0-only"), "Lizenz");
    for line in [
        "Schrift Montserrat (SIL Open Font License)",
        "Symbole Phosphor Icons (MIT)",
        "Schrift Roboto (Apache-2.0)",
        "Logo-Schrift Rajdhani (SIL Open Font License)",
    ] {
        assert!(shot.has(line), "Quelle {line}");
    }
    assert!(
        shot.images
            .iter()
            .any(|r| (r.width() - logo::DIALOG_SIZE).abs() < 0.5
                && (r.height() - logo::DIALOG_SIZE).abs() < 0.5),
        "Logo mit {} Punkten: {:?}",
        logo::DIALOG_SIZE,
        shot.images
    );

    // Schließen-Symbol.
    let close = shot
        .texts
        .iter()
        .filter(|t| t.0 == icons::glyph(Icon::Close).to_string())
        .map(|t| t.1)
        .next_back()
        .expect("Schließen-Symbol");
    let shot = click(&c, &mut app, BIG, close.center());
    assert!(!app.ws_mut().about_open, "{CLOSE_LABEL} schließt");
    assert!(!shot.has(about_dialog::NAME), "Dialog ist weg");
}

#[test]
fn about_texts_are_plain_language() {
    assert_eq!(about_dialog::TITLE, "Über PlanoraCAD");
    let mut all = vec![
        about_dialog::version_line(),
        about_dialog::license_line(),
        about_dialog::SOURCES_HEADING.to_string(),
    ];
    all.extend(about_dialog::SOURCES.iter().map(|s| s.to_string()));
    assert_eq!(about_dialog::SOURCES.len(), 4);
    for t in &all {
        assert!(t.chars().count() <= 60, "kurz genug: {t}");
        for tech in ["TTF", "crate", "::", "Cargo", "bytes", "px"] {
            assert!(!t.contains(tech), "{t} enthält {tech}");
        }
    }
    assert_eq!(
        about_dialog::version_line(),
        format!("Version {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn about_dialog_fits_the_smallest_window() {
    let c = context();
    let mut app = app();
    open_about(&c, &mut app, SMALL);
    settle(&c, &mut app, SMALL);
    let id = egui::Id::new(about_dialog::TITLE);
    let rect = c.memory(|m| m.area_rect(id)).expect("Dialog gezeigt");
    let window = Rect::from_min_size(Pos2::ZERO, SMALL);
    assert!(window.contains_rect(rect), "{rect:?} im Fenster");
    assert!(
        (rect.width() - DialogSize::Normal.width()).abs() < 0.5,
        "Normalbreite: {rect:?}"
    );
}

#[test]
fn small_icon_sits_left_of_the_menu_inside_the_top_field() {
    for size in [BIG, MID, SMALL] {
        let c = context();
        let mut app = app();
        let shot = settle(&c, &mut app, size);
        let top = field(&c, "oben");
        let window = Rect::from_min_size(Pos2::ZERO, size);
        assert!(window.contains_rect(top), "{size:?}: oben {top:?}");
        let small: Vec<Rect> = shot
            .images
            .iter()
            .copied()
            .filter(|r| top.contains_rect(*r))
            .collect();
        assert_eq!(small.len(), 1, "{size:?}: ein Zeichen im Feld: {small:?}");
        let icon = small[0];
        assert!(
            (icon.width() - logo::SMALL_SIZE).abs() < 0.5
                && (icon.height() - logo::SMALL_SIZE).abs() < 0.5,
            "{size:?}: Größe {icon:?}"
        );
        assert_eq!(logo::SMALL_SIZE, 24.0);
        let datei = shot.rect("Datei").unwrap();
        assert!(icon.right() <= datei.left(), "{size:?}: links vom Menü");
        // Passt die Wunschbreite zwischen die Spalten (großes Fenster), ist das Feld so breit,
        // dass auch das Zeichen „Neuer Tab“ mit dem Rand wie links hineinpasst (die
        // Wunschbreite zählt das Zeichen vorn mit). Im engen Fenster rollt die Tab-Leiste.
        if size == BIG {
            let plus = shot
                .texts
                .iter()
                .find(|t| t.0 == icons::glyph(Icon::NewTab).to_string())
                .expect("Neuer Tab")
                .1;
            let margin = icon.left() - top.left();
            assert!(
                plus.right() + margin <= top.right() + 0.5,
                "{size:?}: Rand rechts {plus:?} in {top:?}"
            );
        }
        // Menü und Tab bleiben im Feld.
        for t in ["Datei", "Ansicht", "Unbenannt 1"] {
            let r = shot
                .rect(t)
                .unwrap_or_else(|| panic!("{size:?}: {t} fehlt"));
            assert!(top.contains_rect(r), "{size:?}: {t} {r:?} in {top:?}");
        }
    }
}

#[test]
fn top_field_keeps_its_width_rules_with_the_icon() {
    // Mit dem Zeichen reicht das Feld bei 800 Punkten Fensterbreite noch für Menü und einen Tab,
    // und es überlappt kein anderes Feld.
    let c = context();
    let mut app = app();
    settle(&c, &mut app, SMALL);
    let top = field(&c, "oben");
    assert!(
        top.width() <= SMALL.x - 2.0 * planora_app::shell::SHELL_INSET + 0.5,
        "{top:?}"
    );
    let ansicht_right = {
        let shot = settle(&c, &mut app, SMALL);
        shot.rect("Ansicht").unwrap().right()
    };
    // Menü samt Zeichen und einem Rest für Tabs (mindestens 120 Punkte) passen hinein.
    assert!(
        ansicht_right + 120.0 <= top.right() + 0.5,
        "{ansicht_right} {top:?}"
    );
    for (name, r) in shell_panels::field_rects(&c) {
        if name != "oben" {
            assert!(!r.intersects(top), "{name} {r:?} überlappt {top:?}");
        }
    }
}
