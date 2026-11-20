//! Rückfrage „Löschen mit Folgen“ im Feature-Baum nach dem Ablauf aus der Doku-Runde zu
//! 0.0.2-dev.3: Körper mit davon abhängiger Skizze auf seiner Fläche, Entf, die Rückfrage steht
//! Stunden offen, dann ein Rechtsklick in den Baum und „Abbrechen“. Das Programm muss dabei
//! weiterlaufen, nichts löschen und die Rückfrage schließen. Die ganze App ohne Fenster.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::app::PlanoraApp;
use planora_app::browser::request_delete;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_feature::{FeatureId, SketchPlane};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;

/// Sechs Stunden und vierzig Minuten in Sekunden, so lange stand die Rückfrage offen.
const IDLE_S: f64 = 6.0 * 3600.0 + 40.0 * 60.0;

struct Seen {
    text: String,
    rect: Rect,
}

fn texts(out: &egui::FullOutput) -> Vec<Seen> {
    fn walk(shape: &Shape, v: &mut Vec<Seen>) {
        match shape {
            Shape::Vec(s) => s.iter().for_each(|x| walk(x, v)),
            Shape::Text(t) => v.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
            }),
            _ => {}
        }
    }
    let mut v = Vec::new();
    for c in &out.shapes {
        walk(&c.shape, &mut v);
    }
    v
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    body: FeatureId,
}

impl Rig {
    fn new(prefix: &str) -> Rig {
        let root = common::temp_root(prefix);
        let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        let cfg = common::temp_root(&format!("{prefix}-cfg"));
        std::fs::create_dir_all(&cfg).unwrap();
        ws.config_path = cfg.join("config.json");
        let project = ws.library.as_ref().unwrap().create_project("P").unwrap();
        let st = &mut ws.active_tab_mut().state;
        let body = st.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
        for _ in 0..3 {
            st.add_sketch(SketchPlane::Standard(StandardPlane::XY))
                .unwrap();
        }
        let face = top_face(st);
        st.new_sketch_on_face(0, face).unwrap();
        st.close_sketch();
        ws.save_as(&project, "Winkelhalterung", "Stand").unwrap();
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        Rig {
            ctx,
            app: PlanoraApp::for_tests(ws),
            time: 0.0,
            body,
        }
    }

    fn state(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let seen = texts(&out);
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self, n: usize) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![]);
        }
        seen
    }

    fn click(&mut self, at: Pos2, button: PointerButton) {
        let ev = |pressed| Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![ev(true)]);
        self.frame(vec![ev(false)]);
        self.settle(3);
    }

    /// Mitte des untersten sichtbaren Textes `text`.
    fn at(&mut self, text: &str) -> Pos2 {
        let seen = self.settle(3);
        seen.iter()
            .filter(|s| s.text == text)
            .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .map(|s| s.rect.center())
            .unwrap_or_else(|| {
                let names: Vec<&str> = seen.iter().map(|s| s.text.as_str()).collect();
                panic!("„{text}“ nicht sichtbar: {names:?}")
            })
    }

    /// Die Rückfrage öffnen und dann die lange Pause: Die Uhr springt, die App zeichnet weiter.
    fn open_prompt_and_wait(&mut self) {
        let body = self.body;
        request_delete(self.state(), body);
        assert!(self.state().pending_delete.is_some(), "Rückfrage offen");
        self.settle(4);
        self.time += IDLE_S;
        self.settle(4);
    }
}

fn top_face(s: &AppState) -> planora_kernel::Id<planora_kernel::Face> {
    let solid = &s.bodies[0].solid;
    let top_z = s.bodies[0].bounds.1.z;
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

#[test]
fn right_click_in_the_tree_then_cancel_keeps_everything() {
    let mut r = Rig::new("planora-loeschen-pause");
    let before = r.state().document.features().len();
    r.open_prompt_and_wait();
    // Rechtsklick auf die Baumzeile der Skizze auf der Fläche, während die Rückfrage offen ist.
    let row = r.at("Skizze 4");
    r.click(row, PointerButton::Secondary);
    assert!(r.state().pending_delete.is_some(), "Rückfrage bleibt");
    // Ein offenes Kontextmenü schließt der nächste Klick; dann „Abbrechen“.
    let cancel = r.at(&planora_i18n::tr!("common-cancel"));
    r.click(cancel, PointerButton::Primary);
    if r.state().pending_delete.is_some() {
        let cancel = r.at(&planora_i18n::tr!("common-cancel"));
        r.click(cancel, PointerButton::Primary);
    }
    r.settle(10);
    assert!(r.state().pending_delete.is_none(), "Rückfrage zu");
    assert_eq!(
        r.state().document.features().len(),
        before,
        "nichts gelöscht"
    );
}

#[test]
fn delete_after_the_pause_still_works() {
    let mut r = Rig::new("planora-loeschen-pause-ok");
    let before = r.state().document.features().len();
    r.open_prompt_and_wait();
    let delete = r.at(&planora_i18n::tr!("tree-delete"));
    r.click(delete, PointerButton::Primary);
    r.settle(10);
    assert!(r.state().pending_delete.is_none(), "Rückfrage zu");
    assert!(
        r.state().document.features().len() < before,
        "Körper und Skizze weg"
    );
}
