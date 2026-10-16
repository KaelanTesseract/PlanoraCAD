# Teilprojekt 12: Zeichnungen – Maße und Mittellinien (3b) – Entwurf

Stand: 2026-10-15. Baut auf Teilprojekt 11 (Zeichnungen, Ansichten; `master`, Merge db80de3) auf. Teil 3b der Zeichnungs-Strecke
(3a Ansichten, **3b Maße und Mittellinien**, 3c Blatt: Schriftfeld, Texte, Stückliste, 3d Schnittansichten).

## 1. Ziel und Abgrenzung

Zeichnungen bekommen Maße (linear, Durchmesser, Radius, Winkel, Maßketten und Bezugsmaße, Toleranzen) und Mittellinien. Maße hängen
an der Modellgeometrie und rechnen sich nach Änderungen neu; ein Knopf schlägt die Grundmaße vor. SVG und DXF enthalten Maße und
Mittellinien als einfache Zeichenelemente.

**Entscheidungen (mit dem Nutzer abgestimmt, 2026-10-15):**

| Nr. | Entscheidung |
|---|---|
| M1 | Aufteilung des Rests von Teil 3: **3b Maße und Mittellinien (dieses Teilprojekt)**, 3c Blatt (Rahmen mit Heftrand, Schriftfeld, freie Texte, Positionsnummern, Stückliste), 3d Schnittansichten. |
| M2 | Maße entstehen **von Hand und automatisch**: Werkzeug „Maß“ in der Ansicht; „Maße vorschlagen“ setzt die Grundmaße, alles ist danach änderbar. |
| M3 | Maßarten: **linear** (waagerecht, senkrecht, schräg), **Durchmesser, Radius, Winkel**, **Maßketten und Bezugsmaße**, **Toleranzen** (± symmetrisch, oben/unten, Grenzmaße, Passung nur als Text, siehe M9). |
| M4 | Maße **hängen an der Modellgeometrie** (stabile Verweise auf Kanten, Flächen, Eckpunkte), gezeigt in **einer Ansicht**. Geht der Bezug verloren, bleibt das Maß erhalten und ist **rot markiert**. |
| M5 | Maße gibt es nur in **Normansichten**; die Isometrie bleibt ohne Maße. |
| M6 | Ausgabe als **Linien, gefüllte Pfeile und Text** (SVG: `line`/`path`/`text`; DXF: `LINE`/`SOLID`/`TEXT`); **keine** echten DXF-`DIMENSION`-Objekte. |
| M7 | **Mittellinien und Achsenkreuze** (Strichpunktlinien) werden aus dem Modell berechnet, je Ansicht an- oder abschaltbar, und sind Maßbezüge. |
| M8 | Darstellung fest nach DIN/ISO 129 (vereinfacht): Texthöhe 3,5 mm, Pfeillänge 3,5 mm, Hilfslinienlücke 1 mm, Überstand 2 mm, Abstand der Maßlinien 7 mm, Linienbreite 0,25 mm – alles in **Blatt-Millimetern**, unabhängig vom Maßstab. Der angezeigte Wert ist der Modellwert in mm. |
| M9 | **Passungen** (z. B. H7) nur als Textzusatz, ohne Toleranztabellen nach ISO 286; die Tabellen sind ein späteres Teilprojekt. |
| M10 | Keine neue externe Abhängigkeit, Schrift in der Oberfläche die vorhandene (Roboto), in Dateien die Standardschrift des Programms (SVG `sans-serif`, DXF `STANDARD`). Lizenz: Normen werden nicht abgeschrieben, nur Zahlenwerte der Darstellung. |

**Nicht Teil:** Maße in der Isometrie, Maße in Schnitten, ISO-286-Passungstabellen, echte DXF-Maße, Maßstil-Verwaltung (mehrere Stile),
Oberflächenangaben, Form- und Lagetoleranzen, Schweißsymbole, Gewindedarstellung, Schriftfeld/Texte/Stückliste (3c).

## 2. Datenmodell (im Dokument, gespeichert)

Neu in `planora-drawing::model`:

```
DimId(u64)
DimKind     { Horizontal, Vertical, Aligned, Diameter, Radius, Angle }
GeoRef      { Edge(GeoRefData), EdgeEnd { edge: GeoRefData, end: Start|End }, Center(GeoRefData),
              Face(GeoRefData) /* Zylinder/Kegel/Kugel: Achse bzw. Mittelpunkt */ }
Tolerance   { None, Symmetric(f64), Limits { upper: f64, lower: f64 } /*oben/unten*/,
              LimitSizes { upper: f64, lower: f64 } /*Grenzmaße: Höchst-/Mindestmaß statt Nennmaß*/, Fit(String) }
DimSpec     { id, view: ViewId, kind, refs: Vec<GeoRef>, offset: f64 /*Blatt-mm, Maßlinie vom Maßziel*/,
              angle: f64 /*Grad, Führungslinie bei Ø/R, Vorgabe 45*/, text_shift: (f64, f64) /*Blatt-mm vom Standardort*/, tolerance, decimals: u8 /*0..=3*/,
              text_override: Option<String>, chain: Option<ChainSpec>, suggested: bool }
ChainSpec   { group: u64, mode: Chain | Baseline, order: u32 }
ViewSpec  += centerlines: bool                     // Mittellinien anzeigen
Drawing   += dimensions: Vec<DimSpec>, next_dim: u64
```

- **Verweise:** `planora-drawing` darf `planora-feature` nicht kennen. Deshalb bekommt `planora-drawing` eine eigene Datenstruktur
  `GeoRefData { kind, tag(s): Marke(n) + Hinweispunkt(e) }` (dieselben Felder wie `FaceRef`/`EdgeRef`, aber ohne Abhängigkeit); das
  Anlegen aus einer Kante/Fläche und das **Auflösen** gegen die Körper übernimmt `planora-feature` (dort liegen `EdgeRef`,
  `FaceRef`, `resolve_edge`, `resolve_face`). Die Feature-Seite wandelt `GeoRefData` ⇄ `EdgeRef`/`FaceRef` verlustfrei um.
- Das Maß gehört zu **einer Ansicht** (`view`), die Bezüge zeigen auf Modellgeometrie; zeigt die Ansicht den Bezug nicht (Fläche
  steht schräg, Kante entfällt), gilt das Maß als verloren (rot).
- Maßketten: aufeinanderfolgende lineare Maße einer Gruppe (`Chain`: gemeinsame Maßlinie, Maße nebeneinander; `Baseline`: alle von
  einem Bezugspunkt, Maßlinien gestapelt im Abstand von 7 mm).
- **Speichern:** `FORMAT_VERSION = 5`; Format 5 wird nur geschrieben, wenn mindestens eine Zeichnung Maße enthält oder eine Ansicht
  `centerlines = true` hat. Altdateien (Format 4) laden mit `centerlines = false` und bleiben unverändert, solange nichts geändert
  wird. Neue Normansichten haben `centerlines = true`, die Isometrie `false`.

## 3. Berechnung

Alle Berechnungen sind reine Funktionen im Paket `planora-drawing` (kein UI-Zugriff).

1. **Auflösen:** (berichtigt, siehe Festlegungen) `planora-feature` löst die Bezüge zu `Geo3` auf (Punkt, Strecke, Kreis mit Mitte/Normale/Radius, Achse mit Bereich `t0`/`t1`, Kugel); `planora-drawing::measure(kind, &[Geo3], &Basis, ViewKind, extent) -> Result<Measured, DimError>` projiziert mit der Ansichtsbasis und liefert Wert und Anker (`Anchor::Linear`/`Round`/`Angle`).
2. **Werte:**
   - `Horizontal`/`Vertical`: Abstand der projizierten Bezüge entlang `ex` bzw. `ey`; eine Kante muss quer zur Messrichtung stehen
     (Fehler „Die Kante steht nicht quer zum Maß“).
   - `Aligned`: Abstand in der Bildebene zwischen zwei Punkten oder (parallelen) Kanten.
   - `Diameter`/`Radius`: aus Kreis/Bogen (Blick in Achsrichtung → Kreis, Führungslinie) oder aus Zylinder-/Kegel-/Kugelfläche
     (Blick quer zur Achse → Abstand der beiden Umrisslinien, Maß mit „Ø“).
   - `Angle`: Winkel zwischen zwei projizierten Geraden (0–180°).
   Der Wert ist der **wahre** Modellwert, solange die Messrichtung in der Bildebene liegt (Normansichten); sonst Fehler. Der Anker eines linearen Maßes trägt je Bezug beide Enden quer zur Messrichtung (`a_span`/`b_span`); die Darstellung nimmt das der Maßlinie nähere Ende für die Hilfslinie. `Angle` wird abgelehnt, wenn eine Kante nicht parallel zur Bildebene liegt.
3. **Verloren:** Bezug nicht mehr auflösbar, mehrdeutig oder nicht sichtbar → `status = Lost(Grund)`; die letzte bekannte Lage bleibt
   gespeichert (Hinweispunkte), das Maß wird rot gezeichnet, ohne Wert („?“).
4. **Mittellinien:** je Ansicht aus den sichtbaren Flächen: Kreiskante mit Achse ∥ Blickrichtung → Achsenkreuz (zwei Linien, 3 mm
   Überstand); Zylinder/Kegel mit Achse ⟂ Blickrichtung → Mittellinie entlang der Achse mit 3 mm Überstand; Kugel → Achsenkreuz;
   Torus: Achsenkreuz im Blick entlang der Achse. Konzentrische Kreuze werden zu einem, überlappende kollineare Linien verbunden. Die vorgesehene Begrenzung am Rand der Ansicht ist **nicht umgesetzt** (Linien stehen 3 mm über die Kanten hinaus, werden aber nicht gekürzt); Folgearbeit. Keine Mittellinien, wenn die Blickrichtung keine Weltachse ist.
5. **Vorschlag:** `suggest(bodies, views, existing) -> Vec<Candidate>` (die Oberfläche macht daraus `DimSpec` mit `suggested = true`): (a) **Gesamtmaße** (Breite/Höhe
   der sichtbaren Linien der Ansicht, nur wenn von Kanten stützbar), (b) **Ø/R** für jeden vollen Kreis, jede Bohrung, jeden Zylinder
   und jede Rundung in der Ansicht, (c) **Bohrungsmitten** als Bezugsmaße von der unteren linken Kante. Ein Maß wird im ganzen
   Blatt **nur einmal** gesetzt (gleicher Bezug, gleicher Wert → die Ansicht mit der besten Lage gewinnt: Draufsicht für Ø von
   Bohrungen, Vorderansicht für Höhen). Bereits vorhandene Maße werden nicht verdoppelt.

## 4. Darstellung und Ausgabe

- `annotate.rs` erzeugt aus Maßen und Mittellinien **Markierungen**: `Mark::{Line, Arrow { tip, dir }, Text { pos, height, angle,
  anchor, string }, CenterLine { a, b }}` in Blattkoordinaten; `SheetView` bekommt `marks: Vec<Mark>`.
- **Maßdarstellung:** Hilfslinien mit 1 mm Lücke und 2 mm Überstand, Maßlinie mit Pfeilen (innen, bei zu wenig Platz außen),
  Text mittig über der Maßlinie (mit `text_shift` verschoben); Ø/R mit Führungslinie schräg aus dem Kreis, Winkel mit Bogen;
  Toleranz: `±0,1`, `+0,2 / −0,1` (gestapelt, kleiner), Grenzmaße `10,1 / 9,9`, Passung `Ø10 H7` als Text.
- **Texte:** Ziffern und Sonderzeichen: Ø, °, ±, −. SVG schreibt Unicode; DXF `%%c`, `%%d`, `%%p`; das Minus steht in der DXF-Datei als `-`; Latin-1-Zeichen als `\U+XXXX`, `%` als `%%%`, `\` als `\U+005C`.
- **SVG:** eigene Gruppe `id="Maße"` je Ansicht; Pfeile als gefüllte `path`; Text `font-size` in mm. **DXF:** Ebenen `MASSE`
  (Linientyp CONTINUOUS, Farbe 7), `MITTEL` (Linientyp CENTER, Muster 12/−3/2/−3 mm); Pfeile als `SOLID` (Dreieck), Texte als `TEXT`.
  Alles im Block der Ansicht (wie 3a).
- Die Plot-Stil-Kette, exaktes 2π und alle weiteren Fusion-Erfahrungen aus 3a bleiben bestehen; DXF `TEXT` und `SOLID` werden im
  Handtest in Fusion 360 geprüft.

## 5. Oberfläche

- Werkzeug „Maß“ in der Werkzeugleiste des Zeichnungsbereichs (Untermodi: Linear, Ø, R, Winkel, Kette, Bezug): Klick auf Linie/Kreis
  (Treffer wie bisher ≤ 6 px), zweiter Bezug, dann die Maßlinie an die Stelle ziehen; Esc bricht ab. Der Klick liefert den Bezug
  aus dem Ursprung der Linie (`SheetLine` bekommt `origin: StrokeOrigin`).
- Auswahl und Verschieben eines Maßes (Maßlinienabstand, Textverschiebung); Doppelklick öffnet das Fenster „Maß bearbeiten“ (Wert,
  Toleranzart und -werte, Nachkommastellen, Text überschreiben, Löschen).
- Knöpfe „Maße vorschlagen“ (ganze Zeichnung oder gewählte Ansicht) und je Ansicht „Mittellinien zeigen“.
- Browser: Abschnitt „Maße“ unter der Zeichnung (Name, Wert, rot bei verlorenem Bezug); Löschen mit Rückfrage bei Maßketten.
- Alle Texte deutsch, Laiensprache; Maßänderungen laufen über `Document::update_drawing` (Maße ändern **nie** `model_edit_count`).
- Berechnung der Ansichtslinien bleibt der Hintergrundjob aus 3a; Auflösen/Darstellen der Maße ist billig und läuft im
  Oberflächenfaden (Grenze: höchstens 2000 Maße je Zeichnung, `MAX_DIMENSIONS`).

## 6. Fehler und Grenzen im Betrieb

`DimError`: `NotAcross`, `NotVisible`, `Ambiguous`, `Lost(String)`, `NotAllowedInIsometric`, `TooMany`. Alle mit deutschem Text; die
Oberfläche zeigt sie am Maß bzw. in der Statuszeile. Maße verschwinden nie stillschweigend.

## 7. Prüfung (Tests und Werkzeuge)

- **Formelmodelle** für Quader, Zylinder, Kugel, Kegelstumpf, Bohrung, Winkelprofil in allen Normansichten: erwartete Werte aus
  den Eingabemaßen (nicht aus der Bibliothek), Maßlinienlage und Pfeilrichtung als Formeln.
- **Eigene Leser:** SVG (Pfade, Text) und DXF (LINE/SOLID/TEXT, Ebenen, Linientyp CENTER) – Rücklesetests; die bestehende
  Strukturprüfung der DXF-Datei wird um die neuen Ebenen und Objekte erweitert.
- **Verlust und Neuberechnung:** Maße nach Änderung der Quadergröße (Wert folgt), nach Löschen der Kante (rot), nach Drehung der
  Ansicht (Fehler statt falschem Wert).
- **Serie:** 200 Zufallskörper aus 3a: Vorschlag setzt nur Maße mit Wert = Gesamtmaß/2r laut Formel; keine Doppelungen;
  Mittellinien liegen auf der Achse (Abstand ≤ geom_tol).
- **Mutationsbeweise** für jeden Regressionstest; **Handtest in Fusion 360** (SVG Text/Pfeile, DXF `TEXT`/`SOLID`/CENTER).

## 8. Stufen

1. Datenmodell und Verweise (Auflösung der Bezüge), Werte aller Maßarten.
2. Mittellinien (Berechnung, Darstellung als Marken).
3. Maßdarstellung (`annotate.rs`: Linien, Pfeile, Text, Toleranztexte), Maßketten und Bezugsmaße.
4. SVG-/DXF-Ausgabe der Marken mit eigenen Lesern.
5. Vorschlag der Grundmaße.
6. Speichern/Laden (Format 5), Dokument-Operationen.
7. Oberfläche: Werkzeug, Bearbeiten, Browser, Export.
8. Dokumentation, Handtest, Übergabe.

## 9. Bekannte Grenzen (zur Übernahme in README und offene-punkte)

- Keine Maße in der Isometrie; Passungen nur als Text; keine echten DXF-Maße.
- Maßzeichen und Schrift sind einfach (Standardschrift); sehr kleine Maße (Maßlinie < 6 mm) werden mit Pfeilen außen gesetzt, nicht
  mit Maßpunkten.
- Der Vorschlag setzt Grundmaße, kein vollständiges Fertigungsbemaßung; Sonderfälle (Gewinde, Fasen als Winkel×Länge) folgen.
- Maße an Freiformflächen nur über Kanten und Mittellinien.

## 10. Festlegungen der Umsetzung

Stand: 2026-10-16. Die Abschnitte 1 bis 9 sind der Entwurf; bei Planung und Umsetzung wurde Folgendes entschieden oder berichtigt. Wo ein früherer Abschnitt anders lautet, gilt dieser. Die Aufgabenfolge und ihre Berichte stehen im Plan (`plans/2026-10-15-masse-mittellinien.md`).

### 10.1 Festlegungen der Planung (P1 bis P13)

- **P1 Geometrie-Typ `Geo3`:** `planora-feature` löst `GeoRefData` zu `Geo3` auf: `Point(DVec3)`, `Line { a, b }` (Strecke mit Enden), `Circle { center, normal, radius }`, `Axis { origin, dir, radius, t0, t1 }` für Zylinderflächen (**berichtigt:** mit dem gezeichneten Bereich `t0 ≤ t1` entlang der normierten Richtung; Enden `origin + t0·dir` und `origin + t1·dir`; gefüllt aus den Randkanten der Fläche mit `centerline::axial_range`), `Sphere { center, radius }`. `planora-drawing` kennt nur diese Typen, nicht die Körper. Die Messung bekommt `&[Geo3]` plus `Basis` und liefert Wert und Anker.
- **P2 Verweisdaten:** `GeoRefData` hat dieselben Felder wie `EdgeRef`/`FaceRef` (Marke: `origin`, `role`, `hint`), aber ohne Abhängigkeit zu `planora-feature`; Umwandlung dort verlustfrei in beide Richtungen.
- **P3 Vorgaben:** `decimals = 1`; neue Normansichten `centerlines = true`, Isometrie `false`; Führungslinie bei Ø/R 45°; `offset` Vorgabe `DEFAULT_OFFSET` (10 mm), bei weiteren Maßen derselben Ansicht und Richtung `DEFAULT_OFFSET + n·DIM_STEP` (n zählt alle vorhandenen Maße der Ansicht und Richtung; eine Bezugsmaßgruppe mit k Gliedern belegt k Stufen).
- **P4 Messrichtungen:** `Horizontal` entlang `ex`, `Vertical` entlang `ey`, `Aligned` zwischen zwei Punkten (euklidisch im Bild). Kante, Eckpunkt, Mitte und Achse liefern ihre Lage quer zur Messrichtung; steht eine Kante nicht quer: `NotAcross`.
- **P5 Ø/R-Darstellung:** Kreis oder Bogen mit Blick in Achsrichtung: Führungslinie aus dem Kreis; Kreis von der Seite oder Zylinderfläche quer: lineares Maß mit „Ø“; Radius nur bei Kreis/Bogen mit Blick in Achsrichtung und bei Kugel; schräg gesehene Kreise: Fehler.
- **P6 Ketten und Bezugsmaße:** `ChainSpec { group, mode, order }`; alle Maße einer Gruppe haben dieselbe Messrichtung und denselben Modus. `Chain`: gemeinsame Maßlinie; `Baseline`: gestapelte Maßlinien `offset + order·DIM_STEP`.
- **P7 Textbreite:** `TEXT_ADVANCE · DIM_TEXT_H · Zeichenzahl`, bei Toleranz in zwei Zeilen die des breiteren Teils; reine Schätzung, als Grenze dokumentiert.
- **P8 Mittellinien in `ViewLines`:** `compute_view` füllt `ViewLines.centerlines` immer; `build_sheet` nimmt sie nur bei `ViewSpec.centerlines = true` auf; der Rechenschlüssel `(model_edit_count, Ansichtsart)` bleibt.
- **P9 Markierungen:** `SheetView.marks`; `build_sheet` bleibt (leere Maße), neu `build_sheet_with(drawing, results, dims)`.
- **P10 `SheetLine.origin`:** `SheetLine` trägt `origin: StrokeOrigin` für das Werkzeug.
- **P11 Format 5:** `FORMAT_VERSION = 5`, nur geschrieben, wenn eine Zeichnung Maße hat oder eine Ansicht `centerlines = true`; Format-4-Dateien laden mit `centerlines = false`; bei Format ≤ 4 lässt der Schreiber `mittellinien`/`masse` weg (Byte-genau).
- **P12 Außerhalb des Blatts:** Ein Maß außerhalb des Blatts wird gezeichnet und in der Oberfläche gewarnt; der Export schreibt es und zählt es in der Karte („Maße außerhalb des Blatts: n“).
- **P13 Export bei verlorenen Maßen:** Der Export ist gesperrt (Zustand „Fehlgeschlagen“, nichts wird geschrieben). Der Text lautet „Das Maß „{Text}“ in der Ansicht „{Name}“ hat seinen Bezug verloren: {Grund}. Bitte das Maß löschen und neu setzen.“ (**berichtigt** in der Nachbesserung, siehe 10.4: früher „Bitte ändern oder löschen.“, aber das Fenster „Maß bearbeiten“ kann keinen Bezug neu verknüpfen); für Maße, die sich nur nicht mehr messen lassen (nicht quer, nicht sichtbar, mehrdeutig), ein eigener Satz („… hat sich verändert und lässt sich nicht mehr messen: …“). Genannt wird das erste verlorene Maß in Zeichnungsreihenfolge; `{Text}` ist der eigene Maßtext (`<>` als `?`, auf 40 Zeichen gekürzt) bzw. die Art.

### 10.2 Entscheidungen der Umsetzung (Rulings je Aufgabe)

**Typen und Messung (Aufgaben 1, 2)**
- Format-4-Dateien laden mit `centerlines = false` (P11); der Vorschlag, beim Laden `kind.is_normal()` zu nehmen, wurde abgelehnt, damit alte Zeichnungen unverändert bleiben.
- `Anchor::Linear` trägt je Bezug beide Enden quer zur Messrichtung (`a_span`, `b_span`, geordnet nach der Querlage); die Darstellung (Aufgabe 4) nimmt je Bezug das der Maßlinie nähere Ende für die Hilfslinie. `a`/`b` bleiben als Verträglichkeitsfelder (Ende mit größerer Querlage bzw. die Messpunkte beim schrägen Maß).
- `Geo3::Axis` hat den Bereich `t0`/`t1` (siehe P1).
- `Angle` wird abgelehnt, wenn eine Kante nicht parallel zur Bildebene liegt (`|e·d| ≤ ANGLE_PLANE_TOL`), sonst wäre der Winkel verzerrt. Der Scheitel ist der Schnitt der unbegrenzten Bildgeraden; liegt er an einem Kantenende (bis `END_PARAM_TOL`) oder außerhalb, zeigt die Richtung vom Scheitel zur Kante, sonst zum näheren Ende. `Aligned` zwischen Punkten misst den Abstand im Bild (übliche Zeichnungsregel).
- Radius an Zylinderfläche oder Kreis von der Seite: `Invalid` mit eigenen Texten. Ø einer Zylinderfläche mit Blick entlang der Achse ist ein `Round`-Anker. `Aligned` nur Punkt oder als Punkt gesehene Kante. Winkel-Scheitel am Kantenende: Richtung entlang der Kante. Eine Kante gilt als Punkt bei Bildlänge ≤ 16·`geom_tol`. Überlauf beim Projizieren wird vor `NotAcross` gemeldet.

**Mittellinien (Aufgabe 3)**
- Keine Mittellinien, wenn die Blickrichtung keine Weltachse ist (auch ohne Kugelkreuz). Überlappende oder berührende kollineare Achsenlinien werden verbunden (auch über Körper), konzentrische Kreuze zu einem Kreuz mit dem größten Radius. Der Achsbereich kommt exakt aus den Randkanten (nicht aus dem gepolsterten `v_range`); die Zusammenführungs-Toleranz ist `geom_tol(max(Ausdehnung, größte Koordinate))`. Versteckte Bohrungen bekommen Mittellinien (akzeptiert).

**Darstellung (Aufgabe 4)**
- `GroupBase`/`group_base` und `layout_dimension_in` (mit `layout_lost_in`) teilen den Bezug quer zur Messrichtung für Ketten und Bezugsmaße; `ResolvedDims.last_known` trägt die letzte Lage verlorener Maße. Jeder Messfehler wird rot gezeichnet (Text = Fehlertext, nur „?“ ohne letzte Lage).
- Positives `offset` = oberhalb (waagerecht) bzw. rechts (senkrecht); der Text steht oberhalb der Maßlinie in Leserichtung (ISO 129, Methode 1). `group_index` wirkt nur bei Bezugsmaßen.
- `Tolerance::LimitSizes` hat **Abweichungen** als Werte: 12,5 mit +0,1/−0,1 gibt 12,6 und 12,4 (Doc-Kommentar an der Art; Dialog, Datei und Darstellung nutzen dieselbe Bedeutung). Ein eigener Text ersetzt nur den Wert (`<>` = berechneter Wert; beginnt der Text mit `<>`, bleibt der Vorsatz).
- Ein großes Ø (ab 25 Blatt-mm): Linie durch die Mitte statt Führungslinie, Text bei `c + t·max(r/2, Breite/2 + 1)`.

**Ausgabe (Aufgaben 5, 6, 14)**
- SVG: Bögen in Stücken bis 90° (`large = 0`, `sweep = 1`); Gruppen-Kennungen über `unique_name` (Suffix „ 2“); `stage10_svg` setzt `centerlines = false` (Mittellinien sind jetzt Vorgabe), sonst bleiben alte Ausgaben Byte-genau.
- DXF: Latin-1-Zeichen als `\U+XXXX` (reines ASCII), `^` als `^ `, Steuerzeichen als Leerzeichen, `%` als `%%%` (CAD-Programme decodieren `\U+` beim Laden, `%%`-Codes erst bei der Anzeige; `\U+0025` plus `%c` würde also zu Ø), `\` als `\U+005C`. Ein Text über 255 Zeichen in dieser Schreibweise lässt das Schreiben mit `Unsupported` scheitern; `dxf_text_fits` prüft das vorher. Der Fusion-Handtest muss `%%%` und `\U+005C` prüfen (ezdxf `plain_text()` decodiert `%%%` nicht; das ist kein Fehler der Datei).

**Vorschlag (Aufgabe 7)**
- Der Schlüssel eines Maßes ist **geometrisch**, nicht kantenbasiert (`dimension_key(kind, geos, basis, view_kind, extent)`): `L|<Richtung im Raum>|<Lage 1>|<Lage 2>` bzw. `D|A|…`/`R|A|…`/`D|K|…`, Zahlen auf 1e-6 mm gerundet. Nur so wird dasselbe Maß in zwei Ansichten erkannt (die Breite hängt in Vorne und Oben an verschiedenen Kanten). Schräge Maße und Winkel haben keinen Schlüssel; verlorene Maße ebenfalls.
- `Candidate` hat zusätzlich `chain: Option<ChainSpec>` (Bezugsmaße der Bohrungsmitten); die Gruppennummer gilt nur innerhalb eines `suggest`-Ergebnisses, die Oberfläche vergibt blattweit eindeutige. `existing` wird nur über den Schlüssel verglichen.
- Bekannte Grenze: Zylinder- und Kugelumriss am Rand zählen nicht als Bezug eines Gesamtmaßes.

**Verweise und Dokument (Aufgaben 8, 9)**
- Die Auflösung folgt der Regel des Vorschlags: Kante mit Gerade → `Line` zwischen den Eckpunkten; Zylinderfläche → `Axis` mit `frame.z` und `t0`/`t1` über die Randkanten; Kreiskante → `Circle`. Gleichstand (Abstand ≤ 1e-6) gibt `Ambiguous`. Zusätzliche Prüfungen: Bezugsdaten passen zur Art, Hinweise/Winkel/Textversatz endlich, Zähler weder 0 noch `u64::MAX`.
- Hinweispunkte sind auf `2·MAX_COORD_MM` begrenzt (`MAX_HINT_MM`), nicht auf `MAX_POS_MM`, damit alles, was die Oberfläche anlegen kann, auch speicherbar ist; Rollenlänge höchstens 64 Zeichen (`MAX_ROLE_CHARS`); ein Kettenmodus je Gruppe (in `validate_dimensions` und im Leser).
- **Format 4 speichert `next_dim` nicht** (laut Brief „sonst wie bisher“): löscht man alle Maße bei ausgeschalteten Mittellinien, beginnen die Nummern nach dem Laden wieder bei 1.

**Oberfläche (Aufgaben 10 bis 13)**
- `notes(&self, state, drawing, resolved)`; `sheet.rs` bekommt `DimMarks`/`dimension_marks()` (Markierungen je Maß für Treffer und Vorschau) und `view_transform` (pub). `SuggestOutcome::Partly { added, skipped }` mit `message()`. Das Auflösen nutzt Ergebnisse wieder, solange Modell, Ansichtsarten und Maßbezüge gleich sind (billiges Ziehen). Der Export löst im Arbeitsfaden selbst auf (`planora-feature::resolve_drawing_dimensions`), nicht über den Zwischenspeicher.
- Die Werkzeugleiste des Zeichnungsbereichs hat immer zwei Zeilen (Zeile 2: „Maße vorschlagen“ und die Einstellungen der Ansicht); die Vorschlagsmeldung steht in der Hinweiszeile des Zeichnungsbereichs, nicht in `state.message`; Auswahl von Ansicht und Maß schließen sich aus; die Auswahlfarbe auf Papier ist der helle Akzent; das Ziehen bleibt auf der Startseite (mindestens 1 mm Abstand).
- `finish` des Werkzeugs liefert `Result<Vec<DimId>, _>`; `hover(&AppState, drawing, pointer)`; das Werkzeug ist gesperrt, wenn die Zeichnung keine Normansicht hat (Klick auf Isometrie wird mit Text abgelehnt); ein Klick auf einen Silhouettenumriss ist in linearen Modi ein Flächenbezug (Achse, P4); Esc setzt eine Kette, bricht sonst ab und hängt nie; Doppelklick wirkt nur bei Ketten.
- Das Fenster „Maß bearbeiten“ (`dimension_dialog`) liegt im `AppState`, nicht im Tab; es sperrt im Schreibschutz alle Eingaben und schließt sich bei veralteten Maßen.
- Der Export nennt im Verlustfall nur das erste verlorene Maß; die Karte zeigt „Maße außerhalb des Blatts: n“ (nur bei n > 0) und „Mittellinien“ nur für Normansichten.

### 10.3 Berichtigte Stellen

| Stelle | Entwurf | Umsetzung |
|---|---|---|
| §2 Datenmodell | `DimSpec` ohne Führungswinkel | `angle` (Grad, Vorgabe 45) ergänzt |
| §2 Anker, §3.2 | Anker mit zwei Punkten | `Anchor::Linear` mit `a_span`/`b_span` je Bezug |
| §3.1 Auflösen | `resolve_dimension(...) -> Resolved` | `measure(kind, &[Geo3], &Basis, ViewKind, extent)`; Auflösen der Bezüge in `planora-feature` (`resolve_geo`, `resolve_dimensions`) |
| §3.4 Mittellinien | am Rand der Ansicht begrenzt | nicht begrenzt (3 mm Überstand, Folgearbeit); keine Mittellinien bei Blick nicht entlang einer Weltachse |
| §3.5 Vorschlag | `suggest_dimensions(...) -> Vec<DimSpec>` | `suggest(bodies, views, existing) -> Vec<Candidate>` mit `chain`, geometrischer Schlüssel |
| P1 | `Axis { origin, dir, radius }` | zusätzlich `t0`, `t1` |
| §4 Texte | DXF-Minus `\U+2212` | Minus als `-`; `%` als `%%%`, `\` als `\U+005C`, Latin-1 als `\U+XXXX` |
| §2 Speichern | Format 5 nur bei Maßen oder Mittellinien | zusätzlich: Format 4 speichert `next_dim` nicht |
| P3 Abstand | `DEFAULT_OFFSET + n·DIM_STEP` | n zählt alle Maße der Ansicht und Richtung, Gruppen belegen je Glied eine Stufe |
| P12 Wortlaut | „Maße außerhalb: n“ | „Maße außerhalb des Blatts: n“ |

### 10.4 Nachbesserung nach der Gesamtprüfung (2026-10-16)

- **Ansicht löschen:** Werden mit einer Ansicht (und ihren abgeleiteten) Maße gelöscht, fragt die Oberfläche immer nach und nennt ihre Zahl („… und n Maße werden mit gelöscht.“); „Ansicht löschen“ in der Werkzeugleiste geht denselben Weg wie „Löschen“ im Browser. `Drawing::remove_view` löscht die Maße weiterhin mit.
- **Zwischenspeicher der Verweise:** `planora_feature::RefCache` merkt Prüfpunkte der Flächen, Kantenverwendungen und Achsbereiche je Durchgang (Vorschlag, Klick des Werkzeugs, Auflösung aller Maße). `EdgeRef::from_edge`, `resolve_geo` usw. haben `_cached`-Fassungen mit **gleichem Ergebnis**; die alten Funktionen rufen sie mit einem frischen Speicher.
- **Verlorene Maße im Vorschlag:** `resolve_drawing_dimensions_keyed` liefert zu jedem Maß mit auflösbaren Bezügen den Schlüssel; die Oberfläche merkt ihn (`DimensionUi::last_key`), der Vorschlag zählt ihn für ein später verlorenes Maß als vorhanden. Ein Maß, das in der Sitzung nie aufgelöst wurde, hat keinen Schlüssel (bekannte Grenze). Damit gilt der Satz in 10.2 „verlorene Maße haben keinen Schlüssel“ nur noch für solche Maße.
- **Gründe verlorener Maße** ohne innere Nummern: „Die Fläche, an der das Maß hing, gibt es nicht mehr.“ bzw. „Die Kante, an der das Maß hing, gibt es nicht mehr.“ Der Exporttext (P13) nimmt bei `Lost` diesen Satz ohne den Vorsatz „Der Bezug wurde nicht gefunden“ und endet mit „Bitte das Maß löschen und neu setzen.“ (das Fenster „Maß bearbeiten“ kann keinen Bezug neu verknüpfen); Sperre und übrige Bedeutung von P13 bleiben.
- **Ketten und Bezugsmaße ziehen:** Die Maßlinie eines Glieds zu ziehen ändert den Abstand aller Glieder der Gruppe um dasselbe Stück (ein Schritt).
- **Grenzen:** `ChainSpec.order` < `MAX_DIMENSIONS` im Dateileser und in `validate_dimensions`. Im Fenster „Maß bearbeiten“ werden eigener Text und Passung gekürzt (nur Leerzeichen = kein Text), obere < untere Abweichung und negative symmetrische Abweichung abgelehnt (nur in der Oberfläche).
- **Mittellinien-Schalter** in der Isometrie gesperrt, mit Erklärung.
