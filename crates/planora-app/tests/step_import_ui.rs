//! Fenster „STEP importieren“ (STEP-Import, Aufgabe 14): Lesen und Prüfen im Arbeitsthread,
//! Liste der Körper mit Häkchen, Einheit der Datei, Abbrechen, Fehler, ein Schritt für
//! Rückgängig, veraltete Ergebnisse, echte Tasten, volle Breite und gleiche Kartenhöhe, Menü.
//!
//! Probedateien: Kopien der eigenen Testdateien aus `crates/planora-step/tests/data` (im
//! Temp-Ordner, teils zusammengesetzt) und im Test geschriebene Dateien.

mod common;

use common::{Screen, find};
use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::app::{self, PlanoraApp};
use planora_app::import_dialog::{STALE, Unit};
use planora_app::jobs::Job;
use planora_app::state::*;
use planora_app::step_import_dialog::*;
use planora_app::workspace::Workspace;
use planora_app::{fonts, properties, theme};
use planora_exchange::Progress;
use planora_feature::{SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_step::{LengthUnit, StepHeader, StepImport, UnitState};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ---- Probedateien ----------------------------------------------------------------------------

/// Eigene Testdatei aus `planora-step` (nur gelesen).
fn step_data(name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "planora-step",
        "tests",
        "data",
        name,
    ]
    .iter()
    .collect();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Ohne Kommentare `/* … */` (auch über mehrere Zeilen).
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let end = rest[start..].find("*/").expect("Kommentar geschlossen");
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

/// Datenteil einer Datei (zwischen `DATA;` und dem letzten `ENDSEC;`), ohne Kommentare.
fn data_part(text: &str) -> String {
    let start = text.find("DATA;").expect("DATA") + "DATA;".len();
    let end = text.rfind("ENDSEC;").expect("ENDSEC");
    strip_comments(&text[start..end])
}

/// Ersetzt jede Nummer `#n` durch `#(n + off)`.
fn shift_ids(text: &str, off: u64) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '#' {
            let mut digits = String::new();
            while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                digits.push(*d);
                chars.next();
            }
            let n: u64 = digits.parse().expect("Nummer");
            let _ = write!(out, "{}", n + off);
        }
    }
    out
}

fn wrap(data: &str) -> String {
    format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
         FILE_NAME('probe.step','2026-10-23T00:00:00',(''),(''),'','','');\n\
         FILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 1 1 1 1 }}'));\nENDSEC;\nDATA;\n{data}\n\
         ENDSEC;\nEND-ISO-10303-21;\n"
    )
}

/// Quader aus `box_ap214.step` ohne Deckel, Name „Deckellos“ (Nummern + `off`).
fn lidless(off: u64) -> String {
    let lid = format!("(#{},#{},#{},", off + 30, off + 31, off + 32);
    let text = shift_ids(&data_part(&step_data("box_ap214.step")), off)
        .replace("W\\X2\\00FC\\X0\\rfel", "Deckellos");
    assert!(text.contains(&lid), "Schale gefunden");
    text.replace(&lid, &format!("(#{},#{},", off + 30, off + 32))
}

/// `inch_assembly.step` (Block, Block (2), Stift in Zoll) und der Quader ohne Deckel (mm).
fn combined() -> String {
    wrap(&format!(
        "{}\n{}",
        data_part(&step_data("inch_assembly.step")),
        lidless(100_000)
    ))
}

/// Wie [`combined`], dazu ein Facettenkörper, der nicht übernommen wird.
fn combined_with_facets() -> String {
    wrap(&format!(
        "{}\n{}\n#900001=FACETED_BREP('Netz',#900002);\n#900002=CLOSED_SHELL('',());",
        data_part(&step_data("inch_assembly.step")),
        lidless(100_000)
    ))
}

/// `mapped_item.step` ohne Namen: zwei unbenannte Körper, der zweite gespiegelt (Warnung).
fn unnamed_mapped() -> String {
    step_data("mapped_item.step")
        .replace("'Teil'", "''")
        .replace("'Teil-1'", "''")
        .replace("'Teil-2'", "''")
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-step-ui-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, text).unwrap();
    p
}

/// Fenster mit gelesener und geprüfter Datei `text` (ohne Bindung an ein Dokument).
fn loaded(dir: &Path, name: &str, text: &str) -> StepImportDialog {
    let mut d = StepImportDialog::new(dir.to_path_buf());
    d.load(write(dir, name, text));
    d.tick(true);
    assert!(d.loaded.is_some(), "gelesen: {:?}", d.error);
    d
}

fn reason(d: &StepImportDialog, i: usize) -> String {
    match &d.loaded.as_ref().unwrap().import.bodies[i].outcome {
        Ok(_) => panic!("Körper {i} wurde übernommen"),
        Err(e) => e.clone(),
    }
}

// ---- Zeichnen ---------------------------------------------------------------------------------

/// Ein gezeichneter Text mit Rechteck und Farbe (wie gemalt).
#[derive(Clone, Debug)]
struct Shown {
    text: String,
    rect: Rect,
    color: egui::Color32,
}

fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Shown>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        egui::epaint::Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |x| x.format.color);
            if color == egui::Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            if let Some(c) = t.override_text_color {
                color = c;
            }
            out.push(Shown {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                color,
            });
        }
        _ => {}
    }
}

/// Mehrere Bilder des Fensters (1200 × 800, Thema `dark`); liefert die Texte des letzten.
fn draw_in(d: &mut StepImportDialog, s: &mut AppState, dark: bool) -> Vec<Shown> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, dark);
    let mut dir = None;
    let mut seen = Vec::new();
    for i in 0..4 {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
            time: Some(i as f64 * 0.05),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            assert!(
                d.show_in(
                    ui.ctx(),
                    s,
                    &mut dir,
                    &mut planora_app::places::Places::default()
                ),
                "Fenster bleibt offen"
            );
        });
        seen.clear();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
    }
    seen
}

fn draw(d: &mut StepImportDialog, s: &mut AppState) -> String {
    let seen = draw_in(d, s, false);
    seen.iter().map(|t| t.text.clone() + "\n").collect()
}

fn all(seen: &[common::Seen]) -> String {
    seen.iter().map(|t| t.text.clone() + "\n").collect()
}

fn show_on<'a>(
    d: &'a mut StepImportDialog,
    s: &'a mut AppState,
    open: &'a mut bool,
) -> impl FnMut(&egui::Context) + 'a {
    let mut dir = None;
    move |ctx| {
        if *open {
            *open = d.show_in(
                ctx,
                s,
                &mut dir,
                &mut planora_app::places::Places::default(),
            );
        }
    }
}

// ---- (a) Liste ----------------------------------------------------------------------------------

#[test]
fn list_shows_every_body() {
    let dir = temp_dir("liste");
    let mut d = loaded(&dir, "gestell.step", &combined_with_facets());
    let mut s = AppState::new();
    let l = d.loaded.as_ref().unwrap();
    let names: Vec<&str> = l.import.bodies.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Block", "Block (2)", "Stift", "Deckellos"]);
    assert_eq!(
        d.checked,
        [true, true, true, false],
        "Vorgabe: Ok-Körper an"
    );
    let lid = reason(&d, 3);
    let text = draw(&mut d, &mut s);
    for word in [
        TITLE,
        "Datei",
        "gestell.step",
        "Körper",
        "Name",
        "Flächen",
        "Größe",
        "Einheit",
        "Status",
        "Einheit der Datei",
        "Block",
        "Block (2)",
        "Stift",
        "Deckellos",
        "25,4 × 50,8 × 76,2 mm",
        "50,8 × 25,4 × 76,2 mm",
        "Zoll",
        "wird übernommen",
        "Importieren",
        "Schließen",
    ] {
        assert!(text.lines().any(|l| l == word), "„{word}“ fehlt:\n{text}");
    }
    let skipped = format!("wird ausgelassen: {lid}");
    assert!(
        skipped.starts_with("wird ausgelassen: Der Körper ist ungültig: Die Kante bei (")
            && skipped.ends_with("gehört nur zu einer Fläche."),
        "{skipped}"
    );
    assert!(text.lines().any(|l| l == skipped), "{skipped}\n{text}");
    // Die Hülle des Stifts stammt aus einem Netz (bis 0,05 mm je Seite zu klein): „etwa“.
    let pin = text
        .lines()
        .find(|l| l.ends_with("× 50,8 mm") && l.starts_with("etwa "))
        .unwrap_or_else(|| {
            panic!(
                "Stift ungefähr:
{text}"
            )
        });
    assert!(pin.starts_with("etwa 12,"), "{pin}");
    assert!(
        !text.lines().any(|l| l == "etwa 25,4 × 50,8 × 76,2 mm"),
        "Quader genau"
    );
    // Flächen je Zeile: Quader 6 (auch der ausgelassene zählt seine 5 aus der Datei).
    assert!(text.lines().any(|l| l == "6"), "{text}");
    assert!(text.lines().any(|l| l == "5"), "{text}");
    // Die nicht übernommene Körperart der Datei wird genannt.
    assert!(
        text.contains("(FACETED_BREP); diese Art wird noch nicht unterstützt und ausgelassen."),
        "{text}"
    );
    // Die Datei nennt ihre Einheit: kein Hinweis.
    assert!(!text.contains("keine Längeneinheit"), "{text}");
    assert!(s.document.features().is_empty(), "Zeichnen ändert nichts");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn colors_numbering_and_size_text() {
    for dark in [false, true] {
        let dir = temp_dir(&format!("farben-{dark}"));
        let p = theme::palette(dark);
        let mut s = AppState::new();
        // Ein vorhandener „STEP-Körper 4“ im Dokument: neue Nummern liegen darüber.
        s.create_box(DVec3::splat(5.0)).unwrap();
        let id = s.document.features()[0].id;
        s.rename_feature(id, "STEP-Körper 4").unwrap();
        let mut d = loaded(&dir, "spiegel.step", &unnamed_mapped());
        let l = d.loaded.as_ref().unwrap();
        assert_eq!(l.import.bodies.len(), 2);
        assert!(l.import.bodies.iter().all(|b| b.name.is_empty()));
        assert!(l.import.bodies.iter().all(|b| b.outcome.is_ok()));
        assert!(l.import.bodies[1].placement_reason.is_some(), "gespiegelt");
        let seen = draw_in(&mut d, &mut s, dark);
        let one = seen.iter().find(|t| t.text == "STEP-Körper 5");
        let two = seen.iter().find(|t| t.text == "STEP-Körper 6");
        assert!(one.is_some() && two.is_some(), "{seen:#?}");
        let warned = seen
            .iter()
            .find(|t| t.text.starts_with("wird übernommen – "))
            .expect("Warnung");
        assert_eq!(
            warned.text,
            "wird übernommen – Die Lage von „STEP-Körper 6“ ließ sich nicht bestimmen \
             (Spiegelung); der Körper liegt am Ursprung."
        );
        assert_eq!(warned.color, p.warning, "Warnung in Warnfarbe");
        let plain = seen
            .iter()
            .find(|t| t.text == "wird übernommen")
            .expect("ohne Warnung");
        assert_ne!(plain.color, p.warning);
        assert_ne!(plain.color, p.error);
        // Der Import vergibt dieselben Namen, die die Liste zeigt.
        d.import(&mut s).unwrap();
        let names: Vec<String> = s
            .document
            .features()
            .iter()
            .map(|f| f.name.clone())
            .collect();
        assert_eq!(names, ["STEP-Körper 4", "STEP-Körper 5", "STEP-Körper 6"]);
        let _ = fs::remove_dir_all(&dir);

        // Ausgelassen in Fehlerfarbe.
        let dir = temp_dir(&format!("fehlerfarbe-{dark}"));
        let mut s = AppState::new();
        let mut d = loaded(&dir, "g.step", &combined());
        let seen = draw_in(&mut d, &mut s, dark);
        let skipped = seen
            .iter()
            .find(|t| t.text.starts_with("wird ausgelassen: "))
            .expect("ausgelassen");
        assert_eq!(skipped.color, p.error, "ausgelassen in Fehlerfarbe");
        let _ = fs::remove_dir_all(&dir);
    }
    assert_eq!(
        size_text(Some((DVec3::ZERO, DVec3::new(25.4, 50.8, 76.2)))),
        "25,4 × 50,8 × 76,2 mm"
    );
    assert_eq!(
        size_text(Some((DVec3::splat(-1.0), DVec3::new(0.0, 1.0, 2.0)))),
        "1 × 2 × 3 mm"
    );
    // Auf 0,1 mm gerundet (die Hülle gekrümmter Körper ist bis 0,05 mm zu klein).
    assert_eq!(
        size_text(Some((DVec3::ZERO, DVec3::new(12.66, 0.04, 1234.56)))),
        "12,7 × 0 × 1234,6 mm"
    );
    assert_eq!(size_text(None), "–");
}

// ---- (b) Häkchen und Zusammenfassung ----------------------------------------------------------

#[test]
fn checkboxes_and_summary() {
    assert_eq!(
        summary(3, &[("A".into(), "x".into()), ("B".into(), "y".into())]),
        "3 Körper importiert, 2 ausgelassen: A (x); B (y)"
    );
    assert_eq!(summary(1, &[]), "1 Körper importiert");
    assert_eq!(
        summary(0, &[("A".into(), "x".into())]),
        "0 Körper importiert, 1 ausgelassen: A (x)"
    );

    let dir = temp_dir("haken");
    let mut d = loaded(&dir, "g.step", &combined());
    let lid = reason(&d, 3);
    let mut s = AppState::new();
    let mut open = true;
    let mut screen = Screen::new(1200.0, 800.0);
    screen.settle(4, &mut show_on(&mut d, &mut s, &mut open));
    assert_eq!(d.last_rows.len(), 4, "ein Häkchen je Zeile");
    // Der ausgelassene Körper lässt sich nicht anhaken.
    let locked = d.last_rows[3].1;
    screen.click(locked.center(), &mut show_on(&mut d, &mut s, &mut open));
    assert_eq!(d.checked, [true, true, true, false]);
    // „Stift“ abhaken.
    let pin = d.last_rows[2].1;
    screen.click(pin.center(), &mut show_on(&mut d, &mut s, &mut open));
    assert_eq!(d.checked, [true, true, false, false]);
    // Eine alte Meldung verschwindet mit dem gelungenen Import.
    s.message = Some("alte Meldung".into());
    let seen = screen.settle(2, &mut show_on(&mut d, &mut s, &mut open));
    let button = find(&seen, "Importieren").expect("Importieren").rect;
    screen.click(button.center(), &mut show_on(&mut d, &mut s, &mut open));
    assert!(!open, "Fenster schließt nach dem Import");
    let names: Vec<String> = s
        .document
        .features()
        .iter()
        .map(|f| f.name.clone())
        .collect();
    assert_eq!(names, ["Block", "Block (2)"]);
    // Erfolg ist keine Fehlermeldung: die Zusammenfassung steht als Hinweis da.
    assert_eq!(
        s.info.as_deref(),
        Some(summary(2, &[("Deckellos".into(), lid.clone())]).as_str())
    );
    assert_eq!(
        s.info.as_deref().unwrap(),
        format!("2 Körper importiert, 1 ausgelassen: Deckellos ({lid})")
    );
    assert_eq!(s.message, None, "keine Meldung in Fehlerfarbe");

    // Nichts angehakt: „Importieren“ ist gesperrt und ändert nichts.
    let mut d = loaded(&dir, "g2.step", &combined());
    let mut s = AppState::new();
    d.checked = vec![false; 4];
    assert!(d.import(&mut s).is_err());
    assert_eq!(
        s.message.as_deref(),
        Some(NOTHING_CHECKED),
        "Ablehnung als Meldung"
    );
    assert_eq!(s.info, None);
    s.message = None;
    let mut open = true;
    let mut screen = Screen::new(1200.0, 800.0);
    let seen = screen.settle(4, &mut show_on(&mut d, &mut s, &mut open));
    let button = find(&seen, "Importieren").expect("Importieren").rect;
    screen.click(button.center(), &mut show_on(&mut d, &mut s, &mut open));
    assert!(open, "gesperrter Knopf");
    assert!(d.error.is_none(), "kein Importversuch: {:?}", d.error);
    assert!(s.document.features().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

// ---- (c) Ein Schritt ----------------------------------------------------------------------------

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Shown> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self) -> Vec<Shown> {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn click(&mut self, p: Pos2) {
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.frame(vec![]);
    }

    fn keys(&mut self, k: Key, modifiers: Modifiers) {
        self.frame(vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        self.settle();
    }

    fn bodies(&mut self) -> usize {
        self.ws().active_tab().state.bodies.len()
    }

    /// Text `t` im Fenster „STEP importieren“ des aktiven Tabs.
    fn in_dialog(&mut self, t: &str) -> Rect {
        let seen = self.settle();
        let rect = self
            .ws()
            .active_tab()
            .step_import_dialog
            .as_ref()
            .and_then(|d| d.last_rect)
            .expect("Fenster gezeichnet");
        seen.iter()
            .find(|s| s.text == t && rect.contains_rect(s.rect))
            .unwrap_or_else(|| panic!("„{t}“ fehlt im Fenster"))
            .rect
    }
}

fn workspace() -> Workspace {
    let root = common::temp_root("planora-step-ui");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

const SHIFT_COMMAND: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: true,
};

#[test]
fn one_undo_step() {
    let dir = temp_dir("schritt");
    let mut rig = Rig::new(workspace());
    let edits = rig.ws().active_tab().state.document.edit_count();
    let mut d = StepImportDialog::new(dir.clone());
    d.load_at(write(&dir, "g.step", &combined()), edits);
    d.tick(true);
    rig.ws().active_tab_mut().step_import_dialog = Some(d);
    let button = rig.in_dialog("Importieren");
    rig.click(button.center());
    assert!(
        rig.ws().active_tab().step_import_dialog.is_none(),
        "geschlossen"
    );
    assert_eq!(rig.bodies(), 3);
    let seen = rig.settle();
    assert_eq!(rig.ws().active_tab().undo_label(), Some("STEP importieren"));
    // Die Zusammenfassung überlebt den neuen Verlaufsschritt als Hinweis in neutraler Farbe.
    let st = &rig.ws().active_tab().state;
    let info = st.info.clone().expect("Hinweis nach dem Import");
    assert!(
        info.starts_with("3 Körper importiert, 1 ausgelassen: Deckellos ("),
        "{info}"
    );
    assert_eq!(st.message, None);
    let line = seen
        .iter()
        .find(|t| t.text.starts_with("3 Körper importiert"))
        .expect("Statuszeile");
    assert_eq!(
        line.color,
        theme::palette(true).text_secondary,
        "neutrale Farbe"
    );
    let steps = rig.ws().active_tab().history().undo_len();

    // Strg+Z bei offenem Fenster ändert nichts.
    rig.ws().active_tab_mut().step_import_dialog = Some(StepImportDialog::new(dir.clone()));
    rig.settle();
    assert!(
        rig.ws().active_tab().dialog_open(),
        "Fenster zählt als offen"
    );
    rig.keys(Key::Z, Modifiers::COMMAND);
    assert_eq!(rig.bodies(), 3, "Strg+Z wirkt nicht bei offenem Fenster");
    assert_eq!(rig.ws().active_tab().history().undo_len(), steps);

    // Rückgängig per Menü oder ein neuer Stand schließt die Fenster des Tabs, auch dieses.
    rig.ws().active_tab_mut().close_dialogs();
    assert!(
        rig.ws().active_tab().step_import_dialog.is_none(),
        "close_dialogs verwirft das Fenster"
    );
    rig.ws().active_tab_mut().step_import_dialog = Some(StepImportDialog::new(dir.clone()));
    rig.settle();

    // Nach dem Schließen nimmt es alle Körper auf einmal zurück. Ohne Liste gibt es kein
    // „Schließen“, „Abbrechen“ der Dateiauswahl schließt das Fenster.
    let close = rig.in_dialog("Abbrechen");
    rig.click(close.center());
    assert!(rig.ws().active_tab().step_import_dialog.is_none());
    rig.keys(Key::Z, Modifiers::COMMAND);
    assert_eq!(rig.bodies(), 0, "ein Schritt nimmt alle drei zurück");
    assert!(rig.ws().active_tab().state.document.features().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

// ---- (d) Abbrechen und Fehler -----------------------------------------------------------------

#[test]
fn cancel_and_errors() {
    let dir = temp_dir("fehler");
    let path = write(&dir, "g.step", &combined());

    // Abgebrochener Fortschritt beim Lesen: keine Liste.
    let p = Arc::new(Progress::new());
    p.cancel();
    let probe = Job::spawn_with(Arc::clone(&p), |p: &Progress| p.is_cancelled());
    assert_eq!(probe.wait(), planora_app::jobs::Poll::Ready(true));
    let mut d = StepImportDialog::new(dir.clone());
    d.load_with(path.clone(), None, Arc::clone(&p));
    d.tick(true);
    assert_eq!(d.error.as_deref(), Some("Abgebrochen"));
    assert!(d.loaded.is_none() && !d.is_loading());

    // „Abbrechen“ beendet den Auftrag selbst, nicht nur die Anzeige.
    let p = Arc::new(Progress::new());
    let mut d = StepImportDialog::new(dir.clone());
    d.load_with(path.clone(), None, Arc::clone(&p));
    assert!(d.is_loading());
    assert_eq!(d.phase(), Some(READING));
    d.cancel();
    assert!(p.is_cancelled(), "Auftrag abgebrochen");
    assert_eq!(d.error.as_deref(), Some("Abgebrochen"));
    assert!(!d.is_loading() && d.phase().is_none());
    d.tick(true);
    assert!(
        d.loaded.is_none(),
        "nach dem Abbrechen kommt kein Ergebnis an"
    );

    // Abbrechen der Neuberechnung: die alte Liste bleibt stimmig (Einheit Zoll).
    let mut d = loaded(&dir, "g3.step", &combined());
    d.set_unit(LengthUnit::Millimeter);
    assert_eq!(d.phase(), Some(CHECKING));
    d.cancel();
    d.tick(true);
    let l = d.loaded.as_ref().unwrap();
    assert_eq!(l.import.unit, LengthUnit::Zoll);
    assert_eq!(
        size_text(l.import.bodies[0].bounds),
        "25,4 × 50,8 × 76,2 mm"
    );
    assert_eq!(d.error.as_deref(), Some("Abgebrochen"));

    // Abgeschnittene Datei: Meldung im Fenster, das Fenster bleibt offen.
    let text = combined();
    let cut = write(&dir, "kurz.step", &text[..text.len() / 2]);
    let mut d = StepImportDialog::new(dir.clone());
    d.load(cut);
    d.tick(true);
    let e = d.error.clone().expect("Meldung");
    assert!(
        e.starts_with("Die Datei ist unvollständig: Sie endet in Zeile "),
        "{e}"
    );
    assert!(d.loaded.is_none());
    let mut s = AppState::new();
    let shown = draw(&mut d, &mut s);
    assert!(shown.contains(&e), "{shown}");

    // 51 Körper.
    let one = data_part(&step_data("box_ap214.step"));
    let many: Vec<String> = (0..51).map(|i| shift_ids(&one, i * 10_000)).collect();
    let mut d = StepImportDialog::new(dir.clone());
    d.load(write(&dir, "viele.step", &wrap(&many.join("\n"))));
    d.tick(true);
    assert_eq!(
        d.error.as_deref(),
        Some("Die Datei enthält mehr als 50 Körper.")
    );

    // Nur ein Facettenkörper: „keine Volumenkörper“ und warum.
    let facets = wrap("#1=FACETED_BREP('Netz',#2);\n#2=CLOSED_SHELL('',());");
    let mut d = StepImportDialog::new(dir.clone());
    d.load(write(&dir, "facetten.step", &facets));
    d.tick(true);
    assert_eq!(
        d.error.as_deref(),
        Some("Die Datei enthält keine Volumenkörper.")
    );
    assert_eq!(d.notes.len(), 1, "{:?}", d.notes);
    assert!(d.notes[0].contains("(FACETED_BREP)"), "{:?}", d.notes);
    let shown = draw(&mut d, &mut s);
    assert!(shown.contains(&d.notes[0]), "{shown}");
    // Eine neue Datei verwirft Meldung und Hinweise.
    d.load(path);
    d.tick(true);
    assert!(d.error.is_none() && d.notes.is_empty() && d.loaded.is_some());
    let _ = fs::remove_dir_all(&dir);
}

// ---- (e) Einheit umstellen ----------------------------------------------------------------------

#[test]
fn unit_override_rebuilds() {
    let dir = temp_dir("einheit");
    let mut d = loaded(&dir, "g.step", &combined());
    d.checked[1] = false;
    let mut s = AppState::new();
    let mut open = true;
    let mut screen = Screen::new(1200.0, 800.0);
    let seen = screen.settle(4, &mut show_on(&mut d, &mut s, &mut open));
    let mm = find(&seen, "Millimeter").expect("Radioknopf").rect;
    let mut f = show_on(&mut d, &mut s, &mut open);
    screen.frame(vec![Event::PointerMoved(mm.center())], &mut f);
    let press = |pressed| Event::PointerButton {
        pos: mm.center(),
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    screen.frame(vec![press(true)], &mut f);
    let seen = screen.frame(vec![press(false)], &mut f);
    drop(f);
    assert_eq!(d.phase(), Some(CHECKING));
    let seen = if find(&seen, CHECKING).is_some() {
        seen
    } else {
        screen.frame(vec![], &mut show_on(&mut d, &mut s, &mut open))
    };
    assert!(
        find(&seen, CHECKING).is_some(),
        "Phase sichtbar: {}",
        all(&seen)
    );
    d.tick(true);
    let text = draw(&mut d, &mut s);
    assert!(text.lines().any(|l| l == "1 × 2 × 3 mm"), "{text}");
    let l = d.loaded.as_ref().unwrap();
    assert_eq!(l.import.unit, LengthUnit::Millimeter);
    assert_eq!(
        d.checked,
        [true, false, true, false],
        "Häkchen bleiben je Zeile"
    );
    assert_eq!(
        selected_unit(&l.import, d.chosen_unit()),
        Some(Unit::Millimeter)
    );
    // Import mit der gewählten Einheit.
    d.import(&mut s).unwrap();
    let (lo, hi) = s.bodies[0].bounds;
    assert!(((hi - lo) - DVec3::new(1.0, 2.0, 3.0)).abs().max_element() < 1e-9);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unit_hints_tell_missing_from_unclear() {
    let import = |state: UnitState, unit: LengthUnit| StepImport {
        header: StepHeader::default(),
        unit,
        unit_known: state == UnitState::Named,
        unit_state: state,
        bodies: Vec::new(),
        messages: Vec::new(),
    };
    let info = |state: UnitState, own: LengthUnit, contexts: Vec<LengthUnit>| UnitInfo {
        state,
        own,
        contexts,
        unreadable: false,
        free_skipped: false,
    };
    let named = import(UnitState::Named, LengthUnit::Zoll);
    let zoll = info(UnitState::Named, LengthUnit::Zoll, vec![LengthUnit::Zoll]);
    assert_eq!(unit_hint(&zoll, None), None);
    assert_eq!(selected_unit(&named, None), Some(Unit::Zoll));
    // Eine Wahl auf einer Datei mit eigener Einheit wird genannt.
    assert_eq!(
        unit_hint(&zoll, Some(LengthUnit::Millimeter)).as_deref(),
        Some("Die Datei nennt Zoll; gewählt: Millimeter.")
    );
    // Körper in verschiedenen Einheiten.
    let mixed = info(
        UnitState::Named,
        LengthUnit::Zoll,
        vec![LengthUnit::Zoll, LengthUnit::Millimeter],
    );
    assert_eq!(unit_hint(&mixed, None).as_deref(), Some(MIXED_UNIT_HINT));
    assert_eq!(
        unit_hint(&mixed, Some(LengthUnit::Meter)).as_deref(),
        Some("Die Datei nennt verschiedene Längeneinheiten; gewählt: Meter für alle Körper.")
    );
    // Ein freier Körper ist wegen unklarer Einheit ausgelassen, die Datei nennt sonst eine.
    let free = UnitInfo {
        free_skipped: true,
        ..zoll.clone()
    };
    assert_eq!(unit_hint(&free, None).as_deref(), Some(FREE_SKIPPED_HINT));
    let missing = import(UnitState::Missing, LengthUnit::Millimeter);
    let none = info(UnitState::Missing, LengthUnit::Millimeter, vec![]);
    assert_eq!(unit_hint(&none, None).as_deref(), Some(NO_UNIT_HINT));
    assert_eq!(
        NO_UNIT_HINT,
        "Die Datei nennt keine Längeneinheit; angenommen: Millimeter."
    );
    assert_eq!(selected_unit(&missing, None), Some(Unit::Millimeter));
    assert_eq!(
        unit_hint(&none, Some(LengthUnit::Zoll)).as_deref(),
        Some("Die Datei nennt keine Längeneinheit; gewählt: Zoll.")
    );
    // Unklar nur bei echtem Widerspruch; unlesbare Einheiten haben einen eigenen Hinweis.
    let unclear = import(UnitState::Unclear, LengthUnit::Millimeter);
    let conflict = info(
        UnitState::Unclear,
        LengthUnit::Millimeter,
        vec![LengthUnit::Millimeter, LengthUnit::Zoll],
    );
    assert_eq!(
        unit_hint(&conflict, None).as_deref(),
        Some(UNCLEAR_UNIT_HINT)
    );
    assert_eq!(selected_unit(&unclear, None), None, "nichts vorgewählt");
    assert_eq!(
        unit_hint(&conflict, Some(LengthUnit::Meter)).as_deref(),
        Some("Die Datei nennt verschiedene Längeneinheiten; gewählt: Meter.")
    );
    let unreadable = UnitInfo {
        unreadable: true,
        ..info(UnitState::Unclear, LengthUnit::Millimeter, vec![])
    };
    assert_eq!(
        unit_hint(&unreadable, None).as_deref(),
        Some(UNREADABLE_UNIT_HINT)
    );
    assert_eq!(
        unit_hint(&unreadable, Some(LengthUnit::Zoll)).as_deref(),
        Some("Die Einheit der Datei lässt sich nicht lesen; gewählt: Zoll.")
    );
    let other = import(UnitState::Named, LengthUnit::Other { mm: 2.0 });
    assert_eq!(selected_unit(&other, None), None);
    for u in Unit::ALL {
        assert_eq!(
            selected_unit(&import(UnitState::Named, length_of(u)), None),
            Some(u)
        );
        assert_eq!(length_of(u).mm(), u.factor());
    }

    // Eine Datei ohne Längeneinheit: Hinweis im Fenster, Millimeter angenommen.
    let dir = temp_dir("ohne-einheit");
    let text = step_data("box_ap214.step").replace("((#100,#101,#102))", "((#101,#102))");
    assert!(!text.contains("((#100,#101,#102))"));
    let mut d = loaded(&dir, "ohne.step", &text);
    assert_eq!(
        d.loaded.as_ref().unwrap().import.unit_state,
        UnitState::Missing
    );
    let mut s = AppState::new();
    let shown = draw(&mut d, &mut s);
    assert!(shown.contains(NO_UNIT_HINT), "{shown}");
    let _ = fs::remove_dir_all(&dir);
}

// ---- (f) Veraltet, Rückgängig und Fenster ------------------------------------------------------

#[test]
fn stale_and_undo_consistency() {
    let dir = temp_dir("veraltet");
    let path = write(&dir, "g.step", &combined());
    let mut s = AppState::new();
    let mut d = StepImportDialog::new(dir.clone());
    d.load_at(path.clone(), s.document.edit_count());
    d.tick(true);
    assert!(d.loaded.is_some());
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.info = Some("alt".into());
    assert_eq!(d.import(&mut s), Err(STALE.to_string()));
    assert_eq!(s.message.as_deref(), Some(STALE), "Ablehnung als Meldung");
    assert_eq!(s.info.as_deref(), Some("alt"), "kein Erfolgshinweis");
    assert_eq!(s.document.features().len(), 1);
    let text = draw(&mut d, &mut s);
    assert!(d.loaded.is_none());
    assert_eq!(d.error.as_deref(), Some(STALE));
    assert!(text.contains(STALE), "{text}");
    assert!(!text.lines().any(|l| l == "Importieren"), "{text}");
    assert!(!text.lines().any(|l| l == "Block"), "keine Liste: {text}");

    // Import, Rückgängig, neues Fenster, Wiederholen.
    let mut rig = Rig::new(workspace());
    let edits = rig.ws().active_tab().state.document.edit_count();
    let mut d = StepImportDialog::new(dir.clone());
    d.load_at(path, edits);
    d.tick(true);
    rig.ws().active_tab_mut().step_import_dialog = Some(d);
    let button = rig.in_dialog("Importieren");
    rig.click(button.center());
    assert_eq!(rig.bodies(), 3);
    rig.keys(Key::Z, Modifiers::COMMAND);
    assert_eq!(rig.bodies(), 0);
    rig.ws().active_tab_mut().step_import_dialog = Some(StepImportDialog::new(dir.clone()));
    let seen = rig.settle();
    let fresh = rig.ws().active_tab().step_import_dialog.as_ref().unwrap();
    assert!(fresh.loaded.is_none() && fresh.checked.is_empty());
    assert!(fresh.error.is_none() && fresh.notes.is_empty());
    let rect = fresh.last_rect.unwrap();
    for t in seen.iter().filter(|t| rect.contains_rect(t.rect)) {
        assert!(
            !t.text.starts_with("wird ")
                && t.text != "Block"
                && t.text != STALE
                && t.text != "Importieren",
            "alter Inhalt im neuen Fenster: {}",
            t.text
        );
    }
    // Ohne Liste: „Abbrechen“ der Dateiauswahl schließt das Fenster.
    let close = rig.in_dialog("Abbrechen");
    rig.click(close.center());
    assert!(rig.ws().active_tab().step_import_dialog.is_none());
    rig.keys(Key::Z, SHIFT_COMMAND);
    assert_eq!(rig.bodies(), 3, "Wiederholen");
    let state = &mut rig.ws().active_tab_mut().state;
    let i = state
        .bodies
        .iter()
        .position(|b| b.name == "Stift")
        .expect("Stift");
    state.selected_body = Some(i);
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut panel = Vec::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| properties::show(ui, state));
        panel.clear();
        for c in &out.shapes {
            walk(&c.shape, &mut panel);
        }
        out.drop_without_applying_deltas();
    }
    let texts: Vec<&str> = panel.iter().map(|t| t.text.as_str()).collect();
    assert!(texts.contains(&"Stift"), "{texts:?}");
    assert!(texts.contains(&"STEP-Körper"), "{texts:?}");
    assert!(!texts.contains(&"Block"), "{texts:?}");
    let _ = fs::remove_dir_all(&dir);
}

// ---- (g) Echte Tasten und Zeilenkennungen ------------------------------------------------------

fn key_of(c: char) -> Key {
    match c {
        '.' => Key::Period,
        '-' => Key::Minus,
        c => Key::from_name(&c.to_ascii_uppercase().to_string()).expect("Taste"),
    }
}

fn key_event(k: Key, pressed: bool) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn real_keys_in_the_file_field() {
    let dir = temp_dir("tasten");
    write(&dir, "teil.step", &combined());
    write(&dir, "anderes.stp", &combined());
    let mut d = StepImportDialog::new(dir.clone());
    let mut s = AppState::new();
    let mut open = true;
    let mut screen = Screen::new(1200.0, 800.0);
    let seen = screen.settle(4, &mut show_on(&mut d, &mut s, &mut open));
    let shown = all(&seen);
    assert!(shown.contains("Datei wählen (STEP)"), "{shown}");
    assert!(
        shown.lines().any(|l| l == "teil.step") && shown.lines().any(|l| l == "anderes.stp"),
        "Endungen .step und .stp: {shown}"
    );
    let label = find(&seen, "Dateiname").expect("Feld").rect;
    let field = Pos2::new(label.right() + 80.0, label.center().y);
    screen.click(field, &mut show_on(&mut d, &mut s, &mut open));
    for c in "teil.step".chars() {
        let k = key_of(c);
        let mut f = show_on(&mut d, &mut s, &mut open);
        screen.frame(
            vec![
                key_event(k, true),
                Event::Text(c.to_string()),
                key_event(k, false),
            ],
            &mut f,
        );
        screen.frame(vec![], &mut f);
    }
    assert!(!d.is_loading(), "erst Enter liest");
    let mut f = show_on(&mut d, &mut s, &mut open);
    screen.frame(
        vec![key_event(Key::Enter, true), key_event(Key::Enter, false)],
        &mut f,
    );
    screen.frame(vec![], &mut f);
    drop(f);
    assert!(
        d.is_loading() || d.loaded.is_some(),
        "Enter liest die Datei"
    );
    d.tick(true);
    let l = d.loaded.as_ref().expect("gelesen");
    assert_eq!(l.path.file_name().unwrap(), "teil.step");

    // Die Kennung eines Häkchens hängt an der Zeile, nicht am Namen.
    screen.settle(3, &mut show_on(&mut d, &mut s, &mut open));
    let before: Vec<egui::Id> = d.last_rows.iter().map(|r| r.0).collect();
    assert_eq!(before.len(), 4);
    d.loaded.as_mut().unwrap().import.bodies[0].name = "Anders".into();
    let seen = screen.settle(3, &mut show_on(&mut d, &mut s, &mut open));
    assert!(find(&seen, "Anders").is_some());
    let after: Vec<egui::Id> = d.last_rows.iter().map(|r| r.0).collect();
    assert_eq!(before, after);
    let _ = fs::remove_dir_all(&dir);
}

// ---- (h) Fenster ----------------------------------------------------------------------------------

/// Zeichen außerhalb von ASCII, die in Oberflächentexten erlaubt sind (wie `tests/fonts.rs`).
const ALLOWED: &str = "ÄÖÜäöüß„“–−·×°±²³…ØΔ≈";

#[test]
fn layout() {
    let dir = temp_dir("fenster");
    let path = write(&dir, "g.step", &combined_with_facets());
    for (w, h) in [(800.0, 600.0), (1600.0, 1000.0)] {
        for dark in [false, true] {
            let mut d = StepImportDialog::new(dir.clone());
            d.load(path.clone());
            d.tick(true);
            let mut s = AppState::new();
            let mut open = true;
            let mut screen = Screen::new(w, h);
            theme::apply(&screen.ctx, dark);
            let seen = screen.settle(8, &mut show_on(&mut d, &mut s, &mut open));
            let rect = d.last_rect.expect("gezeichnet");
            let window = Rect::from_min_size(Pos2::ZERO, screen.size);
            assert!(window.contains_rect(rect), "{rect:?} in {window:?}");
            // Dateifenster: 55 % der Fensterbreite, mindestens 720 (Teilprojekt Dateiauswahl).
            let wanted =
                planora_app::dialog::limits(window, planora_app::dialog::DialogSize::File).0;
            assert!(
                (rect.width() - wanted).abs() <= 1.0,
                "volle Breite {wanted}: {rect:?}"
            );
            assert!((rect.center().x - w / 2.0).abs() <= 1.0, "mittig: {rect:?}");
            assert_eq!(d.last_cards.len(), 2, "Karten „Datei“ und „Körper“");
            assert_stacked(rect, &d.last_cards, &seen);
            // Die Größen brechen nicht um (breite Spalten).
            let size = find(&seen, "25,4 × 50,8 × 76,2 mm").expect("Größe");
            assert!(size.rect.height() < 20.0, "einzeilig: {:?}", size.rect);
            let footer = find(&seen, "Importieren").expect("Importieren");
            assert!(
                footer.clip.contains_rect(footer.rect) && rect.contains_rect(footer.rect),
                "Knopf sichtbar"
            );
            for t in &seen {
                for c in t.text.chars() {
                    assert!(
                        c.is_ascii()
                            || ALLOWED.contains(c)
                            || ('\u{e000}'..='\u{f8ff}').contains(&c),
                        "Zeichen „{c}“ in „{}“ ohne Schrift",
                        t.text
                    );
                }
            }
            // Auch die Zeilen der Tabelle bleiben in ihrer Karte.
            let bodies = d.last_cards[1];
            for name in ["Block", "Stift", "Deckellos"] {
                let t = find(&seen, name).expect(name);
                assert!(bodies.contains_rect(t.rect), "{name} in {bodies:?}");
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

/// Karten untereinander über die ganze Breite: gleiche linke und rechte Kante, nahezu so breit
/// wie das Fenster, die zweite unter der ersten; kein Text ragt aus seiner Karte.
fn assert_stacked(rect: Rect, cards: &[Rect], seen: &[common::Seen]) {
    for c in cards {
        assert!((c.left() - cards[0].left()).abs() <= 0.5, "{cards:?}");
        assert!((c.width() - cards[0].width()).abs() <= 0.5, "{cards:?}");
        assert!(
            c.width() >= rect.width() - 60.0,
            "ganze Breite: {c:?} in {rect:?}"
        );
        assert!(
            c.left() >= rect.left() - 0.5 && c.right() <= rect.right() + 0.5,
            "{c:?} in {rect:?}"
        );
    }
    for pair in cards.windows(2) {
        assert!(pair[1].top() >= pair[0].bottom(), "untereinander {cards:?}");
    }
    for t in seen {
        if let Some(card) = cards.iter().find(|c| c.contains(t.rect.center())) {
            assert!(
                card.expand(0.5).contains_rect(t.rect),
                "Text „{}“ {:?} ragt aus seiner Karte {card:?}",
                t.text,
                t.rect
            );
        }
    }
}

#[test]
fn fifty_bodies_keep_the_buttons_reachable() {
    let dir = temp_dir("fuenfzig");
    let one = data_part(&step_data("box_ap214.step"));
    let many: Vec<String> = (0..50).map(|i| shift_ids(&one, i * 10_000)).collect();
    let mut d = loaded(
        &dir,
        "viele.step",
        &wrap(&many.join(
            "
",
        )),
    );
    assert_eq!(d.loaded.as_ref().unwrap().import.bodies.len(), 50);
    let mut s = AppState::new();
    let mut open = true;
    let mut screen = Screen::new(800.0, 600.0);
    let seen = screen.settle(8, &mut show_on(&mut d, &mut s, &mut open));
    let rect = d.last_rect.unwrap();
    assert!(Rect::from_min_size(Pos2::ZERO, screen.size).contains_rect(rect));
    let button = find(&seen, "Importieren").expect("Importieren in der Fußzeile");
    assert!(
        button.clip.contains_rect(button.rect) && rect.contains_rect(button.rect),
        "{:?} in {rect:?}",
        button.rect
    );
    screen.click(
        button.rect.center(),
        &mut show_on(&mut d, &mut s, &mut open),
    );
    assert!(!open);
    assert_eq!(s.document.features().len(), 50);
    let _ = fs::remove_dir_all(&dir);
}

// ---- (i) Menü ------------------------------------------------------------------------------------

fn menu_frame<'a>(ws: &'a mut Workspace) -> impl FnMut(&egui::Context) + 'a {
    move |ctx| {
        egui::Area::new(egui::Id::new("menue"))
            .fixed_pos(Pos2::new(20.0, 20.0))
            .show(ctx, |ui| {
                ui.set_width(280.0);
                theme::menu_style(ui.style_mut());
                app::file_menu(ui, ws);
            });
    }
}

#[test]
fn menu_entry() {
    let mut ws = workspace();
    let mut screen = Screen::new(800.0, 600.0);
    let seen = screen.settle(3, &mut menu_frame(&mut ws));
    let mesh = find(&seen, "Netz importieren …").expect("Netz").rect;
    let step = find(&seen, "STEP importieren …")
        .expect("STEP importieren …")
        .rect;
    assert!(step.top() > mesh.top() && (step.left() - mesh.left()).abs() < 0.5);
    // Direkt darunter: kein anderer Eintrag dazwischen.
    for t in &seen {
        let y = t.rect.center().y;
        assert!(
            !(y > mesh.bottom() && y < step.top()),
            "„{}“ zwischen den Einträgen",
            t.text
        );
    }
    let glyph = planora_app::icons::glyph(planora_app::icons::Icon::Import).to_string();
    assert!(
        seen.iter()
            .any(|t| t.text == glyph && (t.rect.center().y - step.center().y).abs() < 4.0),
        "Symbol Import"
    );

    // Eine offene Skizze wird beim Öffnen beendet (nicht erst beim Import).
    let sketch = {
        let s = &mut ws.active_tab_mut().state;
        let sketch = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sketch).unwrap();
        s.session_sketch_mut()
            .unwrap()
            .1
            .add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0))
            .unwrap();
        sketch
    };
    ws.observe_history(false);
    screen.click(step.center(), &mut menu_frame(&mut ws));
    assert!(
        ws.active_tab().step_import_dialog.is_some(),
        "Fenster offen"
    );
    assert!(
        ws.active_tab().state.sketch_session.is_none(),
        "Skizze beendet"
    );
    ws.observe_history(false);
    // Import im Fenster, dann Rückgängig: nur der Import geht zurück, die Skizze bleibt.
    let dir = temp_dir("menue");
    let tab = ws.active_tab_mut();
    let edits = tab.state.document.edit_count();
    let d = tab.step_import_dialog.as_mut().unwrap();
    d.load_at(write(&dir, "g.step", &combined()), edits);
    d.tick(true);
    d.import(&mut tab.state).unwrap();
    tab.step_import_dialog = None;
    ws.observe_history(false);
    assert_eq!(ws.active_tab().undo_label(), Some("STEP importieren"));
    ws.active_tab_mut().undo().unwrap();
    let s = &ws.active_tab().state;
    assert!(s.bodies.is_empty());
    assert_eq!(
        s.document.sketch_of(sketch).map(|sk| sk.curves().count()),
        Some(4),
        "Skizze bleibt nach Rückgängig des Imports"
    );

    // Das Fenster trägt den Titel „STEP importieren“.
    let mut d = StepImportDialog::new(dir.clone());
    let mut st = AppState::new();
    let text = draw(&mut d, &mut st);
    assert!(text.lines().any(|l| l == "STEP importieren"), "{text}");
    assert_eq!(TITLE, "STEP importieren");

    // In einer alten Version gesperrt, mit Hinweis.
    let mut ws = workspace();
    ws.active_tab_mut().read_only = true;
    let mut screen = Screen::new(800.0, 600.0);
    let seen = screen.settle(3, &mut menu_frame(&mut ws));
    let step = find(&seen, "STEP importieren …").expect("Eintrag").rect;
    screen.frame(
        vec![Event::PointerMoved(step.center())],
        &mut menu_frame(&mut ws),
    );
    let seen = screen.settle(30, &mut menu_frame(&mut ws));
    let hints = seen
        .iter()
        .filter(|t| t.text == "In einer alten Version lässt sich nichts hinzufügen.")
        .count();
    assert!(hints >= 1, "Hinweis: {}", all(&seen));
    screen.click(step.center(), &mut menu_frame(&mut ws));
    assert!(
        ws.active_tab().step_import_dialog.is_none(),
        "schreibgeschützt öffnet nichts"
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---- Fix-Runde 1 ---------------------------------------------------------------------------------

/// Tippt `name` mit echten Tasten in das Feld „Dateiname“ und drückt Enter.
fn choose_by_keys(
    screen: &mut Screen,
    d: &mut StepImportDialog,
    s: &mut AppState,
    open: &mut bool,
    name: &str,
) {
    let seen = screen.settle(3, &mut show_on(d, s, open));
    let label = find(&seen, "Dateiname").expect("Feld").rect;
    screen.click(
        Pos2::new(label.right() + 80.0, label.center().y),
        &mut show_on(d, s, open),
    );
    for c in name.chars() {
        let k = key_of(c);
        let mut f = show_on(d, s, open);
        screen.frame(
            vec![
                key_event(k, true),
                Event::Text(c.to_string()),
                key_event(k, false),
            ],
            &mut f,
        );
        screen.frame(vec![], &mut f);
    }
    let mut f = show_on(d, s, open);
    screen.frame(
        vec![key_event(Key::Enter, true), key_event(Key::Enter, false)],
        &mut f,
    );
    screen.frame(vec![], &mut f);
}

/// Klickt den Text `t` im Fenster, wartet die Aufträge ab und liefert das Bild danach.
fn click_text(
    screen: &mut Screen,
    d: &mut StepImportDialog,
    s: &mut AppState,
    open: &mut bool,
    t: &str,
) -> Vec<common::Seen> {
    let seen = screen.settle(3, &mut show_on(d, s, open));
    let r = find(&seen, t)
        .unwrap_or_else(|| panic!("„{t}“ fehlt:\n{}", all(&seen)))
        .rect;
    screen.click(r.center(), &mut show_on(d, s, open));
    d.tick(true);
    screen.settle(4, &mut show_on(d, s, open))
}

/// Zeile `name`: (Größe, Einheit) wie gezeigt.
fn row_of(d: &StepImportDialog, name: &str) -> (String, String) {
    let l = d.loaded.as_ref().unwrap();
    let i = l
        .import
        .bodies
        .iter()
        .position(|b| b.name == name)
        .unwrap_or_else(|| panic!("{name}"));
    (l.sizes[i].clone(), l.row_units[i].clone())
}

/// Baugruppe in Zoll und ein Würfel 10 × 20 × 30 mm mit eigenem Kontext in einer Datei.
fn mixed_units() -> String {
    wrap(&format!(
        "{}\n{}",
        data_part(&step_data("inch_assembly.step")),
        shift_ids(&data_part(&step_data("box_ap214.step")), 100_000)
    ))
}

#[test]
fn choosing_the_file_unit_again_never_rescales_other_bodies() {
    let dir = temp_dir("gemischt");
    let mut d = loaded(&dir, "gemischt.step", &mixed_units());
    let mut s = AppState::new();
    let mut open = true;
    let mut screen = Screen::new(1200.0, 800.0);
    // Jede Zeile in ihrer eigenen Einheit, Hinweis auf die verschiedenen Einheiten.
    assert_eq!(
        row_of(&d, "Block"),
        ("25,4 × 50,8 × 76,2 mm".into(), "Zoll".into())
    );
    assert_eq!(
        row_of(&d, "Würfel"),
        ("10 × 20 × 30 mm".into(), "mm".into())
    );
    let seen = screen.settle(4, &mut show_on(&mut d, &mut s, &mut open));
    assert!(find(&seen, MIXED_UNIT_HINT).is_some(), "{}", all(&seen));
    assert!(find(&seen, "mm").is_some() && find(&seen, "Zoll").is_some());

    // Millimeter für alle: der Hinweis sagt es.
    let seen = click_text(&mut screen, &mut d, &mut s, &mut open, "Millimeter");
    assert_eq!(d.chosen_unit(), Some(LengthUnit::Millimeter));
    assert_eq!(row_of(&d, "Block"), ("1 × 2 × 3 mm".into(), "mm".into()));
    let chosen_hint =
        "Die Datei nennt verschiedene Längeneinheiten; gewählt: Millimeter für alle Körper.";
    assert!(find(&seen, chosen_hint).is_some(), "{}", all(&seen));

    // Zurück auf „Zoll“, die Einheit der Datei: ohne Wahl gerechnet, der Würfel bleibt in mm.
    let seen = click_text(&mut screen, &mut d, &mut s, &mut open, "Zoll");
    assert_eq!(d.chosen_unit(), None, "keine Wahl mehr");
    assert_eq!(
        row_of(&d, "Würfel"),
        ("10 × 20 × 30 mm".into(), "mm".into())
    );
    assert_eq!(
        row_of(&d, "Block"),
        ("25,4 × 50,8 × 76,2 mm".into(), "Zoll".into())
    );
    assert!(find(&seen, MIXED_UNIT_HINT).is_some(), "{}", all(&seen));
    d.import(&mut s).unwrap();
    let cube = s.bodies.iter().find(|b| b.name == "Würfel").unwrap();
    let size = cube.bounds.1 - cube.bounds.0;
    assert!(
        (size - DVec3::new(10.0, 20.0, 30.0)).abs().max_element() < 1e-9,
        "{size}"
    );

    // Datei mit einer Einheit: die Wahl wird genannt, die eigene Einheit hebt sie auf.
    let mut d = loaded(&dir, "zoll.step", &step_data("inch_assembly.step"));
    let mut s = AppState::new();
    let seen = click_text(&mut screen, &mut d, &mut s, &mut open, "Millimeter");
    assert!(
        find(&seen, "Die Datei nennt Zoll; gewählt: Millimeter.").is_some(),
        "{}",
        all(&seen)
    );
    let seen = click_text(&mut screen, &mut d, &mut s, &mut open, "Zoll");
    assert_eq!(d.chosen_unit(), None);
    assert!(!all(&seen).contains("gewählt"), "{}", all(&seen));
    let _ = fs::remove_dir_all(&dir);
}

/// Würfel als freier Volumen-Eintrag (in keiner Darstellung) in einer Datei, deren Kontexte
/// Millimeter und Zoll nennen.
fn free_cube_conflicting() -> String {
    let cube = data_part(&step_data("box_ap214.step")).replace("(#11,#20)", "(#11)");
    assert!(!cube.contains("(#11,#20)"));
    let inch = data_part(&step_data("inch_assembly.step"));
    let start = inch.find("#100=").expect("Einheiten");
    let end = inch.find("#120=").expect("Richtungen");
    // Winkel wie im Würfel (Radiant), damit sich nur die Längeneinheit widerspricht.
    let units =
        shift_ids(&inch[start..end], 500_000).replace("#500107,#500108))", "#500104,#500108))");
    assert!(units.contains("#500104,#500108))"));
    wrap(&format!("{cube}\n{units}"))
}

#[test]
fn a_newly_usable_body_is_ticked_and_the_choice_is_shown() {
    let dir = temp_dir("frei");
    let mut d = loaded(&dir, "frei.step", &free_cube_conflicting());
    let l = d.loaded.as_ref().unwrap();
    assert_eq!(l.import.unit_state, UnitState::Unclear);
    assert!(l.units.mixed());
    assert_eq!(d.checked, [false], "unklar: ausgelassen");
    let mut s = AppState::new();
    let mut open = true;
    let mut screen = Screen::new(1200.0, 800.0);
    let seen = screen.settle(4, &mut show_on(&mut d, &mut s, &mut open));
    assert!(find(&seen, UNCLEAR_UNIT_HINT).is_some(), "{}", all(&seen));
    let seen = click_text(&mut screen, &mut d, &mut s, &mut open, "Millimeter");
    assert_eq!(d.checked, [true], "neu übernehmbar: angehakt");
    assert_eq!(d.chosen_unit(), Some(LengthUnit::Millimeter));
    assert!(
        find(
            &seen,
            "Die Datei nennt verschiedene Längeneinheiten; gewählt: Millimeter."
        )
        .is_some(),
        "{}",
        all(&seen)
    );
    assert_eq!(
        row_of(&d, "Würfel"),
        ("10 × 20 × 30 mm".into(), "mm".into())
    );
    // Eine neue Datei beginnt ohne Wahl.
    d.load(write(&dir, "g.step", &combined()));
    d.tick(true);
    assert_eq!(d.chosen_unit(), None);

    // In einer Datei mit eigener Einheit: Hinweis auf den ausgelassenen freien Körper.
    let text = wrap(&format!(
        "{}\n{}",
        data_part(&step_data("inch_assembly.step")),
        shift_ids(
            &data_part(&step_data("box_ap214.step")).replace("(#11,#20)", "(#11)"),
            100_000
        )
    ));
    let mut d = loaded(&dir, "frei2.step", &text);
    let l = d.loaded.as_ref().unwrap();
    assert_eq!(l.import.unit_state, UnitState::Named);
    assert!(l.units.free_skipped);
    let text = draw(&mut d, &mut s);
    assert!(text.contains(FREE_SKIPPED_HINT), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_file_chosen_in_the_window_is_bound_to_the_document() {
    let dir = temp_dir("gebunden");
    write(&dir, "teil.step", &combined());
    let mut d = StepImportDialog::new(dir.clone());
    let mut s = AppState::new();
    let mut open = true;
    let mut screen = Screen::new(1200.0, 800.0);
    choose_by_keys(&mut screen, &mut d, &mut s, &mut open, "teil.step");
    d.tick(true);
    assert!(
        d.loaded.as_ref().expect("gelesen").edits.is_some(),
        "an den Stand gebunden"
    );
    // Das Dokument ändert sich nach der Vorschau: nichts wird übernommen.
    s.create_box(DVec3::splat(10.0)).unwrap();
    assert_eq!(d.import(&mut s), Err(STALE.to_string()));
    let seen = screen.settle(3, &mut show_on(&mut d, &mut s, &mut open));
    assert!(find(&seen, STALE).is_some(), "{}", all(&seen));
    assert!(find(&seen, "Importieren").is_none());
    assert_eq!(s.document.features().len(), 1, "kein Teil-Import");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_another_file_choice_returns_to_the_list() {
    let dir = temp_dir("zurueck");
    let mut d = loaded(&dir, "g.step", &combined());
    let mut s = AppState::new();
    let mut open = true;
    let mut screen = Screen::new(1200.0, 800.0);
    let seen = click_text(
        &mut screen,
        &mut d,
        &mut s,
        &mut open,
        "Andere Datei wählen …",
    );
    assert!(
        find(&seen, "Datei wählen (STEP)").is_some(),
        "Dateiwahl offen"
    );
    // Während der Dateiwahl nur „Öffnen“ und „Abbrechen“ in der Fußzeile.
    assert!(find(&seen, "Öffnen").is_some());
    assert!(
        find(&seen, "Schließen").is_none(),
        "kein „Schließen“ bei der Dateiwahl"
    );
    assert!(find(&seen, "Importieren").is_none());
    // „Abbrechen“ der Dateiwahl: zurück zur Liste, das Fenster bleibt offen.
    let seen = click_text(&mut screen, &mut d, &mut s, &mut open, "Abbrechen");
    assert!(open, "Fenster bleibt offen");
    assert!(d.loaded.is_some(), "Liste bleibt");
    assert!(find(&seen, "Datei wählen (STEP)").is_none(), "Dateiwahl zu");
    assert!(find(&seen, "Block").is_some());
    assert!(
        find(&seen, "Schließen").is_some(),
        "in der Liste wieder „Schließen“"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn long_summaries_stay_short_enough_to_read() {
    let skipped: Vec<(String, String)> = ["A", "B", "C", "D", "E"]
        .iter()
        .map(|n| (n.to_string(), format!("Grund {n}")))
        .collect();
    assert_eq!(
        summary(1, &skipped),
        "1 Körper importiert, 5 ausgelassen: A (Grund A); B (Grund B); C (Grund C); und 2 weitere"
    );
    assert_eq!(
        summary(0, &skipped[..3]),
        "0 Körper importiert, 3 ausgelassen: A (Grund A); B (Grund B); C (Grund C)"
    );
}

#[test]
fn import_windows_exclude_each_other_and_sketch_mode_drops_them() {
    let mut ws = workspace();
    let mut screen = Screen::new(800.0, 600.0);
    let seen = screen.settle(3, &mut menu_frame(&mut ws));
    let mesh = find(&seen, "Netz importieren …").unwrap().rect;
    let step = find(&seen, "STEP importieren …").unwrap().rect;
    screen.click(mesh.center(), &mut menu_frame(&mut ws));
    assert!(ws.active_tab().import_dialog.is_some());
    screen.click(step.center(), &mut menu_frame(&mut ws));
    assert!(ws.active_tab().step_import_dialog.is_some());
    assert!(ws.active_tab().import_dialog.is_none(), "Netz-Fenster zu");
    screen.click(mesh.center(), &mut menu_frame(&mut ws));
    assert!(ws.active_tab().import_dialog.is_some());
    assert!(
        ws.active_tab().step_import_dialog.is_none(),
        "STEP-Fenster zu"
    );

    // Eine Skizze verwirft das Fenster (wie das Netz-Fenster).
    let mut rig = Rig::new(workspace());
    let dir = temp_dir("skizze");
    rig.ws().active_tab_mut().step_import_dialog = Some(StepImportDialog::new(dir.clone()));
    rig.settle();
    {
        let st = &mut rig.ws().active_tab_mut().state;
        let sketch = st
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        st.open_sketch(sketch).unwrap();
    }
    rig.settle();
    assert!(rig.ws().active_tab().step_import_dialog.is_none());
    let _ = fs::remove_dir_all(&dir);
}
