# Skizzen 3: Zeichenvarianten – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Zeichenarten wie in Fusion: Rechteck 3 Punkte und Mitte, Kreis 2 und 3 Punkte, 2 und 3 tangential, Bogen 3 Punkte und tangential (auch aus dem Linienwerkzeug), Polygon mit drei Arten, Langloch mit fünf Arten, Mittellinie als dritter Linienstil, dazu Dreieck und Liste in der Werkzeugleiste.

**Architecture:** `planora-sketch` bekommt reine Geometrie (`shapes.rs`, `tangent_circles.rs`) und je Form eine atomare Operation, die alle Abhängigkeiten anlegt; Vorschau und Klick rechnen aus demselben Plan (`ShapePlan`). In der App beschreibt `ToolKind` (`sketch_kinds.rs`) jede Art als Daten: Werkzeug, Stufen samt Feldern, Aktion, Namen. Eine einzige Stufenmaschine (`click_kind`) sammelt Lagen in `pending_pos` und legt die Form erst beim letzten Klick an; die alten Vorgabearten laufen unverändert über ihre `click_*`-Funktionen.

**Tech Stack:** Rust-Workspace, egui 0.36 (`egui::Popup`), Fluent (`de.ftl`/`en.ftl`, `tr!`), serde.

**Spec:** `docs/superpowers/specs/2026-11-02-skizzen-zeichenvarianten-design.md`

## Global Constraints

- Verhalten, Namen und Kürzel wie Autodesk Fusion; Abweichungen in `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 3“) mit Grund. Annahmen über Fusion sind mit **Annahme** markiert.
- Keine neuen Fremd-Abhängigkeiten (`egui::Popup` ist schon im Baum); `cargo deny check` bleibt grün; kein Autodesk-Material, Symbole selbst gezeichnet.
- Alle Oberflächentexte über `tr!` mit Schlüssel in `crates/planora-i18n/languages/de.ftl` **und** `en.ftl`.
- Intern mm, Anzeigen über `planora_i18n` (Zoll muss gehen); Farben nur über Theme-Tokens (Wächter `no_hard_coded_colors.rs`).
- Dateiformat sonst unverändert: Format 14 nur, wenn eine Skizze eine Mittellinie enthält; ohne Mittellinie schreibt das Programm die Datei bytegleich wie bisher.
- Eine Form ist genau ein Rückgängig-Schritt. Esc bricht die laufende Form ab und hinterlässt nichts.
- Keine neue Form ist direkt überbestimmt: nach dem Anlegen `status == Solved`, `redundant` leer, keine Bewegung (`iterations == 0`, solange kein Fang bindet).
- Bestehende Werkzeuge verhalten sich mit ihrer Vorgabeart wie heute. Die Leiste wächst nur um Polygon und Langloch.
- GUI-Tests bei 1920×1080, nie 800×600.
- Prüfen: `cargo test -p <crate> --target-dir C:/Users/denni/AppData/Local/Temp/claude/D--Testumgebung/9257fb9d-a130-4b88-86e3-d09eb7b412f8/scratchpad/target-e2 -j 2`; am Ende `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check`. `cargo fmt --all` vor jedem Commit. Nichts pushen.
- Commit-Nachrichten deutsch, enden mit Leerzeile und `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Nach jeder Task Notiz in `D:\Vault\Vault\Projekte\PlanoraCAD\` nach `CLAUDE.md` des Projekts.
- Alte Tests, die neues Verhalten treffen, werden **bewusst** umgeschrieben (Kommentar „Skizzen 3“, Listen in den Tasks); alle anderen bleiben unverändert grün.

## Entscheidungen des Planers

Die Freiheitsgrade und Zustände unten sind mit einem Probelauf gegen den heutigen Solver gemessen (2026-11-02, Wegwerfprogramm außerhalb des Repos); die Tests in Task 2 und 3 halten sie fest.

1. **Tangent reicht, es kommt keine neue Abhängigkeit dazu.** `check_types` erlaubt Linie–Kreis, Linie–Bogen, Kreis–Kreis, Kreis–Bogen, Bogen–Bogen; `eval_kind` hat zwei Zweige. Ohne gemeinsamen Punkt: Abstand gegen Radius, die Seite (`branch`) setzt `initial_branch` beim Anlegen aus der aktuellen Geometrie; also genügt es, die Form exakt tangential zu setzen, bevor die Abhängigkeit entsteht. Mit gemeinsamem Endpunkt (`shared_end`, nur Linie/Bogen): reine Richtungszeile, eine Zeile, kein Zusammenfall mit `ArcRadius`. Grenze, die bleibt: Eine Linie gilt als unendlich, ein Bogen als ganzer Kreis (kommt in `offene-punkte.md`).
2. **Abhängigkeiten je Form:**

   | Form | Kurven | Abhängigkeiten (ohne interne `ArcRadius`) | Freiheitsgrade |
   |---|---|---|---|
   | Rechteck 3 Punkte | 4 Linien `L0..L3` | `Perpendicular(L0,L1)`, `Parallel(L0,L2)`, `Parallel(L1,L3)` | 5 |
   | Rechteck Mitte | 4 Linien, Konstruktion `D0=c0–c2`, `D1=c1–c3`, Punkt `M` | `Horizontal(L0)`, `Vertical(L1)`, `Midpoint(M,D0)`, `Midpoint(M,D1)` | 4 |
   | Kreis 2/3 Punkte | Kreis | nur Fang (`bind_rim`) | 3 |
   | Kreis 2 tangential | Kreis | `Tangent(Li,K)` zweimal | 1 |
   | Kreis 3 tangential | Kreis | `Tangent(Ci,K)` dreimal | 0 |
   | Bogen 3 Punkte | Bogen | nur Fang | 5 |
   | Bogen tangential | Bogen | `Tangent(Bogen,Vorgänger)` am gemeinsamen Punkt | +2 zur Skizze |
   | Polygon, alle Arten | n Linien, Konstruktionskreis `K`, Mitte `M` | `PointOnCurve(ci,K)` für alle n, `Equal(L0,Li)` für i = 1…n−1 | **genau 4** |
   | Langloch gerade | Seiten `S0,S1`, Enden `E0,E1`, Konstruktion Achse | `Tangent(S0,E0)`, `Tangent(S0,E1)`, `Tangent(S1,E1)`, `Tangent(S1,E0)`, `Equal(E0,E1)` | 5 |
   | … Art Gesamt | wie gerade, ohne eigene Punkte (Ruling Task 2) | wie gerade | 5 |
   | … Art Mittelpunkt | dazu Punkt `C`, Achse `A–B` | dazu `Midpoint(C,Achse)` | 5 |
   | Langloch Bogen | Konstruktionsbogen, Außen, Innen, Enden `E0,E1`, alle mit Mitte `M` bzw. `S`,`E` | `Tangent(Außen,E0)`, `Tangent(Außen,E1)`, `Tangent(Innen,E0)`, `Tangent(Innen,E1)`; **kein** `Equal` | 6 |

   Warum so und nicht naheliegender:
   - **Rechteck Mitte:** `add_rectangle` (H,V,H,V) plus zwei Mittelpunkte ist überbestimmt (gemessen: `redundant = 1`). Zwei halbierende Diagonalen machen schon ein Parallelogramm, also tragen nur `L0` und `L1` H/V; die Gegenseiten bleiben von selbst waagerecht und senkrecht. Die Zeichen H und V stehen darum nur an zwei Seiten (Abweichung, kommt in `offene-punkte.md`).
   - **Polygon:** Ecken auf dem Kreis sind Pflicht. Die naheliegende Form „Seiten tangential am Inkreis plus gleiche Längen“ erlaubt bei gerader Seitenzahl eine Raute (gemessen: n = 4 und 6 ergeben `dof = 5`, `redundant = 1`; n ungerade 4), das wäre das Anti-Ziel „nicht mehr regelmäßig“. Darum läuft der eine Konstruktionskreis **bei allen drei Arten durch die Ecken**. Bei „Umbeschrieben“ gibt die Größe den Abstand Mitte–Seite an und wird zu `DistancePointLine(M,L0,v)` statt `Radius(K,v)`; dieses Maß hält die Form (gemessen). Abweichung von Fusion, das dort vermutlich den Inkreis zeigt (**Annahme**), kommt in `offene-punkte.md`.
   - **Langloch Bogen:** Die fünf `ArcRadius` hängen bei konzentrischer Lage voneinander ab (Außen + Innen + E0 + E1 − 2·Achse = 0). Das ist gewollt und unschädlich: `analyze` zählt innere Zeilen nie als `redundant`, und die Rangzählung ergibt 6 (Mitte 2, Radius, Startwinkel, Öffnungswinkel, Breite). Reihenfolge deshalb: erst alle Bögen, dann die Tangenten. `Equal(E0,E1)` wäre überzählig. `Tangent` am gemeinsamen Ende darf nicht entfallen (dort berühren sich beide Kreise, die Radiuszeilen allein halten den Punkt nicht).
   - Bögen liegen gegen den Uhrzeigersinn: gerades Langloch `E1 = Arc(B, p3, p2)`, `E0 = Arc(A, p1, p4)` mit `S0 = p1→p2`, `S1 = p3→p4`; Bogen-Langloch `Außen = Arc(M, os, oe)`, `Innen = Arc(M, is, ie)`, `E0 = Arc(S, is, os)`, `E1 = Arc(E, oe, ie)`.
3. **Mittellinie:** `CurveData.construction: bool` bleibt, `centerline: bool` kommt dazu, mit der Regel `centerline ⇒ construction` und nur an Linien. Dazu `pub enum LineStyle { Normal, Construction, Centerline }` und `CurveData::style()`. So gilt jede heutige Stelle, die `construction` liest, von selbst für die Mittellinie, und die vielen Tests, die `.construction` lesen, bleiben unberührt. Alle Stellen:
   - **Bleiben wie sie sind** (Mittellinie zählt als Konstruktion): `profile.rs:190`, `profile_legacy.rs:72`, `intersect.rs:134` (Stutzen, Dehnen), `sketch_input.rs:156` und `:170`, `:591` (Hilfslinien), `helpers.rs:174`, `:353`, `:555` (Punkte, Kantenkopie).
   - **Ändern:** `model.rs` (`CurveData`, `push_curve`, `set_construction` löscht `centerline`, neu `set_centerline`), `data.rs` (`CurveRecord`, `to_data`, `from_data` mit Prüfung), `format.rs` (`CurveDto.mittellinie`), `convert.rs` (beide Richtungen, `format_with`), `helpers.rs:131/167` (`copy_curve` übernimmt beides), `edit.rs` (zweites Teilstück, siehe unten), `sketch_view.rs:524-530` und `:1143-1145` (Strichpunkt), `state.rs:316` und `undo_labels.rs:358` (Vergleich), `create_dialog.rs:85` (Name in der Achsenliste), `sketch_state.rs` und `sketch_panel.rs` (Schalter).
   - **Fund:** `edit.rs` legt das zweite Teilstück beim Brechen und Stutzen über `add_piece`/`push_curve` an und verliert dabei schon heute `construction`: Aus dem Rest einer Konstruktionslinie wird eine normale Linie, die Profile begrenzt. Für die Mittellinie wäre das das Anti-Ziel; darum übernehmen die Teilstücke künftig den Stil (auch bei Konstruktion; das behebt nebenbei den alten Fehler und wird in `offene-punkte.md` genannt).
   - **Format:** `FORMAT_VERSION = 14`; `format_with` prüft als erste Regel `sk.has_centerline_entry()` (auch gelöschte Einträge, wie der Ellipsenbogen). `CurveDto` schreibt das Feld mit `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`, also bleiben Dateien ohne Mittellinie bytegleich.
4. **Art als Daten:** `Tool` bleibt und bekommt nur `Polygon` und `Slot`; die Art steht in `SketchSession.kinds: ToolKinds` (je Form die zuletzt gewählte). So bleiben alle Tests mit `Tool::Rectangle` gültig, und Vorgabe bedeutet „wie heute“. `R` wählt immer `Rect2P`, `C` immer `CircleCenter`; ein Klick auf das Symbol nimmt die zuletzt benutzte Art (`choose_tool`). `choose_kind` bei schon gewählter Art ändert nichts, bei anderer Art derselben Form bricht es ab wie `set_tool`.
5. **Lagen statt Punkte:** Alle neuen Arten sammeln in `pending_pos` und `pending_snaps`, sie ändern die Skizze erst beim letzten Klick. Das ergibt von selbst einen Rückgängig-Schritt, und Esc lässt nichts zurück (anders als die Mitte des alten Kreises). Stufe = `pending_pos.len() + pending_curves.len()`. Der Fang bindet beim Abschluss wie beim Rechteck: eigener Punkt an der gemerkten Lage (`bind_own`), `attach` macht daraus `Coincident`, `PointOnCurve` usw. Bogen 3 Punkte und Kreisrand: vorhandener Punkt wird geteilt bzw. `bind_rim`. Umbeschriebenes Polygon: ein gefangener Punkt am Seitenmittelpunkt wird `Midpoint(q,L0)`.
6. **Felder als Tabelle** (`ToolKind::stages()`, Index = Stufe, letzte Stufe schließt ab). Neue `FieldKind`: `Depth` (Tiefe, Länge) und `Sides` (Seitenzahl, `ParamUnit::Number`, ganzzahlig 3…64, wird nie Maß). Die Größe steht vor der Seitenzahl, damit Tippen zuerst die Größe trifft und Tab wechselt (**Annahme**). Sperren wirken über die Lage (`kind_locked_position`), die Pläne nehmen nur Lagen; so bleiben Vorschau und Ergebnis eins.

   | Art | Felder je Stufe (Stufe 0 hat nie welche) | Maß beim Abschluss |
   |---|---|---|
   | `Rect3P` | 1: Breite, Winkel; 2: Tiefe | `Distance(c0,c1)`, `Angle` zur Achshilfe wie die Linie, `Distance(c1,c2)` |
   | `RectCenter` | 1: Breite, Höhe | wie `Rect2P` |
   | `Circle2P` | 1: Durchmesser | `Diameter` |
   | `Circle2T` | Stufe = gewählte Linien; 2: Durchmesser | `Diameter` |
   | `Circle3P`, `Circle3T` | keine | – |
   | `Arc3P` | 2: Radius | `Radius` |
   | `ArcTangent` | 1: Radius, Öffnungswinkel | `Radius`, Öffnungswinkel wie heute |
   | `PolygonInscribed` | 1: Radius, Seitenzahl | `Radius(K)` |
   | `PolygonCircumscribed` | 1: Radius, Seitenzahl | `DistancePointLine(M,L0)` |
   | `PolygonEdge` | 1: Länge, Seitenzahl; 2: Seitenzahl | `Distance(c0,c1)` |
   | `SlotCenterToCenter`, `SlotCenterPoint` | 1: Länge; 2: Breite | `Distance(A,B)`, `Diameter(E0)` (Länge immer Mitte zu Mitte, bei `SlotCenterPoint` also das Doppelte des Zeigerabstands) |
   | `SlotOverall` | 1: Länge; 2: Breite | `Distance(A,B) = L − W`, `Diameter(E0) = W`; L = Gesamtlänge, W = Breite (getippt oder vom Zeiger). Fang an einem äußeren Ende bindet `PointOnCurve(q, Ende)`. Ruling Task 2: Äußere Punkte auf dem Vollkreis konnten nach innen springen. |
   | `SlotThreePointArc` | 2: Radius, Öffnungswinkel; 3: Breite | `Radius(Achse)`, Öffnungswinkel an der Achse, `Diameter(E0)` |
   | `SlotCenterPointArc` | 1: Radius; 2: Öffnungswinkel; 3: Breite | wie oben |

   Bei `SlotThreePointArc` legen Start und Ende die Sehne fest, also bestimmt Radius **oder** Öffnungswinkel den Bogen; das zweite getippte Feld zeigt `sketch-field-radius-or-sweep`.
7. **Seitenzahl:** `DrawInput.sides: Option<usize>` gilt über die Stufen, `clear_pending` vergisst sie; `SketchSession.polygon_sides` (Vorgabe 6) wird erst beim Abschluss überschrieben. „Zuletzt benutzt“ heißt also: ein abgebrochenes Polygon zählt nicht.
8. **Tangentialkreise analytisch für Linie, Kreis und Bogen** (Ruling Task 3, ein Bogen zählt als ganzer Kreis): alle Vorzeichen-Kombinationen, bis zu 8. Der Solver mit zwei Startlagen fand nur 2 von bis zu 8 Zweigen und lieferte an 43 % der Stellen nichts. Von allen Lösungen gilt: kleinster Wert aus Abstand Zeiger–Kreislinie plus 0,05·r (Ruling Task 3: sonst gewinnen riesige Kreise, deren Linie überall nah ist), bei Gleichstand (≤ `tol.linear`) kleinster Abstand Zeiger–Mitte. Dieselbe reine Funktion rechnet Vorschau und Klick mit derselben Rohlage. Nach dem Lösen im echten Sketch: Mitte um mehr als `10·tol.linear` gewandert ⇒ abgelehnt (`sketch-tangent-no-solution`), so nimmt der Löser nie still eine andere Lösung. **Ausgewählt** wird nur Linie, Kreis, Bogen (`2 tangential`: nur Linien), sonst Meldung. Beim letzten Klick von `Circle3T` zählt die Kurve unter dem Zeiger; ohne Kurve passiert nichts. `2 tangential`: Der Zeiger wählt den Sektor (Vorzeichen zu beiden Linien), die Projektion auf die Winkelhalbierende gibt die Mitte; parallele Linien: r = Abstand/2, Mitte auf der Mittellinie, ein abweichender getippter Durchmesser wird abgelehnt.
9. **Linienwerkzeug, Ziehen auf dem letzten Punkt:** egui meldet nach Überschreiten der Klickgrenze (6 px) `drag_started`, einen `clicked` gibt es dann nicht; heute tut ein Zug im Linienwerkzeug nichts, und `begin_drag` kehrt außerhalb von `Tool::Select` sofort zurück. Beides kommt sich also nicht in die Quere:
   - In `handle_input` steht im selben Zweig wie `begin_drag` (`drag_started_by(Primary) && !shift && !panning`) zusätzlich `begin_line_arc`. Es greift nur, wenn die Stelle des Drückens (`press_origin`) im Fangradius von `pending[0]` liegt **und** eine Vorgängerkurve dort endet (`tangent_start_curve`, bevorzugt `line_chain`). Sonst bleibt alles wie heute.
   - Ein Klick ohne Zug läuft weiter durch `click_line`, ein Doppelklick beendet weiter, Umschalt+Ziehen schwenkt weiter. Das Ziehen von Kurven im Auswahlwerkzeug ist gar nicht betroffen.
   - Während des Zugs: keine Felder, keine Richtung, Vorschau ist der tangentiale Bogen zur aufgelösten Lage. Loslassen legt den Bogen über dieselbe Funktion wie `ArcTangent` an, dann `pending = [Ende]`, `line_chain = Some(Bogen)`, `line_after_arc = Some(Bogen)`. Liegt das Ende auf der Tangente, entsteht nichts, Meldung `sketch-arc-tangent-straight`, der Linienzug bleibt. Esc und Rechtsklick nehmen nur den Zug zurück (wie beim Rahmen).
   - Die nächste Linie läuft tangential: Solange `line_after_arc` gilt und der Zeiger vor dem Bogenende liegt, liefert `line_direction` unabhängig von der 3°-Grenze `(Tangent(Bogen), Abgangsrichtung)`; Zeiger dahinter oder Strg ⇒ frei. Getippter Winkel löscht das wie heute; nach der Linie ist `line_after_arc` leer.
10. **Liste an der Leiste:** egui hat `Popup`, also kein eigenes Fenster. Je Form mit Arten: `Popup::menu(&button).id(flyout_id(tool)).open_memory(None).align(RectAlign::RIGHT_START).close_behavior(PopupCloseBehavior::IgnoreClicks)`.
    - **Öffnen:** `secondary_clicked()` oder gedrückt seit `FLYOUT_HOLD_SECS = 0.4` (`is_pointer_button_down_on()` und `pointer.press_start_time()`, solange `request_repaint_after(0.05)`), dann `Popup::open_id`. Gemerkt in egui-Daten; das Loslassen danach ist kein Klick auf den Knopf.
    - **Schließen:** Eintrag gewählt (`ui.close()`), Esc (macht `Popup` selbst), sonst bei `pointer.any_pressed()` außerhalb von Liste und Knopf von Hand. `CloseOnClick` taugt nicht: Das Loslassen nach dem Halten zählt als Klick und schlösse die Liste sofort.
    - **Reihenfolge im Bild:** `viewport::show_in` (mit `handle_input`) läuft vor `sketch_panel::show`. Darum liest `handle_input` `sketch_panel::flyout_open(ctx)`: offen ⇒ Esc bricht das Werkzeug nicht ab, und ein Drücken in der Zeichnung setzt `box_aborted`, es wird also kein Punkt gesetzt.
    - **Dreieck:** in `strip_button` unten rechts, gefülltes Dreieck 5 px, Farbe wie das Symbol.
11. **Menü und Tastatur:** 17 neue Aktionen ohne Vorgabetaste, Ids nach Task 5. `SketchRectangle` und `SketchCircle` behalten Id und Taste. Ihr Anzeigename wird „2-Punkt-Rechteck“ bzw. der Kreisname, weil die Einstellungen sonst „Rechteck“ neben „3-Punkt-Rechteck“ zeigten; `tool_label` behält „Rechteck“ und „Kreis“ für die Leiste über den alten Schlüssel, die Action-Tabelle zeigt auf neue. Alle Stellen, die Aktionen aufzählen, arbeiten über `Action::ALL` und tragen die Neuen von selbst: `keymap.rs:111-122` (Ids und Namen eindeutig, nicht leer), `:255`, `:326`, `:449`, `keymap_settings.rs:293` (jede Zeile sichtbar, belegbar), `keymap_overview.rs:291`, `keymap_model.rs:350` (filtert auf Modell). Anzupassen ist nur `keymap_sketch_drawing.rs:256` („Name wie der Werkzeugknopf“). Das Menü „Erzeugen“ zeigt bei offener Skizze nach den Skizzen-Einträgen je Form ein Untermenü, ohne Skizze bleibt es wie heute.
12. **Namen** (**Annahme**, aus dem Gedächtnis, kommt in `offene-punkte.md`):

    | `ToolKind` | Aktion | Id | Schlüssel `action-sketch-…` | de | en |
    |---|---|---|---|---|---|
    | `Rect2P` | `SketchRectangle` | `skizze.rechteck` | `rect-2p` | 2-Punkt-Rechteck | 2-Point Rectangle |
    | `Rect3P` | `SketchRect3P` | `skizze.rechteck_3p` | `rect-3p` | 3-Punkt-Rechteck | 3-Point Rectangle |
    | `RectCenter` | `SketchRectCenter` | `skizze.rechteck_mitte` | `rect-center` | Mittelpunkt-Rechteck | Center Rectangle |
    | `CircleCenter` | `SketchCircle` | `skizze.kreis` | `circle-center` | Mittelpunkt-Durchmesser-Kreis | Center Diameter Circle |
    | `Circle2P` | `SketchCircle2P` | `skizze.kreis_2p` | `circle-2p` | 2-Punkt-Kreis | 2-Point Circle |
    | `Circle3P` | `SketchCircle3P` | `skizze.kreis_3p` | `circle-3p` | 3-Punkt-Kreis | 3-Point Circle |
    | `Circle2T` | `SketchCircle2T` | `skizze.kreis_2t` | `circle-2t` | 2-Tangenten-Kreis | 2-Tangent Circle |
    | `Circle3T` | `SketchCircle3T` | `skizze.kreis_3t` | `circle-3t` | 3-Tangenten-Kreis | 3-Tangent Circle |
    | `ArcCenter` | `SketchArcCenter` | `skizze.bogen_mitte` | `arc-center` | Mittelpunkt-Bogen | Center Point Arc |
    | `Arc3P` | `SketchArc3P` | `skizze.bogen_3p` | `arc-3p` | 3-Punkt-Bogen | 3-Point Arc |
    | `ArcTangent` | `SketchArcTangent` | `skizze.bogen_tangential` | `arc-tangent` | Tangentialer Bogen | Tangent Arc |
    | `PolygonCircumscribed` | `SketchPolygonCircumscribed` | `skizze.polygon_umbeschrieben` | `polygon-circumscribed` | Umbeschriebenes Polygon | Circumscribed Polygon |
    | `PolygonInscribed` | `SketchPolygonInscribed` | `skizze.polygon_einbeschrieben` | `polygon-inscribed` | Einbeschriebenes Polygon | Inscribed Polygon |
    | `PolygonEdge` | `SketchPolygonEdge` | `skizze.polygon_kante` | `polygon-edge` | Kantenpolygon | Edge Polygon |
    | `SlotCenterToCenter` | `SketchSlotCenterToCenter` | `skizze.langloch_mitte_mitte` | `slot-center-center` | Langloch Mitte zu Mitte | Center to Center Slot |
    | `SlotOverall` | `SketchSlotOverall` | `skizze.langloch_gesamt` | `slot-overall` | Langloch gesamt | Overall Slot |
    | `SlotCenterPoint` | `SketchSlotCenterPoint` | `skizze.langloch_mittelpunkt` | `slot-center-point` | Langloch Mittelpunkt | Center Point Slot |
    | `SlotThreePointArc` | `SketchSlotThreePointArc` | `skizze.langloch_bogen_3p` | `slot-arc-3p` | Langloch 3-Punkt-Bogen | Three Point Arc Slot |
    | `SlotCenterPointArc` | `SketchSlotCenterPointArc` | `skizze.langloch_bogen_mitte` | `slot-arc-center` | Langloch Mittelpunkt-Bogen | Center Point Arc Slot |

    Formen in der Leiste: „Polygon“/„Polygon“, „Langloch“/„Slot“ (`sketch-tool-polygon`, `sketch-tool-slot`); Reihenfolge `Select, Point, Line, Rectangle, Circle, Arc, Polygon, Slot, Ellipse, Spline, ProjectEdge, Trim, Extend, Break`. Mittellinie: `Action::SketchCenterline`, Id `skizze.mittellinie`, „Mittellinie“/„Centerline“. Hinweise je Art `sketch-kind-<schlüssel>-hint`, ein Satz zur Klickfolge.

## Review Focus

1. Unmögliche Eingaben: zweimal dieselbe Stelle, drei Punkte auf einer Geraden, Breite größer als die Gesamtlänge, Radius kleiner als die halbe Sehne, Bogen-Langloch breiter als der Radius oder fast geschlossen. Erwartet: keine Vorschau, beim Klick nur eine Meldung, Skizze bitgleich, die Form läuft weiter (Task 6, 7, 9, 10).
2. Fang auf vorhandene oder feste Geometrie: Rechteck 3 Punkte zwischen zwei festen Punkten, Polygon-Mitte im Ursprung, Langloch auf Punkten einer anderen Kurve. Erwartet: `Solved`, kein Widerspruch, der Fang bleibt nach dem Ändern eines Maßes erfüllt (Task 6, 9, 10).
3. Abbruch und Rückgängig mitten in einer Form: Esc, Rechtsklick, Werkzeug- oder Artwechsel, Strg+Z, gelöschte Kurve unter `pending_curves`. Erwartet: nichts bleibt zurück; ein Rückgängig nimmt ein ganzes 64-Eck samt Maßen, Wiederholen stellt es bitgleich her (Task 5, 6, 9, 10).
4. Doppelklick und Enter in einer laufenden Art: Der Doppelklick zählt als ein Klick und löscht die gesetzten Lagen nicht, Enter ohne Felder tut nichts; offene Liste bei Tastendruck und beim Klick in die Zeichnung setzt keinen Punkt (Task 5, 11).
5. Zoll und Formeln: Breite, Tiefe und Polygon-Radius in Zoll („1“ wird 25,4 mm, Speichertext „1 in“), Formel bleibt am Maß; die Seitenzahl „6“ bleibt in Zoll 6 und erzeugt nie ein Maß oder einen Speichertext (Task 6, 9).

---

### Task 1: Geometrie der Formen

**Files:**
- Create: `crates/planora-sketch/src/shapes.rs`; `lib.rs` (`pub mod shapes`)
- Test: `crates/planora-sketch/tests/shapes.rs` (neu)

**Interfaces:**
- Produces (`planora_sketch::shapes`, alle rein; `tol` ist `tol.linear`; `None` bei Entartung):
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct ArcPlan { pub center: DVec2, pub start: DVec2, pub end: DVec2 }` (gegen den Uhrzeigersinn, Radius aus `start`; `pub fn sweep(&self) -> f64`, `pub fn radius(&self) -> f64`)
  - `pub fn rect_three_point(a: DVec2, b: DVec2, depth_at: DVec2, tol: f64) -> Option<[DVec2; 4]>`; Reihenfolge a, b, b+n, a+n
  - `pub fn rect_center(center: DVec2, corner: DVec2, tol: f64) -> Option<[DVec2; 4]>`; Reihenfolge wie `RectIds`
  - `pub fn circle_through(a: DVec2, b: DVec2, c: DVec2, tol: f64) -> Option<(DVec2, f64)>`
  - `pub fn arc_through(start: DVec2, end: DVec2, on: DVec2, tol: f64) -> Option<ArcPlan>`; liegt `on` im Uhrzeigersinn, sind Start und Ende getauscht
  - `pub fn tangent_arc(start: DVec2, leaving: DVec2, end: DVec2, tol: f64) -> Option<ArcPlan>`; Mitte auf der Normalen durch `start`, `ρ = |e|² / (2 e·n)` mit `e = end − start`, `n = leaving.perp()`; ρ < 0 tauscht Start und Ende
  - `pub fn tangent_arc_locked(start: DVec2, leaving: DVec2, radius: Option<f64>, sweep: Option<f64>, toward: DVec2, tol: f64) -> Option<DVec2>` liefert das Ende; `toward` wählt die Seite und, wo nicht gesperrt, Radius und Winkel
  - `#[derive(Clone, Debug, PartialEq)] pub struct PolygonPlan { pub center: DVec2, pub corners: Vec<DVec2> }`
  - `pub fn polygon_inscribed(center: DVec2, corner: DVec2, sides: usize, tol: f64) -> Option<PolygonPlan>` (`corners[0] == corner`)
  - `pub fn polygon_circumscribed(center: DVec2, side_mid: DVec2, sides: usize, tol: f64) -> Option<PolygonPlan>` (`side_mid` ist die Mitte von `corners[0]–corners[1]`)
  - `pub fn polygon_edge(a: DVec2, b: DVec2, side_at: DVec2, sides: usize, tol: f64) -> Option<PolygonPlan>` (`corners[0] == a`, `corners[1] == b`, Mitte auf der Seite von `side_at`); `sides` außerhalb 3…64 ⇒ `None`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct SlotPlan { pub a: DVec2, pub b: DVec2, pub radius: f64 }`; `pub fn slot_center_to_center(a, b, width_at: DVec2, tol) -> Option<SlotPlan>`, `slot_overall(o0, o1, width_at, tol)` (`None`, wenn `2r ≥ |o0o1| − tol`), `slot_center_point(c, end, width_at, tol)`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct ArcSlotPlan { pub axis: ArcPlan, pub half_width: f64 }`; `pub fn arc_slot(axis: ArcPlan, width_at: DVec2, tol: f64) -> Option<ArcSlotPlan>` (`h = ||w−m|−R|`; `None`, wenn `h ≤ tol`, `h ≥ R − tol` oder die Enden sich auf der offenen Seite treffen: Lücke `g = 2π − sweep < π` und `2R·sin(g/2) ≤ 2h`)
  - `#[derive(Clone, Debug, PartialEq)] pub enum ShapePlan { Quad([DVec2; 4]), Circle(DVec2, f64), Arc(ArcPlan), Polygon(PolygonPlan), Slot(SlotPlan), ArcSlot(ArcSlotPlan) }`; `impl ShapePlan { pub fn outline(&self, steps_per_turn: usize) -> Vec<Vec<DVec2>> }` (geschlossener Umriss; Zentren und Achse nicht)

- [ ] **Step 1: Tests** – `circle_through((0,0),(10,0),(0,10))` = Mitte (5,5), r = √50; auf einer Geraden und doppelte Punkte `None`. `arc_through((10,0),(0,10),(7.07,7.07))` = Mitte (0,0), Start (10,0), Ende (0,10), `sweep` π/2; mit `on = (−7.07,−7.07)`: Start (0,10), Ende (10,0), `sweep` 3π/2. `tangent_arc((10,0), X, (15,5))`: Mitte (10,5), Start (10,0), Ende (15,5); `(15,−5)`: Mitte (10,−5), Start (15,−5), Ende (10,0); `(20,0)` und `(0,0)` `None`. `tangent_arc_locked` mit Radius 5 und Öffnungswinkel π/2 Richtung (15,5) ergibt (15,5). Rechteck: `rect_three_point((0,0),(10,0),(3,4))` = (0,0),(10,0),(10,4),(0,4); Punkt unterhalb ⇒ negative Seite; `rect_center((0,0),(3,2))` = (−3,−2),(3,−2),(3,2),(−3,2). Polygon: n = 3, 4, 6, 64: alle Seiten gleich (1e-9), alle Ecken gleich weit; `polygon_circumscribed((5,−3),(15,−3),6)`: Mitte der ersten Seite genau (15,−3); `polygon_edge((0,0),(10,0),(5,5),4)` = (0,0),(10,0),(10,10),(0,10), Mitte (5,5); `(5,−5)` ⇒ Mitte (5,−5); `sides` 2 und 65 `None`. Langloch: `slot_center_to_center((0,0),(40,0),(20,6))` r = 6; `slot_overall((0,0),(40,0),(20,6))`: a = (6,0), b = (34,0); Breite ≥ Länge `None`; `slot_center_point((0,0),(20,0),(5,6))`: a = (−20,0), b = (20,0). `arc_slot` mit R = 50: Zeiger bei R+5 und bei R−5 je h = 5; h ≥ R `None`; `sweep` 350° mit h = 5 `None`, 300° gut. `outline` eines Langlochs ist geschlossen, jeder Stoß unter 1e-9. Nie NaN bei `tol`-kleinen Eingaben.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch --test shapes` grün.
- [ ] **Step 5: Commit** „Skizze: Geometrie für Rechteck-, Kreis-, Bogenarten, Polygon und Langloch“.

### Task 2: Formen mit ihren Abhängigkeiten

**Files:**
- Modify: `crates/planora-sketch/src/shapes.rs` (Operationen), `helpers.rs` (`atomically` wird `pub(crate)`), `lib.rs` (Re-Export der Ids)
- Test: `crates/planora-sketch/tests/shapes_constraints.rs` (neu)

**Interfaces:**
- Consumes: Task 1; `Sketch::add_point`, `add_line`, `add_arc`, `add_circle`, `add_constraint`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct CenterRectIds { pub rect: RectIds, pub center: PointId, pub diagonals: [CurveId; 2] }`
  - `pub struct PolygonIds { pub center: PointId, pub circle: CurveId, pub points: Vec<PointId>, pub lines: Vec<CurveId> }` (`Clone, Debug, PartialEq`)
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SlotStyle { CenterToCenter, CenterPoint }`
  - `pub struct SlotIds { pub centers: [PointId; 2], pub sides: [CurveId; 2], pub caps: [CurveId; 2], pub axis: CurveId, pub mid: Option<PointId> }`
  - `pub struct ArcSlotIds { pub center: PointId, pub ends: [PointId; 2], pub axis: CurveId, pub outer: CurveId, pub inner: CurveId, pub caps: [CurveId; 2] }`
  - `impl Sketch { pub fn add_rect_three_point(&mut self, corners: [DVec2; 4]) -> Result<RectIds, SketchError>; pub fn add_rect_center(&mut self, center: DVec2, corner: DVec2) -> Result<CenterRectIds, SketchError>; pub fn add_polygon(&mut self, plan: &PolygonPlan) -> Result<PolygonIds, SketchError>; pub fn add_slot(&mut self, plan: &SlotPlan, style: SlotStyle) -> Result<SlotIds, SketchError>; pub fn add_arc_slot(&mut self, plan: &ArcSlotPlan) -> Result<ArcSlotIds, SketchError> }`, alle atomar, Abhängigkeiten nach Entscheidung 2, in dieser Reihenfolge.

- [ ] **Step 1: Tests** – Tabelle je Form (Rechteck 3 Punkte schräg bei 0,4 rad, Rechteck Mitte, Polygon n = 3, 4, 5, 6, 8, 12, 64, je drei Langlöcher schräg bei 0,5 rad, Bogen-Langloch): `solve()` ergibt `Solved`, `redundant` leer, `conflicting` leer, `iterations == 0` und `dof` = 5, 4, **4**, 5, 5, 5, 6. Ein Test `a_second_rectangle_hv_pair_would_be_redundant` hält den Grund für Entscheidung 2 fest (`Horizontal(L2)` dazu ⇒ `redundant` nicht leer). Genau eine Fläche: `profiles().profiles.len() == 1`, Achse, Kreis, Diagonalen sind Konstruktion. **Maß ändern hält die Form** (Fehler unter 1e-6 und `Solved`): Rechteck 3 Punkte, `Distance` an beiden Seiten 50 und 7: Winkel bleibt 90°, Gegenseiten gleich. Rechteck Mitte: Breite 50: `center` bleibt Mitte beider Diagonalen, H/V erfüllt. Polygon n = 3, 4, 6, 64: `Radius` 200, 2, 37, außerdem `Distance(c0,c1)` 33 und bei n = 6 `DistancePointLine(M,L0)` 10 (Seite 11,547): Seiten gleich, Ecken gleich weit, Drehung und Lage ändern das nicht. Langloch: `Diameter(E0)` 30 und 2, `Distance` 100 und 5: Radien gleich, Seiten parallel zur Achse, Seite tangential. Bogen-Langloch: Breite 20 und 3, `Radius(Achse)` 120 und 30: Außen = R+h, Innen = R−h, beide Enden r = h, alles konzentrisch. Ziehen einer Polygon-Ecke um 5 mm: bleibt regelmäßig, `drag_failed` falsch. Breite über den Grenzwert: Skizze meldet nicht `Solved` (Langloch-Länge 0, Innenradius ≤ 0), der Stand bleibt. Messtest `#[ignore]`: 64-Eck, `Radius` ändern < 20 ms im Release (gemessen rund 6 ms).
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch --test shapes_constraints` grün, Messtest grün.
- [ ] **Step 5: Commit** „Skizze: Rechteck, Polygon und Langloch als Formen mit Abhängigkeiten“.

### Task 3: Tangentialkreise

**Files:**
- Create: `crates/planora-sketch/src/tangent_circles.rs`; `lib.rs` (`pub mod tangent_circles`)
- Test: `crates/planora-sketch/tests/tangent_circles.rs` (neu), `crates/planora-sketch/tests/tangent_circles_perf.rs` (neu, `#[ignore]`)

**Interfaces:**
- Consumes: `geom`.
- Produces:
  - `pub fn tangent_circle_two_lines(sk: &Sketch, l0: CurveId, l1: CurveId, pointer: DVec2, diameter: Option<f64>) -> Option<(DVec2, f64)>` (Entscheidung 8)
  - `pub fn tangent_circle_three(sk: &Sketch, curves: [CurveId; 3], pointer: DVec2) -> Option<(DVec2, f64)>`: analytisch für jede Mischung (alle acht Vorzeichen, Determinante fast 0 entfällt, r > tol, ohne Doppelte); Auswahl nach Entscheidung 8
  - `pub fn is_tangent_candidate(sk: &Sketch, c: CurveId) -> bool` (Linie, Kreis, Bogen)

- [ ] **Step 1: Tests** – Zwei Linien: x-Achse und y-Achse, Zeiger (3,5): Mitte auf (t,t), Sektor des Zeigers; Zeiger (−3,5) anderer Sektor; `diameter` 10 ⇒ r = 5 genau; parallele Linien y = 0 und y = 10: r = 5, Mitte (Zeiger.x, 5); Zeiger auf der Linie `None`. Drei Linien (3-4-5-Dreieck): Zeiger innen ⇒ Inkreis r = 1; Zeiger außen ⇒ Ankreis der Seite; zwei parallele und eine Querlinie: zwei Lösungen, Zeiger wählt; drei parallele `None`. Gemischt (Linie, Kreis, Bogen, Werte aus dem Probelauf: Linie y = 0, Kreis (0,30) r 10, Bogen um (30,30) r 8, Startwert (15,10)): Lösung erfüllt alle drei unter 1e-6 und liegt am Zeiger. **Keine stille andere Lösung:** Der Rückgabewert, als Kreis mit drei `Tangent` in die echte Skizze gelegt, wandert beim `solve` unter `10·tol`, `redundant` leer, `dof` des Kreises 0; die vorhandenen Kurven bewegen sich nicht, auch wenn sie frei sind. Gleiche Eingabe ergibt bitgleich dasselbe; `sk` bleibt unverändert (`revision`). Keine Lösung (drei konzentrische Kreise) `None`. Messtest: 200 Zeigerlagen, Median < 4 ms im Release.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch` und Messtest grün.
- [ ] **Step 5: Commit** „Skizze: Tangentialkreise analytisch und über den Solver“.

### Task 4: Mittellinie und Format 14

**Files:**
- Modify: `crates/planora-sketch/src/model.rs`, `data.rs`, `helpers.rs` (`copy_curve`), `edit.rs` (`add_piece`-Aufrufer und `push_curve` bei :422, :454: Stil der Quelle), `crates/planora-project/src/format.rs`, `convert.rs`, `crates/planora-app/src/sketch_state.rs`, `sketch_panel.rs`, `sketch_view.rs` (`style_shapes`), `state.rs:316`, `undo_labels.rs:358`, `create_dialog.rs:85`, `commands.rs`, `keymap/defaults.rs`, `icons.rs` (`HandIcon::Centerline`, `HAND_DRAWN`), `de.ftl`/`en.ftl` (`action-sketch-centerline`, `undo-centerline` „Mittellinie umschalten“/„Toggle centerline“, `create-axis-centerline`, `sketch-err-data-centerline`, Hinweis)
- Test: `crates/planora-sketch/tests/model.rs`, `data.rs`, `trim.rs`, `extend_break.rs`, `crates/planora-project/tests/centerline_format.rs` (neu), `crates/planora-app/tests/sketch_centerline.rs` (neu), `undo_labels.rs`; bewusst: `assert_eq!(FORMAT_VERSION, 13)` wird 14 in `dimensions_format.rs`, `drawings_format.rs`, `elliptic_arc_format.rs`, `format2.rs`, `format3.rs`, `parameter_format.rs`, `parts_format.rs`, `rollbacks_format.rs`, `section_format.rs`, `sheet_format.rs`, `step_format.rs`, `units_format.rs`; `elliptic_arc_format::format_14_is_too_new` prüft 15 (Name mit)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum LineStyle { Normal, Construction, Centerline }`; `impl CurveData { pub fn style(&self) -> LineStyle }`, Feld `pub centerline: bool`
  - `impl Sketch { pub fn set_centerline(&mut self, c: CurveId, on: bool) -> Result<(), SketchError> /* nur Linien; an: beide Felder wahr; aus: beide falsch */; pub fn has_centerline_entry(&self) -> bool /* auch gelöschte */ }`; `set_construction` setzt `centerline = false`
  - `CurveRecord.centerline: bool`; `from_data` lehnt `centerline` ohne `construction` oder an einer Nicht-Linie ab
  - `CurveDto.mittellinie: bool` (Entscheidung 3), `FORMAT_VERSION = 14`
  - `impl SketchSession { pub fn can_toggle_centerline(&self, sk: &Sketch) -> bool /* eine Linie gewählt */; pub fn toggle_centerline_if_possible(&mut self, sk: &mut Sketch) }` (je Linie umschalten, andere Kurven bleiben)
  - `fn style_shapes(path: Vec<Pos2>, stroke: Stroke, style: LineStyle) -> Vec<Shape>`: Centerline über `Shape::dashed_line_with_offset(&path, stroke, &[12.0, 2.0], &[3.0, 3.0], 0.0)`
  - `Action::SketchCenterline`, ohne Vorgabetaste; `pub fn centerline() -> String` im Panel

- [ ] **Step 1: Tests** – Skizze: `set_centerline` an Kreis `WrongType`; Mittellinie im Rechteck teilt keine Fläche und begrenzt kein Profil (`profiles` wie mit Konstruktion, auch `profile_legacy`-Weg); `split_params` ohne Konstruktion ignoriert sie, Stutzen schneidet an ihr wie an Konstruktion; `copy_curve` (Spiegeln, Muster) behält den Stil. **Fund:** Brechen und Stutzen in der Mitte einer Konstruktionslinie und einer Mittellinie: beide Teile behalten den Stil. Rundlauf `to_data`/`from_data` gleich; `from_data` mit `centerline` ohne `construction` und am Bogen ist Fehler. Format: Mittellinie ⇒ `required_format == 14`, Text enthält `"format": 14` und `"mittellinie": true`, lädt gleich; gelöschte Mittellinie ⇒ 14; ohne Mittellinie: Text enthält das Wort nicht, Format wie vorher (bytegleich zu einem zuvor geschriebenen Text, vorhandene Fixture in `format_voll.rs` bleibt); Format 15 zu neu; Konstruktion ⇒ `required_format` unverändert. App: Schalter im Skizzenfeld bei 1920×1080 gesperrt ohne Linie, an bei Linie, wirkt auf die Auswahl, zweimal zurück zu normal; `X` auf Mittellinie ⇒ normal; ein Rückgängig-Schritt „Mittellinie umschalten“; Kürzel belegbar; `style_shapes` Centerline liefert abwechselnd lange und kurze Striche (Länge gemessen), Normal ein Linienzug, Construction wie `path_shapes`; `axis_candidates` listet sie unter eigenem Namen und das Drehen um sie gelingt; Schalter passt in die Feldbreite 250, englisch.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch -p planora-project -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Mittellinie als dritter Linienstil, Format 14 nur wenn nötig“.

### Task 5: Arten am Werkzeug, Stufen, Aktionen

**Files:**
- Create: `crates/planora-app/src/sketch_kinds.rs`; `lib.rs`
- Modify: `sketch_state.rs` (`Tool::Polygon`, `Tool::Slot`; Felder; `click` schickt `planned_kind()` an `click_kind`; `clear_pending`, `Backup`, `forget_removed`, `finish_tool`), `sketch_cursor.rs` (`resolve_unlocked`: bei `picks_curves` nur `Snap::Curve`; `align_skip` für geplante Lagen), `sketch_input.rs` (`FieldKind::Depth`, `Sides`; `key`, `stage_fields`, `read_field`, `field_value`, `locked_position`, `commit_typed` filtert `Sides`), `sketch_inline.rs` (`suggestion` ganzzahlig für `Sides`), `sketch_view.rs` (`draw_preview` zuerst `kind_preview`; `pending_curves` hervorgehoben), `sketch_panel.rs` (`TOOL_ORDER`, `tools()`, `tool_label`, `tool_hint`, Fuß nutzt `active_hint`), `shell.rs` (`STRIP_TOOLS = 14`), `icons.rs` (`HandIcon::Polygon`, `Slot`), `keymap/defaults.rs` (17 Aktionen, `sketch_kind`, `for_kind`), `commands.rs` (`run_action_inner`, `observe_sketch` mit `for_kind`), `de.ftl`/`en.ftl` (Namen nach Entscheidung 12, Hinweise, `sketch-field-depth`, `sketch-field-sides`, `sketch-field-sides-range`, `sketch-field-radius-or-sweep`, `undo-polygon` „Polygon zeichnen“/„Draw polygon“, `undo-slot` „Langloch zeichnen“/„Draw slot“)
- Test: `crates/planora-app/tests/sketch_kinds.rs` (neu); bewusst: `sketch_english.rs` (Namensliste), `sketch_fields.rs:307` (Titel der Leiste), `keymap_sketch_drawing.rs:256` (`SKETCH_KEYS`: Name der Aktion ist der Artname, Name des Knopfs bleibt „Rechteck“), Icon-Tests

**Interfaces:**
- Consumes: `planora_sketch::shapes::ShapePlan`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum ToolKind { … 19 Werte nach Entscheidung 12 … }`; `impl ToolKind { pub const ALL: &'static [ToolKind]; pub fn tool(self) -> Tool; pub fn of(tool: Tool) -> &'static [ToolKind]; pub fn is_classic(self) -> bool /* Rect2P, CircleCenter, ArcCenter */; pub fn stages(self) -> &'static [&'static [FieldKind]]; pub fn label(self) -> String; pub fn hint(self) -> String; pub fn action(self) -> Action; pub fn picks_lines_only(self) -> bool }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct ToolKinds { pub rectangle: ToolKind, pub circle: ToolKind, pub arc: ToolKind, pub polygon: ToolKind, pub slot: ToolKind }`; `Default`: `Rect2P`, `CircleCenter`, `ArcCenter`, `PolygonCircumscribed` (**Annahme** Fusion-Vorgabe), `SlotCenterToCenter`
  - Felder `SketchSession`: `kinds: ToolKinds`, `pending_curves: Vec<CurveId>`, `polygon_sides: usize` (6), `line_arc: Option<LineArcDrag>`, `line_after_arc: Option<CurveId>` (die beiden letzten füllt Task 8); `#[derive(Clone, Copy, Debug, PartialEq)] pub struct LineArcDrag { pub start: PointId, pub prev: CurveId }`
  - `impl SketchSession { pub fn kind_of(&self, tool: Tool) -> Option<ToolKind>; pub fn planned_kind(&self) -> Option<ToolKind> /* aktive Art, nicht klassisch */; pub fn choose_kind(&mut self, sk: &mut Sketch, kind: ToolKind) /* Entscheidung 4 */; pub fn kind_stage(&self) -> usize; pub fn picks_curves(&self) -> bool; pub fn active_hint(&self) -> String; pub(crate) fn click_kind(&mut self, sk: &mut Sketch, cur: &DrawCursor, prior: &Sketch) -> Result<(), Msg>; pub fn kind_plan(&self, sk: &Sketch, at: DVec2) -> Option<ShapePlan>; pub fn kind_preview(&self, sk: &Sketch, at: DVec2) -> Option<Vec<Vec<DVec2>>>; pub(crate) fn kind_locked_position(&self, sk: &Sketch, cur: &DrawCursor) -> Option<DVec2>; pub(crate) fn kind_field_value(&self, sk: &Sketch, kind: FieldKind) -> Option<f64>; pub(crate) fn bind_own(&mut self, sk: &mut Sketch, own: &[PointId], hits: &[DrawCursor]) -> Result<(), Msg> }`
  - `click_kind`: nicht letzte Stufe ⇒ Lage und `remember` anhängen; letzte ⇒ `finish_kind`, das in einem Zug anlegt, bindet, löst (`solve_checked`), `created` und `edit_label` setzt und `clear_pending` ruft. Arten späterer Tasks: `finish_kind` lehnt mit `sketch-kind-pending` ab, Task 10 entfernt den Zweig samt Schlüssel.
  - `DrawInput.sides: Option<usize>`; `key: (Option<Tool>, Option<ToolKind>, Vec<PointId>, usize, usize, bool)` (Lagen, Kurven, Bogenzug).
  - `Action::SketchRect3P` … nach Entscheidung 12, Bereich `Sketch`, `[]`; `impl Action { pub fn sketch_kind(self) -> Option<ToolKind>; pub fn for_kind(kind: ToolKind) -> Action }`; `sketch_tool()` liefert auch für sie das Werkzeug, also bleiben sie wiederholbar. In der Tabelle zwischen `SketchCircle` und `SketchProjectEdge`.
  - `Created::Polygon { ids: PolygonIds, inscribed: bool }`, `Created::Slot(SlotIds)`, `Created::ArcSlot(ArcSlotIds)` samt `undo_label`.

- [ ] **Step 1: Tests** – `ToolKind::ALL` hat 19, `ToolKind::of` deckt sie ohne Lücke; `Line`, `Ellipse` usw. leer; jede Art hat genau eine Aktion, `for_kind(k).sketch_kind() == Some(k)`, Namen de und en nicht leer und verschieden, Hinweise nicht leer. **Alles belegbar:** für jede Art `km.set(k.action(), 0, Taste)` gelingt, Taste gedrückt ⇒ Werkzeug und Art gewählt, Name in der Einstellungsliste; Vorgaben: `R` = `Rect2P`, `C` = `CircleCenter`, 17 Aktionen ohne Taste, Konfiguration ohne Änderung bleibt leer. `R` nach `Rect3P` wählt `Rect2P`; Klick auf das Symbol nach `Rect3P` behält `Rect3P`; neue Sitzung wieder Vorgabe; Artwechsel mitten in einer Form verwirft die Lagen, gleiche Art behält sie; „Letztes Skizzenwerkzeug wiederholen“ nach `Rect3P` wählt `Rect3P` (`keymap_sketch_drawing.rs`-Muster). **Vorgabe wie heute:** alle Tests von `sketch_state.rs`, `keymap_sketch_drawing.rs` laufen ohne Änderung bis auf die genannten; Leiste bei 1920×1080 passt mit 14 Symbolen. **Review-Focus 3:** Esc, Rechtsklick und Werkzeugwechsel in Stufe 1 einer geplanten Art: `pending_pos` leer, Skizze bitgleich, kein Rückgängig-Schritt (`Rect3P` reicht, die Maschine ist dieselbe). `after_history_jump` leert `pending_curves`; `forget_removed` nimmt tote Kurven heraus. **Review-Focus 4:** Doppelklick in Stufe 1: genau eine Lage mehr, Lagen bleiben (`finish_tool` tut bei `planned_kind` nichts); Enter ohne Felder tut nichts.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Arten als Daten am Werkzeug, Kürzel für jede Art“.

### Task 6: Rechteck- und Kreisarten

**Files:**
- Modify: `crates/planora-app/src/sketch_kinds.rs` (`Rect3P`, `RectCenter`, `Circle2P`, `Circle3P`, `Circle2T`, `Circle3T`), `sketch_input.rs` (`dimension_kind`: `Created::Rectangle` + `Depth`, `Angle` über `lines[0]` und `axis_reference`; die Strahl-Rechnung des Linienwerkzeugs wird `locked_ray(from, basis, cur, len, angle)`, damit `Rect3P` sie teilt), `de.ftl`/`en.ftl` (`sketch-circle-collinear`, `sketch-tangent-kinds`, `sketch-tangent-lines-only`, `sketch-tangent-no-solution`)
- Test: `crates/planora-app/tests/sketch_kinds_rect_circle.rs` (neu)

**Interfaces:**
- Consumes: Task 1, 2, 3, 5; `attach`, `bind_rim`, `axis_reference`.
- Produces: `kind_plan`, `kind_locked_position`, `kind_field_value`, `finish_kind` für diese Arten. Zwischenvorschau: `Rect3P` Stufe 1 Linie zum Zeiger; `Circle3P` Stufe 1 Linie, Stufe 2 der Kreis; `Circle2T`/`Circle3T` vor der letzten Stufe keine, die gewählten Kurven sind hervorgehoben.

- [ ] **Step 1: Tests** (ohne Oberfläche über `click`/`commit_input`; je eine Fahrt über Rohereignisse bei 1920×1080) – `Rect3P`: Klicks (0,0), (10,0), (3,4): vier Linien mit `Perpendicular`, zweimal `Parallel`, `dof` passt, schräg bei (0,0), (10,5) genauso; Felder: Breite 30 und Winkel 30° ⇒ zweite Ecke auf dem Strahl, `Distance` und `Angle`; Tiefe 12 ⇒ `Distance(c1,c2)` 12; Zeiger auf der anderen Seite kehrt die Seite um. `RectCenter`: Klicks (5,5), (8,7): Mitte `center`, Diagonalen Konstruktion, zwei `Midpoint`, Breite 20 ⇒ Ecken bei x = 5 ± 10, Höhe entsprechend, Vorzeichen vom Zeiger. `Circle2P`: Klicks (0,0), (10,0): Mitte (5,0), r 5; Durchmesser 30 ⇒ r 15, `Diameter`; Fang auf zwei vorhandenen Punkten ⇒ beide `PointOnCurve`. `Circle3P`: Vorschau erst in Stufe 2, Ergebnis Umkreis; drei Fangpunkte ⇒ drei `PointOnCurve`. `Circle2T`: zwei Linien anklicken, Bogen anklicken ⇒ `sketch-tangent-lines-only`, Durchmesser 10 ⇒ r 5, zwei `Tangent`, `Diameter`, `dof` passt; Zeiger im anderen Sektor andere Lage. `Circle3T`: drei Linien, Linie+Kreis+Bogen; **Vorschau gleich Ergebnis:** `kind_plan` an der Rohlage und das gesetzte Ergebnis weichen unter 1e-6 ab, kein Punkt der Skizze wandert; gleiche Kurve zweimal wird nicht aufgenommen; Ellipse ⇒ `sketch-tangent-kinds`; keine Lösung ⇒ Meldung `sketch-tangent-no-solution`, Skizze bitgleich, die zwei Kurven bleiben gewählt. Klick ohne Kurve in Stufe 2 von `Circle3T` tut nichts. **Review-Focus 1:** gleiche Stelle zweimal, drei Punkte auf einer Geraden, 3-Punkt-Rechteck ohne Tiefe: keine Vorschau, Meldung, Lagen bleiben, Skizze bitgleich. **Review-Focus 2:** `Rect3P` zwischen zwei `Fixed`-Punkten: `Coincident` an beiden, `Solved`, Breite ändern ⇒ abgelehnt, nicht kaputt; Mitte auf dem Ursprung ⇒ `Fixed`. **Review-Focus 3:** ein Rückgängig nimmt jede Form samt Maßen und Fang. **Review-Focus 5:** Zoll: Breite „1“ ⇒ 25,4 mm, Speichertext „1 in“, Formel `Breite / 2` bleibt am Maß. Getippter Wert überlebt Zeigerbewegung; ungültiger Text blockiert den Abschluss. Strg: kein Fang, keine Bindung.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Rechteck 3 Punkte und Mitte, Kreis 2 und 3 Punkte, 2 und 3 tangential“.

### Task 7: Bogen 3 Punkte und tangential

**Files:**
- Modify: `crates/planora-app/src/sketch_kinds.rs` (`Arc3P`, `ArcTangent`), `sketch_cursor.rs` (`tangent_start_curve`), `de.ftl`/`en.ftl` (`sketch-arc-tangent-start`, `sketch-arc-tangent-straight`, `sketch-arc-radius-chord`)
- Test: `crates/planora-app/tests/sketch_kinds_arc.rs` (neu)

**Interfaces:**
- Consumes: Task 1, 5; `leaving_direction`.
- Produces:
  - `pub(crate) fn tangent_start_curve(sk: &Sketch, p: PointId, prefer: Option<CurveId>) -> Option<CurveId>`: `prefer`, wenn dort endend, sonst die jüngste Linie oder der jüngste Bogen, die in `p` enden.
  - `pub(crate) fn create_tangent_arc(&mut self, sk: &mut Sketch, start: PointId, prev: CurveId, cur: &DrawCursor, prior: &Sketch) -> Result<CurveId, Msg>`: Plan aus `tangent_arc`, Punkte anlegen, bei getauschter Lage `add_arc(mitte, ende, start)`, `Tangent(Bogen, prev)`, Fang am Ende, lösen, `created` und `edit_label`. Auch Task 8 nutzt sie.
  - `ArcTangent` Stufe 0: gefangener Punkt nötig; `pending_pos = [Start]`, `auto_tangent = Some(prev)`. Lagen bei Sperren aus `tangent_arc_locked`. Radius-Sperre bei `Arc3P`: Durchgangspunkt auf der Mittelsenkrechten, Seite vom Zeiger; liegt der Zeiger weiter als r von der Sehnenmitte, gilt der große Bogen.

- [ ] **Step 1: Tests** – `Arc3P`: Klicks (10,0), (0,10), (7.07,7.07): Bogen 90°, kein Tausch sichtbar: Enden liegen an den geklickten Stellen, Vorschau ab Stufe 2; Durchgang auf der anderen Seite ⇒ 270°; Fang an Start und Ende teilt die `PointId`, Fang im dritten Klick bindet über `bind_rim`; Radius 10 ⇒ `Radius`, r genau; Radius kleiner als die halbe Sehne ⇒ `sketch-arc-radius-chord`, nichts entsteht (**Review-Focus 1**); Start gleich Ende ⇒ Meldung. `ArcTangent`: Start auf dem Ende einer Linie: Vorschau und Ergebnis gleich (1e-6), `Tangent` entsteht, `Solved`, `redundant` leer, keine Bewegung; nach links und nach rechts vom Linienende (gespeichert gegen den Uhrzeigersinn, Start und Ende getauscht) beide tangential, auch nach `Distance` an der Linie 100; am Bogenende; am Anfang einer Linie in deren Gegenrichtung (`leaving_direction`). Radius 5 und Öffnungswinkel 90° ⇒ beide Maße, Hilfslinien wie der Bestand; Zeiger auf der Tangente: keine Vorschau, Klick meldet `sketch-arc-tangent-straight`. **Spec:** Start auf freier Fläche, auf einem freien Punkt, auf der Mitte einer Linie, auf einer Kreismitte: Statuszeile `sketch-arc-tangent-start`, es entsteht nichts, die Stufe bleibt 0. Ein Rückgängig-Schritt je Bogen, Name „Bogen zeichnen“.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Bogen 3 Punkte und tangential“.

### Task 8: Tangentialer Bogen aus dem Linienwerkzeug

**Files:**
- Modify: `crates/planora-app/src/sketch_kinds.rs` (`begin_line_arc`, `finish_line_arc`), `sketch_state.rs` (`click_line` leert `line_after_arc`; `cancel`: laufender Zug fällt allein weg), `sketch_cursor.rs` (`line_direction` nach Entscheidung 9), `sketch_input.rs` (`stage_fields`: während des Zugs leer), `sketch_view.rs` (`handle_input`, `draw_preview`)
- Test: `crates/planora-app/tests/sketch_line_arc.rs` (neu)

**Interfaces:**
- Consumes: Task 5 `LineArcDrag`, Task 7 `tangent_start_curve`, `create_tangent_arc`.
- Produces:
  - `impl SketchSession { pub fn begin_line_arc(&mut self, sk: &Sketch, at: DVec2, radius: f64) -> bool; pub fn finish_line_arc(&mut self, sk: &mut Sketch, raw: DVec2, radius: f64) -> Result<(), String> }`
  - `handle_input`: Zweig nach Entscheidung 9; Abschluss bei `response.drag_stopped()` und nicht `esc`, wie beim Rahmen.

- [ ] **Step 1: Tests** (Rohereignisse bei 1920×1080, Muster `sketch_trim.rs` `play`) – Zwei Klicks zeichnen eine Linie; Drücken auf dem letzten Punkt, Ziehen, Loslassen: `Line` und `Arc`, `Tangent` am gemeinsamen Punkt, `Solved`; danach läuft der Linienzug vom Bogenende, die nächste Linie ist exakt tangential mit `Tangent`, auch wenn der Zeiger 20° daneben steht; dritte Linie wieder frei. Zeiger hinter dem Bogenende: frei, ohne `Tangent`. Strg beim Weiterzeichnen: frei. Zwei Bögen nacheinander per Ziehen. **Ohne Vorgänger:** nur Startpunkt gesetzt, dann Ziehen auf ihm: nichts entsteht, nächster Klick zeichnet wie heute. Ziehen woanders im Linienwerkzeug: nichts, heutiges Verhalten. **Klick unverändert:** Klickfolge bleibt bitgleich zu heute (Abhängigkeiten, Punkte), Klick auf den letzten Punkt meldet wie heute, Doppelklick beendet, Bewegung unter der Klickgrenze bleibt Klick. Umschalt+Ziehen schwenkt, Mitte schwenkt, Ziehen im Auswahlwerkzeug zieht weiter Kurven (`sketch_curve_drag.rs` unverändert grün). Loslassen auf der Tangente: `sketch-arc-tangent-straight`, Linienzug läuft. Esc während des Zugs: kein Bogen, Linienzug läuft; Esc danach beendet ihn. Rechtsklick während des Zugs desgleichen. Während des Zugs keine Felder und keine Richtungszeichen; Felder danach leer und neu. Fang am Bogenende bindet. Rückgängig: Bogen ein Schritt „Bogen zeichnen“, Linie ein Schritt; die Linie davor bleibt. Bogen auf Linie mit `Fixed`-Enden: `Solved`, keine Bewegung.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Ziehen auf dem letzten Punkt zeichnet einen tangentialen Bogen“.

### Task 9: Polygon

**Files:**
- Modify: `crates/planora-app/src/sketch_kinds.rs` (`PolygonCircumscribed`, `PolygonInscribed`, `PolygonEdge`), `sketch_input.rs` (`dimension_kind`: Radius nach `inscribed`, Länge), `sketch_view.rs` (Vorschau über `kind_preview`, nichts Neues)
- Test: `crates/planora-app/tests/sketch_kinds_polygon.rs` (neu)

**Interfaces:**
- Consumes: Task 1, 2, 5 (`PolygonIds`, `Created::Polygon`, `DrawInput.sides`).
- Produces: `fn current_sides(&self) -> usize` (`input.sides`, sonst `polygon_sides`); Abschluss setzt `polygon_sides = current_sides()`. Bindung: `M`, `corners[0]`, `corners[1]` über `bind_own`; umbeschrieben: gefangener Punkt am Seitenmittelpunkt ⇒ `Midpoint(q, lines[0])`.

- [ ] **Step 1: Tests** – Alle drei Arten mit 3, 6, 64 Seiten: `dof` genau 4, `Solved`, `redundant` leer; Konstruktionskreis durch die Ecken, Umriss eine Fläche. Umbeschrieben: Zeiger auf (15,−3) bei Mitte (5,−3): Mitte der ersten Seite dort; getippter Radius 10 ⇒ `DistancePointLine` 10, kein `Radius`; Einbeschrieben: Radius 20 ⇒ `Radius(K)` 20, `corners[0]` auf dem Strahl zum Zeiger; Kante: Länge 12 ⇒ `Distance` 12, Stufe 2 wählt die Seite. **Maß ändern** in der App hält die Form (`set_dimension_input` 40, dann 4). Seitenzahl: Vorgabe 6; Tab wechselt zwischen Größe und Seitenzahl; „8“ ⇒ Vorschau mit 8 Seiten, nächstes Polygon wieder 8; abgebrochenes zählt nicht; „2“, „65“, „6,5“, „abc“ ⇒ `sketch-field-sides-range`, Abschluss abgelehnt; Kante: Seitenzahl aus Stufe 1 gilt in Stufe 2. **Review-Focus 5:** in Zoll bleibt „6“ sechs Seiten, Größe „1“ ist 25,4 mm; `Sides` erzeugt kein Maß und keine Formel. **Review-Focus 1:** Zeiger auf der Mitte: Meldung, nichts entsteht. **Review-Focus 2:** Mitte im Ursprung (`Fixed`), `dof` 2 im Rest, Radius ändern bleibt regelmäßig; Mitte auf Linienmitte bindet. **Review-Focus 3:** ein Rückgängig nimmt das ganze 64-Eck samt Maß, Name „Polygon zeichnen“, Wiederholen bitgleich. Ganzes Polygon ziehen (Auswahlwerkzeug, Kette per Doppelklick): bleibt regelmäßig, Radius bleibt, ein Schritt.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Polygon umbeschrieben, einbeschrieben, über eine Kante“.

### Task 10: Langloch

**Files:**
- Modify: `crates/planora-app/src/sketch_kinds.rs` (fünf Arten; Zweig `sketch-kind-pending` entfällt), `sketch_input.rs` (`dimension_kind` für `Created::Slot` und `Created::ArcSlot`; der Öffnungswinkel teilt die Hilfslinien-Rechnung mit `Created::Arc`), `de.ftl`/`en.ftl` (`sketch-slot-too-wide`, Schlüssel `sketch-kind-pending` entfällt)
- Test: `crates/planora-app/tests/sketch_kinds_slot.rs` (neu)

**Interfaces:**
- Consumes: Task 1, 2, 5.
- Produces: Stufen nach Entscheidung 6. Zwischenvorschau: gerade Arten Stufe 1 Linie; Bogenarten: Stufe bis zur Achse Linie, dann die Achse als Bogen. Bindung über `bind_own`: `centers`, bei `SlotCenterPoint` zusätzlich `mid`; `SlotOverall` bindet einen gefangenen Punkt an den äußeren Enden über `PointOnCurve(q, Ende)`; Bogenarten `center` und `ends`. Die Breitenstufe bindet nicht.

- [ ] **Falt-Wächter (Ruling Task 2):** `pub fn folded_over(before: &Sketch, after: &Sketch) -> bool` in `planora-sketch` (rein): wahr, wenn nach einem Maß die Richtung einer lebenden Linie oder der Vektor Mitte→Start eines lebenden Bogens umkehrt (Skalarprodukt < 0). Faltet ein Längenmaß (`Distance`, `DistancePointLine`, `Diameter`, `Radius`, auch `Radius` der Bogenachse) die Skizze beim Setzen, versucht die App denselben Wert in 8 gleichen Schritten vom alten Stand aus (Ruling Task 7: ein großer Sprung lässt den Löser umklappen, in kleinen Schritten bleibt die Lage); faltet es dann noch, lehnt sie ab, der Stand bleibt, Meldung `sketch-dimension-folds`. Winkelmaße prüft sie nicht. Tests: tangentialer Bogen an einer 8-mm-Linie, Linie auf 100 mm: Bogen behält die Richtung, keine Spitze (auch `ArcCenter`); Bogen-Langloch Breite 130 bei R 60 abgelehnt, Stand bitgleich; gerades Langloch Breite größer als Mittenabstand bleibt erlaubt; Rechteck-Maße und vorhandene Maßtests unverändert grün.
- [ ] **Step 1: Tests** – Je Art eine Klickfolge mit den Werten aus Task 1, auch schräg: Kurven wie Entscheidung 2, Vorschau gleich Ergebnis (1e-6), `dof` 5 bzw. 6, `Solved`, `redundant` leer, `iterations == 0`, **genau ein Profil**, Extrudieren gelingt (`planora_feature`, Muster `profile_pick.rs`). Felder: Länge 40, Breite 12 ⇒ `Distance`, `Diameter`, Werte exakt; bei `SlotOverall` Mitten 28 (`Distance(A,B)` 28), Enden außen 40 auseinander; `SlotCenterPoint` Länge 40 ⇒ Mitten 40 auseinander, `C` genau dazwischen; `SlotCenterPointArc` Radius 50, Öffnungswinkel 90°, Breite 10; `SlotThreePointArc` Radius 50 oder Öffnungswinkel, beides ⇒ `sketch-field-radius-or-sweep`. **Maß ändern hält die Form** in der App: Breite und Länge hin und her, Bogenradius, Öffnungswinkel; Seiten tangential, Enden Halbkreise (Winkelfehler < 1e-6). **Review-Focus 1:** Breite ≥ Gesamtlänge, Breite ≥ doppelter Radius, Öffnungswinkel 350° bei großer Breite, gleiche Stelle zweimal: keine Vorschau, `sketch-slot-too-wide` bzw. Meldung, nichts entsteht. Länge per Maß unter die Breite (Gesamt) ⇒ abgelehnt, Stand bleibt. **Review-Focus 2:** Enden auf vorhandenen Punkten: `Coincident`, Mitte im Ursprung: `Fixed`; Breite ändern hält die Bindung. **Review-Focus 3:** ein Rückgängig, Name „Langloch zeichnen“; Esc in jeder Stufe hinterlässt nichts. Trimmen einer Seite und Löschen des Bogens lassen keine toten Abhängigkeiten (`Solved`). Alle 19 Arten bedienbar: keine liefert noch `sketch-kind-pending` (Schlüssel fehlt in beiden `.ftl`).
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Langloch mit fünf Arten“.

### Task 11: Dreieck, Liste, Menü „Erzeugen“

**Files:**
- Modify: `crates/planora-app/src/sketch_panel.rs` (`strip_button`, `strip_field`, Liste), `sketch_view.rs` (`handle_input`: `flyout_open`), `app.rs` (`create_menu` ruft `sketch_kinds_menu`)
- Test: `crates/planora-app/tests/sketch_flyout.rs` (neu), `keymap_sketch_drawing.rs`, `sketch_fields.rs`

**Interfaces:**
- Consumes: Task 5 `ToolKind`, `choose_kind`, `kind_of`.
- Produces:
  - `const FLYOUT_HOLD_SECS: f64 = 0.4`; `pub fn has_flyout(tool: Tool) -> bool`; `pub fn flyout_open(ctx: &egui::Context) -> bool`; `pub fn flyout_entries(ctx: &egui::Context) -> Vec<(String, egui::Rect)>` (für Tests, wie `strip_buttons`)
  - `pub fn strip_title(km: &Keymap, se: &SketchSession, tool: Tool) -> String`: bei Formen mit Arten Name und Taste der zuletzt benutzten Art, sonst wie heute. Hinweis beim Überfahren: `kind.hint()`.
  - `fn sketch_kinds_menu(ui: &mut egui::Ui, ws: &mut Workspace)`: nur bei offener Skizze, je Form `ui.menu_button(tool_label(tool), …)`, Einträge `icons::source_entry_keys(ui, icons::sketch_tool_icon(tool), &kind.label(), &keymap::shortcut_hint(&ws.keymap, kind.action()), true)`, Klick ⇒ `commands::run_action(ws, kind.action())`, `ui.close()`.
  - Liste nach Entscheidung 10; offene Liste schließt auch, wenn eine Taste das Werkzeug wechselt.

- [ ] **Step 1: Tests** (1920×1080, Rohereignisse) – Dreieck: `has_flyout` genau für Rechteck, Kreis, Bogen, Polygon, Langloch. Linksklick auf Rechteck wählt die zuletzt benutzte Art und öffnet nichts. Rechtsklick öffnet die Liste rechts neben dem Knopf, im Fenster: alle Arten mit Namen in der Reihenfolge von `ToolKind::of`, Kürzel sichtbar (`R`), neu belegte Taste erscheint. Eintrag wählen: Art und Werkzeug gesetzt, Liste zu, Klick auf das Symbol später behält sie. **Halten:** 0,5 s gedrückt, loslassen: Liste bleibt offen, Werkzeug nicht umgeschaltet; 0,2 s ist ein gewöhnlicher Klick. **Review-Focus 4:** offene Liste mit laufendem Linienzug: Esc schließt nur die Liste, Linienzug läuft; zweites Esc bricht ab. Drücken in die Zeichnung schließt die Liste und setzt keinen Punkt; danach zählt der nächste Klick. Taste `L` bei offener Liste wählt die Linie und schließt sie. Tippen in ein Zeichenfeld öffnet und schließt nichts. Liste englisch. Dunkel und hell ohne feste Farben. Andere Art derselben Form während einer laufenden Form: Lagen weg, gleiche Art: bleiben. **Menü:** bei offener Skizze Untermenüs mit allen 19 Einträgen und Kürzeln, Klick wählt die Art wie die Liste; ohne Skizze Menü wie heute (vorhandene Tests zu `create_menu` unverändert). Überfahren der Leiste zeigt `strip_title` (bewusst in `sketch_fields.rs:307`).
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Dreieck und Liste der Arten, Einträge im Menü Erzeugen“.

### Task 12: Doku und Gesamtprüfung

**Files:** `docs/funktionen.md` (Abschnitt „Skizzen“: alle Arten, Liste an der Leiste, Ziehen im Linienwerkzeug, Mittellinie), `CHANGELOG.md` (ein Absatz in Laiensprache wie der Skizzen-Eintrag; „Eine Skizze mit Mittellinie kann ältere Programmversionen nicht mehr öffnen“), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 3“: alle **Annahme**-Stellen samt Namen und Vorgabe `PolygonCircumscribed`; Rechteck Mitte trägt H/V nur an zwei Seiten; Polygon-Kreis läuft auch umbeschrieben durch die Ecken, Größe dort als Abstand Mitte–Seite; Teilstücke behalten nun den Stil, auch Konstruktion; tangential gilt an Linie als unendlich, am Bogen als ganzer Kreis; `SlotThreePointArc` nimmt Radius oder Öffnungswinkel, nicht beides; `SlotCenterPoint`-Länge ist Mitte zu Mitte; numerischer Tangentialkreis findet Lösungen aus zwei Startlagen am Zeiger, nicht alle; Felder ruhen während des Bogenzugs; abgebrochenes Polygon merkt die Seitenzahl nicht; Grenzen und Folgeideen aus den Tasks)

- [ ] **Step 1:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --target-dir … -j 2`, Messtests `cargo test --release -p planora-sketch --test shapes_constraints -- --ignored` und `--test tangent_circles_perf -- --ignored`, `cargo deny check`, zusätzlich voller Testlauf in WSL (Regel: vor dem Merge lokal Windows und WSL) – alles grün. Rundlauf: vorhandene Fixture laden und ohne Änderung speichern bleibt bytegleich.
- [ ] **Step 2: Commit** „Doku: Skizzen 3 (Zeichenvarianten)“.

Hinweis: Handtest per `tools/gui-drive.ps1` bei 1920×1080 läuft danach auf Dennis' Go, nicht in dieser Aufgabe.
