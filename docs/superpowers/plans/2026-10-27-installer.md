# Installer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Das Release liefert einen normalen Windows-Installer `planoracad-setup-x64.exe` (nur für mich / für alle Benutzer, Deutsch/Englisch), und die Update-Funktion aktualisiert installierte Fassungen in beiden Modi über diesen Installer.

**Architecture:** Ein Inno-Setup-Skript `installer/planoracad.iss` und ein Bauskript `tools/build-installer.ps1` erzeugen Installer und Prüfsumme; `release.yml` ruft dasselbe Skript auf. `planora-update` kennt nur noch die Setup-Dateien und lädt sie in einen eigenen Temp-Ordner; Austausch, `.old` und Rückfall entfallen. Die App liest den Install-Modus aus der Registry (nur wenn der eingetragene Ordner ihr eigener ist), startet nach Rückfrage und zweiter Prüfsummen-Prüfung den Installer still per `ShellExecuteExW` (bei „alle Benutzer“ mit Verb `runas`, damit eine abgelehnte Admin-Abfrage sofort als Fehler zurückkommt) und beendet sich; Inno startet die neue Version über einen `[Run]`-Eintrag, der nur mit dem eigenen Parameter `/NACHUPDATE=1` greift. Die neue Version erkennt das Update ohne Startargument am vorgemerkten `update_pending` (Ziel = laufende Version) und zeigt wie bisher „Aktualisiert auf …“ samt Fensterlage.

**Tech Stack:** Rust (egui/eframe 0.36), `windows-sys` =0.61.2 (nur neue Features), Inno Setup 6.7.3 (Bau-Werkzeug), PowerShell 5.1/7, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-27-installer-design.md`

## Global Constraints

- Branch `feature/installer`; nie pushen, kein Tag, kein `gh release`. GitHub-CI (`ci.yml`, Trockenlauf `release.yml`) nur von Hand durch den Controller vor dem Release.
- Installer-Datei `planoracad-setup-x64.exe`, Prüfsumme `planoracad-setup-x64.sha256` (Inhalt `<64 hex>  planoracad-setup-x64.exe\n`, UTF-8 ohne BOM), Symbole `planoracad-windows-x64.pdb`, `THIRD-PARTY-LICENSES.txt` – genau diese vier Release-Dateien; keine Einzel-exe mehr.
- Ziele: „nur für mich“ `%LOCALAPPDATA%\Programs\PlanoraCAD` ohne Admin, „für alle“ `C:\Program Files\PlanoraCAD` mit Admin (`{autopf}\PlanoraCAD`); Herausgeber „Dennis Guse“; installierte Programmdatei `PlanoraCAD.exe`.
- Registry: `Software\PlanoraCAD`, Wert `InstallMode` = `user` (HKCU) bzw. `all` (HKLM), dazu `InstallPath` = `{app}` (Plan-Ergänzung, siehe Review Focus 2).
- Feste `AppId` `{D42C66D0-6AB0-4D5C-A693-9616105847B8}` (nie ändern, sonst erkennt Windows Updates nicht als dieselbe App).
- Stille Update-Parameter genau: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /CURRENTUSER|/ALLUSERS /NACHUPDATE=1` (ohne `/RESTARTAPPLICATIONS`, siehe Abweichungen).
- Daten bleiben bei der Deinstallation: `%APPDATA%\PlanoraCAD` und `Dokumente\PlanoraCAD` werden vom Installer nie angefasst.
- Texte in einfacher Sprache, Du-Form, kein KI-Ton; Oberflächentexte als `pub const`/`pub fn` im Modul, von Tests geprüft. Installer-Texte nur aus den Inno-Sprachdateien (`Default.isl`, `Languages\German.isl`).
- Keine neue Rust-Abhängigkeit. Erlaubt: zusätzliche Features für das vorhandene `windows-sys = "=0.61.2"`; danach `cargo deny check` grün mit unveränderter `deny.toml`.
- Prüfen lokal: Windows `cargo test -p <crate> --test <name>`, vor jedem Commit `cargo fmt --all --check` und `cargo clippy -p <crate> --all-targets -- -D warnings`; am Ende von Task 4 zusätzlich WSL `wsl -d Ubuntu -u root -- su - ci -c '. ~/.cargo/env; cd ~/PlanoraCAD && cargo test -p planora-update -p planora-app'` (Stand vorher in den WSL-Klon holen wie bei früheren Teilprojekten).
- Oberflächen bei 1920 × 1080 prüfen; keine Tests und keine Sichtprüfungen für kleine Fenster.
- Agenten starten kein Fenster, installieren nichts und bauen keine Release-Builds; Installieren/Deinstallieren auf Dennis' Rechner nur in Task 6 durch den Controller mit Dennis' Go.
- Repo-Adresse nur aus `planora_update::REPO`, Version nur aus `crate::running_version()`.
- Commits mit Zeile `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; nach jeder Aufgabe eine Notiz in `D:\Vault\Vault\Projekte\PlanoraCAD\` (Regel aus `CLAUDE.md` des Repos).

## Review Focus

1. **Admin-Abfrage abgelehnt oder Installer lässt sich nicht starten:** Die alte Version läuft weiter, zeigt „Update abgebrochen …“ bzw. die Startfehler-Meldung mit „Erneut versuchen“, `update_pending` und Patchnotes-Datei sind gelöscht, das Programm beendet sich nicht, die Version wird weiter angeboten (nicht übersprungen) (Tests in Task 4).
2. **Zwei Installationen oder ein Bauordner-Start neben einer Installation:** HKCU und HKLM haben beide einen Eintrag, oder `target\release\planora-app.exe` läuft auf einem Rechner mit installierter Fassung. Ein Modus gilt nur, wenn `InstallPath` dem Ordner der laufenden exe entspricht (Groß-/Kleinschreibung und abschließender `\` egal); sonst „nicht installiert“ – nie wird ein fremder Installationsordner aktualisiert (Tests in Task 2).
3. **Installer scheitert still oder der Nutzer startet danach die alte Version selbst:** Kein „Aktualisiert auf …“, `update_pending` wird gelöscht, ein liegengebliebener Setup-Download wird beim Start weggeräumt, die Version wird bei der nächsten Prüfung wieder angeboten (Tests in Task 4).
4. **Setup-Datei im Temp-Ordner zwischen Download und Start verändert** (jedes Programm des Benutzers kann dort schreiben; bei „alle Benutzer“ läuft sie danach mit Admin-Rechten): Die zweite Prüfung unmittelbar vor dem Start schlägt fehl, nichts wird gestartet, die Datei wird gelöscht (Test in Task 4).
5. **Neustart nach einem Update „für alle Benutzer“:** Die neue Version läuft ohne Admin-Rechte (über `explorer.exe` gestartet, weil der per `runas` gestartete Installer selbst erhöht ist) und öffnet in der Fensterlage der alten, obwohl Inno sie ohne Startargument startet (Test der Fensterlage in Task 4; Rechte von Hand in Task 6).

---

### Task 1: Installer-Skript, Bauskript, Skript-Test

**Files:**
- Create: `installer/planoracad.iss`, `tools/build-installer.ps1`
- Modify: `.gitignore` (`/dist/`)
- Test: `crates/planora-update/tests/installer_script.rs`

**Interfaces:**
- Produces: `tools\build-installer.ps1 [-Exe <Pfad>] [-Licenses <Pfad>] [-OutDir <Pfad>] [-Iscc <Pfad>]` – Vorgaben `target\release\planora-app.exe`, `dist\THIRD-PARTY-LICENSES.txt` (fehlt sie, erzeugt das Skript sie mit `tools\third-party-licenses.ps1 -Output`), `dist`. Ergebnis `<OutDir>\planoracad-setup-x64.exe` und `<OutDir>\planoracad-setup-x64.sha256`. Exit 1 mit Meldung `ABBRUCH: …`, wenn exe oder ISCC fehlt (Hinweis auf `cargo build --release --locked -p planora-app` bzw. Inno Setup 6).
- Produces (Task 2 und 4 prüfen dagegen): Registry-Namen `Software\PlanoraCAD`, `InstallMode`, `InstallPath`; Parameter `{param:NACHUPDATE|0}`; Programmdatei `{app}\PlanoraCAD.exe`.
- ISCC-Defines vom Skript: `AppVersion` (z. B. `0.2.0-dev.1`), `AppVersionNumeric` (`0.2.0`), `SourceExe`, `LicensesFile`; Ausgabe per `/O<OutDir>`.

- [ ] **Step 1: Test schreiben** (`installer_script.rs`, liest `installer/planoracad.iss` relativ zu `CARGO_MANIFEST_DIR/../..` wie `release_meta.rs`; Zeilen getrimmt, Vergleich ohne Groß-/Kleinschreibung der Schlüssel nur wo Inno sie ignoriert):
  - `setup_section_has_the_fixed_values`: Zeilen `AppId={{D42C66D0-6AB0-4D5C-A693-9616105847B8}`, `AppName=PlanoraCAD`, `AppVersion={#AppVersion}`, `VersionInfoVersion={#AppVersionNumeric}`, `AppPublisher=Dennis Guse`, `DefaultDirName={autopf}\PlanoraCAD`, `PrivilegesRequired=lowest`, `PrivilegesRequiredOverridesAllowed=commandline dialog`, `ArchitecturesAllowed=x64compatible`, `ArchitecturesInstallIn64BitMode=x64compatible`, `LicenseFile=..\LICENSE`, `SetupIconFile=..\assets\logo\planoracad.ico`, `UninstallDisplayIcon={app}\PlanoraCAD.exe`, `OutputBaseFilename=planoracad-setup-x64`, `CloseApplications=yes`, `RestartApplications=no`, `ShowLanguageDialog=no`, `DisableProgramGroupPage=yes`.
  - `version_is_never_hard_coded`: keine Zeile `AppVersion=` mit Ziffer; `#ifndef AppVersion` mit `#error` vorhanden.
  - `languages_are_english_and_german`: `[Languages]` hat genau `en` (`compiler:Default.isl`, zuerst, gilt für alle anderen Windows-Sprachen) und `de` (`compiler:Languages\German.isl`).
  - `desktop_icon_task_is_on_by_default`: `[Tasks]` Eintrag `Name: "desktopicon"` ohne `unchecked`; `[Icons]` hat `{autoprograms}\PlanoraCAD` und `{autodesktop}\PlanoraCAD` mit `Tasks: desktopicon`.
  - `files_are_exe_and_licenses`: `[Files]` mit `Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "PlanoraCAD.exe"`, `{#LicensesFile}` als `THIRD-PARTY-LICENSES.txt`, `..\LICENSE` als `LICENSE.txt`.
  - `registry_remembers_mode_and_path`: `Root: HKA; Subkey: "Software\PlanoraCAD"` mit `ValueName: "InstallMode"; ValueData: "user"` (Check `not IsAdminInstallMode`) und `"all"` (Check `IsAdminInstallMode`), `ValueName: "InstallPath"; ValueData: "{app}"`, alle mit `Flags: uninsdeletekey`.
  - `user_data_is_never_deleted`: keine Abschnitte `[UninstallDelete]`/`[InstallDelete]`, nirgends `{userappdata}`, `{userdocs}`, `{localappdata}\PlanoraCAD` außerhalb von Kommentaren.
  - `restart_after_update_only_with_the_parameter`: `[Run]` hat (a) `{app}\PlanoraCAD.exe` mit `postinstall skipifsilent`, (b) `{app}\PlanoraCAD.exe` mit `Check: IsUpdateRun and not IsAdminInstallMode`, (c) `{win}\explorer.exe` mit `Parameters: """{app}\PlanoraCAD.exe"""` und `Check: IsUpdateRun and IsAdminInstallMode`, beide `nowait`; `[Code]` enthält `{param:NACHUPDATE|0}`.
- [ ] **Step 2:** `cargo test -p planora-update --test installer_script` → FAIL (Datei fehlt).
- [ ] **Step 3: `installer/planoracad.iss` schreiben** nach den Test-Werten, dazu `Compression=lzma2`, `SolidCompression=yes`, `WizardStyle=modern`, `AppPublisherURL`/`AppSupportURL` = `https://github.com/KaelanTesseract/PlanoraCAD`, `OutputDir=..\dist` (vom Skript per `/O` überschrieben). `[Code]`: `function IsUpdateRun: Boolean; begin Result := ExpandConstant('{param:NACHUPDATE|0}') = '1'; end;`. Kurzer Kopfkommentar: was das Skript tut, dass `AppId` nie geändert wird, dass Benutzerdaten bewusst bleiben.
- [ ] **Step 4: `tools/build-installer.ps1` schreiben** (Stil wie `tools/release.ps1`: `$ErrorActionPreference='Stop'`, `Stop-Build`, UTF-8 ohne BOM). Version aus `[workspace.package]` von `Cargo.toml` (gleiche Suche wie `release.ps1`), `AppVersionNumeric` = die ersten drei Zahlen. ISCC suchen in dieser Reihenfolge: `-Iscc`, `$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe`, `${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe`, `$env:ProgramFiles\Inno Setup 6\ISCC.exe`, `Get-Command ISCC.exe`. Aufruf `ISCC.exe /Q /DAppVersion=… /DAppVersionNumeric=… "/DSourceExe=<abs>" "/DLicensesFile=<abs>" "/O<abs OutDir>" installer\planoracad.iss`; danach SHA-256 mit `Get-FileHash`, kleingeschrieben, Datei wie in Global Constraints; zum Schluss Name und Größe beider Dateien ausgeben.
- [ ] **Step 5:** `cargo test -p planora-update --test installer_script` → PASS.
- [ ] **Step 6: Bauprobe** (Debug-exe genügt, keine Installation): `cargo build -p planora-app`, dann `powershell -NoProfile -File tools\build-installer.ps1 -Exe target\debug\planora-app.exe -OutDir "$env:TEMP\planora-installer-probe"` → Exit 0; `(Get-Item "$env:TEMP\planora-installer-probe\planoracad-setup-x64.exe").VersionInfo.ProductVersion` = Version aus `Cargo.toml`; Inhalt der `.sha256` = `Get-FileHash` der Setup-Datei + `  planoracad-setup-x64.exe`. Mit `-Iscc C:\gibtsnicht.exe` → Exit 1 und `ABBRUCH:`. Auch unter PowerShell 7 (`pwsh`, falls vorhanden) einmal.
- [ ] **Step 7: Commit** `feat(installer): Inno-Setup-Skript und Bauskript`.

### Task 2: Install-Modus und Start des Installers (ohne Ablauf)

**Files:**
- Create: `crates/planora-app/src/install_mode.rs`
- Modify: `crates/planora-app/src/lib.rs` (`pub mod install_mode;`), `crates/planora-app/src/update_install.rs` (nur neue Elemente unten), `crates/planora-app/Cargo.toml` (`windows-sys`-Features `Win32_System_Registry`, `Win32_UI_Shell`, `Win32_UI_WindowsAndMessaging`)
- Test: `crates/planora-app/tests/install_mode.rs`

**Interfaces:**
- Produces (`install_mode.rs`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum InstallMode { User, AllUsers }`
  - `pub const REG_SUBKEY: &str = "Software\\PlanoraCAD"; pub const REG_MODE: &str = "InstallMode"; pub const REG_PATH: &str = "InstallPath";`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct RegEntry { pub mode: String, pub path: String }`
  - `pub fn pick(exe_dir: &Path, hkcu: Option<RegEntry>, hklm: Option<RegEntry>) -> Option<InstallMode>` – HKCU zählt nur mit `mode == "user"`, HKLM nur mit `"all"`, jeweils nur wenn `path` derselbe Ordner ist wie `exe_dir`; HKCU zuerst.
  - `pub fn same_dir(a: &Path, b: &Path) -> bool` – erst `canonicalize` beider (wenn beide gelingen), sonst Text ohne abschließende Trenner, `to_lowercase`.
  - `pub fn detect(exe: Option<&Path>) -> Option<InstallMode>` – Windows: `RegGetValueW` (`RRF_RT_REG_SZ`) unter `HKEY_CURRENT_USER` und `HKEY_LOCAL_MACHINE`, dann `pick`; sonst immer `None`.
- Produces (`update_install.rs`):
  - `pub const SETUP_RESTART_PARAM: &str = "/NACHUPDATE=1";`
  - `pub fn setup_args(mode: InstallMode) -> Vec<&'static str>` – genau die Liste aus Global Constraints in dieser Reihenfolge, `/CURRENTUSER` bzw. `/ALLUSERS` an sechster Stelle, `SETUP_RESTART_PARAM` zuletzt.
  - `pub fn setup_verb(mode: InstallMode) -> &'static str` – `"open"` / `"runas"`.
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum LaunchError { Cancelled, Failed(String) }`
  - `pub trait SetupLauncher { fn launch(&self, setup: &Path, mode: InstallMode) -> Result<(), LaunchError>; }`
  - `pub struct ShellLauncher;` – Windows: `ShellExecuteExW` mit `setup_verb`, Parametern `setup_args(mode).join(" ")`, Arbeitsordner = Ordner der Setup-Datei, `SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI`, `SW_SHOWNORMAL`; `GetLastError() == ERROR_CANCELLED` → `Cancelled`, sonst `Failed(<io::Error aus dem Code>)`. Sonst: `Err(Failed(ONLY_WINDOWS))`. Läuft synchron im UI-Faden: Die Admin-Abfrage liegt auf dem sicheren Desktop; das kurze „Keine Rückmeldung“ ist hinnehmbar und spart einen COM-Faden.
  - `pub struct NoLaunch;` – immer `Err(Failed("in Tests nicht möglich".into()))`.

- [ ] **Step 1: Test schreiben** (`install_mode.rs`, Temp-Ordner über `common::temp_root`):
  - `user_entry_counts_only_for_its_own_folder` (HKCU `user` + gleicher Ordner → `User`; anderer Ordner → `None`; `mode` `all` unter HKCU → `None`)
  - `all_users_entry_counts_only_for_its_own_folder` (HKLM entsprechend → `AllUsers`)
  - `both_entries_pick_the_one_of_the_running_exe` (beide da, exe im HKLM-Ordner → `AllUsers`; im HKCU-Ordner → `User`; im Bauordner → `None`)
  - `folder_match_ignores_case_and_trailing_separator`
  - `no_entries_means_not_installed`
  - `setup_args_are_exact` (beide Modi, ganze Liste) und `setup_verb_asks_windows_for_admin_only_for_all_users`
  - `installer_script_matches_the_app` (liest `installer/planoracad.iss`: enthält `Subkey: "Software\PlanoraCAD"`, `ValueName: "InstallMode"`, `ValueName: "InstallPath"`, `{param:NACHUPDATE|0}`, und `SETUP_RESTART_PARAM` ist `/NACHUPDATE=1`)
  - `#[cfg(windows)] shell_launcher_reports_a_missing_file` (`ShellLauncher.launch(<Temp>/fehlt.exe, User)` → `Err(Failed(_))`, nie `Ok`)
  - `#[cfg(not(windows))] detect_is_none_off_windows`
- [ ] **Step 2:** `cargo test -p planora-app --test install_mode` → FAIL (Modul fehlt).
- [ ] **Step 3: Umsetzen** wie in Interfaces; unsafe-Blöcke klein, mit `// SAFETY:`-Kommentar wie in `crash_addr.rs`.
- [ ] **Step 4:** `cargo test -p planora-app --test install_mode` → PASS; `cargo deny check` → grün.
- [ ] **Step 5: Commit** `feat(update): Install-Modus aus der Registry, Installer starten`.

### Task 3: `planora-update` lädt den Installer

**Files:**
- Modify: `crates/planora-update/src/release.rs`, `crates/planora-update/src/install.rs`, `crates/planora-update/src/lib.rs`
- Test: `crates/planora-update/tests/releases.rs`, `crates/planora-update/tests/install.rs`, `crates/planora-update/tests/local_source.rs`

**Interfaces:**
- Produces:
  - `pub const SETUP_NAME: &str = "planoracad-setup-x64.exe";` `pub const SHA256_NAME: &str = "planoracad-setup-x64.sha256";` (`EXE_NAME` entfällt).
  - `Release { …, pub setup: AssetRef, pub sha256: AssetRef, … }` (Feld `exe` heißt jetzt `setup`).
  - `pub struct DownloadPaths { pub dir: PathBuf, pub setup: PathBuf, pub checksum: PathBuf }`, `impl DownloadPaths { pub fn in_dir(dir: &Path) -> DownloadPaths }` (`setup = dir/SETUP_NAME`, `checksum = dir/SHA256_NAME`).
  - `pub fn prepare_dir(paths: &DownloadPaths) -> Result<(), InstallError>` (`create_dir_all`, Fehler → `Io`).
  - `pub fn discard_download(paths: &DownloadPaths)` (beide Dateien löschen, dann `remove_dir` nur wenn leer; Fehler übergehen).
  - Entfallen: `Paths`, `SwapState`, `swap`, `rollback`, `cleanup_old`, `OK_MARKER_NAME`, `exe_dir_writable`, `InstallError::NotWritable`.
  - `InstallError::message()`: bei `TooLarge`/`Checksum` „… Es wurde nichts installiert.“; `Io` → „Das Update ließ sich nicht vorbereiten ({detail}). Das Programm läuft in der bisherigen Version weiter.“

- [ ] **Step 1: Tests anpassen und ergänzen.**
  - `releases.rs`: Hilfen auf `SETUP_NAME`; neu `old_single_exe_assets_are_not_an_update` (Release nur mit `planoracad-windows-x64.exe`/`.sha256` → ausgelassen), `setup_name_must_match_exactly` (`PlanoraCAD-setup-x64.exe`, `planoracad-setup-x64.exe.zip`, richtige Datei unter fremdem Tag-Ordner → je ausgelassen), `two_setup_assets_…` (bisher `two_exe_assets_…`).
  - `install.rs`: `download_paths_sit_in_their_own_folder` (statt `paths_sit_next_to_the_exe`), `discard_download_removes_files_and_the_empty_folder`, `discard_download_keeps_a_folder_with_other_files`, `prepare_dir_creates_the_folder`; `errors_have_plain_german_messages` auf die neuen Texte; die Tests `swap_*`, `rollback_restores_old`, `cleanup_old_ignores_errors`, `not_writable_dir_is_reported` löschen.
  - `local_source.rs`: Server liefert `SETUP_NAME`/`SHA256_NAME`.
- [ ] **Step 2:** `cargo test -p planora-update` → FAIL (Kompilierfehler: `SETUP_NAME`, `DownloadPaths` fehlen).
- [ ] **Step 3: Umsetzen** in `release.rs`/`install.rs`/`lib.rs`; Modul-Doku von `install.rs` und `lib.rs` auf „Download und Prüfung des Installers“ ändern (kein Austausch mehr). Die strenge Adressprüfung (`single_asset`, `release_url_ok`, `redirect_target`) bleibt unverändert.
- [ ] **Step 4:** `cargo test -p planora-update` und `cargo test -p planora-update --features update-test-source --test local_source` → PASS. (`planora-app` kompiliert danach bis zum Ende von Task 4 nicht; das ist gewollt, damit der Reviewer den Umbau der Bibliothek für sich sieht. `clippy` hier nur `-p planora-update`. Task 3 und Task 4 laufen direkt nacheinander.)
- [ ] **Step 5: Commit** `feat(update): Installer statt Programmdatei laden`.

### Task 4: Update-Ablauf über den Installer

**Files:**
- Modify: `crates/planora-app/src/update_install.rs`, `crates/planora-app/src/app.rs`, `crates/planora-app/src/main.rs`, `crates/planora-app/src/update_dialog.rs`, `crates/planora-app/src/crash.rs` (nur Doku zum Update-Neustart)
- Test: `crates/planora-app/tests/update_install.rs`, `crates/planora-app/tests/update_dialog.rs`, `crates/planora-app/tests/crash_startup.rs` (nur Anpassungen)

**Interfaces:**
- Consumes: `InstallMode`, `install_mode::detect`, `SetupLauncher`, `ShellLauncher`, `NoLaunch`, `LaunchError`, `setup_args` (Task 2); `DownloadPaths`, `prepare_dir`, `discard_download`, `SETUP_NAME`, `SHA256_NAME` (Task 3).
- Produces:
  - `pub enum RestartPhase { None, AwaitingPrompt, Verifying, Exiting }` (`WaitingForMarker` entfällt); `restart_locked()` = `Verifying | Exiting`.
  - `pub enum InstallDecision { Download, OnlyWindows, NotInstalled }`; `pub fn decide(auto_supported: bool, mode: Option<InstallMode>) -> InstallDecision`.
  - `Installer::start(&mut self, update: &Update, paths: DownloadPaths, downloader: Arc<dyn Downloader + Send + Sync>)`; `Installer::paths() -> Option<&DownloadPaths>`; Prüfsummen-Name `SETUP_NAME`.
  - `pub fn download_dir() -> PathBuf` = `std::env::temp_dir().join("PlanoraCAD-Update")`.
  - `pub fn startup(ws: &mut Workspace, download: &DownloadPaths, running: &Version) -> Option<AfterUpdate>` – löscht `update_pending` und den Download; Hinweis nur, wenn das Ziel die laufende Version ist.
  - `pub fn window_for_start(config: &Config, running: &Version) -> Option<WindowState>` – Fenster aus `update_pending`, nur wenn `to == running`.
  - Texte: `pub const UPDATE_CANCELLED: &str = "Update abgebrochen. Ohne die Erlaubnis von Windows lässt sich PlanoraCAD nicht für alle Benutzer aktualisieren. Du arbeitest in der bisherigen Version weiter.";` `pub fn launch_failed(detail: &str) -> String` → „Das Update ließ sich nicht starten ({detail}). Du arbeitest in der bisherigen Version weiter.“; `pub const NOT_INSTALLED_HINT: &str = "Diese Fassung von PlanoraCAD ist nicht installiert und kann sich nicht selbst aktualisieren. Lade die neue Version auf der Download-Seite herunter.";` `RESTART_BOX` = „PlanoraCAD wird für das Update beendet und startet danach von selbst neu.“
  - Entfallen: `AFTER_UPDATE_ARG`, `MARKER_WAIT`, `Restarter`, `NewProcess`, `ProcessRestarter`, `NoRestart`, `ChildProcess`, `write_ok_marker`, `ok_marker_for`, `not_started`, `rollback_failed`, `RESTART_STOPPED`, `NO_EXE`, `PlanoraApp::set_marker_wait`, `ok_marker_due`, `write_due_ok_marker`, `stop_new_process`, `restart_failed`, `restart_stopped`.
  - `PlanoraApp::new(cc, marker, finding)` (ohne `after_update`); `PlanoraApp::run_update_startup(&mut self)`; `PlanoraApp::for_tests_with_install(ws, source, downloader, launcher: Arc<dyn SetupLauncher + Send + Sync>, mode: Option<InstallMode>, download_dir: PathBuf)`; `PlanoraApp::set_install_mode(&mut self, mode: Option<InstallMode>)`.
  - `update_dialog::show(ctx, ws, state, install, installable: bool)`: ohne `installable` zeigt die Fußzeile `NOT_INSTALLED_HINT` und die Knöpfe `OPEN_DOWNLOAD_PAGE` (Hauptknopf, `UpdateAction::OpenDownloadPage`), `LATER`, `SKIP`; kein `INSTALL_NOW`.

**Ablauf** (`install_step`): `ReadyToRestart` und nichts offen → `AwaitingPrompt` (Rückfrage wie bisher) → `Verifying` (zweite Prüfung von `setup` gegen die gemerkte Summe) → `restart_now(window)`: `update_pending` samt Fenster speichern (sonst `PENDING_NOT_SAVED`, Download löschen), Patchnotes hinterlegen, `launcher.launch(&paths.setup, mode)`. `Ok` → `Exiting`, `ws.quit = true` (Fenster schließt ohne weitere Rückfrage, `on_exit` räumt den Laufmarker wie immer). `Err` → `update_pending` und Patchnotes löschen, Download löschen, `installer.fail(UPDATE_CANCELLED | launch_failed(d))`, `pending_notice` wieder auf das Update, Meldung sichtbar (`installing = true`, `install_hidden = false`); Phase `None`. Die Setup-Datei bleibt nach `Ok` liegen (Inno braucht sie); `startup` der nächsten Version räumt sie weg.

- [ ] **Step 1: Tests anpassen und schreiben.** Harness auf `for_tests_with_install(…, FakeLauncher, Some(InstallMode::User), <Temp>/download)`; `FakeLauncher` merkt `(PathBuf, InstallMode)` und antwortet `Ok`, `Err(Cancelled)` oder `Err(Failed("kaputt"))`.
  - Neu `successful_flow_saves_pending_launches_setup_and_quits`: Launcher einmal mit `(<download>/planoracad-setup-x64.exe, User)`; Datei liegt noch da und hat den Inhalt des Downloads; `pending_update() == Some((running, TARGET))`, auch auf der Platte, mit Fenster; Patchnotes-Datei `"{TARGET}\n- Schneller beim Öffnen"`; `restart_phase() == Exiting`; `ws.quit`; `close_sent()`.
  - Neu `all_users_mode_hands_the_mode_to_the_launcher` (`Some(AllUsers)` → Launcher bekommt `AllUsers`).
  - Neu `declined_admin_prompt_keeps_running_with_cancel_hint`: `Err(Cancelled)` → `state() == Failed(UPDATE_CANCELLED)`; `pending_update() == None`; keine Patchnotes-Datei; Setup-Datei gelöscht; `!ws.quit`; `!close_sent()`; Meldung zeigt `FAILED_TITLE`, `UPDATE_CANCELLED` und `RETRY`; `pending_notice()` ist das Update.
  - Neu `launch_error_keeps_running_with_message`: `Err(Failed("kaputt"))` → `Failed(launch_failed("kaputt"))`, sonst wie oben.
  - Neu `setup_changed_before_launch_is_not_started` (Setup-Datei nach dem Download, vor der zweiten Prüfung ändern, Muster wie bisher `new_file_changed_before_swap_is_not_swapped`) → Launcher nie gerufen, `Failed(InstallError::Checksum.message())`, Datei gelöscht.
  - Neu `not_installed_offers_only_the_download_page`: Modus `None` → Meldung zeigt `NOT_INSTALLED_HINT` und `OPEN_DOWNLOAD_PAGE`, nicht `INSTALL_NOW`; Klick öffnet `release_page(TAG)`; kein Download (`state() == Idle`, Download-Ordner leer).
  - Neu `startup_removes_a_leftover_download` und `window_for_start_only_for_the_target_version` (passendes `to` → Fenster; anderes `to` oder kein `update_pending` → `None`).
  - Angepasst (Name/Erwartung „nichts gestartet“ statt „nicht ausgetauscht“): `wrong_checksum_never_launches_and_shows_message`, `prompt_for_unsaved_work_before_restart_and_cancel_aborts_the_update_and_deletes_the_setup`, `pending_that_cannot_be_saved_aborts_before_launch`, `startup_with_matching_pending_shows_notice_once_and_cleans_up` (jetzt ohne Argument), `startup_notice_once_even_if_the_settings_cannot_be_written`, `startup_with_stale_pending_clears_it_and_does_not_retry`, `changes_while_checking_the_new_file_go_back_to_the_prompt`, `closing_the_window_while_locked_is_held_back` (in `Verifying`), `restart_waits_while_*`, `autosave_offer_during_the_prompt_is_not_a_cancel`, `unsaved_tabs_discarded_then_restart_is_requested`, `cancel_during_download_returns_to_idle`, `disk_full_during_download_shows_message_and_leaves_no_file`, `ui_stays_responsive_state_machine_progress`, `failure_has_its_own_title_message_on_top_and_retry`, `restart_text_appears_once_while_waiting`, `notice_sits_bottom_right_and_dialogs_do_not_cover_it`, `window_state_goes_with_the_pending_update`, `restore_window_limits_size_and_drops_far_positions`, `minimized_window_keeps_the_last_visible_state`, `non_windows_shows_hint_and_opens_release_page`.
  - Gelöscht (Austausch/Rückmeldung gibt es nicht mehr): `failing_start_of_the_new_version_rolls_back`, `missing_ok_marker_triggers_rollback_and_keeps_offering_the_version`, `stale_ok_marker_does_not_count`, `ok_marker_with_wrong_version_does_not_count`, `changes_while_waiting_for_the_new_version_never_close_without_asking`, `new_version_that_ends_at_once_rolls_back_without_waiting`, `rollback_offers_the_chosen_update_again_even_without_a_pending_notice`, `aborted_update_leaves_no_marker_of_the_new_version`, `new_file_changed_before_swap_is_not_swapped` (ersetzt).
  - `update_dialog.rs`: neu `not_installable_notice_has_download_page_as_main_button` (Text- und Knopfprüfung bei 1920 × 1080, Hell und Dunkel).
  - `crash_startup.rs`: nur an die neuen Signaturen anpassen; `notice_never_shows_while_restart_locked` bleibt mit `Exiting`.
- [ ] **Step 2:** `cargo test -p planora-app --test update_install --test update_dialog --test install_mode` → FAIL.
- [ ] **Step 3: Umsetzen.** `update_install.rs` und `app.rs` nach Interfaces und Ablauf; Modul-Doku von `update_install.rs` neu schreiben (Ablauf oben, kein Austausch). `PlanoraApp::new`: `install_mode = install_mode::detect(current_exe().ok().as_deref())`, `launcher = ShellLauncher`, `download = DownloadPaths::in_dir(&download_dir())`. `retry_possible()` = `decide(...) == Download`. `main.rs`: Argumentprüfung entfällt; Fenster über `window_for_start(&Config::load(), &running_version())`. `crash_notice_tick`: Bedingung `!self.ok_marker_due` entfällt. `crash.rs`: Doku „alte Version beim Update-Neustart“ auf „Installer schließt die alte Version“ ändern.
- [ ] **Step 4:** `cargo test -p planora-app` → PASS (alle, auch `crash_startup`, `update_check`, `settings_updates`); `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all --check`; WSL-Lauf aus Global Constraints → PASS.
- [ ] **Step 5: Commit** `feat(update): Update über den Installer, Austausch entfernt`.

### Task 5: Release-Ablauf und Dokumentation

**Files:**
- Modify: `.github/workflows/release.yml`, `README.md` (Abschnitte „Updates und Versionen“, „Bauen“), `docs/haeufige-probleme.md` („Das Update klappt nicht“, SmartScreen-Absatz nennt den Installer), `CHANGELOG.md` (Abschnitt `0.2.0-dev.1`, noch nicht veröffentlicht), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Installer“)
- Test: `crates/planora-update/tests/release_meta.rs`

**Interfaces:**
- Consumes: `tools/build-installer.ps1` (Task 1), `SETUP_NAME`, `SHA256_NAME` (Task 3).

- [ ] **Step 1: Tests schreiben** (`release_meta.rs`):
  - `release_uploads_exactly_the_four_files`: die Argumentliste von `gh release create` enthält genau `'dist/planoracad-setup-x64.exe'`, `'dist/planoracad-setup-x64.sha256'`, `'dist/THIRD-PARTY-LICENSES.txt'`, `'dist/planoracad-windows-x64.pdb'`; nirgends `planoracad-windows-x64.exe`; die Namen sind `format!("'dist/{SETUP_NAME}'")` usw.
  - `release_builds_the_installer_with_the_script`: `release.yml` ruft `tools/build-installer.ps1` mit `-Exe target/release/planora-app.exe` auf und baut ohne `--features`.
  - `release_ships_the_pdb_but_update_never_loads_it` bleibt.
- [ ] **Step 2:** `cargo test -p planora-update --test release_meta` → FAIL.
- [ ] **Step 3: `release.yml` ändern.** Neuer Schritt vor „Dateien zusammenstellen“: „Inno Setup bereitstellen“ – sucht `${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe`, sonst `choco install innosetup --version 6.7.3 -y --no-progress` (gibt es die Version dort nicht: anhalten und den Controller fragen, keine ungepinnte Version). „Dateien zusammenstellen“: `.pdb` und Lizenzen wie bisher nach `dist`, dann `./tools/build-installer.ps1 -Exe target/release/planora-app.exe -Licenses "$PWD/dist/THIRD-PARTY-LICENSES.txt" -OutDir "$PWD/dist"`; die Kopie der Einzel-exe und ihre Prüfsumme entfallen. Job `release`: die vier Dateien. Kopfkommentar: Trockenlauf legt den Installer als Ergebnis des Laufs ab.
- [ ] **Step 4: Doku schreiben** (einfache Sprache, Du): README „Updates und Versionen“ – Installieren (Wahl „nur für mich“/„für alle“, Startmenü, Desktop-Häkchen), Deinstallieren über „Apps“ in den Windows-Einstellungen, Einstellungen/Protokolle/Projekte bleiben; Update lädt den Installer, prüft die Prüfsumme, fragt nach ungespeicherter Arbeit, bei „für alle“ fragt Windows nach Admin-Rechten, Ablehnen → bisherige Version läuft weiter; aus dem Bauordner gestartet nur „Download-Seite öffnen“. „Wenn nach einem Update etwas nicht stimmt“: Installer der gewünschten Version von der Release-Seite noch einmal ausführen (der `.old`-Absatz entfällt). Release-Schritt 6: die vier Dateien. „Bauen“: `tools\build-installer.ps1` (braucht Inno Setup 6). `haeufige-probleme.md`: Admin-Abfrage abgelehnt; nicht installierte Fassung; der Absatz zum geschützten Ordner entfällt. CHANGELOG `0.2.0-dev.1`: Punkt „Austausch/zurücklegen“ ersetzen durch Installer-Punkt (Wahl des Modus, Startmenü, Deinstallieren, Daten bleiben, Update über den Installer). `offene-punkte.md`: Abschnitt „Installer“ mit „Keine Signatur (SmartScreen)“, „UAC-Abfrage blockiert das Programmfenster kurz“, „Inno startet die neue Version bei ‚für alle‘ über explorer.exe“ – jeweils mit „Bleibt liegen: …“.
- [ ] **Step 5:** `cargo test -p planora-update` → PASS (auch `changelog`-Tests). YAML prüfen: `wsl -d Ubuntu -u root -- su - ci -c "cd ~/PlanoraCAD && python3 -c 'import yaml; yaml.safe_load(open(\".github/workflows/release.yml\"))'"` → Exit 0.
- [ ] **Step 6: Commit** `docs(installer): Release-Ablauf, README, Changelog`.

### Task 6: Echte Prüfung auf Dennis' Rechner (Controller, nur mit Dennis' Go)

Kein Agent. Vorher Dennis fragen und Zeitpunkt abstimmen; alles läuft unter seinem Benutzer, am Ende ist alles wieder deinstalliert. Nur 1920 × 1080, keine Prüfung kleiner Fenster. Ergebnis mit Bildern (`tools/gui-drive.ps1`) und Dennis' Urteil in der Vault-Notiz.

- [ ] **Vorbereitung:** zwei Testversionen bauen mit `--features update-test-source`: A = `0.0.1-dev.1`, B = `0.0.2-dev.1` (Version in `Cargo.toml` nur für den Bau ändern, danach `git checkout Cargo.toml Cargo.lock`), je `tools\build-installer.ps1 -OutDir <Scratchpad>\A` bzw. `\B`. Lokale Test-Quelle (kleiner HTTP-Server im Scratchpad, nicht im Repo) liefert Release-Liste mit B und die Setup-Dateien von B unter `http://127.0.0.1:<Port>/…`; `PLANORA_UPDATE_TEST_API` setzen. Vorher `%APPDATA%\PlanoraCAD` sichern (Kopie im Scratchpad).
- [ ] **Nur für mich:** A installieren (Dialog „nur für mich“, deutsche Texte, GPL-Seite, Desktop-Häkchen an) → Ordner `%LOCALAPPDATA%\Programs\PlanoraCAD`, Startmenü, Desktop, „Apps“ zeigt Name, Version, „Dennis Guse“; `HKCU\Software\PlanoraCAD` mit `user` und Pfad. A starten, Update auf B: Rückfrage, Fortschritt, Programm beendet sich, B startet von selbst in derselben Fenstergröße, Hinweis „Aktualisiert auf Version 0.0.2-dev.1“. Deinstallieren → Programmordner und Registry-Eintrag weg, `%APPDATA%\PlanoraCAD` und `Dokumente\PlanoraCAD` unverändert.
- [ ] **Für alle:** A „für alle Benutzer“ installieren (Admin-Abfrage) → `C:\Program Files\PlanoraCAD`, `HKLM\Software\PlanoraCAD` = `all`. Update auf B mit Admin-Abfrage **ablehnen** → A läuft weiter mit „Update abgebrochen …“. Erneut versuchen, zustimmen → B startet von selbst und läuft **nicht erhöht** (Task-Manager, Details, Spalte „Erhöht“ = Nein). Deinstallieren wie oben.
- [ ] **Nicht installiert:** `target\release\planora-app.exe` (Feature an) direkt starten → Meldung zeigt nur „Download-Seite öffnen“.
- [ ] **Englisch:** Installer einmal mit `/LANG=en` starten und auf der ersten Seite abbrechen (Texte englisch).
- [ ] Danach: Konfiguration zurückspielen, Testdateien und Server weg, `git status` sauber; Trockenlauf von `release.yml` erst vor dem Release von Hand (Controller, nach Dennis' Go).

## Abweichungen

Was vom Plan abgewichen ist und warum.

- **Kein doppelter Neustart (Nacharbeit Tasks 3+4):** Ursprünglich `RestartApplications=yes` und `/RESTARTAPPLICATIONS`. Die App beendet sich beim Update aber selbst und wird über `[Run]` mit `/NACHUPDATE=1` neu gestartet; der Neustart-Manager hätte sie (falls er sie noch erfasst) ein zweites Mal gestartet. Jetzt `RestartApplications=no` und kein `/RESTARTAPPLICATIONS`; `CloseApplications=yes` und `/CLOSEAPPLICATIONS` bleiben als Sicherheitsnetz.
- **Installer bis zum Start gesperrt (Nacharbeit Tasks 3+4):** Die zweite Prüfung öffnet den Installer mit Freigabe nur zum Lesen (`FILE_SHARE_READ`), prüft die Summe über diesen Griff und hält ihn bis nach `ShellExecuteExW` (`update_install::lock_verified`, `LockedSetup::launch`). So kann ihn zwischen Prüfung und Start niemand austauschen. Windows startet ein Programm trotz dieser Sperre (das Abbild wird nur zum Lesen und Ausführen geöffnet; Test `locked_setup_can_still_be_executed`), `FILE_SHARE_DELETE` ist daher nicht nötig. Unter Linux unverändert.
