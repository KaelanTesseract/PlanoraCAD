# Schnittansichten (Teilprojekt 3d) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **COMMIT-TRAILER (jeder Commit, ohne Ausnahme, wörtlich als letzte Zeile):**
> `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
> Frühere Agents haben hier wiederholt „Opus“ oder einen anderen Namen eingesetzt. Vor jedem `git commit` die Zeile Zeichen für Zeichen prüfen.
> Immer nur die genannten Pfade einzeln `git add`en (nie `git add -A`/`.`/`-u`).

**Goal:** Vollschnitte als abhängige Ansichten: Schnittlinie mit zwei Klicks in einer Normansicht, Körper mit dem vorhandenen Kernel an der Ebene geschnitten, Schnittfläche schraffiert (Löcher frei), Schnittlinie mit Pfeilen und Buchstaben in der Quellansicht, Ausgabe in SVG, DXF und PDF, Dateiformat 8 nur mit Schnitten.

**Architecture:** `planora-drawing` bekommt drei reine Bausteine: `section.rs` (Schneidequader, `boolean(.., BoolOp::Difference)`, Schnittflächen an der Herkunftsmarke, Randschleifen, `compute_section`), `hatch.rs` (Zuschnitt paralleler Linien gegen Vielecke nach gerade-ungerade, rein 2D) und `section_marks.rs` (Schraffur, Schnittlinie, Titel als `Mark`s mit zwei neuen `MarkKind`s). Das Modell bekommt `ViewSpec.section: Option<SectionSpec>`; die geschnittenen Körper laufen unverändert durch `compute_view`. Die App rechnet Schnittansichten im vorhandenen Hintergrund-Zwischenspeicher (Schlüssel `ViewKey` statt `ViewKind`), bekommt das Werkzeug „Schnitt“, Felder im Ansichtsfeld und die Ausgabe im Export-Fenster; Maße und Positionsnummern in Schnittansichten arbeiten auf den geschnittenen Körpern.

**Tech Stack:** Rust (Workspace, keine neuen Abhängigkeiten), `planora_kernel::boolean::boolean` und `primitives::make_box` (nur benutzt, nicht geändert), egui/eframe 0.36; Tests im eigenen Harness (`egui::Context::run_ui`, `AppState`), Leser `crates/planora-drawing/tests/common/{svg_read,dxf_read,pdf_read}.rs`.

**Spec:** `docs/superpowers/specs/2026-10-19-schnittansichten-design.md` (vom Nutzer freigegeben samt §9 Festlegungen und den Vorschlägen aus §10: Vollschnitt, 3 mm × f und 45°, verdeckte Kanten aus, Buchstaben A, B, C …, Schnittlinie per zwei Klicks). Vorbilder für Stil und Muster: `docs/superpowers/plans/2026-10-18-stueckliste.md` (3c-2: Werkzeug, Format 7, Sperren) und `docs/superpowers/plans/2026-10-18-pdf-ausgabe.md` (3c-3: Goldens, Writer). Basis: `master` @ `7f1e319`; Arbeitszweig `feature/schnittansichten` in eigenem Worktree.

## Global Constraints

- Texte deutsch in einfacher Laiensprache, keine technischen Hinweise, keine Hinweise auf Datenherkunft. Feste Texte: Knopf „Schnitt“; Ansichtsname „Schnitt A–A“ (Halbgeviertstrich U+2013); Titel unter der Schnittansicht „A–A (1:2)“; „Die Schnittebene trifft den Körper nicht.“; „Bei „{Körper}“ lässt sich der Schnitt nicht berechnen.“; Hinweis bei verschobener Ebene „Die Schnittebene lag genau auf einer Fläche oder Kante und wurde um 0,001 mm verschoben.“
- Werte aus der Spec: Schraffur 45°/135° je Körper wechselnd, Abstand Vorgabe 3 mm × f (f = `sheet_scale`, lange Blattseite / 420), dünner Stift wie Maßlinien (`DIM_LINE_W`); Abstand unter 0,5 mm auf dem Blatt wird abgelehnt; Endstücke der Schnittlinie 8 mm × f dick; Buchstaben in Maßtexthöhe (`DIM_TEXT_H`); verdeckte Kanten in Schnittansichten Vorgabe aus; Buchstaben `ABCDEFGHJKLMNPRSTUVWXYZ` (23, ohne I, O, Q); höchstens 12 Schnittansichten je Blatt; Tangentialfall: Verschiebung 0,001 mm; Maßstab der Schnittansicht = Maßstab der Quellansicht; `MAX_STROKES` (20 000) gilt nur für Kanten, Schraffur hat eigene Grenze.
- **Kernel unverändert.** Der Schnitt benutzt nur `make_box`, `Solid::transformed`, `assign_origin`, `boolean`. Geschützte Pfade ohne Diff gegen `7f1e319`: `crates/planora-kernel/**`, `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`. Zeigt Aufgabe 1, dass ein Grundfall (Quader, Zylinder, Quader mit Bohrung, Hohlzylinder, Nut) auch mit Verschiebung nicht geht, **stoppt** der Implementierer und meldet es dem Controller; eine Kernel-Änderung braucht die Zustimmung des Nutzers.
- **Keine neuen Abhängigkeiten**: `Cargo.toml`/`Cargo.lock`/`deny.toml` ohne Diff; `cargo deny check` grün mit den in `deny.toml` vermerkten Ausnahmen.
- **Anti-Ziel der Spec:** nie eine stillschweigend falsche Schnittansicht. Jeder Fehler (Kernel, zu feine Schraffur, verbotene Marke) ergibt eine deutsche Meldung und sperrt die Ausgabe; nie Schraffur über Löchern.
- Dateiformat 8 nur, wenn eine Zeichnung eine Schnittansicht hat (`required_format`); ohne Schnitte schreibt das Programm Format-1-bis-7-Dateien Byte für Byte wie bisher.
- Bestehende Golden-Dateien unverändert: `svg_without_marks_{box,cylinder}.svg`, `dxf_without_marks_{box,cylinder}.dxf`, `svg_balloons_a4_a0.svg`, `svg_splines.svg`, `pdf_cylinder_a4_landscape.pdf`, `pdf_box_frame_a3_portrait.pdf`, `stage15_sheet_lines.txt`.
- Farben nur über `theme::` (Wächter `no_hard_coded_colors`), Fenster nur über `dialog::Dialog` (`no_raw_windows`), Textfelder nur über `theme::text_field*` (`no_raw_text_edits`), Zeichen nur aus Montserrat/Roboto/Phosphor (`tests/fonts.rs`, „–“ ist erlaubt); gleiche Kartenhöhe in einer Reihe; Oberfläche über die ganze Breite.
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests mit `-j 2` **im Vordergrund** (langsamer Rechner; nie `cargo clean`; keine Hintergrundprozesse zurücklassen).
- **Tests müssen beißen:** Jede Aufgabe endet mit mindestens **10 Mutationen** (einzeln einspielen, roten Test notieren, zurücknehmen); Bericht „Mutation → roter Test“. Bleibt eine Mutation grün, wird der Test schärfer.
- Keine Streudateien: Testdateien in `std::env::temp_dir()`-Unterordnern, am Ende entfernt; Proben nur über einen ausgeschalteten Test nach `D:\Testumgebung\zeichnung-proben\schnitt\`; `git status` am Ende sauber.
- Abweichungen von der Spec stehen im Ledger des Controllers, am Ende in `docs/superpowers/offene-punkte.md` und im Abschnitt „Abweichungen bei der Umsetzung“ dieses Plans.

**Festlegungen des Plans (S1–S12; der Nutzer kann sie kippen):**
- **S1 Ebene:** `SectionPlane { kind, offset }`. `kind` ist die Art der Schnittansicht (Normansicht), `d = Basis::of(kind).d` ist ± eine Weltachse `a`; die Ebene ist `p[a] = offset` (Weltkoordinate, unabhängig von der Blickrichtung). Behalten wird `d[a]·(p[a] − offset) ≥ 0` (Material hinter der Ebene, vom Betrachter aus); „Blickrichtung umdrehen“ tauscht `kind` gegen die Gegenansicht, `offset` bleibt.
- **S2 Schneidekörper (Spec §9.1):** Hüllbox `B` des Körpers (Ecken und 64 Punkte je Kante wie `stroke::extent_of`). Quer zur Achse: Mitte von `B`, Seitenlänge `2·max(Größe, 1)`; längs der Achse von der Ebene in Richtung `−d` mit Länge `2·max(Größe_a, 1) + |offset − Mitte_a|`. `make_box` → `transformed(Verschiebung)` → `assign_origin(section_origin(i))` mit `section_origin(i) = u64::MAX − i` (i = Index des Körpers unter den exakten Körpern). Schnittflächen sind die Flächen mit Herkunft ≥ `SECTION_ORIGIN_MIN = u64::MAX − 65 535` (Spec §9.1: an der Marke, nicht an der Lage). Trägt ein Eingangskörper schon eine Marke in diesem Bereich → Fehler (S10), nie ein falsches Ergebnis.
- **S3 Ganz vor/hinter, Tangentialfall (Spec §7, §9.2):** Liegt `B` ganz auf der behaltenen Seite (einschließlich Berührung) → `Kept` ohne Kernel; ganz auf der entfernten Seite (einschließlich Berührung) → `Removed` ohne Kernel. Liegt eine Ecke des Körpers näher als 1e-4 mm an der Ebene, wird vor dem Schneiden um `SECTION_NUDGE_MM = 0.001` zur behaltenen Seite verschoben (`offset + d[a]·0.001`) und die Einordnung wiederholt. Scheitert `boolean` sonst (außer `EmptyResult` → `Removed`, `InvalidInput` → Fehler), ein zweiter Versuch mit Verschiebung; scheitert auch er → Fehler. Das Ergebnis merkt `nudged`.
- **S4 Ansicht und Linien:** `compute_section` schneidet jeden Körper, behält die nicht entfernten in Eingangsreihenfolge (`source[k]` = ursprünglicher Index), ruft die übergebene Ansichtsrechnung (`compute_view` oder die Test-Ersatzrechnung der App) mit den geschnittenen Körpern auf. Gibt es nirgends eine Schnittfläche → `DrawError::SectionMisses`.
- **S5 Blickrichtung aus der Linie:** Waagerechte Linie → Pfeile nach unten (Blattrichtung `(0, −1)`), senkrechte → nach rechts (`(1, 0)`), umgedreht jeweils entgegengesetzt; die Art ist die Normansicht mit `d = v.x·P.ex + v.y·P.ey` (P = Quellansicht). Damit: Vorne/waagerecht → Oben, Vorne/senkrecht → Links, umgedreht Unten/Rechts; Oben/waagerecht → Hinten, Oben/senkrecht → Links; Rechts/waagerecht → Oben, Rechts/senkrecht → Vorne. Im Erstwinkel liegt die Schnittansicht so in Pfeilrichtung.
- **S6 Lage der Linie in der Quellansicht:** waagerecht bei Ansichts-y `= offset · P.ey[a]`, senkrecht bei x `= offset · P.ex[a]`; Länge = Hüllbox der Quellansicht ± 5 mm × f Überstand.
- **S7 Schraffur:** in Blattkoordinaten nach dem Abbilden (Abstand = `spacing · f` unabhängig vom Ansichtsmaßstab). Linienschar `{q : q·n = c₀ + k·s}`, `n = (−sin α, cos α)`, `c₀ = xf.apply(0, 0)·n` (gemeinsame Phase aller Flächen eines Körpers). Winkel nach Rang des Körpers unter den Körpern mit Schnittfläche: gerade 45°, ungerade 135°. Rand als Vieleck mit höchstens 0,005 mm Abweichung auf dem Blatt. Höchstens `MAX_HATCH_LINES = 20 000` Schraffurstücke je Ansicht, sonst Meldung „Die Schraffur von „Schnitt A–A“ ist zu fein.“ (Ausgabe gesperrt).
- **S8 Schnittlinie (Marken der Quellansicht, Spec §5, §9.5):** Kette als `MarkKind::SectionLine(SectionPen::Chain)` mit `CENTER_PATTERN`, an beiden Enden 8 mm × f als `SectionPen::End` (0,5 mm wie sichtbare Kanten); an jedem Ende E Schaft (Chain) von E bis `E + v·5f`, Pfeil `Mark::Arrow { tip: E + v·10f, dir: −v }` mit eigener Größe `SECTION_ARROW_LEN = 5.0`, `SECTION_ARROW_HALF_W = 1.0` (mal f); Buchstabe mittig mit Mitte `tip + v·(DIM_TEXT_H/2 + f)`, Grundlinie Mitte − (0, h/2), Höhe `DIM_TEXT_H`, Winkel 0.
- **S9 Titel:** Text `"{L}–{L} ({Maßstab})"` (Spec §5: „A–A“, Maßstab dahinter), Höhe 5 mm × f, mittig 5 mm × f unter der Unterkante der Schnittansicht, Art `SectionLine(Chain)`.
- **S10 Meldungen:** neue `DrawError::SectionMisses` („Die Schnittebene trifft den Körper nicht.“) und `DrawError::SectionFailed(String)` („Bei „{0}“ lässt sich der Schnitt nicht berechnen.“). Der Kernel-Grund bleibt im Test sichtbar (`cut_body` liefert ihn als `String`), nicht in der Oberfläche.
- **S11 Buchstaben, Zahl, Abstand:** `next_letter` nimmt den ersten freien Buchstaben der Reihe (frei gewordene werden wieder vergeben). `HatchSpec.spacing` ist der A3-Wert (Vorgabe 3,0); gültig, wenn endlich, `spacing · f ≥ 0,5` und `spacing ≤ 50`. Ein Blattwechsel, der einen Abstand unter 0,5 mm brächte, wird mit der Meldung „Der Schraffurabstand von „Schnitt A–A“ wäre auf diesem Blatt kleiner als 0,5 mm.“ abgelehnt.
- **S12 Lage der Schnittansicht:** abhängig (`Placement::Aligned`) von der Quellansicht mit deren Maßstab; Abstand = `default_distance(..)`, liegt schon ein Kind derselben Quellansicht in derselben Ausrichtung, dahinter: `max(Vorgabe, weitestes Geschwister + Ausdehnung der neuen Ansicht · f + GAP_MM)`. Ändert sich der Maßstab der Quellansicht, übernehmen ihre Schnittansichten ihn in derselben Änderung.

## Review Focus

1. **Ebene genau auf einer Fläche oder Kante** (Mitte der Hüllbox trifft eine Stufenfläche, die Naht eines Zylinders, eine Bohrungsachse): Schnitt gelingt mit Verschiebung, Fläche stimmt, Hinweis im Ansichtsfeld, keine Meldung „lässt sich nicht berechnen“. → Aufgabe 1 Test (h), Aufgabe 5 Test (j), Aufgabe 9 Test (f).
2. **Baugruppe mit sich berührenden Teilen** (Bolzen in Bohrung einer Platte, beide geschnitten): zwei Körper, 45° und 135°, keine Schraffurlinie läuft über die Grenze in das andere Teil, ein Spalt bleibt frei. → Aufgabe 6 Test (d).
3. **Modell ändert sich danach** (Bohrung gelöscht, Teil gekürzt, sodass die Ebene außerhalb liegt): Ansicht zeigt „Die Schnittebene trifft den Körper nicht.“, Export gesperrt mit Ansichtsnamen; Teil wieder länger → Schnitt kommt zurück, keine alten Linien. → Aufgabe 8 Test (d).
4. **Blatt- oder Maßstabswechsel bei vorhandenen Schnitten** (A3 → A4 mit Abstand 0,7, A3 → A0, Maßstab 1:2 → 5:1): Abstand auf dem Blatt folgt f und nie dem Maßstab; zu fein → Ablehnung mit Meldung, nie stille Änderung. → Aufgabe 4 Test (e), Aufgabe 6 Test (a), Aufgabe 9 Test (g).
5. **Hohlkörper zerfällt in mehrere Stücke** (Rohr längs geschnitten, U-Profil, Platte mit zwei Bohrungen): alle Stücke schraffiert, jedes Loch frei, Schraffurlänge je Zeile = Handrechnung. → Aufgabe 1 Tests (c, e, f), Aufgabe 3 Tests (d, g), Aufgabe 6 Test (c).

---

### Task 1: Kernel-Schnitt beweisen (`section.rs`, Teil 1)

**Files:**
- Create: `crates/planora-drawing/src/section.rs` (Modulkommentar mit S1–S3)
- Modify: `crates/planora-drawing/src/lib.rs` (`pub mod section;`)
- Test: `crates/planora-drawing/tests/stage28_section_cut.rs` (neu; Hilfe `cut_area(solid) -> f64`: Schleifen der Schnittflächen mit 512 Punkten je gekrümmter Kante abtasten, Gaußsche Trapezformel in den beiden Achsen quer zu `a`, äußere minus innere Schleifen)

**Interfaces:**
- Consumes: `planora_kernel::{boolean::boolean, BoolOp, BooleanError, primitives::make_box, Solid, Face, FaceTag, Id}`, `Basis`, `ViewKind`, `planora_math::{Tolerance, DMat4}`.
- Produces:
```rust
pub const SECTION_ORIGIN_MIN: u64 = u64::MAX - 65_535;
pub const SECTION_NUDGE_MM: f64 = 0.001;
pub fn section_origin(body: usize) -> u64;                     // u64::MAX - body
#[derive(Clone, Copy, Debug, PartialEq)] pub struct SectionPlane { pub kind: ViewKind, pub offset: f64 }
impl SectionPlane { pub fn axis(&self) -> Option<(usize, f64)> } // (a, d[a]); None für Isometrisch/nicht endlich
pub enum CutOutcome { Kept, Removed, Cut { solid: Solid, nudged: bool } }
pub fn solid_bounds(solid: &Solid) -> Option<(DVec3, DVec3)>;
pub fn cut_body(solid: &Solid, body: usize, plane: &SectionPlane, tol: &Tolerance) -> Result<CutOutcome, String>;
pub fn cut_faces(solid: &Solid) -> Vec<Id<Face>>;             // Herkunft ≥ SECTION_ORIGIN_MIN, aufsteigend
```

- [ ] **Step 1: Failing tests** (Körper über `AnalyticKernel`, alle mit `assign_origin(7)` vorher):
  (a) `box_cut_in_half` (Quader 20×10×5, `Front` offset 4: `Cut`, `nudged == false`, `cut_faces().len() == 1`, Fläche 100 ±1e-9, jede Ecke der Schnittfläche bei y = 4 ±1e-9, Hüllbox des Ergebnisses y ∈ [4, 10], `validate` ok, alle übrigen Flächen Herkunft 7);
  (b) `cylinder_through_axis` (`make_cylinder(5, 10)`, `Front` offset 0: `nudged` ist genau dann wahr, wenn eine Ecke des Zylinders näher als 1e-4 an y = 0 liegt (im Test nachgezählt); eine Schnittfläche, Fläche 100 ±1e-5);
  (c) `box_with_bore` (Quader 30×20×10 minus Zylinder r 4 bei (15, 10) durch z, `Front` offset 10: zwei Schnittflächen, Summe 220 ±1e-3, kein Punkt der Schnittflächen mit |x − 15| < 4 − 1e-6);
  (d) `tube_across` (Rohr r 10/6, h 20, `Top` offset 10: eine Schnittfläche mit einer inneren Schleife, Fläche 64π ±1e-3, behalten z ≤ 10);
  (e) `tube_along` (dasselbe Rohr, `Front` offset 0 → zwei Schnittflächen, je 80 ±1e-3);
  (f) `groove_body` (Platte 40×20×10 minus Nut x ∈ [16, 24], z ∈ [5, 10] über die ganze Tiefe, `Front` offset 10: eine Schnittfläche, U-Form mit 8 Kanten, Fläche 360 ±1e-9);
  (g) `whole_side_without_kernel` (Quader y ∈ [0, 10], `Front`: offset −1 und 0 → `Kept`; 10 und 11 → `Removed`; `Back` offset 11 → `Kept`);
  (h) `plane_on_a_step_face` (Vereinigung [0,20]×[0,10]×[0,5] und [0,10]×[0,10]×[5,10], `Top` offset 5 → `nudged == true`, Fläche 200 ±1e-9);
  (i) `reserved_tag_is_refused` (eine Fläche mit Herkunft `u64::MAX − 3` → `Err`, Text nicht leer; Herkunft `SECTION_ORIGIN_MIN − 1` → geht);
  (j) `bad_input` (leerer `Solid::new()` → `Err`; `Isometric` → `Err`; offset NaN → `Err`);
  (k) `two_bodies_keep_apart` (zwei Quader, Körper 0 und 3 geschnitten: Schnittflächen-Herkunft `u64::MAX` bzw. `u64::MAX − 3`, `cut_faces` findet je nur die eigene).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage28_section_cut -j 2`
- [ ] **Step 3: Implement** nach S1–S3 und Interfaces; Toleranz `Tolerance::default()` im Aufrufer. Schlägt (b)–(f) oder (h) auch mit Verschiebung fehl: **stoppen und melden** (Global Constraints), nicht den Kernel ändern.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-drawing --lib -j 2`, clippy, fmt, `git diff --stat 7f1e319 -- crates/planora-kernel` leer.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: behaltene Seite vertauscht; Verschiebung zur falschen Seite; Ecken-Probe 1e-2 statt 1e-4; kein zweiter Versuch; `EmptyResult` als Fehler; Quader nur 1× Hüllbox (schneidet zu wenig); `assign_origin` vergessen; `cut_faces` nach Lage statt Marke; Herkunft für alle Körper gleich; Prüfung vorhandener Marken entfernt; Berührung bei `offset = max` als `Cut`.
- [ ] **Step 6: Commit** — `feat: Körper an einer Ebene schneiden (Schnittansichten, Kernel unverändert)` (Pfade: `section.rs`, `lib.rs`, `stage28_section_cut.rs`).

### Task 2: Randschleifen der Schnittflächen als Vielecke

**Files:**
- Modify: `crates/planora-drawing/src/section.rs`
- Test: `crates/planora-drawing/tests/stage29_section_region.rs` (neu)

**Interfaces:**
- Consumes: Task 1; `stroke::{Stroke, StrokeOrigin}`, `Basis`.
- Produces:
```rust
#[derive(Clone, Debug)] pub struct SectionRegion { pub body: usize, pub loops: Vec<Vec<Stroke>> } // je Schleife Kanten in Laufrichtung
pub fn section_regions(cut: &Solid, body: usize) -> Vec<SectionRegion>;      // eine je Schnittfläche
pub fn loop_polygon(lp: &[Stroke], basis: &Basis, tol: f64) -> Vec<DVec2>;   // Ansichtskoordinaten, ohne doppelten Schlusspunkt
pub fn polygon_area(poly: &[DVec2]) -> f64;                                  // vorzeichenbehaftet (gegen Uhrzeigersinn positiv)
```
Rückwärts durchlaufene Halbkanten bekommen `t0`/`t1` vertauscht; `StrokeOrigin::Edge { body, edge }` mit dem Kantenindex. Abtastung: Strecke 1 Stück; Kreis/Ellipse `n = ceil(Spanne / (2·acos(1 − tol/r_max)))`, mindestens 1; Freiform gleichmäßig 64 Stücke, dann halbieren bis Sehnenabstand ≤ tol (höchstens 4096 Punkte je Kante).

- [ ] **Step 1: Failing tests:** (a) `box_region_is_ccw_in_view` (Quader aus 1 (a), Basis `Front`: eine Region, eine Schleife, `polygon_area == 100` ±1e-9 und positiv); (b) `tube_region_has_hole` (Rohr quer, Basis `Top`, tol 1e-4: zwei Schleifen, Flächen +100π und −36π je ±0,01, Summe 64π ±0,01; jeder Punkt auf r 10 bzw. 6 ±1e-12; größter Sehnenabstand ≤ 1e-4); (c) `u_profile_loop` (Nut-Körper: eine Schleife mit 8 Ecken, Fläche 360 ±1e-9); (d) `two_pieces_two_regions` (Rohr längs: zwei Regionen je +80 ±1e-5); (e) `body_index_kept` (`section_regions(&cut, 4)` → alle `body == 4`, Strich-Herkunft `body == 4`); (f) `tolerance_controls_points` (Kreis r 10: tol 1e-2 weniger Punkte als 1e-4; tol ≤ 0 oder NaN → wie 1e-6, nie unendliche Schleife).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage29_section_region -j 2`
- [ ] **Step 3: Implement** nach Interfaces.
- [ ] **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Rückwärtskanten nicht umgedreht; innere Schleifen ausgelassen; Schlusspunkt doppelt; Kreisformel mit `acos(1 − tol·r)`; Fläche ohne Vorzeichen; Region je Körper statt je Fläche; Basis mit `ey`/`ex` vertauscht; Freiform ohne Verfeinerung; `body` fest 0.
- [ ] **Step 6: Commit** — `feat: Schnittflächen als Schleifen und Vielecke`.

### Task 3: Schraffur-Zuschnitt (`hatch.rs`, rein 2D)

**Files:**
- Create: `crates/planora-drawing/src/hatch.rs`
- Modify: `crates/planora-drawing/src/lib.rs` (`pub mod hatch;`)
- Test: `crates/planora-drawing/tests/stage30_hatch.rs` (neu)

**Interfaces:**
- Produces:
```rust
pub const MAX_HATCH_LINES: usize = 20_000;
#[derive(Clone, Debug, PartialEq)] pub enum HatchError { BadInput, TooMany(usize) }
/// Gerade-ungerade-Regel über alle Kanten aller Vielecke (geschlossen, Reihenfolge und Umlaufsinn egal).
/// Schnitt mit einer Kante zählt halboffen auf der Normalenkoordinate (s0 ≤ c < s1 oder s1 ≤ c < s0);
/// Stücke kürzer als 1e-9 entfallen; Ergebnis je Linie nach Lage sortiert, Linien nach k aufsteigend.
pub fn hatch_lines(polys: &[Vec<DVec2>], angle: f64, spacing: f64, origin: DVec2, max: usize)
    -> Result<Vec<(DVec2, DVec2)>, HatchError>;
```

- [ ] **Step 1: Failing tests** (Ursprung, wo nicht anders genannt, (0, 0,5), Winkel 0):
  (a) `square` ([0,10]², Abstand 3 → genau 4 Stücke bei y 0,5/3,5/6,5/9,5, je von x 0 bis 10 exakt);
  (b) `square_45` (Winkel 45°, Abstand 2, Ursprung (0,0): 7 Stücke, Gesamtlänge `Σ_{k=−3..3} (10 − 2√2·|k|)·√2` ±1e-9, jede Endung auf dem Rand ±1e-12);
  (c) `concave_l` (L (0,0),(10,0),(10,4),(4,4),(4,10),(0,10): Längen 10, 10, 4, 4);
  (d) `square_with_hole` (Loch [3,7]², beide Umlaufsinne: y 0,5 → [0,10]; 3,5 → [0,3] und [7,10]; 6,5 → [0,3] und [7,10]; 9,5 → [0,10]);
  (e) `two_disjoint` ([0,4]² und [6,10]×[0,4]: je Zeile 2 Stücke);
  (f) `through_a_vertex` (Raute (5,0),(10,5),(5,10),(0,5), Ursprung (0,5), Abstand 5: nur das Stück (0,5)–(10,5), keine Stücke der Länge 0);
  (g) `circle_with_hole_rows` (Kreis r 10 und r 4 als Vielecke mit Sehnenabstand ≤ 1e-6, Abstand 1, Ursprung (0,0,5): Länge je Zeile = `2√(100−y²) − 2√(16−y²)` für |y| < 4, sonst `2√(100−y²)`, ±1e-5; innerhalb |x| < `√(16−y²)` − 1e-5 kein Stückpunkt);
  (h) `limits` (Abstand 0, negativ, NaN, Winkel NaN → `BadInput`; [0,1000]², Abstand 0,01 → `TooMany(n)` mit n > `MAX_HATCH_LINES` ohne alle Stücke zu bauen (Zählung zuerst); leere Liste → leer; Vieleck mit < 3 Punkten wird übergangen).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage30_hatch -j 2`
- [ ] **Step 3: Implement** nach Interfaces (Zählung der Stücke vor dem Bauen, damit `TooMany` billig ist).
- [ ] **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: geschlossene statt halboffene Zählung; Paare (1,2) statt (0,1); keine Sortierung; Schlusskante des Vielecks vergessen; Normale statt Richtung; Abstand nicht geprüft; kurze Stücke behalten; Grenze `>=` statt `>`; Ursprung ignoriert.
- [ ] **Step 6: Commit** — `feat: Schraffurlinien gegen Flächen mit Löchern zuschneiden`.

### Task 4: Datenmodell, Prüfung, Namen und Dateiformat 8

**Files:**
- Modify: `crates/planora-drawing/src/model.rs` (`SectionSpec`, `HatchSpec`, `ViewSpec.section`, `ViewKey`, `SECTION_LETTERS`, `MAX_SECTIONS`, `Drawing::{next_letter, add_section, view_names}`, `section_kind`, `section_line_coord`, `plane_offset_from_view`), `crates/planora-drawing/src/lib.rs` (Re-Exports), `crates/planora-drawing/src/sheet.rs` (Namen aus `Drawing::view_names`), alle 8 `ViewSpec { … }`-Literale (`section: None`)
- Modify: `crates/planora-feature/src/drawings.rs` (`validate_drawing`: Schnitte), `crates/planora-project/src/{format.rs,convert.rs}` (`SchnittDto`, `FORMAT_VERSION = 8`, `required_format`, Lesen/Schreiben, Sperre „erst ab Dateiformat 8“), `crates/planora-app/src/drawing_view.rs` (`view_names` ruft `Drawing::view_names`)
- Test: `crates/planora-drawing/tests/stage31_section_model.rs` (neu), `crates/planora-feature/tests/drawings.rs` (neue Fälle), `crates/planora-project/tests/section_format.rs` (neu); anpassen siehe „Bestehende Tests, die sich ändern“

**Interfaces:**
- Consumes: `SectionPlane` (1), `Basis`, `align_dir`, `sheet_scale`.
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq)] pub struct HatchSpec { pub on: bool, pub spacing: f64 }   // Vorgabe { true, 3.0 } (A3-mm)
#[derive(Clone, Copy, Debug, PartialEq)] pub struct SectionSpec { pub offset: f64, pub letter: char, pub hatch: HatchSpec }
pub struct ViewSpec { /* … */ pub section: Option<SectionSpec> }
pub const SECTION_LETTERS: &str = "ABCDEFGHJKLMNPRSTUVWXYZ";
pub const MAX_SECTIONS: usize = 12;
pub const MIN_HATCH_SHEET_MM: f64 = 0.5; pub const MAX_HATCH_SPACING: f64 = 50.0;
#[derive(Clone, Copy, Debug, PartialEq)] pub enum ViewKey { Plain(ViewKind), Section(SectionPlane) }
impl ViewKey { pub fn of(spec: &ViewSpec) -> ViewKey }           // Schraffur gehört nicht zum Schlüssel (Spec §9.10)
impl SectionSpec { pub fn plane(&self, kind: ViewKind) -> SectionPlane }
pub fn section_kind(parent: ViewKind, horizontal: bool, flipped: bool) -> Option<ViewKind>;   // S5
pub fn section_line_coord(parent: ViewKind, plane: &SectionPlane) -> Option<(bool, f64)>;   // S6: (waagerecht?, Ansichtskoordinate)
pub fn plane_offset_from_view(parent: ViewKind, kind: ViewKind, coord: f64) -> Option<f64>;  // Umkehrung von S6
impl Drawing {
    pub fn next_letter(&self) -> Option<char>;
    pub fn add_section(&mut self, parent: ViewId, kind: ViewKind, offset: f64, distance: f64) -> Result<ViewId, String>; // Maßstab der Quelle, show_hidden false, centerlines true
    pub fn view_names(&self) -> BTreeMap<ViewId, String>;    // Normansichten wie bisher (Schnitte zählen nicht mit), Schnitte „Schnitt A–A“
}
pub fn sections_ok_on(d: &Drawing, sheet: Sheet) -> Result<(), String>; // S11-Prüfung des Abstands für ein Blatt
// planora-project
pub struct SchnittDto { pub versatz: f64, pub buchstabe: String, pub schraffur: bool, pub schraffurabstand: f64 }
pub struct AnsichtDto { /* … */ #[serde(default, skip_serializing_if = "Option::is_none")] pub schnitt: Option<SchnittDto> }
```
Prüfregeln in `validate_drawing` (Meldungen wörtlich): Schnittansicht braucht `Aligned` und eine Quelle, die selbst eine Normansicht ohne Schnitt ist („Ein Schnitt geht nur aus einer geraden Ansicht.“); `kind` normal und `d_K · d_P == 0` („Die Schnittebene passt nicht zur Ausgangsansicht.“); Maßstab = Quelle („Eine Schnittansicht hat den Maßstab ihrer Ausgangsansicht.“); Buchstabe aus `SECTION_LETTERS` und eindeutig („Ein Schnittbuchstabe ist ungültig oder doppelt.“); höchstens 12 („Es sind höchstens 12 Schnitte je Blatt möglich.“); `|offset| ≤ MAX_POS_MM`, endlich („Die Lage der Schnittebene ist ungültig.“); Abstand nach S11 („Der Schraffurabstand von „Schnitt A–A“ wäre auf diesem Blatt kleiner als 0,5 mm.“ bzw. bei > 50 „Der Schraffurabstand ist zu groß (höchstens 50 mm).“). Keine Buchstaben frei → „Es sind keine Schnittbuchstaben mehr frei.“

- [ ] **Step 1: Failing tests:** (a) `section_kind_pins` (alle in S5 genannten Paare, `Isometric` → `None`); (b) `line_coord_roundtrip` (Vorne/Oben offset 5 → `(true, 5.0)`; Vorne/Links offset −3 → `(false, −3.0)`; Oben/Hinten offset 7 → `(true, 7.0)`; `plane_offset_from_view` kehrt jeden Fall exakt um); (c) `letters_and_names` (drei Schnitte → A, B, C; B löschen → nächster B; 12 Schnitte → 13. `Err`; Namen: „Vorne“, „Oben“, „Schnitt A–A“, zweite Oben-Ansicht „Oben 2“ unbeeinflusst von Schnitten; `build_sheet` nutzt dieselben Namen); (d) `view_key` (Schnitt mit anderem Abstand → gleicher Schlüssel; anderer `offset` → anderer; Normansicht → `Plain`); (e) `validate_sections` (jede Regel oben einzeln verletzt → genau die Meldung; A3 Abstand 0,7 ok, auf A4 (f 0,707143) → Ablehnung mit dem Ansichtsnamen; `update_drawing` mit Blattwechsel A3→A4 wird abgelehnt, Dokument unverändert); (f) `format_8_only_with_sections` (Zeichnung ohne Schnitt → `required_format` wie bisher (7 bzw. kleiner), Text ohne `"schnitt"`; mit Schnitt → 8, Rundreise gleich (`offset` 12,5, Buchstabe „B“, Schraffur aus, Abstand 4); letzter Schnitt gelöscht → wieder ≤ 7); (g) `format_errors` (Format 7 mit `schnitt` → beschädigt, Meldung enthält „erst ab Dateiformat 8“; Buchstabe „I“, „AB“, leer → beschädigt mit Zeichnungsname; Format 9 → `TooNew { found: 9 }`); (h) `format_7_files_load_unchanged` (Fixtur-Dateien aus `crates/planora-project/tests/fixtures` laden wie vorher, neu geschrieben byte-gleich).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage31_section_model -j 2`, `cargo test -p planora-feature --test drawings -j 2`, `cargo test -p planora-project --test section_format -j 2`
- [ ] **Step 3: Implement** nach Interfaces; danach die bestehenden Formatertests anpassen (Liste unten) und `cargo test -p planora-project -p planora-feature -j 2` grün.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-drawing -j 2`, `cargo test -p planora-app --test drawing_state --test drawing_browser --test parts_sheet -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: I/O/Q zugelassen; erster statt erster freier Buchstabe; `required_format` 8 bei jeder Zeichnung; Schnitt in Format 7 geschrieben; Prüfung `d_K · d_P` fehlt; Maßstab nicht geprüft; 13 Schnitte erlaubt; Abstand gegen A3 statt Blatt geprüft; Schraffur im `ViewKey`; Schnitte zählen in „Oben 2“ mit; Umkehrung mit falschem Vorzeichen.
- [ ] **Step 6: Commit** — `feat: Schnittansichten im Modell und Dateiformat 8` (Pfade einzeln, inkl. angepasster Tests).

### Task 5: Schnittansicht rechnen (`compute_section`)

**Files:**
- Modify: `crates/planora-drawing/src/section.rs`, `crates/planora-drawing/src/error.rs` (S10), `crates/planora-drawing/src/lib.rs` (`pub use section::{compute_section, SectionLines, SectionData}`)
- Test: `crates/planora-drawing/tests/stage32_section_view.rs` (neu); `error.rs`-Test um die zwei Meldungen erweitern

**Interfaces:**
- Consumes: 1, 2, 4; `compute_view`, `DrawBody`, `Progress`.
- Produces:
```rust
pub type ViewFn = fn(&[DrawBody], ViewKind, &Progress) -> Result<ViewLines, DrawError>;  // gleiche Form wie app::ComputeFn
#[derive(Clone, Debug)] pub struct SectionData {
    pub regions: Vec<SectionRegion>,   // body = ursprünglicher Index
    pub solids: Vec<Solid>,            // geschnittene bzw. behaltene Körper, Reihenfolge = Striche der Ansicht
    pub source: Vec<usize>,            // solids[k] stammt von bodies[source[k]]
    pub nudged: bool,
}
pub struct SectionLines { pub lines: ViewLines, pub data: SectionData }
pub fn compute_section(bodies: &[DrawBody], plane: &SectionPlane, view: ViewFn, progress: &Progress) -> Result<SectionLines, DrawError>;
// DrawError::SectionMisses, DrawError::SectionFailed(String)
```

- [ ] **Step 1: Failing tests:** (a) `tube_top_section` (Rohr, `Top` offset 10: `lines.kind == Top`, unter den sichtbaren Strichen zwei Kreise r 10 und 6 in z = 10; `regions.len() == 1` mit 2 Schleifen; `source == [0]`; `nudged == false`); (b) `misses` (offset 30 und −5 → `Err(SectionMisses)`, Text „Die Schnittebene trifft den Körper nicht.“); (c) `one_behind_one_cut` (Quader ganz hinter der Ebene + Rohr geschnitten: `source == [0, 1]`, Regionen nur `body == 1`, Linien enthalten den ganzen Quader); (d) `removed_bodies_drop_out` (Körper 0 ganz vor der Ebene, Körper 1 geschnitten → `source == [1]`, Strich-Herkunft `body == 0` bezieht sich auf `solids[0]`); (e) `kernel_failure_names_the_body` (Körper „Körper 2“ mit reservierter Marke → `Err(SectionFailed("Körper 2"))`, Text „Bei „Körper 2“ lässt sich der Schnitt nicht berechnen.“); (f) `view_fn_error_passes` (Ersatzrechnung liefert `TooComplex(5)` → derselbe Fehler); (g) `cancel` (abgebrochener `Progress` → `Cancelled`, auch zwischen zwei Körpern); (h) `bore_visible_after_cut` (Quader mit Bohrung, `Front` offset 10: mindestens ein Stück der Bohrungskante ist `Visible`; in der gewöhnlichen Vorne-Ansicht ist es `Hidden`); (i) `no_bodies` (leer → `NoBodies`; offset NaN → `NonFinite`); (j) `nudge_reported` (Stufenkörper aus 1 (h) → `nudged == true`).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage32_section_view -j 2`
- [ ] **Step 3: Implement** nach S4, S10 und Interfaces; Fortschritt: Schneiden zählt einen Schritt je Körper vor der Ansichtsrechnung.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-drawing -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: `Kept`-Körper weggelassen; `source` mit neuem statt altem Index; Regionen mit neuem Index; `SectionMisses` nur wenn alle entfernt (statt keine Schnittfläche); Ansicht mit ungeschnittenen Körpern; Abbruch nicht geprüft; Name aus falschem Körper; `nudged` nur vom letzten Körper; Fehler der Ersatzrechnung verschluckt.
- [ ] **Step 6: Commit** — `feat: Schnittansicht aus geschnittenen Körpern rechnen`.

### Task 6: Schraffur, Schnittlinie und Titel als Markierungen

**Files:**
- Create: `crates/planora-drawing/src/section_marks.rs` (Modulkommentar mit S6–S9)
- Modify: `crates/planora-drawing/src/annotate.rs` (`MarkKind::Hatch`, `MarkKind::SectionLine(SectionPen)`, `SectionPen`, Konstanten, `arrow_size`), `crates/planora-drawing/src/lib.rs`
- Test: `crates/planora-drawing/tests/stage33_section_marks.rs` (neu; Ansichtslinien über `compute_view`/`compute_section` echter Körper, `build_sheet`)

**Interfaces:**
- Consumes: 2, 3, 4, 5; `sheet::{view_transform, build_sheet}`, `sheet_scale`, `DIM_TEXT_H`, `CENTER_PATTERN`, `DIM_LINE_W`.
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SectionPen { Chain, End }
// MarkKind::Hatch, MarkKind::SectionLine(SectionPen)
pub const SECTION_END_MM: f64 = 8.0; pub const SECTION_OVERHANG_MM: f64 = 5.0; pub const SECTION_SHAFT_MM: f64 = 5.0;
pub const SECTION_ARROW_LEN: f64 = 5.0; pub const SECTION_ARROW_HALF_W: f64 = 1.0;
pub const SECTION_TITLE_H: f64 = 5.0; pub const SECTION_TITLE_GAP: f64 = 5.0; pub const HATCH_FLAT_TOL: f64 = 0.005;
pub const SECTION_END_W: f64 = 0.5;                                  // Stift der Endstücke (wie sichtbare Kanten)
pub fn arrow_size(kind: MarkKind, sheet_scale: f64) -> (f64, f64);   // SectionLine: (5f, 1f), sonst (ARROW_LEN, ARROW_HALF_W)
pub fn section_title(letter: char, scale: Scale) -> String;          // „A–A (1:2)“
pub fn section_view_name(letter: char) -> String;                    // „Schnitt A–A“
#[derive(Clone, Debug, PartialEq)] pub struct SectionMarks { pub view: ViewId, pub marks: Vec<Mark> }
pub fn section_marks(drawing: &Drawing, results: &BTreeMap<ViewId, &ViewLines>,
                     sections: &BTreeMap<ViewId, &SectionData>) -> (Vec<SectionMarks>, Vec<String>); // Fehler: S7-Meldung je Ansicht
pub fn add_section_marks(model: &mut SheetModel, list: &[SectionMarks]);
```
Schraffur je Schnittansicht mit Ergebnis und `hatch.on`: Vielecke über `loop_polygon` mit `tol = HATCH_FLAT_TOL / Maßstab`, mit `view_transform` aufs Blatt, je Region des Körpers `hatch_lines` (Winkel, Ursprung nach S7, `max = MAX_HATCH_LINES − bisher`). Schnittlinie in der Quellansicht, wenn sie gezeichnet wird; Titel unter der Schnittansicht. Ansichten ohne Ergebnis: nichts. Verlorene Quelle: nichts.

- [ ] **Step 1: Failing tests:** (a) `hatch_follows_sheet_not_scale` (Rohr-Schnitt, A3, Maßstab 1:2 und 5:1: Abstand benachbarter Schraffurlinien 3,0 ±1e-9; A4 (f = 297/420): 2,121429 ±1e-6; A0 (f = 1189/420): 8,492857 ±1e-6; alle Schraffurstriche haben die Art `Hatch`); (b) `spacing_and_switch` (Abstand 5 → 5·f; `on = false` → keine `Hatch`-Marke, Schnittlinie und Titel bleiben); (c) `holes_stay_free_rows` (Rohr quer, A3 1:1, Winkel 45°: jede Schraffurlinie hat höchstens 2 Stücke; kein Stückpunkt näher als 6 − 0,01 mm an der Rohrmitte auf dem Blatt; Rohr längs: Stücke nur in den zwei Streifen, Länge je Zeile = Handrechnung ±0,01); (d) `touching_bodies_alternate` (Platte mit Bohrung r 5 und Bolzen r 5 darin, beide geschnitten: Platte 45°, Bolzen 135°; kein Platten-Stück innerhalb r 5 − 0,01, kein Bolzen-Stück außerhalb r 5 + 0,01; zwei Regionen derselben Platte liegen auf derselben Linienschar (gleiche `c`-Werte mod Abstand ±1e-9)); (e) `section_line_geometry` (Quelle Vorne, Schnitt Oben offset 5, A3, Maßstab 1:1: Linie waagerecht auf Blatt-y = Bild von z = 5; Länge = Hüllbreite + 2·5; zwei `End`-Stücke je 8; Kette dazwischen `Chain`; zwei Pfeile mit `dir == (0, 1)` und Spitze 10 unter den Enden; zwei Texte „A“ Höhe 3,5 mittig unter den Spitzen; auf A0 alle Längen mal 2,830952 (Überstand, Endstücke, Schaft, Pfeil), Texthöhe 3,5); (f) `title` („A–A (1:2)“ Höhe 5·f, Mitte x = Mitte der Ansicht, Grundlinie 5·f + 5·f unter der Unterkante; Name „Schnitt A–A“ in `SheetView.name`); (g) `too_fine` (künstlich 1000 Regionen bzw. Abstand an der Grenze: über `MAX_HATCH_LINES` → Fehlerliste enthält „Die Schraffur von „Schnitt A–A“ ist zu fein.“, für diese Ansicht keine Hatch-Marke); (h) `missing_results` (Schnittansicht ohne Ergebnis → keine Marken; Quelle nicht gezeichnet → keine Schnittlinie, Titel bleibt); (i) `arrow_size` (`SectionLine` auf A4 → (3,535714, 0,707143) ±1e-6, `Dimension` → (3,5, 0,6)).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage33_section_marks -j 2`
- [ ] **Step 3: Implement** nach S6–S9 und Interfaces. Neue `MarkKind`s in allen erschöpfenden `match` der Schreiber vorläufig wie `Dimension` behandeln (Aufgabe 7 macht es richtig); bestehende Goldens bleiben gleich.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-drawing -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Abstand mal Maßstab; Abstand ohne f; Winkel nicht wechselnd; Rang nach Index statt nach Körpern mit Schnittfläche; Ursprung je Region; Pfeile entgegen der Blickrichtung; Linie an `−offset`; Endstücke ohne f; Buchstabenhöhe mal f; Titel über statt unter der Ansicht; `on` ignoriert; Grenze nicht geprüft.
- [ ] **Step 6: Commit** — `feat: Schraffur, Schnittlinie und Titel als Markierungen`.

### Task 7: Ausgabe in SVG, DXF und PDF mit Golden-Dateien

**Files:**
- Modify: `crates/planora-drawing/src/{svg.rs,dxf.rs,pdf.rs}` (Moduldoku ergänzen)
- Create: `crates/planora-drawing/tests/fixtures/{svg_section_tube.svg,dxf_section_tube.dxf,pdf_section_tube.pdf}`
- Test: `crates/planora-drawing/tests/stage34_section_output.rs` (neu)

**Interfaces:**
- Consumes: 6; `common::{svg_read, dxf_read, pdf_read}`.
- Produces (Ausgaberegeln):
  - **SVG:** je Ansicht nach den bisherigen Gruppen zwei weitere, nur wenn vorhanden: `„{Ansicht} Schraffur“` (`Hatch`) und `„{Ansicht} Schnitt“` (`SectionLine`, Linien, Pfeile, Buchstaben, Titel). `Hatch`: `DIM_LINE_W`, ohne Muster; `Chain`: `DIM_LINE_W` mit `CENTER_PATTERN`; `End`: `SECTION_END_W`; Pfeile mit `arrow_size`.
  - **DXF:** Ebenen nur bei Verwendung: `SCHRAFFUR` (7, `CONTINUOUS`, 25), `SCHNITTLINIE` (7, `CENTER`, 25), `SCHNITTLINIE_DICK` (7, `CONTINUOUS`, 50); `LTYPE CENTER` auch, wenn nur die Schnittlinie sie braucht; `SOLID` der Pfeile mit `arrow_size`.
  - **PDF:** gleiche Reihenfolge und Stifte wie SVG (jedes SVG-Element ein PDF-Element, wie 3c-3 P3).
  - Golden-Blatt: Rohr r 20/8, h 40 (Achse z), A4 quer, Vorne frei bei (100, 120) 1:1, Schnitt A–A (Oben, offset 20) abhängig, Mittellinien an, ohne Rahmen; PDF-Meta `title "Golden"`, `author Some("Prüfer Ä")`, `date Some("2026-10-19")`.

- [ ] **Step 1: Failing tests:** (a) `svg_groups_and_pens` (Gruppen „Schnitt A–A Schraffur“ und „Vorne Schnitt“ vorhanden, Breiten 0,25/0,25 mit Muster 12 3 2 3/0,5; Pfeildreiecke 5 lang, 2 breit; ohne Schnitte keine der Gruppen); (b) `dxf_layers` (die drei Ebenen mit Linientyp und Stärke; ohne Schnitte nicht vorhanden; ohne Mittellinien, aber mit Schnittlinie gibt es `CENTER` genau einmal; `SOLID` der Pfeile 5 lang); (c) `pdf_same_as_svg` (Element für Element wie `stage27_pdf_svg::same_content_as_svg`: Zahl, Reihenfolge, Endpunkte ±1e-3 mm, Breiten, Muster, Texte inkl. „A–A (1:1)“); (d) `golden_files` (drei Fixturen byte-gleich; Erzeugen einmalig mit `PLANORA_UPDATE_SECTION_GOLDEN=1`, im Commit begründet; Fixturen bestehen die Leser); (e) `old_goldens_unchanged` (alle bisherigen Fixturen byte-gleich).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage34_section_output -j 2`
- [ ] **Step 3: Implement** nach Interfaces; Goldens erst erzeugen, wenn (a)–(c) grün sind.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-drawing -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Schraffur in Gruppe „Maße“; `End` 0,25; Kette ohne Muster; DXF-Ebene immer angelegt; `CENTER` doppelt; Pfeil mit `ARROW_LEN`; PDF-Reihenfolge Schnitt vor Schraffur; „–“ in DXF als `?`; Golden-Byte geändert.
- [ ] **Step 6: Commit** — `feat: Schnittansichten in SVG, DXF und PDF` (Pfade einzeln, inkl. drei Fixturen; `.gitattributes` deckt `*.svg`, `*.dxf`, `*.pdf` schon ab).

### Task 8: App – rechnen, zeigen, exportieren

**Files:**
- Modify: `crates/planora-app/src/drawing_state.rs` (Einträge mit `ViewKey`, `MAX_CACHED = 7 + MAX_SECTIONS`, Schnittaufträge mit `compute_section(.., self.compute, ..)`, `result_of`, `section_of`, `results_for` je Ansicht), `crates/planora-app/src/drawing_view.rs` (`place_views` mit `result_of`, `sheet_model`/Zwischenspeicher mit `section_marks` + `add_section_marks` und den Daten-Zeigern im Schlüssel, `paint_mark`/`line_width_mm` für `Hatch`/`SectionLine` mit `arrow_size`, Hinweiszeile mit Fehlern aus `section_marks`), `crates/planora-app/src/export_dialog.rs` (`prepare_drawing_parts`: Normarten wie bisher, dann je Schnittansicht `compute_section` über `compute_watched`-Muster; Meldung „Die Ansicht „Schnitt A–A“ lässt sich nicht berechnen: …“; Fehler aus `section_marks` sperren mit der Meldung)
- Test: `crates/planora-app/tests/section_view.rs` (neu); `tests/drawing_state.rs` (Kommentar/Wert `MAX_CACHED`)

**Interfaces:**
- Consumes: 4–7.
- Produces:
```rust
impl DrawingUi {
    pub fn result_of(&self, spec: &ViewSpec, model: u64) -> ViewResult;          // nach ViewKey::of(spec)
    pub fn section_of(&self, spec: &ViewSpec, model: u64) -> Option<Arc<SectionData>>;
    // result(kind, model) bleibt für Plain-Schlüssel (bestehende Tests)
}
```

- [ ] **Step 1: Failing tests** (Hilfen wie `tests/drawing_state.rs`/`sheet_view.rs`: Rohr-Dokument, Zeichnung A3 mit Vorne frei, Schnitt über `Drawing::add_section` per `update_drawing`):
  (a) `section_computes_in_background` (nach `tick(block)` `result_of` = `Ready`, `section_of` hat 1 Region; nur der Schraffurabstand geändert → kein neuer Auftrag (`spawned` gleich); `offset` geändert → genau ein neuer Auftrag; Vorne bleibt zwischengespeichert);
  (b) `sheet_shows_hatch_line_title` (gezeichnetes Blatt (`last_sheet`) hat Hatch-Marken in „Schnitt A–A“, Schnittlinie in „Vorne“, Titel; `DrawingLayout.marks_drawn` zählt sie; Ansicht „Schnitt A–A“ liegt in Pfeilrichtung unter Vorne (Erstwinkel));
  (c) `failure_shows_message` (Ersatzrechnung über `set_compute`, die für `Top` scheitert → Platzhalter mit der Meldung; Ebene außerhalb → „Die Schnittebene trifft den Körper nicht.“);
  (d) `model_change_follows` (Rohr auf h 5 gekürzt, Ebene z 20 → Meldung „trifft … nicht“, Export-Status `Failed("Die Ansicht „Schnitt A–A“ lässt sich nicht berechnen: Die Schnittebene trifft den Körper nicht.")`, keine Datei; zurück auf h 40 → wieder Schraffur, kein altes Ergebnis);
  (e) `export_svg_dxf_pdf` (SVG/DXF/PDF der Zeichnung enthalten Schraffur- und Schnittgruppen bzw. -ebenen; Inhalt == `section_marks` des eigenen Blatts; „Alle Blätter“ mit einem Schnittblatt gelingt);
  (f) `too_fine_blocks_export` (Ersatz-Daten mit > `MAX_HATCH_LINES` → Hinweiszeile zeigt die Meldung, Export gesperrt mit derselben Meldung);
  (g) `hidden_bodies_not_cut` (zweiter Körper ausgeblendet → `section_of(..).source` nennt nur den sichtbaren);
  (h) `guards` (`no_hard_coded_colors`, `fonts`, `no_raw_windows`, `no_raw_text_edits` grün – im Schritt 4 mitlaufen lassen).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test section_view -j 2`
- [ ] **Step 3: Implement** nach Interfaces; jede Stelle, die heute `result(v.kind, model)` für eine Ansicht der Zeichnung fragt (`place_views`, `results_for`, `suggest_for`, `prepare_drawing_parts`), fragt jetzt nach der Ansicht.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test drawing_state --test sheet_view --test drawing_ui --test export_drawing --test export_pdf --test export_dimensions --test parts_sheet --test no_hard_coded_colors --test fonts -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Schlüssel nur `kind` (Schnitt zeigt Normansicht); Schraffurabstand im Schlüssel (rechnet neu); `MAX_CACHED` 7 (verdrängt); Schnitt mit Originalkörpern; Export ohne Schnittmarken; Fehler aus `section_marks` ignoriert; Ersatzrechnung nicht an `compute_section` durchgereicht; Zwischenspeicher des Blatts ohne Daten-Zeiger (alte Schraffur bleibt); Pfeil mit `ARROW_LEN` gemalt.
- [ ] **Step 6: Commit** — `feat: Schnittansichten im Zeichnungsbereich und im Export`.

### Task 9: Werkzeug „Schnitt“, Ansichtsfeld, Löschen, Browser

**Files:**
- Create: `crates/planora-app/src/section_tool.rs` (Muster `balloon_tool.rs`)
- Modify: `crates/planora-app/src/{lib.rs,state.rs}` (`pub section_tool: SectionTool`), `crates/planora-app/src/drawing_view.rs` (Knopf in Zeile 1 nach „Ansicht hinzufügen“, Klicks, Vorschau, Esc, Ansichtsfeld der Schnittansicht, Klick auf die Schnittlinie wählt die Schnittansicht, `Action::Scale` überträgt auf Schnittkinder), `crates/planora-app/src/drawing_browser.rs` (Rückfrage für Schnittansichten), `crates/planora-app/src/icons.rs` (`Icon::SectionTool` = Phosphor `scissors`; Codepunkt aus Phosphor-Light nachschlagen)
- Test: `crates/planora-app/tests/section_tool.rs` (neu); `tests/icons.rs` falls die Symbolliste gezählt wird

**Interfaces:**
- Consumes: 4, 8; `theme::{text_field, Toggle}`, `sketch_view::parse_number`, `drawing_state::default_distance`.
- Produces:
```rust
pub const TOOL: &str = "Schnitt";
pub const TOOL_TIP: &str = "Zwei Klicks in einer geraden Ansicht legen die Schnittlinie fest. Daraus entsteht eine Schnittansicht.";
pub const HINT_FIRST: &str = "Anfang der Schnittlinie in einer geraden Ansicht anklicken. Esc beendet.";
pub const HINT_SECOND: &str = "Ende der Schnittlinie anklicken. Die Linie rastet waagerecht oder senkrecht ein.";
pub const NOT_FROM_HERE: &str = "Ein Schnitt geht nur aus einer geraden Ansicht.";
pub const TOO_SHORT: &str = "Bitte zwei verschiedene Punkte anklicken.";
pub const FULL: &str = "Es sind höchstens 12 Schnitte je Blatt möglich.";
pub const NUDGED: &str = "Die Schnittebene lag genau auf einer Fläche oder Kante und wurde um 0,001 mm verschoben.";
pub const SNAP_PX: f32 = 8.0;
pub struct SectionTool { /* active, start: Option<(ViewId, DVec2)>, message */ }
pub fn blocked(drawing: &Drawing, selected: Option<&ViewSpec>, editable: bool) -> Option<&'static str>; // alte Version, Stücklistenblatt, keine gerade Ansicht ohne Schnitt, gewählte Ansicht ist Schnitt/Isometrie, 12 erreicht
pub fn snap(start: DVec2, end: DVec2) -> bool;                       // waagerecht, wenn |dx| ≥ |dy|
pub fn create(state: &mut AppState, drawing: DrawingId, parent: ViewId, horizontal: bool, coord: f64) -> Result<ViewId, String>; // eine Dokumentänderung
pub fn flip(state: &mut AppState, drawing: DrawingId, view: ViewId) -> Result<(), String>;      // Gegenansicht, offset bleibt
```
Ablauf: erster Klick in der Hüllbox einer Normansicht ohne Schnitt (sonst `NOT_FROM_HERE`) merkt Ansicht und Punkt; die quer liegende Koordinate rastet auf die Mitte der Hüllbox ein, wenn sie auf dem Bildschirm näher als `SNAP_PX` liegt (Spec §2: Standard durch die Mitte). Vorschau (Farbe `theme`-Akzent) mit Pfeilen in Blickrichtung. Zweiter Klick: Richtung nach `snap`, Abstand < 1 mm → `TOO_SHORT`; `create` → Schnittansicht nach S5/S12 gewählt. Ansichtsfeld einer Schnittansicht: Kopf „Schnitt A–A“, Maßstab gesperrt (Tooltip „Der Maßstab folgt der Ausgangsansicht.“), Feld „Schnittposition (mm)“ (`theme::text_field`, Übernahme mit Enter/Fokusverlust, ungültig → rote Meldung, Dokument unverändert), „Blickrichtung umdrehen“, Schalter „Schraffur“, Feld „Schraffurabstand (mm)“, verdeckte Kanten/Mittellinien wie bisher, bei `nudged` der Hinweis `NUDGED`. Rückfrage beim Löschen einer Schnittansicht: „Die Schnittansicht „Schnitt A–A“ wird gelöscht, mit ihr die Schnittlinie in „Vorne“.“ (immer mit Rückfrage); beim Löschen der Quelle nennt die vorhandene Rückfrage „Schnitt A–A“ unter den abgeleiteten Ansichten.

- [ ] **Step 1: Failing tests:** (a) `two_clicks_make_a_section` (Werkzeug an, Klicks in Vorne waagerecht → eine Änderung des Dokuments (`edit_count` + 1), Ansicht `Top` mit Buchstabe A, `Aligned`, Maßstab der Quelle, `show_hidden == false`; Klick nahe der Mitte → `offset` = Mitte der Hüllbox exakt; senkrecht → `Left`; Esc nach dem ersten Klick → nichts angelegt); (b) `blocked_cases` (Isometrie gewählt, Schnittansicht gewählt, Stücklistenblatt, alte Version, 12 Schnitte → Knopf gesperrt mit der passenden Erklärung; Klick in Isometrie → `NOT_FROM_HERE`); (c) `letters_in_order` (drei Schnitte A, B, C; B gelöscht, neuer → B); (d) `properties_edit` (Schnittposition „12,5“ → `offset 12.5`, neuer Auftrag; „abc“ → Meldung, unverändert; Umdrehen → `Bottom`, Ansicht wandert auf die andere Seite; Schraffur aus → keine Hatch-Marken; Abstand „0,3“ auf A3 → Ablehnung mit S11-Meldung); (e) `delete_both_ways` (Schnittansicht löschen → Rückfrage mit Wortlaut, danach Schnittlinie in Vorne weg; Vorne löschen → Rückfrage nennt „Schnitt A–A“, beide weg; Klick auf die Schnittlinie wählt „Schnitt A–A“); (f) `nudge_note` (Stufenkörper, Schnitt durch die Stufe → Ansichtsfeld zeigt `NUDGED`); (g) `scale_and_sheet` (Maßstab von Vorne 1:2 → Schnitt ebenfalls 1:2 in derselben Änderung; Blattwechsel A3 → A4 mit Abstand 0,6 → Meldung aus S11, Blatt unverändert); (h) `save_load` (Dokument mit Schnitt speichern → Format 8, laden → gleiche Zeichnung und Schraffur; Schnitt löschen, speichern → Format 7); (i) `window_sizes` (Ansichtsfeld bei 1100 × 700 und 800 × 600 ohne Überlauf, Werkzeugzeile passt wie bisher; Hell und Dunkel ohne feste Farben).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test section_tool -j 2`
- [ ] **Step 3: Implement** nach Interfaces.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test drawing_ui --test drawing_browser --test balloon_tool --test dimension_tool --test shell_layout --test side_field_width --test icons --test no_hard_coded_colors --test no_raw_text_edits --test no_raw_windows --test fonts -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Rasten auf Mitte fehlt; `snap` mit `>` vertauscht; zwei Dokumentänderungen; Schnitt aus Isometrie erlaubt; Knopf bei gewählter Schnittansicht frei; Umdrehen ändert `offset`; Abstand ohne Prüfung übernommen; Löschen ohne Rückfrage; Maßstab nicht übertragen; Esc legt trotzdem an; Klick auf Schnittlinie wählt Vorne.
- [ ] **Step 6: Commit** — `feat: Werkzeug Schnitt mit Ansichtsfeld und Löschen`.

### Task 10: Maße und Positionsnummern in Schnittansichten

**Files:**
- Modify: `crates/planora-feature/src/dimensions.rs` (`resolve_drawing_dimensions_views`), `crates/planora-app/src/dimension_state.rs` (Auflösung und Vorschlag mit den Körpern der Ansicht; Schlüssel des Zwischenspeichers um die `SectionData`-Zeiger), `crates/planora-app/src/dimension_tool.rs` (Werkzeugkontext mit den Körpern der getroffenen Ansicht), `crates/planora-app/src/balloon_tool.rs` (Körperindex über `SectionData.source`), `crates/planora-app/src/export_dialog.rs` (Auflösung im Arbeitsthread mit den geschnittenen Körpern)
- Test: `crates/planora-feature/tests/dimensions.rs` (neue Fälle), `crates/planora-app/tests/section_dims.rs` (neu)

**Interfaces:**
- Consumes: 5, 8, 9.
- Produces:
```rust
// planora-feature
pub fn resolve_drawing_dimensions_views<E>(d: &Drawing, solids: &[&Solid], view_solids: &BTreeMap<ViewId, Vec<&Solid>>,
    check: impl FnMut() -> Result<(), E>) -> Result<(ResolvedDims, HashMap<DimId, String>), E>;
// Maße einer Schnittansicht ohne Eintrag in view_solids (noch nicht berechnet) fehlen in by_view (werden nicht als verloren gezeigt);
// resolve_drawing_dimensions(_keyed) = dasselbe mit leerer Karte (Verhalten unverändert).
// planora-app
impl DrawingUi { pub fn view_solids(&self, spec: &ViewSpec, model: u64) -> Option<Arc<SectionData>> } // = section_of
```
Spec §9.3: Schnittansichten übernehmen keine Maße der Quelle; „Maß setzen“ und „Maße vorschlagen“ arbeiten auf den geschnittenen Körpern (Bezüge auf Schnittflächen tragen die Marke aus S2).

- [ ] **Step 1: Failing tests:** (a) `resolve_against_cut_solids` (feature: Quader mit Bohrung, Schnitt Vorne offset 10; Maß „waagerecht“ zwischen den Kanten der beiden Schnittflächen an der Bohrung → 8 ±1e-9 mit `view_solids`, ohne Karte für diese Ansicht → nicht in `by_view`; dasselbe Maß in Vorne mit Originalkörpern unverändert aufgelöst); (b) `tool_in_section_view` (App: Werkzeug „Maß“, zwei Klicks auf die Bohrungskanten der Schnittansicht → Maß angelegt, Wert 8; Klick auf eine Schnittkante liefert einen Bezug mit Schnittmarke); (c) `suggest_in_section_view` (gewählte Schnittansicht → Vorschläge vorhanden, alle lösen sich gegen die geschnittenen Körper auf; keine Kopie der Maße von Vorne); (d) `balloon_in_section_view` (zwei Körper, Körper 0 ganz vor der Ebene, Klick auf Körper 1 in der Schnittansicht → Positionsnummer zeigt auf Körper 1 (`BodyKey` des ursprünglichen Index), Nummer stimmt mit Stückliste); (e) `export_resolves_section_dims` (SVG mit Maß in der Schnittansicht wird geschrieben, Maßtext „8“; Bohrung gelöscht → Maß verloren → Export gesperrt mit der bekannten Meldung, die „Schnitt A–A“ nennt); (f) `pending_section_no_lost_flash` (während der Schnitt rechnet, zeigt die Hinweiszeile kein verlorenes Maß).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test dimensions -j 2`, `cargo test -p planora-app --test section_dims -j 2`
- [ ] **Step 3: Implement** nach Interfaces.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test dimension_state --test dimension_tool --test dimension_view --test export_dimensions --test balloon_tool --test parts_sheet -j 2`, `cargo test -p planora-feature -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Schnittansicht mit Originalkörpern aufgelöst; fehlende Karte als verloren; Schlüssel ohne Daten-Zeiger (alte Auflösung bleibt); Positionsnummer mit neuem statt ursprünglichem Index; Vorschlag kopiert Maße der Quelle; Export löst ohne Schnittkörper auf; Werkzeugkontext immer `state.bodies`.
- [ ] **Step 6: Commit** — `feat: Maße und Positionsnummern in Schnittansichten`.

### Task 11: Proben, Dokumentation, Gesamtprüfung, Handliste

**Files:**
- Create: `crates/planora-drawing/tests/hand_samples_section.rs` (`#[ignore]`; Ziel `$PLANORA_SECTION_SAMPLES_DIR`, sonst `D:\Testumgebung\zeichnung-proben\schnitt`)
- Modify: `README.md` (Abschnitt Zeichnungen: Schnittansichten in Laiensprache, Grenzen), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt 3d – Schnittansichten“: Geliefertes, S1–S12, Grenzen: nur Vollschnitt, nur Hauptebenen, Netzkörper ausgelassen, Rippen/Wellen werden schraffiert, Normmaße unbelegt, Schraffur je Blatt neu gerechnet; Handliste), `docs/superpowers/specs/2026-10-19-schnittansichten-design.md` (neuer §11 „Festlegungen der Umsetzung“: S1–S12 kurz), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)

- [ ] **Step 1: Proben erzeugen** (`cargo test --release -p planora-drawing --test hand_samples_section -- --ignored`, Vordergrund): je SVG, DXF, PDF: Rohr quer und längs (A4 quer), Quader mit zwei Bohrungen und Nut (A3, Maßstab 1:2, mit Maßen in der Schnittansicht), Platte mit Bolzen (zwei Körper, 45°/135°), Stufenkörper mit verschobener Ebene, A0 mit drei Schnitten A, B, C, Blatt mit Rahmen.
- [ ] **Step 2: Doku** in Laiensprache; **Handliste** für den Nutzer: Schnitt mit zwei Klicks in Vorne waagerecht und senkrecht ziehen (Einrasten auf die Mitte, Vorschau mit Pfeilen), Schnittansicht erscheint in Pfeilrichtung, Löcher bleiben frei, Buchstaben A, B, C; Schnittposition eintippen, Blickrichtung umdrehen, Schraffur aus/an, Abstand ändern; Modell ändern (Bohrung löschen) → Ansicht folgt; Ebene außerhalb → Meldung; Schnittansicht und Quelle löschen → Rückfragen; Maß in der Schnittansicht setzen; Speichern/Laden; Proben in Inkscape, Fusion (DXF) und zwei PDF-Betrachtern öffnen (Schraffur dünn, Kontur dick, Schnittlinie Strich-Punkt mit dicken Enden, Pfeile, „A–A (1:2)“); Hell/Dunkel, 800 × 600.
- [ ] **Step 3: Gates** mit Ausgabe: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2`; `cargo deny check`; `git diff --stat 7f1e319 -- crates/planora-kernel crates/planora-mesh/src crates/planora-step crates/planora-exchange` leer; `git diff 7f1e319 -- '**/Cargo.toml' Cargo.lock deny.toml` leer; `git status` sauber.
- [ ] **Step 4: Mutationsbeweise:** entfällt für reine Doku; dafür die Summenliste aller Mutationen der Aufgaben 1–10 im Bericht (mindestens 100 Einträge).
- [ ] **Step 5: Commit** — `docs: Schnittansichten (3d), Festlegungen, Handliste, Proben`.

### Task 12 (Controller): unabhängige Prüfung, echtes Fenster, Haltepunkt

- [ ] **Step 1:** Proben mit Poppler (`pdfinfo`, `pdftotext -layout`, `pdftoppm -r 100 -png`) und als Bild ansehen: Schraffur nur auf Material, Löcher frei, Winkel wechseln je Teil, Schnittlinie und Titel richtig; Befunde ins Ledger, Korrektur an die zuständige Aufgabe.
- [ ] **Step 2 (nur mit Freigabe des Nutzers):** Release bauen (`-j 2`, Vordergrund), App starten, mit `tools/gui-drive.ps1` Kernweg (Rohr, Zeichnung, Werkzeug „Schnitt“, zwei Klicks, Ansichtsfeld, Export PDF); Bildschirmfotos ansehen; App beenden, kein Prozess bleibt.
- [ ] **Step 3: Haltepunkt:** Nutzer prüft Proben und Handliste; Gesamtreview des Zweigs, eine Korrekturrunde, Merge-Frage.

---

## Bestehende Tests, die sich ändern

- **`FORMAT_VERSION` 7 → 8:** `crates/planora-project/tests/{dimensions_format.rs:346, drawings_format.rs:323, format2.rs:137, format3.rs:139, parts_format.rs:96, sheet_format.rs:71}`.
- **„Format 8 ist zu neu“ → Format 9:** `dimensions_format.rs:855–863`, `docstore.rs:294, 304`, `drawings_format.rs:743–752`, `format2.rs:151`, `format3.rs:175`, `parts_format.rs:364`, `sheet_format.rs:312` (Zahl im Text und in `TooNew { found: 9 }`).
- **`ViewSpec`-Literale** (`section: None`): `crates/planora-drawing/src/model.rs`, `crates/planora-project/src/convert.rs`, `crates/planora-app/src/drawing_view.rs`, `crates/planora-drawing/tests/{hand_samples_dims.rs, stage18_suggest.rs, stage19_round_arc.rs}`.
- **`MAX_CACHED`:** `crates/planora-app/tests/drawing_state.rs:598–599` (Kommentar und Erwartung 19).
- **Erschöpfende `match` über `MarkKind`:** `svg.rs`, `dxf.rs`, `pdf.rs`, `drawing_view.rs` (Code, keine Testerwartung).
- **Unverändert (muss so bleiben):** alle Golden-Dateien aus den Global Constraints; `required_format` aller heutigen Testdokumente (z. B. `crates/planora-app/tests/parts_sheet.rs` 6/7); alle bisherigen Meldungen.

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung:** §1 Zweck → 5, 6, 8. §2 Vollschnitt an Hauptebene, Standard Mitte → S1, S5, 9 (a); Schraffur 45°, 3 mm × f, dünn, Wechsel je Körper → S7, 6 (a, d); alle Körper an derselben Ebene → 5 (c); Buchstaben, Titel, Maßstab der Quelle → S9, S11, S12, 4 (c), 9 (c, g); SVG/DXF/PDF als Linien → 7; Maße und Mittellinien → 5 (Mittellinien neu berechnet), 10. Nicht-Ziele unberührt. Anti-Ziel → S3, S7, S10, 3 (d, g), 6 (c, g). §3 Kernel-Schnitt → 1; Schnittfläche → 1, 2; Zuschnitt 2D → 3; Hintergrund mit Abbruch → 5 (g), 8 (a); Kernel-Fehler mit Meldung und Sperre → 5 (e), 8 (c, d). §4 Bedienung → 9; Modelländerung → 8 (d); Löschen beidseitig → 9 (e). §5 Darstellung → S8, S9, 6 (e, f), 7; verdeckte Kanten aus → 4, 9 (a); gleiche Phase → 6 (d). §6 Modell, Format 8, Tests „zu neu“, Buchstaben → 4. §7 Netzkörper → `exact_copies` (unverändert, 8); Tangential → S3, 1 (b, h), 9 (f); 12 je Blatt → 4 (c), 9 (b); Strichgrenze und 0,5 mm → S7, S11, 3 (h), 6 (g), 4 (e). §8 Tests → 1–10, Goldens 7, Handliste 11. §9.1 → S2, 1 (k); 9.2 → S3, 1 (g); 9.3 → 10; 9.4 → 3, 1 (c, e); 9.5 → S8, 7; 9.6 → S7; 9.7 → 4 (e), 9 (b); 9.8 → 8 (g); 9.9 → S7, 6 (d); 9.10 → 4 (d), 8 (a).
**Schritt-Scan:** Testschritte nennen Namen, Eingaben und Zahlen; keine Rümpfe außer festen Wortlauten und Regeln.
**Typkonsistenz:** `SectionPlane`, `cut_body`, `cut_faces`, `section_origin` (1) → 2, 5; `SectionRegion`, `loop_polygon`, `polygon_area` (2) → 5, 6; `hatch_lines`, `MAX_HATCH_LINES` (3) → 6, 8; `SectionSpec`, `HatchSpec`, `ViewKey`, `section_kind`, `section_line_coord`, `plane_offset_from_view`, `Drawing::{add_section, next_letter, view_names}` (4) → 5–10; `compute_section`, `SectionLines`, `SectionData`, `ViewFn` (5) → 8, 10; `SectionPen`, `section_marks`, `add_section_marks`, `arrow_size`, `section_title` (6) → 7, 8, 9; `result_of`, `section_of` (8) → 9, 10.
**Proportion:** Entscheidungen, Zahlen, Schnittstellen, Tests; kein Funktionsrumpf.
**Bekannte Risiken:** Aufgabe 1 entscheidet über alles (Kernel-Grenzen bei Naht und koplanaren Flächen); Aufgabe 10 berührt Auflösung, Werkzeug und Vorschlag der Maße (viele bestehende Tests laufen mit). Empfehlung Controller: opus für 1, 6, 8, 10; sonnet für 2–5, 7, 9, 11.

**Fragen an den Nutzer (ändern, was gebaut wird):**
- **Q1 – Kernel:** Der Plan ändert den Kernel nicht. Scheitert in Aufgabe 1 ein Grundfall auch mit Verschiebung, hält die Umsetzung an und fragt nach (Kernel-Änderung ja/nein).
- **Q2 – Maße in Schnittansichten (Aufgabe 10):** Spec §9.3 verlangt Maß-Werkzeug und Vorschlag auf der geschnittenen Geometrie; das berührt Auflösung, Werkzeug und Vorschlag der Maße. Rückfallweg, falls zu riskant: Werkzeug „Maß“ und „Maße vorschlagen“ in Schnittansichten gesperrt mit Hinweis, Aufgabe 10 als eigenes Folgeprojekt.
- **Q3 – Abstand und Blattwechsel (S11):** Der Schraffurabstand wird als A3-Wert gespeichert; ein Blattwechsel, der ihn unter 0,5 mm brächte, wird abgelehnt (nicht still vergrößert).
- **Q4 – Kleinigkeiten:** Titel „A–A (1:2)“; Vorgabe-Blickrichtung „Pfeile nach unten/rechts“ (S5); frei gewordene Buchstaben werden wieder vergeben; Maßstab der Schnittansicht folgt der Quelle (Feld gesperrt).

## Abweichungen bei der Umsetzung

Stand nach Aufgabe 11. Ausführlich mit Begründung: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 3d – Schnittansichten“, und `docs/superpowers/specs/2026-10-19-schnittansichten-design.md` §11.

- **Aufgabe 1:** Prüfhilfe `cut_area` mit 4096 statt 512 Punkten je gekrümmter Kante; Nut-Schneidekörper im Test bis z = 12.
- **Aufgabe 2:** Obergrenzen: Kreis und Ellipse 65 536 Stücke, Freiformkante höchstens 2049 Punkte (statt 4096); darüber gröbere Vielecke ohne Meldung, praktisch unerreichbar.
- **Aufgabe 3:** Die Zahl in `TooMany` ist beim Bauen nur eine Untergrenze und wird nie angezeigt.
- **Aufgabe 4:** Eine Normansicht unter einer Schnittansicht bleibt im Modell erlaubt, die Oberfläche bietet sie nicht an.
- **Aufgabe 6:** Zusätzliche Meldung „Die Schraffur von „Schnitt A–A“ lässt sich nicht berechnen.“ bei ungültigem Abstand.
- **Aufgabe 7:** Der Schnittpfeil ist auf A4 5·f lang (Blattfaktor), nicht 5 mm. Die Zeichnung in der App (`drawing_view.rs`: Stift der Enden, Kette, Pfeilgröße) hat keinen Test; Handliste.
- **Aufgabe 8:** Verdrängen im Zwischenspeicher über den Auftrag hinaus (`MAX_CACHED = 7 + MAX_SECTIONS`); folgt der Einfügereihenfolge.
- **Aufgabe 9:** Der Knopf „Blattangaben“ ist nur noch ein Symbol (Tooltip „Blattangaben: …“, Menüeintrag bleibt), damit „Schnitt“ bei 1100 × 700 passt. Wählt man eine Schnittansicht, geht das Werkzeug aus. Ein Klick auf die Schnittlinie (6 Pixel) gewinnt gegen die Kanten der Ausgangsansicht. Das Winkelfeld der Spec §4 fehlt absichtlich (S7).
- **Aufgabe 10:** Schnittmarken hängen am Körperindex (`u64::MAX − i`): Löschen eines früheren Körpers macht Maße an Schnittkanten rot und sperrt den Export, nie ein falscher Wert. „Maße vorschlagen“ entfernt Doppelte über die ganze Zeichnung, eine Schnittansicht bekommt also kein Maß, das eine andere Ansicht schon trägt. Eine Positionsnummer in einer Schnittansicht, deren Teil aus dem Schnitt fällt, behält ihre Nummer und der Export läuft (Lücke der Spec). `Document::resolve_dimensions` lässt Schnittmaße weg (`*_views` benutzen). `ResolveFn` hat den zusätzlichen Parameter `&ViewSolids`. Maßtext im Export mit einer Nachkommastelle („8,0“).
- **Aufgabe 11:** Die Proben zeigten, dass der Kernel Ebenen nahe an einer Bohrungsnaht oder auf der Bohrungsachse in manchen Fällen nicht schneiden konnte (Meldung „… lässt sich der Schnitt nicht berechnen.“). Die A0-Probe nahm vorübergehend die Ebene 105 statt 100.
- **Aufgabe 12a (Kernel-Korrektur):** Der Nutzer hat „Kernel reparieren“ gewählt. Die Vorgabe „Kernel unverändert“ ist damit für genau eine Änderung aufgehoben: `crates/planora-kernel/src/boolean/seg.rs`, `Seg::closest` sucht bei ganzen Kreisen und Ellipsen (Anfang = Ende) auch im Abschnitt am anderen Ende und nimmt den näheren Punkt; Freiformkurven bleiben unverändert. Neue Tests `crates/planora-kernel/tests/seg_closest.rs` und die Gefahrenzonen-Matrix in `stage28_section_cut.rs`. Die A0-Probe schneidet wieder bei 100 durch die Bohrungsachse. Alle anderen Kernel-Dateien, Netz, STEP, Austausch und die Abhängigkeiten bleiben ohne Diff.
- Keine Normquellen (ISO 128-40, -44, -50, DIN 6): alle Werte unbelegt. Die Handprüfung (Inkscape, Fusion/DXF, zwei PDF-Betrachter) und der Test im echten Fenster (Aufgabe 12) sind offen.
