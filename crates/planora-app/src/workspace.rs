//! Mehrere offene Dokumente (Tabs) mit Speichern, Öffnen, Versionen und Autosicherung. Die
//! Logik kennt keine Oberfläche; die Fenster dazu stehen in `tabs.rs`, `prompts.rs` und den Panels.

use crate::boolean_dialog::BooleanDialog;
use crate::create_dialog::CreateDialog;
use crate::feature_dialog::FeatureDialog;
use crate::state::AppState;
use planora_project::config::Config;
use planora_project::docstore::{Autosave, DocStore, DocumentInfo, LockMode, peek, read_version};
use planora_project::fsutil::validate_name;
use planora_project::library::{Library, NodePath};
use planora_project::{ProjectError, Result as ProjectResult};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Abstand der Autosicherungen geänderter Dokumente.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(120);

/// Meldung, wenn noch keine Wurzel (Projektordner) gewählt ist.
const NO_ROOT: &str = "Es ist noch kein Projektordner gewählt.";

pub struct DocTab {
    pub id: u64,
    pub state: AppState,
    pub dialog: Option<CreateDialog>,
    pub boolean_dialog: Option<BooleanDialog>,
    pub feature_dialog: Option<FeatureDialog>,
    /// Anzeigename ohne `.pcad`, bzw. „Unbenannt n“.
    pub name: String,
    pub path: Option<NodePath>,
    pub store: Option<DocStore>,
    /// Dokumentordner einer schreibgeschützten Ansicht (ohne Sperre).
    pub source_dir: Option<PathBuf>,
    /// Version, auf der der Stand beruht.
    pub based_on: Option<u32>,
    pub read_only: bool,
    pub pending_autosave: Option<Autosave>,
    saved_edits: u64,
    autosaved_edits: u64,
    last_autosave: Instant,
}

impl DocTab {
    fn new(id: u64, name: String, state: AppState) -> DocTab {
        let edits = state.document.edit_count();
        DocTab {
            id,
            state,
            dialog: None,
            boolean_dialog: None,
            feature_dialog: None,
            name,
            path: None,
            store: None,
            source_dir: None,
            based_on: None,
            read_only: false,
            pending_autosave: None,
            saved_edits: edits,
            autosaved_edits: edits,
            last_autosave: Instant::now(),
        }
    }

    /// Ungespeicherte Änderungen: Zähler weicht ab oder eine Skizze ist offen. Schreibgeschützte
    /// Ansichten sind nie „geändert“.
    pub fn is_dirty(&self) -> bool {
        !self.read_only
            && (self.state.document.edit_count() != self.saved_edits
                || self.state.sketch_session.is_some())
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

    fn mark_clean(&mut self) {
        self.saved_edits = self.state.document.edit_count();
        self.autosaved_edits = self.saved_edits;
    }
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
    pub prompt: Option<Prompt>,
    pub quit: bool,
    pub autosave_interval: Duration,
    next_tab_id: u64,
    untitled: usize,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Anzeigename eines Dokuments aus seinem Pfad (letzter Teil ohne `.pcad`).
fn display_name(path: &NodePath) -> String {
    path.last()
        .map(|p| p.trim_end_matches(".pcad").to_string())
        .unwrap_or_default()
}

impl Workspace {
    pub fn new(library: Option<Library>, config: Config) -> Workspace {
        let mut w = Workspace {
            tabs: Vec::new(),
            active: 0,
            library,
            config,
            prompt: None,
            quit: false,
            autosave_interval: AUTOSAVE_INTERVAL,
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
    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.new_tab();
        } else if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
    }

    /// `true` = geschlossen, `false` = Rückfrage (steht in `prompt`).
    pub fn request_close(&mut self, i: usize) -> bool {
        let Some(t) = self.tabs.get(i) else {
            return true;
        };
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

    /// `true` = darf beenden (setzt `quit`); sonst fragt `prompt` nach dem ersten geänderten Tab.
    pub fn request_quit(&mut self) -> bool {
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
                store.discard_autosave();
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
        Self::save_tab(&mut self.tabs[i], comment, new_branch)
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
        let name = validate_name(name).map_err(err)?;
        let mut path = parent.clone();
        path.push(format!("{name}.pcad"));
        let dir = lib.root().resolve(&path).map_err(err)?;
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
        tab.mark_clean();
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
            Ok(SaveOutcome::Saved(_)) => self.finish_after(tab, Some(then)),
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
        let autosave = match store.read_autosave() {
            Ok(Some(a)) if a.time.as_str() >= store.info().latest_time() => Some(a),
            Ok(_) => None,
            Err(e) => {
                notes.push(format!(
                    "Die Autosicherung ist unlesbar und wird ignoriert: {e}"
                ));
                store.discard_autosave();
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
        let i = self.push_tab(tab);
        if offered {
            self.prompt = Some(Prompt::AutosaveOffer { tab: id });
        }
        Ok(OpenResult {
            tab: i,
            notes,
            autosave_offered: offered,
        })
    }

    /// Übernimmt die angebotene Autosicherung in den Tab (gilt danach als ungespeichert).
    pub fn apply_autosave(&mut self, tab: u64) {
        if matches!(self.prompt, Some(Prompt::AutosaveOffer { .. })) {
            self.prompt = None;
        }
        let Some(i) = self.tab_index(tab) else { return };
        let Some(a) = self.tabs[i].pending_autosave.take() else {
            return;
        };
        match AppState::from_snapshot(a.snapshot) {
            Ok(mut state) => {
                state.dark = self.tabs[i].state.dark;
                state.rebuild_scene();
                let t = &mut self.tabs[i];
                t.state = state;
                t.based_on = a.based_on.or(t.based_on);
                // gilt als geändert gegenüber dem gespeicherten Stand
                t.saved_edits = u64::MAX;
            }
            Err(e) => {
                self.prompt = Some(Prompt::Notice(format!(
                    "Die Autosicherung lässt sich nicht laden: {e}"
                )))
            }
        }
    }

    /// Verwirft die angebotene Autosicherung und löscht ihre Datei.
    pub fn discard_autosave(&mut self, tab: u64) {
        if matches!(self.prompt, Some(Prompt::AutosaveOffer { .. })) {
            self.prompt = None;
        }
        if let Some(i) = self.tab_index(tab) {
            self.tabs[i].pending_autosave = None;
            if let Some(s) = &self.tabs[i].store {
                s.discard_autosave();
            }
        }
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
    /// Änderungen im Zieltab verhindern das Wiederherstellen.
    pub fn restore_version(&mut self, i: usize, n: u32) -> Result<(), String> {
        let read_only = self.tabs.get(i).ok_or("Kein solcher Tab.")?.read_only;
        let target = if read_only {
            match self.tabs[i]
                .path
                .as_ref()
                .and_then(|p| self.tab_for_path(p))
            {
                Some(j) => j,
                None => {
                    self.make_editable(i)?;
                    i
                }
            }
        } else {
            i
        };
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
        tab.state = state;
        tab.based_on = Some(new);
        tab.mark_clean();
        self.active = target;
        Ok(())
    }

    /// Wählt die Wurzel (Projektordner, wird bei Bedarf angelegt) und merkt sie in der
    /// Konfiguration (gespeichert wird mit `persist_config`).
    pub fn choose_root(&mut self, root: &Path) -> Result<(), String> {
        let lib = Library::open(root).map_err(err)?;
        self.library = Some(lib);
        self.config.root = Some(root.to_path_buf());
        Ok(())
    }

    /// Speichert die Konfiguration (gewählte Wurzel); Fehler sind nicht schlimm.
    pub fn persist_config(&self) {
        let _ = self.config.save();
    }

    /// Schreibt für geänderte, gespeicherte Dokumente eine Autosicherung, wenn das Intervall
    /// seit der letzten abgelaufen ist.
    pub fn autosave_tick(&mut self, now: Instant) {
        let interval = self.autosave_interval;
        for t in &mut self.tabs {
            if t.read_only || !t.is_dirty() || t.state.document.edit_count() == t.autosaved_edits {
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
                t.last_autosave = now;
            }
        }
    }
}
