//! Rückgängig und Wiederholen je Tab (Aufgabe 6): Jede Aktion ist ein Schritt, erkannt am Ende
//! eines Bildes (R3); Ziehen und Tippen sind je ein Schritt (R4); der Punkt „ungespeichert“ folgt
//! dem Verlauf (R7); Autosicherung und Wiederherstellen beginnen einen neuen Verlauf; Fenster
//! schließen nach dem Zurückladen; ein schreibender Export sperrt (R8).
//!
//! Teils über `Workspace` direkt (`observe_history` wie am Bildende), teils mit echten Bildern
//! der ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`).

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::blend_dialog::BlendDialog;
use planora_app::boolean_dialog::BooleanDialog;
use planora_app::browser::delete_plan;
use planora_app::create_dialog::CreateDialog;
use planora_app::dimension_browser::DimDeletePlan;
use planora_app::dimension_dialog::DimensionDialog;
use planora_app::drawing_browser::{DrawingDeletePlan, READ_ONLY_HINT};
use planora_app::drawing_dialog::NewDrawingDialog;
use planora_app::export_dialog::{ExportDialog, ExportFormat};
use planora_app::feature_dialog::FeatureDialog;
use planora_app::history::{History, snapshot_bytes};
use planora_app::import_dialog::ImportDialog;
use planora_app::properties::POSITION_LABEL;
use planora_app::state::*;
use planora_app::workspace::*;
use planora_app::{drawing_view, fonts, sheet_dialog, tabs, theme, viewport};
use planora_drawing::dimension::{DimId, DimKind, DimSpec, Tolerance as DimTolerance};
use planora_drawing::sheet::SheetModel;
use planora_drawing::{
    DrawError, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId,
    ViewKind,
};
use planora_exchange::Progress;
use planora_feature::{BlendArt, FeatureKind, Snapshot, StandardPlane};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Face, Id, Solid};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::convert::encode_version;
use planora_project::docstore::LockMode;
use planora_project::library::{Library, NodePath};
use std::path::Path;
use std::time::{Duration, Instant};

// ---- Hilfen ---------------------------------------------------------------------------------

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

fn ws() -> Workspace {
    let root = common::temp_root("planora-undo-flow");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn st(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
}

fn tab(w: &mut Workspace) -> &mut DocTab {
    w.active_tab_mut()
}

fn undo_len(w: &Workspace) -> usize {
    w.active_tab().history().undo_len()
}

fn redo_len(w: &Workspace) -> usize {
    w.active_tab().history().redo_len()
}

/// Wie am Ende eines Bildes ohne Geste.
fn observe(w: &mut Workspace) {
    w.observe_history(false);
}

fn text(s: &Snapshot) -> String {
    encode_version(s).expect("Versionsdatei")
}

/// Gleicher Inhalt und gleicher Text der Versionsdatei (byte-gleich).
fn assert_same(a: &Snapshot, b: &Snapshot, what: &str) {
    assert!(a.same_content(b), "{what}: Inhalt verschieden");
    assert_eq!(text(a), text(b), "{what}: Versionsdatei verschieden");
}

fn bounds(w: &Workspace, i: usize) -> (DVec3, DVec3) {
    w.active_tab().state.bodies[i].bounds
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn top_face(s: &AppState, i: usize) -> Id<Face> {
    let solid = &s.bodies[i].solid;
    let top_z = s.bodies[i].bounds.1.z;
    solid
        .faces
        .iter()
        .find(|(id, _)| {
            planora_sketch::plane_of_face(solid, *id)
                .is_some_and(|f| f.z.z > 0.5 && (f.origin.z - top_z).abs() < 1e-9)
        })
        .map(|(id, _)| id)
        .expect("Deckfläche")
}

/// Zeichnung „Blatt 1“ mit Vorderansicht 1:1 in der Blattmitte.
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

fn dim_spec(view: ViewId) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind: DimKind::Horizontal,
        refs: Vec::new(),
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

fn placement(s: &AppState, d: DrawingId, v: ViewId) -> Placement {
    s.document.drawing(d).unwrap().view(v).unwrap().placement
}

/// Projekt „P“ (einmal angelegt) und Speichern des aktiven Tabs als `name`.
fn save_as(w: &mut Workspace, name: &str) -> NodePath {
    let p = w
        .library
        .as_ref()
        .unwrap()
        .create_project("P")
        .unwrap_or_else(|_| vec!["P".to_string()]);
    w.save_as(&p, name, "").unwrap()
}

// ---- (a) Jede Aktion ein Schritt -------------------------------------------------------------

#[test]
fn each_action_is_one_step() {
    let mut w = ws();
    let start = st(&mut w).snapshot();
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    observe(&mut w);
    st(&mut w).move_body(0, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    observe(&mut w);
    let id = st(&mut w).naming_feature(0).unwrap();
    st(&mut w).rename_feature(id, "Teil").unwrap();
    observe(&mut w);
    let end = st(&mut w).snapshot();
    assert_eq!(undo_len(&w), 3);
    assert_eq!(tab(&mut w).redo_label(), None);

    for label in ["Umbenennen", "Verschieben", "Quader anlegen"] {
        assert_eq!(tab(&mut w).undo_label(), Some(label.to_string()));
        tab(&mut w).undo().unwrap();
        // Rückgängig selbst ist kein Schritt.
        observe(&mut w);
        assert_eq!(tab(&mut w).redo_label(), Some(label.to_string()));
    }
    assert_eq!(undo_len(&w), 0);
    assert_eq!(redo_len(&w), 3);
    assert_same(&st(&mut w).snapshot(), &start, "nach dreimal Rückgängig");
    assert!(st(&mut w).bodies.is_empty());
    assert_eq!(tab(&mut w).undo(), Err(nothing_to_undo()));

    for label in ["Quader anlegen", "Verschieben", "Umbenennen"] {
        assert_eq!(tab(&mut w).redo_label(), Some(label.to_string()));
        tab(&mut w).redo().unwrap();
        observe(&mut w);
    }
    assert_same(&st(&mut w).snapshot(), &end, "nach dreimal Wiederholen");
    assert_eq!(undo_len(&w), 3);
    assert_eq!(redo_len(&w), 0);
    assert_eq!(tab(&mut w).redo(), Err(nothing_to_redo()));
    assert_eq!(st(&mut w).bodies[0].name, "Teil");
}

/// Mehrere Rückgängig hintereinander ohne Bild dazwischen: kein Schritt geht verloren, keiner
/// entsteht neu.
#[test]
fn undo_twice_without_frames_between() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    observe(&mut w);
    let one = st(&mut w).snapshot();
    st(&mut w).create_sphere(4.0).unwrap();
    // ohne Bild: Rückgängig übernimmt die Kugel zuerst als Schritt und nimmt sie zurück
    tab(&mut w).undo().unwrap();
    assert_same(&st(&mut w).snapshot(), &one, "Kugel zurück");
    tab(&mut w).undo().unwrap();
    assert!(st(&mut w).bodies.is_empty());
    tab(&mut w).redo().unwrap();
    tab(&mut w).redo().unwrap();
    assert_eq!(st(&mut w).bodies.len(), 2);
    assert_eq!(undo_len(&w), 2);
}

// ---- (b) Keine Schritte ohne Änderung -------------------------------------------------------

#[test]
fn no_step_without_change() {
    let mut w = ws();
    for _ in 0..5 {
        observe(&mut w);
    }
    assert_eq!(undo_len(&w), 0);
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    observe(&mut w);
    assert_eq!(undo_len(&w), 1);

    // Kamera, Auswahl, Export-Fenster
    st(&mut w).camera.yaw += 0.7;
    st(&mut w).camera.distance *= 2.0;
    observe(&mut w);
    st(&mut w).selected_body = None;
    st(&mut w).set_selection(None);
    observe(&mut w);
    st(&mut w).selected_body = Some(0);
    observe(&mut w);
    let export = ExportDialog::new(&w.active_tab().state);
    tab(&mut w).export_dialog = Some(export);
    observe(&mut w);
    tab(&mut w).export_dialog = None;
    observe(&mut w);
    assert_eq!(undo_len(&w), 1);

    // Abgelehntes Verschieben: läge außerhalb des darstellbaren Bereichs.
    assert!(
        st(&mut w)
            .move_body(0, DVec3::new(2.0 * MAX_COORD_MM, 0.0, 0.0))
            .is_err()
    );
    observe(&mut w);
    assert_eq!(undo_len(&w), 1);
    assert_eq!(
        tab(&mut w).undo_label(),
        Some("Quader anlegen".to_string()),
        "kein offener Schritt"
    );

    // Neue Zeichnung, deren Ansicht nicht gelingt: angelegt und wieder entfernt.
    let edits = st(&mut w).document.edit_count();
    let mut dialog = NewDrawingDialog::new(&w.active_tab().state);
    dialog.scale = Scale { num: 0, den: 1 };
    assert!(dialog.create(st(&mut w)).is_err());
    assert_ne!(
        st(&mut w).document.edit_count(),
        edits,
        "das Dokument wurde angefasst"
    );
    observe(&mut w);
    assert_eq!(undo_len(&w), 1, "kein Schritt für die verworfene Zeichnung");
    assert_eq!(tab(&mut w).undo_label(), Some("Quader anlegen".to_string()));
    // Die nächste Zeichnung bekommt dieselbe Kennung wie ohne den Fehlversuch.
    let d = st(&mut w)
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    assert_eq!(d, DrawingId(1));
}

/// Ein Dokument, dessen Bild nie sich selbst gleicht (eine Zahl „keine Zahl“): ohne Änderung
/// entstehen trotzdem keine Schritte, auch nicht über viele Bilder.
#[test]
fn nan_document_makes_no_phantom_steps() {
    let mut w = ws();
    st(&mut w)
        .document
        .add_feature(
            "Kaputt",
            FeatureKind::Box {
                size: DVec3::new(f64::NAN, 1.0, 1.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let s = st(&mut w).snapshot();
    assert!(
        !s.same_content(&s),
        "Voraussetzung: Bild ungleich mit sich selbst"
    );
    observe(&mut w);
    assert_eq!(undo_len(&w), 1);
    for _ in 0..50 {
        w.observe_history(false);
        w.observe_history(true);
    }
    assert_eq!(undo_len(&w), 1);
    // Rückgängig nimmt genau diesen Schritt zurück, ohne neuen.
    tab(&mut w).undo().unwrap();
    for _ in 0..10 {
        observe(&mut w);
    }
    assert_eq!(undo_len(&w), 0);
    assert_eq!(redo_len(&w), 1);
    assert!(st(&mut w).document.features().is_empty());
}

// ---- (c) Löschen mit Folgen -----------------------------------------------------------------

#[test]
fn delete_with_dependents_is_one_step() {
    let mut w = ws();
    let quader = st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    observe(&mut w);
    st(&mut w).move_body(0, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    observe(&mut w);
    let e = edge_between(&st(&mut w).bodies[0].solid, "Deckel", "Wand 1");
    st(&mut w)
        .add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
        .unwrap();
    observe(&mut w);
    let face = top_face(st(&mut w), 0);
    st(&mut w).new_sketch_on_face(0, face).unwrap();
    observe(&mut w);
    assert_eq!(undo_len(&w), 4);
    let before = st(&mut w).snapshot();

    st(&mut w).delete_feature_with_dependents(quader).unwrap();
    assert!(st(&mut w).document.features().is_empty());
    observe(&mut w);
    assert_eq!(undo_len(&w), 5);
    assert_eq!(
        tab(&mut w).undo_label(),
        Some("Feature löschen".to_string())
    );

    tab(&mut w).undo().unwrap();
    assert_same(&st(&mut w).snapshot(), &before, "alles zurück");
    assert_eq!(st(&mut w).bodies.len(), 1);
    assert_eq!(tab(&mut w).undo_label(), Some("Skizze anlegen".to_string()));
    tab(&mut w).redo().unwrap();
    assert!(st(&mut w).document.features().is_empty());
    tab(&mut w).undo().unwrap();
    tab(&mut w).undo().unwrap();
    assert_eq!(tab(&mut w).undo_label(), Some("Verrunden".to_string()));
}

// ---- (d) Ziehen ist ein Schritt -------------------------------------------------------------

#[test]
fn drag_is_one_step() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    observe(&mut w);
    let n = undo_len(&w);
    let before = bounds(&w, 0);
    for _ in 0..30 {
        st(&mut w)
            .move_body_dragging(0, DVec3::new(0.5, -0.25, 0.0))
            .unwrap();
        w.observe_history(true);
        assert_eq!(undo_len(&w), n, "während des Ziehens kein Schritt");
        assert!(tab(&mut w).is_dirty());
        assert_eq!(tab(&mut w).undo_label(), Some("Verschieben".to_string()));
    }
    st(&mut w).end_drag();
    observe(&mut w);
    assert_eq!(undo_len(&w), n + 1);
    assert_eq!(tab(&mut w).undo_label(), Some("Verschieben".to_string()));
    let after = bounds(&w, 0);
    assert_eq!(after.0 - before.0, DVec3::new(15.0, -7.5, 0.0));
    tab(&mut w).undo().unwrap();
    assert_eq!(bounds(&w, 0), before, "Lage vor dem Ziehen exakt");
    tab(&mut w).redo().unwrap();
    assert_eq!(bounds(&w, 0), after, "Endlage exakt");
}

// ---- (e) Ziehen und Tippen in echten Bildern ------------------------------------------------

const SIZE: Vec2 = vec2(1400.0, 900.0);

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        }
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
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
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
            match shape {
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                egui::epaint::Shape::Text(t) => out.push(Seen {
                    text: t.galley.text().to_string(),
                    rect: Rect::from_min_size(t.pos, t.galley.size()),
                }),
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

    fn undo_len(&mut self) -> usize {
        self.ws().active_tab().history().undo_len()
    }
}

fn press(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn key(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Mitte des Positionsfelds X (das linke Zahlenfeld unter der Überschrift „Position“).
fn position_x_field(seen: &[Seen]) -> Pos2 {
    let label = seen
        .iter()
        .find(|s| s.text == POSITION_LABEL)
        .unwrap_or_else(|| panic!("„{POSITION_LABEL}“ fehlt"))
        .rect;
    let field = seen
        .iter()
        .filter(|s| {
            s.rect.top() >= label.bottom() - 0.5
                && s.rect.top() < label.bottom() + 40.0
                && s.text.parse::<f64>().is_ok()
        })
        .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
        .unwrap_or_else(|| panic!("kein Zahlenfeld unter „{POSITION_LABEL}“: {seen:?}"));
    field.rect.center()
}

fn app_with_box() -> Rig {
    let mut w = ws();
    st(&mut w).create_box(DVec3::new(30.0, 20.0, 10.0)).unwrap();
    st(&mut w).selected_body = Some(0);
    let mut rig = Rig::new(w);
    rig.settle();
    rig
}

#[test]
fn drag_in_real_frames() {
    let mut rig = app_with_box();
    assert_eq!(rig.undo_len(), 1, "der Quader ist schon ein Schritt");
    let seen = rig.settle();
    let p = position_x_field(&seen);
    let before = rig.ws().active_tab().state.bodies[0].bounds.0;
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![press(p, true)]);
    for i in 1..=20 {
        rig.frame(vec![Event::PointerMoved(p + vec2(3.0 * i as f32, 0.0))]);
        assert_eq!(
            rig.undo_len(),
            1,
            "während des Ziehens (Bild {i}) kein Schritt"
        );
    }
    let moved = rig.ws().active_tab().state.bodies[0].bounds.0;
    assert!(moved.x > before.x + 5.0, "gezogen: {before:?} → {moved:?}");
    assert!(rig.ws().active_tab().is_dirty());
    rig.frame(vec![press(p + vec2(60.0, 0.0), false)]);
    rig.settle();
    assert_eq!(rig.undo_len(), 2, "ein Schritt für das ganze Ziehen");
    assert_eq!(
        rig.ws().active_tab().undo_label(),
        Some("Verschieben".to_string())
    );
    let end = rig.ws().active_tab().state.bodies[0].bounds.0;
    rig.ws().active_tab_mut().undo().unwrap();
    rig.settle();
    assert_eq!(rig.ws().active_tab().state.bodies[0].bounds.0, before);
    assert_eq!(rig.undo_len(), 1, "Rückgängig ist kein neuer Schritt");
    rig.ws().active_tab_mut().redo().unwrap();
    rig.settle();
    assert_eq!(rig.ws().active_tab().state.bodies[0].bounds.0, end);
    assert_eq!(rig.undo_len(), 2);
}

#[test]
fn typing_a_number_is_one_step() {
    let mut rig = app_with_box();
    let seen = rig.settle();
    let p = position_x_field(&seen);
    // Klick ohne Bewegen: das Feld wird zum Eingabefeld.
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![press(p, true)]);
    rig.frame(vec![press(p, false)]);
    rig.frame(vec![]);
    assert!(
        viewport::typing(&rig.ctx),
        "das Positionsfeld nimmt Tasten an"
    );
    // Alles markieren und überschreiben, Zeichen für Zeichen in eigenen Bildern.
    rig.frame(vec![Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    for (i, c) in ["4", "2", ".", "5"].iter().enumerate() {
        rig.frame(vec![Event::Text((*c).to_string())]);
        assert_eq!(
            rig.undo_len(),
            1,
            "während des Tippens (Zeichen {i}) kein Schritt"
        );
    }
    let x = rig.ws().active_tab().state.bodies[0].bounds.0.x;
    rig.frame(vec![key(Key::Enter)]);
    rig.settle();
    let x_end = rig.ws().active_tab().state.bodies[0].bounds.0.x;
    assert!((x_end - 42.5).abs() < 1e-9, "x = {x_end} (vorher {x})");
    assert_eq!(rig.undo_len(), 2, "ein Schritt für die ganze Eingabe");
    assert_eq!(
        rig.ws().active_tab().undo_label(),
        Some("Verschieben".to_string())
    );
    rig.ws().active_tab_mut().undo().unwrap();
    rig.settle();
    assert_eq!(rig.ws().active_tab().state.bodies[0].bounds.0.x, 0.0);
}

#[test]
fn view_drag_in_real_frames() {
    let mut w = ws();
    st(&mut w)
        .create_box(DVec3::new(100.0, 60.0, 40.0))
        .unwrap();
    let (d, v) = drawing_with_front(st(&mut w));
    st(&mut w).drawing_ui.open(d);
    {
        let s = st(&mut w);
        s.drawing_ui.tick(&s.bodies, &s.document, true);
    }
    let mut rig = Rig::new(w);
    rig.settle();
    rig.ws().observe_history(false);
    let n = rig.undo_len();
    let before = placement(&rig.ws().active_tab().state, d, v);
    let l = drawing_view::last_layout(&rig.ctx).expect("Zeichnungsbereich");
    let p = drawing_view::sheet_to_screen(
        l.canvas,
        l.sheet_mm,
        l.zoom,
        l.pan,
        DVec2::new(148.5, 105.0),
    );
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![press(p, true)]);
    for i in 1..=12 {
        rig.frame(vec![Event::PointerMoved(
            p + vec2(4.0 * i as f32, 2.0 * i as f32),
        )]);
        assert_eq!(
            rig.undo_len(),
            n,
            "während des Ziehens (Bild {i}) kein Schritt"
        );
    }
    let moved = placement(&rig.ws().active_tab().state, d, v);
    assert_ne!(moved, before, "die Ansicht ist gewandert");
    rig.frame(vec![press(p + vec2(48.0, 24.0), false)]);
    rig.settle();
    assert_eq!(rig.undo_len(), n + 1, "ein Schritt für das ganze Ziehen");
    assert_eq!(
        rig.ws().active_tab().undo_label(),
        Some("Ansicht verschieben".to_string())
    );
    let end = placement(&rig.ws().active_tab().state, d, v);
    rig.ws().active_tab_mut().undo().unwrap();
    rig.settle();
    assert_eq!(placement(&rig.ws().active_tab().state, d, v), before);
    rig.ws().active_tab_mut().redo().unwrap();
    rig.settle();
    assert_eq!(placement(&rig.ws().active_tab().state, d, v), end);
    assert_eq!(rig.undo_len(), n + 1);
}

// ---- (f) Punkt „ungespeichert“ --------------------------------------------------------------

#[test]
fn dirty_dot_follows_history() {
    let mut w = ws();
    assert!(!w.active_tab().is_dirty(), "neuer Tab sauber");
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    assert!(
        w.active_tab().is_dirty(),
        "schmutzig schon vor dem Bildende"
    );
    observe(&mut w);
    assert!(w.active_tab().is_dirty());
    st(&mut w).create_cylinder(3.0, 6.0).unwrap();
    // Speichern übernimmt die noch offene Änderung als Schritt.
    save_as(&mut w, "Teil");
    assert!(!w.active_tab().is_dirty(), "gespeichert");
    assert_eq!(undo_len(&w), 2, "Verlauf bleibt nach dem Speichern");
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Zylinder anlegen".to_string())
    );

    tab(&mut w).undo().unwrap();
    assert!(w.active_tab().is_dirty(), "vom gespeicherten Stand weg");
    tab(&mut w).redo().unwrap();
    assert!(
        !w.active_tab().is_dirty(),
        "zurück auf dem gespeicherten Stand"
    );

    // Speichern mit `save_active` behält den Verlauf ebenso.
    st(&mut w).create_sphere(2.0).unwrap();
    observe(&mut w);
    assert!(matches!(w.save_active("", None), Ok(SaveOutcome::Saved(2))));
    assert_eq!(undo_len(&w), 3);
    assert!(!w.active_tab().is_dirty());
    tab(&mut w).undo().unwrap();
    assert!(w.active_tab().is_dirty());
    tab(&mut w).undo().unwrap();
    assert!(w.active_tab().is_dirty());
    tab(&mut w).redo().unwrap();
    tab(&mut w).redo().unwrap();
    assert!(!w.active_tab().is_dirty());

    // Rückgängig und eine neue Aktion: der gespeicherte Stand ist nicht mehr erreichbar.
    tab(&mut w).undo().unwrap();
    st(&mut w).create_cone(2.0, 3.0).unwrap();
    observe(&mut w);
    assert_eq!(redo_len(&w), 0);
    assert!(w.active_tab().is_dirty());
    for _ in 0..4 {
        let _ = tab(&mut w).undo();
        assert!(w.active_tab().is_dirty(), "beim Zurückgehen");
    }
    for _ in 0..4 {
        let _ = tab(&mut w).redo();
        assert!(w.active_tab().is_dirty(), "beim Wiedergehen");
    }

    // Offene Skizze: immer ungespeichert.
    let sk = st(&mut w)
        .add_sketch(planora_feature::SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    w.save_active("", None).unwrap();
    assert!(!w.active_tab().is_dirty());
    let n = undo_len(&w);
    st(&mut w).open_sketch(sk).unwrap();
    observe(&mut w);
    assert!(w.active_tab().is_dirty(), "Skizze offen");
    assert_eq!(undo_len(&w), n, "Öffnen ist kein Schritt");
    // Unverändert geschlossen: kein Schritt, wieder sauber.
    st(&mut w).close_sketch();
    observe(&mut w);
    assert_eq!(undo_len(&w), n, "unverändertes Schließen ist kein Schritt");
    assert!(!w.active_tab().is_dirty());
}

/// Vom Grenzwert verdrängter gespeicherter Schritt: ungespeichert bis zum nächsten Speichern.
#[test]
fn evicted_saved_step_stays_dirty() {
    let mut w = ws();
    save_as(&mut w, "Teil");
    let start = st(&mut w).snapshot();
    // Kleiner Verlauf: höchstens zwei Schritte.
    *tab(&mut w).history_mut() =
        History::with_limits(start.clone(), snapshot_bytes(&start), 2, usize::MAX);
    tab(&mut w).history_mut().mark_saved();
    assert!(!w.active_tab().is_dirty());
    for _ in 0..3 {
        st(&mut w).create_box(DVec3::splat(1.0)).unwrap();
        observe(&mut w);
    }
    assert_eq!(undo_len(&w), 2);
    while tab(&mut w).undo().is_ok() {}
    assert_eq!(st(&mut w).bodies.len(), 1, "der Anfang ist verdrängt");
    assert!(w.active_tab().is_dirty());
    w.save_active("", None).unwrap();
    assert!(!w.active_tab().is_dirty());
}

// ---- (g) Punkt in der Tableiste -------------------------------------------------------------

fn tab_strip(c: &egui::Context, w: &mut Workspace) -> String {
    let out = c.run_ui(egui::RawInput::default(), |ui| tabs::show(ui, w));
    let t = planora_app::testing::collect_texts(&out);
    out.drop_without_applying_deltas();
    t
}

#[test]
fn dirty_dot_in_tab_strip() {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    let mut w = ws();
    let t = tab_strip(&c, &mut w);
    assert!(t.contains("Unbenannt 1") && !t.contains('●'), "{t}");
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    // vor dem Bildende schon mit Punkt
    let t = tab_strip(&c, &mut w);
    assert!(t.contains("Unbenannt 1 ●"), "{t}");
    observe(&mut w);
    save_as(&mut w, "Teil");
    let t = tab_strip(&c, &mut w);
    assert!(t.contains("Teil") && !t.contains('●'), "{t}");
    tab(&mut w).undo().unwrap();
    let t = tab_strip(&c, &mut w);
    assert!(t.contains("Teil ●"), "{t}");
    tab(&mut w).redo().unwrap();
    let t = tab_strip(&c, &mut w);
    assert!(t.contains("Teil") && !t.contains('●'), "{t}");
}

/// Dasselbe über die ganze App: Rückgängig zwischen den Bildern, der Punkt folgt.
#[test]
fn dirty_dot_in_real_frames() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    save_as(&mut w, "Teil");
    let mut rig = Rig::new(w);
    let has_dot = |seen: &[Seen]| seen.iter().any(|s| s.text.contains("Teil ●"));
    let seen = rig.settle();
    assert!(!has_dot(&seen));
    rig.ws().active_tab_mut().undo().unwrap();
    let seen = rig.settle();
    assert!(has_dot(&seen), "nach Rückgängig mit Punkt");
    rig.ws().active_tab_mut().redo().unwrap();
    let seen = rig.settle();
    assert!(!has_dot(&seen), "nach Wiederholen ohne Punkt");
}

// ---- (h) Getrennte Verläufe -----------------------------------------------------------------

#[test]
fn tabs_have_own_history() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    let second = w.new_tab();
    st(&mut w).create_sphere(3.0).unwrap();
    w.observe_history(false);
    st(&mut w).create_sphere(4.0).unwrap();
    w.observe_history(false);
    // Auch der nicht aktive Tab wird beobachtet.
    w.activate(0);
    w.tabs[1].state.create_cone(1.0, 2.0).unwrap();
    w.observe_history(false);
    assert_eq!(w.tabs[0].history().undo_len(), 1);
    assert_eq!(w.tabs[1].history().undo_len(), 3);
    w.activate(second);
    tab(&mut w).undo().unwrap();
    assert_eq!(st(&mut w).bodies.len(), 2, "Kegel weg, beide Kugeln da");
    assert_eq!(w.tabs[0].state.bodies.len(), 1, "anderer Tab unverändert");
    assert_eq!(w.tabs[0].history().undo_len(), 1);
    assert_eq!(w.tabs[0].history().redo_len(), 0);
    // Ein geschlossener und neu geöffneter Tab hat einen neuen Verlauf.
    w.activate(0);
    let path = save_as(&mut w, "Teil");
    assert_eq!(undo_len(&w), 1);
    w.close_tab(0);
    w.open_document(&path, LockMode::Acquire).unwrap();
    assert_eq!(undo_len(&w), 0);
    assert!(!w.active_tab().is_dirty());
}

// ---- (i) Fehler beim Zurückladen ------------------------------------------------------------

#[test]
fn restore_failure_drops_one_step() {
    let mut w = ws();
    let s0 = st(&mut w).snapshot();
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    let mut s1 = st(&mut w).snapshot();
    st(&mut w).create_sphere(3.0).unwrap();
    let s2 = st(&mut w).snapshot();
    st(&mut w).create_cone(2.0, 3.0).unwrap();
    let s3 = st(&mut w).snapshot();
    // Das mittlere Bild ungültig: Marke hinter dem Ende.
    s1.marker = s1.features.len() + 5;
    let mut h = History::new(s0.clone(), snapshot_bytes(&s0));
    h.record(s1.clone(), snapshot_bytes(&s1), "Quader anlegen");
    h.record(s2.clone(), snapshot_bytes(&s2), "Kugel anlegen");
    h.record(s3.clone(), snapshot_bytes(&s3), "Kegel anlegen");
    *tab(&mut w).history_mut() = h;
    observe(&mut w);
    assert_eq!(undo_len(&w), 3);

    tab(&mut w).undo().unwrap();
    assert_same(&st(&mut w).snapshot(), &s2, "erstes Rückgängig");
    assert_eq!(undo_len(&w), 2);
    let edits = st(&mut w).document.edit_count();
    assert!(tab(&mut w).undo().is_err());
    assert_eq!(st(&mut w).message.as_deref(), Some(UNDO_FAILED));
    assert_same(&st(&mut w).snapshot(), &s2, "Dokument unverändert");
    assert_eq!(st(&mut w).document.edit_count(), edits);
    assert_eq!(undo_len(&w), 1, "nur der kaputte Schritt fällt weg");
    assert_eq!(redo_len(&w), 1, "Wiederholen bleibt");
    observe(&mut w);
    assert_eq!(undo_len(&w), 1, "der Fehlversuch ist kein Schritt");
    tab(&mut w).undo().unwrap();
    assert_same(&st(&mut w).snapshot(), &s0, "erstes Bild");
    assert_eq!(undo_len(&w), 0);

    // Wiederholen mit kaputtem Bild: gleiches Verhalten, eigene Meldung.
    let mut h = History::new(s0.clone(), snapshot_bytes(&s0));
    h.record(s1.clone(), snapshot_bytes(&s1), "Quader anlegen");
    h.commit_undo();
    *tab(&mut w).history_mut() = h;
    assert!(tab(&mut w).redo().is_err());
    assert_eq!(st(&mut w).message.as_deref(), Some(REDO_FAILED));
    assert_same(&st(&mut w).snapshot(), &s0, "Dokument unverändert");
    assert_eq!(redo_len(&w), 0, "nur der kaputte Schritt fällt weg");
    assert_eq!(undo_len(&w), 0);
}

// ---- (j) Fenster schließen, Export sperrt ---------------------------------------------------

fn slow_writer(_: &Path, _: &SheetModel, _: DrawingFormat, p: &Progress) -> Result<u64, DrawError> {
    loop {
        p.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Öffnet je ein Fenster bzw. eine Rückfrage des Tabs (Nummer `which`); `false`, wenn es die
/// Nummer nicht gibt.
fn open_dialog(t: &mut DocTab, which: usize, d: DrawingId, v: ViewId) -> bool {
    let quader = t.state.document.features()[0].id;
    match which {
        0 => {
            t.dialog = Some(CreateDialog::Box {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            })
        }
        1 => t.feature_dialog = FeatureDialog::new(&t.state, quader),
        2 => t.boolean_dialog = Some(BooleanDialog::new(&t.state)),
        3 => t.blend_dialog = Some(BlendDialog::fillet()),
        4 => t.export_dialog = Some(ExportDialog::new(&t.state)),
        5 => {
            t.import_dialog = Some(ImportDialog::new(std::env::temp_dir()));
        }
        6 => t.drawing_dialog = Some(NewDrawingDialog::new(&t.state)),
        7 => sheet_dialog::open(&mut t.state, d),
        8 => t.state.pending_delete = Some(delete_plan(&t.state, quader)),
        9 => {
            t.state.pending_delete_drawing = Some(DrawingDeletePlan {
                drawing: d,
                view: None,
                text: "Blatt 1".into(),
            })
        }
        10 => {
            t.state.pending_delete_dimension = Some(DimDeletePlan {
                drawing: d,
                dim: DimId(1),
                text: "Maß".into(),
            })
        }
        11 => t.state.rename_draft = Some((quader, "Neu".into())),
        12 => t.state.rename_drawing_draft = Some((d, "Neu".into())),
        13 => t.state.dimension_dialog = Some(DimensionDialog::new(d, &dim_spec(v), false)),
        _ => return false,
    }
    true
}

#[test]
fn undo_closes_dialogs_and_blocks_while_writing() {
    let mut w = ws();
    st(&mut w)
        .create_box(DVec3::new(100.0, 60.0, 40.0))
        .unwrap();
    let (d, v) = drawing_with_front(st(&mut w));
    observe(&mut w);
    st(&mut w).create_sphere(5.0).unwrap();
    observe(&mut w);
    assert!(!w.active_tab().dialog_open());

    // Jedes Fenster für sich zählt als offen und schließt mit `close_dialogs`.
    let mut n = 0;
    while open_dialog(tab(&mut w), n, d, v) {
        assert!(w.active_tab().dialog_open(), "Fenster {n} zählt nicht");
        tab(&mut w).close_dialogs();
        assert!(!w.active_tab().dialog_open(), "Fenster {n} bleibt offen");
        n += 1;
    }
    assert!(n >= 14);

    // Alle zugleich: Rückgängig schließt sie.
    for i in 0..n {
        open_dialog(tab(&mut w), i, d, v);
    }
    assert!(w.active_tab().dialog_open());
    assert_eq!(tab(&mut w).undo_blocked(), None, "Menü und Knöpfe gehen");
    tab(&mut w).undo().unwrap();
    assert!(!w.active_tab().dialog_open());
    assert_eq!(st(&mut w).bodies.len(), 1);
    // Wiederholen schließt sie ebenso.
    open_dialog(tab(&mut w), 1, d, v);
    tab(&mut w).redo().unwrap();
    assert!(!w.active_tab().dialog_open());

    // Export schreibt: gesperrt, nichts ändert sich, das Fenster bleibt.
    {
        let s = &w.active_tab().state;
        let mut e = ExportDialog::new(s);
        e.format = ExportFormat::Drawing(DrawingFormat::Svg);
        e.drawing = Some(d);
        e.tick(s, true);
        e.set_writer(slow_writer);
        let dir = common::temp_root("planora-undo-export");
        std::fs::create_dir_all(&dir).unwrap();
        e.write_to(dir.join("z.svg"), s);
        assert!(e.is_writing_file());
        w.active_tab_mut().export_dialog = Some(e);
    }
    assert_eq!(tab(&mut w).undo_blocked(), Some(export_writing()));
    assert_eq!(tab(&mut w).redo_blocked(), Some(export_writing()));
    let before = st(&mut w).snapshot();
    assert_eq!(tab(&mut w).undo(), Err(export_writing()));
    assert_same(&st(&mut w).snapshot(), &before, "Export sperrt");
    tab(&mut w).close_dialogs();
    assert!(
        w.active_tab().export_dialog.is_some(),
        "schreibender Export bleibt offen"
    );
    let mut e = tab(&mut w).export_dialog.take().unwrap();
    e.cancel_write();
    e.tick(&w.active_tab().state, true);
    assert!(!e.is_writing_file());
    w.active_tab_mut().export_dialog = Some(e);
    assert_eq!(tab(&mut w).undo_blocked(), None);
    tab(&mut w).undo().unwrap();
    assert!(
        w.active_tab().export_dialog.is_none(),
        "fertiger Export schließt"
    );
}

#[test]
fn blocked_texts() {
    let mut w = ws();
    assert_eq!(tab(&mut w).undo_blocked(), Some(nothing_to_undo()));
    assert_eq!(tab(&mut w).redo_blocked(), Some(nothing_to_redo()));
    assert_eq!(nothing_to_undo(), "Es gibt nichts rückgängig zu machen.");
    assert_eq!(nothing_to_redo(), "Es gibt nichts zu wiederholen.");
    assert_eq!(
        export_writing(),
        "Während des Exports ist das nicht möglich."
    );
    // Eine offene Änderung (noch kein Schritt) lässt sich schon zurücknehmen.
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    assert_eq!(tab(&mut w).undo_blocked(), None);
    assert_eq!(tab(&mut w).undo_label(), Some("Quader anlegen".to_string()));
    tab(&mut w).undo().unwrap();
    assert_eq!(tab(&mut w).undo_blocked(), Some(nothing_to_undo()));
    assert_eq!(tab(&mut w).redo_blocked(), None);
    // Eine neue Änderung macht Wiederholen hinfällig, schon vor dem Bildende.
    st(&mut w).create_sphere(1.0).unwrap();
    assert_eq!(tab(&mut w).redo_blocked(), Some(nothing_to_redo()));
    assert_eq!(tab(&mut w).redo_label(), None);
    assert_eq!(tab(&mut w).redo(), Err(nothing_to_redo()));
    assert_eq!(st(&mut w).bodies.len(), 1);

    // Alte Version: gesperrt mit dem Hinweis der alten Version.
    let path = save_as(&mut w, "Teil");
    st(&mut w).create_box(DVec3::splat(3.0)).unwrap();
    w.save_active("", None).unwrap();
    let v = w.open_version(&path, 1).unwrap();
    w.activate(v);
    assert_eq!(
        tab(&mut w).undo_blocked(),
        Some("In einer alten Version lässt sich nichts ändern.".to_string())
    );
    assert_eq!(
        READ_ONLY_HINT,
        "In einer alten Version lässt sich nichts ändern."
    );
    assert_eq!(tab(&mut w).redo_blocked(), Some(READ_ONLY_HINT.to_string()));
    let before = st(&mut w).snapshot();
    assert!(tab(&mut w).undo().is_err());
    assert_same(&st(&mut w).snapshot(), &before, "alte Version unverändert");
}

// ---- (k) Neuer Verlauf nach Autosicherung und Wiederherstellen ------------------------------

#[test]
fn autosave_and_restore_reset_history() {
    let mut w = ws();
    let path = save_as(&mut w, "Teil");
    w.autosave_interval = Duration::ZERO;
    st(&mut w).create_box(DVec3::splat(5.0)).unwrap();
    observe(&mut w);
    w.autosave_tick(Instant::now());
    // „Absturz“: Tab ohne Speichern schließen, neu öffnen, Sicherung übernehmen.
    w.close_tab(w.tab_for_path(&path).unwrap());
    let r = w.open_document(&path, LockMode::Acquire).unwrap();
    assert!(r.autosave_offered);
    let id = w.tabs[r.tab].id;
    w.tabs[r.tab].dialog = Some(CreateDialog::Sphere { radius: 2.0 });
    w.apply_autosave(id);
    let t = &w.tabs[r.tab];
    assert_eq!(t.state.bodies.len(), 1, "Stand der Sicherung");
    assert_eq!(t.history().undo_len(), 0, "neuer Verlauf");
    assert_eq!(t.history().redo_len(), 0);
    assert!(t.is_dirty(), "Sicherung ist nicht gespeichert");
    assert!(!t.dialog_open(), "Fenster des alten Stands geschlossen");
    w.activate(r.tab);
    assert_eq!(tab(&mut w).undo(), Err(nothing_to_undo()));
    assert!(w.active_tab().is_dirty());
    // Eine Änderung und zurück: bleibt ungespeichert.
    st(&mut w).create_sphere(1.0).unwrap();
    tab(&mut w).undo().unwrap();
    assert!(
        w.active_tab().is_dirty(),
        "der Stand der Sicherung ist nicht gespeichert"
    );
    w.save_active("", None).unwrap();
    assert!(!w.active_tab().is_dirty());

    // Wiederherstellen einer Version: neuer Verlauf, sauber.
    st(&mut w).create_box(DVec3::splat(2.0)).unwrap();
    w.save_active("", None).unwrap();
    assert_eq!(undo_len(&w), 1);
    let v = w.open_version(&path, 1).unwrap();
    w.restore_version(v, 1).unwrap();
    let t = w.active_tab();
    assert!(!t.read_only);
    assert!(t.state.bodies.is_empty(), "Stand von v1");
    assert_eq!(t.history().undo_len(), 0);
    assert_eq!(t.history().redo_len(), 0);
    assert!(!t.is_dirty());
    assert_eq!(tab(&mut w).undo(), Err(nothing_to_undo()));
}

// ---- Nachtrag: gewählter Körper, überholte Autosicherung, Grenzen ----------------------------

/// Rückgängig/Wiederholen eines Verschiebens über die echte Oberfläche: Der Körper bleibt
/// gewählt, das Positionsfeld bleibt sichtbar; wird der Körper selbst zurückgenommen, entfällt
/// die Wahl.
#[test]
fn moved_body_stays_selected_through_undo_and_redo() {
    let mut rig = app_with_box();
    let seen = rig.settle();
    let p = position_x_field(&seen);
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![press(p, true)]);
    for i in 1..=10 {
        rig.frame(vec![Event::PointerMoved(p + vec2(3.0 * i as f32, 0.0))]);
    }
    rig.frame(vec![press(p + vec2(30.0, 0.0), false)]);
    rig.settle();
    assert_eq!(rig.undo_len(), 2);
    let shows_position = |seen: &[Seen]| seen.iter().any(|s| s.text == POSITION_LABEL);

    rig.ws().active_tab_mut().undo().unwrap();
    let seen = rig.settle();
    assert_eq!(rig.ws().active_tab().state.selected_body, Some(0));
    assert!(shows_position(&seen), "Positionsfeld nach Rückgängig");
    assert_eq!(rig.ws().active_tab().state.bodies[0].bounds.0.x, 0.0);

    rig.ws().active_tab_mut().redo().unwrap();
    let seen = rig.settle();
    assert_eq!(rig.ws().active_tab().state.selected_body, Some(0));
    assert!(shows_position(&seen), "Positionsfeld nach Wiederholen");

    rig.ws().active_tab_mut().undo().unwrap();
    rig.ws().active_tab_mut().undo().unwrap();
    let seen = rig.settle();
    assert_eq!(rig.ws().active_tab().state.selected_body, None);
    assert!(!shows_position(&seen), "kein Körper, kein Positionsfeld");
}

/// Auch über Verrunden und Boolesche Operationen hinweg bleibt der Körper gewählt; ein
/// anderer Körper wird dabei nicht gewählt.
#[test]
fn selection_follows_blend_and_boolean() {
    let mut w = ws();
    st(&mut w).create_box(DVec3::splat(10.0)).unwrap();
    st(&mut w).create_box(DVec3::splat(4.0)).unwrap();
    observe(&mut w);
    let e = edge_between(&st(&mut w).bodies[0].solid, "Deckel", "Wand 1");
    st(&mut w)
        .add_blend(0, &[e], BlendArt::Rundung { radius: 1.0 }, true)
        .unwrap();
    observe(&mut w);
    let chosen = st(&mut w).selected_body.expect("Ergebnis gewählt");
    let blend_ref = st(&mut w).bodies[chosen].source;
    tab(&mut w).undo().unwrap();
    let s = st(&mut w);
    let i = s.selected_body.expect("bleibt gewählt");
    assert_eq!(
        s.bodies[i].source.feature,
        s.document.features()[0].id,
        "der erste Quader"
    );
    tab(&mut w).redo().unwrap();
    let i = st(&mut w).selected_body.expect("bleibt gewählt");
    assert_eq!(st(&mut w).bodies[i].source, blend_ref);

    st(&mut w)
        .add_boolean(i, 1 - i, planora_kernel::boolean::BoolOp::Union)
        .unwrap();
    observe(&mut w);
    assert_eq!(st(&mut w).bodies.len(), 1);
    tab(&mut w).undo().unwrap();
    let i = st(&mut w).selected_body.expect("Ziel bleibt gewählt");
    assert_eq!(st(&mut w).bodies[i].source, blend_ref);
    tab(&mut w).redo().unwrap();
    assert_eq!(st(&mut w).selected_body, Some(0));
}

fn autosave_file(w: &Workspace, path: &NodePath) -> std::path::PathBuf {
    w.library
        .as_ref()
        .unwrap()
        .document_dir(path)
        .unwrap()
        .join(".autosave.json")
}

/// Rückgängig bis zum gespeicherten Stand macht eine eigene Autosicherung überflüssig: Sie wird
/// gelöscht und beim nächsten Öffnen nicht angeboten; wieder geändert, wird neu gesichert.
#[test]
fn stale_autosave_is_dropped_when_clean_again() {
    let mut w = ws();
    let path = save_as(&mut w, "Teil");
    let file = autosave_file(&w, &path);
    w.autosave_interval = Duration::ZERO;
    st(&mut w).create_box(DVec3::splat(5.0)).unwrap();
    observe(&mut w);
    w.autosave_tick(Instant::now());
    assert!(file.is_file(), "Autosicherung geschrieben");
    tab(&mut w).undo().unwrap();
    assert!(!w.active_tab().is_dirty());
    w.autosave_tick(Instant::now());
    assert!(!file.exists(), "überholte Autosicherung gelöscht");
    // wieder geändert: neue Sicherung
    tab(&mut w).redo().unwrap();
    assert!(w.active_tab().is_dirty());
    w.autosave_tick(Instant::now());
    assert!(file.is_file(), "neue Autosicherung");
    // Rückgängig und sofort schließen (ohne Bild dazwischen): auch dann gelöscht.
    tab(&mut w).undo().unwrap();
    assert!(w.request_close(w.tab_for_path(&path).unwrap()));
    assert!(!file.exists(), "beim Schließen gelöscht");
    let r = w.open_document(&path, LockMode::Acquire).unwrap();
    assert!(!r.autosave_offered, "nichts anzubieten");
    // Eine fremde Sicherung (Angebot offen) bleibt dagegen.
    st(&mut w).create_box(DVec3::splat(5.0)).unwrap();
    w.autosave_tick(Instant::now());
    w.close_tab(w.tab_for_path(&path).unwrap());
    assert!(file.is_file());
    let r = w.open_document(&path, LockMode::Acquire).unwrap();
    assert!(r.autosave_offered);
    w.autosave_tick(Instant::now());
    let i = w.tab_for_path(&path).unwrap();
    w.close_tab(i);
    assert!(file.is_file(), "angebotene Sicherung bleibt");
}

/// Der Verlauf eines Tabs hat die Grenze von 100 Schritten.
#[test]
fn tab_history_keeps_100_steps() {
    let mut w = ws();
    let id = st(&mut w).create_box(DVec3::splat(5.0)).unwrap();
    observe(&mut w);
    for i in 0..101 {
        st(&mut w).rename_feature(id, &format!("Teil {i}")).unwrap();
        observe(&mut w);
    }
    assert_eq!(undo_len(&w), planora_app::history::MAX_STEPS);
    assert_eq!(undo_len(&w), 100);
    while tab(&mut w).undo().is_ok() {}
    assert_eq!(
        st(&mut w).document.features()[0].name,
        "Teil 0",
        "älteste fallen heraus"
    );
    assert!(w.active_tab().history().bytes() <= planora_app::history::MAX_BYTES);
}
