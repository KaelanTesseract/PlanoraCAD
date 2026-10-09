use crate::state::AppState;
use crate::{sketch_view, theme};
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

fn keyboard_focus_id() -> egui::Id {
    egui::Id::new("planora-keyboard-focus-last-frame")
}

/// Merkt sich am Ende eines Frames, ob ein Eingabefeld die Tastatur hatte. Wird von der App nach
/// dem Aufbau aller Bereiche aufgerufen.
pub fn remember_keyboard_focus(ctx: &egui::Context) {
    let focused = ctx.egui_wants_keyboard_input();
    ctx.data_mut(|d| d.insert_temp(keyboard_focus_id(), focused));
}

/// Hat ein Eingabefeld die Tastatur (in diesem oder im vorigen Frame)? Dann sind Tastenkürzel
/// der Ansicht gesperrt. egui nimmt bei Escape den Fokus schon zu Beginn des Frames weg; deshalb
/// zählt auch, ob im vorigen Frame ein Eingabefeld (z. B. der Name) die Tastatur hatte.
pub fn typing(ctx: &egui::Context) -> bool {
    ctx.egui_wants_keyboard_input()
        || ctx
            .data(|d| d.get_temp::<bool>(keyboard_focus_id()))
            .unwrap_or(false)
}

/// 3D-Ansicht: Navigation, Auswahl, Zeichnen und Navigationswürfel. Im Skizzen-Modus übernimmt
/// [`sketch_view::show_overlay`] die Eingaben (Orbit gesperrt, Verschieben und Zoomen bleiben).
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let sketching = state.sketch_session.is_some();
    let sense = if sketching {
        Sense::hover()
    } else {
        Sense::click_and_drag()
    };
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), sense);
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::viewport_background(state.dark));
    state.camera.set_viewport(rect.width(), rect.height());
    // Platz für die 3D-Szene; sie wird am Ende mit der endgültigen Kamera eingesetzt, damit die
    // Skizze darüber liegt.
    let scene_slot = painter.add(Shape::Noop);

    let cube = cube_rect(rect);
    if sketching {
        sketch_view::show_overlay(ui, rect, state);
    } else {
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
    }

    // Navigationswürfel: eigene Schaltfläche über der Ansicht (auch im Skizzen-Modus).
    let cube_response = ui.interact(cube, ui.id().with("planora-view-cube"), Sense::click());
    let faces = cube_faces(&state.camera);
    let cube_center = cube.center();
    let half = CUBE_SIZE * 0.28;
    let to_screen = |v: Vec2| pos2(cube_center.x + v.x * half, cube_center.y - v.y * half);
    let cube_hover: Option<ViewFace> = cube_response.hover_pos().and_then(|p| {
        let local = Vec2::new((p.x - cube_center.x) / half, -(p.y - cube_center.y) / half);
        cube_hit_test(&faces, local)
    });
    if cube_response.clicked()
        && let Some(face) = cube_hover
    {
        state.camera.set_view(face);
    }

    if !sketching {
        // Auswahl (nicht über dem Würfel; dort ist die Ansicht nicht „hovered“)
        match response.hover_pos() {
            Some(p) => {
                let cursor = Vec2::new(p.x - rect.left(), p.y - rect.top());
                let viewport = Vec2::new(rect.width(), rect.height());
                let hit = state.pick(cursor, viewport);
                state.set_hover(hit);
                if response.clicked() {
                    state.set_selection(hit);
                }
            }
            None => state.set_hover(None),
        }
        if !typing(ui.ctx()) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            state.set_selection(None);
        }
    }

    // 3D-Szene
    let cam = state.camera;
    painter.set(
        scene_slot,
        eframe::egui_wgpu::Callback::new_paint_callback(
            rect,
            ViewportCallback {
                view_proj: cam.view_proj().to_cols_array_2d(),
                light_dir: cam.light_dir().to_array(),
                scene: state.scene.clone(),
                scene_version: state.scene_version,
            },
        ),
    );
    let faces = cube_faces(&state.camera);

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
