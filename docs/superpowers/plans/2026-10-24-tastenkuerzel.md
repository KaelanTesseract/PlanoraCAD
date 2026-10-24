# Tastenkürzel wie in Fusion, änderbar – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Alle Tasten und Mausgesten gehen über eine zentrale, änderbare Tabelle mit Fusion-Voreinstellung; Einstellungen zum Ändern; Übersicht mit Suche.

**Architecture:** Neues Modul `keymap` in `planora-app` (Aktionen, Bereiche, Tasten/Mausgesten, Voreinstellungen, Doppelbelegung, Text). Abweichungen stehen in `planora-project::Config`. Ein Verteiler ersetzt das feste Tastenabfragen in `app.rs`, Menüs/Tooltips zeigen Tasten aus der Tabelle. Die 3D-Ansicht liest ihre Mausgesten aus der Tabelle.

**Tech Stack:** Rust, egui/eframe 0.36, `planora-app`, `planora-project` (Config). Keine neuen Abhängigkeiten.

**Spec:** `docs/superpowers/specs/2026-10-24-tastenkuerzel-design.md` (Abschnitt „Festlegungen nach der Bauherren-Prüfung“ gilt mit).

## Global Constraints

- Oberflächentexte Deutsch, einfache Sprache, Satzanfang groß; geprüft bei 1920 × 1080 (kleinere Fenster sind kein Ziel).
- Voreinstellung ist Fusion (Tabelle in der Spec); heutige Kürzel Strg+N, Strg+O, Strg+S, Strg+W, Strg+Umschalt+E, Strg+Z, Strg+Y und Strg+Umschalt+Z bleiben mit gleichem Verhalten.
- Die heutigen Sperren der Kürzel (Textfeld aktiv, offene Rückfrage, nur erster Durchgang eines Bildes, Rückgängig/Wiederholen gesperrt bei offenen Fenstern und gedrückter Maustaste) gelten unverändert für alle Kürzel.
- Config: nur Abweichungen von der Voreinstellung speichern; beschädigte/unbekannte Einträge ignorieren und melden; Tests nur mit temporärem `config_path`, nie echtes `%APPDATA%`.
- Nur Tastenbelegungen als Tatsachen aus der Fusion-Liste, keine Autodesk-Texte oder Grafiken (`cargo deny` bleibt grün, keine neuen Abhängigkeiten).
- Aktionen für Funktionen, die es nicht gibt, kommen nicht in die Tabelle (Liste „noch nicht vorhanden“ im Bericht von Task 8).
- Agenten bauen keine Release-Builds und starten kein Fenster; Sichtprüfungen macht der Controller.

## Review Focus

- Kein Kürzel feuert in einem Textfeld, bei offener Rückfrage oder offenem Fenster; keine Taste löst zweimal aus (Task 2).
- Nach dem Ändern gilt genau die neue Belegung, die alte ist frei; Doppelbelegung inkl. „Alle“ (Task 1, 6).
- Beschädigte Konfiguration macht das Programm nie unbedienbar (Task 1).
- Mausgesten: links dreht weiter, mittlere schwenkt, Umschalt+mittlere dreht, Rad zoomt; Skizze/Zeichnung behalten Schwenken/Zoomen; Gesten sind änderbar (Task 3).
- Leertaste wiederholt nur, wenn kein Bedienelement den Fokus hat (Task 4).
- Menüs, Werkzeugleiste und Tooltips zeigen nach einer Änderung sofort die neue Taste (Task 2, 4, 5, 6).

---

### Task 1: Keymap-Modell und Config-Speicher

**Files:**
- Create: `crates/planora-app/src/keymap/mod.rs` (Typen, Tabelle), `crates/planora-app/src/keymap/defaults.rs` (Fusion-Voreinstellung), `crates/planora-app/src/keymap/text.rs` (Anzeige und Parsen), `mod keymap;` in `lib.rs`/`main.rs` wie die anderen Module
- Modify: `crates/planora-project/src/config.rs` (neuer optionaler Abschnitt), `crates/planora-app/src/workspace.rs` (`Workspace::keymap`, laden/speichern)
- Test: `crates/planora-app/tests/keymap.rs`, `crates/planora-project/tests/config_keys.rs` (Namen nach Muster der vorhandenen Config-Tests)

**Interfaces:**
- Produces (alle `pub` in `planora_app::keymap`):
  - `enum Area { All, Model, Sketch, Drawing }` (`Area::label()`: „Überall“, „Modell“, „Skizze“, „Zeichnung“)
  - `enum Action { … }` mit `Action::id(self) -> &'static str` (stabiler Name, z. B. `"datei.neu"`), `Action::label(self) -> &'static str` (deutsch), `Action::area(self) -> Area`, `Action::ALL: &'static [Action]`. Task 1 enthält die schon vorhandenen Aktionen: `New, Projects, Save, Close, Export, Undo, Redo` (Bereich `All`) und die Ansicht-Gesten `ViewPan, ViewOrbit, ViewZoom` (Bereich `All`). Weitere Aktionen ergänzen Tasks 4/5.
  - `struct KeyChord { key: egui::Key, ctrl: bool, shift: bool, alt: bool }` (`ctrl` heißt `Modifiers::COMMAND`).
  - `enum MouseButton { Left, Middle, Right }`; `enum Gesture { Drag { button: MouseButton, ctrl: bool, shift: bool, alt: bool }, Wheel { ctrl: bool, shift: bool, alt: bool } }`.
  - `enum Binding { Key(KeyChord), Mouse(Gesture) }`.
  - `struct Keymap` mit: `Keymap::defaults() -> Keymap`; `bindings(&self, a: Action) -> &[Binding]`; `is_default(&self, a: Action) -> bool`; `set(&mut self, a: Action, index: usize, b: Binding) -> SetResult` (`enum SetResult { Ok, Conflict(Action) }`, ändert nichts bei Konflikt); `swap(&mut self, a: Action, index: usize, b: Binding)` (nimmt dem Konflikt-Partner die Taste und setzt sie bei `a`; der Partner verliert diese Belegung ersatzlos); `reset(&mut self, a: Action)`; `reset_all(&mut self)`; `conflict_for(&self, a: Action, b: &Binding) -> Option<Action>` (gleicher `Area` oder eine Seite `All`; Tasten und Gesten werden nie gegeneinander verglichen); `matches_key(&self, a: Action, chord: &KeyChord) -> bool`; `action_for_key(&self, area: Area, chord: &KeyChord) -> Option<Action>` (Bereich zuerst, dann `All`).
  - `keymap::text`: `chord_text(&KeyChord) -> String` („Strg+Umschalt+E“, „Entf“, „Leertaste“, „E“), `gesture_text(&Gesture) -> String` („Mittlere Taste ziehen“, „Umschalt + Mittlere Taste ziehen“, „Mausrad“), `binding_text`, `parse_binding(&str) -> Option<Binding>` (Umkehrung), `Keymap::text(&self, a: Action) -> String` (alle Belegungen, mit „ / “ getrennt; leer → „–“).
  - Config (`planora_project::config::Config`): `pub keys: BTreeMap<String, Vec<String>>` (Aktion-Id → Belegungstexte), `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`; Lesen tolerant wie die Durchsichtigkeit (kein Absturz bei falschem Typ → leer).
  - `Keymap::from_overrides(&BTreeMap<String, Vec<String>>) -> (Keymap, Vec<String>)` (Hinweistexte für ignorierte Einträge: unbekannte Aktion, ungültige Belegung, Doppelbelegung in der Datei → die erste gewinnt, die weiteren werden gemeldet); `Keymap::to_overrides(&self) -> BTreeMap<String, Vec<String>>` (nur Aktionen, die von der Voreinstellung abweichen; eine Aktion ohne Belegung wird als leere Liste gespeichert).
  - `Workspace::keymap: Keymap`, `Workspace::key_notes: Vec<String>` (Hinweise beim Laden), `Workspace::set_keymap(&mut self, km: Keymap) -> bool` (speichert über `Config::update_at(&self.config_path, …)` nur den Abschnitt `keys`; `false` bei Schreibfehler; gleicher Merkmechanismus wie `panel_transparency_pending`).

Voreinstellung in Task 1 (Windows): New Strg+N; Projects Strg+O; Save Strg+S; Close Strg+W; Export Strg+Umschalt+E; Undo Strg+Z; Redo Strg+Y **und** Strg+Umschalt+Z; ViewPan: Mitte ziehen **und** Umschalt+Links ziehen; ViewOrbit: Umschalt+Mitte ziehen **und** Links ziehen; ViewZoom: Rad.

- [ ] **Step 1: Tests schreiben** in `tests/keymap.rs`: `defaults_match_the_spec_table_for_existing_actions` (alle oben genannten Belegungen); `conflict_within_area_and_with_all` (gleiche Taste in Skizze und Modell erlaubt, in Skizze und Überall nicht); `set_refuses_on_conflict_and_changes_nothing`; `swap_gives_the_key_to_the_new_action_and_frees_the_old_one`; `tastes_and_gestures_never_conflict_with_each_other`; `text_roundtrip_for_every_default_binding` (`parse_binding(binding_text(b)) == Some(b)`); `from_overrides_ignores_unknown_action_invalid_binding_and_duplicates_and_reports_them`; `to_overrides_writes_only_differences_and_roundtrips`; `reset_one_and_all`; Config: `config_without_keys_reads_as_defaults`, `config_with_wrong_type_reads_empty`, `keys_survive_other_config_updates` und umgekehrt (Favoriten/Root/Durchsichtigkeit bleiben), `set_keymap_writes_only_keys_in_a_temp_config`.
- [ ] **Step 2:** Tests ausführen → Fehlschlag (Typen fehlen).
- [ ] **Step 3: Implementieren** wie in Interfaces. `Action::ALL` und `defaults()` als eine Tabelle (eine Zeile je Aktion: Variante, Id, Anzeigename, Bereich, Voreinstellungen), damit neue Aktionen nur eine Zeile brauchen.
- [ ] **Step 4:** `cargo test -p planora-app -p planora-project` und `cargo clippy -p planora-app -p planora-project --all-targets -- -D warnings`, `cargo fmt --check`.
- [ ] **Step 5: Commit** `feat(app): Keymap-Modell und Speicher der Tastenbelegung`.

### Task 2: Verteiler und Anzeige der Tasten

**Files:**
- Modify: `crates/planora-app/src/app.rs` (`shortcuts`, `undo_keys`, `shortcut_button`-Aufrufe ~Z. 507–562 und ~837–844), `crates/planora-app/src/shell_panels.rs` (~554–562), `crates/planora-app/src/tabs.rs` (~55–88), `crates/planora-app/src/prompts.rs` (~212, Hinweistext „Strg+O“)
- Test: `crates/planora-app/tests/keymap_dispatch.rs`

**Interfaces:**
- Consumes: Task 1 (`Keymap`, `Action`, `KeyChord`, `Workspace::keymap`).
- Produces: `keymap::dispatch::pressed(ctx: &egui::Context, km: &Keymap, area: Area) -> Vec<Action>` (liest `ctx.input_mut`, verbraucht passende Tastenereignisse über `consume_key`, gibt jede Aktion höchstens einmal je Bild zurück; Reihenfolge: mehr Umschalttasten zuerst, damit Strg+Umschalt+Z vor Strg+Z kommt); `keymap::dispatch::current_area(state) -> Area` (Skizze offen → `Sketch`, Zeichnung offen → `Drawing`, sonst `Model`); `fn shortcut_hint(km: &Keymap, a: Action) -> String` (für Menütexte); `shortcut_button` bekommt den Tastentext aus `km.text(a)` statt fester Zeichenkette.
- Behaviour: Die bestehenden Sperrbedingungen aus `shortcuts` und `undo_keys` bleiben wörtlich erhalten (`viewport::typing`, `ws.prompt`, `ctx.current_pass_index() != 0`, bei Undo/Redo zusätzlich `about_open`, `dialog_open`, Projekte-Dialoge, `settings.is_open()`, offene Versions-Rückfrage, `pointer.any_down()`); dazu kommt: ein Tastaturfokus auf einem Bedienelement (`ctx.memory(|m| m.focused()).is_some()`), das keine Eingabe ist, sperrt nur die Leertaste, nicht die anderen Kürzel. Die Reihenfolge der Ausführung bleibt wie heute.

- [ ] **Step 1: Tests** (egui-Kontext ohne Fenster mit rohen Ereignissen, Muster der vorhandenen UI-Tests): `default_keys_still_trigger_the_same_actions` (alle acht heutigen Kürzel); `ctrl_shift_z_is_redo_and_not_undo`; `no_key_fires_while_typing_or_prompt_or_second_pass` (je ein Fall); `undo_redo_stay_blocked_by_open_windows_and_pressed_mouse_button` (Fälle wie `undo_keys`); `a_held_key_fires_once_per_frame`; `rebinding_changes_dispatch_immediately` (Save auf Strg+Umschalt+S gesetzt: Strg+S tut nichts, neue Taste speichert); `menu_and_tooltip_text_follow_the_keymap` (nach dem Ändern zeigt `shortcut_hint` die neue Taste; Strings in `tabs.rs`/`shell_panels.rs` nicht mehr fest); `prompt_hint_uses_the_current_projects_key`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.** `shortcuts`/`undo_keys` rufen `dispatch::pressed` auf und führen die Aktionen wie bisher aus (Aktion → vorhandene Funktion). Feste Texte „Strg+…“ in Menüs, Tabs, Tooltips und Hinweisen durch Aufrufe der Tabelle ersetzen.
- [ ] **Step 4:** `cargo test -p planora-app` (voll), clippy, fmt.
- [ ] **Step 5: Commit** `feat(app): Verteiler der Kürzel über die Tastenbelegung`.

### Task 3: Maussteuerung wie in Fusion

**Files:**
- Modify: `crates/planora-app/src/viewport.rs` (~Z. 586–595 Schwenken/Drehen, ~644 Zoom, Navigationswürfel ~600–620), `crates/planora-app/src/sketch_view.rs` (~Z. 461–480 Schwenken, Zoom), `crates/planora-app/src/drawing_view.rs` (~Z. 1119–1130 Zoom, Schwenken), `crates/planora-app/src/keymap/mod.rs` (Gesten-Abfrage)
- Test: `crates/planora-app/tests/keymap_mouse.rs`

**Interfaces:**
- Consumes: Task 1 (`Gesture`, `Action::ViewPan/ViewOrbit/ViewZoom`), Task 2 (Kontext).
- Produces: `keymap::mouse::drag_action(km: &Keymap, ui: &egui::Ui, response: &egui::Response, allowed: &[Action]) -> Option<Action>` (welche Aktion gehört zu der gerade laufenden Ziehgeste; genauer Vergleich von Taste und Umschalttasten; bei Mehrdeutigkeit gewinnt die mit mehr Umschalttasten); `keymap::mouse::wheel(km, ui) -> f32` (Zoomwert, wenn eine `ViewZoom`-Geste mit passenden Umschalttasten zutrifft).
- Behaviour: 3D-Ansicht: `ViewPan` schwenkt, `ViewOrbit` dreht, `ViewZoom` zoomt, Würfel-Ziehen dreht weiter wie heute (`ViewOrbit` mit linker Taste). Skizze und Zeichnung: nur `ViewPan` und `ViewZoom` aus der Tabelle (Drehen gibt es dort nicht); die linke Taste zeichnet/wählt und `Umschalt+links`-Schwenken (Zusatz) bleibt. Die Vorrangregel in der Skizze „Umschalt gedrückt ⇒ nicht Punkt ziehen“ bleibt.

- [ ] **Step 1: Tests** (rohe Zeigerereignisse in einem egui-Kontext, `Camera`-Änderung prüfen): `left_drag_still_orbits_in_3d`; `middle_drag_pans_in_3d`; `shift_middle_drag_orbits_in_3d`; `shift_left_drag_still_pans_in_3d`; `wheel_zooms_in_3d_sketch_and_drawing`; `rebinding_pan_to_right_drag_moves_the_gesture` (alte Geste wirkt nicht mehr); `sketch_and_drawing_have_no_orbit`; `a_click_is_not_a_drag_gesture`; `cube_drag_still_orbits`; `drag_in_sketch_with_left_still_draws_not_pans`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.** Feste `PointerButton`-Abfragen in den drei Ansichten durch `mouse::drag_action`/`mouse::wheel` ersetzen; die Bewegungsberechnung (`state.camera.pan/orbit/zoom`, Skizzen-/Blatt-Zoom) bleibt unverändert.
- [ ] **Step 4:** `cargo test -p planora-app` (voll), clippy, fmt.
- [ ] **Step 5: Commit** `feat(app): Mausgesten über die Tastenbelegung (Fusion-Voreinstellung)`.
- [ ] **Sichtprüfung (Controller, 1920 × 1080):** Schwenken, Drehen (beide Wege), Zoomen in 3D, Skizze, Zeichnung.

### Task 4: Fusion-Kürzel im Modell und „Letzten Befehl wiederholen“

**Files:**
- Modify: `crates/planora-app/src/keymap/mod.rs` und `defaults.rs` (neue Aktionen), `crates/planora-app/src/app.rs`, `crates/planora-app/src/shell_panels.rs`, `crates/planora-app/src/browser.rs`, `crates/planora-app/src/state.rs` (nur wo nötig), Menü- und Werkzeugleisten-Code, in dem die Befehle ausgelöst werden (`open_boolean`, `open_blend`, `do_create`, Verschieben)
- Test: `crates/planora-app/tests/keymap_model.rs`

**Interfaces:**
- Consumes: Tasks 1–2; die vorhandenen Befehle.
- Produces: Aktionen im Bereich `Model` für alles, was es schon gibt, mit Fusion-Voreinstellung: `ExtrudeSketch` E (Skizze extrudieren, nur wenn eine Skizze gewählt ist), `Fillet` F (Verrunden/Fase, öffnet den bestehenden Dialog), `MoveBody` M (Verschieben, öffnet das bestehende Werkzeug), `ToggleVisibility` V (Auge des gewählten Körpers), `DeleteSelection` Entf (wie Kontextmenü „Löschen“, mit Rückfrage bei Abhängigen; auch bei gesperrten Körpern erlaubt), `RepeatLast` Leertaste. Die Aktions-Liste wird zuerst durch eine Bestandsaufnahme im Code bestimmt (Bericht): nur Funktionen, die es gibt; für jede fehlende Fusion-Aktion steht eine Zeile im Bericht. Kopieren/Ausschneiden/Einfügen nur, wenn es die Funktion gibt (heute vermutlich nicht).
- `struct LastCommand` (in `AppState` oder `Workspace`): `fn remember(&mut self, a: Action)` (nur Aktionen mit `Action::repeatable() == true`: die oben genannten Erzeugen-/Bearbeiten-Aktionen und die Skizzenwerkzeuge aus Task 5; Speichern, Rückgängig, Navigation nicht), `fn take(&self) -> Option<Action>`. Menü, Werkzeugleiste und Kürzel rufen dieselbe Ausführungsfunktion `run_action(ws, a)` auf, die `remember` aufruft; ein abgebrochener Dialog merkt nichts.
- Tooltips/Menüs für diese Befehle zeigen die Taste über `shortcut_hint`.

- [ ] **Step 1: Tests:** `each_model_key_runs_its_command` (je E, F, M, V, Entf: gleiches Ergebnis wie der Menüweg); `keys_do_nothing_without_a_matching_selection` (E ohne Skizze: Hinweis in der Statuszeile, keine Wirkung); `delete_key_asks_before_deleting_dependents_and_works_on_locked_bodies`; `space_repeats_the_last_command_via_menu_and_via_key`; `space_does_nothing_with_nothing_to_repeat`; `space_is_ignored_while_a_button_has_focus_and_while_typing`; `cancelled_dialog_is_not_remembered`; `save_undo_navigation_are_not_repeatable`; `rebound_model_key_works_and_old_key_is_free`; `tooltips_show_the_current_keys`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.** Menü-/Werkzeugleisten-Aufrufe und Kürzel gehen über `run_action`.
- [ ] **Step 4:** `cargo test -p planora-app` (voll), clippy, fmt.
- [ ] **Step 5: Commit** `feat(app): Fusion-Kürzel im Modell und Befehl wiederholen`.
- [ ] **Sichtprüfung (Controller):** E/F/M/V/Entf/Leertaste im Fenster.

### Task 5: Fusion-Kürzel in Skizze und Zeichnung

**Files:**
- Modify: `crates/planora-app/src/keymap/mod.rs`, `defaults.rs`, `crates/planora-app/src/sketch_view.rs`, `crates/planora-app/src/sketch_panel.rs`, `crates/planora-app/src/sketch_state.rs`, `crates/planora-app/src/drawing_view.rs`, `crates/planora-app/src/drawing_state.rs`, Zeichnungs-Werkzeugleiste/Menü
- Test: `crates/planora-app/tests/keymap_sketch_drawing.rs`

**Interfaces:**
- Consumes: Tasks 1, 2, 4 (`run_action`, `LastCommand`).
- Produces: Aktionen im Bereich `Sketch`: `SketchLine` L, `SketchRectangle` R, `SketchCircle` C, `SketchProjectEdge` P; weitere nur nach Bestandsaufnahme (nicht: Offset, Trim, Skizzenbemaßung, solange es sie nicht gibt). Bereich `Drawing`: die vorhandenen Zeichnungswerkzeuge mit ihren Fusion-Tasten (Ballon B, Mittelpunktmarke C, Bemaßung D, Verschieben M, Projizierte Ansicht P, Text T, Löschen Entf), soweit vorhanden; Zeichnung `Entf` behält das bisherige Verhalten. Die heutigen festen Skizzentasten Esc, Enter, Entf bleiben unverändert (nicht änderbar, nicht in der Tabelle). Skizzenwerkzeug-Wechsel per Taste aktiviert dasselbe wie der Klick auf die Schaltfläche; Tooltips/Schaltflächen zeigen die Taste.
- Behaviour: Skizzenkürzel gelten nur bei offener Skizze und nicht beim Tippen in Maß-/Formelfeldern; C in der Zeichnung ist nicht C in der Skizze.

- [ ] **Step 1: Tests:** `sketch_keys_select_the_tools`; `same_letter_means_different_things_in_sketch_and_drawing_and_nothing_in_model`; `sketch_keys_are_ignored_while_typing_in_dimension_or_formula_fields`; `esc_enter_delete_in_sketch_unchanged`; `drawing_tool_keys_select_the_tools`; `rebinding_a_sketch_key_frees_the_old_one`; `repeat_last_remembers_sketch_tools`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.**
- [ ] **Step 4:** `cargo test -p planora-app` (voll), clippy, fmt.
- [ ] **Step 5: Commit** `feat(app): Fusion-Kürzel in Skizze und Zeichnung`.
- [ ] **Sichtprüfung (Controller):** L/R/C/P in der Skizze, Zeichnungstasten.

### Task 6: Einstellungen – Abschnitt „Tastenkürzel“

**Files:**
- Modify: `crates/planora-app/src/settings_window.rs` (Abschnitt unter „Darstellung“ und „Projekte“), neues Modul `crates/planora-app/src/keymap/settings_ui.rs`
- Test: `crates/planora-app/tests/keymap_settings.rs`

**Interfaces:**
- Consumes: Tasks 1–5; `Workspace::set_keymap`, `Workspace::key_notes`.
- Produces: `keymap::settings_ui::show(ui, ws: &mut Workspace, state: &mut CaptureState)` mit `CaptureState { filter: String, capturing: Option<(Action, usize)>, pending_conflict: Option<(Action, usize, Binding, Action)>, … }`; Tabelle (Aktion, Bereich, Belegungen) mit Suchfeld (sucht Anzeigename, Bereich und Tastentext), je Belegung „Ändern“ und „Entfernen“, „Belegung hinzufügen“, „Zurücksetzen“ je Zeile und „Alles zurücksetzen“. Erfassen: Klick auf „Ändern“ → „Taste drücken …“; nächste Taste (mit Umschalttasten) übernimmt, Esc bricht ab; bei Mausaktionen („Ansicht schwenken/drehen/zoomen“) wird stattdessen die Geste erfasst (Taste halten und mindestens 4 Pixel ziehen, oder Rad drehen, jeweils mit gehaltenen Umschalttasten). Konflikt → Zeile „Strg+S ist schon für „Speichern“ belegt“ mit „Tauschen“ und „Abbrechen“. Schreibfehler → Hinweis wie beim Projektverzeichnis; Wert gilt für die Sitzung. `Workspace::key_notes` erscheinen oben im Abschnitt.
- Behaviour: Beim Erfassen sind alle Kürzel gesperrt (die erfasste Taste löst nichts aus). Änderungen gelten sofort und werden gespeichert.

- [ ] **Step 1: Tests:** `section_lists_every_action_with_current_keys`; `search_filters_by_name_area_and_key_text`; `capturing_a_key_sets_it_and_frees_the_old_one`; `capturing_does_not_trigger_the_captured_key`; `esc_cancels_capturing`; `conflict_shows_swap_or_cancel`; `swap_moves_the_key`; `capturing_a_mouse_gesture_needs_a_drag_or_wheel_not_a_click`; `reset_row_and_reset_all`; `save_failure_keeps_the_value_and_shows_a_hint`; `load_notes_are_shown`; `removing_the_last_binding_keeps_the_action_listed_as_unbound`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.**
- [ ] **Step 4:** `cargo test -p planora-app` (voll), clippy, fmt.
- [ ] **Step 5: Commit** `feat(app): Einstellungen für Tastenkürzel`.
- [ ] **Sichtprüfung (Controller):** Ändern, Tauschen, Zurücksetzen, Suche, Geste ändern.

### Task 7: Übersicht „Tastenkürzel“ mit Suche und S

**Files:**
- Create: `crates/planora-app/src/keymap/overview.rs`
- Modify: `crates/planora-app/src/app.rs` (Menü Ansicht: Eintrag „Tastenkürzel …“; Aktion `ShowShortcuts` mit Fusion-Taste S im Bereich `Model` und `Sketch`), Fensterausschlussregeln (wie die anderen Fenster)
- Test: `crates/planora-app/tests/keymap_overview.rs`

**Interfaces:**
- Consumes: Task 6 (Suche).
- Produces: `keymap::overview::show(ctx, ws, state: &mut OverviewState)` (Fenster „Tastenkürzel“, nur lesen, sortiert nach Bereich, Suchfeld sucht Aktion und Taste, Knopf „In den Einstellungen ändern“); `OverviewState { open: bool, filter: String, focus_search: bool }`.
- Behaviour: Taste S öffnet das Fenster mit aktivem Suchfeld (nur wenn kein Textfeld aktiv); solange das Suchfeld Fokus hat, lösen keine Kürzel aus; Fenster respektiert die Regeln für offene Fenster.

- [ ] **Step 1: Tests:** `view_menu_entry_opens_the_overview`; `s_opens_it_with_the_search_focused`; `search_finds_by_action_and_by_key_text_and_shows_empty_text`; `typing_in_search_triggers_no_shortcuts`; `shows_current_not_default_keys_after_a_change`; `overview_follows_window_exclusion_rules`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.**
- [ ] **Step 4:** `cargo test -p planora-app` (voll), clippy, fmt.
- [ ] **Step 5: Commit** `feat(app): Übersicht der Tastenkürzel mit Suche`.
- [ ] **Sichtprüfung (Controller):** Fenster, Suche, S.

### Task 8: Aufräumen und Dokumentation

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Abschnitt „Tastenkürzel“: was gilt, Liste „noch nicht vorhanden“ der Fusion-Kürzel, offene Punkte), `README.md` (Kurzabschnitt zu Tastenkürzeln und Mausbelegung), Plan-Abschnitt „Abweichungen“; tote feste Tastentexte und `Key::`-Abfragen entfernen, die jetzt über die Tabelle laufen (Esc, Enter, Entf der Skizze und Eingabefelder bleiben)
- Test: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check`

- [ ] **Step 1:** Suche nach festen Tastentexten („Strg+“ in Quelltext außerhalb der Tabelle und der Texte der Dokumentation) und ungenutzten Hilfsfunktionen; entfernen.
- [ ] **Step 2:** Dokumentation in einfacher Sprache; Liste der nicht belegten Fusion-Kürzel mit Grund („Funktion fehlt“).
- [ ] **Step 3:** Alle Prüfungen laufen lassen; Ergebnis im Bericht.
- [ ] **Step 4: Commit** `docs: Tastenkürzel`.

## Abweichungen

(Wird von Task 8 gefüllt.)
