# Änderungen

Hier steht, was sich in jeder Version geändert hat. Die neueste Version steht oben.

## 0.0.2-dev.2

### Neu
- PlanoraCAD spricht jetzt Deutsch und Englisch. Beim ersten Start nimmt das Programm die Sprache von Windows; gibt es sie nicht, ist es englisch. Unter „Datei → Einstellungen …“ lässt sich die Sprache wechseln, alle Texte wechseln sofort mit. Auf einem deutschen Windows bleibt alles wie bisher.
- Zahlen erscheinen in der Sprache des Programms: deutsch mit Komma (38,8), englisch mit Punkt (38.8). Eintippen geht in beiden Sprachen mit Komma und mit Punkt.
- Beim Exportieren einer Zeichnung kannst du eine andere Sprache für die Zeichnung wählen, zum Beispiel eine englische Zeichnung aus dem deutschen Programm.
- Weitere Sprachen lassen sich ohne Programmieren hinzufügen: eine Textdatei je Sprache in den Ordner `languages` legen. Wie das geht, steht auf Englisch in `docs/uebersetzen.md`.
- Rückmeldung und Absturzbericht: Das Formular und die Vorlage öffnen sich in der Sprache des Programms. Protokoll und Absturzbericht sind immer englisch.

## 0.0.2-dev.1

Das ist die erste Entwicklerversion mit Update-Funktion. Sie ist noch nicht für den Alltag gedacht.

### Neu
- Das Programm kann sich selbst aktualisieren. Beim Start schaut es im Hintergrund nach, ob es eine neuere Version gibt, und zeigt dann ein Fenster mit den Änderungen. Du entscheidest, ob du jetzt aktualisierst, später erinnert werden willst oder diese Version überspringst. Vor dem Neustart fragt das Programm wie beim Schließen, ob ungespeicherte Arbeit gespeichert werden soll.
- Unter „Datei → Einstellungen …“ gibt es einen Abschnitt für Updates. Dort lässt sich die Prüfung ausschalten oder von Hand starten.
- Es gibt drei Kanäle: Dev, Beta und Stabil. Im Moment gibt es nur Dev-Versionen.
- PlanoraCAD kommt jetzt als Installer (`planoracad-setup-x64.exe`). Du wählst „Nur für mich“ (ohne Administratorrechte) oder „Für alle Benutzer“, bekommst einen Eintrag im Startmenü und kannst eine Verknüpfung auf dem Desktop anlegen. Deinstallieren geht über „Einstellungen → Apps“ in Windows; deine Projekte, Einstellungen und Protokolle bleiben dabei erhalten. Ein Update lädt den neuen Installer, prüft ihn mit einer Prüfsumme und startet ihn im Hintergrund. Bei „Für alle Benutzer“ fragt Windows dabei nach Administratorrechten; lehnst du ab, läuft die bisherige Version weiter.
- Zu jeder Veröffentlichung gehört eine Datei mit den Lizenzen aller verwendeten Bibliotheken und Schriften.
- Verlaufsleiste unten im Fenster: Sie zeigt die Schritte des gewählten Körpers. Mit dem Marker lässt sich zu einem früheren Schritt zurückgehen, ohne etwas zu löschen.
- Tastenkürzel wie in gängigen CAD-Programmen, zum Beispiel E zum Extrudieren, F zum Verrunden und V zum Ein- und Ausblenden. Alle Kürzel lassen sich in den Einstellungen ändern. „Ansicht → Tastenkürzel …“ zeigt eine Übersicht mit Suche.
- Rückmeldung aus dem Programm: „Hilfe → Rückmeldung …“ (oder Strg+Umschalt+R) öffnet ein Fenster. Du wählst Fehler, Wunsch oder Frage und schreibst kurz, was passiert ist. Darunter steht jede Angabe, die das Programm dazulegt (Version, System, Grafikkarte, letzte Schritte, auf Wunsch Protokollzeilen und ein Bild vom Programmfenster), mit Häkchen und Vorschau; Benutzername, Projektpfade und Mailadressen sind schon entfernt. „Im Browser öffnen“ öffnet ein ausgefülltes Formular auf GitHub. Abgeschickt wird erst, wenn du dort selbst auf den Knopf klickst.
- Rückmeldung ohne GitHub-Konto: Der Knopf „Ohne GitHub-Konto senden“ öffnet ein ausgefülltes Formular auf dennis-guse.de. Du brauchst dafür kein Konto. Die Angaben gehen mit dem Öffnen des Formulars an dennis-guse.de (sie stecken in der Adresse und können dort im Protokoll des Servers stehen); gemeldet wird aber erst, wenn du dort auf „Absenden“ klickst. Bilder und Dateien hängst du dort an; das Programm öffnet dafür einen Ordner mit den fertigen Dateien. Nur wenn der Browser sich gar nicht öffnen lässt, bleibt die Mail an info@dennis-guse.de.
- Absturzbericht: Stürzt das Programm ab, schreibt es vorher einen kurzen Bericht. Der Bericht bleibt auf deinem Rechner und enthält deinen Benutzernamen nicht. Beim nächsten Start fragt das Programm, ob du ihn als Rückmeldung schicken möchtest. Du kannst ihn vorher ansehen, später entscheiden oder löschen. Auf Wunsch kommt eine Kopie des offenen Projekts als ZIP-Datei (`.pcad.zip`) mit in den Rückmeldungsordner; für das Formular darf sie höchstens 25 MB groß sein. Der Bericht enthält auch Adressen aus dem Programm, mit denen sich der Fehler später in der Programmdatei genau finden lässt. Alte Berichte räumt das Programm nach 30 Tagen selbst weg. Endet das Programm ohne Bericht unerwartet, weist es beim nächsten Start kurz darauf hin.
- Ausführliches Protokoll: Unter „Datei → Einstellungen …“, Abschnitt „Updates“, gibt es den Schalter „Ausführliches Protokoll“. Ist er an, schreibt das Programm mehr in die Protokolldatei auf deinem Rechner, zum Beispiel jeden Befehl und wie lange er dauert. Projektinhalte stehen nicht drin, und es wird nichts gesendet. Der Schalter bleibt nach einem Neustart erhalten.
- Nach einem Update zeigt das Programm „Aktualisiert auf …“ mit dem Knopf „Rückmeldung geben“.
- Zu jeder Veröffentlichung gehört jetzt auch die Datei `planoracad-windows-x64.pdb`. Du brauchst sie nicht. Sie hilft nur dabei, Absturzberichte zu lesen, und das Programm lädt sie nie herunter.
- Neues Menü „Hilfe“ mit „Rückmeldung …“, „Häufige Probleme“ und „Über PlanoraCAD“ (stand bisher im Menü „Datei“).
- Skizzen mit Abhängigkeiten und Maßen, Freiformflächen aus Ellipsen und Splines, Rundungen und Fasen, Boolesche Operationen (Vereinigen, Abziehen, Schneiden).
- Feature-Baum: Alles, was du baust, steht als Liste von Schritten. Ändert sich ein Maß, rechnet das Programm die folgenden Schritte neu.
- Parameter und Formeln: Maße bekommen einen Namen und lassen sich ausrechnen.
- Speichern in Projekten und Ordnern, mit Versionen und Zweigen. Rückgängig und Wiederholen für alle Änderungen.
- Austausch mit anderen Programmen: Netze und STEP-Dateien lassen sich ein- und ausgeben.
- Zeichnungen mit Ansichten, Schnittansichten, Maßen, Mittellinien, Stückliste und Positionsnummern. Eine Zeichnung kann als PDF ausgegeben werden.

### Verbessert
- Neues, ruhigeres Aussehen („Graphit“): helles oder dunkles Aussehen, neue Schrift, einheitliche Symbole. Die Felder schweben frei über der 3D-Ansicht.
- Das Programm merkt sich, ob du das helle oder das dunkle Aussehen gewählt hast.
- Eingabefelder zeigen im hellen Aussehen deine Eingabe jetzt in kräftiger Schrift, nicht mehr in blassem Grau.
- Das Fenster „Projekte“ zeichnet sich nach dem Laden sofort neu, ohne dass man die Maus bewegen muss.

### Bekannte Grenzen
- Updates gibt es nur unter Windows.
- Stürzt das Programm ab, ohne dass Rust es merkt (zum Beispiel wenn Windows es hart beendet), gibt es keinen Absturzbericht. Das Programm weist beim nächsten Start nur darauf hin.
- Wird Windows bei laufendem Programm abgemeldet oder heruntergefahren, kann beim nächsten Start „unerwartet beendet“ erscheinen (nicht geprüft).
- Für die Meldung ohne GitHub-Konto brauchst du einen Browser. Das Formular nimmt höchstens 10 Dateien, je 25 MB, zusammen 30 MB.
- Jede Meldung auf GitHub ist öffentlich.
- Manche Lagen bei Freiformflächen und bei Körpern, die sich nur berühren, kann das Programm noch nicht rechnen. Es nennt dann den Grund, statt ein falsches Ergebnis zu zeigen.
- Die ausführliche Liste steht in `docs/superpowers/offene-punkte.md`.
