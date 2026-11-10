# Skizzen 7: Spline mit Kontrollpunkten, Griffe, Kegelschnitt, Glatt G2

Stand 2026-11-08. Siebtes von zehn Skizzen-Teilprojekten. Dennis hat den Kurzplan samt Claudes Empfehlungen am 2026-11-08 freigegeben („OK, so umsetzen“). Maßstab ist Fusions Verhalten. Was hier nur aus dem Gedächtnis stammt, ist als **Annahme** markiert und kommt in `offene-punkte.md`.

## Warum

In Fusion formt man freie Kurven auf drei Wegen: durch Stützpunkte mit ziehbaren Griffen, über Kontrollpunkte und als Kegelschnitt. Heute gibt es nur den natürlichen kubischen Spline ohne Griffe. Dadurch fehlt G2 am Bogen, Freiformkanten lassen sich nicht projizieren, und „Schneiden“ lehnt den schrägen Kegel ab. Die Teilprojekte 8, 9 und 10 brauchen diese Kurven (Schrift, DXF/SVG, 3D-Skizze).

## Bestand (geprüft im Code)

- `CurveKind::Spline { points }`: natürlicher kubischer Spline, Parameter `t ∈ [0, n − 1]` (`curves.rs`, `geom.rs`), 2 bis `MAX_SPLINE_POINTS` = 64 Punkte. Stutzen legt ihn durch die verbliebenen Stützpunkte neu (`edit.rs::spline_piece`, Entscheidung 6 aus Skizzen 1).
- `Geom` trägt Auswertung, Krümmung, Länge, nächsten Punkt, Linienzug und Hülle. Darauf bauen `intersect.rs`, `arrangement.rs`, `profile.rs`, `offset.rs`, `fillet.rs`, `edit.rs`, `snap.rs`.
- `constraint.rs`: `param_curve` rechnet Spline und Ellipse in `Dual`, `into_curve` liefert die Richtung am Ende, `Smooth` hat genau eine Zeile (Winkel). `spline_parameter` begrenzt die Zusatzvariable von „Punkt auf Spline“ und „Tangente Linie–Spline“.
- Dehnen kennt keine Splines (`locate_extend` liefert `None`). Spiegeln lehnt Splines und Ellipsen ab (`check_copy_sources`).
- `planora-math`: `NurbsCurve` mit Gewichten, `natural_cubic`, `elliptic_arc`, `sub_curve`, `insert_knot`, `bezier_segments`, `transformed`, `reversed`. Es fehlen Graderhöhung und Fortsetzen über das Ende hinaus.
- `planora-kernel`: `Curve::Nurbs`, `section_by_plane`. `plane_cone` kennt nur Kreis und zwei Geraden, sonst `Unsupported`.
- Projektion: `project_curve` lehnt `Curve::Nurbs` ab, `project_pieces` zählt das als `skipped_freeform`, `key_of` vergleicht keine Splines.
- App: Leiste mit genau 20 Werkzeugen (`STRIP_TOOLS`, 813 px). `Tool::Spline` hat noch keine Arten. `dragging`/`begin_drag` ziehen Punkte; `grip_drag` gehört zu Verschieben und Skalieren und wird hier nicht benutzt.

## Was Fusion tut

Aus der Autodesk-Hilfe und der API-Beschreibung:

- Spline mit Kontrollpunkten: Grad 5 (Vorgabe) oder 3 im Skizzenfeld, später änderbar. Erhöhen fügt Kontrollpunkte hinzu, Verringern nimmt welche weg und ändert die Form.
- Griffe am Spline durch Stützpunkte: je Stützpunkt ein Tangentengriff (eine Skizzenlinie) und ein Krümmungsgriff (ein Skizzenbogen). Beide werden erst aktiviert, Löschen deaktiviert sie. Sie lassen sich mit Abhängigkeiten und Maßen versehen.
- Kegelschnitt: Start, Ende, Scheitelpunkt (Schnitt der Endtangenten), Rho mit 0 < Rho < 1, per Griff oder getippt; elliptisch, parabolisch oder hyperbolisch.
- Glatt heißt in der englischen Hilfe heute „Curvature“. Es macht einen Spline mit einer angeschlossenen Kurve krümmungsstetig. Sprechende Quelle aus dem Forum: Grad 5 erlaubt das an beiden Enden.
- Projizierte Freiformkurven sind eigene feste Splines (`SketchFixedSpline`).

Aus dem Forum, nicht amtlich: Grad 5 mit nur 5 Punkten wird Grad 4. Rho ist kein Parameter und nimmt keine Formel.

## Ziele

### A. Neue Kurvenarten (`planora-sketch`)

1. **`CurveKind::ControlSpline { points: Vec<PointId>, degree: u8, knots: Vec<f64> }`**: nicht rationale B-Spline-Kurve. Die `points` sind echte Skizzenpunkte (2 bis 64); erster und letzter liegen auf der Kurve. Neu gezeichnet: gleichmäßiger geklemmter Knotenvektor, wirksamer Grad = kleiner aus gewähltem Grad und `n − 1` (**Annahme**, deckt den Foreneintrag). Die Knoten stehen in der Kurve, weil Stutzen und Einfügen sie ungleichmäßig machen.
2. **`CurveKind::Conic { start, end, apex: PointId, rho: f64 }`**: rationale quadratische Bézierkurve mit Gewicht `w = ρ / (1 − ρ)`, Parameter `t ∈ [0, 1]`. Der Schulterpunkt liegt bei `M + ρ·(A − M)`, `M` = Mitte der Sehne. Diese Rho-Definition ist **Annahme** (Fusion nennt sie nicht; es ist die übliche). Rho ist eine Zahl in der Kurve, keine Löservariable und ohne Formel; die Kurve hat also 6 Freiheitsgrade.
3. **`CurveKind::FixedSpline { start, end: PointId, curve: Arc<NurbsCurve> }`**: unveränderliche Kurve in Skizzenkoordinaten (`z = 0`), auch rational; beide Enden mit `Fixed`. Nur die Projektion legt sie an. Bei einer geschlossenen Kante ist `start == end`.
4. **`Spline` bekommt `handles: Vec<SplineHandle>`** (siehe B). Ohne Griffe rechnet der Spline bitgleich wie bisher, auch über denselben Codeweg.
5. `Geom::Nurbs { curve }` trägt die drei neuen Arten; `t` ist der NURBS-Parameter. `polyline_params`, `nearest`, `arc_length` und `bbox` richten sich nach Bézier-Stücken statt nach „je Segment“ (`nurbs_divisions`, `bezier_segments`). `eval` außerhalb des Bereichs setzt die Kurve über ihr letztes Polynomstück fort (de Boor mit geklemmtem Abschnitt), nicht auf der Tangente, damit Dehnen genau trifft. `Sampled` bekommt die passenden Arten für Parameter- und Tangentensuche.
6. `param_curve` rechnet alle Arten in `Dual` (de Boor in homogenen Koordinaten, Knoten und Gewichte konstant). `is_param_kind`, `curve_ends`, `into_curve`, `spline_parameter` (allgemein `(lo, hi)` statt `n − 1`) gelten mit. „Punkt auf Kurve“ und „Tangente Linie–Kurve“ gehen so an allen neuen Arten.

### B. Griffe am Spline durch Stützpunkte

`SplineHandle { at: PointId, line: CurveId, tip: PointId, curvature: HandleCurvature }` mit `HandleCurvature::{ Form, Straight, Arc { arc: CurveId, center: PointId, other: PointId } }`. `Form`: Krümmung ergibt sich aus der Kurve. `Straight`: Krümmung 0 (nur durch Glatt an einer Linie, siehe C3). `Arc`: Krümmungsgriff.

1. **Tangentengriff:** eine echte Konstruktionslinie `at → tip` (`curves` im Modell). Die Ableitung am Stützpunkt ist `C'(i) = 3·(tip − at)`. Die Linie nimmt alle Abhängigkeiten und Maße wie jede andere Linie.
2. **Krümmungsgriff:** ein echter Konstruktionsbogen mit einem Ende auf `at`, tangential an den Tangentengriff, seine Mitte auf der Normalen. `κ = ±1 / r`, Vorzeichen nach der Seite der Mitte. Die neue interne Abhängigkeit `SplineCurvatureHandle(spline, at)` hat zwei Zeilen: Mitte senkrecht zum Griff, und die Öffnung des Bogens ist `min(L / r, 2,5 rad)`, `L` = Grifflänge. So ist der Bogen bei großem Radius so lang wie der Griff und bei kleinem Radius immer noch lösbar. Es bleibt genau ein Freiheitsgrad, der Radius; ein Radiusmaß wirkt von selbst, und ein Zug am freien Bogenende ändert den Radius.
3. **Die Griffe gehören zur Punktgruppe des Splines:** `CurveKind::points()` liefert auch `tip`, `center` und `other`, damit der Löser sie in dieselbe Zusammenhangsgruppe nimmt.
4. **Form mit Griffen.** Die Stützpunkte mit Tangentengriff und die Enden teilen den Spline in Strecken.
   - Jede Strecke ist ein C²-kubischer Spline mit `t`-Abstand 1. An einem Griff ist die Steigung vorgegeben (`2·M₀ + M₁ = 6·(y₁ − y₀ − D₀)`, entsprechend am anderen Ende), an einem Ende ohne Griff ist sie natürlich (`M = 0`).
   - Daraus folgen für jeden Knoten Wert, erste und zweite Ableitung: ohne Griff die des kubischen Splines. Mit Tangentengriff (`Form`) ist die zweite der Mittelwert `M̄` beider Seiten. Sonst gilt `κ·|D|²·N + ((M̄·D)/|D|²)·D` (`N` = linke Normale): Der Anteil entlang der Tangente kommt aus `M̄`, der Griff bestimmt nur den Anteil quer dazu, bei `Straight` mit `κ = 0`. Dadurch ändert das Einschalten nichts an der Form. An einem Spline-Ende ist `M̄` der Wert der einen Seite.
   - Jedes Segment ist ein quintisches Hermite-Stück durch diese Knotenwerte. Damit ist die Kurve überall G2. Ohne vorgegebene zweite Ableitung ist das Stück derselbe Kubik.
   - Für Profile exakt als NURBS Grad 5, innere Knoten dreifach.
5. **Einschalten ändert die Form nicht:** Der Griff übernimmt die aktuelle Ableitung (`tip = at + C'/3`) und die aktuelle Krümmung. Abweichung bis auf Rechenrauschen ist ein Fehler. Einzige Ausnahme: Wo die Krümmung fast null ist, gibt es keinen endlichen Bogen; dann gilt `r = 1000·|tip − at|` auf der linken Seite (**Annahme**).
6. **Anzeige (Annahme):**
   - Ist der Spline, ein Griff oder sein Stützpunkt gewählt, sind alle aktiven Griffe zu sehen. Aktive Griffe mit einer Abhängigkeit oder einem Maß des Nutzers sind immer zu sehen. Unsichtbare Griffe fangen nicht und sind nicht wählbar.
   - An jedem Stützpunkt des gewählten Splines ohne aktiven Griff steht ein gedämpfter Griff. An aktiven Tangentengriffen ohne Krümmungsgriff steht eine gedämpfte Marke auf der Normalen. Der erste Zug daran schaltet ihn ein: ein Rückgängig-Schritt.
7. **Bedienung ohne Kontextmenü (Abweichung):** Rechtsklick bricht in der Skizze ab, darum zusätzlich Knöpfe „Tangentengriff“ und „Krümmungsgriff“ im Abschnitt „Bearbeiten“ des Skizzenfelds, wirksam am gewählten Stützpunkt, und belegbare Aktionen. Ist der Griff an, schalten sie ihn aus.
8. **Löschen:** Löschen der Griff-Linie schaltet den Griff samt Krümmungsgriff aus, Löschen des Griff-Bogens nur den Krümmungsgriff (wie die API). Löschen des Splines löscht seine Griffe. Der Spline springt danach auf die Form ohne diesen Griff.
9. **Seite des Krümmungsgriffs:** Ein Bogen kann nicht durch `r = ∞`. Wer die Seite wechseln will, schaltet den Griff aus und zieht neu (**Annahme**, Fusions Verhalten ist nicht belegt).

### C. Glatt G2

1. Neben der Winkelzeile bekommt `Smooth` eine **Krümmungszeile** `κ₁ + κ₂ = 0` (die Richtungen in die Kurven laufen entgegengesetzt). Sie gibt es, wenn beide Seiten eine „steuerbare oder bekannte“ Krümmung haben und mindestens eine davon nicht von selbst passt. Genau:
   - **Krümmung 0 von selbst:** Linie, Spline-Ende ohne Griff (natürlich), Griff `Straight`. Treffen sich nur solche Seiten, gibt es keine Zeile: Die Fuge ist schon G2.
   - **Mit Wert:** Bogen, Griff `Arc`, Kontrollpunkt-Spline, Kegelschnitt, fester Spline.
   - **Griff `Form`:** Die Krümmung folgt aus der Kurve und ist nicht steuerbar. Keine Zeile; das Glatt gilt dort als G1. So entsteht `Form` nur, wenn ein Tangentengriff an einem alten G1-Glatt eingeschaltet wird (Form bleibt gleich, B5) oder der Krümmungsbogen eines G2-Glatt gelöscht wird (es fällt auf G1 zurück, die Skizze bleibt lösbar; **Annahme**).
   - Ist eine Seite Spline-Ende ohne Griff oder Griff `Form` und die andere „mit Wert“: keine Zeile. Das ist der alte G1-Fall aus C4.
2. **Krümmung am Ende in Zahlen:** Linie 0; Bogen ±1/r; Griff `Arc` der Griffwert; alle übrigen aus ihrer Form (`Geom::curvature`, in `Dual`).
3. **Neues Glatt (Befehl):**
   - Eine Seite ist Bogen, Kontrollpunkt-Spline, Kegelschnitt, fester Spline oder Griff `Arc`: Ein Spline durch Stützpunkte auf der anderen Seite bekommt am Ende Tangenten- und Krümmungsgriff. Beide starten auf den Werten des Partners, die Griff-Lage wird vor dem Lösen gesetzt.
   - Die andere Seite ist eine Linie oder ein Spline-Ende mit Krümmung 0: Ein Spline durch Stützpunkte wird dort zu Krümmung 0. Ohne Griff bleibt er natürlich. Mit Griff wird der zu `Straight`, ein Krümmungsbogen entfällt.
   - Kontrollpunkt-Spline gegen Linie oder Spline-Ende mit Krümmung 0: Krümmungszeile, die Kontrollpunkte passen sich an.
   - **Gleicher Rang:** Treffen zwei Spline-Enden gleichen Rangs aufeinander (`Form`–`Form`, `Arc`–`Arc`), bekommen beide den Mittelwert aus Tangente und Krümmung, unabhängig von der Wahlreihenfolge; ist er fast 0, werden beide `Straight`. Natürliches Ende gegen `Form`: `Form` wird `Straight`.
   - **Vorhandenes Glatt an denselben Enden:** Der Befehl legt kein zweites an, sondern bereitet die Enden wie oben vor. So wird ein altes G1 zu G2 (ein Rückgängig-Schritt).
   - **Krümmungsgriff an einem Ende mit Glatt:** Einschalten wirkt wie das Vorbereiten oben (übernimmt die Werte des Partners, die Form darf sich dabei ändern, Ausnahme zu B5). Ist der Partner eine Linie oder hat er Krümmung 0, wird es mit Meldung abgelehnt. Lässt sich die Skizze danach nicht lösen, wird der ganze Befehl abgelehnt.
   - Alles in einem Rückgängig-Schritt, Meldung `sketch-smooth-arc-g1` entfällt.
4. **Alte Dateien (bewusste Abweichung):** Ein altes Glatt zwischen Spline ohne Griff und Bogen hat keine zweite Zeile (C1). Es bleibt G1, Form und Datei bleiben gleich. Wer G2 will, löscht es und setzt es neu.
5. **Partner:** Mindestens eine Seite ist Spline, Kontrollpunkt-Spline oder fester Spline; die andere Linie, Bogen, Spline, Kontrollpunkt-Spline, Kegelschnitt oder fester Spline; zwei feste Splines gehen nicht. Ellipsenbogen bleibt abgelehnt (**Annahme**, siehe Skizzen 5).
6. **Name:** en „Curvature“ (Hilfe). de bleibt „Glatt“ (**Annahme**, den Namen der deutschen Fusion kennen wir nicht). Interne Bezeichner bleiben `Smooth`.

### D. Werkzeuge und Oberfläche

1. **Klappmenü am Spline-Knopf** in der Leiste: „Spline durch Stützpunkte“, „Spline mit Kontrollpunkten“, „Kegelschnitt“ (en „Fit Point Spline“, „Control Point Spline“, „Conic Curve“). Die Leiste behält 20 Knöpfe und ihre Breite. Abweichung: In Fusion steht der Kegelschnitt nicht im Spline-Menü; im Menü „Erzeugen“ hat er wie dort einen eigenen Eintrag. Der Knopf zeigt die zuletzt gewählte Art wie die anderen Klappmenüs.
2. **Kontrollpunkt-Spline zeichnen:** Klicks setzen Kontrollpunkte, Abschluss wie beim vorhandenen Spline. Skizzenfeld: Grad 5 oder 3, Vorgabe 5. Das Kontrollpolygon ist gestrichelt zu sehen, solange die Kurve gewählt ist oder gezeichnet wird (**Annahme**).
3. **Ändern am gewählten Kontrollpunkt-Spline** im Skizzenfeld, je ein Rückgängig-Schritt:
   - **Grad ändern.** Erhöhen ist exakt: gleiche Form, mehr Kontrollpunkte. Verringern nähert kleinste Quadrate mit festen Enden; die Form ändert sich, wie die Hilfe sagt. Bei wirksamem Grad 4 (5 Punkte bei Wahl von Grad 5) zeigt das Feld „5“ nicht an, sondern den wirksamen Grad.
   - **Kontrollpunkt einfügen:** Knoten an der Klickstelle, Form bleibt. Vorhandene Punkte behalten ihre Ids und wandern, einer kommt hinzu.
   - Beide Befehle prüfen danach die Abhängigkeiten nach Regel 7 (`settle_constraints`): Was nicht mehr hält, entfällt mit Meldung.
4. **Kegelschnitt zeichnen:** Start, Ende, Scheitel. Danach steht das Rho-Feld am Zeiger mit Vorgabe 0,5 (**Annahme**). Enter oder Klick legt die Kurve an. Am gewählten Kegelschnitt: ein Griff auf dem Schulterpunkt, auf der Strecke `M–A` ziehbar, und ein Rho-Feld im Skizzenfeld. Werte außerhalb von 0 < ρ < 1 werden mit Meldung abgelehnt; der Griff bleibt innerhalb `[0,001; 0,999]`. Die Hilfslinien Start–Scheitel und Scheitel–Ende werden gezeichnet, solange der Kegelschnitt gewählt ist; sie sind keine Kurven.
5. **Tasten:** Neue belegbare Aktionen ohne Vorgabetaste (**Annahme**: Fusion hat keine): `skizze.spline_kontrollpunkte`, `skizze.kegelschnitt`, `skizze.tangentengriff`, `skizze.kruemmungsgriff`, `skizze.kontrollpunkt_einfuegen`, `skizze.spline_grad`.
6. **Symbole:** eigene `HandIcon`, in `HAND_DRAWN` eingetragen, ohne Autodesk-Material.
7. **Texte** nur über `tr!`/`msg!`, de und en. `docs/funktionen.md` nachführen.
8. Abbruch (Esc, Rechtsklick, Werkzeugwechsel) hinterlässt nichts. Ein Befehl ist ein Rückgängig-Schritt.

### E. Was an den neuen Kurven hängt

| Befehl | Kontrollpunkt-Spline | Kegelschnitt | Spline mit Griffen | Fester Spline |
|---|---|---|---|---|
| Stutzen, Brechen | exakt (`sub_curve`), neue Punkte und Knoten | bleibt Kegelschnitt, neues Rho aus homogener Teilung | wie heute durch die verbliebenen Stützpunkte; Griffe an verbliebenen Punkten bleiben, die anderen entfallen | abgelehnt, wie andere projizierte Kurven |
| Dehnen | neu in `locate_extend`: exakt, letztes Polynomstück fortgesetzt (Blüten der letzten `p` Kontrollpunkte); Suche bis zur dreifachen Bereichsbreite; **Annahme** | neu in `locate_extend`: exakt, solange die Gewichte positiv bleiben, sonst Meldung | abgelehnt wie heute (`locate_extend` kennt keine Splines) | abgelehnt |
| Versatz | Näherung durch exakte Versatzpunkte, wie heute beim Spline, Ergebnis ist ein Spline durch Stützpunkte | ebenso | ebenso | ebenso; das Ergebnis ist normale Geometrie |
| Abrunden, Fase | numerisch wie heute beim Spline, kürzt über denselben Weg wie Stutzen | ebenso | ebenso | Kurve bleibt, wie sie ist (wie `Fixed`-Enden heute) |
| Spiegeln, Muster, Verschieben, Kopieren, Skalieren | Punkte abgebildet, gleicher Grad und gleiche Knoten | Punkte abgebildet, gleiches Rho | Griffe werden mit abgebildet und mit kopiert. Spiegeln wird für alle Splines neu freigeschaltet (`check_copy_sources`), die Ellipse bleibt dort abgelehnt. Beim Spiegeln bekommen `at` und `tip` `Symmetric`, zwei Krümmungsbögen `Equal`; `center` und `other` nichts, sonst wären die Zeilen überzählig | abgelehnt mit eigener Meldung: Eine Kopie wäre eine unveränderliche Kurve ohne Bezug, und keine freie Art stellt sie exakt dar (Abweichung, kommt in `offene-punkte.md`). Skalieren und Verschieben gehen ohnehin nicht, sie ist fest |
| Fang | Kurve und Endpunkte; Kontrollpunkte als Punkte | Kurve, Enden, Scheitel als Punkt | wie heute; sichtbare Griffspitzen als Punkte | Kurve und Enden |
| Punkt auf Kurve, Tangente Linie–Kurve | ja | ja | ja | ja, die Linie passt sich an |
| Profile, Extrudieren | exakt | exakt | exakt, Grad 5 | exakt, auch rational |
| Maße | Kontrollpunkte wie Punkte | Punkte wie Punkte | Griffe wie Linien und Bögen | keine an der Kurve |

Konstruktion umschalten gilt an allen Arten außer Griffen; die bleiben Konstruktion. `profile_legacy.rs`, `profile_ref.rs`, `query.rs`, `dimension.rs` und `transform.rs` kennen die neuen Arten. Die vollständige Liste liefert der Compiler über die erschöpfenden `match`.

### F. Freiformkanten projizieren

1. `project_curve` nimmt `Curve::Nurbs`: `sub_curve(t0, t1)` (absteigend mit `reversed`), Kontrollpunkte in die Ebene abgebildet (`transformed`), Gewichte und Knoten bleiben. Das ist exakt.
2. Liegen alle projizierten Kontrollpunkte auf einer Geraden: Linie über die wirkliche Ausdehnung (abgetastet), wie hochkant stehende Kreise. Fallen sie auf einen Punkt: `projects_to_point` liefert wahr, das Stück entfällt.
3. `reproject` tauscht Kurve und `Fixed`-Werte, Id und Abhängigkeiten bleiben. Wird aus einer Freiformkante eine andere Art, gilt die Regel „verlorener Bezug ist eine Warnung“. `restore_projected_curve` kann `FixedSpline`. Ändert sich Knotenzahl oder Grad, ist das keine Artänderung.
4. `key_of` vergleicht feste Splines (Start, Ende, Kurvenmitte innerhalb der Toleranz), damit „Körper projizieren“ keine Doppelten anlegt.
5. Meldung: `skipped_freeform` zählt nur noch Fehlschläge; der Text wird entsprechend angepasst. Die Tests, die heute das Überspringen fordern, ändern sich.

### G. Schneiden: Ebene gegen schrägen Kegel

1. Neuer Rechenweg `plane_cone_conic` in `boolean/section.rs`. `section_by_plane` nimmt ihn, wenn `intersect_surfaces` für einen Kegel `Unsupported` liefert. **`plane_cone` und damit die Booleschen Operationen bleiben unverändert.** Das ist ein Folgepunkt.
2. Rechnung: Die Kegelgleichung in Ebenenkoordinaten eingesetzt ergibt einen allgemeinen Kegelschnitt `A u² + B uv + C v² + D u + E v + F = 0`, eingeordnet über die Diskriminante, normiert auf die Kegelsteigung (Parabel: innerhalb `1e-9`).
   - Ellipse → `Curve::Ellipse`.
   - Parabel → `Curve::Nurbs` (Bézier Grad 2), Bereich so, dass der Hüllquader der Fläche sicher überdeckt ist.
   - Hyperbel → je Ast `Curve::Nurbs` (rational Grad 2), ebenso.
   - Ebene durch die Spitze → zwei Geraden, eine Berührgerade → `Touch`, nur die Spitze → nichts.
   - **Abweichung beim Bauen (Parabel):** Das Fenster `1e-9` allein reicht nicht. Als Parabel gilt die Lage nur, solange die weggelassene Größe `A·u²/(2·B)` im Hüllquader seitlich höchstens `5e-8` ausmacht; sonst gilt die genaue Ellipse oder Hyperbel. Grund: Im Fenster allein lief ein Stück bei großen Kegeln bis 22 mm über die Fläche hinaus.
   - **Abweichung beim Bauen (Ellipse):** Eine große Halbachse über `1e7` oder ein Verhältnis der Halbachsen über `1e4` kommt, wenn das Stück den Scheitel nicht umrundet, als rationales Bézier-Stück Grad 2 statt als `Curve::Ellipse`, gerechnet vom nahen Scheitel. Es ist dieselbe Ellipse. Gründe: Die Form aus Mitte und Halbachse erreicht über `1e7` durch Rundung die Grenze `1e-7` (gemessen 5,33e-7 bei r = 1000), und der Skizzenlöser meldet für einen Ellipsenbogen ab etwa `1e5` `NotConverged` (gemessen: 3,3e4 gelöst, 1,1e5 nicht, bei r = 5 bis 1000). Der Grund für die Verhältnisgrenze `1e4` ist vermutlich ein Rundungsboden in `ellipse_residual` (`constraint.rs`): Das Residuum bleibt bei etwa `1e-16·große Halbachse·|sin t|·Verhältnis/2`, gegen `tol_r` = `1e-8`. Das ist eine Folgerung aus dem Code, nicht nachgemessen, und keine harte Grenze: Liefert `near_vertex_ellipse` `None`, bleiben auch schlanke Nadelkegel `Curve::Ellipse` und können `NotConverged` sein.
3. Beschneiden wie gehabt (`breakpoints`, `split_at`, `classify_seg`). Der falsche Ast fällt dort weg.
4. Ergebnis in der Skizze: Ellipse oder Ellipsenbogen wie bisher; Parabel und Hyperbel als **fester Spline** (rational, exakt). Das ist dieselbe Kurve wie ein fester Kegelschnitt, ohne weitere Art. Sie folgen dem Körper nach F3.
   - **Abweichung beim Bauen:** Auch die sehr große oder sehr schlanke Ellipse aus G2 wird ein fester Spline. Der Wechsel zwischen beiden Formen bei einer Maßänderung ist eine Artänderung nach F3, also eine Warnung. Parabel, Hyperbel und Ellipsenbogen laufen am Kegel als ein Stück über die Naht; andere Flächen teilt die Naht weiter (siehe `offene-punkte.md`).
5. `kernel-section-unsupported` bleibt für Torus allgemein; der Kegel fällt heraus. Silhouette bleibt, wie sie ist.

### H. Dateiformat

**Format 17** nur, wenn eine Skizze (auch gelöschte Einträge, wie `curve_entries`) `ControlSpline`, `Conic`, `FixedSpline`, einen Griff oder `SplineCurvatureHandle` enthält. `handles` fehlt in der Datei, wenn leer. Alle anderen Dateien bleiben bytegleich, auch im Format, das sie vorher hatten. `from_data` prüft das Neue so streng wie das Vorhandene: Zahlen endlich, `0 < ρ < 1`, Punkte und Griff-Kurven vorhanden, Knoten gültig (`NurbsCurve::new`), Grad passend, Griff-`at` ein Stützpunkt des Splines. Eine Skizze mit einem kaputten Eintrag wird mit Meldung abgelehnt, nicht still repariert.

## Nicht-Ziele

Laut Auftrag: Text und Schriften (8), DXF/SVG/Canvas (9), 3D-Skizze, auf Fläche projizieren, Schnittkurve (10), Anzeige in der Skizze (Slice, Blick auf Skizze, Ein/Aus, Ebenen mit Verdeckung), Schneiden mit Torus allgemein und Freiformflächen, die übrigen Folgepunkte und Härtungen der Roadmap (nur notieren), Release, CI, Push.

Dazu: geschlossene Splines (Fusion kann das, heute nicht vorhanden), Überblendkurve (Blend Curve), Krümmungskamm, „Spline in Kontrollpunkt-Spline umwandeln“, Punkt in Spline durch Stützpunkte einfügen, Kegelschnitt als Boolescher Rechenweg. Was davon auffällt, kommt in `offene-punkte.md`.

## Anti-Ziele

- Ein Spline ohne Griffe ändert seine Form oder eine alte Datei ihre Bytes.
- Griff einschalten verbiegt die Kurve.
- Griffe stören die Profilsuche oder tauchen als Profilkanten auf.
- Eine gewählte, freie Kurve springt beim Lösen.
- Ein abgelehnter Befehl hinterlässt Griffe, Punkte oder einen Rückgängig-Schritt.
- Die Leiste wird breiter oder rollt bei 1920×1080.
- Stutzen eines Kontrollpunkt-Splines ändert die Form des übrigen Stücks.
- Körper projizieren legt Freiformkanten doppelt an.
- Boolesche Ergebnisse ändern sich durch den neuen Kegelweg.

## Wie (Entscheidungen)

- Rechnen in `planora-math`, `planora-sketch`, `planora-kernel`, ohne egui. Neue Bausteine in `planora-math`: Dehnen über das Ende (`extended_end`), exakte Graderhöhung (Interpolation auf den Knotenvektor mit passender Vielfachheit, das Ergebnis liegt exakt im Raum), kleinste Quadrate für das Verringern, soweit `fit.rs` das nicht schon kann.
- Kein neuer Löser-Sonderweg: Griffe und Kontrollpunkte sind Punkte in `vars`.
- Keine neuen Fremd-Abhängigkeiten.
- Testgetrieben; Kurven gegen dichte Folgen (Rho, Winkel, Griffwerte in feinen Schritten), G2 numerisch nachgemessen (Sprung der Krümmung über die Fuge), Formtreue über dichte Abtastung, Bytegleichheit über vorhandene alte Dateien. Tests bei 1920×1080.

**Reihenfolge:** (1) Mathe-Bausteine und `Geom::Nurbs`, (2) Kontrollpunkt-Spline samt Format und allem aus E, (3) Kegelschnitt, (4) Griffe: Tangente, dann Krümmung, (5) Glatt G2, (6) Freiform projizieren und schräger Kegel, (7) Oberfläche, Texte, Doku, (8) Prüfung Windows und WSL, dann Frage nach dem Handtest (dabei die zwei offenen Punkte aus Skizzen 6).

## Dem Bauenden überlassen

Dateiaufteilung (etwa ein neues `curves_nurbs.rs`), Namen der DTO-Felder in der Stilart von `format.rs`, genaue Toleranzen der Griff-Anzeige und Griffgröße in Pixeln, Abtastdichte für Hüllquader und Grenzen beim Kegelweg. Wo beim Bauen eine Fusion-Frage auftaucht: Annahme nach bestem Wissen treffen und in `offene-punkte.md` notieren, nicht anhalten.
