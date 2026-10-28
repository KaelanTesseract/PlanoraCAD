//! Dialog „Über PlanoraCAD“ (Menü „Hilfe“): Logo, Name, Version, Lizenz und die Quellen der
//! eingebauten Schriften und Symbole, in einfacher Sprache.

use crate::dialog::{Dialog, DialogSize};
use crate::logo;
use crate::theme::{self, SPACE_2, SPACE_4};
use planora_i18n::tr;
use planora_update::Version;

/// Titel des Dialogs und Beschriftung des Menüeintrags.
pub fn title() -> String {
    tr!("about-title")
}

/// Name des Programms.
pub const NAME: &str = "PlanoraCAD";

/// Lizenz des Programms.
pub const LICENSE: &str = "GPL-3.0-only";

/// Überschrift über der Liste der Quellen.
pub fn sources_heading() -> String {
    tr!("about-sources-heading")
}

/// Woher die mitgelieferten Schriften und Symbole stammen.
pub fn sources() -> [String; 4] {
    [
        tr!("about-source-montserrat"),
        tr!("about-source-phosphor"),
        tr!("about-source-roboto"),
        tr!("about-source-rajdhani"),
    ]
}

/// Zeile mit der laufenden Version und ihrem Kanal, etwa „Version 0.2.0-dev.1 (Dev)“.
pub fn version_line() -> String {
    version_line_for(&crate::running_version())
}

/// Zeile mit der Version `v` und ihrem Kanal.
pub fn version_line_for(v: &Version) -> String {
    tr!(
        "about-version",
        version = v.to_string(),
        channel = v.channel().label()
    )
}

/// Zeile mit der Grafikkarte, etwa „Grafikkarte: Name · Treiber X · Vulkan“.
pub fn gpu_text(info: &crate::build_info::BuildInfo) -> String {
    match info.gpu {
        Some(_) => tr!("about-gpu", gpu = info.gpu_line()),
        None => info.gpu_line(),
    }
}

/// Zeile mit der Lizenz.
pub fn license_line() -> String {
    tr!("about-license", license = LICENSE)
}

/// Zeigt den Dialog, solange `*open` gilt; das Schließen-Symbol setzt `*open = false`.
pub fn show(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    Dialog::new(title(), DialogSize::Normal)
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
            ui.strong(sources_heading());
            ui.add_space(SPACE_2);
            for line in sources() {
                ui.label(line);
            }
        });
}
