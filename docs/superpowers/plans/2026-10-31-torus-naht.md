# Torusbänder ohne Naht – Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this
> plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Der STEP-Leser fügt Torusflächen ohne Naht (Band um die Achse, Rohrbogen, voller Ring)
eine Naht hinzu, statt den Körper auszulassen.

**Architecture:** Alles in `crates/planora-step/src/read/seam.rs`. Die Nahtsuche bekommt eine
Richtung (u oder v); `insert_seam` erhält einen Zweig für den Torus.

**Tech Stack:** Rust, Workspace PlanoraCAD.

**Spec:** `docs/superpowers/specs/2026-10-31-torus-naht-design.md`

## Global Constraints

- Keine Änderung am Kernel, keine neuen Meldungstexte.
- Alle vorhandenen Tests bleiben grün; `cargo clippy --workspace --all-targets -- -D warnings`
  sauber.
- Tests mit `-j 2` (Auslagerungsdatei).

## Review Focus

- Band, das über `v = π` läuft (Rundung am Rand einer Bohrung): Volumen stimmt.
- Fläche mit `same_sense = .F.` (Hohlraum, Bohrung): Seite stimmt.
- Kreis falsch herum: Meldung, kein Körper.
- Breitenkreis, dessen Ecke nicht beim gesuchten Winkel liegt: Kante wird geteilt.
- Querbohrung im Band: Naht hält Abstand.

---

### Task 1: Richtung in der Nahtsuche

**Files:** Modify `crates/planora-step/src/read/seam.rs`

- [ ] `u_track`, `crossings`, `free_angle`, `vertex_on`, `hole_ranges`, `u_extent` bekommen
  `dir: usize` (0 = u, 1 = v); Periode aus `u_period`/`v_period`; Lochabstand für `dir = 1` am
  Rohrradius.
- [ ] Alle bisherigen Aufrufe mit `dir = 0`; `cargo test -p planora-step -j 2` grün.
- [ ] Commit.

### Task 2: Band um die Achse

**Files:** `seam.rs`, `tests/data/seamless_torus.step`, `tests/read_seams_torus.rs`

- [ ] Proben: Welle 10/40 mit Rundung 2 an der oberen Kante, Bohrung mit gerundetem Rand
  (Band über `v = π`), Gegenprobe Kreis falsch herum.
- [ ] Test `torus_band_around_axis` (Volumen nach Pappus, Seite über Probepunkt) und
  `torus_band_wrong_circle_is_skipped` schlagen fehl.
- [ ] Zweig in `insert_seam`: Torus, zwei Ränder mit Windung `[±1, 0]`, gegenläufig. Anfang =
  Rand mit Windung `s`; Naht Meridian von dort in wachsendem `v` zum anderen Rand.
- [ ] Tests grün, Commit.

### Task 3: Rohrbogen

- [ ] Probe: Rohrbogen 90°, R 20, r 5, zwei ebene Deckel. Test `torus_elbow` (Volumen
  `π r² · R · π/2`), fehlschlagend.
- [ ] Zweig: zwei Ränder mit Windung `[0, ±1]`; Naht Breitenkreis beim freien `v_s` (Suche
  mit `dir = 1`), vom Anfangsrand in wachsendem `u`.
- [ ] Tests grün, Commit.

### Task 4: Voller Ring und Löcher

- [ ] `seamless.step` „Ring“: jetzt Volumen `2π² R r²`; Test umbenennen.
- [ ] Band mit Querbohrung (Loch im Band): Naht hält Abstand, Volumen.
- [ ] Ring mit nur einem Loch: bisherige Meldung bleibt (Gegenprobe).
- [ ] Commit.

### Task 5: Doku und Gesamttest

- [ ] `docs/funktionen.md` (STEP-Import), `CHANGELOG.md`, `offene-punkte.md`, Modul-Doku.
- [ ] `cargo test --workspace -j 2`, clippy, Commit.
