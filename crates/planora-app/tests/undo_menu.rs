//! Menü „Bearbeiten“ (Aufgabe 9): oben „Rückgängig: <Aktion>“ und „Wiederholen: <Aktion>“ mit den
//! Kürzeln Strg+Z und Strg+Y, ohne Schritt gesperrt mit Hilfetext. Alles mit echten Bildern der
//! ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`) und Mausklicks auf die Texte.

mod common;

use eframe::App as _;
use egui::{Event, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{PlanoraApp, redo_menu_text, undo_menu_text};
use planora_app::create_dialog::CreateDialog;
use planora_app::export_dialog::{ExportDialog, ExportFormat};
use planora_app::icons::{self, Icon};
use planora_app::sketch_state::Tool;
use planora_app::workspace::*;
use planora_app::{fonts, theme};
use planora_drawing::sheet::SheetModel;
use planora_drawing::{
    DrawError, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewKind,
};
use planora_exchange::Progress;
use planora_feature::{SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use std::path::Path;
use std::time::Duration;

const SIZE: Vec2 = vec2(1400.0, 900.0);
const SMALL: Vec2 = vec2(800.0, 600.0);

fn ws() -> Workspace {
    let root = common::temp_root("planora-undo-menu");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    color: egui::Color32,
    elided: bool,
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    size: Vec2,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        Rig::with(w, SIZE, true)
    }

    fn with(w: Workspace, size: Vec2, dark: bool) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, dark);
        PlanoraApp::setup_context(&ctx);
        // Hinweistexte sofort zeigen.
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            size,
        };
        rig.settle();
        rig
    }

    fn tab(&mut self) -> &mut DocTab {
        self.app.ws_mut().active_tab_mut()
    }

    fn bodies(&mut self) -> usize {
        self.tab().state.bodies.len()
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
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
                    out.push(Seen {
                        text: t.galley.text().to_string(),
                        rect: Rect::from_min_size(t.pos, t.galley.size()),
                        color,
                        elided: t.galley.elided,
                    })
                }
                _ => {}
            }
        }
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self) -> Vec<Seen> {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn click(&mut self, p: Pos2) {
        let b = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.frame(vec![]);
    }

    /// Öffnet das Menü „Bearbeiten“; liefert das Bild mit offenem Menü.
    fn open_menu(&mut self) -> Vec<Seen> {
        let seen = self.settle();
        let bar = seen
            .iter()
            .filter(|s| s.text == "Bearbeiten")
            .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .expect("Menü „Bearbeiten“ fehlt")
            .rect
            .center();
        self.click(bar);
        self.settle()
    }

    /// Klick auf den Eintrag `entry` im Menü.
    fn pick(&mut self, entry: &str) {
        let seen = self.open_menu();
        let p = entry_of(&seen, entry).rect.center();
        self.click(p);
    }

    /// Die Texte der beiden obersten Einträge des Menüs.
    fn menu_texts(&mut self) -> (String, String) {
        let seen = self.open_menu();
        let (u, r) = first_two(&seen);
        let out = (u.text.clone(), r.text.clone());
        self.close_menu();
        out
    }

    fn close_menu(&mut self) {
        self.frame(vec![Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.settle();
    }
}

fn entry_of<'a>(seen: &'a [Seen], text: &str) -> &'a Seen {
    seen.iter()
        .find(|s| s.text == text && s.rect.top() > 20.0 && s.rect.top() < 400.0)
        .unwrap_or_else(|| {
            panic!(
                "Menüeintrag „{text}“ fehlt: {:?}",
                seen.iter()
                    .filter(|s| s.rect.top() < 400.0)
                    .map(|s| &s.text)
                    .collect::<Vec<_>>()
            )
        })
}

/// Die beiden obersten Menüeinträge (Texte mit „Rückgängig“ bzw. „Wiederholen“ am Anfang).
fn first_two(seen: &[Seen]) -> (&Seen, &Seen) {
    let find = |word: &str| {
        seen.iter()
            .find(|s| s.text.starts_with(word) && s.rect.top() > 20.0 && s.rect.top() < 400.0)
            .unwrap_or_else(|| panic!("Eintrag „{word}“ fehlt"))
    };
    (find("Rückgängig"), find("Wiederholen"))
}

/// Das Kürzel `kbd` steht in der Zeile des Eintrags `row`, rechts davon.
fn has_shortcut(seen: &[Seen], row: &Seen, kbd: &str) -> bool {
    seen.iter().any(|s| {
        s.text == kbd
            && (s.rect.center().y - row.rect.center().y).abs() < 4.0
            && s.rect.left() > row.rect.right()
    })
}

/// Quader anlegen als ersten Schritt.
fn with_box() -> Workspace {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(30.0, 20.0, 10.0))
        .unwrap();
    w.observe_history(false);
    w
}

fn sheet() -> Sheet {
    Sheet {
        paper: Paper::A4,
        landscape: true,
    }
}

// ---- (a) ------------------------------------------------------------------------------------

#[test]
fn menu_shows_action_names() {
    let mut rig = Rig::new(with_box());
    let seen = rig.open_menu();
    let (u, r) = first_two(&seen);
    assert_eq!(u.text, "Rückgängig: Quader anlegen");
    assert_eq!(r.text, "Wiederholen", "ohne Schritt nur das Wort");
    assert!(has_shortcut(&seen, u, "Strg+Z"), "Kürzel Strg+Z");
    assert!(has_shortcut(&seen, r, "Strg+Y"), "Kürzel Strg+Y");
    assert_ne!(u.color, r.color, "Wiederholen ist gesperrt (andere Farbe)");
    rig.close_menu();

    rig.pick("Rückgängig: Quader anlegen");
    assert_eq!(rig.bodies(), 0, "Quader weg");
    let after = rig.settle();
    assert!(
        !after.iter().any(|s| s.text.starts_with("Wiederholen")),
        "das Menü schließt sich nach dem Klick"
    );
    assert_eq!(
        rig.menu_texts(),
        (
            "Rückgängig".to_string(),
            "Wiederholen: Quader anlegen".into()
        )
    );
    rig.pick("Wiederholen: Quader anlegen");
    assert_eq!(rig.bodies(), 1, "Quader wieder da");
    assert_eq!(
        rig.menu_texts(),
        (
            "Rückgängig: Quader anlegen".to_string(),
            "Wiederholen".into()
        )
    );
    assert_eq!(
        rig.tab().state.info.as_deref(),
        Some("Wiederholen: Quader anlegen")
    );
}

#[test]
fn menu_texts_are_built_from_the_action_name() {
    assert_eq!(
        undo_menu_text(Some("Quader anlegen")),
        "Rückgängig: Quader anlegen"
    );
    assert_eq!(undo_menu_text(None), "Rückgängig");
    assert_eq!(redo_menu_text(Some("Löschen")), "Wiederholen: Löschen");
    assert_eq!(redo_menu_text(None), "Wiederholen");
}

// ---- (b) ------------------------------------------------------------------------------------

fn slow_writer(_: &Path, _: &SheetModel, _: DrawingFormat, p: &Progress) -> Result<u64, DrawError> {
    loop {
        p.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Texte im Bild, nachdem der Zeiger über dem Eintrag `entry` des offenen Menüs steht.
fn hover_texts(rig: &mut Rig, entry: &str) -> Vec<String> {
    let seen = rig.open_menu();
    let p = entry_of(&seen, entry).rect.center();
    let mut seen = vec![];
    for _ in 0..5 {
        // Die Hinweise erscheinen erst nach einer Verzögerung: Zeit springen lassen.
        rig.time += 0.3;
        seen = rig.frame(vec![Event::PointerMoved(p)]);
    }
    seen.into_iter().map(|s| s.text).collect()
}

fn write_export(rig: &mut Rig, d: DrawingId) {
    let t = rig.tab();
    let s = &t.state;
    let mut e = ExportDialog::new(s);
    e.format = ExportFormat::Drawing(DrawingFormat::Svg);
    e.drawing = Some(d);
    e.tick(s, true);
    e.set_writer(slow_writer);
    let dir = common::temp_root("planora-undo-menu-export");
    std::fs::create_dir_all(&dir).unwrap();
    e.write_to(dir.join("z.svg"), s);
    assert!(e.is_writing_file());
    t.export_dialog = Some(e);
}

#[test]
fn disabled_entries_explain() {
    let mut rig = Rig::new(ws());
    let texts = hover_texts(&mut rig, "Rückgängig");
    assert!(
        texts
            .iter()
            .any(|t| t == "Es gibt nichts rückgängig zu machen."),
        "{texts:?}"
    );
    rig.close_menu();
    let texts = hover_texts(&mut rig, "Wiederholen");
    assert!(
        texts.iter().any(|t| t == "Es gibt nichts zu wiederholen."),
        "{texts:?}"
    );
    rig.close_menu();
    // Ein Klick auf den gesperrten Eintrag ändert nichts.
    rig.pick("Rückgängig");
    assert_eq!(rig.bodies(), 0);
    assert_eq!(rig.tab().state.info, None);
    rig.close_menu();

    // Export schreibt: gesperrt mit dem Sperrtext, trotz Schritten.
    let mut w = with_box();
    let d = w
        .active_tab_mut()
        .state
        .document
        .add_drawing("Blatt 1", sheet(), Projection::First)
        .unwrap();
    w.active_tab_mut()
        .state
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 148.5, y: 105.0 },
            None,
        )
        .unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let entry = "Rückgängig: Zeichnung anlegen";
    assert_eq!(rig.menu_texts().0, entry);
    write_export(&mut rig, d);
    rig.settle();
    assert_eq!(rig.tab().undo_blocked(), Some(export_writing()));
    let texts = hover_texts(&mut rig, entry);
    assert!(
        texts
            .iter()
            .any(|t| t == "Während des Exports ist das nicht möglich."),
        "{texts:?}"
    );
    rig.close_menu();
    rig.pick(entry);
    assert_eq!(rig.tab().state.document.drawings().len(), 1, "gesperrt");
    assert!(rig.tab().export_dialog.is_some());
    rig.tab().export_dialog.as_mut().unwrap().cancel_write();
}

// ---- (c) ------------------------------------------------------------------------------------

#[test]
fn menu_in_sketch_mode() {
    let mut w = with_box();
    let id = w
        .active_tab_mut()
        .state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    w.active_tab_mut().state.open_sketch(id).unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let edits = rig.tab().state.document.edit_count();
    {
        let (se, sk) = rig.tab().state.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    for (x, y) in [(0.0, 0.0), (10.0, 4.0)] {
        let (se, sk) = rig.tab().state.session_sketch_mut().unwrap();
        se.click(sk, DVec2::new(x, y), 0.5, false).unwrap();
        rig.frame(vec![]);
    }
    rig.settle();
    let curves = |rig: &mut Rig| rig.tab().state.session_sketch().unwrap().curves().count();
    assert_eq!(curves(&mut rig), 1);
    assert_eq!(
        rig.menu_texts(),
        (
            "Rückgängig: Linie zeichnen".to_string(),
            "Wiederholen".into()
        )
    );
    rig.pick("Rückgängig: Linie zeichnen");
    assert_eq!(curves(&mut rig), 0, "Linie zurückgenommen");
    assert!(
        rig.tab().state.sketch_session.is_some(),
        "Skizze bleibt offen"
    );
    assert_eq!(
        rig.tab().state.document.edit_count(),
        edits,
        "Dokument unberührt"
    );
    assert_eq!(rig.bodies(), 1);
    assert_eq!(
        rig.menu_texts(),
        (
            "Rückgängig".to_string(),
            "Wiederholen: Linie zeichnen".into()
        ),
        "kein Dokumentschritt im Menü"
    );
    rig.pick("Wiederholen: Linie zeichnen");
    assert_eq!(curves(&mut rig), 1);
}

#[test]
fn menu_in_drawing_view() {
    let mut w = with_box();
    let d = w
        .active_tab_mut()
        .state
        .document
        .add_drawing("Blatt 1", sheet(), Projection::First)
        .unwrap();
    w.observe_history(false);
    w.active_tab_mut().state.drawing_ui.open(d);
    let mut rig = Rig::new(w);
    let (u, _) = rig.menu_texts();
    assert_eq!(u, "Rückgängig: Zeichnung anlegen");
    rig.pick(&u);
    assert!(rig.tab().state.document.drawings().is_empty());
    assert_eq!(rig.bodies(), 1);
    assert_eq!(rig.menu_texts().1, "Wiederholen: Zeichnung anlegen");
}

// ---- (d) ------------------------------------------------------------------------------------

#[test]
fn menu_with_open_dialog_closes_it() {
    let mut rig = Rig::new(with_box());
    rig.tab().dialog = Some(CreateDialog::Box {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    });
    rig.settle();
    assert!(rig.tab().dialog_open());
    let (u, _) = rig.menu_texts();
    assert_eq!(
        u, "Rückgängig: Quader anlegen",
        "bei offenem Fenster bedienbar"
    );
    rig.pick(&u);
    assert_eq!(rig.bodies(), 0, "Schritt zurück");
    assert!(!rig.tab().dialog_open(), "Fenster zu");
    assert!(rig.tab().dialog.is_none());
}

// ---- (e) ------------------------------------------------------------------------------------

#[test]
fn icons_registered() {
    assert_eq!(icons::glyph(Icon::Undo), '\u{e038}');
    assert_eq!(icons::phosphor_name(Icon::Undo), "arrow-counter-clockwise");
    assert_eq!(icons::glyph(Icon::Redo), '\u{e036}');
    assert_eq!(icons::phosphor_name(Icon::Redo), "arrow-clockwise");
    assert!(icons::ALL.contains(&Icon::Undo));
    assert!(icons::ALL.contains(&Icon::Redo));
    // Beide Symbole stehen in der Symbolschrift (Abdeckung wie in tests/fonts.rs).
    let defs = fonts::definitions();
    let name = &defs.families[&fonts::icons()][0];
    let bytes = &defs.font_data[name].font;
    use skrifa::MetadataProvider;
    let font = skrifa::FontRef::new(bytes).unwrap();
    for i in [Icon::Undo, Icon::Redo] {
        assert!(font.charmap().map(icons::glyph(i)).is_some(), "{i:?}");
    }
}

// ---- (f) ------------------------------------------------------------------------------------

#[test]
fn menu_fits_and_themes() {
    for dark in [true, false] {
        let mut w = ws();
        for _ in 0..2 {
            w.active_tab_mut()
                .state
                .create_box(DVec3::splat(5.0))
                .unwrap();
        }
        w.observe_history(false);
        w.active_tab_mut().state.document.set_marker(1).unwrap();
        w.observe_history(false);
        let mut rig = Rig::with(w, SMALL, dark);
        let seen = rig.open_menu();
        let (u, r) = first_two(&seen);
        assert_eq!(u.text, "Rückgängig: Zeitleistenmarke setzen");
        let screen = Rect::from_min_size(Pos2::ZERO, SMALL);
        assert!(!u.elided, "Text abgeschnitten");
        assert!(screen.contains_rect(u.rect), "{:?} liegt außerhalb", u.rect);
        assert!(has_shortcut(&seen, u, "Strg+Z"));
        let k = seen
            .iter()
            .find(|s| s.text == "Strg+Z" && (s.rect.center().y - u.rect.center().y).abs() < 4.0)
            .unwrap();
        assert!(screen.contains_rect(k.rect), "Kürzel außerhalb");
        assert!(k.rect.left() > u.rect.right(), "Kürzel überdeckt den Namen");
        let enabled = seen
            .iter()
            .find(|s| s.text == "Vereinigen …" && s.rect.top() > 20.0)
            .unwrap();
        assert_eq!(u.color, enabled.color, "Eintrag wie die anderen ({dark})");
        assert_ne!(r.color, u.color, "gesperrter Eintrag gedämpft ({dark})");
        // Der unterste Eintrag des Menüs liegt ebenfalls im Bild.
        let lowest = seen
            .iter()
            .filter(|s| s.rect.top() < 400.0 && s.rect.left() > u.rect.left() - 40.0)
            .map(|s| s.rect.bottom())
            .fold(0.0, f32::max);
        assert!(lowest < SMALL.y, "Menü reicht aus dem Bild");
    }
}
