# Teilprojekt 9: Netz-Austausch (Export und Import von Dreiecksnetzen) – Entwurf

Stand: 2026-10-14. Baut auf Teilprojekt 8 (Rundungen und Fasen, auf `master`, Merge ba42fa2) und dem Mesh-Task auf.
Dies ist Teil 1 einer dreiteiligen Austausch-Strecke: Teil 2 = STEP-Export (exakte Flächen), Teil 3 = Zeichnungen (SVG/DXF).
Beide folgen als eigene Teilprojekte mit eigener Spec und eigenem Plan.

## 1. Ziel und Abgrenzung

Das Programm kann Körper als Dreiecksnetz in gängige Formate ausgeben (3D-Druck, Ansicht, Präsentation) und Netze aus STL und OBJ einlesen.

**Entscheidungen (mit dem Nutzer abgestimmt, 2026-10-14):**

| Nr. | Entscheidung |
|---|---|
| E1 | Zweck: 3D-Druck, Austausch mit anderer CAD-Software, Präsentation, technische Zeichnung. Aufgeteilt in drei Teilprojekte; jetzt nur Netz-Austausch. |
| E2 | Formate schreiben: STL (binär), OBJ (Text), 3MF (ZIP), glTF (als .glb). Formate lesen: STL (binär und ASCII), OBJ. |
| E3 | Import ist ein **Referenz-Netz** („Netz-Körper“): anzeigen, verschieben, drehen, skalieren, messen, wieder exportieren. Keine Booleans, Rundungen oder Skizzen darauf. |
| E4 | Export-Umfang: ist ein Körper gewählt, nur dieser; sonst alle sichtbaren Körper. Unterdrückte Features und ausgeblendete Körper zählen nicht. |
| E5 | Qualität: drei Stufen im Dialog – Grob 0,1 mm, Normal 0,01 mm, Fein 0,001 mm Sehnentoleranz; Dreieckszahl und geschätzte Dateigröße werden angezeigt. |
| E6 | Nicht ganz geschlossenes Netz (bekannte Lücken bei Freiform): warnen, trotzdem schreiben. |
| E7 | 3MF wird mit einem eigenen ZIP-Schreiber ohne Kompression geschrieben. Keine neue Abhängigkeit. |
| E8 | Grenzen beim Import deutlich groß: höchstens 50 Millionen Dreiecke, höchstens 8 GB Dateigröße. Kein Speicher wird nach Zahlen im Dateikopf reserviert, nur nach tatsächlich Gelesenem. |
| E9 | Das Netz eines Netz-Körpers wird im Dokument gespeichert (kompakt, binär), nicht als Verknüpfung. Neues Dateiformat 3, nur wenn ein Netz-Körper vorkommt. |

**Nicht Teil dieses Teilprojekts:** Import von STEP/3MF/glTF, Farben und Materialien, Texturen, Exportieren einzelner Flächen, Netz-Reparatur, Netz-Booleans, Skizzen auf Netzflächen, Mehrkörper-Baugruppen als eine Datei mit Struktur (3MF/glTF bekommen einfach ein Objekt je Körper).

## 2. Aufbau

Neues Paket `planora-exchange` (Abhängigkeiten: `planora-mesh`, `planora-math`, `thiserror` und das schon vorhandene `earcutr`; keine neuen externen Pakete; Base64, CRC-32 und ZIP werden selbst geschrieben).

```
planora-exchange/src/
  lib.rs        // öffentliche API: write_*, read_*, MeshReport
  limits.rs     // MAX_TRIANGLES = 50_000_000, MAX_FILE_BYTES = 8 GiB
  check.rs      // Geschlossenheit, entartete Dreiecke, Verschmelzen gleicher Punkte
  stl.rs        // binär schreiben; binär + ASCII lesen
  obj.rs        // Text schreiben; lesen (Dreiecke, Vielecke zerlegt)
  zip.rs        // minimaler ZIP-Schreiber (Methode „stored“, CRC-32)
  threemf.rs    // 3MF schreiben (Millimeter)
  gltf.rs       // .glb schreiben (Meter = mm × 0,001)
  error.rs      // ExchangeError mit deutschen Meldungen
```

**Eigener Datentyp** `ExchangeMesh { positions: Vec<[f32;3]>, indices: Vec<u32> }` für das Einlesen; Schreiber nehmen `planora_mesh::Mesh` (Körper aus dem Kernel) oder `ExchangeMesh` (Netz-Körper). Beide Wege laufen über dieselbe Schnittstelle `TriSource` (Iteration über Dreiecke mit Punkten in f64, nach Anwendung der Körpertransformation).

**Einheiten:** Das Programm rechnet in Millimetern. STL, OBJ, 3MF: Millimeter (3MF trägt `unit="millimeter"`). glTF: Meter, also mm × 0,001.

**Dreiecksorientierung:** Außenseite nach Rechte-Hand-Regel (Gegen-Uhrzeigersinn von außen), wie `planora-mesh` sie liefert. Beim Import wird sie übernommen; die Prüfung meldet unstimmige Orientierung als „nicht geschlossen“.

## 3. Formatdetails

- **STL binär:** 80-Byte-Kopf (Text „Planora“, Rest Nullen), u32 Dreieckszahl, je Dreieck Normale (aus den Punkten), drei Punkte, u16 Attribut = 0. Bei mehreren Körpern: alle Dreiecke in einer Datei.
- **STL ASCII (nur lesen):** erkannt, wenn die Datei mit `solid` beginnt **und** die Dateigröße nicht zu 84 + 50·n (n aus Kopf) passt; sonst binär. Mehrere `solid`-Blöcke werden zusammengefasst.
- **OBJ:** `o Name` je Körper, `v x y z`, `vn` je Punkt (Flächennormale der anliegenden Dreiecke gemittelt), `f a//n b//n c//n`. Lesen: `v`, `f` mit `v`, `v/vt`, `v//vn`, `v/vt/vn`; Vielecke ab 4 Ecken werden als Fächer zerlegt (nur konvex sinnvoll; nicht konvexe Vielecke werden mit `earcutr` zerlegt, das bereits Abhängigkeit des Kernels ist); negative Indizes (relativ) werden unterstützt; alles andere (`mtllib`, `usemtl`, `g`, `s`, `vt`, `vn`) wird ignoriert.
- **3MF:** ZIP mit `[Content_Types].xml`, `_rels/.rels`, `3D/3dmodel.model`. Ein `object` je Körper (`type="model"`), ein `build`/`item` je Objekt. Punkte als Dezimalzahlen mit mindestens 6 Nachkommastellen.
- **glTF (.glb):** ein Puffer, je Körper ein Mesh mit Attributen `POSITION` und `NORMAL` und `indices` (u32), ein Knoten je Mesh. `asset.version = "2.0"`, `generator = "PlanoraCAD"`.

## 4. Prüfung und Berichte

`MeshReport { triangles, vertices, open_edges, non_manifold_edges, degenerate_triangles, closed: bool }`.

- `closed` heißt: jede Kante gehört zu genau zwei Dreiecken in entgegengesetzter Richtung.
- Entartete Dreiecke (doppelte Punkte oder Flächeninhalt exakt 0) werden gezählt, nicht gelöscht.
- Beim Import verschmilzt `weld_exact` Punkte mit **bitgleichen** Koordinaten (STL speichert jede Ecke neu). Kein Toleranzverschmelzen.
- Der Export-Dialog zeigt den Bericht; bei `closed == false` erscheint die Warnung (E6).

## 5. Grenzen und Schutz

- Import lehnt ab: Datei > 8 GB, mehr als 50 Millionen Dreiecke, NaN/Unendlich, Indizes außerhalb, abgeschnittene Dateien, Kopfzahl größer als die Datei hergibt.
- **Kein Speicher nach Dateikopf-Zahlen:** Der Reader prüft `Kopfzahl ≤ (Dateigröße − Kopf) / Dreiecksgröße` und reserviert erst dann; Text-Reader wachsen mit dem Gelesenen. Die Zählung läuft gegen die Grenzen schon beim Lesen.
- Alle Fehler sind benannte `ExchangeError`-Varianten mit deutschem Text, mit Zeilennummer bei Textformaten.
- Export schreibt zuerst in eine Hilfsdatei im Zielordner und benennt dann um; eine vorhandene Zieldatei bleibt bei Fehlern unverändert.
- Import ändert das Dokument erst nach vollständigem Erfolg.

## 6. Netz-Körper im Dokument

**Feature** `FeatureKind::MeshBody { mesh: MeshId, offset: DVec3, rotation: DQuat, scale: f64 }`; das Netz selbst liegt in einer Dokument-Tabelle `meshes` (`MeshId → ExchangeMesh`), damit mehrere Features dasselbe Netz teilen können (Umbenennen, Kopieren ohne doppelten Speicher).

- `scale` > 0, endlich, höchstens 1e6 und mindestens 1e-6; `rotation` normiert; `offset` endlich.
- Der Netz-Körper erscheint im Feature-Baum wie ein Körper (Symbol „Netz“), kann unterdrückt und gelöscht werden. Eigenschaften: Name, Position, Drehung (Euler in Grad), Maßstab, Volumen, Oberfläche, Abmessungen, Bericht (geschlossen ja/nein).
- **Bearbeitungssperre:** Menüpunkte Vereinigen/Abziehen/Schneiden/Verrunden/Fase/Skizze-auf-Fläche sind für Netz-Körper ausgegraut mit Hinweis „Netz-Körper lassen sich nicht bearbeiten“. Die Feature-Neuberechnung weist solche Verweise (Booleans mit Netz-Körper als Operand) mit benannter Fehlermeldung ab, falls eine Datei sie enthält.
- Anzeige: wie jeder Körper (Dreiecke aus dem Netz, glatte Normalen nur wo Flächennormalen weniger als 30° abweichen, sonst Kanten hart); Kantenlinien werden nicht gezeichnet.
- Wiederexport: das Netz mit seiner Transformation.

## 7. Dateiformat 3

- Format 3 wird geschrieben, wenn das Dokument mindestens einen Netz-Körper enthält; sonst weiter die kleinste passende Version (1 oder 2), wie in TP8.
- Netzdaten stehen als Base64-Zeichenkette im JSON (kompakt: f32-Positionen und u32-Indizes, little endian, vorangestellt Anzahl). Das Dokument wird also größer als der Quelldateiinhalt (4/3 durch Base64); das ist bewusst einfach gehalten.
- Lesen: Länge der Base64-Zeichenkette gegen `MAX_FILE_BYTES` und gegen die Dokumentgrenzen prüfen, bevor dekodiert wird; Zählungen gegen `MAX_TRIANGLES`; Indizes < Punktzahl; keine NaN. Fehler führen zu `Corrupt` mit benannter Meldung.
- Programme mit Format ≤ 2 lehnen Format 3 als „zu neu“ ab (bestehende Versionsprüfung).
- Die Golden-Datei `format3.json` kommt in den Testdaten dazu; `format1*.json` und `format2.json` bleiben unverändert.

## 8. Oberfläche

**Datei → Exportieren … (Strg+Umschalt+E):** Fenster mit Format (STL/OBJ/3MF/glTF), Qualität (Grob/Normal/Fein), Umfang (Auswahl oder sichtbare Körper), Anzeige „n Dreiecke, ca. x MB“ (Schätzung aus Dreieckszahl und Format), Bericht mit Warnung (E6), Schaltfläche „Exportieren …“ (öffnet den Speichern-unter-Dialog des Betriebssystems für Ordner und Dateiname). Das Netz für die Anzeige im Fenster wird in einem Arbeitsthread berechnet; bei „Fein“ kann das dauern, daher Fortschrittsanzeige und Abbrechen.

**Datei → Netz importieren …:** Dateiwahl (STL/OBJ), Prüfbericht (Dreiecke, geschlossen/nicht geschlossen, entartete Dreiecke), Name, Einheit der Quelldatei (Millimeter, Zentimeter, Meter, Zoll; Standard Millimeter), „Importieren“. Die Datei wird im Arbeitsthread gelesen; Abbrechen möglich.

Texte in einfacher Sprache (Nutzervorgabe), keine Hinweise auf Herkunft von Daten; Layout über die ganze Fensterbreite, nebeneinander stehende Karten gleich hoch.

## 9. Tests

- **Rundlauf:** STL und OBJ schreiben, lesen, vergleichen: Dreieckszahl, Volumen, Fläche, Geschlossenheit.
- **Formeln:** Quader, Zylinder, Kugel, gerundeter Körper (TP8) exportieren und das Volumen aus der Datei gegen die Formel prüfen (Sehnentoleranz berücksichtigen).
- **3MF/glTF:** eigener Mini-Leser prüft ZIP (CRC, Verzeichnis, Methode „stored“), XML wohlgeformt, Dreieckszahl und Punktzahl; glTF: Kopf, JSON-Block, Puffergrößen, Zugriffsgrenzen und `min`/`max` für POSITION.
- **Feindliche Dateien:** abgeschnitten, Kopfzahl größer als Datei, NaN, Indizes außerhalb, 8-GB-Behauptung ohne Inhalt, Zeilenumbruch-Varianten (CRLF), UTF-8-BOM, riesige Zahlen; immer benannter Fehler, keine große Speicheranforderung (gemessen über Allocator-Zähler im Test).
- **Dokument:** Netz-Körper speichern und laden (Format 3), Format 1/2 bleiben lesbar, Format 3 in Programm ≤ 2: „zu neu“ (simuliert), Mehrfachverwendung desselben Netzes, Transformation, Unterdrücken, Löschen, gesperrte Operationen.
- **Zufall:** zufällige Körper aus bekannten Serien exportieren, wieder lesen, Volumenabweichung gegen die Kernel-Masseneigenschaften.
- **Handtest** im echten Fenster am Ende, nach Rückfrage beim Nutzer.

## 10. Stufen

1. `planora-exchange`: Fehler, Grenzen, Prüfung, STL, OBJ (Schreiben und Lesen), Tests.
2. 3MF (ZIP) und glTF, Tests mit eigenen Mini-Lesern.
3. Netz-Körper: Feature, Dokument-Tabelle, Format 3, Eigenschaften, Bearbeitungssperre, Anzeige.
4. Dialoge (Exportieren, Netz importieren), Doku (README, offene-punkte, Spec-Abschnitt „Änderungen bei der Umsetzung“), Gesamtprüfung, Handtest.

Nach jeder Stufe: Tests, fmt, clippy, deny; nach Stufe 4 Gesamtprüfung des Zweigs.

## 11. Risiken

- Große Netze (Millionen Dreiecke): Anzeige und Speicherbedarf; Rendern nutzt dieselbe Pipeline wie Körper, daher Arbeitsgrenze bei der Anzeige (Hinweis, wenn über 5 Millionen Dreiecke: Anzeige in Vorschau-Stufen, Daten bleiben vollständig).
- Base64 im JSON macht Dokumente mit großen Netzen sehr groß; ein späteres Teilprojekt kann einen Binärblock einführen. Dokumentieren.
- Nicht ganz geschlossene Netze der Freiform-Körper: Warnung, bekannte Grenze aus TP7/TP8.
- OBJ mit nicht konvexen Vielecken: Zerlegung mit `earcutr` kann bei entarteten Vielecken fehlschlagen; benannter Fehler.
- ASCII-/Binär-Erkennung bei STL: klassische Mehrdeutigkeit (Binärdateien, die mit `solid` beginnen) wird über die Größenprüfung entschieden.

## 12. Änderungen bei der Umsetzung

(wird beim Abschluss des Teilprojekts ergänzt)
