# Rückgängig und Wiederholen – Entwurf

Stand 2026-10-19. Nächstes Teilprojekt nach 3d (Schnittansichten). **Alle Werte unten sind Vorschläge mit Standardwerten**; Dennis korrigiert, was nicht passt. Beschlossen ist bisher nur: Rückgängig/Wiederholen kommt als nächstes Teilprojekt.

## 1. Zweck

Heute ist jede Änderung endgültig: Ein Fehlklick beim Löschen, Verschieben, Verrunden oder in der Zeichnung lässt sich nur durch Neuaufbau (oder durch Laden einer gespeicherten Version) beheben. Das Teilprojekt macht jede Änderung am Dokument rücknehmbar und wiederholbar, so wie man es aus anderen Programmen kennt.

## 2. Umfang (Vorschlag)

**Rücknehmbar (ein Schritt je Benutzeraktion):**
- Körper anlegen (Quader, Zylinder, Kugel, Kegel), Netz importieren.
- Körper verschieben (Feld „Position“), Booleschen Operationen, Verrunden, Fasen, Extrudieren, Drehen.
- Feature löschen (mit abhängigen), umbenennen, unterdrücken/einblenden, Zeitleistenmarke setzen, Feature ändern (Dialog „Ändern“/Skizze bearbeiten).
- Zeichnungen: Zeichnung anlegen/löschen/umbenennen, Ansicht hinzufügen/verschieben/Maßstab/löschen, Maße (anlegen, ändern, löschen), Positionsnummern, Stücklistentexte, Blattangaben, Schnittansichten und ihre Felder.
- Skizzen: **im Skizzenmodus** je ein Schritt pro gezeichnetem Element, pro Abhängigkeit/Maß und pro Ziehen. „Skizze beenden“ ist danach **ein** Schritt im Dokument (die ganze Skizze entsteht oder ändert sich auf einmal).

**Nicht rücknehmbar (Vorschlag):** Kamera, Auswahl, Ansichtseinstellungen, Fenster-/Tabaufteilung, Export und Speichern (Speichern ändert den Verlauf nicht), Neu-/Öffnen/Schließen eines Tabs.

**Nicht-Ziele:** Rückgängig über Programmneustart hinaus (der Verlauf wird nicht mit der Datei gespeichert); gemeinsamer Verlauf über mehrere Dokumente (Tabs haben **je einen eigenen** Verlauf); verzweigter Verlauf/„Zeitreise“ (das leisten die gespeicherten Versionen); Auswahl der Rückgängig-Tiefe in einer Liste.

**Anti-Ziel:** ein Rückgängig, das das Modell still in einen halben oder anderen Zustand bringt. Lieber nichts tun und eine Meldung zeigen.

## 3. Bedienung (Vorschlag)

- **Strg+Z** = Rückgängig, **Strg+Umschalt+Z** und **Strg+Y** = Wiederholen.
- Menü „Bearbeiten“: oben „Rückgängig: <Aktion>“ und „Wiederholen: <Aktion>“ mit dem Namen der Aktion („Quader anlegen“, „Feature löschen“, „Ansicht verschieben“ …), ausgegraut, wenn nichts da ist.
- Zwei kleine Symbolknöpfe in der Werkzeugleiste der 3D-Ansicht (links neben den Booleschen Operationen) mit Hilfetext; Platz bei 1100 × 700 und 800 × 600 wird wie bei den anderen Leisten geprüft.
- **Tastenschutz:** Strg+Z gilt nicht, solange in einem Textfeld getippt wird (dort gilt Rückgängig des Textfeldes), und nicht bei offenem Dialog.
- Nach Rückgängig/Wiederholen bleibt die Ansicht, wo sie ist; die Auswahl wird zurückgesetzt, wenn das gewählte Ding nicht mehr existiert; offene Dialoge (z. B. „Quader erzeugen“) werden geschlossen.
- Eine neue Aktion nach Rückgängig löscht die Wiederholen-Liste.

## 4. Schrittgröße (Vorschlag)

- Ziehen mit der Maus (Position, Maßlinie, Ansicht verschieben, Skizzenpunkt ziehen) ist **ein** Schritt: der Zustand vor dem Ziehen.
- Tippen in einem Zahlenfeld, das nach „Enter“ oder Verlassen übernommen wird, ist ein Schritt je Übernahme.
- Aktionen, die intern mehrere Änderungen auslösen (Löschen mit abhängigen Features, Ansicht mit Folgeansichten, Zeichnung mit Folgeblatt der Stückliste), sind **ein** Schritt.
- Höchstens **100 Schritte** je Tab und **höchstens 200 MB** Arbeitsspeicher für den Verlauf; die ältesten Schritte fallen heraus, ohne Meldung.

## 5. Gespeichert-Markierung

Der Punkt „ungespeichert“ am Tab folgt dem Verlauf: Wer rückgängig bis zum gespeicherten Stand geht, sieht keinen Punkt mehr; wer vom gespeicherten Stand aus rückgängig geht, sieht ihn. Beim Speichern merkt sich der Tab den Stand; der Verlauf bleibt erhalten. **Umsetzung:** Der bisherige Zähler (`edit_count`) taugt dafür nicht, weil er beim Zurückladen auf 0 fällt und nicht immer nur steigt. Der Verlauf bekommt deshalb eigene Schrittnummern; der Tab merkt sich die Nummer des gespeicherten Standes. Bei offener Skizze gilt weiter „ungespeichert“.

## 6. Technischer Ansatz (Vorschlag, im Plan zu prüfen)

- **Zustandsbilder statt Befehle:** Der Verlauf hält vollständige `Snapshot`s des Dokuments (gibt es bereits für Speichern/Laden: Features, Zeichnungen, Projektangaben, Stücklistentexte, Netze nur als gemeinsame Zeiger). Rückgängig lädt das Bild über das vorhandene `from_snapshot`-Verfahren zurück; Berechnungen laufen danach wie nach dem Laden neu. Vorteil: Jede jetzige und künftige Änderung ist automatisch rücknehmbar, ohne dass jede Funktion eine Umkehrung kennen muss.
- **Auslöser zentral:** Die App erkennt Änderungen an `Document::edit_count` (öffentliche Änderungen) einmal je Bild und legt das Bild **vor** der Änderung auf den Verlauf. Gesten wie Ziehen werden zusammengefasst (Beginn bis Loslassen); Skizzenmodus hat einen eigenen Verlauf für die lebende Skizze.
- **Kein Eingriff in Kernel oder Dateiformat.** Es ändert sich nichts an gespeicherten Dateien.
- **Größe:** Snapshot-Kopien sind billig für Features (kleine Daten), teurer für große Zeichnungen und Netze; Netze werden geteilt, nicht kopiert; die 200-MB-Grenze schützt.

## 6a. Festlegungen aus der Vorabprüfung (meine Entscheidungen, Dennis darf widersprechen)

1. **Nur das Dokument wird ersetzt, nicht die ganze App:** Rückgängig tauscht `document` im bestehenden Tab aus und stößt danach Namenszähler, Neuberechnung und Aufräumen der Auswahl an. `AppState::from_snapshot` (baut alles neu, setzt Kamera zurück) wird **nicht** benutzt; Kamera und Hell/Dunkel bleiben.
2. **Skizzenmodus:** Strg+Z wirkt auf die **lebende Skizze** (eigener Skizzenverlauf nach dem vorhandenen Sicherungsmuster in `sketch_state.rs`), solange sie offen ist; der Dokumentverlauf ruht dann. „Skizze beenden“ ist ein Dokumentschritt.
3. **Nicht im Zustandsbild, aber zurückgesetzt:** Skizzenentwurf, offene Dialoge, Werkzeuge (Maß, Position, Schnitt), markierte Kanten, Umbenennen-Felder, Rückfragen. Rückgängig/Wiederholen schließt/leert sie.
4. **Dialoge sperren:** Strg+Z gilt nicht bei offenem Dialog oder Tippen im Textfeld (vorhandene Sperre erweitern, dazu eine Sammelprüfung „ist ein Dialog offen“).
5. **Auslöser:** Wert von `edit_count` vor und nach jedem Bild vergleichen; bei Abweichung das Bild von **vorher** auf den Verlauf (kein Verlass auf Monotonie). Gesten (Ziehen) werden zu einem Schritt zusammengefasst.
6. **Dauer:** Zurückladen rechnet das ganze Dokument neu (wie Laden) und läuft zuerst synchron; wenn die Messung (Aufgabe 1 des Plans) mehr als ca. 1 s auf großen Modellen zeigt, kommt eine Fortschrittsanzeige/Hintergrundrechnung. Das Ergebnis wird gemessen und im Plan festgehalten.
7. **Speichergrenze:** zuerst eine grobe Schätzung der Bildgröße (Zeichnungen und Features gezählt, Netze nicht), 100 Schritte fest.
8. **Fehler beim Zurückladen:** nur der fehlgeschlagene Schritt wird verworfen, das Dokument bleibt unverändert, Meldung; der übrige Verlauf bleibt. (Ändert die Zeile in §7.)

## 7. Grenzen und Fehler (Vorschlag)

- Schlägt das Zurückladen eines Bildes fehl (sollte nicht vorkommen), bleibt das Dokument unverändert und eine Meldung „Rückgängig ist nicht möglich.“ erscheint; nur dieser Schritt wird verworfen, der übrige Verlauf bleibt (siehe §6a.8).
- Rückgängig ist während einer laufenden langen Berechnung (Export, Schnitt) gesperrt oder bricht sie ab (Vorschlag: bricht ab und führt dann aus).

## 8. Tests und Prüfung

- Pro Aktionsart: Aktion, Rückgängig, Zustand gleich dem Ausgangsbild (Vergleich der Snapshots Byte für Byte), Wiederholen, Zustand gleich dem Ergebnisbild.
- Gesten (Ziehen, Zahlenfeld), Mehrfachaktionen als ein Schritt, 100-Schritte-Grenze, Speicher-Grenze, Wiederholen-Liste nach neuer Aktion gelöscht.
- Tastenschutz (Textfeld, Dialog), Menütexte, Knöpfe bei kleinen Fenstergrößen, Hell/Dunkel.
- „Ungespeichert“-Punkt nach Speichern, Rückgängig, Wiederholen.
- Zeichnungs-, Skizzen- und Schnittaktionen als Stichprobe der Breite; Handliste für das echte Fenster.

## 9. Offen (Vorschläge, bitte korrigieren)

1. Soll der Verlauf **je Tab** getrennt sein? (Vorschlag: ja.)
2. **100 Schritte / 200 MB** passend?
3. Zwei Knöpfe in der Leiste zusätzlich zum Menü und den Tasten?
4. Ein Schritt je gezeichnetem Skizzenelement (statt nur „Skizze beenden“ als ganzer Schritt)?
5. Soll Rückgängig auch **nach dem Speichern** noch funktionieren? (Vorschlag: ja.)
