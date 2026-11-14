# Installer im Graphit-Stil – Entwurf

Stand: 2026-11-14. Wunsch von Dennis: Der Installer soll „ein komplettes Design, angelehnt an das Graphit-Designsystem“ bekommen, „eventuell auch passende Screenshots als Teaser für den Benutzer“. Er soll so vorbereitet sein, dass die Session PlanoraCAD ihn beim Bauen eines Releases einfach nutzen kann. Mit **(Claude)** markierte Stellen hat Claude festgelegt, sie können geändert werden.

## Warum

Der Tester und spätere Benutzer sehen den Installer als Erstes. Heute ist das ein normales, helles Inno-Setup-Fenster, das nicht zu PlanoraCAD passt. Der neue Installer soll aussehen wie die App selbst und beim Warten zeigen, was das Programm kann.

## Ziele

- Ein Installer-Fenster, das genau wie **Entwurf C** aussieht (Vorschau: `2026-11-14-graphit-installer-vorschau/installer-richtungen-v2.html`, dritte Richtung): links ein schwebendes Feld „Installation“ mit allen Schritten wie im Feature-Baum, rechts ein schwebendes Feld mit dem Inhalt. Graphit-Farben, Montserrat, Phosphor-Symbole wie in der App.
- Beim Installieren zeigen drei Karten mit Screenshots, was PlanoraCAD kann.
- Alles, was der heutige Installer kann, bleibt gleich: nur für mich oder für alle Benutzer, Ordner wählbar, Desktop-Verknüpfung, Startmenü, Eintrag in „Apps“, Deinstallieren über Windows, Benutzerdaten bleiben erhalten.
- Die stillen Updates aus der App laufen unverändert weiter. Dateiname (`planoracad-setup-x64.exe`), Prüfsumme und Aufrufparameter bleiben gleich.
- Der Bau bleibt **ein Befehl**: `tools/build-installer.ps1` mit denselben Parametern wie heute. `release.yml` braucht höchstens kleine Ergänzungen.

## Keine Ziele

- Kein eigenes Deinstallations-Fenster im Graphit-Stil. Das Deinstallieren bleibt bei Inno, bekommt aber dunkle Graphit-Farben (siehe unten).
- Keine Signatur, kein MSI, kein ARM, keine Dateizuordnung.
- Keine Änderung am Update-Ablauf der App (`planora-update`, `update_install.rs`).

## Was auf keinen Fall passieren darf

- Updates beim Tester gehen kaputt oder starten PlanoraCAD zweimal.
- Ein schwarzes Konsolenfenster blitzt auf.
- Eine neue Installation landet woanders als eine alte (andere AppId, anderer Ordner, andere Registry-Werte), sodass Windows zwei PlanoraCAD-Einträge zeigt.
- Die App selbst sieht danach anders aus.

## Wege (besprochen am 2026-11-14)

1. **Nur Inno Setup, so nah an C wie möglich:** Felder als Hintergrundbild, Windows-Bauteile dunkel eingefärbt. Kein Risiko für Updates, aber nur etwa 80 % wie der Entwurf (Knöpfe, Häkchen, Balken bleiben Windows-Bauteile).
2. **Eigenes Fenster in PlanoraCAD-Technik, Inno arbeitet unsichtbar dahinter.** Sieht genau aus wie der Entwurf, die getestete Inno-Technik bleibt. Mittlerer Aufwand. **Von Dennis gewählt.**
3. **Komplett eigener Installer ohne Inno:** sieht nicht besser aus als 2, aber Installieren, Deinstallieren, Rechte und Updates müssten neu gebaut werden. Verworfen.

## Aufbau

### Zwei Teile in einer Datei

- **Motor:** das bisherige Inno-Skript `installer/planoracad.iss`, gebaut zu `planoracad-engine-x64.exe` **(Claude)**. AppId, Ordner, Registry-Werte, `[Run]`-Einträge und `/NACHUPDATE` bleiben **unverändert**. Neu sind nur:
  - ein Parameter `/PROGRESSFILE=<Pfad>`: Der Motor schreibt dort in `CurInstallProgressChanged` den Fortschritt in Prozent hinein und in `CurStepChanged` den aktuellen Schritt (z. B. `step=install`, `step=done`). Fehlt der Parameter, passiert nichts (stille Updates bleiben genau wie heute).
  - Graphit-Farben für das Deinstallieren: `WizardStyle=modern dynamic`, `WizardBackColorDynamicDark` = Graphit-Feld `#181818`, hell = `#ffffff` **(Claude)**. Dafür nötig ist Inno Setup ≥ 6.7. In `release.yml` ist das 6.7.3 und auf Dennis' Rechner auch.
- **Hülle:** ein neues Rust-Programm `planora-setup` (neues Crate, egui/eframe in derselben Version wie die App, Fenster ohne Konsole über `windows_subsystem = "windows"`). Es enthält den Motor, die Schriften, die Symbole und die Screenshots fest eingebaut (`include_bytes!`, Pfad des Motors über eine Umgebungsvariable, die das Bauskript setzt) **(Claude)**. Das Ergebnis heißt wie bisher `planoracad-setup-x64.exe`.

### Ablauf der Hülle

1. **Start mit Inno-Parametern** (`/SILENT`, `/VERYSILENT`, `/NACHUPDATE=…`, `/CURRENTUSER`, `/ALLUSERS`, `/LOG…` und alle anderen, die mit `/` beginnen): **kein Fenster**. Die Hülle legt den Motor in einen eigenen Temp-Ordner, startet ihn mit **genau denselben Parametern**, wartet, löscht den Temp-Ordner und gibt den Rückgabewert des Motors zurück. So bleibt der stille Update-Ablauf aus `update_install.rs` unverändert. Bei `/ALLUSERS` muss Windows wie heute nach Admin fragen. Die Hülle startet den Motor deshalb so, dass der Motor sich selbst erhöhen kann (ShellExecute statt CreateProcess, falls nötig) **(Claude, beim Bau prüfen)**.
2. **Start ohne Parameter** (Doppelklick): Das Graphit-Fenster öffnet sich mit fünf Schritten.
3. **Rückfall, wenn das Graphit-Fenster nicht aufgeht** (kein passender Grafiktreiber, VM, Remotedesktop): Die Hülle startet den Motor ohne Parameter, also den normalen Inno-Assistenten mit dunklen Graphit-Farben. So kann der Tester immer installieren. Der Motor behält dafür alle seine Assistenten-Seiten, dafür gibt es einen eigenen Test **(Claude)**.

Die Hülle braucht ein Manifest mit `asInvoker`. Sonst fragt Windows bei einer Datei mit „setup“ im Namen von sich aus nach Admin-Rechten. Fehlt beim Bauen der Motor (Umgebungsvariable nicht gesetzt, z. B. normaler Workspace-Build oder Linux), baut die Hülle trotzdem und meldet beim Start „Diese Fassung enthält keinen Installer“.

### Die fünf Schritte (Entwurf C)

Linkes Feld „Installation“: Willkommen, Lizenz, Ordner, Installieren, Fertig. Erledigt = grüner Haken, aktuell = hervorgehoben wie die Auswahl im Feature-Baum, offen = gedämpft. Rechtes Feld:

1. **Willkommen:** PlanoraCAD-Symbol mit Schriftzug, „Willkommen“, ein Satz („Dieses Programm installiert PlanoraCAD <Version> auf deinem Computer.“), großer Screenshot der Startseite. Knöpfe: Abbrechen, Weiter.
   - Ist PlanoraCAD schon installiert (Registry `Software\PlanoraCAD\InstallPath`/`InstallMode` unter HKCU oder HKLM), steht dort „… aktualisiert PlanoraCAD auf <Version>.“ **(Claude)**
2. **Lizenz:** die GPL-3.0 (`LICENSE`) in einem scrollbaren Feld, darunter „Ich stimme der Lizenz zu“. Weiter erst nach dem Haken.
3. **Ordner:** zwei gleich hohe Karten nebeneinander: „Nur für mich“ (ohne Admin, `%LOCALAPPDATA%\Programs\PlanoraCAD`) und „Für alle Benutzer“ (fragt nach Admin, `C:\Program Files\PlanoraCAD`). Darunter ein Ordnerfeld mit „Durchsuchen“ und der Haken „Verknüpfung auf dem Desktop“ (an). Bei einem Update sind Modus und Ordner vorbelegt und gesperrt, damit kein zweiter Eintrag entsteht **(Claude)**. Knopf: Installieren.
   - **PlanoraCAD läuft noch:** Beim Klick auf Installieren prüft die Hülle, ob PlanoraCAD aus dem Zielordner läuft. Dann erscheint der Hinweis „PlanoraCAD ist noch geöffnet. Speichere deine Arbeit und schließe das Programm.“ mit „Erneut prüfen“. Die Hülle schließt die App nie selbst, damit keine ungespeicherte Arbeit verloren geht. `/CLOSEAPPLICATIONS` bleibt nur als Sicherheitsnetz **(Claude)**.
4. **Installieren:** Fortschrittsbalken in Akzentblau, darunter der Schritt in Worten und die Prozentzahl. Darunter drei Karten mit Screenshots und Unterschrift: „Skizzen zeichnen“, „Zeichnungen ableiten“, „Schnitte zeigen“ **(Claude)**. Abbrechen ist hier gesperrt. Der Motor läuft mit `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS`, `/CURRENTUSER` oder `/ALLUSERS`, `/DIR=…`, `/TASKS=desktopicon` oder `/MERGETASKS=!desktopicon`, `/LANG=de|en`, `/PROGRESSFILE=…` und `/LOG=…` in den Temp-Ordner.
   - Lehnt der Benutzer die Admin-Abfrage ab: zurück zu Schritt 3 mit dem Hinweis „Für alle Benutzer braucht Windows deine Erlaubnis. Wähle ‚Nur für mich‘ oder versuche es noch einmal.“
   - Anderer Fehler (Rückgabewert ≠ 0): Fehlerseite in schlichten Worten, mit Knopf „Protokoll öffnen“ (Log aus dem Temp-Ordner, der dafür erhalten bleibt) und „Schließen“.
5. **Fertig:** „PlanoraCAD ist installiert.“, Haken „PlanoraCAD jetzt starten“ (an), Knopf „Fertig“. Die Hülle läuft ohne Admin, startet die App also mit normalen Rechten.

### Aussehen

- Farben aus der App-Palette (Graphit Dunkel und Hell). Die Hülle folgt der Windows-Einstellung Hell/Dunkel **(Claude)**. Die Screenshots sind immer dunkel.
- Damit die Farben nicht doppelt gepflegt werden, wandern `Palette`, `DARK`, `LIGHT` und die Schriften-Einrichtung aus `planora-app/src/theme.rs` in ein kleines gemeinsames Crate `planora-style`. `planora-app` nutzt es weiter unverändert (reine Verschiebung, Aussehen der App bleibt gleich) **(Claude)**. Achtung: Andere Zweige ändern `theme.rs` vielleicht auch. Die Verschiebung klein halten und beim Merge prüfen.
- Fenster: feste Größe 960 × 640 (logische Punkte), nicht veränderbar, mittig, Windows-Titelleiste im dunklen Modus, Programmsymbol `planoracad.ico` **(Claude)**. Die Felder schweben mit Abstand, Rundung und Rand wie die Dialoge der App.
- Sprache nach Windows-Sprache: Deutsch, sonst Englisch. Texte über `tr!` aus `.ftl`-Dateien in de und en, wie in der App **(Claude)**.
- Screenshots: aus `D:\Vault\Vault\Projekte\PlanoraCAD\Wiki\bilder\` (`startseite-uebersicht`, `skizze-zeichnen`, `zeichnung-uebersicht`, `schnittansicht`), passend verkleinert nach `assets/setup/` im Repo. Vorerst dieselben (deutschen) Bilder für beide Sprachen **(Claude, offen)**.

### Bauen

`tools/build-installer.ps1` macht mit denselben Parametern wie heute:

1. Inno baut den Motor `planoracad-engine-x64.exe` in einen Zwischenordner (nicht nach `dist`). Den Ausgabenamen setzt das Bauskript beim ISCC-Aufruf (`/F`), das Skript `planoracad.iss` bleibt bei `OutputBaseFilename=planoracad-setup-x64`, damit `installer_script.rs` grün bleibt.
2. `cargo build --release --locked -p planora-setup` mit der Umgebungsvariable, die auf den Motor zeigt.
3. Ergebnis als `dist/planoracad-setup-x64.exe` plus `.sha256` wie bisher.

`release.yml` ruft das Skript schon auf. Heute installiert der Lauf Inno 6.7.3 nur, wenn noch gar kein Inno da ist. Künftig muss er mindestens 6.7 sicherstellen, sonst bricht der Bau ab. Das Bauskript prüft die Inno-Version und bricht mit einer klaren Meldung ab. In `CLAUDE.md` (Abschnitt Prüfen und Abläufe) kommt ein Satz dazu, damit die Session PlanoraCAD beim Release weiß: „Installer: `tools/build-installer.ps1`, baut Motor und Hülle.“

### Lizenzen

Keine neuen fremden Abhängigkeiten über die der App hinaus (egui, eframe, Montserrat, Phosphor sind schon geprüft). Falls doch etwas dazukommt, z. B. für die Windows-Registry oder ShellExecute: `cargo deny check` muss sauber sein.

## Prüfen

- **Automatisch:** Tests für die Weitergabe der Parameter (mit `/` → still und unverändert weiter), die Zusammenstellung der Motor-Parameter je Auswahl, die Sprachwahl, das Lesen der Fortschrittsdatei, das Erkennen einer vorhandenen Installation, und ein Skript-Test, dass `planoracad.iss` `PROGRESSFILE` kennt und AppId und `NACHUPDATE` unverändert sind. Die bestehenden Tests (`install_mode.rs`, `installer_script.rs`) bleiben grün.
- **Wie immer:** fmt, clippy `-D warnings`, alle Tests, `cargo deny check`, Windows und WSL. Unter Linux wird `planora-setup` nur gebaut und getestet, soweit es ohne Windows geht.
- **Sichtprüfung und echter Test (nur mit Go von Dennis):** Installer bei 1920 × 1080 durchklicken (beide Modi, Hell und Dunkel), danach ein stilles Update mit `/VERYSILENT /CURRENTUSER /NACHUPDATE=1` testen und wieder deinstallieren. Dennis sieht sich die Schritte als Screenshots an.

## Dem Bauenden überlassen

- Wie die Hülle den Fortschritt abfragt (Datei alle 100 ms lesen genügt).
- Wie Ordner-Auswahl und Windows-Abfragen technisch umgesetzt werden (vorhandene Abhängigkeiten der App bevorzugen).
- Genaue Abstände und Größen innerhalb der Felder, solange es wie Entwurf C aussieht.
