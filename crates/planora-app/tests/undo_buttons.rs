//! Knöpfe „Rückgängig“ und „Wiederholen“ im oberen Feld (Aufgabe 10, Entscheidung des Nutzers:
//! oben statt im Befehlsfeld): rechts neben dem Menü, vor dem Trennstrich und den Tabs, nur
//! Symbole, Hinweistext mit Aktionsnamen und Kürzel, Wirkung wie der Menüeintrag (offene Fenster
//! schließen sich, bei offener Skizze der Skizzenverlauf), in jedem Modus (3D, Zeichnung, Skizze),
//! gesperrt wie das Menü bzw. das obere Feld (alte Version, offene Rückfrage, Export schreibt).
//! Bilder der ganzen App (`eframe::App::ui`) oder, wo ein offenes Fenster sonst darüber läge, nur
//! der Felder (`shell_panels::show_fields`).

mod common;

use eframe::App as _;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::create_dialog::CreateDialog;
use planora_app::export_dialog::{ExportDialog, ExportFormat};
use planora_app::icons::{self, Icon};
use planora_app::sketch_state::Tool;
use planora_app::workspace::*;
use planora_app::{fonts, shell, shell_panels, theme, versions_panel};
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
const MID: Vec2 = vec2(1100.0, 700.0);
const SMALL: Vec2 = vec2(800.0, 600.0);
const UNDO: &str = "Rückgängig";
const REDO: &str = "Wiederholen";
const LINE: &str = "<Linie>";

fn ws() -> Workspace {
    let root = common::temp_root("planora-undo-buttons");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Quader anlegen als erster Schritt.
fn with_box() -> Workspace {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(30.0, 20.0, 10.0))
        .unwrap();
    w.observe_history(false);
    w
}

#[derive(Debug, Clone, PartialEq)]
struct Seen {
    text: String,
    rect: Rect,
    color: egui::Color32,
    /// Ausschnitt, in dem gezeichnet wird (die Tab-Leiste rollt darin).
    clip: Rect,
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    size: Vec2,
    /// `true`: ganze App; sonst nur die Felder (ohne Fenster, Rückfragen und 3D-Ansicht).
    full: bool,
    /// Beschriftungen für Bildschirmleser aus dem letzten Bild.
    access: Vec<String>,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        Rig::with(w, SIZE, true, true)
    }

    fn with(w: Workspace, size: Vec2, dark: bool, full: bool) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, dark);
        PlanoraApp::setup_context(&ctx);
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        ctx.enable_accesskit();
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            size,
            full,
            access: vec![],
        };
        rig.tab().state.dark = dark;
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
        let full = self.full;
        let mut out = self.ctx.run_ui(raw, |ui| {
            if full {
                app.ui(ui, &mut f);
            } else {
                shell_panels::show_fields(ui.ctx(), app);
            }
        });
        fn walk(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<Seen>) {
            match shape {
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
                egui::epaint::Shape::Text(t) => {
                    let section = t.galley.job.sections.first();
                    let mut color = section.map_or(t.fallback_color, |x| x.format.color);
                    if color == egui::Color32::PLACEHOLDER {
                        color = t.fallback_color;
                    }
                    if let Some(c) = t.override_text_color {
                        color = c;
                    }
                    // Gezeichnete Farbe: egui dämpft gesperrte Knöpfe in den Eckpunkten des
                    // Textnetzes (die Abschnittsfarbe bleibt dabei unverändert).
                    if let Some(v) = t
                        .galley
                        .rows
                        .iter()
                        .find_map(|r| r.visuals.mesh.vertices.first())
                        .filter(|v| v.color != egui::Color32::PLACEHOLDER)
                    {
                        color = v.color;
                    }
                    out.push(Seen {
                        text: t.galley.text().to_string(),
                        rect: Rect::from_min_size(t.pos, t.galley.size()),
                        color: color.gamma_multiply(t.opacity_factor),
                        clip,
                    })
                }
                // Trennstriche als eigener Eintrag „<Linie>“.
                egui::epaint::Shape::LineSegment { points, stroke } => out.push(Seen {
                    text: LINE.into(),
                    rect: Rect::from_two_pos(points[0], points[1]),
                    color: stroke.color,
                    clip,
                }),
                _ => {}
            }
        }
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, c.clip_rect, &mut seen);
        }
        self.access.clear();
        if let Some(u) = out.platform_output.accesskit_update.take() {
            for (_, n) in &u.nodes {
                if let Some(l) = n.label() {
                    self.access.push(l.to_string());
                }
            }
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
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.frame(vec![]);
    }

    /// Lage des Knopfs `label` (nach einigen Bildern, damit die Lagen stehen).
    fn button(&mut self, label: &str) -> Rect {
        self.settle();
        rect_of(&self.ctx, label)
    }

    fn press(&mut self, label: &str) {
        let p = self.button(label).center();
        self.click(p);
    }

    /// Texte im Bild, nachdem der Zeiger einige Bilder über dem Knopf `label` stand.
    fn hover(&mut self, label: &str) -> Vec<String> {
        let p = self.button(label).center();
        let mut seen = vec![];
        for _ in 0..5 {
            self.time += 0.3;
            seen = self.frame(vec![Event::PointerMoved(p)]);
        }
        seen.into_iter().map(|s| s.text).collect()
    }

    /// Klick auf den Eintrag `entry` im Menü „Bearbeiten“.
    fn pick(&mut self, entry: &str) {
        let seen = self.settle();
        let bar = seen
            .iter()
            .filter(|s| s.text == "Bearbeiten")
            .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .expect("Menü „Bearbeiten“ fehlt")
            .rect
            .center();
        self.click(bar);
        let seen = self.settle();
        let p = seen
            .iter()
            .find(|s| s.text == entry && s.rect.top() > 20.0 && s.rect.top() < 400.0)
            .unwrap_or_else(|| panic!("Menüeintrag „{entry}“ fehlt"))
            .rect
            .center();
        self.click(p);
    }

    /// Das ganze Bild mit dem Zeiger an einer neutralen Stelle (außerhalb aller Felder).
    /// Reihenfolge der Ebenen und der Pfad des Testordners bleiben außen vor.
    fn picture(&mut self) -> Vec<Seen> {
        let p = pos2(2.0, 2.0);
        for _ in 0..3 {
            self.time += 0.3;
            self.frame(vec![Event::PointerMoved(p)]);
        }
        let mut seen: Vec<Seen> = self
            .frame(vec![Event::PointerMoved(p)])
            .into_iter()
            .filter(|s| !s.text.contains("planora-undo-buttons-"))
            .collect();
        seen.sort_by(|a, b| {
            (a.rect.top(), a.rect.left(), &a.text)
                .partial_cmp(&(b.rect.top(), b.rect.left(), &b.text))
                .unwrap()
        });
        seen
    }
}

fn rect_of(ctx: &egui::Context, label: &str) -> Rect {
    shell_panels::history_buttons_seen(ctx)
        .into_iter()
        .find(|(l, _)| l == label)
        .unwrap_or_else(|| panic!("Knopf „{label}“ fehlt"))
        .1
}

fn top(ctx: &egui::Context) -> Rect {
    shell_panels::last_layout(ctx).expect("Felder").top
}

fn sheet() -> Sheet {
    Sheet {
        paper: Paper::A4,
        landscape: true,
    }
}

fn slow_writer(_: &Path, _: &SheetModel, _: DrawingFormat, p: &Progress) -> Result<u64, DrawError> {
    loop {
        p.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn write_export(rig: &mut Rig, d: DrawingId) {
    let t = rig.tab();
    let s = &t.state;
    let mut e = ExportDialog::new(s);
    e.format = ExportFormat::Drawing(DrawingFormat::Svg);
    e.drawing = Some(d);
    e.tick(s, true);
    e.set_writer(slow_writer);
    let dir = common::temp_root("planora-undo-buttons-export");
    std::fs::create_dir_all(&dir).unwrap();
    e.write_to(dir.join("z.svg"), s);
    assert!(e.is_writing_file());
    t.export_dialog = Some(e);
}

// ---- (a) ------------------------------------------------------------------------------------

#[test]
fn buttons_next_to_the_menu() {
    let mut rig = Rig::new(with_box());
    let seen = rig.settle();
    let buttons = shell_panels::history_buttons_seen(&rig.ctx);
    let labels: Vec<&str> = buttons.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels, [UNDO, REDO]);
    let (undo, redo) = (buttons[0].1, buttons[1].1);
    let top = top(&rig.ctx);
    let in_top = |text: &str| {
        seen.iter()
            .find(|s| s.text.starts_with(text) && top.contains(s.rect.center()))
            .unwrap_or_else(|| panic!("„{text}“ fehlt oben"))
            .rect
    };
    let menu = in_top("Ansicht");
    let tab = in_top("Unbenannt 1");
    // Reihenfolge: Menü, Rückgängig, Wiederholen, Trennstrich, Tabs.
    assert!(menu.right() < undo.left(), "{menu:?} / {undo:?}");
    assert!(undo.right() <= redo.left(), "{undo:?} / {redo:?}");
    assert!(redo.right() < tab.left(), "{redo:?} / {tab:?}");
    for r in [undo, redo] {
        assert!(top.contains_rect(r), "{r:?} in {top:?}");
        assert!(
            (r.center().y - menu.center().y).abs() < 1.5,
            "{r:?} / {menu:?}"
        );
        assert!(r.width() <= 24.0, "nur Symbol: {r:?}");
    }
    // Kein Trennstrich mehr zwischen Menü, Knöpfen und Tabs: die Knöpfe trennen selbst (Platz
    // für die Tab-Leiste, Review I1).
    let lines: Vec<f32> = seen
        .iter()
        .filter(|s| s.text == LINE && top.contains(s.rect.center()) && s.rect.height() > 8.0)
        .map(|s| s.rect.center().x)
        .collect();
    assert!(
        !lines.iter().any(|x| *x > menu.right() && *x < tab.left()),
        "kein Trennstrich: {lines:?}"
    );
    // Die Symbole stehen in den Knöpfen.
    for (r, icon) in [(undo, Icon::Undo), (redo, Icon::Redo)] {
        let g = icons::glyph(icon).to_string();
        assert!(
            seen.iter()
                .any(|s| s.text == g && r.contains(s.rect.center())),
            "{icon:?} nicht im Knopf"
        );
    }
    // Das Befehlsfeld unten bleibt wie es war.
    assert!(
        !shell_panels::bottom_buttons(&rig.ctx)
            .iter()
            .any(|(l, _)| l == UNDO || l == REDO)
    );
}

// ---- (b) ------------------------------------------------------------------------------------

#[test]
fn buttons_do_what_the_menu_does() {
    let mut by_button = Rig::new(with_box());
    let mut by_menu = Rig::new(with_box());
    assert_eq!(by_button.bodies(), 1);

    by_button.press(UNDO);
    by_menu.pick("Rückgängig: Quader anlegen");
    for rig in [&mut by_button, &mut by_menu] {
        assert_eq!(rig.bodies(), 0, "Quader weg");
        assert_eq!(
            rig.tab().state.info.as_deref(),
            Some("Rückgängig: Quader anlegen")
        );
        assert_eq!(rig.tab().redo_label(), Some("Quader anlegen"));
        assert_eq!(rig.tab().undo_label(), None);
    }
    assert_eq!(
        by_button.picture(),
        by_menu.picture(),
        "gleiches Bild danach"
    );

    by_button.press(REDO);
    by_menu.pick("Wiederholen: Quader anlegen");
    for rig in [&mut by_button, &mut by_menu] {
        assert_eq!(rig.bodies(), 1, "Quader wieder da");
        assert_eq!(
            rig.tab().state.info.as_deref(),
            Some("Wiederholen: Quader anlegen")
        );
        assert_eq!(rig.tab().undo_label(), Some("Quader anlegen"));
        assert_eq!(rig.tab().redo_label(), None);
    }
    assert_eq!(by_button.picture(), by_menu.picture());

    // Ein gesperrter Knopf tut nichts (wie der gesperrte Menüeintrag).
    by_button.press(REDO);
    assert_eq!(by_button.bodies(), 1);
    assert_eq!(
        by_button.tab().state.info.as_deref(),
        Some("Wiederholen: Quader anlegen")
    );
}

/// Bei offenem Fenster bleibt der Knopf bedienbar und schließt das Fenster (Plan Q2).
#[test]
fn button_with_open_dialog_closes_it() {
    let mut rig = Rig::new(with_box());
    rig.tab().dialog = Some(CreateDialog::Box {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    });
    rig.settle();
    assert!(rig.tab().dialog_open());
    rig.press(UNDO);
    assert_eq!(rig.bodies(), 0, "Schritt zurück");
    assert!(!rig.tab().dialog_open(), "Fenster zu");
}

/// In der offenen Skizze wirken die Knöpfe auf den Skizzenverlauf; das Dokument bleibt.
#[test]
fn buttons_in_sketch_mode() {
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
    assert!(top(&rig.ctx).contains_rect(rig.button(UNDO)));
    let texts = rig.hover(UNDO);
    assert!(
        texts
            .iter()
            .any(|t| t == "Rückgängig: Linie zeichnen (Strg+Z)"),
        "{texts:?}"
    );
    rig.press(UNDO);
    assert_eq!(curves(&mut rig), 0, "Linie zurückgenommen");
    assert!(rig.tab().state.sketch_session.is_some(), "Skizze offen");
    assert_eq!(rig.tab().state.document.edit_count(), edits);
    assert_eq!(rig.bodies(), 1);
    rig.press(REDO);
    assert_eq!(curves(&mut rig), 1);
}

/// In der Zeichnung stehen die Knöpfe ebenfalls oben und wirken wie das Menü.
#[test]
fn buttons_in_drawing_view() {
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
    assert!(top(&rig.ctx).contains_rect(rig.button(UNDO)));
    let texts = rig.hover(UNDO);
    assert!(
        texts
            .iter()
            .any(|t| t == "Rückgängig: Zeichnung anlegen (Strg+Z)"),
        "{texts:?}"
    );
    rig.press(UNDO);
    assert!(rig.tab().state.document.drawings().is_empty());
    assert_eq!(rig.bodies(), 1);
    // Zurück im Modell steht der Knopf weiter oben.
    rig.press(REDO);
    assert_eq!(rig.tab().state.document.drawings().len(), 1);
}

/// Der Klick bleibt im oberen Feld: die 3D-Ansicht darunter wählt nichts und dreht nicht.
#[test]
fn clicks_stay_on_the_field() {
    let mut w = with_box();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(60.0))
        .unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let fill = |rig: &mut Rig| {
        let state = &mut rig.tab().state;
        state.frame_all();
        // Der Körper füllt das ganze Fenster, auch unter dem oberen Feld.
        state.camera.distance *= 0.25;
        rig.settle();
    };
    fill(&mut rig);
    let cam = |rig: &mut Rig| {
        let s = &rig.tab().state;
        (s.camera.yaw, s.camera.pitch, s.camera.distance)
    };
    let before = cam(&mut rig);
    let p = rig.button(UNDO).center();
    let layer = rig.ctx.layer_id_at(p).expect("Ebene unter dem Zeiger");
    assert_eq!(layer.order, egui::Order::Middle);
    rig.click(p);
    assert_eq!(rig.bodies(), 1, "Rückgängig wirkte");
    assert_eq!(rig.tab().state.selection, None, "nichts gewählt");
    assert_eq!(cam(&mut rig), before, "Ansicht unverändert");
    // Auch Ziehen vom Knopf aus dreht nichts.
    let b = |at, pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let to = p + vec2(60.0, 80.0);
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![b(p, true)]);
    for i in 1..=6 {
        rig.frame(vec![Event::PointerMoved(p + (to - p) * (i as f32 / 6.0))]);
    }
    rig.frame(vec![b(to, false)]);
    rig.settle();
    assert_eq!(cam(&mut rig), before, "Ziehen vom Knopf dreht nicht");
    // Gegenprobe: derselbe Klick in der freien Mitte wählt den verbliebenen Quader.
    fill(&mut rig);
    let free = shell_panels::last_layout(&rig.ctx).unwrap().free.center();
    rig.click(free);
    assert!(
        rig.tab().state.selection.is_some(),
        "Klick in der Mitte wählt"
    );
}

// ---- (c) ------------------------------------------------------------------------------------

#[test]
fn tooltips_name_the_action() {
    let mut rig = Rig::new(with_box());
    let texts = rig.hover(UNDO);
    assert!(
        texts
            .iter()
            .any(|t| t == "Rückgängig: Quader anlegen (Strg+Z)"),
        "{texts:?}"
    );
    // Name für Bildschirmleser: der Menütext.
    assert!(
        rig.access.iter().any(|l| l == "Rückgängig: Quader anlegen"),
        "{:?}",
        rig.access
    );
    // Gesperrt: Name und Grund.
    let texts = rig.hover(REDO);
    assert!(texts.iter().any(|t| t == REDO), "{texts:?}");
    assert!(
        texts.iter().any(|t| t == "Es gibt nichts zu wiederholen."),
        "{texts:?}"
    );
    assert!(!texts.iter().any(|t| t.ends_with("(Strg+Y)")), "{texts:?}");

    rig.press(UNDO);
    let texts = rig.hover(REDO);
    assert!(
        texts
            .iter()
            .any(|t| t == "Wiederholen: Quader anlegen (Strg+Y)"),
        "{texts:?}"
    );
    assert!(
        rig.access
            .iter()
            .any(|l| l == "Wiederholen: Quader anlegen"),
        "{:?}",
        rig.access
    );
    let texts = rig.hover(UNDO);
    assert!(texts.iter().any(|t| t == UNDO), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|t| t == "Es gibt nichts rückgängig zu machen."),
        "{texts:?}"
    );
}

/// Während das Export-Fenster eine Datei schreibt, ist Rückgängig gesperrt (trotz Schritten).
#[test]
fn locked_while_exporting() {
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
    // Nur die Felder: das Export-Fenster läge sonst über dem oberen Feld.
    let mut rig = Rig::with(w, SIZE, true, false);
    write_export(&mut rig, d);
    rig.settle();
    let texts = rig.hover(UNDO);
    assert!(texts.iter().any(|t| t == UNDO), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|t| t == "Während des Exports ist das nicht möglich."),
        "{texts:?}"
    );
    rig.press(UNDO);
    assert_eq!(rig.tab().state.document.drawings().len(), 1, "gesperrt");
    assert!(rig.tab().export_dialog.is_some());
    rig.tab().export_dialog.as_mut().unwrap().cancel_write();
}

// ---- (d) ------------------------------------------------------------------------------------

/// Bei 1100 × 700 und 800 × 600 stehen beide Knöpfe ganz im oberen Feld; bei 1100 (und 1400)
/// bleibt das Feld zwischen den Spalten. Auch mit vielen Tabs: die Knöpfe bleiben stehen, der
/// aktive Tab ist ganz zu sehen (die Tabs rollen hinter den Knöpfen).
#[test]
fn fits_at_1100_and_800_also_with_many_tabs() {
    for (size, tabs) in [(MID, 1), (MID, 8), (SMALL, 1), (SMALL, 8), (SIZE, 8)] {
        let mut w = with_box();
        for _ in 1..tabs {
            w.new_tab();
        }
        let mut rig = Rig::with(w, size, true, true);
        // Der aktive Tab rollt mit kurzer Bewegung ins Bild.
        for _ in 0..20 {
            rig.time += 0.1;
            rig.frame(vec![]);
        }
        let seen = rig.settle();
        let l = shell_panels::last_layout(&rig.ctx).unwrap();
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        assert!(screen.contains_rect(l.top), "{size:?}: {:?}", l.top);
        if size != SMALL {
            assert!(
                l.top.width() <= shell::between_columns(size.x) + 0.5,
                "{size:?}/{tabs}: {:?} über den Spalten",
                l.top
            );
            assert!(
                l.left.top() < l.top.bottom() + 2.0 * shell::GAP,
                "{size:?}/{tabs}: Spalten beginnen unter dem Feld"
            );
        }
        let redo = rect_of(&rig.ctx, REDO);
        for label in [UNDO, REDO] {
            let r = rect_of(&rig.ctx, label);
            assert!(
                l.top.contains_rect(r),
                "{size:?}/{tabs}: {label} {r:?} in {:?}",
                l.top
            );
        }
        let name = rig.tab().title();
        let name = name.trim_end_matches(" ●");
        let active = seen
            .iter()
            .find(|s| s.text.starts_with(name) && l.top.contains(s.rect.center()))
            .unwrap_or_else(|| {
                panic!(
                    "{size:?}/{tabs}: aktiver Tab „{name}“ fehlt: {:?}",
                    seen.iter()
                        .filter(|s| l.top.intersects(s.rect))
                        .map(|s| (&s.text, s.rect))
                        .collect::<Vec<_>>()
                )
            });
        assert!(
            l.top.contains_rect(active.rect) && active.rect.left() > redo.right(),
            "{size:?}/{tabs}: aktiver Tab {:?} sichtbar hinter den Knöpfen",
            active.rect
        );
    }
}

// ---- (e) ------------------------------------------------------------------------------------

fn read_only_ws() -> Workspace {
    let mut w = ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    for _ in 0..2 {
        w.active_tab_mut()
            .state
            .create_box(DVec3::splat(3.0))
            .unwrap();
    }
    w.observe_history(false);
    w.save_active("Stand 2", None).unwrap();
    versions_panel::open_version(&mut w, 1);
    assert!(w.active_tab().read_only);
    w
}

#[test]
fn locked_in_old_versions_and_prompts() {
    // Alte Version.
    let mut rig = Rig::new(read_only_ws());
    let bodies = rig.bodies();
    let texts = rig.hover(UNDO);
    assert!(
        texts
            .iter()
            .any(|t| t == "In einer alten Version lässt sich nichts ändern."),
        "{texts:?}"
    );
    rig.press(UNDO);
    rig.press(REDO);
    assert_eq!(rig.bodies(), bodies);
    assert_eq!(rig.tab().state.info, None);

    // Offene Rückfrage (nur die Felder: das Fenster der Rückfrage läge sonst darüber).
    let mut w = with_box();
    let tab = w.active_tab().id;
    w.prompt = Some(Prompt::SaveChanges {
        tab,
        then: After::CloseTab,
    });
    let mut rig = Rig::with(w, SIZE, true, false);
    let texts = rig.hover(UNDO);
    assert!(
        texts
            .iter()
            .any(|t| t == "Zuerst die offene Rückfrage beantworten."),
        "{texts:?}"
    );
    // Vorrang der Gründe: die Rückfrage vor „nichts zu wiederholen“ (Wiederholen hat hier
    // keinen Schritt).
    let texts = rig.hover(REDO);
    assert!(
        texts
            .iter()
            .any(|t| t == "Zuerst die offene Rückfrage beantworten."),
        "{texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t == "Es gibt nichts zu wiederholen."),
        "{texts:?}"
    );
    rig.press(UNDO);
    assert_eq!(rig.bodies(), 1, "gesperrt bei offener Rückfrage");
    assert_eq!(rig.tab().state.info, None);
    // Gegenprobe: ohne Rückfrage wirkt derselbe Klick.
    rig.app.ws_mut().prompt = None;
    rig.press(UNDO);
    assert_eq!(rig.bodies(), 0);
}

// ---- (f) ------------------------------------------------------------------------------------

#[test]
fn themes() {
    for dark in [true, false] {
        let mut rig = Rig::with(with_box(), SIZE, dark, true);
        let p = theme::palette(dark);
        let glyph = |rig: &Rig, seen: &[Seen], icon: Icon, label: &str| {
            let r = rect_of(&rig.ctx, label);
            let g = icons::glyph(icon).to_string();
            seen.iter()
                .find(|s| s.text == g && r.contains(s.rect.center()))
                .unwrap_or_else(|| panic!("{icon:?} fehlt ({dark})"))
                .color
        };
        let seen = rig.picture();
        let undo = glyph(&rig, &seen, Icon::Undo, UNDO);
        let redo = glyph(&rig, &seen, Icon::Redo, REDO);
        // Bereit: Symbolfarbe wie die übrigen Symbolknöpfe; gesperrt: gedämpft.
        assert_eq!(undo, p.text_secondary, "dark={dark}");
        assert!(
            redo.a() < undo.a(),
            "dark={dark}: gesperrt gedämpft {redo:?}"
        );
        // Überfahren: kräftigere Farbe aus der Palette.
        let at = rect_of(&rig.ctx, UNDO).center();
        let mut seen = vec![];
        for _ in 0..3 {
            seen = rig.frame(vec![Event::PointerMoved(at)]);
        }
        assert_eq!(
            glyph(&rig, &seen, Icon::Undo, UNDO),
            p.text_primary,
            "dark={dark}"
        );
    }
}

// ---- Tab-Leiste neben den Knöpfen (Review I1, Entscheidung Option A) -----------------------

/// Arbeitsbereich mit `n` Tabs; der letzte ist aktiv, auf Wunsch geändert (Titel mit „●“).
fn tabs_ws(n: usize, dirty: bool) -> Workspace {
    let mut w = ws();
    for _ in 1..n {
        w.new_tab();
    }
    if dirty {
        w.active_tab_mut()
            .state
            .create_box(DVec3::splat(5.0))
            .unwrap();
    }
    w
}

/// Was von der Tab-Leiste zu sehen ist, nachdem der aktive Tab ins Bild gerollt ist.
struct Strip {
    /// Sichtbarer Ausschnitt der rollenden Leiste.
    clip: Rect,
    /// Name des aktiven Tabs (ungeschnitten).
    name: Rect,
    /// Schließen-Symbol des aktiven Tabs; `None`: ganz außerhalb des Ausschnitts (egui zeichnet
    /// es dann nicht).
    close: Option<Rect>,
    /// Symbol „Neu“ und sein Ausschnitt.
    plus: Rect,
    plus_clip: Rect,
    top: Rect,
    redo: Rect,
}

impl Strip {
    /// Um wie viel das Schließen-Symbol rechts über den Ausschnitt ragt (≤ 0: ganz sichtbar;
    /// `None`: gar nicht zu sehen).
    fn close_hidden(&self) -> Option<f32> {
        self.close.map(|c| c.right() - self.clip.right())
    }

    fn name_shown(&self) -> bool {
        self.clip.contains_rect(self.name)
    }
}

fn strip(rig: &mut Rig) -> Strip {
    // Der aktive Tab rollt mit kurzer Bewegung ins Bild.
    for _ in 0..20 {
        rig.time += 0.1;
        rig.frame(vec![]);
    }
    let seen = rig.settle();
    let top = top(&rig.ctx);
    let title = rig.tab().title();
    let name = title.trim_end_matches(" ●").to_string();
    let head = seen
        .iter()
        .find(|s| s.text.starts_with(&name) && top.intersects(s.rect))
        .unwrap_or_else(|| panic!("aktiver Tab „{name}“ fehlt"));
    let close_glyph = icons::glyph(Icon::Close).to_string();
    let close = seen
        .iter()
        .filter(|s| s.text == close_glyph && s.rect.left() >= head.rect.right() - 1.0)
        .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
        .map(|s| s.rect);
    let plus_glyph = icons::glyph(Icon::NewTab).to_string();
    let plus = seen
        .iter()
        .find(|s| s.text == plus_glyph && top.intersects(s.rect))
        .expect("Symbol „Neu“");
    Strip {
        clip: head.clip.intersect(top),
        name: head.rect,
        close,
        plus: plus.rect,
        plus_clip: plus.clip,
        top,
        redo: rect_of(&rig.ctx, REDO),
    }
}

/// „Neu“ steht fest hinter der rollenden Tab-Leiste: immer ganz sichtbar und klickbar, bei 1100 ×
/// 700 und 800 × 600 mit 1, 2 und 5 Tabs (geändert und ungeändert). Der aktive Tab ist ins Bild
/// gerollt; die Knöpfe überdecken keinen Tab.
///
/// Gemessen bei 1100 × 700 (Feld 544 px = Rand 26 + Menü 319,25 + Knöpfe mit Abständen 60 + Tab-Leiste
/// 100,75 + „Neu“ mit Abstand 38): Ein aktiver Tab samt Schließen-Symbol braucht etwa 127 px
/// („Unbenannt n“) bzw. 136 px („Unbenannt n ●“). Deshalb (festgehalten, damit eine Änderung auffällt):
/// - 1 Tab, ungeändert: Name ganz sichtbar, Schließen-Symbol ragt 17,8 px über den Rand;
/// - 1 Tab geändert, oder 2/5 Tabs ungeändert: Name ganz sichtbar, Schließen-Symbol ganz außerhalb;
/// - 2/5 Tabs, aktiver geändert: Name am Ende (Kennzeichen „●“) angeschnitten, Schließen-Symbol
///   ganz außerhalb.
///
/// Das Schließen-Symbol ist nach Rollen erreichbar; Strg+W schließt den aktiven Tab (siehe
/// `ctrl_w_closes_the_active_tab_at_1100`).
///
/// Bei 800 × 600 steht das Feld wie bisher über den Spalten: Name und Schließen-Symbol ganz sichtbar.
#[test]
fn new_tab_pinned_and_active_tab_revealed() {
    for size in [MID, SMALL] {
        for n in [1, 2, 5] {
            for dirty in [false, true] {
                let what = format!("{size:?}/{n} Tabs/geändert={dirty}");
                let mut rig = Rig::with(tabs_ws(n, dirty), size, true, true);
                let s = strip(&mut rig);
                // „Neu“ ganz sichtbar, außerhalb der rollenden Leiste, rechts davon.
                assert!(
                    s.top.contains_rect(s.plus) && s.plus_clip.contains_rect(s.plus),
                    "{what}: „Neu“ {:?} sichtbar",
                    s.plus
                );
                assert!(
                    s.plus.left() >= s.clip.right(),
                    "{what}: „Neu“ hinter der Leiste"
                );
                // Die Knöpfe stehen vor der Leiste, kein Tab darunter.
                assert!(
                    s.redo.right() <= s.clip.left(),
                    "{what}: Knöpfe vor den Tabs"
                );
                // Der aktive Tab ist ins Bild gerollt: ein Teil von ihm ist sichtbar.
                assert!(
                    s.clip.intersects(s.name),
                    "{what}: aktiver Tab {:?} in {:?}",
                    s.name,
                    s.clip
                );
                if size == SMALL {
                    assert!(s.name_shown(), "{what}: Name ganz sichtbar");
                    assert!(
                        s.close_hidden().is_some_and(|h| h <= 0.0),
                        "{what}: × ganz sichtbar"
                    );
                } else {
                    assert!(
                        (s.clip.width() - 100.75).abs() < 0.5,
                        "{what}: Leiste {}",
                        s.clip.width()
                    );
                    // Festgehaltene Messung bei 1100 (siehe Doku oben).
                    if n == 1 && !dirty {
                        assert!(s.name_shown(), "{what}");
                        let h = s.close_hidden().expect("×");
                        assert!((h - 17.8).abs() < 1.0, "{what}: × ragt {h}");
                    } else {
                        assert_eq!(
                            s.name_shown(),
                            !(dirty && n > 1),
                            "{what}: Name ganz sichtbar"
                        );
                        assert_eq!(s.close_hidden(), None, "{what}: × außerhalb");
                    }
                }
                // Klick auf „Neu“ legt einen Tab an.
                rig.click(s.plus.center());
                assert_eq!(rig.app.ws_mut().tabs.len(), n + 1, "{what}: „Neu“ klickbar");
            }
        }
    }
}

/// Ab 1150 px Breite sind Name und Schließen-Symbol des aktiven Tabs ganz zu sehen (1 bis 9
/// Tabs, geändert und ungeändert). Bei 1140 px gilt das für ungeänderte Tabs; bei einem
/// geänderten aktiven Tab ragt das Schließen-Symbol je nach Breite der Tabnummer bis etwa 0,6 px
/// hinaus (4, 7, 8, 9 Tabs; gemessen, festgehalten). Bei 1130 px (Leiste 130,75 px) ragt es bei
/// einem geänderten Tab und 2 und mehr Tabs etwa 9,9 px hinaus (gemessen bei 2 und 5 Tabs).
#[test]
fn active_tab_fully_visible_from_1150() {
    for width in [1130.0, 1140.0, 1150.0, 1200.0, 1400.0] {
        let counts: Vec<usize> = if width == 1130.0 {
            vec![1, 2, 5]
        } else {
            (1..=9).collect()
        };
        for n in counts {
            for dirty in [false, true] {
                let what = format!("{width}/{n} Tabs/geändert={dirty}");
                let mut rig = Rig::with(tabs_ws(n, dirty), vec2(width, 700.0), true, true);
                let s = strip(&mut rig);
                let h = s.close_hidden();
                assert!(s.plus_clip.contains_rect(s.plus), "{what}: „Neu“ sichtbar");
                if width == 1400.0 && n == 1 {
                    // „Neu“ folgt dicht auf das Schließen-Symbol des letzten Tabs.
                    let gap = s.plus.left() - s.close.expect("×").right();
                    // Gemessen 24,25 px (Innenabstände beider Symbole und ein Abstand).
                    assert!((gap - 24.25).abs() < 1.0, "{what}: Abstand × → Neu {gap}");
                }
                if width == 1130.0 && dirty && n > 1 {
                    assert!(s.name_shown(), "{what}: Name");
                    let h = h.expect("×");
                    assert!((h - 9.9).abs() < 1.0, "{what}: × ragt {h}");
                } else if width == 1140.0 && dirty && [4, 7, 8, 9].contains(&n) {
                    assert!(s.name_shown(), "{what}: Name");
                    let h = h.expect("×");
                    assert!(h > 0.0 && h < 0.6, "{what}: × ragt {h}");
                } else {
                    assert!(s.name_shown(), "{what}: Name ganz sichtbar");
                    assert!(
                        h.is_some_and(|h| h <= 0.0),
                        "{what}: × ganz sichtbar ({h:?})"
                    );
                }
            }
        }
    }
}

/// Bei 1100 × 700 schließt Strg+W weiter den aktiven Tab (auch wenn sein Schließen-Symbol erst nach
/// Rollen zu sehen ist).
#[test]
fn ctrl_w_closes_the_active_tab_at_1100() {
    let mut rig = Rig::with(tabs_ws(2, false), MID, true, true);
    let s = strip(&mut rig);
    assert!(
        s.close_hidden().is_none_or(|h| h > 0.0),
        "× nicht ganz sichtbar"
    );
    let active = rig.tab().id;
    rig.frame(vec![Event::Key {
        key: egui::Key::W,
        physical_key: Some(egui::Key::W),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    rig.settle();
    assert_eq!(rig.app.ws_mut().tabs.len(), 1);
    assert_ne!(rig.tab().id, active, "der aktive Tab ist zu");
}
