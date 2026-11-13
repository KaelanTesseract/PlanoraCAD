# Fenster abdocken – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dialoge, Featurebaum, rechte Spalte und Skizzenfeld lassen sich per Symbol als eigene Betriebssystemfenster abdocken (zweiter Monitor), mit gemerkter Lage über Neustarts.

**Architecture:** Neues Modul `planora-app/src/detach.rs` hält den Abdock-Zustand je Platz (aus `config.json`), prüft Monitore und zeichnet abgedockte Inhalte über `egui::Context::show_viewport_immediate`. `Dialog` (`dialog.rs`) und die Feldzeichnung (`shell_panels.rs`, `drawing_view.rs`, `sketch_panel.rs`) rufen diesen Baustein; `shell::layout_in_mode` lässt abgedockte Plätze aus. Ohne Abdocken und bei `embed_viewports() == true` bleibt alles wie heute.

**Tech Stack:** Rust, egui/eframe 0.36.2 (wgpu, Multi-Viewport), serde_json, windows-sys 0.61.2.

**Spec:** `docs/superpowers/specs/2026-11-13-fenster-abdocken-design.md`

## Global Constraints

- Code-Fragen zuerst mit `graphify query "<frage>"` (Graph unter `D:\Testumgebung\PlanoraCAD\graphify-out`, im Hauptordner aufrufen; der Worktree hat keinen eigenen).
- Kommentare deutsch. Texte nur über `tr!`, Schlüssel in `crates/planora-i18n/languages/de.ftl` **und** `en.ftl`.
- Keine neue Abhängigkeit; nur `windows-sys` bekommt die Features `Win32_Graphics_Gdi` (und `Win32_UI_HiDpi`, falls in Task 4 nötig). `cargo deny check` grün.
- Ohne abgedockte Plätze: Feldgeometrie, Dialoglage und `config.json` bytegleich wie vorher; alle bestehenden Tests unverändert grün.
- Abdockbar: alle `Dialog`-Aufrufe außer `.fixed()`, `bottom_right_in` (Update-Hinweis) und Absturzhinweis (`crash_dialog.rs`); Plätze `tree`, `right`, `sketch_side`; Verlaufsleiste und alle übrigen Felder nicht.
- Symbolnamen „Abdocken“/„Andocken“ (en „Undock“/„Dock“).
- Abgedockte Fenster: `with_decorations(false)`, `with_taskbar(false)`, normale Ebene, Graphit-Kopfzeile.
- `windows-sys`-Aufrufe nur unter `#[cfg(windows)]`; Linux baut und testet ohne.
- Prüfen: Windows `cargo fmt -p <crate> -- --check` je Crate (nicht `--all`, os error 206), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`; WSL über `~/wsl-s6-full.sh`; danach WSL-Ordner aufräumen (CLAUDE.md).
- Commits mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; E-Mail `info@dennis-guse.de`.

## Review Focus

1. **Kaputte oder fremde `config.json`** (Eintrag `detached` mit Unsinn, negative Größe, NaN, unbekannter Schlüssel): Programm startet, Eintrag verworfen, übrige Einstellungen bleiben – Test in Task 1.
2. **Monitor fehlt beim Start** (gemerkte Lage auf entferntem Monitor): Fenster erscheint angedockt, Eintrag bleibt – Test in Task 2 (reine Funktion) und Task 5 (Dialog öffnet angedockt).
3. **Taste im abgedockten Fenster läuft doppelt oder gar nicht** (Strg+Z aus abgedocktem Baum, Strg+S aus abgedocktem Dialog, Esc im Hauptfenster bei abgedocktem Dialog): genau einmal – Tests in Task 7.
4. **Moduswechsel mit abgedocktem Skizzenfeld** (Skizze beenden, Rückgängig in die Skizze, Tab-Wechsel): kein verwaistes Fenster, kein Absturz, Layout passt im selben Bild – Test in Task 6.
5. **Programm beenden / Alt+F4 im abgedockten Fenster** bei ungespeicherter Änderung: Rückfrage wie heute, Alt+F4 beendet nie das Programm – Test in Task 4 (close_requested des Kindfensters) und Handtest.

---

### Task 1: Konfiguration `detached`

**Files:**
- Modify: `crates/planora-project/src/config.rs` (Struct `Config`, neuer Struct, Tests im vorhandenen `mod tests`)

**Interfaces:**
- Produces: `pub struct DetachedWindow { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }` (physische Pixel, `Serialize, Deserialize, Clone, Copy, PartialEq, Debug`); `Config::detached: BTreeMap<String, DetachedWindow>` mit `#[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "lenient_detached")]`.

- [ ] **Step 1: Tests schreiben** in `config.rs` `mod tests`:
  - `detached_empty_keeps_old_file_bytes`: eine vorhandene Beispiel-`config.json` ohne `detached` laden und mit `save_to` schreiben → bytegleich.
  - `detached_round_trip`: `detached.insert("dialog:blend", DetachedWindow{x:2100.0,y:120.0,width:480.0,height:600.0})`, speichern, laden → gleich.
  - `detached_bad_entries_dropped`: JSON mit `"detached": {"tree": {"x": 1, "y": 2, "width": -5, "height": 10}, "right": "kaputt", "dialog:x": {"x": 10, "y": 20, "width": 300, "height": 400}}` und `"language": "en"` → `detached` enthält nur `"dialog:x"`, `language == Some("en")`. Breite/Höhe ≤ 0 oder nicht endlich verwerfen; `x`,`y` dürfen negativ sein (Monitor links vom Hauptmonitor).
  - `detached_whole_field_garbage`: `"detached": 5` → leer, übrige Felder gelesen.
- [ ] **Step 2:** `cargo test -p planora-project detached` → FAIL (Typ fehlt).
- [ ] **Step 3:** Struct, Feld und `fn lenient_detached` (Muster wie `lenient_favorites`) einbauen.
- [ ] **Step 4:** `cargo test -p planora-project` → PASS.
- [ ] **Step 5:** Commit `feat(project): config.json merkt abgedockte Fenster`.

### Task 2: Kern `detach.rs` – Plätze, Zustand, Monitorprüfung, Umrechnung

**Files:**
- Create: `crates/planora-app/src/detach.rs` (in `lib.rs` als `pub mod detach;`)
- Modify: `crates/planora-app/Cargo.toml` (windows-sys Feature `Win32_Graphics_Gdi`)
- Modify: `crates/planora-app/src/workspace.rs` (Feld `pub detach: detach::DetachState` in `Workspace`, beim Start aus `Config` gefüllt)

**Interfaces:**
- Consumes: `planora_project::config::{Config, DetachedWindow}` (Task 1).
- Produces:
  - `#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub enum Place { Tree, Right, SketchSide, Dialog(&'static str) }` mit `pub fn key(self) -> String` → `"tree"`, `"right"`, `"sketch_side"`, `"dialog:<schlüssel>"`.
  - `pub const DIALOG_KEYS: &[&str]` – alle Dialogschlüssel (in Task 5 gefüllt), `debug_assert!` in `Place::Dialog`-Nutzung, dass der Schlüssel darin steht.
  - `pub struct DetachState` mit `pub fn from_config(c: &Config) -> Self`, `pub fn get(&self, p: Place) -> Option<DetachedWindow>`, `pub fn undock(&mut self, p: Place)`, `pub fn set(&mut self, p: Place, w: DetachedWindow)`, `pub fn take_dirty(&mut self) -> bool` (geändert seit letztem Speichern), `pub fn save(&self)` (über `Config::update_at`, nur Feld `detached` ersetzen).
  - `pub fn on_screen(rect_px: egui::Rect) -> Option<bool>`: Windows: Kopfstreifen `Rect::from_min_size(rect_px.min, vec2(64.0, 24.0))` per `MonitorFromRect(.., MONITOR_DEFAULTTONULL)` ≠ 0; sonst `None`.
  - `pub fn should_detach(saved: Option<DetachedWindow>, on_screen: impl Fn(egui::Rect) -> Option<bool>) -> Option<DetachedWindow>`: `None` wenn nicht gespeichert oder `on_screen == Some(false)`; sonst `Some`.
  - `pub fn to_physical(rect_points: egui::Rect, pixels_per_point: f32) -> DetachedWindow` und `pub fn to_points(w: DetachedWindow, pixels_per_point: f32) -> egui::Rect`.

- [ ] **Step 1: Tests** (`#[cfg(test)] mod tests` in `detach.rs`): `place_keys` (die vier Formen), `should_detach_missing_monitor_keeps_docked` (`on_screen = |_| Some(false)` → `None`), `should_detach_unknown_platform_trusts_saved` (`|_| None` → `Some`), `physical_round_trip_125` (`to_points(to_physical(r,1.25),1.25) == r` bis 0,01), `state_undock_marks_dirty`.
- [ ] **Step 2:** `cargo test -p planora-app --lib detach` → FAIL.
- [ ] **Step 3:** Implementieren; `Workspace` lädt `DetachState::from_config`, `PlanoraApp` ruft `if ws.detach.take_dirty() { ws.detach.save() }` am Ende von `update` (nicht in jedem Bild schreiben: nur bei `dirty`).
- [ ] **Step 4:** `cargo test -p planora-app --lib detach` → PASS; `cargo build -p planora-app` unter Windows.
- [ ] **Step 5:** Commit `feat(app): Abdock-Zustand je Platz mit Monitorprüfung`.

### Task 3: Layout ohne abgedockte Spalten

**Files:**
- Modify: `crates/planora-app/src/shell.rs` (`layout_in_mode`, `layout_with_top_min`), `crates/planora-app/src/shell_panels.rs` (`show_fields` übergibt den Zustand)
- Test: `crates/planora-app/tests/shell_layout.rs`

**Interfaces:**
- Produces: `#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)] pub struct Undocked { pub tree: bool, pub right: bool, pub sketch_side: bool }` in `shell.rs`; `layout_in_mode(content, mode, state, top, bottom_width, status_width, read_only, undocked: Undocked)` (neuer letzter Parameter; alle Aufrufer anpassen, Tests mit `Undocked::default()`).
- Regel: abgedockter Platz → sein Rechteck ist leer (`Rect::from_min_size(ecke, Vec2::ZERO)`), die freie Fläche und die zwischen den Spalten liegenden Felder (oben, Verlaufsleiste, Werkzeugfeld der Zeichnung, Hinweis) nutzen die frei gewordene Breite, als stünde dort eine Spalte der Breite 0 (der Außenrand `SHELL_INSET` bleibt). `tree` wirkt in Modell/Zeichnung, `right` in Modell/Zeichnung, `sketch_side` nur in Skizze.

- [ ] **Step 1: Tests** in `shell_layout.rs`: `undocked_default_is_unchanged` (für alle drei Modi bei 1920×1080 und 1280×800: `layout_in_mode(..., Undocked::default())` gleich den heutigen Erwartungswerten – vorhandene Tests rufen mit Default auf); `tree_undocked_widens_free_area_model` (`free.left() == content.left() + SHELL_INSET + GAP` bzw. das, was „Spalte der Breite 0“ ergibt; `left.width() == 0`); `right_undocked_widens_free_area_drawing`; `sketch_side_undocked_widens_free_area_sketch`; `history_bar_follows_free_area_when_tree_undocked`.
- [ ] **Step 2:** `cargo test -p planora-app --test shell_layout` → FAIL.
- [ ] **Step 3:** Implementieren; `show_fields` baut `Undocked` aus `app.ws.detach` und zeichnet `left_field`/`right_field` nicht, wenn der Platz abgedockt **und** `!ctx.embed_viewports()` ist (die Zeichnung im eigenen Fenster kommt in Task 6).
- [ ] **Step 4:** `cargo test -p planora-app --test shell_layout --test shell_panels --test viewport_shell` → PASS.
- [ ] **Step 5:** Commit `feat(app): Layout lässt abgedockte Spalten aus`.

### Task 4: Fenster-Baustein `detach::show_window` und Test-Harness

**Files:**
- Modify: `crates/planora-app/src/detach.rs`
- Modify: `crates/planora-app/tests/common/mod.rs` (Harness mit Kindfenstern)
- Create: `crates/planora-app/tests/detach_window.rs`

**Interfaces:**
- Consumes: Task 2.
- Produces:
  - `pub struct WindowSpec<'a> { pub place: Place, pub title: &'a str, pub inner_size: egui::Vec2, pub resizable: bool, pub focus_on_open: bool }`
  - `pub enum WindowEvent { None, DockClicked, CloseRequested }`
  - `pub fn show_window<R>(ctx: &egui::Context, state: &mut DetachState, spec: WindowSpec, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> (Option<R>, WindowEvent)`: öffnet `ctx.show_viewport_immediate(ViewportId::from_hash_of(place.key()), builder, …)`; Builder: Titel `"PlanoraCAD – {title}"`, `with_decorations(false)`, `with_taskbar(false)`, Lage aus `state.get(place)` per `to_points(.., root_ppp)`, Größe `inner_size`, `with_resizable(resizable)`, `with_active(focus_on_open)`. Inhalt in `CentralPanel` mit `theme::panel_frame(dark)`. Kopfzeile: Titel, Symbol „Andocken“ (Klick oder Doppelklick auf die Kopfzeile → `DockClicked`), Ziehen an der Kopfzeile → `ViewportCommand::StartDrag`; bei `resizable` Randstreifen 6 px → `BeginResize`. `close_requested` des Kindfensters → `send_viewport_cmd(CancelClose)` und `CloseRequested`. Nach Ziehen/Größenänderung (Maus losgelassen und `outer_rect` geändert) → `state.set(place, to_physical(outer_rect, ppp))`.
  - **Lage-Korrektur (Windows, DPI):** Im zweiten Bild eines neu geöffneten Fensters `outer_rect × pixels_per_point` mit der gewollten physischen Lage vergleichen; weicht sie > 2 px ab, einmal `ViewportCommand::OuterPosition(gewollt / aktueller_ppp)` senden (winit rechnet mit dem Faktor des Fensters zur Zeit des Befehls).
  - Bei `ctx.embed_viewports()` gibt `show_window` nie ein Fenster aus: `debug_assert!` und Rückgabe `(None, WindowEvent::None)`; Aufrufer prüfen vorher `detach::can_detach(ctx) -> bool` (`!ctx.embed_viewports()`).
  - Test-Harness: `Screen::with_viewports()` setzt `ctx.set_embed_viewports(false)` und `ctx.set_immediate_viewport_renderer(..)`, der jedes Kindfenster mit **eigener** `RawInput` aus einer Warteschlange `Screen::child_events: HashMap<ViewportId, Vec<egui::Event>>` ausführt und dessen Texte sammelt; `Screen::frame_child(id, events, f)` legt Ereignisse für das Kindfenster ins nächste Bild; `Screen::child_seen(id) -> Vec<Seen>`, `Screen::child_commands(id) -> Vec<ViewportCommand>`.

- [ ] **Step 1: Tests** `detach_window.rs`: `window_shows_contents_in_child_viewport` (Text des Inhalts erscheint in `child_seen`, nicht im Hauptfenster); `dock_click_reports_dock`; `close_request_is_cancelled_and_reported` (Kind-`RawInput` mit `viewports[id].events = [Close]` → `CancelClose` in `child_commands`, Ereignis `CloseRequested`); `drag_end_stores_physical_rect`; `embed_viewports_never_opens_window`.
- [ ] **Step 2:** `cargo test -p planora-app --test detach_window` → FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** → PASS; alle übrigen App-Tests unverändert (`cargo test -p planora-app`).
- [ ] **Step 5:** Commit `feat(app): eigenes Fenster für abgedockte Inhalte`.

### Task 5: Dialoge abdockbar

**Files:**
- Modify: `crates/planora-app/src/dialog.rs` (`Dialog::dock_key`, Symbol in `header`, Zweig in `show_inner`), `detach.rs` (`DIALOG_KEYS` füllen), alle Dateien mit `Dialog::new` außer festen/ausgenommenen (Liste per `grep -rn "Dialog::new" crates/planora-app/src`, inkl. `insert/dxf_dialog.rs`, `svg_dialog.rs`, `canvas_dialog.rs`), `crates/planora-i18n/languages/de.ftl`/`en.ftl` (`dialog-undock = Abdocken`/`Undock`, `dialog-dock = Andocken`/`Dock`)
- Create: `crates/planora-app/tests/dialog_dock_keys.rs`, Tests in `tests/detach_window.rs`

**Interfaces:**
- Consumes: Task 2, 4.
- Produces: `Dialog::dock_key(self, key: &'static str) -> Self`. Feature-Dialoge mit wechselndem Titel: ein Schlüssel je Feature-Art (z. B. `extrude`, `fillet`, `chamfer`, `box`, `cylinder` …) – der Aufrufer wählt ihn aus der Art.
- Verhalten in `show_inner`: mit Schlüssel und `detach::can_detach(ctx)`: Kopfzeile zeigt links vom Schließen-Symbol „Abdocken“ (`Icon` aus der Phosphor-Schrift, Tooltip/Bildschirmleser `tr!("dialog-undock")`). Klick → `state.set(place, to_physical(aktuelles_rect, ppp))` (springt an der jetzigen Stelle heraus). Ist `should_detach(state.get(place), on_screen)` `Some`: Inhalt über `detach::show_window` (Breite `limits(..)`-Breite des Dialogs, Höhe aus der Inhaltsmessung über `ViewportCommand::InnerSize`, höchstens Monitorhöhe – Rand, sonst rollt der Inhalt), Schließen-Symbol und Fußzeile wie heute, `DockClicked` → `state.undock(place)`, `CloseRequested` → `*open = false`. Die `DetachState` erreicht `Dialog` über `ctx.data` (Task 2 legt sie dort zu Bildbeginn per `detach::install(ctx, &mut ws.detach)` ab und holt sie danach zurück) – **Festlegung des Plans**, damit die 47 Aufrufe ihre Signatur behalten.

- [ ] **Step 1: Tests** `dialog_dock_keys.rs`: liest alle `.rs` unter `crates/planora-app/src`, zählt je Datei `Dialog::new(` und prüft, dass jeder Aufruf im selben Ausdruck (bis zum nächsten `.show`) genau eines von `.dock_key("…")`, `.fixed()`, `.bottom_right_in(` hat oder in `crash_dialog.rs` steht; alle Schlüssel stehen in `DIALOG_KEYS`; `DIALOG_KEYS` ohne Doppelte. In `detach_window.rs`: `undock_symbol_only_with_viewports` (normaler `Screen`: kein Text/Tooltip „Abdocken“); `undocked_dialog_renders_in_child` (Blend-Dialog mit gesetztem Eintrag → Titel in `child_seen`); `missing_monitor_opens_docked_and_keeps_entry` (Eintrag mit x = −100000 unter Windows → Dialog im Hauptfenster, Eintrag noch da; unter Linux übersprungen); `dock_click_returns_to_main_window`.
- [ ] **Step 2:** → FAIL.
- [ ] **Step 3:** Implementieren, alle Aufrufe mit Schlüssel versehen.
- [ ] **Step 4:** `cargo test -p planora-app` → PASS (alle bestehenden Dialogtests unverändert).
- [ ] **Step 5:** Commit `feat(app): Dialoge lassen sich abdocken`.

### Task 6: Felder abdockbar (Baum, rechte Spalte, Ansichtsfeld, Skizzenfeld) und Unterdialoge darüber

**Files:**
- Modify: `crates/planora-app/src/shell_panels.rs` (`left_field`, `right_field`, Feldkopf-Symbol), `drawing_view.rs` (Ansichtsfeld um Zeile 1726), `sketch_panel.rs` (`show`, Skizzenfeld), `dialog.rs` (Unterdialog über Feldfenster)
- Test: `crates/planora-app/tests/detach_panels.rs`

**Interfaces:**
- Consumes: Task 3, 4, 5.
- Produces: `fn panel_dock_button(ui: &mut egui::Ui, place: Place, state: &mut DetachState, rect_points: egui::Rect)` (Symbol oben rechts im Feld, nur bei `can_detach`); `pub fn host_viewport_rect(ctx: &egui::Context) -> Option<egui::Rect>` in `detach.rs` – Bildschirm-Rechteck (Punkte) des Fensters, in dem gerade gezeichnet wird, wenn es ein abgedocktes **Feld** ist.
- Felder: abgedockt → Inhalt über `detach::show_window` mit `resizable: true`, Startgröße = heutiges Spaltenrechteck, Höhe frei. Ein Platz, den der Modus nicht zeigt, ruft `show_window` nicht (egui schließt das Fenster dann selbst). `focus_on_open: false`.
- Unterdialoge (Spec „Rückfragen und Unterdialoge aus abgedockten Feldern“): Läuft `Dialog::show_inner` in einem abgedockten Feld (`host_viewport_rect` ist `Some`), öffnet er ein eigenes immediate-Fenster (Id `place.key() + titel`), mittig über `host_viewport_rect`, ohne Abdock-Symbol, ohne Speichern; feste Rückfragen ebenso. Sonst wie heute.

- [ ] **Step 1: Tests** `detach_panels.rs` (mit `Screen::with_viewports()` und `shell_app`-Aufbau wie in `tests/shell_app.rs`): `tree_undocked_draws_tree_in_child_and_frees_column`; `right_undocked_shows_properties_in_child`; `drawing_view_field_follows_right_place`; `sketch_side_window_appears_only_in_sketch` (Skizze beginnen → Kindfenster da; beenden → im nächsten Bild kein Kindfenster, Layout ohne Spalte stimmt); `undo_back_into_sketch_with_undocked_side` (Review Focus 4); `rename_from_undocked_tree_opens_over_tree_window` (Unterdialog-Kindfenster mittig über dem Baumfenster, nicht im Hauptfenster); `delete_prompt_from_drawing_view_stays_in_main`.
- [ ] **Step 2:** → FAIL. **Step 3:** Implementieren. **Step 4:** `cargo test -p planora-app` → PASS.
- [ ] **Step 5:** Commit `feat(app): Baum, rechte Spalte und Skizzenfeld abdockbar`.

### Task 7: Tastatur in abgedockten Fenstern

**Files:**
- Modify: `crates/planora-app/src/app.rs` (`shortcuts` in `shortcuts_in(&mut self, ctx, only_all: bool)` aufteilen), `detach.rs` (Haken nach dem Inhalt), `keymap/overview.rs::take_escape`, `feedback_dialog.rs::take_escape` und jede Stelle, die Esc/Enter eines abdockbaren Dialogs außerhalb seiner Closure liest (Liste per `grep -rn "Key::Escape\|Key::Enter" crates/planora-app/src` prüfen)
- Test: `crates/planora-app/tests/detach_keys.rs`

**Interfaces:**
- Consumes: Task 4–6.
- Produces: `detach::show_window` bekommt `after: impl FnOnce(&egui::Context)`, aufgerufen im Kind-Callback nach dem Inhalt mit dem Kind-`ctx`. Felder geben dort `|ctx| app.shortcuts_in(ctx, false)`, Dialoge `|ctx| app.shortcuts_in(ctx, true)` mit (nur Bereich `All`, Regel aus `app.rs:1247`). Wo der Haken `app` nicht erreicht (Dialog tief im Aufrufbaum), legt `detach` die Kind-Ereignisse der nicht verbrauchten Tasten in `ctx.data` ab und `PlanoraApp::update` ruft am Bildende `shortcuts_from_children(&ctx)` – **genau eine** der beiden Formen je Fenster; der Umsetzer wählt die, die ohne Leihkonflikt geht, und der Test sichert „genau einmal“.
- Esc im Hauptfenster bei abgedocktem Befehlsdialog: Der Dialog prüft im Kind-Callback zusätzlich `ctx.input_for(ViewportId::ROOT, |i| i.key_pressed(Key::Escape))` (nicht verbraucht) → wie sein eigenes Esc (Abbrechen). Enter aus dem Hauptfenster nicht.

- [ ] **Step 1: Tests** `detach_keys.rs`: `ctrl_z_in_undocked_tree_undoes_once` (Quader anlegen, Baum abgedockt, Strg+Z als Kind-Ereignis → Dokument zurück, Verlauf genau ein Schritt); `delete_in_undocked_tree_deletes_selected_feature`; `s_in_undocked_tree_opens_overview`; `ctrl_s_in_undocked_dialog_saves_once` (Speichern-Zähler bzw. Pfad wie in `shell_commands.rs`); `model_keys_blocked_in_undocked_dialog` (Entf im abgedockten Blend-Dialog löscht nichts); `escape_in_undocked_dialog_closes_it`; `enter_in_undocked_dialog_applies`; `escape_in_main_cancels_undocked_command_dialog`; `typing_in_undocked_field_does_not_trigger_shortcuts` (Text „l“ im Umbenennen-Feld des abgedockten Baums startet kein Linienwerkzeug).
- [ ] **Step 2:** → FAIL. **Step 3:** Implementieren. **Step 4:** `cargo test -p planora-app` → PASS.
- [ ] **Step 5:** Commit `feat(app): Tastenkürzel und Esc/Enter in abgedockten Fenstern`.

### Task 8: Vorschau zieht nach

**Files:**
- Modify: `crates/planora-app/src/detach.rs` (nach einer Änderung im Dialog-Kindfenster `ctx.request_repaint_of(ViewportId::ROOT)`, wenn `ctx.input_for(ROOT, …)`-Bild die 3D-Ansicht schon gezeichnet hat – einfach: immer, wenn das Kindfenster in diesem Bild Ereignisse hatte)
- Test: `crates/planora-app/tests/detach_keys.rs` oder eigene Datei `detach_preview.rs`

- [ ] **Step 1: Test** `blend_radius_in_undocked_dialog_updates_preview_next_frame`: Blend-Dialog abgedockt, Radius im Kindfenster tippen → spätestens im nächsten Hauptbild ohne Eingabe hat die Vorschau den neuen Radius (Zustandswert der Vorschau aus `AppState`), und `repaint_delay` des Hauptfensters ist 0 nach dem Tippen.
- [ ] **Step 2:** FAIL → **Step 3:** implementieren → **Step 4:** PASS.
- [ ] **Step 5:** Commit `fix(app): Vorschau folgt Eingaben im abgedockten Dialog`.

### Task 9: Doku, offene Punkte, Gesamtprüfung

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Abschnitt „Fenster abdocken“: Wayland ohne Lage, kein Besitzerfenster, Linux ohne Monitorprüfung, Fusion-Namen der Symbole ungeprüft, Handliste), `README.md` (Bedienung kurz), `CLAUDE.md` des Repos (eine Zeile: neue Dialoge brauchen `.dock_key`)

- [ ] **Step 1:** Doku schreiben; Handliste aus Spec „Prüfung“ übernehmen.
- [ ] **Step 2:** Windows: `cargo fmt -p planora-app -- --check`, `cargo fmt -p planora-project -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (bekannt: `real_step_text_limit` scheitert auch auf master), `cargo deny check` → grün.
- [ ] **Step 3:** WSL: `~/wsl-s6-full.sh` im Hintergrund → grün bis auf bekannte Ausnahmen; danach Aufräumen laut CLAUDE.md.
- [ ] **Step 4:** `graphify . --update --code-only` im Hauptordner nach dem Merge (nicht im Worktree).
- [ ] **Step 5:** Commit `docs: Fenster abdocken – Bedienung und offene Punkte`.
