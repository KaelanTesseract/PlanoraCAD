# Skizzen 11b: Kopieren/Einfügen, Skizze als DXF, Skizzengruppen, Text-Reste, Drehprofil, Isoparametrische Kurve

Stand 2026-11-17. Zweiter Teil von „Skizzen 11: Reste“ (Teilung von Dennis bestätigt, „passt“). Vorgänger: 11a „Zeichnen und Prüfen“ (b1beff9, Folgepunkte 1e56103c). Maßstab ist Fusions Verhalten („mach es wie in Fusion“; bei den Skizzen entscheidet Claude). Was die Fusion-Hilfe nicht belegt, ist als **Annahme** markiert und kommt in `docs/superpowers/offene-punkte.md`. Recherche mit Quellen: `.superpowers/sdd/2026-11-17-skizzen-11b/recherche.md` (nicht im Git; die Kernaussagen stehen hier).

## Warum

Nach 11a fehlen beim Arbeiten mit Skizzen noch die Werkzeuge, mit denen man Geometrie **übernimmt** und Skizzen **ordnet**: Strg+C/Strg+V zwischen Skizzen, eine Skizze als DXF für Laser oder Fräse speichern, Skizzen im Browser in Gruppen sortieren. Dazu kommen zwei Projektionswerkzeuge aus Fusions Menü „Projizieren/Einbeziehen“ (Drehprofil für Drehteile, Isoparametrische Kurve auf Flächen) und die Reste beim Skizzentext (Höhe als Parameter, Parameter im Text, Drehgriff).

**Gelungen ist es, wenn** man in einer Skizze Kurven wählt, Strg+C drückt, eine andere Skizze öffnet, Strg+V drückt und die Kopie samt Abhängigkeiten und Maßen gleich verschieben kann; wenn man im Browser auf eine Skizze rechtsklickt, „Als DXF speichern“ wählt und die Datei in einem Laserprogramm richtig groß ankommt; wenn man zehn Skizzen in zwei Gruppen ordnet und eine Gruppe mit einem Auge ausblendet; wenn man in einem Text „{Breite} mm“ schreibt und der Text sich mit dem Parameter ändert; wenn man aus einer gedrehten Welle per Drehprofil ihren Umriss in die Skizze bekommt; wenn man auf einer gewölbten Fläche U- und V-Linien als Skizzenkurven bekommt, die mit der Fläche mitgehen.

**Fehlschlag wäre:**
- Alte Dateien werden nicht mehr bytegleich geschrieben, oder ein Dokument ohne das Neue bekommt Format 22.
- Strg+C/V stören das Kopieren in Textfeldern (Formelfeld, Textwerkzeug, Felder in der Zeichnung).
- Eingefügte Abhängigkeiten zeigen auf die Quellskizze oder auf Projektionen, die in der Zielskizze nicht existieren.
- Ein DXF ist in Zoll-Programmen falsch skaliert.
- Ein verlorener Bezug von Drehprofil oder Isokurve ist ein Fehler statt einer Warnung.

## Ziele

### 1. Kopieren, Ausschneiden, Einfügen

- In der offenen Skizze: gewählte Kurven, Punkte und Texte mit **Strg+C** (Kopieren), **Strg+X** (Ausschneiden = Kopieren und Löschen nach der Löschregel aus 11a, `remove_curve_ends`), **Strg+V** (Einfügen); dieselben Befehle im Rechtsklickmenü der Skizze. Neue Aktionen `skizze.kopieren`, `skizze.ausschneiden`, `skizze.einfuegen`, vorbelegt mit Strg+C/X/V, in den Einstellungen änderbar.
- Hat ein Textfeld den Fokus (Formelfeld, Felder in der Zeichnung, Textwerkzeug, Dialogfelder), gehören die Tasten dem Feld (Vorbild `sketch_inline.rs:173`).
- Die Zwischenablage ist programmintern und hält **Werte ohne Ids** der Quellskizze: Kurven mit Stil (Konstruktion, Mittellinie), Punkte, Texte mit Rahmen, die Abhängigkeiten und Maße, deren Beteiligte alle in der Auswahl liegen. Abhängigkeiten zu Dingen außerhalb der Auswahl (Ursprung, Projektionen, andere Kurven) fallen weg. Ein Maß mit Formel behält die Formel als Text. Sie gilt über Skizzen und Dokumente hinweg, solange das Programm läuft (**Annahme**; kein Austausch mit anderen Programmen).
- Was nicht kopierbar ist, bleibt draußen und die Meldung nennt die Zahl: Projektionen und andere Kopien aus `copies` werden als gewöhnliche, freie Kurven kopiert (Fusion: Kopie einer Projektion ist keine Projektion, **Annahme**); feste Splines werden als feste Splines kopiert; Text auf einem Pfad ohne den Pfad in der Auswahl wird abgelehnt wie bei Verschieben/Kopieren (`sketch-text-path-follows`).
- **Einfügen**: die Kopie entsteht an der Quelllage in den Koordinaten der Skizzenebene (2D: dieselben x/y; 3D-Skizze: dieselben Weltkoordinaten), alle eingefügten Elemente sind danach gewählt und das Werkzeug **Verschieben/Kopieren** ist mit ihnen offen, damit man sie gleich setzt (**Annahme**, Fusion öffnet nach dem Einfügen den Verschiebe-Dialog). Einfügen ist ein Rückgängig-Schritt („Einfügen“); bricht man das Verschieben ab, bleibt die Kopie an der Quelllage stehen (das Einfügen ist schon geschehen).
- 3D-Inhalt (Kurven mit z ≠ 0) in eine 2D-Skizze einfügen macht die Skizze zur 3D-Skizze wie in Skizzen 10 (`has_3d_entry`); 2D-Inhalt in eine 3D-Skizze liegt in deren Ebene.
- Ohne offene Skizze sind die Befehle ausgegraut (Fusion kopiert dort Körper; das ist nicht 11b).

### 2. Skizze als DXF speichern

- Rechtsklick auf eine Skizze im Browser → **„Als DXF speichern“** („Save As DXF“). Dialog (mit `.dock_key`): **Einheit** (Vorgabe: Einheit des Dokuments; mm, cm, m, Zoll, Fuß), Häkchen **Punkte**, **Projizierte Geometrie**, **Konstruktionsgeometrie** (Vorgabe aus, **Annahme**). OK → Dateidialog.
- Inhalt: Linien als LINE, Kreise und Bögen als CIRCLE/ARC, Ellipsen und Ellipsenbögen als ELLIPSE, Kegelschnitte und alle Splines als SPLINE (NURBS wie im Blatt), Texte als ihre Kurven, Punkte als POINT. Konstruktionsgeometrie auf einer eigenen Ebene. Koordinaten in der Skizzenebene, umgerechnet in die gewählte Einheit, `$INSUNITS` passend.
- Schreiber: der vorhandene DXF-Schreiber der Zeichnung (`planora-drawing/src/dxf.rs`, R2000, mit den in Fusion 360 geprüften Pflichtteilen) wird wiederverwendet; ein Weg Skizze → `Out2d` kommt dazu. Kein zweiter Schreiber.
- 3D-Skizzen: Meldung „Nur ebene Skizzen lassen sich als DXF speichern.“ (**Annahme**). Leere Auswahl nach Filter: Meldung, keine Datei.
- Kein Dateiformat, nur Export.

### 3. Skizzengruppen

- Im Browser-Ordner „Skizzen“ lassen sich **benannte Gruppen** anlegen und verschachteln: Rechtsklick auf Skizzen → „Gruppe erstellen“ (gewählte Skizzen kommen hinein), Rechtsklick auf Gruppe → „Umbenennen“, „Gruppe auflösen“ (Inhalt geht eine Ebene höher). Skizzen und Gruppen lassen sich per Ziehen in eine Gruppe oder aus ihr heraus legen.
- Das Auge einer Gruppe blendet alle Skizzen darin aus und ein (wie Körpergruppen in Fusion, **Annahme**); eine Skizze in einer ausgeblendeten Gruppe ist ausgeblendet, ihr eigenes Auge bleibt erhalten.
- Gruppen ändern die Zeitleiste und die Neuberechnung nicht. Löschen einer Gruppe löst sie auf und löscht keine Skizze (**Annahme**). Wird eine Skizze gelöscht, verschwindet sie aus ihrer Gruppe; leere Gruppen bleiben stehen.
- Nur Skizzen im Browser; Gruppen von Elementen innerhalb einer Skizze und Körpergruppen sind nicht Teil von 11b.
- Die Gruppen gehören zum Dokument (`Snapshot`): Datei und Rückgängig. Jede Gruppenänderung ist ein Rückgängig-Schritt.

### 4. Text-Reste

- **Höhe als Formel und Parameter**: Das Feld „Höhe“ nimmt Formeln mit Parametern; die Formel bleibt am Text und rechnet mit (wie Maße). Fusion legt zu einem Text Höhen- und Textparameter an; hier wird die Formel gespeichert, ein eigener Parameter je Text entsteht nicht (**Annahme**, schlanker; in `offene-punkte.md`).
- **Parameter im Text**: `{Name}` im Textinhalt wird durch den Wert des Parameters ersetzt (in der Dokumenteinheit, Ausgabe wie im Parameterfeld). Unbekannter Name oder Fehler: der Text zeigt `{Name}` wörtlich und das Skizzen-Feature bekommt eine Warnung (kein Fehler, der Text bleibt). Umbenennen eines Parameters ändert die Verweise mit (wie bei Formeln).
- **Drehgriff** am Textrahmen: Ziehen dreht den Text um seinen Bezugspunkt, Winkel wie im Feld „Winkel“, ein Rückgängig-Schritt.
- Nicht in 11b: SHX-Schriften (Autodesk-Material, Lizenz; eine freie einlinige Schrift wäre ein eigenes Vorhaben). Nicht gebaut, weil Fusion sie nicht hat: Zeilenabstand, Umbruch, senkrechte Schrift, Unterstreichen.

### 5. Drehprofil

- Menü Skizze → Erstellen → Projizieren/Einbeziehen → **„Drehprofil“** („Spun Profile“; deutscher Name **Annahme**). Dialog (mit `.dock_key`): Auswahlfilter **Körper** oder **Bestimmte Elemente** (Kanten, Punkte), Auswahl, **Achse** (Skizzenlinie, Konstruktionsachse, Ursprungsachse, gerade Kante), **Achse projizieren** (Häkchen: die Achse kommt als Projektion in die Skizze), **Toleranz**, **Umkehren**, **Projektionsverknüpfung** (Vorgabe an).
- Die Achse muss in der Skizzenebene liegen oder parallel zu ihr sein; sonst Meldung.
- Rechnung: Jeder Punkt der Auswahl wird um die Achse in die Halbebene der Skizze gedreht (Lage entlang der Achse bleibt, Abstand zur Achse bleibt). Kanten werden so exakt abgebildet (gerade Kante → Linie, Kreiskante koaxial → Punkt bzw. Linie, sonst Kurve, angenähert als Spline innerhalb der **Toleranz**). Für **Körper** ist das Ergebnis der äußere Umriss aller so abgebildeten Kanten und Silhouetten; in 11b exakt für Körper, deren Flächen zur Achse koaxiale Rotationsflächen oder senkrechte Ebenen sind (Wellen, Drehteile); andere Körper: Meldung „Drehprofil geht in dieser Fassung nur für Drehkörper um die gewählte Achse.“ (Abweichung, in `offene-punkte.md`). **Umkehren** legt das Ergebnis auf die andere Seite der Achse.
- Ergebnis wie eine Projektion: violette Kurven in `copies` mit neuer Herkunft `CopySource::SpunProfile`, schwacher Bezug, verlorene Quelle = Warnung und „Verlorene Projektionen verwalten“ wie in 11a. Ohne Projektionsverknüpfung: freie Kurven.
- Ein Rückgängig-Schritt; Abbruch hinterlässt nichts.

### 6. Isoparametrische Kurve

- Menü Skizze → Erstellen → Projizieren/Einbeziehen → **„Isoparametrische Kurve“** („Isoparametric Curve“). Dialog (mit `.dock_key`): **Fläche**, **Punkt** auf der Fläche, Richtung **U**, **V** oder **U und V**, Art **Einzeln** oder **Verteilt**, bei Einzeln die Lage (vorgegeben durch den Punkt; Feld **Entfernung** als Anteil 0–1 des Parameterbereichs, **Annahme** für „proportional“; die Art „physisch“ wird nicht gebaut, in `offene-punkte.md`), bei Verteilt die **Anzahl** (gleichmäßig über den Parameterbereich, ohne die Ränder), **Projektionsverknüpfung** (Vorgabe an). Vorschau live.
- Für alle Flächenarten des Kernels (Ebene, Zylinder, Kegel, Kugel, Torus, NURBS): die Kurve u = const bzw. v = const auf der Fläche, beschnitten an der Berandung der Fläche samt Löchern (ein Wert kann mehrere Stücke ergeben). Ausgabe als 3D-Kurven der Skizze (Linie, Kreisbogen, wo die Fläche das exakt hergibt; sonst fester Spline). Die Skizze wird zur 3D-Skizze wie bei „Auf Fläche projizieren“ (Skizzen 10).
- Herkunft `CopySource::Isoparametric { face, direction, value }`, schwacher Bezug, rechnet bei Flächenänderung mit; verlorene Fläche = Warnung, Neu verknüpfen über den Dialog aus 11a.
- Ein Rückgängig-Schritt; Abbruch hinterlässt nichts.

### Dateiformat

**Format 22** nur, wenn das Dokument eine Skizzengruppe, einen Text mit Höhenformel oder `{…}`-Verweis, ein Drehprofil oder eine Isokurve enthält (Muster `has_format_21_data`; ein gemeinsamer Sprung). Kopieren/Einfügen und DXF ändern die Datei nicht. Alte Dateien bleiben bytegleich, ein Dokument ohne das Neue bleibt bei seinem Format. Ältere Programme lehnen 22 ab wie bisher.

### Texte und Tasten

Alle neuen Texte über `tr!`/`msg!` in de und en, Namen wie in der deutschen/englischen Fusion („Kopieren“/„Copy“, „Ausschneiden“/„Cut“, „Einfügen“/„Paste“, „Als DXF speichern“/„Save As DXF“, „Gruppe erstellen“/„Create Group“, „Gruppe auflösen“/„Ungroup“, „Drehprofil“/„Spun Profile“, „Isoparametrische Kurve“/„Isoparametric Curve“; deutsche Namen aus der Hilfe nicht gegengeprüft = **Annahme**). Strg+C/X/V vorbelegt (Fusion-Kürzel), alles andere belegbar ohne Vorbelegung (Fusion hat keine).

### Entscheidungen nach dem Gegenlesen (Claudes Entscheid)

1. **Format 22 gehört 11b.** Der Zweig `feature/schaltplan` nutzt intern ebenfalls 22 (`FORMAT_E2`, `convert_eda.rs:39`), kommt aber nach Dennis' Regel erst nach allen übrigen Fusion-Funktionen in master. 11b nimmt 22; der Schaltplan-Zweig muss beim späteren Einpflegen auf die nächste freie Nummer gehen (Eintrag in `offene-punkte.md` und Vault-Notiz). Der Sprung kommt im Baustein „Format 22 und Gruppen“ zuerst; Text, Drehprofil und Isokurve setzen darauf auf.
2. **Texthöhe als Formel** nutzt denselben Weg wie Formeln an Skizzenmaßen (Formeltabelle im Feature-Crate, Auswertung vor dem Lösen, Warnung am Skizzen-Feature); der Text speichert weiter den Zahlenwert `height`, die Formel steht in der Nebentabelle. `{Name}` wird ebenfalls im Feature-Crate ersetzt (der Text hält den Rohinhalt, die Kurven entstehen aus dem ersetzten Inhalt).
3. **Einfügen mit Maßformeln, deren Parameter im Ziel fehlen:** Die Zwischenablage hält zu jeder Formel auch den zuletzt berechneten Wert. Fehlt ein Parameter, wird das Maß mit diesem Wert ohne Formel eingefügt; die Meldung nennt die Zahl solcher Maße.
4. **Isokurve, Wert:** Gespeichert wird der **Anteil** 0–1 bezogen auf den Parameterbereich der Berandung der Fläche (Hüllrechteck der Berandung in (u, v); bei geschlossenen Richtungen der volle Umlauf). So bleibt die Kurve bei Flächenänderung an derselben verhältnismäßigen Stelle. Der Punkt setzt beim Wählen den Anteil.
5. **Einfügen + Verschieben = ein Rückgängig-Schritt** „Einfügen“: Bestätigt man das Verschieben, wird es mit dem Einfügen zusammengefasst; bricht man es ab, bleibt das Einfügen an der Quelllage als dieser eine Schritt. (Bewusste, kleine Abweichung von „Abbruch hinterlässt nichts“: der Abbruch betrifft nur das Verschieben.)
6. **Drehprofil bei gemischten Körpern:** ganz oder gar nicht. Ein Körper geht nur, wenn jede Fläche eine Ebene senkrecht zur Achse oder eine zur Achse koaxiale Zylinder-, Kegel-, Kugel- oder Torusfläche ist (Rahmen-z parallel zur Achse, Ursprung auf der Achse, Toleranz 1e-9 relativ); sonst Meldung, nichts entsteht. „Bestimmte Elemente“ (Kanten, Punkte) gehen immer; gekrümmte Kanten, die nicht exakt abbildbar sind, als Spline innerhalb der Toleranz.
7. **DXF-Text:** Texte als ihre Kurven (Linien, Bögen, Splines der Buchstabenumrisse), nicht als TEXT/MTEXT; das ist für Laser und Fräse gedacht und passt zu Fusions „glatteren Textkurven in DXF“.

## Nicht-Ziele und Anti-Ziele

- Nicht: Körper kopieren/einfügen, Austausch über die Systemzwischenablage, Körper- und Zeitleistengruppen, Gruppen innerhalb einer Skizze, SHX-Schriften, Drehprofil für beliebige Körper (echte Umhüllende), Isokurve „physische Entfernung“, DXF aus 3D-Skizzen.
- Anti-Ziel: Funktionen, die Fusion nicht hat (Zeilenabstand, Umbruch), oder Verhalten, das von Fusion abweicht, ohne Eintrag in `offene-punkte.md`.

## Ansätze

- **A (gewählt): sechs unabhängige Bausteine auf gemeinsamen Grundlagen.** Kopieren nutzt `copy_transformed`/`CopyMap` aus Verschieben/Kopieren, DXF den Blattschreiber, Drehprofil und Isokurve das Projektionsmodell aus Skizzen 6/10/11a (`copies`, schwache Bezüge, Verwaltung verlorener Projektionen). Ein Formatsprung auf 22 für alles Neue. Vorteil: wenig neuer Code, bewährte Wege, Fehlerbilder wie bisher.
- B: Zwischenablage über die Systemzwischenablage (Text/JSON). Verworfen: Fusion tauscht Skizzengeometrie nicht mit anderen Programmen aus, und fremder Inhalt in der Zwischenablage wäre eine neue Eingabequelle mit eigenem Prüfaufwand.
- C: Drehprofil als echte Umhüllende beliebiger Körper sofort. Verworfen für 11b: schwere Rechnung, kein belegtes Fusion-Verhalten für Grenzfälle; der erste Schnitt deckt den Hauptnutzen (Drehteile) ab.

## Aufteilung (Vorschlag für den Plan)

1. Kopieren/Ausschneiden/Einfügen (Modell: Zwischenablage-Werte, Einfügen mit neuer Abbildung; App: Tasten, Fokus, Menü, Übergang in Verschieben/Kopieren).
2. Skizze als DXF (Skizze → `Out2d`, Einheiten, Dialog, Browser-Menü).
3. Format 22 und Skizzengruppen (Datenmodell im Dokument, Browser-Baum, Ziehen, Auge, Menü).
4. Text-Reste (Höhenformel, `{Name}`, Drehgriff, Format-22-Felder).
5. Drehprofil (Rechnung im Feature-Crate, `CopySource::SpunProfile`, Dialog).
6. Isoparametrische Kurve (Rechnung je Flächenart mit Beschnitt, `CopySource::Isoparametric`, Dialog, Vorschau).
7. Abschluss: Doku (`docs/funktionen.md`, Wiki-Vermerk, CHANGELOG), `offene-punkte.md`, Gegenüberstellung Fusion im Vault.

## Prüfung

- Tests je Baustein (Modell, Feature-Rechnung, App-Zustand, Dateiformat: bytegleich ohne Neues, 22 mit Neuem, Ablehnung in altem Lesepfad).
- DXF: Rundlauf Schreiben → eigener Leser, Einheiten mm/Zoll.
- Lokal voll auf Windows und WSL (fmt je Crate, clippy `-D warnings`, alle Tests, `cargo deny check`, Messtests im Release). Kein CI vor dem Merge.
- Handtests bei 1920×1080 mit `tools/gui-drive.ps1`, jeweils nur nach Dennis' Go: nach Kopieren/DXF/Gruppen, nach Text/Drehprofil/Isokurve, am Ende. DXF einmal in Fusion 360 öffnen (Dennis' Hand).

## Dem Bauenden überlassen

Namen im Code, Ablage der neuen Module, die Speicherform der Zwischenablage, die genaue Ebenenbenennung im DXF, die Darstellung beim Ziehen im Browser, die Toleranzvorgabe beim Drehprofil, die Stückelung der Isokurve an Nähten geschlossener Flächen.
