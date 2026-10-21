# STEP-Import – Entwurf

Stand 2026-10-21. Nächstes Teilprojekt nach den Parametern. **Alle Werte unten sind Vorschläge mit Standardwerten**; Dennis korrigiert, was nicht passt. Beschlossen ist bisher nur: STEP-Import ist das nächste Teilprojekt.

## 1. Zweck

Heute kommen fremde Teile nur als **Netz-Körper** (STL/OBJ) ins Programm: Dreiecke, die sich anzeigen, verschieben und messen lassen, aber nicht verrechnen, runden oder als Skizzenfläche nutzen. STEP-Dateien enthalten dagegen **echte Volumenkörper** (Flächen, Kanten, Rundungen, Bohrungen). Ein STEP-Import macht daraus ganz normale Körper des Programms: Man kann sie abziehen, vereinigen, verrunden, auf ihre Flächen skizzieren, in Zeichnungen mit Schnitten und Maßen zeigen. Normteile, Zulieferteile und Teile aus anderen CAD-Programmen werden damit nutzbar. Der Export (STEP-Export) gibt es seit Teilprojekt 10; der Import ist die Gegenrichtung.

## 2. Umfang (Vorschlag)

**Ziele:**
- **Menü „Datei → STEP importieren …“** (Endungen `.step`, `.stp`), Fenster wie beim Netz-Import: Das Fenster liest die Datei im Hintergrund (Fortschritt, Abbrechen) und zeigt danach eine **Liste der gefundenen Körper** mit Name, Zahl der Flächen, Größe, Einheit und dem Prüfergebnis („wird übernommen“ oder „wird ausgelassen: <Grund>“). Jeder Körper hat ein Häkchen (Vorgabe: alle, die sich übernehmen lassen). „Importieren“ legt sie an.
- **Gelesene Formate:** STEP AP203, AP214 und AP242 (nur die Geometrie), Dateien als Text (`ISO-10303-21`), Körper vom Typ `MANIFOLD_SOLID_BREP` und `BREP_WITH_VOIDS` (Körper mit Hohlräumen).
- **Flächen:** Ebene, Zylinder, Kegel, Kugel, Torus, B-Spline-Flächen (auch rational, mit und ohne Knotenliste, einfach und als „Quasi-Uniform/Uniform/Bezier“-Varianten). **Kanten:** Gerade, Kreis, Ellipse, B-Spline-Kurve (auch rational), Kantenkurven aus `SURFACE_CURVE`/`SEAM_CURVE` (es zählt die 3D-Kurve).
- **Einheiten:** Die Einheit der Datei (Millimeter, Zentimeter, Meter, Zoll, auch über `CONVERSION_BASED_UNIT`) wird gelesen und nach Millimeter umgerechnet; im Fenster steht sie und lässt sich bei Bedarf umstellen (wie beim Netz-Import).
- **Ergebnis im Dokument:** Je Körper ein Feature **„STEP-Körper“** im Feature-Baum (mit dem Namen aus der Datei). Der Körper ist ein ganz normaler exakter Körper: verschieben (Position, Verschieben-Funktion), vereinigen, abziehen, schneiden, verrunden, fasen, als Fläche für Skizzen, in Zeichnungen, Stückliste, Export (STEP, STL, OBJ, 3MF, glTF) – alles wie bei selbst gebauten Körpern. Seine eigene Form ist nicht parametrisch änderbar (es gibt keine Maße dafür).
- **Speichern:** Die Körperform steht **in der Projektdatei** (kein Verweis auf die Originaldatei): je STEP-Körper ein Eintrag mit dem von unserem eigenen Schreiber erzeugten, normalisierten STEP-Text; beim Laden wird er wieder gelesen. Dateiformat 10, nur wenn ein STEP-Körper im Dokument ist; ältere Dateien unverändert.
- **Heilung (Vorschlag):** Kanten und Ecken, die sich innerhalb der Toleranz der Datei (Vorgabe 1e-3 mm oder der Wert aus `UNCERTAINTY_MEASURE_WITH_UNIT`) treffen, werden zusammengenäht; danach prüft der Kernel den Körper (`validate`). Körper, die nicht geschlossen oder ungültig sind, werden **ausgelassen** mit deutscher Begründung; die anderen werden trotzdem übernommen.
- **Namen:** Der Name des Produkts oder Körpers aus der Datei wird übernommen (gekürzt, doppelte nummeriert), sonst „STEP-Körper n“.

**Nicht-Ziele (erste Fassung):** Baugruppenstruktur (Hierarchie, Mehrfachverwendung, Stücklisten aus STEP; Vorschlag: Alle Körper der Datei werden einzeln und **flach** übernommen, einfache Platzierungen (`ITEM_DEFINED_TRANSFORMATION` mit Verschiebung und Drehung) werden angewandt, alles andere ergibt eine Meldung); Farben, Materialien, Ebenen, Texturen; Toleranzen und Bemaßung (PMI, AP242); Flächenmodelle und offene Schalen (`OPEN_SHELL`, `SHELL_BASED_SURFACE_MODEL`, einzelne Flächen); Flächenarten `SURFACE_OF_LINEAR_EXTRUSION`, `SURFACE_OF_REVOLUTION`, `OFFSET_SURFACE`, `DEGENERATE_TOROIDAL_SURFACE` (Meldung „Fläche vom Typ … wird noch nicht unterstützt“); STEP-Dateien als `.stpz`/gepackt; XML-Form (AP242 XML); Netz-Daten (Tessellation); Schreiben von Importen als „Originaldatei“ (Export schreibt neu).

**Anti-Ziel:** Ein importierter Körper, der still anders aussieht als in der Datei (falsche Fläche, verdrehte Seiten, falsche Einheit) oder der den Kernel zu falschen Verrechnungen verleitet. Lieber ein Körper wird mit Begründung ausgelassen.

## 3. Bedienung (Vorschlag)

- „Datei → STEP importieren …“ (Menü und Befehlsfeld, wie „Netz importieren …“); Dateiauswahl im vorhandenen Dialog; Fenster mit Fortschritt, Liste der Körper (Name, Flächen, Maße der Hülle, Einheit, Status), Einheitenwahl, „Importieren“ und „Abbrechen“.
- Nach dem Import: Die Kamera passt auf die neuen Körper ein; der erste ist gewählt; die Meldung nennt „3 Körper importiert, 1 ausgelassen: …“.
- Rückgängig: Ein Import ist **ein** Schritt („STEP importieren“).
- Zeichnungen, Stückliste und Export behandeln STEP-Körper wie alle exakten Körper.

## 4. Technischer Ansatz (Vorschlag, im Plan zu prüfen)

- **Eigener Leser** in `planora-step` (neben dem Schreiber; keine neue Abhängigkeit): Zeichenstrom → Lexer → Entity-Tabelle (`#id = TYPE(...)`, auch **komplexe Einheiten** `(A() B())`, Verweise nach vorn, Kommentare, Zeichenmaskierung `\X2\…`, Zahlen, Aufzählungen `.T.`, `$` und `*`) → Geometrie-Auflösung (Punkte, Richtungen, Achsenlagen, Kurven, Flächen) → Topologie (ADVANCED_FACE, FACE_BOUND, EDGE_LOOP, ORIENTED_EDGE, EDGE_CURVE, VERTEX_POINT, VERTEX_LOOP) → Kernel-`Solid` (Flächen mit `surface`, `same_sense`, Außen- und Innenschleifen aus 3D-Kanten). Der Leser benutzt dieselben Kernel-Bausteine wie die Booleschen Operationen (Schleifen aus 3D-Kanten, Naht bei periodischen Flächen).
- **Heilung und Prüfung:** Zusammennähen über Toleranz; Orientierung der Flächen nach außen (Volumen positiv, Orientierung aus `ORIENTED_CLOSED_SHELL`/`same_sense` beachten); `validate` des Kernels; Volumen- und Flächenvergleich nach der Heilung als Plausibilität.
- **Hintergrund:** wie Netz-Import: `Progress` mit Abbruch; große Dateien stören die Oberfläche nicht.
- **Dokument:** neue Feature-Art `StepBody { solid_ref }` und eine **Tabelle der Fremdkörper** im Dokument (Kennung → normalisierter STEP-Text, Arc geteilt, Größenschätzer im Verlauf: wie die Netz-Tabelle); Snapshot/Verlauf/Speichern/Laden/Zeichnungen/Stückliste kennen sie; die Berechnung (`recompute`) baut den Körper aus dem Text (zwischengespeichert nach Version).
- **Round-Trip als Prüfstein:** Eigener Export → eigener Import muss für Quader, Zylinder, Bohrung, Rundung, Kugel, Torus, Freiformfläche gleiche Flächenzahl, Volumen, Oberfläche und Hüllbox ergeben.
- **Testdateien nur selbst erzeugt oder handgeschrieben** (Lizenz): keine fremden STEP-Beispiele ins Repository. Eigene Handproben für Dateien, wie sie andere Programme schreiben (AP214 mit komplexen Einheiten, `SEAM_CURVE`, `VERTEX_LOOP` an Kugelpolen, rationale B-Splines, `CONVERSION_BASED_UNIT` Zoll).
- **Kein Eingriff in den Kernel, wenn vermeidbar** (wie bei den Schnitten: eine Kernel-Änderung nur mit Zustimmung); neue Abhängigkeiten: keine.

## 4a. Festlegungen aus der Vorabprüfung (meine Entscheidungen, Dennis darf widersprechen)

1. **Flächen und Kurven, erweitert:** Zusätzlich zu §2 liest die erste Fassung `SURFACE_OF_REVOLUTION` und `SURFACE_OF_LINEAR_EXTRUSION` (werden zu B-Spline-Flächen umgerechnet, der Kernel kann das schon für Drehflächen), `TRIMMED_CURVE` (wird entpackt) sowie **ungeklemmte und periodische B-Splines** (Knoten werden durch Knoteneinfügen geklemmt). Ohne sie fielen viele echte Teile heraus. Nicht dabei bleiben Polylinie, Parabel, Hyperbel, `OFFSET_SURFACE`.
2. **Kernel-Gegebenheiten, die der Leser selbst erledigt:** Der Kernel kennt keine Entartungskanten und keinen Pol als Schleife; der Leser entfernt `VERTEX_LOOP` und Kanten der Länge null und leitet für jede nicht ebene Fläche den Wertebereich (`patch`) und die Kantenparameter aus den Schleifen ab (periodische Flächen entfaltet). Seine Flächen bekommen feste **Marken „STEP-Fläche n“** (Reihenfolge der Datei), damit Skizzen auf Flächen, Verrunden und Boolesche Operationen funktionieren. Die Außenrichtung wird aus dem Volumenvorzeichen bestimmt und die Hülle bei Bedarf umgedreht.
3. **Genauigkeit:** Der Kernel arbeitet mit 1e-6 mm; STEP-Dateien tragen oft 1e-3 mm. Der Leser **zieht nur zusammen und paart gleiche Kanten** (keine Lücken schließen) mit der Toleranz der Datei, höchstens 1e-3 mm. Körper, bei denen die Toleranz der Datei gröber als 1e-5 mm ist, werden **übernommen mit Warnung** („Rechnen mit diesem Körper kann scheitern“), Körper mit ungültigem Ergebnis ausgelassen. Ob Boolesche Operationen und Rundungen auf fremden Körpern sicher laufen, wird gemessen und in den Grenzen festgehalten.
4. **Sofort normalisieren:** Beim Import wird der Körper einmal mit unserem Schreiber (fester Zeitstempel) geschrieben, der Text wieder gelesen und **genau dieser** Körper benutzt; so ist Sitzung = Neuladen. Der Text liegt als `Arc` in einer Tabelle des Dokuments (wie die Netze), der gebaute Körper wird einmal gebaut und geteilt, damit Rückgängig nicht jedes Mal neu liest.
5. **Speicherplatz:** Der normalisierte Text braucht grob 1 KB je Fläche (500 Flächen etwa 0,5 MB); jede gespeicherte Version enthält ihn erneut. Das wird hingenommen; Grenze 20 000 Flächen je Körper.
6. **Dokument:** `StepBody` geht über die gewöhnliche Fertigstellung des Körpers (Prüfung, Ausdehnung), Dateiformat 10 mit eigener Regel vor der Regel für Format 9; die Stellen, die heute `MeshBody` kennen (etwa 14 in 5 Crates), werden ergänzt.
7. **Aufwand (grob):** Lexer/Parser 3 Tage, Geometrie 4, Topologie/Normalisieren/Zusammenziehen 6–8, Dokument/Projekt/App 5: zusammen etwa 18–20 Arbeitstage klassischer Zählung, in der Praxis die größte Aufgabe seit dem Kernel.

## 5. Grenzen und Fehler (Vorschlag)

- Dateigröße höchstens 200 MB, höchstens 5 Millionen Einträge, höchstens 100 000 Flächen je Körper und 50 Körper je Datei; darüber klare Meldung.
- Dateien, die kein STEP sind, beschädigt oder abgeschnitten sind, geben eine deutsche Meldung mit Zeilennummer, nie einen Absturz.
- Heilungsgrenze: Lücken über der Toleranz machen den Körper ungültig (ausgelassen mit „Lücke von x mm zwischen …“).
- Entartete Kanten (Länge null) und Spindelringe (Torus mit Achsenberührung) werden gemeldet oder, wenn eindeutig, bereinigt; im Zweifel ausgelassen.
- Große B-Spline-Körper können beim Verrechnen langsam sein (wie heute bei Freiformflächen); keine neue Grenze.

## 6. Tests und Prüfung

- Lexer und Parser mit Feinheiten (Kommentare, Zeilenumbrüche mitten in Einträgen, Maskierungen, Vorwärtsverweise, komplexe Einheiten); feindliche Dateien (abgeschnitten, Zyklen in Verweisen, riesige Zahlen, tiefe Verschachtelung, doppelte Kennungen, fehlende Verweise) → Meldungen, keine Abstürze.
- Geometrie: jede Flächen- und Kantenart gegen Handrechnung (Kreise, Kegel mit Halbwinkel, Torus, rationale B-Splines: Kreis als NURBS, Kugel).
- Topologie: Schleifen mit Seitenwechsel, Naht, Pole, Innenschleifen, Körper mit Hohlraum.
- Round-Trip mit den bekannten Körpern, auch nach Verrunden/Fasen/Booleschen Operationen; Einheiten (mm, m, Zoll).
- Dokument: Import als ein Schritt, Rückgängig/Wiederholen, Speichern/Laden (Format 10), Zeichnungen und Stückliste mit STEP-Körpern, Export des importierten Körpers wieder als STEP.
- Oberfläche: Fenster, Fortschritt, Abbruch, Liste mit Status, Einheitenwahl, kleine Fenstergrößen, Hell/Dunkel; Handliste mit echtem Fenster.

## 7. Offen (Vorschläge, bitte korrigieren)

1. **Umfang** wie in §2 und §4a.1 (AP203/214/242, Körper und Körper mit Hohlräumen, Ebene/Zylinder/Kegel/Kugel/Torus/B-Spline plus Dreh- und Strangflächen und ungeklemmte Splines)?
2. **Baugruppen:** Alle Körper einer Datei **flach** übernehmen, einfache Platzierungen anwenden, Struktur ignorieren?
3. **Nicht lesbare oder ungültige Körper:** auslassen mit Begründung und die übrigen trotzdem übernehmen (statt alles oder nichts)?
4. **Speichern:** Die Körperform steckt als normalisierter STEP-Text in der Projektdatei (je Version erneut), kein Verweis auf die Originaldatei?
5. **Wirkung und Genauigkeit:** Ein STEP-Körper ist ein normaler exakter Körper, seine Form nicht parametrisch änderbar; Körper mit grober Dateigenauigkeit (gröber als 1e-5 mm) werden mit Warnung übernommen („Rechnen kann scheitern“) statt abgelehnt? Und falls das Messen zeigt, dass der Kernel dafür eine Änderung braucht: darf ich sie vorschlagen und Sie entscheiden dann?
