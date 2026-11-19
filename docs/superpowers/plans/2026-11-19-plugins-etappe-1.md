# Plug-ins Etappe 1: Grundgerüst – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** PlanoraCAD lädt abgeschottete WebAssembly-Plug-ins aus `.pcplugin`-Dateien oder über den Link ihres GitHub-Repos (und hält sie darüber aktuell), fragt Rechte beim ersten Gebrauch ab, zeigt ihre Befehle im Menü „Hilfsprogramme“ und einfache Dialoge, und überlebt kaputte Plug-ins.

**Architecture:** Neues Crate `planora-plugin-api` (nur WIT), neues Crate `planora-plugin-host` (wasmtime ohne egui: Paket, Laufzeit, Grenzen, Rechte, Fundorte), Oberflächenseite in `planora-app` (Verwaltung im `Workspace`, Menü, Dialoge, Rechte-Nachfrage, Einstellungsabschnitt). Plug-ins sind Rust-Komponenten für `wasm32-wasip2` in einem eigenen Workspace `plugins/`; ihre `.wasm` liegen als Testdaten im Repo.

**Tech Stack:** Rust 2024, wasmtime + wasmtime-wasi (Component Model, Cranelift), wit-bindgen (nur Plug-ins), toml, semver, sha2, egui 0.36.

**Spec:** `docs/superpowers/specs/2026-11-19-plugin-schnittstelle-design.md`

## Global Constraints

- Sprache Deutsch in Code-Kommentaren, Commits und Doku; alle Oberflächentexte über `planora-i18n` in `de.ftl` und `en.ftl`.
- Verhalten wie Autodesk Fusion („UTILITIES → ADD-INS“ ist das Vorbild für Menü und Verwaltung), kein Autodesk-Material.
- Jede neue Abhängigkeit mit `cargo deny check` prüfen, Lizenz im Kommentar in `Cargo.toml` notieren (Muster: bestehende Einträge in `[workspace.dependencies]`).
- Lizenz aller neuen Crates und Plug-ins: `GPL-3.0-only` (Workspace-Vorgabe).
- Plattformen Windows und Ubuntu; beide Pfade in jedem Fundort-Code und -Test.
- Volle Testläufe nur mit `CARGO_INCREMENTAL=0` und `CARGO_PROFILE_DEV_DEBUG=0`; WSL danach aufräumen (`/root/target-s6`, `/root/PlanoraCAD/target` löschen, `df -h /`; vorher `pgrep -af 'cargo|rustc|wsl-s6'`).
- Kein Push. Merge nur auf „bitte mergen“. Vor jedem Handtest Dennis fragen.
- Zweig `feature/plugins` (ab master 11b9c031).
- Grenzen aus der Spec: Paket ≤ 64 MB, `plugin.wasm` ≤ 32 MB, Speicher pro Plug-in 512 MB (anforderbar bis 2048 MB), Aufruf aus der Oberfläche 200 ms, Befehl und `init` 5 s, nach 3 Fehlern in einer Sitzung aus.
- Unter API `0.x` lädt das Programm nur Plug-ins mit **genau** seiner `0.y` (hier `0.1`).
- Abgeschottet: leerer WASI-Kontext (keine Ordner, keine Umgebung, kein Netz, keine Argumente, stdout/stderr ins Protokoll, Wanduhr fest auf 0, Monotonuhr auf 1 ms gerundet).

## Review Focus

1. Ein installiertes Plug-in hat dieselbe `id` wie ein mitgeliefertes: das höhere `version` läuft, die Verwaltung zeigt beide (Test in Task 4).
2. Bösartige Paketdatei (Pfad mit `..`, absoluter Pfad, 100 000 Einträge, Zip-Bombe, fehlendes `plugin.toml`): abgelehnt mit verständlicher Meldung, nichts entpackt (Tests in Task 1).
3. Das Ergebnis einer Rechte-Nachfrage oder Netzanfrage kommt an, nachdem das Plug-in abgeschaltet oder neu gestartet wurde: Ergebnis verworfen, kein Absturz, kein Aufruf in die neue Instanz (Test in Task 5).
4. Ein Plug-in fällt aus, während sein Dialog offen ist: Dialog schließt, Meldung erscheint, das Dokument bleibt unverändert (Test in Task 6).
5. Kaputtes oder fremdes Feld `plugins` in `config.json`: nachsichtig gelesen, Wurzel und übrige Einstellungen bleiben (Test in Task 4).

---

## Dateien

| Datei | Aufgabe |
|---|---|
| `crates/planora-plugin-api/{Cargo.toml,src/lib.rs,wit/plugin.wit}` | Vertrag; `lib.rs` exportiert nur `pub const WIT_DIR` und `pub const API_VERSION: &str = "0.1"` |
| `crates/planora-exchange/src/zip_read.rs` | ZIP-Leser mit Grenzen (aus `feature/platinen:crates/planora-eda-lib/src/zip.rs` übernommen und öffentlich gemacht) |
| `crates/planora-plugin-host/src/manifest.rs` | `plugin.toml` lesen und prüfen |
| `crates/planora-plugin-host/src/package.rs` | `.pcplugin` lesen, prüfen, entpacken |
| `crates/planora-plugin-host/src/runtime.rs` | Engine, Store, Grenzen, Epochen-Uhr, Aufrufe |
| `crates/planora-plugin-host/src/bindings.rs` | `wasmtime::component::bindgen!` und Host-Seite der Imports |
| `crates/planora-plugin-host/src/rights.rs` | Rechte, Anfragen mit Kennung |
| `crates/planora-plugin-host/src/places.rs` | Fundorte, Installieren, Entfernen, Zwischenspeicher |
| `crates/planora-plugin-host/src/texts.rs` | Fluent-Texte eines Plug-ins |
| `crates/planora-plugin-host/src/lib.rs` | `PluginHost` (alle Plug-ins, Zustand, Ereignis-Postfach) |
| `crates/planora-project/src/config.rs` | Feld `plugins` |
| `crates/planora-app/src/plugins.rs` | Verwaltung im `Workspace`, Menü, Meldungen, Fehlerleiste |
| `crates/planora-app/src/plugin_dialog.rs` | beschriebene Dialoge zeichnen |
| `crates/planora-app/src/plugin_prompts.rs` | Rechte-Nachfrage, Installationsdialog |
| `crates/planora-app/src/settings_window.rs` | Abschnitt „Plug-ins“ |
| `crates/planora-update/src/plugin_source.rs` | Repo-Link lesen, Releases eines Plug-in-Repos wählen und prüfen |
| `plugins/{Cargo.toml,beispiel-hallo,test-faelle}` | Plug-in-Workspace (nicht im Haupt-Workspace) |
| `tools/build-plugins.ps1`, `tools/build-plugins.sh` | Plug-ins bauen, `.wasm` und Quell-Hash nach `crates/planora-plugin-host/tests/data/` |
| `docs/plugins/entwickler.md` | Entwickler-Doku |

---

### Task 1: ZIP-Leser, Manifest und Paket

**Files:**
- Create: `crates/planora-exchange/src/zip_read.rs`, `crates/planora-exchange/tests/zip_read.rs`
- Modify: `crates/planora-exchange/src/lib.rs`, `Cargo.toml` (workspace: `miniz_oxide`, `toml`, `semver`)
- Create: `crates/planora-plugin-host/{Cargo.toml,src/lib.rs,src/manifest.rs,src/package.rs}`, `crates/planora-plugin-host/tests/package.rs`

**Interfaces:**
- Produces:
  - `planora_exchange::zip_read::{ZipRead, ZipReadLimits { max_entries: usize, max_entry: u64, max_total: u64 }, ZipReadError}`; `ZipRead::open(data: &[u8], limits: &ZipReadLimits) -> Result<ZipRead<'_>, ZipReadError>`, `fn len(&self) -> usize`, `fn name(&self, i: usize) -> &str`, `fn read(&mut self, i: usize) -> Result<Vec<u8>, ZipReadError>`.
  - `planora_plugin_host::manifest::{Manifest, Right, ManifestError}`; `Manifest::parse(text: &str) -> Result<Manifest, ManifestError>`. Felder: `id: String, name: String, version: semver::Version, api: String, author: String, license: String, description: String, rights: BTreeSet<Right>, net_hosts: Vec<String>, memory_mb: u32`.
  - `enum Right { Net, FsFolder, ModelRead, ModelWrite }` mit `fn id(self) -> &'static str` (`"net"`, `"fs-folder"`, `"model-read"`, `"model-write"`).
  - `planora_plugin_host::package::{Package, Signature, PackageError}`; `Package::read(data: &[u8]) -> Result<Package, PackageError>`; Felder `manifest: Manifest, wasm: Vec<u8>, languages: BTreeMap<String, String>` (Sprach-Id → FTL-Text), `icons: BTreeMap<String, Vec<u8>>`, `signature: Signature` (`enum Signature { None, Present }`); `Package::unpack_to(&self, dir: &Path) -> io::Result<()>`.
  - Konstanten in `package.rs`: `MAX_PACKAGE: u64 = 64 << 20`, `MAX_WASM: u64 = 32 << 20`, `MAX_ENTRIES: usize = 1000`.

- [ ] **Step 1: ZIP-Leser übernehmen.** Inhalt von `git show feature/platinen:crates/planora-eda-lib/src/zip.rs` nach `zip_read.rs`, eigene Fehlerart `ZipReadError` (thiserror) statt `ImportError`, `pub` statt `pub(crate)`. Tests aus `feature/platinen:crates/planora-eda-lib/tests/import.rs`, soweit sie den ZIP-Leser betreffen, nach `tests/zip_read.rs`, dazu Testpaare mit `ZipWriter` aus `planora-exchange` (gespeichert und deflate). Kommentar oben: „Vorhaben 2 stellt `planora-eda-lib` auf diesen Leser um.“
- [ ] **Step 2: Tests laufen lassen.** `cargo test -p planora-exchange --test zip_read` → PASS.
- [ ] **Step 3: Failing Tests für das Manifest schreiben** in `tests/package.rs`:
  - `manifest_ok`: vollständiges Beispiel mit `id = "planora.beispiel-hallo"`, `version = "0.1.0"`, `api = "0.1"`, `license = "GPL-3.0-only"`, `rights = ["net"]`, `net-hosts = ["example.org"]` → `memory_mb == 512`, `rights == {Right::Net}`.
  - `manifest_rejects`: je eine Fehlermeldung für: `id` mit Großbuchstaben, ohne Punkt oder länger als 64 Zeichen (Muster `^[a-z0-9]+(\.[a-z0-9-]+)+$`); `version = "1"`; unbekanntes Recht `"kamera"`; `net` ohne `net-hosts`; `net-hosts` ohne `net`; Host mit Schema oder Pfad (`"https://x.org"`, `"x.org/a"`); `memory-mb = 0` und `= 2049`; leeres `license`.
- [ ] **Step 4: `Manifest::parse` implementieren** (toml mit `serde`, Schlüssel in kebab-case, `deny_unknown_fields` **nicht** setzen, damit neuere Pakete lesbar bleiben). `cargo test -p planora-plugin-host --test package manifest` → PASS.
- [ ] **Step 5: Failing Tests für das Paket schreiben:** `package_ok` (mit `ZipWriter` gebaut: `plugin.toml`, `plugin.wasm`, `languages/de.ftl`, `icons/a.svg`, `signature`) → `signature == Present`, `languages["de"]` vorhanden; `package_rejects`: fehlendes `plugin.toml`, fehlendes `plugin.wasm`, Eintrag `../x`, Eintrag `/x`, `C:\x`, `a\..\b`, 1001 Einträge, Paket > 64 MB, Wasm > 32 MB entpackt (Zip-Bombe: 33 MB Nullen deflate); `unpack_writes_only_known`: `unpack_to` schreibt genau `plugin.toml`, `plugin.wasm`, `languages/*`, `icons/*`, `signature`, unbekannte Einträge werden übergangen.
- [ ] **Step 6: `Package` implementieren.** `cargo test -p planora-plugin-host --test package` → PASS.
- [ ] **Step 7: `cargo deny check`** → keine Fehler (toml, semver: MIT OR Apache-2.0).
- [ ] **Step 8: Commit** `Plug-ins Etappe 1 Task 1: ZIP-Leser, Manifest und Paket`.

### Task 2: Vertrag, Laufzeit und Beispiel-Plug-ins

**Files:**
- Create: `crates/planora-plugin-api/{Cargo.toml,src/lib.rs,wit/plugin.wit}`
- Create: `crates/planora-plugin-host/src/{bindings.rs,runtime.rs,texts.rs}`, `crates/planora-plugin-host/tests/runtime.rs`, `crates/planora-plugin-host/tests/data/`
- Create: `plugins/Cargo.toml` (eigener `[workspace]`, `members = ["beispiel-hallo", "test-faelle"]`), `plugins/beispiel-hallo/{Cargo.toml,plugin.toml,languages/de.ftl,languages/en.ftl,src/lib.rs}`, `plugins/test-faelle/...`
- Create: `tools/build-plugins.ps1`, `tools/build-plugins.sh`
- Modify: `Cargo.toml` (workspace: `wasmtime`, `wasmtime-wasi`, `sha2`, interne Crates)

**Interfaces:**
- Consumes: `Manifest`, `Package` (Task 1).
- Produces:
  - WIT `planora:plugin@0.1.0` genau so (Namen sind ab hier fest):

```wit
package planora:plugin@0.1.0;

interface types {
  type request-id = u64;
  enum level { info, warning, error }
  enum right { net, fs-folder, model-read, model-write }
  record command { id: string, label-key: string, icon: option<string>, shortcut: option<string> }
  record registration { commands: list<command> }
  record host-info { api-version: string, program-version: string, language: string }
  variant field-kind { label, text(string), number(f64), toggle(bool) }
  record field { id: string, label-key: string, kind: field-kind }
  record dialog { id: string, title-key: string, fields: list<field> }
  variant field-value { text(string), number(f64), toggle(bool) }
  variant dialog-event { changed(tuple<string, field-value>), ok(list<tuple<string, field-value>>), cancel }
  record http-response { status: u16, body: list<u8> }
  variant request-result { granted, denied, http(http-response), failed(string) }
}
interface log { use types.{level}; write: func(level: level, text: string); }
interface i18n { text: func(key: string, args: list<tuple<string, string>>) -> string; }
interface ui {
  use types.{level, dialog};
  message: func(level: level, text: string);
  open-dialog: func(d: dialog) -> result<_, string>;
  close-dialog: func(id: string);
}
interface rights { use types.{right, request-id}; request: func(r: right) -> request-id; granted: func(r: right) -> bool; }
interface net { use types.{request-id}; get: func(url: string) -> result<request-id, string>; }

world plugin {
  import log; import i18n; import ui; import rights; import net;
  use types.{host-info, registration, dialog-event, request-id, request-result};
  export init: func(info: host-info) -> result<registration, string>;
  export run-command: func(id: string) -> result<_, string>;
  export handle-dialog: func(id: string, event: dialog-event) -> result<_, string>;
  export on-result: func(id: request-id, result: request-result);
  export shutdown: func();
}
```

  - `planora_plugin_host::runtime::{Engine, Instance, CallKind, PluginError, HostCalls}`:
    - `Engine::new() -> Result<Engine, PluginError>` (Cranelift, `component_model`, `epoch_interruption(true)`, Epochen-Thread mit 10 ms Takt, läuft bis `Drop`).
    - `enum CallKind { Ui, Command }` → Frist 20 bzw. 500 Takte.
    - `Engine::instantiate(&self, manifest: &Manifest, wasm: &[u8], host: Box<dyn HostCalls>) -> Result<Instance, PluginError>`.
    - `Instance::init(&mut self, info: HostInfo) -> Result<Registration, PluginError>` (Frist `Command`), `run_command(&mut self, id: &str)` (`Command`), `handle_dialog(&mut self, id: &str, ev: DialogEvent)` (`Ui`), `on_result(&mut self, id: u64, r: RequestResult)` (`Ui`), `shutdown(&mut self)` (`Ui`, Fehler werden nur protokolliert).
    - `enum PluginError { Load(String), Trap(String), Timeout, OutOfMemory, Plugin(String) }` (`Plugin` = Fehlertext aus `result::err` des Plug-ins; die anderen zählen als Ausfall).
    - `trait HostCalls: Send { fn log(&mut self, level: Level, text: &str); fn message(&mut self, level: Level, text: &str); fn open_dialog(&mut self, d: Dialog) -> Result<(), String>; fn close_dialog(&mut self, id: &str); fn request_right(&mut self, r: Right) -> u64; fn right_granted(&self, r: Right) -> bool; fn net_get(&mut self, url: &str) -> Result<u64, String>; }` – `i18n.text` beantwortet `runtime` selbst über `texts.rs`.
  - `texts::PluginTexts::new(languages: &BTreeMap<String, String>) -> PluginTexts`; `fn text(&self, lang: &str, key: &str, args: &[(String, String)]) -> String` (Sprache → `en` → Schlüssel selbst).
  - `tests/data/beispiel-hallo.pcplugin`, `tests/data/test-faelle.pcplugin`, je mit `*.quelle-sha256`.

- [ ] **Step 1: Plug-in-Workspace anlegen.** `beispiel-hallo` (cdylib, `wit-bindgen` mit Pfad `../../crates/planora-plugin-api/wit`): `init` meldet Befehl `hallo` (Text-Schlüssel `hallo-befehl`), `run-command("hallo")` öffnet Dialog `gruss` mit Feld `name` (Text) und Haken `laut`; `ok` → `ui.message(info, i18n.text("hallo-gruss", [("name", …)]))`, bei `laut` Großbuchstaben. `test-faelle` meldet Befehle `endlos` (`loop {}`), `speicher` (wächst je 1 MB bis zum Fehler), `panik` (`panic!`), `fehler` (gibt `Err("absichtlich")`), `netz` (`net.get("https://example.org/")`), `recht` (`rights.request(net)`), `langsam-dialog` (öffnet Dialog, `handle-dialog` läuft endlos).
- [ ] **Step 2: Bauskripte schreiben.** `tools/build-plugins.ps1`/`.sh`: `rustup target add wasm32-wasip2` prüfen, `cargo build --release --target wasm32-wasip2` in `plugins/`, je Plug-in `.pcplugin` (ZIP mit `plugin.toml`, `plugin.wasm`, `languages/`) nach `crates/planora-plugin-host/tests/data/`, dazu `<name>.quelle-sha256` = SHA-256 über die sortierten Pfade und Inhalte von `plugins/<name>/**` ohne `target/`. Ausführen; Pakete committen (Größe notieren, Ziel < 1 MB je Paket mit `opt-level = "s"`, `strip = true`).
- [ ] **Step 3: Failing Tests schreiben** in `tests/runtime.rs` (mit einer `FakeHost`-Implementierung von `HostCalls`, die Aufrufe mitschreibt):
  - `hallo_registers_command`: `init` → `commands[0].id == "hallo"`.
  - `hallo_dialog_roundtrip`: `run_command("hallo")` → `FakeHost` hat Dialog `gruss` mit 2 Feldern; `handle_dialog("gruss", Ok([("name", Text("Dennis")), ("laut", Toggle(true))]))` → Meldung `(Info, "HALLO, DENNIS!")` (de.ftl: `hallo-gruss = Hallo, { $name }!`).
  - `endless_loop_times_out`: `run_command("endlos")` → `Err(Timeout)` nach weniger als 6 s.
  - `ui_call_deadline`: `langsam-dialog` öffnen, `handle_dialog` → `Err(Timeout)` nach weniger als 1 s.
  - `memory_limit`: `speicher` mit `memory_mb = 64` → `Err(OutOfMemory)`; Programm lebt weiter, zweite Instanz lässt sich anlegen.
  - `panic_is_trap`: `panik` → `Err(Trap(_))`.
  - `plugin_error_is_not_failure`: `fehler` → `Err(Plugin("absichtlich"))`.
  - `instance_unusable_after_trap`: nach `Trap` gibt jeder weitere Aufruf `Err(Load("…"))` zurück, ohne Wasm auszuführen.
  - `wasi_is_empty`: Plug-in `test-faelle`-Befehl `umgebung` (in Step 1 ergänzen: liest `std::env::vars()`, `std::fs::read_dir("/")`, `SystemTime::now()`) meldet „0 Variablen, Ordner: Fehler, Zeit: 0“.
  - `sources_match_packages`: berechnet den Quell-Hash wie das Skript und vergleicht mit `*.quelle-sha256`; Fehlermeldung nennt `tools/build-plugins`.
  - `api_version_mismatch`: Manifest mit `api = "0.2"` → `Err(Load(_))` mit Text „braucht PlanoraCAD-Schnittstelle 0.2“.
- [ ] **Step 4: Tests laufen lassen** → FAIL (fehlende Typen).
- [ ] **Step 5: `bindings.rs`, `runtime.rs`, `texts.rs` implementieren.** wasmtime-Features nur `cranelift`, `component-model`, `runtime`, `parallel-compilation`; `wasmtime-wasi` mit `WasiCtxBuilder` ohne Preopens, Umgebung und Netz, stdout/stderr über `HostCalls::log`, Wanduhr fest 0, Monotonuhr auf 1 ms gerundet. Speichergrenze über `StoreLimitsBuilder::memory_size(memory_mb << 20)` und `trap_on_grow_failure(true)`; `OutOfMemory` erkennen am Trap-Grund. Pro Instanz ein `Store`; nach Ausfall `Instance` als „tot“ markieren.
- [ ] **Step 6: Tests laufen lassen.** `cargo test -p planora-plugin-host --test runtime` → PASS.
- [ ] **Step 7: `cargo deny check`** → keine Fehler; neue Lizenzen (falls wasmtime eine noch nicht erlaubte zieht) **nicht** freigeben, sondern anhalten und Dennis fragen. Bauzeit und Größe von `planora-app.exe` vorher/nachher im Commit notieren.
- [ ] **Step 8: Commit** `Plug-ins Etappe 1 Task 2: Vertrag 0.1, Laufzeit mit Grenzen, Beispiel-Plug-ins`.

### Task 3: Rechte und Anfragen mit Kennung

**Files:**
- Create: `crates/planora-plugin-host/src/rights.rs`, `crates/planora-plugin-host/tests/rights.rs`

**Interfaces:**
- Consumes: `Right`, `Manifest` (Task 1).
- Produces:
  - `enum Grant { Allowed, Denied }`; `enum Answer { Allow, Once, Deny }`.
  - `struct Rights { saved: BTreeMap<Right, Grant>, once: BTreeSet<Right> }`; `fn state(&self, r: Right) -> Option<Grant>` (once zählt als `Allowed`), `fn answer(&mut self, r: Right, a: Answer)`, `fn revoke(&mut self, r: Right)`.
  - `struct Requests` (Zähler ab 1, je Instanz neu): `fn next(&mut self) -> u64`.
  - `enum Pending { AskRight { request: u64, right: Right }, Net { request: u64, url: String } }`.
  - `fn check_net_url(manifest: &Manifest, url: &str) -> Result<(), String>`: nur `https://`, Host exakt in `net_hosts` (ohne Groß/Klein-Unterschied), kein Benutzer/Passwort in der URL, Port nur 443 oder keiner.

- [ ] **Step 1: Failing Tests schreiben:** `once_lasts_until_reset` (`Once` → `state == Allowed`, nach `Rights::reset_once()` → `None`); `deny_saved`; `revoke_returns_to_ask`; `undeclared_right_always_denied` (Recht nicht in `manifest.rights` → Host-Aufruf `request_right` liefert sofort ein `Denied`-Ergebnis, ohne Nachfrage); `net_url_rules` (erlaubt `https://example.org/a?b`, abgelehnt `http://example.org`, `https://evil.org`, `https://example.org.evil.org`, `https://u:p@example.org`, `https://example.org:8443`).
- [ ] **Step 2: FAIL prüfen**, dann `rights.rs` implementieren, `cargo test -p planora-plugin-host --test rights` → PASS.
- [ ] **Step 3: Commit** `Plug-ins Etappe 1 Task 3: Rechte und Anfragen`.

### Task 4: Fundorte, Installieren, Zustand, PluginHost

**Files:**
- Create: `crates/planora-plugin-host/src/places.rs`, `crates/planora-plugin-host/tests/places.rs`, `crates/planora-plugin-host/tests/host.rs`
- Modify: `crates/planora-plugin-host/src/lib.rs`, `crates/planora-project/src/config.rs`, `crates/planora-project/tests/config.rs` (oder bestehende Config-Tests)

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces:
  - `planora_project::config::PluginConfig { enabled: bool, rights: BTreeMap<String, String> }` (`"allowed"`/`"denied"`); `Config::plugins: BTreeMap<String, PluginConfig>` mit `#[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "lenient_plugins")]`.
  - `places::Places { bundled: Vec<PathBuf>, user: PathBuf, cache: PathBuf }`; `Places::system() -> Places`: mitgeliefert `<exe-Ordner>/plugins` und unter Linux zusätzlich `<exe-Ordner>/../share/planora-cad/plugins`; Benutzer Windows `%APPDATA%\PlanoraCAD\plugins`, Linux `$XDG_DATA_HOME/planora-cad/plugins` (sonst `~/.local/share/planora-cad/plugins`); Zwischenspeicher Windows `%LOCALAPPDATA%\PlanoraCAD\plugin-cache`, Linux `$XDG_CACHE_HOME/planora-cad/plugin-cache`. (Abweichung von der Spec: `planora-cad` statt `planoracad`, wie `config.json`.)
  - `enum Origin { Bundled, User }`; `struct Found { dir: PathBuf, origin: Origin, manifest: Manifest, problem: Option<String> }`; `fn scan(places: &Places) -> Vec<Found>`.
  - `fn install(places: &Places, pkg: &Package) -> io::Result<PathBuf>`: entpackt in `<user>/.tmp-<id>`, benennt danach auf `<user>/<id>` um (altes vorher nach `.alt-<id>`, danach löschen); `fn uninstall(places: &Places, id: &str) -> io::Result<()>` nur für `Origin::User`.
  - `fn load_component(engine: &Engine, cache: &Path, wasm: &[u8]) -> Result<Component, PluginError>`: Schlüssel `sha256(wasm) + engine.precompile_compatibility_hash()`, Datei `<cache>/<schlüssel>.cwasm`; kaputte Datei → neu übersetzen.
  - `PluginHost` in `lib.rs`: `PluginHost::new(places: Places) -> PluginHost`; `fn entries(&self) -> &[PluginEntry]` (`PluginEntry { id, found: Vec<Found>, active: Option<usize> /* Index in found */, state: EntryState, rights: Rights, failures: u8 }`, `enum EntryState { Off, Loading, Running, Failed(String), Stopped }`); `fn start_all(&mut self, config: &BTreeMap<String, PluginConfig>)`; `fn poll(&mut self) -> Vec<HostEvent>`; `fn run_command(&mut self, full_id: &str)`; `fn dialog_event(&mut self, plugin: &str, dialog: &str, ev: DialogEvent)`; `fn answer(&mut self, plugin: &str, request: u64, a: Answer)`; `fn restart(&mut self, id: &str)`; `fn set_enabled(&mut self, id: &str, on: bool)`; `fn commands(&self) -> Vec<PluginCommand>` (`PluginCommand { full_id: String /* "<plugin-id>.<id>" */, label: String, plugin: String }`).
  - `enum HostEvent { Message { plugin: String, level: Level, text: String }, OpenDialog { plugin: String, dialog: Dialog }, CloseDialog { plugin: String, id: String }, Ask { plugin: String, request: u64, right: Right }, Net { plugin: String, request: u64, url: String }, Failed { plugin: String, reason: String }, Changed }`.
  - Instanzkennung: jede Instanz bekommt eine `generation: u64`; `answer` und Netzergebnisse tragen sie mit und werden bei abweichender Generation verworfen.

- [ ] **Step 1: Failing Tests für Config:** `plugins_roundtrip` (Eintrag mit `enabled = false` und `rights.net = "allowed"` schreibt und liest gleich), `plugins_absent_not_written` (leere Tabelle → kein `"plugins"` im JSON, bestehende Dateien bytegleich), `plugins_lenient` (`"plugins": 5` und `"plugins": {"a.b": {"enabled": "ja"}}` → Wurzel und übrige Felder bleiben, kaputter Eintrag übergangen).
- [ ] **Step 2: `Config::plugins` implementieren** → PASS (`cargo test -p planora-project`).
- [ ] **Step 3: Failing Tests für Fundorte** (`tests/places.rs`, alles in Temp-Ordnern): `scan_both_origins`; `same_id_higher_version_wins` (mitgeliefert 0.1.0, Benutzer 0.2.0 → `active` zeigt auf Benutzer; umgekehrt 0.3.0 mitgeliefert → mitgeliefert); `broken_plugin_listed_with_problem` (kaputtes `plugin.toml` → `problem: Some(_)`, Rest lädt); `install_replaces_atomically` (zweite Installation gleicher id ersetzt, kein `.tmp-`/`.alt-` bleibt); `uninstall_bundled_refused`; `cache_reused_and_repaired` (zweites Laden liest `.cwasm`; abgeschnittene `.cwasm` → neu übersetzt, kein Fehler); `places_linux_and_windows` (Pfadberechnung über übergebene Umgebungswerte, Funktion `Places::from_env(exe: &Path, env: &dyn Fn(&str) -> Option<OsString>, windows: bool)`).
- [ ] **Step 4: `places.rs` implementieren** → PASS.
- [ ] **Step 5: Failing Tests für `PluginHost`** (`tests/host.rs`, mit `tests/data/*.pcplugin` in Temp-Fundorten installiert): `start_registers_commands` (`commands()` enthält `planora.beispiel-hallo.hallo` nach `poll` bis `Running`); `disabled_not_started`; `failure_counts_and_stops` (`test-faelle.panik` dreimal → nach dem dritten `state == Stopped`, `restart` ist danach bis `PluginHost::new` wirkungslos); `failure_event_once_per_failure`; `restart_resets_instance` (`restart` nach erstem Fehler → `Running`, neue Generation).
- [ ] **Step 6: `PluginHost` implementieren.** Übersetzen und `init` laufen in einem Hintergrundthread je Plug-in (`std::thread`, Ergebnis über `mpsc`), `poll` holt fertige Instanzen ab; alle weiteren Aufrufe auf dem aufrufenden Thread. → PASS (`cargo test -p planora-plugin-host`).
- [ ] **Step 7: Commit** `Plug-ins Etappe 1 Task 4: Fundorte, Installieren, Zustand`.

### Task 5: Plug-ins im Programm: Menü, Meldungen, Rechte-Nachfrage, Netz

**Files:**
- Create: `crates/planora-app/src/plugins.rs`, `crates/planora-app/src/plugin_prompts.rs`, `crates/planora-app/tests/plugins.rs`
- Modify: `crates/planora-app/src/workspace.rs` (Feld `plugins: crate::plugins::Plugins`), `crates/planora-app/src/app.rs` (`menu_bar_measured`: neues Menü `menu-utilities` zwischen „Ansicht“ und „Hilfe“; Aufruf von `plugins::show` je Bild), `crates/planora-app/src/lib.rs`, `crates/planora-app/Cargo.toml`, `crates/planora-i18n/languages/{de,en}.ftl`

**Interfaces:**
- Consumes: `PluginHost`, `HostEvent`, `Answer` (Task 4); `Job` aus `crates/planora-app/src/jobs.rs`; der HTTP-Weg aus `planora-update` (bestehende `ureq`-Nutzung in `crates/planora-update/src/net.rs`; Funktion für einen GET mit Größengrenze dort öffentlich machen oder ergänzen: `pub fn get_limited(url: &str, max: u64, timeout: Duration) -> Result<(u16, Vec<u8>), String>`).
- Produces:
  - `plugins::Plugins { host: PluginHost, prompts: VecDeque<PromptRequest>, dialogs: Vec<OpenPluginDialog>, net_jobs: Vec<(String /*plugin*/, u64 /*generation*/, u64 /*request*/, Job<Result<(u16, Vec<u8>), String>>)>, banners: Vec<FailureBanner> }`.
  - `Plugins::new(places: Places, config: &Config) -> Plugins` (startet `start_all`); `fn frame(&mut self, ctx: &egui::Context, ws_config: &mut Config)` (holt `poll`, verteilt Ereignisse, pollt Netz-Jobs, zeichnet Nachfrage, Dialoge, Fehlerleiste); `fn utilities_menu(&mut self, ui: &mut egui::Ui) -> MenuRequest`.
  - Menü „Hilfsprogramme“ (`menu-utilities`, en „Utilities“): Eintrag „Zusatzmodule…“ (`menu-utilities-addins`, öffnet Einstellungen am Abschnitt Plug-ins: `MenuRequest::Settings(SettingsSection::Plugins)` – bestehenden `MenuRequest` um die Abschnittsangabe erweitern), Trennlinie, danach die Befehle aller laufenden Plug-ins nach Plug-in-Name gruppiert (Untermenü je Plug-in, wenn es mehr als einen Befehl hat).
  - Meldungen der Plug-ins: in `AppState::message` des aktiven Tabs, vorangestellt `„<Plug-in-Name>: “`.
  - Rechte-Nachfrage (`plugin_prompts::show_ask`): Titel `plugin-ask-title` („Recht für { $plugin }“), Text je Recht (`plugin-right-net` „…möchte Verbindungen zu { $hosts } aufbauen.“, `plugin-right-fs-folder`, `plugin-right-model-read`, `plugin-right-model-write`), Knöpfe `plugin-ask-allow` „Erlauben“, `plugin-ask-once` „Nur diesmal“, `plugin-ask-deny` „Ablehnen“; Escape = Ablehnen, nur einmal; immer eine Nachfrage gleichzeitig, weitere warten. „Erlauben“/„Ablehnen“ schreibt `config.plugins[id].rights` und speichert die Konfiguration über den bestehenden Weg (`Workspace::try_persist_config`).
  - Netzanfrage: erst nach erlaubtem Recht; `Job::spawn` mit `get_limited(url, 16 << 20, 30 s)`; Ergebnis → `on_result(Http{…})` bzw. `Failed(text)`.
  - Fehlerleiste: oben im Fenster je ausgefallenem Plug-in „Das Plug-in { $plugin } wurde wegen eines Fehlers angehalten.“ (`plugin-failed`) mit Knopf „Neu starten“ (`plugin-restart`, fehlt bei `Stopped`; dort Text `plugin-stopped` „…bleibt bis zum Neustart des Programms aus.“) und „Schließen“.

- [ ] **Step 1: Failing Tests schreiben** (`tests/plugins.rs`, ohne Fenster: `Plugins` mit Temp-Fundorten, `egui::Context` wie in den bestehenden App-Tests, siehe `crates/planora-app/tests/common/mod.rs`):
  - `menu_lists_plugin_command`: nach Laden steht „Hallo sagen“ (de.ftl von `beispiel-hallo`: `hallo-befehl = Hallo sagen`) im Menü „Hilfsprogramme“.
  - `ask_then_allow_saves_config`: `test-faelle.recht` → eine Nachfrage offen; `answer(Allow)` → `config.plugins["planora.test-faelle"].rights["net"] == "allowed"`, Plug-in bekommt `on_result(Granted)`.
  - `once_not_saved`: `Once` → nichts in der Konfiguration, zweite Anfrage in derselben Sitzung ohne Nachfrage.
  - `net_without_right_asks_first`, `net_denied_gives_denied` (Ergebnis `Denied`, kein Job gestartet).
  - `late_answer_dropped`: Nachfrage offen, dann `restart` → Antwort auf alte Nachfrage wird verworfen, neue Instanz bekommt kein `on_result` (Review Focus 3); dasselbe für einen fertig gewordenen Netz-Job nach `set_enabled(false)`.
  - `failure_banner_and_restart`: `test-faelle.panik` → Fehlerleiste mit „Neu starten“; nach dem dritten Ausfall ohne Knopf.
  - `utilities_menu_opens_settings_section`.
- [ ] **Step 2: FAIL prüfen**, dann implementieren. Netz in Tests über ein austauschbares `fn(&str) -> Result<(u16, Vec<u8>), String>` in `Plugins` (Vorgabe: `get_limited`).
- [ ] **Step 3:** `cargo test -p planora-app --test plugins` → PASS; Sprachdatei-Prüfungen (`cargo test -p planora-i18n`) → PASS.
- [ ] **Step 4: Commit** `Plug-ins Etappe 1 Task 5: Menü Hilfsprogramme, Meldungen, Rechte-Nachfrage, Netz`.

### Task 6: Beschriebene Dialoge

**Files:**
- Create: `crates/planora-app/src/plugin_dialog.rs`, `crates/planora-app/tests/plugin_dialog.rs`
- Modify: `crates/planora-app/src/plugins.rs`

**Interfaces:**
- Consumes: `Dialog`, `DialogEvent`, `FieldValue` (WIT-Typen aus Task 2, über `planora_plugin_host` wieder exportiert), `crate::dialog::Dialog` (bestehender Fusion-Dialograhmen in `crates/planora-app/src/dialog.rs`).
- Produces: `struct OpenPluginDialog { plugin: String, spec: Dialog, values: BTreeMap<String, FieldValue> }`; `fn show(ctx: &egui::Context, d: &mut OpenPluginDialog, texts: &dyn Fn(&str) -> String) -> Option<DialogEvent>`.

Verhalten: Titel und Feldbeschriftungen sind Text-Schlüssel und werden **bei jedem Zeichnen** übersetzt (Sprachwechsel mit offenem Dialog). `label` = nur Text, `text` = einzeiliges Feld, `number` = Zahlenfeld (Komma/Punkt nach Sprache, wie die bestehenden Zahlenfelder; Einheiten kommen in Etappe 2), `toggle` = Haken. Jede Änderung sendet `changed`; „OK“ sendet `ok` mit allen Werten und schließt; „Abbrechen“, Escape und X senden `cancel` und schließen. `open-dialog` mit vorhandener `id` ersetzt die Felder und behält Werte gleicher Feld-id. Fällt das Plug-in aus, schließen alle seine Dialoge ohne weiteres Ereignis.

- [ ] **Step 1: Failing Tests:** `fields_render_and_ok` (Werte aus Feldern gehen in `ok`), `cancel_on_escape`, `reopen_keeps_values`, `language_switch_retranslates` (mit `planora_i18n::with_lang(english())` zeigt der Titel die en-Übersetzung), `dialogs_close_on_failure` (Review Focus 4: `test-faelle.langsam-dialog` öffnen, Ereignis auslösen → Timeout → Dialog weg, Fehlerleiste da, Dokument-`edit_count` unverändert), `hallo_end_to_end` (Befehl → Dialog → OK → Meldung „Beispiel-Hallo: Hallo, Dennis!“ in `AppState::message`).
- [ ] **Step 2: FAIL prüfen, implementieren, PASS** (`cargo test -p planora-app --test plugin_dialog`).
- [ ] **Step 3: Commit** `Plug-ins Etappe 1 Task 6: beschriebene Dialoge`.

### Task 7: Einstellungen „Plug-ins“ und Installationsdialog

**Files:**
- Modify: `crates/planora-app/src/settings_window.rs` (neuer Abschnitt nach „Updates“, Funktion `show_plugins`), `crates/planora-app/src/plugin_prompts.rs` (`show_install`), `crates/planora-i18n/languages/{de,en}.ftl`
- Create: `crates/planora-app/tests/plugin_settings.rs`, `crates/planora-plugin-host/src/licenses.rs`

**Interfaces:**
- Consumes: `PluginHost::entries`, `set_enabled`, `install`, `uninstall`, `Rights::revoke` (Tasks 3–4); Dateiauswahl `crates/planora-app/src/file_chooser.rs`.
- Produces:
  - `licenses::GPL_COMPATIBLE: &[&str]` = genau die `allow`-Liste aus `deny.toml`; `fn license_ok(spdx: &str) -> bool` (Ausdrücke mit `OR`: eine verträgliche genügt; mit `AND`: alle müssen verträglich sein; `WITH` nur in der Form, die in der Liste steht).
  - Abschnitt „Plug-ins“: je Eintrag Name, Version, Herkunft („mitgeliefert“/„installiert“), Zustand, Schalter an/aus, aufklappbar: Autor, Lizenz, Beschreibung, je verlangtem Recht Zustand („erlaubt“, „abgelehnt“, „wird beim ersten Gebrauch gefragt“) mit Knopf „Zurücknehmen“, Knopf „Entfernen“ (nur installierte; Rückfrage „Plug-in X entfernen?“). Ein Eintrag mit zwei Fundorten zeigt den nicht aktiven grau mit „überdeckt von Version …“. Knopf „Plug-in installieren…“ öffnet die Dateiauswahl mit Filter `*.pcplugin`.
  - Installationsdialog: Name, Version, Autor, Lizenz, Signatur („nicht signiert“ / „signiert, nicht geprüft“), Liste der verlangten Rechte mit Hinweis „Das Programm fragt beim ersten Gebrauch nach“, Speicherbedarf, falls `memory_mb != 512`. Ist `license_ok` falsch: Hinweis `plugin-license-unknown` „Lizenz unbekannt oder nicht GPL-verträglich“. Gibt es die id schon: „Version A ersetzen durch Version B?“. Knöpfe „Installieren“, „Abbrechen“. Nach dem Installieren startet das Plug-in ohne Neustart.

- [ ] **Step 1: Failing Tests:** `license_list_matches_deny_toml` (liest `../../deny.toml`, vergleicht mit `GPL_COMPATIBLE`), `license_expressions` (`"MIT OR Proprietary"` ok, `"MIT AND Proprietary"` nicht, `"Apache-2.0 WITH LLVM-exception"` ok, `"GPL-2.0-only"` nicht); in `plugin_settings.rs`: `install_flow_starts_plugin`, `install_shows_license_hint`, `install_replace_asks`, `toggle_off_stops_and_saves`, `revoke_returns_to_ask`, `remove_user_plugin_only`, `shadowed_entry_shown`.
- [ ] **Step 2: FAIL prüfen, implementieren, PASS** (`cargo test -p planora-app --test plugin_settings`, `cargo test -p planora-plugin-host`).
- [ ] **Step 3: Commit** `Plug-ins Etappe 1 Task 7: Einstellungen Plug-ins und Installation`.

### Task 8: Installieren und Aktualisieren über den Repo-Link

**Files:**
- Create: `crates/planora-update/src/plugin_source.rs`, `crates/planora-update/tests/plugin_source.rs`
- Modify: `crates/planora-update/src/lib.rs`, `crates/planora-update/src/install.rs` (`redirect_target` und die Host-Regel ohne festes `REPO` nutzbar machen, ohne das Verhalten für Programm-Updates zu ändern), `crates/planora-plugin-host/src/places.rs` (`herkunft.json`), `crates/planora-project/src/config.rs` (`last_plugin_check`), `crates/planora-app/src/plugins.rs`, `crates/planora-app/src/plugin_prompts.rs`, `crates/planora-app/src/settings_window.rs`, `crates/planora-i18n/languages/{de,en}.ftl`
- Create: `crates/planora-app/tests/plugin_repo.rs`

**Interfaces:**
- Consumes: `Package::read`, `places::install`, `PluginHost::rescan` (Task 4); Installationsdialog und Abschnitt „Plug-ins“ (Task 7); `planora_update::{Channel, parse_notes, get_limited}`; `planora_update::check::due` (Tagesregel); `Config::update_check`, `Config::update_channel`.
- Produces:
  - `plugin_source::RepoLink { owner: String, repo: String }`; `fn parse_repo_link(text: &str) -> Result<RepoLink, String>`; `fn releases_url(&self) -> String` (= `https://api.github.com/repos/<owner>/<repo>/releases`); `fn web_url(&self) -> String` (= `https://github.com/<owner>/<repo>`).
  - `plugin_source::PluginRelease { version: semver::Version, tag: String, notes: String, package: AssetRef, sha256: Option<AssetRef> }`; `fn parse_plugin_releases(json: &str, link: &RepoLink) -> Result<Vec<PluginRelease>, String>` (Entwürfe übergangen; Release ohne oder mit mehreren `*.pcplugin` übergangen; Asset-URLs nur `https://github.com/<owner>/<repo>/releases/download/<tag>/<name>`); `fn pick_plugin_release(releases: &[PluginRelease], installed: Option<&Version>, channel: Channel) -> Option<&PluginRelease>` (neueste passende, höher als `installed`).
  - `fn check_downloaded(pkg: &Package, release: &PluginRelease, installed_id: Option<&str>) -> Result<(), String>`: Tag ohne `v` = `pkg.manifest.version`; `installed_id` gesetzt ⇒ gleiche `id`. (Liegt in `planora-app`, weil `planora-update` `Package` nicht kennt.)
  - Grenzen: Releases-JSON ≤ 4 MiB, Paket ≤ `MAX_PACKAGE` (64 MiB), Prüfsummendatei ≤ 64 KiB, Zeitgrenze je Anfrage 30 s; Weiterleitungen nur nach `github.com`, `api.github.com`, `*.githubusercontent.com` (bestehende Regel aus `redirect_target`).
  - `places::Provenance { link: String, tag: String }`; `fn read_provenance(dir: &Path) -> Option<Provenance>`, `fn write_provenance(dir: &Path, p: &Provenance) -> io::Result<()>` (`herkunft.json`, nachsichtig gelesen); `install` bekommt `provenance: Option<&Provenance>`.
  - In `Plugins`: `fn install_from_link(&mut self, text: &str)` (startet einen `Job`: Releases holen → wählen → Paket und ggf. Prüfsumme laden → `Package::read` → `check_downloaded` → Installationsdialog aus Task 7 mit den Paketdaten; erst „Installieren“ schreibt auf die Platte und ruft `rescan`); `fn check_updates(&mut self, manual: bool)` (je installiertem Plug-in mit `herkunft.json` ein Job); `fn updates(&self) -> &[PluginUpdate]` (`PluginUpdate { id: String, from: Version, to: PluginRelease, new_rights: BTreeSet<Right> }`; `new_rights` erst nach dem Laden des Pakets bekannt, vorher leer); `fn apply_update(&mut self, id: &str)`.
  - Oberfläche:
    - Installationsdialog aus Task 7 mit zwei Wegen: Knopf „Aus Datei…“ und Feld „Repo-Link“ mit Knopf „Holen“ (`plugin-install-link`, `plugin-install-fetch`), währenddessen „Wird geholt…“ und „Abbrechen“. Fehler als Text im Dialog: `plugin-link-invalid` „Das ist kein GitHub-Repo-Link (https://github.com/besitzer/repo).“, `plugin-link-no-release` „In diesem Repo gibt es kein Release mit einer .pcplugin-Datei.“, `plugin-link-mismatch` „Das Release { $tag } passt nicht zur Plug-in-Version { $version }.“, `plugin-link-checksum` „Die Prüfsumme stimmt nicht.“, `plugin-link-network` „Keine Verbindung zu GitHub: { $detail }“.
    - Abschnitt „Plug-ins“: bei Plug-ins mit Link die Zeile „Quelle: <web_url>“, Knopf „Nach Updates suchen“ (`plugin-check-updates`), bei verfügbarem Update „Aktualisieren auf { $version }“ (`plugin-update-to`).
    - Hinweis oben im Fenster wie die Fehlerleiste: „Für { $count } Plug-ins gibt es Updates.“ (`plugin-updates-available`, Fluent-Mehrzahl) mit „Anzeigen“ (öffnet Abschnitt „Plug-ins“) und „Schließen“.
    - Aktualisieren-Dialog: alte → neue Version, Release-Notizen (über `parse_notes`, wie im Update-Dialog des Programms), „Neu verlangte Rechte: …“ (nur wenn nicht leer, mit dem Satz „Das Programm fragt beim ersten Gebrauch nach“), Knöpfe „Aktualisieren“, „Abbrechen“. Erteilte Rechte bleiben.
  - Automatische Prüfung: beim Start, wenn `config.update_check` an ist und `due(now, last_plugin_check, true, false)`; Zeitpunkt im neuen Config-Feld `last_plugin_check: Option<u64>` (nachsichtig, nicht geschrieben, wenn `None`). Mitgelieferte Plug-ins werden nie geprüft.

- [ ] **Step 1: Failing Tests in `crates/planora-update/tests/plugin_source.rs`:** `link_forms` (`https://github.com/a/b`, `github.com/a/b`, `https://github.com/a/b/`, `https://github.com/a/b.git`, Großschreibung im Host → alle `RepoLink { owner: "a", repo: "b" }`); `link_rejects` (`https://gitlab.com/a/b`, `https://github.com/a`, `https://github.com/a/b/tree/main`, `https://github.com/a/b?x=1`, `https://u@github.com/a/b`, `http://github.com/a/b`, Besitzer oder Repo mit `..`, leer, Leerzeichen); `releases_parse` (Beispiel-JSON mit Entwurf, Vorabversion, Release ohne Paket, Release mit zwei Paketen, gültigem Release mit `.pcplugin` und `.pcplugin.sha256`, fremder Asset-URL → nur die gültigen, fremde URL verworfen); `pick_respects_channel_and_installed` (Kanal `stable` überspringt `0.2.0-beta.1`; `installed = 0.2.0` → keine Wahl bei neuestem `0.2.0`); `redirect_rule_for_plugin_assets` (Weiterleitung von einer Plug-in-Asset-URL nach `objects.githubusercontent.com` erlaubt, nach `example.org` abgelehnt; bestehende Tests von `redirect_target` bleiben grün).
- [ ] **Step 2: FAIL prüfen, `plugin_source.rs` implementieren**, PASS (`cargo test -p planora-update`).
- [ ] **Step 3: Failing Tests für `herkunft.json`** in `crates/planora-plugin-host/tests/places.rs`: `provenance_roundtrip`, `provenance_lenient` (kaputtes JSON → `None`, Plug-in lädt trotzdem), `install_writes_provenance`. Implementieren, PASS (`cargo test -p planora-plugin-host`).
- [ ] **Step 4: Failing Tests in `crates/planora-app/tests/plugin_repo.rs`** (HTTP über die austauschbare Netzfunktion in `Plugins` aus Task 5; Antworten aus Testdaten: Releases-JSON und `tests/data/beispiel-hallo.pcplugin`; für ein Update ein zur Testzeit gebautes Paket aus demselben Wasm mit `version = "0.2.0"` und zusätzlichem Recht `net`):
  - `install_from_link_shows_dialog_then_runs` (Link eingeben → Dialog mit Name, Version, Lizenz → „Installieren“ → Befehl im Menü, `herkunft.json` vorhanden).
  - `install_from_link_errors` (ungültiger Link, kein Release, falsche Prüfsumme, Tag passt nicht, Netzfehler → jeweils der Text aus den Interfaces, nichts installiert).
  - `update_found_and_applied` (installiert 0.1.0 mit Link → `check_updates(true)` → `updates()` enthält 0.2.0 mit `new_rights == {net}` → `apply_update` → läuft 0.2.0, neue Generation, `net` noch nicht erteilt, alte Rechte erhalten).
  - `no_downgrade_and_no_foreign_id` (Release mit 0.0.9 wird nicht angeboten; Release mit anderer `id` wird beim Aktualisieren abgelehnt).
  - `auto_check_daily` (`update_check` aus → keine Anfrage; an und `last_plugin_check` vor 25 h → eine Anfrage je Plug-in mit Link, danach `last_plugin_check` gesetzt; vor 1 h → keine).
  - `bundled_plugins_not_checked`.
- [ ] **Step 5: FAIL prüfen, implementieren, PASS** (`cargo test -p planora-app --test plugin_repo`, `cargo test -p planora-i18n`).
- [ ] **Step 6: Commit** `Plug-ins Etappe 1 Task 8: Installieren und Aktualisieren über den Repo-Link`.

### Task 9: Doku, Prüfung, Messung

**Files:**
- Create: `docs/plugins/entwickler.md`, Wiki-Seite „Plug-ins“ im Benutzer-Wiki (Ort wie bestehende Wiki-Seiten im Repo)
- Modify: `README.md` (Abschnitt Plug-ins kurz, Plattformen Windows und Ubuntu), `docs/superpowers/offene-punkte.md`, `installer/planoracad.iss` (leerer Ordner `{app}\plugins` anlegen, sonst nichts)

- [ ] **Step 1: Entwickler-Doku schreiben:** gleich am Anfang „Plug-ins müssen unter einer GPL-3.0-verträglichen Lizenz stehen“; Aufbau `.pcplugin`, `plugin.toml` mit allen Feldern und Grenzen, WIT-Datei-Ort, Schritt-für-Schritt „Erstes Plug-in in Rust“ (an `beispiel-hallo`), Hinweise für C/C++ (`wit-bindgen c`), Go (TinyGo, `wit-bindgen-go`), Python (`componentize-py`) als ungetestete Wegweiser, Rechte und Anfragen mit Kennung, Zeitgrenzen, „0.x darf brechen“.
- [ ] **Step 2: Volle Tests Windows:** `$env:CARGO_INCREMENTAL=0; $env:CARGO_PROFILE_DEV_DEBUG=0; cargo test --workspace` → 0 failed; `cargo clippy --workspace --all-targets` → keine Warnungen; `cargo deny check` → ok.
- [ ] **Step 3: Volle Tests Ubuntu (WSL)** mit denselben Variablen → 0 failed außer dem bekannten Vorschaubild-Test als root; danach WSL aufräumen laut Global Constraints.
- [ ] **Step 4: Messen und notieren:** Programmstart ohne Plug-ins und mit beiden Beispiel-Plug-ins (erster Start mit Übersetzen, zweiter Start aus dem Zwischenspeicher), Größe `planora-app.exe` vorher/nachher, Release-Bauzeit vorher/nachher. Ziel: zweiter Start höchstens 50 ms langsamer als ohne Plug-ins.
- [ ] **Step 5: Dennis fragen**, ob er jetzt den Handtest machen möchte (Installieren aus Datei und über einen Repo-Link, Update eines Plug-ins, Hallo-Dialog, Rechte-Nachfrage mit `test-faelle.netz`, Ausfall und Neu starten, Einstellungen).
- [ ] **Step 6: Commit** `Plug-ins Etappe 1 Task 9: Doku, Prüfung, Messung`; Vault-Notiz in `D:\Vault\Vault\Projekte\PlanoraCAD\`.
