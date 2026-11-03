//! Skizzen 4, Task 6: Die neuen Werkzeuge der Leiste (Rundung mit den Fasen, Versatz,
//! Verschieben/Kopieren, Maßstab), ihre Arten, Aktionen, Kürzel, Menüs und Felder. Die Rechnung
//! dahinter kommt mit Task 7 bis 10; hier stehen nur die Stümpfe.
//!
//! Mit echten Bildern der ganzen App bei 1920×1080, wo es um Tasten, Knöpfe und Menüs geht.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::{self, PlanoraApp};
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult, search};
use planora_app::sketch_input::{DrawField, FieldKind as F};
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_panel;
use planora_app::sketch_state::{SketchSession, Tool, TransformStage, TransformState};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell, theme};
use planora_feature::{FeatureId, ParamScope, ParamUnit, SketchPlane, StandardPlane};
use planora_i18n::{DisplayUnit, english, german, t, with_lang, with_unit};
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;

/// Die vier Werkzeuge von Skizzen 4 mit ihrer Art, soweit sie eine haben.
const NEW_TOOLS: [Tool; 4] = [Tool::Fillet, Tool::Offset, Tool::MoveCopy, Tool::Scale];

/// Alle sieben neuen Aktionen.
const NEW_ACTIONS: [Action; 7] = [
    Action::SketchFillet,
    Action::SketchChamferEqual,
    Action::SketchChamferDistanceAngle,
    Action::SketchChamferTwoDistances,
    Action::SketchOffset,
    Action::SketchMoveCopy,
    Action::SketchScale,
];

// ---- Hilfen ---------------------------------------------------------------------------------

fn workspace(open: bool) -> Workspace {
    let root = common::temp_root("planora-sketch-modify-tools");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    let s = &mut w.active_tab_mut().state;
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    if open {
        s.open_sketch(sk).unwrap();
    }
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

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
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
            app: PlanoraApp::for_tests(workspace(true)),
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

    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    fn strip(&self, label: &str) -> Pos2 {
        sketch_panel::strip_buttons(&self.ctx)
            .into_iter()
            .find(|(l, _)| *l == label)
            .unwrap_or_else(|| panic!("Werkzeug „{label}“ fehlt"))
            .1
            .center()
    }

    fn choose_tool(&mut self, tool: Tool) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.choose_tool(sk, tool);
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

/// Der Text der Zeilen eines Menüs, wie sie auf dem Schirm stehen.
fn menu_lines(w: &mut Workspace, edit: bool) -> Vec<String> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut texts = String::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            if edit {
                app::edit_menu(ui, w);
            } else {
                app::create_menu(ui, w);
            }
        });
        texts = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    texts.lines().map(str::to_owned).collect()
}

// ---- Leiste ---------------------------------------------------------------------------------

#[test]
fn strip_has_eighteen_buttons_in_english_that_all_fit_at_full_hd() {
    with_lang(&english(), || {
        let rig = Rig::new();
        let buttons = sketch_panel::strip_buttons(&rig.ctx);
        let names: Vec<&str> = buttons.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "Select",
                "Point",
                "Line",
                "Rectangle",
                "Circle",
                "Arc",
                "Polygon",
                "Slot",
                "Ellipse",
                "Spline",
                "Copy body edge",
                "Trim",
                "Extend",
                "Break",
                "Fillet",
                "Offset",
                "Move/Copy",
                "Sketch Scale",
            ]
        );
        assert_eq!(shell::STRIP_TOOLS, 18);
        for (name, r) in &buttons {
            assert!(
                r.top() >= 0.0 && r.bottom() < SIZE.y - shell::BOTTOM_H,
                "{name}: {r:?}"
            );
            assert_eq!(r.height(), 36.0, "{name}");
        }
        for pair in buttons.windows(2) {
            assert!(pair[0].1.bottom() <= pair[1].1.top(), "{}", pair[1].0);
        }
        // Trennlinie vor „Trim“, sonst überall der gleiche Abstand.
        let gap = |i: usize| buttons[i + 1].1.top() - buttons[i].1.bottom();
        for i in 0..17 {
            let want = if i == 10 { 13.0 } else { 4.0 };
            assert!((gap(i) - want).abs() < 0.01, "Abstand {i}: {}", gap(i));
        }
    });
}

#[test]
fn only_the_forms_with_kinds_and_the_fillet_have_a_triangle() {
    let with: Vec<Tool> = sketch_panel::TOOL_ORDER
        .into_iter()
        .filter(|t| sketch_panel::has_flyout(*t))
        .collect();
    assert_eq!(
        with,
        [
            Tool::Rectangle,
            Tool::Circle,
            Tool::Arc,
            Tool::Polygon,
            Tool::Slot,
            Tool::Fillet
        ]
    );
    assert_eq!(
        ToolKind::of(Tool::Fillet),
        [
            ToolKind::Fillet,
            ToolKind::ChamferEqual,
            ToolKind::ChamferDistanceAngle,
            ToolKind::ChamferTwoDistances
        ]
    );
    for &k in ToolKind::of(Tool::Fillet) {
        assert_eq!(k.tool(), Tool::Fillet, "{k:?}");
        assert!(!k.is_classic(), "{k:?}");
    }
    // Die Stufen nach Entscheidung 13.
    assert_eq!(ToolKind::Fillet.stages(), [&[][..], &[], &[F::Radius]]);
    assert_eq!(
        ToolKind::ChamferEqual.stages(),
        [&[][..], &[], &[F::Distance]]
    );
    assert_eq!(
        ToolKind::ChamferDistanceAngle.stages(),
        [&[][..], &[], &[F::Distance, F::Angle]]
    );
    assert_eq!(
        ToolKind::ChamferTwoDistances.stages(),
        [&[][..], &[], &[F::Distance, F::Distance2]]
    );
}

#[test]
fn the_strip_buttons_choose_the_new_tools() {
    let mut rig = Rig::new();
    for tool in NEW_TOOLS {
        let p = rig.strip(&sketch_panel::tool_label(tool));
        rig.click(p);
        assert_eq!(rig.se().tool, tool, "{tool:?}");
        let p = rig.strip(&sketch_panel::tool_label(Tool::Select));
        rig.click(p);
    }
    assert_eq!(rig.se().kinds.fillet, ToolKind::Fillet);
}

// ---- Aktionen und Kürzel --------------------------------------------------------------------

#[test]
fn f_o_and_m_choose_and_the_rest_have_no_key_but_can_be_bound() {
    let km = Keymap::defaults();
    for (a, k) in [
        (Action::SketchFillet, Key::F),
        (Action::SketchOffset, Key::O),
        (Action::SketchMoveCopy, Key::M),
    ] {
        assert_eq!(km.action_for_key(Area::Sketch, &chord(k)), Some(a), "{k:?}");
    }
    for a in [
        Action::SketchChamferEqual,
        Action::SketchChamferDistanceAngle,
        Action::SketchChamferTwoDistances,
        Action::SketchScale,
    ] {
        assert!(a.default_bindings().is_empty(), "{a:?}");
    }
    assert!(km.to_overrides().is_empty(), "Konfiguration bleibt leer");

    let mut ids = Vec::new();
    for a in NEW_ACTIONS {
        assert_eq!(a.area(), Area::Sketch, "{a:?}");
        assert!(a.repeatable(), "{a:?}");
        assert!(!ids.contains(&a.id()), "{a:?}: Id doppelt");
        ids.push(a.id());
        assert_eq!(Action::from_id(a.id()), Some(a));
        let mut km = Keymap::defaults();
        assert_eq!(
            km.set(a, 0, Binding::Key(chord(Key::F9))),
            SetResult::Ok,
            "{a:?}"
        );
        for lang in [german(), english()] {
            with_lang(&lang, || {
                let label = a.label();
                assert!(!label.is_empty() && !label.starts_with("action-"), "{a:?}");
                assert!(search::matches(a, &km, &label), "{a:?}: {label}");
            });
        }
    }
    assert_eq!(
        NEW_ACTIONS.map(|a| a.id()),
        [
            "skizze.rundung",
            "skizze.fase_gleich",
            "skizze.fase_winkel",
            "skizze.fase_zwei",
            "skizze.versatz",
            "skizze.verschieben",
            "skizze.massstab"
        ]
    );
}

#[test]
fn every_new_action_chooses_its_tool_and_kind() {
    for a in NEW_ACTIONS {
        let mut rig = Rig::new();
        let mut km = rig.ws().keymap.clone();
        assert_eq!(
            km.set(a, 0, Binding::Key(chord(Key::F9))),
            SetResult::Ok,
            "{a:?}"
        );
        rig.ws().keymap = km;
        rig.press(Key::F9);
        let tool = a.sketch_tool().unwrap();
        assert_eq!(rig.se().tool, tool, "{a:?}");
        if let Some(kind) = a.sketch_kind() {
            assert_eq!(rig.se().kind_of(tool), Some(kind), "{a:?}");
        }
    }
    // F, O, M ohne Umbelegung.
    let mut rig = Rig::new();
    for (k, tool) in [
        (Key::F, Tool::Fillet),
        (Key::O, Tool::Offset),
        (Key::M, Tool::MoveCopy),
    ] {
        rig.press(Key::Escape);
        rig.press(k);
        assert_eq!(rig.se().tool, tool, "{k:?}");
    }
}

#[test]
fn f_is_the_model_fillet_in_the_model_and_the_sketch_fillet_in_the_sketch() {
    let km = Keymap::defaults();
    assert_eq!(
        km.action_for_key(Area::Model, &chord(Key::F)),
        Some(Action::Fillet)
    );
    assert_eq!(
        km.action_for_key(Area::Sketch, &chord(Key::F)),
        Some(Action::SketchFillet)
    );
    let f = Binding::Key(chord(Key::F));
    assert_eq!(km.conflict_for(Action::SketchFillet, &f), None);
    assert_eq!(km.conflict_for(Action::Fillet, &f), None);
    // O und M gehören nur der Skizze, in der Zeichnung ist M frei (kein Verschieben-Werkzeug).
    for (a, k) in [
        (Action::SketchOffset, Key::O),
        (Action::SketchMoveCopy, Key::M),
    ] {
        assert_eq!(km.conflict_for(a, &Binding::Key(chord(k))), None, "{a:?}");
    }
    assert_eq!(Action::Fillet.area(), Area::Model);
    assert_eq!(Action::SketchFillet.area(), Area::Sketch);
}

#[test]
fn repeat_last_takes_the_chamfer_kind_after_its_first_change() {
    let mut rig = Rig::new();
    rig.choose_tool(Tool::Fillet);
    {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.choose_kind(sk, ToolKind::ChamferTwoDistances);
    }
    rig.settle();
    assert_eq!(
        rig.ws().last_command.sketch_tool(),
        None,
        "noch nichts geändert"
    );
    // Eine gelungene Änderung des Werkzeugs, wie sie die Tasks 7 und 8 zählen.
    rig.se().modify_edits = (1, Some(Tool::Fillet));
    rig.settle();
    assert_eq!(
        rig.ws().last_command.sketch_tool(),
        Some(Action::SketchChamferTwoDistances)
    );
    rig.press(Key::Escape);
    assert_eq!(rig.se().tool, Tool::Select);
    rig.press(Key::Space);
    assert_eq!(rig.se().tool, Tool::Fillet);
    assert_eq!(rig.se().planned_kind(), Some(ToolKind::ChamferTwoDistances));
    assert_eq!(rig.se().kinds.fillet, ToolKind::ChamferTwoDistances);
}

// ---- Menüs ----------------------------------------------------------------------------------

#[test]
fn menus_show_the_modify_entries_only_with_an_open_sketch() {
    let mut open = workspace(true);
    let lines = menu_lines(&mut open, true);
    assert!(lines.iter().any(|l| l == "Ändern"), "{lines:?}");
    let lines = menu_lines(&mut open, false);
    assert!(lines.iter().any(|l| l == "Versatz"), "{lines:?}");
    assert!(
        !lines
            .iter()
            .any(|l| l == "Rundung" || l == "Skizzenrundung"),
        "Rundung und Fasen stehen unter „Ändern“: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "O"),
        "Taste des Versatzes: {lines:?}"
    );

    let mut closed = workspace(false);
    let lines = menu_lines(&mut closed, true);
    assert!(!lines.iter().any(|l| l == "Ändern"), "{lines:?}");
    let lines = menu_lines(&mut closed, false);
    assert!(!lines.iter().any(|l| l == "Versatz"), "{lines:?}");
}

#[test]
fn menu_entries_run_the_actions() {
    // Der Eintrag „Versatz“ ruft die Aktion; die Untermenü-Einträge tun dasselbe über
    // `run_action`.
    let mut rig = Rig::new();
    for a in NEW_ACTIONS {
        rig.press(Key::Escape);
        planora_app::commands::run_action(rig.ws(), a);
        assert_eq!(rig.se().tool, a.sketch_tool().unwrap(), "{a:?}");
    }
}

// ---- Esc ------------------------------------------------------------------------------------

#[test]
fn escape_in_each_new_tool_ends_it_and_leaves_the_sketch_alone() {
    for tool in NEW_TOOLS {
        let mut rig = Rig::new();
        rig.choose_tool(tool);
        assert!(tool.is_modify(), "{tool:?}");
        rig.settle();
        let (before, steps) = (rig.fingerprint(), rig.undo_len());
        rig.press(Key::Escape);
        assert_eq!(rig.se().tool, Tool::Select, "{tool:?}");
        assert_eq!(rig.fingerprint(), before, "{tool:?}: Skizze bitgleich");
        assert_eq!(rig.undo_len(), steps, "{tool:?}: kein Rückgängig-Schritt");
    }
}

#[test]
fn the_new_tools_are_drawing_tools_and_a_click_changes_nothing_yet() {
    for tool in NEW_TOOLS {
        let mut rig = Rig::new();
        rig.choose_tool(tool);
        rig.settle();
        assert!(rig.se().is_drawing_tool(), "{tool:?}");
        let (before, steps) = (rig.fingerprint(), rig.undo_len());
        rig.click(pos2(900.0, 500.0));
        rig.click(pos2(1000.0, 600.0));
        assert_eq!(rig.fingerprint(), before, "{tool:?}");
        assert_eq!(rig.undo_len(), steps, "{tool:?}");
        assert!(rig.se().pending_pos.is_empty(), "{tool:?}");
    }
}

#[test]
fn changing_the_tool_resets_the_state_of_the_new_tools() {
    let mut rig = Rig::new();
    rig.choose_tool(Tool::MoveCopy);
    {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.transform = TransformState {
            stage: TransformStage::Active,
            factor: 2.0,
            ..Default::default()
        };
        se.fillet_run = None;
        se.pick_points.push(planora_math::DVec2::new(1.0, 2.0));
        se.choose_tool(sk, Tool::Scale);
    }
    let se = rig.se();
    assert_eq!(se.transform, TransformState::default());
    assert!(se.pick_points.is_empty());
    assert!(se.grip_drag.is_none() && se.fillet_run.is_none());
    assert!(se.offset_chain, "Vorgabe: ganze Kette");
    let fresh = SketchSession::new(FeatureId(0));
    assert!(fresh.offset_chain && fresh.pick_points.is_empty());
    assert_eq!(fresh.transform.stage, TransformStage::Picking);
}

// ---- Felder ---------------------------------------------------------------------------------

/// Tippt `text` in ein Feld der Art `kind` und liefert es.
fn typed(kind: F, text: &str) -> DrawField {
    let mut se = SketchSession::new(FeatureId(0));
    se.input.fields = vec![DrawField {
        kind,
        text: String::new(),
        touched: false,
        locked: None,
        error: None,
    }];
    se.type_field(0, text, &ParamScope::default());
    se.input.fields.remove(0)
}

#[test]
fn the_new_fields_have_units_and_names_in_both_languages() {
    assert_eq!(F::Distance.unit(), ParamUnit::Length);
    assert_eq!(F::Distance2.unit(), ParamUnit::Length);
    assert_eq!(F::OffsetDistance.unit(), ParamUnit::Length);
    assert_eq!(F::MoveX.unit(), ParamUnit::Length);
    assert_eq!(F::MoveY.unit(), ParamUnit::Length);
    assert_eq!(F::MoveAngle.unit(), ParamUnit::Angle);
    assert_eq!(F::ScaleFactor.unit(), ParamUnit::Number);
    let all = [
        F::Distance,
        F::Distance2,
        F::OffsetDistance,
        F::MoveX,
        F::MoveY,
        F::MoveAngle,
        F::ScaleFactor,
    ];
    for lang in [german(), english()] {
        with_lang(&lang, || {
            let mut names = Vec::new();
            for k in all {
                let label = k.label();
                assert!(
                    !label.is_empty() && !label.starts_with("sketch-field"),
                    "{k:?}"
                );
                assert!(!names.contains(&label), "{label} doppelt");
                names.push(label);
            }
        });
    }
    for key in [
        "sketch-field-offset-nonzero",
        "sketch-field-factor-positive",
        "menu-sketch-modify",
        "undo-fillet",
        "undo-chamfer",
        "undo-offset",
        "undo-move-copy",
        "undo-scale",
    ] {
        for lang in [german(), english()] {
            with_lang(&lang, || assert_ne!(t(key), key, "{key}"));
        }
    }
    // Die Felder zählen als eigene Felder: dieselbe Tabelle wie bei den Stufen.
    assert_eq!(all.len(), 7);
}

#[test]
fn move_fields_take_zero_and_negative_values() {
    for kind in [F::MoveX, F::MoveY] {
        for (text, want) in [("-3", -3.0), ("0", 0.0), ("2.5", 2.5)] {
            let f = typed(kind, text);
            assert_eq!(f.error, None, "{kind:?} {text}");
            assert_eq!(f.locked, Some(want), "{kind:?} {text}");
        }
        assert!(typed(kind, "abc").error.is_some());
        assert_eq!(typed(kind, "").locked, None);
    }
    // Drehung in Grad, wird Bogenmaß; Vorzeichen erlaubt.
    for (text, want) in [
        ("90", std::f64::consts::FRAC_PI_2),
        ("-45", -std::f64::consts::FRAC_PI_4),
        ("0", 0.0),
    ] {
        let f = typed(F::MoveAngle, text);
        assert_eq!(f.error, None, "{text}");
        assert!((f.locked.unwrap() - want).abs() < 1e-12, "{text}");
    }
}

#[test]
fn offset_field_takes_signs_but_not_zero() {
    let f = typed(F::OffsetDistance, "0");
    assert_eq!(f.locked, None);
    assert_eq!(
        f.error.as_deref(),
        Some(t("sketch-field-offset-nonzero").as_str())
    );
    let f = typed(F::OffsetDistance, "-5");
    assert_eq!((f.locked, f.error), (Some(-5.0), None));
    let f = typed(F::OffsetDistance, "-5");
    assert_eq!((f.locked, f.error), (Some(-5.0), None));
    let f = typed(F::OffsetDistance, "5");
    assert_eq!((f.locked, f.error), (Some(5.0), None));
    // Ungültiger Text gibt frei.
    let f = typed(F::OffsetDistance, "x");
    assert!(f.locked.is_none() && f.error.is_some());
}

#[test]
fn scale_factor_is_positive_and_unitless_also_in_inches() {
    for text in ["0", "-1"] {
        let f = typed(F::ScaleFactor, text);
        assert_eq!(f.locked, None, "{text}");
        assert_eq!(
            f.error.as_deref(),
            Some(t("sketch-field-factor-positive").as_str()),
            "{text}"
        );
    }
    assert_eq!(typed(F::ScaleFactor, "2").locked, Some(2.0));
    assert_eq!(typed(F::ScaleFactor, "0.5").locked, Some(0.5));
    with_unit(DisplayUnit::Zoll, || {
        let f = typed(F::ScaleFactor, "2");
        assert_eq!(
            (f.locked, f.error),
            (Some(2.0), None),
            "Faktor ohne Einheit"
        );
        // Das Maß der Fase ist eine Länge: 1 Zoll sind 25,4 mm.
        for kind in [F::Distance, F::Distance2] {
            let f = typed(kind, "1");
            assert_eq!((f.locked, f.error), (Some(25.4), None), "{kind:?}");
        }
        let f = typed(F::OffsetDistance, "-1");
        assert_eq!(f.locked, Some(-25.4));
        let f = typed(F::MoveX, "1");
        assert_eq!(f.locked, Some(25.4));
        // Die Drehung kennt keine Zoll.
        let f = typed(F::MoveAngle, "180");
        assert!((f.locked.unwrap() - std::f64::consts::PI).abs() < 1e-12);
    });
}

#[test]
fn chamfer_distances_must_be_positive_like_every_length() {
    for kind in [F::Distance, F::Distance2] {
        for bad in ["0", "-1"] {
            let f = typed(kind, bad);
            assert_eq!(f.locked, None, "{kind:?} {bad}");
            assert_eq!(
                f.error.as_deref(),
                Some(t("sketch-field-positive").as_str()),
                "{kind:?} {bad}"
            );
        }
        assert_eq!(typed(kind, "4").locked, Some(4.0));
    }
}

#[test]
fn the_stage_of_a_chamfer_offers_its_fields() {
    // Zwei gewählte Kurven (die Stufenmaschine füllt das mit Task 8): Stufe 2 hat die Felder der
    // Art, Rückgängig-sicher über den Schlüssel der Felder.
    let mut sk = planora_sketch::model::Sketch::new(
        planora_math::Frame::WORLD,
        "S",
        planora_math::Tolerance::default(),
    );
    let mut line = |from: (f64, f64), to: (f64, f64)| {
        let p = sk.add_point(from.0, from.1).unwrap();
        let q = sk.add_point(to.0, to.1).unwrap();
        sk.add_line(p, q).unwrap()
    };
    let (a, b) = (
        line((0.0, 0.0), (10.0, 0.0)),
        line((10.0, 0.0), (10.0, 10.0)),
    );
    let mut se = SketchSession::new(FeatureId(0));
    for (kind, fields) in [
        (ToolKind::Fillet, vec![F::Radius]),
        (ToolKind::ChamferEqual, vec![F::Distance]),
        (ToolKind::ChamferDistanceAngle, vec![F::Distance, F::Angle]),
        (
            ToolKind::ChamferTwoDistances,
            vec![F::Distance, F::Distance2],
        ),
    ] {
        se.choose_kind(&mut sk, kind);
        se.sync_input(&sk);
        assert!(se.input.fields.is_empty(), "{kind:?}: Stufe 0 ohne Felder");
        se.pending_curves = vec![a, b];
        se.sync_input(&sk);
        let got: Vec<F> = se.input.fields.iter().map(|f| f.kind).collect();
        assert_eq!(got, fields, "{kind:?}");
    }
}
