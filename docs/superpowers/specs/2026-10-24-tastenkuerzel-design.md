# Tastenkürzel wie in Fusion, änderbar – Entwurf

Stand: 2026-10-24. Entwurf zur Prüfung durch Dennis. Alles unter „Mein Vorschlag“ ist eine Annahme von Claude, nicht vom Nutzer entschieden.

## Ziel

PlanoraCAD lässt sich mit den Tasten und der Maus bedienen wie Fusion. Alle Kürzel sind ab Werk wie bei Fusion. Jede Belegung lässt sich in den Einstellungen ändern. Eine Übersicht mit Suche zeigt alle Kürzel zum Nachschlagen.

Grund: Fusion ist der Maßstab für die Bedienung (Feature-Baum und Verlaufsleiste stammen schon von dort). Gleiche Tasten sparen Umgewöhnung. Änderbar muss es sein, weil nicht jeder die Fusion-Belegung mag oder eine Taste anderweitig braucht.

## Entschieden (vom Nutzer)

1. Alles ist ab Werk wie in Fusion, auch die Maussteuerung.
2. Das Drehen der Ansicht mit der linken Maustaste bleibt als Zusatz erhalten.
3. Alle Kürzel sind in den Einstellungen änderbar.
4. Es gibt eine Übersicht zum Nachschlagen mit Suche.
5. Bei jeder künftigen Funktion wird geprüft, ob Fusion dafür ein Kürzel hat; es wird in die zentrale Tabelle eingetragen (Gedächtnis: feedback-fusion-hotkeys).
6. Umfang der Fusion-Liste: Modellieren (Design), Skizze, Ansicht (Canvas), Zeichnung, System. Nicht: Formen (T-Splines), Generatives Design, Rendern, Animation, Simulation, Fertigung, Elektronik.

## Fusion-Vorgaben (Windows)

Quelle der Belegung (als Tatsachen, keine Texte oder Grafiken von Autodesk übernommen): die Liste auf productdesignonline.com, Abgleich mit der Autodesk-Seite nicht möglich (403, Tabelle wird nachgeladen). Der Bauer prüft die Liste vor dem Eintragen gegen die offizielle Autodesk-Seite im Browser, soweit lesbar, und meldet Abweichungen.

| Bereich | Aktion | Taste |
|---|---|---|
| Modell | Extrudieren | E |
| Modell | Bohrung | H |
| Modell | Verrunden | F |
| Modell | Verschieben | M |
| Modell | Drücken/Ziehen | Q |
| Modell | Messen | I |
| Modell | Sichtbarkeit umschalten | V |
| Modell | Erscheinungsbild | A |
| Modell | Löschen | Entf |
| Modell | Letzten Befehl wiederholen | Leertaste |
| Modell | Alles neu berechnen | Strg+B |
| Modell | Fenster-/Freiform-/Pinsel-Auswahl | 1 / 2 / 3 |
| Skizze | Linie | L |
| Skizze | 2-Punkt-Rechteck | R |
| Skizze | Kreis (Mittelpunkt, Durchmesser) | C |
| Skizze | Versatz | O |
| Skizze | Projizieren | P |
| Skizze | Stutzen | T |
| Skizze | Bemaßung | D |
| Skizze | Normal / Hilfslinie | X |
| Zeichnung | Ballon | B |
| Zeichnung | Mittelpunktmarke | C |
| Zeichnung | Bemaßung | D |
| Zeichnung | Verschieben | M |
| Zeichnung | Projizierte Ansicht | P |
| Zeichnung | Text | T |
| Zeichnung | Löschen | Entf |
| Alle | Kopieren / Ausschneiden / Einfügen | Strg+C / X / V |
| Alle | Rückgängig / Wiederholen | Strg+Z / Strg+Y |
| System | Version speichern | Strg+S |
| System | Wiederherstellungs-Sicherung | Strg+Umschalt+S |
| Ansicht (Maus) | Schwenken | Mittlere Taste ziehen |
| Ansicht (Maus) | Drehen | Umschalt + mittlere Taste ziehen |
| Ansicht (Maus) | Zoomen | Mausrad |
| Ansicht (Maus) | Um Punkt drehen | Umschalt + Klick, dann mittlere Taste halten |

## Heute vorhanden (bleibt, soweit es nicht Fusion widerspricht)

Strg+N neues Dokument, Strg+O Projekte (bei Fusion: Öffnen), Strg+S Speichern, Strg+W Schließen, Strg+Umschalt+E Exportieren, Strg+Z Rückgängig, Strg+Umschalt+Z und Strg+Y Wiederholen. In der Skizze Esc, Enter, Entf; in der Verlaufsleiste Pfeil links/rechts bei Fokus. Mit der Maus dreht heute die linke Taste, die mittlere (oder Umschalt+links) schwenkt, das Rad zoomt.

## Mein Vorschlag (nicht entschieden)

- **Zentrale Tabelle.** Jede Aktion hat einen festen internen Namen, einen deutschen Anzeigenamen, einen Bereich (Modell, Skizze, Zeichnung, Alle, Ansicht) und eine Voreinstellung. Menüs, Werkzeugleiste und Tooltips zeigen die aktuelle Taste (aus der Tabelle, nicht aus festem Text).
- **Bereiche** entscheiden, welche Belegung gilt: Skizze offen → Bereich Skizze, Zeichnung offen → Bereich Zeichnung, sonst Modell. „Alle“ gilt überall.
- **Tasten und Mausgesten** sind eine Taste mit Umschalttasten (Strg, Umschalt, Alt) oder eine Mausgeste (Taste plus Umschalttaste, Ziehen oder Rad). Eine Aktion darf mehrere Belegungen haben (nötig für „Drehen mit links als Zusatz“, „Wiederholen: Strg+Y und Strg+Umschalt+Z“).
- **Doppelbelegung** im selben Bereich: beim Ändern Warnung mit „Tauschen“ oder „Abbrechen“. Zwischen „Alle“ und einem Bereich zählt es auch als Doppelbelegung. Gleiche Taste in verschiedenen Bereichen ist erlaubt (wie bei Fusion C = Kreis / Mittelpunkt).
- **Eingabevorrang.** Solange ein Textfeld aktiv ist, eine Rückfrage offen ist oder ein Fenster offen ist, das heute die Kürzel sperrt, löst keine Taste einen Befehl aus. Die heutigen Sperren aus `shortcuts`/`undo_keys` gelten für alle Kürzel unverändert weiter (nur im ersten Durchgang eines Bildes, nicht bei gedrückter Maustaste für Rückgängig/Wiederholen).
- **Speichern** in der Konfigurationsdatei (wie Favoriten und Durchsichtigkeit): nur Abweichungen von der Voreinstellung. Neue Aktionen in späteren Versionen übernehmen so automatisch ihre Voreinstellung. Beschädigte oder unbekannte Einträge werden ignoriert und die Voreinstellung gilt; ein Hinweis in den Einstellungen nennt, was ignoriert wurde.
- **Einstellungen:** neuer Abschnitt „Tastenkürzel“ im Fenster Einstellungen: Tabelle (Aktion, Bereich, Belegung), Suche, Zeile anklicken, neue Taste drücken oder Maus bewegen (Esc bricht ab), „Zurücksetzen“ je Zeile und für alles, Hinweis bei Doppelbelegung. Wirkt sofort.
- **Übersicht mit Suche:** Fenster „Tastenkürzel“ im Menü Ansicht; nur lesen, mit Suchfeld (sucht Aktion und Taste), sortiert nach Bereich. Ein Knopf führt in die Einstellungen. Die Fusion-Taste S („Shortcuts“) öffnet dieses Fenster mit aktivem Suchfeld (Bereich Modell und Skizze). Mein Vorschlag, weil Fusion dort die Befehlssuche hat.
- **Letzten Befehl wiederholen (Leertaste):** merkt sich die letzte über Taste, Menü oder Werkzeugleiste ausgelöste Erzeugen- oder Bearbeiten-Aktion (Extrudieren, Verrunden, Verschieben, Skizzenwerkzeug …) und löst sie erneut aus (öffnet deren Dialog bzw. Werkzeug). Nichts zu wiederholen → keine Wirkung. Nicht: Speichern, Rückgängig, Navigation.
- **Maussteuerung:** neue Voreinstellung wie Fusion (mittlere Taste schwenkt, Umschalt+mittlere dreht, Rad zoomt). Zusätzlich bleibt „Drehen mit linker Taste“ und „Schwenken mit Umschalt+links“. „Um Punkt drehen“ in der ersten Stufe nicht, nur falls einfach.
- **Löschen im Modell:** Entf löscht den gewählten Körper bzw. Schritt über denselben Weg wie das Kontextmenü (mit Rückfrage, wenn davon etwas abhängt). Gesperrte Körper: Löschen bleibt erlaubt (wie bisher).
- **Strg+Umschalt+S:** bleibt frei, bis es eine Wiederherstellungs-Sicherung gibt. **Strg+Umschalt+E** (Exportieren) bleibt als PlanoraCAD-eigenes Kürzel.
- **Auswahlarten 1/2/3, Bohrung (H), Messen (I), Drücken/Ziehen (Q), Erscheinungsbild (A), Alles neu berechnen (Strg+B):** nur eintragen, wenn es die Funktion gibt; fehlende Aktionen stehen mit „noch nicht vorhanden“ in einer Liste im Plan, nicht in der Tabelle der Oberfläche.

## Nicht-Ziele

- Fusion-Kürzel für Funktionen, die PlanoraCAD nicht hat.
- Mac-Belegungen (Strg bleibt Strg; in egui steht `COMMAND` für Strg unter Windows).
- Tastenfolgen mit mehreren Anschlägen hintereinander.
- Mehrere Belegungsprofile / Import und Export von Belegungen.
- Eine Befehlspalette mit Auswahl und Ausführen (die Übersicht zeigt nur an).
- Die Fusion-Markierungsmenüs (Radialmenü) und Kontextmenü-Kürzel.

## Anti-Ziele

- Kein Kürzel darf feuern, während man in ein Textfeld tippt oder ein Dialog offen ist, der Tasten braucht (Datei-Auswahl, Formelfelder, Umbenennen).
- Eine Taste darf nie einen Befehl zweimal auslösen (Gehalten: Wiederholung; mehrere Durchgänge pro Bild).
- Ändern darf keine Taste „verschlucken“: Nach dem Ändern gilt genau die neue Belegung, die alte ist frei.
- Beschädigte Einstellungen dürfen das Programm nie unbedienbar machen: Voreinstellung gilt.
- Rückgängig/Wiederholen behalten ihre heutigen Sperren (offene Fenster, gedrückte Maustaste).

## Constraints

- egui/eframe 0.36; Tasten über `egui::Key`, `egui::Modifiers`; Konfiguration über die vorhandene `Config`-Struktur (`Config::update_at`, Tests nur mit temporärem `config_path`).
- Neue Oberflächentexte Deutsch, einfache Sprache; bei 1920 × 1080 geprüft (Gedächtnis ui-min-resolution).
- Kein Autodesk-Material kopieren: nur Tastenbelegungen als Tatsachen; Namen der Aktionen sind eigene deutsche Bezeichnungen.
- Keine neuen Abhängigkeiten.

## Randfälle, die Tests brauchen

- Taste gleichzeitig in Bereich Skizze und Modell (C): in der Skizze wirkt Kreis, im Modell nichts; in der Zeichnung Mittelpunktmarke.
- Eine Aktion mit zwei Belegungen (Wiederholen: Strg+Y und Strg+Umschalt+Z); nur eine davon ändern, die andere bleibt.
- Doppelbelegung beim Ändern; Tauschen; Abbrechen; Ändern auf die Taste, die schon die Voreinstellung der Aktion ist.
- Umschalttasten genau: Strg+Z feuert nicht bei Strg+Umschalt+Z (Reihenfolge wie heute); Alt+Taste.
- Textfeld aktiv, Rückfrage offen, Fenster offen, gedrückte Maustaste, zweiter Durchgang im Bild.
- Gehaltene Taste (Wiederholung) löst nur einmal bzw. wie heute bei Rückgängig pro Bild einmal aus.
- Laden der Konfiguration: fehlender Abschnitt, unbekannte Aktion, ungültige Taste, doppelte Belegung in der Datei, Datei aus neuerer Version.
- Zurücksetzen einer Zeile und aller Zeilen; Speichern schreibt nur Abweichungen.
- Maus: Mittlere Taste schwenkt, Umschalt+Mitte dreht, linke Taste dreht, Umschalt+links schwenkt, Rad zoomt; jede Mausgeste änderbar; Mausgeste und Taste gleichzeitig nicht verwechselbar.
- Leertaste wiederholt, nach Menübefehl, nach Taste, nach Dialog „Abbrechen“ (wird nicht gemerkt), leer.
- Übersicht: Suche nach „Extrudieren“, nach „E“, nach „Strg+S“, nichts gefunden; Fokus im Suchfeld hält Kürzel zurück.
- Tooltips und Menüeinträge zeigen nach einer Änderung sofort die neue Taste.

## Etappen (Vorschlag)

1. Grundlage: Tabelle (Aktionen, Bereiche, Belegungen), Verteiler mit den heutigen Sperren, Umstellen der vorhandenen Kürzel, Konfigurationsspeicher mit Prüfung.
2. Mausbelegung wie Fusion plus Zusatz „links dreht“.
3. Fusion-Belegung der vorhandenen Funktionen (Modell, Skizze, Zeichnung), Anzeige der Tasten in Menüs und Tooltips, „Letzten Befehl wiederholen“.
4. Einstellungen: Abschnitt „Tastenkürzel“ (ändern, Doppelbelegung, Zurücksetzen).
5. Übersicht mit Suche, S-Kürzel.
6. Aufräumen und Dokumentation (README, offene Punkte).

Jede Etappe mit Sichtprüfung im echten Fenster bei 1920 × 1080.

## Festlegungen nach der Bauherren-Prüfung (Claudes Entscheidungen, bitte prüfen)

1. **Maus in 2D-Ansichten:** Die neue Maussteuerung (Drehen, Schwenken, Zoomen mit änderbaren Gesten) gilt für die 3D-Ansicht. In der Skizze und in der Zeichnung gibt es kein Drehen. Dort schwenkt die mittlere Taste und das Rad zoomt schon heute wie bei Fusion; die linke Taste zeichnet und wählt aus (Umschalt+links schwenkt zusätzlich). Das bleibt, die Gesten „Schwenken“ und „Zoomen“ aus der Tabelle gelten dort mit (eine Belegung für alle Ansichten), Drehen nur in 3D.
2. **Skizzenkürzel nur für vorhandene Werkzeuge:** Der Bauer erfasst zuerst, welche Skizzen- und Zeichnungswerkzeuge es gibt (heute in der Skizze: Auswahl, Punkt, Linie, Rechteck, Kreis, Bogen, Ellipse, Spline, Kante projizieren). Eingetragen werden nur Kürzel, für die es ein Werkzeug gibt (Linie L, Rechteck R, Kreis C, Projizieren P; weitere nach der Bestandsaufnahme). Versatz, Stutzen und Skizzenbemaßung entstehen erst mit ihrer Funktion; ihre Kürzel stehen dann in der Liste „noch nicht vorhanden“.
3. **Leertaste:** Wiederholen hat nur Wirkung, wenn kein Bedienelement den Tastaturfokus hat (sonst gehört die Leertaste dem fokussierten Knopf) und wie alle Kürzel nicht bei aktivem Textfeld.
4. **Erfassen einer Mausgeste:** In den Einstellungen wählt man „Mausgeste ändern“ und führt sie aus: Taste gedrückt halten und ziehen (mindestens wenige Pixel) oder Rad drehen, jeweils mit den gerade gehaltenen Umschalttasten (Strg/Umschalt/Alt). Ein bloßer Klick zählt nicht als Geste zum Schwenken/Drehen/Zoomen. Das Rad mit Umschalttaste ist erlaubt. Zwei Gesten sind gleich, wenn Taste/Rad, Umschalttasten und Art (Ziehen/Rad) gleich sind. Tastenbelegungen und Mausgesten werden nie gegeneinander als Doppelbelegung gezählt.
5. **Version der Einstellungen:** Kein neues Versionsfeld. Unbekannte Aktionen und ungültige Tasten in der Konfigurationsdatei werden ignoriert und in den Einstellungen als Hinweis genannt (so verhält sich auch „Datei aus neuerer Version“). Die Konfigurationsdatei ändert sich nur durch einen neuen, optionalen Abschnitt für Abweichungen.
6. **Doppelbelegung mit „Alle“:** Ja: Tauschen wird auch mit einer „Alle“-Aktion angeboten, weil keine Taste verschluckt werden darf (Anti-Ziel).
7. **Gedächtnis:** Der Verweis auf das Gedächtnis (Entscheidung 5) betrifft nur Claudes Arbeitsweise, nicht das Programm; der Plan enthält dafür keinen Schritt.
