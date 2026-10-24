# Projekte-Fenster und Einstellungen – Entwurf

Stand: 2026-10-23. Auftraggeber: Dennis. Entwurf nach Gespräch und Mockup
(https://claude.ai/artifact/VgScGoLGUmTLbZuFqnukqk, privat; „das passt so“).

## 1. Ziel

Das Feld „Projekte“ links im Hauptfenster wird durch ein eigenes Fenster ersetzt. Es zeigt die
Projekte als Kacheln mit Miniaturansicht der Körper, Datum der letzten Änderung und Version. Das
Projektverzeichnis (bisher „Wurzel“) wird nicht mehr im Projektfenster, sondern in einem neuen
Fenster „Einstellungen“ gewählt, in das später weitere Einstellungen kommen.

*Warum:* Das schmale Baumfeld zeigt nur Namen. Wer viele Projekte hat, erkennt sie an der
Miniatur und am Stand schneller. Das Feld kostet außerdem Platz, den Feature-Baum und Zeichnungen
besser nutzen.

## 2. Nicht-Ziele und Anti-Ziele

- Kein neues Dateiformat, keine Änderung an `dokument.json`, den Versionsdateien oder
  `projekt.json`. Alte und neue Programmversionen lesen dieselben Projekte.
- Keine Suche, keine Sortierauswahl, keine Mehrfachauswahl, kein Ziehen von Kacheln (später).
- Die Miniatur ist eine Zusatzdatei zum Wegwerfen. Fehlt sie oder ist sie kaputt, ist das nie ein
  Fehler des Projekts: es erscheint der Platzhalter, und die Miniatur wird neu gemacht.
- Nie darf die Miniatur-Erzeugung ein Projekt verändern (keine Änderung am Dokumentordner außer
  der eigenen Zusatzdatei), die Oberfläche anhalten oder offene Dokumente stören.
- Die Auswahl des Projektverzeichnisses darf nie Projekte verschieben, kopieren oder löschen.

## 3. Begriffe (Laiensprache, wie im Programm)

- **Projektverzeichnis:** der Ordner, in dem alle Projekte liegen (bisher „Wurzel“, in der
  Konfiguration `root`). Vorgabe `Dokumente\PlanoraCAD`.
- **Version:** das Programm nennt die gespeicherten Stände „Versionen“. Auf den Kacheln steht
  „Version 12“ (Dennis sagte „Revision“; wir bleiben bei „Version“, weil der Reiter und alle
  Meldungen so heißen).
- Ein **Projekt** enthält Ordner und **Dokumente**. Versionen und Änderungszeiten gibt es je
  Dokument (`DocumentInfo`), nicht je Projekt.

## 4. Fenster „Projekte“ (Datei → Projekte …)

- Menü: „Datei → Projekte …“ (keine neue Tastenkombination in dieser Etappe).
  Das Fenster ist wie die Dateiauswahl-Fenster aufgebaut: mittig beim Öffnen,
  Breite ≈ 55 % des Hauptfensters (mindestens 720, höchstens 1200 px, wie `file_dialog` der
  Dateiauswahl), Höhe nach Fenster, feste Fußzeile, ✕ oben rechts, Hell und Dunkel, gleiche
  Breite aller Zeilen, funktioniert bei 800 × 600.
- Das Feld „Projekte“ links im Hauptfenster entfällt (Feature-Baum und Zeichnungen bleiben und
  bekommen den Platz). Das gilt für Modell-, Zeichnungs- und Skizzenansicht, soweit das Feld dort
  erscheint.
- Kopfzeile: Titel „Projekte“; darunter „Neues Projekt“ (blau, Hauptaktion) und der Pfad des
  Projektverzeichnisses als Text; rechts ein Knopf „Projektverzeichnis …“, der das Fenster
  „Einstellungen“ öffnet (§ 5).
- **Erste Ebene:** eine Kachel je Projekt, 4 Spalten bei 1100 px Breite (Spaltenzahl nach Breite,
  Kacheln gleich groß, gleich hoch). Kachelinhalt:
  - Miniaturansicht der Körper (§ 6) oder Platzhalter („Noch keine Körper“).
  - Name; „Geändert 23.10.2026, 18:42“; „Version 12 · 3 Dokumente“ („1 Dokument“, „Noch keine
    Körper · 1 Dokument“, „Noch kein Dokument“ bei leerem Projekt).
  - **[Geändert in der Umsetzung, siehe § 11: Datum und Reihenfolge nutzen die Zeit der Spitzenversion des aktiven Zweigs, nicht `latest_time()`.]**
  - Angaben stammen vom **zuletzt geänderten Dokument** des Projekts (größte `latest_time()`;
    bei Gleichstand das mit dem kleineren Namen): Miniatur, Datum = dessen jüngste Version,
    Version = Spitze seines aktiven Zweigs (`tip(active_branch)`). Dokumente, die sich nicht
    lesen lassen (`peek` schlägt fehl), zählen nicht für Datum und Version, werden aber in der
    Anzahl genannt; sind alle unlesbar, steht „Dokumente nicht lesbar“ in Warnfarbe.
- **Zweite Ebene** (Doppelklick auf ein Projekt): Pfad „‹ Projekte › Name“ (Pfeil geht zurück,
  Klick auf „Projekte“ ebenso); Knöpfe „Neues Dokument“ und „Neuer Ordner“; Kacheln für Ordner
  (großes Ordnersymbol, „Geändert …“ = jüngste Änderung darin, „4 Dokumente“) und Dokumente
  (Miniatur, Name, „Geändert …“, „Version 12“). Doppelklick auf einen Ordner geht hinein
  (der Pfad wird länger), Doppelklick auf ein Dokument öffnet es in einem Tab (wie
  `open_node`; ein offenes Dokument wird nur nach vorn geholt) und lässt das Fenster offen.
- Rechtsklick auf eine Kachel: Umbenennen, Verschieben (nicht für Projekte), Löschen, wie im
  bisherigen Baumfeld, mit denselben Regeln, Rückfragen und Sperren (`may_change`: Knoten mit
  offenen Dokumenten sind gesperrt). Die vorhandenen Dialoge „Neuer Ordner“, „Umbenennen“,
  „Verschieben“, „Löschen“ bleiben und werden von hier aufgerufen.
- Ausgewählte Kachel: blauer Rand (Akzentfarbe); Auswahl per Klick; Pfeiltasten/Enter sind nicht
  Teil dieser Etappe, Tab erreicht jedoch jede Kachel (Fokusring) und Enter öffnet sie.
- Leer: „Noch keine Projekte. Lege mit „Neues Projekt“ das erste an.“ Kein Projektverzeichnis
  gewählt oder nicht lesbar: dieselben Meldungen wie bisher (`NO_LIBRARY`, Lesefehler), mit
  Knopf „Projektverzeichnis …“.
- Aktualisieren: das Fenster liest den Baum beim Öffnen und nach jeder eigenen Änderung neu
  (`library_revision`); zusätzlich ein Knopf „Aktualisieren“ in der Fußzeile.
- Fußzeile: links Anzahl („6 Projekte“ / „1 Ordner · 3 Dokumente“), rechts der Hinweis auf
  Doppelklick und Rechtsklick, „Aktualisieren“.
- Das Fenster sperrt das Hauptfenster nicht (nicht modal), schließt sich aber, wenn ein anderes
  Datei- oder Importfenster geöffnet wird (wie die vorhandenen Fenster sich ausschließen).

## 5. Fenster „Einstellungen“ (Datei → Einstellungen …)

- Eigenes Fenster im Stil der Dateiauswahl-Fenster (mittig, wächst mit dem Hauptfenster bis 1200
  px, ✕, feste Fußzeile mit „Schließen“). Inhalt: Abschnitte untereinander mit Überschrift
  (nicht Reiter), damit später weitere Abschnitte (zum Beispiel Darstellung, Autosicherung)
  ohne Umbau dazukommen. Jetzt gibt es einen Abschnitt:
  **Projekte → Projektverzeichnis:** Pfad als Text, Knopf „Projektverzeichnis ändern …“ öffnet die
  Dateiauswahl im Ordner-Modus (§ 5.1). Darunter der Hinweis: „Ihre Projekte bleiben, wo sie sind.
  Im neuen Verzeichnis erscheinen nur die Projekte, die dort liegen.“ (kein Kopieren, kein
  Verschieben).
- Wirkung wie bisher `Workspace::choose_root`: das Verzeichnis wird bei Bedarf angelegt, in
  `config.json` gespeichert (`persist_config`, nur `root` wird geschrieben, Favoriten der
  Dateiauswahl bleiben, siehe Teilprojekt Dateiauswahl) und der Baum neu gelesen. Solange
  gespeicherte Dokumente offen sind, wird nicht gewechselt: die vorhandene Meldung erscheint
  im Fenster in Warnfarbe („… bitte erst die Dokumente schließen“ – Wortlaut aus `choose_root`).
- Abbrechen lässt alles unverändert. Ein Verzeichnis, das sich nicht anlegen oder lesen lässt,
  gibt die vorhandene Fehlermeldung; das alte Verzeichnis bleibt gewählt.
- Der alte Knopf „Wurzel wählen …“ und das Textfeld „Projektordner“ im Baumfeld entfallen. Das
  Wort „Wurzel“ erscheint nirgends mehr in Oberflächentexten (Code und Konfigurationsschlüssel
  `root` bleiben, damit alte Konfigurationen weiter laden).

### 5.1 Ordner-Modus der Dateiauswahl

`FileChooser` bekommt einen dritten Modus „Ordner wählen“: Es werden nur Ordner gelistet
(Doppelklick geht hinein), die Schnellzugriffe und Favoriten gelten wie gehabt; statt eines
Dateinamens steht das Feld mit dem aktuellen Ordner; die Aktionsknöpfe heißen „Diesen Ordner
wählen“ und „Abbrechen“ (feste Fußzeile). Das Ergebnis ist der aktuell angezeigte Ordner, oder der
in der Liste markierte Ordner, falls einer markiert ist (Klick markiert, Doppelklick geht
hinein). Existenz, Lesbarkeit und Namensregeln prüft `choose_root`, nicht die Auswahl.

## 6. Miniaturansicht

- **Inhalt:** alle Körper des Dokumentstands (`Spitze des aktiven Zweigs`), schräg von
  vorne-rechts-oben (derselbe Blick wie der Startblick der Hauptansicht), eingepasst mit Rand,
  flach schattiert in den Körperfarben des Programms (Hell und Dunkel je eigene Zeichnung, siehe
  § 6.4), Hintergrund `bg_inset`. Netz-Körper und STEP-Körper zählen wie alle Körper.
  Unterdrückte Features erscheinen nicht. Skizzen und Ebenen erscheinen nicht.
- **Erzeugung:** ausschließlich im Hintergrund (`jobs`-Muster der Importe: ein Arbeiter, Kanal,
  Abbruch beim Schließen des Fensters), höchstens ein Dokument gleichzeitig, sichtbare Kacheln
  zuerst. Ablauf je Dokument: `peek` → Spitze `n` des aktiven Zweigs → vorhandene Zusatzdatei
  gültig (Version, Darstellung, Programmversion stimmen)? sonst Dokument über `read_version`
  lesen, Körper aufbauen, Netz erzeugen (grobe Sehne), mit einem kleinen eigenen
  Software-Rasterer (kein neuer Fremdcode, keine GPU) in ein Bild von 248 × 140 px (bei
  doppelter Auflösung 496 × 280, damit es auf scharfen Bildschirmen gut aussieht) zeichnen und
  als PNG speichern (`png` ist bereits Abhängigkeit der App).
- **Speicherort:** `<Dokumentordner>/vorschau.png` und daneben eine kleine
  `vorschau.json` (`{"v":1,"version":12,"branch":"…","program":"0.1.0","dark":true,"w":496,"h":280}`)
  bzw. die Angaben in den PNG-Textfeldern. Die Dateien werden atomar geschrieben, sind für das
  Dokumentformat unsichtbar (`DocStore` und `Library` ignorieren sie; das ist durch Test
  zu belegen, inklusive Löschen/Umbenennen/Verschieben/Kopieren des Dokuments mit der Datei) und
  dürfen jederzeit gelöscht werden. Schreibgeschützte oder gesperrte Ordner: die Miniatur bleibt
  nur im Speicher dieser Sitzung, kein Fehler, kein Dauerschleifen-Versuch.
- **Neu erzeugen** nur, wenn sich die Spitze (Version), der aktive Zweig, die Programmversion
  oder die Darstellung (Hell/Dunkel) ändert, oder die Datei fehlt/unlesbar ist. Nie bei jedem
  Öffnen des Fensters.
- **Schutz:** Größe des Dokuments begrenzt die Arbeit (zum Beispiel höchstens 200 000 Dreiecke
  und 3 s je Dokument; danach „keine Miniatur“ mit Platzhalter und einmaligem Hinweis in der
  Kachel „Miniatur zu aufwendig“, kein Wiederholen bis zur nächsten Version). Fehler beim
  Aufbau (ein Feature im Fehlerzustand) zeigen die übrigen Körper; sind keine Körper übrig,
  der Platzhalter. Kein Absturz bei beschädigten Dokumenten.
- Während die Miniatur entsteht, zeigt die Kachel den Platzhalter mit einer ruhigen
  Fortschrittsanzeige (kein Springen der Kachelgröße).
- Das Dokument, das gerade offen und geändert ist, wird nicht aus dem Speicher des Tabs
  gezeichnet: die Kachel zeigt den **gespeicherten** Stand (Version aus der Datei).

### 6.4 Hell und Dunkel
Das Bild wird nach dem aktuellen Design gezeichnet. Wechselt der Nutzer das Design, werden die
Miniaturen beim nächsten Öffnen des Fensters neu gemacht (sichtbare zuerst).

## 7. Datenquellen und Kosten

- Die Kachelangaben liest `Library::tree` (Struktur) und je Dokument `docstore::peek` (nur
  `dokument.json`, kein Dokumentinhalt). Das geschieht im Hintergrund und liefert die Angaben an
  das Fenster, damit auch Verzeichnisse mit vielen Projekten die Oberfläche nicht anhalten. Bei
  mehr als 500 Dokumenten zeigt das Fenster die ersten 500 und einen Hinweis (wie bei der
  Dateiauswahl `MAX_ENTRIES`).
- Zwei Programme gleichzeitig: die Anzeige liest nur; die Miniatur-Datei wird atomar
  geschrieben; gleichzeitiges Erzeugen derselben Miniatur ist harmlos (gleicher Inhalt).

## 8. Texte (Laiensprache, kein Fachjargon, keine Markennamen)

Fenster: „Projekte“, „Einstellungen“. Knöpfe: „Neues Projekt“, „Neues Dokument“, „Neuer Ordner“,
„Projektverzeichnis …“ (öffnet die Einstellungen), „Projektverzeichnis ändern …“, „Diesen Ordner
wählen“, „Abbrechen“, „Schließen“, „Aktualisieren“. Meldungen wie oben; Fehlertexte der
Bibliothek bleiben wie bisher. Das Wort „Wurzel“ entfällt aus der Oberfläche.

## 9. Prüfung und Tests (Anforderungen an den Plan)

- Zustandsebene (ohne Fenster): Kachelmodell (zuletzt geändertes Dokument, Version = Spitze des
  aktiven Zweigs, Zählungen, unlesbare Dokumente, leere Projekte, Gleichstand), Miniatur-Cache
  (gültig/ungültig nach Version, Zweig, Programmversion, Darstellung; beschädigt; nicht
  schreibbar), Rasterer (Eckpunkte, Reihenfolge der Flächen, Rand, leere Szene), Einstellungen
  (Projektverzeichnis wechseln inkl. Sperre bei offenen Dokumenten, Konfiguration nur `root`),
  Ordner-Modus der Dateiauswahl, Löschen/Umbenennen/Verschieben über Kacheln.
- Layout-Tests wie bei der Dateiauswahl (800×600, 1400×900, 2000×1100; mit 0, 1 und 200
  Kacheln; Fußzeile sichtbar; Fenster ≤ 1200 px; keine Verschiebung bei Hover; gleiche
  Kachelgröße).
- Das Dokumentformat bleibt unberührt: alte Goldene Dateien laden und speichern byte-gleich;
  `Library` und `DocStore` ignorieren die Zusatzdateien (Tests).
- Mutationsproben je Aufgabe, unabhängige Prüfung je Aufgabe, Gesamtprüfung am Ende, Handliste
  für den Nutzer; Merge nur auf Ihr „bitte mergen“.

## 9a. Entscheidungen nach der Leserprüfung (2026-10-23)

1. **„Neues Dokument“ in einem Projekt oder Ordner (Entscheidung von Dennis: a):** Der Knopf
   fragt nach einem Namen (Namensregeln und „gibt es schon“-Fehler wie bei Ordnern), legt das
   leere Dokument sofort im Projekt oder Ordner an (`DocStore::create` mit leerem Stand, das ist
   Version 1, so wie das erste Speichern es täte), öffnet es in einem Tab und zeigt die neue
   Kachel. Strg+N („Neu“, leerer ungespeicherter Tab) bleibt unverändert.
2. **Datei → Öffnen … (Strg+O) wird zu „Projekte …“:** Der Menüeintrag „Öffnen …“ heißt künftig
   „Projekte …“ und öffnet das Fenster (Tastenkombination Strg+O bleibt, ist also keine neue).
   „Ansicht → Projekte“ (Häkchen mit Symbol) entfällt mit dem Feld. Alle Texte, die auf das
   Feld verweisen (zum Beispiel in „Speichern unter“ „Zuerst im Feld Projekte …“), sagen künftig
   „im Fenster Projekte“. „Speichern unter“ bleibt wie es ist.
3. **Zeitgrenze der Miniaturen ohne Eingriff in Kernel oder Feature-Crate:** Das Aufbauen eines
   Dokuments (`Document::from_snapshot`) lässt sich nicht abbrechen; es wird dafür nichts in
   `planora-kernel`, `planora-feature` oder anderen Crates geändert. Es läuft höchstens ein
   Aufbau gleichzeitig in einem Hintergrund-Arbeiter. Schließt das Fenster, wird kein weiteres
   Dokument begonnen; ein laufender Aufbau läuft zu Ende, sein Ergebnis wird noch gespeichert
   (die Miniatur ist dann beim nächsten Öffnen da). Dauert ein Aufbau länger als 3 s oder ist das
   Netz größer als 200 000 Dreiecke, wird für diese Version der Zustand „zu aufwendig“ in
   `vorschau.json` gespeichert (kein Bild); die Kachel zeigt den Platzhalter mit „Miniatur zu
   aufwendig“, und es wird nicht erneut versucht, bis sich die Version ändert. Ein sehr
   schweres Dokument kann so den Arbeiter lange belegen, nie die Oberfläche; die Kacheln der
   übrigen Dokumente werden bis dahin nicht erzeugt (der Arbeiter arbeitet nacheinander,
   sichtbare Kacheln zuerst, die jeweils schwere Kachel zeigt den Platzhalter mit der
   ruhigen Anzeige).

## 10. Festlegungen des Bauenden (bitte beim Lesen prüfen)

1. „Version“ statt „Revision“ in der Oberfläche.
2. Version einer Projektkachel = Spitze des aktiven Zweigs des zuletzt geänderten Dokuments.
3. Miniaturen als PNG neben `dokument.json` (nicht im Dokument), im Hintergrund, nur bei
   geänderter Version/Darstellung neu.
4. Software-Rasterer statt GPU, damit Miniaturen auch ohne offene Ansicht und ohne Fenster
   entstehen und testbar sind.
5. Ordner-Modus der Dateiauswahl für das Projektverzeichnis (wiederverwendet Schnellzugriffe,
   Favoriten und Fußzeile).
6. Einstellungen als Abschnitte untereinander, nicht als Reiter.
7. Keine neue Tastenkombination in dieser Etappe.

## 11. Festlegungen der Umsetzung

Beim Bauen haben sich einzelne Festlegungen dieses Entwurfs geändert oder sind dazugekommen. Die vollständige Liste, nach Aufgaben geordnet, steht im Plan `docs/superpowers/plans/2026-10-23-projekte-fenster.md`, Abschnitt „Abweichungen bei der Umsetzung“. Die wichtigsten Punkte:

1. **Datumsregel (§ 4) geändert:** Das Datum einer Kachel ist die Zeit der Spitzenversion des aktiven Zweigs, nicht die jüngste Version über alle Zweige. Datum und „Version n“ gehören so zur selben Version. Die Reihenfolge bei Projekten und Ordnern nutzt denselben Wert.
2. **Zeitangaben in Ortszeit** (statt UTC, Entscheidung B) mit der Bibliothek `chrono`; gespeichert wird weiter UTC.
3. **Strg+O** öffnet das Fenster „Projekte“ (§ 9a.2); „Neues Dokument“ legt das Dokument sofort an (§ 9a.1).
4. **Miniaturen:** zusätzliche Werte `baut` und `fehlgeschlagen` für `art` samt Absturzmarke, Aufwandsgrenze `render_cost`, Bildgröße höchstens 1024 Punkte je Seite, eine Miniatur je Dokument, höchstens 64 Bilder im Grafikspeicher.
5. **Fenster und Sperren:** Bei offener Rückfrage ist „Projekte“ gesperrt und „Einstellungen“ lassen sich nicht öffnen; offene Einstellungen sperren die Tasten für Rückgängig.
6. **Pfad** mit Symbolen statt der Zeichen ‹ und ›, in einer eigenen Zeile unter dem Titel.
