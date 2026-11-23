# C15b: Sichtbarkeit an genäherten Kanten (Kurzentwurf)

Dennis: „Sichtbarkeit angehen“ (2026-11-23). Teil von C15 (Zweig `feature/c15-allgemeine-lage`).

## Warum

Seit C15 entstehen Körper mit Freiformkanten (Schnittkurven in allgemeiner Lage, nur auf
`NURBS_EDGE_TOL` = 1e-5 mm genau). Zeichnungen solcher Körper brechen oft mit „Sichtbarkeit nicht
eindeutig“ ab oder zeigen Stücke falsch sichtbar/verdeckt – auch bei realistischen Fällen
(schräge Bohrung, Zylinder durch Torus, Kugel mit Bohrung).

## Ursache (gemessen)

Umrisse und Kanten enden im Bild tangential auf einer genäherten Kante (Umriss der Bohrung auf der
Schnittkurve). Bei exakten Kanten ist die Zone, in der Prüfstrahlen den Rand der Nachbarfläche
streifen, etwa 2,5e-4 mm lang und wird überbrückt (`inherit_short`, `OWN_EPS`). Bei genäherten
Kanten liegt der Berührpunkt nur auf 1e-5 mm genau; die unsichere Zone wird `√(2·R·1e-5)`, also
0,006–0,02 mm lang, und die Ansicht bricht ab.

## Entwurf

1. **Nachbarregel:** Randtreffer auf einer Nachbarfläche, die über eine Freiformkante an einer
   eigenen Fläche des Strichs hängt, zählen bis zum Abstand `√(2·E·NURBS_EDGE_TOL)` nicht (wie
   Treffer auf eigenen Flächen bis `OWN_EPS`). Treffer im Inneren solcher Flächen zählen weiter.
   (Probe: Abbrüche in der Zufallsreihe 30 → 17 von 1400 Ansichten, bei den sieben realistischen
   Fällen rund 25 → 2.)
2. Danach die übrigen Fälle einzeln nach Ursache: Abbrüche, falsch sichtbare/verdeckte Stücke,
   Punktüberdeckungen am Ende zweier Stücke derselben Kante.
3. Verworfen nach Probe: Prüfstrahlen vom wahren Schnittpunkt der beiden Flächen starten (machte
   mehr Abbrüche).

## Ziel und Prüfung

Zufallsreihe `stage8_merge_series` und die sieben realistischen Fälle (Prüfung `check_view`)
ohne Fehler und ohne Abbruch; alle übrigen Zeichnungstests grün. Volle Läufe Windows und WSL,
Prüfer, Handtest nur nach Go.

## Grenze

Was sich mit vertretbarem Aufwand nicht lösen lässt (Beinahe-Berührungen aus dem 0,5-mm-Raster),
lege ich mit Messwerten vor, bevor ich etwas ausnehme.
