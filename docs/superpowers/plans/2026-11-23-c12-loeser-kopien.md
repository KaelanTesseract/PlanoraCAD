# C12 Löser ohne Kopie je Bild – Plan

**Spec:** `docs/superpowers/specs/2026-11-23-c12-loeser-kopien-design.md`
**Ausführung:** native (von Claude gewählt).

## Global Constraints

- Verhalten bitgleich zum alten Weg; Texte nur über tr!/msg!.
- `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_*_DEBUG=0`, ein Cargo-Lauf zur Zeit, `cargo fmt -p`.

## Review Focus

- Früher Abbruch in `solve_holding` (Anheften scheitert, Punkt unbekannt) muss alles zurücksetzen.
- Festpunkt an einem Musterexemplar (Ablehnung `InstanceOnly`) und an 3D-Punkten (`Fixed3`).
- Schleifengruppen und Lösungsspeicher nach Erfolg und Scheitern wie beim alten Weg.
- Kurvenzug-Wege, die mehr als Werte ändern, dürfen nicht den Wertestand nutzen.
- `solve_holding_in_steps` (Halbieren) bleibt bei der Kopie, weil es Maßwerte setzt.

### Task 1: Wertestand und `solve_holding` an Ort und Stelle (planora-sketch)

- `model.rs`: Feld `structure: u64` (in `touch_structure` und `new` gesetzt), `pub struct
  ValueSnapshot`, `pub fn value_snapshot(&self)`, `pub fn restore_values(&mut self, ValueSnapshot)`
  (Debug-Prüfung Strukturstand), `pub(crate) fn restore_snapshot_unchecked`.
- `solver.rs`: `solve_holding` und `solve_holding_connected` ohne `clone`; alter Weg als
  `#[doc(hidden)] pub fn solve_holding_cloning` für Vergleich und Messung.
- Tests `tests/holding_in_place.rs`: gleiche Werte und Stände alt/neu bei Erfolg, Scheitern,
  Anheften am Exemplar, 3D-Punkt; `to_data` nach Scheitern gleich; Messtest (ignored) großer Skizze.

### Task 2: Kurvenzug und Freiheitsgrade (planora-app)

- `curve_drag_to`: gewöhnlicher Zug ohne `aim` sichert `ValueSnapshot`, sonst volle Kopie;
  `same_values` gegen den gesicherten Wertevektor.
- `dof_of` nutzt `status_cache`/`last_report`, wenn der Stand passt.
- Tests: Zug abgelehnt → Skizze und Stand wie vorher; Zug ohne Bewegung → kein neuer Stand.
