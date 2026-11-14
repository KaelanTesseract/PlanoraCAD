//! Gibt es schon eine Installation? Der Installer schreibt unter `Software\PlanoraCAD` den
//! Ordner (`InstallPath`), in HKCU bei „nur für mich“, in HKLM bei „für alle“.

use std::path::PathBuf;

/// Eine vorhandene Installation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Existing {
    /// `true` bei „für alle Benutzer“ (HKLM).
    pub all_users: bool,
    pub dir: PathBuf,
}

/// Wählt aus den beiden Registry-Werten; die Installation des aktuellen Benutzers (HKCU) gewinnt.
#[cfg_attr(
    not(any(windows, test)),
    expect(dead_code, reason = "nur unter Windows gebraucht")
)]
pub fn pick(hkcu: Option<PathBuf>, hklm: Option<PathBuf>) -> Option<Existing> {
    if let Some(dir) = hkcu {
        Some(Existing {
            all_users: false,
            dir,
        })
    } else {
        hklm.map(|dir| Existing {
            all_users: true,
            dir,
        })
    }
}

/// Liest die vorhandene Installation aus der Registry (HKCU, dann HKLM). Außerhalb von Windows
/// gibt es keine: `None`.
pub fn detect() -> Option<Existing> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        pick(
            win::read_path(HKEY_CURRENT_USER),
            win::read_path(HKEY_LOCAL_MACHINE),
        )
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod win {
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{HKEY, RRF_RT_REG_SZ, RegGetValueW};

    /// Registry-Schlüssel des Installers (unter HKCU bzw. HKLM), wie in der App.
    const REG_SUBKEY: &str = "Software\\PlanoraCAD";
    /// Wert mit dem Installationsordner, wie in der App.
    const REG_PATH: &str = "InstallPath";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Liest `InstallPath` unter `root`; nur lesend, `None` ohne Eintrag oder bei leerem Wert.
    pub(super) fn read_path(root: HKEY) -> Option<PathBuf> {
        let subkey = wide(REG_SUBKEY);
        let value = wide(REG_PATH);
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
                    let s = String::from_utf16_lossy(&text[..end]);
                    return (!s.trim().is_empty()).then(|| PathBuf::from(s));
                }
                ERROR_MORE_DATA => buf = vec![0; (bytes as usize).div_ceil(2) + 1],
                _ => return None,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_install_prefers_current_user() {
        let user = PathBuf::from(r"C:\Users\x\AppData\Local\Programs\PlanoraCAD");
        let all = PathBuf::from(r"C:\Program Files\PlanoraCAD");
        let both = pick(Some(user.clone()), Some(all.clone())).unwrap();
        assert!(!both.all_users);
        assert_eq!(both.dir, user);
        let only_all = pick(None, Some(all.clone())).unwrap();
        assert!(only_all.all_users);
        assert_eq!(only_all.dir, all);
        assert_eq!(pick(None, None), None);
    }
}
