# Offene Punkte B (7–11) – Kurzentwurf

Stand 2026-11-24. Freigegeben von Dennis („Mache komplett B“, Punkt 12 „Auf Etappe 2“).

## Warum
Dennis will vor dem nächsten Push alle technischen Restpunkte aus der Liste B erledigt haben.
Punkt 12 (Kleinpunkte Plug-ins Etappe 1) wird getrennt auf dem Zweig `feature/plugins-etappe-2`
gebaut, weil Etappe 2 fast alle betroffenen Dateien ändert; er kommt mit Etappe 2 nach master.

## Ziele
1. **Absturzbericht (7):**
   - `abort` (SIGABRT, Windows-CRT und Linux) schreibt noch einen Bericht mit Aufrufliste.
   - `__fastfail` und Beenden von außen: Der nächste Start schreibt selbst einen Bericht aus dem
     Laufmarker (Sitzungsbeginn, Version) und den während des Laufs mitgeschriebenen letzten
     Protokollzeilen und Schritten.
   - Harter Windows-Absturz: Aufrufliste mit Namen über `Backtrace::force_capture` auf dem
     abstürzenden Faden; nur beim Stapelüberlauf bleibt es bei Adressen.
2. **`detach.rs` aufteilen (8)** in Untermodule (`place`, `state`, `monitor`, `decide`, `window`,
   `contents`, `keys`, `strip`); alle Pfade `detach::…` bleiben über `pub use` gleich.
3. **C15-Reste (9):**
   - Kegelspitze: ein Spitzenpunkt je Fächer statt Splitterdreiecken.
   - Übergangssuche: Probenzahl nach Kantenlänge und zusätzliche Suche an Abstandsminima, damit
     schmale Berührzonen (≈0,02 mm) auf langen Stücken gefunden werden.
   - Kernel: Ist die ranghöhere Fläche nicht umwandelbar, die andere versuchen.
4. **C12-Reste (10):**
   - `solve_holding_in_steps` ohne Kopie der Skizze.
   - Wächter, der nach einer Panik Festpunkte, abgeschaltete Bedingungen, Toleranz und
     Zwischenspeicher zurücknimmt.
   - Rahmenprobe in `text.rs` ohne eigene Kopie (Wiederherstellung schließt Texte und Aufbau ein).
5. **Wackeltest (11):** `holding_in_place_is_not_slower_on_a_large_sketch` zählt Speicher-
   anforderungen (zählender Allocator nur in der Testdatei) statt Zeit; Zeit wird nur ausgegeben.

## Nicht-Ziele
- Kein Speicherabbild (Minidump), keine Windows-Fehlerberichterstattung (WER) umstellen.
- Kein Bericht beim Stromausfall oder Abmelden außer dem Bericht beim nächsten Start.
- Keine Verhaltensänderung beim Abdocken.

## Ansätze
- Absturzbericht: (a) Bericht beim nächsten Start aus Marker und Mitschrift plus SIGABRT-Abfang
  (gewählt: deckt alle drei Fälle ab, ohne fremden Prozess); (b) eigener Wächterprozess wie
  Fusions Crash-Reporter (verworfen: zweiter Prozess, Installer-Änderung); (c) WER-LocalDumps
  (verworfen: Systemeinstellung).
- Übergangssuche: (a) feste höhere Probenzahl (verworfen: Kosten auf allen Paaren);
  (b) Probenzahl nach Länge plus Minimumsuche (gewählt).
- C12-Wächter: Drop-Guard über den Wertestand bzw. `catch_unwind` mit Aufräumen; Wahl beim Bau
  nach dem Borrow-Zuschnitt (meine Entscheidung).

## Dem Bauenden überlassen
Dateinamen der Mitschrift, genaue Probendichte (Ziel: Zone ≥ 0,01 mm wird gefunden, Laufzeit der
Zufallsreihe höchstens +20 %), Texte nur über `tr!`/`msg!` in de/en.
