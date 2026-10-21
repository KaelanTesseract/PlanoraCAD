# Benannte Parameter und Gleichungen – Entwurf

Stand 2026-10-20. Nächstes Teilprojekt nach Rückgängig/Wiederholen. **Alle Werte unten sind Vorschläge mit Standardwerten**; Dennis korrigiert, was nicht passt. Beschlossen ist bisher nur: Benannte Parameter und Gleichungen sind das nächste Teilprojekt.

## 1. Zweck

Heute ist jedes Maß nur eine Zahl. Wer ein Teil später ändern will („Breite 40 statt 30, das Loch immer ein Viertel der Breite“), muss jede Zahl einzeln anfassen. Mit Parametern legt man Namen fest („Breite = 40“) und schreibt in Maßfelder Formeln („Breite / 4“). Ändert man den Parameter, rechnet das ganze Modell neu.

## 2. Umfang (Vorschlag)

**Ziele:**
- **Parametertabelle je Dokument:** Name, Formel (oder Zahl), Wert (berechnet), Art, Kommentar.
- **Arten:** „Länge“ (mm), „Winkel“ (Grad), „Zahl“ (ohne Einheit). Ein Winkelparameter lässt sich nicht in ein Längenfeld einsetzen und umgekehrt (Meldung); eine Zahl lässt sich überall einsetzen (zum Beispiel Faktor in einer Formel).
- **Formeln:** Zahlen (Komma oder Punkt), Namen anderer Parameter, `+ − * / ^`, Klammern, Funktionen `sqrt`, `sin`, `cos`, `tan`, `abs`, `min`, `max`, `round`, Konstante `pi`. Winkel in Sinus und Kosinus in Grad. Beispiel: `Breite / 4`, `sqrt(a^2 + b^2)`, `min(Breite, Tiefe) - 2*Wand`.
- **Wo man Formeln eintippen kann:** in **allen Zahlenfeldern der Körper-Funktionen** (Quader, Zylinder, Kugel, Kegel: Größen; Extrusion: Höhe; Verschieben: Position/Versatz; Verrunden und Fasen: Radius/Abstände) und in den **Maßen der Skizze** (Abstand, Abstand Punkt–Linie, Radius, Durchmesser, Winkel, Ellipsenachsen/-winkel).
- **Anzeige:** Ein Feld mit Formel zeigt den Text der Formel und daneben „= 20,00 mm“ (der berechnete Wert); ein Zeichen „fx“ markiert Formelfelder. Falsche Formeln werden rot mit einer klaren Meldung erklärt („Der Name „Breit“ gibt es nicht.“).
- **Änderung eines Parameters** rechnet alle abhängigen Features und Skizzen neu (wie heute beim Ändern eines Maßes), als **ein** Schritt im Verlauf („Parameter ändern“).
- **Löschen eines Parameters:** gesperrt, solange er benutzt wird; die Meldung nennt, wo („Breite wird in Quader 1, Skizze 2 benutzt.“) und bietet „Durch den Wert ersetzen“ an (setzt die berechnete Zahl in alle Felder und löscht den Parameter).
- **Umbenennen eines Parameters** zieht alle Formeln mit (der Name wird in den Formeln ersetzt).
- **Zyklen** („a = b + 1“, „b = a“) werden abgelehnt.
- **Rückgängig/Wiederholen** gilt für alle Parameteränderungen (sie sind Teil des Dokuments).
- **Datei:** neues Dateiformat 9, geschrieben nur, wenn das Dokument Parameter oder Formeln enthält; ältere Dateien laden unverändert und Dateien ohne Parameter bleiben Byte für Byte wie bisher.

**Nicht-Ziele (Vorschlag):** Zeichnungsmaße (Maße in Zeichnungen messen das Modell, sie treiben nichts); Blattangaben und Ansichtsmaßstab; Textparameter; Wenn-dann-Formeln; eigene Funktionen; Einheitenrechnung (zum Beispiel „Zoll“ in Formeln); Parameter, die zwischen Dokumenten geteilt werden; Import aus Tabellen; Gruppen oder Sortierung der Tabelle; Auswahlliste mit Namen in jedem Feld (zuerst nur ein Hinweis „Namen: Breite, Tiefe …“ im Dialog).

**Anti-Ziel:** Ein Modell, das nach einer Parameteränderung still falsch ist. Eine ungültige Formel wird abgelehnt (das Modell bleibt unverändert); führt eine gültige Formel zu einem Modellfehler (Länge ≤ 0, Skizze nicht lösbar, Operation scheitert), zeigt das Feature den Fehlerstatus mit deutscher Meldung, und Rückgängig stellt zurück. Nie eine stille falsche Skizze oder ein stilles anderes Modell (siehe §4a.3).

## 3. Bedienung (Vorschlag)

- **Fenster „Parameter“** über Menü „Bearbeiten → Parameter …“ (und Symbolknopf in der Leiste später); die Tabelle hat je Zeile Name, Formel, Wert, Art, Kommentar; „Zeile hinzufügen“, „Zeile löschen“, Enter übernimmt eine Zeile; Änderungen wirken bei Enter oder Verlassen der Zeile (ein Schritt je Änderung).
- **Namensregeln:** beginnt mit Buchstabe oder `_`, danach Buchstaben, Ziffern, `_`; keine Umlaute im ersten Wurf? (Vorschlag: Umlaute und ß sind erlaubt, weil Dennis deutsch arbeitet); nicht gleich einer Funktion oder Konstante (`sin`, `pi` …); eindeutig ohne Rücksicht auf Groß-/Kleinschreibung.
- **Formelfelder:** In Zahlenfeldern gilt: Tippt man eine Zahl, bleibt es eine Zahl; tippt man etwas mit Namen oder Rechenzeichen, wird es eine Formel. Ein Klick auf „fx“ im Feld wandelt die Formel in ihren Wert (Formel löschen).
- **Skizze:** Beim Setzen oder Ändern eines Maßes im Dialog der Skizze gilt dasselbe; ein Maß mit Formel steht in der Maßliste mit „fx“.
- **Maßzeichen im Zeichenbereich der Skizze:** zeigen den Wert (nicht die Formel).

## 4. Technischer Ansatz (Vorschlag, im Plan zu prüfen)

- **Reiner Formelbaustein** (`planora-feature`, ohne Oberfläche): Zerlegen (Lexer/Parser) eines kleinen Ausdruckskerns, Auswerten mit Parametertabelle, Abhängigkeitsgraph und Zykluserkennung, Namensersetzung beim Umbenennen. Kein neuer Fremdcode.
- **Feature-Ebene:** Jedes numerische Feld eines Features kann neben dem Wert einen Ausdruck tragen (`Param<f64>` mit `value` und optional `expr`); beim Neuberechnen werden zuerst die Parameter ausgewertet und dann die Ausdrücke der Features; Fehler zählen wie heutige Prüffehler des Features. Skizzen: Ausdrücke pro Maß liegen in `SketchFeature` (Zuordnung Abhängigkeits-ID → Ausdruck), der Skizzenbaustein selbst bleibt unverändert und bekommt nur die berechneten Werte.
- **Dokument:** `Document` hält die Parametertabelle (Reihenfolge fest, eindeutige IDs); öffentliche Änderungen zählen in `edit_count` und sind im `Snapshot` (für Speichern, Laden, Rückgängig).
- **Rückgängig:** Parameteränderungen kommen über den Snapshot automatisch in den Verlauf; neuer Schrittname „Parameter ändern/anlegen/löschen“ in `undo_labels`.
- **Kein Eingriff in Kernel, Netz, STEP, Austausch.**

## 4a. Festlegungen aus der Vorabprüfung (meine Entscheidungen, Dennis darf widersprechen)

1. **Formeln als Nebentabelle, nicht im Wertfeld:** Die Zahlenfelder der Features bleiben `f64` (zuletzt berechneter Wert); die Formeln liegen als Text in einer Tabelle je Feature (Feldschlüssel → Text) in `Feature` und `SnapshotFeature` (bei Skizzen je Abhängigkeits-ID). So bleiben die rund 177 Stellen, die Features bauen (fast alle Tests), unverändert. Beim Umbenennen eines Parameters werden die Namen in den Formeltexten Wort für Wort ersetzt.
2. **Felder mit Formelfähigkeit (erste Fassung):** Quader 6 (Größe, Ursprung), Zylinder und Kegel je 5, Kugel 4, Extrusion: Höhe, Verschieben-Dialog: Δx, Δy, Δz, Rundung: Radius, Fase: Abstände und Winkel (Winkel in Grad), Netz-Körper: Versatz und Maßstab, Skizzenmaße. **Nicht** in der ersten Fassung: die Prozentpositionen und Radien der variablen Rundung, das **Positionsfeld** des Körpers (es zeigt die abgeleitete Ecke und legt relative Verschiebungen an; Formeln gehören in den Verschieben-Dialog), Drehung (immer 360°).
3. **Fehlerweg:** Eine Formel, die selbst fehlerhaft ist (Schreibfehler, unbekannter Name, Zyklus, Typverwechslung, nicht endlich), wird **abgelehnt**: nichts ändert sich, rote Meldung am Feld oder in der Tabelle. Eine gültige Formel, deren **Ergebnis** das Modell scheitern lässt (Länge ≤ 0, Boolesche Operation geht nicht, Skizze nicht lösbar), wird wie heute behandelt: übernommen, das Feature zeigt seinen Fehlerstatus mit Meldung („Quader 1: Breite ergibt 0 mm …“), Abhängige werden übersprungen, und Rückgängig stellt zurück. Bei Skizzen mit Formeln wird der Rechenstatus des Solvers ausgewertet (Widerspruch oder keine Lösung ergibt einen Fehlerstatus statt einer stillen falschen Skizze).
4. **Verlauf:** `Snapshot` enthält die Parametertabelle (für Speichern, Laden, Rückgängig, Größenschätzung); `same_model` und `Document::restore` berücksichtigen sie; ein Parameterschritt heißt „Parameter ändern/anlegen/löschen“ und hat in `undo_labels` Vorrang vor „Ändern“; „Durch Wert ersetzen“ ist **ein** Schritt.
5. **Übernahme:** Änderungen in der Parametertabelle und in Formelfeldern wirken bei Enter oder beim Verlassen des Feldes (nicht bei jeder Taste), als ein Schritt.
6. **Offene Skizze:** Das Fenster „Parameter“ ist gesperrt (ausgegraut mit Erklärung „Beenden Sie zuerst die Skizze“), solange eine Skizze offen ist.
7. **Beim Laden:** Ist die Parametertabelle selbst ungültig (Zyklus, doppelte Namen), wird die Datei mit deutscher Meldung abgelehnt. Eine einzelne kaputte Feature-Formel lässt die Datei laden; das Feature zeigt den Fehlerstatus, das Feld behält den zuletzt gespeicherten Wert.
8. **Dateiformat 9:** `required_format` springt nur bei Parametern oder Formeln auf 9; neue Felder werden nur geschrieben, wenn sie gefüllt sind (alte Dateien Byte für Byte gleich).
9. **Bekannte Grenzen:** Eine Extrusion wählt ihr Profil über die Nummer; ändert eine Parameteränderung die Profilreihenfolge, kann ein anderes Profil gewählt werden; Flächen-Hinweispunkte können bei sehr großen Sprüngen verloren gehen; der Solver startet von der alten Lösung, große Sprünge (30 auf 400) können eine andere Lösung wählen oder nicht konvergieren (dann Fehlerstatus statt stiller Fehlform). Maße im Modus „abgeleitet“ (nur anzeigend) bekommen keine Formel.

## 5. Grenzen und Fehler (Vorschlag)

- Höchstens 500 Parameter, Formeln bis 200 Zeichen, Ausdrücke bis Tiefe 64.
- Auswertung in `f64`; nicht endliche Ergebnisse (Division durch null, Wurzel aus negativer Zahl) sind ein Fehler mit Meldung.
- Prüfungen der Felder bleiben dieselben wie heute (Länge > 0, Winkel in zulässigem Bereich …); Fehler werden dem Feld zugeordnet („Quader 1: Breite ergibt 0 mm, es muss größer als 0 sein“).
- Beim Laden: siehe §4a.7.

## 6. Tests und Prüfung

- Formelbaustein: Grammatik (Rangfolge, Klammern, unäre Minuszeichen, Potenz rechts-assoziativ), Funktionen, Komma/Punkt, Fehlerfälle (unbekannter Name, Division durch null, Zyklus, zu tief), Namensersetzung, Zahlen-Formatierung hin und zurück.
- Features: je Funktion Formel → Wert → Körper (Maße gegen Handrechnung), Parameteränderung → Folgemodell; Rückgängig stellt den alten Zustand her.
- Skizze: Maß mit Formel, Parameteränderung löst neu, unlösbar → Meldung und alter Zustand.
- Datei: Format 9 nur mit Parametern; Rundreise; alte Dateien Byte für Byte.
- Oberfläche: Fenster „Parameter“, Formelfelder mit „fx“ und Wert, Meldungen, Löschen/Ersetzen, Umbenennen; Texte; kleine Fenstergrößen; Hell/Dunkel; Tastenschutz beim Tippen; Handliste mit echtem Fenster.

## 7. Offen (Vorschläge, bitte korrigieren)

1. Sollen **alle Zahlenfelder der Körper-Funktionen und die Skizzenmaße** Formeln annehmen (nicht die Zeichnungsmaße)?
2. Reichen **drei Arten** (Länge, Winkel, Zahl) mit Prüfung gegen Verwechslung?
3. Sind die **Formelfunktionen** (`+ − * / ^`, `sqrt`, `sin`, `cos`, `tan`, `abs`, `min`, `max`, `round`, `pi`) genug?
4. **Eigenes Fenster** „Parameter“ im Menü „Bearbeiten“ (statt fester Karte am Rand)?
5. Ein benutzter Parameter lässt sich **nicht löschen**, außer man ersetzt ihn durch den Wert?

## 8. Festlegungen der Umsetzung

Stand 2026-10-21. Der Plan (`docs/superpowers/plans/2026-10-20-parameter.md`) hat die Festlegungen P1 bis P14 getroffen; hier die Kurzfassung. Abweichungen und Einzelheiten stehen im Plan, Abschnitt „Abweichungen bei der Umsetzung“, und in `docs/superpowers/offene-punkte.md`.

- **P1 Werte schreiben:** Vor jeder Neuberechnung rechnet das Dokument erst die Tabelle, dann alle Formeln aus und schreibt die Ergebnisse in die Zahlenfelder. Nur Features mit neuem Wert oder Formelfehler bekommen eine neue Version.
- **P2 Zahl statt Formel:** Tippt man in ein Feld mit Formel eine Zahl, entfällt nur diese Formel (auch bei Skizzenmaßen). Formeln für Felder, die es nach einer Änderung der Art nicht mehr gibt, entfallen.
- **P3 Strenge Prüfung, milder Ladeweg:** Eine Formel, die selbst kaputt ist, wird abgelehnt. Eine Formel mit einem Ergebnis außerhalb des Bereichs wird übernommen, das Feature zeigt den Fehlerstatus (das Feld trägt den Wert der Formel). Beim Laden lädt auch eine kaputte Feature-Formel (Fehlerstatus, gespeicherter Wert).
- **P4 Einheiten:** Länge, Winkel, Zahl. Eine reine Zahl passt überall; Länge mit Winkel nie; ein Feld nimmt seine Einheit oder eine reine Zahl.
- **P5 Komma:** Zwischen zwei Ziffern ist es das Dezimalkomma. Werte trennt man mit „;“ oder „, “.
- **P6 Namen:** ohne Rücksicht auf Groß- und Kleinschreibung; Umbenennen ersetzt ganze Namen in den Formeln; Namen bis 64, Kommentare bis 200 Zeichen.
- **P7 Durch den Wert ersetzen:** Direkte Benutzer bekommen den Wert, dann wird der Parameter gelöscht; ein Schritt. Abgelehnt wird, wenn ein Benutzer gerade einen Fehler hat, der sonst still ein anderes Modell ergäbe (Skizzen streng, Körperfelder nur bei Wertfehler).
- **P8 Strg+Z:** wirkt auch bei offenem Fenster „Parameter“; das Fenster lädt danach neu.
- **P9 Übernahme:** In Fenstern mit Knopf erst mit dem Knopf; im Fenster „Parameter“ und in den Netz-Formelfeldern bei Enter oder Verlassen.
- **P10 Netz-Körper:** Position und Maßstab per Tippen; die Drehung bleibt Ziehen.
- **P11 Verschieben mit Formeln:** wird nie erweitert; ein neues Verschieben entsteht.
- **P12 Skizzenstatus:** Eine Skizze mit Formeln ist im Fehlerstatus, wenn der Löser nicht „gelöst“ meldet.
- **P13 Neue Zeile:** „ParameterN“, Formel „10“, Länge.
- **P14 Meldungen:** „Den Namen „Breit“ gibt es nicht.“

Zur Frage 7 (Offen): Formeln gehen in allen Zahlenfeldern der Körper-Funktionen (außer variabler Rundung, Position des Körpers, Drehung) und in den bestimmenden Skizzenmaßen, nicht in Zeichnungsmaßen; drei Arten genügen; die Funktionen sind wie unter §2; das Fenster „Parameter“ liegt im Menü „Bearbeiten“; ein benutzter Parameter lässt sich nur mit „Durch den Wert ersetzen“ entfernen.
