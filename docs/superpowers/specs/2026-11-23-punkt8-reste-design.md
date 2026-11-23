# Reste von Punkt 8 (Kurzentwurf)

Freigegeben von Dennis am 2026-11-23 („Ja, so bauen“). Die Punkte stammen aus
`offene-punkte.md`, Abschnitt „Reste der Härtungen nach Skizzen 5“ (Zurückgestellt und Grenzen).

## Absicht

Die sieben kleinen Reste aus den Härtungen nach Skizzen 5 schließen. Normale Fälle rechnen genau
wie bisher; Dateien ändern sich nicht.

## Punkte

1. **Sprache des Grunds:** „Das lässt sich nicht dorthin ziehen: …“ und „… nicht lösbar: …“
   tragen den Grund als Schlüssel (`Msg`), nicht als fertigen Text. Die stehende Meldung wechselt
   mit der Sprache. `PatternProblem::key()` liefert die Id; `PatternProblem::message()` bleibt.
2. **`add_point3`** führt die Gruppen der Schleifenprüfung fort wie `add_point`.
3. **Nach einer Ablehnung** (Bogen, der Vorlage und eigenes Exemplar verbände) bleiben die Gruppen
   erhalten: Die Prüfung läuft gegen die Gruppen vor dem Anlegen, ohne sie zu ändern
   (`closes_cycle` ist schon eine Probe). Erst wenn nichts abgelehnt wird, werden sie fortgeführt.
   Hängen mehrere interne Abhängigkeiten nicht über gemeinsame, nicht gehaltene Variablen
   zusammen, bleibt der alte Weg (fortführen, prüfen, bei Ablehnung verwerfen), weil die
   gemeinsame Probe dann zu streng wäre.
4. **`free_points`** nennt Punkte einer überholten Gruppe nicht mehr (passend zu `dof`); ebenso
   `free_curves`. Dazu trägt der Bericht intern die freien Variablen.
5. **Rücknahmetests** für elliptischen Bogen, 3D-Bogen und elliptischen 3D-Bogen: Eine Ablehnung
   lässt die Skizze unverändert.
6. **`max_rounds`:** Der Freiheitsgrad-Test prüft, dass die Musterrunden bis zur Höchstzahl liefen
   (`group_solves`).
7. **Messtest ohne feste Zeit:** 200 Bögen werden mit 200 Kopien der Skizze verglichen (der alte
   Weg kopierte je Bogen); der neue Weg muss mindestens zehnmal schneller sein.

## Ansätze

Zu 3 gab es eine Wahl: die Gruppen vor dem Anlegen kopieren (kostet je Bogen eine Kopie, also
genau das, was die Härtung abgeschafft hat) oder vor dem Fortführen prüfen (gewählt, kostet
nichts). Zu 4: den Bericht nachträglich aus Punkten raten (ungenau, weil x und y eines Punkts in
verschiedenen Gruppen liegen können) oder die freien Variablen mitführen (gewählt).

## Nachweis

Je Punkt ein Test, der vorher scheitert (bei 5 und 7 Absicherung ohne Rot); volle Prüfung unter
Windows und WSL; Handtest nur nach Go.
