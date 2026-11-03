# Skizzen 4: Abrunden, Fase, Versatz, Verschieben, Skalieren – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Ändern-Befehle wie in Fusion: Skizzenrundung, Skizzenfase mit drei Arten, Versatz mit Kette und folgender Kopie, Verschieben/Kopieren mit Griffen, Skizzenmaßstab.

**Architecture:** `planora-sketch` bekommt drei neue Einheiten ohne egui: `fillet.rs` (Rundung, Fase), `offset.rs` (Versatz), `transform.rs` (Verschieben, Drehen, Skalieren, Kopieren). Jede hat eine reine Plan-Funktion und eine atomare Operation, die den Plan anlegt; Vorschau und Klick rechnen aus demselben Plan. In der App laufen Rundung und Fase als `ToolKind` durch die Stufenmaschine aus Skizzen 3, Versatz, Verschieben und Skalieren als eigene Werkzeuge; die Griffe sind eine eigene kleine Einheit `sketch_grips.rs`.

**Tech Stack:** Rust-Workspace, egui 0.36, Fluent (`de.ftl`/`en.ftl`, `tr!`), serde.

**Spec:** `docs/superpowers/specs/2026-11-02-skizzen-aendern-design.md`

## Global Constraints

- Verhalten, Namen und Kürzel wie Autodesk Fusion; Abweichungen in `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 4“) mit Grund. Annahmen über Fusion sind mit **Annahme** markiert.
- Keine neuen Fremd-Abhängigkeiten; `cargo deny check` bleibt grün; Symbole selbst gezeichnet (`HandIcon`), kein Autodesk-Material.
- Alle Oberflächentexte über `tr!` mit Schlüssel in `crates/planora-i18n/languages/de.ftl` **und** `en.ftl`. Intern mm, Anzeigen über `planora_i18n` (Zoll muss gehen). Farben nur über Theme-Tokens (`no_hard_coded_colors.rs`).
- **Dateiformat unverändert** (`FORMAT_VERSION` bleibt 14, `convert.rs` bleibt unberührt): Alles läuft über vorhandene Kurven und Abhängigkeiten (gemessen, Entscheidung 3). Sollte das beim Bau scheitern: anhalten, nicht still erhöhen; die Regel wäre die aus Skizzen 1 und 3 (Format 15 nur, wenn die Datei es wirklich braucht, sonst bytegleich).
- Ein Befehl ist genau ein Rückgängig-Schritt. Esc bricht ab und hinterlässt nichts (auch kein Revisionssprung in der Skizze).
- Keine Rundung, Fase oder Versatz macht die Skizze überbestimmt: nach dem Anlegen `status == Solved`, `redundant` leer, `conflicting` leer, die Vorlage bewegt sich nicht (`iterations == 0` außer Ellipse und Spline).
- Bestehendes Verhalten von Stutzen, Dehnen, Brechen, Ziehen, Spiegeln, Muster bleibt. Alte Dateien ändern sich nicht.
- Geometrie wird über **dichte Wertefolgen** geprüft, nicht an Stichproben; bei jeder Folge wird auf Umklappen (`folded_over`) und Seitenwechsel geprüft. Die Vorschau ruft dieselbe reine Funktion wie der Klick; ein Test vergleicht beide unter 1e-9.
- Tests bei 1920×1080, nie 800×600.
- Prüfen: `cargo test -p <crate> --target-dir C:/Users/denni/AppData/Local/Temp/claude/D--Testumgebung/9257fb9d-a130-4b88-86e3-d09eb7b412f8/scratchpad/target-e2 -j 2`; am Ende `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check -p <crate>` je Crate, `cargo deny check`. **`cargo fmt --all` scheitert unter Windows, darum `cargo fmt -p <crate>` je Crate** vor jedem Commit. Nichts pushen. Vor einem Merge kein GitHub-CI, sondern voller Lauf unter Windows und in WSL.
- Commit-Nachrichten deutsch, enden mit Leerzeile und `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Nach jeder Task Notiz in `D:\Vault\Vault\Projekte\PlanoraCAD\` nach `CLAUDE.md` des Projekts.
- Alte Tests, die neues Verhalten treffen, werden **bewusst** umgeschrieben (Kommentar „Skizzen 4“, Listen in den Tasks); alle anderen bleiben unverändert grün.

## Entscheidungen des Planers

Alle Zahlen unten sind gegen den heutigen Solver gemessen (2026-11-02, Wegwerfprogramm außerhalb des Repos, Release-Build); die Tests der Tasks halten sie fest. „rel“ ist der Unterschied der Freiheitsgrade zur Skizze vor dem Befehl.

1. **Rundung: Abhängigkeiten.** Der Bogen `F` hat eigene Mitte `m` und eigene Berührpunkte `T1`, `T2`; die Enden der Vorlage werden auf `T1`/`T2` umgehängt (`set_curve_kind`, wie beim Stutzen), `F` läuft gegen den Uhrzeigersinn. Reihenfolge immer: Kurven, dann Tangenten, dann Ecke, dann Maß.

   | Paar | Abhängigkeiten | rel | Sweep (alle `Solved`, kein `folded_over`, Tangente unter 1e-6) |
   |---|---|---|---|
   | Linie–Linie | `Tangent(L1,F)`, `Tangent(L2,F)` am gemeinsamen Ende, `Radius(F)` | 0 | 35 Winkel 5°…175°, beide Drehsinne, je 60 Radien, freie Linien: 4200/4200. Feste Fernenden, freie Ecke: 4074 direkt, 2 über `solve_in_steps`, 124 abgelehnt (alle ab 87 % der Grenze), nie umgeklappt |
   | … mit Ecke | dazu Punkt `P` auf `PointOnCurve(P,L1)`, `PointOnCurve(P,L2)` | 0 | `Distance(a,P)` 30…70: 400/400 |
   | Linie–Bogen | wie oben, beide am gemeinsamen Ende; Ecke `PoC(P,L)`, `PoC(P,A)` | 0 | r 0,1…12: 120/120 |
   | Bogen–Bogen | wie oben | 0 | 120/120 |
   | Linie–Kreis | `Tangent(F,L)`, `Tangent(F,K)` (ohne gemeinsames Ende: Abstand gegen Radien, mit `branch`), Konstruktionslinie `H = (Kreismitte, m)`, `PoC(T2,H)`, `Radius(F)` | −1 gegen freies Linienende, also 0 gegen eine Linie, die am Kreis hing | r 0,1…30: 245/300, die übrigen sind echte Ablehnungen über r 24,5 |
   | Feste Linie (`Whole`) | `Tangent(L,F)` ohne gemeinsames Ende, Konstruktionslinie `H = (m,T1)`, `Perpendicular(L,H)`, `Radius(F)` | wie Linie–Kreis | r 0,1…30: 300/300 |
   | Weitere Rundung | statt `Radius` nur `Equal(F0,Fk)` | 0 | Rechteck mit vier Rundungen, H, V, `Equal`: dof 4 wie das Rechteck vor dem Runden, r 0,1…14: 140/140 |

   Verworfen, weil gemessen schlechter: `PoC(T2,K)` plus `Tangent(F,K)` (`redundant = 1`, die Kreise berühren sich dort, die Zeile hält den Punkt nicht); `T2` ganz frei (rel 0 stimmt, aber `T2` rutscht auf `F`).
   Ohne `Radius` ist rel +1, also ist das Maß Pflicht, sonst wäre die Skizze freier als vorher. Bis 15,0 auf einem Rechteck 50×30 mit H, V und Maßen: r 14,9 gelöst, ab 15,0 `Conflicting`; zu große Radien lehnt also schon der Weg über `check_change` ab.
2. **Rundung: Kurvenarten und Ecke.**
   - **Exakt** (`fillet_plan` analytisch): Linie, Bogen, Kreis. **Numerisch:** Ellipse, Ellipsenbogen, Spline. `check_types` weist `Tangent(Bogen, Ellipse)` ab (gemessen), und eine neue Zeile wäre eine neue Art; darum hält dort nur `PoC(T2, Kurve)`, die Tangente an der Ellipse gilt beim Anlegen und folgt nicht. Linie–Ellipse: rel 0, `iterations = 4` (die numerische Lage wird nachpoliert). Steht in `offene-punkte.md`.
   - **Kürzen:** Linie und Bogen bis zum Berührpunkt; eine Linie oder ein Bogen mit `Fixed` am zu ändernden Ende (projizierte Kante) bleibt ganz (`EndMove::Whole`). Kreis, Ellipse, Spline bleiben ebenfalls **ganz** (**Annahme**: Bei einem einzigen Berührpunkt gibt es nichts abzuschneiden; eine Linie, die einen Kreis schneidet, wird gekürzt, der Kreis nicht. Beim Spline würde `trim` die Form ändern, die Tangente stimmte danach nicht mehr). Ein Ellipsenbogen wird gekürzt, indem sein Ende auf die Ellipse wandert. Das ist die eine Abweichung vom Satz der Spec „werden bis zum Bogen gekürzt“; sie kommt in `offene-punkte.md`.
   - **Über `edit.rs`:** Nach dem Umhängen `settle_constraints(&[c1,c2],&[])` und `drop_orphans` wie beim Stutzen, also bleibt, was ohne Bewegung noch gilt (H, V, parallel, gleich zu einem Partner, wenn die Länge stimmt), und `Midpoint` an einer geänderten Linie entfällt. `drop_orphans` wird `pub(crate)`, `settle_constraints` ist es schon.
   - **Ecke:** Der alte Punkt `P` bleibt genau dann als Konstruktionspunkt mit den beiden `PointOnCurve`, wenn nach dem Umhängen noch eine lebende, nicht interne Abhängigkeit oder eine dritte Kurve an ihm hängt; sonst fällt er weg (`drop_orphans`). So überlebt ein Maß auf der Ecke mit derselben `PointId` (**Annahme**: So macht es Fusion mit dem virtuellen Eckpunkt).
   - **Schneidende Kurven ohne gemeinsamen Punkt:** Die Klickstellen wählen die Seiten: Behalten wird das Stück, auf dem geklickt wurde. Linien, die sich nur in der Verlängerung treffen, werden bis zum Berührpunkt verlängert (**Annahme**). Parallele Linien, zwei Klicks auf dieselbe Kurve, berührende Kurven am gemeinsamen Ende und ein Berührpunkt jenseits des behaltenen Endes ergeben `None` und eine Meldung.
   - **Zweig:** Von allen Vorzeichen (Abstand ±r zu jeder Kurve) zählen nur Kandidaten, deren Berührpunkte auf dem behaltenen Stück liegen; davon der mit kleinster Summe `|T1−Klick1| + |T2−Klick2|`, bei Gleichstand (≤ `tol`) kleinster Abstand `|m−X|` (`X` = Ecke). Diese reine Regel entscheidet für Vorschau und Klick.
3. **Versatz: kein neues Format, eine Abhängigkeitsform für alles.** Das Rechenproblem „Radius plus Abstand“: Der Solver hat keine Zeile `r2 − r1 = d`. `Equal` kennt nur `r1 = r2`, `Radius` nur eine Kurve. **Es geht ohne neue Zeile:** Ein Konstruktionskreis `C` mit Radius ρ, dessen Mitte `S` auf der Vorlage liegt, und `Tangent(C, Kopie)` ergibt mit dem vorhandenen Zweig Bogen/Kreis `|S−m| = r' ∓ ρ`, also `r' = r ± ρ`; an einer Linie `Abstand(S, Kopie) = ρ`. Der Zweig steht beim Anlegen fest, also kann die Seite nicht wechseln und ρ nicht negativ werden. Das nennt der Plan **Rollkreis**. Gemessen:

   | Bindung des einen Abstands | n-Eck 8 | n-Eck 64 | Anlegen n = 200 | Befund |
   |---|---|---|---|---|
   | je Kurve eigenes `DistancePointLine` | gut | d-Folge 78/100, 55 Umklapper | 605 ms | n Maße statt eines, schlecht |
   | Kreis an jeder Ecke, tangential an beide Nachbarn | gut | 27/100, 7 Umklapper | 1349 ms | verworfen |
   | Querstrich mit `Equal` und `Distance` | gut | 100/100 | 2552 ms | Bogen: `Distance` hat keinen Zweig, Vorlage r 0,25…100: 187/400 falsch und umgeklappt, verworfen |
   | **Rollkreis je Lauf, `Equal` zum ersten, `Radius` am ersten** | 100/100 | 100/100, 0 Umklapper, 40 ms | 1313 ms | **gewählt** |

   **Abhängigkeiten je Vorlage** (Mitte von Bögen und Kreisen ist immer dieselbe `PointId` wie die der Vorlage, das spart die konzentrische Abhängigkeit aus Teilprojekt 5):

   | Vorlage | Kopie trägt | rel |
   |---|---|---|
   | Linie (offen) | `Parallel(Q,K)`, Rollkreis an `Q.a` mit `Tangent(K,C)`, an jedem offenen Ende Konstruktionslinie `(Q-Ende, K-Ende)` mit `Perpendicular(Q, Linie)` | 0 (ohne Querlinien +2) |
   | Bogen (offen) | Mitte geteilt, Rollkreis an `Start` mit `Tangent(C,K)`, an beiden offenen Enden Konstruktionslinie `(Mitte, Q-Ende)` mit `PoC(K-Ende, Linie)` | 0 |
   | Kreis | Mitte geteilt, Hilfspunkt `S` mit `Horizontal(Points(m,S))`, `PoC(S,Q)`, Rollkreis an `S`, `Tangent(C,K)` | 0 |
   | Geschlossene Kette | je Linie `Parallel`, an tangentialer Fuge `Tangent(Bogen', Linie')` am gemeinsamen Ende, an scharfer Fuge nur der gemeinsame Punkt; **je Lauf** (Stück zwischen zwei scharfen Fugen) ein Rollkreis | 0 |
   | Offene Kette | wie geschlossen, dazu die Querlinien der beiden offenen Enden | 0 |

   Gemessene Folgen: L-Form mit zwei gerundeten Ecken (r 6) außen und innen: rel 0, `redundant` 0, d-Folge mit gehaltener Vorlage bis d < 6 alle gelöst, Fehler unter 1e-6, 0 Umklapper; bei d ≥ 6 verschwindet der Bogen, das lehnt der Solver ab. Vorlage-Radius 0,1…10 außen 100/100, innen 60/100 (die übrigen sind r ≤ d, abgelehnt, nicht umgeklappt). Vorlage-Ecke ziehen: Fehler 5e-15. Einzelner Kreis mit Rollkreis: d 0,1…50 außen 500/500, innen bis 19 190/190, Vorlage-Radius innen 380/400. Einzelner Bogen bei gehaltener Vorlage: außen 500/500, innen bis 19,9 alle, ab d = 20 abgelehnt, nie umgeklappt; Vorlage-Radius außen 400/400, innen 380/400.
   **Aufräumen:** Bei rundum tangentialer Kette ist die letzte `Parallel` überzählig (gemessen: abgerundetes Rechteck, `redundant = 1`, genau diese). Nach dem Anlegen darum einmal lösen und angelegte `Parallel`/`Tangent` aus `report.redundant` entfernen, wie schon `add_linear_pattern`; danach `redundant` leer.
4. **Vorlage bleibt stehen, wenn sich das Versatzmaß ändert.** Gemessen: Bei freier Vorlage geht bei d 5→12 auch die Linie um 3,5 mm mit (der Löser verteilt den Schritt), über die Folge bis 30 mm. Das ist das Anti-Ziel. Darum `Sketch::offset_hold_points(dim)`: erkennt am Muster (Maß ist `Radius` an einem Konstruktionskreis, der tangential an einer normalen Kurve hängt), welche Punkte die Vorlage bilden; `solve_holding` löst einmal auf einer Kopie mit vorübergehendem `Fixed` auf ihnen und übernimmt die Werte (gemessen: d-Folgen alle gelöst, Vorlage wandert 0,000). Scheitert das, läuft das normale Lösen weiter. Greift in `set_dimension_value`; Formeln und Parameter laufen weiter über `solve_formulas` und werden **nicht** gehalten (Grenze, kommt in `offene-punkte.md`). **Rundung und Fase bekommen das nicht:** Dort lässt sich aus dem Muster nicht von einem von Hand gebauten Bogen unterscheiden, und das Ändern dieses Verhaltens träfe alte Skizzen. Gemessen dreht eine ganz freie Linie bei r 5→30 um 15,7° (mit gehaltener Ecke 2,8°), mit H, V oder Maßen gar nicht. Steht in `offene-punkte.md` und im Bericht an Dennis.
5. **Versatz: Grenze bei langen Ketten.** Jedes Lösen rechnet den Rang der ganzen Gruppe dicht: n = 64 kostet 40 ms, n = 200 kostet 1313 ms **bei jeder Strukturänderung** der Skizze (die Spec begrenzt nur die Vorschau, die braucht den Solver gar nicht). Darum `OFFSET_TIED_MAX = 64` Stücke: Darüber entsteht die Kopie als reine Geometrie mit gemeinsamen Punkten, ohne Parallel und Rollkreise; sie folgt nicht, und `sketch-offset-not-tied` sagt das. Abweichung von der Spec, in `offene-punkte.md`.
6. **Versatz: Rechnung** (`offset_plan`, rein, ohne Solver):
   1. Kette ordnen (`ordered_chains`): gerichtete Folge, Verzweigung ⇒ `OffsetProblem::Branching`. Konstruktionskurven am Rand der Kette zählen nicht, es sei denn, die Kette besteht aus ihnen (sonst liefe sie über die Querlinien eines früheren Versatzes).
   2. Rohstücke: Linie verschoben, Bogen und Kreis `r ± d`; `r' ≤ tol` oder umgekehrte Richtung ⇒ das Stück entfällt.
   3. Fugen: tangentiale Fuge (Richtungen gleich innerhalb `tol.angular`) ⇒ gemeinsamer Punkt `T + n·d`. Scharfe Fuge ⇒ Schnitt der verlängerten Nachbarn (Linie als Gerade, Bogen als Kreis), der der Vorlage nächste; kein Schnitt ⇒ `OffsetProblem::NoJoin` (**Annahme**: Fusion setzt dort vielleicht einen Bogen; `NoJoin` kommt in `offene-punkte.md`). Nach einem entfallenen Stück neu ab Schritt 2, bis nichts mehr entfällt.
   4. Kreuzungen (geschlossen und offen): Paare über `boxes_touch` gefiltert, `intersect_prepared`, dann `arrangement::split` zerlegt jedes Stück an den Stellen; `Nodes` legt Knoten zusammen. Beide werden `pub(crate)`. **Nicht** `Arrangement::build`: Es wirft doppelte Stücke und Stummel nach Flächenregeln weg und braucht Umläufe; hier zählt allein der Abstand. Teilstücke, deren Mitte näher als `|d| − 10·tol` an irgendeiner Vorlagekurve liegt (`Geom::nearest`), entfallen. Übrig bleiben Knoten mit Grad 2; alles andere ⇒ `OffsetProblem::SelfCrossing`. Eine geschlossene Vorlage ergibt nur geschlossene Schleifen (sonst `SelfCrossing`), auch mehrere (Hantel innen: zwei). Nichts übrig ⇒ `OffsetProblem::Vanished`.
   5. Ändert später ein Maß die Lage so, dass sich die Kopie kreuzt, gilt das, was schon jetzt gilt: Sie steht als `RejectedLoop` im Profil, mit der vorhandenen Meldung.
7. **Exakt oder Näherung.** Exakt: Linie, Bogen, Kreis. Der Versatz einer Ellipse ist **keine Ellipse**. Ellipse, Ellipsenbogen und Spline ergeben Splines durch exakte Versatzpunkte: Ellipse zwei Splines mit je 32 Abschnitten (gemeinsame Enden, also geschlossen), Ellipsenbogen und Spline 4 Abschnitte je Stützabschnitt bzw. höchstens 63. Gemessen an Ellipse 40×20 je Halbkreis: Abweichung d = 5: 0,029 mm, d = −5: 0,009 mm, d = 20: 0,058 mm. Näherungsstücke bekommen keine Abhängigkeit und folgen nicht. Liegt `d` innen über dem kleinsten Krümmungsradius (bei 40×20: 10), entstünden Spitzen: `OffsetProblem::Cusp` statt Trimmen, Meldung. Fuge Näherung–Linie: gemeinsamer Punkt, die Linie trägt ihre Abhängigkeiten.
8. **Stil der Kopie:** wie die Vorlage (`construction`, `centerline`); Vorlage mit festem Punkt (projizierte Kante): normal, und die Kopie folgt ihr. Rollkreise und Querlinien sind immer Konstruktion. **Sichtbarkeit:** Sie sind normale Konstruktionskurven: sichtbar, fangbar, schneiden beim Stutzen. Abweichung von Fusion, das eine eigene Versatz-Verbindung hat; Folgeidee in `offene-punkte.md`, wenn es im Handtest stört.
9. **Fase** (nur Linien, Ecke immer gehalten, weil die Maße von ihr aus zählen). Kurve `C = (T1,T2)`, `L1`, `L2` umgehängt, `P` mit `PoC(P,L1)`, `PoC(P,L2)`:

   | Art | Maße und Abhängigkeiten | rel | d-Folge 0,1…37 |
   |---|---|---|---|
   | Gleicher Abstand | Konstruktion `H1=(P,T1)`, `H2=(P,T2)`, `Equal(H1,H2)`, `Distance(P,T1)` | 0 | 370/370, 0 Umklapper |
   | Zwei Abstände | `Distance(P,T1)`, `Distance(P,T2)` | 0 | 370/370, 0 |
   | Abstand und Winkel | `Distance(P,T1)`, `Angle(L1,C,v)` | 0 | 370/370, 0; Winkel ±0,9 rad 181/181, 0 |

   Der gemessene Winkel wird beim Anlegen aus der Lage genommen (`sk.measure`), damit Vorzeichen und `v` passen. Variante ohne Ecke und `Distance(T1,T2)`: rel +1, verworfen.
10. **Verschieben und Kopieren** (`transform.rs`, ohne Solver). Bewegt werden die Punkte der gewählten Kurven und die gewählten Punkte; Kreisradius und Ellipsen-Halbachsen mit.
    - **Bleibt:** alles, was danach ohne Bewegung erfüllt ist (`satisfied(id, SETTLE_FACTOR)` vor und nach dem Schreiben), also alle inneren Maße, Parallel, Gleich, Tangente. Beim Drehen um ein Vielfaches von 90° entfallen H und V trotzdem (Keine Umschreibung, **Annahme**). `EllipseAngle` innen bekommt den Drehwinkel addiert.
    - **Fällt weg:** Äußere Abhängigkeiten, die nicht mehr gelten; die Meldung nennt die Zahl (`sketch-move-dropped`). Ein Punkt, den auch eine nicht gewählte Kurve benutzt, wird getrennt: Die fremde Kurve bekommt einen neuen Punkt an der alten Lage und bleibt, wo sie war (**Annahme**: Fusion dehnt vielleicht; die Spec sagt „nach außen bindende entfallen“). Das ersetzt jede Nachbarkurven-Dehnung, also braucht der Befehl keinen Solver, und Vorschau und Ergebnis sind dieselbe Rechnung.
    - **Verhindert mit Meldung** (`sketch-move-fixed`): `Fixed` an einem bewegten Punkt (auch projizierte Kanten in der Auswahl) und Lage-Bindungen (`Coincident`, `PointOnCurve`, `Midpoint`, `Symmetric`) an festgehaltene Geometrie außerhalb, die danach nicht mehr erfüllt wären. Winkel und Abstände zu fester Geometrie (Achshilfslinie) entfallen nur.
    - **Kopie:** Kurven über `copy_transformed` samt Stil, jede innere Abhängigkeit mit abgebildeten Ids über `add_constraint` (setzt Zweig und Vorzeichen neu), `driven` und `enabled` mit; `Fixed` nicht, die Kopie ist frei. Keine Bindung ans Original (**Annahme**). Anders als Spiegeln und Muster gehen auch Ellipse und Spline. Formeln der kopierten Maße werden mitkopiert.
11. **Skalieren.** Wie 10 ohne Drehung; Längenmaße (`Distance`, `DistancePointLine`, `Radius`, `Diameter`, `EllipseMajor`, `EllipseMinor`), deren Bezüge ganz in der Menge liegen, werden mit `k` multipliziert, Winkel nicht. `k > 0`. **Formeln:** Der Text wird zu `(alt) * k` (kürzeste Schreibweise der Zahl), damit der Wert stimmt und der Parameter dranbleibt (**Annahme**: Was Fusion mit einem Maß tut, das einen Parameter hat, ist nicht geprüft). **Ganze Skizze:** alle Kurven und freien Punkte ohne feste Geometrie, also ohne Achshilfslinie und Kantenkopien; die bleiben, und ein Maß zu ihnen fällt weg. Ist etwas über eine Lage-Bindung an den Ursprung oder an eine Kante gebunden und bleibt die Bindung nicht erfüllt: Meldung `sketch-move-fixed`, wie die Spec es sagt; Bezugspunkt im Ursprung geht also.
12. **Griffe in `handle_input` und `show_overlay`.**
    - Griffe gibt es nur bei `Tool::MoveCopy` und `Tool::Scale` mit gesetzter Menge. `begin_drag` kehrt außerhalb von `Tool::Select` zurück, `begin_line_arc` außerhalb von `Tool::Line`, `box_can_start` außerhalb von `Tool::Select`; nichts davon kommt sich also in die Quere.
    - **Rahmen beim Wählen:** `box_can_start` lässt außerdem `Tool::MoveCopy` und `Tool::Scale` zu, solange `transform.stage == Picking`.
    - **Griff beginnt:** im selben Zweig wie der Rahmen (`drag_started_by(Primary) && !aborted`), vor `begin_drag`, `se.begin_grip(hit)` mit `hit = sketch_grips::hit(layout, press_origin)`. `aborted` deckt die offene Liste ab (Drücken schließt nur sie). `boxing` zählt `se.grip_drag.is_some()` mit, also schwenkt Umschalt währenddessen nicht.
    - **Laufend:** nach dem Block des Linienbogens; rechte Taste und Esc nehmen nur den Griffzug zurück (wie der Rahmen), `drag_stopped` übernimmt ihn in `transform`, die Skizze bleibt dabei unberührt.
    - **Klicks:** Ein Griffzug liefert kein `clicked`. Ein Klick ohne Griff im Stadium `Picking` schaltet die Auswahl, im Stadium `SetBase` setzt er den Bezugspunkt über `resolve_cursor` (Fang, Raster, Strg wie Teilprojekt 2 und 3).
    - **Zeichnen:** `draw` ruft nach `draw_preview` `sketch_grips::paint`. Felder (`sketch_inline`) nehmen keine Mausklicks an und stören nicht.
    - Abschluss über Enter oder „OK“ im Skizzenfeld (**Annahme**, wie die Palette in Fusion), Esc verwirft alles.
13. **Felder und Werkzeuge.** `Tool::Fillet` (mit Arten, Liste), `Tool::Offset`, `Tool::MoveCopy`, `Tool::Scale`; alle `is_modify()` (Esc beendet wie Stutzen) und `is_drawing_tool()` (Felder, Fang). `TOOL_ORDER` 18 Einträge: …`Trim, Extend, Break, Fillet, Offset, MoveCopy, Scale`. `STRIP_H` = 18·36 + 17·4 + 9 + 8 = 733 passt in 1080; **keine** Liste nötig. Neue `FieldKind`: `Distance`, `Distance2`, `OffsetDistance` (Vorzeichen erlaubt, nicht 0), `MoveX`, `MoveY`, `MoveAngle` (Vorzeichen erlaubt), `ScaleFactor` (`ParamUnit::Number`, > 0). `read_field` bekommt je einen Zweig vor dem `_`-Zweig für „positiv“.
    `ToolKind` +4: `Fillet, ChamferEqual, ChamferDistanceAngle, ChamferTwoDistances`, Stufen `[[],[],[Radius]]`, `[[],[],[Distance]]`, `[[],[],[Distance,Angle]]`, `[[],[],[Distance,Distance2]]`. Zweite und weitere Rundung derselben Sitzung (`fillet_run: Option<(CurveId,f64)>`): Der zweite Klick schließt gleich ab, ohne Radiusstufe, wie Fusion (**Annahme**). Vorgabe `ToolKinds.fillet = Fillet`. `sketch_kinds_menu` im Menü „Erzeugen“ lässt `Tool::Fillet` aus. Radius, Abstand vom Zeiger: Abstand Zeiger–Ecke `X`.
14. **Kürzel und Namen** (**Annahme**, aus dem Gedächtnis, kommt in `offene-punkte.md`). `O` und `M` stehen in der Spec. `F` für die Rundung ist **Annahme** (in Fusion nach Erinnerung auch im Skizzenmodus), der Bereich `Sketch` kollidiert nicht mit `Fillet` in `Model`, so wie `S` schon in beiden steht. Alle übrigen ohne Vorgabetaste, aber belegbar.

    | Aktion | Id | Schlüssel `action-sketch-…` | Taste | de | en |
    |---|---|---|---|---|---|
    | `SketchFillet` | `skizze.rundung` | `fillet` | F | Skizzenrundung | Sketch Fillet |
    | `SketchChamferEqual` | `skizze.fase_gleich` | `chamfer-equal` | – | Fase: gleicher Abstand | Chamfer: Equal Distance |
    | `SketchChamferDistanceAngle` | `skizze.fase_winkel` | `chamfer-distance-angle` | – | Fase: Abstand und Winkel | Chamfer: Distance and Angle |
    | `SketchChamferTwoDistances` | `skizze.fase_zwei` | `chamfer-two-distances` | – | Fase: zwei Abstände | Chamfer: Two Distance |
    | `SketchOffset` | `skizze.versatz` | `offset` | O | Versatz | Offset |
    | `SketchMoveCopy` | `skizze.verschieben` | `move-copy` | M | Verschieben/Kopieren | Move/Copy |
    | `SketchScale` | `skizze.massstab` | `scale` | – | Skizzenmaßstab | Sketch Scale |

    Alle Stellen, die Aktionen aufzählen, laufen über `Action::ALL` (`keymap/mod.rs:113`, `:144`, `:226`, `overview.rs:114`, `settings_ui.rs:499`, `dispatch.rs:44`) und tragen die neuen von selbst. Anzupassen: `commands.rs:205-230`, die beiden `match`-Arme, und der Kommentar `defaults.rs:99`. Der Name der Leiste für `Tool::Fillet` ist „Rundung“/„Fillet“.
    **Menü:** Eine Gruppe „Ändern“ gibt es noch nicht. Fusion hat Versatz unter „Erzeugen“, darum `SketchOffset` im Menü „Erzeugen“; Rundung, Fase, Verschieben, Maßstab als Untermenü `menu-sketch-modify` „Ändern“/„Modify“ im Menü „Bearbeiten“ (nur bei offener Skizze).
15. **Annahmen über Fusion** (alle kommen gesammelt in `offene-punkte.md`): Kreis, Ellipse, Spline bleiben beim Runden ganz; Punkt an der alten Ecke; Radius-Übernahme und Abschluss beim zweiten Klick; Verlängern bis zum Berührpunkt; Fasen-Namen und Feldreihenfolge; Seite folgt dem Zeiger und negativer Wert wechselt sie (Spec); `NoJoin`; Griffe: Pfeile X und Y, Mitte frei, Ring dreht; Abschluss mit OK; Trennen geteilter Punkte beim Verschieben; H und V fallen beim Drehen weg; Kopie unabhängig vom Original; Formel `(alt) * k`; Kürzel `F`.

## Review Focus

1. Unmögliche Eingaben: Rundung größer als die Kurven, parallele oder identische Kurven, Kreis ohne gemeinsame Seite, Fase länger als die Linie, Versatz nach innen so groß, dass alles verschwindet, Faktor 0 oder negativ. Erwartet: keine Vorschau, beim Klick Meldung, Skizze bitgleich, Stufe bleibt (Task 1, 3, 7, 8, 10).
2. Hilfsgeometrie im Weg: Rollkreis oder Querlinie löschen, stutzen, Kopie wieder löschen. Erwartet: `Solved`, keine toten Abhängigkeiten, Kopie wird frei, Vorlage bleibt (Task 4).
3. Abbruch und Rückgängig mitten im Befehl: Esc in jeder Stufe, Rechtsklick und Esc im Griffzug, Werkzeugwechsel, Strg+Z, gelöschte Kurve unter `pending_curves`. Erwartet: Skizze und `revision` unverändert; ein Rückgängig nimmt die ganze Kette samt Maß und Hilfen, Wiederholen stellt sie bitgleich her (Task 7, 8, 9, 10).
4. Feste Geometrie: Rundung an projizierter Kante und am Ursprung gebundener Ecke, Versatz einer Kantenkopie, Verschieben und Skalieren einer gebundenen Auswahl, ganze Skizze mit Achshilfslinie. Erwartet: `Solved` oder Meldung, nie Widerspruch; Vorlage unverändert (Task 2, 4, 5, 9, 10).
5. Zoll und Formeln: Radius, Abstand, Versatz „1“ ⇒ 25,4 mm, Speichertext „1 in“, Formel bleibt am Maß; Faktor „2“ bleibt in Zoll 2; Skalieren einer Formel bleibt richtig nach Parameteränderung; Versatz-Vorzeichen in Zoll (Task 7, 8, 10).

---

### Task 1: Geometrie von Rundung und Fase

**Files:**
- Create: `crates/planora-sketch/src/fillet.rs`; `lib.rs` (`pub mod fillet`, Re-Exports)
- Test: `crates/planora-sketch/tests/fillet_plan.rs` (neu)

**Interfaces:**
- Consumes: `geom.rs` (`Geom`, `nearest`, `eval`), `intersect.rs` (`intersect`), `shapes::ArcPlan`.
- Produces (rein, `&Sketch` nur gelesen; `None` bei Entartung):
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct CurvePick { pub curve: CurveId, pub at: DVec2 }`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum EndMove { Replace(PointId), Whole }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct FilletPlan { pub picks: [CurvePick; 2], pub arc: ArcPlan, pub touch: [DVec2; 2], pub moves: [EndMove; 2], pub corner: Option<PointId>, pub corner_at: DVec2, pub radius: f64, pub exact: bool }`; `touch[i]` gehört zu `picks[i]`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum ChamferKind { Equal(f64), DistanceAngle(f64, f64), TwoDistances(f64, f64) }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct ChamferPlan { pub picks: [CurvePick; 2], pub touch: [DVec2; 2], pub moves: [PointId; 2], pub corner: Option<PointId>, pub corner_at: DVec2, pub kind: ChamferKind }`
  - `impl Sketch { pub fn fillet_plan(&self, a: CurvePick, b: CurvePick, radius: f64) -> Option<FilletPlan>; pub fn fillet_corner(&self, a: CurvePick, b: CurvePick) -> Option<DVec2>; pub fn chamfer_plan(&self, a: CurvePick, b: CurvePick, kind: ChamferKind) -> Option<ChamferPlan>; pub fn curves_at_point(&self, p: PointId) -> Vec<CurveId> /* Linien, Bögen, Ellipsenbögen, die dort enden */ }`
  - `ChamferKind::DistanceAngle(d, θ)`: Winkel bei `T1` zwischen `T1→X` und `T1→T2`; `T2` im Abstand `d·sin θ / sin(π − φ − θ)` von der Ecke (φ Eckwinkel), `None` wenn der Nenner ≤ 0.

- [ ] **Step 1: Tests**
  - `fillet_plan` Linien (−40,0)–(0,0), (0,0)–(0,40), r 5: `touch` (−5,0), (0,5), Mitte (−5,5), `arc.sweep()` 90°, `moves` Replace, `corner` Some(P). Ohne fremdes Maß bleibt `corner` trotzdem gesetzt; `add_fillet` entscheidet.
  - **Dichte Folge, kein Zweigwechsel:** Für alle Winkel 5°…175° in 5°-Schritten, beide Drehsinne, r in 200 Schritten bis zur Grenze: `Some`, Berührpunkte auf dem behaltenen Stück, Abstand Mitte–Linie = r unter 1e-9, und die Mitte läuft stetig (`|m(r+dr) − m(r)| ≤ dr / sin(φ/2) · 1,001`). Klick auf dieselben Stellen, gleiche Eingabe: bitgleich. `sk.revision()` unverändert.
  - Linie–Bogen, Bogen–Bogen, Linie–Kreis mit den Werten aus Entscheidung 1 (Linie (−30,0)–(0,0), Bogen um (10,10) r √200 ab (0,0); Bögen um (−20,0), (20,0) r 25; Kreis (0,12) r 10): `|m−c| = R + r`, Berührpunkt auf der Kurve unter 1e-9; je 120 Radien stetig.
  - Schneidende Linien: Kreuz in (0,0), Klicks (−10,0) und (0,10) ⇒ Fuge im Quadranten der Klicks; Klicks (10,0) und (0,−10) ⇒ gegenüberliegend. Linien, die sich nur verlängert treffen: Replace am näheren Ende.
  - `exact == false` für Ellipse und Spline: Abstand zur Kurve unter 1e-3, `Whole`. Ellipsenbogen: `Replace` am Ende.
  - **Review-Focus 1:** parallel, gleiche Kurve zweimal, tangentiale Kurven am gemeinsamen Ende, r 0, r < 0, NaN, r über die Linie hinaus ⇒ `None`. Nie NaN in `Some`.
  - `chamfer_plan`: `Equal(6)` an den Linien oben: (−6,0), (0,6). `TwoDistances(3,8)`: (−3,0), (0,8). `DistanceAngle(6, 45°)`: (−6,0), (0,6); `DistanceAngle(6, 30°)`: (−6,0), (0, 6·tan 30°). Bogen als Kurve, Fase länger als die Linie, θ so groß, dass der Nenner ≤ 0 ⇒ `None`.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch --test fillet_plan` grün.
- [ ] **Step 5: Commit** „Skizze: Geometrie von Rundung und Fase“.

### Task 2: Rundung und Fase mit ihren Abhängigkeiten

**Files:**
- Modify: `crates/planora-sketch/src/fillet.rs` (Operationen), `edit.rs` (`drop_orphans` `pub(crate)`; `settle_constraints` ist es schon)
- Test: `crates/planora-sketch/tests/fillet_constraints.rs` (neu)

**Interfaces:**
- Consumes: Task 1; `atomically`, `set_curve_kind`, `settle_constraints`, `drop_orphans`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct FilletIds { pub arc: CurveId, pub center: PointId, pub touch: [PointId; 2], pub corner: Option<PointId>, pub helper: Option<CurveId>, pub radius_dim: Option<ConstraintId> }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct ChamferIds { pub line: CurveId, pub touch: [PointId; 2], pub corner: PointId, pub helpers: Option<[CurveId; 2]>, pub dims: Vec<ConstraintId> }`
  - `impl Sketch { pub fn add_fillet(&mut self, plan: &FilletPlan, equal_to: Option<CurveId>) -> Result<FilletIds, SketchError>; pub fn add_chamfer(&mut self, plan: &ChamferPlan) -> Result<ChamferIds, SketchError> }`: Abhängigkeiten nach Entscheidung 1, 2, 9, atomar. Mit `equal_to` kein `Radius`, sondern `Equal`. Die Punkte kommen bitgleich aus dem Plan.

- [ ] **Step 1: Tests** (`Solved`, `redundant` und `conflicting` leer, `iterations == 0`, außer numerisch)
  - **Tabelle der Freiheitsgrade aus Entscheidung 1 und 9:** Linie–Linie, –Bogen, Bogen–Bogen, Linie–Kreis, mit und ohne fremdes Maß an der Ecke, jede Fasenart: rel wie gemessen. Maß an der Ecke (`Distance(a,P)`) überlebt mit derselben Id; ohne fremden Bezug verschwindet `P`.
  - **Vorschau = Ergebnis:** `at(touch[i])` und Bogenmitte gleich dem Plan unter 1e-12.
  - **Dichte Folgen:** `Radius` über die Folge aus Entscheidung 1: freie Linien alle `Solved`, feste Fernenden bis 85 % der Grenze `Solved`, darüber gelöst oder Stand bleibt; immer `folded_over(vorher, nachher) == false`, Tangentenfehler unter 1e-6. Rechteck 50×30 mit H, V, Maßen, vier Rundungen `Equal`: dof wie vorher, r 0,1…14,9 gelöst, H und V halten; r ≥ 15 nicht `Solved`, Stand bleibt. Fasen-Folge 0,1…37 je Art, Winkel ±0,9 rad.
  - **Abhängigkeiten wie beim Stutzen:** H an `L1` bleibt, `Midpoint(q,L1)` entfällt, `Equal(L1,L9)` entfällt, wenn die Länge nicht mehr stimmt; `Fixed` am fernen Ende bleibt. Dritte Kurve an der Ecke: `P` bleibt, sie bleibt dran.
  - **Review-Focus 4:** feste Enden (`Fixed` an `a`, `b`): `Solved`, Punkte unbewegt; Ecke im Ursprung gebunden; Rundung an einer projizierten Linie (feste Punkte): die feste Linie wird nicht umgehängt, wenn ihr Ende `Fixed` ist ⇒ `Whole` statt `Replace`, Plan bleibt gültig.
  - Eine Fläche: abgerundetes Rechteck `profiles().profiles.len() == 1`, `P` und Hilfen stören nicht; Fase ebenso.
  - Ellipse: `exact == false`, `redundant` leer; Linie–Spline ebenso; Linie folgt nicht der Ellipse beim Ändern der Halbachse (Grenze festgehalten).
  - Rundung, dann Rückweg per Skizzenkopie: Rundlauf `to_data`/`from_data` gleich, `required_format` wie vor der Rundung (Test in `planora-project`, Muster `centerline_format.rs`, kein neuer Formatzweig).
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch --test fillet_constraints` grün.
- [ ] **Step 5: Commit** „Skizze: Rundung und Fase mit Abhängigkeiten“.

### Task 3: Geometrie des Versatzes

**Files:**
- Create: `crates/planora-sketch/src/offset.rs`; `lib.rs`
- Modify: `crates/planora-sketch/src/arrangement.rs` (`Nodes`, `split` `pub(crate)`), `intersect.rs` (`boxes_touch` `pub(crate)`), `curves.rs`/`geom.rs` nach Bedarf (`Sampled::spline` ist `pub(crate)`)
- Test: `crates/planora-sketch/tests/offset_plan.rs` (neu), `crates/planora-sketch/tests/offset_perf.rs` (neu, `#[ignore]`)

**Interfaces:**
- Consumes: Task 1 (`ArcPlan`), Entscheidung 6, 7.
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct OrderedChain { pub items: Vec<(CurveId, bool)>, pub closed: bool }` (`bool`: gegen die Kurvenrichtung)
  - `#[derive(Clone, Debug, PartialEq)] pub enum OffsetGeom { Line { a: DVec2, b: DVec2 }, Arc(ArcPlan), Circle { center: DVec2, radius: f64 }, Approx { points: Vec<DVec2> } }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct OffsetPiece { pub source: CurveId, pub geom: OffsetGeom }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct OffsetLoop { pub pieces: Vec<OffsetPiece>, pub closed: bool }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct OffsetPlan { pub loops: Vec<OffsetLoop>, pub distance: f64, pub dropped: usize }`; `impl OffsetPlan { pub fn outline(&self, steps_per_turn: usize) -> Vec<Vec<DVec2>>; pub fn piece_count(&self) -> usize; pub fn is_closed_like_source(&self) -> bool }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum OffsetProblem { Branching, Vanished, NoJoin, SelfCrossing, Cusp, Degenerate }`
  - `impl Sketch { pub fn ordered_chains(&self, curves: &[CurveId]) -> Result<Vec<OrderedChain>, OffsetProblem>; pub fn offset_side(&self, chain: &OrderedChain, pointer: DVec2) -> Option<f64>; pub fn offset_plan(&self, chains: &[OrderedChain], left_distance: f64) -> Result<OffsetPlan, OffsetProblem> }`. `offset_side`: mit Vorzeichen, positiv links der Kettenrichtung, Betrag Abstand zur Kette; `left_distance` ebenso.

- [ ] **Step 1: Tests**
  - Linie (0,0)–(30,0), links 5: (0,5)–(30,5). Bogen um (0,0) r 20, gegen den Uhrzeigersinn, links (nach innen) 5: r 15; rechts 5: r 25. Kreis r 20: links innen. Quadrat 100, geschlossen, nach außen 10: vier Linien, Ecken (−10,−10)…, Länge 120, `closed`.
  - **Tangentiale Kette:** abgerundetes Rechteck: Bögen außen r+d, innen r−d; alle Fugen stimmen unter 1e-9, gemeinsame Punkte bitgleich. Innen d ≥ r: Bögen entfallen, `dropped` zählt, Linien treffen sich in Ecken, Ergebnis `closed`.
  - **Dichte Folge:** d von 0,1 bis Grenze in 200 Schritten für Quadrat, L-Form, Stern (Abwechselnd konkav/konvex), abgerundetes Rechteck, innen und außen: jedes Ergebnis `closed`, **kein Stück kreuzt ein anderes** (Test zählt selbst mit `intersect`), jeder Stückmittelpunkt hat Abstand `|d|` zur Vorlage unter 1e-6, nie NaN. Seite wechselt nur mit dem Vorzeichen: Fläche der Schleife wächst außen, schrumpft innen.
  - **Trimmen:** L-Form innen: überstehende Stücke entfallen. Hantel (zwei Quadrate, schmaler Steg) innen so weit, dass der Steg verschwindet: zwei geschlossene Schleifen. Quadrat innen d = 50: `Vanished`. U-Form innen über die halbe Schlitzbreite: Schleifen bleiben geschlossen.
  - **Offene Kette:** drei Linien: drei Stücke, Enden je auf der Normalen des Vorlagen-Endes; Überstände entfallen. `ordered_chains` mit T-Stück `Branching`; Konstruktionslinie an einer normalen Kette zählt nicht.
  - **Näherung:** Ellipse 40×20, d = 5: zwei `Approx` mit je 33 Punkten, jeder Punkt genau im Abstand 5, Abweichung des Splines dazwischen unter 0,03; d = −11: `Cusp`. Spline: 4 Abschnitte je Stützabschnitt, höchstens 64 Punkte. Linie mit angehängtem Ellipsenbogen: gemeinsamer Punkt.
  - `offset_side` bei Quadrat: Zeiger außen < 0, innen > 0 (bei Lauf gegen den Uhrzeigersinn), Betrag stimmt. Gleiche Eingabe bitgleich, `revision` bleibt.
  - **Messtest:** Kette aus 200 Kurven, Linie und Bogen im Wechsel, außen und innen: `ordered_chains`, `offset_plan`, `outline` zusammen Median unter 16 ms im Release.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch --test offset_plan` und Messtest grün.
- [ ] **Step 5: Commit** „Skizze: Geometrie des Versatzes mit Trimmen“.

### Task 4: Versatz mit Abhängigkeiten, Kopie folgt der Vorlage

**Files:**
- Modify: `crates/planora-sketch/src/offset.rs`, `solver.rs` (`solve_holding`)
- Test: `crates/planora-sketch/tests/offset_constraints.rs` (neu), `crates/planora-sketch/tests/offset_perf.rs`

**Interfaces:**
- Consumes: Task 3.
- Produces:
  - `pub const OFFSET_TIED_MAX: usize = 64;`
  - `#[derive(Clone, Debug, PartialEq)] pub struct OffsetIds { pub curves: Vec<CurveId>, pub rollers: Vec<CurveId>, pub distance_dim: Option<ConstraintId>, pub tied: bool }`
  - `impl Sketch { pub fn add_offset(&mut self, plan: &OffsetPlan) -> Result<OffsetIds, SketchError>; pub fn offset_hold_points(&self, dim: ConstraintId) -> Option<Vec<PointId>>; pub fn solve_holding(&mut self, hold: &[PointId]) -> bool }`
  - `add_offset`: Reihenfolge nach Entscheidung 3 (Kurven, `Parallel`, `Tangent`, Rollkreise, Querlinien), einmal lösen, angelegte überzählige entfernen. Über `OFFSET_TIED_MAX`: nur Kurven, `tied == false`, `distance_dim == None`.
  - `offset_hold_points`: `Radius(c,_)` an einem Konstruktionskreis mit `Tangent` zu einer normalen Kurve, sonst `None`; Ergebnis alle Punkte aller normalen Kurven, die über gemeinsame Enden von den Mitten der ganzen `Equal`-Gruppe aus erreichbar sind, samt deren Mitten; bei einer Vorlage aus Konstruktionskurven mit gleichem Stil. `solve_holding`: Kopie, `Fixed` auf `hold`, lösen; bei `Solved` Werte übernehmen, `touch_values`, `true`; sonst `false`, nichts geändert.

- [ ] **Step 1: Tests**
  - **Tabelle aus Entscheidung 3:** Linie, Bogen, Kreis (außen, innen), geschlossene Kette n = 3, 4, 5, 8, 64, abgerundetes Rechteck, L-Form mit zwei Rundungen, offene Kette: `Solved`, `redundant` und `conflicting` leer, `iterations == 0`, rel 0. Abgerundetes Rechteck: eine `Parallel` weniger als Linien, genau ein Rollkreis. Quadrat: vier Rollkreise, ein `Radius`, drei `Equal`. Vorlage ohne jede Bewegung (bitgleich).
  - **Dichte Folgen** (Fehler unter 1e-6, 0 Umklapper, alle Stücke im richtigen Abstand): Maß 0,5…50 für alle n bis 64; Kreis außen 0,1…50, innen 0,1…19; Bogen ebenso; L-Form bis d < 6, ab d = 6 abgelehnt (nicht gelöst, Stand bleibt). **Vorlage-Radius** 0,25…100 außen und innen: außen alle, innen genau die Werte r ≤ d nicht gelöst, nie umgeklappt.
  - **Vorlage bleibt:** `offset_hold_points(dim)` deckt die Punkte der Vorlage, nicht die der Kopie; `solve_holding` bei allen Folgen oben: Vorlage bitgleich, Kopie stimmt. Nur Linie: d 5→12 Vorlage unbewegt (gegen gemessene 3,5 mm ohne Halten). `offset_hold_points` an einem gewöhnlichen `Radius` eines Bogens und an einem von Hand gezeichneten Kreis `None`.
  - **Kopie folgt:** Vorlage-Ecke ziehen: Abstand bleibt (Fehler unter 1e-12), `drag_failed` falsch. Maß an der Vorlage: Linie 0,5…150; Kopie gleich lang, parallel. Rundungsradius der Vorlage ändert Kopie mit.
  - **Lange Kette:** n = 65: `tied == false`, keine `Parallel`, Punkte teilen die Enden, `Solved`. Messtest `#[ignore]`: n = 64 Anlegen unter 100 ms im Release.
  - **Review-Focus 2:** Rollkreis löschen ⇒ `Solved`, Kopie frei, Vorlage bleibt; erster Rollkreis mit dem Maß löschen ebenso; Querlinie löschen; Stutzen einer Kopie-Linie. Kopie löschen: Rollkreise bleiben oder gehen ohne toten Bezug (`Solved`). **Review-Focus 4:** Versatz einer projizierten Kante (`Fixed`): `Solved`, Kopie normal, folgt nach `reproject_edge`.
  - Profile: Quadrat mit Versatz außen: zwei Flächen (Ring und Innenfläche), Rollkreise sind Konstruktion und teilen nichts. Näherung: `Approx`-Stücke ohne Abhängigkeit, geschlossene Ellipse ergibt ein Profil. Rundlauf `to_data`/`from_data` gleich, `required_format` unverändert.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch` grün, Messtest grün.
- [ ] **Step 5: Commit** „Skizze: Versatz mit Abhängigkeiten, die Kopie folgt der Vorlage“.

### Task 5: Verschieben, Drehen, Skalieren, Kopieren als Rechnung

**Files:**
- Create: `crates/planora-sketch/src/transform.rs`; `lib.rs`
- Test: `crates/planora-sketch/tests/transform.rs` (neu)

**Interfaces:**
- Consumes: Entscheidung 10, 11; `atomically`, `satisfied`, `set_curve_kind`, `add_constraint`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Similarity { pub base: DVec2, pub offset: DVec2, pub angle: f64, pub factor: f64 }`; `impl Similarity { pub fn identity(base: DVec2) -> Similarity; pub fn apply(&self, p: DVec2) -> DVec2; pub fn is_identity(&self, tol: f64) -> bool }`
  - `#[derive(Clone, Debug, PartialEq, Default)] pub struct TransformSet { pub points: Vec<PointId>, pub curves: Vec<CurveId> }`
  - `#[derive(Clone, Debug, PartialEq)] pub enum TransformBlock { FixedInSet(PointId), BoundToFixed(ConstraintId), NothingToDo }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct TransformReport { pub dropped: Vec<ConstraintId>, pub scaled: Vec<ConstraintId> }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct CopyMap { pub points: HashMap<PointId, PointId>, pub curves: HashMap<CurveId, CurveId>, pub constraints: HashMap<ConstraintId, ConstraintId> }`
  - `impl Sketch { pub fn transform_set(&self, curves: &[CurveId], points: &[PointId]) -> TransformSet; pub fn whole_sketch_set(&self) -> TransformSet; pub fn check_transform(&self, set: &TransformSet, s: &Similarity) -> Result<(), TransformBlock>; pub fn apply_transform(&mut self, set: &TransformSet, s: &Similarity) -> Result<TransformReport, TransformBlock>; pub fn copy_transformed(&mut self, set: &TransformSet, s: &Similarity) -> Result<CopyMap, SketchError>; pub fn transform_preview(&self, set: &TransformSet, s: &Similarity, steps_per_turn: usize) -> Vec<Vec<DVec2>> }`
  - `apply_transform`: ändert nur bei `Ok`. Kein Solver. `transform_preview` wendet `apply_transform` auf eine Kopie an und liefert `curve_polyline` der Menge: dieselbe Rechnung. Bewegte `Fixed` außerhalb der Regel gibt es nicht: `FixedInSet`.

- [ ] **Step 1: Tests**
  - Rechteck mit H, V, `Distance`, verschoben um (7,−3): alle Abhängigkeiten bleiben, `solve` danach `iterations == 0`, Lage genau. Gedreht um 30° um (5,5): H und V entfallen, `dropped` nennt sie, Abstände bleiben, Skalarprodukte stimmen; gedreht mit `Parallel` und `Perpendicular` (Rechteck 3 Punkte): alles bleibt. Tangentialer Bogen, Polygon 64, Langloch, abgerundetes Rechteck: drehen und verschieben über **dichte Folge** (Winkel 0…360° in 1°, Versatz bis 500): `iterations == 0`, kein Fehler über 1e-9, `redundant` wie vorher.
  - **Äußeres:** Linie eines Rechtecks allein: Die drei anderen bleiben unbewegt, bekommen einen neuen Punkt an der alten Lage, `Coincident` zur Linie gibt es nicht mehr, geteilte `PointId` ist getrennt. `Distance` zu einem fremden Punkt entfällt, wenn nicht mehr erfüllt; bleibt erfüllt (Drehung um den fremden Punkt): bleibt. `Tangent` am geteilten Ende an fremdem Bogen: entfällt.
  - **Fest:** `Fixed` in der Menge ⇒ `FixedInSet`, Skizze bitgleich. `PointOnCurve` an projizierter Linie, die danach nicht gilt ⇒ `BoundToFixed`; entlang der Linie verschoben ⇒ geht. Keine Menge oder Identität ⇒ `NothingToDo`.
  - **Skalieren:** k = 2 um (0,0): Kreisradius, Ellipse, Spline, Bögen; `Distance`, `Radius`, `Diameter`, `DistancePointLine`, `EllipseMajor`, `EllipseMinor` verdoppelt, `Angle` gleich, `scaled` nennt sie, `iterations == 0`. Dichte Folge k 0,01…100: kein Fehler über 1e-6·k. `k ≤ 0`, NaN ⇒ `Degenerate` über `check_transform`. Fremder Bezug: nicht skaliert, entfällt oder bleibt nach Regel. **Ganze Skizze:** `whole_sketch_set` ohne Achshilfslinie, Kantenkopien, festen Ursprung; Bezugspunkt im Ursprung mit Ecke `Coincident` am Ursprung: geht; Bezugspunkt woanders ⇒ `BoundToFixed`.
  - **Kopieren:** Rechteck mit Maßen und Bogen, Ellipse, Spline, Mittellinie: gleiche Abhängigkeiten mit abgebildeten Ids, `driven`, `enabled`, Stil; `Fixed` nicht; Original bitgleich, `Solved`, `redundant` leer; `Maß an Kopie ändern` lässt das Original stehen. Punkt allein. Geteilter Punkt zwischen zwei gewählten Kurven bleibt in der Kopie geteilt. Gedreht: H, V in der Kopie nur, wenn erfüllt.
  - **Vorschau = Ergebnis:** `transform_preview` und die `curve_polyline` nach `apply_transform` unter 1e-12 gleich; `revision` unverändert. Messtest in Task 9.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-sketch --test transform` grün.
- [ ] **Step 5: Commit** „Skizze: Verschieben, Drehen, Skalieren, Kopieren als Rechnung“.

### Task 6: Werkzeuge, Arten, Aktionen, Felder

**Files:**
- Modify: `crates/planora-app/src/sketch_state.rs` (`Tool::Fillet`, `Offset`, `MoveCopy`, `Scale`; `is_modify`; `click` schickt sie an Stubs), `sketch_cursor.rs` (`is_drawing_tool` unverändert korrekt prüfen), `sketch_kinds.rs` (`ToolKind` +4, `ToolKinds.fillet`, `of`, `stages`, `action`, `hint`, `is_classic` bleibt), `sketch_input.rs` (`FieldKind` nach Entscheidung 13, `unit`, `label`, `read_field`, `key` um `transform_stage`), `sketch_panel.rs` (`TOOL_ORDER` 18, `MODIFY_FIRST`, `tools`, `tool_label`, `tool_hint`), `shell.rs` (`STRIP_TOOLS = 18`, Kommentar), `icons.rs` (`HandIcon::Fillet`, `Chamfer`, `Offset`, `MoveCopy`, `Scale`, `HAND_DRAWN`), `keymap/defaults.rs` (Tabelle, `sketch_tool`, Kommentar `:99`), `commands.rs` (`run_action_inner`; `observe_sketch` unverändert, jeder Abschluss der vier Werkzeuge zählt `modify_edits` hoch), `sketch_kinds.rs` `kind_of` (Arm `Tool::Fillet`), `app.rs` (`sketch_kinds_menu` ohne `Tool::Fillet`; `SketchOffset` in `create_menu`; `edit_menu`: Untermenü `menu-sketch-modify`), `de.ftl`/`en.ftl` (Namen nach Entscheidung 14, `sketch-tool-fillet`, Hinweise `sketch-tool-…-hint`, `sketch-kind-…-hint`, `sketch-field-distance`, `-distance2`, `-offset`, `-move-x`, `-move-y`, `-move-angle`, `-scale-factor`, `sketch-field-offset-nonzero`, `sketch-field-factor-positive`, `menu-sketch-modify`, `undo-fillet`, `undo-chamfer`, `undo-offset`, `undo-move-copy`, `undo-scale`)
- Test: `crates/planora-app/tests/sketch_modify_tools.rs` (neu); bewusst: `tests/icons.rs:261`, `sketch_english.rs:261`, `sketch_kinds.rs:560`, `sketch_trim.rs:399` (14 → 18, Liste ergänzt); `sketch_kinds.rs:208`, `:224` (19 → 23); `keymap_sketch_drawing.rs:293` (`O` ist jetzt Versatz, `D` bleibt frei; `SKETCH_KEYS` um F, O, M); `docs/superpowers/offene-punkte.md:1841` (O gibt es jetzt, kommt mit Task 11)

**Interfaces:**
- Consumes: nichts Neues aus `planora-sketch`.
- Produces:
  - `Action::SketchFillet` … `SketchScale` nach Entscheidung 14, Bereich `Sketch`.
  - `ToolKind::Fillet, ChamferEqual, ChamferDistanceAngle, ChamferTwoDistances`; `ToolKind::of(Tool::Fillet)` = diese vier; `sketch_flyout.rs:682` bleibt 19, weil Erzeugen `Tool::Fillet` auslässt.
  - `FieldKind::{Distance, Distance2, OffsetDistance, MoveX, MoveY, MoveAngle, ScaleFactor}`. `read_field`: `OffsetDistance` endlich und nicht 0; `MoveX`, `MoveY` endlich, auch 0 und negativ; `MoveAngle` endlich, Grad, wird rad; `ScaleFactor` endlich > 0. Nichts davon bedient das Dateiformat. `kind_index` nutzt `as usize`, also muss die Reihenfolge nur eindeutig sein.
  - `Felder SketchSession`: `fillet_run: Option<(CurveId, f64)>`, `offset_chain: bool` (true), `pick_points: Vec<DVec2>`, `pick_radius: f64`, `transform: TransformState`, `grip_drag: Option<GripDrag>`; `Backup`/`restore`, `after_history_jump`, `forget_removed`, `set_tool` kennen sie. `pick_points` zählt nicht in `kind_stage`.
  - `#[derive(Clone, Debug, PartialEq, Default)] pub struct TransformState { pub stage: TransformStage, pub set: TransformSet, pub base: Option<DVec2>, pub offset: DVec2, pub angle: f64, pub factor: f64, pub copy: bool, pub whole: bool }`, `#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)] pub enum TransformStage { #[default] Picking, SetBase, Active }` (Task 9, 10 füllen die Logik).

- [ ] **Step 1: Tests** – Leiste: 18 Knöpfe, alle bei 1920×1080 im Fenster, englisch; Trennlinie vor `Trim`. Dreieck nur an Rechteck, Kreis, Bogen, Polygon, Langloch, Rundung. `F`, `O`, `M` wählen, Scale und die Fasen ohne Taste, alle sieben belegbar (`km.set`), Name in den Einstellungen, Id eindeutig; `F` in `Model` bleibt `Fillet`, in `Sketch` `SketchFillet`, `conflict_for` meldet keinen Konflikt zwischen den Bereichen; Konfiguration ohne Änderung bleibt leer. „Letztes Skizzenwerkzeug wiederholen“ nimmt die Fase-Art nach ihrer ersten Änderung. Menü „Bearbeiten“ zeigt das Untermenü nur bei offener Skizze, „Erzeugen“ zeigt `Versatz`, ohne Skizze wie heute. Esc in jedem der vier Werkzeuge führt zu `Select`, `revision` unverändert. Felder: `MoveX` „−3“ gesperrt −3; `OffsetDistance` „0“ `sketch-field-offset-nonzero`, „−5“ gesperrt −5; `ScaleFactor` „0“ und „−1“ `sketch-field-factor-positive`, in Zoll bleibt „2“ 2; `Distance` „1“ in Zoll 25,4.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Werkzeuge, Arten, Kürzel und Felder für Ändern“.

### Task 7: Rundung und Fase in der App

**Files:**
- Modify: `crates/planora-app/src/sketch_kinds.rs` (`picks_curves`, `click_kind`, `plan_with`, `kind_preview`, `kind_locked_position`, `kind_field_value`, `finish_kind`), `sketch_input.rs` (`Created::Fillet`, `Created::Chamfer`, `undo_label`, `commit_typed`: getippter Radius oder Abstand wird über `dimension_input` Wert und Formel des schon angelegten Maßes), `sketch_view.rs` (Hervorhebung `pending_curves` gibt es schon), `de.ftl`/`en.ftl` (`sketch-fillet-too-big`, `sketch-fillet-no-corner`, `sketch-fillet-kinds`, `sketch-fillet-point-curves`, `sketch-chamfer-lines`, `sketch-chamfer-too-big`)
- Test: `crates/planora-app/tests/sketch_fillet.rs` (neu)

**Interfaces:**
- Consumes: Task 1, 2, 6; `DrawCursor`, `check_change`.
- Produces: `picks_curves()` wahr für diese Arten bis Stufe 2. Stufe 0 prüft zuerst `sk.pick_point(cur.raw, pick_radius)`: Punkt mit genau zwei `curves_at_point` ⇒ beide auf einmal, Klickstellen je ein Punkt auf der Kurve; mehr oder weniger ⇒ `sketch-fillet-point-curves`. `pick_points` hält die Klickstellen. Stufe 2: Wert aus Feld, sonst Abstand Zeiger–`fillet_corner`; Vorschau `fillet_plan`/`chamfer_plan`. Abschluss in einem Zug, `solve_checked`, `edit_label`. Rundung: `fillet_run = Some((arc, r))`, danach `clear_pending`; im Lauf schließt der zweite Klick ab. `forget_removed` und `after_history_jump` vergessen `fillet_run`; `cancel` beendet das Werkzeug.

- [ ] **Step 1: Tests** (`click`/`commit_input`, je eine Fahrt über Rohereignisse bei 1920×1080, Muster `sketch_trim.rs` `play`)
  - Rechteck per `R`, `F`, zwei Seiten anklicken, „5“ tippen, Klick: Bogen, `Radius` 5, zwei `Tangent`, H und V bleiben, `Solved`, dof wie vorher. **Vorschau = Ergebnis** unter 1e-9. Nächste Ecke: zwei Klicks, Rundung sofort, `Equal`, kein zweites `Radius`; `Radius` auf 8 ändert beide; Esc beendet, neue Rundung wieder mit Feld. Klick auf den gemeinsamen Punkt reicht; Punkt mit drei Kurven: Meldung.
  - Zeiger-Radius: Zeiger weiter weg ⇒ größerer Bogen, keine Sprünge über eine **dichte Zeigerfolge** von 500 Lagen (Mitte stetig, kein Vorzeichenwechsel). Strg ändert nichts am Wählen.
  - Kreuz: Klickstellen wählen den Quadranten. Linie–Kreis: Kreis bleibt ganz, Linie gekürzt.
  - **Review-Focus 1:** parallele Linien, gleiche Kurve zweimal, Radius über die Linie ⇒ keine Vorschau, Meldung, Stufe bleibt, `revision` gleich; Klick ins Leere tut nichts.
  - **Review-Focus 3:** Esc in Stufe 1, 2: nichts bleibt, kein Rückgängig-Schritt. Ein Rückgängig nimmt Bogen, Maß, Punkt, Umhängen; Wiederholen bitgleich. Gelöschte Kurve unter `pending_curves` fällt heraus.
  - **Review-Focus 5:** Zoll: „1“ ⇒ 25,4; Formel `Breite / 2` am `Radius`, nach Parameteränderung folgt der Bogen. Radius-Änderung über das Maßfeld: `guard_length` läuft, Rechteckgrenze wird abgelehnt.
  - Fase, je Art: zwei Klicks, Felder, Maße wie Entscheidung 9, Ecke bleibt; Bogen als Kurve: `sketch-chamfer-lines`; Abstand länger als die Linie: `sketch-chamfer-too-big`. `Tab` wechselt Abstand und Winkel. „Abstand und Winkel“ Maß Winkel auf 30° ändern: Ecke bleibt, `T2` wandert, kein Umklappen.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Rundung und Fase bedienbar“.

### Task 8: Versatz in der App

**Files:**
- Modify: `crates/planora-app/src/sketch_state.rs` (`click` → `click_offset`, `set_dimension_value` mit `offset_hold_points`/`solve_holding` vor `solve_checked`), `sketch_input.rs` (`Created::Offset`, `stage_fields`: `Tool::Offset` nach der Kettenwahl `[OffsetDistance]`), `sketch_kinds.rs` (`picks_curves` auch für `Tool::Offset` ohne Kette), `sketch_view.rs` (`draw_preview` zuerst `offset_preview`), `sketch_panel.rs` (`tool_options`: Haken „Kette“), `de.ftl`/`en.ftl` (`sketch-offset-chain`, `sketch-offset-vanished`, `sketch-offset-crossing`, `sketch-offset-branching`, `sketch-offset-no-join`, `sketch-offset-cusp`, `sketch-offset-not-tied`, `sketch-offset-dropped`)
- Test: `crates/planora-app/tests/sketch_offset.rs` (neu)

**Interfaces:**
- Consumes: Task 3, 4, 6.
- Produces:
  - `impl SketchSession { pub fn offset_plan_at(&self, sk: &Sketch, at: DVec2) -> Result<OffsetPlan, OffsetProblem>; pub fn offset_preview(&self, sk: &Sketch, at: DVec2) -> Option<Vec<Vec<DVec2>>>; fn click_offset(&mut self, sk: &mut Sketch, cur: &DrawCursor, prior: &Sketch) -> Result<(), Msg> }`
  - Ablauf: Vorauswahl mit Kurven beim Wählen des Werkzeugs ⇒ gleich Stufe „Abstand“, sonst Klick wählt Kurve bzw. mit `offset_chain` `connected_chain`; `pending_curves` hält die Menge. Abstand: getippt, Seite vom Zeiger (`offset_side`), negativ wechselt; sonst Betrag und Seite vom Zeiger. Klick oder Enter schließt ab; danach bleibt das Werkzeug aktiv (Fusion: **Annahme**) ohne Kette. `commit_typed`: `Created::Offset` legt kein zweites Maß an, der getippte Text wird Wert (Betrag) und Formel von `distance_dim`, wie in Task 7.
  - `set_dimension_value`: ein Zweig, der nur bei `offset_hold_points(id)` wirkt; `guard_length` bleibt für alle.

- [ ] **Step 1: Tests**
  - Quadrat: `O`, Klick auf eine Seite wählt mit „Kette“ alle vier; Zeiger außen: Vorschau außen, Klick: vier Linien, ein `Radius`, `Solved`, `redundant` leer, Vorlage bitgleich. **Vorschau = Ergebnis** unter 1e-9. „Kette“ aus: nur die Linie, mit Querlinien. Vorauswahl aus Doppelklick: ohne weiteren Klick Abstand.
  - Tippen „10“: Zeiger innen ⇒ innen; „−10“ bei Zeiger innen ⇒ außen. Zeiger über die Kette hinüber: **dichte Zeigerfolge**, Seite wechselt genau dort, Vorschau springt nicht sonst. Gerade offen: Zeiger rechts/links. Trimmen: L-Form innen ist geschlossen, zeigt keine Kreuzung.
  - **Maß ändert nur die Kopie:** Maßfeld 5→12 bei freier Vorlage: Vorlage bitgleich, `redundant` leer, ein Schritt; Wert, der einen Bogen verschwinden lässt: abgelehnt, Stand bleibt. Hand-Kreis: `Radius` ändern verhält sich wie vorher (Skizze mit und ohne diesen Code bitgleich an den Tests von `sketch_state.rs`).
  - **Review-Focus 1:** Versatz zu groß: keine Vorschau, `sketch-offset-vanished`, Skizze bitgleich; Ellipse innen zu groß `sketch-offset-cusp`; T-Stück `sketch-offset-branching`.
  - **Review-Focus 3:** Esc nach der Kettenwahl: nichts bleibt; Rückgängig nimmt alle Kurven, Rollkreise, Maß; Wiederholen bitgleich. **Review-Focus 5:** Zoll, Formel am `Radius` des Rollkreises folgt; Formel-Weg hält die Vorlage nicht (Grenze, Test hält den Stand fest, damit ein späterer Umbau bewusst geschieht).
  - Lange Kette: 65 Linien: Meldung `sketch-offset-not-tied`, Kopie ohne Abhängigkeiten. Messtest: `offset_preview` 200 Kurven unter 16 ms im Release (`#[ignore]`). Haken passt in Feldbreite 250, englisch.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Versatz bedienbar, Maß bewegt nur die Kopie“.

### Task 9: Griffe, Verschieben und Kopieren

**Files:**
- Create: `crates/planora-app/src/sketch_grips.rs`; `lib.rs`
- Modify: `crates/planora-app/src/sketch_view.rs` (`handle_input` nach Entscheidung 12, `box_can_start`, `draw` ruft `sketch_grips::paint`, `draw_preview`), `sketch_state.rs` (`click`, `cancel`, `finish_tool`, `is_dragging` bleibt), `sketch_input.rs` (`stage_fields`, `field_value`, `Created` nicht, Filter in `commit_typed` wie `Sides`), `sketch_panel.rs` (`tool_options`: Haken „Kopie erstellen“, „Bezugspunkt setzen“, OK, Abbrechen), `de.ftl`/`en.ftl` (`sketch-move-copy`, `sketch-move-set-base`, `sketch-move-dropped`, `sketch-move-fixed`, `sketch-move-nothing`, `common-ok`, wenn nötig)
- Test: `crates/planora-app/tests/sketch_move_copy.rs` (neu), `crates/planora-app/tests/sketch_grips.rs` (neu)

**Interfaces:**
- Consumes: Task 5, 6.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Grip { Free, AxisX, AxisY, Rotate, Scale }`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct GripLayout { pub center: Pos2, pub x_tip: Pos2, pub y_tip: Pos2, pub ring: f32, pub scale: bool }`; `pub fn layout(center: Pos2, x_dir: Vec2, y_dir: Vec2, scale: bool) -> GripLayout` (`x_dir`, `y_dir` normiert aus der Abbildung der Skizzenachsen, also richtig auf gekippten Ansichten); Konstanten `GRIP_ARROW_PX = 60.0`, `GRIP_RING_PX = 40.0`, `GRIP_HIT_PX = 8.0`.
  - `pub fn hit(l: &GripLayout, p: Pos2) -> Option<Grip>` (Reihenfolge Mitte, Pfeile, Ring; im Skalieren nur `Scale` auf einer Handhabe rechts)
  - `pub fn paint(painter: &egui::Painter, l: &GripLayout, hot: Option<Grip>, color: Color32, hot_color: Color32)` (Farben aus `colors(state.dark)` wie in `draw`, nur Tokens)
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct GripDrag { pub grip: Grip, pub press: DVec2, pub start_offset: DVec2, pub start_angle: f64, pub start_factor: f64 }`
  - `pub fn grip_motion(d: &GripDrag, base: DVec2, now: DVec2) -> (DVec2, f64, f64)` rein: Offset, Winkel, Faktor; `Free` Differenz, `AxisX`/`AxisY` nur die Achse, `Rotate` `atan2`-Differenz um `base` (über ±π durchlaufend), `Scale` `|now−base| / |press−base|`, nie ≤ 0.
  - `impl SketchSession { pub fn begin_transform(&mut self, sk: &Sketch); pub fn transform_now(&self) -> Option<Similarity> /* Felder gesperrt vor Griff */; pub fn begin_grip(&mut self, grip: Grip, at: DVec2); pub fn grip_to(&mut self, at: DVec2); pub fn end_grip(&mut self); pub fn abort_grip(&mut self); pub fn finish_transform(&mut self, sf: &mut SketchFeature) -> Result<(), String>; pub fn transform_preview(&self, sk: &Sketch) -> Option<Vec<Vec<DVec2>>> }`
  - Ablauf: Wahl mit Vorauswahl ⇒ `Active`, Bezugspunkt in der Mitte der Hülle; ohne ⇒ `Picking`, Klick schaltet Kurve oder Punkt (Rahmen geht), Enter ⇒ `Active`. „Bezugspunkt setzen“ ⇒ `SetBase`, Klick mit Fang ⇒ zurück `Active`, Offset und Winkel unverändert. Enter oder OK: `finish_transform` macht Verschieben oder Kopieren, `solve_checked`, `edit_label`, Werkzeug `Select`, Auswahl bleibt (bei Kopie: die Kopien). Identität ohne Kopie: `sketch-move-nothing`, alles bleibt.

- [ ] **Step 1: Tests**
  - **Rein (`sketch_grips.rs`):** `hit` je Griff in Bildpunkten, Rand, Reihenfolge; gekippte Abbildung: Pfeile folgen. `grip_motion` Achse, Mitte; Drehen über ±180° durchgehend, 720° bleibt 720°; Faktor 2 bei doppeltem Abstand; `press == base` nie NaN. **Dichte Zeigerfolgen** ohne Sprung.
  - **Griffe ohne Konflikt** (Rohereignisse, 1920×1080): Ziehen an der Mitte verschiebt nur die Vorschau, `revision` gleich, kein `curve_drag`, kein Rahmen, kein `box_select`. Ziehen neben den Griffen im `Active`: nichts (kein Rahmen). Im `Picking`: Rahmen wählt, Griffe erscheinen erst danach. Umschalt+links während des Griffs schwenkt nicht; Mitte schwenkt immer. Offene Liste: Drücken auf einen Griff schließt nur sie. Rechte Taste und Esc im Griffzug: nur der Zug zurück, nächstes Esc beendet. Auswahl-Werkzeug zieht Kurven wie vorher (`sketch_curve_drag.rs`, `sketch_select.rs` unverändert grün). Felder liegen nicht auf den Griffen als Hindernis (Felder nehmen keine Klicks).
  - **Verschieben:** Rechteck: Felder X 10, Y 5 ⇒ genau dort, Bezugspunkt egal; Griff-Ziehen und Feld zusammen: Feld gewinnt. Winkel 30°. Bezugspunkt per Fang auf Ecke, Drehen um sie: Ecke unbewegt. Haken Kopie: Original bitgleich, Kopie trägt Maße, Formel mitkopiert. **Vorschau = Ergebnis** unter 1e-9. Linie allein: Nachbarn bleiben, Meldung nennt die Zahl. `Fixed` in der Auswahl: `sketch-move-fixed`, Skizze bitgleich; Copy davon geht und ist frei. Strg, Raster, Fang beim Bezugspunkt wie Teilprojekt 2.
  - **Review-Focus 3:** Esc in `Picking`, `SetBase`, `Active`: `revision` gleich, kein Rückgängig-Schritt. Ein Schritt je Befehl, Name „Verschieben/Kopieren“; Wiederholen bitgleich. Strg+Z im Griffzug: Zug fällt weg.
  - **Review-Focus 4:** Punkt gebunden an projizierte Kante: Meldung; entlang: geht. **Messtest** `#[ignore]`: `transform_preview` bei 200 Kurven unter 16 ms im Release.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Griffe, Verschieben und Kopieren“.

### Task 10: Skalieren

**Files:**
- Modify: `crates/planora-app/src/sketch_state.rs`, `sketch_view.rs` (Griffe mit `Scale`), `sketch_input.rs` (`stage_fields`: `Tool::Scale` in `Active` `[ScaleFactor]`), `sketch_panel.rs` (`tool_options`: OK, Abbrechen; Hinweis „Enter nimmt die ganze Skizze“), `de.ftl`/`en.ftl` (`sketch-scale-hint`, `sketch-scale-base`)
- Test: `crates/planora-app/tests/sketch_scale.rs` (neu)

**Interfaces:**
- Consumes: Task 5, 9 (`TransformState`, Griffe).
- Produces:
  - `Tool::Scale`: Vorauswahl ⇒ `SetBase`; ohne ⇒ `Picking`, Enter ohne Auswahl ⇒ `whole = true`, `whole_sketch_set`, `SetBase`. Klick mit Fang setzt Bezugspunkt ⇒ `Active`. Faktor aus Feld, sonst Griff. Enter oder OK schließt ab.
  - `pub(crate) fn scaled_formula(text: &str, k: f64) -> String` (`({text}) * {k}`, `k` in kürzester Form mit Punkt als Trennzeichen).
  - `finish_transform` wendet bei `Tool::Scale` `apply_transform` an, dann für jede Id in `report.scaled` `scaled_formula`; `sf.prune_formulas()` für entfallene.

- [ ] **Step 1: Tests**
  - Rechteck mit `Distance`, Kreis mit `Radius`, gewinkelte Linie mit `Angle`: Faktor 2 um eine Ecke: Maße doppelt, Winkel gleich, `Solved`, `iterations == 0`, Ecke unbewegt. **Vorschau = Ergebnis** unter 1e-9. **Griff:** doppelter Abstand ⇒ Faktor 2, dichte Zeigerfolge stetig; Feld gewinnt.
  - **Formel (Review-Focus 5):** `Breite / 2` mit `Breite = 40` an `Distance`, Faktor 3: Text `(Breite / 2) * 3`, Wert 60; `Breite = 50` ⇒ 75 über `solve_formulas`; Winkel-Formel unverändert; zweimal skaliert verschachtelt und richtig. In Zoll: Faktor „2“ ist 2; Längenmaß bleibt richtig.
  - **Ganze Skizze:** Enter ohne Auswahl; Achshilfslinie, Kantenkopie und fester Ursprung bleiben; `Angle` zur Achshilfslinie bleibt erfüllt. Bezugspunkt im Ursprung bei einer Ecke am Ursprung: geht. Bezugspunkt woanders: `sketch-move-fixed`, Skizze bitgleich (Spec).
  - **Auswahl:** Teilmenge: Maß zu fremdem Punkt entfällt mit Zahl in der Meldung; Punkte und Radien der übrigen unberührt. `Fixed` in der Auswahl: Meldung.
  - **Review-Focus 1:** Faktor 0, −1, leer ⇒ `sketch-field-factor-positive` bzw. nichts; Faktor 1: `sketch-move-nothing`. Sehr kleiner Faktor, der eine Linie unter `tol` bringt: `solve_checked` lehnt ab, Stand bleibt.
  - **Review-Focus 3:** Esc in jeder Stufe, Rückgängig ein Schritt „Skizzenmaßstab“, Wiederholen bitgleich; Rückgängig stellt auch die Formeln her.
- [ ] Steps 2–4: FAIL, implementieren, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Skizzenmaßstab“.

### Task 11: Doku und Gesamtprüfung

**Files:** `docs/funktionen.md` (Abschnitt „Skizzen“: Rundung, Fase, Versatz, Verschieben/Kopieren, Maßstab), `CHANGELOG.md` (Absatz in Laiensprache), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 4“: alle **Annahme**-Stellen aus Entscheidung 15; Abweichungen: Rollkreis statt eigener Versatz-Verbindung und Maß als Radius, Hilfen sichtbar und schneiden beim Stutzen, Kreis, Ellipse, Spline bleiben beim Runden ganz, Ellipse und Spline ohne gehaltene Tangente und ohne folgende Kopie, Grenze `OFFSET_TIED_MAX`, Halten der Vorlage nur beim Versatzmaß und nicht über Formeln, freie Linien drehen beim Runden und Fasen mit, Rundung mit festen Fernenden und freier Ecke wird ab rund 87 % der Grenze abgelehnt, Trennen geteilter Punkte beim Verschieben, Formel `(alt) * k`, `NoJoin`; Zeile 1841: O gibt es, M im Skizzenbereich auch; Folgeideen: eigene Versatz-Art mit Format-Stufe, falls die Hilfen stören; Halten auch bei Rundung und Fase; Anzeige der Hilfen dünner)

- [ ] **Step 1:** `cargo fmt --check -p <crate>` für `planora-sketch`, `planora-app`, `planora-i18n`, `planora-project`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace --target-dir … -j 2`; Messtests `cargo test --release -p planora-sketch --test offset_perf -- --ignored` und die `#[ignore]`-Tests aus Task 8, 9; `cargo deny check`; zusätzlich voller Testlauf in WSL (Regel: vor dem Merge lokal Windows und WSL, kein GitHub-CI) – alles grün. Rundlauf: vorhandene Fixture laden und ohne Änderung speichern bleibt bytegleich, `FORMAT_VERSION` ist 14.
- [ ] **Step 2: Commit** „Doku: Skizzen 4 (Ändern)“.

Hinweis: Handtest per `tools/gui-drive.ps1` bei 1920×1080 läuft danach auf Dennis' Go, nicht in dieser Aufgabe.
