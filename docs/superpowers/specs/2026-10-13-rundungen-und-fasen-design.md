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

## 12. Änderungen bei der Umsetzung

Stand: 2026-10-14. Der Plan (`docs/superpowers/plans/2026-10-13-rundungen-und-fasen.md`) hat die Spec mit den Entscheidungen P1 bis P14 konkretisiert; die Tabelle nennt jeweils das Ergebnis aus der Umsetzung. Danach folgen die übrigen Abweichungen von §1 bis §11, die Befunde der Stufenreviews und die bekannten Grenzen.

### 12.1 Entscheidungen P1 bis P14

| Nr. | Entscheidung | Begründung und Ergebnis |
|---|---|---|
| P1 | Kernel-Schnittstelle `Kernel::blend(&Solid, &BlendRequest) -> Result<Blended, BlendError>` und freie Funktion `blend::blend(solid, request, tol)`. `BlendRequest { edges, kind, tangent_chain }`, `BlendEdge { edge, first_face }`, `Blended { solid, volume_change, edges }`. | Umgesetzt. `Blended` bekam in der Stufe-2-Nacharbeit zusätzlich `volume_scale` (Bruttomenge, siehe P2). |
| P2 | Volumenprobe: Der Kernel kann nicht vernetzen (`planora-mesh` hängt vom Kernel ab). Er liefert eine Schätzung (`volume_change`, Querschnittsfläche mal Weg, Ecken mit geschlossenen Formeln); `planora-feature` vernetzt Eingang und Ergebnis mit gemeinsamer Teilung und prüft mit `planora_mesh::blend_volume_plausible`. Meldung `FeatureError::ImplausibleBlend`. | Umgesetzt mit zwei Änderungen aus dem Review der Stufe 2: (1) Die Schätzung ist netto; konvexe und konkave Anteile heben sich auf, deshalb lehnte die Probe richtige Fälle ab. Der Kernel liefert jetzt die Bruttomenge `volume_scale`, die Probe nimmt `BLEND_ABS_SLACK` hinzu. (2) Bei gemischten Rundungen gilt die engere Schranke `BLEND_REL_MIXED = 0,2` (statt `BLEND_REL = 0,5`), sonst rutschten falsche Körper durch (fehlende Art 342 von 600, vertauschte Art 148 von 300). Rest: Fehlt in gemischten Fällen nur die kleinere Art (bis etwa 20 % der Bruttomenge), geht die Probe durch; die geometrischen Prüfungen im Kernel sind die eigentliche Sicherheit. `BLEND_REL` und `BLEND_SLACK_FACTOR` sind gemeinsame benannte Konstanten in `planora-mesh/src/volume.rs`. Die Schätzung der Gehrung ist etwas zu klein (-4,08 statt -4,20), bei nur teilweise gerundeten Ecken liegt sie 1 bis 3 % daneben; beides im Spielraum. |
| P3 | Eine Rundungsfläche je Kante (Zylinder oder Torus), benachbarte teilen den Querschnitt als gemeinsame Kante. | Umgesetzt. Bei Freiformketten ebenso (je Kante eine Hautfläche). |
| P4 | Fasenmaße: Abstand = Sehnenabstand Kantenpunkt zu Berührpunkt im Querschnitt; Fläche 1 = Fläche der Marke `EdgeRef.a` (`BlendEdge::first_face`); Winkel im Feature in Grad, im Kernel im Bogenmaß, erlaubt 1° bis 179°. | Umgesetzt. Nacharbeit nach dem Review der Stufe 1: Bei gekrümmter Fläche 1 war der Fasenwinkel falsch gemessen (die Richtung im Querschnitt war keine Drehung); jetzt Drehung um die Kante, Winkel gegen die Tangente am Berührpunkt. Freie Fasen (Hautfläche) messen ab der wahren Ecke (`true_corner`). Der Dialog erlaubt nur 1° bis 179° (Meldung „zwischen 1° und 179°“). |
| P5 | Endabschlüsse: Analytische Rundungsflächen schließen an der dritten Fläche über `intersect_surfaces` ab; nicht unterstützte Paare werden benannt abgelehnt. Hautflächen schließen nur an Ebenen senkrecht zur Kante, an Nähten, glatten Übergängen und Ecken ab. | Umgesetzt. Hinzu kam für Kugelecken ein Suchfenster `cut_within` (Aufgabe 17): Ohne dieses Fenster lehnte die Zufallsserie 53 von 300 Fällen an spitzeren Ecken fälschlich ab. Ein Test (`an_end_face_that_cannot_be_cut_is_named`) nutzt Radius 0,5 statt 1, weil Radius 1 schon früher (Achsenprüfung, Aufgabe 3) abgelehnt wird. |
| P6 | Ecken: Zwei gerundete Kanten an einer Ecke (dritte ungerundet) treffen sich an der winkelhalbierenden Ebene (Gehrung); das Ergebnis ist die Schnittmenge der einzeln gerundeten Körper. Drei Kanten mit gleichem Radius: Kugelstück; Stufe 2 nur rechtwinklig, Stufe 3 beliebige drei Ebenen. Alles andere benannt abgelehnt. | Umgesetzt. Die Kugelecke gilt in Aufgabe 17 für beliebige drei Ebenen; das Eckvolumen hat eine geschlossene Formel (Aufgabe 17, Review: die erste Formel war nur für rechte Winkel richtig und lehnte bei 140°, 157° und 20° fälschlich ab; ersetzt durch die exakte Formel über Kantenwinkel und Raumwinkel). Gehrung nur bei spiegelgleichen Seiten. Abgelehnt: variabler Radius oder Freiform an Ecken (`VARIABLE_CORNER`), nach innen springende und gemischte Ecken, drei Fasen an einer Ecke, nicht spiegelgleiche Paare an schiefen Ecken, sehr spitze Gehrungen. Blend-gegen-Blend-Durchstoß wird nicht eigens geprüft (kein Gegenbeispiel gefunden). |
| P7 | Dateiformat: `FORMAT_VERSION = 2` ist die höchste lesbare Version; Versionsdateien werden mit der kleinsten passenden Version geschrieben (1 ohne `Verrunden`, 2 mit); `dokument.json` und `.autosave.json` behalten `format: 1`. Der Test `newer_files_with_changed_structure_report_too_new` nutzt `3`. | Umgesetzt wie geplant (Aufgabe 18, nur diese mechanische Testanpassung). Eine Versionsdatei mit `format: 1` und einer Rundung ist beschädigt. Feindliche Dateien werden benannt abgelehnt. |
| P8 | Variabler Radius: mindestens 2 und höchstens 64 Punkte, Positionen in Prozent streng steigend in [0, 100], monotone kubische Hermite-Interpolation (Fritsch-Butland), außerhalb konstant. | Umgesetzt als `planora_math::RadiusLaw`. Nacharbeit in Aufgabe 14: Erster Punkt über 0 % oder letzter unter 100 % wurde fälschlich abgelehnt (Knick am inneren Ende); die Startschätzung des Newton-Verfahrens wurde für steile Abfälle verschoben (`shifted_start`) und an Knicken werden einseitige Tangenten benutzt (`RadiusLaw::kinks`, `skin_sided`). |
| P9 | Rollen der neuen Flächen: „Rundung N“ bzw. „Fase N“, Eckstücke „Ecke N“ nach Eckpunkt-Id. | Umgesetzt. N beginnt in jedem Schritt bei 1 (bewusst). |
| P10 | Parameterbereiche: Beschnittene Flächen behalten ihren `patch` und werden nur in nicht geschlossenen Richtungen erweitert; neue Flächen bekommen den Bereich ihrer Schleife. | Umgesetzt. |
| P11 | Meldungen am Feature nennen den Featurenamen nicht selbst; bei Freiformursachen beginnt sie mit „Bei Körpern mit Freiformflächen:“. | Umgesetzt. Einzelne Kernmeldungen schreiben Dezimalzahlen mit Punkt statt Komma und ein Mindestmaß mit vielen Stellen (kosmetisch). |
| P12 | Übergänge: Stufe 1 lehnt mehrere Kanten und variable Radien benannt ab; die Übergangstests werden in Aufgabe 9 und 14 ersetzt. | Umgesetzt. In Aufgabe 15 wurde die Annahme `VARIABLE_FREEFORM` in einem bestehenden Test aufgehoben (wie vorgesehen). |
| P13 | `KindDto::Verrunden` und die Umwandlung schon in Aufgabe 7; Formatnummer, Golden-Datei, feindliche Dateien in Aufgabe 18. | Umgesetzt. |
| P14 | Ein gerundeter Körper behält den Namen seines Eingangskörpers; das Feature heißt „Rundung n“ bzw. „Fase n“. | Umgesetzt. |

### 12.2 Weitere Abweichungen von §1 bis §11

- **Hautfläche (§3.3):** Statt eines natürlichen Splines durch die Querschnitte interpoliert die Hautfläche (`planora_math::nurbs::skin`) mit kubischem Hermite (Aufgabe 13); die Rundungsfläche aus rationalen Kreisbögen bleibt, die Gewichte werden entlang der Bahn mit angepasst. Der Abstand zu den exakten Querschnitten wird nicht in der Fläche geprüft, sondern vom Aufrufer (Aufgabe 14: Stichprobe 3 je Lücke und 5 quer, gegen die halbe NURBS-Anpassungstoleranz).
- **Berührkurven (§3.1):** Statt `NurbsCurve::fit` durch die Berührpunkte werden die Randkurven der Hautfläche benutzt (`v_boundary`).
- **Querschnitte (§3.1):** Die Querschnitte stehen senkrecht auf der wahren Schnittrichtung der beiden Nachbarflächen (n_A × n_B), nicht senkrecht auf der gespeicherten Kantenkurve. Dadurch sind sie über die Naht stetig (Nahtabweichung 1,2 bis 2,6 · 10⁻⁶ statt zuvor 3,8 · 10⁻⁴, Aufgabe 15). Die Fläche folgt also den Flächen, nicht der gespeicherten Kurve; an der Naht werden die Normalen in den ersten 5 % der Kantenlänge überblendet (bis etwa 5 · 10⁻⁶ rad Abweichung gegenüber §3.1).
- **Volumenprobe (§4.5):** im Feature, nicht im Kernel (P2).
- **Ecken (§4.4, R7):** Gehrung und Kugelecke nach P6 und Aufgabe 17; Setback und glatte Ecke entfallen wie in §1.
- **Abschlüsse (§4.3):** nach P5; Hautflächen schließen nur an Ebenen senkrecht zur Kante ab.
- **Mehrfachauswahl (§6):** Im Fenster gibt es keinen Körper-Wähler; der Körper folgt aus der angeklickten Kante. Umschalt-Klick fügt Kanten hinzu, ein zweiter Klick nimmt sie heraus; Esc löscht die Auswahl. Die Marken folgen nach einer Neuberechnung den neuen Kantennummern (Review Aufgabe 10).
- **Mehrere Hüllen (§1):** Körper mit Hohlräumen oder mehreren Schalen werden mit benannter Meldung (`CAVITY`) abgelehnt, auch wenn ein Hohlraum weit entfernt wäre (Review Aufgabe 4: eine innere Hülle in oder quer durch den entfernten Bereich ergab einen falschen Körper).
- **Mindestmaß (§10, R10):** 100 · `blend_tol` des Körpers (1e-4 mm analytisch, 2e-3 mm bei Freiformflächen oder variablem Radius, weil `sew` darunter nicht mehr dichtet); Fasenwinkel 1° bis 179°.
- **Tangentenkette (R8):** In Stufe 1 galt „teilen genau eine Fläche“ als Kettenbedingung, ab Stufe 2 die volle Kettenverfolgung (Obergrenze 257 Kanten). Kanten mit etwa 180° zwischen nicht verschmolzenen Flächen (nach Vereinigung) werden übersprungen.
- **Konvexität (Stufe 2, Review):** `is_flat` bei Kantenmessern korrigiert.
- **Rechenbudget:** Grenzen zählen Schritte und NURBS-Auswertungen (`check_budget`), nie Zeit.
- **Fasen an Freiformkanten:** die Prüfkonstanten (`SKIN_TOL` 5e-6, `CONTACT_TOL`) blieben unverändert; die Prüfstichprobe der Fase wurde verdichtet.
- **Kettenlänge (§3.2):** Variabler Radius an Ketten aus Kreisbögen benutzt die Länge der Kernel-Polylinie (48 Sehnen je Kreis); die Radien liegen bis zu 7 · 10⁻⁴ gestaucht. Bei Freiformketten wird die Länge über die Gauß-Bogenlänge bestimmt (Aufgabe 15, Nacharbeit 3).
- **Doppelte Kanten:** Doppelt gewählte Kanten ergeben kein Fehlerergebnis, sondern werden einmal gerechnet (statt der Meldung im Review-Schwerpunkt 3); 257 doppelte Verweise werden abgelehnt.

### 12.3 Befunde der Stufenreviews

- **Stufe 1** (400 Zufallsdokumente, 1446 Fasenprüfungen, Dateien, Freiform): Kritisch C1: Eine Fase mit einer Abschlussfläche, die die Kantenfläche tangential berührt, lieferte einen falschen Körper (22 % Volumenverlust), Ursache war die Wurzelwahl in `end_param`. I1: Fasenwinkel an gekrümmter Fläche falsch. I2: `tangentenkette = true` wurde in Stufe 1 still ignoriert. I3: Mindestmaß 1e-4 mm gegenüber der Freiformschweißtoleranz von 2e-4. Alle behoben (0940cbc), danach 0 falsche Körper in 400 Dokumenten und 1743 gezielten Fällen. Zuvor im Review von Aufgabe 4 behoben: Innenschleifen angrenzender Flächen und innere Hüllen (siehe oben).
- **Stufe 2** (1931 Dokumente, 627 gerechnete geprüft, 0 falsch): I1 falsche Ablehnung durch die Nettoschätzung (P2), M1 Kette folgt 180°-Kanten, M2 und M3 Testschwächen. Zweite Runde: Die Probe war bei gemischten Rundungen zu weit, `BLEND_REL_MIXED = 0,2` (an 600 + 300 richtigen Fällen validiert, größte Abweichung 0,035 bzw. 0,064). Offen: Fehlt nur die kleinere Art, geht die Probe durch. Aufgabe 11 (Review): Die Kontaktkurve wurde nur an 8 Punkten und Randtreffer wurden gar nicht geprüft; dadurch konnte eine Kerbe zwischen den Stützpunkten einen sich selbst durchdringenden Körper ergeben. Behoben durch exakte Schnitte (7e44b85, danach 600 Fälle: 228 richtig, die 3 früheren falschen Körper abgelehnt). Aufgabe 9: Die Bounding-Box-Vorprüfung in `check_piercing` wurde auf Richtigkeit geprüft (sicher).
- **Stufe 3** (Aufgaben 13 bis 17): Aufgabe 14: falsche Ablehnungen bei Gesetzen mit Anfangsposition über 0 % und steilen Abfällen behoben (Ok-Quote von 92 auf 383 von 400, 0 falsche Körper, 5 Millionen Punktproben ohne Abweichung). Parkbefund: Etwa 1 % der gerechneten variablen Rundungen (5 von etwa 500) lässt sich bei normaler Sehne nicht dicht vernetzen (ebene Fläche mit steiler Freiformkurve als Rand; Geometrie, Gültigkeit und Punktprobe in Ordnung); der Auslöser liegt im geschützten Dreiecksnetz-Code, die App zeigt „Netz ungenau“. Aufgabe 15: ein kritischer Fehler (Naht-Verkippung bei variablem Radius: Körper 3 · 10⁻⁴ mm daneben und trotzdem akzeptiert) wurde durch Querschnitte senkrecht zur wahren Schnittrichtung behoben; zwei vermutete Fehler (Kettenlänge, Fase an Lochrand) bestätigt und behoben, zwei weitere Funde („Stein“-Körper, Fasen 8,8 · 10⁻⁶) waren Fehler der Prüfrechnung. Aufgabe 17: Eckvolumen für nicht rechte Winkel war falsch (siehe P6); tote Prüfungen `SKIN_CORNER` entfernt. Die Stufe-3-Prüfung wurde durch den vollständigen Lauf von Aufgabe 17 ersetzt (1167 bestanden, 27 ignoriert, Gates sauber, Release-Reihen grün); die übergreifende Prüfung des Zweigs folgt im Abschlussreview.
- **Bekannt und akzeptiert:** NURBS-Zylinder lassen Rundungen bis 0,78 R zu, der analytische Zwilling lehnt früher ab. Fasen ohne `check_foot` an Freiformkanten (M4 Aufgabe 15). `blend_limited` mit Grenze = Kosten − 1 gibt noch Ok.

### 12.4 Bekannte Grenzen (technisch)

- **CAVITY:** Körper mit mehr als einer Schale werden für Rundungen und Fasen benannt abgelehnt (P-Ruling Aufgabe 4); `has_cavity` hat keine Ausnahme für weit entfernte Hohlräume.
- **Ecken:** Gehrung nur bei spiegelgleichen Seiten; Kugelecke beliebig für drei Ebenen (exakte Eckvolumenformel); abgelehnt werden `VARIABLE_CORNER` (variabler Radius oder Freiformfläche an einer Ecke), konkave und gemischte Ecken, drei Fasen an einer Ecke, nicht spiegelgleiche zwei Fasen oder Rundungen an schiefen Ecken, Gehrung unter etwa 4,3°. Setback und glatte Ecke fehlen.
- **Boolesche Operationen danach:** werden oft abgelehnt (bestehende Grenzen bei Kegel- und Torusflächen, `boolean/**` unverändert).
- **Mindestmaße:** 1e-4 mm analytisch, 2e-3 mm Freiform oder variabel; Winkel 1° bis 179°.
- **Volumenprobe:** nur „kleinere Art fehlt“ (bis 20 % brutto) geht durch; Schätzung bei teilweise gerundeten Ecken 1 bis 3 % daneben.
- **Netz:** Bei steilen variablen Gesetzen etwa 1 % Lücken (geschützter Netzcode); Freiformvernetzung bei Sehne 1e-3 bis 4 % Volumenfehler (Zylinder 8,4e-3, Torus 2,5e-2 relativ), bei 1e-4 stimmt es; sehr kleine Radien erzeugen ungenaue Netze.
- **Querschnitte** folgen den Flächen, nicht der gespeicherten Kurve (siehe 12.2).
- **Nähte:** Glatt nur bei gleichen Steigungen; analytische Kreisketten mit variablem Radius: Stauchung bis 7 · 10⁻⁴ (48-Sehnen-Konvention); Freiformketten exakt.
- **Inkonsistenz M2:** NURBS-Zylinder lässt r bis 0,78 R zu, der analytische Zwilling lehnt früher ab.
- **Sew-Verweigerungsgebiet** an einigen Lochrändern (Beispiel Loch 2,589 × 1,128, r 0,356 bis 0,435); Ablehnquoten Stufe 1: Quader 9 %, Prisma 20 %, L/Langloch 61 bis 68 %, Zylinder 69 %, Drehkörper 77 %, Kegel 84 %; Stufe 2: Quader 44 %, Prisma 81 %, Löcher 66 %, gestufte Quader 84 %.
- **EdgeRef:** Hinweise werden nach einer Neuberechnung nicht zurückgeschrieben; Fläche 1 wird nach Rollenname gewählt (bei gleichnamigen Flächen wirkungslos); `select_edges` verwirft ein `first_face` einer später gewählten Kante still, wenn sie schon in einer früheren Kette liegt; Mindestmaß-Hinweis erst nach der Kantenwahl.
- **Datei:** Dateien mit Rundung werden als Format 2 geschrieben (ohne Rundung Format 1); alte Programme lehnen sie als zu neu ab; Rahmendateien akzeptieren `"format": 2`.
- **Nicht enthalten:** Setback und glatte Ecken, Loft und Sweep, Export von Rundungen, Rundung ganzer Flächen.
- **Laufzeiten:** `blend_variable` Debug etwa 21 s (Grenze 30 s, auf langsamen Rechnern knapp; Messungen schwankten zwischen 19,8 und 26,4 s); schwere Reihen nur im Release (`#[ignore]`): `blend_stage2` 39,7 s, `blend_stage3` zusammen 425 s, `blend_freeform_volume` 263 s (Debug 24,8 s für den schnellen Teil).
- **Prüfstand am Ende von Aufgabe 20:** 1179 bestanden, 0 fehlgeschlagen, 27 ignoriert in 153 Testprogrammen, Laufzeit etwa 7 Minuten mit Übersetzung (`cargo test --workspace`, Debug); die `--ignored`-Reihen im Release liefen zuletzt in Aufgabe 17 grün und wurden hier nicht wiederholt; `git diff --stat f2f748d` auf den geschützten Pfaden ist leer.
