# Skizzen 12: Anzeige in der Skizze – Entwurf

Stand: 2026-11-18, Ausgang master `11b9c031`. Zweig: `feature/skizzen-anzeige` (neu von master).

## Absicht

PlanoraCAD soll Skizzen so anzeigen wie Autodesk Fusion. Heute fehlen im Skizzenfeld die meisten
Anzeigeschalter, Skizzenlinien liegen immer vor den Körpern (keine Verdeckung), und fertige Skizzen
lassen sich in der 3D-Ansicht nicht anklicken. Dadurch gibt es auch kein Slice, und Folgepunkte wie
„Skizzenlinie als Achse für Ebene unter Winkel“ (Skizzen 6) bleiben liegen. Dazu nimmt das
Teilprojekt vier kleinere Reste mit, die Dennis ausdrücklich dabeihaben will.

Erfolg heißt: Wer Fusion kennt, findet im Skizzenfeld dieselben Haken mit derselben Wirkung; eine
Skizze auf der Mittelebene eines Körpers ist verdeckt, bis man Slice einschaltet; eine fertige
Skizze lässt sich in 3D anklicken und als Achse oder Punkt in Befehlen nutzen.

## Ziele

1. **Skizzenfeld wie Fusion** (Fusion-Hilfe „Sketch Palette reference“, Abschnitt Options): neue
   Haken *Slice*, *Punkte anzeigen*, *Maße anzeigen*, *Abhängigkeiten anzeigen*,
   *Konstruktionsgeometrie anzeigen*, *Projizierte Geometrie anzeigen*. Vorhanden sind schon
   *Blick auf* (Symbol oben im Feld), *Skizzenraster*, *Fang*, *Profil anzeigen*, *3D-Skizze*.
2. **Verdeckung:** Körper verdecken die Linien und Profilflächen aller Skizzen, der geöffneten und
   der fertigen. Dazu die Konstruktionsebenen (Roadmap-Punkt „Ebenen mit Verdeckung“). Auch die
   geöffnete Skizze wird verdeckt, ohne Sonderfall (mit Dennis abgestimmt am 2026-11-18): Eine
   Skizze im Innern eines Körpers ist beim Zeichnen nicht zu sehen, bis man Slice einschaltet; dafür
   gibt es Slice in Fusion.
3. **Slice:** schneidet bei geöffneter Skizze die Körper an der Skizzenebene auf, so dass man auf
   die Skizze sieht.
4. **Fertige Skizzen in 3D anklickbar:** Kurven und Punkte fertiger Skizzen lassen sich in der
   Modellansicht wählen (Hervorhebung, Rechtsklickmenü), verdeckte Stücke nicht.
5. **„Maße anzeigen“ für fertige Skizzen** im Rechtsklickmenü des Baums (wie Fusion „Show
   Dimension“): Die Maße der fertigen Skizze stehen dann in der 3D-Ansicht.
6. **Skizzenlinie als Achse, Skizzenpunkt als Punkt** in den Ebenen-Dialogen per Klick in 3D
   (Folgepunkt Skizzen 6; das Modell kennt `AxisRef::SketchLine` und `PointRef::SketchPoint` schon).
7. **Reste:**
   - Winkelanzeige beim Ziehen an Drehgriffen (Text-Drehgriff aus 11b und die übrigen Drehgriffe).
   - „Leistung in der App“ aus `offene-punkte.md`, Abschnitt Skizzen 11b.
   - „Härtungen nach Skizzen 5“, soweit noch offen (siehe Abschnitt Reste).

## Nicht-Ziele

Nicht in diesem Teilprojekt (bleiben in Roadmap und Fusion-Vergleich offen): Netzschnitt-Skizze und
„Kurven an Netzschnitt anpassen“, AutoConstrain, einlinige Schrift statt SHX, Skizzen in
Zeichnungen, DXF aus 3D-Skizzen, Drehprofil für beliebige Körper, Isokurve „physische Entfernung“,
DXF einfügen „je Ebene eine Skizze“. Ebenso nicht: Schnittansicht des Modells (Fusion „Section
Analysis“), Zeichnungen und Vorschaubilder mit ausgeblendeten Körpern (Skizzen-6-Folgepunkt).

## Anti-Ziele

- Ruckeln: Die Ansicht darf bei großen Skizzen oder Modellen nicht spürbar langsamer werden als heute.
  Der Umbau soll schneller oder gleich schnell sein (Messtest, siehe unten).
- Griffe, Felder am Zeiger, Auswahl, Fang und Ziehen der offenen Skizze dürfen sich durch den
  Umbau nicht anders bedienen als heute.
- Keine Skizze auf einer Körperfläche darf durch die Verdeckung flackern oder verschwinden
  (Tiefenkampf zwischen Linie und Fläche).
- Alte Dateien bleiben bytegleich.

## Einschränkungen

- Verhalten genau wie Fusion; Abweichungen nur mit Begründung in dieser Spec und in
  `docs/superpowers/offene-punkte.md`.
- Texte nur über `tr!`/`msg!`, de und en, Namen wie in der deutschen/englischen Fusion.
- Jede neue Schaltfläche ist als Befehl in den Einstellungen belegbar (Fusion hat für diese Haken
  keine Vorgabetasten; also ohne Vorgabe).
- Kein Autodesk-Material (nur Verhalten nachbauen, keine Symbole oder Texte kopieren).
- Neue Dialoge brauchen `.dock_key`. Ein Befehl = ein Rückgängig-Schritt; Abbruch hinterlässt nichts.
- Prüfen bei 1920×1080; lokal Windows und WSL voll, CI nur vor einem Release.

## Ansätze (gewählt: A)

Kern der Frage ist, wie Skizzenlinien von Körpern verdeckt werden. Heute zeichnet
`sketch_view::finished_sketch_shapes` und `show_overlay` alle Skizzen als egui-Formen über das
wgpu-Bild (`viewport.rs`, `sketches_slot`); Körperkanten laufen dagegen schon in
`planora-render/src/gpu.rs` als Linienliste mit Tiefenprüfung.

- **A. Skizzenlinien auf der Grafikkarte (gewählt).** Eine neue Linien-Pipeline mit Strichstärke
  und Strichelung zeichnet die Kurven aller Skizzen mit Tiefenprüfung; Profilflächen und
  Konstruktionsebenen ebenso. Symbole, Maße, Griffe, Felder, Fangmarken und Krümmungskamm bleiben
  egui-Overlay obenauf. Slice ist eine Schnittebene in den Shadern. Anklicken in 3D: Abstand auf dem
  Bildschirm wie heute, dazu ein Strahl gegen die Körper, ob die Stelle verdeckt ist.
  *Vorteil:* schnell und pixelgenau, auch bei großen Modellen; Slice braucht den Shader ohnehin.
  *Nachteil:* größter Umbau, der Zeichenweg der Skizze ändert sich.
- **B. Overlay behalten, verdeckte Stücke per Strahl ausschneiden.** Wenig Umbau, aber langsam bei
  vielen Kurven und großen Netzen, ungenau an Rändern; Slice bräuchte trotzdem den Shader.
- **C. Tiefenbild zurücklesen.** Ein Bild Verzögerung, Zurücklesen bremst, umständlich mit egui.

Dennis hat A gewählt (2026-11-18).

## Entwurf

### 1. Anzeigeschalter je Skizze (Datei, Format 23)

- Neue Haken im Skizzenfeld (`sketch_panel.rs`, `side_contents`), unter den vorhandenen, in
  Fusions Reihenfolge: Slice, Profil anzeigen (vorhanden), Punkte anzeigen, Maße anzeigen,
  Abhängigkeiten anzeigen, Konstruktionsgeometrie anzeigen, Projizierte Geometrie anzeigen. Das Feld
  ist schmal; die Haken dürfen in mehrere Zeilen gehen (Gestaltung dem Bauenden).
- Die Schalter gehören zur Skizze (wie in Fusion) und stehen im `SketchFeature` (neue Struktur,
  z. B. `SketchDisplay`), damit Datei und Wiederherstellen sie kennen. Vorgabe: Slice aus, alle
  anderen an. *(Annahme, im Handtest gegen Fusion prüfen.)*
- Umschalten wird behandelt wie das Auge im Baum (`Document::set_visibility`: zählt in
  `drawing_edits`, rechnet das Modell nicht neu, eigener Rückgängig-Name wie `undo-visibility`) und
  beendet die Skizze nicht. *(Berichtigt beim Planen: Das Auge ist ein Rückgängig-Schritt mit
  eigenem Namen, kein Schritt ohne Verlauf.)*
- „Profil anzeigen“ ist heute programmweit (`AppState::show_sketch_profiles`). In Fusion ist es
  eine Skizzen-Option wie die anderen; es wandert darum mit in `SketchDisplay`. Alte Dateien ohne
  Eintrag gelten als „an“.
- **Dateiformat 23:** Der Eintrag steht nur bei Skizzen, die von der Vorgabe abweichen; ohne
  Abweichung bleibt das Dokument bei seinem Format (alte Dateien bytegleich). Strenger Ladeweg wie
  bisher (unbekannte Schlüssel → beschädigte Datei). Die PCB-Zweige nehmen beim Einpflegen die
  nächste freie Nummer.
- Wirkung in der geöffneten Skizze:
  - **Punkte aus:** Skizzenpunkte (freie Punkte, Endpunkte, Mittelpunkte) werden nicht gezeichnet
    und lassen sich nicht einzeln anklicken; Fang auf Endpunkte und Mittelpunkte bleibt.
    *(Annahme, prüfen.)*
  - **Maße aus:** Maße werden nicht gezeichnet und nicht angeklickt; ein gerade neu gesetztes Maß
    (Bemaßungswerkzeug) zeigt sein Feld trotzdem, bis es bestätigt ist. *(Annahme, prüfen.)*
  - **Abhängigkeiten aus:** keine Abhängigkeitssymbole; die vorübergehenden Marken der
    automatischen Abhängigkeiten beim Zeichnen bleiben. *(Annahme, prüfen.)*
  - **Konstruktion aus:** Konstruktions- und Mittellinien werden nicht gezeichnet, nicht gewählt
    und nicht gefangen. *(Annahme, prüfen.)*
  - **Projiziert aus:** projizierte Geometrie (alles in `copies`) wird nicht gezeichnet, nicht
    gewählt und nicht gefangen; sie zählt weiter für Profile und Löser. *(Annahme, prüfen.)*
  - Ausgeblendetes bleibt im Modell, Profile und Abhängigkeiten wirken weiter.
- Fertige Skizzen zeigen in 3D wie heute Kurven (Konstruktion gestrichelt, Projektionen lila);
  Konstruktion und Projiziertes folgen dort ebenfalls den Schaltern der Skizze.
  *(Annahme: Fusion wendet die Schalter auch auf die fertige Skizze an; prüfen.)*
- Alle neuen Haken sind belegbare Befehle (`keymap/`, ohne Vorgabetaste), wirken nur bei
  geöffneter Skizze.

### 2. Neue Linien auf der Grafikkarte

- `planora-render` bekommt eine Pipeline für breite Linien (Bildschirmbreite in Pixeln, je Strecke
  zwei Dreiecke) mit Farbe, Breite und Strichelung je Ecke, Tiefenprüfung `LessEqual`, ohne
  Tiefenschreiben, mit Kantenglättung. Die heutigen Strichstärken bleiben (`FINISHED_WIDTH` 1,2,
  Konstruktion 1,0, geöffnete Skizze wie bisher), auch auf dem 5120-Pixel-Bildschirm mit
  Pixelfaktor.
- **Kein Tiefenkampf:** Skizzenlinien bekommen einen kleinen Tiefenvorsprung zur Kamera hin (wie
  Kanten heute gegen Flächen); eine Skizze auf einer Körperfläche bleibt ganz sichtbar, auch bei
  flachem Blickwinkel. Messtest/Bildtest dazu.
- Was auf die Grafikkarte geht: Kurven aller sichtbaren Skizzen (fertig und geöffnet, samt
  Hervorhebung, Auswahl- und Projektionsfarben, verlorene Projektionen gelb, Abblenden), die
  Profilflächen (geöffnete Skizze mit „Profil anzeigen“ und Profile in Dialogen mit `profile_pick`)
  und die Konstruktionsebenen (`plane_shapes`, halbdurchsichtig).
- Was Overlay bleibt (immer obenauf, wie in Fusion): Punkte (*Annahme: Fusion zeichnet Skizzenpunkte
  verdeckt wie die Linien; der Bauende entscheidet nach Handtest-Vergleich, ob Punkte auf die GPU
  gehen*), Abhängigkeitssymbole, Maße, Griffe, Felder am Zeiger, Fang- und Zeigermarken,
  Rahmenauswahl, Krümmungskamm, Leinwand-Rahmen, Einfügen- und Kalibrier-Griffe, Vorschau der
  Isokurve.
- Overlay-Teile bleiben auch dann sichtbar, wenn ihre Kurve verdeckt ist (kein Tiefentest für
  Symbole und Maße, auch nicht für „Maße anzeigen“ an fertigen Skizzen). *(Festlegung des
  Entwurfs; Annahme, dass Fusion Maße und Symbole obenauf zeichnet; im Handtest prüfen.)*
- Die Daten der Linien wechseln oft (Ziehen, Hervorhebung). Fertige Skizzen: Puffer nur bei
  Änderung (Version wie `scene_version`); geöffnete Skizze: je Bild hochladen wie das Raster.
- Die 3D-Triade und die Zeichnungs-Ansichten bleiben unberührt.

### 3. Slice

- Nur bei geöffneter Skizze und eingeschaltetem Haken. Eine Schnittebene in den Shadern der
  **Körper** (Flächen und Kanten): Skizzenlinien (auch anderer Skizzen), Profilflächen,
  Konstruktionsebenen und Leinwände schneidet Slice nicht (*Festlegung: Fusion beschreibt Slice als
  Schnitt durch Körper*). Weggeschnitten wird, was auf der Kameraseite vor der
  Skizzenebene liegt (Seite nach der Normalen, die zur Kamera zeigt; dreht man hinter die Ebene,
  wechselt die Seite). *(Annahme, prüfen.)*
- Keine Füllung der Schnittflächen; man sieht in die offenen Körper. *(Annahme, im Handtest gegen
  Fusion prüfen.)*
- Die Skizze selbst und Dinge in der Skizzenebene werden nicht weggeschnitten (kleine Toleranz).
- 3D-Skizze: Schnittebene ist die Skizzenebene (`frame`).
- Anklicken von Körpern (Projizieren, Einbeziehen, Schnitt) ignoriert bei Slice die
  weggeschnittenen Teile: Ein Strahl trifft erst hinter der Ebene. *(Annahme, prüfen.)*
- Beim Beenden der Skizze ist Slice weg; der Haken bleibt an der Skizze gespeichert und wirkt beim
  nächsten Öffnen wieder.

### 4. Fertige Skizzen anklicken

- In der Modellansicht (keine Skizze offen) und in Dialogen, die Skizzengeometrie annehmen, trifft
  ein Klick Kurven und Punkte sichtbarer fertiger Skizzen. Reihenfolge der Treffer: Ecke/Kante eines
  Körpers, dann Skizzenpunkt, dann Skizzenkurve, dann Fläche (wie `include_pick` heute in der
  geöffneten Skizze). Verdeckte Stellen (Strahl trifft vorher einen Körper) zählen nicht.
- Hover hebt die Kurve hervor, Klick wählt, Strg/Umschalt erweitern wie sonst.
- Rechtsklick auf gewählte Skizzengeometrie: „Skizze bearbeiten“, „Maße anzeigen“, „Ausblenden“
  (Auge der Skizze). *(Annahme: Menüumfang wie Fusion; prüfen.)*
- Entf auf gewählter Geometrie fertiger Skizzen: in diesem Teilprojekt **ohne Wirkung**. Ob Fusion
  damit Skizzengeometrie außerhalb der Skizze löscht, ist nicht belegt; der Handtest prüft es, ein
  Ja wird Folgepunkt in `offene-punkte.md`.
- Doppelklick: in diesem Teilprojekt ohne eigene Wirkung. *(Annahme; Handtest prüft, was Fusion
  tut.)*

### 5. „Maße anzeigen“ an fertigen Skizzen

- Rechtsklick auf die Skizze im Baum (und im Menü aus 4.): Haken „Maße anzeigen“ (Fusion „Show
  Dimension“). Dann zeichnet die 3D-Ansicht die Maße der fertigen Skizze (wie in der geöffneten,
  ohne Bearbeiten). Gehört zu `SketchDisplay` (Format 23), Vorgabe aus.
- Im geöffneten Zustand gilt der Haken „Maße anzeigen“ des Skizzenfelds; der Baum-Haken erscheint
  dann nicht (wie Fusion). Ob beide derselbe Schalter sind, entscheidet der Bauende nach Fusion
  (*Annahme: zwei getrennte Schalter, einer für offen, einer für fertig*).

### 6. Skizzengeometrie in den Ebenen-Dialogen

- „Ebene unter Winkel“: Das Feld „Achse“ nimmt per Klick eine gerade Linie einer fertigen (oder der
  geöffneten) Skizze (`AxisRef::SketchLine`).
- „Ebene durch drei Punkte“ und weitere Dialoge mit `PointRef`: nehmen Skizzenpunkte
  (`PointRef::SketchPoint`).
- Weitere Dialoge, die Linien oder Punkte als Bezug kennen, nimmt der Bauende mit, wenn das Modell
  den Bezug schon hat; neue Bezugsarten im Modell gehören nicht hierher.

### 7. Reste

- **Winkelanzeige an Drehgriffen:** Beim Ziehen an einem Drehgriff steht der aktuelle Winkel am
  Zeiger (Format über `format_angle`). Mindestens: Text-Drehgriff (11b), Drehbögen der Triade
  (`sketch_grips.rs`, Verschieben/Kopieren). Der Bauende sucht weitere Drehgriffe.
  Gleichzeitig offene Frage aus 11b, Punkt 5: Werkzeugwechsel mitten im Zug am Rho-Griff und am
  Drehgriff gleich machen (wie Fusion).
- **Leistung in der App** (Liste in `offene-punkte.md`, Skizzen 11b): `Document::reshape_texts`
  klont nur bei Bedarf; `compute_sketch` baut `ParamScope` nur bei Skizzen mit Text;
  `grow_frame_to_fit` ohne Klon der ganzen Skizze; `is_feature_hidden` ohne Lauf über die
  Gruppenkette je Aufruf (Zwischenspeicher); `text_rotate_grips` einmal je Bild. Jede Stelle mit
  Messung vorher/nachher; Verhalten unverändert (vorhandene Tests).
- **Härtungen nach Skizzen 5**, noch offen sind fünf:
  1. Schleife über zwei Muster (A liest Exemplare von B, B von A) wird erkannt und abgelehnt wie die
     Schleife in einem Muster (beim Laden: Muster auflösen mit Meldung, wie dort).
  2. `Id::from_index` schneidet große Indizes nicht mehr still ab (Fehler statt Abschneiden;
     betrifft `MusterWert`).
  3. Ein lebendes Glatt mit veraltetem Ende aus einer von Hand geänderten Datei: `from_data` lehnt
     ab (beschädigte Datei, wie der strenge Ladeweg sonst).
  4. Scheitert das Gitter eines Musters, nennt die Meldung den Grund (z. B. parallele
     Richtungslinien).
  5. Löser-Kosten: Vorschau ohne Klon der Skizze je Bild, wenn sich nichts ändert; `add_constraint`
     baut die Gruppen für die Schleifenprüfung nicht jedes Mal neu; `dof` im gemeldeten Fall neu.
  Erledigt sind schon: „Winkelmaß folgt der Zeigerseite“ (11a, A7) und „Felder am Zeiger mit der
  Maus anklickbar“ (11a, A9: wie Fusion nicht anklickbar). Die Roadmap wird berichtigt.

## Etappen (je ein Handtest mit Dennis' Go)

1. **Verdeckung und Slice:** neue Linien-Pipeline, Skizzen, Profile und Ebenen auf die GPU,
   `SketchDisplay` mit Format 23 und dem Haken Slice, Slice-Schnittebene in den Körper-Shadern.
   So ist Slice im ersten Handtest schon an der verdeckten Skizze prüfbar.
2. **Schalter und Anklicken:** die übrigen Haken mit ihrer Wirkung, Befehle in der Keymap,
   Anklicken fertiger Skizzen, Rechtsklickmenü, „Maße anzeigen“ im Baum, Ebenen-Dialoge.
3. **Reste:** Winkelanzeige, Leistung, Härtungen; Doku, Fusion-Vergleich, voller Lauf.

## Prüfen

- Modelltests für `SketchDisplay`, Format 23 (Rundlauf, alte Dateien bytegleich, beschädigte
  Einträge), Härtungen.
- App-Tests: Haken wirken auf Zeichnen, Wählen und Fang; Anklicken fertiger Skizzen mit und ohne
  Verdeckung (Strahltest gegen einen Quader); Slice-Seite je Kamerarichtung.
- Render: reine Funktionen (Linien zu Dreiecken, Strichelung, Schnittebene) mit Tests; Bild der GPU
  nur im Handtest.
- Messtest im Release: Bildzeit beim Ziehen in einer großen Skizze (z. B. 2000 Kurven) und Umlauf um
  ein Modell mit fertigen Skizzen. Tor: Median aus fünf Läufen im Release auf Dennis' Rechner
  höchstens 10 % über master (Rauschen); die Leistungspunkte aus 11b müssen messbar schneller sein.
- Handtest je Etappe bei 1920×1080 mit `tools/gui-drive.ps1` (Fenster der Klasse „Window Class“),
  Vergleich der markierten Annahmen mit Fusion.

## Dem Bauenden überlassen

Aufteilung der Haken im schmalen Feld, Name und Form von `SketchDisplay`, Aufbau der Pipeline
(Instanzen oder Dreiecksliste), Größe des Tiefenvorsprungs, Puffer-Versionierung, welche weiteren
Drehgriffe und Dialoge mitkommen. Alle mit *Annahme* markierten Punkte: Fusion-Verhalten im Handtest
prüfen und bei Abweichung angleichen, Ergebnis in `offene-punkte.md`.
