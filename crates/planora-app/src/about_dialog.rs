//! Dialog „Über PlanoraCAD“ (Menü „Hilfe“): Logo, Name, Version, Lizenz und die Quellen der
//! eingebauten Schriften und Symbole, in einfacher Sprache.

use crate::dialog::{Dialog, DialogSize};
use crate::logo;
use crate::theme::{self, SPACE_2, SPACE_4};
use planora_update::Version;

/// Titel des Dialogs und Beschriftung des Menüeintrags.
pub const TITLE: &str = "Über PlanoraCAD";

/// Name des Programms.
pub const NAME: &str = "PlanoraCAD";

/// Lizenz des Programms.
pub const LICENSE: &str = "GPL-3.0-only";

/// Überschrift über der Liste der Quellen.
pub const SOURCES_HEADING: &str = "Mitgelieferte Schriften und Symbole";

/// Woher die mitgelieferten Schriften und Symbole stammen.
pub const SOURCES: [&str; 4] = [
    "Schrift Montserrat (SIL Open Font License)",
    "Symbole Phosphor Icons (MIT)",
    "Schrift Roboto (Apache-2.0)",
    "Logo-Schrift Rajdhani (SIL Open Font License)",
];

/// Zeile mit der laufenden Version und ihrem Kanal, etwa „Version 0.2.0-dev.1 (Dev)“.
pub fn version_line() -> String {
    version_line_for(&crate::running_version())
}

/// Zeile mit der Version `v` und ihrem Kanal.
pub fn version_line_for(v: &Version) -> String {
    format!("Version {v} ({})", v.channel().label())
}

/// Zeile mit der Grafikkarte, etwa „Grafikkarte: Name · Treiber X · Vulkan“.
pub fn gpu_text(info: &crate::build_info::BuildInfo) -> String {
    match info.gpu {
        Some(_) => format!("Grafikkarte: {}", info.gpu_line()),
        None => info.gpu_line(),
    }
}

/// Zeile mit der Lizenz.
pub fn license_line() -> String {
    format!("Lizenz: {LICENSE}")
}

/// Zeigt den Dialog, solange `*open` gilt; das Schließen-Symbol setzt `*open = false`.
pub fn show(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    Dialog::new(TITLE, DialogSize::Normal)
        .open(open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(image) =
                    logo::image(ctx, "about-256", logo::ICON_256, logo::DIALOG_SIZE)
                {
                    ui.add(image);
                }
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(NAME).font(theme::heading_font()));
                    ui.label(version_line());
                    let info = crate::build_info::build_info();
                    ui.label(info.system_line());
                    ui.label(gpu_text(&info));
                    ui.label(license_line());
                });
            });
            ui.add_space(SPACE_4);
            ui.strong(SOURCES_HEADING);
            ui.add_space(SPACE_2);
            for line in SOURCES {
                ui.label(line);
            }
        });
}
