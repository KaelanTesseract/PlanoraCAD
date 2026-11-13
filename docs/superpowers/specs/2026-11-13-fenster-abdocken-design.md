# Fenster abdocken (zweiter Monitor)

Stand 2026-11-13. Programmweites Teilprojekt, unabhängig von den Skizzen-Teilprojekten. Dennis hat entschieden: Abdocken auf Wunsch (Weg A), abdockbar sind die Dialoge, der Featurebaum, die rechte Spalte und das Skizzenfeld; die Verlaufsleiste bleibt im Hauptfenster. Alles mit **Festlegung** Markierte hat Claude entschieden; Dennis kann es beim Lesen ändern.

## Warum

Dennis arbeitet mit zwei Monitoren. Er will (1) die 3D-Ansicht frei haben, während ein Dialog offen ist (etwa Extrudieren mit Vorschau und Griffen), und (2) bestimmte Fenster dauerhaft auf dem zweiten Monitor stehen haben, unter anderem den Featurebaum. Heute zeichnet `Dialog` (`planora-app/src/dialog.rs`) jeden Dialog als `egui::Window` ins Hauptfenster, die Felder stehen fest im Layout (`shell_panels.rs`). Nichts davon kann das Hauptfenster verlassen. Fusion lässt seine Befehlsdialoge frei auf andere Monitore ziehen.

## Ziele

1. Jeder Dialog über `Dialog` (außer festen Rückfragen, siehe unten) und jedes der drei Felder lässt sich per Symbol in der Kopfzeile **abdocken**: Er wird ein eigenes Betriebssystemfenster, frei verschiebbar, auch auf einen anderen Monitor. Dasselbe Symbol im abgedockten Fenster **dockt an**: zurück ins Hauptfenster an die gewohnte Stelle.
2. Das Programm merkt sich je Fenster, ob es abgedockt ist, und seine Lage und Größe, **auch über Neustarts** (`config.json`). Ein abgedockter Dialog öffnet beim nächsten Mal gleich abgedockt an seiner Stelle; ein abgedocktes Feld erscheint beim Start gleich dort.
3. Fehlt der Monitor einer gemerkten Lage, erscheint das Fenster **angedockt im Hauptfenster**; der Eintrag „abgedockt mit Lage“ bleibt in der Konfiguration erhalten. Ist der Monitor beim nächsten Öffnen wieder da, steht es wieder dort.
4. Bedienung im abgedockten Fenster wie angedockt: Mausbedienung, Esc, Enter, Tab, Eingabefelder, Tooltips, Kontextmenüs, Rollen. In den abgedockten **Feldern** wirken außerdem alle Tastenkürzel des Programms (Strg+Z, Entf, S, Skizzenwerkzeuge …) genau so, als hätte das Hauptfenster den Fokus.
5. Vorschau, Griffe und Hervorhebung in der 3D-Ansicht laufen weiter, während der Dialog oder Baum woanders steht: Eine Änderung im abgedockten Fenster zeichnet das Hauptfenster im selben Bild neu; Hover im abgedockten Baum hebt im Hauptfenster hervor wie heute.
6. Ist ein Feld abgedockt, fällt sein Platz im Hauptfenster weg und die freie Fläche (3D-Ansicht, Zeichnung) wird entsprechend größer.
7. Ohne Abdocken bleibt alles bytegleich wie heute: Aussehen, Lage, Tests.

## Nicht-Ziele

- Herausziehen über den Fensterrand ohne Klick (egui kann ein `egui::Window` nicht fließend in ein Betriebssystemfenster überführen; Nachbau nur mit Flackern).
- Abdocken der Verlaufsleiste, der Werkzeugleisten, des oberen Feldes, Status- und Hinweisfeldes, der Tab-Leiste, der Skizzen-Werkzeugleiste und des Skizzen-Fußfelds.
- Feste Rückfragen (`Dialog::fixed()`: `prompts.rs`, `project_actions.rs`, `versions_panel.rs`), der Absturzhinweis beim Start und der Update-Hinweis unten rechts (`bottom_right_in`) bleiben ohne Symbol. **Festlegung** für den Update-Hinweis: Er ist eine kurze Meldung, kein Arbeitsfenster.
- Mehrere Hauptfenster, abgedockte 3D-Ansicht, Dateiformat-Änderung.
- Lage merken unter Wayland (siehe Grenzen).

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es technisch läuft)

- Ein abgedocktes Fenster, das nach einem Neustart unerreichbar außerhalb aller Monitore steht.
- Tastenkürzel, die im abgedockten Baum stillschweigend nichts tun, oder Kürzel, die doppelt ausgeführt werden.
- Eine Vorschau in der 3D-Ansicht, die erst nach Mausbewegung im Hauptfenster nachzieht.
- Ein Programm, das sich mit abgedockten Fenstern nicht mehr sauber beenden lässt (Rückfrage „Ungespeichert“ wie heute, abgedockte Fenster schließen mit).

## Wege (betrachtet)

- **A) Abdocken auf Wunsch (gewählt).** Symbol in der Kopfzeile, abgedockt als eigenes Betriebssystemfenster über eframes Multi-Viewport. Ohne Abdocken alles wie heute, mit einem Monitor kein Unterschied, Dialoge und Felder verhalten sich gleich. Nachteil: erst klicken, dann ziehen.
- **B) Jeder Dialog immer eigenes Fenster** (wie Fusion technisch). Verworfen: ändert alle Dialoge auch für Ein-Monitor-Nutzer; egui kann ein Fenster nicht an das Hauptfenster binden, ein Dialog auf demselben Monitor rutscht beim Klick ins Hauptfenster dahinter (oder liegt „immer oben“ auch über fremden Programmen); die Felder bräuchten trotzdem Abdocken auf Wunsch – zwei Verhaltensweisen.
- **C) Hauptfenster über beide Monitore spannen.** Verworfen: Maximieren und Taskleiste gehen kaputt, ohne zweiten Monitor passt nichts.

## Bestand (geprüft im Code)

- `Dialog` in `dialog.rs`: 47 Aufrufe in ~30 Dateien. Größen `Prompt/Normal/Wide/File`, Höhe folgt dem Inhalt, eigene Kopfzeile (`header`: Titel, Schließen-Symbol), Fußzeile (`show_with_footer`), mehrere `request_discard`-Kniffe gegen versetzte erste Bilder, `constrain_to(content)`. Id hängt am **Titel** (also an der Sprache), außer wo `.id(..)` gesetzt ist (z. B. `update_dialog.rs`, `dimension_browser.rs`, `drawing_browser.rs`, `crash_dialog.rs`). `bottom_right_in` nutzt nur der Update-Hinweis (`update_dialog.rs`).
- Felder: `shell_panels::show_fields` zeichnet links den Baum (`left_field` → `browser::show_in_panel`, auch im Zeichnungsmodus), rechts Eigenschaften/Versionen (`RightTab`) bzw. im Zeichnungsmodus das Ansichtsfeld; im Skizzenmodus zeichnet `sketch_panel::show` Leiste, Skizzenfeld (`sketch_side_id`) und Fuß an den Rechtecken aus `shell::layout_in_mode`.
- Tastenkürzel: `PlanoraApp::shortcuts` (app.rs) liest `ctx.input` des Hauptfensters; die Befehle sind gesperrt, solange ein Dialog offen ist (`windows_open`). Esc/Enter behandeln die Dialoge selbst (u. a. `projects_window`, `plane_dialog`, `insert/*`, `update_dialog`, `feedback_dialog`, `file_chooser`) – teils innerhalb, teils möglicherweise außerhalb der Dialog-Closure.
- eframe/egui 0.36.2 mit wgpu: `Context::show_viewport_immediate` erzeugt ein Betriebssystemfenster; bei `embed_viewports() == true` (Grundwert eines nackten `egui::Context`, also in allen Tests) zeichnet egui den Inhalt stattdessen als `egui::Window` ins Elternfenster. `ViewportBuilder` kennt Lage, Größe, `with_decorations`, `with_taskbar`, `with_window_level`, aber **kein Besitzerfenster**. `ViewportCommand::StartDrag`/`BeginResize` gibt es.
- Konfiguration: `planora-project/src/config.rs` (`Config`, `update_at` lesen-ändern-schreiben, tolerant geladen; `WindowState { x, y, width, height, maximized }` für das Hauptfenster).
- `windows-sys 0.61.2` ist schon Abhängigkeit der App (Windows); für Monitorabfragen fehlen die Features `Win32_Graphics_Gdi` (`MonitorFromRect`) und ggf. `Win32_UI_HiDpi` (`GetDpiForMonitor`).
- Test-Harness der App-Tests (`Screen`/`Harness`) rendert einen nackten `egui::Context` mit einem Fenster.

## Entwurf

### Bedienung

- **Symbol „Abdocken“** in der Kopfzeile links neben dem Schließen-Symbol (Dialoge) bzw. rechts in der Kopfzeile der Felder. Abgedockt zeigt dieselbe Stelle das Symbol „Andocken“. Tooltip und Bildschirmleser-Name über `tr!` (de/en). **Festlegung:** deutsche Namen „Abdocken“/„Andocken“ (en „Undock“/„Dock“), sofern Fusion keine anderen nutzt; Symbol aus der vorhandenen Phosphor-Symbolschrift.
- **Abdocken:** Das Fenster erscheint als eigenes Fenster an der gemerkten Lage; gibt es keine (erstes Mal), an der Stelle, wo es im Hauptfenster stand (Bildschirmkoordinaten), sodass es optisch „herausspringt“ und dann gezogen werden kann.
- **Ziehen:** an der eigenen Kopfzeile wie heute (`ViewportCommand::StartDrag`). Felder lassen sich außerdem an den Rändern in der Größe ändern (`BeginResize`); Dialoge nicht (Breite fest wie heute, Höhe folgt dem Inhalt).
- **Andocken:** Symbol oder Doppelklick auf die Kopfzeile. Das Fenster kehrt an seinen gewohnten Platz im Hauptfenster zurück; die gemerkte abgedockte Lage wird gelöscht.
- **Schließen** eines abgedockten Dialogs (Schließen-Symbol, Abbrechen, OK, Esc) schließt ihn wie heute; beim nächsten Öffnen kommt er wieder abgedockt. Abgedockte Felder haben kein Schließen-Symbol (wie heute angedockt).
- **Festlegung:** Abgedockte Fenster ohne Rahmen des Betriebssystems (`with_decorations(false)`), mit der Graphit-Kopfzeile wie im Hauptfenster, ohne eigenen Taskleisten-Eintrag (`with_taskbar(false)`), normale Fensterebene (nicht „immer oben“). Begründung: einheitliches Aussehen; auf dem zweiten Monitor überdecken sich die Fenster nicht, darum stört das fehlende Besitzerfenster dort nicht.
- **Festlegung:** Fokus: Ein frisch abgedockter oder abgedockt geöffneter Dialog bekommt den Fokus (Esc/Enter wirken sofort). Ein beim Start erscheinendes abgedocktes Feld nimmt dem Hauptfenster den Fokus nicht.

### Was abdockbar ist (Plätze)

| Platz | Inhalt | Modi |
|---|---|---|
| `tree` | linke Spalte (Featurebaum bzw. Baum der Zeichnung) | Modell, Zeichnung |
| `right` | rechte Spalte: Eigenschaften/Versionen bzw. Ansichtsfeld der Zeichnung | Modell, Zeichnung |
| `sketch_side` | Skizzenfeld rechts | Skizze |
| `dialog:<schlüssel>` | je Dialog ein Eintrag | überall |

**Rückfragen und Unterdialoge aus abgedockten Feldern** (Dennis' Entscheidung): Öffnet ein abgedocktes Feld selbst einen Dialog (Umbenennen und Lösch-Rückfragen des Baums `browser.rs`, `drawing_browser::windows`, `dimension_browser::windows`; feste Rückfragen der Versionsspalte `versions_panel.rs`), erscheint dieser **als eigenes kleines Fenster mittig über dem abgedockten Feld**, also auf dessen Monitor, nicht im Hauptfenster und nicht im (zu schmalen) Feldfenster. Er hat kein Symbol „Abdocken“ und wird nicht gemerkt; ist das Feld angedockt, erscheint er wie heute im Hauptfenster. Solange er offen ist, bleibt das Feldfenster gesperrt wie heute der Hintergrund einer Rückfrage. Dieselbe Rückfrage, ausgelöst aus dem Hauptfenster (etwa die Lösch-Rückfrage aus der Zeichnungsansicht), erscheint im Hauptfenster.

**Festlegung:** Das Ansichtsfeld der Zeichnung gehört zum Platz `right` (es steht an derselben Stelle; Dennis hat „rechte Spalte“ gewählt). Ein Platz, den der aktuelle Modus nicht zeigt, hat kein Fenster (abgedocktes Skizzenfeld erscheint beim Betreten der Skizze und verschwindet beim Verlassen). Die Felder zeigen immer den aktiven Tab.

### Stabile Schlüssel

Jeder abdockbare Dialog bekommt einen festen, sprachunabhängigen Schlüssel (z. B. `extrude`, `blend`, `export`, `parameters`), angegeben am Aufruf (`Dialog::new(...).dock_key("blend")` o. Ä.). Ohne Schlüssel ist ein Dialog nicht abdockbar (kein Symbol). Ein Test stellt sicher, dass alle nicht festen `Dialog`-Aufrufe einen Schlüssel haben und keine zwei gleich sind. Dialoge mit wechselndem Titel (etwa Feature-Dialoge je Art) bekommen je Art einen Schlüssel – **Festlegung:** je Feature-Art ein eigener Platz (Extrudieren und Verrunden stehen also getrennt), wie Fusion die Lage je Befehl führt (**Annahme**).

### Speichern

- Neues Feld in `Config`: `detached: BTreeMap<String, DetachedWindow>` mit `skip_serializing_if = "BTreeMap::is_empty"` (alte `config.json` bleibt bytegleich, solange nichts abgedockt war). `DetachedWindow { x, y, width, height }` in **physischen Bildschirmpixeln** (siehe DPI). Unbekannte/kaputte Einträge werden beim Laden verworfen, nie die ganze Datei.
- Geschrieben über `Config::update_at` (lesen-ändern-schreiben) beim Abdocken, Andocken und nach dem Ende eines Zieh-/Größenänderungsvorgangs (nicht in jedem Bild). Beim Beenden einmal den letzten Stand.
- Ein Eintrag vorhanden = abgedockt. Andocken entfernt ihn.

### Monitor fehlt

- Vor dem Öffnen eines abgedockten Fensters prüft eine Funktion `on_screen(rect) -> Option<bool>`, ob mindestens ein ausreichend großer Teil der Kopfzeile (**Festlegung:** 64×24 px) auf einem vorhandenen Monitor liegt. Windows: `MonitorFromRect(…, MONITOR_DEFAULTTONULL)` (windows-sys, Feature `Win32_Graphics_Gdi`). Linux/X11: **Festlegung:** keine eigene Prüfung (`None`), die Lage wird übernommen; der Fenstermanager holt Fenster außerhalb der Fläche üblicherweise zurück (**Annahme**, Handprüfung unter WSLg).
- `Some(false)`: Das Fenster erscheint **angedockt** im Hauptfenster (mit Symbol „Abdocken“), der Eintrag bleibt in `config.json`. Klickt Dennis jetzt „Abdocken“, springt es an die aktuelle Stelle heraus und der Eintrag wird überschrieben.
- Geprüft wird bei jedem Öffnen (Dialog) bzw. beim Start und beim Wechsel in einen Modus mit dem Platz (Felder), nicht laufend. Wird ein Monitor im Betrieb abgezogen, verschiebt Windows die Fenster selbst.

### DPI und Monitore mit verschiedener Skalierung

egui rechnet in Punkten je Viewport; ein abgedocktes Fenster kann auf einem Monitor mit anderer Skalierung stehen als das Hauptfenster. Gespeichert wird in **physischen Pixeln**: `ViewportInfo::outer_rect` × `pixels_per_point` des Fensters (voller Faktor, nicht nur `native_pixels_per_point`; heute gleich, weil die App den Zoom abschaltet). Achtung (geprüft in egui-winit 0.36.2 `lib.rs:2111-2133`, winit 0.30.13 Windows `window.rs`): `with_position` und `ViewportCommand::OuterPosition` gehen als logische Lage an winit, und winit rechnet mit dem Faktor um, den das Fenster **gerade** hat, nicht mit dem des Zielmonitors. Darum **Festlegung:** unter Windows die physische Lage nach dem Erzeugen direkt setzen (`SetWindowPos` über die Fensterkennung) oder die logische Lage mit genau dem Faktor rechnen, den winit nimmt; welcher Weg, entscheidet der Umsetzer nach einem Versuch. Die Dialogbreiten (`DialogSize`) gelten in Punkten des Fensters, in dem der Dialog steht. Handprüfung mit 100 % und 125 % nötig (siehe Prüfung).

### Technik

- `Dialog::show_inner` und die Feldzeichnung bekommen einen gemeinsamen Baustein (neues Modul, **Festlegung:** `detach.rs`): Ist der Platz abgedockt und `!ctx.embed_viewports()`, zeichnet er den Inhalt über `ctx.show_viewport_immediate(ViewportId::from_hash_of(platz), builder, …)` in einem `CentralPanel` mit `theme::panel_frame`, sonst wie heute. `immediate` statt `deferred`, weil die Inhalte `&mut AppState` und andere Leihen brauchen.
- Bei `embed_viewports() == true` (Tests, Plattformen ohne Multi-Viewport) gibt es **kein** Symbol „Abdocken“ und alles läuft wie heute. Für Tests des abgedockten Wegs setzt der Test `ctx.set_embed_viewports(false)` und einen eigenen Renderer über `Context::set_immediate_viewport_renderer` (öffentlich, je Thread), der den Inhalt des Kindfensters mit **eigener** `RawInput` ausführt. Nur so sind getrennte Eingabe und getrennter Fokus prüfbar (eingebettete Viewports teilen die Eingabe des Elternfensters).
- Hauptfenster: `ctx.set_embed_viewports(false)` ist bei eframe mit wgpu der Grundwert; prüfen, dass der wgpu-Renderer mehrere Viewports bedient (eframe `wgpu` unterstützt das).
- Die Discard-Kniffe von `Dialog` (Größenmessung, Fußzeilenhöhe) gelten je Viewport; im abgedockten Fenster bestimmt die gemessene Inhaltshöhe die Fensterhöhe (`ViewportCommand::InnerSize`, höchstens bis zur Höhe des Monitors abzüglich Rand, darüber rollt der Inhalt wie heute).
- **Neuzeichnen:** Eine Eingabe im Kindfenster löst bei immediate-Viewports schon selbst ein Neuzeichnen des Hauptfensters aus (eframe `wgpu_integration.rs:640-648`); das Kindfenster läuft mitten im Bild des Hauptfensters an der Stelle seines Aufrufs. Felder laufen vor der 3D-Ansicht (`show_fields`), Dialoge danach. **Festlegung:** Ändert ein abgedockter Dialog Werte, die die Vorschau betreffen, und die 3D-Ansicht ist in diesem Bild schon gezeichnet, fordert er ein weiteres Bild an (`request_repaint`), damit die Vorschau ohne Mausbewegung nachzieht (Anti-Ziel).
- **Tastenkürzel aus abgedockten Fenstern:** `shortcuts()` läuft vor allen Feldern und Dialogen und sieht nur die Eingabe des Hauptfensters. Für jedes abgedockte Fenster mit Fokus läuft darum der Verteiler (`keymap::dispatch`) **im Callback des Kindfensters mit dessen Eingabe**, nach dessen Inhalt (nur, was der Inhalt nicht verbraucht hat; kein Eingabefeld mit Fokus), mit denselben Sperren wie im Hauptfenster. Es gilt dieselbe Regel wie heute: Bei offenem Dialog wirken nur die Kürzel mit Bereich „Überall“ (`area() == All`, etwa Strg+S, Strg+N, Strg+O, Strg+W, Strg+Umschalt+E, Rückmeldung, `app.rs:1247`); ohne Dialog alle. **Festlegung:** Das gilt für abgedockte Felder **und** abgedockte Dialoge, sonst täten die Überall-Kürzel im abgedockten Dialog stillschweigend nichts (Anti-Ziel). Genau einmal: Jedes Ereignis gehört nur dem Fenster, das es empfangen hat. Esc/Enter gehören weiter dem Dialog.
- **Esc/Enter der Dialoge:** Wo ein Dialog Esc/Enter außerhalb seiner Closure über das Hauptfenster liest, wird das auf das Fenster umgestellt, in dem der Dialog steht. Ergebnis: Esc/Enter wirken im abgedockten Dialog, wenn er den Fokus hat. **Festlegung:** Esc im Hauptfenster bricht einen offenen abgedockten Befehlsdialog ebenfalls ab (wie Esc in Fusions Zeichenfläche den Befehl abbricht); Enter im Hauptfenster bestätigt nicht.
- **Beenden:** Das Schließen-Ereignis eines abgedockten Fensters (Alt+F4) dockt es an (bei Feldern) bzw. schließt den Dialog (wie Abbrechen); es beendet nie das Programm. Das Beenden des Hauptfensters schließt alle abgedockten Fenster mit, nach der bestehenden Rückfrage.
- **Hauptfenster minimiert:** abgedockte Fenster bleiben stehen (**Festlegung**, einfachster Weg; Fusion-Verhalten unbekannt).
- **Layout:** `shell::layout_in_mode` bekommt die abgedockten Plätze als Eingabe; ein abgedockter Platz belegt keine Breite, die freie Fläche wächst. Ohne abgedockte Plätze bleiben alle Rechtecke bytegleich.

## Grenzen

- **Wayland:** Programme dürfen Fensterlagen weder setzen noch lesen. Abdocken funktioniert, die Lage bestimmt der Compositor, gemerkt wird nur „abgedockt“ und die Größe. In `offene-punkte.md` vermerken.
- Kein Besitzerfenster: Ein abgedocktes Fenster auf demselben Monitor wie das Hauptfenster rutscht beim Klick ins Hauptfenster dahinter. Wieder vorholen: Andocken oder Wechsel per Alt+Tab (**Festlegung:** ohne Taskleisten-Eintrag; falls Alt+Tab es dann nicht zeigt, ist das in der Handprüfung zu klären und ggf. `with_taskbar(true)`). Optional später unter Windows ein echtes Besitzerfenster über die Fensterkennung (`GWLP_HWNDPARENT`), falls eframe sie hergibt – nicht Teil dieses Projekts.
- Neue Abhängigkeiten: keine (nur ein zusätzliches Feature von `windows-sys`). `cargo deny check` muss grün bleiben.

## Prüfung

- **Tests (Windows und WSL, voll):** reine Funktionen (Layout mit/ohne abgedockte Plätze bytegleich, `on_screen`-Auswertung, Umrechnung physisch↔Punkte, Laden/Schreiben von `detached` inkl. kaputter Einträge und bytegleicher alter Datei); Test über alle `Dialog`-Aufrufe (Schlüssel vorhanden und eindeutig); Harness-Tests des abgedockten Zweigs über den Testschalter (Esc/Enter wirken, Tastenkürzel aus abgedocktem Baum werden genau einmal ausgeführt, Vorschau-Zustand ändert sich im selben Bild); alle bestehenden Tests unverändert grün.
- Messtests im Release, fmt, clippy `-D warnings`, `cargo deny check` wie üblich.
- **Handtest (nur nach Dennis' Go):** 1920×1080 Hauptmonitor, zweiter Monitor; Extrudieren abdocken, auf Monitor 2 ziehen, Werte ändern → Vorschau und Griffe im Hauptfenster; Esc/Enter im abgedockten Dialog; Baum abdocken, Entf/Strg+Z/S im abgedockten Baum; Skizze betreten mit abgedocktem Skizzenfeld; Neustart → alles an seiner Stelle; Neustart ohne zweiten Monitor → angedockt, mit Monitor wieder draußen; Monitor mit anderer Skalierung (125 %); Beenden mit ungespeicherter Änderung; Hell/Dunkel. WSLg (X11) Kurzprüfung.

## Dem Umsetzer überlassen

- Name und Form des Bausteins, wie der Schlüssel am Aufruf angegeben wird, genaue Symbolwahl.
- Wie Tastenereignisse technisch weitergereicht werden (Ereignisse in die Eingabe des Hauptfensters kopieren oder Verteiler mit der Eingabe des Fokusfensters aufrufen), solange „genau einmal“ gilt.
- Reihenfolge der Umstellung der Dialoge (zentral im Baustein; Einzelstellen nur für Esc/Enter außerhalb der Closure).
- Skizzen 9 ist seit 774d393 in `master` (Zweig steht darauf); die Einfüge-Dialoge (`insert/dxf_dialog.rs`, `svg_dialog.rs`, `canvas_dialog.rs`) bekommen ebenfalls Schlüssel.
