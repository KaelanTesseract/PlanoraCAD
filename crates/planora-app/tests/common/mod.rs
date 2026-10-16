//! Gemeinsame Hilfen der Oberflächentests.

use std::cell::RefCell;
use std::path::PathBuf;

/// Löscht den Bibliotheksordner eines Tests, wenn der Test (genauer: sein Thread) endet.
struct TempRoots(Vec<PathBuf>);

impl Drop for TempRoots {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

thread_local! {
    static ROOTS: RefCell<TempRoots> = const { RefCell::new(TempRoots(Vec::new())) };
}

/// Neuer Ordner `%TEMP%\<prefix>-<id>` für die Bibliothek eines Tests; er wird nach dem Test
/// gelöscht, damit Läufe keine Ordner ansammeln. (Der Ordner selbst wird erst von
/// `Library::open` angelegt.)
#[allow(dead_code)]
pub fn temp_root(prefix: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("{prefix}-{}", planora_project::fsutil::new_id()));
    ROOTS.with(|r| r.borrow_mut().0.push(root.clone()));
    root
}
