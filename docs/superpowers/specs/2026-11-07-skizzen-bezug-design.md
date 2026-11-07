# Skizzen 6: Projizieren mit Bezug, Schneiden, Silhouette, Konstruktionsebenen

Stand 2026-11-07. Sechstes von zehn Skizzen-Teilprojekten (Vergleich in der Vault-Notiz „2026-10-31 Vergleich Skizzen mit Fusion“). Dennis hat den Plan am 2026-11-07 freigegeben („OK, so machen“), samt Claudes Empfehlungen zu den offenen Fusion-Fragen. Maßstab ist Fusions Verhalten. Wo es hier aus dem Gedächtnis beschrieben ist, gilt es als Annahme und kommt in `offene-punkte.md`.

## Warum

In Fusion beziehen sich Skizzen auf den Körper: Man projiziert eine Kante, eine Fläche oder einen ganzen Körper, die lila Geometrie folgt jeder Änderung, und man baut daraus Profile. Man schneidet einen Körper mit der Skizzenebene und skizziert auf Ebenen, die nicht am Körper kleben (Versatz, Winkel, Mitte). Heute gibt es nur „Kante kopieren“: Die Kopie folgt zwar der Kante, ist aber Konstruktionsgeometrie, nicht lila, nicht lösbar, und ein verlorener Bezug macht das ganze Skizzen-Feature kaputt. Die Skizzenebene lässt sich nach dem Anlegen nicht ändern.

## Bestand (geprüft im Code)

- `Tool::ProjectEdge` → `State::project_edge_into_sketch` → `SketchSession::project_edge` → `Sketch::add_projected_edge`. Der Verweis steht als `EdgeCopy { curve, edge: EdgeRef }` am `SketchFeature`.
- `compute_sketch` führt jede Kopie mit `Sketch::reproject_edge` nach, solange die Kante ihre Art behält; sonst Fehler am Feature.
- Projiziert werden Geraden, Kreise und Bögen in parallelen Ebenen, ganze Kreise und Ellipsen auch schräg (als Ellipse). Abgelehnt: schräge Bögen, Ellipsenbögen, Freiformkanten.
- Kernel: `intersect_surfaces` kennt Ebene gegen Ebene, Zylinder (auch schräg, Ellipse), Kugel, Kegel und Torus in besonderer Lage. Das Beschneiden auf das Flächeninnere gibt es nur innerhalb der Booleschen Operationen (`imprint.rs`).

## Ziele

### A. Projizieren wie in Fusion
1. Neue Projektionen sind **normale Kurven** (keine Konstruktion), **fest** und **lila**, und sie können Profile bilden. Konstruktion umschalten geht auch an ihnen (Annahme: wie in Fusion).
2. Kopien aus alten Dateien bleiben, wie sie sind (Konstruktion), damit die Dateien bytegleich bleiben. Sie werden ebenfalls lila gezeigt und folgen weiter.
3. Schräge Bögen und Ellipsenbögen werden zu **elliptischen Bögen** projiziert (die Skizze kennt sie schon).
4. **Verlorener Bezug:** Lässt sich die Kante nicht mehr finden oder ändert sie ihre Art, bleibt die letzte Geometrie stehen. Das Feature bekommt eine **Warnung** statt eines Fehlers, wie in Fusion. Abhängige Features rechnen weiter.
5. **Bezug lösen** (Fusion: „Verknüpfung aufheben“) im Kontextmenü einer projizierten Kurve: Sie wird normale Geometrie mit `Fest`, folgt nicht mehr und ist nicht mehr lila. Ein Rückgängig-Schritt.
6. Löschen einer projizierten Kurve löscht ihren Bezug mit.

### B. Flächen und Körper projizieren
1. Das Projizieren-Fenster hat wie in Fusion den Auswahlfilter **„Angegebene Elemente“ / „Körper“**. Taste **P**.
2. Eine Fläche ergibt ihre Randschleifen (außen und Löcher), je Kante eine verknüpfte Kurve.
3. Ein Körper ergibt alle Kanten, die sich nicht zu einem Punkt projizieren. Deckungsgleiche Projektionen kommen nur einmal (Fusions Umriss als 2D-Vereinigung ist Annahme; siehe offene Punkte).
4. Der Verweis wird je Kurve gespeichert (je Kante ein `EdgeCopy`). Die Kurvenmenge wird beim Anlegen festgelegt: Jede Kurve folgt ihrer Kante, eine verlorene Kante bleibt nach A4 stehen. Kanten, die der Körper später neu bekommt, kommen nicht von selbst hinzu (Abweichung: Fusion ergänzt sie vermutlich; sonst müsste die Neuberechnung Kurven in die Skizze einfügen, deren Ids der Nutzer nicht kennt; kommt in `offene-punkte.md`).
5. Im Fenster gibt es wie in Fusion den Schalter **„Projektionsverknüpfung“** (Vorgabe an). Aus: feste Geometrie ohne Bezug.

### C. Schneiden
1. **Schneiden** (Fusion: Schneiden / Intersect): gewählte Körper oder Flächen werden mit der Skizzenebene geschnitten. Die Schnittkurven werden lila, fest, verknüpft (wie A).
2. Neuer Kernel-Baustein `section_by_plane(solid, frame, faces)` aus den vorhandenen Teilen (`intersect_surfaces`, `curve_surface_roots`, `point_in_face`): Schnittkurve je Fläche an den Randtreffern teilen, Stücke im Flächeninneren behalten, doppelte Stücke an gemeinsamen Kanten zusammenlegen, ganz in der Ebene liegende Flächen liefern ihre Ränder.
3. Nur analytische Flächen. Ebene gegen schrägen Kegel (Kegelschnitt), Torus in allgemeiner Lage und Freiformflächen werden mit Meldung abgelehnt.

### D. Silhouette
1. **Silhouette** als eigener Eintrag im Klappmenü (Annahme; in Fusion ggf. Option). Umriss gekrümmter Flächen in Blickrichtung der Skizze (senkrecht zur Ebene):
   - Zylinder und Kegel mit Achse parallel zur Skizzenebene: zwei Linien.
   - Kugel: Kreis.
   - Torus mit Achse senkrecht zur Ebene: zwei Kreise.
2. Beschnitten auf die Fläche wie in C. Andere Lagen werden mit Meldung abgelehnt.

### E. Konstruktionsebenen
1. Neues Feature **Ebene** im Feature-Baum, Menü „Konstruieren“, mit den Arten:
   - **Versatzebene** (Grundebene, ebene Fläche oder andere Konstruktionsebene; Abstand, auch negativ, auch Formel).
   - **Ebene unter Winkel** (um gerade Kante, Skizzenlinie oder X/Y/Z-Achse; Winkel, auch Formel).
   - **Mittelebene** (zwischen zwei Ebenen/ebenen Flächen; bei nicht parallelen die Winkelhalbierende).
   - **Ebene durch drei Punkte** (Eckpunkte von Körpern, Skizzenpunkte, Ursprung).
   - **Tangentialebene** (an Zylinder, Kegel oder Kugel, durch Bezug auf eine Ebene ausgerichtet; Winkel).
2. Die Ebene folgt ihren Bezügen; ein verlorener Bezug ist ein Fehler am Ebenen-Feature (wie andere Features).
3. Anzeige in 3D halbdurchsichtig (orange wie Fusion), mit Rand, anklickbar; im Browser mit Kontextmenü (Bearbeiten, Skizze erstellen, Unterdrücken, Löschen). Gezeichnet als Überlagerung wie fertige Skizzen, also ohne Verdeckung durch Körper (Abweichung; eine eigene GPU-Pipeline mit Tiefentest ist ein eigener Punkt). Beim Anklicken gewinnt eine Fläche oder Kante, die näher liegt.
4. Skizzen auf einer Konstruktionsebene (`SketchPlane::OnPlane(FeatureId)`); die Skizze folgt der Ebene.
5. Ein Eckpunkt wird als „Ende einer Kante“ gespeichert (`EdgeRef` + Ende), ohne neue Verweisart.

### F. Skizzenebene neu festlegen
1. Kontextmenü einer Skizze: **„Skizzenebene neu festlegen“** (Fusion: Redefine Sketch Plane). Danach eine Grundebene, ebene Fläche oder Konstruktionsebene wählen.
2. Die 2D-Geometrie bleibt, Projektionen werden auf der neuen Ebene nachgeführt.
3. Bezüge auf Features, die nach der Skizze kommen, werden abgelehnt (die Reihenfolge im Baum bleibt).
4. Ein Rückgängig-Schritt; Esc hinterlässt nichts.

### Für alle Teile
- Klappmenü **„Projizieren/Einschließen“** in der Skizzenleiste statt des bisherigen Knopfs (Leiste bleibt bei 1080 px gleich breit). Einträge: Projizieren, Schneiden, Silhouette.
- Alle Befehle in den Einstellungen belegbar; P für Projizieren wie in Fusion, die übrigen ohne Vorgabe (wie Fusion).
- Texte nur über `tr!`/`msg!`, de und en, Namen wie in der deutschen/englischen Fusion.
- Ein Befehl = ein Rückgängig-Schritt; Abbruch hinterlässt nichts.
- **Dateiformat 16** nur, wenn eine Datei Neues enthält (neue Projektionsart, gelöster Bezug als Warnzustand, Konstruktionsebene, `OnPlane`). Alte Dateien bleiben bytegleich.

## Nicht-Ziele

- Projizieren auf eine Fläche, Schnittkurve als 3D-Kurve, 3D-Skizze (Teilprojekt 10).
- Freiformkanten projizieren: geht genau erst mit Splines über Kontrollpunkte (Teilprojekt 7).
- Kegelschnitt aus Ebene und schrägem Kegel (Teilprojekt 7), Torus allgemein und Freiformflächen beim Schneiden (eigene Kernel-Arbeit, eigener Punkt).
- Konstruktionsachsen und -punkte als eigene Features; „Ebene durch zwei Kanten“, „Tangential zu Fläche an Punkt“, „Ebene entlang Pfad“.
- Ziehpfeil in 3D für Versatz und Winkel (Feld und Vorschau reichen; Abweichung in `offene-punkte.md`).
- Anzeige-Extras (Blick auf Skizze, Slice, Ein/Aus), Release, CI.

## Anti-Ziele

- Eine Änderung am Körper lässt eine projizierte Kurve an der alten Stelle stehen, obwohl der Bezug noch gefunden wird.
- Ein verlorener Bezug macht die Skizze oder ihre Extrusionen kaputt.
- Abhängigkeiten des Nutzers an projizierten Kurven gehen beim Nachführen verloren.
- Projizieren eines Körpers legt doppelte, übereinanderliegende Kurven an.
- Schneiden liefert Stücke außerhalb der Fläche oder lässt Stücke still weg.
- Alte Dateien ändern sich beim Speichern.
- Die Leiste wird breiter als bei 1080 px Platz ist.

## Wie (Entscheidungen)

- Projektionslogik ohne egui in `planora-sketch` (2D) und `planora-feature` (Bezüge, Nachführen), Kernel-Baustein in `planora-kernel/boolean`.
- Die Herkunft einer Projektion (`EdgeCopy` erweitert: Kante, Fläche, Körper, Schnitt, Silhouette; Zustand verknüpft/verloren) bleibt am `SketchFeature`; die Skizze selbst erfährt nur „Kurve ist projiziert“ (für Farbe und Sperre).
- Ebenen-Feature liefert keinen Körper, nur einen `Frame`; Neuberechnung und Abhängigkeiten wie die übrigen Features.
- Tests zuerst; Kernel-Schnitte mit dichten Lagen (Winkel und Abstände in feinen Schritten), nicht nur Stichproben.
- Tests bei 1920×1080, nie 800×600. Keine neuen Fremd-Abhängigkeiten.

## Annahmen zu Fusion (kommen in `offene-punkte.md`)

- Körper projizieren liefert alle Kanten ohne Doppelte, nicht nur den Umriss.
- Silhouette ist ein eigener Eintrag.
- Projizierte Geometrie lässt sich auf Konstruktion umschalten.
- Verlorener Bezug: Geometrie bleibt, Warnung am Feature.
