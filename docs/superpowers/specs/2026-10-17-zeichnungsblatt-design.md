# Zeichnungsblatt (Teilprojekt 3c-1) – Entwurf

Stand 2026-10-17. Teil 3 der Austausch-Strecke, dritter Schritt nach 3a Ansichten (TP11) und 3b Maße und Mittellinien (TP12). Dieses Teilprojekt macht aus dem bisherigen Blatt (Papier mit einem Innenrahmen) ein formal vollständiges technisches Zeichnungsblatt. Stückliste und Positionsnummern (3c-2), PDF-Ausgabe (3c-3) und Schnittansichten (3d) sind eigene, spätere Teilprojekte.

## 1. Zweck und Warum

Der Nutzer will Zeichnungen **weitergeben, drucken oder in der Werkstatt aufhängen** können, ohne am Blatt etwas nachzuzeichnen. Dazu muss alles auf das Blatt, was die üblichen Richtlinien für technische Zeichnungen verlangen („es soll formal passen“). Maßgeblich sind die Normen für Blattformat und Rahmen (ISO 5457 / DIN EN ISO 5457) und für das Schriftfeld (ISO 7200:2004 / DIN EN ISO 7200, ersetzt DIN 6771-1).

Hinweis zur Quellenlage: Die Normtexte sind kostenpflichtig und lagen nicht vor. Die Maße und Felder unten stammen aus öffentlichen Zusammenfassungen und aus der gängigen Praxis. **Vor der Umsetzung der Maße (Abschnitt 3) ist zu prüfen, ob sie zu den Normen passen; jede Abweichung wird im Ledger und in `offene-punkte.md` vermerkt.** Die Normen selbst zu kaufen ist Sache des Nutzers.

## 2. Ziele, Nicht-Ziele, Anti-Ziele

**Ziele (vom Nutzer):**
- Auf dem Blatt: Rahmen mit Heftrand, Zentriermarken, Koordinatenraster, Schriftfeld, Änderungstabelle und Hinweise (Allgemeintoleranzen, Oberfläche, Kantenbruch).
- Alle Texte änderbar, vieles vorausgefüllt. Bearbeitet wird in einem **Fenster „Blattangaben“** (nicht direkt auf dem Blatt).
- Ein Projekt kann mehrere Blätter haben, durchnummeriert („Blatt 2 von 3“).
- Ausgabe in SVG und DXF mit einem Schalter **„Blattrahmen mit ausgeben“**; ohne den Schalter kommen nur Ansichten, Maße und Mittellinien wie bisher heraus.

**Nicht-Ziele (3c-1):** Stückliste und Positionsnummern (3c-2); PDF (3c-3); Schnittansichten (3d); automatisches Gewicht (es gibt keine Dichtedatenbank); Blattvorlagen, Logos oder Bilder im Schriftfeld; mehrsprachige Oberfläche der Blattbeschriftung (feste deutsche Feldnamen); Blattformate außerhalb A4 bis A0.

**Anti-Ziele (wäre ein Fehlschlag, auch wenn es technisch läuft):** abgeschnittene oder überlappende Texte im Schriftfeld; Rahmen oder Schriftfeld, die beim Drucken vom Blatt rutschen; ein Blatt, das an die Ansichten stößt, ohne dass der Nutzer es merkt; verlorene Änderungen beim Speichern und Laden; alte Dateien, die sich nicht mehr öffnen lassen.

## 3. Das Blatt (Maße in Millimetern)

Alle Maße sind Vorgaben nach Praxis und DIN/ISO 5457 und gelten für alle Papiergrößen A4 bis A0 (quer und, bei A4, hoch). Das Blatt behält sein Koordinatensystem (y nach oben, Ursprung unten links).

- **Rahmen:** Rechteck, links **20 mm** (Heftrand) von der Blattkante, oben, rechts und unten **10 mm**. Linienstärke 0,7 mm. Der bisherige dünne Innenrahmen (`FRAME_MARGIN_MM` 10 mm rundum) entfällt; mit „Blattrahmen aus“ gibt es keinen Rahmen.
- **Zentriermarken:** vier Striche, 0,5 mm stark, jeweils in der Mitte jeder Blattkante, von der Blattkante bis 5 mm in den Rahmen hinein.
- **Koordinatenraster:** Randteilung am Rahmen innen, Feldlänge 50 mm, gerade Anzahl Felder, vom Blattmittelpunkt nach außen geteilt. Spalten mit Zahlen 1, 2, … von links nach rechts, Zeilen mit Buchstaben A, B, … von oben nach unten. Beschriftung außerhalb des Rahmens an allen vier Seiten, Schrifthöhe 3,5 mm.
- **Schriftfeld:** unten rechts, bündig mit dem Rahmen, **180 mm breit**, Höhe so, dass alle Felder (Abschnitt 4) Platz haben (Vorschlag 56 mm, Zeilenraster 7 mm bis 16 mm). Außenlinie 0,7 mm, Innenlinien 0,25 mm. Beschriftung der Felder klein (2,2 mm), Inhalt 3,5 mm, Titel 5 mm.
- **Änderungstabelle:** oben rechts im Rahmen, bündig, 176 mm breit, Kopfzeile oben, neueste Änderung zuunterst, bis zu **6 Zeilen** sichtbar (die neuesten); ältere stehen im Fenster. Spalten: Rev., Änderung, Datum, Name.
- **Hinweisfeld:** Hinweiszeilen von oben nach unten, Schrift 3,3 mm. Ist die Rahmenbreite mindestens 310 mm (A3 quer und größer), steht es **links neben dem Schriftfeld**, gleiche Unterkante, 110 mm breit, 10 mm Abstand. Sonst (A4 quer und hoch, A3 hoch und größer hoch) steht es **direkt über dem Schriftfeld**, 180 mm breit, Höhe nach Zeilenzahl. Der Platz über dem Schriftfeld (bzw. über dem Hinweisfeld) bleibt für die Stückliste aus 3c-2 frei; sie wächst von dort nach oben. Alle Papiergrößen A4 bis A0 in beiden Ausrichtungen bleiben unterstützt, auch für Altdateien.
- **Freier Bereich für Ansichten:** der Rahmen minus Schriftfeld, Änderungstabelle und Hinweisfeld. Die automatische Anordnung neuer und abgeleiteter Ansichten (`layout.rs`) bleibt in diesem Bereich. Manuell platzierte Ansichten sind nicht eingeschränkt (Verantwortung des Nutzers; keine Warnung).

## 4. Felder und Vorbelegung

Jedes Feld ist entweder **automatisch** (das Programm berechnet es) oder ein **eigener Text** (der Nutzer überschreibt). „Zurücksetzen“ im Fenster stellt „automatisch“ wieder her. Eine neue Zeichnung zeigt alles Vorbelegte sofort ohne Zutun des Nutzers.

**Projektweit gespeicherte Angaben** (im Dokument, gelten als Vorgabe für alle Blätter; **Entscheidung des Entwurfs: im Dokument gespeichert, weil ein „Projekt“ im Programm nur ein Ordner ohne eigene Daten ist**): Eigentümer, Ersteller, Geprüft von, Freigabe, Werkstoff, Allgemeintoleranz, Hinweiszeilen (Standard).
**Je Blatt** (in der Zeichnung gespeichert): Zeichnungsnummer, Titel, Zusatztitel, Dokumentart, Ausgabedatum, Blattfelder, Änderungen, eigene Hinweiszeilen.

| Feld | Vorbelegung (automatisch) | Quelle |
|---|---|---|
| Eigentümer | leer (Pflichtfeld nach ISO 7200; Hinweis im Fenster, wenn leer) | Projektangabe |
| Zeichnungsnummer | `<Dokumentname>-<lfd. Nr. 01…>` nach Reihenfolge der Blätter | Blatt |
| Titel | Name der Zeichnung | Blatt |
| Zusatztitel | leer | Blatt |
| Dokumentart | „Fertigungszeichnung“ | Blatt |
| Ausgabedatum | heutiges Datum beim Anlegen des Blatts, danach fest (änderbar) | Blatt |
| Änderungsstand | Buchstabe der letzten Änderung, leer ohne Änderungen | berechnet |
| Blatt / Anzahl | Platz des Blatts in der Reihenfolge der Zeichnungen des Dokuments / Anzahl | berechnet |
| Format | Papiergröße, z. B. „A3“ | Blatt |
| Maßstab | Maßstab der Ansicht, die am meisten Linien hat; bei Gleichstand die erste. Weicht ein anderer ab, bleibt das Feld bei diesem Hauptmaßstab (der Maßstab der abweichenden Ansicht steht an dieser Ansicht, soweit sie ihn schon zeigt) | berechnet |
| Projektion | Symbol für Projektionsmethode 1 oder 3 aus `Drawing.projection` | berechnet |
| Ersteller | Windows-Benutzername des angemeldeten Nutzers beim Anlegen des Projekts (änderbar; **Entscheidung des Entwurfs**) | Projektangabe |
| Geprüft von / Freigabe | leer | Projektangabe |
| Werkstoff | „–“ | Projektangabe |
| Gewicht | „–“ (kein automatischer Wert) | Blatt |
| Sprache | „de“ | fest |

Vorbelegte **Hinweiszeilen:** „Allgemeintoleranzen ISO 2768-m“, „Kanten gebrochen 0,3 mm“, „Oberflächen Ra 3,2 (soweit nicht anders angegeben)“. Bis zu 6 Zeilen, jede änderbar, löschbar, neue anfügbar.

**Änderungen:** Zeilen mit Buchstabe (automatisch A, B, C … ohne I, O und Q, also 23 Buchstaben; änderbar), Beschreibung (Text), Datum (heute, änderbar), Name (Ersteller, änderbar). Anfügen, Ändern, Löschen im Fenster; die letzte bestimmt den Änderungsstand im Schriftfeld. Höchstens 23 Zeilen (danach Meldung „höchstens 23 Änderungen“).

**Grenzen:** jedes Textfeld höchstens 80 Zeichen (Titel 60, Hinweiszeile 100, Änderungsbeschreibung 120); Text, der in sein Feld nicht passt, wird auf die Feldbreite verkleinert (bis 60 % der Schrifthöhe) und danach mit „…“ gekürzt; im Fenster zeigt eine Warnung beim Feld, wenn gekürzt wird.

## 5. Bedienung

- **Fenster „Blattangaben“**: öffnet sich über einen Knopf in der Zeichnungsleiste (neben „Blatt einpassen“) und über das Menü der Zeichnung. Breiter Dialog (Dialograhmen aus TP13, 720 px), drei Abschnitte: „Projekt“ (projektweite Angaben), „Dieses Blatt“ (blattweite Felder), „Änderungen“ (Tabelle). Jedes Feld zeigt den Vorgabetext als Hinweis, einen Knopf „Zurücksetzen“ und die Länge.
- Das Blatt in der Zeichnungsansicht zeigt Rahmen, Raster, Schriftfeld, Änderungstabelle und Hinweisfeld sofort (kein Schalter in der Ansicht; Anzeige immer an).
- Texte nur über Textfelder aus `theme::text_field`; Eingaben sind Laiensprache, keine technischen Hinweise im Programmtext.
- Neues Blatt: Papiergröße und Ausrichtung wie bisher im Dialog „Neue Zeichnung“; alle Blattangaben vorbelegt.
- Hell/Dunkel: das Blatt bleibt weißes Papier mit schwarzer Zeichnung, nur der Hintergrund folgt dem Theme.

## 6. Ausgabe

- **SVG und DXF:** Im Export-Fenster ein Schalter „Blattrahmen mit ausgeben“ (Vorgabe **an**). An: Rahmen, Zentriermarken, Koordinatenraster, Schriftfeld, Änderungstabelle, Hinweisfeld. Aus: nur Ansichten, Maße, Mittellinien wie bisher, **ohne** Rahmen (der bisherige Innenrahmen entfällt dadurch auch aus der Ausgabe ohne Rahmen; **Entscheidung des Entwurfs**, damit „aus“ wirklich nur den Inhalt liefert).
- Linienstärken wie oben (0,7 / 0,5 / 0,25 mm), Schrift im SVG Standardschrift (sans-serif), im DXF wie die Maßtexte heute (`dxf_text_fits`-Prüfung bleibt; ein nicht darstellbares Zeichen im Blatttext verhindert den DXF-Export mit benannter Meldung).
- Das Projektionssymbol wird als Linien gezeichnet (Kegelstumpf mit Kreisen, Methode 1 bzw. gespiegelt für 3).
- Blatt-Texte sind Unicode-Text; das DXF-Format behandelt Umlaute wie die bisherigen Maßtexte.

## 7. Datenmodell und Datei

- Neue Daten: projektweite Angaben (einmal je Dokument) und je Zeichnung ein Blattangaben-Satz (Felder, Änderungen). Felder sind „automatisch“ oder „eigener Text“.
- **Gespeichert wird immer, was fest sein muss:** Beim Anlegen einer Zeichnung entsteht ihr Blattangaben-Satz mit dem Ausgabedatum von heute; beim ersten Anlegen einer Zeichnung (oder beim ersten Öffnen des Fensters) im Dokument entstehen die projektweiten Angaben mit dem Windows-Benutzernamen als Ersteller. Dadurch drifft nichts zwischen Rechnern oder beim erneuten Öffnen.
- **Dateiformat 6, sobald das Dokument einen Blattangaben-Satz oder projektweite Angaben enthält** (also jede Datei mit einer neu angelegten Zeichnung); ohne diese Daten bleibt die Datei Format 5. Ältere Programmstände verweigern Format 6 mit einer benannten Meldung (wie bei den früheren Formaterweiterungen).
- **Alte Zeichnungen (Format ≤ 5) ohne Satz:** das Blatt zeigt die Vorbelegung, Ausgabedatum und projektweite Angaben stehen auf „–“ (leer), bis der Nutzer das Fenster öffnet und sie ausfüllt; erst dann wird gespeichert. Es wird nichts beim bloßen Öffnen verändert.
- Reihenfolge und Anzahl der Blätter ergeben sich aus der Zeichnungsliste des Dokuments (wie im Browser). Umordnen der Blätter ist kein Teil von 3c-1 (die Reihenfolge ist die Anlegereihenfolge, wie heute).

## 8. Qualität und Prüfung

- Alle Maße aus Abschnitt 3 sind als Konstanten benannt und durch Tests mit den genannten Zahlen belegt (Rahmenkoordinaten für A4 hoch, A4 quer, A3, A0; Zentriermarken; Rasterteilung; Schriftfeldlage).
- Textlauf: Test mit Längstexten in jedem Feld (kein Überlauf über die Feldlinie), mit Sonderzeichen (ä ö ü ß Ø ° ±), mit leeren Feldern.
- Projektionssymbol, Maßstab-Auswahl, Blatt n/N, Änderungsbuchstaben (ohne I, O, Q; nach dem 23. Buchstaben (Z) Fehlermeldung „höchstens 23 Änderungen“), Vorbelegung und Zurücksetzen.
- Speichern/Laden und Format 5/6 (leer und gefüllt), alter Stand öffnet; Ausgabe mit und ohne Rahmen (SVG und DXF), DXF-Prüfung von Blatttexten.
- Anzeige in der Zeichnungsansicht bei 800×600 und großem Fenster; Eingabe im Dialog ohne Tastenkürzel-Konflikte (`viewport::typing`).
- Geschützte Pfade (Kernel, Netz-Quelltext, STEP, Austausch) bleiben unverändert.
- Handliste für den Nutzer am Ende (Blatt in allen Papiergrößen ansehen, ausdrucken, Fusion/Inkscape öffnen).

## 9. Offene Annahmen (vom Entwurf getroffen, vom Nutzer überstimmbar)

- Projektweite Angaben liegen im Dokument, nicht im Projektordner.
- Ersteller wird aus dem Windows-Benutzernamen vorbelegt und dann im Dokument gespeichert (einmal je Dokument, nicht je Blatt).
- Jede neu angelegte Zeichnung macht die Datei zu Format 6.
- Hinweisfeld neben dem Schriftfeld nur ab 310 mm Rahmenbreite, sonst darüber.
- Maßstabsfeld zeigt immer den Hauptmaßstab, nie „verschieden“.
- Änderungsbuchstaben: 23 (ohne I, O, Q).
- Ohne „Blattrahmen“ auch kein dünner Innenrahmen mehr in der Ausgabe.
- Hinweisfeld links neben dem Schriftfeld; Platz über dem Schriftfeld für die Stückliste (3c-2).
- Änderungstabelle oben rechts, bis 6 Zeilen sichtbar.
- Kein automatisches Gewicht.
