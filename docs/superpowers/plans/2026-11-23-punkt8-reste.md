# Reste von Punkt 8 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die sieben Reste aus den Härtungen nach Skizzen 5 schließen.

**Architecture:** Änderungen in `planora-sketch` (Modell, Schleifengruppen, Löserbericht) und eine
kleine in `planora-app` (Meldungen als Schlüssel). Keine Formatänderung.

**Tech Stack:** Rust, egui-App, Fluent-Texte.

**Spec:** `docs/superpowers/specs/2026-11-23-punkt8-reste-design.md`

## Global Constraints

- Texte nur über `tr!`/`msg!`; keine neuen Schlüssel nötig (vorhandene Ids wiederverwenden).
- Normale Fälle rechnen unverändert; Dateien bytegleich.
- Prüfen: `cargo fmt -p <crate> -- --check`, clippy `-D warnings`, Tests; CARGO_INCREMENTAL=0,
  CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0; nur ein cargo-Lauf zur Zeit.

## Review Focus

- Bogen um einen gehaltenen (Exemplar-)Mittelpunkt: Die Probe muss gleich entscheiden wie der alte Weg.
- 3D-Bogen mit zwei internen Abhängigkeiten (ArcRadius, InPlane): gemeinsame Probe nur bei Zusammenhang.
- Punkt, dessen x und y in verschiedenen Gruppen liegen: bleibt frei, wenn nur eine überholt ist.
- Debug-Prüfung `same_as` nach einer Ablehnung: die Gruppen gleichen einem Neubau.
- Sprachwechsel bei stehender Meldung mit Grund `TooMany` (Werte max/curves).

---

### Task 1: Grund als Schlüssel (Spec 1)

**Files:** `crates/planora-sketch/src/pattern.rs`, `crates/planora-app/src/sketch_state.rs`,
Test `crates/planora-app/tests/sketch_pattern_reason.rs`

- [ ] Test `the_blocked_reason_follows_the_language`: Meldung aus `drag_blocked_message` mit
  `parallel_report()` bauen, Sprache de → Text enthält den deutschen Grund, Sprache en → derselbe
  `Msg` liefert den englischen Grund. Vorher rot (fester Text).
- [ ] `PatternProblem::key(self) -> &'static str`; `message()` nutzt sie. In der App
  `pub fn pattern_reason(p: PatternProblem) -> Msg` (TooMany mit `max`, `curves`), eingebettet als
  `MsgArg::Msg`. Beide Meldungsbauer nutzen sie.
- [ ] Test grün, Commit.

### Task 2: Schleifengruppen (Spec 2, 3)

**Files:** `crates/planora-sketch/src/model.rs`, `crates/planora-sketch/src/pattern_cycles.rs`,
Tests `crates/planora-sketch/tests/hardening_s5.rs`

- [ ] Test `a_refused_arc_keeps_the_cycle_groups` und `adding_3d_points_keeps_the_cycle_groups`:
  über den Zähler `Sketch::cycle_group_builds()` (Testzähler, `#[doc(hidden)] pub`): nach Ablehnung
  bzw. `add_point3` baut die nächste Prüfung nicht neu. Vorher rot.
- [ ] `add_point3` ruft `carry_cycle_groups(before)`.
- [ ] `Sketch::appended_closes_cycle(&self, before: u64, sets: &[Vec<usize>]) -> Option<bool>`
  in `pattern_cycles.rs`; `with_internal_checked` nutzt sie vor `follow_appended`; `None` → alter Weg.
- [ ] Tests grün, Commit.

### Task 3: Freie Punkte ohne überholte Gruppe (Spec 4, 6)

**Files:** `crates/planora-sketch/src/solver.rs`, Test in `pattern.rs`
(`a_stale_pattern_group_does_not_count_its_freedom`)

- [ ] Test erweitern: `!report.free_points.contains(&a)`, `report.group_solves >= PATTERN_ROUNDS`.
- [ ] `SolveReport` bekommt `pub(crate) free_vars: Vec<usize>`; `fn free_lists(&self, free: &HashSet<usize>) -> (Vec<PointId>, Vec<CurveId>)`
  aus dem Ende von `solve_round` gelöst; `flag_stale_pattern_reads` nimmt die freien Variablen
  überholter Gruppen heraus und baut die Listen neu.
- [ ] Test grün, Commit.

### Task 4: Rücknahmetests und Messtest (Spec 5, 7)

**Files:** `crates/planora-sketch/tests/hardening_s5.rs`

- [ ] `a_refused_elliptic_arc_leaves_the_sketch_unchanged`, `a_refused_3d_arc_leaves_the_sketch_unchanged`,
  `a_refused_elliptic_3d_arc_leaves_the_sketch_unchanged`: `to_data()` und `revision()` gleich,
  danach `solve()` gelöst.
- [ ] Messtest vergleicht 200 Bögen mit 200 `clone()` der Skizze: `ms * 10.0 < clone_ms`.
- [ ] Commit; dann volle Prüfung Windows und WSL, frischer Prüfer.
