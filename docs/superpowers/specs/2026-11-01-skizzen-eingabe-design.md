# Skizzen 2: Eingabe beim Zeichnen, Fang, Raster, Auswahl

Stand 2026-11-01. Zweites von zehn Skizzen-Teilprojekten (Vergleich in der Vault-Notiz „2026-10-31 Vergleich Skizzen mit Fusion“). Dennis hat alle Einzelheiten an Claude übergeben („selber entscheiden“), mit der Vorgabe: **die Funktionen genauso wie in Fusion**. Alles, was hier steht, ist Claudes Entscheidung; wo das Verhalten von Fusion aus dem Gedächtnis beschrieben ist, gilt es als Annahme.

## Warum

Heute klickt man Punkte ungefähr an und vergibt die Maße danach einzeln. In Fusion zeichnet man maßhaltig: Beim Zeichnen stehen Felder neben dem Zeiger, die Zeichnung rastet an vorhandener Geometrie ein und die passenden Abhängigkeiten entstehen von selbst. Das ist der Unterschied, den man bei jeder einzelnen Linie spürt.

## Ziele

1. **Maße beim Zeichnen eintippen (wie Fusion).** Während eine Kurve aufgezogen wird, stehen neben ihr Eingabefelder, die dem Zeiger folgen und den aktuellen Wert zeigen:
   - Linie: Länge und Winkel. Rechteck: Breite und Höhe. Kreis: Durchmesser. Bogen: Radius und Öffnungswinkel. Ellipse: große und kleine Halbachse.
   - Tippen schreibt ins aktive Feld und hält diesen Wert fest; die Vorschau folgt dem Zeiger nur noch in der anderen Größe. Tab wechselt das Feld. Eingabe setzt den Punkt, Esc bricht ab.
   - Ein getippter Wert wird zu einem Maß in der Skizze (Abhängigkeit), wie in Fusion. Nicht getippte Werte erzeugen kein Maß.
   - Die Felder verstehen dasselbe wie die vorhandenen Formelfelder: Zahlen, Einheiten, Formeln, Parameter. Zoll funktioniert.
2. **Mehr Fang (wie Fusion).** Beim Überfahren rastet der Zeiger ein an: Endpunkt, Mittelpunkt einer Linie oder eines Bogens, Mitte von Kreis, Bogen und Ellipse, Schnittpunkt zweier Kurven, Punkt auf einer Kurve, Ursprung. Ein Zeichen am Zeiger zeigt die Art. Reihenfolge bei Gleichstand: Punkt vor Mittelpunkt vor Schnittpunkt vor Kurve.
3. **Ausrichtungslinien.** Zieht man nahe an die waagerechte oder senkrechte Verlängerung eines vorhandenen Punktes, erscheint eine gestrichelte Hilfslinie und der Zeiger rastet dort ein, auch an zwei zugleich. Es entsteht dadurch keine Abhängigkeit, nur die Lage.
4. **Mehr automatische Abhängigkeiten.** Beim Zeichnen entstehen von selbst, wie in Fusion:
   - deckungsgleich an Punkten, Punkt auf Kurve an Kurven, Mittelpunkt am Mittelpunkt einer Linie (heute nur die ersten beiden);
   - waagerecht und senkrecht (gibt es);
   - parallel und rechtwinklig zu einer Linie, deren Richtung die neue Linie fast trifft; das Zeichen erscheint während des Zeichnens an beiden Linien;
   - tangential, wenn eine Linie oder ein Bogen am Ende eines Bogens oder einer Linie in dessen Richtung losläuft;
   - gleich bleibt manuell.
   Gedrückte **Strg-Taste** schaltet Fang, Ausrichtungslinien und automatische Abhängigkeiten aus, solange sie gehalten wird (wie Fusion).
5. **Raster mit Fang.** Im Skizzenfeld die Schalter „Skizzenraster“ und „Fang“ (Fusion: Skizzenpalette), beide Vorgabe an. Das Raster folgt dem Zoom wie das heutige. Mit „Fang“ rastet ein freier Punkt am sichtbaren Raster ein; Geometrie-Fang geht immer vor. Die Einstellung gilt je Sitzung und wird in `config.json` gemerkt.
6. **Rahmenauswahl wie Fusion.** Mit dem Auswahlwerkzeug auf freier Fläche ziehen: von links nach rechts wählt, was ganz im Rahmen liegt (durchgezogener Rahmen); von rechts nach links wählt auch, was den Rahmen berührt (gestrichelter Rahmen). Umschalt ergänzt. Doppelklick auf eine Kurve wählt die ganze zusammenhängende Kette.
7. **Ganze Kurven ziehen.** Linie, Bogen, Kreis, Ellipse und Spline lassen sich an der Kurve selbst ziehen, nicht nur an ihren Punkten, soweit die Abhängigkeiten es erlauben. Kreis am Rand ziehen ändert den Radius, wie in Fusion. Mehrere gewählte Kurven ziehen sich gemeinsam.
8. **Bedienung:** Alles als ein Rückgängig-Schritt je Kurve. Texte in de und en. Neue Kürzel in den Einstellungen belegbar. „Blick auf“ (Fusion: Look At) als Knopf in der Skizzenpalette: Die Kamera schaut senkrecht auf die Skizze.

## Nicht-Ziele (kommen in späteren Teilprojekten)

- Neue Zeichenwerkzeuge und Tangentialbogen aus dem Linienwerkzeug (Teilprojekt 3).
- Abrunden, Versatz, Verschieben mit Dialog (Teilprojekt 4).
- Abhängigkeiten konzentrisch, kollinear, glatt (Teilprojekt 5).
- Ein- und Ausblenden von Maßen, Abhängigkeiten und Punkten; Slice (Teilprojekt 6 oder eigener Anzeigeblock).

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es läuft)

- Ungewollte Abhängigkeiten: Eine Linie bekommt still „parallel“ zu etwas, das der Nutzer nicht sah. Jede automatische Abhängigkeit hat vorher ein sichtbares Zeichen am Zeiger.
- Überbestimmte Skizzen durch die Automatik. Eine automatische Abhängigkeit, die überzählig wäre oder widerspräche, wird still weggelassen.
- Tippen löst Tastenkürzel aus (z. B. „l“ in einem Feld wählt das Linienwerkzeug).
- Träge Eingabe: Fang-Suche bei 200 Kurven unter 4 ms je Bild im Release-Build.
- Alte Dateien ändern sich. Dieses Teilprojekt ändert das Dateiformat nicht.

## Wie (Entscheidungen)

- Fang und Automatik liegen in `planora-sketch` oder `sketch_state.rs` ohne egui, damit sie testbar sind; die Ansicht zeichnet nur Zeichen und Felder. `Snap` wird um die neuen Arten erweitert. Schnittpunkte kommen aus `intersect.rs` (Teilprojekt 1), je Stand der Skizze zwischengespeichert.
- Die Felder nutzen `formula_field`, damit Formeln und Zoll gleich funktionieren wie in den Dialogen.
- Festgehaltene Werte werden in der Sitzung als „gesperrte Größe“ geführt; die Vorschau rechnet den Zeiger darauf um; beim Abschluss entstehen die Maße über die vorhandenen Wege (`set_dimension_input`).
- Tests bei 1920×1080, nie 800×600.

## Was dem Planer überlassen bleibt

- Datenstrukturen, Reihenfolge der Aufgaben, genaue Fangradien und Winkelgrenzen (Vorgabe: Fangradius wie heute, Richtungsgrenze etwa 3°).
- Aussehen der Zeichen am Zeiger, solange sie sich unterscheiden und selbst gezeichnet sind.
