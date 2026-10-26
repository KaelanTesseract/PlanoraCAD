# Änderungen

Hier steht, was sich in jeder Version geändert hat. Die neueste Version steht oben.

## 0.2.0-dev.1

Das ist die erste Entwicklerversion mit Update-Funktion. Sie ist noch nicht für den Alltag gedacht.

### Neu
- Das Programm kann sich selbst aktualisieren. Beim Start schaut es im Hintergrund nach, ob es eine neuere Version gibt, und zeigt dann ein Fenster mit den Änderungen. Du entscheidest, ob du jetzt aktualisierst, später erinnert werden willst oder diese Version überspringst. Vor dem Neustart fragt das Programm wie beim Schließen, ob ungespeicherte Arbeit gespeichert werden soll.
- Unter „Datei → Einstellungen …“ gibt es einen Abschnitt für Updates. Dort lässt sich die Prüfung ausschalten oder von Hand starten.
- Es gibt drei Kanäle: Dev, Beta und Stabil. Im Moment gibt es nur Dev-Versionen.
- Der Download wird vor dem Austausch mit einer Prüfsumme kontrolliert. Geht beim Austausch etwas schief, legt das Programm die alte Version wieder zurück.
- Zu jeder Veröffentlichung gehört eine Datei mit den Lizenzen aller verwendeten Bibliotheken und Schriften.
- Verlaufsleiste unten im Fenster: Sie zeigt die Schritte des gewählten Körpers. Mit dem Marker lässt sich zu einem früheren Schritt zurückgehen, ohne etwas zu löschen.
- Tastenkürzel wie in Fusion, zum Beispiel E zum Extrudieren, F zum Verrunden und V zum Ein- und Ausblenden. Alle Kürzel lassen sich in den Einstellungen ändern. „Ansicht → Tastenkürzel …“ zeigt eine Übersicht mit Suche.
- Rückmeldung aus dem Programm: „Hilfe → Rückmeldung …“ (oder Strg+Umschalt+R) öffnet ein Fenster. Du wählst Fehler, Wunsch oder Frage und schreibst kurz, was passiert ist. Darunter steht jede Angabe, die das Programm dazulegt (Version, System, Grafikkarte, letzte Schritte, auf Wunsch Protokollzeilen), mit Häkchen und Vorschau; Benutzername, Projektpfade und Mailadressen sind schon entfernt. „Im Browser öffnen“ öffnet ein ausgefülltes Formular auf GitHub. Abgeschickt wird erst, wenn du dort selbst auf den Knopf klickst.
- Absturzbericht: Stürzt das Programm ab, schreibt es vorher einen kurzen Bericht. Der Bericht bleibt auf deinem Rechner. Beim nächsten Start fragt das Programm, ob du ihn als Rückmeldung schicken möchtest. Du kannst ihn vorher ansehen, später entscheiden oder löschen. Auf Wunsch kommt eine Kopie des offenen Projekts mit in den Rückmeldungsordner. Alte Berichte räumt das Programm nach 30 Tagen selbst weg.
- Neues Menü „Hilfe“ mit „Rückmeldung …“, „Häufige Probleme“ und „Über PlanoraCAD“ (stand bisher im Menü „Datei“).
- Skizzen mit Abhängigkeiten und Maßen, Freiformflächen aus Ellipsen und Splines, Rundungen und Fasen, Boolesche Operationen (Vereinigen, Abziehen, Schneiden).
- Feature-Baum: Alles, was du baust, steht als Liste von Schritten. Ändert sich ein Maß, rechnet das Programm die folgenden Schritte neu.
- Parameter und Formeln: Maße bekommen einen Namen und lassen sich ausrechnen.
- Speichern in Projekten und Ordnern, mit Versionen und Zweigen. Rückgängig und Wiederholen für alle Änderungen.
- Austausch mit anderen Programmen: Netze und STEP-Dateien lassen sich ein- und ausgeben.
- Zeichnungen mit Ansichten, Schnittansichten, Maßen, Mittellinien, Stückliste und Positionsnummern. Eine Zeichnung kann als PDF ausgegeben werden.

### Verbessert
- Neues, ruhigeres Aussehen („Graphit“): helles oder dunkles Aussehen, neue Schrift, einheitliche Symbole. Die Felder schweben frei über der 3D-Ansicht.

### Bekannte Grenzen
- Updates gibt es nur unter Windows.
- Manche Lagen bei Freiformflächen und bei Körpern, die sich nur berühren, kann das Programm noch nicht rechnen. Es nennt dann den Grund, statt ein falsches Ergebnis zu zeigen.
- Die ausführliche Liste steht in `docs/superpowers/offene-punkte.md`.
