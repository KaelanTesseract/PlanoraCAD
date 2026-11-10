# Skizzen 7: Spline mit Kontrollpunkten, Griffe, Kegelschnitt, Glatt G2 – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kontrollpunkt-Spline, ziehbare Tangenten- und Krümmungsgriffe, Kegelschnitt mit Rho, Glatt als G2 auch an Bögen, projizierte Freiformkanten und „Schneiden“ am schrägen Kegel, Verhalten wie in Fusion.

**Architecture:** Drei neue `CurveKind`-Arten (`ControlSpline`, `Conic`, `FixedSpline`) laufen über ein neues `Geom::Nurbs`, damit Schnitt, Profil, Stutzen und Fang denselben Weg nehmen. Griffe sind echte Konstruktionskurven und Punkte in `vars`; der Spline liest sie, ohne Griffe rechnet er bitgleich wie bisher. Glatt bekommt eine zweite Zeile. Der Kegelweg liegt nur in `section.rs`, die Booleschen Operationen bleiben unberührt.

**Tech Stack:** Rust-Workspace, egui 0.36, Fluent (`de.ftl`/`en.ftl`, `tr!`), serde. Keine neuen Abhängigkeiten.

**Spec:** `docs/superpowers/specs/2026-11-08-skizzen-splines-design.md`. Sie ist Pflichtlektüre für jede Aufgabe; Abschnittsnummern (A1, B4, C3 …) verweisen auf sie.

## Global Constraints

- Verhalten, Namen und Kürzel wie Autodesk Fusion; Abweichungen und **Annahmen** in `docs/superpowers/offene-punkte.md`, neuer Abschnitt „Teilprojekt Skizzen 7“. Nicht anhalten, sondern notieren.
- **graphify zuerst:** vor dem Lesen von Dateien `graphify query "<Frage>" --budget 2000` oder `graphify explain "<Symbol>"` in `D:\Testumgebung\PlanoraCAD`, dann die Dateien lesen, auf die er zeigt. `graphify-out/` nie einchecken.
- **Ohne Griffe bitgleich:** `CurveKind::Spline` mit leerem `handles` nimmt denselben Codeweg wie heute. Bestehende Spline-Tests bleiben unverändert grün.
- **Format 17 nur, wenn nötig** (H): `FORMAT_VERSION` = 17; `format_with` liefert 17 genau dann, wenn eine Skizze, auch mit gelöschten Einträgen, `ControlSpline`, `Conic`, `FixedSpline`, einen Griff oder `SplineCurvatureHandle` enthält. Neue Felder `#[serde(default, skip_serializing_if = …)]`.
- Ein Befehl ist ein Rückgängig-Schritt (`atomic`); Esc, Rechtsklick, Werkzeugwechsel hinterlassen nichts. Abgelehnte Befehle ändern nichts.
- Oberflächentexte nur über `tr!`/`msg!`, Schlüssel in `crates/planora-i18n/languages/de.ftl` **und** `en.ftl`. Farben nur über Theme-Tokens. Kommentare deutsch.
- Tests bei 1920×1080, nie 800×600. Kein Handtest, keine Berührung von `C:\Users\denni\Documents\PlanoraCAD`.
- Prüfen: `cargo test -p <crate> --target-dir C:/Users/denni/AppData/Local/Temp/claude/0497e14a6c2b1da9/bf4c8043-4b56-4be0-9981-6d409f66ada9/scratchpad/target-s7 -j 2`; je Aufgabe `cargo clippy -p <crate> --all-targets -- -D warnings` und `cargo fmt -p <crate> -- --check` (**nicht** `--all`). LNK1285: die genannte Datei im Zielordner löschen.
- Bekannte Fehlschläge: `real_step_text_limit`, unter WSL `a_locked_old_json_never_ends_up_next_to_a_new_png`. Alles andere ist neu und muss geklärt werden.
- Commit-Nachrichten deutsch, enden mit Leerzeile und `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Nichts pushen, kein CI.
- Alte Tests, die neues Verhalten treffen, werden bewusst umgeschrieben (Kommentar „Skizzen 7“).

## Review Focus

1. **Griff an einem Spline aus zwei Punkten, und an einem Stützpunkt, an dem die Ableitung fast null ist** ⇒ keine NaN, kein Sprung, der Griff geht oder wird mit Meldung abgelehnt. Test in Task 6.
2. **Entartete Kontrollpolygone:** alle Kontrollpunkte auf einer Geraden, zwei gleiche Punkte, ein Punkt auf einen anderen gezogen ⇒ Löser und Zeichnen bleiben stabil, Profil meldet höchstens einen Grund. Test in Task 2.
3. **Geschlossene Freiformkante** (`start == end`) projiziert ⇒ eine Kurve, bildet ein Profil, extrudierbar, folgt dem Körper. Test in Task 10.
4. **Alte Datei mit Glatt zwischen Spline und Bogen** laden, lösen, speichern ⇒ Form und Bytes gleich, Format bleibt. Test in Task 9.
5. **Stutzen dicht an einem Knoten oder an einem Ende des Kegelschnitts**, Rho nahe 0 und nahe 1 ⇒ kein zu kleiner Knotenabstand, keine negative Gewichte, Meldung statt Absturz. Test in Tasks 3 und 5.

---

### Task 1: Mathe-Bausteine

**Files:**
- Modify: `crates/planora-math/src/nurbs/bezier.rs`, `crates/planora-math/src/nurbs/curve.rs`
- Create: `crates/planora-math/src/nurbs/degree.rs`
- Test: `crates/planora-math/tests/nurbs_extend_degree.rs`

**Interfaces:**
- Produces:
  - `NurbsCurve::point_unclamped(&self, u: f64) -> DVec3` und `derivs_unclamped(&self, u: f64, n: usize) -> Vec<DVec3>`: außerhalb des Bereichs das erste bzw. letzte Polynomstück.
  - `NurbsCurve::extended(&self, new_lo: f64, new_hi: f64) -> Result<NurbsCurve, NurbsError>`: dieselbe Kurve mit neuen End-Knoten, innere Knoten bleiben. Nur nicht rational, sonst `NurbsError::Input`.
  - `NurbsCurve::elevated(&self, to: usize) -> Result<NurbsCurve, NurbsError>`: exakt; innere Knoten mit Vielfachheit `m + (to − p)`, Kontrollpunkte durch Interpolation auf diesen Knotenvektor.
  - `NurbsCurve::reduced(&self, to: usize, control_count: usize) -> Result<NurbsCurve, NurbsError>`: kleinste Quadrate auf gleichmäßigen Knoten, Enden fest.
  - `pub fn uniform_clamped_knots(degree: usize, n: usize) -> Vec<f64>`: `[0; p+1], 1, …, n−p−1, [n−p; p+1]`.

- [ ] **Step 1: Failing tests** `extended_keeps_the_old_part` (Abweichung auf dichten Stellen < 1e-9, neues Ende = `point_unclamped`), `extended_rejects_rational`, `elevated_is_the_same_curve` (3→5, 5 Kontrollpunkte, Abweichung < 1e-9), `reduced_keeps_the_ends`, `uniform_knots_shape` (p=3, n=5 ⇒ `[0,0,0,0,1,2,2,2,2]`), `unclamped_matches_inside`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren. `extended`: Blüte des letzten Polynomstücks je der letzten `p` Kontrollpunkte, entsprechend am Anfang.
- [ ] **Step 4:** Tests grün, clippy und fmt.
- [ ] **Step 5:** Commit.

### Task 2: Kontrollpunkt-Spline im Modell, Löser und Format

**Files:**
- Modify: `crates/planora-sketch/src/{model,curves,geom,constraint,query,data,dimension}.rs`, `crates/planora-project/src/{convert,format}.rs`
- Test: `crates/planora-sketch/tests/control_spline.rs`, `crates/planora-project/tests/format_17.rs`

**Interfaces:**
- Consumes: `uniform_clamped_knots`, `point_unclamped`.
- Produces:
  - `CurveKind::ControlSpline { points: Vec<PointId>, degree: u8, knots: Vec<f64> }`.
  - `Geom::Nurbs { curve: Arc<NurbsCurve> }`, `Sampled::Nurbs(Arc<NurbsCurve>)`.
  - `Sketch::add_control_spline(&mut self, points: &[PointId], degree: u8) -> Result<CurveId, SketchError>`: wirksamer Grad `min(degree, n − 1)`, `degree` ∈ {3, 5}.
  - `Sketch::nurbs_of(&self, c: CurveId) -> Option<Arc<NurbsCurve>>`: für alle Freiformarten außer Spline ohne Griffe.
  - `pub(crate) fn nurbs_eval_dual(degree, knots, weights: Option<&[f64]>, pts: &[V2], t: &Dual) -> (V2, V2)` in `curves.rs`, de Boor in homogenen Koordinaten, Abschnitt geklemmt.
  - `FORMAT_VERSION = 17`, `needs_format_17(&FeatureKind) -> bool`, `CurveKindDto::KontrollSpline { punkte, grad, knoten }`.

Andere Crates, die `CurveKind` erschöpfend unterscheiden, bekommen hier einen vorläufigen Zweig mit klarer Ablehnung; Task 3 ersetzt ihn. `is_param_kind`, `curve_ends`, `into_curve`, `spline_parameter` (allgemein `(lo, hi)`) sind hier fertig.

- [ ] **Step 1: Failing tests** `control_spline_ends_on_first_and_last_point`, `degree_five_with_five_points_is_degree_four`, `degree_three_with_two_points_is_a_line`, `point_on_control_spline_solves`, `tangent_line_to_control_spline_solves`, `dual_eval_matches_nurbs_curve` (dichte Stellen, < 1e-12), `collinear_and_doubled_control_points_stay_finite` (Review Focus 2: Punkt auf den Nachbarn gezogen, `solve` ohne NaN, `curve_polyline` endlich), `from_data_rejects_bad_knots`. In `format_17.rs`: `control_spline_needs_17`, `deleted_control_spline_needs_17`, `roundtrip_keeps_it`, `old_files_stay_byte_equal` (vorhandene Beispieldateien wie in den Tests zu Format 16).
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren. `polyline_params`, `nearest`, `arc_length`, `bbox` für `Geom::Nurbs` über `nurbs_divisions` bzw. `bezier_segments`; `bbox` über Kontrollpunkte.
- [ ] **Step 4:** Tests grün; `cargo test -p planora-sketch -p planora-project -p planora-feature`.
- [ ] **Step 5:** Commit.

### Task 3: Kontrollpunkt-Spline in allen abhängigen Befehlen

**Files:**
- Modify: `crates/planora-sketch/src/{intersect,arrangement,profile,profile_legacy,profile_ref,edit,offset,fillet,snap,transform,pattern,helpers}.rs`
- Test: `crates/planora-sketch/tests/control_spline_edit.rs`, `crates/planora-feature/tests/control_spline_extrude.rs`

**Interfaces:**
- Consumes: `Geom::Nurbs`, `nurbs_of`, `sub_curve`, `extended`.
- Produces: `pub(crate) fn Sketch::control_spline_piece(&mut self, c: CurveId, u0: f64, u1: f64, s: PointId, e: PointId) -> Result<CurveKind, SketchError>` (Teilstück exakt mit neuen inneren Punkten, Teilungsstellen wie `KNOT_SNAP` auf Knoten gelegt). `locate_extend` nimmt `ControlSpline`. `check_copy_sources` und `copy_curve` lassen `Spline` und `ControlSpline` beim Spiegeln zu.

- [ ] **Step 1: Failing tests:** `trim_keeps_the_shape_of_the_rest` (dichte Abtastung < 1e-9), `break_gives_two_exact_pieces`, `trim_close_to_a_knot_is_stable` (Review Focus 5), `extend_hits_the_target_exactly`, `extend_keeps_the_old_part`, `offset_control_spline_is_a_fit_spline_at_distance`, `fillet_line_and_control_spline`, `mirror_control_spline_is_symmetric`, `mirror_fit_spline_now_works`, `mirror_ellipse_still_refused`, `pattern_control_spline_follows_source`, `snap_finds_control_points_and_curve`, `profile_with_control_spline_is_exact_nurbs`, `extrude_control_spline_profile_has_the_right_volume` (gegen numerische Fläche × Höhe).
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren. Abhängigkeiten nach dem Teilen über `settle_constraints` (Regel 7).
- [ ] **Step 4:** Tests grün. Die alten Tests, die Ablehnen beim Spiegeln fordern, bewusst umschreiben.
- [ ] **Step 5:** Commit.

### Task 4: Grad ändern und Kontrollpunkt einfügen (Modell)

**Files:**
- Modify: `crates/planora-sketch/src/edit.rs`
- Test: `crates/planora-sketch/tests/control_spline_degree.rs`

**Interfaces:**
- Consumes: `elevated`, `reduced`, `insert_knot`.
- Produces: `Sketch::set_control_spline_degree(&mut self, c: CurveId, degree: u8) -> Result<SettleReport, SketchError>`, `Sketch::insert_control_point(&mut self, c: CurveId, at: DVec2) -> Result<(PointId, SettleReport), SketchError>`. `SettleReport` ist der vorhandene Rückgabetyp von `settle_constraints`; gibt es keinen, ein `usize` mit der Zahl entfallener Abhängigkeiten.

- [ ] **Step 1: Failing tests:** `raising_degree_keeps_the_shape`, `lowering_degree_has_fewer_points_and_same_ends`, `insert_keeps_the_shape_and_old_ids`, `insert_drops_constraints_that_no_longer_hold`, `degree_change_rejects_other_kinds`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren (D3).
- [ ] **Step 4:** Tests grün.
- [ ] **Step 5:** Commit.

### Task 5: Kegelschnitt

**Files:**
- Modify: `crates/planora-sketch/src/{model,curves,geom,constraint,query,data,edit,intersect,arrangement,profile,profile_legacy,profile_ref,offset,fillet,snap,transform,pattern,helpers,dimension}.rs`, `crates/planora-project/src/{convert,format}.rs`
- Test: `crates/planora-sketch/tests/conic.rs`, `crates/planora-project/tests/format_17.rs`

**Interfaces:**
- Consumes: `Geom::Nurbs`, `nurbs_eval_dual`.
- Produces:
  - `CurveKind::Conic { start, end, apex: PointId, rho: f64 }`.
  - `Sketch::add_conic(&mut self, start, end, apex: PointId, rho: f64) -> Result<CurveId, SketchError>`, `Sketch::set_conic_rho(&mut self, c: CurveId, rho: f64) -> Result<(), SketchError>`.
  - `pub fn conic_nurbs(p0: DVec2, a: DVec2, p2: DVec2, rho: f64) -> Result<NurbsCurve, NurbsError>`, `pub(crate) fn conic_piece(p0, a, p2, rho, u0: f64, u1: f64) -> Option<(DVec2, DVec2, DVec2, f64)>`: homogene Teilung, Gewicht `w₁/√(w₀·w₂)`, `None` bei nicht positiven Gewichten. Auch für `u` außerhalb `[0, 1]` (Dehnen).
  - `CurveKindDto::Kegelschnitt { start, ende, scheitel, rho }`.

- [ ] **Step 1: Failing tests:** `shoulder_point_is_at_rho` (ρ in 0,01-Schritten: `C(0,5) = M + ρ(A − M)`, < 1e-12), `end_tangents_point_at_apex`, `rho_half_is_a_parabola` (Probe auf Parabelgleichung), `rho_range_is_refused` (0, 1, −0,1, NaN), `apex_on_chord_is_refused`, `conic_has_six_dof`, `trim_stays_a_conic_with_same_points` (dichte Abtastung < 1e-9), `trim_near_the_end_and_extreme_rho_is_stable` (Review Focus 5, ρ = 0,001 und 0,999), `extend_conic_exact_and_refuses_negative_weight`, `mirror_keeps_rho`, `profile_with_conic_is_exact`, `point_on_conic_and_tangent_line_solve`, `conic_needs_17_and_roundtrips`, `from_data_rejects_bad_rho`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren (A2, E).
- [ ] **Step 4:** Tests grün, `-p planora-sketch -p planora-project -p planora-feature`.
- [ ] **Step 5:** Commit.

### Task 6: Tangentengriff

**Files:**
- Modify: `crates/planora-sketch/src/{model,curves,geom,constraint,query,data,edit}.rs`, `crates/planora-project/src/{convert,format}.rs`
- Create: `crates/planora-sketch/src/spline_handles.rs`
- Test: `crates/planora-sketch/tests/spline_handles.rs`

**Interfaces:**
- Produces:
  - `CurveKind::Spline { points: Vec<PointId>, handles: Vec<SplineHandle> }`. Alle Stellen mit `Spline { points }` werden `Spline { points, .. }`.
  - `pub struct SplineHandle { pub at: PointId, pub line: CurveId, pub tip: PointId, pub curvature: HandleCurvature }`, `pub enum HandleCurvature { Form, Straight, Arc { arc: CurveId, center: PointId, other: PointId } }`.
  - `CurveKind::points()` liefert auch `tip`, `center`, `other`; neu `CurveKind::fit_points(&self) -> &[PointId]`.
  - `Sketch::activate_tangent_handle(&mut self, c: CurveId, at: PointId) -> Result<CurveId, SketchError>`, `Sketch::deactivate_handle(&mut self, c: CurveId, at: PointId) -> Result<(), SketchError>`.
  - `pub fn hermite_nodes<T: Scalar>(y, handles) -> Nodes<T>`: Wert, erste und zweite Ableitung je Knoten nach B4, in `Dual` und in `f64` (über `Dual`, wenn die Laufzeit das erlaubt; sonst Generik).
  - `Sampled::Hermite { nodes }`; `Geom::Spline` rechnet damit, sobald `handles` nicht leer ist. `Sketch::handled_spline_nurbs(&self, c) -> Result<NurbsCurve, SketchError>`: Grad 5, innere Knoten dreifach.
  - `CurveKindDto::Spline { punkte, griffe }`, `griffe` entfällt, wenn leer.

Löschen: Die Griff-Linie löschen ruft `deactivate_handle`; den Spline löschen nimmt seine Griffe mit (B8). Griffe zählen nicht als Profilkanten (Konstruktion).

- [ ] **Step 1: Failing tests:** `no_handles_is_bit_equal_to_the_old_spline` (dichte Stellen `==`), `activating_does_not_change_the_shape` (Zufallsfolgen fester Saat, `< 1e-9`), `tip_sets_the_derivative` (`C'(i) = 3(tip − at)`), `curve_is_g2_across_a_handle` (Sprung von Krümmung und Tangente über die Fuge numerisch < 1e-6), `horizontal_on_the_handle_line_holds_after_solve`, `length_dimension_on_the_handle_works`, `handle_on_two_point_spline_and_tiny_derivative` (Review Focus 1), `deleting_the_line_deactivates`, `deleting_the_spline_removes_handles`, `handle_is_not_a_profile_edge`, `dragging_tip_keeps_other_fit_points_fixed`, `handles_need_17_and_roundtrip`, `from_data_rejects_handle_not_on_a_fit_point`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren. `param_curve` nimmt `hermite_nodes`, `profile.rs` nimmt `handled_spline_nurbs`, wenn `handles` nicht leer ist.
- [ ] **Step 4:** Tests grün; alle bisherigen Spline-Tests unverändert grün.
- [ ] **Step 5:** Commit.

### Task 7: Krümmungsgriff

**Files:**
- Modify: `crates/planora-sketch/src/{spline_handles,constraint,data}.rs`, `crates/planora-project/src/{convert,format}.rs`
- Test: `crates/planora-sketch/tests/spline_curvature_handle.rs`

**Interfaces:**
- Consumes: `HandleCurvature`, `hermite_nodes`.
- Produces: `ConstraintKind::SplineCurvatureHandle(CurveId, PointId)` (intern, zwei Zeilen nach B2, Öffnung `min(L / r, 2,5 rad)`), `Sketch::activate_curvature_handle(&mut self, c: CurveId, at: PointId) -> Result<CurveId, SketchError>`, `Sketch::deactivate_curvature_handle(&mut self, c, at) -> Result<(), SketchError>`, `Sketch::end_curvature_dual` zur Nutzung in Task 9.

- [ ] **Step 1: Failing tests:** `activating_curvature_keeps_the_shape` (auch im Beispiel `(0,0),(10,0),(0,1)`, wo der Tangentialanteil 1,5 ist), `radius_sets_the_curvature` (κ an der Fuge = 1/r), `radius_dimension_drives_it`, `small_radius_stays_solvable` (r < L/2), `arc_on_the_right_side_has_negative_kappa`, `near_zero_curvature_starts_at_1000_l`, `handle_leaves_one_dof_for_the_radius`, `deleting_the_arc_keeps_the_tangent_handle`, `curve_stays_g2_while_dragging` (dichte Folge von Griffwerten).
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** Tests grün.
- [ ] **Step 5:** Commit.

### Task 8: Griffe in Stutzen, Brechen, Kopieren, Spiegeln, Muster

**Files:**
- Modify: `crates/planora-sketch/src/{edit,transform,pattern,helpers,snap,offset,fillet}.rs`
- Test: `crates/planora-sketch/tests/spline_handles_edit.rs`

**Interfaces:**
- Consumes: `activate_*`, `SplineHandle`.
- Produces: keine neuen öffentlichen Namen; Verhalten laut Tabelle E.

- [ ] **Step 1: Failing tests:** `trim_keeps_handles_on_remaining_points`, `trim_drops_handles_on_removed_points`, `copy_carries_handles`, `scale_scales_handles`, `mirror_adds_symmetric_on_at_and_tip_and_equal_on_arcs` (`solve` meldet nichts als überzählig), `pattern_instance_follows_source_handle`, `hidden_handle_does_not_snap` (Snap über Parameter „sichtbare Griffe“), `offset_of_handled_spline`, `extend_handled_spline_still_refused`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** Tests grün.
- [ ] **Step 5:** Commit.

### Task 9: Glatt G2

**Files:**
- Modify: `crates/planora-sketch/src/constraint.rs`, `crates/planora-app/src/sketch_state.rs` (nur `smooth_partner`, `smooth_kinds_ok`, `smooth_on_arc`, Meldung), `crates/planora-i18n/languages/{de,en}.ftl`
- Test: `crates/planora-sketch/tests/smooth_g2.rs`, `crates/planora-project/tests/format_17.rs`, `crates/planora-app/tests/sketch_constraints_s5.rs`

**Interfaces:**
- Consumes: `end_curvature_dual`, `activate_curvature_handle`.
- Produces: `Sketch::prepare_smooth(&mut self, a: CurveId, pa: PointId, b: CurveId, pb: PointId) -> Result<(), SketchError>` (legt Griffe nach C3 an und setzt ihre Lage vor dem Lösen), `Smooth` mit zwei Zeilen nach C1. `sketch-smooth-arc-g1` entfällt; en-Name der Abhängigkeit wird „Curvature“.

- [ ] **Step 1: Failing tests:** `spline_arc_is_g2` (Krümmungssprung < 1e-6, auch nach Radiusmaß 3 → 7), `control_spline_arc_is_g2`, `conic_control_spline_is_g2`, `spline_spline_with_handles_is_g2`, `spline_line_goes_straight`, `straight_handle_stays_at_zero_when_tip_is_dragged`, `natural_ends_add_no_row` (`dof` wie vorher), `fixed_spline_partner_is_followed` (nur eine Seite bewegt sich), `two_fixed_splines_refused`, `elliptic_arc_refused`, `old_g1_on_arc_stays` (Review Focus 4: Datei von vor Skizzen 7 mit `Smooth` Spline–Bogen: nach Laden und Lösen ist die Form gleich, Krümmungssprung bleibt, gespeicherte Bytes gleich), `redone_smooth_on_old_file_is_g2_and_17`. App: `smooth_on_arc_has_no_message_any_more` (alter Test umgeschrieben), `smooth_is_one_undo_step_with_handles`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** Tests grün.
- [ ] **Step 5:** Commit.

### Task 10: Fester Spline und Freiformkanten projizieren

**Files:**
- Modify: `crates/planora-sketch/src/{model,geom,curves,constraint,query,data,helpers,edit,transform,offset,profile,snap}.rs`, `crates/planora-app/src/sketch_project.rs`, `crates/planora-project/src/{convert,format}.rs`, `crates/planora-i18n/languages/{de,en}.ftl`
- Test: `crates/planora-sketch/tests/fixed_spline.rs`, `crates/planora-feature/tests/projection.rs`

**Interfaces:**
- Consumes: `Curve::Nurbs`, `sub_curve`, `reversed`, `transformed`.
- Produces: `CurveKind::FixedSpline { start, end: PointId, curve: Arc<NurbsCurve> }`, `Projected::Spline { start: DVec2, end: DVec2, curve: NurbsCurve }`, `CurveKindDto::FesterSpline { start, ende, grad, knoten, punkte, gewichte }`, Meldung `sketch-fixed-spline-no-copy`. `key_of` liefert für feste Splines Start, Ende, Mitte.

- [ ] **Step 1: Failing tests:** `projected_nurbs_edge_is_exact` (dichte Abtastung gegen Punktprojektion < 1e-9, auch rational), `sub_range_and_reversed`, `collinear_projection_becomes_a_line`, `point_projection_is_skipped`, `closed_edge_gives_one_curve_with_one_point` und `closed_projection_makes_a_profile_and_extrudes` (Review Focus 3), `reproject_keeps_id_and_constraints`, `degree_change_on_reproject_is_fine`, `kind_change_is_lost_reference_warning`, `body_projection_has_no_duplicates`, `fixed_spline_refuses_trim_copy_mirror_pattern`, `offset_of_fixed_spline_is_normal_geometry`, `point_on_fixed_spline_solves`, `restore_projected_fixed_spline`, `fixed_needs_17_and_roundtrips`. Die alten Tests zum Überspringen bewusst umschreiben.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren (F).
- [ ] **Step 4:** Tests grün, `-p planora-sketch -p planora-feature -p planora-project -p planora-app`.
- [ ] **Step 5:** Commit.

### Task 11: Schneiden am schrägen Kegel

**Files:**
- Modify: `crates/planora-kernel/src/boolean/section.rs`
- Test: `crates/planora-kernel/tests/section_cone.rs`, `crates/planora-feature/tests/projection.rs`

**Interfaces:**
- Consumes: `breakpoints`, `split_at`, `classify_seg`.
- Produces: `fn plane_cone_conic(plane: &Frame, cone: &Frame, radius: f64, slope: f64, bounds: (DVec3, DVec3), tol: &Tolerance) -> SurfaceIntersection` in `section.rs` nach G2.

- [ ] **Step 1: Failing tests:** dichte Lagen: Ebene in Winkeln in 2°-Schritten und mehreren Abständen, `every_section_point_is_on_the_cone_and_in_the_plane` (< 1e-7), `ellipse_when_steeper_than_the_flank`, `parabola_when_parallel_to_a_generator`, `hyperbola_only_the_right_branch`, `through_apex_gives_two_lines`, `touching_generator_gives_nothing`, `whole_cone_section_is_closed_loop_where_expected`, `boolean_plane_cone_still_unsupported` (Anti-Ziel: `intersect_surfaces` liefert unverändert `Unsupported`), `torus_still_refused`. Feature: `oblique_cone_section_in_sketch_follows_the_body`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren; alte Ablehn-Tests bei `section.rs:632` und `:875` bewusst umschreiben.
- [ ] **Step 4:** Tests grün, `-p planora-kernel -p planora-feature`.
- [ ] **Step 5:** Commit.

### Task 12: App: Werkzeuge, Klappmenü, Tasten, Symbole

**Files:**
- Modify: `crates/planora-app/src/{sketch_state,sketch_kinds,sketch_panel,sketch_view,sketch_cursor,icons,undo_labels,commands}.rs`, `crates/planora-app/src/keymap/defaults.rs`, `crates/planora-i18n/languages/{de,en}.ftl`
- Test: `crates/planora-app/tests/sketch_splines_tools.rs`, `crates/planora-app/tests/icons.rs`, `crates/planora-app/tests/sketch_kinds.rs`

**Interfaces:**
- Consumes: `add_control_spline`, `add_conic`.
- Produces: `ToolKind::{SplineFit, SplineControl, Conic}` (`ToolKind::of(Tool::Spline)`; die Kartenbereiche in `of` hängen an der Reihenfolge von `ALL`, alle danach verschieben), `HandIcon::{SketchSplineControl, SketchConic}`, Aktionen `skizze.spline_kontrollpunkte`, `skizze.kegelschnitt`, Felder `SketchSession::spline_degree: u8` (Vorgabe 5), `conic_rho: f64` (Vorgabe 0,5).

- [ ] **Step 1: Failing tests** (Fenster 1920×1080): `spline_button_has_three_kinds`, `strip_keeps_twenty_tools_and_its_width`, `control_spline_by_clicks_is_one_undo_step`, `esc_after_first_click_leaves_nothing`, `conic_three_clicks_then_rho_enter`, `conic_rho_field_refuses_out_of_range`, `esc_in_rho_leaves_nothing`, `degree_field_applies_to_new_spline`, `menu_create_has_conic`, `actions_are_bindable_without_default_key`, `control_polygon_drawn_while_selected`, `conic_guides_drawn_while_selected`, `new_icons_are_hand_drawn`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren (D1, D2, D4 Zeichnen, D5, D6).
- [ ] **Step 4:** Tests grün.
- [ ] **Step 5:** Commit.

### Task 13: App: Griffe, Rho-Griff, Grad und Einfügen

**Files:**
- Modify: `crates/planora-app/src/{sketch_state,sketch_panel,sketch_view,sketch_input,sketch_inline,undo_labels,app}.rs`, `crates/planora-app/src/keymap/defaults.rs`, `crates/planora-i18n/languages/{de,en}.ftl`
- Create: `crates/planora-app/src/sketch_handles.rs`
- Test: `crates/planora-app/tests/sketch_splines_handles.rs`

**Interfaces:**
- Consumes: `activate_tangent_handle`, `activate_curvature_handle`, `deactivate_*`, `set_conic_rho`, `set_control_spline_degree`, `insert_control_point`.
- Produces: `pub fn SketchSession::visible_handles(&self, sk: &Sketch) -> Vec<HandleView>`, `HandleView { at: DVec2, tip: DVec2, ghost: bool, … }`; Zug an einem gedämpften Griff schaltet ein, über `begin_drag` als Punktzug, ein Rückgängig-Schritt; Knöpfe und Aktionen `skizze.tangentengriff`, `skizze.kruemmungsgriff`, `skizze.kontrollpunkt_einfuegen`, `skizze.spline_grad`.

- [ ] **Step 1: Failing tests:** `selecting_spline_shows_ghost_handles`, `first_drag_activates_as_one_undo_step`, `esc_during_first_drag_leaves_nothing`, `undo_removes_the_handle`, `active_handle_hides_when_deselected_unless_constrained`, `hidden_handle_is_not_picked`, `tangent_button_toggles_at_selected_point`, `curvature_ghost_on_active_handle_activates`, `rho_grip_changes_rho_one_undo_step`, `rho_grip_stays_in_range`, `rho_field_in_panel`, `degree_field_for_selected_control_spline_shows_effective_degree`, `insert_control_point_by_click_one_undo_step`, `drag_handle_does_not_jump_free_parts`.
- [ ] **Step 2:** Ausführen, muss scheitern.
- [ ] **Step 3:** Implementieren (B6, B7, D3, D4 Griff).
- [ ] **Step 4:** Tests grün.
- [ ] **Step 5:** Commit.

### Task 14: Doku und voller Lauf

**Files:**
- Modify: `docs/funktionen.md`, `docs/superpowers/offene-punkte.md`, `CHANGELOG.md`

- [ ] **Step 1:** `offene-punkte.md`: Abschnitt „Teilprojekt Skizzen 7“ mit allen **Annahmen** und Abweichungen aus der Spec und den Aufgaben. Den Eintrag „Glatt am Bogen ist nur tangential“ (Skizzen 5) und die zwei Einträge zu Freiformkanten und Kegel (Skizzen 6) als erledigt streichen, Folgepunkt „Kegelschnitt in den Booleschen Operationen“ notieren.
- [ ] **Step 2:** `docs/funktionen.md` und `CHANGELOG.md` ergänzen.
- [ ] **Step 3:** Windows voll: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt -p <crate> -- --check` je Crate, `cargo test --workspace -j 2 --no-fail-fast`, `cargo deny check`, Messtests im Release (`--ignored` für sketch, feature, app, project). Erwartet: nur die bekannten Fehlschläge.
- [ ] **Step 4:** WSL: `~/PlanoraCAD` holen (`git fetch origin && git switch -C feature/skizzen-splines origin/feature/skizzen-splines`), `~/wsl-s6-full.sh` im Vordergrund. Erwartet: nur die zwei bekannten Fehlschläge.
- [ ] **Step 5:** Commit; `graphify . --update --code-only`.
