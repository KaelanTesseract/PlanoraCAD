//! Durchsichtigkeit der Panels (Einstellungen → Darstellung): Voreinstellung 40 %, Grenzen 0 bis
//! 80 %, Speichern in der Konfiguration (nur im Temp-Ordner), Deckkraft des Baum-Rahmens und
//! Lesbarkeit der Zeilen auch über einem weißen (oder schwarzen) 3D-Bereich.

mod common;

use egui::Color32;
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{browser, theme};
use planora_project::config::{
    Config, DEFAULT_PANEL_TRANSPARENCY, MAX_PANEL_TRANSPARENCY, clamp_panel_transparency,
};
use std::fs;
use std::path::PathBuf;

const BOTH: [bool; 2] = [true, false];

/// Arbeitsbereich ohne Bibliothek, Konfiguration in einem eigenen Temp-Ordner (nie `%APPDATA%`).
fn ws_in_temp(prefix: &str) -> (Workspace, PathBuf, PathBuf) {
    let tmp = common::temp_root(prefix);
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(None, Config::default());
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    (ws, tmp, cfg)
}

fn expected_alpha(p: u8) -> u8 {
    (255.0 * f32::from(100 - p) / 100.0).round() as u8
}

#[test]
fn default_is_forty_percent() {
    assert_eq!(DEFAULT_PANEL_TRANSPARENCY, 40);
    assert_eq!(MAX_PANEL_TRANSPARENCY, 80);
    assert_eq!(Config::default().panel_transparency, 40);
    let (ws, tmp, _) = ws_in_temp("planora-durchsicht-vorgabe");
    assert_eq!(ws.panel_transparency(), 40);
    // Ältere Dateien ohne das Feld und leere Dateien: Voreinstellung, Wurzel bleibt.
    let old = tmp.join("alt.json");
    fs::write(&old, r#"{ "root": "D:/Projekte", "favorites": ["D:/A"] }"#).unwrap();
    let c = Config::read_from(&old).unwrap();
    assert_eq!(c.panel_transparency, 40);
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(
        Config::load_from(&tmp.join("fehlt.json")).panel_transparency,
        40
    );
}

#[test]
fn value_is_clamped_to_zero_to_eighty_on_read_and_set() {
    assert_eq!(clamp_panel_transparency(0), 0);
    assert_eq!(clamp_panel_transparency(80), 80);
    assert_eq!(clamp_panel_transparency(81), 80);
    assert_eq!(clamp_panel_transparency(255), 80);
    let (mut ws, tmp, cfg) = ws_in_temp("planora-durchsicht-grenzen");
    // Lesen: zu groß, negativ, Bruch, falscher Typ, null.
    for (text, want) in [
        ("95", 80),
        ("80", 80),
        ("1000", 80),
        ("-5", 0),
        ("0", 0),
        ("33.6", 34),
        ("\"viel\"", 40),
        ("null", 40),
        ("[1]", 40),
    ] {
        let f = tmp.join("lesen.json");
        fs::write(
            &f,
            format!(r#"{{ "root": "D:/Projekte", "panel_transparency": {text} }}"#),
        )
        .unwrap();
        let c = Config::read_from(&f).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(c.panel_transparency, want, "{text}");
        assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")), "{text}");
    }
    // Setzen.
    assert!(ws.set_panel_transparency(200));
    assert_eq!(ws.panel_transparency(), 80);
    assert_eq!(Config::read_from(&cfg).unwrap().panel_transparency, 80);
    assert!(ws.set_panel_transparency(0));
    assert_eq!(ws.panel_transparency(), 0);
    // Ein von Hand gebauter Stand im Speicher wird beim Abfragen begrenzt.
    ws.config.panel_transparency = 99;
    assert_eq!(ws.panel_transparency(), 80);
}

#[test]
fn setting_is_saved_in_the_config_file_in_the_temp_dir() {
    let (mut ws, tmp, cfg) = ws_in_temp("planora-durchsicht-speichern");
    assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
    Config {
        root: Some(tmp.join("Projekte")),
        favorites: vec![tmp.join("Fav")],
        ..Config::default()
    }
    .save_to(&cfg)
    .unwrap();
    assert!(ws.set_panel_transparency(25));
    assert_eq!(ws.panel_transparency(), 25);
    let disk = Config::read_from(&cfg).unwrap();
    assert_eq!(disk.panel_transparency, 25);
    assert_eq!(disk.root, Some(tmp.join("Projekte")), "Wurzel bleibt");
    assert_eq!(disk.favorites, [tmp.join("Fav")], "Favoriten bleiben");

    // Ohne Schreibrecht: der Wert gilt für die Sitzung, die Datei bleibt, wie sie ist.
    let blocker = tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    ws.config_path = blocker.join("config.json");
    assert!(!ws.set_panel_transparency(60));
    assert_eq!(ws.panel_transparency(), 60);
    assert_eq!(fs::read(&blocker).unwrap(), b"x");
    // Gelingt später das Schreiben (etwa beim Wechsel des Verzeichnisses), kommt der Wert mit.
    ws.config_path = cfg.clone();
    assert!(ws.try_persist_config());
    assert_eq!(Config::read_from(&cfg).unwrap().panel_transparency, 60);

    // Eine unlesbare Datei wird beim Regeln nicht ersetzt (sie könnte noch die Wurzel enthalten).
    fs::write(&cfg, b"{kaputt").unwrap();
    assert!(!ws.set_panel_transparency(10));
    assert_eq!(ws.panel_transparency(), 10);
    assert_eq!(fs::read(&cfg).unwrap(), b"{kaputt");
}

#[test]
fn frame_alpha_matches_percent() {
    for dark in BOTH {
        let p = theme::palette(dark);
        for percent in 0..=MAX_PANEL_TRANSPARENCY {
            let f = theme::panel_frame_translucent(dark, percent);
            let want = expected_alpha(percent);
            assert_eq!(f.fill.a(), want, "dunkel {dark}, {percent} %");
            let un = f.fill.to_srgba_unmultiplied();
            // Farbton bleibt bg_panel (bis auf Rundung beim Vormultiplizieren).
            for (got, base) in un[..3]
                .iter()
                .zip([p.bg_panel.r(), p.bg_panel.g(), p.bg_panel.b()])
            {
                assert!(
                    got.abs_diff(base) <= 2,
                    "dunkel {dark}, {percent} %: {un:?}"
                );
            }
            // Rand, Rundung und Innenabstand wie beim deckenden Panel.
            let opaque = theme::panel_frame(dark);
            assert_eq!(f.stroke, opaque.stroke);
            assert_eq!(f.corner_radius, opaque.corner_radius);
            assert_eq!(f.inner_margin, opaque.inner_margin);
            assert_eq!(browser::tree_frame(dark, percent), f);
        }
        // Über 80 % wird begrenzt.
        assert_eq!(
            theme::panel_frame_translucent(dark, 100),
            theme::panel_frame_translucent(dark, 80)
        );
        assert_eq!(theme::panel_frame_translucent(dark, 40).fill.a(), 153);
        assert_eq!(theme::panel_frame_translucent(dark, 80).fill.a(), 51);
    }
}

#[test]
fn zero_percent_is_opaque() {
    for dark in BOTH {
        assert_eq!(
            theme::panel_frame_translucent(dark, 0),
            theme::panel_frame(dark)
        );
        assert_eq!(theme::panel_frame_translucent(dark, 0).fill.a(), 255);
        assert_eq!(theme::row_backing(dark, 0), Color32::TRANSPARENT);
    }
}

// --- Lesbarkeit --------------------------------------------------------------------------------

/// Legt `top` (vormultipliziert, wie egui) über die deckende Farbe `bottom`; gemischt wird wie im
/// Renderer im sRGB-Raum (egui blendet im Gamma-Raum).
fn over(top: Color32, bottom: Color32) -> Color32 {
    let a = f32::from(top.a()) / 255.0;
    let m = |t: u8, b: u8| (f32::from(t) + f32::from(b) * (1.0 - a)).round().min(255.0) as u8;
    Color32::from_rgb(
        m(top.r(), bottom.r()),
        m(top.g(), bottom.g()),
        m(top.b(), bottom.b()),
    )
}

/// Fläche hinter den Zeilen: 3D-Bereich `bg`, darüber das Panel, darüber die Mindestfläche.
fn surface(dark: bool, percent: u8, bg: Color32) -> Color32 {
    let panel = theme::panel_frame_translucent(dark, percent).fill;
    over(theme::row_backing(dark, percent), over(panel, bg))
}

/// Gesamtdeckung hinter den Zeilen: Panel und Mindestfläche übereinander (aus den echten
/// Alpha-Werten).
fn row_opacity(dark: bool, percent: u8) -> f32 {
    let panel = f32::from(theme::panel_frame_translucent(dark, percent).fill.a()) / 255.0;
    let back = f32::from(theme::row_backing(dark, percent).a()) / 255.0;
    1.0 - (1.0 - panel) * (1.0 - back)
}

/// Alle Schriftfarben des durchsichtigen Baums mit Namen.
fn inks(dark: bool) -> [(&'static str, Color32); 4] {
    let i = theme::tree_ink(dark);
    [
        // Ordner, Dokument, gewählte und überfahrene Zeilen.
        ("aktiv", i.active),
        // Zeilen in Ruhe.
        ("Ruhe", i.rest),
        // Schwacher Text: „zurückgesetzt“, „übersprungen“, „unterdrückt“, „nicht berechnet“,
        // ausgeblendete Körper, Leer-Hinweise.
        ("schwach", i.weak),
        // „Fehler“, verlorene Maße und Positionen.
        ("Fehler", i.error),
    ]
}

#[test]
fn contrast_of_text_over_light_background_at_80_percent_stays_readable() {
    let mut report = Vec::new();
    let mut failures = Vec::new();
    for dark in BOTH {
        for percent in [0, 40, 80] {
            // Schlimmster Fall: weißer 3D-Bereich (helles Modell); dazu schwarz für das helle
            // Thema, dessen dunkle Schrift auf dunklem Grund ebenso leiden würde.
            for (bg_name, bg) in [("weiß", Color32::WHITE), ("schwarz", Color32::BLACK)] {
                let s = surface(dark, percent, bg);
                for (what, fg) in inks(dark) {
                    let r = theme::contrast_ratio(fg, s);
                    report.push(format!(
                        "{} {percent} % {bg_name} {what}: {r:.2}",
                        if dark { "dunkel" } else { "hell" }
                    ));
                    if r < 4.5 {
                        failures.push(report.last().unwrap().clone());
                    }
                }
            }
        }
    }
    for dark in BOTH {
        for percent in [0, 40, 80] {
            report.push(format!(
                "{} {percent} % Deckung der Zeilen: {:.3}",
                if dark { "dunkel" } else { "hell" },
                row_opacity(dark, percent)
            ));
        }
    }
    println!("{report:#?}");
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_slider_still_makes_the_rows_see_through() {
    for dark in BOTH {
        let (o0, o40, o80) = (
            row_opacity(dark, 0),
            row_opacity(dark, 40),
            row_opacity(dark, 80),
        );
        assert!((o0 - 1.0).abs() < 1e-6, "dunkel {dark}: 0 % deckend ({o0})");
        assert!(o80 <= 0.70, "dunkel {dark}: 80 % zu deckend ({o80})");
        assert!(
            o0 - o80 >= 0.3,
            "dunkel {dark}: kaum Unterschied ({o0} / {o80})"
        );
        assert!(
            o40 - o80 >= 0.05,
            "dunkel {dark}: 40 % und 80 % gleich ({o40} / {o80})"
        );
        // Je weiter der Regler, desto durchsichtiger auch die Zeilen.
        let mut last = 1.0;
        for percent in 0..=MAX_PANEL_TRANSPARENCY {
            let o = row_opacity(dark, percent);
            assert!(o <= last + 1e-6, "dunkel {dark}, {percent} %: {o} > {last}");
            last = o;
        }
    }
}

#[test]
fn the_tree_ink_is_stronger_than_the_palette_and_keeps_its_roles() {
    for dark in BOTH {
        let p = theme::palette(dark);
        let i = theme::tree_ink(dark);
        assert_eq!(i.active, p.text_primary);
        // Gegen das deckende Panel: mehr Kontrast als die Palettenfarben.
        let c = |x| theme::contrast_ratio(x, p.bg_panel);
        assert!(c(i.rest) > c(p.text_secondary), "dunkel {dark}");
        assert!(c(i.weak) > c(p.text_tertiary), "dunkel {dark}");
        assert!(c(i.error) >= 4.5, "dunkel {dark}");
        // Rangfolge bleibt: aktiv kräftiger als Ruhe, Ruhe kräftiger als schwach.
        assert!(
            c(i.active) > c(i.rest) && c(i.rest) > c(i.weak),
            "dunkel {dark}"
        );
        // Fehler bleibt rot.
        assert!(
            i.error.r() > i.error.g() && i.error.r() > i.error.b(),
            "dunkel {dark}"
        );
    }
}

#[test]
fn the_backing_only_appears_when_the_panel_alone_is_too_thin() {
    for dark in BOTH {
        // Deckend genug: keine Fläche.
        assert_eq!(theme::row_backing(dark, 0), Color32::TRANSPARENT);
        // Die Fläche hat die Farbe des Panels.
        let b = theme::row_backing(dark, 80).to_srgba_unmultiplied();
        let p = theme::palette(dark).bg_panel;
        assert!(b[3] > 0, "dunkel {dark}");
        for (got, base) in b[..3].iter().zip([p.r(), p.g(), p.b()]) {
            assert!(got.abs_diff(base) <= 2, "dunkel {dark}: {b:?}");
        }
        // Mehr Durchsichtigkeit braucht nie weniger Fläche.
        let mut last = 0;
        for percent in 0..=MAX_PANEL_TRANSPARENCY {
            let a = theme::row_backing(dark, percent).a();
            assert!(a >= last, "dunkel {dark}, {percent} %");
            last = a;
        }
        // Die Mindestdeckung folgt aus den benutzten Schriftfarben (keine feste Zahl).
        let need = theme::tree_min_opacity(dark);
        assert!((0.3..=0.70).contains(&need), "dunkel {dark}: {need}");
        assert!(row_opacity(dark, 80) >= need, "dunkel {dark}");
    }
}

/// Alle gefüllten Rechtecke eines Bildes.
fn rect_fills(shape: &egui::epaint::Shape, out: &mut Vec<(egui::Rect, Color32)>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| rect_fills(s, out)),
        egui::epaint::Shape::Rect(r) => out.push((r.rect, r.fill)),
        _ => {}
    }
}

#[test]
fn the_tree_paints_the_backing_behind_its_rows() {
    for dark in BOTH {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        theme::apply(&ctx, dark);
        let mut s = AppState::new();
        s.dark = dark;
        let mut fills = Vec::new();
        for _ in 0..3 {
            let out = ctx.run_ui(egui::RawInput::default(), |ui| {
                browser::show_in_panel(ui, &mut s, false, 80)
            });
            fills.clear();
            for c in &out.shapes {
                rect_fills(&c.shape, &mut fills);
            }
            out.drop_without_applying_deltas();
        }
        let backing = theme::row_backing(dark, 80);
        let rows = browser::row_rects(&ctx);
        let folders = browser::folder_rects(&ctx);
        let (back, _) = fills
            .iter()
            .find(|(_, c)| *c == backing)
            .unwrap_or_else(|| panic!("keine Mindestfläche (dunkel {dark}): {fills:?}"));
        for (_, r) in &folders {
            assert!(back.contains_rect(*r), "Ordner {r:?} außerhalb {back:?}");
        }
        for (n, r) in &rows {
            assert!(back.contains_rect(*r), "Zeile {n} {r:?} außerhalb {back:?}");
        }
        // Bei 0 % gibt es keine eigene Fläche.
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            browser::show_in_panel(ui, &mut s, false, 0)
        });
        let mut fills = Vec::new();
        for c in &out.shapes {
            rect_fills(&c.shape, &mut fills);
        }
        out.drop_without_applying_deltas();
        assert!(
            !fills.iter().any(|(_, c)| *c == backing),
            "dunkel {dark}: Fläche bei 0 %"
        );
    }
}

/// Alle Texte eines Bildes mit Farbe.
fn text_colors(shape: &egui::epaint::Shape, out: &mut Vec<(String, Color32)>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| text_colors(s, out)),
        egui::epaint::Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |s| s.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            out.push((t.galley.text().to_string(), color));
        }
        _ => {}
    }
}

#[test]
fn the_translucent_tree_draws_its_rows_in_the_tree_ink() {
    for dark in BOTH {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        theme::apply(&ctx, dark);
        let mut s = AppState::new();
        s.dark = dark;
        let mut texts = Vec::new();
        for _ in 0..3 {
            let out = ctx.run_ui(egui::RawInput::default(), |ui| {
                browser::show_in_panel(ui, &mut s, false, 80)
            });
            texts.clear();
            for c in &out.shapes {
                text_colors(&c.shape, &mut texts);
            }
            out.drop_without_applying_deltas();
        }
        let ink = theme::tree_ink(dark);
        let allowed = [ink.active, ink.rest, ink.weak, ink.error];
        let p = theme::palette(dark);
        for (t, c) in &texts {
            if t == "Feature-Baum" {
                // Überschrift auf deckender Abschnittsfläche.
                assert_eq!(*c, p.text_primary);
                continue;
            }
            assert!(allowed.contains(c), "dunkel {dark}: „{t}“ in {c:?}");
        }
        // Leerer Baum: Hinweise in der schwachen Baumfarbe, Ordner in der aktiven.
        assert!(
            texts
                .iter()
                .any(|(t, c)| t.starts_with("Noch keine Körper") && *c == ink.weak),
            "dunkel {dark}: {texts:?}"
        );
        assert!(
            texts.iter().any(|(t, c)| t == "Körper" && *c == ink.active),
            "dunkel {dark}: {texts:?}"
        );
        // Außerhalb des Baums bleibt der Stil unverändert.
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            browser::show_in_panel(ui, &mut s, false, 80);
            assert_eq!(ui.visuals().weak_text_color(), p.text_tertiary);
            assert_eq!(ui.visuals().error_fg_color, p.error);
        });
        out.drop_without_applying_deltas();
    }
}

#[test]
fn save_places_also_writes_a_pending_transparency() {
    let (mut ws, tmp, cfg) = ws_in_temp("planora-durchsicht-favoriten");
    let blocker = tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    ws.config_path = blocker.join("config.json");
    assert!(!ws.set_panel_transparency(70));
    assert!(ws.panel_transparency_pending);
    ws.config_path = cfg.clone();
    let mut places = ws.places();
    places.add_favorite(&tmp.join("Teile")).unwrap();
    assert!(ws.save_places(places));
    let disk = Config::read_from(&cfg).unwrap();
    assert_eq!(disk.panel_transparency, 70);
    assert_eq!(disk.favorites, [tmp.join("Teile")]);
    assert!(
        !ws.panel_transparency_pending,
        "danach nicht mehr vorgemerkt"
    );
    // Ohne Vormerkung bleibt der Wert auf der Platte (eine andere Instanz darf ihn ändern).
    Config::update_at(&cfg, |c| c.panel_transparency = 12).unwrap();
    let mut places = ws.places();
    places.add_favorite(&tmp.join("Mehr")).unwrap();
    assert!(ws.save_places(places));
    assert_eq!(Config::read_from(&cfg).unwrap().panel_transparency, 12);
}
