//! Navigationswürfel (Etappe C, Aufgabe 2): 26 Bereiche, Home-Knopf, Hervorhebung der aktuellen
//! Ansicht, Name unter dem Würfel, Ziehen dreht die Ansicht. Ohne Fenster: Rohereignisse je
//! Frame gegen einen egui-Kontext, geprüft werden die gemalten Formen. Tests, die Interna des
//! Würfels brauchen (Beschriftung, Schwellen), stehen in `src/viewport_cube_tests.rs`.

use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use planora_app::state::AppState;
use planora_app::{fonts, theme, viewport};
use planora_math::DVec3;
use planora_render::{CubeRegion, ViewFace, all_regions, cube_pieces_visible, current_region};

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(800.0, 600.0),
};

const FREE: Rect = Rect {
    min: pos2(278.0, 90.0),
    max: pos2(700.0, 500.0),
};

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn input(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

fn moved(p: Pos2) -> Event {
    Event::PointerMoved(p)
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Ein Frame wie in der App (3D-Bereich im ganzen Fenster, freies Rechteck `FREE`); liefert die
/// gemalten Formen.
fn frame(ctx: &egui::Context, s: &mut AppState, events: Vec<Event>) -> Vec<ClippedShape> {
    let out = ctx.run_ui(input(events), |ui| {
        viewport::show_in(ui, s, Some(FREE));
        viewport::remember_keyboard_focus(ui.ctx());
    });
    let shapes = out.shapes.clone();
    out.drop_without_applying_deltas();
    shapes
}

fn click(ctx: &egui::Context, s: &mut AppState, p: Pos2) {
    for events in [
        vec![moved(p)],
        vec![moved(p), button(p, true)],
        vec![button(p, false)],
        vec![],
    ] {
        frame(ctx, s, events);
    }
}

fn drag(ctx: &egui::Context, s: &mut AppState, from: Pos2, to: Pos2) {
    frame(ctx, s, vec![moved(from)]);
    frame(ctx, s, vec![moved(from), button(from, true)]);
    for i in 1..=6 {
        let p = from + (to - from) * (i as f32 / 6.0);
        frame(ctx, s, vec![moved(p)]);
    }
    frame(ctx, s, vec![button(to, false)]);
    frame(ctx, s, vec![]);
}

fn cube() -> Rect {
    viewport::cube_rect(FREE)
}

/// Klickpunkt (Schwerpunkt des anklickbaren Stücks) eines Bereichs.
fn point_of(s: &AppState, region: CubeRegion) -> Option<Pos2> {
    viewport::region_point(&s.camera, cube(), region)
}

/// Bereiche, die bei dieser Kamera mehr als nur streifend zu sehen sind.
fn visible_regions(s: &AppState) -> Vec<CubeRegion> {
    cube_pieces_visible(&s.camera, 0.02)
        .iter()
        .map(|p| p.region)
        .collect()
}

fn centroid(points: &[Pos2]) -> Pos2 {
    let n = points.len() as f32;
    let s = points.iter().fold(vec2(0.0, 0.0), |a, p| a + p.to_vec2());
    (s / n).to_pos2()
}

#[derive(Default)]
struct Shot {
    polys: Vec<(Vec<Pos2>, Color32)>,
    meshes: Vec<egui::Mesh>,
    texts: Vec<(String, Rect)>,
}

fn walk(shape: &Shape, s: &mut Shot) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
        Shape::Path(p) if p.closed => s.polys.push((p.points.clone(), p.fill)),
        Shape::Mesh(m) => s.meshes.push((**m).clone()),
        Shape::Text(t) => s.texts.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        _ => {}
    }
}

fn shot(shapes: &[ClippedShape]) -> Shot {
    let mut s = Shot::default();
    for c in shapes {
        walk(&c.shape, &mut s);
    }
    s
}

fn state_with_view(face: Option<ViewFace>) -> AppState {
    let mut s = AppState::new();
    match face {
        Some(f) => s.camera.set_view(f),
        None => s.camera.home(),
    }
    s
}

fn iso() -> CubeRegion {
    CubeRegion::Corner {
        x: ViewFace::Right,
        y: ViewFace::Front,
        z: ViewFace::Top,
    }
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn cube_sits_in_the_corner_of_the_free_area() {
    let c = viewport::cube_rect(FREE);
    assert_eq!(viewport::CUBE_SIZE, 100.0);
    assert_eq!(
        c,
        Rect::from_min_size(
            pos2(FREE.right() - 100.0 - 12.0, FREE.top() + 12.0),
            vec2(100.0, 100.0)
        )
    );
    let h = viewport::home_rect(c);
    assert_eq!(h.size(), vec2(26.0, 26.0));
    assert_eq!(h.top(), c.top(), "Home oben bündig mit dem Würfel");
}

// (b) ------------------------------------------------------------------------------------------

#[test]
fn click_on_each_region_sets_the_matching_view() {
    let mut covered = std::collections::HashSet::new();
    for start in [
        None,
        Some(ViewFace::Front),
        Some(ViewFace::Back),
        Some(ViewFace::Left),
        Some(ViewFace::Bottom),
    ] {
        let visible = visible_regions(&state_with_view(start));
        assert!(visible.len() >= 9, "{start:?}: {visible:?}");
        for r in visible {
            let ctx = context();
            let mut s = state_with_view(start);
            frame(&ctx, &mut s, vec![]);
            let at = point_of(&s, r).unwrap_or_else(|| panic!("{} nicht klickbar", r.name()));
            click(&ctx, &mut s, at);
            assert_eq!(
                current_region(&s.camera, 2.0),
                Some(r),
                "Start {start:?}: Klick auf {}",
                r.name()
            );
            covered.insert(r);
        }
    }
    assert_eq!(covered.len(), 26, "alle Bereiche direkt angeklickt");
}

#[test]
fn every_region_is_reachable_with_a_few_clicks() {
    // Unsichtbare Bereiche: erst den Würfel mit einem Klick drehen, dann den Bereich anklicken.
    for target in all_regions() {
        let ctx = context();
        let mut s = state_with_view(None);
        frame(&ctx, &mut s, vec![]);
        let mut reached = false;
        for _ in 0..4 {
            if let Some(at) = point_of(&s, target) {
                click(&ctx, &mut s, at);
                reached = current_region(&s.camera, 2.0) == Some(target);
                break;
            }
            // Zur Fläche drehen, die dem Ziel am nächsten liegt und sichtbar ist.
            let n = target.normal();
            let step = visible_regions(&s)
                .into_iter()
                .filter(|r| point_of(&s, *r).is_some())
                .max_by(|a, b| a.normal().dot(n).total_cmp(&b.normal().dot(n)))
                .unwrap();
            let at = point_of(&s, step).unwrap();
            click(&ctx, &mut s, at);
        }
        assert!(reached, "{} nicht erreichbar", target.name());
    }
}

// (c) ------------------------------------------------------------------------------------------

#[test]
fn home_click_sets_iso_and_fits() {
    let ctx = context();
    let mut s = AppState::new();
    s.create_box(DVec3::splat(100.0)).unwrap();
    frame(&ctx, &mut s, vec![]);
    s.camera.set_view(ViewFace::Front);
    s.camera.distance *= 4.0;
    s.camera.target += glam::Vec3::new(300.0, 0.0, 50.0);
    frame(&ctx, &mut s, vec![]);
    let before = s.camera;
    click(&ctx, &mut s, viewport::home_rect(cube()).center());
    assert_eq!(current_region(&s.camera, 2.0), Some(iso()));
    let got = s.camera;
    s.camera = before;
    s.camera.home();
    s.frame_all();
    assert!(
        (got.distance - s.camera.distance).abs() < 1e-3,
        "Abstand eingepasst"
    );
    assert!(
        (got.target - s.camera.target).length() < 1e-3,
        "Ziel eingepasst: {:?} statt {:?}",
        got.target,
        s.camera.target
    );
}

// (d) ------------------------------------------------------------------------------------------

#[test]
fn current_region_is_highlighted_in_accent_and_none_for_free_orbit() {
    for dark in [true, false] {
        let colors = theme::cube_colors(dark);
        for (start, region) in [
            (Some(ViewFace::Front), CubeRegion::Face(ViewFace::Front)),
            (Some(ViewFace::Top), CubeRegion::Face(ViewFace::Top)),
            (None, iso()),
        ] {
            let ctx = context();
            let mut s = state_with_view(start);
            s.dark = dark;
            let sh = shot(&frame(&ctx, &mut s, vec![]));
            let lit: Vec<_> = sh
                .polys
                .iter()
                .filter(|(_, f)| *f == colors.highlight)
                .collect();
            assert_eq!(lit.len(), 1, "{start:?}: genau ein Bereich hervorgehoben");
            let want = point_of(&s, region).unwrap();
            let got = &lit[0].0;
            let expected_corners = if matches!(region, CubeRegion::Corner { .. }) {
                3
            } else {
                4
            };
            assert_eq!(got.len(), expected_corners, "{start:?}");
            assert!(
                (centroid(got) - want).length() < 0.01,
                "{start:?}: {got:?} nicht um {want:?}"
            );
        }
        // Frei gedreht: nichts hervorgehoben.
        let ctx = context();
        let mut s = state_with_view(None);
        s.dark = dark;
        s.camera.orbit(37.0, 11.0);
        assert_eq!(current_region(&s.camera, 2.0), None);
        let sh = shot(&frame(&ctx, &mut s, vec![]));
        assert!(
            sh.polys.iter().all(|(_, f)| *f != colors.highlight),
            "frei gedreht ist kein Bereich hervorgehoben"
        );
        assert!(sh.polys.len() >= 7, "Würfel gemalt");
    }
}

// (e) ------------------------------------------------------------------------------------------

fn luminance(c: Color32) -> f32 {
    theme::contrast_ratio(c, Color32::TRANSPARENT)
}

#[test]
fn hover_highlights_lighter() {
    let dark = true;
    let colors = theme::cube_colors(dark);
    let ctx = context();
    let mut s = state_with_view(None);
    s.dark = dark;
    frame(&ctx, &mut s, vec![]);
    let front = CubeRegion::Face(ViewFace::Front);
    let at = point_of(&s, front).unwrap();
    let fill_at = |sh: &Shot| {
        sh.polys
            .iter()
            .find(|(pts, _)| pts.len() == 4 && (centroid(pts) - at).length() < 0.01)
            .map(|(_, f)| *f)
            .unwrap()
    };
    let rest = fill_at(&shot(&frame(&ctx, &mut s, vec![])));
    assert_eq!(rest, colors.face[1], "Vorne in Y-Tönung");
    frame(&ctx, &mut s, vec![moved(at)]);
    let hover = fill_at(&shot(&frame(&ctx, &mut s, vec![])));
    assert_ne!(hover, rest, "Hover ändert die Füllung");
    assert!(
        luminance(hover) > luminance(rest),
        "Hover ist heller: {hover:?} / {rest:?}"
    );
    // Die hervorgehobene Ecke wird beim Überfahren zu `highlight_hover`.
    let at = point_of(&s, iso()).unwrap();
    frame(&ctx, &mut s, vec![moved(at)]);
    let sh = shot(&frame(&ctx, &mut s, vec![]));
    assert!(sh.polys.iter().any(|(_, f)| *f == colors.highlight_hover));
    assert!(sh.polys.iter().all(|(_, f)| *f != colors.highlight));
}

// (g) ------------------------------------------------------------------------------------------

fn name_texts(sh: &Shot) -> Vec<String> {
    // Streifen unter dem Würfel (Würfelbreite, 16 px hoch).
    let c = cube();
    let r = Rect::from_min_size(c.left_bottom(), vec2(c.width(), 16.0));
    sh.texts
        .iter()
        .filter(|(t, rect)| !t.trim().is_empty() && r.expand(0.5).contains(rect.center()))
        .map(|(t, _)| t.clone())
        .collect()
}

#[test]
fn name_under_the_cube_follows_the_view() {
    let ctx = context();
    let mut s = state_with_view(Some(ViewFace::Front));
    let sh = shot(&frame(&ctx, &mut s, vec![]));
    assert_eq!(name_texts(&sh), vec!["Vorne".to_string()]);

    s.camera.set_view_region(CubeRegion::Edge {
        a: ViewFace::Top,
        b: ViewFace::Right,
    });
    let sh = shot(&frame(&ctx, &mut s, vec![]));
    assert_eq!(name_texts(&sh), vec!["Oben-Rechts".to_string()]);

    s.camera.orbit(23.0, 9.0);
    let sh = shot(&frame(&ctx, &mut s, vec![]));
    assert!(name_texts(&sh).is_empty(), "frei: kein Name");
}

// (h) ------------------------------------------------------------------------------------------

#[test]
fn clicks_on_the_cube_do_not_select_behind() {
    let ctx = context();
    let mut s = AppState::new();
    s.create_box(DVec3::splat(100.0)).unwrap();
    frame(&ctx, &mut s, vec![]);
    s.frame_all();
    // Ganz nah heran, damit der Körper auch hinter dem Würfel liegt.
    s.camera.distance *= 0.15;
    frame(&ctx, &mut s, vec![]);
    let c = cube();
    // Leere Ecke des Würfelfelds (kein Bereich), über dem Körper.
    let empty = c.left_bottom() + vec2(4.0, -4.0);
    let full = egui::Vec2::new(SCREEN.width(), SCREEN.height());
    assert!(
        s.pick(
            glam::Vec2::new(empty.x, empty.y),
            glam::Vec2::new(full.x, full.y)
        )
        .is_some(),
        "hinter dem Würfel liegt der Körper"
    );
    let (yaw, pitch) = (s.camera.yaw, s.camera.pitch);
    click(&ctx, &mut s, empty);
    assert_eq!(s.selection, None, "Klick auf das Würfelfeld wählt nichts");
    assert_eq!((s.camera.yaw, s.camera.pitch), (yaw, pitch));
    // Klick auf den Home-Knopf wählt ebenfalls nichts.
    click(&ctx, &mut s, viewport::home_rect(c).center());
    assert_eq!(s.selection, None, "Home wählt nichts");
    // Ziehen vom Home-Knopf aus dreht nicht (nur der Würfel selbst zieht die Ansicht).
    s.camera.distance *= 0.15;
    let (yaw, pitch) = (s.camera.yaw, s.camera.pitch);
    drag(
        &ctx,
        &mut s,
        viewport::home_rect(c).center(),
        c.center() + vec2(-60.0, 40.0),
    );
    assert_eq!(
        (s.camera.yaw, s.camera.pitch),
        (yaw, pitch),
        "kein Orbit vom Home-Knopf"
    );
    assert_eq!(s.selection, None);
    // Daneben dreht das Ziehen die Ansicht, und ein Klick daneben wählt.
    drag(&ctx, &mut s, pos2(350.0, 300.0), pos2(320.0, 325.0));
    assert_ne!(
        (s.camera.yaw, s.camera.pitch),
        (yaw, pitch),
        "Orbit daneben"
    );
    click(&ctx, &mut s, FREE.center());
    assert!(s.selection.is_some(), "Klick daneben wählt");
}

// (h1) -----------------------------------------------------------------------------------------

/// Zug mit der Primärtaste in `steps` gleichen Schritten, danach loslassen.
fn drag_in(ctx: &egui::Context, s: &mut AppState, from: Pos2, to: Pos2, steps: usize) {
    frame(ctx, s, vec![moved(from)]);
    frame(ctx, s, vec![moved(from), button(from, true)]);
    for i in 1..=steps {
        let p = from + (to - from) * (i as f32 / steps as f32);
        frame(ctx, s, vec![moved(p)]);
    }
    frame(ctx, s, vec![button(to, false)]);
    frame(ctx, s, vec![]);
}

#[test]
fn dragging_on_the_cube_orbits() {
    let c = cube();
    let top_right = CubeRegion::Face(ViewFace::Top);
    for sketching in [false, true] {
        let ctx = context();
        let mut s = AppState::new();
        s.create_box(DVec3::splat(100.0)).unwrap();
        if sketching {
            let id = s
                .add_sketch(planora_feature::SketchPlane::Standard(
                    planora_app::state::StandardPlane::XY,
                ))
                .unwrap();
            s.open_sketch(id).unwrap();
        }
        s.camera.home();
        frame(&ctx, &mut s, vec![]);
        s.camera.home();
        frame(&ctx, &mut s, vec![]);
        let what = if sketching { "Skizze" } else { "Modell" };

        // Ziehen über den Würfel (mehr als 4 px): die Ansicht dreht wie im Viewport. Nach links
        // und unten gezogen heißt: Gier wächst, Neigung wächst.
        let before = (s.camera.yaw, s.camera.pitch);
        let start = point_of(&s, top_right).unwrap();
        drag(&ctx, &mut s, start, start + vec2(-30.0, 12.0));
        assert!(s.camera.yaw > before.0 + 0.1, "{what}: Gier dreht");
        assert!(
            (s.camera.pitch - before.1).abs() > 0.01
                || s.camera.pitch == planora_render::OrbitCamera::MAX_PITCH,
            "{what}: Neigung dreht"
        );
        assert_eq!(s.selection, None, "{what}: Ziehen wählt nichts");
        // Das Loslassen löst keinen Regionsklick aus: die Ansicht springt nicht auf die Fläche.
        assert_eq!(
            current_region(&s.camera, 2.0),
            None,
            "{what}: nach dem Zug keine Regionsansicht"
        );

        // Auch ein kurzer Zug knapp über der Schwelle (5 px, ein Schritt) dreht und klickt nicht.
        s.camera.home();
        frame(&ctx, &mut s, vec![]);
        let before = (s.camera.yaw, s.camera.pitch);
        let start = point_of(&s, top_right).unwrap();
        drag_in(&ctx, &mut s, start, start + vec2(-5.0, 0.0), 1);
        assert_ne!(
            (s.camera.yaw, s.camera.pitch),
            before,
            "{what}: 5 px drehen"
        );
        assert_eq!(
            current_region(&s.camera, 2.0),
            None,
            "{what}: 5 px lösen keinen Klick aus"
        );

        // Ein Klick ohne Ziehen (auch mit 3 px Wackeln) wählt weiter die Region.
        s.camera.home();
        frame(&ctx, &mut s, vec![]);
        let before = (s.camera.yaw, s.camera.pitch);
        let start = point_of(&s, top_right).unwrap();
        drag_in(&ctx, &mut s, start, start + vec2(3.0, 0.0), 1);
        assert_eq!(
            current_region(&s.camera, 2.0),
            Some(top_right),
            "{what}: Klick wählt die Region ({before:?})"
        );

        // Mausrad über dem Würfel zoomt weiter.
        let dist = s.camera.distance;
        let p = c.center();
        frame(&ctx, &mut s, vec![moved(p)]);
        frame(
            &ctx,
            &mut s,
            vec![
                moved(p),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, 40.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(&ctx, &mut s, vec![]);
        assert_ne!(s.camera.distance, dist, "{what}: Mausrad zoomt");
    }
}

// (h3) -----------------------------------------------------------------------------------------

/// Alle Zahlen einer gemalten Form endlich?
fn finite(shape: &Shape) -> bool {
    match shape {
        Shape::Vec(v) => v.iter().all(finite),
        Shape::Path(p) => p.points.iter().all(|q| q.is_finite()),
        Shape::Mesh(m) => m.vertices.iter().all(|v| v.pos.is_finite()),
        Shape::Circle(c) => c.center.is_finite() && c.radius.is_finite(),
        Shape::LineSegment { points, .. } => points.iter().all(|q| q.is_finite()),
        Shape::Text(t) => t.pos.is_finite(),
        _ => true,
    }
}

#[test]
fn nan_camera_paints_nothing_invalid() {
    for (yaw, pitch) in [
        (f32::NAN, 0.5),
        (0.5, f32::NAN),
        (f32::INFINITY, 0.5),
        (0.5, f32::NEG_INFINITY),
    ] {
        let ctx = context();
        let mut s = AppState::new();
        frame(&ctx, &mut s, vec![]);
        s.camera.yaw = yaw;
        s.camera.pitch = pitch;
        // Mehrere Bilder, auch mit dem Zeiger auf dem Würfel (Hover).
        let mut shapes = frame(&ctx, &mut s, vec![]);
        shapes = [shapes, frame(&ctx, &mut s, vec![moved(cube().center())])].concat();
        shapes = [shapes, frame(&ctx, &mut s, vec![])].concat();
        for c in &shapes {
            assert!(finite(&c.shape), "yaw {yaw}, pitch {pitch}: {:?}", c.shape);
        }
        let sh = shot(&shapes);
        assert!(
            name_texts(&sh).is_empty(),
            "yaw {yaw}, pitch {pitch}: kein Name unter dem Würfel"
        );
        assert!(
            sh.polys.len() <= 1,
            "yaw {yaw}, pitch {pitch}: kein Würfelbereich gemalt, nur der Home-Kreis"
        );
    }
}

// (h4) -----------------------------------------------------------------------------------------

/// Namen aus `names` als ganzes Wort (nicht als Teil eines längeren Namens) in `text`?
fn mentions(text: &str, names: &[&str]) -> Vec<String> {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut found = Vec::new();
    for name in names {
        for (i, _) in text.match_indices(name) {
            let before = text[..i].chars().next_back();
            let after = text[i + name.len()..].chars().next();
            if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
                found.push((*name).to_string());
            }
        }
    }
    found
}

#[test]
fn cube_internals_are_not_public() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let viewport = std::fs::read_to_string(root.join("src/viewport.rs")).unwrap();
    for (kind, name) in [
        ("const", "CUBE_SCALE"),
        ("const", "LABEL_MIN_FACING"),
        ("const", "LABEL_FULL_FACING"),
        ("const", "PIECE_MIN_FACING"),
        ("const", "PIECE_FULL_FACING"),
        ("struct", "FaceFrame"),
        ("fn", "face_label_transform"),
        ("fn", "visible_pieces"),
        ("fn", "clickable_pieces"),
        ("fn", "cube_region_at"),
        ("fn", "piece_opacity"),
        ("fn", "label_opacity"),
        ("fn", "cube_to_screen"),
        ("fn", "name_rect"),
    ] {
        let decl = format!("{kind} {name}");
        let line = viewport
            .lines()
            .find(|l| l.trim_start().contains(&decl) && !l.trim_start().starts_with("//"))
            .unwrap_or_else(|| panic!("{decl} nicht gefunden"));
        assert!(
            line.trim_start().starts_with("pub(crate) "),
            "{decl} ist nicht auf `pub(crate)` beschränkt: {line}"
        );
    }
    // Öffentlich bleibt nur, was die Oberfläche braucht (und genau ein Testhaken).
    for decl in [
        "pub fn region_point",
        "pub const CUBE_SIZE",
        "pub fn cube_rect",
    ] {
        assert!(viewport.contains(decl), "{decl} fehlt");
    }
    let hook = viewport
        .lines()
        .position(|l| l.contains("pub fn region_point"))
        .unwrap();
    assert!(
        viewport
            .lines()
            .nth(hook - 1)
            .unwrap()
            .contains("#[doc(hidden)]"),
        "Testhaken ist versteckt"
    );

    let lib = std::fs::read_to_string(root.join("../planora-render/src/lib.rs")).unwrap();
    let leaked = mentions(
        &lib,
        &[
            "cube_faces",
            "cube_hit_test",
            "cube_pieces",
            "REGION_COUNT",
            "CubeFace",
        ],
    );
    assert!(
        leaked.is_empty(),
        "im Re-Export von planora-render: {leaked:?}"
    );
}

// (h2) ----------------------------------------------------------------------------------------

#[test]
fn wheel_over_cube_and_home_zooms_without_orbit() {
    let ctx = context();
    let mut s = AppState::new();
    s.create_box(DVec3::splat(100.0)).unwrap();
    frame(&ctx, &mut s, vec![]);
    s.frame_all();
    frame(&ctx, &mut s, vec![]);
    let c = cube();
    for (what, p) in [
        ("Würfel", c.center()),
        ("Home", viewport::home_rect(c).center()),
    ] {
        let (yaw, pitch, dist) = (s.camera.yaw, s.camera.pitch, s.camera.distance);
        frame(&ctx, &mut s, vec![moved(p)]);
        frame(
            &ctx,
            &mut s,
            vec![
                moved(p),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, 40.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(&ctx, &mut s, vec![]);
        assert_ne!(s.camera.distance, dist, "{what}: Mausrad zoomt");
        assert_eq!(
            (s.camera.yaw, s.camera.pitch),
            (yaw, pitch),
            "{what}: kein Orbit"
        );
    }
}

// (i) ------------------------------------------------------------------------------------------

#[test]
fn colours_in_both_themes() {
    let mut failures = Vec::new();
    for dark in [true, false] {
        let c = theme::cube_colors(dark);
        let p = theme::palette(dark);
        let bg = theme::viewport_background(dark);
        let name = if dark { "dunkel" } else { "hell" };
        let mut checks = vec![
            (
                "Text auf Hervorhebung",
                c.text_on_highlight,
                c.highlight,
                4.5,
            ),
            ("Name auf Viewport", c.text, bg, 4.5),
            (
                "Text auf Hervorhebung (Hover)",
                c.text_on_highlight,
                c.highlight_hover,
                4.5,
            ),
        ];
        for face in &c.face {
            checks.push(("Text auf Fläche", c.text, *face, 4.5));
            checks.push((
                "Text auf Fläche (Hover)",
                c.text,
                theme::cube_hover_fill(dark, *face),
                4.5,
            ));
            checks.push(("Fläche auf Viewport", *face, bg, 1.15));
            checks.push(("Linie auf Fläche", c.line, *face, 1.5));
        }
        for (what, fg, back, min) in checks {
            let r = theme::contrast_ratio(fg, back);
            if r < min {
                failures.push(format!("{name} {what}: {r:.2} < {min}"));
            }
        }
        // Flächen tragen ihre Achsenfarbe: der Rotanteil bei X, Grün bei Y, Blau bei Z ist der
        // größte unter den drei Flächen.
        assert!(
            c.face[0].r() > c.face[1].r() && c.face[0].r() > c.face[2].r(),
            "{name}"
        );
        assert!(c.face[1].g() > c.face[0].g(), "{name}");
        assert!(
            c.face[2].b() > c.face[0].b() && c.face[2].b() > c.face[1].b(),
            "{name}"
        );
        assert_eq!(c.highlight, p.accent, "{name}");
        assert_eq!(c.edge, p.bg_section, "{name}");
        assert_eq!(c.corner, p.bg_inset, "{name}");
        assert_ne!(c.highlight_hover, c.highlight, "{name}");
        assert_ne!(c.home_hover, c.home_fill, "{name}");
        // Flächen, Kanten und Ecken unterscheiden sich.
        for f in c.face {
            assert_ne!(f, c.edge, "{name}");
            assert_ne!(f, c.corner, "{name}");
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
