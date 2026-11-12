//! Skizzen 8, Task 11 (Spec G4, A1): Die Schriftliste entsteht im Hintergrund, ohne den Start
//! aufzuhalten. Bis sie da ist, bleiben die gespeicherten Kurven eines geladenen Textes stehen,
//! ohne Hinweis und ohne Warnung. Sobald sie da ist, werden alle offenen Dokumente einmal neu
//! gesetzt: ohne Rückgängig-Schritt und ohne „●“, solange die Form gleich bleibt. Weicht die Form
//! ab (fehlende Schrift), gilt das Dokument als geändert, die Skizze warnt, und es gibt trotzdem
//! keinen Rückgängig-Schritt. Dasselbe gilt nach jedem Rückgängig und Wiederholen.
//!
//! Die ganze App läuft ohne Fenster bei 1920×1080. Gesetzt wird nur mit den mitgelieferten
//! Schriften ([`FontLibrary::bundled_only`]) oder einer Nachbildung, nie mit Systemschriften.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Pos2, Rect, Vec2};
use planora_app::app::PlanoraApp;
use planora_app::browser;
use planora_app::crash;
use planora_app::fonts_library;
use planora_app::state::{AppState, FontState, StandardPlane};
use planora_app::workspace::{DocTab, Workspace, nothing_to_undo};
use planora_app::{fonts, theme};
use planora_feature::{FeatureId, FeatureKind, SketchPlane};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::docstore::LockMode;
use planora_project::library::{Library, NodePath};
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextData, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_text::FontLibrary;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
/// Eine Schrift, die es nirgends gibt.
const MISSING: &str = "Gibtsnicht Grotesk";
/// Zeichen „ungespeichert“ hinter dem Namen im Tab (`tabs.rs`).
const DIRTY_MARK: char = '●';

// ---- Hilfen ---------------------------------------------------------------------------------

/// Setzt jedes Zeichen als Quadrat 3 × 3 im Vorschub 4: eine Schrift, die nach dem Laden fehlt.
struct SquareShaper;

impl TextShaper for SquareShaper {
    fn shape(&self, content: &str, _style: &TextStyle) -> Result<TextShape, TextShapeError> {
        let v = DVec2::new;
        let glyphs: Vec<Glyph> = content
            .chars()
            .enumerate()
            .map(|(i, _)| Glyph {
                origin: v(4.0 * i as f64, 0.0),
                advance: 4.0,
                contours: vec![Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(3.0, 0.0)),
                        Seg::Line(v(3.0, 3.0)),
                        Seg::Line(v(0.0, 3.0)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                }],
            })
            .collect();
        let width = 4.0 * glyphs.len() as f64;
        Ok(TextShape {
            lines: vec![ShapedLine { glyphs, width }],
            ascender: 4.0,
            descender: -1.0,
            line_height: 5.0,
            substituted: None,
            synthetic: false,
            missing_chars: false,
        })
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

fn workspace() -> Workspace {
    let root = common::temp_root("planora-sketch-text-load");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    w
}

/// Ein gespeichertes Dokument „Teil“ mit einer Skizze und dem Text „Ho“ in `font`, gesetzt mit
/// `shaper`. Der Tab ist danach geschlossen; das Dokument wird mit [`open`] geöffnet.
struct Saved {
    w: Workspace,
    path: NodePath,
    sketch: FeatureId,
    text: TextId,
    /// Kurven des Textes beim Speichern (Ids und Art).
    curves: Vec<String>,
}

fn saved_doc(font: &str, shaper: &dyn TextShaper) -> Saved {
    saved_doc_with("Ho", font, shaper)
}

/// Wie [`saved_doc`] mit dem Inhalt `content`.
fn saved_doc_with(content: &str, font: &str, shaper: &dyn TextShaper) -> Saved {
    let mut w = workspace();
    let s = &mut w.active_tab_mut().state;
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    let shape = shaper.shape(content, &style(font)).unwrap();
    let mut text = None;
    s.document
        .update(sketch, |k| {
            let FeatureKind::Sketch(sf) = k else {
                unreachable!()
            };
            text = Some(
                sf.sketch
                    .add_text_box(
                        DVec2::new(0.0, 0.0),
                        DVec2::new(40.0, 10.0),
                        TextParams {
                            content: content.into(),
                            style: style(font),
                            height: 5.0,
                            flip_h: false,
                            flip_v: false,
                        },
                        HAlign::Left,
                        VAlign::Bottom,
                        Arc::new(shape),
                    )
                    .unwrap(),
            );
        })
        .unwrap();
    s.refresh();
    let text = text.unwrap();
    w.observe_history(false);
    let p = w
        .library
        .as_ref()
        .unwrap()
        .create_project("P")
        .unwrap_or_else(|_| vec!["P".to_string()]);
    let path = w.save_as(&p, "Teil", "").unwrap();
    let curves = curve_list(&w.active_tab().state, sketch, text);
    assert!(!curves.is_empty());
    w.close_tab(0);
    Saved {
        w,
        path,
        sketch,
        text,
        curves,
    }
}

/// Öffnet das gespeicherte Dokument (danach der aktive Tab).
fn open(w: &mut Workspace, path: &NodePath) {
    let r = w.open_document(path, LockMode::Acquire).unwrap();
    assert!(!r.autosave_offered);
    w.activate(r.tab);
}

fn def(s: &AppState, sketch: FeatureId) -> &Sketch {
    match &s.document.feature(sketch).unwrap().kind {
        FeatureKind::Sketch(sf) => &sf.sketch,
        k => panic!("keine Skizze: {k:?}"),
    }
}

fn text_data(s: &AppState, sketch: FeatureId, t: TextId) -> &TextData {
    def(s, sketch).text(t).unwrap()
}

/// Die Kurven des Textes in der Definition als Text (Id und Art), zum Vergleichen.
fn curve_list(s: &AppState, sketch: FeatureId, t: TextId) -> Vec<String> {
    let sk = def(s, sketch);
    sk.text(t)
        .unwrap()
        .curves
        .iter()
        .map(|c| format!("{c:?} {:?}", sk.curve(*c).unwrap().kind))
        .collect()
}

fn warning(s: &AppState, sketch: FeatureId) -> Option<String> {
    s.document
        .feature(sketch)
        .unwrap()
        .warning()
        .map(str::to_string)
}

fn font_warning(font: &str, count: usize) -> String {
    planora_i18n::tr!(
        "feature-sketch-font-missing",
        font = font.to_string(),
        count = count
    )
}

/// Die ganze App ohne Fenster; die Schriftliste kommt über den Kanal `fonts`.
struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    fonts: Option<Sender<FontLibrary>>,
    /// Wann das letzte Bild das nächste haben will (`Duration::ZERO`: sofort).
    repaint: Duration,
}

impl Rig {
    /// App mit `w`; die Schriftliste ist noch nicht da (Laden-Zustand mit einem Kanal).
    fn loading(w: Workspace) -> Rig {
        let (tx, rx) = mpsc::channel();
        let mut rig = Rig::with_state(w, FontState::Loading(rx));
        rig.fonts = Some(tx);
        rig
    }

    fn with_state(w: Workspace, fonts_state: FontState) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut app = PlanoraApp::for_tests(w);
        app.set_font_state(fonts_state);
        Rig {
            ctx,
            app,
            time: 0.0,
            fonts: None,
            repaint: Duration::MAX,
        }
    }

    /// Ein Bild; liefert alle gezeichneten Texte.
    fn frame(&mut self) -> Vec<String> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H))),
            time: Some(self.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut texts = Vec::new();
        collect(&out.shapes, &mut texts);
        self.repaint = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        out.drop_without_applying_deltas();
        texts
    }

    fn settle(&mut self) -> Vec<String> {
        self.frame();
        self.frame();
        self.frame()
    }

    /// Die Schriftliste kommt an (mitgelieferte Schriften).
    fn fonts_ready(&mut self) {
        self.fonts
            .take()
            .expect("Kanal offen")
            .send(FontLibrary::bundled_only())
            .unwrap();
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn tab(&mut self) -> &mut DocTab {
        self.ws().active_tab_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.tab().state
    }
}

fn collect(shapes: &[ClippedShape], out: &mut Vec<String>) {
    fn walk(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
            Shape::Text(t) => out.push(t.galley.text().to_string()),
            _ => {}
        }
    }
    for c in shapes {
        walk(&c.shape, out);
    }
}

/// Trägt der Tab „Teil“ im Bild das „●“? („Teil“ allein steht auch als Wurzel im Baum.)
fn tab_marked(texts: &[String]) -> bool {
    assert!(
        texts.iter().any(|t| t == "Teil"),
        "„Teil“ fehlt im Bild: {texts:?}"
    );
    texts.iter().any(|t| *t == format!("Teil {DIRTY_MARK}"))
}

// ---- Vor der Schriftliste --------------------------------------------------------------------

#[test]
fn open_file_with_text_before_fonts_ready_keeps_curves() {
    let mut saved = saved_doc("Montserrat", &FontLibrary::bundled_only());
    open(&mut saved.w, &saved.path);
    let mut rig = Rig::loading(saved.w);
    for _ in 0..5 {
        let texts = rig.frame();
        assert!(!tab_marked(&texts), "kein „●“ vor der Schriftliste");
    }
    let (sk, t) = (saved.sketch, saved.text);
    let s = rig.st();
    assert!(s.fonts().is_none(), "die Liste ist noch nicht da");
    assert_eq!(
        curve_list(s, sk, t),
        saved.curves,
        "die gespeicherten Kurven bleiben stehen"
    );
    assert!(
        text_data(s, sk, t).shape().is_none(),
        "noch nicht gesetzt (Spec G4)"
    );
    assert_eq!(warning(s, sk), None, "noch keine Warnung");
    assert_eq!(rig.tab().history().undo_len(), 0);
    assert!(!rig.tab().is_dirty());
}

#[test]
fn startup_does_not_wait_for_font_scan() {
    // Ein Kanal, der nie liefert: Das erste Bild kommt trotzdem, und die App läuft weiter.
    let (tx, rx) = mpsc::channel::<FontLibrary>();
    let mut rig = Rig::with_state(workspace(), FontState::Loading(rx));
    let started = Instant::now();
    let texts = rig.frame();
    assert!(!texts.is_empty(), "das erste Bild ist gezeichnet");
    rig.settle();
    assert!(rig.st().fonts().is_none());
    assert!(started.elapsed() < Duration::from_secs(30));
    drop(tx);

    // Auch der echte Start im Hintergrund kehrt sofort zurück, während das Durchsuchen noch
    // hängt; erst danach kommt die Liste in jedem Tab an.
    let (go, wait) = mpsc::channel::<()>();
    let started = Instant::now();
    let state = fonts_library::spawn_scan(None, move || {
        wait.recv().ok();
        FontLibrary::bundled_only()
    });
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "der Start wartet nicht auf das Durchsuchen"
    );
    let mut rig = Rig::with_state(workspace(), state);
    rig.settle();
    assert!(rig.st().fonts().is_none(), "das Durchsuchen hängt noch");
    go.send(()).unwrap();
    wait_for_fonts(&mut rig);
    assert!(rig.st().fonts().unwrap().has_family("Montserrat"));
    // Ein neuer Tab bekommt die Liste ebenso.
    rig.ws().new_tab();
    rig.frame();
    assert!(rig.st().fonts().is_some(), "neuer Tab");
}

/// Bilder, bis die Schriftliste im aktiven Tab ankommt (höchstens 20 s).
fn wait_for_fonts(rig: &mut Rig) {
    let until = Instant::now() + Duration::from_secs(20);
    while rig.st().fonts().is_none() {
        assert!(Instant::now() < until, "die Schriftliste kommt nie an");
        std::thread::sleep(Duration::from_millis(5));
        rig.frame();
    }
}

static HOOK_LOCK: Mutex<()> = Mutex::new(());

fn report_dir() -> PathBuf {
    let dir = common::temp_root("planora-font-scan-crash");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn font_reader_panic_writes_no_crash_report() {
    // Eine Panik im Schriftleser fängt planora-text selbst ab; der Absturz-Hook der App darf
    // dafür keinen Bericht schreiben. Selbst eine nicht abgefangene Panik beim Durchsuchen ist
    // kein Absturz: Es gelten dann die mitgelieferten Schriften.
    let _g = HOOK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = report_dir();
    let diag: &'static Mutex<planora_app::diag::Diag> =
        Box::leak(Box::new(Mutex::new(planora_app::diag::Diag::in_memory())));
    let old = std::panic::take_hook();
    std::panic::set_hook(crash::hook_with(
        dir.clone(),
        crash::UserMask::none(),
        diag,
        Box::new(|_| {}),
    ));
    let caught = fonts_library::spawn_scan(None, || {
        let r = std::panic::catch_unwind(|| -> () { panic!("Schriftleser stolpert") });
        assert!(r.is_err());
        FontLibrary::bundled_only()
    });
    let uncaught =
        fonts_library::spawn_scan(None, || -> FontLibrary { panic!("Durchsuchen stolpert") });
    let mut rig = Rig::with_state(workspace(), caught);
    wait_for_fonts(&mut rig);
    let mut rig2 = Rig::with_state(workspace(), uncaught);
    wait_for_fonts(&mut rig2);
    std::panic::set_hook(old);
    assert!(
        rig2.st().fonts().unwrap().has_family("Montserrat"),
        "Ersatz: mitgelieferte Schriften"
    );
    // Nur Berichte zu diesen Paniken zählen (eine Panik eines anderen, gleichzeitig laufenden
    // Tests ginge auch durch diesen Hook).
    let ours: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            std::fs::read_to_string(p)
                .is_ok_and(|t| t.contains("Schriftleser") || t.contains("Durchsuchen"))
        })
        .collect();
    assert!(ours.is_empty(), "kein Absturzbericht: {ours:?}");
    let lines = diag.lock().unwrap().recent_lines(50).join("\n");
    assert!(lines.contains(crash::CAUGHT_LINE), "{lines}");
}

// ---- Wenn die Schriftliste da ist ------------------------------------------------------------

#[test]
fn fonts_ready_reshapes_once_without_undo_step_or_dirty() {
    let mut saved = saved_doc("Montserrat", &FontLibrary::bundled_only());
    open(&mut saved.w, &saved.path);
    let (sk, t) = (saved.sketch, saved.text);
    let mut rig = Rig::loading(saved.w);
    rig.settle();
    let edits = rig.st().document.edit_count();
    rig.fonts_ready();
    let texts = rig.frame();
    assert!(!tab_marked(&texts), "kein „●“ im Bild der Schriftliste");
    let s = rig.st();
    assert!(s.fonts().is_some(), "die Liste ist im Tab");
    let shape = text_data(s, sk, t).shape().expect("jetzt gesetzt");
    let shape: *const TextShape = shape;
    assert_eq!(
        curve_list(s, sk, t),
        saved.curves,
        "gleiche Form, gleiche Ids"
    );
    assert_eq!(warning(s, sk), None, "Montserrat ist da");
    assert_eq!(s.document.edit_count(), edits, "nicht geändert");
    for _ in 0..5 {
        let texts = rig.frame();
        assert!(!tab_marked(&texts), "kein „●“ nach dem Neusetzen");
    }
    let s = rig.st();
    assert!(
        std::ptr::eq(text_data(s, sk, t).shape().unwrap(), shape),
        "nur einmal gesetzt"
    );
    assert_eq!(rig.tab().history().undo_len(), 0, "kein Rückgängig-Schritt");
    assert!(!rig.tab().is_dirty());
    assert_eq!(rig.tab().undo(), Err(nothing_to_undo()));
}

#[test]
fn undo_after_fonts_ready_reshapes_restored_texts() {
    // Ein Schritt, solange die Liste fehlt: Sein Bild hält den Text ungesetzt. Nach Rückgängig und
    // Wiederholen wird wieder gesetzt, ohne Schritt und ohne „●“ beim gespeicherten Stand.
    let mut saved = saved_doc("Montserrat", &FontLibrary::bundled_only());
    open(&mut saved.w, &saved.path);
    let (sk, t) = (saved.sketch, saved.text);
    let mut rig = Rig::loading(saved.w);
    rig.settle();
    rig.st()
        .create_box(planora_math::DVec3::splat(10.0))
        .unwrap();
    rig.settle();
    assert_eq!(rig.tab().history().undo_len(), 1);
    rig.fonts_ready();
    let texts = rig.settle();
    assert!(tab_marked(&texts), "der Quader ist ungespeichert");
    assert_eq!(rig.tab().history().undo_len(), 1, "kein Schritt dazu");

    rig.tab().undo().unwrap();
    let texts = rig.settle();
    assert!(!tab_marked(&texts), "zurück beim gespeicherten Stand");
    let s = rig.st();
    assert!(
        text_data(s, sk, t).shape().is_some(),
        "nach dem Sprung gesetzt"
    );
    assert_eq!(curve_list(s, sk, t), saved.curves);
    assert_eq!(rig.tab().history().undo_len(), 0);
    assert_eq!(rig.tab().history().redo_len(), 1);
    assert!(!rig.tab().is_dirty());

    rig.tab().redo().unwrap();
    let texts = rig.settle();
    assert!(tab_marked(&texts));
    assert!(text_data(rig.st(), sk, t).shape().is_some());
    assert_eq!(rig.tab().history().undo_len(), 1);
    assert_eq!(rig.tab().history().redo_len(), 0);
}

#[test]
fn missing_font_marks_dirty_and_warns() {
    // Gespeichert mit einer Schrift, die hier fehlt: Vor der Liste bleibt alles, wie es ist.
    let mut saved = saved_doc(MISSING, &SquareShaper);
    open(&mut saved.w, &saved.path);
    let (sk, t) = (saved.sketch, saved.text);
    let mut rig = Rig::loading(saved.w);
    let texts = rig.settle();
    assert!(!tab_marked(&texts));
    assert_eq!(warning(rig.st(), sk), None, "noch keine Warnung");
    assert_eq!(curve_list(rig.st(), sk, t), saved.curves);

    // Mit der Liste: Montserrat statt der fehlenden Schrift, andere Form, also geändert, mit
    // Warnung, aber ohne Rückgängig-Schritt.
    rig.fonts_ready();
    let texts = rig.settle();
    assert!(tab_marked(&texts), "„●“: Speichern schriebe etwas anderes");
    assert!(rig.tab().is_dirty());
    let s = rig.st();
    assert_ne!(curve_list(s, sk, t), saved.curves, "andere Form");
    assert_eq!(warning(s, sk), Some(font_warning(MISSING, 1)));
    assert_eq!(
        text_data(s, sk, t).style.font,
        MISSING,
        "der gespeicherte Name bleibt"
    );
    assert_eq!(rig.tab().history().undo_len(), 0, "kein Rückgängig-Schritt");
    assert_eq!(rig.tab().undo(), Err(nothing_to_undo()));
    let reshaped = curve_list(rig.st(), sk, t);

    // Ein Schritt und zurück: Das Bild vor dem Schritt ist schon das neu gesetzte.
    rig.st()
        .create_box(planora_math::DVec3::splat(10.0))
        .unwrap();
    rig.settle();
    assert_eq!(rig.tab().history().undo_len(), 1);
    rig.tab().undo().unwrap();
    let texts = rig.settle();
    assert!(tab_marked(&texts), "weiter geändert gegenüber der Datei");
    assert_eq!(curve_list(rig.st(), sk, t), reshaped);
    assert_eq!(warning(rig.st(), sk), Some(font_warning(MISSING, 1)));
    assert_eq!(rig.tab().history().undo_len(), 0);

    // Speichern schreibt den neuen Stand; danach ist der Tab sauber und bleibt es.
    rig.ws().save_active("", None).unwrap();
    let texts = rig.settle();
    assert!(!tab_marked(&texts), "gespeichert");
    assert!(!rig.tab().is_dirty());
}

fn unset_warning(count: usize) -> String {
    planora_i18n::tr!("feature-sketch-text-unset", count = count)
}

/// Rückt die Uhr weit vor und zeichnet, bis kein Bild mehr ein weiteres anfordert (Tooltips und
/// Übergänge sind dann fertig); höchstens 20 Bilder.
fn quiet(rig: &mut Rig) {
    for _ in 0..20 {
        rig.time += 5.0;
        rig.frame();
        if rig.repaint != Duration::ZERO {
            return;
        }
    }
    panic!("die App kommt nicht zur Ruhe");
}

#[test]
fn second_machine_shows_only_the_warning() {
    // Auf einem Rechner ohne die Schrift gespeichert: Die Kurven stammen schon aus Montserrat.
    // Hier fehlt sie ebenso, das Neusetzen liefert dieselbe Form. Nur die Warnung erscheint:
    // kein „●“, kein Rückgängig-Schritt, gleiche Zähler, gleiches Bild des Dokuments.
    let mut saved = saved_doc(MISSING, &FontLibrary::bundled_only());
    open(&mut saved.w, &saved.path);
    let (sk, t) = (saved.sketch, saved.text);
    let mut rig = Rig::loading(saved.w);
    rig.settle();
    quiet(&mut rig);
    let before = rig.st().snapshot();
    let edits = rig.st().document.edit_count();
    let model = rig.st().document.model_edit_count();
    assert_eq!(
        browser::sketch_row_warning(rig.st(), sk),
        None,
        "noch keine"
    );

    rig.fonts_ready();
    rig.frame();
    assert_eq!(
        rig.repaint,
        Duration::ZERO,
        "die Warnung kam nach dem Zeichnen: ein weiteres Bild"
    );
    for _ in 0..5 {
        let texts = rig.frame();
        assert!(!tab_marked(&texts), "kein „●“");
    }
    let s = rig.st();
    assert!(text_data(s, sk, t).shape().is_some(), "gesetzt");
    assert_eq!(
        curve_list(s, sk, t),
        saved.curves,
        "gleiche Form, gleiche Ids"
    );
    assert_eq!(
        browser::sketch_row_warning(s, sk),
        Some(font_warning(MISSING, 1)),
        "Warnung im Baum"
    );
    assert_eq!(s.document.edit_count(), edits, "nicht geändert");
    assert_eq!(s.document.model_edit_count(), model);
    assert!(s.snapshot().same_content(&before), "gleiches Bild");
    assert_eq!(rig.tab().history().undo_len(), 0, "kein Rückgängig-Schritt");
    assert!(!rig.tab().is_dirty());
}

#[test]
fn restored_warning_requests_a_repaint() {
    // Nach Rückgängig sind die Texte wieder ungesetzt; das Neusetzen am Ende des Bildes bringt
    // die Warnung zurück und fordert dafür ein weiteres Bild an (die Liste ist längst da).
    let mut saved = saved_doc(MISSING, &FontLibrary::bundled_only());
    open(&mut saved.w, &saved.path);
    let sk = saved.sketch;
    let mut rig = Rig::loading(saved.w);
    rig.settle();
    rig.fonts_ready();
    rig.settle();
    rig.st()
        .create_box(planora_math::DVec3::splat(10.0))
        .unwrap();
    rig.settle();
    quiet(&mut rig);
    assert_eq!(rig.tab().history().undo_len(), 1);
    rig.tab().undo().unwrap();
    assert_eq!(
        browser::sketch_row_warning(rig.st(), sk),
        None,
        "Voraussetzung: das Bild vor dem Schritt hält den Text ungesetzt"
    );
    rig.frame();
    assert_eq!(
        browser::sketch_row_warning(rig.st(), sk),
        Some(font_warning(MISSING, 1))
    );
    assert_eq!(
        rig.repaint,
        Duration::ZERO,
        "neue Warnung: ein weiteres Bild"
    );
    // Ohne Änderung bleibt es ruhig.
    quiet(&mut rig);
    rig.frame();
    assert_ne!(rig.repaint, Duration::ZERO);
}

#[test]
fn fonts_arriving_while_a_sketch_is_open_wait_for_close() {
    let mut saved = saved_doc("Montserrat", &FontLibrary::bundled_only());
    open(&mut saved.w, &saved.path);
    let (sk, t) = (saved.sketch, saved.text);
    let mut rig = Rig::loading(saved.w);
    rig.settle();
    rig.st().open_sketch(sk).unwrap();
    rig.settle();
    let undo = rig.tab().history().undo_len();
    rig.fonts_ready();
    for _ in 0..5 {
        rig.frame();
    }
    let s = rig.st();
    assert!(s.fonts().is_some(), "die Liste ist im Tab");
    assert!(
        text_data(s, sk, t).shape().is_none(),
        "solange die Skizze offen ist, wird nicht gesetzt"
    );
    let (_, draft) = s.session_draft_mut().expect("Skizze offen");
    assert!(
        draft.sketch.text(t).unwrap().shape().is_none(),
        "Entwurf ebenso"
    );

    rig.st().close_sketch();
    rig.settle();
    let s = rig.st();
    assert!(
        text_data(s, sk, t).shape().is_some(),
        "nach dem Schließen gesetzt"
    );
    assert_eq!(
        curve_list(s, sk, t),
        saved.curves,
        "gleiche Form, gleiche Ids"
    );
    assert_eq!(
        rig.tab().history().undo_len(),
        undo,
        "kein Rückgängig-Schritt"
    );
}

#[test]
fn text_that_cannot_be_set_keeps_its_curves_and_warns() {
    // Gespeichert mit einer Schrift, die hier fehlt, und Zeichen, die weder Montserrat noch Roboto
    // haben: Die Ersatzschrift ergäbe keine Kurve. Der Text verschwindet nicht (Anti-Ziel), die
    // Skizze warnt im Baum, und der Abschnitt „Text“ sagt es beim Ändern.
    let mut saved = saved_doc_with("漢字", MISSING, &SquareShaper);
    open(&mut saved.w, &saved.path);
    let (sk, t) = (saved.sketch, saved.text);
    let mut rig = Rig::loading(saved.w);
    rig.settle();
    rig.fonts_ready();
    let texts = rig.settle();
    assert!(!tab_marked(&texts), "nichts geändert");
    let s = rig.st();
    assert_eq!(curve_list(s, sk, t), saved.curves, "die Kurven bleiben");
    assert!(text_data(s, sk, t).is_unset());
    assert_eq!(
        browser::sketch_row_warning(s, sk),
        Some(unset_warning(1)),
        "Warnung im Baum"
    );
    assert_eq!(rig.tab().history().undo_len(), 0);
    assert!(!rig.tab().is_dirty());

    rig.st().open_sketch(sk).unwrap();
    rig.settle();
    let (se, sf) = rig.st().session_draft_mut().expect("Skizze offen");
    assert!(se.begin_text_edit(&mut sf.sketch, t));
    let texts = rig.settle();
    let hint = planora_i18n::tr!("sketch-text-hint-unset");
    assert!(texts.contains(&hint), "Hinweis im Abschnitt: {texts:?}");
}

#[test]
fn font_scan_logs_skipped_files_and_counts() {
    // Das Protokoll gehört zum Durchsuchen im Hintergrund, steht aber außerhalb des Merkers für
    // abgefangene Paniken (nur `FontLibrary::scan` läuft darin).
    let dir = common::temp_root("planora-font-scan-log");
    std::fs::create_dir_all(&dir).unwrap();
    let name = format!("kaputt-{}.ttf", std::process::id());
    std::fs::write(dir.join(&name), b"keine Schrift").unwrap();
    let state = fonts_library::spawn_scan(None, move || FontLibrary::scan(&[dir]));
    let mut rig = Rig::with_state(workspace(), state);
    wait_for_fonts(&mut rig);
    assert_eq!(rig.st().fonts().unwrap().skipped().len(), 1);
    let lines = planora_app::diag::recent_lines(2000);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Font skipped:") && l.contains(&name)),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Fonts read:") && l.contains("1 files skipped")),
        "{lines:?}"
    );
}
