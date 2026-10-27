# Installer für PlanoraCAD – Entwurf

Stand: 2026-10-27. Wunsch von Dennis: „Das Release sollte mit einem Installationsmanager ausgerüstet sein, wie jedes andere Windows-Programm auch.“ Mit „Meine Entscheidung“ markierte Stellen sind Vorschläge von Claude zum Prüfen.

## Ziele

- Ein normaler Windows-Installer `planoracad-setup-x64.exe` am Release, Deutsch und Englisch (nach Windows-Sprache).
- Beim Installieren wählt der Benutzer: **nur für mich** (ohne Admin, nach `%LOCALAPPDATA%\Programs\PlanoraCAD`) oder **für alle Benutzer** (mit Admin, nach `C:\Program Files\PlanoraCAD`).
- Startmenü-Eintrag, Desktop-Verknüpfung (Häkchen, standardmäßig an), Eintrag in „Apps & Features“ mit Name, Version und Herausgeber „Dennis Guse“.
- Deinstallieren über die Windows-Einstellungen; Einstellungen, Protokolle und Projekte des Benutzers bleiben liegen.
- Updates aus dem Programm funktionieren in beiden Modi weiter.

## Keine Ziele

- Keine Signatur (SmartScreen-Warnung bleibt, wie bisher dokumentiert).
- Keine Einzel-exe zum Herunterladen mehr (Dennis hat sie nicht gewählt).
- Kein MSI, keine Dateizuordnung für `.pcad` (ist ein Ordner), kein Store.

## Festlegungen

1. **Werkzeug: Inno Setup 6** (freie, permissive Lizenz; nur Bau-Werkzeug, ändert nichts an GPL-3.0-only; auf GitHub `windows-latest` vorinstalliert). Ein Skript `installer/planoracad.iss`. Benutzerwahl über `PrivilegesRequired=lowest` + `PrivilegesRequiredOverridesAllowed=dialog`; `{autopf}`, `{autoprograms}`, `{autodesktop}` passen die Ziele an. Feste `AppId` (GUID), `AppVersion` aus `Cargo.toml`. Lizenz (GPL-3.0) wird im Installer angezeigt, `THIRD-PARTY-LICENSES.txt` mit installiert.
2. **Modus merken:** Der Installer schreibt `Software\PlanoraCAD\InstallMode` = `user` oder `all` (unter HKCU bzw. HKLM). Die App liest ihn beim Start; fehlt er (Programm aus dem Bauordner gestartet), gilt sie als „nicht installiert“.
3. **Updates (Meine Entscheidung):** Die Update-Funktion lädt künftig `planoracad-setup-x64.exe` (Prüfsumme wie bisher), fragt wie bisher nach ungespeicherten Änderungen, startet den Installer still im gleichen Modus (`/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /RESTARTAPPLICATIONS /CURRENTUSER` bzw. `/ALLUSERS`, per ShellExecute, damit Windows bei „alle Benutzer“ selbst nach Admin fragt) und beendet sich. Inno startet PlanoraCAD danach neu. Lehnt der Benutzer die Admin-Abfrage ab, läuft die alte Version weiter mit Hinweis „Update abgebrochen“.
4. **Rückfall (Meine Entscheidung):** Der bisherige Austausch mit `.old`-Datei und Rückfall entfällt für installierte Fassungen; Inno ersetzt die Dateien erst am Ende und lässt bei einem Fehler die alte Version stehen. Die neue Version meldet sich wie bisher mit „Aktualisiert auf …“. Läuft das Programm nicht installiert (Entwicklung), bietet die Update-Meldung nur „Download-Seite öffnen“.
5. **Release-Dateien:** `planoracad-setup-x64.exe`, `planoracad-setup-x64.sha256`, `planoracad-windows-x64.pdb` (Symbole, nie geladen), `THIRD-PARTY-LICENSES.txt`. Die Update-Funktion akzeptiert nur noch den exakten Setup-Namen (strenge URL-Prüfung bleibt).
6. **Daten:** Die App speichert weiter in `%APPDATA%\PlanoraCAD` und `Dokumente\PlanoraCAD`; der Deinstaller fasst beides nicht an.
7. **Texte** in einfacher Sprache, Du-Form im Programm; die Installer-Texte kommen aus den Sprachdateien von Inno.

## Prüfen

- Lokal: Installer mit Inno bauen (Inno Setup auf Dennis' Rechner nötig – Installation nur mit seiner Erlaubnis), Skript-Test, dass Version, Dateien und Registry-Wert stimmen.
- Echtes Installieren/Aktualisieren/Deinstallieren in beiden Modi braucht einen sauberen Windows-Benutzer: **Windows-Sandbox** (falls aktivierbar) oder Dennis' Rechner mit anschließendem Deinstallieren – nur mit seinem Go.
- GitHub: Trockenlauf von `release.yml` baut den Installer vor dem ersten Tag.

## Entschieden am 2026-10-27

1. Inno Setup 6 darf auf Dennis' Rechner installiert werden (erledigt, 6.7.3, `%LOCALAPPDATA%ProgramsInno Setup 6ISCC.exe`).
2. Echter Installationstest auf Dennis' Rechner in beiden Modi, danach wieder deinstallieren.
3. Entscheidungen 3 und 4 (stiller Installer, kein `.old`-Rückfall) gelten als angenommen (Dennis hat nicht widersprochen; bei Einwand ändern).
