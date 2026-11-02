# Skizzen 2: Eingabe beim Zeichnen, Fang, Raster, Auswahl – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Maßhaltig zeichnen wie in Fusion: Felder am Zeiger halten getippte Werte fest und machen daraus Maße, der Zeiger rastet an Geometrie, Ausrichtungslinien und Raster ein, passende Abhängigkeiten entstehen sichtbar von selbst; dazu Rahmenauswahl, Kettenwahl, Ziehen ganzer Kurven und „Blick auf“.

**Architecture:** `planora-sketch` bekommt einen je Skizzenstand gebauten Fang-Index (`snap.rs`). In der App bestimmt eine reine Funktion ohne egui je Bild und noch einmal beim Klick den „aufgelösten Zeiger“ (`DrawCursor`: Lage, Fangart, Hilfslinien, geplante Abhängigkeiten), sodass genau das entsteht, was als Zeichen zu sehen war. Getippte Werte liegen als gesperrte Größen in der Sitzung (`sketch_input.rs`) und werden beim Abschluss zu Maßen; `sketch_inline.rs` zeichnet nur die Felder, `sketch_view.rs` nur Zeichen, Raster und Rahmen.

**Tech Stack:** Rust-Workspace, egui, Fluent (`de.ftl`/`en.ftl`, `tr!`), serde.

**Spec:** `docs/superpowers/specs/2026-11-01-skizzen-eingabe-design.md`

## Global Constraints

- Verhalten, Namen und Kürzel wie Autodesk Fusion; Abweichungen in `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt Skizzen 2“) mit Grund. Annahmen über Fusion sind mit **Annahme** markiert.
- Keine neuen Fremd-Abhängigkeiten (sonst `cargo deny check`); kein Autodesk-Material, Symbole selbst gezeichnet.
- Alle Oberflächentexte über `tr!` mit Schlüssel in `crates/planora-i18n/languages/de.ftl` **und** `en.ftl`. Namen: „Skizzenraster/Sketch Grid“, „Fang/Snap“, „Blick auf/Look At“.
- Intern mm, Anzeigen über `planora_i18n` (Zoll muss gehen); Farben nur über Theme-Tokens (Wächter `no_hard_coded_colors.rs`).
- Dateiformat unverändert, alte Dateien bleiben gleich (Einstellungen in `config.json` sind erlaubt; Vorgabewerte werden nicht geschrieben).
- Leistung: Fang-Suche bei 200 Kurven < 4 ms je Bild (Release, `#[ignore]`-Messtest).
- GUI-Tests bei 1920×1080, nie 800×600.
- Prüfen: `cargo test -p <crate> --target-dir C:/Users/denni/AppData/Local/Temp/claude/D--Testumgebung/9257fb9d-a130-4b88-86e3-d09eb7b412f8/scratchpad/target-e2 -j 2`; am Ende `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check`. `cargo fmt --all` vor jedem Commit. Nichts pushen.
- Commit-Nachrichten deutsch, enden mit Leerzeile und `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Alte Tests, die neues Fusion-Verhalten treffen, werden **bewusst** umgeschrieben (Kommentar „Skizzen 2“, Listen in Task 2, 3, 5, 6, 7); alle anderen bleiben unverändert grün.

## Entscheidungen des Planers

1. **Fangradius** wie heute (`PICK_RADIUS_PX` = 8 px). Ausrichtungslinien: 0,6 × Fangradius (etwa 5 px), sonst klebt der Zeiger bei vielen Punkten. Richtungsgrenze 3° (`AUTO_AXIS_ANGLE`) für H/V, parallel, rechtwinklig, tangential.
2. **Fangreihenfolge:** Alle Arten im Radius gelten; die Klasse entscheidet vor dem Abstand (wie heute „Punkte vor Kurven“): Punkt/Mitte/Ursprung > Mittelpunkt > Schnittpunkt > Kurve; innerhalb der Klasse der nächste, bei gleichem Abstand kleinere Id. Danach Ausrichtung, zuletzt Raster je noch freier Achse. Eine Kurve geht vor Ausrichtung; ausgerichtete Achse + Rasterachse dürfen sich mischen.
3. **Ablauf `resolve_cursor`** (rein, je Bild und beim Klick dieselbe Funktion): Rohlage → Strg? (dann nur Sperren, kein Fang, keine Automatik) → Geometrie-Fang → Richtung (Task 3) → Ausrichtung → Raster → gesperrte Größen. Sperren gewinnen immer; Anbindung (Punkt teilen, Punkt auf Kurve …) gilt nur, wenn die Endlage die Fanglage trifft (≤ `tol.linear`).
4. **Richtung beim freien Endpunkt:** Linienende wird exakt auf die erkannte Richtung projiziert (Vorschau und Klick gleich), damit der Solver nichts bewegt. Steht das Ende auf gefangener Geometrie, gilt eine Richtung nur, wenn sie schon exakt stimmt (≤ `tol.angular`). **Änderung gegenüber heute:** Eine Linie auf einen vorhandenen Punkt, die nur fast waagerecht ist, wird nicht mehr geradegezogen. Vorrang: H/V > tangential > parallel/rechtwinklig; Kandidaten sind alle lebenden Linien außer der laufenden Kette, bester Winkel, bei Gleichstand kleinere Id. **Annahme:** Fusion nimmt bevorzugt zuletzt überfahrene Linien; hier zählt jede, das Zeichen zeigt sie.
5. **Tangential** nur in Laufrichtung (kein Zurückfalten): Start auf dem Ende eines Bogens/einer Linie, Abgangsrichtung ≤ 3° zur Fortsetzung des anderen. Linie am Bogen, Bogen an Linie, Bogen an Bogen; beim Bogen wird beim Setzen des Startpunkts entschieden (Mitte liegt schon fest), das Zeichen bleibt bis zum Abschluss.
6. **Anbindung beim Abschluss:** Punkt → dieselbe `PointId` wie heute (Rechteck: `Coincident`, weil es eigene Ecken anlegt); Kurve → `PointOnCurve`; Linienmitte → `Midpoint`; Bogenmitte → `PointOnCurve` (`Midpoint` gibt es nur für Linien; Abweichung); Schnittpunkt → zweimal `PointOnCurve`; Ursprung → neuer Punkt mit `Fixed(p, 0, 0)`, danach fängt er als Punkt (**Annahme** Fusion: deckungsgleich mit dem Ursprungspunkt; den gibt es im Modell nicht; das Zeichen „fix“ wird dort nicht gezeichnet). Kreisrand/Ellipse: nur ein gefangener Punkt bindet (`PointOnCurve(p, kreis)`), sonst keine Abhängigkeit.
7. **Automatik still:** `try_auto` verallgemeinert `add_auto_axis`: Kopie, anlegen, lösen; verworfen, wenn vorher nicht `Solved`, danach nicht `Solved`, die neue in `report.redundant` oder ein vorhandener Punkt sich um mehr als `10·tol.linear` bewegt. Keine Meldung.
8. **Felder:** je Stufe, Tool `Point`/`Spline` ohne Felder. Linie: Länge, Winkel. Rechteck: Breite, Höhe. Kreis: Durchmesser. Bogen: Stufe Start Radius, Stufe Ende Öffnungswinkel. Ellipse: Stufe große Achse, Stufe kleine Achse. Felder folgen dem Zeiger (Zeiger + (24, 24), je Feld 30 px tiefer, am Rand nach links/oben geklappt) und nehmen keine Mausereignisse an (`Area::interactable(false)`), damit sie nie den Zeiger über der Zeichnung verdecken.
9. **Tippen:** Das aktive Feld hat die Tastatur, sobald Felder da sind (außer ein anderes Textfeld hat sie). Vor dem ersten Zeichen zeigt es den aktuellen Wert gerundet; das erste Zeichen ersetzt ihn (`touched`). Gültiger Text sperrt sofort (Vorschau folgt), leerer Text gibt frei, ungültiger gibt frei und zeigt die Meldung; Eingabe wird dann abgelehnt. Tab/Umschalt+Tab wechselt und behält Sperren. Eingabe setzt den Punkt an der aufgelösten Lage (ohne getippte Werte: an der Zeigerlage, ohne Maß), Linienzug läuft weiter. Enter beendet den Linienzug also nicht mehr (Esc oder Doppelklick; Spline unverändert). Esc wie heute `cancel`.
10. **Winkel der Linie:** 0…180°, die Zeigerseite wählt das Vorzeichen, gespeichert positiv über die Reihenfolge (`Angle(bezug, linie, v)` oder `Angle(linie, bezug, v)`). Bezug: vorige Linie der Kette in Vorwärtsrichtung (90 = rechtwinklig, 30 = 30° Abknicken, **Annahme**), sonst Skizzen-x-Achse. Weil `Angle` zwei Linien braucht und das Format unverändert bleibt, legt der erste getippte Achsenwinkel einmal je Skizze eine Konstruktionslinie `(0,0)-(10,0)` mit `Fixed` an beiden Enden an (0 Freiheitsgrade, wiederverwendet; Abweichung).
11. **Öffnungswinkel als Maß:** zwei Konstruktionslinien Mitte→Start und Mitte→Ende plus `Angle` (Residuum arbeitet modulo π, Wert bleibt der getippte; Abweichung, nachgebildet). Bereich 0 < v < 360°. Breite/Höhe: `Distance` an `points[0..1]` bzw. `points[1..2]` von `RectIds`; übrige Maße: `Distance`, `Diameter`, `Radius`, `EllipseMajor`, `EllipseMinor`. Getippte Werte früherer Stufen (Bogenradius, große Achse) bleiben bis zum Abschluss in der Sitzung. Überzähliges Maß wird anzeigend (Regel von `apply`, Meldung `sketch-became-driven`), Widerspruch lehnt den ganzen Abschluss ab.
12. **Rückgängig:** Abschluss samt Maßen und Automatik in einem Bild, also ein Schritt; Name über `edit_label` (`undo-line`, `undo-rectangle` …), auch wenn Maße dabei sind. Der Verlauf wertet heute „Eingabefeld hat Tastatur“ als Geste; ein Zeichenfeld zählt nicht dazu (sonst wäre ein ganzer Linienzug ein Schritt). Strg+Z bei fokussiertem Zeichenfeld nimmt wie sonst den letzten Skizzenschritt zurück.
13. **Raster:** eigenes Raster in der Skizzenebene im Overlay (Stufe und Übergang aus `grid_step`, Farben `viewport_grid`/`viewport_grid_major`), dafür im Skizzenmodus nur noch die Achsen aus der 3D-Szene (das Bodenraster läge bei XZ/YZ/Flächen schief). Raster-Fang nutzt die sichtbare Stufe: fein, ab Übergang ≥ 0,5 die zehnfache; aus der Anzeigeeinheit in `f64` gerechnet, damit Zoll exakt bleibt. Raster aus ⇒ kein Raster-Fang („am sichtbaren Raster“, **Annahme**).
14. **Rahmen:** Richtung und Inhalt in Bildschirmkoordinaten (stimmt auch bei gedrehter Ansicht); `a.x ≤ b.x` = Fenster. Gewählt werden Kurven und nur freie Punkte, sonst passt keine Abhängigkeit mehr zur Auswahl (Abweichung). **Umschalt+Ziehen links auf freier Fläche im Auswahlwerkzeug wählt statt zu schwenken** (Fusion schwenkt mit der Mitte; Abweichung von der eigenen Vorgabe, nur dort); sonst bleibt die Belegung. Umschalt ergänzt ohne Umschalten, ein gewöhnlicher Rahmen ersetzt.
15. **Kette:** über gemeinsame Endpunkte und `Coincident` an Endpunkten; Mitten zählen nicht, Verzweigungen laufen mit („zusammenhängend“), `PointOnCurve` nicht.
16. **Ziehen:** Punkt vor Kurve vor Rahmen. Immer relativ (`Start + Zeiger − Griff`), nie Sprung auf den Zeiger. Gegriffene Kurve außerhalb der Auswahl ersetzt die Auswahl, innerhalb ziehen alle gewählten Kurven und Punkte. Einzelner Kreis am Rand: Radius (**Annahme** auch Bogen und Ellipse werden als Ganzes verschoben). Alle Bilder eines Zugs bilden einen Schritt; nicht lösbar ⇒ Stand vor diesem Bild, `sketch-drag-blocked`.
17. **Blick auf:** nur Richtung und Ziel auf die Ebene, Zoom bleibt (**Annahme**), `sketch_fit = None`. Kürzel ohne Vorgabetaste belegbar: Blick auf, Skizzenraster, Fang.
18. **Einstellung:** `AppState` trägt die Schalter; Knopf setzt zusätzlich `sketch_grid_changed`. `Workspace` übernimmt je Bild: gesetzt ⇒ speichern und in alle Tabs kopieren, sonst Stand aus der Konfiguration in den aktiven Tab.

## Review Focus

1. Tippen in ein Zeichenfeld: „l“, „c“, Leertaste, Entf lösen nichts aus und landen im Feld; der getippte Wert überlebt Zeigerbewegung und Tab (Task 5).
2. Gehaltene Strg-Taste beim Klick, auch wenn sie nur im Ereignis des Klicks steht: kein Fang, keine Ausrichtung, kein Raster, keine automatische Abhängigkeit, auch nicht auf einem Punkt (Task 2, Task 3).
3. Automatische Abhängigkeit überzählig oder widersprüchlich (Linie zwischen zwei festen Punkten, Parallele auf schon bestimmter Geometrie): still weg, Skizze `Solved`, `redundant` leer, nichts bewegt (Task 3).
4. Ganze Kurve mit Abhängigkeiten ziehen: kein Sprung beim ersten Bild, ein Rückgängig-Schritt stellt alles bitgleich her, Esc mitten im Zug und blockierte Züge hinterlassen keinen halben Stand (Task 8).
5. Zoll: Raster-Fang auf Vielfachen von 25,4 mm-Zehnerstufen ohne f32-Fehler, Feld „1“ ergibt 25,4 mm und Speichertext „1 in“, Formel mit Parameter bleibt erhalten (Task 4, Task 6).

---

### Task 1: Fang-Index in der Skizze

**Files:**
- Create: `crates/planora-sketch/src/snap.rs`; `lib.rs` (`pub mod snap`, Re-Export)
- Test: `crates/planora-sketch/tests/snap.rs` (neu), `crates/planora-sketch/tests/snap_perf.rs` (neu, `#[ignore]`)

**Interfaces:**
- Consumes: `Geom` (`domain`, `eval`, `nearest`, `bbox`), `Prepared`, `intersect_prepared`, `Sketch::revision`, `curve_polyline`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Snap { None, Point(PointId), Center(PointId), Midpoint(CurveId), Intersection(CurveId, CurveId), Curve(CurveId), Origin, Grid }`
  - `pub struct SnapHit { pub snap: Snap, pub at: DVec2 }`, `#[derive(Default)] pub struct AlignHit { pub x: Option<(PointId, f64)>, pub y: Option<(PointId, f64)> }` (`x`: Zeiger rastet auf das x des Punktes, senkrechte Hilfslinie; `y` entsprechend).
  - `#[derive(Clone, Debug)] pub struct SnapIndex`; `impl SnapIndex { pub fn build(sk: &Sketch) -> SnapIndex; pub fn revision(&self) -> u64; pub fn find(&self, at: DVec2, radius: f64) -> Option<SnapHit>; pub fn align(&self, at: DVec2, radius: f64, skip: &[PointId]) -> AlignHit }`
  - Index: Punkte (`Center`, wenn nur Mitte von Kreis/Bogen/Ellipse/Ellipsenbogen), Mitten von Linie und Bogen (`eval((lo+hi)/2)`, Domäne ist absolut), Schnitte aller Kurvenpaare inkl. Konstruktion (`intersect_prepared`, Parameter sind absolut), je Kurve Hüllbox und Linienzug (96 je Vollkreis). `Origin` bei `(0,0)`, wenn dort kein lebender Punkt liegt. `find` nach Entscheidung 2; `Curve`-Lage über `Geom::nearest` nachgeschärft.

- [ ] **Step 1: Tests** – je Art ein Treffer mit genauer Lage (Linienmitte; Bogen mit `a0 = 3π/2`, Mitte des Bogens auf dem Kreis, nicht bei `sweep/2`); Reihenfolge: Linienmitte und Kurve zugleich → Mittelpunkt, Endpunkt im Radius neben Schnittpunkt näher am Zeiger → Punkt; zwei Kurven an einem gemeinsamen Punkt → `Point`, nicht `Intersection`; Kreismitte → `Center`, Mitte + Endpunkt derselben Id → `Point`; Ursprung frei → `Origin`, Punkt bei `(0,0)` → `Point`; nichts im Radius → `None`; `align`: x und y zugleich von zwei Punkten, `skip` wirkt, Treffer nur innerhalb des Radius; `revision()` gleich `sk.revision()`; nie NaN bei winziger Kurve und `radius = 0`.
- [ ] **Step 2: Messtest** `snap_perf.rs` `#[ignore]`: 200 Kurven (wie `profile_perf`): `find` + `align` an 200 Stellen, Median < 1 ms, `build` < 50 ms.
- [ ] Steps 3–4: FAIL, implementieren, `cargo test -p planora-sketch` und `--release --test snap_perf -- --ignored` grün.
- [ ] **Step 5: Commit** „Skizze: Fang-Index mit Mittelpunkt, Schnittpunkt, Ursprung, Ausrichtung“.

### Task 2: Aufgelöster Zeiger, Anbindung, Fangzeichen, Ausrichtungslinien

**Files:**
- Create: `crates/planora-app/src/sketch_cursor.rs`; `lib.rs`
- Modify: `crates/planora-app/src/sketch_state.rs` (`Snap` = `pub use planora_sketch::snap::Snap`; `snap()` liefert weiter nur `Point`/`Curve`/`None` für die Auswahl; alle `click_*` und `point_at` arbeiten auf `&DrawCursor`; `pending_snaps: Vec<SnapHit>` für die erste Rechteckecke), `sketch_view.rs` (`handle_input`: Strg-Erkennung wie Umschalt inkl. `PointerButton`-Ereignis, `update_cursor` je Bild; `draw`: Zeichen, gestrichelte Hilfslinien, in Zeichenwerkzeugen `hovered` aus `draw_cursor` statt neuer `se.snap`-Suche; `draw_preview` nutzt `draw_cursor.at`; `overlay_text`: `Fixed` bei `(0,0)` ohne Beschriftung), `theme.rs` (`SketchColors.snap`, `.guide`)
- Test: `crates/planora-app/tests/sketch_snap.rs` (neu), `crates/planora-app/tests/sketch_snap_perf.rs` (neu, `#[ignore]`), `sketch_state.rs`, `undo_sketch.rs`, `keymap_sketch_drawing.rs`, `sketch_view.rs` (bewusst: Tests, die am Ursprung beginnen und Abhängigkeiten zählen, bekommen das erwartete `Fixed` oder einen Start außerhalb des Ursprungs)

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - `#[derive(Clone, Copy, Debug, Default)] pub struct CursorOptions { pub suppressed: bool, pub grid_step: Option<f64> }` (`suppressed` = Strg, gilt bis zum nächsten Setzen; Task 6 setzt `grid_step`).
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum AutoConstraint { Horizontal, Vertical, Parallel(CurveId), Perpendicular(CurveId), Tangent(CurveId) }`
  - `#[derive(Clone, Debug)] pub struct DrawCursor { pub raw: DVec2, pub at: DVec2, pub snap: Snap, pub snap_at: Option<DVec2>, pub guides: AlignHit, pub autos: Vec<AutoConstraint> }` (`autos` bleibt bis Task 3 leer).
  - `impl SketchSession { pub fn resolve_cursor(&mut self, sk: &Sketch, raw: DVec2, radius: f64) -> DrawCursor; pub fn update_cursor(&mut self, sk: &Sketch, raw: Option<DVec2>, radius: f64); pub fn is_drawing_tool(&self) -> bool }`; Felder `cursor_opts: CursorOptions`, `draw_cursor: Option<DrawCursor>`, `snap_index: Option<Arc<SnapIndex>>` (neu gebaut, wenn `revision` abweicht, nur in Zeichenwerkzeugen; alles außer `Select`, `ProjectEdge`, `Trim`, `Extend`, `Break`).
  - `fn attach(&mut self, sk: &mut Sketch, p: Option<PointId>, cur: &DrawCursor) -> Result<PointId, Msg>` nach Entscheidung 6.
  - Zeichen (`paint_snap_mark(painter, at, Snap, color)`): Quadrat Punkt, Kreis Mitte, Dreieck Mittelpunkt, X Schnitt, Sanduhr Kurve, Kreuz im Kreis Ursprung, kleines Plus Raster. `skip` für `align`: laufende `pending`-Punkte; Rechteck: seine erste Ecke über die Lage.

- [ ] **Step 1: Tests (`sketch_snap.rs`)** – Klick auf Linienmitte legt `Midpoint`-Punkt, auf Bogenmitte `PointOnCurve`, auf Schnittpunkt zwei `PointOnCurve`, auf Ursprung `Fixed(p,0,0)`, zweiter Klick dort nutzt denselben Punkt; Rechteckecke auf vorhandenem Punkt → `Coincident`, auf Kurve → `PointOnCurve`, beide Ecken; Kreisrand auf Punkt → `PointOnCurve`; Ausrichtung: Zeiger 3 px neben der Verlängerung eines Punktes → `at.x` exakt, `guides.x` gesetzt, keine neue Abhängigkeit, an zwei Punkten zugleich beide Achsen exakt; Kurve geht vor Ausrichtung; `resolve_cursor` ohne Änderung bitgleich, zweimal gerufen baut den Index nur einmal; **Review-Focus 2:** `cursor_opts.suppressed` auf einem vorhandenen Punkt → neuer Punkt, keine Abhängigkeit; Overlay-Test bei 1920×1080 (Muster `sketch_trim.rs` `play`): `PointerButton { modifiers: ctrl }` ohne gehaltene Taste zählt, Fangzeichen und Hilfslinie erscheinen im `draw_cursor`, Zeichen ändern die Skizze nicht.
- [ ] **Step 2: Messtest** `#[ignore] snap_search_under_4_ms`: 200 Kurven, `resolve_cursor` an 200 Stellen (Index schon gebaut, Linie läuft), Median < 4 ms im Release.
- [ ] Steps 3–4: FAIL, implementieren, alte Tests anpassen, `cargo test -p planora-app` und Messtest grün.
- [ ] **Step 5: Commit** „Skizze: Mehr Fang und Ausrichtungslinien beim Zeichnen“.

### Task 3: Automatische Abhängigkeiten mit Zeichen

**Files:**
- Modify: `crates/planora-app/src/sketch_cursor.rs` (`plan_direction`), `sketch_state.rs` (`try_auto` ersetzt `add_auto_axis`; `click_line`, `click_arc`; `auto_tangent: Option<CurveId>`), `sketch_view.rs` (Zeichen an neuer und an der Bezugslinie, Glyphen wie `overlay_text`), `de.ftl`/`en.ftl` (`sketch-auto-axis` = „Automatische Abhängigkeiten“/„Automatic constraints“, Hinweis)
- Test: `crates/planora-app/tests/sketch_auto.rs` (neu), `sketch_state.rs` (bewusst nur, wo Linien fast parallel gezeichnet und Abhängigkeiten gezählt werden; `the_line_tool_draws_a_chain…`: Ende auf dem ersten Punkt bleibt ohne Richtung)

**Interfaces:**
- Consumes: Task 2 `DrawCursor`, `resolve_cursor`.
- Produces:
  - `fn plan_direction(sk: &Sketch, tool: Tool, start: PointId, chain: Option<CurveId>, raw_end: DVec2, on_geometry: bool) -> Option<(AutoConstraint, DVec2 /* Richtung, normiert */)>` nach Entscheidung 4, 5.
  - `fn try_auto(&mut self, sk: &mut Sketch, kind: ConstraintKind) -> bool` nach Entscheidung 7.
  - `se.auto_constrain == false` schaltet nur die Richtung ab (wie heute), die Anbindung bleibt.

- [ ] **Step 1: Tests** – fast waagerecht: Vorschau exakt waagerecht, `autos == [Horizontal]`, nach dem Klick genau diese Abhängigkeit; **alles, was `autos` nennt, entsteht, und nichts sonst** (Vergleich der Abhängigkeiten vor/nach für H, V, parallel, rechtwinklig, tangential); schräge Linie 2° neben einer anderen → `Parallel`, Punkte bleiben exakt; 88° → `Perpendicular`; 5° daneben → nichts; H schlägt parallel; Linie aus Bogenende in Laufrichtung → `Tangent`, zurück → nichts; Bogen an Linienende → `Tangent` beim Abschluss, Zeichen schon in der Endstufe; gleich entsteht nie; **Review-Focus 3:** Linie zwischen zwei `Fixed`-Punkten exakt waagerecht → kein `Horizontal`, `redundant` leer; Parallele auf schon bestimmter Geometrie → weg, nichts bewegt; vorher ungelöste Skizze → keine Automatik; Linie auf vorhandenen Punkt 2° neben waagerecht → kein `Horizontal`, Punkt unbewegt; `suppressed` → keine Richtung, Vorschau folgt dem rohen Zeiger; `auto_constrain = false` → keine Richtung, `PointOnCurve` bleibt.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Parallel, rechtwinklig und tangential beim Zeichnen, sichtbar und still verworfen“.

### Task 4: Gesperrte Größen und Maße beim Abschluss (ohne Oberfläche)

**Files:**
- Create: `crates/planora-app/src/sketch_input.rs`; `lib.rs`
- Modify: `crates/planora-app/src/sketch_cursor.rs` (Sperren als letzter Schritt von `resolve_cursor`), `sketch_state.rs` (`created: Option<Created>`, `chain_line: Option<CurveId>`, `edit_label` je Werkzeug), `de.ftl`/`en.ftl` (`sketch-field-length`, `-angle`, `-width`, `-height`, `-diameter`, `-radius`, `-sweep`, `-major`, `-minor`; `sketch-field-positive`, `sketch-field-angle-range`, `sketch-field-sweep-range`, `sketch-field-minor-too-big`, `sketch-field-rejected`)
- Test: `crates/planora-app/tests/sketch_input.rs` (neu)

**Interfaces:**
- Consumes: Task 2, 3; `formula_field::{resolve, value_field_text}`, `set_dimension_input`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum FieldKind { Length, Angle, Width, Height, Diameter, Radius, Sweep, MajorAxis, MinorAxis }`; `impl FieldKind { pub fn unit(self) -> ParamUnit; pub fn label(self) -> String }`
  - `#[derive(Clone, Debug)] pub struct DrawField { pub kind: FieldKind, pub text: String, pub touched: bool, pub locked: Option<f64> /* mm oder rad */, pub error: Option<String> }`
  - `#[derive(Clone, Debug, Default)] pub struct DrawInput { pub fields: Vec<DrawField>, pub active: usize, key: (Option<Tool>, Vec<PointId>, usize), carried: Vec<(FieldKind, String)> }`
  - `impl SketchSession { pub fn sync_input(&mut self, sk: &Sketch); pub fn type_field(&mut self, i: usize, text: &str, scope: &ParamScope); pub fn next_field(&mut self, back: bool); pub fn commit_input(&mut self, sf: &mut SketchFeature, raw: DVec2, radius: f64, extend: bool, scope: &ParamScope) -> Result<(), String>; pub fn field_value(&self, sk: &Sketch, kind: FieldKind) -> Option<f64> }`
  - `commit_input`: Kopie von `sf`, `click` an aufgelöster Lage, dann Maße (Entscheidung 10, 11) über `add_constraint` + `set_dimension_input`; Fehler ⇒ Kopie zurück, Sitzung wie vorher, `message` gesetzt. Abschluss einer Stufe legt getippte Werte in `carried`. Ohne Felder oder ohne Tippen gleich `click`.
  - `pub(crate) fn axis_reference(sk: &mut Sketch) -> Result<CurveId, Msg>` (Entscheidung 10).

- [ ] **Step 1: Tests** – Linie: `type_field(Length, "25")`, Zeiger irgendwo schräg → Ende auf Strahl im Abstand 25, nach Zeigerwechsel neue Richtung, gleiche Länge; `commit_input` → `Distance` 25 und `length` exakt; nur Winkel → Ende auf Strahl, Länge folgt dem Zeiger, `Angle`-Maß zur Achshilfe mit genau einem Konstruktionsstück, zweite Linie nutzt dieselbe Hilfe, 0 Freiheitsgrade der Hilfe; Kette: Winkel 90 zur vorigen Linie; Länge+Winkel → Fang wirkt nicht; nur Länge + fast waagerecht → H und Länge zugleich; Rechteck: Breite 30 (Zeiger links vom Start → Ecke links), Höhe frei, nur ein Maß; Breite+Höhe → beide; Kreis Durchmesser „20“ → `Diameter`; Bogen: Radius in Stufe 2, Öffnungswinkel 270 in Stufe 3 → `Radius` und `Angle`, Bogen hat 270°; Ellipse: beide Halbachsen, kleine > große → Fehler am Feld. Nicht getippt → kein Maß. Formel: Parameter `Breite = 40`, Text „Breite / 2“ → Wert 20, `sf.formulas[id] == "Breite / 2"`. **Review-Focus 5:** Zoll: „1“ → 25,4 mm, Formeltext „1 in“. Leeres Feld gibt frei; ungültiger Text → `locked == None`, `error` gesetzt, `commit_input` Err ohne Änderung; 0 und negative Längen, Winkel 200 → Meldung. Widersprüchliches Maß (Linie zwischen festen Punkten mit anderer Länge) → Abschluss abgelehnt, Skizze bitgleich, Sitzung unverändert; überzähliges Maß → anzeigend mit Meldung. Stufenwechsel und neuer Kettenpunkt leeren Felder; `cancel`, `set_tool`, `after_history_jump` leeren auch `carried`.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Getippte Werte sperren und werden zu Maßen“.

### Task 5: Felder am Zeiger

**Files:**
- Create: `crates/planora-app/src/sketch_inline.rs`; `lib.rs`
- Modify: `crates/planora-app/src/formula_field.rs` (`FormulaField::lock_focus(bool)` an `TextEdit::lock_focus`, sonst springt Tab aus dem Feld), `sketch_view.rs` (`show_overlay`: nach `handle_input` die Felder; Klicks in Zeichenwerkzeugen über `commit_input`; Vorschau nach Sperren), `viewport.rs` (`set_draw_field(ctx, id)`; `typing_for_history(ctx)`: wie `typing`, aber ein Zeichenfeld zählt nicht; `remember_keyboard_focus` merkt beides), `app.rs` (:2584 Geste mit `typing_for_history`); `document.param_scope()` wird nur geholt, wenn Felder da sind
- Test: `crates/planora-app/tests/sketch_inline_fields.rs` (neu), `sketch_ui.rs` und `keymap_sketch_drawing.rs` (bewusst: Enter im Linienwerkzeug setzt den Punkt; Spline-Enter unverändert)

**Interfaces:**
- Consumes: Task 4 `DrawInput`, `sync_input`, `type_field`, `next_field`, `commit_input`, `field_value`.
- Produces: `pub fn sketch_inline::show(ui: &mut egui::Ui, rect: Rect, state: &mut AppState, pointer: Option<Pos2>)`. Ablauf je Bild: `sync_input`; Tab/Umschalt+Tab mit `consume_key` vor dem Feld; bei noch nicht `touched` und `Event::Text`/`Paste` im Bild den Text vor dem `TextEdit` leeren; sonst zeigt das Feld `field_value` gerundet (Länge wie `fmt_len` ohne Einheit, Winkel eine Stelle); `FormulaField::new(("planora-draw-field", kind_index), …).lock_focus(true).error_below(false).width(72.0)` in `Area::new(..).fixed_pos(..).order(Order::Foreground).interactable(false)`; aktives Feld `response.request_focus()`, wenn nicht `viewport::keyboard_taken` durch ein anderes Feld. `response.lost_focus()` + `key_pressed(Enter)` → `commit_input` (Rohlage: `se.draw_cursor.raw`); `lost_focus` + `key_pressed(Escape)` → `se.cancel`. `committed` aus `FieldOutcome` wird nicht benutzt (Klick neben das Feld nimmt nur den Fokus, nächstes Bild fordert ihn wieder an).

- [ ] **Step 1: Tests** (1920×1080; Rohereignisse wie `sketch_trim.rs`, Kürzel über `PlanoraApp::for_tests` wie `keymap_sketch_drawing.rs`, aber mit `vec2(1920.0, 1080.0)`) – **Review-Focus 1:** Linie begonnen, Text „l“ landet im Feld, Werkzeug bleibt, kein Tastenkürzel, auch nicht „c“, „t“, Leertaste, Entf; gleiches bei Bogen und Kreis; Feld hat nach dem ersten Klick ohne Mausklick die Tastatur; „25“ tippen, Zeiger 40 Pixel und drei Bilder bewegen, Tab, weiter bewegen: Sperre bleibt, Vorschau-Länge 25, aktives Feld ist Winkel, „45“ tippen → beide; Umschalt+Tab zurück; Eingabe setzt den Punkt, Linienzug läuft, Felder neu und leer von Sperren; Klick mit gesperrtem Wert nutzt die Sperre. Esc im Feld bricht ab, kein zweites Abbrechen im selben Bild (Auswahl bleibt, Werkzeug bleibt). Formel mit Parameter im Feld zeigt „fx“. Das erste Zeichen ersetzt den Vorschlag, Rücktaste vor dem ersten Zeichen leert. Offenes Maßfeld im Skizzenfeld verliert nicht die Tastatur. Feld überdeckt den Zeiger nicht: `response.hovered()` des Overlays bleibt wahr an allen vier Ecken des Bildschirms. **Rückgängig:** Linienzug aus drei Linien mit Feld im Fokus → drei Schritte je „Linie zeichnen“, auch mit getippten Maßen und Automatik; Strg+Z bei Feld im Fokus nimmt die letzte Linie zurück, Felder leer; zwei Sprachen: englische Feldnamen.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Maßfelder am Zeiger wie Fusion“.

### Task 6: Skizzenraster und Fang

**Files:**
- Modify: `crates/planora-render/src/grid.rs` + `lib.rs` (`visible_step`, `axis_lines`, `grid_lines` nutzt `axis_lines`), `crates/planora-project/src/config.rs` (`sketch_grid`, `sketch_snap`), `crates/planora-app/src/state.rs` (`sketch_show_grid`, `sketch_snap_grid`, `sketch_grid_changed`), `workspace.rs` (`set_sketch_grid`, `sketch_grid_pending`, `pending_sketch_grid` in **allen** Schreibwegen, Muster `pending_length_unit`; `sync_sketch_grid`), `commands.rs` (`observe` ruft `sync_sketch_grid`), `viewport.rs` (:731 im Skizzenmodus nur `axis_lines`), `sketch_view.rs` (`handle_input`: `cursor_opts.grid_step`; `draw`: `sketch_grid_shapes` zuunterst), `sketch_panel.rs` (`SideInput.palette`, Zeile unter dem Namen), `de.ftl`/`en.ftl` (`sketch-palette-grid` „Skizzenraster“/„Sketch Grid“, `sketch-palette-snap` „Fang“/„Snap“ plus Hinweise)
- Test: `crates/planora-render/src/grid.rs` (Modultests), `crates/planora-project/tests/` (vorhandene Config-Tests), `crates/planora-app/tests/sketch_grid.rs` (neu), `viewport_shell.rs`; bewusst: GUI-Tests, die freie Koordinaten vergleichen, setzen `s.sketch_snap_grid = false` in ihrer Hilfsfunktion (`sketch_ui.rs`, `sketch_trim.rs`, `keymap_sketch_drawing.rs`, `undo_sketch.rs`, `formula_sketch_ui.rs`, weitere per Testlauf)

**Interfaces:**
- Consumes: Task 2 `cursor_opts.grid_step`.
- Produces:
  - `pub fn visible_step(distance: f32, unit_mm: f64) -> f64` (Entscheidung 13; aus `grid_step` über den gerundeten Exponenten in `f64`), `pub fn axis_lines(target: Vec3, distance: f32, palette: &Palette) -> Vec<LineVertex>`.
  - `Config { sketch_grid: bool, sketch_snap: bool }` (Vorgabe `true`, nachsichtig gelesen, `skip_serializing_if` bei `true`).
  - `pub fn Workspace::set_sketch_grid(&mut self, grid: bool, snap: bool) -> bool` (wie `set_length_unit`), `pub fn Workspace::sync_sketch_grid(&mut self)` (Entscheidung 18).
  - Raster-Fang in `resolve_cursor`: Achse ohne Ausrichtung, kein Geometrie-Fang: `(v / step).round() * step`, `Snap::Grid`; nicht bei `suppressed`; `grid_step = sketch_show_grid && sketch_snap_grid`.

- [ ] **Step 1: Tests** – `visible_step(150.0, 1.0) == 10.0`, ab Übergang ≥ 0,5 die zehnfache Stufe, `visible_step(150.0 * 25.4, 25.4)` auf 1e-9 gleich 254,0, über den ganzen Zoombereich Zehnerpotenz der Einheit; `axis_lines` = Achsen aus `grid_lines`. Config: fehlt → an, `false` übersteht Rundlauf, Datei ohne Änderung bleibt ohne die Felder, kaputter Wert → Vorgabe. App: freier Klick rastet auf 10 mm, Geometrie-Fang geht vor, Ausrichtung+Raster mischen, Strg aus, Fang aus, Raster aus → kein Fang; **Review-Focus 5:** Zoll: gefangene Koordinate `x / step` ganzzahlig auf 1e-12; Skizze auf XZ: Rasterform liegt in der Skizzenebene (Linien über `Mapper`, Anzahl ≤ 200, fehlt bei Schalter aus); Bodenraster fehlt im Skizzenmodus, ist nach dem Beenden wieder da; Knopf schreibt Konfiguration, zweiter Tab übernimmt, neue `Workspace` liest sie (Temp-Ordner); Schreibfehler → `pending`, kommt beim nächsten Schreiben mit; beide Schalter sichtbar in der Feldbreite 250 (`side_field_width.rs`), englisch.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-render -p planora-project -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Skizzenraster und Fang, gemerkt in config.json“.

### Task 7: Rahmenauswahl und Kettenwahl

**Files:**
- Modify: `crates/planora-sketch/src/query.rs` (`connected_chain`), `crates/planora-app/src/sketch_state.rs` (`box_select`, `begin_box`, `box_to`, `end_box`, `select_chain`), `sketch_view.rs` (`handle_input`: Rahmen, Umschalt-Ausnahme vor `pan_2d`, Doppelklick im Auswahlwerkzeug; `draw`: Rahmen), `theme.rs` (`SketchColors.box_fill`)
- Test: `crates/planora-sketch/tests/query.rs`, `crates/planora-app/tests/sketch_select.rs` (neu), `sketch_ui.rs` (bewusst: `in_sketch_mode_the_viewport_does_not_orbit_but_still_pans` schwenkt mit Umschalt im Rechteck-Werkzeug oder mit der Mitte)

**Interfaces:**
- Produces:
  - `impl Sketch { pub fn connected_chain(&self, c: CurveId) -> Vec<CurveId> }` (sortiert, enthält `c`; Entscheidung 15).
  - `#[derive(Clone, Copy, Debug)] pub struct BoxSelect { pub start: DVec2 /* Bildschirm */, pub current: DVec2, pub extend: bool }`, Feld `SketchSession.box_select: Option<BoxSelect>`.
  - `impl SketchSession { pub fn select_rect(&mut self, sk: &Sketch, a: DVec2, b: DVec2, extend: bool, project: &dyn Fn(DVec2) -> Option<DVec2>); pub fn select_chain(&mut self, sk: &Sketch, c: CurveId, extend: bool) }` – `a`, `b` Bildschirmlagen; Fenster: alle Punkte des Linienzugs (96 je Vollkreis) im Rechteck; Kreuzung: zusätzlich jede Strecke, die das Rechteck schneidet. Ein Rahmen, der hinter die Kamera fällt, wählt die Kurve nicht.
  - Zeichnen: Fenster durchgezogen, Kreuzung gestrichelt, Fläche `box_fill`, Rand `selected`.

- [ ] **Step 1: Tests** – von links nach rechts: Linie ganz innen gewählt, halb drin nicht; von rechts nach links: halb drin gewählt, ganz außen nicht; Rahmen schneidet Kreis, ohne einen Stützpunkt innen zu haben → Kreuzung wählt; Bildschirmrichtung zählt: `project` spiegelt x (Blick von hinten) → links-nach-rechts bleibt Fenster; Umschalt ergänzt ohne vorhandenes abzuwählen, ohne Umschalt ersetzt; leerer Rahmen ohne Umschalt leert; freier Punkt innen gewählt, Endpunkte nicht; `available_actions` passt nach Rahmen um zwei Linien; Overlay bei 1920×1080: Zug auf freier Fläche startet Rahmen, Zug auf Linie nicht, Rechtsklick und Esc brechen ab und lassen die alte Auswahl, Umschalt+Zug auf freier Fläche schwenkt nicht und ergänzt, Umschalt+Mitte schwenkt wie vorher; kein Rückgängig-Schritt durch Auswählen. Kette: geschlossenes Rechteck, Doppelklick auf eine Seite → alle vier; zwei Linien über `Coincident` → beide; Bogen an Linie → beide, Kreis alleine; T-Stoß über `PointOnCurve` → nur die eine; Mitte eines Bogens verbindet nicht; Linienwerkzeug: Doppelklick beendet weiterhin.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Rahmenauswahl in beide Richtungen und Kettenwahl“.

### Task 8: Ganze Kurven ziehen

**Files:**
- Modify: `crates/planora-sketch/src/solver.rs` (`drag_radius`), `crates/planora-app/src/sketch_state.rs` (`CurveDrag`, `begin_drag`, `drag_to`, `end_drag` setzt `edit_label` = `undo-sketch-move`, `is_dragging`), `sketch_view.rs` (Ziehen in `handle_input`; `dragging_point` und Profilflächen über `is_dragging`), `de.ftl`/`en.ftl` (`undo-sketch-move` = „Kurven verschieben“/„Move curves“)
- Test: `crates/planora-sketch/tests/drag.rs`, `crates/planora-app/tests/sketch_curve_drag.rs` (neu)

**Interfaces:**
- Consumes: `Sketch::drag`, `Soft`, `DRAG_WEIGHT`; Task 7 Auswahl.
- Produces:
  - `impl Sketch { pub fn drag_radius(&mut self, c: CurveId, target: f64) -> Result<SolveReport, SketchError> }` – nur Kreis, Ziel > `tol.linear`, ohne Abhängigkeit am Radius direkt gesetzt, sonst `solve_with_soft`.
  - `#[derive(Clone, Debug)] pub struct CurveDrag { pub grab: DVec2, pub start: Vec<(PointId, DVec2)>, pub rim: Option<(CurveId, f64 /* Griffabstand − Radius */)> }`, Feld `curve_drag: Option<CurveDrag>`; `begin_drag` bleibt `-> bool` und nimmt Entscheidung 16 auf (`dragging` bleibt nur für Punkte, deren Tests unverändert); `pub fn is_dragging(&self) -> bool`.
  - Bewegte Punkte: `CurveKind::points()` aller gegriffenen Kurven plus gewählte Punkte, ohne Doppelte. `drag_to`: Kopie, Soll nach Entscheidung 16, bei nicht `Solved` Kopie zurück und `sketch-drag-blocked`.

- [ ] **Step 1: Tests** – freie Linie in der Mitte gegriffen, 10 mm gezogen: beide Enden +10, Griff kein Sprung (erstes Bild ohne Bewegung ändert nichts bitgleich); Bogen, Ellipse, Spline verschieben sich formtreu; Kreis am Rand: Radius folgt mit Griffversatz, Mitte bleibt; Kreis mit `Radius`-Maß: Radius bleibt, Meldung nur bei Scheitern; Kreismitte gegriffen: wie heute Punkt; Linie mit `Horizontal`: Abhängigkeit bleibt erfüllt; Linie mit `Fixed` an einem Ende dreht um dieses; zwei gewählte Kurven gemeinsam, nicht gewählte Kurve gegriffen ersetzt die Auswahl; gemeinsamer Punkt zweier Kurven nur einmal. **Review-Focus 4:** voll bestimmte Linie: bleibt, Skizze bitgleich, kein Schritt; Rechteck mit Maßen ziehen, loslassen, ein Rückgängig stellt alle Koordinaten bitgleich her, Name „Kurven verschieben“; Wiederholen geht; Esc mitten im Zug: Stand bleibt wie beim Punkt, ein Schritt; Abbruch per Werkzeugwechsel desgleichen; Zug über mehrere Bilder ist genau ein Schritt (Overlay + `observe_sketch(gesture)`); `Solved` nach dem Zug, Formeln an Maßen unverändert; Profilflächen fehlen während des Zugs.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Ganze Kurven ziehen, Kreisradius am Rand“.

### Task 9: Blick auf und neue Kürzel

**Files:**
- Modify: `crates/planora-app/src/state.rs` (`look_at_sketch`), `keymap/defaults.rs` (`SketchLookAt, "skizze.blick_auf", "action-sketch-look-at", Sketch, []`; `SketchToggleGrid, "skizze.raster", "action-sketch-toggle-grid", Sketch, []`; `SketchToggleSnap, "skizze.fang", "action-sketch-toggle-snap", Sketch, []`), `commands.rs` (`run_action_inner`), `sketch_panel.rs` (`SideInput.palette.look_at`, Knopf in der Palettenzeile, nach dem Feld in `show` ausgeführt), `icons.rs` (`HandIcon::LookAt` selbst gezeichnet), `de.ftl`/`en.ftl` (`sketch-look-at` „Blick auf“/„Look At“, Hinweis, drei `action-…`)
- Test: `crates/planora-app/tests/sketch_look_at.rs` (neu), `keymap_settings.rs`, `keymap_overview.rs`, `icons.rs`

**Interfaces:**
- Consumes: `OrbitCamera::look_along`, `Task 6` `sketch_grid_changed`.
- Produces: `pub fn AppState::look_at_sketch(&mut self)` (Entscheidung 17), `Action::{SketchLookAt, SketchToggleGrid, SketchToggleSnap}`.

- [ ] **Step 1: Tests** – Skizze auf XY und XZ und auf einer Fläche: Ansicht am Würfel gedreht, Knopf → Blickrichtung parallel zur Normalen (Skalarprodukt > 0,9999, im Rahmen von `MAX_PITCH`), `distance` gleich, Ziel in der Ebene, oben = `frame.y`; kein Rückgängig-Schritt; nach `refit_sketch_view` springt nichts; ohne offene Skizze nichts. Kürzel: alle drei ohne Vorgabetaste in den Einstellungen belegbar und wirksam (Taste belegt, gedrückt → Kamera bzw. Schalter + Konfiguration), beim Tippen im Zeichenfeld wirkungslos; Übersicht listet sie deutsch und englisch; Knopf passt in die Palettenzeile bei 1920×1080.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Blick auf, Kürzel für Raster und Fang“.

### Task 10: Doku und Gesamtprüfung

**Files:** `docs/funktionen.md` (Abschnitt „Skizzen“: Felder am Zeiger, Fang, Ausrichtungslinien, Automatik, Strg, Raster, Rahmen, Kette, Ziehen, Blick auf; Satz zu Enter nachführen), `CHANGELOG.md` (ein Absatz in Laiensprache wie der Zoll-Eintrag), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 2“: alle **Annahme**-Stellen, Achshilfslinie für den ersten Winkel, Öffnungswinkel über zwei Hilfslinien, Bogenmitte nur `PointOnCurve`, `Fixed` statt Ursprungspunkt, Rahmen ohne Endpunkte, Umschalt+Ziehen wählt im Auswahlwerkzeug, Bogen und Ellipse am Rand nur verschieben, Grenzen: Index-Neuaufbau kostet ein Bild, Richtungskandidaten sind alle Linien, Fang nur in Zeichenwerkzeugen, nicht beim Ziehen)

- [ ] **Step 1:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --target-dir … -j 2`, Messtests `cargo test --release -p planora-sketch --test snap_perf -- --ignored` und `-p planora-app --test sketch_snap_perf -- --ignored`, `cargo deny check` – alles grün. Rundlauf: eine vorhandene Fixture-Datei laden und ohne Änderung speichern bleibt bytegleich (vorhandene Tests in `planora-project`).
- [ ] **Step 2: Commit** „Doku: Skizzen 2 (Eingabe beim Zeichnen, Fang, Raster, Auswahl)“.

Hinweis: Handtest per `tools/gui-drive.ps1` bei 1920×1080 läuft danach auf Dennis' Go, nicht in dieser Aufgabe.
