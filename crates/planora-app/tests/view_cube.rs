//! Navigationswürfel (Etappe C, Aufgabe 2): 26 Bereiche, Home-Knopf, Hervorhebung der aktuellen
//! Ansicht, Name unter dem Würfel, Beschriftung auf den Flächen. Ohne Fenster: Rohereignisse je
//! Frame gegen einen egui-Kontext, geprüft werden die gemalten Formen.

use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use planora_app::shell::{ShellState, layout};
use planora_app::state::AppState;
use planora_app::{fonts, theme, viewport};
use planora_math::DVec3;
use planora_render::{CubeRegion, OrbitCamera, ViewFace, all_regions, cube_pieces, current_region};

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

/// Bildschirm-Umriss eines sichtbaren Bereichs.
fn piece_on_screen(cam: &OrbitCamera, region: CubeRegion) -> Option<Vec<Pos2>> {
    cube_pieces(cam)
        .into_iter()
        .find(|p| p.region == region)
        .map(|p| {
            p.corners
                .iter()
                .map(|c| viewport::cube_to_screen(cube(), *c))
                .collect()
        })
}

fn centroid(points: &[Pos2]) -> Pos2 {
    let n = points.len() as f32;
    let s = points.iter().fold(vec2(0.0, 0.0), |a, p| a + p.to_vec2());
    (s / n).to_pos2()
}

/// Liegt `p` mindestens `margin` px innerhalb des konvexen Vielecks (Umlaufsinn egal)?
fn inside(poly: &[Pos2], p: Pos2, margin: f32) -> bool {
    let n = poly.len();
    let mut sign = 0.0_f32;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let e = b - a;
        let cross = e.x * (p.y - a.y) - e.y * (p.x - a.x);
        let dist = cross / e.length();
        if sign == 0.0 {
            sign = dist.signum();
        }
        if dist * sign < margin {
            return false;
        }
    }
    true
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
fn cube_rect_and_home_and_name_rects() {
    let c = viewport::cube_rect(FREE);
    assert_eq!(viewport::CUBE_SIZE, 84.0);
    assert_eq!(
        c,
        Rect::from_min_size(
            pos2(FREE.right() - 84.0 - 12.0, FREE.top() + 12.0),
            vec2(84.0, 84.0)
        )
    );
    let h = viewport::home_rect(c);
    assert_eq!(h.size(), vec2(26.0, 26.0));
    assert_eq!(h.top(), c.top(), "Home oben bündig mit dem Würfel");
    assert_eq!(
        h.right() - c.left(),
        2.0,
        "Home überlappt den Würfelrand um 2 px"
    );
    let n = viewport::name_rect(c);
    assert_eq!(n, Rect::from_min_size(c.left_bottom(), vec2(84.0, 16.0)));

    for (w, hgt) in [
        (800.0, 600.0),
        (1100.0, 700.0),
        (1400.0, 900.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
    ] {
        for read_only in [false, true] {
            let win = Rect::from_min_size(Pos2::ZERO, vec2(w, hgt));
            let r = layout(win, &ShellState::default(), 600.0, 520.0, 200.0, read_only);
            let cube = viewport::cube_rect(r.free);
            let mut fields = vec![
                ("oben", r.top),
                ("links", r.left),
                ("rechts", r.right),
                ("unten", r.bottom),
                ("Status", r.status),
            ];
            if let Some(b) = r.banner {
                fields.push(("Hinweis", b));
            }
            for (what, rect) in [
                ("Würfel", cube),
                ("Home", viewport::home_rect(cube)),
                ("Name", viewport::name_rect(cube)),
            ] {
                assert!(
                    r.free.contains_rect(rect),
                    "{what} liegt bei {w}x{hgt} nicht in der freien Fläche"
                );
                for (name, f) in &fields {
                    let i = rect.intersect(*f);
                    assert!(
                        !(i.width() > 0.0 && i.height() > 0.0),
                        "{what} überlappt {name} bei {w}x{hgt}: {rect:?} / {f:?}"
                    );
                }
            }
        }
    }
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
        let visible: Vec<CubeRegion> = cube_pieces(&state_with_view(start).camera)
            .iter()
            .map(|p| p.region)
            .collect();
        assert!(visible.len() >= 9, "{start:?}: {visible:?}");
        for r in visible {
            let ctx = context();
            let mut s = state_with_view(start);
            frame(&ctx, &mut s, vec![]);
            let poly = piece_on_screen(&s.camera, r).unwrap();
            click(&ctx, &mut s, centroid(&poly));
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
            if let Some(poly) = piece_on_screen(&s.camera, target) {
                click(&ctx, &mut s, centroid(&poly));
                reached = current_region(&s.camera, 2.0) == Some(target);
                break;
            }
            // Zur Fläche drehen, die dem Ziel am nächsten liegt und sichtbar ist.
            let n = target.normal();
            let step = cube_pieces(&s.camera)
                .into_iter()
                .max_by(|a, b| {
                    a.region
                        .normal()
                        .dot(n)
                        .total_cmp(&b.region.normal().dot(n))
                })
                .unwrap();
            let poly: Vec<Pos2> = step
                .corners
                .iter()
                .map(|c| viewport::cube_to_screen(cube(), *c))
                .collect();
            click(&ctx, &mut s, centroid(&poly));
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
            let want = piece_on_screen(&s.camera, region).unwrap();
            let got = &lit[0].0;
            assert_eq!(got.len(), want.len(), "{start:?}");
            for (a, b) in got.iter().zip(&want) {
                assert!(
                    (*a - *b).length() < 0.01,
                    "{start:?}: {got:?} statt {want:?}"
                );
            }
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
    let poly = piece_on_screen(&s.camera, front).unwrap();
    let at = centroid(&poly);
    let fill_at = |sh: &Shot| {
        sh.polys
            .iter()
            .find(|(pts, _)| pts.len() == poly.len() && (pts[0] - poly[0]).length() < 0.01)
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
    let corner = piece_on_screen(&s.camera, iso()).unwrap();
    frame(&ctx, &mut s, vec![moved(centroid(&corner))]);
    let sh = shot(&frame(&ctx, &mut s, vec![]));
    assert!(sh.polys.iter().any(|(_, f)| *f == colors.highlight_hover));
    assert!(sh.polys.iter().all(|(_, f)| *f != colors.highlight));
}

// (f) ------------------------------------------------------------------------------------------

/// Glyphen-Vierecke eines Textnetzes: (links oben, rechts oben, links unten).
fn quads(m: &egui::Mesh) -> Vec<[Pos2; 3]> {
    m.vertices
        .chunks(4)
        .filter(|q| q.len() == 4)
        .map(|q| [q[0].pos, q[1].pos, q[2].pos])
        .collect()
}

fn parallel(a: egui::Vec2, b: egui::Vec2) -> bool {
    let (a, b) = (a.normalized(), b.normalized());
    (a.x * b.y - a.y * b.x).abs() < 1e-3
}

/// Netz, das ganz in `poly` liegt.
fn label_in<'a>(sh: &'a Shot, poly: &[Pos2]) -> Option<&'a egui::Mesh> {
    sh.meshes
        .iter()
        .find(|m| !m.vertices.is_empty() && m.vertices.iter().all(|v| inside(poly, v.pos, 0.0)))
}

#[test]
fn labels_are_on_the_faces_and_follow_them() {
    let ctx = context();
    let mut s = state_with_view(Some(ViewFace::Front));
    let front = CubeRegion::Face(ViewFace::Front);
    let sh = shot(&frame(&ctx, &mut s, vec![]));
    assert_eq!(sh.meshes.len(), 1, "nur „Vorne“ ist zugewandt");
    let poly = piece_on_screen(&s.camera, front).unwrap();
    let m = label_in(&sh, &poly).expect("„Vorne“ liegt in der Fläche");
    assert!(m.vertices.len() >= 5 * 4, "fünf Buchstaben");
    assert!(m.vertices.iter().all(|v| inside(&poly, v.pos, 1.0)));

    // Gedreht: der Text bleibt auf der (jetzt schmaleren) Fläche und ist mit ihr verzerrt.
    for (yaw, pitch) in [(40.0_f32, 0.0_f32), (40.0, 25.0), (-30.0, -20.0)] {
        let mut s = state_with_view(Some(ViewFace::Front));
        s.camera.yaw += yaw.to_radians();
        s.camera.pitch += pitch.to_radians();
        let sh = shot(&frame(&ctx, &mut s, vec![]));
        let poly = piece_on_screen(&s.camera, front).unwrap();
        let m = label_in(&sh, &poly)
            .unwrap_or_else(|| panic!("yaw {yaw}, pitch {pitch}: Text verlässt die Fläche"));
        // Textachse parallel zur Flächenkante unten (Weltachse X), nicht waagerecht fest.
        let edge = poly[1] - poly[0];
        for q in quads(m) {
            assert!(
                parallel(q[1] - q[0], edge),
                "yaw {yaw}, pitch {pitch}: Grundlinie {:?} nicht parallel zu {edge:?}",
                q[1] - q[0]
            );
        }
        if pitch != 0.0 {
            assert!(edge.y.abs() > 1.0, "Kante schräg, also Text schräg");
        }
    }

    // Rückseite, Unterseite und alle anderen: nie gespiegelt.
    for face in ViewFace::ALL {
        let mut s = state_with_view(Some(face));
        let sh = shot(&frame(&ctx, &mut s, vec![]));
        let poly = piece_on_screen(&s.camera, CubeRegion::Face(face)).unwrap();
        let m = label_in(&sh, &poly).unwrap_or_else(|| panic!("{face:?}: Text fehlt"));
        for q in quads(m) {
            let (r, d) = (q[1] - q[0], q[2] - q[0]);
            assert!(r.x * d.y - r.y * d.x > 0.0, "{face:?}: gespiegelt");
            assert!(r.x > 0.0, "{face:?}: Text läuft nach rechts");
        }
        // Die reine Abbildung: rechts × oben (Bild, y nach unten) < 0 heißt ungespiegelt.
        let f = viewport::face_label_transform(&s.camera, face, cube());
        assert!(f.right.x * f.up.y - f.right.y * f.up.x < 0.0, "{face:?}");
        assert!(f.facing > 0.99, "{face:?}");
    }
}

#[test]
fn face_label_transform_follows_the_face_edges() {
    let mut cam = OrbitCamera::default();
    cam.home();
    for face in ViewFace::ALL {
        let f = viewport::face_label_transform(&cam, face, cube());
        if f.facing < 0.3 {
            continue;
        }
        let poly = piece_on_screen(&cam, CubeRegion::Face(face)).unwrap();
        // Mitte der Fläche = Ursprung der Abbildung.
        assert!((centroid(&poly) - f.origin).length() < 1e-3, "{face:?}");
        // „rechts“ und „oben“ sind parallel zu je einer Kante des Flächenvierecks.
        let e1 = poly[1] - poly[0];
        let e2 = poly[2] - poly[1];
        for axis in [f.right, f.up] {
            assert!(
                parallel(axis, e1) || parallel(axis, e2),
                "{face:?}: {axis:?} liegt nicht auf einer Kante"
            );
        }
        // Ungespiegelt (Bild y nach unten).
        assert!(f.right.x * f.up.y - f.right.y * f.up.x < 0.0, "{face:?}");
    }
}

// (g) ------------------------------------------------------------------------------------------

fn name_texts(sh: &Shot) -> Vec<String> {
    let r = viewport::name_rect(cube());
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
fn cube_does_not_orbit_or_select() {
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
    // Klick auf den Home-Knopf und auf einen Bereich wählt ebenfalls nichts.
    click(&ctx, &mut s, viewport::home_rect(c).center());
    assert_eq!(s.selection, None, "Home wählt nichts");
    s.camera.distance *= 0.15;
    let (yaw, pitch) = (s.camera.yaw, s.camera.pitch);
    // Ziehen auf dem Würfel dreht nicht.
    drag(&ctx, &mut s, c.center(), c.center() + vec2(-30.0, 25.0));
    assert_eq!((s.camera.yaw, s.camera.pitch), (yaw, pitch), "kein Orbit");
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
    // Daneben dreht dasselbe Ziehen.
    drag(&ctx, &mut s, pos2(350.0, 300.0), pos2(320.0, 325.0));
    assert_ne!(
        (s.camera.yaw, s.camera.pitch),
        (yaw, pitch),
        "Orbit daneben"
    );
    // Und ein Klick daneben wählt.
    click(&ctx, &mut s, FREE.center());
    assert!(s.selection.is_some(), "Klick daneben wählt");
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
