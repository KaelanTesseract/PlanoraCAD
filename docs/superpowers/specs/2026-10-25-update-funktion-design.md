# Update-Funktion – Entwurf

Stand: 2026-10-25. Entwurf zur Prüfung durch Dennis. „Mein Vorschlag“ ist eine Annahme von Claude, nicht vom Nutzer entschieden.

## Ziel

Dennis' Tester (und später Anwender) bekommen neue PlanoraCAD-Versionen, ohne selbst etwas zu tun: Das Programm meldet ein neues Release samt Patchnotes, aktualisiert sich auf Knopfdruck und startet sich selbst neu. Die erste Version lädt der Tester von GitHub.

Grund: Der Tester arbeitet immer mit der aktuellen Version, darf nichts von Hand kopieren müssen und soll sehen, was neu ist.

## Entschieden (vom Nutzer)

1. Die erste Version lädt der Tester von GitHub herunter; das Repository wird dafür **öffentlich** (Dennis: „vielleicht gleich öffentlich schalten“ – Umschalten erst auf sein ausdrückliches Wort).
2. Ist ein neues Release da, erscheint im Programm eine Meldung. Der Tester kann dort aktualisieren, ohne selbst etwas zu tun.
3. Die Meldung zeigt Patchnotes.
4. Nur Windows. Ein Neustart nach dem Update ist in Ordnung, wenn das Programm ihn **selbst** macht.
5. Der Tester arbeitet immer mit der aktuellen Version und macht nie ein Downgrade (nur Aufwärtskompatibilität zählt).
6. Patchnotes schreibt Claude nach jedem fertigen Teilprojekt, in einfacher Sprache.
7. Kanäle **Dev, Beta, Stabil**. Heute gibt es nur Dev. Die Kanal-Auswahl steht **schon jetzt** in den Einstellungen (mit nur „Dev“ zur Auswahl).
8. Lizenz bleibt GPL-3.0-only.

## Mein Vorschlag (nicht entschieden)

**Quelle und Versionen**
- GitHub „Releases“ des Repositorys `KaelanTesseract/PlanoraCAD`. Ein Release hat: Versions-Tag `v0.2.0-dev.1`, die Programmdatei `planoracad-windows-x64.exe` (oder ein Zip mit der exe und den Lizenzhinweisen), eine Prüfsummen-Datei `planoracad-windows-x64.sha256` und die Patchnotes als Beschreibung.
- Version nach SemVer mit Endung für den Kanal: `0.2.0-dev.N` (Dev), `0.9.0-beta.N` (Beta), `1.0.0` (Stabil). Die Endung bestimmt den Kanal; GitHub „Pre-release“ wird für Dev und Beta gesetzt.
- Reihenfolge nach SemVer: `dev` < `beta` < ohne Endung (bei gleicher Nummer), höhere Nummer gewinnt. Das Programm bietet **nur** eine Version an, die höher ist als die laufende.
- Die laufende Version kommt aus `CARGO_PKG_VERSION`; die Versionsnummer im Workspace wird bei jedem Release erhöht.
- Release ab jetzt nach jedem fertigen Teilprojekt (nicht nach jedem Commit).

**Prüfen**
- Beim Programmstart, im Hintergrund, höchstens einmal je 24 Stunden (Zeitpunkt der letzten Prüfung steht in der Konfiguration), und manuell über Einstellungen → Updates → „Jetzt suchen“.
- Abfrage der Release-Liste über die GitHub-Schnittstelle (ohne Anmeldung). Ein Release zählt zum Kanal des Nutzers, wenn dessen Endung im Kanal enthalten ist (Dev: alle, Beta: Beta und Stabil, Stabil: nur Stabil). Entwürfe (Draft) zählen nie.
- Kein Netz, Zeitüberschreitung, Rate-Limit oder fehlerhafte Antwort: still ignorieren (bei manueller Suche kurzer Hinweis), nie ein Fehlerfenster beim Start.
- Schalter in den Einstellungen: „Beim Start nach Updates suchen“ (Voreinstellung an). Aus: es wird nie von allein im Netz gefragt.

**Meldung**
- Fenster „Neue Version verfügbar“ mit Versionsnummer, Kanal, Patchnotes und den Knöpfen „Jetzt aktualisieren“, „Später“ und „Diese Version überspringen“. „Später“ zeigt die Meldung frühestens nach 24 Stunden wieder, „Überspringen“ nie für diese Version, aber für die nächste.
- Patchnotes: Beschreibung des Releases; fehlen Versionen dazwischen (übersprungen), zeigt das Fenster die Notizen **aller** Versionen zwischen der laufenden und der neuen, neueste oben. Darstellung: einfache Überschriften, Aufzählungen, Absätze; kein Netzwerkzugriff beim Anzeigen.
- Zusätzlich ein dezenter Hinweis in der Statuszeile/Titelleiste, solange eine Meldung offen ist; im „Über“-Dialog steht die Version mit Kanal.

**Aktualisieren**
- „Jetzt aktualisieren“: Download im Hintergrund mit Fortschrittsanzeige (Abbrechen möglich), Prüfung der SHA-256-Summe gegen die Prüfsummen-Datei des Releases, nur über HTTPS, nur von `github.com`/`objects.githubusercontent.com`-Adressen des Releases. Passt die Summe nicht: abbrechen, nichts austauschen, Fehlermeldung in einfacher Sprache.
- Austausch unter Windows (laufende exe lässt sich nicht überschreiben): laufende Datei in `planoracad.exe.old` umbenennen, neue Datei an den Ort der alten legen, neue Datei starten und beenden; beim nächsten Start wird die `.old`-Datei gelöscht. Schlägt ein Schritt fehl, wird der alte Stand zurückgelegt und das Programm läuft weiter.
- **Ungespeicherte Arbeit:** vor dem Neustart dieselbe Rückfrage wie beim Schließen („Speichern“, „Verwerfen“, „Abbrechen“). „Abbrechen“ bricht auch das Update ab (die heruntergeladene Datei bleibt zum späteren Versuch liegen oder wird verworfen).
- Nach dem Neustart zeigt ein kleiner Hinweis „Aktualisiert auf Version X“ mit den Patchnotes dieser Version (einmal).
- Liegt die exe in einem Ordner ohne Schreibrecht (z. B. Programme), meldet das Programm das und verweist auf die Download-Seite.
- Projekte, Konfiguration und Dokumente liegen außerhalb der exe und bleiben unberührt. Neue Versionen müssen alte Dateien und Einstellungen lesen können (nur Aufwärts); eine nicht lesbare Konfiguration führt zu den Voreinstellungen, nie zum Absturz.

**Einstellungen**
- Neuer Abschnitt „Updates“ im Einstellungsfenster: „Update-Kanal“ (Auswahl; heute nur „Dev“), „Beim Start nach Updates suchen“ (Schalter), „Jetzt nach Updates suchen“ (Knopf mit Ergebnis), Anzeige der laufenden Version und der letzten Prüfung.
- Konfigurationsfelder: `update_channel` (Voreinstellung „dev“), `update_check` (an), `last_update_check` (Zeitstempel), `skipped_version`. Alle nachsichtig gelesen wie die übrigen Felder; fehlen sie, gelten die Voreinstellungen.
- Ein Kanalwechsel installiert nie eine ältere Version; er wirkt auf die nächste Prüfung.

**Bauen und Veröffentlichen**
- GitHub Actions (es gibt bereits `.github/workflows/ci.yml`): ein neuer Ablauf baut bei einem Tag `v*` auf `windows-latest` die Release-Datei, erzeugt Prüfsumme und Lizenzhinweise (Datei `THIRD-PARTY-LICENSES.txt` mit den Lizenztexten aller Bibliotheken und Schriften), und legt das GitHub-Release mit den Patchnotes aus `CHANGELOG.md` an. Pre-release-Kennzeichnung nach Endung.
- Die Tests laufen vor dem Veröffentlichen; schlägt etwas fehl, entsteht kein Release.
- `CHANGELOG.md` im Repository (deutsch, einfache Sprache, neueste oben, je Version ein Abschnitt); aus dem Abschnitt der Version entsteht die Beschreibung des Releases. Claude schreibt den Abschnitt vor jedem Tag.
- Ein Skript/Hilfe für Dennis: „Release erstellen“ = Version erhöhen, Changelog prüfen, Tag setzen (nur auf sein Wort).

**Internet-Zugriff im Programm**
- Heute hat das Programm keinen HTTP-Client. Der Bauer wählt einen kleinen blockierenden Client (oder die in Windows eingebaute Schnittstelle), der die vorhandene Lizenzliste (`deny.toml`: MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, 0BSD, CC0, BSL-1.0, GPL-3.0-only) einhält. Bringt die Wahl eine andere Lizenz (z. B. OpenSSL-ähnlich bei `ring`/`aws-lc`), meldet er das und fragt nach, statt `deny.toml` still zu erweitern.
- Alle Netzvorgänge in einem Hintergrund-Thread; die Oberfläche bleibt bedienbar; Zeitüberschreitungen (Prüfen 10 s, Download-Stillstand 30 s).
- Datenschutz: Die einzige Anfrage ist die Release-Liste (und der Download); GitHub sieht dabei die IP-Adresse. Es werden keine Nutzer- oder Dokumentdaten gesendet. Ein Hinweis steht im README und beim Schalter „Beim Start nach Updates suchen“.

## Nicht-Ziele

- Linux und macOS.
- Stilles Aktualisieren ohne Rückfrage; Aktualisieren im laufenden Betrieb ohne Neustart.
- Differenz-Updates (nur Teile laden).
- Digitale Signatur der Windows-Datei (Signaturzertifikat kostet Geld); Windows kann beim ersten Download eine Sicherheitswarnung zeigen. Prüfsumme gegen das Release ersetzt keine Signatur und schützt nur vor Übertragungsfehlern, nicht vor einem manipulierten Release.
- Downgrade, Rückspulen auf ältere Versionen.
- Installer/Deinstaller, Startmenüeintrag, automatischer Start mit Windows.
- Telemetrie.
- Eigener Update-Server.

## Anti-Ziele

- Nie eine Datei austauschen, deren Prüfsumme nicht stimmt.
- Nie Nutzerdaten verlieren oder verändern (Projekte, Konfiguration).
- Nie eine ältere oder gleiche Version anbieten oder installieren.
- Nie die Oberfläche anhalten (Prüfen und Herunterladen im Hintergrund).
- Nie bei jedem Start dieselbe Meldung zeigen, nachdem der Nutzer „Später“ oder „Überspringen“ gewählt hat.
- Nie ein Programm hinterlassen, das nicht mehr startet (Austausch mit Rücksetzen).
- Kein Absturz bei fehlendem Netz, kaputter Antwort oder abgebrochenem Download.

## Constraints

- egui/eframe 0.36; Konfiguration über `planora_project::config::Config` (nachsichtiges Lesen, Schreiben nur geänderter Felder, vorhandene Mechanik für fehlgeschlagene Schreibvorgänge); Tests nur mit temporärem `config_path`, nie echtes `%APPDATA%`.
- Keine echten Netzzugriffe in Tests: Abfrage und Download hinter einer austauschbaren Schnittstelle (Trait), Tests nutzen Nachbildungen.
- Keine Systemeingriffe in Tests (kein Ersetzen der echten exe): Austausch-Logik arbeitet auf übergebenen Pfaden und wird in temporären Ordnern getestet.
- Oberflächentexte deutsch, einfache Sprache; bei 1920 × 1080 geprüft.
- Neue Abhängigkeiten nur nach `cargo deny` mit den erlaubten Lizenzen.
- Neue Funktionen tragen das passende Fusion-Kürzel ein, falls es eines gibt (hier keines).

## Randfälle, die Tests brauchen

- Versionsvergleich: `0.2.0-dev.1 < 0.2.0-dev.2 < 0.2.0-beta.1 < 0.2.0`, `0.10.0 > 0.9.0` (nicht als Text), gleiche Version wird nicht angeboten, Kanalfilter für Dev/Beta/Stabil, Entwurf und unbekannte Endung werden ignoriert.
- Release-Antwort: leer, ohne Programmdatei, ohne Prüfsumme, zwei Programmdateien, kaputtes JSON, Rate-Limit-Antwort, sehr lange Patchnotes.
- Mehrere übersprungene Versionen: Notizen aller dazwischen, richtige Reihenfolge, Begrenzung der Länge.
- 24-Stunden-Takt, „Später“, „Überspringen“ (nur diese Version), manueller Knopf ignoriert den Takt, Schalter aus verhindert Netzzugriff beim Start.
- Download: Abbruch, Verbindungsabbruch, falsche Prüfsumme, zu große Datei (Obergrenze), Speicherplatz voll.
- Austausch: alles gut; neue Datei kann nicht an den Ort gelegt werden (Rücksetzen); alte Datei nicht umbenennbar; `.old` aus vorigem Update wird beim Start gelöscht; Ordner ohne Schreibrecht.
- Ungespeicherte Arbeit: Rückfrage; „Abbrechen“ bricht Update ab; mehrere Tabs mit ungespeicherten Änderungen.
- Konfiguration: fehlende, kaputte und unbekannte Werte (Kanal „xyz“ → Dev); Kanal-Auswahl wird gespeichert; Schreibfehler.
- Neustart: nach dem Update erscheint der Hinweis einmal; kein Neustart-Schleifen (Programm startet nicht ständig neu, wenn die neue Datei sofort abstürzt: Austausch wird beim nächsten Start nicht wiederholt).
- Fenster: Meldung folgt den Fensterausschlussregeln, blockiert keine Eingabe in offenen Dialogen, öffnet sich nicht mitten in einer Rückfrage.

## Etappen (Vorschlag)

1. Versionen und Kanäle: Versionsvergleich, Kanalfilter, Release-Antwort lesen (ohne Netz), Konfigurationsfelder.
2. Netzzugriff hinter einer Schnittstelle, Hintergrund-Prüfung mit Takt, Statusanzeige; Einstellungsabschnitt „Updates“.
3. Meldung mit Patchnotes (Anzeige, Später/Überspringen, mehrere Versionen).
4. Download mit Prüfsumme und Austausch mit Neustart, Rückfrage bei ungespeicherter Arbeit, Hinweis nach dem Update.
5. Bauen und Veröffentlichen: Workflow bei Tag, Prüfsumme, Lizenzhinweise, `CHANGELOG.md`, Hilfe zum Erstellen eines Releases, erstes Dev-Release (erst nach Dennis' Wort; Repository öffentlich nur auf sein Wort).
6. Aufräumen und Dokumentation (README, offene Punkte).

Jede Etappe mit Sichtprüfung im echten Fenster bei 1920 × 1080; echtes Update zuletzt mit einer Testversion, die auf eine ältere Testversion aktualisiert.

## Festlegungen nach der Bauherren-Prüfung (Claudes Entscheidungen, bitte prüfen)

1. **Nur die exe.** Das Release enthält `planoracad-windows-x64.exe`, `planoracad-windows-x64.sha256` und `THIRD-PARTY-LICENSES.txt` (Lizenzhinweise als eigenes Release-Asset); kein Zip, kein Entpacker. Die Lizenzhinweise sind zusätzlich im Programm unter „Über“ einsehbar. Die Prüfsumme schützt nur vor Übertragungsfehlern (siehe Nicht-Ziele).
2. **HTTP-Client:** Der Bauer wählt zuerst einen kleinen Client, der ohne neue Lizenzen auskommt (z. B. mit der Windows-eigenen TLS-Schicht). Passt keine Wahl in die Lizenzliste, bleibt `deny.toml` unverändert und der Bauer meldet das an den Controller; `unsafe` ist im Projekt bisher nicht verwendet (keine Lint-Regel dagegen) und wird nur als letzter Ausweg und eng begrenzt zugelassen (WinHTTP-Aufrufe), mit Rückfrage bei Dennis.
3. **Neustart:** Die alte Instanz legt vor dem Austausch einen Eintrag `update_pending` (von-Version, nach-Version) in die Konfiguration, tauscht aus und startet die neue exe mit dem Startargument `--nach-update`. Die neue Instanz schreibt nach erfolgreichem Start die Datei `planoracad.update-ok` neben die exe, löscht `update_pending` und zeigt den Hinweis „Aktualisiert auf Version X“. Die alte Instanz wartet bis zu 10 Sekunden auf `planoracad.update-ok`; fehlt sie, legt sie den alten Stand zurück (`.old` zurückbenennen), vermerkt die Version als übersprungen (`skipped_version`) und zeigt eine Fehlermeldung. Findet eine Instanz beim Start ein `update_pending`, dessen Zielversion nicht die laufende ist (Austausch war nicht erfolgreich), löscht sie den Eintrag und wiederholt das Update nicht.
4. **Weitere Instanzen:** Windows erlaubt das Umbenennen einer laufenden exe auch bei mehreren Instanzen; Austausch ist deshalb kein Fehlerfall. Andere Instanzen laufen mit dem alten Stand weiter; das Löschen von `.old` ist ein „versuche es, ignoriere den Fehler“.
5. **Patchnotes:** eigener kleiner Mini-Parser (Überschriften mit `#`, Aufzählungen mit `-`, Absätze, **fett**) ohne neue Bibliothek; Obergrenzen: die letzten 20 Releases und 20 000 Zeichen insgesamt, darüber steht „… und weitere Änderungen auf GitHub“.
6. **Version, Tag, Changelog müssen übereinstimmen:** `workspace.package.version` in `Cargo.toml` ist die Quelle; der Veröffentlichungsablauf bricht ab, wenn der Tag `vX` nicht zur Cargo-Version passt oder `CHANGELOG.md` keinen Abschnitt für diese Version hat.
7. **Zuordnung:** Das Programm akzeptiert genau die Dateinamen `planoracad-windows-x64.exe` und `planoracad-windows-x64.sha256`; fehlt eine, ist die Version nicht installierbar (Meldung, keine Aktualisierung).
8. **Quelle fest:** Der Repository-Name steht fest im Programm (`KaelanTesseract/PlanoraCAD`), nicht einstellbar; Downloads werden nur von der Release-URL dieses Repositorys akzeptiert.
9. **Rückfrage bei ungespeicherter Arbeit:** gleiche Mechanik wie beim Schließen (`close_requested`-Weg: Speichern, Verwerfen, Abbrechen je Tab); der Bauer prüft den Code und ruft sie für den Neustart auf. Ein Abbruch in der Rückfrage bricht das Update ab, ohne die exe angefasst zu haben.

## Tatsächliche Ergebnisse (Stand nach Aufgabe 7, 2026-10-25)

Gebaut ist alles aus den Festlegungen oben. Folgendes ist anders gekommen oder dazugekommen:

1. **Frist für die Rückmeldung der neuen Version: 30 Sekunden statt 10.** Auf einem Rechner mit Virenprüfung und Grafikstart kann die neue Version länger brauchen. Die neue Version schreibt `planoracad.update-ok` gleich nach dem Start des Fensters und der Grafik; in der Datei steht ihre Versionsnummer. Die alte Version nimmt nur eine Datei mit der Zielversion an, so dass eine übrig gebliebene Datei aus einem früheren Versuch nicht zählt.
2. **Neustart wartet auf einen ruhigen Moment.** Eine offene Rückfrage, ein laufender Export oder ein Angebot zur Wiederherstellung unterbrechen den Neustart nicht mehr; das Fenster zeigt „Update bereit, wartet auf …“ und macht danach von allein weiter.
3. **Prüfsumme wird noch einmal unmittelbar vor dem Austausch geprüft**, weil zwei laufende Programme dieselbe `.new`-Datei benutzen könnten.
4. **Änderungsnotizen** gelangen über die kleine Datei `planoracad-update-notes.md` neben der Konfiguration in die neue Version, weil es keinen Zwischenspeicher der letzten Prüfung gibt und die neue Version wegen des 24-Stunden-Takts nicht selbst nachfragt.
5. **Eigene SHA-256-Berechnung** (FIPS 180-4, mit den Prüfvektoren getestet), weil keine passende Bibliothek ohne neue Lizenz zur Verfügung stand.
6. **HTTP-Client:** `ureq` 3.5 mit der Windows-eigenen TLS-Schicht (Schannel); `deny.toml` blieb unverändert. Weiterleitungen beim Herunterladen werden von Hand gefolgt (höchstens 5, nur zu `github.com` und `*.githubusercontent.com`). Die Abfrage der Liste hat eine Gesamtzeit von 30 s (je Abschnitt 10 s) statt der 10 s im Entwurf.
7. **Testschalter `update-test-source`:** Er erlaubt nur für Tests eine lokale Quelle (`http://127.0.0.1`). Er ist standardmäßig aus, und `release.yml` baut ohne ihn.
8. **Stiller Fehler bei kaputter Veröffentlichung:** Das Programm überspringt sie ohne Meldung (Festlegung 7 sah eine Meldung vor) und nimmt die nächstältere gültige.
9. **„Später“ und Esc** bedeuten immer „Später“, nie „Überspringen“.
10. **Veröffentlichungsablauf:** `ci.yml` läuft nur von Hand, nicht bei jedem Push; `release.yml` führt keine vollen Tests aus, `tools/release.ps1` verlangt dafür einen grünen `ci.yml`-Lauf auf genau diesem Commit und setzt nie selbst einen Tag.
11. **Bei der Arbeit gefundene Fehler, die schon vorher da waren** (Aufgabe 5b): Das Schlüsselwort `use<>` für Linux in einem Test, ein Zeitwettlauf im Test der Maße, Kurznamen im Temp-Ordner von Windows-Läufern, Windows-Pfade in Linux-Tests und winzige Rundungsunterschiede zwischen Windows und Linux bei Vergleichsdateien. Das Programm selbst musste dafür nicht geändert werden.

**Nicht geprüft:** Die echte Netzabfrage gegen `api.github.com`, eine echte Aktualisierung von einer Testversion aus und alle GitHub-Schritte von `release.yml` und `ci.yml` sind noch nie gelaufen (siehe `docs/superpowers/offene-punkte.md`, Abschnitt „Update-Funktion“).
