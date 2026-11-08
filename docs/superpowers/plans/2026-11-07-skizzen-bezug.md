# Skizzen 6: Projizieren mit Bezug, Schneiden, Silhouette, Konstruktionsebenen – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Projizierte Geometrie wie in Fusion (lila, fest, profilfähig, folgt dem Körper, Bezug lösbar, verlorener Bezug = Warnung), Projizieren von Flächen und Körpern, Schneiden und Silhouette mit der Skizzenebene, Konstruktionsebenen als Features und „Skizzenebene neu festlegen“.

**Architecture:** Der Kernel bekommt zwei reine Bausteine (`section_by_plane`, `silhouette`) in `planora-kernel/src/boolean/section.rs`, gebaut aus `intersect_surfaces`, `curve_surface_roots` und `point_in_face`. `planora-sketch` projiziert künftig beliebige Kernkurvenstücke (`Sketch::add_projected_curve`/`reproject_curve`), nicht nur Kanten. In `planora-feature` wird `EdgeCopy` zu einer Projektion mit Herkunft (`CopySource`), `compute_sketch` behält bei verlorenem Bezug die Geometrie und setzt eine Warnung (`Feature::warning`), und ein neues Feature `FeatureKind::Plane(PlaneFeature)` liefert nur einen `Frame`. Die App zeigt Ebenen als egui-Überlagerung, pickt sie per Strahl und baut die Befehle in Klappmenü, Menü „Konstruieren“ und Browser ein. Format 16 nur für neue Einträge.

**Tech Stack:** Rust-Workspace, egui 0.36, Fluent (`de.ftl`/`en.ftl`, `tr!`), serde.

**Spec:** `docs/superpowers/specs/2026-11-07-skizzen-bezug-design.md`

## Global Constraints

- Verhalten, Namen und Kürzel wie Autodesk Fusion; Abweichungen in `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt Skizzen 6“) mit Grund. Annahmen über Fusion sind mit **Annahme** markiert.
- **graphify zuerst:** Wer baut oder prüft, fragt vor dem Lesen von Dateien den Wissensgraphen in `D:\Testumgebung\PlanoraCAD` (`graphify query "<Frage>" --budget 2000`, `graphify explain "<Symbol>"`) und liest dann die Dateien, auf die er zeigt. `graphify-out/` wird nie eingecheckt.
- Keine neuen Fremd-Abhängigkeiten; `cargo deny check` bleibt grün; Symbole selbst gezeichnet, kein Autodesk-Material.
- Alle Oberflächentexte über `tr!`/`msg!` mit Schlüssel in `crates/planora-i18n/languages/de.ftl` **und** `en.ftl`. Intern mm, Anzeige über `planora_i18n` (Zoll muss gehen). Farben nur über Theme-Tokens (`no_hard_coded_colors.rs`).
- **Dateiformat 16 nur, wenn nötig:** `FORMAT_VERSION` wird 16; `format_with` liefert 16 genau dann, wenn das Dokument eine Konstruktionsebene, eine Skizze mit `SketchPlane::OnPlane` oder eine Projektion mit `CopySource::Section`/`Silhouette` enthält. Alles andere wird bytegleich geschrieben wie heute (Test mit den vorhandenen Beispieldateien). Neue Felder `#[serde(default, skip_serializing_if = …)]`.
- Ein Befehl ist genau ein Rückgängig-Schritt (Dokument: eine `edits`-Änderung je Befehl; Skizze: ein `atomic`). Esc, Rechtsklick und Werkzeugwechsel hinterlassen nichts.
- Bestehende Kantenkopien aus alten Dateien folgen weiter und bleiben Konstruktion.
- Tests bei 1920×1080, nie 800×600.
- Prüfen: `cargo test -p <crate> --target-dir C:/Users/denni/AppData/Local/Temp/claude/4b2c4ad6da040f20/431e3bbb-18ac-4517-ad90-b7cfe592eb2a/scratchpad/target-s6 -j 2`; am Ende `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt -p <crate> -- --check` je Crate (**nicht** `cargo fmt --all`, das scheitert unter Windows), `cargo deny check`. Nichts pushen. Vor dem Merge kein CI, sondern voller Lauf unter Windows und in WSL.
- Commit-Nachrichten deutsch, enden mit Leerzeile und `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Alte Tests, die neues Verhalten treffen, werden **bewusst** umgeschrieben (Kommentar „Skizzen 6“); alle anderen bleiben grün.

## Review Focus

1. **Körper mit deckungsgleichen Kanten** (Quader auf seine Deckfläche projiziert: Ober- und Unterkante liegen übereinander, senkrechte Kanten werden Punkte) ⇒ genau 4 Linien, keine doppelten, keine Fehler. Test in Task 9.
2. **Ebene berührt eine Fläche nur** (Skizzenebene tangential an Zylinder, Ebene genau durch eine Kante oder Ecke) ⇒ keine Stücke der Länge 0, kein Fehler, Berührlinie höchstens einmal. Test in Task 1.
3. **Nutzer-Abhängigkeiten an einer Projektion, deren Kante verschwindet** ⇒ Kurve und Abhängigkeiten bleiben, Skizze `Ok` mit Warnung, Extrusion rechnet weiter. Test in Task 4.
4. **Löschen einer Konstruktionsebene mit Skizzen darauf** ⇒ Rückfrage nennt die Skizzen, Löschen nimmt sie samt Extrusionen mit (wie bei Skizze auf Fläche). Test in Task 5.
5. **Skizzenebene auf eine Fläche eines späteren Features oder der eigenen Extrusion setzen** ⇒ abgelehnt mit Meldung, Dokument unverändert. Test in Task 6.

---

### Task 1: Kernel – Ebenenschnitt `section_by_plane`

**Files:**
- Create: `crates/planora-kernel/src/boolean/section.rs`
- Modify: `crates/planora-kernel/src/boolean/mod.rs` (Modul, `pub use`), `crates/planora-kernel/src/lib.rs` (Export)
- Test: in `section.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `pub struct SectionPiece { pub face: Id<Face>, pub curve: Curve, pub t0: f64, pub t1: f64 }`
  - `pub enum SectionError { Unsupported { face: Id<Face>, surface: &'static str }, Freeform(Id<Face>) }` mit `Display` über `tr!("kernel-section-unsupported", surface = surface_label(..))`, `tr!("kernel-section-freeform")`
  - `pub fn section_by_plane(solid: &Solid, plane: &Frame, faces: Option<&[Id<Face>]>, tol: &Tolerance) -> Result<Vec<SectionPiece>, SectionError>`

Vorgehen je Fläche (alle, oder nur `faces`): `face_geom`, `intersect_surfaces(Plane{plane}, surface)`. `None`/`Touch` ⇒ nichts (Touch einer Linie am Zylinder: die Berührlinie kommt als Kante der Nachbarflächen oder gar nicht – **Annahme**, Fusion liefert sie nicht). `Coincident` ⇒ die Randkanten der Fläche als Stücke. `Curves` ⇒ je Kurve Haltepunkte = Treffer der Randkanten mit der Ebene (`curve_surface_roots`, wie `edge_hits` in `imprint.rs`; gemeinsame Hilfsfunktion dorthin auslagern statt kopieren) plus Naht-/Polpunkte (`seam_segments`, `pole_point`); Parameter sortieren, Stücke kürzer als `tol.linear` verwerfen, ein Stück behalten, wenn `point_in_face(mitte) == Inside`. Geraden ohne Haltepunkte gibt es nicht (Fläche beschränkt). Zum Schluss Stücke zusammenlegen, die als Punktfolge (Anfang, Mitte, Ende innerhalb `tol.linear`, Richtung egal) gleich sind. Freiform ⇒ `Freeform`, `Unsupported` ⇒ `Unsupported`.

- [ ] **Step 1: Failing tests**
  - `box_cut_by_mid_plane_gives_four_lines`: Quader 10×20×30 (`primitives`), Ebene z=15 ⇒ 4 Linien, Gesamtlänge 60 ± 1e-9.
  - `box_cut_through_top_face_gives_its_boundary`: Ebene z=30 ⇒ 4 Stücke (keine doppelten).
  - `cylinder_cut_oblique_gives_ellipse_arcs_inside`: Zylinder r=5 h=20, Ebene durch (0,0,10), Normale 30° gekippt ⇒ eine geschlossene Ellipse (major = 5/cos30°), alle Abtastpunkte im Zylinderinneren ± tol.
  - `cylinder_cut_along_axis_gives_two_lines_of_height`: Ebene x=2 ⇒ 2 Linien der Länge 20, Abstand 2·√21.
  - `tangent_plane_gives_no_zero_length_pieces`: Ebene x=5 am Zylinder ⇒ kein Stück mit Länge ≤ tol (Review Focus 2); dazu dichte Folge x = −6…6 in 0,01: nie Fehler, Stückzahl ∈ {0, 2, 4+Deckel}.
  - `plane_through_box_edge_gives_that_edge_once`: Ebene x=0 am Quader ⇒ Randkanten der Fläche x=0 genau einmal.
  - `sphere_cut_gives_circle`; `oblique_cone_is_unsupported`; `nurbs_face_is_freeform_error`.
- [ ] **Step 2:** `cargo test -p planora-kernel section` ⇒ FAIL (Modul fehlt).
- [ ] **Step 3:** `section.rs` nach obigem Vorgehen; Texte `kernel-section-unsupported`, `kernel-section-freeform` in de/en.
- [ ] **Step 4:** Tests ⇒ PASS; `cargo test -p planora-kernel` bleibt grün.
- [ ] **Step 5:** Commit „Kernel: Ebenenschnitt eines Körpers (section_by_plane)“.

### Task 2: Kernel – `silhouette`

**Files:** Modify `crates/planora-kernel/src/boolean/section.rs`; Test ebenda.

**Interfaces:**
- Consumes: `SectionPiece`, `SectionError`, Beschnitt-Hilfsfunktion aus Task 1.
- Produces: `pub fn silhouette(solid: &Solid, face: Id<Face>, view: DVec3, tol: &Tolerance) -> Result<Vec<SectionPiece>, SectionError>`

Analytische Umrisskurven in Richtung `view` (Einheitsvektor, Normale der Skizzenebene): Zylinder/Kegel mit `axis ⟂ view` (|dot| ≤ `tol.angular`) ⇒ die beiden Mantellinien bei Winkel ±90° zu `view` (Kegel: Mantellinien über `slope`); Kugel ⇒ Großkreis senkrecht zu `view`; Torus mit `axis ∥ view` ⇒ Kreise R+r und R−r in der Mittelebene. Jede andere Lage ⇒ `Unsupported`. Beschnitt wie Task 1 (Haltepunkte = Treffer der Randkanten mit der Umrisskurve, über `seg_intersections`; Stück behalten, wenn Mitte `Inside`). Ebene Flächen ⇒ leeres Ergebnis.

- [ ] **Step 1: Failing tests** `cylinder_side_view_gives_two_lines_of_height` (Zylinder r=5 h=20 Achse z, view = Y ⇒ 2 Linien Länge 20 bei x=±5), `half_cylinder_gives_one_line` (Mantel nur 0…180° ⇒ 1 Linie), `sphere_gives_full_circle`, `torus_top_view_gives_two_circles`, `cylinder_view_along_axis_is_empty` (nur Kanten; Ergebnis leer, kein Fehler), `oblique_cylinder_is_unsupported`.
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** PASS. **Step 5:** Commit „Kernel: Silhouette gekrümmter Flächen“.

### Task 3: Skizze – beliebige Kurvenstücke projizieren, elliptische Bögen, verknüpfte Kurven

**Files:** Modify `crates/planora-sketch/src/helpers.rs` (`add_projected_edge`, `reproject_edge`, `project_edge`, `Projected`); Test in `crates/planora-sketch/tests/` (neue Datei `projection.rs`).

**Interfaces:**
- Produces:
  - `pub struct ProjectStyle { pub construction: bool }` (alte Kantenkopie: `true`; neu: `false`)
  - `pub fn add_projected_curve(&mut self, curve: &planora_kernel::Curve, t0: f64, t1: f64, style: ProjectStyle) -> Result<CurveId, SketchError>`
  - `pub fn reproject_curve(&mut self, id: CurveId, curve: &planora_kernel::Curve, t0: f64, t1: f64) -> Result<(), SketchError>`
  - `add_projected_edge`/`reproject_edge` bleiben als dünne Hüllen (Kante ⇒ `(e.curve, e.t0, e.t1)`, `construction: true`), damit alte Aufrufer und Tests gleich bleiben.
  - `pub fn projects_to_point(&self, curve: &Curve, t0: f64, t1: f64) -> bool` (Gerade senkrecht zur Ebene; für Task 9).

Neu: Ein Kreisbogen in schräger Ebene und ein Ellipsenbogen werden `Projected::EllipticArc { center, axis, minor, start, end }` ⇒ `add_elliptic_arc` mit festen Punkten und `Ellipse…`-Werten wie bei der Ellipse; `reproject_curve` führt sie nach. Mit `construction: false` sind die Punkte normale (nicht Konstruktions-)Punkte, die Kurve keine Konstruktion; die `Fixed`-Werte bleiben.

- [ ] **Step 1: Failing tests** `oblique_arc_projects_to_elliptic_arc` (Viertelkreis r=10 in Ebene 45° gekippt ⇒ EllipticArc, Halbachsen 10 und 10·cos45°, Enden auf den projizierten Enden ± 1e-9), `linked_projection_is_not_construction_and_forms_profile` (vier Linien eines Rechtecks mit `construction:false` ⇒ `profiles().len() == 1`), `reproject_curve_keeps_user_constraint` (Linie projiziert, Nutzer hängt `Coincident` einer freien Linie an ein Ende, Kante verschoben ⇒ freie Linie folgt, Id gleich), `elliptic_arc_reprojects`, `old_edge_copy_unchanged` (alle Tests rund um `add_projected_edge` unverändert grün).
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** `cargo test -p planora-sketch` PASS. **Step 5:** Commit „Skizze: Kurvenstücke projizieren, elliptische Bögen, verknüpfte Kurven“.

### Task 4: Feature – Projektion mit Herkunft, verlorener Bezug als Warnung

**Files:** Modify `crates/planora-feature/src/model.rs` (`EdgeCopy`, `Feature`), `compute.rs` (`compute_sketch`, `Computed`), `recompute.rs` (`store`, `reads_visible`, `cache_key`), `lib.rs` (Exporte); Test `crates/planora-feature/tests/projection.rs`.

**Interfaces:**
- Consumes: Task 1–3.
- Produces:
  - `pub enum CopySource { Edge(EdgeRef), Section { face: FaceRef, index: usize }, Silhouette { face: FaceRef, index: usize } }`
  - `pub struct EdgeCopy { pub curve: CurveId, pub source: CopySource }` (Feld `edge` entfällt; `EdgeCopy::edge(&self) -> Option<&EdgeRef>` als Hilfe). `dependencies()` liest die Herkunft aus allen Arten.
  - `Feature::warning: Option<String>` (nicht im Format, wird nur gerechnet); `Computed::warning: Option<String>`.
  - `pub fn SketchFeature::break_link(&mut self, curve: CurveId) -> bool` (entfernt den Eintrag, `Fixed` bleibt).
  - `pub fn SketchFeature::prune_copies(&mut self) -> bool` (entfernt Einträge zu gelöschten Kurven; aufgerufen in `commit_draft` und beim Laden).

`compute_sketch`: Für jede Projektion den Bezug auflösen und die Kurve nachführen (`Edge` ⇒ `resolve_edge` + `reproject_curve`; `Section`/`Silhouette` ⇒ `resolve_face`, `section_by_plane(.., Some(&[face]))` bzw. `silhouette`, Stück `index` in der Reihenfolge des Kernels, nachführen). Scheitert es (nicht gefunden, Art geändert, Index fehlt, Kurve tot), bleibt die Kurve wie gespeichert und der Name kommt in die Warnung `tr!("feature-sketch-projection-lost", count = n)`; das Feature ist `Ok`. Nur `resolve_face` der Skizzenebene bleibt ein Fehler. Anzeige der Warnung in Task 8.

- [ ] **Step 1: Failing tests** `lost_edge_keeps_geometry_and_warns` (Skizze mit projizierter Kante eines Quaders, der durch ein Verschieben-Feature ersetzt wird, das danach gelöscht wird ⇒ Status `Ok`, `warning.is_some()`, Kurve an alter Stelle, Extrusion `Ok`; Review Focus 3 mit Nutzer-`Coincident`), `changed_kind_keeps_geometry_and_warns`, `deleted_curve_is_pruned_not_error`, `break_link_stops_following` (nach `break_link` und Quadergröße ändern bleibt die Kurve), `section_copy_follows_body`, `silhouette_copy_follows_radius`, `hints_do_not_bump_edits` (wie bestehender Test).
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen; alte Aufrufer von `EdgeCopy { edge }` (App `state.rs:2402`, Format) auf `CopySource::Edge` umstellen. **Step 4:** `cargo test -p planora-feature` PASS. **Step 5:** Commit „Feature: Projektionen mit Herkunft, verlorener Bezug als Warnung“.

### Task 5: Feature – Konstruktionsebene

**Files:** Create `crates/planora-feature/src/planes.rs`; Modify `model.rs` (`FeatureKind::Plane`, `label`, `dependencies`, `consumed_bodies`, `FeatureResult::plane`), `compute.rs` (`check_parameters`, `compute_unchecked`), `formulas.rs` (`field_keys`, Feldwerte), `ops.rs` (Abhängige), `params.rs`, `rollback.rs`; Test `crates/planora-feature/tests/planes.rs`.

**Interfaces:**
- Produces:
  - `pub enum PlaneBase { Standard(StandardPlane), Face(FaceRef), Plane(FeatureId) }`
  - `pub enum StdAxis { X, Y, Z }`
  - `pub enum AxisRef { Standard(StdAxis), Edge(EdgeRef), SketchLine { sketch: FeatureId, curve: CurveId } }`
  - `pub enum PointRef { Origin, EdgeEnd { edge: EdgeRef, end: bool }, SketchPoint { sketch: FeatureId, point: PointId } }`
  - `pub enum PlaneDef { Offset { base: PlaneBase, distance: f64 }, Angle { axis: AxisRef, angle: f64 }, Mid { a: PlaneBase, b: PlaneBase }, ThreePoints([PointRef; 3]), Tangent { face: FaceRef, base: PlaneBase, angle: f64 } }`
  - `pub struct PlaneFeature { pub def: PlaneDef }`; `FeatureKind::Plane(PlaneFeature)`; Formelfelder `"abstand"` (Offset), `"winkel"` (Angle, Tangent; Grad wie andere Winkel).
  - `pub fn Document::plane_frame(&self, id: FeatureId) -> Option<Frame>` (aus `FeatureResult::plane`).
  - `pub fn resolve_plane_base(doc: &Document, base: &PlaneBase, solids: &[&Solid], tol: &Tolerance) -> Result<Frame, FeatureError>`

Rechnung (`planes.rs`, rein): Offset = Basis um `distance` entlang z. Angle: Bezugsebene durch die Achse – Kante: erste angrenzende ebene Fläche (`EdgeRef.a`), X/Y ⇒ XY, Z ⇒ XZ, Skizzenlinie ⇒ Skizzenebene; Drehung um die Achse um `angle`. Mid: parallel ⇒ Mitte, z wie `a`; sonst Winkelhalbierende durch die Schnittgerade (`plane_plane`); deckungsgleich ⇒ Fehler `feature-plane-same`. ThreePoints: kollinear (Fläche des Dreiecks ≤ tol²) ⇒ `feature-plane-collinear`; x = p1−p0. Tangent: Zylinder/Kegel/Kugel, Berührpunkt dort, wo die Basis-Normale um `angle` um die Achse gedreht die Fläche trifft; andere Flächen ⇒ `feature-plane-tangent-kind`. Ergebnis: `Computed { plane: Some(frame), bodies: vec![] }`. Eine fehlende Referenz ist Fehler (wie andere Features). Löschen nimmt Abhängige mit (über `dependencies`).

- [ ] **Step 1: Failing tests** je Art ein Frame-Test (z. B. `offset_from_xy_is_parallel_at_distance` 12,5 ⇒ origin.z 12,5, z = Z), `offset_follows_face_when_box_grows`, `angle_about_x_axis_45`, `mid_of_two_box_faces`, `mid_of_non_parallel_is_bisector`, `three_points_from_box_corners`, `collinear_points_error`, `tangent_to_cylinder_at_90`, `formula_drives_offset` (Parameter „a=7“, Formel `a*2` ⇒ 14), `deleting_plane_lists_dependent_sketch` (Review Focus 4: `delete` ⇒ `HasDependents` mit Skizze; `delete_with_dependents` entfernt Skizze und Extrusion).
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen; der Compiler zeigt jede `match`-Stelle über `FeatureKind`. **Step 4:** PASS. **Step 5:** Commit „Feature: Konstruktionsebenen“.

### Task 6: Feature – Skizze auf Konstruktionsebene, Skizzenebene neu festlegen

**Files:** Modify `model.rs` (`SketchPlane::OnPlane(FeatureId)`, `dependencies`), `compute.rs` (Frame aus `plane_frame`), `ops.rs` (neu `set_sketch_plane`); Test `crates/planora-feature/tests/redefine.rs`.

**Interfaces:**
- Produces: `pub fn Document::set_sketch_plane(&mut self, sketch: FeatureId, plane: SketchPlane) -> Result<(), FeatureError>` — ein `commit_update`, prüft mit `check_references`, dass die neue Ebene nur auf Features **vor** der Skizze zeigt (sonst `FeatureError::ForwardReference`, Text `feature-plane-later`), und dass keine Schleife entsteht (Fläche eines Features, das von der Skizze abhängt ⇒ derselbe Fehler).

- [ ] **Step 1: Failing tests** `sketch_on_offset_plane_follows_distance`, `redefine_keeps_2d_geometry` (Punkte in Skizzenkoordinaten bitgleich, Frame neu), `redefine_reprojects_copies`, `redefine_to_later_face_is_rejected` und `redefine_to_own_extrusion_face_is_rejected` (Review Focus 5: Dokument unverändert, `edit_count` gleich), `redefine_is_one_edit`.
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** PASS. **Step 5:** Commit „Feature: Skizzen auf Konstruktionsebenen, Skizzenebene neu festlegen“.

### Task 7: Dateiformat 16

**Files:** Modify `crates/planora-project/src/format.rs` (`EdgeCopyDto`, `PlaneDto`, `KindDto`), `convert.rs` (`FORMAT_VERSION`, `format_with`, `kind_to`, `kind_from`, Prüfungen „Inhalt neuer als Format“); Test `crates/planora-project/tests/format16.rs`.

**Interfaces:**
- `EdgeCopyDto { kurve, #[serde(default, skip_serializing_if = Option::is_none)] kante: Option<EdgeRefDto>, #[serde(default, skip_serializing_if = Option::is_none)] schnitt: Option<FaceIndexDto>, #[serde(default, skip_serializing_if = Option::is_none)] silhouette: Option<FaceIndexDto> }` — genau eins gesetzt; alte Dateien haben `kante` und schreiben sich gleich.
- `PlaneDto::Ebene { feature: u64 }`; `KindDto::Konstruktionsebene(Box<PlaneFeatureDto>)` mit `art` ∈ `versatz | winkel | mitte | drei_punkte | tangential` und deutschen Feldnamen wie im übrigen Format.
- `fn needs_format_16(doc) -> bool` als oberste Regel in `format_with`.

- [ ] **Step 1: Failing tests** `old_files_roundtrip_bytewise` (alle Dateien unter den vorhandenen Format-Testdaten: laden, speichern, Bytes gleich, Format unverändert), `plane_feature_writes_16_and_roundtrips` (je Art), `on_plane_sketch_writes_16`, `section_copy_writes_16`, `edge_copy_with_linked_curve_stays_15` (neue nicht-Konstruktions-Projektion braucht kein neues Format), `format15_file_with_plane_is_rejected` (Inhalt neuer als Format).
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** `cargo test -p planora-project` PASS. **Step 5:** Commit „Dateiformat 16: Konstruktionsebenen und neue Projektionen“.

### Task 8: App – Lila, Bezug lösen, Warnung, Aufräumen

**Files:** Modify `crates/planora-app/src/theme.rs` (`SketchColors::projected`), `sketch_view.rs` (`draw`, `finished_sketch_shapes`), `state.rs` (`commit_draft` ruft `prune_copies`; neu `break_projection_link`), Kontextmenü der Skizzenkurve (Datei, in der heute das Kurven-Kontextmenü gebaut wird), `browser.rs`, `properties.rs`, `history_bar/model.rs` (Warnung), `de.ftl`/`en.ftl`; Tests in `crates/planora-app/tests/`.

**Interfaces:**
- Consumes: `EdgeCopy`, `SketchFeature::break_link`, `Feature::warning` (Task 4).
- Produces: `pub fn AppState::break_projection_link(&mut self, curve: CurveId) -> Result<(), String>` (ein Skizzen-Rückgängig-Schritt); `SketchColors::projected` (dunkel `#a98be8`, hell `#6a3fc0`, wie `BODY_COLORS[3]`).

Farbregel in `draw()`: gewählt > **projiziert (Kurve in `draft.copies`) ⇒ `projected`** > Konstruktion > bestimmt > frei. Projizierte Konstruktionskurven lila gestrichelt. Warnung: Browser-Zeile mit Warnsymbol und Hover-Text, Eigenschaften zeigen den Text, Verlaufsleiste `StepState::Warning` (gelb, Token `warn`). Menüeintrag „Verknüpfung aufheben“ (en „Break Link“) nur an projizierten Kurven.

- [ ] **Step 1: Failing tests** `projected_curve_uses_projected_color`, `break_link_is_one_undo_step` (Rückgängig stellt die Verknüpfung wieder her), `deleting_projected_curve_prunes_copy`, `warning_shows_in_browser_and_history`.
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** `cargo test -p planora-app` PASS. **Step 5:** Commit „App: projizierte Geometrie lila, Verknüpfung aufheben, Warnung“.

### Task 9: App – Projizieren (Kanten, Flächen, Körper), Klappmenü

**Files:** Modify `sketch_state.rs` (`Tool::ProjectEdge` bleibt als Variante, Zustand `ProjectState { filter: ProjectFilter, link: bool }`), `state.rs` (`project_into_sketch`), `sketch_view.rs` (Hover für Flächen/Körper), `sketch_panel.rs` (Feld mit Filter und Schalter), Leiste/Klappmenü (`ToolKind::Project`, `ToolKind::Intersect`, `ToolKind::Silhouette` wie die Muster-Liste), `keymap/defaults.rs`, `icons.rs`, `.ftl`; Tests `crates/planora-app/tests/project.rs`.

**Interfaces:**
- Produces:
  - `pub enum ProjectFilter { Entities, Bodies }`
  - `pub enum ProjectTarget { Edge(usize, Id<Edge>), Face(usize, Id<Face>), Body(usize) }`
  - `pub fn AppState::project_into_sketch(&mut self, target: ProjectTarget, link: bool) -> Result<usize, String>` (Anzahl neuer Kurven; ein Rückgängig-Schritt für den ganzen Klick).

Fläche ⇒ alle Kanten ihrer Schleifen; Körper ⇒ alle Kanten. Kanten, die `projects_to_point`, entfallen; deckungsgleiche Projektionen (gleiche Art, Endpunkte/Mitte innerhalb `tol.linear`, auch mit schon vorhandenen projizierten Kurven derselben Skizze) nur einmal. Je Kurve ein `EdgeCopy` mit `CopySource::Edge` (nur bei `link`). Kurven `construction: false`. Eine Kante, die nicht geht (Freiform), wird übersprungen und in der Meldung gezählt (`sketch-project-skipped`). Taste P bleibt `skizze.projizieren` (gespeicherte Belegungen bleiben gültig); Anzeigename wird „Projizieren“ / „Project“. Klappmenü „Projizieren/Einschließen“ ersetzt den Knopf; `STRIP_TOOLS` bleibt 20.

- [ ] **Step 1: Failing tests** `project_box_body_gives_four_lines_without_duplicates` (Review Focus 1, Skizze auf Deckfläche), `project_face_with_hole_gives_both_loops`, `project_edge_is_linked_and_not_construction`, `link_off_creates_no_copy`, `freeform_edge_is_skipped_with_count`, `project_click_is_one_undo_step`, `esc_leaves_nothing`, `strip_still_fits_1080`.
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** PASS. **Step 5:** Commit „App: Projizieren von Kanten, Flächen und Körpern“.

### Task 10: App – Schneiden und Silhouette

**Files:** Modify `sketch_state.rs` (`Tool::Intersect`, `Tool::Silhouette`), `state.rs` (`intersect_into_sketch`, `silhouette_into_sketch`), `sketch_view.rs`, `sketch_panel.rs`, `keymap/defaults.rs` (Aktionen `skizze.schneiden`, `skizze.silhouette`, ohne Vorgabetaste), `commands.rs`, `.ftl`; Tests `crates/planora-app/tests/intersect.rs`.

**Interfaces:**
- Consumes: `section_by_plane`, `silhouette` (Task 1–2), `CopySource::Section/Silhouette` (Task 4).
- Produces: `pub fn AppState::intersect_into_sketch(&mut self, target: ProjectTarget) -> Result<usize, String>`; `pub fn AppState::silhouette_into_sketch(&mut self, body: usize, face: Id<Face>) -> Result<usize, String>`.

Schneiden nimmt Körper (alle Flächen) oder Fläche; je Stück eine Kurve, `CopySource::Section { face, index }`. Kernelfehler erscheinen als Meldung, nichts wird angelegt. Silhouette nimmt eine Fläche.

- [ ] **Step 1: Failing tests** `intersect_cylinder_gives_ellipse_on_tilted_plane` (Skizze auf Konstruktionsebene 30° ⇒ eine Ellipse, verknüpft), `intersect_follows_radius_change`, `intersect_freeform_reports_and_creates_nothing`, `silhouette_of_cylinder_side`, `intersect_is_one_undo_step`.
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** PASS. **Step 5:** Commit „App: Schneiden und Silhouette“.

### Task 11: App – Ebenen anzeigen und anklicken

**Files:** Modify `crates/planora-render/src/scene.rs` (`Pick::Plane(FeatureId)`, `Pick::Vertex { body: usize, edge: Id<Edge>, end: bool }`), `crates/planora-render/src/pick.rs` (`pick_plane`, `pick_vertex`), `crates/planora-app/src/viewport.rs` (Zeichnen im `sketches_slot`), `state.rs` (`pick`, `set_selection`, Hover), `theme.rs` (`plane_fill`, `plane_edge`, orange); Tests in `crates/planora-render` und `crates/planora-app/tests/plane_view.rs`.

**Interfaces:**
- Produces: `pub fn pick_plane(ray: &Ray, rect: &PlaneRect) -> Option<f64>` (Abstand); `pub struct PlaneRect { pub frame: Frame, pub half: DVec2 }`; `pub fn pick_vertex(..) -> Option<(usize, Id<Edge>, bool)>` (Ecke innerhalb 8 px, verdeckte zählen nicht); `fn AppState::plane_rect(&self, id: FeatureId) -> Option<PlaneRect>`.

Größe: halbe Kante = max(25 mm, 0,6 × Diagonale der Hüllquader aller sichtbaren Körper), Mitte = Fußpunkt des Ursprungs bzw. Mitte der Basisfläche. Füllung halbdurchsichtig, Rand deckend; gewählt/hover heller. Klick: Kanten und Flächen gewinnen, wenn sie näher liegen als die Ebene; sonst die Ebene. Unterdrückte, fehlerhafte und zurückgerollte Ebenen werden nicht gezeichnet.

- [ ] **Step 1: Failing tests** `ray_hits_plane_inside_rect`, `ray_misses_outside_rect`, `nearer_face_wins_over_plane`, `plane_wins_when_in_front`, `vertex_pick_within_8px`, `suppressed_plane_not_drawn`.
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** PASS. **Step 5:** Commit „App: Konstruktionsebenen anzeigen und anklicken“.

### Task 12: App – Ebenen-Befehle, Menü „Konstruieren“, Browser

**Files:** Modify `feature_dialog.rs` (Fenster je Art mit Auswahlfeldern wie beim Verrunden, Feld für Abstand/Winkel mit Formel, Vorschau), `app.rs` (Menü „Konstruieren“), `browser.rs` (Ordner „Konstruktion“ mit Kontextmenü Bearbeiten/Skizze erstellen/Umbenennen/Unterdrücken/Löschen), `icons.rs` (`FeatureIcon::Plane`, fünf Befehlssymbole), `undo_labels.rs`, `properties.rs`, `history_bar/*`, `keymap/defaults.rs` (Aktionen `konstruieren.versatzebene`, `.winkelebene`, `.mittelebene`, `.drei_punkte`, `.tangentialebene`, ohne Vorgabetaste), `commands.rs`, `.ftl`; Tests `crates/planora-app/tests/plane_commands.rs`.

**Interfaces:**
- Consumes: Task 5, 11.
- Produces: `pub fn AppState::add_plane(&mut self, def: PlaneDef) -> Result<FeatureId, String>`; `pub fn AppState::new_sketch_on_plane(&mut self, id: FeatureId) -> Result<FeatureId, String>`.

Namen (**Annahme**, deutsche Fusion): „Versatzebene“, „Ebene unter Winkel“, „Mittelebene“, „Ebene durch drei Punkte“, „Tangentialebene“; en „Offset Plane“, „Plane at Angle“, „Midplane“, „Plane Through Three Points“, „Tangent Plane“. Feature-Name im Baum „Ebene1“, „Ebene2“ … (en „Plane1“). Ein Fenster „OK“ = ein Schritt, „Abbrechen“/Esc hinterlässt nichts. „Skizze auf Fläche“ im Menü „Erzeugen“ nimmt auch eine gewählte Ebene.

- [ ] **Step 1: Failing tests** `offset_plane_dialog_ok_adds_one_step`, `cancel_leaves_nothing`, `browser_lists_planes_in_construction_folder`, `sketch_from_plane_context_menu`, `all_plane_actions_are_bindable`.
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** PASS. **Step 5:** Commit „App: Befehle für Konstruktionsebenen“.

### Task 13: App – Skizzenebene neu festlegen

**Files:** Modify `browser.rs` (Kontextmenü Skizze), `history_bar/view.rs` (Kontextmenü), `state.rs` (Wahlmodus `PendingPick::SketchPlane(FeatureId)`, `redefine_sketch_plane`), `viewport.rs` (Klick im Wahlmodus, Hinweis in der Statuszeile), `keymap/defaults.rs` (`skizze.ebene_neu`), `.ftl`; Tests `crates/planora-app/tests/redefine.rs`.

**Interfaces:**
- Consumes: `Document::set_sketch_plane` (Task 6).
- Produces: `pub fn AppState::redefine_sketch_plane(&mut self, sketch: FeatureId, plane: SketchPlane) -> Result<(), String>`.

Ablauf: Eintrag „Skizzenebene neu festlegen“ (en „Redefine Sketch Plane“) ⇒ Wahlmodus; Klick auf ebene Fläche, Konstruktionsebene oder Grundebene im Browser-Ordner „Ursprung“ (dort neu anklickbar) ⇒ Änderung; Esc/Rechtsklick beendet ohne Änderung. Ist die Skizze offen, wird sie vorher geschlossen (wie Extrudieren aus dem Kontextmenü).

- [ ] **Step 1: Failing tests** `redefine_via_face_click`, `redefine_via_origin_plane`, `esc_leaves_nothing`, `rejected_shows_message_and_keeps_document`, `undo_restores_old_plane`.
- [ ] **Step 2:** FAIL. **Step 3:** umsetzen. **Step 4:** PASS. **Step 5:** Commit „App: Skizzenebene neu festlegen“.

### Task 14: Dokumentation und Gesamtprüfung

**Files:** Modify `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt Skizzen 6“: Abweichungen und Annahmen aus Spec und Tasks; alte Einträge Z. 67, 120, 129, 137, 151 als ersetzt markieren), `docs/funktionen.md`, `CHANGELOG.md` (unter „Unreleased“).

- [ ] **Step 1:** Doku schreiben.
- [ ] **Step 2:** Windows: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt -p <crate> -- --check` je Crate, `cargo test --workspace` (Target-Ordner wie oben), `cargo deny check`, Messtests im Release. Erwartet: alles grün.
- [ ] **Step 3:** WSL (Ubuntu): derselbe Lauf. Erwartet: alles grün.
- [ ] **Step 4:** `graphify . --update --code-only`.
- [ ] **Step 5:** Commit „Doku: Teilprojekt Skizzen 6“. Handtest nur nach Dennis' Go.
