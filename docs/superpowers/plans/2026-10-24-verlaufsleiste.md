# Verlaufsleiste und neuer Feature-Baum Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Der Verlauf der Schritte wandert aus dem Feature-Baum in eine Leiste am unteren Rand mit einem Marker je Körper; der Baum wird zu einer durchsichtigen Körper-Übersicht mit Körperfarben.

**Architecture:** `planora-feature` bekommt Rückrollstellen (eine Stelle setzt einen Schritt und alles zurück, was von ihm abhängt) neben dem unveränderten globalen Marker. Sie laufen durch Schnappschuss und Dateiformat. Die App bekommt eine Leiste (`history_bar.rs`), einen umgebauten Baum (`browser.rs`) und eine Einstellung für die Durchsichtigkeit.

**Tech Stack:** Rust, egui/eframe 0.36, vorhandene Crates `planora-feature`, `planora-project`, `planora-app`. Keine neuen Abhängigkeiten.

**Spec:** `docs/superpowers/specs/2026-10-24-verlaufsleiste-design.md` (Abschnitt „Festlegungen nach der Bauherren-Prüfung“ gilt mit).

## Global Constraints

- Oberflächentexte auf Deutsch, einfache Sprache, Satzanfang groß, keine Fahrzeugmarken, keine Hinweise auf Datenherkunft.
- Der Geometriekern `planora-kernel` bleibt unverändert. Der globale Marker (`Document::marker`, `set_marker`) bleibt mit unverändertem Verhalten.
- Dateien ohne Rückrollstellen bleiben Byte für Byte wie bisher (Format 10 wird weiter geschrieben); nur mit Rückrollstellen wird Format 11 geschrieben. Alte Dateien öffnen sich unverändert.
- Neue Aktionen an einem zurückgesetzten Körper sind gesperrt; Doppelklick (Parameter ändern), Löschen, Auge und Umbenennen bleiben erlaubt.
- Durchsichtigkeit der Panels: Voreinstellung 40 %, Bereich 0 bis 80 %, ganze Prozent.
- Tests berühren nie das echte `%APPDATA%` oder `Documents` (`ws.config_path` auf ein temporäres Verzeichnis).
- Agenten bauen keine Release-Builds und starten kein Fenster; Sichtprüfungen macht der Controller.
- Kein Autodesk-Material, keine neuen Abhängigkeiten (`cargo deny check` bleibt grün).
- Neue Fenster und Panels folgen den Fensterausschlussregeln (P10) und den Eingabesperren, wie die vorhandenen.

## Review Focus

- Rückrollstelle bei einem Schritt, auf dem ein Vereinigen eines zweiten Körpers aufbaut: Körper 2 bleibt bis zum Vereinigen aktiv, beide sind einzeln sichtbar (Task 1).
- Löschen eines Schritts, der Rückrollstelle ist oder hinter ihr liegt, hinterlässt keine verwaiste Stelle (Task 1).
- Speichern und Laden mit Rückrollstellen sowie alte Dateien (Task 2).
- Rückgängig/Wiederholen (Schnappschuss) bringt Rückrollstellen mit zurück (Task 2).
- Sehr lange Ketten: Leiste scrollt, der Marker bleibt erreichbar (Task 4).
- Heller Hintergrund bei 80 % Durchsichtigkeit: Schrift bleibt lesbar (Task 7).

---

### Task 1: Rückrollstellen im Kern

**Files:**
- Modify: `crates/planora-feature/src/model.rs` (Feld `rollbacks` in `Document`, Initialisierung)
- Modify: `crates/planora-feature/src/recompute.rs` (inaktive Schritte)
- Modify: `crates/planora-feature/src/ops.rs` (Löschen, `check_invariants`)
- Create: `crates/planora-feature/src/rollback.rs` (neue öffentliche Methoden), `mod rollback;` in `lib.rs`
- Test: `crates/planora-feature/tests/rollback.rs`

**Interfaces:**
- Consumes: `Document::dependents(FeatureId) -> Vec<FeatureId>`, `FeatureKind::dependencies()`, `Document::index_of`, `recompute`.
- Produces (alle `impl Document`):
  - `pub fn body_tips(&self) -> Vec<FeatureId>` – Schritte, die einen Körper erzeugen und von keinem späteren körpererzeugenden Schritt verbraucht werden (rein aus den Arten, nicht aus dem Status). Ordnung der Liste.
  - `pub fn chain(&self, tip: FeatureId) -> Vec<FeatureId>` – `tip` und alles, wovon er (auch über Umwege) abhängt, in Listenordnung; Skizzen eingeschlossen.
  - `pub fn rollbacks(&self) -> &[FeatureId]` – aktuelle Rückrollstellen, in Listenordnung.
  - `pub fn rollback_in_chain(&self, tip: FeatureId) -> Option<FeatureId>` – die Stelle, die in der Kette von `tip` liegt (höchstens eine).
  - `pub fn set_rollback(&mut self, tip: FeatureId, step: FeatureId) -> Result<(), FeatureError>` – `step` muss in `chain(tip)` liegen, sonst `InvalidInput`; ersetzt vorhandene Stellen dieser Kette; berechnet neu.
  - `pub fn clear_rollbacks(&mut self, tip: FeatureId) -> Result<(), FeatureError>` – entfernt die Stelle der Kette; `UnknownFeature` bei unbekanntem `tip`.
  - `pub fn clear_all_rollbacks(&mut self)`.
  - `pub fn is_active(&self, id: FeatureId) -> bool` – vor dem globalen Marker und nicht von einer Rückrollstelle erfasst (Schritt selbst oder abhängig).
- Innere Regel: Ein Schritt ist inaktiv, wenn sein Index ≥ `marker` ist oder er eine Rückrollstelle ist oder von einer abhängt (über `dependents`). Inaktive Schritte bekommen `FeatureStatus::NotComputed` und behalten ihr Ergebnis (wie hinter dem Marker); `visible_at` zeigt dadurch die Eingangskörper wieder.

- [ ] **Step 1: Tests schreiben** in `crates/planora-feature/tests/rollback.rs` (Hilfsfunktionen wie in den vorhandenen Tests des Crates benutzen):
  - `rollback_marks_step_and_dependents_not_computed`: Quader A, Verschieben(A), Verrunden; `set_rollback(tip, Verschieben)` → Verschieben und Verrunden `NotComputed`, Quader `Ok`, sichtbarer Körper ist der Quader; `check_invariants()` ok.
  - `moving_forward_restores_identical_geometry`: Volumen/Eckenzahl vor dem Rückrollen == nach `clear_rollbacks`; `report.reused` enthält die Ids (Ergebnisse aus dem Zwischenspeicher, nichts neu berechnet).
  - `rollback_before_a_boolean_shows_both_inputs_separately`: Quader A, Zylinder B, Vereinigen(A,B); Stelle bei A → Vereinigen `NotComputed`, B bleibt `Ok`, `visible_bodies()` enthält A und B einzeln. Zweite Stelle in der Kette von B ändert die von A nicht (Ketten sind unabhängig, soweit sie keinen Schritt teilen).
  - `set_rollback_replaces_the_previous_one_in_the_same_chain` und `set_rollback_rejects_a_step_outside_the_chain` (Fehler, nichts geändert).
  - `sketch_rollback_rolls_back_the_extrude`: Skizze + Extrudieren; Stelle bei der Skizze → Körper weg, Skizze bleibt in der Liste.
  - `global_marker_and_rollback_combine`: Marker vor einer Stelle → Stelle ändert nichts; danach `set_marker(len)` → Stelle wirkt.
  - `deleting_a_step_removes_orphan_rollbacks`: `delete_with_dependents(c)` entfernt `c` aus `rollbacks()`; Löschen eines Schritts hinter der Stelle lässt sie stehen.
  - `editing_a_rolled_back_step_recomputes_after_moving_forward`: Parameter eines zurückgesetzten Quaders ändern, dann `clear_rollbacks` → Ergebnis hat das neue Maß.
  - `body_tips_and_chain`: Werte der Beispielmodelle aus den Tests oben.
- [ ] **Step 2:** `cargo test -p planora-feature --test rollback` → schlägt fehl (Methoden fehlen).
- [ ] **Step 3: Implementieren** wie in Interfaces. `recompute_features`: statt `idx >= self.marker` die Prüfung `!self.is_active_idx(idx)` verwenden (Ergebnis bleibt, Status `NotComputed`). Alle Stellen, die `idx < self.marker` für „aktiv“ verwenden (`ops.rs` Invarianten, Einfügeposition am Marker in `add_*`, `snapshot.rs` Bau, `meshes.rs`/`step_bodies.rs` falls betroffen), auf die neue Regel prüfen und anpassen; globales Marker-Verhalten bleibt gleich. `check_invariants`: Features hinter dem Marker **oder** inaktiv sind `NotComputed`; Rückrollstellen verweisen auf vorhandene Ids, höchstens eine je Kette.
- [ ] **Step 4:** `cargo test -p planora-feature` komplett grün (auch bestehende Tests, besonders Marker- und Löschtests); `cargo clippy -p planora-feature --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `feat(feature): Rückrollstellen je Körper`.

### Task 2: Schnappschuss und Dateiformat 11

**Files:**
- Modify: `crates/planora-feature/src/snapshot.rs` (`Snapshot::rollbacks: Vec<FeatureId>`, `same_model`, `snapshot()`, Wiederherstellen)
- Modify: `crates/planora-project/src/format.rs` (`VersionFile::rueckrollstellen: Vec<u64>`, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`, Kopfkommentar „Format 1 bis 11“)
- Modify: `crates/planora-project/src/convert.rs` (`FORMAT_VERSION = 11`; Schreiben wählt Format 11 nur bei nicht leerer Liste, sonst wie bisher; Lesen prüft Ids und lehnt Stellen auf unbekannte Schritte ab)
- Test: `crates/planora-project/tests/rollbacks_format.rs`, Ergänzung in `crates/planora-feature/tests/rollback.rs`

**Interfaces:**
- Consumes: Task 1 (`rollbacks()`, `set_rollback`).
- Produces: `Snapshot { …, rollbacks: Vec<FeatureId> }`; Wiederherstellung (`Document::restore`) setzt die Stellen; Datei-Feld `rueckrollstellen` (Ids als `u64`).

- [ ] **Step 1: Tests:**
  - `snapshot_restore_brings_rollbacks_back` (Kern): Stelle setzen, `snapshot()`, `clear_all_rollbacks`, `restore(..)` → `rollbacks()` wie vorher, Zustände wie vorher; `same_model` ist falsch, wenn sich nur die Stellen unterscheiden.
  - `file_without_rollbacks_is_byte_identical_to_format_10`: dasselbe Dokument vor und nach der Änderung schreiben (Referenz-Bytes aus einer vorhandenen Testdatei oder dem Schreiben mit leerer Liste) → gleich; `format == 10`.
  - `file_with_rollbacks_roundtrips_as_format_11`: Schreiben, Lesen, `rollbacks()` gleich, Neuberechnung ergibt gleiche Zustände.
  - `format_11_with_unknown_step_id_is_rejected` (klare deutsche Fehlermeldung) und `older_program_limit`: `head.format > FORMAT_VERSION` bleibt für 12 abgelehnt.
  - Alle vorhandenen Format-Tests (alte Beispieldateien in den Testdaten) bleiben grün.
- [ ] **Step 2:** `cargo test -p planora-project -p planora-feature` → neue Tests schlagen fehl.
- [ ] **Step 3: Implementieren.** Die Muster der Formate 3 bis 10 (optionales Feld, leer = fehlt) nachbauen.
- [ ] **Step 4:** `cargo test --workspace --exclude planora-app` grün; `cargo clippy` ohne Warnungen.
- [ ] **Step 5: Commit** `feat(project): Rückrollstellen im Schnappschuss und Format 11`.

### Task 3: Leisten-Modell in der App (ohne Oberfläche)

**Files:**
- Create: `crates/planora-app/src/history_bar.rs` (Modul, Logik; Anzeigecode folgt in Task 4)
- Modify: `crates/planora-app/src/state.rs` (`pub selected_body: Option<FeatureId>`), `crates/planora-app/src/lib.rs`/`main.rs` (`mod history_bar;`)
- Modify: `crates/planora-app/src/browser.rs` (Aktionen `roll_back_to`/`roll_to_end` bleiben für den globalen Marker)
- Test: `crates/planora-app/tests/history_bar.rs`

**Interfaces:**
- Consumes: Kern aus Task 1.
- Produces in `history_bar.rs`:
  - `pub struct StepView { pub id: FeatureId, pub name: String, pub icon: FeatureIcon, pub state: StepState, pub shared: bool }` mit `pub enum StepState { Active, Inactive, Error(String) }`; `shared` = der Schritt liegt auch in der Kette eines anderen Körpers.
  - `pub fn steps(state: &AppState, tip: FeatureId) -> Vec<StepView>`.
  - `pub fn marker_position(state: &AppState, tip: FeatureId) -> usize` – Anzahl aktiver Schritte von vorn (Marker steht hinter dem letzten aktiven Schritt der Kette).
  - `pub fn move_marker(state: &mut AppState, tip: FeatureId, position: usize) -> Result<(), String>` – `position == steps.len()` hebt die Stelle auf, sonst `set_rollback` beim Schritt an dieser Position; beendet eine offene Skizze wie `roll_back_to`; ruft `state.refresh()`.
  - `pub fn step(state: &mut AppState, tip: FeatureId, forward: bool) -> Result<(), String>`; `pub fn to_start`, `pub fn to_end`.
  - `pub fn body_color(state: &AppState, tip: FeatureId) -> BodyColor` – Palette der Reihe nach, Index aus der kleinsten Id in `chain(tip)` modulo Palettenlänge (8 Einträge aus der Graphit-Palette in `theme.rs`, je Hell/Dunkel).
  - `pub fn is_locked(state: &AppState, tip: FeatureId) -> bool` – Körper hat eine Rückrollstelle (neue Aktionen gesperrt); `pub const LOCK_HINT: &str = "Dieser Körper ist zurückgesetzt. Springe zuerst zum Ende des Verlaufs.";`
  - `AppState::select_body(Option<FeatureId>)`; Standard bei neuem Dokument/Tab: der letzte Körper (`body_tips().last()`); verschwindet der gewählte Körper (gelöscht), wird neu gewählt.

- [ ] **Step 1: Tests:** `steps_lists_chain_in_order_with_states`, `move_marker_back_sets_rollback_and_forward_clears_it`, `move_marker_to_the_end_clears`, `step_forward_and_back_walks_one_step`, `shared_step_is_flagged_for_both_bodies` (Vereinigen), `body_color_is_stable_when_other_bodies_are_deleted`, `locked_body_reports_hint`, `selected_body_falls_back_when_body_is_deleted`, `move_marker_ends_an_open_sketch`.
- [ ] **Step 2:** Tests laufen lassen → Fehlschlag.
- [ ] **Step 3: Implementieren.** `FeatureIcon::of` für das Symbol; Fehlerstatus aus `FeatureStatus::Error`.
- [ ] **Step 4:** `cargo test -p planora-app --test history_bar` und `cargo clippy -p planora-app --all-targets -- -D warnings` grün.
- [ ] **Step 5: Commit** `feat(app): Modell der Verlaufsleiste`.

### Task 4: Leiste zeichnen und bedienen

**Files:**
- Modify: `crates/planora-app/src/history_bar.rs` (Zeichencode `show(ctx, state)`)
- Modify: `crates/planora-app/src/shell_panels.rs` und/oder `app.rs` (Panel über der Werkzeugleiste einhängen; Platz so, dass sich Werkzeugleiste und „n Körper“-Anzeige nicht überdecken)
- Modify: `crates/planora-app/src/icons.rs` (Symbole der Abspielknöpfe, falls fehlend), `crates/planora-app/src/theme.rs` (Farben Marker/abgeblendet)
- Test: `crates/planora-app/tests/history_bar_ui.rs` (egui-Kontext ohne Fenster, wie vorhandene UI-Tests)

**Interfaces:**
- Consumes: Task 3.
- Produces: `pub fn show(ctx: &egui::Context, state: &mut AppState)`; `pub fn bar_rects(ctx: &egui::Context) -> BarRects { pub bar, pub steps: Vec<(FeatureId, egui::Rect)>, pub marker: egui::Rect }` für Tests und für die Fensterausschlussregeln.

- [ ] **Step 1: Tests:** `bar_shows_one_icon_per_step_of_selected_body`; `dragging_the_marker_to_the_second_step_rolls_back`; `arrow_keys_move_marker_when_bar_has_focus`; `no_selected_body_shows_hint_and_disabled_buttons`; `inactive_steps_are_dimmed` (Zustand in `BarRects` oder abfragbarer Darstellungsdaten); `long_chain_scrolls_and_marker_stays_reachable` (60 Schritte, Leiste schmal: Marker-Rect liegt nach `scroll_to` im sichtbaren Bereich); `bar_does_not_overlap_toolbar`.
- [ ] **Step 2:** Tests → Fehlschlag.
- [ ] **Step 3: Implementieren** nach Entwurf 1 und 2 der Mockups: Abspielknöpfe (Anfang, zurück, Abspielen, vor, Ende), Symbole in Körperfarbe, Marker als senkrechte Linie mit Griff, Klick wählt Schritt und Zeile, Hinweis beim Darüberfahren (Name, wichtigstes Maß), Warnzeichen bei Fehler, „Gesamter Verlauf zurückgesetzt – Alles aktivieren“, wenn der globale Marker vor Schritten des Körpers steht. Abspielen: ein Schritt je 600 ms, stoppt am Ende; Kontext-Neuzeichnen nur solange es läuft.
- [ ] **Step 4:** Tests, `cargo clippy`; Fensterausschlussregeln (P10): offene Dialoge sperren die Leiste wie die Werkzeugleiste.
- [ ] **Step 5: Commit** `feat(app): Verlaufsleiste am unteren Rand`.
- [ ] **Sichtprüfung (Controller, echtes Fenster):** Leiste, Marker ziehen, Hell/Dunkel, schmales Fenster.

### Task 5: Doppelklick, Sperre neuer Aktionen

**Files:**
- Modify: `crates/planora-app/src/history_bar.rs` (Doppelklick)
- Modify: `crates/planora-app/src/feature_dialog.rs` (nur falls ein Weg für zurückgesetzte Schritte fehlt), `app.rs` (Zeile ~1079: Dialog öffnen)
- Modify: `crates/planora-app/src/app.rs`, `shell_panels.rs`, `state.rs` (Menüs „Erzeugen“, „Bearbeiten“, Skizzen- und Werkzeugleisten: gesperrt, wenn `is_locked(selected_body)` für die betroffene Eingabe; Hinweis `LOCK_HINT` als `on_disabled_hover_text`)
- Test: `crates/planora-app/tests/history_bar_actions.rs`

**Interfaces:**
- Consumes: Task 3, 4. Vorhandener `FeatureDialog::new(&AppState, FeatureId) -> Option<FeatureDialog>`, `apply`.
- Produces: `pub fn open_step(ws: &mut Workspace, id: FeatureId)` (öffnet Dialog, bei Skizze die Skizzenbearbeitung wie ein Doppelklick im Baum bisher).

- [ ] **Step 1: Tests:** `double_click_on_extrude_opens_dialog_and_new_height_recomputes_dependents` (Höhe ändern, späterer Schritt hat neues Ergebnis, Status Ok); `double_click_on_a_rolled_back_step_edits_it_without_moving_the_marker`; `double_click_on_sketch_opens_the_sketch`; `new_feature_on_rolled_back_body_is_refused_with_hint`; `delete_hide_and_rename_still_work_on_rolled_back_body`; `other_bodies_are_not_locked`.
- [ ] **Step 2:** Fehlschlag sehen.
- [ ] **Step 3: Implementieren.** Erzeugen-Menü und Werkzeuge, die einen Körper brauchen (Verschieben, Verrunden, Vereinigen …), prüfen den gewählten Körper bzw. die Eingangskörper; neue freistehende Körper (Quader, Zylinder …) sind nicht gesperrt.
- [ ] **Step 4:** Tests, clippy grün.
- [ ] **Step 5: Commit** `feat(app): Doppelklick auf Schritte und Sperre zurückgesetzter Körper`.
- [ ] **Sichtprüfung (Controller):** Extrudier-Höhe ändern, Spätere Schritte rechnen neu.

### Task 6: Neuer Feature-Baum

**Files:**
- Modify: `crates/planora-app/src/browser.rs` (Baum neu: Dokument, Ordner „Körper“, „Skizzen“, „Zeichnungen“, „Ursprung“; Auge je Zeile; farbiges Quadrat je Körper; `marker_line` und die Verlaufszeilen entfallen; Kontextmenü „Zurückrollen bis hier“ entfällt, Unterdrücken/Umbenennen/Löschen bleiben)
- Modify: `crates/planora-app/src/drawing_browser.rs` (falls Zeichnungen im neuen Ordner stehen), `app.rs` (Menüeintrag „Zum Ende der Zeitleiste“ bleibt für den globalen Marker)
- Test: bestehende Tests in `crates/planora-app/tests/` für den Baum anpassen; neue in `crates/planora-app/tests/feature_tree.rs`

**Interfaces:**
- Consumes: Task 3 (`body_color`, `selected_body`, `body_tips`).
- Produces: `browser::show`/`show_with` behalten ihre Signatur; `row_rects` bleibt für Tests.

- [ ] **Step 1: Tests:** `tree_lists_bodies_under_the_bodies_folder_with_colors` (Farbe = `body_color`); `clicking_a_body_selects_it_for_the_bar`; `eye_toggles_visibility_of_the_body`; `steps_are_not_listed_in_the_tree`; `sketches_and_drawings_have_their_own_folders`; `folders_collapse_and_expand_and_remember_state_per_tab`; `empty_document_shows_invitation_text`; `rolled_back_body_is_dimmed_but_listed`.
- [ ] **Step 2:** Fehlschlag.
- [ ] **Step 3: Implementieren.** Alte Tests, die Schrittzeilen im Baum erwarten, auf die Leiste umstellen oder streichen, wenn sie nur die alte Darstellung prüfen (im Bericht begründen).
- [ ] **Step 4:** `cargo test -p planora-app`, clippy grün.
- [ ] **Step 5: Commit** `feat(app): Feature-Baum als Körper-Übersicht`.
- [ ] **Sichtprüfung (Controller):** Farben im Baum und in der Leiste gleich.

### Task 7: Durchsichtigkeit und Einstellungen

**Files:**
- Modify: `crates/planora-app/src/settings_window.rs` (Abschnitt „Darstellung“, Regler „Durchsichtigkeit der Panels“ mit Anzeige in Prozent)
- Modify: `Config` (Konfigurationstyp im Workspace; `panel_transparency: u8`, Standard 40, beim Lesen auf 0 bis 80 begrenzt) und `workspace.rs` (Speichern über `Config::update_at` wie bei den Favoriten)
- Modify: `crates/planora-app/src/theme.rs` (`panel_frame_translucent(dark, percent)`), `browser.rs`/Panel-Aufruf in `shell_panels.rs` (Baum-Panel nutzt ihn; Schrift mit Schatten oder Mindestdeckkraft hinter den Zeilen)
- Test: `crates/planora-app/tests/transparency.rs`

**Interfaces:**
- Consumes: Task 6.
- Produces: `Workspace::set_panel_transparency(&mut self, percent: u8)` (begrenzt, speichert), `Workspace::panel_transparency(&self) -> u8`; `theme::panel_frame_translucent(dark: bool, percent: u8) -> egui::Frame`.

- [ ] **Step 1: Tests:** `default_is_forty_percent`; `value_is_clamped_to_zero_to_eighty_on_read_and_set`; `setting_is_saved_in_the_config_file_in_the_temp_dir` (nie das echte `%APPDATA%`); `frame_alpha_matches_percent` (Deckkraft = 255·(100−p)/100, gerundet); `zero_percent_is_opaque`; `contrast_of_text_over_light_background_at_80_percent_stays_readable` (berechnet Kontrast Schrift gegen Mischfarbe aus weißem Hintergrund und Panel, mindestens 4,5:1, notfalls Mindestdeckkraft hinter den Textzeilen).
- [ ] **Step 2:** Fehlschlag.
- [ ] **Step 3: Implementieren.** Einstellungsfenster zeigt „Darstellung“ über „Projekte“; Änderung wirkt sofort und wird gespeichert; ohne Schreibrecht bleibt der Wert für die Sitzung und das Fenster zeigt einen Hinweis wie beim Projektverzeichnis.
- [ ] **Step 4:** Tests, clippy grün.
- [ ] **Step 5: Commit** `feat(app): Durchsichtigkeit der Panels in den Einstellungen`.
- [ ] **Sichtprüfung (Controller):** 0, 40, 80 %, hell und dunkel.

### Task 8: Aufräumen und Dokumentation

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Abschnitt Verlaufsleiste mit Handliste und offenen Punkten), `README.md` (kurzer Abschnitt Leiste), Plan-Abschnitt „Abweichungen“ (unten ergänzen)
- Modify: tote Reste im Code entfernen (nicht mehr benutzte Marker-Zeichenfunktionen, Texte)
- Test: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo deny check`

- [ ] **Step 1:** Suche nach toten Resten (`marker_line`, `MARKER_TEXT`, ungenutzte Symbole), entfernen.
- [ ] **Step 2:** Dokumentation schreiben: Bedienung der Leiste, Sperre, Dateiformat 11, Durchsichtigkeit.
- [ ] **Step 3:** Alle Prüfungen laufen lassen; Ergebnis im Bericht.
- [ ] **Step 4: Commit** `docs: Verlaufsleiste`.

## Abweichungen

- **`history_body()` statt `selected_body`:** `AppState::selected_body` war schon der Index in der Körperliste (Eigenschaften, Export). Die Leiste nutzt deshalb `select_body(Option<FeatureId>)` und `history_body()`; die Texte der Aufgaben 4 bis 6 sprechen noch von `selected_body`.
- **Lineare Leiste nach dem Vereinigen:** Nach einem Vereinigen gibt es die Eingangskörper nicht mehr; die Leiste der Spitze ist eine einzige Reihe über die ganze Kette in Listenreihenfolge. Getrennte Leisten A und B gibt es nur, solange die Körper getrennt sind (Spec Festlegung 2 geändert).
- **`GLOBAL_HINT`-Ablehnung:** `move_marker` und Schrittbewegungen lehnen mit deutschem Hinweis auf „Alles aktivieren“ ab, wenn das Ziel hinter Schritten liegt, die nur wegen des globalen Markers inaktiv sind. Keine Rückrollstelle, kein Rückgängig-Eintrag.
- **Kontextmenü der Schritte:** Unterdrücken, Umbenennen und Löschen einzelner Schritte liegen im Kontextmenü der Leiste (über den Plan hinaus, vom Bauherrn bestätigt). Die Körperzeilen im Baum wirken auf den Ursprung des Körpers.
- **Zeilenreihenfolge nach Wurzel:** Die Körperzeilen sind nach dem ersten Schritt (Wurzel) der Kette sortiert, damit ein Verschieben sie nicht umsortiert (Befund aus dem Fenstertest).
- **Durchsichtigkeit über abgeleitete Hinterlegung und kräftigere Schrift:** Im durchsichtigen Baum haben ruhende Zeilen eine kräftigere Schriftfarbe, die Mindest-Hinterlegung wird aus der Schriftfarbe berechnet (Deckung bei 80 % etwa 0,68 dunkel, 0,66 hell). Der Plan sah feste Werte vor, die den Regler im Zeilenbereich aufhoben.
- **Format 11 nur mit Rückrollstellen:** Ohne Stellen bleibt die alte Nummer, Byte für Byte; ungültige Stellen werden beim Laden und Wiederherstellen mit deutscher Meldung abgelehnt.
- **`*_with_report`-Methoden:** `Document` hat zusätzlich Methoden mit Bericht (`set_rollback_with_report` und Verwandte), damit Tests prüfen können, was neu gerechnet und was wiederverwendet wurde.
- **Schriften-Test liest rekursiv:** Der Test zu den Schriften liest die Quelldateien jetzt rekursiv, weil `history_bar` ein Ordner mit `model.rs` und `view.rs` ist.
- **Aufgabe 8:** `browser::roll_back_to` bleibt (öffentliche Schnittstelle des globalen Markers, von Tests genutzt); `lock_reason` und `refuse_if_locked` sind `pub(crate)`; „Ein Schritt“-Knopftexte heißen jetzt „Einen Schritt zurück/vor“, am Anfang steht „Vor Schritt 1“ statt „Schritt 0 von n“.
