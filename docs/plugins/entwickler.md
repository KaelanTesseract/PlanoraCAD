# Plug-ins für PlanoraCAD entwickeln

> **Lizenz zuerst:** Plug-ins müssen unter einer GPL-3.0-verträglichen Lizenz stehen (zum Beispiel `GPL-3.0-only`, `GPL-3.0-or-later`, `MIT`, `Apache-2.0`). Beim Installieren prüft das Programm die Angabe `license` in der `plugin.toml` (SPDX-Kennungen ohne Rücksicht auf Groß- und Kleinschreibung) und zeigt bei unbekannten oder unverträglichen Lizenzen einen deutlichen Hinweis.

Ein Plug-in ist ein kleines WebAssembly-Programm (eine WASM-Komponente), das PlanoraCAD in einer Sandbox ausführt. Es kann Befehle ins Menü „Datei → Zusatzmodule“ legen, Dialoge öffnen, Meldungen zeigen und mit Erlaubnis des Nutzers Webseiten abrufen. Es sieht nur, was die Schnittstelle ihm gibt.

> **Stand der Schnittstelle: 0.1.** Solange die Versionsnummer mit 0 beginnt (0.x), darf die Schnittstelle in jeder neuen Version brechen. Ein Plug-in, das für eine andere Schnittstellenversion gebaut ist, wird nicht geladen; das Programm zeigt dann an, welche Version es braucht und welche es spricht.

## Inhalt

- [Aufbau eines Pakets (`.pcplugin`)](#aufbau-eines-pakets-pcplugin)
- [Die Datei `plugin.toml`](#die-datei-plugintoml)
- [Die Schnittstelle (WIT)](#die-schnittstelle-wit)
- [Erstes Plug-in in Rust, Schritt für Schritt](#erstes-plug-in-in-rust-schritt-für-schritt)
- [Rechte und Anfragen](#rechte-und-anfragen)
- [Zeitgrenzen und Speichergrenzen](#zeitgrenzen-und-speichergrenzen)
- [Installieren, Repo-Link und Updates](#installieren-repo-link-und-updates)
- [In den Katalog](#in-den-katalog)
- [Andere Sprachen (ungetestet)](#andere-sprachen-ungetestet)
- [Leistung](#leistung)

## Aufbau eines Pakets (`.pcplugin`)

Ein Paket ist eine ZIP-Datei mit der Endung `.pcplugin`:

| Eintrag | Pflicht | Inhalt |
| --- | --- | --- |
| `plugin.toml` | ja | Beschreibung des Plug-ins (siehe unten) |
| `plugin.wasm` | ja | die WASM-Komponente |
| `languages/<sprache>.ftl` | nein | Texte je Sprache im Fluent-Format, zum Beispiel `languages/de.ftl` und `languages/en.ftl` |
| `icons/` | nein | Symbole für Befehle |
| `signature` | nein | Signatur; Etappe 1 prüft sie noch nicht, das Programm zeigt „signiert, nicht geprüft“ |

Grenzen beim Auspacken: das ganze Paket höchstens 64 MiB, `plugin.wasm` höchstens 32 MiB, höchstens 1000 Einträge.

## Die Datei `plugin.toml`

```toml
id = "planora.beispiel-hallo"
name = "Beispiel-Hallo"
version = "0.1.0"
api = "0.1"
author = "Dennis Guse"
license = "GPL-3.0-only"
description = "Beispiel-Plug-in: öffnet einen Dialog und sagt Hallo."
# rights = ["net"]              # optional
# net-hosts = ["example.org"]   # nur zusammen mit dem Recht "net"
# memory-mb = 256               # optional
```

| Feld | Pflicht | Regel |
| --- | --- | --- |
| `id` | ja | Kleinbuchstaben, Ziffern, Punkte und Bindestriche, mindestens ein Punkt, höchstens 64 Zeichen. Das erste Teilstück vor dem ersten Punkt hat keinen Bindestrich. Die Kennung ist dauerhaft: Wer sie ändert, ist für das Programm ein anderes Plug-in. |
| `name` | ja | Anzeigename, nicht leer |
| `version` | ja | genau wie bei SemVer, zum Beispiel `1.2.3` oder `1.3.0-beta.1` |
| `api` | ja | Schnittstellenversion, derzeit `"0.1"` |
| `license` | ja | SPDX-Kennung, GPL-3.0-verträglich |
| `author` | nein | Name des Autors; ändert er sich bei einem Update, fragt das Programm Rechte neu ab |
| `description` | nein | Kurzbeschreibung |
| `rights` | nein | Liste aus `net`, `fs-folder`, `model-read`, `model-write` |
| `net-hosts` | nur mit `net` | Rechnernamen, die das Plug-in abrufen darf (nur der Name, ohne `https://`, Pfad oder Port; keine IP-Adressen, auch nicht als Zahl wie `2130706433` oder `0x7f000001`) |
| `memory-mb` | nein | Arbeitsspeicher für das ganze Plug-in, 1 bis 2048, Standard 512 |

`net` ohne `net-hosts` und `net-hosts` ohne `net` werden abgelehnt. Die Rechte hier beschreiben, was das Plug-in braucht; erteilt werden sie erst vom Nutzer (siehe unten).

## Die Schnittstelle (WIT)

Die Schnittstelle steht in `crates/planora-plugin-api/wit/plugin.wit` (Paket `planora:plugin@0.1.0`, Welt `plugin`). Das Plug-in bekommt vom Programm diese Schnittstellen:

- `log`: `write(level, text)` schreibt ins Programmprotokoll.
- `i18n`: `text(key, args)` liefert einen Text aus den `.ftl`-Dateien des Plug-ins in der Sprache des Programms.
- `ui`: `message(level, text)` zeigt eine Meldung, `open-dialog(dialog)` und `close-dialog(id)` öffnen und schließen einen Dialog aus Feldern (`label`, `text`, `number`, `toggle`).
- `rights`: `request(recht)` fragt ein Recht an und liefert eine Anfragekennung, `granted(recht)` fragt ab, ob es schon erteilt ist.
- `net`: `get(url)` ruft eine Webseite ab (Recht `net`, nur `https`, nur Rechner aus `net-hosts`) und liefert eine Anfragekennung.

Das Plug-in selbst muss fünf Funktionen liefern:

- `init(info)` wird einmal beim Start aufgerufen und liefert die Befehle (`registration`) oder einen Fehlertext.
- `run-command(id)` wird aufgerufen, wenn der Nutzer einen Befehl wählt.
- `handle-dialog(id, event)` meldet Eingaben im Dialog: `changed`, `ok`, `cancel`.
- `on-result(id, result)` liefert die Antwort auf eine Anfrage: `granted`, `denied`, `http(...)` oder `failed(text)`. Der Parameter heißt in WIT `%result`.
- `shutdown()` wird beim Beenden aufgerufen.

Anfragen des Plug-ins blockieren das Programm nicht: Sie werden eingereiht und später über `on-result` mit derselben Kennung beantwortet.

## Erstes Plug-in in Rust, Schritt für Schritt

Als Vorlage dient `plugins/beispiel-hallo` (ein Befehl „Hallo sagen“, ein Dialog mit Name und „laut“, eine Meldung).

1. **Rust und das Ziel installieren.** Du brauchst Rust 1.85 oder neuer (das Beispiel nutzt Edition 2024; `rustc --version` zeigt die Version, `rustup update` bringt sie auf Stand).
   ```
   rustup target add wasm32-wasip2
   ```
2. **Projekt anlegen**, und zwar außerhalb des PlanoraCAD-Repos: Liegt der Ordner darin, hält Cargo ihn für einen Teil des Repo-Workspace und baut ihn nicht für sich (die Beispiele unter `plugins/` haben dafür ihren eigenen Workspace).
   ```
   cargo new --lib mein-plugin
   ```
3. **`Cargo.toml` anpassen.** Der Crate wird als `cdylib` gebaut und braucht `wit-bindgen`:
   ```toml
   [package]
   name = "mein-plugin"
   version = "0.1.0"
   edition = "2024"
   license = "GPL-3.0-only"
   publish = false

   [lib]
   crate-type = ["cdylib"]

   [dependencies]
   wit-bindgen = { version = "0.63", default-features = false, features = ["macros", "realloc"] }
   ```
4. **WIT-Datei bereitstellen.** Kopiere `crates/planora-plugin-api/wit/plugin.wit` nach `wit/plugin.wit` in dein Projekt, in der Version, für die du baust.
5. **`src/lib.rs` schreiben.** `wit_bindgen::generate!` erzeugt die Bindungen aus der WIT-Datei, dann setzt ein Typ `Guest` um. `generate_all` lässt wit-bindgen auch Bindungen für Schnittstellen aus anderen WIT-Paketen erzeugen, die eine Welt einbindet (ohne die Angabe müsste jede davon unter `with` stehen, sonst bricht der Bau ab). `plugin.wit` braucht heute keine fremden Pakete; die Angabe schadet nicht und hält dein Plug-in baubar, falls eine spätere Version der Schnittstelle etwa WASI-Typen einbindet. Die Typen der Schnittstelle liegen unter `planora::plugin::…`:
   ```rust
   wit_bindgen::generate!({ path: "wit", world: "plugin", generate_all });

   use planora::plugin::types::Command;

   struct Mein;

   impl Guest for Mein {
       fn init(_info: HostInfo) -> Result<Registration, String> {
           Ok(Registration {
               commands: vec![Command {
                   id: "hallo".into(),              // Kennung, kommt in run_command zurück
                   label_key: "hallo-befehl".into(), // Schlüssel in languages/*.ftl
                   icon: None,
                   shortcut: None,
               }],
           })
       }
       fn run_command(_id: String) -> Result<(), String> { Ok(()) }
       fn handle_dialog(_id: String, _event: DialogEvent) -> Result<(), String> { Ok(()) }
       fn on_result(_id: u64, _result: RequestResult) {}
       fn shutdown() {}
   }

   export!(Mein);
   ```
   Den vollständigen Ablauf mit Dialog, Feldern und Übersetzung zeigt `plugins/beispiel-hallo/src/lib.rs`.
6. **Texte anlegen.** In `languages/de.ftl` und `languages/en.ftl` stehen die Schlüssel, die du in `label-key`, `title-key` und `i18n::text` benutzt (siehe `plugins/beispiel-hallo/languages/`).
7. **`plugin.toml` schreiben** (Felder siehe oben).
8. **Bauen.**
   ```
   cargo build --release --target wasm32-wasip2
   ```
   Heraus kommt `target/wasm32-wasip2/release/mein_plugin.wasm`.
9. **Paket packen.** Im Repo erledigt das `tools/build-plugins.ps1` (Windows) bzw. `tools/build-plugins.sh` (Ubuntu): Es baut die Beispiele in `plugins/` und packt sie mit `crates/planora-plugin-host/examples/pack.rs` nach `crates/planora-plugin-host/tests/data/`. Für ein eigenes Plug-in packst du `plugin.toml`, die gebaute `.wasm` (als `plugin.wasm`) und den Ordner `languages/` in ein ZIP und nennst es `mein-plugin.pcplugin`.
10. **Ausprobieren.** In PlanoraCAD „Datei → Zusatzmodule → Verwalten …“ öffnen, dann „Aus Datei…“ und die `.pcplugin` wählen. Rechte und Lizenz werden angezeigt, nach dem Bestätigen erscheint der Befehl unter „Datei → Zusatzmodule“.

## Rechte und Anfragen

Es gibt vier Rechte: `net`, `fs-folder`, `model-read`, `model-write`. In Etappe 1 ist nur `net` mit Funktion hinterlegt (`net.get`); die übrigen drei sind für spätere Etappen vorgesehen: `rights.request` beantwortet sie ohne Nachfrage mit `denied`, und der Installationsdialog zeigt sie als „in dieser Version noch nicht verfügbar“. Ein Recht muss in der `plugin.toml` stehen und wird dann vom Nutzer erteilt:

- Direkt nach dem Installieren hat ein Plug-in **kein** Recht.
- Beim ersten Gebrauch fragt das Programm mit „Erlauben“, „Nur diesmal“ oder „Ablehnen“. „Erlauben“ und „Ablehnen“ gelten auf Dauer; „Nur diesmal“ gilt nur für die Anfrage, die gefragt hat (und für Anfragen desselben Rechts, die bis zur Antwort warten), die nächste fragt wieder. Jede Anfrage trägt eine Kennung (`request-id`), die das Plug-in von `rights.request` bzw. `net.get` zurückbekommt; die Antwort kommt später über `on-result` mit derselben Kennung.
- Der Nutzer kann ein erteiltes Recht im Fenster „Zusatzmodule“ jederzeit zurücknehmen.
- Bei einem Update bleiben erteilte Rechte nur erhalten, wenn `author` und die Herkunft gleich bleiben (derselbe Repo-Link, oder beide aus einer Datei, oder beide mitgeliefert); ändert sich `net-hosts`, entfällt das Recht `net` und wird neu gefragt. Fehlt `author`, wird immer neu gefragt.
- Beim Programmstart gelten gespeicherte Rechte nur, wenn Autor, Herkunft und `net-hosts` zur geladenen `plugin.toml` passen.

`net.get` erlaubt nur `https`, Port 443, keine Zugangsdaten in der Adresse, keine IP-Adressen und nur Rechner aus `net-hosts`. Weiterleitungen werden Schritt für Schritt geprüft.

## Zeitgrenzen und Speichergrenzen

| Grenze | Wert |
| --- | --- |
| Aufrufe, die nur die Oberfläche bedienen | 200 ms |
| Befehle (`run-command`) und `init` | 5 s |
| Arbeitsspeicher je Plug-in (alle Speicher und Tabellen zusammen) | `memory-mb`, Standard 512 MB, höchstens 2048 MB |
| Instanzen, Speicher, Tabellen je Plug-in | höchstens 32, 8, 8 |
| Ausgabe (stdout/stderr), die zwischen zwei Aufrufen gemerkt wird | 64 KiB |
| Warteschlange je Plug-in | 1024 Einträge, Ereignisse (Meldungen, Dialoge, Anfragen) zusammen höchstens 8 MiB; darüber wird verworfen (mit einem Hinweis im Protokoll) |
| Meldung oder Protokolleintrag | 4 KiB, länger wird mit „…“ gekürzt |
| Dialog (`open-dialog`) | höchstens 200 Felder, jede Zeichenkette (Kennung, Schlüssel, Textwert) höchstens 1 KiB, Feld-Kennungen nicht leer und nicht doppelt; sonst gibt `open-dialog` einen Fehler zurück |
| Befehle aus `init` | höchstens 100, jede Zeichenkette höchstens 256 Bytes; sonst startet das Plug-in nicht |
| Gleichzeitige Netzanfragen (`net.get`) | 4; weitere bekommen sofort `failed` |
| Ressourcen (WASI-Streams, Wartezeiten) | höchstens 4096 gleichzeitig |
| Zustellungen (`on-result`) je Bild | höchstens 16 und zusammen 200 ms; der Rest kommt im nächsten Bild |

Wer die Frist reißt, abstürzt oder den Speicher sprengt, wird angehalten; das Programm zeigt „Das Plug-in ist abgestürzt“ bzw. die Ursache und bietet „Neu starten“ an. Reicht das Budget für Zustellungen in drei Bildern hintereinander nicht (etwa weil `on-result` jedes Mal neue Arbeit anfordert), zählt das ebenfalls als Ausfall („zu viel Arbeit“). Nach drei Ausfällen in einer Sitzung bleibt das Plug-in bis zum Neustart des Programms aus; eine neue Version beginnt wieder bei null.

WASI-Sockets (`wasi:sockets`) und Namensauflösung sind abgeschaltet: Netz gibt es nur über `net.get`.

## Installieren, Repo-Link und Updates

- **Aus Datei…** installiert eine `.pcplugin` von der Festplatte.
- **Repo-Link** (Feld im selben Dialog, Knopf „Holen“): ein GitHub-Link der Form `github.com/<Besitzer>/<Repo>`, mit oder ohne `https://`, ohne Port oder Abfrage. Das Programm sucht in den Veröffentlichungen (Releases) des Repos:
  - Eine Veröffentlichung enthält **genau eine** `.pcplugin` und optional dazu `<Paket>.sha256` mit der Prüfsumme. Zwei Prüfsummen oder eine Prüfsumme von fremder Adresse lassen die Veröffentlichung ausfallen.
  - Der **Tag ist die Version**, zum Beispiel `v1.2.0` oder `1.2.0`. Entwürfe und Tags ohne Versionsangabe werden übersprungen.
  - Kanal nach dem Zusatz: ohne Zusatz stabil, `beta…` oder `rc…` Beta, alles andere Entwicklerkanal. Angeboten werden nur Versionen, die nach SemVer-Vorrang höher sind als die installierte (Build-Metadaten wie `+abc` zählen nicht); das gilt auch noch beim Einspielen. Gelesen wird die erste Seite der Release-Liste (bis zu 100 Veröffentlichungen).
- Das Programm sucht beim Start (höchstens einmal am Tag) nach Updates für Plug-ins, die über einen Repo-Link installiert wurden, und holt dabei nur die Release-Liste (30 Sekunden). Gibt es neuere Versionen, erscheint ein Hinweis; das Paket selbst wird erst beim Öffnen von „Aktualisieren“ geladen (höchstens 64 MiB, 5 Minuten insgesamt); bis dahin zeigt der Dialog „Wird geholt…“.
- Beim Ersetzen durch eine neue Version bleiben Rechte nur bei gleichem Autor, gleicher Herkunft und gleichen Rechnern erhalten (siehe oben).

## In den Katalog

Der Katalog (Reiter **Katalog** im Fenster **Zusatzmodule**) wird kuratiert. Plug-ins kommen nur über Dennis Guse in den Katalog; gepflegt wird er von der Session „PlanoraCAD Plugin-Pläne“, die auch die Plug-in-Repos anlegt. Aufgenommen werden nur Plug-ins von Dennis selbst. Wer ein Plug-in anderer Herkunft anbieten will, nutzt Datei oder Repo-Link (siehe oben).

Regeln für ein Plug-in im Katalog:

- Jedes Plug-in hat ein eigenes GitHub-Repo mit Veröffentlichungen (Releases), die dem Abschnitt „Installieren, Repo-Link und Updates“ genügen (eine `.pcplugin`, Tag = Version, optional `.sha256`). Der Installer von PlanoraCAD liefert keine Plug-ins mit.
- Das Katalog-Repo `KaelanTesseract/PlanoraCAD-Plugins` enthält nur die Liste. Das Programm liest das neueste nicht als Entwurf markierte Release (GitHub-API `/releases/latest`); `katalog.json` und die Symbole hängen als Dateien an diesem Release, nicht am Hauptzweig. Der Katalog ist im Programm fest eingetragen und nicht einstellbar.
- `katalog.json` (höchstens 1 MiB), Format 1:

```json
{
  "format": 1,
  "eintraege": [
    {
      "id": "planora.beispiel-hallo",
      "name": { "de": "Beispiel-Hallo", "en": "Example Hello" },
      "beschreibung": { "de": "…", "en": "…" },
      "kategorie": "beispiele",
      "repo": "https://github.com/KaelanTesseract/PlanoraCAD-Plugin-Hallo",
      "symbol": "symbol.png"
    }
  ]
}
```

- `id` ist die Plug-in-Id aus der `plugin.toml`; `repo` ist der Repo-Link ohne Port und Abfrage. `kategorie` ist `elektronik`, `import-export`, `werkzeuge` oder `beispiele`; alles andere erscheint als „Sonstiges“.
- `symbol` ist optional und nennt eine weitere Datei im selben Release: **nur PNG**, höchstens 256 × 256 Pixel und 256 KiB. Fehlt es oder ist es ungültig, nimmt das Programm das Standardsymbol. SVG wird nicht gezeichnet.
- Unbekannte Felder werden ignoriert. Eine unbekannt höhere `format`-Nummer zeigt „Katalog braucht eine neuere PlanoraCAD-Version“ und sonst nichts.
- Version, verlangte Rechte und Autor stammen immer aus dem Plug-in-Repo, nicht aus der Liste. Die Installation läuft wie über den Repo-Link; als Herkunft wird zusätzlich „aus dem Katalog“ vermerkt.

## Andere Sprachen (ungetestet)

Jede Sprache, die WASM-Komponenten für `wasm32-wasip2` erzeugen kann, ist grundsätzlich möglich. Die folgenden Wegweiser sind **nicht getestet**; sie sind nur der erste Anlaufpunkt.

- **C und C++:** Bindungen mit `wit-bindgen c` aus der WIT-Datei erzeugen, mit einer WASI-SDK-Version bauen, die Komponenten unterstützt.
- **Go:** mit TinyGo (`-target=wasip2`) bauen; die Bindungen erzeugt `wit-bindgen-go`.
- **Python:** mit `componentize-py` aus der WIT-Datei eine Komponente erzeugen.

Wichtig bleibt: Die Komponente muss die Welt `plugin` aus `plugin.wit` umsetzen und darf nur die dort genannten Schnittstellen importieren.

## Leistung

Gemessen am 2026-11-20 unter Windows im Release-Profil mit den beiden Beispiel-Plug-ins (`beispiel-hallo`, `test-faelle`). Gemessen wird die Zeit von `PluginHost::start_all` bis beide Plug-ins „Running“ sind, ohne die Oberfläche zu starten (ignorierter Test `messung_start` in `crates/planora-plugin-host/tests/host.rs`; Aufruf: `cargo test --release -p planora-plugin-host --test host messung_start -- --ignored --nocapture`).

| Start | Zeit (zwei Läufe) |
| --- | --- |
| ohne Plug-ins | 0,2 bis 0,3 ms |
| mit beiden Plug-ins, erster Start (Übersetzen mit Cranelift) | 32 bis 34 ms |
| mit beiden Plug-ins, zweiter Start (aus dem Zwischenspeicher) | 11 ms |

Der zweite Start liegt rund 11 ms über dem Start ohne Plug-ins; das Ziel waren höchstens 50 ms. Die Plug-ins starten im Hintergrund, die Oberfläche wartet nicht darauf.

`planora-app.exe` (Release) ist mit der Plug-in-Laufzeit (wasmtime) 61,4 MB groß; ein Release-Bau von `master` zum Vergleich lag nicht vor, ein älterer Release-Bau im Hauptordner (Zweig nicht bekannt) hatte 42,7 MB. Der Release-Bau dieses Zweigs dauerte 207 s (ein Vergleichswert von master fehlt).
