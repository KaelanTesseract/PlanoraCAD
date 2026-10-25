# Update-Funktion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** PlanoraCAD meldet neue GitHub-Releases samt Patchnotes, aktualisiert sich auf Knopfdruck selbst (Windows) und startet sich neu; Update-Kanäle Dev/Beta/Stabil mit Auswahl in den Einstellungen.

**Architecture:** Neues Crate `planora-update` (reine Logik: Versionen, Kanäle, Release-Antwort lesen, Patchnotes, Download-Prüfung, Austausch der exe, alles hinter austauschbaren Schnittstellen und ohne GUI). `planora-app` bekommt Hintergrund-Prüfung, Meldung, Einstellungsabschnitt und Neustart-Ablauf. `planora-project::Config` bekommt vier Felder. Ein GitHub-Actions-Ablauf baut bei einem Tag das Release.

**Tech Stack:** Rust, egui/eframe 0.36, `serde_json` (schon vorhanden), ein kleiner HTTP-Client nach Lizenzprüfung (Wahl in Task 3), GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-25-update-funktion-design.md` (Abschnitt „Festlegungen nach der Bauherren-Prüfung“ gilt mit).

## Global Constraints

- Nur Windows (die Austausch-Logik darf auf Linux in Tests über Pfade laufen, wird aber nur unter Windows aktiviert).
- Oberflächentexte deutsch, einfache Sprache, Satzanfang groß; geprüft bei 1920 × 1080.
- Nie eine Datei austauschen, deren SHA-256 nicht zum Release passt; nie eine gleiche oder ältere Version anbieten; nie Nutzerdaten verändern; nie die Oberfläche anhalten (Netz in einem Hintergrund-Thread); kein Absturz bei fehlendem Netz oder kaputter Antwort.
- Repository-Name fest `KaelanTesseract/PlanoraCAD`; Downloads nur von Release-URLs dieses Repositorys über HTTPS; Dateinamen genau `planoracad-windows-x64.exe`, `planoracad-windows-x64.sha256`.
- Keine echten Netzzugriffe und keine Eingriffe in die echte exe in Tests; Tests nur mit temporärem `config_path`, nie echtes `%APPDATA%`.
- `deny.toml`-Lizenzliste bleibt unverändert; neue Abhängigkeit nur, wenn `cargo deny check` grün bleibt; sonst anhalten und beim Controller nachfragen.
- Lizenz bleibt GPL-3.0-only (`license.workspace = true` im neuen Crate).
- Agenten bauen keine Release-Builds, starten kein Fenster und veröffentlichen nichts (kein Tag, kein Push von Tags, kein `gh release`); das macht nur der Controller auf ausdrückliches Wort von Dennis.

## Review Focus

- Versionsvergleich nach SemVer inkl. `-dev.N`/`-beta.N` und `0.10.0 > 0.9.0` (Task 1).
- Eine Version wird nur angeboten, wenn sie höher ist und im Kanal liegt; Entwürfe und fehlende Dateien machen ein Release unbrauchbar (Task 1).
- Falsche Prüfsumme: nichts wird ausgetauscht (Task 5).
- Austausch scheitert oder neue Version startet nicht: alter Stand wird zurückgelegt, kein Startschleifen-Zustand (Task 5).
- Ungespeicherte Arbeit: Rückfrage wie beim Schließen, „Abbrechen“ bricht das Update ab (Task 5).
- Kein Netz, Zeitüberschreitung, Rate-Limit, kaputte Antwort: still, kein Fehlerfenster beim Start (Task 3).
- „Später“ und „Überspringen“ lassen die Meldung nicht bei jedem Start wiederkehren (Task 4).

---

### Task 1: Crate `planora-update` – Versionen, Kanäle, Release-Modell

**Files:**
- Create: `crates/planora-update/Cargo.toml` (Workspace-Felder, Abhängigkeiten: `serde`, `serde_json` wie im Workspace), `crates/planora-update/src/lib.rs`, `version.rs`, `channel.rs`, `release.rs`, `notes.rs`
- Modify: `Cargo.toml` (Workspace-Mitglieder laufen über `crates/*`; interne Abhängigkeit `planora-update = { path = "crates/planora-update" }` eintragen)
- Test: `crates/planora-update/tests/versions.rs`, `releases.rs`, `notes.rs`

**Interfaces (alle `pub`, in `planora_update`):**
- `version`: `struct Version { major: u64, minor: u64, patch: u64, pre: Option<Pre> }`, `struct Pre { kind: PreKind, number: u64 }`, `enum PreKind { Dev, Beta }` (alles andere → `ParseError`); `Version::parse(&str) -> Result<Version, ParseError>` (mit oder ohne führendes `v`; `0.2.0-dev.1`, `0.9.0-beta.2`, `1.0.0`); `impl Ord` nach SemVer (`dev` < `beta` < ohne Endung bei gleicher Nummer; Zahlen numerisch); `impl Display` (ohne `v`); `Version::channel(&self) -> Channel`; `Version::current() -> Version` (aus `env!("CARGO_PKG_VERSION")` des aufrufenden Crates über eine Funktion `Version::from_cargo(&str)`; im App-Crate die Konstante `RUNNING`).
- `channel`: `enum Channel { Dev, Beta, Stable }` mit `Channel::id() -> &'static str` (`"dev"`, `"beta"`, `"stable"`), `Channel::label() -> &'static str` („Dev“, „Beta“, „Stabil“), `Channel::from_id(&str) -> Option<Channel>`, `Channel::ALL`, `Channel::includes(self, v: &Version) -> bool` (Dev: alle; Beta: Beta und Stabil; Stabil: nur ohne Endung).
- `release`: `struct Release { version: Version, tag: String, notes: String, exe: AssetRef, sha256: AssetRef, licenses: Option<AssetRef> }`, `struct AssetRef { name: String, url: String, size: u64 }`; `parse_releases(json: &str) -> Result<Vec<Release>, ReleaseError>` (GitHub-Format: Liste von Objekten mit `tag_name`, `draft`, `prerelease`, `body`, `assets[{name, browser_download_url, size}]`); Entwürfe und Einträge mit unlesbarer Version oder ohne genau die Dateien `planoracad-windows-x64.exe` und `planoracad-windows-x64.sha256` werden **ausgelassen** (nicht Fehler); Fehler nur bei unlesbarem JSON; URL-Prüfung: `https://github.com/KaelanTesseract/PlanoraCAD/releases/download/…` sonst ausgelassen; `pub const REPO: &str = "KaelanTesseract/PlanoraCAD";`.
- `pick_update(releases: &[Release], running: &Version, channel: Channel, skipped: Option<&Version>) -> Option<Update>` mit `struct Update { latest: Release, notes_between: Vec<(Version, String)> }` (neueste passende Version, die höher ist als `running` und nicht `skipped`; `notes_between` = Notizen aller passenden Versionen größer als `running` bis einschließlich `latest`, neueste zuerst, begrenzt auf 20 Einträge und 20 000 Zeichen insgesamt mit Flag `truncated: bool` in `Update`).
- `notes`: `parse_notes(markdown: &str) -> Vec<NoteBlock>` mit `enum NoteBlock { Heading(u8, Vec<Span>), Bullet(Vec<Span>), Paragraph(Vec<Span>) }`, `struct Span { text: String, bold: bool }` (`#`/`##`/`###`, `-`/`*`-Aufzählung, Absätze, `**fett**`; alles Weitere als normaler Text; nie Panik).

- [ ] **Step 1: Tests** (`versions.rs`): `ordering_follows_semver_with_dev_and_beta` (`0.2.0-dev.1 < 0.2.0-dev.2 < 0.2.0-beta.1 < 0.2.0`), `numbers_compare_numerically` (`0.10.0 > 0.9.0`, `dev.10 > dev.9`), `parse_accepts_leading_v_and_rejects_garbage` (`v0.2.0-dev.1` ok; `0.2`, `0.2.0-rc.1`, `abc`, `0.2.0-dev` → Fehler), `display_roundtrips`, `channel_of_version`, `channel_includes_rules` (Dev alle, Beta Beta+Stabil, Stabil nur Stabil), `channel_ids_roundtrip_and_unknown_is_none`. (`releases.rs`): `parses_a_github_list`, `skips_drafts_missing_files_unknown_versions_and_foreign_urls`, `two_exe_assets_with_the_exact_name_are_ambiguous_and_skipped`, `broken_json_is_an_error`, `empty_list_gives_none`, `pick_offers_only_higher_versions_in_channel` (laufende gleiche/ältere → None), `pick_respects_skipped_version_only_for_that_version`, `notes_between_lists_all_skipped_versions_newest_first`, `notes_are_capped_at_20_releases_and_20000_chars_with_truncated_flag`. (`notes.rs`): `headings_bullets_paragraphs_bold`, `unknown_markdown_is_plain_text`, `empty_and_huge_input_never_panic`.
- [ ] **Step 2:** `cargo test -p planora-update` → Fehlschlag.
- [ ] **Step 3: Implementieren** wie oben; eigener Mini-Parser, keine neue Abhängigkeit außer vorhandenen `serde`/`serde_json`.
- [ ] **Step 4:** `cargo test -p planora-update`, `cargo clippy -p planora-update --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check`.
- [ ] **Step 5: Commit** `feat(update): Versionen, Kanäle und Release-Modell`.

### Task 2: Konfiguration und Einstellungsabschnitt „Updates“

**Files:**
- Modify: `crates/planora-project/src/config.rs` (Felder `update_channel: String` – Id, Voreinstellung `"dev"`; `update_check: bool` – Voreinstellung `true`; `last_update_check: Option<u64>` – Sekunden seit 1970; `skipped_version: Option<String>`; `update_pending: Option<PendingUpdate { from: String, to: String }>`; alle nachsichtig gelesen wie `panel_transparency`, nur Abweichungen geschrieben), `crates/planora-app/src/workspace.rs` (Zugriffe, vorhandene Mechanik „Wert bleibt bei Schreibfehler und wird nachgeholt“ für die neuen Felder wiederverwenden), `crates/planora-app/src/settings_window.rs` (Abschnitt „Updates“), `crates/planora-app/src/about_dialog.rs` (Version mit Kanal)
- Test: `crates/planora-project/tests/config_update.rs`, `crates/planora-app/tests/settings_updates.rs`

**Interfaces:**
- Consumes: Task 1 (`Channel`, `Version`).
- Produces: `Workspace::update_channel(&self) -> Channel`, `set_update_channel(&mut self, Channel) -> bool`, `update_check_enabled()`, `set_update_check(bool) -> bool`, `last_update_check() -> Option<u64>`, `set_last_update_check(u64)`, `skipped_version() -> Option<Version>`, `set_skipped_version(Option<Version>)`, `pending_update() -> Option<(Version, Version)>`, `set_pending_update(Option<(Version, Version)>)`; alle Setzer speichern über denselben Weg wie `set_panel_transparency` (Rückgabe `false` bei Schreibfehler, Wert gilt für die Sitzung).
- Einstellungsabschnitt „Updates“ (unter „Darstellung“, über „Projekte“): Zeile „Laufende Version: 0.2.0-dev.1 (Dev)“; Auswahl „Update-Kanal“ (Kanäle aus `Channel::ALL`, heute nur Dev in der Liste: `AVAILABLE_CHANNELS: &[Channel] = &[Channel::Dev]`, später erweiterbar); Schalter „Beim Start nach Updates suchen“; Knopf „Jetzt nach Updates suchen“ (Task 3 hängt die Wirkung an: bis dahin deaktiviert mit Hinweis) und Zeile „Letzte Prüfung: …“ (Ortszeit-Hilfe `local_time`). Kleiner Hinweis unter dem Schalter: „Beim Suchen fragt das Programm bei GitHub nach; dabei sieht GitHub deine IP-Adresse.“

- [ ] **Step 1: Tests:** Config: `defaults_for_old_files` (alle Felder fehlen → Voreinstellungen), `unknown_channel_reads_as_dev`, `broken_types_read_as_defaults_and_keep_other_fields`, `only_changed_fields_are_written`, `update_fields_survive_other_config_updates_and_vice_versa`, `pending_update_roundtrips`. App: `section_shows_version_channel_switch_and_note`, `channel_choice_is_saved_and_dev_is_the_only_option`, `switch_is_saved_and_failure_keeps_value_for_the_session_with_hint`, `about_dialog_shows_version_with_channel`, bestehende Einstellungstests bleiben grün.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.**
- [ ] **Step 4:** `cargo test -p planora-project -p planora-app` (voll), clippy, fmt.
- [ ] **Step 5: Commit** `feat(app): Einstellungen für Updates und Kanal`.
- [ ] **Sichtprüfung (Controller):** Abschnitt im Einstellungsfenster, Hell/Dunkel.

### Task 3: Netzzugriff, Hintergrund-Prüfung, Takt

**Files:**
- Create: `crates/planora-update/src/net.rs` (HTTP-Implementierung), `crates/planora-update/src/check.rs` (Schnittstellen und Ablauf), `crates/planora-app/src/update_check.rs` (Hintergrund-Thread, Ergebnis an die Oberfläche)
- Modify: `crates/planora-update/Cargo.toml` (HTTP-Abhängigkeit), `Cargo.toml`/`Cargo.lock`, `crates/planora-app/src/app.rs` (Aufruf beim Start, Neuzeichnen bei Ergebnis), `crates/planora-app/src/settings_window.rs` (Knopf „Jetzt nach Updates suchen“ verdrahten)
- Test: `crates/planora-update/tests/check.rs`, `crates/planora-app/tests/update_check.rs`

**Interfaces:**
- Produces in `planora_update::check`: `trait ReleaseSource { fn fetch_releases(&self) -> Result<String, NetError>; }` (liefert die JSON-Zeichenkette von `https://api.github.com/repos/KaelanTesseract/PlanoraCAD/releases`), `enum NetError { Offline, Timeout, RateLimited, Http(u16), Other(String) }`; `struct Checker<S: ReleaseSource>` mit `fn check(&self, running: &Version, channel: Channel, skipped: Option<&Version>) -> CheckOutcome` und `enum CheckOutcome { UpToDate, Available(Update), Failed(NetError) }`; `fn due(now: u64, last: Option<u64>, enabled: bool, manual: bool) -> bool` (Takt 24 h = 86 400 s; `manual` ignoriert Takt und Schalter; `last` in der Zukunft zählt als fällig); `pub struct HttpSource` (echte Implementierung, Zeitüberschreitungen: Verbindung/Antwort 10 s, Größe der Antwort ≤ 2 MB, Benutzer-Kennung `PlanoraCAD/<Version>`).
- Produces in `planora_app::update_check`: `struct UpdateChecker` (hält Thread-Handle und Kanal `mpsc`), `fn start(&mut self, source: Arc<dyn ReleaseSource + Send + Sync>, request: CheckRequest)`, `fn poll(&mut self) -> Option<CheckResult>` (nicht blockierend), `struct CheckRequest { running, channel, skipped, manual }`, `struct CheckResult { outcome: CheckOutcome, manual: bool, at: u64 }`; `PlanoraApp` startet die Prüfung beim ersten Bild, wenn `due(...)`, schreibt nach jeder Prüfung `last_update_check` (auch bei Fehler außer `Offline`/`Timeout`: dort **nicht**, damit es beim nächsten Start erneut versucht wird), und merkt `Available` in einem Feld `pending_notice: Option<Update>` für Task 4. Bei `manual` und `UpToDate`/`Failed`: kurzer Hinweis im Einstellungsabschnitt („Du hast die neueste Version.“ / „Die Suche hat nicht geklappt (kein Netz?).“); beim automatischen Start nie ein Hinweis außer bei `Available`.
- HTTP-Wahl: zuerst ein kleiner blockierender Client mit der Windows-eigenen TLS-Schicht o. Ä., der `cargo deny check` mit der unveränderten Lizenzliste besteht; Begründung der Wahl im Bericht; passt keiner, anhalten und beim Controller nachfragen (siehe Global Constraints).

- [ ] **Step 1: Tests:** `check.rs` mit einer Nachbildung von `ReleaseSource`: `up_to_date_when_running_is_latest`, `available_when_higher_in_channel`, `network_errors_become_failed_not_panics` (je Fehlerart), `broken_json_is_failed`, `empty_list_is_up_to_date`, `due_logic` (nie geprüft, vor 23 h, nach 25 h, manuell, Schalter aus, Zeit in der Zukunft). App: `start_check_runs_off_the_ui_thread_and_poll_returns_result`, `no_automatic_check_when_switch_is_off`, `automatic_check_at_start_when_due_updates_last_check`, `offline_does_not_update_last_check`, `manual_check_shows_up_to_date_hint`, `manual_check_failure_hint`, `no_error_window_at_start_on_failure`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.** `HttpSource` wird nur in `main`/`PlanoraApp::new` (nicht in Tests) angelegt; Tests und `PlanoraApp::for_tests` bekommen eine Nachbildung ohne Netz.
- [ ] **Step 4:** `cargo test -p planora-update -p planora-app` (voll), clippy, fmt, `cargo deny check`.
- [ ] **Step 5: Commit** `feat(update): Prüfung auf neue Versionen im Hintergrund`.

### Task 4: Meldung „Neue Version verfügbar“ mit Patchnotes

**Files:**
- Create: `crates/planora-app/src/update_dialog.rs`
- Modify: `crates/planora-app/src/app.rs` (Fenster zeigen, `windows_open`, Fensterausschlussregeln), `crates/planora-app/src/shell_panels.rs` oder die Statuszeile (dezenter Hinweis), `crates/planora-app/src/workspace.rs` (Zustand der Meldung), `crates/planora-app/src/theme.rs` (nur falls Farben fehlen)
- Test: `crates/planora-app/tests/update_dialog.rs`

**Interfaces:**
- Consumes: Tasks 1–3 (`Update`, `parse_notes`, `NoteBlock`, Workspace-Zugriffe `skipped_version`/`set_skipped_version`).
- Produces: `struct UpdateDialog { open: bool, update: Option<Update>, snooze_until: Option<u64>, installing: bool }`; `show(ctx, ws, state) -> Option<UpdateAction>` mit `enum UpdateAction { InstallNow, Later, Skip }`; Darstellung der Patchnotes aus `NoteBlock` (Überschriften größer, Aufzählungspunkte mit Spiegelstrich, fetter Text, Absätze; scrollbar bei Überlänge, Hinweis „… und weitere Änderungen auf GitHub“ bei `truncated`); Knöpfe „Jetzt aktualisieren“, „Später“, „Diese Version überspringen“ (`Skip` ruft `set_skipped_version(Some(latest))` und schließt); „Später“ schließt und setzt `snooze_until = now + 24 h` (nur für die Sitzung; beim nächsten Programmstart kommt die Meldung wieder, wenn die Prüfung wieder `Available` ergibt); die Meldung erscheint automatisch, wenn ein `pending_notice` eintrifft **und** kein anderes Fenster/Rückfrage offen ist (sonst wartet sie und erscheint, sobald frei); sie nimmt bei „Überspringen“ nie wieder dieselbe Version an, aber die nächste; dezenter Hinweis „Update verfügbar: <Version>“ in der Statuszeile solange `pending_notice` besteht und die Meldung geschlossen ist (Klick öffnet die Meldung). `InstallNow` wird hier nur als Aktion gemeldet und in Task 5 ausgeführt.
- Fensterregeln: zählt in `windows_open()`; schließt sich gegenseitig aus mit Projekte/Einstellungen/Dateifenstern (wie die Übersicht der Tastenkürzel: zuletzt geöffnet gewinnt); blockiert die Bereichstasten wie jedes Fenster.

- [ ] **Step 1: Tests (ganze App-Bilder mit rohen Ereignissen, Nachbildung der Quelle):** `notice_opens_when_an_update_is_found_and_nothing_else_is_open`, `notice_waits_while_a_prompt_or_window_is_open_then_appears`, `notes_of_all_skipped_versions_are_shown_newest_first`, `bold_headings_and_bullets_are_rendered` (Texte vorhanden), `truncated_notes_show_the_hint`, `later_closes_and_does_not_return_in_this_session_within_24h`, `skip_saves_the_version_and_the_same_version_is_not_offered_again_but_the_next_is`, `statusline_hint_while_pending`, `window_exclusion_with_projects_and_settings`, `install_now_reports_the_action`, `no_notice_when_the_check_found_nothing`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.**
- [ ] **Step 4:** `cargo test -p planora-app` (voll), clippy, fmt.
- [ ] **Step 5: Commit** `feat(app): Meldung über neue Versionen mit Patchnotes`.
- [ ] **Sichtprüfung (Controller, 1920 × 1080):** Meldung mit Beispiel-Patchnotes (Testquelle), Hell/Dunkel.

### Task 5: Herunterladen, Prüfen, Austauschen, Neustart

**Files:**
- Create: `crates/planora-update/src/install.rs` (Download, Prüfsumme, Austausch, Rückgängig), `crates/planora-update/src/sha256.rs` (nur falls keine vorhandene Abhängigkeit mit passender Lizenz eine SHA-256 liefert; sonst vorhandene verwenden und Wahl im Bericht begründen), `crates/planora-app/src/update_install.rs` (Ablauf im Programm), Startargument `--nach-update` in `crates/planora-app/src/main.rs`
- Modify: `crates/planora-app/src/app.rs` (Rückfrage vor Neustart über den Weg von `close_requested`, Fortschrittsanzeige, Hinweis nach dem Update), `crates/planora-app/src/update_dialog.rs` (Fortschritt, Abbrechen)
- Test: `crates/planora-update/tests/install.rs`, `crates/planora-app/tests/update_install.rs`

**Interfaces:**
- Produces in `planora_update::install`: `trait Downloader { fn download(&self, url: &str, dest: &Path, max_bytes: u64, progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<(), InstallError>; }` (Rückgabe `false` im Fortschrittsrückruf = abbrechen; Stillstandsgrenze 30 s; nur HTTPS-URLs aus `Release`); `fn verify_sha256(file: &Path, checksum_file_contents: &str, expected_name: &str) -> Result<(), InstallError>` (Format `<hex>  <name>` oder nur `<hex>`; Groß-/Kleinschreibung egal; Name muss passen); `struct Paths { running_exe: PathBuf, new_exe: PathBuf, old_exe: PathBuf, ok_marker: PathBuf }` (`old = <exe>.old`, `ok_marker = planoracad.update-ok` im Ordner der exe, `new = <exe>.new`); `fn swap(paths: &Paths) -> Result<SwapState, InstallError>` (laufende exe → `.old`, `.new` → Originalname; bei Fehler im zweiten Schritt wird `.old` zurückbenannt) und `fn rollback(paths: &Paths) -> Result<(), InstallError>` und `fn cleanup_old(paths: &Paths)` (Fehler ignorieren); `fn exe_dir_writable(dir: &Path) -> bool`; `enum InstallError { Cancelled, Network(String), TooLarge, Checksum, NotWritable, Io(String) }` mit deutscher `message()`.
- Produces in `planora_app::update_install`: `struct Installer` (Zustand: `Idle`, `Downloading{done,total}`, `ReadyToRestart`, `Failed(msg)`), `start(update: &Update, paths: Paths, downloader: Arc<dyn Downloader+Send+Sync>)` im Hintergrund-Thread; `poll()`; `cancel()`; Ablauf nach `ReadyToRestart`: Rückfrage (wie `close_requested`) → bei „Abbrechen“ Update abbrechen und `.new` löschen; sonst `set_pending_update(Some((from,to)))`, `swap`, neue exe mit `--nach-update` starten, bis zu 10 s auf `planoracad.update-ok` warten, bei Erfolg beenden (`ViewportCommand::Close` ohne weitere Rückfrage), bei Ausbleiben `rollback`, `set_skipped_version(Some(to))`, Fehlermeldung. Beim Start mit `--nach-update` oder mit `update_pending`, dessen `to` der laufenden Version entspricht: Marker schreiben, `update_pending` löschen, `cleanup_old`, einmaligen Hinweis „Aktualisiert auf Version X“ mit den Patchnotes dieser Version (aus dem Release-Cache der letzten Prüfung oder, wenn nicht vorhanden, nur die Zeile); findet eine Instanz ein `update_pending`, dessen `to` **nicht** die laufende Version ist, löscht sie den Eintrag und versucht es nicht erneut. Nur Windows: `#[cfg(windows)]` für das Starten/Beenden; sonst zeigt „Jetzt aktualisieren“ den Hinweis „Automatisches Aktualisieren gibt es nur unter Windows.“ und öffnet die Release-Seite.
- Nicht beschreibbarer Ordner: Meldung „Das Programm liegt in einem geschützten Ordner. Lade die neue Version von GitHub herunter.“ mit Knopf „Download-Seite öffnen“ (`webbrowser` ist vorhanden).

- [ ] **Step 1: Tests (`install.rs`, nur temporäre Ordner, Nachbildung von `Downloader`):** `verify_accepts_matching_checksum_in_both_formats`, `verify_rejects_wrong_hash_wrong_name_and_garbage`, `swap_moves_old_aside_and_new_into_place`, `swap_rolls_back_when_second_step_fails` (z. B. Ziel schreibgeschützt/Platzhalter), `rollback_restores_old`, `cleanup_old_ignores_errors`, `not_writable_dir_is_reported`, `download_is_cancelled_by_progress_callback_and_leaves_no_file`, `download_over_max_bytes_is_rejected`, `stalled_download_times_out` (Nachbildung). (`update_install.rs`, ganze App mit Nachbildungen): `wrong_checksum_never_swaps_and_shows_message`, `prompt_for_unsaved_work_before_restart_and_cancel_aborts_the_update_and_deletes_new_file`, `successful_flow_sets_pending_swaps_and_requests_restart` (Neustart über eine austauschbare Schnittstelle `Restarter`, in Tests Nachbildung), `missing_ok_marker_triggers_rollback_and_skips_version`, `startup_with_matching_pending_shows_notice_once_and_cleans_up`, `startup_with_stale_pending_clears_it_and_does_not_retry`, `non_windows_shows_hint_and_opens_release_page` (über `cfg`-freie Prüfung der Entscheidung), `cancel_during_download_returns_to_idle`, `ui_stays_responsive_state_machine_progress`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.** Echte Netz-/Neustart-Implementierungen (`HttpDownloader`, `ProcessRestarter`) sind dünn und werden nur im echten Programm angelegt.
- [ ] **Step 4:** `cargo test -p planora-update -p planora-app` (voll), clippy, fmt, `cargo deny check`.
- [ ] **Step 5: Commit** `feat(update): Herunterladen, Prüfen, Austauschen und Neustart`.
- [ ] **Sichtprüfung/Echtprobe (Controller, mit Dennis' Go):** Testversion A (`0.0.1-dev.1`) aktualisiert auf Testversion B (`0.0.2-dev.1`) gegen eine **lokale** Test-Quelle (kleiner lokaler Server oder Dateien), nie gegen das echte Repository; Rückfrage, Fortschritt, Neustart, Hinweis nach dem Update, Rücksetzen bei kaputter Prüfsumme.

### Task 6: Veröffentlichen – Ablauf, Changelog, Lizenzhinweise

**Files:**
- Create: `.github/workflows/release.yml`, `CHANGELOG.md`, `tools/release.ps1` (Hilfe für Dennis: Version erhöhen, prüfen, Tag vorschlagen, kein Veröffentlichen), `tools/changelog-section.ps1` (liest den Abschnitt einer Version), `tools/third-party-licenses.ps1` (erzeugt `THIRD-PARTY-LICENSES.txt` aus `cargo metadata` + Lizenztexten der Bibliotheken und Schriften unter `crates/planora-app/assets/fonts`; mit festem Ausgabeformat), `docs/superpowers/` nicht berühren
- Modify: `README.md` (Abschnitt „Updates und Versionen“), `docs/superpowers/offene-punkte.md` (Abschnitt „Update-Funktion“)
- Test: `crates/planora-update/tests/release_meta.rs` (liest `CHANGELOG.md` und `Cargo.toml` des Repositorys), Skripte werden per Probelauf im Bericht belegt

**Interfaces / Verhalten:**
- `release.yml`: Auslöser `push: tags: ['v*']`; Schritte: Version aus Tag lesen; **abbrechen**, wenn `workspace.package.version` ≠ Tag ohne `v` oder `CHANGELOG.md` keinen Abschnitt `## <Version>` hat; `cargo test --workspace`, `cargo deny check`; `cargo build --release -p planora-app` auf `windows-latest`; Datei als `planoracad-windows-x64.exe` kopieren; `planoracad-windows-x64.sha256` mit `<hex>  planoracad-windows-x64.exe`; `THIRD-PARTY-LICENSES.txt` erzeugen; Release anlegen mit Abschnitt aus `CHANGELOG.md` als Beschreibung, `prerelease` bei Endung `-dev`/`-beta`; alle drei Dateien anhängen. Berechtigung `contents: write` nur in diesem Ablauf.
- `CHANGELOG.md`: Abschnitte `## 0.2.0-dev.1` usw. (neueste oben), Unterüberschriften `### Neu`, `### Verbessert`, `### Behoben`, Aufzählungen in einfacher Sprache; ein erster Abschnitt für die Startversion (Inhalt: Überblick über den heutigen Stand aus README/offene-punkte, von Claude geschrieben).
- Versionsnummer: Task 6 setzt `workspace.package.version` auf `0.2.0-dev.1`; Task 1 muss `-dev.N` als gültige Cargo-Version akzeptieren (ja, SemVer-Vorabversion).
- Test `release_meta.rs`: `cargo_version_parses_with_planora_update` (liest Cargo.toml, parst), `changelog_has_a_section_for_the_current_version`, `changelog_sections_are_newest_first_and_versions_parse`.

- [ ] **Step 1:** Test schreiben, Fehlschlag sehen (Changelog fehlt).
- [ ] **Step 2: Implementieren.** Skripte unter PowerShell 5.1-kompatiblem Stil; Probelauf von `third-party-licenses.ps1` und `changelog-section.ps1` lokal; `release.yml` mit `actionlint`-ähnlicher Sorgfalt prüfen (nicht ausführbar im Test; Trockenlauf-Anleitung in der README).
- [ ] **Step 3:** `cargo test -p planora-update`, fmt/clippy unverändert grün; Lizenzdatei prüfen (enthält jede Bibliothek der Windows-Build-Liste und die drei Schriftlizenzen).
- [ ] **Step 4: Commit** `feat(release): Ablauf zum Veröffentlichen, Changelog, Lizenzhinweise`.
- [ ] **Controller (nur auf Dennis' ausdrückliches Wort):** Tag `v0.2.0-dev.1` setzen und pushen; Release prüfen; erste echte Aktualisierung von einer Testversion aus.

### Task 7: Aufräumen und Dokumentation

**Files:**
- Modify: `docs/superpowers/specs/2026-10-25-update-funktion-design.md` (tatsächliche Ergebnisse), `docs/superpowers/offene-punkte.md`, `README.md`, Plan-Abschnitt „Abweichungen“; tote Hilfsfunktionen entfernen
- Test: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check`

- [ ] **Step 1:** Suche nach ungenutztem Code, abgelehnten Zwischenlösungen und Überresten; entfernen.
- [ ] **Step 2:** Dokumentation in einfacher Sprache (wie es funktioniert, Datenschutz-Hinweis, was nicht geht: signierte Dateien, Linux/macOS, Downgrade); Liste der zurückgestellten Kleinigkeiten aus den Prüfungen.
- [ ] **Step 3:** Alle Prüfungen laufen lassen, Ergebnisse im Bericht.
- [ ] **Step 4: Commit** `docs: Update-Funktion`.

## Abweichungen

Was vom Plan abgewichen ist und warum.

- **Aufgabe 1:** `Version::current` gibt es nicht. Die Bibliothek kennt nur `Version::from_cargo(&str)`; die laufende Version steht als Konstante in `planora-app` (`env!("CARGO_PKG_VERSION")` in einer Bibliothek wäre deren eigene Versionsnummer, nicht die des Programms). Eine Antwort, die keine Liste ist, ergibt den Fehler `NotAList`.
- **Aufgabe 1/Wurzel-`Cargo.toml`:** Die Arbeitsbereichsversion blieb bis Aufgabe 6 bei 0.1.0 (sie gilt für alle Crates) und wurde dort auf 0.2.0-dev.1 gesetzt.
- **Aufgabe 3:** `CheckOutcome::Available` trägt ein `Box<Update>` (Clippy-Regel `large_enum_variant`). Eine von Hand gestartete Prüfung zeigt zusätzlich den Hinweis „Neue Version gefunden“. Die Höhe der Tastentabelle richtet sich nach der Fensterhöhe statt fest 320 Pixel.
- **Aufgabe 4:** Esc und das Kreuz bedeuten „Später“, nie „Überspringen“. Andere Fenster schließen die Meldung, ohne dass es als „Später“ zählt.
- **Aufgabe 5:** Ergebnis der Prüfung war „Überarbeiten“, dann nachgebessert: Neustart nur in ruhigen Momenten, Prüfsumme noch einmal vor dem Austausch, Frist 30 s statt 10 s, Rückmeldung mit Versionsnummer, `.new` bleibt, wenn die Programmdatei fehlt, `args_os` statt `args`. Die Änderungsnotizen werden in einer Datei übergeben (kein Zwischenspeicher vorhanden). SHA-256 ist selbst geschrieben. Dazu der Testschalter `update-test-source` für die lokale Quelle (im Plan nicht vorgesehen).
- **Aufgabe 5b (neu eingefügt):** Die GitHub-CI war schon auf `master` rot (Linux: Übersetzungsfehler in einem Test; Windows: unzuverlässiger Test). Das Beheben kostete eine eigene Aufgabe mit drei Pushes auf den Zweig. Danach wollte Dennis die GitHub-CI nur noch vor einem Release; seitdem wird nichts mehr zum Auslösen gepusht. Gefunden und behoben wurden Fehler in Tests, nicht im Programm; die Vergleichsdateien für Zeichnungen werden jetzt mit einer eng begrenzten Toleranz für Rundungsrauschen verglichen (`planora-drawing/tests/common/golden.rs`, mit eigenen Tests).
- **Aufgabe 6:** `ci.yml` läuft nur von Hand (`workflow_dispatch`), nicht bei Push und Pull-Request wie zuerst geplant. `release.yml` führt die vollen Tests nicht aus; die Prüfung vor dem Veröffentlichen macht `tools/release.ps1` (verlangt einen grünen Lauf von `ci.yml` für genau diesen Commit). Zusätzlich gibt es `tools/changelog-section.ps1`, `tools/third-party-licenses.ps1` und `tools/license-texts/`. Der Changelog hat keinen leeren Abschnitt „Behoben“.
- **Aufgabe 7:** Die Aktionen in den Abläufen sind auf feste Commits statt Tags gesetzt; `release.yml` hat zwei Aufträge (`build` ohne Schreibrecht, `release` nur bei einem Tag); die Versionssuche liest zeilenweise statt mit einem langen Muster; `tools/release.ps1` prüft zusätzlich, ob der Tag schon existiert und ob `Cargo.lock` aktuell ist; der Test auf leere Abschnitte im Changelog prüft jetzt auch die Unterabschnitte. Das Weiterleiten-Limit der Listenabfrage und die anderen kleinen Punkte aus den Prüfungen wurden bewusst nicht umgebaut, sondern in `docs/superpowers/offene-punkte.md` aufgeführt.
- **Nachbesserung nach der Gesamtprüfung:** Die neue Version meldet sich erst nach ihrem ersten fertig gezeichneten Bild (vorher: gleich nach dem Einrichten der Grafik); `.old` löscht erst der nächste normale Start. Der Neustart behält einen Griff auf die neue Version (`NewProcess`) und beendet sie, bevor er zurücklegt. Während Prüfung und Warten ist die Bedienung gesperrt; neue ungespeicherte Arbeit führt zurück zur Rückfrage oder zum Abbruch des Neustarts, nie zum Schließen ohne Rückfrage. Außerhalb von Windows meldet die Suche „Auf diesem System gibt es keine Aktualisierung über das Programm“ statt „kein Netz?“. `ci.yml` hat nur Leserecht.
- **Nicht ausgeführt:** Die Arbeit lief ohne einen einzigen GitHub-Lauf der neuen Abläufe (`release.yml` ist nur lokal geprüft; die Schritte `gh release create`, Rust-Cache und `cargo install cargo-deny` auf GitHub sind ungeprüft).
