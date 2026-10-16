# Oberfläche im Graphit-Stil – Entwurf (Teilprojekt 13)

Stand: 2026-10-16. Entscheidungen des Nutzers sind als „Nutzer“ markiert, eigene Entscheidungen des Entwurfsverfassers als „Entscheidung (Entwurf)“. Die Vorschauen (Modell, Modell hell, Zeichnung, Skizze) liegen als HTML-Mockups unter `.superpowers/brainstorm/1791-1792144232/content/` (nicht im Repository, nur zum Ansehen).

## 1. Ziel und Hintergrund

PlanoraCAD (Rust, egui 0.36 mit wgpu, Paket `crates/planora-app`) soll durchgehend im Stil des Designsystems **„Graphit“** aussehen (Artifact `https://claude.ai/artifact/G6MHp9cwU2Gq5x5Q6Dov58`, Dateien `README.md`, `tokens.json`, `rust-egui.md`): ein fast einfarbig graues Programm, in dem der 3D-Bereich das ganze Fenster füllt und alle Bedienfelder als abgerundete Felder mit feinem Rand darüber schweben. Farbe kommt nur mit Bedeutung vor. Keine Schlagschatten, keine Verläufe.

Der Nutzer hat die Oberfläche bisher nur in Teilen gesehen, die Ideen sind „wie im Entwurf“. Was ihn am Ergebnis ärgern würde, weiß er erst, wenn er es sieht (Nutzer). Darum wird in Etappen gebaut, und nach jeder Etappe schaut er sich das laufende Programm an (Haltepunkte, siehe Abschnitt 6).

## 2. Entscheidungen

**Nutzer:**
- Das **ganze Programm** wird umgebaut, einschließlich des schwebenden Layouts (3D-Bereich über das ganze Fenster, Felder darüber).
- **Hell und Dunkel** bleiben wählbar. Die helle Variante ist ein Entwurf des Verfassers (Graphit gibt es nur dunkel) und wurde vom Nutzer in der Vorschau gut gefunden.
- Layouts von **Modell-Ansicht, Zeichnungs-Ansicht und Skizzen-Ansicht** wie in den drei Vorschauen (Abschnitt 4).
- **Dialoge:** schmaler und mittig schwebend wie im Graphit-Entwurf. Die bisherige Regel „großer Dialog über die volle Fensterbreite“ (`wide_window`) entfällt für die App-Dialoge. Weiter gilt: Karten nebeneinander sind gleich hoch; Texte in einfacher deutscher Laiensprache ohne technische Hinweise und ohne Hinweise auf die Datenherkunft.
- Das Zeichenblatt bleibt **weißes Papier** mit schwarzen Maßen auch im dunklen Programm (bestehende `theme::PAPER`, `SHEET_*`).

**Entscheidung (Entwurf):**
- **Schrift:** Montserrat Regular, Medium, SemiBold (Lizenz OFL) statt Roboto. Als eigene Familien eingebunden (`medium`, `semibold`), weil egui keinen Fettschnitt erzeugt.
- **Symbole:** dünne Linien-Symbole im Stil Phosphor Light (1,5 px Strich, runde Enden, 16 px in Zeilen, 22 px in Leisten). Einbindung als Symbolschrift mit eigener Codepunkt-Tabelle (Lizenz MIT), ohne neue Abhängigkeit, wenn die Schrift als Datei eingebettet werden kann; passt `egui-phosphor` zu egui 0.36 und besteht `cargo deny`, ist auch das Crate zulässig. Die handgezeichneten Körper-, Zeichnungs- und Maß-Symbole (`icons.rs`) werden an den gleichen Strich angeglichen und bleiben handgezeichnet.
- **Etappenweise** bauen und mergen (Abschnitt 6).
- **Menü:** bleibt in der Leiste oben mittig zusammen mit den Dokument-Tabs (siehe Vorschau). Es bekommt Symbole und getrennte Tastenkürzel-Spalte.
- **Schwebende Felder** sind feste Bereiche (Eigenschaften/Versionen als Umschalter, Baum links usw.). Verschieben und Größe ändern der Felder gehört **nicht** zu diesem Teilprojekt (Abschnitt 5).

## 3. Ziele, Nicht-Ziele, Anti-Ziele

**Ziele**
1. Jede Ansicht und jeder Dialog nutzt dieselben Farben, Schrift, Abstände, Rundungen und Symbole aus einer zentralen Stelle (`theme.rs`), in Hell und Dunkel.
2. Der 3D-Bereich füllt das ganze Fenster; Felder schweben darüber; das Modell erscheint beim „Alles zeigen“ in der **sichtbaren** Mitte (nicht hinter den Feldern).
3. Alle bisherigen Funktionen bleiben erhalten und verhalten sich gleich. Neuberechnung, Speichern, Export usw. werden nicht angefasst.
4. Nach jeder Etappe läuft das Programm vollständig und alle Tests sind grün.

**Nicht-Ziele**
- Keine neuen Funktionen (kein neuer Werkzeugtyp, keine neue Dateiformat-Version).
- Kein frei verschiebbares oder andockbares Feldsystem (Verschieben, Größe ändern, Feld ein- und ausblenden per Ziehen). Das heutige Ein-/Ausblenden von Projekten und Versionen bleibt als Menüpunkt unter „Ansicht“ erhalten.
- Kein Umbau des 3D-Renderers, der Kamera-Mathematik oder der Papier-/Blattdarstellung (nur Rand/Versatz für die sichtbare Mitte).
- Keine Animationen außer 120 ms Überblendung von Füllung und Farbe bei Hover/Aktiv (wie in Graphit).
- Keine Kopie des Dribbble-Konzepts, keine fremden Namen, Marken oder Symbolsätze außer den frei lizenzierten (Phosphor, Montserrat).

**Anti-Ziele (wäre ein Fehlschlag, auch wenn es technisch läuft)**
- Funktionen oder Kürzel gehen verloren oder sind versteckt.
- Text ist schlecht lesbar (Kontrast unter 4,5 : 1 für normalen Text in einem der beiden Themen; die in Graphit selbst markierten Ausnahmen `text-tertiary`, `text-muted`, `chip-split-fg` sind für echten Inhalt tabu).
- Felder verdecken das Modell oder das Blatt, so dass man nichts mehr anklicken kann.
- Zwei verschiedene Stile im selben Programm (halb umgebaut zum Etappenende).

## 4. Aussehen (Vorgaben aus Graphit, angewendet)

Quelle der Werte: `tokens.json` des Designsystems. Dunkel: `bg-viewport #0e0e0e`, `bg-panel #181818`, `bg-section #1a1a1a`, `bg-field #262626`, `bg-inset #0d0d0d`, `bg-active #333333`, `border-subtle #383838`, `text-primary #ffffff`, `text-secondary #939393`, Akzent `#006dfd` nur für „Modus eingeschaltet“, Tastaturfokus (2 px) und aktive Auswahl; Operationschips Bernstein/Blau/Grün; Achsen X rot `#e0303a`, Y grün `#64be62`, Z blau `#3d8bdb`; Auswahl im 3D-Bereich `#f1f05c`.
Abstände 4/8/12/16/24; Rundungen 4/6/10/14; Größen: Zeile 24, Feld 26, Steuerelement 32, Kopf 40, Leiste 48, Dock 56, Feldbreite 250.
Rand 1 px `border-subtle` auf jedem Feld, kein Schatten.

**Helle Variante (Entwurf des Verfassers, vom Nutzer gesehen):** Viewport `#e8e9ec`, Feld `#ffffff`, Abschnitt `#f4f4f6`, Eingabe `#eeeef1`, aktiv `#dcdce0`, Rand `#d4d4d8`, Text `#151515` / `#5f5f66`, gleicher Akzent `#006dfd`, helle Chips (Bernstein `#fbeed9`/`#8a5a13`, Blau `#dbe9fb`/`#1456b0`, Grün `#def3da`/`#2d7a22`), Auswahlgelb dunkler (`#d9a400`). Endgültige Werte werden bei der Umsetzung gegen den Kontrast 4,5 : 1 geprüft und gegebenenfalls leicht angepasst; Abweichungen werden dokumentiert.

**Modell-Ansicht (Vorschau „Modell“):**
- 3D-Bereich füllt das ganze Fenster (Fläche mit Bodenraster, grüne Bodenachse).
- Oben mittig ein Feld mit Menü, Trennstrich und Dokument-Tabs.
- Links ein Feld (Breite 250) mit den Abschnitten Projekte, Feature-Baum (mit der Rückroll-Markerzeile), Zeichnungen und Maße (heutige Browser-Abschnitte, jeweils einklappbar).
- Rechts oben ein Feld mit Modus-Schaltern, darunter ein Feld mit Umschalter Eigenschaften | Versionen.
- Unten mittig das Feld mit den Operationen als farbige Chips und den Werkzeugen als Symbolen (Vereinigen, Abziehen, Schneiden, Verschieben, Rundung und so weiter laut heutigen Menüs Erzeugen und Bearbeiten).
- Unten links die Meldung („1 Körper“, Hinweise, Fehler); Statusmeldungen mit Fehlerfarbe bleiben als eigene Felder erkennbar.
- Navigationswürfel rechts oben im 3D-Bereich, aber **links neben** dem rechten Feld, nicht dahinter.

**Zeichnungs-Ansicht (Vorschau „Zeichnung“):**
- Das Blatt (weiß) in der Mitte, Rahmen, Ansichten und Maße unverändert.
- Links Feld mit Zeichnungen (Ansichten) und Maßen; oben unter dem Menüfeld ein Werkzeugfeld (3D-Modell, Zeichnungsname, Ansicht hinzufügen, Maße vorschlagen, Maß-Werkzeug, Blatt einpassen, Zoom); rechts ein Feld „Ansicht …“ mit Maßstab, Schaltern für verdeckte Kanten und Mittellinien, „Ansicht löschen“; unten ein Hinweisfeld. Die heutige feste, immer zweizeilige Werkzeugleiste entfällt. Die Meldungen zu Maßen (verloren/außerhalb) bleiben im Hinweisfeld.

**Skizzen-Ansicht (Vorschau „Skizze“):**
- Links eine schmale senkrechte Leiste mit den Zeichenwerkzeugen als Symbole (Werkzeugnamen als Hinweistext beim Darüberfahren).
- Rechts ein Feld mit Abhängigkeiten, Maß-Eingabefeld, Liste und „Spiegeln und Muster“.
- Unten ein Feld mit Status („Vollständig bestimmt“ usw.) und „Skizze beenden“.
- Das heutige einzelne Fenster „Skizze“ (300 px, frei verschiebbar) entfällt.

**Dialoge:** mittig schwebend, Breite nach Inhalt (etwa 480–720 px, bei den bisher breiten Dialogen Exportieren, Netz importieren, Neue Zeichnung, Maß bearbeiten bis zu etwa 720 px), Feldrahmen wie `panel_frame()`, Titelzeile im Stil `heading`, Schließen-Symbol. Karten innerhalb eines Dialogs stehen weiter nebeneinander und sind gleich hoch. Rückfragen (`prompts`) bleiben mittig, schmal.

## 5. Einschränkungen und Hinweise für den Bauplan

Quelle: Bestandsaufnahme vom 2026-10-16 (GUI-Inventar, alle Pfade relativ zu `crates/planora-app/`).

**Heutiger Aufbau:**
- `src/app.rs`: Menüleiste (`Panel::top`), Tab-Leiste (`Panel::top`), Statuszeile (`Panel::bottom`), in der Mitte der egui_tiles-Baum (`src/dock.rs`) mit den Bereichen Projekte, Feature-Baum, 3D, Eigenschaften und Versionen (Anteile 14/16/46/24 %). Es gibt kein `Area` und kein `Modal`; alle Dialoge sind `egui::Window`.
- Der 3D-Bereich (`src/viewport.rs`) füllt sein Bereichs-`ui`; die Kamera setzt ihr Seitenverhältnis aus diesem Rechteck. Dasselbe Bereichs-`ui` zeigt je nach Arbeitsbereich 3D oder Zeichnung (`drawing_view.rs`, eigene zweizeilige Werkzeugleiste und Hinweiszeile im Bereich). Die Skizzen-Eingabe liegt als Overlay über dem 3D-Bereich.
- `src/theme.rs` (112 Zeilen) setzt Visuals für Dunkel/Hell. Papierfarben `PAPER`, `SHEET_INK`, `SHEET_FRAME`, `SHEET_PENDING`, `SHEET_DIM`, `SHEET_LOST`, `SHEET_SELECTED`, `PAPER_SHADOW` müssen unverändert bleiben (Tests importieren sie).
- `src/fonts.rs`: nur Roboto-Regular, keine Fallback- und keine Symbolschrift; Tests rufen `fonts::definitions()` selbst auf. `tests/fonts.rs` prüft, dass jedes Nicht-ASCII-Zeichen in `src/*.rs` in einer Erlaubnisliste steht, weil es in Roboto existiert; die Liste und die Glyph-Annahmen müssen für Montserrat und die Symbolschrift neu gebaut werden. Bekannte Lücken: U+23F4/23F5 (egui-Pfeile), „→“, „●“ (die Tab-Leiste zeichnet den Punkt selbst).
- Symbole: `src/icons.rs` zeichnet Körper-, Skizzen-, Verschieben-, Rundungs-, Zeichnungs-, Ansichts- und Maß-Symbole von Hand. Menüs, Werkzeugleisten, Skizzenwerkzeuge und Tabs haben bisher **keine** Symbole.
- Farben außerhalb von `theme.rs`: `sketch_view.rs` (eigene Hell/Dunkel-Farben), `viewport.rs` (Navigationswürfel), `sketch_panel.rs`, `browser.rs` (`ERROR_COLOR`, öffentlich und von Tests importiert; `MARKER_COLOR`), `mesh_body.rs`, `boolean_dialog.rs`, `drawing_view.rs`. Hartkodierte Größen u. a. `ROW_HEIGHT 28`, `PANEL_WIDTH 300`, `CUBE_SIZE 96`, `HINT_HEIGHT 24`, `DIALOG_TOP 60` und `DIALOG_MARGIN 16` (`export_dialog.rs`), `default_pos(330, 90)` (Skizzenfenster).
- Der Hell/Dunkel-Schalter liegt im Menü Ansicht und je Dokument-Tab in `state.dark` (`theme::apply` läuft nur bei Wechsel).

**Stolperstellen, die der Plan beachten muss:**
1. **Sichtbare Mitte im 3D-Bereich:** Läuft der 3D-Bereich hinter den Feldern, stimmen Seitenverhältnis, `frame_all`, Navigationswürfel-Lage und das Skizzen-Blickfeld nicht mehr mit dem unverdeckten Bereich überein. Kamera und „Alles zeigen“ brauchen einen Versatz (freie Fläche = Fenster minus Felder). Die Skizzen- und Zeichnungs-Eingaben dürfen nicht unter Feldern liegen.
2. **Eingabe-Routing:** Felder über dem 3D-Bereich fangen Zeigerereignisse nach Ebene ab (wie heute die Dialoge). Mausrad-Zoom, Auswahl und Ziehen im 3D- und Blattbereich dürfen nur reagieren, wenn der Zeiger nicht über einem Feld ist. Der Schutz gegen Tastenkürzel bei Texteingabe (`viewport::typing`) muss mit mehr Eingabefeldern weiter greifen.
3. **egui_tiles entfällt für das Hauptlayout** (Größenänderung, Verschieben der Tab-Köpfe, Trennbalken): `dock.rs`, `Dock::set_visible` und `tests/dock.rs` werden ersetzt. Umschalter Eigenschaften | Versionen und Ein-/Ausblenden von Projekten und Versionen (heute Menü Ansicht) müssen erhalten bleiben.
4. **Der Zeichnungsbereich** nutzt die Höhe seiner Werkzeugleiste für das Blatt (`toolbar.bottom()` bestimmt `canvas`). Mit schwebenden Feldern wird das Blatt zum Vollbild-Hintergrund, die Werkzeuge werden Felder; `DrawingLayout` und `last_layout` (von vielen Tests benutzt) ziehen mit. Die Papier-Anpassung („Blatt einpassen“) rechnet mit der freien Fläche.
5. **Tests prüfen Lagen:** `tests/drawing_ui.rs` (klickt Leistenelemente über Textpositionen, setzt zwei Leistenzeilen voraus), `dimension_view.rs`, `dimension_tool.rs`, `dimension_browser.rs`, `drawing_browser.rs`, `dimension_dialog.rs`, `tree_ui.rs` (Zeilenhöhe, `browser::row_rects`), `sketch_ui.rs`, `sketch_dialogs.rs` (Viewport-Rechteck, Würfel), die Export- und Import-Tests (Kartengrößen, `wide_window`, `DIALOG_TOP`, `DIALOG_MARGIN`), `tabs_ui.rs` (Text „Unbenannt 1 ●“). Nicht getestet ist `app.rs` selbst (Menüs, Felder, Statuszeile): Hier braucht der Umbau neue Tests, soweit machbar (Feldbereiche, Menüeinträge, Ein-/Ausblenden), und die Handprüfung (`tools/gui-drive.ps1` gehört zum bestehenden Handprüfweg).
6. **Schrift:** Montserrat braucht mehrere Familien. Das Prüfwerkzeug für Nicht-ASCII-Zeichen in Quelltext (`tests/fonts.rs`) und die Annahmen in `sketch_view::symbol` (`has_glyphs`) und `tabs.rs` neu prüfen; Glyphen, die Montserrat nicht hat (zum Beispiel U+23F4/23F5, „→“, „∥“, „●“), müssen durch Symbole oder gezeichnete Formen ersetzt werden.
7. **Lizenzen:** Montserrat (OFL) und Phosphor (MIT) sind neue Inhalte: Lizenztexte im Repository ablegen, `deny.toml`/Lizenzliste anpassen, `cargo deny check` muss grün bleiben (Regel des Nutzers: Lizenzen immer prüfen, kein Autodesk-Material). Das Designsystem selbst nennt als Vorlage ein Dribbble-Konzept; nichts davon wird übernommen außer Stil und freien Schriften/Symbolen.
8. **Farbwelten zusammenführen:** Skizzenfarben (`sketch_view::colors`), Würfel, Fehler-/Warnfarben (`ERROR_COLOR` bleibt als Name erhalten, weil Tests ihn importieren), Marker und Papierfarben bekommen eine gemeinsame Palette in `theme.rs`. Die Skizzengeometrie hat eigene gut sichtbare Farben und kann im Graphit-Look mit Akzentblau/Hellblau getönt werden, bleibt aber lesbar.
9. **Fenster-Sonderheiten:** Rückfragefenster verwerfen den ersten Größen-Durchgang (Flackern vermeiden); `wide_window`-Dialoge brauchen eine neue Breitenregel (Abschnitt 4); `Window::open` liefert heute das Schließen-Kreuz im Titel.
10. **Kontrast prüfen**, in Hell und Dunkel, für jedes Paar aus Text und Untergrund; Graphit nennt selbst drei Ausnahmen (Abschnitt 3).

## 6. Etappen und Haltepunkte

Jede Etappe ist für sich lauffähig, alle Tests grün (`cargo test --workspace -j 2`), `cargo deny check` grün, Proben/Handprüfung durch den Nutzer am Haltepunkt. Ein Merge pro Etappe nach Rückfrage beim Nutzer.

1. **Etappe A – Farben, Schrift, Symbole, Formen** (kein neues Layout): zentrales Theme mit allen Werten in Hell und Dunkel; Montserrat; Symbolschrift und angeglichene Handsymbole; Abstände, Rundungen, Feldrahmen-Helfer (`panel_frame()` und Zeilen-/Feld-Helfer); alle bestehenden Fenster und Panels nutzen sie; Menüs und Werkzeugzeilen bekommen Symbole; Farben aus Skizze, Würfel, Fehler/Warnung zusammengeführt. **Haltepunkt 1:** Nutzer sieht das ganze Programm im neuen Stil, in Hell und Dunkel, noch mit dem alten Aufbau.
2. **Etappe B – schwebendes Layout der Modell-Ansicht:** Menü + Tabs als oberes Feld, linkes Baumfeld, rechte Felder, unteres Befehlsfeld, Meldungsfeld; 3D-Bereich im Vollbild mit sichtbarer Mitte; Eingabe-Routing; Ein-/Ausblenden bleibt; `dock.rs` und `tests/dock.rs` ersetzt. **Haltepunkt 2:** Nutzer arbeitet im Modell-Tab.
3. **Etappe C – Zeichnung und Skizze:** Felder wie in den Vorschauen; Blatt als Vollbild; Skizzenwerkzeug-Leiste, Skizzenfeld rechts, Status unten; Tests an die neuen Lagen angepasst. **Haltepunkt 3:** Nutzer zeichnet und skizziert.
4. **Etappe D – Dialoge:** alle Dialoge im neuen Rahmen, neue Breitenregel, Karten gleich hoch, Rückfragen; Reste aufräumen (`wide_window`, `DIALOG_TOP`, tote Konstanten). **Haltepunkt 4 (Abschluss):** Nutzer geht alle Dialoge durch; Dokumentation (README, `offene-punkte.md`) und Gesamtprüfung.

Nach jeder Etappe schreibt der Nutzer, was ihn stört; das wird in der folgenden Etappe (oder als Nachbesserung) behoben, ohne den Plan neu zu schreiben.

## 7. Dem Bauplan überlassen

- Reihenfolge der Arbeitsschritte innerhalb einer Etappe, Aufteilung in Aufgaben, Namen und Dateiaufteilung (zum Beispiel ein eigenes Modul für Feld-Helfer und Symbol-Tabelle).
- Genaue Pixelmaße der schwebenden Felder (Vorschau gibt Größenordnung: Seitenfelder 210–250 px, Rand 16–24 px), Mindestfenstergröße und Verhalten bei sehr kleinem Fenster (Felder dürfen in diesem Fall schmaler werden oder einklappen, ohne dass etwas verloren geht).
- Wahl zwischen eingebetteter Phosphor-Schriftdatei und dem Crate (Abschnitt 2), sofern Lizenz und egui-Version es zulassen.
- Technik der sichtbaren Mitte (Kamera-Versatz, Hilfsrechteck) und des Eingabe-Routings.
- Welche Tests wie angepasst werden; neue Tests für Hell/Dunkel-Kontrast und für die Feld-Bereiche; Handprüfung nur nach Rückfrage beim Nutzer und mit dem Nutzer, der Dateien selbst öffnet.
- Texte: einfache deutsche Laiensprache; bestehende Beschriftungen bleiben, wo sie passen.

## 8. Kennzahlen zum Umfang (zur Einschätzung)

UI-Quelltext in `crates/planora-app/src` (ohne reine Zustandsdateien): Shell und Docking ≈ 930 Zeilen, 3D-Bereich und 2D-Überlagerungen ≈ 3250 (`drawing_view.rs` allein 2204), Panels ≈ 3150, Dialoge ≈ 4600, Symbole 261. Tests des Pakets machen etwa die Hälfte von rund 40 000 Zeilen aus; etwa ein Dutzend Testdateien hängt an Lagen und Rechtecken (Abschnitt 5).

## 9. Zusätzliche Festlegungen (Entwurf, nach der Bauherren-Prüfung)

Diese Punkte entscheidet der Entwurfsverfasser (nicht der Nutzer); der Nutzer kann sie beim Lesen kippen.

**F1 – Maßgebliche Quellen für das Aussehen.** Die Vorschauen und die Graphit-Dateien liegen im Repository unter `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-vorschau/`: `modell-graphit.html` (dunkel), `modell-graphit-hell.html`, `zeichnung-graphit.html`, `skizze-graphit.html` (reine HTML-Skizzen, Platzhalter-Kästchen statt Symbole) sowie `graphit-tokens.json`, `graphit-rust-egui.md`, `graphit-README.md` (Kopien aus dem Designsystem-Artifact „Graphit“ vom 2026-10-16). Bei Widersprüchen gilt: dieser Entwurf vor den Vorschauen, die Vorschauen vor dem Designsystem. Für Zustände (Ruhe, Hover, Aktiv, Fokus, Gesperrt) gilt die Zustandstabelle in `graphit-README.md`, Schriftgrößen und -stärken je Textstil stehen in `graphit-tokens.json` (`heading` 13/600, `label` 13/500, `body` 12/400, `value` 11/500, `kbd` 10/600, `axis` 15/500). Werte, die dort fehlen (zum Beispiel Hover und Fokusring in Hell, Chip-Farben in Hell über die drei genannten hinaus), erfindet der Bauplan nach den Regeln „grau zuerst, Farbe nur mit Bedeutung“ und prüft sie gegen den Kontrast 4,5 : 1.

**F2 – Sichtbare Mitte auch in Zeichnung und Skizze.** Das Blatt und die Skizzenebene dürfen unter den schwebenden Feldern weiterlaufen (Vollbild-Hintergrund), „Blatt einpassen“, „Alles zeigen“ und das Einpassen der Skizze richten sich aber nach der **freien Fläche** (Fenster minus Felder). Eingaben (Klick, Ziehen, Mausrad) gehen nur an Blatt, Skizze oder 3D-Bereich, wenn der Zeiger nicht über einem Feld ist.

**F3 – Kamera-Versatz erlaubt.** Die Verschiebung zur sichtbaren Mitte darf in der Kamera als Versatz der Bildmitte (oder über Ziel und Abstand) umgesetzt werden, solange Auswahl-Strahlen, Navigationswürfel und Skizzenblick dazu passen. Die Kamera-Mathematik selbst (Projektion, Bahnen, Genauigkeit) wird nicht umgebaut; ein bloßer Bildmitte-Versatz gilt nicht als Umbau.

**F4 – Tests dürfen umgeschrieben werden.** Erwartete Lagen, Zeilenhöhen und Beschriftungen in den lagenabhängigen Tests (Abschnitt 5, Punkt 5) dürfen mit dem Layout wandern; geprüft wird weiter dasselbe Verhalten. Es müssen keine Kompatibilitäts-Hüllen für `DrawingLayout`, `browser::row_rects` oder `ROW_HEIGHT` bestehen bleiben. Der Stand „alle Tests grün“ gilt unverändert nach jeder Etappe (`cargo test --workspace -j 2`).

**F5 – Etappe A darf Übergangs-Kompromisse haben.** Die Größen (`ROW_HEIGHT 28`, Fensterbreiten, 300-px-Skizzenfenster) dürfen in Etappe A auf die Graphit-Maße angepasst werden. Der alte Aufbau (egui_tiles, zweizeilige Zeichnungsleiste, einzelnes Skizzenfenster) bleibt bis Etappe B/C bestehen, sieht aber sauber aus: nichts läuft über, nichts ist abgeschnitten. Keine Wegwerf-Arbeit, die nicht schon der Zielzustand ist (zentrales Theme, Helfer, Schrift, Symbole).

**F6 – Fehlende Zeichen.** Wo Montserrat oder die Symbolschrift ein Zeichen nicht hat, wird es durch ein Symbol, eine gezeichnete Form oder ein deutsches Wort ersetzt (zum Beispiel „→“ durch ein Pfeil-Symbol, „∥“ durch ein Parallel-Symbol, „●“ durch den gezeichneten Punkt). Vom Nutzer tippbare und angezeigte Zeichen müssen in der Schrift vorhanden sein: deutsche Umlaute und ß, „°“, „±“, „×“, „Ø“, „−“, „²“, „³“, Anführungszeichen „“ und „…“; fehlt eines in Montserrat, wird eine Fallback-Schrift (Roboto als zweite Schrift in der Familie) oder ein Symbol eingebunden, und `tests/fonts.rs` wird entsprechend neu gebaut.

**F7 – Hell und Dunkel bleibt je Dokument-Tab** (wie heute, `state.dark`). Beim Tab-Wechsel wechselt das ganze Programm mit (die schwebenden Felder gehören zum aktiven Tab).

**F8 – Offene Gestaltung der Felder-Inhalte.** Genauer Inhalt des unteren Befehlsfelds (Chips und Werkzeug-Symbole) folgt den heutigen Menüs Erzeugen und Bearbeiten, wobei jeder Eintrag erreichbar bleibt (das Menü bleibt vollständig); das untere Feld ist eine Abkürzung, kein Ersatz. Der Modusschalter oben rechts bleibt in Etappe B leer oder zeigt nur, was es heute schon gibt (Anzeige-Modi); neue Modi werden nicht erfunden.
