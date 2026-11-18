//! Skizzen 12, Task 7: die Anzeigeschalter des Skizzenfelds (Spec 1). Ein Schalter aus blendet
//! seine Geometrie aus: nicht gezeichnet, nicht wählbar, Konstruktion und Projektionen auch kein
//! Fang. Punkte aus lässt den Fang an End- und Mittelpunkten; Maße aus lässt das gerade gesetzte
//! Maß der Bemaßung; Abhängigkeiten aus lässt die Zeichen der automatischen Abhängigkeiten beim
//! Zeichnen. Fertige Skizzen folgen Konstruktion und Projektion. Kein Schalter beendet die Skizze.

use egui::Shape;
use egui::epaint::ClippedShape;
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::sketch_state::{Item, Snap, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::testing::{fill_triangles, line_triangles};
use planora_app::viewport::stroke_view;
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_view, theme};
use planora_feature::{CopySource, EdgeCopy, FeatureId, SketchDisplay, SketchPlane};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_render::StrokeVertex;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, PointId};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(W, H),
};
/// Fangradius in Skizzeneinheiten für die direkten Aufrufe.
const R: f64 = 0.5;

fn xy(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Was die Testskizze enthält.
struct Ids {
    /// Rechteck (0, 0)–(20, 10): Ecken und Linien, `lines[0]` unten.
    corners: Vec<PointId>,
    lines: Vec<CurveId>,
    /// Freier Punkt bei (40, 40).
    free: PointId,
    /// Konstruktionslinie (0, 30)–(20, 30) mit ihren Enden.
    construction: CurveId,
    construction_ends: [PointId; 2],
    /// Maß der unteren Linie (20).
    dimension: planora_sketch::constraint::ConstraintId,
}

fn open_sketch() -> (AppState, FeatureId, Ids) {
    let mut s = AppState::new();
    s.camera.set_viewport(W, H);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let ids = {
        let (_, sk) = s.session_sketch_mut().unwrap();
        let corners: Vec<_> = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)]
            .iter()
            .map(|&(x, y)| sk.add_point(x, y).unwrap())
            .collect();
        let lines: Vec<_> = (0..4)
            .map(|i| sk.add_line(corners[i], corners[(i + 1) % 4]).unwrap())
            .collect();
        let free = sk.add_point(40.0, 40.0).unwrap();
        let c0 = sk.add_point(0.0, 30.0).unwrap();
        let c1 = sk.add_point(20.0, 30.0).unwrap();
        let construction = sk.add_line(c0, c1).unwrap();
        sk.set_construction(construction, true).unwrap();
        let dimension = sk
            .add_constraint(K::Distance(corners[0], corners[1], 20.0))
            .unwrap();
        sk.add_constraint(K::Horizontal(LineOrPoints::Line(lines[0])))
            .unwrap();
        sk.solve();
        Ids {
            corners,
            lines,
            free,
            construction,
            construction_ends: [c0, c1],
            dimension,
        }
    };
    s.observe_sketch(false);
    (s, id, ids)
}

fn set(s: &mut AppState, id: FeatureId, f: impl FnOnce(&mut SketchDisplay)) {
    let mut d = s.document.sketch_display(id);
    f(&mut d);
    s.document.set_sketch_display(id, d);
}

/// Formen eines Overlay-Bilds; das letzte von drei zählt (der Zeiger kommt im zweiten).
fn overlay(ctx: &egui::Context, s: &mut AppState, pointer: Option<egui::Pos2>) -> Vec<Shape> {
    let mut shapes = Vec::new();
    for events in [
        vec![],
        pointer
            .map(|p| vec![egui::Event::PointerMoved(p)])
            .unwrap_or_default(),
        vec![],
    ] {
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            events,
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
        });
        shapes = out
            .shapes
            .iter()
            .map(|c: &ClippedShape| c.shape.clone())
            .collect();
        out.drop_without_applying_deltas();
    }
    shapes
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn walk<'a>(shapes: &'a [Shape], out: &mut Vec<&'a Shape>) {
    for s in shapes {
        match s {
            Shape::Vec(v) => walk(v, out),
            other => out.push(other),
        }
    }
}

fn circles(shapes: &[Shape]) -> usize {
    let mut all = Vec::new();
    walk(shapes, &mut all);
    all.iter().filter(|s| matches!(s, Shape::Circle(_))).count()
}

fn texts(shapes: &[Shape]) -> Vec<String> {
    let mut all = Vec::new();
    walk(shapes, &mut all);
    all.iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some(t.galley.text().to_string()),
            _ => None,
        })
        .collect()
}

fn strokes(s: &AppState) -> Vec<StrokeVertex> {
    let mut out = Vec::new();
    sketch_view::open_sketch_strokes(s, &stroke_view(s, SCREEN, 1.0), &mut out);
    out
}

fn finished_strokes(s: &AppState) -> Vec<StrokeVertex> {
    let mut out = Vec::new();
    sketch_view::finished_sketch_strokes(s, &stroke_view(s, SCREEN, 1.0), &mut out);
    out
}

/// Auswahl-Fang (Klick im Auswahlwerkzeug) an `at`.
fn select_snap(s: &AppState, at: DVec2) -> Snap {
    let se = s.sketch_session.as_ref().unwrap();
    se.snap(s.session_sketch().unwrap(), at, R)
}

/// Zeichen-Fang des Linienwerkzeugs an `at`.
fn draw_snap(s: &mut AppState, at: DVec2) -> Snap {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, Tool::Line);
    let snap = se.resolve_cursor(sk, at, R).snap;
    se.set_tool(sk, Tool::Select);
    snap
}

/// Rahmenauswahl (Fenster) von `a` bis `b`; liefert die Auswahl.
fn window_select(s: &mut AppState, a: DVec2, b: DVec2) -> Vec<Item> {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection.clear();
    se.select_rect3(sk, a, b, false, &|p| Some(p.truncate()));
    let out = se.selection.clone();
    se.selection.clear();
    out
}

fn screen_of(s: &AppState, at: DVec2) -> egui::Pos2 {
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let v = sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), at).unwrap();
    egui::pos2(v.x, v.y)
}

#[test]
fn points_off_hides_and_unpicks_points_but_keeps_endpoint_snap() {
    let ctx = context();
    let (mut s, id, ids) = open_sketch();
    let on = circles(&overlay(&ctx, &mut s, None));
    assert!(on >= 7, "Punkte gezeichnet ({on})");
    assert_eq!(select_snap(&s, xy(40.0, 40.0)), Snap::Point(ids.free));
    assert_eq!(
        window_select(&mut s, xy(39.0, 39.0), xy(41.0, 41.0)),
        vec![Item::Point(ids.free)]
    );

    set(&mut s, id, |d| d.points = false);
    assert_eq!(circles(&overlay(&ctx, &mut s, None)), 0, "keine Punkte");
    assert_eq!(select_snap(&s, xy(40.0, 40.0)), Snap::None, "freier Punkt");
    assert!(
        !matches!(select_snap(&s, xy(20.0, 0.0)), Snap::Point(_)),
        "Ecke nicht wählbar"
    );
    assert!(window_select(&mut s, xy(39.0, 39.0), xy(41.0, 41.0)).is_empty());
    // Beim Zeichnen fangen End- und Mittelpunkte weiter.
    assert_eq!(
        draw_snap(&mut s, xy(20.0, 0.1)),
        Snap::Point(ids.corners[1])
    );
    assert_eq!(
        draw_snap(&mut s, xy(10.0, 0.1)),
        Snap::Midpoint(ids.lines[0])
    );
}

#[test]
fn dimensions_off_hides_dimensions_except_new_one() {
    let ctx = context();
    let (mut s, id, ids) = open_sketch();
    overlay(&ctx, &mut s, None);
    assert!(
        sketch_view::label_hits(&ctx, id)
            .iter()
            .any(|(k, _)| *k == ids.dimension),
        "Maß gezeichnet und wählbar"
    );
    set(&mut s, id, |d| d.dimensions = false);
    let shapes = overlay(&ctx, &mut s, None);
    assert!(
        sketch_view::label_hits(&ctx, id).is_empty(),
        "kein Maß im Bild, keins zum Doppelklick"
    );
    assert!(
        !texts(&shapes).iter().any(|t| t.contains("20")),
        "{:?}",
        texts(&shapes)
    );
    // Das gerade gesetzte Maß der Bemaßung zeigt seinen Wert, bis es bestätigt ist.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Dimension);
        // Erst die Linie wählen, dann das Maß daneben setzen: Das Feld steht offen.
        se.click_dimension(sk, xy(20.0, 5.0), R);
        se.click_dimension(sk, xy(26.0, 5.0), R);
        assert!(se.dimension_preview(sk).is_some(), "Maß gesetzt");
    }
    let shapes = overlay(&ctx, &mut s, None);
    assert!(
        s.sketch_session
            .as_ref()
            .unwrap()
            .dimension_preview(s.session_sketch().unwrap())
            .is_some()
    );
    assert!(
        texts(&shapes).iter().any(|t| t.contains("10")),
        "Wert des neuen Maßes: {:?}",
        texts(&shapes)
    );
}

#[test]
fn constraints_off_hides_glyphs_keeps_auto_marks() {
    let ctx = context();
    let (mut s, id, _) = open_sketch();
    assert!(texts(&overlay(&ctx, &mut s, None)).contains(&"H".to_string()));
    set(&mut s, id, |d| d.constraints = false);
    let shown = texts(&overlay(&ctx, &mut s, None));
    assert!(!shown.contains(&"H".to_string()), "{shown:?}");
    assert!(
        shown.iter().any(|t| t.contains("20")),
        "Maße bleiben: {shown:?}"
    );
    // Linie zeichnen: Das Zeichen der automatischen Waagerechten erscheint weiter.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
        se.click(sk, xy(25.0, 20.0), R, false).unwrap();
    }
    s.observe_sketch(false);
    let p = screen_of(&s, xy(35.0, 20.2));
    let shown = texts(&overlay(&ctx, &mut s, Some(p)));
    assert!(shown.contains(&"H".to_string()), "{shown:?}");
}

#[test]
fn construction_off_hides_unpicks_unsnaps() {
    let ctx = context();
    let (mut s, id, ids) = open_sketch();
    let col = theme::sketch_colors(s.dark);
    let on_circles = circles(&overlay(&ctx, &mut s, None));
    assert!(line_triangles(&strokes(&s), col.construction) > 0);
    assert_eq!(
        select_snap(&s, xy(10.0, 30.0)),
        Snap::Curve(ids.construction)
    );
    assert_eq!(
        draw_snap(&mut s, xy(0.0, 30.1)),
        Snap::Point(ids.construction_ends[0])
    );

    set(&mut s, id, |d| d.construction = false);
    let off_circles = circles(&overlay(&ctx, &mut s, None));
    assert_eq!(line_triangles(&strokes(&s), col.construction), 0);
    assert_eq!(on_circles - off_circles, 2, "die Enden der Hilfslinie");
    assert_eq!(select_snap(&s, xy(10.0, 30.0)), Snap::None);
    assert_eq!(select_snap(&s, xy(0.0, 30.0)), Snap::None);
    assert!(window_select(&mut s, xy(-1.0, 29.0), xy(21.0, 31.0)).is_empty());
    for at in [xy(0.0, 30.1), xy(10.0, 30.1), xy(5.0, 30.1)] {
        let snap = draw_snap(&mut s, at);
        assert!(
            !matches!(snap, Snap::Point(_) | Snap::Midpoint(_) | Snap::Curve(_)),
            "{at}: {snap:?}"
        );
    }
    // Das Rechteck bleibt.
    assert_eq!(select_snap(&s, xy(10.0, 0.0)), Snap::Curve(ids.lines[0]));
}

#[test]
fn projected_off_hides_but_keeps_profiles() {
    let (mut s, id, ids) = open_sketch();
    let col = theme::sketch_colors(s.dark);
    {
        let (_, sf) = s.session_draft_mut().unwrap();
        for &l in &ids.lines {
            sf.copies.push(EdgeCopy::new(
                l,
                CopySource::SketchCurve {
                    sketch: id,
                    curve: l,
                },
            ));
        }
    }
    let ctx = context();
    let on = strokes(&s);
    assert!(line_triangles(&on, col.projected) >= 8, "lila Projektionen");
    assert!(fill_triangles(&on, col.profile_fill) >= 2);
    assert_eq!(select_snap(&s, xy(10.0, 0.0)), Snap::Curve(ids.lines[0]));
    let on_circles = circles(&overlay(&ctx, &mut s, None));

    set(&mut s, id, |d| d.projected = false);
    let off_circles = circles(&overlay(&ctx, &mut s, None));
    let off = strokes(&s);
    assert_eq!(line_triangles(&off, col.projected), 0, "ausgeblendet");
    assert!(
        fill_triangles(&off, col.profile_fill) >= 2,
        "das Profil zählt weiter"
    );
    assert_eq!(on_circles - off_circles, 4, "die Ecken");
    assert_eq!(select_snap(&s, xy(10.0, 0.0)), Snap::None);
    assert!(window_select(&mut s, xy(-1.0, -1.0), xy(21.0, 11.0)).is_empty());
    let snap = draw_snap(&mut s, xy(20.0, 0.1));
    assert!(
        !matches!(snap, Snap::Point(_) | Snap::Midpoint(_) | Snap::Curve(_)),
        "{snap:?}"
    );
}

#[test]
fn finished_sketch_follows_construction_and_projected() {
    let mut s = AppState::new();
    s.camera.set_viewport(W, H);
    s.camera.set_iso_view();
    let col = theme::sketch_colors(s.dark);
    // Erste Skizze mit einer Linie, die zweite projiziert sie.
    let a = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(a).unwrap();
    let source = {
        let (_, sk) = s.session_sketch_mut().unwrap();
        let p = sk.add_point(0.0, 50.0).unwrap();
        let q = sk.add_point(20.0, 50.0).unwrap();
        sk.add_line(p, q).unwrap()
    };
    s.close_sketch();
    let b = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(b).unwrap();
    {
        let (_, sf) = s.session_draft_mut().unwrap();
        let sk = &mut sf.sketch;
        let p = sk.add_point(0.0, 50.0).unwrap();
        let q = sk.add_point(20.0, 50.0).unwrap();
        let copy = sk.add_line(p, q).unwrap();
        let c = sk.add_point(0.0, 30.0).unwrap();
        let d = sk.add_point(20.0, 30.0).unwrap();
        let l = sk.add_line(c, d).unwrap();
        sk.set_construction(l, true).unwrap();
        let e = sk.add_point(0.0, 0.0).unwrap();
        let f = sk.add_point(20.0, 0.0).unwrap();
        sk.add_line(e, f).unwrap();
        sf.copies.push(EdgeCopy::new(
            copy,
            CopySource::SketchCurve {
                sketch: a,
                curve: source,
            },
        ));
    }
    s.close_sketch();
    let before = finished_strokes(&s);
    assert!(line_triangles(&before, col.construction) > 0);
    assert!(line_triangles(&before, col.projected) > 0);
    let plain = line_triangles(&before, col.finished);
    assert!(plain > 0);

    set(&mut s, b, |d| d.construction = false);
    let after = finished_strokes(&s);
    assert_eq!(line_triangles(&after, col.construction), 0);
    assert!(line_triangles(&after, col.projected) > 0);
    set(&mut s, b, |d| d.projected = false);
    let after = finished_strokes(&s);
    assert_eq!(line_triangles(&after, col.projected), 0);
    assert_eq!(
        line_triangles(&after, col.finished),
        plain,
        "übrige Linien bleiben"
    );
}

#[test]
fn switch_does_not_end_sketch() {
    let mut w = Workspace::new(None, Config::default());
    let id = {
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
        id
    };
    w.observe_history(false);
    let flag = |d: SketchDisplay, a: Action| match a {
        Action::SketchShowPoints => d.points,
        Action::SketchShowDimensions => d.dimensions,
        Action::SketchShowConstraints => d.constraints,
        Action::SketchShowConstruction => d.construction,
        Action::SketchShowProjected => d.projected,
        _ => unreachable!(),
    };
    for a in [
        Action::SketchShowPoints,
        Action::SketchShowDimensions,
        Action::SketchShowConstraints,
        Action::SketchShowConstruction,
        Action::SketchShowProjected,
    ] {
        let steps = w.active_tab().history().undo_len();
        commands::run_action(&mut w, a);
        w.observe_history(false);
        let s = &w.active_tab().state;
        assert!(
            !flag(s.document.sketch_display(id), a),
            "{a:?} schaltet aus"
        );
        assert_eq!(s.open_sketch_id(), Some(id), "{a:?}: Skizze bleibt offen");
        assert_eq!(s.sketch_session.as_ref().unwrap().tool, Tool::Line);
        assert_eq!(w.active_tab().history().undo_len(), steps + 1, "{a:?}");
        commands::run_action(&mut w, a);
        w.observe_history(false);
        assert!(flag(w.active_tab().state.document.sketch_display(id), a));
        assert_eq!(w.active_tab().state.open_sketch_id(), Some(id));
    }
    // Ohne offene Skizze tun die Befehle nichts.
    w.active_tab_mut().state.close_sketch();
    w.observe_history(false);
    let len = w.active_tab().history().undo_len();
    commands::run_action(&mut w, Action::SketchShowPoints);
    w.observe_history(false);
    assert!(w.active_tab().state.document.sketch_display(id).points);
    assert_eq!(w.active_tab().history().undo_len(), len);
}
