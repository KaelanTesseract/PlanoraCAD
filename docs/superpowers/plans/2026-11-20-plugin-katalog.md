# Plug-in-Katalog – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Das Fenster „Zusatzmodule“ bekommt einen Reiter „Katalog“, der die Plug-ins aus Dennis' Katalog-Repo zeigt und über den bestehenden Repo-Link-Weg installiert/aktualisiert; beim ersten Start weist das Programm einmal darauf hin.

**Architecture:** `planora-update/src/catalog.rs` liest und prüft `katalog.json` und die Release-Antwort von GitHub (ohne Netz, rein). `planora-app/src/plugin_catalog.rs` holt Liste und Symbole in einem `Job`, hält Zustand und Zwischenspeicher. `addins_window.rs` zeichnet die Reiter; „Installieren“ ruft `Plugins::install_from_link`.

**Tech Stack:** Rust 2024, egui 0.36, serde_json, bestehende `planora_update::plugin_source::download`/`redirect_target`, `Job` (crates/planora-app/src/jobs.rs). Keine neuen Abhängigkeiten.

**Spec:** `docs/superpowers/specs/2026-11-20-plugin-katalog-design.md`

## Global Constraints

- Katalog-Repo fest: Besitzer `KaelanTesseract`, Repo `PlanoraCAD-Plugins` (Konstanten `CATALOG_OWNER`, `CATALOG_REPO`), nicht in den Einstellungen änderbar.
- `katalog.json` aus dem **neuesten veröffentlichten Release** (`https://api.github.com/repos/KaelanTesseract/PlanoraCAD-Plugins/releases/latest`), Grenzen: Release-Antwort ≤ 4 MiB (`MAX_RELEASES_BYTES`), `katalog.json` ≤ 1 MiB, Symbol ≤ 256 KiB (PNG oder SVG), höchstens 100 Symbole, höchstens 500 Einträge; Weiterleitungen nur nach den Etappe-1-Regeln (`redirect_target`).
- `format` = 1; höher ⇒ Hinweis „Katalog braucht eine neuere PlanoraCAD-Version“, nichts anzeigen.
- Kategorien mit Übersetzung: `elektronik`, `import-export`, `werkzeuge`, `beispiele`; sonst „Sonstiges“.
- Laden beim Öffnen des Reiters (höchstens einmal je 10 min) und bei der täglichen Update-Prüfung (Schalter `config.update_check`).
- Zwischenspeicher: `<places.cache>/katalog/` (`katalog.json`, Symbole, `stand.json` mit Zeitpunkt in Sekunden seit 1970).
- Erster Start: Config-Feld `catalog_hint_shown: bool`, nachsichtig, Vorgabe false, nur `true` geschrieben.
- Alle Oberflächentexte in `de.ftl` und `en.ftl`; keine deutschen Literale im Code; Verhalten wie Fusion.
- Tests nie mit echtem Netz/echtem AppData; Netz über die austauschbare `RepoFetch` (`crates/planora-app/src/plugin_repo.rs:26`).
- Volle Testläufe mit `CARGO_INCREMENTAL=0` und `CARGO_PROFILE_DEV_DEBUG=0`; WSL danach aufräumen (`/root/target-s6`, `/root/PlanoraCAD/target`, `df -h /`, vorher `pgrep -af 'cargo|rustc|wsl-s6'`).
- Handtests nur per Doppelklick-Start (Claude-gestartete Prozesse sehen ein virtualisiertes AppData); vor jedem Handtest Dennis fragen.
- Kein Push; Merge nur auf „bitte mergen“. Format/API-Änderungen an `katalog.json` der Session „PlanoraCAD Plugin-Pläne“ melden.

## Review Focus

1. Eintrag mit `repo` außerhalb GitHub, mit `..`, mit Leerzeichen oder doppelter `id`: nur dieser Eintrag fällt weg (Test in Task 1).
2. Symbol-Name mit Pfad (`../x.png`, `a/b.png`) oder unbekannter Endung: Standardsymbol, kein Download (Test in Task 1).
3. Netz weg nach einem erfolgreichen Laden: Liste vom Zwischenspeicher mit „offline – Stand vom …“, „Installieren“ meldet den Netzfehler verständlich (Test in Task 2).
4. Kaputter Zwischenspeicher (abgeschnittenes JSON): wird wie „nie geladen“ behandelt, kein Absturz (Test in Task 2).
5. Katalog-Eintrag eines bereits installierten Plug-ins mit höherer Version im Plug-in-Repo: Reiter zeigt „Aktualisieren auf …“, nicht „Installieren“ (Test in Task 3).

---

### Task 1: Katalog lesen und prüfen (`planora-update`)

**Files:**
- Create: `crates/planora-update/src/catalog.rs`, `crates/planora-update/tests/catalog.rs`
- Modify: `crates/planora-update/src/lib.rs`

**Interfaces:**
- Consumes: `plugin_source::parse_repo_link(&str) -> Result<RepoLink, String>`, `release::AssetRef { name, url, size }`.
- Produces:
  - `pub const CATALOG_OWNER: &str = "KaelanTesseract"; pub const CATALOG_REPO: &str = "PlanoraCAD-Plugins";`
  - `pub const MAX_CATALOG_BYTES: u64 = 1 << 20; pub const MAX_SYMBOL_BYTES: u64 = 256 << 10; pub const MAX_SYMBOLS: usize = 100; pub const MAX_ENTRIES: usize = 500; pub const CATALOG_FORMAT: u32 = 1;`
  - `pub fn latest_release_url() -> String` (= `https://api.github.com/repos/KaelanTesseract/PlanoraCAD-Plugins/releases/latest`)
  - `pub struct CatalogRelease { pub tag: String, pub catalog: AssetRef, pub symbols: Vec<AssetRef> }`; `pub fn parse_latest_release(json: &str) -> Result<CatalogRelease, String>` (Asset `katalog.json` Pflicht; Symbole = übrige Assets mit Endung `.png`/`.svg` und `size <= MAX_SYMBOL_BYTES`, höchstens `MAX_SYMBOLS`; jede Asset-URL muss genau `https://github.com/KaelanTesseract/PlanoraCAD-Plugins/releases/download/<tag>/<name>` sein, sonst übergangen; Entwurf ⇒ Err).
  - `pub struct CatalogEntry { pub id: String, pub name: BTreeMap<String, String>, pub description: BTreeMap<String, String>, pub category: String, pub repo: RepoLink, pub symbol: Option<String> }` mit `pub fn name_in(&self, lang: &str) -> &str` und `pub fn description_in(&self, lang: &str) -> &str` (Sprache → `en` → erster Eintrag → leer).
  - `pub enum CatalogError { TooNew(u32), Invalid(String) }`; `pub fn parse_catalog(json: &str) -> Result<(Vec<CatalogEntry>, Vec<String>), CatalogError>` – zweiter Wert: Gründe übergangener Einträge (für das Protokoll).

- [ ] **Step 1: Failing Tests** in `tests/catalog.rs`:
  - `parses_example`: das JSON aus der Spec (Eintrag `planora.elektronik`) ⇒ ein Eintrag, `repo.owner/repo` richtig, `name_in("de") == "Elektronik"`, `name_in("fr") == "Electronics"`.
  - `format_too_new`: `"format": 2` ⇒ `Err(TooNew(2))`; fehlendes `format` oder kein Objekt ⇒ `Err(Invalid(_))`.
  - `bad_entries_skipped` (Review Focus 1): Einträge mit `repo` `https://gitlab.com/a/b`, `https://github.com/a/../b`, `"https://github.com/a/b c"`, ohne `id`, mit id-Form gegen die Manifest-Regel (`^[a-z0-9]+(\.[a-z0-9-]+)+$`), zweiter Eintrag mit gleicher `id` ⇒ nur die gültigen bleiben, `skipped.len()` stimmt.
  - `symbol_names_checked` (Review Focus 2): `symbol` `../x.png`, `a/b.png`, `x.gif`, `""` ⇒ `symbol == None`; `logo.svg` ⇒ `Some`.
  - `too_many_entries`: 501 gültige ⇒ genau 500 übernommen, ein Grund in `skipped`.
  - `unknown_fields_ignored`.
  - `release_parse`: Beispiel-Antwort mit `tag_name: "katalog-1"`, Assets `katalog.json`, `logo.png` (1000 B), `riesig.png` (300 KiB), `fremd.png` mit URL in anderem Repo ⇒ `catalog` gesetzt, `symbols == [logo.png]`; ohne `katalog.json` ⇒ Err; `draft: true` ⇒ Err.
- [ ] **Step 2: FAIL prüfen** (`cargo test -p planora-update --test catalog`).
- [ ] **Step 3: `catalog.rs` implementieren** (serde_json mit `Value`, nachsichtig je Eintrag; Fehlertexte über `tr!` mit neuen Ids `catalog-*` in de/en).
- [ ] **Step 4: PASS**, `cargo test -p planora-update -p planora-i18n`, clippy.
- [ ] **Step 5: Commit** `Plug-in-Katalog Task 1: Katalog lesen und prüfen`.

### Task 2: Katalog holen, Zustand, Zwischenspeicher (`planora-app`)

**Files:**
- Create: `crates/planora-app/src/plugin_catalog.rs`, `crates/planora-app/tests/plugin_catalog.rs`
- Modify: `crates/planora-app/src/plugins.rs` (Feld `catalog: Catalog`, Aufruf aus `frame`/Update-Prüfung), `crates/planora-app/src/lib.rs`

**Interfaces:**
- Consumes: Task 1; `RepoFetch`, `Plugins::repo_fetch`, `Job`, `places.cache`.
- Produces:
  - `pub enum CatalogState { Never, Loading, Ready { entries: Vec<CatalogEntry>, symbols: BTreeMap<String, Vec<u8>>, fetched_at: u64, offline: bool }, TooNew(u32), Failed(String) }`
  - `pub struct Catalog { … }` mit `pub fn new(cache_dir: PathBuf) -> Catalog` (liest Zwischenspeicher; kaputt ⇒ `Never`), `pub fn state(&self) -> &CatalogState`, `pub fn request(&mut self, fetch: RepoFetch, now: u64, force: bool)` (startet Job, wenn `force` oder letzter Versuch ≥ 600 s her und nicht `Loading`), `pub fn poll(&mut self, now: u64) -> bool` (true = Zustand geändert; Erfolg ⇒ `Ready{offline:false}` und Zwischenspeicher atomar schreiben; Netzfehler mit vorhandenem Stand ⇒ derselbe Stand mit `offline: true`; ohne Stand ⇒ `Failed(text)`).
  - Job-Ablauf: `latest_release_url()` (≤ `MAX_RELEASES_BYTES`, 30 s) → `parse_latest_release` → `katalog.json` (≤ 1 MiB) → `parse_catalog` → nur die Symbole laden, die ein Eintrag nennt und die im Release liegen (je ≤ 256 KiB). Übergangene Einträge ⇒ `diag::warn`.
  - In `Plugins`: `pub catalog: Catalog`; tägliche Prüfung (`check_updates(false)`) ruft zusätzlich `catalog.request(…, force: false)`.

- [ ] **Step 1: Failing Tests** (Nachbildung `RepoFetch` mit Antworten aus Testdaten; Temp-Cacheordner):
  - `loads_and_caches`: Erfolg ⇒ `Ready`, `offline == false`, Dateien im Cacheordner; neuer `Catalog::new` auf denselben Ordner ⇒ `Ready` mit denselben Einträgen.
  - `offline_uses_cache` (Review Focus 3): erst Erfolg, dann Netzfehler mit `force` ⇒ `Ready{offline: true}` mit altem `fetched_at`.
  - `never_loaded_fails`: Netzfehler ohne Zwischenspeicher ⇒ `Failed(_)`.
  - `broken_cache_is_never` (Review Focus 4): abgeschnittenes `katalog.json` im Cache ⇒ `Never`.
  - `throttled_ten_minutes`: zweiter `request` nach 300 s ohne `force` startet keinen Job (Zähler in der Nachbildung), nach 601 s schon.
  - `too_new_format`: `format: 2` ⇒ `TooNew(2)`, Zwischenspeicher unverändert.
  - `only_named_symbols_loaded`: Release mit drei Symbolen, Katalog nennt eines ⇒ genau ein Symbol-Download.
  - `daily_check_requests_catalog`: `check_updates(false)` mit `update_check` an ⇒ ein Katalog-Abruf; aus ⇒ keiner.
- [ ] **Step 2: FAIL prüfen**, **Step 3: implementieren**, **Step 4: PASS** (`cargo test -p planora-app --test plugin_catalog`), **Step 5: Commit** `Plug-in-Katalog Task 2: Katalog holen und zwischenspeichern`.

### Task 3: Reiter „Katalog“ im Fenster „Zusatzmodule“

**Files:**
- Modify: `crates/planora-app/src/addins_window.rs`, `crates/planora-plugin-host/src/places.rs` (`Provenance.catalog: bool`, nachsichtig, Vorgabe false), `crates/planora-app/src/plugins.rs` (`install_from_catalog(&mut self, entry: &CatalogEntry)` = `install_from_link(&entry.repo.web_url())` plus `catalog: true` in der Herkunft), `crates/planora-i18n/languages/{de,en}.ftl`
- Test: `crates/planora-app/tests/addins_window.rs` (bestehend), neue Tests dort

**Interfaces:**
- Consumes: Task 2 (`Plugins::catalog`), `Plugins::updates()`, `PluginHost::entries()`.
- Produces: `pub enum AddinsTab { Installed, Catalog }`; `AddinsWindow::open_at(&mut self, tab: AddinsTab)`; `AddinsWindow::tab(&self) -> AddinsTab`.

Verhalten: Reiter oben („Installiert“, „Katalog“), Wechsel auf „Katalog“ ruft `catalog.request(…, force: false)`. Katalog-Ansicht: oben Suchfeld (Name und Beschreibung, ohne Groß/Klein, aktuelle Sprache) und Auswahl Kategorie („Alle“ + vorkommende Kategorien, übersetzt bzw. „Sonstiges“); links Liste (Symbol 24 px oder Standardsymbol, Name, Kategorie); rechts Details: Name, Beschreibung, Kategorie, Quelle (`repo.web_url()`), neueste Version (aus `updates()` bzw. „—“), Zeile „Rechte werden beim Installieren angezeigt“, Knopf je Zustand: nicht installiert ⇒ „Installieren“; installiert ohne Update ⇒ „Installiert“ (aus); Update vorhanden ⇒ „Aktualisieren auf { $version }“ (öffnet den bestehenden Aktualisieren-Dialog). Zustände `Loading` ⇒ „Katalog wird geladen…“; `Failed` ⇒ „Der Katalog konnte nicht geladen werden“ + Text + „Erneut versuchen“ (`force: true`); `offline` ⇒ Hinweis „offline – Stand vom { $datum }“ (Datum über bestehende Ortszeit-Formatierung `local_time.rs`); `TooNew` ⇒ „Der Katalog braucht eine neuere PlanoraCAD-Version.“ Im Reiter „Installiert“ zeigt die Herkunft bei `catalog: true` „aus dem Katalog“.

- [ ] **Step 1: Failing Tests:** `catalog_tab_lists_entries` (Ready mit zwei Einträgen ⇒ beide Namen sichtbar), `search_filters` („elek“ ⇒ nur Elektronik), `category_filter`, `install_button_starts_link_install` (Klick ⇒ `Plugins` ist im Zustand „holt“ für `https://github.com/KaelanTesseract/PlanoraCAD-Plugin-Hallo`), `installed_shows_installed`, `update_shows_update_button` (Review Focus 5), `failed_shows_retry` (Klick ⇒ neuer Abruf), `offline_hint_shown`, `too_new_hint_shown`, `catalog_provenance_shown` (Installiert-Reiter zeigt „aus dem Katalog“), `layout_1920x1080_fits` (bestehendes Muster der Fenster-Layouttests).
- [ ] **Step 2–4:** FAIL prüfen, implementieren, PASS (`cargo test -p planora-app --test addins_window`, `cargo test -p planora-plugin-host`, `cargo test -p planora-i18n`).
- [ ] **Step 5: Commit** `Plug-in-Katalog Task 3: Reiter Katalog`.

### Task 4: Hinweis beim ersten Start

**Files:**
- Modify: `crates/planora-project/src/config.rs` (`catalog_hint_shown`), `crates/planora-app/src/plugins.rs` oder `plugin_prompts.rs` (Hinweis), `crates/planora-app/src/app.rs` (anstoßen nach dem Start), de/en.ftl
- Test: `crates/planora-app/tests/plugins.rs`, Config-Tests in `planora-project`

**Interfaces:** Consumes `AddinsWindow::open_at(AddinsTab::Catalog)` (Task 3).

Verhalten: Ist `catalog_hint_shown == false`, erscheint nach dem Start (wenn keine andere Rückfrage/kein Absturzdialog offen ist, im Stil der bestehenden Hinweise, z. B. Update-Hinweis) einmal: Titel „Zusatzmodule“, Text „Es gibt Zusatzmodule für PlanoraCAD, z. B. Elektronik. Jetzt ansehen?“, Knöpfe „Katalog öffnen“ und „Später“; beide setzen das Feld auf true und speichern die Konfiguration; „Katalog öffnen“ öffnet das Fenster am Reiter Katalog. Escape = „Später“.

- [ ] **Step 1: Failing Tests:** `hint_shown_once` (erster Start ⇒ Hinweis; „Später“ ⇒ Feld true, neuer Start ⇒ kein Hinweis), `hint_opens_catalog`, `hint_waits_for_other_prompt` (offene Rückfrage ⇒ Hinweis erst danach), `config_catalog_hint_roundtrip` (fehlt ⇒ false; `false` nicht geschrieben; `true` geschrieben; `"ja"` ⇒ false).
- [ ] **Step 2–4:** FAIL, implementieren, PASS (`cargo test -p planora-app --test plugins`, `cargo test -p planora-project`).
- [ ] **Step 5: Commit** `Plug-in-Katalog Task 4: Hinweis beim ersten Start`.

### Task 5: Doku, volle Prüfung, Handtest vorbereiten

**Files:** `docs/plug-ins.md`, `docs/plugins/entwickler.md` (Abschnitt „In den Katalog“: Regeln für Plug-in-Repos und `katalog.json`, Verweis auf die Session „PlanoraCAD Plugin-Pläne“ als Pflegerin), `README.md`, `README.en.md`, `docs/superpowers/offene-punkte.md`

- [ ] **Step 1: Doku** schreiben (Deutsch, README.en.md englisch, Plattformen Windows und Ubuntu).
- [ ] **Step 2: Windows:** `cargo test --workspace --no-fail-fast`, `cargo clippy --workspace --all-targets`, `cargo deny check licenses bans sources` ⇒ 0 failed, keine Warnungen.
- [ ] **Step 3: Ubuntu (WSL):** `cargo test -p planora-update -p planora-plugin-host -p planora-project -p planora-app --no-fail-fast` ⇒ 0 failed außer dem bekannten root-Vorschaubild-Test; danach WSL aufräumen.
- [ ] **Step 4: Release bauen** (`cargo build --release -p planora-app`).
- [ ] **Step 5: Commit** `Plug-in-Katalog Task 5: Doku und Prüfung`; Vault-Notiz.
- [ ] **Step 6 (Controller):** Dennis fragen, ob er den Handtest per Doppelklick macht: Hinweis beim ersten Start (vorher `catalog_hint_shown` im echten config.json fehlt), Reiter Katalog zeigt „Beispiel-Hallo“ aus dem echten Katalog-Repo, Installieren über echtes GitHub (`KaelanTesseract/PlanoraCAD-Plugin-Hallo` v0.1.0), Prüfsumme, Herkunft „aus dem Katalog“, offline-Anzeige (Netz kurz trennen).
