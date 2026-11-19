# Skizze erstellen wie Fusion – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Ziel:** Ein Befehl „Skizze erstellen“ (Menü „Erzeugen“, erstes Symbol der unteren Werkzeugleiste, belegbare Taste) legt eine Skizze auf der vorgewählten Ebene/Fläche an oder startet einen Wahlmodus im Bild, in dem auch ausgeblendete Grundebenen wählbar sind.

**Architektur:** `PendingPick` bekommt die Variante `NewSketch`. Bildklick (`viewport.rs`), Baumklick (`browser.rs`), Hervorhebung, Esc/Rechtsklick und Statuszeile laufen schon über `pending_pick` und werden mitgenutzt. Nur das Ende in `pick_sketch_plane` verzweigt: bei `NewSketch` `add_sketch` und `open_sketch`. `origin_plane_rects` liefert während jedes Wahlmodus alle drei Grundebenen.

**Stack:** Rust, egui/eframe 0.36, `planora-app`, `planora-i18n`.

**Spec:** `docs/superpowers/specs/2026-11-19-skizze-erstellen-design.md`

## Feste Vorgaben

- Zweig `feature/skizze-erstellen` von master `ebccdd7e`, eigener Worktree. Im Hauptordner keinen Zweig wechseln.
- Texte nur über `tr!`, Schlüssel in `crates/planora-i18n/languages/de.ftl` und `en.ftl`. Name: de „Skizze erstellen“, en „Create Sketch“.
- Hinweis im Wahlmodus: der vorhandene Schlüssel `state-pick-sketch-plane` („Ebene oder ebene Fläche wählen“), kein neuer.
- Neue Taste: ab Werk ohne Belegung, in den Einstellungen belegbar.
- Kein neues Dateiformat. Die Sichtbarkeit (`Visibility`) wird im Wahlmodus nicht geschrieben.
- Ein Befehl = ein Rückgängig-Schritt. Ein Abbruch legt nichts an.
- Immer nur ein cargo-Lauf zur Zeit. Kommentare deutsch. Code-Fragen zuerst mit `graphify query`.
- Prüfen je Task: `cargo test -p planora-app <filter>`, `cargo clippy -p planora-app --all-targets -- -D warnings`, `cargo fmt -p planora-app -- --check`.

## Prüfschwerpunkte

1. **Wahlmodus und anderer Befehl:** Öffnet man im Wahlmodus ein Ebenen-Fenster oder einen anderen Befehl, endet der Modus ohne Rest (`workspace.rs` ruft schon `cancel_pending_pick`). Test in Task 1.
2. **Gesperrte Vorauswahl:** Ist der Körper der vorgewählten Fläche per Verlaufsleiste zurückgesetzt (`sketch_on_face_blocked` liefert einen Grund), entsteht keine Skizze, die Meldung erscheint und es startet kein Wahlmodus. Test in Task 1.
3. **Ausgeblendete Grundebene nach Abbruch:** Danach wird sie nicht mehr gezeichnet und nicht getroffen. Test in Task 2.
4. **Alte Version (`read_only`):** Symbol und Taste tun nichts, Menüeintrag gedämpft. Test in Task 3.
5. **Datei laden während des Modus:** `restore_document` löscht `pending_pick` schon. Keine neue Arbeit, nur in Task 1 mit einem Test absichern.

---

### Task 1: Befehl im Zustand (Vorauswahl, Wahlmodus, Abschluss)

**Dateien:**
- Ändern: `crates/planora-app/src/state.rs` (`PendingPick` ~L188, nach `start_redefine_sketch_plane` ~L3284, `pick_sketch_plane` ~L3316)
- Ändern: `crates/planora-app/src/browser.rs` ~L2066–2076 nur, falls der Baumklick auf Konstruktionsebenen die Variante prüft (heute `pending_pick.is_some()`, passt dann schon)
- Test: `mod tests` in `state.rs`

**Schnittstellen:**
- Erzeugt: `PendingPick::NewSketch` (ohne Daten).
- Erzeugt: `pub fn start_create_sketch(&mut self) -> Result<Option<FeatureId>, String>`. `Ok(Some(id))` heißt: auf der Vorauswahl angelegt und geöffnet. `Ok(None)` heißt: Wahlmodus läuft. `Err` heißt: Schließen der offenen Skizze oder Anlegen gescheitert, mit `self.message`.
- `pick_sketch_plane(click)` beendet bei `NewSketch` den Modus und legt die Skizze an (Grundebene → `add_sketch(Standard)`, Konstruktionsebene → `new_sketch_on_plane`, Fläche → `face_ref_for_sketch` + `add_sketch(OnFace)`, wie `new_sketch_on_face`) und öffnet sie dann mit `open_sketch`. Eine gekrümmte oder Netz-Fläche gibt wie heute nur eine Meldung, und der Modus bleibt.

Ablauf von `start_create_sketch`:
1. `try_close_sketch()?`, dann `plane_pick = None`, `edge_picking = false`, `clear_edge_marks()`, `message = None`.
2. Vorauswahl in dieser Reihenfolge wie das bisherige `sketch_menu`: `selected_origin_plane()`, `selected_plane()`, ebene Fläche in `selection`. Liefert `sketch_on_face_blocked()` einen Grund, wird `Err(Grund)` zurückgegeben, ohne Wahlmodus.
3. Ohne passende Vorauswahl: `selected_feature = None`, `selected_body = None`, `set_selection(None)`, `selected_origin = None`, `pending_pick = Some(NewSketch)`.

- [ ] **Schritt 1: Fehlschlagende Tests schreiben**

```rust
#[test] fn create_sketch_on_selected_origin_plane_opens_at_once()
// select_origin_plane(XZ); start_create_sketch() == Ok(Some(id)); pending_pick == None;
// open_sketch_id() == Some(id); Plane der Skizze == SketchPlane::Standard(XZ); genau ein neuer Verlaufsschritt.
#[test] fn create_sketch_on_selected_construction_plane()
// Konstruktionsebene anlegen (wie die bestehenden new_sketch_on_plane-Tests), wählen → Ok(Some), Skizze OnPlane(id).
#[test] fn create_sketch_on_selected_planar_face()
// Quader, eine ebene Fläche in selection → Ok(Some), Skizze OnFace.
#[test] fn create_sketch_without_selection_starts_pick()
// nichts gewählt → Ok(None), pending_pick == Some(NewSketch), Dokument unverändert (Featurezahl gleich).
#[test] fn create_sketch_with_edge_selected_clears_and_picks()
// Kante in selection → Ok(None), selection == None, pending_pick == Some(NewSketch).
#[test] fn create_sketch_pick_standard_plane_creates_and_opens()
// Wahlmodus, pick_sketch_plane(PlaneClick::Standard(YZ)) → pending_pick None, Skizze auf YZ offen.
#[test] fn create_sketch_pick_curved_face_keeps_mode()
// Zylinder, Mantelfläche klicken → message Some, pending_pick == Some(NewSketch), keine Skizze.
#[test] fn create_sketch_cancel_leaves_nothing()
// Wahlmodus, cancel_pending_pick() → Featurezahl, Rückgängig-Beschriftung, Visibility, selection, hover wie vorher.
#[test] fn create_sketch_closes_open_sketch_first()
// Skizze A offen → start_create_sketch() → A geschlossen; Abbruch → A bleibt geschlossen, keine neue Skizze.
#[test] fn create_sketch_blocked_face_reports_and_does_not_pick()
// Fläche eines per Verlaufsleiste zurückgesetzten Körpers gewählt → Err, message Some, pending_pick None.
#[test] fn create_sketch_mode_ends_on_restore()
// Wahlmodus, restore_document(...) → pending_pick None.
#[test] fn create_sketch_view_direction_like_fusion()
// Für XY, XZ, YZ: anlegen und öffnen; normierte Richtung (camera.eye() - Ziel) ≈ frame.z
// (XY: +Z, XZ: −Y, YZ: +X, Toleranz 1e-4); Bild-oben ≈ XY: +Y, XZ: +Z, YZ: +Z.
```

- [ ] **Schritt 2: Tests laufen lassen, sie schlagen fehl**

Befehl: `cargo test -p planora-app create_sketch`
Erwartet: FAIL, `start_create_sketch` und `NewSketch` fehlen.

- [ ] **Schritt 3: `PendingPick::NewSketch`, `start_create_sketch` und die Verzweigung in `pick_sketch_plane` bauen**

Der Doc-Kommentar von `PendingPick` nennt beide Befehle. `match` auf `pending_pick` im ganzen Crate anpassen. Ist `viewport.rs` ~L935 gut genug, braucht es dort nur den Kommentar.

- [ ] **Schritt 4: Tests laufen lassen, sie gehen durch**

Befehl: `cargo test -p planora-app create_sketch` und `cargo test -p planora-app redefine`
Erwartet: PASS, die alten Tests zu „Skizzenebene neu festlegen“ bleiben grün.

- [ ] **Schritt 5: Commit** `feat(app): Befehl Skizze erstellen im Zustand (Vorauswahl, Wahlmodus)`

### Task 2: Ausgeblendete Grundebenen im Wahlmodus

**Dateien:**
- Ändern: `crates/planora-app/src/state.rs` `origin_plane_rects` ~L2695 (und `nearest_origin_hit`, falls es nicht darauf aufbaut)
- Prüfen: `crates/planora-app/src/sketch_view.rs` `shown_planes` ~L1307 (nutzt `origin_plane_rects` und wirkt dann von selbst)
- Test: `state.rs`

**Schnittstellen:**
- Verbraucht: `PendingPick` aus Task 1.
- Erzeugt: `origin_plane_rects()` liefert alle drei Grundebenen, solange `pending_pick.is_some()` (beide Varianten). Sonst bleibt es wie heute.

- [ ] **Schritt 1: Fehlschlagende Tests schreiben**

```rust
#[test] fn hidden_origin_planes_show_while_picking()
// set_origin_plane_shown für alle drei auf false; origin_plane_rects().len() == 0;
// start_create_sketch() → len() == 3; Visibility.origin_planes unverändert [false; 3].
#[test] fn hidden_origin_plane_is_hit_while_picking()
// XY ausgeblendet, Kamera von oben, Zeiger Bildmitte: plane_click_at(..., PickFilter::BASES)
// == Some(PlaneClick::Standard(XY)) im Modus, None nach cancel_pending_pick().
#[test] fn redefine_sketch_plane_shows_hidden_origin_planes()
// wie oben mit start_redefine_sketch_plane: len() == 3.
```

- [ ] **Schritt 2: Laufen lassen, schlägt fehl** – `cargo test -p planora-app origin_plane` → FAIL
- [ ] **Schritt 3: Filter in `origin_plane_rects` um `|| self.pending_pick.is_some()` ergänzen**
- [ ] **Schritt 4: Laufen lassen, geht durch** – `cargo test -p planora-app origin_plane` und `cargo test -p planora-app plane` → PASS
- [ ] **Schritt 5: Commit** `feat(app): Grundebenen im Wahlmodus auch ausgeblendet wählbar`

### Task 3: Menü, Werkzeugleiste und Taste

**Dateien:**
- Ändern: `crates/planora-app/src/keymap/defaults.rs` (neue Zeile in `action_table!`, Bereich `Model`, `[]`)
- Ändern: `crates/planora-app/src/commands.rs` (`run_action_inner`: `Action::CreateSketch` → `start_create_sketch`)
- Ändern: `crates/planora-app/src/app.rs` `sketch_menu` ~L2922 (vier Einträge → einer)
- Ändern: `crates/planora-app/src/shell_panels.rs` `bottom_field` ~L1093 (erstes Symbol nach den drei Chips und dem Trennstrich, danach ein weiterer Trennstrich vor `CreateKind::ALL`)
- Ändern: `crates/planora-app/src/icons.rs` (`Icon::SketchOnFace` entfernen, falls danach ungenutzt; `Icon::SketchOnPlane` bleibt das Symbol)
- Ändern: `crates/planora-i18n/languages/de.ftl`, `en.ftl`
- Test: `commands.rs`/`shell_panels.rs`/`keymap` Tests

**Schnittstellen:**
- Verbraucht: `AppState::start_create_sketch` aus Task 1.
- Erzeugt: `Action::CreateSketch` mit fester Id `"modell.skizze-erstellen"`, Anzeigename `"action-create-sketch"`.
- Ftl neu: `action-create-sketch = Skizze erstellen` / `Create Sketch`, `menu-sketch-create = Skizze erstellen` / `Create Sketch`. Entfallen: `menu-sketch-xy`, `menu-sketch-xz`, `menu-sketch-yz`, `menu-sketch-on-face`.
- Menüeintrag und Symbol rufen `commands::run_action(ws, Action::CreateSketch)` (gleicher Weg wie die Taste). Tooltip: `keymap::with_hint(&ws.keymap, Action::CreateSketch, &label)`. Gesperrt mit `edit_lock`.

- [ ] **Schritt 1: Fehlschlagende Tests schreiben**

```rust
#[test] fn create_sketch_action_has_no_default_key()
// Keymap::defaults() hat für Action::CreateSketch keine Belegung; Action::ALL enthält sie; Bereich Model.
#[test] fn create_sketch_action_starts_pick()
// Workspace mit leerem Tab: run_action(ws, CreateSketch) → pending_pick == Some(NewSketch).
#[test] fn create_sketch_action_ignored_in_old_version()
// read_only-Tab: run_action → pending_pick None, keine Skizze.
#[test] fn bottom_bar_shows_create_sketch_first()
// Nach einem Bild: bottom_buttons(ctx)[3].0 == tr!("action-create-sketch") (nach den drei Chips).
```

Der Leistentest folgt den vorhandenen Tests zu `bottom_buttons`.

- [ ] **Schritt 2: Laufen lassen, schlägt fehl** – `cargo test -p planora-app create_sketch` → FAIL
- [ ] **Schritt 3: Aktion, Befehl, Menü, Leiste und Texte bauen; alte Schlüssel und das ungenutzte Symbol entfernen**
- [ ] **Schritt 4: Laufen lassen, geht durch** – `cargo test -p planora-app` (ganz, wegen der Tests zu ftl-Schlüsseln und Tastenbelegung) → PASS
- [ ] **Schritt 5: Commit** `feat(app): Skizze erstellen in Menü, Werkzeugleiste und Tastenbelegung`

### Task 4: Doku, Gesamtprüfung, Handtest

**Dateien:**
- Ändern: `docs/superpowers/offene-punkte.md` (Abschnitt „Skizze erstellen“: was gebaut ist, Annahmen aus der Spec-Klärung, Handtest-Ablauf)
- Ändern: Benutzer-Doku/README nur, wo die alten Menüeinträge genannt sind (`grep -rn "Skizze auf XY" docs README*`)
- Ändern: `CLAUDE.md` (Architektur-Zeile `planora-app`: `PendingPick::NewSketch`, `start_create_sketch`)

- [ ] **Schritt 1:** Doku anpassen, Commit `docs: Skizze erstellen`.
- [ ] **Schritt 2:** Gesamtprüfung Windows: `cargo fmt -p <crate> -- --check` je geänderte Crate, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`. Erwartet: grün, außer dem bekannten `real_step_text_limit`.
- [ ] **Schritt 3:** Gesamtprüfung WSL mit `~/wsl-s6-full.sh` (debug=0, ohne incremental), danach WSL aufräumen (`/root/target-s6`, `/root/PlanoraCAD/target` löschen, vorher `pgrep -af 'cargo|rustc|wsl-s6'`, danach `df -h /`).
- [ ] **Schritt 4:** Handtest nur nach Dennis' Go, bei 1920×1080 mit `tools/gui-drive.ps1`:
  1. Leeres Modell, Ursprung ausgeblendet, Symbol unten klicken: drei Ebenen halbdurchsichtig, Hervorhebung unter dem Zeiger.
  2. Esc: Ebenen weg, nichts im Baum, Rückgängig unverändert.
  3. XY, XZ, YZ je anlegen: Blick von oben, vorn, rechts.
  4. Quader, Fläche vorwählen, Symbol: Skizze sofort auf der Fläche.
  5. Im Wahlmodus Konstruktionsebene im Baum klicken: Skizze darauf.
  6. Werkzeugleiste ganz sichtbar.
- [ ] **Schritt 5:** Vault-Notiz je Task und Übergabe in `Projekte/PlanoraCAD/Sessions/`, Roadmap auf „fertig, wartet auf Merge“. Merge nur auf „bitte mergen“.
