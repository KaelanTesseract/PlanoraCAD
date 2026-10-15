# Teilprojekt 11: Zeichnungen – Ansichten (SVG/DXF) – Entwurf

Stand: 2026-10-14. Baut auf Teilprojekt 10 (STEP-Export, `master`, Merge 9a44307) auf. Teil 3a von 3 der Zeichnungs-Strecke
(3a Ansichten, 3b Bemaßung und Schriftfeld, 3c Schnittansichten). Teil 3 der Austausch-Strecke (Netz, STEP, Zeichnungen).

## 1. Ziel und Abgrenzung

Aus den exakten Körpern eines Dokuments entstehen technische Zeichnungen: Normansichten und Isometrie mit sichtbaren und
verdeckten Kanten, auf einem Blatt mit Maßstab angeordnet, ausgegeben als SVG (Drucken, Dokumente) und DXF (Laser, Fräse,
andere CAD-Programme). Kreise bleiben Kreise, Splines bleiben Splines; es gibt keinen Umweg über Polylinien.

**Entscheidungen (mit dem Nutzer abgestimmt, 2026-10-14):**

| Nr. | Entscheidung |
|---|---|
| Z1 | Zweck: Fertigung/Werkstatt, Dokumentation und CNC/Laser. Gesamtumfang in drei Teilprojekten; **jetzt nur 3a** (Ansichten). Bemaßung und Schriftfeld (3b), Schnitte (3c) folgen. |
| Z2 | **Echte Kurven:** Geraden, Kreise, Bögen, Ellipsen und B-Splines bleiben als solche erhalten (DXF: LINE, CIRCLE, ARC, ELLIPSE, SPLINE). |
| Z3 | **Verfahren A, alles exakt:** Sichtbarkeit wird mit Strahlen gegen die echten Flächen entschieden, nicht am Netz. |
| Z4 | Netz-Körper werden **nicht gezeichnet**; die Oberfläche nennt sie mit Namen (wie beim STEP-Export). |
| Z5 | Die Zeichnung ist ein **Teil des Dokuments**, wird mit gespeichert und ist **live verknüpft**: sie rechnet sich nach Modelländerungen neu. |
| Z6 | Eigener **Arbeitsbereich im Dokument-Tab** (Umschalter 3D-Modell / Zeichnung), Zeichnungen stehen im Browser. |
| Z7 | Ansichten: Vorne, Hinten, Links, Rechts, Oben, Unten und Isometrisch; frei auf dem Blatt platzierbar, je Ansicht eigener Maßstab. |
| Z8 | Projektion: **Erstwinkel (ISO E) und Dritter Winkel (US) wählbar**, Einstellung je Zeichnung. |
| Z9 | Keine neue externe Abhängigkeit (SVG und DXF als Text von Hand geschrieben). Lizenz: DXF-Gruppencodes und SVG sind offene Formate; kein Autodesk-Material kopieren. |

**Nicht Teil von 3a:** Maße, Toleranzen, Schriftfeld, Beschriftung, Mittellinien und Achsenkreuze, Schnittansichten,
Schraffur, Detailansichten, Stücklisten, Netz-Körper, Einlesen von SVG/DXF, PDF-Ausgabe, freie Blickrichtung.

## 2. Aufbau

Neues Paket `planora-drawing` (reine Berechnung, ohne Oberfläche; Abhängigkeiten: `planora-kernel`, `planora-math`,
`planora-exchange`, `thiserror`; für Fortschritt `planora_exchange::Progress` wie bei `planora-step`; `planora-mesh` nur für die
Prüfwerkzeuge der Tests). Die tatsächliche Aufteilung weicht von der Skizze unten ab, siehe „Festlegungen der Umsetzung“ (§11, F1, F2).

```
planora-drawing/src/
  lib.rs        // compute_view, Drawing-Typen, Fehler
  model.rs      // Drawing, Sheet, ViewSpec, ViewKind, Projection
  basis.rs      // Blickrichtung und Bildachsen je Ansichtsart (statt project.rs)
  stroke.rs     // Strich = 3D-Kurve mit Parameterbereich (statt curve2.rs, F2)
  isect.rs      // Schnitte der projizierten Striche, Teilparameter
  raycast.rs    // Strahl gegen Flächen
  silhouette.rs // Umrisslinien der Flächenarten
  visibility.rs // Teilen an Kreuzungen, Strahlprüfung (Kippabstimmung, F9), Sichtbarkeit je Stück
  layout.rs     // Blattgrößen, Maßstabsreihe, Lage der abgeleiteten Ansichten (Erst-/Dritter Winkel)
  sheet.rs      // Blattmodell: Linien aller Ansichten in Blattkoordinaten
  out2d.rs      // Ausgabekurven (Linie, Bogen, Ellipse, Spline) für die Schreiber
  svg.rs        // SVG-Schreiber
  dxf.rs        // DXF-Schreiber (R2000, ASCII)
  file.rs       // Schreiben über eine Hilfsdatei
```

**Schnittstelle:** `compute_view(bodies: &[DrawBody], kind: ViewKind, progress: &Progress) -> Result<ViewLines, DrawError>`
(die Ansicht hängt nur an der Art, nicht an Maßstab und Lage, F8).
`DrawBody { name: String, solid: &Solid }`. `ViewLines` enthält Kurvenstücke in Ansichtskoordinaten (mm, ungeskaliert, y nach
oben) mit `Visibility::{Visible, Hidden}` und dem Ursprung (Körper, Kante oder Umrissfläche). Skalierung und Verschiebung auf
das Blatt macht `layout.rs`; die Schreiber erhalten fertige Blatt-Linien.

## 3. Datenmodell (im Dokument, gespeichert)

```
Drawing   { id, name, sheet: Sheet, projection: Projection, views: Vec<ViewSpec> }
Sheet     { paper: A4 | A3 | A2 | A1 | A0, landscape: bool }            // mm, ISO 216
Projection{ First | Third }
ViewSpec  { id, kind: ViewKind, scale: Scale, placement: Placement /* Free{x,y} | Aligned{distance} */,
            show_hidden: bool, parent: Option<ViewId> }
ViewKind  { Front, Back, Left, Right, Top, Bottom, Isometric }
Scale     { Ratio(num, den) }   // Reihe 1:1, 1:2, 1:5, 1:10, 1:20, 1:50, 1:100, 2:1, 5:1, 10:1, dazu frei
```

- Die **erste** Ansicht einer Zeichnung ist die Grundansicht (`parent = None`). Die übrigen Normansichten sind davon abgeleitet
  (`parent = Some`) und liegen nach der Projektionsregel am richtigen Ort: Erstwinkel: Draufsicht **unter** der Vorderansicht,
  Linksansicht (Blick von links) **rechts** daneben; Dritter Winkel: Draufsicht **über** der Vorderansicht, Rechtsansicht rechts.
  Abgeleitete Ansichten dürfen nur längs ihrer Ausrichtungsachse verschoben werden (Normverhalten); der Maßstab erbt vom Elternteil,
  ist aber änderbar (dann entfällt die Ausrichtung nicht). Isometrisch ist frei.
- **Koordinatensystem:** Weltachsen wie im Modell (Z nach oben). Ansicht „Vorne“ blickt in +Y, „Oben“ in −Z, „Links“ in +X (Blick
  von links). Isometrisch blickt aus (+1, −1, +1)/√3 auf den Ursprung; Bildachsen: x rechts, y oben.
- **Welche Körper:** alle sichtbaren exakten Körper des Dokuments (jetziger Stand nach der Neuberechnung). Es gibt keine
  Körperauswahl in 3a.
- **Speichern:** ein neues optionales Feld `drawings` in der Versionsdatei (`serde(default)`), damit ältere Dateien unverändert
  laden und neuere Dateien in ihrer Formatversion wie bei früheren Erweiterungen behandelt werden (Einzelheiten im Plan nach
  Blick auf `planora-project`). Berechnete Linien werden **nicht** gespeichert, nur die Beschreibung.

## 4. Rechenweg je Ansicht

Alle Schritte laufen in Weltkoordinaten; erst am Ende wird auf Bildkoordinaten gedreht. Blickrichtung `d` (vom Auge weg),
Bildbasis `(ex, ey)` mit `ex × ey = −d`. Parallelprojektion `p ↦ (p·ex, p·ey)`, Tiefe `p·d`.

### 4.1 Kantenkurven

Jede Kante der Körper wird projiziert (Parallelprojektion ist affin, die Kurvenart bleibt erhalten):

| Kernel-Kurve | Projektion |
|---|---|
| `Line` | `Line2` (Länge 0, wenn die Gerade parallel zu `d` liegt: **entfällt**, bei Kanten sichtbar als Punkt) |
| `Circle` | `Ellipse2`; Kreis genau dann, wenn die Kreisachse parallel zu `d` liegt; Strecke, wenn die Kreisebene `d` enthält (entartet: **Line2** zwischen den Extrempunkten, doppelt durchlaufen) |
| `Ellipse` | `Ellipse2` (oder Strecke im entarteten Fall) |
| `Nurbs` | `Spline2` mit denselben Knoten, Gewichten und projizierten Kontrollpunkten (exakt) |

Kantenstücke nutzen den Parameterbereich `[t0, t1]` der Kante.

**Glatte Kanten** (die beiden angrenzenden Flächen sind entlang der Kante tangentenstetig, Normalenwinkel < `SMOOTH_ANGLE =
0,5°` an mehreren Stützstellen) werden **nicht** gezeichnet, ebenso Nahtkanten geschlossener Flächen (Zylindernaht und Ähnliches,
eine Kante, die dieselbe Fläche beidseitig begrenzt). Sie dienen aber als Teilungspunkte der Umrisslinien.

### 4.2 Umrisslinien (Silhouetten)

Eine Umrisslinie ist die Menge der Flächenpunkte mit `n · d = 0`, innerhalb des Gebiets der Fläche (Flächenbegrenzung und Löcher
berücksichtigt).

| Fläche | Umriss | Genauigkeit |
|---|---|---|
| `Plane` | keiner (steht die Ebene parallel zu `d`, erscheint sie als Strecke – die Randkanten tragen sie) | exakt |
| `Cylinder` | zwei Mantellinien im Abstand `±r` von der Achse (wenn Achse nicht parallel zu `d`), begrenzt auf die Länge der Fläche | exakt |
| `Cone` | zwei Mantellinien durch die Spitze (wenn der Blick nicht in den Öffnungswinkel fällt) | exakt |
| `Sphere` | ein Großkreis senkrecht zu `d` (als Kreis, auf das Flächengebiet begrenzt, ggf. mehrere Bögen) | exakt |
| `Torus` | Kurve vierter Ordnung (Umriss eines Torus ist kein Kegelschnitt) | **Spline innerhalb `SILHOUETTE_TOL = 1e-4 mm`** |
| `Nurbs` | Lösung von `n·d = 0` durch Verfolgen und Anpassen | **Spline innerhalb `SILHOUETTE_TOL`** |

Jedes Umrissstück wird auf das Flächengebiet zugeschnitten (innerhalb des Parameterbereichs, außerhalb der Löcher). Wo Umriss
und Randkante zusammenfallen (z. B. Zylinder, dessen Mantellinie genau eine Kante ist), entsteht **ein** Stück, nicht zwei.

### 4.3 Teilen und Sichtbarkeit

1. **Teilpunkte.** Alle projizierten Kurven (Kanten und Umrisse, aller Körper) werden paarweise geschnitten (2D-Kurve gegen
   2D-Kurve; Vorauswahl über Hüllkörper). Schnittpunkte, Kantenenden und Umrissenden teilen die Kurven in Stücke. Gerade gegen
   Gerade und Gerade gegen Kegelschnitt sind geschlossene Formeln; Kegelschnitt gegen Kegelschnitt und alle Spline-Fälle lösen
   Wurzelfinder mit Prüfung der Wurzelzahl (wie in `boolean/roots`). Berührpunkte (doppelte Wurzel) teilen nicht.
2. **Strahlprüfung (Kippabstimmung, F9).** Je Stück wird an mindestens drei Stützpunkten (1/4, 1/2, 3/4) die zugehörige
   3D-Stelle bestimmt. Von dort gehen **acht leicht gekippte Strahlen** zum Auge (`q(s) = P − s·d'`, `s > 0`, `d'` um `TILT`
   gegen `d` gekippt) gegen alle Flächen aller Körper (analytische Strahl-Fläche-Schnitte, bei `Nurbs` eigene Prüfung begrenzt
   durch die Flächenränder). Treffer näher als `SELF_EPS = 1e-7 · Ausdehnung` an `P` zählen nie (alle Flächen, auf den
   eigenen Flächen des Strichs gilt `OWN_EPS = 1e-5 · Ausdehnung`). **Frei unter irgendeinem Kippstrahl heißt sichtbar**
   (Umrisskonvention); verdeckt ist die Stelle nur, wenn alle brauchbaren Strahlen verdeckt sind. Flächen, die in der Ansicht
   nur eine Strecke sind (`|n·d| ≈ 0`), verdecken nichts.
3. **Unbrauchbare Strahlen** (Streifschnitt, Strahl trifft eine Kante oder einen Eckpunkt einer Fläche, `Unsafe`) zählen
   nicht. Gibt es für einen Stützpunkt keinen brauchbaren Strahl, wird er verschoben; danach ist die Ansicht mit
   `DrawError::AmbiguousVisibility` (Stückbezeichnung) gescheitert; es wird **nichts geraten**. (Der ursprünglich
   vorgesehene Einzelstrahl scheitert an Linien, die genau auf dem Umriss einer Fläche oder in der Ebene einer von der Seite
   gesehenen Fläche liegen, etwa Quader von vorne oder Zylinderdeckel; Begründung in §11.)
4. **Konsistenzprüfung.** Liefern die Stützpunkte eines Stücks verschiedene Ergebnisse, wird das Stück an der Übergangsstelle
   per Bisektion in Parameterschritten geteilt (bis `1e-9 · Länge`) und beide Teile neu beurteilt. Eine fehlende Kreuzung im
   Schritt 1 zeigt sich so, statt falsch gezeichnet zu werden.
5. **Zusammenfassen.** Aneinanderstoßende Stücke derselben Ursprungskurve und gleicher Sichtbarkeit werden wieder zu einer
   Kurve vereint (Zahl der Zeichnungsobjekte klein halten).

Ergebnis je Stück: `Curve2` (Bereich) + `Visibility`. Ist `show_hidden = false`, werden verdeckte Stücke nach der Rechnung
verworfen (Berechnung dennoch voll, weil verdeckte Stücke das Teilen beeinflussen).

### 4.4 Genauigkeit und Grenzen

- Ausdehnung `E` = größte Kantenlänge der Hüllbox aller Körper. Alle absoluten Toleranzen skalieren mit `E`
  (`geom_tol = max(1e-9, 64·ε·E)`), nicht fest, wie die Erfahrung aus dem STEP-Export nahelegt.
- **Entartete Ansichten:** Ist ein ganzer Körper senkrecht projiziert flach (z. B. Quader von oben, Tiefe egal), sind
  übereinanderliegende Kanten **ein** Strich; deckungsgleiche Stücke (gleicher Ort, Länge innerhalb `1024·geom_tol`; die Kurvenart und die Parametrisierung spielen keine
  Rolle, §11) werden zusammengeführt und sind sichtbar, sobald eines davon sichtbar ist.
- **Kreuzungen im Innern gleicher Fläche** (Kante liegt in einer Fläche, die in der Ansicht als Strecke erscheint) werden nicht
  als Verdeckung gewertet.
- **Arbeitsgrenzen:** Kurvenpaare, Wurzelschritte und Strahlzahlen sind begrenzt; wer die Grenze verletzt, bekommt
  `DrawError::TooComplex` mit Zahl der betroffenen Stücke. Fortschritt über `Progress`, Abbruch jederzeit.

## 5. Ausgabe

**Blatt:** Papiergröße und ein Innenrahmen als einfache Linie, 10 mm vom Blattrand rundum (keine Randlinie am Blattrand selbst;
Heftrand und Schriftfeld folgen in 3b), Ansichten mit Maßstab und Lage.
Koordinaten: Ursprung links unten, y nach oben, Einheit mm.

**Linienarten (3a):** sichtbar: durchgezogen, 0,5 mm; verdeckt: gestrichelt (Strich 3 mm, Lücke 1 mm), 0,25 mm. Farbe schwarz.

**SVG:** `<svg width="…mm" height="…mm" viewBox="0 0 W H">`, y gespiegelt per Gruppentransformation, je Ansicht eine Gruppe
(`id` = Ansichtsname), sichtbar und verdeckt je ein Pfad (`stroke-dasharray` für verdeckt, `vector-effect="non-scaling-stroke"`
nicht verwendet, damit Druckbreite stimmt). Kurven:

| Kurve | SVG |
|---|---|
| Line2 | `M … L …` |
| Arc2 (Kreis) | `A r r 0 large sweep x y` |
| Ellipse2 | `A rx ry rot large sweep x y` |
| Spline2 | exakte Umwandlung in rationale/nichtrationale Bézier-Stücke, kubisch; Grad ≠ 3 wird durch Graderhöhung/-absenkung mit gemessener Abweichung ≤ `1e-6 mm` bzw. durch Unterteilung ausgedrückt (SVG kennt nur kubisch/quadratisch) |

**DXF:** AC1015 (R2000), ASCII, `$INSUNITS = 4` (mm). Ebenen `SICHTBAR` (Linienart `CONTINUOUS`) und `VERDECKT` (Linienart
`HIDDEN`, Skalenfaktor passend). Objekte: `LINE`, `CIRCLE`, `ARC` (Winkel in Grad, gegen den Uhrzeigersinn), `ELLIPSE`
(Mittelpunkt, Hauptachse, Verhältnis, Start-/Endparameter), `SPLINE` (Grad, Knoten, Kontrollpunkte, Gewichte; Fit-Punkte nicht).
Ansichten stehen als **Blöcke** (`BLOCK`/`INSERT`) mit dem Ansichtsnamen. Handle-Vergabe fortlaufend, `ENDSEC`/`EOF` korrekt;
Jede Ebene trägt einen Verweis (Gruppe 390) auf den Plotstil-Platzhalter; dazu gehören die Klassen
`ACDBDICTIONARYWDFLT` und `ACDBPLACEHOLDER` in `CLASSES` und die Kette `ACAD_PLOTSTYLENAME` → Verzeichnis mit Vorgabe
„Normal“ → Platzhalter in `OBJECTS`. Ohne diese Kette lehnt Fusion 360 die Datei ab (Handtest). Datei soll sich in Fusion 360,
FreeCAD, LibreCAD öffnen.

Beide Formate: Zahlenformat mit höchstens 9 Nachkommastellen (mm), nie `NaN`/`inf`; fehlerhafte Werte → `DrawError::NonFinite`.

## 6. Oberfläche

- Dokument-Tab: Umschalter **3D-Modell / Zeichnung** (Kopfleiste). Im Zeichnungsbereich: Blatt im Maßstab der Ansicht
  (Zoom, Verschieben), Ansichten als Linien, gewählte Ansicht mit Rahmen.
- Browser: Abschnitt **Zeichnungen** mit Eintrag je Zeichnung, darunter die Ansichten.
- Neue Zeichnung: Dialog mit Name, Papier, Hoch/Quer, Projektion, Grundansicht (Normalfall „Vorne“) und Maßstab (Vorschlag aus
  Körpergröße und Blatt: größter Wert der Reihe, bei dem die drei Hauptansichten aufs Blatt passen).
- **Ansicht hinzufügen:** Auswahl der Art; abgeleitete Ansichten springen an ihre Normposition; Isometrie wird frei gesetzt.
- Eigenschaften: Maßstab, Position (bei abgeleiteten nur Längsrichtung), „Verdeckte Kanten zeigen“ (Vorgabe: an für die
  Normansichten, aus für Isometrisch; siehe 11.2), Löschen.
- Neuberechnung: im Hintergrund als Job mit Fortschritt (wie beim Export); bis dahin zeigt die Ansicht den letzten Stand
  mit Hinweis „wird aktualisiert“; gescheiterte Berechnung zeigt den Fehlertext an der Ansicht (die Zeichnung selbst bleibt
  erhalten).
- **Export:** der Export-Dialog bekommt `ExportFormat::DrawingSvg`/`DrawingDxf`, im
  Zeichnungsbereich für die geöffnete Zeichnung, im Modellbereich mit Auswahl der Zeichnung. Hinweise (Netz-Körper weggelassen,
  Ansicht fehlgeschlagen) erscheinen wie beim STEP-Export vor dem Schreiben.
- Alle Texte deutsch, Laiensprache (siehe Gedächtnis zu Texten); keine Hinweise auf Datenherkunft.

## 7. Fehler

`DrawError`: `NoBodies` (nichts zu zeichnen), `MeshBodiesOnly`, `AmbiguousVisibility(String)`, `TooComplex(u64)`,
`NonFinite`, `Cancelled`, `Io`, dazu `InvalidBody(String)` und `Unsupported(String)`. Alle mit deutschem Text; die Oberfläche zeigt sie an der Ansicht bzw. im Export-Dialog.

## 8. Prüfung (Tests und Werkzeuge)

Wie bei STEP: **eigene Prüfwerkzeuge**, die nicht dieselbe Rechnung wiederholen.

- **Formelmodelle** (`tests/common`): für Quader, Zylinder (liegend/stehend), Kugel, Kegel, Torus, Quader mit Bohrung, Quader
  mit Rundung bekannte Linien je Ansicht als Formel; Vergleich nach Art, Lage und Länge (Gesamtlänge sichtbar/verdeckt).
- **Unabhängige Sichtbarkeit:** ein brutaler Referenzprüfer wirft für viele Punkte jeder Linie Strahlen gegen das **Netz** des
  Körpers (Netz aus `planora-mesh`, feiner Wert) und vergleicht Sichtbarkeit; Abweichungen sind nur in `chord_tol`-Nähe von
  Übergängen zulässig.
- **Eigene Leser** für SVG und DXF (Pfade, Bögen, Splines) – Rücklesen und Vergleich mit den Linien (Gesamtlänge, Endpunkte).
- **Mutationsbeweise** für Regressionen (Vorzeichen Erst-/Dritter Winkel, Selbsttreffer-Epsilon, Bisektion, Silhouette
  fehlend).
- **Serie:** 200+ zufällige Körper aus Quader/Zylinder/Kugel mit Booleschen, in allen sieben Ansichten, Invarianten:
  Endpunkte schließen, kein Stück der Länge 0, sichtbare Gesamtlänge ≥ Umrisslänge, keine Mehrdeutigkeit.
- **Oberfläche:** Dialog- und Registerkarten-Tests wie bei den Exporten (Schreiben, Fehler, Fortschritt, Speichern/Laden der
  Zeichnung, Neuberechnung nach Modelländerung).
- **Handtest** (nach Rückfrage beim Nutzer, vom Controller ausgeführt): Dateien öffnen in Fusion 360 (DXF als Skizze), Inkscape
  oder Browser (SVG), Gegenprobe der Maße.

## 9. Stufen

1. Typen, Projektion der Kanten, Linienarten, Blatt/Layout (ohne Sichtbarkeit): Würfel, Zylinder in allen Ansichten.
2. Umrisslinien (Zylinder, Kegel, Kugel exakt; Torus, Freiform als Spline).
3. Teilen und Sichtbarkeit (Strahlprüfung, Bisektion, Zusammenfassen), Serie.
4. SVG- und DXF-Schreiber mit eigenen Lesern.
5. Speichern/Laden (Datenmodell, Format).
6. Oberfläche: Arbeitsbereich, Browser, Dialoge, Neuberechnung im Hintergrund, Export.
7. Dokumentation, Handtest, Übergabe.

## 10. Bekannte Grenzen (zur Übernahme in README und offene-punkte)

- Torus- und Freiform-Umrisse sind angenäherte Splines (1e‑4 mm), alles andere exakt.
- Keine Maße, Mittellinien, Schnitte, Schriftfeld (3b/3c).
- Netz-Körper fehlen in Zeichnungen.
- Glatte Kanten (Tangentenübergänge) werden nicht gezeichnet.
- Streifende Strahlen (Ansicht fast parallel zu einer Fläche) können zu `AmbiguousVisibility` führen statt zu einer Zeichnung.
- Sehr viele Flächen/Splines können `TooComplex` auslösen.

Die vollständige, nach der Umsetzung gültige Liste der Grenzen steht in `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 11 –
Zeichnungen (Ansichten)“ (dazu gehören außer den obigen: stille Lücken bei sehr kleinen Freiform-Umrissschleifen, doppelte
Linien an Umkehrstellen ebener Splines, Freiform mit Pol in Seitenansichten, Ringe knapp neben ihrer Ringebene).

## 11. Festlegungen der Umsetzung

Diese Punkte präzisieren die Spec oder weichen von ihr ab. Sie sind während Planung und Umsetzung entstanden (Plan:
`plans/2026-10-14-zeichnungen-ansichten.md`, Protokoll in `.superpowers/sdd/2026-10-14-zeichnungen-ansichten/progress.md`).

### 11.1 Festlegungen der Planung (F1–F11)

- **F1 Fortschritt:** `Progress` kommt aus `planora-exchange` (die Spec nannte versehentlich `planora-mesh`). §2 ist berichtigt.
- **F2 Striche statt 2D-Kurventypen:** Statt `curve2.rs` mit `Line2/Arc2/Ellipse2/Spline2` gibt es den **Strich**
  `Stroke { curve: Curve (3D), t0, t1, origin }`. Die Projektion wertet die 3D-Kurve aus und projiziert (`p(t) = (P(t)·ex,
  P(t)·ey)`); so behält jede Projektion den Parameter der 3D-Kurve, Teilen heißt „Parameterbereich teilen“, und es gibt keinen
  zweiten Kurventyp-Satz. 2D-Typen (`Out2d`: Linie, Bogen, Ellipse, Spline) entstehen erst für die Ausgabe.
- **F3 Lage der Ansichten:** `Placement::Free { x, y }` oder `Placement::Aligned { distance }` (Abstand der Ansichtsmitten
  entlang der Ausrichtungsachse). Der Satz „abgeleitete Ansichten dürfen nur längs verschoben werden“ ist damit im Datentyp
  erzwungen; `ViewSpec.pos` aus §3 entfällt.
- **F4 Selbsttreffer:** Treffer mit `s ≤ SELF_EPS` zählen nie als Verdeckung (alle Flächen, nicht nur anliegende); auf den
  eigenen Flächen des Strichs gilt `OWN_EPS`. (§4.3 war knapper.)
- **F5 Torus-Umriss:** wird nicht marschiert, sondern parametrisch in `u` berechnet (`v(u) = atan2(−(dx·cos u + dy·sin u), dz)`
  bzw. `+π`, Sonderfall `dz = 0`) und als Spline angepasst; nur Freiformflächen marschieren.
- **F6 Doppelte Striche:** deckungsgleiche Stücke (Ort, Länge innerhalb der Toleranz) werden zu einem Stück vereint, sichtbar
  wenn eines davon sichtbar ist (§4.4).
- **F7 Format:** `FORMAT_VERSION = 4`; Format 4 wird nur geschrieben, wenn das Dokument mindestens eine Zeichnung enthält
  (wie Format 3 bei Netz-Körpern). Alte Dateien bleiben Byte für Byte unverändert.
- **F8 Berechnungsschlüssel:** Ein Ansichtsergebnis hängt nur an `(model_edit_count, ViewKind)`; Maßstab, Lage und andere
  Zeichnungsänderungen ändern es nicht (sie wirken erst beim Zeichnen und Schreiben). `Document::edit_count()` zählt alle
  Änderungen (Marke für „ungespeichert“), `model_edit_count()` nur die an Körpern.
- **F9 Kippabstimmung statt Einzelstrahl:** ersetzt §4.3 Schritte 2–3 (siehe dort). Ein einzelner Strahl scheitert an Linien auf
  dem Umriss einer Fläche und an Linien in der Ebene einer von der Seite gesehenen Fläche. Jede Stützstelle bekommt deshalb
  8 leicht gekippte Strahlen; frei unter irgendeinem heißt sichtbar, verdeckt nur bei lauter verdeckten brauchbaren Strahlen;
  ohne brauchbaren Strahl weicht die Stelle aus, danach `AmbiguousVisibility`.
- **F10 Entartete Striche:** Striche, die in der Ansicht zu einem Punkt werden (Kante parallel zur Blickrichtung), werden vor
  dem Teilen verworfen; ihre Endpunkte sind Endpunkte der Nachbarstriche.
- **F11 Getrennte Berechnung:** Der Zeichnungsbereich und der Export-Dialog rechnen Ansichten getrennt (je eigener Job); es
  gibt keinen gemeinsamen veränderlichen Zwischenspeicher.

### 11.2 Entscheidungen während der Umsetzung

Berechnung (`planora-drawing`)
- **Toleranzen** sind Funktionen der Ausdehnung (`geom_tol(E)`, `self_eps(E)`, `own_eps(E)`), keine Konstanten. Punkte, die
  zusammenfallen sollen, vereint die Bibliothek mit `1024·geom_tol` (≈ 1,5e-11·E, etwa 1e-6 mm bei normalen Körpern; der Kernel
  rundet Kanten bis ~1e-8 mm); Stücke unter `64·geom_tol` werden an den Vorgänger angelehnt oder entfernt.
- **Randnähe von Strahltreffern** wird exakt entschieden (Nächstpunkt auf dem Randstück, Band aus der gemessenen
  Polygonabweichung) statt mit dem Polygon-Test des Kernels (5°-Näherung, Fehler bis ~1e-3·r); der Kernel bleibt unverändert,
  die Logik ist in `raycast.rs` kopiert.
- **Bisektion** (`classify_range`) vergleicht mit den bekannten Urteilen an den Bereichsenden und teilt Teilbereiche unter
  `OWN_EPS` in der Mitte, statt abzubrechen. Stücke unter `OWN_EPS` ohne brauchbaren Strahl erben das Urteil des Nachbarn.
- **Umrisse:** Torus und Freiform → `Unsupported`, bis Aufgabe 7 sie lieferte; die Sichtbarkeit eines Spline-Umrisses wird am
  wahren Umrisspunkt beurteilt (geometrische Zuordnung, nicht über den Parameter). Streifende Freiform-Strahlen, die der Kernel
  ablehnt, entscheidet eine eigene Bézier-Unterteilung (höchstens 20 000 Teilflächen je Strahl, sonst unbrauchbar).
  Neue Arbeitsgrenze `MAX_OUTLINE_POINTS = 400 000` je Torus- oder Freiformfläche.
- **Zusammenfassen:** Reihenfolge Vereinen → Zuschneiden → Winziges → Vereinen; unter deckungsgleichen Stücken gleicher
  Sichtbarkeit gewinnt das dem Auge nähere (sichtbar, Tiefe, Index). Überdeckung wird **geometrisch** entschieden (Enden und
  ¼/½/¾ im Bild), unabhängig von Kurvenart und Parametrisierung (die Serie fand sonst 589 Doppellinien). Torus-Beinahe-Doppelwurzeln
  gelten als unsicher.
- **MAX_STROKES** zählt nach dem Verwerfen der entarteten Striche, die Rohgrenze beim Sammeln ist das Vierfache.
- **Prüfkörper:** Der Kernel kann zwei sich kreuzende Zylinder nicht schneiden; die Prüfkörper haben statt der runden
  Querbohrung einen um 45° gedrehten Vierkant-Durchbruch. Das Referenznetz der Tests rastet auf Merkmalskanten ein und erklärt
  Strahlen nahe Merkmalskanten und streifende Treffer für unentschieden (Grenzen im Test); ein eigener exakter CSG-Richter
  entscheidet Abweichungen.

Ausgabe
- **Ausgabekurven:** `piece_to_outs` liefert eine Liste (die Naht geschlossener Freiformkurven wird geteilt). Winkel können
  negativ oder größer als 2π sein; die Schreiber normalisieren.
- **SVG:** Der Plan nannte, ein gegen den Uhrzeigersinn laufender Bogen erscheine nach der Spiegelung als `sweep 0`; das ist
  falsch. Richtig (von Hand gerechnet, mit eigenem Leser und im Browser nachgewiesen): Im Blattsystem mit y nach oben, in der
  Gruppe `matrix(1 0 0 −1 0 H)`, bekommt ein gegen den Uhrzeigersinn laufender Bogen `sweep = 1`; `x-axis-rotation` ist der
  Blattwinkel der Hauptachse in Grad. Bögen werden in Stücken bis 90° ausgegeben (Halbbögen würden den rekonstruierten
  Mittelpunkt durch die Rundung auf sechs Stellen bis ~3e-3 mm verschieben), Splines mit Ziel 2e-7 mm als kubische Béziers,
  rationale Kegelschnitte ohne `A` (8 Kurvenstücke je Viertelkreis), Rotation auf 6 Stellen gerundet.
- **DXF:** 9 Nachkommastellen (Punktfehler ≤ ~1e-8 mm); ein Bogen über 0° hat Anfang > Ende (DXF-Semantik); Blocknamen:
  Komma ersetzt, Eindeutigkeit ohne Groß-/Kleinschreibung, Nicht-ASCII als `\U+XXXX`; minimales `DIMSTYLE`. Die Datei enthält
  einen Abschnitt `CLASSES` (zwei Klassen) und `OBJECTS` (Wurzel-`DICTIONARY` mit `ACAD_GROUP` und `ACAD_PLOTSTYLENAME`,
  Plotstil-Kette; jede Ebene hat 390), aber keine Layouts (`ACAD_LAYOUT`) und keine
  Verweise (340) an den Blockeinträgen. Fällt der Handtest in Fusion 360 negativ aus, ist das der erste Ausweichschritt,
  danach die Geometrie direkt in `ENTITIES`.
- **Dateiendung:** Eine fremde Endung wird angehängt (`plan.dxf` als SVG → `plan.dxf.svg`), nicht ersetzt.

Dokument, Datei, Oberfläche
- **Verdeckte Kanten (Abschlussprüfung, Entscheidung des Controllers):** Neue Ansichten zeigen verdeckte Kanten in den
  Normansichten (Vorne, Hinten, Links, Rechts, Oben, Unten) von Anfang an, weil die Zeichnungen für die Werkstatt gedacht
  sind; die isometrische Ansicht nicht. Die Regel steht an einer Stelle (`ViewKind::shows_hidden_by_default`, angewandt in
  `Drawing::add_view`); gespeicherte Zeichnungen behalten ihren Wert.
- **Datei:** Der Tag der Lage heißt `typ` (nicht `art`, wie bei Art und Ebene in den anderen Strukturen). Listenlängen werden
  beim Lesen begrenzt (`MAX_DRAWINGS`, `MAX_VIEWS`), Kennung 0 gilt als beschädigt, der Zähler der nächsten Zeichnung wird nur
  mit Zeichnungen gespeichert. Der Serde-Schalter `float_roundtrip` ist im Arbeitsbereich eingeschaltet, damit Positionen
  Speichern und Laden bitgenau überstehen.
- **Dokument:** Namen sind eindeutig nach `trim` und Kleinschreibung; ein Elternbild muss eine frühere Ansicht sein (keine
  Zyklen); `update_drawing` darf die Kennung nicht ändern.
- **Oberfläche:** Das Menü „Zeichnung“ steht zwischen „Erzeugen“ und „Bearbeiten“; alte, schreibgeschützte Versionen lassen
  das Ansehen zu und sperren alles Ändern; eine neue Ansicht wird nicht ausgewählt; das Ziehen schreibt je Zeigerbewegung
  eine Änderung. Abgelaufene Ergebnisse (anderer Modellstand) werden nicht angezeigt (`result(kind, model)`). Laufende Jobs
  für nicht mehr gebrauchte Arten werden verworfen (höchstens ein Eintrag je Art). Eine Ansicht ohne abgeleitete Ansichten
  wird ohne Rückfrage gelöscht. Der Export kennt den Zustand „nicht verfügbar“ (kein Körper, nur Netze, keine Zeichnung,
  Zeichnung ohne Ansicht) und sperrt das Schließen des Fensters, solange eine Datei geschrieben wird.
