//! Abfragen an Windows: Oberflächensprache, Standardordner, läuft PlanoraCAD im Zielordner.
//! Außerhalb von Windows liefern die Abfragen neutrale Werte, damit das Paket dort baut und
//! testet.

use std::path::{Path, PathBuf};

/// Dateiname der App.
#[cfg_attr(
    not(any(windows, test)),
    expect(dead_code, reason = "nur unter Windows gebraucht")
)]
const APP_EXE: &str = "PlanoraCAD.exe";

/// `"de"`, wenn die erste Sprache der Liste deutsch ist (`de`, `de-AT`, …), sonst `"en"`.
pub fn lang_from(list: &[String]) -> &'static str {
    let german = list.first().is_some_and(|l| {
        l.split('-')
            .next()
            .is_some_and(|p| p.eq_ignore_ascii_case("de"))
    });
    if german { "de" } else { "en" }
}

/// Oberflächensprache des Benutzers: `"de"` oder `"en"` (erste bevorzugte Windows-Sprache).
pub fn ui_lang() -> &'static str {
    lang_from(&preferred_languages())
}

/// Standardordner: `%LOCALAPPDATA%\Programs\PlanoraCAD` (nur für mich) bzw.
/// `%ProgramFiles%\PlanoraCAD` (für alle; wie Inno `{autopf}` im 64-Bit-Modus zuerst
/// `ProgramW6432`).
pub fn default_dir(all_users: bool) -> PathBuf {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    let base = if all_users {
        env("ProgramW6432")
            .or_else(|| env("ProgramFiles"))
            .map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from)
    } else {
        env("LOCALAPPDATA")
            .map_or_else(
                || PathBuf::from(r"C:\Users\Public\AppData\Local"),
                PathBuf::from,
            )
            .join("Programs")
    };
    base.join("PlanoraCAD")
}

/// Ist `proc` die `PlanoraCAD.exe` in `dir`? Ohne Groß/Klein, `/` und `\` gleich, ein
/// abschließender Trenner in `dir` zählt nicht.
#[cfg_attr(
    not(any(windows, test)),
    expect(dead_code, reason = "nur unter Windows gebraucht")
)]
pub fn exe_matches(proc: &Path, dir: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
    let dir = norm(dir);
    let want = format!("{}\\{}", dir.trim_end_matches('\\'), APP_EXE.to_lowercase());
    norm(proc) == want
}

/// Läuft gerade eine `PlanoraCAD.exe` aus `dir`? Nur lesend (Prozessliste); Prozesse, die sich
/// nicht öffnen lassen, zählen nicht.
#[cfg(windows)]
pub fn running_in(dir: &Path) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };

    // SAFETY: Reiner Aufruf mit Konstanten; das Ergebnis wird geprüft und unten geschlossen.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return false;
    }
    // SAFETY: PROCESSENTRY32W ist eine reine C-Struktur; Null ist ein gültiger Anfangszustand,
    // `dwSize` wird gleich gesetzt.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = false;
    // SAFETY: `snapshot` ist gültig, `entry` ist mit `dwSize` vorbereitet.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more && !found {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        if name.eq_ignore_ascii_case(APP_EXE) {
            // SAFETY: Öffnet nur mit der Abfragerecht; null heißt „nicht zugänglich“.
            let process =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID) };
            if !process.is_null() {
                let mut buf = vec![0u16; 32768];
                let mut size = buf.len() as u32;
                // SAFETY: `buf` bietet `size` Zeichen; die Funktion setzt `size` auf die Länge.
                let ok =
                    unsafe { QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut size) };
                if ok != 0 {
                    let path = PathBuf::from(String::from_utf16_lossy(
                        &buf[..(size as usize).min(buf.len())],
                    ));
                    found = exe_matches(&path, dir);
                }
                // SAFETY: `process` wurde oben geöffnet und wird nur hier geschlossen.
                unsafe { CloseHandle(process) };
            }
        }
        // SAFETY: wie beim ersten Aufruf.
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // SAFETY: `snapshot` wurde oben erzeugt und wird nur hier geschlossen.
    unsafe { CloseHandle(snapshot) };
    found
}

/// Außerhalb von Windows läuft keine `PlanoraCAD.exe`.
#[cfg(not(windows))]
pub fn running_in(dir: &Path) -> bool {
    let _ = dir;
    false
}

/// Bevorzugte Oberflächensprachen von Windows, wichtigste zuerst (`GetUserPreferredUILanguages`).
#[cfg(windows)]
fn preferred_languages() -> Vec<String> {
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

/// Außerhalb von Windows: keine Angabe, also Englisch.
#[cfg(not(windows))]
fn preferred_languages() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn lang_from_picks_de_only_for_german() {
        assert_eq!(lang_from(&s(&["de-AT"])), "de");
        assert_eq!(lang_from(&s(&["fr-FR", "de-DE"])), "en");
        assert_eq!(lang_from(&[]), "en");
        assert_eq!(lang_from(&s(&["dsb-DE"])), "en");
    }

    #[test]
    fn exe_matches_ignores_case() {
        let dir = Path::new(r"C:\Users\X\AppData\Local\Programs\PlanoraCAD");
        assert!(exe_matches(
            Path::new(r"c:\users\x\appdata\local\programs\planoracad\PLANORACAD.EXE"),
            dir
        ));
        assert!(exe_matches(
            Path::new(r"C:\Users\X\AppData\Local\Programs\PlanoraCAD\PlanoraCAD.exe"),
            Path::new(r"C:\Users\X\AppData\Local\Programs\PlanoraCAD\")
        ));
        assert!(!exe_matches(Path::new(r"C:\Other\PlanoraCAD.exe"), dir));
        assert!(!exe_matches(
            Path::new(r"C:\Users\X\AppData\Local\Programs\PlanoraCAD\sub\PlanoraCAD.exe"),
            dir
        ));
    }
}
