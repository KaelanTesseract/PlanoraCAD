<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/planoracad-logo-fuer-dunkel.png">
    <img src="assets/logo/planoracad-logo-fuer-hell.png" alt="PlanoraCAD" width="280">
  </picture>
</p>

# PlanoraCAD

PlanoraCAD ist ein kostenloses, quelloffenes Programm, mit dem du Bauteile am Computer in 3D konstruierst und technische Zeichnungen davon erstellst.

> **Stand:** PlanoraCAD ist noch in einer frühen Entwicklung. Die erste Version zum Ausprobieren ist die Entwicklerversion `0.0.2-dev.1`, nur für Windows (64 Bit). Sie ist noch nicht für den Alltag gedacht. Rechne mit Fehlern und sichere wichtige Projekte zusätzlich.

## Was du damit machen kannst

Du zeichnest zuerst eine flache Skizze mit Maßen und machst daraus einen Körper. Jeder Schritt wird festgehalten. Änderst du später ein Maß, rechnet das Programm alles, was darauf aufbaut, neu.

- **Skizzen mit Maßen:** Linien, Rechtecke, Kreise, Bögen, Ellipsen und Kurven zeichnen. Mit Maßen und Regeln wie „waagerecht“, „parallel“ oder „gleich lang“ legst du fest, wie die Skizze aussieht.
- **Körper:** Eine Skizze herausziehen (extrudieren) oder um eine Achse drehen. Dazu gibt es Quader, Zylinder, Kugel und Kegel.
- **Körper kombinieren:** zwei Körper vereinigen, einen vom anderen abziehen oder nur die Überschneidung behalten.
- **Rundungen und Fasen:** Kanten runden oder abschrägen.
- **Parameter mit Formeln:** Maße bekommen einen Namen, zum Beispiel „Breite = 40“, und andere Maße rechnen damit, etwa „Breite / 4“. Änderst du die Breite, passt sich alles an.
- **Technische Zeichnungen:** Ansichten von vorne, oben, von der Seite und schräg, Schnittansichten, Maße, Mittellinien, Schriftfeld, Stückliste und Positionsnummern. Ausgabe als PDF, SVG oder DXF.
- **Austausch mit anderen Programmen:** STEP-Dateien (das übliche Format zwischen CAD-Programmen) lesen und schreiben. Für den 3D-Druck STL und 3MF schreiben, dazu OBJ und glTF. STL- und OBJ-Dateien einlesen.
- **Projekte und Versionen:** Deine Arbeit liegt in Projekten und Ordnern. Jedes Speichern legt eine neue Version an, ältere bleiben erhalten. Varianten legst du als eigenen Zweig an.
- **Verlaufsleiste:** Unten im Fenster siehst du die Schritte eines Körpers und kannst zu einem früheren Schritt zurückgehen, ohne etwas zu löschen.
- **Rückgängig und Wiederholen** für jede Änderung am Dokument.
- **Tastenkürzel:** Die Bedienung ist an gängige CAD-Programme angelehnt. Jedes Kürzel und die Mausbelegung lassen sich in den Einstellungen ändern.
- **Hell oder dunkel:** Du wählst das Aussehen selbst.

Alles mit Einzelheiten, Bedienung und Grenzen steht unter [Funktionen im Detail](docs/funktionen.md).

## Was du davon hast

- **Kostenlos und offen:** Der Quelltext steht hier im Repository. Jeder darf ihn ansehen, verändern und weitergeben (Lizenz GPL-3.0).
- **Deine Dateien bleiben bei dir:** Du brauchst kein Konto und keine Cloud. Projekte liegen als ganz normale Ordner auf deinem Rechner. Zum Sichern kopierst du sie einfach.
- **Nichts geht verloren:** Frühere Versionen bleiben beim Speichern erhalten. Alle zwei Minuten sichert das Programm geänderte, schon gespeicherte Dokumente zusätzlich. Nach einem Absturz bietet es diesen Stand beim nächsten Öffnen an.
- **Maße bleiben änderbar:** Weil jeder Schritt gespeichert ist, kannst du ein Bauteil auch später noch anpassen, statt es neu zu zeichnen.

## Herunterladen und starten

1. Öffne die Seite mit den [Veröffentlichungen](https://github.com/KaelanTesseract/PlanoraCAD/releases).
2. Such die neueste Version. Klick darunter im Bereich „Assets“ auf `planoracad-setup-x64.exe`. Die Datei wird heruntergeladen.
3. Doppelklick auf `planoracad-setup-x64.exe`.
4. Windows zeigt wahrscheinlich ein blaues Fenster „Der Computer wurde durch Windows geschützt“. Klick dort auf „Weitere Informationen“ und dann auf „Trotzdem ausführen“. Die Warnung kommt, weil die Datei nicht digital signiert ist. Lade das Programm deshalb nur von der Seite der Veröffentlichungen hier im Repository.
5. Zuerst fragt der Installer, für wen das Programm da sein soll: „Nur für mich“ (ohne Administratorrechte) oder „Für alle Benutzer“ (Windows fragt dann nach Administratorrechten).
6. Auf der Lizenzseite („Lesen Sie bitte die folgenden Lizenzvereinbarungen …“) liest du die Lizenz, wählst „Ich akzeptiere die Vereinbarung“ und klickst auf „Weiter“.
7. Setz bei Bedarf das Häkchen für die Verknüpfung auf dem Desktop.
8. Klick dich bis „Fertigstellen“ durch. Das Programm steht danach im Startmenü und startet auf Wunsch gleich.

Hält dein Virenprogramm die Datei fest oder startet etwas nicht, schau unter [Häufige Probleme](docs/haeufige-probleme.md).

**Deinstallieren**

1. Öffne die Windows-Einstellungen und geh auf „Apps“.
2. Such „PlanoraCAD“ in der Liste und wähle „Deinstallieren“.
3. Deine Projekte, Einstellungen und Protokolle bleiben dabei erhalten (siehe unten).

**Wo deine Daten liegen**

- **Projekte:** im Ordner `Dokumente\PlanoraCAD`. Einen anderen Ordner wählst du unter „Datei → Einstellungen …“ mit „Projektverzeichnis ändern …“.
- **Einstellungen, Protokoll, Absturzberichte und Rückmeldungen:** im Ordner `%APPDATA%\PlanoraCAD`. Du erreichst ihn, wenn du `%APPDATA%\PlanoraCAD` oben in die Adresszeile des Explorers tippst.

Brauchst du die Ordner oben nicht mehr, kannst du sie nach dem Deinstallieren selbst löschen.

## Sprache

PlanoraCAD gibt es auf Deutsch und Englisch. Beim ersten Start nimmt das Programm die Sprache von Windows, sonst Englisch. Umstellen kannst du sie unter „Datei → Einstellungen …“, Abschnitt „Darstellung“, Auswahl „Sprache“.

Du möchtest PlanoraCAD in eine andere Sprache übersetzen? Die Anleitung (auf Englisch) steht in [docs/uebersetzen.md](docs/uebersetzen.md).

## Updates

- Beim Start schaut das Programm höchstens einmal am Tag nach, ob es eine neuere Version gibt. Zurzeit gibt es nur Entwicklerversionen (Kanal „Dev“).
- Gibt es eine, zeigt es ein Fenster mit den Änderungen. Du wählst „Jetzt aktualisieren“, „Später“ oder „Diese Version überspringen“.
- Bei „Jetzt aktualisieren“ lädt das Programm den neuen Installer und prüft ihn. Danach fragt es, ob du ungespeicherte Arbeit speichern willst, und startet erst dann den Installer im Hintergrund. Bei „Für alle Benutzer“ fragt Windows nach Administratorrechten.
- Lehnst du das ab oder klappt etwas nicht, läuft die bisherige Version weiter. Hilfe steht unter [Häufige Probleme](docs/haeufige-probleme.md).
- Abschalten oder von Hand suchen: „Datei → Einstellungen …“, Abschnitt „Updates“.
- Was sich in jeder Version geändert hat, steht in [CHANGELOG.md](CHANGELOG.md). Einzelheiten zu den Updates: [Updates und Versionen](docs/funktionen.md#updates-und-versionen).

## Fehler melden und Wünsche äußern

Du hast einen Fehler gefunden, dir fehlt etwas, oder du hast eine Frage? Dann sag es bitte. Zwei Sätze reichen.

1. Wähl im Programm „Hilfe → Rückmeldung …“ (oder drück Strg+Umschalt+R).
2. Wähl, ob es ein Fehler, ein Wunsch oder eine Frage ist, und schreib kurz, was passiert ist. Mit Häkchen entscheidest du, welche Angaben dazukommen.
3. Klick auf „Im Browser öffnen“, wenn du ein GitHub-Konto hast. Ohne Konto klick auf „Ohne GitHub-Konto senden“; dann öffnet sich ein Formular auf dennis-guse.de.
4. Prüf im Browser die Angaben und schick das Formular ab.

Du kannst auch direkt auf GitHub eine [neue Meldung](https://github.com/KaelanTesseract/PlanoraCAD/issues/new/choose) anlegen. Geht beides nicht, zum Beispiel weil sich kein Browser öffnet, schreib eine Mail an info@dennis-guse.de.

- Schritt für Schritt, mit allen Wegen: [So meldest du etwas](docs/so-meldest-du-etwas.md)
- Lösungen für bekannte Fälle: [Häufige Probleme](docs/haeufige-probleme.md)
- Welche Wünsche geplant sind und woran gerade gearbeitet wird: [Was geplant ist](https://github.com/users/KaelanTesseract/projects/1)

Meldungen auf GitHub sind öffentlich. Schreib deshalb nichts Privates hinein.

## Was noch nicht geht

Die wichtigsten Grenzen:

- Eine fertige Programmdatei gibt es nur für Windows (64 Bit).
- Baugruppen gibt es noch nicht. Aus STEP-Baugruppen wird jeder Körper einzeln übernommen.
- Kein Loft und kein Sweep (Formen zwischen zwei Profilen oder entlang einer Bahn). Muster und Spiegeln gibt es nur innerhalb einer Skizze, nicht als Schritt am Körper.
- Drehen geht nur um volle 360 Grad.
- Rundungen, Fasen und das Kombinieren von Körpern lehnt das Programm in manchen Fällen mit einer Meldung ab. Es liefert lieber kein Ergebnis als ein falsches.
- Aufwendige Rechnungen, vor allem mit Freiformflächen, können einige Sekunden dauern. In dieser Zeit reagiert das Fenster nicht.
- Beim Austausch mit anderen Programmen gehen Farben und Materialien nicht mit.
- Zweige lassen sich nicht wieder zusammenführen.
- Neue Programmversionen speichern teils in einem neueren Dateiformat. Ältere Versionen öffnen solche Dateien dann nicht („zu neu“).

Die vollständigen Grenzen stehen bei jedem Abschnitt unter [Funktionen im Detail](docs/funktionen.md). Noch ausführlicher, mit Messwerten: [offene Punkte](docs/superpowers/offene-punkte.md).

## Datenschutz

- **Updates:** Das Programm fragt bei GitHub (`api.github.com`) nach neuen Versionen, höchstens einmal am Tag und nur, solange die Suche eingeschaltet ist. Zum Herunterladen verbindet es sich mit GitHub. Es schickt dabei keine Projekte, keine Namen und keine Kennung deines Rechners. GitHub sieht wie bei jeder Verbindung deine Internetadresse sowie Name und Version des Programms. Schaltest du die Suche ab, verbindet sich das Programm nie von selbst.
- **Rückmeldungen:** Das Programm schickt nichts von allein. Was mitgehen soll, siehst du vorher und kannst es abwählen. Benutzername, Projektpfade und Mailadressen entfernt das Programm vorher. Die Angaben stehen in der Adresse des Formulars und gehen deshalb schon beim Öffnen im Browser an GitHub oder an dennis-guse.de (sie können dort im Protokoll des Servers stehen). Gespeichert und gemeldet wird erst, wenn du im Formular auf „Absenden“ klickst.
- **Absturzberichte:** Stürzt das Programm ab, schreibt es einen Bericht auf deinen Rechner, ohne deinen Benutzernamen. Beim nächsten Start fragt es, ob du ihn schicken willst. Ohne dein Zutun verlässt er den Rechner nicht. Alte Berichte löscht das Programm nach 30 Tagen.
- **Protokoll:** Die Protokolldatei bleibt auf deinem Rechner. Projektinhalte stehen nicht darin.

Verantwortlich: Dennis Guse, Fischbeker Str. 2a, 23869 Elmenhorst. Kontakt: info@dennis-guse.de.

## Für Entwickler

PlanoraCAD ist in Rust geschrieben, mit einem eigenen Geometrie-Kern.

### Bauen

    cargo run -p planora-app --release

Den Installer baust du mit `tools\build-installer.ps1` (nach `cargo build --release -p planora-app`; braucht Inno Setup 6). Er landet im Ordner `dist`.

Unter Linux werden zusätzlich die System-Bibliotheken aus `.github/workflows/ci.yml` benötigt.

### Entwicklung

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check

### Release und Absturzberichte

- Eine neue Version entsteht mit `tools\release.ps1`, einem Abschnitt in `CHANGELOG.md`, einem grünen Lauf von `ci.yml` und einem Tag. Der Tag startet `.github/workflows/release.yml`; das baut die Windows-Version samt Installer und legt das Release an (Installer `planoracad-setup-x64.exe`, Prüfsumme, `THIRD-PARTY-LICENSES.txt` und die Symboldatei `.pdb`).
- Absturzberichte von Testern löst `tools\symbolize.ps1` mit der `.pdb` des Releases auf.
- Alle Schritte: [Updates und Versionen](docs/funktionen.md#updates-und-versionen). Rückmeldungen und Protokoll: [Rückmeldung, Absturzberichte und Protokoll](docs/funktionen.md#rückmeldung-absturzberichte-und-protokoll).
- Die Projekttafel einrichten: [Projekttafel einrichten](docs/projekttafel-einrichten.md).

### Drittanbieter

- Schrift: Montserrat, OFL 1.1; Symbolschrift Phosphor, MIT; Ersatzschrift Roboto 2.138, Apache-2.0 (siehe `crates/planora-app/assets/fonts/`).
- PDF-Ausgabe: Die Breiten der Helvetica-Buchstaben stammen aus der Adobe-Datei „Helvetica.afm“ der „Core 14 AFM Files“ (AFM 4.1, Version 002.000). Copyright (c) 1985, 1987, 1989, 1990, 1997 Adobe Systems Incorporated. All Rights Reserved. Helvetica is a trademark of Linotype-Hell AG and/or its subsidiaries. Bedingungen der Sammlung (Datei „MustRead.html“): Die Dateien dürfen für jeden Zweck und ohne Gebühr, mit oder ohne Änderung, genutzt, kopiert und weitergegeben werden, wenn alle Urheberrechtsvermerke erhalten bleiben, die AFM-Dateien nicht ohne diesen Hinweis weitergegeben werden, Änderungen deutlich vermerkt werden und der Hinweis selbst nicht verändert wird; Adobe ist nicht zur Unterstützung verpflichtet. Im Quelltext (`crates/planora-drawing/src/pdf_font.rs`) steht nur die Breitentabelle, neu nach dem westeuropäischen Zeichensatz (WinAnsi) angeordnet (eine vermerkte Änderung); die AFM-Datei selbst ist nicht enthalten, und die Schrift wird nicht in die PDF-Dateien eingebettet.

### Lizenz

GPL-3.0-only, siehe [LICENSE](LICENSE).
