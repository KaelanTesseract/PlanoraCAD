# Skizzen 5: Muster, weitere Abhängigkeiten und Maße

Stand 2026-11-03. Fünftes von zehn Skizzen-Teilprojekten (Vergleich in der Vault-Notiz „2026-10-31 Vergleich Skizzen mit Fusion“). Dennis hat alle Einzelheiten an Claude übergeben („selber entscheiden“), mit der Vorgabe: **die Funktionen genauso wie in Fusion**. Alles, was hier steht, ist Claudes Entscheidung; wo das Verhalten von Fusion aus dem Gedächtnis beschrieben ist, gilt es als Annahme und kommt in `offene-punkte.md`.

## Warum

Bohrbilder und Reihen zeichnet man in Fusion mit einem Muster, das sich danach noch ändern lässt. Heute gibt es nur ein lineares Muster in eine Richtung. Auch fehlen drei Abhängigkeiten, die man ständig braucht (konzentrisch, kollinear, glatt), und bei den Maßen kann man nicht waagerecht oder senkrecht messen, nur den direkten Abstand. Ein Durchmesser zur Mittellinie ist die Grundlage jeder Drehteil-Skizze.

## Ziele

1. **Rechteckmuster** (Fusion: Rechteckige Anordnung).
   - Auswahl, dann zwei Richtungen. Jede Richtung hat Anzahl und Abstand; Art „Abstand“ oder „Gesamtlänge“. Symmetrisch je Richtung schaltbar. Die Richtung folgt einer gewählten Linie oder den Achsen, Vorgabe die Achsen der Skizze.
   - Griffe an den Richtungspfeilen zum Ziehen, Felder für die Werte, Vorschau läuft mit.
   - Einzelne Exemplare lassen sich in der Vorschau per Klick abwählen, wie in Fusion.
   - Das Muster bleibt änderbar: Anzahl und Abstand lassen sich danach bearbeiten, und ändert man die Vorlage, folgen die Exemplare.
2. **Kreismuster** (Fusion: Kreisförmige Anordnung).
   - Auswahl, Mittelpunkt anklicken, Anzahl, Winkel (Vorgabe 360°), Art „Voll“, „Winkel“, „Symmetrisch“. Griff am Winkel, Vorschau, Abwählen wie oben, änderbar wie oben.
3. **Konzentrisch** (Fusion: Konzentrisch): zwei Kreise, Bögen oder Ellipsen haben dieselbe Mitte. Linien gehen nicht.
4. **Kollinear** (Fusion: Kollinear): zwei Linien liegen auf derselben Geraden, auch mit Abstand zwischen ihnen.
5. **Glatt** (Fusion: Glatt, G2): stetige Krümmung, wo ein Spline an einen Bogen, eine Linie oder einen anderen Spline stößt. Wo das nach dem Spline-Stand noch nicht ganz geht, sagt die Meldung das; was fehlt, kommt mit Teilprojekt 7.
6. **Maße waagerecht und senkrecht.** Das Bemaßen-Werkzeug zeigt wie in Fusion je nach Zeigerlage das direkte, das waagerechte oder das senkrechte Maß zwischen zwei Punkten. Auch für Punkt und Linie, Linie und Linie (parallel), Mitte von Kreisen.
7. **Durchmessermaß zur Mittellinie.** Bemaßt man eine Kurve oder einen Punkt gegen eine Mittellinie, entsteht ein Durchmessermaß: der doppelte Abstand, mit Ø im Text. Ein Umschalter wählt zwischen Radius und Durchmesser, wie in Fusion.
8. **Mitgenommen aus den Teilprojekten davor** (stehen auf der Liste, Dennis hat sie gesehen):
   - Beim Langloch „gesamt“ bleibt die Gesamtlänge als eigenes Maß, falls sich das mit den neuen Maßen sauber lösen lässt; sonst Begründung in `offene-punkte.md`.
   - Doppelklick auf ein Maß in der Zeichnung öffnet sein Feld, wie die Liste.
   - Fällt der Rasterfang auf einen vorhandenen Punkt, bindet die neue Kurve an ihn, statt einen zweiten Punkt an dieselbe Stelle zu legen.
9. **Bedienung wie Fusion.** Einträge in Leiste, Skizzenfeld und Menüs, alle in den Einstellungen belegbar. Felder, Fang und Strg wie zuvor. Ein Befehl ist ein Rückgängig-Schritt; Esc bricht ab und hinterlässt nichts. Texte in de und en, Namen wie in der deutschen und englischen Fusion.

## Nicht-Ziele (kommen in späteren Teilprojekten)

- Muster von Körpern, das ist kein Skizzenbefehl.
- Griffe für Spline-Tangenten und Kontrollpunkte (Teilprojekt 7).
- Maße im Raum (Teilprojekt 10).

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es läuft)

- Ein Muster macht die Skizze überbestimmt oder meldet einen Widerspruch, obwohl die Geometrie eindeutig ist.
- Beim Ändern von Anzahl oder Abstand gehen Maße und Abhängigkeiten verloren, oder Exemplare bleiben an der alten Stelle.
- Ein Muster mit vielen Exemplaren macht die Skizze spürbar träge: 10×10 Exemplare eines Rechtecks ändern unter 100 ms im Release-Build; darüber wird es Geometrie mit Meldung, wie beim Versatz.
- Waagerechte oder senkrechte Maße springen auf die andere Seite oder klappen die Form um.
- Eine neue Abhängigkeit, die schon erfüllt und überzählig wäre, macht die Skizze widersprüchlich, statt es zu melden.
- Alte Dateien ändern sich. Das Format steigt nur, wenn es wirklich nötig ist, und dann nach der Regel aus Teilprojekt 1 und 3.
- Bestehendes lineares Muster und Spiegeln verlieren etwas. Alte Dateien damit laufen weiter.

## Wie (Entscheidungen)

- Geometrie und Anlegen rein und ohne egui in `planora-sketch`, Vorschau und Klick aus derselben Rechnung, wie in den Teilprojekten 3 und 4.
- Griffe nutzen die Einheit aus Teilprojekt 4 (`sketch_grips.rs`), Stufen und Felder `sketch_kinds.rs`.
- Muster sind änderbar, also braucht das Modell eine Kenntnis davon. Der Planer misst, ob das über Abhängigkeiten geht oder eine eigene Art im Dateiformat braucht. Neue Abhängigkeiten und Maßarten kommen ins Format nach der Regel „nur wenn nötig“.
- Gemessen wird über dichte Wertefolgen, nicht Stichproben, mit Prüfung auf umgeklappte Formen und Seitenwechsel.
- Tests bei 1920×1080, nie 800×600. Keine neuen Fremd-Abhängigkeiten.

## Was dem Planer überlassen bleibt

- Datenstrukturen, Reihenfolge der Aufgaben, Aufteilung der Dateien.
- Welche Abhängigkeiten ein Muster im Einzelnen trägt, gemessen mit dem heutigen Löser.
- Wie das Abwählen einzelner Exemplare gespeichert wird.
