//! Was die Plug-ins vom Nutzer wollen (Plug-ins Etappe 1, Task 5): die Rechte-Nachfrage
//! („Erlauben“, „Nur diesmal“, „Ablehnen“) und die Fehlerleiste oben im Fenster für ausgefallene
//! Plug-ins. Die Fenster liefern nur die Wahl; was sie bewirkt, entscheidet
//! [`crate::plugins::Plugins`].

use crate::dialog::{Dialog, DialogSize, escape_pressed};
use planora_i18n::tr;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::rights::Answer;

/// Titel der Nachfrage.
pub fn ask_title(plugin: &str) -> String {
    tr!("plugin-ask-title", plugin = plugin)
}

/// Was das Plug-in `plugin` mit dem Recht `right` will; `hosts` sind die Rechner aus seinem
/// Manifest (nur für `net`).
pub fn right_text(plugin: &str, right: Right, hosts: &[String]) -> String {
    match right {
        Right::Net => tr!(
            "plugin-right-net",
            plugin = plugin,
            hosts = hosts.join(", ")
        ),
        Right::FsFolder => tr!("plugin-right-fs-folder", plugin = plugin),
        Right::ModelRead => tr!("plugin-right-model-read", plugin = plugin),
        Right::ModelWrite => tr!("plugin-right-model-write", plugin = plugin),
    }
}

/// Zeigt die Nachfrage zum Recht `right` des Plug-ins `plugin` (Anzeigename) als festes Fenster in
/// der Mitte und liefert die Wahl. Esc wirkt wie „Ablehnen“ und wird verbraucht (es beantwortet
/// nur diese eine Nachfrage, die nächste erscheint erst im nächsten Bild).
pub fn show_ask(
    ctx: &egui::Context,
    plugin: &str,
    right: Right,
    hosts: &[String],
) -> Option<Answer> {
    let mut answer = None;
    Dialog::new(ask_title(plugin), DialogSize::Prompt)
        .id(egui::Id::new("planora_plugin_ask"))
        .fixed()
        .show(ctx, |ui| {
            ui.label(right_text(plugin, right, hosts));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("plugin-ask-allow")).clicked() {
                    answer = Some(Answer::Allow);
                }
                if ui.button(tr!("plugin-ask-once")).clicked() {
                    answer = Some(Answer::Once);
                }
                if ui.button(tr!("plugin-ask-deny")).clicked() {
                    answer = Some(Answer::Deny);
                }
            });
            if answer.is_none() && escape_pressed(ui, true) {
                answer = Some(Answer::Deny);
            }
        });
    answer
}

/// Eine Zeile der Fehlerleiste: Kennung, Anzeigename und ob das Plug-in endgültig aus ist.
pub struct BannerLine<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub stopped: bool,
}

/// Was in der Fehlerleiste gedrückt wurde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BannerAction {
    Restart(String),
    Close(String),
}

/// Text einer Zeile der Fehlerleiste.
pub fn banner_text(name: &str, stopped: bool) -> String {
    if stopped {
        tr!("plugin-stopped", plugin = name)
    } else {
        tr!("plugin-failed", plugin = name)
    }
}

/// Die Fehlerleiste oben in der Mitte des Fensters: je ausgefallenem Plug-in eine Zeile mit
/// „Neu starten“ (fehlt, wenn es endgültig aus ist) und „Schließen“.
pub fn show_banners(ctx: &egui::Context, lines: &[BannerLine<'_>]) -> Option<BannerAction> {
    if lines.is_empty() {
        return None;
    }
    let mut action = None;
    egui::Area::new(egui::Id::new("planora_plugin_banners"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 40.0))
        .movable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                for line in lines {
                    ui.horizontal(|ui| {
                        ui.label(banner_text(line.name, line.stopped));
                        if !line.stopped && ui.button(tr!("plugin-restart")).clicked() {
                            action = Some(BannerAction::Restart(line.id.to_string()));
                        }
                        if ui.button(tr!("plugin-banner-close")).clicked() {
                            action = Some(BannerAction::Close(line.id.to_string()));
                        }
                    });
                }
            });
        });
    action
}
