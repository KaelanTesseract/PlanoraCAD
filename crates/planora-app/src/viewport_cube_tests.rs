//! Unit-Tests des Navigationswürfels, die Interna von `viewport.rs` brauchen (Schwellen,
//! Abbildung, Beschriftung). Ohne Fenster: Frames gegen einen egui-Kontext.

use super::*;

/// Würfel ohne aufblitzende Linien (Handprüfung Etappe C, Haltepunkt 3): Beim Drehen der Kamera
/// dürfen keine fast flach gesehenen Bereiche als 1–2 px schmale Streifen mit Umriss gemalt
/// werden, und kein Bereich, kein Umriss und keine Beschriftung darf von einem Schritt zum
/// nächsten schlagartig erscheinen oder verschwinden.
mod artefacts {
    use super::*;

    use crate::fonts;
    use crate::state::AppState;
    use egui::epaint::{ClippedShape, ColorMode, Shape};
    use egui::{Pos2, Rect, pos2};
    use planora_render::{CubeRegion, OrbitCamera, ViewFace, cube_pieces_visible};
    use std::collections::HashMap;

    const SCREEN: Rect = Rect {
        min: pos2(0.0, 0.0),
        max: pos2(800.0, 600.0),
    };

    const FREE: Rect = Rect {
        min: pos2(278.0, 90.0),
        max: pos2(700.0, 500.0),
    };

    /// Schmaler darf kein gemalter Bereich sein (sonst erscheint er als Linie).
    const MIN_WIDTH_PX: f32 = 1.5;

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        ctx
    }

    fn cube() -> Rect {
        cube_rect(FREE)
    }

    fn frame(ctx: &egui::Context, s: &mut AppState) -> Vec<ClippedShape> {
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(SCREEN),
                ..Default::default()
            },
            |ui| show_in(ui, s, Some(FREE)),
        );
        let shapes = out.shapes.clone();
        out.drop_without_applying_deltas();
        shapes
    }

    /// Gemalter Würfelbereich: Umriss, Deckkraft der Füllung und des Umrisses (0…255).
    #[derive(Clone, Debug)]
    struct Painted {
        points: Vec<Pos2>,
        fill_alpha: u8,
        stroke_alpha: u8,
        stroke_width: f32,
    }

    #[derive(Default)]
    struct Shot {
        polys: Vec<Painted>,
        /// Größte Deckkraft der Eckpunkte je Beschriftungsnetz, mit den Eckpunkten.
        labels: Vec<(Vec<Pos2>, u8)>,
    }

    fn walk(shape: &Shape, s: &mut Shot) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
            Shape::Path(p) if p.closed => {
                let stroke_alpha = match p.stroke.color {
                    ColorMode::Solid(c) => c.a(),
                    _ => 255,
                };
                s.polys.push(Painted {
                    points: p.points.clone(),
                    fill_alpha: p.fill.a(),
                    stroke_alpha,
                    stroke_width: p.stroke.width,
                });
            }
            Shape::Mesh(m) if !m.vertices.is_empty() => s.labels.push((
                m.vertices.iter().map(|v| v.pos).collect(),
                m.vertices.iter().map(|v| v.color.a()).max().unwrap_or(0),
            )),
            _ => {}
        }
    }

    /// Formen im Würfelfeld.
    fn cube_shot(shapes: &[ClippedShape]) -> Shot {
        let mut all = Shot::default();
        for c in shapes {
            walk(&c.shape, &mut all);
        }
        let field = cube().expand(1.0);
        Shot {
            polys: all
                .polys
                .into_iter()
                .filter(|p| p.points.iter().all(|q| field.contains(*q)))
                .collect(),
            labels: all
                .labels
                .into_iter()
                .filter(|(pts, _)| pts.iter().all(|q| field.contains(*q)))
                .collect(),
        }
    }

    /// Kleinste Breite eines konvexen Vielecks (Drehschieblehre: eine Seite liegt an).
    fn min_width(points: &[Pos2]) -> f32 {
        let n = points.len();
        let mut best = f32::INFINITY;
        for i in 0..n {
            let a = points[i];
            let e = points[(i + 1) % n] - a;
            let len = e.length();
            if len < 1e-6 {
                continue;
            }
            let far = points
                .iter()
                .map(|p| ((p.x - a.x) * e.y - (p.y - a.y) * e.x).abs() / len)
                .fold(0.0_f32, f32::max);
            best = best.min(far);
        }
        if best.is_finite() { best } else { 0.0 }
    }

    /// Ordnet die gemalten Vielecke ihren Bereichen zu (Umriss aus `cube_pieces_visible`, alle der Kamera
    /// zugewandten Bereiche). Ein Vieleck ohne Bereich ist ein Fehler.
    fn by_region(cam: &OrbitCamera, shot: &Shot) -> HashMap<CubeRegion, Painted> {
        let pieces = cube_pieces_visible(cam, 0.02);
        let mut out = HashMap::new();
        for p in &shot.polys {
            let region = pieces
                .iter()
                .find(|piece| {
                    piece.corners.len() == p.points.len()
                        && piece
                            .corners
                            .iter()
                            .zip(&p.points)
                            .all(|(c, q)| (cube_to_screen(cube(), *c) - *q).length() < 0.01)
                })
                .unwrap_or_else(|| panic!("Vieleck ohne Bereich: {:?}", p.points))
                .region;
            assert!(
                out.insert(region, p.clone()).is_none(),
                "{} doppelt gemalt",
                region.name()
            );
        }
        out
    }

    fn state_at(yaw_deg: f32, pitch_deg: f32) -> AppState {
        let mut s = AppState::new();
        s.camera.yaw = yaw_deg.to_radians();
        s.camera.pitch = pitch_deg.to_radians();
        s
    }

    fn degrees(from: i32, to: i32, step: i32) -> impl Iterator<Item = f32> {
        (from..=to).step_by(step as usize).map(|d| d as f32)
    }

    #[test]
    fn no_sliver_is_painted_for_any_camera() {
        let ctx = context();
        let mut failures = Vec::new();
        for pitch in degrees(-85, 85, 3) {
            for yaw in degrees(-180, 177, 3) {
                let mut s = state_at(yaw, pitch);
                let shot = cube_shot(&frame(&ctx, &mut s));
                let regions = by_region(&s.camera, &shot);
                assert!(regions.len() >= 4, "yaw {yaw}, pitch {pitch}: Würfel fehlt");
                for (region, p) in &regions {
                    // Auch ganz ausgeblendete Vielecke zählen: Sie wären sonst unsichtbar anklickbar.
                    let w = min_width(&p.points);
                    if w < MIN_WIDTH_PX {
                        failures.push(format!(
                            "yaw {yaw}, pitch {pitch}: {} nur {w:.2} px breit (Umriss {} px, Deckkraft {})",
                            region.name(),
                            p.stroke_width,
                            p.stroke_alpha
                        ));
                    }
                    // Keine entarteten Umrissstücke.
                    let n = p.points.len();
                    let shortest = (0..n)
                        .map(|i| (p.points[(i + 1) % n] - p.points[i]).length())
                        .fold(f32::INFINITY, f32::min);
                    if shortest < MIN_WIDTH_PX {
                        failures.push(format!(
                            "yaw {yaw}, pitch {pitch}: {} hat eine {shortest:.2} px kurze Kante",
                            region.name()
                        ));
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} Streifen, z. B.:\n{}",
            failures.len(),
            failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    fn centroid(points: &[Pos2]) -> Pos2 {
        let n = points.len() as f32;
        let s = points.iter().fold(egui::Vec2::ZERO, |a, p| a + p.to_vec2());
        (s / n).to_pos2()
    }

    #[test]
    fn hit_test_and_drawing_agree_on_the_visible_set() {
        let ctx = context();
        for pitch in degrees(-84, 84, 6) {
            for yaw in degrees(-180, 174, 6) {
                let mut s = state_at(yaw, pitch);
                let shot = cube_shot(&frame(&ctx, &mut s));
                let painted = by_region(&s.camera, &shot);
                let visible: Vec<CubeRegion> =
                    visible_pieces(&s.camera).iter().map(|p| p.region).collect();
                let mut drawn: Vec<CubeRegion> = painted.keys().copied().collect();
                let mut want = visible.clone();
                drawn.sort_by_key(|r| format!("{r:?}"));
                want.sort_by_key(|r| format!("{r:?}"));
                assert_eq!(
                    drawn, want,
                    "yaw {yaw}, pitch {pitch}: gemalt und anklickbar stimmen nicht überein"
                );
                // Die Mitte jedes deutlich sichtbaren Bereichs trifft genau diesen Bereich, die eines
                // kaum sichtbaren (unter halb deckend) keinen.
                for (r, p) in &painted {
                    let hit = cube_region_at(&s.camera, cube(), centroid(&p.points));
                    if p.fill_alpha >= 128 {
                        assert_eq!(hit, Some(*r), "yaw {yaw}, pitch {pitch}");
                    } else if p.fill_alpha < 120 {
                        assert_eq!(hit, None, "yaw {yaw}, pitch {pitch}: {}", r.name());
                    }
                }
                // Ein Treffer liegt immer in einem gemalten Bereich, der mindestens halb deckt (kein
                // unsichtbarer Bereich lässt sich anklicken).
                let c = cube();
                for i in 0..=20 {
                    for j in 0..=20 {
                        let q = pos2(
                            c.left() + c.width() * i as f32 / 20.0,
                            c.top() + c.height() * j as f32 / 20.0,
                        );
                        if let Some(r) = cube_region_at(&s.camera, c, q) {
                            let p = &painted[&r];
                            assert!(
                                p.fill_alpha >= 120,
                                "yaw {yaw}, pitch {pitch}: {} anklickbar, aber unsichtbar",
                                r.name()
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn opacity_fades_smoothly_between_the_thresholds() {
        assert_eq!(piece_opacity(PIECE_MIN_FACING), 0.0);
        assert_eq!(piece_opacity(PIECE_FULL_FACING), 1.0);
        assert_eq!(piece_opacity(1.0), 1.0);
        assert_eq!(label_opacity(LABEL_MIN_FACING), 0.0);
        assert_eq!(label_opacity(LABEL_FULL_FACING), 1.0);
        let mut last = 0.0;
        for i in 0..=100 {
            let f = i as f32 / 100.0;
            let o = piece_opacity(f);
            assert!(o >= last, "steigt monoton");
            last = o;
        }
        // Die isometrische Ansicht (Ecken zu 1/3 zugewandt) und alle Würfelansichten bleiben voll
        // deckend: Hervorhebung und Hover-Farben gelten unverändert.
        assert_eq!(piece_opacity(1.0 / 3.0), 1.0);
    }

    /// Deckkraft je Bereich (Füllung, Umriss); fehlt der Bereich, zählt 0.
    fn alphas(regions: &HashMap<CubeRegion, Painted>, r: CubeRegion) -> (u8, u8) {
        regions
            .get(&r)
            .map_or((0, 0), |p| (p.fill_alpha, p.stroke_alpha))
    }

    /// Größte Deckkraft der Beschriftung auf der Fläche `face` (0 = keine Beschriftung).
    fn label_alpha(cam: &OrbitCamera, shot: &Shot, face: ViewFace) -> u8 {
        let Some(piece) = cube_pieces_visible(cam, 0.02)
            .into_iter()
            .find(|p| p.region == CubeRegion::Face(face))
        else {
            return 0;
        };
        let poly: Vec<Pos2> = piece
            .corners
            .iter()
            .map(|c| cube_to_screen(cube(), *c))
            .collect();
        let inside = |q: Pos2| {
            let n = poly.len();
            let mut sign = 0.0_f32;
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                let cross = (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x);
                if cross.abs() < 1e-3 {
                    continue;
                }
                if sign == 0.0 {
                    sign = cross.signum();
                } else if cross.signum() != sign {
                    return false;
                }
            }
            true
        };
        shot.labels
            .iter()
            .filter(|(pts, _)| pts.iter().all(|q| inside(*q)))
            .map(|(_, a)| *a)
            .max()
            .unwrap_or(0)
    }

    /// Größter Sprung der Deckkraft zwischen zwei Schritten von 1° (Bereich, Umriss, Beschriftung).
    /// Ein Bereich, der beim Drehen auftaucht, wird allmählich sichtbar statt auf einmal.
    const MAX_ALPHA_STEP: u8 = 64;

    #[test]
    fn pieces_outlines_and_labels_fade_instead_of_popping() {
        let ctx = context();
        let mut failures = Vec::new();
        let mut sweep = |path: Vec<(f32, f32)>| {
            let mut prev: Option<(OrbitCamera, Shot)> = None;
            for (yaw, pitch) in path {
                let mut s = state_at(yaw, pitch);
                let shot = cube_shot(&frame(&ctx, &mut s));
                if let Some((pcam, pshot)) = &prev {
                    let (a, b) = (by_region(pcam, pshot), by_region(&s.camera, &shot));
                    for r in planora_render::all_regions() {
                        let (fa, sa) = alphas(&a, r);
                        let (fb, sb) = alphas(&b, r);
                        if fa.abs_diff(fb) > MAX_ALPHA_STEP || sa.abs_diff(sb) > MAX_ALPHA_STEP {
                            failures.push(format!(
                                "yaw {yaw}, pitch {pitch}: {} springt (Füllung {fa} zu {fb}, Umriss {sa} zu {sb})",
                                r.name()
                            ));
                        }
                    }
                    for face in ViewFace::ALL {
                        let (la, lb) = (
                            label_alpha(pcam, pshot, face),
                            label_alpha(&s.camera, &shot, face),
                        );
                        if la.abs_diff(lb) > MAX_ALPHA_STEP {
                            failures.push(format!(
                                "yaw {yaw}, pitch {pitch}: Beschriftung {} springt ({la} zu {lb})",
                                face.label()
                            ));
                        }
                    }
                }
                prev = Some((s.camera, shot));
            }
        };
        for pitch in [-70.0, -35.0, -10.0, 0.0, 12.0, 35.0, 64.0] {
            sweep(degrees(-180, 180, 1).map(|y| (y, pitch)).collect());
        }
        for yaw in [-135.0, -90.0, -20.0, 0.0, 45.0, 100.0] {
            sweep(degrees(-85, 85, 1).map(|p| (yaw, p)).collect());
        }
        assert!(
            failures.is_empty(),
            "{} Sprünge, z. B.:\n{}",
            failures.len(),
            failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Beschriftung auf den Flächen und Lage der Felder um den Würfel.
mod labels {
    use super::*;
    use crate::fonts;
    use crate::shell::{ShellState, layout};
    use crate::state::AppState;
    use egui::epaint::{ClippedShape, Shape};
    use planora_render::{CubeRegion, OrbitCamera, ViewFace, cube_pieces_visible};

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

    fn frame(ctx: &egui::Context, s: &mut AppState) -> Vec<ClippedShape> {
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(SCREEN),
                ..Default::default()
            },
            |ui| show_in(ui, s, Some(FREE)),
        );
        let shapes = out.shapes.clone();
        out.drop_without_applying_deltas();
        shapes
    }

    fn cube() -> Rect {
        cube_rect(FREE)
    }

    fn piece_on_screen(cam: &OrbitCamera, region: CubeRegion) -> Option<Vec<Pos2>> {
        cube_pieces_visible(cam, 0.02)
            .into_iter()
            .find(|p| p.region == region)
            .map(|p| {
                p.corners
                    .iter()
                    .map(|c| cube_to_screen(cube(), *c))
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

    fn meshes(shapes: &[ClippedShape]) -> Vec<egui::Mesh> {
        fn walk(shape: &Shape, out: &mut Vec<egui::Mesh>) {
            match shape {
                Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
                Shape::Mesh(m) => out.push((**m).clone()),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut out);
        }
        out
    }

    fn state_with_view(face: Option<ViewFace>) -> AppState {
        let mut s = AppState::new();
        match face {
            Some(f) => s.camera.set_view(f),
            None => s.camera.home(),
        }
        s
    }

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
    fn label_in<'a>(ms: &'a [egui::Mesh], poly: &[Pos2]) -> Option<&'a egui::Mesh> {
        ms.iter()
            .find(|m| !m.vertices.is_empty() && m.vertices.iter().all(|v| inside(poly, v.pos, 0.0)))
    }

    #[test]
    fn cube_rect_and_home_and_name_rects() {
        let c = cube_rect(FREE);
        assert_eq!(CUBE_SIZE, 100.0);
        assert_eq!(
            c,
            Rect::from_min_size(
                pos2(FREE.right() - 100.0 - 12.0, FREE.top() + 12.0),
                vec2(100.0, 100.0)
            )
        );
        let h = home_rect(c);
        assert_eq!(h.size(), vec2(26.0, 26.0));
        assert_eq!(h.top(), c.top(), "Home oben bündig mit dem Würfel");
        assert_eq!(
            h.right() - c.left(),
            2.0,
            "Home überlappt den Würfelrand um 2 px"
        );
        let n = name_rect(c);
        assert_eq!(n, Rect::from_min_size(c.left_bottom(), vec2(100.0, 16.0)));

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
                let cube = cube_rect(r.free);
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
                    ("Home", home_rect(cube)),
                    ("Name", name_rect(cube)),
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

    #[test]
    fn labels_are_on_the_faces_and_follow_them() {
        let ctx = context();
        let mut s = state_with_view(Some(ViewFace::Front));
        let front = CubeRegion::Face(ViewFace::Front);
        let ms = meshes(&frame(&ctx, &mut s));
        assert_eq!(ms.len(), 1, "nur Vorne ist zugewandt");
        let poly = piece_on_screen(&s.camera, front).unwrap();
        let m = label_in(&ms, &poly).expect("Vorne liegt in der Fläche");
        assert!(m.vertices.len() >= 5 * 4, "fünf Buchstaben");
        assert!(m.vertices.iter().all(|v| inside(&poly, v.pos, 0.5)));

        // Gedreht: der Text bleibt auf der (jetzt schmaleren) Fläche und ist mit ihr verzerrt.
        // Die Beschriftung blendet erst ab `LABEL_MIN_FACING` ein; die Winkel bleiben darüber.
        for (yaw, pitch) in [(40.0_f32, 0.0_f32), (40.0, 25.0), (-30.0, -20.0)] {
            let mut s = state_with_view(Some(ViewFace::Front));
            s.camera.yaw += yaw.to_radians();
            s.camera.pitch += pitch.to_radians();
            let ms = meshes(&frame(&ctx, &mut s));
            let poly = piece_on_screen(&s.camera, front).unwrap();
            let m = label_in(&ms, &poly)
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
            let ms = meshes(&frame(&ctx, &mut s));
            let poly = piece_on_screen(&s.camera, CubeRegion::Face(face)).unwrap();
            let m = label_in(&ms, &poly).unwrap_or_else(|| panic!("{face:?}: Text fehlt"));
            for q in quads(m) {
                let (r, d) = (q[1] - q[0], q[2] - q[0]);
                assert!(r.x * d.y - r.y * d.x > 0.0, "{face:?}: gespiegelt");
                assert!(r.x > 0.0, "{face:?}: Text läuft nach rechts");
            }
            // Die reine Abbildung: rechts × oben (Bild, y nach unten) < 0 heißt ungespiegelt.
            let f = face_label_transform(&s.camera, face, cube());
            assert!(f.right.x * f.up.y - f.right.y * f.up.x < 0.0, "{face:?}");
            assert!(f.facing > 0.99, "{face:?}");
        }
    }

    #[test]
    fn face_label_transform_follows_the_face_edges() {
        let mut cam = OrbitCamera::default();
        cam.home();
        for face in ViewFace::ALL {
            let f = face_label_transform(&cam, face, cube());
            if f.facing < LABEL_MIN_FACING {
                continue;
            }
            let poly = piece_on_screen(&cam, CubeRegion::Face(face)).unwrap();
            // Mitte der Fläche = Ursprung der Abbildung.
            assert!((centroid(&poly) - f.origin).length() < 1e-3, "{face:?}");
            // "rechts" und "oben" sind parallel zu je einer Kante des Flächenvierecks.
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
}

// Deckkraft und Schrift der Beschriftung -------------------------------------------------------

#[test]
fn labels_fade_between_0_5_and_0_7() {
    assert_eq!(LABEL_MIN_FACING, 0.5);
    assert_eq!(LABEL_FULL_FACING, 0.7);
    assert_eq!(label_opacity(0.5), 0.0);
    assert_eq!(label_opacity(0.7), 1.0);
    assert_eq!(label_opacity(0.3), 0.0);
    assert_eq!(label_opacity(1.0), 1.0);
    let mid = label_opacity(0.6);
    assert!(mid > 0.0 && mid < 1.0, "dazwischen weich: {mid}");
}

/// Führt `f` mit dem Maler eines Bilds bei `pixels_per_point` aus.
fn with_painter<R>(
    ctx: &egui::Context,
    pixels_per_point: f32,
    f: impl FnOnce(&egui::Painter) -> R,
) -> R {
    ctx.set_pixels_per_point(pixels_per_point);
    let mut result = None;
    let mut f = Some(f);
    // Die Einstellung gilt eventuell erst ab dem folgenden Bild: bis zu zwei Bilder.
    for _ in 0..2 {
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0))),
                ..Default::default()
            },
            |ui| {
                if ui.ctx().pixels_per_point() == pixels_per_point {
                    result = f.take().map(|f| f(ui.painter()));
                }
            },
        );
        out.drop_without_applying_deltas();
        if result.is_some() {
            break;
        }
    }
    result.expect("pixels_per_point wurde nicht übernommen")
}

fn font_context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::fonts::definitions());
    ctx
}

#[test]
fn label_font_is_as_large_as_fits() {
    assert_eq!(CUBE_SIZE, 100.0);
    let face_px = 2.0 * INSET * CUBE_SCALE;
    let limit = LABEL_FILL * face_px;
    let ctx = font_context();
    with_painter(&ctx, 1.0, |painter| {
        let label = label_font(painter);
        let widest = |size: f32| {
            let font = FontId::new(size, fonts::medium());
            ViewFace::ALL
                .iter()
                .map(|f| {
                    painter
                        .layout_no_wrap(f.label().to_string(), font.clone(), Color32::PLACEHOLDER)
                        .mesh_bounds
                        .width()
                })
                .fold(0.0_f32, f32::max)
        };
        let size = label.font.size;
        assert!(size >= 9.0, "Schrift mindestens 9 pt: {size}");
        // Die Glyphen liegen auf ganzen Bildpunkten: Bei der Mindestschrift (9 pt, 32 px statt
        // 31,92 px) darf die Rundung eine halbe Einheit über der Grenze liegen.
        let slack = if size == 9.0 { 0.5 } else { 1e-3 };
        assert!(
            widest(size) <= limit + slack,
            "längstes Wort {} px, erlaubt {limit} px",
            widest(size)
        );
        assert!(
            widest(size + 0.5) > limit,
            "{size} pt ist nicht die größte passende Schrift ({} px)",
            widest(size + 0.5)
        );
        // Der Maßstab bildet das längste Wort auf die Füllbreite der Fläche ab.
        assert!((widest(size) * label.k - LABEL_FILL * 2.0 * INSET).abs() < 1e-4);
    });
}

#[test]
fn label_font_is_cached_per_scale() {
    let ctx = font_context();
    let first = with_painter(&ctx, 1.0, |painter| {
        let a = label_font(painter);
        let b = label_font(painter);
        assert_eq!(a, b, "im selben Bild dasselbe");
        let stored = painter
            .ctx()
            .data(|d| d.get_temp::<LabelFont>(label_font_id()));
        assert_eq!(stored, Some(a.clone()), "gemerkt");
        a
    });
    // Nächstes Bild, gleicher Maßstab: unverändert aus dem Speicher.
    let again = with_painter(&ctx, 1.0, label_font);
    assert_eq!(first, again);
    // Anderer Maßstab: neu gerechnet und gemerkt, nicht der alte Eintrag.
    let doubled = with_painter(&ctx, 2.0, label_font);
    assert_ne!(first, doubled, "anderer pixels_per_point, anderes Ergebnis");
    let fresh = with_painter(&font_context(), 2.0, label_font);
    assert_eq!(doubled, fresh, "wie frisch gerechnet");
    assert_eq!(doubled.key.0, 2.0_f32.to_bits());
}
