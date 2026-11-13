# Skizzen 9: DXF und SVG einfügen, Bild als Vorlage (Leinwand)

Stand 2026-11-12. Neuntes von zehn Skizzen-Teilprojekten. Maßstab ist Fusions Verhalten. Dennis sagt dazu: Die Dateien kommen „von allen möglichen Quellen“. Mein Bild vom Umfang hat er mit „passt“ bestätigt: DXF und SVG werden zu echten Skizzenkurven, ein Bild liegt als Vorlage auf einer Ebene, und fremde Dateien dürfen nichts kaputt machen. Alles Weitere hat Claude nach Fusion festgelegt. Was nur aus dem Gedächtnis oder aus Foren stammt, ist als **Annahme** markiert und kommt in `offene-punkte.md`.

## Warum

Für den 3D-Druck holt man oft fertige Umrisse von außen. Das sind Teile aus einem anderen CAD (DXF), Logos und Schriftzüge aus Inkscape oder Illustrator (SVG) oder ein Foto, das man nachzeichnen will. In Fusion fügt man das ein, bemaßt und extrudiert es. In PlanoraCAD fehlt das bisher ganz. Skizzen 7 und 8 haben die Kurven gebracht, die solche Umrisse genau darstellen: Kegelschnitt (Rho 0,5) für quadratische Bézierkurven und Kontrollpunkt-Spline Grad 3 für kubische.

**Gelungen ist es, wenn** eine DXF aus einem anderen Programm in richtiger Größe als bemaßbare Skizze ankommt, ein SVG-Logo sich vor dem OK verschieben, drehen, skalieren und spiegeln lässt und dann als Profil extrudierbar ist, und ein Foto nach dem Kalibrieren maßstäblich hinter der Skizze liegt. Kaputte, fremde oder riesige Dateien führen zu einer klaren Meldung, nie zu einem Absturz oder Hänger.

## Was Fusion tut

Quellen: Autodesk-Hilfe „Insert a DXF file“, „Add an SVG image“, „Insert canvas (image)“, „Calibrate canvas (image)“. Forenwissen und Erinnerung sind gekennzeichnet.

- **Menü:** Im Design-Bereich gibt es das Menü **Einfügen** (en „Insert“) mit „DXF einfügen“, „SVG einfügen“ und „Leinwand“ (en „Insert DXF“, „Insert SVG“, „Canvas“). Daneben gibt es noch Netz, McMaster und Ähnliches, das hier nicht dazugehört.
- **DXF einfügen:** Zuerst wählt man eine ebene Fläche oder Ebene, dann die Datei. Der DXF-Ursprung liegt auf dem Skizzenursprung. Die Felder sind:
  - **Einheiten:** skalieren die Datei. Hat die DXF keine Einheit, gelten laut Autodesk-Support die Einheiten des Entwurfs.
  - **Einfügemodus:** „Eine Skizze“ oder „Eine Skizze pro Layer“.
  - **X-Abstand, Y-Abstand:** Y ist positiv nach oben.
  - **DXF-Layer:** eine Liste, in der alle Layer angehakt sind.
  - **Kontrollpunktrahmen:** ein Häkchen für Splines.
  - Mit OK entsteht eine neue Skizze.
- **SVG einfügen:** Man wählt Datei und Fläche. Die Skizze liegt mittig auf dem Skizzenursprung. Die Felder sind:
  - **X-Abstand und Y-Abstand:** Bei Fusion ist Y in diesem Dialog positiv nach *unten*, also umgekehrt wie bei DXF.
  - **Z-Winkel:** positiv im Uhrzeigersinn.
  - **Horizontal spiegeln:** an der Y-Achse.
  - **Vertikal spiegeln:** an der X-Achse.
  - **Skalierung:** als Griff oder als Wert.
  - **Kontrollpunktrahmen:** ein Häkchen.
  - **Griffe** gibt es auf der Fläche zum Verschieben, Drehen und Skalieren. Laut Forum wird die SVG-Geometrie wegen der Leistung **fest** eingefügt. Ebenfalls laut Forum ist die Größe oft falsch, weil die DPI unklar sind.
- **Leinwand (Canvas):** Man wählt ein Bild (PNG, JPG, JPEG, TIF, TIFF, die Endung muss zum Inhalt passen) und dann eine Fläche. Die Felder sind:
  - **Durch Objekte anzeigen** (en „Display Through“)
  - **Auswählbar** (en „Selectable“)
  - **Renderfähig** (en „Renderable“)
  - **X-Abstand, Y-Abstand, Z-Winkel**
  - **Skalierung X, Y und XY:** XY behält das Seitenverhältnis.
  - **Horizontal und vertikal spiegeln**
  - **Deckkraft** 0 bis 100. Laut Forum ist die Vorgabe 50 % mit „Durch Objekte anzeigen“ an.
- **Browser:** Im Baum gibt es den Ordner **Leinwände** (en „Canvases“). Im Rechtsklickmenü des Bildes stehen „Kalibrieren“ und (Annahme, aus dem Forum) „Leinwand bearbeiten“. Beim Kalibrieren klickt man zwei Punkte auf dem Bild an, gibt den Abstand ein und drückt Enter. Das Bild wird dann so skaliert.
- **Verlauf (Annahme):** Eine Leinwand auf einer Körperfläche steht als „Angehängte Leinwand“ im Verlauf und folgt der Fläche.

## Bestand (geprüft im Code)

- **Kurven:** `CurveKind` (`planora-sketch/src/model.rs:49`) kennt Linie, Kreis, Bogen, Ellipse, Ellipsenbogen, Spline (Stützpunkte), ControlSpline (Grad 3 oder 5, geklemmte Knoten), Conic und FixedSpline. FixedSpline ist ein unveränderlicher, auch rationaler NURBS mit `Fixed` an den Enden und kommt bisher nur aus der Projektion.
- **Abbildung wie beim Text:** In `text_refresh.rs:76` wird eine quadratische Bézierkurve zu `Conic{rho: QUAD_RHO}` und eine kubische zu `ControlSpline{degree 3, CUBIC_KNOTS}`. Die öffentlichen `add_conic` und `add_control_spline` lehnen Sonderfälle ab. Die nachsichtigen `add_*_as` sind `pub(crate)`. Einen öffentlichen Weg für „viele Kurven auf einmal“ gibt es nicht.
- **Rückgängig:** Innerhalb der Skizze vergleicht `observe_sketch` Bild für Bild. Eine Änderung in einem Befehl ist ein Schritt, beschriftet über `edit_label`. Mit `SketchSession::atomic` gilt alles oder nichts. Im Dokument macht `observe_document` aus einem neuen `Snapshot` einen Schritt.
- **Neue Skizze:** `SketchPlane {Standard, OnFace, OnPlane}`. In der App gibt es `add_sketch`, `new_sketch_on_face` und `new_sketch_on_plane`. Einen Wählmodus für die Ebene gibt es nicht; Fläche oder Ebene müssen vorher ausgewählt sein. `PlanePick` (`plane_dialog.rs:191`) wählt im Ebenen-Dialog.
- **Import heute:** „Netz importieren“ und „STEP importieren“ im Menü Datei. Der `ImportDialog` mit eigenem `FileChooser::open(dir, endungen)` lädt im Hintergrund mit Fortschritt und Abbruch. Der letzte Ordner steht in `ws.exchange_dir`. Ein Menü Einfügen gibt es nicht. Die Menüleiste ist Datei, Erzeugen, Zeichnung, Bearbeiten, Konstruieren, Ansicht, Hilfe.
- **Griffe:** `sketch_grips.rs` (Skizzen 4) bringt `Grip {Free, AxisX, AxisY, Rotate, Scale}`, `layout`, `hit`, `paint` und die reine Funktion `grip_motion`. Die Vorschau läuft über `planora_sketch::transform::Similarity`. Die Griffe arbeiten heute auf Kurven, die schon in der Skizze stehen.
- **Baum:** `browser.rs` mit `enum Folder {Bodies, Sketches, Construction, Drawings, Origin}`, `FolderState { open: [bool;5] }` und Augen über `folder_eye`. Die Sichtbarkeit (`visibility.rs`) steht im `Snapshot`, also in der Datei und im Rückgängig. `HiddenFolders {bodies, sketches, planes}`.
- **Datei:** `FORMAT_VERSION = 18`. `required_format` nimmt das kleinste Format, das der Inhalt braucht. Binärdaten gibt es nur als Base64 in der Versions-JSON (`netze`, `base64.rs`), begrenzt durch `MAX_MESH_BYTES`.
- **Zeichnen in 3D:** `planora-render` hat drei wgpu-Pipelines (Flächen, Linien, Raster) und **keine Texturen**. Skizzenfüllungen sind 2D-egui-Netze ohne Tiefentest.
- **Grenzen:** `planora-exchange/src/limits.rs` mit `MAX_FILE_BYTES` (8 GiB), `CHECK_EVERY` usw. Der Text hat eigene Grenzen (`MAX_TEXT_CURVES`).
- **Profile:** Der Schnitt aller Kurvenpaare kostet O(n²) mit Hüllquader-Vorfilter. Gemessen sind 200 Kurven unter 50 ms. Viele tausend Kurven sind ungemessen.

## Lizenzen (vorläufig, wird beim Einbau mit `cargo deny check` bestätigt)

- **`dxf` 0.6.2** (MIT, ixmilia/dxf-rs) liest DXF als Text und binär. Die Abhängigkeiten sind byteorder, chrono, encoding_rs, enum_primitive, image (nur bmp), itertools, num und uuid, alle MIT oder Apache. `enum_primitive` ist alt und zieht `num-traits` 0.1. Wenn `cargo deny` hier einen Hinweis bringt, entscheidet die Begründung oder ein eigener kleiner Leser (siehe „Dem Bauenden überlassen“).
- **`usvg` 0.50** (Apache-2.0 OR MIT, linebender/resvg) wird **ohne Standard-Features** benutzt, also ohne `text`, Systemschriften und Schreiber. Es bleiben roxmltree, svgtypes, simplecss, kurbo, tiny-skia-path, data-url, imagesize, siphasher und strict-num, alle MIT oder Apache. `svgz` (flate2) darf rein.
- **`image` 0.25** ist schon im Lockfile. Gebraucht werden die Features png, jpeg und tiff.
- Es kommt kein Autodesk-Material in den Code oder die Tests. Testdateien schreiben wir selbst.

## Ziele

### A. Leser (`planora-exchange`, neue Module `dxf.rs`, `svg.rs`, `image.rs`)

Die Leser sind reine Funktionen von Bytes zu einer neutralen 2D-Geometrie ohne Skizzenbezug:

`Import2d { layers: Vec<Layer2d>, units: Option<Unit>, skipped: Skipped }`.

Jedes `Layer2d` hat einen Namen und Elemente. Die Elemente sind: Punkt, Linie, Kreis, Bogen, Ellipse(nbogen), quadratische und kubische Bézierkurve, NURBS (Grad, Knoten, Gewichte), Stützpunkt-Spline und Text (Inhalt, Schrift, Höhe, Lage, Winkel, Ausrichtung). `Skipped` zählt je Art, was übersprungen wurde: unbekannte Elemente, 3D-Elemente, Bemaßungen, Schraffuren, Bilder, SVG-Text.

**DXF:**
- **Elemente:** LINE, CIRCLE, ARC, ELLIPSE (voll und Teilstück), LWPOLYLINE und POLYLINE (2D, mit Bulge zu Bögen, geschlossen), SPLINE (Kontrollpunkte mit Knoten und Gewichten; nur Stützpunkte, wenn keine Kontrollpunkte da sind), POINT, TEXT und MTEXT (Formatcodes entfernt).
- **Blöcke:** INSERT löst Blöcke mit Lage, Drehung und Maßstab rekursiv auf, auch Reihen. Bei ungleichem Maßstab werden aus Kreisen Ellipsen.
- **Ebene:** Elemente mit Extrusionsrichtung (0,0,−1) werden gespiegelt übernommen. Alles, was nicht in der XY-Ebene liegt, wird übersprungen und gezählt.
- **Einheit** kommt aus `$INSUNITS`. Ohne Einheit gilt die Einheit des Entwurfs (wie Fusion).
- **Layer** kommen aus der Datei. Ausgeschaltete oder eingefrorene Layer sind in der Liste trotzdem angehakt. **Annahme:** Fusion zeigt alle Layer angehakt.
- **Übersprungen** werden DIMENSION, HATCH, SOLID, 3DFACE, IMAGE, VIEWPORT und Elemente im Papierbereich.

**SVG:**
- `usvg` vereinfacht alles zu Pfaden in absoluten Koordinaten. Rechteck, Kreis und Ellipse kommen dabei als Bézierkurven an. Gefüllte und nur umrandete Pfade zählen beide.
- **Größe:** px mit 96 DPI, also 1 px = 25,4/96 mm. `width`/`height` mit mm, cm oder in und `viewBox` rechnet usvg ein.
- **Achse:** Y im SVG zeigt nach unten und wird beim Einlesen umgedreht, damit das Bild in der Skizze aufrecht steht.
- **Übersprungen** werden `<text>` (Hinweis „Text vorher in Pfade umwandeln“), Bilder, Masken und Clip-Pfade. Clip-Pfade schneiden nicht. **Annahme:** Fusion verhält sich ähnlich.

**Bild:**
- Formate sind PNG, JPG/JPEG und TIF/TIFF. Erkannt wird am Inhalt. Passt die Endung nicht dazu, gibt es eine Meldung wie bei Fusion.
- Gelesen werden die Pixelgröße und die rohen Bytes. Das Dekodieren für die Anzeige geschieht in der App.

**Robustheit:**
- Jeder Leser ist begrenzt: höchstens `MAX_IMPORT_ELEMENTS` Elemente (Vorschlag 100 000), Blocktiefe 16, höchstens `MAX_IMAGE_BYTES` (Vorschlag 64 MiB) und höchstens 16 384 px Kantenlänge.
- Zahlen, die nicht endlich sind, und entartete Elemente (Länge 0, Radius 0) werden übersprungen und gezählt.
- Kaputte Dateien ergeben einen `ExchangeError` mit verständlichem Text, auch auf Englisch.
- Die Leser laufen im Hintergrund mit Fortschritt und Abbruch (wie `ImportDialog`).
- Fuzz-artige Tests mit abgeschnittenen und zufällig veränderten Dateien dürfen nicht abstürzen.

### B. In die Skizze (`planora-sketch`, neues Modul `import.rs`)

Eine öffentliche Funktion `Sketch::insert_geometry(&Import2d-Auswahl, Similarity, Optionen) -> Result<InsertReport>` legt alles in einem Zug an:
- **Linie, Kreis, Bogen, Ellipse und Ellipsenbogen** werden zu den gleichnamigen Kurven.
- **Quadratische Bézierkurven** werden zu `Conic` mit Rho 0,5, **kubische** zu `ControlSpline` Grad 3, wie beim Text.
- **NURBS:**
  - Nicht rational mit Grad 3 oder 5 wird zu `ControlSpline` mit den Knoten aus der Datei.
  - Grad 1 wird zu einer Linie je Spanne.
  - Grad 2 und Grad 4 werden exakt auf 3 bzw. 5 erhöht.
  - Grad über 5 wird in kubische Stücke zerlegt, mit einer Abweichung unter 1e-6 der Größe.
  - Rationale Kurven werden zu `FixedSpline`, exakt und fest.
- **Ein Stützpunkt-Spline** wird zu `Spline`.
- **Ein Punkt** wird zu einem Skizzenpunkt.
- **Text aus DXF** wird zu Skizzentext aus Skizzen 8 (Rahmen-Text, Schrift aus dem Stil). Fehlt die Schrift, wird wie gehabt mit Montserrat neu gesetzt, mit Hinweis.
- **Zusammenführen:** Endpunkte, die näher als 1e-6 × Größe beieinander liegen, werden ein gemeinsamer Punkt (zusammenhängende Konturen, damit Profile entstehen). **Annahme:** Fusion verbindet ebenfalls, ohne zusätzliche Koinzidenz-Abhängigkeiten.
- **DXF:** Die Geometrie bleibt **frei** und lässt sich bemaßen.
- **SVG:** Die Geometrie wird **fest** eingefügt (`Fixed` an allen Punkten), wie bei Fusion. Mit „Fixierung aufheben“ wird sie bearbeitbar.
- **Kontrollpunktrahmen** schaltet für die eingefügten Splines das Kontrollpolygon dauerhaft sichtbar. Ohne Häkchen gilt die normale Anzeige aus Skizzen 7. **Annahme**, weil die Wirkung in Fusion nur knapp beschrieben ist.
- **Lösen:** Danach wird einmal gelöst. Eingefügte Geometrie darf keine Abhängigkeit verletzen.
- **Rückgabe:** Im Bericht stehen Anzahlen und das Übersprungene.
- **Fehler:** Scheitert etwas, bleibt die Skizze unverändert (alles oder nichts).
- **Leistung:** 10 000 Kurven sollen in unter 2 s eingefügt und in der Skizze angezeigt werden. Die Profile werden dabei **gemessen**. Werden sie zu langsam, kommt ein Messtest und ein Eintrag in `offene-punkte.md` dazu, aber keine Sperre.

### C. Bedienung (`planora-app`)

- **Menü „Einfügen“** kommt in die Menüleiste nach „Erzeugen“, mit „DXF einfügen…“, „SVG einfügen…“ und „Leinwand…“. Dieselben Einträge stehen in der Skizzenleiste nicht; dort ist kein Platz, die 20 Knöpfe bleiben. Alle drei können Tastenkürzel bekommen (`action_table!`). Vorgabe ist keins, wie in Fusion.
- **Eingaben wählen:**
  - Die Fläche oder Ebene wird im Dialog über ein Wählfeld gewählt. Vorbild ist `PlanePick`. Erlaubt sind Ursprungsebenen, Konstruktionsebenen und ebene Körperflächen.
  - Ist eine Skizze offen, wird in diese Skizze eingefügt, das Wählfeld entfällt und „Eine Skizze pro Layer“ ist gesperrt. **Annahme**, so erinnert sich Claude an Fusion.
  - War vorher eine passende Fläche ausgewählt, ist sie schon eingetragen.
- **DXF-Dialog:** Datei, Ebene, Einheiten (mm, cm, m, in, ft; vorgewählt nach Datei bzw. Entwurf), Einfügemodus, X- und Y-Abstand, Layerliste mit Häkchen und Kontrollpunktrahmen. Eine Vorschau zeigt die Geometrie in der Ansicht. Bei OK entsteht eine neue Skizze, bei „pro Layer“ eine je angehaktem Layer mit dem Layernamen als Skizzenname. Alles ist ein Rückgängig-Schritt.
- **SVG-Dialog:**
  - Felder: Datei, Ebene, X- und Y-Abstand (Y positiv nach oben; *Nachtrag Handtest 2026-11-13:* Dennis hat festgelegt, dass ein positiver Y-Wert nach oben schiebt, wie die Skizzenachse), Z-Winkel (positiv im Uhrzeigersinn), Horizontal spiegeln, Vertikal spiegeln, Skalierung und Kontrollpunktrahmen.
  - Griffe in der Ansicht nutzen `sketch_grips` wieder: Freies Ziehen, X- und Y-Pfeil, Drehen und Skalieren auf der Vorschau. Feld und Griff bleiben dabei in beiden Richtungen gleich.
  - Ohne offene Skizze entsteht eine neue Skizze.
  - Die Vorschau sitzt am Anfang mit der Hüllquader-Mitte auf dem Ursprung.
- **Leinwand-Dialog:** Felder: Bild, Fläche, Deckkraft (Vorgabe 50), Durch Objekte anzeigen (Vorgabe an), Auswählbar (Vorgabe an), X- und Y-Abstand, Z-Winkel, Skalierung XY oder getrennt X/Y, Horizontal spiegeln, Vertikal spiegeln. Griffe wie bei SVG. Startgröße: 1 px = 1 mm. **Annahme**, so bleibt die Kalibrierung danach einfach. „Renderfähig“ entfällt, weil es keinen Render-Bereich gibt.
- **Kalibrieren:** Im Rechtsklickmenü der Leinwand im Baum. Man klickt zwei Punkte auf dem Bild an, gibt den Abstand ins Feld am Zeiger ein (mit Einheiten und Formeln wie überall) und bestätigt mit Enter. Die Leinwand wird um den ersten Punkt skaliert, das Seitenverhältnis bleibt. Esc bricht ohne Spur ab.
- **Baum:** Neuer Ordner „Leinwände“ mit Auge, nur sichtbar, wenn es Leinwände gibt. Je Leinwand gibt es ein Auge und das Rechtsklickmenü „Leinwand bearbeiten“, „Kalibrieren“, „Löschen“ und „Umbenennen“. Doppelklick öffnet „Bearbeiten“.
- **Laden und Fehler:**
  - Gelesen wird im Hintergrund mit Fortschrittsbalken und Abbrechen.
  - Danach gibt es einen Hinweis, was übersprungen wurde, zum Beispiel „12 Bemaßungen und 3 Schraffuren übersprungen“.
  - Kaputte Dateien bekommen eine Fehlermeldung im Dialog. Der Dialog bleibt offen.
- **Abbruch:** Esc, Abbrechen oder Werkzeugwechsel hinterlassen nichts.
- **Texte:** Alle Texte laufen über `tr!` in de und en, mit Namen wie in der deutschen und englischen Fusion.

### D. Leinwand im Modell (`planora-feature`)

- **Neues Element `Canvas`:**
  - Name und Bild als Verweis auf ein eingebettetes Bild.
  - Träger `SketchPlane`-artig: Ursprungsebene, Konstruktionsebene oder Fläche (`FaceRef`).
  - Lage: Versatz x/y, Winkel, Maßstab x/y und Spiegelung.
  - Einstellungen: Deckkraft, Durch Objekte anzeigen und Auswählbar.
- **Verlauf:** Die Leinwand steht als eigenes Feature im Verlauf, wie Konstruktionsebenen. Sie folgt ihrem Träger. Geht die Fläche verloren, gibt es eine Warnung am Feature (wie bei Projektionen) und die Leinwand bleibt an der letzten Lage.
- **Sichtbarkeit** über `Visibility` mit neuem Ordnerfeld `canvases` in `HiddenFolders`.
- **Bilddaten** liegen einmal je Dokument in einer Tabelle (Prüfsumme → Bytes). Zwei Leinwände mit demselben Bild teilen sich die Bytes. Rückgängig hält nur Verweise, die Bytes werden geteilt (`Arc<[u8]>`).
- **Andere Bereiche:** Zeichnungen, STEP-, STL- und 3MF-Export sowie Vorschaubilder kennen keine Leinwände. Leinwände sind nur Zeichenhilfe, wie Fusion es selbst sagt.

### E. Leinwand zeichnen (`planora-render`)

- **Neue Pipeline** für ein texturiertes Rechteck mit Alpha und Deckkraft. Mit „Durch Objekte anzeigen“ wird ohne Tiefentest nach den Körpern gezeichnet, sonst mit Tiefentest wie eine Fläche.
- **Reihenfolge:** Skizzenkurven liegen immer über der Leinwand.
- **Textur:** Bilder über 4096 px Kante werden für die GPU verkleinert. Die Datei behält das Original.
- **Auswahl:** Eine auswählbare Leinwand lässt sich in der Ansicht anklicken (Treffer im Rechteck, Pixel mit Alpha 0 zählen nicht; **Annahme**). Eine nicht auswählbare wird durchklickt.
- Das Bild wird einmal dekodiert und zwischengespeichert, nicht in jedem Bild neu.

### F. Speichern und Laden (`planora-project`)

- **Format 19**, nur wenn eine Leinwand im Dokument ist. Alte Dateien werden weiter bytegleich geschrieben. *Nachtrag Gesamtprüfung (I6):* Auch eine Kurve mit eingeschaltetem Kontrollpunktrahmen braucht Format 19, weil der Rahmen dauerhaft gespeichert wird (siehe offene-punkte.md, „Festlegungen mit Abweichungen“).
- **Bilder** stehen als Base64 in der Versions-JSON (Abschnitt `bilder`, wie `netze`) und in jeder Version erneut. **Bewusst einfach.** Wenn Projekte damit zu groß werden, kommt das in `offene-punkte.md`. Gesamtgrenze wie bei Netzen. *Nachtrag Gesamtprüfung (I5):* Für Bilder gilt eine eigene Gesamtgrenze von 1 GiB, schon beim Einfügen geprüft.
- **DXF und SVG** ergeben normale Skizzenkurven und brauchen kein neues Format (außer mit Kontrollpunktrahmen, siehe oben). FixedSpline außerhalb der Projektion ist in Format 17 schon speicherbar (`FesterSpline`); die Prüfung, dass die Kurve ohne Projektion geladen wird, gehört in die Tests.
- **Rundreise-Tests:** Speichern, Laden und Rückgängig mit Leinwand.

## Nicht-Ziele

- DWG lesen (geschlossenes Format) und DXF oder SVG schreiben (Export ist ein eigenes Thema).
- **3D-DXF:** Netze, 3DFACE und Körper (ACIS).
- **Bemaßungen und Schraffuren aus DXF** als Skizzenmaße bzw. Flächen. Sie werden übersprungen und gezählt.
- **SVG-Text** ohne Umwandlung in Pfade. Das bräuchte Schriften in usvg; Fusion kann es laut Forum auch nicht.
- **Kreise aus SVG** bleiben Bézierkurven, sie werden nicht als echte Kreise erkannt. Das entspricht Fusion.
- **„Renderfähig“** bei der Leinwand.
- **„Leinwand auf gekrümmte Fläche“:** In Fusion gibt es nur ebene Flächen.
- **Nachzeichnen per Automatik** (Bild zu Kurven).
- **Einfügen von Netzen, McMaster und Teilen** aus dem Fusion-Einfügen-Menü.

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es technisch läuft)

- Eine fremde Datei stürzt die App ab, hängt sie auf oder füllt den Speicher.
- Die Größe stimmt nicht, obwohl die Datei ihre Einheit nennt.
- Eine geschlossene Kontur aus der Datei ergibt kein Profil, weil Endpunkte nicht verbunden sind.
- Abbrechen hinterlässt etwas, oder Einfügen braucht mehr als einen Rückgängig-Schritt.
- Ein Bild bläht den Rückgängig-Speicher je Schritt um seine Größe auf.
- Alte Dateien ändern sich beim Speichern ohne neuen Inhalt.
- Autodesk-Material im Repo (Hilfe-Texte, Beispieldateien).

## Wie (Entscheidungen)

Alles nach Fusion von Claude festgelegt:
- Menü „Einfügen“ in der Menüleiste.
- DXF wird frei eingefügt, SVG fest.
- Bézierkurven werden zu Conic bzw. ControlSpline wie beim Text, rationale NURBS zu FixedSpline.
- Die Leinwand ist ein Feature im Verlauf und wird als Base64 in die Datei eingebettet.
- Format 19 nur mit Leinwand.
- Zu den Bibliotheken: `dxf` und `usvg` ohne Text, `image` war schon da.

## Nach der Prüfung festgelegt (Claude, Fusion als Maßstab)

- **DXF ohne `$INSUNITS`:** PlanoraCAD hat keine Einheit je Dokument, nur die Anzeigeeinheit in den Einstellungen (mm oder Zoll). Vorgewählt wird diese Anzeigeeinheit, das kommt Fusions „Einheit des Entwurfs“ am nächsten. Daneben steht der Hinweis „Die Datei nennt keine Einheit“, damit niemand die Vorwahl übersieht. Eine Einheit je Dokument einzuführen ist ein Nicht-Ziel. In der Einheitenliste kommt `ft` dazu (mm, cm, m, in, ft).
- **„Fixierung aufheben“:** Fusion hat dafür den Schalter „Fixieren/Lösen“ auf der Auswahl. Bei uns wird das Werkzeug „Fest“ zum Schalter: Sind alle gewählten Punkte und Kurven schon fest, entfernt es die `Fixed`-Abhängigkeiten in einem Rückgängig-Schritt, sonst fixiert es wie bisher. So lässt sich ein ganzes SVG-Logo mit Rahmenauswahl auf einmal lösen. Das gehört in diesen Plan, samt Text de/en und Test.
- **Leinwand im Verlauf:** Die Leinwand steht an der Verlaufsmarke, wie jedes neue Feature. Während eine Skizze offen ist, sind „Leinwand…“, „Leinwand bearbeiten“ und „Kalibrieren“ gesperrt (gedämpft, mit Tooltip „Erst die Skizze beenden“). DXF und SVG bleiben in der offenen Skizze erlaubt. Kalibrieren ist ein eigener Klickmodus außerhalb der Skizze und ein Rückgängig-Schritt im Dokument.
- **Zusammenführen beim Einfügen in eine offene Skizze:** Verbunden werden nur Endpunkte innerhalb der eingefügten Geometrie. Bestehende Skizzenpunkte werden nie angefasst.

## Dem Bauenden überlassen

- **Ob `dxf` oder ein eigener Leser:** Wenn `cargo deny` bei `dxf` Hinweise bringt, die sich nicht begründen lassen, ist ein eigener schlanker Gruppencode-Leser für Text-DXF erlaubt. Binär-DXF wäre dann ein Nicht-Ziel mit Meldung.
- Genaue Grenzwerte, die Toleranz beim Zusammenführen und der Aufbau des Fortschritts.
- Ob die Leinwand-Pipeline in `planora-render` oder als egui-Callback in der App sitzt. Sie braucht aber Tiefe.
- Die Aufteilung der Module und die Reihenfolge im Plan. Jede Aufgabe soll aber einzeln prüfbar sein.
- Testdateien: selbst geschriebene DXF/SVG/PNG unter `crates/planora-exchange/tests/data/`, klein, mit eigener Lizenz (GPL wie das Repo).
