//! Was die Plug-ins vom Nutzer wollen (Plug-ins Etappe 1, Task 5): die Rechte-Nachfrage
//! („Erlauben“, „Nur diesmal“, „Ablehnen“) und die Fehlerleiste oben im Fenster für ausgefallene
//! Plug-ins. Dazu (Task 7) der Installationsdialog und die kurzen Texte zu Rechten, Herkunft und
//! Zustand für die Einstellungen. Die Fenster liefern nur die Wahl; was sie bewirkt, entscheidet
//! [`crate::plugins::Plugins`]. Task 8: der Aktualisieren-Dialog und in der Fehlerleiste der
//! Hinweis auf gefundene Updates.

use crate::dialog::{Dialog, DialogSize, escape_pressed};
use crate::theme;
use planora_i18n::tr;
use planora_plugin_host::EntryState;
use planora_plugin_host::licenses::license_ok;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::package::{Package, Signature};
use planora_plugin_host::places::{Origin, Provenance};
use planora_plugin_host::rights::{Answer, Grant};

/// Speicher, den ein Plug-in ohne Angabe bekommt (MB); nur ein anderer Wert steht im Dialog.
pub const DEFAULT_MEMORY_MB: u32 = 512;

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
    /// „Anzeigen“ im Hinweis auf Updates (öffnet den Abschnitt „Plug-ins“).
    ShowUpdates,
    /// „Schließen“ im Hinweis auf Updates.
    CloseUpdates,
}

/// Text des Hinweises auf Updates für `count` Plug-ins.
pub fn updates_text(count: usize) -> String {
    tr!("plugin-updates-available", count = count)
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
/// „Neu starten“ (fehlt, wenn es endgültig aus ist) und „Schließen“; mit `updates` > 0 darüber
/// der Hinweis „Für N Plug-ins gibt es Updates.“ mit „Anzeigen“ und „Schließen“.
pub fn show_banners(
    ctx: &egui::Context,
    lines: &[BannerLine<'_>],
    updates: usize,
) -> Option<BannerAction> {
    if lines.is_empty() && updates == 0 {
        return None;
    }
    let mut action = None;
    egui::Area::new(egui::Id::new("planora_plugin_banners"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 40.0))
        .movable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                if updates > 0 {
                    ui.horizontal(|ui| {
                        ui.label(updates_text(updates));
                        if ui.button(tr!("plugin-updates-show")).clicked() {
                            action = Some(BannerAction::ShowUpdates);
                        }
                        if ui.button(tr!("plugin-banner-close")).clicked() {
                            action = Some(BannerAction::CloseUpdates);
                        }
                    });
                }
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

/// Kurzer Name eines Rechts (Einstellungen, Installationsdialog); `hosts` nur für `net`.
pub fn right_name(right: Right, hosts: &[String]) -> String {
    match right {
        Right::Net => tr!("plugin-right-name-net", hosts = hosts.join(", ")),
        Right::FsFolder => tr!("plugin-right-name-fs-folder"),
        Right::ModelRead => tr!("plugin-right-name-model-read"),
        Right::ModelWrite => tr!("plugin-right-name-model-write"),
    }
}

/// Zustand eines verlangten Rechts: erlaubt, abgelehnt oder offen (`None`, fragt beim ersten
/// Gebrauch; „Nur diesmal“ zählt bis zum Ende des Aufrufs als erlaubt).
pub fn grant_text(grant: Option<Grant>) -> String {
    match grant {
        Some(Grant::Allowed) => tr!("plugin-right-state-allowed"),
        Some(Grant::Denied) => tr!("plugin-right-state-denied"),
        None => tr!("plugin-right-state-ask"),
    }
}

/// Herkunft eines Fundorts: „mitgeliefert“ oder „installiert“.
pub fn origin_text(origin: Origin) -> String {
    match origin {
        Origin::Bundled => tr!("plugin-origin-bundled"),
        Origin::User => tr!("plugin-origin-user"),
    }
}

/// Zustand eines Plug-ins in der Verwaltung.
pub fn state_text(state: &EntryState) -> String {
    match state {
        EntryState::Off => tr!("plugin-state-off"),
        EntryState::Loading => tr!("plugin-state-loading"),
        EntryState::Running => tr!("plugin-state-running"),
        EntryState::Failed(reason) => tr!("plugin-state-failed", reason = reason.as_str()),
        EntryState::Stopped => tr!("plugin-state-stopped"),
    }
}

/// Text der Rückfrage, wenn die Kennung schon installiert ist.
pub fn replace_text(old: &str, new: &str) -> String {
    tr!("plugin-install-replace", old = old, new = new)
}

/// Ein gelesenes Paket, das auf „Installieren“ oder „Abbrechen“ wartet. Woher es kommt (Datei,
/// später ein Repo-Link) steht nur in `source`; der Dialog liest selbst nichts.
#[derive(Clone, Debug)]
pub struct InstallOffer {
    pub package: Package,
    /// Herkunft für die Anzeige („Quelle: …“), etwa der Pfad der Datei; `None` zeigt nichts.
    pub source: Option<String>,
    /// Version der schon vorhandenen Kennung (dann fragt der Dialog „Version A ersetzen durch
    /// Version B?“).
    pub replaces: Option<String>,
    /// Je verlangtem Recht, was nach dem Installieren gilt (`None`: wird beim ersten Gebrauch
    /// gefragt); siehe [`Rights::carry_over`](planora_plugin_host::rights::Rights::carry_over).
    pub grants: Vec<(Right, Option<Grant>)>,
    /// Über den Repo-Link geholt: Link und Tag für `herkunft.json` (aus einer Datei: keine).
    pub provenance: Option<Provenance>,
}

/// Zustand eines verlangten Rechts im Installationsdialog („bereits erlaubt“, „abgelehnt“,
/// „wird beim ersten Gebrauch gefragt“).
pub fn install_grant_text(grant: Option<Grant>) -> String {
    match grant {
        Some(Grant::Allowed) => tr!("plugin-install-right-allowed"),
        Some(Grant::Denied) => tr!("plugin-install-right-denied"),
        None => tr!("plugin-install-right-ask"),
    }
}

/// Was der Installationsdialog zu einem verlangten Recht zeigt: ein Recht, das diese Version noch
/// nicht umsetzt ([`Right::available`]), als „noch nicht verfügbar“ (es wird ohne Nachfrage
/// abgelehnt), sonst [`install_grant_text`].
pub fn install_right_text(right: Right, grant: Option<Grant>) -> String {
    if right.available() {
        install_grant_text(grant)
    } else {
        tr!("plugin-install-right-unavailable")
    }
}

/// Wahl im Installationsdialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallChoice {
    Install,
    Cancel,
}

/// Der Installationsdialog (festes Fenster in der Mitte, Id `"planora_plugin_install"`): Name,
/// Version, Autor, Lizenz (mit Hinweis, wenn sie nicht GPL-verträglich ist), Signatur, verlangte
/// Rechte, Speicherbedarf (nur wenn nicht [`DEFAULT_MEMORY_MB`]), Quelle und gegebenenfalls die
/// Ersetzen-Frage; Knöpfe „Installieren“ und „Abbrechen“ (Esc wirkt wie „Abbrechen“). `locked`:
/// eine Rückfrage ist offen, „Installieren“ ist gesperrt (Grund beim Überfahren).
pub fn show_install(
    ctx: &egui::Context,
    offer: &InstallOffer,
    locked: bool,
) -> Option<InstallChoice> {
    let m = &offer.package.manifest;
    let mut choice = None;
    Dialog::new(tr!("plugin-install-title"), DialogSize::Prompt)
        .id(egui::Id::new("planora_plugin_install"))
        .fixed()
        .show(ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            ui.strong(m.name.as_str());
            ui.label(tr!(
                "plugin-install-version",
                version = m.version.to_string()
            ));
            ui.label(tr!("plugin-settings-author", author = m.author.as_str()));
            ui.label(tr!("plugin-settings-license", license = m.license.as_str()));
            if !license_ok(&m.license) {
                ui.colored_label(theme::palette(dark).warning, tr!("plugin-license-unknown"));
            }
            ui.label(match offer.package.signature {
                Signature::None => tr!("plugin-install-signature-none"),
                Signature::Present => tr!("plugin-install-signature-present"),
            });
            if m.rights.is_empty() {
                ui.label(tr!("plugin-settings-no-rights"));
            } else {
                ui.label(tr!("plugin-settings-rights"));
                ui.indent("planora_plugin_install_rights", |ui| {
                    for &(r, grant) in &offer.grants {
                        ui.horizontal(|ui| {
                            ui.label(right_name(r, &m.net_hosts));
                            ui.weak(install_right_text(r, grant));
                        });
                    }
                });
                if offer
                    .grants
                    .iter()
                    .any(|(r, g)| r.available() && g.is_none())
                {
                    ui.label(tr!("plugin-install-rights-hint"));
                }
            }
            if m.memory_mb != DEFAULT_MEMORY_MB {
                ui.label(tr!("plugin-install-memory", mb = m.memory_mb.to_string()));
            }
            if let Some(source) = &offer.source {
                ui.add(
                    egui::Label::new(tr!("plugin-install-source", source = source.as_str())).wrap(),
                );
            }
            if let Some(old) = &offer.replaces {
                ui.add_space(6.0);
                ui.strong(replace_text(old, &m.version.to_string()));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!locked, egui::Button::new(tr!("plugin-install-confirm")))
                    .on_disabled_hover_text(tr!("menu-answer-prompt-first"))
                    .clicked()
                {
                    choice = Some(InstallChoice::Install);
                }
                if ui.button(tr!("common-cancel")).clicked() {
                    choice = Some(InstallChoice::Cancel);
                }
            });
            if choice.is_none() && escape_pressed(ui, true) {
                choice = Some(InstallChoice::Cancel);
            }
        });
    choice
}

/// Was der Aktualisieren-Dialog zeigt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateView {
    /// Anzeigename des Plug-ins.
    pub name: String,
    pub from: String,
    pub to: String,
    /// Das Paket wird noch geladen: nur Versionen und „Wird geholt…“, „Aktualisieren“ gesperrt.
    pub loading: bool,
    /// Release-Notizen (Markdown).
    pub notes: String,
    /// Rechte, die das neue Paket zusätzlich verlangt.
    pub new_rights: Vec<Right>,
    /// Rechner aus dem neuen Manifest (für `net`).
    pub hosts: Vec<String>,
}

/// Wahl im Aktualisieren-Dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateChoice {
    Update,
    Cancel,
}

/// Zeile „Neu verlangte Rechte: …“ (leer: keine Zeile).
pub fn new_rights_text(rights: &[Right], hosts: &[String]) -> Option<String> {
    if rights.is_empty() {
        return None;
    }
    let names: Vec<String> = rights.iter().map(|&r| right_name(r, hosts)).collect();
    Some(tr!("plugin-update-new-rights", rights = names.join(", ")))
}

/// Der Aktualisieren-Dialog (festes Fenster in der Mitte, Id `"planora_plugin_update"`): alte →
/// neue Version, die Release-Notizen (zerlegt wie im Update-Dialog des Programms), neu verlangte
/// Rechte mit dem Hinweis, dass beim ersten Gebrauch gefragt wird; „Aktualisieren“ und
/// „Abbrechen“ (Esc wirkt wie „Abbrechen“). `locked`: eine Rückfrage ist offen, „Aktualisieren“
/// ist gesperrt.
pub fn show_update(ctx: &egui::Context, view: &UpdateView, locked: bool) -> Option<UpdateChoice> {
    let mut choice = None;
    Dialog::new(tr!("plugin-update-title"), DialogSize::Prompt)
        .id(egui::Id::new("planora_plugin_update"))
        .fixed()
        .show(ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            ui.strong(tr!(
                "plugin-update-versions",
                plugin = view.name.as_str(),
                from = view.from.as_str(),
                to = view.to.as_str()
            ));
            ui.add_space(4.0);
            let blocks = planora_update::parse_notes(&view.notes);
            if view.loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!("plugin-install-fetching"));
                });
            } else if blocks.is_empty() {
                ui.weak(tr!("update-no-notes"));
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("planora_plugin_update_notes")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for block in &blocks {
                            crate::update_dialog::note_block(ui, block, dark);
                        }
                    });
            }
            if let Some(text) = new_rights_text(&view.new_rights, &view.hosts) {
                ui.add_space(6.0);
                ui.add(egui::Label::new(text).wrap());
                ui.label(tr!("plugin-install-rights-hint"));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !locked && !view.loading,
                        egui::Button::new(tr!("plugin-update-confirm")),
                    )
                    .on_disabled_hover_text(if view.loading {
                        tr!("plugin-install-fetching")
                    } else {
                        tr!("menu-answer-prompt-first")
                    })
                    .clicked()
                {
                    choice = Some(UpdateChoice::Update);
                }
                if ui.button(tr!("common-cancel")).clicked() {
                    choice = Some(UpdateChoice::Cancel);
                }
            });
            if choice.is_none() && escape_pressed(ui, true) {
                choice = Some(UpdateChoice::Cancel);
            }
        });
    choice
}
