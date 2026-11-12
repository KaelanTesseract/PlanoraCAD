# Skizzen 9: DXF, SVG und Leinwand – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** DXF und SVG werden zu echten Skizzenkurven, ein Bild liegt als kalibrierbare Leinwand auf einer Ebene oder Fläche, und alles verhält sich wie in Autodesk Fusion.

**Architecture:** Reine Leser in `planora-exchange` machen aus Bytes eine neutrale 2D-Geometrie (`Import2d`). `planora-sketch::import` legt sie in einem Zug als Kurven an. Die Leinwand ist ein neues Feature (`FeatureKind::Canvas`) mit einer Bildtabelle im `Snapshot` (Muster: Netze). Gespeichert wird sie in Format 19, gezeichnet über eine neue Textur-Pipeline in `planora-render`. Die App bekommt das Menü „Einfügen“ mit drei Dialogen, den Baumordner „Leinwände“ und einen Kalibriermodus.

**Tech Stack:** Rust, egui/eframe 0.36, wgpu, `dxf` 0.6.2 (MIT), `usvg` 0.50 ohne Standard-Features (Apache-2.0 OR MIT), `image` 0.25 (schon im Lockfile).

**Spec:** `docs/superpowers/specs/2026-11-12-skizzen-dxf-svg-design.md` (dazu der Abschnitt „Nach der Prüfung festgelegt“). Jede Aufgabe liest die Spec mit.

## Global Constraints

- Texte nur über `tr!`/`msg!`, Schlüssel in `crates/planora-i18n/languages/de.ftl` und `en.ftl`, Namen wie in der deutschen und englischen Fusion: „Einfügen“/„Insert“, „DXF einfügen“/„Insert DXF“, „SVG einfügen“/„Insert SVG“, „Leinwand“/„Canvas“, „Leinwände“/„Canvases“, „Kalibrieren“/„Calibrate“, „Durch Objekte anzeigen“/„Display Through“, „Auswählbar“/„Selectable“, „Kontrollpunktrahmen“/„Control Point Frame“, „Eine Skizze“/„Single Sketch“, „Eine Skizze pro Layer“/„One Sketch per Layer“.
- Kommentare deutsch. Neue Abhängigkeiten nur nach `cargo deny check` ohne neue Fehler; kein Autodesk-Material, Testdateien selbst geschrieben.
- Ein Befehl = ein Rückgängig-Schritt; Abbruch (Esc, Abbrechen, Werkzeugwechsel) hinterlässt nichts.
- Alte Dateien bytegleich; `FORMAT_VERSION` wird 19, aber 19 wird nur geschrieben, wenn eine Leinwand im Dokument ist.
- Jede neue Aktion steht in `action_table!` (`crates/planora-app/src/keymap/defaults.rs`), ohne Vorgabetaste.
- Grenzen: `MAX_IMPORT_ELEMENTS = 100_000`, `MAX_BLOCK_DEPTH = 16`, `MAX_IMAGE_BYTES = 64 MiB`, `MAX_IMAGE_EDGE_PX = 16_384`, GPU-Kante höchstens `4096` px, Zusammenführtoleranz `1e-6 × Diagonale des Hüllquaders`.
- SVG: 1 px = 25,4/96 mm, Y umgedreht. DXF ohne `$INSUNITS`: vorgewählt ist die Anzeigeeinheit der Einstellungen, mit Hinweis.
- Leinwand-Vorgaben: Deckkraft 50, Durch Objekte anzeigen an, Auswählbar an, 1 px = 1 mm.
- Prüfen je Aufgabe: `cargo fmt -p <crate> -- --check`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`. Code-Fragen zuerst mit `graphify query "<frage>"`.
- Nach jeder Aufgabe eine Notiz in `D:\Vault\Vault\Projekte\PlanoraCAD\` (`2026-MM-TT Skizzen 9 Task N <Thema>.md`, verlinkt in `PlanoraCAD.md`).

## Review Focus

1. **DXF aus CAD-Programmen mit Blöcken in Blöcken und gespiegelten Einfügungen** (Maßstab −1): Bögen müssen ihre Drehrichtung behalten, sonst entstehen Gegenbögen. Test in Task 3.
2. **Polylinie mit Bulge und geschlossenem Flag, deren erster und letzter Punkt zusätzlich gleich sind:** Daraus darf kein Segment der Länge 0 werden, und das Profil muss geschlossen sein. Test in Task 2 und Task 6.
3. **SVG mit `transform` auf verschachtelten Gruppen und `viewBox` mit Versatz:** Die Lage und Größe in mm müssen stimmen. Test in Task 4.
4. **Rückgängig nach dem Einfügen einer Leinwand mit großem Bild:** Kein Kopieren der Bytes je Schritt, sondern dieselbe `Arc`. Test in Task 8.
5. **Abbruch während des Lesens im Hintergrund und gleich danach ein neuer Dialog:** Kein spätes Ergebnis darf in den neuen Dialog fallen. Test in Task 11.

---

### Task 1: Abhängigkeiten, Lizenzen und neutrale 2D-Geometrie

**Files:**
- Modify: `Cargo.toml` (workspace dependencies), `crates/planora-exchange/Cargo.toml`, `deny.toml` (nur falls nötig, mit Begründung)
- Create: `crates/planora-exchange/src/import2d.rs`
- Modify: `crates/planora-exchange/src/lib.rs`, `crates/planora-exchange/src/limits.rs`

**Interfaces:**
- Produces:
  - `pub enum Unit2d { Millimeter, Centimeter, Meter, Inch, Foot }` mit `fn mm(self) -> f64`
  - `pub enum Element2d { Point(DVec2), Line(DVec2, DVec2), Circle { c: DVec2, r: f64 }, Arc { c: DVec2, r: f64, start: f64, end: f64 } /* Radiant, gegen den Uhrzeigersinn */, Ellipse { c: DVec2, major: DVec2, ratio: f64, start: f64, end: f64 } /* voll: 0..TAU */, Quad([DVec2; 3]), Cubic([DVec2; 4]), Nurbs { degree: usize, knots: Vec<f64>, points: Vec<DVec2>, weights: Option<Vec<f64>> }, FitSpline(Vec<DVec2>), Text(Text2d) }`
  - `pub struct Text2d { pub text: String, pub font: String, pub height: f64, pub pos: DVec2, pub angle: f64, pub halign: HAlign2d, pub valign: VAlign2d }`, dazu `HAlign2d {Left, Center, Right}` und `VAlign2d {Bottom, Middle, Top}`
  - `pub struct Layer2d { pub name: String, pub elements: Vec<Element2d> }`
  - `pub struct Skipped { pub unknown: usize, pub three_d: usize, pub dimensions: usize, pub hatches: usize, pub images: usize, pub texts: usize, pub degenerate: usize }`
  - `pub struct Import2d { pub layers: Vec<Layer2d>, pub units: Option<Unit2d>, pub skipped: Skipped }` mit `fn bounds(&self) -> Option<(DVec2, DVec2)>` und `fn element_count(&self) -> usize`
  - Konstanten `MAX_IMPORT_ELEMENTS`, `MAX_BLOCK_DEPTH`, `MAX_IMAGE_BYTES`, `MAX_IMAGE_EDGE_PX` in `limits.rs`

- [ ] **Step 1:** Abhängigkeiten eintragen: `dxf = "0.6.2"`, `usvg = { version = "0.50", default-features = false }` und `image = { version = "0.25", default-features = false, features = ["png", "jpeg", "tiff"] }` in `planora-exchange`.
- [ ] **Step 2:** Run `cargo deny check`. Erwartet: keine neuen Fehler. Neue Warnungen (z. B. doppelte Versionen, `enum_primitive`/`num-traits` 0.1) begründet in `deny.toml` oder in der Notiz festhalten. Steht ein neuer Lizenz- oder Sicherheitsfehler da, die Aufgabe anhalten und Dennis fragen (Spec: dann eigener DXF-Leser).
- [ ] **Step 3:** Tests in `import2d.rs`: `bounds_cover_arc_extremes` (Bogen 0..π mit r = 2 um (0,0): Hüllquader y bis 2, nicht nur bis zu den Enden) und `unit_mm_factors` (Foot = 304,8, Inch = 25,4).
- [ ] **Step 4:** Typen schreiben, Tests grün, fmt/clippy.
- [ ] **Step 5:** Commit `Skizzen 9: neutrale 2D-Geometrie und Leser-Abhängigkeiten`.

### Task 2: DXF-Leser, Grundelemente

**Files:**
- Create: `crates/planora-exchange/src/dxf.rs`, `crates/planora-exchange/tests/dxf_read.rs`, `crates/planora-exchange/tests/data/dxf/*.dxf` (klein, selbst geschrieben, Text-DXF R12 und R2000)

**Interfaces:**
- Consumes: Task 1
- Produces: `pub fn read_dxf(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Import2d, ExchangeError>`

- [ ] **Step 1: Failing tests:**
  - `line_circle_arc_in_layers`: zwei Layer mit Name, Linie (0,0)–(10,0), Kreis r = 5 und Bogen 0°–90°; als `Arc` mit Radiant.
  - `insunits_4_is_mm_and_missing_is_none`
  - `lwpolyline_bulge_becomes_arc`: Bulge 1 zwischen (0,0) und (10,0) ergibt einen Halbkreis mit c = (5,0) und r = 5, gegen den Uhrzeigersinn. Bei negativem Bulge wird es ein Bogen im Uhrzeigersinn, als `Arc` mit vertauschten Enden.
  - `closed_polyline_with_repeated_last_point_has_no_zero_segment` (Review Focus 2)
  - `ellipse_partial_keeps_params`
  - `spline_with_control_points_and_weights`, sowie `spline_fit_points_only_becomes_fit_spline`
  - `point_entity`
  - `extrusion_minus_z_is_mirrored`: ein Kreis mit (0,0,−1) und Mitte x = 3 landet bei x = −3.
  - `out_of_plane_line_is_skipped`: `three_d` = 1.
  - `dimension_and_hatch_are_counted`
  - `paper_space_is_skipped`
  - `nan_and_zero_radius_are_degenerate`
- [ ] **Step 2:** Run `cargo test -p planora-exchange --test dxf_read`. Erwartet: FAIL, `read_dxf` fehlt.
- [ ] **Step 3:** `read_dxf` mit `dxf::Drawing::load(&mut Cursor)` schreiben. Die Einheit kommt aus `header.default_drawing_units`, nur Modellbereich, je `CHECK_EVERY` Elemente `cancel()` prüfen. Fehler aus `dxf` werden zu `ExchangeError` mit `tr!`-Text (`exchange-dxf-broken`).
- [ ] **Step 4:** Tests grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 3: DXF-Blöcke, Text und Robustheit

**Files:**
- Modify: `crates/planora-exchange/src/dxf.rs`, `crates/planora-exchange/tests/dxf_read.rs`
- Create: `crates/planora-exchange/tests/dxf_robust.rs`

**Interfaces:**
- Consumes: Task 2. Produces: keine neuen Namen.

- [ ] **Step 1: Failing tests:**
  - `insert_places_rotates_scales_block`
  - `nested_insert_depth_17_is_cut_and_counted`
  - `insert_array_rows_columns`
  - `mirrored_insert_keeps_arc_direction` (Maßstab x = −1; der Bogen 0°–90° um (0,0) wird zum Bogen 90°–180°, gegen den Uhrzeigersinn; Review Focus 1)
  - `nonuniform_scale_turns_circle_into_ellipse`
  - `text_and_mtext_become_text2d` (MTEXT `\P` ergibt einen Zeilenumbruch; `{\fArial|b1;Hi}` ergibt „Hi“ mit Schrift Arial)
  - `element_limit_stops_with_error` (mehr als `MAX_IMPORT_ELEMENTS` ergibt `ExchangeError` „zu viele Elemente“, kein Teilergebnis)
  - `cancel_returns_cancelled`
  - `binary_dxf_reads` (kleine Binärdatei aus einem selbst geschriebenen Text-DXF mit `dxf` erzeugt)
- [ ] **Step 2:** `dxf_robust.rs`: `truncated_files_never_panic` (jede Datei aus `tests/data/dxf` an 50 Stellen abgeschnitten) und `byte_flips_never_panic` (fester Startwert, 500 Varianten). Erwartet: nur `Ok` oder `Err`, kein Panic.
- [ ] **Step 3:** Run, erwartet FAIL. Dann umsetzen: Blöcke rekursiv mit einer 2D-Affinabbildung; ein Kreis unter ungleichem Maßstab wird zur Ellipse; bei negativer Determinante werden die Bogenenden getauscht und gespiegelt. Ein Panic im Leser von `dxf` wird mit `catch_unwind` zu einem Fehler, mit Kommentar warum.
- [ ] **Step 4:** Tests grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 4: SVG-Leser

**Files:**
- Create: `crates/planora-exchange/src/svg.rs`, `crates/planora-exchange/tests/svg_read.rs`, `tests/data/svg/*.svg`

**Interfaces:**
- Produces: `pub fn read_svg(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Import2d, ExchangeError>` (ein Layer „SVG“, `units = Some(Millimeter)`, Werte schon in mm)

- [ ] **Step 1: Failing tests:**
  - `rect_100px_is_26_458mm_wide`
  - `width_in_mm_with_viewbox_scales`
  - `y_axis_is_flipped` (ein Punkt bei SVG-y = 10 px landet bei −2,6458 mm)
  - `nested_group_transforms_and_viewbox_offset` (Review Focus 3)
  - `quad_and_cubic_kept_exact`
  - `subpath_close_adds_line_only_if_gap`
  - `text_element_counted_as_skipped_text`
  - `image_element_counted`
  - `svgz_reads`
  - `truncated_and_flipped_svg_never_panic`
  - `element_limit`
- [ ] **Step 2:** Run, erwartet FAIL.
- [ ] **Step 3:** Umsetzen: `usvg::Tree::from_data` mit Standard-`Options` (ohne Schriften), Gruppen rekursiv durchlaufen, `path.data()` mit `abs_transform` in mm umrechnen, Y umdrehen. Ein `<text>` wird gezählt; ohne das Feature `text` hält usvg Textknoten nicht als Pfade, darum vor dem Parsen mit `roxmltree` zählen.
- [ ] **Step 4:** Tests grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 5: Bild-Leser

**Files:**
- Create: `crates/planora-exchange/src/image.rs`, `crates/planora-exchange/tests/image_read.rs`, `tests/data/img/` (2×3-PNG, JPG, TIFF, selbst erzeugt)

**Interfaces:**
- Produces:
  - `pub enum ImageFormat { Png, Jpeg, Tiff }`
  - `pub struct ImageInfo { pub format: ImageFormat, pub width: u32, pub height: u32 }`
  - `pub fn probe_image(path_ext: &str, bytes: &[u8]) -> Result<ImageInfo, ExchangeError>`
  - `pub fn decode_rgba(bytes: &[u8], max_edge: u32) -> Result<(u32, u32, Vec<u8>), ExchangeError>` (verkleinert, wenn eine Kante über `max_edge` liegt; Seitenverhältnis bleibt)

- [ ] **Step 1: Failing tests:**
  - `png_jpg_tiff_sizes`
  - `extension_must_match_content` (PNG-Bytes mit `.jpg` ergeben die Meldung `exchange-image-wrong-extension`)
  - `too_big_bytes_or_edge_rejected`
  - `decode_downscales_to_4096` (synthetisches Bild 5000×100 wird zu 4096×82)
  - `broken_image_is_error_not_panic`
- [ ] **Step 2–4:** FAIL, umsetzen mit `image::guess_format` und `ImageReader`, grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 6: Geometrie in die Skizze einfügen

**Files:**
- Create: `crates/planora-sketch/src/import.rs`, `crates/planora-sketch/tests/import.rs`, `crates/planora-sketch/tests/import_perf.rs` (`#[ignore]`, Messtest im Release)
- Modify: `crates/planora-sketch/src/lib.rs`, `crates/planora-sketch/Cargo.toml` (`planora-exchange` als Abhängigkeit; prüfen, dass es keinen Kreis gibt, sonst `Import2d` nach `planora-math` oder in ein kleines Modul in `planora-sketch` verschieben und `planora-exchange` darauf abbilden)

**Interfaces:**
- Consumes: `Import2d`, `Element2d` (Task 1); die bestehenden `add_*_as`, `QUAD_RHO`, `CUBIC_KNOTS` (aus `text_refresh.rs`), Skizzentext aus Skizzen 8 (`add_text_box`).
- Produces:
  - `pub struct InsertOptions { pub fixed: bool, pub show_frame: bool, pub layers: Vec<bool> }`
  - `pub struct InsertReport { pub curves: usize, pub points: usize, pub texts: usize, pub missing_fonts: Vec<String>, pub skipped: Skipped, pub new_curves: Vec<CurveId> }`
  - `pub fn Sketch::insert_geometry(&mut self, src: &Import2d, place: &Placement, opt: &InsertOptions) -> Result<InsertReport, SketchError>`
  - `pub struct Placement { pub scale: DVec2 /* mm je Dateieinheit, negativ = spiegeln */, pub angle: f64, pub offset: DVec2, pub pivot: DVec2 }` mit `fn apply(&self, p: DVec2) -> DVec2` und `fn mirrors(&self) -> bool`
  - `pub fn preview_polylines(src: &Import2d, place: &Placement, layers: &[bool], tol: f64) -> Vec<Vec<DVec2>>` (für die Vorschau der Dialoge, ohne Skizze)
  - Ein Kontrollpunktrahmen-Merkmal je Kurve: `CurveData.show_frame: bool` (Vorgabe false; gespeichert in Task 9). Die Skizzenansicht zeichnet das Kontrollpolygon solcher Kurven immer; Test `show_frame_curves_draw_polygon` in Task 12.

- [ ] **Step 1: Failing tests (`tests/import.rs`):**
  - `square_of_four_lines_shares_points_and_is_profile` (4 Punkte, 1 Profil)
  - `quad_becomes_conic_rho_half` und `cubic_becomes_control_spline_3`
  - `nurbs_degree_3_keeps_knots`, `nurbs_degree_2_elevated_exact` (10 Proben, Abstand < 1e-9) und `nurbs_degree_7_split_into_cubics_within_tolerance`
  - `rational_nurbs_becomes_fixed_spline`
  - `fit_spline_becomes_spline`
  - `svg_fixed_true_fixes_all_points` und `dxf_fixed_false_leaves_free`
  - `merge_only_within_import` (eine bestehende Linie mit Endpunkt auf dem Endpunkt der Einfügung bleibt ein eigener Punkt)
  - `mirror_placement_keeps_arcs_correct`
  - `closed_bulge_polyline_gives_one_profile` (Review Focus 2)
  - `layer_filter`
  - `failure_leaves_sketch_unchanged` (ein Text mit unmöglicher Höhe 0 in der Mitte der Liste: `Err`, Skizze gleich `clone` vorher)
  - `solve_after_insert_changes_nothing`
- [ ] **Step 2:** Run, erwartet FAIL.
- [ ] **Step 3:** Umsetzen: erst alle Endpunkte sammeln und über ein Raster mit der Toleranz zusammenführen, dann die Kurven anlegen. Grad 2 und 4 exakt erhöhen (`planora-math` NURBS, falls vorhanden, sonst Gradanhebung nach Piegl/Tiller), Grad über 5 und ungleichmäßige Fälle, die `ControlSpline` nicht nimmt, in kubische Bézierstücke zerlegen. Die ganze Funktion arbeitet auf einem Klon und tauscht erst am Ende (alles oder nichts).
- [ ] **Step 4:** `import_perf.rs`: `ten_thousand_lines_insert_under_2s` und `ten_thousand_lines_profiles_measured` (misst nur und gibt die Zeit aus).
  Run `cargo test -p planora-sketch --release --test import_perf -- --ignored --nocapture`. Die Messwerte kommen in die Notiz. Liegen die Profile über 2 s, kommt ein Eintrag in `offene-punkte.md`.
- [ ] **Step 5:** Tests grün, fmt/clippy, Commit.

### Task 7: „Fest“ als Schalter (Fixieren/Lösen)

**Files:**
- Modify: `crates/planora-sketch/src/edit.rs` (oder wo `Fixed` angelegt wird), `crates/planora-app/src/sketch_state.rs` (Aktion `A::Fixed`, um Zeile 2359), `de.ftl`/`en.ftl`
- Test: `crates/planora-sketch/tests/fix_toggle.rs`, App-Test in `crates/planora-app/src/sketch_state.rs` (Testmodul)

**Interfaces:**
- Produces: `pub fn Sketch::toggle_fixed(&mut self, points: &[PointId]) -> FixToggle` mit `pub enum FixToggle { Fixed(usize), Released(usize) }`

- [ ] **Step 1: Failing tests:**
  - `all_fixed_selection_is_released`
  - `mixed_selection_fixes_rest`
  - In der App: `fixed_tool_on_fixed_box_selection_releases_in_one_undo_step` (Rahmenauswahl über ein eingefügtes SVG-Quadrat, „Fest“, ein Rückgängig stellt alle `Fixed` wieder her). Die Beschriftung im Verlauf lautet „Fixierung aufheben“/„Unfix“.
- [ ] **Step 2–4:** FAIL, umsetzen, grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 8: Leinwand im Modell

**Files:**
- Create: `crates/planora-feature/src/canvas.rs`, `crates/planora-feature/tests/canvas.rs`
- Modify: `crates/planora-feature/src/model.rs` (`FeatureKind::Canvas`, `label`, `dependencies`), `snapshot.rs` (`images`, `same_images`), `visibility.rs` (`HiddenFolders.canvases`), `compute.rs`/`recompute.rs` (Träger auflösen, Warnung), `lib.rs`

**Interfaces:**
- Produces:
  - `pub struct ImageId(pub u64)` und `pub struct CanvasImage { pub bytes: Arc<[u8]>, pub format: ImageFormat, pub width: u32, pub height: u32, pub hash: u64 }`
  - `pub enum CanvasBase { Standard(StandardPlane), Face(FaceRef), Plane(FeatureId) }`
  - `pub struct CanvasFeature { pub image: ImageId, pub base: CanvasBase, pub offset: DVec2, pub angle: f64, pub scale: DVec2, pub flip_h: bool, pub flip_v: bool, pub opacity: u8 /* 0..=100 */, pub display_through: bool, pub selectable: bool }`
  - `Snapshot.images: BTreeMap<ImageId, Arc<CanvasImage>>` (nur die, auf die eine Leinwand verweist)
  - `pub fn Document::add_image(&mut self, img: CanvasImage) -> ImageId` (gleiche `hash` und gleiche Bytes ergeben dieselbe Id)
  - `pub fn Document::canvas_frame(&self, id: FeatureId) -> Option<Frame>` (berechnete Lage samt Versatz, Winkel, Maßstab und Spiegelung; die Bildmitte liegt auf `offset`)
  - `pub fn CanvasFeature::corners(&self, frame: &Frame, img: &CanvasImage) -> [DVec3; 4]`
  - Warnung `feature-canvas-face-lost` (Feature bleibt an der letzten Lage)

- [ ] **Step 1: Failing tests:**
  - `canvas_on_xy_corners_1px_is_1mm` (ein Bild 200×100 ergibt die Ecken ±100/±50)
  - `flip_and_angle`
  - `canvas_on_face_follows_extrude_height`
  - `lost_face_is_warning_keeps_last_frame`
  - `undo_snapshot_shares_image_arc` (Review Focus 4: `Arc::ptr_eq` zwischen zwei Snapshots)
  - `same_image_twice_shares_id`
  - `unused_image_dropped_from_snapshot`
  - `hidden_folder_canvases`
- [ ] **Step 2–4:** FAIL. Umsetzen; die Trägerauflösung ist dieselbe wie in `compute_sketch` (`compute.rs:722-741`), herausgezogen in eine gemeinsame Funktion `resolve_base_frame`. Tests grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 9: Format 19

**Files:**
- Modify: `crates/planora-project/src/convert.rs` (`FORMAT_VERSION = 19`, `format_with`: 19 bei `FeatureKind::Canvas` oder einer Skizzenkurve mit `show_frame`; das Feld `rahmen` steht nur bei `true` in der Datei), `format.rs` (`VersionFile.bilder: Vec<ImageDto{id, format, daten /*Base64*/}>`, `skip_serializing_if = "Vec::is_empty"`; DTO `Leinwand`), `HiddenFolders`-DTO
- Test: `crates/planora-project/tests/canvas_format.rs`

**Interfaces:**
- Consumes: Task 8.

- [ ] **Step 1: Failing tests:**
  - `old_file_without_canvas_stays_byte_equal` (vorhandene Beispieldatei aus den Tests von Format 18 laden und speichern, Bytes gleich)
  - `canvas_roundtrip_keeps_bytes_and_settings`
  - `format_19_only_with_canvas_or_frame` (ohne Leinwand und ohne `show_frame` bleibt es 18 oder kleiner)
  - `show_frame_roundtrip`
  - `canvas_folder_hidden_roundtrip`
  - `image_total_limit_is_error`
  - `fixed_spline_outside_projection_loads` (eine Skizze mit `FixedSpline` aus `insert_geometry` speichern und laden)
- [ ] **Step 2–4:** FAIL, umsetzen, grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 10: Leinwand zeichnen

**Files:**
- Create: `crates/planora-render/src/canvas.rs`, `crates/planora-render/src/canvas.wgsl`
- Modify: `crates/planora-render/src/gpu.rs` (vierte Pipeline, `ViewportCallback.canvases`), `lib.rs`

**Interfaces:**
- Produces:
  - `pub struct CanvasDraw { pub key: u64 /* Bild-Hash */, pub corners: [[f32; 3]; 4], pub opacity: f32, pub display_through: bool }`
  - `pub struct CanvasTexture { pub key: u64, pub width: u32, pub height: u32, pub rgba: Arc<[u8]> }`
  - `ViewportCallback.canvases: Vec<CanvasDraw>` und `ViewportCallback.canvas_textures: Vec<CanvasTexture>` (nur neue, die GPU-Seite hält einen Zwischenspeicher je `key`)
  - `pub fn canvas_hit(corners: [DVec3; 4], ray: Ray, alpha_at: impl Fn(f64, f64) -> u8) -> Option<f64>` (Abstand; Alpha 0 zählt nicht)
- Reihenfolge im Bild: Flächen und Linien mit Tiefentest, dann Leinwände ohne „Durch Objekte“ mit Tiefentest und Alpha, dann Leinwände mit „Durch Objekte“ ohne Tiefentest. Die Skizze liegt als egui-Überlagerung ohnehin darüber.

- [ ] **Step 1: Failing tests:** `canvas_hit_center`, `canvas_hit_transparent_pixel_misses`, `canvas_hit_behind_camera_none` und `quad_uv_respects_flip` (reine Funktionen). Die Pipeline selbst bekommt `pipeline_builds_headless` nur, wenn es schon einen Headless-Test für die bestehenden Pipelines gibt (`graphify query "headless wgpu test"`), sonst prüft das der Handtest.
- [ ] **Step 2–4:** FAIL, umsetzen, grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 11: Menü „Einfügen“, Lesen im Hintergrund, Wählfeld, DXF-Dialog

**Files:**
- Create: `crates/planora-app/src/insert/mod.rs` (gemeinsames Gerüst), `insert/dxf_dialog.rs`, `insert/pick.rs`
- Modify: `crates/planora-app/src/app.rs` (`menu_bar_measured`: „Einfügen“ nach „Erzeugen“; `show_file_dialogs`), `keymap/defaults.rs` (`insert-dxf`, `insert-svg`, `insert-canvas`), `state.rs`, `de.ftl`/`en.ftl`

**Interfaces:**
- Consumes: `read_dxf` (Task 2), `preview_polylines` und `insert_geometry` (Task 6), `FileChooser::open` und `ws.exchange_dir`, `new_sketch_on_face` und `new_sketch_on_plane` (bzw. `add_sketch_in`).
- Produces:
  - `pub struct BackgroundRead<T> { … }` mit `fn start(path, f: fn(&[u8], &dyn Fn() -> bool) -> Result<T, ExchangeError>) -> Self`, `fn poll(&mut self) -> Option<Result<T, ExchangeError>>`, `fn cancel(&mut self)` und `fn progress(&self) -> f32`. Jeder Lauf hat eine Laufnummer; Ergebnisse alter Läufe werden verworfen (Review Focus 5).
  - `pub enum InsertTarget { OpenSketch, Plane(SketchPlane) }`
  - `pub struct TargetPick` (Wählfeld; nimmt Ursprungsebene, Konstruktionsebene oder ebene Körperfläche aus Klick oder Vorauswahl)
  - `pub struct DxfDialog { pub units: Unit2d, pub single: bool, pub offset: DVec2, pub layers: Vec<bool>, pub frame: bool, … }` mit `fn show(&mut self, ui, state) -> DialogAction`
- Verhalten nach Spec C, DXF-Dialog:
  - Die Einheitenliste ist mm, cm, m, in und ft.
  - Bei `units == None` ist die Anzeigeeinheit vorgewählt, dazu der Hinweis `insert-dxf-no-unit`.
  - Bei offener Skizze ist „pro Layer“ gesperrt und das Wählfeld entfällt.
  - OK ist ein Rückgängig-Schritt im Dokument (neue Skizze) bzw. in der Skizze (`edit_label` „DXF einfügen“).
  - Nach OK kommt der Hinweis zum Übersprungenen (`insert-skipped-summary` mit Anzahlen je Art; nur Arten mit mehr als 0).

- [ ] **Step 1: Failing tests** (App-Tests mit `egui::Context` wie bestehende Dialogtests):
  - `insert_menu_has_three_entries_after_create`
  - `dxf_dialog_cancel_leaves_document_unchanged`
  - `dxf_single_sketch_one_undo_step`
  - `dxf_per_layer_names_sketches_after_layers`
  - `dxf_into_open_sketch_disables_per_layer`
  - `dxf_no_unit_preselects_display_unit_with_hint`
  - `cancelled_read_result_never_reaches_new_dialog` (Review Focus 5)
  - `broken_dxf_shows_error_dialog_stays_open`
  - `dialog_full_width_rule_and_equal_cards` (wie `ImportDialog`-Tests, falls Karten vorkommen)
- [ ] **Step 2–4:** FAIL, umsetzen, grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 12: SVG-Dialog mit Griffen

**Files:**
- Create: `crates/planora-app/src/insert/svg_dialog.rs`
- Modify: `crates/planora-app/src/sketch_view.rs` (Vorschau und Griffe zeichnen), `sketch_grips.rs` (nur falls `layout` und `grip_motion` einen Einstieg ohne `TransformState` brauchen), `de.ftl`/`en.ftl`

**Interfaces:**
- Consumes: `read_svg` (Task 4), `Placement`, `preview_polylines` und `insert_geometry` mit `fixed: true` (Task 6), `BackgroundRead`, `TargetPick` (Task 11), `sketch_grips::{layout, hit, paint, grip_motion}`.
- Produces: `pub struct SvgDialog { pub offset: DVec2 /* y positiv nach unten, wie Fusion */, pub angle_deg: f64 /* positiv im Uhrzeigersinn */, pub flip_h: bool, pub flip_v: bool, pub scale: f64, pub frame: bool, … }` mit `fn placement(&self, bounds) -> Placement` (Hüllquader-Mitte auf dem Ursprung, dann spiegeln, skalieren, drehen und verschieben).

- [ ] **Step 1: Failing tests:**
  - `svg_starts_centered_on_origin`
  - `field_y_positive_moves_down`
  - `angle_positive_is_clockwise`
  - `flip_h_mirrors_about_y_axis`
  - `grip_drag_updates_fields_and_back` (`grip_motion` mit Schritt (10,0) ergibt `offset.x = 10`; Feld auf 20 ergibt den Griff an der neuen Stelle)
  - `svg_ok_is_one_sketch_undo_step_and_fixed`
  - `svg_without_open_sketch_creates_sketch`
  - `svg_text_skipped_hint`
  - `esc_leaves_nothing`
- [ ] **Step 2–4:** FAIL, umsetzen, grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 13: Leinwand-Dialog, Baumordner und Sperre in der Skizze

**Files:**
- Create: `crates/planora-app/src/insert/canvas_dialog.rs`
- Modify:
  - `crates/planora-app/src/browser.rs`: `Folder::Canvases` zwischen `Sketches` und `Construction`; `ALL`, `FolderState` mit 6 Einträgen, Label `tree-folder-canvases`, Auge und Kontextmenü „Leinwand bearbeiten“, „Kalibrieren“, „Umbenennen“ und „Löschen“; Doppelklick ergibt „Bearbeiten“.
  - `viewport.rs`/`app.rs`: `CanvasDraw` je sichtbarer Leinwand füllen, Textur-Zwischenspeicher mit `decode_rgba(…, 4096)` einmal je Bild, Auswahl über `canvas_hit`.
  - `de.ftl`/`en.ftl`

**Interfaces:**
- Consumes: Task 5, Task 8, Task 10, `BackgroundRead` und `TargetPick` (Task 11), Griffe wie in Task 12.
- Produces:
  - `pub struct CanvasDialog { pub edit: Option<FeatureId>, pub opacity: u8, pub display_through: bool, pub selectable: bool, pub offset: DVec2, pub angle_deg: f64, pub scale_xy: Option<f64>, pub scale: DVec2, pub flip_h: bool, pub flip_v: bool, … }`
  - `fn AppState::insert_canvas(&mut self, img: CanvasImage, f: CanvasFeature) -> FeatureId` (an der Verlaufsmarke)

- [ ] **Step 1: Failing tests:**
  - `canvas_defaults_50_through_selectable_1px_1mm`
  - `canvas_ok_one_document_undo_step_at_marker`
  - `edit_canvas_changes_feature_one_step`
  - `canvas_menu_disabled_while_sketch_open_with_tooltip` (`insert-canvas-close-sketch` = „Erst die Skizze beenden“)
  - `canvases_folder_only_when_present`
  - `canvas_eye_hides_and_is_saved`
  - `delete_canvas_drops_image_from_snapshot`
  - `not_selectable_canvas_is_clicked_through`
  - `wrong_extension_message`
- [ ] **Step 2–4:** FAIL, umsetzen, grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 14: Kalibrieren

**Files:**
- Create: `crates/planora-app/src/insert/calibrate.rs`
- Modify: `app.rs`/`viewport.rs` (Klickmodus), `sketch_inline.rs` (Feld am Zeiger wiederverwenden, falls es außerhalb der Skizze nutzbar ist; sonst eigenes Feld mit demselben Formelleser), `de.ftl`/`en.ftl`

**Interfaces:**
- Consumes: Task 8 (`canvas_frame`, `corners`), Task 10 (`canvas_hit`), der Formelleser für Längen mit Einheiten.
- Produces:
  - `pub struct Calibrate { canvas: FeatureId, first: Option<DVec2> /* Bildkoordinaten in mm */, second: Option<DVec2> }`
  - `pub fn calibrated_scale(old: DVec2, a: DVec2, b: DVec2, distance_mm: f64) -> Option<DVec2>` (gleicher Faktor für x und y; der erste Punkt bleibt an seiner Stelle, also wird `offset` mit angepasst)

- [ ] **Step 1: Failing tests:**
  - `calibrated_scale_doubles` (Abstand 10 mm gemessen, Eingabe 20 mm ergibt Faktor 2)
  - `first_point_stays_fixed`
  - `zero_distance_or_same_point_rejected`
  - `esc_leaves_nothing`
  - `enter_is_one_document_undo_step`
  - `formula_with_units_accepted` („1 in“ ergibt 25,4 mm)
  - `calibrate_disabled_while_sketch_open`
- [ ] **Step 2–4:** FAIL, umsetzen, grün, fmt/clippy.
- [ ] **Step 5:** Commit.

### Task 15: Doku und voller Lauf

**Files:**
- Modify: `docs/funktionen.md` (Abschnitt „DXF, SVG und Leinwand (Skizzen 9)“), `CHANGELOG.md`, `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt Skizzen 9“ mit allen **Annahmen** der Spec, den Messwerten aus Task 6 und einer Handliste für den Handtest)

- [ ] **Step 1:** Doku schreiben, in einfacher Sprache.
- [ ] **Step 2:** Die Handliste in `offene-punkte.md` umfasst mindestens:
  - DXF aus einem fremden Programm in richtiger Größe
  - DXF pro Layer
  - SVG-Logo mit Griffen, Spiegeln und Extrudieren
  - Logo mit „Fest“ lösen
  - Foto als Leinwand, mit Kalibrieren, Deckkraft, „Durch Objekte anzeigen“ und Auge
  - Leinwand auf einer Körperfläche nach Höhenänderung
  - Speichern und Laden
  - Kaputte Datei
  - Abbrechen beim Lesen einer großen Datei
  - Y-Richtung im SVG-Dialog gegen die Fusion-Hilfe prüfen
- [ ] **Step 3:** Voller Lauf Windows: fmt je Crate, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, die Messtests im Release.
  Erwartet: Es scheitert nur das bekannte `real_step_text_limit`.
- [ ] **Step 4:** Voller Lauf WSL (`~/wsl-s6-full.sh` auf dem Zweig). Erwartet: Es scheitern nur die bekannten Tests (`a_locked_old_json_never_ends_up_next_to_a_new_png` unter root).
- [ ] **Step 5:** Run `graphify . --update --code-only`.
- [ ] **Step 6:** Commit `Skizzen 9: Doku und Handliste`.
