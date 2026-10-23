//! Schnellzugriffe der Dateiauswahl: bekannte Ordner (Dokumente, Downloads, Desktop, Bilder),
//! der Projektordner, die vorhandenen Laufwerke und die Favoriten des Benutzers.
//!
//! Die Laufwerke werden einmal im Hintergrund ermittelt ([`system_drives`]): Ein getrenntes
//! Netzlaufwerk kann die Abfrage viele Sekunden aufhalten, die Oberfläche zeigt bis dahin nur
//! die schon gefundenen.

use crate::icons::Icon;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// Höchstzahl der Favoriten.
pub const MAX_FAVORITES: usize = planora_project::config::MAX_FAVORITES;
/// Meldung, wenn schon [`MAX_FAVORITES`] Favoriten da sind.
pub const FAVORITES_FULL: &str = "Es gibt schon 24 Favoriten. Bitte zuerst einen entfernen.";
/// Eintrag im Kontextmenü eines Ordners.
pub const ADD_FAVORITE: &str = "Zu Favoriten hinzufügen";
/// Eintrag im Kontextmenü eines Ordners oder Favoriten.
pub const REMOVE_FAVORITE: &str = "Aus Favoriten entfernen";
/// Meldung, wenn der Ordner eines Favoriten fehlt.
pub const FAVORITE_MISSING: &str = "Diesen Ordner gibt es nicht. Mit einem Rechtsklick auf den Knopf lässt er sich aus den Favoriten entfernen.";

/// Art eines Schnellzugriffs (bestimmt das Symbol).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceKind {
    Documents,
    Downloads,
    Desktop,
    Pictures,
    Project,
    Drive,
    Favorite,
}

impl PlaceKind {
    /// Symbol des Knopfs (Phosphor, Linienstil des Programms).
    pub fn icon(self) -> Icon {
        match self {
            PlaceKind::Documents => Icon::Documents,
            PlaceKind::Downloads => Icon::Import,
            PlaceKind::Desktop => Icon::Desktop,
            PlaceKind::Pictures => Icon::Pictures,
            PlaceKind::Project => Icon::Projects,
            PlaceKind::Drive => Icon::Drive,
            PlaceKind::Favorite => Icon::Favorite,
        }
    }
}

/// Ein Knopf der Schnellzugriffe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub kind: PlaceKind,
    pub label: String,
    pub path: PathBuf,
}

/// Bekannte Ordner: Art, Name im Benutzerordner, Beschriftung.
const KNOWN: [(PlaceKind, &str, &str); 4] = [
    (PlaceKind::Documents, "Documents", "Dokumente"),
    (PlaceKind::Downloads, "Downloads", "Downloads"),
    (PlaceKind::Desktop, "Desktop", "Desktop"),
    (PlaceKind::Pictures, "Pictures", "Bilder"),
];

/// Beschriftung des Projektordners.
pub const PROJECT_LABEL: &str = "Projekte";

/// Benutzerordner (`USERPROFILE`, sonst `HOME`).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Ordner `name` im Benutzerordner; fehlt er, der gleichnamige Ordner unter `OneDrive`, wenn
/// es den gibt.
pub fn known_folder(home: &Path, name: &str, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    let plain = home.join(name);
    if exists(&plain) {
        return Some(plain);
    }
    let cloud = home.join("OneDrive").join(name);
    exists(&cloud).then_some(cloud)
}

/// Die vorhandenen bekannten Ordner (Dokumente, Downloads, Desktop, Bilder).
pub fn known_places(home: Option<&Path>, exists: &dyn Fn(&Path) -> bool) -> Vec<Place> {
    let Some(home) = home else {
        return Vec::new();
    };
    KNOWN
        .iter()
        .filter_map(|&(kind, name, label)| {
            known_folder(home, name, exists).map(|path| Place {
                kind,
                label: label.to_string(),
                path,
            })
        })
        .collect()
}

/// Alle Knöpfe in ihrer Reihenfolge: bekannte Ordner, Projektordner (nur wenn vorhanden, vom
/// Aufrufer geprüft), Laufwerke, Favoriten (immer, auch wenn ihr Ordner fehlt).
pub fn build_places(
    known: &[Place],
    project: Option<&Path>,
    drives: &[PathBuf],
    favorites: &[PathBuf],
) -> Vec<Place> {
    let mut out = known.to_vec();
    if let Some(p) = project {
        out.push(Place {
            kind: PlaceKind::Project,
            label: PROJECT_LABEL.to_string(),
            path: p.to_path_buf(),
        });
    }
    out.extend(drives.iter().map(|d| Place {
        kind: PlaceKind::Drive,
        label: drive_label(d),
        path: d.clone(),
    }));
    out.extend(
        favorites
            .iter()
            .zip(favorite_labels(favorites))
            .map(|(p, label)| Place {
                kind: PlaceKind::Favorite,
                label,
                path: p.clone(),
            }),
    );
    out
}

/// „C:\“ wird „C:“.
pub fn drive_label(path: &Path) -> String {
    let s = path.display().to_string();
    let t = s.trim_end_matches(['\\', '/']);
    if t.is_empty() { s } else { t.to_string() }
}

/// Vergleichsform eines Ordners: unter Windows ohne Groß- und Kleinschreibung und mit `\`;
/// abschließende Trennzeichen zählen nicht.
pub fn normalize(path: &Path) -> String {
    planora_project::config::path_key(path)
}

/// Derselbe Ordner (nach [`normalize`])?
pub fn same_dir(a: &Path, b: &Path) -> bool {
    normalize(a) == normalize(b)
}

/// Ist `place` der aktuelle Ordner (hervorgehoben)?
pub fn is_current(place: &Place, dir: &Path) -> bool {
    same_dir(&place.path, dir)
}

/// Steht `path` in den Favoriten?
pub fn is_favorite(favorites: &[PathBuf], path: &Path) -> bool {
    favorites.iter().any(|f| same_dir(f, path))
}

/// Fügt `path` hinten an; `Ok(false)`, wenn er schon da ist, Fehlermeldung bei voller Liste.
pub fn add_favorite(favorites: &mut Vec<PathBuf>, path: &Path) -> Result<bool, &'static str> {
    if is_favorite(favorites, path) {
        return Ok(false);
    }
    if favorites.len() >= MAX_FAVORITES {
        return Err(FAVORITES_FULL);
    }
    favorites.push(path.to_path_buf());
    Ok(true)
}

/// Entfernt `path` (in jeder Schreibweise); `true`, wenn er da war.
pub fn remove_favorite(favorites: &mut Vec<PathBuf>, path: &Path) -> bool {
    let before = favorites.len();
    favorites.retain(|f| !same_dir(f, path));
    favorites.len() != before
}

fn names(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn key(s: &str) -> String {
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.to_string()
    }
}

/// Beschriftungen der Favoriten: der Ordnername; heißen zwei gleich, so viele übergeordnete
/// Ordner davor, bis sie sich unterscheiden (notfalls der ganze Pfad).
pub fn favorite_labels(favorites: &[PathBuf]) -> Vec<String> {
    let parts: Vec<Vec<String>> = favorites.iter().map(|p| names(p)).collect();
    let sep = std::path::MAIN_SEPARATOR.to_string();
    let suffix = |i: usize, k: usize| -> String {
        let n = &parts[i];
        n[n.len().saturating_sub(k)..].join(&sep)
    };
    (0..favorites.len())
        .map(|i| {
            let n = parts[i].len();
            if n == 0 {
                return drive_label(&favorites[i]);
            }
            for k in 1..=n {
                let mine = key(&suffix(i, k));
                let clash = (0..favorites.len()).any(|j| j != i && key(&suffix(j, k)) == mine);
                if !clash {
                    return suffix(i, k);
                }
            }
            drive_label(&favorites[i])
        })
        .collect()
}

/// Eine Änderung der Favoriten; beim Speichern wird sie auf den Stand der Datei angewandt
/// (nicht die ganze Liste aus dem Speicher geschrieben), damit nichts von einer anderen
/// Instanz verloren geht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FavoriteChange {
    Add(PathBuf),
    Remove(PathBuf),
}

impl FavoriteChange {
    /// Wendet die Änderung auf `favorites` an (eine volle Liste nimmt nichts mehr auf).
    pub fn apply(&self, favorites: &mut Vec<PathBuf>) {
        match self {
            FavoriteChange::Add(p) => {
                let _ = add_favorite(favorites, p);
            }
            FavoriteChange::Remove(p) => {
                remove_favorite(favorites, p);
            }
        }
    }
}

/// Gemeinsamer Stand der Schnellzugriffe für alle Dateiauswahlen (Netz importieren, STEP
/// importieren, Exportieren): kommt aus der Konfiguration und geht dorthin zurück
/// (`Workspace::places`, `Workspace::save_places`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Places {
    pub favorites: Vec<PathBuf>,
    /// Projektordner (gewählte Wurzel oder ihre Vorgabe).
    pub project_root: Option<PathBuf>,
    /// Änderungen der Favoriten in diesem Bild, in ihrer Reihenfolge.
    pub changes: Vec<FavoriteChange>,
}

impl Places {
    /// Wurden die Favoriten geändert (sollen sie gespeichert werden)?
    pub fn changed(&self) -> bool {
        !self.changes.is_empty()
    }

    /// Fügt einen Favoriten hinzu (siehe [`add_favorite`]).
    pub fn add_favorite(&mut self, path: &Path) -> Result<(), &'static str> {
        if add_favorite(&mut self.favorites, path)? {
            self.changes.push(FavoriteChange::Add(path.to_path_buf()));
        }
        Ok(())
    }

    /// Entfernt einen Favoriten (siehe [`remove_favorite`]).
    pub fn remove_favorite(&mut self, path: &Path) {
        if remove_favorite(&mut self.favorites, path) {
            self.changes
                .push(FavoriteChange::Remove(path.to_path_buf()));
        }
    }
}

/// Laufwerke, im Hintergrund ermittelt: [`DriveScan::known`] wartet nie.
pub struct DriveScan {
    found: Arc<Mutex<Vec<PathBuf>>>,
    done: Arc<AtomicBool>,
    started: Instant,
}

/// So lange wird höchstens auf hängende Laufwerke gewartet (neu gezeichnet); danach zeigt die
/// Auswahl die bis dahin gefundenen.
pub const DRIVE_WAIT: Duration = Duration::from_secs(3);

/// Rückmeldung eines Laufwerkssuchers (darf in weitere Threads mitgenommen werden).
#[derive(Clone)]
pub struct DriveSink(Arc<Mutex<Vec<PathBuf>>>);

impl DriveSink {
    pub fn report(&self, drive: PathBuf) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(drive);
    }
}

impl DriveScan {
    /// Startet `provider` in einem eigenen Thread; die Oberfläche wartet nicht darauf.
    pub fn start<F>(provider: F) -> DriveScan
    where
        F: FnOnce(DriveSink) + Send + 'static,
    {
        let found = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let sink = DriveSink(Arc::clone(&found));
        let flag = Arc::clone(&done);
        let spawned = std::thread::Builder::new()
            .name("planora-laufwerke".into())
            .spawn(move || {
                provider(sink);
                flag.store(true, Ordering::Release);
            });
        if spawned.is_err() {
            done.store(true, Ordering::Release);
        }
        DriveScan {
            found,
            done,
            started: Instant::now(),
        }
    }

    /// Die bisher gefundenen Laufwerke, sortiert.
    pub fn known(&self) -> Vec<PathBuf> {
        let mut v = self
            .found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        v.sort_by_key(|p| normalize(p));
        v.dedup_by(|a, b| same_dir(a, b));
        v
    }

    /// Ist die Suche fertig?
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    /// Soll die Auswahl noch neu zeichnen, um später gefundene Laufwerke zu zeigen? Nur
    /// solange die Suche läuft und höchstens [`DRIVE_WAIT`] seit ihrem Start.
    pub fn wants_repaint(&self) -> bool {
        self.wants_repaint_at(Instant::now())
    }

    /// Wie [`DriveScan::wants_repaint`] zum Zeitpunkt `now`.
    pub fn wants_repaint_at(&self, now: Instant) -> bool {
        !self.is_done() && now.saturating_duration_since(self.started) < DRIVE_WAIT
    }
}

/// Sucht die Laufwerksbuchstaben A: bis Z:, jeden in einem eigenen Thread, damit ein hängendes
/// Netzlaufwerk die anderen nicht aufhält. Außerhalb von Windows gibt es keine.
fn scan_drive_letters(sink: DriveSink) {
    if !cfg!(windows) {
        return;
    }
    let handles: Vec<_> = ('A'..='Z')
        .filter_map(|c| {
            let sink = sink.clone();
            std::thread::Builder::new()
                .name("planora-laufwerk".into())
                .spawn(move || {
                    let p = PathBuf::from(format!("{c}:\\"));
                    if p.is_dir() {
                        sink.report(p);
                    }
                })
                .ok()
        })
        .collect();
    for h in handles {
        let _ = h.join();
    }
}

/// Die Laufwerke dieses Rechners, einmal je Programmlauf im Hintergrund ermittelt.
pub fn system_drives() -> &'static DriveScan {
    static SCAN: OnceLock<DriveScan> = OnceLock::new();
    SCAN.get_or_init(|| DriveScan::start(scan_drive_letters))
}
