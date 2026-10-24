//! Mehrere offene Dokumente (Tabs) mit Speichern, Öffnen, Versionen und Autosicherung. Die
//! Logik kennt keine Oberfläche; die Fenster dazu stehen in `tabs.rs`, `prompts.rs` und den Panels.

use crate::blend_dialog::BlendDialog;
use crate::boolean_dialog::BooleanDialog;
use crate::create_dialog::CreateDialog;
use crate::drawing_dialog::NewDrawingDialog;
use crate::export_dialog::ExportDialog;
use crate::feature_dialog::FeatureDialog;
use crate::history::{History, Tracker, snapshot_bytes};
use crate::import_dialog::ImportDialog;
use crate::parameter_dialog::ParameterDialog;
use crate::places::{FavoriteChange, Places};
use crate::state::{AppState, REDO_FAILED, UNDO_FAILED};
use crate::step_import_dialog::StepImportDialog;
use crate::undo_labels::describe_change;
use planora_feature::Snapshot;
use planora_project::config::{Config, clamp_panel_transparency};
use planora_project::docstore::{Autosave, DocStore, DocumentInfo, LockMode, peek, read_version};
use planora_project::fsutil::validate_name;
use planora_project::library::{DOCUMENT_SUFFIX, Library, NodePath};
use planora_project::{ProjectError, Result as ProjectResult};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Abstand der Autosicherungen geänderter Dokumente.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(120);

/// Meldung, wenn noch kein Projektverzeichnis gewählt ist.
const NO_ROOT: &str = "Es ist noch kein Projektverzeichnis gewählt.";

/// Ablehnung eines Dokumentnamens auf „.pcad“ (wie bei Ordnern; die Endung hängt das Programm
/// selbst an).
pub const DOCUMENT_NAME_SUFFIX: &str = "Dokumentnamen dürfen nicht auf „.pcad“ enden.";

/// Geprüfter Dokumentname (wie `validate_name`, dazu ohne „.pcad“ am Ende, gleich welche
/// Schreibweise).
fn document_name(name: &str) -> Result<String, String> {
    let name = validate_name(name).map_err(err)?;
    if name.to_lowercase().ends_with(DOCUMENT_SUFFIX) {
        return Err(DOCUMENT_NAME_SUFFIX.into());
    }
    Ok(name)
}

/// Meldung von [`Workspace::choose_root`], solange gespeicherte Dokumente offen sind.
pub const ROOT_LOCKED: &str =
    "Bitte zuerst alle gespeicherten Dokumente schließen, bevor der Projektordner wechselt.";

/// Grund, warum Rückgängig nicht geht: kein Schritt da.
pub const NOTHING_TO_UNDO: &str = "Es gibt nichts rückgängig zu machen.";
/// Grund, warum Wiederholen nicht geht: kein Schritt da.
pub const NOTHING_TO_REDO: &str = "Es gibt nichts zu wiederholen.";
/// Grund, warum Rückgängig und Wiederholen nicht gehen: das Export-Fenster schreibt eine Datei.
pub const EXPORT_WRITING: &str = "Während des Exports ist das nicht möglich.";

pub struct DocTab {
    pub id: u64,
    pub state: AppState,
    pub dialog: Option<CreateDialog>,
    pub boolean_dialog: Option<BooleanDialog>,
    pub feature_dialog: Option<FeatureDialog>,
    pub blend_dialog: Option<BlendDialog>,
    pub export_dialog: Option<ExportDialog>,
    pub import_dialog: Option<ImportDialog>,
    /// Fenster „STEP importieren“.
    pub step_import_dialog: Option<StepImportDialog>,
    /// Fenster „Neue Zeichnung“.
    pub drawing_dialog: Option<NewDrawingDialog>,
    /// Fenster „Parameter“. Es sperrt Rückgängig nicht (Plan P8): nicht in
    /// [`DocTab::dialog_open`], nicht in [`DocTab::close_dialogs`]; nach jedem Sprung lädt es neu.
    pub parameter_dialog: Option<ParameterDialog>,
    /// Anzeigename ohne `.pcad`, bzw. „Unbenannt n“.
    pub name: String,
    pub path: Option<NodePath>,
    pub store: Option<DocStore>,
    /// Dokumentordner einer schreibgeschützten Ansicht (ohne Sperre).
    pub source_dir: Option<PathBuf>,
    /// Version, auf der der Stand beruht.
    pub based_on: Option<u32>,
    pub read_only: bool,
    /// Angebotene, noch nicht beantwortete Autosicherung. Solange sie offen ist, wird die
    /// Datei weder überschrieben noch gelöscht (sie ist nach einem Absturz die einzige Kopie).
    pub pending_autosave: Option<Autosave>,
    /// Eine vorhandene Autosicherung ließ sich nicht lesen (z. B. von einer neueren
    /// Programmversion). Sie bleibt unangetastet: keine neue Autosicherung, kein Löschen.
    keep_autosave: bool,
    /// Verlauf für Rückgängig und Wiederholen (Bilder des Dokuments, R3, R7). Die Nummer des
    /// gespeicherten Stands ersetzt den früheren Zählervergleich für „ungespeichert“.
    history: History<Snapshot>,
    /// Erkennt Änderungen am Dokument je Bild und fasst Gesten zusammen (R4).
    tracker: Tracker,
    autosaved_edits: u64,
    /// Seit dem letzten Speichern hat dieser Tab eine Autosicherung geschrieben. Ist der Tab
    /// danach wieder sauber (Rückgängig bis zum gespeicherten Stand), ist sie überholt und wird
    /// gelöscht ([`DocTab::drop_stale_autosave`]).
    autosave_written: bool,
    last_autosave: Instant,
}

impl DocTab {
    fn new(id: u64, name: String, state: AppState) -> DocTab {
        let edits = state.document.edit_count();
        let (history, tracker) = fresh_history(&state);
        DocTab {
            id,
            state,
            dialog: None,
            boolean_dialog: None,
            feature_dialog: None,
            blend_dialog: None,
            export_dialog: None,
            import_dialog: None,
            step_import_dialog: None,
            drawing_dialog: None,
            parameter_dialog: None,
            name,
            path: None,
            store: None,
            source_dir: None,
            based_on: None,
            read_only: false,
            pending_autosave: None,
            keep_autosave: false,
            history,
            tracker,
            autosaved_edits: edits,
            autosave_written: false,
            last_autosave: Instant::now(),
        }
    }

    /// Ungespeicherte Änderungen (R7): eine Skizze ist offen, eine Änderung ist noch kein
    /// Schritt (auch vor dem ersten Bild danach und während einer Geste), oder der aktuelle
    /// Schritt ist nicht der gespeicherte. Schreibgeschützte Ansichten sind nie „geändert“.
    pub fn is_dirty(&self) -> bool {
        !self.read_only
            && (self.state.sketch_session.is_some()
                || self.tracker.is_pending(self.state.document.edit_count())
                || !self.history.is_saved())
    }

    pub fn title(&self) -> String {
        match (self.read_only, self.based_on) {
            (true, Some(n)) => format!("{} · v{n}", self.name),
            _ => self.name.clone(),
        }
    }

    /// Versionsbaum zum Anzeigen: aus dem Speicher des Tabs, sonst (Ansicht) frisch gelesen.
    pub fn version_info(&self) -> Option<DocumentInfo> {
        match (&self.store, &self.source_dir) {
            (Some(s), _) => Some(s.info().clone()),
            (None, Some(dir)) => peek(dir).ok().map(|(i, _)| i),
            _ => None,
        }
    }

    /// Darf die Autosicherungsdatei dieses Tabs geschrieben oder gelöscht werden?
    fn owns_autosave(&self) -> bool {
        self.pending_autosave.is_none() && !self.keep_autosave
    }

    /// Der Stand ist gespeichert: offene Änderungen werden zuerst ein Schritt, dann merkt sich
    /// der Verlauf diesen Schritt als gespeichert (R7). Der Verlauf bleibt erhalten.
    fn mark_clean(&mut self) {
        self.observe_history(false);
        self.history.mark_saved();
        self.autosaved_edits = self.state.document.edit_count();
        self.autosave_written = false;
    }

    /// Löscht eine selbst geschriebene Autosicherung, wenn der Tab wieder sauber ist (etwa nach
    /// Rückgängig bis zum gespeicherten Stand): Sie hielte einen zurückgenommenen Stand fest und
    /// würde beim nächsten Öffnen angeboten. Der saubere Stand liegt als Version auf der Platte.
    /// Fremde oder unlesbare Sicherungen (Angebot offen, `keep_autosave`) bleiben.
    fn drop_stale_autosave(&mut self) {
        if !self.autosave_written || self.read_only || !self.owns_autosave() || self.is_dirty() {
            return;
        }
        if let Some(store) = &self.store {
            store.discard_autosave();
        }
        self.autosave_written = false;
        self.autosaved_edits = self.state.document.edit_count();
    }

    // ---- Rückgängig und Wiederholen --------------------------------------------------

    /// Am Ende eines Bildes (R3, R4): Hat sich das Dokument geändert und läuft keine Geste
    /// (Maustaste gedrückt, Eingabefeld aktiv) mehr, wird die Änderung ein Schritt – außer das
    /// neue Bild gleicht dem letzten (etwa „angelegt und wieder entfernt“). Ob sich etwas
    /// geändert hat, entscheidet zuerst der Änderungszähler (in beide Richtungen, ohne Verlass
    /// auf Steigen); nur dann entsteht ein Bild. So bildet auch ein Dokument, dessen Bild nie
    /// sich selbst gleicht (eine Zahl „keine Zahl“), ohne Änderung keine Schritte. Danach die
    /// offene Skizze in ihrem eigenen Verlauf (R10, [`AppState::observe_sketch`]); das
    /// Dokument selbst ändert sich während der Skizze kaum (Anlegen im selben Bild, Löschen).
    ///
    /// Der Hinweis `state.info` („Rückgängig: …“) endet hier, sobald ein neuer Schritt entsteht
    /// (im Dokument oder in der Skizze) oder eine Meldung in `state.message` steht.
    pub fn observe_history(&mut self, gesture: bool) {
        let steps = |t: &DocTab| {
            (
                t.history.current_id(),
                t.state.sketch_history().map(|h| h.current_id()),
            )
        };
        let before = steps(self);
        self.observe_document(gesture);
        self.state.observe_sketch(gesture);
        let new_step = steps(self) != before;
        // Ein Hinweis zur gerade entstehenden Änderung überlebt genau ihren Schritt.
        let kept = new_step && std::mem::take(&mut self.state.info_outlives_step);
        if (new_step && !kept) || self.state.message.is_some() {
            self.state.info = None;
            self.state.info_outlives_step = false;
        }
    }

    fn observe_document(&mut self, gesture: bool) {
        if !self
            .tracker
            .observe(self.state.document.edit_count(), gesture)
        {
            return;
        }
        let after = self.state.snapshot();
        if after.same_content(self.history.current()) {
            return;
        }
        let label = describe_change(self.history.current(), &after);
        let bytes = snapshot_bytes(&after);
        self.history.record(after, bytes, label);
    }

    /// Eine Änderung, die noch kein Schritt ist und sich vom letzten Schritt unterscheidet (sie
    /// würde beim nächsten Beobachten einer): das Bild dazu.
    fn pending_change(&self) -> Option<Snapshot> {
        if !self.tracker.is_pending(self.state.document.edit_count()) {
            return None;
        }
        let now = self.state.snapshot();
        (!now.same_content(self.history.current())).then_some(now)
    }

    /// Gemeinsame Sperren von Rückgängig und Wiederholen: alte Version, Export schreibt.
    fn history_locked(&self) -> Option<&'static str> {
        if self.read_only {
            Some(crate::drawing_browser::READ_ONLY_HINT)
        } else if self
            .export_dialog
            .as_ref()
            .is_some_and(|d| d.is_writing_file())
        {
            Some(EXPORT_WRITING)
        } else {
            None
        }
    }

    /// Ist eine Skizze offen? Dann wirken Rückgängig und Wiederholen nur auf ihren Verlauf
    /// (R10), der Dokumentverlauf ruht.
    fn in_sketch(&self) -> bool {
        self.state.sketch_session.is_some()
    }

    /// Warum Rückgängig gerade nicht geht (`None`: es geht).
    pub fn undo_blocked(&self) -> Option<&'static str> {
        self.history_locked().or_else(|| {
            let nothing = if self.in_sketch() {
                self.state.sketch_undo_label().is_none()
            } else {
                self.history.undo_len() == 0 && self.pending_change().is_none()
            };
            nothing.then_some(NOTHING_TO_UNDO)
        })
    }

    /// Warum Wiederholen gerade nicht geht (`None`: es geht). Eine noch offene Änderung leert
    /// beim Übernehmen die Wiederholen-Liste; dann gibt es nichts zu wiederholen.
    pub fn redo_blocked(&self) -> Option<&'static str> {
        self.history_locked().or_else(|| {
            let nothing = if self.in_sketch() {
                self.state.sketch_redo_label().is_none()
            } else {
                self.history.redo_len() == 0 || self.pending_change().is_some()
            };
            nothing.then_some(NOTHING_TO_REDO)
        })
    }

    /// Name des Schritts, den Rückgängig zurücknähme (eine noch offene Änderung zählt mit; bei
    /// offener Skizze der Schritt der Skizze).
    pub fn undo_label(&self) -> Option<&'static str> {
        if self.in_sketch() {
            return self.state.sketch_undo_label();
        }
        match self.pending_change() {
            Some(now) => Some(describe_change(self.history.current(), &now)),
            None => self.history.undo_label(),
        }
    }

    /// Name des Schritts, den Wiederholen herstellte (bei offener Skizze der Schritt der
    /// Skizze).
    pub fn redo_label(&self) -> Option<&'static str> {
        if self.in_sketch() {
            return self.state.sketch_redo_label();
        }
        if self.pending_change().is_some() {
            return None;
        }
        self.history.redo_label()
    }

    /// Nimmt den letzten Schritt zurück. Offene Änderungen werden vorher ein Schritt. Gelingt
    /// das Zurückladen, schließen die Fenster des Tabs (sie gehören zum alten Stand); schlägt es
    /// fehl, bleibt das Dokument, nur dieser Schritt wird verworfen und `state.message` sagt
    /// [`UNDO_FAILED`] (Spec §6a.8). Bei offener Skizze nur deren Verlauf (R10); das Dokument
    /// bleibt dann unberührt, auch wenn die Skizze nichts zurückzunehmen hat.
    pub fn undo(&mut self) -> Result<(), String> {
        if let Some(why) = self.history_locked() {
            return Err(why.into());
        }
        self.observe_history(false);
        if self.in_sketch() {
            return if self.state.undo_sketch() {
                Ok(())
            } else {
                Err(NOTHING_TO_UNDO.into())
            };
        }
        let Some(target) = self.history.peek_undo().cloned() else {
            return Err(NOTHING_TO_UNDO.into());
        };
        match self.state.restore_document(target) {
            Ok(_) => {
                self.history.commit_undo();
                self.after_jump();
                Ok(())
            }
            Err(e) => {
                self.history.discard_undo();
                self.state.message = Some(UNDO_FAILED.into());
                Err(e)
            }
        }
    }

    /// Stellt den zuletzt zurückgenommenen Schritt wieder her (Gegenstück zu [`DocTab::undo`];
    /// Meldung bei Fehler [`REDO_FAILED`]; bei offener Skizze nur deren Verlauf).
    pub fn redo(&mut self) -> Result<(), String> {
        if let Some(why) = self.history_locked() {
            return Err(why.into());
        }
        self.observe_history(false);
        if self.in_sketch() {
            return if self.state.redo_sketch() {
                Ok(())
            } else {
                Err(NOTHING_TO_REDO.into())
            };
        }
        let Some(target) = self.history.peek_redo().cloned() else {
            return Err(NOTHING_TO_REDO.into());
        };
        match self.state.restore_document(target) {
            Ok(_) => {
                self.history.commit_redo();
                self.after_jump();
                Ok(())
            }
            Err(e) => {
                self.history.discard_redo();
                self.state.message = Some(REDO_FAILED.into());
                Err(e)
            }
        }
    }

    /// Nach gelungenem Zurückladen: der neue Zählerstand ist kein Schritt; die Fenster des Tabs
    /// halten Kennungen des alten Stands und schließen.
    fn after_jump(&mut self) {
        self.tracker.reset(self.state.document.edit_count());
        self.close_dialogs();
        self.reload_parameter_dialog();
    }

    /// Das Fenster „Parameter“ liest seine Zeilen neu (Entwürfe und Kennungen des alten Stands
    /// entfallen); es bleibt offen.
    fn reload_parameter_dialog(&mut self) {
        if let Some(d) = &mut self.parameter_dialog {
            d.reload(&self.state);
        }
    }

    /// Nach einer Änderung im Fenster „Parameter“: Die Änderung wird sofort ein eigener Schritt
    /// (auch wenn die Maustaste schon für ein Ziehen gedrückt ist, das sonst mit ihr zu einem
    /// Schritt verschmölze). Ein offenes Bearbeiten-Fenster schließt mit Hinweis: Seine Felder
    /// zeigten alte Werte und Formeln, „Übernehmen“ würde Formeln still verwerfen.
    pub fn after_parameter_change(&mut self) {
        self.observe_history(false);
        if self.feature_dialog.take().is_some() {
            self.state.info = Some(crate::parameter_dialog::DIALOG_CLOSED.into());
        }
    }

    /// Ist ein Fenster oder eine Rückfrage dieses Tabs offen (sperrt Strg+Z, R8)?
    pub fn dialog_open(&self) -> bool {
        let s = &self.state;
        self.dialog.is_some()
            || self.boolean_dialog.is_some()
            || self.feature_dialog.is_some()
            || self.blend_dialog.is_some()
            || self.export_dialog.is_some()
            || self.import_dialog.is_some()
            || self.step_import_dialog.is_some()
            || self.drawing_dialog.is_some()
            || s.dimension_dialog.is_some()
            || s.sheet_dialog.is_some()
            || s.pending_delete.is_some()
            || s.pending_delete_drawing.is_some()
            || s.pending_delete_dimension.is_some()
            || s.rename_draft.is_some()
            || s.rename_drawing_draft.is_some()
    }

    /// Schließt alle Fenster und Rückfragen des Tabs, außer einem Export-Fenster, das gerade
    /// eine Datei schreibt (es darf nicht ohne Meldung enden).
    pub fn close_dialogs(&mut self) {
        self.dialog = None;
        self.boolean_dialog = None;
        self.feature_dialog = None;
        self.blend_dialog = None;
        if !self
            .export_dialog
            .as_ref()
            .is_some_and(|d| d.is_writing_file())
        {
            self.export_dialog = None;
        }
        self.import_dialog = None;
        self.step_import_dialog = None;
        self.drawing_dialog = None;
        let s = &mut self.state;
        s.dimension_dialog = None;
        s.sheet_dialog = None;
        s.pending_delete = None;
        s.pending_delete_drawing = None;
        s.pending_delete_dimension = None;
        s.rename_draft = None;
        s.rename_drawing_draft = None;
    }

    /// Ersetzt den ganzen Zustand des Tabs (Autosicherung übernommen, Version wiederhergestellt):
    /// ein neuer Verlauf beginnt, die Fenster des alten Stands schließen.
    pub fn replace_state(&mut self, state: AppState) {
        self.state = state;
        (self.history, self.tracker) = fresh_history(&self.state);
        self.close_dialogs();
        self.reload_parameter_dialog();
    }

    /// Verlauf des Tabs (lesen).
    pub fn history(&self) -> &History<Snapshot> {
        &self.history
    }

    /// Verlauf des Tabs zum Verändern (nur für Tests).
    #[doc(hidden)]
    pub fn history_mut(&mut self) -> &mut History<Snapshot> {
        &mut self.history
    }
}

/// Neuer Verlauf und Beobachter für `state`; der Anfangsstand gilt als gespeichert.
fn fresh_history(state: &AppState) -> (History<Snapshot>, Tracker) {
    let snapshot = state.snapshot();
    let bytes = snapshot_bytes(&snapshot);
    (
        History::new(snapshot, bytes),
        Tracker::new(state.document.edit_count()),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    CloseTab,
    Quit,
}

#[derive(Clone, Debug)]
pub enum Prompt {
    SaveChanges {
        tab: u64,
        then: After,
    },
    /// `branch = Some` → ein Zweigname ist Pflicht.
    SaveComment {
        tab: u64,
        then: Option<After>,
        comment: String,
        branch: Option<String>,
    },
    SaveAs {
        tab: u64,
        then: Option<After>,
        parent: NodePath,
        name: String,
        comment: String,
    },
    TakeOver {
        path: NodePath,
        version: Option<u32>,
    },
    AutosaveOffer {
        tab: u64,
    },
    Notice(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved(u32),
    NeedsLocation,
    NeedsBranch,
}

pub struct OpenResult {
    pub tab: usize,
    pub notes: Vec<String>,
    pub autosave_offered: bool,
}

pub struct Workspace {
    pub tabs: Vec<DocTab>,
    pub active: usize,
    pub library: Option<Library>,
    pub config: Config,
    /// Ort der Konfigurationsdatei (Vorgabe [`Config::config_path`]; Tests setzen einen
    /// Temp-Ordner, damit nie die echte Datei geschrieben wird).
    pub config_path: PathBuf,
    /// Favoriten-Änderungen, die noch nicht gespeichert werden konnten (Datei unlesbar,
    /// schreibgeschützt, gesperrt). Sie stehen schon in `config.favorites` und werden beim
    /// nächsten gelungenen Schreiben mit angewandt.
    pub favorites_pending: Vec<FavoriteChange>,
    /// Meldung aus dem letzten Speichern der Favoriten (z. B. Liste auf der Platte voll); geht
    /// über [`Workspace::places`] an die nächste gezeichnete Dateiauswahl.
    pub favorites_notice: Option<&'static str>,
    /// Die Durchsichtigkeit in `config.panel_transparency` konnte noch nicht gespeichert werden;
    /// sie wird beim nächsten gelungenen Schreiben ([`Workspace::try_persist_config`]) mit
    /// geschrieben.
    pub panel_transparency_pending: bool,
    pub prompt: Option<Prompt>,
    pub quit: bool,
    /// Der Dialog „Über PlanoraCAD“ ist offen.
    pub about_open: bool,
    /// Zählt Änderungen an der Bibliothek auf der Festplatte, die über den Arbeitsbereich laufen
    /// (Speichern unter, Wurzelwechsel); das Fenster Projekte liest danach den Baum neu.
    pub library_revision: u64,
    /// Zählt neue Stände von Dokumenten über den Arbeitsbereich (jedes gelungene Speichern,
    /// Speichern unter, neues Dokument, Wiederherstellen, Version oder Zweig löschen, Zweig
    /// wechseln). Das Fenster Projekte liest danach neu (Version, Datum, Miniatur), lässt aber
    /// – anders als bei `library_revision` – Ort, Auswahl und offene Fenster der Aktionen stehen.
    pub content_revision: u64,
    pub autosave_interval: Duration,
    /// Zuletzt benutzter Ordner der Dateiauswahl beim Exportieren und Importieren (nicht
    /// gespeichert).
    pub exchange_dir: Option<PathBuf>,
    next_tab_id: u64,
    untitled: usize,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Anzeigename eines Dokuments aus seinem Pfad (letzter Teil ohne `.pcad`).
fn display_name(path: &NodePath) -> String {
    path.last()
        .map(|p| p.strip_suffix(".pcad").unwrap_or(p).to_string())
        .unwrap_or_default()
}

impl Workspace {
    pub fn new(library: Option<Library>, config: Config) -> Workspace {
        let mut w = Workspace {
            tabs: Vec::new(),
            active: 0,
            library,
            config,
            config_path: Config::config_path(),
            favorites_pending: Vec::new(),
            favorites_notice: None,
            panel_transparency_pending: false,
            prompt: None,
            quit: false,
            about_open: false,
            library_revision: 0,
            content_revision: 0,
            autosave_interval: AUTOSAVE_INTERVAL,
            exchange_dir: None,
            next_tab_id: 1,
            untitled: 0,
        };
        w.new_tab();
        w
    }

    fn tab_id(&mut self) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        id
    }

    pub fn new_tab(&mut self) -> usize {
        self.untitled += 1;
        let id = self.tab_id();
        let dark = self
            .tabs
            .get(self.active)
            .map(|t| t.state.dark)
            .unwrap_or(true);
        let mut state = AppState::new();
        state.dark = dark;
        state.rebuild_scene();
        state.frame_all();
        // Die frische Kamera kennt das freie Rechteck noch nicht.
        state.fit_pending = true;
        self.tabs.push(DocTab::new(
            id,
            format!("Unbenannt {}", self.untitled),
            state,
        ));
        self.active = self.tabs.len() - 1;
        self.active
    }

    pub fn activate(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        }
    }

    fn active_index(&self) -> usize {
        self.active.min(self.tabs.len() - 1)
    }

    pub fn active_tab(&self) -> &DocTab {
        &self.tabs[self.active_index()]
    }

    pub fn active_tab_mut(&mut self) -> &mut DocTab {
        let i = self.active_index();
        &mut self.tabs[i]
    }

    pub fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Beschreibbarer Tab des Dokuments `path`.
    pub fn tab_for_path(&self, path: &NodePath) -> Option<usize> {
        self.tabs
            .iter()
            .position(|t| !t.read_only && t.path.as_ref() == Some(path))
    }

    pub fn set_dark(&mut self, dark: bool) {
        for t in &mut self.tabs {
            t.state.dark = dark;
            t.state.rebuild_scene();
        }
    }

    /// Schließt einen Tab ohne Rückfrage; nach dem letzten entsteht ein frischer leerer Tab.
    /// Eine vorhandene `.autosave.json` des Dokuments bleibt liegen (auch beim Verwerfen) und
    /// wird beim nächsten Öffnen wieder angeboten – außer der Tab ist sauber und hat sie selbst
    /// geschrieben (dann ist sie überholt und wird gelöscht).
    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs[i].drop_stale_autosave();
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.new_tab();
        } else if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
    }

    /// Hebt ein offenes Autosicherungs-Angebot wieder als Prompt hervor, wenn gerade keiner
    /// steht. Die Oberfläche ruft das in jedem Bild vor dem Zeichnen der Prompts auf; so geht
    /// kein Angebot verloren, auch wenn mehrere Dokumente mit Autosicherung geöffnet wurden.
    pub fn refresh_prompts(&mut self) {
        if self.prompt.is_none()
            && let Some(t) = self.tabs.iter().find(|t| t.pending_autosave.is_some())
        {
            self.prompt = Some(Prompt::AutosaveOffer { tab: t.id });
        }
    }

    /// Steht (nach `refresh_prompts`) ein Autosicherungs-Angebot an? Dann muss es zuerst
    /// beantwortet werden; Schließen und Beenden warten darauf.
    fn autosave_offer_pending(&mut self) -> bool {
        self.refresh_prompts();
        matches!(self.prompt, Some(Prompt::AutosaveOffer { .. }))
    }

    /// `true` = geschlossen, `false` = Rückfrage (steht in `prompt`, auch ein noch offenes
    /// Autosicherungs-Angebot, das zuerst beantwortet werden muss).
    pub fn request_close(&mut self, i: usize) -> bool {
        if i >= self.tabs.len() {
            return true;
        }
        if self.autosave_offer_pending() {
            return false;
        }
        let t = &self.tabs[i];
        if t.is_dirty() {
            self.prompt = Some(Prompt::SaveChanges {
                tab: t.id,
                then: After::CloseTab,
            });
            false
        } else {
            self.close_tab(i);
            true
        }
    }

    /// `true` = darf beenden (setzt `quit`); sonst fragt `prompt` nach dem ersten geänderten Tab
    /// (oder zeigt ein noch offenes Autosicherungs-Angebot).
    pub fn request_quit(&mut self) -> bool {
        if self.autosave_offer_pending() {
            return false;
        }
        if let Some(t) = self.tabs.iter().find(|t| t.is_dirty()) {
            self.prompt = Some(Prompt::SaveChanges {
                tab: t.id,
                then: After::Quit,
            });
            false
        } else {
            self.quit = true;
            true
        }
    }

    /// „Verwerfen“ im Prompt `SaveChanges`.
    pub fn discard_and_continue(&mut self) {
        if let Some(Prompt::SaveChanges { tab, then }) = self.prompt.take() {
            self.finish_after(tab, Some(then));
        }
    }

    /// Schließt `tab` (Verwerfen oder nach dem Speichern) und macht beim Beenden mit dem
    /// nächsten geänderten Tab weiter.
    pub fn finish_after(&mut self, tab: u64, then: Option<After>) {
        let Some(then) = then else { return };
        if let Some(i) = self.tab_index(tab) {
            self.close_tab(i);
        }
        if then == After::Quit {
            self.request_quit();
        }
    }

    // ---- Speichern -------------------------------------------------------------------

    fn save_tab(
        tab: &mut DocTab,
        comment: &str,
        new_branch: Option<&str>,
    ) -> Result<SaveOutcome, String> {
        if tab.read_only {
            return Err(
                "Diese Version ist schreibgeschützt. Zuerst abzweigen oder wiederherstellen."
                    .into(),
            );
        }
        tab.state.try_close_sketch()?;
        let snapshot = tab.state.snapshot();
        let Some(store) = tab.store.as_mut() else {
            return Ok(SaveOutcome::NeedsLocation);
        };
        match store.save(tab.based_on, &snapshot, comment, new_branch) {
            Ok(n) => {
                if tab.pending_autosave.is_none() && !tab.keep_autosave {
                    // nicht über owns_autosave(): `store` leiht `tab.store` gerade
                    store.discard_autosave();
                }
                tab.based_on = Some(n);
                tab.mark_clean();
                Ok(SaveOutcome::Saved(n))
            }
            Err(ProjectError::NeedsBranch) => Ok(SaveOutcome::NeedsBranch),
            Err(e) => Err(err(e)),
        }
    }

    pub fn save_active(
        &mut self,
        comment: &str,
        new_branch: Option<&str>,
    ) -> Result<SaveOutcome, String> {
        let i = self.active_index();
        let r = Self::save_tab(&mut self.tabs[i], comment, new_branch);
        if matches!(r, Ok(SaveOutcome::Saved(_))) {
            self.content_revision += 1;
        }
        r
    }

    pub fn save_as(
        &mut self,
        parent: &NodePath,
        name: &str,
        comment: &str,
    ) -> Result<NodePath, String> {
        let lib = self.library.as_ref().ok_or(NO_ROOT)?;
        if !lib.is_container(parent) {
            return Err("Bitte ein Projekt oder einen Ordner als Ziel wählen.".into());
        }
        let name = document_name(name)?;
        let mut path = parent.clone();
        path.push(format!("{name}.pcad"));
        let dir = lib.root().resolve(&path).map_err(err)?;
        // vor dem Schließen einer offenen Skizze prüfen, damit ein vergebener Name den Tab
        // nicht verändert (DocStore::create prüft das noch einmal, gegen Wettläufe)
        if dir.exists() {
            return Err(err(ProjectError::Exists(dir.display().to_string())));
        }
        let i = self.active_index();
        let tab = &mut self.tabs[i];
        tab.state.try_close_sketch()?;
        let store = DocStore::create(&dir, &tab.state.snapshot(), comment).map_err(err)?;
        tab.store = Some(store);
        tab.path = Some(path.clone());
        tab.name = name;
        tab.based_on = Some(1);
        tab.read_only = false;
        tab.source_dir = None;
        // ein Angebot des bisherigen Dokuments gilt nicht für das neue; seine Datei bleibt dort
        tab.pending_autosave = None;
        tab.keep_autosave = false;
        tab.mark_clean();
        self.library_revision += 1;
        self.content_revision += 1;
        Ok(path)
    }

    /// Neues leeres Dokument (Version 1 aus `AppState::new().snapshot()`, Kommentar "") in einem
    /// Projekt oder Ordner; dieselben Prüfungen und Meldungen wie `save_as`; `library_revision += 1`.
    /// Die offenen Tabs bleiben unberührt (Öffnen ist Sache des Aufrufers).
    pub fn create_document(&mut self, parent: &NodePath, name: &str) -> Result<NodePath, String> {
        let lib = self.library.as_ref().ok_or(NO_ROOT)?;
        if !lib.is_container(parent) {
            return Err("Bitte ein Projekt oder einen Ordner als Ziel wählen.".into());
        }
        let name = document_name(name)?;
        let mut path = parent.clone();
        path.push(format!("{name}.pcad"));
        let dir = lib.root().resolve(&path).map_err(err)?;
        if dir.exists() {
            return Err(err(ProjectError::Exists(dir.display().to_string())));
        }
        // derselbe Stand wie beim ersten Speichern eines neuen Tabs; die Sperre endet mit `store`
        let store = DocStore::create(&dir, &AppState::new().snapshot(), "").map_err(err)?;
        drop(store);
        self.library_revision += 1;
        self.content_revision += 1;
        Ok(path)
    }

    /// „Speichern“ im Prompt `SaveChanges`: speichert den Tab und macht weiter; fehlen Ort oder
    /// Zweigname, wechselt der Prompt in das passende Fenster.
    pub fn save_for_prompt(&mut self) {
        let Some(Prompt::SaveChanges { tab, then }) = self.prompt.take() else {
            return;
        };
        let Some(i) = self.tab_index(tab) else { return };
        self.active = i;
        match Self::save_tab(&mut self.tabs[i], "", None) {
            Ok(SaveOutcome::Saved(_)) => {
                self.content_revision += 1;
                self.finish_after(tab, Some(then))
            }
            Ok(SaveOutcome::NeedsLocation) => {
                self.prompt = Some(Prompt::SaveAs {
                    tab,
                    then: Some(then),
                    parent: Vec::new(),
                    name: self.tabs[i].name.clone(),
                    comment: String::new(),
                });
            }
            Ok(SaveOutcome::NeedsBranch) => {
                self.prompt = Some(Prompt::SaveComment {
                    tab,
                    then: Some(then),
                    comment: String::new(),
                    branch: Some(String::new()),
                });
            }
            Err(e) => self.prompt = Some(Prompt::Notice(e)),
        }
    }

    // ---- Öffnen ----------------------------------------------------------------------

    fn replace_empty_start_tab(&self) -> bool {
        self.tabs.len() == 1
            && self.tabs[0].store.is_none()
            && !self.tabs[0].read_only
            && !self.tabs[0].is_dirty()
            && self.tabs[0].state.document.features().is_empty()
    }

    fn push_tab(&mut self, tab: DocTab) -> usize {
        if self.replace_empty_start_tab() {
            self.tabs[0] = tab;
            self.active = 0;
        } else {
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
        }
        self.active
    }

    pub fn open_document(&mut self, path: &NodePath, mode: LockMode) -> ProjectResult<OpenResult> {
        if let Some(i) = self.tab_for_path(path) {
            self.active = i;
            return Ok(OpenResult {
                tab: i,
                notes: Vec::new(),
                autosave_offered: false,
            });
        }
        let lib = self
            .library
            .as_ref()
            .ok_or_else(|| ProjectError::Invalid(NO_ROOT.into()))?;
        let dir = lib.document_dir(path)?;
        let (store, report) = DocStore::open(&dir, mode)?;
        let branch = store.info().active_branch.clone();
        let v = store.latest_readable(&branch).ok_or_else(|| {
            ProjectError::corrupt(dir.display().to_string(), "Es gibt keine lesbare Version.")
        })?;
        let snapshot = store.load_version(v)?;
        let state = AppState::from_snapshot(snapshot).map_err(ProjectError::Invalid)?;
        let mut notes = Vec::new();
        if !report.orphans.is_empty() {
            notes.push(format!(
                "Reste eines abgebrochenen Speicherns gefunden und ignoriert: {}.",
                report.orphans.join(", ")
            ));
        }
        if !report.missing.is_empty() {
            let list: Vec<String> = report.missing.iter().map(|n| format!("v{n}")).collect();
            notes.push(format!("Versionsdateien fehlen: {}.", list.join(", ")));
        }
        if store.info().tip(&branch) != Some(v) {
            notes.push(format!(
                "Die neueste Version ist nicht lesbar; v{v} wurde geladen."
            ));
        }
        // Eine Autosicherung existiert nur, wenn das letzte Speichern nicht abgeschlossen wurde;
        // „>=“, weil Zeitstempel sekundengenau sind.
        let mut keep_autosave = false;
        let autosave = match store.read_autosave() {
            Ok(Some(a)) if a.time.as_str() >= store.info().latest_time() => Some(a),
            Ok(_) => None,
            // Die Datei bleibt erhalten (nach einem Absturz womöglich die einzige Kopie, oder
            // von einer neueren Programmversion); sie wird weder angeboten noch überschrieben.
            Err(e) => {
                notes.push(format!(
                    "Die Autosicherung lässt sich nicht lesen und wird nicht verwendet; die Datei bleibt erhalten: {e}"
                ));
                keep_autosave = true;
                None
            }
        };
        let mut state = state;
        state.dark = self.active_tab().state.dark;
        state.rebuild_scene();
        let offered = autosave.is_some();
        let id = self.tab_id();
        let mut tab = DocTab::new(id, display_name(path), state);
        tab.path = Some(path.clone());
        tab.store = Some(store);
        tab.based_on = Some(v);
        tab.pending_autosave = autosave;
        tab.keep_autosave = keep_autosave;
        let i = self.push_tab(tab);
        // ein schon stehender Prompt (z. B. das Angebot eines anderen Dokuments) bleibt;
        // `refresh_prompts` bietet diese Sicherung danach an
        if offered && self.prompt.is_none() {
            self.prompt = Some(Prompt::AutosaveOffer { tab: id });
        }
        Ok(OpenResult {
            tab: i,
            notes,
            autosave_offered: offered,
        })
    }

    /// Nimmt das Angebot für `tab` aus dem Prompt und hebt das nächste offene hervor.
    fn close_autosave_offer(&mut self, tab: u64) {
        if matches!(self.prompt, Some(Prompt::AutosaveOffer { tab: t }) if t == tab) {
            self.prompt = None;
        }
    }

    /// Übernimmt die angebotene Autosicherung in den Tab (gilt danach als ungespeichert).
    pub fn apply_autosave(&mut self, tab: u64) {
        self.close_autosave_offer(tab);
        let Some(i) = self.tab_index(tab) else { return };
        let Some(a) = self.tabs[i].pending_autosave.take() else {
            return;
        };
        match AppState::from_snapshot(a.snapshot) {
            Ok(mut state) => {
                state.dark = self.tabs[i].state.dark;
                state.rebuild_scene();
                let t = &mut self.tabs[i];
                t.replace_state(state);
                t.based_on = a.based_on.or(t.based_on);
                // gilt als geändert gegenüber dem gespeicherten Stand, bis gespeichert wird
                t.history.forget_saved();
            }
            Err(e) => {
                // Datei unangetastet lassen
                self.tabs[i].keep_autosave = true;
                self.prompt = Some(Prompt::Notice(format!(
                    "Die Autosicherung lässt sich nicht laden; die Datei bleibt erhalten: {e}"
                )))
            }
        }
        self.refresh_prompts();
    }

    /// Verwirft die angebotene Autosicherung und löscht ihre Datei.
    pub fn discard_autosave(&mut self, tab: u64) {
        self.close_autosave_offer(tab);
        if let Some(i) = self.tab_index(tab) {
            let t = &mut self.tabs[i];
            t.pending_autosave = None;
            if let Some(s) = &t.store
                && !t.keep_autosave
            {
                s.discard_autosave();
            }
        }
        self.refresh_prompts();
    }

    /// Öffnet Version `n` schreibgeschützt (ein zweites Mal: nur dorthin springen).
    pub fn open_version(&mut self, path: &NodePath, n: u32) -> ProjectResult<usize> {
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.read_only && t.path.as_ref() == Some(path) && t.based_on == Some(n))
        {
            self.active = i;
            return Ok(i);
        }
        let lib = self
            .library
            .as_ref()
            .ok_or_else(|| ProjectError::Invalid(NO_ROOT.into()))?;
        let dir = lib.document_dir(path)?;
        let snapshot = match self
            .tab_for_path(path)
            .and_then(|i| self.tabs[i].store.as_ref())
        {
            Some(store) => store.load_version(n)?,
            None => read_version(&dir, n)?,
        };
        let mut state = AppState::from_snapshot(snapshot).map_err(ProjectError::Invalid)?;
        state.dark = self.active_tab().state.dark;
        state.rebuild_scene();
        let id = self.tab_id();
        let mut tab = DocTab::new(id, display_name(path), state);
        tab.path = Some(path.clone());
        tab.source_dir = Some(dir);
        tab.based_on = Some(n);
        tab.read_only = true;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        Ok(self.active)
    }

    /// Macht eine schreibgeschützte Ansicht bearbeitbar (Abzweigen): holt die Sperre. Gibt es
    /// das Dokument schon beschreibbar in einem anderen Tab, ist das ein Fehler.
    pub fn make_editable(&mut self, i: usize) -> Result<(), String> {
        let tab = self.tabs.get(i).ok_or("Kein solcher Tab.")?;
        if !tab.read_only {
            return Ok(());
        }
        let path = tab.path.clone().ok_or("Das Dokument hat keinen Ort.")?;
        if self.tab_for_path(&path).is_some() {
            return Err("Das Dokument ist schon in einem anderen Tab geöffnet. Dort weiterarbeiten oder den Tab schließen.".into());
        }
        let dir = tab
            .source_dir
            .clone()
            .ok_or("Das Dokument hat keinen Ort.")?;
        let (store, _) = DocStore::open(&dir, LockMode::Acquire).map_err(err)?;
        let tab = &mut self.tabs[i];
        tab.store = Some(store);
        tab.source_dir = None;
        tab.read_only = false;
        tab.mark_clean();
        Ok(())
    }

    /// Stellt Version `n` wieder her: neue Version am Zweig des beschreibbaren Tabs dieses
    /// Dokuments (bei einer Ansicht: im schon offenen beschreibbaren Tab, sonst wird die Ansicht
    /// bearbeitbar). Danach zeigt der beschreibbare Tab den Stand und ist aktiv. Ungespeicherte
    /// Änderungen im Zieltab verhindern das Wiederherstellen. Schlägt es fehl, nachdem die
    /// Ansicht dafür bearbeitbar gemacht wurde, wird sie wieder zur schreibgeschützten Ansicht
    /// (Sperre freigegeben).
    pub fn restore_version(&mut self, i: usize, n: u32) -> Result<(), String> {
        let read_only = self.tabs.get(i).ok_or("Kein solcher Tab.")?.read_only;
        let (target, made_editable) = if read_only {
            match self.tabs[i]
                .path
                .as_ref()
                .and_then(|p| self.tab_for_path(p))
            {
                Some(j) => (j, false),
                None => {
                    self.make_editable(i)?;
                    (i, true)
                }
            }
        } else {
            (i, false)
        };
        let result = self.restore_into(target, n);
        if result.is_err() && made_editable {
            let tab = &mut self.tabs[target];
            tab.source_dir = tab.store.take().map(|s| s.dir().to_path_buf());
            tab.read_only = true;
            tab.mark_clean();
        }
        result
    }

    fn restore_into(&mut self, target: usize, n: u32) -> Result<(), String> {
        let tab = &mut self.tabs[target];
        if tab.is_dirty() {
            return Err(
                "Es gibt ungespeicherte Änderungen. Zuerst speichern oder verwerfen.".into(),
            );
        }
        let store = tab
            .store
            .as_mut()
            .ok_or("Das Dokument ist nicht gespeichert.")?;
        let branch = tab
            .based_on
            .and_then(|b| store.info().version(b))
            .map(|v| v.branch.clone())
            .unwrap_or_else(|| store.info().active_branch.clone());
        let new = store.restore(n, &branch).map_err(err)?;
        let snapshot = store.load_version(new).map_err(err)?;
        let mut state = AppState::from_snapshot(snapshot)?;
        state.dark = tab.state.dark;
        state.rebuild_scene();
        tab.replace_state(state);
        tab.based_on = Some(new);
        tab.mark_clean();
        self.active = target;
        self.content_revision += 1;
        Ok(())
    }

    /// Löscht Version `n` über den beschreibbaren Tab `i`. Beruht der Tab auf dieser Version,
    /// beruht er danach auf der neuen Spitze ihres Zweigs (sonst auf der des aktiven Zweigs),
    /// damit Speichern weiter möglich ist. Offene Ansichten bleiben als Kopien bestehen.
    pub fn delete_version(&mut self, i: usize, n: u32) -> Result<(), String> {
        let tab = self.tabs.get_mut(i).ok_or("Kein solcher Tab.")?;
        let store = tab
            .store
            .as_mut()
            .ok_or("Das Dokument ist nicht gespeichert.")?;
        let branch = store.info().version(n).map(|v| v.branch.clone());
        store.delete_version(n).map_err(err)?;
        Self::fix_based_on(tab, branch.as_deref());
        self.content_revision += 1;
        Ok(())
    }

    /// Löscht den Zweig `name` über den beschreibbaren Tab `i` (siehe `delete_version`).
    pub fn delete_branch(&mut self, i: usize, name: &str) -> Result<(), String> {
        let tab = self.tabs.get_mut(i).ok_or("Kein solcher Tab.")?;
        let store = tab
            .store
            .as_mut()
            .ok_or("Das Dokument ist nicht gespeichert.")?;
        store.delete_branch(name).map_err(err)?;
        Self::fix_based_on(tab, Some(name));
        self.content_revision += 1;
        Ok(())
    }

    fn fix_based_on(tab: &mut DocTab, branch: Option<&str>) {
        let Some(store) = &tab.store else { return };
        let info = store.info();
        if tab.based_on.is_some_and(|b| info.version(b).is_none()) {
            let new = branch
                .and_then(|b| info.tip(b))
                .or_else(|| info.tip(&info.active_branch));
            tab.based_on = new;
        }
    }

    /// Wählt die Wurzel (Projektordner, wird bei Bedarf angelegt) und merkt sie in der
    /// Konfiguration (gespeichert wird mit `persist_config`).
    ///
    /// Solange gespeicherte Dokumente offen sind, wird die Wurzel nicht gewechselt: ihre Pfade
    /// gehören zur bisherigen Wurzel.
    pub fn choose_root(&mut self, root: &Path) -> Result<(), String> {
        if self.has_saved_documents() {
            return Err(ROOT_LOCKED.into());
        }
        let lib = Library::open(root).map_err(err)?;
        self.library = Some(lib);
        self.config.root = Some(root.to_path_buf());
        self.library_revision += 1;
        Ok(())
    }

    /// Ist ein Tab mit einem gespeicherten Dokument offen? Dann bleibt die Wurzel, wie sie ist.
    pub fn has_saved_documents(&self) -> bool {
        self.tabs.iter().any(|t| t.path.is_some())
    }

    /// Am Ende jedes Bildes: Änderungen aller Tabs werden Schritte ihres Verlaufs (R3, R4);
    /// `gesture` = Maustaste gedrückt oder Eingabefeld aktiv.
    pub fn observe_history(&mut self, gesture: bool) {
        for t in &mut self.tabs {
            t.observe_history(gesture);
        }
    }

    /// Speichert die gewählte Wurzel; Fehler sind nicht schlimm. Nur `root` wird in den Stand
    /// der Datei geschrieben, die Favoriten auf der Platte bleiben (eine andere Instanz kann
    /// sie inzwischen geändert haben) und werden übernommen. Ist die Datei unlesbar, wird sie
    /// wie bisher mit dem Stand im Speicher ersetzt: die Wurzel ist eine bewusste Wahl.
    ///
    /// Noch nicht gespeicherte Favoriten-Änderungen werden dabei mit geschrieben.
    pub fn persist_config(&mut self) {
        self.try_persist_config();
    }

    /// Wie [`Workspace::persist_config`]; `true`, wenn die Datei geschrieben wurde (auch über
    /// den Ersatz einer unlesbaren Datei).
    pub fn try_persist_config(&mut self) -> bool {
        let root = self.config.root.clone();
        let pending = std::mem::take(&mut self.favorites_pending);
        let transparency = self
            .panel_transparency_pending
            .then(|| self.panel_transparency());
        let mut full = None;
        let written = Config::update_at(&self.config_path, |c| {
            c.root = root;
            full = apply_changes(&pending, &mut c.favorites);
            if let Some(t) = transparency {
                c.panel_transparency = t;
            }
        });
        match written {
            Ok(disk) => {
                self.config.favorites = disk.favorites;
                if full.is_some() {
                    self.favorites_notice = full;
                }
                self.panel_transparency_pending = false;
                true
            }
            Err(_) => {
                let saved = self.config.save_to(&self.config_path).is_ok();
                if saved {
                    self.panel_transparency_pending = false;
                } else {
                    self.favorites_pending = pending;
                }
                saved
            }
        }
    }

    /// Durchsichtigkeit der Panels in Prozent (0 bis 80, Einstellungen → Darstellung).
    pub fn panel_transparency(&self) -> u8 {
        clamp_panel_transparency(self.config.panel_transparency)
    }

    /// Setzt die Durchsichtigkeit (auf 0 bis 80 begrenzt), sie wirkt sofort, und speichert sie
    /// gleich in [`Workspace::config_path`]; `true`, wenn geschrieben wurde. Nur dieses Feld
    /// wird in den Stand der Datei geschrieben (Wurzel und Favoriten bleiben). Lässt sich die
    /// Datei nicht lesen oder schreiben, bleibt sie unangetastet; der Wert gilt dann für die
    /// Sitzung und wird beim nächsten gelungenen Schreiben mit gespeichert.
    pub fn set_panel_transparency(&mut self, percent: u8) -> bool {
        let percent = clamp_panel_transparency(percent);
        self.config.panel_transparency = percent;
        let written = Config::update_at(&self.config_path, |c| {
            c.panel_transparency = percent;
        })
        .is_ok();
        self.panel_transparency_pending = !written;
        written
    }

    /// Gemeinsamer Stand der Schnellzugriffe für die Dateiauswahlen eines Bildes: Favoriten aus
    /// der Konfiguration, Projektordner = gewählte Wurzel oder ihre Vorgabe.
    pub fn places(&self) -> Places {
        Places {
            favorites: self.config.favorites.clone(),
            project_root: Some(
                self.config
                    .root
                    .clone()
                    .unwrap_or_else(Config::default_root),
            ),
            changes: Vec::new(),
            drives: None,
            notice: self.favorites_notice,
        }
    }

    /// Übernimmt geänderte Favoriten und speichert sie gleich (in [`Workspace::config_path`]);
    /// `true`, wenn geschrieben wurde. Gelesen, geändert, geschrieben: Die Änderungen werden
    /// auf den Stand der Datei angewandt, deren Wurzel und fremde Favoriten bleiben (eine
    /// andere Instanz oder eine beim Start kurz unlesbare Datei überschreibt so nichts).
    ///
    /// Lässt sich die Datei nicht lesen oder schreiben, bleibt sie unangetastet; die
    /// Änderungen bleiben im Speicher (in der Leiste zu sehen) und vorgemerkt
    /// ([`Workspace::favorites_pending`]) und werden beim nächsten gelungenen Schreiben mit
    /// angewandt, sonst gelten sie bis Programmende. Ist die Liste auf der Platte schon voll
    /// (eine andere Instanz), fehlt der neue Favorit danach und die Auswahl meldet
    /// [`crate::places::FAVORITES_FULL`].
    pub fn save_places(&mut self, places: Places) -> bool {
        // Die Meldung des letzten Speicherns hat eine Auswahl übernommen (Feld geleert).
        if places.notice.is_none() {
            self.favorites_notice = None;
        }
        if !places.changed() {
            return false;
        }
        let mut all = std::mem::take(&mut self.favorites_pending);
        all.extend(places.changes);
        let mut full = None;
        // Eine vorgemerkte Durchsichtigkeit kommt mit (wie in `try_persist_config`).
        let transparency = self
            .panel_transparency_pending
            .then(|| self.panel_transparency());
        let written = Config::update_at(&self.config_path, |c| {
            full = apply_changes(&all, &mut c.favorites);
            if let Some(t) = transparency {
                c.panel_transparency = t;
            }
        });
        match written {
            Ok(disk) => {
                self.config.favorites = disk.favorites;
                self.favorites_notice = full;
                self.panel_transparency_pending = false;
                true
            }
            Err(_) => {
                self.config.favorites = places.favorites;
                self.favorites_pending = all;
                false
            }
        }
    }

    /// Schreibt für geänderte, gespeicherte Dokumente eine Autosicherung, wenn das Intervall
    /// seit der letzten abgelaufen ist. Gesichert wird der Dokumentstand; der Entwurf einer
    /// gerade offenen Skizze ist nicht darin (er kommt erst beim Schließen der Skizze ins
    /// Dokument). Tabs mit unbeantwortetem Angebot oder unlesbarer Sicherung werden
    /// übersprungen, damit deren Datei nicht überschrieben wird. Auch ein fehlgeschlagener
    /// Versuch zählt als Versuch: der nächste kommt erst nach dem Intervall.
    pub fn autosave_tick(&mut self, now: Instant) {
        let interval = self.autosave_interval;
        for t in &mut self.tabs {
            t.drop_stale_autosave();
            if t.read_only
                || !t.owns_autosave()
                || !t.is_dirty()
                || t.state.document.edit_count() == t.autosaved_edits
            {
                continue;
            }
            if now.saturating_duration_since(t.last_autosave) < interval {
                continue;
            }
            let Some(store) = &t.store else { continue };
            if store
                .write_autosave(t.based_on, &t.state.snapshot())
                .is_ok()
            {
                t.autosaved_edits = t.state.document.edit_count();
                t.autosave_written = true;
            }
            t.last_autosave = now;
        }
    }
}

/// Wendet Favoriten-Änderungen der Reihe nach an; die erste Meldung (volle Liste) oder `None`.
fn apply_changes(changes: &[FavoriteChange], favorites: &mut Vec<PathBuf>) -> Option<&'static str> {
    let mut first = None;
    for ch in changes {
        if let Err(e) = ch.apply(favorites) {
            first.get_or_insert(e);
        }
    }
    first
}
