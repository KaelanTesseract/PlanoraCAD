# Projekttafel einrichten

Die Projekttafel zeigt, was geplant ist, woran gerade gearbeitet wird und was fertig ist. Sie hat drei Spalten: **Geplant**, **In Arbeit** und **Fertig**. Neue Meldungen landen von selbst in der Tafel.

Du brauchst das nur einmal. Es gibt zwei Wege. Nimm einen davon:

- **Weg A** geht ganz im Browser auf GitHub.
- **Weg B** nutzt das Programm `gh` in der Eingabeaufforderung. Zwei Schritte gehen auch dort nur im Browser.

## Weg A: im Browser

### Tafel anlegen

1. Öffne <https://github.com/KaelanTesseract> im Browser und melde dich an.
2. Klick oben auf den Reiter „Projects“.
3. Klick auf „New project“.
4. Wähl die Vorlage „Board“.
5. Gib als Namen `PlanoraCAD – Wünsche und Fehler` ein.
6. Klick auf „Create project“.

### Spalten einstellen

1. Die Tafel hat drei Spalten mit englischen Namen („Todo“, „In Progress“, „Done“). Klick bei der ersten Spalte auf die drei Punkte.
2. Wähl „Edit details“.
3. Schreib `Geplant` als Namen und speichere.
4. Mach dasselbe bei der zweiten Spalte mit dem Namen `In Arbeit`.
5. Mach dasselbe bei der dritten Spalte mit dem Namen `Fertig`.
6. Prüf, dass die Namen genau so geschrieben sind. Es darf keine weitere Spalte da sein.

### Neue Meldungen automatisch hinzufügen

1. Klick oben rechts in der Tafel auf die drei Punkte.
2. Wähl „Workflows“.
3. Klick links auf „Auto-add to project“.
4. Wähl als Repository `KaelanTesseract/PlanoraCAD`.
5. Schreib in das Filterfeld `is:issue`.
6. Schalte den Ablauf ein („Enable“) und speichere.

### Tafel mit dem Repository verknüpfen

1. Öffne <https://github.com/KaelanTesseract/PlanoraCAD>.
2. Klick auf den Reiter „Projects“.
3. Klick auf „Link a project“.
4. Wähl die Tafel `PlanoraCAD – Wünsche und Fehler`.

### Tafel öffentlich machen

1. Öffne die Tafel und klick oben rechts auf die drei Punkte.
2. Wähl „Settings“.
3. Such den Abschnitt „Visibility“.
4. Wähl „Public“ und speichere.

### Link ins README eintragen

1. Kopier die Adresse der Tafel aus der Adresszeile des Browsers.
2. Öffne `README.md`.
3. Such den Platzhalter `[Link zur Projekttafel – nach dem Anlegen eintragen]`.
4. Ersetz ihn durch die kopierte Adresse.

## Weg B: mit `gh`

`gh` ist das GitHub-Programm für die Eingabeaufforderung. Die Befehle unten legen die Tafel an, verknüpfen sie und machen sie öffentlich.

### Erlaubnis erteilen (das machst du selbst)

1. Öffne die Eingabeaufforderung oder PowerShell.
2. Gib ein: `gh auth refresh -s project`
3. Folge den Hinweisen im Fenster. Ohne diese Erlaubnis geht `gh project` nicht.

### Tafel anlegen

1. Gib ein: `gh project create --owner KaelanTesseract --title "PlanoraCAD – Wünsche und Fehler"`
2. Notier dir die Nummer der Tafel aus der Antwort. Unten steht sie als `NUMMER`.

### Tafel verknüpfen

1. Gib ein: `gh project link NUMMER --owner KaelanTesseract --repo KaelanTesseract/PlanoraCAD`

### Tafel öffentlich machen

1. Gib ein: `gh project edit NUMMER --owner KaelanTesseract --visibility PUBLIC`

### Spalten und automatisches Hinzufügen (nur im Browser)

`gh` kann die Spalten einer neuen Tafel nicht umbenennen und den Ablauf „Auto-add to project“ nicht einschalten. Mach diese zwei Dinge im Browser:

1. Geh zu Weg A, Abschnitt „Spalten einstellen“, und führe ihn aus.
2. Geh zu Weg A, Abschnitt „Neue Meldungen automatisch hinzufügen“, und führe ihn aus.

### Link ins README eintragen

1. Gib ein: `gh project view NUMMER --owner KaelanTesseract --web`
2. Kopier die Adresse aus der Adresszeile des Browsers.
3. Ersetz in `README.md` den Platzhalter `[Link zur Projekttafel – nach dem Anlegen eintragen]` durch diese Adresse.

## Prüfen

1. Leg zum Test eine Meldung an (siehe [So meldest du etwas](so-meldest-du-etwas.md)).
2. Öffne die Tafel. Die Meldung soll nach kurzer Zeit auftauchen.
3. Öffne die Tafel in einem privaten Browserfenster ohne Anmeldung. Du siehst sie, wenn sie öffentlich ist.
