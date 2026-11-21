# Skizzen 12: Anzeige in der Skizze – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Skizzen werden angezeigt wie in Fusion: alle Anzeigeschalter im Skizzenfeld, Slice, Verdeckung der Skizzen durch Körper, fertige Skizzen in 3D anklickbar; dazu Winkelanzeige an Drehgriffen, Leistungspunkte aus 11b und fünf Härtungen aus Skizzen 5.

**Architecture:** Neue Anzeigeschalter je Skizze (`SketchDisplay`) liegen wie die Sichtbarkeit am Dokument (Format 23). Skizzenlinien, Profilflächen und Konstruktionsebenen baut die App je Bild als Dreiecke in Bildschirmlage mit Tiefe (`planora-render::stroke`) und zeichnet sie in einer neuen wgpu-Pipeline mit Tiefenprüfung; Symbole, Maße, Griffe und Felder bleiben egui-Overlay obenauf. Slice ist eine Schnittebene im Uniform der Körper-Shader.

**Tech Stack:** Rust, egui/eframe 0.36, wgpu (WGSL), eigene Crates `planora-render`, `planora-feature`, `planora-project`, `planora-app`, `planora-sketch`.

**Spec:** `docs/superpowers/specs/2026-11-18-skizzen-anzeige-design.md` (von Dennis freigegeben am 2026-11-18). Bei jeder Unklarheit gilt die Spec; mit *Annahme* markierte Punkte im Handtest gegen Fusion prüfen.

## Global Constraints

- Zweig `feature/skizzen-anzeige` von master `11b9c031`.
- Verhalten genau wie Autodesk Fusion; Abweichungen mit Begründung in `docs/superpowers/offene-punkte.md`, neuer Abschnitt „Teilprojekt Skizzen 12 (Anzeige in der Skizze)“.
- Texte nur über `tr!`/`msg!`, de und en; Namen wie in der deutschen/englischen Fusion (en: „Slice“, „Show Points“, „Show Dimensions“, „Show Constraints“, „Show Construction Geometries“, „Show Projected Geometries“, „Show Dimension“).
- Jede neue Schaltfläche als belegbarer Befehl in `keymap/`, **ohne** Vorgabetaste.
- Dateiformat: `FORMAT_VERSION` 22 → 23; alte Dateien bytegleich; Format 23 nur, wenn ein `SketchDisplay` von der Vorgabe abweicht. PCB-Zweige nehmen beim Einpflegen die nächste freie Nummer.
- Vorgabe `SketchDisplay`: `slice = false`, `dimensions_finished = false`, alle anderen `true`.
- Strichstärken unverändert: fertige Skizze 1,2 px, Konstruktion fertig 1,0 px, geöffnete Skizze wie heute; mal Pixelfaktor (`pixels_per_point`).
- Ein Befehl = ein Rückgängig-Schritt; Abbruch hinterlässt nichts. Kommentare deutsch. Kein Autodesk-Material.
- Immer nur ein cargo-Lauf zur Zeit. Windows: `cargo fmt -p <crate> -- --check` je Crate; clippy `-D warnings`.
- Handtest nur nach Dennis' ausdrücklichem Go, jedes Mal neu; 1920×1080, `tools/gui-drive.ps1` (sichtbares Fenster hat Klasse „Window Class“, Bildschirm 5120 px breit). Nach jedem WSL-Lauf `/root/target-s6` und `/root/PlanoraCAD/target` löschen, vorher `pgrep -af 'cargo|rustc|wsl-s6'`.
- Nach jedem Task eine Vault-Notiz `D:\Vault\Vault\Projekte\PlanoraCAD\2026-11-XX Skizzen 12 Task N <Thema>.md`, verlinkt in `PlanoraCAD.md`.

## Review Focus

1. **Skizze auf einer Körperfläche bei flachem Blickwinkel** – die Linie darf nicht flackern oder in der Fläche versinken (Tiefenvorsprung). Test in Task 3 (`stroke_bias_keeps_coplanar_line_in_front`) und Bildprüfung im Handtest 1.
2. **Kamera hinter der Skizzenebene mit Slice** – die weggeschnittene Seite wechselt mit; nie verschwindet alles. Test in Task 6 (`slice_side_follows_camera`).
3. **Körperflächen genau in der Skizzenebene mit Slice** (Skizze auf einer Fläche) – die Fläche bleibt sichtbar (Toleranz). Test in Task 6 (`slice_keeps_faces_in_the_plane`).
4. **Ausgeblendete Skizze oder Skizze in ausgeblendeter Gruppe** – nicht gezeichnet, nicht anklickbar. Test in Task 8 (`hidden_sketch_is_not_pickable`).
5. **Gelöschte Skizze mit eigenem `SketchDisplay`, dann Rückgängig** – der Eintrag fällt beim Löschen weg und kommt mit Rückgängig zurück; Speichern danach bleibt gültig. Test in Task 1 (`display_entry_follows_sketch_delete_and_undo`).

---

## Etappe 1: Verdeckung und Slice

### Task 1: `SketchDisplay` am Dokument

**Files:**
- Create: `crates/planora-feature/src/sketch_display.rs`
- Modify: `crates/planora-feature/src/lib.rs`, `model.rs` (Feld in `Document`), `snapshot.rs` (Feld in `Snapshot`, Wiederherstellen), `crates/planora-app/src/undo_labels.rs`, `crates/planora-i18n/languages/{de,en}.ftl`
- Test: `crates/planora-feature/tests/sketch_display.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct SketchDisplay { pub slice: bool, pub profiles: bool, pub points: bool, pub dimensions: bool, pub constraints: bool, pub construction: bool, pub projected: bool, pub dimensions_finished: bool }` mit `impl Default` (Werte aus Global Constraints) und `fn is_default(&self) -> bool`.
  - `Document::sketch_display(&self, id: FeatureId) -> SketchDisplay` (Vorgabe, wenn kein Eintrag).
  - `Document::set_sketch_display(&mut self, id: FeatureId, d: SketchDisplay)` – nur für Skizzen-Features; Vorgabe entfernt den Eintrag; Änderung zählt `drawing_edits += 1` (wie `set_visibility`), keine Neuberechnung.
  - `Snapshot::sketch_display: BTreeMap<FeatureId, SketchDisplay>` (nur Abweichungen); beim Wiederherstellen und beim Löschen einer Skizze werden Einträge zu nicht vorhandenen Skizzen entfernt (wie `pruned_visibility`).
  - Rückgängig-Name `undo-sketch-display` („Skizzenanzeige ändern“ / „Change Sketch Display“) in `undo_labels.rs`, Vergleich nach `visibility`.

- [ ] **Step 1: Failing tests** in `tests/sketch_display.rs`: `default_display_values` (Vorgabe wie oben, `is_default`), `set_display_counts_as_view_edit` (`edit_count` steigt, `model_edit_count` nicht), `setting_default_removes_entry` (Snapshot-Map leer), `display_on_non_sketch_is_ignored`, `display_entry_follows_sketch_delete_and_undo`.
- [ ] **Step 2:** `cargo test -p planora-feature --test sketch_display` → FAIL (Typ fehlt).
- [ ] **Step 3:** Implementieren wie in Interfaces; Muster `visibility.rs`.
- [ ] **Step 4:** Test → PASS; `cargo test -p planora-feature` grün.
- [ ] **Step 5:** Commit `feat(feature): Anzeigeschalter je Skizze (SketchDisplay)`.

### Task 2: Format 23

**Files:**
- Modify: `crates/planora-project/src/format.rs`, `convert.rs` (`FORMAT_VERSION`, `format_with`, Schreiben bei `sichtbarkeit` ~4100, Lesen ~4458, Altersprüfung ~4446), `crates/planora-i18n/languages/{de,en}.ftl` falls neue Ladefehler
- Test: `crates/planora-project/tests/format_23.rs`

**Interfaces:**
- Consumes: `SketchDisplay`, `Snapshot::sketch_display` (Task 1).
- Produces: Dateischlüssel `skizzenanzeige: Option<Vec<SkizzenanzeigeDto>>` mit `SkizzenanzeigeDto { skizze: u64, slice, profil, punkte, masse, abhaengigkeiten, konstruktion, projiziert, masse_fertig }`, jedes Feld `Option<bool>` und nur geschrieben, wenn es von der Vorgabe abweicht; Liste aufsteigend nach `skizze`; ganz weggelassen, wenn leer.

- [ ] **Step 1: Failing tests:** `old_files_stay_byte_identical` (vorhandene Goldendateien Format ≤ 22 laden und schreiben, gleiche Bytes), `display_needs_format_23` (`required_format == 23` bei einer Abweichung, 22/kleiner sonst), `display_roundtrip` (alle Felder einzeln umgeschaltet), `display_in_old_format_is_corrupt` (Schlüssel bei `format < 23` → `ProjectError` beschädigt, Meldung `proj-since-format`), `display_for_unknown_sketch_is_corrupt`, `duplicate_display_entry_is_corrupt`, `default_entry_in_file_is_corrupt` (Eintrag ohne abweichendes Feld).
- [ ] **Step 2:** `cargo test -p planora-project --test format_23` → FAIL.
- [ ] **Step 3:** Implementieren; `format_with` prüft `!s.sketch_display.is_empty()` vor der 22-Regel.
- [ ] **Step 4:** Test → PASS; `cargo test -p planora-project` grün (bekannt rot: `real_step_text_limit`).
- [ ] **Step 5:** Commit `feat(project): Format 23 mit Skizzenanzeige`.

### Task 3: Linien-Pipeline mit Tiefe (`planora-render`)

**Files:**
- Create: `crates/planora-render/src/stroke.rs`, `crates/planora-render/src/stroke.wgsl`
- Modify: `crates/planora-render/src/lib.rs`, `gpu.rs` (neue Pipeline, Puffer je Bild wie `grid_vertex_buffer`, Feld in `ViewportCallback`)
- Test: `crates/planora-render/tests/stroke.rs`

**Interfaces:**
- Produces:
  - `#[repr(C)] #[derive(Clone, Copy, Pod, Zeroable)] pub struct StrokeVertex { pub ndc: [f32; 3], pub color: [f32; 4], pub edge: f32 }` – `ndc` fertig projiziert (x, y in −1…1, z Tiefe 0…1 wie wgpu), `edge` −1…1 quer zur Linie (0 = Mitte, Flächen 0) für die Kantenglättung im Fragment-Shader.
  - `pub struct StrokeView { pub view_proj: Mat4, pub size_px: Vec2, pub depth_bias: f32 }`
  - `pub fn push_stroke(out: &mut Vec<StrokeVertex>, view: &StrokeView, path: &[DVec3], width_px: f32, color: [f32; 4], dash_px: Option<[f32; 2]>)` – Polylinie → Dreiecke in Bildschirmbreite (je Strecke ein Viereck, Ecken ohne Lücke), Strichelung in Pixeln über die ganze Polylinie (Muster läuft über Stützstellen weiter), Punkte hinter der Kamera werden abgeschnitten (Strecke an `w = ε` kürzen), `depth_bias` von z abgezogen.
  - `pub fn push_fill(out: &mut Vec<StrokeVertex>, view: &StrokeView, triangles: &[[DVec3; 3]], color: [f32; 4])`.
  - `pub const STROKE_DEPTH_BIAS: f32` – Wert so wählen, dass `stroke_bias_keeps_coplanar_line_in_front` hält (Startwert 2e-5, an `near`/`far` der Kamera prüfen).
  - `ViewportCallback::strokes: Vec<StrokeVertex>`; Pipeline: Dreiecksliste, Tiefe `LessEqual`, kein Tiefenschreiben, Alpha-Mischung, gezeichnet nach Flächen/Kanten/Leinwänden.

- [ ] **Step 1: Failing tests:** `stroke_width_in_pixels` (waagerechte Linie 100 px lang, Breite 2 px → Viereck 2 px hoch in Bildschirmlage), `dash_continues_across_vertices` (Polylinie aus vier 5-px-Stücken, Muster [6, 4] → Strichlängen 6, 6 …), `stroke_clipped_behind_camera` (ein Ende hinter der Kamera → keine NaN, Strecke gekürzt), `stroke_bias_keeps_coplanar_line_in_front` (Linie auf einer Ebene, Blickwinkel 85° zur Normalen: Linien-z < Flächen-z an derselben Pixelstelle), `fill_triangles_projected`.
- [ ] **Step 2:** `cargo test -p planora-render --test stroke` → FAIL.
- [ ] **Step 3:** `stroke.rs` (rein, ohne GPU) und Pipeline in `gpu.rs`; Shader: `fs` setzt Alpha `color.a * (1 - smoothstep(1 - 1/width, 1, abs(edge)))`.
- [ ] **Step 4:** Test → PASS; `cargo clippy -p planora-render -- -D warnings`.
- [ ] **Step 5:** Commit `feat(render): Skizzenlinien mit Tiefenprüfung`.

### Task 4: Fertige Skizzen, Profile in Dialogen und Konstruktionsebenen auf die GPU

**Files:**
- Modify: `crates/planora-app/src/sketch_view.rs` (`finished_sketch_shapes` ~1258, `plane_shapes` ~1014, `profile_fill_shape` ~978), `crates/planora-app/src/viewport.rs` (~1000–1040: `strokes` in `ViewportCallback`)
- Test: `crates/planora-app/tests/finished_sketch_strokes.rs`

**Interfaces:**
- Consumes: `push_stroke`, `push_fill`, `StrokeView`, `StrokeVertex` (Task 3).
- Produces:
  - `pub fn finished_sketch_strokes(state: &AppState, view: &StrokeView, out: &mut Vec<StrokeVertex>)` – gleiche Auswahl, Farben und Breiten wie heute `finished_sketch_shapes` (Konstruktion gestrichelt, Projektion lila, verlorene gelb, Dialog-Markierung, Profile bei `profile_pick`).
  - `pub fn plane_strokes(state: &AppState, view: &StrokeView, out: &mut Vec<StrokeVertex>)` – Fläche halbdurchsichtig plus Rand, Farben wie `plane_shapes`.
  - `finished_sketch_shapes` und `plane_shapes` liefern danach nur noch, was obenauf bleibt (z. B. Ebenennamen, falls vorhanden); die egui-Linien entfallen.
  - `pub fn stroke_view(state: &AppState, rect: Rect, ppp: f32) -> StrokeView` in `viewport.rs`.

- [ ] **Step 1: Failing tests:** `finished_sketch_emits_strokes` (eine fertige Skizze mit Linie und Konstruktionslinie → Vertices mit beiden Farben, Konstruktion gestrichelt), `hidden_sketch_emits_nothing`, `open_sketch_not_in_finished_strokes`, `profile_pick_adds_fills`, `planes_emit_fill_and_outline`.
- [ ] **Step 2:** `cargo test -p planora-app --test finished_sketch_strokes` → FAIL.
- [ ] **Step 3:** Implementieren; Farben von `Color32` nach sRGB-Float wie `viewport.rs` `rgba`.
- [ ] **Step 4:** Test → PASS; vorhandene Tests zu `finished_sketch_shapes` anpassen (sie prüfen jetzt die Strokes).
- [ ] **Step 5:** Commit `feat(app): fertige Skizzen und Ebenen mit Verdeckung`.

### Task 5: Geöffnete Skizze auf die GPU, Messtest

**Files:**
- Modify: `crates/planora-app/src/sketch_view.rs` (`draw` ~3065: Kurven, Vorschau `draw_preview`/`draw_preview3`/`draw_text_preview`, Profilfläche der offenen Skizze), `viewport.rs`
- Create: `crates/planora-app/tests/open_sketch_strokes.rs`, `crates/planora-app/benches/` oder Messtest im vorhandenen Messtest-Muster (`tests/*_timing.rs`, `#[ignore]`, Release) `sketch_draw_timing.rs`

**Interfaces:**
- Consumes: Task 3, `stroke_view` (Task 4).
- Produces: `pub fn open_sketch_strokes(state: &AppState, view: &StrokeView, out: &mut Vec<StrokeVertex>)` – Kurven der geöffneten Skizze mit den Farben von `curve_ink`/`lost_ink` (Auswahl, Hover, bestimmt/unbestimmt, Konstruktion, Mittellinie, Projektion), Vorschau des laufenden Werkzeugs, Profilflächen bei `SketchDisplay::profiles`. `draw` zeichnet danach nur noch Punkte, Symbole, Maße, Griffe, Fang- und Zeigermarken, Rahmenauswahl, Krümmungskamm, Felder (Spec 2: Punkte bleiben vorerst Overlay; nach Handtest-Vergleich ggf. Folgeänderung).

- [ ] **Step 1: Failing tests:** `open_sketch_curves_become_strokes`, `selected_curve_uses_selection_color`, `preview_line_is_stroked`, `profiles_follow_display_switch` (mit `SketchDisplay::profiles = false` keine Flächen).
- [ ] **Step 2:** `cargo test -p planora-app --test open_sketch_strokes` → FAIL.
- [ ] **Step 3:** Implementieren; `AppState::show_sketch_profiles` entfällt zugunsten `SketchDisplay::profiles` (Haken im Feld schreibt über `set_sketch_display`).
- [ ] **Step 4:** Tests → PASS; ganze App-Tests grün (`cargo test -p planora-app`).
- [ ] **Step 5: Messtest** `sketch_draw_timing` (Release, `--ignored`): Skizze mit 2000 Kurven, je Bild Strokes + Overlay bauen; Umlauf um ein Modell mit 20 fertigen Skizzen. Median aus 5 Läufen auf master (Commit 11b9c031, heutige Overlay-Funktionen) und auf dem Zweig; Tor: Zweig ≤ 110 % von master. Ergebnis in `offene-punkte.md`.
- [ ] **Step 6:** Commit `feat(app): offene Skizze mit Verdeckung`.

### Task 6: Slice

**Files:**
- Modify: `crates/planora-render/src/shader.wgsl` (Uniform `slice: vec4<f32>`, `slice_on: f32`; Welt-Position an `fs_face`/`fs_line` durchreichen, `discard` auf der Kameraseite), `gpu.rs` (`Uniforms`, `ViewportCallback::slice: Option<[f32; 4]>`), `crates/planora-render/src/stroke.rs` (reine Hilfe), `crates/planora-app/src/viewport.rs`, `sketch_panel.rs` (Haken „Slice“), `state.rs` (`pick_with`, `face_point_under`: Treffer vor der Ebene überspringen), `keymap/` (Befehl), `{de,en}.ftl`
- Test: `crates/planora-render/tests/slice.rs`, `crates/planora-app/tests/sketch_slice.rs`

**Interfaces:**
- Produces:
  - `pub fn slice_plane(origin: DVec3, normal: DVec3, eye: DVec3) -> [f32; 4]` in `planora-render` – Ebene `n·x + d = 0`, Vorzeichen so, dass die Kameraseite positiv ist; der Shader verwirft `n·p + d > SLICE_EPS`.
  - `pub const SLICE_EPS: f32` (Startwert 1e-4 mm·Skalierung; Test `slice_keeps_faces_in_the_plane` legt ihn fest).
  - `pub fn slice_cuts(plane: [f32; 4], p: DVec3) -> bool` – dieselbe Regel auf der CPU (für das Anklicken).
  - `AppState::active_slice(&self) -> Option<[f32; 4]>` – nur bei geöffneter Skizze mit `SketchDisplay::slice`; Ebene aus `frame` der Skizze (auch 3D-Skizze).
  - Haken „Slice“ im Skizzenfeld (`sketch-palette-slice`, Hinweis `sketch-palette-slice-hint`), Befehl `SketchSlice` in der Keymap ohne Taste.

- [ ] **Step 1: Failing tests:** `slice_side_follows_camera` (Kamera vor/hinter der Ebene → `slice_cuts` für denselben Punkt wechselt), `slice_keeps_faces_in_the_plane` (Punkt mit Abstand < `SLICE_EPS` wird nicht geschnitten), `slice_only_while_sketch_open` (`active_slice` None nach `finish_sketch`, Haken bleibt gespeichert), `pick_skips_sliced_faces` (Quader um die Skizzenebene: Klick trifft die Fläche hinter der Ebene, nicht die vordere), `slice_toggle_is_one_undo_step`.
- [ ] **Step 2:** Tests → FAIL.
- [ ] **Step 3:** Implementieren. Strokes und Leinwände bekommen keine Schnittebene (Spec 3).
- [ ] **Step 4:** Tests → PASS; `cargo test -p planora-render -p planora-app`.
- [ ] **Step 5:** Commit `feat: Slice an der Skizzenebene`.

### Etappe-1-Abschluss

- [ ] Doku-Stand in `offene-punkte.md` (Annahmen aus Task 3–6).
- [ ] Windows: fmt je Crate, clippy, alle Tests. WSL: `~/wsl-s6-full.sh`; danach WSL aufräumen.
- [ ] **Dennis um Go für Handtest 1 fragen.** Prüfliste: Skizze auf Körperfläche (flach drehen), Skizze auf Mittelebene verdeckt, Slice an/aus, Kamera hinter die Ebene, fertige Skizzen hinter Körpern, Konstruktionsebenen verdeckt, Ziehen/Griffe wie vorher, Bildfluss bei großer Skizze. Vergleich mit Fusion: Schnittflächen gefüllt? Punkte verdeckt?
- [ ] Funde beheben, Vault-Notiz „Skizzen 12 Handtest Etappe 1“.

## Etappe 2: Schalter und Anklicken

### Task 7: Anzeigeschalter im Skizzenfeld

**Files:**
- Modify: `crates/planora-app/src/sketch_panel.rs` (`side_contents` ~1495), `sketch_view.rs` (`draw`, `open_sketch_strokes`, `select_hit`, `pick_3d`), `sketch_state.rs` (Wahl), `crates/planora-sketch/src/snap.rs` oder der Fang-Aufrufer in der App (Filter), `keymap/`, `{de,en}.ftl`
- Test: `crates/planora-app/tests/sketch_display_switches.rs`

**Interfaces:**
- Consumes: `SketchDisplay` (Task 1), `open_sketch_strokes` (Task 5), `finished_sketch_strokes` (Task 4).
- Produces: Haken in Fusions Reihenfolge (Slice, Profil anzeigen, Punkte anzeigen, Maße anzeigen, Abhängigkeiten anzeigen, Konstruktionsgeometrie anzeigen, Projizierte Geometrie anzeigen); Schlüssel `sketch-show-points`, `sketch-show-dimensions`, `sketch-show-constraints`, `sketch-show-construction`, `sketch-show-projected` (+ `-hint`); Befehle `SketchShowPoints` … `SketchShowProjected` ohne Taste. Ein Filter `pub fn hidden_by_display(d: &SketchDisplay, sk: &Sketch, projected: &HashSet<CurveId>, item: Item) -> bool` in `sketch_view.rs`, den Zeichnen, Wählen, Rahmenauswahl und Fang gemeinsam nutzen.

- [ ] **Step 1: Failing tests** (je Schalter aus): `points_off_hides_and_unpicks_points_but_keeps_endpoint_snap`, `dimensions_off_hides_dimensions_except_new_one`, `constraints_off_hides_glyphs_keeps_auto_marks`, `construction_off_hides_unpicks_unsnaps`, `projected_off_hides_but_keeps_profiles`, `finished_sketch_follows_construction_and_projected`, `switch_does_not_end_sketch`.
- [ ] **Step 2:** Tests → FAIL.
- [ ] **Step 3:** Implementieren (Spec 1, Wirkung je Schalter).
- [ ] **Step 4:** Tests → PASS.
- [ ] **Step 5:** Commit `feat(app): Anzeigeschalter im Skizzenfeld`.

### Task 8: Fertige Skizzen in 3D anklicken

**Files:**
- Modify: `crates/planora-render/src/scene.rs` (`Pick`), `crates/planora-app/src/state.rs` (`pick_with` ~2408, `PickFilter`, Hover/Selection, `sync_visibility`), `sketch_view.rs` (`sketch_curve_under` ~2793 verallgemeinern), `viewport.rs` (Rechtsklickmenü), `browser.rs` (gleiche Menüaktionen), `{de,en}.ftl`
- Test: `crates/planora-app/tests/finished_sketch_pick.rs`

**Interfaces:**
- Produces:
  - `Pick::SketchCurve { sketch: FeatureId, curve: CurveId }`, `Pick::SketchPoint { sketch: FeatureId, point: PointId }`; `PickFilter::sketches: bool` (in `SELECT` an).
  - `pub fn finished_sketch_under(state: &AppState, map: &Mapper, p: Pos2, slice: Option<[f32; 4]>) -> Option<Pick>` – Punkte vor Kurven, nächste gewinnt, nur sichtbare fertige Skizzen (Auge, Ordner, Gruppe), Stelle nicht verdeckt.
  - `AppState::body_hit_distance(&self, origin: DVec3, dir: DVec3, slice: Option<[f32; 4]>) -> Option<f64>` – erster Körpertreffer (über `planora_render::pick_face` je Netz); verdeckt, wenn Treffer < Abstand zur Skizzenstelle − Toleranz.
  - Reihenfolge in `pick_with`: Ecke/Kante → Skizzenpunkt → Skizzenkurve → Fläche → Ebene.
  - Hervorhebung: `finished_sketch_strokes` färbt Hover/Auswahl.
  - Rechtsklick auf Skizzengeometrie: „Skizze bearbeiten“, „Maße anzeigen“ (Haken, Task 9), „Ausblenden“. Entf: keine Wirkung. Doppelklick: keine Wirkung.

- [ ] **Step 1: Failing tests:** `click_selects_finished_sketch_curve`, `point_wins_over_curve`, `occluded_curve_is_not_pickable` (Linie hinter Quader), `visible_part_of_partly_covered_curve_is_pickable`, `hidden_sketch_is_not_pickable` (Auge und ausgeblendete Gruppe), `body_edge_wins_over_sketch_curve`, `context_menu_edit_sketch_opens_it`, `delete_on_sketch_selection_does_nothing`.
- [ ] **Step 2:** Tests → FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** Tests → PASS.
- [ ] **Step 5:** Commit `feat(app): fertige Skizzen in 3D anklickbar`.

### Task 9: „Maße anzeigen“ an fertigen Skizzen

**Files:**
- Modify: `crates/planora-app/src/browser.rs` / `sketch_tree.rs` (Menüpunkt), `sketch_view.rs` (Maße fertiger Skizzen im Overlay, nur Anzeige), `viewport.rs`, `{de,en}.ftl`
- Test: `crates/planora-app/tests/finished_sketch_dimensions.rs`

**Interfaces:**
- Consumes: `SketchDisplay::dimensions_finished` (Task 1), Menü aus Task 8.
- Produces: `pub fn finished_dimension_shapes(state: &AppState, rect: Rect) -> Vec<Shape>` – Maße (Text und Hilfslinien wie in der offenen Skizze, `dimension_text`, `dimension_helpers`) jeder sichtbaren fertigen Skizze mit `dimensions_finished`; obenauf, nicht anklickbar. Menüpunkt `tree-sketch-show-dimension` („Maße anzeigen“ / „Show Dimension“) als Haken, nicht bei geöffneter Skizze.

- [ ] **Step 1: Failing tests:** `show_dimension_draws_finished_dimensions`, `menu_entry_absent_while_sketch_open`, `toggle_is_undo_step_with_display_label`, `saved_and_loaded_with_format_23`.
- [ ] **Step 2:** → FAIL. **Step 3:** implementieren. **Step 4:** → PASS.
- [ ] **Step 5:** Commit `feat(app): Maße fertiger Skizzen anzeigen`.

### Task 10: Skizzengeometrie in den Ebenen-Dialogen

**Files:**
- Modify: `crates/planora-app/src/plane_dialog.rs` (Klickannahme ~500–540, `uses_later_geometry` ~545), ggf. weitere Dialoge mit `AxisRef`/`PointRef` (vorher mit `graphify query "AxisRef PointRef Dialog"` und Grep suchen und im Task-Bericht auflisten)
- Test: `crates/planora-app/tests/plane_dialog_sketch_refs.rs`

**Interfaces:**
- Consumes: `Pick::SketchCurve`, `Pick::SketchPoint` (Task 8).
- Produces: `(SlotKind::Axis, PlaneClick::View(Pick::SketchCurve{..}))` → `SlotValue::Axis(AxisRef::SketchLine{..})` nur für gerade Linien (sonst die vorhandene „falsche Wahl“-Meldung); `(SlotKind::Point, Pick::SketchPoint{..})` → `PointRef::SketchPoint`; `uses_later_geometry` prüft die Skizzen-Feature-Kennung.

- [ ] **Step 1: Failing tests:** `angle_plane_takes_sketch_line_as_axis`, `arc_is_refused_as_axis`, `three_point_plane_takes_sketch_points`, `editing_refuses_later_sketch`.
- [ ] **Step 2:** → FAIL. **Step 3:** implementieren. **Step 4:** → PASS.
- [ ] **Step 5:** Commit `feat(app): Skizzenlinien und -punkte in Ebenen-Dialogen`.

### Etappe-2-Abschluss

- [ ] Doku in `offene-punkte.md`; Windows und WSL voll; WSL aufräumen.
- [ ] **Dennis um Go für Handtest 2 fragen.** Prüfliste: jeder Haken an/aus in offener Skizze, fertige Skizze Konstruktion/Projektion, Klick auf fertige Skizze (verdeckt/sichtbar), Rechtsklickmenü, „Maße anzeigen“ im Baum, Ebene unter Winkel mit Skizzenlinie. Vergleich mit Fusion: Entf auf Skizzenkurve außerhalb der Skizze, Doppelklick, Punkte/Maße bei „aus“.
- [ ] Funde beheben, Vault-Notiz.

## Etappe 3: Reste

### Task 11: Winkelanzeige an Drehgriffen

**Files:**
- Modify: `crates/planora-app/src/sketch_handles.rs` (Text-Drehgriff, `text_rotate_grips` ~406), `sketch_grips.rs` (`Grip::Rotate`, `RotateX`, `RotateY` ~194–230), `sketch_view.rs` (Zeichnen), `sketch_state.rs` (`set_tool` beim Zug)
- Test: `crates/planora-app/tests/rotate_grip_angle.rs`

**Interfaces:**
- Produces: `pub fn rotate_angle_label(at: Pos2, angle_rad: f64) -> (Pos2, String)` (Text über `format_angle`, Lage neben dem Zeiger); gezeichnet während jedes Drehzugs (Text-Drehgriff, Drehring und Drehbögen der Triade, weitere gefundene Drehgriffe im Bericht nennen). Werkzeugwechsel mitten im Zug: Rho-Griff und Drehgriff verhalten sich gleich – Verhalten nach Fusion (im Handtest 3 prüfen; bis dahin beide „Zug zurücknehmen“, Test `a_tool_change_takes_over_the_rho_drag` anpassen und Grund in `offene-punkte.md`).

- [ ] **Step 1: Failing tests:** `text_rotate_shows_angle`, `triad_rotate_shows_angle`, `angle_text_uses_display_units`, `tool_change_during_rho_drag_reverts`.
- [ ] **Step 2:** → FAIL. **Step 3:** implementieren. **Step 4:** → PASS.
- [ ] **Step 5:** Commit `feat(app): Winkelanzeige an Drehgriffen`.

### Task 12: Leistung in der App (Liste aus 11b)

**Files:**
- Modify: `crates/planora-feature/src/ops.rs` (`reshape_texts` ~172), `compute.rs` (`compute_sketch` ~901), `params.rs` (~165), `crates/planora-sketch/src/text.rs` (`grow_frame_to_fit` ~1783), `crates/planora-app/src/state.rs` (`is_feature_hidden` ~1045), `crates/planora-feature/src/sketch_groups.rs` (~306), `crates/planora-app/src/sketch_view.rs` (`text_rotate_grips` ~3480/3487)
- Test: vorhandene Tests unverändert grün; neuer Messtest `crates/planora-feature/tests/app_perf_timing.rs` (`#[ignore]`, Release)

**Interfaces:**
- Produces: kein neues API. Festlegungen: `reshape_texts` klont nur Skizzen mit `report.changed`; `compute_sketch` baut `ParamScope` nur, wenn `sketch.texts().next().is_some()`; `grow_frame_to_fit` löst ohne Klon der ganzen Skizze (Löser auf der Arbeitskopie nur der betroffenen Gruppe oder Rücksprung bei Fehlschlag); `is_feature_hidden` über einen je Dokumentstand (`edit_count`) gefüllten Zwischenspeicher der verdeckten Gruppen; `text_rotate_grips` einmal je Bild.

- [ ] **Step 1:** Messtest schreiben (Dokument mit 50 Skizzen à 5 Texte, 20 verschachtelte Gruppen: Parameteränderung, Rückgängig, Zeichnen von 100 Bildern); auf master laufen lassen, Zahlen notieren.
- [ ] **Step 2:** Je Stelle ändern, `cargo test -p planora-feature -p planora-sketch -p planora-app` grün.
- [ ] **Step 3:** Messtest erneut; jede Stelle messbar schneller (Median aus 5). Zahlen in `offene-punkte.md`.
- [ ] **Step 4:** Commit `perf: Leistungspunkte aus Skizzen 11b`.

### Task 13: Härtungen nach Skizzen 5

**Files:**
- Modify: `crates/planora-sketch/src/pattern.rs` (Schleife über zwei Muster, Gitter-Meldung), `crates/planora-sketch/src/model.rs` oder Fundort von `Id::from_index` (`graphify query "Id from_index"`), `from_data` (Glatt mit veraltetem Ende), Löser (`solver.rs`/`sparse.rs`: Vorschau ohne Klon, Gruppen in `add_constraint`, `dof`), `{de,en}.ftl`
- Test: `crates/planora-sketch/tests/hardening_s5.rs`

**Interfaces:**
- Produces:
  - Schleife A↔B: Anlegen/Ändern wird abgelehnt mit der vorhandenen Schleifenmeldung; beim Laden werden die Muster der Schleife aufgelöst und gezählt (wie die Einzelmuster-Schleife).
  - `Id::from_index` → `Id::try_from_index(usize) -> Option<Id<T>>`; Aufrufer melden Fehler statt still abzuschneiden (`MusterWert`).
  - `from_data`: lebendes Glatt mit veraltetem Ende → Fehler „beschädigt“.
  - Gitter scheitert: eigene Meldung `sketch-pattern-grid-parallel` („Die Richtungslinien des Musters sind parallel.“ / „The pattern direction lines are parallel.“) bzw. passender Grund.
  - Löser: Vorschau klont die Skizze nur bei geänderter Version; `add_constraint` hält die Zusammenhangsgruppen inkrementell; `dof` wird im Fall `NotConverged` neu berechnet.

- [ ] **Step 1: Failing tests:** `loop_across_two_patterns_is_refused`, `loop_across_two_patterns_on_load_dissolves`, `huge_index_is_refused`, `stale_smooth_end_is_corrupt`, `parallel_grid_reports_reason`, `preview_does_not_clone_unchanged_sketch` (Zähler oder `Arc::ptr_eq`), `add_constraint_groups_scale_linearly` (1000 Abhängigkeiten, Zeit grob linear; `#[ignore]` Release).
- [ ] **Step 2:** → FAIL. **Step 3:** implementieren. **Step 4:** → PASS; `cargo test -p planora-sketch` grün.
- [ ] **Step 5:** Commit `fix(sketch): Härtungen nach Skizzen 5`.

### Task 14: Doku und voller Lauf

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Abschnitt Skizzen 12 vollständig, Härtungen und Leistung abhaken), `PlanoraCAD/CLAUDE.md` (Architektur: `planora-render/stroke.rs`, `planora-feature/sketch_display.rs`), Benutzer-Doku/Wiki-Seiten im Repo, falls Skizzenfeld beschrieben ist (`graphify query "Skizzenfeld Doku"`), README nur falls Funktionsliste.

- [ ] **Step 1:** Doku schreiben.
- [ ] **Step 2:** Windows: fmt je Crate, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, Messtests im Release (`--ignored`). WSL: `~/wsl-s6-full.sh`; danach WSL aufräumen und `df -h /`.
- [ ] **Step 3:** `graphify . --update --code-only`.
- [ ] **Step 4:** Commit `docs: Skizzen 12 Anzeige in der Skizze`.
- [ ] **Step 5:** **Dennis um Go für Handtest 3 fragen** (Winkelanzeige, Werkzeugwechsel im Zug, Gesamtdurchgang aller Annahmen). Danach Gesamtprüfung des Zweigs, Vault-Übergabe in `Sessions/`, Roadmap Zeile 12 und Fusion-Vergleich pflegen. Merge nur auf „bitte mergen“.
