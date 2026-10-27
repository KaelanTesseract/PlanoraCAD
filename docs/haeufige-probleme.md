# Häufige Probleme

## Windows zeigt beim ersten Start eine Warnung

Der Installer ist nicht signiert. Darum kann Windows, wenn du `planoracad-setup-x64.exe` startest, eine blaue Warnung zeigen („SmartScreen“). Klick auf „Weitere Informationen“ und dann auf „Trotzdem ausführen“.

Die Prüfsumme, mit der das Programm Updates kontrolliert, schützt nur vor Fehlern bei der Übertragung. Lade das Programm nur von der [Seite der Veröffentlichungen](https://github.com/KaelanTesseract/PlanoraCAD/releases) im Repo.

## Die 3D-Ansicht zeigt Fehler oder das Programm läuft nicht richtig

Aktualisiere den Treiber deiner Grafikkarte. Die Treiber gibt es bei den Herstellern der Karte. Starte das Programm danach neu. Hilft das nicht, melde es bitte (siehe [So meldest du etwas](so-meldest-du-etwas.md)). Schreib dazu, welche Grafikkarte du hast.

## Das Virenprogramm hält die Datei fest

Manche Virenprogramme prüfen eine neue Programmdatei erst eine Weile oder halten sie an. Warte, bis die Prüfung fertig ist, und starte das Programm dann noch einmal.

## Das Update klappt nicht

1. Hast du die Frage von Windows nach Administratorrechten abgelehnt (bei „Für alle Benutzer“)? Dann läuft die bisherige Version einfach weiter. Starte das Update noch einmal und antworte mit „Ja“.
2. Zeigt das Programm nur den Knopf „Download-Seite öffnen“? Dann wurde es nicht mit dem Installer installiert, zum Beispiel direkt aus dem Bauordner gestartet. Lade `planoracad-setup-x64.exe` von der [Seite der Veröffentlichungen](https://github.com/KaelanTesseract/PlanoraCAD/releases) und installiere es.
3. Klappt es trotzdem nicht? Das Programm schreibt den Verlauf in die Datei `planoracad-update.log` im Temp-Ordner. Drück die Windows-Taste und R, tipp `%TEMP%` ein und such die Datei. Lade danach den neuen Installer von der Release-Seite und führ ihn selbst aus. Er ersetzt die alte Version; deine Projekte und Einstellungen bleiben erhalten.

Unter Linux und macOS gibt es keine Updates über das Programm. Dort lädst du neue Versionen von Hand herunter.

## Etwas anderes?

Dann melde es bitte: [So meldest du etwas](so-meldest-du-etwas.md).
