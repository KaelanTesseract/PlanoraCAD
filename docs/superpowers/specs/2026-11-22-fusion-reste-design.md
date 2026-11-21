# Restliche Abweichungen von Fusion – Entwurf

Stand 2026-11-22. Punkt C der offenen Punkte, Vorschlag von Claude, Punkt 2 nach Dennis' Korrektur. Von Dennis freigegeben („Passt so“).

## Warum

PlanoraCAD soll sich genau wie Fusion verhalten. Nach `fix/fusion-abweichungen` (c6f97671) sind fünf
Grenzen geblieben (`docs/superpowers/offene-punkte.md`, Abschnitt „Abweichungen von Fusion“).

## Ziele, Nicht-Ziele

- Ziel: Die fünf Punkte unten, Verhalten wie in Fusion (Quellen: Fusion-Hilfe, verlinkt je Punkt).
- Nicht-Ziel: Fang/Raster beim Ziehen; Griffe, die unter einem Fenster greifbar sind (in Fusion
  verdeckt das Fenster den Griff ebenso); feste Pixellänge des Griffs ändern (Fusion: feste
  Bildschirmgröße).
- Anti-Ziel: Geänderte Dateiformate oder andere Zahlen in bestehenden Dokumenten. Bestehende Tests
  bleiben grün, außer sie verlangen ausdrücklich das alte Verhalten von Punkt 2 oder 4.

## Wege

1. Alles in einem Zweig `fix/fusion-reste`, je Punkt ein Commit (gewählt): kleine Punkte, gemeinsame
   Helfer (Griffe, Enter).
2. Je Punkt ein Zweig: mehr Merges ohne Gewinn, die Punkte berühren dieselben Dateien.

## Entwurf

### 1. Drehgriff an der Tangentialebene
- Fusion: „Drag the Angle manipulator handle“ ([Hilfe](https://help.autodesk.com/cloudhelp/ENU/Fusion-Model/files/SLD-CONSTRUCT-TANGENT-PLANE.htm)).
- `PlaneDef::Tangent { face, base, angle }` bekommt in `plane_dialog::grip_shape` einen
  `GripShape::Angle` um die Berührlinie (Achse der Drehung, wie sie `angle` heute schon dreht).
  Ziehen setzt `angle` wie beim Drehpfeil der Ebene unter Winkel (Rundung 0,1°, Esc nimmt den Zug
  zurück, Formel wird ersetzt).

### 2. Stückliste ohne ausgeblendete Körper, mit Hinweis (Dennis)
- `Document::parts_rows` zählt nur Körper mit `body_shown` (Auge an, Ordner „Körper“ an).
  Positionsnummern zählen über die gezeigten Körper; Positionsnummern-Ballons hängen an den Schlüsseln.
- Hinweis, solange mindestens ein ausgeblendeter Körper fehlt: „N ausgeblendete Körper sind nicht in
  der Stückliste.“ (de/en über `tr!`). Er steht auf dem Bildschirm über einer Zeichnung mit Stückliste
  und im Blatt-Fenster, nicht auf dem gedruckten oder exportierten Blatt.
- Zwischenspeicher der Stückliste (`sheet_dialog::parts_rows`, Zeichnungsansicht) merken die Augen,
  sonst bleibt die Liste nach einem Klick auf ein Auge alt.
- Maße an Kanten ausgeblendeter Körper: unverändert (Kante fehlt in der Ansicht).

### 3. Enter in allen Befehlsfenstern
- Ein gemeinsamer Weg (`dialog::left_with_enter` bzw. ein Nachfolger) für alle Befehlsfenster: Enter
  direkt im Zahlenfeld übernimmt, wenn der Übernehmen-Knopf frei ist; Esc bricht ab.
- Betroffen u. a.: `surface_project_dialog.rs` (Auf Fläche projizieren, Schnittkurve),
  `curvature_dialog.rs`, `isoparametric_dialog.rs`, `spun_profile_dialog.rs`, `sketch_dxf_dialog.rs`,
  `blend_dialog.rs`, `boolean_dialog.rs`, `dimension_dialog.rs`, `sheet_dialog.rs`,
  `drawing_dialog.rs`, `export_dialog.rs`, `step_import_dialog.rs`, `import_dialog.rs`,
  `insert/{svg,dxf,canvas}_dialog.rs`. Fenster ohne Übernehmen-Knopf oder mit mehrzeiligem Text
  (Formeln, Skizzentext) bleiben, wie sie sind.

### 4. Rundung und Fase in der Skizze mit Griff
- Fusion: Radius bzw. Abstand per Griff ziehen oder tippen ([Rundung](https://help.autodesk.com/cloudhelp/ENU/Fusion-Sketch/files/SKT-ADD-FILLETS.htm), [Fase](https://help.autodesk.com/cloudhelp/ENU/Fusion-Sketch/files/SKT-ADD-CHAMFERS.htm)).
- Rundung: Startradius wie heute (`FILLET_DEFAULT_SHARE`), dazu ein Radiusgriff an der Vorschau.
  Ziehen ändert den Radius und schreibt ihn ins Feld.
- Fase: Die Länge kommt nicht mehr aus der Zeigerposition. Startwert nach derselben Regel wie die
  Rundung (Anteil der kürzeren Linie). Je Länge ein Griff auf der Linie (bei gleichem Abstand ein
  Griff, bei zwei Abständen zwei, bei Abstand und Winkel einer für den Abstand).
- Rundung der gezogenen Werte wie bei den Ebenen: 0,01 mm.

### 5. „Skizze erstellen“: Doku berichtigen
- Der Code entspricht den Annahmen. In `offene-punkte.md` den Wortlaut präzisieren: Eine sichtbare
  Grundebene gewinnt, wenn sie vorn liegt; eine gesperrte Vorauswahl (`sketch_on_face_blocked`) gibt nur
  eine Meldung, keinen Wahlmodus.

## Dem Bauenden überlassen

Form und Farbe der neuen Griffe (wie die vorhandenen), Name des Enter-Helfers, Lage des Hinweises
über der Zeichnung, Aufteilung in Commits.
