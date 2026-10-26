# Rückmeldungen einsammeln – Entwurf

Stand: 2026-10-26. Teilprojekt von PlanoraCAD (Rust, egui). Geschrieben nach Dennis' Auftrag; die mit „Meine Entscheidung“ markierten Stellen sind Annahmen von Claude, die Dennis beim Lesen prüfen soll.

## Warum

Ein externer Tester bekommt die App. Er soll Fehler und Wünsche ohne Aufwand melden können. Dennis soll genug Angaben bekommen, um einen Fehler selbst nachzustellen: Version, System, Grafikkarte, die letzten Schritte, eine Fehlermeldung, auf Wunsch ein Bild.

Die Update-Funktion ist schon gemergt (Merge 5c88844, noch nicht veröffentlicht). Dieses Teilprojekt baut darauf auf und teilt sich mit ihr Versionsangabe, Repo-Adresse und das Release-Verfahren.

## Ziele

- Ein Knopf in der App (Hilfe → Rückmeldung …, mit änderbarem Tastenkürzel) öffnet ein Fenster. Der Tester wählt Fehler, Wunsch oder Frage, schreibt zwei Sätze und klickt. Im Browser öffnet sich ein vorausgefülltes GitHub-Formular.
- Nichts verlässt den Rechner ohne sichtbare Zustimmung. Das Fenster zeigt genau den Text, der in den Link kommt.
- Bei einem Absturz bleibt ein Bericht auf der Platte. Beim nächsten Start fragt die App, ob er gespeichert oder gesendet werden soll.
- Wer kein GitHub-Konto hat, hat einen zweiten Weg.
- Wünsche lassen sich mit 👍 gewichten, und es gibt eine öffentliche Übersicht „Geplant / In Arbeit / Fertig“.

## Keine Ziele

- Kein eigener Server, keine Zugangsdaten in der App, kein Dienst, der Daten im Hintergrund sammelt.
- Kein Vergleich mit Fusion als Testhilfe: kein Label „unterschied-zu-fusion“, keine Abhak-Liste.
- Keine Nutzungsstatistik, kein Abgleich mit einem Konto, keine automatische Meldung ohne Klick des Testers.
- Abstürze ohne Rust-Meldung (Zugriffsfehler, Grafiktreiber) werden nicht abgefangen, nur als „letzter Lauf endete unerwartet“ erkannt.

## Gegenziele (daran würde es scheitern, auch wenn es technisch läuft)

- Der Tester versteht nicht, was gesendet wird.
- Etwas wird gesendet, ohne dass er es gesehen und bestätigt hat.
- Persönliche Angaben (Benutzername im Pfad, Projektnamen, Mailadresse) landen unbemerkt in einem öffentlichen Issue.
- Das Melden dauert länger als zwei Minuten.

## Festlegungen

1. **Datenschutz zuerst.** Jeder Bestandteil der Meldung steht im Fenster als Text und hat ein Häkchen. Standardmäßig an: Version, System, Grafikkarte, letzte Schritte. Standardmäßig aus: Protokollzeilen, Bild, Projektdatei. Es geht nichts über das Netz, außer der Tester klickt im Browser selbst auf „Absenden“. Die App öffnet nur eine Adresse.
2. **Bereinigung.** Vor der Anzeige ersetzt die App den Benutzernamen in Pfaden („C:\Users\<Name>\…“), kürzt Projektpfade auf den Dateinamen und entfernt Mailadressen aus Protokollzeilen. Der Tester sieht das Ergebnis.
3. **Länge der Adresse.** Die Adresse bleibt unter 2000 Zeichen (nach dem Kodieren). Passt der Text nicht, kürzt die App in dieser Reihenfolge: Protokollzeilen, letzte Schritte, Beschreibung bleibt immer ganz. Die vollständigen Angaben liegen als Datei in einem Ordner, den die App öffnet. Im Formular steht „Datei hier anhängen“.
4. **Bilder und Dateien** gehen nicht über die Adresse. Die App legt das Bild (PNG) und, wenn gewünscht, die Projektdatei in einen Rückmeldungsordner (`%APPDATA%\PlanoraCAD\rueckmeldungen\<Datum-Uhrzeit>\`), öffnet ihn im Explorer und sagt, welche Datei der Tester ins Formular ziehen soll.
5. **Keine neuen Abhängigkeiten, wenn es geht.** `png` ist schon da (Bild kodieren), wgpu liefert die Grafikkarte (`adapter.get_info()`), Browser öffnet `ctx.open_url`. Für Windows-Version und Zeitstempel entscheidet der Umsetzer (siehe unten). Jede neue Abhängigkeit muss `cargo deny` bestehen und GPL‑3‑kompatibel sein.
6. **Labels.** GitHub setzt Labels aus der Adresse nur bei Leuten mit Schreibrecht. Deshalb bekommen die Formulare feste Labels (`bug` bzw. `idee`), und die Bereichs-Labels (Sketch, 3D, Export, Tastenkürzel, `frage`) setzt Dennis beim Sortieren, aus einem Auswahlfeld „Bereich“ im Formular.
7. **Tastenkürzel.** Neue Aktion `FeedbackOpen`, Bereich „Überall“, Standard **Strg+Umschalt+R** (Meine Entscheidung: frei, kein Fusion-Konflikt, „R“ wie Rückmeldung). Änderbar in Einstellungen → Tastenkürzel, in der Übersicht sichtbar, Id fest `hilfe.rueckmeldung`.
8. **Texte** in einfacher Sprache, Anrede „Du“, wenig Technik, kein KI-Ton. Oberfläche im Graphit-Stil (`Dialog`, `theme`), ausgelegt und geprüft ab 1920×1080.
9. **Prüfen.** Lokal (Windows und WSL). GitHub-CI nur von Hand vor einem Release. Dieser Punkt weicht von Dennis' Auftrag ab („vor dem Merge einmal die GitHub-CI“); es gilt seine neuere Regel vom 2026-10-25 (Meine Entscheidung, bitte bestätigen).

## Bausteine und Etappen

Jede Etappe endet mit einer Sichtprüfung durch Dennis (bei Etappe 1 im Browser, danach im Fenster per `tools/gui-drive.ps1`).

### Etappe 1: GitHub-Vorlagen und Anleitung (keine Programmänderung)

- `.github/ISSUE_TEMPLATE/fehler.yml`, `.github/ISSUE_TEMPLATE/wunsch.yml`, `.github/ISSUE_TEMPLATE/config.yml` (leere Issues aus, Link „Häufige Probleme“ und „So meldest du etwas“).
- Pflichtfelder Fehler: „Was wolltest du tun?“, „Was ist passiert?“, „Was hast du erwartet?“. Dazu: Auswahl „Bereich“ (Skizze, 3D, Export, Zeichnung, Tastenkürzel, Anderes), Version, System, Grafikkarte, letzte Schritte, Bild (freiwillig). Alle Felder haben feste `id`s, damit die App sie über die Adresse vorausfüllen kann.
- Wunsch: „Was möchtest du tun können?“, „Wie machst du es heute?“, „Wie wichtig ist es dir?“ (freiwillig), Bereich. Hinweis: 👍 unter dem Wunsch zeigt, wie viele es auch wollen.
- Labels anlegen: `bug`, `idee`, `frage`, `Sketch`, `3D`, `Export`, `Tastenkürzel` (Skizze/Zeichnung als eigene Labels ist Dennis' Wahl; der Umsetzer legt die genannten sieben an).
- `docs/so-meldest-du-etwas.md` in einfacher Sprache (mit und ohne Konto, was geht an wen, wie Bilder angehängt werden). Verweis aus README.
- Prüfung: GitHub zeigt die Formulare nur auf dem Standardzweig. Sichtprüfung nach dem Push durch Dennis. Das Anlegen der Labels braucht Schreibrecht auf das Repo (`gh label create`); die Ausführung nur auf Dennis' Wort.

### Etappe 2: Rückmeldungs-Fenster in der App

- Neues Menü **Hilfe** (nach Ansicht) mit: Rückmeldung … (zeigt das Kürzel), Häufige Probleme (öffnet die Seite im Browser), Über PlanoraCAD (wandert aus „Datei“ hierher).
- Modul `feedback` (Fenster `feedback_dialog.rs`, Logik ohne Oberfläche in `feedback.rs`): Art (Fehler/Wunsch/Frage), Beschreibung (Textfeld, Pflicht, mind. 10 Zeichen), Häkchen mit Vorschau je Bestandteil, Knöpfe „Im Browser öffnen“ und „Abbrechen“. Das Fenster zeigt unten die fertige Adresse in Textform (einklappbar) und „Text kopieren“.
- Angaben, die die App selbst sammelt: Version und Kanal (aus `running_version()`), Windows-Version, Grafikkarte (Name, Treiber, Backend), Bildschirmgröße, die letzten **Schritte** und auf Wunsch die letzten **Protokollzeilen**.
- „Bild vom Fenster mitschicken“: Das Fenster schließt sich kurz, die App macht über `ViewportCommand::Screenshot` ein Bild, kodiert es mit `png` und legt es in den Rückmeldungsordner. Das Rückmeldungsfenster erscheint danach wieder.
- Ohne Konto: ein zweiter Knopf „Ohne GitHub-Konto senden“ (siehe Baustein 4).
- Protokoll und Schritte brauchen neue Bausteine:
  - Modul `diag`: kleiner Ringpuffer (500 Zeilen) plus Datei `%APPDATA%\PlanoraCAD\logs\planora.log` (höchstens 1 MB, drei Dateien rotierend). Aufrufe `diag::info/warn/fehler`. Standard: Start, Version, Grafikkarte, Fehlermeldungen, Dateivorgänge ohne Inhalt. Kein Projektinhalt.
  - Schritte: ein Ringpuffer (50 Einträge) in `commands::run_action` und an den Stellen, an denen `undo_labels` schon deutsche Namen liefert.
- Config: Neue Felder sind nicht nötig (Tastenkürzel liegt in `keys`). Ein Schalter „Ausführliches Protokoll“ kommt in Etappe 6.

### Etappe 3: Absturzbericht

- `std::panic::set_hook` beim Start: schreibt Version, Panik-Nachricht, Ort, Backtrace (`std::backtrace`), die letzten Schritte und die letzten 100 Protokollzeilen in `%APPDATA%\PlanoraCAD\abstuerze\absturz-<Datum-Uhrzeit>.txt`, ruft danach den vorherigen Hook auf. Der Hook darf selbst nie panicen und nichts anderes tun (kein Netz, kein Fenster).
- Start-Markierung: Beim Start legt die App `laeuft.marker` an, beim sauberen Beenden löscht sie ihn. Findet der nächste Start den Marker, aber keinen Bericht, ist der letzte Lauf unerwartet beendet worden (Zugriffsfehler, Treiber, Task-Manager).
- Beim nächsten Start (nach dem ersten Bild, nicht während des Updates) fragt ein Fenster: „PlanoraCAD ist beim letzten Mal abgestürzt. Möchtest du uns helfen?“ Knöpfe: **Bericht ansehen**, **Als Rückmeldung öffnen** (öffnet das Rückmeldungsfenster mit dem Bericht), **Bericht speichern/Ordner öffnen**, **Nicht jetzt** (Bericht bleibt liegen), **Löschen**. Die Projektdatei hängt nur mit einem eigenen Häkchen an („Projektdatei in den Ordner legen“, Standard aus). Die Autosicherungs-Frage (`AutosaveOffer`) kommt zuerst.
- Bei „unerwartet beendet“ ohne Bericht: kurzer Hinweis mit Weg in das Rückmeldungsfenster, ohne Bericht.
- Berichte älter als 30 Tage und mehr als 10 Stück räumt die App beim Start auf.

### Etappe 4: Weg ohne GitHub-Konto

Geprüft (Stand Okt. 2026): mailto-Link, Formular-Dienste (Tally, Formspree, Web3Forms, Netlify Forms), eigene Brücke (Cloudflare Worker → GitHub-Issue), Mail direkt, GitHub-Konto.

- **Empfehlung (Hauptweg): Tally-Formular.** Link aus der App, öffnet im Browser, Version und System werden über die Adresse vorausgefüllt (versteckte Felder). Gratis-Stufe mit unbegrenzten Einsendungen, Dateianhang bis 10 MB, Meldung kommt als Mail an Dennis. Kein Server, kein Schlüssel in der App. Tally ist ein belgischer Anbieter und gibt Hosting in Europa an. Zu prüfen vor dem Einsatz (Dennis): Hosting-Zusage und Auftragsverarbeitungsvertrag in den Konto-Einstellungen; ein Vergleichsportal behauptet, EU-Hosting sei kostenpflichtig.
- **Notweg: Mail** an `info@dennis-guse.de` per `mailto:` mit kurzem Text (unter 2000 Zeichen kodiert). Anhänge gehen so nicht; das Fenster nennt den Ordner mit dem Bild. Die Adresse steht zusätzlich als Text mit „Kopieren“-Knopf (wer nur Webmail hat, bekommt oft keinen Entwurf).
- Für Leute mit Konto bleibt der vorausgefüllte GitHub-Link der Hauptweg.
- Nicht empfohlen: eigene Brücke (Schlüssel, Spam, Pflege, öffentliche Issues mit persönlichen Angaben), Formspree (50/Monat, keine Anhänge, USA), Google Forms.
- Datenschutzhinweis (README und `docs/so-meldest-du-etwas.md`): wer die Meldung bekommt, wofür, Löschung auf Wunsch, Mailadresse freiwillig.
- **Meine Entscheidung:** Das Tally-Formular legt Dennis selbst an (Konto, Felder, Datenschutz). Die App bekommt die Formular-Adresse als Konstante; ohne eingetragene Adresse zeigt sie nur den Mail-Weg. So blockiert das Konto den Rest nicht.

### Etappe 5: Wünsche gewichten und Übersicht

- 👍 unter dem Issue: braucht nichts. Das Formular sagt es im Text.
- Öffentliche Übersicht als GitHub-Projekttafel (Spalten **Geplant**, **In Arbeit**, **Fertig**), Issues kommen automatisch hinein, wenn das Repo-Projekt „Neue Issues automatisch hinzufügen“ hat. Die Tafel anlegen und öffentlich schalten kann nur Dennis (oder `gh` mit dem Recht `project`; nur auf sein Wort). Der Umsetzer liefert die Schritt-für-Schritt-Anleitung (`docs/projekttafel-einrichten.md`) und den Link im README.

### Etappe 6: Kleinkram

- Nach einem Update zeigt „Aktualisiert auf Version X“ (vorhanden) zusätzlich den Satz „Etwas stimmt nicht? **Rückmeldung geben**“ mit Knopf.
- Einstellungen → Hilfe: Schalter **Ausführliches Protokoll** (mehr `diag`-Zeilen: Befehle, Dateivorgänge, Zeiten; weiter ohne Projektinhalt) mit Hinweis, dass nichts gesendet wird. In der Config: `verbose_log` mit `serde(default)`, lenient gelesen, über alle vier Schreibwege mitgeführt (wie `update_*`).
- `docs/haeufige-probleme.md` (leer-anfangs mit den bekannten Stolpersteinen aus `offene-punkte.md`: Windows-Warnung bei unsignierter Datei, Grafiktreiber, Antivirus). Link im Hilfe-Menü und in `config.yml`.
- Release-Text (`CHANGELOG`/Release-Vorlage) endet mit „So meldest du etwas: <Link>“.

## Zusammenspiel mit der Update-Funktion

Gemeinsam gedacht und geteilt:
- **Eine Quelle für Version und Herkunft.** `running_version()` und `REPO` (`planora-update`) bleiben die einzigen Quellen. Eine Hilfsfunktion `build_info()` (Version, Kanal, Windows-Version, Grafikkarte) kommt in Etappe 2 und wird vom Über-Dialog, vom Rückmeldungsfenster, vom Absturzbericht und vom Update-Dialog genutzt. Die Issue-Adresse baut sich aus `REPO`, nicht aus einer zweiten Konstanten.
- **Aktualisierter Start.** Der Absturz-Dialog (Etappe 3) wartet, bis die Meldung „Aktualisiert auf …“ und der erste Frame durch sind, und zeigt sich nie während des Neustart-Ablaufs.
- **Release.** Etappen 1 und 2 sollten im ersten Dev-Release (`v0.2.0-dev.1`) stecken, damit der Tester von Anfang an melden kann. `CHANGELOG.md` bekommt dafür Einträge, und die Release-Vorlage verlinkt die Anleitung.
- **Datenschutz-Text.** Das README hat den Abschnitt zur Update-Abfrage; Rückmeldungen bekommen einen eigenen Abschnitt daneben, beide verweisen aufeinander.

Getrennt bleiben darf:
- Etappe 1 (GitHub-Vorlagen und Anleitung) braucht keine Programmänderung und kann sofort auf `master`.
- Etappen 4, 5 und 6 sind unabhängig und können nach dem ersten Release folgen. Der Testmodus (6) und die Häufige-Probleme-Seite brauchen kein Update.
- Das Absturz-Modul (3) berührt den Update-Neustart nur an einer Stelle (Zeitpunkt der Rückfrage).

## Randbedingungen für den Umsetzer

- Branch `feature/rueckmeldungen`, Etappen als getrennte Aufgaben, jede mit eigener Prüfung.
- Neue Texte als `pub const` im Modul, von Tests geprüft (wie `about_dialog`). Tastenkürzel über `action_table!` (`keymap/defaults.rs`), Ablauf in `commands::run_action`.
- Reine Logik (Adresse bauen und kürzen, Bereinigung, Berichtsdatei, Marker, Aufräumen) als Funktionen ohne Oberfläche mit Tests; Oberfläche per Sichtprüfung mit `tools/gui-drive.ps1` bei 1920×1080 (und kein Absturz bei 800×600).
- Der Panic-Hook wird in einem Test mit einem Thread geprüft, der absichtlich panict (Hook schreibt die Datei, ruft den früheren Hook).
- Der Bild-Weg wird in einem Test mit einer Bildattrappe geprüft (Kodierung, Pfad); das echte `ViewportCommand::Screenshot` per Sichtprüfung.
- Der Fehler-Weg muss geprüft sein: Ordner nicht beschreibbar, Browser lässt sich nicht öffnen (Hinweis mit „Text kopieren“), sehr lange Beschreibung, Umlaute und Sonderzeichen in der Adresse, Zeilenumbrüche.
- Windows-Version und Zeitstempel: Der Umsetzer wählt zwischen Standardbibliothek/Systembefehl und `windows-sys` (bereits im Baum; genau fest eingetragene Version nötig wegen `wildcards = deny`). Meine Annahme: Registry- oder `RtlGetVersion`-Aufruf über `windows-sys`, Zeitstempel von Hand aus `SystemTime` (UTC).
- Keine Fahrzeugmarken und kein Autodesk-Material in Texten und Beispielen.

## Offene Punkte für Dennis

1. Tally als Hauptweg ohne Konto und Mail als Notweg – oder nur Mail? (Empfehlung: beides; du legst das Formular an.)
2. Standardkürzel Strg+Umschalt+R in Ordnung?
3. GitHub-CI nur vor dem Release (deine Regel vom 25.10.), nicht vor dem Merge: so festgehalten.
4. Projekttafel: du legst sie an, oder darf ich es mit `gh` tun (Recht `project`, ein `gh auth refresh -s project` von dir)?
5. Labels per `gh label create` in deinem Repo anlegen: erlaubt?

6. Die Mailadresse `info@dennis-guse.de` wird als öffentliche Kontaktadresse in App und README genannt (Notweg in Etappe 4). Einverstanden?
7. Datenschutztext: Wer ist verantwortlich (Name, Anschrift)? Bis zur Antwort steht ein Platzhalter „[Verantwortliche Stelle – von Dennis einzutragen]“, damit nichts erfunden wird. Das ist keine Rechtsberatung.

Vom Baumeister-Check geklärt: Das Repo ist öffentlich (seit der Veröffentlichung), Issues und Projekttafel tragen also. Gemeint sind `crates/planora-app/src/undo_labels.rs` und `docs/superpowers/offene-punkte.md` (nicht `docs/offene-punkte.md`). Ohne Antworten auf 1–7 baut Etappe 1 ohne Label-Anlage, Etappe 4 mit Mail-Weg und leerer Tally-Konstante.

## Ausführung (Empfehlung)

Subagenten-gestützt in der Reihenfolge der Etappen, ein Umsetzer pro Etappe mit unabhängiger Prüfung (wie bei der Update-Funktion). Etappe 1 ist klein und ohne Code; dafür genügt ein Umsetzer mit Prüfung am Ende. Etappen 2 und 3 teilen sich `diag` und `build_info()` und sollten nacheinander laufen, nicht parallel. Einsatzreihenfolge: 1 → 2 → 3 → (erstes Dev-Release) → 4 → 5 → 6.
