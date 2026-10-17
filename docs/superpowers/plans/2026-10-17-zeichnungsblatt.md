# Zeichnungsblatt (Teilprojekt 3c-1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aus dem Blatt mit dünnem Innenrahmen wird ein formal vollständiges Zeichnungsblatt (Rahmen mit Heftrand, Zentriermarken, Koordinatenraster, Schriftfeld, Änderungstabelle, Hinweisfeld) mit vorbelegten, im Fenster „Blattangaben“ änderbaren Texten, gespeichert in Dateiformat 6 und ausgegeben in SVG/DXF mit dem Schalter „Blattrahmen mit ausgeben“.

**Architecture:** `planora-drawing` bekommt drei reine Bausteine: `sheet_frame.rs` (Blattgeometrie aus Konstanten: Rahmen, Marken, Raster, Schriftfeldzellen, Änderungstabelle, Hinweisfeld, freier Bereich), `sheet_info.rs` (Datenmodell der Angaben, Vorbelegung, Auflösung zu Texten) und `sheet_marks.rs` (Texte einpassen, alles als `Mark` mit neuer `MarkKind::Sheet(SheetPen)`). `SheetModel.frame` wird `Option<SheetFrame>`; die Schreiber geben den Rahmen nur aus, wenn er gesetzt ist (der alte Innenrahmen entfällt ganz). `Drawing.info` (je Blatt) und `Document::project_info` (je Dokument) tragen die Daten; Format 6 speichert sie. Die App baut den Rahmen für die Anzeige immer, für den Export nach Schalter, und ordnet neue Ansichten im freien Bereich an.

**Tech Stack:** Rust (Workspace, keine neuen Abhängigkeiten), egui/eframe 0.36, serde_json (vorhanden); Tests im eigenen Harness (`egui::Context::run_ui`, `PlanoraApp::for_tests`), SVG/DXF-Leser in `crates/planora-drawing/tests/common/`.

**Spec:** `docs/superpowers/specs/2026-10-17-zeichnungsblatt-design.md` (bindend). Vorbild für Stil und Formaterweiterung: `docs/superpowers/plans/2026-10-17-graphit-etappe-d.md`, `docs/superpowers/specs/2026-10-15-masse-mittellinien-design.md` (P11 Format 5). Basis: `master` @ `42a6d68`; Arbeitszweig `feature/zeichnungsblatt`.

## Global Constraints

- Texte deutsch in einfacher Laiensprache, keine technischen Hinweise, keine Hinweise auf Datenherkunft (Nutzerregel); feste deutsche Feldnamen auf dem Blatt (Spec §2 Nicht-Ziele).
- Farben nur über `theme::` (Wächter `tests/no_hard_coded_colors.rs`); Einzeilen-Textfelder nur über `theme::text_field` (Wächter `no_raw_text_edits`); Fenster nur über `dialog::Dialog` (Wächter `no_raw_windows`), Blattangaben als `Dialog::new("Blattangaben", DialogSize::Wide)`; Zeichen nur aus Montserrat/Symbolschrift (`tests/fonts.rs`).
- Das Blatt bleibt in der App weißes Papier (`theme::PAPER`) mit schwarzer Zeichnung (`theme::SHEET_INK`), Hell und Dunkel; nur die Umgebung folgt dem Theme (Spec §5).
- Maße aus Spec §3 als benannte Konstanten (Tests mit den Zahlen der Spec): Rahmen links 20 mm, sonst 10 mm, Linie 0,7; Zentriermarken 0,5, bis 5 mm in den Rahmen; Rasterfeld 50 mm, Raster-Schrift 3,5; Schriftfeld 180 × 56, Außenlinie 0,7, Innenlinien 0,25, Feldname 2,2, Inhalt 3,5, Titel 5; Änderungstabelle 176 breit, höchstens 6 Zeilen sichtbar; Hinweisfeld Schrift 3,3, neben dem Schriftfeld (110 breit, 10 Abstand) ab 310 mm Rahmenbreite, sonst darüber (180 breit).
- Grenzen (Spec §4): Textfeld 80 Zeichen, Titel 60, Hinweiszeile 100, Änderungsbeschreibung 120; höchstens 6 Hinweiszeilen, höchstens 23 Änderungen (Buchstaben ohne I, O, Q), Meldung „höchstens 23 Änderungen“; Einpassen: verkleinern bis 60 % der Schrifthöhe, dann mit „…“ kürzen.
- Dateiformat: `FORMAT_VERSION = 6`; Format 6 nur, wenn das Dokument projektweite Angaben oder einen Blattangaben-Satz enthält; sonst bleibt alles Byte für Byte wie bisher; nichts wird beim bloßen Öffnen verändert (Spec §7).
- Geschützte Pfade unverändert gegenüber `42a6d68`: `crates/planora-kernel/**`, `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`. Keine neuen Abhängigkeiten; `cargo deny check` sauber.
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests mit `-j 2` (langsamer Rechner; nie `cargo clean`). Am Ende `cargo test --workspace -j 2` und `cargo deny check`.
- Jeder Commit endet mit der Zeile `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Abweichungen von der Spec (Normprüfung, Umsetzung) stehen im Ledger des Controllers und in `docs/superpowers/offene-punkte.md`; Zahlen der Spec ändern sich nur nach Zustimmung des Nutzers.

**Festlegungen (Entwurfsverfasser des Plans; der Nutzer kann sie kippen):**
- **F1** Ein Feld ist „automatisch“, solange es leer ist; „Zurücksetzen“ leert es. Gespeichert: `FieldValue::{Auto, Text(String)}` (Datei: fehlt/`null` = automatisch).
- **F2** Datum = Kalendertag in UTC (eigene Rechnung nach Hinnant wie `planora-project::fsutil`, keine Abhängigkeit für Ortszeit), Schreibweise `JJJJ-MM-TT` (ISO 8601). Kurz nach Mitternacht Ortszeit kann es der Vortag sein (änderbar, in `offene-punkte.md`).
- **F3** `Document::add_drawing` bleibt unverändert (Zeichnung ohne Satz, wie aus Format ≤ 5; alle bestehenden Format-4/5-Tests bleiben gültig). Neu `Document::add_drawing_stamped`, das die App beim Anlegen benutzt: Satz mit Ausgabedatum heute, projektweite Angaben (Ersteller = Windows-Anmeldename `USERNAME`, sonst `USER`, sonst leer), falls noch keine da sind.
- **F4** Alte Zeichnungen bekommen Satz und projektweite Angaben erst mit „Übernehmen“ im Fenster (Öffnen und „Abbrechen“ ändern nichts); das Ausgabedatum ist dann der Tag des Übernehmens, wenn nichts eingetragen ist.
- **F5** (Stand vor der Skalierung, siehe „Abweichungen bei der Umsetzung“) Raster: Felderzahl je Kante `2 · round(Kante / 100)` (gerade Zahl nächst Kante/50; ergibt die ISO-Tabelle A4 6 × 4 … A0 24 × 16), Teilung 50 mm von der Blattmitte (Zentriermarke) nach außen, Randfelder nehmen den Rest. Teilstriche 0,25 vom Rahmen 5 mm nach außen (nicht an der Mitte, dort steht die Zentriermarke); Beschriftung mittig im Rand (oben/unten 5 mm von der Blattkante, links 10, rechts 5), Zeilenbuchstaben ohne I und O (24 reichen für A0).
- **F6** Schriftfeld-Zellen (relativ zur linken unteren Ecke, mm; Zeilen von unten): y 0–10: Zeichnungsnummer 0–100, Änderungsstand 100–125, Blatt 125–155, Sprache 155–180; y 10–28: Titel 0–180 (Titel 5 mm Grundlinie y 18,5, Zusatztitel 3,5 Grundlinie y 12,5); y 28–38: Eigentümer 0–80, Dokumentart 80–135, Dokumentstatus 135–180 (17 Zellen, Höhe bleibt 56); y 38–47: Ersteller 0–45, Geprüft 45–90, Freigabe 90–135, Ausgabedatum 135–180; y 47–56: Werkstoff 0–60, Gewicht 60–90, Maßstab 90–115, Format 115–135, Projektion 135–180. Feldname links oben (x0 + 1, Grundlinie y1 − 1 − 2,2), Inhalt Grundlinie y0 + 1,5, Breite x1 − x0 − 2.
- **F7** Projektionssymbol (h = 3,5): Kegelstumpf als Trapez, Länge 7, linke (schmale) Seite 3,5 hoch, rechte 7 hoch; Kreise Ø 7 und Ø 3,5 konzentrisch; 3,5 Abstand. Methode 1: Trapez links, Kreise rechts; Methode 3: Kreise links, Trapez rechts (die schmale Seite zeigt bei 3 zu den Kreisen). Linien 0,5; in der Zelle Projektion rechts vom Feldnamen, Mitte y 51,5 (Zelle 47–56). **Nutzerentscheidung:** „gespiegelt“ (Spec §6) heißt genau das – der Kegelstumpf behält seine Richtung, Kegelstumpf und Kreise tauschen die Seiten; Aufgabe 1 prüft es gegen die Norm.
- **F8** Freier Bereich = größere zweier Kandidaten (bei Gleichstand der erste): (a) volle Rahmenbreite zwischen Oberkante des Stapels unten (Schriftfeld, Hinweisfeld) und Unterkante der Änderungstabelle; (b) volle Rahmenhöhe links vom linkesten Hindernis. Platz für die Stückliste (3c-2) wird in 3c-1 nicht freigehalten.
- **F9** (Schwelle gilt seit der Skalierung als 310·f) Hinweisfeld nach der Zahlenregel (Rahmenbreite ≥ 310 mm → neben dem Schriftfeld), also auch A2 hoch und größer hoch daneben (**Nutzerentscheidung**, Spec §3 entsprechend berichtigt in `42a6d68`); darüber nur A4 quer/hoch und A3 hoch. Höhe `2 · 2 + n · 6` mm bei n Zeilen, ohne Zeilen kein Feld; Zeile i Grundlinie `oben − 2 − (i + 1) · 6 + (6 − 3,3)/2`, x = links + 2.
- **F10** Änderungstabelle: Zeilen 7 mm, Spalten Rev. 12 | Änderung 104 | Datum 30 | Name 30, Schrift 3,5 (Kopf und Zeilen), Höhe `7 · (1 + min(n, 6))` (ohne Änderungen nur die Kopfzeile). Tabelle und Hinweisfeld mit Linien 0,25; Rahmen und Schriftfeld-Außenlinie 0,7.
- **F11** Textbreite für das Einpassen konservativ (`sheet_text_width`): je Zeichen `M W m w @ %` 0,95·h, `i l j I 1 . , : ; ' | !` 0,3·h, übrige Großbuchstaben und Ziffern 0,72·h, Rest 0,6·h.
- **F12** Hinweiszeilen: projektweit `NotesValue::Auto` = `["Allgemeintoleranzen {Allgemeintoleranz}", "Kanten gebrochen 0,3 mm", "Oberflächen Ra 3,2 (soweit nicht anders angegeben)"]` (Allgemeintoleranz automatisch „ISO 2768-m“); je Blatt `Auto` = die projektweiten, `Lines(..)` = eigene.
- **F13** Ausgabe: SVG-Gruppe `<g id="Blattrahmen">` als erste Gruppe in der gespiegelten Wurzelgruppe; DXF-Objekte direkt im Modellbereich vor den `INSERT`s auf den Ebenen `BLATTRAHMEN` (Farbe 7, `CONTINUOUS`, 70), `BLATT_MITTE` (50, Zentriermarken), `BLATT` (25, übrige Linien und Texte) – nur angelegt, wenn der Rahmen ausgegeben wird.
- **F14** Fenster „Blattangaben“ arbeitet auf einer Kopie; „Übernehmen“ schreibt, „Abbrechen“/Schließen verwirft. In alten Versionen (schreibgeschützt) gesperrt wie die anderen Bearbeitungsknöpfe.
- **F15 (Nutzerentscheidung)** Feld „Dokumentstatus“ (ISO 7200): je Blatt, automatisch „in Arbeit“, eigener Text wie die übrigen Felder (höchstens 80 Zeichen), eigene Zelle im Schriftfeld (F6), im Fenster unter „Dieses Blatt“, in der Datei `dokumentstatus`.

## Review Focus

1. **Alte Dateien bleiben unberührt:** Format-4/5-Datei öffnen, Zeichnung ansehen, Fenster öffnen und abbrechen, speichern → Format und Bytes unverändert, `edit_count` unverändert. → Aufgabe 3 Test (e), Aufgabe 6 Test (c).
2. **Längstexte, Sonderzeichen, leere Felder in allen Papiergrößen:** kein Text ragt über seine Zelllinie (auch Änderungs- und Hinweiszeilen), „ä ö ü ß Ø ° ±“ in SVG und DXF richtig; ein Zeichen außerhalb der Grundebene (z. B. 😀) sperrt nur DXF mit benannter Meldung. → Aufgabe 4 Tests (b), (f); Aufgabe 6 Test (h).
3. **Blatt n/N und Zeichnungsnummer folgen:** Zeichnung löschen/anlegen, Dokument umbenennen (Speichern unter) → automatische Werte folgen, eigene Texte bleiben; der Export nimmt den Stand zum Zeitpunkt des Exports. → Aufgabe 2 Test (d), Aufgabe 5 Test (c), Aufgabe 6 Test (g).
4. **Freier Bereich wird nie leer:** alle Papiergrößen beider Ausrichtungen × 0–6 sichtbare Änderungen × 0–6 Hinweiszeilen → freier Bereich mit positiver Breite und Höhe, ohne Überschneidung mit Schriftfeld, Tabelle, Hinweisfeld; Maßstabsvorschlag bleibt in der Reihe. → Aufgabe 1 Test (f), Aufgabe 5 Test (d).
5. **Blatt bleibt auf dem Papier:** jede Markierung des Rahmens liegt in `[0, w] × [0, h]` (Drucken), SVG `width/height` in mm unverändert; Rahmen auch bei 800 × 600 und Zoom 0,1 ohne Absturz gemalt. → Aufgabe 4 Test (e), Aufgabe 5 Test (a).

---

### Task 1: Normprüfung und Blattgeometrie (`sheet_frame.rs`)

**Files:**
- Create: `crates/planora-drawing/src/sheet_frame.rs`
- Modify: `crates/planora-drawing/src/lib.rs` (`pub mod sheet_frame;`, Re-Exporte), `crates/planora-drawing/src/model.rs` (`Paper::label`)
- Modify: `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt 3c-1 – Zeichnungsblatt“, Unterabschnitt „Normprüfung“)
- Test: `crates/planora-drawing/tests/stage20_sheet_frame.rs` (neu)

**Interfaces:**
- Consumes: `model::{Sheet, Paper, Projection}`.
- Produces:
```rust
pub const FILING_MARGIN_MM: f64 = 20.0;      // links
pub const EDGE_MARGIN_MM: f64 = 10.0;        // oben, rechts, unten
pub const CENTRE_MARK_INSET_MM: f64 = 5.0;
pub const GRID_FIELD_MM: f64 = 50.0; pub const GRID_TEXT_H: f64 = 3.5; pub const GRID_TICK_MM: f64 = 5.0;
pub const TITLE_W: f64 = 180.0; pub const TITLE_H: f64 = 56.0;
pub const LABEL_H: f64 = 2.2; pub const VALUE_H: f64 = 3.5; pub const TITLE_TEXT_H: f64 = 5.0;
pub const REV_W: f64 = 176.0; pub const REV_ROW_H: f64 = 7.0; pub const REV_VISIBLE: usize = 6;
pub const REV_COLUMNS: [f64; 4] = [12.0, 104.0, 30.0, 30.0];
pub const NOTES_TEXT_H: f64 = 3.3; pub const NOTES_LINE_MM: f64 = 6.0; pub const NOTES_PAD_MM: f64 = 2.0;
pub const NOTES_SIDE_W: f64 = 110.0; pub const NOTES_GAP_MM: f64 = 10.0; pub const NOTES_SIDE_MIN_FRAME_W: f64 = 310.0;
pub const SYMBOL_H: f64 = 3.5;
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SheetPen { Heavy, Medium, Thin }
impl SheetPen { pub fn width_mm(self) -> f64 }                 // 0.7 / 0.5 / 0.25
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TitleField { Owner, Number, Title, Subtitle, DocType, Status, Issued, Revision, SheetOf,
                      Format, Scale, Creator, Checked, Approved, Material, Weight, Language }
impl TitleField { pub const ALL: [TitleField; 17]; pub fn label(self) -> &'static str; pub fn max_chars(self) -> usize }
pub struct TitleCell { pub x0: f64, pub x1: f64, pub y0: f64, pub y1: f64, pub label: &'static str }
pub struct TextSlot { pub field: TitleField, pub x0: f64, pub width: f64, pub baseline: f64, pub height: f64 }
pub const TITLE_CELLS: [TitleCell; 17];   // nach F6, Projektion eingeschlossen
pub const TITLE_SLOTS: [TextSlot; 17];    // nach F6
pub type Box2 = (DVec2, DVec2);
pub struct SheetLayout {
    pub sheet: Sheet, pub frame: Box2,
    pub centre_marks: [(DVec2, DVec2); 4],
    pub grid_ticks: Vec<(DVec2, DVec2)>, pub grid_labels: Vec<(DVec2, String)>, // Textmitte
    pub title: Box2, pub revisions: Box2, pub notes: Option<Box2>, pub free: Box2,
}
pub fn sheet_layout(sheet: Sheet, revision_rows: usize, note_lines: usize) -> SheetLayout; // Zeilen auf 6/6 begrenzt
pub fn grid_count(len_mm: f64) -> usize;   // F5, mindestens 2
pub fn grid_letter(i: usize) -> Option<char>; // A…Z ohne I, O
pub fn projection_symbol(projection: Projection, centre_y: f64, x0: f64) -> (Vec<(DVec2, DVec2)>, [(DVec2, f64); 2]); // Trapez-Strecken, Kreise (Mitte, Radius)
```
`TitleField::label`: „Eigentümer“, „Zeichnungsnummer“, „Titel“, „Zusatztitel“, „Dokumentart“, „Dokumentstatus“, „Ausgabedatum“, „Änderung“, „Blatt“, „Format“, „Maßstab“, „Erstellt“, „Geprüft“, „Freigabe“, „Werkstoff“, „Gewicht“, „Sprache“; `max_chars` 60 für `Title`, sonst 80. `Paper::label`: „A4“ … „A0“ (die App-Funktion `drawing_dialog::paper_label` ruft sie auf).

- [ ] **Step 1: Normprüfung (leicht, höchstens eine Stunde):** öffentliche Zusammenfassungen zu ISO 5457 (Rand 20/10, Linienbreiten Rahmen und Zentriermarken, Länge der Zentriermarken, Rasterfeld 50 mm, Felderzahlen je Format, Buchstaben ohne I/O, Schrifthöhe), ISO 7200 (Breite 180, Pflichtfelder, Feldnamen-/Inhaltshöhen), ISO 5456-2 (Projektionssymbol: Maße, Lage von Kegelstumpf und Kreisen je Methode), ISO 3098 (Schriftreihe 2,5/3,5/5; Spec nennt 2,2 und 3,3). Ergebnis als Tabelle „Spec-Wert | gefundener Wert | Quelle | Abweichung ja/nein“ in `offene-punkte.md` und im Ledger; F5/F7 hier bestätigen oder als Abweichung melden. Zahlen der Spec nicht ändern; jede Abweichung geht als Frage an den Controller.
- [ ] **Step 2: Failing tests (`stage20_sheet_frame.rs`):** (a) `frame_rectangles` (A4 hoch `((20,10),(200,287))`, A4 quer `((20,10),(287,200))`, A3 quer `((20,10),(410,287))`, A0 quer `((20,10),(1179,831))`; `SheetPen` 0,7/0,5/0,25); (b) `centre_marks_a4_landscape` (links `(0,105)–(25,105)`, rechts `(297,105)–(272,105)`, unten `(148.5,0)–(148.5,15)`, oben `(148.5,210)–(148.5,195)`); (c) `grid_counts_and_labels` (`grid_count`: 210→4, 297→6, 420→8, 594→12, 841→16, 1189→24; A4 quer: 12 Teilstriche bei x 48,5/98,5/198,5/248,5 oben und unten, y 55/155 links und rechts, jeder 5 mm lang vom Rahmen nach außen; 20 Beschriftungen, Spalte 1 Mitte x 34,25 bei y 5 und 205, Zeile A Mitte links (10, 177,5); A0 hoch: Zeilen A…Z ohne I, O = 24, `grid_letter(24) == None`); (d) `title_block_and_tables` (A4 hoch Schriftfeld `((20,10),(200,66))`, Tabelle ohne Änderungen `((24,280),(200,287))`, mit 9 Änderungen 6 Zeilen: Unterkante 287 − 49 = 238; Hinweise 3 Zeilen darüber `((20,66),(200,88))`; A3 quer daneben `((110,10),(220,32))`; A2 hoch (Rahmen 390) daneben (F9); `TITLE_CELLS` (17, mit Dokumentstatus 135–180 × 28–38) decken 180 × 56 lückenlos und ohne Überlappung ab; jeder `TextSlot` liegt mit Höhe ganz in seiner Zelle und unter dem Feldnamen); (e) `free_area_examples` (A4 hoch `((20,88),(200,280))`, A4 quer `((20,88),(287,193))`, A3 quer `((20,66),(410,280))`, A0 quer `((20,66),(1179,824))`); (f) `free_area_never_empty` (alle 5 Papiere × hoch/quer × Änderungszeilen 0..=9 × Hinweiszeilen 0..=8: Breite und Höhe > 0, keine Überschneidung mit `title`, `revisions`, `notes`, ganz im Rahmen); (g) `projection_symbol_sides` (Methode 1: alle Trapez-Punkte links der Kreise, linke Trapezseite 3,5 hoch, rechte 7; Methode 3: Kreise links; Radien 3,5 und 1,75; alles innerhalb der Zelle Projektion und rechts vom Feldnamen).
- [ ] **Step 3: Run → FAIL.** `cargo test -p planora-drawing --test stage20_sheet_frame -j 2`
- [ ] **Step 4: Implement** nach Interfaces, F5–F10; freier Bereich nach F8.
- [ ] **Step 5: Run → PASS**, clippy, fmt.
- [ ] **Step 6: Commit** — `feat: Blattgeometrie nach ISO 5457/7200 (Rahmen, Raster, Schriftfeld, freier Bereich)`.

### Task 2: Blattangaben-Modell, Vorbelegung und Auflösung (`sheet_info.rs`)

**Files:**
- Create: `crates/planora-drawing/src/sheet_info.rs`
- Modify: `crates/planora-drawing/src/model.rs` (`Drawing.info: Option<SheetInfo>`, `Drawing::new` setzt `None`), `crates/planora-drawing/src/lib.rs`; alle Struct-Literale `Drawing { .. }` in Tests und `crates/planora-project/src/convert.rs::drawing_from` (vorerst `info: None`)
- Modify: `crates/planora-feature/src/drawings.rs` (`validate_drawing` prüft `info`)
- Test: `crates/planora-drawing/tests/stage21_sheet_info.rs` (neu), `crates/planora-feature/tests/drawings.rs` (Prüfung)

**Interfaces:**
- Consumes: `sheet_frame::{TitleField, REV_VISIBLE}` (1), `model::{Drawing, Scale, Paper::label}`, `sheet::SheetModel`.
- Produces:
```rust
#[derive(Clone, Debug, PartialEq, Default)] pub enum FieldValue { #[default] Auto, Text(String) }
impl FieldValue { pub fn resolve<'a>(&'a self, auto: &'a str) -> &'a str; pub fn from_input(s: &str) -> FieldValue } // leer/nur Leerraum → Auto (F1)
#[derive(Clone, Debug, PartialEq, Default)] pub enum NotesValue { #[default] Auto, Lines(Vec<String>) }
pub struct ProjectInfo { pub login: String, pub owner: FieldValue, pub creator: FieldValue, pub checked: FieldValue,
                         pub approved: FieldValue, pub material: FieldValue, pub tolerance: FieldValue, pub notes: NotesValue }
pub struct Revision { pub letter: FieldValue, pub description: String, pub date: String, pub name: String }
pub struct SheetInfo { pub created: String, pub number: FieldValue, pub title: FieldValue, pub subtitle: FieldValue,
                       pub doc_type: FieldValue, pub status: FieldValue, pub issued: FieldValue, pub revision: FieldValue, pub sheet_of: FieldValue,
                       pub format: FieldValue, pub scale: FieldValue, pub weight: FieldValue,
                       pub notes: NotesValue, pub revisions: Vec<Revision> }
// alle: #[derive(Clone, Debug, PartialEq)]
pub struct Stamp { pub date: String, pub user: String }
impl Stamp { pub fn now() -> Stamp }                       // utc_date(jetzt), USERNAME/USER/leer
pub fn utc_date(unix_secs: i64) -> String;                 // "JJJJ-MM-TT"
pub fn revision_letter(index: usize) -> Option<char>;      // 0→'A' … 22→'Z', ohne I/O/Q; 23→None
impl ProjectInfo { pub fn new(stamp: &Stamp) -> ProjectInfo }   // login = stamp.user, alles Auto
impl SheetInfo { pub fn new(stamp: &Stamp) -> SheetInfo;        // created = stamp.date, alles Auto, keine Änderungen
                 pub fn add_revision(&mut self, date: &str, name: &str) -> Result<(), &'static str>; } // ab 23: Err(TOO_MANY_REVISIONS)
pub const MAX_FIELD_CHARS: usize = 80; pub const MAX_TITLE_CHARS: usize = 60; pub const MAX_NOTE_CHARS: usize = 100;
pub const MAX_REVISION_CHARS: usize = 120; pub const MAX_NOTES: usize = 6; pub const MAX_REVISIONS: usize = 23;
pub const TOO_MANY_REVISIONS: &str = "Es sind höchstens 23 Änderungen möglich.";
pub const DEFAULT_DOC_TYPE: &str = "Fertigungszeichnung"; pub const DEFAULT_STATUS: &str = "in Arbeit"; pub const DASH: &str = "–";
pub const DEFAULT_TOLERANCE: &str = "ISO 2768-m"; pub const LANGUAGE: &str = "de";
pub fn default_notes(tolerance: &str) -> Vec<String>;      // F12
pub fn validate_sheet_info(i: &SheetInfo) -> Result<(), String>;    // Längen, ≤ 6 Zeilen, ≤ 23 Änderungen, Steuerzeichen
pub fn validate_project_info(p: &ProjectInfo) -> Result<(), String>;
pub struct SheetContext { pub doc_name: String, pub index: usize /* ab 1 */, pub count: usize }
#[derive(Clone, Debug, PartialEq)]
pub struct TitleTexts { pub fields: BTreeMap<TitleField, String>, pub revisions: Vec<[String; 4]>, pub notes: Vec<String> }
impl TitleTexts { pub fn get(&self, f: TitleField) -> &str; pub fn visible_revisions(&self) -> &[[String; 4]] } // die letzten 6
pub fn resolve_texts(d: &Drawing, project: Option<&ProjectInfo>, ctx: &SheetContext, main: Option<Scale>) -> TitleTexts;
pub fn main_scale(d: &Drawing) -> Option<Scale>; // Abschlussrunde: Maßstab der ersten Ansicht (vorher: meiste Linien, `model`-Parameter)
```
Auflösung (Spec §4): Eigentümer `owner`/„“; Zeichnungsnummer `"{doc_name}-{index:02}"`; Titel `d.name`; Zusatztitel „“; Dokumentart `DEFAULT_DOC_TYPE`; Dokumentstatus `DEFAULT_STATUS`; Ausgabedatum `issued`/`created`; Änderungsstand Buchstabe der letzten Änderung (eigener oder `revision_letter(i)`), sonst „“; Blatt `"{index} von {count}"`; Format `d.sheet.paper.label()`; Maßstab `main.label()` sonst `DASH`; Erstellt `creator`/`login`; Geprüft, Freigabe „“; Werkstoff, Gewicht `DASH`; Sprache `LANGUAGE`. Ohne `d.info`: Blattfelder automatisch, Ausgabedatum `DASH`; ohne `project`: Eigentümer, Erstellt, Geprüft, Freigabe, Werkstoff `DASH` (Spec §7). Hinweise nach F12 (ohne Projekt: `default_notes(DEFAULT_TOLERANCE)`). `main_scale`: Ansicht aus `model.views` mit den meisten `lines`, Gleichstand die erste in Zeichnungsreihenfolge; ist keine berechnet, der Maßstab der ersten Ansicht; ohne Ansichten `None`.

- [ ] **Step 1: Failing tests (`stage21_sheet_info.rs`):** (a) `revision_letters` (0→A, 7→H, 8→J, 13→P, 14→R, 22→Z, 23→None; keine I/O/Q unter den 23); (b) `utc_dates` (`utc_date(0)` „1970-01-01“, `1_792_195_200` „2026-10-17“, `951_868_799` „2000-02-29“); (c) `defaults_of_a_new_sheet` (Stamp {„2026-10-17“, „dennis“}, Zeichnung „Halter“ A3 quer, Kontext {„Werkstück“, 2, 3}, Maßstab 1:2: Zeichnungsnummer „Werkstück-02“, Titel „Halter“, Dokumentart „Fertigungszeichnung“, Dokumentstatus „in Arbeit“, Ausgabedatum „2026-10-17“, Änderung „“, Blatt „2 von 3“, Format „A3“, Maßstab „1:2“, Erstellt „dennis“, Werkstoff „–“, Gewicht „–“, Sprache „de“, Eigentümer „“, Hinweise = die drei Zeilen aus F12 mit „Allgemeintoleranzen ISO 2768-m“); (d) `custom_text_wins_and_reset_restores` (Titel `Text("Lager")` → „Lager“; Dokumentstatus `Text("freigegeben")` → „freigegeben“, zurückgesetzt → „in Arbeit“; `from_input("  ")` → Auto → wieder „Halter“; Kontext {„Neu“, 1, 1} ändert Nummer und Blatt, eigener Text bleibt); (e) `old_drawing_without_info` (`info: None`, `project: None`: Ausgabedatum, Eigentümer, Erstellt, Geprüft, Freigabe „–“, übrige wie (c)); (f) `revisions_and_state` (`add_revision` 23-mal ok, 24. → `Err(TOO_MANY_REVISIONS)`, Text enthält „höchstens 23 Änderungen“; Änderungsstand = Buchstabe der letzten, eigener Buchstabe „X1“ an der letzten → „X1“; `visible_revisions` = die letzten 6, neueste zuletzt); (g) `notes_rules` (Projekt-Toleranz `Text("ISO 2768-f")` → erste Zeile „Allgemeintoleranzen ISO 2768-f“; Projekt `Lines(["P"])` → nur „P“; Blatt `Lines(["A"])` ersetzt alle; Blatt `Lines([])` → keine Zeile, kein Hinweisfeld); (h) `main_scale_rule` (zwei Ansichten 1:2 mit 10 Linien und 1:5 mit 30 → 1:5; Gleichstand → erste; keine berechnet → erste Ansicht; leer → None); (i) `validation_limits` (Titel 61 Zeichen, Feld 81, Hinweis 101, Beschreibung 121, 7 Hinweiszeilen, 24 Änderungen, Steuerzeichen „\n“ → jeweils `Err` mit deutschem Text; Grenzwerte genau ok; Zählung in Zeichen, nicht Bytes: 60 × „Ä“ ok). `crates/planora-feature/tests/drawings.rs`: (j) `update_drawing_rejects_invalid_sheet_info` (Titel 61 Zeichen über `update_drawing` → Fehler, Dokument unverändert).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage21_sheet_info -j 2`; `cargo test -p planora-feature --test drawings -j 2`
- [ ] **Step 3: Implement** nach Interfaces; `utc_date` wie `planora-project::fsutil::iso_from_unix` (nur das Datum; keine gemeinsame Funktion über Paketgrenzen nötig).
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-drawing -p planora-feature -p planora-project -j 2` (Literale mit `info: None` übersetzen), clippy, fmt.
- [ ] **Step 5: Commit** — `feat: Blattangaben mit Vorbelegung, Änderungsbuchstaben und Auflösung zu Texten`.

### Task 3: Speichern im Dokument und Dateiformat 6

**Files:**
- Modify: `crates/planora-feature/src/model.rs` (`project_info: Option<ProjectInfo>`), `drawings.rs` (neue Methoden), `snapshot.rs` (`Snapshot.project_info`), `lib.rs`
- Modify: `crates/planora-project/src/format.rs` (DTOs), `convert.rs` (`FORMAT_VERSION = 6`, `required_format`, encode/decode)
- Test: `crates/planora-feature/tests/sheet_info.rs` (neu), `crates/planora-project/tests/sheet_format.rs` (neu), bestehende `drawings_format.rs`, `dimensions_format.rs` (nur `FORMAT_VERSION`-Erwartungen)

**Interfaces:**
- Consumes: `sheet_info::{ProjectInfo, SheetInfo, Stamp, validate_*}` (2).
- Produces:
```rust
impl Document {
    pub fn project_info(&self) -> Option<&ProjectInfo>;
    pub fn set_project_info(&mut self, p: ProjectInfo) -> Result<(), FeatureError>;   // prüft; zählt drawing_edits nur bei Änderung
    pub fn set_sheet_info(&mut self, id: DrawingId, i: SheetInfo) -> Result<(), FeatureError>; // über update_drawing
    pub fn add_drawing_stamped(&mut self, name: &str, sheet: Sheet, projection: Projection, stamp: &Stamp)
        -> Result<DrawingId, FeatureError>; // F3: info = SheetInfo::new(stamp); project_info.get_or_insert(ProjectInfo::new(stamp)); genau +1 Änderung
}
pub struct Snapshot { /* … */ pub project_info: Option<ProjectInfo> }
// format.rs (ohne deny_unknown_fields, alles `#[serde(default, skip_serializing_if = …)]`):
VersionFile.projektangaben: Option<ProjektangabenDto>
ProjektangabenDto { anmeldename: String, eigentuemer, ersteller, geprueft, freigabe, werkstoff, allgemeintoleranz: Option<String>,
                    hinweise: Option<Vec<String>> /* höchstens 6 beim Lesen */ }
ZeichnungDto.blattangaben: Option<BlattangabenDto>
BlattangabenDto { angelegt: String, nummer, titel, zusatztitel, dokumentart, dokumentstatus, ausgabedatum, aenderungsstand, blatt, format,
                  massstab, gewicht: Option<String>, hinweise: Option<Vec<String>>, aenderungen: Vec<AenderungDto> /* ≤ 23 */ }
AenderungDto { buchstabe: Option<String>, beschreibung: String, datum: String, name: String }
```
`Option<String>`: `None` = automatisch, `Some` = eigener Text; `hinweise` `None` = `NotesValue::Auto`. `required_format`: 6, wenn `project_info.is_some()` oder eine Zeichnung `info.is_some()`; sonst wie bisher. Decode: Format < 6 mit `projektangaben` oder `blattangaben` → `corrupt(origin, "Blattangaben gibt es erst ab Dateiformat 6")`; Längen/Anzahlen über `validate_*` → `corrupt` mit „Zeichnung „Name“: …“ bzw. „Projektangaben: …“. `from_snapshot` prüft `project_info` mit `validate_project_info`. `delete_drawing` lässt `project_info` stehen.

- [ ] **Step 1: Failing tests:** `planora-feature/tests/sheet_info.rs`: (a) `stamped_drawing_has_info_and_project` (Stamp {„2026-10-17“, „dennis“}: Zeichnung hat `info.created` „2026-10-17“, Projekt `login` „dennis“; zweite Zeichnung mit anderem Stamp ändert `login` nicht; `edit_count` je Anlage +1; `add_drawing` (ohne Stamp) legt weder Satz noch Projekt an); (b) `set_infos_validate_and_count` (gültige Änderung +1, gleiche Werte +0, ungültige → Fehler und unverändert; `model_edit_count` unverändert); (c) `snapshot_round_trip` (Snapshot → `from_snapshot` gleich, `edit_count` 0). `planora-project/tests/sheet_format.rs`: (d) `format_6_only_with_sheet_data` (`required_format`: Zeichnung über `add_drawing` → 4 bzw. 5 wie bisher; über `add_drawing_stamped` → 6; nur `project_info` → 6; `FORMAT_VERSION == 6`); (e) `old_files_stay_byte_identical` (je eine Format-4- und Format-5-Datei aus `encode_version` eines Dokuments ohne Satz: `encode(decode(text)) == text`, kein `projektangaben`/`blattangaben` im Text); (f) `full_round_trip` (alle Felder eigener Text, auch Dokumentstatus, Sonderzeichen „ä ö ü ß Ø ° ± –“, 23 Änderungen mit eigenem Buchstaben an einer, eigene Hinweise, Projekt-Hinweise; dazu leere/automatische Variante → `decode(encode(x))` gleich); (g) `rejects_bad_sheet_data` (Format 5 mit `blattangaben` → Meldung „Blattangaben gibt es erst ab Dateiformat 6“; 24 Änderungen, 7 Hinweise, Titel 61 Zeichen → benannte `corrupt`-Meldung mit dem Zeichnungsnamen; Format 7 → `TooNew { found: 7 }`); (h) `unknown_fields_are_ignored` (Format-6-Datei mit zusätzlichem Feld in `blattangaben` lädt).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test sheet_info -j 2`; `cargo test -p planora-project --test sheet_format -j 2`
- [ ] **Step 3: Implement** nach Interfaces (Muster: `de_bounded`, `skip_serializing_if` wie bei Format 5, P11 im TP12-Entwurf).
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-feature -p planora-project -j 2` (bestehende Format-Tests: nur Stellen anpassen, die `FORMAT_VERSION` als 5 erwarten), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** `required_format` ohne die 6-Regel → (d) rot; `skip_serializing_if` an `blattangaben` entfernt → (e) rot.
- [ ] **Step 6: Commit** — `feat: Blattangaben im Dokument, Dateiformat 6`.

### Task 4: Blattrahmen als Markierungen und Ausgabe in SVG/DXF

**Files:**
- Create: `crates/planora-drawing/src/sheet_marks.rs`
- Modify: `crates/planora-drawing/src/annotate.rs` (`MarkKind::Sheet(SheetPen)`), `sheet.rs` (`FRAME_MARGIN_MM` entfällt, `SheetModel.frame: Option<SheetFrame>`, `build_sheet*` setzen `None`), `svg.rs`, `dxf.rs` (`dxf_char_ok`), `lib.rs`; `crates/planora-app/src/drawing_view.rs` (Rechteck des Innenrahmens nicht mehr malen; `MarkKind`-Zuordnung), `crates/planora-app/src/dimension_state.rs` (falls `MarkKind` erschöpfend gematcht)
- Modify (Goldene Dateien): `crates/planora-drawing/tests/fixtures/svg_without_marks_{box,cylinder}.svg`, `dxf_without_marks_{box,cylinder}.dxf`
- Test: `crates/planora-drawing/tests/stage22_sheet_output.rs` (neu); anpassen: `stage9_sheet.rs:882`, `stage10_svg.rs` (`:82`, `:411–418`, `:955`, `:1029`), `stage11_dxf.rs` (`:93`, `:797–802`, `:1417`, `:1604–1607`, `:2089`), `stage16_svg_marks.rs`, `stage17_dxf_marks.rs`, `stage17_file.rs:58`, `common/svg_marks.rs:107`, `hand_samples.rs:313`, `hand_samples_dims.rs:865`

**Interfaces:**
- Consumes: `sheet_frame::{SheetLayout, SheetPen, TITLE_CELLS, TITLE_SLOTS, projection_symbol, …}` (1), `sheet_info::TitleTexts` (2), `annotate::{Mark, TextAnchor}`.
- Produces:
```rust
#[derive(Clone, Debug, PartialEq)] pub struct SheetFrame { pub marks: Vec<Mark> }
pub struct Fitted { pub text: String, pub height: f64, pub cut: bool }
pub fn sheet_text_width(text: &str, height: f64) -> f64;            // F11
pub fn fit_text(text: &str, height: f64, width: f64) -> Fitted;     // ≤ width; Höhe ≥ 0,6·height; sonst Kürzen mit „…“
pub fn field_is_cut(field: TitleField, text: &str) -> bool;         // für die Warnung im Fenster
pub fn sheet_frame(layout: &SheetLayout, texts: &TitleTexts, projection: Projection) -> SheetFrame;
pub enum MarkKind { Dimension, CenterLine, Lost, Sheet(SheetPen) }
pub fn dxf_char_ok(c: char) -> bool;                                // Grundebene und kein Steuerzeichen
// sheet.rs
pub struct SheetModel { pub sheet: Sheet, pub frame: Option<SheetFrame>, pub views: Vec<SheetView> }
```
`sheet_frame` erzeugt: Rahmen (4 Linien `Heavy`), Zentriermarken (`Medium`), Teilstriche (`Thin`), Rasterbeschriftung (Text `GRID_TEXT_H`, `Middle`, Grundlinie = Mitte − h/2), Schriftfeld-Außenlinie (`Heavy`) und Zelltrennlinien (`Thin`, gemeinsame Kanten einmal), Feldnamen (`LABEL_H`), Inhalte über `fit_text` in ihrem `TextSlot`, Projektionssymbol (`Medium`, Kreise als `Mark::Arc` 0…2π), Änderungstabelle (Linien `Thin`, Kopf „Rev.“, „Änderung“, „Datum“, „Name“, Zeilen = `visible_revisions`, neueste unten, Texte eingepasst in Spaltenbreite − 2), Hinweisfeld (Umriss `Thin`, Zeilen nach F9 eingepasst in Breite − 4). Texte: `MarkKind::Sheet(SheetPen::Thin)`, Winkel 0. SVG/DXF nach F13; ohne Rahmen keine `<rect>`, kein `LWPOLYLINE`, keine neuen Ebenen. Farbe/Breite der Markierungen in SVG: `#000`, `SheetPen::width_mm`.

- [ ] **Step 1: Goldene Dateien vorher sichern und Erwartung festhalten:** Die vier Golden-Dateien ändern sich nur so: SVG Zeile 3 (`<rect x="10" … stroke-width="0.5"/>`) entfällt; DXF das `LWPOLYLINE`-Objekt des Innenrahmens entfällt, alle späteren Handles sind um 1 kleiner und `$HANDSEED` um 1 kleiner. Ein Hilfstest im Testcode (`goldens_differ_only_by_the_old_frame`) wendet genau diese Umformung auf die alte Datei (aus `git show 42a6d68:…`, als Kopie `fixtures/old_*` nur für diesen Schritt, danach gelöscht) an und vergleicht mit der neuen Ausgabe; erst dann die Golden-Dateien überschreiben.
- [ ] **Step 2: Failing tests (`stage22_sheet_output.rs`):** (a) `fit_rules` („Muster GmbH“ 3,5 in 98 mm unverändert, `cut` false; 80 × „W“ → Höhe 2,1, endet auf „…“, `sheet_text_width ≤ 98`, `cut` true; Text knapp zu breit → nur verkleinert, Höhe ≥ 2,1, nicht gekürzt; leer → leer); (b) `longest_texts_stay_in_their_cells` (alle Felder mit Höchstlänge aus „W“, „ÄÖÜß“, „Ø°±“, 23 Änderungen mit 120 „W“, 6 Hinweise mit 100 „W“; für A4 hoch, A4 quer, A3 quer, A0 hoch: jede Text-Markierung mit `sheet_text_width` liegt ganz in ihrer Zelle/Spalte/Hinweisfeld und unter dem Feldnamen); (c) `frame_marks_contents` (A3 quer, Methode 1: genau 4 `Heavy`-Linien des Rahmens, 4 `Medium`-Zentriermarken, Texte „Halter“ (Höhe 5), „Blatt“ (2,2), „2 von 3“, „Fertigungszeichnung“, „Dokumentstatus“ (2,2), „in Arbeit“, „1“ … „8“ und „A“ … „F“ je zweimal, Hinweise in Reihenfolge; Methode 3 spiegelt die Lage nach F7); (d) `no_frame_is_content_only` (`build_sheet` → `frame` None; SVG ohne `<rect` und ohne „Blattrahmen“; DXF ohne `LWPOLYLINE` und ohne Ebenen `BLATT*`; goldene Dateien gleich); (e) `framed_output_is_on_the_paper` (mit Rahmen: SVG hat `<g id="Blattrahmen">` vor der ersten Ansichtsgruppe, `width="420mm"`, alle Koordinaten in `[0, 420] × [0, 297]`; Strichbreiten 0,7/0,5/0,25 vorhanden; DXF: Ebenen `BLATTRAHMEN` 70, `BLATT_MITTE` 50, `BLATT` 25, Rahmen-Objekte vor den `INSERT`s, Leser `common/dxf_read.rs` liest alles; Texte mit „Ø“ → `%%c`, „ä“ → `\U+00E4`); (f) `dxf_chars` (`dxf_char_ok('ä')`, `('Ø')` true; `('😀')`, `('\u{7}')` false); (g) `nan_in_frame_is_rejected` (eine Rahmen-Markierung mit NaN → SVG und DXF `NonFinite`, nichts geschrieben; ersetzt die alten NaN-Rahmentests in `stage10_svg.rs:1029` und `stage11_dxf.rs:1604`). Bestehende Tests nach Liste oben umschreiben (Innenrahmen weg: „kein `<rect`“, erstes Objekt `INSERT`, `m.frame.is_none()`, `hand_samples*` nehmen `EDGE_MARGIN_MM`).
- [ ] **Step 3: Run → FAIL.** `cargo test -p planora-drawing --test stage22_sheet_output -j 2`
- [ ] **Step 4: Implement** nach Interfaces und F11, F13; im App-Paket nur, was zum Übersetzen nötig ist (Rechteck weg, `MarkKind::Sheet` wie `Dimension` behandeln).
- [ ] **Step 5: Run → PASS**, `cargo test -p planora-drawing -j 2` (alle Stufen, auch `stage15` mit seiner Fixtur unverändert), `cargo test -p planora-app --test export_drawing --test export_dimensions --test drawing_ui -j 2`, clippy, fmt.
- [ ] **Step 6: Mutationsbeweise:** `fit_text` ohne Kürzen → (b) rot; Rahmen auch bei `None` schreiben → (d) rot.
- [ ] **Step 7: Commit** — `feat: Blattrahmen, Schriftfeld und Tabellen als Markierungen; SVG/DXF ohne alten Innenrahmen`.

### Task 5: Anzeige in der Zeichnungsansicht und Anordnung im freien Bereich

**Files:**
- Modify: `crates/planora-app/src/state.rs` (`AppState.document_name: String`, Vorgabe „Unbenannt“), `app.rs` (setzt `tab.state.document_name` je Bild aus `tab.name`), `drawing_view.rs` (Rahmen bauen, zwischenspeichern, malen; `add_view` im freien Bereich), `drawing_state.rs` (`FRAME_MARGIN_MM` entfällt; `suggest_scale` im freien Bereich), `drawing_dialog.rs` (`create` mit `add_drawing_stamped`, Grundansicht in der Mitte des freien Bereichs)
- Test: `crates/planora-app/tests/sheet_view.rs` (neu); anpassen: `drawing_state.rs` (`suggest_scale_fits_the_three_views`, Kommentare „Innenrahmen“), `drawing_ui.rs:489`, Tests mit festen Lagen nach „Anlegen“ (Blattmitte → Mitte des freien Bereichs)

**Interfaces:**
- Consumes: `sheet_layout`, `SheetLayout.free` (1), `resolve_texts`, `main_scale`, `SheetContext`, `Stamp` (2), `Document::{project_info, add_drawing_stamped}` (3), `sheet_frame`, `SheetPen::width_mm`, `MarkKind::Sheet` (4).
- Produces (in `drawing_view.rs`):
```rust
pub fn sheet_context(state: &AppState, id: DrawingId) -> Option<SheetContext>;  // Platz in document.drawings() + 1, Anzahl, document_name
pub fn sheet_texts(state: &AppState, drawing: &Drawing, model: &SheetModel) -> TitleTexts;
pub fn layout_of(state: &AppState, drawing: &Drawing) -> SheetLayout;            // Zeilenzahlen aus resolve_texts(.., None)
```
in `drawing_state.rs`: `pub fn suggest_scale_in(bodies: &[Body], free: (f64, f64), base: ViewKind) -> Scale` (Regel wie bisher, `inner_w/h` = Größe des freien Bereichs); `suggest_scale(bodies, sheet, projection, base)` bleibt und nimmt `sheet_layout(sheet, 0, 3).free` (Blatt einer neuen Zeichnung). `add_view`: Mitte/„links oben“/Isometrie = `(0,5; 0,5)`, `(0,25; 0,75)`, `(0,75; 0,75)` des freien Bereichs der geöffneten Zeichnung, Maßstab über `suggest_scale_in`. `SheetCache` vergleicht zusätzlich `project_info`, `document_name`, Platz und Anzahl. Malen: Rahmen-Markierungen über `paint_mark` mit `theme::SHEET_INK` und Breite `pen.width_mm() · k` (mindestens 1 px), vor den Ansichtslinien; das Papier bleibt `theme::PAPER`.

- [ ] **Step 1: Failing tests (`sheet_view.rs`, `AppState` + Zeichnungsbereich im Harness, Hell und Dunkel):** (a) `frame_is_shown_at_800x600_and_large` (800 × 600 und 1920 × 1080, A4 quer und A0 quer: gemalte Texte enthalten „Fertigungszeichnung“, „1 von 1“, „A4“ bzw. „A0“, „1“ und „A“; Papierfläche `theme::PAPER`, Rahmenlinien `theme::SHEET_INK` in beiden Themes; Zoom 0,1 und 40 ohne Panik); (b) `new_drawing_is_stamped_and_centred` („Neue Zeichnung“ anlegen: `info` gesetzt mit heutigem `utc_date`, Projekt mit Anmeldename; Grundansicht bei A4 quer `Free { x: 153.5, y: 140.5 }`; Datei würde Format 6); (c) `auto_values_follow_the_document` (zwei Zeichnungen, `document_name` „Werkstück“: Blatt „1 von 2“/„2 von 2“, Nummern „Werkstück-01“/„-02“; erste löschen → übrige „1 von 1“, „Werkstück-01“; `document_name` „Neu“ → „Neu-01“); (d) `views_are_added_in_the_free_area` (A4 hoch, A4 quer, A3 quer: neue freie Ansichten liegen mit ihrer geschätzten Ausdehnung im freien Bereich; `suggest_scale_in` für A4 quer und Quader 100 × 60 × 40 = 1:2 (freier Bereich 267 × 105); bei 6 Änderungen und 6 Hinweiszeilen auf A4 hoch bleibt der Vorschlag in `Scale::SERIES`); (e) `old_drawing_shows_defaults_without_changes` (Zeichnung über `add_drawing`: Blatt zeigt „–“ beim Ausgabedatum und bei Erstellt; nach 10 Bildern `edit_count` unverändert, `info` weiter `None`). Bestehende Erwartungen anpassen: `drawing_state.rs` A4-quer-Fall 1:1 → 1:2 (Rechnung im Kommentar mit 267 × 105), übrige Fälle mit den neuen freien Bereichen nachrechnen; `drawing_ui.rs:489` ebenso.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test sheet_view -j 2`
- [ ] **Step 3: Implement** nach Interfaces.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test drawing_state --test drawing_ui --test drawing_fields --test dimension_view --test shell_app --test no_hard_coded_colors -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Grundansicht wieder in Blattmitte → (b) rot; `SheetCache` ohne `document_name` → (c) rot.
- [ ] **Step 6: Commit** — `feat: Zeichnungsblatt in der Ansicht, neue Ansichten im freien Bereich`.

### Task 6: Fenster „Blattangaben“, Knopf und Menü, Export-Schalter

**Files:**
- Create: `crates/planora-app/src/sheet_dialog.rs`
- Modify: `crates/planora-app/src/lib.rs`, `state.rs` (`sheet_dialog: Option<SheetInfoDialog>`), `app.rs` (zeigen wie `dimension_dialog`, schließen im Skizzen-Modus), `drawing_view.rs` (`Action::SheetInfo`, Knopf „Blattangaben“ nach „Blatt einpassen“), `drawing_dialog.rs` (Menüeintrag „Blattangaben …“), `icons.rs` (`Icon::SheetInfo`, vorhandenes Zeichen der Symbolschrift), `export_dialog.rs` (Schalter, Schlüssel, Rahmen im Arbeitsthread, DXF-Prüfung)
- Test: `crates/planora-app/tests/sheet_dialog.rs` (neu), `tests/export_drawing.rs` (Schalter), `tests/dialogs.rs` und `tests/wide_dialogs.rs` (neuer Dialog in den Listen der Größenklassen/Texte), `tests/icons.rs`, `tests/drawing_fields.rs` (Werkzeugfeld eine Schaltfläche breiter)

**Interfaces:**
- Consumes: alles aus 2–5; `dialog::{Dialog, DialogSize::Wide}`, `theme::{text_field, section_frame, Toggle}`, `field_is_cut` (4), `dxf_char_ok`, `dxf_text_fits`.
- Produces:
```rust
// sheet_dialog.rs
pub const TITLE: &str = "Blattangaben";
pub const OWNER_MISSING: &str = "Bitte den Eigentümer eintragen. Er gehört auf jede Zeichnung.";
pub const CUT_WARNING: &str = "Passt nicht ganz ins Feld und wird auf dem Blatt gekürzt.";
pub struct SheetInfoDialog { pub drawing: DrawingId, pub project: ProjectInfo, pub sheet: SheetInfo, pub message: Option<String>, pub last_rect: Option<egui::Rect> }
pub fn open(state: &mut AppState, id: DrawingId);                 // Kopien; fehlende Daten vorbelegt nach F4 (Stamp::now)
impl SheetInfoDialog {
    pub fn new(state: &AppState, id: DrawingId, stamp: &Stamp) -> Self;
    pub fn apply(&self, state: &mut AppState) -> Result<(), String>;  // set_project_info + set_sheet_info; keine Änderung bei gleichen Werten
    pub fn add_revision(&mut self, stamp: &Stamp, creator: &str);     // ab 23: message = TOO_MANY_REVISIONS
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool;
}
// export_dialog.rs
pub struct FrameInput { pub project: Option<ProjectInfo>, pub ctx: SheetContext }
pub fn prepare_drawing_framed(drawing: &Drawing, solids: &[(String, Solid)], compute: ComputeFn, resolve: ResolveFn,
                              frame: Option<&FrameInput>, progress: &Progress) -> Result<DrawingPrepared, String>; // prepare_drawing_with = framed mit None
pub fn dxf_sheet_texts_ok(texts: &TitleTexts) -> Result<(), String>;
pub const FRAME_SWITCH: &str = "Blattrahmen mit ausgeben";
ExportDialog { pub with_frame: bool /* Vorgabe true */ , … }   // DrawKey (bleibt Copy) += with_frame, Hash von document_name (u64)
```
Fenster: drei Abschnitte in `theme::section_frame` mit Überschrift „Projekt“ (Eigentümer, Erstellt, Geprüft, Freigabe, Werkstoff, Allgemeintoleranz, Hinweise für alle Blätter), „Dieses Blatt“ (Zeichnungsnummer, Titel, Zusatztitel, Dokumentart, Dokumentstatus, Ausgabedatum, Änderungsstand, Blatt, Format, Maßstab, Gewicht; Schalter „Eigene Hinweise für dieses Blatt“ kopiert die Projekt-Hinweise), „Änderungen“ (Zeilen Buchstabe/Beschreibung/Datum/Name, Löschen, „Änderung hinzufügen“). Je Feld: `theme::text_field` mit `hint_text` = automatischer Wert und `char_limit` = Grenze, Knopf „Zurücksetzen“ (nur bei eigenem Text), Länge „n/max“, bei `field_is_cut` die Warnung `CUT_WARNING`; leerer Eigentümer → `OWNER_MISSING`. Hinweiszeilen: „Zeile hinzufügen“ bis 6, je Zeile Löschen. Knöpfe „Übernehmen“, „Abbrechen“. Export: Schalter `theme::Toggle` mit `FRAME_SWITCH` nur bei den Zeichnungsformaten in der Zeichnungskarte; mit Schalter baut der Arbeitsthread `resolve_texts(.., main_scale(..))`, `sheet_layout`, `sheet_frame` und setzt `model.frame`; bei DXF vorher `dxf_sheet_texts_ok`: Zeichen mit `!dxf_char_ok` → „Das Zeichen „{c}“ im Feld „{Feldname}“ lässt sich als DXF nicht ausgeben. Bitte ändern.“; Text, der `dxf_text_fits` nicht erfüllt → „Der Text im Feld „{Feldname}“ ist für DXF zu lang. Bitte kürzen.“ (Änderungen: „Änderung {Buchstabe}“, Hinweise: „Hinweis {n}“).

- [ ] **Step 1: Failing tests (`sheet_dialog.rs`, Harness mit `PlanoraApp::for_tests` bei 800 × 600 und 1400 × 900, Hell und Dunkel):** (a) `opens_from_toolbar_and_menu` (Knopf „Blattangaben“ im Werkzeugfeld rechts neben „Blatt einpassen“ und Menü „Zeichnung → Blattangaben …“ öffnen denselben Dialog: Breite 720 ± 1, mittig, Abschnitte „Projekt“, „Dieses Blatt“, „Änderungen“ sichtbar bzw. erreichbar durch Rollen; in alter Version und im Skizzen-Modus gesperrt mit Hinweis); (b) `edit_apply_and_reset` (Titel „Lager“ und Dokumentstatus „freigegeben“ eintippen, Übernehmen → Blatt zeigt „Lager“ und „freigegeben“, `edit_count` +1; wieder öffnen, „Zurücksetzen“, Übernehmen → Name der Zeichnung; Länge „5/60“ sichtbar; Hinweistext im leeren Feld = automatischer Wert); (c) `cancel_changes_nothing` (alte Zeichnung ohne Satz: öffnen, tippen, Abbrechen bzw. Schließen-Symbol → `info` None, `project_info` None, `edit_count` gleich; Übernehmen ohne Eingabe → beide gesetzt, Ausgabedatum heute (F4)); (d) `revisions_in_the_dialog` (3 Änderungen hinzufügen → Buchstaben-Hinweise A, B, C, Datum heute, Name = Ersteller; nach 23 zeigt der Klick „Es sind höchstens 23 Änderungen möglich.“ und es bleiben 23; Löschen der mittleren → Buchstaben A, B; Änderungsstand auf dem Blatt = B); (e) `warnings_in_place` (Eigentümer leer → `OWNER_MISSING`; Eigentümer 80 × „W“ → `CUT_WARNING` an diesem Feld, nicht an anderen); (f) `typing_does_not_trigger_shortcuts` (Ansicht gewählt, im Titelfeld „Entf“, „d“, „+“, Esc: Ansicht bleibt, Zoom unverändert, Text im Feld; `viewport::typing` wahr); (g) `dialog_texts_are_plain` (alle sichtbaren Texte ohne „Spec“, „ISO“, „DXF“, „egui“, „Format 6“, „Datei stammt“, „TODO“); Eintrag in `tests/dialogs.rs`/`wide_dialogs.rs` (Klasse `Wide`). `export_drawing.rs`: (h) `frame_switch_controls_the_output` (Schalter sichtbar nur bei Zeichnungsformaten, Vorgabe an; an: SVG enthält `id="Blattrahmen"` und „1 von 1“, DXF Ebene `BLATTRAHMEN`; aus: weder noch; Umschalten startet eine neue Berechnung (Schlüssel); Titel mit „😀“ → DXF-Status zeigt die Zeichen-Meldung mit „Titel“, SVG wird geschrieben; Titel mit 60 × „Ä“ und Beschreibung 120 × „Ä“ → Längen-Meldung bzw. Erfolg nach `dxf_text_fits`); (i) `export_uses_current_auto_values` (`document_name` ändern bei offenem Dialog → neue Berechnung, Datei hat die neue Nummer).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test sheet_dialog --test export_drawing -j 2`
- [ ] **Step 3: Implement** nach Interfaces und F1, F4, F14.
- [ ] **Step 4: Run → PASS**, dazu `--test dialogs --test wide_dialogs --test no_raw_windows --test no_raw_text_edits --test no_hard_coded_colors --test icons --test fonts --test drawing_fields --test export_dialog`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** `apply` beim Öffnen statt beim Übernehmen → (c) rot; Schalter ignoriert → (h) rot; `DrawKey` ohne `with_frame` → (h) rot.
- [ ] **Step 6: Commit** — `feat: Fenster Blattangaben, Knopf und Menü, Schalter Blattrahmen beim Export`.

### Task 7: Dokumentation, Gesamtprüfung, Handliste

**Files:**
- Modify: `README.md` (Abschnitt Zeichnungen: Blatt, Blattangaben, Schalter), `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt 3c-1 – Zeichnungsblatt“: Geliefertes, F1–F15, Normprüfung aus Aufgabe 1, Grenzen: UTC-Datum, geschätzte Textbreite, Fusion zeigt DXF-Texte nicht (bekannt aus TP12), kein Platz für die Stückliste freigehalten, keine Blattreihenfolge), `docs/superpowers/specs/2026-10-17-zeichnungsblatt-design.md` (neuer §10 „Festlegungen der Umsetzung“ mit F1–F15 und Abweichungen), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)

- [ ] **Step 1: Proben erzeugen** (Testhilfe wie `hand_samples.rs`, `#[ignore]`): SVG und DXF mit und ohne Rahmen für A4 hoch, A4 quer, A3 quer, A2 hoch, A0 quer, Methode 1 und 3, mit Längstexten und Sonderzeichen, nach `D:\Testumgebung\zeichnung-proben\blatt\`.
- [ ] **Step 2: Doku** in Laiensprache; Handliste für den Nutzer: jede Papiergröße hoch/quer in der App ansehen (800 × 600 und groß, Hell und Dunkel: weißes Papier); Fenster „Blattangaben“ ausfüllen, zurücksetzen, abbrechen; 23 Änderungen; Hinweise ändern; neue Ansicht landet nicht im Schriftfeld; Blatt n/N bei drei Zeichnungen; alte Datei öffnen und speichern (bleibt alt); SVG in Inkscape und im Browser öffnen und auf A3/A4 drucken (Rahmen ganz auf dem Blatt, nichts überlappt); DXF in Fusion und, falls vorhanden, LibreCAD/QCAD (Texte); Projektionssymbol Methode 1 und 3 ansehen.
- [ ] **Step 3: Gates** mit Ausgabe: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2`; `cargo deny check`; `git diff --stat 42a6d68 -- crates/planora-kernel crates/planora-mesh/src crates/planora-step crates/planora-exchange` leer; `git diff 42a6d68 -- '**/Cargo.toml' Cargo.lock` ohne neue Pakete; `git status` sauber.
- [ ] **Step 4: Commit** — `docs: Zeichnungsblatt (3c-1), Festlegungen, Handliste`.
- [ ] **Step 5 (Controller):** Haltepunkt: Nutzer prüft die Handliste; Gesamtreview des Zweigs, eine Korrekturrunde, Merge-Frage.

---

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung:** §1 Normprüfung → Aufgabe 1 Schritt 1; §2 Ziele (Rahmen, Marken, Raster, Schriftfeld, Tabelle, Hinweise; Fenster; mehrere Blätter n/N; Schalter) → 1, 4, 5, 6; Nicht-Ziele unberührt (kein Gewicht, keine Vorlagen, A4–A0); Anti-Ziele → Review Focus 1, 2, 4, 5 und Aufgabe 4 (b), (e); §3 alle Maße → Aufgabe 1 (a)–(g), Konstanten; §3 freier Bereich und `layout`-Regel → Aufgabe 1 (e)/(f), Aufgabe 5 (d); §4 Felder (mit Dokumentstatus, F15), Vorbelegung, Zurücksetzen, Hinweise, Änderungen (ohne I/O/Q, 23, Meldung), Grenzen und Einpassen → Aufgaben 2, 4, 6; §5 Bedienung (Knopf neben „Blatt einpassen“, Menü, 720 px, drei Abschnitte, Hinweis/Zurücksetzen/Länge, immer sichtbar, `text_field`, Neues Blatt vorbelegt, weißes Papier) → 5, 6; §6 Ausgabe (Schalter Vorgabe an, ohne Rahmen nur Inhalt, Linienstärken, sans-serif, DXF-Prüfung, Symbol als Linien, Unicode) → 4, 6; §7 Datenmodell, „immer speichern, was fest sein muss“, Format 6/5, alte Dateien, Reihenfolge = Anlegereihenfolge → 2, 3, 5 (b), (e), 6 (c); §8 Prüfungen → verteilt wie angegeben, Handliste → 7.
**Schritt-Scan:** Jeder Testschritt nennt Namen, Eingaben und Zahlen; Rümpfe fehlen absichtlich; Algorithmen ohne eindeutige Signatur (Rasterzahl, freier Bereich, Einpassen, Textbreite, Symbol) stehen als Festlegung F5–F11.
**Typkonsistenz:** `SheetPen`, `TitleField`, `SheetLayout`, `sheet_layout`, `TITLE_SLOTS` (1) → 2, 4, 5; `FieldValue`, `NotesValue`, `ProjectInfo`, `SheetInfo`, `Stamp`, `SheetContext`, `TitleTexts`, `resolve_texts`, `main_scale` (2) → 3, 5, 6; `add_drawing_stamped`, `set_project_info`, `set_sheet_info` (3) → 5, 6; `SheetFrame`, `sheet_frame`, `fit_text`, `field_is_cut`, `dxf_char_ok`, `MarkKind::Sheet` (4) → 5, 6.
**Proportion:** Entscheidungen, Zahlen und Tests; keine Funktionsrümpfe.
**Entscheidungen des Nutzers (2026-10-17):** Hinweisfeld nach 310-mm-Regel (F9), Projektionssymbol nach F7, Dokumentstatus aufgenommen (F15); keine offenen Fragen.
**Bekannte Risiken:** Textbreite ist geschätzt (echte Schriften in Inkscape/Montserrat können breiter laufen; F11 konservativ, Handliste); viele Tests mit festen Lagen nach „Anlegen“ (Aufgabe 5); DXF-Handles verschieben sich in den Golden-Dateien (Aufgabe 4 Schritt 1 prüft das mechanisch). Empfehlung Controller: opus für 1, 4, 6; sonnet für 2, 3, 5, 7.

## Abweichungen bei der Umsetzung

Die vollständige Liste steht in der Spec §10 und in `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt 3c-1 – Zeichnungsblatt“). Kurzfassung:

- Aufgabe 1: Die rechte Zentriermarke war nach der Aufgabenvorgabe 25 mm lang (A4 quer 297 → 272); richtig nach Spec ist 297 → 282. Berichtigt. Die Normzahlen für Zentriermarken (0,7 mm, 10 mm) bleiben ungeprüft, die Spec-Zahlen gelten.
- Aufgabe 4: Das Projektionssymbol liegt im DXF auf der Ebene `BLATT_MITTE` (Ebene nach Strichstärke), nicht auf `BLATT`. Ein DXF-Blatttext über 255 Zeichen scheitert mit Meldung (kein stilles Kürzen).
- Aufgabe 5: Blatttexte unter einem Bildpunkt werden auf einen Bildpunkt angehoben. Die isometrische Ansicht ist bei fast blattfüllenden Teilen nur bestmöglich im freien Bereich. Alte Zeichnungen werden nicht verschoben.
- Aufgabe 6: Der Knopf „Blattangaben“ steht am Ende der ersten Zeile des Werkzeugfelds (Spec §5 sagt „neben Blatt einpassen“). Ein leeres Feld bedeutet „automatisch“. Die Kürzungswarnung gilt je Zeile der Änderungstabelle und je Hinweiszeile. „ISO 2768-m“ als Vorgabetext ist Blattinhalt.
- Aufgabe 3: Ein Dokument mit nur Projektangaben bleibt nach dem Löschen der letzten Zeichnung im Format 6; ein Format-6-Dokument ohne beide Datensätze lädt nachsichtig.
- Aufgabe 7: Die Proben erzeugt der ignorierte Test `hand_samples_sheet.rs` (80 Dateien).
- Abschlussrunde (Entscheidung des Nutzers): Das Blattzubehör wächst mit dem Format. Alle Zahlen der Spec §3/§4 und F5–F10 sind A3-Werte; auf anderen Formaten gelten sie mal `lange Blattseite / 420` (`SheetLayout::scale`), Linien mal Faktor, nie unter 0,18 mm. Dadurch ändern sich F5 (überall 8 × 6 Felder statt A4 6 × 4 bis A0 24 × 16) und F9 (Schwelle 310 · Faktor, A4 quer bekommt das Hinweisfeld daneben); das Schriftfeld ist nur auf A3 180 mm breit (A4 127 mm, Schrift 2,47 mm).
- Abschlussrunde: Die Isometrie kommt ins leere Viertel des Dreiansichtenblocks und bekommt dort, wenn nötig, die nächstkleinere Stufe der Maßstabsreihe (höchstens eine Stufe kleiner als die Grundansicht; vorher: frei rechts oben im Maßstab der Grundansicht, konnte den Block überdecken).
- Abschlussrunde (Handtest): Das Maßstabsfeld zeigt den Maßstab der ersten Ansicht der Zeichnung (Grundansicht), nicht mehr den der Ansicht mit den meisten Linien (das war oft die Isometrie). `main_scale(d)` braucht kein Blattmodell mehr.
- Abschlussrunde: DXF-Rahmentexte werden mit dem Zuschlag 1,45 auf die geschätzte Breite eingepasst (`sheet_frame_dxf`); SVG und Anzeige bleiben bei F11. Die Schriftfeldzelle des Änderungsstands heißt „Index“. Die Länge „n/max“ steht an allen Feldern (Tabellenzellen im Hinweis). Das zuletzt gezeichnete Blatt gilt nur im eigenen Tab (`AppState::instance`). Die DXF-Meldung nennt ein nicht darstellbares Zeichen als „U+…“.
