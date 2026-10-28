//! Einstellungen, Updates und „Über“ auf Englisch (Sprachplan, Task 5): unter
//! `with_lang(&english(), …)` gezeichnet, ohne Fenster. Kein Test ruft `set_language` oder
//! `set_global` (Ruling 4): die Sprache gilt nur für den Thread des Tests.
//!
//! Die Dateiauswahl im Fenster „Einstellungen“ gehört zu einem späteren Task; sie wird nur
//! gebraucht, um eine Meldung auszulösen, und nicht auf Deutsch geprüft.

mod common;

use common::{Screen, Seen, assert_english, assert_has, join};
use planora_app::build_info::{BuildInfo, GpuInfo};
use planora_app::settings_window::{self, CheckHint, SettingsWindow, change_root};
use planora_app::update_dialog::{self, AfterUpdateAction, InstallView, UpdateDialog};
use planora_app::update_install::{self, AfterUpdate, InstallState, Installer, NoDownload};
use planora_app::workspace::Workspace;
use planora_app::{about_dialog, build_info, running_version};
use planora_i18n::{english, german, with_lang};
use planora_project::config::Config;
use planora_project::library::{Library, PROJECT_FILE};
use planora_update::Version;
use planora_update::install::{DownloadPaths, InstallError};
use planora_update::{AssetRef, Release, Update};
use std::path::PathBuf;
use std::sync::Arc;

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn has_start(seen: &[Seen], start: &str) -> bool {
    seen.iter().any(|s| s.text.starts_with(start))
}

fn assert_starts(what: &str, seen: &[Seen], start: &str) {
    assert!(
        has_start(seen, start),
        "{what}: „{start} …“ fehlt in {}",
        join(seen)
    );
}

/// Klick auf die Mitte des sichtbaren Textes `text`.
fn click_text(screen: &mut Screen, seen: &[Seen], text: &str, f: &mut dyn FnMut(&egui::Context)) {
    let s = seen
        .iter()
        .find(|s| s.text == text && s.clip.contains(s.rect.center()))
        .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar in {}", join(seen)));
    screen.click(s.rect.center(), f);
}

/// Das Fenster „Einstellungen“ mit Arbeitsbereich und Bildschirm.
struct Win {
    screen: Screen,
    w: SettingsWindow,
    ws: Workspace,
    root: PathBuf,
}

impl Win {
    fn new(prefix: &str) -> Win {
        let tmp = common::temp_root(prefix);
        std::fs::create_dir_all(&tmp).unwrap();
        let root = tmp.join("Projekte");
        let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        let cfg = tmp.join("config.json");
        assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
        ws.config_path = cfg;
        let win = Win {
            screen: Screen::new(1920.0, 1080.0),
            w: SettingsWindow::default(),
            ws,
            root,
        };
        win.screen.ctx.set_fonts(planora_app::fonts::definitions());
        planora_app::theme::apply(&win.screen.ctx, true);
        win
    }

    /// Wie `new`, aber das Schreiben der Konfiguration scheitert.
    fn blocked(prefix: &str) -> Win {
        let mut win = Win::new(prefix);
        let blocker = win.root.parent().unwrap().join("blockiert");
        std::fs::write(&blocker, b"x").unwrap();
        win.ws.config_path = blocker.join("config.json");
        win
    }

    fn settle(&mut self, n: usize) -> Vec<Seen> {
        let (w, ws) = (&mut self.w, &mut self.ws);
        self.screen.settle(n, &mut |ctx| {
            w.show_in(ctx, ws);
        })
    }

    fn click(&mut self, text: &str) -> Vec<Seen> {
        let seen = self.settle(4);
        let (w, ws) = (&mut self.w, &mut self.ws);
        click_text(&mut self.screen, &seen, text, &mut |ctx| {
            w.show_in(ctx, ws);
        });
        self.settle(4)
    }
}

#[test]
fn settings_speaks_english() {
    with_lang(&english(), || {
        let mut win = Win::new("planora-settings-english");
        win.w.open(false);
        let seen = win.settle(6);
        // Alle Abschnitte mit allen festen Texten.
        assert_has(
            "Einstellungen",
            &seen,
            &[
                "Settings",
                // Darstellung
                "Appearance",
                "Panel transparency",
                "Language",
                "English",
                // Updates
                "Updates",
                "Update channel",
                "Dev",
                "Check for updates on start",
                "When checking, the program asks GitHub, which can see your IP address.",
                "Detailed log",
                "Writes more to the log file on your computer, for example every command and how long it takes. Nothing of it is sent anywhere.",
                "Check for updates now",
                "Last check: never",
                // Projekte
                "Projects",
                "Project folder",
                "Change project folder …",
                "Your projects stay where they are. The new folder only shows the projects that are in it.",
                // Tastenkürzel
                "Keyboard shortcuts",
                // Fußzeile
                "Close",
            ],
        );
        assert_starts(
            "Einstellungen",
            &seen,
            &format!(
                "Running version: {} ({}",
                running_version(),
                running_version().channel().label()
            ),
        );
        assert_english("Einstellungen", &seen, &[], &[]);

        // Letzte Prüfung mit Zeitpunkt.
        win.ws.set_last_update_check(1_700_000_000);
        let seen = win.settle(3);
        assert_starts("Letzte Prüfung", &seen, "Last check: ");
        assert!(!has_start(&seen, "Last check: never"));
        assert_english("Letzte Prüfung", &seen, &[], &[]);

        // Alle Zustände der Suche neben dem Knopf.
        let found = v("99.0.0-dev.2");
        let hints = [
            (CheckHint::Running, vec!["Checking …"]),
            (CheckHint::UpToDate, vec!["You have the latest version."]),
            (
                CheckHint::Found(found),
                vec!["New version found: 99.0.0-dev.2", "Show notice"],
            ),
            (CheckHint::Failed, vec!["The check failed (no network?)."]),
            (
                CheckHint::Unsupported,
                vec!["Updating through the program is not available on this system."],
            ),
        ];
        for (hint, wanted) in hints {
            win.w.set_check_hint(Some(hint.clone()));
            let seen = win.settle(3);
            assert_has(&format!("Suche {hint:?}"), &seen, &wanted);
            assert_english(&format!("Suche {hint:?}"), &seen, &[], &[]);
        }

        // Übersprungene Version.
        win.w.set_check_hint(None);
        assert!(win.ws.set_skipped_version(Some(v("99.0.0-dev.3"))));
        let seen = win.settle(3);
        assert_has(
            "Übersprungen",
            &seen,
            &["Skipped: version 99.0.0-dev.3", "Show again"],
        );
        assert_english("Übersprungen", &seen, &[], &[]);
    });
}

#[test]
fn settings_messages_speak_english() {
    with_lang(&english(), || {
        // Durchsichtigkeit lässt sich nicht speichern.
        let mut win = Win::blocked("planora-settings-english-transparency");
        win.w.open(false);
        win.settle(3);
        win.w.preview_transparency(&mut win.ws, 55);
        let seen = win.settle(3);
        assert_has(
            "Durchsichtigkeit",
            &seen,
            &[
                "The transparency only applies until you close the program: the setting could not be saved.",
            ],
        );
        assert_english("Durchsichtigkeit", &seen, &[], &[]);

        // Update-Einstellung lässt sich nicht speichern.
        let mut win = Win::blocked("planora-settings-english-updates");
        win.w.open(false);
        let seen = win.click("Check for updates on start");
        assert_has(
            "Update-Einstellung",
            &seen,
            &[
                "The update setting only applies until you close the program: it could not be saved.",
            ],
        );
        assert_english("Update-Einstellung", &seen, &[], &[]);

        // Meldungen zum Projektverzeichnis (Wortlaut; Auslöser sind unten und in change_root).
        assert_eq!(
            settings_window::verbose_not_saved(),
            "The detailed log only applies until you close the program: the setting could not be saved."
        );
        assert_eq!(
            settings_window::not_saved(),
            "The new project folder only applies until you close the program: the setting could not be saved."
        );
        assert_eq!(
            settings_window::language_not_saved(),
            "The language only applies until you close the program: the setting could not be saved."
        );
        assert_eq!(settings_window::unreadable(), "The folder cannot be read.");
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
        ] {
            let text = planora_app::file_chooser::io_message(kind);
            assert!(
                !text.chars().any(|c| "äöüßÄÖÜ„".contains(c)) && !common::looks_like_key(&text),
                "{text}"
            );
        }

        // Ein Ordner, der selbst ein Projekt ist: Meldung im Fenster.
        let mut win = Win::new("planora-settings-english-nested");
        let sub = win.root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join(PROJECT_FILE), b"{}").unwrap();
        win.w.open(false);
        win.settle(3);
        win.w.start_choosing(&win.ws);
        win.w.chooser_mut().unwrap().enter("sub");
        let seen = win.settle(4);
        // Der Knopf zum Wählen steht in der Fußzeile der Dateiauswahl.
        let choose = seen
            .iter()
            // Deutsch, solange die Dateiauswahl nicht übersetzt ist (späterer Task).
            .find(|s| s.text == "Diesen Ordner wählen" || s.text == "Choose this folder")
            .map(|s| s.text.clone());
        let Some(choose) = choose else {
            panic!("Knopf zum Wählen fehlt in {}", join(&seen));
        };
        let seen = win.click(&choose);
        assert_has(
            "Projektordner",
            &seen,
            &[
                "This is already a project or document. Please choose the folder that contains your projects.",
            ],
        );

        // Wechsel bei offenen gespeicherten Dokumenten und in einen Projektordner.
        assert_eq!(
            change_root(&mut win.ws, &sub),
            Err(settings_window::RootMessage::Error(
                settings_window::nested()
            ))
        );
        assert_eq!(
            planora_app::workspace::root_locked(),
            "Please close all saved documents first, before the project folder changes."
        );
    });
}

fn update(truncated: bool) -> Update {
    let asset = |name: &str| AssetRef {
        name: name.into(),
        url: format!(
            "https://github.com/{}/releases/download/v99.0.0-dev.2/{name}",
            planora_update::REPO
        ),
        size: 10,
    };
    Update {
        latest: Release {
            version: v("99.0.0-dev.2"),
            tag: "v99.0.0-dev.2".into(),
            notes: "- Faster".into(),
            setup: asset(planora_update::SETUP_NAME),
            sha256: asset(planora_update::SHA256_NAME),
            licenses: None,
        },
        notes_between: vec![
            (v("99.0.0-dev.2"), "- Faster".into()),
            (v("99.0.0-dev.1"), String::new()),
        ],
        truncated,
    }
}

/// Zeichnet die Meldung „Neue Version verfügbar“ in einem Zustand.
fn draw_notice(
    ws: &mut Workspace,
    state: &mut UpdateDialog,
    view: InstallView<'_>,
    installable: bool,
) -> Vec<Seen> {
    let mut screen = Screen::new(1920.0, 1080.0);
    screen.ctx.set_fonts(planora_app::fonts::definitions());
    planora_app::theme::apply(&screen.ctx, true);
    screen.settle(5, &mut |ctx| {
        update_dialog::show(ctx, ws, state, view, installable);
    })
}

#[test]
fn updates_speak_english() {
    with_lang(&english(), || {
        let tmp = common::temp_root("planora-updates-english");
        std::fs::create_dir_all(&tmp).unwrap();
        let mut ws = Workspace::new(None, Config::default());
        ws.config_path = tmp.join("config.json");
        let mut state = UpdateDialog::default();
        state.open_with(update(true));

        // Angebot.
        let seen = draw_notice(&mut ws, &mut state, InstallView::Preparing, true);
        assert_has(
            "Angebot",
            &seen,
            &[
                "New version available: 99.0.0-dev.2 (Dev)",
                "Version 99.0.0-dev.2",
                "Version 99.0.0-dev.1",
                "No description.",
                "… and more changes on GitHub.",
                "Update now",
                "Later",
                "Skip this version",
            ],
        );
        assert_starts("Angebot", &seen, "You have version ");
        assert_english("Angebot", &seen, &[], &[]);

        // Nicht installiert: nur die Download-Seite.
        let seen = draw_notice(&mut ws, &mut state, InstallView::Preparing, false);
        assert_has(
            "Nicht installiert",
            &seen,
            &[
                "This copy of PlanoraCAD is not installed and cannot update itself. Download the new version from the download page.",
                "Open download page",
                "Later",
                "Skip this version",
            ],
        );
        assert_english("Nicht installiert", &seen, &[], &[]);

        // Während des Updates.
        state.installing = true;
        let progress = [
            (
                "Vorbereiten",
                InstallView::Preparing,
                vec!["Preparing the update …"],
            ),
            (
                "Download",
                InstallView::Downloading {
                    done: 50,
                    total: 100,
                },
                vec!["Downloading … 50 %", "Cancel"],
            ),
            (
                "Download ohne Größe",
                InstallView::Downloading {
                    done: 3 * 1024 * 1024 + 400 * 1024,
                    total: 0,
                },
                vec!["Downloading … 3.4 MB", "Cancel"],
            ),
            (
                "Bereit",
                InstallView::Waiting,
                vec!["The update is ready and waits until you are done."],
            ),
            ("Neustart", InstallView::Restarting, vec![]),
        ];
        for (what, view, wanted) in progress {
            let seen = draw_notice(&mut ws, &mut state, view, true);
            assert_has(what, &seen, &wanted);
            assert_english(what, &seen, &[], &[]);
        }

        // Fehler: Titel, Meldung, Knöpfe.
        let message = update_install::install_error_text(&InstallError::Checksum);
        let seen = draw_notice(
            &mut ws,
            &mut state,
            InstallView::Failed {
                message: &message,
                retry: true,
            },
            true,
        );
        assert_has(
            "Fehler",
            &seen,
            &[
                "Update failed",
                "The downloaded file is damaged (the checksum does not match). Nothing was installed.",
                "Try again",
                "Open download page",
                "Close",
            ],
        );
        assert_english("Fehler", &seen, &[], &[]);
        let seen = draw_notice(
            &mut ws,
            &mut state,
            InstallView::Failed {
                message: &update_install::only_windows(),
                retry: false,
            },
            true,
        );
        assert_has(
            "Fehler ohne Wiederholen",
            &seen,
            &[
                "Automatic updating is only available on Windows.",
                "Open download page",
                "Close",
            ],
        );
        assert!(!seen.iter().any(|s| s.text == "Try again"));
        assert_english("Fehler ohne Wiederholen", &seen, &[], &[]);
    });
}

#[test]
fn update_messages_speak_english() {
    with_lang(&english(), || {
        let text = [
            (
                update_install::install_error_text(&InstallError::Cancelled),
                "The update was cancelled.",
            ),
            (
                update_install::install_error_text(&InstallError::Network("offline".into())),
                "The new version could not be downloaded (offline). Please try again later.",
            ),
            (
                update_install::install_error_text(&InstallError::TooLarge),
                "The downloaded file is larger than expected. Nothing was installed.",
            ),
            (
                update_install::install_error_text(&InstallError::Checksum),
                "The downloaded file is damaged (the checksum does not match). Nothing was installed.",
            ),
            (
                update_install::install_error_text(&InstallError::Io("disk full".into())),
                "The update could not be prepared (disk full). The program keeps running in the current version.",
            ),
            (
                update_install::launch_failed("access denied"),
                "The update could not be started (access denied). You keep working in the current version.",
            ),
            (
                update_install::update_cancelled(),
                "Update cancelled. Without permission from Windows, PlanoraCAD cannot be updated for all users. You keep working in the current version.",
            ),
            (
                update_install::pending_not_saved(),
                "The update could not be scheduled because the settings cannot be saved. Nothing was installed.",
            ),
            (
                update_install::still_stopping(),
                "The cancelled download is still ending. Please try again in a moment.",
            ),
            (
                update_install::restart_box(),
                "PlanoraCAD is closing for the update and will start again by itself.",
            ),
            (update_install::restarting(), "Restarting …"),
            (update_install::failed_short(), "Update failed"),
            (update_install::open_download_page(), "Open download page"),
            (
                update_install::updated_title(&v("99.0.0-dev.2")),
                "Updated to version 99.0.0-dev.2",
            ),
            (
                update_dialog::status_hint(&v("99.0.0-dev.2")),
                "Update available: 99.0.0-dev.2",
            ),
            (
                update_install::status_text(
                    &InstallState::Downloading { done: 1, total: 4 },
                    false,
                ),
                "Downloading … 25 %",
            ),
            (
                update_install::status_text(&InstallState::ReadyToRestart, false),
                "The update is ready and waits until you are done.",
            ),
            (
                update_install::status_text(&InstallState::Failed("x".into()), false),
                "Update failed",
            ),
            (
                update_install::status_text(&InstallState::Idle, false),
                "Preparing the update …",
            ),
            (
                update_install::status_text(&InstallState::Idle, true),
                "Restarting …",
            ),
        ];
        for (shown, wanted) in text {
            assert_eq!(shown, wanted);
        }

        // Ein Download, der gar nicht erst beginnt: Fehlermeldung in der Sprache des Fensters.
        let tmp = common::temp_root("planora-updates-english-install");
        let mut up = update(false);
        up.latest.setup.url = "https://example.com/setup.exe".into();
        let mut installer = Installer::default();
        installer.start(&up, DownloadPaths::in_dir(&tmp), Arc::new(NoDownload));
        let InstallState::Failed(message) = installer.state() else {
            panic!("Fehler erwartet: {:?}", installer.state());
        };
        assert_eq!(
            message,
            "The new version could not be downloaded (Address does not belong to the release). Please try again later."
        );

        // Derselbe Fehler auf Deutsch bleibt wie vorher.
        let de = with_lang(&german(), || {
            update_install::install_error_text(&InstallError::Network(
                "Adresse gehört nicht zum Release".into(),
            ))
        });
        assert_eq!(
            de,
            InstallError::Network("Adresse gehört nicht zum Release".into()).message()
        );
    });
}

#[test]
fn after_update_notice_speaks_english() {
    with_lang(&english(), || {
        let mut screen = Screen::new(1920.0, 1080.0);
        screen.ctx.set_fonts(planora_app::fonts::definitions());
        planora_app::theme::apply(&screen.ctx, true);
        // Mit Patchnotes.
        let mut after = Some(AfterUpdate {
            version: v("99.0.0-dev.2"),
            notes: Some("- Faster".into()),
        });
        let mut action = None;
        let seen = screen.settle(5, &mut |ctx| {
            action = update_dialog::show_after_update(ctx, &mut after);
        });
        assert_has(
            "Nach dem Update",
            &seen,
            &[
                "Updated to version 99.0.0-dev.2",
                "Something wrong?",
                "OK",
                "Send feedback",
            ],
        );
        assert_english("Nach dem Update", &seen, &[], &[]);
        assert_eq!(action, None::<AfterUpdateAction>);
        // Ohne Patchnotes.
        let mut after = Some(AfterUpdate {
            version: v("99.0.0-dev.2"),
            notes: None,
        });
        let seen = screen.settle(5, &mut |ctx| {
            update_dialog::show_after_update(ctx, &mut after);
        });
        assert_has(
            "Nach dem Update ohne Notizen",
            &seen,
            &["See GitHub for what is new."],
        );
        assert_english("Nach dem Update ohne Notizen", &seen, &[], &[]);
    });
}

#[test]
fn about_speaks_english() {
    with_lang(&english(), || {
        let mut screen = Screen::new(1920.0, 1080.0);
        screen.ctx.set_fonts(planora_app::fonts::definitions());
        planora_app::theme::apply(&screen.ctx, true);
        let mut open = true;
        let seen = screen.settle(5, &mut |ctx| about_dialog::show(ctx, &mut open));
        assert_has(
            "Über",
            &seen,
            &[
                "About PlanoraCAD",
                "PlanoraCAD",
                "License: GPL-3.0-only",
                "Bundled fonts and icons",
                "Font Montserrat (SIL Open Font License)",
                "Icons Phosphor Icons (MIT)",
                "Font Roboto (Apache-2.0)",
                "Logo font Rajdhani (SIL Open Font License)",
            ],
        );
        assert_starts("Über", &seen, "Version ");
        assert_starts("Über", &seen, "System: ");
        assert!(
            has_start(&seen, "Graphics card"),
            "Grafikkarte fehlt in {}",
            join(&seen)
        );
        assert_english("Über", &seen, &[], &[]);

        // Grafikkarte: mit und ohne Angaben.
        let info = BuildInfo {
            version: running_version(),
            os: "Test".into(),
            gpu: None,
        };
        assert_eq!(info.gpu_line(), "Graphics card unknown");
        assert_eq!(info.system_line(), "System: Test");
        assert_eq!(about_dialog::gpu_text(&info), "Graphics card unknown");
        let info = BuildInfo {
            gpu: Some(GpuInfo {
                name: "Testcard".into(),
                driver: "1.2.3".into(),
                backend: "Vulkan".into(),
            }),
            ..info
        };
        assert_eq!(info.gpu_line(), "Testcard · Driver 1.2.3 · Vulkan");
        assert_eq!(
            about_dialog::gpu_text(&info),
            "Graphics card: Testcard · Driver 1.2.3 · Vulkan"
        );
        let mut adapter = eframe::egui_wgpu::wgpu::AdapterInfo::new(
            eframe::egui_wgpu::wgpu::DeviceType::DiscreteGpu,
            eframe::egui_wgpu::wgpu::Backend::Vulkan,
        );
        adapter.name = "Testcard".into();
        assert_eq!(build_info::gpu_from_adapter(&adapter).driver, "unknown");
    });
}

#[test]
fn settings_follow_the_language_while_open() {
    // Review-Fokus 5: bei offenem Fenster wechseln alle sichtbaren Texte im nächsten Bild.
    let mut win = Win::new("planora-settings-english-switch");
    win.w.open(false);
    let de = with_lang(&german(), || win.settle(6));
    assert_has(
        "deutsch",
        &de,
        &[
            "Einstellungen",
            "Darstellung",
            "Jetzt nach Updates suchen",
            "Letzte Prüfung: noch nie",
        ],
    );
    let en = with_lang(&english(), || win.settle(2));
    assert_has(
        "englisch",
        &en,
        &[
            "Settings",
            "Appearance",
            "Check for updates now",
            "Last check: never",
        ],
    );
    assert_english("Umschalten", &en, &[], &[]);
}
