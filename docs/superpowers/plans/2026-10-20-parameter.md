# Benannte Parameter und Gleichungen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **COMMIT-TRAILER (jeder Commit, ohne Ausnahme, wörtlich als letzte Zeile):**
> `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
> Frühere Agents haben hier wiederholt „Opus“ oder einen anderen Namen eingesetzt. Vor jedem `git commit` die Zeile Zeichen für Zeichen prüfen.
> Immer nur die genannten Pfade einzeln `git add`en (nie `git add -A`/`.`/`-u`).
> **Nie `git checkout -- …`, `git stash`, `git reset` oder `git restore`** (auch nicht zum Zurücknehmen von Mutationen): Das hat schon fremde, noch nicht gesicherte Arbeit vernichtet. Mutationen werden mit dem Editor eingespielt und mit dem Editor zurückgenommen; `git diff` zeigt danach, dass nichts übrig ist.

**Goal:** Ein Dokument hat eine Parametertabelle (Name, Formel, Wert, Art, Kommentar); in alle Zahlenfelder der Körper-Funktionen und in die Skizzenmaße lassen sich Formeln tippen; ändert man einen Parameter, rechnet das Modell als **ein** rücknehmbarer Schritt neu; Dateiformat 9 nur mit Parametern oder Formeln.

**Architecture:** Ein reiner Formelbaustein `expr.rs` (Lexer, Parser, Auswertung mit Einheitenprüfung, Namensregeln, Umbenennen, Tabelle mit Zykluserkennung) ohne Oberfläche. `Document` hält die Parametertabelle; Formeln liegen als Text in **Nebentabellen** (`Feature.formulas: BTreeMap<FieldKey, String>`, bei Skizzen `SketchFeature.formulas: BTreeMap<ConstraintId, String>`), die `f64`-Felder bleiben der zuletzt berechnete Wert (Spec §4a.1). Vor jeder Neuberechnung schreibt ein Auswertungsdurchgang die Formelwerte in die Felder und erhöht nur bei geänderten Werten die Feature-Version, so bleibt der Zwischenspeicher richtig. Öffentliche Änderungen prüfen alle Formeln **vorher** und lehnen fehlerhafte ab (nichts ändert sich); gültige Formeln mit scheiterndem Modell ergeben wie heute einen Fehlerstatus. Rückgängig, Speichern und Laden laufen über `Snapshot`.

**Tech Stack:** Rust-Workspace, keine neuen Abhängigkeiten, eigener Parser; egui/eframe 0.36; Tests im eigenen Harness (`AppState::new()`, `egui::Context::run_ui`, Vorbilder `crates/planora-app/tests/{feature_dialog.rs, undo_flow.rs, undo_sketch.rs, mesh_body_ui.rs, sketch_dialogs.rs, wide_dialogs.rs}`, `crates/planora-feature/tests/{restore.rs, recompute.rs, mesh_bodies.rs, blend_features.rs}`, `crates/planora-project/tests/section_format.rs`).

**Spec:** `docs/superpowers/specs/2026-10-20-parameter-design.md` (vom Nutzer freigegeben wie geschrieben, alle Vorschläge samt §4a und den Standardwerten aus §7 angenommen). Vorbild für Stil und Muster: `docs/superpowers/plans/2026-10-19-rueckgaengig.md`. Basis: `master` @ `7f571c6`; Arbeitszweig `feature/parameter` in eigenem Worktree.

## Global Constraints

- Texte deutsch in einfacher Laiensprache, keine technischen Hinweise. Feste Texte stehen in den Aufgaben (Meldungstabelle in Aufgabe 1); Menü „Bearbeiten → Parameter …“, Fenstertitel „Parameter“, Spalten „Name“, „Formel“, „Wert“, „Art“, „Kommentar“, Knöpfe „Zeile hinzufügen“, „Zeile löschen“, „Durch den Wert ersetzen“, „Abbrechen“, Arten „Länge“, „Winkel“, „Zahl“, Sperre „Beenden Sie zuerst die Skizze.“, Formelzeichen „fx“, Wertanzeige „= 20,00 mm“, Hinweis „Namen: Breite, Tiefe …“, Löschsperre „Breite wird in Quader 1, Skizze 2 benutzt.“, Schrittnamen „Parameter anlegen“, „Parameter ändern“, „Parameter löschen“.
- Werte aus der Spec: höchstens **500 Parameter** (`MAX_PARAMETERS`), Formeln bis **200 Zeichen** (`MAX_FORMULA_CHARS`), Ausdrücke bis **Tiefe 64** (`MAX_FORMULA_DEPTH`); Auswertung in `f64`, nicht endliche Ergebnisse sind ein Fehler; Winkel in `sin`/`cos`/`tan` in Grad; Funktionen genau `sqrt`, `sin`, `cos`, `tan`, `abs`, `min`, `max`, `round`, Konstante `pi`; Operatoren `+ - * / ^`, Klammern, Komma oder Punkt als Dezimalzeichen.
- Formelfähige Felder (Spec §4a.2), genau diese: Quader 6 (Breite, Tiefe, Höhe, Position x/y/z), Zylinder und Kegel je 5 (Radius, Höhe, Position x/y/z), Kugel 4 (Radius, Position x/y/z), Extrusion 1 (Höhe), Verschieben 3 (Δx, Δy, Δz), Rundung 1 (Radius), Fase gleich 1 (Abstand), Fase zwei 2 (Abstand 1, Abstand 2), Fase mit Winkel 2 (Abstand, Winkel in Grad), Netz-Körper 4 (Position x/y/z, Maßstab), alle bestimmenden Skizzenmaße. **Nicht:** variable Rundung, Positionsfeld des Körpers im Eigenschaftenfeld, Drehung, anzeigende Skizzenmaße, Zeichnungsmaße.
- **Nur `planora-feature`, `planora-project`, `planora-app` und Dokumente ändern sich.** Ohne Diff gegen `7f571c6`: `crates/planora-kernel/**`, `crates/planora-mesh/**`, `crates/planora-step/**`, `crates/planora-exchange/**`, `crates/planora-sketch/**`, `crates/planora-drawing/**`, `crates/planora-render/**`, `crates/planora-math/**`.
- **Keine neuen Abhängigkeiten**: `Cargo.toml`/`Cargo.lock`/`deny.toml` ohne Diff; `cargo deny check` grün mit den in `deny.toml` vermerkten Ausnahmen (zbus-Familie, ttf-parser: bei Abschluss erneut prüfen und Ergebnis berichten).
- **Anti-Ziel der Spec:** nie ein stilles falsches Modell. Fehlerhafte Formel → abgelehnt, Dokument und `edit_count` unverändert; gültige Formel, scheiterndes Modell → Fehlerstatus mit deutscher Meldung, Abhängige übersprungen, Rückgängig stellt zurück; Skizze mit Formeln und ungelöstem Solver → Fehlerstatus.
- Dateien ohne Parameter und Formeln bleiben **Byte für Byte** wie bisher (alle Golden-Dateien unter `crates/planora-project/tests/{data,fixtures}` unverändert).
- Farben nur über `theme::` (Wächter `no_hard_coded_colors`), Fenster nur über `dialog::Dialog` (`no_raw_windows`), Textfelder nur über `theme::text_field*` (`no_raw_text_edits`), Zeichen nur aus Montserrat/Roboto/Phosphor (`tests/fonts.rs`); Symbol des Menüeintrags Phosphor Light `function` U+EBE4 aus `crates/planora-app/assets/fonts/Phosphor-Light-style.css` (`tests/icons.rs`); gleiche Kartenhöhe in einer Reihe; Oberfläche über die ganze Breite.
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests mit `-j 2` **im Vordergrund** (langsamer Rechner; nie `cargo clean`; keine Hintergrundprozesse zurücklassen).
- **Tests müssen beißen:** Jede Aufgabe endet mit mindestens **10 Mutationen** (einzeln mit dem Editor einspielen, roten Test notieren, mit dem Editor zurücknehmen); Bericht „Mutation → roter Test“. Bleibt eine Mutation grün, wird der Test schärfer.
- Keine Streudateien: Testdateien in `std::env::temp_dir()`-Unterordnern, am Ende entfernt; `git status` am Ende sauber.
- Bestehende Tests bleiben unverändert grün, außer: (1) `assert_eq!(FORMAT_VERSION, 8)` in `crates/planora-project/tests/{dimensions_format.rs:346, drawings_format.rs:323, format2.rs:137, format3.rs:139, parts_format.rs:96}` → `9`; (2) die 36 Struktur-Literale `SketchFeature { … }` in 23 Dateien (`grep -rn "SketchFeature {" crates --include=*.rs`) bekommen `formulas: Default::default()`; (3) die Literale `SnapshotFeature { … }` (`crates/planora-project/src/convert.rs`, `crates/planora-app/tests/history.rs:375`) bekommen `formulas: Default::default()`; (4) die Literale `Snapshot { … }` in `crates/planora-feature/tests/{dimensions.rs:89, drawings.rs:37}` bekommen `parameters: Vec::new(), next_param: 1`. Sonst nichts.
- Abweichungen von der Spec stehen im Ledger des Controllers, am Ende in `docs/superpowers/offene-punkte.md` und im Abschnitt „Abweichungen bei der Umsetzung“ dieses Plans.

**Festlegungen des Plans (P1–P14; der Nutzer kann sie kippen):**
- **P1 Werte schreiben:** Vor jeder Neuberechnung (öffentliche Änderung, Laden, Zurückladen) wertet `Document` erst die Tabelle, dann alle Formeln aus und schreibt die Ergebnisse in die `f64`-Felder (Skizzen: `Sketch::set_constraint_value` in der Definition). Nur ein Feature, dessen Werte oder Formelfehler sich ändern, bekommt eine neue Version; Gleiches bleibt gleich (bitgenauer Vergleich), damit nichts unnötig neu rechnet.
- **P2 Zahl statt Formel:** Ändert eine Operation ohne Formelangabe (`update`, `update_with_report`, Ziehen, `set_mesh_transform`) den Wert eines Feldes mit Formel, entfällt **nur diese** Formel. Formeln für Felder, die es nach einer Änderung der Art nicht mehr gibt (Fase zwei → Fase gleich), entfallen ebenfalls.
- **P3 Strenge Prüfung, milder Ladeweg:** Jede Formel hat nach dem Durchgang höchstens einen „Formelfehler“ (`FormulaIssue::Broken`, Formel selbst kaputt) oder „Wertfehler“ (`FormulaIssue::Value`, Ergebnis außerhalb des zulässigen Bereichs). Eine öffentliche Änderung wird abgelehnt, wenn danach ein Feature einen `Broken`-Fehler hätte, den es vorher nicht hatte. Beim Laden entstehen `Broken`-Fehler ohne Ablehnung (Spec §4a.7). Beide Arten setzen den Status `Error(meldung)` und lassen das Feld auf dem letzten Wert.
- **P4 Einheiten:** Ausdrücke rechnen mit Exponenten von Länge und Winkel (`Dim { length, angle }`). Bei `+`, `-`, `min`, `max` passt eine reine Zahl zu allem; Länge mit Winkel ist ein Fehler. `*`, `/` addieren bzw. ziehen Exponenten ab; `^` mit Einheit nur mit ganzzahligem Exponenten; `sqrt` nur bei geraden Exponenten. Ein Feld nimmt seine Einheit oder eine reine Zahl an (Spec §2: „eine Zahl lässt sich überall einsetzen“), eine Länge passt nicht in ein Zahlfeld (Maßstab).
- **P5 Komma:** Ein Komma **zwischen zwei Ziffern** ist Dezimalzeichen („1,5“); sonst trennt es Werte. Werte trennt man deshalb mit „;“ oder „, “ (Komma und Leerzeichen): `min(1; 5)`, `min(a, b)`; `min(1,5)` ist **ein** Wert 1,5 und ergibt die Meldung „„min“ braucht mindestens zwei Werte …“.
- **P6 Namen:** Nachschlagen und Eindeutigkeit ohne Rücksicht auf Groß-/Kleinschreibung (`to_lowercase`); Umbenennen ersetzt ganze Namen-Token (Lexer, nicht Textsuche), der übrige Text bleibt zeichengleich, auch in nicht zerlegbaren Formeln. Namen höchstens 64 Zeichen, Kommentar höchstens 200 Zeichen.
- **P7 Durch den Wert ersetzen:** Jede Formel, die den Parameter **direkt** nennt, wird durch ihren aktuellen Wert ersetzt (Feature-Feld: Formel entfällt; Parameterformel: Zahltext `number_text(wert)`), dann wird der Parameter gelöscht; ein Aufruf, ein `edit_count`-Schritt, keine Wertänderung.
- **P8 Fenster „Parameter“ sperrt Strg+Z nicht:** Es zählt nicht zu `DocTab::dialog_open`; Strg+Z wirkt, wenn kein Feld die Tastatur hat; nach Rückgängig/Wiederholen bleibt es offen und lädt seine Zeilen neu (verworfene Entwürfe).
- **P9 Übernahme:** In Fenstern mit „Übernehmen“/„Erzeugen“/„OK“ (Erzeugen-, Bearbeiten-, Rundungsfenster, Maßfeld der Skizze) wirken Formelfelder wie heute erst mit diesem Knopf bzw. Enter; die Anzeige „= Wert“ rechnet bei jeder Eingabe mit. Bei Enter oder Verlassen des Feldes wirken: Zeilen des Fensters „Parameter“ und die Formelfelder des Netz-Körpers im Eigenschaftenfeld (dort gibt es keinen Knopf).
- **P10 Netz-Körper:** Position x/y/z und Maßstab im Eigenschaftenfeld werden Formelfelder (Tippen statt Ziehen); die Drehung bleibt ein Ziehfeld.
- **P11 Verschieben mit Formeln:** Ein Verschieben-Feature mit Formeln wird beim Ziehen des Körpers oder im Positionsfeld **nie** erweitert; es entsteht ein neues Verschieben (die Formeln bleiben gültig).
- **P12 Skizzenstatus:** Eine Skizze mit mindestens einer Formel bekommt den Fehlerstatus, wenn der Solver nicht `Solved` meldet (auch wenn sie schon vorher ungelöst war); Skizzen ohne Formeln verhalten sich wie heute.
- **P13 Neue Zeile:** „Zeile hinzufügen“ legt `ParameterN` (kleinstes freies N ≥ 1, ohne Rücksicht auf Groß-/Kleinschreibung), Formel „10“, Art Länge, ohne Kommentar an.
- **P14 Meldungsgrammatik:** „Den Namen „Breit“ gibt es nicht.“ (statt „Der Name … gibt es nicht.“ aus dem Beispiel der Spec).

## Review Focus

1. **Parameter löschen oder umbenennen, der in Formeln steckt** (Quader-Breite „Breite/2“, Skizzenmaß „Breite“, Parameter „Tiefe = Breite/2“): Löschen wird mit „Breite wird in Quader 1, Skizze 1, Parameter Tiefe benutzt.“ abgelehnt; „Durch den Wert ersetzen“ lässt alle Werte gleich und ist ein Schritt; Umbenennen schreibt alle Formeln um, Rückgängig bringt Name und Formeln zurück. → Aufgabe 3 Test (f), Aufgabe 6 Test (g), Aufgabe 9 Tests (f, g).
2. **Ungültige Eingaben ändern nichts** (Zyklus „a = b + 1“, „b = a“; Winkel-Parameter in einem Längenfeld; Länge im Maßstab; Division durch null erst nach einer Parameteränderung; Name „sin“; 201 Zeichen): Ablehnung mit genau der Meldung, `snapshot()` inhaltsgleich, `edit_count` gleich. → Aufgabe 1 Tests (d, e, h), Aufgabe 2 Test (b), Aufgabe 3 Tests (c, d), Aufgabe 4 Test (c).
3. **Parameteränderung lässt das Modell scheitern** (Breite → 0; Rundungsradius zu groß; Dreieck mit Seiten a, b, c und c > a + b; Rechteck 30 → 400): Feature zeigt Fehlerstatus mit Meldung, Abhängige „übersprungen“, nie ein stilles anderes Modell; Zurücksetzen bzw. Rückgängig stellt Tabelle **und** Felder **und** Körper her. → Aufgabe 3 Test (d), Aufgabe 5 Test (e), Aufgabe 6 Tests (c, d), Aufgabe 8 Test (c).
4. **Zahl tippen statt Formel, Komma-Zahlen** („2,5“ bleibt Zahl; „min(1,5)“ ist ein Wert; nach „Breite/2“ im Feld „7“ tippen löscht die Formel und der Wert bleibt 7, auch nach späterer Parameteränderung; „fx“ anklicken setzt den Wert ein): → Aufgabe 1 Tests (b, c), Aufgabe 3 Test (e), Aufgabe 9 Test (c), Aufgabe 10 Test (b).
5. **Laden und Speichern** (Datei ohne Parameter Byte für Byte gleich; Format-8-Datei mit Parametern abgelehnt; Datei mit doppeltem Namen oder Zyklus abgelehnt mit deutscher Meldung; Datei mit einer kaputten Feature-Formel lädt, Feature im Fehlerstatus, Feld mit gespeichertem Wert; Formel an Skizzenmaß nach gelöschter früherer Abhängigkeit): → Aufgabe 7 Tests (b, d, e).

---

### Task 1: Reiner Formelbaustein (`expr.rs`)

**Files:**
- Create: `crates/planora-feature/src/expr.rs` (Modulkommentar mit P4, P5, P6; keine Abhängigkeit auf `Document`)
- Modify: `crates/planora-feature/src/lib.rs` (`pub mod expr;` und Re-Exporte unten)
- Test: `crates/planora-feature/tests/expr.rs` (neu)

**Interfaces:**
- Consumes: nichts (nur `std`).
- Produces:
```rust
pub const MAX_PARAMETERS: usize = 500;
pub const MAX_FORMULA_CHARS: usize = 200;   // Zeichen (chars), nicht Bytes
pub const MAX_FORMULA_DEPTH: usize = 64;    // Verschachtelung: Klammern, Funktionen, unäre Zeichen, Operatoren
pub const MAX_PARAM_NAME_CHARS: usize = 64;
pub const MAX_COMMENT_CHARS: usize = 200;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamUnit { Length, Angle, Number }
impl ParamUnit {
    pub const ALL: [ParamUnit; 3];
    pub fn label(self) -> &'static str;   // "Länge", "Winkel", "Zahl"
    pub fn suffix(self) -> &'static str;  // "mm", "°", ""
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dim { pub length: i32, pub angle: i32 }   // Dim::NUMBER = {0,0}; Dim::of(ParamUnit)
#[derive(Clone, Debug, PartialEq, thiserror::Error)] #[error("{0}")]
pub struct ExprError(pub String);
pub struct Expr { /* Baum, privat */ }
impl Expr { pub fn names(&self) -> Vec<String>; }     // Namen wie geschrieben, ohne Doppelte (klein verglichen)
pub fn parse(text: &str) -> Result<Expr, ExprError>;
pub fn evaluate(e: &Expr, lookup: &dyn Fn(&str) -> Option<(f64, ParamUnit)>) -> Result<(f64, Dim), ExprError>;
pub fn fit_unit(value: f64, dim: Dim, want: ParamUnit) -> Result<f64, ExprError>;
pub fn eval_text(text: &str, want: ParamUnit, lookup: &dyn Fn(&str) -> Option<(f64, ParamUnit)>) -> Result<f64, ExprError>;
pub fn plain_number(text: &str) -> Option<f64>;   // genau wie planora_app::sketch_view::parse_number
pub fn number_text(v: f64) -> String;             // v.to_string() mit Komma: verlustfrei hin und zurück, nie "-0" (kann > 200 Zeichen sein)
pub fn value_text(v: f64) -> Result<String, ExprError>; // number_text, aber nur endlich und ≤ 200 Zeichen (für Formeln, z. B. „Durch den Wert ersetzen“), sonst Meldung „Wert zu lang“
pub fn short_number(v: f64) -> String;            // höchstens 2 Nachkommastellen, Nullen weg, Komma, nie "-0"
pub fn check_name(name: &str) -> Result<(), ExprError>;
pub fn same_name(a: &str, b: &str) -> bool;       // a.to_lowercase() == b.to_lowercase()
pub fn rename_in(text: &str, old: &str, new: &str) -> String;
pub fn references(text: &str, name: &str) -> bool;
pub struct ParamDef<'a> { pub name: &'a str, pub formula: &'a str, pub unit: ParamUnit }
#[derive(Clone, Debug, PartialEq)] pub struct TableError { pub index: usize, pub message: String }
pub fn evaluate_table(defs: &[ParamDef<'_>]) -> Result<Vec<f64>, TableError>;
```
Re-Export in `lib.rs`: `pub use expr::{MAX_COMMENT_CHARS, MAX_FORMULA_CHARS, MAX_FORMULA_DEPTH, MAX_PARAMETERS, MAX_PARAM_NAME_CHARS, ParamUnit};`

Grammatik (absteigender Parser): `sum := prod (('+'|'-') prod)*`, `prod := unary (('*'|'/') unary)*`, `unary := '-' unary | power`, `power := atom ('^' unary)?` (rechtsassoziativ, `-2^2 = -4`, `2^-1 = 0,5`), `atom := zahl | name | name '(' args ')' | '(' sum ')'`; Argumente getrennt durch „;“ oder ein Komma, das nicht zwischen zwei Ziffern steht. Zahl: Ziffern mit höchstens einem `.`/`,` vor weiteren Ziffern, oder `.` Ziffern. Name: erstes Zeichen `char::is_alphabetic` oder `_`, danach `is_alphanumeric` oder `_`. Reservierte Namen (klein verglichen): `sqrt sin cos tan abs min max round pi`. `round` rundet halb weg von null (`f64::round`). Zykluserkennung in `evaluate_table` **ohne Rekursion** (Kahn oder eigener Stapel), damit eine Kette von 500 Parametern keinen Stapel sprengt; Stelle in Meldungen 1-basiert in Zeichen.

**Meldungstabelle (wörtlich, alle Aufgaben benutzen sie):**

| Fall | Text |
|---|---|
| leer | `Die Formel ist leer.` |
| zu lang | `Die Formel ist zu lang (höchstens 200 Zeichen).` |
| zu tief | `Die Formel ist zu tief verschachtelt (höchstens 64 Ebenen).` |
| Ende | `Die Formel endet unerwartet.` |
| Zeichen | `Unerwartetes Zeichen „{c}“ an Stelle {n}.` |
| Klammer | `Es fehlt eine schließende Klammer.` |
| Name fehlt | `Den Namen „{name}“ gibt es nicht.` |
| Funktion fehlt | `Die Funktion „{name}“ gibt es nicht.` |
| ein Wert | `„{f}“ braucht genau einen Wert.` |
| zwei Werte | `„{f}“ braucht mindestens zwei Werte (Werte mit „;“ oder „, “ trennen).` |
| durch null | `Division durch null.` |
| Wurzel negativ | `Die Wurzel einer negativen Zahl gibt es nicht.` |
| Wurzel Einheit | `„sqrt“ passt hier nicht zur Einheit.` |
| Potenz Einheit | `Eine Länge oder ein Winkel lässt sich nur mit einer ganzen Zahl potenzieren.` |
| Winkelfunktion | `„{f}“ braucht einen Winkel oder eine Zahl.` |
| Mischung | `Länge und Winkel lassen sich nicht zusammenrechnen.` |
| Einheiten ungleich (Fix-Runde Aufgabe 1) | `Die Einheiten passen nicht zusammen: {A} und {B} lassen sich nicht zusammenrechnen.` mit A, B ∈ {„Länge“, „Fläche“, „Rauminhalt“, „Winkel“, „eine andere Einheit“} (linker, dann rechter Wert; Länge mit Winkel bleibt „Mischung“) |
| Einheiten ungleich, beide unbenannt | `Die Einheiten passen nicht zusammen.` |
| Exponent mit Einheit (Fix-Runde Aufgabe 1) | `Der Exponent darf keine Einheit haben.` (`2^Breite`; „Potenz Einheit“ bleibt für `Breite^1,5`) |
| Wert zu lang (Fix-Runde Aufgabe 1) | `Der Wert lässt sich nicht als Zahl mit höchstens 200 Zeichen schreiben.` (`value_text`: nicht endlich oder Zahltext > 200 Zeichen, etwa ab 1e200 oder unter 1e-190) |
| nicht endlich | `Das Ergebnis ist zu groß oder keine Zahl.` |
| Einheit | `Hier wird {soll} erwartet, die Formel ergibt {ist}.` mit soll ∈ {„eine Länge“, „ein Winkel“, „eine Zahl“}, ist ∈ {„eine Länge“, „einen Winkel“, „eine Zahl“, „eine andere Einheit“} |
| Name leer | `Der Name darf nicht leer sein.` |
| Name Form | `Ein Name beginnt mit einem Buchstaben oder „_“ und enthält nur Buchstaben, Ziffern und „_“.` |
| Name reserviert | `„{name}“ ist schon eine Funktion oder Konstante.` |
| Name lang | `Der Name ist zu lang (höchstens 64 Zeichen).` |
| Name doppelt | `Den Namen „{name}“ gibt es schon.` |
| zu viele | `Höchstens 500 Parameter.` |
| Zyklus | `Die Parameter „a“, „b“ hängen im Kreis voneinander ab.` (Reihenfolge der Tabelle) |
| Selbstbezug | `Der Parameter „{name}“ hängt von sich selbst ab.` |
| Kommentar | `Der Kommentar ist zu lang (höchstens 200 Zeichen).` |

- [ ] **Step 1: Failing tests** (`ev(t)` = `eval_text(t, ParamUnit::Number, &|_| None).unwrap()`; `sc` = Nachschlagen mit `Breite=(40, Length)`, `w=(30, Angle)`, `k=(2, Number)`, klein verglichen):
  (a) `precedence_and_associativity`: `ev("2+3*4") == 14`, `ev("(2+3)*4") == 20`, `ev("10-4-3") == 3`, `ev("8/4/2") == 1`, `ev("2^3^2") == 512`, `ev("-2^2") == -4`, `ev("(-2)^2") == 4`, `ev("2^-1") == 0.5`, `ev("--3") == 3`, `ev("2*-3") == -6`, `ev(" 2 * ( 3 + 4 ) ") == 14`;
  (b) `functions_pi_and_commas`: `ev("sqrt(16)") == 4`, `ev("abs(-2,5)") == 2.5`, `ev("min(3; 1)") == 1`, `ev("max(3, 1)") == 3`, `ev("min(4; 2; 3)") == 2`, `ev("round(2,5)") == 3`, `ev("round(-2,5)") == -3`, `|ev("sin(30)") - 0.5| < 1e-12`, `|ev("cos(60)") - 0.5| < 1e-12`, `|ev("tan(45)") - 1| < 1e-12`, `ev("pi") == PI`, `ev("PI") == PI`; `min(1,5)` → Fehler = Zeile „zwei Werte“ mit `f = min`; `sin(1; 2)` → „„sin“ braucht genau einen Wert.“;
  (c) `numbers_round_trip`: `ev("1,5+1.5") == 3`; `plain_number("2,5") == Some(2.5)`, `plain_number(" -4 ") == Some(-4.0)`, `plain_number("1e3") == Some(1000.0)`, `plain_number("2*3") == None`, `plain_number("inf") == None`, `plain_number("") == None`; für `v` in `[0.1, 1.0/3.0, -12.5, 1e-7, 123456.789, 0.0]`: `plain_number(&number_text(v)) == Some(v)` bitgleich; `number_text(2.5) == "2,5"`; `short_number(0.0) == "0"`, `short_number(12.5) == "12,5"`, `short_number(1.0/3.0) == "0,33"`, `short_number(-0.001) == "0"`, `short_number(40.0) == "40"`;
  (d) `units`: mit `sc`: `eval_text("Breite/4", Length) == 10`, `"breite*k"` → 80, `"Breite+2"` → 42, `"sqrt(Breite^2 + 30^2)"` → 50, `"sin(w)*Breite"` → 20 (±1e-12), `"w*2"` als Angle → 60, `"Breite/Breite"` als Number → 1, `"k"` als Length → 2; Fehler wörtlich: `"w"` als Length → „Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.“, `"Breite"` als Angle → „Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.“, `"Breite"` als Number → „Hier wird eine Zahl erwartet, die Formel ergibt eine Länge.“, `"Breite*Breite"` als Length → „… ergibt eine andere Einheit.“, `"Breite+w"` → Zeile „Mischung“, `"min(Breite; w)"` → „Mischung“, `"sin(Breite)"` → „„sin“ braucht einen Winkel oder eine Zahl.“, `"sqrt(Breite)"` → „Wurzel Einheit“, `"Breite^1,5"` → „Potenz Einheit“;
  (e) `errors_and_limits`: `"1/0"` → „Division durch null.“, `"sqrt(-1)"` → „Wurzel negativ“, `"10^400"` → „nicht endlich“, `""` und `"   "` → „leer“, `"2+"` → „Ende“, `"(2+3"` → „Klammer“, `"2+3)"` → „Unerwartetes Zeichen „)“ an Stelle 4.“, `"2 # 3"` → „Unerwartetes Zeichen „#“ an Stelle 3.“, `"Breit"` → „Den Namen „Breit“ gibt es nicht.“, `"foo(2)"` → „Die Funktion „foo“ gibt es nicht.“; Formel aus 200 Zeichen (`"1+"` × 99 + `"1"` + ein Leerzeichen) ok, mit 201 → „zu lang“; 64 Klammerpaare um `1` ok, 65 → „zu tief“; `"-"` × 65 + `"1"` → „zu tief“ (kein Stapelüberlauf);
  (f) `names`: `check_name` ok für „Breite“, „_a1“, „Höhe“, „Maß“, „Ärger“; Fehler: „“ → „Name leer“, „1a“, „a-b“, „a b“ → „Name Form“, „sin“, „PI“, „Sqrt“ → „„PI“ ist schon eine Funktion oder Konstante.“ (Name wie getippt), 65 × „a“ → „Name lang“, 64 × „a“ ok; `same_name("Breite", "BREITE")`; Nachschlagen klein verglichen (`"breite"` mit `sc` → 40);
  (g) `rename_tokens`: `rename_in("Breite/4 + breite*2 - Breite2 + Breitex", "Breite", "B") == "B/4 + B*2 - Breite2 + Breitex"`; `rename_in("Breite +* 2", "Breite", "B") == "B +* 2"` (nicht zerlegbar, trotzdem ersetzt); `rename_in("min(1,5; Breite)", "Breite", "Weite") == "min(1,5; Weite)"`; `references("min(Breite, 2)", "breite")`, `!references("Breite2", "Breite")`, `!references("2*3", "Breite")`;
  (h) `table`: `[a="b+1" L, b="2" L]` → `[3, 2]`; `[a="b+1", b="a"]` → `TableError { index: 0, message: "Die Parameter „a“, „b“ hängen im Kreis voneinander ab." }`; `[a="a+1"]` → „Der Parameter „a“ hängt von sich selbst ab.“; `[Breite, breite]` → `index 1`, „Den Namen „breite“ gibt es schon.“; `[a="c"]` → `index 0`, „Den Namen „c“ gibt es nicht.“; `[w="30" Angle, l="w" Length]` → `index 1`, Einheitenmeldung; 501 Einträge → `index 500`, „Höchstens 500 Parameter.“; Kette `p1="1"`, `p_i = "p_{i-1}+1"` bis `p500` in umgekehrter Reihenfolge → `p500 == 500`.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test expr -j 2`
- [ ] **Step 3: Implement** nach Interfaces, Grammatik und Meldungstabelle.
- [ ] **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: `^` linksassoziativ; unäres Minus bindet stärker als `^`; Komma immer Dezimalzeichen; Komma nie Dezimalzeichen; `round` als `round_ties_even`; Grad nicht in Bogenmaß umgerechnet; Zahl passt nicht zu Länge bei `+`; Länge + Winkel erlaubt; Tiefe nicht gezählt bei unärem Minus; Grenze `>` statt `>=` bei 200 Zeichen; `rename_in` als Textsuche (`replace`); Namensvergleich mit Groß-/Kleinschreibung; Zyklus als „unbekannter Name“ gemeldet; `short_number` gibt „-0“.
- [ ] **Step 6: Commit** — `feat: Formelbaustein für Parameter (rein, ohne Oberfläche)`.

### Task 2: Parametertabelle im Dokument

**Files:**
- Create: `crates/planora-feature/src/params.rs` (Tabelle, `ParamScope`, Änderungen)
- Modify: `crates/planora-feature/src/model.rs` (`Document` bekommt `parameters: Vec<Parameter>`, `param_values: Vec<f64>`, `next_param: u64` (beginnt bei 1)), `crates/planora-feature/src/snapshot.rs` (`Snapshot.parameters`, `Snapshot.next_param`; `snapshot`, `same_model`, `build`), `crates/planora-feature/src/error.rs` (zwei Varianten), `crates/planora-feature/src/lib.rs`, `crates/planora-feature/tests/{dimensions.rs, drawings.rs}` (Literale, siehe Global Constraints)
- Test: `crates/planora-feature/tests/parameters.rs` (neu)

**Interfaces:**
- Consumes: `expr::{evaluate_table, eval_text, check_name, same_name, rename_in, references, number_text, ParamUnit, MAX_*}` (1).
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)] pub struct ParamId(pub u64);
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter { pub id: ParamId, pub name: String, pub formula: String, pub unit: ParamUnit, pub comment: String }
#[derive(Clone, Debug, PartialEq)]
pub struct ParamEdit { pub name: String, pub formula: String, pub unit: ParamUnit, pub comment: String }
#[derive(Clone, Debug, Default)]
pub struct ParamScope { /* (name klein, Wert, Art) je Parameter */ }
impl ParamScope {
    pub fn lookup(&self, name: &str) -> Option<(f64, ParamUnit)>;
    pub fn eval(&self, text: &str, want: ParamUnit) -> Result<f64, String>;
    pub fn names(&self) -> Vec<String>;   // in Tabellenreihenfolge, wie geschrieben
}
// error.rs:
#[error("{0}")] Formula(String),
#[error("{name} wird in {} benutzt.", .users.join(", "))] ParameterInUse { name: String, users: Vec<String> },
impl Document {
    pub fn parameters(&self) -> &[Parameter];
    pub fn parameter(&self, id: ParamId) -> Option<&Parameter>;
    pub fn parameter_value(&self, id: ParamId) -> Option<f64>;
    pub fn param_scope(&self) -> ParamScope;
    pub fn add_parameter(&mut self, name: &str, formula: &str, unit: ParamUnit, comment: &str) -> Result<ParamId, FeatureError>;
    pub fn add_parameter_default(&mut self) -> Result<ParamId, FeatureError>;            // P13
    pub fn set_parameter(&mut self, id: ParamId, edit: ParamEdit) -> Result<RecomputeReport, FeatureError>; // Umbenennen inklusive
    pub fn remove_parameter(&mut self, id: ParamId) -> Result<(), FeatureError>;        // gesperrt bei Benutzern
    pub fn replace_parameter_by_value(&mut self, id: ParamId) -> Result<(), FeatureError>; // P7
    pub fn parameter_users(&self, id: ParamId) -> Vec<String>;   // direkte Benutzer; in dieser Aufgabe nur "Parameter {name}"
}
pub struct Snapshot { /* … */ pub parameters: Vec<Parameter>, pub next_param: u64 }
```
Regeln: Jede gelungene Änderung erhöht `edits` um genau 1; eine Änderung ohne Unterschied (gleiche `ParamEdit`) ist `Ok` ohne Zähler und ohne Neuberechnung. Jede Änderung baut die neue Tabelle als Kopie, prüft sie mit `evaluate_table` (Namensfehler ohne Vorsilbe, Formel- und Einheitenfehler als `Parameter „{name}“: {meldung}`), Kommentar ≤ 200 Zeichen, und übernimmt erst dann (`FeatureError::Formula`). Umbenennen ersetzt den Namen in allen Parameterformeln mit `rename_in`. `Snapshot::same_model` vergleicht zusätzlich `parameters` und `next_param`. `build` prüft Kennungen (`0 < id < next_param`, keine doppelte: „Ein Parameter hat eine ungültige Kennung“, „Eine Parameter-Kennung kommt doppelt vor“) und die Tabelle (`FeatureError::InvalidInput(format!("Parametertabelle: {meldung}"))`), bevor Features gebaut werden.

- [ ] **Step 1: Failing tests:**
  (a) `add_and_read`: `add_parameter("Breite", "40", Length, "Außenmaß") == Ok(ParamId(1))`, Wert 40; `add_parameter("Tiefe", "Breite/2", Length, "")` → Wert 20; `edit_count` je +1; `param_scope().lookup("TIEFE") == Some((20.0, Length))`; `names() == ["Breite", "Tiefe"]`;
  (b) `rejected_edits_change_nothing` (je Fall: `snapshot().same_content(&vorher)` und `edit_count` gleich): Name „breite“ → `Formula("Den Namen „breite“ gibt es schon.")`; Breite-Formel „Tiefe+1“ → Meldung enthält „im Kreis“; Breite auf Winkel → `Formula("Parameter „Tiefe“: Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.")`; Name „1x“; Formel mit 201 Zeichen; Kommentar mit 201 Zeichen → Zeile „Kommentar“; 501. Parameter → „Höchstens 500 Parameter.“;
  (c) `rename_rewrites_parameter_formulas`: Breite → Weite: Tiefe-Formel `"Weite/2"`, Werte gleich, `edit_count` + 1;
  (d) `remove_blocked_and_replace`: `remove_parameter(Breite)` → `ParameterInUse { name: "Breite", users: ["Parameter Tiefe"] }`, Text „Breite wird in Parameter Tiefe benutzt.“; `replace_parameter_by_value(Breite)` → Breite fehlt, Tiefe-Formel `"20"`, Wert 20, `edit_count` + 1; unbenutzten Parameter löschen ok;
  (e) `snapshot_roundtrip_and_restore`: Bild A mit 2 Parametern → `from_snapshot(A)` hat gleiche Tabelle und Werte, `next_param` gleich; Breite ändern → `!snapshot().same_model(&A)`; `restore(A) == Ok(RestoreKind::Full)`, Tabelle und Werte wie A, `edit_count` größer als davor;
  (f) `build_rejects_invalid_table`: Bild mit Namen „a“ und „A“ → `Err`, Text enthält „Parametertabelle“ und „gibt es schon“; mit Zyklus → enthält „im Kreis“; Kennung 0 bzw. ≥ `next_param` → „Ein Parameter hat eine ungültige Kennung“; doppelte Kennung → „Eine Parameter-Kennung kommt doppelt vor“;
  (g) `no_op_edit_counts_nothing`: `set_parameter` mit unveränderter `ParamEdit` → `Ok`, `edit_count` gleich;
  (h) `default_rows`: zweimal `add_parameter_default` → „Parameter1“, „Parameter2“ (Formel „10“, Länge, Kommentar leer); „Parameter1“ löschen → nächste Vorgabe wieder „Parameter1“; vorhandenes „parameter3“ → nach „Parameter1“/„Parameter2“ kommt „Parameter4“.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test parameters -j 2`
- [ ] **Step 3: Implement** nach Interfaces und Regeln.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-feature -j 2`, `cargo test -p planora-project -j 2` (grün; `planora-project` nur über `Snapshot`-Literal berührt, Konvertierung folgt in Aufgabe 7: in `decode_version` vorerst `parameters: Vec::new(), next_param: 1`), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Prüfung nach statt vor dem Übernehmen; `edits` bei No-op erhöht; `edits` doppelt bei Umbenennen; Umbenennen ohne Umschreiben; `same_model` ohne `parameters`; `same_model` ohne `next_param`; Löschen nicht gesperrt; Ersetzen schreibt Formel statt Wert; Kennung wiederverwendet; `build` ohne Tabellenprüfung; Kommentargrenze nicht geprüft; Vorgabename mit Groß-/Kleinschreibung verglichen.
- [ ] **Step 6: Commit** — `feat: Parametertabelle im Dokument (Rückgängig über das Zustandsbild)`.

### Task 3: Formeln in Körper-Features (Nebentabelle, Auswertungsdurchgang)

**Files:**
- Create: `crates/planora-feature/src/formulas.rs` (`FieldKey`, Feldzugriff, Durchgang, strenge Prüfung)
- Modify: `crates/planora-feature/src/model.rs` (`Feature.formulas: Formulas`, `Feature.issue: Option<FormulaIssue>` (pub(crate)), `Feature::formulas()`), `crates/planora-feature/src/snapshot.rs` (`SnapshotFeature.formulas`; `build` wertet mild aus), `crates/planora-feature/src/ops.rs` (`update_with_report` nach P2; `check_invariants`: Feature vor dem Marker mit `issue` und nicht unterdrückt hat `Error`), `crates/planora-feature/src/recompute.rs` (`add_feature_with_formulas`; Durchgang vor `recompute_features`; `issue` → `Error(meldung)` ohne Ergebnis, `failed = None`), `crates/planora-feature/src/params.rs` (Benutzer, Umbenennen, Ersetzen, strenge Prüfung auch über Features), `crates/planora-feature/src/lib.rs`, `crates/planora-project/src/convert.rs` und `crates/planora-app/tests/history.rs` (nur `SnapshotFeature`-Literale)
- Test: `crates/planora-feature/tests/formulas.rs` (neu)

**Interfaces:**
- Consumes: `ParamScope`, `Document::param_scope`, `ParameterInUse`, `Formula` (2); `expr::{eval_text, rename_in, references, short_number}` (1).
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldKey { Breite, Tiefe, Hoehe, Radius, X, Y, Z, Abstand, Abstand1, Abstand2, Winkel, Massstab }
pub type Formulas = std::collections::BTreeMap<FieldKey, String>;
impl FieldKey {
    pub fn file_key(self) -> &'static str;                 // "breite","tiefe","hoehe","radius","x","y","z","abstand","abstand1","abstand2","winkel","massstab"
    pub fn from_file_key(s: &str) -> Option<FieldKey>;
}
pub fn field_keys(kind: &FeatureKind) -> Vec<FieldKey>;    // Reihenfolge wie im Fenster „bearbeiten“
pub fn field_label(kind: &FeatureKind, key: FieldKey) -> &'static str;
pub fn field_unit(kind: &FeatureKind, key: FieldKey) -> ParamUnit;
pub fn field_value(kind: &FeatureKind, key: FieldKey) -> Option<f64>;
pub fn set_field_value(kind: &mut FeatureKind, key: FieldKey, v: f64) -> bool;
#[derive(Clone, Debug, PartialEq)] pub(crate) enum FormulaIssue { Broken(String), Value(String) }
pub struct SnapshotFeature { /* … */ pub formulas: Formulas }
impl Feature { pub fn formulas(&self) -> &Formulas; }
impl Document {
    pub fn add_feature_with_formulas(&mut self, name: impl Into<String>, kind: FeatureKind, formulas: Formulas) -> Result<FeatureId, FeatureError>;
    pub fn update_with_formulas(&mut self, id: FeatureId, f: impl FnOnce(&mut FeatureKind), formulas: Formulas) -> Result<RecomputeReport, FeatureError>; // ersetzt die ganze Tabelle des Features
}
```
In dieser Aufgabe deckt `field_keys` Quader (Breite, Tiefe, Hoehe, X, Y, Z), Zylinder/Kegel (Radius, Hoehe, X, Y, Z) und Kugel (Radius, X, Y, Z) ab; alle anderen Arten liefern vorerst `[]` (Aufgaben 4–6 ergänzen). Bezeichnungen wie `feature_dialog::numbers_fixed`: „Breite“, „Tiefe“, „Höhe“, „Radius“, „Position x/y/z“. Einheit Länge.

Durchgang (P1, P3): für jedes Feature in Zeitleistenreihenfolge jede Formel mit `ParamScope::eval(text, field_unit)`; Fehler → `Broken(meldung)`; Wert ≤ `kernel.tolerance.linear` bei Breite/Tiefe/Höhe/Radius → `Value(format!("{label} ergibt {} mm, es muss größer als 0 sein", short_number(v)))` (Wert wird trotzdem geschrieben); sonst Wert schreiben. Ein Feature mit geändertem Wert (bitweise) oder geänderter `issue` bekommt `bump()` als neue Version. Strenge Prüfung bei `add_feature_with_formulas`, `update_with_formulas` und allen Parameteränderungen: Kandidat auf einer Kopie auswerten; ein neues `Broken` → `Err(FeatureError::Formula(format!("{feature}, {label}: {meldung}")))`, nichts geändert. Schlüssel, die `field_keys(kind)` nicht enthält → `FeatureError::InvalidInput("Dieses Feld kann keine Formel haben".into())`. `parameter_users` listet zuerst Features (Name, Zeitleistenreihenfolge, Formel nennt den Parameter direkt), dann „Parameter {name}“. Umbenennen und Ersetzen (P7) gelten auch für Feature-Formeln.

- [ ] **Step 1: Failing tests** (Parameter Breite = 40, Tiefe = „Breite/2“):
  (a) `box_follows_parameter`: `add_feature_with_formulas("Quader 1", Box { size: (1,1,1), origin: 0 }, {Breite: "Breite", Tiefe: "Tiefe", Hoehe: "Breite/4", X: "-Breite/2"})` → `size == (40, 20, 10)`, `origin.x == -20`, Hüllbox des Körpers (40, 20, 10) ±1e-9; Breite = 60 → `size == (60, 30, 15)`, `origin.x == -30`, Hüllbox folgt, Bericht `computed` enthält den Quader, `edit_count` + 1;
  (b) `cylinder_cone_sphere_follow`: d = 10: Zylinder Radius „d/2“ Höhe „d“ → (5, 10); Kegel Höhe „2*d“ → 20; Kugel Radius „d“ → Hüllbox 20 × 20 × 20 ±1e-6; `field_keys` je Art in der Reihenfolge oben, Kugel 4, Quader 6, Zylinder 5;
  (c) `invalid_formula_rejected`: Quader-Breite „Breit“ → `Formula("Quader 1, Breite: Den Namen „Breit“ gibt es nicht.")`; mit Winkel-Parameter w → „Quader 1, Breite: Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.“; Schlüssel `Winkel` am Quader → `InvalidInput`, Text „Ungültige Eingabe: Dieses Feld kann keine Formel haben“; jeweils Art, Formeln, Version und `edit_count` gleich;
  (d) `bad_value_is_error_status`: Quader-Breite „Breite-40“ (Wert 0 erst durch Parameteränderung: Breite erst 50, dann 40) → `set_parameter` `Ok`, Status `Error("Breite ergibt 0 mm, es muss größer als 0 sein")`, kein Körper, folgendes Verschieben `Skipped("abhängig von Feature „Quader 1“ (Fehler)")`; Breite 50 → beide `Ok`; Formel „100/(Breite-40)“ und Breite → 40 → `Err(Formula("Quader 1, Breite: Division durch null."))`, Tabelle unverändert; `check_invariants` jeweils ok;
  (e) `plain_update_clears_only_changed_field` (P2): Formeln Breite und Tiefe; `update(id, |k| size.x = 7)` → `formulas() == {Tiefe: "Tiefe"}`; Breite = 80 → `size.x == 7`, `size.y == 40`;
  (f) `rename_users_replace_with_features`: Umbenennen Breite → Weite → Quader-Formeln „Weite“, „-Weite/2“, „Weite/4“; `parameter_users(Weite) == ["Quader 1", "Parameter Tiefe"]`; Löschen → „Weite wird in Quader 1, Parameter Tiefe benutzt.“; `replace_parameter_by_value(Weite)` → Quader behält nur `{Tiefe: "Tiefe"}`, alle Werte gleich, `edit_count` + 1;
  (g) `no_value_change_no_recompute`: Kommentar ändern bzw. unbenutzten Parameter ändern → Bericht `computed` leer, Version des Quaders gleich;
  (h) `load_with_broken_formula`: Bild mit Quader `size.x = 12`, Formel `{Breite: "Gibtsnicht"}` → `from_snapshot` `Ok`, Status `Error("Den Namen „Gibtsnicht“ gibt es nicht.")`, `size.x == 12`; danach `set_parameter` eines anderen Parameters `Ok` (alter Fehler geduldet, P3); `update_with_formulas` mit „Breite“ → Status `Ok`;
  (i) `snapshot_carries_formulas`: `snapshot().features[0].formulas` gleich der Tabelle; Bild B, das sich nur im Formeltext unterscheidet („Breite“ statt „breite“), ist `!same_model`; `restore(B) == Ok(Full)`, `same_content`.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test formulas -j 2`
- [ ] **Step 3: Implement** nach Interfaces und Regeln.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-feature -j 2`, `cargo test -p planora-project -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Version auch ohne Wertänderung erhöht; Version bei Wertänderung nicht erhöht; Wertfehler als Ablehnung statt Status; alter `Broken`-Fehler blockiert jede Änderung; `update` löscht alle Formeln; `update` löscht keine; Schlüsselprüfung fehlt; `issue` ohne Fehlerstatus; Durchgang nach statt vor `recompute_features`; `parameter_users` ohne Features; Umbenennen ohne Feature-Formeln; Ersetzen lässt Formel stehen; `same_model` ohne Formeln.
- [ ] **Step 6: Commit** — `feat: Formeln in Quader, Zylinder, Kegel und Kugel`.

### Task 4: Formeln in Extrusion, Verschieben und Netz-Körper

**Files:**
- Modify: `crates/planora-feature/src/formulas.rs` (Arme für `Extrude`, `Move`, `MeshBody`)
- Test: `crates/planora-feature/tests/formulas_ops.rs` (neu; Netz-Hilfe nach Vorbild `tests/mesh_bodies.rs`)

**Interfaces:**
- Consumes: `field_keys`, `field_label`, `field_unit`, `set_field_value`, Durchgang (3); `MIN_MESH_SCALE`, `MAX_MESH_SCALE`.
- Produces: `field_keys(Extrude) == [Hoehe]` („Höhe“, Länge, > Toleranz wie Aufgabe 3); `field_keys(Move) == [X, Y, Z]` („Δx“, „Δy“, „Δz“, Länge, beliebiges Vorzeichen); `field_keys(MeshBody) == [X, Y, Z, Massstab]` („Position x/y/z“, Länge; „Maßstab“, `ParamUnit::Number`, Wertfehler außerhalb `MIN_MESH_SCALE..=MAX_MESH_SCALE`: `format!("Maßstab ergibt {}, er muss zwischen 0,000001 und 1 000 000 liegen", short_number(v))`).

- [ ] **Step 1: Failing tests:**
  (a) `extrude_height_formula`: Rechteck 10 × 10 auf XY, Extrusion mit `{Hoehe: "h*2"}`, h = 5 → Hüllbox z 10 ±1e-9; h = 0 → Status `Error("Höhe ergibt 0 mm, es muss größer als 0 sein")`; h = 7 → `Ok`, z 14;
  (b) `move_delta_formulas`: Quader 10³ im Ursprung, Verschieben `{X: "a", Y: "-a", Z: "0"}`, a = 15 → Hüllbox-Minimum (15, −15, 0); a = −5 → (−5, 5, 0); `field_label(Move, X) == "Δx"`;
  (c) `mesh_offset_and_scale`: Netz-Körper `{X: "a", Massstab: "k"}`, a = 3, k = 2 (Zahl) → `mesh_transform()` mit `offset.x == 3`, `scale == 2`; k als Länge → `Formula("Netz-Körper 1, Maßstab: Hier wird eine Zahl erwartet, die Formel ergibt eine Länge.")`, nichts geändert; k = 0 → Status `Error("Maßstab ergibt 0, er muss zwischen 0,000001 und 1 000 000 liegen")`;
  (d) `restore_brings_back_table_and_values`: Bild vor einer Parameteränderung, danach `restore(bild)` → Parameterwert, `delta`, Hüllboxen wie vorher;
  (e) `keys_and_labels`: `field_keys` und `field_label` für alle drei Arten wie oben; `field_unit(MeshBody, Massstab) == Number`.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test formulas_ops -j 2`
- [ ] **Step 3: Implement** die drei Arme.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-feature -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Extrusionshöhe ohne Positivprüfung; Verschieben mit Positivprüfung; X/Y vertauscht; Maßstab als Länge; Maßstabgrenzen offen; Grenzen `<` statt `<=`; Netz-Lage z fehlt; Bezeichnung „Position x“ beim Verschieben; Meldung mit „mm“ beim Maßstab; `MeshBody` liefert `[]`.
- [ ] **Step 6: Commit** — `feat: Formeln in Extrusion, Verschieben und Netz-Körper`.

### Task 5: Formeln in Rundungen und Fasen

**Files:**
- Modify: `crates/planora-feature/src/formulas.rs` (Arm `Verrunden`), `crates/planora-feature/src/model.rs` (`pub const MIN_CHAMFER_ANGLE_DEG: f64 = 1.0; pub const MAX_CHAMFER_ANGLE_DEG: f64 = 179.0;`, Re-Export in `lib.rs`)
- Test: `crates/planora-feature/tests/formulas_blend.rs` (neu; Vorbild `tests/blend_features.rs`)

**Interfaces:**
- Consumes: Aufgabe 3.
- Produces: `field_keys` für `Rundung` `[Radius]`, `FaseGleich` `[Abstand]`, `FaseZwei` `[Abstand1, Abstand2]` („Abstand 1“, „Abstand 2“), `FaseWinkel` `[Abstand, Winkel]` (Winkel `ParamUnit::Angle`, Grad), `RundungVariabel` `[]`. Wertfehler: Längen ≤ 0 → wie Aufgabe 3 („Radius ergibt 0 mm, …“); Winkel außerhalb `[1, 179]` → `format!("Winkel ergibt {}°, er muss zwischen 1° und 179° liegen", short_number(v))`. Kleinstmaß des Kerns bleibt eine Prüfung des Kerns (Fehlerstatus mit dessen Meldung).

- [ ] **Step 1: Failing tests** (Quader 20³, eine Kante):
  (a) `fillet_radius_follows`: Rundung `{Radius: "r"}`, r = 2 → `Ok`, `radius == 2`; r = 3 → `Ok`, `radius == 3`, Bericht `computed` enthält die Rundung; r = 0 → `Error("Radius ergibt 0 mm, es muss größer als 0 sein")`;
  (b) `chamfer_kinds`: Fase zwei `{Abstand1: "a", Abstand2: "a*2"}` a = 1 → (1, 2); Fase mit Winkel `{Abstand: "a", Winkel: "w"}` mit w = 45 (Winkel) → `winkel == 45`; Winkel mit Länge → `Formula("Fase 1, Winkel: Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.")`; w = 0 und w = 200 → `Error("Winkel ergibt 0°, er muss zwischen 1° und 179° liegen")` bzw. „… 200° …“;
  (c) `art_switch_drops_missing_keys`: Fase zwei mit Formeln Abstand1/Abstand2, `update(id, |k| art = FaseGleich { abstand: 1.0 })` → `formulas()` leer; Fase mit Winkel → Fase gleich behält `{Abstand}`;
  (d) `variable_fillet_has_no_fields`: `field_keys(RundungVariabel) == []`; `update_with_formulas` mit `Radius` → `InvalidInput`;
  (e) `parameter_breaks_blend_then_back`: r = 50 → Rundung `Error(_)` (Meldung des Kerns), Quader `Ok`; Bild davor `restore` → Rundung `Ok`, r = 2.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test formulas_blend -j 2`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-feature -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Winkel als Länge; Winkel in Bogenmaß gespeichert; Grenzen 0/180 statt 1/179; Abstand2 auf Abstand1 geschrieben; variable Rundung mit Feldern; Artwechsel behält alte Schlüssel; Rundungsradius ohne Positivprüfung; Fase gleich auf `Radius` abgebildet; Bezeichnung „Abstand1“ ohne Leerzeichen; `restore` ohne Neuauswertung.
- [ ] **Step 6: Commit** — `feat: Formeln in Rundungen und Fasen`.

### Task 6: Formeln in Skizzenmaßen mit Solverstatus

**Files:**
- Modify: `crates/planora-feature/src/model.rs` (`SketchFeature.formulas: SketchFormulas`, `PartialEq` vergleicht sie zusätzlich, `prune_formulas`), `crates/planora-feature/src/formulas.rs` (Skizzenzweig des Durchgangs), `crates/planora-feature/src/compute.rs` (`compute_sketch`: Status bei Formeln, P12), `crates/planora-feature/src/params.rs` (Benutzer/Umbenennen/Ersetzen für Skizzen), `crates/planora-feature/src/ops.rs` (`update_with_report` ruft `prune_formulas` für Skizzen), die 36 `SketchFeature`-Literale (Global Constraints)
- Test: `crates/planora-feature/tests/formulas_sketch.rs` (neu)

**Interfaces:**
- Consumes: Aufgabe 3; `planora_sketch::{ConstraintId, ConstraintKind, Sketch::{constraint, set_constraint_value, check_constraint_value, solve}, SolveStatus}` (unverändert).
- Produces:
```rust
pub type SketchFormulas = std::collections::BTreeMap<ConstraintId, String>;
pub struct SketchFeature { pub sketch: Sketch, pub plane: SketchPlane, pub copies: Vec<EdgeCopy>, pub formulas: SketchFormulas }
impl SketchFeature {
    /// Entfernt Formeln an fehlenden, nicht maßgebenden (`!is_dimension()`) oder anzeigenden (`driven`) Abhängigkeiten; `true`, wenn sich etwas änderte.
    pub fn prune_formulas(&mut self) -> bool;
}
pub fn constraint_unit(kind: &ConstraintKind) -> ParamUnit;   // Angle/EllipseAngle → Angle, sonst Length
```
Durchgang: je Formel `ParamScope::eval(text, constraint_unit)`; Winkel in Grad → `to_radians()`; `with_value` + `check_constraint_value`; Fehler der Prüfung → `Value(format!("Ein Maß ergibt {} {}: {}", short_number(anzeigewert), suffix, fehler))`; sonst `set_constraint_value` nur bei bitweise anderem Wert (die Skizzen-Revision ändert sich nur dann). Strenge Meldung: `format!("{skizze}: {meldung}")`. `compute_sketch`: ist `sf.formulas` nicht leer und `solve().status` ≠ `Solved`, dann `Err(FeatureError::Sketch(..))` mit „Mit den Werten der Formeln widersprechen sich Abhängigkeiten“ (`Conflicting`) bzw. „Mit den Werten der Formeln lässt sich die Skizze nicht lösen“ (`NotConverged`); ohne Formeln bleibt `let _ = sketch.solve();`.

- [ ] **Step 1: Failing tests** (Breite = 30):
  (a) `dimension_follows_parameter`: Rechteck mit Abstandsmaß der Unterseite 30 und Formel „Breite“, voll bestimmt, extrudiert; Breite = 40 → gespeicherter Maßwert 40, Unterseite in `sketch_of(id)` 40 ±1e-9, Hüllbox der Extrusion x 40 ±1e-6;
  (b) `angle_formula_in_degrees`: Winkelmaß mit Formel „w“, w = 60 (Winkel) → gespeicherter Wert `60f64.to_radians()` ±1e-12; Länge-Parameter → `Formula("Skizze 1: Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.")`;
  (c) `unsolvable_is_error_status`: Dreieck mit Abständen „a“, „b“, „c“ (30, 30, 40) → `Ok`; c = 100 → Skizze `Error`, Text beginnt mit „Fehler in der Skizze: Mit den Werten der Formeln“, Extrusion `Skipped(_)`; c = 40 → beide `Ok`;
  (d) `never_silently_wrong`: Rechteck-Breite „Breite“ 30 → 400 → entweder Status `Error(_)` oder (`Ok` und gemessene Breite == 400 ±1e-6); danach 30 → `Ok`, Breite 30 ±1e-6;
  (e) `prune_and_equality`: Formel an Abhängigkeit, `remove_constraint` → `prune_formulas() == true`, Tabelle leer; `set_driven(id, true)` → entfernt; nur Formeltext verschieden („Breite“ / „breite“) bei gleicher Revision → `!=`; Klon `==`; `update` mit Formel an anzeigendem Maß → gespeicherte Tabelle ohne sie;
  (f) `value_check_gives_error_status`: Abstandsformel „Breite-30“ → Status `Error`, Text enthält „Ein Maß ergibt 0 mm: “;
  (g) `users_rename_replace_sketch`: `parameter_users(Breite) == ["Skizze 1"]`; Umbenennen → Skizzenformel „Weite“, `SketchFeature` ungleich dem Bild davor; `replace_parameter_by_value` → Skizzentabelle leer, Maßwert gleich, `edit_count` + 1;
  (h) `sketch_without_formulas_unchanged`: ungelöste Skizze ohne Formeln behält Status `Ok` (wie heute).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test formulas_sketch -j 2`
- [ ] **Step 3: Implement**; danach die 36 Literale ergänzen (`grep -rn "SketchFeature {" crates --include=*.rs`).
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-feature -j 2`, `cargo test -p planora-project -j 2`, `cargo test -p planora-app -j 2` (nur Übersetzen der Literale), clippy, fmt; `git diff --stat 7f571c6 -- crates/planora-sketch` leer.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Solverstatus nicht ausgewertet; Status auch ohne Formeln ausgewertet; Grad nicht umgerechnet; `set_constraint_value` auch bei gleichem Wert (Revision springt, Zwischenspeicher verfehlt); `eq` ohne Formeln; `prune` behält anzeigende; `prune` behält fehlende; Wertprüfung übersprungen; Umbenennen ohne Skizzen; Ersetzen ohne Skizzen; `Conflicting` und `NotConverged` vertauscht.
- [ ] **Step 6: Commit** — `feat: Formeln in Skizzenmaßen, Solverstatus bei Formeln`.

### Task 7: Dateiformat 9

**Files:**
- Modify: `crates/planora-project/src/format.rs` (`VersionFile.parameter`, `VersionFile.naechster_parameter`, `ParameterDto`, `FeatureDto.formeln`, `SketchFeatureDto.formeln`, `SketchFormulaDto`), `crates/planora-project/src/convert.rs` (`FORMAT_VERSION = 9`, `required_format`, Schreiben, Lesen mit Prüfungen), die fünf `FORMAT_VERSION`-Tests (Global Constraints)
- Test: `crates/planora-project/tests/parameter_format.rs` (neu)

**Interfaces:**
- Consumes: `Snapshot.{parameters, next_param}`, `SnapshotFeature.formulas`, `SketchFeature.formulas`, `FieldKey::{file_key, from_file_key}`, `MAX_FORMULA_CHARS`, `MAX_PARAMETERS` (2, 3, 6).
- Produces:
```rust
pub const FORMAT_VERSION: u32 = 9;
// VersionFile:
#[serde(default, skip_serializing_if = "Vec::is_empty")] pub parameter: Vec<ParameterDto>,
#[serde(default, skip_serializing_if = "is_zero")] pub naechster_parameter: u64,
pub struct ParameterDto { pub id: u64, pub name: String, pub formel: String, pub art: String /* "laenge"|"winkel"|"zahl" */,
                          #[serde(default, skip_serializing_if = "String::is_empty")] pub kommentar: String }
// FeatureDto:
#[serde(default, skip_serializing_if = "BTreeMap::is_empty")] pub formeln: BTreeMap<String, String>,
// SketchFeatureDto:
#[serde(default, skip_serializing_if = "Vec::is_empty")] pub formeln: Vec<SketchFormulaDto>,
pub struct SketchFormulaDto { pub abhaengigkeit: usize, pub formel: String }   // Index in `abhaengigkeiten` == ConstraintId::index()
```
`required_format` gibt als **erste** Regel 9 zurück, wenn es Parameter, Feature-Formeln oder Skizzenformeln gibt (Kommentar der Funktion ergänzen). `naechster_parameter` wird nur mit Parametern geschrieben. Lesen: `format < 9` mit einem der neuen Felder → `corrupt("Parameter und Formeln gibt es erst ab Dateiformat 9")`; unbekannte Art → `corrupt("Parameter „{name}“: unbekannte Art „{art}“")`; unbekannter Schlüssel → `corrupt("„{feature}“: unbekanntes Formelfeld „{key}“")`; Skizzenformel an Index ohne Abhängigkeit → `corrupt("„{feature}“: Formel an einer fehlenden Abhängigkeit")`; Formel oder Name über den Grenzen, mehr als 500 Parameter → `corrupt` mit Klartext. Die Tabelle selbst (doppelte Namen, Zyklus) prüft `Document::from_snapshot` (Aufgabe 2); eine kaputte Feature-Formel lädt (Aufgabe 3).

- [ ] **Step 1: Failing tests:**
  (a) `format_9_only_with_parameters_or_formulas`: Quader-Dokument → `required_format == 1`; plus Parameter → 9; nur Feature-Formel „2*5“ ohne Parameter → 9; nur Skizzenformel → 9; `FORMAT_VERSION == 9`;
  (b) `roundtrip`: drei Parameter (Länge, Winkel, Zahl; einer mit Kommentar), Quader-Formeln, Skizzenformel an der dritten Abhängigkeit **nachdem** die erste gelöscht wurde, Fase-Winkel-Formel → `encode` → `decode` → `from_snapshot`: Tabelle, Formeln, Werte gleich; erneutes `encode` zeichengleich;
  (c) `json_shape`: `"parameter"` mit Schlüsseln `id`, `name`, `formel`, `art` (`"laenge"`, `"winkel"`, `"zahl"`), `kommentar` nur beim kommentierten; Feature `"formeln": {"breite": "Breite"}`; Skizze `"formeln": [{"abhaengigkeit": 2, "formel": "a"}]`; `"naechster_parameter": 4`;
  (d) `old_files_unchanged`: jede Datei unter `tests/fixtures` und `tests/data` lesen und neu schreiben → bytegleich (vorhandene Tests bleiben grün); ein Schnitt-Dokument (Format 8) ohne Parameter → Text ohne `"parameter"`, `"formeln"`, `"naechster_parameter"`, `"format": 8`;
  (e) `rejected_and_lenient`: Format-8-Text mit `"parameter"` → Fehlertext „Parameter und Formeln gibt es erst ab Dateiformat 9“; Art „laenge2“; Schlüssel „foo“ → „unbekanntes Formelfeld „foo““; Skizzenindex 99; Formel mit 201 Zeichen; `from_snapshot(decode(..))` mit Namen „a“/„A“ → `Err`, Text enthält „Parametertabelle“; mit Zyklus → „im Kreis“; Quader mit `{"breite": "Gibtsnicht"}` → lädt, Status `Error(..)`, `size.x` aus der Datei;
  (f) `too_new`: `"format": 10` → `ProjectError::TooNew { found: 10, .. }`.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-project --test parameter_format -j 2`
- [ ] **Step 3: Implement**; die fünf `FORMAT_VERSION`-Prüfungen auf 9.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-project -j 2`, `cargo test -p planora-app --test workspace -j 2`, clippy, fmt; `git diff 7f571c6 -- crates/planora-project/tests/data crates/planora-project/tests/fixtures` leer.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Regel 9 hinter Regel 8; Skizzenformeln nicht gezählt; `skip_serializing_if` vergessen (alte Dateien ändern sich); `naechster_parameter` immer geschrieben; Kommentar immer geschrieben; Formatsperre `<= 9`; Index statt `ConstraintId::index()` aus der lebenden Reihenfolge; unbekannter Schlüssel still verworfen; Art-Zuordnung vertauscht; Formelgrenze nicht geprüft.
- [ ] **Step 6: Commit** — `feat: Dateiformat 9 mit Parametern und Formeln`.

### Task 8: Zustand der App, Schrittnamen, Verlaufsgröße

**Files:**
- Modify: `crates/planora-app/src/undo_labels.rs` (Regel `parameters` als **erste** in `describe_change`; Formel-Unterschied in `features`; Skizzenregel „Formel ändern“), `crates/planora-app/src/history.rs` (`snapshot_bytes`, `sketch_bytes`), `crates/planora-app/src/state.rs` (Parameter-Aktionen, `commit_draft`, `observe_sketch`, `move_body_with` nach P11, `session_draft_mut`, Formel-Varianten der Anlege-Funktionen)
- Test: `crates/planora-app/tests/parameter_state.rs` (neu), `crates/planora-app/tests/undo_labels.rs` (neue Tests anhängen)

**Interfaces:**
- Consumes: Aufgaben 2–6.
- Produces:
```rust
// state.rs
pub const SKETCH_OPEN_LOCK: &str = "Beenden Sie zuerst die Skizze.";
impl AppState {
    pub fn add_parameter(&mut self) -> Result<ParamId, String>;                    // add_parameter_default
    pub fn edit_parameter(&mut self, id: ParamId, edit: ParamEdit) -> Result<(), String>;
    pub fn delete_parameter(&mut self, id: ParamId) -> Result<(), String>;
    pub fn replace_parameter(&mut self, id: ParamId) -> Result<(), String>;
    pub fn create_box_with(&mut self, size: DVec3, formulas: Formulas) -> Result<FeatureId, String>;   // create_box ruft sie mit leerer Tabelle
    pub fn create_cylinder_with(&mut self, radius: f64, height: f64, formulas: Formulas) -> Result<FeatureId, String>;
    pub fn create_sphere_with(&mut self, radius: f64, formulas: Formulas) -> Result<FeatureId, String>;
    pub fn create_cone_with(&mut self, radius: f64, height: f64, formulas: Formulas) -> Result<FeatureId, String>;
    pub fn extrude_sketch_with(&mut self, sketch: FeatureId, profile: usize, height: f64, formulas: Formulas) -> Result<FeatureId, String>;
    pub fn add_blend_with(&mut self, body: usize, edges: &[Id<Edge>], art: BlendArt, tangentenkette: bool, first_face: Option<&str>, formulas: Formulas) -> Result<FeatureId, String>; // add_blend_first ruft sie
    pub fn set_mesh_formula(&mut self, index: usize, key: FieldKey, text: &str) -> Result<(), String>; // Zahl → Formel entfällt, Wert gesetzt
    pub fn session_draft_mut(&mut self) -> Option<(&mut SketchSession, &mut SketchFeature)>;
}
```
Parameter-Aktionen: bei offener Skizze `Err(SKETCH_OPEN_LOCK)` über `fail`, Dokument unverändert; sonst Dokument-Aufruf, `refresh()`, und das erste Feature, das vorher nicht `Error` war und jetzt `Error(e)` ist, schreibt `message = Some(format!("„{}“: {e}", name))`. Schrittnamen: Parameter neu → „Parameter anlegen“; Kennung fehlt → „Parameter löschen“ (auch „Durch den Wert ersetzen“); sonst `parameters`/`next_param` verschieden → „Parameter ändern“; Feature mit gleicher Art, aber anderen `formulas` → wie Artänderung („Verschieben“ bei Move/Netz-Körper, sonst „Feature ändern“); `describe_sketch_change_beside`: nur `formulas` verschieden (nach allen Strukturregeln, vor „Skizze ändern“) → „Formel ändern“. `snapshot_bytes`: + je Parameter `size_of::<Parameter>()` + Längen von Name, Formel, Kommentar; + je Formel (Feature und Skizze) 32 + Textlänge; `sketch_bytes` zählt die Skizzenformeln mit. `commit_draft`: „unverändert“ verlangt zusätzlich `draft.formulas == def.formulas`. `observe_sketch` ruft zuerst `prune_formulas()` am Entwurf.

- [ ] **Step 1: Failing tests:**
  (a) `labels` (in `undo_labels.rs`): Parameter anlegen / Formel ändern (auch wenn sich ein Quader mitändert) / löschen / ersetzen → „Parameter anlegen“, „Parameter ändern“, „Parameter löschen“, „Parameter löschen“; nur Feature-Formeltext anders → „Feature ändern“; Verschieben-Formel → „Verschieben“; Skizze nur Formel anders → „Formel ändern“;
  (b) `bytes_grow`: `snapshot_bytes` mit einem Parameter > ohne; mit Formel > ohne; `sketch_bytes` mit Skizzenformel > ohne;
  (c) `undo_restores_table_and_fields` (Workspace-Tab, Vorbild `tests/undo_flow.rs`): Breite = 40, Quader mit `{Breite: "Breite"}` → Schritt; Breite = 60 → `observe_history(false)` → **ein** Schritt „Parameter ändern“; Rückgängig → Parameterformel „40“, `size.x == 40`, Körperhülle 40 ±1e-6; Wiederholen → 60;
  (d) `sketch_open_locks_parameters`: offene Skizze → alle vier Aktionen `Err(SKETCH_OPEN_LOCK)`, `edit_count` gleich;
  (e) `message_after_failing_change`: Formel „Breite-40“, Breite 50 → 40 → `message == Some("„Quader 1“: Breite ergibt 0 mm, es muss größer als 0 sein")`;
  (f) `move_with_formulas_not_extended` (P11): Körper per Verschieben mit `{X: "a"}`; `move_body(index, (5,0,0))` → ein **neues** Verschieben, das alte behält `{X: "a"}`; ohne Formeln wird wie bisher erweitert;
  (g) `formula_only_change_commits`: Skizze öffnen, im Entwurf nur den Formeltext ändern („Breite“ → „breite“), schließen → `SketchFeature.formulas` im Dokument neu, ein Schritt „Skizze bearbeiten“;
  (h) `observe_prunes`: im Entwurf ein Maß mit Formel löschen, `observe_sketch(false)` → Entwurfstabelle leer;
  (i) `create_with_formulas`: `create_box_with((40,20,10), {Breite: "Breite"})` → Formel gespeichert, ein Schritt „Quader anlegen“; `set_mesh_formula(i, X, "a")` → Formel, `set_mesh_formula(i, X, "7")` → Formel weg, `offset.x == 7`.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test parameter_state --test undo_labels -j 2`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-app -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Parameterregel hinter der Feature-Regel; Ersetzen heißt „Parameter ändern“; Formelvergleich in `features` fehlt; „Formel ändern“ vor „Löschen“; Bytes ohne Kommentar; Sperre bei offener Skizze fehlt; Meldung vom falschen Feature; Verschieben mit Formeln erweitert; `commit_draft` ohne Formelvergleich; `observe_sketch` ohne `prune`; `create_box` ruft nicht `create_box_with`.
- [ ] **Step 6: Commit** — `feat: Parameter im App-Zustand, Schrittnamen und Verlaufsgröße`.

### Task 9: Formelfeld und Fenster „Parameter“

**Files:**
- Create: `crates/planora-app/src/formula_field.rs`, `crates/planora-app/src/parameter_dialog.rs`
- Modify: `crates/planora-app/src/lib.rs`, `crates/planora-app/src/workspace.rs` (`DocTab.parameter_dialog`, nicht in `dialog_open`, nicht in `close_dialogs`; `after_jump` ruft `reload`), `crates/planora-app/src/app.rs` (Eintrag in `edit_menu` nach Rückgängig/Wiederholen und dem Trennstrich, vor den Booleschen Operationen; Fenster je Bild zeigen), `crates/planora-app/src/icons.rs` (`Icon::Parameters`, Phosphor `function` U+EBE4)
- Test: `crates/planora-app/tests/parameter_ui.rs` (neu), `crates/planora-app/tests/icons.rs` (falls die Symbolliste dort aufgezählt ist)

**Interfaces:**
- Consumes: Aufgaben 2, 8; `theme::{text_field, palette}`, `dialog::{Dialog, DialogSize::Wide}`, `sketch_view::format_number`, `drawing_browser::READ_ONLY_HINT`.
- Produces:
```rust
// formula_field.rs
#[derive(Clone, Debug, PartialEq)] pub enum FieldInput { Number(f64), Formula(String) }
pub fn classify(text: &str) -> Option<FieldInput>;                 // leer → None; plain_number → Number; sonst Formula(trim)
pub fn resolve(text: &str, unit: ParamUnit, scope: &ParamScope) -> Result<(f64, Option<String>), String>; // leer → "Bitte eine Zahl oder Formel eingeben."
pub fn value_text(v: f64, unit: ParamUnit) -> String;              // "= 20,00 mm", "= 45,00°", "= 1,50"
pub fn names_hint(scope: &ParamScope) -> Option<String>;           // None ohne Parameter; "Namen: a, b, …" höchstens 8 Namen, dann " …"
pub const FX: &str = "fx";
pub const FX_HINT: &str = "Formel. Klicken setzt den berechneten Wert ein und löscht die Formel.";
pub struct FieldOutcome { pub response: egui::Response, pub committed: bool, pub fx_clicked: bool }
pub fn formula_field(ui: &mut egui::Ui, text: &mut String, unit: ParamUnit, scope: &ParamScope, width: f32) -> FieldOutcome;
// parameter_dialog.rs
pub const TITLE: &str = "Parameter";
pub const ADD_ROW: &str = "Zeile hinzufügen";
pub const DELETE_ROW: &str = "Zeile löschen";
pub const REPLACE: &str = "Durch den Wert ersetzen";
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Column { Name, Formula, Comment }
pub struct ParameterDialog { /* Entwürfe je ParamId, Fehler, offene Löschfrage */ }
impl ParameterDialog {
    pub fn new(state: &AppState) -> ParameterDialog;
    pub fn reload(&mut self, state: &AppState);
    pub fn row_ids(&self) -> Vec<ParamId>;
    pub fn set_text(&mut self, id: ParamId, column: Column, text: &str);
    pub fn commit(&mut self, id: ParamId, state: &mut AppState) -> Result<(), String>;    // ein edit_parameter für die ganze Zeile
    pub fn set_unit(&mut self, id: ParamId, unit: ParamUnit, state: &mut AppState) -> Result<(), String>;
    pub fn add_row(&mut self, state: &mut AppState) -> Result<ParamId, String>;
    pub fn request_delete(&mut self, id: ParamId, state: &mut AppState) -> Result<(), String>; // unbenutzt: löscht; benutzt: Rückfrage
    pub fn confirm_text(&self) -> Option<&str>;
    pub fn replace_confirmed(&mut self, state: &mut AppState) -> Result<(), String>;
    pub fn error(&self) -> Option<&str>;
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, read_only: bool) -> bool;
}
pub struct DocTab { /* … */ pub parameter_dialog: Option<ParameterDialog> }
```
Formelfeld: bei Formel links ein klickbares „fx“ (Hinweis `FX_HINT`), dann das Textfeld (`theme::text_field`), rechts `value_text` (bei Fehler nichts) bzw. bei Zahl wie heute die Einheit; Fehlermeldung darunter in `palette(dark).error`. `committed` = Enter oder Fokusverlust nach Änderung. „fx“ setzt `*text = number_text(wert)` (nur bei gültiger Formel). Fenster: `DialogSize::Wide`, Tabelle in einem Rollbereich, Kopfzeile mit den Spalten, je Zeile Name, Formel (Formelfeld), Wert (`value_text` ohne „= “-Vorsilbe, also „40,00 mm“), Art (Auswahl `ParamUnit::ALL`, wirkt sofort), Kommentar, Löschsymbol (`DELETE_ROW` als Hinweis); darunter „Zeile hinzufügen“ und die Fehlermeldung. Name/Formel/Kommentar wirken bei Enter oder Verlassen des Feldes (`commit`); bei Fehler bleibt der Entwurf mit roter Meldung. Benutzter Parameter: Rückfrage im Fenster mit der Meldung aus `ParameterInUse` und den Knöpfen `REPLACE` und „Abbrechen“. Offene Skizze: alles gesperrt, oben `SKETCH_OPEN_LOCK`; alte Version: gesperrt, oben `READ_ONLY_HINT`.

- [ ] **Step 1: Failing tests:**
  (a) `classify_and_texts`: `classify("12,5") == Some(Number(12.5))`, `classify(" Breite/2 ") == Some(Formula("Breite/2"))`, `classify("  ") == None`; `value_text(20.0, Length) == "= 20,00 mm"`, `value_text(45.0, Angle) == "= 45,00°"`, `value_text(1.5, Number) == "= 1,50"`; `names_hint` mit 10 Parametern endet auf „ …“ und nennt 8;
  (b) `field_shows_fx_value_and_error` (`run_ui`, Gemaltes einsammeln wie `tests/common`): „Breite/2“ mit Breite = 40 → Texte „fx“ und „= 20,00 mm“; „5“ → kein „fx“; „Breit“ → „Den Namen „Breit“ gibt es nicht.“ in Fehlerfarbe;
  (c) `fx_click_sets_value`: Klick auf „fx“ → Text „20“, `fx_clicked`;
  (d) `menu_opens_window`: Menü „Bearbeiten“ enthält „Parameter …“ mit Symbol; Klick → Fenster „Parameter“ mit allen fünf Spaltenköpfen;
  (e) `edit_commits_on_enter_only`: `add_row` → „Parameter1“; Formel „40“ tippen ohne Enter → Dokument unverändert; Enter → Formel „40“, `edit_count` + 1, danach im Verlauf genau ein Schritt „Parameter ändern“;
  (f) `delete_used_offers_replace`: Breite in Quader 1 benutzt → `request_delete` → `confirm_text() == Some("Breite wird in Quader 1 benutzt.")`, Knöpfe „Durch den Wert ersetzen“ und „Abbrechen“ sichtbar; ersetzen → Parameter weg, Quader ohne Formel, Wert gleich, ein Schritt „Parameter löschen“; Abbrechen → nichts geändert;
  (g) `rename_row`: Name „Weite“ + Enter → Quader-Formel „Weite“, ein Schritt;
  (h) `error_stays_in_row`: Zyklus eingeben → rote Meldung mit „im Kreis“, Dokument unverändert, Entwurf bleibt;
  (i) `locked_while_sketch_open`: offene Skizze → „Beenden Sie zuerst die Skizze.“ sichtbar, Felder gesperrt; alte Version → `READ_ONLY_HINT`;
  (j) `undo_keeps_window_open`: Änderung, dann Strg+Z (kein Feld hat die Tastatur) → Dokument zurück, Fenster offen, Zeile zeigt alten Wert; beim Tippen im Fenster wirkt Strg+Z nicht aufs Dokument;
  (k) `fits_small_and_both_themes`: 800 × 600 hell und dunkel: Fenster ganz im Bild, Breite 720 bzw. eingepasst wie `tests/wide_dialogs.rs`, „Zeile hinzufügen“ erreichbar.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test parameter_ui -j 2`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-app -j 2` (inklusive `no_raw_text_edits`, `no_raw_windows`, `no_hard_coded_colors`, `fonts`, `icons`), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Übernahme bei jeder Taste; Übernahme nur bei Enter (nicht bei Fokusverlust); „fx“ bei Zahl gezeigt; „fx“ setzt gerundeten Wert; Fehler nicht in Fehlerfarbe; Fenster in `dialog_open` (sperrt Strg+Z); `after_jump` schließt das Fenster; Sperre bei offener Skizze fehlt; Rückfrage löscht ohne Ersetzen; Art-Wechsel erst bei Enter; Namenshinweis ohne Begrenzung.
- [ ] **Step 6: Commit** — `feat: Fenster „Parameter“ und Formelfeld`.

### Task 10: Formelfelder in Erzeugen-, Bearbeiten- und Netz-Körper-Feldern

**Files:**
- Modify: `crates/planora-app/src/create_dialog.rs` (`drag` → Formelfeld für Quader, Zylinder, Kugel, Kegel, Extrusionshöhe; Texte im egui-Speicher unter `egui::Id::new(("create-formulas", title))`, beim Schließen entfernt; `CreateDialog`-Varianten bleiben unverändert), `crates/planora-app/src/feature_dialog.rs` (`numbers_of`/`set_numbers` auf `field_keys`/`field_label`/`set_field_value`; Felder zeigen gespeicherte Formeln; `apply` → `update_with_formulas`; variable Rundung bleibt reine Zahl), `crates/planora-app/src/properties.rs` (Netz-Körper: Position x/y/z und Maßstab als Formelfelder, P10)
- Test: `crates/planora-app/tests/formula_dialogs.rs` (neu)

**Interfaces:**
- Consumes: `formula_field::{formula_field, resolve, names_hint}` (9); `AppState::{create_*_with, extrude_sketch_with, set_mesh_formula}` (8); `field_keys`, `field_label`, `field_unit`, `Feature::formulas` (3–5).
- Produces: `FeatureDialog::set_text(label, text)` bleibt; neu `FeatureDialog::text(&self, label: &str) -> Option<&str>`; `CreateDialog::create_with(&self, state: &mut AppState, texts: &BTreeMap<FieldKey, String>) -> bool` (`create` ruft sie mit leeren Texten). Meldungen im Fenster: Das Fenster wertet jedes Feld vor dem Übernehmen selbst aus (`resolve`); Feldfehler `format!("{label}: {meldung}")`; nur-Zahl-Feld mit Formel → `format!("{label}: Hier sind nur Zahlen möglich.")`; unter den Feldern `names_hint`, wenn es Parameter gibt.

- [ ] **Step 1: Failing tests** (Breite = 40):
  (a) `create_box_with_formula`: Fenster „Quader erzeugen“, Breite „Breite“, Tiefe „20“, Höhe „Breite/4“ → `size == (40, 20, 10)`, Formeln `{Breite, Hoehe}`, ein Schritt „Quader anlegen“; Breite „Breit“ → Fenster bleibt offen mit „Breite (x): Den Namen „Breit“ gibt es nicht.“, kein Feature;
  (b) `edit_dialog_shows_and_clears_formula`: `FeatureDialog::new` → `text("Breite") == Some("Breite")`; `set_text("Breite", "7")`, `apply` → `size.x == 7`, keine Breite-Formel; Breite = 80 → `size.x == 7`; `set_text("Breite", "Breite*2")`, `apply` → 160 und Formel gespeichert;
  (c) `edit_dialog_rejects_bad_formula`: „Breit“ → `apply` `Err("Breite: Den Namen „Breit“ gibt es nicht.")`, Dokument und `edit_count` unverändert;
  (d) `move_and_extrude`: Verschieben-Fenster Δx „Breite/2“ → `delta.x == 20`; „Skizze extrudieren“ Höhe „Breite/8“ → Höhe 5, Formel `{Hoehe}`; Bearbeiten der Extrusion zeigt die Formel;
  (e) `variable_fillet_stays_numeric`: Position 1 „Breite“ → `apply` `Err("Position 1: Hier sind nur Zahlen möglich.")`;
  (f) `mesh_body_fields`: Netz-Körper im Eigenschaftenfeld: Position x „Breite“ + Enter → `offset.x == 40`, Formel; „7“ + Enter → `offset.x == 7`, Formel weg; Maßstab „Breite“ (Länge) → rote Meldung „Hier wird eine Zahl erwartet, die Formel ergibt eine Länge.“, Lage unverändert; Drehung bleibt Ziehfeld (bestehende Tests in `mesh_body_ui.rs` grün);
  (g) `names_hint_shown`: Fenster „Quader erzeugen“ zeigt „Namen: Breite“.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test formula_dialogs -j 2`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-app -j 2` (bestehende `feature_dialog.rs`, `dialogs.rs`, `sketch_dialogs.rs`, `graphit_style.rs`, `wide_dialogs.rs`, `mesh_body_ui.rs` unverändert grün), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Zahl lässt Formel stehen; Formel nicht gespeichert (nur Wert); Fenster-Texte beim Schließen nicht entfernt (alter Text beim nächsten Öffnen); `apply` ruft `update` statt `update_with_formulas`; Bearbeiten zeigt Wert statt Formel; variable Rundung nimmt Formeln; Netz-Feld wirkt bei jeder Taste; Netz-Maßstab als Länge; Hinweis ohne Parameter gezeigt; Fehlermeldung ohne Feldname.
- [ ] **Step 6: Commit** — `feat: Formelfelder in Erzeugen, Bearbeiten und Netz-Körper`.

### Task 11: Formelfelder in Rundungsfenster und Skizzenmaß

**Files:**
- Modify: `crates/planora-app/src/blend_dialog.rs` (Felder `first`/`second` als Formelfelder; `art_value` liefert zusätzlich Formeln; private Winkelgrenzen durch `planora_feature::{MIN,MAX}_CHAMFER_ANGLE_DEG` ersetzen), `crates/planora-app/src/sketch_panel.rs` (`show` über `session_draft_mut`; `value_editor` mit Formelfeld; „fx“ in der Liste bei Maßen mit Formel), `crates/planora-app/src/sketch_state.rs` (`SketchSession::set_dimension_input`, `apply_dimension_input`)
- Test: `crates/planora-app/tests/formula_sketch_ui.rs` (neu), `crates/planora-app/tests/blend_ui.rs` (neue Tests anhängen)

**Interfaces:**
- Consumes: Aufgaben 6, 8, 9.
- Produces:
```rust
impl BlendDialog { pub fn art_formulas(&self, scope: &ParamScope) -> Result<(BlendArt, Formulas), String>; }   // art_value bleibt für Zahlen
impl SketchSession {
    /// Formel oder Zahl für das bestehende Maß `id`: auswerten (Grad → Bogenmaß), `set_dimension_value`, dann Formel setzen bzw. entfernen. Atomar.
    pub fn set_dimension_input(&mut self, sf: &mut SketchFeature, id: ConstraintId, text: &str, scope: &ParamScope) -> Result<(), String>;
    /// Neues Maß `action` mit Zahl oder Formel; wird es anzeigend (überzählig), entfällt die Formel und die bestehende Meldung erscheint.
    pub fn apply_dimension_input(&mut self, sf: &mut SketchFeature, action: ConstraintAction, text: &str, scope: &ParamScope) -> Result<ConstraintId, String>;
}
```
Das Maßfeld übernimmt wie heute bei „OK“ oder Enter (P9). Der Zeichenbereich zeigt weiter den Wert (Spec §3), nur die Liste markiert Formelmaße mit „fx“. Bei anzeigenden Maßen gibt es kein Formelfeld (Doppelklick öffnet wie heute nichts Bestimmendes).

- [ ] **Step 1: Failing tests** (Breite = 40, w = 30 Winkel):
  (a) `blend_dialog_formulas` (in `blend_ui.rs`): Rundung Radius „Breite/20“ → Feature-Radius 2, Formel `{Radius}`; Fase mit Winkel, Winkel „w“ → 30, Formel `{Winkel}`; Winkel „Breite“ → Fenstermeldung „Winkel: Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.“, kein Feature;
  (b) `sketch_existing_dimension_formula`: offene Skizze mit Abstandsmaß 30, Maßfeld öffnen, „Breite“ + Enter → Maßwert 40, `draft.formulas[id] == "Breite"`, Liste zeigt „fx“; „25“ + Enter → Formel weg, Wert 25;
  (c) `sketch_new_dimension_formula`: neues Abstandsmaß mit „Breite/2“ → 20 und Formel; Winkelmaß „w“ → gespeichert `30f64.to_radians()`;
  (d) `sketch_unsolvable_is_rejected_in_session`: Formel, die die Skizze widersprüchlich macht → Meldung wie heute („Widerspruch: …“ bzw. „… ließ sich nicht lösen“), Entwurf und Tabelle unverändert;
  (e) `driven_drops_formula`: Maß mit Formel auf „anzeigend“ → Formel weg (nach `observe_sketch`);
  (f) `canvas_shows_value_not_formula`: Gemaltes im Zeichenbereich enthält „40,00“ und nicht „Breite“;
  (g) `close_writes_formulas`: Skizze schließen → Dokument hat die Formel, Parameteränderung Breite 50 → Maß 50.
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test formula_sketch_ui --test blend_ui -j 2`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-app -j 2` (bestehende Skizzen- und Rundungstests grün), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Winkel nicht in Bogenmaß; Formel vor dem Lösen gesetzt (bleibt bei Ablehnung stehen); Zahl lässt Formel stehen; anzeigendes neues Maß behält Formel; „fx“ in der Liste fehlt; Zeichenbereich zeigt Formel; Fase-Winkel ohne Einheitenprüfung; Rundungsfenster speichert Formel ohne Wert; `session_draft_mut` liefert Kopie; alte Winkelgrenzen 1/179 hart codiert gelassen und abweichend.
- [ ] **Step 6: Commit** — `feat: Formeln im Rundungsfenster und in Skizzenmaßen`.

### Task 12: Dokumentation und Abschlussprüfung

**Files:**
- Modify: `README.md` (neuer Abschnitt „Parameter“ in Laiensprache: Fenster, Arten, Formeln mit Beispielen „Breite / 4“, „sqrt(a^2 + b^2)“, „min(Breite; Tiefe) - 2*Wand“, Komma-Regel P5, „fx“, Löschen/Ersetzen, Umbenennen, Rückgängig, Skizzen, Grenzen aus Spec §4a.9), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Benannte Parameter und Gleichungen“: Geliefertes, P1–P14, Grenzen (Profilnummer bei Extrusion, Hinweispunkte bei großen Sprüngen, Solver startet von der gespeicherten Lösung, Netz-Körper ohne Ziehen, Zeichnungsmaße ohne Formeln, keine Auswahlliste), Handliste als Abhakliste), `docs/superpowers/specs/2026-10-20-parameter-design.md` (neuer §8 „Festlegungen der Umsetzung“: P1–P14 kurz), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)

- [ ] **Step 1: Texte schreiben** wie oben (Laiensprache, kein KI-Füllstoff).
- [ ] **Step 2: Gates** mit Ausgabe: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2`; `cargo deny check` (Ausnahmen zbus-Familie und ttf-parser erneut prüfen, Ergebnis berichten); `git diff --stat 7f571c6 -- crates/planora-kernel crates/planora-mesh crates/planora-step crates/planora-exchange crates/planora-sketch crates/planora-drawing crates/planora-render crates/planora-math` leer; `git diff 7f571c6 -- '**/Cargo.toml' Cargo.lock deny.toml crates/planora-project/tests/data crates/planora-project/tests/fixtures` leer; `git status` sauber.
- [ ] **Step 3: Mutationssumme** aller Aufgaben im Bericht (Zahl der Einträge, keine grüne Mutation offen).
- [ ] **Step 4: Commit** — `docs: Parameter und Gleichungen beschrieben, offene Punkte, Handliste`.

### Task 13: Prüfung im echten Fenster (Controller)

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Handliste abhaken, Befunde), dieser Plan („Abweichungen bei der Umsetzung“)

Der Controller führt sie selbst aus (keine Subagent-Aufgabe): Release-Build `cargo build --release -p planora-app`, App starten, Bedienung über `tools/gui-drive.ps1` (Klicks, Tippen, Bildschirmfotos nach `%TEMP%\planora_shot.png`), Fensterversatz 1:1 zum Bildschirmfoto.

- [ ] **Step 1: Handliste** (jeweils Bildschirmfoto prüfen): (1) „Bearbeiten → Parameter …“ öffnen, „Breite = 40“, „Tiefe = Breite/2“ anlegen, Wert „20,00 mm“; (2) Quader mit Breite „Breite“, Höhe „Breite/4“ erzeugen, Bearbeiten zeigt „fx“ und „= 40,00 mm“; (3) Breite → 60: Körper wächst, ein Rückgängig stellt 40 her, Wiederholen 60; (4) Zyklus und „Breit“ eintippen: rote Meldung, nichts ändert sich; (5) Breite löschen: Rückfrage nennt „Quader 1“, „Durch den Wert ersetzen“ lässt den Körper gleich; (6) Skizze mit Maß „Breite“, extrudieren, Parameter ändern: Skizze und Körper folgen; Skizze offen: Fenster „Parameter“ gesperrt mit „Beenden Sie zuerst die Skizze.“; (7) Fase mit Winkel „w“; (8) Speichern, Schließen, Öffnen: alles gleich; (9) Hell/Dunkel und Fenster 800 × 600; (10) Strg+Z beim Tippen im Fenster wirkt nicht aufs Dokument.
- [ ] **Step 2: Befunde** eintragen; Fehler als Korrekturaufgaben an Subagents (mit Test), dann Liste wiederholen.
- [ ] **Step 3: Commit** — `docs: Handprüfung Parameter im echten Fenster`.

---

## Abweichungen bei der Umsetzung

(wird während der Umsetzung gefüllt)

- **Aufgabe 2, Meldungen der Tabelle:** Ohne Vorsilbe stehen Namensfehler (Form, Länge, reserviert, doppelt), „Höchstens 500 Parameter.“, Kreis und Selbstbezug (sie nennen die Namen selbst) und der Kommentar; alle übrigen Fehler als `Parameter „{name}“: {meldung}`. Umbenennen auf einen vorhandenen Namen meldet den **getippten** Namen („Den Namen „tiefe“ gibt es schon.“), nicht den der späteren Zeile. „Durch den Wert ersetzen“ lehnt bei zu langem Wert mit `Parameter „{name}“: Der Wert lässt sich nicht als Zahl mit höchstens 200 Zeichen schreiben.` ab.
- **Aufgabe 2, zusätzliche Texte:** unbekannte Kennung → `InvalidInput("Den Parameter gibt es in diesem Dokument nicht")`; Kennungen erschöpft → „Die Parameter-Kennungen sind aufgebraucht“; `build` lehnt `next_param` 0 oder `u64::MAX` mit „Der Parameterzähler ist ungültig“ ab (wie beim Feature-Zähler).
- **Aufgabe 3, Wertfehler schreibt den Wert:** Wie im Aufgabentext („Wert wird trotzdem geschrieben“), abweichend vom Satz in P3 („lassen das Feld auf dem letzten Wert“), der nur noch für den Formelfehler (`Broken`) gilt. So trägt das Feld immer den Wert seiner Formel, und `check_invariants` kann den abgeschlossenen Durchgang bitgenau prüfen.
- **Aufgabe 3, Test (d):** „100/(Breite-40)“ ergibt nach P4 Zahl durch Länge und passt nicht in ein Längenfeld (Ablehnung mit Einheitenmeldung); der Test benutzt „Breite*10/(Breite-40)“ für die Division durch null nach der Parameteränderung.
- **Aufgabe 3, geduldete alte Formelfehler (P3):** Ein Formelfehler gilt als „schon vorher da“, wenn dasselbe Feature im selben Feld mit **demselben Formeltext** schon vorher einen Formelfehler hatte (beim Umbenennen eines Parameters mit dem umgeschriebenen Text verglichen). Ein neuer, ebenfalls kaputter Text im selben Feld wird abgelehnt; ein Fenster, das die unveränderte kaputte Formel mitschickt, kann andere Felder ändern.
- **Aufgabe 3, Bild mit Formel an einem Feld, das die Art nicht hat:** `build` lehnt mit `Ungültige Eingabe: Feature „{name}“: Dieses Feld kann keine Formel haben` ab (die Datei ist dann kaputt, nicht nur eine Formel).
- **Aufgabe 3, Bericht:** Ein Feature mit Formelfehler wird nicht gerechnet; es steht in `computed`, wenn sich sein Status dadurch ändert, sonst in `reused`. Formelfehler werden vor den Eingangsfeatures geprüft (eigener Fehler vor „übersprungen“).
- **Aufgabe 3 → Hinweis für Aufgabe 10:** Das Fenster „bearbeiten“ (`feature_dialog`) schickt heute alle Felder über `update`. Das ist nach P2 korrekt (verlustfreies `number_text`, gleiche Werte behalten ihre Formel), Aufgabe 10 muss es aber auf `update_with_formulas` umstellen (ganze Formeltabelle des Features in einem Schritt, unveränderte kaputte Formeln dürfen mitgeschickt werden). Feldzuordnung: Aufgaben 4–6 tragen neue Felder genau einmal in `field_table!` (formulas.rs) und in `field_keys` (Reihenfolge) ein.
- **Aufgabe 4, Wertregeln je Art:** Die Extrusionshöhe muss wie Quader-/Zylinderhöhe größer als die Toleranz sein (der Kernel extrudiert nur positiv; ohne Formel scheitert Höhe ≤ 0 ebenso), Meldung „Höhe ergibt {v} mm, es muss größer als 0 sein“. Δx/Δy/Δz und die Netz-Lage haben kein Vorzeichen-Gebot (nur die bisherige Bereichsprüfung beim Rechnen). Der Netz-Maßstab ist eine Zahl und muss in `MIN_MESH_SCALE..=MAX_MESH_SCALE` liegen (Grenzen eingeschlossen, wie beim Eintippen). Die Maßstabmeldung nennt den Wert verlustfrei (`scale_text`: Dezimalkomma, Exponentschreibweise unter 0,000001 und ab 1e15, etwa „Maßstab ergibt 5e-7, …“, „… 1000000,0000001, …“), nie eine gerundete Zahl, die dem Satz widerspräche; Längenmeldungen bleiben bei `short_number` mit „mm“ (Höhe 1e-6 = Toleranz meldet „0 mm“). P11 bleibt App-Sache (Aufgabe 8): `update` ohne Formeln löscht beim Erweitern die Formel des geänderten Feldes, darum dort ein neues Verschieben.
- **Aufgabe 4 → Hinweis für Aufgaben 8/10 (Review F1):** Lage und Maßstab eines Netz-Körpers ändert die App heute über `move_mesh_body` → `set_mesh_transform_of` → `Document::update`; das löscht still die Formel des geänderten Feldes, und das Rückschreiben bei Fehlstatus (`update(alt)`) bringt sie nicht zurück (wie bei `extend_move`). Aufgaben 8/10 müssen die getippten Formelfelder über `update_with_formulas` führen und bei einer Formel im betroffenen Feld das Ziehen entweder ablehnen oder die Formel bewusst durch den Wert ersetzen; das Rückschreiben nach einem Fehler muss die Formeln über `update_with_formulas` bzw. `restore` wiederherstellen.
- **Aufgabe 5, Winkelmeldung:** Der Wert in „Winkel ergibt {v}°, er muss zwischen 1° und 179° liegen“ steht kurz (`short_number`, „200“), außer wenn die gerundete Zahl selbst zulässig wäre (0,999 → „1“, 179,004 → „179“); dann verlustfrei (`number_text`: „0,999°“, „179,004°“), damit die Meldung sich nicht widerspricht (wie der Maßstab in Aufgabe 4). Grenzen eingeschlossen wie im Fenster „Fase“ (`MIN_CHAMFER_ANGLE_DEG..=MAX_CHAMFER_ANGLE_DEG`); der Winkel steht in Grad im Feld, die Formel ergibt Grad, keine Umrechnung. Fasenabstände (Abstand, Abstand 1, Abstand 2) haben dieselbe Positivprüfung wie Längen („Abstand 2 ergibt 0 mm, …“). Das Fenster „Fase“ (`blend_dialog.rs`) behält vorerst seine eigenen gleichlautenden Konstanten; Aufgabe 10 kann sie durch die neuen ersetzen.

- **Aufgabe 6, Skizzenmaße:** Die strenge Meldung nennt den Namen des Skizzen-Features („Skizze 1: …“). Ein Wert, den die Skizze ablehnt, ist ein Wertfehler mit dem Grund **ohne** Vorsilbe der Fehlerart („Ein Maß ergibt 0 mm: Das Maß muss größer als null sein“, Winkel mit „°“ ohne Leerzeichen); anders als bei Feldern (Aufgabe 3) behält das Maß seinen letzten gültigen Wert, weil `set_constraint_value` den Wert selbst prüft. Lehnt die Skizze einen Wert ab, wird er nach den übrigen Formeln derselben Skizze noch einmal versucht (große und kleine Halbachse einer Ellipse dürfen zugleich wechseln). Formeln an gelöschten, anzeigenden oder nicht maßgebenden Abhängigkeiten entfallen nicht nur in `update`, sondern auch beim Anlegen und beim Laden (still, milder Ladeweg); `check_invariants` prüft das. P2 gilt auch für Skizzen: Ändert `update` den Wert eines Maßes mit Formel, ohne deren Text neu zu setzen, entfällt diese Formel (getippte Zahl gilt). Das Dreieck aus Test (c) mit c = 100 meldet der Solver als Widerspruch („… widersprechen sich Abhängigkeiten“); „… lässt sich die Skizze nicht lösen“ prüft ein eigener Test (Linie mit doppelt festgelegtem Ende). Das Rechteck 30 → 400 aus Test (d) löst der Solver richtig (Status Ok, Breite 400).
- **Aufgabe 6 → Hinweise:** Aufgabe 7 muss `SketchFeature.formulas` speichern und laden (bis dahin setzt `convert.rs` eine leere Tabelle, Speichern verliert Skizzenformeln). Aufgabe 8: `snapshot_bytes` zählt auch die Skizzenformeln. Aufgabe 11: `commit_draft` (state.rs) hält den Entwurf für unverändert, wenn Stand der Skizze und Kantenkopien gleich sind; es muss auch die Formeltabelle vergleichen, sonst geht eine reine Formeländerung, deren Wert gleich bleibt, verloren.
- **Aufgabe 6, Nachbesserung (Review F1/F2, Entscheidung des Controllers):** „Durch den Wert ersetzen“ wird abgelehnt (Dokument und Zähler unverändert), solange ein Feature, das den Parameter nennt (Skizze oder Körperfeld), den Status Fehler oder übersprungen oder einen Wertfehler hat: „Parameter „{name}“ lässt sich nicht durch den Wert ersetzen: „{Feature}“ hat gerade einen Fehler. Beheben Sie zuerst den Fehler.“ (sonst entstünde still ein anderes Modell, etwa ein Skizzenmaß mit dem letzten gültigen Wert). Damit ändert sich der Schluss des Tests `rename_tolerates_old_broken_formula` aus Aufgabe 3 (dort wurde ein Quader mit kaputter Formel ersetzt und still Ok). Ändern sich die Formeln einer Skizze in `apply_table` (Umbenennen, Ersetzen), bekommt sie eine neue Version, damit kein gespeicherter Fehlschlag die Änderung überdauert (Status pfadunabhängig wie nach dem Laden).
- **Aufgabe 6, deaktivierte Abhängigkeiten (Review F5):** Formeln an abgeschalteten (`enabled == false`) Maßen bleiben bewusst stehen: Der Wert folgt weiter dem Parameter, der Solver beachtet das Maß nicht, der Parameter gilt als benutzt. Schaltet man das Maß wieder ein, gilt sofort der Formelwert.
- **Aufgabe 6 → Hinweise für Aufgabe 7 (Review F4/F7):** Dateien mit Formeln an gelöschten, anzeigenden oder nicht maßgebenden Abhängigkeiten soll das Laden mit deutscher Meldung als beschädigt ablehnen, statt sie still zu verwerfen (heute räumt `build` für Rückgängig-Bilder still auf). Der Zweig „°“ der Wertfehlermeldung eines Skizzenmaßes ist unerreichbar (Winkel lehnt die Skizze nur bei nicht endlichen Werten ab); er bleibt als Absicherung.
- **Aufgabe 6, endgültige Regel für „Durch den Wert ersetzen“ (Re-Review R1/R2, Entscheidung des Controllers; ersetzt die strenge Fassung oben):** Abgelehnt wird, wenn ein Feature, das den Parameter direkt nennt, (a) einen Formel- oder Wertfehler hat (`issue`, auch hinter dem Marker oder unterdrückt), (b) eine Skizze mit Status Fehler oder übersprungen ist oder (c) eine nicht berechnete Skizze (hinter dem Marker, unterdrückt) ist, deren **letzte** Formel entfiele und deren Definition sich nicht lösen lässt (geprüft ohne Ebenen- und Kantennachführung). Körperfelder ohne Formel- oder Wertfehler tragen schon den Formelwert: Ist ein solches Feature aus anderem Grund im Fehler oder übersprungen, bleibt das Modell nach dem Ersetzen gleich, das Ersetzen ist erlaubt. Ein fehlerhaftes Feature, das den Parameter nicht nennt, sperrt nicht. Meldung wie oben mit dem Namen des Features.
