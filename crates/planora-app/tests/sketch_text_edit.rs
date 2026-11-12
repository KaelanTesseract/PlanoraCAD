//! Skizzen 8, Task 13: Text ändern und auflösen in der App (Spec C8, C9, C10, B8, B9, B10, G4).
//! Doppelklick auf einen Buchstaben und „Text bearbeiten“ öffnen den Abschnitt „Text“ mit den
//! Werten des Textes, OK ist ein Rückgängig-Schritt „Text ändern“, Abbrechen stellt alles wieder
//! her. „Text auflösen“ über Menü, Knopf und Aktion als ein Schritt. Ein Klick auf einen
//! Buchstaben wählt den ganzen Text, Entf löscht ihn ganz oder meldet, warum nicht. Hinweise auf
//! Ersatzschrift, nachgebildete Schnitte und fehlende Zeichen. Solange die Schriftliste gelesen
//! wird, warten Abschnitt und „Text auflösen“.
//!
//! Die ganze App läuft ohne Fenster bei 1920×1080 mit den mitgelieferten Schriften
//! ([`FontLibrary::bundled_only`], nie Systemschriften), Rohereignisse Bild für Bild.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands;
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult};
use planora_app::sketch_state::{Item, SketchSession, Tool};
use planora_app::sketch_text::{TextTool, TextToolKind};
use planora_app::state::{AppState, FontState};
use planora_app::workspace::{DocTab, Workspace};
use planora_app::{fonts, theme};
use planora_feature::{FeatureKind, SketchPlane, StandardPlane};
use planora_i18n::tr;
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use planora_sketch::snap::Snap;
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShaper, TextStyle,
};
use planora_text::FontLibrary;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
/// Gegenüberliegende Ecken des Rahmens (Bildschirm).
const FRAME_A: Pos2 = pos2(800.0, 600.0);
const FRAME_B: Pos2 = pos2(1200.0, 800.0);
/// Zweiter Text weiter oben.
const FRAME2_A: Pos2 = pos2(800.0, 300.0);
const FRAME2_B: Pos2 = pos2(1200.0, 450.0);
/// Eine Linie als Pfad, abseits der Rahmen.
const LINE_A: Pos2 = pos2(500.0, 900.0);
const LINE_B: Pos2 = pos2(1300.0, 900.0);
/// Freie Stelle ohne Geometrie.
const AWAY: Pos2 = pos2(1400.0, 200.0);
/// Eine Schrift, die es nirgends gibt.
const MISSING: &str = "Gibtsnicht Grotesk";

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn key_with(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Strg wie unter Windows: `ctrl` und `command`.
fn ctrl() -> Modifiers {
    Modifiers::CTRL | Modifiers::COMMAND
}

fn pointer(p: Pos2, b: PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers,
    }
}

/// Texte eines Bilds mit ihrer Lage.
#[derive(Default)]
struct Shot {
    texts: Vec<(String, Rect)>,
    /// Mitten und Radien der gezeichneten Kreise (Punkte der Skizze).
    circles: Vec<(Pos2, f32)>,
    /// Eckpunkte aller gezeichneten Linien, Linienzüge, Bézierstücke und Flächen (Netze).
    vertices: Vec<Pos2>,
    /// Gezeichnete Linienzüge mit ihrer Farbe.
    strokes: Vec<(Vec<Pos2>, egui::Color32)>,
    /// Wann das Bild ein nächstes anfordert.
    repaint: Duration,
}

fn walk(shape: &Shape, s: &mut Shot) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
        Shape::Text(t) => s.texts.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        Shape::Circle(c) => s.circles.push((c.center, c.radius)),
        Shape::LineSegment { points, stroke } => {
            s.vertices.extend(points);
            s.strokes.push((points.to_vec(), stroke.color));
        }
        Shape::Path(p) => {
            s.vertices.extend(&p.points);
            if let egui::epaint::ColorMode::Solid(c) = p.stroke.color {
                s.strokes.push((p.points.clone(), c));
            }
        }
        Shape::QuadraticBezier(b) => s.vertices.extend(b.points),
        Shape::CubicBezier(b) => s.vertices.extend(b.points),
        Shape::Mesh(m) => s.vertices.extend(m.vertices.iter().map(|v| v.pos)),
        _ => {}
    }
}

impl Shot {
    /// Der Text `text` am weitesten oben (die Menüleiste vor gleichnamigen Abschnitten).
    fn find_top(&self, text: &str) -> Pos2 {
        self.texts
            .iter()
            .filter(|(t, _)| t == text)
            .min_by(|a, b| a.1.top().total_cmp(&b.1.top()))
            .unwrap_or_else(|| panic!("„{text}“ fehlt"))
            .1
            .center()
    }

    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|(t, _)| t == text)
    }

    /// Der letzte (oberste) Text `text`.
    fn find(&self, text: &str) -> Pos2 {
        self.texts
            .iter()
            .rfind(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("„{text}“ fehlt"))
            .1
            .center()
    }
}

fn style(font: &str) -> TextStyle {
    TextStyle {
        font: font.into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    /// App mit offener, leerer Skizze auf XY; Schriftliste: die mitgelieferten.
    fn new() -> Rig {
        Rig::with_fonts(None)
    }

    /// Wie [`Rig::new`]; `fonts` ersetzt den Stand der Schriftliste vor dem ersten Bild.
    fn with_fonts(fonts_state: Option<FontState>) -> Rig {
        let root = common::temp_root("planora-sketch-text-edit");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        w.observe_history(false);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut app = PlanoraApp::for_tests(w);
        if let Some(f) = fonts_state {
            app.set_font_state(f);
        }
        let mut rig = Rig {
            ctx,
            app,
            time: 0.0,
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn tab(&mut self) -> &mut DocTab {
        self.app.ws_mut().active_tab_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.tab().state
    }

    fn se(&mut self) -> &mut SketchSession {
        self.st().sketch_session.as_mut().expect("Skizze offen")
    }

    fn tool(&mut self) -> &mut TextTool {
        self.se().text.as_mut().expect("Werkzeug Text läuft")
    }

    fn sk(&mut self) -> &Sketch {
        self.st().session_sketch().expect("Skizze offen")
    }

    fn frame(&mut self, events: Vec<Event>) -> Shot {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut shot = Shot::default();
        for c in &out.shapes {
            walk(&c.shape, &mut shot);
        }
        shot.repaint = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self) -> Shot {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn press_with(&mut self, k: Key, modifiers: Modifiers) -> Shot {
        self.frame(vec![key_with(k, modifiers)]);
        self.settle()
    }

    fn press(&mut self, k: Key) -> Shot {
        self.press_with(k, NONE)
    }

    fn click_mod(&mut self, p: Pos2, b: PointerButton, modifiers: Modifiers) -> Shot {
        // Kein Doppelklick mit dem vorigen Klick.
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![pointer(p, b, true, modifiers)]);
        self.frame(vec![pointer(p, b, false, modifiers)]);
        self.settle()
    }

    fn click(&mut self, p: Pos2) -> Shot {
        self.click_mod(p, PointerButton::Primary, NONE)
    }

    fn shift_click(&mut self, p: Pos2) -> Shot {
        self.click_mod(p, PointerButton::Primary, Modifiers::SHIFT)
    }

    fn double_click(&mut self, p: Pos2) -> Shot {
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        for _ in 0..2 {
            self.frame(vec![pointer(p, PointerButton::Primary, true, NONE)]);
            self.frame(vec![pointer(p, PointerButton::Primary, false, NONE)]);
        }
        self.settle()
    }

    /// Zieht mit der linken Taste von `a` nach `b` (Auswahlrahmen auf freier Fläche).
    fn drag(&mut self, a: Pos2, b: Pos2) -> Shot {
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(a)]);
        self.frame(vec![pointer(a, PointerButton::Primary, true, NONE)]);
        for k in 1..=4 {
            let t = k as f32 / 4.0;
            self.frame(vec![Event::PointerMoved(a + (b - a) * t)]);
        }
        self.frame(vec![pointer(b, PointerButton::Primary, false, NONE)]);
        self.settle()
    }

    /// Tippt in das Feld, das die Tastatur hat.
    fn type_text(&mut self, text: &str) -> Shot {
        self.frame(vec![Event::Text(text.to_string())]);
        self.settle()
    }

    /// Klickt den Text `label` des aktuellen Bilds an (Knopf, Umschalter, Eintrag).
    fn click_label(&mut self, label: &str) -> Shot {
        let at = self.settle().find(label);
        self.click(at)
    }

    fn action(&mut self, a: Action) -> Shot {
        commands::run_action(self.app.ws_mut(), a);
        self.settle()
    }

    fn undo_len(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn data(&mut self) -> String {
        format!("{:?}", self.sk().to_data())
    }

    /// Die Lage in der Skizze unter dem Bildschirmpunkt `p` (der Zeiger ohne Fang).
    fn sketch_at(&mut self, p: Pos2) -> DVec2 {
        self.frame(vec![Event::PointerMoved(p)]);
        self.se().cursor.expect("Zeiger über der Skizze")
    }

    /// Der Bildschirmpunkt über der Lage `q` der Skizze: über drei Proben und zwei Schritte
    /// Nachbessern (die Abbildung ist für die Draufsicht affin).
    /// Die Abbildung Skizze → Bildschirm aus drei Proben (für die Draufsicht affin), für viele
    /// Punkte auf einmal.
    fn mapping(&mut self) -> impl Fn(DVec2) -> Pos2 + use<> {
        let p0 = pos2(960.0, 540.0);
        let s0 = self.sketch_at(p0);
        let sx = self.sketch_at(p0 + vec2(100.0, 0.0)) - s0;
        let sy = self.sketch_at(p0 + vec2(0.0, 100.0)) - s0;
        let det = sx.x * sy.y - sy.x * sx.y;
        move |q: DVec2| {
            let d = q - s0;
            let a = (d.x * sy.y - sy.x * d.y) / det;
            let b = (sx.x * d.y - d.x * sx.y) / det;
            p0 + vec2((a * 100.0) as f32, (b * 100.0) as f32)
        }
    }

    fn screen(&mut self, q: DVec2) -> Pos2 {
        let p0 = pos2(960.0, 540.0);
        let s0 = self.sketch_at(p0);
        let sx = self.sketch_at(p0 + vec2(100.0, 0.0)) - s0;
        let sy = self.sketch_at(p0 + vec2(0.0, 100.0)) - s0;
        // Spalten: Skizze je 100 px in x und y.
        let det = sx.x * sy.y - sy.x * sx.y;
        let solve = |d: DVec2| {
            let a = (d.x * sy.y - sy.x * d.y) / det;
            let b = (sx.x * d.y - d.x * sx.y) / det;
            vec2((a * 100.0) as f32, (b * 100.0) as f32)
        };
        let mut p = p0 + solve(q - s0);
        for _ in 0..2 {
            let now = self.sketch_at(p);
            p += solve(q - now);
        }
        p
    }

    /// Ein Text im Rahmen von `a` bis `b` (Bildschirm), über die Skizze angelegt und gesetzt mit
    /// den mitgelieferten Schriften (ein Rückgängig-Schritt). Die Höhe ist die Hälfte des Rahmens,
    /// der Text steht mittig.
    fn api_text(&mut self, content: &str, a: Pos2, b: Pos2) -> TextId {
        let (pa, pb) = (self.sketch_at(a), self.sketch_at(b));
        let (lo, hi) = (pa.min(pb), pa.max(pb));
        let height = 0.5 * (hi.y - lo.y);
        let shape = Arc::new(
            FontLibrary::bundled_only()
                .shape(content, &style("Montserrat"))
                .unwrap(),
        );
        let t = {
            let (_, sk) = self.st().session_sketch_mut().unwrap();
            let t = sk
                .add_text_box(
                    lo,
                    hi,
                    TextParams {
                        content: content.into(),
                        style: style("Montserrat"),
                        height,
                        flip_h: false,
                        flip_v: false,
                    },
                    HAlign::Center,
                    VAlign::Middle,
                    shape,
                )
                .unwrap();
            sk.solve();
            t
        };
        self.st().observe_sketch(false);
        self.settle();
        t
    }

    /// Die Linie von [`LINE_A`] nach [`LINE_B`] über die Skizze (ein Schritt).
    fn api_line(&mut self) -> CurveId {
        let (a, b) = (self.sketch_at(LINE_A), self.sketch_at(LINE_B));
        let c = {
            let (_, sk) = self.st().session_sketch_mut().unwrap();
            let pa = sk.add_point(a.x, a.y).unwrap();
            let pb = sk.add_point(b.x, b.y).unwrap();
            sk.add_line(pa, pb).unwrap()
        };
        self.st().observe_sketch(false);
        self.settle();
        c
    }

    /// Die Bildschirmlage der Mitte der längsten Strecke unter den Buchstaben des Textes `t`.
    fn glyph_spot(&mut self, t: TextId) -> Pos2 {
        let sk = self.sk();
        let mut best: Option<(f64, DVec2)> = None;
        for &c in &sk.text(t).unwrap().curves {
            if let CurveKind::Line { a, b } = sk.curve(c).unwrap().kind {
                let (pa, pb) = (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap());
                let len = pa.distance(pb);
                if best.is_none_or(|(l, _)| len > l) {
                    best = Some((len, (pa + pb) * 0.5));
                }
            }
        }
        let at = best.expect("eine Strecke im Buchstaben").1;
        self.screen(at)
    }

    /// Die Bildschirmlage der Mitte der Rahmenlinie `i` des Textes `t`.
    fn frame_spot(&mut self, t: TextId, i: usize) -> Pos2 {
        let sk = self.sk();
        let TextPlacement::Box { corners, .. } = sk.text(t).unwrap().placement else {
            panic!("Text im Rahmen erwartet");
        };
        let a = sk.point_xy(corners[i]).unwrap();
        let b = sk.point_xy(corners[(i + 1) % 4]).unwrap();
        self.screen((a + b) * 0.5)
    }

    fn glyph_items(&mut self, t: TextId) -> HashSet<Item> {
        self.sk()
            .text(t)
            .unwrap()
            .curves
            .iter()
            .map(|&c| Item::Curve(c))
            .collect()
    }

    fn selection(&mut self) -> HashSet<Item> {
        self.se().selection.iter().copied().collect()
    }

    fn text_count(&mut self) -> usize {
        self.sk().texts().count()
    }
}

/// Hüllquader der Punkte des Textes `t`.
fn text_box(sk: &Sketch, t: TextId) -> (DVec2, DVec2) {
    let d = sk.text(t).unwrap();
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for &p in &d.points {
        let q = sk.point_xy(p).unwrap();
        lo = lo.min(q);
        hi = hi.max(q);
    }
    (lo, hi)
}

/// Hüllquader von Konturen (alle Punkte).
fn contour_box(contours: &[Contour]) -> (DVec2, DVec2) {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    let mut add = |p: DVec2| {
        lo = lo.min(p);
        hi = hi.max(p);
    };
    for c in contours {
        add(c.start);
        for s in &c.segs {
            match *s {
                Seg::Line(p) => add(p),
                Seg::Quad(a, p) => {
                    add(a);
                    add(p);
                }
                Seg::Cubic(a, b, p) => {
                    add(a);
                    add(b);
                    add(p);
                }
            }
        }
    }
    (lo, hi)
}

// ---- Ändern ---------------------------------------------------------------------------------

#[test]
fn double_click_glyph_opens_with_values() {
    let mut rig = Rig::new();
    let t = rig.api_text("HI", FRAME_A, FRAME_B);
    // Gedreht und fett: Das Ändern zeigt die Werte des Textes.
    {
        let (_, sk) = rig.st().session_sketch_mut().unwrap();
        sk.set_text_angle(t, 20f64.to_radians()).unwrap();
        let d = sk.text(t).unwrap().clone();
        let mut style = d.style.clone();
        style.bold = true;
        let shape = Arc::new(FontLibrary::bundled_only().shape("HI", &style).unwrap());
        sk.edit_text_box(
            t,
            TextParams {
                content: d.content.clone(),
                style,
                height: d.height,
                flip_h: false,
                flip_v: true,
            },
            HAlign::Right,
            VAlign::Top,
            shape,
        )
        .unwrap();
        sk.solve();
    }
    rig.st().observe_sketch(false);
    rig.settle();
    let before = rig.data();
    let steps = rig.undo_len();
    let height = rig.sk().text(t).unwrap().height;
    let spot = rig.glyph_spot(t);
    let shot = rig.double_click(spot);
    assert_eq!(rig.se().tool, Tool::Text, "der Abschnitt ist offen");
    assert!(shot.has(&tr!("sketch-text-kind-path")), "Abschnitt Text");
    let tool = rig.tool().clone();
    assert_eq!(tool.editing, Some(t));
    assert!(tool.section_open());
    assert_eq!(tool.kind, TextToolKind::Box);
    assert_eq!(tool.params.content, "HI");
    assert_eq!(tool.params.style.font, "Montserrat");
    assert!(tool.params.style.bold);
    assert!(!tool.params.style.italic);
    assert!(tool.params.flip_v && !tool.params.flip_h);
    assert_eq!(tool.params.height, height);
    assert_eq!((tool.h, tool.v), (HAlign::Right, VAlign::Top));
    assert!(
        (tool.angle - 20f64.to_radians()).abs() < 1e-9,
        "Winkel aus dem Text: {}",
        tool.angle.to_degrees()
    );
    assert_eq!(tool.angle_text, "20");
    // Der Rahmen der Vorschau ist der des Textes.
    let corners = tool.frame_corners().expect("Rahmen");
    let TextPlacement::Box { corners: ids, .. } = rig.sk().text(t).unwrap().placement else {
        panic!("Rahmen erwartet");
    };
    for (i, id) in ids.iter().enumerate() {
        let at = rig.sk().point_xy(*id).unwrap();
        assert!(at.distance(corners[i]) < 1e-9, "Ecke {i}");
    }
    // Die Vorschau ändert nichts und liegt, wo der Text liegt.
    assert_eq!(rig.data(), before);
    assert_eq!(rig.undo_len(), steps);
    let s = rig.st();
    let preview = s
        .sketch_session
        .as_ref()
        .unwrap()
        .text_preview(s.session_sketch().unwrap())
        .expect("Vorschau");
    let (a, b) = (contour_box(&preview), text_box(rig.sk(), t));
    assert!(
        a.0.distance(b.0) < 1e-6 && a.1.distance(b.1) < 1e-6,
        "{a:?} {b:?}"
    );

    // Auch der Knopf „Text bearbeiten“ und die Aktion öffnen ihn, wenn der Text gewählt ist.
    rig.press(Key::Escape);
    assert_eq!(rig.se().tool, Tool::Select);
    rig.click(spot);
    rig.click_label(&tr!("action-sketch-text-edit"));
    assert_eq!(rig.tool().editing, Some(t), "Knopf");
    rig.press(Key::Escape);
    rig.click(spot);
    rig.action(Action::SketchTextEdit);
    assert_eq!(rig.tool().editing, Some(t), "Aktion");
    assert_eq!(rig.data(), before);
}

#[test]
fn edit_ok_is_one_undo_step_labelled_text_aendern() {
    let mut rig = Rig::new();
    let t = rig.api_text("HI", FRAME_A, FRAME_B);
    let old_box = text_box(rig.sk(), t);
    let before = rig.data();
    let steps = rig.undo_len();
    let spot = rig.glyph_spot(t);
    rig.double_click(spot);
    assert_eq!(rig.tool().editing, Some(t));
    // Neuer Inhalt, kursiv, links.
    rig.tool().params.content = "HIH".into();
    rig.tool().params.style.italic = true;
    rig.tool().h = HAlign::Left;
    rig.settle();
    assert_eq!(rig.data(), before, "die Vorschau schreibt nichts");
    let s = rig.st();
    let preview = s
        .sketch_session
        .as_ref()
        .unwrap()
        .text_preview(s.session_sketch().unwrap())
        .expect("Vorschau");
    let shown = contour_box(&preview);
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.se().tool, Tool::Select, "nach OK zurück zur Auswahl");
    assert_eq!(rig.text_count(), 1, "derselbe Text, kein zweiter");
    let d = rig.sk().text(t).expect("der Text lebt").clone();
    assert_eq!(d.content, "HIH");
    assert!(d.style.italic);
    assert!(matches!(
        d.placement,
        TextPlacement::Box {
            h: HAlign::Left,
            ..
        }
    ));
    let now = text_box(rig.sk(), t);
    assert!(
        shown.0.distance(now.0) < 1e-6 && shown.1.distance(now.1) < 1e-6,
        "Vorschau {shown:?}, geändert {now:?}"
    );
    assert_ne!(now, old_box);
    assert_eq!(rig.undo_len(), steps + 1, "ein Rückgängig-Schritt");
    assert_eq!(
        rig.st().sketch_undo_label(),
        Some(tr!("undo-sketch-text-edit"))
    );
    assert_eq!(tr!("undo-sketch-text-edit"), "Text ändern");
    // Ein OK ohne Änderung ist kein Schritt.
    let spot = rig.glyph_spot(t);
    rig.double_click(spot);
    assert_eq!(rig.tool().editing, Some(t));
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.undo_len(), steps + 1, "unverändert: kein Schritt");
    // Rückgängig holt den alten Text zurück.
    rig.press_with(Key::Z, ctrl());
    assert_eq!(rig.data(), before);
}

#[test]
fn edit_cancel_restores() {
    let mut rig = Rig::new();
    let t = rig.api_text("HI", FRAME_A, FRAME_B);
    let path = rig.api_line();
    let before = rig.data();
    let steps = rig.undo_len();
    let spot = rig.glyph_spot(t);
    for way in ["cancel", "esc", "right", "switch"] {
        rig.double_click(spot);
        assert_eq!(rig.tool().editing, Some(t), "{way}");
        rig.tool().params.content = "XYZ".into();
        rig.tool().params.style.bold = true;
        // Auch die Art: auf den Pfad.
        rig.click_label(&tr!("sketch-text-kind-path"));
        assert_eq!(rig.tool().kind, TextToolKind::Path);
        let mid = (rig.sketch_at(LINE_A) + rig.sketch_at(LINE_B)) * 0.5;
        let line_spot = rig.screen(mid);
        rig.click(line_spot);
        assert_eq!(rig.tool().path, Some(path), "{way}: Pfad gewählt");
        match way {
            "cancel" => {
                rig.click_label(&tr!("common-cancel"));
            }
            "esc" => {
                rig.click(AWAY);
                rig.press(Key::Escape);
            }
            "right" => {
                rig.click_mod(AWAY, PointerButton::Secondary, NONE);
            }
            _ => {
                rig.action(Action::SketchLine);
            }
        }
        assert_ne!(rig.se().tool, Tool::Text, "{way}");
        assert_eq!(rig.data(), before, "{way}: alles wie vorher");
        assert_eq!(rig.undo_len(), steps, "{way}: kein Schritt");
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Select);
        rig.settle();
    }
}

// ---- Auflösen -------------------------------------------------------------------------------

#[test]
fn explode_menu_and_button_one_undo_step() {
    let mut rig = Rig::new();
    let t1 = rig.api_text("HI", FRAME_A, FRAME_B);
    let t2 = rig.api_text("IH", FRAME2_A, FRAME2_B);
    let glyphs: Vec<CurveId> = [t1, t2]
        .iter()
        .flat_map(|&t| rig.sk().text(t).unwrap().curves.clone())
        .collect();
    let before = rig.data();
    let steps = rig.undo_len();
    // Beide Texte wählen (Umschalt nimmt den zweiten dazu), dann Menü „Bearbeiten“ → „Ändern“.
    let (s1, s2) = (rig.glyph_spot(t1), rig.glyph_spot(t2));
    rig.click(s1);
    rig.shift_click(s2);
    let (g1, g2) = (rig.glyph_items(t1), rig.glyph_items(t2));
    let all: HashSet<Item> = g1.union(&g2).copied().collect();
    assert_eq!(rig.selection(), all, "beide Texte gewählt");
    let shot = rig.settle();
    let shot = rig.click(shot.find_top(&tr!("menu-edit")));
    let shot = rig.click(shot.find(&tr!("menu-sketch-modify")));
    let label = Action::SketchTextExplode.label();
    assert_eq!(label, tr!("action-sketch-text-explode"));
    assert_eq!(label, "Text auflösen");
    rig.click(shot.find(&label));
    assert_eq!(rig.text_count(), 0, "beide aufgelöst");
    for c in &glyphs {
        assert!(rig.sk().curve(*c).is_some(), "Kurve {c:?} bleibt");
    }
    assert_eq!(rig.undo_len(), steps + 1, "ein Schritt für beide");
    assert_eq!(
        rig.st().sketch_undo_label(),
        Some(tr!("undo-sketch-text-explode"))
    );
    rig.press_with(Key::Z, ctrl());
    assert_eq!(rig.data(), before);
    assert_eq!(rig.text_count(), 2);

    // Der Knopf im Abschnitt „Bearbeiten“.
    let s1 = rig.glyph_spot(t1);
    rig.click(s1);
    rig.click_label(&tr!("action-sketch-text-explode"));
    assert_eq!(rig.text_count(), 1, "nur der gewählte");
    assert!(rig.sk().text(t2).is_some());
    assert_eq!(rig.undo_len(), steps + 1);
    rig.press_with(Key::Z, ctrl());
    assert_eq!(rig.data(), before);

    // Die Aktion.
    let s2 = rig.glyph_spot(t2);
    rig.click(s2);
    rig.action(Action::SketchTextExplode);
    assert_eq!(rig.text_count(), 1);
    assert!(rig.sk().text(t1).is_some());
    assert_eq!(rig.undo_len(), steps + 1);
}

#[test]
fn keymap_actions_text_edit_and_explode_rebindable() {
    let mut rig = Rig::new();
    let t = rig.api_text("IH", FRAME_A, FRAME_B);
    for (a, id, key) in [
        (Action::SketchTextEdit, "skizze.text_bearbeiten", Key::F9),
        (Action::SketchTextExplode, "skizze.text_aufloesen", Key::F8),
    ] {
        assert_eq!(a.id(), id);
        assert_eq!(Action::from_id(id), Some(a));
        assert_eq!(a.area(), Area::Sketch);
        assert!(
            Keymap::defaults().bindings(a).is_empty(),
            "{id}: ohne Vorgabetaste"
        );
        let mut km = rig.ws().keymap.clone();
        let chord = KeyChord {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert_eq!(km.set(a, 0, Binding::Key(chord)), SetResult::Ok, "{id}");
        rig.ws().keymap = km;
    }
    assert_eq!(Action::SketchTextEdit.label(), "Text bearbeiten");
    let spot = rig.glyph_spot(t);
    rig.click(spot);
    rig.press(Key::F9);
    assert_eq!(rig.tool().editing, Some(t), "F9 öffnet den Text");
    rig.press(Key::Escape);
    rig.click(spot);
    rig.press(Key::F8);
    assert_eq!(rig.text_count(), 0, "F8 löst ihn auf");
}

// ---- Auswahl, Löschen, Konstruktion ---------------------------------------------------------

#[test]
fn click_glyph_selects_whole_text() {
    let mut rig = Rig::new();
    let t = rig.api_text("IH", FRAME_A, FRAME_B);
    let glyphs = rig.glyph_items(t);
    assert!(glyphs.len() > 4);
    let spot = rig.glyph_spot(t);
    rig.click(spot);
    assert_eq!(rig.selection(), glyphs, "ein Klick wählt den ganzen Text");
    // Umschalt+Klick auf den gewählten Text wählt ihn ganz ab.
    rig.shift_click(spot);
    assert!(rig.selection().is_empty(), "ganz abgewählt");
    // Eine Rahmenlinie ist einzeln wählbar.
    let top = rig.frame_spot(t, 2);
    rig.click(top);
    let TextPlacement::Box { lines, .. } = rig.sk().text(t).unwrap().placement else {
        panic!("Rahmen erwartet");
    };
    assert_eq!(rig.selection(), HashSet::from([Item::Curve(lines[2])]));
    rig.press(Key::Escape);
    // Ein Fenster um den ersten Buchstaben wählt den ganzen Text.
    let shape = rig.sk().text(t).unwrap().shape().unwrap().clone();
    let first: usize = shape.lines[0].glyphs[0]
        .contours
        .iter()
        .map(|c| c.segs.len())
        .sum();
    let curves = rig.sk().text(t).unwrap().curves[..first].to_vec();
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for c in curves {
        for p in rig.sk().curve_polyline(c, 64).unwrap() {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let margin = 0.1 * (hi.y - lo.y);
    let a = rig.screen(v(lo.x - margin, hi.y + margin));
    let b = rig.screen(v(hi.x + margin, lo.y - margin));
    assert!(a.x < b.x, "Fenster von links nach rechts");
    rig.drag(a, b);
    assert_eq!(rig.selection(), glyphs, "das Fenster wählt den ganzen Text");
}

#[test]
fn construction_toggle_switches_whole_text() {
    let mut rig = Rig::new();
    let t = rig.api_text("IH", FRAME_A, FRAME_B);
    let steps = rig.undo_len();
    let spot = rig.glyph_spot(t);
    rig.click(spot);
    rig.action(Action::SketchConstruction);
    let curves = rig.sk().text(t).unwrap().curves.clone();
    for &c in &curves {
        assert!(
            rig.sk().curve(c).unwrap().construction,
            "{c:?} Konstruktion"
        );
    }
    assert_eq!(rig.undo_len(), steps + 1, "ein Schritt");
    rig.action(Action::SketchConstruction);
    for &c in &curves {
        assert!(!rig.sk().curve(c).unwrap().construction, "{c:?} zurück");
    }
    assert_eq!(rig.undo_len(), steps + 2);
}

#[test]
fn delete_key_on_text_deletes_whole() {
    let mut rig = Rig::new();
    let t = rig.api_text("IH", FRAME_A, FRAME_B);
    let steps = rig.undo_len();
    let before = rig.data();
    let TextPlacement::Box { lines, .. } = rig.sk().text(t).unwrap().placement else {
        panic!("Rahmen erwartet");
    };
    let glyphs = rig.sk().text(t).unwrap().curves.clone();
    let spot = rig.glyph_spot(t);
    rig.click(spot);
    rig.press(Key::Delete);
    assert_eq!(rig.text_count(), 0, "der Text ist fort");
    for c in glyphs.iter().chain(&lines) {
        assert!(rig.sk().curve(*c).is_none(), "{c:?} fort");
    }
    assert_eq!(rig.sk().curves().count(), 0);
    assert_eq!(rig.undo_len(), steps + 1, "ein Schritt");
    rig.press_with(Key::Z, ctrl());
    assert_eq!(rig.data(), before);

    // Bei offenem Abschnitt „Text“ löscht Entf die Auswahl nicht (wie beim Muster).
    rig.click(spot);
    rig.double_click(spot);
    assert_eq!(rig.tool().editing, Some(t));
    rig.click(AWAY);
    rig.press(Key::Delete);
    assert_eq!(
        rig.data(),
        before,
        "Entf löscht bei offenem Abschnitt nichts"
    );
    assert_eq!(rig.se().tool, Tool::Text);
    rig.press(Key::Escape);
    rig.click(spot);
    rig.action(Action::SketchText);
    rig.click(FRAME2_A);
    rig.click(FRAME2_B);
    rig.click(AWAY);
    rig.press(Key::Delete);
    assert_eq!(rig.data(), before, "auch beim neuen Text");
}

#[test]
fn delete_key_on_frame_line_shows_message() {
    let mut rig = Rig::new();
    let t = rig.api_text("IH", FRAME_A, FRAME_B);
    let before = rig.data();
    let steps = rig.undo_len();
    let top = rig.frame_spot(t, 2);
    rig.click(top);
    let shot = rig.press(Key::Delete);
    assert_eq!(rig.data(), before, "nichts gelöscht");
    assert_eq!(rig.undo_len(), steps);
    let part = tr!("sketch-text-part");
    assert_eq!(
        rig.se().message.as_ref().map(|m| m.text()),
        Some(part.clone())
    );
    assert!(shot.has(&part), "die Meldung steht im Skizzenfeld");

    // Ein Pfad mit Text lässt sich nicht löschen.
    let path = rig.api_line();
    let shape = Arc::new(
        FontLibrary::bundled_only()
            .shape("A", &style("Montserrat"))
            .unwrap(),
    );
    {
        let (_, sk) = rig.st().session_sketch_mut().unwrap();
        sk.add_text_path(
            path,
            TextParams {
                content: "A".into(),
                style: style("Montserrat"),
                height: 5.0,
                flip_h: false,
                flip_v: false,
            },
            true,
            false,
            HAlign::Left,
            shape,
        )
        .unwrap();
        sk.solve();
    }
    rig.st().observe_sketch(false);
    rig.settle();
    let before = rig.data();
    let steps = rig.undo_len();
    let near_end = pos2(LINE_B.x - 20.0, LINE_B.y);
    rig.click(near_end);
    assert_eq!(rig.selection(), HashSet::from([Item::Curve(path)]));
    let shot = rig.press(Key::Delete);
    assert_eq!(rig.data(), before);
    assert_eq!(rig.undo_len(), steps);
    let on_path = tr!("sketch-text-on-path");
    assert!(shot.has(&on_path), "Meldung zum Pfad");
}

// ---- Hinweise -------------------------------------------------------------------------------

#[test]
fn substitute_hint_line_shown() {
    let mut rig = Rig::new();
    rig.action(Action::SketchText);
    rig.click(FRAME_A);
    rig.click(FRAME_B);
    rig.type_text("Hallo");
    let hint = tr!("sketch-text-hint-substituted", font = MISSING.to_string());
    assert_eq!(
        hint,
        format!("Schrift „{MISSING}“ fehlt auf diesem Rechner, ersetzt durch Montserrat.")
    );
    assert!(!rig.settle().has(&hint), "vorhandene Schrift: kein Hinweis");
    rig.tool().params.style.font = MISSING.into();
    let shot = rig.settle();
    assert!(shot.has(&hint), "Hinweis auf die Ersatzschrift");
    assert!(!shot.has(&tr!("sketch-text-hint-synthetic")));
    assert!(!shot.has(&tr!("sketch-text-hint-missing-chars")));
}

#[test]
fn synthetic_and_missing_chars_hints_shown() {
    let mut rig = Rig::new();
    rig.action(Action::SketchText);
    rig.click(FRAME_A);
    rig.click(FRAME_B);
    rig.type_text("Ha");
    let (synthetic, missing) = (
        tr!("sketch-text-hint-synthetic"),
        tr!("sketch-text-hint-missing-chars"),
    );
    let shot = rig.settle();
    assert!(!shot.has(&synthetic) && !shot.has(&missing));
    // Montserrat liegt ohne Kursiv bei: nachgebildet.
    rig.tool().params.style.italic = true;
    let shot = rig.settle();
    assert!(shot.has(&synthetic), "nachgebildetes Kursiv");
    assert!(!shot.has(&missing));
    // Ein Zeichen, das weder Montserrat noch Roboto hat.
    rig.tool().params.content = "Ha漢".into();
    let shot = rig.settle();
    assert!(shot.has(&synthetic));
    assert!(shot.has(&missing), "Zeichen fehlen");
}

#[test]
fn history_warning_for_missing_font() {
    let mut rig = Rig::new();
    let sketch = rig.st().open_sketch_id().unwrap();
    rig.action(Action::SketchText);
    rig.click(FRAME_A);
    rig.click(FRAME_B);
    rig.type_text("Hallo");
    rig.tool().params.style.font = MISSING.into();
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.text_count(), 1);
    rig.st().close_sketch();
    rig.settle();
    let s = rig.st();
    let f = s.document.feature(sketch).unwrap();
    assert!(matches!(f.kind, FeatureKind::Sketch(_)));
    assert_eq!(
        f.warning(),
        Some(
            tr!(
                "feature-sketch-font-missing",
                font = MISSING.to_string(),
                count = 1
            )
            .as_str()
        ),
        "Warnung im Verlauf"
    );
}

#[test]
fn fonts_loading_shows_reading_hint() {
    let (tx, rx) = mpsc::channel::<FontLibrary>();
    let mut rig = Rig::with_fonts(Some(FontState::Loading(rx)));
    assert!(rig.st().fonts().is_none());
    let t = rig.api_text("IH", FRAME_A, FRAME_B);
    let before = rig.data();
    let steps = rig.undo_len();
    let spot = rig.glyph_spot(t);
    let reading = tr!("sketch-text-fonts-loading");
    // Ändern: Der Abschnitt wartet, OK tut nichts.
    let shot = rig.double_click(spot);
    assert_eq!(rig.tool().editing, Some(t));
    assert!(shot.has(&reading), "„Schriften werden gelesen …“");
    rig.tool().params.content = "HH".into();
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.se().tool, Tool::Text, "OK ist gesperrt");
    assert_eq!(rig.data(), before);
    rig.press(Key::Escape);
    // Auflösen wartet ebenso: Knopf gesperrt, Aktion mit Meldung.
    rig.click(spot);
    let label = tr!("action-sketch-text-explode");
    let shot = rig.settle();
    rig.click(shot.find(&label));
    assert_eq!(rig.text_count(), 1, "Knopf gesperrt");
    rig.action(Action::SketchTextExplode);
    assert_eq!(rig.text_count(), 1, "Aktion wartet");
    let shot = rig.settle();
    assert!(shot.has(&reading), "die Aktion sagt, warum");
    assert_eq!(rig.data(), before);
    assert_eq!(rig.undo_len(), steps);
    drop(tx);
}

// ---- Vorschau und Fang ----------------------------------------------------------------------

#[test]
fn preview_change_requests_next_frame() {
    // Die Ansicht zeichnet vor dem Skizzenfeld; ändert das Skizzenfeld die Vorschau, fordert es
    // ein weiteres Bild an, damit die neue Vorschau nicht stehen bleibt.
    let mut rig = Rig::new();
    rig.action(Action::SketchText);
    rig.click(FRAME_A);
    rig.click(FRAME_B);
    rig.type_text("Hallo");
    rig.click(AWAY);
    rig.settle();
    let placements = rig.tool().placements();
    rig.tool().params.flip_h = true;
    let shot = rig.frame(vec![]);
    assert_eq!(rig.tool().placements(), placements + 1, "neu gelegt");
    assert_eq!(shot.repaint, Duration::ZERO, "nächstes Bild angefordert");
}

/// Ein Zeichen: Quadrat (0, 0)–(0,6, 0,7) in Einheiten der Höhe, ohne Schrift.
fn square_shape() -> Arc<TextShape> {
    let square = Contour {
        start: v(0.0, 0.0),
        segs: vec![
            Seg::Line(v(0.6, 0.0)),
            Seg::Line(v(0.6, 0.7)),
            Seg::Line(v(0.0, 0.7)),
            Seg::Line(v(0.0, 0.0)),
        ],
    };
    Arc::new(TextShape {
        lines: vec![ShapedLine {
            glyphs: vec![Glyph {
                origin: v(0.0, 0.0),
                advance: 0.8,
                contours: vec![square],
            }],
            width: 0.8,
        }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    })
}

#[test]
fn snap_skips_glyph_for_frame_line_next_to_it() {
    // Rahmen (0, 0)–(40, 10), das Quadrat liegt bei x = 18 … 21, y = 1 … 4,5. Der Buchstabe ist
    // dem Zeiger näher als die Grundlinie des Rahmens; er fängt nicht, die Grundlinie schon.
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let t = {
        let sk = s.session_sketch_mut().unwrap().1;
        let t = sk
            .add_text_box(
                v(0.0, 0.0),
                v(40.0, 10.0),
                TextParams {
                    content: "a".into(),
                    style: style("Test"),
                    height: 5.0,
                    flip_h: false,
                    flip_v: false,
                },
                HAlign::Center,
                VAlign::Bottom,
                square_shape(),
            )
            .unwrap();
        sk.solve();
        t
    };
    let (se, sk) = s.session_sketch_mut().unwrap();
    let TextPlacement::Box { lines, .. } = sk.text(t).unwrap().placement else {
        panic!("Rahmen erwartet");
    };
    se.set_tool(sk, Tool::Line);
    let at = v(17.6, 0.7);
    se.update_cursor(sk, Some(at), 1.0);
    let cur = se.draw_cursor.as_ref().expect("Zeiger");
    assert_eq!(cur.snap, Snap::Curve(lines[0]), "die Grundlinie fängt");
    assert!(cur.at.y.abs() < 1e-9, "auf der Grundlinie: {:?}", cur.at);
}

// ---- Korrekturrunde 1 -----------------------------------------------------------------------

/// Text „A“ auf der Linie `path` (über die Skizze, ein Schritt).
fn api_path_text(rig: &mut Rig, path: CurveId) -> TextId {
    let shape = Arc::new(
        FontLibrary::bundled_only()
            .shape("A", &style("Montserrat"))
            .unwrap(),
    );
    let t = {
        let (_, sk) = rig.st().session_sketch_mut().unwrap();
        let t = sk
            .add_text_path(
                path,
                TextParams {
                    content: "A".into(),
                    style: style("Montserrat"),
                    height: 20.0,
                    flip_h: false,
                    flip_v: false,
                },
                true,
                false,
                HAlign::Center,
                shape,
            )
            .unwrap();
        sk.solve();
        t
    };
    rig.st().observe_sketch(false);
    rig.settle();
    t
}

/// Ist bei `at` ein Punkt gezeichnet (ein Kreis höchstens 1,5 px daneben)?
fn painted(shot: &Shot, at: Pos2) -> bool {
    shot.circles.iter().any(|(c, _)| c.distance(at) <= 1.5)
}

#[test]
fn glyph_points_are_not_painted() {
    let mut rig = Rig::new();
    let t = rig.api_text("IH", FRAME_A, FRAME_B);
    let to_screen = rig.mapping();
    let d = rig.sk().text(t).unwrap().clone();
    let TextPlacement::Box { corners, .. } = d.placement else {
        panic!("Rahmen erwartet");
    };
    let glyph_spots: Vec<Pos2> = d
        .points
        .iter()
        .map(|&p| to_screen(rig.sk().point_xy(p).unwrap()))
        .collect();
    let corner_spots: Vec<Pos2> = corners
        .iter()
        .map(|&p| to_screen(rig.sk().point_xy(p).unwrap()))
        .collect();
    let shot = rig.frame(vec![Event::PointerMoved(AWAY)]);
    for (i, s) in corner_spots.iter().enumerate() {
        assert!(painted(&shot, *s), "Ecke {i} ist ein Punkt wie andere");
    }
    for s in &glyph_spots {
        assert!(!painted(&shot, *s), "kein Punkt am Buchstaben bei {s:?}");
    }
    // Über einem Punkt des Buchstabens: nichts hervorgehoben, ein Klick wählt den Text.
    let spot = glyph_spots[0];
    let shot = rig.frame(vec![Event::PointerMoved(spot)]);
    assert!(!painted(&shot, spot), "kein hervorgehobener Punkt");
    rig.click(spot);
    let glyphs = rig.glyph_items(t);
    assert_eq!(rig.selection(), glyphs, "der ganze Text");
    let shot = rig.settle();
    for s in &glyph_spots {
        assert!(!painted(&shot, *s), "auch gewählt kein Punkt bei {s:?}");
    }
}

#[test]
fn refusal_message_stays_on_screen_with_hints() {
    // 1920 × 1080, Abschnitt offen mit allen drei Hinweisen (Ersatzschrift, nachgebildet,
    // Zeichen fehlen) und einem OK, das abgelehnt wird: Die Meldung steht ganz im Bild, direkt
    // unter OK und Abbrechen.
    let mut rig = Rig::new();
    rig.action(Action::SketchText);
    rig.click(FRAME_A);
    rig.click(FRAME_B);
    rig.tool().params.style.font = MISSING.into();
    rig.tool().params.style.italic = true;
    rig.tool().params.content = "\u{1F600}".into();
    let shot = rig.settle();
    for hint in [
        tr!("sketch-text-hint-substituted", font = MISSING.to_string()),
        tr!("sketch-text-hint-synthetic"),
        tr!("sketch-text-hint-missing-chars"),
    ] {
        assert!(shot.has(&hint), "Hinweis „{hint}“");
    }
    let shot = rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.se().tool, Tool::Text, "abgelehnt, der Abschnitt bleibt");
    let empty = tr!("sketch-text-empty");
    let (_, rect) = shot
        .texts
        .iter()
        .find(|(t, _)| *t == empty)
        .unwrap_or_else(|| panic!("„{empty}“ fehlt im Bild"));
    let screen = Rect::from_min_size(Pos2::ZERO, SIZE);
    assert!(screen.contains_rect(*rect), "ganz im Bild: {rect:?}");
    let ok = shot.find(&tr!("common-ok"));
    assert!(
        rect.top() > ok.y && rect.top() - ok.y < 60.0,
        "direkt unter OK: {rect:?}, OK bei {ok:?}"
    );
    assert_eq!(
        shot.texts.iter().filter(|(t, _)| *t == empty).count(),
        1,
        "nur einmal"
    );
    // Die Hinweise haben ihren vollen Wortlaut.
    assert_eq!(
        tr!("sketch-text-hint-missing-chars"),
        "Zeichen fehlen: Weder die Schrift noch Roboto hat sie, ihr Platz bleibt leer."
    );
}

#[test]
fn edit_box_to_path_and_path_to_box() {
    let mut rig = Rig::new();
    let t = rig.api_text("HI", FRAME_A, FRAME_B);
    let path = rig.api_line();
    let steps = rig.undo_len();
    let TextPlacement::Box { lines, .. } = rig.sk().text(t).unwrap().placement else {
        panic!("Rahmen erwartet");
    };
    // Rahmen → Pfad: Der Rahmen ist fort, ein Schritt.
    let spot = rig.glyph_spot(t);
    rig.double_click(spot);
    rig.click_label(&tr!("sketch-text-kind-path"));
    let mid = (rig.sketch_at(LINE_A) + rig.sketch_at(LINE_B)) * 0.5;
    let line_spot = rig.screen(mid);
    rig.click(line_spot);
    assert_eq!(rig.tool().path, Some(path));
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.se().tool, Tool::Select);
    let d = rig.sk().text(t).expect("derselbe Text").clone();
    assert!(
        matches!(d.placement, TextPlacement::Path { path: p, .. } if p == path),
        "auf dem Pfad: {:?}",
        d.placement
    );
    for c in lines {
        assert!(rig.sk().curve(c).is_none(), "Rahmenlinie {c:?} fort");
    }
    assert_eq!(rig.undo_len(), steps + 1);
    assert_eq!(
        rig.st().sketch_undo_label(),
        Some(tr!("undo-sketch-text-edit"))
    );

    // Pfad → Rahmen: ein neuer Rahmen mit 0°, ein Schritt.
    let spot = rig.glyph_spot(t);
    rig.double_click(spot);
    assert_eq!(rig.tool().editing, Some(t));
    assert_eq!(rig.tool().kind, TextToolKind::Path);
    // „Text“ steht mehrfach im Abschnitt (Überschrift, Art, Feld): die Art direkt.
    rig.se().set_text_kind(TextToolKind::Box);
    rig.settle();
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.se().tool, Tool::Select);
    let d = rig.sk().text(t).unwrap().clone();
    assert!(
        matches!(d.placement, TextPlacement::Box { .. }),
        "im Rahmen"
    );
    let angle = rig.sk().text_angle(t).expect("Winkel");
    assert!(angle.abs() < 1e-9, "0°: {}", angle.to_degrees());
    assert!(rig.sk().curve(path).is_some(), "der Pfad bleibt");
    assert_eq!(rig.undo_len(), steps + 2);
}

#[test]
fn cancel_restores_selection_from_before_the_edit() {
    let mut rig = Rig::new();
    let t = rig.api_text("HI", FRAME_A, FRAME_B);
    let path = rig.api_line();
    let t2 = api_path_text(&mut rig, path);
    let spot = rig.glyph_spot(t);
    let near_end = pos2(LINE_B.x - 20.0, LINE_B.y);
    // Text und Linie gewählt, dann „Text bearbeiten“.
    rig.click(spot);
    rig.shift_click(near_end);
    let mut before = rig.glyph_items(t);
    before.insert(Item::Curve(path));
    assert_eq!(rig.selection(), before);
    rig.click_label(&tr!("action-sketch-text-edit"));
    assert_eq!(rig.tool().editing, Some(t));
    // Während des Änderns ändert sich die Auswahl (etwa durch einen anderen Weg).
    rig.se().selection = rig.glyph_items(t2).into_iter().collect();
    rig.click_label(&tr!("common-cancel"));
    assert_eq!(rig.se().tool, Tool::Select);
    assert_eq!(rig.selection(), before, "die Auswahl von vorher");
}

// ---- Korrektur nach dem Handtest ------------------------------------------------------------

impl Shot {
    /// Das Rechteck des letzten Texts `text`.
    fn rect_of(&self, text: &str) -> Rect {
        self.texts
            .iter()
            .rfind(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("„{text}“ fehlt"))
            .1
    }
}

/// Handtest B1: „Text bearbeiten“ über den Knopf, im Winkelfeld Strg+A und „0“ getippt, dann OK
/// geklickt, während das Feld noch die Tastatur hat: Ein Klick übernimmt den Wert und schließt
/// den Abschnitt. Nachgestellt wie im Handtest (erst Doppelklick mit Höhe 10 und Winkel 30, dann
/// über den Knopf), mit Tastenereignissen wie unter Windows und dem Klick in getrennten Bildern
/// mit Pausen wie `tools/gui-drive.ps1`.
#[test]
fn ok_click_with_focused_angle_field_commits_in_one_click() {
    let mut rig = Rig::new();
    let t = rig.api_text("Hallo", FRAME_A, FRAME_B);
    let field_of = |rig: &mut Rig, key: &str| {
        let label = rig.settle().rect_of(&planora_i18n::t(key));
        pos2(label.right() + 30.0, label.center().y)
    };
    let up = |k: Key, m: Modifiers| Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: false,
        repeat: false,
        modifiers: m,
    };
    // Strg+A und Tippen wie unter Windows: Taste gedrückt, Text, Taste losgelassen.
    let retype = |rig: &mut Rig, text: &str| {
        rig.frame(vec![key_with(Key::A, ctrl())]);
        rig.frame(vec![up(Key::A, ctrl())]);
        for ch in text.chars() {
            let k = Key::from_name(&ch.to_string()).expect("Zifferntaste");
            rig.frame(vec![key_with(k, NONE), Event::Text(ch.to_string())]);
            rig.frame(vec![up(k, NONE)]);
        }
        rig.settle();
    };
    // Klick wie `gui-drive.ps1`: Zeiger hin, 150 ms, drücken, 60 ms, loslassen.
    let click_ok = |rig: &mut Rig| {
        let ok = rig.settle().find(&tr!("common-ok"));
        rig.time += 1.0;
        rig.frame(vec![Event::PointerMoved(ok)]);
        rig.time += 0.15;
        rig.frame(vec![pointer(ok, PointerButton::Primary, true, NONE)]);
        rig.time += 0.06;
        rig.frame(vec![pointer(ok, PointerButton::Primary, false, NONE)]);
        rig.settle();
    };
    // Erster Durchgang: Doppelklick, Höhe 10, Winkel 30, OK.
    let spot = rig.glyph_spot(t);
    rig.double_click(spot);
    assert_eq!(rig.tool().editing, Some(t));
    let h = field_of(&mut rig, "sketch-text-height-label");
    rig.click(h);
    retype(&mut rig, "10");
    let a = field_of(&mut rig, "sketch-text-angle-label");
    rig.click(a);
    retype(&mut rig, "30");
    click_ok(&mut rig);
    assert_eq!(rig.se().tool, Tool::Select, "erster Durchgang: ein Klick");
    let steps = rig.undo_len();
    // Zweiter Durchgang: wählen, Knopf „Text bearbeiten“, Winkel 0, OK.
    let spot = rig.glyph_spot(t);
    rig.click(spot);
    rig.click_label(&tr!("action-sketch-text-edit"));
    assert_eq!(rig.tool().editing, Some(t));
    assert_eq!(rig.tool().angle_text, "30");
    let a = field_of(&mut rig, "sketch-text-angle-label");
    rig.click(a);
    retype(&mut rig, "0");
    assert_eq!(rig.tool().angle_text, "0", "getippt");
    click_ok(&mut rig);
    assert_eq!(rig.se().tool, Tool::Select, "ein Klick auf OK schließt");
    let angle = rig.sk().text_angle(t).expect("Winkel");
    assert!(angle.abs() < 1e-9, "0° übernommen: {}", angle.to_degrees());
    assert_eq!(rig.undo_len(), steps + 1, "ein Schritt");
}

impl Shot {
    /// Liegt eine gezeichnete Ecke (Linie, Linienzug, Fläche) höchstens 0,75 px neben `at`?
    fn near_vertex(&self, at: Pos2) -> bool {
        self.vertices.iter().any(|v| v.distance(at) <= 0.75)
    }
}

/// Handtest B2: Beim Ändern wird vom alten Text nichts gezeichnet, nur die Vorschau: keine
/// Kurven, keine gestrichelten Hilfslinien der runden Buchstaben, keine Profilfüllung der
/// Buchstaben, kein Rahmen (auch nicht hervorgehoben) und keine Ecken.
#[test]
fn edited_text_is_not_painted_while_editing() {
    let mut rig = Rig::new();
    let t = rig.api_text("Hallo", FRAME_A, FRAME_B);
    let to_screen = rig.mapping();
    let d = rig.sk().text(t).unwrap().clone();
    let TextPlacement::Box { corners, .. } = d.placement else {
        panic!("Rahmen erwartet");
    };
    let glyph_spots: Vec<Pos2> = d
        .points
        .iter()
        .map(|&p| to_screen(rig.sk().point_xy(p).unwrap()))
        .collect();
    // Ecke 0 ist der Drehpunkt: Dort beginnt auch der Rahmen der Vorschau.
    let corner_spots: Vec<Pos2> = corners[1..]
        .iter()
        .map(|&p| to_screen(rig.sk().point_xy(p).unwrap()))
        .collect();
    assert!(rig.st().show_sketch_profiles, "Profile werden gezeigt");
    let shot = rig.frame(vec![Event::PointerMoved(AWAY)]);
    let seen = glyph_spots.iter().filter(|s| shot.near_vertex(**s)).count();
    assert!(
        seen * 2 > glyph_spots.len(),
        "vorher gezeichnet: {seen} von {}",
        glyph_spots.len()
    );
    for s in &corner_spots {
        assert!(shot.near_vertex(*s), "Rahmen vorher bei {s:?}");
    }
    // Doppelklick: Der erste Klick wählt den Text, er bleibt beim Ändern gewählt.
    let spot = rig.glyph_spot(t);
    rig.double_click(spot);
    assert_eq!(rig.tool().editing, Some(t));
    assert!(!rig.selection().is_empty(), "der Text bleibt gewählt");
    // Um 180° gedreht: Die Vorschau liegt ganz neben dem alten Text.
    let scope = rig.st().document.param_scope();
    rig.tool().set_angle_text("180", &scope);
    rig.settle();
    assert!(rig.se().placed_text_preview().is_some(), "Vorschau da");
    let shot = rig.frame(vec![Event::PointerMoved(AWAY)]);
    let left: Vec<Pos2> = glyph_spots
        .iter()
        .copied()
        .filter(|s| shot.near_vertex(*s))
        .collect();
    assert!(left.is_empty(), "vom alten Text gezeichnet: {left:?}");
    for s in &corner_spots {
        assert!(!shot.near_vertex(*s), "alter Rahmen bei {s:?}");
        assert!(!painted(&shot, *s), "alte Ecke bei {s:?}");
    }
    // Abbrechen: Der Text ist wieder ganz zu sehen.
    rig.click_label(&tr!("common-cancel"));
    let shot = rig.frame(vec![Event::PointerMoved(AWAY)]);
    let seen = glyph_spots.iter().filter(|s| shot.near_vertex(**s)).count();
    assert!(seen * 2 > glyph_spots.len(), "nachher wieder gezeichnet");
    for s in &corner_spots {
        assert!(shot.near_vertex(*s), "Rahmen nachher bei {s:?}");
    }
}

/// Handtest B4 (Dennis: „Nur färben“): Nach „Text auflösen“ malt die Ansicht kein „fix“-Schild je
/// Punkt mehr (bei „Hallo“ waren es Dutzende); die Kurven tragen die Farbe fester Geometrie. Die
/// Fixierungen bleiben in der Liste „Abhängigkeiten der Skizze“: Eine dort gewählte zeigt ihr
/// Schild, und sie lässt sich einzeln löschen.
#[test]
fn exploded_text_is_coloured_without_a_cloud_of_fix_labels() {
    use planora_sketch::constraint::ConstraintKind;
    let mut rig = Rig::new();
    let t = rig.api_text("Hallo", FRAME_A, FRAME_B);
    let lines: Vec<CurveId> = {
        let sk = rig.sk();
        sk.text(t)
            .unwrap()
            .curves
            .iter()
            .copied()
            .filter(|&c| matches!(sk.curve(c).unwrap().kind, CurveKind::Line { .. }))
            .collect()
    };
    let spot = rig.glyph_spot(t);
    rig.click(spot);
    rig.click_label(&tr!("action-sketch-text-explode"));
    assert_eq!(rig.text_count(), 0, "aufgelöst");
    let fixes: Vec<_> = rig
        .sk()
        .constraints()
        .filter(|(_, c)| matches!(c.kind, ConstraintKind::Fixed(..)))
        .map(|(id, _)| id)
        .collect();
    assert!(fixes.len() > 20, "je Punkt eine Fixierung: {}", fixes.len());
    let fix = |shot: &Shot| shot.texts.iter().filter(|(t, _)| t == "fix").count();
    // Gleich nach dem Auflösen (die Kurven bleiben gewählt) und abgewählt: kein Schild.
    let shot = rig.frame(vec![Event::PointerMoved(AWAY)]);
    assert_eq!(fix(&shot), 0, "keine Schilder, Kurven gewählt");
    rig.click(AWAY);
    assert!(rig.selection().is_empty(), "abgewählt");
    let shot = rig.frame(vec![Event::PointerMoved(AWAY)]);
    assert_eq!(fix(&shot), 0, "keine Schilder");
    // Die Kurven in der Farbe fester (bestimmter) Geometrie.
    let dark = rig.st().dark;
    let fixed_ink = theme::sketch_colors(dark).determined;
    let to_screen = rig.mapping();
    let shot = rig.frame(vec![Event::PointerMoved(AWAY)]);
    for &c in &lines {
        let CurveKind::Line { a, b } = rig.sk().curve(c).unwrap().kind else {
            unreachable!()
        };
        let (sa, sb) = (
            to_screen(rig.sk().point_xy(a).unwrap()),
            to_screen(rig.sk().point_xy(b).unwrap()),
        );
        let ink = shot.strokes.iter().find_map(|(pts, color)| {
            let near = |q: Pos2| pts.iter().any(|p| p.distance(q) <= 0.75);
            (near(sa) && near(sb)).then_some(*color)
        });
        assert_eq!(
            ink,
            Some(fixed_ink),
            "Kurve {c:?} in der Farbe fester Geometrie"
        );
    }
    // Eine in der Liste gewählte Fixierung zeigt ihr Schild; Entf löscht sie einzeln.
    rig.se().selection = vec![Item::Constraint(fixes[0])];
    let shot = rig.frame(vec![Event::PointerMoved(AWAY)]);
    assert_eq!(fix(&shot), 1, "nur das Schild der gewählten Fixierung");
    rig.press(Key::Delete);
    let left = rig
        .sk()
        .constraints()
        .filter(|(_, c)| matches!(c.kind, ConstraintKind::Fixed(..)))
        .count();
    assert_eq!(left, fixes.len() - 1, "einzeln gelöscht");
    for &c in &lines {
        assert!(rig.sk().curve(c).is_some(), "die Kurven bleiben");
    }
}
