# Graphit – Etappe A (Farben, Schrift, Symbole, Formen) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Das ganze Programm bekommt im alten Aufbau den Graphit-Stil: zentrale Farben (Hell und Dunkel), Montserrat, Linien-Symbole, einheitliche Abstände, Rundungen und Feldrahmen; kein neues Layout.

**Architecture:** `theme.rs` wird die einzige Quelle für Farben, Größen und Rahmen (`Palette` je Thema, `panel_frame`). `fonts.rs` liefert Montserrat in drei Stärken, Roboto als Ersatzschrift und eine Symbolschrift (Phosphor Light) als eigene Schriftfamilie; `icons.rs` bekommt eine Symbol-Tabelle. Danach werden alle verstreuten Farben und festen Größen auf das Theme umgestellt und Menüs, Leisten und Zeilen bekommen Symbole.

**Tech Stack:** Rust, egui/eframe 0.36 (wgpu), vorhandene Tests im eigenen Harness (kein egui_kittest). Neue Dateien: Schriftdateien und Lizenztexte unter `crates/planora-app/assets/fonts/`.

**Spec:** `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-design.md` (insbesondere §2, §3, §4, §5, §9 F1/F5/F6/F7; Vorschauen und Graphit-Dateien unter `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-vorschau/`). Dies ist **Plan 1 von 4** (Etappe A). Etappe B (Modell-Layout), C (Zeichnung/Skizze) und D (Dialoge) bekommen eigene Pläne nach dem Haltepunkt.

## Global Constraints

- Kein neues Layout: egui_tiles, Menüleiste, Tab-Leiste, Statuszeile, zweizeilige Zeichnungsleiste und das Fenster „Skizze“ bleiben (Spec F5). Größen dürfen auf Graphit-Maße wechseln; nichts läuft über, nichts wird abgeschnitten.
- Farben (dunkel, aus `graphit-tokens.json`): `bg-viewport #0e0e0e`, `bg-panel #181818`, `bg-section #1a1a1a`, `bg-field #262626`, `bg-inset #0d0d0d`, `bg-active #333333`, `border-subtle #383838`, `text-primary #ffffff`, `text-secondary #939393`, `accent #006dfd`, `selection #f1f05c`, `axis-x #e0303a`, `axis-y #64be62`, `axis-z #3d8bdb`; Chips Bernstein `#251f0c/#e2ab63`, Blau `#0a1d25/#1f6dcc` (Text darf auf `#4a8fe6` aufgehellt werden), Grün `#0f260c/#6ecd62`.
- Farben (hell, Entwurf aus Spec §4): Viewport `#e8e9ec`, Feld `#ffffff`, Abschnitt `#f4f4f6`, Eingabe `#eeeef1`, aktiv `#dcdce0`, Rand `#d4d4d8`, Text `#151515` / `#5f5f66`, Akzent `#006dfd`, Chips `#fbeed9/#8a5a13`, `#dbe9fb/#1456b0`, `#def3da/#2d7a22`, Auswahlgelb `#d9a400`. Endwerte dürfen leicht abweichen, wenn der Kontrast sonst unter 4,5 : 1 fällt; Abweichungen im Bericht nennen.
- Abstände 4/8/12/16/24; Rundungen 4/6/10/14; Größen Zeile 24, Feld 26, Steuerelement 32, Kopf 40, Leiste 48, Dock 56, Feldbreite 250; Rand 1 px `border-subtle`; **kein Schatten, kein Verlauf**; Übergang 120 ms nur für Füllung und Farbe.
- Textstile: `heading` 13 px semibold, `label` 13 px medium, `body` 12 px regular, `value` 11 px medium, `kbd` 10 px semibold, `axis` 15 px medium (Spec F1).
- Papier bleibt: `theme::PAPER`, `SHEET_INK`, `SHEET_FRAME`, `SHEET_PENDING`, `SHEET_DIM`, `SHEET_LOST`, `SHEET_SELECTED`, `PAPER_SHADOW`, `drawing_surround(dark)`, `viewport_background(dark)`, `accent(dark)` behalten Namen und Signatur (Tests importieren sie); `browser::ERROR_COLOR` und `mesh_body::WARNING_COLOR` bleiben als Namen bestehen.
- Akzent nur für „Modus eingeschaltet“, Tastaturfokus (2 px) und aktive Auswahl; Neutrales Aktiv = `bg-active` (Spec Graphit-README, Abschnitt Colour).
- Texte deutsch in einfacher Laiensprache; bestehende Beschriftungen bleiben (Spec §7). Hell und Dunkel je Dokument-Tab (`state.dark`) bleibt (F7).
- Fehlende Zeichen: Symbol, gezeichnete Form oder deutsches Wort; tippbare/angezeigte Zeichen (ÄÖÜäöüß „“ – − · × ° ± ² ³ … Ø) müssen in Montserrat oder in der Ersatzschrift vorhanden sein (F6).
- Lizenzen: Montserrat (OFL 1.1) und Phosphor (MIT) liegen mit Lizenztext im Repository; `cargo deny check` bleibt grün (Schriftdateien sind keine Crates; kein neues Crate nötig).
- Geschützte Pfade unverändert gegenüber 945fa67: `crates/planora-kernel/**`, `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`.
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests (`cargo ... -j 2`, nie `cargo clean`); am Ende `cargo test --workspace -j 2`.
- Vorarbeit durch den Controller (**vor Aufgabe 1, nur mit Erlaubnis des Nutzers**): Die vier Schriftdateien herunterladen: `Montserrat-Regular.ttf`, `Montserrat-Medium.ttf`, `Montserrat-SemiBold.ttf` (Google Fonts / Montserrat-Projekt, OFL 1.1, mit `OFL.txt`) und `Phosphor-Light.ttf` samt Codepunkt-Zuordnung `style.css` (Phosphor Icons Web, MIT, mit `LICENSE`). Jede Datei in ein eigenes leeres Verzeichnis außerhalb des Repos laden, Größe und Quelle melden, dann erst in `assets/fonts/` legen.

## Review Focus

1. **Lesbarkeit im hellen Thema:** Text-auf-Untergrund unter 4,5 : 1 (die helle Variante ist neu). → Aufgabe 2 (Kontrast-Test beider Themen).
2. **Kästchen statt Zeichen:** ein Zeichen fehlt in Montserrat/Ersatzschrift oder ein Symbol-Codepunkt fehlt in der Symbolschrift. → Aufgabe 1 (Zeichentest), Aufgabe 3 (Symbol-Test).
3. **Theme-Wechsel:** nach Hell/Dunkel-Wechsel (je Tab) stimmen alle Flächen, Rahmen und Textfarben; kein Rest des alten Stils. → Aufgabe 2 (beide Visuals gesetzt) und Aufgabe 4 (Test gegen hartkodierte Farben).
4. **Papier bleibt weiß** mit schwarzen Maßen, auch im dunklen Programm. → Aufgabe 4 (bestehende `drawing_ui`-Farbtests laufen unverändert).
5. **Abschneiden/Überlaufen durch neue Schrift und Größen** (Montserrat ist breiter als Roboto): Menüs, Tab-Leiste, Zeichnungsleiste, Skizzenfenster, Dialoge. → Aufgabe 5 (Test: Textbreite je Leiste passt in die vorhandene Breite bei 1400 px Fensterbreite; Handliste für den Nutzer).

---

### Task 1: Schriften (Montserrat, Ersatzschrift, Symbolschrift)

**Files:**
- Create: `crates/planora-app/assets/fonts/Montserrat-Regular.ttf`, `Montserrat-Medium.ttf`, `Montserrat-SemiBold.ttf`, `OFL-Montserrat.txt`, `Phosphor-Light.ttf`, `LICENSE-Phosphor-MIT.txt` (Dateien vom Controller, siehe Vorarbeit)
- Modify: `crates/planora-app/src/fonts.rs`, `crates/planora-app/assets/fonts/README.md`, `crates/planora-app/tests/fonts.rs`

**Interfaces:**
- Produces in `fonts.rs`:
  - `pub fn definitions() -> egui::FontDefinitions` (gleiche Signatur wie heute; Tests rufen sie weiter selbst auf)
  - `pub fn medium() -> egui::FontFamily`, `pub fn semibold() -> egui::FontFamily`, `pub fn icons() -> egui::FontFamily` (benannte Familien `"medium"`, `"semibold"`, `"icons"`)
  - Familien: `Proportional` = [Montserrat-Regular, Roboto]; `medium` = [Montserrat-Medium, Roboto]; `semibold` = [Montserrat-SemiBold, Roboto]; `icons` = [Phosphor-Light]; `Monospace` = [Roboto] (unverändert).

- [ ] **Step 1: Failing tests in `tests/fonts.rs`:** `all_families_have_fonts` (alle fünf Familien haben mindestens eine nichtleere Schrift); `proportional_starts_with_montserrat_and_falls_back_to_roboto` (Reihenfolge der Namen in `definitions().families`); `every_allowed_character_has_a_glyph_in_proportional` (alle Zeichen der Liste `ÄÖÜäöüß„“–−·×°±²³…Ø` plus `Δ≈` sind in Montserrat **oder** Roboto vorhanden, Prüfung über die egui-Glyphen-Abfrage nach `ctx.set_fonts`; fehlt die Abfrage-API, über die Zeichentabelle der Schriftdatei); der vorhandene Quelltext-Wächter `every_non_ascii_character_in_the_ui_sources…` bleibt, `ALLOWED` wird gegen die neue Schrift aus dem Test oben abgeleitet statt von Hand gepflegt.
- [ ] **Step 2: Run:** `cargo test -p planora-app --test fonts -j 2` → FAIL (Familien fehlen).
- [ ] **Step 3: Implement `fonts.rs`** wie in Interfaces; Schriftdaten per `include_bytes!`; Kommentar mit Lizenzen (Roboto Apache-2.0, Montserrat OFL-1.1, Phosphor MIT). `assets/fonts/README.md` nennt Herkunft, Version und Lizenz jeder Datei.
- [ ] **Step 4: Run:** `cargo test -p planora-app --test fonts -j 2` → PASS; danach `cargo test -p planora-app -j 2` (alle Tests, die `fonts::definitions()` rufen, müssen weiter laufen; Textbreiten ändern sich, Lagetests mit festen Pixeln, die jetzt brechen, werden in Aufgabe 5 behandelt — hier nur melden, welche).
- [ ] **Step 5: Commit** — `feat: Montserrat, Ersatzschrift Roboto und Symbolschrift Phosphor`.

### Task 2: Theme (Palette, Visuals, Textstile, Rahmen)

**Files:**
- Modify: `crates/planora-app/src/theme.rs`
- Test: `crates/planora-app/tests/theme.rs` (neu)

**Interfaces:**
- Consumes: `fonts::medium()`, `fonts::semibold()` (Aufgabe 1).
- Produces in `theme.rs` (bestehende Namen aus Global Constraints bleiben):
```rust
pub struct Palette {
    pub bg_viewport: Color32, pub bg_panel: Color32, pub bg_section: Color32, pub bg_field: Color32,
    pub bg_inset: Color32, pub bg_active: Color32, pub border_subtle: Color32,
    pub viewport_grid: Color32, pub ground_axis: Color32,
    pub text_primary: Color32, pub text_secondary: Color32, pub text_tertiary: Color32, pub text_muted: Color32,
    pub accent: Color32, pub on_accent: Color32, pub selection: Color32,
    pub axis_x: Color32, pub axis_y: Color32, pub axis_z: Color32,
    pub chip_bridge: (Color32, Color32), pub chip_split: (Color32, Color32), pub chip_boolean: (Color32, Color32), // (Fläche, Text)
    pub error: Color32, pub warning: Color32, pub ok: Color32, pub marker: Color32,
}
pub fn palette(dark: bool) -> &'static Palette;
pub const SPACE_1: f32 = 4.0; /* SPACE_2 8, SPACE_3 12, SPACE_4 16, SPACE_6 24 */
pub const RADIUS_XS: u8 = 4; /* SM 6, MD 10, LG 14 */
pub const PANEL_WIDTH: f32 = 250.0; pub const ROW_H: f32 = 24.0; pub const FIELD_H: f32 = 26.0;
pub const CONTROL: f32 = 32.0; pub const HEADER_H: f32 = 40.0; pub const TOOLBAR_H: f32 = 48.0; pub const DOCK_H: f32 = 56.0;
pub fn panel_frame(dark: bool) -> egui::Frame;   // bg_panel, 1 px border_subtle, RADIUS_LG, SPACE_3 innen
pub fn section_frame(dark: bool) -> egui::Frame; // bg_section, RADIUS_MD
pub fn inset_frame(dark: bool) -> egui::Frame;   // bg_inset, RADIUS_SM
pub fn contrast_ratio(a: Color32, b: Color32) -> f32; // WCAG-Kontrast, 1.0..21.0
pub fn apply(ctx: &egui::Context, dark: bool);   // setzt beide Visuals, Textstile, Abstände
```
- Fehler-/Warn-/OK-/Marker-Farben: dunkel `error #e65a5a`, `warning #dc9628`, `ok #6ecd62`, `marker #5a96e6` (bisherige Werte aus `browser.rs`/`mesh_body.rs`/`sketch_panel.rs`); hell dunkler so, dass sie auf `bg_panel` 4,5 : 1 erreichen.

- [ ] **Step 1: Failing tests (`tests/theme.rs`):**
  - `contrast_of_every_text_pair_is_at_least_4_5_in_both_themes`: für beide Themen die Paare `text_primary` und `text_secondary` auf `bg_panel`, `bg_section`, `bg_field`, `bg_active`, `bg_inset`; `on_accent` auf `accent`; Chip-Text auf Chip-Fläche; `error`, `warning`, `ok` auf `bg_panel`. Ausgenommen sind nur `text_tertiary` und `text_muted` (nur für Platzhalter/Leseschwaches, im Test nicht geprüft, aber `text_tertiary` ≥ 4,3 auf `bg_section`).
  - `apply_sets_both_visuals_and_paper_constants_stay`: nach `apply(&ctx, true)` sind `Visuals` des dunklen **und** des hellen Themas gesetzt (`panel_fill` = `bg_viewport` bzw. Hellwert); `theme::PAPER == Color32::WHITE`; `SHEET_INK` unverändert (`#14161a`).
  - `text_styles_use_graphit_sizes`: `heading` 13, `body` 12, `button` 12 (medium-Familie), `small` 10, benannte Stile `label` 13, `value` 11, `axis` 15 existieren in `style.text_styles`.
  - `frames_use_one_pixel_border_and_no_shadow`: `panel_frame(d)` hat Rand 1 px in `border_subtle`, Radius 14, keinen Schatten; `window_shadow` und `popup_shadow` der Visuals sind `Shadow::NONE`.
- [ ] **Step 2: Run:** `cargo test -p planora-app --test theme -j 2` → FAIL.
- [ ] **Step 3: Implement** in `theme.rs`: `Palette`-Konstanten je Thema nach Global Constraints; `apply` setzt `Visuals` nach dem Muster in `graphit-rust-egui.md` (Abschnitt 2), aber für Hell **und** Dunkel über `set_visuals_of`; `override_text_color` nicht setzen; Neutrale Auswahl `selection.bg_fill = bg_active`, Akzent nur in Fokusring/aktivem Schalter; `widgets.*`-Zustände nach der Zustandstabelle der Graphit-README (Ruhe transparent, Hover `bg_field`, Aktiv `bg_active`); Textstile mit den Familien aus Aufgabe 1; Abstände `item_spacing (8,4)`, `button_padding (8,4)`, `interact_size (32, 24)`, `window_margin 12`, `menu_margin 16`. Alte Hilfsfunktionen (`accent`, `drawing_surround`, `viewport_background`, Papierkonstanten) bleiben mit gleichem Verhalten; `accent(true)` liefert jetzt `#006dfd`, `accent(false)` ebenfalls (`SHEET_SELECTED` bleibt `#1f6fe0`).
- [ ] **Step 4: Run:** `cargo test -p planora-app --test theme -j 2` → PASS; `cargo test -p planora-app -j 2` (melden, welche Lagetests nun brechen; Behebung in Aufgabe 5).
- [ ] **Step 5: Commit** — `feat: Graphit-Theme mit Palette für Hell und Dunkel`.

### Task 3: Symbole

**Files:**
- Modify: `crates/planora-app/src/icons.rs`
- Test: `crates/planora-app/tests/icons.rs` (neu)

**Interfaces:**
- Consumes: `fonts::icons()` (Aufgabe 1), `theme::palette` (Aufgabe 2).
- Produces in `icons.rs`:
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon { /* ein Eintrag je Menüpunkt und Leistenschalter, siehe Step 3 */ }
pub const ALL: &[Icon];                              // alle Einträge
pub fn glyph(icon: Icon) -> char;                    // Codepunkt in der Symbolschrift
pub fn text(icon: Icon, size: f32, color: egui::Color32) -> egui::RichText; // Familie fonts::icons()
pub fn label(icon: Icon, label: &str) -> egui::WidgetText; // Symbol (16 px, text_secondary) + Abstand 8 px + Text
```
Die handgezeichneten Funktionen (`paint_body_icon`, `paint_sketch_icon`, `paint_move_icon`, `paint_blend_icon`, `paint_drawing_icon`, `paint_view_icon`, `paint_dimension_icon`) bleiben mit gleicher Signatur, ziehen aber Strichfarbe und -breite (`1.5`, runde Enden) aus dem Theme (`palette(dark).text_secondary` bzw. `text_primary` aktiv).

- [ ] **Step 1: Failing tests (`tests/icons.rs`):** `every_icon_has_a_glyph_in_the_icon_font` (für jedes `Icon` aus `ALL`: `glyph` liegt im privaten Bereich der Schrift und `egui` kennt es in der Familie `icons`); `glyphs_are_unique` (kein Codepunkt doppelt); `icon_label_text_contains_the_label` (`label(Icon::…, "Neu")` enthält „Neu“); `hand_drawn_icons_use_theme_stroke` (ein Aufruf von `paint_body_icon` erzeugt Linien mit Breite 1,5).
- [ ] **Step 2: Run:** `cargo test -p planora-app --test icons -j 2` → FAIL.
- [ ] **Step 3: Implement.** `Icon`-Einträge für alle Stellen, an denen es heute Text ohne Symbol gibt: Datei-Menü (Neu, Öffnen, Speichern, Speichern unter, Schließen, Exportieren, Importieren, Beenden), Erzeugen (Quader, Zylinder, Kugel, Kegel, Extrusion, Drehung), Bearbeiten (Booleans, Verschieben, Rundung/Fase), Ansicht (Hell/Dunkel, Ansichten, Alles einpassen, Ein-/Ausblenden), Zeichnung (Neue Zeichnung, Ansicht hinzufügen, Maße vorschlagen, Maß-Werkzeug, Blatt einpassen, Zoom +/−), Skizzen-Werkzeuge und -Abhängigkeiten (`sketch_panel::TOOLS` und `ACTIONS`), Tab-Leiste (Schließen, Neu). Die Zuordnung Icon → Phosphor-Name steht als Tabelle in `icons.rs`; die Codepunkte kommen aus der mitgelieferten `style.css` der Phosphor-Light-Schrift (Controller-Vorarbeit), nicht aus dem Gedächtnis. Wo Phosphor kein passendes Symbol hat, bleibt ein gezeichnetes.
- [ ] **Step 4: Run:** `cargo test -p planora-app --test icons -j 2` → PASS.
- [ ] **Step 5: Commit** — `feat: Symbol-Tabelle (Phosphor Light) und angeglichene Handsymbole`.

### Task 4: Farben zusammenführen

**Files:**
- Modify: `crates/planora-app/src/{sketch_view.rs, viewport.rs, sketch_panel.rs, browser.rs, mesh_body.rs, boolean_dialog.rs, drawing_view.rs}` (nur Farbstellen)
- Test: `crates/planora-app/tests/no_hard_coded_colors.rs` (neu)

**Interfaces:**
- Consumes: `theme::palette(dark)`.
- Produces: keine neuen öffentlichen Namen; `browser::ERROR_COLOR` und `mesh_body::WARNING_COLOR` bleiben als `pub const`/`pub fn`-kompatible Namen (die Tests importieren sie); gelesen aus der Palette (wenn eine `const` nötig ist, bleibt der Dunkelwert als `const`, und eine neue `pub fn error_color(dark: bool) -> Color32` liefert den themenrichtigen Wert, Aufrufer in `src` nutzen die Funktion).

- [ ] **Step 1: Failing test (`tests/no_hard_coded_colors.rs`):** `no_color32_literals_outside_the_theme`: durchsucht `crates/planora-app/src/*.rs` nach `Color32::from_rgb(`, `from_rgba_premultiplied(`, `from_gray(`, `from_black_alpha`, `from_white_alpha` außerhalb von `theme.rs`; erlaubt (Liste im Test, je mit einem Satz Begründung) sind: Navigationswürfel-Flächen (`viewport.rs`, Gizmo-Farben aus Graphit `gizmo-x/y/z` über Palette-Felder oder Konstanten in `theme.rs`), Skizzen-Geometriefarben, falls sie nicht in die Palette passen (`sketch_view.rs`, dann aber über `theme::`-Funktionen). Test schlägt fehl, solange die Trefferliste nicht der erlaubten Liste entspricht.
- [ ] **Step 2: Run:** `cargo test -p planora-app --test no_hard_coded_colors -j 2` → FAIL mit Trefferliste.
- [ ] **Step 3: Implement.** Alle Treffer auf `theme::palette(dark)`-Felder oder neue `theme::`-Funktionen (`sketch_colors(dark)`, `gizmo_colors()`) umstellen, `dark` aus `ui.visuals().dark_mode` bzw. dem vorhandenen Parameter. Skizzenfarben (Geometrie, Auswahl, Fehler, Warnung) bekommen ihre Werte im Graphit-Look: Geometrie hellblau/akzentnah, Auswahl `selection`, Achsen `axis_x/y/z`, alles in beiden Themen lesbar; die Werte stehen in `theme.rs`. Papierfarben und Blattdarstellung bleiben unangetastet.
- [ ] **Step 4: Run:** `cargo test -p planora-app --test no_hard_coded_colors --test drawing_ui --test sketch_ui --test sketch_dialogs --test tree_ui -j 2` → PASS (die Farbtests in `drawing_ui` für Papier/Maße laufen unverändert).
- [ ] **Step 5: Commit** — `refactor: alle Farben der Oberfläche aus dem Theme`.

### Task 5: Stil anwenden (Menüs, Tabs, Zeilen, Fenster, Größen)

**Files:**
- Modify: `crates/planora-app/src/{app.rs, tabs.rs, browser.rs, properties.rs, projects_panel.rs, versions_panel.rs, drawing_browser.rs, dimension_browser.rs, sketch_panel.rs, drawing_view.rs, cards.rs, export_dialog.rs, prompts.rs, create_dialog.rs, boolean_dialog.rs, blend_dialog.rs, feature_dialog.rs, import_dialog.rs, drawing_dialog.rs, dimension_dialog.rs}`
- Test: `crates/planora-app/tests/graphit_style.rs` (neu); bestehende Lagetests werden angepasst.

**Interfaces:**
- Consumes: `theme::{palette, panel_frame, section_frame, inset_frame, ROW_H, FIELD_H, CONTROL, SPACE_*, RADIUS_*}`, `icons::{label, text, Icon}`, `fonts::{medium, semibold}`.
- Produces: keine neuen Namen außer internen Helfern (zum Beispiel `shortcut_button(ui, Icon, label, shortcut) -> egui::Response` in `app.rs`, das Symbol, Text und Kürzel getrennt setzt statt mit Leerzeichen aufzufüllen).

- [ ] **Step 1: Failing tests (`tests/graphit_style.rs`):**
  - `menu_entries_have_icons_and_separate_shortcuts`: im Datei-Menü steht für „Neu“ ein Symbol-Text (Familie `icons`), die Beschriftung lautet „Neu“ ohne eingebettete Leerzeichen-Kolonne, und das Kürzel „Strg+N“ erscheint als eigener Text rechts.
  - `browser_rows_are_24_high_and_use_theme_colors`: Zeilenhöhe aus `browser::row` ist `theme::ROW_H` (24) bzw. der in Step 3 festgelegte Wert; ausgewählte Zeile hat Fläche `bg_active`, nicht Akzent.
  - `dialog_windows_use_the_panel_frame`: die Fenster `create_dialog`, `export_dialog`, `prompts` zeichnen ihre Fläche in `bg_panel` mit Rand `border_subtle` und Radius 14 (aus `Window::frame`).
  - `no_text_is_clipped_at_1400px`: Menüleiste, Tab-Leiste und die zweizeilige Zeichnungsleiste passen bei 1400 px Fensterbreite ohne Umbruch der Leiste (Summe der Textbreiten kleiner als die Breite).
  - `dark_and_light_both_render_the_panels`: ein Frame mit Browser, Eigenschaften und einem Dialog in beiden Themen erzeugt Text in `text_primary`/`text_secondary` des jeweiligen Themas (kein Rest der alten Farben `#1e2127` und `#252830`).
- [ ] **Step 2: Run:** `cargo test -p planora-app --test graphit_style -j 2` → FAIL.
- [ ] **Step 3: Implement.**
  - **Menüs:** jeder Eintrag mit Symbol (`icons::label`) und rechtsbündigem Kürzel (egui `Button::shortcut_text` oder eigener Helfer); Menüs und Popups mit `RADIUS_LG`, `SPACE_4` Innenabstand.
  - **Tab-Leiste:** aktiver Tab Fläche `bg_active`, Text `text_primary`; inaktiv `text_secondary`; geänderter Tab mit gezeichnetem Punkt (`DIRTY_MARK`) bleibt; Schließen/Neu mit Symbolen.
  - **Zeilen und Panels:** `ROW_HEIGHT` 28 → `theme::ROW_H` (24); Panel-Köpfe (`heading`) als `section_frame` mit `HEADER_H` 40 bzw. 28 für Unterabschnitte; Eingabefelder (`DragValue`, `TextEdit`) in `inset_frame` mit `FIELD_H` 26, Wert zentriert; Schalter und ComboBoxen nach `widgets`-Zuständen des Themes.
  - **Fenster:** alle `egui::Window` mit `.frame(theme::panel_frame(dark))`, Titel `heading`, `Window::open` bleibt; `wide_window` bleibt (volle Breite) bis Etappe D, aber im neuen Rahmen; Skizzenfenster 300 → Graphit-Feldbreite 250 + Innenabstand wenn der Inhalt passt, sonst 300 belassen und im Bericht vermerken.
  - **Zeichnungsleiste:** zwei Zeilen bleiben, Schaltflächen mit Symbolen, Schalter (Checkboxen) im Graphit-Look (Umschalter mit Akzent bei „an“).
  - **Statuszeile/Hinweise:** `text_secondary`, Fehler `error`, Warnung `warning`.
  - Test-Anpassungen: bestehende Lagetests, die durch Schrift/Größen brechen (`tree_ui`, `drawing_ui`, `tabs_ui`, Dialog-Tests), werden auf die neuen Werte umgestellt; geprüft wird weiter dasselbe Verhalten (Spec F4). Keine Test-Hüllen für alte Konstanten.
- [ ] **Step 4: Run:** `cargo test -p planora-app -j 2` (komplett) → PASS; clippy, fmt.
- [ ] **Step 5: Commit** — `feat: Graphit-Stil in Menüs, Tabs, Panels und Fenstern`.

### Task 6: Dokumentation, Gesamtprüfung, Haltepunkt 1

**Files:**
- Modify: `README.md`, `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt 13 – Graphit-Oberfläche, Etappe A“), `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-design.md` (Abschnitt „Festlegungen der Umsetzung Etappe A“ mit allen Abweichungen und Rulings)

- [ ] **Step 1: Doku:** README kurz in Laiensprache (neues Aussehen, Hell/Dunkel, Schrift Montserrat, Symbole), `offene-punkte.md`: was geliefert ist, Grenzen (Lagen noch im alten Aufbau, Dialogbreiten, Zeichen-Ersatz, Kontrastabweichungen, Skizzenfenster-Breite), Zahlen der Gesamtreihe, Folgearbeit Etappe B–D.
- [ ] **Step 2: Gates mit Ausgabe im Bericht:** `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2`; `cargo deny check`; `git diff --stat 945fa67 -- crates/planora-kernel crates/planora-mesh crates/planora-step crates/planora-exchange` leer; `git status` sauber.
- [ ] **Step 3: Commit** — `docs: Graphit Etappe A, Teststand und Grenzen`.
- [ ] **Step 4 (Controller):** Haltepunkt 1: Nutzer startet `cargo run -p planora-app --release` selbst und sieht das ganze Programm in Hell und Dunkel (Modell, Skizze, Zeichnung, Dialoge); Beanstandungen werden gesammelt und als Nachbesserung oder in Etappe B behoben; danach Gesamtreview des Zweigs (opus), eine Korrekturrunde, Merge-Frage an den Nutzer.

---

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung (nur Etappe A):** §2 Entscheidungen Schrift/Symbole/Hell-Dunkel → Aufgaben 1, 2, 3; §3 Ziel 1 (zentrale Quelle) → Aufgabe 2 und 4; Anti-Ziel Kontrast → Aufgabe 2 Test; Anti-Ziel „zwei Stile“ → Aufgabe 4 (Wächter-Test) und 5; §4 Aussehen (Farben, Abstände, Rundungen, Größen) → Aufgabe 2; §5 Punkt 6 (Schrift/Zeichen) und 7 (Lizenzen) → Aufgabe 1, Vorarbeit; Punkt 8 (Farbwelten) → Aufgabe 4; Punkt 10 (Kontrast) → Aufgabe 2; F1 (Quellen/Textstile) → Global Constraints, Aufgabe 2; F5 → Global Constraints, Aufgabe 5; F6 → Aufgabe 1; F7 → Global Constraints. **Nicht in Plan 1** (gehören zu B–D): schwebendes Layout, sichtbare Mitte, Eingabe-Routing, Ersatz von egui_tiles, neue Dialogbreiten, Zeichnungs- und Skizzenlayout (Spec §5 Punkte 1–5, 9; F2, F3, F8).
**Schritt-Scan:** Jeder Schritt nennt Test oder Signatur; Codepunkte der Symbole kommen aus der mitgelieferten `style.css`, nicht aus dem Plan.
**Typkonsistenz:** `fonts::{medium, semibold, icons}` (Aufgabe 1) werden in 2, 3, 5 benutzt; `theme::{palette, panel_frame, section_frame, inset_frame, contrast_ratio}` (Aufgabe 2) in 3, 4, 5; `icons::{Icon, glyph, text, label, ALL}` (Aufgabe 3) in 5.
**Proportion:** Der Plan beschreibt Entscheidungen und Tests, keine Funktionsrümpfe.
**Bekannte Risiken:** Montserrat ist breiter als Roboto (Aufgabe 5, Test auf Abschneiden); die Lagetests der Zeichnung und des Baums (Aufgabe 5) brechen mit; Phosphor-Codepunkte müssen aus der Datei stammen; die helle Palette ist neu und kann beim Kontrast nachgezogen werden müssen. Der Controller setzt für Aufgabe 2 und 5 opus (Designurteil, viele Dateien), für 1, 3, 4, 6 sonnet; Prüfung durch opus bei 2 und 5.
