# Speichern, Laden, Projektverwaltung und Revisionen – Entwurf (Teilprojekt 6)

Stand: 2026-10-10. Baut auf den Specs zu Fundament/Kernel, Booleschen Operationen, Skizzen und Feature-Baum auf.

## 1. Ziel und Umfang

PlanoraCAD kann Dokumente (die Feature-Zeitleiste) dauerhaft speichern und wieder laden. Dazu gehören:

- eine **Projektverwaltung** (Projekte → Ordner → Dokumente) als Panel, das sich öffnen lässt oder dauerhaft angedockt bleibt,
- eine **Revisionsverwaltung**: jedes Speichern erzeugt eine neue Version; Versionen bilden einen Baum mit Zweigen,
- **mehrere offene Dokumente** als Tabs.

Das Bedienprinzip orientiert sich an gängigen CAD-Datenverwaltungen. Es wird kein Fremdmaterial übernommen (kein Dateiformat, keine Icons, keine Namen eines Fremdprodukts).

**Nicht Teil dieses Teilprojekts:** Zusammenführen von Zweigen, Cloud/Teilen/Berechtigungen, Export in Fremdformate (STEP/STL: eigenes Teilprojekt), Vorschaubilder, benannte Parameter, Rückgängig/Wiederholen, Speichern der Kameraansicht.

## 2. Entscheidungen

| Nr. | Entscheidung | Begründung |
|---|---|---|
| D1 | Lokaler Projektordner auf der Festplatte (frei wählbare Wurzel, Vorgabe `Dokumente\PlanoraCAD`) | Einfach zu sichern/zu synchronisieren, keine Server-Infrastruktur; Cloud wäre ein eigenes Teilprojekt |
| D2 | Ordnerstruktur aus normalen Dateien; Dokument = Ordner `Name.pcad` mit `dokument.json` und `versionen/NNNN.json` | Lesbar, robust, nur Neues wird geschrieben, ein defektes Byte gefährdet nicht alle Versionen |
| D3 | JSON über `serde`/`serde_json` | Lesbar und robust bei Änderungen; `serde` ist bereits im Build (über andere Pakete), neu ist `serde_json` (MIT OR Apache-2.0) – Lizenzprüfung mit `cargo deny` ist Pflicht (siehe §9) |
| D4 | Versionsbaum (Eltern-Zeiger je Version) mit Zweigen; Zusammenführen entfällt | Linear als Normalfall, Abzweigen/Wiederherstellen ohne Mehraufwand im Datenmodell |
| D5 | Jede Version ist ein vollständiger Zeitleisten-Stand (kein Delta) | Einfach, robust; Dokumente sind klein |
| D6 | Mehrere Dokumente als Tabs; jeder Tab hat seinen eigenen `AppState` | Der Zustand (Zeitleiste, Auswahl, Skizzen-Modus, Kamera) ist bereits in `AppState` gebündelt |
| D7 | Dateiformat entkoppelt von den internen Strukturen: eigene Format-Typen („DTOs“) in `planora-project`, mit `serde`-Ableitungen nur dort | Interne Umbauten brechen keine alten Dateien; Kernel und Skizzen-Crate bleiben frei von `serde` |
| D8 | Gespeichert wird nur die Zeitleiste, nie Körper oder Ergebnisse; beim Laden wird neu berechnet | Eine einzige Quelle der Wahrheit, keine veralteten Zwischenstände |

## 3. Dateiformat und Ordneraufbau

```
<Wurzel>/
  <Projekt>/                 projekt.json   (Format-Version, Projekt-Id, Name)
    <Ordner>/…               normale Unterordner, beliebig tief
      <Dokument>.pcad/
        dokument.json        Versionsbaum
        versionen/0001.json  0002.json …   ein vollständiger Stand je Version
        .autosave.json       nur nach Änderungen, siehe §7
  .papierkorb/               gelöschte Projekte/Ordner/Dokumente (nie endgültig gelöscht)
```

**`dokument.json`:** `format` (Ganzzahl, beginnt bei 1), `programm` (Programmversion beim letzten Schreiben), `id` (zufällige Dokument-Id), `versionen` (Liste: `nummer`, `eltern` (Nummer oder null), `zweig`, `zeit` (UTC, ISO 8601), `kommentar`, `programm`), `zweige` (Name → Nummer der Spitze), `aktiver_zweig`.

**Versionsdatei:** `format`, `features` (Liste in Zeitleistenreihenfolge), `marker` (Anzahl aktiver Features), `naechste_id`. Pro Feature: `id`, `name`, `unterdrueckt`, `art` (Quader, Zylinder, Kugel, Kegel, Skizze, Extrusion, Rotation, Boolesche Operation, Verschieben) mit allen Parametern. Skizzen enthalten Punkte, Kurven, Constraints, Konstruktionsflags, Ebene (Grundebene oder `FaceRef`) und Kantenkopien (`EdgeRef`). Alle Ids (Feature-, Punkt-, Kurven-Ids) werden unverändert gespeichert, damit Verweise gültig bleiben.

**Regeln:**
- Zahlen sind IEEE-754-Doubles im JSON-Textformat; NaN/Inf werden beim Schreiben abgelehnt und beim Lesen als beschädigt gemeldet.
- Unbekannte Felder in bekannten Versionen werden ignoriert; ein `format` größer als das bekannte wird mit klarer Meldung abgelehnt („Datei wurde mit einer neueren Version erstellt“), nie halb gelesen.
- Beim Laden werden **alle** Werte geprüft (Ids eindeutig, Verweise zeigen auf frühere Features, Zahlen im Bereich `MAX_COORD_MM`, Marker ≤ Anzahl Features), bevor ein `Document` entsteht; Fehler nennen Datei und Feature.
- Alle Schreibvorgänge sind atomar (siehe §4).

## 4. Versionsmodell und Operationen

- **Speichern:** neue Versionsdatei (`0007.json`) als `.tmp` schreiben, Datei sichern (`flush`/`sync_all`), umbenennen; danach `dokument.json` genauso ersetzen. Bricht es dazwischen ab, bleibt der alte `dokument.json` gültig; eine nicht eingetragene Versionsdatei gilt beim nächsten Öffnen als „verwaist“, wird ignoriert und gemeldet.
- **Speichern unter:** neues Dokument (neue Id), Version 1 mit dem aktuellen Stand.
- **Alte Version öffnen:** schreibgeschützter Tab „Name · v3“. Bearbeiten ist dort gesperrt; angeboten werden *Wiederherstellen* und *Abzweigen*.
- **Wiederherstellen:** alte Version wird als neue Version an die Spitze des aktuellen Zweigs gehängt (Eltern = bisherige Spitze), mit Kommentar „Wiederhergestellt aus v3“.
- **Abzweigen:** neuer benannter Zweig ab beliebiger Version (Name eindeutig, nicht leer, ≤ 60 Zeichen). Wird ein offenes Dokument von einer Version geöffnet, die nicht die Spitze ihres Zweigs ist, und gespeichert, fragt die App nach einem Zweignamen und legt den Zweig an.
- **Kommentar nachtragen:** ändert nur `dokument.json`.
- **Löschen:** nur Zweigspitzen ohne Kinder (Version) bzw. Zweige, die nicht der aktive sind und deren Versionen nur diesem Zweig gehören; Versionen, auf die andere aufbauen, bleiben. Gelöscht wird endgültig nur im Papierkorb nach Rückfrage.
- **Projekte/Ordner/Dokumente:** Anlegen, Umbenennen, Verschieben, Löschen (in `.papierkorb`, mit Zeitstempel im Namen). Namen werden bereinigt: keine Pfadtrenner, kein `..`, keine reservierten Windows-Namen (CON, NUL, COM1 …), keine führenden/abschließenden Punkte und Leerzeichen, Länge ≤ 80; kollidierende Namen werden abgelehnt. Nichts außerhalb der Wurzel wird gelesen oder geschrieben (kanonische Pfadprüfung, auch gegen Symlinks/Junctions).
- **Nebenläufigkeit:** Eine Sperrdatei `.sperre` (Prozess-Id, Zeit) im Dokumentordner verhindert, dass zwei Programminstanzen dasselbe Dokument schreiben; veraltete Sperren (Prozess nicht mehr vorhanden) werden nach Rückfrage übernommen.

## 5. Umwandlung `Document` ↔ Format

- `planora-feature` bekommt keine Abhängigkeit von `serde`. Es erhält eine kleine, geprüfte Schnittstelle zum **Zerlegen und Zusammensetzen**: `Document::snapshot() -> Snapshot` (reine Daten: Features, Marker, `next_id`) und `Document::from_snapshot(Snapshot) -> Result<Document, FeatureError>` (prüft Ids, Verweise, Zeitleistenordnung und Wertebereiche wie §3, setzt Versionen neu, alle Features „nicht berechnet“).
- `planora-sketch` bekommt entsprechend `Sketch::to_data()`/`Sketch::from_data()` über öffentliche, stabile Datentypen (Punkte, Kurven, Constraints, Flags) – `from_data` baut den Löser-Zustand neu auf und löst einmal.
- `planora-project` enthält die Format-Typen mit `serde`-Ableitungen und die Abbildung Snapshot ↔ Format. Eine **Golden-Datei** im Repository (`tests/data/`) pinnt das Format 1; ein Test liest sie und vergleicht mit dem erwarteten `Document`.

## 6. Oberfläche (`planora-app`)

- **Tabs:** Tab-Leiste über der 3D-Ansicht; je Tab Name, „●“ bei ungespeicherten Änderungen, „×“ zum Schließen. Der Tab-Zustand bündelt `AppState` + Dokumentpfad + aktuelle Version + Schreibschutz-Flag. Browser und Eigenschaften zeigen den aktiven Tab.
- **Projekt-Panel** („Projekte“, andockbar mit dem vorhandenen Docking, Standard links neben dem Browser, ein-/ausblendbar über *Ansicht → Projekte*): Baum Projekt → Ordner → Dokument; Doppelklick öffnet in einem Tab (bereits offen: dorthin springen); Rechtsklick: Neu, Umbenennen, Verschieben (per Auswahldialog, kein Ziehen nötig), Löschen. Kopfzeile: Wurzel wählen. Leerzustand führt zu „Neues Projekt“. Die Wurzel wird in der Konfiguration gemerkt (Datei im Konfigurationsordner des Betriebssystems, ohne neue Abhängigkeit über Umgebungsvariablen `APPDATA`/`XDG_CONFIG_HOME`).
- **Versionen-Panel** („Versionen“, andockbar): Versionsliste des aktiven Tabs (neueste oben) mit Nummer, Zweig, Zeit, Kommentar; oben die Zeile „Änderungen“ bei ungespeicherten Änderungen; Zweige farblich/eingerückt unterscheidbar; Doppelklick öffnet schreibgeschützt; Rechtsklick: Öffnen, Wiederherstellen, Abzweigen, Kommentar ändern, Löschen.
- **Menü Datei:** Neu (Strg+N), Öffnen … (Strg+O, öffnet das Projekt-Panel), Speichern (Strg+S; Kommentardialog, leer erlaubt), Speichern unter …, Schließen (Strg+W). Beenden/Tab schließen mit ungespeicherten Änderungen: Rückfrage Speichern / Verwerfen / Abbrechen.
- **Neues Dokument** ist „Unbenannt n“ ohne Pfad; erstes Speichern fragt nach Projekt, Ordner und Name.
- **Dirty-Erkennung:** Ein Zähler am `Document` (steigt bei jeder Änderung) wird beim Speichern notiert; unterschiedlich = geändert.
- Alle Texte deutsch.

## 7. Fehler und Absturzschutz

- Beschädigte, zu neue oder unvollständige Dateien: klare Meldung mit Pfad; das Dokument wird nicht verändert. Fehlen Versionsdateien, öffnet das Dokument die letzte lesbare Version des Zweigs und meldet den Rest.
- Schreibfehler (Platte voll, kein Zugriff, gesperrt): Meldung, der Tab bleibt geändert, die vorherige Version bleibt unversehrt.
- **Autosicherung:** alle 2 Minuten, nur wenn der Tab geändert ist, als `.autosave.json` neben dem Dokument (bzw. im Konfigurationsordner für unbenannte Dokumente); erzeugt keine Version. Beim Öffnen eines Dokuments mit neuerer Autosicherung bietet die App Wiederherstellen/Verwerfen an. Nach erfolgreichem Speichern oder Verwerfen wird sie gelöscht.
- Keine Panics auf Dateiinhalten: jede Verarbeitung ist fehlerrückgebend, auch bei absichtlich verfälschten Dateien (Tests, siehe §8).

## 8. Tests

- **Rundlauf:** zufällige Zeitleisten (proptest, bestehende Erzeuger aus `planora-feature`) → Snapshot → Format → Datei → laden → neu berechnen ergibt dieselben Körper (Volumen, Flächenzahl, Statuswerte der Features).
- **Golden-Datei** für Format 1 (siehe §5).
- **Beschädigte Dateien:** abgeschnittene, leere, falsch typisierte, NaN/Inf-haltige, zyklische oder auf spätere Features verweisende Dateien, doppelte Ids, zu große Zahlen, unbekannte Formatversion: jeweils Fehler, nie Panik, Dokument unverändert.
- **Versionsbaum:** Speichern, Wiederherstellen, Abzweigen, automatischer Zweig beim Speichern einer alten Version, Löschregeln, Kommentar ändern; Eltern-Zeiger immer gültig, kein Zyklus.
- **Atomarität:** Schreiben bricht nach Versionsdatei ab (simuliert) → alter Stand lesbar, verwaiste Datei erkannt; Absturz nach `.tmp`.
- **Pfadsicherheit:** `..`, absolute Pfade, reservierte Namen, Symlink/Junction aus der Wurzel hinaus, zu lange Namen.
- **Sperre:** zweite Instanz bekommt Meldung; veraltete Sperre wird nach Rückfrage übernommen.
- **App ohne Fenster** (egui-Testrahmen wie bisher): Tabs öffnen/schließen/wechseln, Dirty-Punkt, Rückfragen, Projekt-Panel-Aktionen, Versionen-Panel, Schreibschutz, Autosicherung-Angebot.
- **Handprüfung** mit `tools/gui-drive.ps1`: Projekt anlegen, Dokument speichern, ändern, speichern, alte Version öffnen, wiederherstellen, abzweigen, App beenden und neu starten.

## 9. Abhängigkeiten und Lizenzen

- Neu: `serde` (bereits im Build; hier direkte Abhängigkeit mit `derive`), `serde_json` (MIT OR Apache-2.0) samt seiner Unterpakete. **Vor dem Einbinden `cargo deny check` ausführen; liegt ein Paket außerhalb der Erlaubnisliste, anhalten und den Eigner fragen – die Liste wird nie selbstständig erweitert.** Zufällige Ids und Zeitstempel kommen aus der Standardbibliothek (`SystemTime`, eigene Id-Erzeugung aus Zeit und Prozess-Id), ohne weitere Pakete; ein ISO-8601-Text wird von Hand formatiert.
- `planora-project` hängt von `planora-feature`, `planora-sketch`, `planora-math`, `planora-kernel` (nur `BoolOp`), `serde`, `serde_json`, `thiserror` ab; es kennt keine Oberfläche.
- Kein Fremdmaterial; Projektlizenz GPL-3.0-only.

## 10. Risiken

- **Mehrere Tabs** berühren den App-Zustand breit (Viewport, Dialoge, Eingabeverarbeitung greifen auf einen einzigen `AppState` zu); Aufteilung in Aufgaben so legen, dass zuerst „Tab-Rahmen mit einem Tab“ lauffähig ist.
- **Skizzen-Rundlauf:** die Löser-Zustände (Constraints, Dragging) müssen exakt reproduziert werden; `from_data` löst einmal und vergleicht gegen die gespeicherten Punkte (Abweichung → Warnung am Feature, nicht Fehler).
- **Dateisystem unter Windows:** reservierte Namen, Junctions, gesperrte Dateien (Virenscanner) – Umbenennen mit kurzen Wiederholungen bei Zugriffsfehlern.
- **Wachsende Datenmenge:** vollständige Stände je Version sind für typische Dokumente klein; große Skizzen können hunderte KB je Version erreichen – akzeptiert, Delta/Komprimierung als offener Punkt.
