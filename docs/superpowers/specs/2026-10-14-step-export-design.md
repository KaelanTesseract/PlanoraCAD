# Teilprojekt 10: STEP-Export (exakte Körper) – Entwurf

Stand: 2026-10-14. Baut auf Teilprojekt 9 (Netz-Austausch, `master`, Merge 53ae43e) auf. Teil 2 von 3 der Austausch-Strecke
(Teil 1: Netz-Austausch, Teil 3: Zeichnungen SVG/DXF).

## 1. Ziel und Abgrenzung

Das Programm schreibt seine exakten Körper (B-Rep des Kernels) als STEP-Datei, die andere CAD-Programme (Ziel: Fusion 360,
dazu FreeCAD, SolidWorks, Onshape u. a.) als Volumenkörper öffnen. Rundungen, Bohrungen und Freiformflächen bleiben exakt;
es gibt keinen Umweg über ein Dreiecksnetz.

**Entscheidungen (mit dem Nutzer abgestimmt, 2026-10-14):**

| Nr. | Entscheidung |
|---|---|
| E1 | Schema **AP214** (`AUTOMOTIVE_DESIGN`), Textdatei, Endung `.step`. Nur Geometrie, Namen, Einheit; keine Farben, Materialien, Baugruppenstruktur. |
| E2 | Mehrere Körper: **eine Datei, je Körper ein eigenes Teil** (Produkt) mit dem Namen aus dem Feature-Baum. |
| E3 | **Netz-Körper werden nicht ausgegeben** (nur exakte Körper); der Dialog nennt sie mit Namen und verweist auf STL/OBJ/3MF/glTF. |
| E4 | Nur Schreiben. STEP-Import ist ein eigenes späteres Teilprojekt. |
| E5 | Handtest-Gegenprobe in **Fusion 360** (nach Rückfrage beim Nutzer, vom Controller ausgeführt); Prüfung beim Bauen nur mit eigenem Leser und Formeln. |
| E6 | Keine neue externe Abhängigkeit (Textformat von Hand geschrieben). Lizenz: STEP-Entitätsnamen sind Teil der offenen ISO-Norm 10303; kein Autodesk-Material. |

**Nicht Teil:** STEP-Import, AP203/AP242, Farben, Materialien, Baugruppen mit Lagebeziehungen, PCURVEs/Trimmkurven in
Parameterform, Skizzen/Konstruktionsgeometrie, Netz-Körper (E3).

## 2. Aufbau

Neues Paket `planora-step` (Abhängigkeiten: `planora-kernel`, `planora-math`, `thiserror`).

```
planora-step/src/
  lib.rs       // write_step, StepBody, StepOptions, StepReport
  error.rs     // StepError (deutsche Meldungen)
  number.rs    // STEP-REAL-Format (17 gültige Stellen, immer mit Punkt, nie NaN)
  text.rs      // STEP-Zeichenketten (\X2\…\X0\, ' und \ maskieren)
  entity.rs    // Eintragsverwaltung (#n, fortlaufend, Zeilenaufbau)
  geometry.rs  // Punkte, Richtungen, Koordinatensysteme, Kurven, Flächen
  topology.rs  // Körper, Hüllen, Flächen, Schleifen, Kanten, Punkte
  product.rs   // Kopf, Produkt/Definition/Form, Einheiten, Kontext
```

**Schnittstelle:** `write_step(bodies: &[StepBody], opts: &StepOptions, out: &mut dyn Write, progress: &Progress) -> Result<StepReport, StepError>`.
`StepBody { name: String, solid: &Solid }`. `StepOptions { file_name: String, ... }`. `StepReport` nennt Körper-, Flächen-,
Kanten- und Eintragszahl.

**Kopf:** `ISO-10303-21;` `HEADER` mit `FILE_DESCRIPTION(('PlanoraCAD Export'),'2;1')`, `FILE_NAME(name, Zeitpunkt, (Autor leer),
(Organisation leer), 'PlanoraCAD', 'PlanoraCAD', '')`, `FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'))`, dann `DATA`.
Zeitpunkt UTC als `YYYY-MM-DDThh:mm:ss`.

**Kontext:** `GEOMETRIC_REPRESENTATION_CONTEXT(3)` mit `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT` (Unsicherheit aus der Kernel-Toleranz,
`LENGTH_UNIT`), `SI_UNIT(.MILLI.,.METRE.)`, `SI_UNIT($,.RADIAN.)`, `SI_UNIT($,.STERADIAN.)`.

**Je Körper (E2):** `APPLICATION_CONTEXT`, `PRODUCT`, `PRODUCT_DEFINITION_FORMATION`, `PRODUCT_DEFINITION`,
`PRODUCT_DEFINITION_SHAPE`, `SHAPE_DEFINITION_REPRESENTATION`, `ADVANCED_BREP_SHAPE_REPRESENTATION` mit dem Ursprungs-
`AXIS2_PLACEMENT_3D` und dem `MANIFOLD_SOLID_BREP`. Mit Hohlräumen (mehrere Hüllen): `BREP_WITH_VOIDS`.

## 3. Abbildung Kernel → STEP

| Kernel | STEP |
|---|---|
| `Solid` mit einer Hülle | `MANIFOLD_SOLID_BREP(name, CLOSED_SHELL)` |
| `Solid` mit mehreren Hüllen | `BREP_WITH_VOIDS(name, äußere Hülle, (ORIENTED_CLOSED_SHELL …))`; welche Hülle außen liegt: wie der Kernel/Validator sie unterscheidet (Volumenvorzeichen) |
| `Face` | `ADVANCED_FACE(name, (FACE_OUTER_BOUND, FACE_BOUND …), Fläche, same_sense)` |
| `Loop` | `EDGE_LOOP((ORIENTED_EDGE …))`; äußere Schleife `FACE_OUTER_BOUND`, innere `FACE_BOUND` |
| `HalfEdge` | `ORIENTED_EDGE(name,*,*,EDGE_CURVE,forward)` |
| `Edge` | `EDGE_CURVE(name, Start, Ende, Kurve, same_sense=true)`; Kanten laufen im Kernel von `t0` nach `t1` |
| `Vertex` | `VERTEX_POINT(name, CARTESIAN_POINT)` (ein Eintrag je Eckpunkt, gemeinsam genutzt) |
| `Surface::Plane` | `PLANE(AXIS2_PLACEMENT_3D)` |
| `Surface::Cylinder` | `CYLINDRICAL_SURFACE(placement, radius)` |
| `Surface::Cone { radius, slope }` | `CONICAL_SURFACE(placement, radius, semi_angle = atan(slope))`; Radius am Ursprung des Koordinatensystems; bei negativem Radius oder negativer Steigung wird die Lage entlang der Achse verschoben bzw. die Achse umgekehrt, so dass Radius ≥ 0 und Öffnungswinkel > 0 (Probe: Punkte des Kernels liegen auf der STEP-Fläche) |
| `Surface::Sphere` | `SPHERICAL_SURFACE(placement, radius)` |
| `Surface::Torus` | `TOROIDAL_SURFACE(placement, major, minor)` |
| `Surface::Nurbs` | `B_SPLINE_SURFACE_WITH_KNOTS` (nicht rational) oder zusammengesetztes Objekt `( BOUNDED_SURFACE() B_SPLINE_SURFACE(..) B_SPLINE_SURFACE_WITH_KNOTS(..) RATIONAL_B_SPLINE_SURFACE((gewichte)) REPRESENTATION_ITEM('') SURFACE() )`; Knoten als Werte plus Vielfachheiten |
| `Curve::Line` | `LINE(CARTESIAN_POINT, VECTOR(DIRECTION, 1.0))` |
| `Curve::Circle`/`Ellipse` | `CIRCLE(placement, r)` / `ELLIPSE(placement, a, b)` |
| `Curve::Nurbs` | `B_SPLINE_CURVE_WITH_KNOTS` bzw. zusammengesetzt mit `RATIONAL_B_SPLINE_CURVE`; geschlossene Kurven mit Kennzeichen `.T.` |

Flächen mit `same_sense == false` werden mit `.F.` geschrieben (die Fläche selbst wird nicht gespiegelt).

### Nähte, Pole, Vollkreise (aus dem Kernel gelesen)

- Gekrümmte Flächen haben im Kernel einen Parameterbereich `patch`; **Pole und Spitzen** (Kugel, Kegel) sind keine
  entarteten Kanten, sondern Eckpunkte; die Schleife besteht dort aus der Naht (Meridian) einmal vorwärts und einmal rückwärts.
  STEP wird genauso geschrieben: **ein** `EDGE_CURVE` für die Naht, zwei `ORIENTED_EDGE` mit entgegengesetzter Richtung. Das ist die
  übliche Darstellung vieler STEP-Dateien (z. B. Kugel als eine Fläche); ob Fusion 360 sie ohne Zusatzkante annimmt, klärt der
  Handtest. Fallback, falls nicht: Pol-Eckpunkt als `VERTEX_LOOP` ergänzen oder die Fläche in zwei Hälften teilen (eigene
  Folge-Aufgabe im Plan, nur bei Befund).
- **Vollkreise** (Anfang gleich Ende): derselbe `VERTEX_POINT` am Anfang und Ende.
- **Periodische/geschlossene** Flächen und Kurven: keine Besonderheit in STEP außer dem Kennzeichen „geschlossen“ bei B-Splines.

## 4. Zahlen, Texte, Schutz

- **Zahlen:** `REAL` mit kürzester Darstellung, die verlustfrei zurückgelesen wird (`{:?}`-Form), immer mit Punkt
  (`1.`, `0.5`, `1.E-7`); `-0.` wird `0.`; nicht endliche Werte → `StepError::NotFinite` (kein Schreiben).
- **Texte:** Namen als STEP-Zeichenketten: `'` → `''`, `\` → `\\`, nicht-ASCII → `\X2\…\X0\`, Steuerzeichen → Leerzeichen; Länge
  höchstens 256 Zeichen (gekürzt, im Bericht vermerkt).
- **Prüfung vor dem Schreiben:** jeder Körper besteht `validate`; sonst `StepError::InvalidBody { name, reason }` und der Körper
  wird ausgelassen (Bericht), die anderen werden geschrieben. Sind alle Körper abgelehnt: Fehler `NothingToExport`.
- **Speicher/Größe:** Es wird zeilenweise in den Ausgabestrom geschrieben (kein ganzer Text im Speicher); Eintragsnummern `u64`;
  höchstens `MAX_ENTITIES = 500_000_000`; Abbruch über `Progress`.
- Schreiben in eine Hilfsdatei mit Umbenennen, wie bei den Netzformaten (`planora-exchange::export_file`-Muster).

## 5. Prüfung (ohne fremdes Programm)

1. **Eigener STEP-Leser im Test** (`tests/common/reader.rs`): zerlegt Text in Einträge (inkl. zusammengesetzter Einträge),
   prüft Syntax, alle Verweise gültig, Richtungsvektoren normiert, Koordinatensysteme senkrecht.
2. **Geometrie unabhängig nachrechnen:** Der Test wertet Ebene, Zylinder, Kegel, Kugel, Torus, NURBS aus den *gelesenen* Zahlen
   mit eigener Formel aus und vergleicht Punkte mit dem Kernel; jeder Eckpunkt liegt auf seinen Kurven, jede Kante auf beiden
   Nachbarflächen (Toleranz benannt).
3. **Topologie:** jede Kante je Hülle genau zweimal in entgegengesetzter Richtung, jede Schleife geschlossen, Euler-Zahl
   stimmt (V − E + F = 2 − 2g je Hülle, soweit aus den Daten bestimmbar).
4. **Formeln:** Quader, Zylinder, Kegel, Kugel, Torus, gerundete Körper (TP8), Freiformkörper (TP7): Flächeninhalt und
   Volumen, aus den gelesenen STEP-Flächen berechnet (Dreiecksnetz aus dem Leser, fein), stimmen mit den Kernel-Werten.
5. **Feindliche Eingaben:** ungültige Körper (z. B. offene Hülle), NaN-Koordinaten, leere Namen, Namen mit Sonderzeichen,
   sehr viele Körper: benannter Fehler, nie Absturz.
6. **Handtest in Fusion 360** (E5): Datei öffnen, Körper erkannt (Volumenkörper), Volumen und Flächenzahl vergleichen.

## 6. Oberfläche

Im Fenster „Datei → Exportieren …“ kommt als fünftes Format **„STEP – für andere CAD-Programme (exakt)“** dazu.
- Die Karte „Qualität“ entfällt bei STEP; an ihrer Stelle steht „Rundungen und Bohrungen bleiben exakt.“ (einfache Sprache).
- Umfang wie bisher; Netz-Körper werden ausgelassen, ein Hinweis nennt sie namentlich (E3).
- Statt „Dreiecke, ca. MB“ zeigt das Fenster „n Körper, n Flächen“; abgelehnte Körper stehen mit Grund in der Liste.
- Schreiben im Arbeitsthread mit Fortschritt und Abbrechen, Hilfsdatei, Ersetzen-Rückfrage, Schlüsselprüfung gegen veraltete
  Ergebnisse (wie Task 16 des Teilprojekts 9).
- Die Dateiauswahl nutzt die vorhandene eigene Auswahl; Endung `.step`.

## 7. Stufen

1. **Gerüst:** Paket, Eintragsverwaltung, Zahlen/Texte, Kopf/Kontext/Teile, Ebene/Gerade/Kreis, Topologie; Quader und Prismen; eigener Leser mit Geometrie- und Topologieprüfung; Stufentest 1.
2. **Gekrümmte Standardflächen:** Zylinder, Kegel (Vorzeichenregeln), Kugel, Torus, Ellipse; Nähte, Spitzen, Pole; Formeltests; Stufentest 2.
3. **Freiformen und Sonderfälle:** NURBS-Flächen/-Kurven (rational, geschlossen), Hohlräume (`BREP_WITH_VOIDS`), Körper aus Rundungen/Fasen/Booleans, Zufallsreihe; Stufentest 3.
4. **Fenster, Doku, Gesamtprüfung:** Format im Export-Dialog, README, offene Punkte, Spec §9 „Änderungen bei der Umsetzung“, Handtest in Fusion 360 (nach Rückfrage).

## 8. Risiken

- **Fremdprogramme sind strenger als der eigene Leser.** Fusion 360/OpenCASCADE verlangen z. B. Pole als Eckpunkte, positive Kegelwinkel, geschlossene Schleifen mit exakt passenden Eckpunkten; der Handtest (E5) ist die einzige echte Gegenprobe. Befund dort = Folge-Aufgabe, nicht Nacharbeit unter Zeitdruck.
- **Kegel- und Torusflächen:** Vorzeichen und Lage der Koordinatensysteme sind fehleranfällig; deshalb Punktprobe Kernel ↔ gelesene STEP-Fläche (Abschnitt 5, Punkt 2) für jeden Flächentyp, auch gespiegelt/gedreht.
- **Freiformflächen:** Knotenvielfachheiten und Gewichte müssen exakt stimmen; Probe über Auswertung der geschriebenen B-Spline-Daten mit unabhängigem de-Boor.
- **Große Körper:** zeilenweises Schreiben, aber der Kernel hält den ganzen Körper im Speicher (kein neues Risiko).
- **Namen:** Feature-Namen können Sonderzeichen enthalten (Abschnitt 4).

## 9. Änderungen bei der Umsetzung

(wird beim Abschluss des Teilprojekts ergänzt)
