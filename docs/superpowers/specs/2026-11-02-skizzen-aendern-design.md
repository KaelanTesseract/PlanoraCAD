# Skizzen 4: Abrunden, Fase, Versatz, Verschieben, Skalieren

Stand 2026-11-02. Viertes von zehn Skizzen-Teilprojekten (Vergleich in der Vault-Notiz „2026-10-31 Vergleich Skizzen mit Fusion“). Dennis hat alle Einzelheiten an Claude übergeben („selber entscheiden“), mit der Vorgabe: **die Funktionen genauso wie in Fusion**. Alles, was hier steht, ist Claudes Entscheidung; wo das Verhalten von Fusion aus dem Gedächtnis beschrieben ist, gilt es als Annahme und kommt in `offene-punkte.md`.

## Warum

Zeichnen geht jetzt wie in Fusion, Ändern noch nicht: Eine runde Ecke muss man von Hand aus Bogen und Abhängigkeiten bauen, eine Wandstärke Linie für Linie, und um Kurven zu verschieben oder zu kopieren, gibt es nur das Ziehen. Abrunden und Versatz gehören zu den meistbenutzten Skizzenbefehlen überhaupt.

## Ziele

1. **Abrunden** (Fusion: Skizzenrundung, Kürzel wie in Fusion, sonst belegbar).
   - Zwei Kurven anklicken, die sich treffen oder schneiden, oder einen gemeinsamen Punkt. Ein Feld am Zeiger zeigt den Radius, die Vorschau läuft mit. Die Kurven werden bis zum Bogen gekürzt.
   - Der Bogen ist tangential an beiden Kurven, trägt ein Radius-Maß und gleich bleibende Radien: Weitere Rundungen derselben Sitzung übernehmen den Radius und bekommen „gleich“ zur ersten, solange das Werkzeug läuft (wie Fusion).
   - Abhängigkeiten der gekürzten Kurven bleiben, wo sie noch gelten, wie beim Stutzen. Ein Maß auf der alten Ecke bleibt erhalten, indem an der Ecke ein Konstruktionspunkt über die Verlängerungen hält, wie in Fusion. **Annahme.**
   - Arten von Kurven: Linie, Bogen, Kreis, Ellipse und Spline, soweit eine Lösung existiert. Linie–Linie, Linie–Bogen, Linie–Kreis, Bogen–Bogen sind exakt; Ellipse und Spline dürfen numerisch sein.
2. **Fase** (Fusion: Skizzenfase) mit den drei Arten Gleicher Abstand, Abstand und Winkel, Zwei Abstände. Nur für Linien. Felder am Zeiger, Abhängigkeiten und Maße so, dass die Fase beim Ändern ihre Art behält.
3. **Versatz** (Fusion: Versatz, Kürzel **O**).
   - Kurve oder Kette auswählen, ziehen oder Abstand tippen. Mit „Kette“ an (Vorgabe) wählt ein Klick die ganze zusammenhängende Kette, wie in Fusion. Die Seite folgt dem Zeiger, ein negativer Wert wechselt sie.
   - Das Ergebnis ist geschlossen, wenn die Vorlage es ist. Überstehende und sich kreuzende Stücke werden gekürzt und entfallen, wie in Fusion. Kurven, die beim Versatz verschwinden, entfallen.
   - Abstand als Maß, und die Kopie folgt der Vorlage: Ändert sich die Vorlage, ändert sich die Kopie. Das geschieht über Abhängigkeiten aus dem vorhandenen Löser (parallel plus Abstand bei Linien, gleiche Mitte plus Radienabstand bei Bögen und Kreisen), nicht über eine neue Art von Verbindung im Dateiformat, wenn das geht.
   - Linie, Bogen, Kreis, Ellipse, Spline. Ellipse und Spline dürfen als Näherung entstehen und folgen dann nicht; das steht in `offene-punkte.md`.
4. **Verschieben und Kopieren** (Fusion: Verschieben/Kopieren, Kürzel **M**).
   - Auswahl von Kurven und Punkten. Griffe am Bezugspunkt: Ziehen verschiebt, ein Bogen-Griff dreht. Felder: X, Y, Winkel. Bezugspunkt versetzbar. Haken „Kopie erstellen“.
   - Beim Verschieben ohne Kopie bleiben innere Abhängigkeiten, nach außen bindende entfallen oder verhindern die Bewegung mit einer Meldung, wie in Fusion. **Annahme.**
   - Beim Kopieren nimmt die Kopie die inneren Abhängigkeiten und Maße mit; bestehende Spiegeln- und Muster-Regeln gelten sinngemäß.
5. **Skalieren** (Fusion: Skizzenmaßstab).
   - Auswahl oder ganze Skizze, Bezugspunkt anklicken, Faktor per Feld oder Griff. Maße werden mitskaliert, Winkel nicht. Faktor über 0.
   - Eine feste Geometrie, die nicht zur Auswahl gehört, kann das Skalieren verhindern; dann eine Meldung.
6. **Bedienung wie Fusion.** Rundung und Fase stehen als Liste an einem Symbol unter „Ändern“; Versatz, Verschieben/Kopieren, Skalieren haben je ein Symbol oder liegen in einer Liste, falls die Leiste sonst über 1080 Bildpunkte wächst. Alle im Menü „Erzeugen“ bzw. „Ändern“ und in den Einstellungen belegbar. Felder, Fang, Raster und Strg wie in den Teilprojekten 2 und 3. Ein Befehl ist ein Rückgängig-Schritt; Esc bricht ab und hinterlässt nichts. Texte in de und en, Namen wie in der deutschen und englischen Fusion.

## Nicht-Ziele (kommen in späteren Teilprojekten)

- Abhängigkeit „konzentrisch“ als eigene Art (Teilprojekt 5); bis dahin teilen sich Bögen die Mitte über denselben Punkt.
- Kreismuster und Rechteckmuster in zwei Richtungen (Teilprojekt 5).
- Abrunden und Fasen an Körpern, die gibt es schon.

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es läuft)

- Eine Rundung, Fase oder ein Versatz macht die Skizze überbestimmt oder meldet einen Widerspruch, obwohl die Geometrie eindeutig ist.
- Abrunden oder Versatz springt beim Ändern eines Maßes auf die falsche Seite oder klappt eine Kurve um. Die Vorlage bleibt unverändert, solange der Befehl sie nicht ausdrücklich kürzt.
- Vorschau und Ergebnis weichen voneinander ab.
- Ein Versatz einer geschlossenen Kette ergibt ein offenes oder sich selbst kreuzendes Profil, ohne dass es gemeldet wird.
- Alte Dateien ändern sich. Das Format steigt nur, wenn es wirklich nötig ist, und dann nach der Regel aus Teilprojekt 1 und 3.
- Bestehendes Verhalten von Stutzen, Dehnen, Brechen, Ziehen, Spiegeln und Muster ändert sich.
- Träge Bedienung: Vorschau beim Versatz einer Kette aus 200 Kurven unter 16 ms je Bild im Release-Build.

## Wie (Entscheidungen)

- Geometrie rein und ohne egui in `planora-sketch`, Anlegen samt Abhängigkeiten als eine atomare Operation, Vorschau und Klick aus derselben Rechnung, wie bei `shapes.rs`.
- Die Oberfläche nutzt die Stufen und Felder aus `sketch_kinds.rs`, wo es passt. Verschieben und Skalieren brauchen Griffe; der Griff ist eine eigene kleine Einheit im Stil der vorhandenen Ansicht.
- Der Löser hat aus Teilprojekt 3 eine Tangente am gemeinsamen Ende mit nur einer Lösung; Rundungen nutzen sie.
- Kürzen läuft über die Funktionen aus `edit.rs` (Teilprojekt 1), damit Abhängigkeiten gleich behandelt werden wie beim Stutzen.
- Tests bei 1920×1080, nie 800×600. Keine neuen Fremd-Abhängigkeiten.

## Was dem Planer überlassen bleibt

- Datenstrukturen, Reihenfolge der Aufgaben, Aufteilung der Dateien.
- Welche Abhängigkeiten eine Rundung, Fase und ein Versatz im Einzelnen tragen, solange die Anti-Ziele gelten. Die Freiheitsgrade werden mit dem heutigen Löser gemessen, nicht geschätzt.
- Aussehen der Griffe im Stil der vorhandenen Ansicht.
