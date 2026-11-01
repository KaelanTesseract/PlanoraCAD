# Skizzen 1: Profile wie Fusion, Stutzen, Dehnen, Brechen

Stand 2026-10-31. Erstes von zehn Skizzen-Teilprojekten (Vergleich in der Vault-Notiz „2026-10-31 Vergleich Skizzen mit Fusion“). Dennis hat alle Einzelheiten an Claude übergeben („selber entscheiden“), mit der Vorgabe: **die Funktionen genauso wie in Fusion**. Alles, was hier als Entscheidung steht, ist Claudes Entscheidung; Maßstab ist das Verhalten von Fusion.

## Warum

Heute muss jedes Profil eine einfache geschlossene Kette sein. Kreuzende Linien, T-Verzweigungen, zwei überlappende Kreise oder eine Linie quer durch ein Rechteck ergeben eine Fehlermeldung statt Flächen. In Fusion ist genau das der Normalfall: Man zeichnet grob, Fusion erkennt jede eingeschlossene Fläche, und mit Stutzen räumt man auf. Ohne das fühlt sich jedes weitere Skizzen-Teilprojekt falsch an; deshalb kommt es zuerst.

## Ziele

1. **Profile wie Fusion.** Jede Fläche, die von nicht-konstruktiven Kurven vollständig umschlossen ist, ist ein eigenes Profil:
   - Kurven schneiden sich gegenseitig auf; Kreuzungen, T-Verzweigungen, überlappende Kreise, Selbstschnitte eines Splines und eine Linie quer durch eine Fläche teilen Flächen.
   - Kurvenstücke, die keine Fläche begrenzen (Stummel, frei liegende Linien), werden ignoriert, nicht gemeldet.
   - Eine Fläche innerhalb einer anderen (Kreis im Rechteck) ergibt zwei Profile: den Ring (Rechteck mit Loch) und die Kreisfläche. Beide sind einzeln wählbar, wie in Fusion.
   - Konstruktionsgeometrie begrenzt keine Profile.
   - Berühren sich Kurven nur tangential in einem Punkt, teilen sie dort; zwei Flächen, die sich nur in einem Punkt berühren, sind zwei Profile.
2. **Profile in der Skizze sichtbar.** Im Skizzenmodus sind Profile hell eingefärbt wie in Fusion. Schalter „Profil anzeigen“ im Skizzenfeld (Fusion: Skizzenpalette → „Profil“), Vorgabe an.
3. **Profile durch Anklicken wählen.** Extrudieren und Drehen wählen Profile durch Klick in die Ansicht (3D und Skizze), mehrere mit Klick (Fusion: Klick schaltet um, kein Umschalt nötig). Die Liste im Dialog bleibt als zweiter Weg. Mehrere gewählte Profile:
   - Nebeneinanderliegende Profile (gemeinsame Kante) werden zu einem Körper zusammengefasst, wie in Fusion.
   - Getrennte Profile ergeben mehrere Körper in einem Feature.
   - Drehen behält seine heutigen Grenzen (360°, Achse als Linie der Skizze, Profil auf einer Seite), arbeitet aber auf den neuen Profilen; mehrere Profile beim Drehen nur, wenn jedes für sich die Regeln erfüllt.
4. **Profile bleiben gefunden.** Ein Feature merkt sich seine Profile nicht mehr als laufende Nummer, sondern über die begrenzenden Kurvenstücke. Nach einer Änderung der Skizze wird das passende Profil wiedergefunden, solange seine Begrenzung im Wesentlichen bleibt. Geht ein Profil verloren, meldet das Feature einen Fehler mit dem Text in der Art von Fusion („Ein Profil der Skizze ist nicht mehr vorhanden …“) und lässt sich über Bearbeiten neu wählen.
5. **Stutzen** (Fusion: Stutzen, Kürzel **T**):
   - Beim Überfahren wird das Kurvenstück zwischen den nächsten Schnittpunkten rot hervorgehoben; Klick löscht es. Hat die Kurve keinen Schnittpunkt, wird sie ganz gelöscht.
   - Gedrückt halten und über Kurven ziehen stutzt alle überfahrenen Stücke (Fusion: Stutzen durch Ziehen).
   - Schnittpunkte zählen mit allen Kurven der Skizze, auch Konstruktion und kopierten Kanten.
   - Gestutzt werden Linien, Kreise (werden Bögen), Bögen, Ellipsen (werden Ellipsenbögen), Ellipsenbögen und Splines.
   - An einem neuen Ende entsteht eine Abhängigkeit „Punkt auf Kurve“ zur schneidenden Kurve (bei einem Schnitt an deren Endpunkt „deckungsgleich“), wie Fusion es macht.
   - Abhängigkeiten und Maße der übrig bleibenden Stücke bleiben, wo sie noch Sinn ergeben (z. B. „waagerecht“ an einer gekürzten Linie, „Radius“ an einem Bogen aus einem Kreis); solche an gelöschten Stücken verschwinden.
6. **Dehnen** (Fusion: Dehnen): Klick nahe einem offenen Ende einer Linie, eines Bogens oder Ellipsenbogens verlängert sie bis zum nächsten Schnitt mit einer anderen Kurve; Vorschau beim Überfahren. Gibt es keinen Schnitt, passiert nichts (Hinweis in der Statuszeile). Das verlängerte Ende bekommt die Abhängigkeit wie beim Stutzen.
7. **Brechen** (Fusion: Brechen): Klick auf ein Kurvenstück teilt die Kurve an den beiden Schnittpunkten, die dieses Stück begrenzen; die Teile hängen deckungsgleich zusammen. Ein Kreis wird zu Bögen, eine Ellipse zu Ellipsenbögen.
8. **Bedienung wie Fusion:** Die drei Werkzeuge stehen im Skizzen-Werkzeugfeld unter „Ändern“. Stutzen hat das Kürzel T, Dehnen und Brechen kein Vorgabekürzel; alle drei sind in den Einstellungen belegbar (Regel: alle Fusion-Kürzel in der App). Esc beendet das Werkzeug. Jede Aktion ist ein Schritt für Rückgängig.
9. **Neue Kurvenart Ellipsenbogen**, weil Stutzen und Brechen sie wie in Fusion erzeugen: Mittelpunkt, große Halbachse, kleine Halbachse, Start- und Endpunkt auf der Ellipse (gegen den Uhrzeigersinn). Er kann alles, was eine Ellipse kann: Maße der Halbachsen und des Winkels, Punkt auf Kurve, tangential, deckungsgleiche Enden, Konstruktion, Profile, Extrudieren, STEP-Export.

## Nicht-Ziele (kommen in späteren Teilprojekten)

- Abrunden, Fase, Versatz, Verschieben, Skalieren (Teilprojekt 4).
- Neue Zeichenwerkzeuge, Fang an Schnittpunkten beim Zeichnen (Teilprojekte 2 und 3).
- Splines dehnen; Splines mit Kontrollpunkten (Teilprojekt 7).
- Drehen mit Winkel oder freier Achse (gehört nicht zu den Skizzen).
- Projizierte Kanten, die dem Körper folgen (Teilprojekt 6).

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es läuft)

- **Bestehende Modelle ändern sich still.** Eine alte Datei muss nach dem Öffnen dieselben Körper ergeben. Alte Profilnummern werden beim Laden in die neuen Verweise umgerechnet, und zwar so, dass genau das alte Profil (äußere Schleife mit ihren Löchern) gewählt ist.
- Stutzen löscht mehr als das hervorgehobene Stück oder verschiebt übrig bleibende Geometrie.
- Ein Profil „springt“ nach einer kleinen Änderung still auf eine andere Fläche. Lieber Fehler „Profil nicht mehr vorhanden“ als falscher Körper.
- Die Skizze wird spürbar träge: Profilerkennung für 200 Kurven unter 50 ms im Release-Build, für die Vorschau beim Überfahren beim Stutzen unter 16 ms.

## Wie (Entscheidungen)

- **Profilerkennung als ebene Anordnung** in `planora-sketch`: alle Schnittpunkte aller nicht-konstruktiven Kurven bestimmen (analytisch für Linie, Kreis, Bogen; numerisch über die vorhandene Abtastung mit Newton-Nachschärfen für Ellipse, Ellipsenbogen, Spline), Kurven dort in Stücke teilen, Knoten im Abstand `tol.linear` zusammenlegen, Stummel wegschneiden (Grad 1 wiederholt entfernen), an jedem Knoten die Stücke nach Richtung ordnen (bei gleicher Richtung nach Krümmung), Flächen umlaufen, die unbeschränkte Außenfläche je Zusammenhangskomponente verwerfen, verschachtelte Komponenten als Löcher der umgebenden Fläche eintragen. Die Teilkurven werden zu Kernel-Segmenten: Linie, Bogen, NURBS-Teilstück (neu in `planora-math`: Teilkurve einer NURBS zwischen zwei Parametern).
- `Sketch::profiles()` liefert danach die neuen Profile; jedes Profil trägt seinen Verweis (siehe nächster Punkt). Die bisherigen Ablehnungsgründe entfallen für Kreuzungen und Verzweigungen; Entartungen (Bogen ohne Winkel, Spline mit doppelten Punkten) werden weiter gemeldet.
- **Profil-Verweis:** Liste der begrenzenden Stücke als (Kurven-Id, Stück-Bereich auf der Kurve als Parameterpaar, Richtung). Wiederfinden: das Profil mit der größten Übereinstimmung der Stücke (gleiche Kurve, überlappender Bereich, gleiche Seite); bei Gleichstand oder weniger als der Hälfte Übereinstimmung gilt es als verloren. Das genaue Maß legt der Plan fest.
- **Features:** `Extrude` und `Revolve` speichern eine Liste von Profil-Verweisen statt `profile: usize`. Dateiformat 13; nur geschrieben, wenn ein Feature die neue Form braucht, sonst bleibt die Datei auf ihrem Format. Beim Laden älterer Dateien rechnet ein Übergang die Nummer mit der alten Reihenfolge um (alter Algorithmus bleibt dafür als interne Funktion erhalten).
- **Stutzen/Dehnen/Brechen** als Operationen in `planora-sketch` (eine Funktion je Werkzeug, Eingabe: Kurve und Klickpunkt), die Ansicht nur für Hervorheben und Klick. Splines werden beim Stutzen neu durch die verbleibenden Stützpunkte plus den neuen Endpunkt gelegt; die Form ändert sich dabei leicht. (Claudes Entscheidung; Fusion behält die Form genauer. Der Plan prüft, ob ein Spline mit Parameterbereich ohne großen Umbau geht, und nimmt dann den.)
- **Ellipsenbogen** als neue `CurveKind` mit internen Abhängigkeiten „Start und Ende auf der Ellipse“, wie `ArcRadius` beim Bogen.
- Texte über `tr!` in de.ftl und en.ftl; Werkzeugnamen wie in der deutschen und englischen Fusion („Stutzen/Trim“, „Dehnen/Extend“, „Brechen/Break“, „Profil/Profile“).
- Tests bei 1920×1080, nie 800×600.

## Was dem Planer überlassen bleibt

- Datenstruktur der Anordnung, Wahl der Toleranzen für Schnittpunkte, Zwischenspeicher der Profile (es gibt schon einen in `state.rs`).
- Wie der Dialog die Klickauswahl anzeigt, solange Liste und Klick dasselbe Ergebnis haben.
- Welche Abhängigkeiten beim Stutzen erhalten bleiben, im Rahmen von Ziel 5.
- Reihenfolge der Aufgaben.
