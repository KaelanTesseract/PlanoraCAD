# PlanoraCAD – Spec Teilprojekt 5: Feature-Baum (parametrische Historie)

Datum: 2026-10-10 · Status: Entwurf zur Freigabe · Baut auf: `2026-10-09-fundament-und-kernel-design.md`, `2026-10-09-boolesche-operationen-design.md`, `2026-10-09-skizzen-und-solver-design.md` (alle umgesetzt und nach `master` gemergt)

## 1. Ziel und Einordnung

Alles, was der Nutzer erzeugt, liegt in einer Zeitleiste (Feature-Baum). Ändert man ein Maß, eine Skizze oder einen Parameter, rechnet das Programm ab dieser Stelle neu und alle Folgekörper passen sich an. Aus Momentaufnahmen wird parametrisches Arbeiten.

**Umfang (vom Projekteigner gewählt: „Parametrischer Baum“):**
- Features: Skizze, Quader, Zylinder, Kugel, Kegel, Extrusion, Rotation, Boolesche Operation (Vereinigen, Abziehen, Schneiden), Verschieben.
- Bedienung: umbenennen, löschen (mit Folgen), zurückrollen, unterdrücken, Parameter ändern, Feature am Rückrollmarker einfügen.
- Neuberechnung ab dem geänderten Feature mit Zwischenspeicher je Feature.
- Stabile Verweise auf Flächen und Kanten („Rollen + Herkunft“, persistente Namen), damit eine Skizze auf einer Körperfläche nach einer Änderung weiter auf derselben Fläche liegt.
- Fehler gehören dem Feature: roter Eintrag mit deutschem Text, abhängige Features „ausgesetzt“.

**Nicht im Umfang:** Speichern/Laden (eigenes Teilprojekt, dann mit Dateiformat), benannte Parameter und Gleichungen in Maßen, Rückgängig/Wiederherstellen, Fillets/Chamfer, Export, Muster- und Spiegel-Features für Körper, ein allgemeiner Abhängigkeitsgraph (die Zeitleiste ist linear).

**Erfolgskriterium:** Eine Kette aus Skizze → Extrusion → Skizze auf der Deckelfläche → Extrusion → Boolesche Operation lässt sich in der App aufbauen; ändert man ein Maß der ersten Skizze oder die Höhe der ersten Extrusion, passen sich alle Folgekörper an und die Skizze auf der Deckelfläche bleibt auf ihr; Fehler (z. B. verlorene Fläche) werden am verursachenden Feature gemeldet, ohne die übrigen zu zerstören.

### Einordnung in die Teilprojekte

| # | Teilprojekt | Stand |
|---|---|---|
| 1+2 | Fundament, Kernel-Basis | umgesetzt |
| 3 | Boolesche Operationen | umgesetzt |
| 4 | Skizzen + Constraint-Solver | umgesetzt |
| **5** | **Feature-Baum** | **diese Spec** |
| 6–8+ | Speichern/Laden, Fillets, Export, NURBS, … | später |

## 2. Getroffene Grundentscheidungen

- **Lineare Zeitleiste** statt Abhängigkeitsgraph: Jedes Feature verweist nur auf frühere. Zyklen sind unmöglich; die Neuberechnung ist einfach und vorhersagbar.
- **Neues Crate `planora-feature`** (Datenmodell, Neuberechnung, Verweise; ohne Oberfläche und Grafik). Es hängt von `planora-math`, `planora-kernel` und `planora-sketch` ab; Tessellierung bleibt in `planora-mesh`/App.
- **Persistente Namen über Rollen + Herkunft** (vom Projekteigner bestätigt), Hinweispunkt als zweite Stufe. Keine rein geometrische Wiedererkennung als Hauptmechanismus (nicht deterministisch).
- **Skizzen werden Features** (die bisherige Skizzenliste der App entfällt); Körper entstehen nur noch aus Features. Der bisherige Boolean-Dialog und die Erzeugen-Dialoge werden zu „Feature anlegen“.
- Keine neuen externen Abhängigkeiten; Lizenzen werden weiter geprüft (`cargo deny check` bleibt grün, `deny.toml` nie ohne Rückfrage erweitern). Kein Autodesk-Material; Texte deutsch.

## 3. Datenmodell (`planora-feature`)

- `FeatureId`: stabile, nur hochzählende Nummer (wie `next_sketch_number`; nie wiederverwendet).
- `Feature { id, name, suppressed, kind, status }`. `status`: `Ok`, `Error(String)`, `Skipped` (ausgesetzt wegen eines Fehlers davor), `NotComputed` (hinter dem Rückrollmarker).
- `FeatureKind`:
  - `Sketch(SketchFeature)`: `Sketch` (aus `planora-sketch`) und die Ebene `SketchPlane::{Standard(XY|XZ|YZ), OnFace(FaceRef)}`.
  - `Box { size, origin }`, `Cylinder { radius, height, origin }`, `Sphere { radius, origin }`, `Cone { radius, height, origin }`: Maße und Position (Ursprung des Quaders bzw. Mittelpunkt/Fußpunkt).
  - `Extrude { sketch: FeatureId, profile: usize, height: f64 }`, `Revolve { sketch: FeatureId, profile: usize, axis: CurveId }`.
  - `Boolean { target: BodyRef, tool: BodyRef, op: BoolOp }`.
  - `Move { body: BodyRef, delta: DVec3 }`.
- `BodyRef { feature: FeatureId, index: usize }`: ein Körper aus der Ergebnisliste eines früheren Features.
- `Document { features: Vec<Feature>, marker: usize, next_id: u64 }`. `marker` = Anzahl der Features, die berechnet werden (der Rückrollmarker); neue Features werden an dieser Stelle eingefügt.
- Ergebnis je Feature (`FeatureResult`): `bodies: Vec<Solid>` (Skizzen-Features liefern keine Körper). Ein `Boolean`-Feature **verbraucht** Ziel und Werkzeug (sie gelten ab dort als „aufgegangen“, die Körperliste enthält sie nicht mehr) und erzeugt den Ergebniskörper; `Move` ersetzt den Körper durch den verschobenen. Die **sichtbaren Körper** sind alle Ergebniskörper, die kein späteres Feature verbraucht hat.

### 3.1 Neuberechnung

`Document::recompute(from: usize) -> RecomputeReport`:
1. Von `from` bis zum Marker in Reihenfolge: unterdrückte Features überspringen (ihre Körper erscheinen nicht); sonst Feature berechnen. Eingaben (Skizzen, Körper früherer Features) kommen aus dem Zwischenspeicher.
2. Schlägt ein Feature fehl (unauflösbarer Verweis, Kernel lehnt ab, ungültiger Profilindex, fehlende Achse, nicht mehr vorhandener Eingangskörper), bekommt es `Error(text)`; alle späteren Features, die einen seiner Körper oder seine Skizze brauchen, bekommen `Skipped`; unabhängige Features rechnen weiter.
3. Zwischenspeicher: Je Feature wird das Ergebnis mit einem Schlüssel aus (Feature-Parameter, Ergebnis-Fingerprints der Eingaben) gemerkt. Ändert sich ein Feature, ändert sich der Fingerprint und nur die Nachfolger, die davon abhängen, werden neu berechnet. `RecomputeReport` zählt die tatsächlich neu berechneten Features (für Tests).
4. Änderungsoperationen (alle atomar, bei Fehler bleibt das Dokument unverändert): `add_feature`, `set_params`, `rename`, `suppress`, `move_marker`, `delete(id)`, `edit_sketch(id, f)`. `delete` liefert die Liste der abhängigen Features; erst `delete_with_dependents(id)` entfernt sie mit.

## 4. Persistente Namen

### 4.1 Marken im Kernel
`Face` bekommt `tag: Option<FaceTag>`; `FaceTag { origin: u64, role: Role }` (`origin` = `FeatureId` des erzeugenden Features, kernelneutral als `u64`; `Role` ist eine kleine Aufzählung bzw. ein Text-Schlüssel). Das Feld beschreibt nur; es ändert keine Geometrie und fließt nicht in die Validierung ein.
- Quader: `Boden`, `Deckel`, `Seite +x`, `Seite −x`, `Seite +y`, `Seite −y`. Zylinder/Kegel: `Boden`, `Deckel`, `Mantel`. Kugel: `Kugel`.
- Extrusion: `Boden` (Skizzenebene), `Deckel`, `Wand k` (k = Segmentposition in der Schleife), Löcher `Loch j, Wand k`.
- Rotation: `Fläche k` je Segment der Kette (k von der Achse aus gezählt), Achsenkappen entfallen (es gibt keine).
- Verschieben (`transform`): übernimmt Marken. Zusammennähen (`sew`) und Auswahl (`select`) der Booleschen Operation reichen die Marke jeder `Piece` an die erzeugte Fläche weiter; zerteilte Flächen tragen dieselbe Marke.
- Die Kernel-API bekommt dafür Parameter `origin: u64` für `extrude`/`revolve`/`make_*` bzw. eine nachträgliche Funktion `tag_solid(solid, origin)`; Aufrufer ohne Feature (bestehende Tests, Beispielprofile) geben `None`/0 an und erzeugen unmarkierte oder neutral markierte Flächen.

### 4.2 Verweise
- `FaceRef { origin: u64, role: Role, hint: DVec3 }`. Auflösen in einem Körper: alle Flächen mit gleicher Marke sammeln. Genau eine → sie. Mehrere (zerteilte Fläche) → die mit dem Innenpunkt, der `hint` am nächsten liegt. Keine → Fehler „Fläche … von Feature … nicht mehr vorhanden“. Nach erfolgreichem Auflösen wird `hint` auf den neuen Innenpunkt gesetzt.
- `EdgeRef { a: FaceRef, b: FaceRef, hint: DVec3 }`: die Kante, die beide Flächen gemeinsam haben; bei mehreren entscheidet der Abstand des Kantenpunkts zum `hint`.
- Verweise entstehen automatisch, wenn der Nutzer in der Oberfläche eine Fläche oder Kante anklickt (Skizze auf Fläche, Kante kopieren).

### 4.3 Grenzen
Verschwindet eine Fläche oder entsteht sie neu, ist der Verweis verloren und das Feature meldet es. Bei vertauschten gleichartigen Flächen entscheidet der `hint` meistens richtig, aber nicht garantiert; das wird dokumentiert.

## 5. Oberfläche (`planora-app`)

- **Baum statt Körperliste** im Browser: Zeilen mit Symbol, Name, Statusfarbe (rot Fehler, grau unterdrückt/ausgesetzt/nicht berechnet); erzeugte Körper als Unterzeilen; der Rückrollmarker als ziehbare Linie.
- **Aktionen:** Klick wählt; Doppelklick bearbeitet (Dialog mit aktuellen Werten, Skizzen öffnen den Skizzen-Modus); Kontextmenü: Umbenennen, Unterdrücken/Aktivieren, Löschen (Rückfrage mit Liste der Folgen), Zurückrollen bis hier.
- **Dialoge:** Erzeugen-Dialoge legen Features an; Extrusion/Rotation wählen eine Skizze aus der Zeitleiste; Boolesche Operation wählt Ziel und Werkzeug aus den sichtbaren Körpern; „Verschieben“ ist ein Feature, das Positionsfeld im Eigenschaftenfenster schreibt hinein.
- **Skizzen:** „Skizze beenden“ ruft `edit_sketch` und rechnet ab dort neu. Skizze auf Fläche speichert einen `FaceRef`.
- **Eigenschaften:** zeigen Parameter und Fehlertext des gewählten Features.
- **Ruhe beim Neuberechnen:** Auswahl und Kamera bleiben, soweit die Körper noch existieren.
- **Ableitung:** `AppState::bodies` wird aus dem Dokument abgeleitet (sichtbare Körper + Netz + Maße); die bestehenden Zugriffe (Auswahl, Pick, Boolean-Plausibilität, Netz, Eigenschaften) laufen darüber weiter. Die direkte Mutation der Körperliste entfällt.
- Alle Texte deutsch; keine Autodesk-Begriffe oder -Symbole.

## 6. Fehlerbehandlung

- Änderungsoperationen sind atomar (Klon und Wiederherstellung bei Fehler).
- Fehler beim Neuberechnen gehören dem Feature; die Oberfläche zeigt sie im Baum und in der Statuszeile.
- Verlorene Verweise nennen Fläche/Kante und erzeugendes Feature.
- Keine Panik auf Nutzereingaben: Ids, Indizes, Profile, Achsen werden an den API-Grenzen geprüft.

## 7. Tests

- **Neuberechnung:** Änderung am dritten Feature rechnet nur ab dort (Zähler im `RecomputeReport`); Ergebnisse davor sind identisch; Rückrollen, Unterdrücken, Aussetzen nach Fehler.
- **Beispielketten (Ende zu Ende):** Skizze → Extrusion → Boolesche Operation; Maß ändern → Volumen entspricht der neuen Rechnung; Skizze auf der Deckelfläche → Extrusion der ersten Skizze höher → Skizze 2 folgt.
- **Verweise:** Auflösen bei einer, mehreren und keiner Fläche; Kantenverweise; Verlust meldet Fehler; `hint` wandert mit.
- **Kernel-Marken:** je Primitiv/Extrusion/Rotation die erwarteten Rollen; Marken bleiben durch Verschieben und durch alle Auswahlfälle der Booleschen Operationen (Vereinigen, Abziehen mit Umdrehen, Schneiden; bündige Fälle) erhalten; bestehende Kernel-Tests bleiben grün.
- **Löschen mit Folgen, Umbenennen, Einfügen am Marker.**
- **Eigenschaft (proptest, feste Seeds):** zufällige Folgen von Änderungsoperationen halten das Dokument gültig (keine Panik, Status konsistent, sichtbare Körper bestehen den Validator); Mindestzahl geprüfter Fälle wird erzwungen.
- **App:** Baum-Zustand ohne Fenster; Rauchtests der Dialoge; Rechtsklickmenü wie bisher; Skizzen-Modus über Features.

## 8. Risiken

| Risiko | Gegenmaßnahme |
|---|---|
| Marken gehen in Booleschen Operationen verloren | Marke als Teil von `Piece`; Tests über alle Auswahlfälle der Tabelle |
| Neuberechnung bei großen Bäumen träge | Zwischenspeicher je Feature, Fingerprints der Eingaben, nur Nachfolger rechnen neu |
| Umbau der App (Körperliste → Baum) berührt viel | Schrittweise in Aufgaben; abgeleitete Körperliste; bestehende Tests als Netz |
| Verweise auf Flächen mit mehreren Stücken mehrdeutig | `hint` als zweite Stufe; klare Fehlermeldung, wenn mehrdeutig |
| Skizzen als Features ändern bisherige Abläufe | Skizzen-Modus bleibt, nur der Speicherort wechselt; Tests der Skizzen-App bleiben |

## 9. Lizenzen und Abhängigkeiten

Keine neue externe Abhängigkeit geplant. Falls doch nötig: vorher Lizenz prüfen, `cargo deny check` grün halten, `deny.toml` nie ohne Rückfrage erweitern. Kein Autodesk-Material.

## 10. Änderungen bei der Umsetzung

Stand: 2026-10-10, nach Abschluss der Umsetzung (Branch `feature/feature-baum`). Die Punkte F1–F13 stammen aus dem Plan und wurden dort vor Beginn festgelegt; die weiteren Punkte sind bei der Umsetzung entstanden.

**Entscheidungen aus dem Plan (F1–F13)**
- **F1 – Rollen der Grundkörper:** Quader, Zylinder, Kegel und Kugel werden im Kernel über `extrude`/`revolve` gebaut und erben deren Rollen (`Boden`, `Deckel`, `Wand k`, `Fläche k`). Eigene Rollen wie „Seite +x“ gibt es nicht (ändert §4.1).
- **F2 – Herkunft nachträglich:** Der Kernel erzeugt Marken mit `origin = 0` („noch nicht zugewiesen“); `Solid::assign_origin(feature_id)` setzt die Herkunft aller unzugewiesenen Marken. `FeatureId`s beginnen bei 1.
- **F3 – Zwischenspeicher über Versionen statt Hashes:** Jedes Feature hat eine `version` und ein Ergebnis mit `result_version`. Der Schlüssel eines Features ist die eigene Version plus die Ergebnisversionen aller gelesenen Eingaben. `Document::recompute()` hat kein Argument `from`: es durchläuft immer die Zeitleiste bis zum Marker und rechnet nur dort neu, wo der Schlüssel abweicht.
- **F4 – Volumenprobe im Feature:** `planora-feature` hängt von `planora-mesh` ab und führt die Volumenprobe der Booleschen Operation (`boolean_volume_plausible`) beim Berechnen des Features aus (bisher in der App).
- **F5 – Körperverbrauch:** Ein `Boolean`-Feature verbraucht Ziel und Werkzeug, `Move` ersetzt den Körper; sichtbar ist, was kein späteres Feature verbraucht hat. `Move` behält keine Körperidentität (neuer Körper ist `BodyRef { feature: move_id, index: 0 }`).
- **F6 – Skizzen-Entwurf:** Der Skizzen-Modus arbeitet auf einer Kopie der Skizze (`sketch_draft`); „Skizze beenden“ schreibt sie per `Document::update` zurück und rechnet neu. Während des Zeichnens wird nicht neu berechnet.
- **F7 – Kantenkopien folgen:** Kopierte Körperkanten werden als `EdgeCopy { curve, edge }` im Skizzen-Feature geführt und beim Neuberechnen mit `Sketch::reproject_edge` angepasst. Ändert die Kante ihre Art, meldet das Skizzen-Feature einen Fehler.
- **F8 – Profilindex:** `Extrude`/`Revolve` verweisen mit `profile: usize` auf das Profil in `Sketch::profiles()`.
- **F9 – Position:** Position und Verschieben laufen über `Move`-Features. Das Positionsfeld ändert das neueste `Move` desselben Körpers, wenn es das letzte Feature ist, das diesen Körper liest; sonst entsteht ein neues `Move` am Marker.
- **F10 – Status:** `FeatureStatus` hat fünf Varianten (`Ok`, `Error`, `Skipped`, `Suppressed`, `NotComputed`).
- **F11 – Rückrollmarker per Menü** („Zurückrollen bis hier“, „Zum Ende“); Ziehen mit der Maus gibt es nicht.
- **F12 – Beispielprofile entfernt:** `samples.rs` und die zugehörigen Dialoge entfallen; Skizzen ersetzen sie.
- **F13 – Übergang:** Zwischenzeitlich legten `extrude_sketch`/`revolve_sketch` je ein Skizzen-Feature als Kopie an; mit der Oberfläche des Baums (Aufgabe 8) wurde die alte Skizzenliste abgelöst.

**Weitere Abweichungen**
- **Fehler „unplausibles Ergebnis“:** Die Volumenprobe meldet `FeatureError::ImplausibleResult`. Die Probe mit 2 % Abweichung kann kleine gekrümmte Körper zu Unrecht ablehnen.
- **Fehler werden zwischengespeichert:** Ein fehlgeschlagenes Feature trägt einen eigenen Schlüssel (`failed`) und wird bei unveränderten Eingaben nicht jedes Mal neu berechnet; `RecomputeReport.reused` enthält daher auch übernommene Fehler. Die erste Fassung rechnete Fehler bei jedem Lauf neu.
- **Unterdrücken meldet seinen Umfang:** Neben `set_suppressed` gibt es `set_suppressed_with_report`. Wiederholtes Setzen desselben Werts ändert nichts.
- **Reihenfolge der sichtbaren Körper:** Das Ergebnis einer Booleschen Operation oder eines `Move` nimmt den Platz seines ersten Eingangs ein (statt am Ende angehängt zu werden). Die Oberfläche darf sich nicht auf feste Listenplätze verlassen.
- **Zusatzfunktionen im Dokument:** `delete_body`, `naming_feature`, eine Prüfung `check_invariants` (inkl. Regel „kein `NotComputed` vor dem Marker“) und eine Liste der Abhängigen (`dependents`) für „Löschen mit Folgen“.
- **Fehlgeschlagene Boolesche Operation oder Verschiebung** werden in der App zurückgenommen, damit das Dokument unverändert bleibt.
- **Grenze für Verschiebungen:** `move_body` weist Lagen jenseits von ±1e9 mm ab (`MAX_COORD_MM`).
- **Leser hinter dem Marker:** Beim Verschieben werden auch Features hinter dem Rückrollmarker als Leser des Körpers berücksichtigt.
- **Position nach Zeitleistenlogik (R9):** Wird ein Körper im Eigenschaftenfenster verschoben, kommt ein `Move` ans Ende der Zeitleiste; eine Skizze oder Extrusion, die davor liegt, folgt nicht. Änderungen am Quader-Feature selbst werden dagegen von abhängigen Skizzen übernommen.
- **Skizzenebene bei Skizzen auf Flächen:** Die Ebene wird aus der Fläche abgeleitet; fehlt dem Skizzen-Feature ein Ergebnis, nutzt die Oberfläche die gespeicherte Definition.
- **Offene Skizze:** Vor Änderungen am Dokument wird der Entwurf zuerst übernommen („Commit zuerst“); wird er abgelehnt, bleibt die Skizze offen. Löschen eines Features beendet die offene Skizze.
- **Namen von Booleschen Operationen:** „Boolesche Operation N“ (statt Vereinigen/Abziehen/Schneiden im Namen); drei ältere Tests wurden mechanisch angepasst.
- **Handprüfung:** Die Oberfläche wurde nicht von Hand durchgespielt; die Bildschirmsteuerung wurde in Aufgaben 7–9 und bei der Abschlussprüfung unterbrochen. Gesehen wurde nur Start, Quader anlegen und das Rechtsklickmenü des Baums (siehe `docs/superpowers/offene-punkte.md`).
