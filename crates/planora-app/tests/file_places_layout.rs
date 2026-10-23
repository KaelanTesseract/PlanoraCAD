//! Dateiauswahl bei 800 × 600: Die Leiste der Schnellzugriffe bricht um, die Fenster bleiben
//! höchstens 720 px breit und im Fenster, „Speichern“ bzw. „Öffnen“ bleibt erreichbar – mit 0,
//! 1 und 24 Favoriten, auch mit langen Namen.

mod common;

use common::{Screen, Seen, find};
use egui::{Rect, pos2};
use planora_app::export_dialog::*;
use planora_app::import_dialog::ImportDialog;
use planora_app::places::Places;
use planora_app::state::AppState;
use planora_app::step_import_dialog::StepImportDialog;
use planora_math::DVec3;
use std::fs;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-leiste-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Projektordner und `n` Favoriten; mit `long` sehr lange Ordnernamen.
fn places(dir: &std::path::Path, n: usize, long: bool) -> Places {
    let root = dir.join("Projekte");
    fs::create_dir_all(&root).unwrap();
    Places {
        favorites: (0..n)
            .map(|i| {
                if long {
                    dir.join(format!(
                        "Ein sehr langer Ordnername für die Kundenprojekte aus dem Jahr {i} mit vielen Unterordnern"
                    ))
                } else {
                    dir.join(format!("Fav {i}"))
                }
            })
            .collect(),
        project_root: Some(root),
        ..Places::default()
    }
}

fn window() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0))
}

/// Fenster höchstens 720 breit und im Bild, alle Texte waagrecht im Fenster, der Knopf
/// `button` sichtbar (nicht abgeschnitten) im Fenster.
fn check(what: &str, rect: Rect, seen: &[Seen], button: &str) {
    assert!(rect.width() <= 720.0 + 1.0, "{what}: {rect:?}");
    assert!(window().contains_rect(rect), "{what}: {rect:?}");
    for s in seen {
        if s.clip.intersects(s.rect) {
            assert!(
                s.rect.left() >= -0.5 && s.rect.right() <= 800.5,
                "{what}: „{}“ bei {:?}",
                s.text,
                s.rect
            );
        }
    }
    let b = find(seen, button).unwrap_or_else(|| panic!("{what}: „{button}“ fehlt"));
    assert!(
        b.clip.contains_rect(b.rect) && rect.contains_rect(b.rect),
        "{what}: „{button}“ {:?} in {:?}",
        b.rect,
        b.clip
    );
}

const CASES: [(usize, bool); 4] = [(0, false), (1, false), (24, false), (24, true)];

#[test]
fn the_export_chooser_wraps_its_places_at_800x600() {
    for (n, long) in CASES {
        let dir = temp_dir(&format!("export-{n}-{long}"));
        let mut s = AppState::new();
        s.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
        let mut d = ExportDialog::new(&s);
        d.tick(&s, true);
        d.begin_save(dir.clone(), &s);
        assert!(d.chooser().is_some());
        let mut shared = places(&dir, n, long);
        let mut last = None;
        let mut screen = Screen::new(800.0, 600.0);
        let seen = screen.settle(8, &mut |ctx| {
            assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
        });
        check(
            &format!("Exportieren, {n} Favoriten, lang {long}"),
            d.last_rect.unwrap(),
            &seen,
            "Speichern",
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn the_import_choosers_wrap_their_places_at_800x600() {
    for (n, long) in CASES {
        let dir = temp_dir(&format!("import-{n}-{long}"));
        let mut s = AppState::new();
        let mut shared = places(&dir, n, long);
        let mut last = None;

        let mut d = ImportDialog::new(dir.clone());
        let mut screen = Screen::new(800.0, 600.0);
        let seen = screen.settle(8, &mut |ctx| {
            assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
        });
        check(
            &format!("Netz importieren, {n} Favoriten, lang {long}"),
            d.last_rect.unwrap(),
            &seen,
            "Öffnen",
        );

        let mut d = StepImportDialog::new(dir.clone());
        let mut screen = Screen::new(800.0, 600.0);
        let seen = screen.settle(8, &mut |ctx| {
            assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
        });
        check(
            &format!("STEP importieren, {n} Favoriten, lang {long}"),
            d.last_rect.unwrap(),
            &seen,
            "Öffnen",
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

/// Ordner mit 12 Unterordnern und passenden Dateien (STL, OBJ, STEP), dazu 6 vorgegebene
/// Laufwerke: die Leiste braucht so schon ohne Favoriten zwei Zeilen.
fn populated(name: &str, favorites: usize) -> (PathBuf, Places) {
    let dir = temp_dir(name);
    for i in 0..12 {
        fs::create_dir_all(dir.join(format!("Ordner {i:02}"))).unwrap();
    }
    for f in ["a.stl", "b.obj", "c.step", "d.stp", "e.stl"] {
        fs::write(dir.join(f), b"x").unwrap();
    }
    let mut p = places(&dir, favorites, false);
    p.drives = Some(
        ["C:\\", "D:\\", "E:\\", "F:\\", "G:\\", "H:\\"]
            .into_iter()
            .map(PathBuf::from)
            .collect(),
    );
    (dir, p)
}

#[test]
fn action_buttons_stay_visible_in_a_full_folder_at_800x600() {
    for n in [0, 24] {
        // Exportieren (Speichern).
        let (dir, mut shared) = populated(&format!("voll-export-{n}"), n);
        let mut s = AppState::new();
        s.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
        let mut d = ExportDialog::new(&s);
        d.tick(&s, true);
        d.begin_save(dir.clone(), &s);
        let mut last = None;
        let mut screen = Screen::new(800.0, 600.0);
        let seen = screen.settle(8, &mut |ctx| {
            assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
        });
        let what = format!("Exportieren, voller Ordner, {n} Favoriten");
        assert!(
            find(&seen, "Ordner 00").is_some(),
            "{what}: Liste gezeichnet"
        );
        check(&what, d.last_rect.unwrap(), &seen, "Speichern");
        check(&what, d.last_rect.unwrap(), &seen, "Abbrechen");
        let _ = fs::remove_dir_all(&dir);

        // Netz importieren und STEP importieren (Öffnen).
        let (dir, mut shared) = populated(&format!("voll-import-{n}"), n);
        let mut s = AppState::new();
        let mut d = ImportDialog::new(dir.clone());
        let mut screen = Screen::new(800.0, 600.0);
        let seen = screen.settle(8, &mut |ctx| {
            assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
        });
        let what = format!("Netz importieren, voller Ordner, {n} Favoriten");
        check(&what, d.last_rect.unwrap(), &seen, "Öffnen");
        let mut d = StepImportDialog::new(dir.clone());
        let mut screen = Screen::new(800.0, 600.0);
        let seen = screen.settle(8, &mut |ctx| {
            assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
        });
        let what = format!("STEP importieren, voller Ordner, {n} Favoriten");
        check(&what, d.last_rect.unwrap(), &seen, "Öffnen");

        // In einem niedrigen Fenster (800 × 380) rollt der Inhalt; „Öffnen“ bleibt in der
        // Fußzeile sichtbar.
        let low = |seen: &[Seen], rect: Rect, what: &str| {
            let b = find(seen, "Öffnen").unwrap_or_else(|| panic!("{what}: „Öffnen“ fehlt"));
            assert!(
                b.clip.contains_rect(b.rect) && rect.contains_rect(b.rect),
                "{what}: {:?} in {:?}",
                b.rect,
                b.clip
            );
        };
        let mut d = ImportDialog::new(dir.clone());
        let mut screen = Screen::new(800.0, 380.0);
        let seen = screen.settle(8, &mut |ctx| {
            assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
        });
        low(&seen, d.last_rect.unwrap(), "Netz importieren, 800 × 380");
        let mut d = StepImportDialog::new(dir.clone());
        let mut screen = Screen::new(800.0, 380.0);
        let seen = screen.settle(8, &mut |ctx| {
            assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
        });
        low(&seen, d.last_rect.unwrap(), "STEP importieren, 800 × 380");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_replace_question_stays_visible_in_a_full_folder() {
    let (dir, mut shared) = populated("ersetzen", 24);
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    d.begin_save(dir.clone(), &s);
    // Die vorgeschlagene Datei gibt es schon: erst „Speichern“, dann Rückfrage „Ersetzen“.
    let name = format!("{}.stl", d.chooser().unwrap().file_name);
    fs::write(dir.join(name), b"alt").unwrap();
    let mut last = None;
    let mut screen = Screen::new(800.0, 600.0);
    let mut f = |ctx: &egui::Context| {
        assert!(d.show_in(ctx, &mut s, &mut last, &mut shared));
    };
    let seen = screen.settle(8, &mut f);
    let button = find(&seen, "Speichern").expect("Speichern").rect;
    screen.click(button.center(), &mut f);
    let seen = screen.settle(4, &mut f);
    drop(f);
    let rect = d.last_rect.unwrap();
    check("Ersetzen", rect, &seen, "Ersetzen");
    let hint = seen
        .iter()
        .find(|t| t.text.starts_with("Diese Datei gibt es schon"))
        .expect("Rückfrage");
    assert!(
        hint.clip.contains_rect(hint.rect),
        "Rückfrage sichtbar: {:?}",
        hint.rect
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Höhe der ganzen Auswahl (in einem hohen Fenster) mit `drives` vorgegebenen Laufwerken.
fn chooser_height(dir: &std::path::Path, drives: usize) -> f32 {
    let mut c = planora_app::file_chooser::FileChooser::open(dir.to_path_buf(), vec!["stl"]);
    let mut shared = Places {
        drives: Some(
            (0..drives)
                .map(|i| PathBuf::from(format!("Laufwerk {i}")))
                .collect(),
        ),
        ..Places::default()
    };
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    let mut h = 0.0;
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                pos2(0.0, 0.0),
                egui::vec2(700.0, 2000.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            let top = ui.cursor().top();
            c.show_with(ui, &mut shared);
            h = ui.cursor().top() - top;
        })
        .drop_without_applying_deltas();
    }
    h
}

#[test]
fn a_second_row_of_places_comes_off_the_list() {
    let (dir, _) = populated("zeilen", 0);
    for i in 12..40 {
        fs::write(dir.join(format!("Teil {i}.stl")), b"x").unwrap();
    }
    let one = chooser_height(&dir, 1);
    let many = chooser_height(&dir, 14);
    assert!(
        many - one <= 4.0,
        "mit zweiter Zeile der Leiste kaum höher: {one} → {many}"
    );
    let _ = fs::remove_dir_all(&dir);
}
