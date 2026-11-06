# Skizzen 5: Muster, weitere Abhängigkeiten und Maße – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rechteck- und Kreismuster, die änderbar bleiben und der Vorlage folgen; Konzentrisch, Kollinear, Glatt; ein Bemaßen-Werkzeug mit waagerechten, senkrechten und Durchmessermaßen zur Mittellinie; dazu die Gesamtlänge des Langlochs als echtes Maß, Doppelklick auf ein Maß in der Zeichnung und Rasterfang, der an vorhandene Punkte bindet.

**Architecture:** `planora-sketch` bekommt zwei neue Einheiten ohne egui: `pattern.rs` (Muster als eigene Art im Modell, reine Rechnung der Lagen, Exemplare als vom Löser gehaltene Variablen) und `dimension.rs` (reine Wahl des Maßes aus Auswahl und Zeiger). Sechs neue Abhängigkeitsarten kommen in `constraint.rs`. Das Dateiformat steigt auf 15, aber nur für Skizzen, die ein Muster oder eine der neuen Arten enthalten. In der App laufen Muster und Bemaßen als eigene Werkzeuge mit eigenem Zustand wie Verschieben/Kopieren; die Griffe gehen über `sketch_grips.rs`.

**Tech Stack:** Rust-Workspace, egui 0.36, Fluent (`de.ftl`/`en.ftl`, `tr!`), serde.

**Spec:** `docs/superpowers/specs/2026-11-03-skizzen-muster-design.md`

## Global Constraints

- Verhalten, Namen und Kürzel wie Autodesk Fusion; Abweichungen in `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 5“) mit Grund. Annahmen über Fusion sind mit **Annahme** markiert.
- **graphify zuerst:** Wer baut oder prüft, fragt vor dem Lesen von Dateien den Wissensgraphen in `D:\Testumgebung\PlanoraCAD` (`graphify query "<Frage>" --budget 2000`, `graphify path "A" "B"`, `graphify explain "<Symbol>"`) und liest dann die Dateien, auf die er zeigt. `graphify-out/` wird nie eingecheckt (lokal ausgeschlossen).
- Keine neuen Fremd-Abhängigkeiten; `cargo deny check` bleibt grün; Symbole selbst gezeichnet (`HandIcon`), kein Autodesk-Material.
- Alle Oberflächentexte über `tr!` mit Schlüssel in `crates/planora-i18n/languages/de.ftl` **und** `en.ftl`. Intern mm, Anzeige über `planora_i18n` (Zoll muss gehen). Farben nur über Theme-Tokens (`no_hard_coded_colors.rs`).
- **Dateiformat 15 nur, wenn nötig** (Regel aus Skizzen 1 und 3): `FORMAT_VERSION` wird 15; `required_format` liefert 15 genau dann, wenn eine Skizze einen Mustereintrag oder einen Abhängigkeitseintrag einer neuen Art hat (auch gelöscht, wie bei Mittellinie und Ellipsenbogen). Alle anderen Dateien werden bytegleich geschrieben wie heute; alte Dateien laden unverändert, neue Prüfungen gelten nur für neue Einträge.
- Ein Befehl ist genau ein Rückgängig-Schritt. Esc bricht ab und hinterlässt nichts (auch keinen Revisionssprung).
- Kein Muster und keine neue Abhängigkeit macht eine eindeutige Skizze überbestimmt: nach dem Anlegen `status == Solved`, `conflicting` leer; ist die neue Abhängigkeit schon erfüllt und überzählig, steht sie in `redundant`, die Skizze bleibt gelöst und die Meldung sagt es (Maße werden anzeigend wie heute).
- Bestehendes Verhalten von linearem Muster (`add_linear_pattern`, Knopf im Skizzenfeld), Spiegeln, Rundung, Fase, Versatz, Verschieben, Maßstab bleibt. Alte Dateien damit laufen weiter.
- Geometrie wird über **dichte Wertefolgen** geprüft, nicht an Stichproben; bei jeder Folge wird auf Umklappen (`folded_over`) und Seitenwechsel geprüft. Die Vorschau ruft dieselbe reine Funktion wie der Klick; ein Test vergleicht beide unter 1e-9.
- Tests bei 1920×1080, nie 800×600.
- Prüfen: `cargo test -p <crate> --target-dir C:/Users/denni/AppData/Local/Temp/claude/4c529cdd42b76c49/9257fb9d-a130-4b88-86e3-d09eb7b412f8/scratchpad/target-s5 -j 2`; am Ende `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check -p <crate>` je Crate, `cargo deny check`. **`cargo fmt --all` scheitert unter Windows, darum `cargo fmt -p <crate>` je Crate** vor jedem Commit. Nichts pushen. Vor einem Merge kein GitHub-CI, sondern voller Lauf unter Windows und in WSL.
- Commit-Nachrichten deutsch, enden mit Leerzeile und `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Nach jeder Task Notiz in `D:\Vault\Vault\Projekte\PlanoraCAD\` nach `CLAUDE.md` des Projekts.
- Alte Tests, die neues Verhalten treffen, werden **bewusst** umgeschrieben (Kommentar „Skizzen 5“, Listen in den Tasks); alle anderen bleiben unverändert grün.

## Entscheidungen des Planers

Alle Zahlen sind gegen den heutigen Löser gemessen (2026-11-06, Wegwerfprogramm in einer Kopie des Repos im Scratchpad, Release-Build, nicht eingecheckt). Für die neuen Zeilen wurde die Kopie um genau die Residuen erweitert, die Task 1 baut. „rel“ ist der Unterschied der Freiheitsgrade zur Skizze davor.

1. **Muster brauchen eine eigene Art im Modell und im Format.** Gemessen am heutigen Weg (Kopien mit `Equal`, `Parallel`, `Distance`, wie `add_linear_pattern`), Rechteck mit Maßen als Vorlage:

   | Exemplare | Anlegen | Breite der Vorlage ändern |
   |---|---|---|
   | 10 | 1 ms | 2 ms |
   | 25 | 13 ms | 18 ms |
   | 50 | 104 ms | 132 ms |
   | 100 | 852 ms | 1085 ms |

   100 Exemplare (10×10) brauchen also das Zehnfache des Ziels; jede Strukturänderung rechnet den Rang der ganzen Gruppe dicht. Dazu kennt eine reine Abhängigkeitslösung weder Anzahl noch abgewählte Exemplare noch die Art „Abstand/Gesamtlänge“. **Gewählt: Exemplare sind gehaltene Variablen.** Das Muster steht als `PatternData` in der Skizze; die Lage jedes Exemplars ist eine reine Funktion der Vorlage und der Musterwerte (`pattern_transforms`). Der Löser behandelt die Variablen der Exemplare als Konstanten (keine Spalte, kein Freiheitsgrad, nicht frei) und schreibt sie vor und nach jedem Lauf neu. Gemessen: 10×10 Abstand ändern 0,08 ms, Vorlagenmaß ändern 0,18 ms, 30×30 1,3 ms, 100×100 16 ms; rel 0 beim Anlegen; eine fremde Linie mit `Coincident` an einem Exemplar folgt nach der zweiten Runde (Fehler 1,6e-11). Ein Muster kann die Skizze damit **gar nicht** überbestimmen.
2. **Obergrenze.** Teuer ist danach nur noch die Flächensuche: `profiles()` 100 Rechtecke 5 ms, 900 Rechtecke 62 ms, 10 000 Rechtecke 4,7 s. `PATTERN_MAX_INSTANCES = 900` (ohne Vorlage gezählt mit) und `PATTERN_MAX_CURVES = 3600` (Kurven aller Exemplare). Darüber wird **abgelehnt** mit `sketch-pattern-too-many`. Abweichung von der Spec („darüber wird es Geometrie“): Lose Geometrie kostete bei der Flächensuche genauso viel, sie hülfe nicht. Kommt in `offene-punkte.md`.
3. **Musterwerte sind Maße ohne Zeile.** Neue Art `ConstraintKind::PatternValue(PatternId, PatternSlot, f64)`: `is_dimension()`, keine Residuen, `points()` und `curves()` leer. So laufen Liste, Doppelklick, Formel, Parameter, Zoll, Rückgängig und `solve_formulas` ohne neuen Weg. `PatternSlot`: `Count1, Spacing1, Count2, Spacing2` (Rechteck), `Count, Angle` (Kreis). Anzahl hat `ParamUnit::Number`, ganzzahlig 1…`PATTERN_MAX_INSTANCES`; Abstand Länge, Vorzeichen kehrt die Richtung um, Betrag > `tol.linear`; Winkel `ParamUnit::Angle`, Betrag ≤ 2π, ≠ 0. `length_value()` ist `None` (kein Falt-Wächter, die Exemplare falten nicht). Ein Musterwert lässt sich nicht löschen (Meldung `sketch-pattern-value-keep`) und nicht anzeigend schalten. `set_constraint_value` auf eine Anzahl gleicht die Exemplare an (`sync_pattern`), auch über Formeln.
4. **Lage der Exemplare** (`pattern_transforms`, rein, liefert `Vec<([i32; 2], Similarity)>` ohne abgewählte und ohne die Vorlage):
   - Rechteck: Richtung `PatternDir::SketchX | SketchY | Line(CurveId)` (Linie: normierte Richtung `b − a` zum Zeitpunkt der Rechnung, folgt also), zweite Richtung Vorgabe senkrecht zur ersten (**Annahme**). Schritt `s = v` bei „Abstand“, `s = v / (n − 1)` bei „Gesamtlänge“ (`n = 1`: kein Schritt). Indizes `0..n` oder bei „Symmetrisch“ `−(n−1)..=(n−1)`: Die Anzahl zählt **je Seite** einschließlich Vorlage (**Annahme**: Fusion zählt so; dann gibt es keinen geraden Fall ohne Mitte). Versatz `i·s1·d1 + j·s2·d2`.
   - Kreis: Mitte `PointId` (folgt). „Voll“: Schritt `2π/n`, `k = 0..n`, kein Winkelwert. „Winkel“: Schritt `θ/(n−1)`. „Symmetrisch“: Schritt `(θ/2)/(n−1)`, `k = −(n−1)..=(n−1)` (**Annahme** wie oben).
   - Ein Exemplar hat dieselben Kurvenarten wie die Vorlage, auch Ellipse, Ellipsenbogen und Spline (anders als Spiegeln und lineares Muster; Fusion kann alle). Radius und Halbachsen werden mitgeschrieben, die große Halbachse gedreht. Stil wie die Vorlage, auch nach späterem Umschalten.
5. **Löser** (`solver.rs`): `pattern_vars()` sammelt die Exemplar-Variablen. `build_groups` streicht sie aus `locals` (Zeilen, die nur sie lesen, entfallen), nimmt sie aber in `Group::reads` auf (Zwischenspeicher). `live_variables` zählt sie nicht. `solve_with_soft`: `refresh_patterns()` vor dem Lauf, danach erneut; hat sich ein Exemplarwert geändert und liest eine Gruppe Exemplare, noch eine Runde, höchstens `PATTERN_ROUNDS = 3`. `free_points`/`free_curves`: Ein Exemplarpunkt gilt als frei, wenn sein Vorlagenpunkt frei ist (nur Farbe). `drag` auf einen Exemplarpunkt zieht den Vorlagenpunkt mit der Umkehrung der Lage (Zug an einem Exemplar bewegt die Vorlage, **Annahme**). `point_unconstrained` ist für Exemplarpunkte `false`.
6. **Bindungen an Exemplare, Abwählen, Auflösen.**
   - Fremde Geometrie darf an Exemplare binden (einseitig, Entscheidung 5). Eine Abhängigkeit, deren Gruppe eine Vorlage, Richtungslinie oder Mitte desselben Musters enthält **und** ein Exemplar dieses Musters liest, wird abgelehnt (`sketch-pattern-cycle`), sonst liefe die Schleife nie zusammen. Ein Exemplar kann keine Vorlage, Richtungslinie oder Mitte eines Musters sein (`sketch-pattern-instance-source`).
   - Abgewählte Exemplare stehen in `PatternData::suppressed: Vec<[i32; 2]>`; sie haben keine Kurven. Löschen eines Exemplars über die Oberfläche wählt es ab (alle Kurven dieses Exemplars). Ändert sich die Anzahl, behalten bleibende Exemplare ihre Ids samt Bindungen; wegfallende nehmen ihre Abhängigkeiten mit, und die Meldung nennt die Zahl (`sketch-pattern-dropped`).
   - **Auflösen** (`dissolve_pattern`): Das Muster und seine Werte verschwinden, die Exemplare bleiben als lose Geometrie an ihrer Stelle. Das geschieht beim Löschen einer Vorlage, der Richtungslinie oder der Mitte und vor Stutzen, Dehnen, Brechen, Rundung und Fase an einer Vorlage oder einem Exemplar (Meldung `sketch-pattern-dissolved`; **Annahme**: Fusion löst die Verbindung dort ebenfalls). Die Haken sitzen in `remove_curve`, `remove_point` und `set_curve_kind`; `SketchSession::atomic` vergleicht `pattern_count()` davor und danach und setzt die Meldung.
   - **Zusammenspiel:** Versatz an einer Exemplarkette und Spiegeln eines Exemplars binden einseitig und gehen. Verschieben oder Drehen mit einem Exemplar in der Menge ist gesperrt (`TransformBlock::PatternInstance`, Meldung `sketch-move-pattern-instance`); Vorlage verschieben nimmt die Exemplare über den Löser mit. Skalieren der ganzen Skizze oder einer Menge, die Vorlagen und (wenn vorhanden) Mitte oder Richtungslinie enthält: Abstände mal `k`, Anzahl und Winkel bleiben. Kopieren von Exemplaren oder Vorlage ergibt lose Kopien ohne Muster. `folded_over` und `sweep_crossed_half_turn` übergehen Exemplarkurven. `whole_sketch_set` lässt Exemplare aus.
7. **Konzentrisch = `Coincident` der Mitten**, keine neue Art. Gemessen: Kreis–Kreis–Bogen–Ellipse je rel −2 (16 → 10), `redundant` leer; vierte, transitive Bindung meldet genau sie als überzählig; beide Mitten fest an derselben Stelle: das zweite `Fixed` überzählig, `Solved`; Radius-Folge 0,1…100: 1000/1000. Anzeige: Ein `Coincident` zweier Punkte, die beide Mitte eines Kreises, Bogens, einer Ellipse oder eines Ellipsenbogens sind, heißt in Liste und Ansicht „Konzentrisch“ mit Zeichen `◎` (Ersatz „(o)“ über `symbol`). Teilen beide Kurven schon die Mitte: Meldung `sketch-concentric-already`, nichts angelegt. Linien: `sketch-concentric-kinds`.
8. **Kollinear: neue Art `Collinear(CurveId, CurveId)`** mit den Zeilen `cross(d1,d2)/(|d1||d2|)` und `signed_distance(q, L1)`, `q` der Punkt von `L2`, der kein Punkt von `L1` ist (beide geteilt: `Degenerate`). Gemessen gegen zwei `PointOnCurve`: dichte Folgen (Winkel −170…170° in 1°, Lücke 0,5…200) beide 0 Fehler, 0 Umklapper; große Zufallssprünge (500): zwei `PointOnCurve` kehren `L2` 186-mal um, die gewählten Zeilen nie (113 abgelehnt, Stand bleibt; 126 über `solve_in_steps` erreicht). Gemeinsames Ende: rel −1, `redundant` leer. Schon parallel und auf der Geraden: überzählig gemeldet. Eine Art statt zweier, damit Liste und Löschen wie in Fusion eine Abhängigkeit zeigen; das Format steigt dafür nicht eigens, denn Muster brauchen Format 15 ohnehin (Entscheidung 1).
9. **Glatt: neue Art `Smooth(CurveId, PointId, CurveId, PointId)`** (Kurve, ihr Ende, Kurve, ihr Ende). Zeile: Winkel zwischen der Richtung aus dem ersten Ende in die erste Kurve und der umgekehrten Richtung aus dem zweiten Ende in die zweite, `atan2(cross, dot)`, wie die Tangente am gemeinsamen Ende (die Spitze bei ±π ist keine Lösung). Mindestens eine Kurve ist ein Spline, die andere Linie, Bogen oder Spline; Ellipsen nicht (`sketch-smooth-kinds`). Liegen die Enden auseinander, legt der Befehl zuerst `Coincident` an (nächstes Endenpaar, **Annahme**). Gemessen: Linie–Spline, Bogen–Spline, Spline–Spline je rel −1, `redundant` leer, Restfehler unter 3e-9; Zug am inneren Stützpunkt über ein Gitter 21×21: 441/441 gelöst, nie Spitze. Die Krümmung am Splineende ist null (natürlicher Spline, gemessen 1e-6 numerisch). Darum ist Glatt an **Linie und Spline echt G2**, an einem **Bogen nur G1**: Meldung `sketch-smooth-arc-g1` beim Anlegen, Eintrag in `offene-punkte.md`, Rest kommt mit Teilprojekt 7. Die heutige `Tangent(Bogen, Spline)` lehnt die Typprüfung ab, darum geht es nicht ohne neue Art.
10. **Waagerecht/senkrecht: neue Arten `DistanceX(a, b, v)`, `DistanceY(a, b, v)`**, Zeile `branch · (b − a)ₓ − v`, `branch` aus der Lage beim Anlegen (wird schon gespeichert). Gemessen über 3000 Schritte (dicht 0,05…50, Zufall, mit wechselnder Länge der Nachbarlinie) gegen Hilfspunkt mit `Horizontal`, `Vertical`, `Distance`: beide 60 echte Ablehnungen, neue Zeile 0 Seitenwechsel, Hilfspunkt 1; Falten 4 gegen 5 (fängt `guard_length` wie heute). Gewählt die neue Art: ein Eintrag statt drei und kein Hilfspunkt, der beim Verschieben, Kopieren und Löschen mitgeführt werden müsste. `DistanceX` an einer Linie mit `Horizontal` und Länge: überzählig gemeldet; `DistanceY` 1 an einer waagerechten Linie: `Conflicting`, abgelehnt.
11. **Durchmesser zur Mittellinie: neue Art `CenterlineDiameter(PointId, CurveId, v)`**, Zeile `2 · branch · signed_distance(p, L) − v`; nur an Mittellinien (`centerline`), sonst `sketch-diameter-centerline`. Gemessen 2000 Schritte 0,3…300 und Zufall: 0 Ablehnungen, 0 Seitenwechsel (Hilfspunkt mit `Symmetric` und `Distance` ebenso gut, aber drei Einträge und ein Hilfspunkt). Text „Ø 30“. **Umschalter** `Sketch::toggle_radius_diameter(id)`: `Radius(c,v) ↔ Diameter(c,2v)`, `DistancePointLine(p,L,v) ↔ CenterlineDiameter(p,L,2v)` (nur an Mittellinien), dieselbe `ConstraintId` und derselbe `branch`; eine Formel wird mit `scaled_formula(text, 2)` bzw. `0.5` umgeschrieben. Kurve gegen Mittellinie: Kreis und Bogen über ihre Mitte, Linie (parallel) über ihren ersten Punkt (**Annahme**).
12. **Langloch „Gesamt“ mit echtem Maß: neue Art `SlotOverall(a, b, cap, v)`**, Zeile `|a − b| + 2·r(cap) − v`. Gemessen 4000 Schritte (L 11…211, W 0,5…59, Zufall): 11 echte Ablehnungen (W ≥ L), 0 falsche Gesamtlängen, 1 Falten (der Wächter fängt es). Der alte Weg mit Scheitelpunkten (`PointOnCurve` an Achse und Kappe, `Distance`) gab in 2411 von 4000 Schritten die innere Lage, ist also weiter verworfen. Das Langloch „Gesamt“ legt ab jetzt `SlotOverall` und `Diameter` an statt `Distance(L − W)`; eine Formel bleibt an der Gesamtlänge. Alte Skizzen behalten ihr `Distance`. Der Eintrag „Gesamtlänge kein eigenes Maß“ in `offene-punkte.md` (Skizzen 3) wird als ersetzt markiert.
13. **Bemaßen** (`dimension.rs`, rein): `Sketch::dimension_plan(picks: &[DimPick], pointer: DVec2) -> Option<DimPlan>`. Regeln (Werte gemessen, nie 0):
    - eine Linie: Länge; ein Kreis: Durchmesser; ein Bogen: Radius; Ellipse: Halbachse, deren Achse dem Zeiger näher liegt (**Annahme**).
    - zwei Punkte, Punkt und Kreismitte, zwei Kreise/Bögen (Mitten): **Zeigerlage**: Zeiger in der x-Spanne der beiden Punkte und außerhalb der y-Spanne ⇒ `DistanceX`; in der y-Spanne und außerhalb der x-Spanne ⇒ `DistanceY`; sonst `Distance` (**Annahme** zur Fusion-Regel). Ist `|dx| ≤ tol` (bzw. `|dy|`), fällt diese Art aus.
    - Punkt oder Kreis(mitte) und Linie: `DistancePointLine`; ist die Linie Mittellinie: `CenterlineDiameter`.
    - zwei Linien: parallel (innerhalb `tol.angular · 10`) ⇒ `DistancePointLine(L2.a, L1)`, ist eine Mittellinie ⇒ `CenterlineDiameter(Punkt der anderen, Mittellinie)`; sonst `Angle` mit dem Vorzeichen wie `line_angle`.
    - `DimPlan { kind: ConstraintKind /* mit gemessenem Wert */, anchor: DVec2, mode: DimMode }`, `DimMode` ∈ `Aligned, Horizontal, Vertical, PointLine, Radius, Diameter, CenterlineDiameter, CenterlineRadius, Angle, Major, Minor`. Der Umschalter Ø/R wirkt schon in der Vorschau (`DimPlan::toggled()`).
    - Die Lage des Texts wird **nicht** gespeichert; er steht wie jedes Maß am gerechneten Anker (Abweichung: Fusion legt ihn dort ab, wo man klickt; ein Format für Textlagen wäre ein eigener Schritt).
14. **Neue Arten im Kern, Pflichtstellen:** `is_dimension`, `length_value` (die vier Längenarten und `SlotOverall`), `with_value`, `points`, `curves`, `eval_kind`, `measure` (Beträge; `DistanceX` ⇒ `|Δx|`), `check_types`, `check_constraint_value`, `initial_branch`, `satisfied` (Zeile 0 von `Collinear` und `Smooth` sind Winkelzeilen), `constraint_numbers_ok`, `transform.rs::map_ids` und `transformed_value` (Längen mal `k`), dazu in der App `kind_label`, `overlay_text`, `anchors`, `stored_value`, `constraint_unit`, `constraint_name`. Beim Drehen gilt für `DistanceX`/`DistanceY` dieselbe Regel wie für H und V: Was danach nicht erfüllt ist, entfällt (auch in der Kopie). Kompilerfehler an `match` ohne `_` zeigen jede Stelle; dort, wo `_` steht, prüft der Test der Task.
15. **Werkzeuge und Bedienung.**
    - `Tool::Pattern` (Leistenknopf mit Liste wie Rundung/Fase, Arten `ToolKind::PatternRect`, `ToolKind::PatternCircular`, Vorgabe `ToolKinds.pattern = PatternRect`) und `Tool::Dimension`. Leiste 20 Werkzeuge: …`Spline, ProjectEdge, Pattern, Dimension,` | `Trim…Scale`. `STRIP_TOOLS = 20`, `STRIP_H` = 20·36 + 19·4 + 9 + 8 = 813, passt in 1080; keine Liste. Beide `is_drawing_tool()`, nicht `is_modify()`.
    - Muster: Stufen `Picking` (Auswahl, Rahmen, Vorauswahl übernommen) → `Active` (Griffe, Felder, Klick auf ein Exemplar der Vorschau wählt es ab oder wieder an, Klick auf eine Linie im Feld „Richtung“ setzt sie) → Enter/OK legt an, Esc verwirft. Kreismuster: nach der Auswahl Klick auf die Mitte (mit Fang, Raster, Strg wie Teilprojekt 2). Felder: `PatternCount1, PatternSpacing1, PatternCount2, PatternSpacing2, PatternCount, PatternAngle` (neue `FieldKind`), im Skizzenfeld dazu „Abstand/Gesamtlänge“, „Symmetrisch“ je Richtung, „Voll/Winkel/Symmetrisch“. Vorgaben: Richtung 1 Anzahl 3, Richtung 2 Anzahl 1, Abstand = 1,5 × Ausdehnung der Auswahl in dieser Richtung; Kreis Anzahl 6, Voll (**Annahme**). Griffe: Pfeil je Richtung am letzten Exemplar, Zug ändert den Abstand längs der Richtung; Kreis: Griff am Ende des Bogens ändert den Winkel (`sketch_grips::pattern_layout`, `pattern_hit`, `pattern_motion`, rein).
    - **Muster bearbeiten:** Doppelklick auf ein Exemplar oder eine Vorlage im Auswahlwerkzeug öffnet das Werkzeug mit dem bestehenden Muster in `Active`; OK schreibt Werte und Abwahl als **einen** Rückgängig-Schritt (`Sketch::edit_pattern`). Die Werte gehen auch einzeln über die Liste und ihren Doppelklick.
    - Bemaßen: erster Klick wählt (Punkt vor Kurve), zweiter Klick wählt oder setzt; nach dem Setzen öffnet das Maßfeld mit dem gemessenen Wert, Enter legt Maß und Wert als **einen** Schritt an (`EditTarget::Planned(ConstraintKind)`), Esc lässt nichts zurück; das Werkzeug bleibt aktiv. Taste `Tab` oder Knopf „Ø/R“ im Skizzenfeld schaltet Radius/Durchmesser (**Annahme**: Fusion über das Kontextmenü). Überzählig ⇒ anzeigend wie heute.
    - Konzentrisch, Kollinear, Glatt als `ConstraintAction` mit Knöpfen im Skizzenfeld (`ACTIONS` 19): `(0,0,2,0)`, `(0,0,1,1)`, `(0,0,0,2)` mit Kreisen/Bögen/Ellipsen ⇒ Konzentrisch; `(0,2,0,0)` ⇒ Kollinear; ein Spline dabei ⇒ Glatt.
16. **Kürzel, Namen, Menüs** (**Annahme**, aus dem Gedächtnis). Nur `D` hat eine Vorgabetaste (Fusion „Skizzenbemaßung“). `D` steht schon im Bereich Zeichnung; die Bereiche kollidieren nicht.

    | Aktion | Id | Schlüssel `action-sketch-…` | Taste | de | en |
    |---|---|---|---|---|---|
    | `SketchPatternRect` | `skizze.muster_rechteck` | `pattern-rect` | – | Rechteckige Anordnung | Rectangular Pattern |
    | `SketchPatternCircular` | `skizze.muster_kreis` | `pattern-circular` | – | Kreisförmige Anordnung | Circular Pattern |
    | `SketchDimension` | `skizze.bemassen` | `dimension` | D | Skizzenbemaßung | Sketch Dimension |
    | `SketchConcentric` | `skizze.konzentrisch` | `concentric` | – | Konzentrisch | Concentric |
    | `SketchCollinear` | `skizze.kollinear` | `collinear` | – | Kollinear | Collinear |
    | `SketchSmooth` | `skizze.glatt` | `smooth` | – | Glatt | Smooth |

    Menü „Erzeugen“: Rechteckige Anordnung, Kreisförmige Anordnung, Skizzenbemaßung. Die drei Abhängigkeiten wenden die Aktion auf die Auswahl an (wie der Knopf). Listen über `Action::ALL` tragen die neuen von selbst; anzupassen sind `commands.rs` (beide `match`-Arme), `keymap/defaults.rs` (Tabelle und `tool()`), `app.rs::sketch_kinds_menu`.
17. **Mitgenommen:** Doppelklick auf einen Maßtext in der Ansicht öffnet sein Feld wie die Liste: `sketch_view` merkt sich je Bild die Rechtecke der Maßtexte (`LabelHit { id, rect }` in den Temp-Daten), der Doppelklick prüft sie vor der Kettenwahl. **Raster bindet:** Fällt der Rasterfang auf eine Stelle, an der ein lebender Punkt innerhalb `tol.linear` liegt, wird daraus `Snap::Point(p)` (`resolve_free`, nach dem Rasterzweig); Strg schaltet das wie jeden Fang ab.
18. **Annahmen über Fusion** (gesammelt in `offene-punkte.md`): Anzahl je Seite bei „Symmetrisch“; zweite Richtung senkrecht zur ersten; Vorgaben 3×1 und 6 voll; Zug am Exemplar bewegt die Vorlage; Auflösen beim Stutzen und Runden; Konzentrisch als deckende Mitten mit eigenem Namen; Glatt verbindet die nächsten Enden; Zeigerregel für waagerecht/senkrecht; Ellipsenhalbachse nach Zeigernähe; Kurve gegen Mittellinie über Mitte bzw. ersten Punkt; Ø/R mit Tab; Kürzel D; Namen der Tabelle.

## Review Focus

1. Muster in Grenzlage: Anzahl 1 in beiden Richtungen, Abstand so klein, dass sich Exemplare überdecken, Gesamtlänge mit Anzahl 1, Winkel 360° bei „Winkel“ (letztes Exemplar auf der Vorlage), Anzahl über der Obergrenze, alle Exemplare abgewählt. Erwartet: keine NaN, keine doppelte Kurve auf der Vorlage, Meldung statt Absturz, Flächen bleiben gültig (Task 3, 4).
2. Muster und Rückgängig/Formeln: Anzahl über eine Formel von 5 auf 2 und zurück, Abstand mit Parameter in Zoll, Rückgängig nach dem Anlegen und nach dem Bearbeiten. Erwartet: ein Schritt je Befehl, Ids bleibender Exemplare und Bindungen daran bleiben, Speichertext „1 in“, Wiederholen bitgleich (Task 4, 5, 9).
3. Bemaßen an der Grenze der Zeigerregel und an fast senkrechten Punktpaaren: Zeiger genau auf der Kante der Spanne, `dx` knapp über `tol`, zwei Punkte übereinander, Maß, das schon gilt. Erwartet: keine Nullmaße, kein Springen zwischen Arten bei 1 px Bewegung außer an der Kante, überzählig ⇒ anzeigend (Task 6, 8).
4. Neue Abhängigkeiten an fester und geteilter Geometrie: Konzentrisch an einer projizierten Kante, Kollinear mit gemeinsamem Ende und mit einer Achshilfslinie, Glatt zwischen zwei Splines am selben Punkt und an einem Bogen mit fester Mitte, Durchmesser zur Mittellinie mit dem Punkt auf der Achse. Erwartet: `Solved` oder Meldung, nie Widerspruch bei erfüllbarer Lage, nie Umklappen (Task 1, 7).
5. Alte Dateien und Format: jede Datei in `planora-project/tests/fixtures` lädt und schreibt sich bytegleich; eine Skizze mit gelöschtem Muster oder gelöschtem `Collinear` braucht 15; ein Programm mit Format 14 öffnet 15 nicht (vorhandene Ablehnung) (Task 2, 5).

---

### Task 1: Neue Abhängigkeitsarten im Kern

**Files:**
- Modify: `crates/planora-sketch/src/constraint.rs`, `transform.rs` (`map_ids`, `transformed_value`), `data.rs` (`constraint_numbers_ok`), `lib.rs`
- Test: `crates/planora-sketch/tests/constraints_s5.rs` (neu)

**Interfaces:**
- Consumes: Entscheidung 8–12, 14.
- Produces:
  - `ConstraintKind::{Collinear(CurveId, CurveId), Smooth(CurveId, PointId, CurveId, PointId), DistanceX(PointId, PointId, f64), DistanceY(PointId, PointId, f64), CenterlineDiameter(PointId, CurveId, f64), SlotOverall(PointId, PointId, CurveId, f64)}`
  - `impl ConstraintKind { pub fn needs_format_15(&self) -> bool }` (die sechs; Task 3 nimmt `PatternValue` dazu)
  - `impl Sketch { pub fn smooth_ends(&self, a: CurveId, b: CurveId, near: Option<DVec2>) -> Option<(PointId, PointId)>; pub fn toggle_radius_diameter(&mut self, id: ConstraintId) -> Result<(), SketchError> }`

- [ ] **Step 1: Tests** (je `Solved`, `conflicting` leer, Folgen mit `folded_over == false`)
  - Zeilen und rel: `Collinear` mit Lücke rel −2, mit gemeinsamem Ende −1; `Smooth` Linie–, Bogen–, Spline–Spline je −1; `DistanceX`/`DistanceY` −1; `CenterlineDiameter` −1, `measure` = doppelter Abstand; `SlotOverall` auf Langloch mit festem Mittelpunkt, `Horizontal` Achse und `Diameter`: dof 1 → 0.
  - **Dichte Folgen** wie Entscheidung 8–12 (Werte und Schrittzahlen dort): `Collinear` Winkel −170…170° in 1° und Lücke 0,5…200: alle gelöst, `L2` nie gegen `L1` gedreht; `DistanceX` 0,05…50 in 1000 Schritten: Seite von `b − a` unverändert; `CenterlineDiameter` 0,3…300: Seite unverändert; `SlotOverall` L 11…211 und W 0,5…59: `|a−b| + 2r = L` unter 1e-6, Mitten nie vertauscht; `Smooth` Zug-Gitter 21×21: Zeile unter 1e-8, nie Spitze.
  - **Überzählig, nicht widersprüchlich:** `Collinear` nach `Parallel` und `PointOnCurve`; `DistanceX` an Linie mit `Horizontal` und Länge; Konzentrisch transitiv (`Coincident` der Mitten): je in `redundant`, `Solved`. `DistanceY 1` an waagerechter Linie: `Conflicting`.
  - Typprüfung: `Collinear(L, L)`, Kreis in `Collinear`, `Smooth` ohne Spline oder mit Ellipse, `Smooth` mit Punkt, der kein Ende der Kurve ist, `CenterlineDiameter` an normaler Linie, Werte ≤ `tol`, NaN ⇒ Fehler, Skizze unverändert.
  - `smooth_ends`: gemeinsamer Punkt; getrennte Enden ⇒ nächstes Paar; kein Ende in Reichweite (`near`) ⇒ `None`.
  - `toggle_radius_diameter`: Wert verdoppelt/halbiert, `id` und `branch` gleich, zweimal ⇒ bitgleich; `DistancePointLine` an normaler Linie ⇒ Fehler.
  - Verschieben, Drehen um 30° und Skalieren mit `k = 2` einer Menge mit allen neuen Arten: Längen ×2, `DistanceX` beim Drehen entfallen (Bericht `dropped`), Kopie ebenso; `Collinear`, `Smooth` bleiben.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch --test constraints_s5` und alle `planora-sketch`-Tests grün.
- [ ] **Step 5: Commit** „Skizze: Kollinear, Glatt, waagerechte, senkrechte und Durchmessermaße, Gesamtlänge Langloch“.

### Task 2: Neue Arten im Dateiformat

**Files:**
- Modify: `crates/planora-project/src/format.rs` (`ConstraintKindDto`), `convert.rs` (`constraint_to`, `constraint_from`, `FORMAT_VERSION`, `format_with`), `crates/planora-sketch/src/model.rs`
- Test: `crates/planora-project/tests/s5_format.rs` (neu, Muster `centerline_format.rs`)

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - DTO-Arten `Kollinear { a, b }`, `Glatt { a, punkt_a, b, punkt_b }`, `AbstandWaagerecht { a, b, wert }`, `AbstandSenkrecht { a, b, wert }`, `DurchmesserZurAchse { punkt, kurve, wert }`, `LanglochGesamt { a, b, kappe, wert }` (Kommentar „Ab Format 15“).
  - `impl Sketch { pub fn needs_format_15(&self) -> bool }`: ein Abhängigkeitseintrag mit `kind.needs_format_15()` (auch gelöscht) oder ein Mustereintrag (Task 5 ergänzt das). `FORMAT_VERSION = 15`; `format_with` prüft 15 als erste Regel.

- [ ] **Step 1: Tests**
  - Rundlauf je neuer Art `to_data`/`from_data` und durch das Projektformat: gleich, `branch` erhalten.
  - `required_format`: Skizze ohne neue Einträge wie vorher (14, 13, …, 1 je nach Inhalt); mit lebendem oder gelöschtem `Collinear` 15.
  - **Bytegleich:** Jede Datei unter `crates/planora-project/tests/fixtures` laden und schreiben ergibt dieselben Bytes; die Formatnummer im Kopf bleibt.
  - Eine Datei mit Format 15 wird von einem Kopf mit `FORMAT_VERSION` 14 abgelehnt (vorhandener Pfad, Test über die Kopfprüfung).
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-project` grün.
- [ ] **Step 5: Commit** „Projekt: Format 15 für die neuen Abhängigkeiten, nur wenn sie vorkommen“.

### Task 3: Muster im Modell, reine Lage, gehaltene Exemplare im Löser

**Files:**
- Create: `crates/planora-sketch/src/pattern.rs`; `lib.rs`
- Modify: `model.rs` (`patterns: Arena<PatternData>`), `solver.rs` (Entscheidung 5), `constraint.rs` (`PatternValue`), `helpers.rs` (`copy_curve` auch Ellipse und Spline, nur für Muster; `check_copy_sources` bleibt für Spiegeln und lineares Muster)
- Test: `crates/planora-sketch/tests/pattern.rs` (neu)

**Interfaces:**
- Consumes: Entscheidung 1–5.
- Produces:
  - `pub type PatternId = Id<PatternData>;`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum PatternDir { SketchX, SketchY, Line(CurveId) }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum PatternSlot { Count1, Spacing1, Count2, Spacing2, Count, Angle }`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum PatternKind { Rect { dirs: [PatternDir; 2], extent: [bool; 2], symmetric: [bool; 2] }, Circular { center: PointId, mode: CircularMode } }`, `pub enum CircularMode { Full, Angle, Symmetric }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct PatternInstance { pub at: [i32; 2], pub points: Vec<PointId>, pub curves: Vec<CurveId> }` (parallel zu `source_points` bzw. `sources`)
  - `#[derive(Clone, Debug, PartialEq)] pub struct PatternData { pub kind: PatternKind, pub sources: Vec<CurveId>, pub source_points: Vec<PointId>, pub values: Vec<ConstraintId>, pub instances: Vec<PatternInstance>, pub suppressed: Vec<[i32; 2]>, pub(crate) alive: bool }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct PatternPlan { pub kind: PatternKind, pub sources: Vec<CurveId>, pub counts: [u32; 2], pub values: [f64; 2], pub suppressed: Vec<[i32; 2]> }` (`values`: Abstände bzw. `[Winkel, 0]`)
  - `pub const PATTERN_MAX_INSTANCES: usize = 900; pub const PATTERN_MAX_CURVES: usize = 3600; pub(crate) const PATTERN_ROUNDS: usize = 3;`
  - `impl Sketch { pub fn pattern_transforms(&self, plan: &PatternPlan) -> Result<Vec<([i32; 2], Similarity)>, PatternProblem>; pub fn pattern_preview(&self, plan: &PatternPlan, steps_per_turn: usize) -> Result<Vec<([i32; 2], Vec<Vec<DVec2>>)>, PatternProblem>; pub fn add_pattern(&mut self, plan: &PatternPlan) -> Result<PatternId, SketchError>; pub fn pattern(&self, id: PatternId) -> Option<&PatternData>; pub fn patterns(&self) -> impl Iterator<Item = (PatternId, &PatternData)>; pub fn pattern_count(&self) -> usize; pub fn pattern_of_curve(&self, c: CurveId) -> Option<(PatternId, Option<[i32; 2]>)> /* None-Index: Vorlage */; pub(crate) fn refresh_patterns(&mut self) -> bool; pub(crate) fn pattern_vars(&self) -> HashSet<usize> }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum PatternProblem { Empty, TooMany, ZeroStep, BadDirection, InstanceAsSource, Cycle }`
  - `add_pattern`: Kopien an den Lagen aus `pattern_transforms`, `PatternValue` je Wert (bei „Voll“ ohne Winkel, bei Anzahl 1 in Richtung 2 trotzdem beide Werte), einmal lösen.

- [ ] **Step 1: Tests**
  - Lagen: Rechteck 10×6, Richtung X 3 × 15, Y 2 × 10: fünf Exemplare, Versatz (15,0), (30,0), (0,10), (15,10), (30,10). Gesamtlänge 30 bei 3 ⇒ Schritt 15. Symmetrisch X 3 ⇒ Indizes −2…2. Richtung `Line(L)` mit `L` unter 30°: Versatz längs `L`. Negativer Abstand kehrt um. Kreis um (0,0), 6 voll: Drehungen k·60°; „Winkel“ 90° bei 4: 30° Schritt; symmetrisch 90° bei 3: −45…45° in 22,5°.
  - **Vorschau = Ergebnis:** `pattern_preview` gleich den Linienzügen der angelegten Exemplare unter 1e-9; abgewählte fehlen in beiden.
  - **Folgen:** Vorlage mit Maßen, Breite 1…40 in 400 Schritten: jedes Exemplar ist die verschobene Vorlage unter 1e-9; Richtungslinie dreht −180…180° in 1°: Versatz folgt; Kreismitte wandert: Exemplare folgen. Abstand 0,5…100: Exemplare folgen, `revision` neu, Vorlage unverändert bitgleich.
  - **Löser:** rel 0 beim Anlegen (10×10, 30×30), `redundant` leer, Exemplarpunkte nicht in `free_points` bei voll bestimmter Vorlage, bei freier Vorlage drin. Fremde Linie mit `Coincident` an einem Exemplar folgt nach `solve` (Abstand unter 1e-9). `drag` am Exemplarpunkt zieht die Vorlage um denselben Versatz. Abhängigkeit zwischen Vorlage und eigenem Exemplar ⇒ `PatternProblem::Cycle` beim Anlegen der Abhängigkeit (Fehler, Skizze unverändert).
  - **Review-Focus 1:** Anzahl 1/1 ⇒ kein Exemplar, Muster bleibt; Abstand 0, NaN ⇒ `ZeroStep`; „Winkel“ mit `|θ| ≥ 2π − tol.angular` gilt wie „Voll“ (Schritt `2π/n`, sonst läge das letzte Exemplar auf der Vorlage; **Annahme**): bei 4 vier Lagen 0°, 90°, 180°, 270°; 31×30 Rechtecke ⇒ `TooMany`; alle abgewählt ⇒ gültig, keine Kurve. Überdeckende Exemplare (Abstand 3 bei Breite 10): `profiles()` ohne Panik, Rückgabe endlich.
  - Ellipse, Ellipsenbogen, Spline als Vorlage im Kreismuster: Halbachse gedreht, Spline-Stützpunkte gedreht, unter 1e-9.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch --test pattern`, `solver`, `transform`, `offset_constraints`, `fillet_constraints` grün.
- [ ] **Step 5: Commit** „Skizze: Muster mit gehaltenen Exemplaren“.

### Task 4: Muster ändern, abwählen, auflösen, Zusammenspiel

**Files:**
- Modify: `crates/planora-sketch/src/pattern.rs`, `constraint.rs` (`remove_curve`, `remove_point`, `set_constraint_value`), `model.rs` (`set_curve_kind`, `set_construction`), `transform.rs` (`TransformBlock::PatternInstance`, Skalieren der Abstände, `whole_sketch_set`), `shapes.rs` (`folded_over`, `sweep_crossed_half_turn`)
- Test: `crates/planora-sketch/tests/pattern_edit.rs` (neu)

**Interfaces:**
- Consumes: Task 3.
- Produces:
  - `impl Sketch { pub fn edit_pattern(&mut self, id: PatternId, plan: &PatternPlan) -> Result<PatternEdit, SketchError>; pub fn suppress_instance(&mut self, id: PatternId, at: [i32; 2], on: bool) -> Result<(), SketchError>; pub fn dissolve_pattern(&mut self, id: PatternId) -> Result<(), SketchError>; pub fn pattern_instance_at(&self, plan: &PatternPlan, at: DVec2, radius: f64) -> Option<[i32; 2]> }`
  - `#[derive(Clone, Debug, PartialEq, Default)] pub struct PatternEdit { pub added: usize, pub removed: usize, pub dropped_constraints: usize }`
  - `pub(crate) fn sync_pattern(&mut self, id: PatternId) -> PatternEdit` (aus `set_constraint_value` bei Anzahl)

- [ ] **Step 1: Tests**
  - Anzahl 3 → 5 → 2 → 3 über `set_constraint_value`: bleibende Exemplare behalten `CurveId` und `PointId`; ein `Distance` von fremdem Punkt an Exemplar (1,0) bleibt, an (2,0) fällt beim Schritt auf 2 weg (`dropped_constraints == 1`); Lagen stimmen nach jedem Schritt.
  - `suppress_instance` an und wieder aus: Kurven weg bzw. neu an der richtigen Lage; `remove_curve` eines Exemplars über `suppress_instance` (die Oberfläche ruft es so) lässt das Muster bestehen.
  - **Auflösen:** `remove_curve(Vorlage)`, Löschen der Richtungslinie, Löschen der Mitte, `trim`/`break` an Vorlage und Exemplar, `add_fillet` an einer Exemplarecke: `pattern_count()` sinkt, Exemplare bleiben an Ort und Stelle bitgleich, ihre Variablen sind danach frei (dof steigt), `Solved`.
  - **Zusammenspiel:** Versatz einer Exemplarkette: Kopie folgt, wenn die Vorlage des Musters breiter wird (Abstand unter 1e-6). Spiegeln eines Exemplars: Spiegelbild folgt. Verschieben mit Exemplar in der Menge ⇒ `TransformBlock::PatternInstance`, Skizze unverändert; Vorlage verschieben ⇒ Exemplare folgen. Ganze Skizze skalieren `k = 2`: Abstände ×2, Anzahl gleich, Lagen stimmen; Kreismuster Winkel gleich. Kopieren eines Exemplars: lose Kopie, `pattern_count` gleich. `folded_over` an Kreismuster mit Winkel 90° → 300°: `false`.
  - `edit_pattern`: Art Abstand → Gesamtlänge, Symmetrisch an, neue Abwahl: Ergebnis gleich einem frisch angelegten Muster mit diesem Plan (Lagen unter 1e-9), ein Aufruf = eine `revision`-Änderung der Struktur.
  - `pattern_instance_at`: Klick auf Exemplar (1,1) liefert `[1,1]`, auf die Vorlage `None`, ins Leere `None`.
- [ ] Steps 2–4: FAIL, implementieren, alle `planora-sketch`-Tests grün.
- [ ] **Step 5: Commit** „Skizze: Muster ändern, abwählen und auflösen“.

### Task 5: Muster im Dateiformat und Messtest

**Files:**
- Modify: `crates/planora-sketch/src/data.rs` (`SketchData::patterns`, Prüfungen in `from_data`), `crates/planora-project/src/format.rs` (`PatternDto`, `SketchDto::muster` mit `#[serde(default, skip_serializing_if = "Vec::is_empty")]`, DTO `MusterWert { muster, feld, wert }`), `convert.rs`
- Test: `crates/planora-project/tests/s5_format.rs`, `crates/planora-sketch/tests/pattern_perf.rs` (neu, `#[ignore]`)

**Interfaces:**
- Consumes: Task 3, 4, 2.
- Produces: `pub struct PatternRecord` (wie `PatternData` mit `alive`), `SketchData { …, pub patterns: Vec<PatternRecord> }`; `needs_format_15` zählt jeden Mustereintrag.

- [ ] **Step 1: Tests**
  - Rundlauf eines Rechteck- und eines Kreismusters mit Abwahl, Formel am Abstand und einem gelöschten Muster: gleich; nach dem Laden `solve` ohne Schritt (`iterations == 0`), Lagen bitgleich.
  - `from_data` weist zurück: Exemplar mit falscher Punktzahl, Wert-Id, die kein `PatternValue` desselben Musters ist, Vorlage gelöscht bei lebendem Muster, Exemplar zugleich Vorlage. Alte Daten ohne `patterns` laden wie heute (Fixtures, bytegleich).
  - **Messtest (Release, `--ignored`):** Rechteck mit Maßen, 10×10: Abstand ändern, Vorlagenmaß ändern, je `set_constraint_value` + `solve` + `profiles()`, Median über 20 Läufe unter 100 ms; 30×30 unter 100 ms ohne `profiles()`.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-project` und Messtest grün.
- [ ] **Step 5: Commit** „Projekt: Muster speichern (Format 15 nur mit Muster)“.

### Task 6: Reine Wahl des Maßes

**Files:**
- Create: `crates/planora-sketch/src/dimension.rs`; `lib.rs`
- Test: `crates/planora-sketch/tests/dimension_plan.rs` (neu)

**Interfaces:**
- Consumes: Task 1, Entscheidung 13.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum DimPick { Point(PointId), Curve(CurveId, DVec2) }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum DimMode { Aligned, Horizontal, Vertical, PointLine, Radius, Diameter, CenterlineDiameter, CenterlineRadius, Angle, Major, Minor }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct DimPlan { pub kind: ConstraintKind, pub mode: DimMode, pub anchor: DVec2 }`; `impl DimPlan { pub fn toggled(&self, sk: &Sketch) -> Option<DimPlan> }`
  - `impl Sketch { pub fn dimension_plan(&self, picks: &[DimPick], pointer: DVec2) -> Option<DimPlan> }`

- [ ] **Step 1: Tests**
  - Zwei Punkte (0,0), (40,30): Zeiger (20, 50) ⇒ `Horizontal`, Wert 40; (60, 15) ⇒ `Vertical`, 30; (60, 50) ⇒ `Aligned`, 50. **Dichtes Gitter** 200×200 Zeigerlagen um die Punkte: Art wechselt nur an den Kanten der Spannen, Wert = `measure(kind)` unter 1e-12, nie 0, nie NaN. Punkte übereinander (`dx = 0`): nie `Horizontal`.
  - Linie allein, Kreis ⇒ `Diameter`, Bogen ⇒ `Radius`, Ellipse nach Zeiger. Punkt–Linie ⇒ `PointLine`; Punkt–Mittellinie ⇒ `CenterlineDiameter` Wert 2·Abstand; `toggled()` ⇒ `CenterlineRadius`/`DistancePointLine`, zweimal ⇒ gleich. Kreis–Mittellinie über die Mitte. Zwei parallele Linien ⇒ Abstand; nicht parallel ⇒ `Angle` mit positivem Wert. Zwei Kreise ⇒ Mitten nach Zeigerregel.
  - **Vorschau = Ergebnis:** `add_constraint(plan.kind)` + `solve` lässt die Skizze unbewegt (`iterations == 0`).
  - `sk.revision()` unverändert; gleiche Eingabe bitgleich.
- [ ] Steps 2–4: FAIL, implementieren, Test grün.
- [ ] **Step 5: Commit** „Skizze: Wahl des Maßes aus Auswahl und Zeiger“.

### Task 7: Konzentrisch, Kollinear, Glatt in der Oberfläche

**Files:**
- Modify: `crates/planora-app/src/sketch_state.rs` (`ConstraintAction`, `available_actions`, `build`, `apply`), `sketch_panel.rs` (`ACTIONS`), `sketch_view.rs` (`kind_label`, `overlay_text`, `anchors`, Zeichen Konzentrisch), `icons.rs` (`sketch_action_icon`, `HandIcon` für die drei), `de.ftl`, `en.ftl`
- Test: `crates/planora-app/tests/sketch_constraints_s5.rs` (neu)

**Interfaces:**
- Consumes: Task 1.
- Produces: `ConstraintAction::{Concentric, Collinear, Smooth}`; `pub fn is_concentric(sk: &Sketch, kind: &ConstraintKind) -> bool` (in `sketch_view.rs`).

- [ ] **Step 1: Tests** (Fenster 1920×1080)
  - Zwei Kreise wählen ⇒ Knopf „Konzentrisch“ an, Klick ⇒ ein `Coincident`, Liste zeigt „Konzentrisch“, Ansicht `◎`; ein Rückgängig nimmt es. Kreis und Linie ⇒ Knopf aus. Gemeinsame Mitte ⇒ Meldung `sketch-concentric-already`, `revision` gleich.
  - Zwei Linien ⇒ Kollinear, rel −2; Spline und Bogen ⇒ Glatt mit Meldung `sketch-smooth-arc-g1`; Spline und Linie ⇒ Glatt ohne Meldung; getrennte Enden ⇒ zusätzlich ein `Coincident`, ein Rückgängig-Schritt für beides.
  - Überzählig ⇒ Meldung `sketch-redundant`, Skizze gelöst.
  - `sketch_speaks_english`/Sprachtest: neue Schlüssel in beiden Dateien.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app --test sketch_constraints_s5` grün.
- [ ] **Step 5: Commit** „Skizze: Konzentrisch, Kollinear und Glatt im Skizzenfeld“.

### Task 8: Werkzeug Skizzenbemaßung

**Files:**
- Create: `crates/planora-app/src/sketch_dimension.rs`
- Modify: `sketch_state.rs` (`Tool::Dimension`, `DimToolState`), `sketch_view.rs` (Vorschau: Text am Zeiger, Hilfsstriche für waagerecht/senkrecht; Anzeige der neuen Arten, Ø), `sketch_panel.rs` (`EditTarget::Planned(ConstraintKind)`, Knopf „Ø/R“ an Maßen mit Umschalter), `sketch_input.rs`, `de.ftl`, `en.ftl`
- Test: `crates/planora-app/tests/sketch_dimension_tool.rs` (neu)

**Interfaces:**
- Consumes: Task 6; `apply`, `set_dimension_input`, `guard_length`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Default)] pub struct DimToolState { pub picks: Vec<DimPick>, pub plan: Option<DimPlan>, pub toggled: bool }`
  - `impl SketchSession { pub fn click_dimension(&mut self, sk: &Sketch, at: DVec2, radius: f64); pub fn dimension_preview(&self, sk: &Sketch) -> Option<DimPlan>; pub fn apply_planned(&mut self, sf: &mut SketchFeature, kind: ConstraintKind, text: &str, scope: &ParamScope) -> Result<ConstraintId, String>; pub fn toggle_radius_diameter(&mut self, sf: &mut SketchFeature, id: ConstraintId) -> Result<(), String> }`
  - `apply_planned`: Anlegen, Wert, Formel und Lösen in einem `atomic`, also ein Rückgängig-Schritt; Wächter wie `apply` (`prepare_dimension_change`, `guard_length`, überzählig ⇒ anzeigend).

- [ ] **Step 1: Tests** (1920×1080)
  - Taste D ⇒ Werkzeug; Klick Punkt, Klick Punkt, Zeiger über der Spanne ⇒ Vorschau „40“ waagerecht; Klick ⇒ Feld mit 40; „55“ Enter ⇒ `DistanceX` 55, Punkte stehen 55 auseinander, **ein** Rückgängig nimmt alles. Esc im Feld ⇒ keine Abhängigkeit, `revision` gleich.
  - Punkt und Mittellinie ⇒ Vorschau „Ø 30“; Tab ⇒ „15“; Enter ⇒ `DistancePointLine`. Umschalter an einem bestehenden `CenterlineDiameter` mit Formel `d`: Formel wird `d * 0.5`, Wert halb, ein Schritt.
  - Kreis ⇒ Ø, Bogen ⇒ R; zwei Linien schräg ⇒ Winkel in Grad.
  - Zoll: „1“ ⇒ 25,4 mm, Speichertext „1 in“; Formel bleibt am Maß. Überzähliges Maß ⇒ anzeigend mit Meldung `sketch-became-driven`.
  - **Review-Focus 3:** Zeiger springt 1 px über die Kante der Spanne ⇒ Art wechselt genau dort; zwei Punkte übereinander ⇒ kein waagerechtes Maß; schon geltendes Maß ⇒ anzeigend.
  - Werkzeugwechsel mitten im Wählen, Rechtsklick, Strg+Z: nichts bleibt.
- [ ] Steps 2–4: FAIL, implementieren, Test grün.
- [ ] **Step 5: Commit** „Skizze: Werkzeug Skizzenbemaßung mit waagerecht, senkrecht und Durchmesser“.

### Task 9: Werkzeug Rechteckige Anordnung

**Files:**
- Create: `crates/planora-app/src/sketch_pattern.rs`
- Modify: `sketch_state.rs` (`Tool::Pattern`, `PatternToolState`), `sketch_kinds.rs` (`ToolKind::PatternRect`, `PatternCircular`, `ToolKinds.pattern`), `sketch_input.rs` (`FieldKind::PatternCount1, PatternSpacing1, PatternCount2, PatternSpacing2, PatternCount, PatternAngle`, `read_field`-Zweige: Anzahl ganzzahlig ≥ 1, Abstand ≠ 0 mit Vorzeichen, Winkel ≠ 0), `sketch_grips.rs` (`pattern_layout`, `pattern_hit`, `pattern_motion`), `sketch_view.rs` (Vorschau, abgewählte gestrichelt), `sketch_panel.rs` (Optionen), `de.ftl`, `en.ftl`
- Test: `crates/planora-app/tests/sketch_pattern_rect.rs` (neu)

**Interfaces:**
- Consumes: Task 3, 4; `TransformStage`-Muster aus `sketch_grips.rs`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct PatternToolState { pub stage: TransformStage, pub plan: Option<PatternPlan>, pub editing: Option<PatternId>, pub selection_before: Vec<Item>, pub picking_direction: bool }`
  - `impl SketchSession { pub fn begin_pattern(&mut self, sk: &Sketch); pub fn click_pattern(&mut self, sk: &Sketch, cur: &DrawCursor); pub fn finish_pattern(&mut self, sf: &mut SketchFeature) -> Result<(), String>; pub fn pattern_preview(&self, sk: &Sketch) -> Option<Vec<([i32; 2], Vec<Vec<DVec2>>)>> }`
  - `pub fn pattern_layout(origin: Pos2, tips: [Option<Pos2>; 2], angle_tip: Option<Pos2>) -> PatternGripLayout; pub fn pattern_hit(l: &PatternGripLayout, p: Pos2) -> Option<PatternGrip>; pub fn pattern_motion(d: &PatternGripDrag, dir: DVec2, now: DVec2) -> f64`

- [ ] **Step 1: Tests** (1920×1080)
  - Rechteck wählen, Werkzeug „Rechteckige Anordnung“: Vorschau 3×1 im Abstand 1,5 × Breite; Feld Anzahl „4“, Tab, Abstand „20“: Vorschau folgt; Klick auf Exemplar 2 ⇒ gestrichelt, nochmal ⇒ zurück; Enter ⇒ Muster mit drei Exemplaren, **ein** Rückgängig nimmt es samt Werten, Wiederholen bitgleich. Esc ⇒ nichts.
  - **Vorschau = Ergebnis:** Linienzüge der Vorschau gleich den angelegten Kurven unter 1e-9.
  - Griff am Pfeil ziehen ⇒ Abstand folgt der Projektion des Zugs auf die Richtung; Rechtsklick im Zug ⇒ alter Wert.
  - Richtung über Klick auf eine schräge Linie; „Gesamtlänge“, „Symmetrisch“: Lagen wie Task 3.
  - Doppelklick auf ein Exemplar im Auswahlwerkzeug ⇒ Werkzeug im Bearbeiten, Anzahl 4 → 2, OK ⇒ ein Schritt, Bindung an Exemplar 1 bleibt.
  - Wert „Abstand 1“ in der Liste doppelklicken ⇒ Feld; Formel `b * 2` bleibt, Parameter `b` ändern ⇒ Exemplare folgen.
  - **Review-Focus 2:** Anzahl über Formel 5 → 2 → 5, Zoll „1“ ⇒ 25,4 mm und Speichertext „1 in“; Löschen eines Exemplars über Entf ⇒ abgewählt, Muster bleibt.
  - 10×10 im Werkzeug angelegt, dann Abstand im Feld ändern: Zeit bis gelöste Skizze unter 100 ms (Release-Messung `#[ignore]`).
- [ ] Steps 2–4: FAIL, implementieren, Test grün.
- [ ] **Step 5: Commit** „Skizze: Werkzeug Rechteckige Anordnung“.

### Task 10: Werkzeug Kreisförmige Anordnung

**Files:**
- Modify: `crates/planora-app/src/sketch_pattern.rs`, `sketch_grips.rs`, `sketch_panel.rs`, `de.ftl`, `en.ftl`
- Test: `crates/planora-app/tests/sketch_pattern_circular.rs` (neu)

**Interfaces:**
- Consumes: Task 9.
- Produces: keine neuen öffentlichen Namen außer `PatternGrip::Angle`.

- [ ] **Step 1: Tests** (1920×1080)
  - Kreis wählen, Werkzeug, Klick auf den Ursprung (Fang) als Mitte: Vorschau 6 voll; Anzahl 8 ⇒ 45° Schritt; Art „Winkel“ 120° bei 3 ⇒ 0°, 60°, 120°; „Symmetrisch“ ⇒ −60…60°. Griff am Winkel ziehen ⇒ Winkel folgt, über 180° hinweg ohne Sprung (`unwrapped_angle`).
  - Mitte mit Raster und Strg wie Teilprojekt 2; Mitte auf einem Exemplar eines anderen Musters ⇒ Meldung `sketch-pattern-instance-source`.
  - Abwählen, Enter, ein Rückgängig-Schritt; Esc ⇒ nichts. Mittelpunkt später verschieben ⇒ Exemplare folgen.
  - **Review-Focus 1:** „Winkel“ 360° bei 4 ⇒ Vorlage und drei Exemplare bei 90°, 180°, 270°, keins deckt die Vorlage; Anzahl 1 ⇒ keine Exemplare, OK legt trotzdem an (Muster mit Anzahl 1, änderbar).
- [ ] Steps 2–4: FAIL, implementieren, Test grün.
- [ ] **Step 5: Commit** „Skizze: Werkzeug Kreisförmige Anordnung“.

### Task 11: Mitgenommen: Langloch „Gesamt“, Doppelklick auf Maße, Rasterfang bindet

**Files:**
- Modify: `crates/planora-app/src/sketch_input.rs` (`overall_center_distance` entfällt; `dimension_kind`: `(Created::Slot, Length)` bei `SlotOverall` ⇒ `SlotOverall(centers, caps[0])`), `sketch_view.rs` (`LabelHit`, Doppelklick), `sketch_cursor.rs` (`resolve_free`), `crates/planora-sketch/src/query.rs` (`point_at`)
- Test: `crates/planora-app/tests/sketch_kinds_slot.rs` (bewusst umgeschrieben, Kommentar „Skizzen 5“), `crates/planora-app/tests/sketch_carried_s5.rs` (neu)

**Interfaces:**
- Consumes: Task 1 (`SlotOverall`).
- Produces: `impl Sketch { pub fn point_at(&self, at: DVec2, tol: f64) -> Option<PointId> }`; `pub(crate) struct LabelHit { pub id: ConstraintId, pub rect: Rect }`.

- [ ] **Step 1: Tests**
  - Langloch „Gesamt“ mit L 60, W 10: ein `SlotOverall` 60 und ein `Diameter` 10; W auf 20 ⇒ Gesamtlänge bleibt 60, Mittenabstand 40. Formel `L` an der Gesamtlänge bleibt nach Parameteränderung richtig. Umgeschrieben: die alten Erwartungen „Mittenabstand L − W“ in `sketch_kinds_slot.rs` (Liste der Tests im Commit).
  - Doppelklick auf den Text „Ø 10“ in der Ansicht ⇒ Feld dieses Maßes offen wie aus der Liste; Doppelklick neben dem Text ⇒ Kettenwahl wie bisher.
  - Linie mit Start 8,5 px neben einem Punkt bei Raster 1 mm, Rasterpunkt = Punkt ⇒ Linie beginnt an diesem `PointId`, kein zweiter Punkt; mit Strg ⇒ freier Punkt wie heute. Danach rundet die Rundung diese Ecke (Beobachtung aus Handtest Skizzen 4).
- [ ] Steps 2–4: FAIL, implementieren, Tests grün.
- [ ] **Step 5: Commit** „Skizze: Gesamtlänge des Langlochs als Maß, Doppelklick auf Maße, Raster bindet“.

### Task 12: Leiste, Menüs, Kürzel und Einstellungen

**Files:**
- Modify: `crates/planora-app/src/sketch_panel.rs` (`TOOL_ORDER` 20, `tool_label`, `tool_hint`), `shell.rs` (`STRIP_TOOLS = 20`, Kommentar 813 px), `keymap/defaults.rs`, `commands.rs`, `app.rs` (`sketch_kinds_menu`), `icons.rs`, `de.ftl`, `en.ftl`
- Test: vorhandene Leisten-, Kürzel- und Menütests erweitern (`keymap`-Konflikttest, Leiste passt bei 1920×1080), `crates/planora-app/tests/sketch_s5_commands.rs` (neu)

**Interfaces:**
- Consumes: Task 7–10, Entscheidung 15, 16.
- Produces: `Action::{SketchPatternRect, SketchPatternCircular, SketchDimension, SketchConcentric, SketchCollinear, SketchSmooth}`.

- [ ] **Step 1: Tests** (1920×1080)
  - Leiste 20 Knöpfe, ganz sichtbar, keine Liste nötig; „Muster“ mit Liste (zwei Arten), letzte Art gemerkt.
  - D öffnet die Skizzenbemaßung nur in der Skizze; in der Zeichnung weiter das Zeichnungsmaß. Alle sechs Aktionen in den Einstellungen belegbar, Konflikttest grün.
  - Menü „Erzeugen“ zeigt die drei Einträge nur bei offener Skizze; die drei Abhängigkeiten über eine belegte Taste wirken wie der Knopf.
  - Alle neuen Schlüssel in `de.ftl` und `en.ftl` (vorhandener Vollständigkeitstest).
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Muster, Bemaßung und neue Abhängigkeiten in Leiste, Menü und Kürzeln“.

### Task 13: Doku, offene Punkte, Gesamtlauf, Handtest

**Files:**
- Modify: `docs/funktionen.md` (Abschnitt „Skizzen“), `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt Skizzen 5“; im Abschnitt Skizzen 3 die Gesamtlänge als ersetzt markieren; Folgeideen Skizzen 4 „Doppelklick“ und „Raster bindet“ als erledigt), `CHANGELOG.md`

- [ ] **Step 1:** `offene-punkte.md`: alle Abweichungen (Obergrenze lehnt ab statt Geometrie; Glatt am Bogen nur G1; Textlage der Maße nicht gespeichert; Konzentrisch als deckende Mitten), Grenzen (teilweise überzählige Abhängigkeit meldet „überzählig“, obwohl eine Zeile noch wirkt; Anzahl über Formel lässt Bindungen wegfallender Exemplare still fallen; einseitige Bindung an Exemplare; Kollinear lehnt große Sprünge ab statt sie zu erreichen) und die Annahmen aus Entscheidung 18.
- [ ] **Step 2:** Voller Lauf unter Windows (`cargo test --workspace`, Clippy, `cargo fmt --check -p` je Crate, `cargo deny check`) und in WSL; alle Messtests `--ignored` im Release.
- [ ] **Step 3:** Handtest bei 1920×1080 nach `tools/gui-drive.ps1`: Rechteckmuster 10×10 ändern, Abwählen, Kreismuster mit Winkel, Muster bearbeiten, Bemaßen waagerecht/senkrecht/Ø an Mittellinie mit Umschalter, Konzentrisch, Kollinear, Glatt, Langloch gesamt, Doppelklick auf Maß, Raster bindet, Rückgängig, eine alte Datei öffnen und speichern (bytegleich).
- [ ] **Step 4: Commit** „Doku: Skizzen 5“.
