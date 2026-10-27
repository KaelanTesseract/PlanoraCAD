# Sprachen für PlanoraCAD – Entwurf

Stand: 2026-10-27. Wunsch von Dennis: „Wir müssen noch einfügen, dass man die App auf Englisch stellen kann.“ – „Das Programm soll weltweit nutzbar sein.“ Mit „Meine Entscheidung“ markierte Stellen sind Vorschläge von Claude, die Dennis in der Zusammenfassung gesehen und mit „passt so“ bestätigt hat; sie dürfen beim Bauen begründet angepasst werden.

## Warum

PlanoraCAD ist seit 0.0.2-dev.1 öffentlich. Alle Texte im Programm sind fest auf Deutsch im Code (grob über 2000 Texte, davon rund 1000 in `planora-app`, der Rest vor allem als Fehlermeldungen in `planora-feature`, `planora-step`, `planora-kernel`, `planora-project`, `planora-drawing`, `planora-sketch`). Wer kein Deutsch kann, kann das Programm nicht nutzen.

## Ziele

1. **Englisch** als zweite Sprache, vollständig: Menüs, Fenster, Knöpfe, Hinweise, Tooltips, Statusleiste, Meldungen, Fehlermeldungen aus allen Crates, die der Benutzer sieht.
2. **Weitere Sprachen ohne Programmieren:** Jede Sprache ist eine eigene Textdatei. Eine neue Sprache = Datei anlegen, übersetzen, beilegen. Eine Anleitung (`docs/uebersetzen.md`, auf Englisch, weil sich Übersetzer weltweit melden) erklärt das.
3. **Sprachwahl:**
   - Beim ersten Start nimmt das Programm die Windows-Sprache (Benutzeroberflächen-Sprache). Gibt es dafür keine Datei, gilt Englisch.
   - In den Einstellungen gibt es eine Auswahl „Sprache“; jede Sprache steht dort in ihrem eigenen Namen („Deutsch“, „English“). Die Wahl wird in der Konfiguration gespeichert.
   - Das Umstellen wirkt sofort, ohne Neustart (Meine Entscheidung).
   - Bestehende Installationen (Konfiguration ohne Spracheintrag) werden behandelt wie ein erster Start: Windows-Sprache, sonst Englisch. Auf einem deutschen Windows bleibt also alles deutsch. (Meine Entscheidung.)
4. **Zeichnungen und Exporte:**
   - Beschriftungen, die in einer Zeichnung oder exportierten Datei landen (Schriftfeld-Bezeichnungen wie Maßstab, Blatt, Gezeichnet; Schnitt- und Ansichtsbezeichnungen; Stücklisten-Überschriften; PDF-Dokumenteigenschaften, soweit Text vom Programm), folgen voreingestellt der Programmsprache.
   - Im Export-Fenster kann man für den Export eine andere Sprache wählen. Die Anzeige im Programm bleibt dabei in der Programmsprache.
   - Die Wahl gibt es nur bei Zeichnungs-Exporten (alle Zeichnungsformate, z. B. PDF und SVG); Netz- und STEP-Exporte enthalten keine Programmtexte. Voreinstellung ist „wie das Programm“; die Wahl gilt, solange das Export-Fenster offen ist, und wird weder im Projekt noch in der Konfiguration gespeichert. (Meine Entscheidung.)
   - Feste Beschriftungen, die heute in `planora-drawing` stehen (z. B. Stücklisten-Überschriften `PARTS_HEAD`), bekommt die Zeichnungs-Crate von `planora-app` in der Export-Sprache übergeben (siehe Festlegung 5).
   - Vom Benutzer eingegebene Texte (Namen, Titel, Schriftfeld-Inhalte) werden nie übersetzt.
   - Namen, die das Programm vorschlägt („Zeichnung 1“, „Körper 2“ usw.), entstehen in der Programmsprache, die beim Anlegen gilt, und gelten danach als Benutzertext: Sie werden nie nachträglich übersetzt. Kein Wechsel des Projektformats. (Meine Entscheidung.)
5. **Protokolle und Berichte immer Englisch:** Diagnose-Protokoll, letzte Schritte, Absturzbericht, die technischen Teile von `rueckmeldung.txt` und `planoracad-update.log`-Einträge des Programms sind immer englisch, unabhängig von der Programmsprache. Fenster, die der Benutzer dazu sieht (Rückmelde-Fenster, Absturz-Rückfrage), folgen seiner Sprache. Vom Benutzer geschriebener Text bleibt, wie er ist.
6. **Zahlen:**
   - Anzeige je nach Sprache: Deutsch `38,8`, Englisch `38.8` (Dezimalzeichen kommt aus der Sprachdatei).
   - Eingabefelder nehmen immer Komma und Punkt als Dezimalzeichen an.
   - Zahlen in Zeichnungen und Exporten folgen der Export-Sprache.
   - Keine Tausendertrennzeichen (wie bisher).
7. **Rückmeldung je nach Sprache:**
   - Webformular: Deutsch `https://dennis-guse.de/planoracad/rueckmeldung.php`, sonst `https://dennis-guse.de/en/planoracad/feedback.php` (gleiche Feldnamen). Die englische Seite gibt es schon (Dennis); hier ändert sich nur die Adresse im Programm, auf der Website nichts.
   - GitHub: Die Vorlagen `fehler.yml` und `wunsch.yml` bekommen englische Gegenstücke; das Programm öffnet bei Deutsch die deutsche, sonst die englische Vorlage. `config.yml` nennt beide Wege.
8. **Deutsch bleibt unverändert:** Nach der Umstellung sieht das Programm auf Deutsch genau aus wie vorher (gleiche Texte, gleiche Längen). Ausnahme: Texte, die durch Ziel 5 englisch werden.

## Keine Ziele

- **Zoll / andere Einheiten:** eigenes Teilprojekt (in `offene-punkte.md` vermerken).
- Andere Sprachen als Deutsch und Englisch werden jetzt nicht mitgeliefert, nur ermöglicht.
- Schrift von rechts nach links (Arabisch, Hebräisch) und Schriften, die die eingebauten Schriftarten nicht abdecken (z. B. Chinesisch, Japanisch). Die Anleitung sagt ehrlich, dass solche Sprachen erst nach einer Erweiterung gehen. (Meine Entscheidung.)
- Übersetzung von Wiki, README und `docs/` (Wiki gibt es schon auf Englisch).
- Installer: hat Deutsch und Englisch schon.
- Datums- und Uhrzeitformate über das hinaus, was Sprachdateien ohnehin festlegen.

## Festlegungen

1. **Format: Project Fluent** (`.ftl`-Dateien, Crates `fluent-bundle`/`fluent`; Lizenz Apache-2.0/MIT, mit `cargo deny` prüfen). Kann Platzhalter und Mehrzahl, ist unter Übersetzern verbreitet. (Meine Entscheidung.)
2. **Ablage:** Die Sprachdateien für Deutsch und Englisch werden ins Programm eingebaut (immer vorhanden, auch ohne Installer). Zusätzliche Sprachen liegen als `.ftl` in einem Ordner `languages` neben der exe (der Installer liefert ihn mit) und werden beim Start gelesen. Wie genau Ordner und Dateinamen heißen, entscheidet der Plan. (Meine Entscheidung.)
3. **Rückfall:** Fehlt ein Text in einer Sprache, erscheint der englische. Fehlt er auch dort (Programmfehler), erscheint der Schlüssel; nie ein Absturz. Eine fehlerhafte Sprachdatei wird übergangen und im (englischen) Protokoll vermerkt.
4. **Prüfungen:** Ein Test stellt sicher, dass Deutsch und Englisch dieselben Schlüssel haben, dass jeder im Code benutzte Schlüssel existiert und dass die Platzhalter übereinstimmen. Ein zweiter Test belegt Ziel 8 an Stichproben (bestehende Text-Tests bleiben deutsch und laufen weiter mit Sprache Deutsch).
5. **Fehlermeldungen der Bibliotheks-Crates:** Die Crates außer `planora-app` bekommen keine Sprachdateien. Ihre Fehlertypen bleiben Daten; der Text für den Benutzer entsteht in `planora-app` über die Sprachdatei, der englische Text fürs Protokoll ebenso. Wie die Zuordnung technisch aussieht (z. B. Schlüssel je Fehlerfall, Platzhalter für Werte), entscheidet der Plan. (Meine Entscheidung.)
6. **Englische Texte** schreibt Claude, in derselben einfachen Sprache wie die deutschen (kurze Sätze, Alltagswörter, „you“-Form). Fachwörter wie in gängigen englischen CAD-Programmen (Extrude, Revolve, Fillet, Chamfer, Sketch, Constraint), ohne Markennamen fremder Programme.
7. **Breite:** Englische Texte sind oft anders lang. Das Layout muss bei 1920 × 1080 in beiden Sprachen ohne abgeschnittene oder überlappende Texte funktionieren; kleinere Fenster sind kein Ziel.

## Vorgehen (Meine Entscheidung)

In Etappen, jede mit Prüfung und Sichtprüfung im Fenster (nur mit Dennis' Go):

1. Grundgerüst: Sprachdateien, Laden, Rückfall, Sprachwahl in den Einstellungen, Windows-Sprache beim ersten Start, Zahlenformat je Sprache, Eingabe mit Komma und Punkt.
2. Texte von `planora-app` in die deutsche Datei umziehen (Deutsch sieht danach gleich aus), Bereich für Bereich.
3. Fehlermeldungen der anderen Crates anbinden.
4. Protokolle und Berichte auf Englisch, Rückmeldung je nach Sprache (Webformular, GitHub-Vorlagen).
5. Englische Datei vollständig, Export-Sprache im Export-Fenster.
6. Anleitung für Übersetzer, Installer liefert den Sprachordner mit, CHANGELOG, Wiki-Hinweis zur Sprachwahl.

## Prüfen

- Alle bestehenden Tests laufen weiter (mit Sprache Deutsch).
- Neue Tests für Laden, Rückfall, Vollständigkeit, Zahlen, Export-Sprache, englische Berichte, Wahl von Formular und Vorlage.
- Sichtprüfung bei 1920 × 1080 auf Deutsch und Englisch, hell und dunkel, alle Fenster.
- Lizenzprüfung der neuen Crates mit `cargo deny`.

## Dem Bauenden überlassen

- Schlüsselnamen und Aufteilung der `.ftl`-Dateien.
- Wie die aktuelle Sprache im Code erreichbar ist (z. B. über den App-Zustand oder einen Kontext).
- Wie die Windows-Sprache gelesen wird.
- Reihenfolge der Bereiche innerhalb von Etappe 2.
