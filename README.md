# PlanoraCAD

Ein quelloffenes, parametrisches 3D-CAD in reinem Rust für Windows und Linux.

**Status:** frühe Entwicklung (Fundament, Kernel-Basis, Boolesche Operationen, Skizzen mit Constraint-Solver, Feature-Baum, Speichern mit Projekten und Versionen, Freiformflächen).

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

## Bauen

    cargo run -p planora-app --release

Unter Linux werden zusätzlich die System-Bibliotheken aus `.github/workflows/ci.yml` benötigt.

## Entwicklung

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check

## Drittanbieter
- Schrift: Roboto 2.138, Apache-2.0 (siehe `crates/planora-app/assets/fonts/`).
