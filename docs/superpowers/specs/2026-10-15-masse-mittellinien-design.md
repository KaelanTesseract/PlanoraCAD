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
DimSpec     { id, view: ViewId, kind, refs: Vec<GeoRef>, offset: f64 /*mm, Maßlinie vom Maßziel*/,
              text_shift: (f64, f64) /*Blatt-mm vom Standardort*/, tolerance, decimals: u8 /*0..=3*/,
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

1. **Auflösen:** `resolve_dimension(solids, spec, basis) -> Resolved { value, tolerance_text, geometry, status }`. Die Bezüge werden
   zu 3D-Geometrie (Punkt, Gerade, Kreis mit Mitte/Normale/Radius, Achse) aufgelöst und mit der Ansichtsbasis auf die Blattebene
   projiziert.
2. **Werte:**
   - `Horizontal`/`Vertical`: Abstand der projizierten Bezüge entlang `ex` bzw. `ey`; eine Kante muss quer zur Messrichtung stehen
     (Fehler „Die Kante steht nicht quer zum Maß“).
   - `Aligned`: Abstand in der Bildebene zwischen zwei Punkten oder (parallelen) Kanten.
   - `Diameter`/`Radius`: aus Kreis/Bogen (Blick in Achsrichtung → Kreis, Führungslinie) oder aus Zylinder-/Kegel-/Kugelfläche
     (Blick quer zur Achse → Abstand der beiden Umrisslinien, Maß mit „Ø“).
   - `Angle`: Winkel zwischen zwei projizierten Geraden (0–180°).
   Der Wert ist der **wahre** Modellwert, solange die Messrichtung in der Bildebene liegt (Normansichten); sonst Fehler.
3. **Verloren:** Bezug nicht mehr auflösbar, mehrdeutig oder nicht sichtbar → `status = Lost(Grund)`; die letzte bekannte Lage bleibt
   gespeichert (Hinweispunkte), das Maß wird rot gezeichnet, ohne Wert („?“).
4. **Mittellinien:** je Ansicht aus den sichtbaren Flächen: Kreiskante mit Achse ∥ Blickrichtung → Achsenkreuz (zwei Linien, 3 mm
   Überstand); Zylinder/Kegel mit Achse ⟂ Blickrichtung → Mittellinie entlang der Achse mit 3 mm Überstand; Kugel → Achsenkreuz;
   Torus: Achsenkreuz im Blick entlang der Achse. Konzentrische/gleiche Linien werden einmal geschrieben; Linien, die die Ansicht
   verlässt, werden nicht gekürzt, sondern am Rand der Ansicht begrenzt.
5. **Vorschlag:** `suggest_dimensions(view, resolved lines) -> Vec<DimSpec>` mit `suggested = true`: (a) **Gesamtmaße** (Breite/Höhe
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
- **Texte:** Ziffern und Sonderzeichen: Ø, °, ±, −. SVG schreibt Unicode; DXF `%%c`, `%%d`, `%%p` und `\U+2212`.
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
