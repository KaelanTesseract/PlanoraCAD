//! Navigationswürfel ohne aufblitzende Linien (Handprüfung Etappe C, Haltepunkt 3): Beim Drehen
//! der Kamera dürfen keine fast flach gesehenen Bereiche als 1–2 px schmale Streifen mit Umriss
//! gemalt werden, und kein Bereich, kein Umriss und keine Beschriftung darf von einem Schritt zum
//! nächsten schlagartig erscheinen oder verschwinden. Ohne Fenster: Frames gegen einen
//! egui-Kontext, geprüft werden die gemalten Formen im Würfelfeld.

use egui::epaint::{ClippedShape, ColorMode, Shape};
use egui::{Pos2, Rect, pos2};
use planora_app::state::AppState;
use planora_app::{fonts, viewport};
use planora_render::{CubeRegion, OrbitCamera, ViewFace, cube_pieces};
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
    viewport::cube_rect(FREE)
}

fn frame(ctx: &egui::Context, s: &mut AppState) -> Vec<ClippedShape> {
    let out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(SCREEN),
            ..Default::default()
        },
        |ui| viewport::show_in(ui, s, Some(FREE)),
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

/// Ordnet die gemalten Vielecke ihren Bereichen zu (Umriss aus `cube_pieces`, alle der Kamera
/// zugewandten Bereiche). Ein Vieleck ohne Bereich ist ein Fehler.
fn by_region(cam: &OrbitCamera, shot: &Shot) -> HashMap<CubeRegion, Painted> {
    let pieces = cube_pieces(cam);
    let mut out = HashMap::new();
    for p in &shot.polys {
        let region =
            pieces
                .iter()
                .find(|piece| {
                    piece.corners.len() == p.points.len()
                        && piece.corners.iter().zip(&p.points).all(|(c, q)| {
                            (viewport::cube_to_screen(cube(), *c) - *q).length() < 0.01
                        })
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
            let visible: Vec<CubeRegion> = viewport::visible_pieces(&s.camera)
                .iter()
                .map(|p| p.region)
                .collect();
            let mut drawn: Vec<CubeRegion> = painted.keys().copied().collect();
            let mut want = visible.clone();
            drawn.sort_by_key(|r| format!("{r:?}"));
            want.sort_by_key(|r| format!("{r:?}"));
            assert_eq!(drawn, want, "yaw {yaw}, pitch {pitch}: gemalt ≠ anklickbar");
            // Die Mitte jedes deutlich sichtbaren Bereichs trifft genau diesen Bereich, die eines
            // kaum sichtbaren (unter halb deckend) keinen.
            for (r, p) in &painted {
                let hit = viewport::cube_region_at(&s.camera, cube(), centroid(&p.points));
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
                    if let Some(r) = viewport::cube_region_at(&s.camera, c, q) {
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
    assert_eq!(viewport::piece_opacity(viewport::PIECE_MIN_FACING), 0.0);
    assert_eq!(viewport::piece_opacity(viewport::PIECE_FULL_FACING), 1.0);
    assert_eq!(viewport::piece_opacity(1.0), 1.0);
    assert_eq!(viewport::label_opacity(viewport::LABEL_MIN_FACING), 0.0);
    assert_eq!(viewport::label_opacity(viewport::LABEL_FULL_FACING), 1.0);
    let mut last = 0.0;
    for i in 0..=100 {
        let f = i as f32 / 100.0;
        let o = viewport::piece_opacity(f);
        assert!(o >= last, "steigt monoton");
        last = o;
    }
    // Die isometrische Ansicht (Ecken zu 1/3 zugewandt) und alle Würfelansichten bleiben voll
    // deckend: Hervorhebung und Hover-Farben gelten unverändert.
    assert_eq!(viewport::piece_opacity(1.0 / 3.0), 1.0);
}

/// Deckkraft je Bereich (Füllung, Umriss); fehlt der Bereich, zählt 0.
fn alphas(regions: &HashMap<CubeRegion, Painted>, r: CubeRegion) -> (u8, u8) {
    regions
        .get(&r)
        .map_or((0, 0), |p| (p.fill_alpha, p.stroke_alpha))
}

/// Größte Deckkraft der Beschriftung auf der Fläche `face` (0 = keine Beschriftung).
fn label_alpha(cam: &OrbitCamera, shot: &Shot, face: ViewFace) -> u8 {
    let Some(piece) = cube_pieces(cam)
        .into_iter()
        .find(|p| p.region == CubeRegion::Face(face))
    else {
        return 0;
    };
    let poly: Vec<Pos2> = piece
        .corners
        .iter()
        .map(|c| viewport::cube_to_screen(cube(), *c))
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
                            "yaw {yaw}, pitch {pitch}: {} springt (Füllung {fa}→{fb}, Umriss {sa}→{sb})",
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
                            "yaw {yaw}, pitch {pitch}: Beschriftung {} springt ({la}→{lb})",
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
