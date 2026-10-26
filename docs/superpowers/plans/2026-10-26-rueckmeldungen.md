# Rückmeldungen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tester melden Fehler, Wünsche und Fragen aus PlanoraCAD heraus über ein vorausgefülltes GitHub-Formular (oder ohne Konto per E-Mail), sehen vorher genau, was mitgeht, und nach einem Absturz liegt ein Bericht bereit, den die App beim nächsten Start anbietet.

**Architecture:** Etappe 1 legt nur Dateien im Repo an (Issue-Formulare, Anleitungen, Label-Skript). Im Programm kommen reine Logik-Module ohne Oberfläche dazu (`diag` für Protokoll und Schritte, `build_info` für Version/System/Grafik, `feedback` für Bereinigung, Adressbau und Kürzung, `crash` für Panic-Hook, Laufmarker und Aufräumen), darüber dünne Oberflächen (`feedback_dialog`, Absturz-Rückfrage, Menü „Hilfe“). Alles, was nach außen geht, ist eine Adresse, die der Tester im Browser selbst abschickt.

**Tech Stack:** Rust, egui/eframe 0.36 (wgpu), `png` 0.18 und `chrono` =0.4.45 (beide schon da), optional `windows-sys` mit fest eingetragener Version (schon im Baum), GitHub Issue Forms (YAML), PowerShell 5.1 für Skripte.

**Spec:** `docs/superpowers/specs/2026-10-26-rueckmeldungen-design.md`

## Global Constraints

- Branch `feature/rueckmeldungen`; nie pushen, kein Tag, kein `gh release` (nur der Controller, Push nach Prüfung). Erlaubt von Dennis (2026-10-26): Labels mit `tools/labels.ps1` anlegen; Projekttafel mit `gh project` anlegen, sobald Dennis `gh auth refresh -s project` ausgeführt hat. Kein Formular-Dienst (Tally ist abgelehnt). Die Mailadresse `info@dennis-guse.de` darf öffentlich in App und README stehen; verantwortliche Stelle im Datenschutztext: „Dennis Guse, Fischbeker Str. 2a, 23869 Elmenhorst“, sonst kein Rechtstext erfunden, nur sachliche Angaben.
- Datenschutz: Jeder Bestandteil der Meldung steht im Fenster als Text mit Häkchen. Standard an: Version, System, Grafikkarte, letzte Schritte. Standard aus: Protokollzeilen, Bild, Projektdatei. Die App öffnet nur eine Adresse oder einen Ordner; gesendet wird nur durch den Klick des Testers im Browser.
- Bereinigung vor jeder Anzeige: Benutzername in Pfaden („C:\Users\<Name>\…“) ersetzt, Projektpfade auf den Dateinamen gekürzt, Mailadressen aus Protokollzeilen entfernt.
- Adresse unter 2000 Zeichen nach dem Kodieren; gekürzt wird in der Reihenfolge Protokollzeilen, letzte Schritte; die Beschreibung bleibt ganz (zur Grenze siehe Review Focus 1).
- Ablagen: `%APPDATA%\PlanoraCAD\rueckmeldungen\<Datum-Uhrzeit>\`, `%APPDATA%\PlanoraCAD\logs\planora.log` (höchstens 1 MB, drei Dateien rotierend), `%APPDATA%\PlanoraCAD\abstuerze\absturz-<Datum-Uhrzeit>.txt`. Basisordner ist immer der Ordner von `Config::config_path()` bzw. `Workspace::config_path`; Tests nur in Temp-Ordnern, nie im echten `%APPDATA%`.
- Texte in einfacher Sprache, Anrede „Du“, wenig Technik, kein KI-Ton, keine Fahrzeugmarken, kein Autodesk-Material; alle Oberflächentexte als `pub const` (oder `pub fn` für Texte mit Werten) im Modul, von Tests geprüft.
- Oberfläche im Graphit-Stil: Fenster nur über `dialog::Dialog`, Textfelder nur über `theme::text_field*`, Farben nur aus `theme` (Wächtertests `no_raw_windows`, `no_raw_text_edits`, `no_hard_coded_colors` bleiben grün). Ausgelegt und geprüft bei 1920 × 1080; bei 800 × 600 kein Absturz.
- Tastenkürzel nur über `action_table!` (`keymap/defaults.rs`), Ausführung über `commands::run_action`; jede neue Aktion ist in Einstellungen → Tastenkürzel änderbar und in der Übersicht sichtbar.
- Keine neue Abhängigkeit, wenn es ohne geht; jede neue muss `cargo deny check` mit unveränderter `deny.toml` bestehen, GPL-3-kompatibel sein und eine genau fest eingetragene Version haben (`wildcards = deny`). Sonst anhalten und beim Controller nachfragen.
- Prüfen lokal: Windows `cargo test -p <crate> --test <name>`, vor jedem Commit `cargo fmt --all --check` und `cargo clippy -p <crate> --all-targets -- -D warnings`; am Ende jeder Etappe zusätzlich WSL: `wsl -d Ubuntu -u root -- su - ci -c '. ~/.cargo/env; cd ~/PlanoraCAD && cargo test -p planora-app -p planora-project'` (Stand vorher in den WSL-Klon holen wie bei der Update-Funktion). GitHub-CI nur von Hand vor einem Release (Dennis' Regel vom 2026-10-25).
- Agenten starten kein Fenster und bauen keine Release-Builds; Sichtprüfungen macht der Controller mit `tools/gui-drive.ps1` bzw. Dennis.
- Repo-Adresse nur aus `planora_update::REPO`, Version nur aus `crate::running_version()`.

## Review Focus

1. **Sehr lange Beschreibung, Umlaute, Emoji, Zeilenumbrüche, `&`/`#`/`%`/`+` in der Beschreibung:** Die Adresse bleibt gültig und unter 2000 Zeichen; erst fallen Protokollzeilen, dann die ältesten Schritte. Passt die Beschreibung allein nicht, wird sie in der Adresse an einer Zeichengrenze (nie mitten in einem UTF-8-Zeichen oder einer `%XX`-Folge) gekürzt und endet mit `KUERZUNG_HINWEIS`; ganz steht sie in `rueckmeldung.txt` im geöffneten Ordner (Tests in Task 6).
2. **Persönliche Angaben in Protokoll, Schritten, Backtrace und Absturzbericht:** Benutzername mit Leerzeichen, Umlauten, in anderer Groß-/Kleinschreibung, mit `/` statt `\`, als `%USERPROFILE%`-Pfad oder unter `/home/<name>`; Projektpfade mit `.pcad`; Mailadressen – nichts davon steht in Vorschau, Adresse oder Datei im Rückmeldungsordner (Tests in Task 6 und Task 10).
3. **`%APPDATA%\PlanoraCAD` nicht beschreibbar oder voll:** Programmstart, Rückmeldungsfenster und Panic-Hook laufen weiter; Protokoll bleibt nur im Speicher; „Im Browser öffnen“ klappt ohne Ordner, das Fenster sagt „Die Datei ließ sich nicht anlegen“ statt „Datei hier anhängen“ (Tests in Task 3, 6, 8 und 10).
4. **Panik im Hook selbst, Panik während `diag` gesperrt ist, Panik in einem Nebenfaden, zwei Paniken kurz hintereinander:** Der Hook blockiert nie (nur `try_lock`), schreibt höchstens einen Bericht je Panik, ruft immer den früheren Hook, und eine zweite Panik im Hook wird verschluckt (Tests in Task 10).
5. **Absturz rund um den Update-Neustart:** Stürzt die neue Version vor ihrer Rückmeldung ab, legt die alte zurück (vorhanden); der Bericht der neuen Version wird beim nächsten Start angeboten, mit deren Versionsnummer. Die Absturz-Rückfrage erscheint nie bei gesperrter Bedienung (`restart_locked`), nie vor dem ersten Bild, nie vor dem Hinweis „Aktualisiert auf …“ und nie vor der Autosicherungs-Frage; der Laufmarker der alten Version zählt nicht als „unerwartet beendet“ (Tests in Task 11).

---

## Etappe 1: GitHub-Vorlagen und Anleitung

### Task 1: Issue-Formulare, Anleitungen, Label-Skript

**Files:**
- Create: `.github/ISSUE_TEMPLATE/fehler.yml`, `.github/ISSUE_TEMPLATE/wunsch.yml`, `.github/ISSUE_TEMPLATE/config.yml`, `docs/so-meldest-du-etwas.md`, `docs/haeufige-probleme.md` (aus Etappe 6 vorgezogen, weil `config.yml` und das Hilfe-Menü sie schon verlinken), `tools/labels.ps1`
- Modify: `README.md` (Verweis „So meldest du etwas“ oben unter der Kurzbeschreibung)
- Test: `crates/planora-app/tests/issue_templates.rs`

**Interfaces:**
- Produces (Task 6 prüft dagegen): Formular-Dateinamen `fehler.yml`, `wunsch.yml`; Feld-`id`s
  - `fehler.yml` (Label `bug`, Titelvorgabe `"Fehler: "`): `ziel` („Was wolltest du tun?“, Pflicht), `passiert` („Was ist passiert?“, Pflicht), `erwartet` („Was hast du erwartet?“, Pflicht), `bereich` (Dropdown: Skizze, 3D, Export, Zeichnung, Tastenkürzel, Anderes; Pflicht), `version`, `system`, `grafik`, `schritte`, `protokoll` (alle `textarea`, freiwillig, Beschreibung „Füllt das Programm aus.“), `bild` (`textarea`, freiwillig, „Bild hierher ziehen“).
  - `wunsch.yml` (Label `idee`, Titelvorgabe `"Wunsch: "`; dient auch für Fragen, die die App mit der Vorsilbe „Frage: “ vorbelegt, das Label `frage` setzt Dennis; es gibt kein eigenes Frage-Formular): `wunsch` („Was möchtest du tun können?“, Pflicht), `heute` („Wie machst du es heute?“, Pflicht), `wichtig` (Dropdown „Wie wichtig ist es dir?“: Wäre schön, Wichtig, Ohne geht es kaum; freiwillig), `bereich` (wie oben, Pflicht), `angaben` (`textarea`, freiwillig, „Füllt das Programm aus.“). Ein `markdown`-Block oben: „Zeig mit 👍 unter einem Wunsch, dass du ihn auch willst.“
  - `config.yml`: `blank_issues_enabled: false`; `contact_links`: „Häufige Probleme“ → `https://github.com/KaelanTesseract/PlanoraCAD/blob/master/docs/haeufige-probleme.md`, „So meldest du etwas“ → `…/docs/so-meldest-du-etwas.md`.
- `tools/labels.ps1 [-Probelauf]`: legt `bug`, `idee`, `frage`, `Sketch`, `3D`, `Export`, `Tastenkürzel` mit fester Farbe und kurzer deutscher Beschreibung an (`gh label create`), überspringt vorhandene (`gh label list --json name`); mit `-Probelauf` nur die Befehle ausgeben, ohne `gh` aufzurufen.

- [ ] **Step 1: Test schreiben** (`issue_templates.rs`, liest die Dateien relativ zu `CARGO_MANIFEST_DIR/../..`, kleine Zeilenauswertung ohne YAML-Bibliothek; die Formulare halten sich dafür an eine feste Einrückung `- type:` / `  id:` / `  validations:` / `    required: true`):
  - `bug_form_has_fixed_ids_and_required_fields` (genau die IDs oben in dieser Reihenfolge; Pflicht genau `ziel`, `passiert`, `erwartet`, `bereich`; `labels: ["bug"]`)
  - `idea_form_has_fixed_ids_and_required_fields` (Pflicht genau `wunsch`, `heute`, `bereich`; `labels: ["idee"]`; Text enthält „👍“)
  - `area_options_are_the_six_areas` (beide Formulare: Skizze, 3D, Export, Zeichnung, Tastenkürzel, Anderes)
  - `config_disables_blank_issues_and_links_both_pages` (beide verlinkten Dateien existieren im Repo)
  - `ids_are_url_safe` (nur `a-z`, `-`)
- [ ] **Step 2:** `cargo test -p planora-app --test issue_templates` → FAIL (Dateien fehlen).
- [ ] **Step 3: Dateien schreiben.** Anleitung `so-meldest-du-etwas.md` in einfacher Sprache, Du-Form: mit Konto (aus der App: Hilfe → Rückmeldung …; oder direkt auf GitHub), ohne Konto (Platzhalter-Abschnitt „kommt mit Etappe 4“, wird dort durch das Webformular ersetzt; Mail nur als Notweg), was an wen geht (öffentlich sichtbares Issue), wie Bilder angehängt werden (Datei ins Feld ziehen), wie 👍 wirkt. `haeufige-probleme.md`: Windows-Warnung bei unsignierter Datei (SmartScreen: „Weitere Informationen → Trotzdem ausführen“), Grafiktreiber aktualisieren, Antivirus hält die Datei fest, Programm liegt unter „Programme“ und kann sich nicht aktualisieren (aus README/`docs/superpowers/offene-punkte.md` übernehmen, nichts erfinden). README-Verweis eine Zeile.
- [ ] **Step 4:** `cargo test -p planora-app --test issue_templates` → PASS. YAML mit einem echten Parser prüfen: `wsl -d Ubuntu -u root -- su - ci -c "cd ~/PlanoraCAD && python3 -c 'import sys,yaml; [yaml.safe_load(open(f)) for f in sys.argv[1:]]' .github/ISSUE_TEMPLATE/*.yml"` → keine Ausgabe, Exit 0 (fehlt PyYAML: anhalten, Controller fragen, nichts installieren). `powershell -File tools\labels.ps1 -Probelauf` → sieben `gh label create`-Zeilen, kein Netz.
- [ ] **Step 5: Commit** `docs(rueckmeldung): Issue-Formulare, Anleitung, häufige Probleme, Label-Skript`.

### Task 2: Sichtprüfung Etappe 1 (Dennis)

- [ ] Dennis hat erlaubt (Spec, „Entschieden am 2026-10-26“): Task 1 darf vorab nach `master` gemergt und vom Controller nach der Prüfung gepusht werden (die Formulare zeigt GitHub nur auf dem Standardzweig), und `tools/labels.ps1` darf ohne `-Probelauf` laufen. Der Controller führt beides aus.
- [ ] Nach Dennis' Push: Checkliste für Dennis im Browser: „New issue“ zeigt nur „Fehler melden“, „Wunsch“ und die zwei Links; Pflichtfelder sind markiert; Vorausfüllung klappt mit `https://github.com/KaelanTesseract/PlanoraCAD/issues/new?template=fehler.yml&passiert=Test%20%C3%A4%C3%B6%C3%BC%0AZeile%202&version=0.2.0-dev.1` (Umlaute und Zeilenumbruch kommen an); beide Anleitungen lesen sich verständlich.

## Etappe 2: Rückmeldungs-Fenster in der App

### Task 3: Modul `diag` – Protokoll im Speicher und in der Datei

**Files:**
- Create: `crates/planora-app/src/diag.rs`
- Modify: `crates/planora-app/src/lib.rs` (`pub mod diag;`), `crates/planora-app/src/main.rs` (`diag::init_global(&data_dir)` vor `run_native`; `data_dir` = Ordner von `Config::config_path()`), `crates/planora-app/src/app.rs` (`PlanoraApp::new`: Startzeilen), `crates/planora-app/src/workspace.rs` (`save_active`, `save_as`, `open_document`: je eine Zeile mit Ergebnis und Dateiname, nie Inhalt)
- Test: `crates/planora-app/tests/diag.rs`

**Interfaces (Produces):**
- `pub const RING_LINES: usize = 500; pub const MAX_LOG_BYTES: u64 = 1_048_576; pub const LOG_FILES: usize = 3; pub const LOG_DIR: &str = "logs"; pub const LOG_NAME: &str = "planora.log";` (Rotation: `planora.log` → `planora.1.log` → `planora.2.log`, ältere fallen weg)
- `pub enum Level { Info, Warn, Fehler, Detail }` (`Detail` nur bei ausführlichem Protokoll, Task 18)
- `pub struct Diag` mit `Diag::in_memory() -> Diag`, `Diag::with_dir(dir: &Path) -> Diag` (legt `dir/logs` an; klappt das nicht, bleibt sie im Speicher und merkt `file_error: bool`), `fn log(&mut self, level: Level, msg: &str)` (Zeile `2026-10-26 14:03:22 INFO …`, Ortszeit über `chrono::Local`), `fn recent_lines(&self, n: usize) -> Vec<String>` (älteste zuerst), `fn set_verbose(&mut self, on: bool)`, `fn file_error(&self) -> bool`.
- Global: `pub fn init_global(data_dir: &Path)`, `pub fn info(msg: &str)`, `pub fn warn(msg: &str)`, `pub fn fehler(msg: &str)`, `pub fn detail(msg: &str)`, `pub fn recent_lines(n: usize) -> Vec<String>`, `pub fn set_verbose(on: bool)`, `pub(crate) fn global() -> &'static Mutex<Diag>` (`OnceLock`; ohne `init_global` eine reine Speicher-Instanz, damit Tests nie in `%APPDATA%` schreiben). Vergiftete Sperre wird mit `into_inner` weiter benutzt, nie `unwrap`.
- Startzeilen (Info): `Start PlanoraCAD <Version>`, System, Grafikkarte (aus Task 5, bis dahin nur Version).

- [ ] **Step 1: Tests** (alle mit `Diag::with_dir(tempdir)`): `ring_keeps_the_last_500_lines_in_order`, `lines_go_to_the_file_with_level_and_time`, `file_rotates_at_1_mb_and_keeps_three_files` (Zeilen schreiben bis > 1 MB; danach `planora.log` klein, `planora.1.log` ≈ 1 MB; nach zwei weiteren Rotationen nie `planora.3.log`), `detail_lines_only_when_verbose`, `unwritable_dir_stays_in_memory_and_reports_file_error` (Basisordner ist eine Datei statt eines Ordners → `file_error()`, `log` panict nicht, `recent_lines` liefert die Zeile), `poisoned_global_lock_still_logs` (in einem Faden mit gehaltener Sperre paniken, danach `diag::info` und `recent_lines` funktionieren), `save_and_open_write_one_line_without_content` (Workspace in Temp-Ordner, Dokument mit Körpernamen „Geheimteil“ speichern und öffnen; `recent_lines` enthält „Gespeichert“ bzw. „Geöffnet“ und den Dateinamen, nie „Geheimteil“).
- [ ] **Step 2:** `cargo test -p planora-app --test diag` → FAIL.
- [ ] **Step 3: Implementieren.** Keine neue Abhängigkeit; Datei mit `OpenOptions::append`, Größe vor jedem Schreiben aus einem mitgeführten Zähler.
- [ ] **Step 4:** `cargo test -p planora-app --test diag` → PASS; fmt, clippy.
- [ ] **Step 5: Commit** `feat(diag): Protokoll im Speicher und in rotierender Datei`.

### Task 4: Letzte Schritte

**Files:**
- Modify: `crates/planora-app/src/diag.rs` (Schritte-Ringpuffer), `crates/planora-app/src/commands.rs` (`run_action`: `diag::step(a.label())` vor dem `match`, auch für gesperrte Aktionen nicht), `crates/planora-app/src/workspace.rs` (Zeilen 233 und 301: wo `describe_change` den Namen eines neuen Rückgängig-Schritts liefert, `diag::step(label)`)
- Test: `crates/planora-app/tests/diag_steps.rs`

**Interfaces (Produces):**
- `pub const STEPS: usize = 50;` `pub struct Step { pub time: String /* "14:03:22" */, pub label: String }`; `Diag::step(&mut self, label: &str)`, `Diag::recent_steps(&self) -> Vec<Step>` (älteste zuerst); global `pub fn step(label: &str)`, `pub fn recent_steps() -> Vec<Step>`; `pub fn steps_text(steps: &[Step]) -> String` (eine Zeile je Schritt `14:03:22 Skizze extrudieren`).
- Zwei gleiche Schritte direkt hintereinander werden zu einem mit Zähler (`Rückgängig ×3`), damit gehaltene Tasten den Puffer nicht füllen.

- [ ] **Step 1: Tests:** `steps_keep_the_last_50`, `repeated_steps_are_counted_not_repeated`, `run_action_records_the_action_label` (`commands::run_action(&mut ws, Action::New)` → letzter Schritt „Neues Dokument“; globale Liste, Prüfung mit „enthält“, weil Tests parallel laufen), `blocked_model_action_in_old_version_records_nothing`, `undo_label_becomes_a_step` (eine Änderung, die `describe_change` benennt, landet als Schritt), `steps_text_format`.
- [ ] **Step 2:** `cargo test -p planora-app --test diag_steps` → FAIL.
- [ ] **Step 3: Implementieren.**
- [ ] **Step 4:** PASS; fmt, clippy; `cargo test -p planora-app --test shell_commands --test undo_labels` bleiben grün.
- [ ] **Step 5: Commit** `feat(diag): letzte Schritte`.

### Task 5: `build_info()` – Version, Kanal, System, Grafikkarte

**Files:**
- Create: `crates/planora-app/src/build_info.rs`
- Modify: `crates/planora-app/src/lib.rs`, `crates/planora-app/src/app.rs` (`PlanoraApp::new`: `build_info::set_gpu(build_info::gpu_from_adapter(&render_state.adapter.get_info()))`, danach Startzeilen in `diag`), `crates/planora-app/src/about_dialog.rs` (Zeilen System und Grafik unter der Version), `crates/planora-app/Cargo.toml` (nur wenn `windows-sys` gewählt)
- Test: `crates/planora-app/tests/build_info.rs`, `crates/planora-app/tests/about_dialog.rs` (erweitern)

**Interfaces (Produces):**
- `pub struct GpuInfo { pub name: String, pub driver: String, pub backend: String }`, `pub struct BuildInfo { pub version: Version, pub os: String, pub gpu: Option<GpuInfo> }`
- `pub fn build_info() -> BuildInfo`, `pub fn set_gpu(info: GpuInfo)` (einmal, `OnceLock`), `pub fn gpu_from_adapter(info: &eframe::egui_wgpu::wgpu::AdapterInfo) -> GpuInfo` (`driver` = `driver` und `driver_info` mit Leerzeichen, leer → „unbekannt“; `backend` = `format!("{:?}", info.backend)`), `pub fn os_text() -> String`, `pub fn windows_name(major: u32, minor: u32, build: u32) -> String`
- `impl BuildInfo { pub fn version_line(&self) -> String /* "0.2.0-dev.1 (Dev)" */; pub fn system_line(&self) -> String; pub fn gpu_line(&self) -> String /* "Name · Treiber X · Vulkan" oder GPU_UNKNOWN */ }`; `pub const GPU_UNKNOWN: &str = "Grafikkarte unbekannt";`
- Windows-Version: `RtlGetVersion` über `windows-sys = "=0.61.2"` (im `Cargo.lock` vorhanden; nur `[target.'cfg(windows)'.dependencies]`, nur die nötigen Features); `windows_name`: Build ≥ 22000 → „Windows 11 (Build N)“, sonst „Windows 10 (Build N)“ bei 10.0, sonst „Windows major.minor (Build N)“. Andere Systeme: `std::env::consts::OS`. Besteht `cargo deny check` nicht oder zieht es neue Pakete in den Baum: anhalten, Controller fragen (Ausweichweg ohne Abhängigkeit: Registry-Wert per `reg query` mit `CREATE_NO_WINDOW`).

- [ ] **Step 1: Tests:** `windows_names` ((10,0,26100) → „Windows 11 (Build 26100)“; (10,0,19045) → „Windows 10 (Build 19045)“; (6,3,9600) → „Windows 6.3 (Build 9600)“), `version_line_has_channel`, `gpu_line_without_gpu_is_unknown`, `gpu_from_adapter_fills_unknown_driver` (`AdapterInfo` von Hand gebaut), `build_info_uses_running_version`, `os_text_is_not_empty`; About: `about_shows_system_and_gpu_lines`.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3: Implementieren.**
- [ ] **Step 4:** `cargo test -p planora-app --test build_info --test about_dialog` → PASS; fmt, clippy, `cargo deny check`.
- [ ] **Step 5: Commit** `feat(app): build_info mit System und Grafikkarte`.

### Task 6: Modul `feedback` – Bereinigung, Adresse, Kürzung, Rückmeldungsordner

**Files:**
- Create: `crates/planora-app/src/feedback.rs`
- Modify: `crates/planora-app/src/lib.rs`, `crates/planora-app/tests/issue_templates.rs` (Abgleich mit den Konstanten)
- Test: `crates/planora-app/tests/feedback.rs`

**Interfaces:**
- Consumes: Task 4 (`Step`, `steps_text`), Task 5 (`BuildInfo`), `planora_update::REPO`.
- Produces:
  - `pub enum Kind { Fehler, Wunsch, Frage }` mit `label()` („Fehler“, „Wunsch“, „Frage“), `template()` (`"fehler.yml"`; Wunsch und Frage → `"wunsch.yml"`), `title_prefix()` („Fehler: “, „Wunsch: “, „Frage: “), `description_field()` (Fehler → `FIELD_PASSIERT`, sonst `FIELD_WUNSCH`).
  - Feld-Konstanten `FIELD_PASSIERT = "passiert"`, `FIELD_WUNSCH = "wunsch"`, `FIELD_VERSION = "version"`, `FIELD_SYSTEM = "system"`, `FIELD_GRAFIK = "grafik"`, `FIELD_SCHRITTE = "schritte"`, `FIELD_PROTOKOLL = "protokoll"`, `FIELD_ANGABEN = "angaben"` (beim Wunsch gehen alle Angaben gesammelt in `angaben`).
  - `pub enum Part { Version, System, Grafik, Schritte, Protokoll, Bild, Projektdatei, Absturzbericht }` mit `label()` und `default_on()` (an: Version, System, Grafik, Schritte, Absturzbericht; aus: Protokoll, Bild, Projektdatei).
  - `pub struct Personal { pub user_names: Vec<String>, pub home_dirs: Vec<String>, pub project_paths: Vec<PathBuf> }`, `Personal::from_env(project_paths: Vec<PathBuf>) -> Personal` (aus `USERNAME`, `USER`, `USERPROFILE`, `HOME`), `pub fn sanitize(text: &str, who: &Personal) -> String` (Benutzername in Pfaden → `<Benutzer>`, Groß/Klein egal, `\` und `/`; jeder Pfad, der auf `.pcad` endet oder in `project_paths` liegt → nur der letzte Teil; Mailadressen → `<Mailadresse>`).
  - `pub fn percent_encode(s: &str) -> String` (UTF-8, unverändert nur `A-Z a-z 0-9 - . _ ~`, Leerzeichen `%20`, Zeilenumbruch `%0A`, `\r` entfällt).
  - `pub const MAX_URL_LEN: usize = 2000;` `pub const KUERZUNG_HINWEIS: &str = "… (gekürzt, ganz in der angehängten Datei)";` `pub const ATTACH_HINT: &str = "Datei hier anhängen: rueckmeldung.txt";`
  - `pub struct Report { pub kind: Kind, pub description: String, pub parts: Vec<(Part, String)> }` (Texte schon bereinigt, nur angehakte Teile).
  - `pub struct Built { pub url: String, pub dropped: Vec<Part>, pub steps_cut: bool, pub description_cut: bool }`; `pub fn issue_url(report: &Report, with_file: bool) -> Built` (`https://github.com/{REPO}/issues/new?template=…&title=…&<feld>=…`; Titel = Präfix + erste Zeile der Beschreibung, höchstens 60 Zeichen; `with_file` hängt `ATTACH_HINT` an das Beschreibungsfeld; Kürzen: erst Protokoll weg, dann Schritte von den ältesten her, dann Beschreibung wie Review Focus 1).
  - `pub fn full_text(report: &Report) -> String` (alles ungekürzt, mit Überschrift je Teil).
  - `pub const FEEDBACK_DIR: &str = "rueckmeldungen";` `pub fn bundle_dir(data_dir: &Path, now: chrono::NaiveDateTime) -> PathBuf` (`rueckmeldungen/2026-10-26_14-03-22`, bei Kollision `_2`), `pub fn write_bundle(dir: &Path, report: &Report) -> std::io::Result<PathBuf>` (legt `rueckmeldung.txt` an, liefert den Pfad).
  - `pub trait SystemOpen { fn open_folder(&self, dir: &Path) -> bool; }`, `pub struct ExplorerOpen` (Windows `explorer <dir>`, sonst `xdg-open`), in Tests eine Nachbildung.

- [ ] **Step 1: Tests:**
  - `encode_examples`: `percent_encode("Größe\nä &=#%+")` == `"Gr%C3%B6%C3%9Fe%0A%C3%A4%20%26%3D%23%25%2B"`; `"a\r\nb"` → `"a%0Ab"`; Emoji „👍“ → `"%F0%9F%91%8D"`.
  - `short_report_fits_with_all_parts` (Adresse beginnt mit `https://github.com/KaelanTesseract/PlanoraCAD/issues/new?template=fehler.yml`, enthält `passiert=`, `version=`, `schritte=`; `dropped` leer).
  - `wish_and_question_use_the_wish_form_with_their_prefix`.
  - `long_log_is_dropped_first_then_oldest_steps` (Protokoll 500 Zeilen, 50 Schritte → `dropped == [Protokoll]`, `steps_cut`, Länge ≤ 2000, letzter Schritt noch drin).
  - `description_is_never_cut_while_it_fits_alone` (Beschreibung 1500 Zeichen ASCII → ganz drin).
  - `oversized_description_is_cut_at_a_char_boundary_with_hint` (5000 × „ü“ und Emoji → Länge ≤ 2000, endet auf kodiertem `KUERZUNG_HINWEIS`, keine halbe `%XX`-Folge, `description_cut`; `full_text` enthält alle 5000 Zeichen).
  - `url_never_exceeds_2000_for_any_mix` (Tabelle aus Beschreibungslängen 0, 10, 1999, 3000 × Zeichen `a`, `ä`, `&`, `\n`, `👍`).
  - `sanitize_user_name_variants` (`C:\Users\Max Müller\AppData\x.log`, `c:/users/max müller/…`, `C:\USERS\MAX MÜLLER\…`, `/home/max/…` → `<Benutzer>`; der Name „Max Müller“ steht nirgends mehr).
  - `sanitize_project_paths_to_file_name` (`D:\Kunden\Geheim\Halter.pcad\dokument.json` → `Halter.pcad\dokument.json` bzw. nur `Halter.pcad`; Ordnername „Geheim“ verschwindet).
  - `sanitize_mail_addresses` (`max.mueller@example.org` → `<Mailadresse>`; „3 @ 4“ bleibt).
  - `bundle_dir_names_and_collision`, `write_bundle_writes_full_text`, `write_bundle_into_unwritable_dir_is_an_error_not_a_panic`.
  - In `issue_templates.rs`: `app_field_ids_exist_in_the_forms` (jede `FIELD_*` steht in der passenden YAML-Datei; `Kind::template()` sind existierende Dateien).
- [ ] **Step 2:** `cargo test -p planora-app --test feedback --test issue_templates` → FAIL.
- [ ] **Step 3: Implementieren.** Kein Regex-Paket; Mailadressen über eine kleine Zeichenprüfung (Wort mit genau einem `@`, davor und danach Zeichen, danach ein Punkt).
- [ ] **Step 4:** PASS; fmt, clippy.
- [ ] **Step 5: Commit** `feat(rueckmeldung): Bereinigung, Adresse und Kürzung`.

### Task 7: Rückmeldungsfenster, Menü „Hilfe“, Tastenkürzel, Über-Dialog umziehen

**Files:**
- Create: `crates/planora-app/src/feedback_dialog.rs`
- Modify: `crates/planora-app/src/keymap/defaults.rs` (Zeile in `action_table!`), `crates/planora-app/src/commands.rs` (`Action::FeedbackOpen => return MenuRequest::Feedback`), `crates/planora-app/src/app.rs` (`MenuRequest::Feedback`, Menü „Hilfe“ in `menu_bar_measured` nach „Ansicht“, „Über PlanoraCAD“ aus `file_menu` entfernen, Feld `feedback: FeedbackDialog` in `PlanoraApp`, `open_feedback`, Regeln wie `open_overview`/`enforce_overview_rules`, `other_windows_open` zählt das Fenster), `crates/planora-app/src/lib.rs`, `CHANGELOG.md` (obersten Abschnitt ergänzen; existiert dessen Tag schon (`git tag -l`), Controller fragen)
- Test: `crates/planora-app/tests/feedback_dialog.rs`, bestehende `keymap.rs`, `keymap_overview.rs`, `shell_app.rs`/Menütests anpassen

**Interfaces:**
- Consumes: Tasks 3–6.
- Produces:
  - Tastenkürzel: `FeedbackOpen, "hilfe.rueckmeldung", "Rückmeldung geben", All, [ctrl_shift(Key::R)];` (am Ende der Zeilen für „Überall“ vor `Undo`, damit die Reihenfolge der Übersicht sinnvoll bleibt).
  - `app.rs`: `pub const HELP_MENU: &str = "Hilfe"; pub const FEEDBACK_ENTRY: &str = "Rückmeldung …"; pub const FAQ_ENTRY: &str = "Häufige Probleme";` `pub fn help_menu(ui, ws) -> MenuRequest` (Rückmeldung mit Kürzel-Hinweis, Häufige Probleme öffnet `feedback::faq_url()`, Über PlanoraCAD setzt `ws.about_open`); `pub fn open_feedback(ws, projects, settings, overview, feedback: &mut FeedbackDialog, ctx_info: FeedbackContext)`.
  - `feedback.rs` ergänzt: `pub fn faq_url() -> String`, `pub fn guide_url() -> String` (Blob-Adressen auf `master` aus `REPO`).
  - `feedback_dialog.rs`: `pub const TITLE: &str = "Rückmeldung";` Konstanten für alle Texte (u. a. `KIND_LABEL = "Worum geht es?"`, `DESCRIPTION_LABEL = "Beschreib kurz, was du gemacht hast und was passiert ist."`, `MIN_CHARS: usize = 10`, `TOO_SHORT = "Bitte schreib mindestens 10 Zeichen."`, `PREVIEW_HEADING = "Das geht mit"`, `OPEN_BROWSER = "Im Browser öffnen"`, `CANCEL = "Abbrechen"`, `SHOW_ADDRESS = "Adresse anzeigen"`, `COPY_TEXT = "Text kopieren"`, `BROWSER_HINT = "Hat sich kein Browser geöffnet? Kopier die Adresse und füg sie im Browser ein."`, `COPIED = "Kopiert."`, `FOLDER_HINT` als `pub fn folder_hint(file: &str) -> String`, `NO_FILE = "Die Datei ließ sich nicht anlegen. Die Meldung geht trotzdem."`, `PRIVACY_LINE = "Es geht nichts weg, bevor du im Browser auf „Absenden“ klickst."`).
  - `pub struct FeedbackDialog { pub open: bool, … }` mit `open_fresh(&mut self, ctx: FeedbackContext)`, `pub struct FeedbackContext { pub info: BuildInfo, pub screen: Option<(u32, u32)>, pub steps: Vec<Step>, pub log: Vec<String>, pub personal: Personal, pub data_dir: PathBuf, pub crash_report: Option<PathBuf> }` (Schnappschuss beim Öffnen; `log` = letzte 100 Zeilen), `pub fn show(ctx, state: &mut FeedbackDialog, dark: bool) -> Option<FeedbackAction>`, `pub enum FeedbackAction { OpenBrowser(String), OpenFolder(PathBuf), Copy(String), TakeScreenshot, Close }`; die App führt Aktionen aus (`ctx.open_url(OpenUrl::new_tab)`, `SystemOpen`, `ctx.copy_text`).
  - Ablauf „Im Browser öffnen“: Ordner anlegen und `rueckmeldung.txt` schreiben, wenn etwas gekürzt wurde oder Bild/Projektdatei/Absturzbericht dabei ist (sonst kein Ordner); Adresse mit `with_file` entsprechend; Ordner öffnen; Adresse öffnen; danach bleibt das Fenster offen und zeigt `BROWSER_HINT` mit „Text kopieren“ und ggf. den Ordner-Hinweis; „Schließen“ beendet.
  - Größe `DialogSize::Wide`, mittig; Vorschau je Bestandteil als einklappbarer, schreibgeschützter Mehrzeilentext (bereinigt); fertige Adresse einklappbar darunter.

- [ ] **Step 1: Tests (ganze App-Bilder wie `update_dialog.rs`, Temp-Workspace, Nachbildung `SystemOpen`):** `help_menu_has_feedback_faq_and_about_and_file_menu_no_longer_has_about`, `ctrl_shift_r_opens_the_window_even_with_an_open_tab_dialog`, `shortcut_is_listed_and_rebindable` (Id `hilfe.rueckmeldung` in `Action::ALL`, Bereich `All`, Standard Strg+Umschalt+R; Übersicht zeigt „Rückmeldung geben“; geänderte Belegung wirkt), `no_default_shortcut_conflicts` (bestehender Konflikttest bleibt grün), `default_checkboxes` (an/aus wie Spec), `preview_shows_sanitized_text` (Benutzername aus `Personal` steht nicht in der Ausgabe), `button_disabled_below_10_chars_with_hint`, `open_browser_emits_the_issue_url_and_shows_copy_hint` (Ausgabe `OpenUrl` mit `issues/new?template=fehler.yml`; danach `BROWSER_HINT` sichtbar), `copy_puts_the_url_into_the_clipboard` (`OutputCommand::CopyText`), `long_input_writes_bundle_and_opens_folder` (Nachbildung bekommt den Ordner; Datei vorhanden; Adresse enthält `ATTACH_HINT`), `unwritable_data_dir_still_opens_browser_with_no_file_hint`, `unchecked_parts_are_not_in_the_url`, `esc_and_cancel_close_without_side_effects`, `window_rules_like_the_overview` (öffnet sich über Über/Übersicht, schließt diese; während einer Rückfrage nicht), `fits_at_1920x1080_and_does_not_panic_at_800x600` (Bild mit `screen_rect` 800 × 600 läuft durch).
- [ ] **Step 2:** `cargo test -p planora-app --test feedback_dialog` → FAIL.
- [ ] **Step 3: Implementieren.** Über-Dialog-Tests, die den Eintrag im Datei-Menü erwarten, auf das Hilfe-Menü umstellen.
- [ ] **Step 4:** `cargo test -p planora-app` (voll), fmt, clippy.
- [ ] **Step 5: Commit** `feat(rueckmeldung): Fenster, Menü Hilfe und Tastenkürzel`.
- [ ] **Step 6: Formulartexte zurückstellen (Nachtrag aus Task-1-Review).** Weil das Fenster in Etappe 1 noch fehlte, sagen die Texte „später“. Jetzt wieder auf den Ist-Zustand ändern: `.github/ISSUE_TEMPLATE/fehler.yml` (markdown-Hinweis, Zeile 9: „Die Felder Version bis Bild füllt das Programm aus …“) und alle Feldbeschreibungen „Wird später vom Programm ausgefüllt; sonst bitte selbst eintragen oder leer lassen.“ (`fehler.yml` bei `version`, `system`, `grafik`, `schritte`, `protokoll`; `wunsch.yml` bei `angaben`) zurück auf „Füllt das Programm aus.“; `docs/so-meldest-du-etwas.md`: die Vorbehalte „Ab einer späteren Version …“ in den Absätzen „Aus dem Programm“, „Wer sieht meine Meldung?“ und „Bilder anhängen“ streichen. Adresse: Bereich nicht als Parameter, sondern als Textanfang „Bereich: …“ (Spec, Abschnitt Formulare), Ids `grafik`/`schritte`.

### Task 8: Bild vom Fenster

**Files:**
- Create: `crates/planora-app/src/feedback_shot.rs`
- Modify: `crates/planora-app/src/feedback_dialog.rs` (Häkchen „Bild vom Fenster mitschicken“ löst `TakeScreenshot` aus; Vorschau zeigt Dateiname und Größe), `crates/planora-app/src/app.rs` (Ablauf je Bild, `egui::Event::Screenshot` mit eigener `UserData` abholen), `crates/planora-app/src/lib.rs`
- Test: `crates/planora-app/tests/feedback_shot.rs`

**Interfaces (Produces):**
- `pub const SHOT_FILE: &str = "bild.png"; pub const SHOT_TIMEOUT: Duration = Duration::from_secs(3); pub const SHOT_FAILED: &str = "Das Bild hat nicht geklappt.";`
- `pub fn encode_png(img: &egui::ColorImage) -> Result<Vec<u8>, String>` (RGBA 8 Bit über `png`), `pub fn save_png(dir: &Path, img: &egui::ColorImage) -> std::io::Result<PathBuf>` (`dir/bild.png`, Ordner wird angelegt).
- `pub enum ShotPhase { Idle, Hiding, Waiting { since: Instant }, Done(PathBuf), Failed }`, `pub struct ShotFlow` mit `start(&mut self)`, `fn tick(&mut self, now: Instant) -> ShotStep` (`ShotStep::{Nothing, RequestScreenshot, ShowDialogAgain}`), `fn received(&mut self, img: &ColorImage, dir: &Path)`. Ablauf: Fenster ein Bild lang verbergen, dann `ViewportCommand::Screenshot(UserData::new(FEEDBACK_SHOT_TAG))`, Ergebnis speichern, Fenster wieder zeigen; nach `SHOT_TIMEOUT` ohne Ereignis `Failed`.

- [ ] **Step 1: Tests:** `encode_roundtrips_a_4x3_dummy` (mit `png` wieder lesen, Pixel gleich), `save_png_writes_bild_png_in_the_bundle_dir`, `save_png_into_unwritable_dir_fails_cleanly`, `flow_hides_then_requests_then_shows_again` (Zustände mit eingespeistem `Instant`), `timeout_shows_dialog_again_with_failed_hint`, `foreign_screenshot_events_are_ignored` (andere `UserData`), `app_sends_viewport_screenshot_command_after_one_hidden_frame` (ganzes App-Bild: `ViewportCommand::Screenshot` in der Ausgabe, im Bild davor kein Rückmeldungsfenster).
- [ ] **Step 2:** FAIL. **Step 3:** Implementieren. **Step 4:** `cargo test -p planora-app --test feedback_shot --test feedback_dialog`, fmt, clippy.
- [ ] **Step 5: Commit** `feat(rueckmeldung): Bild vom Fenster`.

### Task 9: Sichtprüfung Etappe 2 (Controller mit `tools/gui-drive.ps1`, Dennis schaut die Bilder an)

- [ ] WSL-Lauf (siehe Global Constraints) grün.
- [ ] 1920 × 1080, Hell und Dunkel: Menü Hilfe (drei Einträge, Kürzel sichtbar), Strg+Umschalt+R, Fenster mit Vorschau, Häkchen, eingeklappte Adresse; „Im Browser öffnen“ öffnet GitHub mit vorausgefülltem Formular (nur ansehen, **nicht** absenden); Bild vom Fenster liegt im Ordner und zeigt das Programm ohne das Rückmeldungsfenster; Explorer öffnet den Ordner; „Über PlanoraCAD“ zeigt System und Grafikkarte; Einstellungen → Tastenkürzel zeigt „Rückmeldung geben“ und lässt es ändern.
- [ ] 800 × 600: Fenster öffnen, scrollen, schließen – kein Absturz.
- [ ] `%APPDATA%\PlanoraCAD\logs\planora.log` enthält Start, Version, Grafikkarte, keine Projektinhalte.

## Etappe 3: Absturzbericht

### Task 10: Panic-Hook und Bericht

**Files:**
- Create: `crates/planora-app/src/crash.rs`
- Modify: `crates/planora-app/src/main.rs` (`crash::install_hook(data_dir.join(crash::CRASH_DIR))` direkt nach `diag::init_global`), `crates/planora-app/src/app.rs` (je Bild bei Wechsel `diag::set_document(Some(pfad))` des aktiven Tabs), `crates/planora-app/src/diag.rs` (`set_document`, `current_document`), `crates/planora-app/src/lib.rs`, `crates/planora-app/Cargo.toml` (Feature `absturz-test` wie `update-test-source`, Standard aus: Strg+Umschalt+Alt+F12 löst eine Panik aus; Release-Builds schalten es nie ein)
- Test: `crates/planora-app/tests/crash.rs`

**Interfaces (Produces):**
- `pub const CRASH_DIR: &str = "abstuerze"; pub const LOG_LINES_IN_REPORT: usize = 100;`
- `pub struct ReportInput { pub version: String, pub message: String, pub location: String, pub thread: String, pub backtrace: String, pub steps: Vec<Step>, pub log: Vec<String>, pub document: Option<PathBuf>, pub time: chrono::NaiveDateTime }`
- `pub fn report_text(input: &ReportInput) -> String` (Abschnitte „Version“, „Fehler“, „Ort“, „Faden“, „Letzte Schritte“, „Protokoll“, „Ablauf (Backtrace)“; unbereinigt – bereinigt wird erst bei der Anzeige mit `feedback::sanitize`), `pub fn report_file_name(t: chrono::NaiveDateTime) -> String` (`absturz-2026-10-26_14-03-22.txt`), `pub fn write_report(dir: &Path, input: &ReportInput) -> std::io::Result<PathBuf>` (bei Kollision `_2`).
- `pub fn install_hook(dir: PathBuf)` und testbar `pub fn hook_with(dir: PathBuf, diag: &'static Mutex<Diag>, previous: Box<dyn Fn(&PanicHookInfo) + Send + Sync>) -> Box<dyn Fn(&PanicHookInfo) + Send + Sync>`: sammelt Nachricht (`&str`/`String`, sonst „unbekannt“), Ort, Fadenname, `Backtrace::force_capture()`, Schritte und 100 Zeilen nur per `try_lock` (gesperrt → „nicht verfügbar“), schreibt mit `catch_unwind` um alles herum, ein Wiedereintritts-Merker (`thread_local!`) verhindert Bericht bei Panik im Hook; ruft danach immer `previous`. Kein Netz, kein Fenster.

- [ ] **Step 1: Tests:** `report_text_has_all_sections`, `report_file_name_format`, `panicking_thread_writes_a_report_and_calls_previous_hook` (Hook mit Temp-Ordner und Zähler als `previous`; `std::thread::spawn(|| panic!("Testpanik"))` → `join` ist `Err`, genau eine Datei, enthält „Testpanik“, Ort `crash.rs`, Fadenname; Zähler 1), `hook_does_not_block_when_diag_is_locked` (Sperre in einem Faden halten, im anderen paniken → Bericht mit „nicht verfügbar“, Test endet unter 5 s), `panic_inside_the_hook_is_swallowed` (Bericht-Ordner ist eine Datei → kein Bericht, `previous` trotzdem gerufen, kein Abbruch des Testprozesses), `two_panics_write_two_reports`, `non_string_payload_is_unknown` (`std::panic::panic_any(42)`), `report_keeps_steps_and_100_log_lines`, `shown_report_is_sanitized` (`feedback::sanitize` auf einen Bericht mit Pfad im Backtrace und Dokumentpfad `…\Kunde\Teil.pcad` → kein Benutzername, nur `Teil.pcad`). Tests, die den globalen Hook setzen, laufen in einer eigenen Testdatei mit genau einem Test oder stellen `take_hook`/`set_hook` wieder her.
- [ ] **Step 2:** `cargo test -p planora-app --test crash` → FAIL. **Step 3:** Implementieren. **Step 4:** PASS (auch `--features absturz-test` übersetzt), fmt, clippy.
- [ ] **Step 5: Commit** `feat(absturz): Panic-Hook schreibt einen Bericht`.

### Task 11: Laufmarker, Absturz-Rückfrage beim Start, Aufräumen

**Files:**
- Create: `crates/planora-app/src/crash_dialog.rs`
- Modify: `crates/planora-app/src/crash.rs` (Marker, Start-Befund, Aufräumen), `crates/planora-app/src/main.rs` (Marker anlegen und Befund ermitteln vor `run_native`, an `PlanoraApp::new` übergeben), `crates/planora-app/src/app.rs` (Feld `crash_notice`, Zeigen nach den Regeln unten, `on_exit` löscht den eigenen Marker, „Als Rückmeldung öffnen“ → `open_feedback` mit `crash_report`), `crates/planora-app/src/feedback_dialog.rs` (Bestandteil Absturzbericht: Vorschau bereinigt, in der Adresse nur Nachricht und Ort, ganzer Bericht als Datei im Ordner; Häkchen „Projektdatei in den Ordner legen“, Standard aus, kopiert das im Bericht genannte Dokument (`.pcad`-Ordner) in den Rückmeldungsordner), `CHANGELOG.md`
- Test: `crates/planora-app/tests/crash_startup.rs`

**Interfaces (Produces):**
- Marker: `pub fn marker_path(dir: &Path, pid: u32) -> PathBuf` (`laeuft-<pid>.marker` im Datenordner), `pub struct RunMarker` (hält die Datei offen; Windows mit `share_mode(0)` über `std::os::windows::fs::OpenOptionsExt`, damit eine laufende Instanz ihren Marker sperrt), `pub fn start_run(data_dir: &Path, pid: u32) -> (Option<RunMarker>, StartFinding)`, `impl RunMarker { pub fn finish(self) }` (löscht).
- `pub enum StartFinding { Clean, Crashed(Vec<PathBuf>) /* Berichte, neueste zuerst */, Unexpected }`: fremde Marker, die sich öffnen lassen, sind liegengeblieben (löschen); gesperrte gehören einer laufenden Instanz (z. B. der alten Version beim Update-Neustart) und zählen nicht. Liegengeblieben + Bericht neuer als der Marker → `Crashed`; liegengeblieben ohne Bericht → `Unexpected`; Berichte ohne Marker (Panik mit sauberem Ende) → `Crashed`.
- `pub const KEEP_DAYS: u64 = 30; pub const KEEP_MAX: usize = 10; pub fn cleanup_reports(dir: &Path, now: SystemTime)` (älter als 30 Tage weg, danach höchstens die 10 neuesten; Fehler übergehen).
- `crash_dialog.rs`: `pub const TITLE: &str = "PlanoraCAD ist beim letzten Mal abgestürzt";` `pub const QUESTION: &str = "Möchtest du uns helfen?";` Knöpfe `VIEW = "Bericht ansehen"`, `AS_FEEDBACK = "Als Rückmeldung öffnen"`, `OPEN_FOLDER = "Ordner öffnen"`, `NOT_NOW = "Nicht jetzt"`, `DELETE = "Löschen"`; `UNEXPECTED_TITLE = "PlanoraCAD wurde beim letzten Mal unerwartet beendet"`, `UNEXPECTED_TEXT` (ein Satz mit Weg ins Rückmeldungsfenster), Knöpfe `AS_FEEDBACK`, `NOT_NOW`. `pub fn show(ctx, state: &mut CrashNotice, dark) -> Option<CrashAction>`, `pub enum CrashAction { View, AsFeedback, OpenFolder, NotNow, Delete }`. „Nicht jetzt“ lässt den Bericht liegen (kommt beim nächsten Start wieder); „Löschen“ löscht die angebotenen Berichte; „Bericht ansehen“ zeigt den bereinigten Text in einem Bereich mit Bildlauf.
- Zeigen nur, wenn: erstes Bild vorbei (`first_frame`), `ws.prompt.is_none()` (Autosicherungs-Frage zuerst), `after_update.is_none()`, `!restart_locked()`, `!ok_marker_due`, kein anderes Fenster offen; zählt danach in `other_windows_open`.

- [ ] **Step 1: Tests (Temp-Ordner; App-Bilder mit eingespeistem `StartFinding`):** `clean_exit_leaves_no_marker_and_next_start_is_clean`, `stale_marker_without_report_is_unexpected`, `stale_marker_with_newer_report_is_crashed`, `locked_marker_of_a_running_instance_is_ignored` (nur Windows, `#[cfg(windows)]`; zweiter `RunMarker` offen halten), `report_without_marker_is_crashed`, `cleanup_keeps_at_most_10_and_drops_older_than_30_days` (Änderungszeit per `File::set_modified`), `cleanup_ignores_unreadable_entries`, `notice_waits_for_first_frame_autosave_offer_and_after_update_notice` (in genau dieser Reihenfolge; erst wenn alle drei durch sind, erscheint sie), `notice_never_shows_while_restart_locked`, `report_from_a_newer_version_is_offered_with_its_version` (Bericht mit `Version 9.9.9-dev.1` → Vorschau zeigt sie), `not_now_keeps_the_report_delete_removes_it`, `as_feedback_opens_the_feedback_window_with_the_report_part_on_and_project_off`, `project_checkbox_copies_the_document_into_the_bundle_only_when_checked`, `unexpected_shows_the_short_hint_without_report`, `fits_at_1920x1080_and_no_panic_at_800x600`.
- [ ] **Step 2:** `cargo test -p planora-app --test crash_startup` → FAIL. **Step 3:** Implementieren. **Step 4:** `cargo test -p planora-app` (voll, `update_install.rs` bleibt grün), fmt, clippy.
- [ ] **Step 5: Commit** `feat(absturz): Laufmarker, Rückfrage beim Start, Aufräumen`.

### Task 12: Sichtprüfung Etappe 3 (Controller, Build mit `--features absturz-test`, nur lokal)

- [ ] WSL-Lauf grün.
- [ ] Strg+Umschalt+Alt+F12 → Programm endet; Bericht in `%APPDATA%\PlanoraCAD\abstuerze\`; Neustart: erst ggf. Autosicherungs-Frage, dann die Absturz-Rückfrage; alle fünf Knöpfe einmal; „Als Rückmeldung öffnen“ zeigt den bereinigten Bericht.
- [ ] Prozess im Task-Manager beenden → nächster Start zeigt „unerwartet beendet“.
- [ ] Zwei Instanzen gleichzeitig: die zweite meldet nichts.
- [ ] 1920 × 1080 Hell/Dunkel; 800 × 600 kein Absturz.

## Etappe 4: Weg ohne GitHub-Konto (vorgezogen: nach Task 8, vor Etappe 3)

Reihenfolge geändert: Etappe 4 wird gleich nach Task 8 gebaut, weil ein externer Tester die App bekommt. Weg ohne Konto ist das Webformular auf dennis-guse.de, die Mail ist nur Notweg (Vorgabe: `.superpowers/sdd/2026-10-26-rueckmeldungen/webformular-vorgabe.md`).

### Task 13: Webformular-Knopf, Notweg-Zeile und Datenschutztext

**Files:**
- Modify: `crates/planora-app/src/feedback.rs`, `crates/planora-app/src/feedback_dialog.rs`, `README.md` (Abschnitt „Rückmeldungen und Datenschutz“ neben „Updates und Versionen“), `docs/so-meldest-du-etwas.md` (Abschnitt „Ohne GitHub-Konto“ und Datenschutzhinweis), `CHANGELOG.md`
- Test: `crates/planora-app/tests/feedback.rs`, `crates/planora-app/tests/feedback_dialog.rs`

**Interfaces (Produces):**
- `pub const WEBFORM_URL`, `WEBFORM_URL_EN`; `pub fn webform_url(report: &Report, with_file: bool) -> Built` (gleiche Bereinigung, Kodierung und Kürzung wie `issue_url`, gemeinsamer Code; Parameter laut Vorgabe; Grenze 2000).
- Fenster: `WEBFORM_BUTTON = "Ohne GitHub-Konto senden"`, `WEBFORM_HINT`, `MAIL_FALLBACK = "Klappt beides nicht? Schreib an info@dennis-guse.de."` (nach dem Senden).
- Datenschutztext wie in der Spec (Verantwortliche Stelle: „Dennis Guse, Fischbeker Str. 2a, 23869 Elmenhorst“; kein erfundener Rechtstext; „keine Rechtsberatung“ nicht im Anwendertext).

- [x] **Teil A (gebaut):** `webform_url`, Knopf, Notweg-Zeile und Tests (Umlaute, `%0A`, `&`, `#`, `+`, art/bereich, Länge unter 2000, Reihenfolge beim Kürzen, Sperre, Ordner bei Bedarf). Commit `feat(rueckmeldung): Knopf „Ohne GitHub-Konto senden“`.
- [ ] **Teil B (offen):** Datenschutzabschnitt in README und Anleitung, `CHANGELOG.md`; Tests `readme_and_guide_have_privacy_section_with_responsible_party` und `no_form_service_is_referenced` (kein „tally“ u. a.). Commit `docs(rueckmeldung): Datenschutzabschnitt`.

### Task 14: Sichtprüfung Etappe 4

- [ ] 1920 × 1080: „Ohne GitHub-Konto senden“ öffnet das Webformular im Browser, vorausgefüllt (nicht absenden); Umlaute, Zeilenumbrüche, `&` und `#` kommen richtig an; bei langem Text öffnet sich der Ordner mit `rueckmeldung.txt` und der Hinweis steht im Feld.
- [ ] Dennis liest den Datenschutzabschnitt in README und Anleitung (Name und Anschrift stehen drin, keine Platzhalter).

## Etappe 5: Wünsche gewichten und Übersicht

### Task 15: Anleitung Projekttafel

**Files:**
- Create: `docs/projekttafel-einrichten.md`
- Modify: `README.md` (Link „Was geplant ist“ auf die Tafel, solange keine Adresse bekannt ist mit Platzhalter `[Link zur Projekttafel – nach dem Anlegen eintragen]`), `docs/so-meldest-du-etwas.md` (Satz zu 👍 und Tafel)
- Test: `crates/planora-app/tests/issue_templates.rs` (`guide_files_exist_and_are_linked_from_readme`)

- [ ] **Step 1:** Test ergänzen (README verlinkt `docs/so-meldest-du-etwas.md`, `docs/haeufige-probleme.md`, `docs/projekttafel-einrichten.md`) → FAIL.
- [ ] **Step 2:** Anleitung schreiben, Schritt für Schritt in einfacher Sprache: neues Projekt (Tafel) am Repo, Spalten **Geplant**, **In Arbeit**, **Fertig**, Ablauf „Neue Issues automatisch hinzufügen“ (Filter `is:issue`), Sichtbarkeit öffentlich, Link ins README; dazu der `gh`-Weg (`gh project create …`), den der Controller gehen darf. Als eigener Schritt für Dennis steht davor: `gh auth refresh -s project` (ohne dieses Recht geht `gh project` nicht).
- [ ] **Step 3:** Test PASS. **Step 4: Commit** `docs(rueckmeldung): Anleitung für die Projekttafel`.

### Task 16: Sichtprüfung Etappe 5 (Dennis)

- [ ] Dennis führt `gh auth refresh -s project` aus; danach legt der Controller die Tafel mit `gh` an (oder Dennis von Hand nach der Anleitung). Dennis prüft mit einem Test-Issue, dass es automatisch in „Geplant“ landet, und nennt den Link; Controller ersetzt den Platzhalter im README.

## Etappe 6: Kleinkram

### Task 17: „Rückmeldung geben“ nach einem Update

**Files:**
- Modify: `crates/planora-app/src/update_dialog.rs` (`show_after_update` liefert `Option<AfterUpdateAction>` mit `enum AfterUpdateAction { Feedback }`; Satz `AFTER_UPDATE_FEEDBACK = "Etwas stimmt nicht?"`, Knopf `FEEDBACK_BUTTON = "Rückmeldung geben"`; die Zeile „Laufende Version“ kommt aus `build_info()`), `crates/planora-app/src/app.rs` (Knopf schließt den Hinweis und öffnet `open_feedback`)
- Test: `crates/planora-app/tests/update_dialog.rs` (erweitern)

- [ ] **Step 1: Tests:** `after_update_notice_offers_feedback`, `feedback_button_closes_notice_and_opens_feedback_window`, `ok_still_just_closes`.
- [ ] **Step 2:** FAIL. **Step 3:** Implementieren. **Step 4:** `cargo test -p planora-app --test update_dialog --test update_install`, fmt, clippy.
- [ ] **Step 5: Commit** `feat(rueckmeldung): Rückmeldung nach einem Update anbieten`.

### Task 18: Schalter „Ausführliches Protokoll“

**Files:**
- Modify: `crates/planora-project/src/config.rs` (Feld `verbose_log: bool`, `#[serde(default, skip_serializing_if = "is_false", deserialize_with = "lenient_bool_false")]`, kein Wahrheitswert → aus; `Default` aus), `crates/planora-app/src/workspace.rs` (`verbose_log()`, `set_verbose_log(bool) -> bool` über `save_update_field`; das Feld kommt in `UpdateSettings::of`/`apply`, damit es auf allen vier Schreibwegen mitgeht: `try_persist_config`, `set_panel_transparency`, `set_keymap`, `save_places`), `crates/planora-app/src/settings_window.rs` (Abschnitt `SECTION_HELP = "Hilfe"` unter „Updates“: Schalter `VERBOSE_LABEL = "Ausführliches Protokoll"`, Hinweis `VERBOSE_NOTE = "Schreibt mehr in die Protokolldatei auf deinem Rechner, zum Beispiel jeden Befehl und wie lange er dauert. Gesendet wird davon nichts."`, bei Schreibfehler `VERBOSE_NOT_SAVED` wie die anderen Abschnitte), `crates/planora-app/src/app.rs` (`diag::set_verbose` beim Start und beim Umschalten), `crates/planora-app/src/commands.rs` (`diag::detail` mit Befehl und Dauer in `run_action`)
- Test: `crates/planora-project/tests/config_verbose.rs`, `crates/planora-app/tests/settings_help.rs`

- [ ] **Step 1: Tests:** Config: `missing_field_is_off`, `non_bool_reads_as_off_and_keeps_other_fields`, `only_on_is_written`. App: `switch_saves_and_turns_on_detail_lines`, `verbose_survives_each_of_the_four_write_paths` (je Weg: Schalter setzen bei unschreibbarer Datei → vorgemerkt; Datei wieder schreibbar; Weg auslösen; Datei enthält `"verbose_log": true`), `other_fields_survive_set_verbose_log`, `write_failure_keeps_value_for_the_session_with_hint`, `detail_lines_have_no_project_content`.
- [ ] **Step 2:** `cargo test -p planora-project --test config_verbose` und `cargo test -p planora-app --test settings_help` → FAIL. **Step 3:** Implementieren. **Step 4:** `cargo test -p planora-project -p planora-app` (voll), fmt, clippy.
- [ ] **Step 5: Commit** `feat(diag): Schalter Ausführliches Protokoll`.

### Task 19: Release-Vorlage, Changelog, Dokumentation, Aufräumen

**Files:**
- Modify: `tools/changelog-section.ps1` (Schalter `-Footer`: hängt eine Leerzeile und `So meldest du etwas: https://github.com/KaelanTesseract/PlanoraCAD/blob/master/docs/so-meldest-du-etwas.md` an), `.github/workflows/release.yml` (Aufruf mit `-Footer`), `CHANGELOG.md`, `README.md`, `docs/superpowers/offene-punkte.md` (Abschnitt „Rückmeldungen“: was geht, was nicht: Abstürze ohne Rust-Meldung, Weg ohne Konto nur per Mail, kein Formular-Dienst, Projekttafel-Stand), Plan-Abschnitt „Abweichungen“; tote Hilfsfunktionen entfernen
- Test: `crates/planora-update/tests/release_meta.rs` (bleibt grün), Probelauf des Skripts

- [ ] **Step 1:** `powershell -File tools\changelog-section.ps1 -Version <oberste Version> -Footer` → Abschnitt endet mit der Zeile „So meldest du etwas: …“; ohne `-Footer` unverändert.
- [ ] **Step 2:** Changelog-Einträge in einfacher Sprache prüfen (Etappen 2–6 stehen drin); README-Abschnitte verweisen aufeinander.
- [ ] **Step 3:** `cargo test --workspace --no-fail-fast -j 2`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo deny check`; WSL-Lauf. Ergebnisse (Anzahl bestanden/ignoriert) in den Bericht.
- [ ] **Step 4: Commit** `docs: Rückmeldungen`.

### Task 20: Sichtprüfung Etappe 6 und Gesamtprüfung

- [ ] Nach einem Test-Update (lokale Quelle wie bei der Update-Funktion, Feature `update-test-source`) zeigt „Aktualisiert auf …“ den Knopf „Rückmeldung geben“, der das Fenster öffnet.
- [ ] Einstellungen → Hilfe: Schalter an → `planora.log` bekommt Befehlszeilen mit Dauer, keine Projektinhalte; Neustart behält den Schalter.
- [ ] Ganzer Weg einmal: Fehler mit Bild → Browser (nicht absenden) → Ordner mit `bild.png` und ggf. `rueckmeldung.txt`; Wunsch; Frage; ohne Konto. 1920 × 1080 Hell/Dunkel, 800 × 600 kein Absturz.
- [ ] Gesamtprüfung durch einen frischen Prüfer über den ganzen Zweig; danach Merge nur auf Dennis' Wort.

## Abweichungen

Was vom Plan abgewichen ist und warum.
