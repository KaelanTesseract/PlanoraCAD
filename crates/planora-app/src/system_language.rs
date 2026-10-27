//! Sprachen beim Start: die bevorzugten Sprachen des Systems (Vorschlag, solange in der
//! Konfiguration keine gewählt ist) und zusätzliche Sprachdateien aus den Ordnern `languages`
//! neben dem Programm und im Datenordner. Probleme mit Sprachdateien kommen englisch ins
//! Protokoll.

use planora_i18n::LoadProblem;
use std::path::{Path, PathBuf};

/// Name der Ordner mit zusätzlichen Sprachdateien (`*.ftl`).
pub const LANGUAGES_DIR: &str = "languages";

/// Bevorzugte Sprachen der Oberfläche, wichtigste zuerst, als Ids wie `de-DE`. Unter Windows
/// aus `GetUserPreferredUILanguages`, sonst aus `LC_ALL`, `LC_MESSAGES` und `LANG`. Leer, wenn
/// das System nichts meldet.
pub fn system_languages() -> Vec<String> {
    #[cfg(windows)]
    {
        let list = windows_ui_languages();
        if !list.is_empty() {
            return list;
        }
    }
    let mut out: Vec<String> = Vec::new();
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(id) = std::env::var(var).ok().and_then(|v| from_posix_locale(&v))
            && !out.contains(&id)
        {
            out.push(id);
        }
    }
    out
}

/// Macht aus einer POSIX-Locale eine Sprach-Id: `de_DE.UTF-8` → `de-DE`, `pt_BR.UTF-8@euro` →
/// `pt-BR`. `C`, `POSIX` und Leeres ergeben keine.
pub fn from_posix_locale(raw: &str) -> Option<String> {
    let base = raw.trim().split(['.', '@']).next().unwrap_or("").trim();
    if base.is_empty() || base.eq_ignore_ascii_case("C") || base.eq_ignore_ascii_case("POSIX") {
        return None;
    }
    Some(base.replace('_', "-"))
}

#[cfg(windows)]
fn windows_ui_languages() -> Vec<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut count = 0u32;
    let mut len = 0u32;
    // SAFETY: Erster Aufruf nur mit Zeigern auf lokale Zähler und ohne Puffer (fragt die Länge
    // ab).
    let ok = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut len,
        )
    };
    if ok == 0 || len == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: Der Puffer hat genau die eben gemeldete Länge in UTF-16-Zeichen.
    let ok = unsafe {
        GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buf.as_mut_ptr(), &mut len)
    };
    if ok == 0 {
        return Vec::new();
    }
    // Mehrfach nullterminierte Liste: Einträge durch 0 getrennt, am Ende zwei Nullen.
    buf.truncate((len as usize).min(buf.len()));
    buf.split(|c| *c == 0)
        .map(String::from_utf16_lossy)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Ordner mit zusätzlichen Sprachdateien: neben dem Programm und im Datenordner (neben der
/// Konfiguration, unter Windows `%APPDATA%\PlanoraCAD\languages`). Später gelesene Dateien
/// ersetzen frühere mit derselben Id.
pub fn language_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir.join(LANGUAGES_DIR));
    }
    if let Some(data_dir) = planora_project::config::Config::config_path().parent() {
        dirs.push(data_dir.join(LANGUAGES_DIR));
    }
    dirs
}

/// Protokollzeile (englisch) zu einer übergangenen Sprachdatei.
pub fn problem_line(problem: &LoadProblem) -> String {
    format!(
        "Language file skipped: {}: {}",
        problem.file.display(),
        problem.message
    )
}

/// Lädt die Sprachdateien aus `dirs` (fehlende Ordner sind kein Problem). Jede übergangene Datei
/// kommt als Warnung ins Protokoll; die Zeilen werden auch zurückgegeben.
pub fn load_languages(dirs: &[PathBuf]) -> Vec<String> {
    let mut lines = Vec::new();
    for dir in dirs {
        for problem in planora_i18n::load_dir(dir) {
            let line = problem_line(&problem);
            crate::diag::warn(&line);
            lines.push(line);
        }
    }
    lines
}

/// Beim Start: Sprachdateien laden und die Sprache setzen (gespeicherte Wahl, sonst die erste
/// passende Sprache des Systems, sonst Englisch). Nur aus `main`.
pub fn apply_at_start(saved: Option<&str>) {
    load_languages(&language_dirs());
    let system = system_languages();
    let lang = planora_i18n::pick(saved, &system);
    crate::diag::info(&format!(
        "Language: {} (saved: {}, system: {})",
        lang.id(),
        saved.unwrap_or("none"),
        if system.is_empty() {
            "none".to_string()
        } else {
            system.join(", ")
        }
    ));
    planora_i18n::set_global(lang);
}
