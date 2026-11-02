# Skizzen 3: Zeichenvarianten

Stand 2026-11-02. Drittes von zehn Skizzen-Teilprojekten (Vergleich in der Vault-Notiz „2026-10-31 Vergleich Skizzen mit Fusion“). Dennis hat alle Einzelheiten an Claude übergeben („selber entscheiden“), mit der Vorgabe: **die Funktionen genauso wie in Fusion**. Alles, was hier steht, ist Claudes Entscheidung; wo das Verhalten von Fusion aus dem Gedächtnis beschrieben ist, gilt es als Annahme und kommt in `offene-punkte.md`.

## Warum

Heute gibt es je Form genau einen Weg: Rechteck über zwei Ecken, Kreis über Mitte und Rand, Bogen über Mitte, Start und Ende. Polygon und Langloch fehlen ganz, und tangentiale Bögen und Kreise lassen sich nur mühsam über Abhängigkeiten bauen. In Fusion sind das die alltäglichen Werkzeuge. Sie stehen hier auf der Eingabe aus Teilprojekt 2 (Felder am Zeiger, Fang, Automatik, Strg), und genau so sollen sie sich anfühlen.

## Ziele

1. **Rechteck.** Vorhanden: 2 Punkte. Neu:
   - **3 Punkte:** erste Kante über zwei Punkte, dritter Punkt gibt die Tiefe. Das Rechteck darf schräg liegen. Felder: Breite und Winkel für die Kante, dann Tiefe. Bleibt rechtwinklig über Rechtwinklig- und Parallel-Abhängigkeiten.
   - **Mitte:** Mittelpunkt, dann eine Ecke. Felder: Breite und Höhe. Der Mittelpunkt ist ein Punkt, der über zwei Konstruktionsdiagonalen mit Mittelpunkt-Abhängigkeit an das Rechteck gebunden ist, wie in Fusion.
2. **Kreis.** Vorhanden: Mitte und Durchmesser. Neu:
   - **2 Punkte:** die Endpunkte eines Durchmessers. Feld: Durchmesser.
   - **3 Punkte:** drei Punkte auf dem Umfang. Vorschau ab dem dritten Punkt am Zeiger.
   - **2 tangential:** zwei Linien anklicken, dann Durchmesser per Feld oder Zeiger.
   - **3 tangential:** drei Kurven anklicken (Linie, Kreis, Bogen). Von mehreren Lösungen gilt die, die dem Zeiger am nächsten liegt; die Vorschau zeigt sie vor dem letzten Klick.
   Die Abhängigkeiten „tangential“ und, wo gefangen wurde, „Punkt auf Kurve“ entstehen von selbst.
3. **Bogen.** Vorhanden: Mitte, Start, Ende. Neu:
   - **3 Punkte:** Start, Ende, dann ein Punkt auf dem Bogen. Felder: Radius beim dritten Punkt.
   - **Tangential:** Start auf dem Ende einer Linie oder eines Bogens; der Bogen läuft dort tangential los. Feld: Radius, Öffnungswinkel. Beginnt der Start nicht auf einem solchen Ende, meldet die Statuszeile das und es entsteht nichts.
   - **Aus dem Linienwerkzeug:** Im laufenden Linienzug die linke Taste auf dem letzten Punkt gedrückt halten und ziehen: es entsteht ein tangentialer Bogen, danach läuft der Linienzug weiter als Linie, tangential am Bogenende, wie in Fusion. Ohne Vorgängerkurve entsteht beim Ziehen nichts Besonderes.
4. **Polygon.** Neu, mit drei Arten:
   - **Umbeschrieben:** Mitte, dann Punkt auf der Mitte einer Seite.
   - **Einbeschrieben:** Mitte, dann Punkt auf einer Ecke.
   - **Kante:** zwei Punkte der ersten Seite, dann die Seite, auf der das Polygon liegt.
   Felder: Seitenzahl (Vorgabe 6, 3 bis 64) und Größe (Radius des Kreises bzw. Kantenlänge); Tab wechselt. Die zuletzt benutzte Seitenzahl gilt für das nächste Polygon der Sitzung. Es besteht aus Linien mit gleichen Längen und einem Konstruktionskreis um die Mitte, sodass es sich als Ganzes ziehen, drehen und bemaßen lässt. Es hat genau 4 Freiheitsgrade (Mitte, Größe, Drehung).
5. **Langloch.** Neu, mit fünf Arten:
   - **Mitte zu Mitte:** die beiden Bogenmitten, dann die Breite.
   - **Gesamt:** die beiden äußeren Enden, dann die Breite.
   - **Mittelpunkt:** Mitte des Langlochs, eine Bogenmitte, dann die Breite.
   - **3 Punkte Bogen:** Start, Ende und ein Punkt auf dem Mittelbogen, dann die Breite.
   - **Mittelpunkt Bogen:** Mitte, Start und Ende des Mittelbogens, dann die Breite.
   Felder: Länge und Breite (bei den Bogenarten: Radius, Öffnungswinkel, Breite). Es besteht aus Linien oder Bögen an den Seiten und zwei Halbkreis-Bögen an den Enden, tangential verbunden, mit einer Konstruktionslinie oder einem Konstruktionsbogen als Mittelachse. Seiten und Enden behalten beim Ändern der Maße ihre Form. Es ergibt eine einzige Fläche als Profil.
6. **Mittellinie.** Ein dritter Linienstil neben „normal“ und „Konstruktion“: strichpunktiert gezeichnet, begrenzt wie Konstruktion keine Profile, wählbar im Skizzenfeld über den Schalter „Mittellinie“ (wie „Konstruktion umschalten“, gilt für die Auswahl). Beim Drehen dient sie wie jede Linie als Achse.
7. **Bedienung wie Fusion.**
   - In der Werkzeugleiste trägt das Symbol einer Form ein kleines Dreieck. Klick wählt die zuletzt benutzte Art, rechter Klick oder gedrückt halten öffnet die Liste aller Arten mit Namen und Kürzel. Dieselben Einträge stehen im Menü „Erzeugen“.
   - Jede Art ist in den Einstellungen als Kürzel belegbar. Vorgabe wie Fusion: R Rechteck 2 Punkte, C Kreis Mitte; vorhandene Belegungen bleiben, sonst keine neuen Vorgabekürzel außer solchen, die Fusion hat.
   - Alle neuen Arten nutzen Felder, Fang, Ausrichtungslinien, Raster und Strg aus Teilprojekt 2. Getippte Werte werden zu Maßen. Eine Form ist ein Rückgängig-Schritt. Esc bricht die laufende Form ab. Namen und Hinweise in de und en, Namen wie in der deutschen und englischen Fusion.

## Nicht-Ziele (kommen in späteren Teilprojekten)

- Abhängigkeit „konzentrisch“ (Teilprojekt 5); bis dahin teilen sich zwei Bögen ihre Mitte über denselben oder deckungsgleiche Punkte.
- Durchmessermaß zur Mittellinie (Teilprojekt 5).
- Abrunden, Fase, Versatz (Teilprojekt 4).
- Spline mit Kontrollpunkten, Kegelschnitt (Teilprojekt 7).

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es läuft)

- Eine neue Form ist direkt überbestimmt oder meldet einen Widerspruch.
- Eine Form verliert beim Ändern eines Maßes ihre Gestalt (Polygon nicht mehr regelmäßig, Langloch nicht mehr tangential, Rechteck nicht mehr rechtwinklig).
- Der Löser nimmt von mehreren tangentialen Lösungen still eine andere, als die Vorschau zeigte.
- Alte Dateien ändern sich. Das Format steigt nur, wenn eine Skizze eine Mittellinie enthält; sonst schreibt das Programm die Datei wie bisher.
- Bestehende Werkzeuge verhalten sich anders als heute, solange man ihre Vorgabeart nutzt. Das Linienwerkzeug bleibt beim Klicken gleich; nur Ziehen auf dem letzten Punkt ist neu.
- Die Werkzeugleiste wird länger: Die Arten liegen in den Listen, nicht als eigene Symbole. Polygon und Langloch kommen als je ein Symbol dazu.

## Wie (Entscheidungen)

- Geometrie der neuen Arten rein und ohne egui, damit sie testbar ist: Umkreis aus 3 Punkten, Tangentialkreise, Polygonecken und Langloch-Umriss in `planora-sketch`; das Anlegen samt Abhängigkeiten als eine Operation je Form, sodass Vorschau und Ergebnis aus derselben Rechnung kommen.
- Die Art einer Form als Daten am Werkzeug, nicht als je eine neue Zweigkette durch alle `match`-Stellen. Die letzte Art je Form wird in der Sitzung gemerkt.
- Tangentialkreise analytisch für Linie, Kreis und Bogen, über alle Vorzeichen-Kombinationen. Gibt es keine Lösung, sagt die Statuszeile das und es entsteht nichts.
- Mittellinie als neuer Wert des Linienstils; Dateiformat 14 nur, wenn sie vorkommt (gleiche Regel wie Format 13 in Teilprojekt 1). Alle Stellen, die heute `construction` lesen (Profile, Stutzen, Export, Zeichnung), behandeln sie wie Konstruktion.
- Tests bei 1920×1080, nie 800×600.

## Was dem Planer überlassen bleibt

- Datenstrukturen, Reihenfolge der Aufgaben, Aufteilung der Dateien.
- Welche Abhängigkeiten eine Form im Einzelnen trägt, solange die Freiheitsgrade und die Anti-Ziele stimmen.
- Aussehen des Dreiecks und der Liste im Stil der vorhandenen Leiste.
