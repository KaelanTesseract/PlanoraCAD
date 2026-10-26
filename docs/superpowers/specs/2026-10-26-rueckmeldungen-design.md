# Rückmeldungen einsammeln – Entwurf

Stand: 2026-10-26. Teilprojekt von PlanoraCAD (Rust, egui). Geschrieben nach Dennis' Auftrag. Dennis hat die offenen Fragen am 2026-10-26 beantwortet (Abschnitt „Entschieden am 2026-10-26“ am Ende); die Antworten sind eingearbeitet. Was noch „Meine Entscheidung“ heißt, ist eine Annahme von Claude, die Dennis noch nicht bestätigt hat.

## Warum

Ein externer Tester bekommt die App. Er soll Fehler und Wünsche ohne Aufwand melden können. Dennis soll genug Angaben bekommen, um einen Fehler selbst nachzustellen: Version, System, Grafikkarte, die letzten Schritte, eine Fehlermeldung, auf Wunsch ein Bild.

Die Update-Funktion ist schon gemergt (Merge 5c88844, noch nicht veröffentlicht). Dieses Teilprojekt baut darauf auf und teilt sich mit ihr Versionsangabe, Repo-Adresse und das Release-Verfahren.

## Ziele

- Ein Knopf in der App (Hilfe → Rückmeldung …, mit änderbarem Tastenkürzel) öffnet ein Fenster. Der Tester wählt Fehler, Wunsch oder Frage, schreibt zwei Sätze und klickt. Im Browser öffnet sich ein vorausgefülltes GitHub-Formular.
- Nichts verlässt den Rechner ohne sichtbare Zustimmung. Das Fenster zeigt genau den Text, der in den Link kommt.
- Bei einem Absturz bleibt ein Bericht auf der Platte. Beim nächsten Start fragt die App, ob er gespeichert oder gesendet werden soll.
- Wer kein GitHub-Konto hat, hat einen zweiten Weg: eine vorbereitete E-Mail.
- Wünsche lassen sich mit 👍 gewichten, und es gibt eine öffentliche Übersicht „Geplant / In Arbeit / Fertig“.

## Keine Ziele

- Kein eigener Server, keine Zugangsdaten in der App, kein Dienst, der Daten im Hintergrund sammelt.
- Kein Formular-Dienst (Tally, Formspree und ähnliche): Dennis hat Tally abgelehnt.
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
3. **Länge der Adresse.** Die Adresse bleibt unter 2000 Zeichen (nach dem Kodieren). Passt der Text nicht, kürzt die App in dieser Reihenfolge: Protokollzeilen, letzte Schritte, zuletzt die Beschreibung. Die Beschreibung bleibt so lange ganz, wie sie allein in die Adresse passt. Passt sie allein nicht, kürzt die App sie in der Adresse an einer Zeichengrenze und hängt einen Hinweis an; der volle Text steht in `rueckmeldung.txt` im Ordner, den die App öffnet. Die vollständigen Angaben liegen ebenfalls dort. Im Formular steht „Datei hier anhängen“.
4. **Bilder und Dateien** gehen nicht über die Adresse. Die App legt das Bild (PNG) und, wenn gewünscht, die Projektdatei in einen Rückmeldungsordner (`%APPDATA%\PlanoraCAD\rueckmeldungen\<Datum-Uhrzeit>\`), öffnet ihn im Explorer und sagt, welche Datei der Tester ins Formular ziehen soll.
5. **Keine neuen Abhängigkeiten, wenn es geht.** `png` ist schon da (Bild kodieren), wgpu liefert die Grafikkarte (`adapter.get_info()`), Browser öffnet `ctx.open_url`. Für Windows-Version und Zeitstempel entscheidet der Umsetzer (siehe unten). Jede neue Abhängigkeit muss `cargo deny` bestehen und GPL‑3‑kompatibel sein.
6. **Labels.** GitHub setzt Labels aus der Adresse nur bei Leuten mit Schreibrecht. Deshalb bekommen die Formulare feste Labels (`bug` bzw. `idee`), und die Bereichs-Labels (Sketch, 3D, Export, Tastenkürzel, `frage`) setzt Dennis beim Sortieren, aus einem Auswahlfeld „Bereich“ im Formular. „Frage“ bekommt kein eigenes Formular: Sie nutzt `wunsch.yml` mit der Titel-Vorsilbe „Frage: …“; das Label `frage` setzt Dennis.
7. **Tastenkürzel.** Neue Aktion `FeedbackOpen`, Bereich „Überall“, Standard **Strg+Umschalt+R** (von Dennis bestätigt; frei, kein Fusion-Konflikt, „R“ wie Rückmeldung). Änderbar in Einstellungen → Tastenkürzel, in der Übersicht sichtbar, Id fest `hilfe.rueckmeldung`.
8. **Texte** in einfacher Sprache, Anrede „Du“, wenig Technik, kein KI-Ton. Oberfläche im Graphit-Stil (`Dialog`, `theme`), ausgelegt und geprüft ab 1920×1080.
9. **Prüfen.** Lokal (Windows und WSL). GitHub-CI nur von Hand vor einem Release. Dieser Punkt weicht von Dennis' Auftrag ab („vor dem Merge einmal die GitHub-CI“); es gilt seine neuere Regel vom 2026-10-25 (Meine Entscheidung, bitte bestätigen).

## Bausteine und Etappen

Jede Etappe endet mit einer Sichtprüfung durch Dennis (bei Etappe 1 im Browser, danach im Fenster per `tools/gui-drive.ps1`).

### Etappe 1: GitHub-Vorlagen und Anleitung (keine Programmänderung)

- `.github/ISSUE_TEMPLATE/fehler.yml`, `.github/ISSUE_TEMPLATE/wunsch.yml`, `.github/ISSUE_TEMPLATE/config.yml` (leere Issues aus, Link „Häufige Probleme“ und „So meldest du etwas“).
- Pflichtfelder Fehler: „Was wolltest du tun?“, „Was ist passiert?“, „Was hast du erwartet?“. Dazu: Auswahl „Bereich“ (Skizze, 3D, Export, Zeichnung, Tastenkürzel, Anderes), Version, System, Grafikkarte, letzte Schritte, Bild (freiwillig). Alle Felder haben feste `id`s, damit die App sie über die Adresse vorausfüllen kann. Zusätzliche freiwillige Felder (von Dennis bestätigt): `protokoll` im Fehler-Formular, `angaben` im Wunsch-Formular.
- Wunsch: „Was möchtest du tun können?“, „Wie machst du es heute?“, „Wie wichtig ist es dir?“ (freiwillig), Bereich. Hinweis: 👍 unter dem Wunsch zeigt, wie viele es auch wollen.
- Labels anlegen: `bug`, `idee`, `frage`, `Sketch`, `3D`, `Export`, `Tastenkürzel` (Skizze/Zeichnung als eigene Labels ist Dennis' Wahl; die genannten sieben legt das Skript `tools/labels.ps1` an). Dennis erlaubt Umsetzer bzw. Controller, das Skript auszuführen.
- `docs/so-meldest-du-etwas.md` in einfacher Sprache (mit und ohne Konto, was geht an wen, wie Bilder angehängt werden). Verweis aus README.
- Prüfung: GitHub zeigt die Formulare nur auf dem Standardzweig. Dennis erlaubt deshalb, Etappe 1 (Aufgabe 1) vorab nach `master` zu mergen und zu pushen; gepusht wird nur durch den Controller nach der Prüfung. Sichtprüfung danach durch Dennis. Das Anlegen der Labels braucht Schreibrecht auf das Repo (`gh label create` im Skript).

### Etappe 2: Rückmeldungs-Fenster in der App

- Neues Menü **Hilfe** (nach Ansicht) mit: Rückmeldung … (zeigt das Kürzel), Häufige Probleme (öffnet die Seite im Browser), Über PlanoraCAD (wandert aus „Datei“ hierher).
- Modul `feedback` (Fenster `feedback_dialog.rs`, Logik ohne Oberfläche in `feedback.rs`): Art (Fehler/Wunsch/Frage), Beschreibung (Textfeld, Pflicht, mind. 10 Zeichen), Häkchen mit Vorschau je Bestandteil, Knöpfe „Im Browser öffnen“ und „Abbrechen“. Das Fenster zeigt unten die fertige Adresse in Textform (einklappbar) und „Text kopieren“.
- Angaben, die die App selbst sammelt: Version und Kanal (aus `running_version()`), Windows-Version, Grafikkarte (Name, Treiber, Backend), Bildschirmgröße, die letzten **Schritte** und auf Wunsch die letzten **Protokollzeilen**.
- „Bild vom Fenster mitschicken“: Das Fenster schließt sich kurz, die App macht über `ViewportCommand::Screenshot` ein Bild, kodiert es mit `png` und legt es in den Rückmeldungsordner. Das Rückmeldungsfenster erscheint danach wieder.
- Ohne Konto: ein zweiter Knopf „Per E-Mail senden“ (siehe Etappe 4).
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

### Etappe 4: Weg ohne GitHub-Konto (nur Mail)

Geprüft (Stand Okt. 2026): mailto-Link, Formular-Dienste (Tally, Formspree, Web3Forms, Netlify Forms), eigene Brücke (Cloudflare Worker → GitHub-Issue), Mail direkt, GitHub-Konto.

Die Recherche hatte Tally empfohlen (Gratis-Stufe, Dateianhang bis 10 MB, belgischer Anbieter). **Dennis hat Tally abgelehnt; es wird kein Formular-Dienst genutzt.** Es gibt deshalb keine Formular-Adresse und keine Konstante dafür in der App.

- **Weg ohne Konto: Mail** an `info@dennis-guse.de` per `mailto:` mit Betreff und vorbereitetem Text (unter 2000 Zeichen kodiert). Der Knopf im Fenster heißt „Per E-Mail senden“.
- Anhänge gehen über `mailto:` nicht. Das Fenster nennt den Ordner mit Bild oder Protokoll (`bild.png`, `rueckmeldung.txt`) und öffnet ihn; der Tester hängt die Dateien in der Mail selbst an.
- Die Adresse steht zusätzlich als Text mit „Kopieren“-Knopf (wer nur Webmail hat, bekommt oft keinen Entwurf).
- Für Leute mit Konto bleibt der vorausgefüllte GitHub-Link der Hauptweg.
- Nicht genutzt: Tally (von Dennis abgelehnt), Formspree und andere Formular-Dienste, eigene Brücke (Schlüssel, Spam, Pflege, öffentliche Issues mit persönlichen Angaben), Google Forms.
- Datenschutzhinweis (README und `docs/so-meldest-du-etwas.md`), nur sachliche Angaben: wer die Meldung bekommt (GitHub-Issue: öffentlich sichtbar; Mail: nur Dennis), wofür (Fehler nachstellen, Wünsche sortieren), Löschung auf Wunsch, Mailadresse freiwillig. Verantwortliche Stelle: **Dennis Guse, Fischbeker Str. 2a, 23869 Elmenhorst**. Es wird kein Rechtstext erfunden. Das ist keine Rechtsberatung; dieser Hinweis steht nur hier in der Spec, nicht im Anwendertext.
- Die Adresse `info@dennis-guse.de` darf öffentlich in App und README stehen (von Dennis erlaubt).

### Etappe 5: Wünsche gewichten und Übersicht

- 👍 unter dem Issue: braucht nichts. Das Formular sagt es im Text.
- Öffentliche Übersicht als GitHub-Projekttafel (Spalten **Geplant**, **In Arbeit**, **Fertig**), Issues kommen automatisch hinein, wenn das Repo-Projekt „Neue Issues automatisch hinzufügen“ hat. Die Tafel darf der Umsetzer bzw. Controller mit `gh` anlegen. Dafür braucht der Token das Recht `project`: Dennis führt vorher `gh auth refresh -s project` aus (Schritt für Dennis, steht so in der Anleitung). Der Umsetzer liefert die Schritt-für-Schritt-Anleitung (`docs/projekttafel-einrichten.md`) und den Link im README.

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
- Etappe 1 (GitHub-Vorlagen und Anleitung) braucht keine Programmänderung und darf vorab nach `master` gemergt und gepusht werden (Push nur durch den Controller nach der Prüfung), damit GitHub die Formulare zeigt.
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

## Entschieden am 2026-10-26

1. **Weg ohne Konto:** Tally ist abgelehnt, es gibt keinen Formular-Dienst. Etappe 4 ist nur der Mail-Weg (`mailto:` an `info@dennis-guse.de`, Knopf „Per E-Mail senden“).
2. **Standardkürzel** Strg+Umschalt+R ist bestätigt.
3. **Labels und Projekttafel:** Labels darf der Umsetzer/Controller anlegen (Skript `tools/labels.ps1`). Die Projekttafel darf er mit `gh` anlegen; vorher führt Dennis `gh auth refresh -s project` aus.
4. **Mailadresse** `info@dennis-guse.de` darf öffentlich in App und README stehen.
5. **Verantwortliche Stelle** im Datenschutztext: Dennis Guse, Fischbeker Str. 2a, 23869 Elmenhorst. Kein erfundener Rechtstext, nur sachliche Angaben; „keine Rechtsberatung“ nur in dieser Spec.
6. **Frage:** kein eigenes Formular; `wunsch.yml` mit Titel-Vorsilbe „Frage: …“. Das Label `frage` setzt Dennis.
7. **Lange Beschreibung:** In der Adresse gekürzt, mit Hinweis; der volle Text steht in `rueckmeldung.txt` im geöffneten Ordner (Festlegung 3).
8. **Zusätzliche Formularfelder** `protokoll` (`fehler.yml`) und `angaben` (`wunsch.yml`), jeweils freiwillig, sind bestätigt.
9. **Etappe 1** (Aufgabe 1) darf vorab nach `master` gemergt und gepusht werden, damit GitHub die Formulare zeigt. Gepusht wird nur durch den Controller nach der Prüfung.

Zur GitHub-CI (nur vor einem Release, nicht vor dem Merge; Festlegung 9) gibt es keine neue Antwort; es gilt weiter Dennis' Regel vom 2026-10-25.

Vom Baumeister-Check geklärt: Das Repo ist öffentlich (seit der Veröffentlichung), Issues und Projekttafel tragen also. Gemeint sind `crates/planora-app/src/undo_labels.rs` und `docs/superpowers/offene-punkte.md` (nicht `docs/offene-punkte.md`).
## Ausführung (Empfehlung)

Subagenten-gestützt in der Reihenfolge der Etappen, ein Umsetzer pro Etappe mit unabhängiger Prüfung (wie bei der Update-Funktion). Etappe 1 ist klein und ohne Code; dafür genügt ein Umsetzer mit Prüfung am Ende. Etappen 2 und 3 teilen sich `diag` und `build_info()` und sollten nacheinander laufen, nicht parallel. Einsatzreihenfolge: 1 → 2 → 3 → (erstes Dev-Release) → 4 → 5 → 6.
