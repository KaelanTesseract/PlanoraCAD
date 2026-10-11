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
- **Nebenläufigkeit:** Eine Sperrdatei `.sperre` (Prozess-Id, Zeit) im Dokumentordner verhindert, dass zwei Programminstanzen dasselbe Dokument schreiben; eine vorhandene Sperre wird immer gemeldet und nur nach Rückfrage übernommen (ob der Prozess noch lebt, wird nicht geprüft; siehe §11, P4).

## 5. Umwandlung `Document` ↔ Format

- `planora-feature` bekommt keine Abhängigkeit von `serde`. Es erhält eine kleine, geprüfte Schnittstelle zum **Zerlegen und Zusammensetzen**: `Document::snapshot() -> Snapshot` (reine Daten: Features, Marker, `next_id`) und `Document::from_snapshot(Snapshot) -> Result<Document, FeatureError>` (prüft Ids, Verweise, Zeitleistenordnung und Wertebereiche wie §3, setzt Versionen neu, alle Features „nicht berechnet“). Die Typen heißen `Snapshot`/`SnapshotFeature`; `Document::edit_count()` liefert den Änderungszähler (siehe §11).
- `planora-sketch` bekommt entsprechend `Sketch::to_data()`/`Sketch::from_data()` über öffentliche, stabile Datentypen (Punkte, Kurven, Constraints, Flags) – `from_data` baut den Löser-Zustand neu auf und prüft alle Typen und Werte, löst aber **nicht** (siehe §11, P2).
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
- **Autosicherung:** alle 2 Minuten, nur wenn der Tab geändert ist, als `.autosave.json` neben dem Dokument (bzw. im Konfigurationsordner für unbenannte Dokumente); erzeugt keine Version. Beim Öffnen eines Dokuments mit Autosicherung bietet die App Wiederherstellen/Verwerfen an (nur für gespeicherte Dokumente; siehe §11, P9). Nach erfolgreichem Speichern oder Verwerfen wird sie gelöscht.
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
- **Skizzen-Rundlauf:** die Löser-Zustände (Constraints, Dragging) müssen exakt reproduziert werden; `from_data` löst nicht und vergleicht nicht; die Neuberechnung des Dokuments löst die Skizzen (siehe §11, P2).
- **Dateisystem unter Windows:** reservierte Namen, Junctions, gesperrte Dateien (Virenscanner) – Umbenennen mit kurzen Wiederholungen bei Zugriffsfehlern.
- **Wachsende Datenmenge:** vollständige Stände je Version sind für typische Dokumente klein; große Skizzen können hunderte KB je Version erreichen – akzeptiert, Delta/Komprimierung als offener Punkt.

## 11. Änderungen bei der Umsetzung

Dieser Abschnitt hält fest, was beim Bauen anders wurde als oben beschrieben, jeweils mit Grund. Wo der Wortlaut oben abweicht, gilt dieser Abschnitt.

### Entscheidungen aus dem Plan (P1–P10)

| Nr. | Entscheidung | Grund |
|---|---|---|
| P1 | `Document::snapshot()` liefert reine Daten (`Snapshot { features, marker, next_id }`, Features als `SnapshotFeature { id, name, suppressed, kind }`); `Document::from_snapshot(snapshot, chord_tolerance)` baut ein Dokument neu auf und rechnet einmal. Die Format-Typen liegen nur in `planora-project`. | Kein Zugriff auf Interna von außen; `planora-feature` bleibt ohne `serde`. |
| P2 | `Sketch::to_data()`/`from_data()` übertragen alle Einträge der Arenen, auch gelöschte (`alive = false`), damit Ids unverändert bleiben. `from_data` löst **nicht**; die Neuberechnung des Dokuments löst die Skizzen. Abweichung von §10 (Lösen mit Vergleich). | Einfacher, derselbe Nutzen: Eine Skizze wird ohnehin beim Berechnen gelöst. |
| P3 | `Document::edit_count()` steigt bei jeder öffentlichen Änderung (Hinzufügen, Ändern, Umbenennen, Löschen, Unterdrücken, Marker setzen), nur nach erfolgreicher Prüfung; bei Umbenennen, Unterdrücken und Marker nur bei tatsächlicher Änderung. Ein Tab ist „geändert“, wenn der Zähler vom beim Speichern notierten Wert abweicht oder eine Skizze offen ist. | Billige, eindeutige Änderungserkennung (§6). Bekannte Folge: `update` zählt auch dann, wenn der Dialog unveränderte Werte übernimmt. |
| P4 | Eine vorhandene Sperre wird immer gemeldet; der Nutzer kann sie nach Rückfrage übernehmen. Abweichung von §4 („veraltete Sperren“). | Ob der Prozess einer `.sperre` noch lebt, lässt sich ohne neue Abhängigkeit nicht prüfen. |
| P5 | Schreibgeschützte Versions-Tabs sperren Browser, Eigenschaften und die Menüs „Erzeugen“/„Bearbeiten“ als deaktivierte Bedienelemente; `AppState` bekommt kein Schreibschutz-Flag. | Kleinste Änderung am bestehenden Zustand. Folge: Nicht jede Zustandsmethode prüft den Schreibschutz. |
| P6 | Jeder Tab merkt sich die Version, auf der sein Stand beruht (`based_on`). Beim Speichern entscheidet sie: Ist sie die Spitze ihres Zweigs, wird dort angehängt; sonst muss ein neuer Zweigname angegeben werden. `aktiver_zweig` in `dokument.json` ist nur der Zweig, dessen Spitze beim Öffnen geladen wird. | Ein Dokument kann Ansichten in mehreren Tabs haben; der Zweig darf nicht von einer globalen Einstellung abhängen. |
| P7 | `AppState::scene_version` stammt aus einem globalen Zähler. | Beim Wechsel zwischen Tabs muss die GPU-Szene sicher neu hochgeladen werden. |
| P8 | `Config { root }` liegt in `planora-project`; Speicherort `%APPDATA%\PlanoraCAD\config.json` bzw. `$XDG_CONFIG_HOME/planora-cad/config.json` (Rückfall `~/.config`). | Dort ist `serde` schon vorhanden; keine neue Abhängigkeit. |
| P9 | Autosicherung nur für gespeicherte Dokumente. Abweichung von §7 (Konfigurationsordner für unbenannte). | Unbenannte Dokumente haben keinen Ort; später nachrüstbar. |
| P10 | *Wiederherstellen* aus einer Ansicht: Ist das Dokument schon beschreibbar in einem Tab offen, wirkt es in diesem Tab (nur ohne ungespeicherte Änderungen); sonst wird die Ansicht bearbeitbar (holt die Sperre). Scheitert das, wird der Tab wieder zur Ansicht und die Sperre frei. | Zwei beschreibbare Tabs desselben Dokuments sind wegen der Sperre ausgeschlossen. |

### Tatsächliche Abweichungen und Ergänzungen

**Skizzen und Format**
- `Sketch::from_data` prüft alles: die Typen der Constraint-Argumente (auch bei gelöschten Einträgen), Wertebereiche, Punktzahlen von Splines und dass jede lebende Abhängigkeit, die die zusätzliche Löservariable braucht (Punkt auf Spline, Tangente mit Ellipse oder Spline), diese Variable auch hat. Grund: Ohne diese Prüfungen konnte eine verfälschte Datei beim späteren Lösen abstürzen (Fund der Prüfung von Aufgabe 2).
- `encode_version` prüft sich selbst, indem es das Geschriebene wieder dekodiert. Format 0 ist ungültig; ein Byte-Order-Mark am Dateianfang wird ignoriert. Fehlertexte nennen das Feature.
- Zwei Referenzdateien statt einer: `format1.json` (kleines Dokument) und `format1_voll.json` (alle Feature-Arten, alle Kurven- und Abhängigkeitsarten, Fläche und Kantenkopie, Konstruktions- und Maß-Flags). Die Golden-Tests vergleichen den erzeugten Text mit der Datei, nicht nur das Dokument. `.gitattributes` setzt für diese Dateien LF-Zeilenenden, sonst schlüge der Test nach einem CRLF-Checkout fehl.
- `Document::from_snapshot` lehnt `next_id == u64::MAX` ab; `add_feature` meldet eine erschöpfte Id-Zählung, statt zu überlaufen.
- Feste Grenzen (benannte Konstanten): Listen in einer Datei höchstens eine Million Einträge, Zweignamen 60 Zeichen, Kommentare 500, Namen 80, höchstens 100 000 Versionen je Dokument; `COM`/`LPT` mit hochgestellten Ziffern gelten als reserviert.

**Dokumentspeicher (`DocStore`)**
- `dokument.json` enthält zusätzlich `hoechste_nummer`: Versionsnummern werden nie wiederverwendet, auch nicht nach dem Löschen der Spitze. Ältere Dateien ohne das Feld laden (der Wert wird aus dem Baum bestimmt).
- Beim Laden wird der Versionsbaum vollständig geprüft (Nummern aufsteigend, Eltern vorhanden und früher, Spitzen, Zweignamen, aktiver Zweig, Zeit- und Kommentarform). Die Prüfung ist linear; ein Baum mit 20 000 Versionen lädt sofort.
- Zu neue Dateien werden am Formatkopf erkannt (`TooNew`), auch wenn sich der Aufbau geändert hat, und nicht als beschädigt gemeldet. Das gilt auch für Autosicherungen.
- `peek` und `read_version` lesen Dokumentinfo und einzelne Versionen ohne Sperre; die Ansichten des Versionen-Panels nutzen sie.
- `save` ohne Version und mit Zweigname legt einen neuen Zweig an der aktiven Spitze an; Speichern auf einer alten Version ohne Zweigname wird mit `NeedsBranch` abgelehnt.
- Schreibt ein Speicher, dessen Sperre inzwischen von einem anderen Programm übernommen wurde, schlägt jede Schreiboperation mit `LockLost` fehl, ohne etwas zu verändern.
- `create` räumt den eben angelegten Ordner bei einem Fehler wieder auf; `open` räumt liegengebliebene `.tmp`-Dateien auf.
- `atomic_write` entfernt die `.tmp`-Datei, wenn das Schreiben scheitert. Der Papierkorbname nutzt die mittleren Zeichen der Id (die ersten acht kollidierten bei zwei Löschungen in derselben Sekunde).
- Der Verzeichnisbaum überspringt Symlinks und Junctions sowie Einträge, deren Namen die Pfadprüfung ablehnt; Projektnamen dürfen nicht auf `.pcad` enden; reine Groß-/Kleinschreibungsänderungen beim Umbenennen sind erlaubt.

**Autosicherung**
- Eine nicht lesbare Autosicherung (beschädigt oder zu neu) wird nie gelöscht und nie überschrieben: Sie wird gemeldet, und der Tab bekommt in dieser Sitzung keine eigene Autosicherung.
- Angebote stehen in einer Warteschlange: `Workspace::refresh_prompts()` (jedes Bild vor den Rückfragen) bringt das nächste Angebot hervor; solange eines offen ist, kehren `request_close` und `request_quit` mit `false` zurück. Ein Tab mit offenem Angebot wird nicht automatisch gesichert, und Speichern löscht die Datei nicht, solange das Angebot offen ist.
- Keine Autosicherung für offene Skizzenentwürfe und (P9) für unbenannte Dokumente. Ein fehlgeschlagenes Schreiben wird erst nach dem nächsten Intervall wiederholt. Beim Verwerfen eines geänderten Tabs bleibt die Autosicherung liegen und wird beim nächsten Öffnen wieder angeboten.

**Oberfläche**
- Das Schließen des Fensters wird in `App::logic` behandelt (`handle_close_request`), nicht in `ui()`: eframe ruft bei minimiertem oder verdecktem Fenster nur `logic` auf; sonst ginge das Fenster ohne Rückfrage zu. Bei einer Schließanfrage mit Änderungen wird das Fenster wiederhergestellt und fokussiert, die Anfrage abgebrochen und gefragt. `Close` wird nur einmal gesendet. Dort läuft auch die Autosicherung, mit einem Wiederholungstakt von 30 Sekunden.
- Workaround für das erste Bild: egui 0.36 zeichnet ein neues Fenster im ersten Durchlauf unsichtbar. Die Rückfragefenster verwerfen diesen Durchlauf (`request_discard`) und schalten die Einblendung ab; Tastenkürzel laufen deshalb nur im ersten Durchlauf.
- Der Punkt „●“ am Tab wird gezeichnet: Roboto enthält U+25CF nicht. Das Zeichen steht weiter im Text (für Tests und Bildschirmleser), ist aber unsichtbar.
- Solange eine Rückfrage offen ist, sind Menü, Tab-Leiste, Projekt-Panel und Versionen-Panel gesperrt, und Tastenkürzel werden ignoriert; sonst könnte ein Klick eine wartende Beenden- oder Autosicherungs-Rückfrage ersetzen.
- Das Fenster „Speichern unter“ zeigt den Projektbaum; ohne Projekt weist der Text auf das Projekt-Panel hin.
- Angeordnet sind Projekte, Browser, 3D-Ansicht und rechts als Tabs Eigenschaften und Versionen. „Projekte“ blendet den ganzen Container aus, „Versionen“ nur seinen Tab (Menü „Ansicht“).
- Das Projekt-Panel liest den Baum nur nach Aktionen, „Aktualisieren“, Wurzelwechsel und Strg+O; auch ein Lesefehler wird zwischengespeichert. Umbenennen, Verschieben und Löschen verlangen geschlossene Tabs (auch Ansichten). „Wurzel wählen“ ist nur ohne offene gespeicherte Dokumente möglich, weil Tab-Pfade zur Wurzel relativ sind; die Prüfung liegt im Panel, nicht in `Workspace::choose_root`.
- Das Versionen-Panel liest keine Dateien je Bild: Beschreibbare Tabs nutzen den Stand im Speicher, reine Ansichten eine einmal gelesene und zwischengespeicherte Kopie. Löschen verlangt eine Rückfrage; die Fenster für Kommentar und Löschen gehören zu einem Tab und werden verworfen, wenn der Tab wechselt.
- `Workspace::delete_version` und `delete_branch` führen `based_on` der Tabs nach: Verschwindet die Ausgangsversion, zeigt der Tab auf die neue Spitze des Zweigs (sonst des aktiven Zweigs), damit er speicherbar bleibt.
- „Zu diesem Zweig wechseln“ öffnet die Spitze als Ansicht; der Arbeitsstand des beschreibbaren Tabs bleibt.
- Ein geöffnetes Dokument übernimmt das Thema (hell/dunkel) des aktiven Tabs.
- Die Namenszähler („Quader n“) laufen nach dem Laden weiter; Zahlen über 1 000 000 in geladenen Namen werden ignoriert, und die Zähler rechnen überlaufsicher.

**Nicht umgesetzt oder offen**
- Kein Test mit einem echten zweiten Prozess für die Sperre (P4).
- Die Handprüfung nach §8 ist noch offen; siehe `offene-punkte.md`.
