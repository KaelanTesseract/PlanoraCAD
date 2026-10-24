//! Tastenkürzel für Rückgängig und Wiederholen (Aufgabe 8, R8): Strg+Z nimmt zurück,
//! Strg+Umschalt+Z und Strg+Y stellen wieder her – je Druck genau ein Schritt, auch wenn ein Bild
//! zwei Durchgänge hat. Gesperrt beim Tippen (das Textfeld behält sein eigenes Rückgängig), bei
//! jedem offenen Fenster, bei „Über“, bei offener Rückfrage und solange eine Maustaste gedrückt
//! ist. In der offenen Skizze trifft es die Skizze, in der Zeichnung den Verlauf des Tabs. Die
//! Statuszeile nennt danach die Aktion bzw. sagt, dass es nichts zu tun gibt.
//!
//! Alles mit echten Bildern der ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`) und
//! Tastenereignissen mit `Modifiers::COMMAND` (und `SHIFT`).

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::blend_dialog::BlendDialog;
use planora_app::browser::delete_plan;
use planora_app::create_dialog::CreateDialog;
use planora_app::dimension_dialog::DimensionDialog;
use planora_app::drawing_dialog::NewDrawingDialog;
use planora_app::drawing_state::WorkArea;
use planora_app::export_dialog::ExportDialog;
use planora_app::feature_dialog::FeatureDialog;
use planora_app::history::{History, snapshot_bytes};
use planora_app::properties::POSITION_LABEL;
use planora_app::sketch_state::Tool;
use planora_app::state::*;
use planora_app::workspace::*;
use planora_app::{about_dialog, fonts, sheet_dialog, theme, viewport};
use planora_drawing::dimension::{DimId, DimKind, DimSpec, GeoRef, Tolerance as DimTolerance};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_feature::{FeatureKind, FeatureStatus, SketchPlane, StandardPlane, geo_ref_for_edge};
use planora_math::{DVec2, DVec3, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1400.0, 900.0);

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

// ---- Hilfen ---------------------------------------------------------------------------------

fn ws() -> Workspace {
    let root = common::temp_root("planora-undo-keys");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    color: egui::Color32,
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    /// Blieben nach dem Bild Tastenereignisse Z oder Y mit Strg übrig (nicht verbraucht)?
    left_over: bool,
    /// Wie oft `ui` im letzten Bild lief (Durchgänge).
    passes: usize,
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
            left_over: false,
            passes: 0,
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn tab(&mut self) -> &mut DocTab {
        self.app.ws_mut().active_tab_mut()
    }

    /// Ein Bild; mit `two_passes` verwirft der erste Durchgang sich selbst (wie egui es etwa
    /// beim Wechsel der Felder tut), `ui` läuft dann zweimal mit denselben Eingaben.
    fn run(&mut self, events: Vec<Event>, two_passes: bool) -> Vec<Seen> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let mut left_over = false;
        let mut passes = 0;
        let out = self.ctx.run_ui(raw, |ui| {
            passes += 1;
            if two_passes && ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("Test: zweiter Durchgang");
            }
            app.ui(ui, &mut f);
            left_over = ui.input(|i| {
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        Event::Key {
                            key: Key::Z | Key::Y,
                            pressed: true,
                            modifiers,
                            ..
                        } if modifiers.command
                    )
                })
            });
        });
        self.left_over = left_over;
        self.passes = passes;
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
            match shape {
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                egui::epaint::Shape::Text(t) => {
                    // Farbe wie gemalt: erster Abschnitt, Platzhalter → Rückfallfarbe, Vorgabe
                    // überschreibt.
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

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        self.run(events, false)
    }

    fn settle(&mut self) -> Vec<Seen> {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    /// Ein Kürzel in einem Bild; liefert dieses Bild.
    fn press(&mut self, k: Key, m: Modifiers) -> Vec<Seen> {
        self.frame(vec![key(k, m)])
    }

    fn undo_key(&mut self) -> Vec<Seen> {
        self.press(Key::Z, Modifiers::COMMAND)
    }

    fn undo_len(&mut self) -> usize {
        self.ws().active_tab().history().undo_len()
    }

    fn redo_len(&mut self) -> usize {
        self.ws().active_tab().history().redo_len()
    }

    fn bodies(&mut self) -> usize {
        self.st().bodies.len()
    }

    fn message(&mut self) -> Option<String> {
        self.st().message.clone()
    }

    fn info(&mut self) -> Option<String> {
        self.st().info.clone()
    }
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

const SHIFT_COMMAND: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: true,
};

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Klick an `p` in drei Bildern (hin, drücken, loslassen) und ein Bild danach.
fn click(rig: &mut Rig, p: Pos2) {
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![button(p, true)]);
    rig.frame(vec![button(p, false)]);
    rig.frame(vec![]);
}

/// Quader, dann Kugel: zwei Schritte.
fn box_and_sphere() -> Rig {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(30.0, 20.0, 10.0))
        .unwrap();
    w.observe_history(false);
    w.active_tab_mut().state.create_sphere(5.0).unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    assert_eq!(rig.undo_len(), 2);
    assert_eq!(rig.bodies(), 2);
    rig
}

fn drawing_with_front(s: &mut AppState) -> (DrawingId, ViewId) {
    let d = s
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let v = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 148.5, y: 105.0 },
            None,
        )
        .unwrap();
    (d, v)
}

/// Breitenmaß zwischen zwei senkrechten Kanten des ersten Körpers.
fn add_width(st: &mut AppState, d: DrawingId, view: ViewId) {
    fn edge_where(st: &AppState, pred: impl Fn(DVec3) -> bool) -> GeoRef {
        let solid = &st.bodies[0].solid;
        let (id, _) = solid
            .edges
            .iter()
            .find(|(_, e)| {
                let a = solid.vertices.get(e.start).unwrap().point;
                let b = solid.vertices.get(e.end).unwrap().point;
                e.start != e.end && pred(a) && pred(b)
            })
            .expect("Kante nicht gefunden");
        GeoRef::Edge(geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap())
    }
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let (lo, hi) = st.bodies[0].bounds;
    let refs = vec![
        edge_where(st, |p| near(p.x, lo.x) && near(p.y, lo.y)),
        edge_where(st, |p| near(p.x, hi.x) && near(p.y, lo.y)),
    ];
    st.document.add_dimension(d, dim_spec(view, refs)).unwrap();
}

fn dim_spec(view: ViewId, refs: Vec<GeoRef>) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind: DimKind::Horizontal,
        refs,
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

/// Mitte des Positionsfelds X (das linke Zahlenfeld unter der Überschrift „Position“).
fn position_x_field(seen: &[Seen]) -> Pos2 {
    let label = seen
        .iter()
        .find(|s| s.text == POSITION_LABEL)
        .unwrap_or_else(|| panic!("„{POSITION_LABEL}“ fehlt"))
        .rect;
    seen.iter()
        .filter(|s| {
            s.rect.top() >= label.bottom() - 0.5
                && s.rect.top() < label.bottom() + 40.0
                && s.text.parse::<f64>().is_ok()
        })
        .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
        .unwrap_or_else(|| panic!("kein Zahlenfeld unter „{POSITION_LABEL}“"))
        .rect
        .center()
}

/// Text des Positionsfelds X im Bild.
fn position_x_text(seen: &[Seen], at: Pos2) -> String {
    seen.iter()
        .filter(|s| (s.rect.center().y - at.y).abs() < 6.0 && s.rect.left() <= at.x + 40.0)
        .filter(|s| s.rect.right() >= at.x - 40.0)
        .map(|s| s.text.clone())
        .next()
        .unwrap_or_default()
}

/// Farbe des einzigen gemalten Textes `text` (die Statuszeile).
fn status_color(seen: &[Seen], text: &str) -> egui::Color32 {
    let all: Vec<_> = seen.iter().filter(|s| s.text == text).collect();
    assert_eq!(all.len(), 1, "„{text}“ genau einmal gemalt: {all:?}");
    all[0].color
}

// ---- (a) Strg+Z, Strg+Umschalt+Z, Strg+Y ------------------------------------------------------

#[test]
fn keys_undo_and_redo() {
    let mut rig = box_and_sphere();

    rig.undo_key();
    assert_eq!(rig.bodies(), 1, "Strg+Z: die Kugel ist weg");
    assert_eq!(
        (rig.undo_len(), rig.redo_len()),
        (1, 1),
        "genau ein Schritt"
    );
    assert!(!rig.left_over, "das Kürzel ist verbraucht");
    assert_eq!(rig.info().as_deref(), Some("Rückgängig: Kugel anlegen"));
    assert_eq!(rig.message(), None, "keine Fehlermeldung");
    let seen = rig.settle();
    assert_eq!(
        status_color(&seen, "Rückgängig: Kugel anlegen"),
        theme::palette(true).text_secondary,
        "die Statuszeile nennt die Aktion in normaler Farbe"
    );
    assert_eq!(rig.undo_len(), 1, "Rückgängig ist kein neuer Schritt");

    rig.press(Key::Z, SHIFT_COMMAND);
    assert_eq!(rig.bodies(), 2, "Strg+Umschalt+Z: die Kugel ist wieder da");
    assert_eq!((rig.undo_len(), rig.redo_len()), (2, 0));
    assert!(!rig.left_over);
    assert_eq!(rig.info().as_deref(), Some("Wiederholen: Kugel anlegen"));
    rig.settle();

    rig.undo_key();
    assert_eq!(rig.bodies(), 1);
    rig.press(Key::Y, Modifiers::COMMAND);
    assert_eq!(rig.bodies(), 2, "Strg+Y: wieder da");
    assert_eq!((rig.undo_len(), rig.redo_len()), (2, 0));
    assert!(!rig.left_over);
    assert_eq!(rig.info().as_deref(), Some("Wiederholen: Kugel anlegen"));
    rig.settle();

    // Zwei Durchgänge eines Bildes: das Kürzel wirkt trotzdem nur einmal.
    rig.run(vec![key(Key::Z, Modifiers::COMMAND)], true);
    assert_eq!(rig.passes, 2, "das Bild hatte zwei Durchgänge");
    assert_eq!(rig.bodies(), 1);
    assert_eq!(
        (rig.undo_len(), rig.redo_len()),
        (1, 1),
        "nur ein Schritt zurück"
    );
    rig.run(vec![key(Key::Y, Modifiers::COMMAND)], true);
    assert_eq!(
        (rig.undo_len(), rig.redo_len()),
        (2, 0),
        "nur ein Schritt vor"
    );
    rig.run(vec![key(Key::Z, SHIFT_COMMAND)], true);
    assert_eq!(
        (rig.undo_len(), rig.redo_len()),
        (2, 0),
        "nichts mehr zu wiederholen"
    );
    assert_eq!(rig.message().as_deref(), Some(NOTHING_TO_REDO));
    assert_eq!(rig.bodies(), 2);

    // Bis zum Anfang und darüber hinaus: die Statuszeile sagt, dass es nichts gibt.
    rig.undo_key();
    rig.undo_key();
    assert_eq!(rig.bodies(), 0);
    assert_eq!(rig.info().as_deref(), Some("Rückgängig: Quader anlegen"));
    rig.undo_key();
    assert_eq!(rig.bodies(), 0);
    assert_eq!((rig.undo_len(), rig.redo_len()), (0, 2));
    assert_eq!(rig.message().as_deref(), Some(NOTHING_TO_UNDO));
    assert_eq!(rig.info(), None);
    let seen = rig.settle();
    assert_eq!(
        status_color(&seen, NOTHING_TO_UNDO),
        theme::palette(true).error
    );
}

/// Ohne jeden Schritt: beide Kürzel ändern nichts und sagen es.
#[test]
fn empty_history_says_so() {
    let mut rig = Rig::new(ws());
    rig.undo_key();
    assert_eq!(rig.message().as_deref(), Some(NOTHING_TO_UNDO));
    assert!(!rig.left_over);
    rig.press(Key::Y, Modifiers::COMMAND);
    assert_eq!(rig.message().as_deref(), Some(NOTHING_TO_REDO));
    rig.press(Key::Z, SHIFT_COMMAND);
    assert_eq!(rig.message().as_deref(), Some(NOTHING_TO_REDO));
    assert_eq!((rig.undo_len(), rig.redo_len()), (0, 0));
}

// ---- (b) Tippen behält Strg+Z für das Feld ---------------------------------------------------

#[test]
fn typing_keeps_ctrl_z_for_the_field() {
    // Positionsfeld (kein Fenster): nur die Tipp-Sperre schützt das Dokument.
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(30.0, 20.0, 10.0))
        .unwrap();
    w.active_tab_mut().state.selected_body = Some(0);
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let seen = rig.settle();
    let p = position_x_field(&seen);
    let original = position_x_text(&seen, p);
    assert_eq!(
        original.parse::<f64>().ok(),
        Some(0.0),
        "Feld zeigt {original}"
    );
    let steps = rig.undo_len();
    click(&mut rig, p);
    assert!(
        viewport::typing(&rig.ctx),
        "das Positionsfeld nimmt Tasten an"
    );
    rig.frame(vec![key(Key::A, Modifiers::COMMAND)]);
    for c in ["4", "2"] {
        rig.frame(vec![Event::Text(c.into())]);
    }
    let seen = rig.frame(vec![]);
    assert_eq!(position_x_text(&seen, p), "42", "getippt");
    assert_eq!(
        rig.st().bodies[0].bounds.0.x,
        42.0,
        "das Feld wirkt beim Tippen"
    );

    rig.undo_key();
    assert!(rig.left_over, "das Kürzel gehört dem Feld");
    let seen = rig.frame(vec![]);
    // Das Feld nimmt nur sein Tippen zurück (es wirkt sofort, also steht der Quader wieder
    // bei 0); der Verlauf bleibt unberührt, der Quader selbst bleibt.
    assert_eq!(rig.st().bodies[0].bounds.0.x, 0.0);
    assert_eq!(rig.undo_len(), steps);
    assert_eq!(rig.redo_len(), 0);
    assert_eq!(rig.bodies(), 1);
    assert_eq!(rig.message(), None, "keine Meldung");
    assert_eq!(
        position_x_text(&seen, p),
        original,
        "das Feld nimmt sein Tippen selbst zurück"
    );
    assert!(viewport::typing(&rig.ctx), "das Feld bleibt aktiv");
    // Strg+Y ebenso.
    rig.press(Key::Y, Modifiers::COMMAND);
    assert!(rig.left_over);
    assert_eq!((rig.undo_len(), rig.redo_len()), (steps, 0));
    assert_eq!(rig.bodies(), 1);

    // Feld verlassen (Escape), danach wirkt Strg+Z wieder auf das Dokument.
    rig.frame(vec![key(Key::Escape, Modifiers::NONE)]);
    rig.settle();
    assert!(!viewport::typing(&rig.ctx));
    let n = rig.undo_len();
    assert!(n >= 1);
    rig.undo_key();
    assert_eq!(rig.undo_len(), n - 1, "Strg+Z wirkt wieder");
    assert!(rig.info().unwrap().starts_with("Rückgängig: "));
}

#[test]
fn typing_in_rename_window_keeps_ctrl_z() {
    let mut w = ws();
    let id = w
        .active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.observe_history(false);
    w.active_tab_mut().state.rename_draft = Some((id, "Teil".into()));
    let mut rig = Rig::new(w);
    let seen = rig.settle();
    let at = seen
        .iter()
        .find(|s| s.text == "Teil")
        .expect("Feld im Fenster „Umbenennen“")
        .rect
        .center();
    click(&mut rig, at);
    assert!(viewport::typing(&rig.ctx), "das Namensfeld nimmt Tasten an");
    rig.frame(vec![key(Key::End, Modifiers::NONE)]);
    for c in ["X", "Y"] {
        rig.frame(vec![Event::Text(c.into())]);
    }
    rig.frame(vec![]);
    assert_eq!(rig.st().rename_draft.as_ref().unwrap().1, "TeilXY");
    let doc = rig.st().snapshot();

    rig.undo_key();
    rig.frame(vec![]);
    assert!(
        rig.st().snapshot().same_content(&doc),
        "Dokument unverändert"
    );
    assert_eq!(rig.bodies(), 1);
    assert_eq!(
        rig.st().rename_draft.as_ref().map(|d| d.1.as_str()),
        Some("Teil"),
        "das Feld nimmt das Tippen zurück, das Fenster bleibt"
    );
}

// ---- (c) Fenster, „Über“, Rückfrage sperren ---------------------------------------------------

/// Öffnet Fenster Nummer `which` (`None`, wenn es die Nummer nicht gibt) und liefert seinen Namen.
fn open_window(w: &mut Workspace, which: usize, d: DrawingId, v: ViewId) -> Option<&'static str> {
    let t = w.active_tab_mut();
    let quader = t.state.document.features()[0].id;
    Some(match which {
        0 => {
            t.dialog = Some(CreateDialog::Box {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            });
            "Quader erzeugen"
        }
        1 => {
            t.feature_dialog = FeatureDialog::new(&t.state, quader);
            "Ändern"
        }
        2 => {
            t.blend_dialog = Some(BlendDialog::fillet());
            "Verrunden"
        }
        3 => {
            t.drawing_dialog = Some(NewDrawingDialog::new(&t.state));
            "Neue Zeichnung"
        }
        4 => {
            let spec = t.state.document.drawing(d).unwrap().dimensions[0].clone();
            assert_eq!(spec.view, v);
            t.state.dimension_dialog = Some(DimensionDialog::new(d, &spec, false));
            "Maß bearbeiten"
        }
        5 => {
            sheet_dialog::open(&mut t.state, d);
            "Blattangaben"
        }
        6 => {
            t.state.pending_delete = Some(delete_plan(&t.state, quader));
            "Löschen mit Folgen"
        }
        7 => {
            t.export_dialog = Some(ExportDialog::new(&t.state));
            "Exportieren"
        }
        8 => {
            w.about_open = true;
            about_dialog::TITLE
        }
        9 => {
            w.prompt = Some(Prompt::Notice("Hinweis".into()));
            "Rückfrage"
        }
        _ => return None,
    })
}

fn still_open(w: &Workspace, which: usize) -> bool {
    match which {
        8 => w.about_open,
        9 => w.prompt.is_some(),
        _ => w.active_tab().dialog_open(),
    }
}

fn close_window(w: &mut Workspace) {
    w.about_open = false;
    w.prompt = None;
    w.active_tab_mut().close_dialogs();
}

#[test]
fn dialog_blocks_keys() {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(100.0, 60.0, 40.0))
        .unwrap();
    let (d, v) = drawing_with_front(&mut w.active_tab_mut().state);
    add_width(&mut w.active_tab_mut().state, d, v);
    w.observe_history(false);
    w.active_tab_mut().state.create_sphere(5.0).unwrap();
    w.observe_history(false);
    // Noch eine Kugel, gleich zurückgenommen: so gibt es auch etwas zu wiederholen.
    w.active_tab_mut().state.create_sphere(2.0).unwrap();
    w.observe_history(false);
    w.active_tab_mut().undo().unwrap();
    let mut rig = Rig::new(w);
    assert_eq!((rig.undo_len(), rig.redo_len(), rig.bodies()), (2, 1, 2));
    let mut n = 0;
    while let Some(name) = open_window(rig.ws(), n, d, v) {
        rig.settle();
        assert!(still_open(rig.ws(), n), "{name}: Fenster offen");
        let doc = rig.st().snapshot();
        let steps = (rig.undo_len(), rig.redo_len());
        let message = rig.message();
        for (k, m, what) in [
            (Key::Z, Modifiers::COMMAND, "Strg+Z"),
            (Key::Y, Modifiers::COMMAND, "Strg+Y"),
            (Key::Z, SHIFT_COMMAND, "Strg+Umschalt+Z"),
        ] {
            rig.press(k, m);
            rig.frame(vec![]);
            assert!(
                rig.st().snapshot().same_content(&doc),
                "{name}: {what} ändert nichts"
            );
            assert_eq!((rig.undo_len(), rig.redo_len()), steps, "{name}: {what}");
            assert_eq!(rig.bodies(), 2, "{name}: {what}");
            assert_eq!(rig.message(), message, "{name}: {what} ohne Meldung");
            assert!(still_open(rig.ws(), n), "{name}: Fenster bleibt offen");
        }

        close_window(rig.ws());
        rig.settle();
        rig.undo_key();
        assert_eq!(rig.bodies(), 1, "{name}: nach dem Schließen wirkt Strg+Z");
        rig.press(Key::Y, Modifiers::COMMAND);
        assert_eq!(rig.bodies(), 2, "{name}: und Strg+Y");
        rig.press(Key::Z, SHIFT_COMMAND);
        assert_eq!(rig.bodies(), 3, "{name}: und Strg+Umschalt+Z");
        rig.undo_key();
        assert_eq!((rig.undo_len(), rig.redo_len(), rig.bodies()), (2, 1, 2));
        rig.settle();
        n += 1;
    }
    assert_eq!(n, 10);
}

/// Die Fenster der Felder Projekte (Rückfrage „Löschen“) und Versionen (Rückfrage „Löschen“)
/// sperren ebenso.
#[test]
fn panel_windows_block_keys() {
    let mut w = ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    w.active_tab_mut().state.create_sphere(2.0).unwrap();
    w.observe_history(false);
    w.active_tab_mut().state.create_sphere(1.0).unwrap();
    w.observe_history(false);
    w.active_tab_mut().undo().unwrap();
    let mut rig = Rig::new(w);
    assert_eq!((rig.bodies(), rig.redo_len()), (2, 1));

    for which in 0..2 {
        {
            let (ws, projects, versions) = rig.app.panels_mut();
            if which == 0 {
                planora_app::project_actions::request_delete(projects, p.clone());
                assert!(planora_app::project_actions::dialog_open(projects));
            } else {
                planora_app::versions_panel::request_delete_version(ws, versions, 1);
                assert!(planora_app::versions_panel::has_pending(versions));
            }
        }
        rig.settle();
        rig.undo_key();
        rig.settle();
        assert_eq!(rig.bodies(), 2, "Fenster {which}: Strg+Z ändert nichts");
        assert_eq!(rig.redo_len(), 1, "Fenster {which}");
        rig.press(Key::Y, Modifiers::COMMAND);
        rig.settle();
        assert_eq!(rig.bodies(), 2, "Fenster {which}: Strg+Y ändert nichts");
        assert_eq!(rig.redo_len(), 1, "Fenster {which}");
        let (_, projects, versions) = rig.app.panels_mut();
        *projects = Default::default();
        *versions = Default::default();
    }
    rig.settle();
    rig.undo_key();
    assert_eq!(rig.bodies(), 1, "ohne Fenster wirkt Strg+Z");
}

#[test]
fn held_mouse_button_blocks_keys() {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(30.0, 20.0, 10.0))
        .unwrap();
    w.active_tab_mut().state.selected_body = Some(0);
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let seen = rig.settle();
    let p = position_x_field(&seen);
    let before = rig.st().bodies[0].bounds.0;
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![button(p, true)]);
    for i in 1..=10 {
        rig.frame(vec![Event::PointerMoved(p + vec2(3.0 * i as f32, 0.0))]);
    }
    let moved = rig.st().bodies[0].bounds.0;
    assert!(moved.x > before.x + 5.0, "gezogen");
    // Strg+Z mitten im Ziehen: nichts (sonst würde das halbe Ziehen ein Schritt und zurück).
    rig.undo_key();
    assert_eq!(rig.undo_len(), 1, "kein Schritt während des Ziehens");
    assert_eq!(rig.redo_len(), 0);
    assert_eq!(
        rig.st().bodies[0].bounds.0,
        moved,
        "das Ziehen bleibt stehen"
    );
    assert_eq!(rig.message(), None);
    rig.press(Key::Y, Modifiers::COMMAND);
    assert_eq!((rig.undo_len(), rig.redo_len()), (1, 0));
    assert_eq!(rig.bodies(), 1);
    assert_eq!(rig.message(), None);
    rig.frame(vec![Event::PointerMoved(p + vec2(45.0, 0.0))]);
    let end = rig.st().bodies[0].bounds.0;
    assert!(end.x > moved.x, "das Ziehen geht weiter");
    rig.frame(vec![button(p + vec2(45.0, 0.0), false)]);
    rig.settle();
    assert_eq!(rig.undo_len(), 2, "das ganze Ziehen ist ein Schritt");
    rig.undo_key();
    assert_eq!(rig.st().bodies[0].bounds.0, before, "zurück vor das Ziehen");
    assert_eq!(rig.info().as_deref(), Some("Rückgängig: Verschieben"));
}

// ---- (d) Skizze und Zeichnung ------------------------------------------------------------------

#[test]
fn keys_in_sketch_mode() {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(5.0))
        .unwrap();
    w.observe_history(false);
    let id = w
        .active_tab_mut()
        .state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    w.active_tab_mut().state.open_sketch(id).unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let edits = rig.st().document.edit_count();
    let doc_steps = (rig.undo_len(), rig.redo_len());
    let curves = |rig: &mut Rig| rig.st().session_sketch().unwrap().curves().count();
    {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    for (x, y) in [(0.0, 0.0), (10.0, 4.0), (3.0, 12.0)] {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.click(sk, DVec2::new(x, y), 0.5, false).unwrap();
        rig.frame(vec![]);
    }
    rig.settle();
    assert_eq!(curves(&mut rig), 2);

    rig.undo_key();
    assert_eq!(curves(&mut rig), 1, "Strg+Z nimmt eine Linie zurück");
    assert!(!rig.left_over);
    assert!(rig.st().sketch_session.is_some(), "die Skizze bleibt offen");
    assert_eq!(rig.st().document.edit_count(), edits, "Dokument unberührt");
    assert_eq!(
        (rig.undo_len(), rig.redo_len()),
        doc_steps,
        "Dokumentverlauf ruht"
    );
    assert_eq!(rig.info().as_deref(), Some("Rückgängig: Linie zeichnen"));
    rig.settle();
    rig.press(Key::Y, Modifiers::COMMAND);
    assert_eq!(curves(&mut rig), 2, "Strg+Y stellt sie wieder her");
    assert_eq!(rig.info().as_deref(), Some("Wiederholen: Linie zeichnen"));
    rig.settle();
    rig.undo_key();
    rig.undo_key();
    rig.undo_key();
    assert_eq!(curves(&mut rig), 0);
    assert_eq!(rig.message().as_deref(), Some(NOTHING_TO_UNDO));
    assert_eq!(rig.st().document.edit_count(), edits);
    assert_eq!(rig.bodies(), 1, "der Quader bleibt");
    assert!(rig.st().sketch_session.is_some());
}

#[test]
fn keys_in_drawing_view() {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(100.0, 60.0, 40.0))
        .unwrap();
    let (d, _) = drawing_with_front(&mut w.active_tab_mut().state);
    w.observe_history(false);
    w.active_tab_mut().state.drawing_ui.open(d);
    let mut rig = Rig::new(w);
    let views = |rig: &mut Rig| rig.st().document.drawing(d).unwrap().views.len();
    assert_eq!(views(&mut rig), 1);
    rig.st()
        .document
        .add_view(
            d,
            ViewKind::Top,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 60.0, y: 60.0 },
            None,
        )
        .unwrap();
    rig.settle();
    assert_eq!(views(&mut rig), 2);
    let steps = rig.undo_len();

    rig.undo_key();
    assert_eq!(views(&mut rig), 1, "Strg+Z nimmt die Ansicht zurück");
    assert_eq!(rig.undo_len(), steps - 1, "derselbe Verlauf des Tabs");
    assert!(matches!(rig.st().drawing_ui.area, WorkArea::Drawing(x) if x == d));
    assert!(rig.info().unwrap().starts_with("Rückgängig: "));
    rig.settle();
    rig.press(Key::Z, SHIFT_COMMAND);
    assert_eq!(views(&mut rig), 2, "Strg+Umschalt+Z stellt sie wieder her");
    assert!(rig.info().unwrap().starts_with("Wiederholen: "));
}

// ---- (e) Andere Kürzel unverändert ---------------------------------------------------------

#[test]
fn other_shortcuts_unchanged() {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);

    rig.press(Key::N, Modifiers::COMMAND);
    rig.settle();
    assert_eq!(rig.ws().tabs.len(), 2, "Strg+N: neuer Tab");
    assert_eq!(rig.ws().active, 1);
    rig.press(Key::W, Modifiers::COMMAND);
    rig.settle();
    assert_eq!(rig.ws().tabs.len(), 1, "Strg+W: Tab zu");
    assert_eq!(rig.bodies(), 1, "der erste Tab ist unberührt");

    rig.press(Key::E, SHIFT_COMMAND);
    rig.settle();
    assert!(rig.tab().export_dialog.is_some(), "Strg+Umschalt+E: Export");
    rig.tab().export_dialog = None;
    rig.settle();

    rig.press(Key::S, Modifiers::COMMAND);
    rig.settle();
    assert!(
        matches!(rig.ws().prompt, Some(Prompt::SaveAs { .. })),
        "Strg+S: speichern fragt nach dem Ort"
    );
    rig.ws().prompt = None;
    rig.settle();

    // Strg+O blendet die Projekte ein und ändert das Dokument nicht.
    let doc = rig.st().snapshot();
    rig.press(Key::O, Modifiers::COMMAND);
    rig.settle();
    assert!(rig.st().snapshot().same_content(&doc));
    assert_eq!(rig.bodies(), 1);

    // Keines der Kürzel war ein Rückgängig; Strg+Z wirkt danach wie gewohnt.
    assert_eq!(rig.redo_len(), 0);
    rig.undo_key();
    assert_eq!(rig.bodies(), 0);
}

// ---- Hinweis in der Statuszeile, Fehler, gehaltene Taste ------------------------------------

/// Der Hinweis „Rückgängig: …“ bleibt über Bilder ohne Änderung stehen und endet mit dem
/// nächsten Schritt oder einer Meldung (die dann in der Fehlerfarbe steht).
#[test]
fn info_ends_with_next_step_or_message() {
    let mut rig = box_and_sphere();
    rig.undo_key();
    rig.settle();
    rig.frame(vec![Event::PointerMoved(Pos2::new(700.0, 450.0))]);
    assert_eq!(
        rig.info().as_deref(),
        Some("Rückgängig: Kugel anlegen"),
        "bleibt ohne neue Änderung"
    );
    // Neue Aktion → neuer Schritt → Hinweis weg.
    rig.st().create_sphere(1.0).unwrap();
    let seen = rig.settle();
    assert_eq!(rig.info(), None, "endet mit dem nächsten Schritt");
    assert!(!seen.iter().any(|s| s.text.starts_with("Rückgängig")));
    assert_eq!(
        status_color(&seen, "2 Körper"),
        theme::palette(true).text_secondary
    );

    // Hinweis, dann eine Meldung: die Meldung steht in der Fehlerfarbe, der Hinweis ist weg.
    rig.undo_key();
    assert_eq!(rig.info().as_deref(), Some("Rückgängig: Kugel anlegen"));
    rig.st().message = Some("Etwas ging schief".into());
    let seen = rig.settle();
    assert_eq!(rig.info(), None, "endet mit einer Meldung");
    assert_eq!(
        status_color(&seen, "Etwas ging schief"),
        theme::palette(true).error
    );
    rig.st().message = None;
    let seen = rig.settle();
    assert!(
        !seen.iter().any(|s| s.text.starts_with("Rückgängig")),
        "der alte Hinweis kommt nicht wieder"
    );
}

/// Im Skizzen-Modus steht der Hinweis hinter der Zeile „Skizzen-Modus: …“; beides bleibt
/// sichtbar, der Hinweis endet mit dem nächsten Skizzenschritt.
#[test]
fn info_keeps_the_sketch_line() {
    let mut w = ws();
    let id = w
        .active_tab_mut()
        .state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    w.active_tab_mut().state.open_sketch(id).unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let name = rig.st().session_sketch().unwrap().name.clone();
    {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    for (x, y) in [(0.0, 0.0), (10.0, 4.0)] {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.click(sk, DVec2::new(x, y), 0.5, false).unwrap();
        rig.frame(vec![]);
    }
    rig.settle();
    rig.undo_key();
    let seen = rig.settle();
    let both = format!("Skizzen-Modus: {name} · Rückgängig: Linie zeichnen");
    assert_eq!(
        status_color(&seen, &both),
        theme::palette(true).text_secondary
    );
    // Nächster Skizzenschritt: nur noch die Zeile des Skizzen-Modus.
    {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Point);
        se.click(sk, DVec2::new(5.0, 5.0), 0.5, false).unwrap();
    }
    let seen = rig.settle();
    assert_eq!(rig.info(), None);
    assert_eq!(
        status_color(&seen, &format!("Skizzen-Modus: {name}")),
        theme::palette(true).text_secondary
    );
}

/// Schlägt das Zurückladen über die Taste fehl, steht die Fehlermeldung in der Fehlerfarbe und
/// kein Erfolgshinweis.
#[test]
fn failed_restore_through_keys() {
    // Rückgängig: das mittlere Bild ist unbrauchbar (Marke hinter dem Ende).
    let mut w = ws();
    let s0 = w.active_tab().state.snapshot();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    let mut s1 = w.active_tab().state.snapshot();
    w.active_tab_mut().state.create_sphere(3.0).unwrap();
    let s2 = w.active_tab().state.snapshot();
    w.active_tab_mut().state.create_cone(2.0, 3.0).unwrap();
    let s3 = w.active_tab().state.snapshot();
    s1.marker = s1.features.len() + 5;
    let mut h = History::new(s0.clone(), snapshot_bytes(&s0));
    h.record(s1, 0, "Quader anlegen");
    h.record(s2, 0, "Kugel anlegen");
    h.record(s3, 0, "Kegel anlegen");
    *w.active_tab_mut().history_mut() = h;
    let mut rig = Rig::new(w);
    assert_eq!(rig.undo_len(), 3);
    rig.undo_key();
    assert_eq!(rig.info().as_deref(), Some("Rückgängig: Kegel anlegen"));
    rig.settle();
    rig.undo_key();
    assert_eq!(rig.message().as_deref(), Some(UNDO_FAILED));
    assert_eq!(rig.info(), None, "kein Erfolgshinweis");
    assert_eq!(rig.bodies(), 2, "Dokument unverändert");
    let seen = rig.settle();
    assert_eq!(status_color(&seen, UNDO_FAILED), theme::palette(true).error);
    assert!(!seen.iter().any(|s| s.text.starts_with("Rückgängig:")));

    // Wiederholen: das nächste Bild ist unbrauchbar.
    let mut scratch = AppState::new();
    let t0 = scratch.snapshot();
    scratch.create_box(DVec3::splat(4.0)).unwrap();
    let mut t1 = scratch.snapshot();
    t1.marker = t1.features.len() + 5;
    let mut h = History::new(t0.clone(), snapshot_bytes(&t0));
    h.record(t1, 0, "Quader anlegen");
    h.commit_undo();
    let mut w = ws();
    *w.active_tab_mut().history_mut() = h;
    let mut rig = Rig::new(w);
    assert_eq!(rig.redo_len(), 1);
    rig.press(Key::Y, Modifiers::COMMAND);
    assert_eq!(rig.message().as_deref(), Some(REDO_FAILED));
    assert_eq!(rig.info(), None, "kein Erfolgshinweis");
    assert_eq!(rig.bodies(), 0);
    let seen = rig.settle();
    assert_eq!(status_color(&seen, REDO_FAILED), theme::palette(true).error);
    assert!(!seen.iter().any(|s| s.text.starts_with("Wiederholen:")));
}

/// Gehaltene Taste: mehrere Ereignisse (Wiederholungen) in einem Bild sind ein Schritt, jedes
/// weitere Bild mit einem Ereignis ein weiterer.
#[test]
fn held_key_is_one_step_per_frame() {
    let mut rig = box_and_sphere();
    rig.st().create_sphere(1.0).unwrap();
    rig.settle();
    assert_eq!(rig.undo_len(), 3);
    let repeat = |k: Key| Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: true,
        modifiers: Modifiers::COMMAND,
    };
    rig.frame(vec![key(Key::Z, Modifiers::COMMAND), repeat(Key::Z)]);
    assert_eq!(
        (rig.undo_len(), rig.redo_len()),
        (2, 1),
        "ein Schritt im Bild"
    );
    assert!(!rig.left_over, "alle Ereignisse verbraucht");
    rig.frame(vec![repeat(Key::Z), repeat(Key::Z)]);
    assert_eq!(
        (rig.undo_len(), rig.redo_len()),
        (1, 2),
        "nächstes Bild, nächster Schritt"
    );
    rig.frame(vec![repeat(Key::Y), repeat(Key::Y)]);
    assert_eq!((rig.undo_len(), rig.redo_len()), (2, 1));
}

/// Stehen Meldung und Hinweis zugleich (im Bild, in dem beide gesetzt wurden), gewinnt die
/// Meldung in der Fehlerfarbe.
#[test]
fn message_wins_over_info() {
    let mut rig = box_and_sphere();
    rig.st().info = Some("Rückgängig: Kugel anlegen".into());
    rig.st().message = Some("Etwas ging schief".into());
    let seen = rig.frame(vec![]);
    assert_eq!(
        status_color(&seen, "Etwas ging schief"),
        theme::palette(true).error
    );
    assert!(!seen.iter().any(|s| s.text.starts_with("Rückgängig")));
}

/// Landet Rückgängig auf einem Stand mit einem fehlerhaften Feature, zeigt die Statuszeile
/// dessen Fehler in der Fehlerfarbe statt des Hinweises „Rückgängig: …“.
#[test]
fn undo_onto_a_failing_feature_shows_the_error() {
    let mut w = ws();
    let id = w
        .active_tab_mut()
        .state
        .add_feature(
            None,
            FeatureKind::Cylinder {
                radius: -1.0,
                height: 1.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    w.observe_history(false);
    w.active_tab_mut().state.message = None;
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let name = rig.st().document.feature(id).unwrap().name.clone();
    rig.undo_key();
    assert!(matches!(
        rig.st().document.feature(id).unwrap().status,
        FeatureStatus::Error(_)
    ));
    assert_eq!(rig.bodies(), 0, "der Quader ist zurückgenommen");
    assert_eq!(rig.info(), None, "kein Hinweis über dem Fehler");
    let seen = rig.settle();
    let error = seen
        .iter()
        .find(|s| s.text.starts_with(&format!("„{name}“")))
        .unwrap_or_else(|| panic!("Fehler von „{name}“ nicht in der Statuszeile"));
    assert_eq!(error.color, theme::palette(true).error);
    assert!(!seen.iter().any(|s| s.text.starts_with("Rückgängig")));
    // Wiederholen auf einen Stand, in dem der Fehler weiter besteht: ebenso.
    rig.press(Key::Y, Modifiers::COMMAND);
    assert_eq!(rig.bodies(), 1);
    assert_eq!(rig.info(), None);
    let seen = rig.settle();
    assert!(
        seen.iter()
            .any(|s| s.text.starts_with(&format!("„{name}“")))
    );
}

/// In der offenen Skizze steht der Hinweis auch dann, wenn das Dokument ein fehlerhaftes Feature
/// hat (dort kommt die Skizzenzeile vor Feature-Fehlern), und eine ältere Meldung („nichts zu
/// wiederholen“) verdrängt ihn nicht.
#[test]
fn sketch_hint_despite_failing_feature_and_old_message() {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .add_feature(
            None,
            FeatureKind::Cylinder {
                radius: -1.0,
                height: 1.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    w.observe_history(false);
    let id = w
        .active_tab_mut()
        .state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    w.active_tab_mut().state.open_sketch(id).unwrap();
    w.observe_history(false);
    w.active_tab_mut().state.message = None;
    let mut rig = Rig::new(w);
    let name = rig.st().session_sketch().unwrap().name.clone();
    {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    for (x, y) in [(0.0, 0.0), (10.0, 4.0)] {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.click(sk, DVec2::new(x, y), 0.5, false).unwrap();
        rig.frame(vec![]);
    }
    rig.settle();
    rig.press(Key::Y, Modifiers::COMMAND);
    assert_eq!(rig.message().as_deref(), Some(NOTHING_TO_REDO));
    rig.settle();
    rig.undo_key();
    assert_eq!(rig.message(), None, "die alte Meldung ist weg");
    assert_eq!(rig.info().as_deref(), Some("Rückgängig: Linie zeichnen"));
    let seen = rig.settle();
    assert_eq!(
        status_color(
            &seen,
            &format!("Skizzen-Modus: {name} · Rückgängig: Linie zeichnen")
        ),
        theme::palette(true).text_secondary
    );
}
