//! Versionsspeicher eines Dokuments: `dokument.json` (Versionsbaum) und `versionen/NNNN.json`.

use crate::convert::{FORMAT_VERSION, decode_version, encode_version};
use crate::error::{ProjectError, Result};
use crate::fsutil::{DocLock, atomic_write, new_id, now_iso};
use crate::library::DOCUMENT_FILE;
use planora_feature::Snapshot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAIN_BRANCH: &str = "Haupt";
pub const VERSIONS_DIR: &str = "versionen";
pub const AUTOSAVE_FILE: &str = ".autosave.json";
pub const MAX_BRANCH_CHARS: usize = 60;
pub const MAX_COMMENT_CHARS: usize = 500;
/// Mehr Versionen in einer `dokument.json` gelten als beschädigt.
const MAX_VERSIONS: usize = 100_000;
const PROGRAM: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq)]
pub struct VersionInfo {
    pub number: u32,
    pub parent: Option<u32>,
    pub branch: String,
    pub time: String,
    pub comment: String,
    pub program: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentInfo {
    pub id: String,
    pub versions: Vec<VersionInfo>,
    pub branches: BTreeMap<String, u32>,
    pub active_branch: String,
    /// Höchste Versionsnummer, die je vergeben wurde (auch für inzwischen gelöschte Versionen),
    /// damit Nummern nie wiederverwendet werden.
    pub max_number: u32,
}

impl DocumentInfo {
    /// Sucht eine Version. `versions` ist (nach `check`) aufsteigend nach Nummer sortiert.
    pub fn version(&self, n: u32) -> Option<&VersionInfo> {
        self.versions
            .binary_search_by_key(&n, |v| v.number)
            .ok()
            .map(|i| &self.versions[i])
    }

    pub fn tip(&self, branch: &str) -> Option<u32> {
        self.branches.get(branch).copied()
    }

    pub fn children(&self, n: u32) -> Vec<u32> {
        self.versions
            .iter()
            .filter(|v| v.parent == Some(n))
            .map(|v| v.number)
            .collect()
    }

    /// Zeitpunkt der jüngsten Version (ISO-Texte sind als Text vergleichbar).
    pub fn latest_time(&self) -> &str {
        self.versions
            .iter()
            .map(|v| v.time.as_str())
            .max()
            .unwrap_or("")
    }

    /// Prüft den Versionsbaum: Nummern aufsteigend und eindeutig, genau eine Wurzel, Eltern
    /// früher und vorhanden, jeder Zweig nicht leer mit seiner jüngsten Version als Spitze,
    /// aktiver Zweig vorhanden.
    pub fn check(&self) -> std::result::Result<(), String> {
        if self.versions.is_empty() {
            return Err("Das Dokument hat keine Version".into());
        }
        let mut seen = BTreeSet::new();
        let mut last = 0;
        // jüngste Version je Zweig, in einem Durchgang
        let mut newest: BTreeMap<&str, u32> = BTreeMap::new();
        for (i, v) in self.versions.iter().enumerate() {
            if v.number == 0 || v.number <= last {
                return Err(format!(
                    "Versionsnummer {} ist doppelt oder nicht aufsteigend",
                    v.number
                ));
            }
            last = v.number;
            match v.parent {
                None if i != 0 => {
                    return Err(format!("Version {} hat keine Elternversion", v.number));
                }
                Some(p) if p >= v.number || !seen.contains(&p) => {
                    return Err(format!(
                        "Version {} verweist auf eine ungültige Elternversion {p}",
                        v.number
                    ));
                }
                _ => {}
            }
            seen.insert(v.number);
            if !branch_name_ok(&v.branch) {
                return Err(format!(
                    "Version {} hat einen ungültigen Zweignamen",
                    v.number
                ));
            }
            if !time_ok(&v.time) {
                return Err(format!(
                    "Version {} hat eine ungültige Zeitangabe",
                    v.number
                ));
            }
            if v.comment.chars().count() > MAX_COMMENT_CHARS
                || v.comment.chars().any(|c| c.is_control() && c != '\n')
            {
                return Err(format!(
                    "Version {} hat einen ungültigen Kommentar",
                    v.number
                ));
            }
            newest.insert(&v.branch, v.number);
        }
        if self.max_number < last {
            return Err("Die höchste vergebene Versionsnummer stimmt nicht".into());
        }
        if newest.len() != self.branches.len() {
            return Err("Zweige und Versionen passen nicht zusammen".into());
        }
        for (name, tip) in &self.branches {
            if newest.get(name.as_str()) != Some(tip) {
                return Err(format!("Die Spitze des Zweigs „{name}“ stimmt nicht"));
            }
        }
        if !self.branches.contains_key(&self.active_branch) {
            return Err("Der aktive Zweig gibt es nicht".into());
        }
        Ok(())
    }
}

/// Zweigname: getrimmt nicht leer, höchstens 60 Zeichen, keine Steuerzeichen.
pub fn branch_name_ok(n: &str) -> bool {
    !n.trim().is_empty()
        && n == n.trim()
        && n.chars().count() <= MAX_BRANCH_CHARS
        && !n.chars().any(char::is_control)
}

/// Zeitangabe in der festen Form „JJJJ-MM-TTThh:mm:ssZ“ (nur Ziffern und Trennzeichen), damit
/// Textvergleiche stimmen.
fn time_ok(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

/// Liest zuerst nur das Format, damit eine neuere Datei auch bei geänderter Struktur die
/// richtige Meldung (zu neu) bekommt.
fn check_head(text: &str, origin: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Head {
        format: u32,
    }
    let head: Head =
        serde_json::from_str(text).map_err(|e| ProjectError::corrupt(origin, e.to_string()))?;
    if head.format > FORMAT_VERSION {
        return Err(ProjectError::TooNew {
            path: origin.into(),
            found: head.format,
        });
    }
    if head.format == 0 {
        return Err(ProjectError::corrupt(origin, "ungültiges Format 0"));
    }
    Ok(())
}

fn clean_comment(t: &str) -> Result<String> {
    let t: String = t
        .trim()
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    if t.chars().count() > MAX_COMMENT_CHARS {
        return Err(ProjectError::Invalid(format!(
            "Der Kommentar ist zu lang (höchstens {MAX_COMMENT_CHARS} Zeichen)."
        )));
    }
    Ok(t)
}

// ---- dokument.json ----------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct DocFile {
    format: u32,
    programm: String,
    id: String,
    versionen: Vec<VersionDto>,
    zweige: BTreeMap<String, u32>,
    aktiver_zweig: String,
    /// Höchste je vergebene Versionsnummer (fehlt in älteren Dateien).
    #[serde(default)]
    hoechste_nummer: u32,
}

#[derive(Serialize, Deserialize)]
struct VersionDto {
    nummer: u32,
    eltern: Option<u32>,
    zweig: String,
    zeit: String,
    kommentar: String,
    programm: String,
}

fn to_file(i: &DocumentInfo) -> DocFile {
    DocFile {
        format: FORMAT_VERSION,
        programm: PROGRAM.into(),
        id: i.id.clone(),
        versionen: i
            .versions
            .iter()
            .map(|v| VersionDto {
                nummer: v.number,
                eltern: v.parent,
                zweig: v.branch.clone(),
                zeit: v.time.clone(),
                kommentar: v.comment.clone(),
                programm: v.program.clone(),
            })
            .collect(),
        zweige: i.branches.clone(),
        aktiver_zweig: i.active_branch.clone(),
        hoechste_nummer: i.max_number,
    }
}

fn from_file(f: DocFile, origin: &str) -> Result<DocumentInfo> {
    if f.versionen.len() > MAX_VERSIONS {
        return Err(ProjectError::corrupt(origin, "zu viele Versionen"));
    }
    let tree_max = f.versionen.iter().map(|v| v.nummer).max().unwrap_or(0);
    let info = DocumentInfo {
        id: f.id,
        versions: f
            .versionen
            .into_iter()
            .map(|v| VersionInfo {
                number: v.nummer,
                parent: v.eltern,
                branch: v.zweig,
                time: v.zeit,
                comment: v.kommentar,
                program: v.programm,
            })
            .collect(),
        branches: f.zweige,
        active_branch: f.aktiver_zweig,
        max_number: f.hoechste_nummer.max(tree_max),
    };
    info.check().map_err(|m| ProjectError::corrupt(origin, m))?;
    Ok(info)
}

fn version_file(n: u32) -> String {
    format!("{n:04}.json")
}

/// Wie die Sperre beim Öffnen behandelt wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Acquire,
    TakeOver,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenReport {
    /// Dateien in `versionen/`, die `dokument.json` nicht kennt (Reste eines abgebrochenen Speicherns).
    pub orphans: Vec<String>,
    /// Versionen, die `dokument.json` nennt, deren Datei aber fehlt.
    pub missing: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Autosave {
    pub based_on: Option<u32>,
    pub time: String,
    pub snapshot: Snapshot,
}

#[derive(Serialize, Deserialize)]
struct AutosaveFile {
    format: u32,
    basiert_auf: Option<u32>,
    zeit: String,
    /// Versionsdatei als Text (derselbe Inhalt wie `versionen/NNNN.json`).
    inhalt: String,
}

/// Liest Versionsbaum und Befund, ohne Sperre und ohne etwas zu ändern (für schreibgeschützte
/// Ansichten und Listen).
pub fn peek(dir: &Path) -> Result<(DocumentInfo, OpenReport)> {
    let doc = dir.join(DOCUMENT_FILE);
    let origin = doc.display().to_string();
    let text = fs::read_to_string(&doc).map_err(|e| ProjectError::io(&doc, &e))?;
    check_head(&text, &origin)?;
    let file: DocFile = serde_json::from_str(&text)
        .map_err(|e| ProjectError::corrupt(origin.clone(), e.to_string()))?;
    let info = from_file(file, &origin)?;
    let mut report = OpenReport::default();
    let mut present = BTreeSet::new();
    if let Ok(rd) = fs::read_dir(dir.join(VERSIONS_DIR)) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".tmp") {
                continue;
            }
            match name
                .strip_suffix(".json")
                .and_then(|n| n.parse::<u32>().ok())
            {
                Some(n) if info.version(n).is_some() => {
                    present.insert(n);
                }
                _ => report.orphans.push(name),
            }
        }
    }
    report.orphans.sort();
    report.missing = info
        .versions
        .iter()
        .map(|v| v.number)
        .filter(|n| !present.contains(n))
        .collect();
    Ok((info, report))
}

/// Liest eine Version, ohne Sperre.
pub fn read_version(dir: &Path, n: u32) -> Result<Snapshot> {
    let (info, _) = peek(dir)?;
    if info.version(n).is_none() {
        return Err(ProjectError::NotFound(format!("Version {n}")));
    }
    let path = dir.join(VERSIONS_DIR).join(version_file(n));
    let text = fs::read_to_string(&path).map_err(|e| ProjectError::io(&path, &e))?;
    decode_version(&text, &path.display().to_string())
}

pub struct DocStore {
    dir: PathBuf,
    info: DocumentInfo,
    _lock: DocLock,
}

impl DocStore {
    /// Legt ein neues Dokument an. `dir` darf noch nicht existieren (Ordnername endet auf `.pcad`).
    /// Schlägt das Schreiben fehl, wird der angelegte Ordner wieder entfernt.
    pub fn create(dir: &Path, snapshot: &Snapshot, comment: &str) -> Result<DocStore> {
        Self::create_with(dir, snapshot, comment, |_| {})
    }

    /// Wie `create`; `after_mkdir` läuft direkt nach dem Anlegen der Ordner (Prüfhaken für Tests).
    fn create_with(
        dir: &Path,
        snapshot: &Snapshot,
        comment: &str,
        after_mkdir: impl FnOnce(&Path),
    ) -> Result<DocStore> {
        if dir.exists() {
            return Err(ProjectError::Exists(dir.display().to_string()));
        }
        let comment = clean_comment(comment)?;
        let text = encode_version(snapshot)?;
        if let Some(parent) = dir.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|e| ProjectError::io(parent, &e))?;
        }
        // create_dir (nicht create_dir_all): ein gleichzeitig angelegter Ordner ist ein Fehler
        fs::create_dir(dir).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                ProjectError::Exists(dir.display().to_string())
            } else {
                ProjectError::io(dir, &e)
            }
        })?;
        // ab hier gehört der Ordner uns: bei einem Fehler wieder entfernen
        let built = (|| {
            fs::create_dir(dir.join(VERSIONS_DIR))
                .map_err(|e| ProjectError::io(&dir.join(VERSIONS_DIR), &e))?;
            after_mkdir(dir);
            let lock = DocLock::acquire(dir)?;
            let info = DocumentInfo {
                id: new_id(),
                versions: vec![VersionInfo {
                    number: 1,
                    parent: None,
                    branch: MAIN_BRANCH.into(),
                    time: now_iso(),
                    comment,
                    program: PROGRAM.into(),
                }],
                branches: BTreeMap::from([(MAIN_BRANCH.to_string(), 1)]),
                active_branch: MAIN_BRANCH.into(),
                max_number: 1,
            };
            atomic_write(
                &dir.join(VERSIONS_DIR).join(version_file(1)),
                text.as_bytes(),
            )?;
            let store = DocStore {
                dir: dir.to_path_buf(),
                info,
                _lock: lock,
            };
            store.persist(&store.info)?;
            Ok(store)
        })();
        if built.is_err() {
            // die Sperre ist hier schon freigegeben (Store und Sperre wurden verworfen)
            let _ = fs::remove_dir_all(dir);
        }
        built
    }

    pub fn open(dir: &Path, mode: LockMode) -> Result<(DocStore, OpenReport)> {
        let lock = match mode {
            LockMode::Acquire => DocLock::acquire(dir)?,
            LockMode::TakeOver => DocLock::take_over(dir)?,
        };
        let (info, report) = peek(dir)?;
        // Reste abgebrochener Schreibvorgänge entfernen (nur mit Sperre)
        for name in [DOCUMENT_FILE, AUTOSAVE_FILE] {
            let _ = fs::remove_file(dir.join(format!("{name}.tmp")));
        }
        if let Ok(rd) = fs::read_dir(dir.join(VERSIONS_DIR)) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().ends_with(".tmp") {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
        Ok((
            DocStore {
                dir: dir.to_path_buf(),
                info,
                _lock: lock,
            },
            report,
        ))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn info(&self) -> &DocumentInfo {
        &self.info
    }

    fn persist(&self, info: &DocumentInfo) -> Result<()> {
        let text = serde_json::to_string_pretty(&to_file(info))
            .map_err(|e| ProjectError::Invalid(e.to_string()))?;
        atomic_write(&self.dir.join(DOCUMENT_FILE), text.as_bytes())
    }

    pub fn load_version(&self, n: u32) -> Result<Snapshot> {
        if self.info.version(n).is_none() {
            return Err(ProjectError::NotFound(format!("Version {n}")));
        }
        let path = self.dir.join(VERSIONS_DIR).join(version_file(n));
        let text = fs::read_to_string(&path).map_err(|e| ProjectError::io(&path, &e))?;
        decode_version(&text, &path.display().to_string())
    }

    /// Spitze des Zweigs oder, wenn deren Datei fehlt, die nächste Vorfahrenversion mit Datei.
    /// Prüft nur, dass die Datei existiert, nicht, dass sie sich lesen lässt.
    pub fn latest_readable(&self, branch: &str) -> Option<u32> {
        let mut cur = self.info.tip(branch);
        while let Some(n) = cur {
            if self.dir.join(VERSIONS_DIR).join(version_file(n)).is_file() {
                return Some(n);
            }
            cur = self.info.version(n).and_then(|v| v.parent);
        }
        None
    }

    fn next_number(&self) -> Result<u32> {
        let mut max = self.info.max_number.max(
            self.info
                .versions
                .iter()
                .map(|v| v.number)
                .max()
                .unwrap_or(0),
        );
        if let Ok(rd) = fs::read_dir(self.dir.join(VERSIONS_DIR)) {
            for e in rd.flatten() {
                if let Some(n) = e
                    .file_name()
                    .to_string_lossy()
                    .strip_suffix(".json")
                    .and_then(|n| n.parse::<u32>().ok())
                {
                    max = max.max(n);
                }
            }
        }
        max.checked_add(1)
            .ok_or_else(|| ProjectError::Invalid("Die Versionsnummern sind aufgebraucht.".into()))
    }

    pub fn save(
        &mut self,
        based_on: Option<u32>,
        snapshot: &Snapshot,
        comment: &str,
        new_branch: Option<&str>,
    ) -> Result<u32> {
        let comment = clean_comment(comment)?;
        let (parent, branch) = match based_on {
            None => {
                let b = self.info.active_branch.clone();
                let tip = self.info.tip(&b);
                match new_branch {
                    // Mit Zweigname und ohne Ausgangsversion: neuer Zweig ab der Spitze des
                    // aktiven Zweigs (der Name wird nie stillschweigend ignoriert).
                    Some(name) => {
                        if !branch_name_ok(name) || self.info.branches.contains_key(name) {
                            return Err(ProjectError::InvalidName(
                                "Der Zweigname ist ungültig oder vergeben.".into(),
                            ));
                        }
                        (tip, name.to_string())
                    }
                    None => (tip, b),
                }
            }
            Some(n) => {
                let v = self
                    .info
                    .version(n)
                    .ok_or_else(|| ProjectError::NotFound(format!("Version {n}")))?;
                match new_branch {
                    Some(name) => {
                        if !branch_name_ok(name) || self.info.branches.contains_key(name) {
                            return Err(ProjectError::InvalidName(
                                "Der Zweigname ist ungültig oder vergeben.".into(),
                            ));
                        }
                        (Some(n), name.to_string())
                    }
                    None if self.info.tip(&v.branch) == Some(n) => (Some(n), v.branch.clone()),
                    None => return Err(ProjectError::NeedsBranch),
                }
            }
        };
        self.append(parent, branch, snapshot, comment)
    }

    pub fn restore(&mut self, version: u32, onto_branch: &str) -> Result<u32> {
        let tip = self
            .info
            .tip(onto_branch)
            .ok_or_else(|| ProjectError::NotFound(format!("Zweig {onto_branch}")))?;
        let snapshot = self.load_version(version)?;
        self.append(
            Some(tip),
            onto_branch.to_string(),
            &snapshot,
            format!("Wiederhergestellt aus v{version}"),
        )
    }

    fn append(
        &mut self,
        parent: Option<u32>,
        branch: String,
        snapshot: &Snapshot,
        comment: String,
    ) -> Result<u32> {
        let text = encode_version(snapshot)?;
        let number = self.next_number()?;
        let mut info = self.info.clone();
        info.versions.push(VersionInfo {
            number,
            parent,
            branch: branch.clone(),
            time: now_iso(),
            comment,
            program: PROGRAM.into(),
        });
        info.branches.insert(branch.clone(), number);
        info.active_branch = branch;
        info.max_number = info.max_number.max(number);
        info.check().map_err(ProjectError::Invalid)?;
        // erst die Versionsdatei, dann der Versionsbaum: bricht es dazwischen ab, bleibt der
        // alte Stand gültig und die Datei verwaist
        atomic_write(
            &self.dir.join(VERSIONS_DIR).join(version_file(number)),
            text.as_bytes(),
        )?;
        self.persist(&info)?;
        self.info = info;
        Ok(number)
    }

    pub fn set_comment(&mut self, n: u32, text: &str) -> Result<()> {
        let text = clean_comment(text)?;
        let mut info = self.info.clone();
        let v = info
            .versions
            .iter_mut()
            .find(|v| v.number == n)
            .ok_or_else(|| ProjectError::NotFound(format!("Version {n}")))?;
        v.comment = text;
        self.persist(&info)?;
        self.info = info;
        Ok(())
    }

    pub fn set_active_branch(&mut self, name: &str) -> Result<()> {
        if !self.info.branches.contains_key(name) {
            return Err(ProjectError::NotFound(format!("Zweig {name}")));
        }
        let mut info = self.info.clone();
        info.active_branch = name.to_string();
        self.persist(&info)?;
        self.info = info;
        Ok(())
    }

    pub fn delete_version(&mut self, n: u32) -> Result<()> {
        let v = self
            .info
            .version(n)
            .ok_or_else(|| ProjectError::NotFound(format!("Version {n}")))?
            .clone();
        if self.info.versions.len() == 1 {
            return Err(ProjectError::Invalid(
                "Die einzige Version lässt sich nicht löschen.".into(),
            ));
        }
        if !self.info.children(n).is_empty() {
            return Err(ProjectError::Invalid(
                "Auf dieser Version bauen andere auf.".into(),
            ));
        }
        if self.info.tip(&v.branch) != Some(n) {
            return Err(ProjectError::Invalid(
                "Nur die neueste Version eines Zweigs lässt sich löschen.".into(),
            ));
        }
        let mut info = self.info.clone();
        info.versions.retain(|x| x.number != n);
        match info
            .versions
            .iter()
            .filter(|x| x.branch == v.branch)
            .map(|x| x.number)
            .max()
        {
            Some(tip) => {
                info.branches.insert(v.branch.clone(), tip);
            }
            None => {
                info.branches.remove(&v.branch);
                if info.active_branch == v.branch {
                    let parent_branch = v
                        .parent
                        .and_then(|p| info.version(p))
                        .map(|p| p.branch.clone());
                    info.active_branch = parent_branch.unwrap_or_else(|| {
                        info.branches.keys().next().cloned().unwrap_or_default()
                    });
                }
            }
        }
        info.check().map_err(ProjectError::Invalid)?;
        self.persist(&info)?;
        self.info = info;
        let _ = fs::remove_file(self.dir.join(VERSIONS_DIR).join(version_file(n)));
        Ok(())
    }

    pub fn delete_branch(&mut self, name: &str) -> Result<()> {
        if !self.info.branches.contains_key(name) {
            return Err(ProjectError::NotFound(format!("Zweig {name}")));
        }
        if self.info.active_branch == name {
            return Err(ProjectError::Invalid(
                "Der aktive Zweig lässt sich nicht löschen.".into(),
            ));
        }
        let own: BTreeSet<u32> = self
            .info
            .versions
            .iter()
            .filter(|v| v.branch == name)
            .map(|v| v.number)
            .collect();
        if own
            .iter()
            .any(|n| self.info.children(*n).iter().any(|c| !own.contains(c)))
        {
            return Err(ProjectError::Invalid(
                "Auf diesem Zweig bauen andere Zweige auf.".into(),
            ));
        }
        let mut info = self.info.clone();
        info.versions.retain(|v| !own.contains(&v.number));
        info.branches.remove(name);
        info.check().map_err(ProjectError::Invalid)?;
        self.persist(&info)?;
        self.info = info;
        for n in own {
            let _ = fs::remove_file(self.dir.join(VERSIONS_DIR).join(version_file(n)));
        }
        Ok(())
    }

    pub fn write_autosave(&self, based_on: Option<u32>, snapshot: &Snapshot) -> Result<()> {
        let file = AutosaveFile {
            format: FORMAT_VERSION,
            basiert_auf: based_on,
            zeit: now_iso(),
            inhalt: encode_version(snapshot)?,
        };
        let text =
            serde_json::to_string(&file).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        atomic_write(&self.dir.join(AUTOSAVE_FILE), text.as_bytes())
    }

    pub fn read_autosave(&self) -> Result<Option<Autosave>> {
        let path = self.dir.join(AUTOSAVE_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        let origin = path.display().to_string();
        let text = fs::read_to_string(&path).map_err(|e| ProjectError::io(&path, &e))?;
        check_head(&text, &origin)?;
        let f: AutosaveFile = serde_json::from_str(&text)
            .map_err(|e| ProjectError::corrupt(origin.clone(), e.to_string()))?;
        if let Some(n) = f.basiert_auf
            && self.info.version(n).is_none()
        {
            return Err(ProjectError::corrupt(
                origin,
                format!("die Autosicherung beruht auf der unbekannten Version {n}"),
            ));
        }
        Ok(Some(Autosave {
            based_on: f.basiert_auf,
            time: f.zeit,
            snapshot: decode_version(&f.inhalt, &origin)?,
        }))
    }

    pub fn discard_autosave(&self) {
        let _ = fs::remove_file(self.dir.join(AUTOSAVE_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_create_removes_the_folder_it_made() {
        let dir = std::env::temp_dir().join(format!("planora-create-{}.pcad", new_id()));
        let snap = planora_feature::Document::new().snapshot();
        let r = DocStore::create_with(&dir, &snap, "", |d| {
            // Die Versionsdatei lässt sich nicht schreiben, wenn dort ein Ordner steht.
            fs::create_dir_all(d.join(VERSIONS_DIR).join(version_file(1))).unwrap();
        });
        assert!(r.is_err());
        assert!(!dir.exists(), "der halb gebaute Ordner wurde entfernt");
    }
}
