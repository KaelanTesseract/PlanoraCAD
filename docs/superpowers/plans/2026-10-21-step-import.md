# STEP-Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **COMMIT-TRAILER (jeder Commit, ohne Ausnahme, wörtlich als letzte Zeile):**
> `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
> Frühere Agents haben hier wiederholt „Opus“ oder einen anderen Namen eingesetzt. Vor jedem `git commit` die Zeile Zeichen für Zeichen prüfen.
> Immer nur die genannten Pfade einzeln `git add`en (nie `git add -A`/`.`/`-u`).
> **Nie `git checkout -- …`, `git stash`, `git reset` oder `git restore`** (auch nicht zum Zurücknehmen von Mutationen): Das hat schon fremde, noch nicht gesicherte Arbeit vernichtet. Mutationen werden mit dem Editor eingespielt und mit dem Editor zurückgenommen; `git diff` zeigt danach, dass nichts übrig ist.

**Goal:** „Datei → STEP importieren …“ liest STEP-Dateien (AP203/214/242, Text) und legt jeden lesbaren Volumenkörper als gewöhnlichen exakten Körper „STEP-Körper“ an (verrechnen, verrunden, Skizze auf Fläche, Zeichnung, Export), an der Lage aus der Datei, als **ein** rücknehmbarer Schritt; die Form steht als normalisierter STEP-Text in der Projektdatei (Format 10).

**Architecture:** Ein eigener Leser in `planora-step/src/read/` (neben dem Schreiber): Indexlauf über die Datei (Kennung → Byte-Bereich, Zeile, Typname), Einträge werden erst bei Bedarf zerlegt; darauf Einheiten/Kontext, Geometrie-Auflösung, Topologie-Aufbau zu einem Kernel-`Solid` (Schleifen, Orientierung, Parameterbereiche, Nähte, Pole, Zusammenziehen), Baugruppen-Lagen und eine Pipeline, die jeden Körper sofort **normalisiert** (mit unserem Schreiber bei festem Zeitstempel schreiben, wieder lesen, genau dieses Ergebnis benutzen). `planora-feature` bekommt eine Formtabelle (`StepShapeId → Arc<StepShape>`, der gebaute Körper liegt einmal gebaut im `Arc`) und die Feature-Art `StepBody`, die über die gewöhnliche Fertigstellung (`finish`: `validate`, Marken) läuft; `planora-project` speichert den Text ab Format 10; die App liefert Fenster, Hintergrundarbeit und den einen Rückgängig-Schritt.

**Tech Stack:** Rust-Workspace, keine neuen externen Abhängigkeiten, eigener Lexer/Parser; egui/eframe 0.36; Tests im eigenen Harness (Vorbilder `crates/planora-step/tests/{reader.rs, freeform.rs, curved.rs, stage3.rs}`, `crates/planora-feature/tests/{mesh_bodies.rs, restore.rs}`, `crates/planora-project/tests/{format3.rs, parameter_format.rs}`, `crates/planora-app/tests/{import_dialog.rs, mesh_flow.rs, parameter_ui.rs, wide_dialogs.rs, undo_labels.rs}`).

**Spec:** `docs/superpowers/specs/2026-10-21-step-import-design.md` (vom Nutzer freigegeben samt §4a und den Antworten §4b: Lagen aus der Datei je Körper anwenden, Hierarchie ignorieren; unlesbare oder ungültige Körper auslassen, Rest übernehmen; STEP-Text in der Projektdatei; grobe Dateigenauigkeit mit Warnung übernehmen; Kernel-Änderungen nur als Vorschlag mit Zustimmung). Basis: `master` @ `c16d988`; Arbeitszweig `feature/step-import` in eigenem Worktree.

## Global Constraints

- Texte deutsch in einfacher Laiensprache. Feste Texte: Menü „Datei → STEP importieren …“ (direkt unter „Netz importieren …“, Symbol `Icon::Import`), Fenstertitel „STEP importieren“, Abschnitt „Datei wählen (STEP)“, Fortschritt „Die Datei wird gelesen …“ und „Die Körper werden geprüft …“, Knöpfe „Importieren“, „Abbrechen“, „Schließen“, Spalten „Name“, „Flächen“, „Größe“, „Einheit“, „Status“, Status „wird übernommen“, „wird übernommen – {Warnung}“, „wird ausgelassen: {Grund}“, Abschnitt „Einheit der Datei“, Schrittname „STEP importieren“, Feature-Bezeichnung „STEP-Körper“, Flächenmarke „STEP-Fläche {n}“, Meldung nach dem Import `summary` (Aufgabe 14). Meldungstabelle des Lesers in Aufgabe 1 (wörtlich).
- Grenzen (Spec §5, §4a.5): Datei höchstens **200 MB** (`MAX_STEP_FILE_BYTES = 200 * 1024 * 1024`), höchstens **5 000 000** Einträge (`MAX_STEP_ENTITIES`), höchstens **50** Körper je Datei (`MAX_STEP_BODIES`, gezählt werden Vorkommen), höchstens **20 000** Flächen je Körper (`MAX_STEP_FACES`, §4a.5 ersetzt die 100 000 aus §5), Klammertiefe höchstens **64** (`MAX_STEP_NESTING`), Baugruppentiefe höchstens **32** (`MAX_ASSEMBLY_DEPTH`), Namen höchstens **64** Zeichen (`MAX_BODY_NAME_CHARS`).
- Genauigkeit (§4a.3): Zusammenziehen nur mit `weld_tol = clamp(Dateiwert oder 1e-3, 1e-6, 1e-3)` mm (`DEFAULT_FILE_TOL_MM = 1e-3`, `MAX_WELD_TOL_MM = 1e-3`, `MIN_WELD_TOL_MM = 1e-6`); **keine Lücken schließen**; Warnung, wenn `weld_tol > COARSE_TOL_MM = 1e-5` („grob“). Kernel-Toleranz bleibt `Tolerance::default()` (1e-6, `NURBS_EDGE_TOL` 1e-5).
- **Kernel unangetastet:** Ohne Diff gegen `c16d988`: `crates/planora-kernel/**`, `crates/planora-math/**`, `crates/planora-mesh/**`, `crates/planora-exchange/**`, `crates/planora-drawing/**`, `crates/planora-sketch/**`, `crates/planora-render/**`. Zeigt das Machbarkeitstor (Aufgabe 4) oder eine spätere Aufgabe, dass es ohne Kernel-Änderung nicht geht: **anhalten**, Befund und Vorschlag an den Controller; nur Dennis entscheidet.
- **Keine neuen externen Abhängigkeiten.** Erlaubt sind genau zwei interne Kanten: `crates/planora-feature/Cargo.toml` bekommt `planora-step.workspace = true` (Aufgabe 11), `crates/planora-step/Cargo.toml` bekommt `[dev-dependencies] planora-drawing.workspace = true` (Aufgabe 4); `Cargo.lock` ändert sich nur um diese Kanten; `deny.toml` ohne Diff; `cargo deny check` grün mit den in `deny.toml` vermerkten Ausnahmen (zbus-Familie, ttf-parser: bei Abschluss erneut prüfen und Ergebnis berichten).
- **Testdateien nur selbst erzeugt oder handgeschrieben** (Lizenz): keine fremden STEP-Beispiele; jede Datei unter `crates/planora-step/tests/data/` beginnt nach `ISO-10303-21;` mit dem Kommentar `/* Selbst geschrieben für die Tests von PlanoraCAD (GPL-3.0-only). */`.
- **Anti-Ziel der Spec:** nie ein Körper, der still anders ist als in der Datei (falsche Fläche, verdrehte Seiten, falsche Einheit, falsche Lage). Im Zweifel wird der Körper mit deutscher Begründung ausgelassen; die übrigen werden übernommen. Kein `panic!`, kein `unwrap` auf Dateidaten, keine Rekursion ohne Tiefengrenze.
- Dateien ohne STEP-Körper bleiben **Byte für Byte** wie bisher (alle Golden-Dateien unter `crates/planora-project/tests/{data,fixtures}` unverändert).
- Farben nur über `theme::` (Wächter `no_hard_coded_colors`), Fenster nur über `dialog::Dialog` (`no_raw_windows`), Textfelder nur über `theme::text_field*` (`no_raw_text_edits`), Zeichen nur aus Montserrat/Roboto/Phosphor (`tests/fonts.rs`); neues Symbol Phosphor Light `file-arrow-down` U+E232 aus `crates/planora-app/assets/fonts/Phosphor-Light-style.css` (`tests/icons.rs`); gleiche Kartenhöhe in einer Reihe; Oberfläche über die ganze Breite.
- **Oberflächentests (Lehre aus Teilprojekt Parameter):** Tippen nur mit **echten Tasten, ein Zeichen je Bild** (`Event::Key{pressed:true}` + `Event::Text(c)` + `Event::Key{pressed:false}`, danach ein leeres Bild); Kennungen von Feldern und Zeilen hängen **nie** vom Text ab (`ui.push_id(("step-row", index))`); jedes Fenster hält Texte und Zustand nach Rückgängig/Wiederholen stimmig (Prüfung in Aufgabe 14).
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests mit `-j 2` **im Vordergrund** (langsamer Rechner; nie `cargo clean`; keine Hintergrundprozesse zurücklassen). Lange Reihen nur mit `--release -- --ignored`.
- **Tests müssen beißen:** Jede Aufgabe endet mit mindestens **10 Mutationen** (einzeln mit dem Editor einspielen, roten Test notieren, mit dem Editor zurücknehmen); Bericht „Mutation → roter Test“. Bleibt eine Mutation grün, wird der Test schärfer.
- Keine Streudateien: Testdateien in `std::env::temp_dir()`-Unterordnern, am Ende entfernt; nichts im Wurzelordner des Repos ablegen; `git status` am Ende sauber.
- Bestehende Tests bleiben unverändert grün, außer: (1) `assert_eq!(FORMAT_VERSION, 9)` in `crates/planora-project/tests/{dimensions_format.rs, drawings_format.rs, format2.rs, format3.rs, parameter_format.rs, parts_format.rs, section_format.rs, sheet_format.rs}` → `10`; (2) „zu neu“-Proben mit Format 10 (`grep -rn '"format\\": 10\|found: 10' crates/planora-project/tests`) → 11; (3) Literale `Snapshot { … }` (`crates/planora-feature/tests/{dimensions.rs, drawings.rs}` und was der Compiler nennt) bekommen `step_shapes: Default::default()`; (4) die Menüliste in `crates/planora-app/tests/graphit_style.rs` bekommt „STEP importieren …“; (5) Hilfsfunktionen aus `crates/planora-step/tests/{freeform.rs, curved.rs}` wandern unverändert nach `tests/common/bodies.rs` (Aufgabe 10). Sonst nichts.
- Abweichungen von der Spec stehen im Ledger des Controllers, am Ende in `docs/superpowers/offene-punkte.md` und im Abschnitt „Abweichungen bei der Umsetzung“ dieses Plans.

**Festlegungen des Plans (P1–P16; der Nutzer kann sie kippen):**
- **P1 Lesen in zwei Stufen:** Ein Indexlauf zerlegt die Datei in Einträge (Kennung → Byte-Bereich, Zeile, erster Typname) und prüft Kopf, Struktur, doppelte Kennungen und Grenzen; Parameter eines Eintrags werden erst beim Gebrauch zerlegt (`StepFile::entity`). Fehlende Verweise fallen dort auf und treffen nur die Körper, die sie brauchen. Speicher: Dateibytes plus etwa 32 Bytes je Eintrag.
- **P2 Was ist ein Körper:** Je Vorkommen einer Darstellung (`ADVANCED_BREP_SHAPE_REPRESENTATION` oder `SHAPE_REPRESENTATION`) mit mindestens einem `MANIFOLD_SOLID_BREP`/`BREP_WITH_VOIDS` ein Körper; alle Volumen-Einträge dieser Darstellung werden Schalen **eines** `Solid` (so überlebt ein Körper aus zwei Stücken den Round-Trip). Volumen-Einträge, die in keiner Darstellung stehen, sind je ein eigener Körper. Ein Teil, das die Baugruppe zweimal benutzt, ergibt zwei Körper.
- **P3 Lagen:** Richtung jeder `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` wird über die zugehörige `NEXT_ASSEMBLY_USAGE_OCCURRENCE` bestimmt (Elternteil = `relating_product_definition`); `ITEM_DEFINED_TRANSFORMATION(it1, it2)` ergibt `T = A(Eltern-Lage) · A(Kind-Lage)⁻¹`; `MAPPED_ITEM`/`REPRESENTATION_MAP` ebenso; `SHAPE_REPRESENTATION_RELATIONSHIP` ohne Transformation ist die Einheitslage. Verschachtelte Lagen werden bis zur Wurzel multipliziert. Spiegelung, Skalierung (`CARTESIAN_TRANSFORMATION_OPERATOR_3D` mit Maßstab ≠ 1 oder schiefen Achsen), Zyklen, Tiefe > 32 oder mehrdeutige Richtung → Meldung, Körper am Ursprung (Spec §4b.2).
- **P4 Genauigkeit:** `weld_tol` wie in den Global Constraints; fehlt `UNCERTAINTY_MEASURE_WITH_UNIT` (oder ist jeder Wert ≤ 0 bzw. nicht endlich: solche Werte werden übergangen, die Datei bleibt lesbar – Nacharbeit Aufgabe 2), gilt 1e-3 mm **und** der Körper bekommt die Warnung (vorsichtig, „Genauigkeit unbekannt“ = grob). Gespeichert wird `weld_tol` als „Genauigkeit der Datei“ (`StepShape::tolerance_mm`), damit die Warnung nach dem Laden bleibt.
- **P5 Normalisieren:** `normalize(solid, name)` schreibt mit `write_step` bei `time = UNIX_EPOCH`, `file_name = "planora-step-koerper.step"`, `Tolerance::default()`, liest den Text mit demselben Leser wieder (`read_normal_body`) und liefert Text **und** diesen Körper. Session = Neuladen. Lehnt der Schreiber ab (schiefes System > 1e-9, Torus klein ≥ groß, Kegel jenseits der Spitze), wird der Körper mit dessen Grund ausgelassen.
- **P6 Keine Lagefelder:** `FeatureKind::StepBody { shape }` hat keine Zahlenfelder und keine Formeln; Verschieben geht über das gewöhnliche Verschieben-Feature; das Bearbeiten-Fenster gibt es für ihn nicht (`FeatureDialog::new` → `None`), Umbenennen im Baum.
- **P7 Marken:** Jede Fläche bekommt `FaceTag { origin: 0, role: "STEP-Fläche {n}" }`, `n` ab 1 in der Reihenfolge der Flächen im normalisierten Text (Schalen, dann Flächen); `finish` setzt die Herkunft auf die Feature-Kennung. Weil Session und Laden denselben Text lesen, sind die Marken nach dem Laden gleich.
- **P8 Dreh- und Strangflächen:** Wo exakt möglich, wird analytisch umgerechnet (Gerade parallel zur Achse → Zylinder, Gerade schneidet Achse → Kegel, Gerade senkrecht → Ebene, Kreis in Achsebene → Torus bzw. Kugel; Gerade entlang Vektor → Ebene, Kreis entlang Normale → Zylinder), sonst NURBS (`NurbsSurface::revolve` über die ganze Kurve, `NurbsSurface::extrude` über den aus den Schleifen bestimmten Bereich). Ob die natürliche Normale gekippt ist (`flipped`), entscheidet der Vergleich der STEP-Normale (Ableitungen der Definition) mit der Kernel-Normale an einem Probepunkt, nie eine Annahme.
- **P9 Nähte:** Periodische Flächen ohne Naht (zwei volle Kreise, Kugel ohne Kanten) bekommen eine eingefügte Naht nur bei Zylinder, Kegel und Kugel; andere Flächenarten ohne Naht → Körper ausgelassen („Fläche vom Typ {Typ} ohne Naht wird noch nicht unterstützt.“).
- **P10 Namen:** Produktname, sonst Name der Darstellung, sonst Name des Volumen-Eintrags; getrimmt, Steuerzeichen → Leerzeichen, höchstens 64 Zeichen; Doppelte „Name (2)“, „Name (3)“ …; leer → die App vergibt „STEP-Körper n“ (Zähler wie „Netz-Körper n“).
- **P11 Einheit umstellen:** Die Wahl im Fenster ersetzt die Längeneinheit **aller** Darstellungen der Datei (auch Lagen und Genauigkeit, soweit diese in der Längeneinheit des Kontexts steht; eine Genauigkeit mit eigener Einheit, etwa ausdrücklich mm in einer Zoll-Datei, bleibt – Nacharbeit Aufgabe 2, `Units::uncertainty_follows_length`); der Winkel bleibt wie in der Datei. Die Neuberechnung läuft im Hintergrund auf der schon zerlegten Datei (`Arc<StepFile>`).
- **P12 Grenzen:** Mehr als 50 Körper (Vorkommen) → ganze Datei abgelehnt mit „Die Datei enthält mehr als 50 Körper.“; mehr als 20 000 Flächen → nur dieser Körper ausgelassen.
- **P13 Fenster:** Wie „Netz importieren“: Ergebnis gilt nur für den Dokumentstand bei der Dateiwahl (sonst `STALE`), das Fenster zählt zu `dialog_open` (Strg+Z wirkt nicht, solange es offen ist).
- **P14 Laden:** Ein beschädigter STEP-Text in einer Projektdatei lässt die Datei laden; der STEP-Körper bekommt den Fehlerstatus mit Meldung („Die gespeicherte STEP-Form lässt sich nicht lesen: …“). `decode_version` prüft nur den Kopf (`check_normal_header`), weil `encode_version` sich selbst dekodiert.
- **P15 Fortschritt:** Zwei Aufträge nacheinander: Lesen (Fortschritt in KiB) und Prüfen/Bauen (Fortschritt je Fläche); Abbrechen in beiden, geprüft alle `CHECK_EVERY` (4096) Schritte und je Körper.
- **P16 Nachziehen der Ecken (bedingt, Aufgabe 7 Schritt 4):** Nur wenn das Tor zeigt, dass grobe analytische Körper ohne es scheitern: Ecken werden innerhalb `weld_tol` auf den Schnitt ihrer analytischen Flächen gezogen (Zusammenziehen, kein Lückenschließen). Entscheidung trägt der Controller nach Aufgabe 4 ins Ledger ein.

## Review Focus

1. **Datei aus einem anderen CAD-Programm mit Drehfläche, Strangfläche, ungeklemmten oder rationalen B-Splines und Zylindern aus zwei Halbschalen:** Körper kommt an, Volumen und Hülle wie berechnet, keine nach innen gekehrte Fläche (Boolesche Operation mit einem Quader ergibt das erwartete Volumen). → Aufgabe 5 Tests (d, e, f), Aufgabe 3 Test (b), Aufgabe 10 Test (c).
2. **Körper mit Genauigkeit 1e-3 mm** (Ecken je Fläche einzeln, um bis zu 9e-4 mm versetzt): übernommen mit Warnung; Abziehen und Verrunden gelingen oder melden klar „…“ – nie Status Ok mit falschem Volumen; Lücke 2e-3 mm → ausgelassen mit „Lücke von 0,002 mm …“. → Aufgabe 4 (Messung), Aufgabe 7 Tests (c, d, e), Aufgabe 13 Test (e).
3. **Baugruppe mit Lagen, mehrfach benutztem Teil, Zoll und Grad:** Körper liegen wie in der Datei, Maße ×25,4, Kegelwinkel richtig; gespiegelte Lage → Meldung, Körper am Ursprung; Einheit im Fenster auf Millimeter umgestellt → Hülle 1 × 2 × 3 mm. → Aufgabe 2 Test (c), Aufgabe 8 Tests (a–e), Aufgabe 14 Test (e).
4. **Abgeschnittene, riesige oder feindliche Datei** (bei jedem 97. Byte abgeschnitten, Verweis im Kreis, `1E400`, 100 Klammern tief, doppelte Kennung, 6 Millionen Einträge, 201 MB): deutsche Meldung mit Zeile, nie Absturz, Oberfläche bleibt bedienbar, Abbrechen wirkt. → Aufgabe 1 Tests (d–g), Aufgabe 9 Test (e), Aufgabe 10 Test (e), Aufgabe 14 Tests (d, g).
5. **Importierten Körper weiterverwenden und wieder öffnen:** Abziehen, Verrunden, Skizze auf „STEP-Fläche 3“, Zeichnung mit Schnitt, Stückliste, STEP-Export; Speichern, Schließen, Öffnen: dieselben Marken, die Skizze sitzt auf derselben Fläche; Rückgängig des Imports als ein Schritt. → Aufgabe 11 Tests (c, d, g), Aufgabe 12 Test (c), Aufgabe 13 Tests (b, c, d).

---

### Task 1: Lexer, Index und Einträge (`read/lex.rs`, `read/index.rs`, `read/value.rs`)

**Files:**
- Create: `crates/planora-step/src/read/mod.rs` (Modulkommentar mit P1; `pub mod error; pub mod lex; pub mod index; pub mod value;` – spätere Aufgaben fügen Module an)
- Create: `crates/planora-step/src/read/error.rs`, `crates/planora-step/src/read/lex.rs`, `crates/planora-step/src/read/index.rs`, `crates/planora-step/src/read/value.rs`
- Modify: `crates/planora-step/src/lib.rs` (`pub mod read;` und Re-Exporte unten; Kopfkommentar „STEP-Export und -Import“)
- Test: `crates/planora-step/tests/read_parse.rs` (neu; unabhängig vom Testleser `tests/common/reader.rs`, der bleibt unverändert)

**Interfaces:**
- Consumes: `planora_exchange::{Progress, CHECK_EVERY}`.
- Produces:
```rust
// read/error.rs
pub const MAX_STEP_FILE_BYTES: u64 = 200 * 1024 * 1024;
pub const MAX_STEP_ENTITIES: usize = 5_000_000;
pub const MAX_STEP_BODIES: usize = 50;
pub const MAX_STEP_FACES: usize = 20_000;
pub const MAX_STEP_NESTING: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadLimits { pub max_bytes: u64, pub max_entities: usize, pub max_bodies: usize, pub max_faces: usize }
impl ReadLimits { pub const DEFAULT: ReadLimits; }   // die vier Konstanten oben
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StepReadError {
    NotStep, Truncated { line: usize }, TooLarge { bytes: u64 }, TooManyEntities,
    Duplicate { line: usize, id: u64, first: usize }, MissingRef { line: usize, id: u64, target: u64 },
    Cycle { line: usize, id: u64 }, BadNumber { line: usize, text: String }, TooDeep { line: usize },
    UnexpectedChar { line: usize, c: char }, OpenString { line: usize }, OpenComment { line: usize },
    Syntax { line: usize, msg: String }, Io { msg: String }, NoBodies, TooManyBodies, Cancelled, NoEntity { id: u64 },
}
// read/value.rs
#[derive(Clone, Debug, PartialEq)]
pub enum Value { Int(i64), Real(f64), Str(String), Enum(String), Ref(u64), List(Vec<Value>), Typed(String, Vec<Value>), Unset, Derived }
impl Value {   // Fehler als `String` (Grund ohne Zeile; der Aufrufer setzt die Zeile davor)
    pub fn real(&self) -> Result<f64, String>;     // nimmt auch Int (nachsichtig)
    pub fn int(&self) -> Result<i64, String>;
    pub fn reference(&self) -> Result<u64, String>;
    pub fn list(&self) -> Result<&[Value], String>;
    pub fn text(&self) -> Result<&str, String>;
    pub fn enumeration(&self) -> Result<&str, String>;
    pub fn logical(&self) -> Result<Option<bool>, String>;   // .T./.F./.U. (U → None)
    pub fn refs(&self) -> Result<Vec<u64>, String>;
    pub fn reals(&self) -> Result<Vec<f64>, String>;
    pub fn is_unset(&self) -> bool;                // `$` oder `*`
}
#[derive(Clone, Debug, PartialEq)] pub struct Record { pub name: String, pub args: Vec<Value> }
#[derive(Clone, Debug, PartialEq)] pub enum Entity { Simple(Record), Complex(Vec<Record>) }
impl Entity {
    pub fn is(&self, name: &str) -> bool;           // einfacher Typ oder Teil eines zusammengesetzten
    pub fn part(&self, name: &str) -> Option<&[Value]>;  // auch für Simple mit diesem Namen
    pub fn names(&self) -> Vec<&str>;
}
// read/index.rs
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepHeader { pub schemas: Vec<String>, pub file_name: String, pub system: String }
pub struct StepFile { /* Bytes, Index, Kopf; Send + Sync */ }
impl StepFile {
    pub fn parse(bytes: Vec<u8>, limits: &ReadLimits, progress: &Progress) -> Result<StepFile, StepReadError>;
    pub fn header(&self) -> &StepHeader;
    pub fn len(&self) -> usize;
    pub fn kib(&self) -> u64;                        // Dateigröße in KiB, mindestens 1
    pub fn limits(&self) -> &ReadLimits;
    pub fn entity(&self, id: u64) -> Result<Entity, StepReadError>;   // zerlegt jetzt; prüft, dass alle Verweise existieren
    pub fn line(&self, id: u64) -> Option<usize>;
    pub fn ids_of(&self, name: &str) -> &[u64];      // einfache Einträge dieses Typs und zusammengesetzte mit diesem Teil, aufsteigend, ohne Doppelte (Nacharbeit: Slice statt Vec)
    pub fn fail(&self, id: u64, msg: impl Into<String>) -> StepReadError;   // Nacharbeit: Syntax mit der Zeile von #id (unbekannt → NoEntity)
}
// Nacharbeit Aufgabe 1: Value::measure() (Real/Int/Typed(_, [Zahl])), Entity::arg(name, i) -> Result<&Value, String>,
// RefPath { new, with_limit(n), limit, enter(file, id), leave, depth } erkennt längere Verweiskreise (Cycle) und begrenzt die Tiefe (Vorgabe MAX_REF_DEPTH = 256; der Resolver aus Aufgabe 2 nimmt 64).
// entity() zerlegt bei jedem Aufruf neu: Folger speichern Ergebnisse selbst zwischen.
pub fn read_step_file(path: &std::path::Path, limits: &ReadLimits, progress: &Progress) -> Result<StepFile, StepReadError>;
```
Re-Export in `lib.rs`: `pub use read::error::{MAX_STEP_BODIES, MAX_STEP_ENTITIES, MAX_STEP_FACES, MAX_STEP_FILE_BYTES, ReadLimits, StepReadError}; pub use read::index::{StepFile, StepHeader, read_step_file};`

**Meldungstabelle des Lesers (wörtlich, alle Aufgaben benutzen sie; `{n}` Zeile ab 1):**

| Fall | Text |
|---|---|
| `NotStep` | `Die Datei ist keine STEP-Datei (am Anfang fehlt „ISO-10303-21;“).` |
| `Truncated` | `Die Datei ist unvollständig: Sie endet in Zeile {n}, bevor alles gelesen ist.` |
| `TooLarge` | `Die Datei ist zu groß ({x} MB); erlaubt sind höchstens 200 MB.` (x mit Komma, eine Nachkommastelle, MB = 1024·1024 Bytes) |
| `TooManyEntities` | `Die Datei hat mehr als 5 Millionen Einträge.` |
| `Duplicate` | `Zeile {n}: Die Nummer #{id} kommt doppelt vor (zuerst in Zeile {first}).` |
| `MissingRef` | `Zeile {n}: #{id} verweist auf #{target}, den es nicht gibt.` |
| `Cycle` | `Zeile {n}: Die Einträge verweisen im Kreis (#{id}).` |
| `BadNumber` | `Zeile {n}: Die Zahl „{text}“ ist zu groß oder keine gültige Zahl.` (`text` höchstens 32 Zeichen) |
| `TooDeep` | `Zeile {n}: Die Klammern sind zu tief verschachtelt (höchstens 64 Ebenen).` (nur Klammertiefe des Lexers; zu tiefe Verweisketten melden `RefPath`/Resolver als `Syntax` „Die Verweise sind zu tief verschachtelt (höchstens {Grenze} Ebenen).“) |
| `UnexpectedChar` | `Zeile {n}: Unerwartetes Zeichen „{c}“.` (Steuer- und unsichtbare Zeichen als `U+XXXX`) |
| `OpenString` | `Zeile {n}: Eine Zeichenkette ist nicht geschlossen.` |
| `OpenComment` | `Zeile {n}: Ein Kommentar ist nicht geschlossen.` |
| `Syntax` | `Zeile {n}: {msg}` (`msg` z. B. „Es fehlt ein „;“.“, „Es fehlt eine schließende Klammer.“, „Nach „#{id}“ fehlt „=“.“) |
| `Io` | `Die Datei lässt sich nicht lesen: {msg}` |
| `NoBodies` | `Die Datei enthält keine Volumenkörper.` |
| `TooManyBodies` | `Die Datei enthält mehr als 50 Körper.` |
| `Cancelled` | `Abgebrochen` |
| `NoEntity` | `Den Eintrag #{id} gibt es nicht.` (nur bei Fehlern des Aufrufers) |

**Meldungstabelle des Körperaufbaus (`Skip`, ein Körper wird ausgelassen; ergänzt in den Aufgaben 3–6; `Zeile {n}:` steht davor, wo ein Eintrag bekannt ist; `{f}` ist die Marke „STEP-Fläche n“):**

| Fall | Text |
|---|---|
| Rand schließt sich nicht (Aufgabe 3; seit Aufgabe 6 auch: Ränder um die Achse, die zu keiner Fläche ohne Naht passen) | `Der Rand der Fläche „{f}“ schließt sich nicht: Eine Kante läuft falsch herum.` |
| Rand und Sinn passen nicht (Aufgabe 3; seit Aufgabe 6 auch am Pol) | `Die Fläche „{f}“ ist verdreht: Rand und Außenseite passen nicht zusammen.` |
| Kante der Länge null, die sich nicht entfernen lässt (Aufgabe 3; seit Aufgabe 6 erst nach dem Entfernen entarteter Kanten) | `Zeile {n}: Eine Kante hat die Länge null.` |
| Fläche ohne Rand (seit Aufgabe 6 erst nach dem Einfügen der Nähte) | `Zeile {n}: Eine Fläche hat keinen Rand.` |
| Leere Schale (Aufgabe 3) | `Der Körper hat keine Flächen.` |
| `validate` bzw. Vernetzung (Aufgabe 3) | `Der Körper ist ungültig: {Grund}` (z. B. `Die Kante bei ({x}; {y}; {z}) gehört nur zu einer Fläche.`), `Die Fläche „{f}“ lässt sich nicht in Dreiecke zerlegen.`, `Der Körper hat kein Volumen.` |
| Kegel über die Spitze (Aufgabe 5) | `Zeile {n}: Eine Kegelfläche reicht über ihre Spitze hinaus.` |
| Profil kreuzt die Drehachse (Aufgabe 5; seit Aufgabe 6 erst um mehr als `weld_tol`, nur im getrimmten Bereich) | `Das Profil einer Drehfläche schneidet die Drehachse.` |
| Periodische Fläche ohne Naht, nicht Zylinder/Kegel/Kugel (P9, Aufgabe 6) | `Fläche vom Typ {Typ} ohne Naht wird noch nicht unterstützt.` (`{Typ}`: `Torus`, `Freiformfläche`) |
| Rand nahe einem Pol, aber nicht genau durch ihn (Aufgabe 6) | `Der Rand der Fläche „{f}“ läuft nicht genau durch einen Pol.` |
| Naht lässt sich nicht einfügen (Aufgabe 6; auch: kein Winkel mit Abstand zu Löchern, Mantellinien und Ecken des Rands) | `Der Fläche „{f}“ lässt sich keine Naht hinzufügen.`, `Zeile {n}: Eine Naht hätte die Länge null.` |
| Kugelkappe ohne Naht und ohne Pol in einer Schale, die umgedreht werden müsste (Aufgabe 6, Nacharbeit) | `Die Seite der Fläche „{f}“ ist nicht eindeutig: Sie hat weder Naht noch Pol, und der Körper müsste umgedreht werden.` |
| Hohlraum ragt aus seiner äußeren Schale (Aufgabe 6, Nacharbeit) | `Ein Hohlraum liegt nicht ganz im Körper.` |
| Stücke eines Körpers überschneiden sich, auch ein innen-außen geschriebenes Stück im anderen (Aufgabe 6, Nacharbeit; Berühren erlaubt) | `Zwei Stücke des Körpers überschneiden sich.` |
| Getrimmtes Profil einer Drehfläche (Aufgabe 6) | `Zeile {n}: Eine Trimmkurve hat keine Grenze.`, `Zeile {n}: Eine Grenze einer Trimmkurve ist keine endliche Zahl.`, `Zeile {n}: Eine Trimmkurve hat keine Ausdehnung.`, `Ein getrimmtes Profil lässt sich nicht ausschneiden.` |
| Probe am fertigen Netz je Stück (Aufgabe 6, Nacharbeit 4): dicht hinter einer Fläche ist die Umlaufzahl nicht 1 oder dicht davor nicht 0 (doppelt belegte, gegengleich übereinanderliegende oder falsch liegende Flächen, Ränder über die falsche Kreishälfte; seit Aufgabe 10 auch vor dem Netz: Naht auf einer gekrümmten Fläche, die sich in keiner Richtung schließt) | `Die Fläche „{f}“ liegt nicht richtig im Körper: Innen und außen passen an ihr nicht zusammen.` |
| Schale ohne Volumen (Aufgabe 6, Nacharbeit 4: Volumen unter 1e-6 der Summe der Tetraederbeträge, etwa zwei gegengleiche Flächen am selben Ort) | `Der Körper hat kein Volumen.` (wie bei `validate`) |
| Offene Kante mit einer anderen offenen Kante daneben (Aufgabe 7: geprüft vom Leser vor `validate`, nach Nähten und Rändern, damit P9 und „keinen Rand“ zuerst kommen; Partner = nächste offene Kante, Mittelpunkte höchstens `max(100 · weld_tol, 0,01 mm)` auseinander; `{d}` = größter Abstand gleichlaufender Punkte beider Kanten – Ecken, Enden, Viertel, Mitte –, Komma, höchstens vier gültige Stellen, ohne überflüssige Nullen; Ort = Mitte der offenen Kante; Kopien einer Ecke weiter als `weld_tol` auseinander werden nicht zusammengezogen) | `Lücke von {d} mm an der Kante bei ({x}; {y}; {z}).` |
| Offene Kante ohne Partner (Aufgabe 7: dieselbe Prüfung; gleicher Text wie bisher aus `validate`) | `Der Körper ist ungültig: Die Kante bei ({x}; {y}; {z}) gehört nur zu einer Fläche.` |
| Ecke weiter als 1e9 mm vom Ursprung (Aufgabe 7, Nacharbeit: das Raster des Zusammenziehens unterschiede dort keine Zellen mehr) | `Der Körper liegt weiter als 1 000 km vom Ursprung entfernt.` |

Warnung des Körperaufbaus (kein `Skip`, der Körper wird übernommen; Aufgabe 7): grobe Datei (`Units::is_coarse`: Genauigkeit gröber als 1e-5 mm oder nicht genannt), `{tol}` = `weld_tol` wie `{d}` geschrieben (`0,001`) → `Genauigkeit der Datei {tol} mm: Rechnen mit diesem Körper kann scheitern.`

**Meldungen der Baugruppen-Lagen (Aufgabe 8, `read/assembly.rs`):**

| Fall | Text |
|---|---|
| Lage nicht bestimmbar (Warnung, der Körper wird am Ursprung übernommen; gilt für alles unter dieser Lage) | `Die Lage von „{name}“ ließ sich nicht bestimmen ({Grund}); der Körper liegt am Ursprung.` |
| Gründe | `Spiegelung` (negative Determinante, auch Maßstab −1), `Skalierung` (Maßstab ≠ 1 oder gegebene Achsen eines `CARTESIAN_TRANSFORMATION_OPERATOR_3D` nicht senkrecht, Kosinus > 1e-6), `Verweise im Kreis` (Darstellung in einem Kreis oder nur über einen Kreis erreichbar; je einmal), `zu tief verschachtelt` (mehr als 32 Vorkommen zwischen Wurzel und Körper), `Richtung unklar` (Vorkommen ohne `NEXT_ASSEMBLY_USAGE_OCCURRENCE` bzw. keine der beiden Darstellungen gehört zu Kind oder Eltern), `unbekannte Art {TYP}` (Lage weder `AXIS2_PLACEMENT_3D` noch `CARTESIAN_TRANSFORMATION_OPERATOR_3D`); eine fehlerhafte Lage selbst (z. B. Richtung der Länge null) oder fehlerhafte Einheiten ihres Kontexts geben deren Meldung ohne Schlusspunkt als Grund |
| Vorkommen (PDS/NAUO hinter einem CDSR) nicht lesbar, Beziehung lesbar (Fix-Runde 1, I1: Grund in der Lage-Meldung) | `Richtung unklar – {Meldung ohne Schlusspunkt}` |
| Lagen einer `ITEM_DEFINED_TRANSFORMATION`, die keiner Zuordnung zu `rep_1`/`rep_2` passen (Fix-Runde 1, I2) | `Richtung unklar` |
| Ganze Datei abgelehnt (Fix-Runde 1, I1: nur wenn sonst ein Körper still an falscher Stelle läge; fehlende Verweise wie gehabt `MissingRef` mit Zeile): Beziehung mit fehlendem oder unlesbarem Ende, unter deren anderem Ende Körper hängen; CDSR ohne lesbare Beziehung, dessen Kind Körper hat; unlesbare Abbildung oder unlesbares CDSR | `Zeile {n}: Erwartet wird eine Darstellung.`, `Zeile {n}: Die Beziehung des Vorkommens fehlt.`, `Zeile {n}: Die Abbildung lässt sich nicht lesen.`, `Zeile {n}: Das Vorkommen lässt sich nicht lesen.` |
| Unlesbare Darstellung und freie Volumen-Einträge (Fix-Runde 1, I1: sie könnten in ihr gestanden haben; Grund in der Lage-Meldung des freien Körpers) | `Zeile {n}: Erwartet wird eine Darstellung` (bzw. die Meldung des Lesers ohne Schlusspunkt) |
| Fehlerhafte Transformation (als Grund in der Lage-Meldung) | `Zeile {n}: Eine Lage der Transformation fehlt`, `Zeile {n}: Parameter des Operators fehlen`, `Zeile {n}: Die Definition fehlt` |
| Nicht unterstützte Körperarten (Fix-Runde 1, M5: `unsupported_kinds`, je Art eine Meldung; Aufgabe 9 legt sie in `StepImport::messages`, Aufgabe 14 zeigt sie) | `Die Datei enthält {n} {Art} ({TYP}); diese Art wird noch nicht unterstützt und ausgelassen.` mit `{Art}` Facettenkörper (`FACETED_BREP`), Flächenmodell/Flächenmodelle (`SHELL_BASED_SURFACE_MODEL`, `FACE_BASED_SURFACE_MODEL`), Drahtmodell/Drahtmodelle (`SHELL_BASED_WIREFRAME_MODEL`, `EDGE_BASED_WIREFRAME_MODEL`) |

**Meldungen der Import-Pipeline (Aufgabe 9, `read/import.rs`):**

| Fall | Text |
|---|---|
| Freier Volumen-Eintrag (in keiner Darstellung), die Kontexte der Datei nennen verschiedene Längen- oder Winkeleinheiten und keine Einheit ist gewählt (`StepReadError::UnclearUnits`; Körper ausgelassen) | `Die Einheit ist unklar: Der Körper steht in keiner Darstellung, und die Datei nennt verschiedene Einheiten.` |
| Lage lässt sich auf den gebauten Körper nicht anwenden (kann bei geprüften Lagen nicht vorkommen; Körper ausgelassen) | `Der Körper lässt sich nicht an seine Lage bringen.` |
| Schreiber lehnt den Körper beim Normalisieren ab (P5; Körper ausgelassen; bei Körpern aus unserem Leser bisher nicht beobachtet) | Grund des Schreibers mit Schlusspunkt, hinter dem Vorsatz der nächsten Zeile, z. B. `Der Körper lässt sich nicht in die Form des Programms umschreiben: Eine Kegelfläche liegt jenseits ihrer Spitze.`; `Der Schreiber lieferte keinen Text.` |
| Lage-Meldung eines Körpers ohne Namen | wie Aufgabe 8 mit dem Namen `STEP-Körper`; `ImportedBody::warnings_named("STEP-Körper n")` bildet sie mit dem Namen der App neu |
| Normalisieren eines gelesenen Körpers scheitert (Fix-Runde 1, I4: ohne „Zeile n“ unseres Textes) | `Der Körper lässt sich nicht in die Form des Programms umschreiben: {Grund}` |
| Richtung einer Beziehung mit Transformation ohne Vorkommen angenommen (Fix-Runde 1, P3; Warnung, Lage angewandt; leerer Name wie oben) | `Die Lage von „{name}“ ist angenommen: Die Datei sagt nicht, welches Teil das übergeordnete ist.` |
| `check_normal_header` / `read_normal_body` (Aufgabe 11 setzt „Die gespeicherte STEP-Form lässt sich nicht lesen: “ davor) | `Der Text ist größer als 64 MB.`, `Der Text enthält Zeichen außerhalb von ASCII.`, `Der Text hat nicht Anfang und Ende einer STEP-Datei.`, `Der Text enthält {n} Körper statt einem.`, `Der Text ist nicht in Millimeter geschrieben.`, `Der Körper im Text liegt nicht an der Einheitslage.`; sonst die Meldungen des Lesers |

Entfallen mit Aufgabe 6 (Platzhalter aus Aufgabe 3): „Ein Körper aus mehreren Stücken wird noch nicht unterstützt.“, „Ein Körper mit Hohlräumen wird noch nicht unterstützt.“, „Ein Rand vom Typ VERTEX_LOOP wird noch nicht unterstützt.“ (andere Randtypen behalten „Ein Rand vom Typ {Typ} wird noch nicht unterstützt.“) und „Eine Fläche, deren Rand durch einen Pol läuft, wird noch nicht unterstützt.“ (mit der Nacharbeit ganz entfallen, samt `derive_patch`/`derive_patch_with_tol` ohne Flächensinn; es gilt `derive_patch_sensed`).

Lexer-Regeln (nachsichtig, wie fremde Schreiber es brauchen): Leerraum `' ' \t \r \n` und Kommentare `/* … */` überall außerhalb von Zeichenketten (auch vor `ISO-10303-21;`); Schlüsselwörter `[A-Za-z_][A-Za-z0-9_-]*` werden in Großbuchstaben verglichen; Zahlen `[+-]?Ziffern[.Ziffern*][(E|e)[+-]?Ziffern]` (mit Punkt oder Exponent → `Real`, sonst `Int`; `Int` über `i64` oder nicht endliches `Real` → `BadNumber`); `#Ziffern` Verweis; `.NAME.` Aufzählung; `$` `*`; Zeichenketten mit `''`, `\\`, `\X2\…\X0\`, `\X4\…\X0\`, `\X\hh` (ein Byte ISO 8859-1), `\S\c` (Zeichen `c` + 128), `\Px\` (wird übergangen); rohe Bytes ≥ 0x80 in Zeichenketten: gültiges UTF-8 als UTF-8, sonst ISO 8859-1 (Nacharbeit), außerhalb sind sie `UnexpectedChar`; eine UTF-8-Kennung (BOM) am Anfang wird übergangen. Ein Eintrag ist `#id = TYP(…);` oder zusammengesetzt `#id = ( A(…) B(…) … );` (Reihenfolge der Teile wird nicht geprüft). Kopf: `ISO-10303-21; HEADER; … ENDSEC;`, dann ein oder mehrere `DATA; … ENDSEC;`, dann `END-ISO-10303-21;`; unbekannte Kopfeinträge werden übergangen. Tiefe von Listen und getypten Werten zählt; ab 65 → `TooDeep` (Zerleger mit eigenem Stapel oder Rekursion mit Zähler). Fortschritt: `set_total(kib)`, je 4096 Einträge und zusätzlich je 1 MiB gelesener Bytes (auch beim ersten Wort) `advance` und Abbruchprüfung.

- [ ] **Step 1: Write the failing tests** in `crates/planora-step/tests/read_parse.rs` (Hilfe `file(data)` baut Kopf + `DATA;` + `data` + Ende wie in `tests/reader.rs`):
  (a) `all_value_kinds_and_complex_entities`: Text mit Kommentar vor dem Kopf, `#3=FOO(#1,$,*,.T.,.UNSPECIFIED.,(1,2,3),LENGTH_MEASURE(1.E-6),'O''Brien \\ \X2\00FC\X0\ \X4\0001F600\X0\ \X\E4 \S\d')` über drei Zeilen umbrochen, Vorwärtsverweis `#1` nach `#3` definiert, zusammengesetzter Eintrag `#4=(BOUNDED_CURVE()B_SPLINE_CURVE(2,(#1,#1,#1),.UNSPECIFIED.,.F.,.F.)…RATIONAL_B_SPLINE_CURVE((1.,0.5,1.))REPRESENTATION_ITEM(''))` → `entity(3)` liefert die Werte in dieser Reihenfolge, Text `"O'Brien \\ ü 😀 ä ä"`; `entity(4).part("RATIONAL_B_SPLINE_CURVE")` = `[List([1.0,0.5,1.0])]`; `ids_of("B_SPLINE_CURVE") == vec![4]`; `line(3) == Some(Zeile von „#3=“)`.
  (b) `lenient_numbers_and_keywords`: `A(1E5, 1.e-3, -0.5, +2, foo_bar())` lädt: `Real(1e5)`, `Real(1e-3)`, `Real(-0.5)`, `Int(2)`, `Typed("FOO_BAR", [])`; `Value::Int(2).real() == Ok(2.0)`.
  (c) `header_is_read`: `schemas == ["AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }"]`, `file_name == "a.step"`, `system` = 6. Feld von `FILE_NAME`; zwei `DATA;`-Abschnitte werden beide gelesen.
  (d) `hostile_files_give_exact_messages` (Tabelle Text → `to_string()` exakt; mit der Hilfe `file` beginnt der Datenteil in Zeile 8): „hallo“ → `NotStep`; gültige Datei bei jedem Byte, das kein Leerraum ist, abgeschnitten (alle Schnittstellen der Probe aus (a)) → nie Panik, immer `Err`, Meldung beginnt mit „Die Datei ist unvollständig“ oder „Zeile“; `#1=A(1E400);` → `Zeile 8: Die Zahl „1E400“ ist zu groß oder keine gültige Zahl.`; `#1=A(99999999999999999999);` → `BadNumber`; 65-fach geschachtelte Klammern → `TooDeep`, 64-fach lädt; `#1=A();\n#1=B();` → `Zeile 9: Die Nummer #1 kommt doppelt vor (zuerst in Zeile 8).`; `#1=A('offen);` → `OpenString`; `/* offen` → `OpenComment`; `#1=A(ä);` → `UnexpectedChar`; `#1=A(1)` ohne `;` vor `ENDSEC` → `Syntax` mit „Es fehlt ein „;“.“.
  (e) `missing_refs_hit_only_the_entity`: `#1=A(#2);\n#3=B(1);` lädt; `entity(1)` → `MissingRef { line: 8, id: 1, target: 2 }` mit Text `Zeile 8: #1 verweist auf #2, den es nicht gibt.`; `entity(3)` ist `Ok`.
  (f) `limits_are_named_errors`: `ReadLimits { max_entities: 3, ..DEFAULT }` mit 4 Einträgen → `TooManyEntities`; `read_step_file` auf eine Datei von `max_bytes + 1` Bytes (Limit 1000 in der Probe) → `TooLarge { bytes: 1001 }` **ohne** die Datei zu lesen (Datei aus Nullbytes, Fehler kommt vor dem Lesen: Zeitmessung nicht nötig, aber `bytes` muss aus den Metadaten stammen); `TooLarge { bytes: 210_000_000 }.to_string() == "Die Datei ist zu groß (200,3 MB); erlaubt sind höchstens 200 MB."`.
  (g) `cancel_and_progress`: `Progress` vorher abgebrochen, 10 000 Einträge → `Err(Cancelled)`; ohne Abbruch `progress.fraction() == 1.0` nach dem Lesen.
  (h) `our_writer_output_is_read`: `common::export(&[("Quader", &box)])` (Quader 10×20×30 aus `AnalyticKernel`) → `StepFile::parse` gelingt, `ids_of("ADVANCED_FACE").len() == 6`, jedes `entity(id)` für alle Kennungen `Ok`.
- [ ] **Step 2: Run** `cargo test -p planora-step --test read_parse -j 2` → FAIL (Modul `read` fehlt).
- [ ] **Step 3: Implement** die Typen und Funktionen der Interfaces in den vier Dateien. Index: `HashMap<u64, Slot { start: u32-oder-usize, end, line: u32, name: Symbol, complex: bool }>` plus `HashMap<String, Vec<u64>>` je Typname (bei zusammengesetzten Einträgen alle Teilnamen). `entity` zerlegt nur den Byte-Bereich des Eintrags mit demselben Lexer.
- [ ] **Step 4: Run** Step-2-Befehl → PASS; dazu `cargo test -p planora-step -j 2` (alte Tests grün).
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Kommentar vor dem Kopf nicht übergangen; `''` nicht entmaskiert; `\X\hh` falsch dekodiert; Tiefe ab 64 statt 65 abgelehnt; doppelte Kennung überschreibt still; Zeilenzählung ohne `\r\n`-Paar; `MissingRef` erst ganz am Ende statt in `entity`; Größe erst nach dem Lesen geprüft; Abbruchprüfung entfernt; `Int`-Überlauf wird `Real`; zweiter `DATA;`-Abschnitt ignoriert.
- [ ] **Step 6: Commit** `feat(step): STEP-Leser – Lexer, Index und Einträge` (Pfade: die vier neuen Dateien, `src/read/mod.rs`, `src/lib.rs`, `tests/read_parse.rs`).

---

### Task 2: Einheiten, Kontext und analytische Geometrie (`read/units.rs`, `read/geom.rs`)

**Files:**
- Create: `crates/planora-step/src/read/units.rs`, `crates/planora-step/src/read/geom.rs`
- Modify: `crates/planora-step/src/read/mod.rs`
- Test: `crates/planora-step/tests/read_geometry.rs`

**Interfaces:**
- Consumes: Aufgabe 1 (`StepFile`, `Entity`, `Value`, `StepReadError`).
- Produces:
```rust
// read/units.rs
pub const DEFAULT_FILE_TOL_MM: f64 = 1e-3;
pub const MAX_WELD_TOL_MM: f64 = 1e-3;
pub const MIN_WELD_TOL_MM: f64 = 1e-6;
pub const COARSE_TOL_MM: f64 = 1e-5;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LengthUnit { Millimeter, Zentimeter, Meter, Zoll, Other { mm: f64 } }
impl LengthUnit { pub fn mm(self) -> f64; pub fn label(self) -> String; }   // "Millimeter", "Zentimeter", "Meter", "Zoll", Other: "1 Einheit = {mm} mm"
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Units { pub length: LengthUnit, pub length_known: bool, pub angle_rad: f64, pub uncertainty_mm: Option<f64>, pub uncertainty_follows_length: bool }   // Feld follows: Nacharbeit (P11)
impl Units {
    pub const MM: Units;                         // Millimeter, bekannt, Radiant, keine Unsicherheit
    pub fn weld_tol_mm(&self) -> f64;            // clamp(uncertainty oder DEFAULT, MIN, MAX)
    pub fn is_coarse(&self) -> bool;             // uncertainty fehlt oder weld_tol > COARSE_TOL_MM
    pub fn with_length(self, l: LengthUnit) -> Units;   // P11: Unsicherheit wird mit umgerechnet
}
pub fn context_units(file: &StepFile, context: u64) -> Result<Units, StepReadError>;
// read/geom.rs
#[derive(Clone, Debug, PartialEq, Eq)] pub struct Skip(pub String);   // Grund, fertig für die Oberfläche
impl From<StepReadError> for Skip { /* Text der Meldung */ }
#[derive(Clone, Debug)]
pub enum SurfaceGeom {
    Ready { surface: Surface, flipped: bool },
    /// Strangfläche aus einer Freiformkurve: der Bereich entlang `dir` folgt aus den Schleifen (Aufgabe 5).
    Extrusion { curve: NurbsCurve, dir: DVec3, flipped: bool },
}
pub struct Resolver<'f> { /* Datei, Einheiten, Zwischenspeicher je Kennung, Auflösungsstapel */ }
impl<'f> Resolver<'f> {
    pub fn new(file: &'f StepFile, units: Units) -> Resolver<'f>;
    pub fn units(&self) -> &Units;
    pub fn file(&self) -> &'f StepFile;
    pub fn point(&mut self, id: u64) -> Result<DVec3, Skip>;       // mm
    pub fn direction(&mut self, id: u64) -> Result<DVec3, Skip>;   // Länge 1
    pub fn placement(&mut self, id: u64) -> Result<Frame, Skip>;   // AXIS2_PLACEMENT_3D, orthonormal ≤ 1e-12
    pub fn curve(&mut self, id: u64) -> Result<Curve, Skip>;
    pub fn surface(&mut self, id: u64) -> Result<SurfaceGeom, Skip>;
    pub fn cached_count(&self) -> usize;                        // Nacharbeit: Zahl der gespeicherten Ergebnisse
}
```
Regeln: Längen × `units.length.mm()`, Winkel × `units.angle_rad`. `AXIS2_PLACEMENT_3D(name, location, axis, ref_direction)`: `axis` `$` → (0,0,1), `ref_direction` `$` → (1,0,0); `x = normalize(ref − (ref·z)z)`; ist das (nach dem Normieren) kürzer als 1e-9: bei `$` beliebige Senkrechte, bei angegebener Bezugsrichtung Skip „Zeile {n}: Die Bezugsrichtung liegt auf der Achse.“ (ISO verbietet sie; Nacharbeit); `y = z × x`. Richtungen werden zuerst durch ihren größten Betrag geteilt (kein Überlauf). Kurven: `LINE(p, VECTOR(d, m))` → `Curve::Line { origin: p, dir: normalize(d) }` (`m < 0` → Skip „Der Betrag eines Vektors ist negativ.“); `CIRCLE(pos, r)`, `ELLIPSE(pos, a1, a2)` (a1 entlang `x`, so wie der Kernel rechnet); `SURFACE_CURVE`/`SEAM_CURVE`/`INTERSECTION_CURVE` → ihre 3D-Kurve; `TRIMMED_CURVE` → Grundkurve (die Ecken der Kante begrenzen sie), bei `sense_agreement` `.F.` mit umgekehrter Laufrichtung (Gerade `dir → −dir`, Kreis/Ellipse `y → −y`, `z → −z`, also `point'(t) = point(−t)`), `.U.` → Skip „Die Laufrichtung einer Trimmkurve ist unbestimmt („.U.“).“ (Nacharbeit, Review I1); `B_SPLINE_*` → bis Aufgabe 5 `Skip("Kante vom Typ {TYP} wird noch nicht unterstützt.")`. Flächen: `PLANE`, `CYLINDRICAL_SURFACE(pos, r)`, `CONICAL_SURFACE(pos, r, α)` → `Cone { radius: r, slope: tan α }` (α in Dateiwinkeln, 0 < α < π/2, sonst Skip), `SPHERICAL_SURFACE`, `TOROIDAL_SURFACE(pos, R, r)` (r ≥ R → `Skip("Eine Torusfläche mit kleinem Radius größer oder gleich dem großen wird noch nicht unterstützt.")`); alle mit `flipped: false`; andere Typen → `Skip("Fläche vom Typ {TYP} wird noch nicht unterstützt.")`. Radien ≤ 0, nicht endliche Zahlen, Richtung der Länge < 1e-12 → Skip mit Grund und Zeile („Zeile {n}: …“). Zyklus (Kennung schon auf dem Weg) → Skip mit Text von `StepReadError::Cycle`; Wegtiefe > 64 (`RefPath::with_limit(64)`) → Skip „Zeile {n}: Die Verweise sind zu tief verschachtelt (höchstens 64 Ebenen).“; Ergebnisse, die an der Tiefe scheitern, werden nicht zwischengespeichert (hängen vom Weg ab). Einheiten: `GLOBAL_UNIT_ASSIGNED_CONTEXT` des Kontexts; `SI_UNIT(prefix, .METRE.)` mit `MILLI`=1, `CENTI`=10, `DECI`=100, `$`=1000, `KILO`=1e6, `MICRO`=1e-3 mm; `CONVERSION_BASED_UNIT(name, LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(f), unit))` = f × Einheit (rekursiv, höchstens 8 Stufen); 25,4 mm → `Zoll`, 10 → `Zentimeter`, 1000 → `Meter`, 1 → `Millimeter` (relativ 1e-9), sonst `Other`; fehlt die Längeneinheit → Millimeter mit `length_known: false`. Winkel: `.RADIAN.` → 1, `CONVERSION_BASED_UNIT` mit `PLANE_ANGLE_MEASURE_WITH_UNIT` → Faktor; fehlt → 1. Unsicherheit: `UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(u), unit, …)` in `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT`, umgerechnet aus ihrer eigenen Einheit, kleinster Wert in mm; Werte ≤ 0 oder nicht endlich werden übergangen (P4). Bekannte Einheiten (Längen, Grad) rasten je Stufe ein, vor dem Vergleich mehrerer Einheiten einer Art; verschiedene Einheiten einer Art im Kontext → Meldung.

- [ ] **Step 1: Write the failing tests** in `tests/read_geometry.rs` (Hilfe: kleiner Text mit Kontext-Einträgen und den zu prüfenden Einträgen):
  (a) `placements_are_orthonormal`: `AXIS2_PLACEMENT_3D('',#p,#z,#x)` mit `x` = (1, 0, 0,1) schief zu `z` = (0,0,1) → `frame.x == (1,0,0)` auf 1e-15, `y == z × x`; `$`-Achsen → Welt; Richtung (0,0,0) → Skip mit „Zeile“.
  (b) `curves_match_hand_values`: Kreis r 5 um (1,2,3) mit Achse z: `curve.point(π/2)` = (1, 7, 3) ±1e-12; Ellipse a1 = 2 < a2 = 4: `point(0)` = Mitte + 2·x; `LINE` mit `VECTOR(…, 7.)` → `dir` hat Länge 1; `SEAM_CURVE('',#c,(#pc1,#pc2),.PCURVE_S1.)` liefert die Kurve `#c`; `TRIMMED_CURVE` liefert die Grundkurve; `TRIMMED_CURVE`, dessen Grundkurve er selbst ist → Skip mit „verweisen im Kreis“.
  (c) `units_and_angles`: Kontext mit `CONVERSION_BASED_UNIT('INCH',#m)` und `LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#mm)` → `length == Zoll`, Punkt (1,0,0) → (25.4,0,0); `CONVERSION_BASED_UNIT('DEGREE',…)` mit `PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(0.0174532925199433),#rad)` und `CONICAL_SURFACE(#a, 1., 45.)` → `slope` = 1 ±1e-12; `SI_UNIT($,.METRE.)` → Meter, Faktor 1000; `SI_UNIT(.CENTI.,.METRE.)` → Zentimeter; Unsicherheit 1e-3 in Zoll → `uncertainty_mm == Some(0.0254)`, `weld_tol_mm() == 1e-3`, `is_coarse()`; Unsicherheit 1e-7 mm → `weld_tol_mm() == 1e-6`, nicht grob; ohne Unsicherheit → `weld_tol_mm() == 1e-3` und `is_coarse()`; ohne Längeneinheit → `length_known == false`.
  (d) `surfaces_match_the_kernel_conventions`: je Fläche ein Punkt aus der STEP-Definition (Handrechnung) gleich `surface.point(u, v)` ±1e-12 und `normal` zeigt wie die STEP-Normale (Zylinder: radial nach außen, Kugel: nach außen, Torus R 20 r 5: `point(0,0)` = (25,0,0)); Torus r ≥ R → genau der Skip-Text; `B_SPLINE_SURFACE_WITH_KNOTS` → „Fläche vom Typ B_SPLINE_SURFACE_WITH_KNOTS wird noch nicht unterstützt.“ (wird in Aufgabe 5 ersetzt); `OFFSET_SURFACE` → entsprechender Text.
  (e) `units_override`: `Units::with_length(Meter)` bei Datei-Unsicherheit 0,001 (Dateieinheit mm) → `uncertainty_mm == Some(1.0)` (P11), `weld_tol_mm() == 1e-3`.
- [ ] **Step 2: Run** `cargo test -p planora-step --test read_geometry -j 2` → FAIL.
- [ ] **Step 3: Implement** `units.rs` und `geom.rs` wie in den Interfaces und Regeln.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: `ref_direction` nicht senkrecht gemacht; Kegelwinkel ohne Winkeleinheit; `CENTI` = 100; `$`-Präfix = 1; Zoll ohne Rekursion auf die Grundeinheit; Unsicherheit nicht in mm; `weld_tol` ohne Untergrenze; Ellipsenachsen getauscht; Torus `>` statt `>=`; Zyklusprüfung entfernt; `VECTOR`-Betrag in die Richtung übernommen.
- [ ] **Step 6: Commit** `feat(step): STEP-Leser – Einheiten und analytische Geometrie`.

---

### Task 3: Topologie, Grundweg (`read/topo.rs`)

**Files:**
- Create: `crates/planora-step/src/read/topo.rs`
- Create (handgeschrieben): `crates/planora-step/tests/data/box_ap214.step` (Quader 10 × 20 × 30, Ecke im Ursprung, Produkt „W\X2\00FC\X0\rfel“, zusammengesetzte Einheiten wie fremde AP214-Dateien, Kommentare, Einträge über mehrere Zeilen, Vorwärtsverweise, zwei Flächen mit `same_sense .F.` und eine Kante mit `EDGE_CURVE … .F.`), `crates/planora-step/tests/data/bore_halves.step` (Zylinder R 20, h 30 um z mit Bohrung r 5; jede Zylinderfläche als **zwei** Halbschalen mit je zwei Geraden und zwei Halbkreisen; Deckel als Kreisringe), `crates/planora-step/tests/data/bore_seam.step` (derselbe Körper mit je **einer** Zylinderfläche mit `SEAM_CURVE` als Naht und `SURFACE_CURVE` an den Kreisen)
- Modify: `crates/planora-step/src/read/mod.rs`
- Test: `crates/planora-step/tests/read_topology.rs`

**Interfaces:**
- Consumes: Aufgabe 2 (`Resolver`, `SurfaceGeom`, `Skip`, `Units`).
- Produces:
```rust
pub const STEP_FACE_ROLE: &str = "STEP-Fläche";             // Rolle "STEP-Fläche {n}", n ab 1 (P7)
#[derive(Clone, Copy, Debug, PartialEq)] pub struct BuildOptions { pub weld_tol_mm: f64 }
#[derive(Clone, Debug)] pub struct BuiltSolid { pub solid: Solid, pub faces: usize, pub warnings: Vec<String> }
/// `items`: Kennungen von MANIFOLD_SOLID_BREP / BREP_WITH_VOIDS (Aufgabe 6) eines Körpers.
pub fn build_solid(file: &StepFile, units: Units, items: &[u64], opts: &BuildOptions, progress: &Progress) -> Result<BuiltSolid, Skip>;
/// Kantenparameter: `t0` am Anfang, `t1` am Ende; periodisch in Laufrichtung (same_sense) entfaltet,
/// volle Kreise ±Periode; t1 < t0 erlaubt.
pub fn edge_params(curve: &Curve, start: DVec3, end: DVec3, same_sense: bool) -> Result<(f64, f64), Skip>;
/// Parameterbereich aus dichten Punktfolgen der Schleifen (je Schleife in Laufrichtung).
pub fn derive_patch(surface: &Surface, loops: &[Vec<DVec3>]) -> Result<UvRect, Skip>;
```
Intern baut `build_solid` zuerst eine Rohform `RawBody` (Ecken, Kanten mit Kurve/t0/t1/Ecken, Flächen mit Fläche/`same_sense`/Schleifen aus `RawHalfEdge { edge: usize, forward: bool }`/Marke, Schalen) und erst daraus das `Solid`; Aufgaben 6 und 7 arbeiten auf dieser Rohform und machen sie öffentlich.

Regeln: Gleiche `VERTEX_POINT`-Kennung → gleiche Kernel-Ecke, gleiche `EDGE_CURVE`-Kennung → gleiche Kernel-Kante (geometrisches Zusammenziehen folgt in Aufgabe 7). Kernel-Schleife = Kanten des `EDGE_LOOP` in Reihenfolge, wenn `FACE_BOUND.orientation` wahr, sonst umgekehrt; `HalfEdge.forward = ORIENTED_EDGE.orientation` (umgekehrte Schleife kippt beides wie `Loop::reversed`); `Face.same_sense = ADVANCED_FACE.same_sense` XOR `flipped`. Äußere Schleife: `FACE_OUTER_BOUND`; sonst bei Ebenen die mit der größten Fläche in der Ebene, bei anderen die erste. Kantenparameter: Gerade `t = (p − origin)·dir`; Kreis/Ellipse `project`, mit `same_sense` wahr `t1 = t0 + rem_euclid(t_end − t0, 2π)`, gleich null bei gleicher Anfangs- und Endecke → `t0 + 2π`; falsch spiegelbildlich. Parameterbereich (nicht für Ebenen, `patch = None`): Punkte jeder Kante (Ecken und 16 Zwischenpunkte) mit `project_near`, periodische Richtungen entlang der Schleife fortlaufend entfaltet (Sprung > halbe Periode → ±Periode); `UvRect` = Hülle; `u0` danach um Vielfache der Periode in `[0, 2π)` geschoben; Breite > Periode + 1e-9 → Skip. Marken nach P7 mit `origin: 0`. Außenrichtung: `tessellate` (Sehne `CLASSIFY_CHORD`) und `mass_properties`; Volumen < 0 → jede Fläche `same_sense` umdrehen und Schleifen `reversed()` (bei mehreren Schalen je Schale, Aufgabe 6). Danach prüft der Leser jede Kante: Mittelpunkt liegt auf beiden Flächen (`implicit` ≤ `max(weld_tol, point_surface_tol)`), sonst Skip „Die Kante bei ({x}; {y}; {z}) liegt nicht auf ihren Flächen.“ (Koordinaten mit Komma, zwei Nachkommastellen); dann `validate` (Fehler → Skip „Der Körper ist ungültig: {Meldung}“). Mehr als `file.limits().max_faces` Flächen → Skip „Der Körper hat mehr als 20 000 Flächen.“ vor dem Aufbau. Fortschritt: je Fläche `advance(1)`, Abbruch → Skip „Abgebrochen“ (die Pipeline in Aufgabe 9 macht daraus `Cancelled`).

- [ ] **Step 1: Write the failing tests** in `tests/read_topology.rs` (Hilfe `read(path) -> BuiltSolid` über `MANIFOLD_SOLID_BREP`-Kennungen der Datei, `Units` aus dem Kontext der Datei):
  (a) `box_reads_with_exact_measures`: `box_ap214.step` → 6 Flächen, 12 Kanten, `validate` Ok, Volumen 6000 ±1e-9, Hülle (0,0,0)–(10,20,30) ±1e-12, Marken `["STEP-Fläche 1", …, "STEP-Fläche 6"]` in Dateireihenfolge, alle `origin == 0`.
  (b) `cylinder_halves_and_seam_give_the_same_body`: `bore_halves.step` (8 Flächen) und `bore_seam.step` (4 Flächen) → je `validate` Ok, Volumen 11 250π mit Sehne 0,001 relativ ±1e-4, Oberfläche relativ ±1e-4 gleich, jede Zylinderfläche hat `patch` mit Breite π bzw. 2π ±1e-12; ein Punkt außen auf jeder Zylinderfläche: Außennormale (`normal · (same_sense ? 1 : −1)`) zeigt vom Material weg (außen radial nach außen, Bohrung zur Achse).
  (c) `edge_params_follow_the_sense`: Halbkreis von (5,0,0) nach (−5,0,0) um z: `same_sense` wahr → (0, π); falsch → (0, −π) bzw. `t1 < t0`; voller Kreis (Anfang = Ende) wahr → (t0, t0 + 2π), falsch → (t0, t0 − 2π); Gerade rückwärts → `t1 < t0`.
  (d) `inside_out_file_is_turned`: `box_ap214.step` mit allen `same_sense` und Schleifenrichtungen gekippt (Textersetzung im Test) → Volumen +6000 (nicht −6000), `validate` Ok.
  (e) `broken_topology_is_skipped_with_reason`: Quader mit fehlender Fläche → `Err(Skip)` mit Text, der mit „Der Körper ist ungültig:“ beginnt; Quader mit einer Ecke um 0,5 mm neben ihren Flächen → „Die Kante bei (… ) liegt nicht auf ihren Flächen.“; `ReadLimits { max_faces: 5 }` → „Der Körper hat mehr als 20 000 Flächen.“ (Text aus der Konstante, nicht aus dem Testlimit).
- [ ] **Step 2: Run** `cargo test -p planora-step --test read_topology -j 2` → FAIL.
- [ ] **Step 3: Implement** `build_solid`, `edge_params`, `derive_patch` in `read/topo.rs`.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: `FACE_BOUND.orientation` ignoriert; `forward` nicht gekippt beim Umdrehen der Schleife; `flipped` ignoriert; voller Kreis ergibt Breite 0; Entfalten ohne Periodensprung; `u0` nicht in `[0, 2π)`; Volumenprobe ohne Umdrehen; Marken ab 0 statt 1; äußere Schleife = erste auch bei Ebenen; Kantenprobe entfernt; Flächengrenze nach statt vor dem Aufbau.
- [ ] **Step 6: Commit** `feat(step): STEP-Leser – Topologie (Grundweg)`.

---

### Task 4: Machbarkeitstor – fremde Körper im Kernel

**Files:**
- Modify: `crates/planora-step/Cargo.toml` (`[dev-dependencies] planora-drawing.workspace = true`), `Cargo.lock` (nur diese Kante)
- Test: `crates/planora-step/tests/gate.rs`

**Interfaces:**
- Consumes: Aufgabe 3 (`build_solid`, Testdaten), `planora_kernel::{AnalyticKernel, Kernel, BoolOp, BlendRequest, BlendEdge, BlendKind}`, `planora_drawing::section::{cut_body, SectionPlane, CutOutcome}`, `planora_drawing::model::ViewKind`, `planora_mesh::{boolean_volume_plausible, mass_properties, tessellate}`.
- Produces: keine Schnittstelle; einen **Torbericht** an den Controller (Tabelle Körper × Schritt → Ergebnis mit Kernel-Meldung).

Ablauf je Körper `S` aus `box_ap214.step`, `bore_halves.step`, `bore_seam.step` (**genaue Fälle**) und deren groben Fassungen (**Messfälle**: im Test erzeugt, jede Ecke um einen festen Versatz bis 9e-4 mm in fester Richtung je Ecke verschoben, Unsicherheit `1.E-3`; Flächen, Kreise und Geraden bleiben):
1. `validate(&S)`; 2. Volumen gegen die Handrechnung (relativ 1e-4, Sehne 0,001); 3. Abziehen eines Quaders 4 × 4 × 100 durch die Mitte des Rings (Quader-Fall: durch die Mitte des Quaders) mit `kernel.boolean(.., BoolOp::Difference)`, `validate` und `boolean_volume_plausible`; 4. Rundung Radius 1 an der oberen äußeren Kreiskante (Quader: Kante oben vorn) mit `kernel.blend`, `validate`; 5. `cut_body(&S, 0, &SectionPlane { kind: ViewKind::Front, offset: Mitte }, &tol)` → `CutOutcome::Cut`, geschnittener Körper `validate` Ok.

- [ ] **Step 1: Write the tests** in `tests/gate.rs`: `exact_bodies_pass_every_step` (alle fünf Schritte für die drei genauen Körper, Behauptung je Schritt mit Namen in der Meldung) und `coarse_bodies_are_measured` (führt dieselben Schritte aus und vergleicht das Ergebnis je Schritt mit einer Tabelle `EXPECTED_COARSE: [(&str, [Outcome; 5])]`, die der Implementierer nach der Messung füllt; `Outcome::Ok` oder `Outcome::Fails`; nie Status Ok mit Volumenabweichung > 1e-3 relativ).
- [ ] **Step 2: Run** `cargo test -p planora-step --test gate -j 2`; Messfälle ausführen und die Tabelle mit den gemessenen Ergebnissen füllen (Kernel-Meldungen in Kommentaren).
- [ ] **Step 3: Torentscheidung:** Scheitert ein **genauer** Fall in einem Schritt: **ANHALTEN** (Status BLOCKED), keine Umgehung im Leser, kein Kernel-Eingriff; Bericht an den Controller mit Körper, Schritt, Kernel-Meldung, vermuteter Ursache und einem Vorschlag für eine Kernel-Änderung (Datei, Funktion, Aufwand). Der Controller legt Dennis die Entscheidung vor. Bestehen alle genauen Fälle: Ergebnisse der Messfälle an den Controller; scheitert ein grober analytischer Fall bei Schritt 3–5, gilt P16 (Nachziehen in Aufgabe 7) als beschlossen und der Controller vermerkt es im Ledger.
- [ ] **Step 4: Mutationsbeweise (≥ 10)** am Leser, die das Tor rot machen müssen, z. B.: Halbschalen-`patch` um π verschoben; `same_sense` der Bohrung nicht gekippt; Kreisparameter ohne Entfaltung; Marken fehlen (`tag: None` → Rundung scheitert); Volumen der Bohrung positiv gerechnet; Schnittebene neben dem Körper (`Kept` statt `Cut`); Abziehen statt Vereinigen bei falscher Normale; Kantenprobe entfernt (grober Fall Ok mit falschem Ergebnis); `validate` nach dem Umdrehen ausgelassen; Versatz 0 statt 9e-4 in den Messfällen.
- [ ] **Step 5: Commit** `test(step): Machbarkeitstor für eingelesene Körper` (Pfade: `crates/planora-step/Cargo.toml`, `Cargo.lock`, `tests/gate.rs`).

---

### Task 5: Freiformkurven und -flächen, Dreh- und Strangflächen (`read/spline.rs`)

**Files:**
- Create: `crates/planora-step/src/read/spline.rs`
- Create (handgeschrieben): `crates/planora-step/tests/data/rational_bspline.step` (Viertelzylinder R 10, Höhe 5, als Körper: Mantel rationale B-Spline-Fläche Grad 2 × 1 mit Gewicht √2/2, Kanten rationale Viertelkreise, dazu eine **ungeklemmte, periodische** kubische B-Spline-Kante `UNIFORM_CURVE` als Deckelrand eines zweiten Körpers „Flansch“), `crates/planora-step/tests/data/revolution_extrusion.step` (Körper „Welle“: Mantel `SURFACE_OF_REVOLUTION` aus einer Geraden parallel zur Achse, R 8, h 20; Körper „Profil“: Prisma, dessen Seitenfläche `SURFACE_OF_LINEAR_EXTRUSION` einer B-Spline-Kurve ist, Höhe 12; Körper „Kegelstumpf“: Drehfläche aus einer schrägen Geraden, die die Achse schneidet)
- Modify: `crates/planora-step/src/read/geom.rs` (B-Spline-, Dreh- und Strangflächen statt Skip), `crates/planora-step/src/read/topo.rs` (`SurfaceGeom::Extrusion` nach den Schleifen fertigstellen)
- Test: `crates/planora-step/tests/read_splines.rs`

**Interfaces:**
- Consumes: Aufgaben 2–3; `planora_math::nurbs::{NurbsCurve, NurbsSurface, MAX_DEGREE, MIN_WEIGHT, MAX_WEIGHT}`; `NurbsSurface::{revolve, extrude}`, `NurbsCurve::elliptic_arc`; `planora_kernel::Surface::nurbs`.
- Produces:
```rust
/// Knoten aus Vielfachheiten und Werten (STEP-Form); Summen > 10 000 oder fallende Werte → Skip.
pub fn expand_knots(mults: &[i64], values: &[f64]) -> Result<Vec<f64>, Skip>;
/// Knoten für UNIFORM/QUASI_UNIFORM/BEZIER/PIECEWISE_BEZIER-Formen (knot_type ohne Liste).
pub fn implicit_knots(kind: &str, degree: usize, count: usize) -> Result<Vec<f64>, Skip>;
/// Klemmt einen Knotenvektor durch Knoteneinfügen (Boehm, A5.1, homogen): Bereich
/// [k[p], k[n]] bleibt, Kurve unverändert, danach geklemmt (je p+1 gleiche Endknoten).
pub fn clamp_knots(degree: usize, knots: &[f64], control: &[DVec4]) -> Result<(Vec<f64>, Vec<DVec4>), Skip>;
/// Teilt alle Gewichte durch das größte; kleinstes danach < MIN_WEIGHT → Skip.
pub fn normalize_weights(w: &mut [f64]) -> Result<(), Skip>;
/// Kurve des Kernels als NURBS über [t0, t1] (Gerade Grad 1, Kreis/Ellipse rational Grad 2).
pub fn curve_to_nurbs(c: &Curve, t0: f64, t1: f64) -> Result<NurbsCurve, Skip>;
/// Strangfläche über den Bereich der Schleifenpunkte entlang `dir` (Rand 1e-6 relativ).
pub fn finish_extrusion(curve: &NurbsCurve, dir: DVec3, loops: &[Vec<DVec3>]) -> Result<Surface, Skip>;
```
Regeln: `B_SPLINE_CURVE_WITH_KNOTS` (auch in zusammengesetzten Einträgen mit `RATIONAL_B_SPLINE_CURVE`), `UNIFORM_CURVE`, `QUASI_UNIFORM_CURVE`, `BEZIER_CURVE`; Flächen entsprechend mit `control_points_list[i][j]` (i in u). Ist der Vektor nicht geklemmt → `clamp_knots` je Reihe/Spalte. Grad > 9, innere Vielfachheit > Grad, Knotenabstand < `MIN_KNOT_GAP_RATIO` · Bereich, Gewicht ≤ 0 → Skip „Eine Freiformfläche lässt sich nicht übernehmen: {Grund}“ (bzw. „…kurve…“). Periodisch (`periodic_u`/`v`) genau dann, wenn erste und letzte Kontrollreihe nach dem Klemmen gleich sind: Abweichung ≤ 1e-9 → periodisch; ≤ `weld_tol` → beide Reihen auf ihren Mittelwert gesetzt, dann periodisch; sonst nicht. Fällt eine Spalte zusammen (Pol in u-Richtung, wird vom Kernel abgelehnt): u und v tauschen und `flipped` umkehren; Pol in beiden Richtungen oder im Inneren → Skip mit der Kernel-Meldung. `SURFACE_OF_REVOLUTION` und `SURFACE_OF_LINEAR_EXTRUSION` nach P8; die STEP-Normale wird aus der Definition (`σ(u,v)`, Ableitungen per zentraler Differenz mit h = 1e-6 · Bereich) an einem Probepunkt bestimmt und mit `Surface::normal` verglichen (`flipped = dot < 0`); schräge Gerade, die die Achse nicht schneidet und nicht parallel ist → Skip „Eine Drehfläche aus einer schrägen Geraden wird noch nicht unterstützt.“. `SurfaceGeom::Extrusion` wird in `topo.rs` nach dem Sammeln der Schleifenpunkte mit `finish_extrusion` fertig.

- [ ] **Step 1: Write the failing tests** in `tests/read_splines.rs`:
  (a) `clamping_keeps_the_curve`: kubische gleichförmige Kurve mit 7 Kontrollpunkten, Knoten 0..=10 (ungeklemmt) → `clamp_knots` → geklemmt, `KnotVector::new` nimmt ihn an, an 50 Stellen im Bereich [3, 7] gleicher Punkt wie die De-Boor-Auswertung des Originals (eigene kleine Auswertung im Test) ±1e-12; rationaler Fall mit Gewichten (1, 2, 1, 2, …) ebenso.
  (b) `knot_forms`: `expand_knots(&[3,1,3], &[0.,0.5,1.])` = `[0,0,0,0.5,1,1,1]`; `implicit_knots("QUASI_UNIFORM_KNOTS", 2, 4)` = `[0,0,0,1,2,2,2]`; `"BEZIER_KNOTS"` mit Grad 3, 4 Punkte = `[0,0,0,0,1,1,1,1]`; fallende Werte → Skip.
  (c) `rational_circle_and_cylinder`: `rational_bspline.step` → Viertelzylinder: Volumen π·100/4·5 relativ ±1e-6 (Sehne 0,001), jeder Kontrollrand-Punkt der Kante liegt auf dem Kreis ±1e-12, `normalize_weights` hat das größte Gewicht 1; Körper „Flansch“ liest (ungeklemmte periodische Kante) und `validate` Ok.
  (d) `revolution_becomes_analytic_where_exact`: „Welle“ → Mantel ist `Surface::Cylinder` mit Radius 8 ±1e-12, Volumen π·64·20 relativ ±1e-6; „Kegelstumpf“ → `Surface::Cone`; Außennormalen zeigen nach außen (Probepunkt + 0,1 · Außennormale liegt außerhalb: Umlaufzahl über das Netz < 0,5).
  (e) `extrusion_of_a_spline_is_bounded_by_its_loops`: „Profil“ → Seitenfläche `Surface::Nurbs`, `patch.v` deckt 0..12 (in den Parametern der Fläche) und das Volumen gleich Grundfläche × 12 relativ ±1e-6 (Grundfläche per Simpson wie `freeform.rs::bulge_area`).
  (f) `flipped_is_measured_not_assumed`: dieselbe „Welle“ mit umgekehrter Achsrichtung im Text (`DIRECTION('',(0.,0.,-1.))`) → gleiches Volumen (positiv), gleiche Außennormalen.
  (g) `unsupported_spline_cases_are_named`: Grad 10 → Skip mit „Freiformfläche“; Gewicht 0 → Skip; Pol in u-Richtung (Spalte zusammengefallen, sonst gültig) → gelesen mit getauschten Richtungen und Außennormale nach außen.
- [ ] **Step 2: Run** `cargo test -p planora-step --test read_splines -j 2` → FAIL.
- [ ] **Step 3: Implement** `spline.rs` und die Verzweigungen in `geom.rs`/`topo.rs` (Boehm-Einfügen auf rohen Feldern, weil `KnotVector::new` ungeklemmte Vektoren ablehnt).
- [ ] **Step 4: Run** → PASS; `cargo test -p planora-step --test read_topology --test gate -j 2` bleibt grün.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Einfügen nur am Anfang; Kontrollpunkte nicht homogen eingefügt; Gewichte nicht normiert; Zeilen-/Spaltenordnung der Kontrollpunkte vertauscht; Periodizität aus dem STEP-Kennzeichen statt aus den Reihen; Mittelung bei Abweichung > `weld_tol`; Tausch u/v ohne `flipped`; `flipped` fest auf `false`; Strangbereich nur von der Kurve aus nach vorn; Kegel statt Zylinder bei paralleler Geraden; BEZIER-Knoten falsch.
- [ ] **Step 6: Commit** `feat(step): STEP-Leser – Freiform-, Dreh- und Strangflächen`.

---

### Task 6: Nähte, Pole, Entartungen und Hohlräume (`read/seam.rs`)

**Files:**
- Create: `crates/planora-step/src/read/seam.rs`
- Create (handgeschrieben): `crates/planora-step/tests/data/sphere_vertex_loop.step` (Kugel r 10 als eine Fläche mit Naht hin und zurück und `VERTEX_LOOP` an beiden Polen; zweiter Körper: Halbkugel auf Zylinder, am Pol eine Kante der Länge null), `crates/planora-step/tests/data/seamless.step` (Zylinder R 5, h 10 als eine Fläche mit zwei vollen Kreisen ohne Naht; deren Ecken auf verschiedenen Winkeln; Kugel ohne Kanten nur mit `VERTEX_LOOP`s), `crates/planora-step/tests/data/void_box.step` (Quader 20³ mit kugelförmigem Hohlraum r 5 als `BREP_WITH_VOIDS` mit `ORIENTED_CLOSED_SHELL(…,.F.)`; zweiter Körper: Darstellung mit zwei getrennten Quadern als zwei `MANIFOLD_SOLID_BREP`)
- Modify: `crates/planora-step/src/read/topo.rs` (Aufrufe der Bereinigung vor dem Bauen der Schleifen, Hohlräume und mehrere Schalen)
- Test: `crates/planora-step/tests/read_seams.rs`

**Interfaces:**
- Consumes: Aufgaben 3, 5.
- Produces (innerhalb von `topo.rs` benutzt, für Tests öffentlich):
```rust
/// Kanten kürzer als `tol` (Kurvenlänge über 16 Abschnitte) aus einer Schleife entfernen; VERTEX_LOOP-Schranken verwerfen.
pub fn drop_degenerate(loop_edges: &mut Vec<RawHalfEdge>, tol: f64);
/// Fügt Zylinder/Kegel/Kugel-Flächen ohne Naht eine Naht hinzu (P9); teilt dazu Kreiskanten, die
/// alle benutzenden Flächen gemeinsam ändern.
pub fn insert_seams(raw: &mut RawBody, tol: f64) -> Result<(), Skip>;
pub struct RawHalfEdge { pub edge: usize, pub forward: bool }   // Index in RawBody.edges
pub struct RawBody { /* Ecken, Kanten (Kurve, t0, t1, Ecken), Flächen (Fläche, same_sense, Schleifen, Marke), Schalen */ }
```
Regeln: `VERTEX_LOOP` wird verworfen; bleibt danach keine Schleife (Kugel nur mit Polen), fügt `insert_seams` die Naht ein. Naht: Zylinder/Kegel – Gerade entlang der Mantellinie beim Winkel `u_s` der Ecke der ersten Kreisschleife; hat die andere Kreisschleife dort keine Ecke, wird ihre Kante bei `u_s` geteilt (neue Ecke, in **allen** Schleifen, die sie benutzen, durch zwei Halbkanten ersetzt); Kugel – Halbkreis im Meridian `u_s` von Pol zu Pol (`Curve::Circle` wie `revolve.rs` den Meridian baut), Pole als Ecken. Die neue Schleife hat die Kernel-Form `[Kreis unten, Naht hoch, Kreis oben rückwärts, Naht runter]` bzw. `[Naht, Naht rückwärts]`. Andere Flächenarten ohne Naht → Skip nach P9. Hohlräume: `ORIENTED_CLOSED_SHELL(.F.)` → Flächen der Schale umgekehrt; jede Schale bekommt einen `Shell`-Eintrag; Volumenvorzeichen je Schale: äußere Schalen positiv, Hohlräume negativ, sonst die Schale umdrehen. Mehrere `MANIFOLD_SOLID_BREP` eines Körpers → mehrere äußere Schalen in einem `Solid`.

- [ ] **Step 1: Write the failing tests** in `tests/read_seams.rs`:
  (a) `sphere_with_vertex_loops`: Kugel → 1 Fläche, Schleife `[Naht, Naht rückwärts]`, 2 Ecken (Pole), `patch.v` = [−π/2, π/2] ±1e-12, Volumen 4/3·π·1000 relativ ±1e-4, `validate` Ok; Halbkugel-auf-Zylinder mit Nulllängenkante → gelesen, Kante entfernt, `validate` Ok.
  (b) `seams_are_inserted`: nahtloser Zylinder → Mantel mit einer Naht-Geraden der Länge 10 ±1e-12, die obere Kreiskante wurde geteilt (Kantenzahl 4 + 1 = 5: zwei Kreise, eine geteilte Kreishälfte, Naht), Deckel-Schleife enthält dieselben Teilkanten; Volumen π·25·10 relativ ±1e-4; Kugel nur mit `VERTEX_LOOP`s → Naht eingefügt, `validate` Ok.
  (c) `unsupported_seamless_torus_is_skipped`: Torusfläche ohne Kanten → Skip „Fläche vom Typ Torus ohne Naht wird noch nicht unterstützt.“.
  (d) `voids_and_lumps`: `void_box.step` → 2 Schalen, Volumen 8000 − 4/3·π·125 relativ ±1e-4, Außennormale am Hohlraum zeigt zur Kugelmitte; Darstellung mit zwei Quadern → ein `Solid` mit 2 Schalen, Volumen Summe; beide Körper `validate` Ok.
  (e) `void_written_without_flip_is_turned`: `void_box.step` mit `ORIENTED_CLOSED_SHELL(…,.T.)` (falsch geschrieben) → gleiches Volumen (Schale nach Vorzeichen umgedreht).
- [ ] **Step 2: Run** `cargo test -p planora-step --test read_seams -j 2` → FAIL.
- [ ] **Step 3: Implement** `seam.rs`, Anpassungen in `topo.rs`.
- [ ] **Step 4: Run** → PASS; `read_topology`, `read_splines`, `gate` grün.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Kante nur in einer Schleife geteilt; Naht beim Winkel 0 statt `u_s`; Nulllänge über Eckengleichheit statt Kurvenlänge (voller Kreis verschwindet); `VERTEX_LOOP` als leere Schleife behalten; Kugelnaht als Viertelkreis; `.F.` am Hohlraum ignoriert und kein Vorzeichen-Umdrehen; alle Schalen gemeinsam umgedreht; Pole nicht als Ecken; Torus ohne Naht still übernommen; zwei Stücke als zwei Körper.
- [ ] **Step 6: Commit** `feat(step): STEP-Leser – Nähte, Pole und Hohlräume`.

---

### Task 7: Zusammenziehen und Paaren mit Rastersuche (`read/weld.rs`)

**Files:**
- Create: `crates/planora-step/src/read/weld.rs`
- Create (handgeschrieben): `crates/planora-step/tests/data/split_vertices.step` (Quader 10 × 20 × 30, jede Fläche mit **eigenen** `VERTEX_POINT`- und `EDGE_CURVE`-Einträgen, Koordinaten genau)
- Modify: `crates/planora-step/src/read/topo.rs` (Ecken und Kanten vor dem Aufbau zusammenziehen)
- Test: `crates/planora-step/tests/read_weld.rs`

**Interfaces:**
- Consumes: Aufgaben 3, 6; `Units::{weld_tol_mm, is_coarse}`.
- Produces:
```rust
/// Raster mit Zellgröße `cell`; Nachbarsuche in 27 Zellen.
pub struct HashGrid { /* … */ }
impl HashGrid { pub fn new(cell: f64) -> HashGrid; pub fn insert(&mut self, p: DVec3, index: usize); pub fn near(&self, p: DVec3, tol: f64) -> Vec<usize>; }
/// Je Punkt der Vertreter (Index des ersten Punkts innerhalb `tol` in Eingabereihenfolge; nicht transitiv).
pub fn weld_points(points: &[DVec3], tol: f64) -> Vec<usize>;
/// Warnungstext für grobe Dateien.
pub fn coarse_warning(tol_mm: f64) -> String;   // "Genauigkeit der Datei {tol} mm: Rechnen mit diesem Körper kann scheitern."
```
Regeln: Ecken mit `weld_points(·, weld_tol)` (Vertreter-Punkt bleibt, keine Mittelung); `EDGE_CURVE`s mit gleichem Eckenpaar (beide Richtungen) und Mittel- und Viertelpunkten ≤ `weld_tol` auseinander → eine Kernel-Kante, Richtung der zweiten aus dem Vergleich der Tangenten; Kurve und Parameter der ersten. Danach muss jede Kante genau zweimal benutzt werden; sonst Skip: zur nächstliegenden anderen offenen Kante (Abstand der Mittelpunkte) „Lücke von {d} mm an der Kante bei ({x}; {y}; {z}).“ (d mit Komma, ohne überflüssige Nullen, höchstens 4 gültige Stellen), ohne Partner „Die Kante bei ({x}; {y}; {z}) gehört nur zu einer Fläche.“ Kein Lückenschließen. `Units::is_coarse()` → `warnings.push(coarse_warning(weld_tol))` (Zahl mit Komma: `0,001`). Komplexität: Ecken und Kanten über `HashGrid`, kein O(n²) (Test (f)).

- [ ] **Step 1: Write the failing tests** in `tests/read_weld.rs`:
  (a) `grid_finds_neighbours_only`: 1000 Punkte im Abstand 1, ein Punkt 0,4e-3 neben Punkt 17 → `weld_points(·, 1e-3)[1000] == 17`, alle anderen auf sich selbst; Punkt an der Zellgrenze (Abstand 0,9e-3 über die Grenze) gefunden; Kette a–b–c mit je 0,8e-3 → b → a, c → c (nicht transitiv: c ist 1,6e-3 von a).
  (b) `split_vertices_box_is_welded`: `split_vertices.step` → 8 Ecken, 12 Kanten, `validate` Ok, Volumen 6000 ±1e-9.
  (c) `coarse_box_is_taken_with_warning`: `split_vertices.step` mit Ecken je Fläche um bis zu 9e-4 versetzt und Unsicherheit `1.E-3` → gelesen, `warnings == ["Genauigkeit der Datei 0,001 mm: Rechnen mit diesem Körper kann scheitern."]`.
  (d) `gap_above_tolerance_is_named`: eine Ecke einer Fläche um 2e-3 versetzt (Unsicherheit 1e-3) → Skip „Lücke von 0,002 mm an der Kante bei (…).“ (Abstand gerundet wie festgelegt).
  (e) `missing_face_is_named`: Quader ohne Deckel → Skip, Text endet mit „gehört nur zu einer Fläche.“.
  (f) `welding_scales_linearly`: Körper mit 10 000 Ecken (Treppe aus 2500 Stufen, im Test als STEP-Text erzeugt, jede Fläche mit eigenen Ecken) → Zusammenziehen unter 2 s im Debug-Build (Zeitmessung um `weld_points` und das Paaren).
- [ ] **Step 2: Run** `cargo test -p planora-step --test read_weld -j 2` → FAIL.
- [ ] **Step 3: Implement** `weld.rs` und den Einbau in `topo.rs`.
- [ ] **Step 4 (bedingt, P16): Nachziehen der Ecken** – nur wenn der Controller es nach Aufgabe 4 beschlossen hat: `pub fn tighten_vertices(raw: &mut RawBody, tol: f64)` (Gauß-Newton auf den `implicit`-Funktionen der analytischen Flächen einer Ecke mit mindestens drei Flächen; angenommen nur bei Verschiebung ≤ `tol` und größtem Rest ≤ 1e-9 · (1 + |p|); Geraden werden durch die neuen Ecken neu gelegt, wenn sich ihre Richtung um ≤ `tol`/Länge ändert); Test `coarse_box_after_tightening_passes_the_gate_steps`: der grobe Quader aus (c) besteht Schritte 1–5 des Tors. Scheitert auch das: **anhalten**, Bericht mit Kernel-Vorschlag an den Controller.
- [ ] **Step 5: Run** → PASS; `gate`, `read_*` grün.
- [ ] **Step 6: Mutationsbeweise (≥ 10)**, z. B.: Nachbarsuche nur in der eigenen Zelle; Zellgröße kleiner als `tol`; transitive Verkettung; Mittelung statt Vertreter; Paaren nur nach Ecken (zwei verschiedene Bögen zwischen denselben Ecken verschmolzen); Richtung der zweiten Kante nicht geprüft; Lücke als Abstand der Ecken statt der Mittelpunkte; Warnung bei feinen Dateien; `weld_tol` ohne Obergrenze (2e-3-Lücke geschlossen); quadratische Suche (Test f rot).
- [ ] **Step 7: Commit** `feat(step): STEP-Leser – Zusammenziehen und Paaren`.

---

### Task 8: Körper, Namen und Baugruppen-Lagen (`read/assembly.rs`)

**Files:**
- Create: `crates/planora-step/src/read/assembly.rs`
- Create (handgeschrieben): `crates/planora-step/tests/data/inch_assembly.step` (AP214, Längeneinheit Zoll über `CONVERSION_BASED_UNIT`, Winkel in Grad; Teil „Block“ 1 × 2 × 3 in, Teil „Stift“ Zylinder r 0,25 in, h 2 in; Baugruppe „Gestell“ mit Block an der Einheitslage, Block ein zweites Mal um 90° um z gedreht und um (4 in, 0, 0) verschoben, Stift bei (0, 0, 3 in); Lagen über `NEXT_ASSEMBLY_USAGE_OCCURRENCE` + `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` + `(REPRESENTATION_RELATIONSHIP() REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION() SHAPE_REPRESENTATION_RELATIONSHIP())` mit `ITEM_DEFINED_TRANSFORMATION`; beim Stift `rep_1`/`rep_2` absichtlich vertauscht), `crates/planora-step/tests/data/mapped_item.step` (Teil über `MAPPED_ITEM`/`REPRESENTATION_MAP` um (0, 0, 50) mm versetzt; zweites Vorkommen mit `CARTESIAN_TRANSFORMATION_OPERATOR_3D` Maßstab −1)
- Modify: `crates/planora-step/src/read/mod.rs`
- Test: `crates/planora-step/tests/read_assembly.rs`

**Interfaces:**
- Consumes: Aufgaben 1–2.
- Produces:
```rust
pub const MAX_ASSEMBLY_DEPTH: usize = 32;
pub const MAX_BODY_NAME_CHARS: usize = 64;
#[derive(Clone, Debug, PartialEq)]
pub struct FoundBody {
    pub name: String,            // nach P10 gekürzt und eindeutig; leer, wenn die Datei keinen Namen hat
    pub items: Vec<u64>,         // MANIFOLD_SOLID_BREP / BREP_WITH_VOIDS
    pub context: Option<u64>,    // GEOMETRIC_REPRESENTATION_CONTEXT der Darstellung
    pub placement: DMat4,        // Ähnlichkeit in mm (Einheitslage bei `placement_note`)
    pub placement_note: Option<String>,
}
/// Alle Körper (Vorkommen) der Datei; `units_of(context)` liefert die (ggf. umgestellten) Einheiten je Kontext.
pub fn find_bodies(file: &StepFile, units_of: &dyn Fn(Option<u64>) -> Result<Units, StepReadError>) -> Result<Vec<FoundBody>, StepReadError>;
pub fn placement_note(name: &str, reason: &str) -> String;
// "Die Lage von „{name}“ ließ sich nicht bestimmen ({reason}); der Körper liegt am Ursprung."
```
Regeln: P2, P3, P10. Gründe (wörtlich): „Spiegelung“, „Skalierung“, „Verweise im Kreis“, „zu tief verschachtelt“, „Richtung unklar“, „unbekannte Art {TYP}“. Verschiebungen jeder Lage in den Einheiten der Darstellung, zu der die Lage gehört. Mehr als `limits.max_bodies` Vorkommen → `TooManyBodies`; keine Volumen-Einträge → `NoBodies`. Reihenfolge: Tiefensuche ab den Wurzeln in Dateireihenfolge, dann nicht erreichte Darstellungen, dann freie Volumen-Einträge.

- [ ] **Step 1: Write the failing tests** in `tests/read_assembly.rs`:
  (a) `assembly_places_each_occurrence`: `inch_assembly.step` → 3 Körper, Namen `["Block", "Block (2)", "Stift"]`; Hülle nach Bau und Lage (Aufgaben 3–7 benutzt): Block (0,0,0)–(25,4; 50,8; 76,2) ±1e-9; „Block (2)“ x ∈ [50,8; 101,6], y ∈ [0; 25,4] ±1e-9; Stift z ∈ [76,2; 127] ±1e-9 (vertauschte Richtung erkannt).
  (b) `mapped_items_and_bad_transforms`: `mapped_item.step` → erstes Vorkommen um 50 mm in z verschoben; zweites `placement_note == Some("Die Lage von „Teil (2)“ ließ sich nicht bestimmen (Spiegelung); der Körper liegt am Ursprung.")` und `placement == DMat4::IDENTITY`.
  (c) `cycles_and_depth`: Baugruppe A enthält B, B enthält A (im Test erzeugt) → Körper mit Grund „Verweise im Kreis“ am Ursprung, kein Absturz; Kette mit 33 Ebenen → „zu tief verschachtelt“.
  (d) `names`: Name mit 80 Zeichen → 64 Zeichen; Steuerzeichen → Leerzeichen; ohne Produkt → Name der Darstellung; ganz ohne Namen → `""`.
  (e) `limits`: 51 Vorkommen eines Teils → `Err(TooManyBodies)` mit „Die Datei enthält mehr als 50 Körper.“; Datei nur mit Kontext → `NoBodies`; plain Teil ohne Baugruppe (`box_ap214.step`) → 1 Körper „Würfel“, Einheitslage.
- [ ] **Step 2: Run** `cargo test -p planora-step --test read_assembly -j 2` → FAIL.
- [ ] **Step 3: Implement** `assembly.rs`.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: `T = A2·A1` statt `A2·A1⁻¹`; Richtung immer `rep_1` = Kind; Lage der Kinder nicht bis zur Wurzel multipliziert (verschachtelte Baugruppe); Verschiebung in Dateieinheiten statt mm; Maßstab −1 als Drehung; zweites Vorkommen verworfen; Doppelname ohne „(2)“; Kürzen nach Bytes statt Zeichen; Zyklus ohne Stapel (Endlosschleife – Test mit Zeitgrenze); Grenze 50 auf Teile statt Vorkommen.
- [ ] **Step 6: Commit** `feat(step): STEP-Leser – Körper, Namen und Baugruppen-Lagen`.

---

### Task 9: Import-Pipeline und Normalisieren (`read/import.rs`)

**Files:**
- Create: `crates/planora-step/src/read/import.rs`
- Modify: `crates/planora-step/src/read/mod.rs`, `crates/planora-step/src/lib.rs` (Re-Exporte)
- Test: `crates/planora-step/tests/read_import.rs`

**Interfaces:**
- Consumes: Aufgaben 1–8; `write_step`, `StepOptions`, `StepBody` (Schreiber); `Solid::transformed`.
- Produces (Re-Export in `lib.rs` aus `read::import`, `read::units::LengthUnit`, `read::units::{COARSE_TOL_MM, DEFAULT_FILE_TOL_MM}`):
```rust
pub const NORMAL_FILE_NAME: &str = "planora-step-koerper.step";
#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct ImportOptions { pub unit_override: Option<LengthUnit> }
#[derive(Clone, Debug)] pub struct NormalBody { pub text: String, pub solid: Solid }
#[derive(Clone, Debug)]
pub struct ImportedBody {
    pub name: String,
    pub faces: usize,
    pub bounds: Option<(DVec3, DVec3)>,      // mm, nach der Lage
    pub tolerance_mm: f64,                   // weld_tol (P4)
    pub warnings: Vec<String>,               // grobe Genauigkeit, Lage-Meldung
    pub outcome: Result<NormalBody, String>, // Err: Grund des Auslassens
}
#[derive(Clone, Debug)]
pub struct StepImport { pub header: StepHeader, pub unit: LengthUnit, pub unit_known: bool, pub bodies: Vec<ImportedBody> }
pub fn import_bodies(file: &StepFile, opts: &ImportOptions, progress: &Progress) -> Result<StepImport, StepReadError>;
pub fn normalize(solid: &Solid, name: &str) -> Result<NormalBody, String>;          // P5
pub fn read_normal_body(text: &str) -> Result<Solid, String>;                        // genau ein Körper, Einheitslage, Marken P7
pub fn check_normal_header(text: &str) -> Result<(), String>;                        // billig: ASCII, Anfang "ISO-10303-21;", Ende "END-ISO-10303-21;", Länge ≤ MAX_STEP_TEXT_BYTES
pub const MAX_STEP_TEXT_BYTES: usize = 64 * 1024 * 1024;
```
Ablauf je Körper: Einheiten (`context_units`, ggf. `with_length(override)`), `build_solid`, Lage anwenden (`transformed`), `normalize`; `outcome` Err mit dem Skip-Grund bzw. Schreibergrund; `Skip("Abgebrochen")` oder `progress.is_cancelled()` → ganzer Aufruf `Err(Cancelled)`. Fortschritt nach P15: `set_total(Summe der Flächen aller Körper)` (Zählung der `ADVANCED_FACE` je Körper vorab über die Kennungen). `read_normal_body` nimmt nur Text an, dessen Körperzahl genau 1 ist und dessen Einheit Millimeter ist; sonst Err.

- [ ] **Step 1: Write the failing tests** in `tests/read_import.rs`:
  (a) `session_equals_reload`: für `box_ap214.step`, `bore_halves.step`, `inch_assembly.step` je Körper: `read_normal_body(&n.text)` ergibt dieselben Flächenzahlen, Marken (Liste der Rollen), Eckpunkte (bitgleich) und Kantenparameter wie `n.solid`; `normalize(&n.solid, name).text == n.text` (Normalisieren ist ruhend).
  (b) `normal_text_is_fixed`: Kopf enthält `'1970-01-01T00:00:00'` und `'planora-step-koerper.step'`; zwei Läufe auf derselben Datei ergeben denselben Text.
  (c) `bad_bodies_are_left_out_others_kept`: Datei mit drei Körpern (Quader, Quader ohne Deckel, Torus r ≥ R) → `bodies.len() == 3`, Ergebnisse `[Ok, Err("… gehört nur zu einer Fläche."), Err("Eine Torusfläche mit kleinem Radius größer oder gleich dem großen wird noch nicht unterstützt.")]`.
  (d) `units_override_scales_everything`: `inch_assembly.step` mit `unit_override: Some(Millimeter)` → Block-Hülle (0,0,0)–(1,2,3) ±1e-12, „Block (2)“ x ∈ [2; 4]; `unit == Millimeter`, `tolerance_mm` umgerechnet.
  (e) `cancel_and_progress`: vorher abgebrochener `Progress` → `Err(Cancelled)`; ohne Abbruch `fraction() == 1.0`; Datei mit 20 001 Flächen in einem Körper (`ReadLimits { max_faces: 20_000 }`, synthetisch: Treppe) → dieser Körper Err „Der Körper hat mehr als 20 000 Flächen.“, Fortschritt zählt ihn nicht hoch.
  (f) `header_check_is_cheap_and_strict`: `check_normal_header` lehnt „ISO-10303-21;“ ohne Ende, Nicht-ASCII und > 64 MiB ab, nimmt jeden `normalize`-Text an; `read_normal_body` auf `inch_assembly.step` (Zoll, 3 Körper) → Err.
- [ ] **Step 2: Run** `cargo test -p planora-step --test read_import -j 2` → FAIL.
- [ ] **Step 3: Implement** `import.rs`.
- [ ] **Step 4: Run** → PASS; ganze Crate `cargo test -p planora-step -j 2` grün.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Körper aus der Datei statt aus dem normalisierten Text benutzt (Test a: Kegel/Ellipse weichen ab); Zeitstempel `SystemTime::now()`; Lage nach statt vor dem Normalisieren (Text ohne Lage); ein fehlerhafter Körper bricht alle ab; Abbruch wird `Err` eines einzelnen Körpers; Override ohne Lagen; `read_normal_body` nimmt mehrere Körper; Kopfprüfung liest den ganzen Körper; Fortschritt ohne Gesamtzahl; Flächengrenze ignoriert.
- [ ] **Step 6: Commit** `feat(step): STEP-Import – Pipeline und Normalisieren`.

---

### Task 10: Round-Trip mit allen eigenen Körpern, Handproben und feindliche Dateien

**Files:**
- Create: `crates/planora-step/tests/common/bodies.rs` (verschoben, Rümpfe unverändert: `torus_ring`, `tube`, `notched` aus `tests/curved.rs`; `bulge`, `spline_prism`, `meridian`, `vase`, `nurbs_sphere`, `weighted_prism` samt Hilfen aus `tests/freeform.rs`); Modify: `tests/common/mod.rs` (`pub mod bodies;`), `tests/curved.rs`, `tests/freeform.rs` (benutzen `common::bodies`)
- Test: `crates/planora-step/tests/roundtrip.rs`, `crates/planora-step/tests/read_hostile.rs`

**Interfaces:**
- Consumes: Aufgabe 9 (`import_bodies`, `normalize`, `read_normal_body`), `common::{export, options, Lcg}`.
- Produces: keine.

- [ ] **Step 1: Write the tests:**
  (a) `roundtrip.rs::every_exportable_body_comes_back` – Körper: Quader, Zylinder, Kegel, Quader mit Bohrung (Abziehen), Quader mit Rundung r 2 an einer Kante, Quader mit Fase 1, Kugel, Torus (`torus_ring`), `vase`, `spline_prism`, `weighted_prism`, `nurbs_sphere`, Vereinigung zweier Zylinder (Freiform-Schnittkurve wie in `stage3.rs`), Körper aus zwei Stücken; je Lage aus `curved.rs::poses()`. `common::export` → `import_bodies` → genau ein Körper Ok; gleiche Flächenzahl wie `validate(original).faces`, Volumen und Oberfläche (Sehne 0,01, beide über `tessellate`) relativ ±1e-6, Hülle ±1e-9 (mit Freiform ±`NURBS_EDGE_TOL`).
  (b) `roundtrip.rs::units_mm_m_inch`: Quader-Text mit `SI_UNIT(.MILLI.,.METRE.)` ersetzt durch `SI_UNIT($,.METRE.)` → Hülle ×1000; durch eine Zoll-`CONVERSION_BASED_UNIT` → ×25,4; Override zurück auf Millimeter → ×1.
  (c) `roundtrip.rs::hand_samples_have_their_values` (Tabelle Datei → Körperzahl, Namen, Flächen, Volumen relativ ±1e-4): `box_ap214` (1, „Würfel“, 6, 6000), `bore_halves` (1, 8, 11 250π), `bore_seam` (1, 4, 11 250π), `rational_bspline`, `revolution_extrusion`, `sphere_vertex_loop`, `seamless`, `void_box`, `split_vertices`, `inch_assembly` (3), `mapped_item`; jeder übernommene Körper danach mit Quader 4×4×100 abgezogen: Ok und `boolean_volume_plausible`.
  (d) `roundtrip.rs::after_blends_and_booleans` – die Körper aus `stage3.rs`-Art (Rundung + Boolesche Operation + Spiegelung) einmal exportiert, importiert, noch einmal gerundet (r 0,5 an einer geraden Kante) → Ok.
  (e) `read_hostile.rs::never_panics`: für jede Datei unter `tests/data` (i) bei jedem 97. Byte abgeschnitten, (ii) 200 Fälle (Debug) bzw. 5000 (Release, `#[ignore]`) mit `Lcg(20_261_021)`: ein zufälliges Byte durch eines aus `"#();,'.$*=0123456789E-"` ersetzt, ein Bereich doppelt eingefügt oder ein Verweis auf `#999999` gesetzt → `StepFile::parse` und `import_bodies` liefern `Ok` oder `Err`, nie Panik (`std::panic::catch_unwind` um jeden Fall), und jede Meldung ist nicht leer und enthält kein „panicked“/„unwrap“; Laufzeit je Fall < 2 s.
  (f) `read_hostile.rs::big_file_release_only` (`#[ignore]`): Treppe mit 10 000 Flächen in einem Körper und eine Datei mit 50 Quadern → `import_bodies` im Release-Build unter 20 s bzw. 5 s; gemessene Zeiten in den Bericht.
- [ ] **Step 2: Run** `cargo test -p planora-step --test roundtrip --test read_hostile --test curved --test freeform -j 2` → PASS (Fehler hier sind Befunde in den Aufgaben 2–9: dort beheben, nicht im Test lockern; jede Abweichung an den Controller).
- [ ] **Step 3: Run** `cargo test -p planora-step --release --test read_hostile -- --ignored` und Zeiten berichten.
- [ ] **Step 4: Mutationsbeweise (≥ 10)** in den Lesemodulen (je ein roter Test aus dieser Aufgabe), z. B.: Kegel ohne Verschiebung der Lage gelesen; Ellipsenachse falsch; Gewicht 1 statt Datei; `same_sense` der zweiten Schale; Zoll = 25; Hülle ohne Lage; Abschneiden ohne `Truncated` (Panik im Zerleger); Verweis auf fehlenden Eintrag mit `unwrap`; Lcg-Fälle mit Endlosschleife (Zeitgrenze); Torus-Probe ohne Normalisieren.
- [ ] **Step 5: Commit** `test(step): Round-Trip, Handproben und feindliche Dateien`.

---

### Task 11: STEP-Körper im Dokument (`planora-feature`)

**Files:**
- Create: `crates/planora-feature/src/step_bodies.rs`
- Modify: `crates/planora-feature/Cargo.toml` (`planora-step.workspace = true`), `Cargo.lock`; `crates/planora-feature/src/{lib.rs, model.rs, compute.rs, error.rs, formulas.rs, ops.rs, snapshot.rs}` (neue Art an allen `match`-Stellen; der Compiler nennt sie; heute 14 Stellen mit `MeshBody` in 5 Crates, die übrigen folgen in Aufgaben 12–13)
- Test: `crates/planora-feature/tests/step_bodies.rs`

**Interfaces:**
- Consumes: Aufgabe 9 (`read_normal_body`, `check_normal_header`, `MAX_STEP_TEXT_BYTES`, `COARSE_TOL_MM`).
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)] pub struct StepShapeId(pub u64);
pub struct StepShape { /* text, tolerance_mm, solid: OnceLock<Result<Solid, String>> */ }
impl StepShape {
    pub fn new(text: String, tolerance_mm: f64) -> Result<StepShape, String>;   // check_normal_header, tolerance ∈ [1e-6, 1e-3]; baut nicht
    pub fn with_solid(text: String, tolerance_mm: f64, solid: Solid) -> StepShape; // aus dem Import (schon gebaut)
    pub fn text(&self) -> &str;
    pub fn tolerance_mm(&self) -> f64;
    pub fn is_coarse(&self) -> bool;                 // tolerance_mm > COARSE_TOL_MM
    pub fn solid(&self) -> Result<&Solid, &str>;     // baut beim ersten Aufruf (read_normal_body), zählt step_builds()
}
impl std::fmt::Debug for StepShape { /* Länge des Texts, Toleranz */ }
#[doc(hidden)] pub fn step_builds() -> u64;          // Zähler je Thread wie mesh_validations
// model.rs
FeatureKind::StepBody { shape: StepShapeId }        // label() "STEP-Körper"; dependencies() leer
Document { step_shapes: BTreeMap<StepShapeId, Arc<StepShape>>, next_step_shape: u64 /* ab 1 */ }
#[derive(Clone, Debug)] pub struct NewStepBody { pub name: String, pub shape: Arc<StepShape> }
impl Document {
    pub fn add_step_bodies(&mut self, bodies: Vec<NewStepBody>) -> Result<Vec<FeatureId>, FeatureError>;  // ganz oder gar nicht
    pub fn step_shape(&self, id: StepShapeId) -> Option<&Arc<StepShape>>;
    pub fn step_body(&self, feature: FeatureId) -> Option<&Arc<StepShape>>;
}
// snapshot.rs
Snapshot { …, pub step_shapes: BTreeMap<StepShapeId, Arc<StepShape>> }   // nur benutzte; same_model vergleicht mit Arc::ptr_eq
// error.rs
FeatureError::StepShapeMissing(u64)   // "Die STEP-Form {0} fehlt im Dokument"
FeatureError::StepShape(String)       // "Die gespeicherte STEP-Form lässt sich nicht lesen: {0}"
```
Regeln: `compute_unchecked` für `StepBody`: Form holen (fehlt → `StepShapeMissing`), `solid()` (Err → `StepShape`), Kopie an `finish` (`validate` + `assign_origin`); `check_extent` wie bei allen Körpern; `check_parameters`: kein Feld. `field_keys` → leer; `check_references` prüft die Form; `prune`/`used` wie bei Netzen (`used_step_shapes`, `prune_step_shapes`); `restore`/`build`: Formen werden übernommen (Arc), gebaut wird über `solid()` beim Rechnen (ein schon gebautes `Arc` baut nie neu). `add_step_bodies`: legt Formen mit fortlaufenden Kennungen an, dann je Körper ein Feature am Marker; ist danach eines nicht `Ok`, wird alles zurückgenommen (Features, Formen, `next_id`, `next_step_shape`, `edits`, `marker`) und die Meldung des ersten geliefert („„{Name}“: {Meldung}“). Netz-Sperre (`ensure_not_mesh`) gilt für STEP-Körper **nicht**.

- [ ] **Step 1: Write the failing tests** in `tests/step_bodies.rs` (Formen aus `normalize` eines Kernel-Quaders 10×20×30 und einer Kugel r 10):
  (a) `step_body_is_one_exact_body`: `add_step_bodies([Quader])` → ein Feature „Q“, Status `Ok`, `label() == "STEP-Körper"`, sichtbarer Körper mit 6 Flächen, Marken `"STEP-Fläche 1"…"STEP-Fläche 6"` mit `origin == id.0`; `edit_count` +1 je Körper.
  (b) `all_or_nothing`: `[Quader, Form mit kaputtem Körpertext]` (Kopf gültig, Körper fehlt) → `Err`, Meldung beginnt mit „„Kaputt“:“; `snapshot().same_content(&vorher)`, `edit_count`, `next_id` unverändert, `step_shapes` leer.
  (c) `works_like_any_exact_body`: Quader-STEP-Körper; Zylinder-Feature r 2 durch die Mitte; Boolean Difference → `Ok`, Volumen 6000 − π·4·30 relativ ±1e-3; Verrunden r 1 an einer Kante zwischen „STEP-Fläche 1“ und „STEP-Fläche 3“ → `Ok`; Skizze auf `FaceRef::from_face(STEP-Fläche 2)` mit Rechteck, Extrusion 5 → `Ok`; Verschieben um (1,0,0) → Hülle verschoben.
  (d) `restore_reuses_the_built_shape`: Import, Snapshot S1, Verschieben, `restore(S1)` → `RestoreKind::Full`, `step_builds()` unverändert, `Arc::ptr_eq` der Form; Rückgängig des Imports (Snapshot vor dem Import) → keine STEP-Körper, Form verworfen; Wiederholen → gleiche Marken.
  (e) `snapshot_rules`: `same_model` falsch bei gleichem Text in anderem `Arc`; `from_snapshot` mit Form, deren Text nicht lesbar ist → Dokument lädt, Feature-Status `Error("Die gespeicherte STEP-Form lässt sich nicht lesen: …")` (P14); Feature mit fehlender Form → `from_snapshot` Err `StepShapeMissing`.
  (f) `coarse_flag`: `StepShape::new(text, 1e-3).is_coarse()`, `1e-6` nicht; Toleranz 2e-3 oder NaN → `new` Err.
  (g) `drawings_and_parts_list_see_it`: Zeichnung mit Vorderansicht und Schnitt des STEP-Körpers → Ansicht und Schnitt berechnet (wie in `tests/drawings.rs` geprüft: Kanten > 0, Schnittfläche > 0); Stückliste hat eine Zeile mit dem Namen des Features; STEP-Export des Dokuments (`write_step` über die sichtbaren Körper) → 1 Körper, 6 Flächen.
- [ ] **Step 2: Run** `cargo test -p planora-feature --test step_bodies -j 2` → FAIL.
- [ ] **Step 3: Implement** `step_bodies.rs` und alle `match`-Stellen; Re-Exporte `pub use step_bodies::{NewStepBody, StepShape, StepShapeId, step_builds};`.
- [ ] **Step 4: Run** → PASS; `cargo test -p planora-feature -j 2` grün (Snapshot-Literale in `tests/{dimensions.rs, drawings.rs}` mit `step_shapes: Default::default()`).
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: `Computed::default()` statt `finish` (keine Körper, Marken ohne Herkunft); `assign_origin` vergessen (Skizze auf Fläche scheitert); `same_model` vergleicht Inhalt statt Zeiger; Rücknahme vergisst `next_step_shape`; ungenutzte Formen nicht verworfen; `solid()` baut bei jedem Aufruf (Zähler); Lade-Fehler bricht das Laden ab statt Fehlerstatus; Netz-Sperre trifft STEP-Körper; `check_extent` ausgelassen; Toleranzbereich nicht geprüft.
- [ ] **Step 6: Commit** `feat(feature): STEP-Körper im Dokument`.

---

### Task 12: Dateiformat 10 (`planora-project`)

**Files:**
- Modify: `crates/planora-project/src/{format.rs, convert.rs, lib.rs}`; die bestehenden Tests laut Global Constraints (1), (2)
- Test: `crates/planora-project/tests/step_format.rs`

**Interfaces:**
- Consumes: Aufgabe 11 (`StepShape::{new, text, tolerance_mm}`, `StepShapeId`, `FeatureKind::StepBody`).
- Produces:
```rust
pub const FORMAT_VERSION: u32 = 10;
// format.rs
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StepFormDto { pub id: u64, pub genauigkeit: f64, pub text: String }
VersionFile { …, #[serde(default, skip_serializing_if = "Vec::is_empty")] pub step_formen: Vec<StepFormDto> }
KindDto::StepKoerper { form: u64 }
```
Regeln: `required_format`: erste Regel „10 mit einem STEP-Körper“, danach die bisherige Reihenfolge unverändert (Kommentar der Funktion ergänzen). `decode_version`: `format < 10` mit STEP-Körper oder `step_formen` → corrupt „STEP-Körper gibt es erst ab Dateiformat 10“; je Form `StepShape::new` (nur Kopf, P14) → Fehler corrupt „STEP-Form {id}: {Meldung}“; Kennung 0 oder doppelt → corrupt; Summe der Textlängen > 200 MiB → corrupt „Die STEP-Formen sind zu groß“; Feature verweist auf fehlende Form → corrupt „„{Name}“: Die STEP-Form {id} fehlt in der Datei“. `encode_version` schreibt nur benutzte Formen (wie bei Netzen).

- [ ] **Step 1: Write the failing tests** in `tests/step_format.rs`:
  (a) `roundtrip_keeps_text_and_tolerance`: Dokument mit zwei STEP-Körpern (eine Form grob 1e-3) → `encode_version` → `"format": 10`, `decode_version` → gleiche Texte (Zeichen für Zeichen), Toleranzen, Kennungen; `Document::from_snapshot` → gleiche Marken und Volumina.
  (b) `old_files_unchanged`: alle Golden-Dateien unter `tests/{data,fixtures}` dekodieren und kodieren Byte für Byte wie bisher; Dokument ohne STEP-Körper, aber mit Parametern → Format 9.
  (c) `reopen_keeps_sketch_on_imported_face`: STEP-Quader, Skizze auf „STEP-Fläche 2“, Extrusion; speichern, laden → Skizze sitzt auf derselben Fläche (Hülle der Extrusion gleich ±1e-9), alle Status `Ok`.
  (d) `hostile_step_data`: Format 9 mit `step_formen` → corrupt mit dem Text; Text „hallo“ → corrupt „STEP-Form 1: …“; doppelte Kennung; fehlende Form; Genauigkeit `-1` → corrupt; Text mit gültigem Kopf, aber kaputtem Körper → lädt, Feature-Status Fehler (P14).
  (e) `encode_does_not_build`: `encode_version` eines Snapshots mit einer noch nicht gebauten Form (`StepShape::new`) → `step_builds()` unverändert.
- [ ] **Step 2: Run** `cargo test -p planora-project --test step_format -j 2` → FAIL.
- [ ] **Step 3: Implement** in `format.rs`/`convert.rs`; die Formatproben der alten Tests anpassen (nur die in den Global Constraints genannten Zeilen).
- [ ] **Step 4: Run** `cargo test -p planora-project -j 2` → PASS.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Regel 10 nach Regel 9; `skip_serializing_if` fehlt (alte Dateien ändern sich); Formatprüfung `<= 10`; unbenutzte Formen geschrieben; Dekodieren baut den Körper; Genauigkeit nicht gespeichert; fehlende Form erst beim Rechnen; Summengrenze fehlt; doppelte Kennung überschreibt; `TooNew` bei 10.
- [ ] **Step 6: Commit** `feat(project): Dateiformat 10 mit STEP-Körpern`.

---

### Task 13: Zustand der App, Schrittname, Verlaufsgröße, Anzeige

**Files:**
- Modify: `crates/planora-app/src/{state.rs, undo_labels.rs, history.rs, icons.rs, properties.rs, feature_dialog.rs}`
- Test: `crates/planora-app/tests/step_state.rs`

**Interfaces:**
- Consumes: Aufgaben 9, 11.
- Produces:
```rust
// state.rs
BodyKind::Step                                              // Symbol Icon::StepBody
pub const STEP_LABEL: &str = "STEP-Körper";
impl AppState {
    /// Legt die Körper an (leerer Name → "STEP-Körper n"), wählt den ersten, richtet die Kamera auf die neuen Körper aus.
    pub fn import_step(&mut self, bodies: Vec<(String, Arc<StepShape>)>) -> Result<Vec<FeatureId>, String>;
    pub fn frame_bodies(&mut self, indices: &[usize]);       // wie frame_all, nur diese Körper
}
// icons.rs
Icon::StepBody => ('\u{e232}', "file-arrow-down")
```
Regeln: `import_step` beendet zuerst eine offene Skizze (`try_close_sketch`), sonst wie `import_mesh` (Dokument bei Fehler unverändert, Meldung über `fail`). `created_label(StepBody) == "STEP importieren"` (auch für mehrere neue Körper in einem Schritt). `snapshot_bytes`: STEP-Formen zählen nicht (geteilte `Arc`, Kommentar wie bei Netzen). Eigenschaftenfeld: Zeilen „Art: STEP-Körper“, „Flächen: {n}“, „Genauigkeit der Datei: {t} mm“ und bei `is_coarse()` die Warnung in Warnfarbe („Rechnen mit diesem Körper kann scheitern.“). `FeatureDialog::new` → `None` für STEP-Körper (P6). `body_kind` → `BodyKind::Step`.

- [ ] **Step 1: Write the failing tests** in `tests/step_state.rs` (Formen über `planora_step::normalize`):
  (a) `import_is_one_step_named`: drei Körper importieren → genau **ein** Verlaufsschritt mit `undo_label() == Some("STEP importieren")`; Rückgängig entfernt alle drei, Wiederholen bringt sie mit gleichen Namen und Marken zurück.
  (b) `names_selection_camera`: Namen `["", "Teil", ""]` → „STEP-Körper 1“, „Teil“, „STEP-Körper 2“; der erste ist gewählt; Kamera-Ziel liegt in der Hülle der neuen Körper (nicht beim alten Körper weit daneben bei (1000,0,0)).
  (c) `downstream_operations`: STEP-Quader → Abziehen mit Zylinder, Verrunden, Skizze auf Fläche über `AppState`-Funktionen wie in `tests/mesh_flow.rs` → alle `Ok`; Positionsfeld verschiebt ihn (Verschieben-Feature entsteht); Zeichnung mit Schnittansicht und Stückliste zeigt ihn.
  (d) `history_size_ignores_shapes`: `snapshot_bytes` mit einer Form von 1 MB Text minus ohne < 4096.
  (e) `coarse_body_never_silently_wrong`: grober Quader (Toleranz 1e-3, Ecken versetzt wie in Aufgabe 7 (c)) → Eigenschaften zeigen die Warnung; Abziehen mit Zylinder → entweder Status `Ok` und Volumen relativ ±1e-3 wie berechnet **oder** Fehlerstatus mit Meldung; nie `Ok` mit anderem Volumen.
  (f) `kind_icon_and_dialog`: `body_kind == BodyKind::Step`, Symbol `file-arrow-down`, `FeatureDialog::new(state, id).is_none()`; offene Skizze wird vor dem Import beendet.
- [ ] **Step 2: Run** `cargo test -p planora-app --test step_state -j 2` → FAIL.
- [ ] **Step 3: Implement** die Änderungen in den sechs Dateien.
- [ ] **Step 4: Run** → PASS; `cargo test -p planora-app --test undo_labels --test history --test icons --test fonts -j 2` grün.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: je Körper ein eigener Verlaufsschritt (zwei `refresh`-Bilder dazwischen); Schrittname „Netz importieren“; Formtext in `snapshot_bytes` gezählt; leerer Name bleibt leer; Zähler nicht erhöht; Kamera auf alle Körper; erster Körper nicht gewählt; Warnung ohne Warnfarbe; Bearbeiten-Fenster öffnet für STEP-Körper; Skizze bleibt offen.
- [ ] **Step 6: Commit** `feat(app): STEP-Körper im Zustand, Schrittname und Anzeige`.

---

### Task 14: Fenster „STEP importieren“

**Files:**
- Create: `crates/planora-app/src/step_import_dialog.rs`
- Modify: `crates/planora-app/src/{lib.rs, app.rs, workspace.rs}`; `crates/planora-app/tests/graphit_style.rs` (Menüliste)
- Test: `crates/planora-app/tests/step_import_ui.rs`

**Interfaces:**
- Consumes: Aufgaben 9, 13; `import_dialog::{Unit, STALE}`, `jobs::{Job, Poll}`, `cards::card_row`, `dialog::{Dialog, DialogSize}`, `file_chooser::FileChooser`.
- Produces:
```rust
pub const TITLE: &str = "STEP importieren";
pub struct StepLoaded { pub path: PathBuf, pub file: Arc<StepFile>, pub import: StepImport, pub edits: Option<u64> }
pub struct StepImportDialog {
    pub loaded: Option<StepLoaded>, pub checked: Vec<bool>, pub error: Option<String>,
    pub last_rect: Option<egui::Rect>, pub last_cards: Vec<egui::Rect>, /* chooser, Aufträge */
}
impl StepImportDialog {
    pub fn new(dir: PathBuf) -> StepImportDialog;        // Endungen ["step", "stp"]
    pub fn load(&mut self, path: PathBuf); pub fn load_at(&mut self, path: PathBuf, edits: u64);
    pub fn set_unit(&mut self, unit: LengthUnit);        // neue Berechnung im Hintergrund (P11)
    pub fn is_loading(&self) -> bool; pub fn progress(&self) -> f32; pub fn phase(&self) -> Option<&'static str>;
    pub fn cancel(&mut self); pub fn tick(&mut self, block: bool);
    pub fn import(&mut self, state: &mut AppState) -> Result<Vec<FeatureId>, String>;
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, last_dir: &mut Option<PathBuf>) -> bool;
}
/// "3 Körper importiert" bzw. "3 Körper importiert, 1 ausgelassen: Name (Grund)" (mehrere mit "; ").
pub fn summary(imported: usize, skipped: &[(String, String)]) -> String;
pub fn size_text(bounds: Option<(DVec3, DVec3)>) -> String;   // "25,4 × 50,8 × 76,2 mm", ohne Hülle "–"
```
Einheitenwahl: `Unit::Millimeter/Zentimeter/Meter/Zoll` ↔ `LengthUnit` gleichen Namens; `LengthUnit::Other` zeigt `label()` als fünfte, nicht wählbare Angabe. Ablauf: Datei wählen → Auftrag „Lesen“ (`read_step_file`, Phase „Die Datei wird gelesen …“) → Auftrag „Prüfen“ (`import_bodies`, Phase „Die Körper werden geprüft …“) → Liste. Karten in einer Reihe (gleich hoch): „Datei“ (Dateiname, Schema, Einheit der Datei mit Auswahl `Unit::ALL`, bei `unit_known == false` Hinweis „Die Datei nennt keine Längeneinheit; angenommen: Millimeter.“) und „Körper“ (Tabelle mit Spalten aus den Global Constraints; Häkchen je Zeile mit `push_id(("step-row", i))`, Vorgabe: Ok-Körper an, ausgelassene aus und gesperrt; Status-Texte wie festgelegt, ausgelassen in Fehlerfarbe, Warnung in Warnfarbe). „Importieren“ gesperrt, wenn nichts angehakt ist. Nach Erfolg: `state.message = Some(summary(..))`, Fenster schließt. Stale wie P13. Menü „STEP importieren …“ in `app.rs` gesperrt in schreibgeschützten Ansichten („In einer alten Version lässt sich nichts hinzufügen.“), öffnet über `open_step_import` (beendet offene Skizze); `workspace.rs`: Feld `step_import_dialog`, in `dialog_open`, beim Schließen/Tab-Wechsel verworfen wie `import_dialog`.

- [ ] **Step 1: Write the failing tests** in `tests/step_import_ui.rs` (Muster `tests/import_dialog.rs`, Probedateien aus `planora_step::normalize` und Kopien von `crates/planora-step/tests/data/*.step` im Temp-Ordner):
  (a) `list_shows_every_body`: `inch_assembly.step` + `box`-Datei ohne Deckel kombiniert (im Test geschrieben) → nach `tick(true)` zeigen die Texte Name, Flächen, „25,4 × 50,8 × 76,2 mm“, „Zoll“, „wird übernommen“ und „wird ausgelassen: Die Kante bei (…) gehört nur zu einer Fläche.“.
  (b) `checkboxes_and_summary`: einen Ok-Körper abhaken, Importieren → Dokument hat die übrigen; `state.message == Some(summary(2, &[("Deckellos", "…")]))`; `summary(3, &[("A","x"),("B","y")]) == "3 Körper importiert, 2 ausgelassen: A (x); B (y)"`, `summary(1, &[]) == "1 Körper importiert"`.
  (c) `one_undo_step`: nach dem Import `undo_label() == Some("STEP importieren")`; Strg+Z über Tastenereignis bei offenem Fenster ändert nichts (P13), nach dem Schließen nimmt es alle Körper zurück.
  (d) `cancel_and_errors`: `Job::spawn_with` mit abgebrochenem Fortschritt → `error == Some("Abgebrochen")`, keine Liste; abgeschnittene Datei → Meldung „Die Datei ist unvollständig: Sie endet in Zeile …“ im Fenster, Fenster bleibt offen; Datei mit 51 Körpern → „Die Datei enthält mehr als 50 Körper.“.
  (e) `unit_override_rebuilds`: Einheit „Millimeter“ wählen (Klick auf den Radioknopf) → Phase „Die Körper werden geprüft …“, danach Größe „1 × 2 × 3 mm“ für Block; Häkchen bleiben je Zeile erhalten.
  (f) `stale_and_undo_consistency`: Liste geladen, dann Dokument ändern (Quader anlegen) → `STALE`, keine Liste, „Importieren“ nicht da; Import, Rückgängig, Fenster neu öffnen → leeres Fenster ohne alte Häkchen oder Meldungen; nach Wiederholen zeigt das Eigenschaftenfeld den richtigen Körper.
  (g) `real_keys_in_the_file_field`: Dateiname im Auswahlfeld mit echten Tasten tippen, **ein Zeichen je Bild** (`Key` gedrückt, `Text`, `Key` losgelassen, leeres Bild), Enter → Datei wird gelesen; die Zeilen-Kennungen ändern sich nicht, wenn sich Namen ändern (zwei Bilder mit verschiedenen Namen: `Id` des Häkchens gleich).
  (h) `layout`: Fenster 800 × 600 und 1600 × 1000, hell und dunkel: volle Breite (`last_rect` wie in `wide_dialogs.rs`), gleiche Kartenhöhe (`assert_cards_inside`), keine Zeichen außerhalb der Schriften.
  (i) `menu_entry`: Menü „Datei“ zeigt „STEP importieren …“ direkt unter „Netz importieren …“; in einer alten Version gesperrt mit Hinweis; öffnet das Fenster mit Titel „STEP importieren“.
- [ ] **Step 2: Run** `cargo test -p planora-app --test step_import_ui -j 2` → FAIL.
- [ ] **Step 3: Implement** `step_import_dialog.rs` und die Verdrahtung.
- [ ] **Step 4: Run** → PASS; dazu `--test graphit_style --test no_hard_coded_colors --test no_raw_windows --test no_raw_text_edits --test fonts --test icons --test wide_dialogs --test import_dialog`.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: ausgelassene Körper angehakt; Häkchen-`Id` aus dem Namen; Override verwirft Häkchen; Strg+Z wirkt bei offenem Fenster; kein `STALE`; Abbrechen beendet nur die Anzeige, nicht den Auftrag; Fehlerfarbe für Warnungen; Zusammenfassung ohne Gründe; Endung `.stp` fehlt; Fenster nicht in `dialog_open`; schreibgeschützte Ansicht kann importieren.
- [ ] **Step 6: Commit** `feat(app): Fenster „STEP importieren“`.

---

### Task 15: Dokumentation und Abschlussprüfung

**Files:**
- Modify: `README.md` (neuer Abschnitt „STEP-Import“ nach „STEP-Export (exakte Körper)“ in Laiensprache: Menü, Fenster, was gelesen wird, Baugruppen und Lagen, Einheiten, Warnung bei grober Genauigkeit, was ausgelassen wird und warum, Speichern in der Projektdatei, Grenzen aus Spec §5 und P12), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt: STEP-Import“: Geliefertes, P1–P16, Torergebnis aus Aufgabe 4 mit Tabelle, Grenzen: Flächenmodelle, Farben, PMI, `OFFSET_SURFACE`, Torus ohne Naht, schräge Drehgeraden, Speicherbedarf je Version; Handliste als Abhakliste), `docs/superpowers/specs/2026-10-21-step-import-design.md` (neuer §8 „Festlegungen der Umsetzung“: P1–P16 kurz), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)
- Test: keiner (Gates)

- [ ] **Step 1: Texte schreiben** (einfache Sprache, kein Fachjargon, keine Füllsätze).
- [ ] **Step 2: Gates** mit Ausgabe: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2`; `cargo test -p planora-step --release -- --ignored` (Zeiten berichten); `cargo deny check` (Ausnahmen zbus-Familie und ttf-parser erneut prüfen, Ergebnis berichten); `git diff --stat c16d988 -- crates/planora-kernel crates/planora-math crates/planora-mesh crates/planora-exchange crates/planora-drawing crates/planora-sketch crates/planora-render` leer (außer einer von Dennis beschlossenen Kernel-Änderung); `git diff c16d988 -- '**/Cargo.toml' Cargo.lock deny.toml` zeigt nur die zwei erlaubten internen Kanten; `git diff c16d988 -- crates/planora-project/tests/data crates/planora-project/tests/fixtures` leer; `git status` sauber.
- [ ] **Step 3: Mutationssumme** aller Aufgaben (Zahl, keine grüne offen) in den Bericht.
- [ ] **Step 4: Commit** `docs: STEP-Import beschrieben, offene Punkte, Handliste`.

---

### Task 16: Prüfung im echten Fenster (Controller)

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Handliste abhaken, Befunde), dieser Plan („Abweichungen bei der Umsetzung“)

Der Controller führt sie selbst aus (keine Subagent-Aufgabe): Release-Build `cargo build --release -p planora-app`, App starten, Bedienung über `tools/gui-drive.ps1` (Klicks, Tippen, Bildschirmfotos nach `%TEMP%\planora_shot.png`), Fensterversatz 1:1 zum Bildschirmfoto. Probedateien: Kopien der handgeschriebenen Dateien und ein Export eigener Körper in einen Temp-Ordner (am Ende entfernt).

- [ ] **Step 1: Handliste** (jeweils Bildschirmfoto prüfen): (1) „Datei → STEP importieren …“ öffnen, `inch_assembly.step` wählen: Fortschritt, dann Liste mit drei Körpern, „Zoll“; (2) Einheit auf Millimeter und zurück: Größen ändern sich, Häkchen bleiben; (3) Importieren: Meldung „3 Körper importiert“, Kamera zeigt die Körper, erster gewählt, Baum zeigt „Block“, „Block (2)“, „Stift“ mit Symbol; (4) Strg+Z nimmt alle drei zurück, Strg+Y bringt sie wieder; (5) Block mit Zylinder abziehen, Kante runden, Skizze auf eine Blockfläche, extrudieren; (6) Zeichnung mit Schnitt und Stückliste; (7) Speichern, Schließen, Öffnen: alles gleich, Skizze auf derselben Fläche; (8) Datei mit ausgelassenem Körper: roter Grund in der Liste, Häkchen gesperrt; (9) großer Export (Treppe 10 000 Flächen): Oberfläche bleibt bedienbar, Abbrechen wirkt sofort; (10) abgeschnittene Datei: Meldung mit Zeile; (11) grobe Datei: Warnung in Liste und Eigenschaftenfeld; (12) Hell/Dunkel und Fenster 800 × 600; (13) STEP-Export des importierten Körpers und erneuter Import: gleiche Flächenzahl.
- [ ] **Step 2: Befunde** beheben lassen (eigene Fix-Aufgaben nach demselben Muster) oder als offene Punkte eintragen.
- [ ] **Step 3: Commit** `docs: Handprüfung STEP-Import im echten Fenster`.

---

## Abweichungen bei der Umsetzung

(Wird während der Umsetzung gefüllt.)

- **Aufgabe 9:** `ImportedBody` hat zusätzlich `placement_reason: Option<String>` und `warnings_named(shown)` (Lage-Meldung mit dem App-Namen „STEP-Körper n“ neu gebildet; im Feld `warnings` steht bei leerem Namen „STEP-Körper“ statt „„“); `StepImport` hat zusätzlich `messages: Vec<String>` (`unsupported_kinds`); neue Konstante `UNNAMED_BODY = "STEP-Körper"`, neue Fehlerart `StepReadError::UnclearUnits` (freie Volumen-Einträge bei widersprüchlichen Einheiten, nie still Millimeter); `StepImport::unit_known` sagt, ob die *Datei* eine Längeneinheit nennt (auch bei gewählter Einheit). Neu in `read/topo.rs`: `count_faces(file, items)` (Flächen so gezählt wie `read_raw`); `read_raw` zählt bei einer scheiternden Fläche den Rest des Körpers im Fortschritt mit (Gesamtzahl bleibt exakt). `read/geom.rs`: Richtungen mit Länge 1 und Bezugsrichtungen senkrecht zur Achse (je bis auf 4 ε) bleiben bitgleich (sonst wäre Normalisieren nicht ruhend). `normalize` schreibt den gelesenen Körper noch einmal und liest nur, wenn der Text sich ändert (höchstens 4 Lesungen; die erste Lesung zieht etwa Ecken auf eine Kegelspitze); „ruhend“ ist in den Tests erfüllt, nicht bewiesen. `ImportedBody::bounds` aus Ecken und einem Netz (Sehne 0,05 mm). Freie Volumen-Einträge: Einheiten aller Kontexte mit Längeneinheit müssen in Länge und Winkel übereinstimmen (Genauigkeit: die gröbste), ohne solche Kontexte Millimeter (unbekannt).
- **Aufgabe 9, Fix-Runde 1:** Schreiber (`geometry.rs`, Kegel/Zylinder): liegt der Bezugsort des Kernels (v = 0) im v-Bereich der Fläche (Spielraum 1e-9 der Bereichslänge, mindestens 1e-9) und ist der Radius dort positiv, bleibt er – ein gelesener Körper ist dann ein Fixpunkt von Schreiben/Lesen (vorher kroch der Radius weit vom Ursprung in der letzten Stelle, und ein zweites Normalisieren lehnte 2–3 % ab). Freie Volumen-Einträge: jeder Kontext wird zuerst auf die gewählte Einheit umgestellt, dann die gröbste Genauigkeit genommen (unabhängig von der Reihenfolge). Neu `StepImport::unit_state: UnitState { Named, Missing, Unclear }` (Aufgabe 14: „keine Längeneinheit“ nur bei `Missing`, bei `Unclear` „verschiedene Einheiten“); `unit_known == (unit_state == Named)`. Gründe aus dem Normalisieren ohne „Zeile n“: „Der Körper lässt sich nicht in die Form des Programms umschreiben: {Grund}“. Abbruch auch zwischen den Stufen von `build_solid` und zwischen den Runden des Normalisierens. `import_bodies` erwartet einen frischen `Progress` (Aufgabe 14: je Auftrag ein neuer). Aufgabe 8 (P3 der Review): `FoundBody::placement_guessed` / `ImportedBody::placement_guessed` und Meldung `guessed_note` („Die Lage von „{name}“ ist angenommen: Die Datei sagt nicht, welches Teil das übergeordnete ist.“), wenn auf dem Weg eine Beziehung mit Transformation ohne Vorkommen liegt (Lage angewandt).
- **Aufgabe 10:** (a) Volumen und Oberfläche „über `tessellate` relativ ±1e-6“ und die Hülle ±1e-9 aus dem Netz sind nicht durchgehend erreichbar, weil das Netz von der Parametrisierung abhängt, die STEP nicht ganz trägt (Kernel-Bereich nach Booleschen Operationen weiter als die Fläche, Kegelachse umgekehrt, Kugelnaht 4e-10 rad neben 0 aus dem Polrauschen, gemittelte Kontrollreihen): 4 von 60 Fällen weichen bis 4,5e-4 (Volumen) bzw. 7e-3 mm (Netzhülle) ab, alle anderen sind bitgleich. Der Test prüft darum die Geometrie selbst (Netzpunkte jeder Fläche auf einer Fläche des anderen Körpers mit gleicher Außenseite, Kantenpunkte auf Kanten, Ecken, je ±1e-9 bzw. ±`NURBS_EDGE_TOL`, beide Richtungen; Zahlen aus `validate` gleich) und das Netz nur bis zu seinem eigenen Fehler (2 · Sehne · Oberfläche bzw. 2 · Sehne). (c) `bore_halves` hat 6 Flächen (nicht 8); Volumen über das Netz auf Sehne 0 hochgerechnet (`2 · V(0,001) − V(0,002)`); Abziehen des Stabs bei Körpern mit Kegel- oder Torusfläche lehnt der Kernel ab („Der Schnitt von Kegel/Torus mit Ebene wird noch nicht unterstützt“, auch bei eigenen Körpern) und wird dort übersprungen. (d) Gerundet wird die längste gerade Kante, die der Kernel auch am Original rundet. `tests/common/bodies.rs` enthält auch `poses()` aus `curved.rs`. Leser: Schleifen durch einen Pol gelten als verdreht, wenn sie sich in u nicht schließen (nicht mehr nach ihrer Ausdehnung); Punkte beim Anlauf auf einen Pol (bis 1e-4 · (1 + |Pol|), höchstens 1 % der Schleife) gelten als Pol; Naht auf einer nicht geschlossenen gekrümmten Fläche wird vor dem Netz abgelehnt.
