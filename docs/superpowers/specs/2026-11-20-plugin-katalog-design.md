# Plug-in-Katalog im Fenster „Zusatzmodule“

Stand 2026-11-20. Entscheidungen von Dennis aus dem Brainstorming am 2026-11-20. Was als **Vorschlag (Claude)** markiert ist, hat Claude festgelegt; Dennis hat den Entwurf mit diesen Vorschlägen freigegeben („ja spec schreiben“). Baut auf Plug-ins Etappe 1 auf (`docs/superpowers/specs/2026-11-19-plugin-schnittstelle-design.md`, Zweig `feature/plugins`).

## Warum

Plug-ins sollen nicht mehr mit dem Installer kommen, sondern aus einem festen Katalog, den Dennis pflegt. Wer PlanoraCAD installiert, sucht sich im Programm aus, was er braucht (z. B. die Elektronik), und hält es darüber aktuell. Wer nur mechanisch zeichnet, installiert nichts davon.

**Gelungen ist es, wenn**
- ein frisch installiertes PlanoraCAD beim ersten Start einmal auf die Zusatzmodule hinweist und „Katalog öffnen“ direkt den Katalog zeigt;
- man im Reiter „Katalog“ ein Plug-in sucht, „Installieren“ klickt, den bekannten Installationsdialog bestätigt und das Plug-in ohne Neustart läuft;
- ein im Katalog neueres Plug-in in beiden Reitern als „Aktualisieren auf …“ erscheint;
- der Katalog ohne Internet den letzten Stand mit dem Hinweis „offline“ zeigt.

**Fehlschlag wäre:**
- ein kaputter oder manipulierter Katalogeintrag bringt die ganze Liste zum Absturz oder führt auf eine andere Seite als GitHub;
- der Katalog lädt etwas ohne Zutun des Nutzers außer der Liste selbst;
- der Hinweis beim ersten Start kommt mehr als einmal.

## Entscheidungen von Dennis

1. **Jedes Plug-in hat ein eigenes GitHub-Repo** mit Releases wie in Etappe 1 (Tag = Version, genau eine `.pcplugin`, optional `.sha256`). **Ein Katalog-Repo** enthält nur die Liste `katalog.json`. Das Programm kennt fest nur die Adresse des Katalog-Repos. (Verworfen: alle Plug-ins in einem Repo – bräuchte Tags pro Plug-in und mehrere Pakete pro Release, also einen Umbau des Update-Wegs.)
2. **Nur Katalog:** Der Installer bringt keine Plug-ins mit, auch nicht die Elektronik. Die geplante Installer-Komponente „Elektronik“ aus Vorhaben 3 entfällt. (Verworfen: Installer und Katalog.)
3. **Beim ersten Programmstart** erscheint einmal ein Hinweis auf die Zusatzmodule mit „Katalog öffnen“ und „Später“.
4. **Nur Dennis' eigene Plug-ins** stehen im Katalog. Fremde gibt es weiter über „Aus Datei…“ oder einen eigenen Repo-Link. (Verworfen: fremde Plug-ins markiert im Katalog.)
5. **Zuständigkeit:** Die Session „PlanoraCAD Plugin-Pläne“ verwaltet die Plug-ins und legt die Repos an (Katalog-Repo und Plug-in-Repos). Diese Spec beschreibt die Programmseite; das Format von `katalog.json` ist der Vertrag zwischen beiden.

## Format `katalog.json` (Vorschlag (Claude))

Liegt als Release-Datei im **neuesten Release** des Katalog-Repos (nicht im Hauptzweig), damit nur freigegebene Stände ausgeliefert werden.

```json
{
  "format": 1,
  "eintraege": [
    {
      "id": "planora.elektronik",
      "name": { "de": "Elektronik", "en": "Electronics" },
      "beschreibung": { "de": "Bauteilbibliothek, Schaltplan und Platine.", "en": "Part library, schematic and PCB." },
      "kategorie": "elektronik",
      "repo": "https://github.com/<besitzer>/<plugin-repo>",
      "symbol": "symbol.png"
    }
  ]
}
```

- `format`: Ganzzahl; unbekannte höhere Hauptversion ⇒ Hinweis „Katalog braucht eine neuere PlanoraCAD-Version“, nichts anzeigen.
- `id`: muss zur `id` im `plugin.toml` des Plug-ins passen (geprüft beim Installieren, Etappe-1-Regel `plugin-link-foreign-id`).
- `name`, `beschreibung`: Texte je Sprache; fehlt die aktuelle Sprache ⇒ Englisch ⇒ erster vorhandener.
- `kategorie`: freier Bezeichner; das Programm kennt Übersetzungen für eine feste Liste (Vorschlag: `elektronik`, `import-export`, `werkzeuge`, `beispiele`), unbekannte erscheinen unter „Sonstiges“.
- `repo`: muss die Etappe-1-Regel für Repo-Links erfüllen (`parse_repo_link`), sonst wird der Eintrag übergangen.
- `symbol`: optional, Name einer weiteren Datei im selben Katalog-Release (nur PNG, ≤ 256 KiB; SVG wird nicht angenommen, Abschluss-Ruling); fehlt sie ⇒ Standardsymbol.
- Unbekannte Felder werden ignoriert (spätere Erweiterungen).

## Programmseite

### Fenster „Zusatzmodule“
- Zwei Reiter: **„Installiert“** (das heutige Fenster) und **„Katalog“**.
- Katalog: links Liste mit Symbol, Name, Kategorie; oben Suche (Name und Beschreibung, ohne Groß/Klein) und Filter nach Kategorie; rechts Details: Beschreibung, neueste Version aus dem Plug-in-Repo, verlangte Rechte (erst bekannt nach dem Laden des Pakets – bis dahin „wird beim Installieren angezeigt“), Quelle (Repo-Link) und Knopf **„Installieren“** bzw. „Installiert“ / „Aktualisieren auf { $version }“.
- „Installieren“ nutzt den Repo-Link-Weg aus Etappe 1 (Release holen, Prüfungen, Installationsdialog, Rechte); `herkunft.json` bekommt zusätzlich die Kennung `katalog: true` (Vorschlag (Claude)), damit „Installiert“ die Herkunft „aus dem Katalog“ anzeigen kann.

### Laden und Zwischenspeicher
- Die Liste wird geholt, wenn man den Reiter „Katalog“ öffnet (höchstens einmal je 10 Minuten) und bei der täglichen Update-Prüfung (gleicher Schalter wie Programm-Updates).
- Holen: Release-Liste des Katalog-Repos (`?per_page=100`, neuestes nicht als Entwurf markiertes Release), daraus `katalog.json` (≤ 1 MiB) und die Symbole (je ≤ 256 KiB, höchstens 100) über den Etappe-1-Download mit den GitHub-Weiterleitungsregeln.
- Letzter erfolgreicher Stand im Benutzer-Cacheordner (`plugin-cache/katalog/`); ohne Netz ⇒ dieser Stand mit Hinweis „offline – Stand vom …“; noch nie geladen ⇒ „Der Katalog konnte nicht geladen werden“ mit „Erneut versuchen“.
- Fehlerhafte Einträge werden einzeln übergangen und ins Protokoll geschrieben; höchstens 500 Einträge.
- Die Versionen der Plug-ins (für „Aktualisieren auf …“) kommen aus der bestehenden Update-Prüfung von Etappe 1 bzw. werden beim Öffnen der Details aus dem Plug-in-Repo geholt (nur die Release-Liste, kein Paket).

### Erster Start
- Neues Config-Feld `catalog_hint_shown: bool` (nachsichtig, Vorgabe false; nur `true` wird geschrieben).
- Beim ersten Start (Feld false) nach dem Startbildschirm ein Hinweis im Stil der bestehenden Hinweise: „Es gibt Zusatzmodule für PlanoraCAD, z. B. Elektronik. Jetzt ansehen?“ – „Katalog öffnen“ (öffnet Fenster „Zusatzmodule“ am Reiter Katalog) / „Später“. Beide setzen das Feld auf true. Nach einem Update aus einer älteren Version gilt das auch als erster Start (Feld fehlt).

### Feste Adresse
- Konstante im Code (`planora-update`): Katalog-Repo **`KaelanTesseract/PlanoraCAD-Plugins`** (angelegt von der Session „PlanoraCAD Plugin-Pläne“, 2026-11-20; erstes Release `katalog-1` mit einem Eintrag `planora.beispiel-hallo`). Erstes Plug-in-Repo: `KaelanTesseract/PlanoraCAD-Plugin-Hallo`, Release `v0.1.0`. Nicht in den Einstellungen änderbar.

## Nicht-Ziele
- Kein Hochladen oder Einreichen von Plug-ins aus dem Programm.
- Keine Bewertungen, Download-Zahlen oder Screenshots im Katalog.
- Keine Signaturprüfung des Katalogs über GitHub hinaus (HTTPS zu GitHub, Release-Stand). Signierte Kataloge sind spätere Arbeit.

## Rahmen
Wie Etappe 1: Deutsch, Texte in de/en, Verhalten wie Fusion (Vorbild: Fusions „App Store“-Verweis im Fenster „Skripte und Zusatzmodule“), Windows und Ubuntu, keine neuen Abhängigkeiten ohne cargo-deny, Handtests nur per Doppelklick-Start (Claude-gestartete Prozesse sehen ein virtualisiertes AppData).

## Dem Builder überlassen
Aufteilung in Module, genaue Grenzwerte in Konstanten, Gestaltung der Karten im Stil des bestehenden Fensters, Testaufbau mit nachgebildetem Netz.
