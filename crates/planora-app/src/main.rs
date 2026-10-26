#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use planora_app::app::PlanoraApp;
use planora_render::gpu::MSAA_SAMPLES;

fn main() -> eframe::Result {
    // Protokoll im Datenordner (neben der Konfiguration), noch vor allem anderen.
    let mut run = (None, planora_app::crash::StartFinding::Clean);
    if let Some(data_dir) = planora_project::config::Config::config_path().parent() {
        planora_app::diag::init_global(data_dir);
        // Absturzbericht nach `abstuerze/`; der bisherige Hook läuft danach weiter.
        planora_app::crash::install_hook(data_dir.join(planora_app::crash::CRASH_DIR));
        // Laufmarker anlegen und nachsehen, wie der letzte Lauf endete (Rückfrage in der App).
        run = planora_app::crash::start_run(data_dir, std::process::id());
    }
    let (marker, finding) = run;
    // Von der alten Version nach dem Austausch gestartet (siehe `update_install`).
    // `args_os`: ein Argument, das kein gültiges Unicode ist (etwa ein Dateipfad), bricht nicht ab.
    let after_update = std::env::args_os()
        .skip(1)
        .any(|a| a == planora_app::update_install::AFTER_UPDATE_ARG);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("PlanoraCAD")
        .with_inner_size([1400.0, 900.0])
        // Darunter überlappen sich die schwebenden Felder.
        .with_min_inner_size([800.0, 600.0]);
    // Nach einem Update öffnet das Fenster in Größe, Lage und Zustand der alten Version (sie hat
    // es mit dem begonnenen Update in die Konfiguration geschrieben).
    if after_update
        && let Some(window) = planora_project::config::Config::load()
            .update_pending
            .and_then(|p| p.window)
    {
        viewport = planora_app::update_install::restore_window(viewport, &window);
    }
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
    let marker_file = marker.is_some().then(|| {
        planora_project::config::Config::config_path()
            .parent()
            .map(|d| planora_app::crash::marker_path(d, std::process::id()))
    });
    let result = eframe::run_native(
        "PlanoraCAD",
        options,
        Box::new(move |cc| Ok(Box::new(PlanoraApp::new(cc, after_update, marker, finding)))),
    );
    // Start sichtbar gescheitert (etwa ohne Grafik): kein „unerwartet beendet“ beim nächsten
    // Mal. Marker und App sind hier schon weg, die Datei ist also nicht mehr offen.
    if result.is_err()
        && let Some(Some(path)) = marker_file
    {
        let _ = std::fs::remove_file(path);
    }
    result
}
