# Installer im Graphit-Stil – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `planoracad-setup-x64.exe` wird eine egui-Hülle im Graphit-Stil (Entwurf C), die das bisherige Inno-Setup als unsichtbaren Motor enthält und stille Aufrufe unverändert weiterreicht.

**Architecture:** Neues Crate `planora-setup` (Binary `planoracad-setup`) mit eingebettetem Motor `planoracad-engine-x64.exe` (Inno, aus `installer/planoracad.iss`). Ein reiner Zustandsautomat (`wizard.rs`) steuert die fünf Seiten, die egui-Schicht (`ui/`) zeichnet nur. Farben und Schriften kommen aus dem neuen Crate `planora-style`, das `planora-app` per Re-Export weiter nutzt.

**Tech Stack:** Rust 2024, eframe/egui 0.36 (wgpu), windows-sys 0.61.2, planora-i18n (Fluent), Inno Setup ≥ 6.7, PowerShell 5.1/7.

**Spec:** `docs/superpowers/specs/2026-11-14-graphit-installer-design.md` (Vorschau: `docs/superpowers/specs/2026-11-14-graphit-installer-vorschau/installer-richtungen-v2.html`, Richtung C).

## Global Constraints

- Dateiname am Release bleibt `planoracad-setup-x64.exe` plus `planoracad-setup-x64.sha256`; `planora-update` und `update_install.rs` werden **nicht** geändert.
- `installer/planoracad.iss`: AppId `{{D42C66D0-6AB0-4D5C-A693-9616105847B8}`, `[Run]`, `[Registry]`, `/NACHUPDATE`, `OutputBaseFilename=planoracad-setup-x64` bleiben; `crates/planora-update/tests/installer_script.rs` und `crates/planora-app/tests/install_mode.rs` bleiben unverändert grün.
- Bau: ein Befehl `tools/build-installer.ps1` mit denselben Parametern (`-Exe`, `-Licenses`, `-OutDir`, `-Iscc`) wie heute; Inno ≥ 6.7 oder Abbruch mit klarer Meldung.
- Keine neuen Fremd-Abhängigkeiten außer weiteren Features von `windows-sys =0.61.2`; `cargo deny check` sauber.
- Die App sieht nach Task 1 pixelgleich aus (reine Verschiebung).
- Alle Texte über `tr!` mit Schlüsseln `setup-*` in `crates/planora-i18n/languages/de.ftl` und `en.ftl`; Du-Form, Laiensprache, keine Superlative. Kommentare deutsch.
- Fenster 960 × 640 logische Punkte, nicht veränderbar; Prüfung bei 1920 × 1080.
- Hülle: `windows_subsystem = "windows"`, Manifest `requestedExecutionLevel level="asInvoker"`, Programmsymbol `assets/logo/planoracad.ico`.
- Handtest und echtes Installieren nur nach ausdrücklichem Go von Dennis (jedes Mal neu fragen). Nach Tests WSL-Ordner aufräumen (siehe `CLAUDE.md` im Testumgebung-Ordner).

## Review Focus

1. **Parameter mit Leerzeichen und Anführungszeichen** (`/DIR="C:\Program Files\PlanoraCAD"`, `/LOG="C:\Users\Max Muster\…"`): Die stille Weitergabe muss sie unverändert an den Motor geben. → Test `passthrough_keeps_quoted_args_verbatim` in Task 3.
2. **Admin-Abfrage abgelehnt** (Fehlercode 1223 vom Start): Zurück zu Seite „Ordner“ mit Hinweis, kein Fehlerbildschirm. → Test `uac_declined_returns_to_folder_with_hint` in Task 5.
3. **Fortschrittsdatei halb geschrieben oder leer** (Motor schreibt gerade): kein Rücksprung des Balkens, kein Absturz. → Test `progress_never_goes_backwards_and_ignores_garbage` in Task 5.
4. **Ordnerpfad leer, relativ oder mit ungültigen Zeichen** (`<>|"*?`): Installieren bleibt gesperrt mit Hinweis. → Test `invalid_folder_blocks_install` in Task 5.
5. **Vorhandene Installation in HKLM und HKCU gleichzeitig** (alter Doppel-Eintrag): HKCU gewinnt wie in `install_mode::pick` der App, Modus und Ordner gesperrt. → Test `existing_install_prefers_current_user` in Task 5.

---

### Task 1: Gemeinsames Crate `planora-style` (Farben und Schriften)

**Files:**
- Create: `crates/planora-style/Cargo.toml`, `crates/planora-style/src/lib.rs`, `crates/planora-style/src/fonts.rs`
- Modify: `Cargo.toml` (workspace.dependencies `planora-style = { path = "crates/planora-style" }`), `crates/planora-app/Cargo.toml`, `crates/planora-app/src/theme.rs:1-150` (Palette/DARK/LIGHT/rgb/palette), `crates/planora-app/src/fonts.rs`
- Test: `crates/planora-style/tests/palette.rs`

**Interfaces:**
- Produces: `planora_style::{Palette, DARK, LIGHT, palette(dark: bool) -> &'static Palette}`; `planora_style::fonts::{definitions() -> egui::FontDefinitions, medium() -> FontFamily, semibold() -> FontFamily, icons() -> FontFamily}`. Felder von `Palette` bleiben exakt wie heute (`bg_viewport`, `bg_panel`, `bg_section`, `bg_field`, `accent`, `accent_subtle`, `text_primary`, `text_secondary`, `border_subtle`, `ok`, `error`, …).
- `planora-app` behält alle Pfade: `theme.rs` bekommt `pub use planora_style::{Palette, palette};` und `pub(crate) use planora_style::DARK;`, `fonts.rs` wird zu `pub use planora_style::fonts::*;`.

- [ ] **Step 1: Test schreiben** `palette_values_are_unchanged`: `DARK.bg_viewport == Color32::from_rgb(0x0e,0x0e,0x0e)`, `DARK.bg_panel == 0x181818`, `DARK.accent == 0x006dfd`, `LIGHT.bg_viewport == 0xe8e9ec`, `LIGHT.bg_panel == 0xffffff`, `LIGHT.text_primary == 0x151515`; `fonts_have_three_families`: `definitions()` enthält Familien `medium()`, `semibold()`, `icons()`.
- [ ] **Step 2:** `cargo test -p planora-style` → FAIL (Crate fehlt).
- [ ] **Step 3:** Crate anlegen, `Palette`, `rgb`, `DARK`, `LIGHT`, `palette` aus `theme.rs` und den Inhalt von `fonts.rs` hineinverschieben. Schriften per `include_bytes!("../../planora-app/assets/fonts/…")` — **Dateien nicht verschieben** (andere Zweige). Kommentare mitnehmen.
- [ ] **Step 4:** `cargo test -p planora-style -p planora-app` → PASS; `cargo clippy -p planora-style -p planora-app -- -D warnings` sauber; `cargo fmt -p planora-style -p planora-app -- --check`.
- [ ] **Step 5:** Commit `planora-style: Farben und Schriften als eigenes Crate`.

### Task 2: Motor – Fortschrittsdatei und dunkle Graphit-Farben

**Files:**
- Modify: `installer/planoracad.iss` (`[Setup]`, `[Code]`)
- Test: `crates/planora-setup/tests/engine_script.rs` (Crate-Ordner hier schon anlegen mit minimalem `Cargo.toml` und leerem `src/main.rs`, `fn main() {}`)

**Interfaces:**
- Produces: Motor-Parameter `/PROGRESSFILE=<Pfad>`. Dateiinhalt, immer ganz neu geschrieben (`SaveStringToFile(…, False)`), UTF-8/ASCII, zwei Zeilen: `percent=<0-100>` und `step=<prepare|install|postinstall|done>`.

- [ ] **Step 1: Tests schreiben** (lesen `installer/planoracad.iss` wie `installer_script.rs`): `progress_file_is_written_only_with_param` – `[Code]` enthält `{param:PROGRESSFILE|}`, `CurInstallProgressChanged`, `CurStepChanged`, und jede Schreibstelle steht hinter einer Prüfung auf leeren Pfad; `wizard_is_dark_graphit` – `[Setup]` enthält `WizardStyle=modern dynamic`, `WizardBackColor=#ffffff`, `WizardBackColorDynamicDark=#181818`; `update_contract_unchanged` – AppId, `IsUpdateRun`, `{param:NACHUPDATE|0}`, `RestartApplications=no` vorhanden.
- [ ] **Step 2:** `cargo test -p planora-setup --test engine_script` → FAIL.
- [ ] **Step 3:** `.iss` ergänzen: `procedure CurInstallProgressChanged(CurProgress, MaxProgress: Integer)` und `procedure CurStepChanged(CurStep: TSetupStep)` schreiben über eine Hilfsfunktion `WriteProgress(Percent: Integer; Step: String)`; `WizardStyle=modern dynamic` plus die zwei Farben. Sonst nichts ändern.
- [ ] **Step 4:** Tests → PASS; `cargo test -p planora-update --test installer_script` → PASS; Motor einmal bauen: `& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" /DAppVersion=0.0.0 /DAppVersionNumeric=0.0.0 /DSourceExe=<beliebige exe> /DLicensesFile=LICENSE /O"target\engine-probe" installer\planoracad.iss` → „Successful compile“. Ordner danach löschen.
- [ ] **Step 5:** Commit `Installer-Motor: Fortschrittsdatei und dunkle Graphit-Farben`.

### Task 3: Hülle – Gerüst, eingebetteter Motor, stille Weitergabe

**Files:**
- Create: `crates/planora-setup/Cargo.toml` (bin `planoracad-setup`, deps: `planora-style`, `planora-i18n`, `eframe`, `egui`, `png = "0.18"`; windows: `windows-sys =0.61.2` mit `Win32_Foundation`, `Win32_UI_Shell`, `Win32_System_Threading`, `Win32_System_Registry`, `Win32_Globalization`, `Win32_System_Diagnostics_ToolHelp`, `Win32_UI_WindowsAndMessaging`; build-dep `embed-resource = "3"`), `build.rs`, `planoracad-setup.rc`, `planoracad-setup.manifest`, `src/main.rs`, `src/args.rs`, `src/engine.rs`
- Test: `crates/planora-setup/src/args.rs` (Unit-Tests)

**Interfaces:**
- `build.rs`: liest `PLANORA_SETUP_ENGINE`; gesetzt und Datei vorhanden → `cargo:rustc-env=PLANORA_SETUP_ENGINE_PATH=<abs>` und `cargo:rustc-cfg=has_engine` (plus `cargo::rustc-check-cfg=cfg(has_engine)`), sonst nichts. `rerun-if-env-changed` und `rerun-if-changed` auf die Datei. Symbol und Manifest wie `planora-app/build.rs`.
- `args.rs`: `pub fn raw_args() -> Vec<String>` (Windows: Kommandozeile über `GetCommandLineW`, ohne Programmnamen, **Rohtext je Argument, Anführungszeichen erhalten**); `pub fn is_passthrough(args: &[String]) -> bool` (irgendein Argument beginnt mit `/`); `pub struct EngineChoice { pub all_users: bool, pub dir: PathBuf, pub desktop_icon: bool, pub lang: &'static str, pub progress_file: PathBuf, pub log_file: PathBuf }`; `pub fn engine_args(c: &EngineChoice) -> Vec<String>` → `["/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART","/CLOSEAPPLICATIONS", "/ALLUSERS"|"/CURRENTUSER", "/DIR=\"…\"", "/MERGETASKS=desktopicon"|"/MERGETASKS=!desktopicon", "/LANG=de"|"/LANG=en", "/PROGRESSFILE=\"…\"", "/LOG=\"…\""]`.
- `engine.rs`: `pub fn engine_bytes() -> Option<&'static [u8]>` (`#[cfg(has_engine)] include_bytes!(env!("PLANORA_SETUP_ENGINE_PATH"))`, sonst `None`); `pub struct Unpacked { pub dir: PathBuf, pub exe: PathBuf }` mit `fn unpack() -> io::Result<Unpacked>` (eigener Ordner `%TEMP%\planoracad-setup-<pid>-<zeit>`) und `fn cleanup(self)`; `pub enum RunError { Declined, Io(io::Error) }`; `pub fn spawn(exe: &Path, args: &[String]) -> Result<ProcessHandle, RunError>` über `ShellExecuteExW` mit `SEE_MASK_NOCLOSEPROCESS` (Fehler 1223 → `Declined`); `impl ProcessHandle { fn try_wait(&self) -> Option<u32>; fn wait(self) -> u32 }`.
- `main.rs`: `is_passthrough` → `unpack`, `spawn(raw_args)`, `wait`, `cleanup`, `std::process::exit(code)`; ohne Motor → Exit-Code 1. Sonst vorerst Exit 0 (Fenster kommt in Task 6).

- [ ] **Step 1: Tests in `args.rs`:** `passthrough_detects_any_slash_arg` (`["/VERYSILENT"]` → true, `[]` → false); `passthrough_keeps_quoted_args_verbatim` (Zerlegung von `prog.exe /VERYSILENT /DIR="C:\Program Files\PlanoraCAD" /LOG="C:\Users\Max Muster\a.log"` liefert genau `/DIR="C:\Program Files\PlanoraCAD"` und `/LOG="C:\Users\Max Muster\a.log"`; Zerlegungsfunktion `split_command_line(line: &str) -> Vec<String>` plattformneutral testbar); `engine_args_for_current_user` und `engine_args_for_all_users_without_icon` mit den genauen Listen oben.
- [ ] **Step 2:** `cargo test -p planora-setup` → FAIL.
- [ ] **Step 3:** Implementieren wie in Interfaces. Unter Nicht-Windows: `spawn` mit `std::process::Command` (nur damit es baut und testet).
- [ ] **Step 4:** Tests → PASS; `cargo build -p planora-setup` ohne `PLANORA_SETUP_ENGINE` baut; clippy/fmt sauber; `cargo deny check` sauber.
- [ ] **Step 5:** Commit `planora-setup: Gerüst und stille Weitergabe an den Motor`.

### Task 4: Bauskript, Release und Doku

**Files:**
- Modify: `tools/build-installer.ps1`, `.github/workflows/release.yml:80-105`, `CLAUDE.md` (Abschnitt „Prüfen und Abläufe“)
- Test: `crates/planora-setup/tests/build_script.rs`

**Interfaces:**
- Consumes: `PLANORA_SETUP_ENGINE` (Task 3), Motor-Ausgabename.
- Bauskript-Ablauf: Inno-Version prüfen (`(Get-Item $isccPath).VersionInfo.ProductVersion` ≥ 6.7, sonst `Stop-Build 'Inno Setup 6.7 oder neuer nötig …'`) → ISCC mit `/F"planoracad-engine-x64"` und `/O"target\installer-engine"` → `$env:PLANORA_SETUP_ENGINE` setzen → `cargo build --release --locked -p planora-setup` → `target\release\planoracad-setup.exe` nach `<OutDir>\planoracad-setup-x64.exe` kopieren → `.sha256` wie bisher. Motor landet **nicht** in `dist`.
- `release.yml`: Inno 6.7.3 installieren, wenn keins da **oder** Version < 6.7.
- `CLAUDE.md`: Zeile „Installer: `tools/build-installer.ps1` baut Motor (Inno) und Graphit-Hülle (`planora-setup`) zu `dist/planoracad-setup-x64.exe`; Inno ≥ 6.7 nötig.“

- [ ] **Step 1: Tests** (lesen die Textdateien): `build_script_checks_inno_version` (enthält `ProductVersion` und `6.7`), `build_script_builds_engine_then_shell` (Reihenfolge: `/F"planoracad-engine-x64"` vor `-p planora-setup` vor `planoracad-setup-x64.exe`), `engine_never_lands_in_dist`, `release_ensures_inno_67`.
- [ ] **Step 2:** → FAIL. **Step 3:** umsetzen. **Step 4:** → PASS; echter Lauf: `cargo build --release --locked -p planora-app; powershell -File tools\build-installer.ps1` → `dist\planoracad-setup-x64.exe` und `.sha256` entstehen, Motor nur unter `target\installer-engine`.
- [ ] **Step 5:** Commit `Installer: Bauskript baut Motor und Graphit-Hülle`.

### Task 5: Zustandsautomat und Systemabfragen (ohne Oberfläche)

**Files:**
- Create: `crates/planora-setup/src/wizard.rs`, `src/progress.rs`, `src/existing.rs`, `src/system.rs`
- Test: Unit-Tests in den jeweiligen Dateien

**Interfaces:**
- `existing.rs`: `pub struct Existing { pub all_users: bool, pub dir: PathBuf }`; `pub fn pick(hkcu: Option<PathBuf>, hklm: Option<PathBuf>) -> Option<Existing>` (HKCU gewinnt); `pub fn detect() -> Option<Existing>` liest `Software\PlanoraCAD` → `InstallPath` (Werte wie `install_mode.rs` der App: `REG_SUBKEY`, `REG_PATH`).
- `system.rs`: `pub fn ui_lang() -> &'static str` (`"de"` wenn die erste Windows-Oberflächensprache mit `de` beginnt, sonst `"en"`; reine Hilfsfunktion `lang_from(list: &[String]) -> &'static str`); `pub fn default_dir(all_users: bool) -> PathBuf` (`%LOCALAPPDATA%\Programs\PlanoraCAD` bzw. `%ProgramFiles%\PlanoraCAD`); `pub fn running_in(dir: &Path) -> bool` (Toolhelp-Snapshot, `QueryFullProcessImageNameW`, Vergleich ohne Groß/Klein mit `dir\PlanoraCAD.exe`; reine Hilfsfunktion `exe_matches(proc: &Path, dir: &Path) -> bool`).
- `progress.rs`: `pub struct Progress { pub percent: u8, pub step: Step }`, `pub enum Step { Prepare, Install, PostInstall, Done }`; `pub fn parse(text: &str) -> Option<Progress>`; `pub struct Tracker` mit `fn update(&mut self, p: Progress)` (Prozent nur steigend), `fn current(&self) -> Progress`.
- `wizard.rs`: `pub enum Page { Welcome, License, Folder, Installing, Done, Failed { log: PathBuf } }`; `pub enum Hint { None, AppRunning, UacDeclined, InvalidFolder }`; `pub struct Wizard { pub page, pub accepted: bool, pub all_users: bool, pub dir: String, pub desktop_icon: bool, pub start_app: bool, pub locked: bool, pub hint: Hint, pub existing: Option<Existing> }`; `Wizard::new(existing: Option<Existing>) -> Wizard` (Vorbelegung: nur für mich, Standardordner, Desktop an, Starten an; mit `existing` Modus/Ordner daraus und `locked = true`); `fn next(&mut self)`, `fn back(&mut self)`, `fn can_next(&self) -> bool` (Lizenz nur mit `accepted`; Ordner nur mit gültigem Pfad); `fn set_all_users(&mut self, v: bool)` (setzt Standardordner, wenn nicht gesperrt und Ordner noch Standard); `fn folder_valid(&self) -> bool` (absolut, nicht leer, keine `<>|"*?`); `fn start_install(&mut self, app_running: bool) -> bool` (läuft App → `Hint::AppRunning`, bleibt auf `Folder`); `fn engine_finished(&mut self, result: Result<u32, RunError>, log: PathBuf)` (0 → `Done`; `Declined` → `Folder` + `Hint::UacDeclined`; sonst `Failed { log }`); `fn steps(&self) -> [(Page, StepState); 5]` mit `StepState { Done, Current, Open }` für die linke Leiste.

- [ ] **Step 1: Tests:** `existing_install_prefers_current_user`; `lang_from_picks_de_only_for_german` (`["de-AT"]`→de, `["fr-FR","de-DE"]`→en, `[]`→en); `exe_matches_ignores_case`; `progress_never_goes_backwards_and_ignores_garbage` (`"percent=40\nstep=install"`, dann `""`, dann `"percent=3"`, dann `"percent=x"` → bleibt 40/Install; `"percent=100\nstep=done"` → 100/Done); `license_needs_acceptance`; `invalid_folder_blocks_install` (`""`, `"relativ"`, `"C:\\a<b"`); `switching_mode_moves_default_folder`; `existing_install_locks_mode_and_folder`; `running_app_shows_hint_and_stays`; `uac_declined_returns_to_folder_with_hint`; `engine_error_shows_failed_with_log`; `steps_mark_done_current_open`.
- [ ] **Step 2:** → FAIL. **Step 3:** umsetzen (Windows-Aufrufe hinter `#[cfg(windows)]`, sonst neutrale Rückgabe). **Step 4:** → PASS, clippy/fmt.
- [ ] **Step 5:** Commit `planora-setup: Ablauf der Seiten und Systemabfragen`.

### Task 6: Oberfläche im Graphit-Stil (Entwurf C)

**Files:**
- Create: `crates/planora-setup/src/ui/mod.rs`, `ui/steps.rs` (linke Leiste), `ui/pages.rs` (rechte Seiten), `ui/assets.rs`, `crates/planora-setup/assets/` (`startseite.png`, `skizze.png`, `zeichnung.png`, `schnitt.png`)
- Modify: `crates/planora-i18n/languages/de.ftl`, `en.ftl` (Schlüssel `setup-*`), `crates/planora-setup/src/main.rs`
- Test: `crates/planora-setup/tests/texts.rs`

**Interfaces:**
- Consumes: `Wizard`, `Tracker`, `engine::{unpack, spawn, ProcessHandle}`, `args::engine_args`, `system::*`, `planora_style::{palette, fonts::definitions}`.
- `ui::run(wizard: Wizard) -> eframe::Result<()>`: `NativeOptions` mit `renderer: Wgpu`, Fenster 960 × 640, `with_resizable(false)`, Symbol aus `assets/logo/planoracad-icon-256.png`, Titel `tr!("setup-window-title")`. Hell/Dunkel nach `ctx.system_theme()` (fehlt → dunkel).
- Bilder: aus `D:\Vault\Vault\Projekte\PlanoraCAD\Wiki\bilder\` (`startseite-uebersicht`, `skizze-zeichnen`, `zeichnung-uebersicht`, `schnittansicht`) mit Pillow auf 1280 × 720 verkleinert (PNG, optimiert), eingebettet per `include_bytes!`, als Textur einmal geladen.
- Aufbau wie Vorschau Richtung C: zwei schwebende Felder (`bg_panel`, Rand `border_subtle`, Rundung und Schatten wie `theme::panel_frame` der App) auf `bg_viewport`, Abstand 12 Punkte; links 26 % Breite „Installation“ mit fünf Zeilen (Done = Haken in `ok`, Current = Fläche `bg_active`, Open = `text_secondary`), rechts Inhalt, Knöpfe unten rechts (Hauptknopf `accent`). Installieren-Seite: Balken in `accent`, darunter Schritt in Worten + Prozent, drei gleich hohe Karten mit Bild und Unterschrift. Ordner-Seite: zwei gleich hohe Karten „Nur für mich“ / „Für alle Benutzer“, Ordnerfeld mit „Durchsuchen“ (Windows-Ordnerdialog über `IFileOpenDialog` mit `FOS_PICKFOLDERS` – neues windows-sys-Feature `Win32_UI_Shell_Common`/`Win32_System_Com` nötig, in `Cargo.toml` begründen), Haken Desktop, Hinweiszeile für `Hint`.
- Texte (de; en sinngemäß gleich einfach): `setup-window-title = PlanoraCAD installieren`, `setup-steps-title = Installation`, `setup-step-welcome = Willkommen`, `setup-step-license = Lizenz`, `setup-step-folder = Ordner`, `setup-step-install = Installieren`, `setup-step-done = Fertig`, `setup-welcome-text = Dieses Programm installiert PlanoraCAD { $version } auf deinem Computer.`, `setup-welcome-update = Dieses Programm aktualisiert PlanoraCAD auf { $version }.`, `setup-license-accept = Ich stimme der Lizenz zu`, `setup-mode-user = Nur für mich`, `setup-mode-user-hint = Ohne Administratorrechte`, `setup-mode-all = Für alle Benutzer`, `setup-mode-all-hint = Windows fragt nach deiner Erlaubnis`, `setup-folder-browse = Durchsuchen`, `setup-desktop-icon = Verknüpfung auf dem Desktop`, `setup-hint-running = PlanoraCAD ist noch geöffnet. Speichere deine Arbeit und schließe das Programm.`, `setup-hint-recheck = Erneut prüfen`, `setup-hint-uac = Für alle Benutzer braucht Windows deine Erlaubnis. Wähle „Nur für mich“ oder versuche es noch einmal.`, `setup-hint-folder = Bitte wähle einen gültigen Ordner.`, `setup-installing = Wird installiert …`, `setup-teaser-sketch = Skizzen zeichnen`, `setup-teaser-drawing = Zeichnungen ableiten`, `setup-teaser-section = Schnitte zeigen`, `setup-done-text = PlanoraCAD ist installiert.`, `setup-start-app = PlanoraCAD jetzt starten`, `setup-failed-title = Die Installation hat nicht geklappt`, `setup-failed-text = Es wurde nichts verändert. Im Protokoll steht, woran es lag.`, `setup-open-log = Protokoll öffnen`, `setup-no-engine = Diese Fassung enthält keinen Installer.`, Knöpfe `setup-next = Weiter`, `setup-back = Zurück`, `setup-cancel = Abbrechen`, `setup-install = Installieren`, `setup-finish = Fertig`, `setup-close = Schließen`.
- Lizenztext: `include_str!("../../../LICENSE")` in scrollbarem Feld (`theme`-Scrollstil der App nachbilden: dünne schwebende Balken).

- [ ] **Step 1: Tests** `all_setup_keys_exist_in_de_and_en` (Liste der Schlüssel oben, `planora_i18n::with_lang(&german()/&english(), || t(key))` liefert nie den Schlüssel selbst) und `setup_assets_are_small` (jede Datei in `crates/planora-setup/assets/` < 600 KB, 1280 px breit).
- [ ] **Step 2:** → FAIL. **Step 3:** umsetzen; `main.rs` ohne Weitergabe: `Wizard::new(existing::detect())` → `ui::run`. Motor-Lauf in eigenem Thread, Fortschrittsdatei alle 100 ms lesen, `ctx.request_repaint_after(100 ms)`.
- [ ] **Step 4:** Tests → PASS; clippy/fmt; vorhandene i18n-Tests (`cargo test -p planora-i18n`) grün.
- [ ] **Step 5:** Commit `planora-setup: Oberfläche im Graphit-Stil`.

### Task 7: Rückfall, App-Start, Aufräumen

**Files:**
- Modify: `crates/planora-setup/src/main.rs`, `src/engine.rs`
- Test: Unit-Tests in `main.rs` über eine reine Funktion `decide(passthrough: bool, has_engine: bool, gui: Result<(), String>) -> Action`

**Interfaces:**
- `enum Action { Passthrough, Gui, FallbackInno, NoEngine }`: Weitergabe hat Vorrang; ohne Motor → `NoEngine` (Windows-Meldungsfenster `MessageBoxW` mit `setup-no-engine`); schlägt `ui::run` fehl (wgpu) → `FallbackInno`: Motor **ohne** Parameter starten (normaler Inno-Assistent), warten, Exit-Code weitergeben.
- Fertig-Seite: „PlanoraCAD jetzt starten“ an → `std::process::Command::new(dir.join("PlanoraCAD.exe")).spawn()` (Hülle läuft ohne Admin). Temp-Ordner wird nach Erfolg gelöscht, bei Fehler bleibt das Protokoll bis „Schließen“ (dann löschen).

- [ ] **Step 1: Tests** `passthrough_wins`, `missing_engine_never_opens_gui`, `gui_failure_falls_back_to_inno`.
- [ ] **Step 2:** → FAIL. **Step 3:** umsetzen. **Step 4:** → PASS; ganzer Lauf: `cargo test --workspace`, clippy `-D warnings` je Crate, `cargo deny check`, `tools\build-installer.ps1` baut.
- [ ] **Step 5:** Commit `planora-setup: Rückfall auf Inno und Start nach der Installation`.

### Task 8: Gesamtprüfung und Handtest

- [ ] **Step 1:** Windows voll: `cargo fmt -p <crate> -- --check` je geändertem Crate, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, Messtests im Release wie üblich.
- [ ] **Step 2:** WSL voll über `~/wsl-s6-full.sh` (Prüfkopie aus diesem Zweig); `planora-setup` muss unter Linux bauen und seine plattformneutralen Tests bestehen. Danach WSL aufräumen (`/root/target-s6`, `/root/PlanoraCAD/target`, `df -h /`), vorher `pgrep -af 'cargo|rustc|wsl-s6'`.
- [ ] **Step 3:** **Dennis um Go für den Handtest bitten.** Mit Go: `APPDATA`/`USERPROFILE` auf Testordner umleiten, bei 1920 × 1080 mit `tools/gui-drive.ps1` durchklicken: Doppelklick → alle fünf Seiten (Hell und Dunkel) → Installieren „Nur für mich“ in Testordner → Screenshots jeder Seite für Dennis; dann stilles Update `planoracad-setup-x64.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /CURRENTUSER /NACHUPDATE=1` → App startet genau einmal; dann über Windows deinstallieren (dunkler Inno-Dialog). Testbauten und Fotos danach löschen.
- [ ] **Step 4:** Vault-Notiz `2026-11-14 Graphit-Installer umgesetzt.md` (Geändert/Warum/Offen), in `PlanoraCAD.md` verlinken; Merge nur auf „bitte mergen“.
