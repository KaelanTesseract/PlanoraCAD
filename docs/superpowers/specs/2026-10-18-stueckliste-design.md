# Stückliste und Positionsnummern (Teilprojekt 3c-2) – Entwurf

Stand 2026-10-18. Zweiter Schritt der Blatt-Strecke nach 3c-1 (Zeichnungsblatt, gemergt). Dieses Teilprojekt macht aus den Körpern des Projekts eine Stückliste auf dem Blatt und auf einem eigenen Stücklistenblatt und lässt den Nutzer Positionsnummern an die Teile in den Ansichten setzen. PDF-Ausgabe (3c-3) und Schnittansichten (3d) bleiben eigene, spätere Teilprojekte.

## 1. Zweck

Eine Zeichnung mit mehreren Teilen soll formal vollständig sein: Jede Position steht in einer Stückliste (Norm ISO 7573 nennt die Mindestanforderungen, die Quelle lag nicht vor, siehe Abschnitt 8), und an den Teilen in den Ansichten stehen die passenden Positionsnummern. Die Texte sind wie überall änderbar und vorbelegt.

## 2. Ziele, Nicht-Ziele, Anti-Ziele

**Ziele (vom Nutzer entschieden):**
- Jeder Körper des Projekts ist eine Zeile der Stückliste. Benennung = Körpername. Gleich benannte Körper werden zu einer Zeile mit der gezählten **Menge**.
- **Stückliste auf dem Zeichnungsblatt** mit **Schalter je Blatt**; vorbelegt an, sobald das Projekt mehr als einen Körper hat, sonst aus. Höchstens 10 Zeilen auf dem Blatt.
- **Eigenes Stücklistenblatt**: ein Knopf „Stücklistenblatt erzeugen“ legt ein zusätzliches Blatt an, dessen Inhalt die Stückliste ist (große Tabelle mit Rahmen und Schriftfeld, Blatt n von N wie die übrigen Blätter). Sind es mehr Zeilen als auf ein Blatt passen, entstehen Folgeblätter. Das Blatt bleibt aktuell, wenn sich die Körper ändern.
- **Positionsnummern** setzt der Nutzer von Hand per Klick auf das Teil in einer Ansicht: Kreis mit Hinweislinie und Nummer, der Kreis ist verschiebbar. Ein Teil kann mehrere Nummernkreise haben. Die Nummer folgt der Reihenfolge der Stückliste.
- Stückliste und Nummernkreise erscheinen auch in SVG und DXF (mit dem Schalter „Blattrahmen mit ausgeben“ aus 3c-1 wie dort).

**Nicht-Ziele:** automatisches Setzen aller Nummernkreise; Baugruppenstruktur mit Unterbaugruppen; Gewicht oder Werkstoff aus einer Datenbank; Artikelnummern, Lieferanten, Preise; PDF (3c-3); Umordnen der Zeilen von Hand (Reihenfolge = Reihenfolge der Körper).

**Anti-Ziele:** eine Stückliste, die nicht zum Modell passt (veraltet); Nummernkreise, die nach dem Löschen oder Umbenennen eines Körpers auf das falsche Teil zeigen; Text, der aus Zellen ragt; Verlust von Eingaben beim Speichern; alte Dateien, die sich nicht mehr öffnen lassen; Stückliste und Nummernkreise, die sich beim Neuberechnen der Zeichnung verschieben.

## 3. Die Stückliste

- **Datenquelle:** die sichtbaren, nicht unterdrückten Körper des Dokuments in Reihenfolge des Feature-Baums. Gleichnamige Körper ergeben eine Zeile (Menge = Anzahl), die Position bekommt die Nummer der ersten Fundstelle. **Entscheidung des Entwurfs:** gleich = gleicher Name (so der Nutzer); gleichnamige Körper mit unterschiedlicher Geometrie bleiben eine Zeile (der Nutzer benennt sie dann anders).
- **Spalten (Vorgabe, nach Praxis und ISO 7573):** Pos., Benennung, Menge, Werkstoff, Bemerkung. Vorbelegung: Pos. fortlaufend ab 1; Benennung = Körpername; Menge gezählt; Werkstoff = Projektangabe „Werkstoff“ (3c-1), sonst „–“; Bemerkung leer. Jede Zelle ist **automatisch oder eigener Text** (wie in 3c-1, „Zurücksetzen“ stellt „automatisch“ wieder her; leeres Feld = automatisch). Texte höchstens 60 Zeichen (Benennung 60, Bemerkung 80).
- **Reihenfolge:** Positionen laufen auf dem Blatt von unten nach oben (die Tabelle sitzt über dem Schriftfeld, die Kopfzeile ganz unten an das Schriftfeld anschließend); auf dem Stücklistenblatt von oben nach unten mit Kopfzeile oben (**Entscheidung des Entwurfs**: auf dem eigenen Blatt so üblicher zu lesen).
- **Auf dem Zeichnungsblatt:** Tabelle direkt über dem Schriftfeld, gleiche Breite wie das Schriftfeld, Zeilenhöhe wie die Änderungstabelle, skaliert mit der Papiergröße (Faktor aus 3c-1). **Platz wird immer für 10 Zeilen plus Kopf reserviert, solange der Schalter an ist**, egal wie viele Körper es gibt; die automatische Anordnung von Ansichten hält diesen Platz frei (Muster wie bei der Änderungstabelle, damit später hinzukommende Zeilen keine Ansicht überdecken). Bei mehr als 10 Zeilen stehen die ersten 10 auf dem Blatt; das Fenster warnt „Weitere Zeilen passen nicht aufs Blatt. Dafür gibt es das Stücklistenblatt.“ Das Hinweisfeld (3c-1) rückt, falls nötig, so, dass nichts überlappt (neue Regel in der Geometrie: Stückliste über dem Schriftfeld, Hinweisfeld darüber oder daneben je nach Breite).
- **Stücklistenblatt:** Eine Zeichnung eines neuen Typs „Stückliste“ (ohne Ansichten): Rahmen, Raster, Zentriermarken und Schriftfeld wie jedes Blatt (Dokumentart vorbelegt „Stückliste“, Titel „Stückliste“, Blatt n von N, Zeichnungsnummer fortlaufend), dazu die Tabelle über die ganze Rahmenbreite mit so vielen Zeilen, wie in die freie Fläche passen; Folgeblätter („Stückliste (2)“, …) entstehen und verschwinden automatisch mit der Zahl der Zeilen. Der Nutzer legt das Blatt mit dem Knopf an und kann es wie jedes Blatt löschen; wenn es gelöscht wird, gehören auch die Folgeblätter dazu. Format wie gewählt (Vorgabe A4 hoch).

## 4. Positionsnummern

- **Setzen:** Werkzeug „Position“ in der Zeichnungsleiste: Klick auf eine Linie eines Teils in einer Ansicht setzt einen Nummernkreis, dessen Hinweislinie am Klickpunkt endet; der Kreis liegt in Standardabstand, schräg oben; Ziehen verschiebt den Kreis (Hinweislinie bleibt am Teil). Klick auf einen vorhandenen Kreis wählt ihn; Entf löscht ihn; Esc bricht das Werkzeug ab.
- **Zuordnung zum Teil:** jede Linie einer Ansicht kennt ihren Körper (`StrokeOrigin`). Ein Nummernkreis merkt sich Körper, Ansicht und Ankerpunkt in Ansichtskoordinaten. **Der Körper wird über sein Feature (stabile Kennung, wie bei den Maßen) gemerkt, nicht über seinen Index**, damit Umordnen und Neuberechnen die Zuordnung nicht kaputt machen. Verschwindet der Körper (gelöscht, unterdrückt oder durch eine Boolesche Operation verbraucht), wird der Kreis **verloren** wie ein verlorenes Maß: er bleibt mit Warnzeichen sichtbar, die Ausgabe ist gesperrt, bis er gelöscht oder neu zugewiesen ist (gleiche Regel wie bei Maßen in 3b).
- **Nummer:** die Position der Körper-Zeile in der Stückliste (bei gleichnamigen Körpern dieselbe Nummer). Die Nummer im Kreis ist nicht frei editierbar (sie folgt der Stückliste).
- **Zeichnung:** Kreis Ø 8 mm (skaliert mit dem Blattfaktor), Linienstärke 0,35, Zahl 3,5 mm; Hinweislinie 0,25 mit einem kleinen Punkt am Teil. Maße in mm nach ISO-Praxis; **nicht aus der Norm belegt**, siehe Abschnitt 8.
- **Mehrere Kreise je Teil** sind erlaubt (ein Kreis je Ansicht ist üblich). Auf dem Stücklistenblatt gibt es keine Nummernkreise.

## 5. Bedienung

- **Fenster „Blattangaben“** (3c-1) bekommt einen vierten Abschnitt **„Stückliste“**: Schalter „Stückliste auf diesem Blatt“, die Zeilenliste (Pos. nur zum Lesen, Benennung, Menge, Werkstoff, Bemerkung als Textfelder mit „automatisch“ und „Zurücksetzen“), Warnung bei mehr als 10 Zeilen, Knopf „Stücklistenblatt erzeugen“ (bzw. „Stücklistenblatt öffnen“, wenn es schon existiert). Die Zeilen sind blattübergreifend gemeinsam (Stückliste gilt für das Dokument, die Texte werden **im Dokument** gespeichert; **Entscheidung des Entwurfs**: eine Stückliste je Dokument, nicht je Blatt).
- **Zeichnungsleiste:** Werkzeug „Position“ neben „Maß“ (Platz in Zeile 2; 1100×700 darf weiter nicht rollen). Browser links: Abschnitt „Positionen“ (Liste der Kreise je Ansicht, Auswahl, Löschen, verlorene mit Warnzeichen) wie der Maß-Browser.
- **Texte** in einfacher Laiensprache.

## 6. Ausgabe und Datei

- SVG und DXF: Stückliste (Rahmenlinien, Texte) und Nummernkreise als Markierungen wie die Maße; mit „Blattrahmen mit ausgeben“ aus 3c-1 wird die Tabelle mitgeschaltet (Tabelle gehört zum Blattrahmen, die Nummernkreise gehören zu den Ansichten und bleiben immer). Ein verlorener Nummernkreis sperrt die Ausgabe mit benannter Meldung (Muster der Maße).
- **Dateiformat 7**, nur wenn das Dokument Stücklistendaten (Textüberschreibungen, Schalter abweichend von der Vorbelegung, Stücklistenblatt) oder Nummernkreise enthält; sonst bleibt es bei Format 6/5/4 (Muster aus 3c-1). Ältere Programmstände verweigern Format 7 mit benannter Meldung.
- Grenzen: höchstens 10 Zeilen auf dem Zeichnungsblatt; Stücklistenblatt höchstens 200 Zeilen insgesamt; höchstens 200 Nummernkreise je Zeichnung; Texte wie in Abschnitt 3.

## 7. Qualität und Prüfung

- Stücklisten-Ableitung (Gruppieren, Menge, Pos.), Vorbelegung und Zurücksetzen, Grenzen; Geometrie der Tabelle auf dem Blatt (A4 bis A0, hoch und quer) mit exakten Zahlen; freier Bereich nie leer, keine Überlappung mit Schriftfeld, Änderungstabelle, Hinweisfeld, Stückliste (Eigenschaftstests über Papiere, Zeilenzahlen 0..=12, Änderungen 0..=3, Hinweise 0..=6).
- Stücklistenblatt: Seitenaufteilung (Zeilen je Blatt je Papier), Folgeblätter entstehen und verschwinden, Blatt-n/N-Zählung, Löschen mit Folgeblättern.
- Positionsnummern: Setzen per Klick auf alle Linienarten, Verschieben, Löschen, Zuordnung über Neuberechnung und Umordnen der Körper, verloren bei Löschen des Körpers, Sperre der Ausgabe, Datei-Rundlauf (Format 7, alte Dateien unverändert), SVG/DXF-Ausgabe (gültiges XML, Handles, Texte maskiert).
- Anzeige in der Zeichnungsansicht bei 800×600 und großem Fenster, Eingabe im Fenster ohne Tastenkürzel-Konflikte.
- Handliste für den Nutzer; **eigener Test im echten Fenster durch den Controller vor der Merge-Frage** (`tools/gui-drive.ps1`, mit Freigabe des Nutzers).
- Geschützte Pfade (Kernel, Netz-Quelltext, STEP, Austausch) unverändert; keine neuen Abhängigkeiten.

## 8. Offene Annahmen (vom Entwurf getroffen, vom Nutzer überstimmbar)

- Gleich = gleicher Name; Werkstoff aus Projektangabe, je Zeile änderbar.
- Eine Stückliste je Dokument; Texte im Dokument gespeichert.
- Auf dem Zeichnungsblatt Positionen von unten nach oben, auf dem Stücklistenblatt von oben nach unten.
- 10 Zeilen reserviert, solange der Schalter an ist.
- Kreis Ø 8 mm (mal Blattfaktor), Schrift 3,5 mm, Hinweislinie mit Punkt: nach Praxis, nicht aus der Norm belegt (Normen kostenpflichtig, nicht verfügbar). Abweichungen werden dokumentiert, wenn der Nutzer die Norm prüft.
- Nummer im Kreis nicht frei editierbar.

## 9. Festlegungen des Entwurfs nach dem Fremdlesen (vom Nutzer überstimmbar)

- **Körper-Kennung (F1):** Zeilen und Nummernkreise merken den **Ursprungs-Körper**, also das Feature, das den Körper ursprünglich erzeugt hat (so wie die App den Körpernamen schon über Verschieben und Verrunden zurückverfolgt). Verrunden und Verschieben ändern die Kennung nicht. Bei einer Booleschen Operation gehört das Ergebnis dem Ziel-Körper; Nummernkreise am Ziel bleiben, Nummernkreise am Werkzeug-Körper (der im Ergebnis aufgeht) werden **verloren**. Die Texte der Zeilen (Benennung, Werkstoff, Bemerkung) werden im Dokument unter dieser Kennung gespeichert, nicht nach Position oder Name, damit sie Umordnen und Umbenennen überstehen. Die Ableitung der Stückliste (Gruppieren, Menge, Namen) gehört in `planora-feature`, nicht in die App.
- **Ankerpunkt (F2):** ein Nummernkreis ist an einen **Körper** gebunden, nicht an eine einzelne Kante. Der Ankerpunkt wird relativ zur Ansicht gespeichert (0…1 der Ansichtsbreite und -höhe), damit er Größenänderungen folgt; er wird nicht über Kanten nachgeführt. Verschwindet eine Kante, bleibt der Kreis am Körper und am gespeicherten Punkt. Nur das Verschwinden des Körpers macht den Kreis verloren.
- **Folgeblätter (F3):** Das Stücklistenblatt und seine Folgeblätter („Stückliste“, „Stückliste (2)“, …) sind **echte Zeichnungen** (Blatt-Typ „Stückliste“, Seitennummer). Sie entstehen und verschwinden automatisch mit der Zeilenzahl. „Blatt n von N“ zählt jedes Blatt. Jedes hat seine eigenen Blattangaben (Schriftfeld), Titel und Nummer werden automatisch fortgeschrieben, der Nutzer kann sie wie andere Felder überschreiben. Ein einzelnes Folgeblatt lässt sich nicht löschen oder umbenennen; das erste Stücklistenblatt löschen entfernt alle. Export: jedes Stücklistenblatt ist eine eigene Zeichnung und wird wie jede Zeichnung im Export-Fenster gewählt.
- **Schalter (F4):** Die Vorbelegung (an bei mehr als einem Körper) wird **einmal beim Anlegen der Zeichnung** festgelegt und im Blattangaben-Satz gespeichert; sie folgt der Körperzahl danach nicht mehr von selbst. Der Nutzer schaltet von Hand. Alte Zeichnungen (Format 6 und älter) haben die Stückliste **aus**, und nichts ändert sich beim Öffnen. Ein gespeicherter Schalter macht die Datei zu Format 7.
- **Umbenennen (F5):** Ein Nummernkreis bleibt am Körper, auch wenn der Körper umbenannt wird; die Nummer richtet sich nach der Stücklistenzeile (Gruppierung nach dem Körpernamen, nicht nach einer überschriebenen Benennung). Verschmelzen zwei Körper durch Umbenennen zu einer Zeile, tragen ihre Kreise dieselbe Nummer.
- **Netz-Körper (F6):** Importierte Netz-Körper stehen **nicht** in der Stückliste, weil sie nicht in Zeichnungen erscheinen. Das steht als Grenze in der Doku.
