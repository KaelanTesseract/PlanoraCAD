//! Wie ist die laufende exe installiert? Der Installer schreibt unter `Software\PlanoraCAD`
//! den Modus (`user` in HKCU, `all` in HKLM) und den Ordner. Ein Eintrag zählt nur, wenn sein
//! Ordner der Ordner der laufenden exe ist; so gilt eine Fassung aus dem Bauordner oder eine
//! zweite Installation nie fälschlich als „installiert“.

use std::path::Path;

/// Für wen PlanoraCAD installiert ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallMode {
    /// „Nur für mich“ (HKCU, ohne Admin).
    User,
    /// „Für alle“ (HKLM, mit Admin).
    AllUsers,
}

/// Registry-Schlüssel des Installers (unter HKCU bzw. HKLM).
pub const REG_SUBKEY: &str = "Software\\PlanoraCAD";
/// Wert mit dem Modus: `user` oder `all`.
pub const REG_MODE: &str = "InstallMode";
/// Wert mit dem Installationsordner.
pub const REG_PATH: &str = "InstallPath";

/// Ein gelesener Registry-Eintrag (beide Werte als Text).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegEntry {
    pub mode: String,
    pub path: String,
}

/// Wählt den Modus zur laufenden exe in `exe_dir`. HKCU zählt nur mit `user`, HKLM nur mit
/// `all`, jeweils nur für denselben Ordner; HKCU wird zuerst geprüft.
pub fn pick(exe_dir: &Path, hkcu: Option<RegEntry>, hklm: Option<RegEntry>) -> Option<InstallMode> {
    let fits = |e: &Option<RegEntry>, mode: &str| {
        e.as_ref()
            .is_some_and(|e| e.mode == mode && same_dir(exe_dir, Path::new(&e.path)))
    };
    if fits(&hkcu, "user") {
        Some(InstallMode::User)
    } else if fits(&hklm, "all") {
        Some(InstallMode::AllUsers)
    } else {
        None
    }
}

/// Derselbe Ordner? Erst über `canonicalize` (wenn es für beide gelingt), sonst als Text ohne
/// Präfix `\\?\` und abschließende Trenner, ohne Groß/Klein.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    if let (Ok(ca), Ok(cb)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return ca == cb || plain(&ca) == plain(&cb);
    }
    plain(a) == plain(b)
}

fn plain(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_lowercase()
}

/// Modus der laufenden exe `exe` aus der Registry (HKCU, dann HKLM). Ohne exe, außerhalb von
/// Windows oder ohne passenden Eintrag: `None` (nicht installiert).
pub fn detect(exe: Option<&Path>) -> Option<InstallMode> {
    let dir = exe?.parent()?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        pick(
            dir,
            win::read_entry(HKEY_CURRENT_USER),
            win::read_entry(HKEY_LOCAL_MACHINE),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        None
    }
}

#[cfg(windows)]
mod win {
    use super::{REG_MODE, REG_PATH, REG_SUBKEY, RegEntry};
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{HKEY, RRF_RT_REG_SZ, RegGetValueW};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub(super) fn read_entry(root: HKEY) -> Option<RegEntry> {
        Some(RegEntry {
            mode: read_string(root, REG_MODE)?,
            path: read_string(root, REG_PATH)?,
        })
    }

    fn read_string(root: HKEY, value: &str) -> Option<String> {
        let subkey = wide(REG_SUBKEY);
        let value = wide(value);
        let mut buf: Vec<u16> = vec![0; 260];
        for _ in 0..4 {
            let mut bytes = (buf.len() * 2) as u32;
            // SAFETY: Schlüssel und Wertname sind nullterminiert; `buf` bietet `bytes` Bytes;
            // RegGetValueW schreibt höchstens so viele und setzt `bytes` auf die wahre Länge.
            let rc = unsafe {
                RegGetValueW(
                    root,
                    subkey.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            match rc {
                ERROR_SUCCESS => {
                    let n = (bytes as usize / 2).min(buf.len());
                    let text = &buf[..n];
                    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
                    return Some(String::from_utf16_lossy(&text[..end]));
                }
                ERROR_MORE_DATA => buf = vec![0; (bytes as usize).div_ceil(2) + 1],
                _ => return None,
            }
        }
        None
    }
}
