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
- **Änderungstabelle:** oben rechts im Rahmen, bündig, 176 mm breit, Kopfzeile oben, neueste Änderung zuunterst, bis zu **3 Zeilen** sichtbar (die neuesten; Entscheidung des Nutzers nach dem Handtest, vorher 6); ältere stehen im Fenster. Spalten: Rev., Änderung, Datum, Name.
- **Hinweisfeld:** Hinweiszeilen von oben nach unten, Schrift 3,3 mm. Ist die Rahmenbreite mindestens 310 mm (A3 quer und größer), steht es **links neben dem Schriftfeld**, gleiche Unterkante, 110 mm breit, 10 mm Abstand. Sonst (Rahmenbreite unter 310 mm, auf A3 also im Hochformat; mit der Skalierung unten gilt die Schwelle 310 · Faktor, damit steht es auf jeder Größe im Querformat daneben und im Hochformat darüber) steht es **direkt über dem Schriftfeld**, 180 mm breit, Höhe nach Zeilenzahl. Der Platz über dem Schriftfeld (bzw. über dem Hinweisfeld) bleibt für die Stückliste aus 3c-2 frei; sie wächst von dort nach oben. Alle Papiergrößen A4 bis A0 in beiden Ausrichtungen bleiben unterstützt, auch für Altdateien.
- **Freier Bereich für Ansichten:** der Rahmen minus Schriftfeld, Änderungstabelle und Hinweisfeld. Die automatische Anordnung neuer und abgeleiteter Ansichten (`layout.rs`) bleibt in diesem Bereich. Manuell platzierte Ansichten sind nicht eingeschränkt (Verantwortung des Nutzers; keine Warnung).

**Skalierung (Entscheidung des Nutzers, 2026-10-17).** Alle Zahlen oben sind die Werte für **A3** (Bezugsgröße). Auf jedem anderen Format wächst oder schrumpft das ganze Blattzubehör im selben Maß, „wie ein A3-Blatt, das auf A4 verkleinert oder auf A0 vergrößert gedruckt wird“: Faktor `f = lange Blattseite / 420 mm` (A4 0,707, A3 1, A2 1,414, A1 2,002, A0 2,831), hoch und quer gleich, ohne Grenzen. Mit `f` skalieren Heftrand und Rand, Zentriermarken, Rasterfeld (50 · f, Felderzahl wie auf dem vergrößerten A3-Blatt, also auf jedem Format 8 × 6 bzw. 6 × 8), Teilstriche, Schriftfeld mit allen Zellen und Schrifthöhen, Änderungstabelle (Breite, Zeilenhöhe; 3 Zeilen sichtbar), Hinweisfeld, Projektionssymbol und die Schwelle der Hinweisfeld-Regel (310 · f; A4 quer bekommt das Hinweisfeld daher daneben wie A3 quer). Linienstärken wachsen mit, nie dünner als 0,18 mm (A3 0,7 / 0,5 / 0,25; A4 0,49 / 0,35 / 0,18, die ISO-Reihe; DXF rundet auf die nächste Stärke, die das Format kennt). Das weicht vom üblichen festen 180-mm-Schriftfeld auf allen Formaten ab: auf A4 ist das Schriftfeld 127 mm breit, Inhalte 2,47 mm und Feldnamen etwa 1,6 mm hoch – kleiner als die üblichen 2,5 / 3,5 mm, vom Nutzer so gewollt. Auf A0 ist alles 2,8-mal so groß wie auf A3.

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
| Dokumentstatus | „in Arbeit“ (Vorgabe, änderbar; Entscheidung des Entwurfs, weil Zusammenfassungen von ISO 7200 es als Pflichtfeld nennen) | Blatt |
| Ausgabedatum | heutiges Datum beim Anlegen des Blatts, danach fest (änderbar) | Blatt |
| Änderungsstand | Buchstabe der letzten Änderung, leer ohne Änderungen | berechnet |
| Blatt / Anzahl | Platz des Blatts in der Reihenfolge der Zeichnungen des Dokuments / Anzahl | berechnet |
| Format | Papiergröße, z. B. „A3“ | Blatt |
| Maßstab | Maßstab der ersten Ansicht der Zeichnung (Grundansicht, Reihenfolge der Zeichnung); ohne Ansichten „–“. (Geändert nach dem Handtest: vorher die Ansicht mit den meisten Linien, das war oft die Isometrie mit eigenem Maßstab.) Weicht ein anderer ab, bleibt das Feld bei diesem Hauptmaßstab (der Maßstab der abweichenden Ansicht steht an dieser Ansicht, soweit sie ihn schon zeigt) | berechnet |
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
- Änderungstabelle oben rechts, bis 3 Zeilen sichtbar (nach dem Handtest; vorher 6).
- Kein automatisches Gewicht.

## 10. Festlegungen der Umsetzung

Stand 2026-10-17. Der Plan (`docs/superpowers/plans/2026-10-17-zeichnungsblatt.md`) hat Lücken der Spec mit den Festlegungen F1 bis F15 geschlossen; der Nutzer hat F9 (Hinweisfeld nach der 310-mm-Regel), F7 (Projektionssymbol) und F15 (Dokumentstatus) entschieden. Die Zahlen aus §3 und §4 gelten für den Bezug A3; für andere Formate skalieren sie (Absatz „Skalierung“ in §3).

**F1 bis F15**
- **F1** Ein Feld ist „automatisch“, solange es leer ist; „Zurücksetzen“ leert es. Gespeichert: automatisch oder eigener Text (in der Datei: fehlt oder `null` heißt automatisch).
- **F2** Datum = Kalendertag in UTC, Schreibweise `JJJJ-MM-TT`; eigene Rechnung, keine Abhängigkeit für Ortszeit.
- **F3** `Document::add_drawing` bleibt unverändert (Zeichnung ohne Satz, wie aus Format ≤ 5); neu `add_drawing_stamped` für die App (Satz mit Datum von heute, Projektangaben mit Windows-Anmeldename, falls noch keine da sind).
- **F4** Alte Zeichnungen bekommen Satz und Projektangaben erst mit „Übernehmen“; Öffnen und „Abbrechen“ ändern nichts.
- **F5** Raster: Felderzahl je Kante `2 · round(Kante / 100)` (Bezug A3: 8 × 6; die Teilung 50 mm skaliert mit dem Format, die Zahl der Felder bleibt wegen der Skalierung auf allen Formaten 8 × 6), Teilung 50 mm von der Blattmitte nach außen, Teilstriche 0,25 mm vom Rahmen 5 mm nach außen, Beschriftung mittig im Rand, Zeilenbuchstaben ohne I und O.
- **F6** Schriftfeld mit 17 Zellen in fünf Zeilen (Höhe 56 mm): Zeichnungsnummer, Änderungsstand, Blatt, Sprache; Titel mit Zusatztitel; Eigentümer, Dokumentart, Dokumentstatus; Ersteller, Geprüft, Freigabe, Ausgabedatum; Werkstoff, Gewicht, Maßstab, Format, Projektion. Feldname klein links oben, Inhalt darunter.
- **F7** Projektionssymbol (h = 3,5): Kegelstumpf (Trapez, 7 lang, 3,5 und 7 hoch) und zwei Kreise Ø 7 und Ø 3,5, Abstand 3,5. Methode 1: Kegelstumpf links, Kreise rechts; Methode 3: Kreise links, Kegelstumpf rechts. Linien 0,5 mm.
- **F8** Freier Bereich = die größere zweier Rechtecke (volle Rahmenbreite zwischen Schriftfeld-Stapel und Änderungstabelle, oder volle Rahmenhöhe links vom linkesten Hindernis).
- **F9** Hinweisfeld ab 310 mm Rahmenbreite neben dem Schriftfeld, sonst darüber; Höhe `2 · 2 + n · 6` mm, ohne Zeilen kein Feld.
- **F10** Änderungstabelle: Zeilen 7 mm, Spalten 12 | 104 | 30 | 30, Schrift 3,5 mm, höchstens 3 Zeilen sichtbar (Abschlussrunde; vorher 6); Tabelle und Hinweisfeld mit Linien 0,25 mm.
- **F11** Textbreite zum Einpassen konservativ geschätzt: breite Zeichen 0,95·h, schmale 0,3·h, übrige Großbuchstaben und Ziffern 0,72·h, Rest 0,6·h.
- **F12** Vorbelegte Hinweiszeilen: „Allgemeintoleranzen ISO 2768-m“, „Kanten gebrochen 0,3 mm“, „Oberflächen Ra 3,2 (soweit nicht anders angegeben)“; je Blatt automatisch = die des Projekts, oder eigene Zeilen.
- **F13** Ausgabe: SVG-Gruppe `Blattrahmen` als erste Gruppe; DXF-Objekte im Modellbereich vor den Ansichten, Ebenen `BLATTRAHMEN`, `BLATT_MITTE`, `BLATT`, nur wenn der Rahmen ausgegeben wird.
- **F14** Das Fenster „Blattangaben“ arbeitet auf einer Kopie; „Übernehmen“ schreibt, „Abbrechen“ und Schließen verwerfen; in alten (schreibgeschützten) Ständen gesperrt.
- **F15** Dokumentstatus (ISO 7200): je Blatt, automatisch „in Arbeit“, eigene Zelle im Schriftfeld, im Fenster unter „Dieses Blatt“.

**Abweichungen von diesem Entwurf bei der Umsetzung**
- **§5 Knopf:** „Blattangaben“ steht am Ende der ersten Zeile des Werkzeugfelds, nicht neben „Blatt einpassen“; sonst rollt das Werkzeugfeld schon bei 1100 × 700. Bei 800 × 600 rollt es wie vorher.
- **§3 Zentriermarken:** 0,5 mm und 5 mm hinter dem Rahmen wie im Entwurf; ein öffentlich zugänglicher Auszug der Norm nennt 0,7 mm und 10 mm, ungeprüft (siehe `offene-punkte.md`, Normprüfung). Die rechte Marke lief zunächst wegen eines Fehlers der Aufgabenvorgabe 15 mm hinter den Rahmen und ist berichtigt.
- **§3 Projektionssymbol und Ebenen:** In der DXF-Datei liegt das Symbol auf der Ebene `BLATT_MITTE` (Zuordnung nach Strichstärke), nicht auf `BLATT`.
- **§3 Schriftfeldhöhe:** die Zellenhöhen folgen F6 (nicht der Notiz „7 bis 16 mm“).
- **§4 Schrift:** Reihen 2,2 / 3,5 / 5 wie im Entwurf; ISO 3098 nennt 2,5 / 3,5 / 5 (ungeprüft).
- **§4 Grenzen:** Einpassen verkleinert bis 60 % und kürzt danach mit „…“; die Warnung im Fenster gilt auch für Zellen der Änderungstabelle (je Zeile eine Meldung) und für Hinweiszeilen. Blatttexte unter einem Bildpunkt werden in der Ansicht auf einen Bildpunkt angehoben.
- **§4 Felder:** Ein leeres Feld bedeutet „automatisch“ und lässt sich nicht auf „leer“ erzwingen.
- **§6 DXF:** Ein Blatttext über 255 Zeichen in der Datei und ein nicht darstellbares Zeichen lehnen nur den DXF-Export ab, mit Feldname (kein stilles Kürzen). SVG nimmt alles.
- **§7 Datei:** Format 6 sobald Projekt- oder Blattangaben vorhanden sind; ein Dokument mit nur Projektangaben bleibt nach dem Löschen der letzten Zeichnung Format 6. Ein Format-6-Dokument ohne beide Datensätze lädt nachsichtig.
- **§3 freier Bereich:** Eine neue Zeichnung hat nur die Grundansicht; sie liegt in der Mitte des freien Bereichs (Entscheidung des Nutzers nach dem Handtest). Mit der ersten weiteren Ansicht rückt sie im selben Schritt so, dass der Dreiansichtenblock mittig im freien Bereich liegt, sofern sie noch an ihrer automatischen Stelle steht (Abweichung höchstens 0,01 mm); hat der Benutzer sie verschoben, bleibt sie. Der Maßstabsvorschlag bleibt der für den ganzen Block. Die isometrische Ansicht kommt in das leere Viertel des Blocks (Erstwinkel rechts unten, Drittwinkel links oben, 20 mm neben der Grundansicht); passt sie dort im Maßstab der Grundansicht nicht hinein, bekommt sie die nächstkleinere Stufe der Maßstabsreihe (höchstens eine Stufe kleiner). Gibt es auch so keinen Platz (fast blattfüllende Teile oder ein schmales Viertel), sitzt sie im Maßstab der Grundansicht bündig in der Ecke des freien Bereichs und kann den Block überdecken. Alte Zeichnungen werden nicht verschoben. Für die Stückliste wird kein Platz freigehalten.
- **§3 Skalierung:** siehe Absatz „Skalierung“ in §3; F5 (Felderzahl), F9 (310 mm), F7, F10 und alle Maße in F6 sind die A3-Werte und gelten auf den anderen Formaten mal Faktor.
- **§3/§4 Änderungstabelle:** Auf dem Blatt stehen nur die letzten 3 Änderungen (Kopf und höchstens 3 Zeilen, die neueste unten); das Fenster hält bis 23 und sagt „Auf dem Blatt stehen die letzten 3 Änderungen.“. Die Warnung bei Kürzung gilt nur für diese drei Zeilen. Für die automatische Anordnung neuer Ansichten und den Maßstabsvorschlag hält der freie Bereich immer Platz für Kopf und 3 Zeilen frei, auch wenn es noch weniger Änderungen gibt; gezeichnet wird die Tabelle mit den tatsächlichen Zeilen.
- **§4 Schriftfeld:** Die Zelle des Änderungsstands heißt im Schriftfeld „Index“ (die Spalte der Änderungstabelle heißt weiter „Änderung“, im Fenster „Änderungsstand“).
- **§6 DXF-Texte:** Die Höhe eines DXF-Texts ist die Höhe der Großbuchstaben; die Schriften der CAD-Programme laufen 1,1- bis 1,7-mal breiter als die Schätzung F11. Für DXF werden die Rahmentexte deshalb gegen die 1,45-fache Schätzung eingepasst (dieselbe Regel: erst kleiner bis drei Fünftel, dann gekürzt mit „…“); SVG bleibt bei der einfachen Schätzung. Ein nicht darstellbares Zeichen nennt die Meldung als Zeichennummer („U+1F600“) mit dem Feld.
- **§5 Länge:** Die Länge „n/max“ steht an allen Feldern und Hinweiszeilen, auch leeren; bei den Zellen der Änderungstabelle steht sie im Hinweis am Feld (daneben wäre die Zeile zu breit).
