# Rundungen und Fasen – Entwurf (Teilprojekt 8)

Stand: 2026-10-13. Baut auf den Specs zu Fundament/Kernel, Booleschen Operationen, Skizzen, Feature-Baum, Speichern/Laden und NURBS auf (insbesondere NURBS §13 „Änderungen bei der Umsetzung“ und die dort genannten Grenzen). Ein eigener Mesh-Task (Vernetzung, gemergt 2026-10-12) ist Voraussetzung dafür, dass die Ergebnisse sauber angezeigt werden.

## 1. Ziel und Umfang

Der Nutzer kann Kanten eines Körpers **runden** und **fasen**:

- **Rundung** mit konstantem Radius oder mit **variablem Radius** (Radien an Punkten entlang der Kante),
- **Fase** mit gleichem Abstand, mit zwei Abständen oder mit Abstand und Winkel,
- an Kanten zwischen **allen Flächenarten**: Ebene, Zylinder, Kegel, Kugel, Torus und Freiformflächen (NURBS),
- konvexe und konkave Kanten,
- mehrere Kanten in einem Feature, **Kantenketten** mit abschaltbarer **Tangentenkette**,
- Ecken, an denen mehrere gerundete Kanten zusammenlaufen, im Stil **rollende Kugel**,
- als Feature im Feature-Baum (Zeitleiste), mit stabilen Kantenverweisen, Neuberechnung, Unterdrücken, Speichern und Laden, und mit einem Fenster in der App (Menü „Bearbeiten“).

**Nicht Teil dieses Teilprojekts:** Ecken-Stile *Setback* und *glatte Ecke* (Nutzerentscheidung: später), Gesichtsrundung/Gesichtsfase und Regelrundungen mit Kurvenführung, Rundung ganzer Flächen (Face-Fillet), Rundung an Kanten von Körpern mit mehreren Schalen, Loft und Sweep, Export, Kontrollpunkte-Bearbeitung.

## 2. Entscheidungen

| Nr. | Entscheidung | Begründung |
|---|---|---|
| R1 | Alles in einem Teilprojekt, gebaut in vier Stufen (§9), jede mit eigenem Abschlussreview | Nutzerentscheidung (alles in einem Zug); die Stufen beherrschen das Risiko |
| R2 | Verfahren **rollende Kugel**: Die Mittelpunkte einer Kugel mit Radius r (variabel r(t)), die beide Nachbarflächen berührt, bilden die Bahn; ihre Berührungspunkte bilden zwei Kurven; dazwischen liegt die Rundungsfläche aus Kreisbögen | Standardverfahren der Industrie-CAD; trägt später Setback/glatte Ecken und variablen Radius |
| R3 | Bahn per Newton-Verfahren mit der vorhandenen Abstandsrechnung (`project`/`closest`/`normal`), **keine Versatzfläche als Objekt** | Spart die Versatzfläche (NURBS §13 hatte sie ausdrücklich ausgeklammert); die Bahn braucht nur Abstände |
| R4 | **Analytische Rundungsflächen, wo exakt möglich** (konstanter Radius: Ebene–Ebene → Zylinder, Ebene–Zylinder koaxial → Torus, Ebene–Kegel → Kegel/Torus usw.), sonst NURBS-Fläche aus rationalen Kreisbögen („Hautfläche“) | Exakt, schnell, bekannte Wege im Kernel; NURBS nur wo nötig |
| R5 | Fase = dieselbe Rechnung mit einer **Geraden** statt des Kreisbogens (Regelfläche); „zwei Abstände“ und „Abstand und Winkel“ ändern nur, wo die Berührungspunkte liegen | Ein gemeinsamer Weg für beide Arten |
| R6 | **Umbau über Stücke und `sew`**: beschnittene Nachbarflächen, Rundungsflächen und Eckstücke werden als `Piece` gesammelt und mit dem vorhandenen Zusammensetzer zu einem neuen Körper verschweißt; danach `validate_with` und Volumenprobe | Der `Solid` kann nichts entfernen (nur anhängen); `sew` ist erprobt und behält Marken; keine zweite Topologie-Maschine |
| R7 | Ecken: nur **rollende Kugel**; einfachster Fall zuerst (drei rechtwinklige Kanten, gleicher Radius), alles Weitere zunächst **benannt abgelehnt**, spätere Stufen lockern | Ecken sind die schwierigste Stelle; Nutzerentscheidung „nur Standard, Stile später“ |
| R8 | **Tangentenkette** standardmäßig an (abschaltbar): Beim Wählen einer Kante werden glatt anschließende Kanten mitgewählt und als eine Kette gerechnet | Nutzerentscheidung; sonst bricht die Rundung an Übergängen ab |
| R9 | Variabler Radius über **Punkte entlang der Kante** (Anfang, Ende und beliebig viele Zwischenpunkte: Position in Prozent der Kantenlänge, Radius), dazwischen glatt (kubisch, ohne Überschwingen) | Nutzerentscheidung (wie Fusion) |
| R10 | Nicht lösbare Fälle werden **benannt abgelehnt**, auf Deutsch, mit dem Featurenamen; das Dokument bleibt unverändert; Folgefeatures `Skipped` | Wie bei allen bisherigen Features |
| R11 | Dateiformat wird **2** (neue Golden-Datei); Format 1 bleibt lesbar; ältere Programme melden „zu neu“ | Neue Feature-Art in der Zeitleiste; klare Meldung statt Fehler |
| R12 | Keine neue externe Abhängigkeit; die Rechnung steckt in `planora-math` (Hautfläche, Spline der Radien) und `planora-kernel` (Ordner `blend/`) | Lizenzregel (GPL-3.0-only, `cargo deny` bleibt grün) |

## 3. Geometrie

### 3.1 Bahn und Berührungskurven

Für eine Kante `e` zwischen den Flächen `A` und `B` und einen Parameter `t` wird der Kugelmittelpunkt `c(t)` gesucht mit

`dist(c, A) = r(t)`, `dist(c, B) = r(t)` und `c` in der Ebene senkrecht zur Kante durch `e(t)`.

Die Newton-Iteration startet an der Kante (Mittelpunkt in Richtung der Winkelhalbierenden der Normalen) und folgt Schritt für Schritt. Die Fußpunkte `pA(t)`, `pB(t)` auf den Flächen (mit `project`/`project_near`) sind die Berührungspunkte. Daraus entstehen zwei Kurven; analytische Flächen geben exakte Kurven, sonst werden sie mit `NurbsCurve::fit` (Schranke `NURBS_FIT_TOL`) angenähert und gegen beide Flächen geprüft (wie im Schnittrechner).

Konkave Kanten: gleiche Rechnung mit umgekehrtem Vorzeichen des Versatzes; das Material wächst. Konvex/konkav folgt aus den Flächennormalen an einem Kantenpunkt (mit `same_sense`).

### 3.2 Rundungsfläche

- **Analytisch** (R4) bei konstantem Radius und passenden Flächenpaaren: Flächenart, Frame und Parameterbereich ergeben sich geschlossen; die Berührungskurven sind ebenfalls analytisch (Kreise, Linien).
- **NURBS** sonst: Jeder Querschnitt ist der Kreisbogen um `c(t)` von `pA(t)` nach `pB(t)` (rationales quadratisches Segment, exakt). Entlang der Bahn werden die Querschnitte zu einer NURBS-Fläche verbunden („Hautfläche“, neuer Baustein in `planora-math`: gemeinsame Knotenfolge, rationales Mischen der Querschnitte, `NurbsSurface::new` prüft). Bei variablem Radius ändert sich nur `r(t)` entlang der Bahn.
- **Fase**: Querschnitt = Gerade `pA(t)`–`pB(t)`; Regelfläche (bilineare Hautfläche oder, bei Ebenen, eine Ebene).

### 3.3 Radien und Maße

- Variabler Radius: Kubischer Spline durch die Punkte (Radius gegen Position in Prozent der Kantenlänge), monoton zwischen den Punkten (kein Überschwingen unter 0). Alle Radien > 0, endlich, ≤ `MAX_COORD_MM`.
- Fase „zwei Abstände“: ein Abstand je Seite; die Seite wird über die Reihenfolge der beiden Flächen im Kantenverweis festgelegt und im Fenster als „Fläche 1/Fläche 2“ angezeigt. „Abstand und Winkel“: Abstand auf Fläche 1, Winkel zur Fläche 1.

### 3.4 Benannte Ablehnungen (Auswahl)

Radius oder Abstand größer als die Nachbarfläche erlaubt; Bahn bricht ab (Kugel findet keine Lage); Krümmungsradius einer Fläche kleiner als der Rundungsradius auf der konvexen Seite; fast tangentiale Nachbarflächen (Kantenwinkel nahe 0° oder 180°); Kante an einer Naht oder einem Pol, die die Kugel nicht sicher verfolgen kann; nicht endliche oder zu große Eingaben; Kanten, an denen eine Schleife nach der Rundung kreuzt (Selbstdurchdringung); Überschreiten der Arbeitsgrenze (wie `MAX_FREEFORM_EVALUATIONS`).

## 4. Topologie und Ecken

### 4.1 Einzelne Kante

- Beide Nachbarflächen werden entlang ihrer Berührungskurve beschnitten (Schleifen neu aufgebaut, `Seg`/`Piece`).
- Die alte Kante entfällt. Die Rundungsfläche hat vier Ränder: zwei Berührungskurven und zwei Abschlusskanten.
- **Enden:** Läuft die Kante in eine dritte Fläche, wird die Rundungsfläche dort mit dieser Fläche geschnitten (vorhandener Schnittrechner); der Abschluss ist diese Schnittkurve. Endet die Kante an einer anderen gerundeten Kante, gilt die Eckenregel (§4.3).

### 4.2 Ketten

Gewählte Kanten werden zu Ketten gruppiert; Folgekanten, die glatt (tangential) anschließen, kommen bei eingeschalteter Tangentenkette dazu. Entlang einer glatten Kette läuft die Kugel in einem Zug; die Rundungsfläche ist je glattem Abschnitt eine Fläche. Wechselt der Radius an einer Kettenecke, wird das benannt abgelehnt (Stufe 2), später gelockert.

### 4.3 Ecken (Stil rollende Kugel)

- Zwei gerundete Kanten, die an einer Ecke zusammentreffen, werden in einem Zug durch den Eckpunkt geführt (die Bahn der Kugel geht dort weiter).
- Drei Kanten an einer Ecke mit gleichem Radius und rechtwinkligen Flächen: Kugelstück (Achteloberfläche) als eigene Fläche zwischen den drei Rundungen.
- Alle weiteren Eckenfälle (ungleiche Radien, schräge Ecken, mehr als drei Kanten, Ecken auf Freiformflächen) sind in Stufe 2 benannt abgelehnt; Stufe 3 lockert, was sich sicher berechnen lässt. Setback und glatte Ecke: nicht in diesem Teilprojekt.

### 4.4 Marken und Verweise

Jede neue Fläche bekommt eine Marke (`origin` = Feature-Nummer nach `assign_origin`, `role` zum Beispiel „Rundung N“ oder „Fase N“, mit N nach der Kantenreihenfolge der Auswahl); Eckenstücke „Ecke N“. Beschnittene Nachbarflächen behalten ihre Marke (Hinweispunkt entscheidet, wie bei den Booleschen Operationen). Die neuen Berührungs- und Abschlusskanten sind über die Marken ihrer zwei Nachbarflächen wieder wählbar (`EdgeRef`).

### 4.5 Zusammenbau und Prüfung

Alle Stücke gehen als `Piece` in `sew` (Knotentoleranz wie bisher, mit der größeren Toleranz bei NURBS: `pipeline_tol`). Danach `validate_with`, dazu eine **Volumenprobe**: konvexe Kanten dürfen nur Material abtragen, konkave nur anfügen; die Volumenänderung muss zum Radius und zur Kantenlänge passen (grobe Ober-/Untergrenze, benannte Meldung bei Verletzung). Bei Freiformflächen gilt das gemeinsame Netz der Volumenprobe aus der Abschlussnachbesserung von Teilprojekt 7.

## 5. Feature und Zeitleiste (`planora-feature`)

- Neue Art `FeatureKind::Verrunden { body, edges: Vec<EdgeRef>, art, tangentenkette }`, mit `art`:
  - `Rundung { radius }`,
  - `RundungVariabel { punkte: Vec<(f64 Prozent, f64 Radius)> }` (je Kantenkette),
  - `FaseGleich { abstand }`, `FaseZwei { abstand1, abstand2 }`, `FaseWinkel { abstand, winkel }`.
- Verhält sich wie Boolesche Operation/Verschieben: ersetzt seinen Eingangskörper in der Zeitleiste, hängt von dessen Feature und den Features seiner Kantenverweise ab, lässt sich unterdrücken, löschen (mit Abhängigen), umbenennen. Änderungen an Parametern berechnen ab diesem Feature neu.
- `compute`: Kantenverweise auflösen (`resolve_edge` über die sichtbaren Körper), Parameter prüfen (`check_parameters`: Bereiche, endlich), Kernel-Operation, `validate`, `assign_origin`, Volumenprobe. Fehler werden auf Deutsch gemeldet (eigene Zuordnung `blend_error`, Freiformgrenzen mit dem bekannten Hinweis).
- Alle erschöpfenden `match`-Stellen über `FeatureKind` werden ergänzt (Beschriftung, Abhängigkeiten, Prüfung, Berechnung, Sichtbarkeit, Eigenschaftenzeilen, Fenster-Zahlen, Browser, Umwandlung für die Datei).

## 6. Oberfläche (`planora-app`, `planora-render`)

- Menü „Bearbeiten“: **Verrunden …** und **Fase …** (aktiv, wenn mindestens ein Körper existiert). Das Fenster folgt dem Muster der Booleschen Operation/Extrusion: Körper wählen (falls mehrere), **Kanten im Fenster anklicken** (Umschalt: mehrere; ein erneuter Klick nimmt zurück; bei eingeschalteter Tangentenkette werden die Folgekanten mitgewählt und mit hervorgehoben), Maße eintragen, Art wählen, „Erzeugen“/„Abbrechen“.
- Variabler Radius: Liste der Punkte (Position in %, Radius) mit „Punkt hinzufügen/entfernen“.
- **Neu:** Mehrfachauswahl von Kanten im Zustand der App und Hervorhebung mehrerer Kanten in der Szene (`planora-render`).
- Das Feature lässt sich im Eigenschaftenfenster nachträglich ändern (Radius, Punkte, Art), wie bei den anderen Features (Versions-Prüfung gegen veraltete Fenster).
- Alle Texte deutsch; Fehler erscheinen in der Statuszeile und am Feature im Baum.

## 7. Datei

- `FORMAT_VERSION = 2`. Neue Zeitleisten-Art `Verrunden` mit deutschen Feldnamen (`koerper`, `kanten`, `art`, …) im Stil der übrigen Arten; Kantenverweise wie bei Skizzen-Kantenkopien.
- Format 1 bleibt lesbar; die Golden-Dateien für Format 1 bleiben unverändert; eine neue Golden-Datei für Format 2 enthält Rundung, variable Rundung und Fase. `decode_version` meldet bei höherer Version weiter „zu neu“.
- Feldprüfungen (`num`, endlich, Bereiche) auch für die neuen Felder; Hostile-Dateien (NaN, riesige Zahlen, leere Kantenliste, negative Radien) werden beim Laden abgelehnt oder beim Berechnen benannt gemeldet.

## 8. Tests

- **Mathematik:** Hautfläche gegen exakte Fälle (Zylinder, Torus aus Kreisbögen), Spline der Radien (monoton, durch die Punkte).
- **Kern, Stufe 1:** Fase und Rundung an Quader, Zylinder, Kegel, Kugel, Extrusion; Volumen gegen die geschlossene Formel; `validate`; geschlossenes Netz; Folgeoperationen (Boolesche Operation, zweite Rundung auf dem Ergebnis); konkave Kanten; Grenzfälle (Radius zu groß, fast tangential).
- **Stufe 2:** Ketten, Tangentenkette, Ecken (Quader mit drei gerundeten Kanten an einer Ecke gegen die Kugelformel).
- **Stufe 3:** Variable Radien (Volumen gegen numerisches Integral), Freiformkanten (Ellipsenextrusion, Spline), analytische Zwillinge, wo es sie gibt.
- **Zufallstests** (festgelegte Startwerte, nur Release für die großen Serien): Rundung/Fase an zufälligen Kanten zufälliger Körper; jedes Ergebnis ist ein gültiger Körper mit plausiblem Volumen und geschlossenem Netz oder eine benannte Ablehnung, nie ein falscher Körper.
- **Feature/App/Projekt:** wie bei Teilprojekt 7 (Zeitleiste, Neuberechnung mit Zwischenspeicher, Mehrfachauswahl, Fenster ohne Bildschirm, Speichern/Laden, Golden-Datei Format 2, feindliche Dateien).
- **Handprüfung** am Ende, nach Rückfrage beim Nutzer, mit dem Skript `tools/gui-drive.ps1` und temporärem Ordner.

## 9. Stufen

1. **Stufe 1 – Fase und konstanter Radius, einzelne Kanten, analytische Rundungsflächen.** Bahn per Newton (zunächst für Ebene–Ebene, Ebene–Zylinder/Kegel/Kugel), Berührungskurven, Rundungs- und Fasenflächen, Enden an Nachbarflächen, Umbau über `sew`, Validierung und Volumenprobe; Kernel-Operation, Feature `Verrunden`, einfaches Fenster (eine Kante). Review.
2. **Stufe 2 – Ketten, Tangentenkette, Mehrfachauswahl, Ecken (rollende Kugel).** Mehrfachauswahl in App und Szene. Review.
3. **Stufe 3 – Freiformkanten und variabler Radius.** Hautfläche (NURBS) im Kern, variabler Radius mit Punktliste, alle Flächenpaare, Lockerung der Eckenfälle. Review.
4. **Stufe 4 – Datei (Format 2), Eigenschaftenfenster, Dokumentation, Gesamtprüfung, Handprüfung.** Abschlussreview des ganzen Zweigs.

## 10. Risiken

- **Ecken** sind das Hauptrisiko; deshalb Eingrenzung (R7) und benannte Ablehnung statt Raten.
- **Freiformflächen** mit Rundung und anschließender Boolescher Operation erreichen schnell die Rechengrenzen (NURBS §13); dann kommt die bekannte Meldung „zu aufwendig“.
- **Bahnrechnung** nahe Nähten, Polen und sehr flachen Kantenwinkeln: benannte Ablehnung mit Prüfung gegen beide Flächen (wie die Prüfung der Schnittkurven).
- **Aufwand** ähnlich groß wie NURBS (viele Prüfrunden); Grenzen, die nicht beseitigt werden, stehen am Ende offen in `offene-punkte.md`.

## 11. Lizenzen

Keine neue Abhängigkeit; `cargo deny` bleibt grün (GPL-3.0-only, keine Erweiterung der Erlaubnisliste). Kein Autodesk-Material; Verfahren sind in der Fachliteratur und Lehrbüchern öffentlich (rollende Kugel/Canal-Flächen); der Code ist eigene Arbeit.
