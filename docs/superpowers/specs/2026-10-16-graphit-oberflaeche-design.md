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

## 10. Festlegungen der Umsetzung Etappe A

Stand: 2026-10-16, Zweig `feature/graphit-etappe-a`. Plan: `plans/2026-10-16-graphit-etappe-a.md`. Hier steht, was bei der Umsetzung anders wurde oder entschieden werden musste.

**Entscheidungen (Rulings)**
- **Kontrast Nebentext auf aktivem Grund.** Graphit selbst führt `text-secondary #939393` auf `bg-active #333333` mit etwa 3,9 : 1. Der Kontrasttest verlangt deshalb `text_primary` auf allen Flächen mit 4,5 : 1, `text_secondary` auf Panel, Abschnitt, Feld und Eingabe mit 4,5 : 1, auf `bg_active` nur 3,0 : 1. Aktive Elemente zeigen ihre Beschriftung in `text_primary`.
- **Schrift.** Montserrat Regular, Medium und SemiBold (OFL 1.1) liegen mit Lizenztext in `crates/planora-app/assets/fonts/`, ebenso Phosphor Light (MIT) mit `style.css` als Quelle der Codepunkte. Roboto bleibt als zweite Schrift jeder Familie (Absicherung; Montserrat hat alle erlaubten Zeichen schon selbst). `skrifa` 0.44 ist Dev-Abhängigkeit des Glyphentests (steht über `epaint` schon im `Cargo.lock`; die Glyphenprüfung von egui 0.36.2 war unzuverlässig).
- **Symbol-Codepunkte** stehen als `\u{..}`-Escapes im Quelltext (der Wächtertest gegen Nicht-ASCII-Zeichen bleibt gültig).
- **Skizzenfenster bleibt 300 px breit** (F5 erlaubte 250 px nur, wenn der Inhalt passt; er passt nicht).
- **Zeichnungsleiste** bricht im Bereich um 640 px Breite schon seit vorher auf mehr als zwei Zeilen um. Das bleibt; es ist ein Layout-Thema für Etappe C.
- **Schalter ohne Rand.** Werkzeug- und Symbolschalter, Tabs und Menüknöpfe sind randlos; den Tastaturfokus zeigt `theme::focus_ring` (2 px Akzent).

**Abweichungen in der hellen Palette** (Kontrast unter 4,5 : 1 bzw. Unterscheidbarkeit, Spec §4 war ein Entwurf)
- `bg_active` hell `#cfd0d5` statt `#dcdce0` (Hover und Aktiv müssen sich unterscheiden), `bg_field` hell `#dadbdf`.
- `text_tertiary` hell `#66666e`; Achsenfarben und Fehlerrot (`#c62828`) hell dunkler als im Entwurf.
- Warnung `#8c5200`, Erfolg `#2a7020`, Markierung `#1a5cc4` hell dunkler: der Kontrasttest prüft auch `bg_viewport` `#e8e9ec` (zugleich `panel_fill`; Menü-, Tab- und Statusleiste stehen darauf) und verlangt dort 4,5 : 1. Der dunkle Graphit-Wert `text_tertiary` `#7d7d7d` bleibt davon ausgenommen (4,2 bis 4,7 : 1).
- `disabled_alpha` 0,75 statt 0,5: gesperrter Nebentext mindestens 3 : 1 auf Fenster, Feld, Abschnitt und Eingabe in beiden Themen.
- Skizzen-Auswahl hell `#a86a00` statt `#d9a400`, `axis_y` hell `#36903a`.
- `ground_axis` hell bleibt unter 3 : 1 (2,45 : 1; es ist eine Linie, kein Text).
- `drawing_surround` hell nutzt `bg_active` (geliehene Bedeutung).

**Rundungen der Umsetzung**
- `SHEET_SELECTED` bleibt `#1f6fe0` (Papier), nur das Programm-`accent` wechselt auf `#006dfd`.
- Fehlertext und Rahmen einer fehlgeschlagenen Ansicht auf dem weißen Blatt nutzen in beiden Themen `SHEET_LOST` (Test `drawing_ui`).
- Die 3D-Szene nimmt Raster, Achsen und Auswahl aus der Palette (`viewport::render_palette`); Flächen, Kanten und Hover bleiben bei `planora_render` (Etappe B).
- `animation_time` 120 ms.
- Plain-Labels sind grau (`text_secondary`), Werte und Titel stark; Hyperlinks haben `text_primary`.
- Buttons sind im Ruhezustand ohne Fläche, mit 1 px `border_subtle` an Dialogknöpfen; Hover `text_tertiary`.

**Öffentliche Schnittstelle (mehr als im Plan)**
- `theme`: `window`, `section_header`, `SUB_HEADER_H`, `value`, `field`, `Toggle`, `menu_style`, `frameless`, `heading_font`, `label_font`, `value_font`, `kbd_font`, `text_field`, `text_field_enabled`, `focus_ring`, `entry`, `DISABLED_ALPHA`, `ANIMATION_TIME`.
- `viewport::render_palette`, `sketch_view::overlay_text` (für Tests).
- `icons`: `atom`, `button`, `selectable`, `source_entry`, `IconSource`, `sketch_tool_icon`, `sketch_action_icon`, `source_button`, `label`, `label_colored`, `stroke_color`; 59 Phosphor-Symbole (`Icon`) und 8 selbst gezeichnete (`HandIcon`).
- `app`: `file_menu`, `menu_bar` (`doc(hidden)`).
Vieles davon wäre `pub(crate)`; es ist `pub`, weil Tests und mehrere Module es brauchen. Aufräumen in Etappe D.

**Wächtertests**
- `no_raw_text_edits` (alle Einzeilen-Textfelder über `theme::text_field`) und der Test gegen hartkodierte Farben prüfen nur `src/*.rs` der obersten Ebene (es gibt dort keine Unterordner). Mehrzeilige Textfelder prüft keiner (es gibt keine). Der Selbsttest des Farb-Scanners deckt nicht alle Konstruktoren ab.
- Die Layout-Tests der meisten Testgeräte setzen nur die Schrift, nicht das Theme; Überlauf durch die neuen Abstände ist daher nur durch den Näherungstest `no_text_is_clipped_at_1400px` (Summe der Textbreiten) und die Handliste abgedeckt.

**Bewusst offen gelassen** (Einzelheiten in `offene-punkte.md`): Symbolwahl für Symmetrisch, Muster, Kante projizieren, Deckungsgleich, Kegel, Kugel, Drehen, Ansicht, „zum Ende springen“ nur ungefähr; Browser-Kontextmenüs ohne Symbole; Dialog-Checkboxen unverändert; Fokusring am Menürand evtl. beschnitten.

## 11. Festlegungen der Umsetzung Etappe B

Stand: 2026-10-16, Zweig `feature/graphit-etappe-b`. Plan: `plans/2026-10-16-graphit-etappe-b.md`. Hier steht, was bei der Umsetzung entschieden wurde oder anders kam.

**Festlegungen B1–B8** (Entwurfsverfasser, der Nutzer kann sie kippen)
- **B1** Rand 16 px (`SHELL_INSET`), Abstand zwischen Feldern 12 px (`SPACE_3`). Graphit nennt 24 px Rand; 16 spart Platz, die Vorschauen nutzen 16.
- **B2** Die freie Fläche wird deterministisch aus Konstanten berechnet (kein Messen der Feldgrößen, keine Rückkopplung): links und rechts je `SHELL_INSET + PANEL_WIDTH + SPACE_3`, wenn dort ein Feld sichtbar ist, oben `SHELL_INSET + TOP_H + SPACE_3`, unten `SHELL_INSET + DOCK_H + SPACE_3`.
- **B3** Positionen aus `ctx.content_rect()` und diesen Konstanten (`fixed_pos`), Höhen der Seitenfelder aus der freien Fläche. Felder mit unbekannter Breite laufen mit `fade_in(false)` und verwerfen den ersten Größen-Durchgang (`request_discard`, wie `prompts::window`).
- **B4** Felder `Order::Middle` und `movable(false)`; `theme::window` (alle Dialoge und das Skizzenfenster) `Order::Foreground`.
- **B5** Zeiger-Routing über `response.hovered()`, `hover_pos()` und `ui.rect_contains_pointer`; `ctx.is_pointer_over_egui()` wird nicht benutzt (taugt mit Area-Feldern nicht).
- **B6** Der Modusschalter oben rechts der Vorschau entfällt (es gibt keine Modi dafür, F8: nichts erfinden). Das Befehlsfeld unten ist nur eine Abkürzung zu vorhandenen Menüaktionen.
- **B7** Der Hinweis „Alte Version vN – schreibgeschützt …“ steht als eigenes Feld unter dem oberen Feld (Farbe `warning`).
- **B8** Die Zeichnungsansicht bleibt in Etappe B im freien Rechteck mit ihrer alten Werkzeugleiste (Umbau in Etappe C) auf einem Hintergrund `theme::viewport_background` über das ganze Fenster. Das Skizzenfenster bleibt ein verschiebbares Fenster (300 px), Startposition rechts neben dem linken Feld.

**Entscheidungen (Rulings)**
- **Kamera.** `ray()` rechnet die Inverse in f64, weil 1e-4 mm mit f32 nicht erreichbar ist (1,6e-4 mm ohne, 1,1e-3 mm mit Versatz); `view_proj` und `project` bleiben ohne Versatz bitgleich. Der Versatz ist ein NDC-Versatz in `projection()`, Standard 0 (F3). `set_visible` wird pro Bild nach `set_viewport` aufgerufen; `restore_camera` übernimmt Versatz, `visible_aspect` und Anteil der aktuellen Kamera.
- **Menü.** `menu_bar_shell` wurde zunächst neben dem alten `menu_bar` angelegt und in Aufgabe 6 zu `menu_bar` umbenannt (der Plan hatte die Signaturänderung erst dort vorgesehen).
- **Befehlsfeld.** Wirkt im Skizzenmodus wie das Menü (die Skizze wird beendet, der Dialog öffnet sich) und wird nicht gesperrt. Es verschwindet nie (Mindestbreite etwa 250 px). Das Meldungsfeld bekommt den Rest der Breite und umbricht in bis zu 3 Zeilen (`STATUS_ROWS`). Chips wählen die Operation im Booleschen Dialog vor.
- **Hinweisfeld.** Volle Breite zwischen den Spalten, bis zu zwei Zeilen, `BANNER_H` 40; bei schreibgeschützter Ansicht hebt es die Oberkante der freien Fläche um `BANNER_H + SPACE_3` an (auch Zeichnung und Skizze, F2).
- **Mindestfenstergröße** 800 × 600 (`with_min_inner_size`); darunter überlappen Felder (bekannte Grenze, unter etwa 640 px Breite).
- **Kein Tastatur-Zoom** der Oberfläche (`PlanoraApp::setup_context` schaltet `zoom_with_keyboard` ab), weil die Felder in Punkten auf 800 × 600 ausgelegt sind; sonst hebelt Strg und Plus die Mindestgröße aus.
- **Oberes Feld und Spalten (`shell::layout_with_top_min`).** Das obere Feld meldet seine Mindestbreite (Menü, Trennstrich, 120 px Tabs, Rand, rund 431 px). Passt sie nicht zwischen die Seitenspalten (unter etwa 990 px Breite, also auch bei 800 × 600), steht das obere Feld mittig über den Spalten, und die Spalten beginnen darunter (`GAP` unter dem oberen Feld). Sonst gilt die Breite zwischen den Spalten wie bisher. `shell::layout` ist derselbe Aufruf mit Mindestbreite 0.
- **Seitenfelder.** `shell_panels::side_column` zeichnet den Inhalt in einer Fläche fester Breite (`PANEL_WIDTH` minus Rahmen), die nicht ins Feld zurückwirkt; die Felder sind immer genau 250 px breit. Eigenschaften stehen in Zeilen „Bezeichnung – Wert“ (Wert rechtsbündig daneben, sonst darunter und umbrochen). Lage und Drehung: `properties::POSITION_LABEL` („Position x / y / z in mm“) und `properties::ROTATION_LABEL` („Drehung x / y / z in Grad“) mit drei Zahlenfeldern, die sich die Breite teilen; die Position steht beim Körper nach „Flächen / Kanten“.
- **Rollbalken.** `theme::scroll_style` (schwebend, in Ruhe unsichtbar, 6 px, beim Griff 8 px), `theme::scroll_area` (färbt den Griff) und `theme::scroll_handle_colors` (Farben je Thema) für alle Rollbereiche der Felder.
- **Einpassen.** Geladene Stände und neue Tabs werden einmal in die freie Fläche eingepasst (`AppState::fit_pending`, gesetzt in `from_snapshot` und `Workspace::new_tab`, nachgeholt im ersten Modellbild ohne Skizze).
- **Schreibschutz** wie vorher (Baum und Eigenschaften gesperrt, Projekte und Versionen nicht, Zeichnung `editable = !read_only`).
- **Texte.** „Projekt-Panel“ in den Aufforderungen heißt jetzt „Feld Projekte“.

**Abweichungen vom Plan**
- Innenbreite eines Seitenfelds = Außenbreite − `frame.total_margin()` (Innenrand und 1-px-Strich), damit die Außenbreite genau 250 ist.
- Das Meldungsfeld ist nicht auf 250 px begrenzt, sondern reicht bis zum Befehlsfeld; die Breite wird aus der Textbreite gerechnet, nicht gemessen.
- Das Befehlsfeld lässt dem Meldungsfeld Platz für bis zu 3 Zeilen und wird dafür schmaler (siehe oben); der Plan ging von einem Befehlsfeld fester Breite aus.
- Die Überschriften Eigenschaften und Versionen stehen mit Umschalter nur einmal (`properties::show_content`, `versions_panel::show_with_header`).
- Die Tab-Leiste rollt zum aktiven Tab (`tabs::show_revealing`), sonst bliebe er bei vielen Tabs unsichtbar.

**Öffentliche Schnittstelle (mehr als im Plan)**: `#[doc(hidden)] pub` Testhaken `PlanoraApp::{for_tests, ws_mut, shell_mut}`, `shell_panels::{last_layout, field_rects, bottom_buttons, discard}`; `theme::{field_area, chip_fill, scroll_style, scroll_area, scroll_handle_colors}`, `viewport::{show_in, cube_rect}`, `properties::{POSITION_LABEL, ROTATION_LABEL}`, `PlanoraApp::setup_context`. Aufräumen in Etappe D.

**Bewusst offen gelassen** (Einzelheiten in `offene-punkte.md`, Teilprojekt 13 Etappe B): Fenster unter 640 px, Platzierung des Skizzenfensters (es liegt bei Breiten unter etwa 1000 px über dem Navigationswürfel; Etappe C), Verrunden und Fase mit gleichem Symbol, `layout()` ohne `ShellState`, linke Spalte mit fester Meldungshöhe, erstes Einpassen bei Skizze vor dem ersten Modellbild.

## 12. Festlegungen der Umsetzung Etappe C

Stand: 2026-10-16, Zweig `feature/graphit-etappe-c`. Plan: `plans/2026-10-16-graphit-etappe-c.md`. Hier steht, was bei der Umsetzung entschieden wurde oder anders kam. Einzelheiten und Grenzen: `offene-punkte.md`, Teilprojekt 13 Etappe C.

**Festlegungen C1–C10** (Entwurfsverfasser, der Nutzer kann sie kippen; C1 ist eine Nutzer-Entscheidung)
- **C1 (Nutzer)** Navigationswürfel: 26 anklickbare Bereiche (6 Flächen, 12 Kanten, 8 Ecken), Home-Knopf, aktuelle Ansicht blau hervorgehoben (nur wenn die Blickrichtung auf höchstens 2° zu einem der 26 Bereiche passt, sonst nichts), Graphit-Achsenfarben leicht getönt (Rechts/Links = X, Vorne/Hinten = Y, Oben/Unten = Z), Größe 84 px, Beschriftung auf der Fläche (verzerrt und mitgedreht), Name der aktuellen Ansicht klein unter dem Würfel.
- **C2** Home = Isometrisch und „Alles einpassen“.
- **C3** Klick auf eine Fläche = Ansicht auf diese Fläche (`set_view`); Kante = Blick entlang der Kantennormalen (45° zwischen den beiden Flächen); Ecke = Blick auf die Ecke (Isometrie-Winkel, Pitch ±35,264°, Yaw 45° + k·90°).
- **C4** Beschriftungsausrichtung: Seitenflächen „oben“ = +Z; Oben-Fläche „oben“ = +Y (nach hinten); Unten-Fläche „oben“ = −Y; „rechts“ = oben × Normale (nie gespiegelt).
- **C5** Skizzenmodus: linke senkrechte Werkzeugleiste (9 Symbole, vertikal mittig, 44 px) ersetzt das linke Feld; das rechte Feld ersetzt Eigenschaften/Versionen; das Fußfeld ersetzt das Befehlsfeld; das Menü Erzeugen/Bearbeiten bleibt vollständig und beendet die Skizze wie bisher.
- **C6** Zeichnungsmodus: Werkzeugfeld (`TOOL_H` 76) mittig unter dem oberen Feld (verschiebt `free.top`); linke Spalte unverändert; rechtes Feld „Ansicht „Name““ ersetzt Eigenschaften/Versionen; Hinweisfeld ersetzt das Befehlsfeld; das Statusfeld bleibt unten links.
- **C7** `drawing_view::show_with` und `sketch_panel::show` zeichnen die Modusfelder selbst; liegt kein Shell-Layout im `ctx`, rechnen sie die Rechtecke mit `shell::layout_in_mode` aus `ctx.content_rect()`.
- **C8** Das Befehlsfeld erscheint nur im Modus Modell.
- **C9** Das Skizzenfenster entfällt vollständig (erledigt „Skizzenfenster überdeckt Würfel/Status“ aus Etappe B).
- **C10** Fase bekommt ein eigenes Symbol (gerade Schräge); Verrunden behält `Blend`.

**Navigationswürfel, Tatsachen**
- 26 Bereiche: `CubeRegion::{Face, Edge, Corner}`, `all_regions`, `cube_pieces` (nach hinten sortierte Stücke) und `cube_pieces_visible(camera, min_facing)`; den Filter auf endliche Werte und auf deutlich zugewandte Stücke macht `viewport::visible_pieces`, `pieces_hit_test`. Namen deutsch („Vorne“, „Oben-Rechts“, „Oben-Vorne-Rechts“).
- Home = `OrbitCamera::home()` (Isometrisch) plus `frame_all`; die Isometrie ist die Ecke Oben-Vorne-Rechts.
- Hervorhebung: `current_region(camera, 2.0)`; nahe den Polen gilt `|pitch| ≥ MAX_PITCH − 1e-4` als genau senkrecht (die Kamera stoppt 0,573° davor).
- Beschriftungsausrichtung nach C4 (`face_text_frame`); der Text wird als Galley-Netz auf die Fläche abgebildet. Stücke erscheinen ab `facing` 0,15 (`PIECE_MIN_FACING`) und blenden bis 0,3 ein; die Beschriftung blendet zwischen 0,3 (`LABEL_MIN_FACING`) und 0,5 (`LABEL_FULL_FACING`) ein.
- Zum Einpassen kamen `OrbitCamera::frame_points` (Punkte in die freie Fläche einpassen) und `AppState::refit_sketch_view` (neue Skizze mittig und herangezoomt nachpassen) hinzu.
- Die Kamera-Mathematik ist nicht umgebaut; `planora-render` hat nur Zusätze (`ViewFace`, `cube_faces`, `cube_hit_test`, `set_view` und ihre Tests bleiben).

**Entscheidungen (Rulings)**
- Home links neben dem Würfel (oben bündig, 2 px Überlapp), Hover-Farbe `cube_hover_fill`, Hover der aktuellen Ansicht `highlight_hover`; Würfel und Home mit `Sense::click_and_drag`.
- Das Fußfeld der Skizze darf breiter sein als der Platz zwischen den Spalten (Status schrumpft), sonst überläuft er bei 800 × 600.
- Schalter „Waagerecht/senkrecht erkennen“ (vorher „Automatisch waagerecht/senkrecht“) zweizeilig, Hover-Text `AUTO_AXIS_HINT`.
- Löschen in der Abhängigkeitsliste als Symbolknopf; Maßeingabe gestapelt; Zahlenfeld mit Fokus, Esc/Enter/Entf wirken nicht auf die Skizze, solange es Fokus hat.
- Feldrand-Hover-Widget in `shell_panels::field` für alle Felder (Mausrad am Feldrand erreicht die Fläche darunter nicht mehr).
- Zeichnung: `DrawingLayout.canvas` = `free` (Einpassen, Zentrieren), `paint` = Fenster (Blatt, Umgebung, Eingabe). Werkzeugfeld: Zeile 1 „3D-Modell“, „Zeichnung“, Trenner, „Ansicht hinzufügen“; Zeile 2 „Blatt einpassen“, −, +, Trenner, „Maße vorschlagen“, „Maß“, Modus; seitlich rollbar. Schreibgeschützt: `free.top` +52 (Hinweisfeld + Abstand).
- Hinweisfeld höchstens zwei Zeilen; Maßhinweise werden gekürzt und zeigen beim Überfahren den vollen Text.
- Test-Fenster der Maßtests 1600 × 1000 (vorher 1200 × 800), weil das Einpassen jetzt die kleinere freie Fläche nutzt.

**Abweichungen vom Plan**
- Werkzeugfeld-Zeilen anders gruppiert als im Brief (die alte erste Zeile passte bei 1100 × 700 nicht).
- Schreibgeschützte Zeichnung: +52 statt +40.
- Fußfeld in der Skizze breiter als „zwischen den Spalten“.
- Schalter umbenannt und zweizeilig; Löschen als Symbolknopf; „anzeigend“ in zweiter Zeile.
- Feldrand-Hover-Widget für alle Felder (nicht nur die neuen); im Plan nicht vorgesehen.
- Das Werkzeugfeld rollt seitlich mit sichtbarem Balken bei Überlauf (Nachbesserung nach der Prüfung).
- Fase-Test prüft zusätzlich den Quelltext (brüchig, siehe `offene-punkte.md`).

**Patent- und Namenshinweis zum Würfel**
Der Navigationswürfel ist eine eigene Gestaltung unter dem Namen „Navigationswürfel“; die Marke „ViewCube“ wird nicht verwendet; zum Autodesk-Patent US 7,782,319 (angemeldet 2007) wurde keine Prüfung der Ansprüche vorgenommen; vor einer kommerziellen Nutzung Rechtsberatung einholen.

**Bewusst offen gelassen** (Einzelheiten in `offene-punkte.md`): Beschriftungsgröße, Hover-Kontrast der aktuellen Ansicht (3,8 : 1), Kontrast von Kanten und Ecken im Dunkeln, Werkzeugfeld in voller Breite, Ein-Bild-Wechsel zwischen den Modi, doppelter langer Skizzenhinweis. Etappe D: Dialoge, Aufräumen der `pub`-Fläche.
