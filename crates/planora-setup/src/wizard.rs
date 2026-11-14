//! Zustandsautomat der fünf Seiten (ohne Oberfläche).

use crate::engine::RunError;
use crate::existing::Existing;
use crate::system::default_dir;
use std::path::{Path, PathBuf};

/// Name des App-Ordners, den Inno (`AppendDefaultDirName=yes`) an einen gewählten Ordner hängt.
const APP_FOLDER: &str = "PlanoraCAD";

/// Ordner aus „Durchsuchen“ wie im Inno-Assistenten: `\PlanoraCAD` wird angehängt, außer der
/// letzte Teil heißt schon so (ohne Rücksicht auf Groß- und Kleinschreibung). Arbeitet auf
/// Windows-Pfaden (`\` und `/`), auch außerhalb von Windows.
pub fn with_app_folder(picked: &Path) -> PathBuf {
    let text = picked.to_string_lossy();
    let trimmed = text.trim_end_matches(['\\', '/']);
    let last = trimmed.rsplit(['\\', '/']).next().unwrap_or("");
    if last.eq_ignore_ascii_case(APP_FOLDER) {
        return picked.to_path_buf();
    }
    PathBuf::from(format!("{trimmed}\\{APP_FOLDER}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Welcome,
    License,
    Folder,
    Installing,
    Done,
    Failed { log: PathBuf },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    None,
    AppRunning,
    UacDeclined,
    InvalidFolder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    Current,
    Open,
}

pub struct Wizard {
    pub page: Page,
    pub accepted: bool,
    pub all_users: bool,
    pub dir: String,
    /// Desktop-Symbol anlegen? Bei einer vorhandenen Installation (gesperrt) gilt deren frühere
    /// Wahl: Der Haken ist dann verborgen, der Motor bekommt keine Vorgabe dazu.
    pub desktop_icon: bool,
    pub start_app: bool,
    pub locked: bool,
    pub hint: Hint,
    pub existing: Option<Existing>,
}

impl Wizard {
    /// Vorbelegung: nur für mich, Standardordner, Desktop-Symbol und Starten an. Mit einer
    /// vorhandenen Installation gelten deren Modus und Ordner, gesperrt (Update am selben Ort).
    pub fn new(existing: Option<Existing>) -> Wizard {
        let (all_users, dir, locked) = match &existing {
            Some(e) => (e.all_users, e.dir.to_string_lossy().into_owned(), true),
            None => (
                false,
                default_dir(false).to_string_lossy().into_owned(),
                false,
            ),
        };
        Wizard {
            page: Page::Welcome,
            accepted: false,
            all_users,
            dir,
            desktop_icon: true,
            start_app: true,
            locked,
            hint: Hint::None,
            existing,
        }
    }

    /// Eine Seite weiter (Willkommen → Lizenz → Ordner), nur wenn `can_next`. Von der
    /// Ordner-Seite geht es mit `start_install` weiter, nicht hiermit.
    pub fn next(&mut self) {
        if !self.can_next() {
            return;
        }
        self.hint = Hint::None;
        match self.page {
            Page::Welcome => self.page = Page::License,
            Page::License => self.page = Page::Folder,
            _ => {}
        }
    }

    /// Eine Seite zurück (Lizenz → Willkommen, Ordner → Lizenz). Auf den übrigen Seiten, auch auf
    /// der Fehlerseite (nur „Protokoll öffnen“ und „Schließen“), geschieht nichts.
    pub fn back(&mut self) {
        match self.page {
            Page::License => self.page = Page::Welcome,
            Page::Folder => self.page = Page::License,
            _ => return,
        }
        self.hint = Hint::None;
    }

    /// Darf „Weiter“ gedrückt werden? Lizenz nur mit Zustimmung, Ordner nur mit gültigem Pfad.
    pub fn can_next(&self) -> bool {
        match self.page {
            Page::Welcome => true,
            Page::License => self.accepted,
            Page::Folder => self.folder_valid(),
            _ => false,
        }
    }

    /// Schaltet „für alle“ um; steht noch der Standardordner des alten Modus da, wandert er
    /// mit. Gesperrt (vorhandene Installation): keine Wirkung.
    pub fn set_all_users(&mut self, v: bool) {
        if self.locked {
            return;
        }
        if self.dir == default_dir(self.all_users).to_string_lossy() {
            self.dir = default_dir(v).to_string_lossy().into_owned();
        }
        self.all_users = v;
    }

    /// Setzt den Ordner aus dem Eingabefeld; gesperrt: keine Wirkung. Löscht den Hinweis auf einen
    /// ungültigen Ordner.
    pub fn set_dir(&mut self, dir: String) {
        if self.locked {
            return;
        }
        self.dir = dir;
        if self.hint == Hint::InvalidFolder {
            self.hint = Hint::None;
        }
    }

    /// Neue Prüfung, ob die App noch läuft: auf der Ordner-Seite nimmt sie den Hinweis
    /// `AppRunning` zurück, sobald sie beendet ist. Startet nie die Installation.
    pub fn recheck_running(&mut self, app_running: bool) {
        if self.page == Page::Folder && self.hint == Hint::AppRunning && !app_running {
            self.hint = Hint::None;
        }
    }

    /// Absoluter Pfad (`C:\…` oder `\\Server\…`), nicht leer, ohne `<>|"*?`.
    pub fn folder_valid(&self) -> bool {
        let dir = self.dir.as_str();
        if dir.trim() != dir {
            return false;
        }
        let b = dir.as_bytes();
        let absolute = (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/'))
            || (dir.starts_with(r"\\") && dir.len() > 2);
        absolute && !dir.contains(['<', '>', '|', '"', '*', '?'])
    }

    /// „Installieren“ auf der Ordner-Seite: ungültiger Ordner → `Hint::InvalidFolder`, läuft
    /// die App im Zielordner → `Hint::AppRunning`; jeweils bleibt es auf `Folder` und das Ergebnis
    /// ist `false`. Sonst geht es auf `Installing` (`true`).
    pub fn start_install(&mut self, app_running: bool) -> bool {
        if self.page != Page::Folder {
            return false;
        }
        if !self.folder_valid() {
            self.hint = Hint::InvalidFolder;
            return false;
        }
        if app_running {
            self.hint = Hint::AppRunning;
            return false;
        }
        self.hint = Hint::None;
        self.page = Page::Installing;
        true
    }

    /// Der Motor ist fertig: Exit-Code 0 → `Done`; Admin-Abfrage abgelehnt → zurück zum Ordner
    /// mit Hinweis; alles andere → `Failed` mit dem Protokoll.
    pub fn engine_finished(&mut self, result: Result<u32, RunError>, log: PathBuf) {
        match result {
            Ok(0) => {
                self.hint = Hint::None;
                self.page = Page::Done;
            }
            Err(RunError::Declined) => {
                self.hint = Hint::UacDeclined;
                self.page = Page::Folder;
            }
            Ok(_) | Err(RunError::Io(_)) => {
                self.hint = Hint::None;
                self.page = Page::Failed { log };
            }
        }
    }

    /// Vorgabe für das Desktop-Symbol an den Motor: `None` bei einer vorhandenen Installation
    /// (Inno behält dann die frühere Wahl), sonst der Haken.
    pub fn desktop_icon_choice(&self) -> Option<bool> {
        if self.locked {
            None
        } else {
            Some(self.desktop_icon)
        }
    }

    /// Die fünf Schritte der linken Leiste mit ihrem Zustand. Die Fehlerseite gehört zum Schritt
    /// „Installieren“.
    pub fn steps(&self) -> [(Page, StepState); 5] {
        let current = match self.page {
            Page::Welcome => 0,
            Page::License => 1,
            Page::Folder => 2,
            Page::Installing | Page::Failed { .. } => 3,
            Page::Done => 4,
        };
        let pages = [
            Page::Welcome,
            Page::License,
            Page::Folder,
            Page::Installing,
            Page::Done,
        ];
        let mut i = 0;
        pages.map(|page| {
            let state = match i.cmp(&current) {
                std::cmp::Ordering::Less => StepState::Done,
                std::cmp::Ordering::Equal => StepState::Current,
                std::cmp::Ordering::Greater => StepState::Open,
            };
            i += 1;
            (page, state)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn at_folder() -> Wizard {
        let mut w = Wizard::new(None);
        w.next();
        w.accepted = true;
        w.next();
        assert_eq!(w.page, Page::Folder);
        w
    }

    #[test]
    fn license_needs_acceptance() {
        let mut w = Wizard::new(None);
        assert_eq!(w.page, Page::Welcome);
        assert!(w.can_next());
        w.next();
        assert_eq!(w.page, Page::License);
        assert!(!w.can_next());
        w.next();
        assert_eq!(
            w.page,
            Page::License,
            "ohne Zustimmung geht es nicht weiter"
        );
        w.accepted = true;
        assert!(w.can_next());
        w.next();
        assert_eq!(w.page, Page::Folder);
        w.back();
        assert_eq!(w.page, Page::License);
    }

    #[test]
    fn invalid_folder_blocks_install() {
        for bad in [
            "", "relativ", "C:\\a<b", "C:\\a|b", "C:\\a\"b", "C:\\a*b", "C:\\a?b", " C:\\x",
            "C:\\x ",
        ] {
            let mut w = at_folder();
            w.dir = bad.to_string();
            assert!(!w.folder_valid(), "{bad:?}");
            assert!(!w.can_next(), "{bad:?}");
            assert!(!w.start_install(false), "{bad:?}");
            assert_eq!(w.page, Page::Folder);
            assert_eq!(w.hint, Hint::InvalidFolder);
        }
        let mut w = at_folder();
        w.dir = r"D:\Programme\PlanoraCAD".to_string();
        assert!(w.folder_valid());
    }

    #[test]
    fn switching_mode_moves_default_folder() {
        let mut w = Wizard::new(None);
        assert!(!w.all_users);
        assert_eq!(w.dir, default_dir(false).to_string_lossy());
        w.set_all_users(true);
        assert!(w.all_users);
        assert_eq!(w.dir, default_dir(true).to_string_lossy());
        w.set_all_users(false);
        assert_eq!(w.dir, default_dir(false).to_string_lossy());
        // Eigener Ordner bleibt beim Umschalten stehen.
        w.dir = r"D:\Eigen".to_string();
        w.set_all_users(true);
        assert_eq!(w.dir, r"D:\Eigen");
    }

    #[test]
    fn existing_install_locks_mode_and_folder() {
        let ex = Existing {
            all_users: true,
            dir: PathBuf::from(r"E:\Apps\PlanoraCAD"),
        };
        let mut w = Wizard::new(Some(ex));
        assert!(w.locked && w.all_users);
        assert_eq!(w.dir, r"E:\Apps\PlanoraCAD");
        w.set_all_users(false);
        assert!(w.all_users);
        assert_eq!(w.dir, r"E:\Apps\PlanoraCAD");
        assert!(w.desktop_icon && w.start_app);
    }

    #[test]
    fn update_keeps_earlier_desktop_icon_choice() {
        let mut w = Wizard::new(Some(Existing {
            all_users: false,
            dir: PathBuf::from(r"E:\A"),
        }));
        assert_eq!(w.desktop_icon_choice(), None);
        w.desktop_icon = false;
        assert_eq!(w.desktop_icon_choice(), None);
        let mut n = Wizard::new(None);
        assert_eq!(n.desktop_icon_choice(), Some(true));
        n.desktop_icon = false;
        assert_eq!(n.desktop_icon_choice(), Some(false));
    }

    #[test]
    fn browse_appends_app_folder_like_inno() {
        let case = |picked: &str| {
            with_app_folder(Path::new(picked))
                .to_string_lossy()
                .into_owned()
        };
        assert_eq!(case(r"D:\Programme"), r"D:\Programme\PlanoraCAD");
        assert_eq!(case(r"D:\x\planoracad"), r"D:\x\planoracad");
        assert_eq!(case(r"D:\x\PlanoraCAD"), r"D:\x\PlanoraCAD");
        assert_eq!(case(r"D:\"), r"D:\PlanoraCAD");
        assert_eq!(case(r"D:\Programme\"), r"D:\Programme\PlanoraCAD");
        assert_eq!(case(r"\\Server\Freigabe"), r"\\Server\Freigabe\PlanoraCAD");
        assert_eq!(case(r"D:\PlanoraCAD-alt"), r"D:\PlanoraCAD-alt\PlanoraCAD");
    }

    #[test]
    fn start_install_only_from_folder() {
        for page in [Page::Welcome, Page::License, Page::Installing, Page::Done] {
            let mut w = at_folder();
            w.page = page.clone();
            assert!(!w.start_install(false));
            assert_eq!(w.page, page);
        }
    }

    #[test]
    fn back_is_noop_on_failed_and_later_pages() {
        let mut w = at_folder();
        w.start_install(false);
        w.engine_finished(Ok(2), PathBuf::from("l"));
        let failed = w.page.clone();
        w.back();
        assert_eq!(w.page, failed);
        w.page = Page::Done;
        w.back();
        assert_eq!(w.page, Page::Done);
    }

    #[test]
    fn set_dir_respects_lock_and_clears_invalid_hint() {
        let mut w = at_folder();
        w.dir = String::new();
        assert!(!w.start_install(false));
        assert_eq!(w.hint, Hint::InvalidFolder);
        w.set_dir(r"D:\X".to_string());
        assert_eq!(w.dir, r"D:\X");
        assert_eq!(w.hint, Hint::None);
        let mut l = Wizard::new(Some(Existing {
            all_users: false,
            dir: PathBuf::from(r"E:\A"),
        }));
        l.set_dir(r"D:\Y".to_string());
        assert_eq!(l.dir, r"E:\A");
    }

    #[test]
    fn recheck_running_clears_only_app_running_hint() {
        let mut w = at_folder();
        assert!(!w.start_install(true));
        w.recheck_running(true);
        assert_eq!(w.hint, Hint::AppRunning);
        w.recheck_running(false);
        assert_eq!(w.hint, Hint::None);
        assert_eq!(w.page, Page::Folder);
        w.hint = Hint::UacDeclined;
        w.recheck_running(false);
        assert_eq!(w.hint, Hint::UacDeclined);
    }

    #[test]
    fn running_app_shows_hint_and_stays() {
        let mut w = at_folder();
        assert!(!w.start_install(true));
        assert_eq!(w.page, Page::Folder);
        assert_eq!(w.hint, Hint::AppRunning);
        assert!(w.start_install(false));
        assert_eq!(w.page, Page::Installing);
        assert_eq!(w.hint, Hint::None);
    }

    #[test]
    fn uac_declined_returns_to_folder_with_hint() {
        let mut w = at_folder();
        assert!(w.start_install(false));
        w.engine_finished(Err(RunError::Declined), PathBuf::from("log.txt"));
        assert_eq!(w.page, Page::Folder);
        assert_eq!(w.hint, Hint::UacDeclined);
    }

    #[test]
    fn engine_error_shows_failed_with_log() {
        let log = PathBuf::from(r"C:\Temp\setup.log");
        let mut w = at_folder();
        w.start_install(false);
        w.engine_finished(Ok(0), log.clone());
        assert_eq!(w.page, Page::Done);

        let mut w = at_folder();
        w.start_install(false);
        w.engine_finished(Ok(5), log.clone());
        assert_eq!(w.page, Page::Failed { log: log.clone() });

        let mut w = at_folder();
        w.start_install(false);
        w.engine_finished(Err(RunError::Io(io::Error::other("x"))), log.clone());
        assert_eq!(w.page, Page::Failed { log });
    }

    #[test]
    fn steps_mark_done_current_open() {
        use StepState::*;
        let states = |w: &Wizard| w.steps().map(|(_, s)| s);
        let mut w = Wizard::new(None);
        assert_eq!(states(&w), [Current, Open, Open, Open, Open]);
        w.next();
        assert_eq!(states(&w), [Done, Current, Open, Open, Open]);
        w.accepted = true;
        w.next();
        assert_eq!(states(&w), [Done, Done, Current, Open, Open]);
        w.start_install(false);
        assert_eq!(states(&w), [Done, Done, Done, Current, Open]);
        assert_eq!(w.steps()[3].0, Page::Installing);
        w.engine_finished(Ok(1), PathBuf::from("l"));
        assert_eq!(states(&w), [Done, Done, Done, Current, Open]);
        let mut w = at_folder();
        w.start_install(false);
        w.engine_finished(Ok(0), PathBuf::from("l"));
        assert_eq!(states(&w), [Done, Done, Done, Done, Current]);
    }
}
