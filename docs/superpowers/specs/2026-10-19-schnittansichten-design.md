# Schnittansichten (3d) – Entwurf

Stand 2026-10-19. Teil 3 der Austausch-Strecke, letzter Schritt nach 3a Ansichten, 3b Maße und Mittellinien, 3c-1 Zeichnungsblatt, 3c-2 Stückliste, 3c-3 PDF. **Alle Werte unten sind Vorschläge mit Standardwerten**; Dennis korrigiert, was nicht passt. Was von mir geraten ist (nicht von Dennis gesagt), ist als „Vorschlag“ gekennzeichnet; nichts davon ist beschlossen.

Die Normen (ISO 128-40/-44, ISO 128-50 Schraffur, DIN 6) liegen nicht vor. Wie beim Blatt gilt: Die Werte stammen aus gängiger Praxis und öffentlichen Zusammenfassungen; Abweichungen von der Norm kommen in `offene-punkte.md`.

## 1. Zweck

Ein Teil mit Hohlräumen (Bohrung, Tasche, Gehäuse) zeigt in den Normansichten nur gestrichelte verdeckte Kanten. Eine Schnittansicht schneidet das Teil an einer Ebene auf, zeigt die Schnittfläche **schraffiert** und die dahinterliegenden Kanten als sichtbare Linien. Die Schnittlinie wird in der Ansicht, aus der man schneidet, als Strich-Punkt-Linie mit Pfeilen und Buchstaben („A–A“) markiert; die Schnittansicht trägt dieselbe Beschriftung.

## 2. Umfang

**Ziele (Vorschlag):**
- **Vollschnitt** an einer geraden Ebene durch die ganze Ansicht (der häufigste Fall).
- **Schnittebene** ist eine der Hauptebenen durch einen frei wählbaren Abstand: in einer Normansicht (Vorne, Oben, Links …) zieht man eine gerade Schnittlinie senkrecht zur Blattachse; sie legt Ebene und Blickrichtung fest. Standard: durch die Mitte der Hüllbox des Teils (trifft Bohrungen mittig).
- **Schraffur** 45°, Linienabstand 3 mm bei Referenzblatt A3 (skaliert mit dem Blatt wie das Schriftfeld, Faktor f), dünne Linie. Eine Schraffurrichtung je Körper; bei mehreren Körpern in einer Ansicht wechselt die Richtung (45° / 135°) und der Abstand bleibt, damit angrenzende Teile unterscheidbar sind.
- **Ausgeschnittene Körper**: Alle Körper der Zeichnung werden an derselben Ebene geschnitten.
- **Beschriftung**: Buchstaben A, B, C … in Reihenfolge, Schnittansicht heißt „Schnitt A–A“ unter der Ansicht. Maßstab der Schnittansicht = Maßstab der Quellansicht (Vorschlag; eigener Maßstab später).
- **Ausgabe** in SVG, DXF und PDF, Schraffur als Linien (kein Muster-Füllobjekt), damit alle drei Formate dasselbe zeigen.
- **Maße und Mittellinien** in der Schnittansicht wie in jeder Ansicht (vorhandene Funktionen laufen weiter).

**Nicht-Ziele (Vorschlag):** Halbschnitt, Teilschnitt (Ausbruch), Stufenschnitt/versetzte Ebenen, Winkelschnitt (schräge Ebene), Detailansicht, Rippen und Wellen nicht schraffieren (Norm-Sonderfall), Werkstoffabhängige Schraffurmuster, Schnittverlauf-Pfeile mit Beschriftung per Hand verschieben, Schnitt durch Netzkörper (nur exakte Körper wie bisher).

**Anti-Ziel:** Eine Schnittansicht, die stillschweigend falsch ist (zum Beispiel Schraffur über Löchern). Lieber eine deutsche Meldung und kein Ergebnis.

## 3. Ansatz

- **Schneiden mit dem vorhandenen Kernel.** Der Körper wird mit einem großen Quader (die Hälfte hinter der Schnittebene) durch `planora_kernel::boolean(.., BoolOp::Subtract)` abgetrennt. Das Ergebnis ist ein gewöhnlicher exakter Körper; die bestehende Ansichtsberechnung (`compute_view`) projiziert ihn ohne Änderung. Vorteil: Sichtbarkeit, Umrisse, Mittellinien und alle Ausgaben bleiben unverändert.
- **Schnittfläche finden:** Die neuen ebenen Flächen, die in der Schnittebene liegen, sind die Schnittflächen. Ihre Randschleifen (außen und Löcher) werden in die Ansichtsebene projiziert.
- **Schraffur:** Parallele Linien im gewählten Winkel werden gegen die Schleifen der Fläche zugeschnitten (gerade-ungerade-Regel, Löcher bleiben frei) und als `Mark::Line` mit neuem `MarkKind::Hatch` ausgegeben. Zuschnitt in 2D, kein Kernel nötig.
- **Rechenaufwand:** Eine Schnittansicht kostet ein boolesches Schneiden je Körper plus eine Ansichtsberechnung; sie läuft wie jede Ansicht im Hintergrund mit Fortschritt und Abbruch.
- **Scheitert das Schneiden** (Kernel meldet Fehler, zum Beispiel bei Spline-Flächen), zeigt die Ansicht eine Meldung „Bei „Körper X“ lässt sich der Schnitt nicht berechnen“ und die Ausgabe ist gesperrt wie bei anderen fehlerhaften Ansichten.

## 4. Bedienung (Vorschlag)

- Werkzeugleiste der Zeichnung: Knopf **„Schnitt“** (neben „Ansicht hinzufügen“). Klick, dann in einer Ansicht zwei Klicks (Anfang, Ende) für die Schnittlinie; sie rastet auf achsparallel ein und zeigt eine Vorschau mit der Blickrichtung. Pfeilrichtung ändert man im Eigenschaftenfeld („Blickrichtung umdrehen“).
- Es entsteht automatisch eine **abhängige Ansicht** (wie die bisherigen Folgeansichten, im Raster ausgerichtet) mit Schnittbuchstabe.
- Eigenschaftenfeld der Schnittansicht: Schnittposition (Zahl in mm, änderbar), Blickrichtung umdrehen, Schraffur an/aus, Schraffurabstand (Vorgabe 3 mm × f), Winkel (45°/135°).
- Ändert sich das Modell, folgen Schnittlinie und Schnittansicht. Liegt die Ebene außerhalb des Teils, steht „Die Schnittebene trifft den Körper nicht“ in der Ansicht.
- Löschen der Schnittansicht löscht die Schnittlinie in der Quellansicht (und umgekehrt) nach Rückfrage wie beim Löschen anderer abhängiger Ansichten.

## 5. Darstellung

- Schnittlinie: Strich-Punkt-Linie, an den Enden dick und abgewinkelt (Vorschlag: Endstücke 8 mm × f), Pfeile 90° zur Linie in Blickrichtung, Buchstabe neben jedem Pfeil (Texthöhe wie Maßtext).
- Schnittansicht: Titel „A–A“ unter der Ansicht, Maßstab dahinter. Verdeckte Kanten in der Schnittansicht Vorgabe **aus** (Norm: in Schnitten werden sie üblicherweise weggelassen).
- Schraffur dünn (Stift wie Maßlinien), Schnittflächenkontur dick wie jede sichtbare Kante.
- Schnittflächen, die dasselbe Teil an mehreren Stellen treffen, werden einheitlich schraffiert (gleicher Winkel und Phase), damit zusammengehörige Flächen zueinander passen.

## 6. Datenmodell und Datei

- `ViewKind` bleibt für die Normansichten. Neu: `ViewSpec.section: Option<SectionSpec>` mit `{ offset: f64, letter: char, hatch: HatchSpec }`. Quelle ist das vorhandene `ViewSpec.parent`, die Blickrichtung ergibt sich aus `kind` (Umdrehen = die Gegenansicht, z. B. Hinten statt Vorne). Eine Schnittansicht ist eine abhängige Ansicht mit gesetztem `section`.
- **Dateiformat 8** (Zeichnung erhält optionale Schnitte); Format-7-Dateien laden unverändert. `required_format` springt auf 8 nur, wenn Schnitte vorhanden sind. Die Tests, die „Format 8 ist zu neu“ prüfen (`sheet_format`, `parts_format`, `format3`), werden angepasst.
- Buchstaben vergibt die Zeichnung (A, B, … ohne I, O, Q, wie üblich); fehlen Buchstaben (mehr als 23 Schnitte), steht eine Meldung.

## 7. Grenzen und Fehler (Vorschlag)

- Nur Körper, die der Kernel booleschen Operationen zugänglich macht; Netzkörper werden übersprungen mit Meldung (wie bisher bei Ansichten).
- Schnitt genau durch eine Fläche oder Kante (tangential): der Schnitt wird um 0,001 mm verschoben und im Eigenschaftenfeld vermerkt; schlägt das fehl, Meldung.
- Höchstens 12 Schnittansichten je Blatt.
- Die Schraffur zählt zu den Ausgabegrenzen (Strichzahl); zu feiner Abstand (< 0,5 mm auf dem Blatt) wird abgelehnt.

## 8. Tests und Prüfung

- Kernel-Schnitt von Quader, Zylinder, Quader mit Bohrung, Hohlzylinder, Körper mit Nut: Schnittflächen-Anzahl, Fläche in mm², Löcher bleiben frei (Schraffurlänge je Zeile gegen Handrechnung).
- Schraffur-Zuschnitt: konvex, konkav, Loch, mehrere Teilflächen, Linienabstand bei Maßstab 1:2 und 5:1, Blatt A4 und A0 (Skalierung f).
- Ausgabe: SVG-, DXF- und PDF-Golden einer Schnittansicht (Zylinder mit Bohrung).
- Oberfläche: Schnittlinie ziehen, Buchstabenvergabe, Löschen, Modelländerung, Datei speichern und laden (Format 7 und 8).
- Handliste mit echtem Fenster nach der Umsetzung.

## 9. Festlegungen aus der Vorabprüfung (meine Entscheidungen, Dennis darf widersprechen)

1. **Schneidekörper:** Quader aus der Hüllbox des Körpers mal Faktor 2, selbst auf die Ebene gelegt (der Kernel-Quader hat eine Ecke im Ursprung). Eigene Herkunft per `assign_origin`, damit seine Flächenmarken („Deckel“, „Wand n“) nicht mit Körperflächen kollidieren; so erkennt die Zeichnung die **Schnittflächen an der Marke**, nicht an der Lage.
2. **Körper ganz hinter oder vor der Ebene:** verschwindet bzw. bleibt ohne Meldung (kein Fehler „Schnitt lässt sich nicht berechnen“). Nur ein echter Kernel-Fehler gibt die Meldung.
3. **Maße und Mittellinien:** Ein geschnittener Körper ist ein neuer Körper mit anderen Flächen- und Kantennummern; bestehende Maße der Quellansicht gelten dort nicht. **In Schnittansichten gibt es zunächst keine übernommenen Maße**; Mittellinien werden neu berechnet, und „Maße vorschlagen“ und Maß setzen arbeiten auf der geschnittenen Geometrie. (Das ersetzt die Zeile „Maße wie in jeder Ansicht“ in Abschnitt 2 im Sinne von: dieselben Werkzeuge, aber neue Maße.)
4. **Mehrere Schalen oder Schleifen:** Ein Schnitt kann ein Teil in mehrere Stücke teilen; Zuschnitt und Schraffur arbeiten mit beliebig vielen Schleifen je Ebene.
5. **Ausgabe:** Schraffur und Schnittlinie sind eigene Markenarten und kommen in SVG, DXF und PDF neu hinzu. Schnittlinie und Pfeile sind Marken der **Quellansicht**. Die Schnittpfeile bekommen eine eigene Größe (nicht `ARROW_LEN`).
6. **Strichgrenze:** Schraffurlinien zählen getrennt von den Kantenstrichen (`MAX_STROKES` 20 000 gilt nur für Kanten), mit eigener Obergrenze.
7. **Schnitt aus Schnittansicht:** nicht erlaubt (Schnitt nur aus Normansichten); der Knopf ist dort ausgegraut.
8. **Ausgeblendete Körper** (in der Zeichnung nicht sichtbar) werden nicht geschnitten.
9. **Schraffurwinkel:** je Körper automatisch wechselnd (45°/135°) in der Reihenfolge der Körper; im Eigenschaftenfeld nur Abstand und „Schraffur an/aus“. Berühren sich Teile in der Ebene, bleiben sie getrennt schraffiert (Phase je Körper).
10. **Kleinigkeiten:** Schnitt wird nicht neu berechnet, wenn sich nur der Schraffurabstand ändert; Tangentialfall wie in Abschnitt 7; Grenzen 12 Schnitte je Blatt, Buchstaben A–Z ohne I, O, Q (23).

## 10. Offen (Vorschläge, bitte korrigieren)

1. Reicht der **Vollschnitt** für den ersten Wurf? (Halbschnitt ist bei symmetrischen Teilen häufig; Vorschlag: eigenes Teilprojekt danach.)
2. **Schraffurabstand 3 mm** (A3) und **45°** passen?
3. Verdeckte Kanten in Schnittansichten **aus**?
4. Schnittbuchstaben **A, B, C …** in Reihenfolge der Anlage?
5. Schnittlinie **per zwei Klicks** in einer Ansicht (statt Wert eintippen)?
