# Projekte-Fenster und Einstellungen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **COMMIT-TRAILER (jeder Commit, ohne Ausnahme, wörtlich als letzte Zeile):**
> `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
> Vor jedem `git commit` die Zeile Zeichen für Zeichen prüfen. Immer nur die genannten Pfade einzeln `git add`en (nie `git add -A`/`.`/`-u`).
> **Nie `git checkout -- …`, `git stash`, `git reset` oder `git restore`** (auch nicht zum Zurücknehmen von Mutationen). Mutationen nur mit dem Mutationsskript (Global Constraints), das mit Prüfsumme zurückstellt.

**Goal:** Das Feld „Projekte“ links weicht einem eigenen, nicht modalen Fenster „Projekte“ mit Kacheln (Miniatur der Körper, „Geändert …“, „Version n“), und das Projektverzeichnis wird im neuen Fenster „Einstellungen“ über die Dateiauswahl im Ordner-Modus gewählt.

**Architecture:** Alles in `planora-app`. Reine Bausteine zuerst: Ordner-Modus der `FileChooser`, `change_root` mit Sperre, ein Kachelmodell über `Library::tree` + `docstore::peek` (Hintergrund-`Job`), ein eigener Software-Rasterer (Dreiecke → RGBA, z-Puffer, flach schattiert) mit Datei-Zwischenspeicher `vorschau.png`/`vorschau.json` im Dokumentordner und genau ein Hintergrund-Arbeiter für Miniaturen. Darauf die Fenster `ProjectsWindow` und `SettingsWindow` (beide über `dialog::Dialog`, Größe `DialogSize::File`), die bisherigen Aktionen und Rückfragen des Baumfelds wandern unverändert nach `project_actions.rs`; zuletzt Menü, Strg+O, Schließregeln und Entfernen des Felds.

**Tech Stack:** Rust-Workspace, egui/eframe 0.36, `png` 0.18 (schon direkte Abhängigkeit der App), `planora-mesh::tessellate`, `planora-feature::Document::from_snapshot`, `planora-project::{library, docstore, config, fsutil}`; Tests im eigenen Harness ohne Fenster (Vorbilder `crates/planora-app/tests/{file_places_layout.rs, file_dialog_size.rs, file_dialog_buttons.rs, dialogs.rs, projects_ui.rs, step_state.rs, shell_app.rs}` und `tests/common/mod.rs`).

**Spec:** `docs/superpowers/specs/2026-10-23-projekte-fenster-design.md` (freigegeben samt §9a und §10). Basis: Branch `feature/projekte-fenster` @ `0bd0743`.

## Global Constraints

- **Texte** deutsch, Laiensprache, keine Markennamen, keine Hinweise auf Datenherkunft. Das Wort „Wurzel“ erscheint in keinem Oberflächentext (Code, Kommentare und Konfigurationsschlüssel `root` bleiben). Feste Texte (wörtlich): Menü „Projekte …“ (Strg+O, ersetzt „Öffnen …“), „Einstellungen …“; Fenster „Projekte“, „Einstellungen“; Knöpfe „Neues Projekt“, „Neues Dokument“, „Neuer Ordner“, „Projektverzeichnis …“, „Projektverzeichnis ändern …“, „Diesen Ordner wählen“, „Abbrechen“, „Schließen“, „Aktualisieren“, „Anlegen“; Kontextmenü „Umbenennen …“, „Verschieben …“ (nicht für Projekte), „Löschen …“; Leer: „Noch keine Projekte. Lege mit „Neues Projekt“ das erste an.“; Hinweis Einstellungen: „Ihre Projekte bleiben, wo sie sind. Im neuen Verzeichnis erscheinen nur die Projekte, die dort liegen.“; Kacheltexte und Zählungen siehe Aufgabe 3. Meldungen der Bibliothek und von `choose_root` bleiben wörtlich.
- **Nur `crates/planora-app/**` ändert sich** (Quellen, Tests). Ohne Diff gegen `0bd0743`: alle anderen Crates (`planora-project`, `-feature`, `-kernel`, `-mesh`, …). **Ausnahme (Entscheidung von Dennis, 2026-10-23: Ortszeit, Variante B):** Aufgabe 0 darf genau eine kleine, geprüfte Zeit-Abhängigkeit in `Cargo.toml` (Workspace und App) und `Cargo.lock` aufnehmen (`deny.toml` nur, wenn die Lizenzprüfung es zwingend verlangt: dann anhalten und fragen); danach ohne Diff. Sonst keine neue Abhängigkeit, kein `unsafe` in eigenem Code.
- **Dateiformat unberührt:** `dokument.json`, `versionen/`, `projekt.json`, `config.json` (nur `root` wird beim Wechsel geschrieben, über `Workspace::persist_config`). Golden-Tests in `crates/planora-project/tests` laufen unverändert.
- **Farben** nur aus `theme.rs` (`theme::palette(dark)`, `viewport::render_palette`; Wächter `tests/no_hard_coded_colors.rs`), Fenster nur über `dialog::Dialog` (`no_raw_windows.rs`), Textfelder nur über `theme::text_field*` (`no_raw_text_edits.rs`), Symbole nur aus Phosphor-Light mit Codepunkt aus `crates/planora-app/assets/fonts/Phosphor-Light-style.css` (`tests/icons.rs`). Kacheln einer Reihe gleich groß und gleich hoch; Zeilen über die ganze Fensterbreite; Hell und Dunkel.
- **Tests berühren nie** `%APPDATA%` oder den Ordner Dokumente des Nutzers: Jede `Workspace`, die `persist_config`/`save_places` erreichen kann, bekommt `ws.config_path = <temp>/config.json`; Bibliotheken nur unter `common::temp_root(…)`; Miniaturen nur in Temp-Bibliotheken. Ordner am Testende entfernen.
- **Keine Fenster starten:** Agents starten weder `planora-app` noch ein anderes GUI, fassen die laufende App des Nutzers nicht an und bauen nie `--release`. Prüfungen nur auf Zustands- und Layout-Ebene (egui-Kontext ohne Fenster, Rohereignisse je Bild; Tippen mit echten Tasten, ein Zeichen je Bild; Kennungen nie vom Text abhängig).
- **Nach jeder Aufgabe:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -j 2 -- -D warnings`, betroffene Tests mit `-j 2` im Vordergrund (nie `cargo clean`, keine Hintergrundprozesse zurücklassen).
- **Mutationsbeweise (≥ 10 je Aufgabe, wo sinnvoll):** Ein Skript außerhalb des Repos (`%TEMP%\planora-mutate.ps1`, einmal vom Ausführenden angelegt) nimmt `-File`, `-Find`, `-Replace`, `-Test`: liest die Datei als Bytes, merkt SHA-256, verlangt **genau einen** Treffer von `-Find` (sonst Abbruch ohne Änderung), schreibt die Mutation, führt `-Test` aus, schreibt die Originalbytes zurück und bricht laut ab, wenn die SHA-256 nicht wieder gleich ist. Die Arbeitskopie hat CRLF-Zeilenenden (`core.autocrlf=true`): Muster nur einzeilig, nie `\n` darin. Bericht je Aufgabe „Mutation → roter Test“; bleibt eine grün, wird der Test schärfer. Am Ende `git diff --stat` ohne Reste.
- **Keine Streudateien:** nichts im Wurzelordner des Repos; `git status` nach jedem Commit sauber.
- **Bestehende Tests bleiben unverändert grün**, außer den in den Aufgaben 7 und 9 genannten Anpassungen (vollständige Liste dort).

**Festlegungen des Plans (P1–P12; der Nutzer kann sie kippen):**
- **P1 Zeitangaben in Ortszeit (Entscheidung von Dennis: B, 2026-10-23):** `VersionInfo.time` ist UTC-Text „JJJJ-MM-TTThh:mm:ssZ“ (`fsutil::now_iso`). Die Oberfläche zeigt Ortszeit des Rechners (mit Sommerzeit) als „23.10.2026, 18:42“, auf den Kacheln und im Reiter „Versionen“ (Aufgabe 0: kleine geprüfte Zeit-Abhängigkeit, `local_time.rs`). Gespeichert wird weiter UTC, am Dateiformat ändert sich nichts. Lässt sich die Ortszeit nicht ermitteln, zeigt die Oberfläche UTC mit dem Zusatz „UTC“.
- **P2 Modulnamen:** `projects_panel.rs` wird aufgeteilt: Aktionen, `ProjectsUi`, die vier vorhandenen Fenster und zwei neue („Neues Projekt“, „Neues Dokument“) nach `project_actions.rs` (Aufgabe 7); das Feld selbst verschwindet in Aufgabe 9.
- **P3 Kachelzählung rekursiv:** Projekt- und Ordnerkacheln zählen alle Dokumente darunter (auch in Unterordnern), unlesbare eingeschlossen; Datum und Version kommen vom zuletzt geänderten **lesbaren** Dokument darunter (größtes `latest_time()`, bei Gleichstand kleinerer Name ohne Groß-/Kleinschreibung, dann kleinerer Pfad).
- **P4 500 Kacheln je Ebene:** Eine Ebene zeigt höchstens `MAX_TILES = 500` Kacheln (Hinweis „Es werden nur die ersten 500 gezeigt.“). Gelesen (`peek`) werden alle Dokumente, im Hintergrund und abbrechbar.
- **P5 Feste Kachelmaße:** `TILE_MIN_W = 240`, `TILE_H = 220`, Abstand `TILE_GAP = theme::SPACE_3`; Spaltenzahl `columns_for(innen) = max(1, ⌊(innen + GAP) / (TILE_MIN_W + GAP)⌋)`, die Kacheln teilen die Zeile gleichmäßig. Bildfläche je Kachel 140 px hoch, Miniatur 248 × 140 eingepasst. Der Kachelbereich ist `grid_height_for(fensterhöhe) = fensterhöhe − PROJECTS_CHROME_H` (`= 260`) hoch und rollt selbst; das Fenster springt dadurch nie, wenn Kacheln oder Miniaturen eintreffen.
- **P6 Miniatur-Dateien:** `vorschau.png` (496 × 280 RGBA) und `vorschau.json` (`{"v":1,"version":12,"branch":"Haupt","program":"0.1.0","dark":true,"w":496,"h":280,"art":"bild"}`, `art` ∈ `bild`, `leer`, `zu_aufwendig`). Erst PNG, dann JSON, beide mit `fsutil::atomic_write_unique`; die JSON ist das „fertig“-Zeichen. Gültig nur, wenn Version, Zweig, Programmversion, Hell/Dunkel, Größe und `v` stimmen und (bei `bild`) das PNG sich zu genau 496 × 280 lesen lässt.
- **P7 Blick und Licht:** Gleiche Drehung wie `planora_render::camera::OrbitCamera::default()` (yaw, pitch dort ablesen, nicht abschreiben), **parallele** Projektion, Rand `MARGIN = 0.08` der kürzeren Bildseite, 2 × 2 Unterabtastung, beidseitige Flachschattierung `hell = 0.35 + 0.65·|n·l|` mit Licht aus Blickrichtung leicht von links oben (`l` im Kameraraum `(-0.3, 0.5, 1.0)` normiert), Sehne `THUMB_CHORD_MM = 0.1`.
- **P8 Grenzen der Miniatur:** `MAX_TRIANGLES = 200_000` (Summe aller Körper; Netz-Körper vor dem Platzieren gezählt), `MAX_BUILD = 3 s` (gemessen nach `from_snapshot` und nach jedem Körper). Überschritten → `zu_aufwendig` speichern. Lese- oder Aufbaufehler, auch eine Panik (`catch_unwind`) → nur im Speicher „fehlgeschlagen“ für diese Version (kein Speichern, kein erneuter Versuch in dieser Sitzung).
- **P9 Ein Arbeiter je Programmlauf:** Der Miniatur-Arbeiter gehört dem `ProjectsWindow` und lebt mit der App; Schließen des Fensters leert nur die Warteschlange. Der laufende Aufbau speichert selbst (im Arbeiter), auch wenn das Fenster schon zu ist.
- **P10 Fenster schließen einander:** Projekte, Einstellungen und die Dateifenster des aktiven Tabs (Exportieren, Netz importieren, STEP importieren) sind nie gleichzeitig offen. Öffnen von Projekte/Einstellungen schließt die Dateifenster des Tabs (ein schreibendes Exportfenster nie; dann öffnet Projekte/Einstellungen nicht). Ist ein Dateifenster des aktiven Tabs offen (neu geöffnet oder nach Tabwechsel), schließen Projekte und Einstellungen (Dateifenster haben Vorrang). „Projektverzeichnis …“ schließt Projekte und öffnet Einstellungen mit Rückweg: Schließen der Einstellungen öffnet dann wieder Projekte.
- **P11 Menüsymbol:** „Projekte …“ bekommt `Icon::Projects` (Aktentasche, wie der Schnellzugriff „Projekte“), „Einstellungen …“ das neue `Icon::Settings` (Phosphor `gear-six`, U+E272). `Icon::Open` und `Icon::PanelProjects` entfallen.
- **P12 Meldung ohne Projektverzeichnis:** Der alte Text verwies auf ein Feld „unten“, das es nicht mehr gibt. Neu: „Es ist noch kein Projektverzeichnis gewählt. Wähle mit „Projektverzeichnis …“ den Ordner, in dem deine Projekte liegen sollen.“; Lesefehler bleibt „Der Projektordner lässt sich nicht lesen: {e}“.

## Review Focus

1. **Projektverzeichnis wechseln, während gespeicherte Dokumente offen sind, die Konfiguration unlesbar ist oder abgebrochen wird:** nichts wird verschoben, kopiert oder gelöscht; `config.json` behält Favoriten und fremde Felder, nur `root` ändert sich; bei Sperre/Fehler bleibt das alte Verzeichnis gewählt und die Datei unverändert (Byte für Byte). → Aufgabe 2 Tests `changing_the_directory_with_open_documents_is_refused_and_writes_nothing`, `a_new_directory_keeps_favorites_and_only_changes_root`, `an_unreadable_config_is_not_overwritten_by_favorites_but_root_is_chosen`, `cancel_changes_nothing`.
2. **Beschädigte oder riesige Dokumente bei den Miniaturen** (abgeschnittene Versionsdatei, fehlende Spitzenversion, Feature im Fehlerzustand, Panik im Aufbau, schreibgeschützter Ordner, 300 000 Dreiecke): Platzhalter, kein Absturz, kein Dauerversuch, die Oberfläche wartet nie; außer `vorschau.*` ändert sich keine Datei im Dokumentordner. → Aufgabe 5 Tests `damaged_documents_give_failed_without_panic_and_write_nothing`, `building_never_changes_other_files_in_the_document_folder`, `a_read_only_folder_keeps_the_thumbnail_in_memory_only`, `too_many_triangles_store_too_complex_once`; Aufgabe 6 `a_panicking_build_reports_failed_and_the_worker_goes_on`; Aufgabe 8 `the_frame_never_waits_for_a_slow_thumbnail`.
3. **Falsche „Version“ oder falsches Datum:** aktiver Zweig ist nicht der jüngste, Spitze gelöscht, gleiche Zeiten, unlesbare Dokumente neben lesbaren, Dokument in Unterordner, offenes geändertes Dokument (Kachel zeigt den gespeicherten Stand). → Aufgabe 3 Tests `version_is_the_tip_of_the_active_branch_not_the_newest`, `ties_pick_the_smaller_name`, `unreadable_documents_count_but_do_not_date`, `nested_documents_count_for_project_and_folder`; Aufgabe 5 `an_open_modified_document_shows_the_saved_version`.
4. **Nach dem Wegfall des Felds kein Weg mehr zum Öffnen, Umbenennen, Verschieben, Löschen** (auch per Tastatur, mit gesperrten offenen Dokumenten, nach Wechsel des Verzeichnisses, ohne Projektverzeichnis): alles über das Fenster erreichbar, Strg+O öffnet es, „Speichern unter“ verweist darauf. → Aufgabe 7 Tests `every_panel_action_is_reachable_from_tiles`, `open_documents_lock_rename_move_and_delete`; Aufgabe 8 `tab_reaches_every_tile_and_enter_opens_it`; Aufgabe 9 `ctrl_o_opens_the_projects_window_everywhere`, `after_a_directory_change_the_window_shows_the_new_projects`.
5. **Fenster bei 800 × 600 und mit vielen Projekten:** 200 Kacheln, sehr lange Namen, 0 Kacheln, Hell/Dunkel, 1400 × 900, 2000 × 1100: Fenster ≤ 1200 px und ganz im Bild, Fußzeile sichtbar, alle Kacheln gleich groß, nichts verschiebt sich beim Überfahren oder wenn Miniaturen eintreffen. → Aufgabe 8 Tests `layout_holds_at_all_sizes_with_0_1_and_200_tiles`, `hover_and_arriving_thumbnails_move_nothing`, `long_names_stay_inside_their_tile`.

---

### Task 0: Ortszeit statt UTC (Entscheidung B)

**Files:**
- Modify: `Cargo.toml` (Workspace-Abhängigkeit), `crates/planora-app/Cargo.toml`, `Cargo.lock` (nur die Zeit-Abhängigkeit und ihre eigenen Unterabhängigkeiten), `deny.toml` nur falls `cargo deny check` die Lizenz zwingend nicht zulässt (dann anhalten, Controller fragt Dennis)
- Create: `crates/planora-app/src/local_time.rs`, `crates/planora-app/tests/local_time.rs`
- Modify: `crates/planora-app/src/lib.rs` (`pub mod local_time;`), die Stelle(n), an der der Reiter „Versionen“ die Zeit zeigt (`versions_panel.rs` oder wo `VersionInfo.time` formatiert wird; mit `grep -rn "\.time\|zeit" crates/planora-app/src` finden), dazugehörige bestehende Tests

**Interfaces:**
- Produces:
  - `pub fn format_offset(iso_utc: &str, offset_seconds: i32) -> Option<String>` – rein: „2026-10-23T16:42:07Z“ + 7200 → „23.10.2026, 18:42“ (Tages- und Jahreswechsel, Schaltjahr; ungültiger Text → `None`).
  - `pub fn local_time_text(iso_utc: &str) -> String` – Ortszeit des Rechners zum **angegebenen Zeitpunkt** (Sommer-/Winterzeit des damaligen Datums, nicht des heutigen), Form „dd.mm.yyyy, hh:mm“; lässt sich die Zone nicht ermitteln: UTC-Text mit Zusatz „ UTC“; ungültiger Text: der Eingabetext unverändert.
  - `pub fn local_offset_at(unix_seconds: i64) -> Option<i32>` – dünne Schicht über der gewählten Bibliothek (die einzige Stelle, die sie berührt); Tests prüfen `format_offset` mit festen Abständen und die Schicht mit einer fest gewählten Zone, soweit die Bibliothek das erlaubt (Europe/Berlin: 2026-01-15 → +3600, 2026-07-15 → +7200, Umstellungstage).
- Consumes: nichts aus früheren Aufgaben. Spätere Aufgaben (3, 8) benutzen `local_time::local_time_text` statt eigener Formatierung (`format_changed` in Aufgabe 3 ruft es auf).

- [ ] **Step 1: Bibliothek wählen und prüfen.** Kandidaten `jiff`, `time` (nur Feature `local-offset`), `chrono` (nur `clock`). Bewerten: Lizenz laut `deny.toml` erlaubt, kein `unsafe` im eigenen Code, möglichst wenige Unterabhängigkeiten und kleine Merkmale, korrekte Ortszeit **mit Sommerzeit zu einem beliebigen vergangenen Zeitpunkt** unter Windows (auch ohne Neustart der App bei Zonenwechsel nicht nötig), Version mindestens zwei Wochen alt und exakt festgelegt. `cargo deny check` muss sauber bleiben (die Ausnahmen für zbus-Familie und `ttf-parser` gelten weiter, `deny.toml` bleibt unverändert). Begründung und Messzahlen (Anzahl neuer Pakete, Größenzuwachs) in den Bericht.
- [ ] **Step 2: Tests zuerst** (`tests/local_time.rs`): `format_offset` (Normalfall, +0, negativer Abstand, Tageswechsel vorwärts/rückwärts, Jahreswechsel, 29.02.2028, Sekunden ignoriert, ungültige Texte: leer, ohne `Z`, falscher Monat, `2026-10-23 16:42:07`), Zeitzonenschicht (Berliner Winter/Sommer, Umstellungstage 29.03.2026 und 25.10.2026 jeweils eine Stunde vor/nach der Umstellung, wenn die Bibliothek eine feste Zone erlaubt; sonst die Schicht nur auf „liefert Some und plausibel (|Abstand| ≤ 14 h)“ testen), Rückfall „ UTC“, Versionen-Reiter zeigt Ortszeit (bestehender Test anpassen oder neuen schreiben).
- [ ] **Step 3: Implementieren** (`local_time.rs`, Reiter „Versionen“ benutzt `local_time_text`; alle UTC-Anzeigen der Oberfläche auf diese Funktion umstellen: `grep -rn "now_iso\|\.time\b" crates/planora-app/src` prüfen).
- [ ] **Step 4: Mutationsbeweise (≥ 10):** Abstand mit falschem Vorzeichen, Tageswechsel ohne Übertrag, Schaltjahr falsch, Sekunden statt Minuten, 24-Stunden-Fehler (12/24), Monat um eins versetzt, Sommerzeit des heutigen statt des angegebenen Datums, Rückfall ohne „ UTC“, ungültiger Text liefert leeren Text, Versionen-Reiter nutzt wieder UTC.
- [ ] **Step 5: Gates und Commit** `feat(app): Ortszeit statt UTC in Versionen und Kacheln`: fmt, clippy, `cargo deny check`, `git diff --stat 0bd0743 -- Cargo.toml Cargo.lock deny.toml crates/planora-app/Cargo.toml` zeigt nur die Zeit-Abhängigkeit.

---

### Task 1: Ordner-Modus der Dateiauswahl

**Files:**
- Modify: `crates/planora-app/src/file_chooser.rs` (`ChooserMode`, `FileChooser::{folder, reload, target, confirm, show_body, show_footer}`; `display_path` samt Unit-Test aus `projects_panel.rs` hierher verschoben)
- Modify: `crates/planora-app/src/projects_panel.rs` (`display_path` entfernen, `use crate::file_chooser::display_path`)
- Test: `crates/planora-app/tests/file_chooser_folder.rs` (neu)

**Interfaces:**
- Consumes: vorhandene `FileChooser`, `Places`, `places::*`.
- Produces:
```rust
pub enum ChooserMode { Save, Open, Folder }
pub const CHOOSE_FOLDER: &str = "Diesen Ordner wählen";
pub const NO_SUBFOLDERS: &str = "Hier gibt es keine Unterordner.";
pub const CHOSEN_PREFIX: &str = "Gewählt:";
pub const NO_FOLDER: &str = "Bitte einen vorhandenen Ordner wählen.";
impl FileChooser {
    pub fn folder(dir: PathBuf) -> FileChooser;     // nur Ordner gelistet
    pub fn marked(&self) -> Option<&str>;           // per Klick markierter Unterordner
    pub fn mark(&mut self, name: &str);             // nur Ordner-Modus; Ordnerwechsel löscht die Marke
}
/// Pfad für die Anzeige ohne `\\?\`-Präfix (unverändert aus `projects_panel.rs` verschoben).
pub fn display_path(path: &Path) -> String;
```
Regeln: Im Ordner-Modus listet `reload` nur Ordner (Dateien nie, auch passende Endungen nicht); Klick markiert einen Ordner (hervorgehoben wie eine gewählte Datei), Doppelklick geht hinein; `target()` = markierter Ordner (falls vorhanden und Ordner), sonst der aktuelle Ordner, `None` wenn der nicht (mehr) existiert; `confirm()` fragt nie nach Ersetzen, Fehler `NO_FOLDER`. Statt der Zeile „Dateiname“ steht eine Zeile `CHOSEN_PREFIX` + `display_path(target)` (gekürzt). Fußzeile: `CHOOSE_FOLDER`, „Abbrechen“. Schnellzugriffe, Favoriten, Ordnerfeld wie bisher. Save/Open unverändert.

- [ ] **Step 1: Failing tests** in `tests/file_chooser_folder.rs` (Temp-Ordner mit `a/`, `b/`, `teil.stl`, `x.step`; `Places { drives: Some(vec![]), .. }`):
```rust
#[test] fn folder_mode_lists_only_folders() { assert_eq!(names(&c), ["a", "b"]); assert!(c.entries().iter().all(|e| e.is_dir)); }
#[test] fn without_mark_the_current_folder_is_chosen() { assert_eq!(c.confirm(), ChooserAction::Chosen(dir.clone())); }
#[test] fn a_marked_folder_is_chosen_and_entering_clears_the_mark() {
    c.mark("b"); assert_eq!(c.target(), Some(dir.join("b")));
    c.enter("a"); assert_eq!(c.marked(), None); assert_eq!(c.target(), Some(dir.join("a"))); }
#[test] fn up_and_places_clear_the_mark() { /* mark, up() → marked None; mark, go_to_place → None */ }
#[test] fn a_vanished_folder_cannot_be_chosen() { /* remove dir → target None; confirm None; error == NO_FOLDER */ }
#[test] fn click_marks_and_double_click_enters_in_the_list() { /* Screen: Klick auf „b“ → marked Some("b"); Doppelklick „a“ → dir() endet auf a */ }
#[test] fn footer_says_choose_this_folder_and_cancel_closes() { /* Texte CHOOSE_FOLDER, „Abbrechen“; kein „Dateiname“, keine „Öffnen“/„Speichern“; Klick „Abbrechen“ → Cancel */ }
#[test] fn the_chosen_line_shows_the_target() { /* Text beginnt mit CHOSEN_PREFIX und enthält „b“ nach mark("b") */ }
#[test] fn empty_folder_shows_no_subfolders_hint() { assert!(texts.contains(NO_SUBFOLDERS)); }
#[test] fn save_and_open_modes_are_unchanged() { /* Open listet Ordner + passende Dateien; Klick auf Ordner markiert nichts (marked None) */ }
```
Der verschobene Unit-Test `the_verbatim_prefix_is_hidden_in_the_display_only` läuft unverändert in `file_chooser.rs`.
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test file_chooser_folder` → FAIL (kein `ChooserMode::Folder`).
- [ ] **Step 3: Implement** die Interfaces. `Entry`, `RowMenu`, Kontextmenü „Zu Favoriten hinzufügen“ gelten im Ordner-Modus unverändert.
- [ ] **Step 4:** Tests grün; zusätzlich `--test file_places --test file_places_layout --test file_dialog_size --test file_dialog_buttons` grün.
- [ ] **Step 5: Mutationen (≥ 10):** Dateien im Ordner-Modus gelistet; Marke bei `enter` nicht gelöscht; Marke bei `up` nicht gelöscht; Marke bei `go_to_place` nicht gelöscht; `target` ignoriert Marke; `target` liefert nicht vorhandenen Ordner; `confirm` fragt nach Ersetzen; Klick auf Ordner im Öffnen-Modus markiert; Fußzeile „Öffnen“ im Ordner-Modus; Hinweis „passende Dateien“ statt `NO_SUBFOLDERS`; Zeile `CHOSEN_PREFIX` zeigt aktuellen statt markierten Ordner.
- [ ] **Step 6: Commit** `feat(app): Dateiauswahl im Ordner-Modus` (Pfade einzeln, Trailer).

---

### Task 2: Fenster „Einstellungen“ mit Projektverzeichnis

**Files:**
- Create: `crates/planora-app/src/settings_window.rs`
- Modify: `crates/planora-app/src/lib.rs` (`pub mod settings_window;`)
- Test: `crates/planora-app/tests/settings_window.rs` (neu)

**Interfaces:**
- Consumes: Task 1 `FileChooser::folder`, `ChooserMode::Folder`, `CHOOSE_FOLDER`; `Workspace::{choose_root, persist_config, places, save_places, library, config, config_path, library_revision}`; `Dialog`, `DialogSize::File`.
- Produces:
```rust
pub const TITLE: &str = "Einstellungen";
pub const SECTION_PROJECTS: &str = "Projekte";
pub const DIRECTORY_LABEL: &str = "Projektverzeichnis";
pub const CHANGE_DIRECTORY: &str = "Projektverzeichnis ändern …";
pub const CHOOSE_HEADING: &str = "Projektverzeichnis wählen";
pub const KEEP_HINT: &str = "Ihre Projekte bleiben, wo sie sind. Im neuen Verzeichnis erscheinen nur die Projekte, die dort liegen.";
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootMessage { Warning(String), Error(String) }   // Warnfarbe bzw. Fehlerfarbe
/// Wechselt das Projektverzeichnis wie `Workspace::choose_root` + `persist_config`.
/// Gleicher Ordner (nach `places::same_dir`) → Ok ohne Schreiben. Offene gespeicherte Dokumente
/// → Warning mit dem Wortlaut aus `choose_root`; `Library::open`-Fehler → Error.
pub fn change_root(ws: &mut Workspace, dir: &Path) -> Result<(), RootMessage>;
/// Angezeigter Pfad: gewählte Bibliothek, sonst `config.root`, sonst `Config::default_root()`.
pub fn current_directory(ws: &Workspace) -> PathBuf;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsEvent { None, Closed { return_to_projects: bool } }
#[derive(Default)]
pub struct SettingsWindow { /* open, return_to_projects, chooser: Option<FileChooser>, message: Option<RootMessage> */ }
impl SettingsWindow {
    pub fn open(&mut self, return_to_projects: bool);   // leert Meldung und Auswahl
    pub fn close(&mut self);
    pub fn is_open(&self) -> bool;
    pub fn start_choosing(&mut self, ws: &Workspace);   // Ordner-Modus im aktuellen Verzeichnis, sonst dessen Eltern, sonst `file_chooser::default_dir()`
    pub fn chooser_mut(&mut self) -> Option<&mut FileChooser>;
    pub fn message(&self) -> Option<&RootMessage>;
    /// Zeichnet das Fenster (Id `"planora_settings_window"`, `center_on_open`, ✕, Fußzeile
    /// „Schließen“ bzw. beim Wählen die Fußzeile der Auswahl).
    pub fn show(&mut self, ctx: &egui::Context, ws: &mut Workspace, places: &mut Places) -> SettingsEvent;
}
```
Aufbau: Abschnitte untereinander mit Überschrift (`theme::section_header`), jetzt einer: `SECTION_PROJECTS`; darin `DIRECTORY_LABEL`, Pfad (`file_chooser::display_path`), Knopf `CHANGE_DIRECTORY`, `KEEP_HINT`, darunter `message`. „Diesen Ordner wählen“ → `change_root`; Erfolg schließt die Auswahl, Fehler bleibt als Meldung stehen, Auswahl schließt ebenfalls. „Abbrechen“ schließt nur die Auswahl.

- [ ] **Step 1: Failing tests** (`tests/settings_window.rs`; jede `Workspace` mit `config_path` im Temp-Ordner, Bibliothek unter `common::temp_root`):
```rust
#[test] fn the_window_shows_section_path_button_and_hint() { /* Texte TITLE, SECTION_PROJECTS, DIRECTORY_LABEL, Pfad, CHANGE_DIRECTORY, KEEP_HINT, „Schließen“; kein Text enthält „Wurzel“ */ }
#[test] fn changing_the_directory_with_open_documents_is_refused_and_writes_nothing() {
    // save_as legt ein Dokument an (Tab offen); config.json mit {"root":…,"favorites":[F]} vorbelegt
    assert_eq!(change_root(&mut ws, &other), Err(RootMessage::Warning("Bitte zuerst alle gespeicherten Dokumente schließen, bevor der Projektordner wechselt.".into())));
    assert_eq!(fs::read(&cfg).unwrap(), before); assert_eq!(ws.library.as_ref().unwrap().root().path(), old_root); assert!(!other.exists()); }
#[test] fn a_new_directory_keeps_favorites_and_only_changes_root() {
    assert_eq!(change_root(&mut ws, &other), Ok(())); let disk = Config::read_from(&cfg).unwrap();
    assert_eq!(disk.root, Some(other.clone())); assert_eq!(disk.favorites, vec![fav]); assert_eq!(ws.library_revision, rev + 1);
    assert!(old_root.join("P").join("projekt.json").is_file(), "altes Projekt bleibt, wo es ist"); }
#[test] fn the_same_directory_changes_nothing() { /* change_root(root) → Ok; config.json nicht angelegt; library_revision gleich */ }
#[test] fn an_unusable_directory_gives_an_error_and_keeps_the_old_one() { /* Ziel ist eine Datei → Err(RootMessage::Error(_)); Wurzel gleich; config unverändert */ }
#[test] fn an_unreadable_config_is_not_overwritten_by_favorites_but_root_is_chosen() { /* config.json = "{kaputt" → change_root Ok; Datei danach gültig mit root (Verhalten von persist_config) */ }
#[test] fn cancel_changes_nothing() { /* start_choosing, „Abbrechen“ per Klick → chooser None, Wurzel gleich, config fehlt weiter */ }
#[test] fn choosing_a_marked_folder_switches_and_closes_the_chooser() { /* mark("Neu") + Klick CHOOSE_FOLDER → Wurzel = …/Neu, message None, chooser None */ }
#[test] fn the_chooser_starts_in_the_current_directory() { assert!(places::same_dir(w.chooser_mut().unwrap().dir(), &root)); }
#[test] fn a_refusal_is_shown_in_warning_colour() { /* Text der Meldung mit Farbe theme::palette(dark).warning, Hell und Dunkel */ }
#[test] fn closing_reports_the_way_back_to_projects() { /* open(true); Klick „Schließen“ → Closed{return_to_projects:true}; ✕ ebenso; open(false) → false */ }
#[test] fn the_window_fits_800x600_while_choosing() { /* mit 24 Favoriten und vollem Ordner (wie file_places_layout::check): ≤ 720 breit, im Bild, CHOOSE_FOLDER und „Abbrechen“ sichtbar */ }
```
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test settings_window` → FAIL (Modul fehlt).
- [ ] **Step 3: Implement** Modul und Interfaces. Geänderte Favoriten der Auswahl gehen über `places` zurück (der Aufrufer ruft `ws.save_places`).
- [ ] **Step 4:** Tests grün; `--test workspace --test file_places --test file_places_wiring` grün.
- [ ] **Step 5: Mutationen (≥ 10):** Sperre umgangen (`choose_root` vor der Prüfung); `persist_config` vergessen; `save_to` statt `update_at` (Favoriten weg); gleicher Ordner schreibt trotzdem; Fehler als Warning; Warning als Error; Abbrechen ruft `change_root`; Marke ignoriert; Meldung bleibt nach neuem `open`; `return_to_projects` immer `false`; Auswahl startet in `default_dir` statt im Verzeichnis.
- [ ] **Step 6: Commit** `feat(app): Fenster Einstellungen mit Projektverzeichnis`.

---

### Task 3: Kachelmodell (rein, ohne Oberfläche)

**Files:**
- Create: `crates/planora-app/src/project_tiles.rs`
- Modify: `crates/planora-app/src/lib.rs`
- Test: `crates/planora-app/tests/project_tiles.rs` (neu)

**Interfaces:**
- Consumes: `planora_project::library::{Library, TreeNode, NodeKind, NodePath}`, `docstore::{peek, DocumentInfo}`, `planora_exchange::Progress`.
- Produces:
```rust
pub const MAX_TILES: usize = 500;
pub const TOO_MANY: &str = "Es werden nur die ersten 500 gezeigt.";
pub const CANCELLED: &str = "Abgebrochen";
#[derive(Clone, Debug, PartialEq, Eq)] pub struct DocFacts { pub version: u32, pub branch: String, pub changed: String /* ISO, UTC */ }
#[derive(Clone, Debug, PartialEq, Eq)] pub enum DocState { Read(DocFacts), Unreadable(String) }
#[derive(Clone, Debug, PartialEq, Eq)] pub struct DocEntry { pub dir: PathBuf, pub state: DocState }
#[derive(Clone, Debug)] pub struct Scan { pub root: PathBuf, pub tree: Vec<TreeNode>, pub docs: BTreeMap<NodePath, DocEntry> }
/// Baum + `peek` je Dokument; fehlt `root`, Fehler (nie anlegen); Abbruch → Err(CANCELLED).
pub fn scan_library(root: &Path, progress: &Progress) -> Result<Scan, String>;
/// Wie `scan_library` mit vorgegebenem Baum und Lesefunktion (Tests).
pub fn scan_tree(root: PathBuf, tree: Vec<TreeNode>, read: impl FnMut(&NodePath) -> DocEntry) -> Scan;
/// Spitze des aktiven Zweigs, Zweigname, `latest_time()`; ohne Spitze → None.
pub fn facts(info: &DocumentInfo) -> Option<DocFacts>;
pub fn doc_entry(dir: &Path) -> DocEntry;   // peek-Fehler → Unreadable(e.to_string())
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum TileKind { Project, Folder, Document }
#[derive(Clone, Debug, PartialEq)]
pub struct Tile { pub kind: TileKind, pub name: String, pub path: NodePath,
    pub documents: usize, pub unreadable: usize,
    /// Zuletzt geändertes lesbares Dokument darunter (bei Dokumentkacheln es selbst).
    pub newest: Option<(NodePath, DocFacts)> }
/// Kacheln der Ebene `at` (leer = Projekte); `None`, wenn es `at` nicht mehr gibt. Reihenfolge
/// wie der Baum (Ordner und Dokumente gemischt nach Namen, wie `Library::tree`).
pub fn tiles_at(scan: &Scan, at: &NodePath) -> Option<Vec<Tile>>;
/// „23.10.2026, 16:42“ aus „2026-10-23T16:42:07Z“ (P1: Ortszeit über `local_time`); unlesbar → None.
pub fn format_changed(iso: &str) -> Option<String>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Tone { Normal, Warning }
pub fn changed_line(tile: &Tile) -> String;     // „Geändert 23.10.2026, 16:42“ | „Datum unbekannt“ | "" (kein lesbares Dokument)
pub fn meta_line(tile: &Tile, no_bodies: bool) -> (String, Tone);
pub fn documents_text(n: usize) -> String;      // „1 Dokument“ / „{n} Dokumente“
pub fn footer_text(tiles: &[Tile], top_level: bool) -> String;
```
Texte von `meta_line` (wörtlich): Projekt: 0 Dokumente → „Noch kein Dokument“; alle unlesbar → („Dokumente nicht lesbar“, Warning); `no_bodies` → „Noch keine Körper · {documents_text}“; sonst „Version {v} · {documents_text}“. Ordner: 0 → „Noch kein Dokument“, alle unlesbar → („Dokumente nicht lesbar“, Warning), sonst `documents_text`. Dokument: unlesbar → („Dokument nicht lesbar“, Warning), sonst „Version {v}“. `footer_text`: oben „1 Projekt“/„{n} Projekte“/„Keine Projekte“; darunter Teile „{n} Ordner“ (nur n > 0) und `documents_text` (nur n > 0) mit „ · “, beides 0 → „Leer“. Mehr als `MAX_TILES` Kacheln: abgeschnitten (der Hinweis steht im Fenster).

- [ ] **Step 1: Failing tests** (`tests/project_tiles.rs`; reine Fälle über `scan_tree` mit von Hand gebauten `TreeNode`/`DocEntry`, zwei Fälle über echte Bibliotheken):
```rust
#[test] fn format_changed_is_day_month_year_and_minutes() {
    assert_eq!(format_changed("2026-10-23T16:42:07Z").as_deref(), Some("23.10.2026, 16:42"));
    assert_eq!(format_changed("2026-1-3T1:2Z"), None); assert_eq!(format_changed(""), None); assert_eq!(format_changed("x".repeat(30).as_str()), None); }
#[test] fn newest_document_dates_the_project() { /* zwei Dokumente 10:00 und 12:00 → newest = das von 12:00, changed_line „Geändert 23.10.2026, 12:00“ */ }
#[test] fn version_is_the_tip_of_the_active_branch_not_the_newest() {
    // echtes Dokument: v1 Haupt, v2 Zweig „B“ (jünger), aktiver Zweig Haupt
    assert_eq!(facts(&info).unwrap().version, 1); }
#[test] fn ties_pick_the_smaller_name() { /* „b“ und „A“ gleiche Zeit → „A“ */ }
#[test] fn unreadable_documents_count_but_do_not_date() { /* 1 lesbar alt + 1 unlesbar → documents 2, unreadable 1, newest = lesbares */ }
#[test] fn all_unreadable_warns() { assert_eq!(meta_line(&t, false), ("Dokumente nicht lesbar".into(), Tone::Warning)); }
#[test] fn nested_documents_count_for_project_and_folder() { /* P/F/G/d1, P/d2 → Projekt 2, Ordner F 1, newest über alle Ebenen */ }
#[test] fn meta_lines_have_the_spec_texts() { /* „Version 12 · 3 Dokumente“, „Noch keine Körper · 1 Dokument“, „Noch kein Dokument“, Ordner „4 Dokumente“, Dokument „Version 12“ */ }
#[test] fn footer_counts() { /* „6 Projekte“, „1 Projekt“, „Keine Projekte“, „1 Ordner · 3 Dokumente“, „2 Ordner“, „Leer“ */ }
#[test] fn tiles_at_a_vanished_location_is_none() { assert!(tiles_at(&scan, &vec!["weg".into()]).is_none()); }
#[test] fn more_than_500_tiles_are_cut() { assert_eq!(tiles_at(&scan_mit_501_projekten, &vec![]).unwrap().len(), MAX_TILES); }
#[test] fn scan_library_reads_a_real_library_and_ignores_extra_files() { /* Bibliothek mit Projekt, Ordner, Dokument (DocStore::create), plus fremde Datei „notiz.txt“ im Projekt → docs.len()==1, DocState::Read */ }
#[test] fn scan_library_never_creates_a_missing_root() { /* Err, Ordner existiert danach nicht */ }
#[test] fn a_cancelled_scan_stops() { /* Progress::cancel() vorher → Err(CANCELLED) */ }
```
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test project_tiles` → FAIL.
- [ ] **Step 3: Implement.** `scan_library`: `root.is_dir()` prüfen, dann `Library::open` und `tree()`; Dokumentordner über `Library::document_dir`. Abbruch je Dokument prüfen.
- [ ] **Step 4:** grün.
- [ ] **Step 5: Mutationen (≥ 10):** `max` statt neueste Zeit falsch herum (`min`); Version = höchste Nummer statt Spitze des aktiven Zweigs; Gleichstand nimmt größeren Namen; Namensvergleich mit Groß-/Kleinschreibung; unlesbare zählen nicht mit; unlesbare datieren; nur direkte Kinder gezählt; `format_changed` vertauscht Tag/Monat; Sekunden statt Minuten; `MAX_TILES` nicht angewandt; fehlende Wurzel wird angelegt; Abbruch ungeprüft; „1 Dokumente“.
- [ ] **Step 6: Commit** `feat(app): Kachelmodell für das Projekte-Fenster`.

---

### Task 4: Software-Rasterer für Miniaturen

**Files:**
- Create: `crates/planora-app/src/thumbnail.rs`
- Modify: `crates/planora-app/src/lib.rs`, `crates/planora-app/src/theme.rs` (`pub fn thumbnail_background(dark: bool) -> Color32` = `palette(dark).bg_inset`)
- Test: `crates/planora-app/tests/thumbnail_render.rs` (neu)

**Interfaces:**
- Consumes: `planora_mesh::Mesh` (`positions`, `indices`), `viewport::render_palette(dark).face_base`, `planora_render::camera::OrbitCamera::default()`.
- Produces:
```rust
pub const THUMB_W: u32 = 248; pub const THUMB_H: u32 = 140; pub const THUMB_SCALE: u32 = 2;   // Datei 496 × 280
pub const MARGIN: f32 = 0.08; pub const SUPERSAMPLE: u32 = 2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct ThumbColors { pub background: [u8; 4], pub body: [u8; 4] }
pub fn thumb_colors(dark: bool) -> ThumbColors;          // aus theme/render_palette, ohne Color32-Konstruktoren
#[derive(Clone, Copy, Debug, PartialEq)] pub struct ThumbView { pub yaw: f32, pub pitch: f32 }
impl ThumbView { pub fn start() -> ThumbView; }          // Werte aus OrbitCamera::default()
#[derive(Clone, Debug, PartialEq, Eq)] pub struct Image { pub width: u32, pub height: u32, pub rgba: Vec<u8> }
/// Parallele Projektion, alle endlichen Punkte eingepasst mit Rand; None ohne endliche Punkte.
pub struct Projector { /* … */ }
impl Projector {
    pub fn fit(meshes: &[&Mesh], view: ThumbView, width: u32, height: u32) -> Option<Projector>;
    pub fn project(&self, p: DVec3) -> (f32, f32, f32);   // Pixel x, Pixel y (nach unten), Tiefe (größer = näher)
}
pub fn render(meshes: &[&Mesh], view: ThumbView, colors: &ThumbColors, width: u32, height: u32) -> Image;
pub fn encode_png(img: &Image) -> Result<Vec<u8>, String>;
/// Nur RGBA8 in genau `width × height` (sonst Fehler); keine anderen Formate.
pub fn decode_png(bytes: &[u8], width: u32, height: u32) -> Result<Image, String>;
```
Algorithmus (nicht durch Tests bestimmt): Kantenfunktionen je Dreieck über das Begrenzungsrechteck, Tiefe baryzentrisch interpoliert, z-Puffer in `f32`, Unterabtastung `SUPERSAMPLE²` je Pixel und Mittelung; Normale aus den Eckpunkten (nicht aus `normals`), Helligkeit nach P7; Dreiecke mit nicht endlichen Punkten oder Fläche 0 werden übergangen. Kein `panic!` bei beliebigen Eingaben.

- [ ] **Step 1: Failing tests** (`tests/thumbnail_render.rs`; Würfel- und Quadrat-Netze von Hand):
```rust
#[test] fn an_empty_scene_is_only_background() { assert!(img.rgba.chunks(4).all(|p| p == c.background)); assert_eq!((img.width, img.height), (496, 280)); }
#[test] fn the_projector_fits_with_the_margin() { /* Würfel: alle 8 Ecken projiziert in [m, w−m]×[m, h−m] mit m = 0.08·280; mindestens eine Achse berührt den Rand ±1 px */ }
#[test] fn corners_land_where_the_projector_says() { /* Dreieck: Pixel 2 px innerhalb jeder projizierten Ecke ist Körperfarbe, 3 px außerhalb Hintergrund */ }
#[test] fn the_nearer_face_wins_in_any_order() { /* zwei Quadrate hintereinander, beide Reihenfolgen → gleiche Bilder; Mitte zeigt das nähere (anders schattiert) */ }
#[test] fn three_visible_cube_faces_have_three_shades() { /* Startblick: drei verschiedene Farben an den Flächenmitten, alle ≠ Hintergrund */ }
#[test] fn the_view_is_the_start_view_of_the_main_window() { assert_eq!(ThumbView::start(), ThumbView { yaw: OrbitCamera::default().yaw, pitch: OrbitCamera::default().pitch }); }
#[test] fn degenerate_and_non_finite_input_does_not_panic() { /* NaN-Punkt, Index außerhalb, Fläche 0, 1e300 → Bild ohne Panik; nur NaN → Hintergrund */ }
#[test] fn huge_and_tiny_bodies_both_fill_the_image() { /* Würfel 1e-6 und 1e6: Anteil Körperpixel gleich ±2 % */ }
#[test] fn colours_follow_the_theme() { assert_eq!(thumb_colors(true).background, theme::thumbnail_background(true).to_array()); /* hell ≠ dunkel */ }
#[test] fn png_roundtrip_and_strict_decode() { /* encode→decode gleich; falsche Größe, Graustufen-PNG, abgeschnittene Bytes → Err */ }
```
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test thumbnail_render` → FAIL.
- [ ] **Step 3: Implement.** `tests/no_hard_coded_colors.rs` muss grün bleiben.
- [ ] **Step 4:** grün, dazu `--test no_hard_coded_colors --test theme`.
- [ ] **Step 5: Mutationen (≥ 10):** z-Vergleich umgedreht; z-Puffer fehlt (Reihenfolge entscheidet); Rand 0; y nicht gespiegelt; yaw fest statt aus der Kamera; Normale aus `normals`; `|n·l|` ohne Betrag (Rückseiten schwarz); Unterabtastung ohne Mittelung; NaN-Prüfung entfernt (Panik/Fehlbild); Einpassen nur in x; `decode_png` nimmt falsche Größe an.
- [ ] **Step 6: Commit** `feat(app): eigener Rasterer für Miniaturen`.

---

### Task 5: Miniatur aus einem Dokument und Zusatzdateien

**Files:**
- Create: `crates/planora-app/src/thumbnail_cache.rs`
- Modify: `crates/planora-app/src/lib.rs`
- Test: `crates/planora-app/tests/thumbnail_cache.rs` (neu), `crates/planora-app/tests/thumbnail_files.rs` (neu: Unsichtbarkeit für Bibliothek und Versionsspeicher)

**Interfaces:**
- Consumes: Task 4 `render`, `encode_png`, `decode_png`, `ThumbColors`, `thumb_colors`, `ThumbView::start`, `Image`, Konstanten; `docstore::read_version`, `Document::{from_snapshot, visible_bodies, visible_meshes}`, `planora_mesh::{tessellate, TessellationOptions}`, `fsutil::atomic_write_unique`.
- Produces:
```rust
pub const PREVIEW_PNG: &str = "vorschau.png"; pub const PREVIEW_JSON: &str = "vorschau.json";
pub const PREVIEW_FORMAT: u32 = 1; pub const PROGRAM: &str = env!("CARGO_PKG_VERSION");
pub const MAX_TRIANGLES: usize = 200_000; pub const MAX_BUILD: Duration = Duration::from_secs(3);
pub const THUMB_CHORD_MM: f64 = 0.1;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThumbKey { pub version: u32, pub branch: String, pub program: String, pub dark: bool }
impl ThumbKey { pub fn new(version: u32, branch: &str, dark: bool) -> ThumbKey; }   // program = PROGRAM
#[derive(Clone, Debug)]
pub enum Thumb { Image(Arc<Image>), NoBodies, TooComplex, Failed(String) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct BuildLimits { pub max_triangles: usize, pub max_time: Duration }
impl Default for BuildLimits { /* MAX_TRIANGLES, MAX_BUILD */ }
/// Gültige Zusatzdatei (P6) → Some; alles andere (fehlt, kaputt, anderer Schlüssel) → None.
pub fn load_cached(doc_dir: &Path, key: &ThumbKey) -> Option<Thumb>;
/// Liest Version `key.version`, baut, vernetzt, zeichnet; nie eine Panik nach außen.
pub fn build(doc_dir: &Path, key: &ThumbKey, limits: &BuildLimits) -> Thumb;
/// PNG (nur bei Image), dann JSON; `Failed` wird nie gespeichert (Err).
pub fn store(doc_dir: &Path, key: &ThumbKey, thumb: &Thumb) -> Result<(), String>;
#[derive(Clone, Debug)] pub struct ThumbOutcome { pub thumb: Thumb, pub stored: bool, pub from_cache: bool }
/// `load_cached` oder `build` + `store`.
pub fn thumbnail_for(doc_dir: &Path, key: &ThumbKey, limits: &BuildLimits) -> ThumbOutcome;
```
Regeln: Körper = `visible_bodies()` (Kernel- und STEP-Körper, je `tessellate` mit `THUMB_CHORD_MM`; ein Netzfehler lässt nur diesen Körper weg) + `visible_meshes()` (`mesh.placed(&transform)`, Dreiecke vorher gezählt). Keine Körper → `NoBodies`. Grenzen nach P8. `from_snapshot`-Fehler, Lesefehler, Panik → `Failed`. Nur `vorschau.png`/`vorschau.json` (und ihre Zwischendateien) werden je geschrieben.

- [ ] **Step 1: Failing tests** (`tests/thumbnail_cache.rs`; Bibliothek in `common::temp_root`, Dokumente über `DocStore::create(dir, &state.snapshot(), "")` mit `AppState::create_box` usw.):
```rust
#[test] fn a_box_gives_an_image_that_is_stored_and_reused() { /* thumbnail_for → Image, stored, !from_cache; zweiter Aufruf from_cache, gleiche Pixel; JSON-Felder wie P6 mit "art":"bild" */ }
#[test] fn the_key_invalidates_on_version_branch_program_and_theme() { /* load_cached mit version+1, anderem Zweig, program "0.0.0", dark umgedreht → je None; gleicher Schlüssel → Some */ }
#[test] fn a_damaged_or_missing_preview_is_rebuilt() { /* PNG abgeschnitten / JSON "{" / JSON ohne PNG / PNG 10×10 → load_cached None, thumbnail_for baut neu und speichert */ }
#[test] fn an_empty_document_stores_no_bodies() { /* Thumb::NoBodies, JSON "art":"leer", kein PNG nötig */ }
#[test] fn suppressed_features_and_sketches_do_not_appear() { /* Quader unterdrückt + Skizze → NoBodies */ }
#[test] fn a_failed_feature_leaves_the_other_bodies() { /* Quader + Feature im Fehlerzustand → Image */ }
#[test] fn mesh_and_step_bodies_are_drawn() { /* Netz-Körper (state.import_mesh mit Würfel) → Image; STEP-Körper wie tests/step_state.rs::shape → Image */ }
#[test] fn too_many_triangles_store_too_complex_once() { /* BuildLimits{max_triangles: 10,..} → TooComplex, JSON "zu_aufwendig"; zweiter Aufruf from_cache ohne Aufbau */ }
#[test] fn a_slow_build_is_too_complex() { /* max_time: Duration::ZERO → TooComplex */ }
#[test] fn damaged_documents_give_failed_without_panic_and_write_nothing() { /* Versionsdatei abgeschnitten, Spitzenversion gelöscht, JSON mit NaN → Failed; kein vorschau.* */ }
#[test] fn a_read_only_folder_keeps_the_thumbnail_in_memory_only() { /* Ordner schreibgeschützt (Windows: Datei vorschau.json als Ordner angelegt, damit Schreiben scheitert) → thumb Image, stored false, keine Panik */ }
#[test] fn an_open_modified_document_shows_the_saved_version() { /* save_as v1 Quader; im Tab Kugel dazu (nicht gespeichert); build(v1) zeigt nur den Quader (Anteil Körperpixel gleich wie ein reines Quader-Dokument) */ }
#[test] fn building_never_changes_other_files_in_the_document_folder() { /* SHA-256 aller Dateien außer vorschau.* vor/nach thumbnail_for gleich, auch .lock eines offenen Tabs */ }
```
`tests/thumbnail_files.rs` (Spec §6, §9: Unsichtbarkeit):
```rust
#[test] fn library_tree_ignores_preview_files() { /* tree() mit und ohne vorschau.* gleich (Namen, Arten, Kinder) */ }
#[test] fn peek_and_open_report_no_orphans_and_save_still_works() { /* peek: orphans/missing leer; DocStore::open + save + load_version ok */ }
#[test] fn rename_move_delete_carry_the_preview_along() { /* Library::rename/move_node/delete → vorschau.* liegt am neuen Ort bzw. im .papierkorb */ }
#[test] fn a_copied_document_folder_keeps_a_valid_preview() { /* Ordner rekursiv kopieren (neuer Name .pcad) → peek ok, load_cached gültig */ }
#[test] fn leftover_temporary_preview_files_are_ignored() { /* vorschau.png.123-4.tmp im Ordner → tree/peek/open unverändert */ }
#[test] fn old_golden_documents_still_load_and_save_byte_identical() { /* eine Datei aus crates/planora-project/tests/data in Temp-Dokument kopieren, vorschau.* daneben, read_version + encode gleich wie vorher */ }
```
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test thumbnail_cache --test thumbnail_files` → FAIL.
- [ ] **Step 3: Implement.** Die App hat `serde_json` nicht als direkte Abhängigkeit und bekommt es nicht (Cargo.toml bleibt): `vorschau.json` wird von Hand geschrieben (eine Zeile, Schlüssel in der Reihenfolge von P6, Zweigname mit `\"` und `\\` maskiert, Steuerzeichen als `\uXXXX`) und mit einem kleinen strengen Leser gelesen (genau diese acht Schlüssel, Zahlen ganzzahlig ohne Vorzeichen, `true`/`false`, höchstens 4 KiB; alles andere → ungültig, also neu bauen).
- [ ] **Step 4:** grün; `cargo test -p planora-project -j 2` unverändert grün.
- [ ] **Step 5: Mutationen (≥ 10):** Schlüsselvergleich ohne Zweig; ohne Programm; ohne `dark`; JSON vor PNG geschrieben; `atomic_write` statt `_unique`; PNG-Größe nicht geprüft; `Failed` gespeichert; Dreieckszahl erst nach `placed`; Zeitgrenze nie geprüft; `catch_unwind` entfernt (Panik-Test rot); unterdrückte Körper mitgezeichnet (`features` statt `visible_bodies`); Versionsnummer aus `max_number` statt Schlüssel.
- [ ] **Step 6: Commit** `feat(app): Miniaturen als Zusatzdatei im Dokumentordner`.

---

### Task 6: Hintergrund-Arbeiter für Miniaturen

**Files:**
- Create: `crates/planora-app/src/thumbnail_worker.rs`
- Modify: `crates/planora-app/src/lib.rs`
- Test: `crates/planora-app/tests/thumbnail_worker.rs` (neu)

**Interfaces:**
- Consumes: Task 5 `ThumbKey`, `Thumb`, `ThumbOutcome`, `thumbnail_for`, `BuildLimits`.
- Produces:
```rust
#[derive(Clone, Debug, PartialEq, Eq)] pub struct ThumbRequest { pub dir: PathBuf, pub key: ThumbKey }
#[derive(Clone, Debug)] pub struct ThumbDone { pub dir: PathBuf, pub key: ThumbKey, pub outcome: ThumbOutcome }
pub struct ThumbWorker { /* ein Thread, Mutex<VecDeque<ThumbRequest>> + Condvar, mpsc für Ergebnisse */ }
impl ThumbWorker {
    pub fn start() -> ThumbWorker;   // Arbeit = thumbnail_for(dir, key, &BuildLimits::default())
    pub fn start_with(work: impl Fn(&ThumbRequest) -> ThumbOutcome + Send + 'static) -> ThumbWorker;
    /// Ersetzt die Warteschlange: erst `visible`, dann `rest`, ohne Doppelte (nach `dir`), ohne
    /// den gerade laufenden Auftrag.
    pub fn set_queue(&self, visible: Vec<ThumbRequest>, rest: Vec<ThumbRequest>);
    /// Fenster zu: nichts Neues beginnt; ein laufender Auftrag läuft zu Ende und speichert.
    pub fn cancel_pending(&self);
    pub fn poll(&self) -> Vec<ThumbDone>;
    pub fn running(&self) -> Option<PathBuf>;
    pub fn wait_idle(&self, timeout: Duration) -> bool;   // Tests
}
```
Panik in `work` → `ThumbDone` mit `Thumb::Failed("…")`, der Thread läuft weiter. Fallenlassen des `ThumbWorker` beendet den Thread nach dem laufenden Auftrag.

- [ ] **Step 1: Failing tests** (Arbeit über `start_with` mit Zählern und Sperren aus `std::sync`):
```rust
#[test] fn at_most_one_build_runs_at_a_time() { /* 20 Aufträge, Arbeit zählt gleichzeitige Läufe → Höchstwert 1, alle 20 fertig */ }
#[test] fn visible_tiles_come_first() { /* x läuft (Sperre); set_queue([c], [a, b]); freigeben → Reihenfolge x, c, a, b */ }
#[test] fn a_new_queue_replaces_the_old_one() { /* set_queue([a,b]) dann set_queue([c],[]) während x läuft → x, c */ }
#[test] fn duplicates_and_the_running_request_are_not_queued_twice() { /* set_queue([x, a, a]) während x läuft → x, a */ }
#[test] fn closing_starts_nothing_new_but_finishes_and_keeps_the_running_one() { /* x läuft; cancel_pending; freigeben → nur x in poll(), stored true */ }
#[test] fn a_panicking_build_reports_failed_and_the_worker_goes_on() { /* Arbeit panikt für a → Failed; b danach fertig */ }
#[test] fn running_names_the_document_in_progress() { assert_eq!(w.running(), Some(dir_x)); }
#[test] fn the_real_worker_stores_a_preview() { /* start() mit echtem Quader-Dokument → Image, vorschau.json existiert */ }
#[test] fn dropping_the_worker_does_not_hang() { /* drop während Leerlauf kehrt sofort zurück (Zeitgrenze 2 s) */ }
```
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test thumbnail_worker` → FAIL.
- [ ] **Step 3: Implement** (Thread-Name „planora-miniaturen“; `catch_unwind(AssertUnwindSafe(..))`).
- [ ] **Step 4:** grün (Test dreimal hintereinander, keine Flackerer).
- [ ] **Step 5: Mutationen (≥ 10):** zwei Threads; `rest` vor `visible`; Anhängen statt Ersetzen; keine Doppeltprüfung; laufender Auftrag erneut eingereiht; `cancel_pending` bricht auch den laufenden ab (Ergebnis fehlt); `cancel_pending` wirkungslos; `catch_unwind` entfernt; `running` nie gesetzt; Ergebnis nur bei `stored`; Condvar-Benachrichtigung fehlt (Zeitgrenze rot).
- [ ] **Step 6: Commit** `feat(app): Hintergrund-Arbeiter für Miniaturen`.

---

### Task 7: Projekte-Fenster – Zustand, Navigation und Aktionen

**Files:**
- Create: `crates/planora-app/src/project_actions.rs` (aus `projects_panel.rs` verschoben: `ProjectsUi`, `notice`, `is_under`, `open_tabs_under`, `may_change`, `open_node`, `create_project`, `create_folder`, `rename_node`, `move_node`, `delete_node`, `containers`, `label`, `dialog_window`, `request_*`, `dialog_open`, `name_field`, `dialogs` → `pub fn show_dialogs`, der Unit-Test `the_dialog_windows_are_not_drawn_while_a_prompt_is_open`)
- Create: `crates/planora-app/src/projects_window.rs` (Zustand und Navigation; Zeichnen kommt in Aufgabe 8)
- Modify: `crates/planora-app/src/projects_panel.rs` (behält bis Aufgabe 9 nur `show`, `show_enabled`, `root_field`, `draw_node`, `common_menu` und ruft `project_actions`), `crates/planora-app/src/workspace.rs` (`create_document`), `crates/planora-app/src/lib.rs`, `crates/planora-app/src/app.rs` (`undo_keys`: `project_actions::dialog_open`; `panels_mut` liefert weiter `&mut ProjectsUi`)
- Test: `crates/planora-app/tests/project_actions.rs` (neu; übernimmt aus `tests/projects_ui.rs` die Aktionstests `a_panel_action_with_a_bad_name_…`, `opening_a_document_…`, `renaming_moving_and_deleting_…`, `notes_are_not_lost_…` unverändert bis auf den Import), `crates/planora-app/tests/projects_window.rs` (neu)
- Modify (Tests, nur Import `projects_panel::` → `project_actions::`): `tests/dialogs.rs` (Zeilen 25, 385, 551–564), `tests/undo_keys.rs` (Zeilen 711–712); `tests/projects_ui.rs` behält bis Aufgabe 9 nur die vier Zeichentests des Felds.

**Interfaces:**
- Consumes: Task 3 `Scan`, `scan_library`, `tiles_at`, `Tile`, `TileKind`, `CANCELLED`; `jobs::{Job, Poll}`; `Workspace`.
- Produces:
```rust
// project_actions.rs (zusätzlich zu den verschobenen, unveränderten Funktionen)
pub const NO_LIBRARY: &str = "Es ist noch kein Projektverzeichnis gewählt. Wähle mit „Projektverzeichnis …“ den Ordner, in dem deine Projekte liegen sollen.";
pub const READ_ERROR_PREFIX: &str = "Der Projektordner lässt sich nicht lesen: ";
pub struct ProjectsUi { /* wie bisher ohne root_text; neu: new_project: Option<String> (Fenster), new_document: Option<(NodePath, String)> */ }
impl ProjectsUi {
    pub fn invalidate(&mut self); pub fn selected(&self) -> Option<&NodePath>;
    pub fn select(&mut self, path: Option<NodePath>);
    pub fn is_stale(&self, ws: &Workspace) -> bool;                  // Baum fehlt oder library_revision neu
    pub fn set_tree(&mut self, tree: Result<Vec<TreeNode>, String>, revision: u64);
}
pub fn create_document(ws: &mut Workspace, st: &mut ProjectsUi, parent: &NodePath, name: &str) -> Option<NodePath>; // anlegen, öffnen, wählen
#[doc(hidden)] pub fn request_new_project(st: &mut ProjectsUi, name: &str);
#[doc(hidden)] pub fn request_new_document(st: &mut ProjectsUi, parent: NodePath, name: &str);
pub fn dialog_open(st: &ProjectsUi) -> bool;    // jetzt auch new_project, new_document
pub fn show_dialogs(ctx: &egui::Context, ws: &mut Workspace, st: &mut ProjectsUi);  // nur ohne offene Rückfrage
// Fenster „Neues Projekt“ (key "project") und „Neues Dokument“ (key "document"): DialogSize::Prompt,
// fest, Text „In „{label}“:“ (Dokument) bzw. ohne Ort (Projekt), Feld, „Anlegen“ (auch Enter), „Abbrechen“.

// workspace.rs
impl Workspace {
    /// Neues leeres Dokument (Version 1 aus `AppState::new().snapshot()`, Kommentar "") in einem
    /// Projekt oder Ordner; dieselben Prüfungen und Meldungen wie `save_as`; `library_revision += 1`.
    pub fn create_document(&mut self, parent: &NodePath, name: &str) -> Result<NodePath, String>;
}

// projects_window.rs
pub const TITLE: &str = "Projekte";
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum ProjectsEvent { None, OpenSettings, Closed }
pub struct ProjectsWindow { pub ui: ProjectsUi, /* open, location, scan job, data, root_seen, … */ }
impl Default for ProjectsWindow;
impl ProjectsWindow {
    pub fn open(&mut self);                 // öffnet und liest neu (invalidate)
    pub fn close(&mut self);
    pub fn is_open(&self) -> bool;
    pub fn location(&self) -> &NodePath;    // leer = Projekte
    pub fn go_to(&mut self, at: NodePath);  // auch für die Pfadteile
    pub fn back(&mut self);                 // eine Ebene hinauf
    /// Startet bei Bedarf das Lesen (`is_stale`, anderes Verzeichnis) und holt fertige Ergebnisse;
    /// hält bis zum neuen Ergebnis die alten Kacheln. Ein anderes Verzeichnis setzt Ort und Auswahl zurück.
    pub fn update(&mut self, ws: &Workspace);
    pub fn wait_for_scan(&mut self, ws: &Workspace);   // Tests: blockiert bis fertig
    pub fn scan_state(&self) -> Option<&Result<Scan, String>>;
    pub fn tiles(&self) -> Vec<Tile>;       // Kacheln der aktuellen Ebene (leer, solange nichts gelesen)
    /// Doppelklick/Enter: Projekt oder Ordner → hinein; Dokument → `open_node` (Fenster bleibt offen).
    pub fn activate(&mut self, ws: &mut Workspace, path: &NodePath);
    pub fn apply_menu(&mut self, tile: &Tile, entry: &str);   // öffnet das passende vorhandene Fenster in `ui`
    pub fn new_project(&mut self);                 // Knopf „Neues Projekt“ → request_new_project(.., "")
    pub fn new_document(&mut self);                // Knopf „Neues Dokument“ in `location`
    pub fn new_folder(&mut self);                  // Knopf „Neuer Ordner“ in `location`
}
/// Einträge des Rechtsklick-Menüs: „Umbenennen …“, „Verschieben …“, „Löschen …“; ohne „Verschieben …“ bei Projekten.
pub fn tile_menu(kind: TileKind) -> &'static [&'static str];
```
Verschwindet der Ort (gelöscht, umbenannt von außen), geht `update` zum nächsten vorhandenen Elternteil (sonst oben).

- [ ] **Step 1: Failing tests** (`tests/projects_window.rs`; Temp-Bibliothek, `config_path` im Temp):
```rust
#[test] fn opening_reads_the_projects_in_the_background() { /* open; update; wait_for_scan → tiles() Namen sortiert; vor dem Warten blockiert update nicht (Zeitgrenze 200 ms bei 50 Projekten) */ }
#[test] fn double_click_on_a_project_goes_in_and_back_returns() { /* activate(P) → location [P]; back → [] */ }
#[test] fn activating_a_document_opens_a_tab_and_keeps_the_window_open() { /* activate(doc) → tabs mit path doc, is_open */ }
#[test] fn activating_an_open_document_only_brings_it_to_the_front() { /* tabs.len gleich, active = Tab des Dokuments */ }
#[test] fn every_panel_action_is_reachable_from_tiles() { /* apply_menu Umbenennen/Verschieben/Löschen → dialog_open; tile_menu(Project) ohne „Verschieben …“; new_project/new_document/new_folder öffnen ihr Fenster; Verschieben-Ziele (containers) kommen aus dem gelesenen Baum */ }
#[test] fn open_documents_lock_rename_move_and_delete() { /* Notice „Bitte zuerst die betroffenen Dokumente schließen.“ für Dokument und sein Projekt */ }
#[test] fn new_document_creates_version_1_opens_it_and_selects_the_tile() {
    // create_document → dokument.json mit einer Version, Tab mit path, ui.selected == Some(path), library_revision + 1
}
#[test] fn new_document_names_follow_the_folder_rules() { /* „..“, „a/b“, vorhandener Name → Notice, nichts angelegt */ }
#[test] fn a_save_as_elsewhere_shows_up_after_the_revision_changes() { /* save_as → update → neuer Scan, Dokument da */ }
#[test] fn a_vanished_location_falls_back_to_the_parent() { /* Ordner F extern löschen, Aktualisieren → location [P] */ }
#[test] fn changing_the_directory_resets_location_and_selection() { /* settings_window::change_root → location [], selected None, neue Kacheln */ }
#[test] fn without_a_library_there_is_no_scan() { /* Workspace::new(None, …) → scan_state None */ }
#[test] fn the_new_project_and_new_document_windows_are_not_drawn_under_a_prompt() { /* wie bisheriger Unit-Test, mit new_project */ }
```
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test projects_window --test project_actions` → FAIL.
- [ ] **Step 3: Implement** Verschieben (`git mv` nicht nötig: neue Datei, alte Teile entfernen), `create_document`, `ProjectsWindow`. `scan_library` über `jobs::Job::spawn`; Abbruch durch Fallenlassen des Jobs.
- [ ] **Step 4:** grün; dazu `--test dialogs --test undo_keys --test projects_ui --test workspace --test settings_window` grün (das Feld funktioniert unverändert).
- [ ] **Step 5: Mutationen (≥ 10):** `activate` schließt das Fenster; Dokument doppelt geöffnet (ohne `tab_for_path`); `create_document` ohne `library_revision`; ohne Öffnen; ohne Auswahl; `tile_menu(Project)` mit „Verschieben …“; `is_stale` ignoriert die Revision; anderes Verzeichnis setzt Ort nicht zurück; verschwundener Ort bleibt (leere Kacheln); `update` wartet auf den Scan; `dialog_open` ohne `new_document`; `may_change` umgangen.
- [ ] **Step 6: Commit** `refactor(app): Aktionen der Projekte in eigenes Modul, Zustand des Projekte-Fensters`.

---

### Task 8: Projekte-Fenster – Kacheln, Miniaturen, Layout

**Files:**
- Modify: `crates/planora-app/src/projects_window.rs` (Zeichnen), `crates/planora-app/src/theme.rs` (`pub fn primary_button(text: &str, dark: bool) -> egui::Button<'_>`: Füllung `accent`, Schrift `on_accent`, gleiche Größe in allen Zuständen)
- Test: `crates/planora-app/tests/projects_window_ui.rs` (neu), `crates/planora-app/tests/projects_window_layout.rs` (neu)

**Interfaces:**
- Consumes: Task 3 Texte und Kacheln; Task 5 `Thumb`, `ThumbKey`; Task 6 `ThumbWorker`; Task 7 alles; `thumbnail::{THUMB_W, THUMB_H}`; `Dialog::new(TITLE, DialogSize::File).id(egui::Id::new("planora_projects_window")).center_on_open().open(..).show_with_footer(..)`.
- Produces:
```rust
pub const TILE_MIN_W: f32 = 240.0; pub const TILE_H: f32 = 220.0; pub const TILE_GAP: f32 = theme::SPACE_3;
pub const PROJECTS_CHROME_H: f32 = 260.0;
pub const FOOTER_HINT: &str = "Doppelklick: öffnen · Rechtsklick: weitere Befehle";
pub const DIRECTORY_BUTTON: &str = "Projektverzeichnis …";
pub const NO_BODIES: &str = "Noch keine Körper"; pub const TOO_COMPLEX: &str = "Miniatur zu aufwendig";
pub fn columns_for(inner_width: f32) -> usize;
pub fn grid_height_for(window_height: f32) -> f32;
impl ProjectsWindow {
    /// Zeichnet das Fenster (nur wenn offen); Kacheln gesperrt bei offener Rückfrage.
    pub fn show(&mut self, ctx: &egui::Context, ws: &mut Workspace) -> ProjectsEvent;
    pub fn thumb_state(&self, doc_dir: &Path) -> Option<&Thumb>;   // Tests
    pub fn wait_for_thumbnails(&mut self, timeout: Duration) -> bool;  // Tests
}
```
Aufbau (oben nach unten): Ebene 1: Zeile „Neues Projekt“ (`primary_button`), Pfad (`file_chooser::display_path`, gekürzt), rechtsbündig `DIRECTORY_BUTTON` (→ `ProjectsEvent::OpenSettings`). Ebene ≥ 2: Pfad „‹ Projekte › P › F“ (‹ = `back`, jeder Teil `go_to`), „Neues Dokument“ (`primary_button`), „Neuer Ordner“. Ohne Bibliothek `NO_LIBRARY` + `DIRECTORY_BUTTON`; Lesefehler `READ_ERROR_PREFIX{e}` in Fehlerfarbe + `DIRECTORY_BUTTON`; leer: Leertext (Global Constraints); mehr als 500: `TOO_MANY`. Kachelbereich: eigene `ScrollArea` mit fester Höhe `grid_height_for`, Kacheln `allocate_exact_size([w, TILE_H], Sense::click())`, gemalt: Bildfläche 140 px (`bg_inset`; Miniatur `THUMB_W × THUMB_H` eingepasst, als Textur aus `ctx.load_texture`), Ordner: großes `Icon::Folder` in `theme::folder_icon_color`; Platzhalter: `NO_BODIES`, `TOO_COMPLEX`, ruhige Anzeige (`egui::Spinner`, feste Größe) solange angefragt; darunter Name (stark, gekürzt mit „…“), `changed_line`, `meta_line` (Warnfarbe bei `Tone::Warning`). Rand immer 2 px: gewählt `accent`, Überfahren `border_subtle`, sonst transparent; `theme::focus_ring`. Klick wählt, Doppelklick/Enter `activate`, Rechtsklick `tile_menu` → `apply_menu`; Knöpfe → `new_project`, `new_document`, `new_folder`. Fußzeile: links `footer_text`, rechts `FOOTER_HINT` und „Aktualisieren“ (`ui.invalidate()`). Miniaturen: je Bild `set_queue(sichtbare, übrige)` nur bei geänderter Menge; Schlüssel `ThumbKey::new(version, branch, dark)` mit `dark` des aktiven Tabs; Speicher je Dokumentordner (nicht je `NodePath`); `Failed`/nicht gespeichert bleibt bis zur nächsten Version; `show` wartet nie. Schließen (✕) → `ThumbWorker::cancel_pending`, `ProjectsEvent::Closed`. Danach `project_actions::show_dialogs`.

- [ ] **Step 1: Failing tests** `tests/projects_window_ui.rs` (App-lose Bilder mit `common::Screen`):
```rust
#[test] fn tiles_show_name_date_version_and_count() { /* „Gehäuse“, „Geändert {format_changed(..)}“, „Version 1 · 1 Dokument“ */ }
#[test] fn an_empty_library_shows_the_empty_text_and_no_library_shows_the_directory_button() { /* Leertext; NO_LIBRARY + DIRECTORY_BUTTON; kein „Wurzel“ */ }
#[test] fn click_selects_with_accent_border_and_double_click_goes_in() { /* Rand-Strich in palette.accent um die Kachel; Doppelklick → location */ }
#[test] fn the_breadcrumb_goes_back() { /* Klick „‹“ und Klick „Projekte“ → location [] */ }
#[test] fn right_click_menus_offer_the_spec_entries() { /* Projekt: Umbenennen …, Löschen …; Ordner/Dokument zusätzlich Verschieben … */ }
#[test] fn tab_reaches_every_tile_and_enter_opens_it() { /* echte Tab-Tasten bis zur 2. Dokumentkachel, Enter → Tab geöffnet */ }
#[test] fn new_project_button_creates_and_selects() { /* Klick „Neues Projekt“, Name tippen (ein Zeichen je Bild), Enter → Kachel da, gewählt */ }
#[test] fn new_document_button_creates_opens_and_shows_the_tile() { /* Ebene 2: „Neues Dokument“ → Tab offen, Kachel „Version 1“ */ }
#[test] fn thumbnails_arrive_without_blocking_and_show_as_texture() { /* nach wait_for_thumbnails: Shape::Mesh mit Textur in der Bildfläche der Kachel */ }
#[test] fn the_frame_never_waits_for_a_slow_thumbnail() { /* Arbeiter mit Sperre (start_with über #[doc(hidden)] set_worker) → show() < 50 ms, Spinner sichtbar */ }
#[test] fn placeholders_name_no_bodies_and_too_complex() { /* leeres Dokument → NO_BODIES + „Noch keine Körper · 1 Dokument“; TooComplex → TOO_COMPLEX */ }
#[test] fn unreadable_documents_are_shown_in_warning_colour() { /* „Dokument nicht lesbar“ in palette.warning */ }
#[test] fn refresh_rereads_external_changes() { /* Projekt außerhalb anlegen; Klick „Aktualisieren“ → Kachel da */ }
#[test] fn a_prompt_disables_the_tiles() { /* ws.prompt = Notice; Doppelklick → location unverändert */ }
#[test] fn the_directory_button_asks_for_the_settings() { assert_eq!(event, ProjectsEvent::OpenSettings); }
#[test] fn closing_stops_pending_thumbnails() { /* ✕ → Closed, is_open false, Arbeiter-Warteschlange leer */ }
#[test] fn a_failed_store_is_not_retried_in_this_session() { /* Ordner mit Verzeichnis „vorschau.json“: Arbeiter zählt Aufträge → genau 1 über 20 Bilder und ein „Aktualisieren“; Kachel zeigt die Miniatur aus dem Speicher */ }
#[test] fn a_theme_change_requests_new_thumbnails() { /* dark umschalten → neuer Schlüssel angefragt, vorschau.json danach "dark":false */ }
```
`tests/projects_window_layout.rs`:
```rust
#[test] fn four_columns_at_1100_and_widths_follow_the_window() { assert_eq!(columns_for(innen_bei_dialog_1100), 4); /* 2000×1100 → Dialog 1100 → 4 Spalten gezeichnet */ }
#[test] fn layout_holds_at_all_sizes_with_0_1_and_200_tiles() {
    // je 800×600, 1400×900, 2000×1100 × {0, 1, 200 Projekte} × {hell, dunkel}:
    // Dialog ≤ 1200 breit und ganz im Fenster; Fußzeile („Aktualisieren“) sichtbar und nicht abgeschnitten;
    // alle Kachelrechtecke gleich groß (±0,5); Kacheln einer Reihe gleiche Oberkante; nichts ragt rechts über den Dialog
}
#[test] fn hover_and_arriving_thumbnails_move_nothing() { /* Rechtecke aller Kacheln und des Dialogs vor/nach Überfahren und nach Eintreffen der Miniaturen gleich */ }
#[test] fn long_names_stay_inside_their_tile() { /* Name mit 80 Zeichen: Text endet vor dem rechten Kachelrand, endet mit „…“ */ }
#[test] fn the_window_does_not_jump_when_tiles_arrive() { /* Dialoghöhe vor dem Scan == danach */ }
#[test] fn more_than_500_projects_show_the_hint() { /* 501 Projekte → TOO_MANY sichtbar */ }
```
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test projects_window_ui --test projects_window_layout` → FAIL.
- [ ] **Step 3: Implement.** Für Tests: `#[doc(hidden)] pub fn set_worker(&mut self, w: ThumbWorker)`. Passt `PROJECTS_CHROME_H` bei 800 × 600 nicht, den Wert anheben (der Layout-Test entscheidet) und im Plan unter „Abweichungen“ notieren.
- [ ] **Step 4:** grün; dazu `--test no_hard_coded_colors --test no_raw_windows --test no_raw_text_edits --test icons --test dialog_frame` grün.
- [ ] **Step 5: Mutationen (≥ 10):** Spaltenzahl `ceil`; Kachelhöhe aus dem Inhalt; Rand nur bei Auswahl (Hover verschiebt); Kachelbereich ohne feste Höhe (Fenster springt); Name nicht gekürzt; Warnfarbe fehlt; `set_queue` ohne sichtbare zuerst; `show` ruft `wait_idle`; Doppelklick auf Dokument schließt das Fenster; Kacheln bei Rückfrage nicht gesperrt; Schlüssel ohne `dark`; Speicher nach `NodePath` (gleichnamige Projekte nach Verzeichniswechsel zeigen alte Bilder); Enter ohne Fokusprüfung.
- [ ] **Step 6: Commit** `feat(app): Projekte-Fenster mit Kacheln und Miniaturen`.

---

### Task 9: Menü, Strg+O, Einstellungen, Schließregeln und Entfernen des Felds

**Files:**
- Modify: `crates/planora-app/src/app.rs` (Felder `projects: ProjectsWindow`, `settings: SettingsWindow` statt `projects_ui`; `MenuRequest`; `file_menu`, `menu_bar`, `menu_bar_measured`; `shortcuts` (Strg+O); `undo_keys`; `shell_toggles` nur Versionen; `show_windows`; `enforce_window_rules`; `open_projects`, `open_settings`; `panels_mut`, neu `windows_mut`; Kommentar Zeile 918), `crates/planora-app/src/shell.rs` (`projects_visible`, `show_projects` entfernen), `crates/planora-app/src/shell_panels.rs` (Abschnitt „Projekte“ in `left_field` und `app.show_projects()` entfernen, `MenuRequest` auswerten; Modulkommentar), `crates/planora-app/src/icons.rs` (`Icon::Open`, `Icon::PanelProjects` entfernen, `Icon::Settings` = `'\u{e272}'`, `"gear-six"` in `ALL` und `TABLE`), `crates/planora-app/src/prompts.rs` (Zeile 197 → „Zuerst im Fenster Projekte ein Projektverzeichnis wählen.“, Zeile 210 → „Noch kein Projekt vorhanden. Zuerst im Fenster Projekte ein Projekt anlegen.“), `crates/planora-app/src/workspace.rs` (Kommentar `library_revision`: „das Fenster Projekte“), `crates/planora-app/src/lib.rs` (`projects_panel` entfernen)
- Delete: `crates/planora-app/src/projects_panel.rs`, `crates/planora-app/tests/projects_panel_width.rs`, `crates/planora-app/tests/projects_ui.rs` (deren Zeichentests sind durch Aufgabe 8 ersetzt; vorher prüfen, dass jede Aussage dort eine Entsprechung hat, Liste im Commit-Text)
- Modify (Tests): `tests/shell_app.rs` (`view_menu_toggles_projects_and_versions_fields` → nur „Versionen“ und „Projekte“ steht nicht im Menü Ansicht; `ctrl_o_shows_the_projects_field` → `ctrl_o_opens_the_projects_window`; Zeilen 721–724 Strg+O → `windows_mut().1.is_open()`), `tests/shell_panels.rs` (`view_menu_toggles_projects_and_versions_in_the_shell` → nur Versionen; `left_field_shows_projects_and_tree_…` → `left_field_shows_only_the_tree`), `tests/shell_layout.rs` (`show_projects_makes_projects_visible` entfernen), `tests/graphit_style.rs` (Menüliste Zeile 205 „Projekte …“ und „Einstellungen …“; Zeilen 761–775: Klick „Projekte …“ liefert `MenuRequest::Projects`), `tests/dialogs.rs` (Zeile 196 `show_projects()` entfernen; neue Fälle „Projekte“ (File, schließbar), „Einstellungen“ (File, schließbar), „Neues Projekt“, „Neues Dokument“ (Prompt, fest, Id `("planora_projects_dialog", "project"/"document")`)), `tests/undo_keys.rs` (Zeilen 914–916 Kommentar „Strg+O öffnet das Fenster Projekte“), `tests/file_places_wiring.rs` (Zeile 270 ohne `Icon::PanelProjects`)
- Create (Tests): `crates/planora-app/tests/projects_menu.rs`, `crates/planora-app/tests/no_root_word.rs`

**Interfaces:**
- Consumes: Task 2 `SettingsWindow`, `SettingsEvent`; Task 7/8 `ProjectsWindow`, `ProjectsEvent`; `show_file_dialogs`.
- Produces:
```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub enum MenuRequest { #[default] None, Projects, Settings }
pub const PROJECTS_ENTRY: &str = "Projekte …"; pub const SETTINGS_ENTRY: &str = "Einstellungen …";
pub fn file_menu(ui: &mut egui::Ui, ws: &mut Workspace) -> MenuRequest;   // „Projekte …“ (Icon::Projects, Strg+O) an Stelle von „Öffnen …“; „Einstellungen …“ (Icon::Settings) vor „Über PlanoraCAD“
pub fn menu_bar(ui: &mut egui::Ui, ws: &mut Workspace, shell: &mut ShellState) -> MenuRequest;
/// P10: schließt die Dateifenster des aktiven Tabs (nicht ein schreibendes Export-Fenster; dann
/// bleibt Projekte zu) und Einstellungen; öffnet Projekte.
pub fn open_projects(ws: &mut Workspace, projects: &mut ProjectsWindow, settings: &mut SettingsWindow);
pub fn open_settings(ws: &mut Workspace, projects: &mut ProjectsWindow, settings: &mut SettingsWindow, return_to_projects: bool);
/// P10: Dateifenster des aktiven Tabs offen → Projekte und Einstellungen schließen.
pub fn enforce_window_rules(ws: &Workspace, projects: &mut ProjectsWindow, settings: &mut SettingsWindow);
/// Nach `show_file_dialogs`, vor den Rückfragen: Regeln, Projekte (Ereignisse), Einstellungen
/// (`ws.places()`/`ws.save_places`, Rückweg nach Projekte).
#[doc(hidden)] pub fn show_windows(ctx: &egui::Context, ws: &mut Workspace, projects: &mut ProjectsWindow, settings: &mut SettingsWindow);
impl PlanoraApp {
    #[doc(hidden)] pub fn windows_mut(&mut self) -> (&mut Workspace, &mut ProjectsWindow, &mut SettingsWindow);
    #[doc(hidden)] pub fn panels_mut(&mut self) -> (&mut Workspace, &mut ProjectsUi, &mut VersionsUi);   // ProjectsUi = projects.ui
}
```
`undo_keys` sperrt wie bisher bei `project_actions::dialog_open(&self.projects.ui)`; das Fenster Projekte und Einstellungen selbst sperren Rückgängig nicht.

- [ ] **Step 1: Failing tests** `tests/projects_menu.rs` (ganze App über `PlanoraApp::for_tests`, `config_path` im Temp):
```rust
#[test] fn the_file_menu_has_projects_with_ctrl_o_and_settings() { /* „Projekte …“ + „Strg+O“ in einer Zeile, „Einstellungen …“, kein „Öffnen …“; Symbole Icon::Projects / Icon::Settings */ }
#[test] fn ctrl_o_opens_the_projects_window_everywhere() { /* Modell, Zeichnung, Skizze offen: Strg+O → windows_mut().1.is_open(); Dokument unverändert (same_content) */ }
#[test] fn the_view_menu_has_no_projects_entry_and_the_left_field_no_projects_section() { /* Ansicht: „Versionen“ ja, „Projekte“ nein; linkes Feld: „Feature-Baum“, kein „Projekte“, kein „Noch keine Projekte“ – Modell und Zeichnung */ }
#[test] fn settings_from_the_menu_open_without_way_back() { /* Klick „Einstellungen …“ → settings offen, projects zu; Schließen → projects bleibt zu */ }
#[test] fn the_directory_button_goes_to_settings_and_back() { /* Projekte → DIRECTORY_BUTTON → Einstellungen offen, Projekte zu; „Schließen“ → Projekte wieder offen */ }
#[test] fn opening_projects_closes_import_windows_but_never_a_writing_export() { /* Netz-Import offen + Strg+O → import_dialog None, Projekte offen; schreibendes Export (wie `tests/undo_flow.rs` Zeilen 977–988: `set_writer(slow_writer)` + `write_to`) + Strg+O → Export bleibt, Projekte zu */ }
#[test] fn a_file_window_of_the_active_tab_closes_projects_and_settings() { /* Projekte offen; Datei → Netz importieren … → Projekte zu; Tabwechsel auf Tab mit offenem STEP-Import → Projekte zu */ }
#[test] fn after_a_directory_change_the_window_shows_the_new_projects() { /* Projekte offen mit A; über Einstellungen auf leeres Verzeichnis mit B → nach Rückweg Kachel „B“, kein „A“ */ }
#[test] fn opening_a_document_from_the_window_switches_the_tab() { /* Doppelklick Dokumentkachel → ws.active zeigt den Tab, Projekte offen */ }
#[test] fn save_as_points_to_the_projects_window() { /* Speichern unter ohne Bibliothek: „Zuerst im Fenster Projekte ein Projektverzeichnis wählen.“ */ }
#[test] fn undo_is_blocked_by_project_dialogs_but_not_by_the_windows() { /* Projekte offen: Strg+Z wirkt; „Löschen …“-Rückfrage offen: Strg+Z wirkt nicht */ }
```
`tests/no_root_word.rs` (Wächter wie `no_raw_windows.rs`): `no_ui_text_says_wurzel_or_feld_projekte` – in allen `src/**/*.rs` kein Zeichenkettenliteral (außerhalb von Kommentarzeilen) enthält „Wurzel“, „Feld Projekte“, „Feld „Projekte““ oder „Öffnen …“; dazu `the_scanner_finds_literals_and_skips_comments`.
- [ ] **Step 2:** `cargo test -p planora-app -j 2 --test projects_menu --test no_root_word` → FAIL.
- [ ] **Step 3: Implement** Interfaces und Entfernungen. Danach `grep -rn "projects_panel\|projects_visible\|show_projects\|PanelProjects\|Icon::Open\b" crates/planora-app` → keine Treffer.
- [ ] **Step 4:** `cargo test -p planora-app -j 2` vollständig grün (alle angepassten Tests, `icons`, `fonts`, `dialogs`, `dialog_frame`, `graphit_style`).
- [ ] **Step 5: Mutationen (≥ 10):** Strg+O öffnet nichts; Menü meldet `Settings` bei „Projekte …“; Rückweg immer; Rückweg nie; Export beim Schreiben geschlossen; Import bleibt bei Strg+O offen; `enforce_window_rules` fehlt (Tabwechsel-Test rot); `undo_keys` sperrt bei offenem Fenster; Einstellungen ohne `save_places` (Favoriten der Ordnerauswahl gehen verloren); Ansicht-Häkchen „Projekte“ wieder da; Prompt-Text „Feld Projekte“; Wächter überspringt keine Kommentare (Selbsttest rot).
- [ ] **Step 6: Commit** `feat(app): Datei → Projekte … und Einstellungen …, Feld Projekte entfällt`.

---

### Task 10: Dokumentation und Abschlussprüfung

**Files:**
- Modify: `README.md` (Abschnitt „Speichern, Projekte und Versionen“: Zeile 427 → Projektverzeichnis in „Datei → Einstellungen …“, Zeile 441 → Fenster „Projekte“ (Datei → Projekte …, Strg+O) mit Kacheln, Doppelklick, Rechtsklick, „Neues Dokument“; ein Satz zu `vorschau.png`/`vorschau.json`: Zusatzdateien, jederzeit löschbar; Zeiten in UTC wie im Reiter „Versionen“), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt: Projekte-Fenster und Einstellungen“: Geliefertes, P1–P12, Grenzen: Ortszeit fehlt (P1), Aufbau großer Dokumente nicht abbrechbar (§9a.3), Miniatur entsteht erst beim Öffnen des Fensters, keine Suche/Sortierung/Mehrfachauswahl/Ziehen, Pfeiltasten fehlen; Zahlen; Handliste unten als Abhakliste), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)
- Keine Code-Dateien.

- [ ] **Step 1: Texte schreiben** (Laiensprache, kein „Wurzel“).
- [ ] **Step 2: Abschluss-Gates** (alle im Vordergrund, `-j 2`):
  - `cargo fmt --all --check` → keine Ausgabe
  - `cargo clippy --workspace --all-targets -j 2 -- -D warnings` → ohne Warnung
  - `cargo test --workspace -j 2` → alles grün (Zahl der Tests in den Bericht)
  - `cargo deny check` → grün mit den Ausnahmen aus `deny.toml`; die zbus-Familie und ttf-parser erneut prüfen und das Ergebnis berichten
  - `git diff --stat 0bd0743 -- Cargo.toml Cargo.lock deny.toml crates ':!crates/planora-app'` → leer
  - `grep -rn "unsafe" crates/planora-app/src` → keine neuen Treffer gegenüber `0bd0743`
  - `git status --short` → leer; nichts im Wurzelordner
- [ ] **Step 3: Mutationssumme** aller Aufgaben (Zahl, keine grüne offen) in den Bericht.
- [ ] **Step 4: Commit** `docs: Projekte-Fenster und Einstellungen beschrieben, Handliste`.

**Handliste für den Nutzer (echtes Fenster; Agents starten die App nicht):**
1. Datei-Menü: „Projekte …“ mit Strg+O und „Einstellungen …“ da, „Öffnen …“ weg; Ansicht ohne „Projekte“; links nur noch Feature-Baum (Modell, Zeichnung, Skizze).
2. Strg+O: Fenster „Projekte“ mittig, ≈ 55 % breit, Kacheln mit Miniatur, „Geändert …“, „Version n · k Dokumente“ für deine echten Projekte; Uhrzeit in Ortszeit, gleich wie im Reiter „Versionen“ (P1).
3. Miniaturen erscheinen nach und nach (sichtbare zuerst), das Fenster bleibt bedienbar und springt nicht; zweites Öffnen zeigt sie sofort.
4. Doppelklick auf ein Projekt → Pfad „‹ Projekte › Name“; ‹ und „Projekte“ führen zurück; Doppelklick auf Ordner geht hinein.
5. Doppelklick auf ein Dokument öffnet es im Tab, das Fenster bleibt offen; noch einmal → nur nach vorn geholt.
6. Rechtsklick: Umbenennen, Verschieben (nicht bei Projekten), Löschen mit den gewohnten Rückfragen; bei offenem Dokument „Bitte zuerst die betroffenen Dokumente schließen.“
7. „Neues Projekt“, „Neuer Ordner“, „Neues Dokument“ (öffnet sofort einen Tab, Kachel „Version 1“).
8. Tab-Taste läuft über die Kacheln (Fokusring), Enter öffnet.
9. „Projektverzeichnis …“ → Einstellungen; „Projektverzeichnis ändern …“ → Ordnerauswahl mit Schnellzugriffen; „Abbrechen“ ändert nichts. Mit offenem gespeichertem Dokument: Warnung in Warnfarbe. Mit allen Dokumenten geschlossen: Wechsel auf einen Testordner, Projekte-Fenster zeigt nur dessen Projekte; zurück auf `Dokumente\PlanoraCAD`, alles wieder da (nichts verschoben).
10. Hell/Dunkel umschalten, Fenster wieder öffnen: Miniaturen werden neu gezeichnet, beide Designs gut lesbar.
11. Fenstergröße 800 × 600: Kopf, Kacheln (rollen), Fußzeile mit „Aktualisieren“ sichtbar; großes Fenster: mehr Spalten, alle Kacheln gleich groß.
12. Netz importieren / STEP importieren / Exportieren öffnen, während Projekte offen ist: Projekte schließt; Strg+O bei offenem Import schließt den Import.
13. Ein Dokumentordner im Explorer: `vorschau.png` und `vorschau.json` liegen neben `dokument.json`; beide löschen und Fenster neu öffnen → werden neu angelegt.
14. Ein großes Dokument (z. B. großer Netz-Körper): Kachel „Miniatur zu aufwendig“, Oberfläche bleibt flüssig.

---

## Abweichungen bei der Umsetzung

(wird während der Ausführung gefüllt)

- **Aufgabe 8:** `PROJECTS_CHROME_H` bleibt 260 (passt bei 800 × 600). `ThumbKey::new` braucht seit
  Aufgabe 5 die Kennung des Dokuments: `project_tiles::Scan` hat dafür `ids` (Kennung je lesbarem
  Dokument, beim Lesen mit `peek` gemerkt; `scan_tree` lässt sie leer). Die Zeichen „‹“ und „›“
  fehlen in den Schriften (Test `fonts`): der Pfad nutzt die Phosphor-Symbole `caret-left`
  (`Icon::Back`, U+E138) und `caret-right` (`Icon::PathSeparator`, U+E13A). Die Namensfelder der
  Fenster „Neues Projekt“, „Neues Dokument“, „Neuer Ordner“ und „Umbenennen“ bekommen im ersten
  Bild den Tastaturfokus (gleich lostippen). egui 0.36 malt Bilder als Rechteck mit Textur, nicht
  als `Shape::Mesh`; der Test prüft beides.
- **Aufgabe 8, Prüfrunde 1:** Über dem Kachelbereich stehen zwei Zeilen (auf jeder Ebene gleich
  hoch): oben der Pfad (Ebene 1: Projektverzeichnis; tiefer „‹ Projekte › … › F“, vordere Teile
  fallen zu „…“ zusammen, der letzte bleibt, notfalls gekürzt), rechts „Projektverzeichnis …“
  (Ebene 1 und bei Lesefehler); darunter „Neues Projekt“ bzw. „Neues Dokument“/„Neuer Ordner“.
  Der Pfad steht direkt unter der Titelzeile des Dialogs, nicht in ihr (dialog.rs bleibt
  unverändert). Kacheln als Karte (`bg_section`, 1 px `border_subtle`, gewählt Akzentrand und
  `theme::card_fill(dark, true)`), Fokus als eigener Ring außen. Statt Spinner der ruhige Text
  „Wird gerade erzeugt …“. Texturen nur für Kacheln bis ± 2 Höhen um den Ausschnitt, höchstens
  64 (`MAX_TEXTURES`); Pixel gespeicherter Miniaturen werden nach dem Hochladen verworfen und bei
  Bedarf über die Warteschlange neu geholt. Fußzeile über 500: „500 von 600 Projekten“.
