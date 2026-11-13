//! Menü „Einfügen“ und Fenster „DXF einfügen“ (Skizzen 9, Task 11): Lesen im Hintergrund mit
//! Laufnummer, Wählfeld, Einheiten, Einfügemodus, Layer, ein Rückgängig-Schritt, Hinweis zum
//! Übersprungenen, Fehler im Fenster, volle Breite und gleiche Kartenhöhe.
//!
//! Probedateien: die eigene Testdatei `basic_r12.dxf` aus `planora-exchange` (Layer „Wand“ mit
//! Linie und Kreis, Layer „Tuer“ mit einem Bogen, ohne Einheit) und im Test geschriebene Dateien.

mod common;

use common::{Screen, find};
use eframe::App as _;
use egui::{Event, Key, Modifiers, Pos2, Rect};
use planora_app::app;
use planora_app::insert::dxf_dialog::DxfDialog;
use planora_app::insert::pick::InsertTarget;
use planora_app::insert::{BackgroundRead, DialogAction, ReadSpec, dxf_spec};
use planora_app::keymap::{Action, Area, Binding, KeyChord, SetResult};
use planora_app::plane_dialog::PlaneClick;
use planora_app::shell::ShellState;
use planora_app::state::*;
use planora_app::theme;
use planora_app::workspace::Workspace;
use planora_exchange::ExchangeError;
use planora_exchange::import2d::Unit2d;
use planora_feature::{FeatureKind, SketchPlane, StandardPlane};
use planora_i18n::DisplayUnit;
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use std::fs;
use std::path::{Path, PathBuf};

const BASIC: &[u8] = include_bytes!("../../planora-exchange/tests/data/dxf/basic_r12.dxf");

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-dxf-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, bytes).unwrap();
    p
}

/// Text-DXF aus Kopfvariablen und Entitäten; jede Zeile ist „Code Wert“.
fn dxf(header: &str, entities: &str) -> Vec<u8> {
    fn pairs(s: &str, out: &mut String) {
        for line in s.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let (code, value) = line.split_once(' ').unwrap_or((line, ""));
            out.push_str(&format!("{code:>3}\n{}\n", value.trim()));
        }
    }
    let mut out = String::new();
    if !header.is_empty() {
        out.push_str("  0\nSECTION\n  2\nHEADER\n");
        pairs(header, &mut out);
        out.push_str("  0\nENDSEC\n");
    }
    out.push_str("  0\nSECTION\n  2\nENTITIES\n");
    pairs(entities, &mut out);
    out.push_str("  0\nENDSEC\n  0\nEOF\n");
    out.into_bytes()
}

fn workspace() -> Workspace {
    let root = common::temp_root("planora-dxf-ui");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Liest `path` im Dialog und wartet auf das Ergebnis.
fn loaded(dir: &Path, path: PathBuf, state: &AppState) -> DxfDialog {
    let mut d = DxfDialog::new(dir.to_path_buf(), state);
    d.load(path);
    d.tick(true);
    d
}

fn sketch_curves(s: &AppState, name: &str) -> usize {
    let (id, _) = s
        .sketch_features()
        .into_iter()
        .find(|(_, n)| n == name)
        .unwrap_or_else(|| panic!("Skizze „{name}“ fehlt: {:?}", s.sketch_features()));
    s.document.sketch_of(id).unwrap().curves().count()
}

/// Zeichnet den Dialog in jedem Bild, solange er offen ist; merkt die letzte Antwort.
fn show_on<'a>(
    d: &'a mut DxfDialog,
    s: &'a mut AppState,
    last: &'a mut DialogAction,
) -> impl FnMut(&egui::Context) + 'a {
    let mut dir = None;
    move |ctx| {
        if *last == DialogAction::Open {
            *last = d.show(
                ctx,
                s,
                &mut dir,
                &mut planora_app::places::Places::default(),
            );
        }
    }
}

/// Gibt es in `sk` einen Punkt bei `at`?
fn has_point(sk: &planora_sketch::Sketch, at: DVec2) -> bool {
    sk.points()
        .any(|(p, _)| sk.point_xy(p).is_ok_and(|q| (q - at).length() < 1e-9))
}

fn all(seen: &[common::Seen]) -> String {
    seen.iter().map(|t| t.text.clone() + "\n").collect()
}

// ---- Menü -----------------------------------------------------------------------------------

fn insert_menu_frame<'a>(ws: &'a mut Workspace) -> impl FnMut(&egui::Context) + 'a {
    move |ctx| {
        egui::Area::new(egui::Id::new("einfuegen"))
            .fixed_pos(Pos2::new(20.0, 20.0))
            .show(ctx, |ui| {
                ui.set_width(280.0);
                theme::menu_style(ui.style_mut());
                app::insert_menu(ui, ws);
            });
    }
}

#[test]
fn insert_menu_has_three_entries_after_create() {
    // In der Menüleiste: „Einfügen“ direkt nach „Erzeugen“ und vor „Zeichnung“.
    let mut ws = workspace();
    let mut shell = ShellState::default();
    let mut screen = Screen::new(1920.0, 1080.0);
    let seen = screen.settle(3, &mut |ctx: &egui::Context| {
        egui::Area::new(egui::Id::new("leiste"))
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(1800.0);
                app::menu_bar(ui, &mut ws, &mut shell);
            });
    });
    let create = find(&seen, "Erzeugen").expect("Erzeugen").rect;
    let insert = find(&seen, "Einfügen").expect("Einfügen").rect;
    let drawing = find(&seen, "Zeichnung").expect("Zeichnung").rect;
    assert!(
        (insert.center().y - create.center().y).abs() < 1.0,
        "eine Zeile"
    );
    assert!(create.right() < insert.left() && insert.right() < drawing.left());
    for t in &seen {
        let x = t.rect.center().x;
        assert!(
            !(x > create.right() && x < insert.left()) || t.text.trim().is_empty(),
            "„{}“ zwischen Erzeugen und Einfügen",
            t.text
        );
    }

    // Das Menü selbst: drei Einträge untereinander.
    let mut ws = workspace();
    let mut screen = Screen::new(1920.0, 1080.0);
    let seen = screen.settle(3, &mut insert_menu_frame(&mut ws));
    let dxf = find(&seen, "DXF einfügen …").expect("DXF").rect;
    let svg = find(&seen, "SVG einfügen …").expect("SVG").rect;
    let canvas = find(&seen, "Leinwand …").expect("Leinwand").rect;
    assert!(
        dxf.top() < svg.top() && svg.top() < canvas.top(),
        "Reihenfolge"
    );
    // SVG öffnet sein Fenster (Task 12), die Leinwand noch nichts (Task 13).
    screen.click(svg.center(), &mut insert_menu_frame(&mut ws));
    assert!(ws.active_tab().dxf_dialog.is_none());
    assert!(
        ws.active_tab().svg_dialog.is_some(),
        "SVG öffnet das Fenster"
    );
    // „Leinwand …“ öffnet ohne offene Skizze sein Fenster (Task 13).
    let mut fresh = workspace();
    screen.settle(2, &mut insert_menu_frame(&mut fresh));
    screen.click(canvas.center(), &mut insert_menu_frame(&mut fresh));
    let tab = fresh.active_tab();
    assert!(
        tab.dxf_dialog.is_none() && tab.svg_dialog.is_none(),
        "nur das Leinwand-Fenster"
    );
    assert!(tab.canvas_dialog.is_some(), "Leinwand öffnet das Fenster");
    assert!(tab.state.message.is_none() && tab.state.info.is_none());
    screen.click(dxf.center(), &mut insert_menu_frame(&mut ws));
    assert!(
        ws.active_tab().dxf_dialog.is_some(),
        "DXF öffnet das Fenster"
    );

    // Die Aktionen stehen in der Tabelle, ohne Vorgabetaste.
    for (a, id, label) in [
        (Action::InsertDxf, "einfuegen.dxf", "DXF einfügen"),
        (Action::InsertSvg, "einfuegen.svg", "SVG einfügen"),
        (Action::InsertCanvas, "einfuegen.leinwand", "Leinwand"),
    ] {
        assert!(Action::ALL.contains(&a));
        assert_eq!(a.id(), id);
        assert_eq!(a.label(), label);
        assert!(a.default_bindings().is_empty(), "{a:?}");
    }
    // DXF und SVG gelten auch in der offenen Skizze, die Leinwand nur im Modell.
    assert_eq!(Action::InsertDxf.area(), Area::All);
    assert_eq!(Action::InsertSvg.area(), Area::All);
    assert_eq!(Action::InsertCanvas.area(), Area::Model);
    // Die Taste der Aktion öffnet das Fenster ebenso.
    let mut ws = workspace();
    planora_app::commands::run_action(&mut ws, Action::InsertDxf);
    assert!(ws.active_tab().dxf_dialog.is_some());
}

// ---- Abbrechen ------------------------------------------------------------------------------

#[test]
fn dxf_dialog_cancel_leaves_document_unchanged() {
    let dir = temp_dir("abbrechen");
    let path = write(&dir, "plan.dxf", BASIC);
    let mut s = AppState::new();
    s.select_origin_plane(StandardPlane::XY);
    let mut d = loaded(&dir, path, &s);
    // Vorauswahl: die gewählte Ursprungsebene steht schon im Wählfeld.
    assert_eq!(
        d.target.target(),
        Some(InsertTarget::Plane(SketchPlane::Standard(
            StandardPlane::XY
        )))
    );
    let before = s.snapshot();
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(1920.0, 1080.0);
    let seen = screen.settle(4, &mut show_on(&mut d, &mut s, &mut last));
    // Die Vorschau liegt in der Ansicht, das Wählfeld nimmt Klicks.
    let preview = s.insert_preview.as_ref().expect("Vorschau");
    assert_eq!(preview.lines.len(), 3, "Linie, Kreis, Bogen");
    assert!(s.plane_pick.is_some());
    let cancel = find(&seen, "Abbrechen").expect("Abbrechen").rect;
    screen.click(cancel.center(), &mut show_on(&mut d, &mut s, &mut last));
    assert_eq!(last, DialogAction::Closed);
    assert!(s.snapshot().same_content(&before), "nichts geändert");
    assert!(s.sketch_features().is_empty());
    assert!(
        s.insert_preview.is_none() && s.plane_pick.is_none(),
        "aufgeräumt"
    );

    // Esc schließt ebenso, ohne Spur.
    let mut d = loaded(&dir, dir.join("plan.dxf"), &s);
    let mut last = DialogAction::Open;
    screen.settle(3, &mut show_on(&mut d, &mut s, &mut last));
    screen.frame(
        vec![Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        &mut show_on(&mut d, &mut s, &mut last),
    );
    assert_eq!(last, DialogAction::Closed);
    assert!(s.snapshot().same_content(&before));
    assert!(s.insert_preview.is_none());
    let _ = fs::remove_dir_all(&dir);
}

// ---- Eine Skizze ------------------------------------------------------------------------------

#[test]
fn dxf_single_sketch_one_undo_step() {
    let dir = temp_dir("eine");
    let path = write(&dir, "plan.dxf", BASIC);
    let mut ws = workspace();
    let tab = ws.active_tab_mut();
    let mut d = loaded(&dir, path, &tab.state);
    assert!(d.single, "Vorgabe: Eine Skizze");
    assert_eq!(d.layers, vec![true, true], "alle Layer angehakt");
    // Ohne Ebene geht nichts.
    assert!(d.target.target().is_none());
    assert!(d.insert(&mut tab.state).is_err());
    assert!(tab.state.sketch_features().is_empty());
    assert!(
        d.target
            .take_click(&tab.state, PlaneClick::Standard(StandardPlane::XZ))
    );
    d.offset = DVec2::new(5.0, 0.0);
    d.insert(&mut tab.state).unwrap();
    ws.observe_history(false);
    let s = &ws.active_tab().state;
    assert_eq!(s.sketch_features().len(), 1);
    let (id, name) = s.sketch_features()[0].clone();
    assert_eq!(name, "Skizze 1");
    assert_eq!(sketch_curves(s, "Skizze 1"), 3);
    let f = s.document.feature(id).unwrap();
    match &f.kind {
        FeatureKind::Sketch(def) => {
            assert_eq!(def.plane, SketchPlane::Standard(StandardPlane::XZ));
        }
        k => panic!("keine Skizze: {k:?}"),
    }
    // X-Abstand: die Linie beginnt bei (5, 0).
    let sk = s.document.sketch_of(id).unwrap();
    assert!(has_point(sk, DVec2::new(5.0, 0.0)), "versetzt");
    // Ein Rückgängig nimmt alles zurück.
    let tab = ws.active_tab_mut();
    tab.undo().unwrap();
    assert!(tab.state.sketch_features().is_empty(), "ein Schritt");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dxf_skipped_summary_names_only_present_kinds() {
    let dir = temp_dir("hinweis");
    let ent = "0 DIMENSION\n8 M\n2 *D1\n10 0\n20 0\n30 0\n11 0\n21 0\n31 0\n70 0\n13 0\n23 0\n33 0\n14 5\n24 0\n34 0\n\
               0 HATCH\n8 S\n10 0\n20 0\n30 0\n210 0\n220 0\n230 1\n2 SOLID\n70 1\n91 0\n\
               0 LINE\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0";
    let path = write(&dir, "mass.dxf", &dxf("", ent));
    let mut s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    d.target
        .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
    d.insert(&mut s).unwrap();
    let info = s.info.clone().expect("Hinweis");
    assert_eq!(info, "1 Bemaßung und 1 Schraffur übersprungen.");
    assert!(s.info_outlives_step);

    // Nichts übersprungen: kein Hinweis.
    let path = write(&dir, "plan.dxf", BASIC);
    let mut s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    d.target
        .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
    d.insert(&mut s).unwrap();
    assert_eq!(s.info, None);
    let _ = fs::remove_dir_all(&dir);
}

// ---- Pro Layer --------------------------------------------------------------------------------

#[test]
fn dxf_per_layer_names_sketches_after_layers() {
    let dir = temp_dir("layer");
    let path = write(&dir, "plan.dxf", BASIC);
    let mut ws = workspace();
    let tab = ws.active_tab_mut();
    let mut d = loaded(&dir, path, &tab.state);
    d.single = false;
    d.target
        .take_click(&tab.state, PlaneClick::Standard(StandardPlane::XY));
    d.insert(&mut tab.state).unwrap();
    ws.observe_history(false);
    let s = &ws.active_tab().state;
    let names: Vec<String> = s.sketch_features().into_iter().map(|(_, n)| n).collect();
    assert_eq!(names, ["Wand", "Tuer"]);
    assert_eq!(sketch_curves(s, "Wand"), 2);
    assert_eq!(sketch_curves(s, "Tuer"), 1);
    // Beide Skizzen in einem Schritt.
    let tab = ws.active_tab_mut();
    tab.undo().unwrap();
    assert!(tab.state.sketch_features().is_empty(), "ein Schritt");

    // Nur angehakte Layer bekommen eine Skizze.
    let mut s = AppState::new();
    let mut d = loaded(&dir, dir.join("plan.dxf"), &s);
    d.single = false;
    d.layers = vec![false, true];
    d.target
        .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
    d.insert(&mut s).unwrap();
    let names: Vec<String> = s.sketch_features().into_iter().map(|(_, n)| n).collect();
    assert_eq!(names, ["Tuer"]);
    // Kein Layer angehakt: Meldung, nichts angelegt.
    let mut s = AppState::new();
    let mut d = loaded(&dir, dir.join("plan.dxf"), &s);
    d.layers = vec![false, false];
    d.target
        .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
    assert!(d.insert(&mut s).is_err());
    assert!(s.sketch_features().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

// ---- Offene Skizze ----------------------------------------------------------------------------

#[test]
fn dxf_into_open_sketch_disables_per_layer() {
    let dir = temp_dir("offen");
    let path = write(&dir, "plan.dxf", BASIC);
    let mut ws = workspace();
    let sketch = {
        let s = &mut ws.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        s.session_sketch_mut()
            .unwrap()
            .1
            .add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0))
            .unwrap();
        id
    };
    ws.observe_history(false);
    let tab = ws.active_tab_mut();
    let mut d = loaded(&dir, path, &tab.state);
    assert!(d.into_open());
    assert_eq!(d.target.target(), Some(InsertTarget::OpenSketch));

    // Im Fenster: „pro Layer“ gesperrt (ein Klick ändert nichts), kein Wählfeld.
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(1920.0, 1080.0);
    let seen = screen.settle(4, &mut show_on(&mut d, &mut tab.state, &mut last));
    let text = all(&seen);
    assert!(text.contains("In die offene Skizze"), "{text}");
    assert!(!text.contains("Ebene oder ebene Fläche wählen"), "{text}");
    let per_layer = find(&seen, "Eine Skizze pro Layer")
        .expect("pro Layer")
        .rect;
    screen.click(
        per_layer.center(),
        &mut show_on(&mut d, &mut tab.state, &mut last),
    );
    assert!(d.single, "gesperrt");
    assert!(
        tab.state.plane_pick.is_none(),
        "kein Wählfeld in der Skizze"
    );

    // Auch mit `single = false` landet alles in der offenen Skizze.
    d.single = false;
    d.insert(&mut tab.state).unwrap();
    assert_eq!(tab.state.sketch_features().len(), 1, "keine neue Skizze");
    assert_eq!(tab.state.session_sketch().unwrap().curves().count(), 4 + 3);
    ws.observe_history(false);
    assert_eq!(
        ws.active_tab().undo_label(),
        Some("DXF einfügen".to_string())
    );
    let tab = ws.active_tab_mut();
    tab.undo().unwrap();
    assert_eq!(tab.state.session_sketch().unwrap().curves().count(), 4);
    assert_eq!(tab.state.open_sketch_id(), Some(sketch));
    let _ = fs::remove_dir_all(&dir);
}

// ---- Einheiten ----------------------------------------------------------------------------------

#[test]
fn dxf_no_unit_preselects_display_unit_with_hint() {
    let dir = temp_dir("einheit");
    let path = write(&dir, "plan.dxf", BASIC);
    let mut s = AppState::new();
    // Anzeigeeinheit Zoll: Zoll vorgewählt.
    let mut d = planora_i18n::with_unit(DisplayUnit::Zoll, || loaded(&dir, path.clone(), &s));
    assert_eq!(d.units, Unit2d::Inch);
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(1920.0, 1080.0);
    let text = all(&screen.settle(4, &mut show_on(&mut d, &mut s, &mut last)));
    assert!(text.contains("Die Datei nennt keine Einheit."), "{text}");
    for u in ["Millimeter", "Zentimeter", "Meter", "Zoll", "Fuß"] {
        assert!(text.lines().any(|l| l == u), "„{u}“ fehlt:\n{text}");
    }
    // Zoll skaliert: die Linie von 10 Zoll ist 254 mm lang.
    d.target
        .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
    d.insert(&mut s).unwrap();
    let id = s.sketch_features()[0].0;
    let sk = s.document.sketch_of(id).unwrap();
    assert!(has_point(sk, DVec2::new(254.0, 0.0)), "254 mm");

    // Anzeigeeinheit Millimeter: Millimeter vorgewählt.
    let s = AppState::new();
    let d = planora_i18n::with_unit(DisplayUnit::Millimeter, || loaded(&dir, path.clone(), &s));
    assert_eq!(d.units, Unit2d::Millimeter);

    // Nennt die Datei ihre Einheit, gilt sie, ohne Hinweis.
    let meter = write(
        &dir,
        "meter.dxf",
        &dxf(
            "9 $INSUNITS\n70 6",
            "0 LINE\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0",
        ),
    );
    let mut s = AppState::new();
    let mut d = planora_i18n::with_unit(DisplayUnit::Zoll, || loaded(&dir, meter, &s));
    assert_eq!(d.units, Unit2d::Meter);
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(1920.0, 1080.0);
    let text = all(&screen.settle(4, &mut show_on(&mut d, &mut s, &mut last)));
    assert!(!text.contains("Die Datei nennt keine Einheit."), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

// ---- Laufnummer -------------------------------------------------------------------------------

#[test]
fn cancelled_read_result_never_reaches_new_dialog() {
    let dir = temp_dir("lauf");
    let first = write(&dir, "eins.dxf", BASIC);
    let second = write(
        &dir,
        "zwei.dxf",
        &dxf("", "0 LINE\n8 Neu\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0"),
    );
    // Der Leser selbst: nach Abbrechen kommt nie ein Ergebnis, ein neuer Lauf hat eine neue
    // Nummer und liefert nur sein eigenes.
    let mut a = BackgroundRead::start(first.clone(), dxf_spec());
    a.cancel();
    assert!(!a.is_running());
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(a.poll().is_none(), "abgebrochen");
    let mut b = BackgroundRead::start(second.clone(), dxf_spec());
    assert_ne!(a.run(), b.run());
    let got = b.wait().expect("Ergebnis").unwrap();
    assert_eq!(got.layers[0].name, "Neu");
    assert!(b.poll().is_none(), "nur einmal");

    // Im Fenster: Abbrechen, dann ein neues Fenster mit anderer Datei.
    let s = AppState::new();
    let mut d = DxfDialog::new(dir.clone(), &s);
    d.load(first.clone());
    d.cancel();
    let mut e = DxfDialog::new(dir.clone(), &s);
    e.load(second.clone());
    e.tick(true);
    let l = e.loaded.as_ref().expect("gelesen");
    assert_eq!(l.import.layers[0].name, "Neu");
    // Im selben Fenster ersetzt eine neue Wahl das laufende Lesen.
    let mut f = DxfDialog::new(dir.clone(), &s);
    f.load(first);
    f.load(second);
    f.tick(true);
    assert_eq!(f.loaded.as_ref().unwrap().import.layers[0].name, "Neu");
    assert_eq!(f.layers, vec![true]);
    let _ = fs::remove_dir_all(&dir);
}

// ---- Fehler ---------------------------------------------------------------------------------------

#[test]
fn broken_dxf_shows_error_dialog_stays_open() {
    let dir = temp_dir("kaputt");
    let path = write(
        &dir,
        "kaputt.dxf",
        b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nabc\n",
    );
    let mut s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    assert!(d.loaded.is_none());
    let error = d.error_text().expect("Meldung");
    assert!(!error.is_empty());
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(1920.0, 1080.0);
    let text = all(&screen.settle(4, &mut show_on(&mut d, &mut s, &mut last)));
    assert_eq!(last, DialogAction::Open, "bleibt offen");
    assert!(text.contains(&error), "{text}");
    assert!(s.sketch_features().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

// ---- Fenster --------------------------------------------------------------------------------------

#[test]
fn dialog_full_width_rule_and_equal_cards() {
    let dir = temp_dir("fenster");
    let path = write(&dir, "plan.dxf", BASIC);
    for dark in [false, true] {
        let mut s = AppState::new();
        let mut d = loaded(&dir, path.clone(), &s);
        let mut last = DialogAction::Open;
        let mut screen = Screen::new(1920.0, 1080.0);
        theme::apply(&screen.ctx, dark);
        let seen = screen.settle(8, &mut show_on(&mut d, &mut s, &mut last));
        let rect = d.last_rect.expect("gezeichnet");
        let window = Rect::from_min_size(Pos2::ZERO, screen.size);
        assert!(window.contains_rect(rect), "{rect:?}");
        let wanted = planora_app::dialog::limits(window, planora_app::dialog::DialogSize::File).0;
        assert!(
            (rect.width() - wanted).abs() <= 1.0,
            "volle Breite {wanted}: {rect:?}"
        );
        assert!((rect.center().x - 960.0).abs() <= 1.0, "mittig: {rect:?}");
        assert_eq!(d.last_cards.len(), 2, "zwei Karten");
        common::assert_cards_inside(rect, &d.last_cards, &seen);
        let text = all(&seen);
        for word in [
            "DXF einfügen",
            "plan.dxf",
            "Einheiten",
            "Einfügemodus",
            "Eine Skizze",
            "Eine Skizze pro Layer",
            "X-Abstand",
            "Y-Abstand",
            "DXF-Layer",
            "Wand",
            "Tuer",
            "Kontrollpunktrahmen",
            "OK",
            "Abbrechen",
        ] {
            assert!(text.lines().any(|l| l == word), "„{word}“ fehlt:\n{text}");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

// ---- Taste in der Skizze ----------------------------------------------------------------------

fn chord(key: Key, ctrl: bool, shift: bool) -> KeyChord {
    KeyChord {
        key,
        ctrl,
        shift,
        alt: false,
    }
}

/// Ganze App ohne Fenster; ein Bild je Aufruf.
struct Rig {
    ctx: egui::Context,
    app: app::PlanoraApp,
    time: f64,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        theme::apply(&ctx, true);
        app::PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: app::PlanoraApp::for_tests(w),
            time: 0.0,
        };
        for _ in 0..3 {
            rig.frame(vec![]);
        }
        rig
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1920.0, 1080.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        out.drop_without_applying_deltas();
    }

    fn press(&mut self, c: KeyChord) {
        let modifiers = Modifiers {
            ctrl: c.ctrl,
            command: c.ctrl,
            shift: c.shift,
            ..Modifiers::NONE
        };
        self.frame(vec![Event::Key {
            key: c.key,
            physical_key: Some(c.key),
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        self.frame(vec![]);
    }
}

#[test]
fn an_assigned_insert_dxf_key_works_in_an_open_sketch() {
    let mut ws = workspace();
    // Eine Taste der Skizze ist für „DXF einfügen“ belegt (Bereich „Überall“).
    assert_eq!(
        ws.keymap.set(
            Action::InsertDxf,
            0,
            Binding::Key(chord(Key::L, false, false))
        ),
        SetResult::Conflict(Action::SketchLine)
    );
    let key = chord(Key::I, true, true);
    assert_eq!(
        ws.keymap.set(Action::InsertDxf, 0, Binding::Key(key)),
        SetResult::Ok
    );
    let sketch = {
        let s = &mut ws.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        id
    };
    ws.observe_history(false);
    let mut rig = Rig::new(ws);
    rig.press(key);
    let tab = rig.app.ws_mut().active_tab();
    let d = tab.dxf_dialog.as_ref().expect("Fenster offen");
    assert!(d.into_open(), "in die offene Skizze");
    assert_eq!(
        tab.state.open_sketch_id(),
        Some(sketch),
        "Skizze bleibt offen"
    );
}

// ---- Größengrenze -----------------------------------------------------------------------------

/// Leser, der nie aufgerufen werden darf.
fn must_not_read(_: &[u8], _: &dyn Fn() -> bool) -> Result<u8, ExchangeError> {
    Err(ExchangeError::Corrupt("gelesen".into()))
}

#[test]
fn a_file_over_the_reader_limit_is_refused_before_reading() {
    let dir = temp_dir("grenze");
    let path = write(&dir, "gross.dxf", &[b'x'; 100]);
    let spec = ReadSpec {
        max_bytes: 99,
        too_large: ExchangeError::DxfTooLarge,
        read: must_not_read,
        broken: ExchangeError::DxfBroken,
    };
    let mut r = BackgroundRead::start(path.clone(), spec.clone());
    assert_eq!(r.wait(), Some(Err(ExchangeError::DxfTooLarge)));
    // Genau an der Grenze wird gelesen.
    let mut r = BackgroundRead::start(
        path,
        ReadSpec {
            max_bytes: 100,
            ..spec
        },
    );
    assert_eq!(
        r.wait(),
        Some(Err(ExchangeError::Corrupt("gelesen".into())))
    );
    // Der DXF-Leser hat seine eigene Grenze, wie SVG (256 MiB).
    assert_eq!(dxf_spec().max_bytes, planora_exchange::MAX_DXF_BYTES);
    assert_eq!(planora_exchange::MAX_DXF_BYTES, 256 * 1024 * 1024);
    let _ = fs::remove_dir_all(&dir);
}

/// Gesamtprüfung M1: In einer offenen Skizze schließt ein Esc das Fenster „DXF einfügen“, das
/// Skizzenwerkzeug bleibt (ein Esc wirkt nicht zweimal). Ein zweites Esc gilt wieder der Skizze:
/// Es beendet das leere Bemaßen (wie in Fusion).
#[test]
fn one_esc_closes_the_dialog_but_not_the_sketch_tool() {
    let mut ws = workspace();
    let key = chord(Key::I, true, true);
    assert_eq!(
        ws.keymap.set(Action::InsertDxf, 0, Binding::Key(key)),
        SetResult::Ok
    );
    {
        let s = &mut ws.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, planora_app::sketch_state::Tool::Dimension);
    }
    ws.observe_history(false);
    let mut rig = Rig::new(ws);
    rig.press(key);
    assert!(rig.app.ws_mut().active_tab().dxf_dialog.is_some());
    let esc = KeyChord {
        key: Key::Escape,
        ctrl: false,
        shift: false,
        alt: false,
    };
    rig.press(esc);
    let tab = rig.app.ws_mut().active_tab();
    assert!(tab.dxf_dialog.is_none(), "Fenster zu");
    let tool = tab.state.sketch_session.as_ref().map(|se| se.tool);
    assert_eq!(
        tool,
        Some(planora_app::sketch_state::Tool::Dimension),
        "Werkzeug bleibt"
    );
    rig.press(esc);
    let tab = rig.app.ws_mut().active_tab();
    let tool = tab.state.sketch_session.as_ref().map(|se| se.tool);
    assert_ne!(
        tool,
        Some(planora_app::sketch_state::Tool::Dimension),
        "zweites Esc beendet das leere Bemaßen"
    );
}

/// Gesamtprüfung M2: Eine DXF ohne ein einziges brauchbares Element (nur Schraffur) nennt den
/// Grund samt Übersprungenem, statt nur OK zu sperren.
#[test]
fn a_dxf_without_usable_elements_names_the_reason() {
    let dxf = "0\nSECTION\n2\nENTITIES\n0\nHATCH\n8\n0\n0\nENDSEC\n0\nEOF\n";
    let imp = planora_exchange::read_dxf(dxf.as_bytes(), &|| false).unwrap();
    assert_eq!(imp.element_count(), 0);
    let why = planora_app::insert::nothing_usable(&imp).expect("Grund");
    assert!(why.contains("nichts"), "{why}");
    assert!(why.contains("Schraffur"), "{why}");
}
