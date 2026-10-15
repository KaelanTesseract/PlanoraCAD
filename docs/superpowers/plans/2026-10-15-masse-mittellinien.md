# Zeichnungen – Maße und Mittellinien (Teilprojekt 12) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Zeichnungen bekommen Maße (linear, Ø, R, Winkel, Maßketten und Bezugsmaße, Toleranzen) und Mittellinien; Maße hängen an der Modellgeometrie, rechnen sich nach Änderungen neu, lassen sich von Hand setzen oder automatisch vorschlagen und erscheinen in SVG und DXF als Linien, Pfeile und Text.

**Architecture:** `planora-drawing` bekommt Maß-Datentypen, Messung (`dimension.rs`), Mittellinien (`centerline.rs`), Darstellung als Markierungen (`annotate.rs`), Vorschlag (`suggest.rs`) und die Ausgabe der Markierungen in SVG/DXF. `planora-feature` löst die Verweise (`GeoRefData`) gegen die Körper auf und verwaltet Maße im `Document`. `planora-project` bekommt Format 5. `planora-app` bekommt Werkzeug, Bearbeiten-Fenster, Browser-Abschnitt und Export der Maße.

**Tech Stack:** Rust 2024 (Workspace), `planora-kernel` (öffentliche Schnittstelle), `planora-math`, `planora-exchange`, `planora-drawing` (Teilprojekt 11), `planora-feature::refs` (`EdgeRef`, `FaceRef`), `serde`/`serde_json`, egui 0.36. Keine neue externe Abhängigkeit.

**Spec:** `docs/superpowers/specs/2026-10-15-masse-mittellinien-design.md` (Spec). Der Plan argumentiert aus der Spec; bei Widerspruch gilt die Spec, mit den **Festlegungen der Planung** unten.

## Global Constraints

- Branch `feature/masse-mittellinien` (Basis `master` db80de3, Spec-Commit aae4619). Nicht auf `master` arbeiten; Merge entscheidet der Nutzer.
- **Geschützte Pfade bleiben Byte für Byte wie in 945fa67:** `crates/planora-kernel/**`, `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`. Nur deren **öffentliche** Schnittstelle benutzen.
- **Bestehendes Verhalten von Teilprojekt 11 bleibt:** alle vorhandenen Tests (Zeichenpaket, Feature, Projekt, App) laufen unverändert grün; Ausnahmen sind ausdrücklich genannte Konstruktoren-/Feld-Ergänzungen (mechanische Anpassung von Struktur-Literalen). Bestehende Ausgabedateien ohne Maße und ohne Mittellinien (SVG/DXF) bleiben **Byte für Byte gleich**, solange keine Ansicht Mittellinien oder Maße hat (Test in Aufgabe 5/6).
- Abhängigkeitsrichtung: `planora-drawing` ← `planora-feature` ← `planora-project` ← `planora-app`. `planora-drawing` darf `planora-feature` **nicht** kennen (daher `GeoRefData`/`Geo3` in `planora-drawing`, Umwandlung und Auflösung in `planora-feature`).
- Längen in **Millimetern** (Blatt-mm für Darstellung, Modell-mm für Werte), Winkel intern Radiant, in Anzeige und DXF Grad.
- Darstellung (Konstanten in `crates/planora-drawing/src/annotate.rs`, alle Blatt-mm): `DIM_TEXT_H = 3.5`, `TEXT_ADVANCE = 0.6` (Zeichenbreite = 0,6 × Höhe, feste Schätzung), `ARROW_LEN = 3.5`, `ARROW_HALF_W = 0.6`, `WITNESS_GAP = 1.0`, `WITNESS_OVER = 2.0`, `DIM_STEP = 7.0` (Abstand gestapelter Maßlinien), `DEFAULT_OFFSET = 10.0` (erste Maßlinie), `DIM_LINE_W = 0.25`, `TEXT_GAP = 1.0` (Text über der Maßlinie), `CENTER_OVERSHOOT = 3.0`, `CENTER_W = 0.25`, Strichpunkt-Muster `[12.0, 3.0, 2.0, 3.0]` (Strich, Lücke, Punktstrich, Lücke). Toleranzen: Text der Toleranz 0,7 × `DIM_TEXT_H`.
- Arbeitsgrenzen (`limits.rs`): `MAX_DIMENSIONS = 2000` je Zeichnung. Zahlen in Dateien endlich, `|·| ≤ MAX_POS_MM` (1e5). Überschreiten → benannter Fehler, nie stilles Kürzen.
- Zahlentext der Maße: deutsches Komma, `decimals` Nachkommastellen (Vorgabe **1**), `-0` wird `0`, Minus als `−` (U+2212) in Anzeige und SVG, in DXF `-`; Sonderzeichen: Ø (U+00D8), ° , ±; in DXF `%%c`, `%%d`, `%%p` (Zeichenkürzel), keine Unicode-Zeichen außer Latin-1.
- Alle Meldungen deutsch, Laiensprache; keine Hinweise auf Datenherkunft; Oberfläche volle Fensterbreite, nebeneinanderstehende Karten gleich hoch (Gedächtnisregeln des Projekts).
- Quality Gates je Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p <betroffene Pakete>` (bei Speicherfehlern `-j 2`, nie `cargo clean`); vor Abschluss: Gesamtlauf, `cargo deny check`, Prüfung der geschützten Pfade.
- **Tests dürfen nicht dieselbe Rechnung wiederholen:** Sollwerte aus den Eingabemaßen der Testkörper (Formelmodelle), eigene Leser für SVG und DXF, Mutationsbeweis für jeden Regressionstest. Testcode in diesem Plan ist Vorlage: Namen an die tatsächliche API anpassen, Zahlen nicht abschwächen.
- Commit-Stil: `feat:`/`test:`/`fix:`/`docs:` (deutsch), mit der Zeile `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Maß nach Modelländerung oder Löschen des Bezugs:** der Wert folgt dem Modell oder das Maß wird **rot** mit Grund – nie ein falscher oder stehengebliebener Wert – Aufgaben 2, 8, 10.
2. **Maße in Isometrie oder mit schräg stehenden Bezügen (Kante nicht quer, Kreis schräg):** benannter Fehler statt verzerrtem Wert – Aufgaben 2, 12.
3. **Alte Dateien (Format 1–4) und Dateien ohne Maße:** unverändert Byte für Byte; feindliche Dateien mit Maßen (NaN, riesige Zahlen, fremde Kennungen, doppelte Kennungen, unbekannte Arten, Listen über der Grenze) → klare Fehlermeldung, kein Absturz – Aufgabe 9.
4. **Sonderzeichen in Maßtexten** (Ø, °, ±, −, Komma, sehr lange eigene Texte, Anführungszeichen, `<`, `&`): in SVG richtig maskiert, in DXF richtig kodiert; Fusion öffnet die Dateien – Aufgaben 5, 6, 14.
5. **Sehr kleine Maße (Maßlinie unter 6 mm), A0-Blatt, Maßstab 1:100 und 10:1; Maß außerhalb des Innenrahmens oder weit außerhalb des Blatts:** Pfeile/Text vernünftig, nichts verschwindet stillschweigend (Hinweis „außerhalb des Blatts“) – Aufgaben 4, 12.
6. **Vorschlag auf leerem Dokument, Netz-Dokument, Ansicht ohne Berechnung, wiederholtes Drücken:** keine Doppelungen, benannte Meldung, nie Absturz – Aufgaben 7, 12.

## Festlegungen der Planung (Rulings gegenüber der Spec)

- **P1 Geometrie-Typ `Geo3`:** `planora-feature` löst `GeoRefData` zu `Geo3` auf (`Point(DVec3)`, `Line { a, b }` (Strecke mit Enden), `Circle { center, normal, radius }`, `Axis { origin, dir, radius: f64 }` für Zylinderflächen, `Sphere { center, radius }`); `planora-drawing` kennt nur diese Typen, nicht die Körper. Die **Messung** (Aufgabe 2) bekommt `&[Geo3]` plus `Basis` und liefert Wert und Anker.
- **P2 Verweisdaten:** `GeoRefData` hat dieselben Felder wie `EdgeRef`/`FaceRef` (je Marke `origin: u64`, `role: String`, `hint: DVec3`; Kante: zwei Flächenmarken `a`, `b` plus `hint`), aber ohne Abhängigkeit zu `planora-feature`. Umwandlung in `planora-feature::georef` (verlustfrei, beide Richtungen getestet).
- **P3 Vorgaben:** `decimals = 1`; neue Normansichten haben `centerlines = true`, die Isometrie `false`; Ø/R-Führungslinie Vorgabe 45° (`angle = 45.0`); `offset` bei linearen Maßen Vorgabe `DEFAULT_OFFSET`, bei weiteren Maßen derselben Ansicht und Richtung `DEFAULT_OFFSET + n·DIM_STEP`.
- **P4 Messrichtungen:** `Horizontal` misst entlang `ex`, `Vertical` entlang `ey` der Ansichtsbasis, `Aligned` zwischen zwei Punkten (euklidisch in der Bildebene). Ein Bezug „Kante“ liefert die Lage der projizierten Geraden quer zur Messrichtung (sie muss quer stehen, sonst `NotAcross`), „Eckpunkt“/„Mitte“ liefern den Punkt, „Achse“ die Lage der projizierten Achse (muss quer stehen).
- **P5 Ø/R-Darstellung:** Kreis oder Bogen mit Blick in Achsrichtung → Führungslinie aus dem Kreis (Pfeil auf dem Kreis, Text am Ende); Kreis von der Seite (Normale ⟂ Blickrichtung) oder Zylinderfläche quer → lineares Maß quer zur Achse mit Vorsatz „Ø“ (Radius: „R“ nur bei Kreis/Bogen mit Blick in Achsrichtung und bei Kugel). Schräg gesehene Kreise → `NotAcross`.
- **P6 Maßketten/Bezugsmaße:** `ChainSpec { group, mode, order }`. Alle Maße einer Gruppe haben dieselbe Messrichtung. `Chain`: gemeinsame Maßlinie (gleicher `offset`, Texte jeweils zwischen den Pfeilen); `Baseline`: gestapelte Maßlinien `offset + order·DIM_STEP`. Gruppen entstehen im Werkzeug (Kette/Bezug) und im Vorschlag.
- **P7 Textbreite:** `width = TEXT_ADVANCE · DIM_TEXT_H · zeichenzahl`, Text eines Maßes mit Toleranz in zwei Zeilen (`+0,2 / −0,1` gestapelt) hat die Breite des breiteren Teils; reine Schätzung, **dokumentiert als Grenze** (andere Schrift kann breiter sein).
- **P8 Mittellinien in `ViewLines`:** `compute_view` füllt `ViewLines.centerlines: Vec<CenterLine>` (Ansichtskoordinaten, ungeskaliert) immer; `build_sheet` nimmt sie nur bei `ViewSpec.centerlines = true` auf. Der Schlüssel der Berechnung ändert sich nicht (`(model_edit_count, Ansichtsart)`).
- **P9 Markierungen:** `SheetView.marks: Vec<Mark>`; `build_sheet(drawing, results)` bleibt (leere Maße), neu `build_sheet_with(drawing, results, dims: &ResolvedDims)`; die Oberfläche und der Export rufen die neue Funktion. Mittellinien sind `Mark::CenterLine`.
- **P10 SheetLine mit Ursprung:** `SheetLine` bekommt `origin: StrokeOrigin` (für das Werkzeug); mechanische Anpassung der Literale in Tests.
- **P11 Format 5:** `FORMAT_VERSION = 5`; geschrieben nur, wenn eine Zeichnung Maße enthält **oder** eine Ansicht `centerlines = true` hat; Format-4-Dateien laden mit `centerlines = false`; der Schreiber lässt `mittellinien`/`masse` bei Format ≤ 4 weg (Byte-genau).
- **P13 Export bei verlorenen Maßen:** Hat ein Maß seinen Bezug verloren, wird der Export **gesperrt** (Zustand „Fehlgeschlagen“ mit Text „Das Maß … hat seinen Bezug verloren: …“); nichts wird geschrieben (Aufgabe 14).
- **P12 Außerhalb des Blatts:** Ein Maß, dessen Darstellung außerhalb des Blatts liegt, wird trotzdem gezeichnet und in der Oberfläche **gewarnt** („liegt außerhalb des Blatts“); der Export schreibt es und zählt es in der Karte („Maße außerhalb: n“).

## Dateistruktur

```
crates/planora-drawing/src/
  dimension.rs   // DimId, DimKind, GeoRefData, GeoRef, Tolerance, ChainSpec, DimSpec; Geo3; measure(); DimError
  centerline.rs  // CenterLine, centerlines(bodies, basis, extent) -> Vec<CenterLine>
  annotate.rs    // Konstanten, Mark, layout_dimension(...), format_value(), tolerance_text(), ResolvedDim(s)
  suggest.rs     // suggest_dimensions(...)
  model.rs       // ViewSpec.centerlines, Drawing.dimensions/next_dim, Hilfsfunktionen
  sheet.rs       // SheetLine.origin, SheetView.marks, build_sheet_with
  svg.rs, dxf.rs // Ausgabe der Markierungen
  tests/common/{dim_model.rs, svg_marks.rs (Leser-Erweiterung), dxf_marks.rs}  // eigene Prüfwerkzeuge
  tests/stage12_dimensions.rs ... stage18_*.rs
crates/planora-feature/src/georef.rs     // GeoRefData ⇄ EdgeRef/FaceRef, resolve_geo, geo_ref_for_edge/face
crates/planora-feature/src/dimensions.rs // Document-Operationen für Maße, Validierung, Snapshot
crates/planora-project/                  // Format 5
crates/planora-app/src/dimension_state.rs  // Auflösen, Zwischenspeicher, Meldungen
crates/planora-app/src/dimension_tool.rs   // Werkzeug, Vorschau
crates/planora-app/src/dimension_dialog.rs // Maß bearbeiten
crates/planora-app/src/dimension_browser.rs // Browser-Abschnitt „Maße“
```

---

### Task 1: Datentypen für Maße und Mittellinien

**Files:**
- Create: `crates/planora-drawing/src/dimension.rs` (nur Typen und `DimError`), `tests/stage12_types.rs`
- Modify: `src/model.rs` (`ViewSpec.centerlines`, `Drawing.dimensions`, `Drawing.next_dim`, Hilfsfunktionen), `src/lib.rs`, `src/limits.rs` (`MAX_DIMENSIONS`)

**Interfaces:**
- Produces (`dimension.rs`):
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct DimId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum DimKind { Horizontal, Vertical, Aligned, Diameter, Radius, Angle }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum EndSel { Start, End }
/// Verweis auf Modellgeometrie ohne Abhängigkeit von planora-feature (P2).
#[derive(Clone, Debug, PartialEq)] pub struct TagData { pub origin: u64, pub role: String, pub hint: DVec3 }
#[derive(Clone, Debug, PartialEq)] pub enum GeoRefData {
    Face(TagData),                                   // Fläche (Zylinder, Kugel, Ebene …)
    Edge { a: TagData, b: TagData, hint: DVec3 },    // Kante über die zwei angrenzenden Flächen
}
#[derive(Clone, Debug, PartialEq)] pub enum GeoRef { Edge(GeoRefData), EdgeEnd { edge: GeoRefData, end: EndSel },
    Center(GeoRefData), Face(GeoRefData) }
#[derive(Clone, Debug, PartialEq)] pub enum Tolerance { None, Symmetric(f64), Limits { upper: f64, lower: f64 },
    LimitSizes { upper: f64, lower: f64 }, Fit(String) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum ChainMode { Chain, Baseline }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct ChainSpec { pub group: u64, pub mode: ChainMode, pub order: u32 }
#[derive(Clone, Debug, PartialEq)] pub struct DimSpec { pub id: DimId, pub view: ViewId, pub kind: DimKind,
    pub refs: Vec<GeoRef>, pub offset: f64, pub angle: f64 /*Grad, nur Ø/R-Führungslinie*/, pub text_shift: (f64, f64),
    pub tolerance: Tolerance, pub decimals: u8, pub text_override: Option<String>, pub chain: Option<ChainSpec>,
    pub suggested: bool }
pub enum Geo3 { Point(DVec3), Line { a: DVec3, b: DVec3 }, Circle { center: DVec3, normal: DVec3, radius: f64 },
    Axis { origin: DVec3, dir: DVec3, radius: f64 }, Sphere { center: DVec3, radius: f64 } }   // Clone, Debug, PartialEq
#[derive(Debug, thiserror::Error, Clone, PartialEq)] pub enum DimError {
    #[error("Die Kante steht nicht quer zum Maß.")] NotAcross,
    #[error("Der Bezug ist in dieser Ansicht nicht zu sehen.")] NotVisible,
    #[error("Der Bezug ist nicht eindeutig.")] Ambiguous,
    #[error("Der Bezug wurde nicht gefunden: {0}")] Lost(String),
    #[error("In der Isometrie gibt es keine Maße.")] NotAllowedInIsometric,
    #[error("Es gibt zu viele Maße in dieser Zeichnung.")] TooMany,
    #[error("Das Maß ist ungültig: {0}")] Invalid(String) }
```
  `ref_count(kind) -> (usize, usize)` (Mindest-/Höchstzahl der Bezüge): Horizontal/Vertical/Aligned 2 (Kette: je Kettenglied 2), Diameter 1, Radius 1, Angle 2.
- Produces (`model.rs`): `ViewSpec { …, pub centerlines: bool }` (Vorgabe in `Drawing::add_view`: `kind.is_normal()`); `Drawing { …, pub dimensions: Vec<DimSpec>, pub next_dim: u64 }` (`Drawing::new`: leer, `next_dim = 1`); `Drawing::add_dimension(&mut self, spec_without_id: DimSpec) -> DimId` (vergibt Kennung), `Drawing::dimension(&self, DimId) -> Option<&DimSpec>`, `Drawing::remove_dimension(&mut self, DimId) -> bool`, und `Drawing::remove_view` entfernt zusätzlich alle Maße der entfernten Ansichten (Rückgabe unverändert: Ansichtskennungen).
- Produces (`limits.rs`): `pub const MAX_DIMENSIONS: usize = 2000;`.

- [ ] **Step 1: Failing tests** (`tests/stage12_types.rs`): (a) `add_view_sets_centerlines_by_kind` (alle 7 `ViewKind`: `true` für die sechs Normansichten, `false` für Isometrisch); (b) `add_and_remove_dimension_assigns_increasing_ids` (Kennungen 1, 2, 3, nie wiederverwendet; `remove_dimension` bei unbekannter Kennung → `false`); (c) `removing_a_view_removes_its_dimensions` (Vorne mit zwei Maßen, Oben mit einem; Löschen von Vorne entfernt zwei, das dritte bleibt; abgeleitete Ansicht mit Maß verschwindet mit dem Elternteil); (d) `ref_counts_match_the_kinds`; (e) `dim_error_texts_are_german_and_plain` (jede Variante: Text endet mit Punkt, enthält kein englisches Wort aus einer Sperrliste (`view`, `sheet`, `error`, `dimension`)); (f) `drawing_equality_includes_dimensions`.
- [ ] **Step 2: Run → FAIL** (`cargo test -p planora-drawing --test stage12_types`). **Step 3: Implementieren**; alle **bestehenden** Konstruktoren/Literale von `ViewSpec`/`Drawing` im Arbeitsbereich mechanisch ergänzen (`cargo build --workspace --all-targets -j 2` zeigt sie: `planora-feature`, `planora-project`, `planora-app`, Tests). **Step 4: Run → PASS**, alle bestehenden Pakete weiter grün (`cargo test -p planora-feature -p planora-project -j 2`; App-Tests nur kompilieren: `cargo test -p planora-app --no-run -j 2`), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** `remove_view` ohne Maße-Entfernung → (c) rot; `centerlines` immer `true` → (a) rot.
- [ ] **Step 6: Commit** — `feat: Datentypen für Maße und Mittellinien`.

---

### Task 2: Messung – Werte und Anker aller Maßarten

**Files:**
- Create: `crates/planora-drawing/src/dimension.rs` (Erweiterung: `measure`), `tests/common/dim_model.rs`, `tests/stage13_measure.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `Basis` (Aufgabe 1 von TP11), `Geo3`, `DimKind`, `DimError`, `limits::geom_tol`.
- Produces:
```rust
/// Ergebnis einer Messung in **Ansichtskoordinaten** (ungeskaliert, mm; y nach oben).
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    /// Zwei Messpunkte und die Messrichtung (Einheitsvektor): Horizontal/Vertical/Aligned und Ø/R quer.
    Linear { a: DVec2, b: DVec2, dir: DVec2 },
    /// Kreis mit Blick in Achsrichtung: Mittelpunkt und Radius (Ø und R mit Führungslinie).
    Round { center: DVec2, radius: f64 },
    /// Winkel zwischen zwei Geraden: Scheitel, zwei Einheitsrichtungen vom Scheitel weg.
    Angle { vertex: DVec2, d1: DVec2, d2: DVec2 },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Measured { pub value: f64 /*mm, Winkel in Grad*/, pub anchor: Anchor, pub prefix: Option<char> /*'Ø' oder 'R'*/ }
pub fn measure(kind: DimKind, geos: &[Geo3], basis: &Basis, view_kind: ViewKind, extent: f64) -> Result<Measured, DimError>;
```
  Regeln (P4, P5): `view_kind == Isometric` → `Err(NotAllowedInIsometric)`. Projektion: `proj(p) = basis.project(p)`. Messrichtung `u = ex` (Horizontal), `ey` (Vertical), `normalize(b − a)` (Aligned; Punkte). Je Bezug wird die **Lage entlang `u`** bestimmt: `Point` → `p·u`; `Line { a, b }` → muss quer stehen (`|dir·u| ≥ 1 − 1e-6` im Bild? nein: quer = Richtung der **projizierten** Kante steht senkrecht zu `u`: `|proj_dir·u| ≤ 1e-6·|proj_dir|`), sonst `NotAcross`; ist die Kante parallel zur Blickrichtung (projiziert Länge 0), ist sie ein Punkt; `Circle` (Mitte) → `center·u`; `Axis` → Lage der projizierten Achse quer zu `u` (sonst `NotAcross`); `Sphere` → Mitte. Wert = `|lage_b − lage_a|`; zwei **gleiche** Lagen (Wert < `geom_tol(extent)`) → `Err(Invalid("Das Maß hat die Länge null."))`. `Aligned` mit zwei Punkten (nur `Point`) oder zwei parallelen Kanten (Abstand der Geraden im Bild). `Diameter`/`Radius`: `Circle` mit `|normal·d| ≥ 1 − 1e-9` → `Anchor::Round` (Wert `2r` bzw. `r`, Vorsatz 'Ø'/'R'); `Circle` mit `|normal·d| ≤ 1e-9` (Kante von der Seite) → `Anchor::Linear` quer zur projizierten Kreislinie (Wert `2r`, Vorsatz 'Ø', bei Radius `NotAcross`); `Axis` (Zylinder) mit `|dir·d| ≤ 1e-9` → `Anchor::Linear` quer zur projizierten Achse (Wert `2·radius`, Vorsatz 'Ø'); `Sphere` → `Round` mit Mitte `proj(center)` (Wert `2r`/`r`); alles andere → `NotAcross` (Text „Der Kreis erscheint schräg.“ über `Invalid`). `Angle`: zwei `Line`: projizierte Geraden schneiden, Scheitel, Winkel zwischen den **Richtungen vom Scheitel weg zu den näher am Scheitel liegenden Enden** (0°–180°, spitzer oder stumpfer wird durch die Lage der Enden bestimmt); parallele Geraden → `Invalid`.
- [ ] **Step 1: Prüfwerkzeug** (`tests/common/dim_model.rs`): Erwartungsrechner für Quader/Zylinder/Kugel/Winkelprofil nur aus den Maßen: `box_dims(a,b,c)`, die Geo3-Beschreibungen der Testkörper **von Hand** (Kanten des Quaders als `Geo3::Line` mit den bekannten Endpunkten, Kreise mit Mitte und Normale): **nicht** über die Auflösung aus `planora-feature` (die kommt in Aufgabe 8). Zufallszahlen: eigener kleiner Generator wie in TP11.
- [ ] **Step 2: Failing tests** (`tests/stage13_measure.rs`): (a) `horizontal_between_two_vertical_edges_front_view` (Quader 10×20×30 vorne: Kanten bei x=0 und x=10 → Wert 10, Anker `Linear` mit `dir = (1,0)`); (b) `vertical_between_two_horizontal_edges_front_view` (Wert 30); (c) `depth_in_top_view_uses_the_y_axis` (Oben: vertikal = 20); (d) `horizontal_between_a_vertical_and_a_slanted_edge_is_not_across` → `NotAcross` (Kante in Ansicht schräg); (e) `aligned_between_two_points_is_euclidean` (3-4-5 → 5); (f) `diameter_of_a_circle_seen_along_its_axis_is_a_round_anchor` (Oben, Zylinder r=5: Wert 10, `Round`, Vorsatz Ø); (g) `radius_of_a_circle_seen_along_its_axis` (5, 'R'); (h) `diameter_of_a_cylinder_axis_across_view_is_linear_across_the_axis` (Vorne, Zylinder r=5 h=12 senkrecht: Wert 10, `Linear` quer zur Achse); (i) `circle_seen_from_the_side_gives_diameter_but_not_radius` (Ø ok, R → `NotAcross`/`Invalid`); (j) `oblique_circle_is_refused`; (k) `sphere_diameter_in_any_normal_view` (r=7 → 14); (l) `angle_between_two_edges_of_an_l_profile` (90°) und eine 45°-Fase (45°) mit Scheitelpunkt-Test; (m) `parallel_lines_for_an_angle_are_invalid`; (n) `zero_length_is_invalid`; (o) `isometric_is_refused_for_every_kind`; (p) `all_normal_views_of_a_box` (6 Ansichten × passende Maße: Wert = Eingabemaß, bei 200 Zufallsquadern 0,5-mm-Raster, Verschiebung 1e6 und Maßstab-unabhängig: gleicher Wert bei 1e-1·/1e5·-Skalierung relativ `1e-9`); (q) `value_does_not_depend_on_the_sheet_scale` (nur Basis, kein Maßstab im Aufruf – Typ-Test, Dokumentation).
- [ ] **Step 3: Run → FAIL. Step 4: Implementieren. Step 5: Run → PASS**, clippy, fmt.
- [ ] **Step 6: Mutationsbeweise:** Quer-Prüfung weglassen → (d) rot; `Ø` und `R` vertauschen → (f)/(g) rot; Isometrie-Sperre entfernen → (o) rot.
- [ ] **Step 7: Commit** — `feat: Messung der Maßarten mit Ankern`.

---

### Task 3: Mittellinien

**Files:**
- Create: `crates/planora-drawing/src/centerline.rs`, `tests/stage14_centerlines.rs`
- Modify: `src/lib.rs` (`ViewLines.centerlines`, Aufruf in `compute_view`), `tests/common/model.rs` (Hilfskörper)

**Interfaces:**
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CenterLine { pub a: DVec2, pub b: DVec2, pub source: CenterSource }   // Ansichtskoordinaten (mm, ungeskaliert), OHNE Überstand
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CenterSource { Circle { body: usize, edge: usize }, Axis { body: usize, face: usize }, Sphere { body: usize, face: usize }, Torus { body: usize, face: usize } }
pub fn centerlines(bodies: &[DrawBody], basis: &Basis, extent: f64, progress: &Progress) -> Result<Vec<CenterLine>, DrawError>;
```
  `ViewLines` bekommt `centerlines: Vec<CenterLine>`; `compute_view` füllt sie (P8) und zählt sie in `bbox` **nicht** mit. Der Überstand (`CENTER_OVERSHOOT` Blatt-mm je Seite) wird erst beim Aufbau des Blatts (`layout_centerline`, Aufgabe 4) addiert, weil er in Blatt-mm gilt und die Ansichtskoordinaten ungeskaliert sind.
  Regeln: Je **Kreiskante** (volle Kreise oder Bögen mit Mittelpunkt) mit `|normal·d| ≥ 1 − 1e-9` → Achsenkreuz: zwei Linien durch den projizierten Mittelpunkt, waagerecht und senkrecht in Ansichtskoordinaten, Halblänge = Radius. Je **Zylinder-/Kegelfläche** mit Achse ⟂ Blickrichtung (`|dir·d| ≤ 1e-9`): eine Linie entlang der projizierten Achse zwischen den Enden der Fläche (Höhenbereich der Fläche in der Ansicht). Je **Kugel**: Achsenkreuz am projizierten Mittelpunkt (Halblänge = Radius). **Torus** mit Achse ∥ Blickrichtung: Achsenkreuz (Halblänge = großer + kleiner Radius). Gleiche Linien (Ort und Richtung innerhalb `geom_tol`) und konzentrische Kreise ergeben **ein** Kreuz. Nur Kanten, die Kreise/Bögen mit Mittelpunkt sind (Linien, Ellipsen, Splines nicht). In der Isometrie liefert die Regel keine Linien (Achsen/Kreise stehen schräg).
- [ ] **Step 1: Failing tests** (`tests/stage14_centerlines.rs`): (a) `cylinder_top_view_has_one_cross_at_the_center` (zwei Kreise übereinander → ein Kreuz, Halblänge 5, Mitte (0,0)); (b) `cylinder_front_view_has_one_axis_line` (Länge 12, x = 0); (c) `sphere_has_a_cross_in_every_normal_view`; (d) `box_has_no_centerlines`; (e) `hole_along_y_front_view_has_a_cross_at_the_hole_center` (Quader 40×20×30 mit Ø10-Loch längs Y bei (20, ·, 15): Kreuz bei (20,15), Halblänge 5; von oben: Achsenlinie des Lochs bei x = 20, Länge 20); (f) `concentric_circles_give_one_cross` (Zylinder mit Sackloch gleiche Achse); (g) `torus_top_view_cross_half_length_is_major_plus_minor`; (h) `isometric_has_centerlines_computed_but_flagged_off_in_the_sheet` (Berechnung liefert Linien für Zylinder in der Isometrie nicht, weil Achse/Kreis schräg: erwartet **keine** Mittellinien in der Isometrie, Test dokumentiert die Regel); (i) `half_cylinder_and_arcs_use_the_arc_center` (Bogenkanten: Kreuz am Mittelpunkt, Halblänge = Radius); (j) `values_scale_with_the_body_and_survive_far_translation` (1e5·, 1e-1·, Verschiebung 1e6); (k) `series_of_random_bodies_no_panic_and_centerlines_lie_on_the_axes` (200 Körper wie TP11-Reihe: jede Linie liegt auf einer Kreis-/Zylinderachse laut **eigener** Rechnung aus den Erzeugungsdaten, Abstand ≤ `geom_tol`); (l) `compute_view_bbox_ignores_centerlines`.
- [ ] **Step 2–4:** FAIL → Implementieren → PASS (Release für die Reihe), clippy, fmt. **Step 5: Mutationsbeweise:** Konzentrik-Zusammenfassung weglassen → (f) rot; Achsenrichtungs-Prüfung (`|dir·d|`) umdrehen → (b)/(e) rot. **Step 6: Commit** — `feat: Mittellinien aus dem Modell`.

---

### Task 4: Darstellung als Markierungen (`annotate.rs`) und `build_sheet_with`

**Files:**
- Create: `crates/planora-drawing/src/annotate.rs`, `tests/common/mark_check.rs`, `tests/stage15_annotate.rs`
- Modify: `src/sheet.rs` (`SheetLine.origin`, `SheetView.marks`, `build_sheet_with`), `src/lib.rs`

**Interfaces:**
- Produces:
```rust
pub enum Mark {
    Line { a: DVec2, b: DVec2, kind: MarkKind },                  // Blattkoordinaten
    Arrow { tip: DVec2, dir: DVec2 /*Einheitsvektor vom Pfeil weg zur Maßlinie*/, kind: MarkKind },
    Arc { center: DVec2, r: f64, start: f64, end: f64, kind: MarkKind },   // Winkelmaß (ccw, Radiant)
    Text { pos: DVec2 /*Anker*/, height: f64, angle: f64 /*Radiant*/, anchor: TextAnchor, text: String, kind: MarkKind },
}
pub enum MarkKind { Dimension, CenterLine, Lost }   // Lost: rot
pub enum TextAnchor { Start, Middle, End }
pub struct ResolvedDim { pub spec_id: DimId, pub result: Result<Measured, DimError>, pub last_known: Option<Anchor> }
pub struct ResolvedDims { pub by_view: BTreeMap<ViewId, Vec<(DimSpec, Result<Measured, DimError>)>> }
pub fn format_value(value: f64, decimals: u8) -> String;                 // "12,5", "−0,5"? (nur Zahl, ohne Zeichen)
pub fn dimension_text(spec: &DimSpec, m: &Measured) -> DimText;          // Haupttext + optional Toleranz-Zeilen
pub fn layout_dimension(spec: &DimSpec, m: &Measured, xf: &SheetTransform, group_index: usize) -> Vec<Mark>;
pub fn layout_lost(spec: &DimSpec, last: Option<&Anchor>, xf: &SheetTransform, reason: &str) -> Vec<Mark>;
pub fn layout_centerline(line: &CenterLine, xf: &SheetTransform) -> Vec<Mark>;
```
  `SheetLine { vis, curve, origin: StrokeOrigin }`; `SheetView { …, marks: Vec<Mark> }`; `build_sheet_with(drawing, results, dims: &ResolvedDims) -> SheetModel` (Mittellinien nur bei `spec.centerlines`; Maße der Ansicht aus `dims.by_view`; Marken liegen in `SheetView.marks`; `build_sheet` ruft `build_sheet_with` mit leeren Maßen **und ohne** Mittellinien-Aufnahme bei `centerlines == false`).
  **Layout-Regeln (alle Blatt-mm, `xf` bildet Ansichts- auf Blattkoordinaten ab):**
  - **Linear waagerecht** (`dir = (1,0)`, Punkte `A`, `B` in Blattkoordinaten, `A.x ≤ B.x` nach Sortierung): Bezugshöhe `y0 = max(A.y,B.y)` bei `offset ≥ 0`, sonst `min(A.y,B.y)`; Maßlinie bei `y = y0 + offset`. Hilfslinien: von `P.y + s·WITNESS_GAP` bis `y + s·WITNESS_OVER` je `P ∈ {A,B}` (`s = ±1`: Richtung zur Maßlinie, wenn die Maßlinie auf der Seite des Punktes liegt; liegt der Punkt zwischen Bezug und Maßlinie, entfällt keine Hilfslinie, sie reicht über die Maßlinie hinaus). Pfeile: Spitzen bei `(A.x, y)` und `(B.x, y)`, innen (`dir` zeigt zur Maßlinie hin ausgerichtet nach außen: Spitze zeigt zur Hilfslinie), wenn `B.x − A.x ≥ 2·ARROW_LEN + 1`; sonst außen (Pfeile von außen auf die Hilfslinien, Maßlinie um 5 mm verlängert). Maßlinie von Spitze zu Spitze. Text: `Middle`-Anker bei `((A.x+B.x)/2, y + TEXT_GAP) + text_shift`, Höhe `DIM_TEXT_H`, Winkel 0; passt der Text nicht zwischen die Pfeile (`Textbreite > B.x − A.x − 2·ARROW_LEN`), steht er rechts von `B.x + ARROW_LEN + 2` (Anker `Start`).
  - **Linear senkrecht:** gedreht um 90° (Text Winkel `π/2`, liest von rechts); **Aligned:** gedreht auf die Richtung `B − A` (Maßlinie um `offset` senkrecht versetzt); Text immer lesbar (Winkel in `(−π/2, π/2]` normalisiert).
  - **Round (Ø/R):** `center, r`; Richtung `u = (cos θ, sin θ)`, `θ = spec.angle` Grad; Pfeilspitze bei `center + u·r` (Pfeil zeigt zum Kreis hin), Führungslinie von dort nach `center + u·(r + offset)`, danach 4 mm waagerecht (Schulter) in Richtung `sign(u.x)`; Text am Ende (`Start`/`End` je Seite) mit Vorsatz `Ø`/`R`. Für `Diameter` bei **großen** Kreisen (`2r·scale ≥ 25 mm`) zusätzlich zweiter Pfeil gegenüber (Linie durch die Mitte): Maßlinie von `center − u·r` bis `center + u·r`, Pfeile innen, Text auf der Linie bei `Middle`.
  - **Angle:** Bogen um `vertex` mit Radius `offset` zwischen `d1` und `d2` (kürzere Bogenrichtung), Hilfslinien entlang `d1`,`d2` bis `offset + WITNESS_OVER`, Pfeile tangential an den Bogenenden, Text bei der Bogenmitte außen.
  - **Kette/Bezug:** `group_index` verschiebt `offset` bei `Baseline` um `order·DIM_STEP` (`order` aus `ChainSpec`); `Chain`: gleiche Maßlinie, Text je Glied.
  - **Toleranz:** `Symmetric(t)` → `Haupttext ± t` hinter dem Wert (`12,5 ±0,1`); `Limits` → zweizeilig hinter dem Wert (obere/untere Zahl mit Vorzeichen, Höhe `0,7·DIM_TEXT_H`); `LimitSizes` → Text `Höchst-/Mindestmaß` (zwei Zeilen `12,6` / `12,4`, ohne Nennmaß); `Fit(s)` → `Haupttext s`; `text_override` ersetzt den Haupttext vollständig (Vorsatz bleibt, wenn der Text mit `<>` beginnt: `<>` = berechneter Wert).
  - **Verloren:** `layout_lost` zeichnet in `MarkKind::Lost` die letzte bekannte Maßlinie (falls `last_known`) und den Text `?` plus Grund (kurz) an der Maßlinie; ohne `last_known` an der Mitte der Ansicht (kleines `?`-Zeichen).
  - **Mittellinien:** `layout_centerline` verlängert die Linie auf beiden Seiten um `CENTER_OVERSHOOT` (Blatt-mm, nach der Abbildung) → `Mark::Line { kind: CenterLine }`.
- [ ] **Step 1: Prüfwerkzeug** (`tests/common/mark_check.rs`): `bbox_of_marks`, `find_arrows`, `line_through(point)`, Abstandsrechner – unabhängig geschrieben.
- [ ] **Step 2: Failing tests** (`tests/stage15_annotate.rs`, Zahlen von Hand ausgerechnet): (a) `horizontal_dimension_geometry` (A=(20,50), B=(50,50) Blatt, offset 10 → Maßlinie y=60; Hilfslinien von y=51 bis y=62 bei x=20 und 50; Pfeilspitzen (20,60) und (50,60) mit `dir` nach innen; Text `10,0`-Entsprechung mittig (35, 61), Höhe 3,5); (b) `short_dimension_puts_arrows_outside` (B.x − A.x = 6 mm → Pfeile außen, Maßlinie 5 mm verlängert); (c) `text_that_does_not_fit_moves_right` (Textbreite 0,6·3,5·7 = 14,7 > 4 → Text rechts von B); (d) `vertical_dimension_rotates_text_90_degrees`; (e) `aligned_dimension_follows_the_direction` (3-4-5-Dreieck, offset 8: Maßlinie parallel, Abstand 8); (f) `negative_offset_puts_the_line_on_the_other_side`; (g) `diameter_leader_default_45_degrees` (Kreis (100,100) r=10: Pfeilspitze (107,071…,107,071), Führungslinie bis `center + u·(r+offset)`, Schulter 4 mm); (h) `large_diameter_gets_a_through_line_with_two_arrows` (Kreis 30 mm auf dem Blatt); (i) `radius_text_has_r_prefix`; (j) `angle_arc_between_two_lines` (90° bei Scheitel (10,10), offset 15: Bogen r=15 von 0 bis π/2, Text bei 45°); (k) `baseline_group_stacks_by_7_mm`; (l) `chain_shares_the_dimension_line`; (m) `tolerance_texts` (Symmetric 0,1 → `12,5 ±0,1`; Limits +0,2/−0,1 → zwei Zeilen mit Höhe 2,45; LimitSizes; Fit `Ø10 H7`; override mit `<>`); (n) `format_value_rounds_and_uses_the_german_comma` (0,05→`0,1` Rundung Mathematisch? festlegen: kaufmännisch **nicht** — `format!`-Rundung des Rust-Standards (Rundung zur nächsten geraden bei exaktem Halbwert); Test dokumentiert die Wahl; `-0,04` mit `decimals 1` → `0,0` ohne Minus), (o) `lost_dimension_is_red_with_a_question_mark_and_reason`; (p) `centerline_is_extended_by_3_mm` und liegt in `MarkKind::CenterLine`; (q) `scale_independence` (gleiche Marken bei Maßstab 1:1 und 1:2 für gleiche **Blatt**-Punkte; Wert ändert sich nur durch den Wert-Text: Aufruf mit anderen Ankern); (r) `build_sheet_with_adds_marks_only_for_views_with_centerlines_flag` und `build_sheet_without_dims_has_no_marks_for_flagged_off_views`; (s) `old_sheet_construction_is_unchanged` (Ansicht ohne Mittellinien und Maße: `SheetModel.views[i].lines` und `extent_mm` wie vor der Änderung, Marken leer).
- [ ] **Step 3: Run → FAIL. Step 4: Implementieren. Step 5: Run → PASS**, clippy, fmt (bestehende Stufen 1–11 der TP11-Tests laufen weiter; `SheetLine`-Literale in Tests mechanisch ergänzen).
- [ ] **Step 6: Mutationsbeweise:** Pfeilrichtung umdrehen → (a) rot; Textbreitenregel weglassen → (c) rot; `Baseline`-Versatz weglassen → (k) rot.
- [ ] **Step 7: Commit** — `feat: Maße und Mittellinien als Markierungen auf dem Blatt`.

---

### Task 5: SVG – Maße und Mittellinien

**Files:**
- Create: `crates/planora-drawing/tests/fixtures/svg_without_marks_*.svg` (Goldene Dateien **vor** der Änderung mit dem alten Schreiber erzeugt), `tests/common/svg_marks.rs`, `tests/stage16_svg_marks.rs`
- Modify: `crates/planora-drawing/src/svg.rs`, `tests/common/svg_read.rs` (Leser um `<text>`, `<polygon>`/gefüllte `path`, Gruppen erweitern)

**Interfaces:**
- Consumes: `SheetModel` mit `SheetView.marks` (Aufgabe 4).
- Produces: `write_svg`/`svg_string` schreiben je Ansicht, **nur wenn Markierungen vorhanden sind**, zwei weitere Gruppen nach der Liniengruppe der Ansicht: `<g id="{Ansichtsname} Maße">` (Marken `Dimension` und `Lost`) und `<g id="{Ansichtsname} Mittellinien">` (Marken `CenterLine`). Ansichten ohne Marken erzeugen **keine** zusätzlichen Zeilen: die Ausgabe bleibt Byte für Byte gleich (goldene Dateien).
  - `Mark::Line` → `<path d="M x y L x y" stroke="#000" stroke-width="0.25" fill="none" stroke-linecap="round"/>`; Mittellinien zusätzlich `stroke-dasharray="12 3 2 3"`; `Lost` → `stroke="#c00"`.
  - `Mark::Arrow` → gefülltes Dreieck: `<path d="M tip L base+perp·ARROW_HALF_W L base−perp·ARROW_HALF_W Z" fill="#000" stroke="none"/>`, `base = tip + dir·ARROW_LEN`.
  - `Mark::Arc` → `<path d="M … A r r 0 large sweep … "/>` (gleiche Sweep-Regel wie die vorhandenen Bögen: 1 für gegen den Uhrzeigersinn im y-aufwärts-Blattsystem innerhalb der gespiegelten Gruppe).
  - `Mark::Text` → `<text transform="translate(x y) scale(1 -1) rotate(-{grad})" font-family="sans-serif" font-size="{height}" text-anchor="start|middle|end" fill="#000">…</text>` (die Wurzelgruppe spiegelt y: das `scale(1 -1)` hebt die Spiegelung für den Text auf; Winkel in Grad, positiv gegen den Uhrzeigersinn auf dem Blatt). Text maskiert (`& < > " '`), `Ø`, `°`, `±`, `−` als Unicode; gestapelte Toleranztexte sind zwei `<text>`-Elemente (Haupttext und Toleranzzeilen sind getrennte `Mark::Text`). Zahlen höchstens 6 Nachkommastellen, wie die übrigen.
- [ ] **Step 1: Goldene Dateien erzeugen (vor jeder Änderung an `svg.rs`):** kleiner Test/Beispiel im Arbeitsbaum erzeugt aus `build_sheet` zwei Blätter (Quader mit drei Ansichten; Zylinder mit Isometrie) mit dem **aktuellen** Schreiber nach `tests/fixtures/svg_without_marks_{box,cylinder}.svg`; **eigener Commit** („test: Goldene SVG-Dateien ohne Marken (alter Schreiber)“) mit `.gitattributes` `eol=lf` für `*.svg`-Fixturen.
- [ ] **Step 2: Failing tests** (`tests/stage16_svg_marks.rs`): (a) `old_sheets_stay_byte_for_byte` (zwei Blätter über `build_sheet_with` ohne Maße, mit `centerlines=false`: Ausgabe == Fixtur); (b) `centerlines_are_written_dashed_in_their_own_group`; (c) `dimension_roundtrip_geometry` (Marken aus Aufgabe 4 für ein waagerechtes Maß → SVG → eigener Leser → Linien/Pfeile/Text an den erwarteten Blattkoordinaten, y-Orientierung geprüft, Text liest sich aufrecht: Leser rechnet die Transformationsmatrix durch und prüft, dass die Textachse der Blatt-x-Achse folgt); (d) `rotated_text_for_vertical_dimension` (Winkel 90°: Leser findet Textrichtung nach oben); (e) `special_characters` (Text `Ø10 H7 ±0,1 − & <x> "q"` → maskiert, Leser dekodiert gleich); (f) `lost_marks_are_red`; (g) `arc_and_arrow_roundtrip` (Winkelmaß); (h) `fill_arrows_are_triangles_with_expected_size` (Länge 3,5, Breite 1,2); (i) `groups_have_unique_valid_ids_with_odd_view_names`; (j) `non_finite_marks_refuse_with_nonfinite_error`; (k) `a0_sheet_and_large_numbers`.
- [ ] **Step 3: Run → FAIL. Step 4: Implementieren. Step 5: Run → PASS**, clippy, fmt.
- [ ] **Step 6: Mutationsbeweise:** Gegenspiegelung des Texts weglassen → (c)/(d) rot (Text steht spiegelverkehrt); Maskierung weglassen → (e) rot; Gruppen auch ohne Marken schreiben → (a) rot.
- [ ] **Step 7: Commit** — `feat: SVG-Ausgabe von Maßen und Mittellinien`.

---

### Task 6: DXF – Maße und Mittellinien

**Files:**
- Create: `crates/planora-drawing/tests/fixtures/dxf_without_marks_*.dxf` (goldene Dateien **vor** der Änderung), `tests/stage17_dxf_marks.rs`
- Modify: `crates/planora-drawing/src/dxf.rs`, `tests/common/dxf_read.rs` (Leser/Validator), `docs`-Hinweise in Doku-Kommentaren

**Interfaces:**
- Produces: Ebenen `MASSE` (Linientyp `CONTINUOUS`, Farbe 7, Linienstärke 25) und `MITTEL` (Linientyp `CENTER`, Farbe 7, Stärke 25) und `MASSE_FEHLT` (Farbe 1 = rot, `CONTINUOUS`) **nur wenn Marken dieser Art vorhanden sind** (alte Ausgaben bleiben Byte für Byte gleich: Tabellenzähler `70`, Handles unverändert, wenn keine Marken). Linientyp `CENTER` in der `LTYPE`-Tabelle: `72 65`, `73 4`, `40 20`, `49 12`, `74 0`, `49 -3`, `74 0`, `49 2`, `74 0`, `49 -3`, `74 0` (nur wenn `MITTEL` vorkommt). Objekte im Block der Ansicht: `Mark::Line` → `LINE`; `Mark::Arrow` → `SOLID` (Dreieck: Ecken in der DXF-Reihenfolge 1, 2, 4, 3 mit gleicher dritter und vierter Ecke; Subklasse `AcDbTrace`, Dicke 0); `Mark::Arc` → `ARC` (Winkel in Grad, normalisiert wie bisher); `Mark::Text` → `TEXT` (`100 AcDbText`, Einfügepunkt `10/20/30`, Höhe `40`, Text `1`, Drehung `50` in Grad, Textstil `7 STANDARD`, Ausrichtung `72` = 0/1/2 für Start/Mitte/Ende mit zweitem Ausrichtepunkt `11/21/31` = Einfügepunkt, danach zweite Subklasse `100 AcDbText` und `73 0`); Text mit `Ø`→`%%c`, `°`→`%%d`, `±`→`%%p`, `−`→`-`, nicht-Latin-1 → `\U+XXXX`; Länge höchstens 255 Zeichen (längere Texte: Fehler `DrawError::Unsupported("Der Maßtext ist zu lang.")`).
- Validator (`dxf_read`): `TEXT`, `SOLID` einlesbar; Ebenen/Linientypen der Objekte existieren; `CENTER` nur mit Muster; Handles/Besitzer wie bisher.
- [ ] **Step 1: Goldene Dateien** (wie Aufgabe 5, `dxf_without_marks_{box,cylinder}.dxf`, vor der Änderung, eigener Commit, `eol=lf`? DXF verwendet `\r\n` – Fixtur mit `-text`, binär behandeln: `.gitattributes` `*.dxf -text` für diese Fixturen).
- [ ] **Step 2: Failing tests** (`tests/stage17_dxf_marks.rs`): (a) `old_sheets_stay_byte_for_byte`; (b) `centerline_layer_and_linetype_present_only_when_used` (Strukturprüfung inkl. `LTYPE`-Zähler); (c) `dimension_entities_roundtrip` (LINE/SOLID/TEXT/ARC an den erwarteten Koordinaten; Drehwinkel, Höhe, Ausrichtung); (d) `solid_triangle_vertex_order` (Ecken 1,2,4,3 mit gleicher 3./4. Ecke); (e) `text_code_conversion` (`Ø10,0` → `%%c10,0`, `90°` → `%%d`-Entsprechung, `±` → `%%p`, `−` → `-`); (f) `lost_marks_go_to_a_red_layer`; (g) `validator_rejects_corrupted_mark_structures` (TEXT ohne Ebene, SOLID mit drei Ecken, CENTER ohne Muster, doppelte Handles); (h) `very_long_text_is_refused_by_name`; (i) `a0_and_far_coordinates`; (j) `hand_check_list_documented` (Test prüft, dass das Doku-Kommentar von `dxf.rs` den neuen Abschnitt über `TEXT`/`SOLID`/`CENTER` nennt – leichtes Wartungs-Netz).
- [ ] **Step 3: Run → FAIL. Step 4: Implementieren. Step 5: Run → PASS**; zusätzlich **externe Prüfung** (nicht im Repo): `D:\Testumgebung\dxf-debug\venv\Scripts\python.exe D:\Testumgebung\dxf-debug\chk.py <Beispieldatei>` auf einer neuen Datei mit Maßen (readfile OK, Audit 0), Ausgabe in den Bericht. clippy, fmt.
- [ ] **Step 6: Mutationsbeweise:** Reihenfolge der SOLID-Ecken vertauschen → (d) rot; Textkürzel weglassen → (e) rot; CENTER-Muster weglassen → (b)/(g) rot.
- [ ] **Step 7: Commit** — `feat: DXF-Ausgabe von Maßen und Mittellinien`.

---

### Task 7: Vorschlag der Grundmaße

**Files:**
- Create: `crates/planora-drawing/src/suggest.rs`, `tests/stage18_suggest.rs`
- Modify: `src/lib.rs`, `tests/common/dim_model.rs`

**Interfaces:**
- Consumes: `ViewLines`, `Measured`/`measure` (Aufgabe 2), `Geo3`.
- Produces:
```rust
/// Kandidat für ein Maß: beschreibt die Bezüge als Geo3 (zum Messen) UND die Herkunft (zum späteren Verweisbau durch die Oberfläche).
pub struct Candidate { pub kind: DimKind, pub view: ViewId, pub geos: Vec<Geo3>, pub origins: Vec<CandidateOrigin>, pub key: String }
pub enum CandidateOrigin { Edge { body: usize, edge: usize }, EdgeEnd { body: usize, edge: usize, end: EndSel }, Center { body: usize, edge: usize }, Face { body: usize, face: usize } }
pub fn suggest(bodies: &[DrawBody], views: &[(ViewSpec, &ViewLines)], existing: &[(ViewId, DimKind, String /*key*/)]) -> Vec<Candidate>;
```
  `key`: stabile Zeichenkette des Maßes nach **Bezug und Messrichtung** (z. B. `H|e:<body>:<edge>|e:<body>:<edge>` oder `D|c:<body>:<edge>`), damit gleiche Maße in zwei Ansichten und schon vorhandene Maße erkannt werden; die Oberfläche übersetzt `origins` in `GeoRefData`.
  Regeln (Spec §3.5): Nur Normansichten. (a) **Gesamtmaße**: Breite (`Horizontal`) und Höhe (`Vertical`) der **sichtbaren** Linien der Ansicht, bezogen auf die äußersten Kanten (die Kanten, deren projizierte Lage dem Minimum/Maximum entspricht und die quer stehen; bei Rundungen/Bögen am Rand keine Kante quer → Bezug auf den Eckpunkt oder die Umrisslinie des Zylinders; gibt es keine tragfähigen Bezüge, **kein** Gesamtmaß für diese Richtung); (b) **Ø** für jeden vollen Kreis, der als Kreis erscheint, und für jeden Zylinder von der Seite (Paar Umrisslinien); **R** für jeden Bogen (nicht voll) mit Mittelpunkt, der als Bogen erscheint (Rundungen); (c) **Bohrungsmitten** als `Baseline`-Gruppe von der linken bzw. unteren äußersten Kante der Ansicht: je Kreis, der als Kreis erscheint, ein Horizontal- und ein Vertikalmaß vom Bezug zum Mittelpunkt (nur wenn die Bezugskante existiert), Gruppen je Richtung, `order` aufsteigend nach Abstand. **Eindeutigkeit:** ein Maß (gleicher `key`) wird im ganzen Blatt **einmal** vorgeschlagen; bei Wahl zwischen Ansichten gewinnt: Ø/R/Bohrungsmitte → die Ansicht, in der der Kreis **als Kreis** erscheint, sonst die Vorderansicht (Höhen), Draufsicht (Tiefe), Gesamtbreite Vorderansicht; schon vorhandene `existing`-Maße werden übersprungen. Reihenfolge deterministisch.
- [ ] **Step 1: Failing tests** (`tests/stage18_suggest.rs`): (a) `box_gets_three_overall_dimensions_over_three_views` (Quader 10×20×30: Vorne Breite 10 + Höhe 30, Oben Tiefe 20; keine Doppelung der Breite in Oben); (b) `cylinder_gets_diameter_in_top_view_and_height_in_front_view` (Ø10 von oben, Höhe 12 von vorne, Ø nicht zusätzlich von vorne); (c) `hole_diameter_and_position_from_left_bottom_edge` (Quader 40×20×30 mit Ø10-Loch längs Y bei x=20,z=15: Ø10 in Vorne; Horizontal 20 und Vertikal 15 als Bezugsmaße vom Rand); (d) `filleted_box_gets_radii` (R4 an senkrechten Kanten in der Draufsicht); (e) `existing_dimensions_are_not_duplicated` (zweimaliges Vorschlagen → zweites Mal leer); (f) `isometric_gets_nothing`; (g) `empty_and_mesh_only_give_empty_list`; (h) `random_series_no_panic_values_match_formulas` (200 Zufallskörper wie TP11: jedes vorgeschlagene Maß wird mit `measure` gemessen, Wert = Eingabemaß/2r laut Formel, kein Schlüssel doppelt, Reihenfolge deterministisch); (i) `sphere_gets_one_diameter`; (j) `baseline_order_increases_with_distance`.
- [ ] **Step 2–4:** FAIL → Implementieren → PASS (Reihe im Release), clippy, fmt. **Step 5: Mutationsbeweise:** Eindeutigkeitsregel weglassen → (a)/(b) rot; Bezug „äußerste Kante“ gegen „beliebige Kante“ tauschen → (a)/(c) rot. **Step 6: Commit** — `feat: Vorschlag der Grundmaße`.

---

### Task 8: Verweise auflösen und Maße im Dokument (`planora-feature`)

**Files:**
- Create: `crates/planora-feature/src/georef.rs`, `crates/planora-feature/src/dimensions.rs`, `crates/planora-feature/tests/georef.rs`, `crates/planora-feature/tests/dimensions.rs`
- Modify: `crates/planora-feature/src/{lib.rs,drawings.rs}` (`validate_drawing` um Maße erweitern)

**Interfaces:**
- Produces (`georef.rs`):
```rust
pub fn geo_ref_for_edge(solid: &Solid, edge: Id<Edge>, tol: &Tolerance) -> Result<GeoRefData, FeatureError>;
pub fn geo_ref_for_face(solid: &Solid, face: Id<Face>, tol: &Tolerance) -> Result<GeoRefData, FeatureError>;
pub fn to_edge_ref(d: &GeoRefData) -> Option<EdgeRef>;  pub fn to_face_ref(d: &GeoRefData) -> Option<FaceRef>;   // verlustfrei
pub fn from_edge_ref(r: &EdgeRef) -> GeoRefData;        pub fn from_face_ref(r: &FaceRef) -> GeoRefData;
/// Löst einen Bezug gegen die exakten Körper auf. Fehler als DimError (Lost/Ambiguous).
pub fn resolve_geo(solids: &[&Solid], r: &GeoRef, tol: &Tolerance) -> Result<Geo3, DimError>;
```
  Regeln: `Edge` mit gerader Kurve → `Line { a, b }` (Endpunkte der Kante); mit Kreis (volle Kante oder Bogen) → `Circle { center, normal, radius }`; Ellipse/Spline → `Err(Lost("Dieser Kantentyp lässt sich nicht bemaßen."))`; `EdgeEnd` → `Point` (Kantenanfang/-ende, bei Kreis ohne Ende: Anfangspunkt); `Center` → `Point(center)` nur für Kreise/Bögen; `Face` → Zylinderfläche → `Axis { origin, dir, radius }`, Kugel → `Sphere`, andere Flächenarten → `Err(Lost("Diese Fläche lässt sich nicht bemaßen."))`. Mehrdeutigkeit (`resolve_edge` liefert Gleichstand) → `Ambiguous`; nicht gefunden → `Lost(Text)`.
- Produces (`dimensions.rs`, `impl Document`):
```rust
pub fn add_dimension(&mut self, drawing: DrawingId, spec: DimSpec) -> Result<DimId, FeatureError>;   // Kennung wird vergeben
pub fn update_dimension(&mut self, drawing: DrawingId, id: DimId, f: impl FnOnce(&mut DimSpec)) -> Result<(), FeatureError>;
pub fn delete_dimension(&mut self, drawing: DrawingId, id: DimId) -> Result<(), FeatureError>;
pub fn resolve_dimensions(&self, drawing: DrawingId, solids: &[&Solid]) -> Result<ResolvedDims, FeatureError>;   // je Maß Result<Measured, DimError>; Basis/Extent aus der Ansicht und den Körpern
```
  Validierung (`validate_drawing` erweitert, gilt bei Hinzufügen/Ändern und beim Laden): Maßkennung eindeutig, `> 0`, `< next_dim`; `view` existiert und ist eine **Normansicht**; Anzahl der Bezüge passt zur Art (`ref_count`); alle Zahlen endlich, `|offset| ≤ MAX_POS_MM`, `decimals ≤ 3`, `text_override` ≤ 200 Zeichen, `Fit`-Text ≤ 50 Zeichen, Toleranzwerte endlich und `|·| ≤ 1000`; `chain`: Maße derselben `group` haben dieselbe Art und Ansicht, `order` eindeutig in der Gruppe; höchstens `MAX_DIMENSIONS`; Maße einer Isometrie → Fehler. Alle Meldungen deutsch.
  `resolve_dimensions`: Bezüge auflösen (`resolve_geo`), dann `measure`; Ergebnis je Maß (Fehler bleiben im Ergebnis, kein Abbruch); `last_known` aus dem vorigen Ergebnis liefert die Oberfläche (Speicher), hier nicht.
- [ ] **Step 1: Failing tests** (`tests/georef.rs`, `tests/dimensions.rs`): (a) `edge_ref_roundtrip_through_geo_ref_data` und `face_ref_roundtrip` (verlustfrei, `PartialEq`); (b) `resolve_line_circle_axis_sphere_point` (Quader-Kante, Zylinder-Kreiskante, Zylinderfläche, Kugelfläche, Eckpunkt, Kreismitte: Geo3 mit den Formelwerten der Erzeugung); (c) `resolve_after_model_edit_follows` (Quader-Kantenverweis, Feature `Quader` von 10 auf 20 geändert → Kante liegt bei x=20; Wert über `measure` = 20); (d) `resolve_after_deleting_the_feature_is_lost` (`Lost`); (e) `ambiguous_after_split_by_boolean_uses_hint` (zwei Flächen gleicher Marke → Hinweispunkt entscheidet; sonst `Ambiguous`); (f) `unsupported_kinds_are_named` (Spline-Kante, Ebenenfläche, Kegelfläche); (g) `add_update_delete_dimension_with_edit_counters` (nur `drawing_edits` wächst, `model_edit_count` bleibt); (h) `validation_refuses_each_rule` (je Regel ein Test: Isometrie, falsche Bezugszahl, NaN-Offset, `decimals = 4`, 201 Zeichen Text, doppelte Kennung im Snapshot, Gruppe mit gemischten Arten, 2001. Maß); (i) `snapshot_roundtrip_keeps_dimensions_and_centerlines`; (j) `resolve_dimensions_full_pipeline` (Zeichnung mit Quader, vier Maße der Gesamtmaße über echte Verweise → Werte 10/30/20); (k) `deleting_a_view_removes_its_dimensions_in_the_document`; (l) `old_drawings_without_dimensions_load_unchanged`.
- [ ] **Step 2–4:** FAIL → Implementieren → PASS (`cargo test -p planora-feature -j 2`; bestehende 88+ Tests unverändert), clippy, fmt. **Step 5: Mutationsbeweise:** Auflösung ohne Hinweispunkt-Wahl → (e) rot; Ansichtsprüfung weglassen → (h) rot; Kennungsprüfung im Snapshot weglassen → (h) rot. **Step 6: Commit** — `feat: Bezüge auflösen und Maße im Dokument`.

---

### Task 9: Dateiformat 5 (`planora-project`)

**Files:**
- Create: `crates/planora-project/tests/fixtures/format4_drawings_no_dims.json` (**vor** der Änderung mit dem alten Schreiber erzeugt), `tests/dimensions_format.rs`
- Modify: `crates/planora-project/src/{format.rs,convert.rs}`, `.gitattributes`

**Interfaces:**
- Produces (`format.rs`): `MassDto { id, ansicht, art, bezuege: Vec<BezugDto>, abstand, winkel, textversatz: [f64;2], toleranz: ToleranzDto, stellen: u8, text: Option<String>, kette: Option<KetteDto>, vorgeschlagen: bool }`; `BezugDto` mit `typ` ∈ `kante`, `kantenende` (+`ende` `anfang|ende`), `mitte`, `flaeche` und den Marken (`a`, `b`, `hinweis` bzw. `marke`); `ToleranzDto` mit `typ` ∈ `keine`, `symmetrisch`, `abmasse`, `grenzmasse`, `passung`; `KetteDto { gruppe, modus: kette|bezug, reihenfolge }`; `AnsichtDto.mittellinien: Option<bool>` (`skip_serializing_if = "Option::is_none"`); `ZeichnungDto.masse: Vec<MassDto>` (`skip_serializing_if = "Vec::is_empty"`, `default`), `ZeichnungDto.naechstes_mass: u64` (`skip_serializing_if` null).
- Produces (`convert.rs`): `FORMAT_VERSION = 5`; `required_format` = **5**, wenn eine Zeichnung Maße enthält oder eine Ansicht `centerlines = true` hat; sonst wie bisher. Schreiben: bei Format ≤ 4 **kein** `mittellinien`, kein `masse` (alte Dateien Byte für Byte); Lesen: Format < 5 mit `masse`/`mittellinien` → beschädigt; `mittellinien` fehlt → `false`; Prüfungen wie bei den Ansichten in TP11 (Zahlen endlich, `≤ MAX_POS_MM`, Listenlängen vor dem Anlegen begrenzt (`MAX_DIMENSIONS`, Bezüge höchstens 4 je Maß, `stellen ≤ 3`, Texte ≤ 200 Zeichen), Kennungen `≠ 0`, `< naechstes_mass`, Strings → Aufzählungen); danach die Prüfung des Dokuments (`validate_drawing`).
- [ ] **Step 1: Goldene Fixtur zuerst** (`format4_drawings_no_dims.json` aus dem **jetzigen** Schreiber: Zeichnung mit allen Ansichtsarten, `centerlines = false` explizit gesetzt), eigener Commit („test: Fixtur Format 4 ohne Maße (alter Encoder)“).
- [ ] **Step 2: Failing tests** (`tests/dimensions_format.rs`): (a) `old_format_4_files_stay_byte_for_byte` (Fixtur wie bisher, `centerlines=false`: `encode(decode(fixtur)) == fixtur`, Format 4); (b) `format_5_only_with_dimensions_or_centerlines`; (c) `roundtrip_all_kinds` (Maßarten, Bezugsarten, Toleranzarten, Kette, Zahlen bitgenau); (d) `format_4_with_dimensions_is_refused`; (e) `hostile_dimension_files` (je Fall Fehler mit deutschem Präfix: NaN/1e999, unbekannte Art/Bezugstyp/Toleranztyp, `stellen = 9`, 1e300, doppelte Kennung, Kennung ≥ `naechstes_mass`, Bezug mit fremder Zahl, Ansicht unbekannt, Isometrie-Ansicht, zu viele Maße (2001) **schnell abgewiesen ohne große Zuteilung** (Zählung wie im TP11-Test), Text 201 Zeichen); (f) `centerlines_default_is_false_for_old_files`; (g) `newer_format_6_is_refused_with_the_clear_text`; (h) `autosave_and_version_store_keep_dimensions` (über `DocStore`); (i) `float_values_roundtrip_exactly`.
- [ ] **Step 3: Run → FAIL. Step 4: Implementieren. Step 5: Run → PASS** (`cargo test -p planora-project -j 2`, `cargo test -p planora-feature -j 2`; bestehende Tests, die die Formatnummer 4 als „zu neu“-Grenze nutzen, auf 6 anheben – mechanisch, wie in TP11), clippy, fmt.
- [ ] **Step 6: Mutationsbeweise:** `skip_serializing_if` bei `mittellinien` entfernen → (a) rot; Formatprüfung < 5 entfernen → (d) rot; Längenbegrenzung entfernen → Riesenlisten-Test rot.
- [ ] **Step 7: Commit** — `feat: Format 5 mit Maßen und Mittellinien`.

---

### Task 10: Zustand der App – Maße auflösen, Vorschlag setzen, Blatt mit Marken

**Files:**
- Create: `crates/planora-app/src/dimension_state.rs`, `crates/planora-app/tests/dimension_state.rs`
- Modify: `crates/planora-app/src/{lib.rs,state.rs,drawing_view.rs}` (nur die Stelle, an der das Blattmodell gebaut wird: `build_sheet` → `build_sheet_with`), `crates/planora-app/src/drawing_state.rs` (Hilfsfunktion `exact_solids`, falls noch nicht öffentlich)

**Interfaces:**
- Consumes: `Document::{resolve_dimensions, add_dimension, update_dimension, delete_dimension}` (Aufgabe 8), `planora_drawing::{suggest, build_sheet_with, ResolvedDims, Candidate}` (Aufgaben 4, 7), `DrawingUi::{results_for, result}`.
- Produces:
```rust
#[derive(Default)] pub struct DimensionUi { /* Zwischenspeicher, letzte bekannte Anker */ }
impl DimensionUi {
    /// Löst die Maße der Zeichnung auf (nur wenn Modell- oder Dokumentstand sich geändert haben); billig bei unverändertem Stand.
    pub fn resolve(&mut self, state: &AppState, drawing: DrawingId) -> Arc<ResolvedDims>;
    /// Wie `resolve`, aber nur lesend aus dem Zwischenspeicher für genau diese Stände (sonst leer) – Schutz gegen veraltete Maße (wie `result(kind, model)`).
    pub fn cached(&self, drawing: DrawingId, model: u64, doc_edit: u64) -> Option<Arc<ResolvedDims>>;
    pub fn notes(&self, resolved: &ResolvedDims, sheet: &SheetModel) -> DimNotes;   // verloren: n, außerhalb des Blatts: m, Texte
}
pub struct DimNotes { pub lost: usize, pub outside: usize, pub texts: Vec<String> }
pub fn suggest_for(state: &mut AppState, drawing: DrawingId, only_view: Option<ViewId>) -> SuggestOutcome;
pub enum SuggestOutcome { Added(usize), NothingNew, Unavailable(String) }
```
  `resolve`: Schlüssel `(drawing, model_edit_count, edit_count)`; löst über `Document::resolve_dimensions` mit den **exakten** Körpern von `state.bodies` (Netz-Körper nicht); pflegt `last_known: BTreeMap<DimId, Anchor>` (letzter Erfolg), damit verlorene Maße an der alten Stelle rot erscheinen; nach Löschen eines Maßes verfällt sein Eintrag. `suggest_for`: nimmt die **berechneten** Ansichten (`DrawingUi::results_for(drawing, model)`); fehlen Ergebnisse → `Unavailable("Die Ansichten werden noch berechnet.")`; sonst `suggest(..)`, wandelt Herkunft in `GeoRefData` (`geo_ref_for_edge/face`), vergibt Gruppen für Bezugsmaße, schreibt über `Document::add_dimension` (jedes Maß einzeln validiert; scheitert eines, wird es übersprungen und in `texts` gemeldet), setzt `offset` nach P3 (`DEFAULT_OFFSET + n·DIM_STEP` je Ansicht und Richtung), `suggested = true`. Wiederholtes Drücken → `NothingNew`.
- [ ] **Step 1: Failing tests** (`tests/dimension_state.rs`, Stil wie `tests/drawing_state.rs`): (a) `resolve_gives_values_for_a_box_after_adding_overall_dimensions`; (b) `model_edit_updates_values_without_recomputing_views` (Quader 10 → 20: Maßwert 20, `DrawingUi::spawned()` wächst nur durch die Ansichtsberechnung); (c) `deleted_reference_becomes_lost_with_reason_and_keeps_last_anchor`; (d) `cached_refuses_stale_states` (nach Modelländerung ohne `resolve` liefert `cached` `None`); (e) `suggest_adds_overall_and_diameter_dimensions_once` (zweimaliges Drücken → `NothingNew`); (f) `suggest_waits_for_views` (`Unavailable`); (g) `suggest_on_mesh_only_or_empty_document_is_named` (kein Absturz, Text); (h) `suggest_respects_max_dimensions` (2000 erreicht → benannte Meldung, kein Überlauf); (i) `notes_count_lost_and_outside_the_sheet` (Maß mit `offset = 5000` → `outside = 1`, Text „liegt außerhalb des Blatts“); (j) `resolve_is_cheap_when_nothing_changed` (zweiter Aufruf liefert denselben `Arc` – `Arc::ptr_eq`); (k) `drag_edits_do_not_trigger_view_recompute` (Offset ändern → `spawned()` unverändert); (l) `drawing_area_uses_the_sheet_with_marks` (Blatt enthält Marken des Maßes).
- [ ] **Step 2–4:** FAIL → Implementieren → PASS (`cargo test -p planora-app --test dimension_state -j 2`, dann die ganze App-Reihe einmal), clippy, fmt. **Step 5: Mutationsbeweise:** Schlüssel ohne `edit_count` → (b)/(k) rot; `cached` ohne Standvergleich → (d) rot; Duplikatprüfung in `suggest_for` weglassen → (e) rot. **Step 6: Commit** — `feat: Zustand der App für Maße (Auflösen, Vorschlag, Blatt mit Marken)`.

---

### Task 11: Anzeige der Maße und Mittellinien im Zeichnungsbereich

**Files:**
- Modify: `crates/planora-app/src/drawing_view.rs` (Zeichnen der Marken, Auswahl, Ziehen, Werkzeugleiste: „Mittellinien“, „Maße vorschlagen“), `crates/planora-app/src/theme.rs` (Farben: Maße Blatt-schwarz, Fehler rot, Auswahl Akzent)
- Create: `crates/planora-app/tests/dimension_view.rs`

**Interfaces:**
- Consumes: `SheetView.marks`, `DimensionUi`, `suggest_for`.
- Produces: Der Zeichnungsbereich zeichnet `Mark`s: `Line` (Breite `max(1 px, 0,25·k)`), Mittellinien mit dem Strichmuster `[12, 3, 2, 3]` in Blatt-mm (wie die Strichelung verdeckter Linien, `dashes()`-Helfer wiederverwenden), `Arrow` als gefülltes Dreieck, `Arc` als Polylinie (≤ 1 px Abweichung), `Text` mit `egui`-Schrift (Roboto) in `height·k` Pixeln (`k` = Pixel je Blatt-mm), Drehung, Anker; `MarkKind::Lost` rot. Auswahl: Klick auf eine Maßlinie oder in die Hüllbox des Textes wählt das Maß (Abstand ≤ 6 px); gewähltes Maß wird akzentfarben gezeichnet. **Ziehen** des gewählten Maßes: auf der **Maßlinie** → `offset` ändern (Bewegung auf die Normale der Maßlinie projiziert, Vorzeichen wie im Layout), auf dem **Text** → `text_shift` ändern, am **Führungslinienende** (Ø/R) → `angle` und `offset` ändern; jede Änderung über `Document::update_dimension` (ändert nie `model_edit_count`); schreibgeschützte Tabs: nur Auswählen. Werkzeugleiste: Schalter „Mittellinien“ für die gewählte Ansicht (`ViewSpec.centerlines`, über `update_drawing`), Knöpfe „Maße vorschlagen“ (ganze Zeichnung, bzw. nur gewählte Ansicht, wenn eine gewählt ist) mit Meldung (`SuggestOutcome`), Hinweis „n Maße haben ihren Bezug verloren“ und „m Maße liegen außerhalb des Blatts“ unten im Bereich (neben dem bisherigen Hinweis). Der Abflachungs-Zwischenspeicher aus TP11 bleibt: Marken sind wenige; ein Test sorgt dafür, dass das Ziehen eines Maßes die Linien **nicht** neu abflacht (Zähler).
- [ ] **Step 1: Failing tests** (`tests/dimension_view.rs`, echte egui-Ereignisse wie `tests/drawing_ui.rs`): (a) `marks_are_painted_with_expected_screen_positions` (Maßlinie, Pfeile, Text an den Blattkoordinaten aus der Abbildung `sheet_to_screen`); (b) `centerlines_are_dashed_and_follow_the_toggle`; (c) `lost_dimension_is_red_and_has_a_tooltip_with_the_reason`; (d) `click_selects_the_dimension_by_line_and_by_text`; (e) `dragging_the_dimension_line_changes_only_the_offset` (exakter Zahlenwert: 20 px Zug bei Zoom k → offset + 20/k); (f) `dragging_the_text_changes_only_the_text_shift`; (g) `leader_drag_changes_angle_and_offset`; (h) `read_only_tab_can_select_but_not_drag_or_toggle_or_suggest`; (i) `suggest_button_adds_dimensions_and_reports_the_count`; (j) `suggest_button_reports_nothing_new_on_second_press`; (k) `toggle_centerlines_updates_the_sheet`; (l) `notes_line_shows_lost_and_outside_counts_in_german`; (m) `dragging_does_not_reflatten_the_lines` (Zähler wie in TP11); (n) `german_texts_only` (Wortsperrliste wie in `drawing_ui`); (o) `dark_theme_keeps_marks_black_on_white_paper`.
- [ ] **Step 2–4:** FAIL → Implementieren → PASS (`cargo test -p planora-app --test dimension_view --test drawing_ui -j 2`), clippy, fmt. **Step 5: Mutationsbeweise:** Normalenprojektion beim Ziehen weglassen → (e) rot; Rotfärbung entfernen → (c) rot; Schreibschutz weglassen → (h) rot. **Step 6: Commit** — `feat: Maße und Mittellinien im Zeichnungsbereich anzeigen und ziehen`.

---

### Task 12: Werkzeug „Maß“

**Files:**
- Create: `crates/planora-app/src/dimension_tool.rs`, `crates/planora-app/tests/dimension_tool.rs`
- Modify: `crates/planora-app/src/drawing_view.rs` (Werkzeugleiste: Schalter „Maß“ mit Untermodi und Klick-/Vorschaulogik anbinden), `crates/planora-app/src/state.rs` (Feld `dimension_tool`)

**Interfaces:**
- Produces:
```rust
pub enum ToolMode { Off, Linear(LinearDir), Diameter, Radius, Angle, Chain, Baseline }
pub enum LinearDir { Horizontal, Vertical, Aligned }
pub struct DimensionTool { pub mode: ToolMode, pending: Vec<GeoRef>, preview: Option<DimSpec>, group: Option<u64> }
impl DimensionTool {
    pub fn click(&mut self, state: &mut AppState, drawing: DrawingId, view: ViewId, hit: &Hit) -> ToolOutcome;   // Treffer = SheetLine-Ursprung + Position (nahe Ende → EdgeEnd)
    pub fn hover(&mut self, ...) -> Option<Vec<Mark>>;      // Vorschau der Maßlinie am Zeiger
    pub fn cancel(&mut self); pub fn finish(&mut self, state, drawing, pointer: DVec2 /*Blatt*/) -> Result<DimId, ToolError>;
}
pub enum ToolOutcome { NeedMore(String /*Hinweistext*/), Ready, Rejected(String) }
```
  Ablauf: Wahl des Untermodus; **Klick auf eine Linie** der Ansicht → Bezug aus `SheetLine.origin` (`StrokeOrigin::Edge{body,edge}` → `geo_ref_for_edge`; Klick innerhalb 8 px eines Linienendes → `EdgeEnd`; `Silhouette{body,face}` → `GeoRef::Face` (Zylinder/Kugel) für Ø/R quer; Kreiskante + Modus Ø/R → `Edge`; Kreisklick + „Mitte“ (Klick nahe der Mittellinie) → `Center`); nach der geforderten Bezugszahl wird die Maßlinie am Zeiger gezeigt (Vorschau, `Document`-unabhängig: `measure` + `layout_dimension` auf dem Entwurf); **zweiter Klick** setzt das Maß (`Document::add_dimension` mit `offset` aus der Zeigerlage, Vorzeichen nach Seite). Ø/R: ein Bezug, Klick setzt (Winkel/Offset aus der Zeigerlage). Winkel: zwei Geradenkanten, Klick setzt Bogenradius. **Kette:** nacheinander Punktklicks A, B, C … (EdgeEnd/Kante), Esc/Doppelklick beendet; erzeugt Maße AB, BC… mit gemeinsamem `offset`; **Bezug:** erster Klick = Bezug, weitere Klicks = Ziele, Maßlinien gestapelt. Ablehnung (`Rejected`) mit deutschem Text, z. B. „Die Kante steht nicht quer zum Maß“, „In der Isometrie gibt es keine Maße“ (Werkzeug in Isometrie gesperrt, Schalter deaktiviert mit Erklärung), „Der Bezug lässt sich nicht bemaßen“. Esc bricht ab. Alle Änderungen über `Document::add_dimension`; ein Maß = eine Änderung.
- [ ] **Step 1: Failing tests** (`tests/dimension_tool.rs`, Ereignisse wie in `drawing_ui`): (a) `horizontal_dimension_by_two_edge_clicks_and_placing` (Quader 10×20×30 Vorne: zwei senkrechte Kanten → Wert 10, `offset` aus der Zeigerlage genau); (b) `edge_end_hit_when_clicking_near_the_end` (8 px Schwelle); (c) `vertical_and_aligned_modes`; (d) `diameter_by_clicking_a_circle_in_top_view` (Zylinder r=5 → Ø10, Winkel 45°); (e) `diameter_by_clicking_the_cylinder_silhouette_in_front_view` (`Face`-Bezug, Ø10 linear quer); (f) `radius_of_a_fillet_arc`; (g) `angle_between_two_lines_of_an_l_profile` (90°); (h) `chain_by_three_clicks_makes_two_dimensions_with_one_line`; (i) `baseline_stacks_dimension_lines_by_7_mm`; (j) `rejections_are_german_and_leave_the_document_unchanged` (Kante nicht quer; Isometrie; Netz-Körper; Ellipsenkante); (k) `escape_cancels_without_changes`; (l) `preview_matches_the_final_dimension_marks` (Vorschau == Marken des gesetzten Maßes); (m) `tool_is_disabled_in_read_only_tabs_and_isometric_view`; (n) `every_created_dimension_resolves_to_the_value_of_the_formula_after_model_edit` (Quader ändern → Wert folgt); (o) `each_created_dimension_is_one_document_edit`.
- [ ] **Step 2–4:** FAIL → Implementieren → PASS, clippy, fmt. **Step 5: Mutationsbeweise:** Schwelle 8 px entfernen (immer Kante) → (b) rot; Isometrie-Sperre weglassen → (m) rot; Vorschau ≠ Endergebnis (anderes Layout) → (l) rot. **Step 6: Commit** — `feat: Werkzeug zum Setzen von Maßen`.

---

### Task 13: „Maß bearbeiten“ und Browser-Abschnitt „Maße“

**Files:**
- Create: `crates/planora-app/src/dimension_dialog.rs`, `crates/planora-app/src/dimension_browser.rs`, `crates/planora-app/tests/dimension_dialog.rs`, `crates/planora-app/tests/dimension_browser.rs`
- Modify: `crates/planora-app/src/{browser.rs,drawing_browser.rs,state.rs,app.rs,lib.rs}` (Anbindung)

**Interfaces:**
- Produces:
  - `DimensionDialog` (Fenster „Maß bearbeiten“, wie `NewDrawingDialog`: `show(&ctx, &mut state) -> bool`): zeigt Art, Wert (nur lesen, rot bei verlorenem Bezug mit Grund), Eingaben: Nachkommastellen (0–3), Toleranzart (Keine / Symmetrisch ± / Oben und unten / Grenzmaße / Passung) mit Zahlenfeldern bzw. Textfeld, Maßtext überschreiben (Feld, Hinweis „<> steht für den berechneten Wert“), Knöpfe „Übernehmen“, „Abbrechen“, „Maß löschen“. Fehler (Zahl kein Wert, Text zu lang) rot, „Übernehmen“ gesperrt. Öffnet per Doppelklick auf das Maß im Blatt oder im Browser. Änderungen über `Document::update_dimension`.
  - Browser: Abschnitt „Maße“ unter dem Abschnitt „Zeichnungen“ für die **geöffnete** Zeichnung: je Ansicht eine Gruppenzeile (Ansichtsname), darunter Zeilen `Ø10,0` / `12,5 ±0,1` / `?` (rot, Grund als Tooltip) mit Symbol je Art; Klick wählt das Maß (und öffnet die Zeichnung), Doppelklick öffnet „Maß bearbeiten“, Rechtsklick „Bearbeiten“, „Löschen“ (Rückfrage nur bei Maßketten: „Dieses Maß gehört zu einer Kette; die anderen Maße bleiben“). Leer: „Noch keine Maße. Mit dem Werkzeug „Maß“ oder „Maße vorschlagen“ lassen sich welche anlegen.“ Schreibgeschützte Tabs: ansehen/auswählen, nichts ändern. Volle Breite, Zeilenhöhe wie Featurezeilen.
- [ ] **Step 1: Failing tests:** `tests/dimension_dialog.rs`: (a) `dialog_changes_decimals_and_tolerance_kinds` (alle fünf Arten, Werte exakt); (b) `invalid_numbers_block_apply_with_red_text`; (c) `override_text_with_placeholder`; (d) `delete_button_removes_the_dimension_and_closes`; (e) `lost_dimension_shows_the_reason_and_the_last_value_is_not_shown`; (f) `read_only_tab_disables_every_input`; (g) `german_texts_only`; (h) `double_click_on_the_sheet_dimension_opens_the_dialog`. `tests/dimension_browser.rs`: (i) `rows_and_texts_for_all_kinds` (exakte Texte inkl. Komma, Ø, ±, −); (j) `lost_row_is_red_with_reason`; (k) `click_selects_double_click_edits_context_menu_deletes`; (l) `chain_delete_asks_first`; (m) `empty_state_text`; (n) `rows_use_full_width_and_equal_heights`; (o) `deleting_the_last_dimension_clears_selection`; (p) `save_and_load_keep_dimensions_through_workspace` (Speichern → Laden → Browser zeigt dieselben Maße, Werte neu berechnet).
- [ ] **Step 2–4:** FAIL → Implementieren → PASS (`cargo test -p planora-app --test dimension_dialog --test dimension_browser -j 2`, dann die ganze App-Reihe), clippy, fmt. **Step 5: Mutationsbeweise:** Rückfrage umgehen → (l) rot; Schreibschutz entfernen → (f)/(k) rot. **Step 6: Commit** — `feat: Maß bearbeiten und Browser-Abschnitt Maße`.

---

### Task 14: Export mit Maßen und Mittellinien

**Files:**
- Modify: `crates/planora-app/src/export_dialog.rs` (Aufbereitung: Maße auflösen, `build_sheet_with`; Karte: Zähler; Sperre bei verlorenen Maßen), `crates/planora-app/tests/export_drawing.rs` (mechanische Anpassungen), Create: `crates/planora-app/tests/export_dimensions.rs`

**Interfaces:**
- Produces: `prepare_drawing` löst nach den Ansichten die Maße der Zeichnung gegen **Kopien der exakten Körper** auf (im Arbeitsthread, wie die Ansichten; kein Zugriff aus zwei Stellen: F11), baut `SheetModel` mit `build_sheet_with`; Karte zeigt zusätzlich „Maße: n“, „Mittellinien: ja/nein“ (je Ansicht gemischt: „teilweise“). **Festlegung P13:** Hat ein Maß seinen Bezug verloren (Fehler im Ergebnis), ist der Zustand `Failed` mit Text „Das Maß „{Text}“ in der Ansicht „{Name}“ hat seinen Bezug verloren: {Grund}. Bitte ändern oder löschen.“; es wird nichts geschrieben. Maße außerhalb des Blatts werden geschrieben und in der Karte gezählt (P12). Schlüssel `(Zeichnung, edit_count, Format)` bleibt. Dateiinhalt: Marken in SVG/DXF nach Aufgaben 5/6; Ausgaben ohne Maße/Mittellinien Byte für Byte wie bisher.
- [ ] **Step 1: Failing tests** (`tests/export_dimensions.rs`, Stil `export_drawing.rs`): (a) `svg_and_dxf_contain_dimensions_and_centerlines` (Quader mit Gesamtmaßen und Zylinder mit Mittellinien: Dateien lesen, Marken per eigener Leser/Validator prüfen, Bytes == frische `svg_string(build_sheet_with(..))`); (b) `old_drawings_export_byte_for_byte_like_before` (Zeichnung ohne Maße und `centerlines=false` → identisch zu `build_sheet`-Ausgabe); (c) `lost_dimension_blocks_writing_with_its_text`; (d) `outside_dimension_is_written_and_counted`; (e) `card_shows_counts_and_stays_full_width_equal_height`; (f) `stale_dimension_edit_prepares_again` (Maß verschoben → neue Aufbereitung, kein altes Ergebnis); (g) `hostile_text_is_escaped_in_both_formats` (Maßtext mit `<&"'>`); (h) `dimensions_resolved_in_the_worker_not_from_the_drawing_cache` (Zähler/Hook wie bei den Ansichten); (i) `cancel_during_dimension_resolution_leaves_no_file`; (j) `every_view_kind_exports_without_marks_on_isometric`.
- [ ] **Step 2–4:** FAIL → Implementieren → PASS (`cargo test -p planora-app --test export_dimensions --test export_drawing --test export_dialog --test export_step -j 2`, dann die ganze App-Reihe), clippy, fmt. **Step 5: Mutationsbeweise:** Sperre bei verlorenen Maßen entfernen → (c) rot; Schlüssel ohne `edit_count` → (f) rot. **Step 6: Commit** — `feat: Export von Zeichnungen mit Maßen und Mittellinien`.

---

### Task 15: Dokumentation, Proben für den Handtest, Gates

**Files:**
- Modify: `README.md`, `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt 12 – Maße und Mittellinien“), Spec (Abschnitt „Festlegungen der Umsetzung“ mit P1–P13 und den Rulings aus dem Protokoll; berichtigte Stellen), diesen Plan (falls Festlegungen angepasst wurden)
- Create: `crates/planora-drawing/tests/hand_samples_dims.rs` (`#[ignore]`, schreibt Proben nach `$PLANORA_SAMPLES_DIR` oder `D:\Testumgebung\zeichnung-proben-masse`)

**Interfaces / Schritte:**
- [ ] **Step 1: Proben-Test** `write_dimension_samples_for_the_hand_test` (ignoriert; zusätzlich ein nicht ignorierter Mini-Test, der 2 Proben in ein Temp-Verzeichnis schreibt): Proben `Quader_Masse`, `Zylinder_Masse`, `Bohrung_Masse` (Ø, Bohrungsmitten als Bezugsmaße, Mittellinien), `Winkel_Masse` (Winkelmaß 90° und eine 45°-Fase), `Rundung_Masse` (R), `Kette_Bezug` (Maßkette und Bezugsmaße), `Toleranzen` (symmetrisch, oben/unten, Grenzmaße, Passung), `Verloren` (ein Maß mit verlorenem Bezug als Gegenprobe: nicht exportierbar – Probe wird mit `build_sheet_with` ohne Sperre geschrieben und als „rot“ beschriftet), je SVG und DXF, A3 quer, Vorschlag **und** handgesetzte Maße. `ERWARTUNG.txt`: je Probe die Maßwerte aus Formeln (z. B. Quader 10/20/30, Zylinder Ø10, Höhe 12, Bohrung Ø10 bei 20/15) und die Prüfpunkte für Fusion (Text lesbar und aufrecht, Pfeilspitzen auf den Hilfslinien, Mittellinien strichpunktiert, `Ø`/`±` richtig, rot für verlorenes Maß), ausdrücklich: **Fusion 360 liest DXF-TEXT/SOLID** – Ergebnis offen bis zum Handtest.
- [ ] **Step 2: Doku:** README (kurz, Laiensprache: Maße, Mittellinien, Vorschlag, Werkzeug, was fehlt: Isometrie, Passungstabellen, echte DXF-Maße, Schriftfeld/Texte/Stückliste/Schnitte folgen), `offene-punkte.md` (Entscheidungen, Gelieferte Funktionen, **alle** bekannten Grenzen aus dem Protokoll (P7 Textbreite geschätzt, Maße nur Normansichten, Auflösung Ellipsen/Splines/Ebenenflächen nicht bemaßbar, Vorschlag nur Grundmaße, Export gesperrt bei verlorenen Maßen, Fusion-Handtest-Stand), Zahlen (Testanzahl: einmal `cargo test --workspace -j 2` am Ende, Zeit), Folgearbeiten mit Prioritäten), Spec „Festlegungen der Umsetzung“, Plan berichtigen.
- [ ] **Step 3: Gates (Bericht mit Ausgaben):** `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2` (lang; Zahlen berichten); `cargo deny check`; `git diff --stat 945fa67 -- crates/planora-kernel crates/planora-mesh crates/planora-step crates/planora-exchange` **leer**; `git status` sauber.
- [ ] **Step 4: Commit(s)** — `docs: Maße und Mittellinien, Proben für den Handtest, Grenzen`.
- [ ] **Step 5 (Controller, nach Rückfrage beim Nutzer):** Handtest der Proben in Fusion 360 (SVG im Browser/Inkscape, DXF in Fusion einfügen); Ergebnis nachtragen; falls DXF-`TEXT`/`SOLID` in Fusion abgelehnt werden: Gegenprobe-Varianten mit ezdxf bauen (Verfahren aus TP11: einzelne Teile vertauschen) und gezielt beheben.
- [ ] **Step 6 (Controller):** Gesamtreview des Zweigs (opus), Nacharbeit, dann Meldung „fertig, Merge-Entscheidung beim Nutzer“.

---

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung:** §1 M1 (Aufteilung) → Aufgabe 15 (Doku); M2 (von Hand und automatisch) → Aufgaben 7, 10, 11, 12; M3 (Maßarten, Ketten, Toleranzen) → Aufgaben 1, 2, 4, 12, 13; M4 (Verweise, Rotfärbung) → Aufgaben 1, 8, 10, 11; M5 (nur Normansichten) → Aufgaben 2, 8, 12; M6 (Linien/Pfeile/Text) → Aufgaben 4–6; M7 (Mittellinien) → Aufgaben 3, 4, 11; M8 (Darstellung) → Global Constraints, Aufgabe 4; M9 (Passung als Text) → Aufgaben 1, 4, 13; M10 → Global Constraints. §2 Datenmodell → Aufgaben 1, 8, 9. §3 Berechnung → Aufgaben 2, 3, 7. §4 Ausgabe → Aufgaben 4–6, 14. §5 Oberfläche → Aufgaben 10–13. §6 Fehler → Aufgaben 1, 2, 12. §7 Prüfung → alle Aufgaben, 15. §8 Stufen → Aufgabenreihenfolge.
**Platzhalter-Scan:** Die Aufgaben nennen für jede Prüfung Eingabe und erwartetes Ergebnis; Zahlenwerte stehen in den Tests (von Hand ausgerechnet) oder sind aus den Eingabemaßen abzuleiten (Formelmodell). Offene Gestaltungsfragen sind als Festlegungen P1–P13 entschieden.
**Typkonsistenz:** `DimSpec`, `GeoRef`, `GeoRefData`, `Geo3`, `Measured`, `Anchor`, `Mark`, `MarkKind`, `ResolvedDims`, `CenterLine`, `Candidate` heißen in allen Aufgaben gleich; `build_sheet_with` (4) wird in 10, 11, 14 benutzt; `suggest` (7) in 10; `resolve_dimensions` (8) in 10, 14; Format 5 (9) trägt dieselben Felder wie `DimSpec`.
**Bekannte Risiken des Plans:** Aufgabe 4 (Layout-Regeln: viele Sonderfälle), Aufgabe 6 (Fusion-Eigenheiten bei `TEXT`/`SOLID`; Handtest zeigt es erst am Ende, siehe Erfahrung aus Teilprojekt 11), Aufgabe 7 (Vorschlag: „äußerste Kanten“ bei gerundeten Körpern), Aufgabe 12 (Treffer-/Bezugslogik). Der Controller setzt für 2, 3, 4, 6, 7, 8, 9, 10, 12, 14 den fähigsten Implementierer (opus) ein; 1, 5, 11, 13, 15 ein mittleres Modell (sonnet) mit Prüfung durch opus bei 5, 11 und 13.
