# PlanoraCAD STEP-Export – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die exakten Körper des Kernels (B-Rep mit Ebenen, Zylindern, Kegeln, Kugeln, Tori und NURBS) als STEP-Datei (AP214) schreiben – eine Datei, je Körper ein Teil mit dem Namen aus dem Feature-Baum – und das als fünftes Format im Fenster „Exportieren …“ anbieten.

**Architecture:** Neues Paket `crates/planora-step`: `number` und `text` (REAL-Zahlen und Zeichenketten), `entity` (fortlaufende Einträge `#n`, sofort in den Ausgabestrom, Zeilenumbruch, Grenze, Abbrechen), `geometry` (Punkte, Richtungen, Koordinatensysteme, Kurven, Flächen), `topology` (Schalen, Flächen, Schleifen, Kanten, Eckpunkte, Hohlräume), `product` (Kopf, Einheiten, Kontext, Teile) und `file` (Hilfsdatei). Jeder Körper wird vor dem Schreiben geprüft (`check_body`: Verweise, `validate`, Probelauf des Schreibens in `std::io::sink()`); abgelehnte Körper stehen im Bericht, die anderen werden geschrieben. Geprüft wird ohne fremdes Programm mit einem eigenen Leser in `tests/common` (Zerlegung nach ISO 10303-21, Modell mit eigenen Formeln, Topologie-, Geometrie- und Kernelvergleich, Netz aus den gelesenen STEP-Flächen). `planora-app` bekommt `ExportFormat { Mesh(Format), Step }` und einen eigenen STEP-Weg im Export-Fenster.

**Tech Stack:** Rust (Edition 2024, `rust-version = 1.88`), bestehende Crates (`planora-kernel`, `planora-math`, `planora-mesh`, `planora-exchange`, `thiserror`, `egui`); **keine neue externe Abhängigkeit**.

**Spec:** `docs/superpowers/specs/2026-10-14-step-export-design.md` (bindend, vom Nutzer freigegeben; §7 gibt die vier Stufen vor). Lernstand: Plan des Vorgängers `docs/superpowers/plans/2026-10-14-netz-austausch.md`, `docs/superpowers/offene-punkte.md`.

**Vorabprüfung beim Planen:** Der Code dieses Plans wurde vor dem Schreiben in einer Kopie des Arbeitsbereichs (Stand `07bac2b`) Task für Task übersetzt und getestet (`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, alle Tests der Tasks grün, die lange Zufallsreihe in Release 3,7 s). Die in den Tasks genannten Mutationsproben schlugen dort wie erwartet fehl. Das ersetzt keine Prüfung bei der Umsetzung: Weicht ein Ergebnis ab, gilt „Ursache beheben oder anhalten und berichten“.

## Entscheidungen zur Spec (Planungsstand 2026-10-14)

Sie konkretisieren die Spec; Abweichungen werden in Task 14 in die Spec (§9 „Änderungen bei der Umsetzung“) eingetragen.

- **P1 – Abhängigkeiten:** `planora-step` hängt von `planora-kernel`, `planora-math`, `thiserror`, zusätzlich von `planora-exchange` (`Progress`, `temp_path`, `CHECK_EVERY`; die Spec-Schnittstelle nimmt `&Progress`) und ab Task 11 von `planora-mesh` ab (Vorzeichen des Schalenvolumens). Beides sind Pakete des Arbeitsbereichs, keine neue externe Abhängigkeit. Spec §2 nennt nur Kernel, Mathe und `thiserror`.
- **P2 – Schnittstelle:** `write_step(bodies: &[StepBody], opts: &StepOptions, out: &mut dyn Write, progress: &Progress) -> Result<StepReport, StepError>` wie in Spec §2, `StepBody { name: String, solid: &Solid }`, `StepOptions { file_name: String, time: SystemTime, tolerance: Tolerance }` (`StepOptions::new(file_name)` nimmt den jetzigen Zeitpunkt), `StepReport { bodies, faces, edges, entities, rejected, shortened_names }`. Dazu `check_body`, `export_step_file` (Hilfsdatei), `display_name`, `utc_timestamp`.
- **P3 – Prüfung vor dem Schreiben:** `check_body` prüft, dass alle Verweise im Körper auf vorhandene Teile zeigen (sonst würde `validate` abbrechen), ruft `validate_with(…, Tolerance::default())` und schreibt den Körper einmal probeweise nach `std::io::sink()` mit derselben Logik wie beim echten Schreiben. So fallen auch geometrische Ablehnungen (Kegel jenseits seiner Spitze, schiefes Koordinatensystem, Radius ≤ 0, nicht endliche Zahl) auf, **bevor** ein Eintrag geschrieben ist (es wird zeilenweise geschrieben, Spec §4). Eine nicht endliche Zahl in einem Körper ergibt `InvalidBody` mit dem Text von `NotFinite` (Spec §4 nennt `NotFinite`); die anderen Körper werden geschrieben. Kosten: jeder Körper wird zweimal durchlaufen.
- **P4 – Einträge:** Einträge werden von unten nach oben geschrieben (erst Punkte, dann Kanten, dann Flächen …), Verweise zeigen deshalb nur auf kleinere Nummern. Lange Einträge werden nach einem Komma außerhalb von Zeichenketten ab Spalte `WRAP_COLUMN = 72` umbrochen. Alle `CHECK_EVERY = 4096` Einträge wird gefragt, ob abgebrochen werden soll; der Fortschritt zählt Einträge (Gesamtzahl aus dem Probelauf).
- **P5 – Richtung der Kanten:** `EDGE_CURVE(…, same_sense)` mit `same_sense = (t1 >= t0)`. Spec §3 schreibt „same_sense = true“, aber Bögen im Uhrzeigersinn haben im Kernel `t1 < t0` (`extrude.rs`: `a1 = a0 + sweep`, `sweep < 0`); mit `.T.` liefe die Kante den falschen Bogen entlang.
- **P6 – Kegel:** Die Lage wird auf die Mitte des v-Bereichs der Fläche verschoben (`v_mid = (v0 + v1)/2`, Radius dort `ρ = radius + slope · v_mid`); ist `ρ ≤ 0`, wird der Körper benannt abgelehnt („Eine Kegelfläche liegt jenseits ihrer Spitze“). Bei `slope < 0` wird die Achse umgekehrt (`z' = −z`, `x' = x`, also `y' = −y`), der halbe Öffnungswinkel ist `atan(|slope|)` in Radiant. Die natürliche Normale (von der Achse weg) bleibt dabei dieselbe, `same_sense` der Fläche bleibt. `slope = 0` wird `CYLINDRICAL_SURFACE`.
- **P7 – Ellipse:** Ist die kleine Halbachse des Kernels größer als die große (`minor > major`), wird die Ellipse mit Bezugsrichtung `y` und vertauschten Halbachsen geschrieben (erste Halbachse in Bezugsrichtung, die längere vorn; der Parameter verschiebt sich um π/2, die Kante wird über ihre Eckpunkte begrenzt).
- **P8 – B-Splines:** Knoten als verschiedene Werte mit Vielfachheiten (bitgleicher Vergleich, der Kernel speichert gleiche Knoten gleich). Rational, sobald ein Gewicht ≠ 1, als zusammengesetzter Eintrag mit Teilen in alphabetischer Reihenfolge (ISO 10303-21) **einschließlich `GEOMETRIC_REPRESENTATION_ITEM()`**, das in Spec §3 fehlt. Kennzeichen „geschlossen“: Kurven `is_closed(Tolerance::default().linear)` (wie im Kernel), Flächen `is_periodic_u/v`. Kontrollpunkte zeilenweise in u (`control[i][j]`, `i` in u).
- **P9 – Mehrere Schalen:** Schalen mit positivem Netzvolumen sind äußere Schalen, mit negativem Hohlräume (Netz mit `CLASSIFY_CHORD = 0.05`, nur bei mehr als einer Schale). Ein Hohlraum gehört zur kleinsten äußeren Schale, die seinen ersten Eckpunkt enthält (Umlaufzahl). Jede äußere Schale wird ein `MANIFOLD_SOLID_BREP`, mit Hohlräumen ein `BREP_WITH_VOIDS`; getrennte Stücke (Boolesche Vereinigung ohne Berührung) bleiben **ein Teil** mit mehreren Körpern. Die Schale eines Hohlraums wird umgekehrt geschrieben (`same_sense` gekippt, Ränder mit `.F.`) und als `ORIENTED_CLOSED_SHELL('',*,#…,.F.)` eingetragen (ISO 10303-42 verlangt `.F.`).
- **P10 – Unsicherheit:** `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT` mit `tolerance.linear` (1e-6 mm), bei Körpern mit Freiformkurven oder -flächen `max(tolerance.linear, NURBS_EDGE_TOL)` = 1e-5 mm (so weit dürfen dort Kanten von ihren Flächen abweichen).
- **P11 – Gemeinsamer Kontext:** `APPLICATION_CONTEXT`, `APPLICATION_PROTOCOL_DEFINITION`, `PRODUCT_CONTEXT`, `PRODUCT_DEFINITION_CONTEXT`, Einheiten und der geometrische Kontext stehen einmal je Datei; je Teil `PRODUCT`, `PRODUCT_RELATED_PRODUCT_CATEGORY('part',…)`, `PRODUCT_DEFINITION_FORMATION`, `PRODUCT_DEFINITION`, `PRODUCT_DEFINITION_SHAPE`, `SHAPE_DEFINITION_REPRESENTATION` und `ADVANCED_BREP_SHAPE_REPRESENTATION` mit dem Ursprungs-`AXIS2_PLACEMENT_3D`.
- **P12 – Namen:** Ein leerer (oder nur aus Leerzeichen bestehender) Name wird „Körper n“ (n = Stelle in der Liste, ab 1). Kürzung auf `MAX_NAME_CHARS = 256` Zeichen vor der Maskierung; Zeichen jenseits von U+FFFF als `\X4\…\X0\`.
- **P13 – Prüfwerkzeug in `tests/common`:** `reader.rs` (Zerlegung, strenge Schreibweise), `model.rs` (Modell mit eigenen Formeln: Cox-de-Boor-Basis, rationale Auswertung, Fußpunktsuche Raster + Gauß-Newton + Mustersuche), `check.rs` (Topologie, Geometrie, Kernelvergleich, Netz). Zuordnung zum Kernel: Teil i ↔ i-ter angenommener Körper, Hüllen über Flächenzahl und ersten Eckpunkt (bitgleich, weil die Zahlen verlustfrei geschrieben werden), Flächen und Schleifen in Schreibreihenfolge (der Schreiber hält die Reihenfolge des Kernels ein). „Dreiecksnetz aus dem Leser“ (Spec §5.4): Die Verbindung der Dreiecke kommt aus dem Kernel-Netz, jeder Punkt wird auf seine STEP-Fläche gelegt und jedes Dreieck nach deren Außennormale (natürliche Normale × `same_sense`) ausgerichtet; die Zahl der umgedrehten Dreiecke muss 0 sein.
- **P14 – Fenster:** Das Feld `format` des Export-Fensters wird `ExportFormat { Mesh(Format), Step }` (`ExportFormat::ALL` mit fünf Einträgen); eine Zeile in `tests/export_dialog.rs` wird dafür mechanisch angepasst (mit Kommentar). Schlüssel des Ergebnisses `(Option<Quality>, Scope, edit_count, Option<BodyRef>)`, `None` bei STEP; Netz- und STEP-Ergebnis liegen getrennt, ein Wechsel zurück zu STL rechnet nicht neu. STEP prüft im Arbeitsthread (`prepare_step`), schreibt im Arbeitsthread mit Fortschritt und Abbrechen (`write_progress`, `cancel_write`). Die Karte „Qualität“ zeigt bei STEP nur „Rundungen und Bohrungen bleiben exakt.“; Netz-Körper werden genannt (E3), abgelehnte Körper stehen mit Grund im Fenster.
- **P15 – Handtest:** Gegenprobe in Fusion 360 nur nach Rückfrage, vom Controller ausgeführt (E5). Ein `#[ignore]`-Test schreibt dafür Beispieldateien mit Flächenzahl und Volumen. Befunde dort sind Folge-Aufgaben, keine Nacharbeit unter Zeitdruck (Spec §8).

## Global Constraints

- Reines Rust, Edition 2024, `rust-version = 1.88`; Einheit Millimeter. Keine freien Epsilon-Zahlen in neuem Code: jede Schranke ist eine benannte `const` mit Kommentar.
- Schema und Kopf (Spec E1, §2): AP214 (`AUTOMOTIVE_DESIGN`), Textdatei, Endung `.step`; `ISO-10303-21;`, `HEADER` mit `FILE_DESCRIPTION(('PlanoraCAD Export'),'2;1')`, `FILE_NAME(name, Zeitpunkt, (Autor leer), (Organisation leer), 'PlanoraCAD', 'PlanoraCAD', '')`, `FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'))`, dann `DATA`; Zeitpunkt UTC als `YYYY-MM-DDThh:mm:ss`.
- Kontext (Spec §2): `GEOMETRIC_REPRESENTATION_CONTEXT(3)` mit `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT` (Unsicherheit aus der Kernel-Toleranz, `LENGTH_UNIT`), `SI_UNIT(.MILLI.,.METRE.)`, `SI_UNIT($,.RADIAN.)`, `SI_UNIT($,.STERADIAN.)`.
- Nur Geometrie, Namen, Einheit; keine Farben, Materialien, Baugruppenstruktur (E1). Mehrere Körper: eine Datei, je Körper ein eigenes Teil mit dem Namen aus dem Feature-Baum (E2). Netz-Körper werden nicht ausgegeben; der Dialog nennt sie mit Namen und verweist auf STL/OBJ/3MF/glTF (E3). Nur Schreiben (E4).
- Zahlen (Spec §4): `REAL` mit kürzester Darstellung, die verlustfrei zurückgelesen wird (`{:?}`-Form), immer mit Punkt (`1.`, `0.5`, `1.E-7`); `-0.` wird `0.`; nicht endliche Werte → `StepError::NotFinite` (kein Schreiben).
- Texte (Spec §4): `'` → `''`, `\` → `\\`, nicht-ASCII → `\X2\…\X0\`, Steuerzeichen → Leerzeichen; höchstens 256 Zeichen (gekürzt, im Bericht vermerkt).
- Speicher und Größe (Spec §4): zeilenweise in den Ausgabestrom (kein ganzer Text im Speicher); Eintragsnummern `u64`; höchstens `MAX_ENTITIES = 500_000_000`; Abbruch über `Progress`. Schreiben in eine Hilfsdatei mit Umbenennen wie bei den Netzformaten.
- Jeder Körper besteht `validate`; sonst `StepError::InvalidBody { name, reason }`, der Körper wird ausgelassen (Bericht), die anderen werden geschrieben; sind alle abgelehnt: `NothingToExport` (Spec §4).
- Oberfläche (Spec §6): fünftes Format **„STEP – für andere CAD-Programme (exakt)“**; die Karte „Qualität“ entfällt bei STEP, an ihrer Stelle steht „Rundungen und Bohrungen bleiben exakt.“; statt „Dreiecke, ca. MB“ steht „n Körper, n Flächen“; Fenster über die ganze Breite (`DIALOG_MARGIN = 16`), nebeneinander stehende Karten gleich hoch (`cards::card_row`). Texte deutsch, einfache Laiensprache, keine Hinweise auf die Herkunft von Daten.
- **Keine neue externe Abhängigkeit** (E6). `cargo deny check` am Ende jeder Stufe (Task 6, 9, 12, 14) grün; `deny.toml` nie selbstständig erweitern – bei Befund anhalten und den Menschen fragen. Projektlizenz GPL-3.0-only. STEP-Entitätsnamen sind Teil der offenen ISO-Norm 10303; **kein Autodesk-Material** (kein Code, keine Dateien, keine Namen aus Fusion 360).
- **Geschützte Pfade bleiben bitgleich zu `master` `07bac2b`:** `crates/planora-kernel/src/boolean/**`, `crates/planora-mesh/src/{tessellate,trimmed,discretize,check}.rs`. **Der Kernel bleibt unverändert** (nur lesend nutzen), ebenso alle anderen Pakete außer `planora-step` (neu) und `planora-app` (`Cargo.toml`, `src/export_dialog.rs`, `tests/export_dialog.rs` eine Zeile, `tests/export_step.rs` neu). Prüfung am Ende jeder Stufe:
  `git diff --stat 07bac2b -- crates/planora-kernel crates/planora-math crates/planora-mesh crates/planora-exchange crates/planora-feature crates/planora-project crates/planora-render crates/planora-sketch` ist leer. Sollte eine Änderung am Kernel unvermeidbar scheinen: anhalten und als offene Frage berichten, nicht ändern.
- **Tests:** Alle bisherigen Tests bleiben unverändert grün; einzige erlaubte Änderung ist die mechanische Zeile in `crates/planora-app/tests/export_dialog.rs` (P14, mit Kommentar). Tests werden nie abgeschwächt (Schranken, Asserts, `#[ignore]`), um grün zu werden: Schlägt ein Test fehl, Ursache beheben oder anhalten und berichten. Jedes Testprogramm läuft im Debug-Build in höchstens etwa 30 s; längere Reihen bekommen `#[ignore = "…"]` und laufen in Release. Zufallstests nur mit festem Startwert (`Lcg`).
- Keine Panics auf Nutzereingaben und Körperdaten; jeder Fehler ist eine benannte `StepError`-Variante mit deutschem Text.
- Clippy-Stil des Repos: `cargo clippy --workspace --all-targets -- -D warnings` mit `clippy::all`. Daher u. a.: `is_multiple_of` statt `% n == 0` (nur bei vorzeichenlosen Zahlen; bei `i64` bleibt `% 2 != 0`), Formatargumente inline (`{name}`), `as_chunks::<3>()` statt Indexschleifen, keine unbenutzten Importe oder `mut` in Tests (Hilfsdateien in `tests/common` stehen unter `#![allow(dead_code)]`, Importe dort müssen trotzdem benutzt sein), keine `pub(crate)`-Funktion ohne Aufrufer (sonst `dead_code`).
- Plattform Windows (MSVC) und Linux. Befehle in Bash (Git Bash) mit `export PATH="$PATH:/c/Users/denni/.cargo/bin"`; PowerShell 5.1 kennt `&&` nicht. **Dateien mit dem Write-Werkzeug schreiben, nicht über Heredocs:** In dieser Umgebung verschluckt ein Heredoc Rückstriche (`\\` wurde beim Planen zu `\`, Zeilenfortsetzungen `\` am Zeilenende verschwanden) – das bricht STEP-Maskierungen und mehrzeilige Formatzeichenketten.
- Am Ende jeder Task grün (vorher einmal `cargo fmt` ausführen; der Code im Plan ist rustfmt-formatiert): `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p <geänderte Crates>`; am Ende jeder Stufe (Task 6, 9, 12, 14) zusätzlich `cargo test --workspace`, `cargo deny check` und die Prüfung der unveränderten Pfade. Bricht ein Übersetzen unter Windows mit „invalid metadata files“ ab (beim Planen einmal beobachtet, parallele Übersetzung), einmal mit `cargo build -j 4` wiederholen.
- Commits mit ausdrücklichen Pfaden (nie `git add -A`), Nachricht deutsch, Schlusszeile `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- **Ehrlich berichten:** Jede Task endet mit einem Bericht, was geprüft wurde und was nicht.
- **Keine Handprüfung durch Implementierer** (der Nutzer sitzt am Rechner); Task 15 nur nach Rücksprache, ausgeführt vom Controller.

## Review Focus

Eingaben, die die Spec andeutet und die einen Nutzer am ehesten treffen; jede Zeile hat ihren Test in der genannten Task:

1. **Kegel mit negativem Radius am Ursprung, fallender Steigung, Spitze im Ursprung, gespiegelt oder gedreht** (jeder gedrehte Kegelstumpf aus „Drehen“ in der Skizze) → `CONICAL_SURFACE` mit Radius > 0, Winkel in (0, π/2), Punkte des Kernels auf der STEP-Fläche, gleiche Außennormale; eine Kegelfläche jenseits ihrer Spitze → benannte Ablehnung. Tests `the_test_bodies_cover_every_sign_of_radius_and_slope`, `cones_of_every_sign_pass_all_checks_in_every_pose`, `a_cone_face_beyond_its_apex_is_refused_by_name` (Task 8).
2. **Bögen im Uhrzeigersinn und Vollkreise** (Buchten in Profilen, Bohrungen, Zylinder) → `EDGE_CURVE` mit `.F.` bzw. derselbe `VERTEX_POINT` an Anfang und Ende, Naht ein `EDGE_CURVE` zweimal mit entgegengesetzter Richtung. Tests `arcs_running_clockwise_are_edges_against_the_curve`, `a_cylinder_has_one_seam_used_twice_and_full_circles_with_one_vertex`, `a_sphere_is_one_face_with_poles_as_vertices` (Task 7).
3. **Körper mit Hohlraum oder mehreren getrennten Stücken** (Abziehen eines inneren Körpers, Vereinigung ohne Berührung) → `BREP_WITH_VOIDS` mit umgekehrter Hohlraumschale bzw. mehrere Körper in einem Teil, Volumen stimmt, keine falsch herum liegende Fläche. Tests `a_cavity_becomes_a_brep_with_voids_with_a_reversed_shell`, `separate_pieces_are_several_solids_of_one_part`, `a_piece_inside_a_cavity_is_its_own_solid_and_the_cavity_belongs_to_the_big_box` (Task 11).
4. **Namen aus dem Feature-Baum** mit Umlauten, Hochkomma, Rückstrich, Emoji, leer oder über 256 Zeichen → als Text unverändert zurückgelesen bzw. „Körper n“, gekürzt mit Eintrag im Bericht. Tests `strings_are_quoted_and_escaped` (Task 1), `empty_names_become_numbered_bodies` (Task 3), `several_bodies_become_several_named_parts`, `long_names_are_shortened_and_reported` (Task 6).
5. **Veraltetes Ergebnis und Formatwechsel im Fenster** (Auswahl ändert sich nach der Prüfung, nur Netz-Körper im Umfang, Wechsel STEP ↔ STL, Abbrechen) → nie eine falsche Datei, benannte Meldung, das Netz-Ergebnis bleibt gültig. Tests `a_stale_step_result_is_never_written`, `only_mesh_bodies_leave_nothing_to_write`, `cancelling_the_step_check_says_so_and_switching_back_keeps_the_mesh_result`, `cancelling_step_writing_keeps_the_old_file` (Task 13).

## Dateistruktur

```
Cargo.toml                                    (ändern: planora-step in workspace.dependencies)
crates/planora-step/                          (neu)
├─ Cargo.toml
├─ src/lib.rs          write_step, check_body, StepBody, StepOptions, StepReport, BodyStats, display_name
├─ src/error.rs        StepError
├─ src/number.rs       push_real, real
├─ src/text.rs         step_string, StepText, MAX_NAME_CHARS
├─ src/entity.rs       Entities, MAX_ENTITIES, WRAP_COLUMN, refs, logical
├─ src/geometry.rs     Punkte, Richtungen, Koordinatensysteme, Kurven, Flächen (FRAME_TOL)
├─ src/topology.rs     Schalen, Flächen, Schleifen, Kanten, Eckpunkte; Stücke und Hohlräume
├─ src/product.rs      Kopf, Kontext, Einheiten, Teile; utc_timestamp, SCHEMA, SYSTEM
├─ src/file.rs         export_step_file (Hilfsdatei)
└─ tests/
   ├─ common/mod.rs    options, export, Lcg, checked
   ├─ common/reader.rs eigener Leser (Zerlegung)
   ├─ common/model.rs  Modell mit eigenen Formeln
   ├─ common/check.rs  Topologie, Geometrie, Vergleich mit dem Kernel, Netz aus STEP-Flächen
   └─ basics, entities, writer, reader, model, stage1, curved, cone, stage2, freeform, voids, stage3 (.rs)
crates/planora-app/Cargo.toml                 (ändern: planora-step)
crates/planora-app/src/export_dialog.rs       (ändern: ExportFormat, STEP-Weg)
crates/planora-app/tests/export_dialog.rs     (eine Zeile mechanisch, P14)
crates/planora-app/tests/export_step.rs       (neu)
docs: README.md, docs/superpowers/offene-punkte.md, Spec §9
```

Aufgabenzuschnitt: Stufe 1 = Task 1–6, Stufe 2 = Task 7–9, Stufe 3 = Task 10–12, Stufe 4 = Task 13–15. Erwartete Testzahlen je Datei stehen im Prüfschritt jeder Task.

---


## Stufe 1 – Gerüst: Paket, Einträge, Zahlen und Texte, Kopf/Kontext/Teile, ebene Körper, eigener Leser

### Task 1: Paketgerüst, Fehler, Zahlen und Zeichenketten

**Files:**
- Modify: `Cargo.toml` (Abschnitt `[workspace.dependencies]`, interne Crates)
- Create: `crates/planora-step/Cargo.toml`, `crates/planora-step/src/lib.rs`, `src/error.rs`, `src/number.rs`, `src/text.rs`
- Test: `crates/planora-step/tests/basics.rs`

**Interfaces:**
- Consumes: nichts aus diesem Plan; `thiserror`.
- Produces (alle `pub` in `planora_step`):
  ```rust
  pub enum StepError { Io { msg: String }, NotFinite, InvalidBody { name: String, reason: String },
      NothingToExport, TooManyEntities, Cancelled }   // Debug, Clone, PartialEq, Eq, Error; From<std::io::Error>
  pub fn push_real(out: &mut String, x: f64) -> Result<(), StepError>;
  pub fn real(x: f64) -> Result<String, StepError>;
  pub const MAX_NAME_CHARS: usize = 256;
  pub struct StepText { pub text: String, pub shortened: bool }   // Clone, Debug, PartialEq, Eq
  pub fn step_string(name: &str) -> StepText;
  ```

- [ ] **Step 1: Arbeitsbereich und Paket anlegen**

In `Cargo.toml` unter `# interne Crates` nach `planora-exchange = { path = "crates/planora-exchange" }` einfügen:

```toml
planora-step = { path = "crates/planora-step" }
```

`crates/planora-step/Cargo.toml` (`planora-mesh` zunächst nur für die Tests; Task 11 macht es zur normalen Abhängigkeit):

```toml
[package]
name = "planora-step"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
planora-math.workspace = true
planora-kernel.workspace = true
planora-exchange.workspace = true
thiserror.workspace = true

[dev-dependencies]
planora-mesh.workspace = true

[lints]
workspace = true
```

- [ ] **Step 2: Tests schreiben** (`crates/planora-step/tests/basics.rs`)

```rust
//! Fehler, Zahlen und Zeichenketten (Teilprojekt 10, Task 1).

use planora_step::*;

/// Kleiner Zufallsgenerator mit festem Startwert (keine Abhängigkeit).
struct Lcg(u64);

impl Lcg {
    fn bits(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
}

/// Liest ein STEP-REAL so, wie es ein fremdes Programm tut: Punkt Pflicht, `E` groß.
fn read_back(s: &str) -> f64 {
    assert!(s.contains('.'), "kein Punkt in {s}");
    assert!(!s.contains('e'), "kleines e in {s}");
    let (mantissa, _) = s.split_once('E').unwrap_or((s, ""));
    let (whole, _) = mantissa.split_once('.').unwrap();
    let digits = whole.trim_start_matches('-');
    assert!(
        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
        "vor dem Punkt fehlen Ziffern: {s}"
    );
    s.replace('E', "e").parse().unwrap()
}

#[test]
fn reals_always_have_a_point_and_read_back_exactly() {
    for (x, want) in [
        (1.0, "1."),
        (0.5, "0.5"),
        (-2.25, "-2.25"),
        (0.0, "0."),
        (-0.0, "0."),
        (1e-7, "1.E-7"),
        (1.5e21, "1.5E21"),
        (1e16, "1.E16"),
        (-3e-300, "-3.E-300"),
        (123456.789, "123456.789"),
        (f64::MAX, "1.7976931348623157E308"),
    ] {
        assert_eq!(real(x).unwrap(), want, "{x}");
    }
    let mut rng = Lcg(7);
    let mut checked = 0;
    while checked < 20_000 {
        let x = f64::from_bits(rng.bits());
        if !x.is_finite() {
            continue;
        }
        let s = real(x).unwrap();
        let back = read_back(&s);
        let want = if x == 0.0 { 0.0 } else { x };
        assert_eq!(back.to_bits(), want.to_bits(), "{x:?} → {s}");
        // Gültige Stellen: Ziffern der Mantisse ohne führende Nullen.
        let digits: String = s
            .split('E')
            .next()
            .unwrap()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let significant = digits.trim_start_matches('0').len();
        assert!(significant <= 17, "{s}: zu viele Stellen");
        checked += 1;
    }
}

#[test]
fn non_finite_numbers_are_refused_and_leave_the_text_alone() {
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(real(x), Err(StepError::NotFinite));
        let mut s = String::from("(1.,");
        assert_eq!(push_real(&mut s, x), Err(StepError::NotFinite));
        assert_eq!(s, "(1.,");
    }
}

#[test]
fn strings_are_quoted_and_escaped() {
    let t = |s: &str| step_string(s).text;
    assert_eq!(t("Quader 1"), "'Quader 1'");
    assert_eq!(t(""), "''");
    assert_eq!(t("O'Brien"), "'O''Brien'");
    assert_eq!(t("a\\b"), "'a\\\\b'");
    assert_eq!(t("Würfel"), "'W\\X2\\00FC\\X0\\rfel'");
    assert_eq!(t("Maß äö"), "'Ma\\X2\\00DF\\X0\\ \\X2\\00E400F6\\X0\\'");
    assert_eq!(t("Zeile\nzwei\t\u{7f}"), "'Zeile zwei  '");
    assert_eq!(t("\u{1F600}x"), "'\\X4\\0001F600\\X0\\x'");
    assert_eq!(t("é\u{1F600}"), "'\\X2\\00E9\\X0\\\\X4\\0001F600\\X0\\'");
    for name in ["Quader 1", "O'Brien", "Würfel", "\u{1F600}", ""] {
        let s = step_string(name);
        assert!(!s.shortened);
        assert!(
            s.text.chars().all(|c| (' '..='~').contains(&c)),
            "{}",
            s.text
        );
    }
}

#[test]
fn long_names_are_shortened_to_the_limit() {
    assert_eq!(MAX_NAME_CHARS, 256);
    let exact = "x".repeat(MAX_NAME_CHARS);
    let s = step_string(&exact);
    assert!(!s.shortened);
    assert_eq!(s.text.len(), MAX_NAME_CHARS + 2);
    let long = "ä".repeat(MAX_NAME_CHARS + 1);
    let s = step_string(&long);
    assert!(s.shortened);
    assert_eq!(s.text.matches("00E4").count(), MAX_NAME_CHARS);
}

#[test]
fn error_texts_are_german() {
    assert_eq!(StepError::Cancelled.to_string(), "Abgebrochen");
    let e = StepError::InvalidBody {
        name: "Quader 1".into(),
        reason: "Die Hülle ist offen".into(),
    };
    assert_eq!(
        e.to_string(),
        "„Quader 1“ wird nicht ausgegeben: Die Hülle ist offen"
    );
    assert!(
        StepError::TooManyEntities
            .to_string()
            .contains("500 Millionen")
    );
    assert!(
        StepError::NothingToExport
            .to_string()
            .contains("keinen Körper")
    );
    assert!(
        StepError::NotFinite
            .to_string()
            .contains("keine gültige Zahl")
    );
    let io: StepError = std::io::Error::other("voll").into();
    assert!(matches!(io, StepError::Io { .. }));
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test basics`
Expected: FAIL – `src/lib.rs` fehlt bzw. `cannot find function real`.

- [ ] **Step 4: Implementieren**

`crates/planora-step/src/lib.rs`:

```rust
//! STEP-Export (Teilprojekt 10): exakte Körper des Kernels als STEP-Datei (AP214,
//! `AUTOMOTIVE_DESIGN`). Das Textformat ist von Hand geschrieben, ohne neue Abhängigkeit.

pub mod error;
pub mod number;
pub mod text;

pub use error::StepError;
pub use number::{push_real, real};
pub use text::{MAX_NAME_CHARS, StepText, step_string};
```

`crates/planora-step/src/error.rs`:

```rust
//! Fehler beim Schreiben von STEP-Dateien (Meldungen deutsch, für die Oberfläche bestimmt).

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StepError {
    #[error("Die Datei lässt sich nicht schreiben: {msg}")]
    Io { msg: String },
    #[error("Eine Zahl ist zu groß oder keine gültige Zahl")]
    NotFinite,
    #[error("„{name}“ wird nicht ausgegeben: {reason}")]
    InvalidBody { name: String, reason: String },
    #[error("Es gibt keinen Körper, der sich als STEP-Datei ausgeben lässt")]
    NothingToExport,
    #[error("Die Datei würde zu groß: mehr als 500 Millionen Einträge")]
    TooManyEntities,
    #[error("Abgebrochen")]
    Cancelled,
}

impl From<std::io::Error> for StepError {
    fn from(e: std::io::Error) -> Self {
        StepError::Io { msg: e.to_string() }
    }
}
```

`crates/planora-step/src/number.rs`:

```rust
//! Zahlen im STEP-Format (REAL, Spec §4): kürzeste Darstellung, die verlustfrei zurückgelesen
//! wird (höchstens 17 gültige Stellen), immer mit Punkt, Exponent mit großem `E`.

use crate::error::StepError;

/// Hängt `x` als STEP-REAL an `out` an: `1.`, `0.5`, `-2.25`, `1.E-7`, `1.5E21`; `-0` wird `0.`.
/// Nicht endliche Werte ergeben `NotFinite`, `out` bleibt dann unverändert.
pub fn push_real(out: &mut String, x: f64) -> Result<(), StepError> {
    if !x.is_finite() {
        return Err(StepError::NotFinite);
    }
    // `-0.0 == 0.0`: beide werden `0.`.
    let x = if x == 0.0 { 0.0 } else { x };
    // `{:?}` ist die kürzeste Darstellung, die beim Lesen wieder genau `x` ergibt.
    let s = format!("{x:?}");
    match s.split_once('e') {
        Some((mantissa, exponent)) => {
            out.push_str(mantissa);
            if !mantissa.contains('.') {
                out.push('.');
            }
            out.push('E');
            out.push_str(exponent);
        }
        None => match s.strip_suffix(".0") {
            Some(whole) => {
                out.push_str(whole);
                out.push('.');
            }
            None => out.push_str(&s),
        },
    }
    Ok(())
}

/// `x` als STEP-REAL (siehe [`push_real`]).
pub fn real(x: f64) -> Result<String, StepError> {
    let mut s = String::new();
    push_real(&mut s, x)?;
    Ok(s)
}
```

`crates/planora-step/src/text.rs`:

```rust
//! Zeichenketten im STEP-Format (Spec §4): in Hochkommas; `'` wird `''`, `\` wird `\\`; Zeichen
//! außerhalb von ASCII als `\X2\…\X0\` (vier Hexziffern je Zeichen) bzw. `\X4\…\X0\` (acht
//! Hexziffern, Zeichen jenseits von U+FFFF); Steuerzeichen werden Leerzeichen.

use std::fmt::Write as _;

/// Höchstzahl der Zeichen eines Namens (Spec §4); längere Namen werden gekürzt.
pub const MAX_NAME_CHARS: usize = 256;
/// Größtes Zeichen, das in einen `\X2\`-Abschnitt passt (vier Hexziffern).
const X2_MAX: u32 = 0xFFFF;

/// Fertige STEP-Zeichenkette samt Hinweis, ob gekürzt wurde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepText {
    /// Mit Hochkommas, nur druckbares ASCII.
    pub text: String,
    pub shortened: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Run {
    Ascii,
    X2,
    X4,
}

/// Wandelt `name` in eine STEP-Zeichenkette (höchstens [`MAX_NAME_CHARS`] Zeichen).
pub fn step_string(name: &str) -> StepText {
    let shortened = name.chars().count() > MAX_NAME_CHARS;
    let mut out = String::from("'");
    let mut run = Run::Ascii;
    for c in name.chars().take(MAX_NAME_CHARS) {
        let c = if c.is_control() { ' ' } else { c };
        let want = if c.is_ascii() {
            Run::Ascii
        } else if u32::from(c) <= X2_MAX {
            Run::X2
        } else {
            Run::X4
        };
        if want != run {
            if run != Run::Ascii {
                out.push_str("\\X0\\");
            }
            match want {
                Run::X2 => out.push_str("\\X2\\"),
                Run::X4 => out.push_str("\\X4\\"),
                Run::Ascii => {}
            }
            run = want;
        }
        match want {
            Run::Ascii => match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            },
            Run::X2 => {
                let _ = write!(out, "{:04X}", u32::from(c));
            }
            Run::X4 => {
                let _ = write!(out, "{:08X}", u32::from(c));
            }
        }
    }
    if run != Run::Ascii {
        out.push_str("\\X0\\");
    }
    out.push('\'');
    StepText {
        text: out,
        shortened,
    }
}
```

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (5 Tests in `basics`), keine Warnungen. Die Zufallsprobe liest 20 000 zufällige Bitmuster bitgleich zurück.

```bash
git add Cargo.toml Cargo.lock crates/planora-step/Cargo.toml crates/planora-step/src/lib.rs crates/planora-step/src/error.rs crates/planora-step/src/number.rs crates/planora-step/src/text.rs crates/planora-step/tests/basics.rs
git commit -m "step: Paket mit Fehlern, REAL-Zahlen und Zeichenketten

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Eintragsverwaltung

**Files:**
- Create: `crates/planora-step/src/entity.rs`
- Modify: `crates/planora-step/src/lib.rs` (ganz ersetzen)
- Test: `crates/planora-step/tests/entities.rs`

**Interfaces:**
- Consumes: `StepError` (Task 1); `planora_exchange::{Progress, CHECK_EVERY}` (`CHECK_EVERY = 4096`).
- Produces:
  ```rust
  pub const MAX_ENTITIES: u64 = 500_000_000;
  pub const WRAP_COLUMN: usize = 72;
  pub struct Entities<'a>;
  impl<'a> Entities<'a> {
      pub fn new(out: &'a mut dyn std::io::Write, progress: &'a Progress) -> Entities<'a>;   // zählt im Fortschritt
      pub fn probe(out: &'a mut dyn std::io::Write, progress: &'a Progress) -> Entities<'a>; // zählt nicht
      pub fn count(&self) -> u64;
      pub fn set_next(&mut self, next: u64);            // nur für Tests der Grenze
      pub fn push(&mut self, text: &str) -> Result<u64, StepError>;   // schreibt `#n=text;`
      pub fn line(&mut self, text: &str) -> Result<(), StepError>;    // Zeile ohne Nummer
      pub fn flush(&mut self) -> Result<(), StepError>;
  }
  pub fn refs(ids: &[u64]) -> String;          // "(#1,#2)"
  pub fn logical(b: bool) -> &'static str;     // ".T." / ".F."
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/entities.rs`)

```rust
//! Eintragsverwaltung: Nummern, Zeilen, Umbruch, Grenze und Abbrechen (Teilprojekt 10, Task 2).

use planora_exchange::{CHECK_EVERY, Progress};
use planora_step::*;

#[test]
fn entities_are_numbered_from_one_and_end_with_a_semicolon() {
    let mut out = Vec::new();
    let p = Progress::new();
    let mut e = Entities::new(&mut out, &p);
    e.line("DATA;").unwrap();
    assert_eq!(e.push("CARTESIAN_POINT('',(0.,0.,0.))").unwrap(), 1);
    assert_eq!(e.push("VERTEX_POINT('',#1)").unwrap(), 2);
    assert_eq!(e.count(), 2);
    e.flush().unwrap();
    drop(e);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "DATA;\n#1=CARTESIAN_POINT('',(0.,0.,0.));\n#2=VERTEX_POINT('',#1);\n"
    );
    p.set_total(4);
    assert!((p.fraction() - 0.5).abs() < 1e-6, "jeder Eintrag zählt");
}

#[test]
fn long_entities_wrap_after_commas_but_never_inside_strings() {
    let mut out = Vec::new();
    let p = Progress::new();
    let mut e = Entities::new(&mut out, &p);
    let ids: Vec<u64> = (1..=60).collect();
    let name = format!("'{}'", "a,b''c,".repeat(30));
    let text = format!("CLOSED_SHELL({name},{})", refs(&ids));
    e.push(&text).unwrap();
    drop(e);
    let s = String::from_utf8(out).unwrap();
    assert!(s.lines().count() > 2, "{s}");
    for line in s.lines() {
        // Jede Zeile außer der letzten endet mit einem Komma, und die Zeichenkette steht ganz
        // in einer Zeile.
        assert!(line.ends_with(',') || line.ends_with(");"), "{line}");
    }
    assert!(s.lines().next().unwrap().contains(&name));
    assert_eq!(s.replace('\n', ""), format!("#1={text};"));
}

#[test]
fn refs_and_logicals_have_the_step_form() {
    assert_eq!(refs(&[]), "()");
    assert_eq!(refs(&[7]), "(#7)");
    assert_eq!(refs(&[1, 22, 333]), "(#1,#22,#333)");
    assert_eq!(logical(true), ".T.");
    assert_eq!(logical(false), ".F.");
    assert_eq!(WRAP_COLUMN, 72);
}

#[test]
fn the_entity_limit_is_a_named_error() {
    assert_eq!(MAX_ENTITIES, 500_000_000);
    let mut out = Vec::new();
    let p = Progress::new();
    let mut e = Entities::new(&mut out, &p);
    e.set_next(MAX_ENTITIES);
    assert_eq!(e.push("A()").unwrap(), MAX_ENTITIES);
    assert_eq!(e.push("B()"), Err(StepError::TooManyEntities));
    drop(e);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        format!("#{MAX_ENTITIES}=A();\n")
    );
}

#[test]
fn cancelling_stops_at_the_next_check_and_the_probe_does_not_count() {
    let mut sink = std::io::sink();
    let p = Progress::new();
    p.cancel();
    let mut e = Entities::probe(&mut sink, &p);
    for i in 1..CHECK_EVERY {
        assert_eq!(e.push("A()").unwrap(), i);
    }
    assert_eq!(e.push("A()"), Err(StepError::Cancelled));
    let q = Progress::new();
    q.set_total(10);
    let mut e = Entities::probe(&mut sink, &q);
    e.push("A()").unwrap();
    assert_eq!(q.fraction(), 0.0, "der Probelauf zählt nicht");
}

#[test]
fn write_errors_become_io_errors() {
    struct Full;
    impl std::io::Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("Datenträger voll"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut full = Full;
    let p = Progress::new();
    let mut e = Entities::new(&mut full, &p);
    assert!(matches!(e.push("A()"), Err(StepError::Io { .. })));
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test entities`
Expected: FAIL – `cannot find type Entities`.

- [ ] **Step 3: Implementieren**

`crates/planora-step/src/entity.rs`:

```rust
//! Eintragsverwaltung (Spec §2, §4): fortlaufende Nummern `#1, #2, …`, ein Eintrag je Zeile
//! (lange Einträge nach einem Komma umbrochen), höchstens [`MAX_ENTITIES`] Einträge, Abbrechen
//! über [`Progress`]. Geschrieben wird sofort in den Ausgabestrom, nichts sammelt sich an.

use crate::error::StepError;
use planora_exchange::{CHECK_EVERY, Progress};
use std::fmt::Write as _;
use std::io::Write;

/// Höchstzahl der Einträge einer Datei (Spec §4).
pub const MAX_ENTITIES: u64 = 500_000_000;
/// Ab dieser Spalte wird ein langer Eintrag nach dem nächsten Komma außerhalb einer
/// Zeichenkette umbrochen (die Zeilen bleiben für Menschen und strenge Leser lesbar).
pub const WRAP_COLUMN: usize = 72;

/// Schreibt Einträge mit fortlaufender Nummer.
pub struct Entities<'a> {
    out: &'a mut dyn Write,
    progress: &'a Progress,
    next: u64,
    /// Jeder Eintrag zählt im Fortschritt (beim Probelauf nicht).
    count_progress: bool,
    line: String,
}

impl<'a> Entities<'a> {
    /// Schreibt nach `out`; jeder Eintrag zählt einen Schritt in `progress`.
    pub fn new(out: &'a mut dyn Write, progress: &'a Progress) -> Entities<'a> {
        Entities {
            out,
            progress,
            next: 1,
            count_progress: true,
            line: String::new(),
        }
    }

    /// Probelauf (etwa nach `std::io::sink()`): zählt nicht im Fortschritt, bricht aber ab.
    pub fn probe(out: &'a mut dyn Write, progress: &'a Progress) -> Entities<'a> {
        Entities {
            count_progress: false,
            ..Entities::new(out, progress)
        }
    }

    /// Zahl der bisher geschriebenen Einträge.
    pub fn count(&self) -> u64 {
        self.next - 1
    }

    /// Setzt die nächste Nummer (nur für Tests der Grenze [`MAX_ENTITIES`]).
    pub fn set_next(&mut self, next: u64) {
        self.next = next;
    }

    /// Schreibt `#n=text;` und liefert `n`. Alle [`CHECK_EVERY`] Einträge wird gefragt, ob
    /// abgebrochen werden soll.
    pub fn push(&mut self, text: &str) -> Result<u64, StepError> {
        if self.next > MAX_ENTITIES {
            return Err(StepError::TooManyEntities);
        }
        if self.next.is_multiple_of(CHECK_EVERY) && self.progress.is_cancelled() {
            return Err(StepError::Cancelled);
        }
        let id = self.next;
        self.line.clear();
        let _ = write!(self.line, "#{id}=");
        push_wrapped(&mut self.line, text);
        self.line.push_str(";\n");
        self.out.write_all(self.line.as_bytes())?;
        self.next += 1;
        if self.count_progress {
            self.progress.advance(1);
        }
        Ok(id)
    }

    /// Schreibt eine Zeile ohne Nummer (Kopf und Abschnittsgrenzen).
    pub fn line(&mut self, text: &str) -> Result<(), StepError> {
        self.out.write_all(text.as_bytes())?;
        self.out.write_all(b"\n")?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), StepError> {
        self.out.flush()?;
        Ok(())
    }
}

/// Hängt `text` an `line` an und bricht nach einem Komma außerhalb einer Zeichenkette um, sobald
/// die Zeile [`WRAP_COLUMN`] Zeichen erreicht hat. `''` in einer Zeichenkette schaltet zweimal
/// um und ändert daher nichts.
fn push_wrapped(line: &mut String, text: &str) {
    let mut column = line.len();
    let mut in_string = false;
    for c in text.chars() {
        line.push(c);
        column += 1;
        if c == '\'' {
            in_string = !in_string;
        } else if c == ',' && !in_string && column >= WRAP_COLUMN {
            line.push('\n');
            column = 0;
        }
    }
}

/// Liste von Verweisen: `(#1,#2,#3)`.
pub fn refs(ids: &[u64]) -> String {
    let mut s = String::with_capacity(2 + ids.len() * 8);
    s.push('(');
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "#{id}");
    }
    s.push(')');
    s
}

/// STEP-Wahrheitswert: `.T.` oder `.F.`.
pub fn logical(b: bool) -> &'static str {
    if b { ".T." } else { ".F." }
}
```

`crates/planora-step/src/lib.rs` (ganz ersetzen):

```rust
//! STEP-Export (Teilprojekt 10): exakte Körper des Kernels als STEP-Datei (AP214,
//! `AUTOMOTIVE_DESIGN`). Das Textformat ist von Hand geschrieben, ohne neue Abhängigkeit.

pub mod entity;
pub mod error;
pub mod number;
pub mod text;

pub use entity::{Entities, MAX_ENTITIES, WRAP_COLUMN, logical, refs};
pub use error::StepError;
pub use number::{push_real, real};
pub use text::{MAX_NAME_CHARS, StepText, step_string};
```

- [ ] **Step 4: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (5 in `basics`, 6 in `entities`).

```bash
git add crates/planora-step/src/entity.rs crates/planora-step/src/lib.rs crates/planora-step/tests/entities.rs
git commit -m "step: Eintragsverwaltung mit Umbruch, Grenze und Abbrechen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Schreiber für ebene Körper – Kopf, Kontext, Teile, Topologie, Prüfung vor dem Schreiben

**Files:**
- Create: `crates/planora-step/src/geometry.rs`, `src/topology.rs`, `src/product.rs`
- Modify: `crates/planora-step/src/lib.rs` (ganz ersetzen)
- Test: `crates/planora-step/tests/writer.rs`

**Interfaces:**
- Consumes: Task 1–2; `planora_kernel::{Solid, Face, Edge, Vertex, Loop, Id, Curve, Surface, Curve::{Line, Circle}, Surface::Plane, validate_with, NURBS_EDGE_TOL}`, `planora_math::{DVec3, Frame, Tolerance}`, `Frame::is_orthonormal(eps)`.
- Produces (`pub` in `planora_step`):
  ```rust
  pub struct StepBody<'a> { pub name: String, pub solid: &'a Solid }                 // Clone, Debug
  pub struct StepOptions { pub file_name: String, pub time: SystemTime, pub tolerance: Tolerance }
  impl StepOptions { pub fn new(file_name: &str) -> StepOptions; }                 // jetzt, Tolerance::default()
  pub struct StepReport { pub bodies: usize, pub faces: usize, pub edges: usize, pub entities: u64,
      pub rejected: Vec<StepError>, pub shortened_names: Vec<String> }             // Clone, Debug, Default, PartialEq
  pub struct BodyStats { pub faces: usize, pub edges: usize, pub entities: u64 }   // Clone, Copy, Debug, PartialEq, Eq
  pub fn display_name(name: &str, index: usize) -> String;                         // leer → "Körper {index+1}"
  pub fn check_body(body: &StepBody, progress: &Progress) -> Result<BodyStats, StepError>;
  pub fn write_step(bodies: &[StepBody], opts: &StepOptions, out: &mut dyn Write, progress: &Progress)
      -> Result<StepReport, StepError>;
  pub fn utc_timestamp(t: SystemTime) -> String;                                    // "YYYY-MM-DDThh:mm:ss"
  pub const SCHEMA: &str = "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }";
  pub const SYSTEM: &str = "PlanoraCAD";
  pub const FRAME_TOL: f64 = 1e-9;
  ```
  Innerhalb des Pakets (für Task 6–11): `geometry::{reject(&str) -> StepError, point, direction, placement(e, origin, z, x), frame_placement(e, &Frame), curve(e, &Curve), surface(e, &Surface)}` (alle `-> Result<u64, StepError>` außer `reject`), `topology::{WrittenSolid { items, faces, edges }, ids_in_range(&Solid) -> bool, shell_faces(&Solid) -> Vec<Vec<Id<Face>>>, write_solid(e, name, &Solid) -> Result<WrittenSolid, StepError>}`, `product::{header, context, product, trailer}`.
  Stufenweise: Kurven `Ellipse`/`Nurbs`, Flächen außer `Plane` und Körper mit mehreren Schalen ergeben bis Task 7/8/10/11 die benannte Ablehnung „… wird noch nicht ausgegeben“ (kein Test hängt daran; die späteren Tasks ersetzen genau diese Zweige).

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/writer.rs`)

```rust
//! Schreiber für ebene Körper: Kopf, Kontext, Teile, Topologie (Teilprojekt 10, Task 3).

use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, Kernel, Solid, validate};
use planora_math::DVec3;
use planora_step::*;
use std::time::{Duration, UNIX_EPOCH};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn cube(size: f64) -> Solid {
    k().make_box(DVec3::splat(size)).unwrap()
}

/// Feste Einstellungen (gleiche Datei bei gleichem Inhalt).
fn opts() -> StepOptions {
    StepOptions {
        time: UNIX_EPOCH + Duration::from_secs(1_791_981_296),
        ..StepOptions::new("probe.step")
    }
}

fn export(bodies: &[StepBody]) -> (String, StepReport) {
    let mut out = Vec::new();
    let report = write_step(bodies, &opts(), &mut out, &Progress::new()).unwrap();
    (String::from_utf8(out).unwrap(), report)
}

fn body<'a>(name: &str, solid: &'a Solid) -> StepBody<'a> {
    StepBody {
        name: name.to_string(),
        solid,
    }
}

/// Einträge `#n=…;` (Zeilenumbrüche entfernt) in Reihenfolge.
fn entities(text: &str) -> Vec<(u64, String)> {
    let data = text.split("DATA;").nth(1).unwrap();
    let data = data.split("ENDSEC;").next().unwrap().replace('\n', "");
    data.split(';')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let (id, body) = s.split_once('=').unwrap();
            (
                id.trim_start_matches('#').parse().unwrap(),
                body.to_string(),
            )
        })
        .collect()
}

fn count(text: &str, name: &str) -> usize {
    entities(text)
        .iter()
        .filter(|(_, b)| b.starts_with(&format!("{name}(")))
        .count()
}

#[test]
fn a_cube_becomes_one_part_with_shared_edges_and_vertices() {
    let s = cube(10.0);
    let (text, report) = export(&[body("Quader 1", &s)]);
    assert_eq!(
        (report.bodies, report.faces, report.edges),
        (1, 6, 12),
        "{report:?}"
    );
    assert!(report.rejected.is_empty() && report.shortened_names.is_empty());
    for (name, n) in [
        ("ADVANCED_FACE", 6),
        ("PLANE", 6),
        ("FACE_OUTER_BOUND", 6),
        ("FACE_BOUND", 0),
        ("EDGE_LOOP", 6),
        ("ORIENTED_EDGE", 24),
        ("EDGE_CURVE", 12),
        ("LINE", 12),
        ("VERTEX_POINT", 8),
        ("CLOSED_SHELL", 1),
        ("MANIFOLD_SOLID_BREP", 1),
        ("ADVANCED_BREP_SHAPE_REPRESENTATION", 1),
        ("PRODUCT", 1),
        ("PRODUCT_DEFINITION", 1),
        ("SHAPE_DEFINITION_REPRESENTATION", 1),
        ("APPLICATION_CONTEXT", 1),
    ] {
        assert_eq!(count(&text, name), n, "{name}");
    }
    assert!(text.contains("MANIFOLD_SOLID_BREP('Quader 1',"));
    assert!(text.contains("PRODUCT('Quader 1','Quader 1','',("));
    assert_eq!(report.entities, entities(&text).len() as u64);
}

#[test]
fn the_header_names_schema_time_and_units() {
    let s = cube(1.0);
    let (text, _) = export(&[body("Würfel", &s)]);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "ISO-10303-21;");
    assert_eq!(lines[1], "HEADER;");
    assert_eq!(lines[2], "FILE_DESCRIPTION(('PlanoraCAD Export'),'2;1');");
    assert_eq!(
        lines[3],
        "FILE_NAME('probe.step','2026-10-14T12:34:56',(''),(''),'PlanoraCAD','PlanoraCAD','');"
    );
    assert_eq!(
        lines[4],
        "FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));"
    );
    assert_eq!(lines[5], "ENDSEC;");
    assert_eq!(lines[6], "DATA;");
    assert_eq!(lines[lines.len() - 2], "ENDSEC;");
    assert_eq!(lines[lines.len() - 1], "END-ISO-10303-21;");
    assert!(text.ends_with("END-ISO-10303-21;\n"));
    assert!(text.contains("SI_UNIT(.MILLI.,.METRE.)"));
    assert!(text.contains("SI_UNIT($,.RADIAN.)"));
    assert!(text.contains("SI_UNIT($,.STERADIAN.)"));
    assert!(
        text.contains("LENGTH_MEASURE(1.E-6)"),
        "Unsicherheit = Kernel-Toleranz"
    );
    assert!(text.contains("'W\\X2\\00FC\\X0\\rfel'"));
}

#[test]
fn numbers_run_from_one_and_references_point_backwards() {
    let a = cube(2.0);
    let b = k()
        .transform(
            &cube(3.0),
            &planora_math::DMat4::from_translation(DVec3::X * 10.0),
        )
        .unwrap();
    let (text, report) = export(&[body("A", &a), body("B", &b)]);
    assert_eq!(report.bodies, 2);
    let list = entities(&text);
    for (i, (id, body)) in list.iter().enumerate() {
        assert_eq!(*id, i as u64 + 1);
        for part in body.split('#').skip(1) {
            let n: u64 = part
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap();
            assert!(n < *id, "#{id} verweist auf #{n}");
        }
    }
    assert_eq!(count(&text, "PRODUCT"), 2);
    assert_eq!(count(&text, "APPLICATION_CONTEXT"), 1, "Kontext gemeinsam");
}

#[test]
fn the_same_bodies_give_the_same_bytes() {
    let s = cube(5.0);
    assert_eq!(export(&[body("Q", &s)]).0, export(&[body("Q", &s)]).0);
}

#[test]
fn invalid_bodies_are_left_out_by_name_and_the_rest_is_written() {
    let good = cube(1.0);
    let mut open = cube(1.0);
    let first = open.faces.iter().next().unwrap().0;
    open.faces[first].outer.half_edges.pop();
    let mut nan = cube(1.0);
    let v = nan.vertices.iter().next().unwrap().0;
    nan.vertices[v].point.x = f64::NAN;
    validate(&nan).unwrap();
    let (text, report) = export(&[body("Offen", &open), body("Gut", &good), body("NaN", &nan)]);
    assert_eq!(report.bodies, 1);
    assert_eq!(report.rejected.len(), 2, "{report:?}");
    match &report.rejected[0] {
        StepError::InvalidBody { name, reason } => {
            assert_eq!(name, "Offen");
            assert!(!reason.is_empty());
        }
        other => panic!("{other:?}"),
    }
    match &report.rejected[1] {
        StepError::InvalidBody { name, reason } => {
            assert_eq!(name, "NaN");
            assert!(reason.contains("keine gültige Zahl"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    assert!(!text.contains("NaN") && !text.contains("inf"));
    assert_eq!(count(&text, "PRODUCT"), 1);
    let mut out = Vec::new();
    assert_eq!(
        write_step(&[body("Offen", &open)], &opts(), &mut out, &Progress::new()),
        Err(StepError::NothingToExport)
    );
    assert!(out.is_empty(), "nichts geschrieben");
}

#[test]
fn dangling_references_are_refused_without_a_crash() {
    let mut s = cube(1.0);
    let first = s.faces.iter().next().unwrap().0;
    s.faces[first].outer.half_edges[0].edge = planora_kernel::Id::from_index(999);
    let err = check_body(&body("Kaputt", &s), &Progress::new()).unwrap_err();
    assert!(
        matches!(&err, StepError::InvalidBody { name, .. } if name == "Kaputt"),
        "{err:?}"
    );
}

#[test]
fn check_body_counts_faces_edges_and_entities() {
    let s = cube(1.0);
    let stats = check_body(&body("Q", &s), &Progress::new()).unwrap();
    assert_eq!((stats.faces, stats.edges), (6, 12));
    // 8 Punkte + 8 Eckpunkte, 12 × (Punkt, Richtung, Vektor, Gerade, Kante), 24 orientierte
    // Kanten, 6 × (Schleife, Rand, 4 Einträge Ebene, Fläche), Schale, Körper.
    assert_eq!(stats.entities, 16 + 60 + 24 + 48 + 2);
}

#[test]
fn empty_names_become_numbered_bodies() {
    assert_eq!(display_name("  ", 2), "Körper 3");
    assert_eq!(display_name("Quader 1", 0), "Quader 1");
    let s = cube(1.0);
    let (text, _) = export(&[body("", &s)]);
    assert!(text.contains("PRODUCT('K\\X2\\00F6\\X0\\rper 1'"), "{text}");
}

#[test]
fn timestamps_are_utc() {
    assert_eq!(utc_timestamp(UNIX_EPOCH), "1970-01-01T00:00:00");
    assert_eq!(
        utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_709_251_199)),
        "2024-02-29T23:59:59"
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_791_981_296)),
        "2026-10-14T12:34:56"
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH - Duration::from_secs(10)),
        "1970-01-01T00:00:00"
    );
}

#[test]
fn cancelling_during_writing_is_a_named_error() {
    let s = cube(1.0);
    let bodies: Vec<StepBody> = (0..40).map(|_| body("Q", &s)).collect();
    let p = Progress::new();
    p.cancel();
    let mut out = Vec::new();
    assert_eq!(
        write_step(&bodies, &opts(), &mut out, &p),
        Err(StepError::Cancelled)
    );
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test writer`
Expected: FAIL – `cannot find function write_step`.

- [ ] **Step 3: Implementieren**

`crates/planora-step/src/geometry.rs`:

```rust
//! Geometrie (Spec §3): Punkte, Richtungen, Koordinatensysteme, Kurven und Flächen.

use crate::entity::Entities;
use crate::error::StepError;
use crate::number::push_real;
use planora_kernel::{Curve, Surface};
use planora_math::{DVec3, Frame};

/// Abweichung, bis zu der ein Koordinatensystem des Kernels als senkrecht und normiert gilt.
/// Die Systeme des Kernels weichen um Rundungsreste (etwa 1e-15) ab; schief gebaute Systeme
/// werden benannt abgelehnt statt falsch geschrieben.
pub const FRAME_TOL: f64 = 1e-9;

/// Ablehnung eines Körpers ohne Namen; [`crate::check_body`] setzt den Namen ein.
pub(crate) fn reject(reason: &str) -> StepError {
    StepError::InvalidBody {
        name: String::new(),
        reason: reason.to_string(),
    }
}

/// Hängt `(x,y,z)` an.
fn push_triple(s: &mut String, v: DVec3) -> Result<(), StepError> {
    s.push('(');
    push_real(s, v.x)?;
    s.push(',');
    push_real(s, v.y)?;
    s.push(',');
    push_real(s, v.z)?;
    s.push(')');
    Ok(())
}

pub(crate) fn point(e: &mut Entities, p: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("CARTESIAN_POINT('',");
    push_triple(&mut s, p)?;
    s.push(')');
    e.push(&s)
}

pub(crate) fn direction(e: &mut Entities, d: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("DIRECTION('',");
    push_triple(&mut s, d)?;
    s.push(')');
    e.push(&s)
}

/// `AXIS2_PLACEMENT_3D` mit Ursprung, z-Achse (`axis`) und x-Achse (`ref_direction`).
pub(crate) fn placement(
    e: &mut Entities,
    origin: DVec3,
    z: DVec3,
    x: DVec3,
) -> Result<u64, StepError> {
    let p = point(e, origin)?;
    let a = direction(e, z)?;
    let r = direction(e, x)?;
    e.push(&format!("AXIS2_PLACEMENT_3D('',#{p},#{a},#{r})"))
}

/// Koordinatensystem des Kernels; schiefe oder nicht normierte Systeme werden abgelehnt.
pub(crate) fn frame_placement(e: &mut Entities, f: &Frame) -> Result<u64, StepError> {
    if !f.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    placement(e, f.origin, f.z, f.x)
}

/// Eintrag `NAME('',#placement,zahl,…)`.
fn with_numbers(
    e: &mut Entities,
    name: &str,
    placement: u64,
    numbers: &[f64],
) -> Result<u64, StepError> {
    let mut s = format!("{name}('',#{placement}");
    for x in numbers {
        s.push(',');
        push_real(&mut s, *x)?;
    }
    s.push(')');
    e.push(&s)
}

/// Prüft eine Länge, die in STEP positiv sein muss (Radius, Halbachse).
fn positive(x: f64, what: &str) -> Result<f64, StepError> {
    if !x.is_finite() {
        return Err(StepError::NotFinite);
    }
    if x > 0.0 {
        Ok(x)
    } else {
        Err(reject(&format!("{what} ist nicht größer als null")))
    }
}

/// Trägerkurve einer Kante.
pub(crate) fn curve(e: &mut Entities, c: &Curve) -> Result<u64, StepError> {
    match c {
        Curve::Line { origin, dir } => {
            let p = point(e, *origin)?;
            let d = direction(e, *dir)?;
            let v = e.push(&format!("VECTOR('',#{d},1.)"))?;
            e.push(&format!("LINE('',#{p},#{v})"))
        }
        Curve::Circle { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Kreises")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CIRCLE", a, &[r])
        }
        Curve::Ellipse { .. } | Curve::Nurbs(_) => {
            Err(reject("Diese Kantenart wird noch nicht ausgegeben"))
        }
    }
}

/// Trägerfläche einer Fläche.
pub(crate) fn surface(e: &mut Entities, s: &Surface) -> Result<u64, StepError> {
    match s {
        Surface::Plane { frame } => {
            let a = frame_placement(e, frame)?;
            e.push(&format!("PLANE('',#{a})"))
        }
        _ => Err(reject("Diese Flächenart wird noch nicht ausgegeben")),
    }
}
```

`crates/planora-step/src/topology.rs`:

```rust
//! Topologie (Spec §3): Körper, Hüllen, Flächen, Schleifen, Kanten und Eckpunkte. Jede Kante
//! und jeder Eckpunkt wird je Körper einmal geschrieben und von allen Flächen gemeinsam benutzt
//! (Nähte: ein `EDGE_CURVE`, zwei `ORIENTED_EDGE` mit entgegengesetzter Richtung).

use crate::entity::{Entities, logical, refs};
use crate::error::StepError;
use crate::geometry::{self, reject};
use planora_kernel::{Edge, Face, Id, Loop, Solid, Vertex};

/// Ergebnis eines geschriebenen Körpers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WrittenSolid {
    /// Darstellungselemente für `ADVANCED_BREP_SHAPE_REPRESENTATION`.
    pub items: Vec<u64>,
    pub faces: usize,
    pub edges: usize,
}

/// Verweisen alle Schleifen, Kanten und Schalen auf vorhandene Teile? Sonst würde die Prüfung
/// des Kernels beim Zugriff abbrechen.
pub(crate) fn ids_in_range(solid: &Solid) -> bool {
    let (nv, ne, nf) = (solid.vertices.len(), solid.edges.len(), solid.faces.len());
    let edges_ok = solid
        .edges
        .iter()
        .all(|(_, e)| e.start.index() < nv && e.end.index() < nv);
    let loops_ok = solid.faces.iter().all(|(_, f)| {
        std::iter::once(&f.outer)
            .chain(f.inner.iter())
            .all(|lp| lp.half_edges.iter().all(|he| he.edge.index() < ne))
    });
    let shells_ok = solid
        .shells
        .iter()
        .all(|s| s.faces.iter().all(|f| f.index() < nf));
    edges_ok && loops_ok && shells_ok
}

/// Flächen der Schalen (ohne Schalenliste: alle Flächen als eine Schale, wie im Kernel).
pub(crate) fn shell_faces(solid: &Solid) -> Vec<Vec<Id<Face>>> {
    if solid.shells.is_empty() {
        vec![solid.faces.iter().map(|(id, _)| id).collect()]
    } else {
        solid.shells.iter().map(|s| s.faces.clone()).collect()
    }
}

/// Schreibt Teile eines Körpers; merkt sich schon geschriebene Kanten und Eckpunkte.
struct TopoWriter<'s> {
    solid: &'s Solid,
    vertices: Vec<Option<u64>>,
    edges: Vec<Option<u64>>,
    edge_count: usize,
}

impl<'s> TopoWriter<'s> {
    fn new(solid: &'s Solid) -> TopoWriter<'s> {
        TopoWriter {
            solid,
            vertices: vec![None; solid.vertices.len()],
            edges: vec![None; solid.edges.len()],
            edge_count: 0,
        }
    }

    fn vertex(&mut self, e: &mut Entities, id: Id<Vertex>) -> Result<u64, StepError> {
        if let Some(n) = self.vertices[id.index()] {
            return Ok(n);
        }
        let p = geometry::point(e, self.solid.vertices[id].point)?;
        let n = e.push(&format!("VERTEX_POINT('',#{p})"))?;
        self.vertices[id.index()] = Some(n);
        Ok(n)
    }

    /// `EDGE_CURVE` vom Anfangs- zum Endpunkt; läuft die Kante gegen den Kurvenparameter
    /// (`t1 < t0`), ist `same_sense` falsch.
    fn edge(&mut self, e: &mut Entities, id: Id<Edge>) -> Result<u64, StepError> {
        if let Some(n) = self.edges[id.index()] {
            return Ok(n);
        }
        let edge = &self.solid.edges[id];
        let start = self.vertex(e, edge.start)?;
        let end = self.vertex(e, edge.end)?;
        let curve = geometry::curve(e, &edge.curve)?;
        let sense = logical(edge.t1 >= edge.t0);
        let n = e.push(&format!("EDGE_CURVE('',#{start},#{end},#{curve},{sense})"))?;
        self.edges[id.index()] = Some(n);
        self.edge_count += 1;
        Ok(n)
    }

    fn edge_loop(&mut self, e: &mut Entities, lp: &Loop) -> Result<u64, StepError> {
        let mut oriented = Vec::with_capacity(lp.half_edges.len());
        for he in &lp.half_edges {
            let ec = self.edge(e, he.edge)?;
            let dir = logical(he.forward);
            oriented.push(e.push(&format!("ORIENTED_EDGE('',*,*,#{ec},{dir})"))?);
        }
        e.push(&format!("EDGE_LOOP('',{})", refs(&oriented)))
    }

    /// `ADVANCED_FACE`; die äußere Schleife wird `FACE_OUTER_BOUND`, Löcher `FACE_BOUND`.
    fn face(&mut self, e: &mut Entities, id: Id<Face>) -> Result<u64, StepError> {
        let f = &self.solid.faces[id];
        let surface = geometry::surface(e, &f.surface)?;
        let mut bounds = Vec::with_capacity(1 + f.inner.len());
        let outer = self.edge_loop(e, &f.outer)?;
        bounds.push(e.push(&format!("FACE_OUTER_BOUND('',#{outer},.T.)"))?);
        for lp in &f.inner {
            let inner = self.edge_loop(e, lp)?;
            bounds.push(e.push(&format!("FACE_BOUND('',#{inner},.T.)"))?);
        }
        let sense = logical(f.same_sense);
        e.push(&format!(
            "ADVANCED_FACE('',{},#{surface},{sense})",
            refs(&bounds)
        ))
    }

    fn closed_shell(&mut self, e: &mut Entities, faces: &[Id<Face>]) -> Result<u64, StepError> {
        let mut ids = Vec::with_capacity(faces.len());
        for &f in faces {
            ids.push(self.face(e, f)?);
        }
        e.push(&format!("CLOSED_SHELL('',{})", refs(&ids)))
    }
}

/// Schreibt den Körper als `MANIFOLD_SOLID_BREP` mit dem Namen `name` (schon als
/// STEP-Zeichenkette).
pub(crate) fn write_solid(
    e: &mut Entities,
    name: &str,
    solid: &Solid,
) -> Result<WrittenSolid, StepError> {
    let shells = shell_faces(solid);
    if shells.len() != 1 {
        return Err(reject(
            "Körper mit mehreren Schalen werden noch nicht ausgegeben",
        ));
    }
    let mut w = TopoWriter::new(solid);
    let shell = w.closed_shell(e, &shells[0])?;
    let brep = e.push(&format!("MANIFOLD_SOLID_BREP({name},#{shell})"))?;
    Ok(WrittenSolid {
        items: vec![brep],
        faces: shells[0].len(),
        edges: w.edge_count,
    })
}
```

`crates/planora-step/src/product.rs`:

```rust
//! Kopf, Einheiten, Kontext und je Körper ein Teil (Spec §2, E1, E2).

use crate::entity::{Entities, refs};
use crate::error::StepError;
use crate::geometry::{direction, point};
use crate::number::push_real;
use crate::text::step_string;
use planora_math::DVec3;
use std::time::{SystemTime, UNIX_EPOCH};

/// Schema AP214 (Spec E1).
pub const SCHEMA: &str = "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }";
/// Name des Programms im Kopf.
pub const SYSTEM: &str = "PlanoraCAD";

const SECONDS_PER_DAY: u64 = 86_400;
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_MINUTE: u64 = 60;

/// Datum aus der Zahl der Tage seit 1970-01-01 (Verfahren „civil_from_days“ von Howard
/// Hinnant; die Zahlen sind die des Verfahrens: Tage bis 0000-03-01, Tage je 400 Jahre usw.).
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month as u64, day as u64)
}

/// Zeitpunkt in UTC als `YYYY-MM-DDThh:mm:ss` (Spec §2); vor 1970 gilt 1970-01-01.
pub fn utc_timestamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / SECONDS_PER_DAY).unwrap_or(i64::MAX / 2);
    let rest = secs % SECONDS_PER_DAY;
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (
        rest / SECONDS_PER_HOUR,
        rest % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
        rest % SECONDS_PER_MINUTE,
    );
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}")
}

/// Kopf bis einschließlich `DATA;`.
pub(crate) fn header(e: &mut Entities, file_name: &str, time: SystemTime) -> Result<(), StepError> {
    let name = step_string(file_name).text;
    let stamp = utc_timestamp(time);
    e.line("ISO-10303-21;")?;
    e.line("HEADER;")?;
    e.line("FILE_DESCRIPTION(('PlanoraCAD Export'),'2;1');")?;
    e.line(&format!(
        "FILE_NAME({name},'{stamp}',(''),(''),'{SYSTEM}','{SYSTEM}','');"
    ))?;
    e.line(&format!("FILE_SCHEMA(('{SCHEMA}'));"))?;
    e.line("ENDSEC;")?;
    e.line("DATA;")
}

/// `ENDSEC;` und Dateiende.
pub(crate) fn trailer(e: &mut Entities) -> Result<(), StepError> {
    e.line("ENDSEC;")?;
    e.line("END-ISO-10303-21;")?;
    e.flush()
}

/// Gemeinsame Einträge aller Teile.
pub(crate) struct Context {
    product: u64,
    definition: u64,
    geometry: u64,
}

/// Anwendungskontext, Einheiten (Millimeter, Radiant, Steradiant) und der geometrische Kontext
/// mit der Unsicherheit `uncertainty` in Millimetern.
pub(crate) fn context(e: &mut Entities, uncertainty: f64) -> Result<Context, StepError> {
    let app =
        e.push("APPLICATION_CONTEXT('core data for automotive mechanical design processes')")?;
    e.push(&format!(
        "APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{app})"
    ))?;
    let product = e.push(&format!("PRODUCT_CONTEXT('',#{app},'mechanical')"))?;
    let definition = e.push(&format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',#{app},'design')"
    ))?;
    let length = e.push("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))")?;
    let angle = e.push("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))")?;
    let solid_angle = e.push("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())")?;
    let mut u = String::from("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(");
    push_real(&mut u, uncertainty)?;
    u.push_str(&format!(
        "),#{length},'distance_accuracy_value','confusion accuracy')"
    ));
    let uncertainty = e.push(&u)?;
    let geometry = e.push(&format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{uncertainty}))\
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#{length},#{angle},#{solid_angle}))\
         REPRESENTATION_CONTEXT('3D','3D'))"
    ))?;
    Ok(Context {
        product,
        definition,
        geometry,
    })
}

/// Ein Teil (Spec E2): Produkt mit Namen, Definition und Form; `items` sind die schon
/// geschriebenen Körper (`MANIFOLD_SOLID_BREP` bzw. `BREP_WITH_VOIDS`).
pub(crate) fn product(
    e: &mut Entities,
    ctx: &Context,
    name: &str,
    items: &[u64],
) -> Result<(), StepError> {
    let o = point(e, DVec3::ZERO)?;
    let z = direction(e, DVec3::Z)?;
    let x = direction(e, DVec3::X)?;
    let origin = e.push(&format!("AXIS2_PLACEMENT_3D('',#{o},#{z},#{x})"))?;
    let mut all = Vec::with_capacity(items.len() + 1);
    all.push(origin);
    all.extend_from_slice(items);
    let shape = e.push(&format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION({name},{},#{})",
        refs(&all),
        ctx.geometry
    ))?;
    let prod = e.push(&format!("PRODUCT({name},{name},'',(#{}))", ctx.product))?;
    e.push(&format!(
        "PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#{prod}))"
    ))?;
    let formation = e.push(&format!("PRODUCT_DEFINITION_FORMATION('','',#{prod})"))?;
    let definition = e.push(&format!(
        "PRODUCT_DEFINITION('design','',#{formation},#{})",
        ctx.definition
    ))?;
    let pds = e.push(&format!("PRODUCT_DEFINITION_SHAPE('','',#{definition})"))?;
    e.push(&format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{shape})"))?;
    Ok(())
}
```

`crates/planora-step/src/lib.rs` (ganz ersetzen):

```rust
//! STEP-Export (Teilprojekt 10): exakte Körper des Kernels als STEP-Datei (AP214,
//! `AUTOMOTIVE_DESIGN`). Das Textformat ist von Hand geschrieben, ohne neue Abhängigkeit.

pub mod entity;
pub mod error;
mod geometry;
pub mod number;
mod product;
pub mod text;
mod topology;

pub use entity::{Entities, MAX_ENTITIES, WRAP_COLUMN, logical, refs};
pub use error::StepError;
pub use geometry::FRAME_TOL;
pub use number::{push_real, real};
pub use product::{SCHEMA, SYSTEM, utc_timestamp};
pub use text::{MAX_NAME_CHARS, StepText, step_string};

use planora_exchange::Progress;
use planora_kernel::{Curve, NURBS_EDGE_TOL, Solid, Surface, validate_with};
use planora_math::Tolerance;
use std::io::Write;
use std::time::SystemTime;

/// Ein Körper für die Ausgabe: Name aus dem Feature-Baum und der exakte Körper des Kernels.
#[derive(Clone, Debug)]
pub struct StepBody<'a> {
    pub name: String,
    pub solid: &'a Solid,
}

/// Einstellungen der Ausgabe.
#[derive(Clone, Debug, PartialEq)]
pub struct StepOptions {
    /// Dateiname für den Kopf (`FILE_NAME`), ohne Ordner.
    pub file_name: String,
    /// Zeitpunkt für den Kopf.
    pub time: SystemTime,
    /// Toleranz des Kernels; daraus folgt die Unsicherheit im geometrischen Kontext.
    pub tolerance: Tolerance,
}

impl StepOptions {
    /// Jetziger Zeitpunkt, Toleranz des Kernels.
    pub fn new(file_name: &str) -> StepOptions {
        StepOptions {
            file_name: file_name.to_string(),
            time: SystemTime::now(),
            tolerance: Tolerance::default(),
        }
    }
}

/// Bericht über eine geschriebene Datei (Spec §2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepReport {
    /// Zahl der geschriebenen Körper (Teile).
    pub bodies: usize,
    pub faces: usize,
    pub edges: usize,
    /// Zahl der Einträge `#n` in der Datei.
    pub entities: u64,
    /// Abgelehnte Körper, je ein [`StepError::InvalidBody`].
    pub rejected: Vec<StepError>,
    /// Namen, die auf [`MAX_NAME_CHARS`] Zeichen gekürzt wurden.
    pub shortened_names: Vec<String>,
}

/// Kennzahlen eines geprüften Körpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyStats {
    pub faces: usize,
    pub edges: usize,
    /// Zahl der Einträge, die der Körper ohne Teil-Einträge braucht.
    pub entities: u64,
}

/// Anzeigename: leere Namen werden „Körper n“ (n ab 1).
pub fn display_name(name: &str, index: usize) -> String {
    if name.trim().is_empty() {
        format!("Körper {}", index + 1)
    } else {
        name.to_string()
    }
}

/// Prüft einen Körper (Spec §4): Verweise, `validate` und ein Probelauf des Schreibens ohne
/// Ausgabe. Jede Ablehnung ist ein [`StepError::InvalidBody`] mit dem Namen des Körpers;
/// `Cancelled` und `TooManyEntities` werden durchgereicht.
pub fn check_body(body: &StepBody, progress: &Progress) -> Result<BodyStats, StepError> {
    let named = |reason: String| StepError::InvalidBody {
        name: body.name.clone(),
        reason,
    };
    if !topology::ids_in_range(body.solid) {
        return Err(named(
            "Der Körper verweist auf Teile, die es nicht gibt".into(),
        ));
    }
    validate_with(body.solid, &Tolerance::default()).map_err(|e| named(e.to_string()))?;
    let mut sink = std::io::sink();
    let mut e = Entities::probe(&mut sink, progress);
    match topology::write_solid(&mut e, "''", body.solid) {
        Ok(w) => Ok(BodyStats {
            faces: w.faces,
            edges: w.edges,
            entities: e.count(),
        }),
        Err(StepError::InvalidBody { reason, .. }) => Err(named(reason)),
        Err(StepError::NotFinite) => Err(named(StepError::NotFinite.to_string())),
        Err(other) => Err(other),
    }
}

/// Hat der Körper Freiformkurven oder -flächen?
fn has_freeform(solid: &Solid) -> bool {
    solid
        .edges
        .iter()
        .any(|(_, e)| matches!(e.curve, Curve::Nurbs(_)))
        || solid
            .faces
            .iter()
            .any(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
}

/// Unsicherheit für den geometrischen Kontext: die Längentoleranz des Kernels, bei Körpern mit
/// Freiformkurven oder -flächen mindestens [`NURBS_EDGE_TOL`] (so weit dürfen dort Kanten von
/// ihren Flächen abweichen).
fn uncertainty(solids: &[&Solid], tol: &Tolerance) -> f64 {
    if solids.iter().any(|s| has_freeform(s)) {
        tol.linear.max(NURBS_EDGE_TOL)
    } else {
        tol.linear
    }
}

/// Schreibt die Körper als STEP-Datei nach `out` (Spec §2): eine Datei, je Körper ein Teil mit
/// dem Namen aus dem Feature-Baum. Körper, die die Prüfung nicht bestehen, werden ausgelassen
/// und stehen im Bericht; bleibt keiner übrig, ist das `NothingToExport` (dann wurde nichts
/// geschrieben). `progress` zählt die Einträge und bricht ab, wenn gewünscht.
pub fn write_step(
    bodies: &[StepBody],
    opts: &StepOptions,
    out: &mut dyn Write,
    progress: &Progress,
) -> Result<StepReport, StepError> {
    let mut report = StepReport::default();
    let mut accepted: Vec<(String, &Solid, BodyStats)> = Vec::new();
    for (i, b) in bodies.iter().enumerate() {
        if progress.is_cancelled() {
            return Err(StepError::Cancelled);
        }
        let name = display_name(&b.name, i);
        let checked = StepBody {
            name: name.clone(),
            solid: b.solid,
        };
        match check_body(&checked, progress) {
            Ok(stats) => accepted.push((name, b.solid, stats)),
            Err(e @ StepError::InvalidBody { .. }) => report.rejected.push(e),
            Err(e) => return Err(e),
        }
    }
    if accepted.is_empty() {
        return Err(StepError::NothingToExport);
    }
    progress.set_total(accepted.iter().map(|a| a.2.entities).sum());
    let solids: Vec<&Solid> = accepted.iter().map(|a| a.1).collect();
    let mut e = Entities::new(out, progress);
    product::header(&mut e, &opts.file_name, opts.time)?;
    let ctx = product::context(&mut e, uncertainty(&solids, &opts.tolerance))?;
    for (name, solid, _) in &accepted {
        let text = step_string(name);
        if text.shortened {
            report.shortened_names.push(name.clone());
        }
        let w = topology::write_solid(&mut e, &text.text, solid)?;
        product::product(&mut e, &ctx, &text.text, &w.items)?;
        report.bodies += 1;
        report.faces += w.faces;
        report.edges += w.edges;
    }
    product::trailer(&mut e)?;
    report.entities = e.count();
    Ok(report)
}
```

- [ ] **Step 4: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (5 + 6 + 10 in `writer`). `check_body_counts_faces_edges_and_entities` erwartet für den Würfel genau 150 Einträge (8 Punkte und 8 Eckpunkte, 12 × 5 für Geraden und Kanten, 24 orientierte Kanten, 6 × 8 für Ebenen und Flächen, Schale, Körper) – diese Zahl gilt auch nach den späteren Tasks.

```bash
git add crates/planora-step/src/geometry.rs crates/planora-step/src/topology.rs crates/planora-step/src/product.rs crates/planora-step/src/lib.rs crates/planora-step/tests/writer.rs
git commit -m "step: Kopf, Kontext, Teile und Topologie ebener Körper, Prüfung vor dem Schreiben

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Eigener STEP-Leser (Zerlegung nach ISO 10303-21)

**Files:**
- Create: `crates/planora-step/tests/common/mod.rs`, `crates/planora-step/tests/common/reader.rs`
- Test: `crates/planora-step/tests/reader.rs`

**Interfaces:**
- Consumes: nichts (unabhängig vom Schreiber, Spec §5.1).
- Produces (`tests/common/reader.rs`, in Task 5–13 benutzt):
  ```rust
  pub enum Value { Int(i64), Real(f64), Str(String), Enum(String), Ref(u64), List(Vec<Value>),
      Typed(String, Vec<Value>), Unset, Derived }                     // Clone, Debug, PartialEq
  impl Value { pub fn real(&self) -> Result<f64, String>; pub fn int(&self) -> Result<i64, String>;
      pub fn reference(&self) -> Result<u64, String>; pub fn list(&self) -> Result<&[Value], String>;
      pub fn text(&self) -> Result<&str, String>; pub fn enumeration(&self) -> Result<&str, String>;
      pub fn logical(&self) -> Result<bool, String>; pub fn refs(&self) -> Result<Vec<u64>, String>;
      pub fn reals(&self) -> Result<Vec<f64>, String>; pub fn ints(&self) -> Result<Vec<i64>, String>; }
  pub struct Record { pub name: String, pub args: Vec<Value> }
  pub enum Entity { Simple(Record), Complex(Vec<Record>) }
  impl Entity { pub fn part(&self, name: &str) -> Option<&[Value]>; pub fn names(&self) -> Vec<&str>; }
  pub struct StepFile { pub header: Vec<Record>, pub entities: BTreeMap<u64, Entity> }
  impl StepFile { pub fn get(&self, id: u64) -> Result<&Entity, String>;
      pub fn simple(&self, id: u64, name: &str) -> Result<&[Value], String>;
      pub fn name_of(&self, id: u64) -> Result<Option<&str>, String>;
      pub fn ids_of(&self, name: &str) -> Vec<u64>; pub fn count(&self, name: &str) -> usize; }
  pub fn parse(text: &str) -> Result<StepFile, String>;
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/reader.rs`)

```rust
//! Selbsttest des eigenen STEP-Lesers: gültige Dateien, zusammengesetzte Einträge, Maskierung
//! und benannte Fehler (Teilprojekt 10, Task 4).

mod common;

use common::reader::{Entity, Value, parse};

/// Kleine gültige Datei um `data` (Einträge ohne `DATA;`/`ENDSEC;`).
fn file(data: &str) -> String {
    let head = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('x'),'2;1');\n\
                FILE_NAME('a.step','2026-10-14T00:00:00',(''),(''),'P','P','');\n\
                FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n";
    format!("{head}{data}ENDSEC;\nEND-ISO-10303-21;\n")
}

#[test]
fn a_small_file_with_all_value_kinds_is_read() {
    let text = file(concat!(
        "#1=CARTESIAN_POINT('',(0.,-1.5,2.E-3));\n",
        "#2=DIRECTION('d',\n(0.,0.,1.));\n",
        r"#3=FOO(#1,$,*,.T.,.UNSPECIFIED.,(1,2,3),LENGTH_MEASURE(1.E-6),",
        r"'O''Brien \\ \X2\00FC\X0\ \X4\0001F600\X0\');",
        "\n",
        "#4=(BOUNDED_CURVE()B_SPLINE_CURVE(2,(#1,#1,#1),.UNSPECIFIED.,.F.,.F.)\n",
        "B_SPLINE_CURVE_WITH_KNOTS((3,3),(0.,1.),.UNSPECIFIED.)CURVE()",
        "GEOMETRIC_REPRESENTATION_ITEM()\n",
        "RATIONAL_B_SPLINE_CURVE((1.,0.5,1.))REPRESENTATION_ITEM(''));\n",
    ));
    let f = parse(&text).unwrap();
    assert_eq!(f.header.len(), 3);
    assert_eq!(f.entities.len(), 4);
    let p = f.simple(1, "CARTESIAN_POINT").unwrap();
    assert_eq!(p[1].reals().unwrap(), vec![0.0, -1.5, 2e-3]);
    assert_eq!(f.simple(2, "DIRECTION").unwrap()[0], Value::Str("d".into()));
    let foo = f.simple(3, "FOO").unwrap();
    assert_eq!(foo[0], Value::Ref(1));
    assert_eq!(foo[1], Value::Unset);
    assert_eq!(foo[2], Value::Derived);
    assert!(foo[3].logical().unwrap());
    assert_eq!(foo[4].enumeration().unwrap(), "UNSPECIFIED");
    assert_eq!(foo[5].ints().unwrap(), vec![1, 2, 3]);
    assert_eq!(
        foo[6],
        Value::Typed("LENGTH_MEASURE".into(), vec![Value::Real(1e-6)])
    );
    assert_eq!(foo[7].text().unwrap(), "O'Brien \\ ü \u{1F600}");
    let e = f.get(4).unwrap();
    assert!(matches!(e, Entity::Complex(parts) if parts.len() == 7));
    assert_eq!(
        e.part("RATIONAL_B_SPLINE_CURVE").unwrap()[0]
            .reals()
            .unwrap(),
        vec![1.0, 0.5, 1.0]
    );
    assert_eq!(e.part("B_SPLINE_CURVE").unwrap()[0].int().unwrap(), 2);
    assert_eq!(f.ids_of("DIRECTION"), vec![2]);
    assert_eq!(f.count("CARTESIAN_POINT"), 1);
}

#[test]
fn integers_are_not_reals_and_reals_are_not_integers() {
    let f = parse(&file("#1=A(1,1.,-0.5);\n")).unwrap();
    let a = f.simple(1, "A").unwrap();
    assert!(a[0].real().is_err(), "1 ohne Punkt ist keine REAL-Zahl");
    assert_eq!(a[0].int().unwrap(), 1);
    assert!(a[1].int().is_err());
    assert_eq!(a[2].real().unwrap(), -0.5);
}

#[test]
fn broken_files_get_named_errors() {
    for (data, want) in [
        ("#1=A(1.e5);\n", "kleines e"),
        ("#1=A(1E5);\n", "Exponent ohne Punkt"),
        ("#1=A(.5);\n", "Aufzählung"),
        ("#1=A(#2);\n", "#2"),
        ("#1=A();\n#1=B();\n", "doppelt"),
        ("#1=(B()A());\n", "aufsteigend"),
        ("#1=(A());\n", "weniger als zwei"),
        ("#1=A('offen);\n", "Zeichenkette"),
        ("#1=A('a\nb');\n", "Steuerzeichen"),
        (r"#1=A('\X2\00F\X0\');", "Länge"),
        (r"#1=A('\S\a');", "Maskierung"),
        ("#1=a();\n", "unerwartetes Zeichen"),
        ("#1=A(1,);\n", "Wert"),
        ("#1=A(1)\n", "Semicolon"),
        ("#1=A('ä');\n", "ASCII"),
    ] {
        let err = parse(&file(data)).unwrap_err();
        assert!(err.contains(want), "{data:?}: {err}");
    }
    let good = file("#1=A();\n");
    assert!(
        parse(&format!("{good}#2=A('x"))
            .unwrap_err()
            .contains("nicht geschlossen")
    );
    assert!(parse(&good.replace("ENDSEC;\nEND", "END")).is_err());
    assert!(
        parse(&format!("{good}#2=B();"))
            .unwrap_err()
            .contains("nach dem Dateiende")
    );
    assert!(
        parse(&good.replace("FILE_NAME", "FILE_X"))
            .unwrap_err()
            .contains("FILE_NAME")
    );
}

#[test]
fn simple_checks_the_entity_name() {
    let f = parse(&file("#1=A();\n#2=(B()C());\n")).unwrap();
    assert!(f.simple(1, "B").is_err());
    assert!(f.simple(2, "B").is_err());
    assert!(f.simple(3, "A").unwrap_err().contains("#3 fehlt"));
    assert_eq!(f.name_of(1).unwrap(), Some("A"));
    assert_eq!(f.name_of(2).unwrap(), None);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test reader`
Expected: FAIL – `file not found for module common`.

- [ ] **Step 3: Implementieren**

`crates/planora-step/tests/common/mod.rs`:

```rust
//! Gemeinsame Hilfen der Tests: eigener STEP-Leser (Spec §5.1).
#![allow(dead_code)]

pub mod reader;
```

`crates/planora-step/tests/common/reader.rs`:

```rust
//! Eigener STEP-Leser für die Tests (Spec §5.1), unabhängig vom Schreiber: zerlegt eine Datei
//! nach ISO 10303-21 in Kopf und Einträge (auch zusammengesetzte) und prüft die Schreibweise
//! streng: Zahlen, Zeichenketten, Aufzählungen, Verweise, Reihenfolge der Teile.

use std::collections::BTreeMap;

/// Ein Wert in der Parameterliste eines Eintrags.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Real(f64),
    /// Zeichenkette, schon entmaskiert.
    Str(String),
    /// Aufzählung ohne Punkte, etwa `T`, `F`, `UNSPECIFIED`, `MILLI`.
    Enum(String),
    Ref(u64),
    List(Vec<Value>),
    /// Getypter Wert, etwa `LENGTH_MEASURE(1.E-6)`.
    Typed(String, Vec<Value>),
    /// `$`
    Unset,
    /// `*`
    Derived,
}

impl Value {
    pub fn real(&self) -> Result<f64, String> {
        match self {
            Value::Real(x) => Ok(*x),
            other => Err(format!("Zahl mit Punkt erwartet, gefunden {other:?}")),
        }
    }

    pub fn int(&self) -> Result<i64, String> {
        match self {
            Value::Int(x) => Ok(*x),
            other => Err(format!("ganze Zahl erwartet, gefunden {other:?}")),
        }
    }

    pub fn reference(&self) -> Result<u64, String> {
        match self {
            Value::Ref(x) => Ok(*x),
            other => Err(format!("Verweis erwartet, gefunden {other:?}")),
        }
    }

    pub fn list(&self) -> Result<&[Value], String> {
        match self {
            Value::List(x) => Ok(x),
            other => Err(format!("Liste erwartet, gefunden {other:?}")),
        }
    }

    pub fn text(&self) -> Result<&str, String> {
        match self {
            Value::Str(x) => Ok(x),
            other => Err(format!("Zeichenkette erwartet, gefunden {other:?}")),
        }
    }

    pub fn enumeration(&self) -> Result<&str, String> {
        match self {
            Value::Enum(x) => Ok(x),
            other => Err(format!("Aufzählung erwartet, gefunden {other:?}")),
        }
    }

    /// `.T.` oder `.F.`.
    pub fn logical(&self) -> Result<bool, String> {
        match self.enumeration()? {
            "T" => Ok(true),
            "F" => Ok(false),
            other => Err(format!(".T. oder .F. erwartet, gefunden .{other}.")),
        }
    }

    pub fn refs(&self) -> Result<Vec<u64>, String> {
        self.list()?.iter().map(Value::reference).collect()
    }

    pub fn reals(&self) -> Result<Vec<f64>, String> {
        self.list()?.iter().map(Value::real).collect()
    }

    pub fn ints(&self) -> Result<Vec<i64>, String> {
        self.list()?.iter().map(Value::int).collect()
    }
}

/// Name und Parameter (ein einfacher Eintrag oder ein Teil eines zusammengesetzten).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub name: String,
    pub args: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Simple(Record),
    /// Zusammengesetzter Eintrag `( A(..) B(..) … )`, Teile in aufsteigender Reihenfolge.
    Complex(Vec<Record>),
}

impl Entity {
    /// Teil `name` eines zusammengesetzten Eintrags.
    pub fn part(&self, name: &str) -> Option<&[Value]> {
        match self {
            Entity::Complex(parts) => parts
                .iter()
                .find(|r| r.name == name)
                .map(|r| r.args.as_slice()),
            Entity::Simple(_) => None,
        }
    }

    pub fn names(&self) -> Vec<&str> {
        match self {
            Entity::Simple(r) => vec![r.name.as_str()],
            Entity::Complex(parts) => parts.iter().map(|r| r.name.as_str()).collect(),
        }
    }
}

/// Gelesene Datei.
#[derive(Clone, Debug)]
pub struct StepFile {
    /// `FILE_DESCRIPTION`, `FILE_NAME`, `FILE_SCHEMA` in dieser Reihenfolge.
    pub header: Vec<Record>,
    pub entities: BTreeMap<u64, Entity>,
}

impl StepFile {
    pub fn get(&self, id: u64) -> Result<&Entity, String> {
        self.entities.get(&id).ok_or_else(|| format!("#{id} fehlt"))
    }

    /// Parameter des einfachen Eintrags `#id`, der `name` heißen muss.
    pub fn simple(&self, id: u64, name: &str) -> Result<&[Value], String> {
        match self.get(id)? {
            Entity::Simple(r) if r.name == name => Ok(&r.args),
            other => Err(format!(
                "#{id}: {name} erwartet, gefunden {:?}",
                other.names()
            )),
        }
    }

    /// Name des einfachen Eintrags `#id` (zusammengesetzte: `None`).
    pub fn name_of(&self, id: u64) -> Result<Option<&str>, String> {
        Ok(match self.get(id)? {
            Entity::Simple(r) => Some(r.name.as_str()),
            Entity::Complex(_) => None,
        })
    }

    /// Nummern aller einfachen Einträge mit diesem Namen, aufsteigend.
    pub fn ids_of(&self, name: &str) -> Vec<u64> {
        self.entities
            .iter()
            .filter(|(_, e)| matches!(e, Entity::Simple(r) if r.name == name))
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn count(&self, name: &str) -> usize {
        self.ids_of(name).len()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Keyword(String),
    Id(u64),
    Equals,
    Open,
    Close,
    Comma,
    Semicolon,
    Str(String),
    Enum(String),
    Int(i64),
    Real(f64),
    Dollar,
    Star,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_uppercase() || b == b'_'
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
}

/// Zerlegt den Text in Zeichen der Austauschdatei. Nur ASCII ist erlaubt; Leerraum (Leerzeichen,
/// Zeilenende, Tabulator) nur zwischen Zeichen.
fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let b = text.as_bytes();
    if let Some(i) = b.iter().position(|c| !c.is_ascii()) {
        return Err(format!("Zeichen außerhalb von ASCII an Stelle {i}"));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\n' | b'\r' | b'\t' => i += 1,
            b'=' => {
                out.push(Token::Equals);
                i += 1;
            }
            b'(' => {
                out.push(Token::Open);
                i += 1;
            }
            b')' => {
                out.push(Token::Close);
                i += 1;
            }
            b',' => {
                out.push(Token::Comma);
                i += 1;
            }
            b';' => {
                out.push(Token::Semicolon);
                i += 1;
            }
            b'$' => {
                out.push(Token::Dollar);
                i += 1;
            }
            b'*' => {
                out.push(Token::Star);
                i += 1;
            }
            b'#' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                let n: u64 = text[start..j]
                    .parse()
                    .map_err(|_| format!("Nummer nach # an Stelle {i}"))?;
                out.push(Token::Id(n));
                i = j;
            }
            b'\'' => {
                let (s, next) = string(b, i)?;
                out.push(Token::Str(s));
                i = next;
            }
            b'.' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len()
                    && (b[j].is_ascii_uppercase() || b[j].is_ascii_digit() || b[j] == b'_')
                {
                    j += 1;
                }
                if j == start || j >= b.len() || b[j] != b'.' || !is_name_start(b[start]) {
                    return Err(format!("Aufzählung an Stelle {i} ungültig"));
                }
                out.push(Token::Enum(text[start..j].to_string()));
                i = j + 1;
            }
            b'+' | b'-' | b'0'..=b'9' => {
                let (t, next) = number(text, i)?;
                out.push(t);
                i = next;
            }
            _ if is_name_start(c) => {
                let start = i;
                while i < b.len() && is_name_char(b[i]) {
                    i += 1;
                }
                out.push(Token::Keyword(text[start..i].to_string()));
            }
            _ => {
                return Err(format!(
                    "unerwartetes Zeichen {:?} an Stelle {i}",
                    char::from(c)
                ));
            }
        }
    }
    Ok(out)
}

/// Zahl nach ISO 10303-21: `[Vorzeichen] Ziffern [. Ziffern*] [E [Vorzeichen] Ziffern]`; mit
/// Punkt REAL, ohne Punkt INTEGER; ein Exponent ohne Punkt, ein kleines `e` oder ein Punkt ohne
/// Ziffern davor sind Fehler.
fn number(text: &str, start: usize) -> Result<(Token, usize), String> {
    let b = text.as_bytes();
    let mut i = start;
    if b[i] == b'+' || b[i] == b'-' {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return Err(format!("Zahl ohne Ziffern an Stelle {start}"));
    }
    let mut is_real = false;
    if i < b.len() && b[i] == b'.' {
        is_real = true;
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < b.len() && b[i] == b'e' {
        return Err(format!("kleines e in der Zahl an Stelle {start}"));
    }
    if i < b.len() && b[i] == b'E' {
        if !is_real {
            return Err(format!("Exponent ohne Punkt an Stelle {start}"));
        }
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e0 = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e0 {
            return Err(format!("Exponent ohne Ziffern an Stelle {start}"));
        }
    }
    let s = &text[start..i];
    let t = if is_real {
        let x: f64 = s
            .replace('E', "e")
            .parse()
            .map_err(|_| format!("Zahl {s} unlesbar"))?;
        if !x.is_finite() {
            return Err(format!("Zahl {s} nicht endlich"));
        }
        Token::Real(x)
    } else {
        Token::Int(s.parse().map_err(|_| format!("Zahl {s} unlesbar"))?)
    };
    Ok((t, i))
}

/// Hexziffern als Zeichen (je `width` Ziffern eines).
fn hex_chars(hex: &[u8], width: usize) -> Result<String, String> {
    if hex.is_empty() || !hex.len().is_multiple_of(width) {
        return Err("Länge eines \\X2\\- oder \\X4\\-Abschnitts falsch".into());
    }
    let mut out = String::new();
    for chunk in hex.chunks(width) {
        if !chunk
            .iter()
            .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(c))
        {
            return Err("ungültige Hexziffer".into());
        }
        let s = std::str::from_utf8(chunk).map_err(|e| e.to_string())?;
        let code = u32::from_str_radix(s, 16).map_err(|e| e.to_string())?;
        out.push(char::from_u32(code).ok_or_else(|| format!("kein Zeichen: {code:X}"))?);
    }
    Ok(out)
}

/// Zeichenkette ab dem öffnenden Hochkomma; liefert den entmaskierten Text und die Stelle
/// hinter dem schließenden Hochkomma. Nur druckbares ASCII; `''`, `\\`, `\X2\…\X0\` und
/// `\X4\…\X0\` werden entmaskiert, jeder andere Rückstrich ist ein Fehler.
fn string(b: &[u8], start: usize) -> Result<(String, usize), String> {
    let mut out = String::new();
    let mut i = start + 1;
    loop {
        let Some(&c) = b.get(i) else {
            return Err(format!("Zeichenkette ab Stelle {start} nicht geschlossen"));
        };
        if !(b' '..=b'~').contains(&c) {
            return Err(format!(
                "Steuerzeichen in der Zeichenkette ab Stelle {start}"
            ));
        }
        match c {
            b'\'' if b.get(i + 1) == Some(&b'\'') => {
                out.push('\'');
                i += 2;
            }
            b'\'' => return Ok((out, i + 1)),
            b'\\' => {
                let rest = &b[i..];
                if rest.starts_with(b"\\\\") {
                    out.push('\\');
                    i += 2;
                } else if rest.starts_with(b"\\X2\\") || rest.starts_with(b"\\X4\\") {
                    let width = if rest[2] == b'2' { 4 } else { 8 };
                    let body = &rest[4..];
                    let end = body
                        .windows(4)
                        .position(|w| w == b"\\X0\\")
                        .ok_or("\\X0\\ fehlt")?;
                    out.push_str(&hex_chars(&body[..end], width)?);
                    i += 4 + end + 4;
                } else {
                    return Err(format!("unbekannte Maskierung ab Stelle {i}"));
                }
            }
            _ => {
                out.push(char::from(c));
                i += 1;
            }
        }
    }
}

struct Parser {
    t: Vec<Token>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.t.get(self.i)
    }

    fn bump(&mut self) -> Result<Token, String> {
        let t = self.t.get(self.i).cloned().ok_or("Datei endet zu früh")?;
        self.i += 1;
        Ok(t)
    }

    fn expect(&mut self, want: &Token) -> Result<(), String> {
        let got = self.bump()?;
        if got == *want {
            Ok(())
        } else {
            Err(format!("{want:?} erwartet, gefunden {got:?}"))
        }
    }

    fn keyword(&mut self, word: &str) -> Result<(), String> {
        self.expect(&Token::Keyword(word.to_string()))
    }

    /// `( wert, wert, … )` (auch leer).
    fn values(&mut self) -> Result<Vec<Value>, String> {
        self.expect(&Token::Open)?;
        let mut out = Vec::new();
        if self.peek() == Some(&Token::Close) {
            self.i += 1;
            return Ok(out);
        }
        loop {
            out.push(self.value()?);
            match self.bump()? {
                Token::Comma => {}
                Token::Close => return Ok(out),
                other => return Err(format!(", oder ) erwartet, gefunden {other:?}")),
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        Ok(match self.peek().ok_or("Wert fehlt")?.clone() {
            Token::Open => Value::List(self.values()?),
            Token::Keyword(name) => {
                self.i += 1;
                Value::Typed(name, self.values()?)
            }
            other => {
                self.i += 1;
                match other {
                    Token::Int(x) => Value::Int(x),
                    Token::Real(x) => Value::Real(x),
                    Token::Str(s) => Value::Str(s),
                    Token::Enum(s) => Value::Enum(s),
                    Token::Id(n) => Value::Ref(n),
                    Token::Dollar => Value::Unset,
                    Token::Star => Value::Derived,
                    t => return Err(format!("Wert erwartet, gefunden {t:?}")),
                }
            }
        })
    }

    fn record(&mut self) -> Result<Record, String> {
        match self.bump()? {
            Token::Keyword(name) => Ok(Record {
                name,
                args: self.values()?,
            }),
            other => Err(format!("Name erwartet, gefunden {other:?}")),
        }
    }
}

/// Alle Verweise in einem Wert.
fn collect_refs(v: &Value, out: &mut Vec<u64>) {
    match v {
        Value::Ref(n) => out.push(*n),
        Value::List(items) | Value::Typed(_, items) => {
            for x in items {
                collect_refs(x, out);
            }
        }
        _ => {}
    }
}

/// Liest eine ganze Datei: Kopf mit genau `FILE_DESCRIPTION`, `FILE_NAME`, `FILE_SCHEMA`,
/// Datenabschnitt, Ende; danach nur Leerraum. Prüft: jede Nummer einmal, jeder Verweis zeigt
/// auf einen vorhandenen Eintrag, Teile zusammengesetzter Einträge aufsteigend und verschieden.
pub fn parse(text: &str) -> Result<StepFile, String> {
    let mut p = Parser {
        t: tokens(text)?,
        i: 0,
    };
    p.keyword("ISO-10303-21")?;
    p.expect(&Token::Semicolon)?;
    p.keyword("HEADER")?;
    p.expect(&Token::Semicolon)?;
    let mut header = Vec::new();
    for want in ["FILE_DESCRIPTION", "FILE_NAME", "FILE_SCHEMA"] {
        let r = p.record()?;
        if r.name != want {
            return Err(format!("Kopf: {want} erwartet, gefunden {}", r.name));
        }
        p.expect(&Token::Semicolon)?;
        header.push(r);
    }
    p.keyword("ENDSEC")?;
    p.expect(&Token::Semicolon)?;
    p.keyword("DATA")?;
    p.expect(&Token::Semicolon)?;
    let mut entities = BTreeMap::new();
    loop {
        match p.bump()? {
            Token::Keyword(k) if k == "ENDSEC" => {
                p.expect(&Token::Semicolon)?;
                break;
            }
            Token::Id(id) => {
                p.expect(&Token::Equals)?;
                let entity = if p.peek() == Some(&Token::Open) {
                    p.i += 1;
                    let mut parts = Vec::new();
                    while p.peek() != Some(&Token::Close) {
                        parts.push(p.record()?);
                    }
                    p.i += 1;
                    if parts.len() < 2 {
                        return Err(format!(
                            "#{id}: zusammengesetzt mit weniger als zwei Teilen"
                        ));
                    }
                    if parts.windows(2).any(|w| w[0].name >= w[1].name) {
                        return Err(format!("#{id}: Teile nicht aufsteigend geordnet"));
                    }
                    Entity::Complex(parts)
                } else {
                    Entity::Simple(p.record()?)
                };
                p.expect(&Token::Semicolon)?;
                if entities.insert(id, entity).is_some() {
                    return Err(format!("#{id} doppelt"));
                }
            }
            other => return Err(format!("Eintrag erwartet, gefunden {other:?}")),
        }
    }
    p.keyword("END-ISO-10303-21")?;
    p.expect(&Token::Semicolon)?;
    if p.i != p.t.len() {
        return Err("Text nach dem Dateiende".into());
    }
    for (id, e) in &entities {
        let records: Vec<&Record> = match e {
            Entity::Simple(r) => vec![r],
            Entity::Complex(parts) => parts.iter().collect(),
        };
        let mut found = Vec::new();
        for r in records {
            for v in &r.args {
                collect_refs(v, &mut found);
            }
        }
        if let Some(missing) = found.iter().find(|n| !entities.contains_key(n)) {
            return Err(format!("#{id} verweist auf #{missing}, den es nicht gibt"));
        }
    }
    Ok(StepFile { header, entities })
}
```

- [ ] **Step 4: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (4 in `reader`). Hinweis: Die Rohzeichenketten `r"…"` im Test sind nötig, weil `\X2\` in einer normalen Zeichenkette eine ungültige Maskierung wäre.

```bash
git add crates/planora-step/tests/common/mod.rs crates/planora-step/tests/common/reader.rs crates/planora-step/tests/reader.rs
git commit -m "step: eigener STEP-Leser für die Tests (Zerlegung, strenge Schreibweise)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Modell des Lesers mit eigenen Formeln

**Files:**
- Create: `crates/planora-step/tests/common/model.rs`
- Modify: `crates/planora-step/tests/common/mod.rs` (ganz ersetzen)
- Test: `crates/planora-step/tests/model.rs`

**Interfaces:**
- Consumes: `reader::{parse, StepFile, Entity, Value}` (Task 4); `planora_step::{write_step, StepBody, StepOptions, StepReport}` (Task 3).
- Produces (`tests/common/model.rs`; das Modell versteht schon alle Flächen- und Kurvenarten, auch B-Splines und Hohlräume – die Formeln werden hier mit bekannten Formen geprüft, damit spätere Tasks nur noch den Schreiber ändern):
  ```rust
  pub const DIR_TOL: f64 = 1e-9; pub const ORTHO_TOL: f64 = 1e-9; pub const NEAR_ACCEPT: f64 = 1e-9;
  pub struct Axis { pub origin: DVec3, pub x: DVec3, pub y: DVec3, pub z: DVec3 }  // local(p), radial(u)
  pub struct Spline1 { pub degree: usize, pub knots: Vec<f64>, pub control: Vec<DVec3>, pub weights: Vec<f64>, pub closed: bool }
  impl Spline1 { pub fn domain(&self) -> (f64, f64); pub fn point(&self, t: f64) -> DVec3; }
  pub struct Spline2 { pub du: usize, pub dv: usize, pub ku: Vec<f64>, pub kv: Vec<f64>,
      pub control: Vec<Vec<DVec3>>, pub weights: Vec<Vec<f64>>, pub u_closed: bool, pub v_closed: bool }
  impl Spline2 { pub fn counts(&self) -> (usize, usize); pub fn domain(&self) -> ((f64, f64), (f64, f64));
      pub fn point(&self, u: f64, v: f64) -> DVec3; pub fn derivs(&self, u: f64, v: f64) -> (DVec3, DVec3);
      pub fn closest_near(&self, p: DVec3, hint: Option<(f64, f64)>) -> (f64, f64, f64);
      pub fn closest(&self, p: DVec3) -> (f64, f64, f64); pub fn normal(&self, u: f64, v: f64) -> DVec3; }
  pub enum Crv { Line { p: DVec3, d: DVec3 }, Circle { a: Axis, r: f64 }, Ellipse { a: Axis, r1: f64, r2: f64 }, Spline(Spline1) }
  impl Crv { pub fn point(&self, t: f64) -> DVec3; pub fn tangent(&self, t: f64) -> DVec3;
      pub fn period(&self) -> Option<f64>; pub fn param(&self, q: DVec3) -> f64; pub fn distance(&self, q: DVec3) -> f64; }
  pub enum Surf { Plane(Axis), Cylinder { a: Axis, r: f64 }, Cone { a: Axis, r: f64, semi: f64 },
      Sphere { a: Axis, r: f64 }, Torus { a: Axis, major: f64, minor: f64 }, Spline(Spline2) }
  impl Surf { pub fn distance(&self, p: DVec3) -> f64;
      pub fn foot(&self, p: DVec3, hint: &mut Option<(f64, f64)>) -> (DVec3, DVec3);   // Fußpunkt, natürliche Normale
      pub fn normal_at(&self, p: DVec3) -> DVec3; pub fn project(&self, p: DVec3) -> DVec3; pub fn is_spline(&self) -> bool; }
  pub struct Oriented { pub edge: u64, pub forward: bool }
  pub struct Bound { pub outer: bool, pub orientation: bool, pub edges: Vec<Oriented> }
  pub struct FaceM { pub id: u64, pub surface: Surf, pub same_sense: bool, pub bounds: Vec<Bound> }
  pub struct ShellM { pub id: u64, pub faces: Vec<FaceM>, pub orientation: bool }  // Hohlraum: false, Flächen schon umgekehrt
  pub struct SolidM { pub name: String, pub outer: ShellM, pub voids: Vec<ShellM> }
  pub struct Product { pub name: String, pub solids: Vec<SolidM> }
  pub struct EdgeM { pub start: u64, pub end: u64, pub curve: Crv, pub same_sense: bool }
  pub struct Model { pub file: StepFile, pub uncertainty: f64, pub products: Vec<Product>,
      pub edges: BTreeMap<u64, EdgeM>, pub vertices: BTreeMap<u64, DVec3> }
  impl Model { pub fn read(text: &str) -> Result<Model, String>; pub fn point(&self, id: u64) -> Result<DVec3, String>;
      pub fn direction(&self, id: u64) -> Result<DVec3, String>; pub fn axis(&self, id: u64) -> Result<Axis, String>;
      pub fn shells(&self) -> Vec<&ShellM>; pub fn ends(&self, o: &Oriented) -> (u64, u64); }
  ```
  `tests/common/mod.rs`: `pub fn options() -> StepOptions` (Zeitpunkt fest 2026-10-14T12:34:56, Datei `probe.step`), `pub fn export(bodies: &[(&str, &Solid)]) -> (String, StepReport)`, `pub struct Lcg(pub u64)` mit `uniform()` und `range(a, b)`.

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/model.rs`)

```rust
//! Modell des eigenen Lesers: Formeln für Kurven und Flächen (mit bekannten Formen gegengeprüft)
//! und Aufbau aus einer geschriebenen Datei (Teilprojekt 10, Task 5).

mod common;

use common::export;
use common::model::*;
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DVec3, PI, TAU};

/// Schranke der Formeltests dieser Datei (Rundung bei Größen um 1–10).
const EPS: f64 = 1e-12;
/// Schranke für Ergebnisse der Näherungssuchen (Differenzenquotient, Intervallschachtelung).
const SEARCH_EPS: f64 = 1e-7;

fn world() -> Axis {
    Axis {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    }
}

/// Viertelkreis (Radius 1, xy-Ebene) als rationaler B-Spline vom Grad 2.
fn quarter_circle() -> Spline1 {
    Spline1 {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control: vec![DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y],
        weights: vec![1.0, std::f64::consts::FRAC_1_SQRT_2, 1.0],
        closed: false,
    }
}

#[test]
fn a_rational_quarter_circle_stays_on_the_circle() {
    let s = quarter_circle();
    assert_eq!(s.domain(), (0.0, 1.0));
    for i in 0..=20 {
        let p = s.point(f64::from(i) / 20.0);
        assert!((p.length() - 1.0).abs() < EPS, "{p}");
        assert!(p.z.abs() < EPS);
    }
    assert!((s.point(0.5) - DVec3::new(1.0, 1.0, 0.0).normalize()).length() < EPS);
    let c = Crv::Spline(s);
    let q = DVec3::new(0.6, 0.8, 0.0) * 1.5;
    let t = c.param(q);
    assert!((c.point(t) - DVec3::new(0.6, 0.8, 0.0)).length() < SEARCH_EPS);
    assert!((c.distance(q) - 0.5).abs() < SEARCH_EPS);
    let tan = c.tangent(0.0);
    assert!(tan.normalize().dot(DVec3::Y) > 1.0 - SEARCH_EPS, "{tan}");
}

#[test]
fn knots_with_inner_multiplicity_give_a_polyline() {
    // Grad 1 mit Knoten 0,0,1,2,2: Streckenzug durch drei Punkte.
    let s = Spline1 {
        degree: 1,
        knots: vec![0.0, 0.0, 1.0, 2.0, 2.0],
        control: vec![DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0)],
        weights: vec![1.0; 3],
        closed: false,
    };
    assert!((s.point(0.5) - DVec3::new(0.5, 0.0, 0.0)).length() < EPS);
    assert!((s.point(1.5) - DVec3::new(1.0, 0.5, 0.0)).length() < EPS);
    assert!((s.point(2.0) - DVec3::new(1.0, 1.0, 0.0)).length() < EPS);
    assert!(
        (s.point(5.0) - DVec3::new(1.0, 1.0, 0.0)).length() < EPS,
        "begrenzt"
    );
}

#[test]
fn a_rational_quarter_cylinder_surface_has_radius_one() {
    let row = |z: f64| {
        vec![
            DVec3::new(1.0, 0.0, z),
            DVec3::new(1.0, 1.0, z),
            DVec3::new(0.0, 1.0, z),
        ]
    };
    let w = std::f64::consts::FRAC_1_SQRT_2;
    // control[i][j]: i in u (Grad 1, Höhe), j in v (Grad 2, Bogen).
    let s = Spline2 {
        du: 1,
        dv: 2,
        ku: vec![0.0, 0.0, 1.0, 1.0],
        kv: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control: vec![row(0.0), row(3.0)],
        weights: vec![vec![1.0, w, 1.0], vec![1.0, w, 1.0]],
        u_closed: false,
        v_closed: false,
    };
    for (u, v) in [(0.0, 0.0), (0.3, 0.7), (1.0, 0.5), (0.5, 1.0)] {
        let p = s.point(u, v);
        assert!((p.x.hypot(p.y) - 1.0).abs() < EPS, "{p}");
        assert!((p.z - 3.0 * u).abs() < EPS);
    }
    let surf = Surf::Spline(s.clone());
    let q = DVec3::new(1.2, 1.2, 1.0);
    let foot = DVec3::new(FRAC, FRAC, 1.0);
    assert!((surf.project(q) - foot).length() < SEARCH_EPS);
    assert!((surf.distance(q) - (q - foot).length()).abs() < SEARCH_EPS);
    // S_u = +z, S_v läuft gegen den Uhrzeigersinn: S_u × S_v zeigt nach innen.
    let n = surf.normal_at(foot);
    assert!(
        n.dot(-foot.with_z(0.0).normalize()) > 1.0 - SEARCH_EPS,
        "{n}"
    );
}

/// cos 45°.
const FRAC: f64 = std::f64::consts::FRAC_1_SQRT_2;

#[test]
fn analytic_surfaces_measure_distance_and_natural_normals() {
    let a = Axis {
        origin: DVec3::new(1.0, 2.0, 3.0),
        ..world()
    };
    let p = a.origin + DVec3::new(3.0, 4.0, 2.0);
    let plane = Surf::Plane(a);
    assert!((plane.distance(p) - 2.0).abs() < EPS);
    assert_eq!(plane.normal_at(p), DVec3::Z);
    let cyl = Surf::Cylinder { a, r: 2.0 };
    assert!((cyl.distance(p) - 3.0).abs() < EPS);
    assert!((cyl.normal_at(p) - DVec3::new(0.6, 0.8, 0.0)).length() < EPS);
    let sph = Surf::Sphere { a, r: 1.0 };
    let len = 29f64.sqrt();
    assert!((sph.distance(p) - (len - 1.0)).abs() < EPS);
    let tor = Surf::Torus {
        a,
        major: 5.0,
        minor: 1.0,
    };
    assert!(
        (tor.distance(p) - 1.0).abs() < EPS,
        "Rohrmitte im Abstand 2"
    );
    assert!((tor.normal_at(p) - DVec3::Z).length() < EPS);
    // Kegel mit Radius 1 am Ursprung, 45°: Radius 1 + z.
    let cone = Surf::Cone {
        a: world(),
        r: 1.0,
        semi: PI / 4.0,
    };
    let on = DVec3::new(3.0, 0.0, 2.0);
    assert!(cone.distance(on) < EPS);
    assert!((cone.project(on + DVec3::new(1.0, 0.0, -1.0)) - on).length() < EPS);
    let n = cone.normal_at(on);
    assert!((n - DVec3::new(FRAC, 0.0, -FRAC)).length() < EPS, "{n}");
    for s in [&cyl, &sph, &tor, &cone] {
        let q = s.project(p);
        assert!(s.distance(q) < 1e-9, "{s:?}");
    }
}

#[test]
fn analytic_curves_have_points_tangents_and_parameters() {
    let a = Axis {
        origin: DVec3::new(0.0, 0.0, 1.0),
        ..world()
    };
    let c = Crv::Circle { a, r: 2.0 };
    assert!((c.point(PI / 2.0) - DVec3::new(0.0, 2.0, 1.0)).length() < EPS);
    assert!((c.tangent(0.0) - DVec3::new(0.0, 2.0, 0.0)).length() < EPS);
    assert!((c.param(DVec3::new(-5.0, 0.0, 1.0)) - PI).abs() < EPS);
    assert!((c.distance(DVec3::new(0.0, 0.0, 1.0)) - 2.0).abs() < EPS);
    assert_eq!(c.period(), Some(TAU));
    let e = Crv::Ellipse {
        a,
        r1: 3.0,
        r2: 1.0,
    };
    let q = e.point(1.0);
    assert!((e.param(q) - 1.0).abs() < EPS);
    assert!(e.distance(q) < EPS);
    let l = Crv::Line {
        p: DVec3::X,
        d: DVec3::Y * 2.0,
    };
    assert!((l.param(DVec3::new(5.0, 4.0, 0.0)) - 2.0).abs() < EPS);
    assert_eq!(l.period(), None);
}

#[test]
fn the_model_of_a_written_cube_has_one_part_and_all_counts() {
    let s = AnalyticKernel::default()
        .make_box(DVec3::new(1.0, 2.0, 3.0))
        .unwrap();
    let (text, _) = export(&[("Quader 1", &s)]);
    let m = Model::read(&text).unwrap();
    assert_eq!(m.uncertainty, 1e-6);
    assert_eq!(m.products.len(), 1);
    let p = &m.products[0];
    assert_eq!(p.name, "Quader 1");
    assert_eq!(p.solids.len(), 1);
    assert_eq!(p.solids[0].name, "Quader 1");
    assert!(p.solids[0].voids.is_empty());
    assert_eq!(p.solids[0].outer.faces.len(), 6);
    assert_eq!((m.edges.len(), m.vertices.len()), (12, 8));
    assert!(
        p.solids[0]
            .outer
            .faces
            .iter()
            .all(|f| matches!(f.surface, Surf::Plane(_)) && f.bounds.len() == 1)
    );
    assert_eq!(m.shells().len(), 1);
}

#[test]
fn the_model_refuses_wrong_units_and_skewed_axes() {
    let s = AnalyticKernel::default()
        .make_box(DVec3::splat(1.0))
        .unwrap();
    let (text, _) = export(&[("Q", &s)]);
    // Erstes Koordinatensystem mit der Achse als Bezugsrichtung (schief).
    let line = text
        .lines()
        .find(|l| l.contains("AXIS2_PLACEMENT_3D("))
        .unwrap();
    let args: Vec<&str> = line.trim_end_matches(");").split(',').collect();
    let skewed = format!("{},{},{},{});", args[0], args[1], args[2], args[2]);
    for (from, to, want) in [
        (
            "SI_UNIT(.MILLI.,.METRE.)",
            "SI_UNIT($,.METRE.)",
            "Millimetern",
        ),
        (
            "DIRECTION('',(0.,0.,1.))",
            "DIRECTION('',(0.,0.,2.))",
            "nicht normiert",
        ),
        (line, skewed.as_str(), "nicht senkrecht"),
        ("MANIFOLD_SOLID_BREP('Q'", "MANIFOLD_SOLID_BREP('X'", ""),
    ] {
        assert!(text.contains(from), "{from}");
        let broken = text.replacen(from, to, 1);
        match Model::read(&broken) {
            Err(e) => assert!(e.contains(want), "{from}: {e}"),
            Ok(m) => assert_eq!(m.products[0].solids[0].name, "X", "{from}"),
        }
    }
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test model`
Expected: FAIL – `could not find model in common`.

- [ ] **Step 3: Implementieren**

`crates/planora-step/tests/common/mod.rs` (ganz ersetzen):

```rust
//! Gemeinsame Hilfen der Tests: eigener STEP-Leser (Spec §5.1), Modell mit eigenen Formeln
//! (§5.2) und Ausgabe über `write_step`.
#![allow(dead_code)]

pub mod model;
pub mod reader;

use planora_exchange::Progress;
use planora_kernel::Solid;
use planora_step::{StepBody, StepOptions, StepReport, write_step};
use std::time::{Duration, UNIX_EPOCH};

/// Feste Einstellungen (gleiche Datei bei gleichem Inhalt): Zeitpunkt 2026-10-14T12:34:56.
pub fn options() -> StepOptions {
    StepOptions {
        time: UNIX_EPOCH + Duration::from_secs(1_791_981_296),
        ..StepOptions::new("probe.step")
    }
}

/// Schreibt die Körper mit Namen und liefert Text und Bericht.
pub fn export(bodies: &[(&str, &Solid)]) -> (String, StepReport) {
    let list: Vec<StepBody> = bodies
        .iter()
        .map(|(name, solid)| StepBody {
            name: (*name).to_string(),
            solid,
        })
        .collect();
    let mut out = Vec::new();
    let report = write_step(&list, &options(), &mut out, &Progress::new())
        .unwrap_or_else(|e| panic!("write_step: {e}"));
    (String::from_utf8(out).expect("nur ASCII"), report)
}

/// Kleiner Zufallsgenerator mit festem Startwert (keine Abhängigkeit).
pub struct Lcg(pub u64);

impl Lcg {
    pub fn uniform(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.uniform()
    }
}
```

`crates/planora-step/tests/common/model.rs`:

```rust
//! Aus der gelesenen Datei aufgebautes Modell (Spec §5.1, §5.2): Teile, Körper, Hüllen, Flächen,
//! Schleifen, Kanten, Eckpunkte sowie Kurven und Flächen mit eigenen Formeln (unabhängig vom
//! Kernel). Prüft beim Aufbau: Richtungen normiert, Koordinatensysteme senkrecht, Radien und
//! Winkel im erlaubten Bereich, Knoten und Vielfachheiten stimmig.

use super::reader::{Entity, StepFile, Value, parse};
use planora_math::{DVec3, PI, TAU};
use std::collections::BTreeMap;

/// Größte Abweichung der Länge einer Richtung von 1.
pub const DIR_TOL: f64 = 1e-9;
/// Größter Betrag des Skalarprodukts von Achse und Bezugsrichtung eines Koordinatensystems.
pub const ORTHO_TOL: f64 = 1e-9;
/// Probestellen je Kontrollpunkt bei der Suche des nächsten Kurvenpunkts.
const CURVE_SAMPLES_PER_CONTROL: usize = 16;
/// Schritte der Intervallschachtelung nach der groben Probe (Kurve).
const CURVE_REFINE_STEPS: usize = 80;
/// Rasterpunkte je Knotenabschnitt und Richtung bei der Suche des nächsten Flächenpunkts.
const SURFACE_GRID_PER_SPAN: usize = 8;
/// Mindestzahl der Rasterpunkte je Richtung.
const SURFACE_GRID_MIN: usize = 16;
/// Höchstzahl der Gauß-Newton-Schritte auf einer Freiformfläche.
const SURFACE_NEWTON_STEPS: usize = 40;
/// Schrittweite der Differenzenquotienten, bezogen auf die Breite des Parameterbereichs.
const DIFF_STEP_REL: f64 = 1e-6;
/// Newton endet, wenn der Schritt kleiner als dieser Anteil des Parameterbereichs ist.
const NEWTON_STEP_REL: f64 = 1e-14;
/// So nah (mm) muss der Punkt aus dem Startwert liegen, damit die Rastersuche entfällt.
pub const NEAR_ACCEPT: f64 = 1e-9;
/// Die Normale wird so weit (Anteil des Bereichs) vom Rand entfernt ausgewertet; an Polen ist
/// `S_u` dort null.
const NORMAL_INSET_REL: f64 = 1e-4;

/// Rechtshändiges Koordinatensystem aus `AXIS2_PLACEMENT_3D` (`y = z × x`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
    pub z: DVec3,
}

impl Axis {
    pub fn local(&self, p: DVec3) -> DVec3 {
        let d = p - self.origin;
        DVec3::new(d.dot(self.x), d.dot(self.y), d.dot(self.z))
    }

    /// Einheitsvektor in der xy-Ebene unter dem Winkel `u`.
    pub fn radial(&self, u: f64) -> DVec3 {
        self.x * u.cos() + self.y * u.sin()
    }
}

/// Knotenvektor mit Vielfachheiten ausgeschrieben.
fn expand_knots(mults: &[i64], values: &[f64]) -> Result<Vec<f64>, String> {
    if mults.len() != values.len() || mults.is_empty() {
        return Err("Vielfachheiten und Knotenwerte passen nicht zusammen".into());
    }
    if values.windows(2).any(|w| w[1] <= w[0]) {
        return Err("Knotenwerte nicht streng steigend".into());
    }
    let mut out = Vec::new();
    for (m, v) in mults.iter().zip(values) {
        if *m < 1 {
            return Err("Vielfachheit unter 1".into());
        }
        out.extend(std::iter::repeat_n(*v, *m as usize));
    }
    Ok(out)
}

/// Prüft Grad, Zahl der Kontrollpunkte und Randvielfachheiten (höchstens Grad + 1).
fn check_knots(degree: usize, count: usize, mults: &[i64], knots: &[f64]) -> Result<(), String> {
    if degree < 1 {
        return Err("Grad unter 1".into());
    }
    if knots.len() != count + degree + 1 {
        return Err(format!(
            "{} Knoten, erwartet {} (Kontrollpunkte + Grad + 1)",
            knots.len(),
            count + degree + 1
        ));
    }
    let edge_ok = |m: i64| m <= degree as i64 + 1;
    if !edge_ok(mults[0]) || !edge_ok(mults[mults.len() - 1]) {
        return Err("Randknoten öfter als Grad + 1".into());
    }
    if mults[1..mults.len() - 1].iter().any(|m| *m > degree as i64) {
        return Err("innerer Knoten öfter als der Grad".into());
    }
    Ok(())
}

/// Index des Knotenabschnitts mit `knots[i] <= u < knots[i + 1]`; am rechten Rand der letzte
/// nichtleere Abschnitt.
fn find_span(knots: &[f64], degree: usize, count: usize, u: f64) -> usize {
    if u >= knots[count] {
        let mut i = count - 1;
        while knots[i] >= knots[i + 1] {
            i -= 1;
        }
        return i;
    }
    let mut i = degree;
    while u >= knots[i + 1] {
        i += 1;
    }
    i
}

/// Werte `N_{span−p+k, p}(u)`, `k = 0..=p`, nach der Rekursion von Cox und de Boor.
fn basis(knots: &[f64], degree: usize, span: usize, u: f64) -> Vec<f64> {
    let p = degree;
    let mut prev = vec![0.0; p + 2];
    prev[p] = 1.0;
    for d in 1..=p {
        let mut cur = vec![0.0; p + 2];
        for k in (p - d)..=p {
            let i = span - p + k;
            let left = if knots[i + d] > knots[i] {
                (u - knots[i]) / (knots[i + d] - knots[i]) * prev[k]
            } else {
                0.0
            };
            let right = if knots[i + d + 1] > knots[i + 1] {
                (knots[i + d + 1] - u) / (knots[i + d + 1] - knots[i + 1]) * prev[k + 1]
            } else {
                0.0
            };
            cur[k] = left + right;
        }
        prev = cur;
    }
    prev.truncate(p + 1);
    prev
}

/// B-Spline-Kurve aus der Datei (Knoten ausgeschrieben).
#[derive(Clone, Debug, PartialEq)]
pub struct Spline1 {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub control: Vec<DVec3>,
    pub weights: Vec<f64>,
    pub closed: bool,
}

impl Spline1 {
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.control.len()])
    }

    /// Punkt bei `t`; geschlossene Kurven falten `t` in den Bereich, offene begrenzen.
    pub fn point(&self, t: f64) -> DVec3 {
        let (a, b) = self.domain();
        let t = if self.closed && (t < a || t > b) {
            a + (t - a).rem_euclid(b - a)
        } else {
            t
        };
        let t = t.clamp(a, b);
        let span = find_span(&self.knots, self.degree, self.control.len(), t);
        let n = basis(&self.knots, self.degree, span, t);
        let (mut sum, mut w) = (DVec3::ZERO, 0.0);
        for (k, nk) in n.iter().enumerate() {
            let i = span - self.degree + k;
            sum += self.control[i] * (nk * self.weights[i]);
            w += nk * self.weights[i];
        }
        sum / w
    }
}

/// B-Spline-Fläche aus der Datei; `control[i][j]`, `i` in u-, `j` in v-Richtung.
#[derive(Clone, Debug, PartialEq)]
pub struct Spline2 {
    pub du: usize,
    pub dv: usize,
    pub ku: Vec<f64>,
    pub kv: Vec<f64>,
    pub control: Vec<Vec<DVec3>>,
    pub weights: Vec<Vec<f64>>,
    pub u_closed: bool,
    pub v_closed: bool,
}

impl Spline2 {
    pub fn counts(&self) -> (usize, usize) {
        (self.control.len(), self.control[0].len())
    }

    pub fn domain(&self) -> ((f64, f64), (f64, f64)) {
        let (nu, nv) = self.counts();
        (
            (self.ku[self.du], self.ku[nu]),
            (self.kv[self.dv], self.kv[nv]),
        )
    }

    pub fn point(&self, u: f64, v: f64) -> DVec3 {
        let (nu, nv) = self.counts();
        let ((u0, u1), (v0, v1)) = self.domain();
        let (u, v) = (u.clamp(u0, u1), v.clamp(v0, v1));
        let su = find_span(&self.ku, self.du, nu, u);
        let sv = find_span(&self.kv, self.dv, nv, v);
        let bu = basis(&self.ku, self.du, su, u);
        let bv = basis(&self.kv, self.dv, sv, v);
        let (mut sum, mut w) = (DVec3::ZERO, 0.0);
        for (a, nu_a) in bu.iter().enumerate() {
            let i = su - self.du + a;
            for (b, nv_b) in bv.iter().enumerate() {
                let j = sv - self.dv + b;
                let f = nu_a * nv_b * self.weights[i][j];
                sum += self.control[i][j] * f;
                w += f;
            }
        }
        sum / w
    }

    /// Ableitungen nach u und v über zentrale Differenzen (am Rand einseitig).
    pub fn derivs(&self, u: f64, v: f64) -> (DVec3, DVec3) {
        let ((u0, u1), (v0, v1)) = self.domain();
        let (hu, hv) = (DIFF_STEP_REL * (u1 - u0), DIFF_STEP_REL * (v1 - v0));
        let (ua, ub) = ((u - hu).max(u0), (u + hu).min(u1));
        let (va, vb) = ((v - hv).max(v0), (v + hv).min(v1));
        let su = (self.point(ub, v) - self.point(ua, v)) / (ub - ua);
        let sv = (self.point(u, vb) - self.point(u, va)) / (vb - va);
        (su, sv)
    }

    /// Geschlossene Richtungen über die Naht falten, offene begrenzen.
    fn fold(&self, u: f64, v: f64) -> (f64, f64) {
        let ((u0, u1), (v0, v1)) = self.domain();
        let f = |x: f64, a: f64, b: f64, closed: bool| {
            if closed {
                a + (x - a).rem_euclid(b - a)
            } else {
                x.clamp(a, b)
            }
        };
        (f(u, u0, u1, self.u_closed), f(v, v0, v1, self.v_closed))
    }

    /// Mustersuche ohne Ableitungen von `(u, v)` aus mit den Anfangsschritten `(hu, hv)`
    /// (für Stellen, an denen Newton nicht greift, etwa nahe an Polen).
    fn compass(&self, p: DVec3, start: (f64, f64, f64), hu: f64, hv: f64) -> (f64, f64, f64) {
        let ((u0, u1), (v0, v1)) = self.domain();
        let mut best = start;
        let (mut hu, mut hv) = (hu, hv);
        while hu > NEWTON_STEP_REL * (u1 - u0) || hv > NEWTON_STEP_REL * (v1 - v0) {
            let mut moved = false;
            for (du, dv) in [(hu, 0.0), (-hu, 0.0), (0.0, hv), (0.0, -hv)] {
                let (u, v) = self.fold(best.0 + du, best.1 + dv);
                let d = self.point(u, v).distance(p);
                if d < best.2 {
                    best = (u, v, d);
                    moved = true;
                }
            }
            if !moved {
                hu *= 0.5;
                hv *= 0.5;
            }
        }
        best
    }

    /// Gauß-Newton auf `|S(u, v) − p|` von `(u, v)` aus (offene Richtungen begrenzt, geschlossene
    /// über die Naht gefaltet); liefert den besten gesehenen Punkt `(u, v, Abstand)`.
    fn newton(&self, p: DVec3, u: f64, v: f64) -> (f64, f64, f64) {
        let ((u0, u1), (v0, v1)) = self.domain();
        let (mut u, mut v) = (u, v);
        let mut best = (u, v, self.point(u, v).distance(p));
        for _ in 0..SURFACE_NEWTON_STEPS {
            let r = self.point(u, v) - p;
            let (su, sv) = self.derivs(u, v);
            let (a, b, c) = (su.dot(su), su.dot(sv), sv.dot(sv));
            let det = a * c - b * b;
            if det.abs() <= f64::MIN_POSITIVE {
                break;
            }
            let (gu, gv) = (su.dot(r), sv.dot(r));
            let du = (c * gu - b * gv) / det;
            let dv = (a * gv - b * gu) / det;
            (u, v) = self.fold(u - du, v - dv);
            let d = self.point(u, v).distance(p);
            if d < best.2 {
                best = (u, v, d);
            }
            if du.abs() <= NEWTON_STEP_REL * (u1 - u0) && dv.abs() <= NEWTON_STEP_REL * (v1 - v0) {
                break;
            }
        }
        best
    }

    /// Nächster Flächenpunkt `(u, v, Abstand)`: Newton vom Startwert `hint`, wenn der dabei
    /// gefundene Punkt höchstens [`NEAR_ACCEPT`] entfernt ist; sonst Raster, dann Newton.
    pub fn closest_near(&self, p: DVec3, hint: Option<(f64, f64)>) -> (f64, f64, f64) {
        if let Some((u, v)) = hint {
            let r = self.newton(p, u, v);
            if r.2 <= NEAR_ACCEPT {
                return r;
            }
        }
        let ((u0, u1), (v0, v1)) = self.domain();
        let (nu, nv) = self.counts();
        let gu = (SURFACE_GRID_PER_SPAN * (nu - self.du)).max(SURFACE_GRID_MIN);
        let gv = (SURFACE_GRID_PER_SPAN * (nv - self.dv)).max(SURFACE_GRID_MIN);
        let mut best = (u0, v0, f64::INFINITY);
        for a in 0..=gu {
            let u = u0 + (u1 - u0) * a as f64 / gu as f64;
            for b in 0..=gv {
                let v = v0 + (v1 - v0) * b as f64 / gv as f64;
                let d = self.point(u, v).distance(p);
                if d < best.2 {
                    best = (u, v, d);
                }
            }
        }
        let r = self.newton(p, best.0, best.1);
        if r.2 <= NEAR_ACCEPT {
            return r;
        }
        self.compass(p, r, (u1 - u0) / gu as f64, (v1 - v0) / gv as f64)
    }

    pub fn closest(&self, p: DVec3) -> (f64, f64, f64) {
        self.closest_near(p, None)
    }

    /// Natürliche Einheitsnormale `S_u × S_v` bei `(u, v)`; am Rand (Pole liegen dort) etwas
    /// nach innen versetzt ausgewertet.
    pub fn normal(&self, u: f64, v: f64) -> DVec3 {
        let ((u0, u1), (v0, v1)) = self.domain();
        let (iu, iv) = (NORMAL_INSET_REL * (u1 - u0), NORMAL_INSET_REL * (v1 - v0));
        let (su, sv) = self.derivs(u.clamp(u0 + iu, u1 - iu), v.clamp(v0 + iv, v1 - iv));
        su.cross(sv).normalize_or_zero()
    }
}

/// Kurve einer Kante.
#[derive(Clone, Debug, PartialEq)]
pub enum Crv {
    /// `p + t · d` (`d` = Richtung mal Betrag des `VECTOR`).
    Line {
        p: DVec3,
        d: DVec3,
    },
    Circle {
        a: Axis,
        r: f64,
    },
    Ellipse {
        a: Axis,
        r1: f64,
        r2: f64,
    },
    Spline(Spline1),
}

impl Crv {
    pub fn point(&self, t: f64) -> DVec3 {
        match self {
            Crv::Line { p, d } => *p + *d * t,
            Crv::Circle { a, r } => a.origin + a.radial(t) * *r,
            Crv::Ellipse { a, r1, r2 } => a.origin + a.x * (r1 * t.cos()) + a.y * (r2 * t.sin()),
            Crv::Spline(s) => s.point(t),
        }
    }

    /// Ableitung nach dem Parameter (Freiform: Differenzenquotient).
    pub fn tangent(&self, t: f64) -> DVec3 {
        match self {
            Crv::Line { d, .. } => *d,
            Crv::Circle { a, r } => (a.y * t.cos() - a.x * t.sin()) * *r,
            Crv::Ellipse { a, r1, r2 } => a.y * (r2 * t.cos()) - a.x * (r1 * t.sin()),
            Crv::Spline(s) => {
                let (lo, hi) = s.domain();
                let t = if s.closed {
                    lo + (t - lo).rem_euclid(hi - lo)
                } else {
                    t.clamp(lo, hi)
                };
                let h = DIFF_STEP_REL * (hi - lo);
                let (ta, tb) = ((t - h).max(lo), (t + h).min(hi));
                (s.point(tb) - s.point(ta)) / (tb - ta)
            }
        }
    }

    /// Periode: Kreis und Ellipse `TAU`, geschlossene B-Splines die Breite des Bereichs.
    pub fn period(&self) -> Option<f64> {
        match self {
            Crv::Line { .. } => None,
            Crv::Circle { .. } | Crv::Ellipse { .. } => Some(TAU),
            Crv::Spline(s) => s.closed.then(|| {
                let (a, b) = s.domain();
                b - a
            }),
        }
    }

    /// Parameter des nächsten Kurvenpunkts.
    pub fn param(&self, q: DVec3) -> f64 {
        match self {
            Crv::Line { p, d } => (q - *p).dot(*d) / d.length_squared(),
            Crv::Circle { a, .. } => {
                let l = a.local(q);
                l.y.atan2(l.x)
            }
            Crv::Ellipse { a, r1, r2 } => {
                let l = a.local(q);
                (l.y / r2).atan2(l.x / r1)
            }
            Crv::Spline(s) => {
                let (lo, hi) = s.domain();
                let n = CURVE_SAMPLES_PER_CONTROL * s.control.len();
                let at = |i: usize| lo + (hi - lo) * i as f64 / n as f64;
                let best = (0..=n)
                    .min_by(|x, y| {
                        s.point(at(*x))
                            .distance(q)
                            .total_cmp(&s.point(at(*y)).distance(q))
                    })
                    .unwrap_or(0);
                let (mut a, mut b) = (at(best.saturating_sub(1)), at((best + 1).min(n)));
                for _ in 0..CURVE_REFINE_STEPS {
                    let m1 = a + (b - a) / 3.0;
                    let m2 = b - (b - a) / 3.0;
                    if s.point(m1).distance(q) < s.point(m2).distance(q) {
                        b = m2;
                    } else {
                        a = m1;
                    }
                }
                0.5 * (a + b)
            }
        }
    }

    /// Abstand zur Kurve (Ellipse: über den Winkel; für Punkte auf der Kurve genau).
    pub fn distance(&self, q: DVec3) -> f64 {
        match self {
            Crv::Circle { a, r } => {
                let l = a.local(q);
                ((l.x.hypot(l.y) - r).powi(2) + l.z * l.z).sqrt()
            }
            _ => self.point(self.param(q)).distance(q),
        }
    }
}

/// Trägerfläche einer Fläche.
#[derive(Clone, Debug, PartialEq)]
pub enum Surf {
    Plane(Axis),
    Cylinder {
        a: Axis,
        r: f64,
    },
    /// Radius `r` in der Ebene des Ursprungs, halber Öffnungswinkel `semi`.
    Cone {
        a: Axis,
        r: f64,
        semi: f64,
    },
    Sphere {
        a: Axis,
        r: f64,
    },
    Torus {
        a: Axis,
        major: f64,
        minor: f64,
    },
    Spline(Spline2),
}

impl Surf {
    /// Abstand zur Fläche (Freiform: über den nächsten Punkt).
    pub fn distance(&self, p: DVec3) -> f64 {
        match self {
            Surf::Plane(a) => a.local(p).z.abs(),
            Surf::Cylinder { a, r } => {
                let l = a.local(p);
                (l.x.hypot(l.y) - r).abs()
            }
            Surf::Cone { a, r, semi } => {
                let l = a.local(p);
                ((l.x.hypot(l.y) - (r + l.z * semi.tan())) * semi.cos()).abs()
            }
            Surf::Sphere { a, r } => ((p - a.origin).length() - r).abs(),
            Surf::Torus { a, major, minor } => {
                let l = a.local(p);
                ((l.x.hypot(l.y) - major).hypot(l.z) - minor).abs()
            }
            Surf::Spline(s) => s.closest(p).2,
        }
    }

    /// Fußpunkt (nächster Flächenpunkt) und natürliche Einheitsnormale dort (an Polen und
    /// Spitzen kann sie null sein). `hint` ist bei Freiformflächen Startwert und Ergebnis der
    /// Parametersuche (für benachbarte Punkte); bei analytischen Flächen bleibt er unberührt.
    pub fn foot(&self, p: DVec3, hint: &mut Option<(f64, f64)>) -> (DVec3, DVec3) {
        let radial = |a: &Axis, l: DVec3| (a.x * l.x + a.y * l.y).normalize_or_zero();
        match self {
            Surf::Plane(a) => (p - a.z * a.local(p).z, a.z),
            Surf::Cylinder { a, r } => {
                let l = a.local(p);
                let n = radial(a, l);
                (a.origin + n * *r + a.z * l.z, n)
            }
            Surf::Cone { a, r, semi } => {
                // Fußpunkt auf der Mantellinie in der Meridianebene; an der Spitze (auf der
                // Achse) keine Normale.
                let l = a.local(p);
                let dir = radial(a, l);
                let rho = l.x.hypot(l.y);
                let (s, c) = semi.sin_cos();
                let d = (rho - (r + l.z * s / c)) * c;
                let foot = a.origin + dir * (rho - d * c) + a.z * (l.z + d * s);
                let n = if dir == DVec3::ZERO {
                    DVec3::ZERO
                } else {
                    dir * c - a.z * s
                };
                (foot, n)
            }
            Surf::Sphere { a, r } => {
                let n = (p - a.origin).normalize_or_zero();
                (a.origin + n * *r, n)
            }
            Surf::Torus { a, major, minor } => {
                let center = a.origin + radial(a, a.local(p)) * *major;
                let n = (p - center).normalize_or_zero();
                (center + n * *minor, n)
            }
            Surf::Spline(s) => {
                let (u, v, _) = s.closest_near(p, *hint);
                *hint = Some((u, v));
                (s.point(u, v), s.normal(u, v))
            }
        }
    }

    /// Natürliche Einheitsnormale am nächsten Flächenpunkt.
    pub fn normal_at(&self, p: DVec3) -> DVec3 {
        self.foot(p, &mut None).1
    }

    /// Nächster Flächenpunkt.
    pub fn project(&self, p: DVec3) -> DVec3 {
        self.foot(p, &mut None).0
    }

    pub fn is_spline(&self) -> bool {
        matches!(self, Surf::Spline(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Oriented {
    /// Nummer des `EDGE_CURVE`.
    pub edge: u64,
    pub forward: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bound {
    pub outer: bool,
    pub orientation: bool,
    pub edges: Vec<Oriented>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceM {
    pub id: u64,
    pub surface: Surf,
    pub same_sense: bool,
    pub bounds: Vec<Bound>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShellM {
    pub id: u64,
    pub faces: Vec<FaceM>,
    /// `.F.` bei einem Hohlraum (`ORIENTED_CLOSED_SHELL`), sonst `.T.`.
    pub orientation: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SolidM {
    pub name: String,
    pub outer: ShellM,
    pub voids: Vec<ShellM>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    pub name: String,
    pub solids: Vec<SolidM>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeM {
    /// Nummern der `VERTEX_POINT`.
    pub start: u64,
    pub end: u64,
    pub curve: Crv,
    pub same_sense: bool,
}

/// Ganze Datei als Modell.
#[derive(Clone, Debug)]
pub struct Model {
    pub file: StepFile,
    /// Unsicherheit aus dem geometrischen Kontext (Millimeter).
    pub uncertainty: f64,
    pub products: Vec<Product>,
    pub edges: BTreeMap<u64, EdgeM>,
    pub vertices: BTreeMap<u64, DVec3>,
}

impl Model {
    /// Liest den Text und baut das Modell; jeder Fehler nennt den Eintrag.
    pub fn read(text: &str) -> Result<Model, String> {
        let file = parse(text)?;
        let mut m = Model {
            file,
            uncertainty: 0.0,
            products: Vec::new(),
            edges: BTreeMap::new(),
            vertices: BTreeMap::new(),
        };
        m.check_header()?;
        let context = m.context()?;
        for sdr in m.file.ids_of("SHAPE_DEFINITION_REPRESENTATION") {
            let product = m.product(sdr, context)?;
            m.products.push(product);
        }
        Ok(m)
    }

    fn check_header(&self) -> Result<(), String> {
        let schema = self.file.header[2].args[0].list()?;
        if schema.len() != 1 || !schema[0].text()?.starts_with("AUTOMOTIVE_DESIGN") {
            return Err(format!("Schema {schema:?}"));
        }
        Ok(())
    }

    /// Geometrischer Kontext: Millimeter, Radiant, Unsicherheit.
    fn context(&mut self) -> Result<u64, String> {
        let ids: Vec<u64> = self
            .file
            .entities
            .iter()
            .filter(|(_, e)| e.part("GEOMETRIC_REPRESENTATION_CONTEXT").is_some())
            .map(|(id, _)| *id)
            .collect();
        let [id] = ids[..] else {
            return Err(format!("{} geometrische Kontexte", ids.len()));
        };
        let e = self.file.get(id)?.clone();
        if e.part("GEOMETRIC_REPRESENTATION_CONTEXT").unwrap()[0].int()? != 3 {
            return Err("Kontext nicht dreidimensional".into());
        }
        let units = e
            .part("GLOBAL_UNIT_ASSIGNED_CONTEXT")
            .ok_or("Einheiten fehlen")?[0]
            .refs()?;
        let mut length = None;
        let mut angle = false;
        for u in units {
            let unit = self.file.get(u)?;
            let si = unit.part("SI_UNIT").ok_or("keine SI-Einheit")?;
            if unit.part("LENGTH_UNIT").is_some() {
                let milli = Value::Enum("MILLI".into());
                if si[0] != milli || si[1] != Value::Enum("METRE".into()) {
                    return Err("Länge nicht in Millimetern".into());
                }
                length = Some(u);
            }
            if unit.part("PLANE_ANGLE_UNIT").is_some() {
                if si[0] != Value::Unset || si[1] != Value::Enum("RADIAN".into()) {
                    return Err("Winkel nicht in Radiant".into());
                }
                angle = true;
            }
        }
        let length = length.ok_or("keine Längeneinheit")?;
        if !angle {
            return Err("keine Winkeleinheit".into());
        }
        let unc = e
            .part("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT")
            .ok_or("Unsicherheit fehlt")?[0]
            .refs()?;
        let [u] = unc[..] else {
            return Err("genau eine Unsicherheit erwartet".into());
        };
        let a = self.file.simple(u, "UNCERTAINTY_MEASURE_WITH_UNIT")?;
        match &a[0] {
            Value::Typed(name, v) if name == "LENGTH_MEASURE" => self.uncertainty = v[0].real()?,
            other => return Err(format!("Unsicherheit {other:?}")),
        }
        if a[1].reference()? != length {
            return Err("Unsicherheit nicht in der Längeneinheit".into());
        }
        if self.file.get(id)?.part("REPRESENTATION_CONTEXT").is_none() {
            return Err("REPRESENTATION_CONTEXT fehlt".into());
        }
        Ok(id)
    }

    /// Teil aus `SHAPE_DEFINITION_REPRESENTATION` über Definition bis zum Produkt.
    fn product(&mut self, sdr: u64, context: u64) -> Result<Product, String> {
        let a = self
            .file
            .simple(sdr, "SHAPE_DEFINITION_REPRESENTATION")?
            .to_vec();
        let pds = self
            .file
            .simple(a[0].reference()?, "PRODUCT_DEFINITION_SHAPE")?;
        let pd = self
            .file
            .simple(pds[2].reference()?, "PRODUCT_DEFINITION")?;
        let pdf = self
            .file
            .simple(pd[2].reference()?, "PRODUCT_DEFINITION_FORMATION")?;
        let prod = self.file.simple(pdf[2].reference()?, "PRODUCT")?;
        let name = prod[0].text()?.to_string();
        if prod[1].text()? != name {
            return Err("Kennung und Name des Produkts verschieden".into());
        }
        let shape = self
            .file
            .simple(a[1].reference()?, "ADVANCED_BREP_SHAPE_REPRESENTATION")?
            .to_vec();
        if shape[0].text()? != name {
            return Err("Name der Darstellung weicht ab".into());
        }
        if shape[2].reference()? != context {
            return Err("Darstellung ohne den geometrischen Kontext".into());
        }
        let mut solids = Vec::new();
        let mut placements = 0;
        for item in shape[1].refs()? {
            match self.file.name_of(item)? {
                Some("AXIS2_PLACEMENT_3D") => {
                    let ax = self.axis(item)?;
                    if ax.origin != DVec3::ZERO || ax.z != DVec3::Z || ax.x != DVec3::X {
                        return Err("Lage des Teils nicht im Ursprung".into());
                    }
                    placements += 1;
                }
                Some("MANIFOLD_SOLID_BREP") => {
                    let b = self.file.simple(item, "MANIFOLD_SOLID_BREP")?.to_vec();
                    solids.push(SolidM {
                        name: b[0].text()?.to_string(),
                        outer: self.shell(b[1].reference()?, true)?,
                        voids: Vec::new(),
                    });
                }
                Some("BREP_WITH_VOIDS") => {
                    let b = self.file.simple(item, "BREP_WITH_VOIDS")?.to_vec();
                    let mut voids = Vec::new();
                    for v in b[2].refs()? {
                        let o = self.file.simple(v, "ORIENTED_CLOSED_SHELL")?.to_vec();
                        if o[1] != Value::Derived {
                            return Err("ORIENTED_CLOSED_SHELL ohne *".into());
                        }
                        let orientation = o[3].logical()?;
                        if orientation {
                            return Err("Hohlraum ohne Umkehrung (.F.)".into());
                        }
                        // `.F.` kehrt alle Flächen um: hier gleich angewendet, damit die Flächen
                        // des Modells wie alle anderen nach außen (ins Hohlrauminnere) zeigen.
                        let mut shell = self.shell(o[2].reference()?, false)?;
                        for f in &mut shell.faces {
                            f.same_sense = !f.same_sense;
                            for b in &mut f.bounds {
                                b.orientation = !b.orientation;
                            }
                        }
                        voids.push(shell);
                    }
                    if voids.is_empty() {
                        return Err("BREP_WITH_VOIDS ohne Hohlraum".into());
                    }
                    solids.push(SolidM {
                        name: b[0].text()?.to_string(),
                        outer: self.shell(b[1].reference()?, true)?,
                        voids,
                    });
                }
                other => return Err(format!("unerwartetes Element {other:?}")),
            }
        }
        if placements != 1 || solids.is_empty() {
            return Err(format!("{placements} Lagen, {} Körper", solids.len()));
        }
        Ok(Product { name, solids })
    }

    fn shell(&mut self, id: u64, orientation: bool) -> Result<ShellM, String> {
        let a = self.file.simple(id, "CLOSED_SHELL")?.to_vec();
        let mut faces = Vec::new();
        for f in a[1].refs()? {
            faces.push(self.face(f)?);
        }
        Ok(ShellM {
            id,
            faces,
            orientation,
        })
    }

    fn face(&mut self, id: u64) -> Result<FaceM, String> {
        let a = self.file.simple(id, "ADVANCED_FACE")?.to_vec();
        let mut bounds = Vec::new();
        for b in a[1].refs()? {
            let (outer, args) = match self.file.name_of(b)? {
                Some("FACE_OUTER_BOUND") => (true, self.file.simple(b, "FACE_OUTER_BOUND")?),
                Some("FACE_BOUND") => (false, self.file.simple(b, "FACE_BOUND")?),
                other => return Err(format!("#{b}: Rand erwartet, gefunden {other:?}")),
            };
            let (lp, orientation) = (args[1].reference()?, args[2].logical()?);
            let l = self.file.simple(lp, "EDGE_LOOP")?.to_vec();
            let mut edges = Vec::new();
            for oe in l[1].refs()? {
                let o = self.file.simple(oe, "ORIENTED_EDGE")?.to_vec();
                if o[1] != Value::Derived || o[2] != Value::Derived {
                    return Err(format!("#{oe}: Anfang und Ende müssen * sein"));
                }
                let edge = o[3].reference()?;
                self.edge(edge)?;
                edges.push(Oriented {
                    edge,
                    forward: o[4].logical()?,
                });
            }
            if edges.is_empty() {
                return Err(format!("#{lp}: leere Schleife"));
            }
            bounds.push(Bound {
                outer,
                orientation,
                edges,
            });
        }
        if bounds.iter().filter(|b| b.outer).count() != 1 {
            return Err(format!("#{id}: genau ein äußerer Rand erwartet"));
        }
        Ok(FaceM {
            id,
            surface: self.surface(a[2].reference()?)?,
            same_sense: a[3].logical()?,
            bounds,
        })
    }

    fn edge(&mut self, id: u64) -> Result<(), String> {
        if self.edges.contains_key(&id) {
            return Ok(());
        }
        let a = self.file.simple(id, "EDGE_CURVE")?.to_vec();
        let start = self.vertex(a[1].reference()?)?;
        let end = self.vertex(a[2].reference()?)?;
        let curve = self.curve(a[3].reference()?)?;
        self.edges.insert(
            id,
            EdgeM {
                start,
                end,
                curve,
                same_sense: a[4].logical()?,
            },
        );
        Ok(())
    }

    fn vertex(&mut self, id: u64) -> Result<u64, String> {
        let a = self.file.simple(id, "VERTEX_POINT")?;
        let p = self.point(a[1].reference()?)?;
        self.vertices.insert(id, p);
        Ok(id)
    }

    pub fn point(&self, id: u64) -> Result<DVec3, String> {
        let a = self.file.simple(id, "CARTESIAN_POINT")?;
        let c = a[1].reals()?;
        let [x, y, z] = c[..] else {
            return Err(format!("#{id}: drei Koordinaten erwartet"));
        };
        Ok(DVec3::new(x, y, z))
    }

    /// Richtung, normiert bis [`DIR_TOL`].
    pub fn direction(&self, id: u64) -> Result<DVec3, String> {
        let a = self.file.simple(id, "DIRECTION")?;
        let c = a[1].reals()?;
        let [x, y, z] = c[..] else {
            return Err(format!("#{id}: drei Komponenten erwartet"));
        };
        let d = DVec3::new(x, y, z);
        if (d.length() - 1.0).abs() > DIR_TOL {
            return Err(format!("#{id}: Richtung nicht normiert ({})", d.length()));
        }
        Ok(d)
    }

    /// Koordinatensystem; Achse und Bezugsrichtung senkrecht bis [`ORTHO_TOL`].
    pub fn axis(&self, id: u64) -> Result<Axis, String> {
        let a = self.file.simple(id, "AXIS2_PLACEMENT_3D")?;
        let origin = self.point(a[1].reference()?)?;
        let z = self.direction(a[2].reference()?)?;
        let x = self.direction(a[3].reference()?)?;
        if z.dot(x).abs() > ORTHO_TOL {
            return Err(format!("#{id}: Achse und Bezugsrichtung nicht senkrecht"));
        }
        Ok(Axis {
            origin,
            x,
            y: z.cross(x),
            z,
        })
    }

    fn positive(v: &Value, what: &str) -> Result<f64, String> {
        let x = v.real()?;
        if x > 0.0 {
            Ok(x)
        } else {
            Err(format!("{what} nicht positiv: {x}"))
        }
    }

    fn curve(&self, id: u64) -> Result<Crv, String> {
        let e = self.file.get(id)?;
        if let Entity::Complex(_) = e {
            return Ok(Crv::Spline(self.spline_curve(id)?));
        }
        let name = self.file.name_of(id)?.unwrap_or_default();
        let a = self.file.simple(id, name)?;
        Ok(match name {
            "LINE" => {
                let p = self.point(a[1].reference()?)?;
                let v = self.file.simple(a[2].reference()?, "VECTOR")?;
                let d = self.direction(v[1].reference()?)?;
                Crv::Line {
                    p,
                    d: d * Model::positive(&v[2], "Betrag des Vektors")?,
                }
            }
            "CIRCLE" => Crv::Circle {
                a: self.axis(a[1].reference()?)?,
                r: Model::positive(&a[2], "Kreisradius")?,
            },
            "ELLIPSE" => Crv::Ellipse {
                a: self.axis(a[1].reference()?)?,
                r1: Model::positive(&a[2], "Halbachse 1")?,
                r2: Model::positive(&a[3], "Halbachse 2")?,
            },
            "B_SPLINE_CURVE_WITH_KNOTS" => Crv::Spline(self.spline_curve(id)?),
            other => return Err(format!("#{id}: Kurvenart {other} unbekannt")),
        })
    }

    /// `B_SPLINE_CURVE_WITH_KNOTS` einfach oder zusammengesetzt mit `RATIONAL_B_SPLINE_CURVE`.
    fn spline_curve(&self, id: u64) -> Result<Spline1, String> {
        let (degree, cps, form, closed, si, mults, values, rational) = match self.file.get(id)? {
            Entity::Simple(r) if r.name == "B_SPLINE_CURVE_WITH_KNOTS" => {
                let a = &r.args;
                (&a[1], &a[2], &a[3], &a[4], &a[5], &a[6], &a[7], None)
            }
            e @ Entity::Complex(_) => {
                let want = [
                    "BOUNDED_CURVE",
                    "B_SPLINE_CURVE",
                    "B_SPLINE_CURVE_WITH_KNOTS",
                    "CURVE",
                    "GEOMETRIC_REPRESENTATION_ITEM",
                    "RATIONAL_B_SPLINE_CURVE",
                    "REPRESENTATION_ITEM",
                ];
                if e.names() != want {
                    return Err(format!("#{id}: Teile {:?}", e.names()));
                }
                let b = e.part("B_SPLINE_CURVE").unwrap();
                let k = e.part("B_SPLINE_CURVE_WITH_KNOTS").unwrap();
                let w = e.part("RATIONAL_B_SPLINE_CURVE").unwrap();
                (&b[0], &b[1], &b[2], &b[3], &b[4], &k[0], &k[1], Some(&w[0]))
            }
            other => {
                return Err(format!(
                    "#{id}: B-Spline erwartet, gefunden {:?}",
                    other.names()
                ));
            }
        };
        if form.enumeration()? != "UNSPECIFIED" || si.logical()? {
            return Err(format!("#{id}: Form oder Selbstschnitt unerwartet"));
        }
        let degree = usize::try_from(degree.int()?).map_err(|e| e.to_string())?;
        let control: Vec<DVec3> = cps
            .refs()?
            .into_iter()
            .map(|c| self.point(c))
            .collect::<Result<_, _>>()?;
        let mults = mults.ints()?;
        let knots = expand_knots(&mults, &values.reals()?)?;
        check_knots(degree, control.len(), &mults, &knots)?;
        let weights = match rational {
            Some(w) => {
                let w = w.reals()?;
                if w.len() != control.len() || w.iter().any(|x| *x <= 0.0) {
                    return Err(format!("#{id}: Gewichte ungültig"));
                }
                w
            }
            None => vec![1.0; control.len()],
        };
        Ok(Spline1 {
            degree,
            knots,
            control,
            weights,
            closed: closed.logical()?,
        })
    }

    fn surface(&self, id: u64) -> Result<Surf, String> {
        if let Entity::Complex(_) = self.file.get(id)? {
            return Ok(Surf::Spline(self.spline_surface(id)?));
        }
        let name = self.file.name_of(id)?.unwrap_or_default();
        let a = self.file.simple(id, name)?;
        Ok(match name {
            "PLANE" => Surf::Plane(self.axis(a[1].reference()?)?),
            "CYLINDRICAL_SURFACE" => Surf::Cylinder {
                a: self.axis(a[1].reference()?)?,
                r: Model::positive(&a[2], "Zylinderradius")?,
            },
            "CONICAL_SURFACE" => {
                let r = a[2].real()?;
                let semi = a[3].real()?;
                if r < 0.0 {
                    return Err(format!("#{id}: Kegelradius negativ"));
                }
                if !(semi > 0.0 && semi < PI / 2.0) {
                    return Err(format!("#{id}: Öffnungswinkel {semi} außerhalb (0, π/2)"));
                }
                Surf::Cone {
                    a: self.axis(a[1].reference()?)?,
                    r,
                    semi,
                }
            }
            "SPHERICAL_SURFACE" => Surf::Sphere {
                a: self.axis(a[1].reference()?)?,
                r: Model::positive(&a[2], "Kugelradius")?,
            },
            "TOROIDAL_SURFACE" => Surf::Torus {
                a: self.axis(a[1].reference()?)?,
                major: Model::positive(&a[2], "großer Torusradius")?,
                minor: Model::positive(&a[3], "kleiner Torusradius")?,
            },
            "B_SPLINE_SURFACE_WITH_KNOTS" => Surf::Spline(self.spline_surface(id)?),
            other => return Err(format!("#{id}: Flächenart {other} unbekannt")),
        })
    }

    /// `B_SPLINE_SURFACE_WITH_KNOTS` einfach oder zusammengesetzt mit
    /// `RATIONAL_B_SPLINE_SURFACE`.
    fn spline_surface(&self, id: u64) -> Result<Spline2, String> {
        type Parts<'v> = (&'v [Value], &'v [Value], Option<&'v Value>);
        let (b, k, w): Parts = match self.file.get(id)? {
            Entity::Simple(r) if r.name == "B_SPLINE_SURFACE_WITH_KNOTS" => {
                (&r.args[1..8], &r.args[8..13], None)
            }
            e @ Entity::Complex(_) => {
                let want = [
                    "BOUNDED_SURFACE",
                    "B_SPLINE_SURFACE",
                    "B_SPLINE_SURFACE_WITH_KNOTS",
                    "GEOMETRIC_REPRESENTATION_ITEM",
                    "RATIONAL_B_SPLINE_SURFACE",
                    "REPRESENTATION_ITEM",
                    "SURFACE",
                ];
                if e.names() != want {
                    return Err(format!("#{id}: Teile {:?}", e.names()));
                }
                (
                    e.part("B_SPLINE_SURFACE").unwrap(),
                    e.part("B_SPLINE_SURFACE_WITH_KNOTS").unwrap(),
                    Some(&e.part("RATIONAL_B_SPLINE_SURFACE").unwrap()[0]),
                )
            }
            other => {
                return Err(format!(
                    "#{id}: B-Spline-Fläche erwartet, gefunden {:?}",
                    other.names()
                ));
            }
        };
        if b.len() != 7 || k.len() != 5 {
            return Err(format!("#{id}: falsche Zahl der Parameter"));
        }
        if b[3].enumeration()? != "UNSPECIFIED" || b[6].logical()? {
            return Err(format!("#{id}: Form oder Selbstschnitt unerwartet"));
        }
        let du = usize::try_from(b[0].int()?).map_err(|e| e.to_string())?;
        let dv = usize::try_from(b[1].int()?).map_err(|e| e.to_string())?;
        let mut control = Vec::new();
        for row in b[2].list()? {
            let pts: Vec<DVec3> = row
                .refs()?
                .into_iter()
                .map(|c| self.point(c))
                .collect::<Result<_, _>>()?;
            control.push(pts);
        }
        let nv = control.first().map_or(0, Vec::len);
        if control.len() < 2 || nv < 2 || control.iter().any(|r| r.len() != nv) {
            return Err(format!("#{id}: Kontrollpunktnetz nicht rechteckig"));
        }
        let (mu, mv) = (k[0].ints()?, k[1].ints()?);
        let ku = expand_knots(&mu, &k[2].reals()?)?;
        let kv = expand_knots(&mv, &k[3].reals()?)?;
        check_knots(du, control.len(), &mu, &ku)?;
        check_knots(dv, nv, &mv, &kv)?;
        let weights = match w {
            Some(w) => {
                let mut rows = Vec::new();
                for row in w.list()? {
                    rows.push(row.reals()?);
                }
                if rows.len() != control.len()
                    || rows
                        .iter()
                        .any(|r| r.len() != nv || r.iter().any(|x| *x <= 0.0))
                {
                    return Err(format!("#{id}: Gewichte ungültig"));
                }
                rows
            }
            None => vec![vec![1.0; nv]; control.len()],
        };
        Ok(Spline2 {
            du,
            dv,
            ku,
            kv,
            control,
            weights,
            u_closed: b[4].logical()?,
            v_closed: b[5].logical()?,
        })
    }

    /// Alle Hüllen aller Teile (Körper für Körper, außen zuerst).
    pub fn shells(&self) -> Vec<&ShellM> {
        self.products
            .iter()
            .flat_map(|p| p.solids.iter())
            .flat_map(|s| std::iter::once(&s.outer).chain(s.voids.iter()))
            .collect()
    }

    /// Anfangs- und Endpunkt einer orientierten Kante in Laufrichtung.
    pub fn ends(&self, o: &Oriented) -> (u64, u64) {
        let e = &self.edges[&o.edge];
        if o.forward {
            (e.start, e.end)
        } else {
            (e.end, e.start)
        }
    }
}
```

- [ ] **Step 4: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (7 in `model`; alle bisherigen weiter grün).

```bash
git add crates/planora-step/tests/common/mod.rs crates/planora-step/tests/common/model.rs crates/planora-step/tests/model.rs
git commit -m "step: Modell des eigenen Lesers mit eigenen Formeln für Kurven und Flächen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Prüfungen aus den gelesenen Daten, Datei über Hilfsdatei, Stufentest 1

**Files:**
- Create: `crates/planora-step/tests/common/check.rs`, `crates/planora-step/src/file.rs`
- Modify: `crates/planora-step/tests/common/mod.rs` (ganz ersetzen), `crates/planora-step/src/lib.rs` (ganz ersetzen: `mod file`, `pub use file::export_step_file`)
- Test: `crates/planora-step/tests/stage1.rs`

**Interfaces:**
- Consumes: Task 3–5; `planora_mesh::{tessellate, TessellationOptions}` (Dev-Abhängigkeit), `planora_kernel::validate`, `planora_exchange::temp_path`.
- Produces:
  ```rust
  // src/file.rs (pub in planora_step)
  pub fn export_step_file(path: &Path, bodies: &[StepBody], opts: &StepOptions, progress: &Progress)
      -> Result<(StepReport, u64), StepError>;
  // tests/common/check.rs
  pub const GEOM_TOL: f64 = 1e-7; pub const FREEFORM_TOL: f64 = 2e-5; pub const EDGE_SAMPLES: usize = 8;
  pub const PATCH_SAMPLES: usize = 6; pub const NORMAL_MIN: f64 = 0.5; pub const NORMAL_AGREE: f64 = 0.9;
  pub struct ShellStats { pub vertices: usize, pub edges: usize, pub faces: usize, pub rings: usize, pub genus: usize }
  pub fn check_topology(m: &Model) -> Result<Vec<ShellStats>, String>;
  pub fn edge_range(m: &Model, e: &EdgeM) -> Result<(f64, f64), String>;
  pub fn edge_points(m: &Model, e: &EdgeM) -> Result<Vec<DVec3>, String>;
  pub fn check_geometry(m: &Model) -> Result<(), String>;
  pub fn kernel_shells(solid: &Solid) -> Vec<Vec<Id<Face>>>;
  pub fn face_map<'m>(m: &'m Model, product: usize, solid: &Solid) -> Result<Vec<(&'m FaceM, Id<Face>)>, String>;
  pub fn compare_with_kernel(m: &Model, solids: &[&Solid]) -> Result<(), String>;
  pub struct Measure { pub volume: f64, pub area: f64, pub kernel_volume: f64, pub flipped: usize, pub max_offset: f64 }
  pub fn measure(m: &Model, solids: &[&Solid], chord: f64) -> Result<Measure, String>;
  // tests/common/mod.rs
  pub fn checked(text: &str, solids: &[&Solid]) -> (Model, Vec<ShellStats>);   // liest, prüft alles, Panik mit Befund
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/stage1.rs`)

```rust
//! Stufentest 1 (Teilprojekt 10, Task 6): Quader und Prismen werden geschrieben, mit dem eigenen
//! Leser gelesen und geprüft (Topologie, Euler-Zahl, Geometrie, Vergleich mit dem Kernel,
//! Volumen und Fläche aus den STEP-Flächen); Namen, viele Körper, feindliche Eingaben, Datei.

mod common;

use common::check::{Measure, measure};
use common::model::Model;
use common::{checked, export, options};
use planora_exchange::{Progress, temp_path};
use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop, Solid};
use planora_math::{DMat4, DVec2, DVec3, Frame};
use planora_step::{StepBody, StepError, export_step_file, write_step};
use std::fs;
use std::path::PathBuf;

/// Sehnentoleranz der Netze (ebene Körper werden exakt vernetzt).
const CHORD: f64 = 0.01;
/// Schranke für Volumen und Fläche ebener Körper (nur Rundung).
const FLAT_REL: f64 = 1e-12;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

fn prism(points: &[DVec2], holes: &[&[DVec2]], h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::polygon(points).unwrap(),
        holes: holes
            .iter()
            .map(|p| ProfileLoop::polygon(p).unwrap())
            .collect(),
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

fn l_shape() -> Solid {
    let p = [
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (4.0, 4.0),
        (4.0, 10.0),
        (0.0, 10.0),
    ]
    .map(|(x, y)| DVec2::new(x, y));
    prism(&p, &[], 5.0)
}

fn frame_with_hole() -> Solid {
    let outer = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)].map(|(x, y)| DVec2::new(x, y));
    let hole = [(3.0, 2.0), (7.0, 2.0), (7.0, 6.0), (3.0, 6.0)].map(|(x, y)| DVec2::new(x, y));
    prism(&outer, &[&hole], 2.0)
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn measured(text: &str, solids: &[&Solid]) -> Measure {
    let m = Model::read(text).unwrap();
    let r = measure(&m, solids, CHORD).unwrap();
    assert_eq!(r.flipped, 0, "kein Dreieck falsch herum");
    r
}

#[test]
fn boxes_and_prisms_pass_all_checks_with_the_right_volume() {
    let cube = k().make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    for (name, s, volume, area, genus) in [
        ("Quader", cube, 6000.0, 2200.0, 0),
        ("L-Form", l_shape(), 320.0, 2.0 * 64.0 + 40.0 * 5.0, 0),
        (
            "Rahmen",
            frame_with_hole(),
            128.0,
            2.0 * 64.0 + (36.0 + 16.0) * 2.0,
            1,
        ),
    ] {
        let (text, report) = export(&[(name, &s)]);
        let (m, stats) = checked(&text, &[&s]);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].genus, genus, "{name}");
        assert_eq!(report.faces, stats[0].faces);
        assert_eq!(m.products[0].name, name);
        let r = measured(&text, &[&s]);
        assert!(rel(r.volume, volume) < FLAT_REL, "{name}: {}", r.volume);
        assert!(rel(r.area, area) < FLAT_REL, "{name}: {}", r.area);
        assert!(rel(r.kernel_volume, volume) < FLAT_REL);
        assert!(r.max_offset < 1e-9, "{name}: {}", r.max_offset);
    }
}

#[test]
fn moved_rotated_and_mirrored_boxes_keep_their_outward_normals() {
    let base = l_shape();
    for (what, m) in [
        (
            "verschoben",
            DMat4::from_translation(DVec3::new(-50.0, 12.5, 3.0)),
        ),
        (
            "gedreht",
            DMat4::from_rotation_y(0.7) * DMat4::from_rotation_z(-1.1),
        ),
        ("gespiegelt", DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0))),
        (
            "gespiegelt und gedreht",
            DMat4::from_rotation_x(2.0) * DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        ),
        ("vergrößert", DMat4::from_scale(DVec3::splat(3.5))),
    ] {
        let s = moved(&base, m);
        let (text, _) = export(&[(what, &s)]);
        checked(&text, &[&s]);
        let r = measured(&text, &[&s]);
        let want = 320.0 * m.determinant().abs();
        assert!(
            rel(r.volume, want) < 1e-9,
            "{what}: {} statt {want}",
            r.volume
        );
    }
}

#[test]
fn several_bodies_become_several_named_parts() {
    let a = k().make_box(DVec3::splat(2.0)).unwrap();
    let b = moved(&l_shape(), DMat4::from_translation(DVec3::X * 20.0));
    let c = moved(&frame_with_hole(), DMat4::from_translation(DVec3::Y * 20.0));
    let names = ["Würfel „klein“", "O'Brien \\ L", "Rahmen 😀"];
    let (text, report) = export(&[(names[0], &a), (names[1], &b), (names[2], &c)]);
    assert_eq!(report.bodies, 3);
    let (m, stats) = checked(&text, &[&a, &b, &c]);
    let got: Vec<&str> = m.products.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(got, names);
    assert_eq!(stats.len(), 3);
    assert_eq!(m.file.count("PRODUCT"), 3);
    assert_eq!(m.file.count("APPLICATION_CONTEXT"), 1);
    let r = measured(&text, &[&a, &b, &c]);
    assert!(
        rel(r.volume, 8.0 + 320.0 + 128.0) < FLAT_REL,
        "{}",
        r.volume
    );
}

#[test]
fn the_header_is_read_back_by_the_own_reader() {
    let s = k().make_box(DVec3::splat(1.0)).unwrap();
    let (text, _) = export(&[("Q", &s)]);
    let m = Model::read(&text).unwrap();
    let name = &m.file.header[1];
    assert_eq!(name.args[0].text().unwrap(), "probe.step");
    assert_eq!(name.args[1].text().unwrap(), "2026-10-14T12:34:56");
    assert_eq!(name.args[4].text().unwrap(), "PlanoraCAD");
    assert_eq!(m.uncertainty, 1e-6);
}

#[test]
fn long_names_are_shortened_and_reported() {
    let s = k().make_box(DVec3::splat(1.0)).unwrap();
    let long = "Sehr langer Name ".repeat(20);
    let (text, report) = export(&[(&long, &s)]);
    assert_eq!(report.shortened_names, vec![long.clone()]);
    let m = Model::read(&text).unwrap();
    let got = &m.products[0].name;
    assert_eq!(got.chars().count(), 256);
    assert!(long.starts_with(got.as_str()));
}

#[test]
fn many_bodies_fit_into_one_file() {
    let s = k().make_box(DVec3::splat(1.0)).unwrap();
    let names: Vec<String> = (1..=200).map(|i| format!("Quader {i}")).collect();
    let bodies: Vec<(&str, &Solid)> = names.iter().map(|n| (n.as_str(), &s)).collect();
    let (text, report) = export(&bodies);
    assert_eq!(report.bodies, 200);
    let m = Model::read(&text).unwrap();
    assert_eq!(m.products.len(), 200);
    assert_eq!(m.products[199].name, "Quader 200");
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-step-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn files_go_through_a_helper_and_failures_keep_the_old_file() {
    let s = k().make_box(DVec3::splat(3.0)).unwrap();
    let dir = temp_dir("datei");
    let path = dir.join("Quader.step");
    let bodies = [StepBody {
        name: "Quader".into(),
        solid: &s,
    }];
    let (report, bytes) = export_step_file(&path, &bodies, &options(), &Progress::new()).unwrap();
    assert_eq!(report.bodies, 1);
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(bytes, text.len() as u64);
    checked(&text, &[&s]);
    assert!(!temp_path(&path).exists());
    // Abbrechen und „nichts auszugeben“ lassen die alte Datei stehen.
    let cancelled = Progress::new();
    cancelled.cancel();
    assert_eq!(
        export_step_file(&path, &bodies, &options(), &cancelled).unwrap_err(),
        StepError::Cancelled
    );
    let mut open = s.clone();
    let f = open.faces.iter().next().unwrap().0;
    open.faces[f].outer.half_edges.pop();
    let bad = [StepBody {
        name: "Offen".into(),
        solid: &open,
    }];
    assert_eq!(
        export_step_file(&path, &bad, &options(), &Progress::new()).unwrap_err(),
        StepError::NothingToExport
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert!(!temp_path(&path).exists());
    let missing = dir.join("fehlt").join("x.step");
    assert!(matches!(
        export_step_file(&missing, &bodies, &options(), &Progress::new()),
        Err(StepError::Io { .. })
    ));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bodies_rejected_before_writing_never_reach_the_file() {
    let good = k().make_box(DVec3::splat(1.0)).unwrap();
    let mut nan = good.clone();
    let v = nan.vertices.iter().next().unwrap().0;
    nan.vertices[v].point.y = f64::INFINITY;
    let mut out = Vec::new();
    let bodies = [
        StepBody {
            name: "Unendlich".into(),
            solid: &nan,
        },
        StepBody {
            name: "Gut".into(),
            solid: &good,
        },
    ];
    let report = write_step(&bodies, &options(), &mut out, &Progress::new()).unwrap();
    assert_eq!(report.bodies, 1);
    assert_eq!(report.rejected.len(), 1);
    let text = String::from_utf8(out).unwrap();
    checked(&text, &[&good]);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test stage1`
Expected: FAIL – `could not find check in common` bzw. `cannot find function export_step_file`.

- [ ] **Step 3: Implementieren**

`crates/planora-step/src/file.rs`:

```rust
//! Schreiben in eine Datei über eine Hilfsdatei im Zielordner (Spec §4, wie
//! `planora_exchange::export_file`).

use crate::error::StepError;
use crate::{StepBody, StepOptions, StepReport, write_step};
use planora_exchange::{Progress, temp_path};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

/// Schreibt die Körper als STEP-Datei nach `path`: erst in die Hilfsdatei `.<Name>.planora-tmp`,
/// `sync_all`, dann umbenennen. Bei jedem Fehler (auch Abbrechen und `NothingToExport`) bleibt
/// eine vorhandene Zieldatei unverändert und die Hilfsdatei wird entfernt. Liefert Bericht und
/// Dateigröße in Bytes.
pub fn export_step_file(
    path: &Path,
    bodies: &[StepBody],
    opts: &StepOptions,
    progress: &Progress,
) -> Result<(StepReport, u64), StepError> {
    let tmp = temp_path(path);
    let result = write_to(&tmp, bodies, opts, progress).and_then(|done| {
        fs::rename(&tmp, path)?;
        Ok(done)
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_to(
    tmp: &Path,
    bodies: &[StepBody],
    opts: &StepOptions,
    progress: &Progress,
) -> Result<(StepReport, u64), StepError> {
    let mut w = BufWriter::new(File::create(tmp)?);
    let report = write_step(bodies, opts, &mut w, progress)?;
    let file = w
        .into_inner()
        .map_err(|e| StepError::from(e.into_error()))?;
    file.sync_all()?;
    Ok((report, file.metadata()?.len()))
}
```

`crates/planora-step/src/lib.rs` (ganz ersetzen):

```rust
//! STEP-Export (Teilprojekt 10): exakte Körper des Kernels als STEP-Datei (AP214,
//! `AUTOMOTIVE_DESIGN`). Das Textformat ist von Hand geschrieben, ohne neue Abhängigkeit.

pub mod entity;
pub mod error;
mod file;
mod geometry;
pub mod number;
mod product;
pub mod text;
mod topology;

pub use entity::{Entities, MAX_ENTITIES, WRAP_COLUMN, logical, refs};
pub use error::StepError;
pub use file::export_step_file;
pub use geometry::FRAME_TOL;
pub use number::{push_real, real};
pub use product::{SCHEMA, SYSTEM, utc_timestamp};
pub use text::{MAX_NAME_CHARS, StepText, step_string};

use planora_exchange::Progress;
use planora_kernel::{Curve, NURBS_EDGE_TOL, Solid, Surface, validate_with};
use planora_math::Tolerance;
use std::io::Write;
use std::time::SystemTime;

/// Ein Körper für die Ausgabe: Name aus dem Feature-Baum und der exakte Körper des Kernels.
#[derive(Clone, Debug)]
pub struct StepBody<'a> {
    pub name: String,
    pub solid: &'a Solid,
}

/// Einstellungen der Ausgabe.
#[derive(Clone, Debug, PartialEq)]
pub struct StepOptions {
    /// Dateiname für den Kopf (`FILE_NAME`), ohne Ordner.
    pub file_name: String,
    /// Zeitpunkt für den Kopf.
    pub time: SystemTime,
    /// Toleranz des Kernels; daraus folgt die Unsicherheit im geometrischen Kontext.
    pub tolerance: Tolerance,
}

impl StepOptions {
    /// Jetziger Zeitpunkt, Toleranz des Kernels.
    pub fn new(file_name: &str) -> StepOptions {
        StepOptions {
            file_name: file_name.to_string(),
            time: SystemTime::now(),
            tolerance: Tolerance::default(),
        }
    }
}

/// Bericht über eine geschriebene Datei (Spec §2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepReport {
    /// Zahl der geschriebenen Körper (Teile).
    pub bodies: usize,
    pub faces: usize,
    pub edges: usize,
    /// Zahl der Einträge `#n` in der Datei.
    pub entities: u64,
    /// Abgelehnte Körper, je ein [`StepError::InvalidBody`].
    pub rejected: Vec<StepError>,
    /// Namen, die auf [`MAX_NAME_CHARS`] Zeichen gekürzt wurden.
    pub shortened_names: Vec<String>,
}

/// Kennzahlen eines geprüften Körpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyStats {
    pub faces: usize,
    pub edges: usize,
    /// Zahl der Einträge, die der Körper ohne Teil-Einträge braucht.
    pub entities: u64,
}

/// Anzeigename: leere Namen werden „Körper n“ (n ab 1).
pub fn display_name(name: &str, index: usize) -> String {
    if name.trim().is_empty() {
        format!("Körper {}", index + 1)
    } else {
        name.to_string()
    }
}

/// Prüft einen Körper (Spec §4): Verweise, `validate` und ein Probelauf des Schreibens ohne
/// Ausgabe. Jede Ablehnung ist ein [`StepError::InvalidBody`] mit dem Namen des Körpers;
/// `Cancelled` und `TooManyEntities` werden durchgereicht.
pub fn check_body(body: &StepBody, progress: &Progress) -> Result<BodyStats, StepError> {
    let named = |reason: String| StepError::InvalidBody {
        name: body.name.clone(),
        reason,
    };
    if !topology::ids_in_range(body.solid) {
        return Err(named(
            "Der Körper verweist auf Teile, die es nicht gibt".into(),
        ));
    }
    validate_with(body.solid, &Tolerance::default()).map_err(|e| named(e.to_string()))?;
    let mut sink = std::io::sink();
    let mut e = Entities::probe(&mut sink, progress);
    match topology::write_solid(&mut e, "''", body.solid) {
        Ok(w) => Ok(BodyStats {
            faces: w.faces,
            edges: w.edges,
            entities: e.count(),
        }),
        Err(StepError::InvalidBody { reason, .. }) => Err(named(reason)),
        Err(StepError::NotFinite) => Err(named(StepError::NotFinite.to_string())),
        Err(other) => Err(other),
    }
}

/// Hat der Körper Freiformkurven oder -flächen?
fn has_freeform(solid: &Solid) -> bool {
    solid
        .edges
        .iter()
        .any(|(_, e)| matches!(e.curve, Curve::Nurbs(_)))
        || solid
            .faces
            .iter()
            .any(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
}

/// Unsicherheit für den geometrischen Kontext: die Längentoleranz des Kernels, bei Körpern mit
/// Freiformkurven oder -flächen mindestens [`NURBS_EDGE_TOL`] (so weit dürfen dort Kanten von
/// ihren Flächen abweichen).
fn uncertainty(solids: &[&Solid], tol: &Tolerance) -> f64 {
    if solids.iter().any(|s| has_freeform(s)) {
        tol.linear.max(NURBS_EDGE_TOL)
    } else {
        tol.linear
    }
}

/// Schreibt die Körper als STEP-Datei nach `out` (Spec §2): eine Datei, je Körper ein Teil mit
/// dem Namen aus dem Feature-Baum. Körper, die die Prüfung nicht bestehen, werden ausgelassen
/// und stehen im Bericht; bleibt keiner übrig, ist das `NothingToExport` (dann wurde nichts
/// geschrieben). `progress` zählt die Einträge und bricht ab, wenn gewünscht.
pub fn write_step(
    bodies: &[StepBody],
    opts: &StepOptions,
    out: &mut dyn Write,
    progress: &Progress,
) -> Result<StepReport, StepError> {
    let mut report = StepReport::default();
    let mut accepted: Vec<(String, &Solid, BodyStats)> = Vec::new();
    for (i, b) in bodies.iter().enumerate() {
        if progress.is_cancelled() {
            return Err(StepError::Cancelled);
        }
        let name = display_name(&b.name, i);
        let checked = StepBody {
            name: name.clone(),
            solid: b.solid,
        };
        match check_body(&checked, progress) {
            Ok(stats) => accepted.push((name, b.solid, stats)),
            Err(e @ StepError::InvalidBody { .. }) => report.rejected.push(e),
            Err(e) => return Err(e),
        }
    }
    if accepted.is_empty() {
        return Err(StepError::NothingToExport);
    }
    progress.set_total(accepted.iter().map(|a| a.2.entities).sum());
    let solids: Vec<&Solid> = accepted.iter().map(|a| a.1).collect();
    let mut e = Entities::new(out, progress);
    product::header(&mut e, &opts.file_name, opts.time)?;
    let ctx = product::context(&mut e, uncertainty(&solids, &opts.tolerance))?;
    for (name, solid, _) in &accepted {
        let text = step_string(name);
        if text.shortened {
            report.shortened_names.push(name.clone());
        }
        let w = topology::write_solid(&mut e, &text.text, solid)?;
        product::product(&mut e, &ctx, &text.text, &w.items)?;
        report.bodies += 1;
        report.faces += w.faces;
        report.edges += w.edges;
    }
    product::trailer(&mut e)?;
    report.entities = e.count();
    Ok(report)
}
```

`crates/planora-step/tests/common/mod.rs` (ganz ersetzen):

```rust
//! Gemeinsame Hilfen der Tests: eigener STEP-Leser (Spec §5.1), Modell mit eigenen Formeln
//! (§5.2) und Ausgabe über `write_step`.
#![allow(dead_code)]

pub mod check;
pub mod model;
pub mod reader;

use check::{ShellStats, check_geometry, check_topology, compare_with_kernel};
use model::Model;
use planora_exchange::Progress;
use planora_kernel::{Solid, validate};
use planora_step::{StepBody, StepOptions, StepReport, write_step};
use std::time::{Duration, UNIX_EPOCH};

/// Feste Einstellungen (gleiche Datei bei gleichem Inhalt): Zeitpunkt 2026-10-14T12:34:56.
pub fn options() -> StepOptions {
    StepOptions {
        time: UNIX_EPOCH + Duration::from_secs(1_791_981_296),
        ..StepOptions::new("probe.step")
    }
}

/// Schreibt die Körper mit Namen und liefert Text und Bericht.
pub fn export(bodies: &[(&str, &Solid)]) -> (String, StepReport) {
    let list: Vec<StepBody> = bodies
        .iter()
        .map(|(name, solid)| StepBody {
            name: (*name).to_string(),
            solid,
        })
        .collect();
    let mut out = Vec::new();
    let report = write_step(&list, &options(), &mut out, &Progress::new())
        .unwrap_or_else(|e| panic!("write_step: {e}"));
    (String::from_utf8(out).expect("nur ASCII"), report)
}

/// Kleiner Zufallsgenerator mit festem Startwert (keine Abhängigkeit).
pub struct Lcg(pub u64);

impl Lcg {
    pub fn uniform(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.uniform()
    }
}

/// Liest den Text mit dem eigenen Leser und prüft alles (Topologie, Geometrie, Vergleich mit
/// dem Kernel); die Kennzahlen der Hüllen müssen zu `validate` passen. Panik mit Meldung bei
/// jedem Befund.
pub fn checked(text: &str, solids: &[&Solid]) -> (Model, Vec<ShellStats>) {
    let m = Model::read(text).unwrap_or_else(|e| panic!("Lesen: {e}"));
    let stats = check_topology(&m).unwrap_or_else(|e| panic!("Topologie: {e}"));
    check_geometry(&m).unwrap_or_else(|e| panic!("Geometrie: {e}"));
    compare_with_kernel(&m, solids).unwrap_or_else(|e| panic!("Vergleich: {e}"));
    let faces: usize = stats.iter().map(|s| s.faces).sum();
    let edges: usize = stats.iter().map(|s| s.edges).sum();
    let genus: usize = stats.iter().map(|s| s.genus).sum();
    let (kf, ke, kg) = solids.iter().fold((0, 0, 0), |(f, e, g), s| {
        let r = validate(s).unwrap_or_else(|e| panic!("validate: {e}"));
        (f + r.faces, e + r.edges, g + r.genus)
    });
    assert_eq!((faces, edges, genus), (kf, ke, kg), "Zahlen wie im Kernel");
    (m, stats)
}
```

`crates/planora-step/tests/common/check.rs` (enthält schon die Prüfung geschlossener B-Splines; sie greift ab Task 10):

```rust
//! Prüfungen aus den gelesenen Daten (Spec §5.2, §5.3) und Vergleich mit dem Kernel: Topologie
//! und Euler-Zahl je Hülle, Eckpunkte auf ihren Kurven, Kanten auf beiden Nachbarflächen,
//! Punktprobe Kernel ↔ STEP-Fläche mit Normalenrichtung, Netz aus den STEP-Flächen.

use super::model::{Crv, EdgeM, FaceM, Model, Oriented, ShellM, Surf};
use planora_kernel::{Face, Id, Solid};
use planora_math::DVec3;
use planora_mesh::{TessellationOptions, tessellate};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Lagetoleranz für analytische Kurven und Flächen (mm).
pub const GEOM_TOL: f64 = 1e-7;
/// Lagetoleranz, wenn eine Freiformkurve oder -fläche beteiligt ist (mm); über der des Kernels
/// (`NURBS_EDGE_TOL` = 1e-5), weil Kanten dort nur so genau auf ihren Flächen liegen.
pub const FREEFORM_TOL: f64 = 2e-5;
/// Abschnitte, an deren Grenzen eine Kante geprüft wird (Enden eingeschlossen).
pub const EDGE_SAMPLES: usize = 8;
/// Raster der Punktprobe auf gekrümmten Flächen: `(n + 1)²` Punkte über den Parameterbereich.
pub const PATCH_SAMPLES: usize = 6;
/// Mindestlänge einer Normale, damit ihre Richtung zählt (an Polen und Spitzen kürzer).
pub const NORMAL_MIN: f64 = 0.5;
/// Mindestwert des Skalarprodukts zweier Einheitsnormalen, die dieselbe Seite meinen.
pub const NORMAL_AGREE: f64 = 0.9;

/// Kennzahlen einer Hülle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellStats {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    /// Innere Ränder (Löcher in Flächen).
    pub rings: usize,
    pub genus: usize,
}

/// Toleranz für eine Kurve.
fn curve_tol(c: &Crv) -> f64 {
    if matches!(c, Crv::Spline(_)) {
        FREEFORM_TOL
    } else {
        GEOM_TOL
    }
}

/// Orientierte Kanten eines Randes in Laufrichtung (`orientation = .F.` kehrt um).
fn walk(edges: &[Oriented], orientation: bool) -> Vec<Oriented> {
    if orientation {
        edges.to_vec()
    } else {
        edges
            .iter()
            .rev()
            .map(|o| Oriented {
                edge: o.edge,
                forward: !o.forward,
            })
            .collect()
    }
}

/// Topologie je Hülle (Spec §5.3): jede Kante genau einmal vorwärts und einmal rückwärts, jede
/// Schleife geschlossen, keine Kante in zwei Hüllen, Euler-Poincaré
/// `V − E + F − R = 2 − 2g` mit ganzzahligem `g ≥ 0`.
pub fn check_topology(m: &Model) -> Result<Vec<ShellStats>, String> {
    let mut owner: HashMap<u64, u64> = HashMap::new();
    let mut out = Vec::new();
    for shell in m.shells() {
        let mut uses: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
        let mut rings = 0;
        for face in &shell.faces {
            rings += face.bounds.len() - 1;
            for bound in &face.bounds {
                let seq = walk(&bound.edges, bound.orientation);
                for (i, o) in seq.iter().enumerate() {
                    let u = uses.entry(o.edge).or_default();
                    if o.forward {
                        u.0 += 1;
                    } else {
                        u.1 += 1;
                    }
                    let next = &seq[(i + 1) % seq.len()];
                    if m.ends(o).1 != m.ends(next).0 {
                        return Err(format!("Fläche #{}: Schleife nicht geschlossen", face.id));
                    }
                }
            }
        }
        let mut verts = BTreeSet::new();
        for (edge, (f, b)) in &uses {
            if (*f, *b) != (1, 1) {
                return Err(format!(
                    "Kante #{edge}: {f}-mal vorwärts, {b}-mal rückwärts (erwartet je einmal)"
                ));
            }
            if let Some(other) = owner.insert(*edge, shell.id) {
                return Err(format!(
                    "Kante #{edge} in den Hüllen #{other} und #{}",
                    shell.id
                ));
            }
            let e = &m.edges[edge];
            verts.insert(e.start);
            verts.insert(e.end);
        }
        let chi = verts.len() as i64 - uses.len() as i64 + shell.faces.len() as i64 - rings as i64;
        let twice_genus = 2 - chi;
        if twice_genus < 0 || twice_genus % 2 != 0 {
            return Err(format!("Hülle #{}: Euler-Zahl {chi} unmöglich", shell.id));
        }
        out.push(ShellStats {
            vertices: verts.len(),
            edges: uses.len(),
            faces: shell.faces.len(),
            rings,
            genus: (twice_genus / 2) as usize,
        });
    }
    Ok(out)
}

/// Parameterbereich einer Kante auf ihrer STEP-Kurve: vom Anfangs- zum Endpunkt in
/// Kurvenrichtung (`same_sense`) bzw. dagegen; Vollkreise (gleicher Eckpunkt) eine ganze
/// Periode.
pub fn edge_range(m: &Model, e: &EdgeM) -> Result<(f64, f64), String> {
    let (p0, p1) = (m.vertices[&e.start], m.vertices[&e.end]);
    let t0 = e.curve.param(p0);
    let raw = e.curve.param(p1);
    let t1 = match e.curve.period() {
        Some(per) if e.start == e.end => {
            if e.same_sense {
                t0 + per
            } else {
                t0 - per
            }
        }
        Some(per) => {
            if e.same_sense {
                t0 + (raw - t0).rem_euclid(per)
            } else {
                t0 - (t0 - raw).rem_euclid(per)
            }
        }
        None => {
            if (raw > t0) != e.same_sense && raw != t0 {
                return Err("Kante läuft gegen die Richtung ihres same_sense".into());
            }
            raw
        }
    };
    Ok((t0, t1))
}

/// Punkte einer Kante (Enden eingeschlossen) auf der STEP-Kurve.
pub fn edge_points(m: &Model, e: &EdgeM) -> Result<Vec<DVec3>, String> {
    let (t0, t1) = edge_range(m, e)?;
    Ok((0..=EDGE_SAMPLES)
        .map(|k| {
            e.curve
                .point(t0 + (t1 - t0) * k as f64 / EDGE_SAMPLES as f64)
        })
        .collect())
}

/// Geometrie aus den gelesenen Daten (Spec §5.2): jeder Eckpunkt liegt auf seinen Kurven, jede
/// Kante auf allen Flächen, die sie begrenzt; die Enden der Kurvenstücke treffen die Eckpunkte.
pub fn check_geometry(m: &Model) -> Result<(), String> {
    let mut samples: BTreeMap<u64, Vec<DVec3>> = BTreeMap::new();
    for (id, e) in &m.edges {
        let tol = curve_tol(&e.curve);
        for v in [e.start, e.end] {
            let d = e.curve.distance(m.vertices[&v]);
            if d > tol {
                return Err(format!("Eckpunkt #{v} liegt {d} neben der Kurve von #{id}"));
            }
        }
        let pts = edge_points(m, e)?;
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        if first.distance(m.vertices[&e.start]) > tol || last.distance(m.vertices[&e.end]) > tol {
            return Err(format!(
                "Kante #{id}: Kurvenstück trifft die Eckpunkte nicht"
            ));
        }
        samples.insert(*id, pts);
    }
    for shell in m.shells() {
        for face in &shell.faces {
            let face_tol = if face.surface.is_spline() {
                FREEFORM_TOL
            } else {
                GEOM_TOL
            };
            for bound in &face.bounds {
                for o in &bound.edges {
                    let tol = face_tol.max(curve_tol(&m.edges[&o.edge].curve));
                    for p in &samples[&o.edge] {
                        let d = face.surface.distance(*p);
                        if d > tol {
                            return Err(format!(
                                "Kante #{} liegt {d} neben der Fläche #{}",
                                o.edge, face.id
                            ));
                        }
                    }
                }
            }
            check_closed_surface(face)?;
        }
    }
    for (id, e) in &m.edges {
        if let Crv::Spline(s) = &e.curve
            && s.closed
        {
            let (a, b) = s.domain();
            let gap = s.point(a).distance(s.point(b));
            if gap > FREEFORM_TOL {
                return Err(format!("Kurve von #{id} heißt geschlossen, Lücke {gap}"));
            }
        }
    }
    Ok(())
}

/// Als geschlossen gekennzeichnete Freiformflächen haben gleiche erste und letzte
/// Kontrollpunktreihe in dieser Richtung.
fn check_closed_surface(face: &FaceM) -> Result<(), String> {
    let Surf::Spline(s) = &face.surface else {
        return Ok(());
    };
    let (nu, nv) = s.counts();
    let u_ok = (0..nv).all(|j| s.control[0][j].distance(s.control[nu - 1][j]) <= FREEFORM_TOL);
    let v_ok = (0..nu).all(|i| s.control[i][0].distance(s.control[i][nv - 1]) <= FREEFORM_TOL);
    if (s.u_closed && !u_ok) || (s.v_closed && !v_ok) {
        return Err(format!(
            "Fläche #{}: als geschlossen gekennzeichnet, ist es aber nicht",
            face.id
        ));
    }
    Ok(())
}

/// Flächen der Schalen eines Kernel-Körpers (ohne Schalenliste alle Flächen als eine).
pub fn kernel_shells(solid: &Solid) -> Vec<Vec<Id<Face>>> {
    if solid.shells.is_empty() {
        vec![solid.faces.iter().map(|(id, _)| id).collect()]
    } else {
        solid.shells.iter().map(|s| s.faces.clone()).collect()
    }
}

/// Erster Eckpunkt einer Hülle: Anfang der ersten Kante im äußeren Rand der ersten Fläche.
fn first_point_step(m: &Model, s: &ShellM) -> DVec3 {
    let b = &s.faces[0].bounds[0];
    let seq = walk(&b.edges, b.orientation);
    m.vertices[&m.ends(&seq[0]).0]
}

fn first_point_kernel(solid: &Solid, faces: &[Id<Face>]) -> DVec3 {
    let he = solid.faces[faces[0]].outer.half_edges[0];
    solid.vertices[solid.half_edge_start(he)].point
}

/// Zuordnung STEP-Fläche → Kernel-Fläche für einen Körper: Hüllen über Flächenzahl und ersten
/// Eckpunkt (bitgleich, weil die Zahlen verlustfrei geschrieben werden), Flächen in Reihenfolge.
pub fn face_map<'m>(
    m: &'m Model,
    product: usize,
    solid: &Solid,
) -> Result<Vec<(&'m FaceM, Id<Face>)>, String> {
    let shells: Vec<&ShellM> = m.products[product]
        .solids
        .iter()
        .flat_map(|s| std::iter::once(&s.outer).chain(s.voids.iter()))
        .collect();
    let kernel = kernel_shells(solid);
    if shells.len() != kernel.len() {
        return Err(format!("{} Hüllen statt {}", shells.len(), kernel.len()));
    }
    let mut used = vec![false; kernel.len()];
    let mut out = Vec::new();
    for s in shells {
        let p = first_point_step(m, s);
        let k = (0..kernel.len())
            .find(|&k| {
                !used[k]
                    && kernel[k].len() == s.faces.len()
                    && first_point_kernel(solid, &kernel[k]) == p
            })
            .ok_or_else(|| format!("Hülle #{} passt zu keiner Schale des Kernels", s.id))?;
        used[k] = true;
        out.extend(s.faces.iter().zip(kernel[k].iter().copied()));
    }
    Ok(out)
}

/// Probepunkte einer Kernel-Fläche mit äußerer Normale: Raster über den Parameterbereich bzw.
/// bei Ebenen die Eckpunkte der Schleifen.
fn kernel_probes(solid: &Solid, id: Id<Face>) -> Vec<(DVec3, DVec3)> {
    let f = &solid.faces[id];
    let sign = if f.same_sense { 1.0 } else { -1.0 };
    match f.patch {
        Some(r) => {
            let mut out = Vec::new();
            for i in 0..=PATCH_SAMPLES {
                for j in 0..=PATCH_SAMPLES {
                    let u = r.u0 + (r.u1 - r.u0) * i as f64 / PATCH_SAMPLES as f64;
                    let v = r.v0 + (r.v1 - r.v0) * j as f64 / PATCH_SAMPLES as f64;
                    out.push((f.surface.point(u, v), f.surface.normal(u, v) * sign));
                }
            }
            out
        }
        None => std::iter::once(&f.outer)
            .chain(f.inner.iter())
            .flat_map(|lp| lp.half_edges.iter())
            .map(|he| {
                let p = solid.vertices[solid.half_edge_start(*he)].point;
                let (u, v) = f.surface.project(p);
                (p, f.surface.normal(u, v) * sign)
            })
            .collect(),
    }
}

/// Vergleich mit dem Kernel (Spec §5.2): je Teil ein Körper in Reihenfolge; gleiche Zahl von
/// Rändern und Kanten, gleiche Richtungen, bitgleiche Eckpunkte; Punkte der Kernel-Kanten auf
/// den STEP-Kurven (Laufrichtung gleich), Punkte der Kernel-Flächen auf den STEP-Flächen mit
/// gleicher Außennormale (auch bei gespiegelten und gedrehten Körpern).
pub fn compare_with_kernel(m: &Model, solids: &[&Solid]) -> Result<(), String> {
    if m.products.len() != solids.len() {
        return Err(format!("{} Teile statt {}", m.products.len(), solids.len()));
    }
    for (i, solid) in solids.iter().enumerate() {
        for (fm, fid) in face_map(m, i, solid)? {
            compare_face(m, fm, solid, fid)?;
        }
    }
    Ok(())
}

fn compare_face(m: &Model, fm: &FaceM, solid: &Solid, fid: Id<Face>) -> Result<(), String> {
    let kf = &solid.faces[fid];
    let loops: Vec<_> = std::iter::once(&kf.outer).chain(kf.inner.iter()).collect();
    if fm.bounds.len() != loops.len() || !fm.bounds[0].outer {
        return Err(format!("Fläche #{}: Ränder passen nicht", fm.id));
    }
    for (b, lp) in fm.bounds.iter().zip(&loops) {
        if !b.orientation || b.edges.len() != lp.half_edges.len() {
            return Err(format!("Fläche #{}: Schleife passt nicht", fm.id));
        }
        for (o, he) in b.edges.iter().zip(&lp.half_edges) {
            if o.forward != he.forward {
                return Err(format!(
                    "Kante #{}: Richtung in der Schleife falsch",
                    o.edge
                ));
            }
            let e = &m.edges[&o.edge];
            let ke = &solid.edges[he.edge];
            let (ks, kend) = (solid.vertices[ke.start].point, solid.vertices[ke.end].point);
            if m.vertices[&e.start] != ks || m.vertices[&e.end] != kend {
                return Err(format!("Kante #{}: Eckpunkte nicht bitgleich", o.edge));
            }
            let tol = curve_tol(&e.curve);
            for k in 0..=EDGE_SAMPLES {
                let t = ke.t0 + (ke.t1 - ke.t0) * k as f64 / EDGE_SAMPLES as f64;
                let d = e.curve.distance(ke.curve.point(t));
                if d > tol {
                    return Err(format!(
                        "Kante #{}: Kernelpunkt {d} neben der Kurve",
                        o.edge
                    ));
                }
            }
            let kt = ke.curve.tangent(ke.t0) * (ke.t1 - ke.t0).signum();
            let st = e.curve.tangent(e.curve.param(ks)) * if e.same_sense { 1.0 } else { -1.0 };
            if kt.dot(st) <= 0.0 {
                return Err(format!("Kante #{}: läuft in STEP andersherum", o.edge));
            }
        }
    }
    let tol = if fm.surface.is_spline() {
        FREEFORM_TOL
    } else {
        GEOM_TOL
    };
    let sign = if fm.same_sense { 1.0 } else { -1.0 };
    for (p, nk) in kernel_probes(solid, fid) {
        let d = fm.surface.distance(p);
        if d > tol {
            return Err(format!(
                "Fläche #{}: Kernelpunkt {p} liegt {d} daneben",
                fm.id
            ));
        }
        let ns = fm.surface.normal_at(p) * sign;
        if nk.length() >= NORMAL_MIN && ns.length() >= NORMAL_MIN {
            let agree = nk.normalize().dot(ns.normalize());
            if agree < NORMAL_AGREE {
                return Err(format!(
                    "Fläche #{}: Außennormale bei {p} zeigt anders ({agree})",
                    fm.id
                ));
            }
        }
    }
    Ok(())
}

/// Maße aus den STEP-Flächen (Spec §5.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    /// Volumen und Fläche des Netzes mit Punkten auf den STEP-Flächen und Dreiecken nach deren
    /// Außennormale ausgerichtet.
    pub volume: f64,
    pub area: f64,
    /// Volumen desselben Netzes mit den Punkten und Richtungen des Kernels.
    pub kernel_volume: f64,
    /// Dreiecke, die nach der STEP-Normale umgedreht werden mussten (richtig: 0).
    pub flipped: usize,
    /// Größter Abstand eines Netzpunkts des Kernels von seiner STEP-Fläche.
    pub max_offset: f64,
}

/// Netz aus den gelesenen STEP-Flächen: Die Dreiecke des Kernel-Netzes (Sehnentoleranz `chord`)
/// geben nur die Verbindung vor; jeder Punkt wird auf die STEP-Fläche seiner Fläche gelegt, jedes
/// Dreieck nach deren Außennormale (natürliche Normale × `same_sense`) ausgerichtet.
pub fn measure(m: &Model, solids: &[&Solid], chord: f64) -> Result<Measure, String> {
    let mut out = Measure {
        volume: 0.0,
        area: 0.0,
        kernel_volume: 0.0,
        flipped: 0,
        max_offset: 0.0,
    };
    for (i, solid) in solids.iter().enumerate() {
        let faces: HashMap<Id<Face>, &FaceM> = face_map(m, i, solid)?
            .into_iter()
            .map(|(f, id)| (id, f))
            .collect();
        let mesh = tessellate(
            solid,
            &TessellationOptions {
                chord_tolerance: chord,
            },
        )
        .map_err(|e| format!("Netz: {e}"))?;
        let (lo, hi) = mesh
            .positions
            .iter()
            .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(*p), hi.max(*p))
            });
        let o = 0.5 * (lo + hi);
        for r in &mesh.face_ranges {
            let fm = faces[&r.face];
            let sign = if fm.same_sense { 1.0 } else { -1.0 };
            let start = r.first_index as usize;
            let end = start + r.index_count as usize;
            // Fußpunkt und Normale je Netzpunkt einmal; der letzte Fußpunkt dient als Startwert.
            let mut feet: HashMap<u32, (DVec3, DVec3)> = HashMap::new();
            let mut hint = None;
            for t in mesh.indices[start..end].as_chunks::<3>().0 {
                let k = t.map(|i| mesh.positions[i as usize]);
                out.kernel_volume += (k[0] - o).dot((k[1] - o).cross(k[2] - o)) / 6.0;
                let f = t.map(|i| {
                    *feet
                        .entry(i)
                        .or_insert_with(|| fm.surface.foot(mesh.positions[i as usize], &mut hint))
                });
                for (a, (b, _)) in k.iter().zip(&f) {
                    out.max_offset = out.max_offset.max(a.distance(*b));
                }
                let s = f.map(|(p, _)| p);
                let n = (s[1] - s[0]).cross(s[2] - s[0]);
                let ns = (f[0].1 + f[1].1 + f[2].1) * sign;
                let flip = n.length() > 0.0 && ns.length() >= NORMAL_MIN && n.dot(ns) < 0.0;
                let (a, b, c) = if flip {
                    out.flipped += 1;
                    (s[0], s[2], s[1])
                } else {
                    (s[0], s[1], s[2])
                };
                out.volume += (a - o).dot((b - o).cross(c - o)) / 6.0;
                out.area += 0.5 * n.length();
            }
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Tests**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step`
Expected: PASS (8 in `stage1`; Volumen und Fläche der ebenen Körper bis auf Rundung, `flipped == 0`).

- [ ] **Step 5: Mutationsproben (danach zurücksetzen)**

1. In `src/topology.rs` in `face` `let sense = logical(f.same_sense);` durch `let sense = logical(!f.same_sense);` ersetzen → `stage1` schlägt fehl („Außennormale … zeigt anders“).
2. In `edge_loop` `let dir = logical(he.forward);` durch `let dir = logical(true);` ersetzen → `stage1` schlägt fehl („Schleife nicht geschlossen“).

Beide Änderungen zurücknehmen (`git checkout -- crates/planora-step/src/topology.rs`) und `cargo test -p planora-step` erneut grün sehen.

- [ ] **Step 6: Stufenabschluss**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
git diff --stat 07bac2b -- crates/planora-kernel crates/planora-math crates/planora-mesh crates/planora-exchange crates/planora-feature crates/planora-project crates/planora-render crates/planora-sketch
```
Expected: alles grün, die `git diff`-Ausgabe leer.

```bash
git add crates/planora-step/src/file.rs crates/planora-step/src/lib.rs crates/planora-step/tests/common/mod.rs crates/planora-step/tests/common/check.rs crates/planora-step/tests/stage1.rs
git commit -m "step: Prüfungen aus den gelesenen Daten, Hilfsdatei, Stufentest 1

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---


## Stufe 2 – Gekrümmte Standardflächen: Zylinder, Kegel, Kugel, Torus, Ellipse; Nähte, Spitzen, Pole

### Task 7: Zylinder, Kugel, Torus und Ellipse; Nähte, Pole und Vollkreise

**Files:**
- Modify: `crates/planora-step/src/geometry.rs` (ganz ersetzen; neu sind die Zweige `Curve::Ellipse` in `curve` und `Surface::{Cylinder, Sphere, Torus}` in `surface`)
- Test: `crates/planora-step/tests/curved.rs`

**Interfaces:**
- Consumes: Task 3 (`geometry::{positive, with_numbers, placement, frame_placement, reject}`), Task 5–6 (`checked`, `Model`, `Crv`, `Surf`); `planora_kernel::boolean::BoolOp`, `Kernel::{make_cylinder, make_sphere, revolve, extrude, boolean, transform}`.
- Produces: `CYLINDRICAL_SURFACE('',#a,r)`, `SPHERICAL_SURFACE('',#a,r)`, `TOROIDAL_SURFACE('',#a,major,minor)` (alle Radien > 0, sonst benannte Ablehnung), `ELLIPSE('',#a,lang,kurz)` mit der längeren Halbachse in Bezugsrichtung (P7). Kegel und Freiform bleiben bis Task 8/10 benannt abgelehnt.

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/curved.rs`)

```rust
//! Zylinder, Kugel, Torus und Ellipse mit Nähten, Polen und Vollkreisen; Punktprobe auch
//! gespiegelt und gedreht (Teilprojekt 10, Task 7).

mod common;

use common::model::{Crv, Model, Surf};
use common::{checked, export};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Curve, Kernel, Profile, ProfileLoop, Segment, Solid};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

/// Lagen, in denen jeder Körper geprüft wird: wie gebaut, verschoben und gedreht, gespiegelt,
/// gespiegelt und gedreht.
fn poses() -> Vec<(&'static str, DMat4)> {
    vec![
        ("wie gebaut", DMat4::IDENTITY),
        (
            "gedreht",
            DMat4::from_translation(DVec3::new(5.0, -3.0, 7.0))
                * DMat4::from_rotation_x(0.9)
                * DMat4::from_rotation_z(0.4),
        ),
        ("gespiegelt", DMat4::from_scale(DVec3::new(1.0, -1.0, 1.0))),
        (
            "gespiegelt und gedreht",
            DMat4::from_rotation_y(-1.3) * DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        ),
    ]
}

/// Ring mit Torusfläche: Kette (0,0) → (8,0) → Halbkreis um (8,3) nach außen → (0,6).
fn torus_ring() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(8.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(8.0, 3.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 6.0),
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

/// Quader 10 × 10 × 4 mit einer nach innen gewölbten Bucht (Bogen im Uhrzeigersinn).
fn notched() -> Solid {
    let outer = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(10.0, 10.0),
            },
            Segment::Arc {
                center: DVec2::new(5.0, 10.0),
                sweep: -PI,
            },
            Segment::Line { to: DVec2::ZERO },
        ],
    };
    k().extrude(
        &Profile {
            outer,
            holes: vec![],
        },
        &Frame::WORLD,
        4.0,
    )
    .unwrap()
}

/// Rohr: Kreis r = 5 mit Loch r = 3, Höhe 8.
fn tube() -> Solid {
    k().extrude(
        &Profile {
            outer: ProfileLoop::circle(DVec2::ZERO, 5.0),
            holes: vec![ProfileLoop::circle(DVec2::ZERO, 3.0)],
        },
        &Frame::WORLD,
        8.0,
    )
    .unwrap()
}

/// Zylinder r = 5, h = 40, oben schräg abgeschnitten (Ellipse als Schnittkante).
fn slanted() -> Solid {
    let cylinder = k().make_cylinder(5.0, 40.0).unwrap();
    let block = k().make_box(DVec3::splat(200.0)).unwrap();
    let block = moved(
        &block,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 25.0))
            * DMat4::from_rotation_x(0.5)
            * DMat4::from_translation(DVec3::new(-100.0, -100.0, 0.0)),
    );
    k().boolean(&cylinder, &block, BoolOp::Difference).unwrap()
}

fn surfaces(m: &Model) -> Vec<&Surf> {
    m.shells()
        .iter()
        .flat_map(|s| s.faces.iter())
        .map(|f| &f.surface)
        .collect()
}

#[test]
fn every_curved_body_passes_all_checks_in_every_pose() {
    for (name, body) in [
        ("Zylinder", k().make_cylinder(4.0, 9.0).unwrap()),
        ("Kugel", k().make_sphere(6.0).unwrap()),
        ("Ring", torus_ring()),
        ("Bucht", notched()),
        ("Rohr", tube()),
        ("Schräg", slanted()),
    ] {
        for (pose, m) in poses() {
            let s = moved(&body, m);
            let (text, _) = export(&[(name, &s)]);
            let (_, stats) = std::panic::catch_unwind(|| checked(&text, &[&s]))
                .unwrap_or_else(|_| panic!("{name}, {pose}"));
            assert_eq!(stats.len(), 1, "{name}, {pose}");
        }
    }
}

#[test]
fn a_cylinder_has_one_seam_used_twice_and_full_circles_with_one_vertex() {
    let s = k().make_cylinder(4.0, 9.0).unwrap();
    let (text, _) = export(&[("Zylinder", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(
        (stats[0].faces, stats[0].edges, stats[0].vertices),
        (3, 3, 2)
    );
    let mantle = m.shells()[0]
        .faces
        .iter()
        .find(|f| matches!(f.surface, Surf::Cylinder { .. }))
        .unwrap();
    let line = m
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Crv::Line { .. }))
        .map(|(id, _)| *id)
        .unwrap();
    let uses: Vec<bool> = mantle.bounds[0]
        .edges
        .iter()
        .filter(|o| o.edge == line)
        .map(|o| o.forward)
        .collect();
    assert_eq!(uses.len(), 2, "die Naht zweimal in derselben Schleife");
    assert_ne!(uses[0], uses[1], "in entgegengesetzter Richtung");
    let circles: Vec<_> = m
        .edges
        .values()
        .filter(|e| matches!(e.curve, Crv::Circle { .. }))
        .collect();
    assert_eq!(circles.len(), 2);
    assert!(
        circles.iter().all(|e| e.start == e.end),
        "Vollkreis: ein Eckpunkt"
    );
}

#[test]
fn a_sphere_is_one_face_with_poles_as_vertices() {
    let s = k().make_sphere(6.0).unwrap();
    let (text, _) = export(&[("Kugel", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(
        (stats[0].faces, stats[0].edges, stats[0].vertices),
        (1, 1, 2)
    );
    assert!(matches!(surfaces(&m)[..], [Surf::Sphere { r, .. }] if *r == 6.0));
    let poles: Vec<DVec3> = m.vertices.values().copied().collect();
    for z in [-6.0, 6.0] {
        let pole = DVec3::new(0.0, 0.0, z);
        assert!(
            poles.iter().any(|p| p.distance(pole) < 1e-12),
            "Pol {pole} fehlt: {poles:?}"
        );
    }
    let face = &m.shells()[0].faces[0];
    assert_eq!(face.bounds.len(), 1);
    let b = &face.bounds[0];
    assert_eq!(b.edges.len(), 2);
    assert_eq!(b.edges[0].edge, b.edges[1].edge, "Meridian hin und zurück");
    assert_ne!(b.edges[0].forward, b.edges[1].forward);
}

#[test]
fn torus_rings_keep_both_radii() {
    let s = torus_ring();
    let (text, _) = export(&[("Ring", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let tori: Vec<(f64, f64)> = surfaces(&m)
        .iter()
        .filter_map(|s| match s {
            Surf::Torus { major, minor, .. } => Some((*major, *minor)),
            _ => None,
        })
        .collect();
    assert_eq!(tori, vec![(8.0, 3.0)]);
}

#[test]
fn arcs_running_clockwise_are_edges_against_the_curve() {
    let s = notched();
    let (text, _) = export(&[("Bucht", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let backwards = m
        .edges
        .values()
        .filter(|e| matches!(e.curve, Crv::Circle { .. }) && !e.same_sense)
        .count();
    assert_eq!(backwards, 2, "Bogen unten und oben laufen gegen den Kreis");
    let inner = m.shells()[0]
        .faces
        .iter()
        .filter(|f| matches!(f.surface, Surf::Cylinder { .. }))
        .count();
    assert_eq!(inner, 1);
}

#[test]
fn an_oblique_cut_gives_an_ellipse_with_the_long_axis_first() {
    let s = slanted();
    let (text, _) = export(&[("Schräg", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let ellipses: Vec<(f64, f64)> = m
        .edges
        .values()
        .filter_map(|e| match e.curve {
            Crv::Ellipse { r1, r2, .. } => Some((r1, r2)),
            _ => None,
        })
        .collect();
    assert_eq!(ellipses.len(), 1, "{ellipses:?}");
    assert!((ellipses[0].0 - 5.0 / 0.5f64.cos()).abs() < 1e-9);
    assert!((ellipses[0].1 - 5.0).abs() < 1e-9);
}

/// Dieselbe Ellipse mit vertauschten Halbachsen (kurze Achse in x, Parameter um +π/2 versetzt).
fn swap_ellipses(s: &Solid) -> Solid {
    let mut out = s.clone();
    let ids: Vec<_> = out.edges.iter().map(|(id, _)| id).collect();
    for id in ids {
        let e = &mut out.edges[id];
        if let Curve::Ellipse {
            frame,
            major,
            minor,
        } = e.curve
        {
            e.curve = Curve::Ellipse {
                frame: Frame {
                    x: -frame.y,
                    y: frame.x,
                    ..frame
                },
                major: minor,
                minor: major,
            };
            e.t0 += PI / 2.0;
            e.t1 += PI / 2.0;
        }
    }
    out
}

#[test]
fn ellipses_with_the_short_axis_first_are_turned_around() {
    let s = swap_ellipses(&slanted());
    let swapped = s
        .edges
        .iter()
        .find_map(|(_, e)| match e.curve {
            Curve::Ellipse { major, minor, .. } => Some(minor > major),
            _ => None,
        })
        .unwrap();
    assert!(swapped, "die Probe hat die kurze Achse vorn");
    let (text, _) = export(&[("Schräg", &s)]);
    let (m, _) = checked(&text, &[&s]);
    for e in m.edges.values() {
        if let Crv::Ellipse { r1, r2, .. } = e.curve {
            assert!(r1 >= r2, "{r1} < {r2}");
        }
    }
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test curved`
Expected: FAIL – `write_step: Es gibt keinen Körper, der sich als STEP-Datei ausgeben lässt` (Zylinder werden noch abgelehnt).

- [ ] **Step 3: Implementieren** – `crates/planora-step/src/geometry.rs` ganz ersetzen:

```rust
//! Geometrie (Spec §3): Punkte, Richtungen, Koordinatensysteme, Kurven und Flächen.

use crate::entity::Entities;
use crate::error::StepError;
use crate::number::push_real;
use planora_kernel::{Curve, Surface};
use planora_math::{DVec3, Frame};

/// Abweichung, bis zu der ein Koordinatensystem des Kernels als senkrecht und normiert gilt.
/// Die Systeme des Kernels weichen um Rundungsreste (etwa 1e-15) ab; schief gebaute Systeme
/// werden benannt abgelehnt statt falsch geschrieben.
pub const FRAME_TOL: f64 = 1e-9;

/// Ablehnung eines Körpers ohne Namen; [`crate::check_body`] setzt den Namen ein.
pub(crate) fn reject(reason: &str) -> StepError {
    StepError::InvalidBody {
        name: String::new(),
        reason: reason.to_string(),
    }
}

/// Hängt `(x,y,z)` an.
fn push_triple(s: &mut String, v: DVec3) -> Result<(), StepError> {
    s.push('(');
    push_real(s, v.x)?;
    s.push(',');
    push_real(s, v.y)?;
    s.push(',');
    push_real(s, v.z)?;
    s.push(')');
    Ok(())
}

pub(crate) fn point(e: &mut Entities, p: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("CARTESIAN_POINT('',");
    push_triple(&mut s, p)?;
    s.push(')');
    e.push(&s)
}

pub(crate) fn direction(e: &mut Entities, d: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("DIRECTION('',");
    push_triple(&mut s, d)?;
    s.push(')');
    e.push(&s)
}

/// `AXIS2_PLACEMENT_3D` mit Ursprung, z-Achse (`axis`) und x-Achse (`ref_direction`).
pub(crate) fn placement(
    e: &mut Entities,
    origin: DVec3,
    z: DVec3,
    x: DVec3,
) -> Result<u64, StepError> {
    let p = point(e, origin)?;
    let a = direction(e, z)?;
    let r = direction(e, x)?;
    e.push(&format!("AXIS2_PLACEMENT_3D('',#{p},#{a},#{r})"))
}

/// Koordinatensystem des Kernels; schiefe oder nicht normierte Systeme werden abgelehnt.
pub(crate) fn frame_placement(e: &mut Entities, f: &Frame) -> Result<u64, StepError> {
    if !f.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    placement(e, f.origin, f.z, f.x)
}

/// Eintrag `NAME('',#placement,zahl,…)`.
fn with_numbers(
    e: &mut Entities,
    name: &str,
    placement: u64,
    numbers: &[f64],
) -> Result<u64, StepError> {
    let mut s = format!("{name}('',#{placement}");
    for x in numbers {
        s.push(',');
        push_real(&mut s, *x)?;
    }
    s.push(')');
    e.push(&s)
}

/// Prüft eine Länge, die in STEP positiv sein muss (Radius, Halbachse).
fn positive(x: f64, what: &str) -> Result<f64, StepError> {
    if !x.is_finite() {
        return Err(StepError::NotFinite);
    }
    if x > 0.0 {
        Ok(x)
    } else {
        Err(reject(&format!("{what} ist nicht größer als null")))
    }
}

/// Trägerkurve einer Kante.
pub(crate) fn curve(e: &mut Entities, c: &Curve) -> Result<u64, StepError> {
    match c {
        Curve::Line { origin, dir } => {
            let p = point(e, *origin)?;
            let d = direction(e, *dir)?;
            let v = e.push(&format!("VECTOR('',#{d},1.)"))?;
            e.push(&format!("LINE('',#{p},#{v})"))
        }
        Curve::Circle { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Kreises")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CIRCLE", a, &[r])
        }
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => {
            let (a, b) = (
                positive(*major, "Eine Halbachse einer Ellipse")?,
                positive(*minor, "Eine Halbachse einer Ellipse")?,
            );
            if b > a {
                // STEP legt die erste Halbachse in die Bezugsrichtung; die längere kommt dorthin
                // (x' = y, y' = −x, derselbe Umlaufsinn um z). Der Parameter verschiebt sich um
                // π/2; die Kante wird über ihre Eckpunkte begrenzt.
                if !frame.is_orthonormal(FRAME_TOL) {
                    return Err(reject(
                        "Ein Koordinatensystem ist schief oder nicht normiert",
                    ));
                }
                let p = placement(e, frame.origin, frame.z, frame.y)?;
                with_numbers(e, "ELLIPSE", p, &[b, a])
            } else {
                let p = frame_placement(e, frame)?;
                with_numbers(e, "ELLIPSE", p, &[a, b])
            }
        }
        Curve::Nurbs(_) => Err(reject("Diese Kantenart wird noch nicht ausgegeben")),
    }
}

/// Trägerfläche einer Fläche.
pub(crate) fn surface(e: &mut Entities, s: &Surface) -> Result<u64, StepError> {
    match s {
        Surface::Plane { frame } => {
            let a = frame_placement(e, frame)?;
            e.push(&format!("PLANE('',#{a})"))
        }
        Surface::Cylinder { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Zylinders")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CYLINDRICAL_SURFACE", a, &[r])
        }
        Surface::Sphere { frame, radius } => {
            let r = positive(*radius, "Der Radius einer Kugel")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "SPHERICAL_SURFACE", a, &[r])
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let big = positive(*major, "Der große Radius eines Torus")?;
            let small = positive(*minor, "Der kleine Radius eines Torus")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "TOROIDAL_SURFACE", a, &[big, small])
        }
        Surface::Cone { .. } | Surface::Nurbs(_) => {
            Err(reject("Diese Flächenart wird noch nicht ausgegeben"))
        }
    }
}
```

- [ ] **Step 4: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (7 in `curved`). Hinweis zum Test `ellipses_with_the_short_axis_first_are_turned_around`: Die Hilfsfunktion `swap_ellipses` baut dieselbe Ellipse mit Bezugsrichtung `−y` und verschiebt den Parameter um **+π/2** (beim Planen zuerst falsch mit −π/2 geschrieben; der Kernelvergleich meldete „läuft in STEP andersherum“ – die Richtungsprüfung greift also).

Mutationsprobe: im Zweig `Curve::Ellipse` die Bedingung `if b > a {` durch `if false {` ersetzen → `ellipses_with_the_short_axis_first_are_turned_around` schlägt fehl (`r1 < r2`). Zurücknehmen.

```bash
git add crates/planora-step/src/geometry.rs crates/planora-step/tests/curved.rs
git commit -m "step: Zylinder, Kugel, Torus und Ellipse mit Nähten, Polen und Vollkreisen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Kegel mit Vorzeichenregeln

**Files:**
- Modify: `crates/planora-step/src/geometry.rs` (ganz ersetzen; neu: `fn cone`, `surface(e, s, patch)` mit Parameterbereich, `UvRect` im Import), `crates/planora-step/src/topology.rs` (ganz ersetzen; einzige Änderung: `geometry::surface(e, &f.surface, f.patch)` in `TopoWriter::face`)
- Test: `crates/planora-step/tests/cone.rs`

**Interfaces:**
- Consumes: Task 7; `planora_kernel::{Surface::Cone { frame, radius, slope }, UvRect}`, `Face::patch: Option<UvRect>`.
- Produces: `pub(crate) fn surface(e: &mut Entities, s: &Surface, patch: Option<UvRect>) -> Result<u64, StepError>`; `CONICAL_SURFACE('',#a,ρ,atan(|slope|))` mit Lage auf der Mitte des v-Bereichs und umgekehrter Achse bei fallendem Radius (P6); `slope = 0` → `CYLINDRICAL_SURFACE`; Ablehnung „Eine Kegelfläche liegt jenseits ihrer Spitze“ bzw. „Eine Kegelfläche hat keinen Parameterbereich“.

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/cone.rs`)

```rust
//! Kegel mit allen Vorzeichen (Radius am Ursprung negativ, null oder positiv, Steigung steigend
//! oder fallend), Spitze als Eckpunkt, gespiegelt und gedreht (Teilprojekt 10, Task 8).

mod common;

use common::model::Surf;
use common::{checked, export};
use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, Kernel, ProfileLoop, Segment, Solid, Surface};
use planora_math::{DMat4, DVec2, DVec3, Frame};
use planora_step::{StepBody, StepError, check_body};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Drehkörper aus einer Kette über die Punkte (r, z), die auf der Achse beginnt und endet.
fn revolved(points: &[(f64, f64)]) -> Solid {
    let pts: Vec<DVec2> = points.iter().map(|&(r, z)| DVec2::new(r, z)).collect();
    k().revolve(
        &ProfileLoop {
            start: pts[0],
            segments: pts[1..].iter().map(|&to| Segment::Line { to }).collect(),
        },
        &Frame::WORLD,
    )
    .unwrap()
}

/// (Name, Körper, Vorzeichen von Radius am Ursprung und Steigung im Kernel).
fn cones() -> Vec<(&'static str, Solid, f64, f64)> {
    vec![
        // Spitze oben, Radius fällt: radius 6 > 0, slope < 0.
        ("Kegel", k().make_cone(6.0, 9.0).unwrap(), 1.0, -1.0),
        // Kegelstumpf über dem Ursprung, Radius wächst: radius = 2 − 4 · 10 < 0, slope 4.
        (
            "Stumpf steigend",
            revolved(&[(0.0, 10.0), (2.0, 10.0), (10.0, 12.0), (0.0, 12.0)]),
            -1.0,
            1.0,
        ),
        // Unter dem Ursprung, Radius fällt nach oben: radius = 10 − 20 < 0, slope −1.
        (
            "Stumpf unten",
            revolved(&[(0.0, -20.0), (10.0, -20.0), (5.0, -15.0), (0.0, -15.0)]),
            -1.0,
            -1.0,
        ),
        // Spitze genau im Ursprung des Kernels: radius 0, slope 1.
        (
            "Trichter",
            revolved(&[(0.0, 0.0), (5.0, 5.0), (0.0, 5.0)]),
            0.0,
            1.0,
        ),
    ]
}

fn poses() -> Vec<(&'static str, DMat4)> {
    vec![
        ("wie gebaut", DMat4::IDENTITY),
        (
            "gedreht",
            DMat4::from_translation(DVec3::new(-4.0, 2.0, 1.0)) * DMat4::from_rotation_y(2.2),
        ),
        ("gespiegelt", DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0))),
        (
            "gespiegelt und gedreht",
            DMat4::from_rotation_z(0.8) * DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)),
        ),
    ]
}

fn kernel_cone(s: &Solid) -> (f64, f64) {
    s.faces
        .iter()
        .find_map(|(_, f)| match f.surface {
            Surface::Cone { radius, slope, .. } => Some((radius, slope)),
            _ => None,
        })
        .unwrap()
}

#[test]
fn the_test_bodies_cover_every_sign_of_radius_and_slope() {
    for (name, s, r_sign, s_sign) in cones() {
        let (radius, slope) = kernel_cone(&s);
        let sign = |x: f64| if x == 0.0 { 0.0 } else { x.signum() };
        assert_eq!((sign(radius), sign(slope)), (r_sign, s_sign), "{name}");
    }
}

#[test]
fn cones_of_every_sign_pass_all_checks_in_every_pose() {
    for (name, body, _, _) in cones() {
        for (pose, m) in poses() {
            let s = k().transform(&body, &m).unwrap();
            let (text, _) = export(&[(name, &s)]);
            let (model, _) = std::panic::catch_unwind(|| checked(&text, &[&s]))
                .unwrap_or_else(|_| panic!("{name}, {pose}"));
            let (_, slope) = kernel_cone(&s);
            for f in model.shells().iter().flat_map(|sh| sh.faces.iter()) {
                if let Surf::Cone { r, semi, .. } = f.surface {
                    assert!(r > 0.0, "{name}, {pose}: Radius {r}");
                    assert!((semi - slope.abs().atan()).abs() < 1e-15, "{name}, {pose}");
                }
            }
        }
    }
}

#[test]
fn the_apex_of_a_cone_is_a_vertex() {
    let s = k().make_cone(6.0, 9.0).unwrap();
    let (text, _) = export(&[("Kegel", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(
        (stats[0].faces, stats[0].edges, stats[0].vertices),
        (2, 2, 2)
    );
    assert!(
        m.vertices
            .values()
            .any(|p| p.distance(DVec3::new(0.0, 0.0, 9.0)) < 1e-12),
        "Spitze als Eckpunkt"
    );
}

#[test]
fn a_flat_cone_is_written_as_a_cylinder() {
    let mut s = k().make_cylinder(3.0, 5.0).unwrap();
    let id = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .unwrap()
        .0;
    if let Surface::Cylinder { frame, radius } = s.faces[id].surface {
        s.faces[id].surface = Surface::Cone {
            frame,
            radius,
            slope: 0.0,
        };
    }
    let (text, _) = export(&[("Flach", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let cylinders = m
        .shells()
        .iter()
        .flat_map(|sh| sh.faces.iter())
        .filter(|f| matches!(f.surface, Surf::Cylinder { r, .. } if r == 3.0))
        .count();
    assert_eq!(cylinders, 1);
}

#[test]
fn a_cone_face_beyond_its_apex_is_refused_by_name() {
    let mut s = revolved(&[(0.0, 10.0), (2.0, 10.0), (10.0, 12.0), (0.0, 12.0)]);
    let id = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cone { .. }))
        .unwrap()
        .0;
    if let Surface::Cone {
        frame,
        radius,
        slope,
    } = s.faces[id].surface
    {
        // Radius in der Mitte des Bereichs (z = 11) jetzt −1: die Fläche läge jenseits der
        // Spitze.
        s.faces[id].surface = Surface::Cone {
            frame,
            radius: radius - 7.0,
            slope,
        };
    }
    let err = check_body(
        &StepBody {
            name: "Jenseits".into(),
            solid: &s,
        },
        &Progress::new(),
    )
    .unwrap_err();
    match err {
        StepError::InvalidBody { name, reason } => {
            assert_eq!(name, "Jenseits");
            assert!(reason.contains("Spitze"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test cone`
Expected: FAIL – Kegel werden noch abgelehnt (`write_step: …` bzw. `a_cone_face_beyond_its_apex_is_refused_by_name` findet „noch nicht ausgegeben“ statt „Spitze“).

- [ ] **Step 3: Implementieren**

`crates/planora-step/src/geometry.rs` ganz ersetzen:

```rust
//! Geometrie (Spec §3): Punkte, Richtungen, Koordinatensysteme, Kurven und Flächen.

use crate::entity::Entities;
use crate::error::StepError;
use crate::number::push_real;
use planora_kernel::{Curve, Surface, UvRect};
use planora_math::{DVec3, Frame};

/// Abweichung, bis zu der ein Koordinatensystem des Kernels als senkrecht und normiert gilt.
/// Die Systeme des Kernels weichen um Rundungsreste (etwa 1e-15) ab; schief gebaute Systeme
/// werden benannt abgelehnt statt falsch geschrieben.
pub const FRAME_TOL: f64 = 1e-9;

/// Ablehnung eines Körpers ohne Namen; [`crate::check_body`] setzt den Namen ein.
pub(crate) fn reject(reason: &str) -> StepError {
    StepError::InvalidBody {
        name: String::new(),
        reason: reason.to_string(),
    }
}

/// Hängt `(x,y,z)` an.
fn push_triple(s: &mut String, v: DVec3) -> Result<(), StepError> {
    s.push('(');
    push_real(s, v.x)?;
    s.push(',');
    push_real(s, v.y)?;
    s.push(',');
    push_real(s, v.z)?;
    s.push(')');
    Ok(())
}

pub(crate) fn point(e: &mut Entities, p: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("CARTESIAN_POINT('',");
    push_triple(&mut s, p)?;
    s.push(')');
    e.push(&s)
}

pub(crate) fn direction(e: &mut Entities, d: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("DIRECTION('',");
    push_triple(&mut s, d)?;
    s.push(')');
    e.push(&s)
}

/// `AXIS2_PLACEMENT_3D` mit Ursprung, z-Achse (`axis`) und x-Achse (`ref_direction`).
pub(crate) fn placement(
    e: &mut Entities,
    origin: DVec3,
    z: DVec3,
    x: DVec3,
) -> Result<u64, StepError> {
    let p = point(e, origin)?;
    let a = direction(e, z)?;
    let r = direction(e, x)?;
    e.push(&format!("AXIS2_PLACEMENT_3D('',#{p},#{a},#{r})"))
}

/// Koordinatensystem des Kernels; schiefe oder nicht normierte Systeme werden abgelehnt.
pub(crate) fn frame_placement(e: &mut Entities, f: &Frame) -> Result<u64, StepError> {
    if !f.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    placement(e, f.origin, f.z, f.x)
}

/// Eintrag `NAME('',#placement,zahl,…)`.
fn with_numbers(
    e: &mut Entities,
    name: &str,
    placement: u64,
    numbers: &[f64],
) -> Result<u64, StepError> {
    let mut s = format!("{name}('',#{placement}");
    for x in numbers {
        s.push(',');
        push_real(&mut s, *x)?;
    }
    s.push(')');
    e.push(&s)
}

/// Prüft eine Länge, die in STEP positiv sein muss (Radius, Halbachse).
fn positive(x: f64, what: &str) -> Result<f64, StepError> {
    if !x.is_finite() {
        return Err(StepError::NotFinite);
    }
    if x > 0.0 {
        Ok(x)
    } else {
        Err(reject(&format!("{what} ist nicht größer als null")))
    }
}

/// Trägerkurve einer Kante.
pub(crate) fn curve(e: &mut Entities, c: &Curve) -> Result<u64, StepError> {
    match c {
        Curve::Line { origin, dir } => {
            let p = point(e, *origin)?;
            let d = direction(e, *dir)?;
            let v = e.push(&format!("VECTOR('',#{d},1.)"))?;
            e.push(&format!("LINE('',#{p},#{v})"))
        }
        Curve::Circle { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Kreises")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CIRCLE", a, &[r])
        }
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => {
            let (a, b) = (
                positive(*major, "Eine Halbachse einer Ellipse")?,
                positive(*minor, "Eine Halbachse einer Ellipse")?,
            );
            if b > a {
                // STEP legt die erste Halbachse in die Bezugsrichtung; die längere kommt dorthin
                // (x' = y, y' = −x, derselbe Umlaufsinn um z). Der Parameter verschiebt sich um
                // π/2; die Kante wird über ihre Eckpunkte begrenzt.
                if !frame.is_orthonormal(FRAME_TOL) {
                    return Err(reject(
                        "Ein Koordinatensystem ist schief oder nicht normiert",
                    ));
                }
                let p = placement(e, frame.origin, frame.z, frame.y)?;
                with_numbers(e, "ELLIPSE", p, &[b, a])
            } else {
                let p = frame_placement(e, frame)?;
                with_numbers(e, "ELLIPSE", p, &[a, b])
            }
        }
        Curve::Nurbs(_) => Err(reject("Diese Kantenart wird noch nicht ausgegeben")),
    }
}

/// Kegel (Spec §3): Der Kernel beschreibt den Radius an der Höhe `v` als `radius + slope · v`;
/// STEP verlangt am Ursprung einen Radius ≥ 0 und einen halben Öffnungswinkel in (0, π/2). Die
/// Lage wird deshalb auf die Mitte des v-Bereichs der Fläche verschoben (dort ist der Radius der
/// Fläche positiv) und bei fallendem Radius (`slope < 0`) die Achse umgekehrt; die natürliche
/// Normale (von der Achse weg) bleibt dieselbe. `slope = 0` ist ein Zylinder.
fn cone(
    e: &mut Entities,
    frame: &Frame,
    radius: f64,
    slope: f64,
    patch: Option<UvRect>,
) -> Result<u64, StepError> {
    if !(radius.is_finite() && slope.is_finite()) {
        return Err(StepError::NotFinite);
    }
    if !frame.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    let Some(p) = patch else {
        return Err(reject("Eine Kegelfläche hat keinen Parameterbereich"));
    };
    let v_mid = 0.5 * (p.v0 + p.v1);
    if !v_mid.is_finite() {
        return Err(StepError::NotFinite);
    }
    let rho = radius + slope * v_mid;
    if rho.is_nan() || rho <= 0.0 {
        return Err(reject("Eine Kegelfläche liegt jenseits ihrer Spitze"));
    }
    let origin = frame.origin + frame.z * v_mid;
    if slope == 0.0 {
        let a = placement(e, origin, frame.z, frame.x)?;
        return with_numbers(e, "CYLINDRICAL_SURFACE", a, &[rho]);
    }
    let z = if slope > 0.0 { frame.z } else { -frame.z };
    let a = placement(e, origin, z, frame.x)?;
    with_numbers(e, "CONICAL_SURFACE", a, &[rho, slope.abs().atan()])
}

/// Trägerfläche einer Fläche; `patch` ist der Parameterbereich der Fläche (für den Kegel).
pub(crate) fn surface(
    e: &mut Entities,
    s: &Surface,
    patch: Option<UvRect>,
) -> Result<u64, StepError> {
    match s {
        Surface::Plane { frame } => {
            let a = frame_placement(e, frame)?;
            e.push(&format!("PLANE('',#{a})"))
        }
        Surface::Cylinder { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Zylinders")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CYLINDRICAL_SURFACE", a, &[r])
        }
        Surface::Sphere { frame, radius } => {
            let r = positive(*radius, "Der Radius einer Kugel")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "SPHERICAL_SURFACE", a, &[r])
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let big = positive(*major, "Der große Radius eines Torus")?;
            let small = positive(*minor, "Der kleine Radius eines Torus")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "TOROIDAL_SURFACE", a, &[big, small])
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => cone(e, frame, *radius, *slope, patch),
        Surface::Nurbs(_) => Err(reject("Diese Flächenart wird noch nicht ausgegeben")),
    }
}
```

`crates/planora-step/src/topology.rs` ganz ersetzen:

```rust
//! Topologie (Spec §3): Körper, Hüllen, Flächen, Schleifen, Kanten und Eckpunkte. Jede Kante
//! und jeder Eckpunkt wird je Körper einmal geschrieben und von allen Flächen gemeinsam benutzt
//! (Nähte: ein `EDGE_CURVE`, zwei `ORIENTED_EDGE` mit entgegengesetzter Richtung).

use crate::entity::{Entities, logical, refs};
use crate::error::StepError;
use crate::geometry::{self, reject};
use planora_kernel::{Edge, Face, Id, Loop, Solid, Vertex};

/// Ergebnis eines geschriebenen Körpers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WrittenSolid {
    /// Darstellungselemente für `ADVANCED_BREP_SHAPE_REPRESENTATION`.
    pub items: Vec<u64>,
    pub faces: usize,
    pub edges: usize,
}

/// Verweisen alle Schleifen, Kanten und Schalen auf vorhandene Teile? Sonst würde die Prüfung
/// des Kernels beim Zugriff abbrechen.
pub(crate) fn ids_in_range(solid: &Solid) -> bool {
    let (nv, ne, nf) = (solid.vertices.len(), solid.edges.len(), solid.faces.len());
    let edges_ok = solid
        .edges
        .iter()
        .all(|(_, e)| e.start.index() < nv && e.end.index() < nv);
    let loops_ok = solid.faces.iter().all(|(_, f)| {
        std::iter::once(&f.outer)
            .chain(f.inner.iter())
            .all(|lp| lp.half_edges.iter().all(|he| he.edge.index() < ne))
    });
    let shells_ok = solid
        .shells
        .iter()
        .all(|s| s.faces.iter().all(|f| f.index() < nf));
    edges_ok && loops_ok && shells_ok
}

/// Flächen der Schalen (ohne Schalenliste: alle Flächen als eine Schale, wie im Kernel).
pub(crate) fn shell_faces(solid: &Solid) -> Vec<Vec<Id<Face>>> {
    if solid.shells.is_empty() {
        vec![solid.faces.iter().map(|(id, _)| id).collect()]
    } else {
        solid.shells.iter().map(|s| s.faces.clone()).collect()
    }
}

/// Schreibt Teile eines Körpers; merkt sich schon geschriebene Kanten und Eckpunkte.
struct TopoWriter<'s> {
    solid: &'s Solid,
    vertices: Vec<Option<u64>>,
    edges: Vec<Option<u64>>,
    edge_count: usize,
}

impl<'s> TopoWriter<'s> {
    fn new(solid: &'s Solid) -> TopoWriter<'s> {
        TopoWriter {
            solid,
            vertices: vec![None; solid.vertices.len()],
            edges: vec![None; solid.edges.len()],
            edge_count: 0,
        }
    }

    fn vertex(&mut self, e: &mut Entities, id: Id<Vertex>) -> Result<u64, StepError> {
        if let Some(n) = self.vertices[id.index()] {
            return Ok(n);
        }
        let p = geometry::point(e, self.solid.vertices[id].point)?;
        let n = e.push(&format!("VERTEX_POINT('',#{p})"))?;
        self.vertices[id.index()] = Some(n);
        Ok(n)
    }

    /// `EDGE_CURVE` vom Anfangs- zum Endpunkt; läuft die Kante gegen den Kurvenparameter
    /// (`t1 < t0`), ist `same_sense` falsch.
    fn edge(&mut self, e: &mut Entities, id: Id<Edge>) -> Result<u64, StepError> {
        if let Some(n) = self.edges[id.index()] {
            return Ok(n);
        }
        let edge = &self.solid.edges[id];
        let start = self.vertex(e, edge.start)?;
        let end = self.vertex(e, edge.end)?;
        let curve = geometry::curve(e, &edge.curve)?;
        let sense = logical(edge.t1 >= edge.t0);
        let n = e.push(&format!("EDGE_CURVE('',#{start},#{end},#{curve},{sense})"))?;
        self.edges[id.index()] = Some(n);
        self.edge_count += 1;
        Ok(n)
    }

    fn edge_loop(&mut self, e: &mut Entities, lp: &Loop) -> Result<u64, StepError> {
        let mut oriented = Vec::with_capacity(lp.half_edges.len());
        for he in &lp.half_edges {
            let ec = self.edge(e, he.edge)?;
            let dir = logical(he.forward);
            oriented.push(e.push(&format!("ORIENTED_EDGE('',*,*,#{ec},{dir})"))?);
        }
        e.push(&format!("EDGE_LOOP('',{})", refs(&oriented)))
    }

    /// `ADVANCED_FACE`; die äußere Schleife wird `FACE_OUTER_BOUND`, Löcher `FACE_BOUND`.
    fn face(&mut self, e: &mut Entities, id: Id<Face>) -> Result<u64, StepError> {
        let f = &self.solid.faces[id];
        let surface = geometry::surface(e, &f.surface, f.patch)?;
        let mut bounds = Vec::with_capacity(1 + f.inner.len());
        let outer = self.edge_loop(e, &f.outer)?;
        bounds.push(e.push(&format!("FACE_OUTER_BOUND('',#{outer},.T.)"))?);
        for lp in &f.inner {
            let inner = self.edge_loop(e, lp)?;
            bounds.push(e.push(&format!("FACE_BOUND('',#{inner},.T.)"))?);
        }
        let sense = logical(f.same_sense);
        e.push(&format!(
            "ADVANCED_FACE('',{},#{surface},{sense})",
            refs(&bounds)
        ))
    }

    fn closed_shell(&mut self, e: &mut Entities, faces: &[Id<Face>]) -> Result<u64, StepError> {
        let mut ids = Vec::with_capacity(faces.len());
        for &f in faces {
            ids.push(self.face(e, f)?);
        }
        e.push(&format!("CLOSED_SHELL('',{})", refs(&ids)))
    }
}

/// Schreibt den Körper als `MANIFOLD_SOLID_BREP` mit dem Namen `name` (schon als
/// STEP-Zeichenkette).
pub(crate) fn write_solid(
    e: &mut Entities,
    name: &str,
    solid: &Solid,
) -> Result<WrittenSolid, StepError> {
    let shells = shell_faces(solid);
    if shells.len() != 1 {
        return Err(reject(
            "Körper mit mehreren Schalen werden noch nicht ausgegeben",
        ));
    }
    let mut w = TopoWriter::new(solid);
    let shell = w.closed_shell(e, &shells[0])?;
    let brep = e.push(&format!("MANIFOLD_SOLID_BREP({name},#{shell})"))?;
    Ok(WrittenSolid {
        items: vec![brep],
        faces: shells[0].len(),
        edges: w.edge_count,
    })
}
```

- [ ] **Step 4: Tests**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step`
Expected: PASS (5 in `cone`; `curved`, `stage1` und die anderen weiter grün).

- [ ] **Step 5: Mutationsproben (danach zurücksetzen)**

1. In `cone` `let z = if slope > 0.0 { frame.z } else { -frame.z };` durch `let z = frame.z;` ersetzen → `cone` schlägt fehl („Kante … liegt … neben der Fläche“).
2. `let origin = frame.origin + frame.z * v_mid;` durch `let origin = frame.origin; let rho = radius;` ersetzen → `cone` schlägt fehl („Lesen: … Kegelradius negativ“).

Zurücknehmen und erneut grün sehen.

- [ ] **Step 6: Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`

```bash
git add crates/planora-step/src/geometry.rs crates/planora-step/src/topology.rs crates/planora-step/tests/cone.rs
git commit -m "step: Kegel mit Lage auf der Flächenmitte und umgekehrter Achse bei fallendem Radius

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Stufentest 2 – Volumen und Fläche gekrümmter Körper aus den STEP-Flächen

**Files:**
- Test: `crates/planora-step/tests/stage2.rs`

**Interfaces:**
- Consumes: Task 6 (`measure`, `checked`), Task 7–8.
- Produces: Stufenabschluss 2. Schranken: Volumen gegen die Formel höchstens `SAG_FACTOR · CHORD · Oberfläche` mit `SAG_FACTOR = 2.0`, `CHORD = 0.002` (beim Planen gemessen: höchstens 1,1 bei der Kugel); Oberfläche relativ `AREA_REL = 1e-3` (gemessen höchstens 5,6e-4); gegen das Kernel-Netz derselben Teilung `MESH_MATCH_REL = 1e-9` (gemessen unter 2e-15).

- [ ] **Step 1: Test schreiben** (`crates/planora-step/tests/stage2.rs`)

```rust
//! Stufentest 2 (Teilprojekt 10, Task 9): Volumen und Fläche gekrümmter Körper aus den
//! gelesenen STEP-Flächen gegen die Formeln (Spec §5.4) und gegen das Netz des Kernels;
//! Körper mit Loch (Geschlecht 1).

mod common;

use common::check::measure;
use common::model::Model;
use common::{checked, export};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop, Segment, Solid};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};

/// Sehnentoleranz der Netze.
const CHORD: f64 = 0.002;
/// Volumenfehler eines Netzes gegen die Formel höchstens `SAG_FACTOR · CHORD · Oberfläche`: Jedes
/// Dreieck liegt bis zur Sehnentoleranz unter der Fläche, in der Mitte eines Dreiecks im
/// Breiten-/Längenraster bis zu zwei Sehnenhöhen (wie in Teilprojekt 9, Ruling R-1; gemessen
/// höchstens 1,1 bei der Kugel).
const SAG_FACTOR: f64 = 2.0;
/// Relativer Fehler der Oberfläche gegen die Formel (gemessen höchstens 5,6e-4 bei der Kugel).
const AREA_REL: f64 = 1e-3;
/// Schranke gegen das Kernel-Netz derselben Teilung: nur Rundung beim Auflegen der Punkte auf
/// die STEP-Flächen.
const MESH_MATCH_REL: f64 = 1e-9;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn torus_ring() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(8.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(8.0, 3.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 6.0),
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

fn tube() -> Solid {
    k().extrude(
        &Profile {
            outer: ProfileLoop::circle(DVec2::ZERO, 5.0),
            holes: vec![ProfileLoop::circle(DVec2::ZERO, 3.0)],
        },
        &Frame::WORLD,
        8.0,
    )
    .unwrap()
}

fn slanted() -> Solid {
    let cylinder = k().make_cylinder(5.0, 40.0).unwrap();
    let block = k()
        .transform(
            &k().make_box(DVec3::splat(200.0)).unwrap(),
            &(DMat4::from_translation(DVec3::new(0.0, 0.0, 25.0))
                * DMat4::from_rotation_x(0.5)
                * DMat4::from_translation(DVec3::new(-100.0, -100.0, 0.0))),
        )
        .unwrap();
    k().boolean(&cylinder, &block, BoolOp::Difference).unwrap()
}

/// Quader 20 × 20 × 10 mit senkrechter Bohrung r = 4 durch die Mitte.
fn drilled() -> Solid {
    let block = k().make_box(DVec3::new(20.0, 20.0, 10.0)).unwrap();
    let pin = k()
        .transform(
            &k().make_cylinder(4.0, 30.0).unwrap(),
            &DMat4::from_translation(DVec3::new(10.0, 10.0, -10.0)),
        )
        .unwrap();
    k().boolean(&block, &pin, BoolOp::Difference).unwrap()
}

/// (Name, Körper, Volumen, Oberfläche, Geschlecht) nach Formeln.
fn bodies() -> Vec<(&'static str, Solid, f64, f64, usize)> {
    let (r, h) = (4.0, 9.0);
    let (rc, hc) = (6.0, 9.0);
    let rs = 6.0;
    // Ring: Zylinder r 8, h 6 plus halbe Kreisscheibe r 3 um x = 8 gedreht (Pappus).
    let disc = PI * 9.0 / 2.0;
    let disc_x = 8.0 + 4.0 * 3.0 / (3.0 * PI);
    let arc_x = 8.0 + 2.0 * 3.0 / PI;
    let ring_v = PI * 64.0 * 6.0 + 2.0 * PI * disc_x * disc;
    let ring_a = 2.0 * PI * 64.0 + 2.0 * PI * arc_x * 3.0 * PI;
    // Schräg: Schnittebene durch die Achse bei z = 25, mittlere Höhe 25.
    let slant = 0.5f64;
    let slant_a = PI * 25.0 + PI * 25.0 / slant.cos() + 2.0 * PI * 5.0 * 25.0;
    vec![
        (
            "Zylinder",
            k().make_cylinder(r, h).unwrap(),
            PI * r * r * h,
            2.0 * PI * r * (r + h),
            0,
        ),
        (
            "Kegel",
            k().make_cone(rc, hc).unwrap(),
            PI * rc * rc * hc / 3.0,
            PI * rc * (rc + rc.hypot(hc)),
            0,
        ),
        (
            "Kugel",
            k().make_sphere(rs).unwrap(),
            4.0 / 3.0 * PI * rs.powi(3),
            4.0 * PI * rs * rs,
            0,
        ),
        ("Ring", torus_ring(), ring_v, ring_a, 0),
        (
            "Rohr",
            tube(),
            PI * 16.0 * 8.0,
            2.0 * PI * 16.0 + 2.0 * PI * 8.0 * 8.0,
            1,
        ),
        ("Schräg", slanted(), PI * 25.0 * 25.0, slant_a, 0),
        (
            "Bohrung",
            drilled(),
            4000.0 - PI * 16.0 * 10.0,
            2.0 * (400.0 - PI * 16.0) + 80.0 * 10.0 + 2.0 * PI * 4.0 * 10.0,
            1,
        ),
    ]
}

#[test]
fn curved_bodies_keep_volume_and_area_through_the_file() {
    for (name, s, volume, area, genus) in bodies() {
        let (text, _) = export(&[(name, &s)]);
        let (m, stats) = checked(&text, &[&s]);
        assert_eq!(
            stats.iter().map(|x| x.genus).sum::<usize>(),
            genus,
            "{name}"
        );
        let r = measure(&m, &[&s], CHORD).unwrap();
        assert_eq!(r.flipped, 0, "{name}");
        assert!(r.max_offset < 1e-9, "{name}: {}", r.max_offset);
        assert!(rel(r.volume, r.kernel_volume) < MESH_MATCH_REL, "{name}");
        assert!(
            (r.volume - volume).abs() <= SAG_FACTOR * CHORD * area,
            "{name}: Volumen {} statt {volume}",
            r.volume
        );
        assert!(
            rel(r.area, area) < AREA_REL,
            "{name}: Fläche {} statt {area}",
            r.area
        );
    }
}

#[test]
fn mirrored_curved_bodies_keep_a_positive_volume() {
    for (name, s, volume, area, _) in bodies() {
        let s = k()
            .transform(
                &s,
                &(DMat4::from_rotation_z(0.3) * DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0))),
            )
            .unwrap();
        let (text, _) = export(&[(name, &s)]);
        let m = Model::read(&text).unwrap();
        let r = measure(&m, &[&s], CHORD).unwrap();
        assert_eq!(r.flipped, 0, "{name}");
        assert!(
            (r.volume - volume).abs() <= SAG_FACTOR * CHORD * area,
            "{name}: {}",
            r.volume
        );
    }
}
```

- [ ] **Step 2: Tests** (dieser Test prüft Fertiges; er muss sofort grün sein)

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test stage2`
Expected: PASS (2 Tests, Debug-Build unter 1 s). Schlägt er fehl, ist das ein Befund an Task 7/8 (Ursache beheben, Schranken nicht ändern).

- [ ] **Step 3: Stufenabschluss**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
git diff --stat 07bac2b -- crates/planora-kernel crates/planora-math crates/planora-mesh crates/planora-exchange crates/planora-feature crates/planora-project crates/planora-render crates/planora-sketch
```
Expected: alles grün, `git diff` leer.

```bash
git add crates/planora-step/tests/stage2.rs
git commit -m "step: Stufentest 2 – Formeln für Zylinder, Kegel, Kugel, Ring, Rohr, Schrägschnitt, Bohrung

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---


## Stufe 3 – Freiformen und Sonderfälle: NURBS, Hohlräume, Körper aus Rundungen/Fasen/Booleans, Zufallsreihe

### Task 10: NURBS-Kurven und -Flächen (rational und nicht rational, geschlossen, mit Polen)

**Files:**
- Modify: `crates/planora-step/src/geometry.rs` (ganz ersetzen; neu: `knot_lists`, `push_list`, `nurbs_curve`, `nurbs_surface`, Zweige `Curve::Nurbs` und `Surface::Nurbs`)
- Test: `crates/planora-step/tests/freeform.rs`

**Interfaces:**
- Consumes: Task 8; `planora_math::nurbs::{NurbsCurve, KnotVector}` (`degree()`, `knots().knots()`, `control_points()`, `weights()`, `is_closed(tol)`), `planora_kernel::NurbsSurfaceData` (`surface()`, `is_periodic_u()`, `is_periodic_v()`), `planora_math::nurbs::NurbsSurface` (`counts()`, `control_at(i, j)`, `weight_at(i, j)`, `degree_u/v()`, `knots_u/v()`).
- Produces: `B_SPLINE_CURVE_WITH_KNOTS('',p,(#…),.UNSPECIFIED.,closed,.F.,(m…),(k…),.UNSPECIFIED.)` bzw. der zusammengesetzte Eintrag `(BOUNDED_CURVE()B_SPLINE_CURVE(…)B_SPLINE_CURVE_WITH_KNOTS(…)CURVE()GEOMETRIC_REPRESENTATION_ITEM()RATIONAL_B_SPLINE_CURVE((w…))REPRESENTATION_ITEM(''))`; für Flächen `B_SPLINE_SURFACE_WITH_KNOTS('',du,dv,((#…),…),.UNSPECIFIED.,uc,vc,.F.,(mu),(mv),(ku),(kv),.UNSPECIFIED.)` bzw. `(BOUNDED_SURFACE()B_SPLINE_SURFACE(…)B_SPLINE_SURFACE_WITH_KNOTS(…)GEOMETRIC_REPRESENTATION_ITEM()RATIONAL_B_SPLINE_SURFACE(((w…),…))REPRESENTATION_ITEM('')SURFACE())` (P8). **Reihenfolge der Knotenlisten bei Flächen: erst beide Vielfachheiten, dann beide Wertelisten** (beim Planen zuerst falsch zusammengehängt; der eigene Leser meldete „ganze Zahl erwartet, gefunden Real“).

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/freeform.rs`)

```rust
//! Freiformkurven und -flächen: rational und nicht rational, geschlossen, mit Polen; Knoten mit
//! Vielfachheiten; Punktprobe und Volumen aus den gelesenen B-Splines (eigenes de Boor)
//! (Teilprojekt 10, Task 10).

mod common;

use common::check::measure;
use common::model::{Crv, Model, Surf};
use common::{checked, export};
use planora_kernel::{
    AnalyticKernel, Curve, Kernel, Profile, ProfileLoop, Segment, Solid, Surface, UvRect,
};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};
use std::sync::Arc;

/// Sehnentoleranz der Netze (Freiform: gröber, die Suche des Fußpunkts ist teuer).
const CHORD: f64 = 0.01;
/// Wie in `stage2.rs`: Volumenfehler höchstens `SAG_FACTOR · CHORD · Oberfläche`.
const SAG_FACTOR: f64 = 2.0;
/// Schranke gegen das Kernel-Netz derselben Teilung (Fußpunktsuche auf der Freiformfläche).
const MESH_MATCH_REL: f64 = 1e-7;
/// Teilung der Simpson-Summen in den Formeln.
const SIMPSON_STEPS: usize = 2000;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Simpson-Summe von `f` über `[a, b]`.
fn simpson(a: f64, b: f64, f: impl Fn(f64) -> f64) -> f64 {
    let n = SIMPSON_STEPS;
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        let w = if i % 2 == 1 { 4.0 } else { 2.0 };
        s += w * f(a + h * i as f64);
    }
    s * h / 3.0
}

/// Ellipse 6 × 3 als geschlossene rationale Kurve, extrudiert um 5.
fn ellipse_prism() -> Solid {
    let c =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 6.0, DVec3::Y * 3.0, 0.0, TAU).unwrap();
    k().extrude(
        &Profile {
            outer: ProfileLoop::nurbs_loop(c).unwrap(),
            holes: vec![],
        },
        &Frame::WORLD,
        5.0,
    )
    .unwrap()
}

/// Kubischer Spline durch (10,0), (12,5), (10,10) (nicht rational).
fn bulge() -> NurbsCurve {
    NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(12.0, 5.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ],
        &[0.0, 0.5, 1.0],
    )
    .unwrap()
}

/// Quadrat 10 × 10, rechte Seite als Spline, Höhe 4.
fn spline_prism() -> Solid {
    let outer = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Nurbs {
                curve: Arc::new(bulge()),
            },
            Segment::Line {
                to: DVec2::new(0.0, 10.0),
            },
            Segment::Line { to: DVec2::ZERO },
        ],
    };
    k().extrude(
        &Profile {
            outer,
            holes: vec![],
        },
        &Frame::WORLD,
        4.0,
    )
    .unwrap()
}

/// Spline in der (r, z)-Halbebene von (4, 0) über (5, 3) nach (3, 6).
fn meridian() -> NurbsCurve {
    NurbsCurve::natural_cubic(
        &[
            DVec3::new(4.0, 0.0, 0.0),
            DVec3::new(5.0, 3.0, 0.0),
            DVec3::new(3.0, 6.0, 0.0),
        ],
        &[0.0, 0.5, 1.0],
    )
    .unwrap()
}

/// Drehkörper: (0,0) → (4,0) → Spline → (3,6) → (0,6); die Mantelfläche ist eine rationale
/// Drehfläche.
fn vase() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(4.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(meridian()),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 6.0),
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

/// Kugel r = 5, deren Fläche eine NURBS-Drehfläche mit Polen ist (Kanten bleiben Kreise).
fn nurbs_sphere() -> Solid {
    let r = 5.0;
    let mut solid = k().make_sphere(r).unwrap();
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, -r),
        DVec3::new(r, 0.0, 0.0),
        0.0,
        PI,
    )
    .unwrap();
    let ns = NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, u1), (v0, v1)) = ns.domain();
    let surface = Surface::nurbs(ns, true, false).unwrap();
    let (id, f) = solid.faces.iter().next().unwrap();
    let p = DVec3::new(r * 0.6, r * 0.8, 0.0);
    let (ou, ov) = f.surface.project(p);
    let outward = f.surface.normal(ou, ov) * if f.same_sense { 1.0 } else { -1.0 };
    let (nu, nv) = surface.project(p);
    let same_sense = surface.normal(nu, nv).dot(outward) > 0.0;
    let f = &mut solid.faces[id];
    f.surface = surface;
    f.same_sense = same_sense;
    f.patch = Some(UvRect { u0, u1, v0, v1 });
    solid
}

/// Fläche zwischen Spline und der Linie x = 10 (Spline rechts davon).
fn bulge_area() -> f64 {
    let c = bulge();
    // ∫ (x − 10) dy entlang des Splines.
    simpson(0.0, 1.0, |t| {
        let d = c.derivs(t, 1);
        (d[0].x - 10.0) * d[1].y
    })
}

/// Volumen der Vase: π ∫ r² dz über den Spline (die Strecken auf der Achse und die waagrechten
/// tragen nichts bei).
fn vase_volume() -> f64 {
    let c = meridian();
    PI * simpson(0.0, 1.0, |t| {
        let d = c.derivs(t, 1);
        d[0].x * d[0].x * d[1].y
    })
    .abs()
}

/// (Name, Körper, Volumen nach Formel).
fn bodies() -> Vec<(&'static str, Solid, f64)> {
    vec![
        ("Ellipse", ellipse_prism(), PI * 6.0 * 3.0 * 5.0),
        (
            "Spline-Prisma",
            spline_prism(),
            (100.0 + bulge_area()) * 4.0,
        ),
        ("Vase", vase(), vase_volume()),
        ("NURBS-Kugel", nurbs_sphere(), 4.0 / 3.0 * PI * 125.0),
    ]
}

fn poses() -> Vec<(&'static str, DMat4)> {
    vec![
        ("wie gebaut", DMat4::IDENTITY),
        (
            "gedreht",
            DMat4::from_translation(DVec3::new(3.0, 1.0, -2.0)) * DMat4::from_rotation_x(1.1),
        ),
        ("gespiegelt", DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0))),
    ]
}

#[test]
fn freeform_bodies_pass_all_checks_and_keep_their_volume() {
    for (name, body, volume) in bodies() {
        for (pose, m) in poses() {
            let s = k().transform(&body, &m).unwrap();
            let (text, _) = export(&[(name, &s)]);
            let (model, _) = std::panic::catch_unwind(|| checked(&text, &[&s]))
                .unwrap_or_else(|_| panic!("{name}, {pose}"));
            let r = measure(&model, &[&s], CHORD).unwrap();
            assert_eq!(r.flipped, 0, "{name}, {pose}");
            assert!(r.max_offset < 1e-7, "{name}, {pose}: {}", r.max_offset);
            assert!(
                ((r.volume - r.kernel_volume) / r.kernel_volume).abs() < MESH_MATCH_REL,
                "{name}, {pose}: {} gegen {}",
                r.volume,
                r.kernel_volume
            );
            assert!(
                (r.volume - volume).abs() <= SAG_FACTOR * CHORD * r.area,
                "{name}, {pose}: {} statt {volume}",
                r.volume
            );
        }
    }
}

#[test]
fn rational_and_plain_b_splines_use_the_right_entities() {
    let s = ellipse_prism();
    let (text, _) = export(&[("Ellipse", &s)]);
    assert!(text.contains("RATIONAL_B_SPLINE_CURVE("));
    assert!(text.contains("RATIONAL_B_SPLINE_SURFACE("));
    let m = Model::read(&text).unwrap();
    let closed: Vec<bool> = m
        .edges
        .values()
        .filter_map(|e| match &e.curve {
            Crv::Spline(c) => Some(c.closed),
            _ => None,
        })
        .collect();
    assert_eq!(closed, vec![true, true], "Ober- und Unterkante geschlossen");
    let wall = m.shells()[0]
        .faces
        .iter()
        .find_map(|f| match &f.surface {
            Surf::Spline(sp) => Some(sp.clone()),
            _ => None,
        })
        .unwrap();
    assert!(wall.u_closed && !wall.v_closed);
    let p = Model::read(&export(&[("Prisma", &spline_prism())]).0).unwrap();
    assert_eq!(
        p.file.count("B_SPLINE_CURVE_WITH_KNOTS"),
        2,
        "einfach, nicht rational"
    );
    assert_eq!(p.file.count("B_SPLINE_SURFACE_WITH_KNOTS"), 1);
    assert!(
        !export(&[("Prisma", &spline_prism())])
            .0
            .contains("RATIONAL")
    );
}

#[test]
fn knots_are_written_as_values_with_multiplicities() {
    let s = ellipse_prism();
    let (text, _) = export(&[("Ellipse", &s)]);
    let m = Model::read(&text).unwrap();
    let c = m
        .edges
        .values()
        .find_map(|e| match &e.curve {
            Crv::Spline(c) => Some(c.clone()),
            _ => None,
        })
        .unwrap();
    // Vollellipse aus vier Viertelbögen vom Grad 2: Knoten 0 (3×), 1/4, 1/2, 3/4 (je 2×), 1 (3×).
    let kernel = s
        .edges
        .iter()
        .find_map(|(_, e)| match &e.curve {
            Curve::Nurbs(n) => Some(n.knots().knots().to_vec()),
            _ => None,
        })
        .unwrap();
    assert_eq!(c.knots, kernel);
    assert_eq!(c.degree, 2);
    assert!(text.contains("(3,2,2,2,3)"), "Vielfachheiten");
}

#[test]
fn poles_of_a_freeform_sphere_are_vertices_with_a_seam_used_twice() {
    let s = nurbs_sphere();
    let (text, _) = export(&[("NURBS-Kugel", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(
        (stats[0].faces, stats[0].edges, stats[0].vertices),
        (1, 1, 2)
    );
    let face = &m.shells()[0].faces[0];
    assert!(face.surface.is_spline());
    let b = &face.bounds[0];
    assert_eq!(b.edges.len(), 2);
    assert_eq!(b.edges[0].edge, b.edges[1].edge);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test freeform`
Expected: FAIL – Freiformkörper werden noch abgelehnt (`write_step: Es gibt keinen Körper …`).

- [ ] **Step 3: Implementieren** – `crates/planora-step/src/geometry.rs` ganz ersetzen:

```rust
//! Geometrie (Spec §3): Punkte, Richtungen, Koordinatensysteme, Kurven und Flächen.

use crate::entity::{Entities, logical, refs};
use crate::error::StepError;
use crate::number::push_real;
use planora_kernel::{Curve, NurbsSurfaceData, Surface, UvRect};
use planora_math::nurbs::{KnotVector, NurbsCurve};
use planora_math::{DVec3, Frame, Tolerance};
use std::fmt::Write as _;

/// Abweichung, bis zu der ein Koordinatensystem des Kernels als senkrecht und normiert gilt.
/// Die Systeme des Kernels weichen um Rundungsreste (etwa 1e-15) ab; schief gebaute Systeme
/// werden benannt abgelehnt statt falsch geschrieben.
pub const FRAME_TOL: f64 = 1e-9;

/// Ablehnung eines Körpers ohne Namen; [`crate::check_body`] setzt den Namen ein.
pub(crate) fn reject(reason: &str) -> StepError {
    StepError::InvalidBody {
        name: String::new(),
        reason: reason.to_string(),
    }
}

/// Hängt `(x,y,z)` an.
fn push_triple(s: &mut String, v: DVec3) -> Result<(), StepError> {
    s.push('(');
    push_real(s, v.x)?;
    s.push(',');
    push_real(s, v.y)?;
    s.push(',');
    push_real(s, v.z)?;
    s.push(')');
    Ok(())
}

pub(crate) fn point(e: &mut Entities, p: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("CARTESIAN_POINT('',");
    push_triple(&mut s, p)?;
    s.push(')');
    e.push(&s)
}

pub(crate) fn direction(e: &mut Entities, d: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("DIRECTION('',");
    push_triple(&mut s, d)?;
    s.push(')');
    e.push(&s)
}

/// `AXIS2_PLACEMENT_3D` mit Ursprung, z-Achse (`axis`) und x-Achse (`ref_direction`).
pub(crate) fn placement(
    e: &mut Entities,
    origin: DVec3,
    z: DVec3,
    x: DVec3,
) -> Result<u64, StepError> {
    let p = point(e, origin)?;
    let a = direction(e, z)?;
    let r = direction(e, x)?;
    e.push(&format!("AXIS2_PLACEMENT_3D('',#{p},#{a},#{r})"))
}

/// Koordinatensystem des Kernels; schiefe oder nicht normierte Systeme werden abgelehnt.
pub(crate) fn frame_placement(e: &mut Entities, f: &Frame) -> Result<u64, StepError> {
    if !f.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    placement(e, f.origin, f.z, f.x)
}

/// Eintrag `NAME('',#placement,zahl,…)`.
fn with_numbers(
    e: &mut Entities,
    name: &str,
    placement: u64,
    numbers: &[f64],
) -> Result<u64, StepError> {
    let mut s = format!("{name}('',#{placement}");
    for x in numbers {
        s.push(',');
        push_real(&mut s, *x)?;
    }
    s.push(')');
    e.push(&s)
}

/// Prüft eine Länge, die in STEP positiv sein muss (Radius, Halbachse).
fn positive(x: f64, what: &str) -> Result<f64, StepError> {
    if !x.is_finite() {
        return Err(StepError::NotFinite);
    }
    if x > 0.0 {
        Ok(x)
    } else {
        Err(reject(&format!("{what} ist nicht größer als null")))
    }
}

/// Trägerkurve einer Kante.
pub(crate) fn curve(e: &mut Entities, c: &Curve) -> Result<u64, StepError> {
    match c {
        Curve::Line { origin, dir } => {
            let p = point(e, *origin)?;
            let d = direction(e, *dir)?;
            let v = e.push(&format!("VECTOR('',#{d},1.)"))?;
            e.push(&format!("LINE('',#{p},#{v})"))
        }
        Curve::Circle { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Kreises")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CIRCLE", a, &[r])
        }
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => {
            let (a, b) = (
                positive(*major, "Eine Halbachse einer Ellipse")?,
                positive(*minor, "Eine Halbachse einer Ellipse")?,
            );
            if b > a {
                // STEP legt die erste Halbachse in die Bezugsrichtung; die längere kommt dorthin
                // (x' = y, y' = −x, derselbe Umlaufsinn um z). Der Parameter verschiebt sich um
                // π/2; die Kante wird über ihre Eckpunkte begrenzt.
                if !frame.is_orthonormal(FRAME_TOL) {
                    return Err(reject(
                        "Ein Koordinatensystem ist schief oder nicht normiert",
                    ));
                }
                let p = placement(e, frame.origin, frame.z, frame.y)?;
                with_numbers(e, "ELLIPSE", p, &[b, a])
            } else {
                let p = frame_placement(e, frame)?;
                with_numbers(e, "ELLIPSE", p, &[a, b])
            }
        }
        Curve::Nurbs(c) => nurbs_curve(e, c),
    }
}

/// Knoten als verschiedene Werte mit Vielfachheiten (Spec §3): `((m1,m2,…), (k1,k2,…))`. Der
/// Kernel speichert gleiche Knoten bitgleich, daher genügt der genaue Vergleich.
fn knot_lists(knots: &KnotVector) -> Result<(String, String), StepError> {
    let mut values: Vec<f64> = Vec::new();
    let mut mults: Vec<usize> = Vec::new();
    for &k in knots.knots() {
        match values.last() {
            Some(&last) if last == k => {
                if let Some(m) = mults.last_mut() {
                    *m += 1;
                }
            }
            _ => {
                values.push(k);
                mults.push(1);
            }
        }
    }
    let mut m = String::from("(");
    for (i, n) in mults.iter().enumerate() {
        if i > 0 {
            m.push(',');
        }
        let _ = write!(m, "{n}");
    }
    m.push(')');
    let mut v = String::new();
    push_list(&mut v, &values)?;
    Ok((m, v))
}

/// Hängt `(x1,x2,…)` an.
fn push_list(s: &mut String, xs: &[f64]) -> Result<(), StepError> {
    s.push('(');
    for (i, x) in xs.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        push_real(s, *x)?;
    }
    s.push(')');
    Ok(())
}

/// Freiformkurve (Spec §3): `B_SPLINE_CURVE_WITH_KNOTS`; mit Gewichten ungleich 1 als
/// zusammengesetzter Eintrag mit `RATIONAL_B_SPLINE_CURVE` (Teile in alphabetischer Reihenfolge,
/// wie ISO 10303-21 sie verlangt). Geschlossene Kurven tragen das Kennzeichen `.T.`.
fn nurbs_curve(e: &mut Entities, c: &NurbsCurve) -> Result<u64, StepError> {
    let mut points = Vec::with_capacity(c.control_points().len());
    for p in c.control_points() {
        points.push(point(e, *p)?);
    }
    let degree = c.degree();
    let closed = logical(c.is_closed(Tolerance::default().linear));
    let (mults, values) = knot_lists(c.knots())?;
    let knots = format!("{mults},{values}");
    let cps = refs(&points);
    if c.weights().iter().all(|w| *w == 1.0) {
        return e.push(&format!(
            "B_SPLINE_CURVE_WITH_KNOTS('',{degree},{cps},.UNSPECIFIED.,{closed},.F.,{knots},.UNSPECIFIED.)"
        ));
    }
    let mut weights = String::new();
    push_list(&mut weights, c.weights())?;
    e.push(&format!(
        "(BOUNDED_CURVE()B_SPLINE_CURVE({degree},{cps},.UNSPECIFIED.,{closed},.F.)\
         B_SPLINE_CURVE_WITH_KNOTS({knots},.UNSPECIFIED.)CURVE()GEOMETRIC_REPRESENTATION_ITEM()\
         RATIONAL_B_SPLINE_CURVE({weights})REPRESENTATION_ITEM(''))"
    ))
}

/// Freiformfläche (Spec §3): Kontrollpunkte zeilenweise in u (`control[i][j]`), Kennzeichen
/// „geschlossen“ in u und v wie im Kernel; rational wie bei [`nurbs_curve`].
fn nurbs_surface(e: &mut Entities, d: &NurbsSurfaceData) -> Result<u64, StepError> {
    let s = d.surface();
    let (nu, nv) = s.counts();
    let mut rows = String::from("(");
    let mut weights = String::from("(");
    let mut rational = false;
    for i in 0..nu {
        let mut row = Vec::with_capacity(nv);
        let mut w = Vec::with_capacity(nv);
        for j in 0..nv {
            row.push(point(e, s.control_at(i, j))?);
            w.push(s.weight_at(i, j));
            rational |= s.weight_at(i, j) != 1.0;
        }
        if i > 0 {
            rows.push(',');
            weights.push(',');
        }
        rows.push_str(&refs(&row));
        push_list(&mut weights, &w)?;
    }
    rows.push(')');
    weights.push(')');
    let (du, dv) = (s.degree_u(), s.degree_v());
    let (uc, vc) = (logical(d.is_periodic_u()), logical(d.is_periodic_v()));
    let (mu, ku) = knot_lists(s.knots_u())?;
    let (mv, kv) = knot_lists(s.knots_v())?;
    let knots = format!("{mu},{mv},{ku},{kv}");
    if !rational {
        return e.push(&format!(
            "B_SPLINE_SURFACE_WITH_KNOTS('',{du},{dv},{rows},.UNSPECIFIED.,{uc},{vc},.F.,{knots},.UNSPECIFIED.)"
        ));
    }
    e.push(&format!(
        "(BOUNDED_SURFACE()B_SPLINE_SURFACE({du},{dv},{rows},.UNSPECIFIED.,{uc},{vc},.F.)\
         B_SPLINE_SURFACE_WITH_KNOTS({knots},.UNSPECIFIED.)GEOMETRIC_REPRESENTATION_ITEM()\
         RATIONAL_B_SPLINE_SURFACE({weights})REPRESENTATION_ITEM('')SURFACE())"
    ))
}

/// Kegel (Spec §3): Der Kernel beschreibt den Radius an der Höhe `v` als `radius + slope · v`;
/// STEP verlangt am Ursprung einen Radius ≥ 0 und einen halben Öffnungswinkel in (0, π/2). Die
/// Lage wird deshalb auf die Mitte des v-Bereichs der Fläche verschoben (dort ist der Radius der
/// Fläche positiv) und bei fallendem Radius (`slope < 0`) die Achse umgekehrt; die natürliche
/// Normale (von der Achse weg) bleibt dieselbe. `slope = 0` ist ein Zylinder.
fn cone(
    e: &mut Entities,
    frame: &Frame,
    radius: f64,
    slope: f64,
    patch: Option<UvRect>,
) -> Result<u64, StepError> {
    if !(radius.is_finite() && slope.is_finite()) {
        return Err(StepError::NotFinite);
    }
    if !frame.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    let Some(p) = patch else {
        return Err(reject("Eine Kegelfläche hat keinen Parameterbereich"));
    };
    let v_mid = 0.5 * (p.v0 + p.v1);
    if !v_mid.is_finite() {
        return Err(StepError::NotFinite);
    }
    let rho = radius + slope * v_mid;
    if rho.is_nan() || rho <= 0.0 {
        return Err(reject("Eine Kegelfläche liegt jenseits ihrer Spitze"));
    }
    let origin = frame.origin + frame.z * v_mid;
    if slope == 0.0 {
        let a = placement(e, origin, frame.z, frame.x)?;
        return with_numbers(e, "CYLINDRICAL_SURFACE", a, &[rho]);
    }
    let z = if slope > 0.0 { frame.z } else { -frame.z };
    let a = placement(e, origin, z, frame.x)?;
    with_numbers(e, "CONICAL_SURFACE", a, &[rho, slope.abs().atan()])
}

/// Trägerfläche einer Fläche; `patch` ist der Parameterbereich der Fläche (für den Kegel).
pub(crate) fn surface(
    e: &mut Entities,
    s: &Surface,
    patch: Option<UvRect>,
) -> Result<u64, StepError> {
    match s {
        Surface::Plane { frame } => {
            let a = frame_placement(e, frame)?;
            e.push(&format!("PLANE('',#{a})"))
        }
        Surface::Cylinder { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Zylinders")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CYLINDRICAL_SURFACE", a, &[r])
        }
        Surface::Sphere { frame, radius } => {
            let r = positive(*radius, "Der Radius einer Kugel")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "SPHERICAL_SURFACE", a, &[r])
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let big = positive(*major, "Der große Radius eines Torus")?;
            let small = positive(*minor, "Der kleine Radius eines Torus")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "TOROIDAL_SURFACE", a, &[big, small])
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => cone(e, frame, *radius, *slope, patch),
        Surface::Nurbs(d) => nurbs_surface(e, d),
    }
}
```

- [ ] **Step 4: Tests**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step`
Expected: PASS (4 in `freeform`, Debug etwa 4 s; die Fußpunktsuche auf den B-Splines ist der teure Teil).

- [ ] **Step 5: Mutationsproben (danach zurücksetzen)**

1. In `nurbs_surface` `if !rational {` durch `if true {` ersetzen → `rational_and_plain_b_splines_use_the_right_entities` und die Punktproben schlagen fehl.
2. `let (uc, vc) = (logical(d.is_periodic_u()), logical(d.is_periodic_v()));` durch `let (uc, vc) = (logical(true), logical(true));` ersetzen → „als geschlossen gekennzeichnet, ist es aber nicht“ bzw. `wall.u_closed && !wall.v_closed` schlägt fehl.

- [ ] **Step 6: Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`

```bash
git add crates/planora-step/src/geometry.rs crates/planora-step/tests/freeform.rs
git commit -m "step: NURBS-Kurven und -Flächen mit Knotenvielfachheiten, rational und geschlossen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Hohlräume und getrennte Stücke

**Files:**
- Modify: `crates/planora-step/Cargo.toml` (ganz ersetzen: `planora-mesh` wird normale Abhängigkeit), `crates/planora-step/src/topology.rs` (ganz ersetzen; neu: `TopoWriter::face(…, reversed)`, `closed_shell(…, reversed)`, `Lump`, `CLASSIFY_CHORD`, `INSIDE_WINDING`, `solid_angle`, `lumps`, `write_solid` für mehrere Schalen)
- Test: `crates/planora-step/tests/voids.rs`

**Interfaces:**
- Consumes: Task 10; `planora_mesh::{tessellate, TessellationOptions, Mesh::face_ranges}`, `Solid::shells`.
- Produces: `pub(crate) struct Lump { pub outer: usize, pub voids: Vec<usize> }`, `pub(crate) fn lumps(solid: &Solid) -> Result<Vec<Lump>, StepError>`, `pub const CLASSIFY_CHORD: f64 = 0.05;` (im privaten Modul), `write_solid` liefert je Stück ein Element (`MANIFOLD_SOLID_BREP` oder `BREP_WITH_VOIDS(name,#außen,(#ORIENTED_CLOSED_SHELL…))`); Ablehnungen „Die Schalen lassen sich nicht unterscheiden (…)“, „Eine Schale hat kein Volumen“, „Ein Hohlraum liegt in keinem Teil des Körpers“.

- [ ] **Step 1: Tests schreiben** (`crates/planora-step/tests/voids.rs`)

```rust
//! Körper mit mehreren Schalen: Hohlräume als `BREP_WITH_VOIDS`, getrennte Stücke als eigene
//! `MANIFOLD_SOLID_BREP` im selben Teil (Teilprojekt 10, Task 11).

mod common;

use common::check::measure;
use common::{checked, export};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Kernel, Solid};
use planora_math::{DMat4, DVec3, PI};

/// Sehnentoleranz der Netze.
const CHORD: f64 = 0.002;
/// Wie in `stage2.rs`: Volumenfehler höchstens `SAG_FACTOR · CHORD · Oberfläche`.
const SAG_FACTOR: f64 = 2.0;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn cube(size: f64) -> Solid {
    k().make_box(DVec3::splat(size)).unwrap()
}

/// Würfel 10 mit würfelförmigem Hohlraum 2 in der Mitte.
fn hollow_cube() -> Solid {
    let r = k()
        .boolean(
            &cube(10.0),
            &moved(&cube(2.0), DVec3::splat(4.0)),
            BoolOp::Difference,
        )
        .unwrap();
    assert_eq!(r.shells.len(), 2);
    r
}

/// Würfel 10 mit kugelförmigem Hohlraum r = 2 in der Mitte.
fn hollow_ball() -> Solid {
    let ball = moved(&k().make_sphere(2.0).unwrap(), DVec3::splat(5.0));
    let r = k().boolean(&cube(10.0), &ball, BoolOp::Difference).unwrap();
    assert_eq!(r.shells.len(), 2);
    r
}

/// Zwei getrennte Würfel (Vereinigung ohne Berührung).
fn two_cubes() -> Solid {
    let r = k()
        .boolean(
            &cube(4.0),
            &moved(&cube(3.0), DVec3::X * 10.0),
            BoolOp::Union,
        )
        .unwrap();
    assert_eq!(r.shells.len(), 2);
    r
}

/// Würfel 12 mit Hohlraum 8, darin ein freier Würfel 2: drei Schalen.
fn box_in_box() -> Solid {
    let shell = k()
        .boolean(
            &cube(12.0),
            &moved(&cube(8.0), DVec3::splat(2.0)),
            BoolOp::Difference,
        )
        .unwrap();
    let r = k()
        .boolean(&shell, &moved(&cube(2.0), DVec3::splat(5.0)), BoolOp::Union)
        .unwrap();
    assert_eq!(r.shells.len(), 3);
    r
}

#[test]
fn a_cavity_becomes_a_brep_with_voids_with_a_reversed_shell() {
    let s = hollow_cube();
    let (text, _) = export(&[("Hohl", &s)]);
    assert_eq!(text.matches("BREP_WITH_VOIDS(").count(), 1);
    assert_eq!(text.matches("ORIENTED_CLOSED_SHELL('',*,#").count(), 1);
    assert!(!text.contains("MANIFOLD_SOLID_BREP("));
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(stats.len(), 2);
    let solid = &m.products[0].solids[0];
    assert_eq!(solid.name, "Hohl");
    assert_eq!(solid.voids.len(), 1);
    assert!(!solid.voids[0].orientation);
    let r = measure(&m, &[&s], CHORD).unwrap();
    assert_eq!(r.flipped, 0);
    assert!((r.volume - 992.0).abs() < 1e-9, "{}", r.volume);
}

#[test]
fn a_spherical_cavity_keeps_the_volume() {
    let s = hollow_ball();
    for (pose, m) in [
        ("wie gebaut", DMat4::IDENTITY),
        ("gespiegelt", DMat4::from_scale(DVec3::new(1.0, -1.0, 1.0))),
    ] {
        let s = k().transform(&s, &m).unwrap();
        let (text, _) = export(&[("Kugelhohl", &s)]);
        let (model, _) = checked(&text, &[&s]);
        let r = measure(&model, &[&s], CHORD).unwrap();
        assert_eq!(r.flipped, 0, "{pose}");
        let want = 1000.0 - 4.0 / 3.0 * PI * 8.0;
        assert!(
            (r.volume - want).abs() <= SAG_FACTOR * CHORD * 4.0 * PI * 4.0,
            "{pose}: {} statt {want}",
            r.volume
        );
    }
}

#[test]
fn separate_pieces_are_several_solids_of_one_part() {
    let s = two_cubes();
    let (text, report) = export(&[("Zwei", &s)]);
    assert_eq!(report.bodies, 1);
    assert_eq!(text.matches("MANIFOLD_SOLID_BREP(").count(), 2);
    let (m, _) = checked(&text, &[&s]);
    assert_eq!(m.products.len(), 1);
    assert_eq!(m.products[0].solids.len(), 2);
    let r = measure(&m, &[&s], CHORD).unwrap();
    assert!((r.volume - 91.0).abs() < 1e-9, "{}", r.volume);
}

#[test]
fn a_piece_inside_a_cavity_is_its_own_solid_and_the_cavity_belongs_to_the_big_box() {
    let s = box_in_box();
    let (text, _) = export(&[("Schachtel", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(stats.len(), 3);
    let solids = &m.products[0].solids;
    assert_eq!(solids.len(), 2);
    let with_void = solids.iter().filter(|x| x.voids.len() == 1).count();
    assert_eq!(with_void, 1);
    let r = measure(&m, &[&s], CHORD).unwrap();
    assert!(
        (r.volume - (1728.0 - 512.0 + 8.0)).abs() < 1e-9,
        "{}",
        r.volume
    );
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test voids`
Expected: FAIL – `write_step: Es gibt keinen Körper …` (mehrere Schalen werden noch abgelehnt).

- [ ] **Step 3: Implementieren**

`crates/planora-step/Cargo.toml` ganz ersetzen:

```toml
[package]
name = "planora-step"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
planora-math.workspace = true
planora-kernel.workspace = true
planora-exchange.workspace = true
planora-mesh.workspace = true
thiserror.workspace = true

[lints]
workspace = true
```

`crates/planora-step/src/topology.rs` ganz ersetzen:

```rust
//! Topologie (Spec §3): Körper, Hüllen, Flächen, Schleifen, Kanten und Eckpunkte. Jede Kante
//! und jeder Eckpunkt wird je Körper einmal geschrieben und von allen Flächen gemeinsam benutzt
//! (Nähte: ein `EDGE_CURVE`, zwei `ORIENTED_EDGE` mit entgegengesetzter Richtung).

use crate::entity::{Entities, logical, refs};
use crate::error::StepError;
use crate::geometry::{self, reject};
use planora_kernel::{Edge, Face, Id, Loop, Solid, Vertex};
use planora_math::DVec3;
use planora_mesh::{TessellationOptions, tessellate};

/// Ergebnis eines geschriebenen Körpers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WrittenSolid {
    /// Darstellungselemente für `ADVANCED_BREP_SHAPE_REPRESENTATION`.
    pub items: Vec<u64>,
    pub faces: usize,
    pub edges: usize,
}

/// Verweisen alle Schleifen, Kanten und Schalen auf vorhandene Teile? Sonst würde die Prüfung
/// des Kernels beim Zugriff abbrechen.
pub(crate) fn ids_in_range(solid: &Solid) -> bool {
    let (nv, ne, nf) = (solid.vertices.len(), solid.edges.len(), solid.faces.len());
    let edges_ok = solid
        .edges
        .iter()
        .all(|(_, e)| e.start.index() < nv && e.end.index() < nv);
    let loops_ok = solid.faces.iter().all(|(_, f)| {
        std::iter::once(&f.outer)
            .chain(f.inner.iter())
            .all(|lp| lp.half_edges.iter().all(|he| he.edge.index() < ne))
    });
    let shells_ok = solid
        .shells
        .iter()
        .all(|s| s.faces.iter().all(|f| f.index() < nf));
    edges_ok && loops_ok && shells_ok
}

/// Flächen der Schalen (ohne Schalenliste: alle Flächen als eine Schale, wie im Kernel).
pub(crate) fn shell_faces(solid: &Solid) -> Vec<Vec<Id<Face>>> {
    if solid.shells.is_empty() {
        vec![solid.faces.iter().map(|(id, _)| id).collect()]
    } else {
        solid.shells.iter().map(|s| s.faces.clone()).collect()
    }
}

/// Schreibt Teile eines Körpers; merkt sich schon geschriebene Kanten und Eckpunkte.
struct TopoWriter<'s> {
    solid: &'s Solid,
    vertices: Vec<Option<u64>>,
    edges: Vec<Option<u64>>,
    edge_count: usize,
}

impl<'s> TopoWriter<'s> {
    fn new(solid: &'s Solid) -> TopoWriter<'s> {
        TopoWriter {
            solid,
            vertices: vec![None; solid.vertices.len()],
            edges: vec![None; solid.edges.len()],
            edge_count: 0,
        }
    }

    fn vertex(&mut self, e: &mut Entities, id: Id<Vertex>) -> Result<u64, StepError> {
        if let Some(n) = self.vertices[id.index()] {
            return Ok(n);
        }
        let p = geometry::point(e, self.solid.vertices[id].point)?;
        let n = e.push(&format!("VERTEX_POINT('',#{p})"))?;
        self.vertices[id.index()] = Some(n);
        Ok(n)
    }

    /// `EDGE_CURVE` vom Anfangs- zum Endpunkt; läuft die Kante gegen den Kurvenparameter
    /// (`t1 < t0`), ist `same_sense` falsch.
    fn edge(&mut self, e: &mut Entities, id: Id<Edge>) -> Result<u64, StepError> {
        if let Some(n) = self.edges[id.index()] {
            return Ok(n);
        }
        let edge = &self.solid.edges[id];
        let start = self.vertex(e, edge.start)?;
        let end = self.vertex(e, edge.end)?;
        let curve = geometry::curve(e, &edge.curve)?;
        let sense = logical(edge.t1 >= edge.t0);
        let n = e.push(&format!("EDGE_CURVE('',#{start},#{end},#{curve},{sense})"))?;
        self.edges[id.index()] = Some(n);
        self.edge_count += 1;
        Ok(n)
    }

    fn edge_loop(&mut self, e: &mut Entities, lp: &Loop) -> Result<u64, StepError> {
        let mut oriented = Vec::with_capacity(lp.half_edges.len());
        for he in &lp.half_edges {
            let ec = self.edge(e, he.edge)?;
            let dir = logical(he.forward);
            oriented.push(e.push(&format!("ORIENTED_EDGE('',*,*,#{ec},{dir})"))?);
        }
        e.push(&format!("EDGE_LOOP('',{})", refs(&oriented)))
    }

    /// `ADVANCED_FACE`; die äußere Schleife wird `FACE_OUTER_BOUND`, Löcher `FACE_BOUND`. Mit
    /// `reversed` wird die Fläche umgekehrt geschrieben (`same_sense` und die Richtung der
    /// Ränder gekippt), für die Schalen von Hohlräumen.
    fn face(&mut self, e: &mut Entities, id: Id<Face>, reversed: bool) -> Result<u64, StepError> {
        let f = &self.solid.faces[id];
        let surface = geometry::surface(e, &f.surface, f.patch)?;
        let along = logical(!reversed);
        let mut bounds = Vec::with_capacity(1 + f.inner.len());
        let outer = self.edge_loop(e, &f.outer)?;
        bounds.push(e.push(&format!("FACE_OUTER_BOUND('',#{outer},{along})"))?);
        for lp in &f.inner {
            let inner = self.edge_loop(e, lp)?;
            bounds.push(e.push(&format!("FACE_BOUND('',#{inner},{along})"))?);
        }
        let sense = logical(f.same_sense != reversed);
        e.push(&format!(
            "ADVANCED_FACE('',{},#{surface},{sense})",
            refs(&bounds)
        ))
    }

    fn closed_shell(
        &mut self,
        e: &mut Entities,
        faces: &[Id<Face>],
        reversed: bool,
    ) -> Result<u64, StepError> {
        let mut ids = Vec::with_capacity(faces.len());
        for &f in faces {
            ids.push(self.face(e, f, reversed)?);
        }
        e.push(&format!("CLOSED_SHELL('',{})", refs(&ids)))
    }
}

/// Ein zusammenhängendes Stück: äußere Schale und die Hohlräume darin (Indizes in
/// [`shell_faces`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lump {
    pub outer: usize,
    pub voids: Vec<usize>,
}

/// Sehnentoleranz (mm) des Netzes, mit dem die Schalen eines Körpers mit mehreren Schalen nach
/// dem Vorzeichen ihres Volumens unterschieden werden (grob genügt: es zählen nur das Vorzeichen
/// und ob ein Punkt innen liegt).
pub const CLASSIFY_CHORD: f64 = 0.05;
/// Ab diesem Wert der Umlaufzahl liegt ein Punkt innerhalb einer Schale (innen 1, außen 0).
const INSIDE_WINDING: f64 = 0.5;

/// Raumwinkel des Dreiecks `a, b, c`, von `p` aus gesehen (Formel von Van Oosterom und
/// Strackee).
fn solid_angle(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> f64 {
    let (a, b, c) = (a - p, b - p, c - p);
    let (la, lb, lc) = (a.length(), b.length(), c.length());
    let det = a.dot(b.cross(c));
    let div = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
    2.0 * det.atan2(div)
}

/// Teilt die Schalen eines Körpers in Stücke (Spec §3, Volumenvorzeichen): Schalen mit positivem
/// Volumen sind äußere Schalen, mit negativem Hohlräume. Jeder Hohlraum gehört zur kleinsten
/// äußeren Schale, in der sein erster Eckpunkt liegt (Umlaufzahl des Netzes). Ein Körper mit
/// einer Schale braucht kein Netz.
pub(crate) fn lumps(solid: &Solid) -> Result<Vec<Lump>, StepError> {
    let shells = shell_faces(solid);
    if shells.len() == 1 {
        return Ok(vec![Lump {
            outer: 0,
            voids: Vec::new(),
        }]);
    }
    let mesh = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: CLASSIFY_CHORD,
        },
    )
    .map_err(|e| {
        reject(&format!(
            "Die Schalen lassen sich nicht unterscheiden ({e})"
        ))
    })?;
    let mut shell_of = vec![usize::MAX; solid.faces.len()];
    for (k, faces) in shells.iter().enumerate() {
        for f in faces {
            shell_of[f.index()] = k;
        }
    }
    let (lo, hi) = mesh
        .positions
        .iter()
        .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
    let o = 0.5 * (lo + hi);
    let mut volume = vec![0.0; shells.len()];
    let mut triangles: Vec<Vec<[DVec3; 3]>> = vec![Vec::new(); shells.len()];
    for r in &mesh.face_ranges {
        let k = shell_of[r.face.index()];
        let start = r.first_index as usize;
        let end = start + r.index_count as usize;
        for t in mesh.indices[start..end].as_chunks::<3>().0 {
            let p = t.map(|i| mesh.positions[i as usize]);
            volume[k] += (p[0] - o).dot((p[1] - o).cross(p[2] - o)) / 6.0;
            triangles[k].push(p);
        }
    }
    if volume.iter().any(|v| !v.is_finite() || *v == 0.0) {
        return Err(reject("Eine Schale hat kein Volumen"));
    }
    let mut out: Vec<Lump> = (0..shells.len())
        .filter(|&k| volume[k] > 0.0)
        .map(|outer| Lump {
            outer,
            voids: Vec::new(),
        })
        .collect();
    for void in (0..shells.len()).filter(|&k| volume[k] < 0.0) {
        let he = solid.faces[shells[void][0]].outer.half_edges[0];
        let p = solid.vertices[solid.half_edge_start(he)].point;
        let inside = |k: usize| {
            let w: f64 = triangles[k]
                .iter()
                .map(|t| solid_angle(p, t[0], t[1], t[2]))
                .sum();
            w / (4.0 * std::f64::consts::PI) > INSIDE_WINDING
        };
        let owner = (0..out.len())
            .filter(|&i| inside(out[i].outer))
            .min_by(|&a, &b| volume[out[a].outer].total_cmp(&volume[out[b].outer]))
            .ok_or_else(|| reject("Ein Hohlraum liegt in keinem Teil des Körpers"))?;
        out[owner].voids.push(void);
    }
    Ok(out)
}

/// Schreibt den Körper (Spec §3): je Stück ein `MANIFOLD_SOLID_BREP`, mit Hohlräumen ein
/// `BREP_WITH_VOIDS` (Hohlräume als `ORIENTED_CLOSED_SHELL` mit `.F.`; ihre Schale wird dafür
/// umgekehrt geschrieben). `name` ist schon eine STEP-Zeichenkette.
pub(crate) fn write_solid(
    e: &mut Entities,
    name: &str,
    solid: &Solid,
) -> Result<WrittenSolid, StepError> {
    let shells = shell_faces(solid);
    let mut w = TopoWriter::new(solid);
    let mut items = Vec::new();
    for lump in lumps(solid)? {
        let outer = w.closed_shell(e, &shells[lump.outer], false)?;
        if lump.voids.is_empty() {
            items.push(e.push(&format!("MANIFOLD_SOLID_BREP({name},#{outer})"))?);
            continue;
        }
        let mut voids = Vec::with_capacity(lump.voids.len());
        for &v in &lump.voids {
            let shell = w.closed_shell(e, &shells[v], true)?;
            voids.push(e.push(&format!("ORIENTED_CLOSED_SHELL('',*,#{shell},.F.)"))?);
        }
        items.push(e.push(&format!(
            "BREP_WITH_VOIDS({name},#{outer},{})",
            refs(&voids)
        ))?);
    }
    Ok(WrittenSolid {
        items,
        faces: shells.iter().map(Vec::len).sum(),
        edges: w.edge_count,
    })
}
```

- [ ] **Step 4: Tests**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step`
Expected: PASS (4 in `voids`; alle anderen weiter grün, auch `check_body_counts_faces_edges_and_entities` mit 150 Einträgen – Körper mit einer Schale brauchen kein Netz).

- [ ] **Step 5: Mutationsprobe (danach zurücksetzen)**

In `write_solid` `let shell = w.closed_shell(e, &shells[v], true)?;` durch `…, false)?;` ersetzen → `a_cavity_becomes_a_brep_with_voids_with_a_reversed_shell` und `a_spherical_cavity_keeps_the_volume` schlagen fehl („Schleife passt nicht“).

- [ ] **Step 6: Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo deny check`
Expected: grün (`planora-mesh` ist ein Paket des Arbeitsbereichs, `cargo deny` bleibt sauber).

```bash
git add Cargo.lock crates/planora-step/Cargo.toml crates/planora-step/src/topology.rs crates/planora-step/tests/voids.rs
git commit -m "step: Hohlräume als BREP_WITH_VOIDS, getrennte Stücke als mehrere Körper eines Teils

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Stufentest 3 – Rundungen, Fasen, Booleans, Zufallsreihe, Beispieldateien

**Files:**
- Test: `crates/planora-step/tests/stage3.rs`

**Interfaces:**
- Consumes: Task 10–11; `planora_kernel::{BlendEdge, BlendKind::{Fillet, ChamferEqual, VariableFillet}, BlendRequest, Kernel::blend}`, `BoolOp`, `DMat4::from_euler(EulerRot::XYZ, …)`.
- Produces: Stufenabschluss 3; `write_samples_for_the_hand_test` (`#[ignore]`, schreibt `Rundung.step`, `Ecke.step`, `Fase.step`, `Variabel.step`, `Kreuz.step`, `Kuppel.step`, `Zylinder.step`, `Kegel.step`, `Kugel.step`, `Hohl.step` und `Alle.step` in den Ordner aus `PLANORA_STEP_SAMPLES` und nennt Flächenzahl und Volumen; für Task 15).
  Hinweise aus dem Planen: Schräge Zylinder mit Zylindern oder Kugeln rechnet der Kernel nicht (`UnsupportedIntersection`); die Zufallsreihe verrechnet deshalb einen gedrehten Quader mit einem gedrehten Quader, Zylinder oder einer Kugel. Die Freiform-Schnittkurve kommt aus Quader minus NURBS-Zylinder (Rezept aus `planora-mesh/tests/nurbs_common`).

- [ ] **Step 1: Test schreiben** (`crates/planora-step/tests/stage3.rs`)

```rust
//! Stufentest 3 (Teilprojekt 10, Task 12): Körper aus Rundungen, Fasen und Booleschen
//! Operationen (auch mit Freiform-Schnittkurven), gespiegelt und gedreht; Zufallsreihe mit
//! festem Startwert (kurz im Debug-Build, lang nur mit `--release --ignored`).

mod common;

use common::check::measure;
use common::{Lcg, checked, export};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{
    AnalyticKernel, BlendEdge, BlendKind, BlendRequest, Edge, Id, Kernel, Solid, Surface, UvRect,
};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec3, EulerRot, TAU};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use std::fs;
use std::path::PathBuf;

/// Sehnentoleranz der Netze.
const CHORD: f64 = 0.01;
/// Wie in `stage2.rs`: Volumenfehler höchstens `SAG_FACTOR · CHORD · Oberfläche`.
const SAG_FACTOR: f64 = 2.0;
/// Fälle der kurzen Zufallsreihe (Debug-Build, unter 30 s).
const SHORT_SERIES: usize = 12;
/// Fälle der langen Zufallsreihe (nur Release).
const LONG_SERIES: usize = 300;
/// Sehnentoleranz (mm) für die Volumenangaben der Beispieldateien.
const SAMPLE_CHORD: f64 = 0.001;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

/// Kante, deren beide Endpunkte `test` erfüllen (die erste, nach Id).
fn edge_where(s: &Solid, test: impl Fn(DVec3) -> bool) -> Id<Edge> {
    s.edges
        .iter()
        .find(|(_, e)| test(s.vertices[e.start].point) && test(s.vertices[e.end].point))
        .map(|(id, _)| id)
        .expect("Kante gefunden")
}

fn blend(s: &Solid, edges: &[Id<Edge>], kind: BlendKind) -> Solid {
    k().blend(
        s,
        &BlendRequest {
            edges: edges
                .iter()
                .map(|&edge| BlendEdge {
                    edge,
                    first_face: None,
                })
                .collect(),
            kind,
            tangent_chain: false,
        },
    )
    .unwrap_or_else(|e| panic!("Rundung: {e}"))
    .solid
}

fn cube() -> Solid {
    k().make_box(DVec3::splat(10.0)).unwrap()
}

/// Obere vordere Kante (y = 0, z = 10) und die beiden anderen Kanten an der Ecke (10, 0, 10).
fn corner_edges(s: &Solid) -> Vec<Id<Edge>> {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    vec![
        edge_where(s, |p| near(p.y, 0.0) && near(p.z, 10.0)),
        edge_where(s, |p| near(p.x, 10.0) && near(p.z, 10.0)),
        edge_where(s, |p| near(p.x, 10.0) && near(p.y, 0.0)),
    ]
}

/// Quader 20 × 20 × 10 um den Ursprung (z ∈ [−5, 5]).
fn centered_box() -> Solid {
    moved(
        &k().make_box(DVec3::new(20.0, 20.0, 10.0)).unwrap(),
        DMat4::from_translation(DVec3::new(-10.0, -10.0, -5.0)),
    )
}

/// Zylinder r = 3, h = 20 ab z = −10, dessen Mantel eine NURBS-Drehfläche ist (wie in
/// `planora-mesh/tests/nurbs_common`), um `angle` um z gedreht und um `dx` in x verschoben.
fn nurbs_cylinder(angle: f64, dx: f64) -> Solid {
    let (r, h) = (3.0, 20.0);
    let mut solid = k().make_cylinder(r, h).unwrap();
    let foot = DVec3::new(r, 0.0, 0.0);
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![foot, foot + DVec3::Z * h],
        vec![1.0; 2],
    )
    .unwrap();
    let surface = Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let id = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .unwrap();
    solid.faces[id].surface = surface;
    solid.faces[id].patch = Some(UvRect {
        u0: 0.0,
        u1: TAU,
        v0: 0.0,
        v1: 1.0,
    });
    moved(
        &solid,
        DMat4::from_translation(DVec3::new(dx, 0.0, -10.0)) * DMat4::from_rotation_z(angle),
    )
}

/// (Name, Körper) aus Teilprojekt 8, 7 und 3.
fn bodies() -> Vec<(&'static str, Solid)> {
    let c = cube();
    let edges = corner_edges(&c);
    let crossed = k()
        .boolean(
            &centered_box(),
            &nurbs_cylinder(0.3, 2.0),
            BoolOp::Difference,
        )
        .unwrap();
    let dome = k()
        .boolean(
            &c,
            &moved(
                &k().make_sphere(4.0).unwrap(),
                DMat4::from_translation(DVec3::new(5.0, 5.0, 10.0)),
            ),
            BoolOp::Union,
        )
        .unwrap();
    vec![
        (
            "Rundung",
            blend(&c, &edges[..1], BlendKind::Fillet { radius: 2.0 }),
        ),
        ("Ecke", blend(&c, &edges, BlendKind::Fillet { radius: 2.0 })),
        (
            "Fase",
            blend(&c, &edges[..1], BlendKind::ChamferEqual { distance: 1.5 }),
        ),
        (
            "Variabel",
            blend(
                &c,
                &edges[..1],
                BlendKind::VariableFillet {
                    points: vec![(0.0, 1.0), (1.0, 2.5)],
                },
            ),
        ),
        ("Kreuz", crossed),
        ("Kuppel", dome),
    ]
}

/// Prüft einen Körper vollständig und vergleicht das Volumen mit dem Netz des Kernels.
fn full(name: &str, s: &Solid) {
    let (text, _) = export(&[(name, s)]);
    let (m, _) = checked(&text, &[s]);
    let r = measure(&m, &[s], CHORD).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(r.flipped, 0, "{name}");
    assert!(
        (r.volume - r.kernel_volume).abs() <= SAG_FACTOR * CHORD * r.area,
        "{name}: {} gegen {}",
        r.volume,
        r.kernel_volume
    );
}

#[test]
fn rounded_chamfered_and_combined_bodies_pass_all_checks() {
    for (name, s) in bodies() {
        for (pose, m) in [
            ("wie gebaut", DMat4::IDENTITY),
            (
                "gespiegelt und gedreht",
                DMat4::from_rotation_z(0.6) * DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
            ),
        ] {
            let s = moved(&s, m);
            let label = format!("{name}, {pose}");
            full(&label, &s);
        }
    }
}

#[test]
fn the_blends_produce_the_expected_kinds_of_faces() {
    let kinds = |s: &Solid| {
        let (text, _) = export(&[("x", s)]);
        (
            text.contains("CYLINDRICAL_SURFACE("),
            text.contains("SPHERICAL_SURFACE("),
            text.contains("B_SPLINE_SURFACE"),
        )
    };
    let all = bodies();
    let get = |n: &str| &all.iter().find(|(name, _)| *name == n).unwrap().1;
    assert!(kinds(get("Rundung")).0);
    assert!(kinds(get("Ecke")).1, "Kugelecke");
    assert!(kinds(get("Variabel")).2, "variabler Radius ist Freiform");
    let (text, _) = export(&[("x", get("Kreuz"))]);
    assert!(
        text.contains("B_SPLINE_CURVE"),
        "Schnittkurve mit der Freiformfläche"
    );
}

/// Zufällige Lage: verschoben um bis zu ±2 und beliebig gedreht.
fn random_pose(rng: &mut Lcg) -> DMat4 {
    DMat4::from_translation(DVec3::new(
        rng.range(-2.0, 2.0),
        rng.range(-2.0, 2.0),
        rng.range(-2.0, 2.0),
    )) * DMat4::from_euler(
        EulerRot::XYZ,
        rng.range(0.0, 3.0),
        rng.range(0.0, 3.0),
        rng.range(0.0, 3.0),
    )
}

/// Zufälliger Körper: ein gedrehter Quader, verrechnet mit einem gedrehten Quader, einem Zylinder
/// oder einer Kugel (Paare, die der Kernel kann; Zylinder mit Zylinder oder Kugel schräg zur
/// Achse rechnet er noch nicht). `None`, wenn der Kernel die Operation benannt ablehnt.
fn random_body(rng: &mut Lcg) -> Option<Solid> {
    let size = |rng: &mut Lcg| {
        DVec3::new(
            rng.range(2.0, 8.0),
            rng.range(2.0, 8.0),
            rng.range(2.0, 8.0),
        )
    };
    let a = moved(&k().make_box(size(rng)).unwrap(), random_pose(rng));
    let b = match (rng.uniform() * 3.0) as usize {
        0 => k().make_box(size(rng)).unwrap(),
        1 => k()
            .make_cylinder(rng.range(1.0, 4.0), rng.range(2.0, 8.0))
            .unwrap(),
        _ => k().make_sphere(rng.range(1.0, 4.0)).unwrap(),
    };
    let b = moved(&b, random_pose(rng));
    let op =
        [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][(rng.uniform() * 3.0) as usize];
    k().boolean(&a, &b, op).ok()
}

fn series(cases: usize) -> usize {
    let mut rng = Lcg(20_261_014);
    let mut done = 0;
    for i in 0..cases {
        if let Some(s) = random_body(&mut rng) {
            full(&format!("Zufall {i}"), &s);
            done += 1;
        }
    }
    done
}

#[test]
fn a_short_random_series_passes_all_checks() {
    assert!(
        series(SHORT_SERIES) >= SHORT_SERIES / 2,
        "zu viele Ablehnungen"
    );
}

#[test]
#[ignore = "lange Reihe, nur mit --release --ignored"]
fn a_long_random_series_passes_all_checks() {
    assert!(
        series(LONG_SERIES) >= LONG_SERIES / 2,
        "zu viele Ablehnungen"
    );
}

/// Schreibt Beispieldateien für den Handtest in Fusion 360 (Task 15) in den Ordner aus der
/// Umgebungsvariable `PLANORA_STEP_SAMPLES` (sonst `planora-step-proben` im Ordner für
/// vorübergehende Dateien) und nennt Flächenzahl und Volumen (Netz, Sehne 0,001 mm) je Körper.
#[test]
#[ignore = "nur für den Handtest (Task 15): schreibt Beispieldateien"]
fn write_samples_for_the_hand_test() {
    let dir = std::env::var_os("PLANORA_STEP_SAMPLES")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("planora-step-proben"));
    fs::create_dir_all(&dir).unwrap();
    let hollow = k()
        .boolean(
            &cube(),
            &moved(
                &k().make_box(DVec3::splat(4.0)).unwrap(),
                DMat4::from_translation(DVec3::splat(3.0)),
            ),
            BoolOp::Difference,
        )
        .unwrap();
    let mut all = bodies();
    all.push(("Zylinder", k().make_cylinder(5.0, 12.0).unwrap()));
    all.push(("Kegel", k().make_cone(6.0, 9.0).unwrap()));
    all.push(("Kugel", k().make_sphere(7.0).unwrap()));
    all.push(("Hohl", hollow));
    for (name, s) in &all {
        let (text, report) = export(&[(name, s)]);
        fs::write(dir.join(format!("{name}.step")), &text).unwrap();
        let mesh = tessellate(
            s,
            &TessellationOptions {
                chord_tolerance: SAMPLE_CHORD,
            },
        )
        .unwrap();
        println!(
            "{name}.step: {} Flächen, Volumen {:.3} mm³",
            report.faces,
            mass_properties(&mesh).volume
        );
    }
    let list: Vec<(&str, &Solid)> = all.iter().map(|(n, s)| (*n, s)).collect();
    let (text, report) = export(&list);
    fs::write(dir.join("Alle.step"), &text).unwrap();
    println!(
        "Alle.step: {} Teile, Ordner {}",
        report.bodies,
        dir.display()
    );
}
```

- [ ] **Step 2: Tests** (prüft Fertiges; muss sofort grün sein)

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-step --test stage3 && cargo test -p planora-step --release --test stage3 -- --ignored a_long_random_series`
Expected: PASS (3 Tests, 2 ignoriert; Debug etwa 5 s; lange Reihe in Release etwa 4 s, beim Planen ohne eine einzige Ablehnung des Kernels). Laufzeiten für Task 14 notieren.

- [ ] **Step 3: Stufenabschluss**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
git diff --stat 07bac2b -- crates/planora-kernel crates/planora-math crates/planora-mesh crates/planora-exchange crates/planora-feature crates/planora-project crates/planora-render crates/planora-sketch
```
Expected: alles grün, `git diff` leer.

```bash
git add crates/planora-step/tests/stage3.rs
git commit -m "step: Stufentest 3 – Rundungen, Fasen, Booleans, Zufallsreihe, Beispieldateien

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---


## Stufe 4 – Fenster, Dokumentation, Gesamtprüfung, Handtest

### Task 13: STEP im Fenster „Exportieren …“

**Files:**
- Modify: `crates/planora-app/Cargo.toml` (Abschnitt `[dependencies]`), `crates/planora-app/src/export_dialog.rs` (ganz ersetzen), `crates/planora-app/tests/export_dialog.rs` (eine Zeile, mechanisch, P14)
- Test: `crates/planora-app/tests/export_step.rs`

**Interfaces:**
- Consumes: `planora_step::{StepBody, StepError, StepOptions, check_body, export_step_file}` (Task 3, 6); vorhandene Fensterteile `cards::card_row`, `jobs::{Job, Poll}` (`Job::spawn(|p: &Progress| …)`, `fetch(block)`, `fraction()`, `cancel()`), `file_chooser::FileChooser::save(dir, stem, extension)`, `state::{AppState, BodyKind}`, `AppState::{bodies, selected_body, document.edit_count(), document.mesh_body(..)}`.
- Produces (`pub` in `planora_app::export_dialog`, zusätzlich zum Bisherigen):
  ```rust
  pub enum ExportFormat { Mesh(Format), Step }              // Clone, Copy, Debug, PartialEq, Eq, Hash
  impl ExportFormat { pub const ALL: [ExportFormat; 5]; pub fn extension(self) -> &'static str;
      pub fn label(self) -> &'static str; pub fn is_step(self) -> bool; }
  pub const STEP_LABEL: &str = "STEP – für andere CAD-Programme (exakt)";
  pub const STEP_EXACT: &str = "Rundungen und Bohrungen bleiben exakt.";
  pub const STEP_NOTHING: &str = "Es gibt keinen Körper, der sich als STEP-Datei ausgeben lässt.";
  pub struct StepPrepared { pub bodies: Vec<(String, Solid)>, pub faces: usize,
      pub rejected: Vec<String>, pub skipped_meshes: Vec<String> }
  pub enum ExportStatus { …, StepReady { bodies: usize, faces: usize, rejected: Vec<String>, skipped_meshes: Vec<String> }, … }
  pub fn prepare_step(items: &[ExportItem], progress: &Progress) -> Result<StepPrepared, String>;
  pub fn write_step_bodies(path: &Path, prepared: &StepPrepared, progress: &Progress) -> Result<u64, StepError>;
  pub fn skipped_text(names: &[String]) -> String;
  pub struct ExportDialog { pub format: ExportFormat, … }   // vorher `pub format: Format`
  impl ExportDialog { pub fn write_progress(&self) -> f32; pub fn cancel_write(&mut self); … }
  ```
  Unverändert bleiben `Quality`, `Scope`, `ExportSource`, `ExportItem`, `Prepared`, `prepare`, `format_label`, `group_digits`, `mb_text`, `estimate_text`, `WriteState` (mit `Writing` ohne Wert), `STALE`, `NOT_COMPUTED`, `LOST`, `DIALOG_MARGIN`, `DIALOG_TOP`, `wide_window`, `PROGRESS_WIDTH`, `REPAINT`, `file_name` (vom Import-Fenster mitbenutzt).

- [ ] **Step 1: Abhängigkeit**

In `crates/planora-app/Cargo.toml` unter `[dependencies]` nach `planora-exchange.workspace = true` einfügen:

```toml
planora-step.workspace = true
```

- [ ] **Step 2: Tests schreiben** (`crates/planora-app/tests/export_step.rs`)

```rust
//! Fenster „Exportieren“ mit STEP (Teilprojekt 10, Task 13): fünftes Format, Karte statt
//! Qualität, Körper und Flächen, Netz-Körper ausgelassen, abgelehnte Körper, Schreiben mit
//! Hilfsdatei, veraltete Ergebnisse, Abbrechen, volle Breite und gleich hohe Karten.

use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_exchange::{ExchangeMesh, Format, Progress};
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::DVec3;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-step-ui-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn wide() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    }
}

/// Zeichnet das Fenster dreimal und liefert die Texte des letzten Bildes.
fn draw(d: &mut ExportDialog, s: &mut AppState) -> String {
    let ctx = context();
    let mut dir = None;
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show(ui.ctx(), s, &mut dir));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

/// Quader, Kugel und ein Netz-Körper „Würfel“; nichts gewählt.
fn mixed_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_sphere(5.0).unwrap();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    s.selected_body = None;
    s
}

fn step_dialog(s: &AppState) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.format = ExportFormat::Step;
    d.tick(s, true);
    d
}

#[test]
fn step_is_the_fifth_format_with_its_own_extension() {
    assert_eq!(ExportFormat::ALL.len(), 5);
    assert_eq!(ExportFormat::ALL[4], ExportFormat::Step);
    assert_eq!(ExportFormat::Step.extension(), "step");
    assert_eq!(ExportFormat::Step.label(), STEP_LABEL);
    assert_eq!(STEP_LABEL, "STEP – für andere CAD-Programme (exakt)");
    assert_eq!(ExportFormat::Mesh(Format::Stl).extension(), "stl");
    assert!(ExportFormat::Step.is_step() && !ExportFormat::Mesh(Format::Obj).is_step());
}

#[test]
fn step_counts_bodies_and_faces_and_names_the_mesh_bodies() {
    let s = mixed_state();
    let d = step_dialog(&s);
    assert_eq!(
        d.status(&s),
        ExportStatus::StepReady {
            bodies: 2,
            faces: 7,
            rejected: vec![],
            skipped_meshes: vec!["Würfel".into()],
        }
    );
}

#[test]
fn the_window_shows_step_texts_without_the_quality_card() {
    let mut s = mixed_state();
    let mut d = step_dialog(&s);
    let text = draw(&mut d, &mut s);
    for word in [
        STEP_LABEL,
        STEP_EXACT,
        "2 Körper, 7 Flächen",
        "Netz-Körper werden als STEP nicht ausgegeben: „Würfel“",
        "STL – für den 3D-Druck",
        "Umfang",
        "Exportieren …",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    for word in ["Qualität", "Grob", "Dreiecke"] {
        assert!(
            !text.contains(word),
            "„{word}“ gehört nicht zu STEP:\n{text}"
        );
    }
    let rect = d.last_rect.unwrap();
    assert!(
        rect.width() >= 1200.0 - 2.0 * DIALOG_MARGIN - 1.0,
        "{rect:?}"
    );
    assert_eq!(d.last_cards.len(), 3);
    assert!(
        d.last_cards
            .windows(2)
            .all(|w| (w[0].height() - w[1].height()).abs() < 0.5),
        "{:?}",
        d.last_cards
    );
}

#[test]
fn writing_step_goes_to_the_chosen_file() {
    let s = mixed_state();
    let dir = temp_dir("schreiben");
    let mut d = step_dialog(&s);
    d.begin_save(dir.clone(), &s);
    assert_eq!(d.chooser().unwrap().target(), Some(dir.join("Körper.step")));
    let path = dir.join("Körper.step");
    d.write_to(path.clone(), &s);
    d.tick(&s, true);
    let bytes = match d.write_state() {
        WriteState::Done(p, n) => {
            assert_eq!(p, path);
            n
        }
        other => panic!("{other:?}"),
    };
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(bytes, text.len() as u64);
    assert!(text.starts_with("ISO-10303-21;"));
    assert!(text.contains("FILE_NAME('K\\X2\\00F6\\X0\\rper.step'"));
    assert_eq!(text.matches("PRODUCT('").count(), 2);
    assert!(text.contains("PRODUCT('Quader 1','Quader 1'"));
    assert!(
        !text.contains("W\\X2\\00FC\\X0\\rfel"),
        "Netz-Körper nicht in der Datei"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_stale_step_result_is_never_written() {
    let mut s = mixed_state();
    s.selected_body = Some(0);
    let dir = temp_dir("veraltet");
    let mut d = step_dialog(&s);
    assert_eq!(d.scope, Scope::Selected);
    s.selected_body = Some(1);
    let path = dir.join("x.step");
    d.write_to(path.clone(), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_mesh_bodies_leave_nothing_to_write() {
    let mut s = AppState::new();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    let mut d = step_dialog(&s);
    assert!(matches!(
        d.status(&s),
        ExportStatus::StepReady { bodies: 0, .. }
    ));
    let text = draw(&mut d, &mut s);
    assert!(text.contains(STEP_NOTHING), "{text}");
    let dir = temp_dir("nichts");
    d.write_to(dir.join("x.step"), &s);
    assert!(matches!(d.write_state(), WriteState::Failed(m) if m.contains("keinen Körper")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rejected_bodies_are_listed_with_their_reason() {
    let mut open = AnalyticKernel::default()
        .make_box(DVec3::splat(1.0))
        .unwrap();
    let f = open.faces.iter().next().unwrap().0;
    open.faces[f].outer.half_edges.pop();
    let good = AnalyticKernel::default()
        .make_box(DVec3::splat(1.0))
        .unwrap();
    let items = vec![
        ExportItem {
            name: "Offen".into(),
            source: ExportSource::Solid(open),
        },
        ExportItem {
            name: "Gut".into(),
            source: ExportSource::Solid(good),
        },
    ];
    let p = prepare_step(&items, &Progress::new()).unwrap();
    assert_eq!(p.bodies.len(), 1);
    assert_eq!(p.faces, 6);
    assert_eq!(p.rejected.len(), 1);
    assert!(
        p.rejected[0].starts_with("„Offen“ wird nicht ausgegeben: "),
        "{}",
        p.rejected[0]
    );
    let cancelled = Progress::new();
    cancelled.cancel();
    assert_eq!(prepare_step(&items, &cancelled).unwrap_err(), "Abgebrochen");
}

#[test]
fn cancelling_step_writing_keeps_the_old_file() {
    let s = mixed_state();
    let items = step_dialog(&s).items(&s);
    let prepared = prepare_step(&items, &Progress::new()).unwrap();
    let dir = temp_dir("abbrechen");
    let path = dir.join("alt.step");
    fs::write(&path, "alt").unwrap();
    let p = Progress::new();
    p.cancel();
    assert_eq!(
        write_step_bodies(&path, &prepared, &p)
            .unwrap_err()
            .to_string(),
        "Abgebrochen"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "alt");
    let n = write_step_bodies(&path, &prepared, &Progress::new()).unwrap();
    assert_eq!(n, fs::metadata(&path).unwrap().len());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_the_step_check_says_so_and_switching_back_keeps_the_mesh_result() {
    let s = mixed_state();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
    d.format = ExportFormat::Step;
    d.tick(&s, false);
    d.cancel();
    assert_eq!(d.status(&s), ExportStatus::Failed("Abgebrochen".into()));
    d.format = ExportFormat::Mesh(Format::ThreeMf);
    assert!(
        matches!(d.status(&s), ExportStatus::Ready { .. }),
        "das Netz bleibt gültig"
    );
    d.format = ExportFormat::Step;
    d.recompute();
    d.tick(&s, true);
    assert!(matches!(
        d.status(&s),
        ExportStatus::StepReady { bodies: 2, .. }
    ));
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test export_step`
Expected: FAIL – `cannot find type ExportFormat`.

- [ ] **Step 4: Implementieren**

`crates/planora-app/src/export_dialog.rs` ganz ersetzen:

```rust
//! Fenster „Exportieren“ (Spec §8 des Netz-Austauschs, Spec §6 des STEP-Exports): Format,
//! Qualität und Umfang als gleich hohe Karten über die ganze Breite; das Netz bzw. die Prüfung
//! der Körper entsteht im Arbeitsthread (Fortschritt, Abbrechen); danach Zahl der Dreiecke,
//! geschätzte Größe und Prüfbericht (Netz) bzw. Zahl der Körper und Flächen (STEP); gespeichert
//! wird über die eigene Dateiauswahl (Plan P14) und eine Hilfsdatei.

use crate::browser::ERROR_COLOR;
use crate::cards::card_row;
use crate::file_chooser::{ChooserAction, FileChooser, default_dir};
use crate::jobs::{Job, Poll};
use crate::mesh_body::{WARNING_COLOR, report_text};
use crate::state::{AppState, BodyKind};
use planora_exchange::{
    ExchangeError, ExchangeMesh, ExportBody, Format, MeshReport, MeshTransform, PlacedSource,
    Progress, SolidSource, check_mesh, collect_body, export_file,
};
use planora_feature::BodyRef;
use planora_kernel::Solid;
use planora_mesh::{TessellationOptions, tessellate};
use planora_step::{StepBody, StepError, StepOptions, check_body, export_step_file};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Abstand des Fensters zum linken und rechten Rand; sonst reicht es über die ganze Breite.
pub const DIALOG_MARGIN: f32 = 16.0;
/// Abstand des Fensters zum oberen Rand (unter Menü und Tab-Leiste).
pub const DIALOG_TOP: f32 = 60.0;
/// Kleinste Breite bei sehr schmalen Fenstern.
pub(crate) const MIN_WIDTH: f32 = 320.0;
/// Breite des Fortschrittsbalkens.
pub(crate) const PROGRESS_WIDTH: f32 = 280.0;
/// So oft wird neu gezeichnet, solange ein Arbeitsthread läuft.
pub(crate) const REPAINT: Duration = Duration::from_millis(100);
/// Bytes je Megabyte (dezimal, wie die Dateiverwaltung).
const BYTES_PER_MB: f64 = 1e6;
/// Darunter zeigt das Fenster „unter 0,1 MB“.
const MIN_SHOWN_MB: f64 = 0.1;
/// Meldung, wenn ein Arbeitsthread ohne Ergebnis endet.
pub const LOST: &str = "Die Berechnung wurde unerwartet beendet.";
/// Warnung bei nicht geschlossenem Netz (Spec E6).
pub const NOT_CLOSED_WARNING: &str = "Die Datei wird trotzdem geschrieben. Manche Programme für den 3D-Druck melden solche Netze als fehlerhaft.";
/// Eintrag des fünften Formats (STEP-Spec §6).
pub const STEP_LABEL: &str = "STEP – für andere CAD-Programme (exakt)";
/// Steht bei STEP an der Stelle der Karte „Qualität“ (STEP-Spec §6).
pub const STEP_EXACT: &str = "Rundungen und Bohrungen bleiben exakt.";
/// Meldung, wenn bei STEP kein Körper übrig bleibt.
pub const STEP_NOTHING: &str = "Es gibt keinen Körper, der sich als STEP-Datei ausgeben lässt.";

/// Fenster über die ganze Breite (Rand DIALOG_MARGIN), unter Menü und Tab-Leiste.
pub(crate) fn wide_window(title: &'static str, ctx: &egui::Context) -> egui::Window<'static> {
    let width = (ctx.content_rect().width() - 2.0 * DIALOG_MARGIN).max(MIN_WIDTH);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, DIALOG_TOP])
        .min_width(width)
        .max_width(width)
}

/// Sehnentoleranz „Grob“ in Millimetern (Spec E5).
const CHORD_COARSE: f64 = 0.1;
/// Sehnentoleranz „Normal“ in Millimetern (Spec E5).
const CHORD_NORMAL: f64 = 0.01;
/// Sehnentoleranz „Fein“ in Millimetern (Spec E5).
const CHORD_FINE: f64 = 0.001;
/// Meldung, wenn sich Auswahl, Qualität oder Modell seit der Berechnung geändert haben.
pub const STALE: &str = "Die Auswahl hat sich geändert, bitte noch einmal speichern.";
/// Meldung, wenn es noch kein fertiges Netz gibt.
pub const NOT_COMPUTED: &str = "Es ist noch nichts berechnet. Bitte warten oder neu berechnen.";

/// Qualität des Netzes (Spec E5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quality {
    Coarse,
    Normal,
    Fine,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Coarse, Quality::Normal, Quality::Fine];

    /// Sehnentoleranz in Millimetern.
    pub fn chord_tolerance(self) -> f64 {
        match self {
            Quality::Coarse => CHORD_COARSE,
            Quality::Normal => CHORD_NORMAL,
            Quality::Fine => CHORD_FINE,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quality::Coarse => "Grob (bis 0,1 mm Abweichung)",
            Quality::Normal => "Normal (bis 0,01 mm)",
            Quality::Fine => "Fein (bis 0,001 mm)",
        }
    }
}

/// Umfang (Spec E4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Selected,
    Visible,
}

/// Format im Fenster: eines der vier Netzformate oder STEP (exakte Körper).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Mesh(Format),
    Step,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 5] = [
        ExportFormat::Mesh(Format::Stl),
        ExportFormat::Mesh(Format::Obj),
        ExportFormat::Mesh(Format::ThreeMf),
        ExportFormat::Mesh(Format::Glb),
        ExportFormat::Step,
    ];

    /// Dateiendung ohne Punkt.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Mesh(f) => f.extension(),
            ExportFormat::Step => "step",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Mesh(f) => format_label(f),
            ExportFormat::Step => STEP_LABEL,
        }
    }

    pub fn is_step(self) -> bool {
        self == ExportFormat::Step
    }
}

/// Ein Körper für den Arbeitsthread (Kopie, damit das Dokument frei bleibt).
#[derive(Clone, Debug)]
pub enum ExportSource {
    Solid(Solid),
    Mesh(Arc<ExchangeMesh>, MeshTransform),
}

#[derive(Clone, Debug)]
pub struct ExportItem {
    pub name: String,
    pub source: ExportSource,
}

/// Fertig vernetzte Körper mit gemeinsamem Bericht.
#[derive(Debug)]
pub struct Prepared {
    pub bodies: Vec<ExportBody>,
    pub report: MeshReport,
}

impl Prepared {
    pub fn triangles(&self) -> u64 {
        self.bodies
            .iter()
            .map(|b| b.mesh.triangle_count() as u64)
            .sum()
    }

    pub fn vertices(&self) -> u64 {
        self.bodies
            .iter()
            .map(|b| b.mesh.positions.len() as u64)
            .sum()
    }
}

/// Für STEP geprüfte Körper (STEP-Spec §4, §6, E3).
#[derive(Debug)]
pub struct StepPrepared {
    /// Körper, die die Prüfung bestanden haben: Name und exakter Körper.
    pub bodies: Vec<(String, Solid)>,
    /// Flächen aller geprüften Körper.
    pub faces: usize,
    /// Abgelehnte Körper mit Grund („„Name“ wird nicht ausgegeben: …“).
    pub rejected: Vec<String>,
    /// Netz-Körper, die STEP nicht enthält (E3).
    pub skipped_meshes: Vec<String>,
}

/// Was das Fenster zum Ergebnis zeigt.
#[derive(Debug, PartialEq)]
pub enum ExportStatus {
    Computing(f32),
    Ready {
        triangles: u64,
        bytes: u64,
        report: MeshReport,
    },
    /// STEP: Zahl der Körper und Flächen, abgelehnte Körper, ausgelassene Netz-Körper.
    StepReady {
        bodies: usize,
        faces: usize,
        rejected: Vec<String>,
        skipped_meshes: Vec<String>,
    },
    Failed(String),
}

/// Stand des Schreibens der Datei.
#[derive(Debug, PartialEq)]
pub enum WriteState {
    Idle,
    Writing,
    Done(PathBuf, u64),
    Failed(String),
}

/// Vernetzt die Körper (läuft im Arbeitsthread); bricht zwischen den Körpern ab, wenn
/// gewünscht (Plan P15).
pub fn prepare(
    items: &[ExportItem],
    quality: Quality,
    progress: &Progress,
) -> Result<Prepared, String> {
    progress.set_total(items.len() as u64);
    let mut bodies = Vec::with_capacity(items.len());
    for item in items {
        progress.check().map_err(|e| e.to_string())?;
        let body = match &item.source {
            ExportSource::Solid(s) => {
                let options = TessellationOptions {
                    chord_tolerance: quality.chord_tolerance(),
                };
                let mesh = tessellate(s, &options)
                    .map_err(|e| format!("„{}“ lässt sich nicht vernetzen: {e}", item.name))?;
                collect_body(&SolidSource {
                    name: &item.name,
                    mesh: &mesh,
                })
            }
            ExportSource::Mesh(m, t) => collect_body(&PlacedSource {
                name: &item.name,
                mesh: m,
                transform: *t,
            }),
        }
        .map_err(|e| format!("„{}“: {e}", item.name))?;
        bodies.push(body);
        progress.advance(1);
    }
    let reports: Vec<MeshReport> = bodies.iter().map(|b| check_mesh(&b.mesh)).collect();
    Ok(Prepared {
        report: MeshReport::combine(&reports),
        bodies,
    })
}

/// Prüft die Körper für STEP (läuft im Arbeitsthread): Netz-Körper werden ausgelassen und
/// genannt, jeder exakte Körper durchläuft `check_body`; abgelehnte stehen mit Grund in der
/// Liste. Bricht zwischen den Körpern und in großen Körpern ab, wenn gewünscht.
pub fn prepare_step(items: &[ExportItem], progress: &Progress) -> Result<StepPrepared, String> {
    progress.set_total(items.len() as u64);
    let mut out = StepPrepared {
        bodies: Vec::new(),
        faces: 0,
        rejected: Vec::new(),
        skipped_meshes: Vec::new(),
    };
    for item in items {
        progress.check().map_err(|e| e.to_string())?;
        match &item.source {
            ExportSource::Mesh(..) => out.skipped_meshes.push(item.name.clone()),
            ExportSource::Solid(s) => {
                let body = StepBody {
                    name: item.name.clone(),
                    solid: s,
                };
                match check_body(&body, progress) {
                    Ok(stats) => {
                        out.faces += stats.faces;
                        out.bodies.push((item.name.clone(), s.clone()));
                    }
                    Err(e @ StepError::InvalidBody { .. }) => out.rejected.push(e.to_string()),
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        progress.advance(1);
    }
    Ok(out)
}

/// Schreibt geprüfte Körper als STEP-Datei nach `path` (Hilfsdatei, Fortschritt, Abbrechen);
/// liefert die Dateigröße.
pub fn write_step_bodies(
    path: &Path,
    prepared: &StepPrepared,
    progress: &Progress,
) -> Result<u64, StepError> {
    let bodies: Vec<StepBody> = prepared
        .bodies
        .iter()
        .map(|(name, solid)| StepBody {
            name: name.clone(),
            solid,
        })
        .collect();
    let opts = StepOptions::new(&file_name(path));
    export_step_file(path, &bodies, &opts, progress).map(|(_, bytes)| bytes)
}

/// Kurzbeschreibung eines Netzformats für die Auswahl.
pub fn format_label(f: Format) -> &'static str {
    match f {
        Format::Stl => "STL – für den 3D-Druck",
        Format::Obj => "OBJ – für viele 3D-Programme",
        Format::ThreeMf => "3MF – für den 3D-Druck, mit Namen",
        Format::Glb => "glTF (.glb) – zum Ansehen und Zeigen",
    }
}

/// Hinweis auf ausgelassene Netz-Körper (STEP-Spec E3).
pub fn skipped_text(names: &[String]) -> String {
    let list: Vec<String> = names.iter().map(|n| format!("„{n}“")).collect();
    format!(
        "Netz-Körper werden als STEP nicht ausgegeben: {}. Für sie bitte STL, OBJ, 3MF oder glTF wählen.",
        list.join(", ")
    )
}

/// Zahl mit Leerzeichen als Tausendertrenner („12 345“).
pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Größe in Megabyte mit Komma; unter 0,1 MB „unter 0,1 MB“.
pub fn mb_text(bytes: u64) -> String {
    let mb = bytes as f64 / BYTES_PER_MB;
    if mb < MIN_SHOWN_MB {
        "unter 0,1 MB".into()
    } else {
        format!("{mb:.1} MB").replace('.', ",")
    }
}

/// Geschätzte Größe: „ca. 1,2 MB“ (sehr kleine Dateien „unter 0,1 MB“).
pub fn estimate_text(bytes: u64) -> String {
    let t = mb_text(bytes);
    if t.starts_with("unter") {
        t
    } else {
        format!("ca. {t}")
    }
}

/// Dateiname ohne Ordner.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Stand, zu dem ein Ergebnis gehört: Qualität (`None` bei STEP), Umfang, Änderungszahl des
/// Dokuments, gewählter Körper.
type Key = (Option<Quality>, Scope, u64, Option<BodyRef>);
type PrepJob = Job<Result<Prepared, String>>;
type PrepResult = Result<Arc<Prepared>, String>;
type StepJob = Job<Result<StepPrepared, String>>;
type StepResult = Result<Arc<StepPrepared>, String>;
type WriteJob = Job<Result<u64, ExchangeError>>;
type StepWriteJob = Job<Result<u64, StepError>>;

pub struct ExportDialog {
    pub format: ExportFormat,
    pub quality: Quality,
    pub scope: Scope,
    prepared: Option<(Key, PrepResult)>,
    job: Option<(Key, PrepJob)>,
    step_prepared: Option<(Key, StepResult)>,
    step_job: Option<(Key, StepJob)>,
    chooser: Option<FileChooser>,
    /// Schlüssel und Format zur Zeit des Öffnens der Dateiauswahl; ändern sie sich, schließt sie.
    chooser_key: Option<Key>,
    writing: Option<(PathBuf, WriteJob)>,
    step_writing: Option<(PathBuf, StepWriteJob)>,
    written: Option<Result<(PathBuf, u64), String>>,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
}

impl ExportDialog {
    /// Voreinstellung: STL, Normal; Umfang „gewählter Körper“, wenn einer gewählt ist.
    pub fn new(state: &AppState) -> ExportDialog {
        let selected = state.selected_body.is_some_and(|i| i < state.bodies.len());
        ExportDialog {
            format: ExportFormat::Mesh(Format::Stl),
            quality: Quality::Normal,
            scope: if selected {
                Scope::Selected
            } else {
                Scope::Visible
            },
            prepared: None,
            job: None,
            step_prepared: None,
            step_job: None,
            chooser: None,
            chooser_key: None,
            writing: None,
            step_writing: None,
            written: None,
            last_rect: None,
            last_cards: Vec::new(),
        }
    }

    fn has_selection(state: &AppState) -> bool {
        state.selected_body.is_some_and(|i| i < state.bodies.len())
    }

    /// Indizes der Körper im Umfang (Spec E4: der gewählte, sonst alle sichtbaren).
    fn chosen(&self, state: &AppState) -> Vec<usize> {
        match (self.scope, state.selected_body) {
            (Scope::Selected, Some(i)) if i < state.bodies.len() => vec![i],
            _ => (0..state.bodies.len()).collect(),
        }
    }

    /// Kopien der Körper im Umfang für den Arbeitsthread.
    pub fn items(&self, state: &AppState) -> Vec<ExportItem> {
        self.chosen(state)
            .into_iter()
            .filter_map(|i| {
                let b = &state.bodies[i];
                let source = if b.kind == BodyKind::Mesh {
                    let (mesh, t) = state.document.mesh_body(b.source.feature)?;
                    ExportSource::Mesh(Arc::clone(mesh), t)
                } else {
                    ExportSource::Solid(b.solid.clone())
                };
                Some(ExportItem {
                    name: b.name.clone(),
                    source,
                })
            })
            .collect()
    }

    fn key(&self, state: &AppState) -> Key {
        let selected = match self.scope {
            Scope::Selected => state
                .selected_body
                .and_then(|i| state.bodies.get(i))
                .map(|b| b.source),
            Scope::Visible => None,
        };
        let quality = (!self.format.is_step()).then_some(self.quality);
        (quality, self.scope, state.document.edit_count(), selected)
    }

    /// Startet das Vernetzen bzw. die STEP-Prüfung, wenn sich Format, Qualität, Umfang oder
    /// Dokument geändert haben, und holt fertige Ergebnisse ab. Mit `block` wird gewartet
    /// (Tests).
    pub fn tick(&mut self, state: &AppState, block: bool) {
        // Ohne Auswahl gibt es „Gewählter Körper“ nicht mehr (Anzeige und Export stimmen überein).
        if self.scope == Scope::Selected && !Self::has_selection(state) {
            self.scope = Scope::Visible;
        }
        let key = self.key(state);
        if self.format.is_step() {
            let fresh = self.step_prepared.as_ref().is_some_and(|(k, _)| *k == key);
            let running = self.step_job.as_ref().is_some_and(|(k, _)| *k == key);
            if !fresh && !running {
                let items = self.items(state);
                self.step_job = Some((key, Job::spawn(move |p| prepare_step(&items, p))));
            }
            let finished = match &self.step_job {
                Some((k, job)) => match job.fetch(block) {
                    Poll::Pending => None,
                    Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                    Poll::Lost => Some((*k, Err(LOST.to_string()))),
                },
                None => None,
            };
            if let Some(done) = finished {
                self.step_prepared = Some(done);
                self.step_job = None;
            }
        } else {
            let fresh = self.prepared.as_ref().is_some_and(|(k, _)| *k == key);
            let running = self.job.as_ref().is_some_and(|(k, _)| *k == key);
            if !fresh && !running {
                let items = self.items(state);
                let quality = self.quality;
                // Ein alter Auftrag wird dabei fallengelassen und bricht ab.
                self.job = Some((key, Job::spawn(move |p| prepare(&items, quality, p))));
            }
            let finished = match &self.job {
                Some((k, job)) => match job.fetch(block) {
                    Poll::Pending => None,
                    Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                    Poll::Lost => Some((*k, Err(LOST.to_string()))),
                },
                None => None,
            };
            if let Some(done) = finished {
                self.prepared = Some(done);
                self.job = None;
            }
        }
        let wrote = match &self.writing {
            Some((path, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n)).map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.written = Some(w);
            self.writing = None;
        }
        let wrote = match &self.step_writing {
            Some((path, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n)).map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.written = Some(w);
            self.step_writing = None;
        }
    }

    pub fn status(&self, state: &AppState) -> ExportStatus {
        let key = self.key(state);
        if self.format.is_step() {
            if let Some((k, job)) = &self.step_job
                && *k == key
            {
                return ExportStatus::Computing(job.fraction());
            }
            return match &self.step_prepared {
                Some((k, Ok(p))) if *k == key => ExportStatus::StepReady {
                    bodies: p.bodies.len(),
                    faces: p.faces,
                    rejected: p.rejected.clone(),
                    skipped_meshes: p.skipped_meshes.clone(),
                },
                Some((k, Err(m))) if *k == key => ExportStatus::Failed(m.clone()),
                _ => ExportStatus::Computing(0.0),
            };
        }
        if let Some((k, job)) = &self.job
            && *k == key
        {
            return ExportStatus::Computing(job.fraction());
        }
        match (&self.prepared, self.format) {
            (Some((k, Ok(p))), ExportFormat::Mesh(f)) if *k == key => ExportStatus::Ready {
                triangles: p.triangles(),
                bytes: f.estimate_bytes(p.triangles(), p.vertices()),
                report: p.report,
            },
            (Some((k, Err(m))), _) if *k == key => ExportStatus::Failed(m.clone()),
            _ => ExportStatus::Computing(0.0),
        }
    }

    pub fn write_state(&self) -> WriteState {
        if self.writing.is_some() || self.step_writing.is_some() {
            return WriteState::Writing;
        }
        match &self.written {
            None => WriteState::Idle,
            Some(Ok((p, n))) => WriteState::Done(p.clone(), *n),
            Some(Err(m)) => WriteState::Failed(m.clone()),
        }
    }

    /// Bricht das Vernetzen bzw. die Prüfung ab; das Fenster zeigt „Abgebrochen“, bis neu
    /// berechnet wird.
    pub fn cancel(&mut self) {
        if let Some((key, _)) = self.job.take() {
            self.prepared = Some((key, Err(ExchangeError::Cancelled.to_string())));
        }
        if let Some((key, _)) = self.step_job.take() {
            self.step_prepared = Some((key, Err(StepError::Cancelled.to_string())));
        }
    }

    /// Erledigter Anteil beim Schreiben einer STEP-Datei (Netzformate: 0).
    pub fn write_progress(&self) -> f32 {
        self.step_writing
            .as_ref()
            .map_or(0.0, |(_, job)| job.fraction())
    }

    /// Bricht das Schreiben einer STEP-Datei ab; die Zieldatei bleibt unverändert.
    pub fn cancel_write(&mut self) {
        if let Some((_, job)) = &self.step_writing {
            job.cancel();
        }
    }

    /// Verwirft das Ergebnis; das nächste `tick` rechnet neu.
    pub fn recompute(&mut self) {
        self.prepared = None;
        self.step_prepared = None;
    }

    /// Öffnet die Dateiauswahl im Ordner `dir` mit dem Namen des Körpers (bei mehreren
    /// „Körper“); Zeichen, die in Dateinamen nicht erlaubt sind, werden „_“.
    pub fn begin_save(&mut self, dir: PathBuf, state: &AppState) {
        let names: Vec<&str> = self
            .chosen(state)
            .into_iter()
            .filter_map(|i| state.bodies.get(i).map(|b| b.name.as_str()))
            .collect();
        let stem = match names.as_slice() {
            [one] => (*one).to_string(),
            _ => "Körper".to_string(),
        };
        let stem: String = stem
            .chars()
            .map(|c| {
                if c.is_control() || "\\/:*?\"<>|".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        self.chooser_key = Some(self.key(state));
        self.chooser = Some(FileChooser::save(dir, stem, self.format.extension()));
    }

    pub fn chooser(&self) -> Option<&FileChooser> {
        self.chooser.as_ref()
    }

    /// Schreibt das fertige Ergebnis im Arbeitsthread nach `path` (über eine Hilfsdatei).
    /// Schreibt nur, wenn das Ergebnis noch zum Stand von `state` passt; sonst gibt es eine
    /// Meldung.
    pub fn write_to(&mut self, path: PathBuf, state: &AppState) {
        let key = self.key(state);
        match self.format {
            ExportFormat::Step => {
                let Some((k, Ok(prepared))) = &self.step_prepared else {
                    self.written = Some(Err(NOT_COMPUTED.to_string()));
                    return;
                };
                if *k != key {
                    self.written = Some(Err(STALE.to_string()));
                    return;
                }
                if prepared.bodies.is_empty() {
                    self.written = Some(Err(StepError::NothingToExport.to_string()));
                    return;
                }
                let (prepared, target) = (Arc::clone(prepared), path.clone());
                self.written = None;
                self.step_writing = Some((
                    path,
                    Job::spawn(move |p| write_step_bodies(&target, &prepared, p)),
                ));
            }
            ExportFormat::Mesh(format) => {
                let Some((k, Ok(prepared))) = &self.prepared else {
                    self.written = Some(Err(NOT_COMPUTED.to_string()));
                    return;
                };
                if *k != key {
                    self.written = Some(Err(STALE.to_string()));
                    return;
                }
                let (prepared, target) = (Arc::clone(prepared), path.clone());
                self.written = None;
                self.writing = Some((
                    path,
                    Job::spawn(move |_| export_file(&target, format, &prepared.bodies)),
                ));
            }
        }
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll. `last_dir` ist der zuletzt
    /// benutzte Ordner der Dateiauswahl.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
    ) -> bool {
        self.tick(state, false);
        let status = self.status(state);
        let write_state = self.write_state();
        let write_progress = self.write_progress();
        let has_selection = Self::has_selection(state);
        let before = self.format;
        let (mut format, mut quality, mut scope) = (self.format, self.quality, self.scope);
        let (mut open, mut close, mut cancel, mut recompute, mut begin, mut drop_chooser) =
            (true, false, false, false, false, false);
        let mut cancel_write = false;
        let mut chosen: Option<PathBuf> = None;
        let mut cards = Vec::new();
        let chooser = &mut self.chooser;
        let response = wide_window("Exportieren", ctx)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut format_card = |ui: &mut egui::Ui| {
                    ui.strong("Format");
                    for f in ExportFormat::ALL {
                        ui.radio_value(&mut format, f, f.label());
                    }
                };
                let mut quality_card = |ui: &mut egui::Ui| {
                    if before.is_step() {
                        ui.label(STEP_EXACT);
                    } else {
                        ui.strong("Qualität");
                        for q in Quality::ALL {
                            ui.radio_value(&mut quality, q, q.label());
                        }
                        ui.weak("Feiner heißt mehr Dreiecke und größere Dateien.");
                    }
                };
                let mut scope_card = |ui: &mut egui::Ui| {
                    ui.strong("Umfang");
                    let selected = ui
                        .add_enabled(
                            has_selection,
                            egui::RadioButton::new(scope == Scope::Selected, "Gewählter Körper"),
                        )
                        .on_disabled_hover_text("Dafür zuerst einen Körper wählen.");
                    if selected.clicked() {
                        scope = Scope::Selected;
                    }
                    ui.radio_value(&mut scope, Scope::Visible, "Alle sichtbaren Körper");
                    ui.weak("Unterdrückte Schritte zählen nicht.");
                };
                let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                    &mut [&mut format_card, &mut quality_card, &mut scope_card];
                cards = card_row(ui, "export", row);
                ui.add_space(8.0);
                match &status {
                    ExportStatus::Computing(f) => {
                        let text = if before.is_step() {
                            "Die Körper werden geprüft …"
                        } else {
                            "Das Netz wird berechnet …"
                        };
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(*f)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text(text),
                            );
                            if ui.button("Abbrechen").clicked() {
                                cancel = true;
                            }
                        });
                    }
                    ExportStatus::Failed(m) => {
                        ui.colored_label(ERROR_COLOR, m);
                        if ui.button("Neu berechnen").clicked() {
                            recompute = true;
                        }
                    }
                    ExportStatus::Ready {
                        triangles,
                        bytes,
                        report,
                    } => {
                        ui.label(format!(
                            "{} Dreiecke, {}",
                            group_digits(*triangles),
                            estimate_text(*bytes)
                        ));
                        if report.closed {
                            ui.label(report_text(report));
                        } else {
                            ui.colored_label(WARNING_COLOR, report_text(report));
                            ui.colored_label(WARNING_COLOR, NOT_CLOSED_WARNING);
                        }
                    }
                    ExportStatus::StepReady {
                        bodies,
                        faces,
                        rejected,
                        skipped_meshes,
                    } => {
                        if *bodies == 0 {
                            ui.colored_label(WARNING_COLOR, STEP_NOTHING);
                        } else {
                            ui.label(format!(
                                "{} Körper, {} Flächen",
                                group_digits(*bodies as u64),
                                group_digits(*faces as u64)
                            ));
                        }
                        for r in rejected {
                            ui.colored_label(WARNING_COLOR, r);
                        }
                        if !skipped_meshes.is_empty() {
                            ui.colored_label(WARNING_COLOR, skipped_text(skipped_meshes));
                        }
                    }
                }
                match &write_state {
                    WriteState::Idle => {}
                    WriteState::Writing => {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(write_progress)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text("Die Datei wird geschrieben …"),
                            );
                            if before.is_step() && ui.button("Abbrechen").clicked() {
                                cancel_write = true;
                            }
                        });
                    }
                    WriteState::Done(path, n) => {
                        ui.label(format!(
                            "Fertig: „{}“ geschrieben ({}).",
                            file_name(path),
                            mb_text(*n)
                        ));
                    }
                    WriteState::Failed(m) => {
                        ui.colored_label(ERROR_COLOR, format!("Nicht geschrieben: {m}"));
                    }
                }
                ui.separator();
                match chooser.as_mut() {
                    Some(c) => {
                        ui.strong("Speichern unter");
                        match c.show(ui) {
                            ChooserAction::Chosen(p) => chosen = Some(p),
                            ChooserAction::Cancel => drop_chooser = true,
                            ChooserAction::None => {}
                        }
                    }
                    None => {
                        let ready = match &status {
                            ExportStatus::Ready { .. } => true,
                            ExportStatus::StepReady { bodies, .. } => *bodies > 0,
                            _ => false,
                        } && write_state != WriteState::Writing;
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(ready, egui::Button::new("Exportieren …"))
                                .on_disabled_hover_text(
                                    "Erst möglich, wenn die Berechnung fertig ist.",
                                )
                                .clicked()
                            {
                                begin = true;
                            }
                            if ui.button("Schließen").clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        (self.format, self.quality, self.scope) = (format, quality, scope);
        let stale_chooser = self.chooser_key.is_some_and(|k| k != self.key(state));
        if self.format != before || drop_chooser || stale_chooser {
            self.chooser = None;
            self.chooser_key = None;
        }
        if self.format != before || stale_chooser {
            // Die Meldung „Fertig“ gehörte zum alten Format oder Stand.
            self.written = None;
        }
        let busy = matches!(status, ExportStatus::Computing(_))
            || self.write_state() == WriteState::Writing;
        if busy {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel {
            self.cancel();
        }
        if cancel_write {
            self.cancel_write();
        }
        if recompute {
            self.recompute();
        }
        if begin {
            let dir = last_dir.clone().unwrap_or_else(default_dir);
            self.begin_save(dir, state);
        }
        if let Some(path) = chosen {
            *last_dir = path.parent().map(Path::to_path_buf);
            self.chooser = None;
            self.write_to(path, state);
        }
        open && !close
    }
}
```

In `crates/planora-app/tests/export_dialog.rs` (Test `preparing_shows_triangles_estimate_and_report`) die Zeile

```rust
    d.format = Format::Obj;
```

ersetzen durch (mechanische Anpassung, sonst nichts an dieser Datei):

```rust
    // Mechanisch angepasst (Teilprojekt 10, Task 13): Formate des Fensters als `ExportFormat`.
    d.format = ExportFormat::Mesh(Format::Obj);
```

- [ ] **Step 5: Tests**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test export_step --test export_dialog --test import_dialog --test exchange_infra`
Expected: PASS (9 in `export_step`, 12 in `export_dialog`, 8 in `import_dialog`, `exchange_infra` unverändert grün). Dann `cargo test -p planora-app` ganz.

- [ ] **Step 6: Mutationsprobe (danach zurücksetzen)**

In `write_to`, Zweig `ExportFormat::Step`, die drei Zeilen

```rust
                if *k != key {
                    self.written = Some(Err(STALE.to_string()));
                    return;
                }
```

entfernen → `a_stale_step_result_is_never_written` schlägt fehl. Zurücknehmen und grün sehen.

- [ ] **Step 7: Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`

```bash
git add Cargo.lock crates/planora-app/Cargo.toml crates/planora-app/src/export_dialog.rs crates/planora-app/tests/export_dialog.rs crates/planora-app/tests/export_step.rs
git commit -m "app: STEP als fünftes Format im Fenster Exportieren (Prüfung, Hinweise, Schreiben mit Abbrechen)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 14: Dokumentation und Gesamtprüfung

**Files:**
- Modify: `README.md` (Statuszeile; neuer Abschnitt „STEP-Export (exakte Körper)“ nach „Netz-Austausch (Export und Import)“; im Abschnitt Netz-Austausch die Zeile „Nicht dabei: …“), `docs/superpowers/offene-punkte.md` (Kopf „nach Teilprojekt 1–10“, neuer Abschnitt „Teilprojekt 10 – STEP-Export“ vor „Aufräumen / Qualität“), `docs/superpowers/specs/2026-10-14-step-export-design.md` (§9 ausfüllen)

**Interfaces:**
- Consumes: die Berichte der Tasks 1–13 (Testzahlen, Laufzeiten, Befunde der Stufenreviews).
- Produces: Dokumentation in einfacher Sprache (Laiensprache, wenig Technik, kein Füllstoff), Spec §9 mit P1–P15 und allen Abweichungen.

- [ ] **Step 1: README**

In der Statuszeile oben (`**Status:** frühe Entwicklung (…)`) nach „Netz-Austausch“ ergänzen: „, STEP-Export“.

Im Abschnitt „Netz-Austausch (Export und Import)“ die Zeile

```markdown
- Nicht dabei: STEP-Export, Zeichnungen (SVG/DXF) und das Einlesen von 3MF, glTF oder STEP.
```

ersetzen durch

```markdown
- Nicht dabei: Zeichnungen (SVG/DXF) und das Einlesen von 3MF, glTF oder STEP.
```

Nach dem Abschnitt „Netz-Austausch (Export und Import)“ (vor „## Feature-Baum“) einfügen (Inhalt nur kürzen, wo Befunde es verlangen):

```markdown
## STEP-Export (exakte Körper)

Körper lassen sich als STEP-Datei speichern. Andere CAD-Programme öffnen sie als echte Volumenkörper, nicht als Dreiecksnetz: Menü „Datei“ → „Exportieren …“ (Strg+Umschalt+E), Format „STEP – für andere CAD-Programme (exakt)“.

- Rundungen, Bohrungen und Freiformflächen bleiben exakt. Deshalb gibt es bei STEP keine Auswahl der Qualität.
- Mehrere Körper kommen in eine Datei, jeder als eigenes Teil mit seinem Namen aus dem Feature-Baum. Ein Körper mit Hohlraum bleibt ein Körper mit Hohlraum; getrennte Stücke eines Körpers bleiben ein Teil.
- Eingelesene Netz-Körper kann STEP nicht aufnehmen. Das Fenster nennt sie mit Namen; für sie STL, OBJ, 3MF oder glTF wählen.
- Vor dem Schreiben prüft das Programm jeden Körper. Wer die Prüfung nicht besteht, wird ausgelassen und mit Grund genannt; die anderen werden geschrieben.
- Das Fenster zeigt vorher, wie viele Körper und Flächen es werden. Das Schreiben läuft im Hintergrund mit Fortschrittsbalken und lässt sich abbrechen. Eine vorhandene Datei wird erst ersetzt, wenn alles geklappt hat.

**Grenzen**
- In der Datei stehen nur Form, Namen und die Einheit Millimeter; keine Farben, Materialien oder Baugruppen. STEP-Dateien einlesen kann das Programm noch nicht.
- Beim Bauen prüft das Programm seine Dateien mit einem eigenen Leser. Mit einem fremden CAD-Programm ist das nur im Handtest geschehen (siehe offene Punkte); andere Programme können strenger sein.
- Kugeln und Kegelspitzen werden so geschrieben, wie es viele Programme tun: Der Pol ist ein Eckpunkt, die Naht läuft hin und zurück.
- Körper mit mehreren Schalen braucht das Programm zum Unterscheiden von Hohlraum und Stück ein grobes Netz; lässt es sich nicht berechnen, wird der Körper mit Grund ausgelassen.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 10 – STEP-Export“.
```

- [ ] **Step 2: Offene Punkte** – Kopfzeile `# Offene Punkte nach Teilprojekt 1–9` in `# Offene Punkte nach Teilprojekt 1–10` ändern und vor `## Aufräumen / Qualität` einen Abschnitt `## Teilprojekt 10 – STEP-Export` mit Stand-Datum anlegen. Er enthält mindestens diese Punkte, jeweils mit den gemessenen Zahlen aus den Berichten (Testanzahl aus `cargo test --workspace` in Step 4, Laufzeit der langen Reihe aus Task 12):
  - Prüfung ohne fremdes Programm: eigener Leser, Modell mit eigenen Formeln, Topologie- und Euler-Prüfung, Kernelvergleich, Netz aus den STEP-Flächen; die einzige echte Gegenprobe ist der Handtest (Task 15, Spec §8). Ein Absatz „Handtest“ (Task 15 ergänzt ihn).
  - Pole und Kegelspitzen als Eckpunkt mit Naht hin und zurück (Spec §3); Ausweichweg (Pol als `VERTEX_LOOP` oder Fläche in zwei Hälften teilen) nur bei Befund im Handtest, als eigene Folge-Aufgabe.
  - Doppelte Rechenzeit: jeder Körper wird vor dem Schreiben einmal probeweise geschrieben (P3); das Fenster prüft dazu schon beim Öffnen (`prepare_step`), also dreimal insgesamt. Bei sehr großen Freiformkörpern spürbar.
  - Hohlräume über ein grobes Netz (`CLASSIFY_CHORD = 0,05 mm`, P9): Scheitert das Netz, wird der Körper benannt ausgelassen.
  - Unsicherheit 1e-5 mm bei Körpern mit Freiform (P10); fremde Programme heilen Kanten bis zu dieser Größe.
  - Torus mit kleinem Radius ≥ großem Radius (Spindeltorus) wird als `TOROIDAL_SURFACE` geschrieben; manche Programme erwarten dafür `DEGENERATE_TOROIDAL_SURFACE` (nicht beobachtet, kein Test).
  - Kanten über die Naht geschlossener Freiformkurven (der Kernel faltet `t` über das Bereichsende) werden über ihre Eckpunkte begrenzt; ob fremde Programme bei offenen, nur als geschlossen gekennzeichneten B-Splines über das Ende laufen, ist ungeprüft.
  - Abbrechen: beim Prüfen zwischen den Körpern und alle 4096 Einträge, beim Schreiben alle 4096 Einträge.
  - Der Kernel rechnet schräge Zylinder mit Zylindern oder Kugeln noch nicht (beim Planen in der Zufallsreihe beobachtet); die Reihe deckt deshalb nur Quader mit Quader, Zylinder oder Kugel ab.
  - Fenster: `ExportFormat` statt `Format` (P14) samt mechanischer Testzeile.

- [ ] **Step 3: Spec §9** – den Platzhalter „(wird beim Abschluss des Teilprojekts ergänzt)“ in §9 ersetzen durch: Stand-Datum; eine Tabelle P1–P15 („Entscheidung“, „Begründung und Ergebnis“) wie in diesem Plan, je Zeile ergänzt um das Ergebnis aus der Umsetzung; danach „Weitere Abweichungen von §1–§8“ mit mindestens: zusätzliche Pakete `planora-exchange` und `planora-mesh` (§2, P1); Modul `file.rs` (§2); `EDGE_CURVE` mit `same_sense = (t1 ≥ t0)` statt immer `.T.` (§3, P5); `GEOMETRIC_REPRESENTATION_ITEM()` in den rationalen B-Splines (§3, P8); Hohlraumschale umgekehrt mit `ORIENTED_CLOSED_SHELL .F.` und getrennte Stücke als mehrere Körper eines Teils (§3, P9); `NotFinite` eines Körpers als Ablehnung dieses Körpers (§4, P3); leere Namen als „Körper n“ (P12); gemeinsamer Kontext statt je Körper (§2, P11); Kegel-Lage auf der Flächenmitte (§3, P6); dazu die Befunde der Stufenreviews.

- [ ] **Step 4: Gesamtprüfung**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p planora-step --release --test stage3 -- --ignored a_long_random_series
cargo deny check
git diff --stat 07bac2b -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs
git diff --stat 07bac2b -- crates/planora-kernel crates/planora-math crates/planora-mesh crates/planora-exchange crates/planora-feature crates/planora-project crates/planora-render crates/planora-sketch
```
Expected: alles grün; beide `git diff`-Ausgaben leer. Testzahl (bestanden/ignoriert) und Laufzeit der Release-Reihe in `offene-punkte.md` und Spec §9 eintragen. Schlägt eine Reihe fehl, ist das ein Befund für den Bericht (nicht ignorieren, nicht abschwächen).

- [ ] **Step 5: Commit**

```bash
git add README.md docs/superpowers/offene-punkte.md docs/superpowers/specs/2026-10-14-step-export-design.md
git commit -m "docs: STEP-Export – README, offene Punkte, Änderungen bei der Umsetzung

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 15: Handtest mit Gegenprobe in Fusion 360 (nur nach Rücksprache mit dem Nutzer, vom Controller ausgeführt)

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Abschnitt Teilprojekt 10, Absatz „Handtest“)

**Interfaces:**
- Consumes: Release-Build der App, `tools/gui-drive.ps1` (Befehle `shot`, `click`, `dclick`, `rclick`, `drag`, `wheel`, `type`; Screenshot nach `%TEMP%\planora_shot.png`), der Test `write_samples_for_the_hand_test` (Task 12), ein vom Nutzer angemeldetes Fusion 360.
- Produces: Bericht des Handtests (was geprüft, was nicht); Befunde als Folge-Aufgaben.

- [ ] **Step 1: Rückfrage** – Den Nutzer fragen, ob der Handtest jetzt laufen darf (er sitzt am Rechner; das Skript übernimmt Maus und Tastatur, Fusion 360 wird bedient) und ob Fusion 360 installiert und angemeldet ist. Ohne ausdrückliches Ja nicht fortfahren. Anmeldedaten gibt nur der Nutzer selbst ein. Implementierer führen diese Task nie aus, nur der Controller. Aus Fusion 360 wird nichts übernommen (kein Autodesk-Material), nur angesehen und gemessen.

- [ ] **Step 2: Beispieldateien** (PowerShell; vorübergehender Ordner)

```powershell
$tmp = Join-Path $env:TEMP ("planora_step_hand_" + [guid]::NewGuid())
New-Item -ItemType Directory -Force (Join-Path $tmp "proben") | Out-Null
$env:PLANORA_STEP_SAMPLES = Join-Path $tmp "proben"
cargo test -p planora-step --release --test stage3 write_samples_for_the_hand_test -- --ignored --nocapture
```

Die ausgegebenen Zeilen (Flächenzahl und Volumen je Datei) notieren.

- [ ] **Step 3: Weg über das Fenster** (vorübergehende Benutzerordner, damit keine echten Projekte berührt werden)

```powershell
cargo build --release -p planora-app
New-Item -ItemType Directory -Force (Join-Path $tmp "Documents") | Out-Null
$env:APPDATA = $tmp; $env:USERPROFILE = $tmp
Start-Process -FilePath .\target\release\planora-app.exe -RedirectStandardOutput "$tmp\out.log" -RedirectStandardError "$tmp\err.log"
```

Je Schritt `powershell -File tools/gui-drive.ps1 <Befehl> …` und den Screenshot ansehen:
1. Quader 40 × 30 × 20, eine Kante mit Radius 5 runden, Zylinder r = 8, h = 25 daneben; ein STL importieren (Netz-Körper).
2. „Datei → Exportieren …“: Format „STEP – für andere CAD-Programme (exakt)“ wählen. Fenster über die ganze Breite, drei gleich hohe Karten, mittlere Karte „Rundungen und Bohrungen bleiben exakt.“; Anzeige „2 Körper, … Flächen“ und der Hinweis auf den Netz-Körper mit Namen.
3. „Exportieren …“ → Dateiauswahl schlägt `Körper.step` vor → „Speichern“ → „Fertig: „Körper.step“ geschrieben (…)“. Noch einmal speichern → „Diese Datei gibt es schon …“, „Ersetzen“.
4. Format zurück auf STL: Dreiecksanzeige erscheint sofort wieder (kein neues Rechnen), die Fertig-Zeile ist weg.

- [ ] **Step 4: Gegenprobe in Fusion 360** (vom Controller mit Bildschirmsteuerung)

Für `Körper.step` und jede Datei aus Step 2 (mindestens `Alle.step`, `Kugel.step`, `Kegel.step`, `Hohl.step`, `Variabel.step`, `Kreuz.step`): in Fusion 360 „Datei“ → „Öffnen“ → „Von meinem Computer öffnen …“, die Datei wählen, Hochladen und Übersetzen abwarten. Prüfen und notieren:
- Die Körper erscheinen im Browser unter „Körper“ als Volumenkörper (nicht als Netz, keine Fehlermeldung beim Übersetzen); bei `Alle.step` je Körper ein eigenes Teil mit dem Namen aus der Liste.
- Körper wählen → „Prüfen“ → „Eigenschaften“: Volumen gegen die Zahl aus Step 2 (Abweichung höchstens 0,1 %), Flächenzahl gegen die Ausgabe.
- `Hohl.step`: Volumen mit Hohlraum (936 mm³); `Kugel.step` und `Kegel.step`: geschlossene Volumenkörper trotz Pol bzw. Spitze als Eckpunkt.

- [ ] **Step 5: Aufräumen und berichten** – Programme beenden, `$tmp` löschen (`Remove-Item -Recurse -Force $tmp -Confirm:$false`). In `docs/superpowers/offene-punkte.md` im Abschnitt „Teilprojekt 10 – STEP-Export“ den Absatz „Handtest“ ergänzen: Datum, Build, Fusion-360-Version, welche Dateien geöffnet wurden, Volumen und Flächenzahlen im Vergleich, was nicht geprüft wurde (etwa FreeCAD, SolidWorks, Onshape, sehr große Körper, Linux). Ein Befund (etwa: Pole werden nicht angenommen) wird als Folge-Aufgabe mit dem Ausweichweg aus Spec §3 eingetragen, nicht unter Zeitdruck nachgebessert.

```bash
git add docs/superpowers/offene-punkte.md
git commit -m "docs: Handtest STEP-Export mit Gegenprobe in Fusion 360

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---


## Anhang: Selbstprüfung

### Abdeckung der Spec

| Spec | Inhalt | Task |
|---|---|---|
| §1, E1 | AP214 `AUTOMOTIVE_DESIGN`, Textdatei, `.step`; nur Geometrie, Namen, Einheit | 3 (`the_header_names_schema_time_and_units`), 13 (Endung) |
| §1, E2 | eine Datei, je Körper ein Teil mit Namen aus dem Feature-Baum | 3, 6 (`several_bodies_become_several_named_parts`), 13 (Namen aus `AppState::bodies`) |
| §1, E3 | Netz-Körper nicht ausgegeben, Dialog nennt sie, Verweis auf Netzformate | 13 (`step_counts_bodies_and_faces_and_names_the_mesh_bodies`, `the_window_shows_step_texts_without_the_quality_card`, `only_mesh_bodies_leave_nothing_to_write`) |
| §1, E4 | nur Schreiben | ganzer Plan (kein Leser im Paket; der Leser steht nur in `tests/common`) |
| §1, E5 | Handtest in Fusion 360 nach Rückfrage; beim Bauen nur eigener Leser und Formeln | 15; 4–6, 9, 12 |
| §1, E6 | keine neue externe Abhängigkeit, kein Autodesk-Material | Global Constraints, `cargo deny check` in 6, 9, 11, 12, 14 |
| §2 | Paket `planora-step` mit lib/error/number/text/entity/geometry/topology/product | 1, 2, 3 (dazu `file.rs` in 6, P1) |
| §2 | `write_step(bodies, opts, out, progress)`, `StepBody`, `StepOptions`, `StepReport` (Körper, Flächen, Kanten, Einträge) | 3 (P2) |
| §2 | Kopf `FILE_DESCRIPTION`, `FILE_NAME`, `FILE_SCHEMA`, Zeitpunkt UTC | 3 (`the_header_names_schema_time_and_units`, `timestamps_are_utc`), 6 (`the_header_is_read_back_by_the_own_reader`) |
| §2 | Kontext mit Unsicherheit, Millimeter, Radiant, Steradiant | 3, 5 (`the_model_refuses_wrong_units_and_skewed_axes`), 10 (P10) |
| §2 | je Körper Produkt, Definition, Form, `ADVANCED_BREP_SHAPE_REPRESENTATION` mit Ursprungslage, `MANIFOLD_SOLID_BREP` bzw. `BREP_WITH_VOIDS` | 3, 5 (Modell prüft die Kette), 11 |
| §3 | Abbildung Solid/Face/Loop/HalfEdge/Edge/Vertex | 3, 6 (`compare_with_kernel`) |
| §3 | Ebene, Zylinder, Kugel, Torus | 3, 7 |
| §3 | Kegel: Radius ≥ 0, Öffnungswinkel > 0, Lage verschoben bzw. Achse umgekehrt, Punktprobe | 8 (P6) |
| §3 | NURBS-Flächen und -Kurven, rational als zusammengesetzter Eintrag, Knoten mit Vielfachheiten, geschlossen `.T.` | 10 (P8) |
| §3 | Gerade, Kreis, Ellipse | 3, 7 (P7) |
| §3 | `same_sense == false` → `.F.`, Fläche nicht gespiegelt | 3, 6 (Normalenprobe), 7–11 |
| §3 | Nähte als ein `EDGE_CURVE` zweimal entgegengesetzt, Pole/Spitzen als Eckpunkte, Vollkreise mit einem `VERTEX_POINT` | 7 (`a_cylinder_has_one_seam…`, `a_sphere_is_one_face_with_poles_as_vertices`), 8 (`the_apex_of_a_cone_is_a_vertex`), 10 (`poles_of_a_freeform_sphere…`) |
| §3 | Fallback für Pole nur bei Befund im Handtest | 15 (Folge-Aufgabe), 14 (offene Punkte) |
| §4 | REAL kürzeste Darstellung, Punkt, `-0` → `0.`, `NotFinite` | 1 (`reals_always_have_a_point_and_read_back_exactly`, `non_finite_numbers_are_refused…`), 3/6 (Körper mit NaN/∞) |
| §4 | Texte maskiert, Steuerzeichen, 256 Zeichen mit Bericht | 1, 6 (`long_names_are_shortened_and_reported`) |
| §4 | `validate`, `InvalidBody` ausgelassen, `NothingToExport` | 3 (`invalid_bodies_are_left_out_by_name_and_the_rest_is_written`, `dangling_references_are_refused_without_a_crash`), 6, 13 |
| §4 | zeilenweise, `u64`, `MAX_ENTITIES`, Abbruch über `Progress` | 2 (`the_entity_limit_is_a_named_error`, `cancelling_stops_at_the_next_check…`), 3 (`cancelling_during_writing_is_a_named_error`) |
| §4 | Hilfsdatei mit Umbenennen | 6 (`files_go_through_a_helper_and_failures_keep_the_old_file`), 13 (`cancelling_step_writing_keeps_the_old_file`) |
| §5.1 | eigener Leser: Einträge, zusammengesetzte Einträge, Syntax, Verweise, normierte Richtungen, senkrechte Koordinatensysteme | 4, 5 |
| §5.2 | Geometrie unabhängig nachrechnen (Ebene, Zylinder, Kegel, Kugel, Torus, NURBS), Eckpunkte auf Kurven, Kanten auf beiden Flächen, benannte Toleranz | 5 (Formeln), 6 (`check_geometry`, `compare_with_kernel`, `GEOM_TOL`, `FREEFORM_TOL`) |
| §5.3 | Topologie: Kanten je Hülle zweimal entgegengesetzt, Schleifen geschlossen, Euler | 6 (`check_topology`), Mutationsprobe in 6 |
| §5.4 | Formeln für Quader, Zylinder, Kegel, Kugel, Torus, gerundete Körper, Freiform: Fläche und Volumen aus den STEP-Flächen | 6 (ebene Körper), 9, 10, 11, 12 (P13) |
| §5.5 | feindliche Eingaben: offene Hülle, NaN, leere Namen, Sonderzeichen, sehr viele Körper | 3, 6 (`many_bodies_fit_into_one_file` mit 200 Körpern) |
| §5.6 | Handtest in Fusion 360 | 15 |
| §6 | fünftes Format, Karte „Qualität“ entfällt, „Rundungen und Bohrungen bleiben exakt.“ | 13 |
| §6 | Umfang wie bisher, Netz-Körper ausgelassen mit Hinweis | 13 |
| §6 | „n Körper, n Flächen“, abgelehnte Körper mit Grund | 13 (`rejected_bodies_are_listed_with_their_reason`) |
| §6 | Arbeitsthread mit Fortschritt und Abbrechen, Hilfsdatei, Ersetzen-Rückfrage, Schlüsselprüfung | 13 (`a_stale_step_result_is_never_written`, Mutationsprobe), Ersetzen über `FileChooser` (unverändert, Handtest 15) |
| §6 | eigene Dateiauswahl, Endung `.step` | 13 (`writing_step_goes_to_the_chosen_file`) |
| §7 | vier Stufen mit Stufentests | 6, 9, 12, 13–15 |
| §8 | Risiken: Fremdprogramme strenger, Kegel/Torus-Vorzeichen, Knoten/Gewichte, große Körper, Namen | 15; 7, 8 (Lagen gespiegelt/gedreht); 10 (eigenes de Boor); 2 (Strom); 1, 6 |
| §9 | Änderungen bei der Umsetzung | 14 |

### Platzhalter-Prüfung

Der Plan enthält keine offenen Stellen im Code: Jede Datei steht vollständig im Plan (geänderte Dateien „ganz ersetzen“, dazu die zwei einzeiligen Einfügungen in den `Cargo.toml`-Dateien und die eine mechanische Testzeile). Die Zweige „… wird noch nicht ausgegeben“ in Task 3 sind beabsichtigte, benannte Ablehnungen eines Zwischenstands und werden in Task 7, 8, 10 und 11 durch die fertigen Zweige ersetzt; kein Test prüft sie. Zahlen, die erst die Umsetzung liefert (Testanzahl, Laufzeiten, Ergebnisse des Handtests), werden in Task 14 und 15 aus den genannten Ausgaben eingetragen. Beim Planen gemessen: `cargo test --workspace` 1390 bestanden, 0 fehlgeschlagen, 30 ignoriert (vor dem Beispieldatei-Test aus Task 12, der einen weiteren ignorierten Test hinzufügt).

### Typkonsistenz (geprüft)

- `StepError` (Task 1) mit denselben Varianten in Task 2–13; `StepError::Cancelled.to_string() == "Abgebrochen"` wie `ExchangeError::Cancelled` (das Fenster zeigt beide gleich).
- `Entities::{new, probe, push, line, count, flush, set_next}`, `refs`, `logical` (Task 2) in Task 3, 7, 8, 10, 11 unverändert.
- `StepBody { name: String, solid: &Solid }`, `StepOptions { file_name, time, tolerance }`, `StepOptions::new(&str)`, `StepReport`, `BodyStats`, `check_body(&StepBody, &Progress)`, `write_step`, `display_name` (Task 3) in Task 6 (`export_step_file`), 12 und 13 (`prepare_step`, `write_step_bodies`).
- `geometry::surface(e, s)` (Task 3, 7) wird in Task 8 zu `surface(e, s, patch)`; der einzige Aufrufer `TopoWriter::face` wird in derselben Task angepasst. `TopoWriter::face(e, id)`/`closed_shell(e, faces)` werden in Task 11 zu `face(e, id, reversed)`/`closed_shell(e, faces, reversed)`, ebenfalls mit dem einzigen Aufrufer `write_solid`.
- Leser: `reader::{parse, StepFile, Entity, Record, Value}` (Task 4) → `model::Model::read` (Task 5) → `check::{check_topology, check_geometry, compare_with_kernel, measure, face_map}` (Task 6) → `common::checked` (Task 6) in Task 6–12; `Measure { volume, area, kernel_volume, flipped, max_offset }` überall gleich benutzt; `Surf::foot(p, &mut hint)` nur in `measure`.
- Fenster: `ExportFormat::{Mesh(Format), Step}`, `ExportStatus::StepReady { bodies, faces, rejected, skipped_meshes }`, `StepPrepared { bodies, faces, rejected, skipped_meshes }` (Task 13) in den Tests derselben Task; `WriteState::Writing` bleibt ohne Wert (das Import-Fenster und die alten Tests bleiben unberührt).

### Selbstkorrekturen beim Planen

Beim Übersetzen und Testen der Kopie aufgefallen und im Plan schon behoben:
1. Knotenlisten der B-Spline-Flächen standen in falscher Reihenfolge (`mu, ku, mv, kv` statt `mu, mv, ku, kv`); der eigene Leser fand es (Task 10).
2. Die Fußpunktsuche auf geschlossenen Freiformflächen blieb an der Naht hängen (Begrenzen statt Falten) und an Polen (Newton ohne brauchbare Ableitung); jetzt Falten über die Naht, Mustersuche als Rückfall, Normale etwas vom Rand weg (Task 5).
3. Die Prüfung der Freiformkörper dauerte im Debug-Build 132 s; mit Startwert vom Nachbarpunkt und einmaliger Fußpunktsuche je Netzpunkt 4 s (Task 5, 6).
4. An der Kegelspitze ist die Normale unbestimmt; die Normalenprobe überspringt Punkte auf der Achse (Task 5).
5. `is_multiple_of` gibt es nur für vorzeichenlose Zahlen; die Euler-Prüfung nutzt `% 2 != 0` (Task 6).
6. Die Testhilfe für vertauschte Ellipsen verschob den Parameter mit dem falschen Vorzeichen; die Richtungsprüfung fand es (Task 7).
7. Analytische Zylinder untereinander und mit Kugeln schräg zur Achse rechnet der Kernel noch nicht; der Kreuzkörper nutzt einen NURBS-Zylinder, die Zufallsreihe nur unterstützte Paare (Task 12).
8. Schranken der Formeltests aus der Sehnentoleranz hergeleitet und gemessen (Task 9), statt einer festen relativen Schranke, die die Kugel nicht hielt.
9. Fertige Zahlen in Tests nachgerechnet (L-Profil: 64 mm² Grundfläche, nicht 52).
10. `WriteState::Writing` bleibt ohne Wert, der Schreibfortschritt kommt über `write_progress()`, damit bestehende Tests unverändert bleiben (Task 13).
11. Heredocs in Git Bash verschluckten Rückstriche; Hinweis in den Global Constraints.
