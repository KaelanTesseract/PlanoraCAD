# PDF-Ausgabe der Zeichnungen (Teilprojekt 3c-3) – Entwurf

Stand 2026-10-18. Dritter Schritt der Blatt-Strecke nach 3c-1 (Zeichnungsblatt) und 3c-2 (Stückliste und Positionsnummern), beide gemergt. Dieses Teilprojekt gibt Zeichnungen als PDF aus. Schnittansichten (3d) bleiben ein eigenes, späteres Teilprojekt.

## 1. Zweck

Der Nutzer will Zeichnungen **drucken, weitergeben und archivieren**. PDF ist dafür das übliche Format. Wie SVG und DXF muss es das ganze Blatt enthalten (Rahmen, Schriftfeld, Tabellen, Ansichten, Maße, Positionsnummern), wahlweise ohne den Blattrahmen.

## 2. Ziele, Nicht-Ziele, Anti-Ziele

**Ziele (vom Nutzer entschieden):**
- PDF als drittes Zeichnungsformat im Export-Fenster („Zeichnung als PDF“).
- Wahlweise das **gewählte Blatt** oder **alle Blätter** des Dokuments in einer mehrseitigen Datei.
- Der Schalter **„Blattrahmen mit ausgeben“** aus 3c-1 gilt auch für PDF.
- **Standardschrift (Helvetica-ähnlich), nicht eingebettet.** Zeichensatz: westeuropäisch (Umlaute, ß, Ø, °, ±, µ u. a.); ein Zeichen außerhalb des Zeichensatzes stoppt den Export mit einer benannten Meldung (wie beim DXF).
- Die PDF wird **selbst geschrieben, ohne neue Bibliothek**.

**Nicht-Ziele:** Einbetten von Schriften; Farbe (alles schwarz bzw. wie SVG); Lesezeichen, Verknüpfungen, Formularfelder; PDF/A oder PDF/X; Verschlüsselung; Bilder; Lesen von PDF; Drucken aus dem Programm (der Nutzer öffnet die PDF im Betrachter).

**Anti-Ziele (wäre ein Fehlschlag, auch wenn es technisch läuft):** eine PDF, die in gängigen Betrachtern nicht oder falsch öffnet; Text außerhalb seiner Zelle oder verrutscht; Seiten in falscher Größe oder Ausrichtung; Linien in falscher Stärke; eine PDF, die nicht zum Bildschirm und zu SVG/DXF passt; ein Export, der stumm Blätter weglässt; verlorene Positionsnummern, die trotzdem ausgegeben werden.

## 3. Inhalt und Aufbau

- **Seite = Blatt:** Jede Seite hat die Größe des Blatts in Punkt (1 pt = 1/72 Zoll; 1 mm = 72/25,4 pt), Hoch- oder Querformat wie das Blatt. Ein A2-Blatt lässt sich im Betrachter einfach verkleinert auf A4 drucken. Der Nullpunkt der Seite ist unten links wie im Blattmodell (y nach oben).
- **Inhalt je Seite** = genau das, was die SVG-Ausgabe für dieses Blatt schreibt: Ansichtslinien (sichtbar, verdeckt gestrichelt, Mittellinien Strich-Punkt), Maße, Positionsnummern, und mit Blattrahmen: Rahmen, Zentriermarken, Raster, Schriftfeld, Änderungstabelle, Hinweise, Stückliste, Projektionssymbol. Linienstärken, Strichmuster und Maße wie in SVG (3c-1 skaliert mit dem Blattfaktor, Mindeststärke 0,18 mm).
- **Quelle der Geometrie:** dieselbe Zwischendarstellung wie SVG/DXF (`SheetModel`, `Out2d`); kein zweiter Berechnungsweg. Linien, Bögen (als Bézier-Kurven in Stücken bis 90°), Kreise, Texte.
- **Texte:** PDF-Standardschrift Helvetica (Base-14, nicht eingebettet), Zeichenkodierung WinAnsi. Texthöhen wie im SVG (Schrifthöhe = Texthöhe des Blattmodells). Zentrierte und rechtsbündige Texte brauchen die **Zeichenbreiten der Helvetica**; diese Breiten liegen als Tabelle im Quelltext (aus den öffentlich verfügbaren Adobe-Core-14-Metriken; Quelle und Lizenz im Kopf der Datei dokumentiert). Das Kürzen zu langer Texte folgt der Regel F11 aus 3c-1 mit den PDF-Breiten.
- **Mehrseitig:** Reihenfolge der Seiten = Reihenfolge der Zeichnungen im Dokument (wie „Blatt n von N“). „Alle Blätter“ enthält auch die Stücklistenblätter und ihre Folgeblätter.
- **Dokumenteigenschaften:** Titel = Zeichnungsname (bzw. Dokumentname bei mehreren Blättern), Ersteller = Ersteller aus den Projektangaben (falls vorhanden), Programm = „PlanoraCAD“, Erstellungsdatum = Ausgabedatum des Blatts bzw. heutiges Datum in UTC; keine zufälligen Kennungen, damit gleiche Eingaben **byte-gleiche PDF** ergeben (wichtig für Tests).
- **Struktur der Datei:** PDF 1.4, klassische Querverweistabelle, ein Inhaltsstrom je Seite (optional unkomprimiert, damit sie sich prüfen lässt; Komprimierung mit Flate nur, wenn ohne neue Abhängigkeit möglich), ASCII-Kopf. **Entscheidung des Entwurfs:** unkomprimierte Inhaltsströme (Dateien sind Text, leicht prüfbar; Größe bei typischen Blättern unter 1 MB), spätere Komprimierung ist kein Teil von 3c-3.

## 4. Bedienung

- **Export-Fenster:** neues Format „Zeichnung als PDF“. Neben der Zeichnungsauswahl (wie bei SVG/DXF) erscheint für PDF die Wahl **„Gewähltes Blatt“ / „Alle Blätter“** (Vorgabe: gewähltes Blatt). Der Schalter „Blattrahmen mit ausgeben“ gilt wie bei SVG/DXF.
- **Dateiname:** Vorgabe = Zeichnungsname (gewähltes Blatt) bzw. Dokumentname (alle Blätter); Endung `.pdf` wird erzwungen wie bei SVG/DXF.
- **Meldungen** in Laiensprache: verlorene Positionsnummer (wie G13: nennt die Ansicht; bei „alle Blätter“ auch das Blatt), nicht darstellbares Zeichen („Das Zeichen „X“ im Feld „Y“ lässt sich in einer PDF nicht ausgeben.“, bei „alle Blätter“ mit Blattname), zu großes Blatt für die PDF-Grenzen (siehe Abschnitt 5).
- **Fortschritt/Berechnung** wie bei den anderen Zeichnungsformaten (Hintergrundthread, Abbruch beim Schließen); bei „alle Blätter“ werden alle Ansichten aller Blätter berechnet; solange etwas fehlt, ist „Exportieren …“ gesperrt und der Fortschritt zeigt es.

## 5. Grenzen und Fehlerfälle

- PDF-Seitengrenze: höchstens 14 400 pt je Kante (200 Zoll, Grenze vieler Betrachter); A0 (≈ 2384 pt) liegt weit darunter. Mehr als 200 Seiten werden abgelehnt (Meldung).
- Zahlen im Inhaltsstrom sind endlich und auf 4 Nachkommastellen (pt) gerundet; nicht endliche Werte werden wie in SVG/DXF abgewiesen (nichts geschrieben).
- Texte: Zeichen außerhalb WinAnsi → benannte Meldung vor dem Schreiben, nichts wird gekürzt oder ersetzt.
- Schreiben wie die anderen Formate: über eine Hilfsdatei und Umbenennen; bei Fehler bleibt die alte Zieldatei erhalten; Abbruch beim Schließen des Tabs hinterlässt keine Hilfsdatei.

## 6. Qualität und Prüfung

- **Selbsttest der Datei:** ein eigener kleiner PDF-Leser im Test (Kopf, Objekte, Querverweistabelle mit exakten Versätzen, Seitenbaum, Seitenzahl, Seitengrößen in pt, Inhaltsstrom-Länge stimmt) und ein Inhaltsleser (Operatoren `m l c re S w d BT Tf Td Tj ET`), mit dem Linien/Bögen/Texte gegen das Blattmodell geprüft werden (Anzahl, Lage, Stärke).
- **Äquivalenz zu SVG:** für dieselbe Zeichnung stimmen Linien- und Textanzahl und die Textinhalte von SVG und PDF überein.
- **Byte-Gleichheit** bei gleicher Eingabe; Goldene Dateien für ein Blatt ohne Rahmen und eines mit vollem Rahmen (A4 quer, A3 hoch).
- Zeichenmetrik: Tests für die Breitentabelle (bekannte Werte), zentrierter Text liegt mittig, Kürzung mit „…“.
- Export-Fenster: Format wählbar, gewähltes Blatt gegen alle Blätter (Seitenzahl, Reihenfolge, Stücklistenblatt inkl. Folgeblätter), Schalter Rahmen, Sperren (verlorene Kreise, unzulässige Zeichen), Abbruch.
- **Prüfung mit echten Betrachtern:** der Controller prüft Beispieldateien (A4 hoch/quer, A3, A0, 12 Körper mit Stückliste, Stücklistenblatt mit zwei Seiten) mit einem unabhängigen Werkzeug (z. B. Rendern mit einem im System vorhandenen PDF-Betrachter, wenn verfügbar, oder durch Öffnen im Browser) und legt Proben ins `zeichnung-proben`-Verzeichnis; der Nutzer sieht sie an.
- Geschützte Pfade (Kernel, Netz-Quelltext, STEP, Austausch) unverändert; keine neuen Abhängigkeiten.
- **Eigener Test im echten Fenster** durch den Controller vor der Merge-Frage (mit Freigabe des Nutzers).

## 7. Offene Annahmen (vom Entwurf getroffen, vom Nutzer überstimmbar)

- Standardschrift, nicht eingebettet (so entschieden); Breitentabelle aus den Adobe-Core-14-Metriken.
- Unkomprimierte Inhaltsströme; PDF 1.4.
- Seitengröße = Blattgröße; Ausrichtung wie das Blatt.
- Reihenfolge der Seiten = Reihenfolge der Zeichnungen; „alle Blätter“ enthält die Stücklistenblätter.
- Alles in Schwarz, wie SVG.
- Keine Lesezeichen und keine Seitenbeschriftungen.

## 8. Festlegungen des Entwurfs nach dem Fremdlesen (vom Nutzer überstimmbar)

- **Textbreiten (F1):** Das Einpassen und Kürzen von Texten geschieht beim Bau des Blattmodells und ist für Bildschirm, SVG und PDF **dasselbe** (die großzügige Schätzung aus 3c-1); der PDF-Schreiber hat **keine eigene Variante** wie das DXF. Die Helvetica-Breitentabelle dient nur dem **Zentrieren und Rechtsbündigsetzen** im Schreiber (die echte Helvetica ist schmaler als die Schätzung, der Text passt also immer). Dadurch stimmen Textinhalte von SVG und PDF überein, und die Warnungen „wird gekürzt“ im Fenster gelten auch für die PDF.
- **Alle Blätter (F2):** Die ganze PDF wird gesperrt, solange irgendein Blatt nicht ausgegeben werden kann (verlorene Positionsnummer, verlorenes Maß, nicht darstellbares Zeichen, nicht berechenbare Ansicht). Die Meldung nennt das Blatt und den Grund; es werden nie Blätter stillschweigend weggelassen und keine Teil-PDF geschrieben. Die Berechnung läuft für alle Blätter im Hintergrund, der Fortschritt zeigt „Blatt n von N“; der erste Fehler (in Blattreihenfolge) wird gemeldet.

## 9. Festlegungen der Umsetzung

Stand 2026-10-19. Was beim Bauen festgelegt oder gegenüber dem Entwurf geändert wurde; die ausführliche Fassung steht im Plan (P1 bis P14, „Abweichungen bei der Umsetzung“) und in `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 3c-3 – PDF-Ausgabe“.

- **P1 Dateiaufbau:** PDF 1.4, klassische Querverweistabelle, ein unkomprimierter Inhaltsstrom je Seite, ganze Datei ASCII, Seitenbaum in Eingabereihenfolge. **P2 Zahlen:** Punkt (1 mm = 72/25,4 pt), vier Nachkommastellen, nicht endlich → nichts geschrieben. **P4 Strichbreiten** und Muster wie SVG.
- **P3 Ecken (Abweichung vom Plan):** spitze Ecken (`0 j`) wie SVG ohne `stroke-linejoin`, runde Enden wie SVG. Die Gehrungsgrenze der PDF (10) ist größer als die der SVG (4); es betrifft nur Ecken unter etwa 29°.
- **P5 Kurven:** Bögen und Ellipsen in Bézier-Stücken bis 90° (wie die SVG-Bogenstücke), Splines wie SVG (Gradanhebung, Grad 3 direkt, sonst die gemeinsame Näherung). Ein Bogen über einen Vollkreis hinaus ist auf einen Vollkreis begrenzt.
- **P6 Texte (ändert §8 F1):** Das Zentrieren und Rechtsbündigsetzen nutzt die Helvetica-Breiten. Gestaucht wird nur gegen die Zelle, in die ein Text eingepasst wurde (`Mark::Text::fit_width`: Schriftfeld, Tabellen, Stückliste, Nummernkreise), nicht gegen die Schätzung des Blattmodells, weil die Schätzung schmale Zeichen wie „1“ zu schmal annimmt. Texte ohne Zelle (Maße, Raster, Beschriftungen) werden nie gestaucht. `Tz` wird mit der geschriebenen Schriftgröße gerechnet und abgerundet, damit ein Text nie breiter ist als seine Zelle. Das Einpassen und Kürzen selbst bleibt beim Bau des Blattmodells und ist für Bildschirm, SVG und PDF dasselbe (F1 gilt in diesem Teil weiter).
- **Schrift und Zeichensatz:** Helvetica Base-14, nicht eingebettet; der Betrachter ersetzt sie (Arial oder Helvetica, Darstellung leicht anders). Breiten aus der Adobe-Datei `Helvetica.afm` (Core 14 AFM Files), Lizenzvermerk im README unter „Drittanbieter“ und im Kopf von `pdf_font.rs`. WinAnsi mit 219 zulässigen Zeichen; U+2212 wird als „-“ geschrieben; Emoji, chinesische und kyrillische Zeichen sperren den Export mit benannter Meldung (Feld, bei Maßtexten die Ansicht, bei „Alle Blätter“ das Blatt).
- **P7 Eigenschaften und P11 (Antwort auf Q2):** Titel UTF-16BE; Ersteller nur, wenn in den Projektangaben ausdrücklich eingetragen (ohne Leerraum an den Rändern), nie der Anmeldename; Erstellungsdatum nach Weltzeit (UTC): gültiges Ausgabedatum des ersten Blatts, sonst `info.created`, sonst heute. Keine `/ID`, kein `/ModDate`, damit gleiche Eingabe gleiche Bytes ergibt.
- **P8 Grenzen:** höchstens 200 Seiten, höchstens 14 400 pt je Seitenkante, 0 Seiten abgewiesen.
- **P9, P10, P12 Export-Fenster und „Alle Blätter“ (Antwort auf Q1/F2):** Das Fenster hält ein berechnetes Ergebnis je Zeichnungsformat (Wechsel zwischen SVG, DXF und PDF rechnet nicht neu). „Alle Blätter“ rechnet jedes Blatt von Grund auf neu, in einem Hintergrundauftrag mit Fortschritt „Blatt n von N“; der erste Fehler in Blattreihenfolge sperrt die ganze PDF und nennt das Blatt; mehr als 200 Blätter werden vor dem Rechnen gemeldet. Die Berechnung teilt nichts zwischen den Blättern (bekannte Grenze: bei vielen Blättern langsam). Dateiname: Zeichnungsname bzw. Dokumentname, `.pdf` wird erzwungen.
- **P13 Zeichenprüfung:** dieselben Feldnamen wie beim DXF (gemeinsamer Helfer; die DXF-Meldungen bleiben wörtlich gleich), dazu die Maßtexte je Ansicht.
- **P14 Schreiben:** alle drei Zeichnungsformate schreiben über dieselbe Hilfsdatei-Logik (`write_via_temp`); bei Fehler oder Abbruch bleibt die alte Zieldatei, die Hilfsdatei wird entfernt.
- **Golden-Dateien:** `pdf_cylinder_a4_landscape.pdf` und `pdf_box_frame_a3_portrait.pdf` (Byte-Vergleich). Sie hängen an `sin` und `cos` der Plattform; an Rundungsgrenzen kann ein Unterschied im letzten Bit eine Ziffer kippen (selten; bei einer CI unter Linux oder macOS gegebenenfalls einmal neu erzeugen und die Abweichung ansehen).
- **Nicht umgesetzt (wie im Entwurf):** Einbetten von Schriften, Komprimierung, Lesezeichen, PDF/A, Farbe, Drucken aus dem Programm.
