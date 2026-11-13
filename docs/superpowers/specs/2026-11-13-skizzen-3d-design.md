# Skizzen 10: 3D-Skizze, 3D-Geometrie einbeziehen, auf Fläche projizieren, Schnittkurve, Verschieben aus der Ebene

Stand 2026-11-13. Letztes der zehn Skizzen-Teilprojekte. Maßstab ist Fusions Verhalten. Dennis hat dazu gesagt: „mach es wie in Fusion“, und als Bauart **B** gewählt: Eine dritte Koordinate bekommen nur Punkte, die die Skizzenebene verlassen. Alles Weitere hat Claude nach der Fusion-Hilfe festgelegt. Was dort nicht steht und aus dem Gedächtnis oder aus Foren stammt, ist als **Annahme** markiert und kommt in `offene-punkte.md`.

## Warum

In Fusion zeichnet man mit einer 3D-Skizze Kurven frei im Raum, etwa einen Rohrverlauf, einen Bügel oder einen Kabelweg. Mit „3D-Geometrie einbeziehen“, „Auf Fläche projizieren“ und „Schnittkurve“ legt man solche Kurven auf vorhandene Körper. Später dienen sie als Pfad für Sweep und Rohr oder als Führung beim Loft. Diese Werkzeuge gibt es in PlanoraCAD noch nicht. Bis dahin sind 3D-Kurven Bezug, Konstruktionshilfe und Quelle für „Projizieren“ in andere Skizzen.

**Gelungen ist es, wenn** man in einer Skizze den Haken „3D-Skizze“ setzt und mit der Triade Linien, Bögen und Splines frei im Raum zeichnet. Die zehn Fusion-Abhängigkeiten wirken auf diese Kurven. Kanten eines Körpers lassen sich als 3D-Kurven einbeziehen, Skizzenkurven lassen sich auf eine gewölbte Fläche legen und mit einer Fläche schneiden, und vorhandene Geometrie lässt sich aus der Ebene schieben. Alte Dateien bleiben bytegleich, und normale 2D-Skizzen werden nicht langsamer.

**Fehlschlag wäre:**
- 2D-Skizzen verhalten sich anders als vorher oder rechnen langsamer.
- Ein Profil entsteht aus einer Kurve, die nicht in der Skizzenebene liegt.
- Eine Projektion auf eine Fläche hängt oder stürzt bei Freiformflächen ab.

## Was Fusion tut

Quellen sind die Autodesk-Hilfeseiten „Create a sketch“ (SKT-CREATE-3D-SKETCH), „3D Sketch tools reference“, „3D Sketch constraints reference“, „Sketch Palette reference“, „Project and include geometry in sketches“, „Include 3D geometry“, „Project to surface“ samt Dialog-Referenz, „Intersection curve“ samt Dialog-Referenz sowie „Move/Copy reference“ und „Move or copy geometry“.

- **3D-Skizze einschalten:** In der Skizzenpalette gibt es unter „Optionen“ den Haken **3D-Skizze** (en „3D Sketch“: „Enables or disables the ability to sketch in 3D“).
- **Triade (3D Sketch Manipulator):** Sie erscheint, sobald ein Zeichenwerkzeug aktiv ist, zuerst am Ursprung (0,0,0). Sie hat drei Ebenen (XY, YZ, ZX) und Drehgriffe. Die aktive Ebene ist zu Beginn XY.
  - Ein Linksklick auf eine Ebene der Triade macht sie aktiv.
  - Ziehen an einem Drehgriff dreht die Zeichenebene.
  - **Alt** schaltet reihum durch die Ebenen, **Tab** reihum durch die Drehgriffe.
  - **Pfeil hoch** sperrt die XY-Lage des Zeigers auf der aktiven Ebene. Damit endet eine Linie oder ein Spline auf einer anderen Höhe (Z).
  - Rechtsklickmenü:
    - **Triadenausrichtung zurücksetzen** (en „Reset Triad Orientation“)
    - **Drehgriffe anzeigen** (en „Show Rotation Handles“)
    - **Ebenen anzeigen** (en „Show Planes“)
  - Nach jedem gesetzten Punkt wandert der Ursprung der Triade auf diesen Punkt.
- **Punkte setzen:**
  - Ein Klick auf die aktive Ebene setzt einen Punkt.
  - Fährt man entlang einer Achse, erscheint ihre Verlängerung. Ein Klick setzt den Punkt dann auf die Achse.
  - Alternativ tippt man **Drehung und Abstand** ein.
  - Um auf einer anderen Ebene weiterzuzeichnen, stellt man die Triade um.
- **Werkzeuge in 3D:** Linie, Bogen (3 Punkte), Spline, Rechteck (2 Punkte), Kreis (2 Punkte), Ellipse, Punkt, Text, Kegelschnitt. Die Hilfe-Symbole zeigen genau diese Arten.
- **Abhängigkeiten in 3D**, auch zu Körper- und Flächengeometrie: Horizontal/Vertikal, Deckungsgleich, Tangential, Gleich, Parallel, Senkrecht, Fixieren/Lösen, Mittelpunkt, Konzentrisch, Kollinear.
- **Projizieren/Einbeziehen** (Menü unter Erzeugen) umfasst Projizieren, Schneiden, 3D-Geometrie einbeziehen, Auf Fläche projizieren, Schnittkurve, Isoparametrische Kurve und Gedrehtes Profil.
  - **3D-Geometrie einbeziehen** (en „Include 3D Geometry“) nimmt Flächen, Kanten, Punkte, Arbeitsgeometrie und Skizzenkurven als 3D-Skizzengeometrie in die aktive Skizze.
  - **Auf Fläche projizieren** (en „Project To Surface“) legt Skizzengeometrie, Kanten, Ecken oder Arbeitspunkte auf gewählte Flächen und erzeugt dabei 3D-Skizzenkurven. Der Dialog hat die Felder:
    - **Flächen** (Fläche, Flächenkörper oder Arbeitsebene)
    - **Kurven**
    - **Projektionsart:** „Nächster Punkt“ (en „Closest Point“) oder „Entlang Vektor“ (en „Along Vector“, mit gewähltem Vektor)

    Die Hilfe rät, dafür eine neue Skizze zu beginnen.
  - **Schnittkurve** (en „Intersection Curve“) erzeugt eine 3D-Skizzenkurve aus dem Schnitt einer Skizzenkurve mit Flächen, Arbeitsebenen oder anderen Skizzenkurven. Der Dialog hat die Felder „Kurve“ und „Kurven/Flächen“. Die gewählte Kurve und das Ergebnis dürfen nicht in derselben Skizze liegen.
- **Verschieben/Kopieren:** „Freies Verschieben“ hat einen Manipulator. Damit verschiebt man entlang X/Y/Z und dreht um X/Y/Z. Die Felder sind X/Y/Z-Abstand und X/Y/Z-Winkel, dazu „Kopie erstellen“ und „Drehpunkt festlegen“.

**Annahmen**, nicht in der Hilfe belegt:
- **A1:** Ein Verschieben in Z oder ein Drehen um X/Y macht Skizzengeometrie zu 3D-Geometrie. In einer Skizze ohne Haken „3D-Skizze“ sind diese Felder und Griffe gesperrt.
- **A2:** Bemaßen geht in 3D:
  - ausgerichteter Abstand zwischen zwei Punkten
  - Länge einer Linie
  - Radius und Durchmesser an Kreis und Bogen
  - Winkel zwischen zwei Linien

  Waagerechte und senkrechte Maße gibt es an 3D-Geometrie nicht.
- **A3:** Horizontal/Vertikal macht eine 3D-Linie parallel zur X-, Y- oder Z-Achse der Skizze, und zwar zu der Achse, die der Linie beim Anlegen am nächsten liegt.
- **A4:** Eingebezogene Geometrie und „Auf Fläche projizieren“ sind verknüpft wie „Projizieren“ (lila, folgen dem Körper). Geht der Bezug verloren, gibt es eine Warnung.
- **A5:** Was in der Hilfe-Liste der 3D-Werkzeuge fehlt, lässt sich auf 3D-Geometrie nicht anwenden: Stutzen, Dehnen, Brechen, Rundung, Fase, Versatz, Muster, Spiegeln und Skalieren. Die Werkzeuge bleiben für 2D-Geometrie nutzbar; 3D-Kurven lassen sich mit ihnen nicht wählen, und ein Hinweis am Zeiger nennt den Grund.
- **A6:** Der Haken „3D-Skizze“ gehört zur Skizze und wird mit ihr gespeichert.
- **A7:** Fang in 3D gibt es auf Skizzenpunkte, Kurvenmitten und Körperecken. Wer an einer Körperecke fängt, bekommt dort einen eingebezogenen, verknüpften Punkt.

Nicht im Umfang: Isoparametrische Kurve und Gedrehtes Profil. Sie stehen in der [[Gegenüberstellung Fusion]] unter „Skizzen 11: Reste“.

## Bestand (geprüft im Code)

- **Punkte und Löser:**
  - `PointData { x, y: usize, construction, alive }` (`planora-sketch/src/model.rs:15`) hält Indizes in den flachen Vektor `Sketch.vars`.
  - `pos()` und `point_xy()` liefern `DVec2` (model.rs:343, :360).
  - Ausgewertet wird in `Ctx` (`constraint.rs:326`) mit Dual-Zahlen. `pt()` liefert `V2` (:381), darauf bauen die 2D-Helfer `sub2`, `cross`, `dot`, `length` (:386–410), `line_dir` und `circle_of` auf.
  - Die Gruppen entstehen rein über Variablenindizes per Union-Find (`solver.rs:903`), eine z-Variable fiele also von selbst richtig hinein.
  - Der Ziehwunsch heißt `Drag { point, target: DVec2 }` (solver.rs:218).
- **Kurven:** `CurveKind` (model.rs:49).
  - Eine Ellipse ist `center` plus große Halbachse als Variablenpaar (ax, ay) plus `minor`.
  - `FixedSpline` trägt einen `NurbsCurve` mit `DVec3`-Kontrollpunkten, heute immer mit z = 0.
  - Der Kegelschnitt trägt `rho` als Zahl in der Kurve. Das ist das Vorbild für feste Zahlen, die keine Löservariablen sind.
- **Zeichnen:**
  - Die Skizze wird beim Bearbeiten und danach als egui-Formen gezeichnet, jeder Punkt durch die Kamera projiziert: `Mapper { camera, frame, rect }` mit `to_screen(DVec2)` und `to_sketch(Pos2)` (`planora-app/src/sketch_view.rs:522`).
  - Bildschirm zu Skizze: `screen_to_sketch` (:80) schneidet den Blickstrahl mit der Skizzenebene.
  - Fertige Skizzen zeichnet `finished_sketch_shapes` (:753).
  - Die wgpu-Linienpipeline zeichnet keine Skizzen.
- **Griffe:** `sketch_grips.rs` hat `Grip { Free, AxisX, AxisY, Rotate, Scale }`. Die Griffe sind rein 2D in der Skizzenebene und in Bildschirmpunkten gezeichnet. Eine 3D-Triade gibt es nirgends.
- **Projektion (Skizzen 6):**
  - `Tool::ProjectEdge` mit den Arten Projizieren, Schneiden und Silhouette (`sketch_kinds.rs:148`).
  - In der App: `project_into_sketch` und `place_projection` (`state.rs:3450`, :3689), `SketchSession::project_pieces` (`sketch_project.rs:66`).
  - Im Modell: `CopySource { Edge, Section, Silhouette }` und `EdgeCopy` (`planora-feature/src/model.rs:84`, :106).
  - Neuberechnung über `compute_sketch` und `follow_copy` (`compute.rs:832`, :948).
  - Kernel-Kurven werden über `helpers.rs::project_curve` (:1016) mit `frame.to_local(p).truncate()` flach.
- **Kernel:**
  - `Surface::project(p) -> (u, v)` (`geometry.rs:378`, Freiform `nurbs_surface.rs:778`) und `Curve::project`
  - `intersect_surfaces` (`boolean/surfsurf.rs:64`)
  - `curve_surface_roots` (`boolean/roots.rs:63`)
  - `face_geom` und `point_in_face` (`boolean/uv.rs:29`, :298) für „liegt im getrimmten Bereich der Fläche“
- **Skizzenpalette:** `SketchPalette` (`sketch_panel.rs:909`). Die Zeile „Raster, Fang, Blick auf“ steht in `side_contents` (:1314). Projizieren/Einbeziehen ist ein Werkzeug mit Klappliste (`sketch-tool-project-include`, :74), seine Optionen stehen in `project_options` (:1161).
- **Profile:** `Sketch::profiles()` (`profile.rs:185`) filtert bei :189 nur Konstruktionskurven aus.
- **Datei:**
  - `FORMAT_VERSION = 19`, und `required_format` ist eine Kaskade `needs_format_19`, … (`planora-project/src/convert.rs:51`, :83).
  - Punkte werden als `PointDto { x, y, konstruktion, aktiv }` geschrieben (convert.rs:1045, `format.rs:776`).
- **Dialoge:** `Dialog::new(title, DialogSize)` (`planora-app/src/dialog.rs:65`). Die Session `PlanoraCAD-fenster` baut den Rahmen gerade für abdockbare Fenster um, also nur diesen Weg benutzen.
- **Löseraufwand:** Lange Bogenketten sind schon heute langsam (Skizzen 9, `import_perf.rs`).

## Ansätze

- **A – jeder Punkt hat immer z.** Das ist einheitlich, aber alle 2D-Abhängigkeiten, Profile, Stutzen und Muster müssten z mitführen. Jede 2D-Skizze hätte 50 % mehr Variablen, und alle Dateien würden sich ändern. Verworfen, weil es langsamer ist und alte Dateien nicht mehr bytegleich bleiben.
- **B – z nur bei Punkten außerhalb der Ebene (gewählt von Dennis).** Normale Punkte bleiben unverändert. 3D-Punkte haben eine z-Variable, ebene 3D-Kurven eine eigene Ebene, und für 3D gibt es eine eigene Fassung der zehn Fusion-Abhängigkeiten. Der 2D-Code bleibt unberührt.
- **C – 3D-Kurven als feste Kurven ohne Löser.** Das wäre billig, ist aber nicht wie Fusion, denn dort kann man 3D-Linien verknüpfen und bemaßen. Verworfen.

## Entwurf

### 1. Datenmodell (`planora-sketch`)

- **3D-Punkt:** `PointData` bekommt `z: Option<usize>`.
  - `None` heißt: Der Punkt liegt in der Skizzenebene (z = 0). Das gilt wie bisher für alle 2D-Punkte.
  - `Some(i)` heißt: Der Punkt ist ein 3D-Punkt mit der Variablen `vars[i]`. Ein 3D-Punkt bleibt 3D, auch wenn sein z zufällig 0 wird.
  - Neue Zugriffe: `pos3()` und `point_xyz() -> DVec3` (bei `None` mit z = 0), `add_point3(x, y, z)`, `is_3d(p)`.
  - `pos()` und `point_xy()` bleiben für 2D-Code. Bei 3D-Punkten liefern sie die Lage in der Draufsicht. Wer 3D-Geometrie zeichnen oder auswerten muss, benutzt die 3D-Zugriffe.
- **Wann ein Punkt 3D wird:** Ein neu gezeichneter Punkt wird nur dann 3D, wenn er bei eingeschalteter 3D-Skizze außerhalb der Skizzenebene liegt (|z| > Toleranz) oder zu einer ebenen Kurve gehört, deren Ebene nicht die Skizzenebene ist. Wer bei eingeschalteter 3D-Skizze auf der Skizzenebene zeichnet, bekommt also ganz normale 2D-Geometrie. Die bleibt profilfähig. *(Claudes Festlegung, damit eine 3D-Skizze ihre 2D-Profile behält.)*
- **Ebene einer Kurve:** Kreis, Bogen, Ellipse, Ellipsenbogen und Text liegen in einer Ebene. Liegt diese Ebene nicht in der Skizzenebene, bekommt die Kurve eine **feste Normale** `normal: Option<DVec3>` in `CurveData`. Das ist eine Zahl in der Kurve wie `rho` beim Kegelschnitt, keine Löservariable. `None` heißt Skizzenebene, also alles wie heute.
  - Eine interne Abhängigkeit `InPlane(curve)` hält die Punkte der Kurve in der Ebene durch den Mittelpunkt mit dieser Normale. Für den Bogen sind das Start und Ende, für den Kreis und die Ellipse der Mittelpunkt allein. Kreis- und Ellipsenumfang liegen über die Normale ohnehin in der Ebene.
  - Die große Halbachse der Ellipse bekommt eine dritte Variable `az` und wird intern senkrecht zur Normale gehalten.
  - Drehen um eine Achse in der Ebene (Verschieben aus der Ebene, Triade gedreht) ändert die Normale direkt. Der Löser dreht die Ebene nie. *(Claudes Festlegung: so bleiben Ebenen stabil, und Fusion verhält sich laut Erfahrung genauso; Annahme A8.)*
  - Der Kegelschnitt braucht keine Normale, denn Start, Scheitel und Ende spannen seine Ebene auf.
  - Linie, Spline (Stützpunkte), Spline mit Kontrollpunkten und Punkt sind frei im Raum.
- **FixedSpline in 3D:** Die Kontrollpunkte dürfen z ≠ 0 haben, wenn die Endpunkte 3D-Punkte sind. Das ist das Ergebnis von Einbeziehen, Projizieren auf Fläche und Schnittkurve.
- **Skizze:** `Sketch` bekommt `three_d: bool` für den Haken „3D-Skizze“ (Annahme A6). Damit lassen sich auch 3D-Werkzeuge wieder abschalten. Bereits gezeichnete 3D-Geometrie bleibt, wenn der Haken fällt.
- **Grenzen:** Für z gilt dasselbe `MAX_COORD` wie für x und y. Die Normale muss endlich und normierbar sein, sonst gibt es `InvalidInput`.

### 2. Löser und Abhängigkeiten in 3D

- **Bestand:** Abhängigkeiten, an denen nur 2D-Punkte und Kurven ohne eigene Normale beteiligt sind, rechnen **genau wie heute**. Der Code bleibt derselbe, und auch die Laufzeit ändert sich nicht.
- **3D-Fassung:** Ist ein 3D-Punkt oder eine Kurve mit Normale beteiligt, rechnet die 3D-Fassung. `Ctx` bekommt dafür `pt3() -> V3` (bei z = `None` ist z eine Konstante 0 ohne Gradient) und 3D-Helfer (`sub3`, `cross3`, `dot3`, `len3`).
  - **Deckungsgleich:** alle drei Koordinaten gleich.
  - **Horizontal/Vertikal:** parallel zur nächstgelegenen Skizzenachse X, Y oder Z (A3). Die gewählte Achse steckt in der Art der Abhängigkeit (`Horizontal`, `Vertical`, neu `AlongZ`, siehe Abschnitt 10), damit sie beim Laden nicht wechselt.
  - **Parallel, Senkrecht, Kollinear, Gleich:** jeweils über die Richtungen bzw. Längen. Gleich gilt bei Linien für die Länge, bei Kreis und Bogen für den Radius.
  - **Tangential:** Linie–Bogen/Kreis und Bogen–Bogen am gemeinsamen Punkt. Die Richtung ist senkrecht zum Radius und liegt in der Bogenebene. Beim Spline gilt die Richtung am Ende.
  - **Mittelpunkt:** Punkt = Mitte der Linie (3D) bzw. Bogenmitte.
  - **Konzentrisch:** gleiche Mittelpunkte in 3D.
  - **Fixieren/Lösen:** auch z ist fest.
  - **Punkt auf Kurve** entsteht bei Deckungsgleich mit einer Kurve und rechnet in 3D über den nächsten Punkt bzw. über die Ebene plus Radius.
- **Gesperrte Abhängigkeiten:** Alle anderen Abhängigkeiten (Symmetrisch, Glatt, Muster usw.) lassen sich nicht auf 3D-Geometrie setzen. Der Knopf bleibt gedämpft, ein Hinweis am Zeiger nennt den Grund. Laden lehnt sie mit einer klaren Meldung ab.
- **Maße (A2):** Abstand (ausgerichtet, 3D), Länge, Radius, Durchmesser und Winkel zwischen Linien bekommen eine 3D-Fassung. Waagerecht/senkrecht bemaßen ist an 3D-Geometrie gesperrt.
- **Ziehen:** `Drag` bekommt ein Ziel in 3D. Gezogen wird ein 3D-Punkt in der aktiven Triadenebene.
- **Messtest:** Eine reine 2D-Skizze aus `import_perf.rs` muss genauso schnell lösen wie auf `master` (Abweichung höchstens 5 %). Dazu kommt ein neuer Messtest mit 200 verketteten 3D-Linien und -Bögen samt Abhängigkeiten, gemessen und in `offene-punkte.md` notiert.

### 3. Profile, Projizieren, Muster

- **Profile:** `profiles()` lässt alle Kurven mit einem 3D-Punkt oder mit Normale weg. 3D-Geometrie, die in der Ebene liegt, ergibt kein Profil. *(Claudes Festlegung, eindeutig und billig.)*
- **Projizieren in andere Skizzen:** Die vorhandenen Werkzeuge „Projizieren“ und „Schneiden“ (Skizzen 6) nehmen fertige 3D-Skizzenkurven als Quelle, damit die Kurven sofort zu etwas taugen.
  - Projizieren: in die Zielebene werfen.
  - Schneiden: Schnittpunkte mit der Ebene.
  - Die Herkunft kommt als neue `CopySource::SketchCurve { sketch, curve }` dazu.
- **Ändern-Werkzeuge:** Stutzen, Dehnen, Brechen, Rundung, Fase, Versatz, Muster, Spiegeln und Skalieren ignorieren 3D-Kurven bei der Auswahl (A5), ein Hinweis nennt den Grund.

### 4. Triade und Zeichnen in 3D (`planora-app`)

- **Haken:** „3D-Skizze“ steht in der Skizzenpalette in der Zeile mit Raster, Fang und Blick auf. Das Umschalten ist ein Rückgängig-Schritt.
- **3D-Abbildung:** `Mapper` bekommt `to_screen3(DVec3)` (Skizzenkoordinaten in 3D). `screen_to_plane(pixel, plane)` schneidet den Blickstrahl mit einer beliebigen Ebene in Skizzenkoordinaten. Fertige Skizzen und die aktive Skizze zeichnen 3D-Kurven mit z.
- **Triade:** Sie erscheint, wenn der Haken gesetzt ist und eines der neun 3D-Werkzeuge aktiv ist (Linie, Bogen nur „3 Punkte“, Spline beider Arten, Rechteck „2 Punkte“, Kreis „2 Punkte“, Ellipse, Punkt, Text, Kegelschnitt). Die anderen Arten dieser Werkzeuge zeichnen nur 2D. Ihre Einträge sind bei aktiver Triade gedämpft. *(Wie die Hilfe-Liste.)*
  - **Aussehen:** Sie wird als egui-Formen gezeichnet: drei halbtransparente Ebenenquadrate in den Achsfarben (rot/grün/blau wie die Ursprungsachsen), drei Achsen und drei Drehbögen. Die aktive Ebene ist kräftiger. Die Größe ist in Bildschirmpunkten fest.
  - **Lage:** Sie beginnt am Skizzenursprung mit den Achsen der Skizze und der aktiven Ebene XY (= Skizzenebene). Nach jedem gesetzten Punkt springt sie auf diesen Punkt.
  - **Bedienung:**
    - Ein Klick auf ein Ebenenquadrat macht diese Ebene aktiv.
    - Ziehen an einem Drehbogen dreht die Triade um die zugehörige Achse. Beim Ziehen gilt der Winkelfang, und ein Feld zeigt den Winkel.
    - **Alt** (allein gedrückt und losgelassen) schaltet die aktive Ebene reihum.
    - **Tab** macht reihum einen Drehgriff aktiv. Dessen Winkel lässt sich dann eintippen.
    - **Pfeil hoch** (gehalten) sperrt die Lage in der aktiven Ebene. Der Zeiger bewegt dann nur noch entlang der Ebenennormale, die Höhe zeigt ein Feld.
    - Rechtsklickmenü: „Triadenausrichtung zurücksetzen“, „Drehgriffe anzeigen“, „Ebenen anzeigen“ (Haken, gespeichert in den Einstellungen).
  - **Tasten:** Alt, Tab und Pfeil hoch sind wie alle Tasten in den Einstellungen belegbar (neue Aktionen in `keymap/`, Vorgabe wie Fusion).
- **Punkte setzen:**
  - Ein Klick setzt den Punkt auf die aktive Ebene (Blickstrahl ∩ Ebene).
  - Fährt der Zeiger nahe an einer Triadenachse entlang, erscheint die gestrichelte Achsverlängerung, und der Punkt rastet auf der Achse ein. Gilt das noch beim Klick, entsteht eine Horizontal/Vertikal-Abhängigkeit (A3) zum vorigen Punkt, wie heute beim 2D-Fang.
  - **Felder am Zeiger:** Linie und Spline zeigen „Länge“ und „Winkel“ in der aktiven Ebene. Das ist Fusions „Drehung und Abstand“, und es entstehen dieselben Maße wie in 2D, in 3D-Fassung.
  - Steht die Blickrichtung fast parallel zur aktiven Ebene (Winkel unter 5°), setzt ein Klick nichts, und ein Hinweis bittet, die Ansicht zu drehen oder die Ebene zu wechseln. *(Claudes Festlegung gegen Punkte im Unendlichen.)*
- **Ebene Kurven:**
  - **Kreis, Bogen, Ellipse, Rechteck, Text:** Sie entstehen in der aktiven Triadenebene. Ist die aktive Ebene die Skizzenebene (Triade unverdreht, XY, Ursprung z = 0), entsteht normale 2D-Geometrie.
  - **Text in 3D:** Rahmen und Schrift liegen in der Triadenebene. Der Text-Abschnitt im Skizzenfeld bleibt derselbe. „Text auf Pfad“ gibt es nur in 2D.
- **Fang (A7):** Skizzenpunkte und Mitten aller Kurven (in 3D über den Bildschirmabstand), Ursprung, Körperecken. Fang an einer Körperecke legt einen eingebezogenen Punkt an (wie Abschnitt 5) und verbindet ihn mit „Deckungsgleich“.
- **Auswahl:** Ein Klick wählt 3D-Kurven über den Bildschirmabstand zur projizierten Kurve. Die Rahmenauswahl wirkt auf die projizierte Lage.
- **Rückgängig:** Ein Befehl ist ein Schritt, Esc hinterlässt nichts. Die Triadenlage gehört nicht zum Dokument.

### 5. 3D-Geometrie einbeziehen

- **Ort:** ein neuer Eintrag in der Klappliste „Projizieren/Einbeziehen“, Werkzeug-Art `Include3d` neben Projizieren/Schneiden/Silhouette.
- **Auswahl:** Kanten, Flächen (alle Randkanten der Fläche), Körperecken, Konstruktionsebenen (vier Randlinien des angezeigten Rechtecks, Annahme), Konstruktionsachsen, Konstruktionspunkte, Ursprung und Kurven fertiger Skizzen.
- **Ergebnis:** 3D-Geometrie in Raumlage, verknüpft und lila wie bei „Projizieren“ (A4):
  - Linien werden Linien mit 3D-Punkten.
  - Kreise, Bögen und Ellipsen bekommen ihre Normale.
  - Alles andere wird FixedSpline in 3D.

  Die Enden bekommen `Fixed`. Neue Herkunftsart: `CopySource::Include { … }` mit Kante, Fläche + Randindex, Ecke, Konstruktionsobjekt oder Skizzenkurve. Die Neuberechnung (`follow_copy`) setzt die 3D-Lage neu. Geht der Bezug verloren, gibt es eine Warnung, und die Kurve bleibt stehen (Repo-Regel).
- **Zustand des Hakens:** Einbeziehen geht auch ohne den Haken „3D-Skizze“, denn Fusion setzt ihn dafür nicht voraus (Annahme). Die Geometrie ist trotzdem 3D.

### 6. Auf Fläche projizieren

- **Dialog** „Auf Fläche projizieren“ über den `Dialog`-Rahmen mit den Feldern:
  - **Flächen:** eine oder mehrere Körperflächen oder Konstruktionsebenen
  - **Kurven:** Kurven fertiger Skizzen, Körperkanten, Körperecken, Konstruktionspunkte
  - **Projektionsart:** „Nächster Punkt“ oder „Entlang Vektor“ mit dem Wählfeld **Richtung** (Linie, Kante, Konstruktionsachse oder Ebene als Normale)

  OK oder Enter übernimmt, Esc bricht ab.
- **Rechnung** (neues Modul `planora-feature/src/surface_project.rs`, Kernel-Helfer in `planora-kernel`):
  1. Jede Quellkurve wird adaptiv abgetastet (Sehnenfehler ≤ Toleranz, höchstens 2000 Punkte je Kurve).
  2. „Nächster Punkt“: Jeder Abtastpunkt wird mit `Surface::project` auf jede gewählte Fläche geworfen. Es zählt der nächste Treffer, der laut `point_in_face` im getrimmten Bereich liegt.
  3. „Entlang Vektor“: Strahl in beide Richtungen, Schnitt über `curve_surface_roots` mit einer Linie. Es zählt der nächste Treffer im getrimmten Bereich.
  4. Wo die Treffer abreißen (aus der Fläche gelaufen) oder springen, wird die Kurve geteilt.
  5. Jedes Stück wird als FixedSpline (Grad 3) durch die Treffer angenähert. Die Abweichung wird an Zwischenpunkten geprüft, nötigenfalls wird verfeinert.
  6. Sonderfälle exakt: Ebene als Ziel + „Entlang Vektor“ senkrecht zur Ebene, und Linie auf Ebene, ergeben Linie bzw. Bogen.
- **Grenzen:** Rechenzeit höchstens 2 s. Wer darüber liegt, bricht mit Meldung ab und hinterlässt nichts. Kein Treffer ergibt eine Meldung „Die Kurven treffen die Flächen nicht“.
- **Ergebnis:** in die aktive Skizze, als 3D-Geometrie, verknüpft (A4) mit `CopySource::OnSurface { quelle, flächen, art, richtung }`. Die Neuberechnung rechnet das Ganze neu. Ein Dialog-OK ist ein Rückgängig-Schritt.
- **Quelle in der aktiven Skizze:** Eine Kurve der aktiven Skizze ist als Quelle nicht wählbar, wie Fusions Rat, eine neue Skizze zu beginnen. *(Claudes Festlegung, verhindert Kreisbezüge.)*

### 7. Schnittkurve

- **Dialog** „Schnittkurve“ mit den Feldern:
  - **Kurve:** Kurven fertiger Skizzen, nicht aus der aktiven Skizze (wie Fusion)
  - **Kurven/Flächen:** Körperflächen, Konstruktionsebenen oder Kurven anderer fertiger Skizzen
- **Rechnung:** Die Quellkurve wird gedanklich entlang der Normale ihrer Skizzenebene unendlich ausgezogen.
  - Gegen Flächen ist das dasselbe wie „Entlang Vektor“ mit der Skizzennormale aus Abschnitt 6, also derselbe Code.
  - Gegen eine andere Skizzenkurve wird auch diese entlang ihrer Normale ausgezogen, und beide Auszüge werden geschnitten. Je Abtastpunkt der ersten Kurve sucht ein Newton-Verfahren den Punkt auf dem zweiten Auszug. Annäherung und Teilung wie in Abschnitt 6.
- **Ergebnis:** 3D-Geometrie, verknüpft mit `CopySource::Intersection { … }`, sonst wie Abschnitt 6.

### 8. Verschieben aus der Ebene

- **Werkzeug:** Das vorhandene „Verschieben/Kopieren“ (Skizzen 4) bekommt bei gesetztem Haken „3D-Skizze“:
  - zusätzlich die Felder **Z-Abstand**, **X-Winkel** und **Y-Winkel**
  - einen **Z-Pfeil** und zwei Drehbögen um X und Y am Griff (gezeichnet wie die Triade)
  - Ohne Haken bleiben sie verborgen (A1).
- **Wirkung:**
  - Bewegte Punkte werden 3D-Punkte.
  - Kreise, Bögen, Ellipsen und Text bekommen ihre Normale (gedreht).
  - Splines und Linien folgen ihren Punkten.
- **Abhängigkeiten:**
  - 2D-only-Abhängigkeiten an bewegter Geometrie (siehe Abschnitt 2) entfallen beim Bewegen. Eine Meldung nennt ihre Zahl. *(Erweitert die Skizzen-4-Annahme: nach außen bindende entfallen.)*
  - 2D-Maße mit einer 3D-Fassung werden in die 3D-Fassung umgesetzt, waagerechte und senkrechte Maße entfallen.
  - Wer das nicht will, macht es rückgängig (ein Schritt).
- **Kopie:** „Kopie erstellen“ nimmt die inneren Abhängigkeiten mit, soweit sie in 3D erlaubt sind.
- **Zurück in die Ebene:** Wird 3D-Geometrie wieder in die Ebene bewegt, bleibt sie 3D (Abschnitt 1).

### 9. Datei, Baum, Texte

- **Format 20:** Nur wenn eine Skizze einen 3D-Punkt, eine Kurve mit Normale, eine 3D-FixedSpline, eine der neuen `CopySource`-Arten oder `three_d = true` enthält (`needs_format_20`).
  - Neue Felder: `PointDto.z` und `CurveDto.normale`, beide weggelassen, wenn leer, dazu `SketchDto.drei_d` (nur wenn wahr).
  - Alte Dateien bleiben bytegleich. Eine Datei mit Format 20 lehnen ältere Programme wie gewohnt ab.
- **Baum:** Eine Skizze mit 3D-Geometrie bekommt kein eigenes Symbol. *(Fusion unterscheidet laut Hilfe nicht.)*
- **Texte:** alle über `tr!`, de und en, Namen wie in der deutschen und englischen Fusion. Die genauen deutschen Begriffe aus Fusions deutscher Oberfläche sind Annahmen: „3D-Skizze“, „3D-Geometrie einbeziehen“, „Auf Fläche projizieren“, „Schnittkurve“, „Nächster Punkt“, „Entlang Vektor“, „Triadenausrichtung zurücksetzen“, „Drehgriffe anzeigen“, „Ebenen anzeigen“.

### 10. Festlegungen nach der Prüfung (Claudes Festlegungen, nach Fusion-Logik)

- **2D-Punkt wird 3D:** Setzt man eine 3D-Abhängigkeit zwischen einem 2D-Punkt und 3D-Geometrie, wird der 2D-Punkt dabei zum 3D-Punkt (z-Variable mit Wert 0). Das gilt zum Beispiel für „Deckungsgleich“ mit einem hochliegenden Punkt. Danach darf der Löser ihn anheben. Der Löser selbst ändert die Art eines Punkts nie, das passiert nur beim Setzen einer Abhängigkeit, beim Einbeziehen und beim Verschieben aus der Ebene. Hängen an dem Punkt 2D-only-Abhängigkeiten, ist das Setzen gesperrt, und ein Hinweis nennt den Grund. Verliert ein Profil dadurch seine Ebene, verschwindet es, wie in Fusion. Eine 3D-Linie, die an einer Rechteckecke beginnt, benutzt die Ecke selbst als Startpunkt, und die bleibt 2D, solange nichts sie anhebt.
- **Abhängigkeiten zu Körpergeometrie:** Sie laufen nur über eingebezogene Geometrie, also feste, verknüpfte Kopien. Wählt man bei einer Abhängigkeit eine Körperkante oder -ecke, wird sie dafür automatisch einbezogen (wie A7). Eine echte „Punkt auf Fläche“-Abhängigkeit im Löser gibt es nicht. **Annahme A9**, kommt in `offene-punkte.md`.
- **Herkunft mit mehreren Stücken:**
  - Eine Quelle (Fläche mit Randkanten, Projektion auf Fläche, Schnittkurve) wird als **Gruppe** gespeichert: `EdgeCopy` bekommt einen Stückindex, die Kurven einer Quelle teilen sich dieselbe `source`.
  - Ergibt die Neuberechnung dieselbe Stückzahl, folgen die Kurven der Reihe nach.
  - Ergibt sie eine andere Zahl, bleiben die alten Kurven stehen, und die Skizze bekommt die Warnung „Bezug passt nicht mehr“ wie bei einem verlorenen Bezug. Abhängigkeiten und Maße bleiben dabei erhalten.
- **Ziehen ohne Triade:** Wer mit dem Auswahlwerkzeug einen 3D-Punkt oder eine 3D-Kurve zieht, bewegt sie in der Ebene parallel zum Bildschirm durch den gefassten Punkt. `Drag` bekommt dafür eine Zielebene.
- **Skizze als Quelle einer anderen Skizze:**
  - Nur Skizzen, die im Verlauf **vor** der aktiven stehen, sind wählbar. Damit sind gegenseitige Bezüge ausgeschlossen.
  - Wird die Quelle gelöscht oder durch Umsortieren hinter die Skizze geschoben, ist der Bezug verloren. Es gibt eine Warnung, die Kurve bleibt (Repo-Regel).
  - `compute_sketch` liest die schon berechnete Quellskizze aus dem Verlauf.
- **Rechenzeit beim Neuberechnen:** Die 2-s-Grenze gilt auch beim Neuberechnen und Laden. Wird sie überschritten, bleibt die gespeicherte Lage, und die Skizze bekommt eine Warnung. Die Rechnung läuft im selben Weg wie die übrige Neuberechnung, ohne eigenen Hintergrundfaden.
- **Dialog-Rahmen:** Auf dem Zweig `feature/fenster-abdocken` stehen bisher nur Spec und Plan. Etappe 3 baut auf dem heutigen `Dialog` von `master` auf. Wird der Umbau vorher gemergt, wird vor Etappe 3 auf `master` nachgezogen.
- **Horizontal/Vertikal in 3D:**
  - Ein Klick auf den Knopf H/V an einer 3D-Linie wählt die nächstgelegene Achse:
    - X wird `Horizontal`.
    - Y wird `Vertical`.
    - Z wird die neue Art `AlongZ(LineOrPoints)`.
  - `Horizontal` und `Vertical` behalten ihre Bedeutung, sie rechnen an 3D-Geometrie nur in der 3D-Fassung, also parallel zur X- bzw. Y-Achse, mit z-Anteil 0.
  - In die Datei kommen neu `entlang_z`, `fest3` sowie die internen `in_ebene` und `ellipsenachsen3` (Format 20). Das Symbol für Z ist ein senkrechter Strich mit „Z“.

## Etappen (ein Zweig `feature/skizzen-3d`, je Etappe Handtest nach Dennis' Go)

1. **Modell, Löser, Datei:** Abschnitte 1, 2, 3 (Profile) und 9 (Format). Ganz mit Tests in `planora-sketch`, `planora-project`, `planora-feature`; noch nichts sichtbar.
2. **Triade und Zeichnen:** Abschnitt 4 samt Palette, Tasten, Feldern, Fang, Auswahl und Anzeige fertiger 3D-Skizzen.
3. **Einbeziehen, Auf Fläche projizieren, Schnittkurve:** Abschnitte 5 bis 7, dazu Projizieren und Schneiden aus 3D-Skizzen (Abschnitt 3).
4. **Verschieben aus der Ebene, Doku:** Abschnitt 8, Benutzer-Doku, `offene-punkte.md`, voller Lauf Windows und WSL, Messtests im Release.

## Nicht im Umfang

Sweep, Rohr, Loft und jede andere Nutzung von 3D-Kurven durch Features. Außerdem nicht dabei:
- Isoparametrische Kurve und Gedrehtes Profil
- Slice und „Anzeige in der Skizze“ (eigenes Teilprojekt)
- Stutzen, Rundung, Versatz, Muster, Spiegeln und Skalieren an 3D-Geometrie (A5)
- 3D-Skizzen in 2D-Zeichnungen
- Push, Release, CI

## Dem Bauenden überlassen

- Die genaue Formel jeder 3D-Abhängigkeit, solange sie im Ergebnis gleich wirkt und in 2D-Lage dieselbe Lösung wie die 2D-Fassung gibt.
- Die Art der Spline-Annäherung (Interpolation oder Ausgleich), solange die Abweichung unter der Toleranz bleibt.
- Farben und Maße der Triade im Rahmen des Graphit-Stils.
- Die Aufteilung der Tasks innerhalb der Etappen.
