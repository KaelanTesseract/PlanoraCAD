# Skizzen 11b Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Ziel:** Geometrie übernehmen und Skizzen ordnen wie in Autodesk Fusion: Strg+C/X/V zwischen Skizzen, Skizze als DXF speichern, Skizzengruppen im Browser, Texthöhe als Formel und `{Name}` im Text, Drehgriff am Text, Drehprofil und Isoparametrische Kurve.

**Aufbau:** Je Baustein erst das Modell (planora-sketch, planora-feature, planora-drawing), dann die App. Kopieren und DXF brauchen kein Dateiformat; Task 7 legt alle neuen Daten auf einmal mit Format 22 an, die Bausteine 3 bis 6 setzen darauf auf. Drehprofil und Isokurve laufen über das Projektionsmodell aus Skizzen 6/10/11a (`copies`, `source_pieces`, schwache Bezüge, „Verlorene Projektionen verwalten“).

**Technik:** Rust, egui/eframe 0.36 (egui-winit 0.36.2), wgpu. Keine neue Bibliothek.

**Spec:** `docs/superpowers/specs/2026-11-17-skizzen-uebernehmen-design.md` (freigegeben, maßgeblich, besonders „Entscheidungen nach dem Gegenlesen“ 1–7). Hintergrund mit Datei:Zeile: `.superpowers/sdd/2026-11-17-skizzen-11b/recherche.md` (nicht im Git). Jede Aufgabe liest die Spec mit.

## Globale Vorgaben

- Zweig `feature/skizzen-uebernehmen`; keine Zweigwechsel, kein `git stash`.
- Prüfen je Aufgabe: `cargo fmt -p <crate> -- --check` je berührtem Crate (nie `--all`, „os error 206“), `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`. Nur **ein** cargo-Lauf gleichzeitig, auch über WSL und andere Sitzungen hinweg (vorher `Get-Process cargo,rustc -ErrorAction SilentlyContinue` bzw. in WSL `pgrep -af 'cargo|rustc|wsl-s6'`); bei „os error 1455“ mit `-j 4`.
- Texte nur über `tr!`/`msg!`, Schlüssel in `crates/planora-i18n/languages/de.ftl` und `en.ftl`. Namen: „Kopieren“/„Copy“, „Ausschneiden“/„Cut“, „Einfügen“/„Paste“, „Als DXF speichern“/„Save As DXF“, „Gruppe erstellen“/„Create Group“, „Umbenennen“/„Rename“, „Gruppe auflösen“/„Ungroup“, „Drehprofil“/„Spun Profile“, „Isoparametrische Kurve“/„Isoparametric Curve“, „Projektionsverknüpfung“/„Projection Link“, „Achse projizieren“/„Project Axis“, „Umkehren“/„Flip“, „Toleranz“/„Tolerance“, „Körper“/„Bodies“, „Bestimmte Elemente“/„Specific Geometry“, „Einzeln“/„Single“, „Verteilt“/„Distributed“, „Entfernung“/„Distance“, „Anzahl“/„Count“. Deutsche Namen nicht gegengeprüft = **Annahme** (in `offene-punkte.md`).
- Feste Meldungen (Spec): „Nur ebene Skizzen lassen sich als DXF speichern.“ / „Only planar sketches can be saved as DXF.“; „Drehprofil geht in dieser Fassung nur für Drehkörper um die gewählte Achse.“ / „Spun Profile currently works only for bodies of revolution about the chosen axis.“; „Die Achse muss in der Skizzenebene liegen oder parallel zu ihr sein.“ / „The axis must lie in the sketch plane or parallel to it.“
- Neue Aktionen in `action_table!` (`crates/planora-app/src/keymap/defaults.rs`): `SketchCopy` `"skizze.kopieren"` `[ctrl(Key::C)]`, `SketchCut` `"skizze.ausschneiden"` `[ctrl(Key::X)]`, `SketchPaste` `"skizze.einfuegen"` `[ctrl(Key::V)]` (Bereich `Sketch`); ohne Vorbelegung `[]`: `SketchSaveAsDxf` `"modell.skizze_als_dxf"`, `SketchGroupCreate` `"modell.gruppe_erstellen"`, `SketchGroupUngroup` `"modell.gruppe_aufloesen"` (Bereich `Model`), `SketchSpunProfile` `"skizze.drehprofil"`, `SketchIsoparametric` `"skizze.isokurve"` (Bereich `Sketch`). Jede Taste in den Einstellungen belegbar.
- Neue Dialoge (`Dialog::new`) mit `.dock_key(..)`, Schlüssel in `detach::DIALOG_KEYS` (`crates/planora-app/src/detach.rs:21`): `"sketch_dxf"`, `"tree_group_rename"`, `"sketch_spun_profile"`, `"sketch_isoparametric"`. Dialog-Closures leeren außen angelegte Listen am Anfang. Test `dialog_dock_keys` bleibt grün.
- Ein Befehl = ein Rückgängig-Schritt; Abbruch (Esc, Rechtsklick, Werkzeugwechsel) hinterlässt nichts. Einzige gewollte Ausnahme: Einfügen + Verschieben (Spec-Entscheidung 5, Task 3).
- Dateiformat: `FORMAT_VERSION = 22` ab Task 7 (`crates/planora-project/src/convert.rs:53`). 22 nur mit Skizzengruppe, Text mit Höhenformel oder `{…}`-Verweis (auch gelöschter Texteintrag), Drehprofil oder Isokurve. Alte Dateien bytegleich, ein Dokument ohne das Neue behält sein Format. Kopieren/Einfügen und DXF ändern die Datei nie.
- Verlorener Bezug von Drehprofil und Isokurve ist eine Warnung am Skizzen-Feature, kein Fehler; die Kurven bleiben (wie `CopySource::OnSurface`).
- Kommentare deutsch. Kein Autodesk-Material (keine SHX-Schriften, keine Hilfetexte). `cargo deny check` grün, keine neue Abhängigkeit.
- Code-Fragen zuerst mit `graphify query "<frage>"` (Graph in `graphify-out/`), dann gezielt lesen; das gilt auch für Briefings an Subagenten.
- Jede Annahme und Abweichung kommt in `docs/superpowers/offene-punkte.md` (Abschnitt „Skizzen 11b“), spätestens in Task 17; die Aufgabe, die sie trifft, trägt sie gleich ein.
- Commit je Aufgabe: `git commit -m "Skizzen 11b Task N: <Titel>" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"` (Leerzeile durch das zweite `-m`).
- Nach jeder Aufgabe: Vault-Notiz `D:\Vault\Vault\Projekte\PlanoraCAD\2026-MM-TT Skizzen 11b Task N <Titel>.md` mit Frontmatter `aktualisiert: 2026-MM-TT` und `tags: [planoracad]`, Abschnitte **Geändert** / **Warum** / **Offen**, verlinkt in `Projekte/PlanoraCAD/PlanoraCAD.md` unter „Aufgaben-Notizen“. Keine Schlüssel oder Tokens.
- Handtests nur nach Dennis' Go (der Steuerer fragt, jedes Mal neu), bei 1920×1080 mit `tools/gui-drive.ps1`, APPDATA/USERPROFILE auf einen Testordner; `C:\Users\denni\Documents\PlanoraCAD` nicht anfassen; danach Test-Builds und Fotos im Scratchpad löschen.

## Prüf-Schwerpunkte

1. **Strg+V bei leerer Systemzwischenablage.** egui-winit macht aus Strg+C/X/V keine Tasten, sondern `Event::Copy`/`Event::Cut`/`Event::Paste(text)`, und `Paste` nur, wenn die Systemzwischenablage Text hat (`egui-winit-0.36.2/src/lib.rs:1021-1033`); das Loslassen von V kommt trotzdem als `Event::Key`. Erwartet: Einfügen klappt auch bei leerer Systemzwischenablage, genau einmal. Test `ctrl_v_with_empty_system_clipboard_pastes` in Task 3.
2. **Einfügen, nachdem die Quellskizze gelöscht ist, und in ein anderes Dokument** (anderer Tab, andere Parameter): keine Abhängigkeit zeigt ins Leere, Formeln mit fehlendem Parameter werden zum Wert, die Meldung nennt die Zahl. Test `paste_after_source_deleted_into_other_document` in Task 2.
3. **Skizze in einer Gruppe löschen, dann Rückgängig; verschachtelte ausgeblendete Gruppen speichern und laden.** Erwartet: nach Löschen ist sie aus der Gruppe, nach Rückgängig wieder darin; Laden gibt dieselbe Gruppenstruktur samt Augen. Tests `delete_sketch_in_group_then_undo` (Task 8) und `nested_hidden_groups_roundtrip` (Task 7).
4. **Parameter, auf den `{Name}` zeigt, wird gelöscht** („durch Wert ersetzen“): der Text zeigt `{Name}` wörtlich, die Skizze warnt, kein Fehler, Körper aus dem Text rechnen weiter. Test `deleted_parameter_leaves_literal_and_warning` in Task 11.
5. **Isokurve auf einer Zylinderfläche mit Loch und über die Naht:** ein Wert ergibt mehrere Stücke, keines läuft durch das Loch, keines bricht an der Naht falsch ab. Test `cylinder_with_hole_splits_iso_line` in Task 15.

---

## Etappe 1: Kopieren, Ausschneiden, Einfügen

### Task 1: Zwischenablage im Skizzenmodell

**Dateien:**
- Create: `crates/planora-sketch/src/clipboard.rs`; Modify: `crates/planora-sketch/src/lib.rs` (Modul, Export)
- Test: `crates/planora-sketch/tests/clipboard.rs` (neu)

**Schnittstellen:**
- Consumes: `Sketch::transform_set`, `texts_in_set`, `text_off_its_path`, `copy_curve` (`helpers.rs:409`), `copy_texts` (`transform.rs:1443`), `copy_inner_constraints` (`transform.rs:1554`), `CopyMap` (`transform.rs:224`), `Sketch::has_3d_entry` (`data.rs:139`), `Sketch::set_three_d` (`model.rs:309`).
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Default)] pub struct ClipSet { pub curves: Vec<CurveId>, pub points: Vec<PointId>, pub texts: Vec<TextId> }`
  - `pub struct Extracted { pub clip: Sketch, pub map: CopyMap }` (`map`: Quelle → Clip)
  - `Sketch::extract(&self, set: &ClipSet) -> Result<Extracted, SketchError>`: eine eigene, neue Skizze (`Sketch::new(frame, "", tol)`, `three_d` gesetzt, wenn der Inhalt 3D ist) mit eigenen, dichten Ids. Kurven mit Stil (Konstruktion, Mittellinie, Rahmen, Kamm, Normale), Punkte, Texte mit Rahmen und ihren internen Abhängigkeiten, alle Abhängigkeiten und Maße, deren Beteiligte alle in der Menge liegen, mit `driven`, `enabled`, Zweig, Gegenwinkel; **ohne** `Fixed`/`Fixed3` (wie `copy_transformed`: die Kopie ist frei und lässt sich danach verschieben; **Annahme**). Feste Splines bleiben feste Splines. Muster werden zu gewöhnlicher Geometrie. Text auf Pfad ohne Pfad → `Err(InvalidInput(tr!("sketch-text-path-follows")))`; leere Menge → `sketch-nothing-selected`. Die Quelle bleibt unverändert (auch ihr Stand).
  - `Sketch::paste_from(&mut self, clip: &Sketch) -> Result<CopyMap, SketchError>` (`CopyMap`: Clip → Ziel): legt alles Lebende aus `clip` mit neuen Ids an, atomar; vorhandene Ids, Texte samt gesetzter Form und der übrige Inhalt bleiben, `revision()` steigt. Neue Texte dürfen „noch nicht gesetzt“ sein (Kurven stehen). 3D-Punkte in eine 2D-Skizze: Fehler `sketch-err-tool-not-in-3d`, das Umschalten macht der Aufrufer (Task 2).
  - Weg frei (über die vorhandenen Kopier-Hilfen verallgemeinert auf eine fremde Quelle oder über `SketchData` mit Indexversatz); eine gemeinsame innere Funktion für beide Richtungen.

- [ ] **Schritt 1: Tests schreiben** (`clipboard.rs`):
  ```rust
  #[test] fn extract_keeps_inner_constraints_drops_outer() // Rechteck (4 Linien, Coincident, H/V, 2 Maße) + Linie außerhalb mit Parallel zu einer Rechteckkante: Clip hat 4 Kurven, 2 Maße, kein Parallel, kein Fixed
  #[test] fn extract_has_own_dense_ids()                  // Quelle mit 50 gelöschten Kurven davor: clip.curves() hat Ids 0..n ohne Lücken, kein Id-Wert der Quelle nötig
  #[test] fn extract_keeps_style_and_fixed_spline()        // Konstruktion, Mittellinie, Kamm, FixedSpline: alles im Clip gleich
  #[test] fn extract_text_with_box()                       // Text im Rahmen: Clip hat Text, 4 Rahmenlinien, Inhalt und Höhe gleich
  #[test] fn extract_text_on_path_without_path_refused()   // Fehlertext sketch-text-path-follows, Quelle unverändert (revision gleich)
  #[test] fn paste_twice_gives_two_independent_copies()    // paste_from zweimal: doppelte Zahl, keine Abhängigkeit zwischen den Kopien, alte Ids unverändert
  #[test] fn paste_keeps_driven_and_disabled()             // anzeigendes Maß bleibt anzeigend, abgeschaltete Abhängigkeit abgeschaltet
  #[test] fn paste_3d_clip_into_2d_sketch_errors()         // ohne set_three_d: Fehler, Ziel unverändert
  #[test] fn paste_solves_like_source()                    // nach paste_from und solve: Punkte an derselben Lage wie in der Quelle (1e-9)
  ```
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test clipboard` → FAIL (Funktionen fehlen).
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-sketch` (besonders `transform`, `transform_3d`, `text_*`).
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 1: Zwischenablage im Skizzenmodell`. Vault-Notiz „Task 1 Zwischenablage Modell“.

### Task 2: Zwischenablage am Skizzen-Feature

**Dateien:**
- Create: `crates/planora-feature/src/clipboard.rs`; Modify: `crates/planora-feature/src/lib.rs`
- Test: `crates/planora-feature/tests/clipboard.rs` (neu)

**Schnittstellen:**
- Consumes: Task 1 (`ClipSet`, `Sketch::extract`, `Sketch::paste_from`); `SketchFeature { copies, formulas }` (`model.rs:340`); `ParamScope::eval` (`params.rs:69`); `constraint_unit` (`formulas.rs:411`); `Sketch::move_rigid3` (`transform.rs:998`), `Rigid3`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum ClipSlot { Dim(ConstraintId) }` (Ids im Clip; Task 10 ergänzt `TextHeight(TextId)`)
  - `#[derive(Clone, Debug, PartialEq)] pub struct ClipFormula { pub slot: ClipSlot, pub text: String, pub value: f64 }`
  - `#[derive(Clone, Debug)] pub struct FeatureClip { pub sketch: Sketch, pub frame: Frame, pub formulas: Vec<ClipFormula>, pub made_free: usize }` (`frame`: Weltlage der Quellebene)
  - `pub fn copy_selection(sf: &SketchFeature, frame: &Frame, set: &ClipSet) -> Result<FeatureClip, SketchError>`: Ziele aus `sf.copies` in der Menge werden freie Kurven/Punkte (`made_free` = ihre Zahl); jede Formel an einem kopierten Maß mit ihrem gespeicherten Wert.
  - `#[derive(Clone, Debug, PartialEq)] pub struct PasteReport { pub map: CopyMap, pub formulas_dropped: usize, pub made_3d: bool }`
  - `pub fn paste_clip(sf: &mut SketchFeature, frame: &Frame, clip: &FeatureClip, scope: &ParamScope) -> Result<PasteReport, SketchError>`. Lage (Spec 1 „Einfügen“, so festgelegt): 2D-Inhalt → dieselben x/y der Zielebene; 3D-Inhalt in eine 3D-Skizze → dieselben Weltkoordinaten (`Rigid3` von `clip.frame` nach `frame`); 3D-Inhalt in eine 2D-Skizze → `set_three_d(true)`, dieselben x/y/z, `made_3d = true`. Formel: lässt sie sich mit `scope` in der Einheit des Maßes rechnen, kommt sie in `sf.formulas`; sonst bleibt nur der Wert, `formulas_dropped += 1` (Spec-Entscheidung 3). Nie ein Eintrag in `sf.copies`. Bei einem Fehler bleibt `sf` unverändert.

- [ ] **Schritt 1: Tests schreiben** (`clipboard.rs`):
  ```rust
  #[test] fn projection_becomes_free_curve()           // Skizze mit projizierter Kante + eigener Linie: Clip.made_free == 1; nach paste_clip in zweite Skizze: copies der Zielskizze leer, Kurve ohne Fixed
  #[test] fn formula_survives_when_parameter_exists()  // Maß „Breite*2“ mit Parameter Breite: Ziel hat Formel „Breite*2“, Wert gleich
  #[test] fn formula_becomes_value_without_parameter() // Ziel-Dokument ohne Breite: keine Formel, Wert = gespeicherter Wert, formulas_dropped == 1
  #[test] fn paste_2d_into_other_plane_keeps_xy()      // Quelle XY, Ziel XZ: lokale x/y gleich
  #[test] fn paste_3d_into_3d_keeps_world()            // 3D-Kurve aus Skizze auf XY in 3D-Skizze auf YZ: Weltpunkte gleich (1e-9)
  #[test] fn paste_3d_into_2d_makes_3d()               // made_3d, Ziel has_3d_entry()
  #[test] fn paste_after_source_deleted_into_other_document() // Prüf-Schwerpunkt 2: Clip aus Dokument A, Skizze in A löschen, in Dokument B (ohne Parameter, anderer Ebene) einfügen: recompute ohne Fehler und Warnung, alle Abhängigkeiten zeigen auf lebende Ziel-Ids, formulas_dropped == Zahl der Formeln
  ```
- [ ] **Schritt 2:** `cargo test -p planora-feature --test clipboard` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-feature`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 2: Zwischenablage am Skizzen-Feature`. Vault-Notiz „Task 2 Zwischenablage Feature“.

### Task 3: Kopieren, Ausschneiden, Einfügen in der App

**Dateien:**
- Create: `crates/planora-app/src/keymap/clipboard_keys.rs`; Modify: `crates/planora-app/src/keymap/mod.rs`, `keymap/defaults.rs`, `keymap/dispatch.rs` (`pressed_if`, `pressed_in`-Aufrufer für abgedockte Fenster), `keymap/settings_ui.rs` (Erfassen: Copy/Cut/Paste als Strg+C/X/V erkennen)
- Modify: `crates/planora-app/src/workspace.rs` (Feld `sketch_clipboard`), `commands.rs` (drei Aktionen), `state.rs` (`copy_sketch_selection`, `cut_sketch_selection`, `paste_sketch_clip`, Zusammenfassen in `observe_sketch`), `sketch_state.rs` (`TransformState.pasted`, `curve_menu` öffnet auch mit Auswahl oder voller Zwischenablage), `sketch_view.rs` (`show_curve_menu`: drei Einträge mit Tastentext), `undo_labels.rs`, `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_clipboard.rs` (neu), `crates/planora-app/tests/keymap_dispatch.rs`

**Schnittstellen:**
- Consumes: Task 2 (`FeatureClip`, `copy_selection`, `paste_clip`, `PasteReport`); Löschweg der Skizze (Löschregel `remove_curve_ends`, `constraint.rs:2983`); `History::replace_current` (`history.rs:161`); `SketchSession::set_tool(sk, Tool::MoveCopy)`.
- Produces:
  - `#[derive(Clone, Copy, Debug, Default)] pub struct PasteLatch { pub paste_seen: bool }`
  - `pub fn clipboard_events_as_keys(events: &mut Vec<egui::Event>, typing: bool, latch: &mut PasteLatch)`: mit `typing` (ein Textfeld hat den Fokus: Formelfeld, Felder in der Zeichnung, Textwerkzeug, Dialogfelder; Prüfung wie `sketch_inline.rs:165`) bleibt alles, wie es ist. Sonst wird `Event::Copy` zu Taste C gedrückt mit Strg, `Event::Cut` zu X, `Event::Paste(_)` zu V (und `paste_seen = true`); ein losgelassenes Strg+V (`Event::Key { key: V, pressed: false }` mit Strg) ohne `Paste` seit dem letzten Loslassen wird zusätzlich ein Druck (leere Systemzwischenablage); danach `paste_seen = false`. Der Riegel liegt je Fenster in `ctx.data_mut` (temp). Aufruf vor `pressed_in` in `pressed_if` und im Weg der abgedockten Fenster.
  - `Workspace.sketch_clipboard: Option<planora_feature::FeatureClip>` (über Tabs und Dokumente, solange das Programm läuft; **Annahme**)
  - `AppState::copy_sketch_selection(&mut self) -> Result<FeatureClip, Msg>`; `cut_sketch_selection(&mut self) -> Result<FeatureClip, Msg>` (ein Schritt „Ausschneiden“, Löschen wie Entf); `paste_sketch_clip(&mut self, clip: &FeatureClip) -> Result<PasteReport, Msg>` (ein Schritt „Einfügen“; danach alle eingefügten Elemente gewählt, `Tool::MoveCopy` offen mit ihnen, `TransformState.pasted = true`).
  - Bestätigtes Verschieben mit `pasted`: `observe_sketch` legt keinen neuen Schritt an, sondern ersetzt das aktuelle Bild (`replace_current`), der Name bleibt „Einfügen“. Abbruch des Verschiebens: die Kopie bleibt an der Quelllage (Spec-Entscheidung 5).
  - Meldungen (`msg!`): `sketch-copy-done` (Zahl), `sketch-paste-made-free` (Zahl der Projektionen, die frei wurden), `sketch-paste-formulas-dropped` (Zahl), `sketch-paste-empty` (Zwischenablage leer). Schrittnamen `undo-sketch-cut` („Ausschneiden“), `undo-sketch-paste` („Einfügen“).
  - Ohne offene Skizze: Menüeinträge ausgegraut, Tasten wirken nicht (Bereich `Sketch`).

- [ ] **Schritt 1: Tests schreiben:**
  `keymap_dispatch.rs`:
  ```rust
  #[test] fn copy_event_becomes_ctrl_c()                 // [Event::Copy] → pressed_in liefert SketchCopy
  #[test] fn paste_event_becomes_ctrl_v_once()           // [Paste("x"), Key{V, pressed:false, ctrl}] → genau einmal SketchPaste
  #[test] fn ctrl_v_with_empty_system_clipboard_pastes() // Prüf-Schwerpunkt 1: nur [Key{V, pressed:false, ctrl}] → SketchPaste, ein zweites Loslassen im nächsten Bild ohne neuen Druck → nichts mehr
  #[test] fn typing_keeps_clipboard_events()             // typing = true: Copy/Paste bleiben in events, keine Aktion
  #[test] fn rebound_copy_works_with_plain_key()         // SketchCopy auf Strg+K umbelegt: Key K mit Strg → SketchCopy; Event::Copy → nichts
  ```
  `sketch_clipboard.rs`:
  ```rust
  #[test] fn copy_paste_between_sketches()       // Skizze A: Rechteck mit 2 Maßen kopieren; Skizze B öffnen, einfügen: 4 Kurven, 2 Maße, gewählt, Tool::MoveCopy offen
  #[test] fn paste_and_move_is_one_undo_step()   // einfügen, verschieben bestätigen: ein Schritt „Einfügen“; Rückgängig → Skizze wie vor dem Einfügen
  #[test] fn paste_then_cancel_move_keeps_copy() // einfügen, Esc: Kopie an Quelllage, ein Schritt „Einfügen“
  #[test] fn cut_is_one_step_with_delete_rule()  // Linie mit Endpunkt-Maß ausschneiden: Linie, Endpunkte und Maß weg, ein Schritt „Ausschneiden“; Einfügen bringt Linie und Maß zurück
  #[test] fn commands_greyed_without_sketch()    // ohne offene Skizze: Aktionen nicht ausführbar, keine Änderung
  #[test] fn formula_field_keeps_ctrl_c()        // Formelfeld fokussiert: Strg+C kopiert keine Skizzenauswahl, Zwischenablage unverändert
  #[test] fn right_click_menu_with_selection_or_clip() // Auswahl nur Punkte → Menü offen; leere Auswahl + volle Zwischenablage → Menü offen mit „Einfügen“
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_clipboard --test keymap_dispatch` → FAIL.
- [ ] **Schritt 3:** Umsetzen; Abweichung „Einfügen + Verschieben ein Schritt“ und „Zwischenablage nur programmintern“ in `offene-punkte.md`.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-app --test sketch_clipboard --test keymap_dispatch --test keymap --test keymap_settings --test sketch_move_copy --test sketch_move_3d --test sketch_comb --test dialog_dock_keys`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 3: Kopieren, Ausschneiden, Einfügen in der App`. Vault-Notiz „Task 3 Kopieren in der App“.

---

## Etappe 2: Skizze als DXF

### Task 4: DXF-Schreiber für ebene Modelle

**Dateien:**
- Modify: `crates/planora-drawing/src/dxf.rs` (`header` mit Einheit und Ausdehnung statt Blattgröße, `write_tables` mit Ebenenliste von außen; der Blattweg ruft beides mit den heutigen Werten), `crates/planora-drawing/src/lib.rs`
- Create: `crates/planora-drawing/src/plain_dxf.rs`
- Test: `crates/planora-drawing/tests/plain_dxf.rs` (neu)

**Schnittstellen:**
- Consumes: `Out2d` (`out2d.rs:18`), die Helfer in `dxf.rs` (`Out`, `Handles`, `write_curve` `:1098`, `encode`, `unique_name`).
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum DxfUnit { Millimeter, Centimeter, Meter, Inch, Foot }` mit `pub fn mm_per_unit(self) -> f64` (1, 10, 1000, 25,4, 304,8), `pub fn insunits(self) -> i16` (4, 5, 6, 1, 2), `pub fn metric(self) -> bool` (`$MEASUREMENT` 1 bzw. 0)
  - `pub struct PlainLayer { pub name: String, pub color: i16 }`
  - `pub enum PlainEntity { Curve(Out2d), Point(DVec2) }`
  - `pub struct PlainDxf { pub unit: DxfUnit, pub layers: Vec<PlainLayer>, pub entities: Vec<(usize, PlainEntity)> }` (Koordinaten schon in `unit`; `usize` = Ebenenindex)
  - `pub fn plain_dxf_string(m: &PlainDxf) -> Result<String, DrawError>` und `pub fn write_plain_dxf(m: &PlainDxf, out: &mut dyn Write) -> Result<(), DrawError>`: R2000 wie das Blatt (gleiche Pflichtteile, gleiche Reihenfolge der Abschnitte und Tabellen), `$INSUNITS`/`$MEASUREMENT` nach `unit`, `$EXTMIN`/`$EXTMAX` aus den Objekten, Punkte als `POINT` (`AcDbPoint`), alles im Modellraum ohne Blöcke je Ansicht. Leeres Modell → `DrawError` (kein Text).
  - Der Blattweg (`dxf_string`) bleibt Byte für Byte gleich.

- [ ] **Schritt 1: Tests schreiben** (`plain_dxf.rs`, Rundlauf über `planora_exchange::read_dxf`):
  ```rust
  #[test] fn inch_file_reads_back_in_inch()        // Linie (0,0)-(1,0) in Inch: read_dxf → units == Some(Unit2d::Inch), Linie 1 lang
  #[test] fn mm_cm_m_ft_insunits()                 // je Einheit: $INSUNITS 4/5/6/2 im Text, read_dxf nennt dieselbe Einheit
  #[test] fn point_arc_ellipse_spline_roundtrip()   // je ein Objekt: Element2d gleicher Art, Lage 1e-9
  #[test] fn layers_by_name()                      // zwei Ebenen „0“ und „Konstruktion“: Objekte im richtigen Layer2d
  #[test] fn empty_model_is_error()
  #[test] fn sheet_dxf_unchanged()                 // vor dem Umbau die Ausgabe von dxf_string für das Blatt aus stage11_dxf als tests/data/sheet_before.dxf festhalten; danach gleich
  ```
- [ ] **Schritt 2:** `cargo test -p planora-drawing --test plain_dxf` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-drawing` (besonders `stage11_dxf`, `stage17_dxf_marks`).
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 4: DXF-Schreiber für ebene Modelle`. Vault-Notiz „Task 4 DXF-Schreiber“.

### Task 5: Skizze als DXF-Modell

**Dateien:**
- Create: `crates/planora-feature/src/sketch_dxf.rs`; Modify: `crates/planora-feature/src/lib.rs`, `de.ftl`/`en.ftl`
- Test: `crates/planora-feature/tests/sketch_dxf.rs` (neu)

**Schnittstellen:**
- Consumes: Task 4 (`PlainDxf`, `DxfUnit`, `PlainLayer`, `PlainEntity`); `Sketch::curve_world` (`geom.rs:853`); `planora_drawing::piece_to_outs` (`out2d.rs:658`) mit `Stroke`, `Basis { d: frame.z, ex: frame.x, ey: frame.y }` und `SheetTransform { scale: 1 / unit.mm_per_unit(), tx: 0, ty: 0, bbox_center: basis.project(frame.origin) }` (so werden es lokale Koordinaten in der Einheit; `origin` des `Stroke` liest `piece_to_outs` nicht, prüfen); `Document::sketch_of`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct SketchDxfOptions { pub unit: DxfUnit, pub points: bool, pub projected: bool, pub construction: bool }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SketchDxfError { NotPlanar, Empty, Unknown }` mit `pub fn message(self) -> String` (`NotPlanar` = feste Meldung der Spec; `Empty` = `sketch-dxf-empty`)
  - `pub fn sketch_dxf(doc: &Document, id: FeatureId, o: &SketchDxfOptions) -> Result<PlainDxf, SketchDxfError>`: die berechnete Skizze. `has_3d_entry()` → `NotPlanar`. Linien, Kreise, Bögen, Ellipsen, Ellipsenbögen, Kegelschnitte, alle Splines und die Buchstabenkurven der Texte über `piece_to_outs`; Rahmenlinien der Texte nur mit `construction`. Punkte (nur freie Punkte, die keine Kurve benutzt) als `PlainEntity::Point` nur mit `points`. Ziele aus `copies` nur mit `projected`. Konstruktions- und Mittellinien nur mit `construction`, auf Ebene 1 `tr!("sketch-dxf-layer-construction")` (Farbe 8), alles andere auf Ebene 0 `"0"` (Farbe 7). Nach dem Filter nichts → `Empty`.

- [ ] **Schritt 1: Tests schreiben** (`sketch_dxf.rs`):
  ```rust
  #[test] fn rectangle_100mm_in_inch_is_3_937()    // Rechteck 100 × 50 mm, unit Inch: Linien 3,937007… lang (1e-9)
  #[test] fn construction_only_when_ticked()        // Konstruktionslinie: ohne Häkchen fehlt sie, mit Häkchen auf Ebene 1
  #[test] fn projected_only_when_ticked()           // projizierte Kante: ohne Häkchen fehlt sie
  #[test] fn free_point_only_when_ticked()
  #[test] fn text_as_curves_not_text_entity()       // Text „A“: nur Kurven, keine Rahmenlinien ohne construction
  #[test] fn three_d_sketch_refused()               // NotPlanar, Meldung = Spec-Text
  #[test] fn only_construction_and_unticked_is_empty() // Empty
  #[test] fn sketch_on_offset_plane_uses_local_xy() // Skizze auf versetzter, gedrehter Ebene: Punkt (10, 20) lokal → (10, 20) in der Datei
  ```
- [ ] **Schritt 2:** `cargo test -p planora-feature --test sketch_dxf` → FAIL.
- [ ] **Schritt 3:** Umsetzen; Annahmen (3D-Meldung, Konstruktion vorgabe aus, Ebenennamen, Mittellinien auf der Konstruktionsebene) in `offene-punkte.md`.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-feature`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 5: Skizze als DXF-Modell`. Vault-Notiz „Task 5 Skizze als DXF-Modell“.

### Task 6: „Als DXF speichern“ im Browser

**Dateien:**
- Create: `crates/planora-app/src/sketch_dxf_dialog.rs`; Modify: `crates/planora-app/src/browser.rs` (`feature_menu` bei Skizzen, `Action::SaveAsDxf(FeatureId)`, `run`), `detach.rs` (`"sketch_dxf"`), `keymap/defaults.rs`, `commands.rs`, `lib.rs`/`app.rs` (Dialog zeigen), `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_dxf_export.rs` (neu)

**Schnittstellen:**
- Consumes: Task 5 (`sketch_dxf`, `SketchDxfOptions`, `SketchDxfError`); Task 4 (`write_plain_dxf`, `DxfUnit`); Dateidialog wie im Export (`file_chooser.rs`); `planora_i18n::current_unit()`.
- Produces:
  - `pub struct SketchDxfDialog { pub sketch: FeatureId, pub options: SketchDxfOptions }` mit `pub fn new(state: &AppState, sketch: FeatureId) -> SketchDxfDialog` (Einheit aus `current_unit()`: mm → `Millimeter`, Zoll → `Inch`; `points: true`, `projected: true`, `construction: false`; **Annahme** für die Häkchen außer Konstruktion), `pub fn show(&mut self, ctx, state) -> DialogOutcome` (`.dock_key("sketch_dxf")`), OK → Dateidialog „*.dxf“, Dateiname = Skizzenname
  - `pub fn save_sketch_dxf(state: &AppState, sketch: FeatureId, o: &SketchDxfOptions, path: &Path) -> Result<(), String>` (bei Fehler keine Datei; `NotPlanar`/`Empty` als Meldung)
  - Menüeintrag `tree-save-as-dxf` („Als DXF speichern“) im Kontextmenü jeder Skizzenzeile, Taste über `SketchSaveAsDxf` (ohne Vorbelegung; wirkt auf die gewählte Skizze). Ändert das Dokument nicht (kein Schritt, `edit_count` gleich).

- [ ] **Schritt 1: Tests schreiben** (`sketch_dxf_export.rs`):
  ```rust
  #[test] fn defaults_follow_document_unit()   // Einheit Zoll eingestellt: Dialog startet mit Inch, construction false
  #[test] fn save_writes_file_and_no_step()    // save_sketch_dxf in Testordner: Datei da, read_dxf liest 4 Linien, edit_count unverändert
  #[test] fn three_d_sketch_message_no_file()  // Fehlertext = Spec-Meldung, keine Datei
  #[test] fn menu_entry_on_sketch_rows_only()  // Skizzenzeile hat „Als DXF speichern“, Körperzeile nicht
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_dxf_export` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-app --test sketch_dxf_export --test feature_tree --test dialog_dock_keys --test keymap`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 6: Als DXF speichern im Browser`. Vault-Notiz „Task 6 Als DXF speichern“.

---

## Etappe 3: Format 22 und Skizzengruppen

### Task 7: Format 22 und die neuen Daten

Der eine Formatsprung für alles Neue aus 11b. Nur Daten, Datei und Rundlauf; Rechnen und Oberfläche kommen in Task 8–16.

**Dateien:**
- Create: `crates/planora-feature/src/sketch_groups.rs` (Typen), `crates/planora-feature/src/spun_profile.rs` und `crates/planora-feature/src/isoparametric.rs` (zunächst nur die Typen), `crates/planora-feature/src/text_refs.rs` (`has_param_ref`)
- Modify: `crates/planora-feature/src/model.rs` (`CopySource`, `origins`, `loose_origins`, `same_family`, `is_3d_kind`; `SketchFeature.text_formulas`, `prune_formulas`, `PartialEq`), `snapshot.rs` (`Snapshot.sketch_groups`), `include.rs` (`source_pieces`: neue Arten vorerst `Err(Unfollowed::Lost)`), alle Stellen mit `SketchFeature { .. }` (117 in 69 Dateien, je `text_formulas: TextFormulas::new()`), `crates/planora-app/src/state.rs` (`relink_pickable`: neue Arten `false`)
- Modify: `crates/planora-project/src/format.rs` (DTOs), `convert.rs` (`FORMAT_VERSION = 22`, `needs_format_22`, `has_format_22_data`, Prüfung beim Laden, Hin- und Rückweg); alle Tests mit `assert_eq!(FORMAT_VERSION, 21)` auf 22 (`rg "FORMAT_VERSION, 21" crates`)
- Test: `crates/planora-project/tests/format22.rs` (neu), `fixtures_format22/` (neu), `crates/planora-project/tests/format21.rs` (Grenzen `< 22`)

**Schnittstellen:**
- Produces (planora-feature, öffentlich):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct SketchGroupId(pub u64)`
  - `#[derive(Clone, Debug, PartialEq)] pub struct SketchGroup { pub id: SketchGroupId, pub name: String, pub parent: Option<SketchGroupId>, pub hidden: bool }`
  - `#[derive(Clone, Debug, Default, PartialEq)] pub struct SketchGroups { pub groups: Vec<SketchGroup>, pub member: BTreeMap<FeatureId, SketchGroupId>, pub next: u64 }` mit `is_empty()`; `Document::sketch_groups(&self) -> &SketchGroups`; `Snapshot.sketch_groups: SketchGroups` (Datei und Rückgängig über den `Snapshot`)
  - `pub type TextFormulas = BTreeMap<TextId, String>`; Feld `SketchFeature.text_formulas: TextFormulas` (Formeltext der Höhe; der Text hält weiter den Zahlenwert `height`); `prune_formulas` entfernt Einträge toter Texte; `PartialEq` vergleicht das Feld mit
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum IsoDirection { U, V }` (U: Kurve `u = const`; **Annahme**)
  - `CopySource::Isoparametric { face: FaceRef, direction: IsoDirection, value: f64 }` (`value`: Anteil 0–1, Spec-Entscheidung 4)
  - `#[derive(Clone, Debug, PartialEq)] pub enum SpunSelection { Bodies(Vec<BodyRef>), Elements(Vec<CurveSource>) }`; `#[derive(Clone, Debug, PartialEq)] pub enum SpunAxis { Own(CurveId), Ref(AxisRef) }` (`Own`: Linie der eigenen Skizze); `#[derive(Clone, Debug, PartialEq)] pub struct SpunSpec { pub selection: SpunSelection, pub axis: SpunAxis, pub tolerance: f64, pub flip: bool }`; `CopySource::SpunProfile(SpunSpec)`
  - `origins()`: Körper-Features, Kanten, Achsenkante, Fläche der Isokurve; `loose_origins()`: Quellskizzen (`AxisRef::SketchLine`, `CurveSource::SketchCurve`); `is_3d_kind()` `true` für beide (Gruppe mit Stückzahl wie „Auf Fläche projizieren“)
  - `pub fn has_param_ref(content: &str) -> bool` in `text_refs`: enthält `{Name}` mit einem Namen, den `expr::check_name` annimmt
- Datei (Namen in der Datei deutsch, alle mit `default` und `skip_serializing_if`, damit alte Dateien gleich bleiben): `VersionFileDto.skizzengruppen: Vec<SkizzengruppeDto>` (vor `bilder`; `{ kennung, name, eltern (fehlt oben), aus (fehlt, wenn sichtbar), skizzen: [Feature-Kennungen] }`), `SketchFeatureDto.hoehenformeln: Vec<TextFormulaDto>` (`{ text, formel }`), `EdgeCopyDto.drehprofil: Option<DrehprofilDto>`, `EdgeCopyDto.isokurve: Option<IsokurveDto>` (`{ flaeche, richtung: "u"|"v", anteil }`).
- `needs_format_22` (Snapshot): mindestens eine Gruppe, ein `text_formulas`-Eintrag, ein Texteintrag (auch gelöscht) mit `has_param_ref`, eine Kopie mit `SpunProfile` oder `Isoparametric`. Steht vor `needs_format_21` in `format_with`. `has_format_22_data` (Datei): dieselben Inhalte in der Datei; `file.format < 22` damit → Fehler `proj-since-format` mit `what = tr!("proj-feat-sketch-transfer")` („Skizzengruppen, Textformeln, Drehprofil oder Isokurve“).

- [ ] **Schritt 1: Tests schreiben** (`format22.rs`):
  ```rust
  #[test] fn format_version_is_22()
  #[test] fn old_file_bytes_unchanged()            // wie in format21.rs über data, fixtures, fixtures_format18, fixtures_old_smooth, fixtures_format21, step_fixtures: bytegleich, Format < 22, keine Felder skizzengruppen/hoehenformeln/drehprofil/isokurve
  #[test] fn plain_document_stays_below_22()        // Skizze mit Text ohne {…}, ohne Formel: Format ≤ 21
  #[test] fn group_writes_22_and_roundtrips()
  #[test] fn nested_hidden_groups_roundtrip()       // Prüf-Schwerpunkt 3: Gruppe A ⊃ Gruppe B (aus) ⊃ Skizze; Laden ergibt dieselben SketchGroups
  #[test] fn text_formula_writes_22()
  #[test] fn param_ref_in_text_writes_22()          // „{Breite} mm“
  #[test] fn deleted_text_with_param_ref_needs_22()
  #[test] fn spun_profile_copy_roundtrips()         // Kopie mit SpunProfile (Körper und Elemente, Achse Own und Ref): Laden gleich
  #[test] fn isoparametric_copy_roundtrips()
  #[test] fn format_21_file_with_22_data_refused()  // Datei mit skizzengruppen und "format": 21 → Fehler „ab Format 22“
  ```
- [ ] **Schritt 2:** `cargo test -p planora-project --test format22` → FAIL.
- [ ] **Schritt 3:** Umsetzen. Eintrag in `offene-punkte.md`: „Format 22 gehört 11b; `feature/schaltplan` (`FORMAT_E2`, `convert_eda.rs:39`) geht beim Einpflegen auf die nächste freie Nummer“ (Spec-Entscheidung 1), dazu eine Vault-Notiz im PCB-Bereich.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-project`, `cargo test -p planora-feature`, `cargo test -p planora-app --test lost_projections_dialog --test sketch_text_dependents` (Kompilieren aller Crates mit dem neuen Feld: `cargo test --workspace --no-run`).
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 7: Format 22 und die neuen Daten`. Vault-Notiz „Task 7 Format 22“.

### Task 8: Skizzengruppen im Dokument

**Dateien:**
- Modify: `crates/planora-feature/src/sketch_groups.rs` (Operationen), `ops.rs` (Löschen einer Skizze räumt die Mitgliedschaft), `snapshot.rs` (Wiederherstellen prüft streng: unbekannte Skizze/Gruppe, Kreis → Fehler beim Laden)
- Test: `crates/planora-feature/tests/sketch_groups.rs` (neu)

**Schnittstellen:**
- Consumes: Task 7 (`SketchGroups`, `SketchGroupId`).
- Produces (alle an `Document`; jede Änderung zählt `drawing_edits` hoch wie `set_visibility`, rechnet nicht neu; Fehler mit `FeatureError::InvalidInput`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum GroupItem { Sketch(FeatureId), Group(SketchGroupId) }`
  - `create_sketch_group(&mut self, items: &[GroupItem]) -> Result<SketchGroupId, FeatureError>`: neue Gruppe im gemeinsamen Elternteil aller `items` (sonst oben), Name `tr!("tree-group-name", n = k)` mit dem kleinsten freien k („Gruppe1“/„Group1“, **Annahme**); `items` kommen hinein; leer → Fehler.
  - `rename_sketch_group(&mut self, id: SketchGroupId, name: &str) -> Result<(), FeatureError>` (leer oder nur Leerzeichen → Fehler `tree-rename-empty`)
  - `ungroup_sketch_group(&mut self, id: SketchGroupId) -> Result<(), FeatureError>`: Inhalt (Skizzen und Untergruppen) eine Ebene höher, Gruppe weg; Löschen einer Gruppe ist dasselbe (Spec 3, **Annahme**)
  - `move_to_group(&mut self, item: GroupItem, target: Option<SketchGroupId>) -> Result<(), FeatureError>`: Gruppe in sich selbst oder einen Nachfahren → Fehler `tree-group-cycle`
  - `set_sketch_group_hidden(&mut self, id: SketchGroupId, hidden: bool)`
  - `sketch_hidden_by_group(&self, id: FeatureId) -> bool`: eine Gruppe auf dem Weg nach oben ist aus (das eigene Auge der Skizze bleibt, wie es ist)
  - Löschen einer Skizze nimmt sie aus `member`; leere Gruppen bleiben (Spec 3).

- [ ] **Schritt 1: Tests schreiben** (`sketch_groups.rs`):
  ```rust
  #[test] fn ten_sketches_two_groups()             // Spec „Gelungen“: 10 Skizzen, 2 Gruppen à 5; sketch_hidden_by_group nach set_sketch_group_hidden(A) für die 5 aus A
  #[test] fn hidden_group_keeps_own_eye()          // Skizze selbst aus, Gruppe aus und wieder an: Skizze bleibt selbst aus
  #[test] fn nested_hidden_parent_hides_child_group()
  #[test] fn ungroup_moves_content_one_level_up()
  #[test] fn move_group_into_descendant_refused()
  #[test] fn delete_sketch_in_group_then_undo()    // Prüf-Schwerpunkt 3: Skizze löschen → nicht mehr in member; Snapshot von vorher wiederherstellen → wieder in derselben Gruppe
  #[test] fn empty_group_stays()
  #[test] fn groups_do_not_recompute()             // model_edit_count und result_version aller Features unverändert, edit_count +1
  #[test] fn default_names_count_up()              // Gruppe1, Gruppe2; nach Auflösen von Gruppe1 heißt die nächste wieder Gruppe1
  ```
- [ ] **Schritt 2:** `cargo test -p planora-feature --test sketch_groups` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-feature`, `cargo test -p planora-project --test format22`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 8: Skizzengruppen im Dokument`. Vault-Notiz „Task 8 Skizzengruppen Modell“.

### Task 9: Skizzengruppen im Browser

Der Browser kennt bis jetzt nur eine gewählte Zeile und kein Ziehen; beides kommt hier für den Ordner „Skizzen“ dazu.

**Dateien:**
- Create: `crates/planora-app/src/sketch_tree.rs` (reine Baumlogik)
- Modify: `crates/planora-app/src/browser.rs` (`sketches` zeichnet den Baum; Gruppenzeilen mit Pfeil, Auge, Name, Menü; „Gruppe erstellen“ im Skizzenmenü; Ziehen; `TreePrompt::GroupRename` mit `.dock_key("tree_group_rename")`), `state.rs` (`is_feature_hidden` berücksichtigt `sketch_hidden_by_group`; `browser_sketches: Vec<FeatureId>` für Strg-Klick-Mehrfachwahl), `undo_labels.rs`, `detach.rs`, `keymap/defaults.rs`, `commands.rs`, `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_groups_tree.rs` (neu), `crates/planora-app/tests/feature_tree.rs`

**Schnittstellen:**
- Consumes: Task 8 (alle Gruppen-Operationen, `GroupItem`, `sketch_hidden_by_group`).
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct TreeRow { pub item: GroupItem, pub depth: usize, pub name: String }`
  - `pub fn sketch_tree(doc: &Document, open: &BTreeSet<SketchGroupId>) -> Vec<TreeRow>`: je Ebene erst die Gruppen in Anlegereihenfolge, dann die Skizzen in Verlaufsreihenfolge (**Annahme**); Inhalt zugeklappter Gruppen fehlt. Aufklappzustand gehört zur Ansicht (wie die Ordner, `browser.rs:387`), nicht zum Dokument.
  - `pub fn drop_target_ok(doc: &Document, item: GroupItem, target: Option<SketchGroupId>) -> bool`
  - Strg-Klick auf Skizzenzeilen schaltet sie in `browser_sketches`; einfacher Klick wählt nur diese. „Gruppe erstellen“ nimmt `browser_sketches` (mindestens die Zeile des Menüs).
  - Gruppenmenü: „Umbenennen“, „Gruppe auflösen“, „Löschen“ (= auflösen). Ziehen einer Skizzen- oder Gruppenzeile auf eine Gruppenzeile legt hinein, auf die Ordnerzeile „Skizzen“ legt nach oben.
  - Augen der Gruppen wie die der Skizzen (ohne eigenen Layout-Bereich, gesperrt = gedämpft ohne Wirkung).
  - Schrittnamen: `undo-group-create` („Gruppe erstellen“), `undo-group-rename` („Gruppe umbenennen“), `undo-group-ungroup` („Gruppe auflösen“), `undo-group-move` („In Gruppe verschieben“), `undo-group-visibility` („Gruppe ein-/ausblenden“); je Änderung ein Schritt.

- [ ] **Schritt 1: Tests schreiben** (`sketch_groups_tree.rs`):
  ```rust
  #[test] fn tree_order_groups_then_sketches()
  #[test] fn collapsed_group_hides_rows()
  #[test] fn ctrl_click_then_create_group()      // zwei Skizzen per Strg-Klick, „Gruppe erstellen“: ein Schritt „Gruppe erstellen“, beide drin
  #[test] fn group_eye_hides_sketches_in_view()  // is_feature_hidden aller Mitglieder true, eigenes Auge unverändert
  #[test] fn drop_into_and_out_of_group()        // je ein Schritt
  #[test] fn drop_group_on_own_child_refused()   // drop_target_ok false, nichts geändert
  #[test] fn rename_dialog_has_dock_key()
  #[test] fn undo_ungroup_restores_group()
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_groups_tree` → FAIL.
- [ ] **Schritt 3:** Umsetzen; Annahmen (Reihenfolge, Namen, Löschen = Auflösen, Auge wie Körpergruppen) in `offene-punkte.md`.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-app --test sketch_groups_tree --test feature_tree --test dialog_dock_keys --test kontext_sperre`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 9: Skizzengruppen im Browser`. Vault-Notiz „Task 9 Skizzengruppen Browser“.

### Handtest Etappe 1–3 (kein Code, Handtest nur nach Dennis' Go)

- [ ] Der Steuerer fragt Dennis und wartet auf das Go. Dann `cargo build -p planora-app --release`, Start über `tools/gui-drive.ps1` bei 1920×1080, APPDATA/USERPROFILE umgeleitet.
- [ ] Liste: Rechteck mit Maßen und Formel in Skizze A kopieren, in Skizze B auf anderer Ebene einfügen und gleich verschieben (ein Rückgängig-Schritt); Strg+X; Strg+C im Formelfeld und im Textwerkzeug kopiert dort Text; Einfügen in einen zweiten Tab; Rechtsklickmenü der Skizze. Skizze als DXF in mm und Zoll speichern, mit Konstruktion und ohne. Zehn Skizzen in zwei Gruppen ordnen, verschachteln, ziehen, Auge, umbenennen, auflösen, speichern und neu laden.
- [ ] Dennis öffnet die DXF-Dateien einmal in Fusion 360 (seine Hand).
- [ ] Ergebnis als Vault-Notiz „Skizzen 11b Handtest 1“; Funde als eigene Korrekturaufgaben; Test-Builds und Fotos im Scratchpad löschen.

---

## Etappe 4: Text-Reste

### Task 10: Texthöhe als Formel

**Dateien:**
- Modify: `crates/planora-sketch/src/text.rs` (`set_text_height`), `crates/planora-feature/src/formulas.rs` (`Slot::TextHeight`, `formula_slots`, `mentions`, `rewrite_formulas`, `evaluate_sketch`, `prune_sketch`), `crates/planora-feature/src/clipboard.rs` (`ClipSlot::TextHeight`), `crates/planora-app/src/sketch_text.rs` (Feld „Höhe“ als Formelfeld wie die Maße, `formula_field.rs`), `de.ftl`/`en.ftl`
- Test: `crates/planora-feature/tests/formulas_sketch.rs`, `crates/planora-sketch/tests/text_edit.rs`, `crates/planora-app/tests/sketch_text_formula.rs` (neu)

**Schnittstellen:**
- Consumes: Task 7 (`SketchFeature.text_formulas`); `evaluate_sketch` (`formulas.rs:454`); `ParamScope::eval`.
- Produces:
  - `Sketch::set_text_height(&mut self, t: TextId, h: f64) -> Result<(), SketchError>`: wie die Höhe in `edit_text_box`/`edit_text` (Kurven der Reihe nach wiederverwendet); `h` nicht endlich oder ≤ Toleranz → Fehler wie `check_params`; gleicher Wert ändert nichts (auch nicht den Stand); ein noch nicht gesetzter Text bekommt nur den Wert, das nächste Neusetzen setzt ihn.
  - `Slot::TextHeight(TextId)` mit Einheit `ParamUnit::Length`; die Formelrunde setzt Texthöhen vor dem Lösen; Fehler → Warnung bzw. Fehlerart wie bei Maßen (Text `feature-formula-in-text-height`, „Formel der Texthöhe: …“).
  - Umbenennen eines Parameters schreibt auch `text_formulas` um (`rewrite_formulas`).
  - `ClipSlot::TextHeight(TextId)`; `copy_selection`/`paste_clip` tragen die Höhenformel mit derselben Regel wie Maße (fehlender Parameter → nur Wert).

- [ ] **Schritt 1: Tests schreiben:**
  `formulas_sketch.rs`:
  ```rust
  #[test] fn text_height_follows_parameter()        // Text mit Höhenformel „H/2“, H = 10 → 5; H = 20 → Höhe 10, Kurven doppelt so hoch, Ids gleich
  #[test] fn text_height_formula_renamed_with_param() // H → Hoehe: Formel „Hoehe/2“
  #[test] fn bad_text_height_formula_is_issue()       // „H-30“ ergibt ≤ 0: Meldung feature-formula-in-text-height, Text behält alte Höhe
  ```
  `text_edit.rs`: `set_text_height_reuses_curves`, `set_text_height_same_value_keeps_revision`.
  `sketch_text_formula.rs`: `height_field_takes_formula` (Textdialog: „H/2“ eingeben → Eintrag in `text_formulas`, ein Schritt), `paste_text_keeps_height_formula`.
- [ ] **Schritt 2:** `cargo test -p planora-feature --test formulas_sketch` → FAIL.
- [ ] **Schritt 3:** Umsetzen; **Annahme** „kein eigener Parameter je Text“ (Spec 4) in `offene-punkte.md`.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-sketch`, `cargo test -p planora-feature`, `cargo test -p planora-app --test sketch_text_formula --test sketch_clipboard --test no_raw_text_edits`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 10: Texthöhe als Formel`. Vault-Notiz „Task 10 Texthöhe als Formel“.

### Task 11: Parameter im Text

Die Feature-Berechnung hat keinen `TextShaper` (`planora-feature/Cargo.toml:22-24`): Ersetzt und neu gesetzt wird darum in `Document::reshape_texts` (das die App mit der Schriftliste ruft); die Neuberechnung prüft nur die Verweise und warnt.

**Dateien:**
- Modify: `crates/planora-feature/src/text_refs.rs` (Ersetzen, Umbenennen, `apply_shown`), `ops.rs` (`Document::reshape_texts` setzt vorher den gezeigten Inhalt), `compute.rs` (Warnung am Skizzen-Feature), `params.rs` (Umbenennen schreibt `{alt}` → `{neu}` in allen Texten), `crates/planora-sketch/src/text.rs` (`TextData.shown`, `set_text_shown`, `shaping_content` nimmt `shown`), `crates/planora-app/src/workspace.rs` (`reshape_due = true` nach jeder Änderung der Parametertabelle, nach Rückgängig/Wiederholen einer solchen und nach Wechsel der Längeneinheit), `sketch_text.rs` (offene Skizze setzt mit `apply_shown`), `de.ftl`/`en.ftl`
- Test: `crates/planora-feature/tests/text_refs.rs` (neu), `crates/planora-app/tests/sketch_text_params.rs` (neu)

**Schnittstellen:**
- Consumes: Task 7 (`has_param_ref`); `ParamScope::lookup` (`params.rs:64`); `expr::check_name`, `expr::same_name`; `planora_i18n::fmt_len_num`, `format_number`; `Sketch::reshape_texts` (`text.rs:1602`).
- Produces:
  - `pub fn param_refs(content: &str) -> Vec<String>` (Namen in `{…}`, Reihenfolge des Auftretens)
  - `pub fn substitute(content: &str, scope: &ParamScope) -> (String, Vec<String>)`: ersetzt jeden bekannten `{Name}` durch den Wert ohne Einheitenzeichen — Länge `fmt_len_num` (Dokumenteinheit, Nachkommastellen wie im Parameterfeld), Winkel und Zahl `format_number(v, 2)`; Unbekanntes bleibt wörtlich und steht in der zweiten Liste (**Annahme**: keine Maskierung von `{`).
  - `pub fn rename_refs(content: &str, old: &str, new: &str) -> String` (ganze Namen, Groß/Klein wie `same_name`)
  - `pub fn apply_shown(sk: &mut Sketch, scope: &ParamScope) -> bool` (setzt `shown` je Text: `Some(ersetzt)` bei Verweisen, sonst `None`; `true` bei Änderung)
  - `TextData.shown: Option<String>` (nur zur Laufzeit, nie in der Datei); `Sketch::set_text_shown(&mut self, t: TextId, shown: Option<String>) -> bool`
  - Warnung `feature-sketch-text-param-unknown` („Unbekannter Parameter im Text: {names}“) am Skizzen-Feature, Status bleibt Ok.

- [ ] **Schritt 1: Tests schreiben:**
  `text_refs.rs`:
  ```rust
  #[test] fn breite_mm_text()                     // Spec „Gelungen“: „{Breite} mm“, Breite = 20 → „20,00 mm“ (de) bzw. „20.00 mm“ (en)
  #[test] fn inch_unit_shows_inch_value()         // with_unit(Zoll): Breite = 25,4 mm → „1,000“
  #[test] fn unknown_name_stays_literal()
  #[test] fn rename_updates_text()                // Parameter umbenennen: Inhalt „{Neu} mm“, Format 22 bleibt
  #[test] fn deleted_parameter_leaves_literal_and_warning() // Prüf-Schwerpunkt 4: replace_parameter_by_value(Breite): reshape → „{Breite} mm“ gezeigt, Warnung, Status Ok, Extrusion aus dem Text rechnet weiter
  #[test] fn shown_never_written()                // Speichern: Datei hat Rohinhalt „{Breite} mm“
  ```
  `sketch_text_params.rs`: `parameter_change_reshapes_text` (Parameter ändern → nach dem nächsten Bild sind die Kurven neu, kein zusätzlicher Rückgängig-Schritt), `undo_parameter_change_reshapes_back`, `open_sketch_shows_substituted`.
- [ ] **Schritt 2:** `cargo test -p planora-feature --test text_refs` → FAIL.
- [ ] **Schritt 3:** Umsetzen; Annahmen (kein Einheitenzeichen, keine Maskierung, Ersetzen außerhalb der Neuberechnung) in `offene-punkte.md`.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-sketch`, `cargo test -p planora-feature`, `cargo test -p planora-app --test sketch_text_params --test sketch_text_dependents`, `cargo test -p planora-project --test format22 --test format18_text`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 11: Parameter im Text`. Vault-Notiz „Task 11 Parameter im Text“.

### Task 12: Drehgriff am Textrahmen

**Dateien:**
- Modify: `crates/planora-app/src/sketch_grips.rs` (`Grip::TextRotate(TextId)`, Lage, Ziehen), `sketch_view.rs` (Griff zeichnen und treffen), `undo_labels.rs`, `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_text_rotate.rs` (neu)

**Schnittstellen:**
- Consumes: `Sketch::text_angle` (`text.rs:1278`), `Sketch::set_text_angle` (`text.rs:1302`, dreht um Ecke 0 = Bezugspunkt); `begin_grip`/`grip_to`/`end_grip`/`abort_grip` (`sketch_grips.rs:745-869`).
- Produces:
  - Griff nur im Auswahlwerkzeug bei genau einem gewählten Text im Rahmen in der Skizzenebene (`normal == None`); kein Griff an Text auf Pfad und an 3D-Text (**Annahme**).
  - Lage: über der Mitte der oberen Rahmenlinie (Linie 2), 20 px nach außen senkrecht zur Grundlinie.
  - Ziehen: Winkel = Startwinkel + (Zeigerwinkel um Ecke 0 jetzt − beim Anfassen); ein Schritt „Text drehen“ (`undo-text-rotate`) beim Loslassen; Esc oder Rechtsklick währenddessen stellt den Startwinkel her, kein Schritt. Abgelehnt (`sketch-text-angle-held`): Meldung, Text bleibt.

- [ ] **Schritt 1: Tests schreiben** (`sketch_text_rotate.rs`):
  ```rust
  #[test] fn rotate_grip_turns_about_corner0()   // Zeiger um 90° um Ecke 0 ziehen: text_angle == π/2 (1e-9), Ecke 0 unverändert
  #[test] fn rotate_is_one_undo_step()
  #[test] fn rotate_esc_restores()
  #[test] fn no_grip_for_path_or_3d_text()
  #[test] fn held_angle_refused_with_message()   // Horizontal auf der Grundlinie: Meldung, Winkel 0
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_text_rotate` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-app --test sketch_text_rotate --test sketch_move_copy`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 12: Drehgriff am Textrahmen`. Vault-Notiz „Task 12 Drehgriff am Text“.

---

## Etappe 5: Drehprofil

### Task 13: Drehprofil rechnen und nachführen

**Dateien:**
- Modify: `crates/planora-feature/src/spun_profile.rs` (Rechnung), `include.rs` (`source_pieces`: `SpunProfile` statt `Err(Lost)`), `planes.rs` (`resolve_axis` `:241` als `pub(crate)`), `surface_project.rs` (`fit` `:1418` als `pub(crate)`), `de.ftl`/`en.ftl`
- Test: `crates/planora-feature/tests/spun_profile.rs` (neu)

**Schnittstellen:**
- Consumes: Task 7 (`SpunSpec`, `SpunSelection`, `SpunAxis`); `DocView` (`surface_project.rs:300`); `section_by_plane` (`planora-kernel/src/boolean/section.rs:151`); `resolve_axis`; `Included` (`planora-sketch/src/helpers.rs:93`); `Surface` (`planora-kernel/src/geometry.rs:197`).
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SpunError { AxisNotInPlane, NotRevolved, Lost, Nothing }` mit `pub fn message(self) -> String` (feste Texte der Spec für die ersten beiden; `Nothing` = `sketch-spun-nothing`)
  - `pub fn spun_profile(view: &DocView<'_>, spec: &SpunSpec, own_axis: Option<(DVec3, DVec3)>, tol: &Tolerance) -> Result<Vec<Included>, SpunError>` (Ergebnis in Skizzenkoordinaten, alle z = 0; `own_axis`: Punkt und Richtung einer `SpunAxis::Own` in Weltkoordinaten aus der gespeicherten Skizze)
  - Achse (Punkt `a`, Einheitsrichtung `d`): `|d · frame.z| > 1e-9` → `AxisNotInPlane`. `a'` = `a` senkrecht in die Skizzenebene gelegt, `n̂ = frame.z × d` (mit `flip` negiert). Abbildung eines Punkts `p`: `s = (p − a) · d`, `r = |(p − a) − s d|`, Bild `a' + s d + r n̂`.
  - **Bestimmte Elemente** (gehen immer, Spec-Entscheidung 6): Ecke → Punkt; gerade Kante, die mit der Achse in einer Ebene liegt → Linie; Kreiskante koaxial (Mitte auf der Achse, Normale ∥ Achse) → Punkt; jede andere Kante (auch eine windschiefe Gerade, deren Bild ein Hyperbelast ist) → abgetastet, abgebildet, `fit` mit `spec.tolerance`, als fester Spline.
  - **Körper:** jede Fläche muss eine Ebene senkrecht zur Achse oder eine Zylinder-, Kegel-, Kugel- oder Torusfläche koaxial zur Achse sein (Rahmen-z ∥ Achse, Ursprung auf der Achse; Kugel: Mitte auf der Achse; Toleranz 1e-9 relativ zur Körpergröße), sonst `NotRevolved`, nichts entsteht. Dann: Meridianebene durch die Achse mit den Richtungen `d` und `n̂` (`section_by_plane`), Stücke auf `r ≥ 0` beschnitten, zu Schleifen verkettet; Stücke auf der Achse selbst fallen weg (**Annahme**); Schleifen, die ganz in einer anderen liegen (innere Hohlräume), fallen weg („äußerer Umriss“); das Übrige abgebildet wie oben (Linien bleiben Linien, Bögen Bögen).
  - `source_pieces` für `CopySource::SpunProfile`: Stücke neu wie bei `OnSurface` (Gruppe, Stückzahl); `Lost` → verlorener Bezug (Warnung, Kurven bleiben), `Nothing` → null Stücke, `NotRevolved`/`AxisNotInPlane` nach einer Änderung der Quelle → `Unfollowed::Lost` (Warnung, kein Fehler).

- [ ] **Schritt 1: Tests schreiben** (`spun_profile.rs`):
  ```rust
  #[test] fn stepped_shaft_outline()            // Spec „Gelungen“: Welle aus zwei Zylindern (R 10 L 30, R 5 L 20) um Z, Skizze auf XZ, Achse Z: Linien r = 10 und r = 5, drei Stirnlinien, keine Linie auf der Achse
  #[test] fn tube_has_inner_line()              // Rohr R 10 / r 6: Rechteck aus vier Linien bei r 6…10
  #[test] fn sphere_gives_half_circle_arc()
  #[test] fn closed_cavity_dropped()            // Hohlkugel innen: nur äußerer Bogen
  #[test] fn flip_puts_result_other_side()
  #[test] fn axis_off_plane_refused()           // Skizze auf XY, Achse Z: AxisNotInPlane, Meldung = Spec-Text
  #[test] fn axis_parallel_offset_ok()          // Achse parallel zur Ebene in 5 mm Abstand: Bild liegt in der Ebene (z = 0)
  #[test] fn box_not_revolved()                 // Quader um Z: NotRevolved, Meldung = Spec-Text
  #[test] fn coaxial_circle_edge_is_point()
  #[test] fn skew_line_becomes_spline_within_tolerance() // windschiefe Kante: Spline, Abstand zu 50 abgebildeten Punkten ≤ tolerance
  #[test] fn shaft_change_follows()             // Radius 10 → 12 nach Neuberechnung: Linie bei r = 12
  #[test] fn deleted_body_is_warning()          // Körper löschen: Warnung, lost_projections() nicht leer, Kurven stehen, Status Ok
  ```
- [ ] **Schritt 2:** `cargo test -p planora-feature --test spun_profile` → FAIL.
- [ ] **Schritt 3:** Umsetzen; Abweichungen („nur Drehkörper“, „Stücke auf der Achse fehlen“, „windschiefe Gerade als Spline“) in `offene-punkte.md`.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-feature` (besonders `weak_refs`, `lost_projections`, `surface_project`, `include_3d`).
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 13: Drehprofil rechnen und nachführen`. Vault-Notiz „Task 13 Drehprofil Rechnung“.

### Task 14: Drehprofil in der App

**Dateien:**
- Create: `crates/planora-app/src/spun_profile_dialog.rs`; Modify: `sketch_kinds.rs` (`ToolKind::SpunProfile` in der Gruppe Projizieren/Einbeziehen, Hinweis), `keymap/defaults.rs`, `commands.rs`, `state.rs` (`add_spun_profile`), `icons.rs` (eigenes Handsymbol), `detach.rs`, `app.rs` (Menü Skizze → Erstellen → Projizieren/Einbeziehen), `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_spun_profile.rs` (neu)

**Schnittstellen:**
- Consumes: Task 13 (`spun_profile`, `SpunError`); Task 7 (`SpunSpec`, `SpunSelection`, `SpunAxis`); `IncludePick` (`state.rs:2547`; Fläche angeklickt im Filter „Körper“ = deren Körper); Weg „Projektion anlegen“ (`state.rs:4370ff`, `include_pieces`).
- Produces:
  - `pub struct SpunProfileDialog { pub filter: SpunFilter, pub picks: Vec<IncludePick>, pub axis: Option<SpunAxis>, pub project_axis: bool, pub tolerance: f64, pub flip: bool, pub linked: bool }` mit `pub enum SpunFilter { Bodies, Elements }`; Vorgaben `Bodies`, `project_axis: false`, `tolerance: 0.01` mm (**Annahme**), `flip: false`, `linked: true`; `.dock_key("sketch_spun_profile")`; Vorschau live aus `spun_profile`, Fehlertext im Dialog.
  - `AppState::add_spun_profile(&mut self, spec: SpunSpec, project_axis: bool, linked: bool) -> Result<usize, Msg>`: ein Schritt „Drehprofil“ (`undo-spun-profile`). `linked`: Kurven violett, Einträge `CopySource::SpunProfile` in `copies` (eine Gruppe); sonst freie Kurven ohne `Fixed` und ohne `copies`-Eintrag. `project_axis`: Kante → `CopySource::Include(IncludeRef::Edge)`, Ursprungsachse → `Include(IncludeRef::Axis)`, Linie einer früheren Skizze → `Include(IncludeRef::SketchCurve)`, eigene Linie → nichts (liegt schon da). Abbruch hinterlässt nichts.
  - „Verlorene Projektionen verwalten“ bietet für Drehprofil „Verknüpfung aufheben“ und „Geometrie löschen“, nicht „Neu verknüpfen“ (`relink_pickable` bleibt `false`, wie „Auf Fläche projizieren“; Abweichung in `offene-punkte.md`).

- [ ] **Schritt 1: Tests schreiben** (`sketch_spun_profile.rs`):
  ```rust
  #[test] fn shaft_profile_one_step()           // Welle, Achse Z, OK: Kurven in der Skizze, ein Schritt „Drehprofil“, copies mit SpunProfile
  #[test] fn unlinked_gives_free_curves()       // linked false: keine copies, kein Fixed
  #[test] fn project_axis_adds_axis_line()      // Ursprungsachse Z mit Häkchen: zusätzlich Include(Axis)
  #[test] fn cancel_leaves_nothing()            // Esc im Dialog: Skizze und Verlauf unverändert
  #[test] fn box_shows_message_and_ok_disabled()
  #[test] fn lost_body_offers_unlink_not_relink()
  #[test] fn dialog_has_dock_key()
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_spun_profile` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-app --test sketch_spun_profile --test lost_projections_dialog --test dialog_dock_keys --test keymap`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 14: Drehprofil in der App`. Vault-Notiz „Task 14 Drehprofil App“.

---

## Etappe 6: Isoparametrische Kurve

### Task 15: Isokurven rechnen und nachführen

**Dateien:**
- Modify: `crates/planora-math/src/nurbs/surface.rs` (`iso_curve_u`, `iso_curve_v`), `crates/planora-feature/src/isoparametric.rs` (Rechnung), `include.rs` (`source_pieces`: `Isoparametric`)
- Test: `crates/planora-math/tests/nurbs_iso.rs` (neu), `crates/planora-feature/tests/isoparametric.rs` (neu)

**Schnittstellen:**
- Consumes: Task 7 (`IsoDirection`, `CopySource::Isoparametric`); `resolve_face` (`refs.rs:148`); `FaceGeom`, `face_polys`, `point_in_face_at` (`planora-kernel/src/boolean/uv.rs:298-330`); `Surface::point`, `project`, `u_period`, `v_period` (`geometry.rs`); `NurbsSurface::insert_knot_u/_v` (`surface.rs:325/350`); `ProjectError` (`surface_project.rs:266`).
- Produces:
  - `NurbsSurface::iso_curve_u(&self, u: f64) -> Result<NurbsCurve, NurbsError>` (Kurve `u = const` über v, exakt: Knoten auf volle Vielfachheit, Kontrollzeile mit Gewichten) und `iso_curve_v`.
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct IsoRange { pub u: (f64, f64), pub v: (f64, f64) }`; `pub fn iso_range(face: &FaceGeom) -> IsoRange`: Hüllrechteck von `face_polys` in (u, v); ist die Fläche in einer Richtung periodisch und deckt die Fläche den ganzen Umlauf (Ausdehnung ≥ Periode − Toleranz), gilt der volle Umlauf ab der Naht (Spec-Entscheidung 4).
  - `pub fn iso_value_at(face: &FaceGeom, p: DVec3, dir: IsoDirection) -> f64` (Anteil 0–1 des Punkts)
  - `pub fn iso_curves(view: &DocView<'_>, face: &FaceRef, dir: IsoDirection, value: f64, tol: &Tolerance, budget: Duration) -> Result<Vec<Included>, ProjectError>`: Parameterlinie bei `lo + value · (hi − lo)` über den ganzen Bereich der anderen Richtung, geschnitten mit den Randpolygonen (mit Periodenverschiebung), Abschnitte mit Mittelpunkt `Inside` werden Stücke, in Parameterrichtung geordnet. Ausgabe exakt: Ebene → Linie; Zylinder/Kegel `u = const` → Linie, `v = const` → Bogen/Kreis; Kugel beide → Bogen; Torus beide → Bogen/Kreis; NURBS → fester Spline aus `iso_curve_*`, auf das Stück beschnitten. Linien mit Länge ≤ Toleranz entfallen. Fläche weg → `Lost`; kein Stück → `NoHit`.
  - `source_pieces` für `Isoparametric`: wie `OnSurface` (Gruppe, Zeitgrenze, `NoHit` = null Stücke).

- [ ] **Schritt 1: Tests schreiben:**
  `nurbs_iso.rs`: `iso_curve_matches_surface_points` (bikubische Fläche, rational: 20 Punkte der Kurve = `point(u, v)` auf 1e-12), `iso_at_knot_and_end`.
  `isoparametric.rs`:
  ```rust
  #[test] fn cylinder_u_is_line_v_is_circle()
  #[test] fn plane_face_with_hole()               // Rechteckfläche mit rundem Loch: v-Linie durch die Mitte gibt zwei Linien
  #[test] fn cylinder_with_hole_splits_iso_line()  // Prüf-Schwerpunkt 5: Zylindermantel mit Querbohrung, v-Kreis durch die Bohrung: zwei Bögen, keiner über die Naht falsch getrennt (Summe der Winkel = 2π − Bohrungswinkel)
  #[test] fn sphere_and_torus_arcs()
  #[test] fn nurbs_face_gives_fixed_spline()
  #[test] fn value_is_fraction_of_range()         // value 0.25 auf Teilzylinder 0…90°: Linie bei 22,5°
  #[test] fn follows_face_change()                // Zylinder höher: Linie länger, an derselben verhältnismäßigen Stelle
  #[test] fn lost_face_is_warning()               // Körper löschen: Warnung, Kurve bleibt, Status Ok
  ```
- [ ] **Schritt 2:** `cargo test -p planora-feature --test isoparametric` → FAIL.
- [ ] **Schritt 3:** Umsetzen; Annahmen (U = `u = const`, „physisch“ nicht gebaut, Stückelung an Nähten) in `offene-punkte.md`.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-math`, `cargo test -p planora-kernel`, `cargo test -p planora-feature`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 15: Isokurven rechnen und nachführen`. Vault-Notiz „Task 15 Isokurve Rechnung“.

### Task 16: Isoparametrische Kurve in der App

**Dateien:**
- Create: `crates/planora-app/src/isoparametric_dialog.rs`; Modify: `sketch_kinds.rs` (`ToolKind::Isoparametric`), `keymap/defaults.rs`, `commands.rs`, `state.rs` (`add_isoparametric`, `relink_pickable` → `true` für `Isoparametric`, Neu verknüpfen über Flächenwahl), `sketch_view.rs` (Vorschau), `lost_projections_dialog.rs`, `icons.rs`, `detach.rs`, `app.rs` (Menü), `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_isoparametric.rs` (neu)

**Schnittstellen:**
- Consumes: Task 15 (`iso_curves`, `iso_range`, `iso_value_at`); `pick_face_under` (`state.rs:2495`); `relink_copies` (`ops.rs:606`); Weg „Auf Fläche projizieren“ für das Umschalten auf 3D-Skizze.
- Produces:
  - `pub enum IsoDirections { U, V, Both }`, `pub enum IsoMode { Single, Distributed }`; `pub struct IsoDialog { pub face: Option<FaceRef>, pub directions: IsoDirections, pub mode: IsoMode, pub distance: f64, pub count: u32, pub linked: bool }`; Vorgaben `U`, `Single`, `distance 0.5`, `count 3` (1–50, **Annahme**), `linked true`; Punkt auf der Fläche setzt `distance` über `iso_value_at`; `.dock_key("sketch_isoparametric")`; Vorschau live.
  - `pub fn iso_values(mode: IsoMode, distance: f64, count: u32) -> Vec<f64>`: Einzeln `[distance]`, Verteilt `k / (count + 1)` für `k = 1..=count`.
  - `AppState::add_isoparametric(&mut self, face: FaceRef, dirs: &[IsoDirection], values: &[f64], linked: bool) -> Result<usize, Msg>`: ein Schritt „Isoparametrische Kurve“ (`undo-isoparametric`); Skizze wird 3D-Skizze; je (Richtung, Wert) eine Gruppe `CopySource::Isoparametric`; `linked` aus → freie Kurven. Abbruch hinterlässt nichts.
  - Neu verknüpfen: Fläche wählen, Richtung und Wert bleiben; ein Schritt „Neu verknüpfen“.

- [ ] **Schritt 1: Tests schreiben** (`sketch_isoparametric.rs`):
  ```rust
  #[test] fn iso_values_distributed_without_edges() // count 3 → [0.25, 0.5, 0.75]
  #[test] fn u_and_v_on_dome_one_step()             // Spec „Gelungen“: Kugelkappe, Both, Verteilt 3: 6 Gruppen, ein Schritt, Skizze 3D
  #[test] fn point_sets_distance()
  #[test] fn unlinked_gives_free_curves()
  #[test] fn cancel_leaves_nothing()
  #[test] fn relink_to_other_face()                 // Fläche verloren, Neu verknüpfen auf andere Zylinderfläche: Kurven folgen, Warnung weg
  #[test] fn dialog_has_dock_key()
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_isoparametric` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-app --test sketch_isoparametric --test lost_projections_dialog --test dialog_dock_keys --test keymap`.
- [ ] **Schritt 5:** Commit `Skizzen 11b Task 16: Isoparametrische Kurve in der App`. Vault-Notiz „Task 16 Isokurve App“.

### Handtest Etappe 4–6 (kein Code, Handtest nur nach Dennis' Go)

- [ ] Der Steuerer fragt Dennis und wartet auf das Go. Build und Start wie Handtest 1.
- [ ] Liste: Texthöhe „H/2“ und Parameter H ändern; Text „{Breite} mm“, Breite ändern, umbenennen, durch Wert ersetzen (Warnung); Drehgriff ziehen, Esc. Drehprofil einer abgesetzten Welle (Körper), einer Kante (Bestimmte Elemente), Umkehren, Achse projizieren, ohne Verknüpfung, Quader (Meldung); Welle ändern und löschen (Warnung, „Verlorene Projektionen verwalten“). Isokurve auf Zylinder, Kugel, NURBS-Fläche, Einzeln über Punkt, Verteilt mit U und V, Fläche ändern, Körper löschen und neu verknüpfen. Speichern, laden, Format 22 in einer älteren Version öffnen (Meldung „ab Format 22“).
- [ ] Ergebnis als Vault-Notiz „Skizzen 11b Handtest 2“; Funde als eigene Korrekturaufgaben; Test-Builds und Fotos im Scratchpad löschen.

---

## Etappe 7: Abschluss

### Task 17: Doku, offene Punkte, Gesamtprüfung

**Dateien:**
- Modify: `docs/funktionen.md` (Kopieren/Einfügen, Skizze als DXF, Skizzengruppen, Texthöhe/Parameter im Text/Drehgriff, Drehprofil, Isoparametrische Kurve; Plattformen mit Ubuntu, wo welche genannt sind), `CHANGELOG.md` (Abschnitt „Unveröffentlicht“), `docs/superpowers/offene-punkte.md` (alle Annahmen und Abweichungen aus Task 1–16 gesammelt, Abschnitt „Skizzen 11b“), Wiki-Vermerk für das erste Release (Projekttafel/Wiki: neue Funktionen nachtragen, Screenshots dort, nicht jetzt)
- Vault: `Projekte/PlanoraCAD/Gegenüberstellung Fusion.md` (Zeilen 35, 51, 55, 56, 70, 325 auf „gebaut“ bzw. Abweichung), `Roadmap.md`, Übergabe in `Projekte/PlanoraCAD/Sessions/`

- [ ] **Schritt 1:** `offene-punkte.md` gegen die Spec prüfen: jede **Annahme** der Spec und jede Abweichung aus den Aufgaben steht drin (Liste der Stichworte in der Vault-Notiz).
- [ ] **Schritt 2:** Doku und CHANGELOG schreiben (Laiensprache, Namen wie in Fusion).
- [ ] **Schritt 3:** Gesamtprüfung Windows, nacheinander: `cargo fmt -p <crate> -- --check` für alle Crates, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, Messtests im Release (`cargo test --release -p planora-sketch --test three_d_perf -- --ignored`, `import_perf`): nicht langsamer als im Ledger von 11a (Abschluss-Zeile). Bekannt: `real_step_text_limit` scheitert auch auf `master`.
- [ ] **Schritt 4:** WSL (Ubuntu): erst prüfen, dass kein Lauf aktiv ist (`pgrep -af 'cargo|rustc|wsl-s6'`), dann `~/wsl-s6-full.sh`; bekannt scheitert dort als root `a_locked_old_json_never_ends_up_next_to_a_new_png`. Danach `/root/target-s6` und `/root/PlanoraCAD/target` löschen, `df -h /`.
- [ ] **Schritt 5:** `graphify . --update --code-only`.
- [ ] **Schritt 6:** Commit `Skizzen 11b Task 17: Doku, offene Punkte, Gesamtprüfung`. Vault-Notiz „Task 17 Abschluss“, Übergabe in `Sessions/`, Roadmap.

### Handtest Ende (kein Code, Handtest nur nach Dennis' Go)

- [ ] Der Steuerer fragt Dennis und wartet auf das Go.
- [ ] Alle Punkte aus Handtest 1 und 2 zusammen in einem Dokument; eine alte Datei (Format ≤ 21) laden und ohne Änderung speichern (bytegleich); eine Datei mit Gruppe in einer älteren Version öffnen (Meldung „ab Format 22“).
- [ ] Ergebnis als Vault-Notiz „Skizzen 11b Handtest Ende“; Funde als eigene Korrekturaufgaben; Test-Builds und Fotos im Scratchpad löschen. Merge nur auf „bitte mergen“.
