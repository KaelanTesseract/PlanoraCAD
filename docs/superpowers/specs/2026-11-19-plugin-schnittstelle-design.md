# Plug-in-Schnittstelle (Vorhaben 1)

Stand 2026-11-19. Entscheidungen von Dennis aus dem Brainstorming am 2026-11-19. Was hier als **Vorschlag (Claude)** steht, hat Claude entschieden; Dennis kann es beim Lesen korrigieren. Zweig: `feature/plugins` ab master (Claude, siehe „Zweig“).

## Warum

PlanoraCAD soll Erweiterungen bekommen, die **jeder** schreiben kann, auch Fremde, in Rust, C/C++, Go oder Python. Das erste große Plug-in wird die Elektronik (E1 Bauteilbibliothek, E2 Schaltplan, E3 Platine, heute fest eingebaut auf `feature/platinen`). Dahinter steht: Wer nur mechanisch zeichnet, soll von der Elektronik nicht gestört werden; wer sie beim Installieren abwählt, bekommt sie gar nicht installiert. Dateien mit Elektronik-Daten bleiben auch ohne das Plug-in verlustfrei erhalten und werden mitgespeichert.

Es gibt drei Vorhaben, jedes einzeln geplant:
1. **die Plug-in-Schnittstelle selbst** (dieses Dokument), in drei Etappen,
2. Elektronik auf die Schnittstelle umziehen,
3. Installer-Komponente, Updates, „Plug-in nachinstallieren“, später ein Katalog.

**Gelungen ist Vorhaben 1, wenn**
- man eine Plug-in-Datei über „Plug-in installieren…“ auswählt, die verlangten Rechte sieht, bestätigt und das Plug-in ohne Neustart im Menü erscheint;
- das Beispiel-Plug-in einen Befehl mit Tastenkürzel in der Keymap, einen Dialog, einen Baumknoten und einen eigenen Dateiabschnitt hat, Rückgängig und Wiederholen dafür wie bei jedem anderen Schritt funktionieren;
- eine Datei mit Daten eines Plug-ins, das nicht installiert ist, geöffnet, mechanisch bearbeitet und gespeichert wird und die Plug-in-Daten danach Byte für Byte noch da sind;
- ein Beispiel-Arbeitsbereich eine eigene Zeichenfläche mit ein paar tausend Elementen zeigt, die sich beim Ziehen so flüssig anfühlt wie die heutige Platine, und diese Fläche in ein zweites Fenster gelöst werden kann;
- ein absichtlich kaputtes Plug-in (Endlosschleife, Speicherhunger, Absturz) das Programm nicht mitreißt: es wird angehalten, gemeldet und abgeschaltet, das Dokument bleibt unversehrt.

**Fehlschlag wäre:**
- ein Plug-in kommt ohne Zustimmung an Dateien, Netz oder das mechanische Modell;
- Dateien ohne Plug-in-Daten werden nicht mehr bytegleich geschrieben oder bekommen eine neue Formatnummer;
- Daten eines fehlenden Plug-ins gehen beim Speichern verloren oder werden verändert;
- ein Plug-in sieht anders aus als das Programm (eigene Farben, eigene Schrift, folgt nicht hellem/dunklem Design oder der Sprache);
- das Programm ruckelt, weil es bei jedem Bild auf ein Plug-in wartet;
- ein Plug-in-Fehler verliert ungespeicherte Arbeit.

## Ziele, Nicht-Ziele

**Ziele (Vorhaben 1):** Laufzeit, versionierter Vertrag, Paketformat, Installation aus Datei, Rechte, Plug-in-Verwaltung in den Einstellungen, Dateiabschnitte, Befehle/Menüs/Keymap/Baum/Dialoge, Arbeitsbereiche mit Leisten und Zeichenfläche, zweites Fenster, Beispiel-Plug-ins in Rust, Entwickler-Doku.

**Nicht-Ziele:** Umzug der Elektronik (Vorhaben 2); Installer-Komponente, Katalog, Signaturprüfung gegen einen Schlüssel (Vorhaben 3; das Paketformat hat aber schon den Platz dafür); Beispiel-Plug-ins in C/C++, Go, Python (Vorlagen und Doku genügen; gebaut wird nur Rust, **Vorschlag (Claude)**: ein Python-Hallo-Welt nur als Rauchtest, wenn componentize-py ohne Aufwand läuft); Plug-ins, die eigene 3D-Körper berechnen oder in den Modellierkern eingreifen.

**Anti-Ziele:** Plug-ins zeichnen keine freien Pixel und bekommen keinen egui-Zugang; kein Plug-in-Code läuft außerhalb der Abschottung, auch nicht die mitgelieferten.

## Entscheidungen von Dennis

1. **Bauart A, WebAssembly** mit **wasmtime und Component Model/WIT** (Weg 1 von drei; verworfen: wasmi mit eigenem Datenaustausch, zu langsam für große Platinen und keine Sprachanschlüsse; Extism, untypisierter JSON-Vertrag bei gleicher Größe).
2. **Oberfläche beschreiben (Weg A):** Das Plug-in beschreibt, was es zeigen will; das Programm zeichnet alles selbst. Verworfen: eigenes egui im Plug-in mit fertigen Zeichenlisten (billigerer Umzug der Elektronik, aber uneinheitliches Aussehen, größere Plug-ins, weniger Kontrolle bei fremdem Code).
3. **Herkunft:** fremde Plug-ins kommen **als Datei** oder über den **Link ihres Repos** (Dennis, 2026-11-19, in Etappe 1): „Plug-in installieren…“ nimmt eine Datei oder einen Repo-Link; ein über den Link installiertes Plug-in hält sich darüber aktuell. Katalog später (Vorhaben 3; dann eine Liste solcher Links). Mitgelieferte Plug-ins kommen über den Installer und werden mit dem Programm aktualisiert.
4. **Lizenz:** Plug-ins müssen **GPL-verträglich** lizenziert sein (Dennis, 2026-11-19; verworfen: eine Plug-in-Ausnahme nach §7 GPLv3, die jede Lizenz erlaubt hätte). Siehe „Lizenz der Plug-ins“.
5. **Rechte:** wie unten unter „Rechte“, Nachfrage **beim ersten Gebrauch**, pro Plug-in gemerkt, in den Einstellungen widerrufbar.
6. Elektronik ist das erste Plug-in; die Schnittstelle muss ganze Arbeitsbereiche tragen (eigene Zeichenfläche, Leisten, Dialoge, Menüs, Keymap, Baumknoten, Dateiabschnitte, Rückgängig im selben Schritt, zwei Fenster).

## Aufbau

### Neue Crates

- **`planora-plugin-api`**: die WIT-Dateien (`wit/`), Paket `planora:plugin@0.1.0` bis zur ersten Freigabe, danach `@1.0.0`. Lizenz wie das Programm: GPL-3.0-only (siehe „Lizenz der Plug-ins“). Enthält keinen Host-Code.
- **`planora-plugin-host`**: wasmtime-Einbettung, Laden, Grenzen, Rechteprüfung, Übersetzung zwischen WIT-Typen und Programmtypen. Ohne egui; testbar ohne Oberfläche.
- **`planora-plugin-sdk`** (Rust, GPL-3.0-only): dünne Hülle um die mit `wit-bindgen` erzeugten Anschlüsse, Makro `planora_plugin!`, Hilfen für Zeichenbefehle und Dialogbeschreibungen. Wird nicht ins Programm gebaut.
- **`plugins/beispiel-hallo`** und **`plugins/beispiel-flaeche`**: Beispiel-Plug-ins (Rust → `wasm32-wasip2`). Gehören nicht zum Workspace-Build des Programms (eigener Workspace oder `exclude`), damit `cargo test` im Hauptprojekt kein Wasm-Ziel braucht; ihre fertigen `.wasm` liegen als Testdaten im Repo (**Vorschlag (Claude)**, hält CI und WSL einfach; ein Skript `tools/build-plugins.ps1`/`.sh` baut sie neu, ein Test prüft, dass die eingecheckte Datei zum Quelltext passt, über einen Hash in einer Nachbardatei).

Die App (`planora-app`) bekommt die Oberflächenseite: Plug-in-Verwaltung, Rechte-Dialog, Zeichnen der beschriebenen Oberfläche, Einbindung in Keymap, Baum, Undo, Detach.

### Vertrag (WIT)

Zwei Richtungen:
- **Plug-in exportiert** (`world plugin`): `init(host-info) -> registration`, `run-command(id, context)`, `handle-dialog(event)`, `handle-canvas(event)`, `section-upgrade(old-schema, bytes) -> bytes`, `tree-nodes(document-view)`, `shutdown()`.
- **Programm stellt bereit** (Imports, je ein WIT-Interface): `log`, `i18n` (Texte aus den mitgelieferten Sprachdateien des Plug-ins), `document` (eigener Abschnitt lesen/schreiben in einer Transaktion), `ui` (Dialoge, Leisten, Meldungen), `canvas` (Zeichenbefehle), `settings` (eigene Einstellungen), `storage` (eigener Datenordner), dazu die rechtepflichtigen `net`, `fs-folder`, `model-read`, `model-write`.

Versionierung: WIT-Paket mit Semver. Das Programm lädt Plug-ins, deren verlangte API-Version die gleiche Hauptversion und eine Nebenversion ≤ der eigenen hat. Neue Funktionen kommen nur in neuen Nebenversionen dazu; Entfernen oder Ändern heißt neue Hauptversion. Ein Plug-in mit zu neuer oder fremder Hauptversion wird nicht geladen und in der Verwaltung mit Grund angezeigt („braucht PlanoraCAD-Schnittstelle 2.x“). Bis Vorhaben 2 fertig ist, bleibt die Version `0.x` und darf brechen; unter 1.0 lädt das Programm nur Plug-ins mit **genau** seiner `0.y` (Vorschlag (Claude)). Vor `1.0.0` wird der Vertrag gegen die umgezogene Elektronik geprüft.

### Paketformat

Eine Datei `name.pcplugin` = ZIP mit:
- `plugin.toml`: `id` (umgekehrter Domainname oder `planora.<name>`, eindeutig), `name`, `version` (Semver), `api` (z. B. `"0.1"`), `author`, `license`, `description`, `rights = [...]` (nur rechtepflichtige), `net-hosts = [...]` falls `net`.
- `plugin.wasm` (Komponente),
- optional `languages/de.ftl`, `languages/en.ftl`, `icons/*.svg|png`,
- optional `signature` (Platz für Vorhaben 3; wird jetzt gelesen und angezeigt als „nicht signiert“/„signiert, nicht geprüft“, nicht geprüft).

Grenzen beim Lesen wie bei Projektdateien (Größe, Anzahl Einträge, keine Pfade mit `..`), **Vorschlag (Claude)**: Paket ≤ 64 MB, Wasm ≤ 32 MB.

### Installieren und Aktualisieren über den Repo-Link

- **Link:** in Etappe 1 nur GitHub: `https://github.com/<besitzer>/<repo>` (auch ohne `https://`, mit `/` oder `.git` am Ende). Andere Adressen werden mit Hinweis abgelehnt (**Vorschlag (Claude)**; weitere Hoster später).
- **Quelle:** die Releases des Repos über `https://api.github.com/repos/<besitzer>/<repo>/releases`. Entwürfe zählen nie; Vorabversionen nur, wenn der Update-Kanal des Programms sie einschließt (dieselbe Regel wie bei den Programm-Updates). Ein Release zählt, wenn es **genau eine** Datei `*.pcplugin` hat; liegt daneben `<datei>.sha256`, wird die Prüfsumme geprüft.
- **Prüfungen:** Tag (mit oder ohne `v`) = `version` in `plugin.toml`; bei einem Update gleiche `id` wie das installierte Plug-in und höhere Version (nie zurück). Download nur über HTTPS von `github.com`, `api.github.com` und `*.githubusercontent.com`, Weiterleitungen nur dorthin (wie bei den Programm-Updates), Paketgrenze 64 MB. Das Programm lädt selbst; das Plug-in hat damit nichts zu tun und braucht kein Recht.
- **Herkunft merken:** im Plug-in-Ordner `herkunft.json` mit Link und installiertem Tag (**Vorschlag (Claude)**: gehört zur Installation und verschwindet mit ihr).
- **Aktuell halten:** beim Start einmal am Tag (dieselbe Regel wie die Update-Prüfung des Programms, abschaltbar mit demselben Schalter) und mit dem Knopf „Nach Updates suchen“ in den Einstellungen. Gibt es neue Versionen, ein Hinweis „Für N Plug-ins gibt es Updates“; im Abschnitt „Plug-ins“ steht beim Plug-in „Aktualisieren“. Der Aktualisieren-Dialog zeigt alte und neue Version, die Release-Notizen und **neu verlangte Rechte**; erteilte Rechte bleiben, neue werden beim ersten Gebrauch gefragt. Danach läuft die neue Version ohne Neustart.
- Mitgelieferte Plug-ins haben keinen Link; ein über den Link installiertes Plug-in gleicher `id` überdeckt sie nach der bestehenden Versionsregel.

### Fundorte

- **Mitgeliefert:** `<Programmordner>/plugins/<id>/` (Installer; unter Ubuntu `<prefix>/share/planoracad/plugins/`). Nur lesbar.
- **Vom Nutzer:** `%APPDATA%\PlanoraCAD\plugins\<id>\` bzw. `~/.local/share/planoracad/plugins/<id>/`. „Plug-in installieren…“ entpackt die Datei dorthin.
- Gleiche `id` in beiden: das höhere `version` gewinnt, die Verwaltung zeigt beide.
- Zustand (an/aus, erteilte Rechte) in der Programmkonfiguration (`planora-project/src/config.rs`), pro `id`.
- Übersetzter Maschinencode wird zwischengespeichert (eigener Zwischenspeicher im Benutzer-Cacheordner, siehe Rahmenbedingungen), damit der Start schnell bleibt. **Vorschlag (Claude):** Plug-ins laden beim Start im Hintergrund; ihre Menüeinträge erscheinen, sobald sie bereit sind.

### Laufzeit und Abschottung

- Jedes Plug-in hat einen eigenen wasmtime-`Store`; kein gemeinsamer Speicher, kein WASI-Dateisystem, keine Umgebungsvariablen, keine Uhr außer einer groben Monotonuhr, kein Netz außer über `net`.
- **Grenzen:** Speicher pro Plug-in (Vorschlag (Claude): 512 MB, in `plugin.toml` bis 2 GB anforderbar, wird beim Installieren angezeigt), Zeit über wasmtime-Epochen: ein Aufruf aus der Oberfläche darf **Vorschlag (Claude)** 200 ms dauern, ein Befehl 5 s; danach Abbruch.
- **Fehler** (Trap, Zeitüberschreitung, Speichergrenze): Aufruf abgebrochen, eine offene Transaktion verworfen (Dokument bleibt wie vorher), Meldung im Programm: „Das Plug-in X wurde wegen eines Fehlers angehalten“ mit Knopf „Neu starten“; nach drei Fehlern in einer Sitzung bleibt es bis zum Neustart aus. Eintrag ins Protokoll (bestehendes Protokoll-Modul).
- Aufrufe laufen auf dem Oberflächen-Thread, aber kurz (siehe Zeitgrenze). wasmtime wird synchron benutzt.
- **Nichts wartet im Aufruf (Vorschlag (Claude)):** Alles, was auf den Nutzer oder das Netz warten muss – Rechte-Nachfrage, Datei-Dialog, `net`-Anfrage, Bestätigungsfrage –, ist eine **Anfrage mit Kennung**: der Host-Aufruf kehrt sofort mit `request-id` zurück, das Ergebnis kommt später als Ereignis `on-result(request-id, ergebnis)` an das Plug-in. Die Zeitgrenze misst damit nur echtes Rechnen. Netzanfragen führt das Programm auf seinem bestehenden Hintergrund-Weg (`jobs.rs`) aus. Ein Plug-in kann Rechte auch vorab anfragen (`rights.request`), z. B. beim Öffnen eines Dialogs.
- **Langes Rechnen** (z. B. Prüfregeln einer Platine) ist in Vorhaben 1 nicht gebaut. Weil schon alles über Ereignisse mit Kennung läuft, passt ein späteres `job`-Interface (zweite Instanz des Plug-ins im Hintergrund, Ergebnis als `on-result`) ohne Bruch als Nebenversion dazu. Für Vorhaben 2 erneut prüfen.

### Rechte

Ohne Nachfrage (Grundausstattung):
- eigene Abschnitte im Dokument lesen und schreiben (mit Rückgängig),
- Oberfläche über die Bausteine beschreiben,
- eigene Einstellungen, eigener Datenordner (`storage`, Plattengrenze Vorschlag (Claude) 1 GB),
- Dateien, die der Nutzer im **Datei-Dialog des Programms** wählt (Import/Export): das Plug-in fordert den Dialog an, bekommt nur den Inhalt bzw. einen Schreibgriff auf genau diese Datei, nie den Pfad als freien Zugang.

Mit Nachfrage beim ersten Gebrauch:
- `net`: nur HTTPS zu den Hosts aus `net-hosts`; das Programm führt die Anfrage aus (bestehender HTTP-Weg aus `planora-update`), Größen- und Zeitgrenzen.
- `fs-folder`: ein Ordner, den der Nutzer im Nachfragedialog auswählt, dauerhaft lesbar (optional schreibbar, eigener Haken).
- `model-read`: Geometrie und Namen des mechanischen Modells lesen.
- `model-write`: mechanisches Modell verändern. In Vorhaben 1 nur Körper einfügen, ersetzen und entfernen, die das Plug-in selbst eingefügt hat, und zwar als **Bauanleitung**, nicht als fertige Geometrie (Vorschlag (Claude)): (a) ein Profil aus Linien und Bögen mit Löchern, das um eine Dicke ausgetragen wird (Platinenkörper mit Bohrungen und Ausschnitten), (b) ein mitgebrachtes Modell als STEP-Bytes oder Dreiecksnetz mit Lage (Bauteilmodelle). Das Programm baut daraus echte Körper mit dem eigenen Kern. Ob das für die 3D-Platine reicht, wird vor `1.0.0` in Vorhaben 2 geprüft.

Ablauf: Beim Installieren zeigt das Programm Name, Autor, Version, Lizenz, Signaturzustand und **alle** verlangten Rechte. Ruft das Plug-in ein rechtepflichtiges Interface zum ersten Mal auf, fragt ein Dialog: „Erlauben“, „Nur diesmal“, „Ablehnen“. Die Antwort wird pro Plug-in gemerkt (außer „Nur diesmal“, gilt bis Programmende). Abgelehnt heißt: der Aufruf gibt einen Fehler zurück, das Plug-in muss damit umgehen. Rechte, die nicht in `plugin.toml` stehen, sind nie erreichbar. Mitgelieferte Plug-ins bekommen ihre Rechte vom Installer gesetzt (Konfigurationseintrag), laufen aber genauso abgeschottet.

Einstellungen → neue Seite **„Plug-ins“**: Liste (Name, Version, Herkunft mitgeliefert/installiert, an/aus, Zustand), je Plug-in die Rechte mit Zustand und „Zurücknehmen“, „Entfernen“ (nur bei installierten), „Plug-in installieren…“.

### Dateiabschnitte

- `Document` bekommt eine Tabelle **Plug-in-Abschnitte**: `id` → `{ schema: u32, daten: Bytes }`. Die Bytes gehören dem Plug-in (Vorschlag (Claude): das SDK bietet JSON und eine kompakte Binärform an; das Programm schaut nicht hinein).
- Da `History<T>` den ganzen Zustand als Schritt speichert, ist eine Plug-in-Änderung automatisch Teil eines Rückgängig-Schritts. Ein Plug-in ändert nur über `document.begin(label)` … `commit()`; ein Commit ist **ein** Schritt mit dem übersetzten Label. Ein Befehl, der auch das mechanische Modell ändert (`model-write`), erzeugt trotzdem nur einen Schritt.
- Größenzählung für die Undo-Grenzen: die Bytes des Abschnitts zählen mit; Vorschlag (Claude): gleiche Bytes zwischen Schritten werden geteilt (`Arc`), damit kleine Änderungen nicht jedes Mal den ganzen Abschnitt kopieren.
- **Fehlt das Plug-in**, bleibt der Abschnitt unverändert im Dokument und wird Byte für Byte so geschrieben. Beim Öffnen ein Hinweis: „Diese Datei enthält Daten von ‚X‘ (nicht installiert). Sie bleiben erhalten.“ Im Baum ein grauer Knoten „X (nicht installiert)“.
- **Schema-Version:** Ist `schema` älter als die des Plug-ins, ruft das Programm `section-upgrade` beim Öffnen auf; neuer als die des Plug-ins: Abschnitt bleibt unberührt, nur lesend, Hinweis „mit neuerer Version von X erstellt“.
- **Dateiformat:** neue Formatnummer **23** (nächste freie auf master; `FORMAT_VERSION` ist dort 22, `feature/platinen` belegt 23/24 – die Platinen-Nummern werden beim späteren Zusammenführen wieder umnummeriert, wie schon einmal). Neues Feld `plugin_abschnitte` in jeder Version (`versionen/NNNN.json`), mit `serde(default, skip_serializing_if = "is_empty")`: Dokumente ohne Plug-in-Daten bleiben bytegleich und bei ihrer alten Nummer (Muster wie `bauteile` auf `feature/platinen`). Daten als Base64 im JSON (Vorschlag (Claude); einfacher als eine Nachbardatei, für große Platinen in Vorhaben 2 prüfen).
- Gleiche Behandlung in Zwischenablage, Versionsvergleich und Vorschaubild: das Programm ignoriert fremde Abschnitte.

### Befehle, Menüs, Keymap, Baum

- `registration` meldet Befehle: `id` (im Namensraum `<plugin-id>.<befehl>`), Text-Schlüssel, Symbol, vorgeschlagenes Kürzel, Ort (Menü, Leiste eines Arbeitsbereichs, Rechtsklickmenü eines eigenen Baumknotens), Bedingung („nur in Arbeitsbereich Y“, „nur mit offenem Dokument“).
- Die Befehle erscheinen in der Keymap (`crates/planora-app/src/keymap`) wie eingebaute. Die Keymap kennt heute nur die feste `enum Action` (`keymap/defaults.rs`); **Vorschlag (Claude):** die Enum bleibt, daneben kommt ein Ziel `Plugin(String)` (Befehls-id), also `enum Ziel { Eingebaut(Action), Plugin(String) }` in Belegungen, Konfliktprüfung, Suche und Einstellungsseite. Die Konfiguration speichert ohnehin Ids als Text, also kein Formatbruch. Die Befehle sind in den Einstellungen änderbar; das vorgeschlagene Kürzel wird nur belegt, wenn es frei ist, sonst ohne Kürzel (Hinweis in der Verwaltung). Belegungen werden in der Keymap-Konfiguration unter der Befehls-id gemerkt und bleiben, wenn das Plug-in fehlt.
- Menü: Plug-ins ohne eigenen Arbeitsbereich erscheinen unter **„Hilfsprogramme → Zusatzmodule“** (wie Fusions „UTILITIES → ADD-INS“, Vorschlag (Claude), Ort beim Planen gegen die bestehenden Menüs prüfen).
- Baum: das Plug-in liefert Knoten (Text, Symbol, Kinder, Augen-Schalter, Rechtsklickbefehle) unter einem Wurzelknoten des Plug-ins; das Programm fragt sie nur nach einer Änderung des Abschnitts neu ab.
- Texte: das Plug-in bringt Fluent-Dateien mit (`planora-i18n`); Schlüssel sind auf den Namensraum des Plug-ins beschränkt. Fehlende Sprache → Englisch → Schlüssel.

### Dialoge und Leisten (beschriebene Oberfläche)

Bausteine (an E1–E3 ausgerichtet, das Planen prüft die Liste gegen deren Dialoge):
Beschriftung, Zahl mit Einheit (bestehendes Formelfeld mit Parametern), Text, Auswahl (Liste/Ausklapp), Haken, Knopfgruppe, Auswahlfeld „im Modell/in der Fläche wählen“, Tabelle (Spalten, Zeilen, Auswahl), Bild/Vorschau (Zeichenbefehle wie die Fläche), Gruppe/Aufklappbereich, Trennlinie.

- Ein Dialog wird als Baum beschrieben, das Programm zeichnet ihn im Fusion-Dialogstil (bestehendes `dialog.rs`). Änderungen gehen als Ereignis an das Plug-in, das mit einer neuen Beschreibung oder einem kleinen Teil-Update antwortet. „OK“ führt den Befehl als ein Rückgängig-Schritt aus.
- Leisten: Arbeitsbereichs-Leiste im Fusion-Stil mit Gruppen und Knöpfen, die Befehle auslösen.

### Arbeitsbereiche und Zeichenfläche (Etappe 3)

- Ein Plug-in kann einen **Arbeitsbereich** anmelden, der in der Arbeitsbereichsauswahl neben „Konstruktion“ und „Zeichnung“ steht (wie Fusions Arbeitsbereich-Umschalter). Er hat eine Leiste, einen Baum und eine Zeichenfläche.
- **Zeichenbefehle:** Linie, Polylinie, Bogen, Kreis, gefüllte Fläche (mit Löchern), Text (Programmschrift), Bild aus dem Paket; Farbe als **Rolle** aus dem Design (Vordergrund, Auswahl, Hervorhebung, Gitter …) oder als eigene Farbe für Daten wie Lagenfarben; Strichbreite in Welt- oder Bildschirmeinheiten.
- **Ebenen:** Befehle liegen in benannten Ebenen mit Reihenfolge und Sichtbarkeit; das Plug-in ersetzt nur geänderte Ebenen oder einzelne Elemente mit Kennung (`upsert`, `remove`). Das Programm speichert sie zwischen und zeichnet jedes Bild selbst (Ziel: Ziehen bei 2000 Elementen so flüssig wie heute, ≤ 1 ms pro Bild Programmseite; **Vorschlag (Claude)** als Messtest).
- **Ziehen ohne Rundweg:** Für Verschieben einer Auswahl meldet das Plug-in eine „Zieh-Ebene“; das Programm verschiebt sie beim Ziehen selbst und meldet erst beim Loslassen das Ergebnis. Fangen an Gitter und an Punkten, die das Plug-in als Fangpunkte meldet, macht das Programm.
- **Ansicht:** Verschieben, Zoomen, Gitter, Fadenkreuz, Lineal macht das Programm; die mittlere Maustaste und Tab-Belegung folgen der Programm-Keymap (offener Punkt aus E3, hier gleich richtig).
- **Ereignisse:** Maus (Weltkoordinaten, Tasten, Modifikatoren), Treffer-Abfrage („was liegt hier“, vom Plug-in beantwortet oder über Kennungen der Elemente vom Programm), Tasten, die keinem Befehl gehören.
- **Zweites Fenster:** Eine Zeichenfläche lässt sich über das bestehende `detach.rs` in ein eigenes Fenster lösen; Schließen mit X dockt sie wieder an (wie die Platine heute). Zwei Flächen eines Plug-ins gleichzeitig (Schaltplan und Platine) sind möglich.

## Etappen

1. **Grundgerüst:** Crates `planora-plugin-api`, `-host`, `-sdk`; wasmtime-Einbettung mit Grenzen; Paketformat, Installation aus Datei und über den Repo-Link mit Updates, Fundorte; Einstellungsseite „Plug-ins“; Rechte mit Nachfragedialog (an `net` beispielhaft); Fehlerbehandlung; Befehl im Menü „Zusatzmodule“, Meldung, einfacher Dialog; Beispiel-Plug-in `beispiel-hallo`; cargo-deny für wasmtime; Entwickler-Doku (erste Fassung) und Benutzer-Wiki-Seite.
2. **Dateiabschnitte und Befehle:** Plug-in-Abschnitte im Dokument mit Rückgängig, neue Formatnummer, Erhalt fremder Abschnitte, Schema-Upgrade; Keymap, Baumknoten, alle Dialogbausteine, Datei-Dialog-Zugriff, `fs-folder`, `model-read`, `model-write`; `beispiel-hallo` erweitert (Zähler im Dokument, Baum, Import einer Textdatei).
3. **Arbeitsbereiche mit Zeichenfläche:** Arbeitsbereich, Leiste, Zeichenbefehle, Ebenen, Ziehen, Fangen, Ereignisse, zweites Fenster; `beispiel-flaeche` (einfacher Zeichen-Arbeitsbereich mit Rechtecken und Linien), Messtest.

Jede Etappe endet mit Handtest (vorher Dennis fragen), Gesamtprüfung, Vault-Notiz.

## Lizenz der Plug-ins

- PlanoraCAD bleibt GPL-3.0-only ohne Ausnahme; API (WIT-Dateien), SDK und Beispiel-Plug-ins ebenso.
- Fremde Plug-ins müssen unter einer GPL-3.0-verträglichen Lizenz stehen (z. B. GPL-3.0, MIT, Apache-2.0, BSD). Die Entwickler-Doku sagt das gleich am Anfang.
- `plugin.toml` verlangt `license` als SPDX-Ausdruck. Beim Installieren zeigt das Programm die Lizenz an; steht sie nicht auf der bekannten Liste verträglicher Lizenzen (dieselbe Liste wie `allow` in `deny.toml`), zeigt es einen Hinweis „Lizenz unbekannt oder nicht GPL-verträglich“, verhindert die Installation aber nicht (Vorschlag (Claude): das Programm kann Lizenzen nicht durchsetzen, nur darauf hinweisen).

## Rahmenbedingungen

- Alles in Deutsch, Meldungen und Texte über `planora-i18n` (de/en).
- Plattformen Windows und Ubuntu; Ubuntu-Pfade wie oben.
- **cargo-deny:** wasmtime und Abhängigkeiten (Lizenzen Apache-2.0 WITH LLVM-exception ist schon erlaubt; neue Lizenzen einzeln prüfen), `wit-bindgen` nur in SDK/Beispielen. Kein Autodesk-Material.
- Platz auf D: knapp: volle Testläufe mit `CARGO_INCREMENTAL=0` und `CARGO_PROFILE_DEV_DEBUG=0`; wasmtime nur mit den nötigen Features (`cranelift`, `component-model`, `runtime`, `parallel-compilation`; ohne `wat`-Parser im Programm, nur in Tests). **Abweichung (Etappe 1):** statt des wasmtime-Features `cache` gibt es einen eigenen Zwischenspeicher (`places::load_component`: `<cache>/<SHA-256 des Codes>-<Verträglichkeits-Hash der Engine>.cwasm`, atomar geschrieben, ganz gelesen statt eingeblendet); das spart die Abhängigkeiten des Features. Einträge, die keinem gefundenen Plug-in mehr gehören (oder einer anderen wasmtime-Version), löscht das Programm beim Start im Hintergrund (`places::prune_cache`).
- Installer: `plugins/`-Ordner im Programm vorgesehen; die Komponente „Elektronik“ kommt in Vorhaben 3.

## Zweig

**Vorschlag (Claude):** `feature/plugins` ab master. Vorhaben 1 braucht die Elektronik nicht; so bleibt die Regel „Platinen erst nach master, wenn alle übrigen Fusion-Funktionen portiert sind“ unberührt, und die Schnittstelle kann (auf „bitte mergen“) vorher nach master. Vorhaben 2 holt sie dann auf `feature/platinen`.

## Dem Builder überlassen

Genaue WIT-Typen und Namen; Aufteilung der Module in `planora-app`; Form der Teil-Updates für Dialoge; wie die Treffer-Abfrage technisch läuft; Gestaltung der Einstellungsseite im bestehenden Stil; Testaufbau (Host-Tests mit den eingecheckten Beispiel-Wasm-Dateien).
