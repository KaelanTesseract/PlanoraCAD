# Skizzen 1: Profile wie Fusion, Stutzen, Dehnen, Brechen – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Jede von nicht-konstruktiven Kurven umschlossene Fläche ist ein wählbares Profil (wie Fusion), Features merken sich Profile über ihre Begrenzung, und die Skizze bekommt Stutzen (T), Dehnen und Brechen samt neuer Kurvenart Ellipsenbogen.

**Architecture:** In `planora-sketch` entsteht eine Schicht „Kurvengeometrie + Schnittpunkte“ (`geom.rs`, `intersect.rs`), darauf eine ebene Anordnung als Halbkanten-Struktur (`arrangement.rs`), aus der `Sketch::profiles()` Flächen, Profil-Verweise und Umrisse liefert; der alte Algorithmus bleibt als `legacy_profiles()` für den Übergang alter Dateien. Stutzen/Dehnen/Brechen sind Operationen in `planora-sketch/src/edit.rs`; Features speichern `ProfileSel` statt einer Nummer; die App zeigt Profile, wählt sie per Klick und bietet die drei Werkzeuge.

**Tech Stack:** Rust-Workspace, egui, Fluent (`de.ftl`/`en.ftl`, `tr!`), serde, eigene NURBS in `planora-math`.

**Spec:** `docs/superpowers/specs/2026-10-31-skizzen-profile-design.md`

## Global Constraints

- Verhalten, Namen und Kürzel genau wie Autodesk Fusion; Abweichungen nur, wenn technisch nötig, dann in `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt Skizzen 1“) mit Grund. Annahmen über Fusion-Verhalten sind unten mit **Annahme** markiert.
- Keine neuen Fremd-Abhängigkeiten; falls doch unvermeidbar: Lizenz muss `cargo deny check` bestehen. Kein Autodesk-Material (Symbole selbst gezeichnet als `HandIcon`).
- Alle Oberflächentexte über `tr!` mit Schlüssel in `crates/planora-i18n/languages/de.ftl` **und** `en.ftl` (Wächter `planora-i18n/tests/no_german_literals.rs`). Namen: „Stutzen/Trim“, „Dehnen/Extend“, „Brechen/Break“, „Profil/Profile“, „Ändern/Modify“, „Ellipsenbogen/Elliptical arc“.
- Intern alles in mm; Anzeigen über `planora_i18n::fmt_len` & Co. (Zoll muss gehen). Farben nur über Theme-Tokens (Wächter `no_hard_coded_colors.rs`).
- Alte Dateien laden und ergeben dieselben Körper (Volumen, Hülle, Flächen- und Kantenzahl gleich). Alter Algorithmus bleibt als `Sketch::legacy_profiles()` (doc-hidden, nur für Übergang und Tests).
- Dateiformat: `FORMAT_VERSION` 12 → 13; 13 nur, wenn eine Skizze einen Ellipsenbogen hat oder ein Feature Profil-Verweise braucht, die sich nicht als alte Nummer schreiben lassen (Muster `required_format`). „Zu neu“-Tests wandern von 13 auf 14.
- Leistung (Release): `profiles()` für 200 Kurven < 50 ms, `trim_piece` (Vorschau beim Überfahren) < 16 ms – je ein `#[ignore]`-Messtest, laufen mit `cargo test --release -p planora-sketch -- --ignored`.
- Toleranzen: Knoten zusammenlegen bei `tol.linear`; Schnittpunkt gilt bei Restabstand ≤ `tol.linear`; Richtungsgleichstand an Knoten bei Winkeldifferenz < 1e-9 rad.
- GUI-Tests und Handtest bei 1920×1080, nie 800×600 (Handtest per `tools/gui-drive.ps1`).
- Prüfen: `cargo test -p <crate> --target-dir C:/Users/denni/AppData/Local/Temp/claude/D--Testumgebung/9257fb9d-a130-4b88-86e3-d09eb7b412f8/scratchpad/target-e2 -j 2`; am Ende `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check`. `cargo fmt --all` vor jedem Commit. Nichts pushen.
- Commit-Nachrichten deutsch, enden mit Leerzeile und `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Alte Tests, die Ablehnungen von Kreuzungen/Verzweigungen/Berührungen prüfen, werden **bewusst** auf das neue Fusion-Verhalten umgeschrieben (Liste in Task 4 und Task 11); alle anderen bleiben unverändert grün.

## Entscheidungen des Planers

1. **Kurvenparameter** (`Geom`, Task 3): Linie `s∈[0,1]` von `a` nach `b`; Kreis Winkel `θ∈[0,2π)` ab +x (periodisch); Bogen Winkel ab Start `∈[0,sweep]`; Ellipse `t∈[0,2π)` wie `ellipse_point` (periodisch); Ellipsenbogen `t` ab Startparameter `∈[0,sweep]`; Spline `t∈[0,n−1]`. Profil-Verweise nutzen den auf `[0,1]` normierten Parameter `u`.
2. **Anordnung:** Halbkanten (je Kurvenstück zwei), Knoten per Gitter-Hash zusammengelegt, doppelte Stücke (gleiche Knoten, Mittelpunkt näher als `10·tol.linear`) zusammengefasst (kleinere Kurven-Id bleibt) – deckt überlappende kollineare Linien und doppelte Kreise ab. Stummel durch wiederholtes Entfernen von Grad-1-Knoten. Ordnung am Knoten nach Abgangswinkel, bei Gleichstand nach Krümmung (stärker links gekrümmt = weiter gegen den Uhrzeigersinn). Zyklen mit positiver Fläche = Flächen, negative = Außenrand einer Komponente → Loch der kleinsten umschließenden Fläche einer anderen Komponente, sonst verworfen. Flächen mit |Fläche| ≤ tol² entfallen still.
3. **Eingeklemmte Flächen** (Loch berührt den Rand in einem Punkt, innen tangentiale Kreise, sehr flache Ellipse): Fläche erscheint, wird aber mit `SelfIntersecting` abgelehnt, weil der Kernel sich selbst berührende Schleifen nicht kann. Fusion kann das → Abweichung in offene-punkte.
4. **Profil-Verweis-Abgleich:** `J = M / (L_R + L_F − M)` über die Stücke des **äußeren** Rands (Längen in mm; `M` = Länge der Überdeckung gleiche Kurve, gleiche Seite). Gefunden, wenn bestes `J ≥ 0,5` und Abstand zum zweitbesten `≥ 1e-6`; bei Gleichstand entscheidet dasselbe Maß über alle Stücke inkl. Löcher; sonst verloren. Zwei Verweise auf dieselbe Fläche = verloren.
5. **Übergang alter Dateien:** Altes Profil `n` (alter Algorithmus) → alle neuen Flächen, deren Innenpunkt in dessen äußerer Schleife und außerhalb seiner Löcher liegt; nebeneinanderliegende werden beim Berechnen zu einem Körper zusammengefasst (gleiches Ergebnis wie früher, auch wenn heute eine offene Linie quer durch liegt). Umrechnung beim Laden; beim Speichern wird ein Verweis, der genau `migrate(n)` entspricht, wieder als `profil: n` geschrieben (Datei bleibt auf ihrem Format).
6. **Splines stutzen:** neu durch die verbleibenden Stützpunkte plus neuen Endpunkt (Spec-Vorgabe). Parameterbereich geprüft und verworfen: Enden eines Bereichs wären keine `PointId`s; Deckungsgleich, Profile, Solver (`spline_eval` mit variablem Bereich), Datenformat und alle `points.first()/last()`-Stellen müssten umgebaut werden. Hat das übrige Stück keinen inneren Stützpunkt, kommt der Kurvenpunkt in der Bereichsmitte als Stützpunkt dazu.
7. **Abhängigkeiten beim Stutzen/Dehnen/Brechen:** Erhalten bleibt jede Abhängigkeit an den betroffenen Kurven und Punkten, die nach der Änderung ohne Bewegung erfüllt ist (alle Residuen von `Sketch::evaluate` ≤ `10·tol.linear` bzw. `10·tol.angular` für Winkel); die übrigen entfallen – so bewegt sich nie etwas. `Midpoint` an einer gekürzten Linie entfällt immer. Neue Teilstücke: Linie bekommt `Horizontal`/`Vertical` vom Original, sonst `Parallel(orig, neu)` + `PointOnCurve(neu.a, orig)` (kollinear; **Annahme** Fusion setzt „kollinear“, das es hier noch nicht gibt); Bogen teilt den Mittelpunkt und bekommt `Equal(orig, neu)`; Ellipsenbogen teilt Mittelpunkt und die Variablen `ax, ay, minor`; Spline nichts. Neue Enden: `Coincident` mit einem vorhandenen Punkt der schneidenden Kurve, wenn der Schnitt dort liegt (≤ tol), sonst `PointOnCurve(neu, schneidende Kurve)`; bei mehreren schneidenden Kurven die mit kleinster Id; Selbstschnitt eines Splines ohne Abhängigkeit. Punkte, die nur das gelöschte Stück nutzte, werden mit `remove_point` entfernt.
8. **Dehnen** bewegt den vorhandenen Endpunkt (behält seine Id), „offenes Ende“ = Endpunkt, den keine andere Kurve nutzt und der kein `Coincident` hat. **Annahme:** ein dadurch verletztes Längenmaß entfällt (Regel 7) statt das Dehnen zu verhindern.
9. **Brechen** erzeugt getrennte Punkte an der Trennstelle mit `Coincident` zwischen den Teilen (wie Fusion) plus Regel 7 für die schneidende Kurve.
10. **Dialog:** Fusion-Vorgabe – genau ein Profil in der Skizze → vorgewählt, sonst keins (OK gesperrt mit Hinweis). Klick in die 3D-Ansicht schaltet ein Profil um; die Liste zeigt Häkchen; beides schreibt dieselbe `Vec<usize>`.

## Review Focus

1. Schnitt genau in einem Endpunkt (Rechteckecke, T-Stoß an einem Linienende): kein doppelter Knoten, kein Nullstück; Stutzen dort setzt `Coincident`, nicht `PointOnCurve` (Tests Task 3 und Task 8).
2. Überlappende kollineare Linien und zwei gleiche Kreise übereinander: ein Profil, keine Panik, keine Nullflächen (Task 4).
3. Ziehen-Stutzen über mehrere Kurven ist **ein** Rückgängig-Schritt; Esc mitten im Ziehen lässt Gestutztes stehen und beendet das Werkzeug (Task 10).
4. Alte Datei, deren gültiges Profil heute von einer offenen Linie gekreuzt wird: nach dem Laden derselbe Körper (Task 5 und Task 7).
5. Stutzen an einer Kurve mit Maß (Radius am Kreis, Länge an Linie): keine übrig bleibende Geometrie bewegt sich, Solver danach `Solved` oder unverändert (Task 8).

---

### Task 1: Teilkurve einer NURBS

**Files:**
- Modify: `crates/planora-math/src/nurbs/bezier.rs` (neben `insert_knot` :411)
- Test: `crates/planora-math/tests/nurbs_curve.rs`

**Interfaces:**
- Produces: `impl NurbsCurve { pub fn sub_curve(&self, u0: f64, u1: f64) -> Result<NurbsCurve, NurbsError> }` – `domain().0 ≤ u0 < u1 ≤ domain().1`, sonst Fehler; Knoten auf Vielfachheit = Grad einfügen, Kontrollpunkte und Knoten herausschneiden; Domäne der Teilkurve ist `(u0, u1)` (gleiche Parametrisierung).

- [ ] **Step 1: Tests** – `sub_curve_matches_the_original_pointwise`: natürlicher kubischer Spline durch 5 Punkte, Teil `(0.7, 3.2)`: an 50 Parametern `|sub.point(u) − c.point(u)| < 1e-12`, `sub.domain() == (0.7, 3.2)`; rationaler Fall mit `elliptic_arc(…, 0, TAU)` Teil `(1.0, 2.5)` ebenso; `(u0 ≥ u1)` und `u0 < domain().0` → `Err`; ganzer Bereich → gleiche Punkte.
- [ ] **Step 2:** `cargo test -p planora-math --test nurbs_curve …` → FAIL (fehlt).
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `cargo test -p planora-math` grün.
- [ ] **Step 5: Commit** „NURBS: Teilkurve zwischen zwei Parametern“.

### Task 2: Kurvenart Ellipsenbogen (Skizze, Solver, Datei)

**Files:**
- Modify: `crates/planora-sketch/src/model.rs` (`CurveKind::EllipticArc`, `points()`, `add_elliptic_arc`), `constraint.rs` (internes `EllipticArcEnds`, `ellipse_of`, `param_curve`, `is_param_kind`, `check_types` für `EllipseMajor/Minor/Angle`, `PointOnCurve`, `Tangent` mit Linie, `measure`), `solver.rs` (:430 Entartung, :844 Variablen, :892), `query.rs` (`curve_polyline`, `nearest_on_curve`), `data.rs` (:141 Prüfung), `helpers.rs` (:84, :143 Spiegeln/Muster/Kopieren), `profile.rs` (alter Algorithmus: Ellipsenbogen ist eine Graph-Kante wie ein Bogen, Segment `Nurbs` aus `NurbsCurve::elliptic_arc`)
- Modify: `crates/planora-project/src/format.rs` (`CurveKindDto::Ellipsenbogen { mitte, ax, ay, klein, start, ende }`, Abhängigkeits-DTO `EllipsenbogenEnden { kurve }`), `convert.rs` (Hin/Rück, `FORMAT_VERSION = 13`, `required_format`: 13 als erste Regel, wenn irgendein Kurveneintrag einer Skizze – auch gelöschte – ein Ellipsenbogen ist; Doku-Kommentar ergänzen)
- Modify: exhaustive `match`es in `crates/planora-app/src/{sketch_state.rs, sketch_view.rs, sketch_panel.rs, create_dialog.rs, undo_labels.rs}` (Zeichnen wie Ellipse über `curve_polyline`, Rückgängig-Name `undo-elliptic-arc`), `de.ftl`/`en.ftl`
- Modify: „zu neu“-Tests von 13 auf 14: `crates/planora-project/tests/{dimensions_format, docstore, drawings_format, format2, format3, parameter_format, parts_format, rollbacks_format, section_format, sheet_format, step_format}.rs`
- Test: `crates/planora-sketch/tests/elliptic_arc.rs` (neu), `crates/planora-project/tests/elliptic_arc_format.rs` (neu)

**Interfaces:**
- Produces:
  - `CurveKind::EllipticArc { center: PointId, ax: usize, ay: usize, minor: usize, start: PointId, end: PointId }` – gegen den Uhrzeigersinn von `start` nach `end` auf der Ellipse `center + a·cos t + b̂·minor·sin t`.
  - `pub fn add_elliptic_arc(&mut self, center: PointId, major: DVec2, minor: f64, start: PointId, end: PointId) -> Result<CurveId, SketchError>` (Prüfungen wie `add_ellipse`; legt intern `ConstraintKind::EllipticArcEnds(id)` an; `add_constraint` lehnt es ab wie `ArcRadius`).
  - `pub(crate) fn add_elliptic_arc_vars(&mut self, center, ax, ay, minor, start, end) -> CurveId` (geteilte Variablen, für Task 8/9).
  - `pub fn ellipse_param_of(&self, c: CurveId, p: DVec2) -> Option<f64>` (Parameter `t` des Punktes in Achskoordinaten via `atan2`, für Ellipse und Ellipsenbogen).
  - Residuum `EllipticArcEnds`: zwei Zeilen, `start` und `end` auf der Ellipse (gleiche implizite Form wie `PointOnCurve` an der Ellipse).

- [ ] **Step 1: Tests** – Ellipsenbogen `(0,0)`, `a=(10,0)`, `minor=5`, Start `(10,0)`, Ende `(0,5)`: `curve_polyline` endet exakt auf Start/Ende und alle Punkte erfüllen `x²/100 + y²/25 = 1 ± 1e-9`; nach Ziehen von `end` nach `(3,9)` und `solve()` liegt `end` wieder auf der Ellipse; `EllipseMajor(c, 12)` wirkt; `Tangent(line, arc)` und `PointOnCurve(p, arc)` lösen; `add_constraint(EllipticArcEnds(c))` → `WrongType`; Rundlauf `to_data`/`from_data`; Spiegeln kopiert ihn; alter Algorithmus: Ellipsenbogen + Linie zwischen seinen Enden ist ein Profil mit Fläche π·10·5/4 + 25/2 (±0,1). Projekt: Datei mit Ellipsenbogen schreibt `"format": 13` und lädt gleich; ohne bleibt die Datei auf ihrem Format; `"format": 14` → `TooNew { found: 14 }`.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implementieren; „zu neu“-Tests auf 14 umstellen.
- [ ] **Step 4:** `cargo test -p planora-sketch`, `-p planora-project`, `-p planora-app` grün.
- [ ] **Step 5: Commit** „Skizze: Kurvenart Ellipsenbogen, Format 13“.

### Task 3: Kurvengeometrie und Schnittpunkte

**Files:**
- Create: `crates/planora-sketch/src/geom.rs`, `crates/planora-sketch/src/intersect.rs`; `lib.rs` (Module `pub(crate)`)
- Test: `crates/planora-sketch/tests/intersections.rs` (neu)

**Interfaces:**
- Consumes: Task 2 `EllipticArc`; `curves::Sampled`.
- Produces:
  - `pub(crate) enum Geom { Line { a, b }, Circle { c, r }, Arc { c, r, a0, sweep }, Ellipse { c, a, b }, EllipticArc { c, a, b, t0, sweep }, Spline { s: Sampled, n: usize } }` mit `fn eval(&self, t: f64) -> (DVec2, DVec2)`, `fn domain(&self) -> (f64, f64)`, `fn periodic(&self) -> bool`, `fn curvature(&self, t: f64) -> f64`, `fn length(&self) -> f64`, `fn nearest(&self, p: DVec2) -> f64`, `fn polyline(&self, t0: f64, t1: f64, steps_per_turn: usize) -> Vec<DVec2>`.
  - `impl Sketch { pub(crate) fn geom(&self, c: CurveId) -> Option<Geom>; pub fn curve_eval(&self, c: CurveId, t: f64) -> Option<DVec2>; pub fn curve_domain(&self, c: CurveId) -> Option<(f64, f64)>; pub fn split_params(&self, c: CurveId, with_construction: bool) -> Vec<f64> }` – `split_params`: sortierte, entdoppelte (≤ tol in Länge) Parameter aller Schnitte von `c` mit allen lebenden Kurven (ohne Konstruktion, wenn `false`) und mit sich selbst; Enden offener Kurven nicht enthalten.
  - `pub(crate) fn intersect(g1: &Geom, g2: &Geom, tol: f64) -> Vec<(f64, f64)>` (Parameterpaare; analytisch für Linie/Kreis/Bogen untereinander, sonst Polylinien-Schnitte + Gauß-Newton auf `C1(t1) − C2(t2)`, höchstens 20 Schritte, angenommen bei Rest ≤ tol; zusätzlich lokale Abstandsminima der Polylinien unter `2·tol` nachgeschärft = Berührpunkte; Hüllrechteck-Vortest; kollineare Überlappung und gleiche Kreise liefern die Enden des Überlappungsbereichs).
  - `pub(crate) fn self_intersect(g: &Geom, tol: f64) -> Vec<(f64, f64)>` (nur Spline; nicht benachbarte Polylinienstücke).

- [ ] **Step 1: Tests** über `split_params`/`curve_eval`: zwei kreuzende Linien → je ein Parameter 0,5; Linie tangential an Kreis (Abstand = r) → genau ein Parameter; Kreis/Kreis mit zwei Schnitten; Bogen schneidet nur innerhalb seines Winkels; Linie/Ellipse (Ellipse 10×5, Linie `x = 6`) → 2 Punkte mit `|Rest| ≤ 1e-6`; Spline-Achterschleife hat einen Selbstschnitt; T-Stoß am Linienende: an der durchgehenden Linie ein Parameter, an der stoßenden keiner (Ende); kollineare Linien `(0,0)-(10,0)` und `(5,0)-(15,0)` → Parameter 0,5 bzw. 0,5; Konstruktionslinie zählt nur mit `with_construction = true`; nie NaN, nie Panik bei Kurven mit Länge ~tol.
- [ ] **Step 2:** FAIL. **Step 3:** Implementieren. **Step 4:** grün.
- [ ] **Step 5: Commit** „Skizze: Kurvengeometrie und Schnittpunkte“.

### Task 4: Profile als ebene Anordnung

**Files:**
- Create: `crates/planora-sketch/src/arrangement.rs`, `crates/planora-sketch/src/profile_legacy.rs` (heutiger Inhalt von `profile.rs` unverändert, Einstieg umbenannt in `#[doc(hidden)] pub fn legacy_profiles(&self) -> ExtractedProfiles`)
- Modify: `crates/planora-sketch/src/profile.rs` (neues `profiles()`), `lib.rs`
- Test: `crates/planora-sketch/tests/profile.rs`, `nurbs_profiles.rs`, `crates/planora-sketch/tests/arrangement.rs` (neu), `crates/planora-sketch/tests/profile_perf.rs` (neu, `#[ignore]`)

**Interfaces:**
- Consumes: Task 1 `sub_curve`, Task 3 `Geom`, `split_params`.
- Produces:
  - `ExtractedProfiles { pub profiles: Vec<Profile>, pub refs: Vec<ProfileRef>, pub outlines: Vec<Vec<Vec<DVec2>>>, pub rejected: Vec<RejectedLoop>, pub curve_loops: Vec<Vec<CurveId>>, pub(crate) arrangement: Option<Arc<Arrangement>> }` – `outlines[i][0]` äußerer Umriss, danach Löcher (Abtastung 64 je Vollkreis); `curve_loops` = Kurven des äußeren Rands in Umlaufreihenfolge ohne direkte Wiederholung.
  - `impl ExtractedProfiles { pub fn profile_at(&self, p: DVec2) -> Option<usize> }` (kleinste Fläche, die `p` enthält).
  - `pub struct ProfileRef { pub outer: Vec<PieceRef>, pub holes: Vec<PieceRef> }`, `pub struct PieceRef { pub curve: CurveId, pub u0: f64, pub u1: f64, pub left: bool, pub len: f64 }` (`Clone, Debug, PartialEq`; `u0 < u1`, bei periodischen Kurven `u1` bis `u0 + 1`; `left` = Fläche links der Kurvenrichtung; `len` in mm). Re-Export in `lib.rs`.
  - Reihenfolge: nach (kleinste Kurven-Id des äußeren Rands, Schwerpunkt x, Schwerpunkt y).
  - Schleifen: Zyklus beginnt beim Stück mit kleinstem (Kurven-Index, `u0`). Unzerteilter Kreis → `ProfileLoop::circle`, unzerteilte Ellipse → heutiges `ellipse_loop` (gleiche Körper wie früher); Linie → `Segment::Line`, Kreis/Bogen-Stück → `Segment::Arc { sweep: ±Δθ }`, Ellipsen-Stück → `NurbsCurve::elliptic_arc` (umgekehrt bei Rückwärtslauf), Spline-Stück → `natural_cubic(…).sub_curve(t0, t1)`.
  - Abgelehnt (mit heutigen Texten): Bogen ohne Winkel, Bogen-Ende neben dem Kreis, Spline mit zusammengefallenen Nachbarpunkten, zu kleine Ellipse (Kurve fällt aus der Anordnung); Fläche, deren `SampledLoop::touches_itself` gilt (`SelfIntersecting`, Entscheidung 3). Offene Ketten, Stummel, T-Stöße, Kreuzungen werden **nicht** gemeldet.

- [ ] **Step 1: Neue Tests** (`arrangement.rs`): Rechteck mit Diagonale → 2 Profile je Fläche 50 (Rechteck 10×10); zwei kreuzende Rechtecke `(0,0)-(10,10)` und `(5,5)-(15,15)` → 3 Profile (Flächen 75, 25, 75); zwei überlappende Kreise r=5 im Abstand 6 → 3 Profile; Kreis im Rechteck → 2 Profile (Ring mit 1 Loch, Kreisfläche); Linie quer durch Kreis → 2 Halbkreise; Stummel an einer Ecke → 1 Profil, `rejected` leer; Konstruktionslinie quer durch → 1 Profil; zwei Dreiecke, die sich nur in einer Ecke berühren → 2 Profile; Spline-Achterschleife → 2 Profile; überlappende kollineare Rechteckkanten (zwei Rechtecke nebeneinander mit doppelter gemeinsamer Kante) → 2 Profile; zwei gleiche Kreise übereinander → 1 Profil; `profile_at` trifft Ring bzw. Kreisfläche; jedes Profil extrudiert und validiert (`AnalyticKernel`); `refs[i]` hat für das Rechteck mit Diagonale je 3 Stücke außen, `left` korrekt.
- [ ] **Step 2: Alte Tests bewusst umschreiben** (Kommentar „Fusion-Verhalten, Skizzen 1“): `profile.rs` `open_chains_and_t_junctions_are_rejected_with_a_reason` → `open_chains_are_ignored_and_t_junctions_split_faces`; `bow_ties_and_crossing_loops_are_rejected` → `bow_ties_and_crossing_loops_give_faces` (Schleife 2, kreuzende Quadrate 3); `a_line_back_and_forth_encloses_no_area` → kein Profil, `rejected` leer; `a_hole_tangent_to_an_edge_is_rejected_at_every_scale` → Kreisfläche ist Profil, eingeklemmte Fläche `SelfIntersecting`, in jedem Maßstab; `internally_tangent_circles_are_rejected_and_a_small_gap_is_fine` → innere Kreisfläche Profil, Sichel abgelehnt, kleiner Spalt weiter Ring. `nurbs_profiles.rs` `a_spline_crossing_the_profile_is_rejected_as_self_intersecting` → `a_spline_crossing_the_profile_splits_it` (Flächen addieren sich zur Gesamtfläche ±1e-3). Alle übrigen Tests in `planora-sketch/tests` unverändert.
- [ ] **Step 3: Messtest** `profile_perf.rs` `#[ignore] two_hundred_curves_under_50_ms`: 10×10-Gitter aus 20 Linien, 100 Kreise r=0,4 an den Gitterpunkten, 80 Bögen; Median aus 5 Läufen `< 50 ms` im Release.
- [ ] **Step 4:** FAIL, dann implementieren, dann `cargo test -p planora-sketch` grün und `cargo test --release -p planora-sketch --test profile_perf -- --ignored` grün.
- [ ] **Step 5: Commit** „Skizze: Profile als ebene Anordnung wie Fusion“.

### Task 5: Profil-Verweise wiederfinden, zusammenfassen, alte Nummern umrechnen

**Files:**
- Create: `crates/planora-sketch/src/profile_ref.rs`
- Test: `crates/planora-sketch/tests/profile_ref.rs` (neu)

**Interfaces:**
- Consumes: Task 4.
- Produces:
  - `impl ExtractedProfiles { pub fn find(&self, r: &ProfileRef) -> Option<usize>; pub fn resolve(&self, refs: &[ProfileRef]) -> Result<Vec<usize>, usize>; pub fn merged(&self, sel: &[usize]) -> Vec<Profile> }` – `find` nach Entscheidung 4 (`MATCH_MIN = 0.5`, `TIE_EPS = 1e-6`); `resolve` liefert bei Verlust den Index des verlorenen Verweises; `merged`: Zusammenhangsgruppen über gemeinsame Halbkanten, Rand = Halbkanten mit gewählter Fläche und nicht gewähltem Zwilling, Löcher der kleinsten umschließenden äußeren Schleife; eine einzelne Fläche gibt genau `profiles[i]` zurück.
  - `impl Sketch { pub fn migrate_profile_index(&self, n: usize) -> Option<Vec<ProfileRef>>; pub fn legacy_index_of(&self, refs: &[ProfileRef]) -> Option<usize> }` (Entscheidung 5; Innenpunkt einer Fläche: waagerechte Gerade durch die Höhe des Schwerpunkts ihres äußeren Umrisses, Schnitte mit allen Umrissen der Fläche (gerade/ungerade), Mitte des breitesten Innenintervalls).

- [ ] **Step 1: Tests** – Rechteck 20×10 verschoben/vergrößert (Punkte ziehen) → `find` gleich; Fase an einer Ecke (Linie dazu, Ecke gestutzt) → gefunden; Rechteck durch Mittellinie geteilt → beide Hälften `J = 0,4` → `None`; Kreis im Rechteck dazu → Ring gefunden (äußerer Rand gleich); Kurve gelöscht → `None`; `resolve` zweimal derselbe Verweis → `Err(1)`; `merged` zweier Hälften = ein Profil mit Fläche 200, zweier getrennter Rechtecke = 2 Profile, Ring + Kreisfläche = volles Rechteck ohne Loch; Übergang: für jedes Szenario aus `tests/profile.rs` und jedes alte `n`: `extrude(legacy_profiles().profiles[n])` und `extrude(merged(resolve(migrate(n))))` haben gleiches Volumen (rel. 1e-9), gleiche Hülle (1e-9), gleiche Flächen- und Kantenzahl; Review-Focus 4: Rechteck plus offene Linie quer durch → `migrate(0)` = 2 Flächen, Körper gleich dem alten; `legacy_index_of(migrate(n)) == Some(n)`.
- [ ] Steps 2–4 (FAIL, implementieren, `cargo test -p planora-sketch` grün).
- [ ] **Step 5: Commit** „Skizze: Profil-Verweise, Zusammenfassen, Übergang alter Nummern“.

### Task 6: Features mit Profil-Verweisen und mehreren Körpern

**Files:**
- Modify: `crates/planora-feature/src/model.rs` (:241-250), `compute.rs` (:242-261, `nth_profile` :450 → `selected_profiles`), `lib.rs` (Re-Export `ProfileSel`), `de.ftl`/`en.ftl` (`feature-profile-lost` = „Ein Profil der Skizze ist nicht mehr vorhanden. Wähle es über Bearbeiten neu.“ / „A profile of the sketch no longer exists. Select it again via Edit.“)
- Modify (mechanisch `profile: n` → `profiles: ProfileSel::Index(n)`): alle Baustellen aus `rg "profile: " crates/*/tests crates/*/src` in `planora-feature`, `planora-project`, `planora-app`, `planora-step` (Liste: `chain.rs`, `compute.rs`, `nurbs_features.rs`, `formulas_ops.rs`, `feature_speaks_english.rs`, `blend_features.rs`, `formulas_sketch.rs`, `georef.rs`, `recompute.rs`, `ref_cache.rs`, `rollback.rs`, `step_bodies.rs`, `planora-project/tests/{format, format_voll, nurbs_roundtrip, step_format}.rs`, `planora-step/tests/read_seams.rs`, `planora-app/src/{crash, create_dialog, state, feature_dialog}.rs`, `planora-app/tests/{balloon_tool, dimension_perf, dimension_tool, history_perf, modelling_english, sketch_dialogs}.rs`)
- Test: `crates/planora-feature/tests/profiles.rs` (neu)

**Interfaces:**
- Consumes: Task 5.
- Produces:
  - `pub enum ProfileSel { Index(usize), Refs(Vec<ProfileRef>) }` (`Clone, Debug, PartialEq`); `Index(n)` = altes Profil `n` (nur alte Dateien und Tests), beim Berechnen über `migrate_profile_index`.
  - `FeatureKind::Extrude { sketch, profiles: ProfileSel, height }`, `FeatureKind::Revolve { sketch, profiles: ProfileSel, axis }`.
  - Berechnen: `merged(resolve(..))` → je Gruppe ein Körper, `Computed::bodies(alle)` (Ursprung je Körper `id.0`, Reihenfolge der Gruppen nach kleinster Profilnummer); Drehen: jede Gruppe muss `revolve_input` bestehen, sonst Fehler. Verlust → `FeatureError::InvalidInput(tr!("feature-profile-lost"))`; `Index(n)` ohne Profil → heutige Meldung `feature-no-profile(-rejected)`.

- [ ] **Step 1: Tests** – zwei getrennte Rechtecke, beide gewählt → 2 Körper in einem Feature; Rechteck mit Diagonale, beide Hälften → 1 Körper Volumen = ganzes Rechteck; Skizze geändert, dass das Profil geteilt wird → Feature-Status Fehler mit `feature-profile-lost` (deutsch und englisch); `ProfileSel::Index(0)` eines Rechtecks mit Kreisloch = alter Körper; Drehen zweier Profile, eins quer über der Achse → Fehler; Profil aus Ellipsenbogen + Linie extrudiert, validiert und lässt sich als STEP schreiben (`planora-step`-Export ohne Fehler).
- [ ] Steps 2–4: FAIL, implementieren + mechanische Umstellung, `cargo test -p planora-feature -p planora-step -p planora-app` grün (App-Tests nur kompiliert/umgestellt, Verhalten unverändert).
- [ ] **Step 5: Commit** „Features: Profil-Verweise statt Nummer, mehrere Körper“.

### Task 7: Dateiformat 13 für Profil-Verweise und Übergang beim Laden

**Files:**
- Modify: `crates/planora-project/src/format.rs` (:838-847: `Extrusion`/`Rotation` mit `#[serde(default, skip_serializing_if = "Option::is_none")] profil: Option<usize>` und `profile: Option<Vec<ProfilVerweisDto>>`; `ProfilVerweisDto { aussen: Vec<StueckDto>, loecher: Vec<StueckDto> }`, `StueckDto = (usize kurve, f64 u0, f64 u1, bool links, f64 laenge)` als Tupel-Array), `convert.rs` (:597-612, :783-795, `required_format`, `decode_version` :2113)
- Test: `crates/planora-project/tests/profile_format.rs` (neu), Fixtures in `crates/planora-project/tests/fixtures`

**Interfaces:**
- Consumes: Task 5 `migrate_profile_index`, `legacy_index_of`; Task 6 `ProfileSel`.
- Produces: Lesen: `profil: n` → `Index(n)`, danach Durchgang über den Snapshot: `Index(n)` → `Refs(migrate)` mit der gespeicherten Skizze (kein Profil → bleibt `Index(n)`); genau eins von `profil`/`profile` muss da sein, sonst benannter Fehler (`proj-err-profile-field`). Schreiben: `Index(n)` → `profil: n`; `Refs(r)` mit `legacy_index_of == Some(n)` → `profil: n`; sonst `profile: […]`. `required_format`: 13 auch, wenn ein Feature `profile` schreiben muss. Prüfung beim Lesen: Kurven-Index < Kurvenzahl der Skizze, `u0 < u1 ≤ u0 + 1`, Zahlen endlich, höchstens `MAX_LIST` Stücke.

- [ ] **Step 1: Tests** – alle vorhandenen Fixture-Dateien mit Extrusion/Rotation laden, berechnen, Körper (Volumen, Hülle, Flächenzahl) gleich dem Ergebnis mit `legacy_profiles()`; unverändert speichern → bytegleich und gleiches Format; neues Feature mit zwei Profilen → Format 13, Rundlauf gleich; kaputte Verweise (Kurve 999, `u0 > u1`, NaN, beide Felder, keins) → benannte Fehler ohne Panik.
- [ ] Steps 2–4, `cargo test -p planora-project` grün.
- [ ] **Step 5: Commit** „Projektformat 13: Profil-Verweise, Übergang alter Profilnummern“.

### Task 8: Stutzen in der Skizze

**Files:**
- Create: `crates/planora-sketch/src/edit.rs` (+ `pub(crate) fn set_curve_kind(&mut self, c, kind)` in `model.rs`)
- Test: `crates/planora-sketch/tests/trim.rs` (neu), `crates/planora-sketch/tests/trim_perf.rs` (neu, `#[ignore]`)

**Interfaces:**
- Consumes: Task 2, Task 3 (`split_params(c, true)`, `Geom::nearest`).
- Produces:
  - `pub struct TrimPiece { pub curve: CurveId, pub t0: f64, pub t1: f64, pub whole: bool, pub polyline: Vec<DVec2> }`
  - `impl Sketch { pub fn trim_piece(&self, c: CurveId, at: DVec2) -> Option<TrimPiece>; pub fn trim(&mut self, c: CurveId, at: DVec2) -> Result<Vec<CurveId>, SketchError> }` – Stück = zwischen den Schnittparametern links und rechts vom nächsten Parameter zu `at` (periodisch umlaufend); kein Schnitt → ganze Kurve (`remove_curve` + Punkte nach Entscheidung 7). Rückgabe: übrige Kurven (erste = `c`, falls sie bleibt). Ergebnisse je Art: Linie kürzer/zwei Linien; Kreis → Bogen (gleiche Id, `Radius`/`Diameter` bleiben); Bogen kürzer/zwei Bögen; Ellipse → Ellipsenbogen (gleiche Id, Halbachsen-Maße bleiben); Ellipsenbogen kürzer/zwei; Spline nach Entscheidung 6. Abhängigkeiten nach Entscheidung 7.
  - Gemeinsam für Task 9: `pub(crate) fn settle_constraints(&mut self, touched: &[CurveId], points: &[PointId])` (Regel 7: unerfüllte Abhängigkeiten entfernen, Zusatzvariablen von `PointOnCurve`/`Tangent` an neu gelegten Splines per `nearest_to_point` neu setzen) und `pub(crate) fn end_constraint(&mut self, p: PointId, cutter: CurveId, at: DVec2)` (`Coincident` oder `PointOnCurve`).

- [ ] **Step 1: Tests** – Kreuz aus zwei Linien, Stutzen eines Arms → Linie endet im Kreuzpunkt mit `PointOnCurve` zur anderen, alle anderen Punkte bitgleich; T-Stoß am Linienende → `Coincident` (Review-Focus 1); Mittelstück einer waagerechten Linie zwischen zwei senkrechten → zwei Linien, beide `Horizontal`, kollinear (zweite bleibt nach Ziehen auf der Geraden); Kreis mit zwei Schnittlinien → Bogen, `Radius(c, 5)` bleibt, `solve()` bewegt nichts (Review-Focus 5); Linie mit Längenmaß gestutzt → Maß entfällt, nichts bewegt sich; Ellipse → Ellipsenbogen mit `EllipseMajor`; Spline durch 5 Punkte gestutzt → Endpunkt auf der Schnittkurve, ≥ 3 Stützpunkte; Kurve ohne Schnitt → weg, freie Punkte weg; Schnitt mit Konstruktionslinie zählt; `trim_piece` liefert dieselben Grenzen wie `trim` löscht; danach `profiles()` ohne Panik.
- [ ] **Step 2: Messtest** `#[ignore] trim_hover_under_16_ms` auf der Skizze aus Task 4: `trim_piece` für 50 Zeigerstellen, längste < 16 ms (Release).
- [ ] Steps 3–4: FAIL, implementieren, grün (auch `--release -- --ignored`).
- [ ] **Step 5: Commit** „Skizze: Stutzen“.

### Task 9: Dehnen und Brechen in der Skizze

**Files:**
- Modify: `crates/planora-sketch/src/edit.rs`
- Test: `crates/planora-sketch/tests/extend_break.rs` (neu)

**Interfaces:**
- Consumes: Task 3 `intersect`, Task 8 (Abhängigkeitsregeln als `pub(crate) fn settle_constraints(&mut self, touched: &[CurveId], points: &[PointId])`, `pub(crate) fn end_constraint(&mut self, p: PointId, cutter: CurveId, at: DVec2)`).
- Produces:
  - `impl Sketch { pub fn extend_preview(&self, c: CurveId, at: DVec2) -> Option<Vec<DVec2>>; pub fn extend(&mut self, c: CurveId, at: DVec2) -> Result<bool, SketchError>; pub fn break_curve(&mut self, c: CurveId, at: DVec2) -> Result<Vec<CurveId>, SketchError> }`
  - Dehnen: nur Linie, Bogen, Ellipsenbogen; Ende = näheres Ende zu `at`, muss offen sein (Entscheidung 8), sonst `Ok(false)`; Linie: Strahl ab Ende (als Hilfslinie der Länge 2 × Diagonale von `extent()`), Bogen/Ellipsenbogen: volle Kurve, nächster Schnitt jenseits des Endes (Bogen höchstens bis kurz vor den eigenen Start); geschnitten wird nur mit **anderen** lebenden Kurven (auch Konstruktion). Kein Schnitt → `Ok(false)`, Skizze unverändert.
  - Brechen: teilt an den beiden Schnittparametern um `at` (Entscheidung 9); Kreis/Ellipse mit mindestens 2 Schnitten → 2 Bögen bzw. Ellipsenbögen; ohne Schnitt → `Ok(vec![c])` unverändert.

- [ ] **Step 1: Tests** – Linie `(0,0)-(5,0)` und senkrechte `(10,-5)-(10,5)`: Dehnen am rechten Ende → Ende `(10,0)`, `PointOnCurve`; Dehnen am linken Ende ohne Ziel → `false`, Skizze gleich (Stand unverändert); Bogen bis an Linie; Ende an Rechteckecke (geteilt) → `false`; Vorschau = Strecke vom alten zum neuen Ende; Brechen einer Linie mit zwei Kreuzungen → 3 Linien, `Coincident` an beiden Stellen, Gesamtgeometrie gleich; Kreis mit zwei Schnitten → 2 Bögen mit gemeinsamer Mitte; danach Profile wie vorher (gleiche Flächen).
- [ ] Steps 2–4, grün.
- [ ] **Step 5: Commit** „Skizze: Dehnen und Brechen“.

### Task 10: Werkzeuge Stutzen, Dehnen, Brechen in der App

**Files:**
- Modify: `crates/planora-app/src/sketch_state.rs` (`Tool::{Trim, Extend, Break}`, `click` → `trim/extend/break_curve` mit `sk.pick_curve(at, radius)`, Hinweis `sketch-extend-no-hit` in `message`, `SketchSession.hover_preview: Option<Vec<DVec2>>`, `SketchSession.edit_label: Option<Msg>` für den Schrittnamen), `sketch_view.rs` (`handle_input`: Vorschau je Bild; bei `Tool::Trim` und gehaltener linker Taste jede überfahrene Kurve stutzen; `draw`: Vorschau rot über Theme-Token `trim_highlight`, Dehnen gestrichelt), `sketch_panel.rs` (`TOOL_ORDER: [Tool; 12]`, Trennlinie vor `Trim`, Hinweis „Ändern → …“), `shell.rs` (`STRIP_TOOLS = 12`, `STRIP_H` + Trennlinie `STRIP_SEPARATOR_H = 9.0`), `icons.rs` (`HandIcon::{Trim, Extend, Break}` selbst gezeichnet), `keymap/defaults.rs` (`SketchTrim, "skizze.stutzen", "action-sketch-trim", Sketch, [key(Key::T)]`; `SketchExtend, "skizze.dehnen", …, []`; `SketchBreak, "skizze.brechen", …, []`; `sketch_tool()`; Kommentar :99 nachführen), `state.rs` (`sketch_label` nutzt und leert `edit_label`), `theme.rs` (Token `trim_highlight`, `profile_fill`, `profile_fill_selected`, `profile_fill_hover` für hell und dunkel), `de.ftl`/`en.ftl` (`action-sketch-trim/extend/break`, `sketch-tool-*-hint`, `sketch-strip-modify`, `sketch-extend-no-hit`, `undo-trim/extend/break`)
- Test: `crates/planora-app/tests/sketch_trim.rs` (neu), `keymap_sketch_drawing.rs`, `shell_layout.rs` (Leiste passt bei 1920×1080)

**Interfaces:**
- Consumes: Task 8, Task 9.
- Produces: `Tool::{Trim, Extend, Break}`, `Action::{SketchTrim, SketchExtend, SketchBreak}`.

- [ ] **Step 1: Tests** – T wählt Stutzen; Dehnen/Brechen ohne Vorgabetaste, in den Einstellungen belegbar und wirksam; Klick auf Kreuzarm stutzt, Rückgängig stellt her, Schrittname „Stutzen“; Ziehen mit gehaltener Taste über drei Kurven stutzt alle drei, **ein** Schritt (Review-Focus 3); Esc während des Ziehens beendet das Werkzeug, Gestutztes bleibt; Dehnen ohne Ziel zeigt den Hinweis und ändert nichts; Vorschau ist gesetzt, wenn der Zeiger über einem Stück steht; Leiste mit 12 Knöpfen passt in 1080 px Höhe; englische Oberfläche zeigt „Trim“, „Extend“, „Break“.
- [ ] Steps 2–4, `cargo test -p planora-app` grün.
- [ ] **Step 5: Commit** „App: Stutzen (T), Dehnen, Brechen“.

### Task 11: Profile anzeigen und per Klick wählen

**Files:**
- Create: `crates/planora-sketch/src/triangulate.rs` (`pub fn triangulate(outline: &[Vec<DVec2>]) -> Vec<[DVec2; 3]>`, Ohrenschneiden mit Brücken zu Löchern) und `impl ExtractedProfiles { pub fn fill(&self, i: usize) -> &[[DVec2; 3]] }` (träge, `OnceLock` je Profil); `crates/planora-app/src/profile_pick.rs` (gemeinsame Liste für beide Dialoge)
- Modify: `crates/planora-app/src/state.rs` (`pub show_sketch_profiles: bool` = true; `pub profile_pick: Option<ProfilePick>`; `extrude_profiles`/`revolve_profiles`; `check_profile` → Verweise; `extrude_sketch(sketch, profile: usize, h)` und `revolve_sketch` bleiben als Kurzform für `&[profile]`), `sketch_view.rs` (`draw`: Flächen unter den Kurven, wenn `show_sketch_profiles`; `finished_sketch_shapes`: Flächen der Skizze aus `profile_pick`, gewählte kräftiger, überfahrene heller), `sketch_panel.rs` (`side_contents`: Häkchen `sketch-show-profile` „Profil anzeigen“ oben), `viewport.rs` (bei `profile_pick`: Linksklick ohne Ziehen → Strahl ∩ Skizzenebene → `profile_at` → `clicked`; Körperauswahl dann nicht), `create_dialog.rs` (`profile: usize` → `profiles: Vec<usize>`, Vorgabe nach Entscheidung 10, OK gesperrt bei leerer Wahl mit `create-pick-profile`), `feature_dialog.rs` (`profile_choice` :880 → `profile_pick::list`; Anfangswahl aus `resolve` der gespeicherten Verweise, Verlust → leer plus Meldung `feature-profile-lost`), `de.ftl`/`en.ftl`
- Test: `crates/planora-sketch/tests/triangulate.rs` (neu), `crates/planora-app/tests/profile_pick.rs` (neu), `crates/planora-app/tests/sketch_state.rs`, `sketch_dialogs.rs`

**Interfaces:**
- Consumes: Task 4–6.
- Produces: `pub struct ProfilePick { pub sketch: FeatureId, pub selected: Vec<usize>, pub hover: Option<usize>, pub clicked: Option<usize> }`; `AppState::extrude_profiles(&mut self, sketch: FeatureId, sel: &[usize], height: f64, formulas: Formulas) -> Result<FeatureId, String>`; `AppState::revolve_profiles(&mut self, sketch: FeatureId, sel: &[usize], axis: CurveId) -> Result<FeatureId, String>`; Dialog-Ergebnis schreibt `ProfileSel::Refs(sel.map(|i| ex.refs[i].clone()))`.

- [ ] **Step 1: Tests** – Triangulierung: Summe der Dreiecksflächen = Profilfläche (±1e-6 rel.) für Rechteck, Ring, Halbkreis, Achterschleifen-Hälfte; App: Klick in die Mitte eines von zwei Rechtecken (Kamera senkrecht und schräg, 1920×1080) schaltet dessen Profil in der Liste um, zweiter Klick wieder aus; Liste und Klick ergeben dasselbe Feature; zwei Profile gewählt → 2 Körper; Skizze mit genau einem Profil → vorgewählt, mit mehreren → keins und OK gesperrt; Bearbeiten eines Features mit verlorenem Profil zeigt die Meldung und erlaubt Neuwahl; „Profil anzeigen“ aus → keine Flächen gezeichnet.
- [ ] **Step 2: Alten App-Test bewusst umschreiben:** `sketch_state.rs` `a_hole_tangent_to_an_edge_explains_why_it_is_not_usable` → `a_hole_tangent_to_an_edge_leaves_the_circle_usable`: ein Profil (Kreisfläche), `rejected[0]` nennt weiter „berührt“. Tests in `sketch_dialogs.rs`, die `profile: 0` bei mehreren Profilen voraussetzen, wählen das Profil ausdrücklich.
- [ ] Steps 3–4: FAIL, implementieren, `cargo test -p planora-sketch -p planora-app` grün.
- [ ] **Step 5: Commit** „App: Profile anzeigen und per Klick wählen“.

### Task 12: Doku, Gesamtprüfung, Handtest

**Files:** `docs/funktionen.md` (Abschnitt „Skizzen“: Profile wie Fusion, Klickwahl, mehrere Körper, Stutzen/Dehnen/Brechen, Ellipsenbogen; Satz „erklärt, warum andere Kurvenzüge nicht verwendbar sind“ :52 anpassen), `CHANGELOG.md` (0.0.2-dev.2 „Neu“, ein Absatz in Laiensprache wie der Zoll-Eintrag), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 1“: eingeklemmte Flächen, Spline-Form beim Stutzen, „kollinear“ nachgebildet, Dehnen entfernt verletzte Maße (Annahme), Dialog-Vorwahl; im Abschnitt Tastenkürzel „Stutzen T“ aus „Fusion-Kürzel ohne Funktion“ streichen)

- [ ] **Step 1:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace … -j 2`, beide Messtests im Release, `cargo deny check` – alles grün.
- [ ] **Step 2: Handtest** per `tools/gui-drive.ps1` bei 1920×1080: Rechteck mit Diagonale und Kreis zeichnen, Flächen sichtbar, T-Stutzen per Klick und Ziehen, Dehnen, Brechen, Rückgängig; Extrudieren mit Klick auf zwei Profile; alte Projektdatei öffnen, Körper gleich. Bildschirmfotos ansehen.
- [ ] **Step 3: Commit** „Doku: Skizzen 1 (Profile wie Fusion, Stutzen, Dehnen, Brechen)“.
