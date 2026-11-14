//! Kommandozeile der Hülle: Rohtext der Argumente, Erkennung der stillen Weitergabe und die
//! Parameter, mit denen die Oberfläche den Inno-Setup-Motor startet.

use std::path::PathBuf;

/// Argumente des eigenen Aufrufs ohne Programmnamen, je Argument der Rohtext mit allen
/// Anführungszeichen. Unter Windows aus `GetCommandLineW`, damit die stille Weitergabe dem Motor
/// genau den Text gibt, den der Aufrufer (z. B. das Update der App) übergeben hat.
pub fn raw_args() -> Vec<String> {
    let mut tokens = split_command_line(&command_line());
    if !tokens.is_empty() {
        // Erster Eintrag ist der Programmname (ggf. selbst in Anführungszeichen).
        tokens.remove(0);
    }
    tokens
}

#[cfg(windows)]
fn command_line() -> String {
    use windows_sys::Win32::System::Environment::GetCommandLineW;
    // SAFETY: GetCommandLineW liefert einen Zeiger auf die nullterminierte Kommandozeile des
    // Prozesses, der bis Prozessende gültig bleibt; gelesen wird nur bis zur Null.
    unsafe {
        let ptr = GetCommandLineW();
        if ptr.is_null() {
            return String::new();
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
    }
}

/// Außerhalb von Windows gibt es keine Roh-Kommandozeile; die Argumente werden nur wieder
/// zusammengefügt (reicht zum Bauen und Testen).
#[cfg(not(windows))]
fn command_line() -> String {
    std::env::args().collect::<Vec<_>>().join(" ")
}

/// Zerlegt eine Kommandozeile an Leerraum (Leerzeichen, Tab) außerhalb von Anführungszeichen.
/// Anführungszeichen bleiben im Eintrag stehen, damit z. B. `/DIR="C:\Program Files\X"` als ein
/// Eintrag unverändert weitergegeben wird.
pub fn split_command_line(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                current.push(ch);
            }
            ' ' | '\t' if !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Stille Weitergabe, sobald irgendein Argument mit `/` beginnt.
pub fn is_passthrough(args: &[String]) -> bool {
    args.iter().any(|a| a.starts_with('/'))
}

/// Auswahl aus der Oberfläche, die der Motor als Parameter bekommt.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "die Oberfläche (Task 6) füllt die Auswahl")
)]
pub struct EngineChoice {
    pub all_users: bool,
    pub dir: PathBuf,
    pub desktop_icon: bool,
    pub lang: &'static str,
    pub progress_file: PathBuf,
    pub log_file: PathBuf,
}

/// Parameter für den stillen Lauf des Motors mit der Auswahl `c`; Pfade stehen immer in
/// Anführungszeichen.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "die Oberfläche (Task 6) startet damit den Motor")
)]
pub fn engine_args(c: &EngineChoice) -> Vec<String> {
    let quoted = |key: &str, path: &PathBuf| format!("/{key}=\"{}\"", path.display());
    vec![
        "/VERYSILENT".into(),
        "/SUPPRESSMSGBOXES".into(),
        "/NORESTART".into(),
        "/CLOSEAPPLICATIONS".into(),
        if c.all_users {
            "/ALLUSERS"
        } else {
            "/CURRENTUSER"
        }
        .into(),
        quoted("DIR", &c.dir),
        if c.desktop_icon {
            "/MERGETASKS=desktopicon"
        } else {
            "/MERGETASKS=!desktopicon"
        }
        .into(),
        format!("/LANG={}", c.lang),
        quoted("PROGRESSFILE", &c.progress_file),
        quoted("LOG", &c.log_file),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn passthrough_detects_any_slash_arg() {
        assert!(is_passthrough(&strings(&["/VERYSILENT"])));
        assert!(is_passthrough(&strings(&["egal", "/LOG=x"])));
        assert!(!is_passthrough(&[]));
        assert!(!is_passthrough(&strings(&["kein-schalter"])));
    }

    #[test]
    fn passthrough_keeps_quoted_args_verbatim() {
        let line = r#"prog.exe /VERYSILENT /DIR="C:\Program Files\PlanoraCAD" /LOG="C:\Users\Max Muster\a.log""#;
        assert_eq!(
            split_command_line(line),
            strings(&[
                "prog.exe",
                "/VERYSILENT",
                r#"/DIR="C:\Program Files\PlanoraCAD""#,
                r#"/LOG="C:\Users\Max Muster\a.log""#,
            ])
        );
    }

    #[test]
    fn split_handles_quoted_program_and_extra_whitespace() {
        let line = "\"C:\\Program Files\\x.exe\"   /A\t/B=\"1 2\"  ";
        assert_eq!(
            split_command_line(line),
            strings(&[r#""C:\Program Files\x.exe""#, "/A", r#"/B="1 2""#])
        );
        assert!(split_command_line("   ").is_empty());
    }

    fn choice(all_users: bool, desktop_icon: bool, lang: &'static str) -> EngineChoice {
        EngineChoice {
            all_users,
            dir: PathBuf::from(r"C:\Program Files\PlanoraCAD"),
            desktop_icon,
            lang,
            progress_file: PathBuf::from(r"C:\Temp\x y\progress.txt"),
            log_file: PathBuf::from(r"C:\Temp\x y\setup.log"),
        }
    }

    #[test]
    fn engine_args_for_current_user() {
        assert_eq!(
            engine_args(&choice(false, true, "de")),
            strings(&[
                "/VERYSILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/CLOSEAPPLICATIONS",
                "/CURRENTUSER",
                r#"/DIR="C:\Program Files\PlanoraCAD""#,
                "/MERGETASKS=desktopicon",
                "/LANG=de",
                r#"/PROGRESSFILE="C:\Temp\x y\progress.txt""#,
                r#"/LOG="C:\Temp\x y\setup.log""#,
            ])
        );
    }

    #[test]
    fn engine_args_for_all_users_without_icon() {
        assert_eq!(
            engine_args(&choice(true, false, "en")),
            strings(&[
                "/VERYSILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/CLOSEAPPLICATIONS",
                "/ALLUSERS",
                r#"/DIR="C:\Program Files\PlanoraCAD""#,
                "/MERGETASKS=!desktopicon",
                "/LANG=en",
                r#"/PROGRESSFILE="C:\Temp\x y\progress.txt""#,
                r#"/LOG="C:\Temp\x y\setup.log""#,
            ])
        );
    }
}
