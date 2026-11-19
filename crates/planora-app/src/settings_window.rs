//! Fenster „Einstellungen“ (Spec §5): Abschnitte untereinander mit Überschrift, damit später
//! weitere dazukommen. Oben „Darstellung“ mit dem Regler „Durchsichtigkeit der Panels“ (wirkt
//! sofort, wird gespeichert wie das Projektverzeichnis) und der Auswahl „Sprache“ (wirkt im
//! nächsten Bild, gespeichert sofort), darunter „Updates“ (laufende Version, Update-Kanal,
//! Suchen beim Start, letzte Prüfung; gespeichert sofort; Knopf „Jetzt nach Updates
//! suchen“ mit Ergebnis und bei einem Fund „Meldung anzeigen“, die Suche selbst macht die App;
//! eine übersprungene Version mit „Doch wieder anzeigen“), dann „Plug-ins“ (je Plug-in Schalter,
//! Zustand und aufklappbar Autor, Lizenz, Rechte mit „Zurücknehmen“ und „Entfernen“; Knopf
//! „Plug-in installieren…“ mit der Dateiauswahl für `*.pcplugin`), dann „Projekte“ mit dem
//! Projektverzeichnis und zuletzt „Tastenkürzel“ ([`crate::keymap::settings_ui`]; zuletzt, weil
//! die lange Tabelle sonst „Projekte“ nach unten schöbe). Gewählt wird über die
//! Dateiauswahl im Ordner-Modus; der Wechsel wirkt wie [`Workspace::choose_root`] und
//! [`Workspace::persist_config`] (nur `root` wird geschrieben, Favoriten bleiben). Nichts wird
//! verschoben, kopiert oder gelöscht.

use crate::dialog::{Dialog, DialogSize, close_label};
use crate::file_chooser::{self, ChooserAction, FileChooser, display_path};
use crate::keymap::settings_ui::{self, CaptureState};
use crate::places::{self, Places};
use crate::plugin_prompts::{grant_text, origin_text, right_name, state_text};
use crate::plugins::{self as app_plugins, Plugins, entry_name};
use crate::theme;
use crate::workspace::{Prompt, Workspace, root_locked};
use crate::{local_time, running_version};
use planora_i18n::{Lang, tr};
use planora_plugin_host::EntryState;
use planora_plugin_host::licenses::license_ok;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::places::Origin;
use planora_project::config::{Config, MAX_PANEL_TRANSPARENCY, clamp_panel_transparency};
use planora_project::library::{DOCUMENT_FILE, DOCUMENT_SUFFIX, PROJECT_FILE};
use planora_update::{Channel, Version};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// Titel des Fensters.
pub fn title() -> String {
    tr!("settings-title")
}
pub fn section_appearance() -> String {
    tr!("settings-section-appearance")
}
pub fn transparency_label() -> String {
    tr!("settings-transparency-label")
}
/// Warnung, wenn die Durchsichtigkeit gilt, aber nicht gespeichert werden konnte.
pub fn transparency_not_saved() -> String {
    tr!("settings-transparency-not-saved")
}
/// Beschriftung der Auswahl „Sprache“ im Abschnitt „Darstellung“.
pub fn section_language() -> String {
    tr!("settings-language")
}
/// Warnung, wenn die Sprache gilt, aber nicht gespeichert werden konnte.
pub fn language_not_saved() -> String {
    tr!("settings-language-not-saved")
}
/// Beschriftung der Auswahl „Maßeinheit“ im Abschnitt „Darstellung“.
pub fn section_unit() -> String {
    tr!("settings-unit-label")
}
/// Eintrag „Millimeter (mm)“ der Auswahl „Maßeinheit“.
pub fn unit_mm() -> String {
    tr!("settings-unit-mm")
}
/// Eintrag „Zoll (in)“ der Auswahl „Maßeinheit“.
pub fn unit_inch() -> String {
    tr!("settings-unit-inch")
}
/// Warnung, wenn die Maßeinheit gilt, aber nicht gespeichert werden konnte.
pub fn unit_not_saved() -> String {
    tr!("settings-unit-not-saved")
}
/// Breite der Schieberspur des Reglers.
pub const SLIDER_WIDTH: f32 = 200.0;
pub fn section_updates() -> String {
    tr!("settings-section-updates")
}
pub fn channel_label() -> String {
    tr!("settings-channel-label")
}
/// Kanäle in der Auswahl „Update-Kanal“ (heute nur Dev; später kommen Beta und Stabil dazu).
pub const AVAILABLE_CHANNELS: &[Channel] = &[Channel::Dev];
pub fn check_on_start_label() -> String {
    tr!("settings-check-on-start")
}
/// Hinweis zum Datenschutz unter dem Schalter.
pub fn privacy_note() -> String {
    tr!("settings-privacy-note")
}
pub fn check_now() -> String {
    tr!("settings-check-now")
}
/// Neben dem Knopf [`check_now`], solange die Suche läuft (der Knopf ist dann aus).
pub fn check_running() -> String {
    tr!("settings-check-running")
}
/// Ergebnis der Suche über den Knopf: keine neuere Version.
pub fn up_to_date() -> String {
    tr!("settings-up-to-date")
}
/// Ergebnis der Suche über den Knopf: Abruf oder Antwort gescheitert.
pub fn check_failed() -> String {
    tr!("settings-check-failed")
}
/// Ergebnis der Suche über den Knopf auf einem System ohne Aktualisierung über das Programm
/// (alles außer Windows).
pub fn not_on_this_system() -> String {
    tr!("settings-not-on-this-system")
}
/// Knopf neben „Neue Version gefunden“: öffnet die Meldung mit Jetzt, Später, Überspringen.
pub fn show_notice() -> String {
    tr!("settings-show-notice")
}
/// Knopf neben der übersprungenen Version: hebt das Überspringen auf und sucht gleich nach.
pub fn unskip() -> String {
    tr!("settings-unskip")
}
/// Statt eines Zeitpunkts, wenn noch nie geprüft wurde.
pub fn never_checked() -> String {
    tr!("settings-never-checked")
}
/// Warnung, wenn eine Update-Einstellung gilt, aber nicht gespeichert werden konnte.
pub fn updates_not_saved() -> String {
    tr!("settings-updates-not-saved")
}
/// Schalter im Abschnitt „Updates“ unter [`check_on_start_label`].
pub fn verbose_label() -> String {
    tr!("settings-verbose-label")
}
/// Hinweis unter dem Schalter [`verbose_label`] (schwache Zeile wie der Datenschutzhinweis darüber).
pub fn verbose_note() -> String {
    tr!("settings-verbose-note")
}
/// Warnung, wenn der Schalter gilt, aber nicht gespeichert werden konnte.
pub fn verbose_not_saved() -> String {
    tr!("settings-verbose-not-saved")
}
pub fn section_projects() -> String {
    tr!("settings-section-projects")
}
pub fn directory_label() -> String {
    tr!("settings-directory-label")
}
pub fn change_directory() -> String {
    tr!("settings-change-directory")
}
pub fn choose_heading() -> String {
    tr!("settings-choose-heading")
}
pub fn keep_hint() -> String {
    tr!("settings-keep-hint")
}
/// Anfang der Meldung, wenn sich das gewählte Verzeichnis nicht lesen lässt (danach der Grund).
pub fn unreadable() -> String {
    tr!("settings-unreadable")
}
/// Meldung, wenn das gewählte Verzeichnis selbst ein Projekt oder Dokument ist.
pub fn nested() -> String {
    tr!("settings-nested")
}
/// Warnung, wenn der Wechsel gilt, die Einstellung aber nicht gespeichert werden konnte.
pub fn not_saved() -> String {
    tr!("settings-not-saved")
}

/// Ab dieser Breite des Programmfensters stehen „Darstellung“ und „Updates“ nebeneinander (so
/// bleibt bei 1920 × 1080 Platz für die Tabelle der Tastenkürzel).
pub const TWO_COLUMNS_FROM: f32 = 1600.0;
/// Breite der Spalte „Darstellung“ neben „Updates“.
const APPEARANCE_COLUMN_W: f32 = 280.0;

/// Kennung des Fensters (unabhängig vom Titel).
const WINDOW_ID: &str = "planora_settings_window";

/// „Laufende Version: 0.2.0-dev.1 (Dev)“ für die laufende Version.
pub fn running_version_line() -> String {
    running_version_line_for(&running_version())
}

/// „Laufende Version: <Version> (<Kanal>)“.
pub fn running_version_line_for(v: &Version) -> String {
    tr!(
        "settings-running-version",
        version = v.to_string(),
        channel = v.channel().label()
    )
}

/// „Letzte Prüfung: 25.10.2026, 18:42“ (Ortszeit) oder „Letzte Prüfung: noch nie“.
pub fn last_check_line(unix_seconds: Option<u64>) -> String {
    let when = unix_seconds.map_or_else(never_checked, |t| {
        local_time::unix_time_text(i64::try_from(t).unwrap_or(i64::MAX))
    });
    tr!("settings-last-check", when = when)
}

/// „Neue Version gefunden: 0.2.0-dev.2“ (Ergebnis der Suche über den Knopf).
pub fn update_found_line(v: &Version) -> String {
    tr!("settings-update-found", version = v.to_string())
}

/// „Übersprungen: Version 0.2.0-dev.2“ (Einstellungen → Updates).
pub fn skipped_line(v: &Version) -> String {
    tr!("settings-skipped", version = v.to_string())
}

/// Zustand der Suche über den Knopf [`check_now`], neben dem Knopf angezeigt. Die App setzt ihn
/// ([`SettingsWindow::set_check_hint`]); eine Suche beim Start zeigt hier nichts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckHint {
    /// Die Suche läuft ([`check_running`]).
    Running,
    /// [`up_to_date`].
    UpToDate,
    /// Neue Version gefunden ([`update_found_line`]).
    Found(Version),
    /// [`check_failed`] (Warnfarbe).
    Failed,
    /// [`not_on_this_system`].
    Unsupported,
}

impl CheckHint {
    pub fn text(&self) -> String {
        match self {
            CheckHint::Running => check_running(),
            CheckHint::UpToDate => up_to_date(),
            CheckHint::Found(v) => update_found_line(v),
            CheckHint::Failed => check_failed(),
            CheckHint::Unsupported => not_on_this_system(),
        }
    }
}

/// Grund einer Meldung im Fenster. Gespeichert wird der Grund, nicht der Text: gezeichnet wird in
/// der Sprache des Bildes ([`Note::text`]), auch wenn sie bei offenem Fenster wechselt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    TransparencyNotSaved,
    UpdatesNotSaved,
    VerboseNotSaved,
    LanguageNotSaved,
    UnitNotSaved,
    KeysNotSaved,
    /// Das neue Projektverzeichnis gilt nur bis zum Programmende.
    NotSaved,
    /// Gespeicherte Dokumente sind offen.
    RootLocked,
    /// Das Ziel ist selbst ein Projekt oder Dokument.
    Nested,
    /// Das Ziel lässt sich nicht lesen (mit dem Grund).
    Unreadable(std::io::ErrorKind),
    /// Fertiger Text aus dem Betriebssystem oder einer Bibliothek (nicht übersetzt).
    Raw(String),
}

impl Note {
    /// Der Text in der aktuellen Sprache.
    pub fn text(&self) -> String {
        match self {
            Note::TransparencyNotSaved => transparency_not_saved(),
            Note::UpdatesNotSaved => updates_not_saved(),
            Note::VerboseNotSaved => verbose_not_saved(),
            Note::LanguageNotSaved => language_not_saved(),
            Note::UnitNotSaved => unit_not_saved(),
            Note::KeysNotSaved => settings_ui::keys_not_saved(),
            Note::NotSaved => not_saved(),
            Note::RootLocked => root_locked(),
            Note::Nested => nested(),
            Note::Unreadable(kind) => {
                format!("{} {}", unreadable(), file_chooser::io_message(*kind))
            }
            Note::Raw(text) => text.clone(),
        }
    }
}

/// Meldung nach einem Wechsel: Warnung (Warnfarbe) oder Fehler (Fehlerfarbe).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootMessage {
    Warning(Note),
    Error(Note),
}

impl RootMessage {
    /// Der Text in der aktuellen Sprache.
    pub fn text(&self) -> String {
        match self {
            RootMessage::Warning(n) | RootMessage::Error(n) => n.text(),
        }
    }

    pub fn color(&self, dark: bool) -> egui::Color32 {
        match self {
            RootMessage::Warning(_) => theme::palette(dark).warning,
            RootMessage::Error(_) => theme::palette(dark).error,
        }
    }
}

/// Angezeigter Pfad: gewählte Bibliothek (ohne Windows-Präfix für lange Pfade), sonst
/// `config.root`, sonst [`Config::default_root`].
pub fn current_directory(ws: &Workspace) -> PathBuf {
    match (&ws.library, &ws.config.root) {
        (Some(lib), _) => PathBuf::from(display_path(lib.root().path())),
        (None, Some(root)) => root.clone(),
        (None, None) => Config::default_root(),
    }
}

/// Ist `dir` der Ordner der geöffneten Bibliothek (auch anders geschrieben)?
fn is_open_library(ws: &Workspace, dir: &Path) -> bool {
    let Some(lib) = &ws.library else {
        return false;
    };
    let root = lib.root().path();
    places::same_dir(dir, &current_directory(ws))
        || std::fs::canonicalize(dir).is_ok_and(|c| places::same_dir(&c, root))
}

/// Prüft ein vorhandenes Ziel vor dem Wechsel: lesbar und nicht selbst ein Projekt- oder
/// Dokumentordner. Ein fehlendes Ziel legt `choose_root` an.
fn check_target(dir: &Path) -> Result<(), RootMessage> {
    if !dir.exists() {
        return Ok(());
    }
    if let Err(e) = std::fs::read_dir(dir) {
        return Err(RootMessage::Error(Note::Unreadable(e.kind())));
    }
    let document_name = dir.file_name().is_some_and(|n| {
        n.to_string_lossy()
            .to_lowercase()
            .ends_with(DOCUMENT_SUFFIX)
    });
    if document_name || dir.join(PROJECT_FILE).exists() || dir.join(DOCUMENT_FILE).exists() {
        return Err(RootMessage::Error(Note::Nested));
    }
    Ok(())
}

/// Wechselt das Projektverzeichnis wie `Workspace::choose_root` + `persist_config`.
/// Gleicher Ordner (nach `places::same_dir`) → Ok ohne Schreiben. Offene gespeicherte Dokumente
/// → Warning mit dem Wortlaut aus `choose_root`; ein nicht lesbares Ziel, ein Projekt- oder
/// Dokumentordner und `Library::open`-Fehler → Error (altes Verzeichnis und Datei bleiben).
/// Lässt sich die Einstellung nicht speichern, gilt der Wechsel bis Programmende (Warning
/// [`not_saved`]).
pub fn change_root(ws: &mut Workspace, dir: &Path) -> Result<(), RootMessage> {
    if is_open_library(ws, dir) {
        return Ok(());
    }
    if ws.has_saved_documents() {
        return Err(RootMessage::Warning(Note::RootLocked));
    }
    check_target(dir)?;
    ws.choose_root(dir)
        .map_err(|e| RootMessage::Error(Note::Raw(e)))?;
    if !ws.try_persist_config() {
        return Err(RootMessage::Warning(Note::NotSaved));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsEvent {
    None,
    Closed { return_to_projects: bool },
}

/// Ein Abschnitt, an dem das Fenster geöffnet werden kann (Datei → „Zusatzmodule“ →
/// „Verwalten …“ öffnet am Abschnitt Plug-ins).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSection {
    /// Die Plug-ins ([`plugins_title`]).
    Plugins,
}

/// Wofür die Dateiauswahl des Fensters gerade wählt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChooserPurpose {
    /// Das Projektverzeichnis (Ordner-Modus, Abschnitt „Projekte“).
    #[default]
    Directory,
    /// Ein Plug-in-Paket `*.pcplugin` (Abschnitt „Plug-ins“).
    Plugin,
}

/// So viele Bilder lang rollt das Fenster zum Ziel aus [`SettingsWindow::open_at`] (das erste
/// Bild eines neu geöffneten Fensters misst nur aus).
const SCROLL_FRAMES: u8 = 2;

/// Größte Höhe der Liste im Abschnitt „Plug-ins“; darüber rollt sie (der Abschnitt schiebt das
/// Fenster sonst über die Menüleiste).
pub const PLUGIN_LIST_MAX_H: f32 = 240.0;

/// Überschrift des Abschnitts „Plug-ins“.
pub fn plugins_title() -> String {
    tr!("settings-section-plugins")
}
/// Knopf „Plug-in installieren…“.
pub fn install_plugin_label() -> String {
    tr!("plugin-settings-install")
}
/// Knopf „Entfernen“ eines installierten Plug-ins.
pub fn remove_plugin_label() -> String {
    tr!("plugin-settings-remove")
}
/// Knopf „Zurücknehmen“ eines entschiedenen Rechts.
pub fn revoke_label() -> String {
    tr!("plugin-right-revoke")
}
/// Zeile eines nicht aktiven Fundorts: „installiert 0.1.0: überdeckt von Version 0.2.0“.
pub fn shadowed_text(origin: Origin, version: &str, active: &str) -> String {
    tr!(
        "plugin-settings-shadowed",
        origin = origin_text(origin),
        version = version,
        active = active
    )
}

/// Id des Aufklapp-Zustands eines Plug-ins im Abschnitt „Plug-ins“ (Tests klappen damit auf).
pub fn plugin_details_id(plugin: &str) -> egui::Id {
    egui::Id::new(("planora_plugin_details", plugin))
}

/// Was im Abschnitt „Plug-ins“ gedrückt wurde (ausgeführt nach dem Zeichnen).
#[derive(Clone, Debug, PartialEq, Eq)]
enum PluginAction {
    Enable(String, bool),
    Revoke(String, Right),
    Remove(String),
    Install,
}

#[derive(Default)]
pub struct SettingsWindow {
    open: bool,
    /// Der Abschnitt, zu dem das Fenster beim Öffnen rollen soll ([`SettingsWindow::open_at`]);
    /// nach [`SCROLL_FRAMES`] gezeichneten Bildern ist es abgeholt.
    scroll_target: Option<SettingsSection>,
    /// Bilder, in denen schon zum Ziel gerollt wurde.
    scroll_frames: u8,
    /// Beim Schließen wieder das Fenster „Projekte“ öffnen.
    return_to_projects: bool,
    /// Dateiauswahl, solange gewählt wird (wofür: `chooser_for`).
    chooser: Option<FileChooser>,
    chooser_for: ChooserPurpose,
    message: Option<RootMessage>,
    /// Hinweis unter dem Regler (nur [`transparency_not_saved`]).
    appearance_message: Option<RootMessage>,
    /// Hinweis im Abschnitt „Updates“ (nur [`updates_not_saved`]).
    updates_message: Option<RootMessage>,
    /// Zustand der Suche über den Knopf [`check_now`].
    check_hint: Option<CheckHint>,
    /// Das Ergebnis in `check_hint` war schon einmal im offenen Fenster zu sehen (erst dann
    /// leert es ein erneutes Öffnen).
    check_hint_shown: bool,
    /// Der Knopf [`check_now`] wurde gedrückt; die App holt das ab
    /// ([`SettingsWindow::take_check_request`]).
    check_requested: bool,
    /// Der Knopf [`show_notice`] wurde gedrückt; die App holt das ab
    /// ([`SettingsWindow::take_notice_request`]) und öffnet die Meldung.
    notice_requested: bool,
    /// Der Regler wurde bewegt, der Wert ist aber noch nicht gespeichert (gespeichert wird beim
    /// Loslassen, nicht in jedem Bild des Ziehens).
    transparency_unsaved: bool,
    /// Abschnitt „Tastenkürzel“: Suche, Erfassen, Doppelbelegung, Hinweise.
    keys: CaptureState,
    /// Rechteck des Fensters im letzten Bild (Layouttests).
    pub last_rect: Option<egui::Rect>,
}

impl SettingsWindow {
    /// Öffnet das Fenster; leert Meldung und Auswahl.
    pub fn open(&mut self, return_to_projects: bool) {
        self.open = true;
        self.scroll_target = None;
        self.return_to_projects = return_to_projects;
        self.chooser = None;
        self.message = None;
        self.appearance_message = None;
        self.updates_message = None;
        // Eine noch laufende Suche bleibt sichtbar, ebenso ein Ergebnis, das noch nie zu sehen war
        // (das Fenster ging zu, während die Suche lief); ein schon gezeigtes Ergebnis nicht.
        if self.check_hint_shown && self.check_hint != Some(CheckHint::Running) {
            self.check_hint = None;
        }
        self.keys = CaptureState::default();
    }

    /// Wie [`SettingsWindow::open`], dazu der Abschnitt, zu dem das Fenster rollen soll.
    pub fn open_at(&mut self, return_to_projects: bool, section: Option<SettingsSection>) {
        self.open(return_to_projects);
        self.scroll_target = section;
        self.scroll_frames = 0;
    }

    /// Der Abschnitt, zu dem das Fenster rollen soll (noch nicht abgeholt).
    pub fn scroll_target(&self) -> Option<SettingsSection> {
        self.scroll_target
    }

    /// Holt das Rollziel ab (das Zeichnen tut das selbst, sobald hingerollt ist).
    pub fn take_scroll_target(&mut self) -> Option<SettingsSection> {
        self.scroll_target.take()
    }

    /// Schließt das Fenster; ein laufendes Erfassen einer Taste endet (die Sperre der Kürzel
    /// fällt im nächsten Bild, siehe [`settings_ui::is_capturing`]).
    pub fn close(&mut self) {
        self.open = false;
        self.chooser = None;
        self.keys.stop();
    }

    /// Zustand des Abschnitts „Tastenkürzel“ (Suchtext, Erfassen; für Tests).
    #[doc(hidden)]
    pub fn keys_mut(&mut self) -> &mut CaptureState {
        &mut self.keys
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Öffnet die Auswahl im Ordner-Modus im aktuellen Verzeichnis, fehlt es, in dessen
    /// Elternordner, sonst in [`file_chooser::default_dir`].
    pub fn start_choosing(&mut self, ws: &Workspace) {
        let current = current_directory(ws);
        let dir = if current.is_dir() {
            current
        } else {
            match current.parent().filter(|p| p.is_dir()) {
                Some(parent) => parent.to_path_buf(),
                None => file_chooser::default_dir(),
            }
        };
        self.chooser = Some(FileChooser::folder(dir));
        self.chooser_for = ChooserPurpose::Directory;
    }

    /// Öffnet die Auswahl einer Datei `*.pcplugin` (Knopf „Plug-in installieren…“) in
    /// [`file_chooser::default_dir`]; die gewählte Datei liest [`Plugins::read_package`].
    pub fn start_choosing_plugin(&mut self) {
        self.chooser = Some(FileChooser::open(
            file_chooser::default_dir(),
            vec!["pcplugin"],
        ));
        self.chooser_for = ChooserPurpose::Plugin;
    }

    /// Wofür die offene Auswahl wählt.
    pub fn chooser_purpose(&self) -> Option<ChooserPurpose> {
        self.chooser.as_ref().map(|_| self.chooser_for)
    }

    pub fn chooser_mut(&mut self) -> Option<&mut FileChooser> {
        self.chooser.as_mut()
    }

    pub fn message(&self) -> Option<&RootMessage> {
        self.message.as_ref()
    }

    /// Hinweis des Abschnitts „Darstellung“ (Speichern fehlgeschlagen).
    pub fn appearance_message(&self) -> Option<&RootMessage> {
        self.appearance_message.as_ref()
    }

    /// Hinweis des Abschnitts „Updates“ (Speichern fehlgeschlagen).
    pub fn updates_message(&self) -> Option<&RootMessage> {
        self.updates_message.as_ref()
    }

    /// Zustand der Suche über den Knopf [`check_now`].
    pub fn check_hint(&self) -> Option<&CheckHint> {
        self.check_hint.as_ref()
    }

    /// Setzt den Zustand der Suche (die App, wenn sie startet und wenn das Ergebnis da ist).
    pub fn set_check_hint(&mut self, hint: Option<CheckHint>) {
        self.check_hint = hint;
        self.check_hint_shown = false;
    }

    /// Wurde der Knopf [`check_now`] seit dem letzten Abholen gedrückt? Setzt zurück.
    pub fn take_check_request(&mut self) -> bool {
        std::mem::take(&mut self.check_requested)
    }

    /// Wurde der Knopf [`show_notice`] seit dem letzten Abholen gedrückt? Setzt zurück.
    pub fn take_notice_request(&mut self) -> bool {
        std::mem::take(&mut self.notice_requested)
    }

    /// Hinweis nach dem Speichern einer Update-Einstellung: [`updates_not_saved`], wenn es
    /// misslang (der Wert gilt dann für die Sitzung), sonst keiner.
    fn note_updates_saved(&mut self, written: bool) {
        self.updates_message = (!written).then_some(RootMessage::Warning(Note::UpdatesNotSaved));
    }

    /// Was der Regler beim Ziehen tut: der Wert gilt sofort, ist aber noch nicht gespeichert. Er
    /// wird im Arbeitsbereich vorgemerkt (`panel_transparency_pending`), damit er auch dann in
    /// die Datei kommt, wenn das Fenster von außen ([`SettingsWindow::close`]) zugeht: beim
    /// nächsten gelungenen Schreiben der Konfiguration.
    pub fn preview_transparency(&mut self, ws: &mut Workspace, percent: u8) {
        ws.config.panel_transparency = clamp_panel_transparency(percent);
        ws.panel_transparency_pending = true;
        self.transparency_unsaved = true;
    }

    /// Speichert die Durchsichtigkeit (sie gilt schon); misslingt das, bleibt sie für die
    /// Sitzung und der Hinweis [`transparency_not_saved`] erscheint.
    fn save_transparency(&mut self, ws: &mut Workspace) {
        self.transparency_unsaved = false;
        self.appearance_message = (!ws.set_panel_transparency(ws.panel_transparency()))
            .then_some(RootMessage::Warning(Note::TransparencyNotSaved));
    }

    /// Wie in der App: [`SettingsWindow::show`] mit den Schnellzugriffen des Arbeitsbereichs;
    /// geänderte Favoriten werden danach gespeichert. Das Ereignis (auch der Rückweg zu
    /// „Projekte“) geht an den Aufrufer.
    pub fn show_in(&mut self, ctx: &egui::Context, ws: &mut Workspace) -> SettingsEvent {
        let mut places = ws.places();
        let event = self.show(ctx, ws, &mut places);
        ws.save_places(places);
        event
    }

    /// Zeichnet das Fenster (Id `"planora_settings_window"`, `center_on_open`, ✕, Fußzeile
    /// „Schließen“ bzw. beim Wählen die Fußzeile der Auswahl).
    ///
    /// Geänderte Favoriten der Auswahl stehen danach in `places` (der Aufrufer speichert sie mit
    /// [`Workspace::save_places`]).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        ws: &mut Workspace,
        places: &mut Places,
    ) -> SettingsEvent {
        if !self.open {
            self.last_rect = None;
            settings_ui::set_capturing(ctx, false);
            return SettingsEvent::None;
        }
        let (mut open, mut close, mut start) = (true, false, false);
        // Bei offener Rückfrage (etwa „Dokument gesperrt“, deren Pfad zum aktuellen Verzeichnis
        // gehört) ist der Inhalt gesperrt: kein Wechsel, keine Auswahl, bis sie beantwortet ist.
        // „Schließen“ und ✕ bleiben.
        let locked = ws.prompt.is_some();
        // Erfasst der Abschnitt „Tastenkürzel“ gerade eine Taste, gehört sie ihm: vor allen
        // Knöpfen und Feldern des Fensters.
        settings_ui::capture_input(ctx, ws, &mut self.keys, locked);
        let keys = &mut self.keys;
        let path = display_path(&current_directory(ws));
        let message = self.message.clone();
        let appearance_message = self.appearance_message.clone();
        let updates_message = self.updates_message.clone();
        let mut verbose = ws.verbose_log();
        let check_hint = self.check_hint.clone();
        self.check_hint_shown = true;
        let mut check_now = false;
        let mut show_notice = false;
        let skipped = ws.skipped_version();
        let mut unskip = false;
        let channel = ws.update_channel();
        let (mut chosen_channel, mut check_on_start) = (channel, ws.update_check_enabled());
        let last_check = last_check_line(ws.last_update_check());
        let mut transparency = ws.panel_transparency();
        let (mut slid, mut released) = (false, false);
        // Vorgewählt ist die Sprache, in der das Fenster gerade spricht.
        let language = planora_i18n::current();
        let mut chosen_language: Option<Lang> = None;
        let unit = planora_i18n::current_unit();
        let mut chosen_unit: Option<planora_i18n::DisplayUnit> = None;
        let choosing = self.chooser.is_some();
        let chooser_for = self.chooser_for;
        let scroll_plugins = self.scroll_target == Some(SettingsSection::Plugins);
        let mut plugin_actions: Vec<PluginAction> = Vec::new();
        // Die Auswahl zeichnet oben (rollt) und in der Fußzeile (wählen, abbrechen).
        let chooser = RefCell::new(self.chooser.as_mut());
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        let response = Dialog::new(title(), DialogSize::File)
            .dock_key("settings")
            .id(egui::Id::new(WINDOW_ID))
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    ui.add_enabled_ui(!locked, |ui| {
                        let dark = ui.visuals().dark_mode;
                        let mut appearance = |ui: &mut egui::Ui| {
                            show_appearance(
                                ui,
                                &mut transparency,
                                (&mut slid, &mut released),
                                (&language, &mut chosen_language),
                                (unit, &mut chosen_unit),
                                appearance_message.as_ref(),
                            )
                        };
                        let mut updates = |ui: &mut egui::Ui| {
                            show_updates(
                                ui,
                                &mut chosen_channel,
                                &mut check_on_start,
                                &last_check,
                                UpdateButtons {
                                    hint: check_hint.as_ref(),
                                    check_now: &mut check_now,
                                    show_notice: &mut show_notice,
                                    skipped,
                                    unskip: &mut unskip,
                                    verbose: &mut verbose,
                                },
                                updates_message.as_ref(),
                            )
                        };
                        if ctx.content_rect().width() >= TWO_COLUMNS_FROM {
                            // Nebeneinander; die Reihe ist so hoch wie die höhere Spalte.
                            ui.horizontal_top(|ui| {
                                column(ui, APPEARANCE_COLUMN_W, &mut appearance);
                                ui.add_space(theme::SPACE_6);
                                let rest = ui.available_width();
                                column(ui, rest, &mut updates);
                            });
                        } else {
                            appearance(ui);
                            ui.add_space(theme::SPACE_4);
                            updates(ui);
                        }
                        ui.add_space(theme::SPACE_4);
                        if scroll_plugins {
                            // Ohne Animation: das Fenster öffnet gleich am Abschnitt.
                            ui.scroll_to_cursor_animation(
                                Some(egui::Align::TOP),
                                egui::style::ScrollAnimation::none(),
                            );
                        }
                        let plugins_top = ui.cursor().top();
                        show_plugins(ui, &ws.plugins, choosing, &mut plugin_actions);
                        // Der Abschnitt kam nach der Bemessung der Tastenkürzel-Tabelle dazu: um
                        // seine Höhe wird sie niedriger (die Liste darin ist begrenzt).
                        let plugins_h = ui.cursor().top() - plugins_top + theme::SPACE_4;
                        if chooser_for == ChooserPurpose::Plugin
                            && let Some(c) = chooser.borrow_mut().as_mut()
                        {
                            ui.add_space(theme::SPACE_2);
                            ui.strong(install_plugin_label());
                            body_action = c.show_body(ui, places);
                        }
                        ui.add_space(theme::SPACE_4);
                        theme::section_header(ui, &section_projects(), theme::SUB_HEADER_H);
                        ui.add_space(theme::SPACE_2);
                        ui.strong(directory_label());
                        ui.add(egui::Label::new(path.as_str()).wrap());
                        if !choosing && ui.button(change_directory()).clicked() {
                            start = true;
                        }
                        ui.label(keep_hint());
                        if let Some(m) = &message {
                            ui.colored_label(m.color(dark), m.text());
                        }
                        if chooser_for == ChooserPurpose::Directory
                            && let Some(c) = chooser.borrow_mut().as_mut()
                        {
                            ui.add_space(theme::SPACE_2);
                            ui.strong(choose_heading());
                            body_action = c.show_body(ui, places);
                        }
                        ui.add_space(theme::SPACE_4);
                        settings_ui::show_reserving(ui, ws, keys, plugins_h);
                    });
                },
                |ui| match chooser.borrow_mut().as_mut() {
                    Some(c) => {
                        footer_action = ui.add_enabled_ui(!locked, |ui| c.show_footer(ui)).inner
                    }
                    None => {
                        if ui.button(close_label()).clicked() {
                            close = true;
                        }
                    }
                },
            );
        self.last_rect = response.map(|r| r.response.rect);
        if scroll_plugins {
            self.scroll_frames += 1;
            if self.scroll_frames >= SCROLL_FRAMES {
                self.take_scroll_target();
            }
        }
        if !locked {
            for action in plugin_actions {
                self.apply_plugin_action(ws, action);
            }
        }
        // Wirkt sofort; gespeichert wird, sobald der Regler losgelassen ist (Tastatur sofort).
        if slid && !locked {
            self.preview_transparency(ws, transparency);
        }
        if self.transparency_unsaved && released {
            self.save_transparency(ws);
        }
        // Die Sprache wirkt ab dem nächsten Bild und wird sofort gespeichert.
        if let Some(lang) = chosen_language
            && !locked
            && lang.id() != language.id()
        {
            let written = ws.set_language(&lang);
            self.appearance_message =
                (!written).then_some(RootMessage::Warning(Note::LanguageNotSaved));
        }
        // Die Maßeinheit wirkt ab dem nächsten Bild und wird sofort gespeichert.
        if let Some(chosen) = chosen_unit
            && !locked
            && chosen != unit
        {
            let written = ws.set_length_unit(chosen);
            self.appearance_message =
                (!written).then_some(RootMessage::Warning(Note::UnitNotSaved));
        }
        if check_now && !locked {
            self.check_requested = true;
        }
        if show_notice && !locked {
            self.notice_requested = true;
        }
        if unskip && !locked {
            // Wieder anbieten: das Überspringen aufheben und gleich nachsehen.
            let written = ws.set_skipped_version(None);
            self.note_updates_saved(written);
            self.check_requested = true;
        }
        // Update-Einstellungen gelten und werden sofort gespeichert.
        if !locked && chosen_channel != channel {
            let written = ws.set_update_channel(chosen_channel);
            self.note_updates_saved(written);
        }
        if !locked && check_on_start != ws.update_check_enabled() {
            let written = ws.set_update_check(check_on_start);
            self.note_updates_saved(written);
        }
        if !locked && verbose != ws.verbose_log() {
            let written = ws.set_verbose_log(verbose);
            self.updates_message =
                (!written).then_some(RootMessage::Warning(Note::VerboseNotSaved));
        }
        for action in [body_action, footer_action] {
            let target = match action {
                ChooserAction::Chosen(p) => Some(p),
                ChooserAction::Cancel => None,
                ChooserAction::None => continue,
            };
            // Erfolg wie Fehler schließen die Auswahl; ein Fehler bleibt als Meldung stehen.
            self.chooser = None;
            match (target, chooser_for) {
                (Some(dir), ChooserPurpose::Directory) => {
                    self.message = change_root(ws, &dir).err();
                }
                (Some(file), ChooserPurpose::Plugin) => ws.plugins.read_package(file),
                (None, _) => {}
            }
        }
        if start {
            self.start_choosing(ws);
        }
        if !open || close {
            // Ein noch nicht gespeicherter Reglerwert geht beim Schließen nicht verloren.
            if self.transparency_unsaved {
                self.save_transparency(ws);
            }
            let return_to_projects = self.return_to_projects;
            self.close();
            return SettingsEvent::Closed { return_to_projects };
        }
        SettingsEvent::None
    }
}

impl SettingsWindow {
    /// Führt eine Handlung aus dem Abschnitt „Plug-ins“ aus; Schalter und „Zurücknehmen“ werden
    /// gleich gespeichert ([`app_plugins::persist`]), „Entfernen“ fragt erst nach.
    fn apply_plugin_action(&mut self, ws: &mut Workspace, action: PluginAction) {
        ws.plugins.note = None;
        match action {
            PluginAction::Enable(id, on) => {
                ws.plugins.set_enabled(&id, on);
                app_plugins::persist(ws);
            }
            PluginAction::Revoke(id, right) => {
                ws.plugins.host.revoke(&id, right);
                app_plugins::persist(ws);
            }
            PluginAction::Remove(id) => {
                let name = ws.plugins.plugin_name(&id);
                ws.prompt = Some(Prompt::RemovePlugin { id, name });
            }
            PluginAction::Install => self.start_choosing_plugin(),
        }
    }
}

/// Abschnitt „Plug-ins“: je Plug-in eine Kopfzeile (Schalter mit Namen, Version, Herkunft,
/// Zustand), aufklappbar Autor, Lizenz, Beschreibung, Rechte mit „Zurücknehmen“ und bei
/// installierten „Entfernen“; nicht aktive Fundorte grau darunter. „Plug-in installieren…“
/// (nicht, solange gewählt, gelesen oder der Dialog gezeigt wird) steht rechts in der
/// Überschrift, ein Hinweis ([`Plugins::note`]) gleich darunter.
fn show_plugins(
    ui: &mut egui::Ui,
    plugins: &Plugins,
    choosing: bool,
    actions: &mut Vec<PluginAction>,
) {
    let dark = ui.visuals().dark_mode;
    let entries = plugins.host.entries();
    // Knopf und Leer-Hinweis stehen rechts in der Überschrift: der Abschnitt kostet so wenig
    // Höhe vor der Tastenkürzel-Tabelle (auch in schmalen Fenstern bleiben deren Zeilen im Bild).
    let header = theme::section_header(ui, &plugins_title(), theme::SUB_HEADER_H);
    let busy = choosing || plugins.is_reading() || plugins.install.is_some();
    let enabled = ui.is_enabled();
    let mut header_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(header.rect.shrink2(egui::vec2(theme::SPACE_1, 2.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    if header_ui
        .add_enabled(enabled && !busy, egui::Button::new(install_plugin_label()))
        .clicked()
    {
        actions.push(PluginAction::Install);
    }
    if plugins.is_reading() {
        header_ui.label(tr!("plugin-settings-reading"));
    } else if entries.is_empty() {
        header_ui.weak(tr!("plugin-settings-none"));
    }
    if let Some(note) = &plugins.note {
        ui.colored_label(theme::palette(dark).warning, note.as_str());
    }
    if entries.is_empty() {
        return;
    }
    ui.add_space(theme::SPACE_2);
    let states = plugins.host.plugin_states();
    let list = egui::ScrollArea::vertical()
        .id_salt("planora_plugin_list")
        .max_height(PLUGIN_LIST_MAX_H)
        .auto_shrink([false, true]);
    theme::scroll_area(ui, list, |ui| {
        for e in entries {
            let Some(shown) = e.active.or((!e.found.is_empty()).then_some(0)) else {
                continue;
            };
            let f = &e.found[shown];
            let m = &f.manifest;
            let state = match (&f.problem, e.active) {
                (Some(problem), None) => tr!("plugin-state-broken", problem = problem.as_str()),
                _ => state_text(&e.state),
            };
            let enabled = states.get(&e.id).is_none_or(|s| s.enabled);
            let version = m.version.to_string();
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                plugin_details_id(&e.id),
                false,
            )
            .show_header(ui, |ui| {
                let mut on = enabled;
                let toggle = ui
                    .add_enabled(
                        e.state != EntryState::Stopped,
                        egui::Checkbox::new(&mut on, entry_name(e)),
                    )
                    .on_hover_text(tr!("plugin-settings-enabled-hint"));
                if toggle.changed() {
                    actions.push(PluginAction::Enable(e.id.clone(), on));
                }
                ui.label(version.as_str());
                ui.weak(origin_text(f.origin));
                ui.label(state);
            })
            .body(|ui| {
                ui.label(tr!("plugin-settings-author", author = m.author.as_str()));
                ui.label(tr!("plugin-settings-license", license = m.license.as_str()));
                if !license_ok(&m.license) {
                    ui.colored_label(theme::palette(dark).warning, tr!("plugin-license-unknown"));
                }
                if !m.description.is_empty() {
                    ui.add(egui::Label::new(m.description.as_str()).wrap());
                }
                if m.rights.is_empty() {
                    ui.label(tr!("plugin-settings-no-rights"));
                } else {
                    ui.label(tr!("plugin-settings-rights"));
                    for &r in &m.rights {
                        let grant = e.rights.state(r);
                        ui.horizontal(|ui| {
                            ui.label(right_name(r, &m.net_hosts));
                            ui.weak(grant_text(grant));
                            if ui
                                .add_enabled(grant.is_some(), egui::Button::new(revoke_label()))
                                .on_hover_text(tr!("plugin-right-revoke-hint"))
                                .clicked()
                            {
                                actions.push(PluginAction::Revoke(e.id.clone(), r));
                            }
                        });
                    }
                }
                if e.found.iter().any(|g| g.origin == Origin::User)
                    && ui.button(remove_plugin_label()).clicked()
                {
                    actions.push(PluginAction::Remove(e.id.clone()));
                }
            });
            if let Some(active) = e.active {
                for (j, g) in e.found.iter().enumerate() {
                    if j != active {
                        ui.indent(("planora_plugin_shadowed", &e.id, j), |ui| {
                            ui.weak(shadowed_text(
                                g.origin,
                                &g.manifest.version.to_string(),
                                &version,
                            ));
                        });
                    }
                }
            }
        }
    });
}

/// Knöpfe und Anzeigen zur Suche im Abschnitt „Updates“ (gedrückt: das jeweilige Feld `true`).
struct UpdateButtons<'a> {
    /// Zustand der Suche neben [`check_now`].
    hint: Option<&'a CheckHint>,
    check_now: &'a mut bool,
    /// [`show_notice`] neben „Neue Version gefunden“.
    show_notice: &'a mut bool,
    /// Übersprungene Version (dann [`skipped_line`] mit [`unskip`]).
    skipped: Option<Version>,
    unskip: &'a mut bool,
    /// Schalter [`verbose_label`].
    verbose: &'a mut bool,
}

/// Abschnitt „Updates“: laufende Version und letzte Prüfung, Auswahl des Kanals (nur [`AVAILABLE_CHANNELS`]; ein
/// anderer gespeicherter Kanal steht nur als Anzeige da), Schalter mit Hinweis zum Datenschutz,
/// der Schalter [`verbose_label`] mit [`verbose_note`], der Knopf [`check_now`] mit dem Zustand der Suche daneben (aus, solange sie läuft; nach einem
/// Fund mit [`show_notice`]) und eine übersprungene Version mit [`unskip`].
fn show_updates(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    check_on_start: &mut bool,
    last_check: &str,
    buttons: UpdateButtons<'_>,
    message: Option<&RootMessage>,
) {
    let UpdateButtons {
        hint,
        check_now,
        show_notice,
        skipped,
        unskip,
        verbose,
    } = buttons;
    let dark = ui.visuals().dark_mode;
    // Knapp gehalten (je Zeile zwei Angaben), damit das Fenster bei 1920 × 1080 nicht bis über
    // die Menüleiste wächst.
    theme::section_header(ui, &section_updates(), theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_2);
    ui.horizontal(|ui| {
        ui.label(running_version_line());
        ui.add_space(theme::SPACE_6);
        ui.label(last_check);
    });
    ui.add_space(theme::SPACE_1);
    ui.horizontal(|ui| {
        ui.strong(channel_label());
        egui::ComboBox::from_id_salt("planora_settings_update_channel")
            .selected_text(channel.label())
            .show_ui(ui, |ui| {
                for c in AVAILABLE_CHANNELS {
                    ui.selectable_value(channel, *c, c.label());
                }
            });
        ui.add_space(theme::SPACE_6);
        let r = ui.checkbox(check_on_start, check_on_start_label());
        theme::focus_ring(ui, &r);
    });
    ui.add(egui::Label::new(egui::RichText::new(privacy_note()).weak()).wrap());
    ui.add_space(theme::SPACE_1);
    let r = ui.checkbox(verbose, verbose_label());
    theme::focus_ring(ui, &r);
    ui.add(egui::Label::new(egui::RichText::new(verbose_note()).weak()).wrap());
    ui.add_space(theme::SPACE_1);
    // Das Ergebnis steht neben dem Knopf, damit der Abschnitt nicht höher wird.
    ui.horizontal(|ui| {
        let running = hint == Some(&CheckHint::Running);
        let r = ui.add_enabled(!running, egui::Button::new(self::check_now()));
        theme::focus_ring(ui, &r);
        *check_now = r.clicked();
        match hint {
            Some(h @ CheckHint::Failed) => {
                ui.colored_label(theme::palette(dark).warning, h.text());
            }
            Some(h @ CheckHint::Found(_)) => {
                ui.label(h.text());
                let r = ui.button(self::show_notice());
                theme::focus_ring(ui, &r);
                *show_notice = r.clicked();
            }
            Some(h) => {
                ui.label(h.text());
            }
            None => {}
        }
    });
    if let Some(v) = skipped {
        ui.add_space(theme::SPACE_1);
        ui.horizontal(|ui| {
            ui.label(skipped_line(&v));
            let r = ui.button(self::unskip());
            theme::focus_ring(ui, &r);
            *unskip = r.clicked();
        });
    }
    if let Some(m) = message {
        ui.colored_label(m.color(dark), m.text());
    }
}

/// Abschnitt „Darstellung“: Regler der Durchsichtigkeit (`slid`: bewegt, `released`: nicht mehr
/// gezogen), Auswahl der Sprache (alle verfügbaren nach Eigenname, `current` vorgewählt; eine
/// gewählte steht danach in `chosen`) und ein Hinweis nach einem Schreibfehler.
fn show_appearance(
    ui: &mut egui::Ui,
    transparency: &mut u8,
    (slid, released): (&mut bool, &mut bool),
    (current, chosen): (&Lang, &mut Option<Lang>),
    (unit, chosen_unit): (
        planora_i18n::DisplayUnit,
        &mut Option<planora_i18n::DisplayUnit>,
    ),
    message: Option<&RootMessage>,
) {
    let dark = ui.visuals().dark_mode;
    theme::section_header(ui, &section_appearance(), theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_2);
    ui.strong(transparency_label());
    ui.spacing_mut().slider_width = SLIDER_WIDTH;
    let r = ui.add(
        egui::Slider::new(transparency, 0..=MAX_PANEL_TRANSPARENCY)
            .integer()
            .suffix(" %"),
    );
    theme::focus_ring(ui, &r);
    *slid = r.changed();
    *released = !r.is_pointer_button_down_on() && !r.dragged();
    // Beschriftung und Auswahl in einer Zeile, damit das Fenster bei 1400 × 900 nicht höher wird.
    ui.add_space(theme::SPACE_1);
    ui.horizontal(|ui| {
        ui.strong(section_language());
        let r = egui::ComboBox::from_id_salt("planora_settings_language")
            .selected_text(current.name())
            .show_ui(ui, |ui| {
                for lang in planora_i18n::available() {
                    let r = ui.selectable_label(lang.id() == current.id(), lang.name());
                    if r.clicked() {
                        *chosen = Some(lang);
                    }
                }
            });
        theme::focus_ring(ui, &r.response);
    });
    ui.add_space(theme::SPACE_1);
    ui.horizontal(|ui| {
        ui.strong(section_unit());
        let name = |u: planora_i18n::DisplayUnit| match u {
            planora_i18n::DisplayUnit::Millimeter => unit_mm(),
            planora_i18n::DisplayUnit::Zoll => unit_inch(),
        };
        let r = egui::ComboBox::from_id_salt("planora_settings_unit")
            .selected_text(name(unit))
            .show_ui(ui, |ui| {
                for u in [
                    planora_i18n::DisplayUnit::Millimeter,
                    planora_i18n::DisplayUnit::Zoll,
                ] {
                    let r = ui.selectable_label(u == unit, name(u));
                    if r.clicked() {
                        *chosen_unit = Some(u);
                    }
                }
            });
        theme::focus_ring(ui, &r.response);
    });
    if let Some(m) = message {
        ui.colored_label(m.color(dark), m.text());
    }
}

/// Eine Spalte der Breite `width` (von oben nach unten).
fn column(ui: &mut egui::Ui, width: f32, add: &mut dyn FnMut(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            add(ui);
        },
    );
}
