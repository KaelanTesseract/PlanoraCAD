# Skizzen 11a: Mittelpunktlinie, Übergangskurve, Krümmungskamm, Skizzenprüfung, verlorene Projektionen, schneller Löser und Nacharbeiten

Stand 2026-11-15. Erster Teil von „Skizzen 11: Reste“. Dennis hat Skizzen 11 in zwei Teilprojekte geteilt („passt“): **11a „Zeichnen und Prüfen“** (dieses) und **11b „Übernehmen und Ordnen“** (Kopieren/Einfügen, Skizze als DXF, Skizzengruppen, Text-Reste, Drehprofil, Isoparametrische Kurve). Maßstab ist Fusions Verhalten („mach es wie in Fusion“; bei den Skizzen entscheidet Claude). Was die Fusion-Hilfe nicht belegt, ist als **Annahme** markiert und kommt in `offene-punkte.md`.

## Warum

Nach Skizzen 1–10 fehlen beim täglichen Zeichnen noch Fusion-Werkzeuge: die Mittelpunktlinie, die Übergangskurve zwischen zwei Kurvenenden, der Krümmungskamm und vor allem die Skizzenprüfung („Unterbestimmte finden“), mit der man eine Skizze voll bestimmt. Dazu kommen Abweichungen aus früheren Teilprojekten, die beim Arbeiten stören: Löscht man einen Körper, verschwindet eine Skizze mit Projektionen gleich mit (Fusion warnt nur); lange 3D-Ketten ziehen sich zäh; einige Zeichenwerkzeuge verhalten sich anders als in Fusion.

**Gelungen ist es, wenn** man eine Linie von der Mitte aus zeichnet, zwei Kurvenenden mit einer G1- oder G2-Übergangskurve verbindet, an jeder gekrümmten Kurve den Kamm einblendet, im Skizzenfeld „Status: unterbestimmt“ sieht und mit „Unterbestimmte finden“ Element für Element bestimmt, bis das rote Schloss im Baum erscheint. Löscht man die Quelle einer Projektion, bleibt die Skizze mit Warnung stehen und lässt sich neu verknüpfen. Eine 3D-Kette aus 200 Kurven zieht flüssig.

**Fehlschlag wäre:**
- 2D-Skizzen lösen anders als vorher (andere Lage, anderer Freiheitsgrad, andere überzählige Abhängigkeiten) oder langsamer.
- Alte Dateien werden nicht mehr bytegleich geschrieben.
- Löschen eines Körpers lässt eine Skizze in einem Zustand, den die Datei nicht laden kann.

## Was Fusion tut

Quellen (Fusion-Hilfe, per curl gelesen): „Create midpoint lines“ (SKT-CREATE-MIDPOINT-LINES), „Lines in sketches“, „Create a Blend Curve“ (SKT-BLEND-CURVE), „Sketch modification tools“, „Analyze curvature of edges“, „Curvature Comb Analysis reference“, „Create splines“, „Sketch Palette reference“, „Find underconstrained sketch geometry“ (SKT-FIND-UNDERCONSTRAINED), „Fully define and constrain sketches“, „Create circles“, „Create polygons“, „Create arcs“, „Create lines“, „Project geometry onto a sketch plane“, „Project reference“, „Fusion keyboard shortcuts reference“.

- **Mittelpunktlinie** (Erzeugen → Mittelpunktlinie): Der erste Klick setzt die Mitte; dann Länge und Winkel tippen oder mit einem Klick das Ende setzen; die Linie liegt symmetrisch zur Mitte. Enter beendet das Werkzeug. Bei aktivem Linienwerkzeug schaltet das Skizzenfeld unter „Optionen des Features“ zwischen Linie und Mittelpunktlinie um. Kein eigenes Kürzel.
- **Übergangskurve** (Ändern → Übergangskurve): Art im Skizzenfeld „Tangential“ (G1) oder „Krümmung“ (G2). Erste Kurve oder Kante wählen, dann die zweite; welches Ende verbunden wird, entscheidet die Nähe des Zeigers. Überfahren hebt gültige Ziele hervor und zeigt eine Vorschau; ein zweiter Klick auf eine gewählte Kurve hebt die Wahl auf. An den Anschlüssen entstehen „tangential“ bzw. „Krümmung“ (G2). Auch Körperkanten sind als Quelle erlaubt.
- **Krümmungskamm** in Skizzen: Kurve wählen, Rechtsklick → „Krümmungsanzeige ein/aus“ (en „Toggle Curvature Display“); es öffnet ein Dialog „Krümmungsanzeige einrichten“ mit **Dichte** und **Maßstab**, dann OK. Derselbe Befehl schaltet wieder aus. Bei gewähltem Spline gibt es im Skizzenfeld unter den Optionen den Schalter „Krümmungskamm“. (Inspect → Krümmungskamm-Analyse für Körperkanten mit Ordner „Analyse“ gehört zu „Prüfen und Material“, nicht hierher.)
- **Skizzenstatus:** Oben im Skizzenfeld steht „Status: Unterbestimmt“ oder „Status: Bestimmt“, live. Unbestimmte Geometrie ist blau, voll bestimmte schwarz. Eine voll bestimmte Skizze zeigt im Browser ein rotes Schloss am Symbol.
- **Unterbestimmte finden** (Knopf im Skizzenfeld): Liste aller unterbestimmten Elemente im Skizzenfeld. Ein Klick auf eine Zeile hebt das Element hervor und zoomt hin (Einzelwahl, kein „Weiter“). Ein X je Zeile löscht das Element, außer bei Skizzenpunkten. Voll bestimmte Geometrie lässt sich ausblenden. Der Befehl bleibt aktiv, während man Abhängigkeiten oder Maße setzt oder löscht; Liste und Status laufen mit.
- **Skizze berechnen** (Skizzenfeld): rechnet die Bestimmtheit der ganzen Skizze neu.
- **Verlorene Projektion:** Geht die Verknüpfung verloren, erscheint eine gelbe Warnung am Skizzen-Feature in der Zeitleiste; die Geometrie bleibt stehen und wird markiert. Rechtsklick in der Zeitleiste → „Verlorene Projektionen verwalten“ bietet je Element „Neu verknüpfen“ (neue Quelle wählen), „Verknüpfung aufheben“ (bleibt an Ort und Stelle, unverknüpft) und „Geometrie löschen“, dazu „Alle löschen“, „Alle Verknüpfungen aufheben“ und „Übrige Geometrie abblenden“.
- **Kreis:** „Mittelpunkt-Durchmesser“ und „2 Punkte“ tippen den Durchmesser, „2 Tangenten“ den Radius; „3 Punkte“ und „3 Tangenten“ haben kein Feld.
- **Polygon:** Umschrieben/Eingeschrieben: Mittelpunkt, Radiusfeld, Tab zur Seitenzahl, mit der Maus ausrichten, Klick. Kante: Länge, Tab zum Winkel, dann Seitenzahl; die Maus wählt die Seite.
- **Bogen aus der Linie:** Die Maus auf die Gegenseite bewegen kehrt die Bogenrichtung um.

## Annahmen (nicht in der Hilfe belegt)

- **A1 Mittelpunktlinie:** Die getippte Länge ist die ganze Linie. Es entsteht eine Linie und ein freier Punkt in ihrer Mitte mit der Abhängigkeit „Mittelpunkt“ (der Punkt ist kein Konstruktionspunkt, sondern ein gewöhnlicher Skizzenpunkt). Fängt der erste Klick einen vorhandenen Punkt, wird dieser Punkt die Mitte (keine neue Mitte, kein „deckungsgleich“). Auf den Endpunkten wirkt der Fang wie bei der Linie (automatische Abhängigkeiten). Es gibt keine Kette: Nach dem zweiten Klick beginnt die nächste Mittelpunktlinie neu. Ein getipptes Maß wird zum Längenmaß der Linie, ein getippter Winkel wie bei der Linie (Zeigerregel waagerecht/senkrecht). Enter, Esc und Rechtsklick beenden. Ein Kürzel ist belegbar, aber nicht vorbelegt.
- **A2 Übergangskurve:** Das Ergebnis ist ein Kontrollpunkt-Spline (Skizzen 7): bei G1 Grad 3 mit 4 Kontrollpunkten, bei G2 Grad 5 mit 6 Kontrollpunkten. Erster und letzter Kontrollpunkt **sind** die Endpunkte der Quellkurven (gemeinsame `PointId`, kein „deckungsgleich“). An beiden Enden: bei G2 „glatt“ (`Smooth`), bei G1 die **neue Abhängigkeit „Tangente am Ende“** (`EndTangent(Kurve, Punkt, Kurve, Punkt)`: Richtungen beider Kurven am gemeinsamen Punkt gleich oder entgegengesetzt, ohne Krümmungszeile), weil `Tangent` mit Splines an beliebiger Stelle berührt und `Smooth` an Splines G2 erzwingt (Bauprüfung). `EndTangent` gilt für alle offenen Kurven (Linie, Bogen, Ellipsenbogen, beide Splinearten, Kegelschnitt), in 2D und 3D, im Browser als „tangential“ angezeigt. `Smooth` wird für den Ellipsenbogen erweitert (`smooth_end` liefert heute `None`), damit G2 auch dort geht. Format 21, sobald ein `EndTangent` oder ein `Smooth` am Ellipsenbogen in der Datei steht. Die Anfangslage der inneren Kontrollpunkte folgt den Tangenten (und Krümmungen) der Enden mit einem Drittel des Endabstands als Hebel. Standardart: **G2** (Fusion-Erinnerung: „Krümmung“ ist vorbelegt; nicht belegt). Quellen: Linie, Bogen, Ellipsenbogen, beide Splinearten, Kegelschnitt (offene Kurven mit Enden). Körperkanten als Quelle kommen mit „Anzeige in der Skizze“, weil Kanten heute in der Skizzenansicht nicht anklickbar sind; bis dahin gelten projizierte Kanten in der Skizze (sie sind Skizzenkurven). Beide Enden müssen verschieden sein; ein Spline an zwei Enden derselben Kurve ist erlaubt. 3D-Kurven (Skizzen 10) sind keine Quelle (wie A5 dort), Meldung am Zeiger.
- **A3 Krümmungskamm:** gilt für Bogen, Kreis, Ellipse, Ellipsenbogen, beide Splinearten und Kegelschnitt; Linien haben keinen Kamm (der Befehl wird bei ihnen gedämpft). Standard Dichte 20 Zähne je Kurve (Bereich 2–100, wie bei der Kantenanalyse), Maßstab 1,0 (Bereich 0,01–100); der Maßstab 1 bildet die größte Krümmung der Kurve auf 15 % ihrer Länge ab (auf dem Bildschirm mindestens 10 px). Zähne zeigen auf die Seite des Krümmungsmittelpunkts nach außen (wie Fusion: Kamm auf der konvexen Seite), die Hüllkurve verbindet die Spitzen. Ein- und ausschalten ist ein Rückgängig-Schritt und steht in der Datei (je Kurve `kamm: {dichte, massstab}`). Der Kamm erscheint auch außerhalb des Skizzenmodus, solange die Skizze sichtbar ist. 3D-Kurven bekommen den Kamm in ihrer Ebene (planare Kurven mit Normale) bzw. die Zähne entlang der Hauptnormale.
- **A4 Status und Liste:** „unterbestimmt“ heißt: Der Löser meldet für das Element freie Richtungen (`free_points`/`free_curves` aus `SolveReport`). Gezählt und gelistet werden Kurven und alleinstehende Punkte (Punkte, die nur Kurvenenden sind, zählen über ihre Kurve, wie in Fusion die Linie samt Enden). Konstruktions- und projizierte Geometrie zählt mit (projizierte ist fest und also nie unterbestimmt); Text zählt als ein Element über seine Lagepunkte. Die Zeile nennt Art und laufende Nummer wie der Browser („Linie 3“, „Punkt 7“). Ein Widerspruch in der Skizze ergibt „Status: Widerspruch“ (rote Schrift), eine Gruppe, die nicht konvergiert, „Status: nicht gelöst“ (orange); in beiden Fällen kein Schloss, und die Liste zeigt nur die freien Elemente der gelösten Gruppen. „Bestimmt“ und Schloss gibt es nur, wenn alle Gruppen gelöst sind und nichts frei ist. Das rote Schloss erscheint im Baum am Skizzen-Eintrag und an der Zeitleiste nicht. „Skizze berechnen“ leert den Zwischenspeicher des Lösers und löst die Skizze komplett neu (ein Rückgängig-Schritt nur, wenn sich Werte ändern).
- **A5 Verlorene Projektion:** Projektionen, Schnitte, Silhouetten, eingezogene 3D-Geometrie, „Auf Fläche projizieren“ und Schnittkurven (alle Einträge in `copies`) sind **schwache** Bezüge. Löschen ihrer Quelle löscht die Skizze nicht mehr mit; der Löschen-Dialog nennt die Skizze nicht als abhängig. Die Skizzenebene (auf Fläche, auf Konstruktionsebene) bleibt ein harter Bezug wie heute. „Neu verknüpfen“ nimmt in der Skizze eine neue Quelle desselben Typs (Kante für Kante, Fläche für Fläche, Körper für Körper); die Kurve wird aus der neuen Quelle neu berechnet, ihre Abhängigkeiten bleiben, soweit die Stückzahl gleich bleibt. „Übrige Geometrie abblenden“ dämpft alles außer den verlorenen Kurven, solange der Dialog offen ist. Der Dialog ist abdockbar (`dock_key` „lost-projections“).
- **A6 Bogen „Mittelpunkt“ in 2D:** folgt der Zugrichtung wie die 3D-Fassung aus Skizzen 10: Der Drehsinn ergibt sich aus der Seite, auf die der Zeiger nach dem ersten Endpunkt zuerst wandert; Überstreichen über 180° hinaus läuft weiter (bis knapp 360°). Ein Rückwärtsziehen über den Start kehrt den Sinn um.
- **A7 Winkelmaß nach Zeigerseite:** Beim Bemaßen zweier Linien wählt die Lage des Zeigers zwischen Winkel und Gegenwinkel (180° − α) in den vier Feldern zwischen den Linien. Dafür bekommt `Angle` einen Zusatz „Gegenwinkel“; alte Maße ohne Zusatz bleiben bytegleich, ein neuer Gegenwinkel schreibt Format 21. Das ändert die Entscheidung „Winkel immer in (0, π)“ aus Skizzen 5.
- **A8 Felder in 3D:** Kreis „2 Punkte“, „Mittelpunkt-Durchmesser“, Polygon (alle Arten) und Rechteck bekommen in der 3D-Skizze dieselben Felder am Zeiger wie in 2D, in der Triadenebene; die getippten Werte werden zu denselben Maßen wie in 2D. Das Polygon nimmt die Seitenzahl dort per Tab.
- **A9 Felder am Zeiger:** Sie folgen dem Zeiger und nehmen wie in Fusion keinen Mausklick an; Tab wechselt. Die Nacharbeit aus Skizzen 5 entfällt damit als „wie Fusion“ und wird in `offene-punkte.md` geschlossen.

## Bauweise

### Löser für lange Ketten (Dennis' Wahl: Claude, Weg A)

Heute: je Gruppe eine dichte Jacobi-Matrix, Normalgleichungen `JᵀJ` dicht, Cholesky O(n³), Rangprüfung mit Gram-Schmidt über alle Zeilen (`linalg.rs`, `solver.rs`). 3D-Kette mit 200 Kurven: Lösen 8,6 s, Ziehen 35 s.

- **Erst messen:** Messtest `perf_200_chained_3d` in `crates/planora-sketch/tests/three_d_perf.rs` (Aufbau `chain_3d`, aus Skizzen 10 Task 4) und dieselbe Kette in 2D als neuer Messtest, und Aufteilung der Zeit auf Aufbau der Jacobi-Matrix, Normalgleichungen, Cholesky und Rangprüfung. Ergebnis kommt in den Plan-Ledger, bevor gebaut wird.
- **Dünn rechnen:** Jacobi-Matrix je Zeile dünn (die Zeilen haben wenige Einträge). Variablen einer Gruppe einmal je Aufbau nach umgekehrtem Cuthill-McKee ordnen; `JᵀJ` als Hülle (Skyline) speichern und mit Hüllen-Cholesky lösen. Für Ketten ist das Band schmal; Aufwand etwa O(n·b²).
- **Rangprüfung nur bei Strukturänderung:** Freiheitsgrad, überzählige Abhängigkeiten und freie Variablen hängen von Struktur und Lage ab. Beim Ziehen (gleiche Abhängigkeiten, nur weiche Ziele) wird die Analyse vom Beginn des Zugs übernommen und erst beim Loslassen an der neuen Lage neu gerechnet. Während des Zugs können Färbung und Status darum kurz vom heutigen Verhalten abweichen (etwa beim Durchziehen durch eine Lage, in der zwei Linien parallel sind); im Ruhezustand sind sie gleich wie heute. Bewusst angenommen (**A10**). Die Rangprüfung selbst arbeitet ebenfalls auf der dünnen Matrix (Zeilen-Gram-Schmidt gegen eine Basis, deren Vektoren dünn bleiben, oder dünne QR mit der gleichen Ordnung). Ergebnisse müssen gleich sein wie bisher (gleiche Toleranzen).
- **2D darf nicht anders lösen:** Für Gruppen unter einer Schwelle (Messung entscheidet, etwa 60 Variablen) bleibt der dichte Weg. Darüber muss der dünne Weg dieselbe Lösung liefern (Toleranz 1e-9 auf den Variablen in den vorhandenen Tests, gleiche `dof`, gleiche `redundant`/`conflicting`).
- **Ziele (Release):** 3D-Kette 200: Lösen ≤ 0,5 s, ein Ziehschritt ≤ 50 ms. 2D: vorhandene Messtests (`import_perf.rs` u. a.) höchstens 5 % langsamer, große 2D-Gruppen schneller.
- Keine neue Bibliothek.

### Mittelpunktlinie

Eigene Art des Linienwerkzeugs (`Tool::Line` mit Art „Mittelpunktlinie“, umschaltbar im Skizzenfeld und in der Liste am Leistenknopf Linie). Vorschau symmetrisch, Felder Länge und Winkel am Zeiger wie die Linie. In 3D-Skizzen auf der Triadenebene (wie alle Arten, Skizzen 10).

### Übergangskurve

Neues Werkzeug unter Ändern, Art G1/G2 im Skizzenfeld. Zwei Klicks; die Nähe des Zeigers wählt das Ende (das nähere der beiden Enden der überfahrenen Kurve). Vorschau beim Überfahren der zweiten Kurve. Ergebnis: ein Befehl, ein Rückgängig-Schritt (Spline an den gemeinsamen Endpunkten, zwei `EndTangent` bzw. zwei „glatt“). Lässt sich der Spline mit den Abhängigkeiten nicht lösen, wird nichts angelegt und eine Meldung erscheint.

### Krümmungskamm

Kurve(n) wählen, Rechtsklick „Krümmungsanzeige ein/aus“: Bei Kurven ohne Kamm öffnet der Dialog (Dichte, Maßstab, Vorschau live), OK schaltet ein; bei Kurven mit Kamm schaltet der Befehl ohne Dialog aus. Bei genau einer gewählten Kurve mit Kamm zeigt das Skizzenfeld den Schalter und die zwei Werte zum Ändern. Zeichnen im Overlay der Skizzenansicht und in der 3D-Ansicht (dieselbe Linienfarbe wie Fusions Kamm: Magenta-ähnlich, eigene Graphit-Farbe `comb`). Rechnung der Krümmung je Kurvenart geschlossen (Bogen, Kreis, Ellipse) bzw. aus den Ableitungen (Splines, Kegelschnitt).

### Skizzenprüfung

Statuszeile oben im Skizzenfeld. Die heutige Freiheitsgrad-Anzeige im Fußfeld (`foot_field`) bleibt, wo sie ist. Knopf „Unterbestimmte finden“ öffnet die Liste im Skizzenfeld; sie bleibt offen, bis man sie schließt oder die Skizze verlässt, auch während anderer Werkzeuge. Zeilenklick: Element wählen, hervorheben, Ansicht so verschieben/zoomen, dass es mit Rand sichtbar ist (nur verschieben, wenn es schon passt). X: Element löschen (ein Rückgängig-Schritt), bei Punkten, die Kurvenenden sind, kein X. Haken „Bestimmte ausblenden“ dämpft voll bestimmte Geometrie in der Ansicht. Knopf „Skizze berechnen“ daneben. Schloss: neues Symbol im Baum, nur wenn Status „bestimmt“ und die Skizze mindestens ein Element hat.

### Verlorene Projektionen

Neue Funktion `weak_dependencies()` für die `copies`-Herkunft; `dependencies()` liefert nur noch harte Bezüge (Ebene). Heute ergibt eine gelöschte Quelle `UnknownFeature`, eine dahinter geschobene `ForwardReference`, und `check_references` bricht beim Einfügen, Verschieben und Laden ab (Bauprüfung). Darum gilt für schwache Bezüge:
- `dependents()` (Löschen-Dialog) und `check_references` (Einfügen, Verschieben, Laden) sehen sie nicht: Eine fehlende oder spätere Quelle ist kein Fehler. Eine Skizze darf in der Zeitleiste vor ihre Projektionsquelle geschoben werden; der Bezug ist dann verloren (Warnung), zurückgeschoben lebt er wieder auf.
- `blocked_by_dependency` und die Neuberechnung: Eine fehlende, spätere, unterdrückte oder fehlerhafte schwache Quelle ist ein verlorener Bezug (Warnung, letzte Lage bleibt), kein Fehler.
- `cache_key` und die Zurückroll-Kette (`rollback::chain`, `rolled_back`) nehmen schwache Bezüge mit, soweit die Quelle existiert und vor der Skizze steht (Verhalten wie heute).
- Verlauf (`history_bar/model.rs`), `refuse_if_locked` und `state.rs`: wie heute über alle Bezüge (hart und schwach), weil sie nur Hervorhebung und Sperren betreffen.
- Laden einer Datei, in der eine schwache Quelle fehlt (etwa nach dem Löschen gespeichert), geht ohne Fehler; Test dafür.

Neu: Markierung der verlorenen Kurven in der Skizze (gelbe Farbe), Eintrag „Verlorene Projektionen verwalten“ im Rechtsklickmenü der Zeitleiste und des Baums an einer Skizze mit Warnung, Dialog wie oben. Gilt für alle `CopySource`-Arten. Rückgängig stellt die Quelle wieder her, und die Verknüpfung lebt wieder auf (die Herkunft bleibt gespeichert).

### Nacharbeiten

- Bogen „Mittelpunkt“ 2D nach A6.
- Winkelmaß nach A7.
- Felder in 3D nach A8.
- Felder am Zeiger nach A9 (nur Doku).

### Dateiformat

Format 21 nur, wenn ein Kamm an ist, ein Winkelmaß „Gegenwinkel“ hat, ein `EndTangent` oder ein `Smooth` am Ellipsenbogen vorkommt. Die Mittelpunktlinie und eine G2-Übergangskurve an Kurven außer dem Ellipsenbogen ändern die Datei nicht. Alte Dateien bytegleich.

### Texte und Tasten

Alle neuen Texte über `tr!` in de und en, Namen wie in der deutschen/englischen Fusion („Mittelpunktlinie“/„Midpoint Line“, „Übergangskurve“/„Blend Curve“, „Krümmungsanzeige ein/aus“/„Toggle Curvature Display“, „Unterbestimmte finden“/„Find Under-Constrained“, „Skizze berechnen“/„Compute Sketch“, „Verlorene Projektionen verwalten“/„Manage Lost Projections“). Neue Befehle sind in den Einstellungen belegbar, ohne Vorbelegung (Fusion hat keine).

## Nicht in 11a

- Skizzenlinie als Drehachse und Körperkanten als Quelle der Übergangskurve: brauchen anklickbare fertige Skizzen und Kanten in der Skizzenansicht → „Anzeige in der Skizze“.
- Konstruktionspunkte (Punkt-Features) für Einbeziehen/Projizieren → „Konstruieren komplett“.
- Krümmungskamm-Analyse an Körperkanten (Inspect, Ordner „Analyse“) → „Prüfen und Material“.
- Alles aus 11b.

## Prüfung

- Tests je Baustein (Modell, Löser, App-Zustand), Messtests im Release für den Löser.
- Lokal voll auf Windows und WSL (fmt je Crate, clippy `-D warnings`, alle Tests, `cargo deny check`, Messtests). Kein CI vor dem Merge.
- Handtests bei 1920×1080 mit `tools/gui-drive.ps1`, jeweils nur nach Dennis' Go: nach dem Löser und den Zeichenwerkzeugen, nach Prüfung und verlorenen Projektionen, am Ende.

## Dem Bauenden überlassen

Aufteilung in Aufgaben, Namen im Code, Ablage der neuen Module, die genaue Schwelle dicht/dünn (nach Messung), die Farbe `comb` im Graphit-Thema, die Rechnung der Zoom-Lage beim Zeilenklick.
