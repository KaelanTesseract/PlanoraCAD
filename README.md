# PlanoraCAD

Ein quelloffenes, parametrisches 3D-CAD in reinem Rust für Windows und Linux.

**Status:** frühe Entwicklung (Fundament, Kernel-Basis, Boolesche Operationen, Skizzen mit Constraint-Solver, Feature-Baum, Speichern mit Projekten und Versionen, Freiformflächen, Rundungen und Fasen, Netz-Austausch, STEP-Export, Zeichnungen mit Ansichten).

Erste Version: Quader, Zylinder, Kugel, Kegel, Extrusion und Rotation von Skizzen, 3D-Ansicht mit Auswahl, Körper verschieben (Feld „Position“) sowie Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit Flächen aus Ebenen, Zylindern, Kegeln, Kugeln und Tori (Stufe A) und für Körper mit Freiformflächen aus Ellipsen und Splines (siehe „Freiformflächen“).

Bekannte Grenzen: Bei Körpern aus einfachen Flächen werden Schnitte, die Raumkurven ergeben (z. B. zwei Zylinder mit gekreuzten Achsen), noch nicht unterstützt (mit Freiformflächen dagegen oft schon, siehe „Freiformflächen“); das Vereinigen von Körpern, die sich nur an einer Kante berühren, manche genau tangentialen Lagen und Kanten oder Ecken, die genau auf einer Fläche des anderen Körpers aufliegen, liefern eine Fehlermeldung statt eines Ergebnisses (bei Berührung nur an einer Ecke entsteht ein Körper aus zwei Teilen mit gemeinsamer Ecke); es gibt noch kein Rückgängig. Details: `docs/superpowers/offene-punkte.md`.

## Skizzen

Über „Erzeugen → Skizze auf XY-/XZ-/YZ-Ebene“ oder „Skizze auf gewählter Fläche“ (ebene Fläche eines Körpers) entsteht eine Skizze. Im Skizzen-Modus schaut die Kamera senkrecht auf die Ebene; ein Fenster „Skizze“ enthält Werkzeuge, Abhängigkeiten und deren Liste.

**Was geht**
- Zeichnen: Punkt, Linie, Rechteck, Kreis, Bogen, Ellipse, Spline; Konstruktionsgeometrie über „Konstruktion umschalten“; Spiegeln und lineares Muster.
- Abhängigkeiten: deckungsgleich, waagerecht, senkrecht, parallel, rechtwinklig, tangential, gleich, fest, Mittelpunkt, symmetrisch, Punkt auf Kurve; Maße für Abstand, Abstand Punkt–Linie, Radius, Durchmesser und Winkel (Eingabe als Zahl).
- Der eigene Solver (keine neue Bibliothek) zeigt die Freiheitsgrade an, färbt bestimmte Geometrie anders ein und meldet überzählige und widersprüchliche Abhängigkeiten. Der Solver arbeitet je zusammenhängender Gruppe: Eine Gruppe mit Widerspruch (oder die sich nicht lösen lässt) bleibt in ihrem letzten Zustand, die übrigen Gruppen werden trotzdem gelöst. Lösungen, bei denen Geometrie entartet (Radius null oder negativ, Linie der Länge null, Bogenanfang auf der Mitte), gelten nicht als Lösung; die Gruppe bleibt dann ebenfalls stehen, und die Ursache wird genannt. Eine neue Abhängigkeit, die eine gelöste Skizze kaputt machen würde, wird abgelehnt; in einer schon fehlerhaften Skizze wird sie mit Warnung behalten. Ziehen startet vom letzten Zustand, mehrdeutige Lagen klappen in der Regel nicht um.
- Körperkanten einmalig als feste Konstruktionsgeometrie in die Skizze kopieren (Geraden; Kreise und Bögen in parallelen Ebenen; ganze Kreise und Ellipsen auch schräg, dann als Ellipse).
- „Skizze beenden“ legt die Skizze als Eintrag im Feature-Baum ab; sie bleibt als dünne Linien in der 3D-Ansicht sichtbar und lässt sich per Doppelklick oder Rechtsklick → „Bearbeiten“ wieder öffnen.
- Körper aus Skizzen: Rechtsklick auf die Skizze → „Extrudieren …“ / „Drehen …“ oder „Erzeugen → Skizze extrudieren … / Skizze drehen …“. Der Dialog listet die geschlossenen Profile mit Fläche und Löchern und erklärt, warum andere Kurvenzüge nicht verwendbar sind. Das Ergebnis ist ein normaler Körper, der sich mit den Booleschen Operationen weiterverarbeiten lässt und der Skizze folgt (siehe „Feature-Baum“).

**Was (noch) nicht geht**
- Profile müssen einfache geschlossene Ketten aus Linien, Bögen und Splines sein, oder ein Kreis, eine Ellipse oder ein Spline, dessen Enden aufeinanderliegen (keine T-Verzweigungen, keine Kreuzungen, kein Zerschneiden an Schnittpunkten). Ellipsen und Splines werden seit Teilprojekt 7 zu Körpern (siehe „Freiformflächen“).
- Drehen nur für Profile ohne Löcher, die mit genau einer Linie auf der Drehachse liegen und ganz auf einer Seite davon; immer 360°.
- Skizzen nur auf den Grundebenen und auf ebenen Körperflächen.
- Kopierte Körperkanten folgen der Kante nur, solange sie ihre Art behält (eine Gerade bleibt eine Gerade).
- Kein Rückgängig.
- Maße nur als Zahl, keine Gleichungen oder Parameter.
- Mehrdeutige Lagen (z. B. ein Winkel um 180° gedreht) entscheidet der nächstgelegene Zustand; es gibt keine Auswahl der Lösung.
- Der Solver rechnet je zusammenhängender Gruppe mit dichten Matrizen. Unveränderte Gruppen werden nicht neu gerechnet (auch beim Ziehen nicht); innerhalb einer Gruppe wird flüssiges Ziehen ab etwa 150–300 Variablen zäh (300 Variablen: etwa 0,15 s je Zugschritt im Release-Build). Widersprüchliche Gruppen sind langsamer: Beim Ziehen sucht der Solver dort nicht nach der Ursache, das holt er am Ende des Ziehens nach; hat eine Gruppe mehr als 48 Abhängigkeiten, meldet er statt der genauen Ursache „nicht lösbar“ mit den verletzten Abhängigkeiten. Ein Muster mit 100 Kopien eines Rechtecks ist eine Gruppe mit über 800 Variablen (Anlegen etwa 1,7 s, Ziehen etwa 1,5 s je Schritt).
- Fertige Skizzen liegen in der 3D-Ansicht immer über den Körpern (keine Verdeckung) und lassen sich dort nicht anklicken.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Nach Teilprojekt 4 (Skizzen)“.

## Freiformflächen

Ellipsen und Splines aus Skizzen werden jetzt zu Körpern. Für ihre Wände baut das Programm Freiformflächen (in der Fachsprache NURBS): Flächen, die der gezeichneten Kurve genau folgen, statt sie aus Ebenen und Zylindern zusammenzusetzen.

**Was geht**
- Extrudieren: eine Ellipse, ein geschlossener Spline oder eine geschlossene Kette aus Linien, Bögen und Splines. Eine Ellipse in einem Rechteck wird zum Loch in der Platte.
- Drehen: auch Profile mit Splines, mit denselben Regeln wie bisher (ohne Löcher, eine Linie auf der Drehachse, immer 360°). Eine Ellipse allein lässt sich nicht drehen, weil sie keine Linie auf der Achse hat.
- Vereinigen, Abziehen und Schneiden funktionieren auch mit solchen Körpern und auch zwischen zwei Freiformkörpern (zum Beispiel zwei sich kreuzende Walzen verschiedener Dicke). Durch Tests geprüft ist das vor allem zusammen mit Quadern und Zylindern; mit Kugeln und Kegeln ist bisher nur das Berechnen der Schnittlinien geprüft.
- Änderst du ein Maß der Ellipse oder einen Punkt des Splines, rechnen sich Extrusion, Drehung und alle Booleschen Operationen danach neu.
- Speichern und Öffnen wie bisher. Das Dateiformat hat sich nicht geändert.
- Im Eigenschaftenfenster heißt eine solche Fläche „Freiformfläche“ und eine solche Kante „Freiformkurve“.

**Grenzen**
- Manche Lagen kann das Programm mit Freiformflächen nicht sicher berechnen. Dann steht am Schritt im Feature-Baum eine Meldung, die den Hinweis „Bei Körpern mit Freiformflächen:“ enthält; die Körper davor bleiben, wie sie waren, und Schritte, die darauf aufbauen, werden übersprungen. Oft hilft es, einen Körper ein wenig zu verschieben oder ein Maß leicht zu ändern. Typische Fälle:
  - Flächen, die sich nur berühren oder sehr flach schneiden (Meldung „nahezu tangential“ oder „berühren sich“),
  - zwei gleich dicke Walzen aus Freiformflächen, die sich kreuzen (an der Stelle, wo sich beide Schnittlinien treffen, kann das Programm nicht rechnen),
  - sehr dünne Stellen im Ergebnis, dünner als etwa 0,0001 mm (ein zehntausendstel Millimeter),
  - Freiformkörper, die winzig klein sind (unter etwa 0,00002 mm),
  - zwei Freiformflächen, die nur teilweise genau aufeinanderliegen.
- Selten fehlt beim Verschneiden ein ganz kurzes Stück einer Schnittlinie, ohne dass eine Meldung kommt. Meist merkt die Prüfung des fertigen Körpers das und meldet einen Fehler, aber nicht immer: In seltenen Fällen kann das Ergebnis einer Booleschen Operation mit Freiformflächen falsch sein.
- Schnittlinien mit Freiformflächen sind Näherungen. Sie weichen höchstens 0,00001 mm (ein hunderttausendstel Millimeter) von den Flächen ab; das ist für die Darstellung und für Maße ohne Bedeutung.
- Boolesche Operationen mit Freiformflächen dauern länger als mit einfachen Flächen: im Release-Build meist eine halbe bis eine Sekunde. Körper aus Splines mit vielen Punkten (etwa ein gedrehter Spline mit 39 Punkten, mit einem Zylinder verschnitten) brauchen oft mehrere Sekunden, in Tests bis etwa 16 Sekunden. Damit das nicht beliebig lange dauert, hat jede solche Operation eine feste Arbeitsgrenze; wird sie erreicht, kommt die Meldung „Die Berechnung ist zu aufwendig“, und das Dokument bleibt, wie es war. Das kann auch Lagen treffen, die eigentlich lösbar wären. Während der Rechnung reagiert das Fenster nicht, auch beim Öffnen eines Dokuments mit solchen Schritten.
- Das Bildschirmnetz kann bei seltenen Ergebnissen winzige Lücken haben, vor allem an sehr schmalen Streifen bei grober Netzgenauigkeit. Zwei Ursachen sind behoben (Quader auf der Stirnfläche eines Zylinders; doppelt belegte Stücke an Freiformflächen). In den Testreihen mit gedrehten Quadern gegen einen Freiformzylinder (über 6000 Ergebnisse) gab es danach keine Lücken mehr. Ohne Freiformflächen bleiben seltene Fälle bei grober Netzgenauigkeit: In einer Stichprobe von 200 Ergebnissen ließen sich zwei bei 0,05 gar nicht vernetzen (mit Meldung), eines hatte bei 0,1 bis 0,01 eine kleine Lücke. Ältere Prüfungen hatten 0,5 bis 2 von 100 Ergebnissen gemeldet; deren Testreihen lagen nicht mehr vor. Der Körper selbst ist davon nicht betroffen. Erkennt das Programm, dass das Netz eines Körpers an einzelnen Stellen ungenau ist, zeigt die Statuszeile einen Hinweis (wenn dort keine andere Meldung steht).
- **Alte Dokumente:** Ellipsen und Splines bekommen jetzt eine Profilnummer, vorher wurden sie übergangen. Eine Extrusion oder Drehung merkt sich ihr Profil nur als Nummer. Deshalb kann ein altes Dokument mit Ellipsen oder Splines in der Skizze beim Öffnen ein anderes Profil verwenden als früher, eine Platte kann ein Loch bekommen, oder es erscheint ein Fehler „Profil 0 gibt es nicht“.
- Kontrollpunkte von Freiformflächen lassen sich nicht direkt bearbeiten; Loft und Sweep gibt es noch nicht.

**Handprüfung:** Am 12.10.2026 von Hand im echten Fenster geprüft (Release-Build): Ellipse auf der XY-Ebene skizzieren und extrudieren (Profilfläche 633,30 mm², Volumen etwa 6327 mm³, die Fläche heißt im Eigenschaftenfenster „Freiformfläche“), Zylinder anlegen und über „Position“ verschieben, Abziehen (Zylinder minus Ellipsenkörper: 7 Flächen, 15 Kanten, 16 110 mm³), Mittelpunkt der Ellipse in der Skizze ziehen (der Körper rechnet sich neu: 16 341,67 mm³), in ein Projekt speichern, Programm hart beenden, neu starten, die zurückgebliebene Sperre mit „Sperre übernehmen“ übernehmen und das Dokument öffnen (gleiche Form, gleiches Volumen, gleiche 7 Flächen). Ein Spline in einer Skizze auf der XZ-Ebene, mit einer Linie geschlossen, die den Spline kreuzt: Die Oberfläche meldet richtig „Nicht verwendbar: Das Profil schneidet oder berührt sich selbst oder ein anderes Profil“. Nicht von Hand geprüft (nur durch automatische Tests ohne Fenster): Extrusion oder Drehung eines gültigen Splines im Fenster, Skizze auf einer Körperfläche, Ändern der Halbachsen-Maße, Arbeitsgrenze und Einfrieren des Fensters, der Netz-Hinweis in der Statuszeile. Dabei aufgefallen: Für eine Ellipse gibt es in der Skizzen-Oberfläche noch keine Maß-Eingabe (Radius, Durchmesser und Abstand sind ausgegraut).

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 7 – NURBS (Freiformkurven)“.

## Rundungen und Fasen

Kanten eines Körpers lassen sich runden oder abschrägen (Menü „Bearbeiten“ → „Verrunden …“ bzw. „Fase …“). Im Fenster die Kante in der Ansicht anklicken; mit gedrückter Umschalttaste kommen weitere Kanten dazu, ein zweiter Klick nimmt eine Kante wieder heraus. Ist „Tangentenkette“ eingeschaltet, werden Kanten, die glatt anschließen (etwa rund um ein Langloch), automatisch mitgenommen.

- **Rundung:** fester Radius oder variabler Radius über Punkte entlang der Kante (Position in Prozent, Radius); dazwischen ändert sich der Radius glatt und ohne Überschwingen.
- **Fase:** gleicher Abstand auf beiden Flächen, zwei Abstände (Fläche 1 und Fläche 2) oder Abstand und Winkel.
- Treffen zwei gerundete Kanten an einer Ecke zusammen, laufen die Rundungen schräg ineinander; drei gerundete Kanten mit gleichem Radius an einer Ecke aus drei ebenen Flächen bekommen ein Kugelstück.
- Fälle, die das Programm nicht sicher rechnen kann, werden mit einer Meldung abgelehnt, zum Beispiel ein Radius, der größer ist als die Fläche daneben, oder Flächen, die fast glatt ineinander übergehen. Im Zweifel lehnt das Programm lieber ab, als einen falschen Körper zu liefern.
- Vor der Ausgabe wird das Ergebnis geprüft: Der Körper muss geschlossen und gültig sein, keine Kante einer anderen Fläche darf durch die neue Fläche laufen, Löcher in den Nachbarflächen werden berücksichtigt, und jede Fläche, die der neuen Fläche nahekommt, wird mit ihr geschnitten. Lässt sich dieser Schnitt nicht genau rechnen (etwa bei einer Kugel und einem Zylinder mit verschiedenen Achsen), kommt eine Meldung. Ganz ausgeschlossen ist ein falscher Körper damit nicht; die bekannten Lücken stehen unten unter „Grenzen“.

Rundungen stehen wie alle Schritte in der Zeitleiste, lassen sich nachträglich ändern, unterdrücken und löschen. Dokumente mit Rundungen oder Fasen speichert das Programm im Dateiformat 2; ältere Programmversionen melden dann „zu neu“. Dokumente ohne Rundung bleiben im Format 1.

**Grenzen**
- Körper mit Hohlräumen oder mehreren getrennten Teilen lassen sich noch nicht runden oder fasen.
- An Ecken ist vieles noch nicht möglich. Zwei gerundete Kanten laufen nur dann sauber ineinander, wenn ihre Seiten spiegelgleich liegen. Drei Rundungen mit gleichem Radius bekommen ein Kugelstück. Nicht möglich sind Ecken mit variablem Radius oder an Freiformflächen, Ecken, die nach innen springen, drei Fasen an einer Ecke und sehr spitze Ecken. Die Ecken-Stile „Setback“ und „glatte Ecke“ gibt es noch nicht.
- Sehr kleine Maße lehnt das Programm ab: unter 0,0001 mm, bei Freiformflächen oder variablem Radius unter 0,002 mm. Fasenwinkel gehen von 1° bis 179°.
- Ein Teil der Anfragen wird abgelehnt, auch wenn er sich theoretisch rechnen ließe. In unseren Zufallstests waren es bei Quadern etwa 9 von 100 Anfragen (bei einzelnen Kanten), bei Zylindern etwa 70 von 100, bei Kegeln etwa 84 von 100. Mehrere Kanten, Ecken und Körper mit Löchern werden öfter abgelehnt; es steht immer eine Meldung dabei.
- Boolesche Operationen (Vereinigen, Abziehen, Schneiden) mit einem gerundeten Körper werden oft abgelehnt, vor allem, wenn dabei Kegel- oder Ringflächen aus einer Rundung getroffen werden.
- Bei steil verlaufenden variablen Radien kann das Bildschirmnetz sehr kleine Lücken haben (in etwa 1 von 100 Fällen); der Körper selbst ist in Ordnung, die Statuszeile zeigt dann „Netz ungenau“. Bei Rundungen an Freiformflächen ist das Netz bei grober Einstellung bis zu einigen Prozent im Volumen ungenau.
- Eine kleine Rundung, die ein Teil des Körpers wegnimmt oder hinzufügt, prüft das Programm auf Plausibilität. Fehlt dabei nur die kleinere Art (bis etwa ein Fünftel), fällt das nicht auf; das Ergebnis ist dann geometrisch geprüft, aber nicht zusätzlich über das Volumen.
- Was die Prüfung nicht abdeckt: Bei zwei Flächen, die schon eine Ecke oder Kante gemeinsam haben, sucht das Programm keinen weiteren Schnitt, wenn es ihn nicht genau rechnen kann. Bei den beiden Flächen direkt an der gerundeten Kante verlässt es sich auf die Bauweise und die Krümmungsprüfung. Eine solche Lücke ist in den Tests bisher nicht aufgefallen, aber auch nicht ausgeschlossen. Bis Oktober 2026 gab es eine weitere: Eine Kugeltasche, die sich von innen bis in die Rundung wölbte, ohne dass ihre Kanten die Rundung trafen, wurde übersehen und ergab einen falschen Körper. Das ist behoben; solche Fälle werden jetzt abgelehnt.
- Die Kantenverweise im Schritt werden nach einer Neuberechnung nicht aktualisiert; „Fläche 1“ wird über den Flächennamen gewählt und wirkt bei zwei gleich benannten Flächen nicht. Der Hinweis zum Mindestmaß erscheint erst, nachdem eine Kante gewählt ist.

Einzelheiten und Messwerte: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 8 – Rundungen und Fasen“.

## Netz-Austausch (Export und Import)

Körper lassen sich als Dreiecksnetz speichern, etwa für den 3D-Druck oder um sie in einem anderen Programm anzusehen: Menü „Datei“ → „Exportieren …“ (Strg+Umschalt+E).

- **Formate:** STL (für den 3D-Druck), OBJ (für viele 3D-Programme), 3MF (für den 3D-Druck, mit dem Namen jedes Körpers) und glTF als .glb (zum Ansehen und Zeigen).
- **Qualität:** Grob, Normal oder Fein. Gewölbte Flächen werden aus kleinen Dreiecken zusammengesetzt. Bei „Fein“ weicht das Netz höchstens ein tausendstel Millimeter von der echten Fläche ab; die Datei wird dafür größer. Das Fenster zeigt vorher, wie viele Dreiecke es werden und wie groß die Datei ungefähr wird.
- **Umfang:** Ist ein Körper gewählt, nur dieser; sonst alle sichtbaren Körper. Unterdrückte Schritte zählen nicht.
- **Prüfung:** Ist das Netz nicht ganz geschlossen (das kann bei Körpern mit Freiformflächen und bei eingelesenen Netzen mit Rundungsresten vorkommen), warnt das Fenster. Die Datei wird trotzdem geschrieben.
- Das Programm schreibt zuerst in eine Hilfsdatei. Eine vorhandene Datei wird erst ersetzt, wenn alles geklappt hat.

Netze aus anderen Programmen lassen sich einlesen: „Datei“ → „Netz importieren …“ (STL und OBJ). Das Fenster zeigt vorher, wie viele Dreiecke die Datei hat und ob das Netz geschlossen ist. Die Einheit der Datei lässt sich wählen (Millimeter, Zentimeter, Meter, Zoll). Das Ergebnis ist ein **Netz-Körper**: Er lässt sich anzeigen, verschieben, drehen, vergrößern oder verkleinern, messen und wieder exportieren. Mit anderen Körpern verrechnen, runden oder als Fläche für eine Skizze nutzen lässt er sich nicht; diese Menüpunkte sind dann ausgegraut.

Dokumente mit Netz-Körpern speichert das Programm im Dateiformat 3; das Netz steckt im Dokument selbst. Ältere Programmversionen melden dann „zu neu“. Dokumente ohne Netz-Körper bleiben im Format 1 oder 2.

**Grenzen**
- Höchstens 50 Millionen Dreiecke und 8 GB je Datei. Das schützt nicht vor Speichermangel: Zum Laden braucht das Programm ungefähr das Dreifache der Dokumentgröße an Arbeitsspeicher; ein einzelnes Netz hat höchstens etwa 2,4 GB Netzdaten und braucht beim Laden rund 13 GB Spitze; bei 8 GB Netzdaten im Dokument sind es bis etwa 35 GB. Ab 5 Millionen Dreiecken zeigt die Ansicht nur einen Teil der Dreiecke; gespeichert und exportiert wird alles.
- Das Netz steckt als Text im Dokument; das Dokument wird dadurch etwa ein Drittel größer als die reinen Netzdaten und lädt bei großen Netzen langsamer. Bei Netzen um 5 Millionen Dreiecke wird auch die Ansicht spürbar langsam, weil sie nach jeder Änderung am Dokument neu aufgebaut wird.
- Farben, Materialien und Texturen werden nicht übernommen. 3MF-Dateien über 4 GB kann das Programm nicht schreiben (dann STL wählen); bei glTF gilt dieselbe Grenze.
- In OBJ-Dateien führen Vielecke, die sich nicht sauber in Dreiecke zerlegen lassen, und Flächen, die auf später stehende Punkte verweisen, zu einer Meldung mit Zeilennummer. Ein Vieleck, das nicht gewölbt nach außen liegt, darf höchstens 5000 Ecken haben. Sehr fein gerundete Einzel-Vielecke mit mehr Ecken werden deshalb abgelehnt.
- Eine Binärdatei, die mit dem Wort „solid“ beginnt, aber eine falsche Größe hat, wird als Text gelesen; die Meldung dazu ist dann irreführend.
- Wird das Dokument geändert, während das Fenster „Netz importieren“ offen ist, verwirft es die gelesene Datei; sie muss dann neu gewählt werden.
- Zum Wählen der Datei hat das Programm ein eigenes Fenster mit Ordnerliste und Dateiname; der Dateidialog des Betriebssystems wird nicht benutzt, weil dafür ein zusätzliches Programmpaket nötig wäre. Unter Windows wechselt man das Laufwerk, indem man den Pfad eintippt. Sehr große Ordner und Netzlaufwerke können das Fenster kurz hakeln lassen.
- Während das Netz berechnet oder eine Datei gelesen wird, bleibt das Fenster bedienbar. „Abbrechen“ wirkt beim Export zwischen zwei Körpern; ein einzelner großer Körper wird noch zu Ende gerechnet.
- Nicht dabei: das Einlesen von 3MF, glTF oder STEP. (Zeichnungen als SVG/DXF gibt es seit Teilprojekt 11, siehe „Zeichnungen“.)
- Von Hand im echten Fenster ist der Netz-Austausch teilweise geprüft (Export, Import, Speichern und Laden); Abbrechen, sehr große Netze und das Öffnen der Dateien in fremden Programmen nicht. Siehe „Handprüfung“ in den offenen Punkten.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 9 – Netz-Austausch“.

## STEP-Export (exakte Körper)

Körper lassen sich als STEP-Datei speichern. Andere CAD-Programme öffnen sie als echte Volumenkörper, nicht als Dreiecksnetz: Menü „Datei“ → „Exportieren …“ (Strg+Umschalt+E), Format „STEP – für andere CAD-Programme (exakt)“.

- Rundungen, Bohrungen und Freiformflächen bleiben exakt. Deshalb gibt es bei STEP keine Auswahl der Qualität.
- Mehrere Körper kommen in eine Datei, jeder als eigenes Teil mit seinem Namen aus dem Feature-Baum. Ein Körper mit Hohlraum bleibt ein Körper mit Hohlraum; getrennte Stücke eines Körpers bleiben ein Teil.
- Eingelesene Netz-Körper kann STEP nicht aufnehmen. Das Fenster nennt sie mit Namen; für sie STL, OBJ, 3MF oder glTF wählen.
- Vor dem Schreiben prüft das Programm jeden Körper. Wer die Prüfung nicht besteht, wird ausgelassen und mit Grund genannt; die anderen werden geschrieben.
- Das Fenster zeigt vorher, wie viele Körper und Flächen es werden. Das Schreiben läuft im Hintergrund mit Fortschrittsbalken und lässt sich abbrechen. Eine vorhandene Datei wird erst ersetzt, wenn alles geklappt hat.

**Grenzen**
- In der Datei stehen nur Form, Namen und die Einheit Millimeter; keine Farben, Materialien oder Baugruppen. STEP-Dateien einlesen kann das Programm noch nicht.
- **Fusion 360:** Alle Beispieldateien lassen sich dort ohne Fehler öffnen; bei Kugel und Hohlkörper stimmen Volumen und Oberfläche mit den erwarteten Werten überein. Bei den übrigen wurden sie dort noch nicht verglichen. Beim Bauen prüft das Programm seine Dateien nur mit einem eigenen Leser und eigenen Formeln. Andere Programme können strenger sein als Fusion 360.
- Beendet man das Programm oder schließt man den Tab, während eine STEP-Datei geschrieben wird, wird das Schreiben abgebrochen und die alte Datei bleibt unverändert (ohne Meldung).
- Ringe aus einer Drehung, bei denen der Bogen die Drehachse berührt oder überschreitet (sogenannte Spindelringe), kann das Programm noch nicht als STEP ausgeben. Solche Körper werden ausgelassen und mit Grund genannt.
- Kugeln und Kegelspitzen werden so geschrieben, wie es viele Programme tun: Der Pol ist ein Eckpunkt, die Naht läuft hin und zurück. Falls ein Programm das nicht mag, wird die Kugel später in zwei Hälften geteilt.
- Körper mit mehreren Schalen braucht das Programm zum Unterscheiden von Hohlraum und Stück ein grobes Netz; lässt es sich nicht berechnen, wird der Körper mit Grund ausgelassen. Bei verschachtelten Hohlräumen kann ein Hohlraum, der sehr nah (unter 0,05 mm) an einer gekrümmten Fläche der inneren Insel beginnt, falsch zugeordnet werden; das ist sehr unwahrscheinlich.
- Namen über 256 Zeichen werden in der Datei gekürzt; das Fenster nennt sie nach dem Schreiben. Gibt man „.stp“ ein, wird daraus „.step“; Dateien mit „.stp“ stehen aber nicht in der Liste. Zwei Körper mit gleichem Namen werden zwei Teile mit demselben Namen. Bei Körpern mit vielen Hohlräumen dauert die Zuordnung länger.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 10 – STEP-Export“.

## Zeichnungen (Ansichten)

Aus den Körpern eines Dokuments entsteht eine technische Zeichnung: Menü „Zeichnung“ → „Neue Zeichnung …“. Die Zeichnung gehört zum Dokument, wird mit gespeichert und rechnet sich neu, wenn man das Modell ändert. Ausgeben lässt sie sich als SVG (zum Drucken und für Dokumente) oder als DXF (für Laser, Fräse und andere CAD-Programme): „Datei“ → „Exportieren …“, Format „Zeichnung als SVG“ oder „Zeichnung als DXF“.

**Was geht**
- Ansichten: Vorne, Hinten, Links, Rechts, Oben, Unten und Isometrisch. Die Normansichten legen sich selbst an die richtige Stelle, wahlweise im Erstwinkel (Europa) oder im Dritten Winkel (USA); sie lassen sich nur entlang ihrer Richtung verschieben. Die isometrische Ansicht steht frei auf dem Blatt.
- Blatt von A4 bis A0, hoch oder quer, mit Rahmen 10 mm vom Rand. Jede Ansicht hat ihren eigenen Maßstab (1:1, 1:2, 1:5, 1:10, 1:20, 1:50, 1:100, 2:1, 5:1, 10:1). Beim Anlegen schlägt das Programm den größten Maßstab vor, bei dem die drei Hauptansichten aufs Blatt passen.
- Sichtbare Kanten sind durchgezogen, verdeckte gestrichelt. Das Zeigen der verdeckten Kanten lässt sich je Ansicht ausschalten (Vorgabe: an für die Normansichten, aus für Isometrisch).
- Kreise bleiben Kreise, Bögen bleiben Bögen, Freiformlinien bleiben glatte Kurven; es werden keine Streckenzüge daraus. Nur die Umrisse von Ringen (Torus) und Freiformflächen sind angenähert (auf 0,0001 mm genau).
- Im Fenster: Umschalter zwischen 3D-Modell und Zeichnung, Zoomen mit dem Mausrad, Verschieben mit der mittleren Maustaste, Ansichten anklicken und ziehen. Im Browser stehen die Zeichnungen mit ihren Ansichten. Die Berechnung läuft im Hintergrund mit Fortschrittsbalken; scheitert eine Ansicht, steht der Grund an ihr, und die Zeichnung bleibt erhalten.
- Gespeichert wird nur die Beschreibung (Blatt, Ansichten, Maßstab, Lage), nicht die berechneten Linien. Ein Dokument mit Zeichnung bekommt das Dateiformat 4 (mit Maßen oder Mittellinien das Dateiformat 5); ältere Programmversionen melden dann „zu neu“. Dokumente ohne Zeichnung bleiben unverändert.

**Grenzen**
- Nicht dabei: Schriftfeld, Beschriftung, Schnittansichten, Schraffur, Detailansichten, Stücklisten, PDF und das Einlesen von SVG oder DXF. Es gibt nur diese sieben Blickrichtungen. Maße und Mittellinien stehen im nächsten Abschnitt.
- Netz-Körper (eingelesene Netze) werden nicht gezeichnet; das Programm nennt sie mit Namen.
- Glatte Übergänge, etwa zwischen einer Rundung und der Fläche daneben, werden nicht als Linie gezeichnet. Der Umriss der Rundung selbst schon.
- Kann das Programm bei einer Linie nicht sicher sagen, ob sie sichtbar oder verdeckt ist, steht an der Ansicht eine Meldung statt einer Zeichnung; es wird nichts geraten. Bei sehr vielen Flächen meldet es „zu aufwendig“ (höchstens 20 000 Linien je Ansicht).
- An runden Teilen kann eine sehr knapp angeschnittene Abflachung oder Bohrung eine Doppellinie ergeben; auf Papier fällt das nicht auf, in einer DXF-Datei liegen dann zwei Linien übereinander.
- Freiformflächen: Ein sehr kleiner, in sich geschlossener Umriss (kleiner als ein Bruchteil der Körpergröße) kann fehlen, ohne dass es gemeldet wird. Eine Freiform mit einem Pol (die Wand läuft in einer Spitze auf der Achse zusammen) wird in Seitenansichten abgelehnt. Ein ausgezogenes Spline-Profil, das man genau von der Seite sieht, kann eine Linie doppelt zeichnen. Bei einem schräg liegenden Ring, den man fast genau von der Seite seiner Ringebene sieht, meldet das Programm „Umriss zu stark gekrümmt“.
- In Inkscape ist das Öffnen noch nicht von Hand geprüft (siehe „Handtest“ in den offenen Punkten). Beim Bauen prüft das Programm seine Dateien nur mit eigenen Lesern. In Fusion 360 haben SVG-Dateien und DXF-Dateien mit Linien, Kreisen, Bögen und Ellipsen gut funktioniert. Geschlossene Freiformkurven zeigt Fusion an, erkennt sie aber nicht als Fläche; das betrifft nur das Weiterverarbeiten dort, nicht das Ansehen oder Schneiden. Die neu erzeugten DXF-Proben Freiform, Zylinder, Torus und Rundung wurden dort erneut geprüft und lassen sich richtig einfügen; die übrigen Proben wurden nach den letzten Korrekturen nicht einzeln neu geprüft. In der DXF-Datei stehen die Ansichten als Blöcke; falls ein Programm sie nicht anzeigt, hilft es, die Blöcke aufzulösen.
- Das Schreiben der Datei lässt sich abbrechen; die alte Datei bleibt dann unverändert. Unter Windows reservierte Dateinamen (zum Beispiel „CON“) werden im Namensvorschlag nicht abgefangen.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 11 – Zeichnungen (Ansichten)“.

## Maße und Mittellinien

In einer Zeichnung lassen sich Maße setzen und Mittellinien zeigen. Die Maße hängen am Körper: Ändert man den Körper, ändert sich das Maß mit. Alles kommt in die SVG- und DXF-Dateien.

**Was geht**
- Mittellinien: Bohrungen, Zylinder, Kugeln und Ringe bekommen strichpunktierte Mittellinien. In den Normansichten sind sie von Anfang an an, je Ansicht lassen sie sich ausschalten (zweite Zeile der Werkzeugleiste, wenn eine Ansicht gewählt ist). In der Isometrie gibt es keine Mittellinien; der Schalter ist dort gesperrt.
- „Maße vorschlagen“: Ein Knopf setzt die wichtigsten Maße selbst: Breite, Höhe und Tiefe, Durchmesser und Radien sowie den Abstand der Bohrungsmitten von der Kante. Jedes Maß kommt nur einmal aufs Blatt, ein zweiter Druck setzt nichts doppelt. Danach lässt sich alles ändern.
- Werkzeug „Maß“: Man wählt die Art (waagerecht, senkrecht, schräg, Durchmesser, Radius, Winkel, Maßkette oder Bezugsmaße), klickt die Linien an und klickt dann dorthin, wo die Maßlinie stehen soll. Mit der Esc-Taste bricht man ab. Gesetzte Maße lassen sich anklicken und ziehen (bei einer Maßkette oder Bezugsmaßen wandert die ganze Gruppe mit); ein Doppelklick öffnet das Fenster „Maß bearbeiten“. Im Browser steht unter der Zeichnung der Abschnitt „Maße“.
- Wer eine Ansicht löscht, an der Maße hängen, wird vorher gefragt; die Frage nennt die Zahl der Maße.
- Toleranzen: plus/minus, oben und unten, Grenzmaße, und eine Passung (zum Beispiel H7) als Text hinter dem Wert. Dazu Nachkommastellen und ein eigener Text.
- Geht der Bezug eines Maßes verloren (zum Beispiel weil man die Kante gelöscht hat), bleibt das Maß erhalten und wird rot, mit dem Grund. Ein falscher Wert wird nie gezeigt. Solange ein rotes Maß da ist, schreibt das Programm die Zeichnung nicht in eine Datei; man löscht das rote Maß (und setzt es bei Bedarf neu) und exportiert dann. „Maße vorschlagen“ setzt neben einem roten Maß nicht dasselbe Maß noch einmal.

**Grenzen**
- Maße gibt es nur in den sechs Normansichten, nicht in der Isometrie. Steht eine Kante oder ein Kreis schräg zur Ansicht, lehnt das Programm das Maß ab, statt einen verzerrten Wert zu zeigen.
- Bemaßen lassen sich gerade Kanten, Kreise und Bögen, Zylinder, Kugeln und Eckpunkte. Ellipsen, Freiformkurven und ebene Flächen nicht.
- Der Vorschlag setzt nur Grundmaße, keine vollständige Fertigungsbemaßung.
- Passungen sind nur ein Text; es gibt keine Passungstabellen. In der DXF-Datei sind die Maße einfache Linien, Pfeile und Texte, keine echten CAD-Maße.
- Wie breit ein Text wird, schätzt das Programm nur. Mit einer anderen Schrift kann er etwas breiter ausfallen.
- Ob Fusion 360 die Maßtexte und Pfeile der DXF-Dateien richtig anzeigt, ist noch nicht von Hand geprüft.
- Nicht dabei: Schriftfeld, freie Texte, Stückliste und Schnitte (kommen später).

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 12 – Maße und Mittellinien“.

## Feature-Baum

Alles, was du baust, steht als Liste von Schritten im Feature-Baum: Quader, Skizze, Extrusion, Drehung, Boolesche Operation, Verschiebung. Die Reihenfolge ist die Reihenfolge der Entstehung.

**Was geht**
- Zeitleiste: Jeder Schritt baut auf den früheren auf. Ein Schritt kann mit Rechtsklick umbenannt, bearbeitet oder gelöscht werden.
- Neuberechnung: Änderst du ein Maß (z. B. in einer Skizze oder die Höhe einer Extrusion), rechnen sich alle Schritte danach neu. Nur das, was sich wirklich ändert, wird neu gebaut.
- Feste Verweise: Eine Skizze auf einer Körperfläche oder eine Kantenkopie merkt sich die Fläche bzw. Kante und findet sie nach einer Änderung wieder.
- Unterdrücken: Ein Schritt lässt sich abschalten. Schritte, die von ihm abhängen, werden dann übersprungen.
- Zurückrollen: Über „Zurückrollen bis hier“ siehst du den Stand früherer Schritte; „Zum Ende“ holt alles zurück.
- Löschen mit Folgen: Beim Löschen zeigt das Programm, welche weiteren Schritte davon betroffen sind, und fragt nach.
- Fehler stehen an dem Schritt, der sie auslöst. Alles davor bleibt benutzbar; Schritte, die davon abhängen, werden übersprungen.

**Was (noch) nicht geht**
- Kein Rückgängig.
- Keine benannten Maße oder Gleichungen.
- Keine Muster- oder Spiegel-Schritte.
- Der Zurückroll-Marker lässt sich nur über das Menü setzen, nicht mit der Maus ziehen.
- Die Ebene einer Skizze lässt sich nachträglich nicht ändern.
- Ein Körper lässt sich nicht nachträglich vor eine Skizze setzen, die ihn braucht.
- Verschiebst du einen Körper im Eigenschaftenfenster, kommt ein neuer Schritt ans Ende; Skizzen und Extrusionen davor ziehen nicht mit. Änderst du dagegen die Maße des Quaders selbst, folgen sie.
- Die Oberfläche des Feature-Baums wurde zuletzt von Hand durchgespielt (mit `tools/gui-drive.ps1`): Quader anlegen und ändern, Skizze auf einer Fläche, Extrusion, Vereinigen, Unterdrücken, Zurückrollen, Löschen mit Rückfrage und Position ziehen liefen ohne Absturz. Rotation und Fehlerfälle wurden nur durch automatische Tests geprüft.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Nach Teilprojekt 5 (Feature-Baum)“.

## Speichern, Projekte und Versionen

Deine Arbeit bleibt jetzt erhalten. Gespeichert wird die Liste der Schritte aus dem Feature-Baum; beim Öffnen rechnet das Programm daraus alles neu.

**Wo die Dateien liegen**
- Alles liegt in einem normalen Ordner auf deiner Festplatte, vorgegeben ist `Dokumente\PlanoraCAD`. Im Fenster „Projekte“ kannst du einen anderen Ordner wählen („Wurzel wählen“). Das Programm merkt sich die Wahl.
- Ein **Projekt** ist ein Ordner direkt darin. Darin kannst du beliebig viele **Ordner** anlegen und verschachteln. Ein **Dokument** ist ein Ordner mit der Endung `.pcad`; darin liegen die Versionen als einzelne Textdateien. Du kannst den ganzen Projektordner also einfach kopieren oder sichern.
- Gelöschte Projekte, Ordner und Dokumente landen im Ordner `.papierkorb` unter dem Projektordner und werden vom Programm nie endgültig gelöscht. Gelöschte Versionen und Zweige im Fenster „Versionen“ sind dagegen nach der Rückfrage endgültig weg.

**Speichern und Versionen**
- Mit „Datei → Speichern“ (Strg+S) legst du eine **neue Version** an; dabei kannst du einen Kommentar eintragen. Frühere Versionen bleiben unverändert erhalten.
- Ein neues Dokument heißt „Unbenannt 1“. Beim ersten Speichern wählst du Projekt, Ordner und Namen. „Speichern unter …“ legt ein eigenes neues Dokument an.
- Ein „●“ am Tab zeigt, dass es ungespeicherte Änderungen gibt. Beim Schließen eines Tabs oder des Programms fragt PlanoraCAD nach (Speichern, Verwerfen, Abbrechen). Eine gerade offene Skizze wird beim Speichern mitgenommen.
- Das Fenster „Versionen“ (Menü „Ansicht“) listet alle Versionen des Dokuments mit Nummer, Zweig, Zeit und Kommentar. Ein Doppelklick öffnet eine alte Version **nur zum Ansehen** in einem eigenen Tab („Name · v1“); dort lässt sich nichts ändern.
- Per Rechtsklick auf eine Version kannst du sie **wiederherstellen** (sie wird als neueste Version noch einmal gespeichert, nichts geht verloren), von ihr aus einen **Zweig abzweigen** (eine Variante mit eigenem Namen), den Kommentar ändern oder eine Version bzw. einen Zweig löschen, solange keine andere Version darauf aufbaut.
- Speicherst du ein Dokument, das auf einer älteren Version beruht, fragt das Programm nach dem Namen eines neuen Zweigs, damit nichts überschrieben wird.
- Zusammenführen von Zweigen gibt es noch nicht.

**Mehrere Dokumente und Absicherung**
- Mehrere Dokumente können gleichzeitig in Tabs offen sein. Das Fenster „Projekte“ (Menü „Ansicht“ oder Strg+O) zeigt den Baum aus Projekten, Ordnern und Dokumenten; Doppelklick öffnet, Rechtsklick bietet Anlegen, Umbenennen, Verschieben und Löschen.
- Ein Dokument kann nur von einem Programm zur gleichen Zeit geändert werden. Ist es schon gesperrt, fragt PlanoraCAD, ob du die Sperre übernehmen willst.
- Alle zwei Minuten sichert das Programm geänderte, schon gespeicherte Dokumente automatisch (das ist keine neue Version). Nach einem Absturz bietet es beim nächsten Öffnen an, diesen Stand zurückzuholen oder zu verwerfen.
- Beschädigte oder zu neue Dateien werden mit einer Meldung abgelehnt; das geöffnete Dokument bleibt dabei unverändert.

**Was (noch) nicht geht**
- Kein Zusammenführen von Zweigen, kein Teilen über das Internet, keine Vorschaubilder, die Kameraansicht wird nicht gespeichert.
- Unbenannte Dokumente (noch nie gespeichert) haben keine Autosicherung.
- Umbenennen, Verschieben und Löschen im Projektbaum geht nur, wenn die betroffenen Dokumente geschlossen sind. Verschoben wird über ein Auswahlfenster, nicht durch Ziehen.

**Handprüfung:** Am 11.10.2026 von Hand im echten Fenster geprüft (mit `tools/gui-drive.ps1`): Projekt anlegen, Quader anlegen, Strg+S und „Speichern unter“, zweite Version mit Kommentar, Versionen-Panel, alte Version öffnen (schreibgeschützt, Menüs ausgegraut), Wiederherstellen, Fenster-× mit ungespeicherten Änderungen (Rückfrage, das Programm bleibt offen), „Speichern“ in der Rückfrage beendet das Programm und gibt die Sperre frei, Neustart und Öffnen aus dem Projekt-Panel (der Stand ist vollständig da), × ohne Änderungen schließt sofort. Nicht von Hand geprüft (nur durch automatische Tests ohne Fenster): Abzweigen mit Zweignamen-Fenster, Zweig oder Version löschen, Kommentar ändern, Ordner anlegen, umbenennen, verschieben und löschen im Projekt-Panel, Autosicherungs-Angebot nach einem Absturz, Sperre übernehmen, Schließen bei minimiertem Fenster, mehrere Tabs mit mehreren Dokumenten.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 6 – Speichern und Projekte“.

Lizenz: GPL-3.0-only (siehe `LICENSE`).

## Neues Aussehen (Graphit)

Das Programm hat ein neues, ruhigeres Aussehen bekommen: dunkelgraue Flächen, dünne Ränder, Farbe nur dort, wo sie etwas bedeutet (eingeschalteter Modus, gewählte Teile, Tastaturfokus).

- Hell oder Dunkel: Du kannst für jedes Dokument zwischen hellem und dunklem Aussehen wählen. Das Papier der Zeichnung bleibt in beiden Fällen weiß.
- Neue Schrift: Montserrat in drei Stärken, überall im Programm.
- Neue Symbole: einheitliche Linien-Symbole in den Menüs und Werkzeugleisten, dazu passend gezeichnete Symbole für die Skizzenwerkzeuge.
- Einheitliche Abstände, Ränder und Rundungen in allen Fenstern.

**Neuer Aufbau des Fensters (Modell-Ansicht):** Die Felder schweben jetzt frei über dem 3D-Bereich, der das ganze Fenster füllt.

- Oben in der Mitte: Menü und Tabs.
- Links: ein Feld mit deinen Projekten und darunter der Baum mit den Arbeitsschritten.
- Rechts: Eigenschaften und Versionen. Ein Umschalter im Feld wechselt zwischen beiden.
- Unten: das Befehlsfeld mit Abkürzungen zu den Menüeinträgen, daneben Meldungen.
- Die Felder lassen sich nicht verschieben oder andocken. Projekte und Versionen blendest du im Menü Ansicht ein oder aus.
- Klicken, Ziehen und das Mausrad auf einem Feld bewegen das Modell nicht. Dialoge liegen immer über den Feldern.
- „Alles einpassen“ zentriert das Modell in dem Bereich, der zwischen den Feldern frei bleibt.
- Das Fenster ist mindestens 800 × 600 groß. Kleiner lässt es sich nicht ziehen.

**Noch nicht neu:** Die Leiste der Zeichnung, das Fenster „Skizze“ und die Dialoge sehen noch aus wie vorher. Sie folgen in den nächsten Etappen (Zeichnung und Skizze in Etappe C, die Dialoge in Etappe D). Bis dahin kann die Zeichnungsleiste in einem schmalen Fenster auf mehrere Zeilen umbrechen, und das Fenster „Skizze“ kann Teile der unteren Felder überdecken.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 13“.

## Bauen

    cargo run -p planora-app --release

Unter Linux werden zusätzlich die System-Bibliotheken aus `.github/workflows/ci.yml` benötigt.

## Entwicklung

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check

## Drittanbieter
- Schrift: Montserrat, OFL 1.1; Symbolschrift Phosphor, MIT; Ersatzschrift Roboto 2.138, Apache-2.0 (siehe `crates/planora-app/assets/fonts/`).
