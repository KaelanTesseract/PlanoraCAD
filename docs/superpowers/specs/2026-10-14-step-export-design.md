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
  Handtest. Fallback, falls nicht: die Kugel an einem zweiten Meridian (u = π) in zwei Flächen teilen (eigene Folge-Aufgabe im
  Plan, nur bei Befund). *(Berichtigt in der Umsetzung: Die ursprünglich genannte Ausweichlösung „Pol-Eckpunkt als
  `VERTEX_LOOP` ergänzen“ war falsch, weil der Pol bereits ein Eckpunkt ist; siehe §9.)*
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

Stand: 2026-10-14. Plan: `docs/superpowers/plans/2026-10-14-step-export.md`. Die Entscheidungen P1–P15 wurden beim Planen
getroffen und konkretisieren die Spec; die Tabelle nennt je Zeile das Ergebnis aus der Umsetzung.

| Nr. | Entscheidung | Begründung und Ergebnis |
|---|---|---|
| P1 | `planora-step` hängt von `planora-kernel`, `planora-math`, `thiserror`, `planora-exchange` (`Progress`, `temp_path`, `CHECK_EVERY`) und ab Task 11 von `planora-mesh` ab | Die Schnittstelle nimmt `&Progress`; das Vorzeichen des Schalenvolumens kommt aus dem Netz. Nur Arbeitsbereichs-Pakete, keine neue externe Abhängigkeit (`cargo deny check` grün). |
| P2 | `write_step(&[StepBody], &StepOptions, &mut dyn Write, &Progress) -> Result<StepReport, StepError>`; dazu `check_body`, `export_step_file`, `display_name`, `utc_timestamp` | Umgesetzt wie geplant. `StepReport { bodies, faces, edges, entities, rejected, shortened_names }`. |
| P3 | `check_body`: Verweise prüfen, `validate_with`, dann einmal probeweise nach `io::sink()` schreiben | Geometrische Ablehnungen fallen auf, bevor ein Eintrag geschrieben ist. `NotFinite` eines Körpers wird `InvalidBody` dieses Körpers, die anderen werden geschrieben. Kosten: zwei Durchläufe je Körper, mit `prepare_step` im Fenster drei. Ein Körper ohne Flächen wird abgelehnt (Befund Task 3: sonst entstünde `CLOSED_SHELL('',())`, ungültig nach ISO 10303-42). |
| P4 | Einträge von unten nach oben; Umbruch ab Spalte 72; Abbruchfrage alle 4096 Einträge; Fortschritt zählt Einträge | Umgesetzt. Die Gesamtzahl für den Fortschritt kommt aus dem Probelauf plus feste Einträge (`CONTEXT_ENTITIES = 9`, `PART_ENTITIES = 11`, Befund Task 3). Abbrechen wirkt beim Prüfen zwischen den Körpern und alle 4096 Einträge, beim Schreiben alle 4096 Einträge. |
| P5 | `EDGE_CURVE` mit `same_sense = (t1 ≥ t0)` | Spec §3 schreibt immer `.T.`; Bögen im Uhrzeigersinn haben im Kernel `t1 < t0`, sonst liefe die Kante den falschen Bogen entlang. |
| P6 | Kegel: Lage auf die Mitte des v-Bereichs, bei fallendem Radius Achse umgekehrt, halber Öffnungswinkel `atan(\|slope\|)`; Mitte hinter der Spitze → benannte Ablehnung | Umgesetzt (Task 8). Nur die Mitte wird auf die Spitze geprüft; der Kernel erzeugt keine Fläche, die über die Spitze reicht. `slope = 0` wird `CYLINDRICAL_SURFACE`. |
| P7 | Ellipse mit `minor > major`: Bezugsrichtung `y`, Halbachsen vertauscht | Parameter um π/2 verschoben, die Kante wird über ihre Eckpunkte begrenzt. Test: `ellipses_with_the_short_axis_first_are_turned_around`. |
| P8 | B-Splines: Knoten als verschiedene Werte mit Vielfachheiten; rational als zusammengesetzter Eintrag in alphabetischer Teile-Reihenfolge **mit** `GEOMETRIC_REPRESENTATION_ITEM()`; „geschlossen“ nach Kernel-Prüfung; Kontrollpunkte zeilenweise in u | Das Teil `GEOMETRIC_REPRESENTATION_ITEM()` fehlt in der Tabelle von §3, die Norm verlangt es. Die Gewichtsreihenfolge ist mit asymmetrischen Gewichten (Grad 3, `[1,3,0.5,2,1]`) abgesichert (Mutationsprobe: umgekehrte Gewichte → rot). |
| P9 | Mehrere Schalen: Volumenvorzeichen des groben Netzes (`CLASSIFY_CHORD = 0,05 mm`) trennt äußere Schalen von Hohlräumen; Hohlraum gehört zur kleinsten äußeren Schale um seinen ersten Eckpunkt; getrennte Stücke bleiben ein Teil mit mehreren Körpern | Hohlraumschale umgekehrt geschrieben (`same_sense` gekippt, Ränder `.F.`), eingetragen als `ORIENTED_CLOSED_SHELL('',*,#…,.F.)` (ISO 10303-42). Zusatztest für Schachtelung (Mutationsprobe `min_by` → `max_by` rot). Grenze: nur der erste Eckpunkt wird gegen das grobe Netz geprüft; bei verschachtelten Hohlräumen (Körper ⊃ Hohlraum ⊃ Insel ⊃ Hohlraum) kann ein Hohlraum, dessen erster Eckpunkt näher als 0,05 mm an einer gekrümmten Fläche der Insel liegt, falsch zugeordnet werden (sehr unwahrscheinlich). Körper mit mehreren Schalen werden zweimal klassifiziert. |
| P10 | Unsicherheit `tolerance.linear` (1e-6 mm), bei Freiform `max(linear, NURBS_EDGE_TOL)` = 1e-5 mm | So weit dürfen Kanten bei Freiformflächen von ihren Flächen abweichen (gemessen bis 5,1e-7 beim „Kreuz“). |
| P11 | Gemeinsamer Kontext einmal je Datei, je Teil `PRODUCT` bis `ADVANCED_BREP_SHAPE_REPRESENTATION` | Umgesetzt; ISO-10303-214-Aufbau vom Reviewer mit Quader, Prisma mit Loch, gespiegelt/gedreht und Boolean-Ergebnis geprüft. |
| P12 | Leerer Name → „Körper n“; Kürzung auf 256 Zeichen vor der Maskierung; Zeichen über U+FFFF als `\X4\…\X0\` | Umgesetzt. Gekürzt wird an einer Zeichengrenze, nicht an einer Graphem-Grenze. Im Fenster werden Kürzungen nicht gemeldet (der Bericht kennt sie). |
| P13 | Prüfwerkzeug in `tests/common`: `reader.rs`, `model.rs`, `check.rs`; Zuordnung Teil i ↔ i-ter angenommener Körper, Hüllen über Flächenzahl und ersten Eckpunkt, „Netz aus dem Leser“ aus dem Kernel-Netz mit Punkten auf den STEP-Flächen | Umgesetzt. Prüfungen: Topologie (jede Kante zweimal gegenläufig, Schleifen geschlossen, Euler), Geometrie (Eckpunkte auf Kurven, Kanten auf beiden Flächen), Kernelvergleich (Punkte und Außennormalen), Netzvergleich (umgedrehte Dreiecke = 0, Volumen und Abstand). Schranken siehe unten. |
| P14 | `ExportFormat { Mesh(Format), Step }` statt `Format` im Fenster; Schlüssel des Ergebnisses `None` bei STEP; Netz- und STEP-Ergebnis getrennt | Eine Zeile in `tests/export_dialog.rs` mechanisch angepasst (mit Kommentar). Prüfen und Schreiben im Arbeitsthread (`prepare_step`, `write_progress`, `cancel_write`). |
| P15 | Handtest in Fusion 360 nur nach Rückfrage, vom Controller | **Erledigt 2026-10-14** (Release-Build, vom Nutzer selbst): alle elf Dateien öffnen in Fusion 360 ohne Fehler; Kugel (1436,755 mm³, 615,752 mm², exakte Kugel r = 7; der Tabellenwert 1436,072 ist das Prüfnetz) und Hohl (936,00 mm³, 696,00 mm²) stimmen, beide als Volumenkörper; nicht verglichen: Volumen und Flächenzahl der übrigen neun, Fusion-Version, Export über das Fenster. Beispieldateien erzeugt `write_samples_for_the_hand_test` (`PLANORA_STEP_SAMPLES`), `Alle.step` mit 10 Teilen. |

### Weitere Abweichungen von §1–§8

**Aufbau (§2)**
- Zusätzliche Pakete `planora-exchange` und `planora-mesh` (P1). Ein Modul `file.rs` (`export_step_file`: Hilfsdatei, Umbenennen) kam dazu.
- Ein gemeinsamer Kontext je Datei statt je Körper (P11).
- Die Hilfsdatei hat wie bei den Netzformaten einen festen Namen (`.Name.planora-tmp`); zwei gleichzeitige Exporte in dasselbe Ziel würden kollidieren (das Fenster startet nur einen).

**Abbildung (§3)**
- `EDGE_CURVE`: `same_sense = (t1 ≥ t0)` statt immer `.T.` (P5).
- Rationale B-Splines mit zusätzlichem `GEOMETRIC_REPRESENTATION_ITEM()` (P8). Eine Kurve oder Fläche wird als rational geschrieben, sobald ein Gewicht bitgleich ungleich 1 ist.
- Hohlraumschale umgekehrt mit `ORIENTED_CLOSED_SHELL … .F.`; getrennte Stücke als mehrere Körper eines Teils (P9).
- Kegel: Lage auf der Flächenmitte, Achse umgekehrt bei fallendem Radius (P6).
- **Berichtigung der Ausweichlösung bei Polen:** Die in §3 genannte Ausweichlösung „Pol-Eckpunkt als `VERTEX_LOOP` ergänzen“ ist falsch, denn der Pol ist im Kernel und in der Datei schon ein Eckpunkt (`VERTEX_POINT`); ein `VERTEX_LOOP` würde keine Schleife schließen, die es nicht schon gibt. Die echte Ausweichlösung bei einem Befund im Handtest ist, die Kugel an einem zweiten Meridian (u = π) in zwei Flächen zu teilen. Wie OpenCASCADE eine Kugel schreibt (eine Fläche, Naht zweimal, Pole als Eckpunkte), ist die geschriebene Form.
- **Spindeltorus (Ruling Task 7):** Der Kernel kann über „Drehen“ eines Bogens, der die Achse berührt oder überschreitet, einen Torus mit kleinem Radius ≥ großem Radius erzeugen (nur das Runden lehnt ihn ab). Für ihn gibt es in der Spec kein Bild. `TOROIDAL_SURFACE` mit `minor ≥ major` ist für fremde Programme ungültig; `DEGENERATE_TOROIDAL_SURFACE` wäre die Norm-Form, ließ sich aber ohne fremdes Programm weder prüfen noch verifizieren. Entscheidung: benannte Ablehnung („Torus mit kleinem Radius größer oder gleich dem großen …“) des betroffenen Körpers, im Schreiber und im Prüfmodell; kein unbelegter Schreiber. Folge: Solche Körper lassen sich nicht als STEP ausgeben, bis ein geprüfter `DEGENERATE_TOROIDAL_SURFACE`-Schreiber vorliegt (Folge-Aufgabe).
- **Kegelspitze (Task 8):** Der Kernel setzt die Spitze auf eine Stelle, die um ca. 1e-15 neben der Achse liegt. Im Prüfmodell (`Surf::foot`) gilt ein Punkt mit Achsabstand ≤ `64 · ε · max(1, |Koordinaten|)` als Spitze ohne Normale; sonst entstanden zufällige Normalen und falsche Alarme in schrägen Lagen. Der Schreiber war davon nicht betroffen.

**Zahlen, Texte, Schutz (§4)**
- `NotFinite` in einem Körper lehnt diesen Körper ab, statt die Datei abzubrechen (P3). Der Fehler `NotFinite` bleibt als Fehlerart bestehen (z. B. für die Zahlenausgabe).
- Leere Namen werden „Körper n“ (P12).
- Der Fortschritt zählt feste Einträge mit (P4).

**Prüfung (§5)**
- Das „Dreiecksnetz aus dem Leser“ (§5 Punkt 4) kommt aus der Verbindung der Dreiecke des Kernel-Netzes; jeder Punkt wird auf die STEP-Fläche gelegt und jedes Dreieck nach deren Außennormale ausgerichtet (P13).
- **Prüfwerkzeug, Freiform (Nachbesserung):** Sobald ein Modell Freiformkurven oder -flächen enthält, gilt für alle Lagebedingungen `FREEFORM_TOL` (`body_tol`), da die Datei dann die Unsicherheit 1e-5 mm trägt (P10); Prüfer-Befund: Eckpunkt 6e-9 neben einer Geraden bei gedrehtem NURBS-Schnitt. Neue Tests: beide Schnitte in drei Lagen; ein um 1e-3 verschobener Eckpunkt wird gefunden. Beim Ausrichten zählen Dreiecke mit Kreuzprodukt unter `FLIP_MIN_CROSS` = 1e-9 nicht (Splitter aus Rundungsrauschen, gemessen bis 6e-15).
- **Schranken im Prüfwerkzeug:** Die Geometrieschranke ist relativ: `geom_tol = max(1e-11, 64 · ε · größter Koordinatenbetrag)`. Die erste feste Schranke 1e-7 wurde auf gemessene 1,4e-14 gesenkt (Task 7), dann relativ gemacht, weil die feste Schranke ab etwa 1e4 mm (große oder weit verschobene Körper) nicht mehr hielt; ein Test mit Körpern bei 2e5 mm trennt sie von der festen. Freiform: bis `NURBS_EDGE_TOL`. Netzvergleich: Volumenunterschied zum Kernel-Netz höchstens `1e-9` (analytisch) bzw. `1e-7` (Freiform), Abstand 1e-9 bzw. `NURBS_EDGE_TOL`; gemessen höchstens 5,5e-15 bzw. 5,2e-11. Formelvolumen für Rundung, Fase, Kuppel und Kreuz mit Sehnenschranke auf die gekrümmte Fläche bezogen. Alle Tests mit festem Startwert.
- **Messwerkzeug-Fix Ellipse (Task 12):** Die lange Zufallsreihe (Fall 200: Ellipse mit Halbachsen 2079,86 und 3,17 mm) meldete einen Eckpunkt 6,9e-11 neben der Kurve. Nachrechnen mit 60 Stellen ergab 2,5e-13: Das Messwerkzeug (`Crv::distance`) nahm den Winkel im zum Kreis gestreckten Raum und vergrößerte Rundung um das Achsenverhältnis. Behoben im Prüfwerkzeug (Newton auf dem echten Abstand, `ELLIPSE_NEWTON_STEPS = 8`), Schranken nicht gelockert; Test `the_distance_to_a_very_slender_ellipse_is_not_amplified_by_its_shape` (mit 0 Schritten rot). Schreiber und Kernel waren in Ordnung.
- Die Zufallsreihe nutzt nur Paare, die der Kernel rechnen kann: Schräge Zylinder mit Zylindern oder Kugeln rechnet er noch nicht, die Reihe deckt Quader mit Quader, Zylinder oder Kugel ab. Von 300 Fällen werden 20 (leere Schnittmenge, `BooleanError::EmptyResult`) übersprungen; jeder andere Kernelfehler lässt die Reihe scheitern; Mindestanteil gültiger Fälle 0,8 (gemessen 0,93).
- Das Prüfmodell liest `knot_spec` strikt als `.UNSPECIFIED.` und lehnt `TOROIDAL_SURFACE` mit `minor ≥ major` benannt ab.

**Oberfläche (§6)**
- `ExportFormat` statt `Format` (P14).
- **Schlüsselprüfung (Task 13):** `write_to` prüft im Zweig Step den Schlüssel `(Option<Quality>, Scope, edit_count, Option<BodyRef>)` des Ergebnisses; ohne diese Prüfung schreibt das Fenster ein veraltetes Ergebnis (Mutationsprobe: `a_stale_step_result_is_never_written` rot).
- **Fenster schließen während STEP geschrieben wird (Korrekturwelle):** Befund: Schließen, X oder Skizzenmodus verwarfen den Dialog, der Job brach ab, die Hilfsdatei verschwand ohne Meldung, die alte Datei blieb. Behoben: `ExportDialog::request_close` weist „Schließen“ und das X beim STEP-Schreiben ab (Hinweis „Das Schreiben läuft noch. Bitte warten oder erst abbrechen.“); ebenso bleibt das Fenster im Skizzenmodus. Abbrechen meldet „Abgebrochen“. Tab-Schließen und Beenden sind bewusst nicht gesperrt (eine Sperre würde bei einem Hintergrund-Tab ohne `tick` oder einem hängenden Schreiben das Programm unbeendbar machen). **Grenze:** Beendet man das Programm oder schließt man den Tab, während eine STEP-Datei geschrieben wird, wird das Schreiben abgebrochen und die alte Datei bleibt unverändert (ohne Meldung).
- **Nachbesserungen des Fensters (vor dem Zusammenführen, alle mit Test und Mutationsprobe):** Der Abbrechen-Knopf hängt am laufenden Schreibweg (`is_writing_step`), nicht am gewählten Format. Die Meldung „Fertig“ wird dem Format zugeordnet, mit dem geschrieben wurde (`written_format`), und beim Formatwechsel gelöscht (Klicktest). `cancel()` und `recompute()` betreffen nur das gewählte Format; das Netz-Ergebnis bleibt. „x.stp“ wird zu „x.step“ (`normalize_target`, Rückfrage beim Ersetzen über `accept_chosen`). Gekürzte Namen: Hinweis nach dem Schreiben, lange Namen in Listen auf 40 Zeichen gekürzt. Der Hinweis am gesperrten Knopf bei 0 Körpern ist `STEP_NOTHING` (`disabled_hint`). `check_body(body, opts, progress)` nimmt die Toleranz aus `StepOptions`. Offen: `.stp`-Dateien stehen nicht in der Dateiliste.

**Weitere Befunde der Stufenprüfungen (alle behoben, soweit nicht oben genannt)**
- Task 3: Körper ohne Flächen (siehe P3), Fortschrittssumme (P4), Test für Abbruch mitten im Schreiben.
- Task 5: Zusatztests für Namens-Rundlauf (`'`, `\`, Umlaute, Emoji) und für lückenlose Eintragsnummern `1..=n`.
- Task 9: Sehnenschranke in Stufentest 2 auf die gekrümmte Fläche bezogen; Spiegeltest vergleicht zusätzlich Volumen und Abstand zum Kernel (Nachtrag in Task 12).
- Task 10/12: freie Zahlen in Tests zu benannten Konstanten (`MAX_OFFSET`, `CORNER_NEAR`, `SERIES_SEED`); `assert_eq!(flipped, 0)` im Hohlraumtest.
- Zurückgestellt (nicht behoben): Der Leser hat keine Tiefenbegrenzung (Absturz bei 100 000 verschachtelten Klammern, nur Prüfwerkzeug); Tokenfehler nennen nur die Byte-Stelle; die Spalte beim Umbruch zählt Bytes und Zeichen gemischt (in der Praxis nur ASCII); Prüfmodell wendet keine Orthogonalisierung nach Teil 42 an (< 1e-9 Länge); Richtungsvektor Null wird nicht geprüft (der Kernel erzeugt keinen); nur die Kegelmitte wird auf die Spitze geprüft; die Kegelschranke des Kernels für „fast Zylinder“ (1e-6) ist gröber als die Geometrieschranke.

**Teststand (2026-10-14)**
- `cargo test --workspace` (Debug): 1404 bestanden, 0 fehlgeschlagen, 31 ignoriert in 193 Testprogrammen, Gesamtlaufzeit etwa 9,6 Minuten mit Übersetzung. Lange Reihe (`cargo test -p planora-step --release -- --ignored`): 2 bestanden in 3,7 s (die 300-Fälle-Reihe und das Schreiben der Beispieldateien).
- Kernel, `planora-math`, `planora-mesh`, `planora-exchange`, `planora-feature`, `planora-project`, `planora-render`, `planora-sketch` und die Golden-Dateien sind bitgleich zu `07bac2b`; geändert wurden nur `planora-step` (neu) und `planora-app`.
- Gegenprobe in einem fremden CAD-Programm: **nicht geschehen** (P15).
