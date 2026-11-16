# Skizzen 11a Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Ziel:** Skizzen zeichnen und prüfen wie in Autodesk Fusion: Mittelpunktlinie, Übergangskurve (G1/G2), Krümmungskamm, Skizzenstatus mit „Unterbestimmte finden“ und rotem Schloss, verlorene Projektionen als Warnung statt Mitlöschen, ein dünn rechnender Löser für lange Ketten und die Nacharbeiten A6–A9.

**Aufbau:** Erst wird der Löser gemessen und für große Gruppen dünn (RCM-Ordnung, Hüllen-Cholesky, dünne Rangprüfung, Analyse beim Ziehen aus dem Zwischenspeicher) gerechnet, kleine Gruppen bleiben auf dem dichten Weg. Danach kommen die Modellbausteine (`EndTangent`, Glatt am Ellipsenbogen, Kamm, Gegenwinkel) samt Format 21, darauf die Werkzeuge in `planora-app`. Zum Schluss werden Projektionsbezüge in `planora-feature` schwach, und die App verwaltet verlorene Projektionen.

**Technik:** Rust, egui/eframe 0.36, wgpu. Keine neue Bibliothek (auch nicht für dünne Matrizen).

**Spec:** `docs/superpowers/specs/2026-11-15-skizzen-pruefen-design.md` (freigegeben, maßgeblich). Jede Aufgabe liest die Spec mit, besonders „Annahmen“ A1–A10.

## Globale Vorgaben

- Zweig `feature/skizzen-pruefen`; keine Zweigwechsel, kein `git stash`.
- Texte nur über `tr!`/`msg!`, Schlüssel in `crates/planora-i18n/languages/de.ftl` und `en.ftl`. Namen: „Mittelpunktlinie“/„Midpoint Line“, „Übergangskurve“/„Blend Curve“, „Tangential“/„Tangent“ (G1), „Krümmung“/„Curvature“ (G2), „Krümmungsanzeige ein/aus“/„Toggle Curvature Display“, „Krümmungsanzeige einrichten“/„Curvature Display Settings“, „Dichte“/„Density“, „Maßstab“/„Scale“, „Krümmungskamm“/„Curvature Comb“, „Unterbestimmte finden“/„Find Under-Constrained“, „Skizze berechnen“/„Compute Sketch“, „Bestimmte ausblenden“/„Hide Fully Constrained“, „Verlorene Projektionen verwalten“/„Manage Lost Projections“, „Neu verknüpfen“/„Relink“, „Verknüpfung aufheben“/„Break Link“, „Geometrie löschen“/„Delete Geometry“, „Alle löschen“/„Delete All“, „Alle Verknüpfungen aufheben“/„Break All Links“, „Übrige Geometrie abblenden“/„Dim Other Geometry“, „tangential“ im Browser für `EndTangent`.
- Statuszeile: „Status: Unterbestimmt“/„Status: Under Constrained“, „Status: Bestimmt“/„Status: Fully Constrained“, „Status: Widerspruch“/„Status: Conflict“ (rot, `theme.error`), „Status: nicht gelöst“/„Status: Not Solved“ (orange, `palette.warning`).
- Neue Aktionen in `action_table!` (`crates/planora-app/src/keymap/defaults.rs`), alle ohne Vorbelegung (`[]`): `SketchMidpointLine` `"skizze.mittelpunktlinie"`, `SketchBlendCurve` `"skizze.uebergangskurve"`, `SketchToggleCurvature` `"skizze.kruemmungsanzeige"`, `SketchFindUnderConstrained` `"skizze.unterbestimmte_finden"`, `SketchCompute` `"skizze.berechnen"`, `ManageLostProjections` `"modell.verlorene_projektionen"` (Bereich `Model`).
- Neue Dialoge mit `.dock_key(..)` aus `detach::DIALOG_KEYS`: `"curvature_display"` und `"lost_projections"` (die Liste schreibt Schlüssel mit Unterstrich; die Spec nennt „lost-projections“). Dialog-Closures leeren außen angelegte Listen am Anfang. Test `dialog_dock_keys` bleibt grün.
- Ein Befehl = ein Rückgängig-Schritt; Abbruch (Esc, Rechtsklick, Werkzeugwechsel) hinterlässt nichts.
- Dateiformat: `FORMAT_VERSION = 21` (`crates/planora-project/src/convert.rs`). 21 wird nur geschrieben, wenn `needs_format_21` zutrifft: Kamm an einer Kurve, `Angle` mit Gegenwinkel, `EndTangent`, `Smooth` mit Ellipsenbogen, auch in gelöschten Einträgen. Mittelpunktlinie und G2-Übergangskurve ohne Ellipsenbogen ändern das Format nicht. Alte Dateien bytegleich.
- Löser: 2D löst nicht anders. Toleranzen bleiben `RANK_REL_TOL = 1e-7`, `FREE_COMPONENT = 1e-6`, `RESIDUAL_FRACTION = 1e-2`. Dichter Weg unter `SPARSE_MIN_VARS` Variablen je Gruppe (Startwert 60, endgültig nach Task 3), darüber der dünne Weg mit gleicher Lösung (Variablen bis 1e-9, gleiche `dof`, `redundant`, `conflicting`, `free_points`, `free_curves`).
- Messziele (Release): 3D-Kette 200 Kurven lösen ≤ 0,5 s, ein Ziehschritt ≤ 50 ms; 2D-Messtests höchstens 5 % langsamer als die Werte aus Task 1.
- Kamm: Dichte Vorgabe 20 (2–100), Maßstab Vorgabe 1,0 (0,01–100), größter Zahn = `max(0,15 · Kurvenlänge, 10 px)` · Maßstab, Farbe `SketchColors::comb`.
- Übergangskurve: G1 = Kontrollpunkt-Spline Grad 3 mit 4 Kontrollpunkten, G2 = Grad 5 mit 6; Hebel = 1/3 des Endabstands; Vorgabe G2.
- Kommentare deutsch. Kein Autodesk-Material. `cargo deny check` grün.
- Prüfen je Aufgabe: `cargo fmt -p <crate> -- --check`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`. Nie zwei cargo-Läufe gleichzeitig, bei „os error 1455“ mit `-j 4`.
- Code-Fragen zuerst mit `graphify query "<frage>"` (Graph in `graphify-out/`), dann gezielt lesen; das gilt auch für Briefings an Subagenten.
- Commit je Aufgabe: `git commit -m "Skizzen 11a: <Titel>" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"` (deutsche Nachricht, Leerzeile, Co-Author-Zeile).
- Nach jeder Aufgabe: Vault-Notiz `D:\Vault\Vault\Projekte\PlanoraCAD\2026-MM-TT Skizzen 11a Task N <Titel>.md` mit Frontmatter `aktualisiert: 2026-MM-TT` und `tags: [planoracad]`, Abschnitte **Geändert** / **Warum** / **Offen**, verlinkt in `Projekte/PlanoraCAD/PlanoraCAD.md` unter „Aufgaben-Notizen“. Keine Schlüssel oder Tokens.
- Handtests nur nach Dennis' Go, jedes Mal neu fragen, bei 1920×1080 mit `tools/gui-drive.ps1`, APPDATA/USERPROFILE auf einen Testordner; danach Test-Builds und Fotos im Scratchpad löschen.

## Prüf-Schwerpunkte

1. **Übergangskurve zwischen zwei Enden, die schon ein Punkt sind** (zwei Linien mit gemeinsamer Ecke, beide Enden an der Ecke gewählt): Nichts wird angelegt, Meldung „Die Enden liegen aufeinander“, kein Rückgängig-Schritt. Test `blend_between_joined_ends_refused` in Task 9.
2. **Skizze in der Zeitleiste vor ihre Projektionsquelle schieben und wieder zurück:** Davor ist der Bezug verloren (Warnung, Kurve an letzter Lage, kein Fehler), zurück lebt er wieder auf, die Kurve folgt der Kante. Test `move_sketch_before_source_and_back` in Task 17.
3. **Krümmungskamm an einer geraden Kurve mit Kammart** (Kontrollpunkt-Spline mit Kontrollpunkten auf einer Geraden, Krümmung überall 0): kein NaN, alle Zähne haben Länge 0, Hülle = Kurve. Test `comb_on_straight_spline_has_zero_teeth` in Task 5.
4. **Liste „Unterbestimmte finden“ nach Löschen und Rückgängig:** Ein per X gelöschtes Element verschwindet aus der Liste; nach Rückgängig steht es wieder darin; ein Zeilenklick auf einen veralteten Eintrag tut nichts (keine tote Zeile, kein Absturz). Test `under_list_follows_delete_and_undo` in Task 16.
5. **Widerspruch in einer großen 2D-Gruppe** (eingefügte Bogenkette mit 500 Bögen, danach ein widersprüchliches Maß, dünner Weg): Status „Widerspruch“ und dieselbe Liste `conflicting` wie auf dem dichten Weg. Test `sparse_conflict_names_same_constraints` in Task 2.

## Ledger

Vom Ausführenden in Task 1 und Task 3 auszufüllen (Release, dieselbe Maschine, je Test der Median aus 3 Läufen):

| Messung | vor dem Zweig | nach Task 3 | Ziel |
|---|---|---|---|
| `perf_200_chained_3d` lösen | 8,64 s (neu gemessen in Task 3 auf e9811c5b: 7,18 s) | 0,018 s | ≤ 0,5 s |
| `perf_200_chained_3d` Zugschritt | 36,1 s (neu gemessen in Task 3 auf e9811c5b: 30,35 s; Läufe 30,36 / 30,28 / 30,35 s) | 0,026 s (Läufe 0,026 / 0,025 / 0,027 s; nach Fix-Runde 1 mit Zugabbruch auf dem dünnen Weg, Ruling T3-3; vorher 0,120 s) | ≤ 50 ms |
| Aufteilung 3D: Jacobi / Normalgl. / Cholesky / Rang | lösen 27 / 72 / 2 105 / 5 780 ms (Summe 8,0 s); Zugschritt 290 / 1 025 / 24 735 / 5 064 ms (Summe 31,1 s) | lösen 10 / 1 / 1 / 3 ms (Rest 2 ms); Zugschritt 18 / 2 / 2 / 0 ms (Rest 4 ms) | – |
| `perf_200_chained_2d` lösen / Zugschritt | 1,97 s / 12,26 s (Aufteilung lösen: Jacobi 12 / Normalgl. 23 / Cholesky 619 / Rang 1 205 ms; Zugschritt 142 / 329 / 9 360 / 1 331 ms) | 0,020 s / 0,015 s (lösen 4 / 0 / 0 / 14 ms) | ≤ +5 % |
| `perf_2d_unchanged` einfügen / Zugschritt | 9,79 s / 16,02 s | 0,007 s / 0,006 s | ≤ +5 % (große Gruppe: schneller) |
| `import_perf` (alle `--ignored`) | 100 Bögen 0,060 s / 0,205 s; 250 Bögen 1,03 s / 1,65 s; 500 Bögen 12,2 s / 19,1 s; 1000 Bögen 78,3 s / 131,8 s (einfügen / Zugschritt); 10 000 Linien 0,008 s; Profile 100 000 Linien 32,9 s | 100 Bögen 0,001 s / 0,001 s; 250 Bögen 0,002 s / 0,003 s; 500 Bögen 0,007 s / 0,015 s; 1000 Bögen 0,025 s / 0,012 s; neu auch 2500 Bögen 0,155 s / 0,030 s; 10 000 Linien 0,008 s; Profile 100 000 Linien 25,15 s | ≤ +5 % |
| Schwelle `SPARSE_MIN_VARS` | 60 (Start) | **30** (`sparse_threshold_crossover`, Median aus 21: bei 20 Variablen lösen dicht 0,079 / dünn 0,084 ms, Zug 0,731 / 0,680 ms; bei 30 Variablen lösen 0,148 / 0,136 ms, Zug 1,282 / 1,105 ms, ab da dünn immer schneller) | Kreuzungspunkt dicht/dünn |
| **Abschluss** (Windows, Release, Median aus 3 Läufen, 2026-11-16) | – | `perf_200_chained_3d` lösen 0,020 s, Zugschritt 0,030 s (Läufe 0,030 / 0,028 / 0,030); `perf_200_chained_2d` lösen 0,021 s / Zugschritt 0,016 s; `perf_2d_unchanged` einfügen 0,008 s / Zugschritt 0,006 s; `import_perf` (einfügen / Zugschritt): 100 Bögen 0,001 / 0,001 s, 250 Bögen 0,002 / 0,003 s, 500 Bögen 0,007 / 0,007 s, 1000 Bögen 0,027 / 0,013 s, 2500 Bögen 0,148 / 0,033 s; 10 000 Linien 0,009 s; Profile 100 000 Linien 32,1 s; Schwelle 30 bestätigt (bei 30 Variablen Zug dünn 0,217 ms gegen 1,280 ms dicht) | 3D lösen ≤ 0,5 s und Zugschritt ≤ 50 ms erfüllt; 2D weit unter den Werten vor dem Zweig; gegenüber „nach Task 3“ liegen 2D lösen und Zugschritt um 0,001 s höher, das ist Messrauschen |

Stand Task 3: Release, Median aus 3 Läufen (`--test-threads 1`), Rechner ohne andere cargo-Läufe. Die Spalte „vor dem Zweig“ für die 3D-Kette wurde auf e9811c5b (Zeitzähler eingebaut, Löser unverändert) neu gemessen, weil Task 1 36,1 s und 31,7 s zeigte; heute 7,18 s / 30,35 s. Nach Fix-Runde 1 (Ruling T3-3) endet die weiche Zugphase auf dem dünnen Weg früher; 3D-Zug 0,026 s. Vorher verfehlte der 3D-Zugschritt das Ziel: Rang und Cholesky sind fast null, es bleiben 100 Levenberg-Marquardt-Schritte (die unterbestimmte Kette läuft bis `MAX_ITERATIONS`) mit je einem Aufbau der Jacobi-Matrix (598 Abhängigkeiten, etwa 0,9 ms je Aufbau, davon `Tangent` 3D etwa die Hälfte).

Stand Task 1: Release, Median aus 3 Läufen, Messung auf dem Zweig ohne Lösercode-Änderung (Spalte „vor dem Zweig“, ohne Zeitzähler); Cholesky und Rang (Dichte `analyze_rows`) tragen fast die ganze Zeit, Jacobi und Normalgleichungen sind klein. Mit eingebauten Zeitzählern (im normalen Weg inaktiv) lösen 2D 1,85 s, 3D 8,04 s, Zugschritt 11,4 s / 31,7 s, also kein Mehraufwand.

---

## Etappe 1: Löser

### Task 1: Löser messen und Zeit aufteilen

**Dateien:**
- Modify: `crates/planora-sketch/src/solver.rs` (Zeitzähler um `assemble`, `gram`, `cholesky_solve`, `analyze_rows`), `crates/planora-sketch/src/lib.rs` (Export)
- Modify: `crates/planora-sketch/tests/three_d_perf.rs` (neuer Messtest 2D, Ausgabe der Aufteilung)
- Test: `crates/planora-sketch/tests/solver_timings.rs` (neu)

**Schnittstellen:**
- Produces:
  - `#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct SolveTimings { pub assemble: Duration, pub normal: Duration, pub cholesky: Duration, pub rank: Duration, pub total: Duration }` in `planora_sketch::solver`
  - `#[doc(hidden)] pub fn solve_timed(&mut self) -> (SolveReport, SolveTimings)` und `#[doc(hidden)] pub fn drag_timed(&mut self, drags: &[Drag]) -> Result<(SolveReport, SolveTimings), SketchError>` an `Sketch`
  - Zähler über ein `thread_local!` `Cell<Option<SolveTimings>>`; ohne `*_timed` ist er `None`, und kein `Instant::now()` läuft (keine Kosten im normalen Weg).
  - `fn chain_2d(s: &mut Sketch, segments: usize) -> (PointId, usize)` im Messtest: dieselbe Kette wie `chain_3d`, aber in der Ebene (Richtungen wechseln x → y → −x → −y, Bögen in der Skizzenebene mit `add_arc`), Coincident und Tangent wie dort, gleiche Störung in x/y.

- [ ] **Schritt 1: Test schreiben** (`solver_timings.rs`):
  ```rust
  #[test]
  fn solve_timings_add_up() {
      let mut s = rect_with_dims(); // Rechteck mit zwei Maßen, wie in tests/solver.rs
      let (r, t) = s.solve_timed();
      assert_eq!(r.status, SolveStatus::Solved);
      assert!(t.total >= t.assemble + t.normal + t.cholesky + t.rank);
      assert!(t.assemble > Duration::ZERO);
  }
  #[test]
  fn timed_solve_equals_plain_solve() {
      let (mut a, mut b) = (rect_with_dims(), rect_with_dims());
      let ra = a.solve();
      let (rb, _) = b.solve_timed();
      assert_eq!(ra.dof, rb.dof);
      assert_eq!(a.values_bits(), b.values_bits()); // Hilfsfunktion im Test über point_xy aller Punkte
  }
  ```
  Messtest `perf_200_chained_2d` (`#[ignore]`) in `three_d_perf.rs`: `chain_2d(&mut s, 100)` → `assert_eq!(curves, 200)`, lösen, ein Zugschritt `+(0.1, 0.1)`, Ausgabe `PERF2D-KETTE …` mit der Aufteilung; `perf_200_chained_3d` gibt zusätzlich die Aufteilung aus (`solve_timed`/`drag_timed`).
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test solver_timings` → FAIL (`solve_timed` fehlt).
- [ ] **Schritt 3:** Umsetzen wie oben. `total` misst den ganzen Aufruf.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-sketch --test solver_timings`; fmt/clippy/Tests des Crates.
- [ ] **Schritt 5: Messen und Ledger füllen.** Erst auf dem Stand vor dem Zweig messen (`git worktree add ../planora-base <merge-base von HEAD und master>` in einem eigenen Ordner, dort `chain_2d` und die Messtests einfügen und laufen lassen, nichts committen; danach `git worktree remove ../planora-base`), dann auf dem Zweig: `cargo test -p planora-sketch --release --test three_d_perf -- --ignored --nocapture` und `cargo test -p planora-sketch --release --test import_perf -- --ignored --nocapture`. Werte und Aufteilung in die Tabelle „Ledger“ dieses Plans eintragen.
- [ ] **Schritt 6:** Commit `Skizzen 11a: Löser messen und Zeit aufteilen`. Vault-Notiz „Task 1 Löser messen“.

### Task 2: Dünne Jacobi-Matrix, RCM-Ordnung und Hüllen-Cholesky

**Dateien:**
- Create: `crates/planora-sketch/src/sparse.rs` (`pub mod sparse`, `#[doc(hidden)]`)
- Modify: `crates/planora-sketch/src/solver.rs` (`Group`, `Assembled`, `levenberg_marquardt_from`, `diagnose`), `crates/planora-sketch/src/model.rs` (Feld `solver_path`), `crates/planora-sketch/src/lib.rs`
- Test: `crates/planora-sketch/tests/sparse_linalg.rs` (neu), `crates/planora-sketch/tests/sparse_solver.rs` (neu)

**Schnittstellen:**
- Consumes: Task 1 (`SolveTimings`, `chain_2d`/`chain_3d` als Testaufbau, kopiert in `sparse_solver.rs`)
- Produces (`planora_sketch::sparse`):
  - `pub struct SparseRows { pub cols: usize, pub row_start: Vec<usize>, pub col: Vec<usize>, pub val: Vec<f64> }` (CSR) mit `pub fn from_rows(cols: usize, rows: &[Vec<(usize, f64)>]) -> SparseRows`, `pub fn rows(&self) -> usize`, `pub fn row(&self, i: usize) -> (&[usize], &[f64])`, `pub fn mul_vec(&self, x: &[f64]) -> Vec<f64>`, `pub fn transpose_mul_vec(&self, y: &[f64]) -> Vec<f64>`, `pub fn to_dense(&self) -> Matrix`
  - `pub fn rcm_order(rows: &SparseRows) -> Vec<usize>`: Permutation neu → alt nach umgekehrtem Cuthill-McKee über den Graphen von `JᵀJ` (Start je Zusammenhang am Knoten kleinsten Grads, Nachbarn nach Grad aufsteigend, gleiche Grade nach Index); deterministisch.
  - `pub struct Envelope { n: usize, first: Vec<usize>, start: Vec<usize>, data: Vec<f64> }` (untere Hülle, Zeile i von `first[i]` bis i) mit `pub fn gram(rows: &SparseRows, perm: &[usize]) -> Envelope`, `pub fn add_diagonal(&mut self, v: f64)`, `pub fn max_diagonal(&self) -> f64`, `pub fn get(&self, i: usize, j: usize) -> f64`
  - `pub fn envelope_cholesky_solve(a: &Envelope, b: &[f64]) -> Option<Vec<f64>>`: `None` unter denselben Bedingungen wie `linalg::cholesky_solve` (Pivot ≤ 0, NaN, nicht endlich).
  - `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub enum SolverPath { #[default] Auto, Dense, Sparse }`; `pub fn from_env() -> SolverPath` liest `PLANORA_SOLVER` (`dense`/`sparse`, sonst `Auto`) einmal über `OnceLock`.
  - `pub const SPARSE_MIN_VARS: usize = 60;`
- Produces (Löser): `Group.order: Option<Vec<usize>>` (RCM, einmal je `build_groups_holding`, nur auf dem dünnen Weg); `enum Jacobian { Dense(Matrix), Sparse(SparseRows) }` in `Assembled.j`; `Sketch.solver_path: SolverPath` (nicht in Datei und Snapshot-Vergleich, Vorgabe `SolverPath::from_env()`), `#[doc(hidden)] pub fn set_solver_path(&mut self, p: SolverPath)`.

- [ ] **Schritt 1: Tests schreiben.**
  `sparse_linalg.rs`:
  ```rust
  #[test] fn sparse_gram_equals_dense_gram() // zufällige Bandmatrix 50×40 (fester Seed), perm = rcm_order: env.get(i,j) == dense.gram()[perm[i]][perm[j]] bis 1e-12
  #[test] fn envelope_cholesky_equals_dense() // gleiche Lösung bis 1e-10 wie cholesky_solve auf derselben Matrix + 1e-3·I
  #[test] fn envelope_cholesky_rejects_indefinite() // assert!(envelope_cholesky_solve(&a, &b).is_none()) bei Diagonale −1
  #[test] fn rcm_of_chain_has_small_bandwidth() // Kette 200 Variablen, gemischt nummeriert: Bandbreite nach RCM ≤ 4
  #[test] fn rcm_is_deterministic() // zweimal gleich
  ```
  `sparse_solver.rs` (jeweils derselbe Aufbau zweimal, `set_solver_path(Dense)` und `set_solver_path(Sparse)`):
  ```rust
  fn same(a: &SolveReport, b: &SolveReport, sa: &Sketch, sb: &Sketch) {
      assert_eq!(a.status, b.status); assert_eq!(a.dof, b.dof);
      assert_eq!(a.redundant, b.redundant); assert_eq!(a.conflicting, b.conflicting);
      for (p, _) in sa.points() { assert!((sa.point_xyz(p).unwrap() - sb.point_xyz(p).unwrap()).abs().max_element() <= 1e-9); }
  }
  #[test] fn sparse_solves_like_dense_2d_chain()      // chain_2d(40)
  #[test] fn sparse_solves_like_dense_3d_chain()      // chain_3d(20)
  #[test] fn sparse_drag_like_dense()                 // chain_2d(40), Drag (+0.3, −0.2)
  #[test] fn sparse_redundant_like_dense()            // Kette + doppeltes Horizontal
  #[test] fn sparse_conflict_names_same_constraints() // Prüf-Schwerpunkt 5: arc_chain(500) einfügen, Distance zweier Kettenpunkte gegen die Kette widersprüchlich: status Conflicting, conflicting gleich
  #[test] fn small_group_stays_dense()                // Rechteck (8 Variablen) im Auto-Weg: bitgleich zu Dense
  ```
  (`free_points`/`free_curves` prüft Task 3.)
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test sparse_linalg` und `--test sparse_solver` → FAIL (Modul fehlt).
- [ ] **Schritt 3: Umsetzen.**
  - `assemble` baut die Zeilen schon heute als `(c, d)`-Listen; daraus `SparseRows` statt `Matrix` auf dem dünnen Weg.
  - Dünner LM-Schritt: Gradient `transpose_mul_vec`, `Envelope::gram` in RCM-Ordnung, Dämpfung `λ · max(max_diagonal, MIN_DIAG)` auf die Diagonale (wie dicht), `envelope_cholesky_solve`, Schritt zurück in die Spaltenordnung permutieren. Alles Weitere (Annahme der Schritte, Stillstand, Dämpfungsfolge) bleibt Zeile für Zeile.
  - Weg je Gruppe: `Auto` → dünn ab `g.vars.len() >= SPARSE_MIN_VARS`; `Dense`/`Sparse` erzwingen. Die Rangprüfung bleibt in dieser Aufgabe dicht (`to_dense()` + `analyze_rows`).
  - `diagnose` und `solve_keeping_tangent_drift` nehmen denselben Weg wie die Gruppe.
- [ ] **Schritt 4:** Grün: beide Tests; dazu die ganze Testreihe mit erzwungenem dünnen Weg: `PLANORA_SOLVER=sparse cargo test -p planora-sketch` (PowerShell: `$env:PLANORA_SOLVER='sparse'; cargo test -p planora-sketch; Remove-Item Env:PLANORA_SOLVER`). Erwartet: grün. Scheitert ein Test nur an einem bitgenauen Vergleich, wird er im Ledger mit Grund vermerkt und die Abweichung ≤ 1e-9 belegt; sonst ist es ein Fehler.
- [ ] **Schritt 5:** fmt/clippy/Tests; Commit `Skizzen 11a: Dünne Jacobi-Matrix und Hüllen-Cholesky`. Vault-Notiz „Task 2 Dünner Löser“.

### Task 3: Dünne Rangprüfung und Analyse beim Ziehen (A10)

**Dateien:**
- Modify: `crates/planora-sketch/src/sparse.rs`, `crates/planora-sketch/src/solver.rs` (`analyze_assembled`, `SolveCache`, `Outcome`, `solve_round`)
- Modify: `crates/planora-sketch/tests/three_d_perf.rs` (Messziele als Zusicherungen)
- Test: `crates/planora-sketch/tests/sparse_solver.rs`, `crates/planora-sketch/tests/drag_analysis.rs` (neu)

**Schnittstellen:**
- Consumes: Task 2 (`SparseRows`, `rcm_order`, `Group.order`, `SolverPath`)
- Produces:
  - `pub struct SparseRowSpace { pub independent: Vec<bool>, pub rank: usize, pub free: Vec<bool> }` und `pub fn analyze_rows_sparse(rows: &SparseRows, perm: &[usize], rel_tol: f64, free_tol: f64) -> SparseRowSpace`
  - Algorithmus: inkrementelle Givens-QR der Zeilen in Zeilenreihenfolge, Spalten in RCM-Ordnung, `R` als Hülle; Zeile i ist unabhängig ⇔ der Rest nach dem Eliminieren gegen `R` hat Länge `> rel_tol · ‖Zeile i‖` (gleich dem Gram-Schmidt-Test von `analyze_rows`); nur unabhängige Zeilen gehen in `R`. Freie Spalte c ⇔ `‖P_N e_c‖ > free_tol` mit `P_N` = Projektion auf den Nullraum (aus `R` über die Spalten ohne Pivot und Rückwärtseinsetzen, danach orthonormiert). Der dichte Weg bleibt unverändert.
  - `Outcome::Solved { dof, redundant, free_vars, carried: bool }`: `carried == true` heißt „Analyse vom Zugbeginn übernommen“. `SolveCache::take` gibt einen `carried`-Eintrag nur an Gruppen mit Zug (`group_soft` nicht leer) heraus; ohne Zug wird neu gerechnet. So rechnet das Lösen beim Loslassen an der neuen Lage neu.
  - `fn analysis_for(&self, constraints: &[ConstraintId]) -> Option<Analysis>` an `SolveCache`: Analyse eines gelösten Eintrags mit gleichen Abhängigkeiten, ohne Blick auf die Werte.
  - `SPARSE_MIN_VARS` endgültig: kleinste Gruppengröße, ab der der dünne Weg in `chain_2d` schneller ist, auf die nächste Zehnerzahl aufgerundet; Wert und Messung in den Ledger.

- [ ] **Schritt 1: Tests schreiben.**
  `sparse_solver.rs` ergänzen:
  ```rust
  #[test] fn sparse_rank_like_dense()       // 20 zufällige Bandmatrizen mit linear abhängigen Zeilen: independent und rank gleich analyze_rows, free gleich der dichten Freiheitsprüfung
  #[test] fn sparse_free_parts_like_dense() // chain_2d(40) mit einem Fixed am Start: free_points und free_curves gleich dicht
  #[test] fn sparse_parallel_lines_redundant() // zwei Linien parallel + Winkel 0 in großer Gruppe: redundant gleich dicht
  ```
  `drag_analysis.rs`:
  ```rust
  #[test] fn drag_reuses_analysis_of_drag_start() // chain_2d(40): solve(); drei drag()-Schritte; Bericht dof/free_* gleich dem Bericht vor dem Zug
  #[test] fn release_solve_recomputes_analysis()   // nach den Zügen solve(): Ergebnis gleich einem frischen Sketch an denselben Werten (dof, redundant, free_*)
  #[test] fn structure_change_drops_carried()     // nach add_constraint während eines Zugs: nächster drag() rechnet neue Analyse (dof sinkt)
  #[test] fn small_group_drag_unchanged()         // Rechteck: drag-Bericht bitgleich zum Stand vor der Aufgabe (fester Erwartungswert dof = 4 nach zwei Maßen)
  ```
  Messziele in `three_d_perf.rs`, `perf_200_chained_3d`: `assert!(solve <= 0.5, "{solve}")`, `assert!(drag <= 0.05, "{drag}")`.
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test sparse_solver --test drag_analysis` → FAIL.
- [ ] **Schritt 3:** Umsetzen wie oben. Im Zuglauf (`group_soft` nicht leer, konvergiert): erst `old_cache.analysis_for(&g.constraints)`, sonst `analyze`; Ergebnis mit `carried: true` merken.
- [ ] **Schritt 4:** Grün; `PLANORA_SOLVER=sparse cargo test -p planora-sketch` grün; Messtests im Release: `cargo test -p planora-sketch --release --test three_d_perf -- --ignored --nocapture` (3D-Ziele erfüllt), `--test import_perf -- --ignored --nocapture`; 2D-Werte ≤ +5 % gegen den Ledger. Ledger-Spalte „nach Task 3“ füllen, Schwelle festlegen.
- [ ] **Schritt 5:** fmt/clippy/Tests; Commit `Skizzen 11a: Dünne Rangprüfung und Analyse beim Ziehen`. Vault-Notiz „Task 3 Rangprüfung und Ziehen“ mit den Messwerten.

## Etappe 2: Modell und Datei

### Task 4: „Tangente am Ende“ (2D und 3D) und Glatt am Ellipsenbogen

**Dateien:**
- Modify: `crates/planora-sketch/src/constraint.rs` (`ConstraintKind`, Auswertung, Strukturprüfung, `smooth_end`, `SmoothEnd`, `smooth_step`, `smooth_partner_kind`, `end_curvature_dual`), `crates/planora-sketch/src/constraint3d.rs` (3D-Zeilen), `crates/planora-app/src/sketch_view.rs` (`kind_label`: „tangential“)
- Test: `crates/planora-sketch/tests/end_tangent.rs` (neu), `crates/planora-sketch/tests/smooth_g2.rs` (ergänzen)

**Schnittstellen:**
- Produces:
  - `ConstraintKind::EndTangent(CurveId, PointId, CurveId, PointId)`: Kurve, ihr Ende, Kurve, ihr Ende; Richtungen am gemeinsamen Punkt gleich oder entgegengesetzt, keine Krümmungszeile. 2D eine Zeile `cross(t1, t2) / (|t1|·|t2|)` mit `t = into_curve`; 3D zwei Zeilen wie `Parallel` in `constraint3d.rs` auf den Endrichtungen. `allowed_in_3d()` wahr. Kein `is_dimension`, kein `needs_format_15`.
  - `pub(crate) fn open_curve_ends(kind: &CurveKind) -> Option<[PointId; 2]>`: wie `curve_ends`, dazu `EllipticArc { start, end }`. `curve_ends` bleibt unverändert (Tangent, Stutzen, Brechen lösen sonst anders).
  - `into_curve` und `end_curvature_dual` kennen den Ellipsenbogen (Richtung und Krümmung aus `ellipse_eval` am Start- bzw. Endwinkel).
  - `SmoothEnd::EllipticArc` (eigene Krümmung wie `Arc`): `smooth_end` liefert sie; `smooth_partner_kind(EllipticArc) == true`; `smooth_step(EllipticArc, _) == Keep`; `smooth_step(Natural | Straight | Form | ArcHandle, EllipticArc)` wie gegen `Arc`.
  - `Sketch::add_constraint(EndTangent(..))` prüft: beide Punkte sind Enden ihrer Kurve nach `open_curve_ends`, beide Punkte gleich (`PointId`) oder per Deckungsgleich verbunden, keine geschlossene Kurve, nicht Kreis/Ellipse; sonst `SketchError::InvalidInput(tr!("sketch-end-tangent-ends"))`.
  - `pub fn needs_format_21(&self) -> bool` an `ConstraintKind`: `EndTangent(..)`; `Smooth` mit Ellipsenbogen prüft `Sketch::needs_format_21` (Task 7), weil die Art allein es nicht weiß.

- [ ] **Schritt 1: Tests schreiben** (`end_tangent.rs`):
  ```rust
  #[test] fn end_tangent_line_line_aligns()          // zwei Linien am gemeinsamen Punkt, 30° Knick: nach solve() |cross| ≤ 1e-9
  #[test] fn end_tangent_allows_reverse_direction()   // Linie läuft zurück (180°): Residuum 0 ohne Bewegung (status Solved, iterations == 0)
  #[test] fn end_tangent_has_no_curvature_row()        // ControlSpline Grad 3 an Bogen: Krümmungen an der Fuge verschieden nach solve()
  #[test] fn end_tangent_on_elliptic_arc()             // Ellipsenbogen-Ende + Linie: tangential nach solve()
  #[test] fn end_tangent_rejects_inner_point()         // Spline-Mittelpunkt: Err(InvalidInput)
  #[test] fn end_tangent_rejects_circle()              // Err(InvalidInput)
  #[test] fn end_tangent_3d_two_lines()                // chain-artig zwei 3D-Linien: Richtungen parallel bis 1e-9
  #[test] fn end_tangent_counts_one_dof_in_2d()        // dof sinkt um genau 1
  ```
  `smooth_g2.rs` ergänzen:
  ```rust
  #[test] fn smooth_on_elliptic_arc_matches_curvature() // ControlSpline Grad 5 + Ellipsenbogen: Smooth angelegt, Krümmung beider Seiten an der Fuge gleich bis 1e-6
  #[test] fn smooth_elliptic_arc_needs_spline_leader()  // Ellipsenbogen + Linie: Err wie heute bei zwei Nicht-Splines
  ```
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test end_tangent --test smooth_g2` → FAIL.
- [ ] **Schritt 3:** Umsetzen. Datenausgabe (`data.rs`) und `kind_label` für „tangential“ ergänzen; `Tangent` an Splines bleibt, wie es ist.
- [ ] **Schritt 4:** Grün; alle Tests von `planora-sketch` (2D unverändert); `cargo test -p planora-app --test sketch_ui` grün.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Tangente am Ende und Glatt am Ellipsenbogen`. Vault-Notiz „Task 4 Tangente am Ende“.

### Task 5: Krümmungskamm – Modell und Rechnung

**Dateien:**
- Create: `crates/planora-sketch/src/comb.rs`
- Modify: `crates/planora-sketch/src/model.rs` (`CurveData.comb`), `crates/planora-sketch/src/lib.rs`, `crates/planora-sketch/src/data.rs` (Ausgabe)
- Test: `crates/planora-sketch/tests/comb.rs` (neu)

**Schnittstellen:**
- Produces (`planora_sketch::comb`):
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct CombSettings { pub density: u32, pub scale: f64 }`, `Default` = `{ density: 20, scale: 1.0 }`
  - `pub const COMB_DENSITY: RangeInclusive<u32> = 2..=100; pub const COMB_SCALE_MIN: f64 = 0.01; pub const COMB_SCALE_MAX: f64 = 100.0; pub const COMB_MAX_FRACTION: f64 = 0.15; pub const COMB_MIN_PX: f64 = 10.0;`
  - `CurveData.comb: Option<CombSettings>` (Vorgabe `None`)
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct CombSample { pub at: DVec3, pub out: DVec3, pub kappa: f64 }`: `out` Einheitsvektor vom Krümmungsmittelpunkt weg (konvexe Seite), `kappa ≥ 0`
  - `Sketch::comb_capable(&self, c: CurveId) -> bool`: Bogen, Kreis, Ellipse, Ellipsenbogen, Spline, ControlSpline, Conic, FixedSpline; Linie nein
  - `Sketch::set_comb(&mut self, c: CurveId, comb: Option<CombSettings>) -> Result<(), SketchError>`: Linie oder Werte außerhalb der Bereiche → `InvalidInput`; ändert weder Werte noch `solve_cache`
  - `Sketch::comb_samples(&self, c: CurveId, density: u32) -> Option<Vec<CombSample>>`: `density` Stellen gleichmäßig im Parameter, offene Kurven mit beiden Enden, geschlossene ohne doppelten Endpunkt; Kreis/Bogen `1/r`, Ellipse `ab / (a² sin²t + b² cos²t)^{3/2}`, Splines und Kegelschnitt aus erster und zweiter Ableitung; 3D-Kurven in ihrer Ebene (Normale) bzw. entlang der Hauptnormale
  - `pub fn comb_teeth(samples: &[CombSample], curve_length: f64, scale: f64, min_world: f64) -> Vec<(DVec3, DVec3)>`: (Fuß, Spitze); größter Zahn `max(COMB_MAX_FRACTION · curve_length, min_world) · scale`, Zahn i proportional zu `kappa_i / kappa_max`; `kappa_max == 0` → Spitze = Fuß
  - `Sketch::curve_length(&self, c: CurveId) -> Option<f64>` (falls nicht vorhanden; sonst die vorhandene nutzen)

- [ ] **Schritt 1: Tests schreiben** (`comb.rs`):
  ```rust
  #[test] fn circle_comb_constant_and_outward() // Kreis Mitte (0,0) r = 5, density 8: alle kappa = 0.2 ± 1e-12; Stelle (5,0) hat out = (1,0,0)
  #[test] fn ellipse_curvature_at_vertices()    // a = 4, b = 2: kappa am Hauptscheitel 4/4 = 1.0, am Nebenscheitel 2/16 = 0.125 (± 1e-9)
  #[test] fn density_gives_tooth_count()        // offener Bogen, density 20: 20 Samples, erstes/letztes an Start/Ende
  #[test] fn longest_tooth_is_15_percent()      // Kegelschnitt Länge L: längster Zahn = 0.15·L·1.0 ± 1e-9 bei min_world = 0
  #[test] fn min_world_lifts_short_curves()     // L = 1, min_world = 2: längster Zahn 2.0
  #[test] fn comb_on_straight_spline_has_zero_teeth() // Prüf-Schwerpunkt 3: ControlSpline mit Punkten auf einer Geraden: alle Spitzen = Füße, nichts NaN
  #[test] fn line_has_no_comb()                 // set_comb(line, Some(default)) → Err; comb_capable(line) == false
  #[test] fn comb_out_of_range_rejected()       // density 1, density 101, scale 0.0, scale 101.0 → Err
  #[test] fn set_comb_keeps_values_and_cache()  // Werte bitgleich, nächstes solve() hat group_solves == 0
  #[test] fn comb_3d_arc_in_its_plane()         // add_arc3 mit Normale X: alle out ⟂ X
  ```
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test comb` → FAIL.
- [ ] **Schritt 3:** Umsetzen. `comb` wandert beim Kopieren/Verschieben/Muster der Kurve mit wie `show_frame`; Stutzen/Brechen geben beiden Teilen den Kamm.
- [ ] **Schritt 4:** Grün; Tests des Crates.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Krümmungskamm im Modell`. Vault-Notiz „Task 5 Krümmungskamm Modell“.

### Task 6: Winkelmaß mit Gegenwinkel – Modell (A7)

**Dateien:**
- Modify: `crates/planora-sketch/src/constraint.rs` (`ConstraintData.supplement`, Auswertung `Angle`, Messung), `crates/planora-sketch/src/dimension.rs` (`DimPlan.supplement`, Zeigerregel zweier Linien), `crates/planora-sketch/src/constraint3d.rs` (3D-Winkel)
- Test: `crates/planora-sketch/tests/angle_supplement.rs` (neu), `crates/planora-sketch/tests/dimension_plan.rs` (anpassen)

**Schnittstellen:**
- Produces:
  - `ConstraintData.supplement: bool` (`pub(crate)`) mit `pub fn supplement(&self) -> bool`; nur an `Angle` wahr. Auswertung mit Gegenwinkel: das Residuum von `Angle` mit `(d1', d2') = (−d2, d1)`; der Messwert ist dann `π − θ ∈ (0, π)`. In 3D: Winkel zwischen `d1` und `−d2`.
  - `Sketch::add_constraint_supplement(&mut self, kind: ConstraintKind, supplement: bool) -> Result<ConstraintId, SketchError>`: `supplement` nur bei `Angle`, sonst `InvalidInput`; `add_constraint(kind)` = `add_constraint_supplement(kind, false)`.
  - `Sketch::measure_angle(&self, c1: CurveId, c2: CurveId, supplement: bool) -> Option<f64>`; `measure(&kind)` misst wie bisher ohne Gegenwinkel; `measure_constraint(id)` beachtet das Feld.
  - `DimPlan.supplement: bool` (Vorgabe `false`); Zeigerregel zweier nicht paralleler Linien (2D und 3D): Schnittpunkt X der Geraden, `u = pointer − X`; liegt u im Feld zwischen `+d1` und `+d2` oder zwischen `−d1` und `−d2` → Winkel (`supplement = false`), sonst Gegenwinkel. Liegt u genau auf einer Geraden (|cross| ≤ `tol.angular`·|u|) → Winkel. Geordnet wie bisher so, dass der Wert positiv ist.
  - `set_constraint_value` und Rückgängig behalten `supplement`.

- [ ] **Schritt 1: Tests schreiben** (`angle_supplement.rs`):
  ```rust
  #[test] fn supplement_measures_pi_minus_theta() // Linien 0° und 60°: measure_angle(.., true) = 120° ± 1e-12
  #[test] fn supplement_drives_geometry()         // Angle 60° mit supplement = true auf Wert 100° gesetzt: gemessener Winkel ohne Zusatz = 80° ± 1e-9
  #[test] fn plan_picks_supplement_by_pointer()   // Linien durch (0,0): Zeiger in den vier Feldern → false, true, false, true
  #[test] fn plan_on_line_takes_angle()           // Zeiger genau auf Linie 1: supplement = false
  #[test] fn supplement_only_on_angle()           // add_constraint_supplement(Distance(..), true) → Err
  #[test] fn supplement_3d_lines()                // zwei 3D-Linien 50°: Gegenwinkel 130°
  #[test] fn old_angle_unchanged()                // add_constraint(Angle) bitgleich zum Stand vor der Aufgabe (Residuum und Messwert)
  ```
  `dimension_plan.rs`: vorhandene Erwartungen bekommen `supplement: false`; ein Plan mit Zeiger im Gegenfeld erwartet `supplement: true`.
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test angle_supplement --test dimension_plan` → FAIL.
- [ ] **Schritt 3:** Umsetzen. Doku-Kommentar von `dimension_plan` ändern: Die Entscheidung „Winkel immer in (0, π) ohne Gegenwinkel“ aus Skizzen 5 gilt nicht mehr.
- [ ] **Schritt 4:** Grün; Tests des Crates.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Gegenwinkel im Winkelmaß`. Vault-Notiz „Task 6 Gegenwinkel Modell“.

### Task 7: Format 21

**Dateien:**
- Modify: `crates/planora-project/src/format.rs` (`CurveDto.kamm`, `ConstraintDto.gegenwinkel`, `ConstraintKindDto::TangenteAmEnde`), `crates/planora-project/src/convert.rs` (`FORMAT_VERSION = 21`, `needs_format_21`, `format_with`, Lesen/Schreiben, Ablehnung neuer Daten in alten Formaten), `crates/planora-sketch/src/model.rs` (`Sketch::needs_format_21`)
- Test: `crates/planora-project/tests/format21.rs` (neu), `crates/planora-project/tests/format20.rs` (anpassen)

**Schnittstellen:**
- Consumes: Tasks 4, 5, 6
- Produces:
  - `CurveDto.kamm: Option<KammDto>` mit `#[serde(default, skip_serializing_if = "Option::is_none")]`, `pub struct KammDto { pub dichte: u32, pub massstab: f64 }`
  - `ConstraintDto.gegenwinkel: bool` mit `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`
  - `ConstraintKindDto::TangenteAmEnde { a: usize, punkt_a: usize, b: usize, punkt_b: usize }` (serde-Name `tangente_am_ende`)
  - `Sketch::needs_format_21(&self) -> bool`: ein Kurveneintrag mit `comb`, ein Eintrag `EndTangent`, ein `Angle` mit `supplement`, ein `Smooth` mit einem Ellipsenbogen – auch gelöschte Einträge
  - `fn needs_format_21(k: &FeatureKind) -> bool` als erste Regel in `format_with` (vor 20)
  - Lesen: Format ≤ 20 mit `kamm`, `gegenwinkel` oder `tangente_am_ende` → Fehler wie bei 3D-Daten in Format 19; Format > 21 → „zu neu“

- [ ] **Schritt 1: Tests schreiben** (`format21.rs`, Hilfen wie in `format20.rs`):
  ```rust
  #[test] fn format_version_is_21()                 // assert_eq!(FORMAT_VERSION, 21)
  #[test] fn old_file_bytes_unchanged()             // alle Dateien in tests/fixtures*: lesen + schreiben = bytegleich
  #[test] fn plain_sketch_stays_below_21()          // Skizze mit Linie, Bogen, Spline: format < 21
  #[test] fn midpoint_line_stays_below_21()         // Linie + Punkt + Midpoint: format wie vorher
  #[test] fn g2_blend_without_elliptic_arc_below_21() // ControlSpline Grad 5 + zwei Smooth an Linie und Bogen: format < 21
  #[test] fn comb_writes_21_and_roundtrips()        // set_comb(Some{density: 30, scale: 2.5}): format 21, geladen gleich
  #[test] fn deleted_curve_with_comb_needs_21()
  #[test] fn end_tangent_writes_21_and_roundtrips()
  #[test] fn supplement_writes_21_and_roundtrips()  // geladen: supplement() == true, Wert gleich
  #[test] fn smooth_on_elliptic_arc_writes_21()
  #[test] fn format_20_with_comb_rejected()         // "format": 20 + kamm → Fehler
  #[test] fn format_22_is_too_new()
  ```
  `format20.rs`: `format_version_is_20` → entfernt (ersetzt durch `format_version_is_21`), `format_21_is_too_new` → `format_22_is_too_new` dort löschen (steht jetzt in `format21.rs`).
- [ ] **Schritt 2:** `cargo test -p planora-project --test format21` → FAIL.
- [ ] **Schritt 3:** Umsetzen; Doku-Kommentar von `required_format` um „21 mit Kamm, Gegenwinkel, Tangente am Ende oder Glatt am Ellipsenbogen“ ergänzen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-project`; dazu `cargo test -p planora-feature --test snapshot`.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Dateiformat 21`. Vault-Notiz „Task 7 Format 21“.

## Etappe 3: Zeichenwerkzeuge und Nacharbeiten

### Task 8: Mittelpunktlinie

**Dateien:**
- Modify: `crates/planora-app/src/sketch_kinds.rs` (`ToolKind::Line`, `ToolKind::MidpointLine`, `ToolKinds.line`), `crates/planora-app/src/sketch_state.rs` (Klickablauf), `crates/planora-app/src/sketch_panel.rs` (Umschalter unter „Optionen des Features“, Liste am Leistenknopf Linie), `crates/planora-app/src/sketch_inline.rs` (Felder Länge/Winkel), `crates/planora-app/src/sketch_draw3.rs` (Triadenebene), `crates/planora-app/src/sketch_view.rs` (Vorschau), `keymap/defaults.rs`, `de.ftl`/`en.ftl`
- Modify: `crates/planora-sketch/src/shapes.rs`
- Test: `crates/planora-app/tests/sketch_midpoint_line.rs` (neu), `crates/planora-sketch/tests/shapes.rs` (ergänzen)

**Schnittstellen:**
- Produces:
  - `pub fn midpoint_line_plan(mid: DVec2, end: DVec2, tol: f64) -> Option<(DVec2, DVec2)>` in `planora_sketch::shapes`: `(2·mid − end, end)`, `None` bei `|end − mid| ≤ tol`
  - `Sketch::add_midpoint_line(&mut self, mid: PointId, a: PointId, b: PointId) -> Result<CurveId, SketchError>`: Linie a–b und `Midpoint(mid, line)`; `mid` ist ein gewöhnlicher Skizzenpunkt
  - `ToolKind::Line` (klassisch, `click_line`) und `ToolKind::MidpointLine` (beide `tool() == Tool::Line`), `ToolKinds.line` (Vorgabe `ToolKind::Line`), Felder `MidpointLine => &[&[], &[F::Length, F::Angle]]`, Aktion `Action::SketchMidpointLine`
  - Getippte Länge = ganze Linie → `Distance(a, b, L)`; getippter Winkel wie bei der Linie (Zeigerregel waagerecht/senkrecht); fängt der erste Klick einen Punkt, wird er die Mitte; an den Enden Fang und automatische Abhängigkeiten wie bei der Linie (das gespiegelte Ende fängt nicht). Keine Kette. Enter, Esc, Rechtsklick beenden.

- [ ] **Schritt 1: Tests schreiben.**
  `shapes.rs`: `midpoint_line_plan_is_symmetric` (mid (1,1), end (4,5) → ((−2,−3),(4,5))), `midpoint_line_plan_zero_refused`.
  `sketch_midpoint_line.rs`:
  ```rust
  #[test] fn two_clicks_make_line_and_mid_point()   // 1 Kurve, 3 neue Punkte, Midpoint vorhanden, ein Rückgängig-Schritt
  #[test] fn typed_length_is_whole_line()           // Länge 40 getippt: Distance(a, b, 40), Linie 40 lang, Mitte am ersten Klick
  #[test] fn first_click_on_point_reuses_it()       // Klick auf Linienende: kein neuer Mittelpunkt, kein Coincident
  #[test] fn no_chain_after_second_click()          // dritter Klick setzt neue Mitte, keine Verbindung zur ersten Linie
  #[test] fn esc_after_first_click_leaves_nothing() // Skizze bitgleich zum Stand vorher, kein Schritt
  #[test] fn panel_switch_toggles_line_kind()       // Umschalter im Skizzenfeld: tool bleibt Line, Art wechselt
  #[test] fn midpoint_line_in_3d_on_triad_plane()   // 3D-Skizze: Endpunkte in der Triadenebene
  #[test] fn action_has_no_default_binding()        // Action::SketchMidpointLine.default_bindings().is_empty()
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_midpoint_line` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün; `cargo test -p planora-app --test sketch_kinds --test sketch_line_arc --test sketch_flyout` grün.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Mittelpunktlinie`. Vault-Notiz „Task 8 Mittelpunktlinie“.

### Task 9: Übergangskurve – Modell

**Dateien:**
- Create: `crates/planora-sketch/src/blend.rs`
- Modify: `crates/planora-sketch/src/lib.rs`
- Test: `crates/planora-sketch/tests/blend.rs` (neu)

**Schnittstellen:**
- Consumes: Task 4 (`EndTangent`, Glatt am Ellipsenbogen, `open_curve_ends`)
- Produces (`planora_sketch::blend`):
  - `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub enum BlendKind { G1, #[default] G2 }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct BlendEnd { pub curve: CurveId, pub point: PointId }`
  - `Sketch::blend_source(&self, c: CurveId) -> Result<(), BlendRefusal>` mit `pub enum BlendRefusal { Closed, ThreeD, Text, NotCurve }` (Kreis, Ellipse, geschlossener FixedSpline → `Closed`; 3D-Kurve → `ThreeD`; Buchstabe → `Text`)
  - `Sketch::blend_end_near(&self, c: CurveId, at: DVec2) -> Option<BlendEnd>`: das nähere der beiden Enden
  - `Sketch::blend_preview(&self, a: BlendEnd, b: BlendEnd, kind: BlendKind) -> Result<Vec<DVec2>, SketchError>`: Polylinie der Anfangslage (ohne Lösen)
  - `Sketch::add_blend_curve(&mut self, a: BlendEnd, b: BlendEnd, kind: BlendKind) -> Result<CurveId, SketchError>`: ControlSpline (G1 Grad 3, 4 Kontrollpunkte; G2 Grad 5, 6 Kontrollpunkte), erster/letzter Kontrollpunkt = `a.point`/`b.point`; G1 zwei `EndTangent(spline, a.point, a.curve, a.point)` bzw. für b; G2 zwei `Smooth(..)` in derselben Form; danach `solve()`; ist das Ergebnis schlechter (`is_worse_than`), wird alles zurückgenommen und `Err(InvalidInput(tr!("sketch-blend-unsolvable")))` geliefert.
  - Anfangslage: `d = |b.point − a.point|`, `h = d/3`, `t_a` Einheitsrichtung aus `a.point` von der Quellkurve weg (Gegenrichtung von `into_curve`), ebenso `t_b`; G1: `P1 = P0 + h·t_a`, `P2 = P3 + h·t_b`; G2 zusätzlich mit Krümmung κ und Normale N der Quelle am Ende: `P1 = P0 + h·t`, `P2 = P0 + 2h·t + (5/4)·κ·h²·N`, symmetrisch für das andere Ende.
  - Prüfungen: `a.point == b.point` oder Abstand ≤ `tol.linear` → `InvalidInput(tr!("sketch-blend-ends-touch"))`; dasselbe Ende zweimal → `InvalidInput(tr!("sketch-blend-same-end"))`; zwei Enden derselben Kurve erlaubt.

- [ ] **Schritt 1: Tests schreiben** (`blend.rs`):
  ```rust
  #[test] fn g2_blend_two_lines()                   // Linien (0,0)-(10,0) und (20,5)-(30,5): Grad 5, 6 Punkte, zwei Smooth, Status Solved, Tangente und Krümmung 0 an beiden Fugen ± 1e-9
  #[test] fn g1_blend_line_arc()                    // Grad 3, 4 Punkte, zwei EndTangent, tangential ± 1e-9
  #[test] fn blend_shares_end_points()              // erster/letzter Kontrollpunkt == a.point/b.point, kein Coincident
  #[test] fn blend_elliptic_arc_g2()                // Ellipsenbogen + Linie, G2: Krümmung gleich ± 1e-6
  #[test] fn blend_two_ends_of_same_curve()         // Bogen 270°: beide Enden einer Kurve → Ok
  #[test] fn blend_same_end_refused()               // Err, Skizze bitgleich
  #[test] fn blend_between_joined_ends_refused()    // Prüf-Schwerpunkt 1: zwei Linien mit gemeinsamem Punkt, beide Enden dort → Err("…aufeinander…"), Skizze bitgleich
  #[test] fn blend_from_3d_curve_refused()          // blend_source → Err(ThreeD)
  #[test] fn blend_circle_refused()                 // Err(Closed)
  #[test] fn unsolvable_blend_leaves_nothing()      // Quellenden fest und Enden G2 unerfüllbar (zwei feste Bögen, Fixed an allen Punkten): Err, Skizze bitgleich
  #[test] fn end_near_picks_closer_end()
  #[test] fn initial_inner_points_follow_tangents() // G1 zwei Linien: P1 = P0 + d/3·t_a ± 1e-12 vor dem Lösen (blend_preview)
  ```
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test blend` → FAIL.
- [ ] **Schritt 3:** Umsetzen (Schlüssel `sketch-blend-*` in de/en).
- [ ] **Schritt 4:** Grün; Tests des Crates.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Übergangskurve im Modell`. Vault-Notiz „Task 9 Übergangskurve Modell“.

### Task 10: Übergangskurve – Werkzeug

**Dateien:**
- Modify: `crates/planora-app/src/sketch_state.rs` (`Tool::BlendCurve`, `SketchSession.blend: BlendState`), `crates/planora-app/src/sketch_panel.rs` (Eintrag unter Ändern, Art G1/G2), `crates/planora-app/src/sketch_view.rs` (Hervorhebung, Vorschau), `crates/planora-app/src/sketch_cursor.rs` (Meldung am Zeiger), `keymap/defaults.rs`, `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_blend_tool.rs` (neu)

**Schnittstellen:**
- Consumes: Task 9
- Produces:
  - `Tool::BlendCurve` (`is_modify() == true`), `pub struct BlendState { pub kind: BlendKind, pub first: Option<BlendEnd>, pub hover: Option<BlendEnd> }` mit Vorgabe `kind = G2`
  - `SketchSession::click_blend(&mut self, sk: &mut Sketch, at: DVec2) -> Result<Option<CurveId>, Msg>`: erster Klick wählt, Klick auf die gewählte Kurve hebt die Wahl auf, zweiter Klick legt an (ein Rückgängig-Schritt)
  - `SketchSession::blend_hover(&mut self, sk: &Sketch, at: DVec2) -> Option<Vec<DVec2>>`: gültiges Ziel hervorheben, Vorschau nach gewähltem erstem Ende
  - Meldung am Zeiger bei `BlendRefusal::ThreeD` („3D-Kurven sind keine Quelle“), `Closed`, `Text`
  - Aktion `Action::SketchBlendCurve`

- [ ] **Schritt 1: Tests schreiben** (`sketch_blend_tool.rs`):
  ```rust
  #[test] fn two_clicks_make_one_undo_step()   // Kurvenzahl +1, Verlauf +1, Rückgängig stellt bitgleich her
  #[test] fn default_kind_is_g2()
  #[test] fn panel_switch_to_g1()              // Art G1: Ergebnis Grad 3
  #[test] fn second_click_on_first_deselects() // first == None, Skizze unverändert
  #[test] fn hover_shows_preview_after_first() // blend_hover liefert Some(Polylinie) mit ≥ 2 Punkten
  #[test] fn hover_over_circle_gives_no_target()
  #[test] fn esc_leaves_nothing()
  #[test] fn unsolvable_shows_message()        // message == Some(sketch-blend-unsolvable), nichts angelegt
  #[test] fn projected_edge_is_a_source()      // projizierte Kante (FixedSpline offen) als Quelle → Ok
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_blend_tool` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün; `cargo test -p planora-app --test sketch_modify_tools --test keymap_sketch_drawing`.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Werkzeug Übergangskurve`. Vault-Notiz „Task 10 Übergangskurve Werkzeug“.

### Task 11: Bogen „Mittelpunkt“ in 2D nach Zugrichtung (A6)

**Dateien:**
- Modify: `crates/planora-app/src/sketch_state.rs` (`arc_sweep: Option<(f64, f64)>` neben `arc_sweep3`), `crates/planora-app/src/sketch_kinds.rs` (Plan `ArcCenter` in 2D), `crates/planora-app/src/sketch_view.rs` (Vorschau)
- Test: `crates/planora-app/tests/sketch_kinds_arc.rs` (ergänzen)

**Schnittstellen:**
- Produces: `fn track_arc_sweep(&mut self, at: DVec2)` und `fn arc_cw(&self) -> bool` an `SketchSession`, gleiche Regel wie `track_arc_sweep3` (Schritt `(angle − last + π).rem_euclid(2π) − π`, Summe mit Vorzeichen); Plan `ArcCenter` nutzt bei `arc_cw()` die vertauschte Lage (Ende als Start); Überstreichen bis `2π − 1e-6` läuft weiter, Rückwärtsziehen über den Start kehrt den Sinn um. Das Winkelfeld (`F::Sweep`) übernimmt das Vorzeichen des Zugs.

- [ ] **Schritt 1: Tests schreiben:**
  ```rust
  #[test] fn arc_center_follows_clockwise_drag()  // Mitte (0,0), Start (10,0), Zeiger über (7,−7) nach (0,−10): Bogen im Uhrzeigersinn, Öffnung 90° ± 1e-9
  #[test] fn arc_center_ccw_unchanged()          // Zug über (7,7): wie vor der Aufgabe (gegen den Uhrzeigersinn)
  #[test] fn arc_center_sweeps_past_180()        // Zeiger läuft 270° herum: Öffnung 270°
  #[test] fn arc_center_reverse_over_start_flips() // erst +30°, dann zurück auf −40°: Uhrzeigersinn, 40°
  #[test] fn arc_center_typed_sweep_keeps_drag_sign() // Zug im Uhrzeigersinn, Feld 45: Öffnung 45° im Uhrzeigersinn
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_kinds_arc` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün; `cargo test -p planora-app --test sketch_draw_3d` (3D unverändert).
- [ ] **Schritt 5:** Commit `Skizzen 11a: Bogen Mittelpunkt folgt dem Zug`. Vault-Notiz „Task 11 Bogen Zugrichtung“.

### Task 12: Winkelmaß nach Zeigerseite im Werkzeug (A7)

**Dateien:**
- Modify: `crates/planora-app/src/sketch_dimension.rs` (`apply_planned`, `apply_planned_value` nehmen `plan.supplement`), `crates/planora-app/src/sketch_view.rs` (Maßbogen im gewählten Feld zeichnen), `crates/planora-app/src/dimension_browser.rs` falls dort Winkel angezeigt werden
- Test: `crates/planora-app/tests/sketch_dimension_tool.rs` (ergänzen)

**Schnittstellen:**
- Consumes: Task 6 (`DimPlan.supplement`, `add_constraint_supplement`, `measure_constraint`)
- Produces: Die Vorschau folgt dem Zeiger zwischen Winkel und Gegenwinkel; das angelegte Maß trägt `supplement`; Doppelklick zum Ändern zeigt den Gegenwinkel-Wert.

- [ ] **Schritt 1: Tests schreiben:**
  ```rust
  #[test] fn angle_tool_pointer_in_opposite_field_sets_supplement() // Linien 0°/60°: Zeiger im stumpfen Feld → Maß 120°, supplement() == true
  #[test] fn angle_tool_pointer_in_acute_field_unchanged()          // 60°, supplement() == false
  #[test] fn supplement_dimension_edit_shows_120()                  // Feld zeigt 120
  #[test] fn supplement_is_one_undo_step()
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_dimension_tool` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Winkelmaß nach Zeigerseite`. Vault-Notiz „Task 12 Winkelmaß Zeigerseite“.

### Task 13: Felder in 3D für Kreis, Polygon und Rechteck (A8)

**Dateien:**
- Modify: `crates/planora-app/src/sketch_draw3.rs` (Felder je Art in der Triadenebene, `click_kind3`), `crates/planora-app/src/sketch_inline.rs` (Lage der Felder über `to_screen3`), `crates/planora-app/src/sketch_input.rs` (Werte → Maße wie in 2D), `crates/planora-app/src/sketch_kinds.rs` (`fields()`)
- Test: `crates/planora-app/tests/sketch_draw_3d.rs` (ergänzen), `crates/planora-app/tests/sketch_kinds_polygon.rs`, `crates/planora-app/tests/sketch_kinds_rect_circle.rs` (ergänzen)

**Schnittstellen:**
- Produces (2D, Abgleich mit „Was Fusion tut“, heute abweichend): `Circle2T => &[&[], &[], &[F::Radius]]` (heute Durchmesser; Spec: „2 Tangenten“ tippt den Radius) und `PolygonEdge => &[&[], &[F::Length, F::Angle], &[F::Sides]]` (heute ohne Winkel; Spec: Länge, Tab zum Winkel, dann Seitenzahl). Ein getippter Kantenwinkel wird wie bei der Linie zum Winkel gegen die Waagerechte.
- Produces (3D): In der 3D-Skizze gelten dieselben `fields()` wie in 2D für `Circle2P` (Durchmesser), `CircleCenter` (Durchmesser), `PolygonCircumscribed`/`PolygonInscribed` (Radius, Tab Seitenzahl), `PolygonEdge` (Länge, Tab Winkel, Seitenzahl), `Rect2P`/`Rect3P`/`RectCenter` (Breite/Höhe bzw. Längen); Werte gelten in der Triadenebene und werden zu denselben Maßen wie in 2D (in 3D: `Diameter`, `Radius`, `Distance`, `Angle`); die Seitenzahl kommt per Tab (ersetzt „zuletzt benutzte Seitenzahl“).

- [ ] **Schritt 1: Tests schreiben:**
  ```rust
  #[test] fn circle_center_diameter_typed_in_3d()   // 3D-Skizze, Ebene XZ: Feld 20 → Kreis r = 10 in der Ebene, Diameter(20) vorhanden
  #[test] fn circle_2p_diameter_typed_in_3d()
  #[test] fn polygon_sides_by_tab_in_3d()           // Radius 15, Tab, 7 → 7 Kanten
  #[test] fn polygon_edge_length_angle_in_3d()      // Länge 12, Tab, Winkel 30°, Seitenzahl 5
  #[test] fn rect_2p_typed_in_3d()                  // 30 × 20 → zwei Distance-Maße
  #[test] fn fields_follow_pointer_no_click()       // A9: Klick auf das Feld setzt die Form nicht ab und fokussiert nicht
  ```
  2D: `circle_2t_types_radius` (Feld 8 → Radius-Maß 8, kein Durchmesser) in `sketch_kinds_rect_circle.rs`; `polygon_edge_length_tab_angle_then_sides` (Länge 10, Tab, 30°, dann 6 → erste Kante 10 lang unter 30°, 6 Kanten) in `sketch_kinds_polygon.rs`.
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_draw_3d` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün; `cargo test -p planora-app --test sketch_fields --test sketch_inline_fields`.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Felder am Zeiger in 3D`. Vault-Notiz „Task 13 Felder in 3D“.

### Handtest Etappe 1–3 (kein Code, nur nach Dennis' Go)

- [ ] Dennis fragen und Go abwarten. Dann `cargo build -p planora-app --release`, Start über `tools/gui-drive.ps1` bei 1920×1080, APPDATA/USERPROFILE umgeleitet.
- [ ] Liste: 3D-Kette mit 200 Kurven ziehen (flüssig), große DXF-Bogenkette ziehen, Mittelpunktlinie (Klick/Klick, Länge tippen, Punkt als Mitte), Übergangskurve G1 und G2 zwischen Linie/Bogen/Ellipsenbogen/Spline, Bogen „Mittelpunkt“ im Uhrzeigersinn, Winkelmaß in allen vier Feldern, Felder in 3D für Kreis/Polygon/Rechteck.
- [ ] Ergebnis als Vault-Notiz „Handtest 1“; Funde als eigene Korrekturaufgaben; Test-Builds und Fotos im Scratchpad löschen.

## Etappe 4: Prüfen und verlorene Projektionen

### Task 14: Krümmungskamm – Anzeige, Rechtsklick, Dialog, Skizzenfeld

**Dateien:**
- Create: `crates/planora-app/src/curvature_dialog.rs`
- Modify: `crates/planora-app/src/sketch_view.rs` (Zeichnen im Overlay, `finished_sketch_shapes` für die 3D-Ansicht, Rechtsklickmenü im Wählwerkzeug), `crates/planora-app/src/sketch_state.rs`, `crates/planora-app/src/sketch_panel.rs` (Schalter „Krümmungskamm“ + Dichte/Maßstab bei genau einer gewählten Kurve mit Kamm), `crates/planora-app/src/theme.rs` (`SketchColors::comb`), `crates/planora-app/src/detach.rs` (`"curvature_display"`), `keymap/defaults.rs`, `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_comb.rs` (neu)

**Schnittstellen:**
- Consumes: Task 5
- Produces:
  - `pub enum CombToggle { Dialog(Vec<CurveId>), Off(usize), Refused(Msg) }` und `SketchSession::toggle_curvature(&mut self, sk: &mut Sketch) -> CombToggle`: haben alle gewählten kammfähigen Kurven einen Kamm → aus (ein Rückgängig-Schritt); sonst Dialog für die ohne Kamm; nur Linien gewählt → `Refused` (Menüpunkt gedämpft)
  - `pub struct CurvatureDialog { pub curves: Vec<CurveId>, pub density: u32, pub scale: f64 }` mit `pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> DialogOutcome` (Typ aus `surface_project_dialog`); Vorschau live ohne Änderung der Skizze; OK setzt `set_comb` für alle Kurven in einem Schritt; Felder begrenzen auf die Bereiche aus Task 5
  - `SketchColors::comb: Color32` (Magenta-ähnlich, hell und dunkel je eigener Graphit-Ton)
  - Rechtsklick im Wählwerkzeug mit gewählter Kurve öffnet ein Menü mit „Krümmungsanzeige ein/aus“; sonst bricht Rechtsklick weiter ab wie heute (es gibt bisher kein Menü in der Skizzenansicht)
  - `pub fn comb_shapes(sk: &Sketch, to_screen: impl Fn(DVec3) -> egui::Pos2, px_per_mm: f64) -> Vec<egui::Shape>`: Zähne und Hüllkurve, `min_world = COMB_MIN_PX / px_per_mm`

- [ ] **Schritt 1: Tests schreiben:**
  ```rust
  #[test] fn toggle_on_opens_dialog_ok_sets_comb()   // Bogen gewählt: Dialog; OK → comb == Some(default), ein Schritt
  #[test] fn toggle_off_without_dialog()             // Kurve mit Kamm: Off(1), comb == None
  #[test] fn dialog_cancel_leaves_nothing()
  #[test] fn lines_only_refused()
  #[test] fn panel_edits_density_one_step()          // Dichte 35 im Skizzenfeld: comb.density == 35, ein Schritt
  #[test] fn comb_drawn_outside_sketch_mode()        // fertige Skizze sichtbar: finished_sketch_shapes enthält Kamm-Linien in Farbe comb
  #[test] fn hidden_sketch_draws_no_comb()
  #[test] fn right_click_menu_only_in_select_with_curve() // anderes Werkzeug: Rechtsklick bricht ab
  #[test] fn dialog_has_dock_key()                   // DIALOG_KEYS.contains(&"curvature_display")
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_comb` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün; `cargo test -p planora-app --lib detach` (`dialog_dock_keys`).
- [ ] **Schritt 5:** Commit `Skizzen 11a: Krümmungskamm anzeigen`. Vault-Notiz „Task 14 Krümmungskamm Anzeige“.

### Task 15: Skizzenstatus und rotes Schloss

**Dateien:**
- Create: `crates/planora-sketch/src/check.rs`
- Modify: `crates/planora-sketch/src/lib.rs`, `crates/planora-feature/src/compute.rs` (`compute_sketch`), `crates/planora-feature/src/model.rs` (`FeatureResult`, `Feature`), `crates/planora-app/src/sketch_panel.rs` (Statuszeile oben), `crates/planora-app/src/browser.rs` (Schloss am Skizzen-Eintrag), `crates/planora-app/src/icons.rs` (`Icon::Lock`, Phosphor `lock-simple`, MIT), `de.ftl`/`en.ftl`
- Test: `crates/planora-sketch/tests/sketch_check.rs` (neu), `crates/planora-feature/tests/compute.rs` (ergänzen), `crates/planora-app/tests/tree_ui.rs` (ergänzen)

**Schnittstellen:**
- Produces (`planora_sketch::check`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SketchStatus { Determined, Under, Conflict, NotSolved }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum CheckItem { Curve(CurveId), Point(PointId), Text(TextId) }`
  - `Sketch::status(&self, r: &SolveReport) -> SketchStatus`: `Conflicting` → `Conflict`, `NotConverged` → `NotSolved`, sonst `Determined`, wenn `under_items` leer, sonst `Under`
  - `Sketch::under_items(&self, r: &SolveReport) -> Vec<CheckItem>` nach A4: Kurven aus `free_curves` (ohne erzeugte Kurven von Texten), alleinstehende Punkte aus `free_points` (kein Punkt einer lebenden Kurve, kein Lagepunkt eines Textes), Texte mit einem freien Lagepunkt; Reihenfolge: Kurven, Punkte, Texte, je nach Id
  - `Sketch::is_locked(&self, r: &SolveReport) -> bool`: `Determined` und mindestens ein lebendes Element
  - `Sketch::item_ordinal(&self, it: CheckItem) -> usize`: laufende Nummer ab 1 je Art unter den lebenden Elementen derselben Art (der Browser nummeriert Skizzenelemente nicht; diese Zählung ist neu)
  - `planora-feature`: `Feature::sketch_locked(&self) -> bool` aus dem letzten Rechenstand (`compute_sketch` nimmt den Bericht von `solve()`, mit Formeln nach `solve_formulas` ein zweites `solve()`, das aus dem Zwischenspeicher kommt)
  - App: `pub fn status_line(sk: &Sketch, r: &SolveReport) -> (String, StatusTone)` mit `pub enum StatusTone { Normal, Error, Warning }` in `sketch_panel.rs`; der Browser nimmt für die offene Skizze `SketchSession.last_report`, sonst `Feature::sketch_locked`; keine Anzeige an der Zeitleiste

- [ ] **Schritt 1: Tests schreiben.**
  `sketch_check.rs`:
  ```rust
  #[test] fn free_line_is_under()                    // Linie frei: Under, under_items == [Curve(l)]
  #[test] fn line_ends_count_via_curve()             // freie Linie: keine Point-Einträge für ihre Enden
  #[test] fn lone_point_listed()                     // freier Punkt: [Point(p)]
  #[test] fn fixed_rect_is_determined_and_locked()   // Rechteck mit 2 Maßen + Fixed an einer Ecke + H/V: Determined, is_locked
  #[test] fn empty_sketch_not_locked()               // Determined, !is_locked
  #[test] fn projected_curve_never_under()           // projizierte Kurve allein: Determined
  #[test] fn conflict_status_and_only_solved_free()  // Gruppe A widersprüchlich, Gruppe B freie Linie: Conflict, under_items == [Curve(B)]
  #[test] fn not_converged_status()
  #[test] fn text_counts_as_one_item()               // Text mit freiem Rahmen: genau ein Text-Eintrag, keine Buchstabenkurven
  #[test] fn ordinal_counts_per_kind()               // drei Linien, zweite gelöscht: dritte hat Nummer 2
  ```
  `compute.rs`: `sketch_locked_after_recompute` (voll bestimmte Skizze → `true`, nach Löschen eines Maßes im Dokument → `false`).
  `tree_ui.rs`: `locked_sketch_shows_lock_icon`, `under_sketch_shows_no_lock`, `timeline_shows_no_lock`.
  App-Status (in `crates/planora-app/tests/sketch_check_ui.rs`, neu): `status_line_texts` (vier Zustände, Ton `Error` bei Widerspruch, `Warning` bei nicht gelöst), `status_follows_live_edit` (Maß setzen → „Bestimmt“ im selben Bild).
- [ ] **Schritt 2:** `cargo test -p planora-sketch --test sketch_check` → FAIL; ebenso die App-Tests.
- [ ] **Schritt 3:** Umsetzen. Die Freiheitsgrad-Anzeige im Fußfeld (`foot_field`) bleibt.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-sketch --test sketch_check`, `cargo test -p planora-feature --test compute`, `cargo test -p planora-app --test tree_ui --test sketch_check_ui`.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Skizzenstatus und Schloss`. Vault-Notiz „Task 15 Status und Schloss“.

### Task 16: „Unterbestimmte finden“ und „Skizze berechnen“

**Dateien:**
- Modify: `crates/planora-app/src/sketch_panel.rs` (Knopf, Liste mit Zeilen, X, Haken, Knopf „Skizze berechnen“), `crates/planora-app/src/sketch_state.rs` (`SketchSession.under_list: Option<UnderList>`), `crates/planora-app/src/sketch_view.rs` (Hervorhebung, Dämpfen, Ansicht verschieben/zoomen), `crates/planora-sketch/src/solver.rs` (`recompute_all`), `keymap/defaults.rs`, `de.ftl`/`en.ftl`
- Test: `crates/planora-app/tests/sketch_under_list.rs` (neu), `crates/planora-sketch/tests/sketch_check.rs` (ergänzen)

**Schnittstellen:**
- Consumes: Task 15 (`CheckItem`, `under_items`, `item_ordinal`, `status`)
- Produces:
  - `Sketch::recompute_all(&mut self) -> SolveReport`: leert `solve_cache` und löst komplett neu; ändert Werte nur, wenn das Lösen sie ändert
  - `pub struct UnderList { pub hide_determined: bool }`; die Zeilen werden je Bild aus `last_report` gebaut (kein gespeicherter Id-Bestand, darum keine toten Zeilen)
  - `SketchSession::under_rows(&self, sk: &Sketch) -> Vec<(CheckItem, String, bool /*löschbar*/)>`: Text „Linie 3“, „Punkt 7“, „Text 1“ (`sketch-check-row-*` mit `$n`); `löschbar == false` für Punkte, die Kurvenenden sind (kommt durch A4 nie vor, die Regel steht trotzdem)
  - `SketchSession::focus_item(&mut self, sk: &Sketch, it: CheckItem)`: wählt nur dieses Element
  - `pub fn focus_move(view: (DVec2, DVec2), item: (DVec2, DVec2), margin: f64) -> FocusMove` mit `pub enum FocusMove { Keep, Pan(DVec2), Zoom { center: DVec2, half_height: f64 } }`: passt das Element mit Rand (`margin` = 15 % der Ansichtsgröße je Seite) schon hinein → `Keep`; passt es in der Größe, aber nicht in der Lage → `Pan`; sonst `Zoom`
  - Liste bleibt offen bis „Schließen“ oder Verlassen der Skizze, auch in anderen Werkzeugen; X löscht (ein Rückgängig-Schritt); „Bestimmte ausblenden“ dämpft voll bestimmte Geometrie in der Ansicht
  - „Skizze berechnen“: `recompute_all`, ein Rückgängig-Schritt nur bei geänderten Werten
  - Aktionen `Action::SketchFindUnderConstrained`, `Action::SketchCompute`

- [ ] **Schritt 1: Tests schreiben.**
  `sketch_check.rs`: `recompute_all_equals_fresh_solve` (Bericht gleich einem frischen Sketch), `recompute_all_unchanged_values_bitwise` (gelöste Skizze: Werte bitgleich).
  `sketch_under_list.rs`:
  ```rust
  #[test] fn list_shows_free_elements_with_names()   // freie Linie + freier Punkt: ["Linie 1", "Punkt 1"]
  #[test] fn row_click_selects_only_that_element()
  #[test] fn row_click_pans_or_zooms()                // focus_move: Element außerhalb → Pan; zu groß → Zoom; sichtbar → Keep
  #[test] fn x_deletes_one_undo_step()
  #[test] fn under_list_follows_delete_and_undo()     // Prüf-Schwerpunkt 4: X → Zeile weg; Rückgängig → Zeile wieder da; Zeilenklick auf veraltetes CheckItem tut nichts
  #[test] fn list_stays_open_while_dimensioning()     // Werkzeug Bemaßung, Maß setzen: Liste offen, Zeile der Linie weg, Status läuft mit
  #[test] fn hide_determined_dims_geometry()          // Haken an: bestimmte Kurve gedämpft gezeichnet
  #[test] fn compute_sketch_no_step_without_change()  // Knopf: Verlauf unverändert bei gelöster Skizze
  #[test] fn list_closes_when_leaving_sketch()
  #[test] fn actions_have_no_default_binding()
  ```
- [ ] **Schritt 2:** `cargo test -p planora-app --test sketch_under_list` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün; Tests von `planora-sketch` und `cargo test -p planora-app --test sketch_ui --test undo_sketch`.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Unterbestimmte finden und Skizze berechnen`. Vault-Notiz „Task 16 Unterbestimmte finden“.

### Task 17: Schwache Bezüge in planora-feature

**Dateien:**
- Modify: `crates/planora-feature/src/model.rs` (`dependencies`, neu `weak_dependencies`, `all_dependencies`), `crates/planora-feature/src/ops.rs` (`dependents`, `check_references`), `crates/planora-feature/src/recompute.rs` (`cache_key`, `blocked_by_dependency`), `crates/planora-feature/src/rollback.rs` (`chain`, `rolled_back`), `crates/planora-feature/src/compute.rs` (`follow_copies` sammelt verlorene Ziele), `crates/planora-app/src/history_bar/model.rs`, `crates/planora-app/src/state.rs` (Zeilen 1409, 1674, 1799: `all_dependencies`)
- Test: `crates/planora-feature/tests/weak_refs.rs` (neu), `crates/planora-project/tests/format21.rs` (Lade-Test)

**Schnittstellen:**
- Produces:
  - `FeatureKind::dependencies(&self) -> Vec<FeatureId>`: bei Skizzen nur noch die Herkunft der Ebene (`OnFace`, `OnPlane`)
  - `FeatureKind::weak_dependencies(&self) -> Vec<FeatureId>`: bei Skizzen `copies[..].source.origins()`, sortiert, ohne Doppelte; sonst leer. Die schon vorhandenen losen Verweise (`loose_origins`: Quellskizzen, Konstruktionspunkte) bleiben, wie sie sind
  - `FeatureKind::all_dependencies(&self) -> Vec<FeatureId>`: Vereinigung beider = das heutige `dependencies()`
  - `Document::present_weak_dependencies(&self, idx: usize) -> Vec<FeatureId>`: schwache Bezüge des Features an `idx`, deren Quelle existiert und vor `idx` steht
  - Aufrufer: `dependents`, `check_references` → `dependencies()`; `blocked_by_dependency` → `dependencies()` (schwache Quellen werden in `follow_copies` zu „verloren“, auch unterdrückte und fehlerhafte); `cache_key` → `dependencies()` + für jeden schwachen Bezug `result_version`, wenn vor der Skizze, sonst 0 (wie `loose_origins`); `rollback::chain` und `rolled_back` → `dependencies()` + `present_weak_dependencies`; Verlauf, `refuse_if_locked`, `state.rs` → `all_dependencies()`
  - `FeatureResult.lost: Vec<CopyTarget>` und `Feature::lost_projections(&self) -> &[CopyTarget]` (Ziele aus `counts.lost`, in Reihenfolge der Kopien)
  - Die sichtbaren Körper für `follow_copies` stammen nur von Features vor der Skizze (prüfen; eine Quelle hinter der Skizze ist verloren)

- [ ] **Schritt 1: Tests schreiben** (`weak_refs.rs`):
  ```rust
  #[test] fn delete_body_keeps_projecting_sketch()   // Quader, Skizze mit projizierter Kante: dependents(quader) leer; delete(quader) Ok; Skizze bleibt, warning() nennt verlorene Projektion, lost_projections().len() == 1, Kurve an alter Lage
  #[test] fn plane_on_face_stays_hard()              // Skizze auf Fläche des Quaders: dependents(quader) == [skizze]
  #[test] fn undo_restores_link()                    // Snapshot vor dem Löschen wiederherstellen: lost_projections() leer, Kurve folgt einer Quaderänderung
  #[test] fn move_sketch_before_source_and_back()    // Prüf-Schwerpunkt 2: Verschieben vor den Quader Ok (kein ForwardReference), verloren + Warnung; zurück → nicht verloren, folgt der Kante
  #[test] fn suppressed_source_is_lost_not_skipped() // Quader unterdrückt: Skizze Status Ok mit Warnung (nicht Skipped)
  #[test] fn failing_source_is_lost_not_skipped()
  #[test] fn cache_key_follows_present_source()      // Quaderänderung rechnet Skizze neu; nach Löschen kein Neurechnen bei Änderung anderer Features
  #[test] fn rollback_chain_includes_present_source()// chain(skizze) enthält den Quader, nach dem Löschen nicht
  #[test] fn all_kinds_of_copy_source_are_weak()     // Edge, Section, Silhouette, Include(Edge), OnSurface, Intersection: delete der Quelle Ok
  #[test] fn all_dependencies_equals_old_dependencies() // für jedes FeatureKind der Testdokumente gleich der Liste vor der Aufgabe (fester Erwartungswert)
  ```
  `format21.rs`: `load_file_with_missing_weak_source` (Datei mit Skizze, deren Kantenquelle fehlt, als Fixture in `tests/fixtures_format21/missing_source.json`; Laden Ok, Warnung nach Neuberechnung, Speichern bytegleich).
- [ ] **Schritt 2:** `cargo test -p planora-feature --test weak_refs` → FAIL.
- [ ] **Schritt 3:** Umsetzen; Doku an `dependents` („schwache Bezüge zählen nicht“) und an `blocks` anpassen.
- [ ] **Schritt 4:** Grün: `cargo test -p planora-feature`, `cargo test -p planora-project --test format21`, `cargo test -p planora-app --test history_bar --test history_bar_actions --test sketch_projection_s6 --test sketch_include_3d`.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Projektionen als schwache Bezüge`. Vault-Notiz „Task 17 Schwache Bezüge“.

### Task 18: Verlorene Projektionen verwalten

**Dateien:**
- Create: `crates/planora-app/src/lost_projections_dialog.rs`
- Modify: `crates/planora-feature/src/model.rs` (`CopySource::same_family`), `crates/planora-feature/src/ops.rs` (Befehle am Dokument), `crates/planora-app/src/history_bar/view.rs` (`step_menu`), `crates/planora-app/src/browser.rs` (Menü am Skizzen-Eintrag), `crates/planora-app/src/sketch_view.rs` (gelbe Markierung, Abblenden), `crates/planora-app/src/theme.rs` (`SketchColors::lost`), `crates/planora-app/src/state.rs` (Neu-verknüpfen-Wahl), `crates/planora-app/src/detach.rs` (`"lost_projections"`), `keymap/defaults.rs`, `de.ftl`/`en.ftl`
- Test: `crates/planora-feature/tests/lost_projections.rs` (neu), `crates/planora-app/tests/lost_projections_dialog.rs` (neu)

**Schnittstellen:**
- Consumes: Task 17 (`lost_projections`, schwache Bezüge)
- Produces:
  - `CopySource::same_family(&self, other: &CopySource) -> bool`: Kante ↔ Kante, Schnitt ↔ Schnitt, Silhouette ↔ Silhouette, `Include` gleicher `IncludeRef`-Art, Skizzenkurve ↔ Skizzenkurve, `OnSurface` ↔ `OnSurface`, `Intersection` ↔ `Intersection`
  - `Document::relink_projection(&mut self, sketch: FeatureId, target: CopyTarget, source: CopySource) -> Result<(), FeatureError>`: andere Familie → `FeatureError::Sketch(tr!("feature-relink-kind"))`; setzt die Quelle, rechnet neu (Abhängigkeiten bleiben, soweit die Stückzahl gleich bleibt)
  - `Document::unlink_projections(&mut self, sketch: FeatureId, targets: &[CopyTarget]) -> Result<usize, FeatureError>` (über `SketchFeature::break_link`)
  - `Document::delete_lost_geometry(&mut self, sketch: FeatureId, targets: &[CopyTarget]) -> Result<usize, FeatureError>`: löscht die Kurven/Punkte samt Kopie-Eintrag
  - `pub struct LostProjectionsDialog { pub sketch: FeatureId, pub dim_rest: bool, pub relink: Option<CopyTarget> }` mit `pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> DialogOutcome`; je Zeile „Neu verknüpfen“, „Verknüpfung aufheben“, „Geometrie löschen“; unten „Alle löschen“, „Alle Verknüpfungen aufheben“, Haken „Übrige Geometrie abblenden“ (dämpft alles außer den verlorenen Kurven, solange der Dialog offen ist); jede Aktion ein Rückgängig-Schritt
  - „Neu verknüpfen“ öffnet die Skizze (falls zu) und setzt `SketchSession.relink: Option<RelinkPick { target: CopyTarget, family: CopySource }>`; der nächste passende Klick auf Kante/Fläche/Körper ruft `AppState::relink_projection(..)`; Esc nimmt die Wahl zurück
  - Menüeintrag „Verlorene Projektionen verwalten“ in `step_menu` und im Baum nur an Skizzen mit `!lost_projections().is_empty()`; Aktion `Action::ManageLostProjections`
  - `SketchColors::lost: Color32` (Gelb wie die Warnung), Markierung verlorener Kurven in Skizzenansicht und 3D-Ansicht

- [ ] **Schritt 1: Tests schreiben.**
  `lost_projections.rs`:
  ```rust
  #[test] fn relink_edge_to_other_edge()            // Quader A gelöscht, Quader B: relink auf Kante von B → nicht verloren, Kurve auf B-Kante, Maß an der Kurve bleibt
  #[test] fn relink_other_family_refused()          // Kante → Fläche (Schnitt): Err, Dokument unverändert
  #[test] fn unlink_keeps_curve_in_place()          // Kurve bleibt, copies ohne Eintrag, keine Warnung mehr
  #[test] fn delete_lost_geometry_removes_curve()
  #[test] fn break_all_and_delete_all()             // zwei verlorene: unlink_projections(&beide) == 2
  ```
  `lost_projections_dialog.rs`:
  ```rust
  #[test] fn menu_entry_only_with_lost()            // step_menu enthält den Eintrag nur bei verlorener Projektion
  #[test] fn dialog_lists_lost_rows()
  #[test] fn unlink_row_is_one_undo_step()
  #[test] fn delete_all_is_one_undo_step()
  #[test] fn dim_rest_dims_other_curves()           // Haken an: nicht verlorene Kurven gedämpft, verlorene gelb
  #[test] fn relink_pick_then_click_edge()          // Neu verknüpfen, Klick auf Kante → verknüpft, ein Schritt
  #[test] fn relink_esc_leaves_nothing()
  #[test] fn delete_body_dialog_no_longer_names_sketch() // Löschen-Dialog des Quaders nennt die Skizze nicht
  #[test] fn lost_curve_drawn_yellow()
  #[test] fn dialog_has_dock_key()                  // DIALOG_KEYS.contains(&"lost_projections")
  ```
- [ ] **Schritt 2:** `cargo test -p planora-feature --test lost_projections` und `cargo test -p planora-app --test lost_projections_dialog` → FAIL.
- [ ] **Schritt 3:** Umsetzen.
- [ ] **Schritt 4:** Grün; `cargo test -p planora-app --lib detach`, `cargo test -p planora-app --test feature_tree --test history_bar_ui`.
- [ ] **Schritt 5:** Commit `Skizzen 11a: Verlorene Projektionen verwalten`. Vault-Notiz „Task 18 Verlorene Projektionen“.

### Handtest Etappe 4 (kein Code, nur nach Dennis' Go)

- [ ] Dennis fragen und Go abwarten; Start wie beim ersten Handtest.
- [ ] Liste: Kamm an Bogen/Ellipse/Spline ein und aus (Rechtsklick, Dialog, Skizzenfeld), Kamm außerhalb der Skizze; Status in allen vier Zuständen; „Unterbestimmte finden“ bis zum roten Schloss, Zeilenklick, X, Haken, „Skizze berechnen“; Quader mit projizierter Kante löschen → Skizze bleibt gelb markiert, Dialog, Neu verknüpfen, Verknüpfung aufheben, Alle löschen, Rückgängig; Speichern und Laden.
- [ ] Vault-Notiz „Handtest 2“; Funde als Korrekturaufgaben; Scratchpad aufräumen.

## Etappe 5: Doku und Abschluss

### Task 19: Doku

**Dateien:**
- Modify: `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 11a“: Annahmen A1–A10 mit Begründung, Messwerte aus dem Ledger, Schwelle `SPARSE_MIN_VARS`, Grenzen: Körperkanten als Quelle der Übergangskurve und Kamm an Körperkanten offen; A9 schließt den Punkt aus Skizzen 5 zu den Feldern am Zeiger: dort durchstreichen mit „Erledigt in Skizzen 11a: wie Fusion, A9“; die Zeile „Ebene Formen in 3D ohne Felder am Zeiger“ aus Skizzen 10 als erledigt durch A8 markieren)
- Modify: `CHANGELOG.md` (Abschnitt „Unveröffentlicht“), `docs/funktionen.md` (Abschnitt „Skizzen“: Mittelpunktlinie, Übergangskurve, Krümmungskamm, Status und Unterbestimmte finden, verlorene Projektionen; einfache Sprache), `CLAUDE.md` (Architekturzeile: `sparse.rs`, `blend.rs`, `comb.rs`, `check.rs`, `curvature_dialog.rs`, `lost_projections_dialog.rs`; Konvention „Projektionsbezüge sind schwach: `weak_dependencies`“)

- [ ] **Schritt 1:** Doku schreiben; keine Bilder (kommen beim Release).
- [ ] **Schritt 2:** `graphify . --update --code-only`.
- [ ] **Schritt 3:** Commit `Skizzen 11a: Doku und offene Punkte`. Vault-Notiz „Task 19 Doku“.

## Abschluss

- [ ] **Voller Lauf Windows** (nacheinander, nie zwei cargo-Läufe gleichzeitig):
  - `cargo fmt -p <crate> -- --check` für jedes Crate des Workspace
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `cargo deny check`
  - Messtests im Release: `cargo test -p planora-sketch --release -- --ignored --nocapture` (darin `three_d_perf`, `import_perf` und die übrigen Messtests) und die Messtests der anderen Crates wie bisher

  Erwartet: alles grün außer dem bekannten `real_step_text_limit`; Messziele aus „Globale Vorgaben“ erfüllt, Werte in den Ledger.
- [ ] **Voller Lauf WSL:** erst `pgrep -af 'cargo|rustc|wsl-s6'` ohne Treffer (auch kein Lauf einer anderen Sitzung), dann `~/wsl-s6-full.sh`. Erwartet: grün außer `real_step_text_limit` und `a_locked_old_json_never_ends_up_next_to_a_new_png` (root). Danach erneut `pgrep -af 'cargo|rustc|wsl-s6'` prüfen, `/root/target-s6` und `/root/PlanoraCAD/target` löschen, `df -h /` prüfen und den freien Platz in der Vault-Notiz nennen.
- [ ] **Gesamtprüfung** des Zweigs durch einen frischen Prüfer (Spec gegen Code, Prüf-Schwerpunkte, Format 21, 2D-Gleichheit); Funde beheben, Lauf wiederholen.
- [ ] **Handtest Ende nur nach Dennis' Go:** alle Punkte aus Handtest 1 und 2 im Zusammenhang an einem Projekt, dazu eine alte Datei laden und speichern (bytegleich) und eine Datei mit Format 21 in einer älteren Version (Meldung „zu neu“).
- [ ] Vault-Notiz „Abschluss“, Übergabe in `Projekte/PlanoraCAD/Sessions/`, Roadmap-Zeile „Skizzen 11a“ pflegen.
- [ ] **Merge nur auf „bitte mergen“** von Dennis; Zweig nur auf sein Wort löschen.
