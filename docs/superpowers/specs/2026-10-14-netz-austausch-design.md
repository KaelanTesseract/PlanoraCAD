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

Stand: 2026-10-14. Der Plan `plans/2026-10-14-netz-austausch.md` legte vor der Umsetzung 17 Entscheidungen (P1–P17) zur Spec fest; eine Vorprüfung (15 Rulings R-1 bis R-15) korrigierte den Plan an einigen Stellen. Hier steht, was daraus geworden ist. Bei Widerspruch gilt dieser Abschnitt vor §1–§11.

### Entscheidungen P1–P17

| Nr. | Entscheidung | Begründung und Ergebnis |
|---|---|---|
| P1 | Alle Schreiber nehmen `&[ExportBody]`; Quellen über `TriSource` (`SolidSource`, `PlacedSource`), `collect_body` wandelt nach f32, prüft Punktnummern und verschmilzt bitgleiche Punkte. | Ein Weg für Kernel-Körper und Netz-Körper. Umgesetzt wie geplant (Task 2, 9). |
| P2 | `MeshReport` hat zusätzlich `flipped_edges`; `closed` heißt: mindestens eine Kante, keine offene, keine an mehr als zwei Dreiecken, keine falsch herum. Dreiecke mit doppelter Punktnummer sind entartet und tragen keine Kanten bei. | Die Spec nannte nur offene und mehrfach belegte Kanten; die Orientierung (§2) braucht den Zähler. Umgesetzt (Task 2, 12 Tests). |
| P3 | `weld_exact` verschmilzt bitgleiche Punkte (−0 gleich +0), entfernt unbenutzte Punkte, ordnet nach erster Verwendung; beim Lesen von STL **und** OBJ. | Spec §4 nannte nur STL; OBJ-Dateien anderer Programme verdoppeln Punkte an Normalen- und Texturnähten. Umgesetzt (Task 2, 3, 4). |
| P4 | Leser reservieren höchstens 65 536 Dreiecke im Voraus (`RESERVE_STEP`), dann wächst der Speicher mit dem Gelesenen; `MAX_VERTICES = 3 · MAX_TRIANGLES`, `MAX_LINE_BYTES = 1 MiB`; Zeilenenden `\n`, `\r\n`, `\r`; UTF-8-BOM entfällt; Nicht-UTF-8-Bytes werden ersetzt. | Setzt §5 („kein Speicher nach Kopfzahlen“) um; der Allocator-Test (`alloc_guard`) misst es. Umgesetzt (Task 1, 3, 4, 5). Offene Nebenpunkte aus dem Review: kein Test für CR/LF genau an der 64-KiB-Puffergrenze, keiner für „keine Zuteilung nach der Kopfzahl“ im Einzelnen. |
| P5 | STL-Text: erkannt, wenn die Datei (nach BOM und Leerraum) mit `solid` beginnt und die Größe nicht 84 + 50 · n ist; jedes andere erste Wort ist ein `Syntax`-Fehler mit Zeile. | Konkretisiert §3. Folge: Eine Binärdatei mit „solid“-Kopf und falscher Größe ergibt eine irreführende Meldung (in §3 so gewollt). Umgesetzt (Task 3). |
| P6 | OBJ: Ecken verweisen nur auf schon gelesene Punkte (positiv oder negativ); Vielecke ab 4 Ecken: konvex als Fächer, sonst `earcutr` in der Ebene der größten Normalkomponente; `earcutr` ist Abhängigkeit von `planora-mesh`. | Siehe „Weitere Abweichungen“. Zusätzlich `MAX_EARCUT_CORNERS = 5000` aus dem Review (Task 4). |
| P7 | 3MF: jeder ZIP-Eintrag wird zweimal erzeugt (erst Größe und CRC, dann schreiben); kein ZIP64: ab 4 GiB je Eintrag oder Archiv `TooLargeForFormat`; feste Zeit 1.1.1980; Namen für XML maskiert; Metadaten `Application = PlanoraCAD`. | Nichts liegt im Speicher, gleiche Datei bei gleichem Inhalt. Umgesetzt (Task 6, 7); mit eigenem ZIP- und XML-Mini-Leser geprüft und gegenüber `unzip`, Python und `Expand-Archive` gegengeprüft. |
| P8 | glTF: Normalen über `shade` (Grenzwinkel 30°, harte Kanten verdoppeln Punkte); JSON selbst geschrieben; Längen über 4 GiB `TooLargeForFormat`. | Dieselbe Funktion wie die Anzeige der Netz-Körper. Umgesetzt (Task 8); die Datei wurde gegen Python gegengeprüft. Ruling R-7: Bei genau 30° wird nicht mehr geglättet (`>` statt `>=`; §6 sagt „weniger als 30°“). |
| P9 | `export_file` schreibt in `.<Name>.planora-tmp`, `sync_all`, dann umbenennen; bei Fehler Hilfsdatei weg, Zieldatei unverändert; `import_file` wählt nach Endung. | Setzt §5 um. Umgesetzt (Task 9, `stage2`). Der feste Hilfsname erlaubt nur einen Export je Zieldatei zur Zeit. |
| P10 | `FeatureKind::MeshBody { mesh, offset, rotation, scale }` erzeugt keinen Kernel-Körper; neu `visible_meshes()`; Boolesche Operation, Verschieben, Verrunden/Fase, Skizze auf Fläche ergeben `FeatureError::MeshBodyLocked`. In der Oberfläche verschiebt „Position“ über die eigene Lage. | Spec §6 wörtlich bis auf: kein Feature „Verschieben“ für Netz-Körper, kein Bearbeiten-Fenster. Umgesetzt (Task 10, 13). |
| P11 | `Document.meshes: BTreeMap<MeshId, Arc<ExchangeMesh>>`; Kennungen ab 1, nie wiederverwendet; `add_mesh_body` legt Netz und Feature nur gemeinsam an; Löschen räumt unbenutzte Netze weg; `Snapshot.meshes` enthält nur benutzte. | Konkretisiert §6. Umgesetzt (Task 10). Lücke: Nach Wechsel der Netz-Kennung bleibt ein Netz im Speicher, bis es gelöscht wird (nur Speicher). |
| P12 | Format 3: `FORMAT_VERSION = 3`, kleinste passende Version wird geschrieben; Feld `netze` nur bei Bedarf; `daten` = Base64 von Punktzahl, Dreieckszahl, f32-Punkten, u32-Indizes (little endian); Base64 in `planora-project`; Länge, Kopf und Grenzen vor dem Dekodieren geprüft; Netz-Körper in Format 1/2 → beschädigt; mechanische Testanpassung („zu neu“ = Format 4). | Siehe „Weitere Abweichungen“. Umgesetzt (Task 11, `format3.json`). |
| P13 | Anzeige: Netz-Körper ist in der App ein `Body` mit `BodyKind::Mesh`, einer Fläche über das ganze Netz, ohne Kantenlinien; ab `DISPLAY_TRIANGLES = 5 000 000` zeigt die Ansicht jedes k-te Dreieck; Volumen, Fläche und Hülle immer aus dem ganzen Netz. | Setzt §11 um. Umgesetzt (Task 12). Der Prüfbericht wird je `MeshId` gemerkt; Anzeige-Netz und Masseneigenschaften dagegen bei jeder Änderung für alle sichtbaren Netz-Körper neu berechnet (Review Task 12, in offene-punkte vermerkt). |
| P14 | Eigene Dateiauswahl im Fenster statt Systemdialog. | Siehe „Weitere Abweichungen“ und R-4. Umgesetzt (Task 15, 16, 17). |
| P15 | Arbeitsthreads: `std::thread`, `mpsc`, `Progress`; Lesen bricht alle 4096 Zeilen oder Dreiecke ab; Vernetzen eines Körpers nicht unterbrechbar, Abbruch zwischen Körpern; Schreiben ohne Abbrechen; ein ohne Ergebnis endender Thread wird als Meldung angezeigt. | Weicht von §8 ab („Abbrechen möglich“), siehe unten. Umgesetzt (Task 15, 16, 17). |
| P16 | Reihenfolge der `match`-Stellen: alle Arme für `FeatureKind::MeshBody`, `KindDto::NetzKoerper` und `BodyKind::Mesh` schon in Task 10, Format 3 in Task 11. | Wie in Teilprojekt 8 (P13), damit jeder Zwischenstand übersetzt. Zwischen Task 10 und 11 speicherte ein Dokument mit Netz-Körper noch ohne Netzdaten; kein Test berührt das. |
| P17 | Drehung als Quaternion `[x, y, z, w]` (Feld `drehung`), angezeigt als Euler-Winkel in Grad (XYZ); „normiert“ heißt `|‖q‖ − 1| ≤ ROTATION_NORM_TOL = 1e-6`. | Konkretisiert §6. Umgesetzt (Task 10, 11, 13). Ruling R-2: serde_json liest f64 nur auf 1 ULP genau zurück; nach Speichern und Laden sind Lage und Maßstab bitgleich, Drehung und Volumen stimmen bis `1e-12` (relativ) überein, `float_roundtrip` ist nicht eingeschaltet. |

### Weitere Abweichungen von §1–§11

- **Eigene Dateiauswahl statt Dialog des Betriebssystems (§8, P14, R-4).** §8 verlangt für „Exportieren …“ den Speichern-unter-Dialog des Betriebssystems; §2, E7 und die Projektvorgabe verbieten neue externe Pakete, und ein Systemdialog braucht eines (etwa `rfd`). Die Spec widerspricht sich hier selbst; „keine neue Abhängigkeit“ wiegt schwerer. Das Programm hat deshalb ein eigenes Auswahlfenster (Ordnerliste, Ordner eintippen, Doppelklick öffnet einen Ordner, Dateiname, Ersetzen nach zweitem Klick bestätigen); gleiches gilt für „Netz importieren …“. Aus der Fixrunde von Task 15: Dateinamen mit `: < > " | ? * / \` und Steuerzeichen werden abgewiesen (so lässt sich nicht außerhalb des Ordners schreiben), relative Pfade werden absolut gemacht, Lesefehler haben einfache deutsche Texte. Laufwerkswechsel nur durch Eintippen. Will der Nutzer den Systemdialog: neue Abhängigkeit mit Lizenzprüfung, die Dateiauswahl (`FileChooser`) in den zwei Fenstern ersetzen.
- **`flipped_edges` im Bericht (§4, P2).** §4 kennt das Feld nicht; es zählt Kanten, deren zwei Dreiecke dieselbe Richtung haben.- **OBJ wird beim Lesen ebenfalls verschmolzen (§4, P3).** §4 nennt nur STL.
- **Base64 in `planora-project` statt in `planora-exchange` (§2, P12).** Es wird nur dort gebraucht; §2 führt es unter `planora-exchange`. Dort bleiben `ExchangeMesh::{to_bytes, byte_len_from_header, from_bytes}` (Binärform, Task 11).
- **`earcutr` aus `planora-mesh` (§3, P6).** §3 schreibt, `earcutr` sei schon Abhängigkeit des Kernels; es ist Abhängigkeit von `planora-mesh`. Es kommt als Arbeitsbereichs-Abhängigkeit zu `planora-exchange` dazu, kein neues Paket, `cargo deny check` bleibt sauber.
- **Abbrechen nur zwischen Körpern (§8, P15).** §8 sieht Abbrechen für das Berechnen des Netzes vor; ein einzelner Körper wird aber vom Vernetzer (`tessellate`, bitgleich zu `master`, geschützter Pfad) in einem Stück gerechnet. Er rechnet zu Ende, das Ergebnis wird verworfen. Lesen lässt sich alle 4096 Zeilen oder Dreiecke abbrechen; das Schreiben der Datei nicht.
- **Grenzen von 3MF und glTF bei 4 GiB (P7, P8).** §5 kennt nur die Grenzen beim Lesen. Beim Schreiben von 3MF (ohne ZIP64) und glTF (32-Bit-Längen) liefert das Programm bei mehr als 4 GiB den Fehler `TooLargeForFormat` und rät zu STL. Dazu: höchstens 65 535 Einträge je 3MF-Archiv.
- **Größe des Dokuments (R-13, §7).** §7 schreibt, das Dokument werde „größer als der Quelldateiinhalt“. Das stimmt nicht allgemein: Binäres STL braucht 50 Byte je Dreieck, das Dokument etwa 4/3 · (12 · Punkte + 12 · Dreiecke) Byte, ist also meist kleiner als das STL. Richtig ist: etwa ein Drittel größer als die reinen Netzdaten (Base64). Die README sagt das entsprechend.
- **MAX_EARCUT_CORNERS (Task 4, §3 und §11).** Die Zerlegung nicht gewölbter Vielecke mit `earcutr` wächst quadratisch (ein Stern mit 100 000 Ecken: 14,7 s im Debug-Build) und lässt sich nicht abbrechen. Nicht gewölbte Vielecke mit mehr als `MAX_EARCUT_CORNERS = 5000` Ecken ergeben deshalb `Polygon` mit Zeile; gewölbte Fächer bleiben bis zur Zeilengrenze erlaubt. Folge: Sehr fein gerundete Einzel-Vielecke (Kreis mit über 5000 Ecken) gelten wegen der f32-Rundung als nicht gewölbt und werden abgelehnt.
- **Karten gleich hoch: `allocate_space` (Task 15, §8, Nutzervorgabe).** Der Plan setzte `ui.set_min_height(min_height)` nach dem Karteninhalt; das ergab Höhen von 119 und 191 statt gleich hoch (der Test scheiterte). `cards::card_row` reserviert stattdessen nach dem Inhalt `ui.allocate_space(vec2(0, (min_height - natural).max(0)))`. Die Höhe wird erst neu gesetzt, wenn der Unterschied `HEIGHT_SLACK` (0,5 Punkte) übersteigt.
- **Gemeinsame Fensterhilfen (R-5, R-6).** `wide_window`, `Job::fetch` und die gemeinsamen Konstanten (`MIN_WIDTH`, `PROGRESS_WIDTH`, `REPAINT`, `LOST`) stehen einmal im Export-Fenster und werden vom Import-Fenster mitbenutzt; die freien Zahlen sind benannte Konstanten.
- **Veraltetes Netz wird nie geschrieben (Task 16, Review).** Der Plan prüfte beim Schreiben nicht, ob sich Auswahl, Qualität, Umfang oder Dokument seit der Berechnung geändert hatten; es wäre ein veraltetes Netz in die Datei gekommen (im Test: „Quader 1“ statt „Kugel“). `write_to(path, state)` prüft jetzt den Schlüssel (Meldung `STALE`), meldet ein fehlendes Netz (`NOT_COMPUTED`), die Dateiauswahl schließt bei Änderung des Schlüssels, und der Umfang fällt ohne Auswahl auf „Alle sichtbaren Körper“ zurück. Sehnenschranken stehen als `CHORD_COARSE/NORMAL/FINE`.
- **`load_at` und `STALE` im Import-Fenster (Task 17, Review).** Der Plan band ein gelesenes Netz nicht an den Dokumentstand. `load_at(path, edits)` merkt `document.edit_count()`; `import` liefert bei geändertem Dokument `STALE` und ändert nichts, `show` verwirft ein veraltetes Ergebnis. Eine neue Dateiwahl ersetzt das laufende Lesen (der alte Auftrag wird fallengelassen und bricht ab); ein fertiger Auftrag wird mit `take()` verworfen. Das ist strenger als nötig: Jede Änderung am Dokument verwirft ein langes Lesen, auch eine ohne Bezug.
- **Eigenschaftenfenster (Task 13).** Der Hilfetext der Netz-Körper steht als `MESH_BODY_HINT` (mit `concat!`), weil der Planquelltext eine Leerzeichenfolge enthielt; `euler_degrees` und `mass_rows` sind gemeinsame Hilfsfunktionen (R-8, R-9).
- **Anpassungen an Plantests (Rulings).** R-1: `closed_formulas_survive_the_files` nimmt als Sehnenschranke das Zweifache (`SAG_FACTOR = 2.0`), weil die Mitte eines Dreiecks im Breiten-/Längenraster bis zu zwei Sehnenhöhen unter der Fläche liegt (gemessen 1,08 bis 1,10). R-2: Rundlauf Drehung und Volumen mit `1e-12`, siehe P17. R-3: unbenutzter Import in `tests/zip.rs` entfernt. R-11: Die Hilfsfunktion zum Nachbau der Versionsprüfung entfällt. Alle anderen Schranken, Asserts und `#[ignore]` blieben unangetastet.
- **Test Task 10:** `type Edit = fn(&mut FeatureKind);` wegen `clippy::type_complexity`; die Asserts sind unverändert. Bei Task 15 und 16 gibt es zusätzliche Tests aus den Review-Runden (je mit Mutationsprobe).

### Befunde der Stufenreviews, die bewusst offen blieben

Alle sind in `offene-punkte.md` (Abschnitt „Teilprojekt 9 – Netz-Austausch“) erwähnt, soweit sie Nutzer betreffen. Unwesentliches:

- Task 2: `weld_exact` setzt gültige Indizes voraus (nur Doc); kein Test für zwei Dreiecke mit gleicher Kantenrichtung allein.
- Task 3: kein Test für CR/LF an der 64-KiB-Puffergrenze.
- Task 5: das Ignore auf der 0,5-s-Reihe (R-12) ist unnötig; der Zuteilungswächter (`LIMIT` 16 MiB) ist locker; die Formelschranke bei Sehne 0,1 ist schwach.
- Task 6: ZIP-Namen werden nicht geprüft (sie sind fest im Code); die Grenze 65 535 Einträge hat keinen Randtest; der Mini-Leser darf bei kaputten Dateien ausschlagen (Prüfwerkzeug).
- Task 7: `xml_escape` lässt U+FFFE und U+FFFF in Namen stehen; die Schreiber prüfen das Netz nicht selbst (`collect_body` tut es); die Tests prüfen Namensräume, Inhaltstypen und Orientierung nicht.
- Task 8: `write_glb` schreibt Wert für Wert; Aufrufer müssen `BufWriter` nehmen (tut `export_file`); keine eigene `is_finite`-Prüfung in `write_glb` (`collect_body` und `check_mesh` davor).
- Task 9: `stage2` prüft für STL und OBJ weniger als der Brief; `export_file` ist nicht abbrechbar.
- Task 10: Beim Import laufen `validate` und `check_mesh_extent` je einmal über das ganze Netz (doppelter Aufwand); ein Dokument mit fehlendem Netz wird als Ganzes abgelehnt (so geplant).
- Task 11: `MAX_MESH_BYTES = 8 GiB` lässt das Dekodieren etwa das Dreifache des Textes an Speicher brauchen; `from_bytes` macht eine zusätzliche Kopie (`Vec<f32>`); `required_format` übergeht ungenutzte Netze in einem von Hand gebauten Snapshot (verwirrende Meldung, kein Fehler).
- Task 12: Vorschaugrenze bei genau 5 Millionen um eins ungenau; ein fehlgeschlagenes Neurechnen lässt `edit_count` erhöht.
- Task 14: freie Toleranzen in `mesh_flow` (0,01 · Fläche, 1e-6, 1e-4) sind keine benannten Konstanten; Lage nach OBJ-Export wird dort nicht geprüft (Task 2 deckt gelagerte Netze ab); der Vergleich „geschlossen == geschlossen“ ist schwach.
- Task 15: das Lesen eines Ordners prüft je Eintrag `is_dir` im Hauptfaden; ein einfacher Klick auf einen Ordner zeigt keine Markierung.
- Task 16: Meldung „veraltet“ ohne Ordner; der Umfang wechselt nicht von selbst zurück auf „Gewählter Körper“; ein Kommentar in `app.rs` ist 107 Zeichen lang (rustfmt umbricht keine Kommentare).
- Task 17: Namensfeld ohne Prüfung auf Steuerzeichen und Länge (das gemeinsame `rename_feature` auch nicht); Fehlertext nennt „1e9“; Fehlermeldung doppelt in Fenster und Statuszeile.

### Prüfergebnis (Stufe 4, 2026-10-14)

Einzeln nacheinander gelaufen, alle grün:

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`: sauber.
- `cargo test --workspace` (Debug): 1311 bestanden, 0 fehlgeschlagen, 29 ignoriert in 177 Testprogrammen; etwa 8 Minuten mit Übersetzung; längstes Testprogramm 27,7 s (aus früheren Teilprojekten, `nurbs_ssi_stress` 25,7 s).
- `cargo deny check`: advisories, bans, licenses, sources ok; keine neue Abhängigkeit (`deny.toml` unverändert).
- `cargo test -p planora-exchange --release -- --ignored`: `a_long_random_series_keeps_the_volume` ok in 0,48 s.
- `git diff --stat 249bd38` auf `planora-kernel/src/boolean` und `planora-mesh/src/{tessellate,trimmed,discretize,check}.rs`: leer. Die Golden-Dateien `format1.json`, `format1_voll.json`, `format2.json`: unverändert (neu ist nur `format3.json`).
- Nicht gelaufen: der Handtest im echten Fenster (Task 19, nur nach Rücksprache mit dem Nutzer); die langen Release-Reihen anderer Pakete (nicht verändert).

Hinweis: Plan und Brief nannten als Vergleichsstand 134cde9 (Spec); geprüft wurde gegen 249bd38 (Verzweigung von `master`), für die geschützten Pfade dasselbe.
