#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use planora_app::app::PlanoraApp;
use planora_render::gpu::MSAA_SAMPLES;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("PlanoraCAD")
        .with_inner_size([1400.0, 900.0])
        // Darunter überlappen sich die schwebenden Felder.
        .with_min_inner_size([800.0, 600.0]);
    // Ohne lesbares Symbol bleibt das Standardsymbol des Systems.
    if let Some(icon) = planora_app::logo::window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        multisampling: MSAA_SAMPLES as u16,
        depth_buffer: 32,
        ..Default::default()
    };
    eframe::run_native(
        "PlanoraCAD",
        options,
        Box::new(|cc| Ok(Box::new(PlanoraApp::new(cc)))),
    )
}
