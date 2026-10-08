use crate::state::AppState;
use crate::theme;
use egui::{Align2, Color32, FontId, PointerButton, Rect, Sense, Shape, Stroke, pos2, vec2};
use glam::Vec2;
use planora_render::gpu::ViewportCallback;
use planora_render::{ViewFace, cube_faces, cube_hit_test};

const CUBE_SIZE: f32 = 96.0;

fn cube_rect(viewport: Rect) -> Rect {
    Rect::from_min_size(
        pos2(viewport.right() - CUBE_SIZE - 12.0, viewport.top() + 12.0),
        vec2(CUBE_SIZE, CUBE_SIZE),
    )
}

/// 3D-Ansicht: Navigation, Auswahl, Zeichnen und Navigationswürfel.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::viewport_background(state.dark));
    state.camera.set_viewport(rect.width(), rect.height());

    let cube = cube_rect(rect);
    let pointer = response.hover_pos();
    let over_cube = pointer.is_some_and(|p| cube.contains(p));

    // Navigation
    let pan = response.dragged_by(PointerButton::Middle)
        || (response.dragged_by(PointerButton::Primary) && ui.input(|i| i.modifiers.shift));
    if pan {
        let d = response.drag_motion();
        state.camera.pan(d.x, d.y, rect.height());
    } else if response.dragged_by(PointerButton::Primary) {
        let d = response.drag_motion();
        state.camera.orbit(d.x, d.y);
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta().y);
        if scroll != 0.0 {
            state.camera.zoom(scroll);
        }
    }

    // Navigationswürfel
    let faces = cube_faces(&state.camera);
    let cube_center = cube.center();
    let half = CUBE_SIZE * 0.28;
    let to_screen = |v: Vec2| pos2(cube_center.x + v.x * half, cube_center.y - v.y * half);
    let mut cube_hover: Option<ViewFace> = None;
    if let (true, Some(p)) = (over_cube, pointer) {
        let local = Vec2::new((p.x - cube_center.x) / half, -(p.y - cube_center.y) / half);
        cube_hover = cube_hit_test(&faces, local);
    }
    if response.clicked()
        && let Some(face) = cube_hover
    {
        state.camera.set_view(face);
    }

    // Auswahl (nicht über dem Würfel)
    if let (false, Some(p)) = (over_cube, pointer) {
        let cursor = Vec2::new(p.x - rect.left(), p.y - rect.top());
        let viewport = Vec2::new(rect.width(), rect.height());
        let hit = state.pick(cursor, viewport);
        state.set_hover(hit);
        if response.clicked() {
            state.set_selection(hit);
        }
    } else if pointer.is_none() || over_cube {
        state.set_hover(None);
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.set_selection(None);
    }

    // 3D-Szene
    let cam = state.camera;
    painter.add(eframe::egui_wgpu::Callback::new_paint_callback(
        rect,
        ViewportCallback {
            view_proj: cam.view_proj().to_cols_array_2d(),
            light_dir: cam.light_dir().to_array(),
            scene: state.scene.clone(),
            scene_version: state.scene_version,
        },
    ));

    // Würfel zeichnen
    let (fill, hover_fill, line, text) = if state.dark {
        (
            Color32::from_rgba_unmultiplied(70, 76, 90, 235),
            Color32::from_rgba_unmultiplied(76, 154, 255, 235),
            Color32::from_rgb(150, 158, 175),
            Color32::from_rgb(235, 238, 245),
        )
    } else {
        (
            Color32::from_rgba_unmultiplied(225, 229, 238, 240),
            Color32::from_rgba_unmultiplied(31, 111, 224, 240),
            Color32::from_rgb(90, 98, 115),
            Color32::from_rgb(30, 34, 44),
        )
    };
    for f in &faces {
        let points: Vec<_> = f.corners.iter().map(|c| to_screen(*c)).collect();
        let color = if cube_hover == Some(f.face) {
            hover_fill
        } else {
            fill
        };
        painter.add(Shape::convex_polygon(
            points.clone(),
            color,
            Stroke::new(1.0, line),
        ));
        let center = points.iter().fold(pos2(0.0, 0.0), |a, p| {
            pos2(a.x + p.x / 4.0, a.y + p.y / 4.0)
        });
        painter.text(
            center,
            Align2::CENTER_CENTER,
            f.face.label(),
            FontId::proportional(11.0),
            text,
        );
    }
}
