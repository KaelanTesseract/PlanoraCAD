//! Skizzen 5, Task 12: Muster, Skizzenbemaßung und die drei neuen Abhängigkeiten in Leiste,
//! Menü „Erzeugen“, Kürzeln und Einstellungen. Die ganze App läuft ohne Fenster bei 1920×1080.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::icons::{self, HandIcon, IconSource};
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult};
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_panel::{self, TOOL_ORDER};
use planora_app::sketch_state::{ConstraintAction, Item, SketchSession, Tool};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell, theme};
use planora_feature::{SketchPlane, StandardPlane};
use planora_i18n::{english, german, with_lang};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternKind};

const SIZE: Vec2 = vec2(1920.0, 1080.0);

struct Shot {
    texts: Vec<(String, Rect)>,
    /// Beschriftung und „gesperrt“ jedes Knotens für Bildschirmleser.
    nodes: Vec<(String, bool)>,
}

fn walk(shape: &Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
        Shape::Text(t) => out.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        _ => {}
    }
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let mut texts = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut texts);
        }
        Shot {
            texts,
            nodes: Vec::new(),
        }
    }

    fn has(&self, exact: &str) -> bool {
        self.texts.iter().any(|(t, _)| t == exact)
    }

    fn find(&self, exact: &str) -> Pos2 {
        self.texts
            .iter()
            .rfind(|(t, _)| t == exact)
            .unwrap_or_else(|| panic!("„{exact}“ fehlt"))
            .1
            .center()
    }

    /// „Gesperrt“ der Knöpfe mit der Beschriftung `label`.
    fn disabled(&self, label: &str) -> Vec<bool> {
        self.nodes
            .iter()
            .filter(|(l, _)| l == label)
            .map(|(_, d)| *d)
            .collect()
    }
}

fn pointer(p: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn plain() -> Rig {
        let root = common::temp_root("planora-sketch-s5-commands");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        ctx.enable_accesskit();
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        rig.settle();
        rig
    }

    /// Leere Skizze auf XY, offen.
    fn new() -> Rig {
        let mut rig = Rig::plain();
        let id = rig
            .st()
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        rig.st().open_sketch(id).unwrap();
        rig.settle();
        rig
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn se(&mut self) -> &SketchSession {
        self.st().sketch_session.as_ref().expect("Skizze offen")
    }

    fn sk(&mut self) -> &mut Sketch {
        self.st().session_sketch_mut().unwrap().1
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
        let mut out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut shot = Shot::of(&out.shapes);
        if let Some(update) = out.platform_output.accesskit_update.take() {
            for (_, node) in &update.nodes {
                if let Some(label) = node.label() {
                    shot.nodes.push((label.to_string(), node.is_disabled()));
                }
            }
        }
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self) -> Shot {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn click_with(&mut self, p: Pos2, b: PointerButton) -> Shot {
        // Kein Doppelklick mit dem vorigen Klick.
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![pointer(p, b, true)]);
        self.frame(vec![pointer(p, b, false)]);
        self.settle()
    }

    fn click(&mut self, p: Pos2) -> Shot {
        self.click_with(p, PointerButton::Primary)
    }

    fn press(&mut self, k: Key) -> Shot {
        self.frame(vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        self.settle()
    }

    fn strip_rect(&self, label: &str) -> Rect {
        sketch_panel::strip_buttons(&self.ctx)
            .into_iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("Werkzeug „{label}“ fehlt"))
            .1
    }

    fn entries(&self) -> Vec<(String, Rect)> {
        sketch_panel::flyout_entries(&self.ctx)
    }

    fn select(&mut self, items: Vec<Item>) {
        self.st().sketch_session.as_mut().unwrap().selection = items;
        self.settle();
    }
}

fn plain(k: Key) -> Binding {
    Binding::Key(KeyChord {
        key: k,
        ctrl: false,
        shift: false,
        alt: false,
    })
}

fn data(sk: &Sketch) -> String {
    format!("{:?}", sk.to_data())
}

// ---- Leiste ---------------------------------------------------------------------------------

#[test]
fn strip_has_twenty_buttons_that_all_fit_at_1920x1080_without_scrolling() {
    use Tool::*;
    assert_eq!(
        TOOL_ORDER[..],
        [
            Select,
            Point,
            Line,
            Rectangle,
            Circle,
            Arc,
            Polygon,
            Slot,
            Ellipse,
            Spline,
            ProjectEdge,
            Pattern,
            Dimension,
            Trim,
            Extend,
            Break,
            Fillet,
            Offset,
            MoveCopy,
            Scale,
        ]
    );
    assert_eq!(shell::STRIP_TOOLS, 20);
    // 20 × 36, 19 Abstände zu 4, Trennlinie 9, Rand 2 × 4 (Entscheidung 15).
    assert_eq!(shell::STRIP_H, 813.0);
    for (lang, pattern, dimension) in [
        (german(), "Muster", "Skizzenbemaßung"),
        (english(), "Pattern", "Sketch Dimension"),
    ] {
        with_lang(&lang, || {
            let rig = Rig::new();
            let buttons = sketch_panel::strip_buttons(&rig.ctx);
            assert_eq!(buttons.len(), 20);
            assert_eq!(buttons[11].0, pattern);
            assert_eq!(buttons[12].0, dimension);
            for (name, r) in &buttons {
                assert!(
                    r.top() >= 0.0 && r.bottom() < SIZE.y - shell::BOTTOM_H,
                    "{name}: {r:?}"
                );
                assert_eq!(r.height(), 36.0, "{name}");
            }
            // Keine Rolle: vom ersten bis zum letzten Knopf genau die Höhe ohne Rand.
            let span = buttons[19].1.bottom() - buttons[0].1.top();
            assert!((span - (shell::STRIP_H - 8.0)).abs() < 0.01, "{span}");
            // Trennlinie vor „Stutzen“ (Index 13), sonst überall 4.
            for i in 0..19 {
                let gap = buttons[i + 1].1.top() - buttons[i].1.bottom();
                let want = if i == 12 { 13.0 } else { 4.0 };
                assert!((gap - want).abs() < 0.01, "Abstand {i}: {gap}");
            }
        });
    }
}

#[test]
fn pattern_and_dimension_have_their_own_icons() {
    let rect = icons::sketch_kind_icon(ToolKind::PatternRect);
    let circ = icons::sketch_kind_icon(ToolKind::PatternCircular);
    assert_eq!(rect, IconSource::Hand(HandIcon::PatternRect));
    assert_eq!(circ, IconSource::Hand(HandIcon::PatternCircular));
    assert_eq!(
        icons::sketch_tool_icon(Tool::Dimension),
        IconSource::Hand(HandIcon::SketchDimension)
    );
    for h in [
        HandIcon::PatternRect,
        HandIcon::PatternCircular,
        HandIcon::SketchDimension,
    ] {
        assert!(icons::HAND_DRAWN.contains(&h), "{h:?}");
    }
}

#[test]
fn the_pattern_button_lists_two_kinds_and_remembers_the_last() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        assert!(sketch_panel::has_flyout(Tool::Pattern));
        assert!(!sketch_panel::has_flyout(Tool::Dimension));
        let button = rig.strip_rect("Muster");
        rig.click_with(button.center(), PointerButton::Secondary);
        let names: Vec<String> = rig.entries().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["Rechteckige Anordnung", "Kreisförmige Anordnung"]);
        let circular = rig.entries()[1].1.center();
        rig.click(circular);
        assert_eq!(rig.se().tool, Tool::Pattern);
        assert_eq!(
            rig.se().kind_of(Tool::Pattern),
            Some(ToolKind::PatternCircular)
        );
        rig.press(Key::Escape);
        rig.press(Key::Escape);
        assert_eq!(rig.se().tool, Tool::Select);
        // Ein Linksklick nimmt die zuletzt benutzte Art; der Name beim Überfahren nennt sie.
        rig.click(button.center());
        assert_eq!(rig.se().tool, Tool::Pattern);
        assert_eq!(
            rig.se().kind_of(Tool::Pattern),
            Some(ToolKind::PatternCircular)
        );
        let km = rig.app.ws_mut().keymap.clone();
        let se = rig.se();
        assert_eq!(
            sketch_panel::strip_title(&km, se, Tool::Pattern),
            "Kreisförmige Anordnung"
        );
        // Der Knopf der Leiste ist der Skizzenbemaßung: D wie in Fusion.
        rig.press(Key::Escape);
        rig.press(Key::Escape);
        let dim = rig.strip_rect("Skizzenbemaßung").center();
        rig.click(dim);
        assert_eq!(rig.se().tool, Tool::Dimension);
        let km = rig.app.ws_mut().keymap.clone();
        let se = rig.se();
        assert_eq!(
            sketch_panel::strip_title(&km, se, Tool::Dimension),
            "Skizzenbemaßung (D)"
        );
    });
}

// ---- Kürzel und Einstellungen ---------------------------------------------------------------

const SIX: [(Action, &str, &str, &str); 6] = [
    (
        Action::SketchPatternRect,
        "skizze.muster_rechteck",
        "Rechteckige Anordnung",
        "Rectangular Pattern",
    ),
    (
        Action::SketchPatternCircular,
        "skizze.muster_kreis",
        "Kreisförmige Anordnung",
        "Circular Pattern",
    ),
    (
        Action::SketchDimension,
        "skizze.bemassen",
        "Skizzenbemaßung",
        "Sketch Dimension",
    ),
    (
        Action::SketchConcentric,
        "skizze.konzentrisch",
        "Konzentrisch",
        "Concentric",
    ),
    (
        Action::SketchCollinear,
        "skizze.kollinear",
        "Kollinear",
        "Collinear",
    ),
    (Action::SketchSmooth, "skizze.glatt", "Glatt", "Smooth"),
];

#[test]
fn the_six_actions_are_bindable_and_only_d_has_a_default() {
    let km = Keymap::defaults();
    for (a, id, de, en) in SIX {
        assert_eq!(a.id(), id);
        assert_eq!(Action::from_id(id), Some(a));
        assert_eq!(a.area(), Area::Sketch, "{a:?}");
        with_lang(&german(), || assert_eq!(a.label(), de));
        with_lang(&english(), || assert_eq!(a.label(), en));
        let want = if a == Action::SketchDimension {
            vec![plain(Key::D)]
        } else {
            vec![]
        };
        assert_eq!(km.bindings(a), want.as_slice(), "{a:?}");
    }
    // Die Voreinstellung hat keine Doppelbelegung.
    for &a in Action::ALL {
        for b in km.bindings(a) {
            assert_eq!(km.conflict_for(a, b), None, "{a:?} {b:?}");
        }
    }
    // Jede der sechs lässt sich auf eine freie Taste legen; eine Taste der Skizze stößt sich.
    let mut km2 = km.clone();
    let free = [Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10];
    for ((a, ..), k) in SIX.into_iter().zip(free) {
        assert_eq!(km2.conflict_for(a, &plain(k)), None, "{a:?} {k:?}");
        assert_eq!(km2.set(a, 0, plain(k)), SetResult::Ok, "{a:?}");
        assert_eq!(
            km2.action_for_key(
                Area::Sketch,
                &KeyChord {
                    key: k,
                    ctrl: false,
                    shift: false,
                    alt: false
                }
            ),
            Some(a)
        );
    }
    assert_eq!(
        km.conflict_for(Action::SketchConcentric, &plain(Key::L)),
        Some(Action::SketchLine)
    );
}

#[test]
fn d_opens_the_sketch_dimension_only_inside_a_sketch() {
    let km = Keymap::defaults();
    let d = KeyChord {
        key: Key::D,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert_eq!(
        km.action_for_key(Area::Sketch, &d),
        Some(Action::SketchDimension)
    );
    assert_eq!(
        km.action_for_key(Area::Drawing, &d),
        Some(Action::DrawingDimension)
    );
    assert_eq!(km.action_for_key(Area::Model, &d), None);
    assert_eq!(km.action_for_key(Area::All, &d), None);
    let mut rig = Rig::new();
    rig.press(Key::D);
    assert_eq!(rig.se().tool, Tool::Dimension);
    // Ohne Skizze tut D im Modell nichts.
    let mut rig = Rig::plain();
    rig.press(Key::D);
    assert!(rig.st().sketch_session.is_none());
}

/// Zwei Kreise, zwei Linien, eine Linie mit einem Spline am gemeinsamen Ende.
fn three_pairs(sk: &mut Sketch) -> [[CurveId; 2]; 3] {
    let m1 = sk.add_point(0.0, 0.0).unwrap();
    let m2 = sk.add_point(30.0, 10.0).unwrap();
    let c1 = sk.add_circle(m1, 5.0).unwrap();
    let c2 = sk.add_circle(m2, 8.0).unwrap();
    let p = [
        sk.add_point(0.0, 40.0).unwrap(),
        sk.add_point(10.0, 41.0).unwrap(),
        sk.add_point(20.0, 45.0).unwrap(),
        sk.add_point(30.0, 49.0).unwrap(),
    ];
    let l1 = sk.add_line(p[0], p[1]).unwrap();
    let l2 = sk.add_line(p[2], p[3]).unwrap();
    let a = sk.add_point(-40.0, 0.0).unwrap();
    let j = sk.add_point(-30.0, 0.0).unwrap();
    let q1 = sk.add_point(-25.0, 5.0).unwrap();
    let q2 = sk.add_point(-20.0, 3.0).unwrap();
    let l3 = sk.add_line(a, j).unwrap();
    let sp = sk.add_spline(&[j, q1, q2]).unwrap();
    [[c1, c2], [l1, l2], [l3, sp]]
}

#[test]
fn concentric_collinear_and_smooth_over_a_bound_key_act_like_the_button() {
    with_lang(&german(), || {
        let pairs = [
            (
                Action::SketchConcentric,
                ConstraintAction::Concentric,
                Key::F6,
            ),
            (
                Action::SketchCollinear,
                ConstraintAction::Collinear,
                Key::F7,
            ),
            (Action::SketchSmooth, ConstraintAction::Smooth, Key::F8),
        ];
        // Über den Knopf: dieselbe Sitzungsfunktion wie im Skizzenfeld.
        let mut by_button = Rig::new();
        let curves = three_pairs(by_button.sk());
        by_button.settle();
        for (k, (_, action, _)) in pairs.iter().enumerate() {
            by_button.select(curves[k].iter().map(|&c| Item::Curve(c)).collect());
            let (se, sk) = by_button.st().session_sketch_mut().unwrap();
            se.apply(sk, *action, None).expect("passt");
            by_button.settle();
        }
        // Über eine belegte Taste.
        let mut by_key = Rig::new();
        for (a, _, key) in pairs {
            assert_eq!(
                by_key.app.ws_mut().keymap.set(a, 0, plain(key)),
                SetResult::Ok
            );
        }
        let curves2 = three_pairs(by_key.sk());
        assert_eq!(curves, curves2);
        by_key.settle();
        for (k, (_, _, key)) in pairs.iter().enumerate() {
            by_key.select(curves[k].iter().map(|&c| Item::Curve(c)).collect());
            let before = by_key.sk().constraints().count();
            by_key.press(*key);
            assert!(by_key.sk().constraints().count() > before, "{key:?}");
        }
        let a = data(by_button.sk());
        let b = data(by_key.sk());
        assert_eq!(a, b, "die Taste wirkt wie der Knopf");
    });
}

// ---- Menü „Erzeugen“ ------------------------------------------------------------------------

#[test]
fn the_create_menu_shows_the_three_entries_only_with_an_open_sketch() {
    with_lang(&german(), || {
        let three = [
            "Rechteckige Anordnung",
            "Kreisförmige Anordnung",
            "Skizzenbemaßung",
        ];
        let mut rig = Rig::plain();
        let shot = rig.settle();
        let shot = rig.click(shot.find("Erzeugen"));
        assert!(shot.has("Quader …"), "Menü offen");
        for name in three {
            assert!(!shot.has(name), "{name} ohne Skizze");
        }
        assert!(!shot.has("Muster"));

        let mut rig = Rig::new();
        for (name, tool, kind) in [
            (three[0], Tool::Pattern, Some(ToolKind::PatternRect)),
            (three[1], Tool::Pattern, Some(ToolKind::PatternCircular)),
            (three[2], Tool::Dimension, None),
        ] {
            rig.press(Key::Escape);
            rig.press(Key::Escape);
            let shot = rig.settle();
            let shot = rig.click(shot.find("Erzeugen"));
            for n in three {
                assert!(shot.has(n), "{n} im Menü");
            }
            // Direkt im Menü wie in Fusion, nicht in einem Untermenü „Muster“.
            assert!(!shot.has("Muster"), "kein Untermenü");
            let shot = rig.click(shot.find(name));
            assert!(!shot.has(name), "Menü zu");
            assert_eq!(rig.se().tool, tool, "{name}");
            if let Some(kind) = kind {
                assert_eq!(rig.se().kind_of(tool), Some(kind), "{name}");
            }
        }
    });
}

// ---- Aus früheren Prüfungen: Muster-Richtung 2 und Löschen in der Liste --------------------

/// Rechteck, Linie und laufendes Rechteckmuster mit dem Rechteck als Vorlage.
fn running_rect_pattern(rig: &mut Rig) -> CurveId {
    let ids = rig
        .sk()
        .add_rectangle(
            planora_math::DVec2::new(0.0, 0.0),
            planora_math::DVec2::new(10.0, 5.0),
        )
        .unwrap();
    let a = rig.sk().add_point(0.0, -20.0).unwrap();
    let b = rig.sk().add_point(10.0, -15.0).unwrap();
    let line = rig.sk().add_line(a, b).unwrap();
    rig.sk().solve();
    rig.settle();
    rig.select(ids.lines.iter().map(|&c| Item::Curve(c)).collect());
    planora_app::commands::run_action(rig.app.ws_mut(), Action::SketchPatternRect);
    rig.settle();
    assert_eq!(rig.se().tool, Tool::Pattern);
    line
}

fn second_dir(rig: &mut Rig) -> PatternDir {
    match rig.se().pattern_now().expect("Plan").kind {
        PatternKind::Rect { dirs, .. } => dirs[1],
        k => panic!("{k:?}"),
    }
}

#[test]
fn direction_two_can_go_back_to_perpendicular() {
    for (lang, label) in [
        (german(), "Senkrecht zu 1"),
        (english(), "Perpendicular to 1"),
    ] {
        with_lang(&lang, || {
            let mut rig = Rig::new();
            let line = running_rect_pattern(&mut rig);
            assert_eq!(second_dir(&mut rig), PatternDir::Perpendicular);
            {
                let se = rig.st().sketch_session.as_mut().unwrap();
                se.set_pattern_direction(1, PatternDir::Line(line));
            }
            let shot = rig.settle();
            assert_eq!(second_dir(&mut rig), PatternDir::Line(line));
            rig.click(shot.find(label));
            assert_eq!(second_dir(&mut rig), PatternDir::Perpendicular, "{label}");
            assert_eq!(rig.se().tool, Tool::Pattern, "das Werkzeug bleibt");
        });
    }
}

#[test]
fn the_row_delete_icon_is_disabled_while_the_pattern_runs() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        let a = rig.sk().add_point(0.0, 0.0).unwrap();
        let b = rig.sk().add_point(10.0, 0.0).unwrap();
        let l = rig.sk().add_line(a, b).unwrap();
        rig.sk()
            .add_constraint(planora_sketch::constraint::ConstraintKind::Horizontal(
                planora_sketch::constraint::LineOrPoints::Line(l),
            ))
            .unwrap();
        let shot = rig.settle();
        assert_eq!(shot.disabled("Abhängigkeit löschen"), [false]);
        running_rect_pattern(&mut rig);
        let shot = rig.settle();
        assert!(rig.se().runs_pattern());
        let rows = shot.disabled("Abhängigkeit löschen");
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|d| *d), "gesperrt: {rows:?}");
    });
}
