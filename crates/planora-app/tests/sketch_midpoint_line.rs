//! Mittelpunktlinie (Skizzen 11a, Task 8): zweite Art des Linienwerkzeugs. Der erste Klick setzt die
//! Mitte, der zweite ein Ende; die Linie liegt symmetrisch zur Mitte, die getippte Länge ist die
//! ganze Linie. Ohne Fenster.

mod common;

use planora_app::keymap::Action;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{SketchSession, Tool, TriadPlane};
use planora_app::workspace::*;
use planora_feature::{ParamScope, SketchFeature, SketchPlane, StandardPlane};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

/// Fangradius in Skizzeneinheiten (klein: Fang nur, wo der Test ihn will).
const R: f64 = 0.5;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Offene, leere Skizze auf XY, Werkzeug Linie in der Art Mittelpunktlinie.
fn rig() -> Workspace {
    let root = common::temp_root("planora-sketch-midpoint-line");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let s = &mut w.active_tab_mut().state;
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.sketch_snap_grid = false;
    w.observe_history(false);
    let (se, sf, _) = parts(&mut w);
    se.choose_kind(&mut sf.sketch, ToolKind::MidpointLine);
    w
}

fn parts(w: &mut Workspace) -> (&mut SketchSession, &mut SketchFeature, ParamScope) {
    let s = &mut w.active_tab_mut().state;
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    (se, sf, scope)
}

fn commit(w: &mut Workspace, at: DVec2) -> Result<(), String> {
    let (se, sf, scope) = parts(w);
    se.commit_input(sf, at, R, false, &scope)
}

fn type_in(w: &mut Workspace, kind: F, text: &str) {
    let (se, sf, scope) = parts(w);
    se.sync_input(&sf.sketch);
    let i = se
        .input
        .fields
        .iter()
        .position(|f| f.kind == kind)
        .unwrap_or_else(|| panic!("kein Feld {kind:?}"));
    se.type_field(i, text, &scope);
}

fn sk(w: &Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

fn undo_len(w: &mut Workspace) -> usize {
    w.observe_history(false);
    w.active_tab().state.sketch_history().unwrap().undo_len()
}

fn lines(sk: &Sketch) -> Vec<(CurveId, PointId, PointId)> {
    sk.curves()
        .filter_map(|(c, d)| match d.kind {
            CurveKind::Line { a, b } => Some((c, a, b)),
            _ => None,
        })
        .collect()
}

fn only_line(sk: &Sketch) -> (CurveId, PointId, PointId) {
    let all = lines(sk);
    assert_eq!(all.len(), 1, "genau eine Linie");
    all[0]
}

fn midpoints(sk: &Sketch) -> Vec<(PointId, CurveId)> {
    sk.constraints()
        .filter_map(|(_, c)| match c.kind {
            K::Midpoint(p, l) => Some((p, l)),
            _ => None,
        })
        .collect()
}

/// Ein dichter Stand, mit dem sich „bitgleich“ vergleichen lässt.
fn fingerprint(sk: &Sketch) -> (usize, usize, usize, Vec<u64>) {
    (
        sk.points().count(),
        sk.curves().count(),
        sk.constraints().count(),
        sk.variables().iter().map(|v| v.to_bits()).collect(),
    )
}

#[test]
fn two_clicks_make_line_and_mid_point() {
    let mut w = rig();
    let steps = undo_len(&mut w);
    commit(&mut w, v(5.0, 5.0)).unwrap();
    assert_eq!(
        sk(&w).curves().count(),
        0,
        "nach dem ersten Klick keine Kurve"
    );
    commit(&mut w, v(25.0, 12.0)).unwrap();
    let s = sk(&w);
    assert_eq!(s.curves().count(), 1);
    assert_eq!(s.points().count(), 3, "Mitte und beide Enden");
    let (line, a, b) = only_line(s);
    let mid = midpoints(s);
    assert_eq!(mid.len(), 1);
    assert_eq!(mid[0].1, line);
    let m = s.point_xy(mid[0].0).unwrap();
    assert!(
        m.distance(v(5.0, 5.0)) < 1e-6,
        "Mitte am ersten Klick: {m:?}"
    );
    assert!(s.point_xy(b).unwrap().distance(v(25.0, 12.0)) < 1e-6);
    assert!(s.point_xy(a).unwrap().distance(v(-15.0, -2.0)) < 1e-6);
    assert_eq!(undo_len(&mut w), steps + 1, "ein Rückgängig-Schritt");
    assert!(w.active_tab_mut().state.undo_sketch());
    assert_eq!(sk(&w).points().count(), 0);
}

#[test]
fn typed_length_is_whole_line() {
    let mut w = rig();
    commit(&mut w, v(5.0, 5.0)).unwrap();
    type_in(&mut w, F::Length, "40");
    commit(&mut w, v(30.0, 5.0)).unwrap();
    let s = sk(&w);
    let (line, a, b) = only_line(s);
    assert!((s.line_length(line).unwrap() - 40.0).abs() < 1e-6);
    let (mid, _) = midpoints(s)[0];
    assert!(s.point_xy(mid).unwrap().distance(v(5.0, 5.0)) < 1e-6);
    assert!(
        s.point_xy(b).unwrap().distance(v(25.0, 5.0)) < 1e-6,
        "halbe Länge je Seite"
    );
    assert!(s.point_xy(a).unwrap().distance(v(-15.0, 5.0)) < 1e-6);
    let dist: Vec<_> = s
        .constraints()
        .filter_map(|(_, c)| match c.kind {
            K::Distance(p, q, d) => Some((p, q, d)),
            _ => None,
        })
        .collect();
    assert_eq!(dist.len(), 1, "{dist:?}");
    assert_eq!((dist[0].0, dist[0].1), (a, b));
    assert!((dist[0].2 - 40.0).abs() < 1e-9);
}

#[test]
fn first_click_on_point_reuses_it() {
    let mut w = rig();
    // Eine klassische Linie mit Ende bei (10, 0); der erste Klick der Mittelpunktlinie fängt es.
    {
        let (se, sf, _) = parts(&mut w);
        se.choose_kind(&mut sf.sketch, ToolKind::Line);
    }
    commit(&mut w, v(0.0, 0.0)).unwrap();
    commit(&mut w, v(10.0, 0.0)).unwrap();
    {
        let (se, sf, _) = parts(&mut w);
        se.cancel(&mut sf.sketch);
        se.choose_kind(&mut sf.sketch, ToolKind::Line);
        se.choose_kind(&mut sf.sketch, ToolKind::MidpointLine);
    }
    let points = sk(&w).points().count();
    commit(&mut w, v(10.0, 0.0)).unwrap();
    commit(&mut w, v(10.0, 20.0)).unwrap();
    let s = sk(&w);
    assert_eq!(
        s.points().count(),
        points + 2,
        "nur die zwei Enden sind neu"
    );
    let all = lines(s);
    assert_eq!(all.len(), 2);
    let (new_line, a, b) = all[1];
    let mid = midpoints(s);
    assert_eq!(mid.len(), 1);
    assert_eq!(mid[0].1, new_line);
    let m = mid[0].0;
    assert!(m != a && m != b, "die Mitte ist keines der Enden");
    assert!(s.point_xy(m).unwrap().distance(v(10.0, 0.0)) < 1e-6);
    let first_line = all[0];
    assert!(
        m == first_line.1 || m == first_line.2,
        "der vorhandene Punkt ist die Mitte"
    );
    let coincident = s
        .constraints()
        .filter(|(_, c)| matches!(c.kind, K::Coincident(..)))
        .count();
    assert_eq!(coincident, 0, "kein Coincident");
}

#[test]
fn no_chain_after_second_click() {
    let mut w = rig();
    commit(&mut w, v(5.0, 5.0)).unwrap();
    commit(&mut w, v(25.0, 12.0)).unwrap();
    assert!(parts(&mut w).0.pending.is_empty(), "keine Kette");
    commit(&mut w, v(50.0, 50.0)).unwrap();
    assert_eq!(
        parts(&mut w).0.pending.len(),
        1,
        "dritter Klick setzt eine neue Mitte"
    );
    assert_eq!(sk(&w).curves().count(), 1);
    commit(&mut w, v(70.0, 50.0)).unwrap();
    let s = sk(&w);
    assert_eq!(s.curves().count(), 2);
    assert_eq!(midpoints(s).len(), 2);
    assert_eq!(
        s.points().count(),
        6,
        "keine Verbindung zwischen den Linien"
    );
    assert!(
        s.constraints()
            .all(|(_, c)| !matches!(c.kind, K::Coincident(..)))
    );
}

#[test]
fn esc_after_first_click_leaves_nothing() {
    for cancel in 0..2 {
        let mut w = rig();
        let before = fingerprint(sk(&w));
        let steps = undo_len(&mut w);
        commit(&mut w, v(5.0, 5.0)).unwrap();
        let (se, sf, _) = parts(&mut w);
        if cancel == 0 {
            se.cancel(&mut sf.sketch);
        } else {
            se.set_tool(&mut sf.sketch, Tool::Select);
        }
        assert_eq!(fingerprint(sk(&w)), before, "Abbruch {cancel}");
        assert_eq!(undo_len(&mut w), steps, "Abbruch {cancel}");
        assert!(parts(&mut w).0.pending.is_empty());
    }
}

#[test]
fn panel_switch_toggles_line_kind() {
    let mut w = rig();
    {
        let (se, sf, _) = parts(&mut w);
        assert_eq!(se.tool, Tool::Line);
        assert_eq!(se.kind_of(Tool::Line), Some(ToolKind::MidpointLine));
        assert_eq!(
            ToolKind::of(Tool::Line),
            [ToolKind::Line, ToolKind::MidpointLine]
        );
        se.choose_kind(&mut sf.sketch, ToolKind::Line);
        assert_eq!(se.tool, Tool::Line, "das Werkzeug bleibt");
        assert_eq!(se.kinds.line, ToolKind::Line);
        se.choose_kind(&mut sf.sketch, ToolKind::MidpointLine);
        assert_eq!(se.kinds.line, ToolKind::MidpointLine);
        assert_eq!(ToolKind::MidpointLine.tool(), Tool::Line);
        assert_eq!(ToolKind::Line.action(), Action::SketchLine);
        assert_eq!(ToolKind::MidpointLine.action(), Action::SketchMidpointLine);
        assert_eq!(
            ToolKind::MidpointLine.label(),
            tr!("action-sketch-midpoint-line")
        );
        se.choose_kind(&mut sf.sketch, ToolKind::Line);
    }
    // Klassisch zeichnet es wieder eine Kette.
    commit(&mut w, v(0.0, 0.0)).unwrap();
    commit(&mut w, v(10.0, 0.0)).unwrap();
    assert_eq!(parts(&mut w).0.pending.len(), 1, "Linienzug läuft weiter");
    assert!(midpoints(sk(&w)).is_empty());
}

#[test]
fn midpoint_line_in_3d_on_triad_plane() {
    let mut w = rig();
    let (se, sf, _) = parts(&mut w);
    se.set_three_d(&mut sf.sketch, true);
    se.choose_kind(&mut sf.sketch, ToolKind::MidpointLine);
    se.triad.active = TriadPlane::Yz;
    let m = DVec3::new(0.0, 10.0, 20.0);
    let e = DVec3::new(0.0, 22.0, 14.0);
    se.place_point_3d(&mut sf.sketch, m, None).unwrap();
    se.place_point_3d(&mut sf.sketch, e, None).unwrap();
    assert!(se.pending.is_empty(), "keine Kette");
    let s = &sf.sketch;
    let (_, a, b) = only_line(s);
    let (mid, _) = midpoints(s)[0];
    let got = |p| s.point_xyz(p).unwrap();
    assert!(got(mid).distance(m) < 1e-6);
    assert!(got(b).distance(e) < 1e-6);
    let mirror = DVec3::new(0.0, -2.0, 26.0);
    assert!(got(a).distance(mirror) < 1e-6, "{:?}", got(a));
    for p in [a, b, mid] {
        assert!(got(p).x.abs() < 1e-9, "in der Triadenebene");
    }
}

#[test]
fn action_has_no_default_binding() {
    assert!(Action::SketchMidpointLine.default_bindings().is_empty());
    assert_eq!(
        Action::from_id("skizze.mittelpunktlinie"),
        Some(Action::SketchMidpointLine)
    );
    assert_eq!(
        Action::SketchMidpointLine.sketch_kind(),
        Some(ToolKind::MidpointLine)
    );
    assert_eq!(Action::SketchMidpointLine.sketch_tool(), Some(Tool::Line));
}

// ---- Oberfläche: Umschalter im Skizzenfeld und Liste am Leistenknopf -----------------------------

mod gui {
    use super::common;
    use eframe::App as _;
    use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
    use planora_app::app::PlanoraApp;
    use planora_app::sketch_kinds::ToolKind;
    use planora_app::sketch_panel;
    use planora_app::sketch_state::Tool;
    use planora_app::workspace::Workspace;
    use planora_app::{fonts, theme};
    use planora_feature::{SketchPlane, StandardPlane};
    use planora_project::config::Config;
    use planora_project::library::Library;

    struct Rig {
        ctx: egui::Context,
        app: PlanoraApp,
        time: f64,
    }

    impl Rig {
        fn new() -> Rig {
            let root = common::temp_root("planora-sketch-midpoint-line-gui");
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
            let mut rig = Rig {
                ctx,
                app: PlanoraApp::for_tests(w),
                time: 0.0,
            };
            rig.settle();
            rig
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let mut f = eframe::Frame::_new_kittest();
            let app = &mut self.app;
            let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
            out.drop_without_applying_deltas();
        }

        fn settle(&mut self) {
            for _ in 0..3 {
                self.frame(vec![]);
            }
        }

        fn click(&mut self, p: Pos2) {
            self.time += 1.0;
            let button = |pressed| Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(p)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        fn line_kind(&mut self) -> (Tool, ToolKind) {
            let se = self
                .app
                .ws_mut()
                .active_tab_mut()
                .state
                .sketch_session
                .as_ref()
                .unwrap();
            (se.tool, se.kinds.line)
        }
    }

    #[test]
    fn panel_buttons_switch_the_kind() {
        let mut rig = Rig::new();
        assert!(sketch_panel::line_kind_buttons(&rig.ctx).is_empty());
        {
            let st = &mut rig.app.ws_mut().active_tab_mut().state;
            let (se, sk) = st.session_sketch_mut().unwrap();
            se.choose_tool(sk, Tool::Line);
        }
        rig.settle();
        let buttons = sketch_panel::line_kind_buttons(&rig.ctx);
        assert_eq!(
            buttons.iter().map(|b| b.0).collect::<Vec<_>>(),
            [ToolKind::Line, ToolKind::MidpointLine]
        );
        assert!(
            sketch_panel::has_flyout(Tool::Line),
            "Liste am Leistenknopf"
        );
        assert_eq!(rig.line_kind(), (Tool::Line, ToolKind::Line));
        rig.click(buttons[1].1.center());
        assert_eq!(rig.line_kind(), (Tool::Line, ToolKind::MidpointLine));
        let buttons = sketch_panel::line_kind_buttons(&rig.ctx);
        rig.click(buttons[0].1.center());
        assert_eq!(rig.line_kind(), (Tool::Line, ToolKind::Line));
    }
}
