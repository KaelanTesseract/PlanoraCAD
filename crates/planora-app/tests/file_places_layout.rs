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
