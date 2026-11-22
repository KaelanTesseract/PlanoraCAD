# Reste der Härtungen nach Skizzen 5 (Kurzentwurf)

Freigegeben von Dennis am 2026-11-22 („Ja, so bauen“). Die Reste stammen aus Skizzen 12, Task 13
(`offene-punkte.md`, Abschnitt Task 13).

## Absicht

Drei kleine Reste der Härtungen schließen: eine genauere Meldung, weniger Kopieren, ein fehlender
Test. Am Verhalten der Skizze ändert sich nichts.

## 1. Genaue Meldung bei Gitterfehler beim Ziehen

- Heute: Scheitert das Gitter eines Musters erst in der Nachführung (z. B. Richtungslinien beim
  Ziehen parallel), meldet der Löser nur `NotConverged`. Der Zug wird abgelehnt mit
  „Das lässt sich wegen der Abhängigkeiten nicht dorthin ziehen“, der Status heißt „Nicht lösbar“.
- Neu: `refresh_patterns` merkt sich das erste `PatternProblem`; `SolveReport` bekommt
  `pattern_problem: Option<PatternProblem>`. Der abgelehnte Zug zeigt dessen Meldung; der Status
  „Nicht lösbar“ hängt den Grund an.
- Nicht: Zugregeln ändern. Der Zug bleibt abgelehnt.

## 2. Bogen anlegen ohne Kopie der Skizze

- Heute: `with_internal_checked` kopiert mit Mustern die ganze Skizze (`atomically`), um bei einer
  Schleife zurückzurollen; das kostet je Bogen eine Kopie.
- Neu: Ohne Anheben (2D) nur anhängen: `append_mark`, bei Ablehnung `cut_back` und den alten Stand
  (`restore_revision`) zurück. Mit Anheben in 3D (`apply_lift` ändert Vorhandenes) bleibt die Kopie.
- Nachweis: Messtest (`#[ignore]`, release) und Test, dass eine Ablehnung die Skizze unverändert
  lässt (Daten und Stand).

## 3. Test der Freiheitsgrade im Sonderfall

- `flag_stale_pattern_reads` zählt überholte Gruppen nicht mehr bei `dof`. Von außen kaum
  erreichbar, daher ein Test in der Bibliothek, der eine Schleife zwischen Vorlage und Exemplar
  erzwingt (`push_internal` umgeht die Prüfung).

## Ansätze

Je Rest gibt es einen sinnvollen Weg (Begründung in der Unterhaltung). Verworfen: für Rest 2 die
Kopie auch in 3D vermeiden (Werte merken und zurückschreiben) — mehr Code für einen seltenen Fall.
