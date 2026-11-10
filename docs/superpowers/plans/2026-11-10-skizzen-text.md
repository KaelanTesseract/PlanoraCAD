# Skizzen 8: Text – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Text in der Skizze wie in Fusion: Text im Rahmen und auf Pfad, mit Schriftart, Höhe, Fett/Kursiv, Ausrichtung, Abstand, Winkel und Umkehren, Auflösen in Kurven, Profile mit Innenkonturen, änderbar nach Speichern und Laden, Ersatzschrift mit Hinweis.

**Architecture:** Das neue Crate `planora-text` liest Systemschriften und die mitgelieferten Schriften mit `skrifa`, setzt mit `harfrust` und liefert ein `TextShape`, also Glyphen-Konturen in Einheiten der Höhe. `planora-sketch` hält ein `TextData` je Text in einer eigenen Arena. Daraus legt es echte Kurven an (Linie, Kegelschnitt ρ = 0,5, Kontrollpunkt-Spline Grad 3). Ihre Punkte sind für den Löser Konstanten wie Musterkopien, und `refresh_texts` legt sie nach jedem Lösen aus `TextShape` und Rahmen oder Pfad neu fest. Das Setzen selbst (Schrift → `TextShape`) geschieht nur außerhalb des Lösers über `Sketch::reshape_texts(&dyn TextShaper)`. Fehlt das gesetzte `TextShape` (gerade geladen, Schriftliste noch nicht fertig), bleiben die gespeicherten Kurven unverändert stehen.

**Tech Stack:** Rust 2024, egui/eframe 0.36, `skrifa` 0.44 und `harfrust` 0.12 (beide schon im Lockfile, MIT/Apache), serde.

**Spec:** `docs/superpowers/specs/2026-11-10-skizzen-text-design.md` (Abschnitte A bis G). Jede Aufgabe liest die Spec mit.

## Global Constraints

- Texte nur über `tr!`/`msg!`, Schlüssel in `planora-i18n/languages/de.ftl` und `en.ftl`. Namen wie in Fusion: de „Text“, „Text auf Pfad“, „An Pfad anpassen“, „Text auflösen“, „Schriftart“, „Fett“, „Kursiv“, „Höhe“, „Zeichenabstand“, „Platzierung“, „oberhalb“, „unterhalb“, „Umkehren“, „Ausrichtung“; en „Text“, „Text On Path“, „Fit to Path“, „Explode Text“.
- Keine neue Fremd-Abhängigkeit außer `skrifa = "0.44"` und `harfrust = "0.12"` als direkte Abhängigkeiten von `planora-text`, in genau den Versionen aus `Cargo.lock`. `cargo deny check` sauber, ohne neue Ausnahme. Keine Schriftdatei neu ins Repo, kein Autodesk-Material.
- In die Projektdatei kommen nie Schriftdaten, nur Familienname, Fett/Kursiv und die erzeugten Kurven als normale Einträge.
- Format 18 nur, wenn eine Skizze ein `TextData` enthält, auch ein gelöschtes. Alle anderen Dateien bleiben bytegleich.
- Ein Befehl ist ein Rückgängig-Schritt. Esc, Rechtsklick, Werkzeugwechsel und Abbrechen hinterlassen nichts.
- Leiste bleibt bei `STRIP_TOOLS` = 20 und 813 px.
- Grenzen: Inhalt ≤ 500 Zeichen, ≤ 4000 erzeugte Kurven je Text, Zeichenabstand `0 ≤ p ≤ 1000` %, Höhe > 0.
- Vorgaben: Schrift „Arial“, wenn installiert, sonst „Montserrat“. Höhe 5 mm (Zoll-Dokument 0,2 in). Abstand 0, links, unten, nicht umgekehrt.
- Kursiv nachgebildet: Scherung `tan = 0,2126` (12°). Fett nachgebildet: Verbreitern um 2 % der Höhe (Verfahren `FT_Outline_Embolden`).
- Höhe = Geviert (units per em).
- Tests hängen nie an Systemschriften: Sie benutzen `FontLibrary::bundled_only()` oder eigene Ordner. App-Tests laufen bei 1920×1080.
- Kommentare deutsch. Code-Fragen zuerst mit `graphify query "…"` (Graph in `graphify-out/`). Das gilt auch für Briefings an Subagenten.
- Prüfen je Aufgabe: `cargo fmt -p <crate> -- --check`, `cargo clippy -p <crate> --all-targets -- -D warnings`, Tests des Crates. Bekannte Fehlschläge: `real_step_text_limit` und in WSL als root `a_locked_old_json_never_ends_up_next_to_a_new_png`. Alles andere ist neu und muss geklärt werden. Ein Test wird nur mit sichtbarer Begründung abgeschwächt, entfernt oder auf `#[ignore]` gesetzt.
- Jede Fusion-Frage beim Bauen: Annahme treffen, in `docs/superpowers/offene-punkte.md` unter „## Teilprojekt Skizzen 8“ notieren, nicht anhalten.
- Nach jeder Aufgabe eine Vault-Notiz `D:\Vault\Vault\Projekte\PlanoraCAD\2026-MM-TT Skizzen 8 Task N <Thema>.md` (Geändert/Warum/Offen), verlinkt in `PlanoraCAD.md` unter „Aufgaben-Notizen“.
- Commits enden mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Kein Push.

## Review Focus

1. **Undo und „geändert“ durch den Laufzeit-Zwischenspeicher:** `TextData` trägt ein gesetztes `TextShape`. Steckt es in `PartialEq` oder im Vergleich der Rückgängig-Bilder, erzeugt bloßes Neusetzen nach dem Laden einen Rückgängig-Schritt oder ein „•“ im Tab. Erwartet wird: kein Schritt, kein „•“, wenn die Form gleich bleibt. Test in Task 3 und Task 11.
2. **Unsinnige Schriftdateien:** abgeschnittene `.ttf`, `.ttc` mit kaputtem Eintrag, eine Datei ohne `glyf`/`CFF`, eine Schrift ohne `cmap` für Latein. Erwartet: still übersprungen bzw. Ersatz mit Hinweis, kein Absturz und kein Hängen. Test in Task 1 und Task 2.
3. **Pfad mit Länge fast null oder ein Pfad, der beim Lösen kurz verschwindet** (Kreis mit Radius 0 beim Ziehen, Linie mit gleichen Enden). Erwartet: Der Text bleibt an der letzten guten Lage, es gibt keine NaN-Koordinaten und keinen Absturz. Test in Task 5.
4. **Leerzeichen, Tabulator, nur Zeilenumbrüche, Emoji, kombinierende Akzente, rechts-nach-links:** Erwartet: Leerzeichen geben Vorschub ohne Kurven. Ein Text nur aus Leerraum wird beim OK abgelehnt. Emoji ohne Umriss bleiben leer mit Hinweis. „é“ als e + Akzent wird richtig gesetzt. Test in Task 2.
5. **Rückgängig mitten im Bearbeiten und Speichern während der Abschnitt offen ist:** Erwartet: Strg+Z bei offenem Text-Abschnitt bricht den Abschnitt ab, wie andere Werkzeuge mit Feldern, und nimmt dann den letzten Schritt zurück. Speichern schreibt den Stand ohne die Vorschau. Test in Task 12.

---

## Dateiübersicht

| Datei | Aufgabe |
|---|---|
| `crates/planora-text/` (neu): `lib.rs`, `library.rs`, `face.rs`, `shape.rs`, `outline.rs`, `synth.rs` | Schriftliste, Schnittwahl, Setzen, Umrisse, nachgebildete Schnitte |
| `crates/planora-sketch/src/text_shape.rs` (neu) | reine Datentypen `TextShape`, `TextStyle`, Trait `TextShaper` |
| `crates/planora-sketch/src/text.rs` (neu) | `TextData`, Anlegen, Löschen, Ändern, Auswahl-Hilfen, Auflösen |
| `crates/planora-sketch/src/text_layout.rs` (neu) | Lage der Glyphen im Rahmen und auf Pfad (reine Geometrie) |
| `crates/planora-sketch/src/text_refresh.rs` (neu) | `refresh_texts`, Kurven wiederverwenden (gleiche Ids) |
| `planora-sketch`: `model.rs`, `solver.rs`, `data.rs`, `constraint.rs`, `edit.rs`, `offset.rs`, `fillet.rs`, `helpers.rs`, `pattern.rs`, `transform.rs`, `dimension.rs`, `arrangement.rs`/`intersect.rs`, `profile.rs` | Anschlüsse |
| `planora-project`: `format.rs`, `convert.rs` | Format 18 |
| `planora-feature`: `compute.rs` | Warnung zur fehlenden Schrift |
| `planora-app`: `fonts_library.rs` (neu), `sketch_text.rs` (neu), `sketch_panel.rs`, `sketch_state.rs`, `sketch_view.rs`, `app.rs`, `keymap/defaults.rs`, `icons.rs`, `profile_pick.rs`, `state.rs` | Bedienung |

---

### Task 1: Crate `planora-text` – Schriftliste und Schnittwahl

**Files:**
- Create: `crates/planora-text/Cargo.toml`, `src/lib.rs`, `src/library.rs`, `src/face.rs`
- Modify: `Cargo.toml` (Workspace: `planora-text = { path = "crates/planora-text" }`, `skrifa = "0.44"`, `harfrust = "0.12"` mit Lizenz-Kommentar wie bei den anderen)
- Test: `crates/planora-text/tests/library.rs`

**Interfaces:**
- Produces:
  - `pub struct FontLibrary`
  - `FontLibrary::scan(dirs: &[PathBuf]) -> FontLibrary` (durchsucht rekursiv, überspringt Kaputtes, nimmt immer die mitgelieferten dazu)
  - `FontLibrary::bundled_only() -> FontLibrary`
  - `pub fn system_font_dirs() -> Vec<PathBuf>` (Windows: `%WINDIR%\Fonts`, `%LOCALAPPDATA%\Microsoft\Windows\Fonts`; Linux: `/usr/share/fonts`, `/usr/local/share/fonts`, `~/.local/share/fonts`, `~/.fonts`)
  - `fn families(&self) -> Vec<String>` (alphabetisch, mitgelieferte zuerst)
  - `fn has_family(&self, name: &str) -> bool` (ohne Rücksicht auf Groß- und Kleinschreibung)
  - `fn select(&self, family: &str, bold: bool, italic: bool) -> FaceChoice`
  - `pub struct FaceChoice { pub face: FaceRef, pub synth_bold: bool, pub synth_italic: bool, pub substituted: bool }`
  - `FaceRef` hält Datei-Bytes (`Arc<[u8]>`) und den Index in der Sammlung
  - `pub const FALLBACK_FAMILY: &str = "Montserrat"`
- Mitgelieferte Schriften per `include_bytes!("../../planora-app/assets/fonts/…")`: Montserrat Regular und SemiBold, Roboto Regular. Es wird keine Kopie angelegt.

- [ ] **Step 1: Failing tests schreiben**
  - `bundled_has_montserrat_and_roboto`: `bundled_only().families()` beginnt mit `["Montserrat", "Roboto"]`.
  - `scan_reads_ttf_and_ttc_in_subfolders`: Ein Temp-Ordner bekommt die mitgelieferten Bytes in `a/x.ttf`. `scan` findet die Familie einmal, nicht doppelt.
  - `scan_skips_broken_files`: Eine abgeschnittene Datei (die ersten 100 Bytes von Roboto als `.ttf`), eine leere `.otf` und eine `.ttc` mit Unsinn. `scan` endet ohne Panik, `families()` wie `bundled_only()`.
  - `select_missing_family_falls_back`: `select("Gibtsnicht", false, false)` ergibt `substituted == true` und die Familie Montserrat.
  - `select_bold_uses_semibold_for_montserrat`: `select("Montserrat", true, false)` ergibt den SemiBold-Schnitt und `synth_bold == false`.
  - `select_italic_without_face_is_synthetic`: `select("Roboto", false, true)` ergibt `synth_italic == true`.
  - `family_match_ignores_case`: `has_family("montserrat")`.
- [ ] **Step 2:** `cargo test -p planora-text --test library` → schlägt fehl (Crate fehlt).
- [ ] **Step 3: Umsetzen.** Namen aus `name`-ID 16/17 vor 1/2 lesen, Gewicht und Kursiv aus `OS/2` (`usWeightClass`, `fsSelection` Bit 0) über `skrifa`/`read-fonts`. Die Schnittwahl nimmt das Gewicht am nächsten an 700 bzw. 400, dann Kursiv. Hat eine variable Schrift die Achse `wght` und ist Fett gewählt, gilt `synth_bold = false` und die Achse steht auf 700 (in `FaceRef` als `wght: Option<f32>`).
- [ ] **Step 4:** Tests grün. `cargo deny check` grün. `cargo tree -d -p planora-text` zeigt keine zweite Version von `skrifa` oder `harfrust`.
- [ ] **Step 5:** Commit `Skizzen 8: Crate planora-text mit Schriftliste und Schnittwahl`.

### Task 2: Setzen und Umrisse → `TextShape`

**Files:**
- Create: `crates/planora-sketch/src/text_shape.rs` (in `lib.rs` als `pub mod text_shape`), `crates/planora-text/src/shape.rs`, `src/outline.rs`, `src/synth.rs`
- Modify: `crates/planora-text/Cargo.toml` (Abhängigkeit `planora-sketch` nur für die Datentypen, **Abweichung** zur Spec A10, Grund in `offene-punkte.md`)
- Test: `crates/planora-text/tests/shape.rs`

**Interfaces:**
- Produces (in `planora-sketch::text_shape`, ohne Schrift-Abhängigkeit):
  - `pub struct TextStyle { pub font: String, pub bold: bool, pub italic: bool, pub spacing: f64 }` (Zeichenabstand in %)
  - `pub enum Seg { Line(DVec2), Quad(DVec2, DVec2), Cubic(DVec2, DVec2, DVec2) }` (jeweils die folgenden Punkte, Start ist das Ende des vorigen)
  - `pub struct Contour { pub start: DVec2, pub segs: Vec<Seg> }` (geschlossen)
  - `pub struct Glyph { pub origin: DVec2, pub advance: f64, pub contours: Vec<Contour> }`. Alles in Einheiten der Höhe 1, `origin` relativ zur Zeile.
  - `pub struct ShapedLine { pub glyphs: Vec<Glyph>, pub width: f64 }`
  - `pub struct TextShape { pub lines: Vec<ShapedLine>, pub ascender: f64, pub descender: f64, pub line_height: f64, pub substituted: Option<String>, pub synthetic: bool, pub missing_chars: bool }`
  - `pub trait TextShaper { fn shape(&self, content: &str, style: &TextStyle) -> Result<TextShape, TextShapeError>; }`
  - `pub enum TextShapeError { TooLong, TooManyCurves, Empty }`
- Produces (in `planora-text`): `impl TextShaper for FontLibrary`.

- [ ] **Step 1: Failing tests schreiben** (alle mit `bundled_only()`):
  - `o_has_two_contours`, `i_has_two_contours`, `b_upper_has_three_contours`
  - `quadratics_stay_exact`: Montserrat ist TrueType. Jedes `Quad` abgetastet gegen den `skrifa`-Pen-Weg auf `< 1e-9` (Höhe 1).
  - `implied_on_curve_points_are_explicit`: Kein `Quad` folgt direkt auf ein `Quad` ohne ausdrücklichen Punkt dazwischen.
  - `collinear_quad_becomes_line`, `zero_length_segment_dropped`
  - `kerning_applies`: Breite von „AV“ < Breite von „A“ + Breite von „V“.
  - `spacing_percent_extends_advances_except_last`: Bei `spacing = 50` ist „AB“ um `0,5·adv(A)` breiter.
  - `newline_makes_lines`: „a\nb“ ergibt 2 Zeilen, `line_height` gleich `ascender − descender + lineGap`.
  - `spaces_advance_without_contours`
  - `combining_accent_shapes`: „e\u{0301}“ ergibt Konturen über dem e.
  - `emoji_without_outline_sets_missing_chars`: „a😀“ ergibt `missing_chars == true`, kein Fehler.
  - `missing_family_reports_substitute`: `substituted == Some("Gibtsnicht")`.
  - `synthetic_italic_shears`: Kursiv Roboto, die oberste Kontur ist um `≈ 0,2126 · y` nach rechts verschoben.
  - `synthetic_bold_widens`: Hüllquader breiter um `≈ 0,02` je Seite.
  - `limits`: 501 Zeichen ergibt `TooLong`. „W“ × 500 in einer Schrift mit vielen Stücken ergibt `TooManyCurves`, sobald die Summe der Segmente 4000 übersteigt.
  - `whitespace_only_is_empty`
  - `rtl_text_shapes_without_panic`: hebräischer Text mit Roboto ergibt entweder Konturen oder `missing_chars`.
- [ ] **Step 2:** Tests rot.
- [ ] **Step 3: Umsetzen.** `harfrust` ergibt Glyph-Ids und Lagen. Zeichen ohne Glyphe (`gid 0`) werden mit Roboto neu gesetzt. Fehlen sie auch dort, gilt `missing_chars`. Die Umrisse kommen über `skrifa::outline` mit einem eigenen `OutlinePen`. Teilen durch units per em ergibt Höhe 1. Nachgebildete Schnitte in `synth.rs`.
- [ ] **Step 4:** Tests grün, clippy, fmt.
- [ ] **Step 5:** Commit `Skizzen 8: Text setzen und Umrisse als TextShape`.

### Task 3: Text im Rahmen in der Skizze

**Files:**
- Create: `planora-sketch/src/text.rs`, `text_layout.rs`, `text_refresh.rs`
- Modify: `model.rs` (Arena `texts`, `revision`), `solver.rs` (`refresh_texts` vor und nach dem Lösen, neben `refresh_patterns`), `constraint.rs::remove_curve`/`remove_point` (Ablehnung von Teilen)
- Test: `planora-sketch/tests/text_box.rs` (mit einem Test-Shaper: `struct BoxShaper`, der für jedes Zeichen ein Quadrat mit Loch liefert, damit `planora-sketch` ohne `planora-text` testet)

**Interfaces:**
- Produces:
  - `pub type TextId = Id<TextData>`
  - `pub enum HAlign { Left, Center, Right }`, `pub enum VAlign { Top, Middle, Bottom }`
  - `pub enum TextPlacement { Box { lines: [CurveId; 4], corners: [PointId; 4], h: HAlign, v: VAlign }, Path { path: CurveId, above: bool, fit: bool, h: HAlign } }`
  - `pub struct TextData { pub content: String, pub style: TextStyle, pub height: f64, pub flip_h: bool, pub flip_v: bool, pub placement: TextPlacement, pub curves: Vec<CurveId>, pub points: Vec<PointId>, pub(crate) alive: bool, pub(crate) shaped: ShapedCache }`
  - `ShapedCache(Option<Arc<TextShape>>)`, dessen `PartialEq` immer `true` liefert (**Review Focus 1**)
  - `pub struct TextParams { pub content: String, pub style: TextStyle, pub height: f64, pub flip_h: bool, pub flip_v: bool }`
  - `Sketch::add_text_box(&mut self, a: DVec2, b: DVec2, params: TextParams, h: HAlign, v: VAlign, shape: Arc<TextShape>) -> Result<TextId, SketchError>`. Der Rahmen bekommt 1 × `Perpendicular`, 2 × `Parallel` und `Horizontal` auf der Grundlinie (Ecke 0 → 1).
  - `Sketch::texts(&self) -> impl Iterator<Item = (TextId, &TextData)>`
  - `Sketch::text_of_curve(&self, c: CurveId) -> Option<TextId>` (nur erzeugte Kurven)
  - `Sketch::text_of_frame(&self, c: CurveId) -> Option<TextId>` (Rahmenlinien)
  - `Sketch::remove_text(&mut self, t: TextId) -> Result<(), SketchError>`
  - `Sketch::set_text_construction(&mut self, t: TextId, on: bool)`
  - `Sketch::reshape_texts(&mut self, shaper: &dyn TextShaper) -> ReshapeReport { pub changed: bool, pub substituted: Vec<(TextId, String)>, pub synthetic: Vec<TextId>, pub missing_chars: Vec<TextId> }`
  - `Sketch::is_text_shaped(&self, t: TextId) -> bool`
  - Fehler-Schlüssel: `sketch-text-part`, `sketch-text-on-path`, `sketch-text-empty`, `sketch-text-too-long`, `sketch-text-too-many-curves`
- `refresh_texts` legt die Lage in `text_layout::place_box(shape, height, corners, h, v, flip_h, flip_v) -> Vec<Contour>` fest (Sketch-Koordinaten) und schreibt die Punkte. Ist der Text noch nicht gesetzt (`shaped` leer), wird er übersprungen.
- Erzeugte Kurven: `Line` / `add_conic_as(…, 0.5)` / `add_control_spline_as(…, 3, [0,0,0,0,1,1,1,1])`. Eine Kontur teilt die Endpunkte, der letzte ist der erste.

- [ ] **Step 1: Failing tests:**
  - `box_text_makes_curves_and_frame`: 4 Rahmenlinien sind Konstruktion. Die erzeugten Kurven sind keine Konstruktion. Der Rahmen hat 5 Freiheitsgrade minus `Horizontal`.
  - `box_text_profiles_have_holes`: Mit `BoxShaper` und „ab“ ergeben `profiles()` 2 Profile mit je 1 Loch.
  - `moving_frame_moves_glyphs_keeps_ids`: Ecke ziehen und lösen. Die Kurven-Ids sind gleich, die Punkte verschoben.
  - `height_change_keeps_ids`, `alignment_change_keeps_ids`
  - `content_change_reuses_ids_in_order`: „ab“ → „abc“. Die ersten Ids bleiben, neue kommen dazu.
  - `glyph_points_are_solver_constants`: Ein Fixed auf eine Glyphe wird abgelehnt (`sketch-text-part`).
  - `align_right_top`: Hüllquader rechts bündig an Ecke 1, oben bei `ascender·h` unter der Gegenlinie.
  - `flip_h_mirrors_about_frame_axis`
  - `remove_frame_line_rejected`, `remove_glyph_rejected`, `remove_text_removes_all`: Nach dem Entfernen ist keine Kurve, kein Punkt und keine Abhängigkeit des Textes mehr am Leben.
  - `construction_toggle_whole_text`: Danach gilt `profiles().is_empty()`.
  - `unshaped_text_keeps_curves`: Nach `to_data`/`from_data` ist der Text ohne Reshape. Lösen ändert die Glyphpunkte nicht.
  - `reshape_same_shape_reports_unchanged`: `changed == false`.
  - `text_data_eq_ignores_cache`: zwei `TextData`, die sich nur im Cache unterscheiden, sind gleich.
- [ ] **Step 2:** rot.
- [ ] **Step 3:** umsetzen wie oben.
- [ ] **Step 4:** `cargo test -p planora-sketch` komplett grün, plus clippy und fmt.
- [ ] **Step 5:** Commit `Skizzen 8: Text im Rahmen in der Skizze`.

### Task 4: Format 18

**Files:**
- Modify: `planora-sketch/src/data.rs` (`TextRecord`, `SketchData.texts`, strenge Prüfung), `planora-project/src/format.rs` (`TextDto`, `SketchDto.texte` mit `#[serde(default, skip_serializing_if = "Vec::is_empty")]`), `planora-project/src/convert.rs` (`FORMAT_VERSION = 18`, `needs_format_18`, `has_format_18_data`, Doc-Block an `required_format` ergänzen)
- Test: `planora-project/tests/format18_text.rs`, `planora-sketch/tests/text_data.rs`

**Interfaces:**
- DTO-Felder nach Spec G2: `inhalt, schrift, fett, kursiv, hoehe, abstand, umkehren_waagerecht, umkehren_senkrecht, lage (rahmen{linien, ecken, waagerecht, senkrecht} | pfad{kurve, oberhalb, anpassen, waagerecht}), kurven, punkte, lebt`. Ausrichtungen als `snake_case`-Wörter: `links|mitte|rechts`, `oben|mitte|unten`.
- Fehlerschlüssel für `from_data`: `sketch-err-text-data` mit Grund.

- [ ] **Step 1: Failing tests:**
  - `old_files_stay_byte_identical`: alle vorhandenen Beispieldateien der Formattests einlesen und schreiben, gleiche Bytes und gleiche Formatnummer.
  - `text_needs_format_18`, `deleted_text_still_needs_format_18`
  - `roundtrip_keeps_text_params_and_ids`
  - `format_17_file_with_text_rejected` (`proj-since-format`)
  - `from_data_rejects`: Höhe 0, NaN, Abstand 1001, Rahmen mit 3 Linien, Pfad gleich einer Textkurve, eine Kurve in zwei Texten, 501 Zeichen. Jeweils `Err`, keine Panik.
  - `file_contains_no_font_bytes`: Die geschriebene JSON enthält kein `glyf`, keine Base64-Folge > 1 KiB und keinen Schlüssel außer den genannten.
- [ ] **Step 2–4:** rot, umsetzen, grün (`cargo test -p planora-project -p planora-sketch`).
- [ ] **Step 5:** Commit `Skizzen 8: Format 18 für Text`.

### Task 5: Text auf Pfad

**Files:**
- Modify: `text.rs` (`add_text_path`), `text_layout.rs` (`place_path`)
- Test: `planora-sketch/tests/text_path.rs`

**Interfaces:**
- `Sketch::add_text_path(&mut self, path: CurveId, params: TextParams, above: bool, fit: bool, h: HAlign, shape: Arc<TextShape>) -> Result<TextId, SketchError>`. Ein Pfad, der Textkurve oder Griffkurve ist, ergibt `sketch-text-bad-path`.
- `text_layout::place_path(shape, height, path_geom: &Geom, closed: bool, is_circle_or_arc: bool, above, fit, h, flip_h, flip_v) -> Option<Vec<Contour>>`. `None` heißt: keine gültige Lage, die letzte gute bleibt (**Review Focus 3**).
- Regeln wie in Spec B7: Leserichtung (Kreis und Bogen im Uhrzeigersinn), Bezugspunkt (Kreis oben, sonst Anfang), über das Ende hinaus auf der Tangente bzw. weiter um die Kurve, Anpassen mit gleichen Lücken, auch negativen. Mehrere Zeilen werden mit Leerzeichen verbunden.
- `remove_curve` auf einem Pfad mit Text ergibt `sketch-text-on-path`.

- [ ] **Step 1: Failing tests:**
  - `on_circle_centered_is_upright_on_top`: Glyph-Mitte bei 90°, die y-Achse der Glyphe zeigt nach außen.
  - `on_line_left_starts_at_start`, `right_ends_at_end`, `centered_at_mid`
  - `below_puts_ascender_on_curve`
  - `fit_on_open_path_spans_ends`: erste Glyphe am Anfang, letzte endet am Ende, gleiche Lücken auf `1e-9`.
  - `fit_too_long_overlaps`: negative Lücken, kein Fehler.
  - `beyond_end_follows_tangent`
  - `on_spline_ellipse_conic_fixedspline_works`: je ein Pfad, keine NaN.
  - `glyphs_stay_rigid`: Abstände innerhalb einer Glyphe gleich wie im Shape × Höhe.
  - `path_change_moves_text_keeps_ids`
  - `zero_length_path_keeps_last_good`: Linie auf Länge 0 ziehen. Die Punkte bleiben, alle Koordinaten sind endlich.
  - `deleting_path_rejected`
  - `flip_h_reverses_reading`, `flip_v_swaps_side`
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Text auf Pfad`.

### Task 6: Text ändern und Winkel

**Files:**
- Modify: `text.rs`
- Test: `planora-sketch/tests/text_edit.rs`

**Interfaces:**
- `pub enum TextKindEdit { Box { h: HAlign, v: VAlign }, Path { path: CurveId, above: bool, fit: bool, h: HAlign } }`
- `Sketch::edit_text(&mut self, t: TextId, params: TextParams, kind: TextKindEdit, shape: Arc<TextShape>) -> Result<(), SketchError>`
  - Rahmen → Pfad löscht den Rahmen und die Abhängigkeiten daran.
  - Pfad → Rahmen legt einen Rahmen um die aktuelle Lage mit Winkel 0 und `Horizontal` an.
  - Als Ganzes atomar (über `atomically`).
- `Sketch::text_angle(&self, t: TextId) -> Option<f64>`
- `Sketch::set_text_angle(&mut self, t: TextId, rad: f64) -> Result<(), SketchError>`: Dreht um Ecke 0 und entfernt das interne `Horizontal`. Hält eine Abhängigkeit des Nutzers den Winkel, kommt `sketch-text-angle-held`.

- [ ] **Step 1: Failing tests:** `edit_keeps_ids_when_only_style_changes`, `box_to_path_removes_frame`, `path_to_box_wraps_current_place`, `set_angle_rotates_about_corner0`, `angle_held_by_user_constraint_rejected_unchanged` (Skizze danach gleich `==` vorher), `edit_failure_leaves_sketch_unchanged`.
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Text ändern und Winkel`.

### Task 7: Was am Text hängt (Spec E)

**Files:**
- Modify: `constraint.rs` (`add_constraint`), `dimension.rs`, `transform.rs`, `helpers.rs::check_copy_sources`, `pattern.rs`, `offset.rs`, `edit.rs` (`trim`, `extend`, `break_curve`), `fillet.rs`, `snap.rs` (keine Fangpunkte an Textkurven)
- Test: `planora-sketch/tests/text_dependents.rs`

**Interfaces:**
- Gemeinsame Prüfung `Sketch::refuse_text_curves(&self, curves: &[CurveId], key: &'static str) -> Result<(), SketchError>`
- Neue Schlüssel: `sketch-text-no-dimension`, `sketch-text-no-mirror`, `sketch-text-no-pattern`, `sketch-text-no-offset`, `sketch-text-no-trim`, `sketch-text-no-extend`, `sketch-text-no-break`, `sketch-text-no-fillet`, `sketch-text-no-chamfer`, `sketch-text-path-follows`
- `transform_set` nimmt Texte auf. Text im Rahmen bewegt die Ecken. Bei Text auf Pfad ohne den Pfad in der Auswahl kommt `sketch-text-path-follows`. Kopieren legt einen neuen Text mit gleichen `TextParams` an. Skalieren multipliziert `height` mit dem Faktor.

- [ ] **Step 1: Failing tests:** je Zeile der Tabelle E ein Test, zum Beispiel `dimension_on_glyph_rejected`, `dimension_on_frame_ok`, `mirror_text_rejected`, `pattern_text_rejected`, `offset_text_rejected`, `trim_glyph_rejected`, `trim_other_curve_at_glyph_ok`, `extend_to_glyph_ok`, `fillet_glyph_rejected`, `move_box_text`, `move_path_text_without_path_rejected`, `move_path_text_with_path_ok`, `copy_box_text_makes_second_text`, `scale_box_text_scales_height`, `mirror_frame_lines_only_copies_lines`, `no_snap_points_on_glyphs`. Bei jeder Ablehnung gilt: Skizze `==` vorher und `revision` unverändert.
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Abhängige Befehle am Text`.

### Task 8: Text auflösen

**Files:**
- Modify: `text.rs`
- Test: `planora-sketch/tests/text_explode.rs`

**Interfaces:**
- `Sketch::explode_texts(&mut self, ts: &[TextId]) -> Result<Vec<CurveId>, SketchError>`, atomar.

- [ ] **Step 1: Failing tests:** `explode_keeps_curve_ids_and_points`, `explode_fixes_every_point` (auch Scheitel und Kontrollpunkte), `explode_removes_frame_and_text`, `explode_keeps_path`, `exploded_curves_are_editable` (Stutzen geht danach), `profiles_same_before_and_after_explode` (gleiche `ProfileRef`s), `explode_two_texts_at_once`.
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Text auflösen`.

### Task 9: Profile schnell genug und Textprofile erkennen

**Files:**
- Modify: `arrangement.rs` und/oder `intersect.rs` (Raster-Vorfilter für Paare nach Hüllquader, für alle Kurven, wenn das allgemein schneller ist, sonst nur für Textkurven), `profile.rs`
- Create: `planora-sketch/tests/text_profile_perf.rs` (Messtest, `#[ignore = "Messtest, nur im Release-Build sinnvoll"]`), `planora-text/tests/real_glyph_profiles.rs` (Dev-Abhängigkeit `planora-text` in `planora-sketch` ist nicht nötig: Der Test liegt in `planora-text` und benutzt `planora-sketch`)

**Interfaces:**
- `Sketch::text_of_profile(&self, p: &ProfileRef) -> Option<TextId>`: Liefert `Some`, wenn alle Kanten des äußeren Rings und der Löcher Kurven desselben Textes sind.

- [ ] **Step 1: Failing tests:**
  - `montserrat_hello_profiles`: „Hallo“ ergibt 5 Profile (H, a, l, l, o), `a` und `o` mit je 1 Loch.
  - `overlapping_contours_give_pieces_without_error`: zwei sich überlappende Glyphen „OO“ mit negativem Abstand, Fit ergibt mehrere Profile ohne Fehler.
  - `text_of_profile_finds_text`, `mixed_profile_is_not_text`
  - Messtests: `forty_chars_profiles_under_150ms`, `two_hundred_chars_under_1s`, `refresh_200_chars_under_20ms` (Median über 11 Läufe). Lauf mit `cargo test --release -p planora-text --test real_glyph_profiles -- --ignored`.
- [ ] **Step 2:** messen. Die Zahlen gehen in die Vault-Notiz.
- [ ] **Step 3:** beschleunigen, bis die Ziele erreicht sind. `profile_perf.rs` (200 Kurven < 50 ms) bleibt grün, die Profile aller vorhandenen Tests bleiben gleich. Ist ein Ziel nicht erreichbar: mit Zahlen melden, nicht still senken.
- [ ] **Step 4:** grün.
- [ ] **Step 5:** Commit `Skizzen 8: Profile aus Text, schnelleres Schneiden`.

### Task 10: Feature-Berechnung und Warnung

**Files:**
- Modify: `planora-feature/src/compute.rs` (`compute_sketch`), `planora-feature/src/model.rs` (falls ein Feld für den Bericht nötig ist, ohne Datei-Auswirkung)
- Test: `planora-feature/tests/sketch_text.rs`

**Interfaces:**
- `compute_sketch` liest aus `Sketch::texts()` den Stand. Hat ein gesetzter Text `substituted`, kommt die Warnung `feature-sketch-font-missing` (mit `font`, `count`). Ein nicht gesetzter Text gibt keine Warnung. Mehrere Warnungen werden mit der Projektionswarnung zusammengeführt, wie `warning` heute gebaut wird.
- `pub fn Document::reshape_texts(&mut self, shaper: &dyn TextShaper) -> bool` (geändert ja/nein; geht über alle Skizzen-Features). Lebt in `planora-feature`, schreibt keinen Verlaufsschritt.

- [ ] **Step 1: Failing tests:** `extrude_of_text_keeps_profile_after_move`, `missing_font_gives_warning_not_error`, `unshaped_text_gives_no_warning`, `reshape_after_load_same_font_is_unchanged`.
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Text in der Feature-Berechnung`.

### Task 11: Schriftliste in der App und Neusetzen nach dem Laden

**Files:**
- Create: `planora-app/src/fonts_library.rs`
- Modify: `state.rs` (Zustand `FontState`), `app.rs` (Start des Hintergrund-Threads, Abfrage je Bild), Dokument-Laden
- Test: `planora-app/tests/sketch_text_load.rs`

**Interfaces:**
- `pub enum FontState { Loading(std::sync::mpsc::Receiver<FontLibrary>), Ready(Arc<FontLibrary>) }`
- `AppState::fonts(&self) -> Option<&Arc<FontLibrary>>`
- Test-Konstruktor: Die App-Tests setzen `FontState::Ready(Arc::new(FontLibrary::bundled_only()))` über einen bestehenden Test-Haken der App. Gibt es keinen, kommt `PlanoraApp::with_fonts(FontLibrary)` dazu, `#[doc(hidden)]`.
- Sobald `Ready`: `reshape_texts` für alle offenen Dokumente. Liefert es `changed == false`, gibt es kein „•“ und keinen Rückgängig-Schritt. Bei `true` wird das Dokument „geändert“, ebenfalls ohne Rückgängig-Schritt (Spec G4). Der Start wird nicht blockiert.

- [ ] **Step 1: Failing tests:** `open_file_with_text_before_fonts_ready_keeps_curves`, `fonts_ready_reshapes_once_without_undo_step_or_dirty`, `missing_font_marks_dirty_and_warns`, `startup_does_not_wait_for_font_scan` (Laden-Zustand mit einem Kanal, der nie liefert, das erste Bild kommt trotzdem).
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Schriftliste im Hintergrund`.

### Task 12: Werkzeug „Text“ und Abschnitt im Skizzenfeld

**Files:**
- Create: `planora-app/src/sketch_text.rs` (Zustand des Werkzeugs, Vorschau, OK/Abbrechen)
- Modify: `sketch_state.rs` (`Tool::Text`, `cancel`, `set_tool`), `sketch_panel.rs` (Abschnitt „Text“ in Fusions Feldreihenfolge, Spec C3), `sketch_view.rs` (zwei Klicks für den Rahmen, Klick auf den Pfad, Vorschau zeichnen), `app.rs` (`SKETCH_CREATE_ACTIONS` um `SketchText` erweitern, Länge 5), `keymap/defaults.rs` (`SketchText, "skizze.text", "action-sketch-text", Sketch, []`), `icons.rs` (`HandIcon::Text`, `HandIcon::ExplodeText` in `HAND_DRAWN`), `de.ftl`/`en.ftl`
- Test: `planora-app/tests/sketch_text_tool.rs`

**Interfaces:**
- `pub struct TextTool { pub kind: TextToolKind, pub corners: Vec<DVec2>, pub path: Option<CurveId>, pub params: TextParams, pub h: HAlign, pub v: VAlign, pub above: bool, pub fit: bool, pub angle: f64, pub editing: Option<TextId>, preview: Option<Arc<TextShape>> }`
- Die zuletzt benutzten Werte (ohne Text) leben in `SketchSession` für den Programmlauf.
- Vorschau setzt höchstens einmal je Bild neu, nur wenn sich Inhalt oder Stil geändert hat.
- OK: `add_text_box`/`add_text_path` mit `edit_label = Some(msg!("undo-sketch-text-create"))`.
- Strg+Enter im Textfeld gilt als OK, Enter macht eine neue Zeile.

- [ ] **Step 1: Failing tests** (1920×1080):
  - `create_menu_has_text_entry`
  - `strip_stays_20_buttons_813px`
  - `two_clicks_then_ok_creates_one_undo_step`
  - `esc_rightclick_toolswitch_cancel_leave_nothing` (je Weg: Skizze `==` vorher, `History` gleich lang)
  - `empty_text_ok_rejected_with_message`
  - `kind_switch_to_path_waits_for_curve_click`
  - `fit_disables_alignment_and_spacing`
  - `defaults_arial_else_montserrat_5mm`
  - `inch_document_defaults_0_2_in`
  - `last_values_remembered_in_session`
  - `ctrl_enter_is_ok_enter_is_newline`
  - `keymap_action_skizze_text_rebindable`
  - `undo_while_text_section_open_cancels_first` (**Review Focus 5**)
  - `save_while_section_open_writes_without_preview`
  - `font_list_shows_missing_font_marked`
  - `i18n_no_german_literals` bleibt grün
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Werkzeug Text im Skizzenfeld`.

### Task 13: Ändern, Auflösen und Hinweise in der App

**Files:**
- Modify: `sketch_view.rs` (Doppelklick auf eine Textkurve öffnet `TextTool` mit `editing`, vor `begin_pattern_edit_item`), `sketch_panel.rs` (Abschnitt „Bearbeiten“: „Text bearbeiten“, „Text auflösen“; Hinweiszeilen nach Spec C10), `app.rs::sketch_modify_menu` („Text auflösen“), `keymap/defaults.rs` (`skizze.text_bearbeiten`, `skizze.text_aufloesen`), Auswahl (`sketch_state.rs`: Klick auf Textkurve wählt alle Kurven des Textes, Rahmenwahl mit dem Rechteck ebenso), Löschen mit der Entf-Taste (ganzer Text bzw. Meldung), Konstruktion umschalten
- Test: `planora-app/tests/sketch_text_edit.rs`

- [ ] **Step 1: Failing tests:** `double_click_glyph_opens_with_values`, `edit_ok_is_one_undo_step_labelled_text_aendern`, `edit_cancel_restores`, `explode_menu_and_button_one_undo_step`, `click_glyph_selects_whole_text`, `delete_key_on_text_deletes_whole`, `delete_key_on_frame_line_shows_message`, `substitute_hint_line_shown`, `synthetic_and_missing_chars_hints_shown`, `history_warning_for_missing_font`, `fonts_loading_shows_reading_hint`.
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Text bearbeiten und auflösen in der App`.

### Task 14: Ganzen Text als Profil wählen

**Files:**
- Modify: `planora-app/src/profile_pick.rs` (`toggle` → Gruppen, Strg+Klick einzeln, Listenzeile je Text mit Zwischenzustand), Aufrufer in den Dialogen Extrudieren, Drehen und Bearbeiten
- Test: `planora-app/tests/profile_pick_text.rs`

**Interfaces:**
- `pub fn toggle_group(sel: &mut Vec<usize>, group: &[usize])`: Sind alle gewählt, werden alle abgewählt, sonst alle gewählt.
- `pub fn groups(extracted: &ExtractedProfiles, sketch: &Sketch) -> Vec<ProfileGroup>`
- `enum ProfileGroup { Single(usize), Text { text: TextId, members: Vec<usize> } }`
- `take_click` bekommt die Modifikatoren.

- [ ] **Step 1: Failing tests:** `click_text_profile_selects_all_of_text`, `click_again_deselects_all`, `second_text_adds`, `ctrl_click_toggles_single`, `ctrl_click_on_plain_profile_same_as_click`, `list_has_one_row_per_text_with_partial_state`, `extrude_text_with_holes_gives_body_with_holes` (Volumen < Volumen ohne Löcher), `revolve_dialog_uses_groups_too`.
- [ ] **Step 2–4:** rot, umsetzen, grün.
- [ ] **Step 5:** Commit `Skizzen 8: Text als Profil wählen`.

### Task 15: Doku und voller Lauf

**Files:**
- Modify: `docs/funktionen.md` (Abschnitt „Text (Skizzen 8)“ unter „## Skizzen“, „Was (noch) nicht geht“ ergänzen), `docs/superpowers/offene-punkte.md` („## Teilprojekt Skizzen 8“ mit „### Stand und Geliefertes“, „### Festlegungen mit Abweichungen“, „### Grenzen“, „### Zahlen“, „### Handliste“), `CHANGELOG.md`

- [ ] **Step 1:** Doku schreiben. Alle **Annahmen** der Spec und aus dem Bauen stehen in `offene-punkte.md`, dazu die Abweichung „planora-text kennt die Datentypen der Skizze“ und die gemessenen Zahlen aus Task 9.
- [ ] **Step 2:** Windows voll: `cargo fmt -p <crate> -- --check` je Crate, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, Messtests im Release (`cargo test --release --workspace -- --ignored`). Erwartet: nur `real_step_text_limit` rot.
- [ ] **Step 3:** WSL voll: Prüfkopie `~/PlanoraCAD` auf `feature/skizzen-text` holen, `~/wsl-s6-full.sh`. Erwartet: nur die zwei bekannten Fehlschläge.
- [ ] **Step 4:** `graphify . --update --code-only`.
- [ ] **Step 5:** Commit `Skizzen 8: Doku und offene Punkte`. Danach Dennis nach dem Go für den Handtest fragen. Handliste: Text im Rahmen mit Arial anlegen, Höhe und Winkel ändern, Text auf Kreis oben, An Pfad anpassen, Extrudieren mit Löchern und Ganzer-Text-Wahl, Auflösen, Speichern und Laden, fehlende Schrift (Testordner mit umgeleitetem `LOCALAPPDATA` und einer nur dort installierten Schrift), Abbruchwege.
