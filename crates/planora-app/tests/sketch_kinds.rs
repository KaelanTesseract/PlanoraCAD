//! Skizzen 3, Task 5: Die Arten eines Werkzeugs als Daten, die Stufenmaschine, die 17 neuen
//! Aktionen und die beiden neuen Werkzeuge der Leiste. Die Arten selbst stehen in den
//! `sketch_kinds_*`-Tests.
//!
//! Mit echten Bildern der ganzen App bei 1920×1080 (`PlanoraApp::for_tests`), wo es um Tasten,
//! Knöpfe und Mausereignisse geht, sonst ohne Oberfläche.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult, search};
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_kinds::{ToolKind, ToolKinds};
use planora_app::sketch_panel;
use planora_app::sketch_state::{Item, SketchSession, Snap, Tool};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{
    FeatureId, ParamScope, ParamUnit, SketchFeature, SketchPlane, StandardPlane,
};
use planora_i18n::{english, german, t, with_lang};
use planora_math::{DVec2, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::{CurveId, Sketch};

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
/// Drei freie Stellen in der Zeichenfläche.
const SPOT_A: Pos2 = pos2(800.0, 500.0);
const SPOT_B: Pos2 = pos2(1000.0, 500.0);
const SPOT_C: Pos2 = pos2(1000.0, 650.0);

// ---- Hilfen ---------------------------------------------------------------------------------

/// Eine offene, leere Skizze auf XY.
fn sketch_ws() -> Workspace {
    let root = common::temp_root("planora-sketch-kinds");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    let s = &mut w.active_tab_mut().state;
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    w.observe_history(false);
    w
}

fn chord(key: Key) -> KeyChord {
    KeyChord {
        key,
        ctrl: false,
        shift: false,
        alt: false,
    }
}

fn key(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: NONE,
    }
}

fn button(p: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: NONE,
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new() -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(sketch_ws()),
            time: 0.0,
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

    fn se(&mut self) -> &mut SketchSession {
        self.st().sketch_session.as_mut().expect("Skizze offen")
    }

    fn frame(&mut self, events: Vec<Event>) {
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
        out.drop_without_applying_deltas();
    }

    fn settle(&mut self) {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![]);
    }

    fn press(&mut self, k: Key) {
        self.frame(vec![key(k)]);
        self.settle();
    }

    fn click_with(&mut self, p: Pos2, b: PointerButton) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, b, true)]);
        self.frame(vec![button(p, b, false)]);
        self.frame(vec![]);
    }

    fn click(&mut self, p: Pos2) {
        self.click_with(p, PointerButton::Primary);
    }

    fn strip(&self, label: &str) -> Pos2 {
        sketch_panel::strip_buttons(&self.ctx)
            .into_iter()
            .find(|(l, _)| *l == label)
            .unwrap_or_else(|| panic!("Werkzeug „{label}“ fehlt"))
            .1
            .center()
    }

    /// Wählt die Art wie ihre Aktion (Taste, Menü).
    fn choose(&mut self, kind: ToolKind) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.choose_kind(sk, kind);
    }

    fn undo_len(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn fingerprint(&mut self) -> (u64, Vec<u64>) {
        let sk = self.st().session_sketch().unwrap();
        (
            sk.revision(),
            sk.variables().iter().map(|v| v.to_bits()).collect(),
        )
    }
}

fn session() -> (SketchSession, Sketch) {
    (
        SketchSession::new(FeatureId(0)),
        Sketch::new(Frame::WORLD, "S", Tolerance::default()),
    )
}

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let a = sk.add_point(a.0, a.1).unwrap();
    let b = sk.add_point(b.0, b.1).unwrap();
    sk.add_line(a, b).unwrap()
}

fn bits(sk: &Sketch) -> (u64, Vec<u64>) {
    (
        sk.revision(),
        sk.variables().iter().map(|v| v.to_bits()).collect(),
    )
}

fn at(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

const R: f64 = 0.5;

// ---- Daten ----------------------------------------------------------------------------------

#[test]
fn kinds_cover_their_tools_without_gaps() {
    // Skizzen 4: Rundung und die drei Fasen kamen dazu (19 → 23).
    assert_eq!(ToolKind::ALL.len(), 23);
    let mut seen = Vec::new();
    for tool in [
        Tool::Rectangle,
        Tool::Circle,
        Tool::Arc,
        Tool::Polygon,
        Tool::Slot,
        Tool::Fillet,
    ] {
        assert!(!ToolKind::of(tool).is_empty(), "{tool:?}");
        for &k in ToolKind::of(tool) {
            assert_eq!(k.tool(), tool, "{k:?}");
            assert!(!seen.contains(&k), "{k:?} doppelt");
            seen.push(k);
        }
    }
    assert_eq!(seen.len(), 23);
    for tool in [
        Tool::Select,
        Tool::Point,
        Tool::Line,
        Tool::Ellipse,
        Tool::Spline,
        Tool::ProjectEdge,
        Tool::Trim,
        Tool::Extend,
        Tool::Break,
        Tool::Offset,
        Tool::MoveCopy,
        Tool::Scale,
    ] {
        assert!(ToolKind::of(tool).is_empty(), "{tool:?}");
    }
    let classic: Vec<ToolKind> = ToolKind::ALL
        .iter()
        .copied()
        .filter(|k| k.is_classic())
        .collect();
    assert_eq!(
        classic,
        [
            ToolKind::Rect2P,
            ToolKind::CircleCenter,
            ToolKind::ArcCenter
        ]
    );
    assert_eq!(
        ToolKinds::default(),
        ToolKinds {
            rectangle: ToolKind::Rect2P,
            circle: ToolKind::CircleCenter,
            arc: ToolKind::ArcCenter,
            polygon: ToolKind::PolygonCircumscribed,
            slot: ToolKind::SlotCenterToCenter,
            fillet: ToolKind::Fillet,
        }
    );
    let lines_only: Vec<ToolKind> = ToolKind::ALL
        .iter()
        .copied()
        .filter(|k| k.picks_lines_only())
        .collect();
    assert_eq!(lines_only, [ToolKind::Circle2T]);
}

#[test]
fn every_kind_has_one_action_and_names_in_both_languages() {
    for lang in [german(), english()] {
        with_lang(&lang, || {
            let mut names = Vec::new();
            let mut actions = Vec::new();
            for &k in ToolKind::ALL {
                let a = k.action();
                assert_eq!(Action::for_kind(k), a, "{k:?}");
                assert_eq!(a.sketch_kind(), Some(k), "{k:?}");
                assert_eq!(a.sketch_tool(), Some(k.tool()), "{k:?}");
                assert!(a.repeatable(), "{k:?}");
                assert_eq!(a.area(), Area::Sketch, "{k:?}");
                assert!(!actions.contains(&a), "{k:?}: Aktion doppelt");
                actions.push(a);
                let label = k.label();
                assert!(!label.is_empty() && !label.starts_with("action-"), "{k:?}");
                assert_eq!(label, a.label(), "{k:?}");
                assert!(!names.contains(&label), "{label} doppelt");
                names.push(label);
                let hint = k.hint();
                assert!(
                    !hint.is_empty() && !hint.starts_with("sketch-kind"),
                    "{k:?}"
                );
            }
        });
    }
    // Namen nach Entscheidung 12 (Stichprobe je Form).
    with_lang(&german(), || {
        assert_eq!(ToolKind::Rect2P.label(), "2-Punkt-Rechteck");
        assert_eq!(
            ToolKind::CircleCenter.label(),
            "Mittelpunkt-Durchmesser-Kreis"
        );
        assert_eq!(ToolKind::ArcTangent.label(), "Tangentialer Bogen");
        assert_eq!(ToolKind::PolygonEdge.label(), "Kantenpolygon");
        assert_eq!(
            ToolKind::SlotCenterPointArc.label(),
            "Langloch Mittelpunkt-Bogen"
        );
    });
    with_lang(&english(), || {
        assert_eq!(ToolKind::Rect2P.label(), "2-Point Rectangle");
        assert_eq!(ToolKind::CircleCenter.label(), "Center Diameter Circle");
        assert_eq!(ToolKind::ArcTangent.label(), "Tangent Arc");
        assert_eq!(ToolKind::PolygonEdge.label(), "Edge Polygon");
        assert_eq!(
            ToolKind::SlotCenterPointArc.label(),
            "Center Point Arc Slot"
        );
    });
    // Feste Ids (Schlüssel in der Konfiguration).
    assert_eq!(ToolKind::Rect2P.action().id(), "skizze.rechteck");
    assert_eq!(ToolKind::CircleCenter.action().id(), "skizze.kreis");
    assert_eq!(ToolKind::Rect3P.action().id(), "skizze.rechteck_3p");
    assert_eq!(
        ToolKind::SlotThreePointArc.action().id(),
        "skizze.langloch_bogen_3p"
    );
    assert_eq!(
        ToolKind::SlotCenterPointArc.action().id(),
        "skizze.langloch_bogen_mitte"
    );
    // In der Tabelle zwischen Kreis und Kantenkopie.
    let i = |a: Action| Action::ALL.iter().position(|&x| x == a).unwrap();
    assert!(i(Action::SketchCircle) < i(ToolKind::Rect3P.action()));
    assert!(i(ToolKind::SlotCenterPointArc.action()) < i(Action::SketchProjectEdge));
    assert_eq!(i(Action::SketchProjectEdge) - i(Action::SketchCircle), 18);
    // Leiste: die Knöpfe behalten ihren Namen, die neuen Formen kommen dazu.
    with_lang(&german(), || {
        assert_eq!(sketch_panel::tool_label(Tool::Rectangle), "Rechteck");
        assert_eq!(sketch_panel::tool_label(Tool::Circle), "Kreis");
        assert_eq!(sketch_panel::tool_label(Tool::Polygon), "Polygon");
        assert_eq!(sketch_panel::tool_label(Tool::Slot), "Langloch");
        assert_eq!(t("undo-polygon"), "Polygon zeichnen");
        assert_eq!(t("undo-slot"), "Langloch zeichnen");
    });
    with_lang(&english(), || {
        assert_eq!(sketch_panel::tool_label(Tool::Slot), "Slot");
        assert_eq!(t("undo-polygon"), "Draw polygon");
        assert_eq!(t("undo-slot"), "Draw slot");
    });
}

#[test]
fn stages_follow_the_table() {
    use ToolKind as K;
    let empty: &[F] = &[];
    let table: [(ToolKind, &[&[F]]); 16] = [
        (K::Rect3P, &[empty, &[F::Width, F::Angle], &[F::Depth]]),
        (K::RectCenter, &[empty, &[F::Width, F::Height]]),
        (K::Circle2P, &[empty, &[F::Diameter]]),
        (K::Circle2T, &[empty, empty, &[F::Diameter]]),
        (K::Circle3P, &[empty, empty, empty]),
        (K::Circle3T, &[empty, empty, empty]),
        (K::Arc3P, &[empty, empty, &[F::Radius]]),
        (K::ArcTangent, &[empty, &[F::Radius, F::Sweep]]),
        (K::PolygonInscribed, &[empty, &[F::Radius, F::Sides]]),
        (K::PolygonCircumscribed, &[empty, &[F::Radius, F::Sides]]),
        (
            K::PolygonEdge,
            &[empty, &[F::Length, F::Sides], &[F::Sides]],
        ),
        (K::SlotCenterToCenter, &[empty, &[F::Length], &[F::Width]]),
        (K::SlotCenterPoint, &[empty, &[F::Length], &[F::Width]]),
        (K::SlotOverall, &[empty, &[F::Length], &[F::Width]]),
        (
            K::SlotThreePointArc,
            &[empty, empty, &[F::Radius, F::Sweep], &[F::Width]],
        ),
        (
            K::SlotCenterPointArc,
            &[empty, &[F::Radius], &[F::Sweep], &[F::Width]],
        ),
    ];
    for (k, stages) in table {
        assert_eq!(k.stages(), stages, "{k:?}");
    }
    for &k in ToolKind::ALL {
        assert!(k.stages()[0].is_empty(), "{k:?}: Stufe 0 hat nie Felder");
    }
}

// ---- Belegbar -------------------------------------------------------------------------------

#[test]
fn default_keys_stay_and_twenty_actions_have_none() {
    let km = Keymap::defaults();
    assert_eq!(
        km.action_for_key(Area::Sketch, &chord(Key::R)),
        Some(ToolKind::Rect2P.action())
    );
    assert_eq!(
        km.action_for_key(Area::Sketch, &chord(Key::C)),
        Some(ToolKind::CircleCenter.action())
    );
    let free = ToolKind::ALL
        .iter()
        .filter(|k| k.action().default_bindings().is_empty())
        .count();
    // Skizzen 4: Die Rundung hat F, die drei Fasen keine Taste (17 → 20).
    assert_eq!(free, 20);
    assert!(km.to_overrides().is_empty(), "Konfiguration bleibt leer");
}

#[test]
fn every_kind_can_be_bound_and_its_key_chooses_it() {
    for &kind in ToolKind::ALL {
        let mut rig = Rig::new();
        let mut km = rig.ws().keymap.clone();
        assert_eq!(
            km.set(kind.action(), 0, Binding::Key(chord(Key::F9))),
            SetResult::Ok,
            "{kind:?}"
        );
        // In der Einstellungsliste steht sie unter ihrem Namen.
        assert!(
            search::matches(kind.action(), &km, &kind.label()),
            "{kind:?}"
        );
        rig.ws().keymap = km;
        rig.press(Key::F9);
        let se = rig.se();
        assert_eq!(se.tool, kind.tool(), "{kind:?}");
        assert_eq!(se.kind_of(kind.tool()), Some(kind), "{kind:?}");
    }
}

// ---- Wählen ---------------------------------------------------------------------------------

#[test]
fn keys_choose_the_default_and_the_button_the_last_kind() {
    let mut rig = Rig::new();
    let label = sketch_panel::tool_label(Tool::Rectangle);
    rig.choose(ToolKind::Rect3P);
    assert_eq!(rig.se().tool, Tool::Rectangle);
    assert_eq!(rig.se().planned_kind(), Some(ToolKind::Rect3P));

    // Klick auf das Symbol, von einem anderen Werkzeug aus: die zuletzt benutzte Art.
    rig.choose(ToolKind::CircleCenter);
    assert_eq!(rig.se().tool, Tool::Circle);
    let button = rig.strip(&label);
    rig.click(button);
    assert_eq!(rig.se().tool, Tool::Rectangle);
    assert_eq!(rig.se().kinds.rectangle, ToolKind::Rect3P);
    assert_eq!(rig.se().planned_kind(), Some(ToolKind::Rect3P));
    // Ebenso bei schon gewähltem Werkzeug.
    rig.click(button);
    assert_eq!(rig.se().planned_kind(), Some(ToolKind::Rect3P));

    // R wählt immer die Vorgabe (klassisch: keine geplante Art).
    rig.press(Key::R);
    assert_eq!(rig.se().tool, Tool::Rectangle);
    assert_eq!(rig.se().kinds.rectangle, ToolKind::Rect2P);
    assert_eq!(rig.se().planned_kind(), None);

    // Neue Sitzung: wieder die Vorgabe.
    assert_eq!(SketchSession::new(FeatureId(0)).kinds, ToolKinds::default());
    assert_eq!(SketchSession::new(FeatureId(0)).polygon_sides, 6);
}

#[test]
fn other_kind_drops_the_positions_same_kind_keeps_them() {
    let (mut se, mut sk) = session();
    se.choose_kind(&mut sk, ToolKind::Rect3P);
    se.click(&mut sk, at(0.0, 0.0), R, false).unwrap();
    assert_eq!(se.pending_pos.len(), 1);
    // Dieselbe Art: alles bleibt.
    se.choose_kind(&mut sk, ToolKind::Rect3P);
    assert_eq!(se.pending_pos, [at(0.0, 0.0)]);
    se.choose_tool(&mut sk, Tool::Rectangle);
    assert_eq!(se.pending_pos.len(), 1);
    // Andere Art derselben Form: Abbruch.
    se.choose_kind(&mut sk, ToolKind::RectCenter);
    assert!(se.pending_pos.is_empty() && se.pending_snaps.is_empty());
    assert_eq!(se.planned_kind(), Some(ToolKind::RectCenter));
    // Die Art ist auch zurück zur Vorgabe ein Wechsel.
    se.click(&mut sk, at(0.0, 0.0), R, false).unwrap();
    se.choose_kind(&mut sk, ToolKind::Rect2P);
    assert!(se.pending_pos.is_empty());
    assert_eq!(se.planned_kind(), None);
}

#[test]
fn repeat_last_chooses_the_used_kind() {
    let mut rig = Rig::new();
    rig.choose(ToolKind::Rect3P);
    rig.settle();
    // Die Arten legen erst mit späteren Tasks an; das Merken hängt nur an „eine Kurve kam dazu“.
    {
        let (_, sk) = rig.st().session_sketch_mut().unwrap();
        line(sk, (0.0, 0.0), (10.0, 0.0));
    }
    rig.settle();
    assert_eq!(
        rig.ws().last_command.sketch_tool(),
        Some(ToolKind::Rect3P.action())
    );
    rig.press(Key::R);
    assert_eq!(rig.se().planned_kind(), None);
    let select = sketch_panel::tool_label(Tool::Select);
    let p = rig.strip(&select);
    rig.click(p);
    assert_eq!(rig.se().tool, Tool::Select);
    rig.press(Key::Space);
    assert_eq!(rig.se().tool, Tool::Rectangle);
    assert_eq!(rig.se().planned_kind(), Some(ToolKind::Rect3P));
}

/// Auch der 3-Punkt-Bogen wird gemerkt; sobald eine Kurve dazukommt, zählt die aktive Art.
#[test]
fn planned_arc_kind_is_remembered() {
    let mut rig = Rig::new();
    rig.choose(ToolKind::Arc3P);
    rig.settle();
    {
        let (_, sk) = rig.st().session_sketch_mut().unwrap();
        line(sk, (0.0, 0.0), (10.0, 0.0));
    }
    rig.settle();
    assert_eq!(
        rig.ws().last_command.sketch_tool(),
        Some(ToolKind::Arc3P.action())
    );
}

// ---- Leiste und Hinweis ---------------------------------------------------------------------

#[test]
fn strip_has_eighteen_tools_and_fits_at_full_hd() {
    assert_eq!(
        sketch_panel::TOOL_ORDER,
        [
            Tool::Select,
            Tool::Point,
            Tool::Line,
            Tool::Rectangle,
            Tool::Circle,
            Tool::Arc,
            Tool::Polygon,
            Tool::Slot,
            Tool::Ellipse,
            Tool::Spline,
            Tool::ProjectEdge,
            Tool::Trim,
            Tool::Extend,
            Tool::Break,
            // Skizzen 4
            Tool::Fillet,
            Tool::Offset,
            Tool::MoveCopy,
            Tool::Scale,
        ]
    );
    let rig = Rig::new();
    let buttons = sketch_panel::strip_buttons(&rig.ctx);
    assert_eq!(buttons.len(), 18);
    for (label, r) in &buttons {
        assert!(
            r.top() >= 0.0 && r.bottom() < SIZE.y - planora_app::shell::BOTTOM_H,
            "{label}: {r:?}"
        );
    }
    for pair in buttons.windows(2) {
        assert!(pair[0].1.bottom() <= pair[1].1.top(), "{}", pair[1].0);
    }
}

#[test]
fn polygon_and_slot_can_be_chosen_by_their_button() {
    let mut rig = Rig::new();
    for (tool, kind) in [
        (Tool::Polygon, ToolKind::PolygonCircumscribed),
        (Tool::Slot, ToolKind::SlotCenterToCenter),
    ] {
        let p = rig.strip(&sketch_panel::tool_label(tool));
        rig.click(p);
        assert_eq!(rig.se().tool, tool);
        assert_eq!(rig.se().planned_kind(), Some(kind));
        assert!(rig.se().is_drawing_tool());
    }
}

#[test]
fn active_hint_names_the_kind_and_the_default_says_what_it_said() {
    let (mut se, mut sk) = session();
    se.set_tool(&mut sk, Tool::Line);
    assert_eq!(se.active_hint(), sketch_panel::tool_hint(Tool::Line));
    for (tool, kind) in [
        (Tool::Rectangle, ToolKind::Rect2P),
        (Tool::Circle, ToolKind::CircleCenter),
        (Tool::Arc, ToolKind::ArcCenter),
    ] {
        se.set_tool(&mut sk, tool);
        assert_eq!(se.active_hint(), sketch_panel::tool_hint(tool), "{kind:?}");
        assert_eq!(kind.hint(), sketch_panel::tool_hint(tool), "{kind:?}");
    }
    se.choose_kind(&mut sk, ToolKind::Rect3P);
    assert_eq!(se.active_hint(), ToolKind::Rect3P.hint());
    assert_ne!(se.active_hint(), sketch_panel::tool_hint(Tool::Rectangle));
}

// ---- Stufenmaschine -------------------------------------------------------------------------

#[test]
fn stages_collect_positions_and_a_last_click_without_a_shape_refuses() {
    let (mut se, mut sk) = session();
    // Skizzen 3, Task 9: Das Polygon ist fertig; es zeigt das Langloch.
    se.choose_kind(&mut sk, ToolKind::SlotCenterToCenter);
    let start = bits(&sk);
    assert_eq!(se.kind_stage(), 0);
    se.click(&mut sk, at(0.0, 0.0), R, false).unwrap();
    assert_eq!(se.kind_stage(), 1);
    se.click(&mut sk, at(10.0, 0.0), R, false).unwrap();
    assert_eq!(se.kind_stage(), 2);
    assert_eq!(se.pending_pos, [at(0.0, 0.0), at(10.0, 0.0)]);
    assert_eq!(se.pending_snaps.len(), 2);
    assert_eq!(bits(&sk), start, "die Skizze bleibt bis zum letzten Klick");
    // Skizzen 3, Task 10: Alle Arten sind fertig; ein letzter Klick ohne Form lehnt ab, und die
    // Form läuft weiter.
    let refused = se.click(&mut sk, at(3.0, 0.0), R, false).unwrap_err();
    assert_eq!(refused, t("sketch-slot-too-wide"));
    assert_eq!(se.message_text().as_deref(), Some(refused.as_str()));
    assert_eq!(se.pending_pos.len(), 2);
    assert_eq!(bits(&sk), start);
    assert!(se.created.is_none());
}

#[test]
fn snaps_are_remembered_and_a_snapped_point_stays_unchanged() {
    let (mut se, mut sk) = session();
    let p = sk.add_point(5.0, 5.0).unwrap();
    se.choose_kind(&mut sk, ToolKind::RectCenter);
    se.click(&mut sk, at(5.1, 5.1), R, false).unwrap();
    assert_eq!(se.pending_pos, [at(5.0, 5.0)]);
    assert_eq!(se.pending_snaps[0].snap, Snap::Point(p));
    // Die gemerkte Lage richtet den Zeiger nicht mehr aus: Fang geht vor, das bleibt gleich;
    // daneben auf gleicher Höhe zieht der eigene Punkt nicht.
    se.cursor_opts.suppressed = false;
    let cur = se.resolve_cursor(&sk, at(9.0, 5.2), 0.5);
    assert!(cur.guides.y.is_none(), "{:?}", cur.guides);
}

#[test]
fn double_click_at_stage_one_adds_one_position_and_keeps_all() {
    let mut rig = Rig::new();
    rig.choose(ToolKind::Rect3P);
    rig.settle();
    rig.click(SPOT_A);
    assert_eq!(rig.se().pending_pos.len(), 1);
    rig.time += 2.0;
    let p = SPOT_B;
    rig.frame(vec![Event::PointerMoved(p)]);
    for _ in 0..2 {
        rig.frame(vec![button(p, PointerButton::Primary, true)]);
        rig.frame(vec![button(p, PointerButton::Primary, false)]);
    }
    rig.settle();
    assert_eq!(rig.se().pending_pos.len(), 2, "genau eine Lage mehr");
    assert_eq!(rig.st().session_sketch().unwrap().curves().count(), 0);
}

#[test]
fn enter_without_fields_does_nothing() {
    let mut rig = Rig::new();
    rig.choose(ToolKind::Circle3P);
    rig.settle();
    rig.click(SPOT_A);
    assert_eq!(rig.se().pending_pos.len(), 1);
    assert!(rig.se().input.fields.is_empty());
    rig.press(Key::Enter);
    assert_eq!(rig.se().pending_pos.len(), 1);
    // Ohne Oberfläche: `finish_tool` tut bei einer geplanten Art nichts.
    let (mut se, mut sk) = session();
    se.choose_kind(&mut sk, ToolKind::Rect3P);
    se.click(&mut sk, at(1.0, 1.0), R, false).unwrap();
    se.finish_tool(&mut sk).unwrap();
    assert_eq!(se.pending_pos.len(), 1);
}

// ---- Abbruch --------------------------------------------------------------------------------

/// Stufe 1 einer geplanten Art und `leave`: nichts bleibt zurück, kein Rückgängig-Schritt.
fn leaves_nothing(leave: impl FnOnce(&mut Rig)) {
    let mut rig = Rig::new();
    rig.choose(ToolKind::Rect3P);
    rig.settle();
    let (before, steps) = (rig.fingerprint(), rig.undo_len());
    rig.click(SPOT_C);
    assert_eq!(rig.se().pending_pos.len(), 1);
    leave(&mut rig);
    rig.settle();
    assert!(rig.se().pending_pos.is_empty());
    assert!(rig.se().pending_snaps.is_empty());
    assert_eq!(rig.fingerprint(), before, "Skizze bitgleich");
    assert_eq!(rig.undo_len(), steps, "kein Rückgängig-Schritt");
}

#[test]
fn escape_at_stage_one_leaves_nothing() {
    leaves_nothing(|rig| rig.press(Key::Escape));
}

#[test]
fn right_click_at_stage_one_leaves_nothing() {
    leaves_nothing(|rig| rig.click_with(SPOT_C, PointerButton::Secondary));
}

#[test]
fn tool_change_at_stage_one_leaves_nothing() {
    // Über den Knopf: Eine Taste ginge in Stufe 1 ins Feld am Zeiger.
    leaves_nothing(|rig| {
        let p = rig.strip(&sketch_panel::tool_label(Tool::Line));
        rig.click(p);
        assert_eq!(rig.se().tool, Tool::Line);
    });
}

#[test]
fn kind_change_at_stage_one_leaves_nothing() {
    leaves_nothing(|rig| rig.choose(ToolKind::RectCenter));
}

#[test]
fn history_jump_and_removed_curves_clear_picked_curves() {
    let (mut se, mut sk) = session();
    let a = line(&mut sk, (0.0, 0.0), (10.0, 0.0));
    let b = line(&mut sk, (0.0, 5.0), (10.0, 5.0));
    se.choose_kind(&mut sk, ToolKind::Circle2T);
    se.pending_curves = vec![a, b];
    se.after_history_jump(&sk);
    assert!(se.pending_curves.is_empty());

    // Stutzen löscht die ganze Linie: `forget_removed` nimmt sie heraus.
    se.set_tool(&mut sk, Tool::Trim);
    se.pending_curves = vec![a, b];
    se.click(&mut sk, at(5.0, 0.0), R, false).unwrap();
    assert!(sk.curve(a).is_none());
    assert_eq!(se.pending_curves, [b]);

    // Löschen der Auswahl ebenso.
    se.selection = vec![Item::Curve(b)];
    se.delete_selection(&mut sk);
    assert!(se.pending_curves.is_empty());

    // Esc vergisst die gewählten Kurven.
    let c = line(&mut sk, (0.0, 9.0), (10.0, 9.0));
    se.choose_kind(&mut sk, ToolKind::Circle2T);
    se.pending_curves = vec![c];
    se.cancel(&mut sk);
    assert!(se.pending_curves.is_empty());
}

// ---- Kurven wählen --------------------------------------------------------------------------

#[test]
fn picking_kinds_snap_to_curves_only() {
    let (mut se, mut sk) = session();
    let a = line(&mut sk, (0.0, 0.0), (10.0, 0.0));
    let b = line(&mut sk, (0.0, 5.0), (10.0, 5.0));
    se.set_tool(&mut sk, Tool::Rectangle);
    assert!(!se.picks_curves());
    se.choose_kind(&mut sk, ToolKind::Circle2T);
    assert!(se.picks_curves());
    // Auf dem Endpunkt: trotzdem die Kurve, nicht der Punkt.
    let cur = se.resolve_cursor(&sk, at(0.1, 0.1), R);
    assert_eq!(cur.snap, Snap::Curve(a));
    // Ohne Kurve: nichts, auch kein Raster.
    se.cursor_opts.grid_step = Some(1.0);
    let cur = se.resolve_cursor(&sk, at(3.3, 2.4), R);
    assert_eq!(cur.snap, Snap::None);
    assert_eq!(cur.at, at(3.3, 2.4));

    // Klick ohne Kurve wählt nichts; Klick auf eine Kurve zählt als Stufe, zweimal dieselbe nicht.
    let start = bits(&sk);
    se.click(&mut sk, at(3.3, 2.4), R, false).unwrap();
    assert_eq!(se.kind_stage(), 0);
    se.click(&mut sk, at(3.0, 0.1), R, false).unwrap();
    assert_eq!(se.pending_curves, [a]);
    se.click(&mut sk, at(4.0, 0.1), R, false).unwrap();
    assert_eq!(se.pending_curves, [a]);
    se.click(&mut sk, at(3.0, 5.1), R, false).unwrap();
    assert_eq!(se.pending_curves, [a, b]);
    assert_eq!(se.kind_stage(), 2);
    assert!(se.pending_pos.is_empty());
    assert_eq!(bits(&sk), start);
    // `2 tangential`: Die letzte Stufe ist der Durchmesser, keine Kurvenwahl mehr.
    assert!(!se.picks_curves());
    se.sync_input(&sk);
    assert_eq!(se.input.fields.len(), 1);
    assert_eq!(se.input.fields[0].kind, F::Diameter);
    // `3 tangential` wählt bis zuletzt.
    se.choose_kind(&mut sk, ToolKind::Circle3T);
    se.pending_curves = vec![a, b];
    assert!(se.picks_curves());
}

// ---- Felder ---------------------------------------------------------------------------------

#[test]
fn sides_field_is_a_whole_number_from_three_to_sixty_four() {
    assert_eq!(F::Sides.unit(), ParamUnit::Number);
    assert_eq!(F::Depth.unit(), ParamUnit::Length);
    assert!(!F::Sides.label().is_empty() && !F::Depth.label().is_empty());
    let scope = ParamScope::default();
    let (mut se, mut sk) = session();
    se.choose_kind(&mut sk, ToolKind::PolygonEdge);
    se.click(&mut sk, at(0.0, 0.0), R, false).unwrap();
    se.sync_input(&sk);
    let i = se
        .input
        .fields
        .iter()
        .position(|f| f.kind == F::Sides)
        .expect("Feld Seitenzahl");
    assert_eq!(se.field_value(&sk, F::Sides), Some(6.0), "zuletzt benutzt");
    let sides_before = se.input.sides;
    for bad in ["2", "65", "6.5", "-3"] {
        se.type_field(i, bad, &scope);
        assert_eq!(
            se.input.fields[i].error.as_deref(),
            Some(t("sketch-field-sides-range").as_str()),
            "{bad}"
        );
        assert_eq!(se.input.sides, sides_before, "{bad}");
    }
    // Ungültiger Text hält die letzte gültige Zahl, die Vorschau springt nicht zurück.
    se.type_field(i, "8", &scope);
    se.type_field(i, "99", &scope);
    assert!(se.input.fields[i].error.is_some());
    assert_eq!(se.input.sides, Some(8));
    for (good, n) in [("3", 3), ("64", 64), ("8", 8)] {
        se.type_field(i, good, &scope);
        assert_eq!(se.input.fields[i].error, None, "{good}");
        assert_eq!(se.input.sides, Some(n), "{good}");
    }
    assert_eq!(se.field_value(&sk, F::Sides), Some(8.0));
    // Sie gilt über die Stufen, das Polygon zählt erst beim Abschluss.
    let mut sf = SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: Vec::new(),
        formulas: Default::default(),
    };
    se.commit_input(&mut sf, at(10.0, 0.0), R, false, &scope)
        .unwrap();
    assert_eq!(se.kind_stage(), 2);
    assert_eq!(se.input.sides, Some(8));
    assert_eq!(se.input.fields[0].kind, F::Sides);
    assert!(se.input.fields[0].locked.is_none());
    assert_eq!(se.field_value(&sf.sketch, F::Sides), Some(8.0));
    assert_eq!(se.polygon_sides, 6);
    assert_eq!(
        sf.sketch.constraints().count(),
        0,
        "die Seitenzahl wird kein Maß"
    );
    // Esc vergisst sie.
    se.cancel(&mut sf.sketch);
    assert_eq!(se.input.sides, None);
    assert_eq!(se.polygon_sides, 6);
}

#[test]
fn field_texts_exist_in_both_languages() {
    for lang in [german(), english()] {
        with_lang(&lang, || {
            for key in [
                "sketch-field-depth",
                "sketch-field-sides",
                "sketch-field-sides-range",
                "sketch-field-radius-or-sweep",
            ] {
                assert!(!t(key).is_empty() && t(key) != key, "{key}");
            }
        });
    }
}

// ---- Vorschau ohne Plan ---------------------------------------------------------------------

#[test]
fn planned_kinds_have_no_plan_yet_and_draw_without_panic() {
    let (mut se, mut sk) = session();
    se.choose_kind(&mut sk, ToolKind::Rect3P);
    assert!(se.kind_plan(&sk, at(1.0, 1.0)).is_none());
    assert!(se.kind_preview(&sk, at(1.0, 1.0)).is_none());
    // Mit jeder Art einmal zeichnen: Felder, Vorschau, Leiste und Fuß laufen ohne Plan durch.
    let mut rig = Rig::new();
    for &kind in ToolKind::ALL {
        rig.choose(kind);
        rig.settle();
        rig.click(SPOT_A);
        rig.frame(vec![Event::PointerMoved(SPOT_B)]);
        assert_eq!(rig.se().kind_of(kind.tool()), Some(kind));
        rig.press(Key::Escape);
    }
}
