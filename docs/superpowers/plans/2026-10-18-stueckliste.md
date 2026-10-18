# Stückliste und Positionsnummern (Teilprojekt 3c-2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **COMMIT-TRAILER (jeder Commit, ohne Ausnahme, wörtlich als letzte Zeile):**
> `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
> Frühere Agents haben hier wiederholt einen anderen Namen eingesetzt. Vor jedem `git commit` die Zeile prüfen.
> Immer nur die genannten Pfade einzeln `git add`en (nie `git add -A`/`.`): der unverfolgte Ordner `assets/` im Repo darf nie in einen Commit.

**Goal:** Jeder Körper des Projekts wird zu einer Zeile einer Stückliste, die auf dem Zeichnungsblatt (Schalter je Blatt) und auf automatisch mitwachsenden Stücklistenblättern steht; der Nutzer setzt Positionsnummern per Klick auf ein Teil, die über Umbenennen, Verschieben, Verrunden und Boolesche Operationen am Teil bleiben, verloren gehen, wenn es verschwindet, und mit der Stückliste in SVG/DXF und Dateiformat 7 ausgegeben werden.

**Architecture:** `planora-drawing` bekommt die reinen Bausteine: `parts.rs` (Körperkennung `BodyKey`, Zeilen, Überschreibungen, Auflösung zu Zelltexten, Seitenrechnung), Tabellengeometrie in `sheet_frame.rs` (`PartsSpace`, `sheet_layout_with`), Tabellenmarkierungen in `sheet_marks.rs` und `balloon.rs` (Nummernkreise als `Mark` mit `MarkKind::Position`). `planora-feature` leitet Kennung und Stückliste aus dem Feature-Baum ab, speichert die Zeilentexte je Kennung im Dokument, verwaltet die Stücklistenblätter als echte Zeichnungen (`DrawingKind::PartsList`) und die Nummernkreise. `planora-project` speichert alles in Format 7. Die App zeigt die Tabelle, reserviert ihren Platz, erweitert „Blattangaben“ um den Abschnitt „Stückliste“, bekommt das Werkzeug „Position“ samt Browser-Abschnitt und gibt alles beim Export aus.

**Tech Stack:** Rust (Workspace, keine neuen Abhängigkeiten), egui/eframe 0.36, serde_json (vorhanden); Tests im eigenen Harness (`egui::Context::run_ui`, `PlanoraApp::for_tests`), SVG/DXF-Leser in `crates/planora-drawing/tests/common/`.

**Spec:** `docs/superpowers/specs/2026-10-18-stueckliste-design.md` (bindend, mit §9 Festlegungen F1–F6). Vorbild für Stil, Skalierung und Formaterweiterung: `docs/superpowers/plans/2026-10-17-zeichnungsblatt.md` (3c-1, mit „Abweichungen bei der Umsetzung“), `docs/superpowers/specs/2026-10-17-zeichnungsblatt-design.md`, `docs/superpowers/specs/2026-10-15-masse-mittellinien-design.md` (verlorene Maße, Exportsperre, Browser). Basis: `master` @ `84148e4`; Arbeitszweig `feature/stueckliste`.

## Global Constraints

- Texte deutsch in einfacher Laiensprache, keine technischen Hinweise, keine Hinweise auf Datenherkunft (Nutzerregel); feste Spaltennamen „Pos.“, „Benennung“, „Menge“, „Werkstoff“, „Bemerkung“ (Spec §3).
- Farben nur über `theme::` (Wächter `tests/no_hard_coded_colors.rs`); Einzeilen-Textfelder nur über `theme::text_field` (Wächter `no_raw_text_edits`); Fenster nur über `dialog::Dialog` (Wächter `no_raw_windows`); Zeichen nur aus Montserrat/Symbolschrift (`tests/fonts.rs`).
- Das Blatt bleibt in der App weißes Papier (`theme::PAPER`) mit schwarzer Zeichnung (`theme::SHEET_INK`), Hell und Dunkel.
- Alle Blattmaße sind A3-Werte mal Faktor `f = lange Blattseite / 420` (`SheetLayout::scale`, `sheet_scale`); Linien mal `f`, nie unter `MIN_PEN_MM = 0.18` (wie 3c-1). Das gilt für Tabelle (Zeile 7, Schrift 3,5, Spalten) und Nummernkreis (Ø 8, Schrift 3,5, Kreislinie 0,35, Hinweislinie 0,25).
- Grenzen (Spec §3, §6): höchstens 10 Zeilen auf dem Zeichnungsblatt; höchstens 200 Zeilen auf den Stücklistenblättern insgesamt; höchstens 200 Nummernkreise je Zeichnung; Benennung, Menge, Werkstoff höchstens 60 Zeichen, Bemerkung 80; Einpassen wie 3c-1 (`fit_text`: verkleinern bis 3/5, dann „…“).
- Automatische Anordnung hält Platz für die volle Stückliste (10 Zeilen + Kopf) frei, solange der Schalter an ist (Muster `placement_layout` mit `REV_VISIBLE`).
- Werkzeugfeld: bei 1100 × 700 kein Rollbalken (bestehender Test `drawing_fields.rs` bleibt grün).
- Dateiformat: `FORMAT_VERSION = 7`; Format 7 nur mit Stücklistendaten (Überschreibungen, Schalter an, Stücklistenblatt) oder Nummernkreisen; sonst bleibt jede Datei Byte für Byte wie bisher (Format 6/5/4…); Öffnen ändert nichts.
- Geschützte Pfade unverändert gegenüber `84148e4`: `crates/planora-kernel/**`, `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`. Keine neuen Abhängigkeiten; `cargo deny check` sauber mit den in `deny.toml` vermerkten Ausnahmen (drei zbus-Hinweise bewusst ignoriert, mit Prüfvermerk; nicht anfassen).
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests mit `-j 2` im Vordergrund (langsamer Rechner; nie `cargo clean`; keine Hintergrundprozesse zurücklassen). Am Ende `cargo test --workspace -j 2` und `cargo deny check`.
- Commits: nur genannte Pfade einzeln `git add`; `assets/` nie; Trailer siehe Kasten oben.
- Abweichungen von der Spec stehen im Ledger des Controllers und in `docs/superpowers/offene-punkte.md`; Zahlen der Spec ändern sich nur nach Zustimmung des Nutzers.

**Festlegungen des Plans (G1–G15; der Nutzer kann sie kippen):**
- **G1 Körperkennung** `BodyKey { feature: u64, index: u32 }` = Ursprungs-Körper (F1): `Document::body_key(r)` folgt `Move.body` und `Verrunden.body`; bei `Boolean` folgt Ergebnis-Index 0 dem `target`, Index ≥ 1 ist ein eigener Körper `(boolean, index)`; sonst Ende. Schleife höchstens `features().len()` Schritte. Der Werkzeug-Körper einer Booleschen Operation ist danach nicht mehr sichtbar → seine Kreise sind verloren.
- **G2 Benennung** = Anzeigename des Körpers nach der heutigen Regel (`state.rs::body_name`, folgt nur Verschieben/Verrunden; das Ergebnis einer Booleschen Operation heißt wie das Feature). Die Funktion zieht unverändert nach `planora-feature` (`Document::body_display_name`). Siehe offene Frage Q1.
- **G3 Zeilen und Texte:** Zeilen = `visible_bodies()` (Netz-Körper sind dort nicht enthalten, F6) gruppiert nach Anzeigename in Reihenfolge der ersten Fundstelle; Pos. ab 1; Schlüssel einer Zeile = `BodyKey` ihres ersten Körpers. Überschreibungen (`RowOverride`) liegen im Dokument unter diesem Schlüssel; ganz automatische werden entfernt; Einträge für gerade unsichtbare Körper bleiben (Unterdrücken und Zurückholen stellt sie wieder her); höchstens 1000 Einträge. Werkstoff automatisch = Projektangabe „Werkstoff“ (`project_info.material.resolve("–")`), ohne Projektangaben „–“; Bemerkung automatisch leer; Menge automatisch die Zahl; Pos. nie überschreibbar.
- **G4 Spalten** (A3, Summe 180 = Schriftfeldbreite): Pos. 12 | Benennung 78 | Menge 15 | Werkstoff 45 | Bemerkung 30; Zeilenhöhe `REV_ROW_H` 7, Schrift `VALUE_H` 3,5, Linien `Thin`, Text bei `x0 + 1`, Grundlinie `Zeilenunterkante + (7 − 3,5)/2`, Breite `Spalte − 2` (alles · f). Auf dem Stücklistenblatt dieselben Anteile auf die volle Rahmenbreite gestreckt.
- **G5 Lage auf dem Zeichnungsblatt** (`PartsSpace::OnSheet { rows }`): Tabelle `(title.x0, title.y1) – (title.x1, title.y1 + 7f · (1 + min(rows, 10)))`, Kopfzeile unten, Pos. 1 direkt darüber, aufsteigend nach oben. Hinweisfeld neben dem Schriftfeld bleibt, wo es ist; steht es darüber, rückt es über die Tabelle. Freier Bereich nach der 3c-1-Regel (F8) mit der Tabelle als Hindernis (Stapeloberkante = Tabellen- bzw. Hinweisoberkante; Tabellenkante links zählt bei Kandidat b). Anordnung/Maßstabsvorschlag nehmen `rows = 10`, gezeichnet wird mit der tatsächlichen Zeilenzahl (≤ 10).
- **G6 Stücklistenblatt** (`PartsSpace::ListPage`): Hinweisfeld gibt es nicht (Hinweiszeilen zählen 0, der Satz eines neuen Blatts bekommt `notes = Lines([])`, das Fenster zeigt keinen Hinweis-Editor); Änderungstabelle immer mit `REV_VISIBLE` reserviert; Tabelle `(fx0, title.y1) – (fx1, Unterkante der reservierten Änderungstabelle)`, Kopf oben, Zeilen nach unten; gezeichnet werden Kopf und gefüllte Zeilen. Zeilen je Blatt `parts_rows_per_page(sheet) = floor(Höhe / 7f + 1e-9) − 1` (hoch 44, quer 26 auf jedem Papier).
- **G7 Seiten:** Blätter heißen „Stückliste“, „Stückliste (2)“, …; Kind `DrawingKind::PartsList { page }`; benötigt `pages_needed(min(Zeilen, 200), pro_Blatt)` (mindestens 1). `Document::sync_parts_sheets()` legt fehlende Folgeblätter direkt hinter dem letzten Stücklistenblatt an (Papier, Ausrichtung, Projektion von Seite 1; Satz mit `created` von Seite 1, `notes = Lines([])`) und entfernt überzählige von hinten; ein geänderter Blattformat von Seite 1 geht auf die Folgeblätter über. Aufgerufen am Ende von `recompute()`, in `rename()`, in `add_parts_sheets()` und nach `update_drawing()` an einem Stücklistenblatt; **nicht** beim Laden (`from_snapshot`), damit Öffnen nichts ändert. Seite 1 löschen entfernt alle (eine Änderung); Folgeblatt löschen und jedes Stücklistenblatt umbenennen wird mit benannter Meldung abgelehnt. Es gibt höchstens einen Satz Stücklistenblätter je Dokument.
- **G8 Schalter** `SheetInfo.parts_list: bool` (Vorgabe `false`); `add_drawing_stamped` setzt ihn auf `visible_bodies().len() > 1` (F4). Gespeichert wird er nur, wenn `true` (nur dann Format 7); alte Zeichnungen haben `false`.
- **G9 Nummernkreis-Daten:** `Balloon { id, view, body: BodyKey, anchor: (u, v), offset: (dx, dy) }`; `anchor` ∈ [0, 1]² relativ zur Hüllbox der Ansicht (`ViewLines.bbox`, F2), beim Setzen auf [0, 1] begrenzt; `offset` in Blatt-mm vom Anker zur Kreismitte, Vorgabe `(10, 10) · f` (schräg rechts oben). Ansicht löschen löscht ihre Kreise.
- **G10 Nummernkreis-Zeichnung** (A3 · f): Kreis Ø 8 (`PositionPen::Circle` 0,35), Zahl 3,5 mittig (Grundlinie `Mitte.y − h/2`, in `0,8 · Ø` eingepasst), Hinweislinie (`PositionPen::Leader` 0,25) vom Anker bis zum Kreisrand (entfällt, wenn der Anker im Kreis liegt), Punkt am Teil als Kreis r 0,5 (`Leader`). Verloren: dieselbe Geometrie in `MarkKind::Lost`, Text „?“.
- **G11 Werkzeug „Position“** in Zeile 2 des Werkzeugfelds direkt nach dem Auswahlfeld des Maß-Werkzeugs; nur wenn der Test „kein Rollbalken bei 1100 × 700“ dann rot wird, ans Ende von Zeile 1 (nach „Blattangaben“). Werkzeuge „Maß“ und „Position“ schließen sich aus. Kreise lassen sich auch ohne Werkzeug anklicken (wählt) und ziehen (verschiebt den Kreis, der Anker bleibt); Entf löscht den gewählten Kreis (nicht beim Tippen in einem Feld); Esc beendet das Werkzeug bzw. hebt die Auswahl auf. „Neu zuweisen“ = löschen und neu setzen.
- **G12 Ausgabe:** Nummernkreise gehören zu den Ansichten (in `SheetView.marks`) und werden immer geschrieben; die Tabelle gehört auf Zeichnungsblättern zum Blattrahmen (nur mit „Blattrahmen mit ausgeben“), auf Stücklistenblättern ist sie der Inhalt und wird immer geschrieben (ohne Schalter nur die Tabelle, in derselben SVG-Gruppe `Blattrahmen`). DXF-Ebenen `POSITIONEN` (Farbe 7, Strichstärke 35) und `POSITIONEN_LINIE` (Farbe 7, 25), nur angelegt, wenn benutzt.
- **G13 Exportsperre:** ein verlorener Nummernkreis → „Die Positionsnummer in der Ansicht „{Ansicht}“ gehört zu einem Teil, das es nicht mehr gibt. Bitte die Positionsnummer löschen und neu setzen.“; nichts wird geschrieben.
- **G14 Browser:** Stücklistenblätter erscheinen im Abschnitt „Zeichnungen“ wie jede Zeichnung; „Umbenennen“ bei allen gesperrt („Stücklistenblätter lassen sich nicht umbenennen.“), „Löschen“ bei Folgeblättern gesperrt (Hinweis `FOLLOW_PAGE_DELETE`), bei Seite 1 Rückfrage „Stückliste mit {n} Blättern löschen?“ (n = 1: „Stückliste löschen?“).
- **G15 Fenster:** Der Abschnitt „Stückliste“ arbeitet wie die übrigen auf Kopien (Schalter im Blattsatz, Überschreibungen in `PartsDraft`); „Übernehmen“ schreibt Blattangaben und geänderte Überschreibungen; der Knopf „Stücklistenblatt erzeugen/öffnen“ handelt sofort (legt an bzw. öffnet und zeigt das Blatt), Papierwahl daneben (Vorgabe A4 hoch); das Fenster bleibt für sein Blatt offen.

## Review Focus

1. **Alte Dateien und alte Zeichnungen bleiben unberührt:** Format-4/5/6-Datei öffnen, Zeichnung ansehen, „Blattangaben“ öffnen und abbrechen, speichern → Bytes, Format und `edit_count` unverändert; eine alte Zeichnung mit mehreren Körpern bekommt keine Tabelle. → Aufgabe 5 Test (b), Aufgabe 3 Test (f), Aufgabe 6 Test (j).
2. **Kreis und Zeilentext bleiben am richtigen Teil:** Verschieben, Verrunden, Boolesche Operation (Ziel bleibt, Werkzeug verloren), Umbenennen, Löschen eines früheren Körpers, Unterdrücken und Zurückholen → richtige Nummer, richtiger Text, keine stille Umhängung. → Aufgabe 2 Tests (a), (b), (e), (i); Aufgabe 4 Test (k); Aufgabe 7 Test (g).
3. **Seitengrenzen:** 26/27 und 44/45 Zeilen, 200/201 Zeilen, Körper löschen, während ein Folgeblatt offen ist → Blätter entstehen/verschwinden genau an der Grenze, der Bereich fällt ohne Absturz aufs Modell zurück, „Blatt n von N“ stimmt. → Aufgabe 1 Test (e), Aufgabe 3 Test (b), Aufgabe 6 Test (f).
4. **Nichts überlappt, nichts ragt heraus:** alle Papiere hoch/quer × Zeilen 0..=12 × Änderungen 0..=3 × Hinweise 0..=6 → Tabelle, Schriftfeld, Änderungstabelle, Hinweisfeld, freier Bereich überschneidungsfrei, freier Bereich nie leer; Längstexte in jeder Zelle. → Aufgabe 1 Tests (c), (d).
5. **Tippen löst nichts aus:** in einem Tabellenfeld des Fensters Entf, Esc, „d“, „+“ → kein Kreis gelöscht, Werkzeug bleibt, Text im Feld. → Aufgabe 7 Test (c).

---

### Task 1: Stücklisten-Bausteine im Zeichnungspaket (Typen, Tabellengeometrie, Tabellenmarkierungen)

**Files:**
- Create: `crates/planora-drawing/src/parts.rs`
- Modify: `crates/planora-drawing/src/model.rs` (`DrawingKind`, `Drawing.kind`), `sheet_frame.rs` (`PartsSpace`, `sheet_layout_with`, `SheetLayout.parts`, `parts_columns`, `parts_rows_per_page`, Konstanten), `sheet_info.rs` (`SheetInfo.parts_list`, `TitleTexts.parts`, Dokumentart „Stückliste“), `sheet_marks.rs` (Tabelle im Rahmen, `parts_table_marks`, `parts_cell_is_cut`), `lib.rs`
- Modify (nur Übersetzen): Literale `Drawing { .. }` in `crates/planora-project/src/convert.rs::drawing_from` (`kind: DrawingKind::Views`), `crates/planora-drawing/tests/stage12_types.rs`, `crates/planora-feature/tests/dimensions.rs`; Literale `SheetInfo { .. }` in `convert.rs:1263`, `crates/planora-project/tests/sheet_format.rs:124` (`parts_list: false`); Literale `TitleTexts { .. }` in `crates/planora-drawing/tests/stage22_sheet_output.rs` (4 Stellen), `crates/planora-app/tests/export_drawing.rs:996` (`parts: Vec::new()`)
- Test: `crates/planora-drawing/tests/stage23_parts_table.rs` (neu)

**Interfaces:**
- Consumes: `sheet_frame::{SheetLayout, sheet_layout, REV_ROW_H, VALUE_H, REV_VISIBLE, MIN_PEN_MM}`, `sheet_info::{FieldValue, DASH}`, `sheet_marks::{fit_text, fitted_text}`.
- Produces:
```rust
// model.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrawingKind { #[default] Views, PartsList { page: u32 } }   // page ab 1
pub struct Drawing { /* … */ pub kind: DrawingKind }                   // Drawing::new → Views
// parts.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyKey { pub feature: u64, pub index: u32 }
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RowOverride { pub name: FieldValue, pub qty: FieldValue, pub material: FieldValue, pub note: FieldValue }
impl RowOverride { pub fn is_auto(&self) -> bool }
#[derive(Clone, Debug, PartialEq)]
pub struct PartsRow { pub pos: u32, pub name: String, pub qty: u32, pub keys: Vec<BodyKey> } // keys[0] = Schlüssel
pub type PartsCells = [String; 5];                    // Pos., Benennung, Menge, Werkstoff, Bemerkung
pub const PARTS_HEAD: [&str; 5] = ["Pos.", "Benennung", "Menge", "Werkstoff", "Bemerkung"];
pub const MAX_PARTS_NAME_CHARS: usize = 60; pub const MAX_PARTS_QTY_CHARS: usize = 60;
pub const MAX_PARTS_MATERIAL_CHARS: usize = 60; pub const MAX_PARTS_NOTE_CHARS: usize = 80;
pub const PARTS_ON_SHEET: usize = 10; pub const MAX_PARTS_LIST_ROWS: usize = 200;
pub const MAX_BALLOONS: usize = 200; pub const MAX_OVERRIDES: usize = 1000;
pub const PARTS_DOC_TYPE: &str = "Stückliste";
pub fn parts_sheet_name(page: u32) -> String;         // 1 → "Stückliste", 2 → "Stückliste (2)"
pub fn validate_override(o: &RowOverride) -> Result<(), String>;   // Längen in Zeichen, Steuerzeichen
pub fn resolve_parts(rows: &[PartsRow], overrides: &BTreeMap<BodyKey, RowOverride>, material_auto: &str) -> Vec<PartsCells>;
pub fn pages_needed(rows: usize, per_page: usize) -> u32;          // Zeilen auf 200 begrenzt, mindestens 1
pub fn page_rows(cells: &[PartsCells], page: u32, per_page: usize) -> &[PartsCells]; // nur die ersten 200
// sheet_frame.rs
pub const PARTS_ROW_H: f64 = 7.0;
pub const PARTS_COLUMNS: [f64; 5] = [12.0, 78.0, 15.0, 45.0, 30.0];
#[derive(Clone, Copy, Debug, PartialEq)] pub enum PartsSpace { None, OnSheet { rows: usize }, ListPage }
pub struct SheetLayout { /* … */ pub parts: Option<Box2>, pub parts_space: PartsSpace }
impl SheetLayout { pub fn parts_columns(&self) -> [f64; 5] }      // G4
pub fn sheet_layout_with(sheet: Sheet, revision_rows: usize, note_lines: usize, parts: PartsSpace) -> SheetLayout;
// sheet_layout(..) = sheet_layout_with(.., PartsSpace::None), Ergebnis unverändert
pub fn parts_rows_per_page(sheet: Sheet) -> usize;                // G6
// sheet_info.rs
pub struct SheetInfo { /* … */ pub parts_list: bool }            // new() → false
pub struct TitleTexts { /* … */ pub parts: Vec<PartsCells> }     // resolve_texts → leer; Aufrufer füllt
// Dokumentart automatisch: PARTS_DOC_TYPE bei DrawingKind::PartsList, sonst DEFAULT_DOC_TYPE
// sheet_marks.rs
pub fn parts_table_marks(layout: &SheetLayout, rows: &[PartsCells], widen: f64) -> Vec<Mark>; // leer ohne layout.parts
pub fn parts_cell_is_cut(col: usize, text: &str, layout: &SheetLayout) -> bool;
// build_frame hängt parts_table_marks(layout, &texts.parts, widen) an
```

- [ ] **Step 1: Failing tests (`stage23_parts_table.rs`):** (a) `resolve_defaults_and_overrides` (Zeilen `[{1,"Bolzen",2,[k1,k3]}, {2,"Platte",1,[k2]}]`, keine Überschreibung, Werkstoff-Vorgabe „–“ → `[["1","Bolzen","2","–",""], ["2","Platte","1","–",""]]`; Überschreibung unter `k1` Benennung „Zylinderstift“ und Bemerkung „gehärtet“ → Zeile 1 so, Pos. bleibt „1“; Überschreibung unter `k3` (nicht erster Schlüssel) wirkt nicht; `validate_override`: Benennung 61 × „Ä“ → `Err`, 60 ok, Bemerkung 81 → `Err`, „\n“ → `Err`); (b) `layout_on_sheet_examples` (`OnSheet{rows: 10}`, Hinweise 3, Änderungen 3: A3 quer Tabelle `((230,66),(410,143))`, Hinweisfeld `((110,10),(220,32))`, frei `((20,143),(410,259))`; A3 hoch Tabelle `((107,66),(287,143))`, Hinweise `((107,143),(287,165))`, frei `((20,165),(287,382))`; A4 hoch (±1e-3) Tabelle `((75.643,46.671),(202.929,101.121))`, Hinweise bis 116.679, frei `((14.143,116.679),(202.929,270.129))`; `OnSheet{rows: 2}` A3 quer Tabelle bis y 87; `OnSheet{rows: 12}` = `rows: 10`; `PartsSpace::None` = `sheet_layout` für alle 5 Papiere × hoch/quer × Änderungen 0..=3 × Hinweise 0..=6 (alle Felder gleich)); (c) `nothing_overlaps_and_free_never_empty` (5 Papiere × hoch/quer × `OnSheet` Zeilen 0..=12 × Änderungen 0..=3 × Hinweise 0..=6: Tabelle, Schriftfeld, Änderungstabelle, Hinweisfeld paarweise ohne Überschneidung (Berührung erlaubt, 1e-9), alle im Rahmen; frei mit Breite und Höhe > 0 und ohne Überschneidung); (d) `longest_texts_stay_in_cells` (Zellen mit 60 × „W“, 80 × „W“, „ÄÖÜß Ø°±“ für A4 hoch, A3 quer, A0 hoch, `OnSheet` und `ListPage`: jede Text-Markierung mit `sheet_text_width` ganz in ihrer Spalte und Zeile; `parts_cell_is_cut(1, 60 × "W", ..)` wahr, `("Platte")` falsch); (e) `pages` (`parts_rows_per_page`: A4/A3/A0 hoch 44, A4/A3/A1 quer 26; `pages_needed(0, 26) == 1`, `(26,26) == 1`, `(27,26) == 2`, `(44,44) == 1`, `(45,44) == 2`, `(201,26) == 8`; `page_rows(cells(201), 8, 26).len() == 18`, Seite 2 beginnt mit Pos. „27“); (f) `list_page_layout` (A3 hoch `ListPage`: Tabelle `((20,66),(287,382))`, `parts_columns` = G4-Anteile auf 267 mm (Pos. 17,8 ±1e-9), kein Hinweisfeld auch bei 6 Hinweiszeilen); (g) `table_marks` (A3 quer `OnSheet{3}` mit 3 Zeilen: Kopf-Grundlinie y 67,75 mit „Pos.“ … „Bemerkung“, Pos. „1“ Grundlinie 74,75, „3“ Grundlinie 88,75; senkrechte Trennlinien x 242, 320, 335, 380; alle Linien `Sheet(Thin)`; A3 hoch `ListPage` mit 3 Zeilen: Kopf-Grundlinie 376,75, Pos. „1“ 369,75; leere Zeilenliste: nur Kopf); (h) `doc_type_of_parts_sheet` (`DrawingKind::PartsList{page: 2}`, Name „Stückliste (2)“ → Dokumentart „Stückliste“, Titel „Stückliste (2)“; `Views` → „Fertigungszeichnung“; `parts_sheet_name(1)` „Stückliste“, `(3)` „Stückliste (3)“).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage23_parts_table -j 2`
- [ ] **Step 3: Implement** nach Interfaces und G3–G6 (Muster `revision_table`/`notes_box`; Hindernisregel F8 aus 3c-1 mit der Tabelle).
- [ ] **Step 4: Run → PASS**, dann `cargo test -p planora-drawing -p planora-feature -p planora-project -j 2` (Literale nachgezogen, `stage20`/`stage22` und Golden-Dateien unverändert), `cargo test -p planora-app --test export_drawing -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Hinweisfeld nicht über die Tabelle rücken → (c) rot; Kopfzeile auf dem Zeichnungsblatt oben → (g) rot.
- [ ] **Step 6: Commit** — `feat: Stücklistentabelle als Blattgeometrie und Markierungen`.

### Task 2: Körperkennung und Stücklisten-Ableitung im Dokument

**Files:**
- Create: `crates/planora-feature/src/parts.rs`
- Modify: `crates/planora-feature/src/model.rs` (`Document.parts_overrides: BTreeMap<BodyKey, RowOverride>`), `snapshot.rs` (`Snapshot.parts_overrides`, Prüfung in `from_snapshot`), `lib.rs`; `crates/planora-app/src/state.rs` (`origin_of`/`body_name` entfallen, Aufrufe gehen an `Document::body_display_name`; `body_kind` nutzt die verschobene Ursprungsregel)
- Modify (nur Übersetzen): Literale `Snapshot { .. }` (10 Stellen, u. a. `crates/planora-feature/tests/{dimensions,drawings}.rs`, `crates/planora-project/src/convert.rs`) mit `parts_overrides: BTreeMap::new()`
- Test: `crates/planora-feature/tests/parts.rs` (neu)

**Interfaces:**
- Consumes: `parts::{BodyKey, RowOverride, PartsRow, PartsCells, resolve_parts, validate_override, MAX_OVERRIDES}`, `sheet_info::DASH` (1).
- Produces:
```rust
impl Document {
    pub fn body_key(&self, r: BodyRef) -> BodyKey;                  // G1
    pub fn body_display_name(&self, r: BodyRef) -> Option<String>; // Regel aus state.rs::body_name, unverändert (G2)
    pub fn parts_rows(&self) -> Vec<PartsRow>;                      // G3
    pub fn parts_cells(&self) -> Vec<PartsCells>;                   // resolve_parts mit Werkstoff-Vorgabe G3
    pub fn row_of(&self, key: BodyKey) -> Option<u32>;              // Pos. der Zeile, die den Körper enthält
    pub fn parts_overrides(&self) -> &BTreeMap<BodyKey, RowOverride>;
    pub fn set_parts_override(&mut self, key: BodyKey, o: RowOverride) -> Result<(), FeatureError>;
    // prüft; is_auto → Eintrag entfernen; drawing_edits nur bei Änderung; > MAX_OVERRIDES → Fehler
}
pub struct Snapshot { /* … */ pub parts_overrides: BTreeMap<BodyKey, RowOverride> }
```

- [ ] **Step 1: Failing tests (`tests/parts.rs`):** (a) `identity_survives_move_fillet_boolean` (Quader „Platte“ A, Quader „Stift“ B; Verschieben von A, Verrunden des verschobenen A, Vereinigen mit Ziel = verrundetes A, Werkzeug = B: `body_key` des Ergebnisses `== BodyKey{feature: A.0, index: 0}`; `row_of(key(B)) == None`; eine Zeile mit `keys == [key(A)]`, Benennung `== body_display_name(Ergebnis)`); (b) `rename_and_reorder_keep_keys` (drei Quader; Feature des zweiten umbenennen → Kennung gleich, Benennung neu; ersten löschen → Kennung des dritten gleich, seine Pos. von 3 auf 2); (c) `grouping_and_quantity` („Bolzen“, „Platte“, „Bolzen“ → `[{1,"Bolzen",2,[k1,k3]}, {2,"Platte",1,[k2]}]`, `row_of(k3) == Some(1)`); (d) `meshes_and_suppressed_are_not_rows` (Netz-Körper und unterdrückter Quader fehlen, F6); (e) `overrides_by_key` (Benennung „Grundplatte“ unter `key(A)`: Zelle „Grundplatte“, nach Umbenennen des Features weiter; alles automatisch → Eintrag weg; gleicher Wert +0 Änderungen, neuer +1; `model_edit_count` unverändert; 61 Zeichen → Fehler, Dokument gleich; 1001. Schlüssel → Fehler); (f) `cells_defaults` (ohne Projektangaben Werkstoff „–“, Bemerkung „“; Projekt-Werkstoff `Text("S235")` → „S235“; Überschreibung „Alu“ gewinnt); (g) `boolean_split_has_own_key` (`body_key(BodyRef{feature: bool, index: 1}) == BodyKey{bool.0, 1}`, Index 0 → Ziel); (h) `snapshot_round_trip_keeps_overrides` (Snapshot → `from_snapshot` gleich, `edit_count` 0; Überschreibung mit 61 Zeichen im Snapshot → Fehler); (i) `suppress_and_restore` (A unterdrücken → `row_of(key(A)) == None`, Überschreibung bleibt; zurückholen → Text wieder da).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test parts -j 2`
- [ ] **Step 3: Implement** nach Interfaces und G1–G3; `state.rs` ruft nur noch die Dokumentfunktionen (keine zweite Kopie der Regel).
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-feature -p planora-project -j 2`, `cargo test -p planora-app --test state --test tree_ui --test boolean_state --test feature_state -j 2` (Körpernamen unverändert), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Boolean in `body_key` nicht verfolgen → (a) rot; Gruppieren nach Kennung statt Name → (c) rot.
- [ ] **Step 6: Commit** — `feat: stabile Körperkennung und Stückliste im Dokument`.

### Task 3: Stücklistenblätter als Zeichnungen und Schalter-Vorgabe

**Files:**
- Create: `crates/planora-feature/src/parts_sheets.rs`
- Modify: `crates/planora-feature/src/drawings.rs` (`validate_drawing`: Stücklistenblatt ohne Ansichten, Maße, Kreise; `delete_drawing`, `rename_drawing`, `update_drawing`, `add_drawing_stamped` nach G7/G8), `recompute.rs` (am Ende von `recompute`: `sync_parts_sheets`), `ops.rs` (`rename`: `sync_parts_sheets`), `snapshot.rs` (kein Abgleich beim Laden), `lib.rs`
- Test: `crates/planora-feature/tests/parts_sheets.rs` (neu); anpassen: `crates/planora-feature/tests/sheet_info.rs` (Fälle mit mehr als einem Körper erwarten `parts_list == true`)

**Interfaces:**
- Consumes: `DrawingKind`, `parts_sheet_name`, `pages_needed`, `parts_rows_per_page` (1), `Document::parts_rows` (2).
- Produces:
```rust
pub const PARTS_SHEET_EXISTS: &str = "Es gibt schon ein Stücklistenblatt.";
pub const PARTS_NAME_TAKEN: &str = "Es gibt schon eine Zeichnung mit dem Namen „Stückliste“.";
pub const FOLLOW_PAGE_DELETE: &str = "Ein Folgeblatt der Stückliste lässt sich nicht einzeln löschen. Es verschwindet von selbst, wenn die Zeilen auf weniger Blätter passen.";
pub const PARTS_SHEET_RENAME: &str = "Stücklistenblätter lassen sich nicht umbenennen.";
impl Document {
    pub fn parts_sheets(&self) -> Vec<DrawingId>;     // nach Seitennummer
    pub fn add_parts_sheets(&mut self, sheet: Sheet, stamp: &Stamp) -> Result<DrawingId, FeatureError>; // Seite 1 + Abgleich; Projektion Erstwinkel
    pub fn sync_parts_sheets(&mut self) -> bool;      // G7; true, wenn sich etwas geändert hat
}
```

- [ ] **Step 1: Failing tests (`tests/parts_sheets.rs`):** (a) `create_first_page` (3 Körper, A4 hoch: eine Zeichnung „Stückliste“, `PartsList{page: 1}`, Satz mit `notes == Lines([])`, `parts_list == false`; zweiter Aufruf → `PARTS_SHEET_EXISTS`; vorhandene Zeichnung „stückliste“ → `PARTS_NAME_TAKEN`; `edit_count` +1); (b) `pages_follow_row_count` (A4 quer: 26 verschieden benannte Quader → 1 Blatt; 27. Quader → „Stückliste (2)“ mit `page: 2`; einen löschen → wieder 1; 53 → 3; Blattformat von Seite 1 über `update_drawing` auf A3 quer → Folgeblätter A3 quer); (c) `pages_stay_together` (Seite 1, dann Zeichnung „Halter“, dann 27. Körper → Reihenfolge „Stückliste“, „Stückliste (2)“, „Halter“); (d) `delete_and_rename_rules` (Seite 2 löschen → `FOLLOW_PAGE_DELETE`, nichts geändert; Seite 1 löschen → alle Blätter weg, `edit_count` +1; jedes Blatt umbenennen → `PARTS_SHEET_RENAME`); (e) `no_views_on_parts_sheets` (`add_view` auf ein Stücklistenblatt → Fehler; `validate_drawing` lehnt `PartsList` mit Ansicht, Maß oder Kreis ab, `page == 0` ebenso); (f) `loading_does_not_sync` (Snapshot mit 1 Blatt, aber 27 Zeilen auf A4 quer → `from_snapshot` behält 1 Blatt, `edit_count` 0; nächste Modelländerung → 2 Blätter); (g) `stamped_switch_default` (F4: 0 und 1 Körper → `parts_list` false; 2 Körper → true; danach Körper hinzufügen oder löschen ändert ihn nicht; `add_drawing` → `info` None); (h) `rename_regroups_pages` (27 Zeilen auf A4 quer, ein Feature so umbenennen, dass zwei Zeilen verschmelzen → 1 Blatt, über `rename`).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test parts_sheets -j 2`
- [ ] **Step 3: Implement** nach Interfaces und G7/G8.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-feature -p planora-project -j 2` (bestehende Format-6-Tests: Dokumente mit mehr als einem Körper und `add_drawing_stamped` sind jetzt Format 7 – nur dort die Erwartung ändern und im Commit nennen), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Abgleich in `from_snapshot` → (f) rot; neue Seite ans Ende statt hinter Seite 1 → (c) rot.
- [ ] **Step 6: Commit** — `feat: Stücklistenblätter als Zeichnungen mit automatischen Folgeblättern`.

### Task 4: Positionsnummern im Modell, als Markierungen und in SVG/DXF

**Files:**
- Create: `crates/planora-drawing/src/balloon.rs`
- Modify: `crates/planora-drawing/src/model.rs` (`BalloonId`, `Balloon`, `Drawing.balloons`, `Drawing.next_balloon`, Methoden; `remove_view` entfernt Kreise), `annotate.rs` (`MarkKind::Position(PositionPen)`), `svg.rs` (Farbe, Breite), `dxf.rs` (Ebenen G12), `lib.rs`; `crates/planora-feature/src/drawings.rs` (Prüfung, `add_balloon`, `balloon_numbers`); `crates/planora-app/src/drawing_view.rs` (nur die neuen `MarkKind`-Zweige: Farbe `theme::SHEET_DIM`, Breite `pen.width_at(sheet_scale)`); Literale `Drawing { .. }` wie in Aufgabe 1
- Test: `crates/planora-drawing/tests/stage24_balloons.rs` (neu), `crates/planora-feature/tests/balloons.rs` (neu)

**Interfaces:**
- Consumes: `BodyKey`, `MAX_BALLOONS` (1), `Document::row_of` (2), `sheet::{SheetModel, SheetTransform, view_transform}`, `sheet_frame::{sheet_scale, MIN_PEN_MM}`, `sheet_marks::fit_text`.
- Produces:
```rust
// model.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct BalloonId(pub u64);
#[derive(Clone, Debug, PartialEq)]
pub struct Balloon { pub id: BalloonId, pub view: ViewId, pub body: BodyKey, pub anchor: (f64, f64), pub offset: (f64, f64) }
pub struct Drawing { /* … */ pub balloons: Vec<Balloon>, pub next_balloon: u64 }   // new(): leer, 1
impl Drawing { pub fn add_balloon(&mut self, b: Balloon) -> BalloonId; pub fn balloon(&self, id: BalloonId) -> Option<&Balloon>;
               pub fn remove_balloon(&mut self, id: BalloonId) -> bool }
// balloon.rs (A3-Werte, mal Faktor)
pub const BALLOON_D: f64 = 8.0; pub const BALLOON_TEXT_H: f64 = 3.5; pub const BALLOON_DOT_R: f64 = 0.5;
pub const DEFAULT_BALLOON_OFFSET: (f64, f64) = (10.0, 10.0);
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum PositionPen { Circle, Leader }
impl PositionPen { pub fn width_mm(self) -> f64; pub fn width_at(self, scale: f64) -> f64 }   // 0,35 / 0,25; ≥ MIN_PEN_MM
pub fn anchor_point(bbox: ((f64, f64), (f64, f64)), xf: &SheetTransform, anchor: (f64, f64)) -> DVec2;
pub fn anchor_of(bbox: ((f64, f64), (f64, f64)), xf: &SheetTransform, at: DVec2) -> (f64, f64); // auf [0,1] begrenzt
pub fn layout_balloon(at: DVec2, offset: (f64, f64), number: Option<u32>, scale: f64) -> Vec<Mark>; // G10; nicht endlich → leer
#[derive(Clone, Debug, PartialEq)]
pub struct BalloonMarks { pub view: ViewId, pub id: BalloonId, pub centre: DVec2, pub radius: f64, pub marks: Vec<Mark> }
pub fn balloon_marks(drawing: &Drawing, results: &BTreeMap<ViewId, &ViewLines>, numbers: &BTreeMap<BalloonId, Option<u32>>) -> Vec<BalloonMarks>;
pub fn add_balloon_marks(model: &mut SheetModel, list: &[BalloonMarks]);   // an SheetView.marks der Ansicht
// annotate.rs
pub enum MarkKind { Dimension, CenterLine, Lost, Sheet(SheetPen), Position(PositionPen) }
// planora-feature
pub const TOO_MANY_BALLOONS: &str = "Es sind höchstens 200 Positionsnummern je Zeichnung möglich.";
impl Document {
    pub fn add_balloon(&mut self, drawing: DrawingId, view: ViewId, body: BodyKey, anchor: (f64, f64), offset: (f64, f64)) -> Result<BalloonId, FeatureError>;
    pub fn balloon_numbers(&self, drawing: DrawingId) -> BTreeMap<BalloonId, Option<u32>>; // row_of; None = verloren
}
```
`validate_drawing`: höchstens 200 Kreise, Kennungen `1..next_balloon` eindeutig, Ansicht vorhanden, `anchor` endlich in [0, 1], `offset` endlich mit Betrag ≤ `MAX_POS_MM`; `Drawing` auf einem Stücklistenblatt ohne Kreise (Aufgabe 3).

- [ ] **Step 1: Failing tests (`stage24_balloons.rs`):** (a) `balloon_geometry_a3` (Anker (100,100), Versatz (10,10), Nummer 3, f = 1: Kreis Mitte (110,110) r 4 `Position(Circle)`; Linie von (100,100) bis (110 − 4/√2, 110 − 4/√2) `Position(Leader)`; Punkt r 0,5 um (100,100); Text „3“ `Middle` bei (110, 108,25), Höhe 3,5); (b) `scales_with_paper` (A0: r = 4 · 2,8310 ±1e-3; `Circle.width_at(0.7071)` 0,2475, `Leader.width_at(0.7071)` 0,18; Text „200“ passt in 0,8 · Ø); (c) `short_offset_has_no_leader` (Versatz (1,1) → keine Linie, Kreis und Punkt da); (d) `lost_balloon` (`None` → alle Markierungen `Lost`, Text „?“); (e) `anchor_round_trip` (Hüllbox ((−10,−5),(30,15)), Maßstab 1:2, Mitte (150,100): `anchor_of(anchor_point(uv)) == uv` für (0,0), (0.25,0.8), (1,1); (0.5,0.5) = Ansichtsmitte; Punkt außerhalb → auf [0,1] begrenzt); (f) `follows_view_scale` (Quader-Ansicht, Kreis bei (1,1): Maßstab 1:1 → 1:2 → Anker bleibt an der Ecke, Versatz unverändert); (g) `svg_and_dxf_output` (SVG: Kreis in der Gruppe der Ansicht, Strichbreiten 0,35 und 0,25; DXF: Ebenen `POSITIONEN` (Strichstärke 35) und `POSITIONEN_LINIE` (25), Kreis, Linie und `TEXT` „3“, Leser `common/dxf_read.rs` liest alles; ohne Kreise keine dieser Ebenen und Golden-Dateien gleich); (h) `remove_view_drops_balloons`; (i) `non_finite_is_dropped` (Versatz NaN → keine Markierungen, SVG/DXF schreiben ohne Fehler). `tests/balloons.rs`: (j) `add_and_number` (Kreis an „Platte“ → Nummer 2 bei Zeilen „Bolzen“, „Platte“; 201. → `TOO_MANY_BALLOONS`; unbekannte Ansicht, Stücklistenblatt, Anker NaN → Fehler, Dokument gleich; `edit_count` +1); (k) `numbers_follow_the_part` (Körper löschen → `None`; Werkzeug einer Vereinigung → `None`, Ziel → Nummer; zwei Körper durch Umbenennen in eine Zeile → gleiche Nummer (F5); Unterdrücken/Zurückholen → `None`/Nummer).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage24_balloons -j 2`; `cargo test -p planora-feature --test balloons -j 2`
- [ ] **Step 3: Implement** nach Interfaces, G9, G10, G12 (Ebenen über die vorhandenen Nutzungsmerker in `dxf.rs`).
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-drawing -j 2` (Golden-Dateien unverändert), `cargo test -p planora-feature -j 2`, `cargo test -p planora-app --test export_drawing --test export_dimensions --test dimension_view -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweis:** Hinweislinie bis zur Kreismitte statt zum Rand → (a) rot.
- [ ] **Step 6: Commit** — `feat: Positionsnummern als Markierungen in Ansichten, SVG und DXF`.

### Task 5: Dateiformat 7

**Files:**
- Create (Schritt 1, vor jeder Codeänderung): `crates/planora-project/tests/fixtures/format6_sheet_info.json` – mit dem Stand `84148e4` erzeugt (Dokument mit 1 Körper, Zeichnung über `add_drawing_stamped`, Projektangaben mit eigenem Eigentümer, 2 Änderungen, eigene Hinweise; `encode_version`), danach eingefroren
- Modify: `crates/planora-project/src/format.rs` (DTOs), `convert.rs` (`FORMAT_VERSION = 7`, `required_format`, encode/decode)
- Test: `crates/planora-project/tests/parts_format.rs` (neu); anpassen: `sheet_format.rs` (`FORMAT_VERSION == 6` → 7; „Format 7 → TooNew{7}“ wird „Format 8 → TooNew{8}“)

**Interfaces:**
- Consumes: alles aus 1–4.
- Produces (DTOs ohne `deny_unknown_fields`, alles `#[serde(default, skip_serializing_if = …)]`):
```rust
VersionFile.stueckliste: Vec<StuecklisteZeileDto>                 // leer → fehlt; ≤ MAX_OVERRIDES
StuecklisteZeileDto { koerper: [u64; 2], benennung: Option<String>, menge: Option<String>, werkstoff: Option<String>, bemerkung: Option<String> }
ZeichnungDto.stuecklistenseite: Option<u32>                       // None = Ansichtenzeichnung
ZeichnungDto.positionen: Vec<PositionDto>; ZeichnungDto.naechste_position: u64   // leer/0 → fehlt
PositionDto { id: u64, ansicht: u64, koerper: [u64; 2], anker: [f64; 2], versatz: [f64; 2] }
BlattangabenDto.stueckliste: bool                                  // false → fehlt
```
`required_format`: 7, wenn `parts_overrides` nicht leer, eine Zeichnung `PartsList`, `balloons` nicht leer oder `info.parts_list`; sonst wie bisher. Decode: Format < 7 mit einem dieser Felder → `corrupt(origin, "Stücklisten und Positionsnummern gibt es erst ab Dateiformat 7")`; Längen über `validate_override`, Kreise über `validate_drawing` (Meldung mit Zeichnungsname), `koerper[1]` > `u32::MAX` → corrupt.

- [ ] **Step 1: Fixture einfrieren** (vor allen Änderungen dieser Aufgabe, aus `84148e4` mit einem temporären Test, der danach gelöscht wird); Datei prüfen: enthält `"format": 6`, kein „stueckliste“, kein „positionen“.
- [ ] **Step 2: Failing tests (`parts_format.rs`):** (a) `format_7_only_with_parts_data` (Matrix: nichts → wie bisher; nur Überschreibung → 7; nur Schalter an → 7; Schalter aus → 6; nur Stücklistenblatt → 7; nur Kreis → 7; `FORMAT_VERSION == 7`); (b) `old_files_stay_byte_identical` (Fixtures Format 4, 5 und das neue Format 6: `encode(decode(text)) == text`); (c) `full_round_trip` (Überschreibungen mit „ä ö ü ß Ø ° ± –“, drei Stücklistenblätter, Kreise mit Kennungslücken und einem verlorenen Körper, Schalter an → `decode(encode(x)) == x`); (d) `rejects_bad_parts_data` (Format 6 mit `positionen` → Meldung „erst ab Dateiformat 7“; 201 Kreise, Benennung 61 Zeichen, Seite 0, Stücklistenblatt mit Ansicht, Anker NaN → benannte `corrupt`-Meldung; Format 8 → `TooNew { found: 8 }`); (e) `unknown_fields_are_ignored` (Format-7-Datei mit Zusatzfeld in `positionen` lädt).
- [ ] **Step 3: Run → FAIL.** `cargo test -p planora-project --test parts_format -j 2`
- [ ] **Step 4: Implement** nach Interfaces (Muster Format 6 in 3c-1 Aufgabe 3).
- [ ] **Step 5: Run → PASS**, `cargo test -p planora-project -p planora-feature -j 2`, clippy, fmt.
- [ ] **Step 6: Mutationsbeweise:** `required_format` ohne die 7-Regel → (a) rot; `skip_serializing_if` an `stueckliste` entfernt → (b) rot.
- [ ] **Step 7: Commit** — `feat: Stückliste und Positionsnummern in Dateiformat 7` (Fixture-Pfad einzeln `git add`).

### Task 6: Stückliste in der App (Blatt, Platzreserve, Stücklistenblatt, Abschnitt „Stückliste“, Browser-Regeln)

**Files:**
- Create: `crates/planora-app/src/parts_section.rs`
- Modify: `crates/planora-app/src/drawing_view.rs` (`parts_space`; `sheet_texts` füllt `parts`; `layout_from_texts`, `layout_of`, `placement_layout`, `new_sheet_layout` mit `PartsSpace`; `SheetCache` vergleicht die Zellen; „Ansicht hinzufügen“ auf Stücklistenblättern gesperrt), `sheet_dialog.rs` (Abschnitt „Stückliste“ als vierter Abschnitt, `PartsDraft`, `apply`; kein Hinweis-Editor auf Stücklistenblättern), `drawing_browser.rs` (G14), `lib.rs`
- Test: `crates/planora-app/tests/parts_sheet.rs` (neu); anpassen, nur wo Zeichnungen über „Neue Zeichnung“ mit mehr als einem Körper entstehen (Schalter jetzt an, freier Bereich kleiner): betroffene Erwartungen in `sheet_view.rs`, `drawing_ui.rs`, `sheet_dialog.rs`, `wide_dialogs.rs` neu rechnen und im Commit nennen

**Interfaces:**
- Consumes: 1–3; `dialog::{Dialog, DialogSize::Wide}`, `theme::{text_field, Toggle, section_header}`.
- Produces:
```rust
// drawing_view.rs
pub fn parts_space(state: &AppState, drawing: &Drawing, placement: bool) -> PartsSpace;
// Views + parts_list: OnSheet{ rows: placement ? 10 : Zeilenzahl }; PartsList: ListPage; sonst None
pub fn layout_from_texts(drawing: &Drawing, texts: &TitleTexts, parts: PartsSpace) -> SheetLayout;
pub const NO_VIEWS_ON_PARTS_SHEET: &str = "Auf dem Stücklistenblatt gibt es keine Ansichten.";
// parts_section.rs
pub const SECTION: &str = "Stückliste";
pub const SWITCH: &str = "Stückliste auf diesem Blatt";
pub const TOO_MANY_ON_SHEET: &str = "Weitere Zeilen passen nicht aufs Blatt. Dafür gibt es das Stücklistenblatt.";
pub const TOO_MANY_ROWS: &str = "Mehr als 200 Zeilen passen nicht auf die Stücklistenblätter.";
pub const NO_PARTS: &str = "Es gibt noch keine Teile.";
pub const CREATE: &str = "Stücklistenblatt erzeugen"; pub const OPEN: &str = "Stücklistenblatt öffnen";
#[derive(Clone, Debug)] pub struct PartsDraft { pub overrides: BTreeMap<BodyKey, RowOverride>, pub paper: Paper, pub landscape: bool } // Vorgabe A4 hoch
pub fn create_or_open(state: &mut AppState, sheet: Sheet) -> Result<DrawingId, String>;   // legt an (Stamp::now) oder öffnet Seite 1; zeigt sie
pub fn show_section(ui: &mut egui::Ui, dark: bool, kind: DrawingKind, info: &mut SheetInfo, draft: &mut PartsDraft,
                    rows: &[PartsRow], auto: &[PartsCells], layout: &SheetLayout, has_sheets: bool) -> bool; // true = Knopf geklickt
// sheet_dialog.rs
pub struct SheetInfoDialog { /* … */ pub parts: PartsDraft }  // apply: set_sheet_info, dann set_parts_override je geändertem Schlüssel
```
Abschnitt: Schalter `SWITCH` (nur bei Ansichtenzeichnungen); je Zeile Pos. als Text, Benennung/Menge/Werkstoff/Bemerkung über `theme::text_field` mit `hint_text` = automatischer Wert, `char_limit` nach Aufgabe 1, Länge „n/max“, „Zurücksetzen“ bei eigenem Text, `CUT_WARNING` bei `parts_cell_is_cut`; über 10 Zeilen (Schalter an) `TOO_MANY_ON_SHEET`, über 200 `TOO_MANY_ROWS`, ohne Zeilen `NO_PARTS`; darunter Papier (A4…A0) und hoch/quer, Knopf `CREATE` bzw. `OPEN`. Ansicht: Tabellenmarkierungen kommen mit dem Rahmen (`sheet_frame`), also gleiche Malregeln wie 3c-1.

- [ ] **Step 1: Failing tests (`parts_sheet.rs`, Harness mit `PlanoraApp::for_tests` bei 800 × 600 und 1400 × 900, Hell und Dunkel):** (a) `table_on_sheet_and_reserved_space` (3 Körper, „Neue Zeichnung“ → `parts_list` an; gemalte Texte „Pos.“, „Benennung“ und die drei Namen in der Tabellenfläche; `placement_layout(..).free == sheet_layout_with(.., OnSheet{rows: 10}).free`; 1 Körper → aus, kein „Benennung“; danach Körper hinzufügen → bleibt aus (F4)); (b) `more_than_ten_rows` (12 Körper: in der Tabellenfläche Pos. „1“ … „10“, nicht „11“; Abschnitt zeigt `TOO_MANY_ON_SHEET`); (c) `views_never_under_the_table` (A4 hoch, A4 quer, A3 quer, Schalter an: Vorne, Oben, Rechts, Isometrisch hinzufügen → geschätzte Ausdehnungen im freien Bereich, keine Überschneidung mit der 10-Zeilen-Tabelle); (d) `section_edit_apply_reset` (Benennung „Grundplatte“ an Zeile 1, Übernehmen → Blatt zeigt „Grundplatte“; Feature umbenennen → bleibt; „Zurücksetzen“, Übernehmen → Körpername; Abbrechen ändert nichts (`edit_count`); Pos. ohne Textfeld; „11/60“ sichtbar); (e) `switch_toggle` (aus → Tabelle weg, freier Bereich größer, `required_format` 6; an → 7); (f) `create_and_open_parts_sheet` (Knopf `CREATE` → „Stückliste“ A4 hoch offen, Schriftfeld „Stückliste“ als Dokumentart und Titel, „2 von 2“; Knopf heißt jetzt `OPEN` und öffnet es; Papier A3 quer gewählt → so angelegt; 45 Körper auf A4 hoch → 2 Blätter, Blatt 2 zeigt Pos. „45“; Blatt 2 offen, 1 Körper löschen → Bereich fällt aufs Modell zurück, keine Panik); (g) `browser_rules` (Umbenennen bei allen Stücklistenblättern gesperrt mit Hinweis, Löschen bei Blatt 2 gesperrt mit `FOLLOW_PAGE_DELETE`, Löschen bei Blatt 1 fragt „Stückliste mit 2 Blättern löschen?“ und entfernt beide; „Ansicht hinzufügen“ auf dem Stücklistenblatt gesperrt mit `NO_VIEWS_ON_PARTS_SHEET`); (h) `texts_are_plain` (sichtbare Texte ohne „Spec“, „ISO“, „DXF“, „Format 7“, „Kennung“, „TODO“); (i) `fits_small_window` (800 × 600: Abschnitt durch Rollen erreichbar, Felder ragen nicht über den Fensterrand); (j) `old_drawing_unchanged` (Zeichnung über `add_drawing` mit 3 Körpern: keine Tabelle; Fenster öffnen und abbrechen → `edit_count`, `required_format` gleich).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test parts_sheet -j 2`
- [ ] **Step 3: Implement** nach Interfaces, G5, G6, G14, G15.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test sheet_view --test sheet_dialog --test drawing_ui --test drawing_state --test drawing_browser --test drawing_fields --test dialogs --test wide_dialogs --test no_raw_windows --test no_raw_text_edits --test no_hard_coded_colors --test fonts -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** `placement_layout` mit tatsächlicher statt 10 Zeilen → (c) rot; `apply` schreibt beim Öffnen → (d) rot.
- [ ] **Step 6: Commit** — `feat: Stückliste auf dem Blatt, Stücklistenblatt und Abschnitt in den Blattangaben`.

### Task 7: Werkzeug „Position“ und Browser-Abschnitt „Positionen“

**Files:**
- Create: `crates/planora-app/src/balloon_tool.rs`, `crates/planora-app/src/balloon_browser.rs`
- Modify: `crates/planora-app/src/drawing_view.rs` (Knopf nach G11, `Action::BalloonTool`, Klick/Ziehen/Auswahl/Entf/Esc, Kreise im Blattmodell über `add_balloon_marks`, Hinweiszeile für verlorene Kreise), `drawing_state.rs` (`DrawingUi.selected_balloon`, beim Wechsel/Löschen geleert), `state.rs` (`balloon_tool: BalloonTool`), `browser.rs` (Abschnitt nach „Maße“), `icons.rs` (`Icon::PositionTool`, vorhandenes Zeichen der Symbolschrift), `lib.rs`
- Test: `crates/planora-app/tests/balloon_tool.rs`, `crates/planora-app/tests/balloon_browser.rs` (neu); anpassen: `tests/icons.rs` (neues Symbol), `tests/drawing_fields.rs` nur falls die Erwartung der Werkzeugfeldbreite eine feste Knopfzahl nennt

**Interfaces:**
- Consumes: 2, 4; `dimension_tool::{NO_VIEWS_TIP}`, `drawing_view::{tool_hit, polylines_cached, screen_to_sheet}`.
- Produces:
```rust
pub const TOOL: &str = "Position";
pub const TOOL_TIP: &str = "Ein Klick auf eine Linie eines Teils setzt eine Positionsnummer. Den Kreis kann man danach verschieben.";
pub const HINT: &str = "Auf eine Linie eines Teils klicken. Esc beendet.";
pub const MISSED: &str = "Bitte eine Linie eines Teils anklicken.";
pub const READ_ONLY_TIP: &str = "In einer alten Version lassen sich keine Positionsnummern setzen.";
pub const LOST_TIP: &str = "Das Teil zu dieser Positionsnummer gibt es nicht mehr.";
pub fn lost_note(n: usize) -> String;   // „1 Positionsnummer hat ihr Teil verloren.“ / „{n} Positionsnummern haben ihr Teil verloren.“
#[derive(Debug, Default)] pub struct BalloonTool { /* aktiv, Meldung */ }
impl BalloonTool { pub fn is_active(&self) -> bool; pub fn set_active(&mut self, on: bool); pub fn message(&self) -> Option<&str> }
pub fn body_key_of_stroke(state: &AppState, body: usize) -> Option<BodyKey>;  // Index unter den exakten Körpern (wie exact_copies)
pub fn place(state: &mut AppState, drawing: DrawingId, view: ViewId, body: usize, at: DVec2) -> Result<BalloonId, String>;
pub fn delete_selected(state: &mut AppState) -> bool;
pub fn balloon_hit(list: &[BalloonMarks], to_screen: &dyn Fn(DVec2) -> Pos2, pos: Pos2) -> Option<BalloonId>; // Kreis + PICK_PX
// balloon_browser.rs
pub const EMPTY_TEXT: &str = "Noch keine Positionsnummern. Das Werkzeug „Position“ setzt welche.";
pub fn row_text(number: Option<u32>, name: &str) -> String;  // „Pos. 2 – Platte“; verloren „Pos. ? – Teil fehlt“
pub fn show(ui: &mut egui::Ui, state: &mut AppState, read_only: bool);
```

- [ ] **Step 1: Failing tests (`balloon_tool.rs`, Harness 1400 × 900 und 800 × 600):** (a) `click_sets_balloon_on_every_line_kind` (Werkzeug an; Klick auf sichtbare Kante, verdeckte Kante, Umriss eines Zylinders, Kreisbogen → je ein Kreis mit der Kennung des Körpers, `anchor` in [0,1]², `offset == (10,10)·f`; Kreis und Nummer gemalt; Klick daneben → `MISSED` in der Hinweiszeile, nichts gesetzt); (b) `drag_moves_circle_only` (Kreis ziehen um (40 px, −20 px) → `offset` ändert sich um (40, 20)/k, `anchor` gleich; auch ohne Werkzeug); (c) `select_delete_escape` (Klick auf den Kreis wählt; Entf löscht; Esc beendet das Werkzeug; bei Fokus in einem Textfeld des Fensters „Blattangaben“: Entf, Esc, „d“, „+“ → Kreis bleibt, Werkzeug bleibt, Text im Feld); (d) `exclusive_tools` (Position an → Maß aus und umgekehrt); (e) `lost_balloon_is_shown` (Körper-Feature löschen → Kreis rot mit „?“, Hinweiszeile `lost_note(1)`, Tooltip `LOST_TIP`); (f) `numbers_follow_parts_list` (zwei „Bolzen“ → beide Kreise „1“; einen umbenennen → Nummern nach den neuen Zeilen); (g) `survives_recompute_and_reorder` (Maß des Quaders ändern, früheren Körper löschen, Verschieben anhängen → Kreis bleibt am Teil, Nummer nach Zeile); (h) `toolbar_fits_1100x700` (kein Rollbalken; Knopf „Position“ in Zeile 2 rechts vom Maß-Auswahlfeld, sonst nach G11 in Zeile 1); (i) `blocked_states` (alte Version, keine Ansicht, Stücklistenblatt → gesperrt mit Hinweis; Isometrie erlaubt); (j) `limit_message` (200 Kreise, 201. Klick → `TOO_MANY_BALLOONS` in der Hinweiszeile). `balloon_browser.rs`: (k) `section_lists_and_acts` (Gruppen je Ansicht, Zeile „Pos. 1 – Platte“, Klick wählt den Kreis, Kontextmenü „Löschen“ löscht, in alter Version gesperrt; verlorene Zeile rot mit `LOST_TIP`; ohne Kreise `EMPTY_TEXT`; ohne offene Zeichnung kein Abschnitt).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test balloon_tool --test balloon_browser -j 2`
- [ ] **Step 3: Implement** nach Interfaces, G9–G11 (Muster `dimension_tool`/`dimension_browser`; Treffer über dieselben Linienzüge wie das Maß-Werkzeug).
- [ ] **Step 4: Run → PASS**, dazu `--test drawing_fields --test dimension_tool --test dimension_view --test drawing_ui --test icons --test fonts --test no_hard_coded_colors --test no_raw_text_edits --test shell_panels`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Entf ohne Prüfung `viewport::typing` → (c) rot; Kennung über Körperindex statt `body_key` → (g) rot.
- [ ] **Step 6: Commit** — `feat: Werkzeug Position und Browser-Abschnitt Positionen`.

### Task 8: Ausgabe in SVG/DXF aus dem Export-Fenster

**Files:**
- Modify: `crates/planora-app/src/export_dialog.rs` (`PartsInput`, `prepare_drawing_parts`, Sperre G13, Tabelle nach G12, DXF-Prüfung der Zellen)
- Test: `crates/planora-app/tests/export_drawing.rs` (neue Tests)

**Interfaces:**
- Consumes: 1–7; `FrameInput`, `prepare_drawing_framed`, `dxf_sheet_texts_ok`.
- Produces:
```rust
#[derive(Clone, Debug, Default)]
pub struct PartsInput { pub cells: Vec<PartsCells> /* Zeilen dieses Blatts */, pub balloons: BTreeMap<BalloonId, Option<u32>> }
pub fn parts_input(state: &AppState, drawing: &Drawing) -> PartsInput;
pub fn prepare_drawing_parts(drawing: &Drawing, solids: &[(String, Solid)], compute: ComputeFn, resolve: ResolveFn,
                             frame: Option<&FrameInput>, parts: &PartsInput, progress: &Progress) -> Result<DrawingPrepared, String>;
// prepare_drawing_framed(..) = prepare_drawing_parts(.., &PartsInput::default(), ..)
pub fn lost_balloon_text(view: &str) -> String;   // G13
```
Ablauf: verlorener Kreis → `Err(lost_balloon_text)` vor dem Bau des Blatts; Kreise über `balloon_marks` + `add_balloon_marks`; Zeichnungsblatt: Tabelle nur mit Rahmen und Schalter; Stücklistenblatt: ohne Rahmen `model.frame = Some(SheetFrame { marks: parts_table_marks(..) })`, mit Rahmen der volle Rahmen; DXF-Zellen in `dxf_sheet_texts_ok` mit dem Namen „Stückliste, Pos. {n}“. Der Schlüssel der Berechnung folgt `edit_count` (deckt Überschreibungen und Kreise ab).

- [ ] **Step 1: Failing tests (`export_drawing.rs`):** (a) `balloons_always_written` (Rahmen aus und an: SVG mit Kreis und „1“ in der Ansichtsgruppe; DXF `TEXT` „1“ auf `POSITIONEN`); (b) `table_follows_frame_switch` (Zeichnungsblatt mit Schalter: Rahmen an → „Benennung“ in SVG und DXF; aus → nicht; Schalter im Blatt aus → nie); (c) `parts_sheet_exports_its_table` (Stücklistenblatt, Rahmen aus: „Benennung“ und Namen da, kein „Fertigungszeichnung“, keine Ebene `BLATTRAHMEN`; an: beides; Blatt 2 von 2 enthält Pos. „45“); (d) `lost_balloon_blocks_export` (Körper gelöscht → Status zeigt `lost_balloon_text("Vorne")`, keine Datei; Kreis löschen → Export gelingt); (e) `dxf_rejects_bad_cell_text` (Benennung „😀“ → DXF-Meldung mit „Stückliste, Pos. 1“, SVG wird geschrieben); (f) `special_chars_in_cells` („ä Ø“ → SVG maskiert, DXF `\U+00E4`, `%%c`); (g) `override_change_restarts` (Überschreibung bei offenem Fenster ändern → neue Berechnung, Datei mit neuem Text).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test export_drawing -j 2`
- [ ] **Step 3: Implement** nach Interfaces, G12, G13.
- [ ] **Step 4: Run → PASS**, dazu `--test export_dialog --test export_dimensions`, `cargo test -p planora-drawing -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweis:** Sperre nur bei Maßen → (d) rot.
- [ ] **Step 6: Commit** — `feat: Stückliste und Positionsnummern im Export`.

### Task 9: Dokumentation, Gesamtprüfung, Handliste, Test im echten Fenster

**Files:**
- Create: `crates/planora-drawing/tests/hand_samples_parts.rs` (`#[ignore]`, Proben)
- Modify: `README.md` (Abschnitt Zeichnungen: Stückliste, Stücklistenblatt, Positionsnummern; Grenze: Netz-Körper stehen nicht in der Stückliste (F6)), `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt 3c-2 – Stückliste“: Geliefertes, G1–G15, Grenzen: Maße nicht aus der Norm belegt (Spec §8), gleichnamige Körper mit verschiedener Geometrie bleiben eine Zeile, Benennung einer Booleschen Operation (Q1), Laden gleicht Blätter nicht ab, Tabelle des Stücklistenblatts in der SVG-Gruppe „Blattrahmen“), `docs/superpowers/specs/2026-10-18-stueckliste-design.md` (neuer §10 „Festlegungen der Umsetzung“), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)

- [ ] **Step 1: Proben erzeugen** (`hand_samples_parts.rs`): SVG und DXF mit und ohne Rahmen für A4 hoch, A4 quer, A3 quer, A0 quer mit Tabelle (3 und 12 Zeilen, Längstexte, Sonderzeichen) und Kreisen (auch Nummer „200“), dazu ein Stücklistenblatt mit 2 Seiten, nach `D:\Testumgebung\zeichnung-proben\stueckliste\`.
- [ ] **Step 2: Doku** in Laiensprache; Handliste für den Nutzer: Projekt mit drei Teilen (zwei gleich benannt) → neue Zeichnung zeigt die Tabelle; Teil umbenennen, verschieben, verrunden, vereinigen → Tabelle und Kreise folgen, Werkzeug-Körper wird rot; Kreis setzen, ziehen, löschen (Entf), Esc; Abschnitt „Stückliste“ ausfüllen, zurücksetzen, abbrechen; Schalter aus/an; Stücklistenblatt erzeugen, öffnen, 45 Teile → zweites Blatt, Teile löschen → Blatt verschwindet, erstes löschen → alle weg; alte Datei öffnen und speichern (bleibt alt); 800 × 600 und groß, Hell und Dunkel; SVG im Browser/Inkscape, DXF in Fusion (Texte zeigt Fusion nicht, bekannt aus TP12), Druck auf A4/A3 (nichts überlappt).
- [ ] **Step 3: Gates** mit Ausgabe: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2`; `cargo deny check`; `git diff --stat 84148e4 -- crates/planora-kernel crates/planora-mesh/src crates/planora-step crates/planora-exchange` leer; `git diff 84148e4 -- '**/Cargo.toml' Cargo.lock` ohne neue Pakete; `git status` zeigt nur `assets/` unverfolgt.
- [ ] **Step 4: Commit** — `docs: Stückliste und Positionsnummern (3c-2), Festlegungen, Handliste`.
- [ ] **Step 5 (Controller, nur mit Freigabe des Nutzers):** eigener Test im echten Fenster vor der Merge-Frage: Release bauen (Vordergrund, `-j 2`), App starten, mit `tools/gui-drive.ps1` (click/dclick/rclick/drag/wheel/type, Bildschirmfoto `%TEMP%\planora_shot.png`) den Kernweg der Handliste durchspielen (Tabelle, Kreis setzen/ziehen/löschen, Stücklistenblatt, Export SVG), Bildschirmfotos ansehen, Befunde ins Ledger; App danach beenden, kein Prozess bleibt zurück.
- [ ] **Step 6 (Controller):** Haltepunkt: Nutzer prüft die Handliste; Gesamtreview des Zweigs, eine Korrekturrunde, Merge-Frage.

---

## Bestehende Tests und Golden-Dateien, die sich ändern

- **Nur Übersetzen (neue Felder):** Literale `Drawing { .. }` (`stage12_types.rs`, `planora-feature/tests/dimensions.rs`, `convert.rs`), `SheetInfo { .. }` (`convert.rs`, `sheet_format.rs`), `TitleTexts { .. }` (`stage22_sheet_output.rs` ×4, `export_drawing.rs:996`), `Snapshot { .. }` (10 Stellen) – Werte gleich, Erwartungen gleich.
- **`MarkKind` erschöpfend:** `svg.rs`, `dxf.rs`, `drawing_view.rs` bekommen den Zweig `Position`; Tests mit erschöpfendem `match` (`stage22_sheet_output.rs`) ebenso.
- **Formatnummer:** `sheet_format.rs` (`FORMAT_VERSION == 6` → 7, „zu neue Datei“ jetzt mit 8).
- **Schalter-Vorgabe (F4):** Tests, die über `add_drawing_stamped` oder „Neue Zeichnung“ eine Zeichnung in einem Dokument mit mehr als einem Körper anlegen, bekommen die Tabelle: Format 7 statt 6 (`planora-feature/tests/sheet_info.rs`, `sheet_format.rs`) und kleinerer freier Bereich bzw. anderer Maßstabsvorschlag (`sheet_view.rs`, `drawing_ui.rs`, `sheet_dialog.rs`, `wide_dialogs.rs`). Jede geänderte Erwartung wird mit der neuen Rechnung im Kommentar und im Commit genannt.
- **Unverändert (muss so bleiben):** Golden-Dateien `svg_without_marks_{box,cylinder}.svg`, `dxf_without_marks_{box,cylinder}.dxf`, `stage15_sheet_lines.txt`; Fixtures Format 1–5; `stage20_sheet_frame.rs` und alle Tests über `sheet_layout` ohne Stückliste.

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung:** §2 Ziele: Zeilen je Körper und Menge → 2; Tabelle mit Schalter und Vorgabe → 1, 3 (G8), 6; Stücklistenblatt mit Folgeblättern → 1 (e/f), 3, 6 (f); Positionsnummern → 4, 7; SVG/DXF → 4, 8. Nicht-Ziele unberührt (kein automatisches Setzen, kein Umordnen). Anti-Ziele: veraltete Stückliste → Ableitung bei jedem Bild/Export aus dem Dokument (2, 6, 8); falsches Teil → G1, 2 (a/b/i), 4 (k); Text aus Zellen → 1 (d); Eingabeverlust → 5 (c); alte Dateien → 5 (b), 3 (f); Verschieben beim Neuberechnen → G9 + 4 (f), 7 (g). §3 Datenquelle, Spalten, Vorbelegung, Grenzen, Reihenfolge, Lage, Reserve, Warnung, Hinweisfeld-Regel, Stücklistenblatt → 1, 2, 3, 6. §4 Setzen, Ziehen, Wählen, Entf, Esc, Kennung, verloren, Nummer, Zeichnung, mehrere Kreise, keine auf dem Stücklistenblatt → 4, 7. §5 Abschnitt, Knopf, Werkzeug, Browser → 6, 7. §6 Ausgabe, Sperre, Format 7, Grenzen → 4, 5, 8. §7 Prüfungen → verteilt wie angegeben; Handliste und eigener Test → 9. §9 F1 → G1/G3, 2; F2 → G9, 4 (e/f); F3 → G7, 3, 6 (g); F4 → G8, 3 (g), 6 (a/j); F5 → 2 (c), 4 (k), 7 (f); F6 → 2 (d), Doku 9.
**Schritt-Scan:** Jeder Testschritt nennt Namen, Eingaben und Zahlen; Zahlen der Geometrie aus der 3c-1-Regel mit Faktor nachgerechnet; Rümpfe fehlen absichtlich, offene Algorithmen stehen als G1–G15.
**Typkonsistenz:** `BodyKey`, `RowOverride`, `PartsRow`, `PartsCells`, `PartsSpace`, `sheet_layout_with`, `parts_rows_per_page`, `pages_needed`, `page_rows`, `parts_table_marks`, `parts_cell_is_cut`, `DrawingKind` (1) → 2–8; `body_key`, `row_of`, `parts_rows`, `parts_cells`, `set_parts_override` (2) → 3, 4, 6–8; `add_parts_sheets`, `sync_parts_sheets`, `parts_sheets` (3) → 6; `Balloon`, `BalloonId`, `balloon_marks`, `add_balloon_marks`, `MarkKind::Position`, `add_balloon`, `balloon_numbers` (4) → 5, 7, 8.
**Proportion:** Entscheidungen, Zahlen, Schnittstellen und Tests; keine Funktionsrümpfe.
**Bekannte Risiken:** Aufgabe 6 ist die größte (Anzeige, Fenster, Browser); viele bestehende App-Tests mit „Neue Zeichnung“ und mehreren Körpern ändern Erwartungen durch F4; die Lage des Knopfs „Position“ hängt an der Breitenmessung (G11 regelt den Ausweg). Empfehlung Controller: opus für 1, 4, 6, 7; sonnet für 2, 3, 5, 8, 9.

**Offene Frage an den Nutzer (ändert, was gebaut wird):** Q1 – Das Ergebnis einer Booleschen Operation heißt heute überall wie die Operation („Vereinigung 1“). Die Kennung folgt nach F1 dem Ziel-Körper. Soll auch die Benennung (in der Stückliste und überall sonst) dem Ziel folgen („Platte“)? Der Plan lässt den Namen wie heute (G2); eine Änderung beträfe nur `Document::body_display_name` (Aufgabe 2) und die Namen im Browser.

## Abweichungen bei der Umsetzung

Stand 2026-10-18, nach Aufgabe 8; Einzelheiten und Begründungen: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 3c-2“, und die Berichte unter `.superpowers/sdd/2026-10-18-stueckliste/`.

- **Aufgabe 1:** Auf Stücklistenblättern (`ListPage`) ist `SheetLayout.free` die Tabellenfläche (F8 ergäbe dort einen leeren Bereich); `sheet_layout` ruft `sheet_layout_with` mit `PartsSpace::None` auf, Ergebnisse unverändert.
- **Aufgabe 2:** Der Kern liefert je Boolescher Operation genau einen Körper; `boolean_split_has_own_key` prüft Index ≥ 1 nur strukturell. `naming_origin` ist öffentlich. Nachbesserung: Gruppieren nach genauem Namen (Groß-/Kleinschreibung, kein Trim), Pos. = laufende Zeilennummer.
- **Aufgabe 3:** Zusätzlich reserviert: die Namen „Stückliste“ bis „Stückliste (8)“ für andere Zeichnungen; `add_parts_sheets` lehnt bei belegtem Folgenamen ab; `set_drawings` prüft Seiten ohne Lücke; der Abgleich ändert nur `drawing_edits`; `update_drawing` an einem Stücklistenblatt zählt nur bei Wirkung. Die Regel „kein Kreis auf einem Stücklistenblatt“ kam mit Aufgabe 4 in `validate_drawing`.
- **Aufgabe 4:** Kreis und Punkt als volle `Mark::Arc` (DXF `CIRCLE`); verlorene Kreise als `MarkKind::Lost` (Gruppe „Maße“ / Ebene `MASSE_FEHLT`); die DXF-Strichstärken der Ebenen `POSITIONEN`/`POSITIONEN_LINIE` skalieren mit dem Blattfaktor (Entscheid: angenommen); die DXF-Zahl wird mit `balloon_text_dxf` an den Zuschlag 1,45 angepasst (Nachbesserung).
- **Aufgabe 5:** Der Kennungszähler der Kreise wird bei Format 7 mitgeschrieben (auch nach Löschen); eingefrorene Prüfdateien `format6_sheet_info.json` und `format7_parts.json`; Format 8 statt 7 ist „zu neu“. Die Lesegrenzen liegen beim Streamen (`de_bounded`, `de_capped`); keine Abweichung von der Vorgabe außer der fehlenden Rot-Phase (Implementierung vor dem ersten Lauf, Wirksamkeit durch Mutationen belegt).
- **Aufgabe 6:** Neuer Fußbereich `Dialog::show_with_footer` (Blattangaben benutzt ihn); der Abschnitt ist kompakt (eine Zeile je Teil, Kreuz-Symbol als Zurücksetzen); ein bestehender Test änderte seine Erwartung (`wide_dialogs::sheet_dialog_stays_inside_and_scrolls_at_800x600`: „Übernehmen“ immer sichtbar). Kein anderer Test brauchte wegen der Schalter-Vorgabe (F4) eine neue Erwartung, weil die betroffenen Suiten nur Dokumente mit einem Körper anlegen. „Rechts“ im Erstwinkel und eine zu große Isometrie können den freien Bereich verlassen (3c-1-Verhalten, geprüft wird nur „nicht unter der Tabelle“).
- **Aufgabe 7:** Der Knopf „Position“ ist beschriftet in Zeile 2; dafür ist „Blatt einpassen“ ein Symbolknopf und das Maß-Auswahlfeld hat feste Breite (Entscheid des Controllers; G11 sah nur die zwei Orte vor). Entf-Wächter mit vier Bedingungen; Zwischenspeicher für Nummern und Kreise je (Instanz, Änderungsstand); der Hinweistext `HINT` der Zeichnungsansicht nennt Positionsnummern nicht (bestehende Tests prüfen ihn wörtlich).
- **Aufgabe 8:** `PartsInput` hat ein drittes Feld `dxf`; die DXF-Meldung nennt Zeile und Spalte („Stückliste, Pos. {n}, {Spalte}“); der DXF-Schreiber legt die Blattebenen je Strichstärke an (Goldene Dateien gleich), sonst gäbe es beim Stücklistenblatt ohne Rahmen eine leere Ebene `BLATTRAHMEN`; Wortlaut der Sperre wie G13.
- **Aufgabe 9:** Proben im neuen Test `hand_samples_parts.rs`; die Tabelle des Stücklistenblatts ohne Rahmen liegt in der SVG-Gruppe `Blattrahmen`. Der Test im echten Fenster (Schritt 5) und die Haltepunkte (Schritt 6) gehören dem Controller und brauchen die Freigabe des Nutzers.
