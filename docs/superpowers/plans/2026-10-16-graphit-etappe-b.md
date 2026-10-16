# Graphit – Etappe B (schwebendes Layout) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Das Hauptfenster bekommt das schwebende Graphit-Layout: Der 3D-Bereich füllt das ganze Fenster, Menü mit Tabs, Baumfeld, Eigenschaften/Versionen, Befehlsfeld und Meldungsfeld schweben als Felder darüber; das Modell erscheint in der sichtbaren Mitte.

**Architecture:** Neue Datei `shell.rs` (Zustand der Felder, reine Funktion für die freie Fläche) und `shell_panels.rs` (Zeichnen der Felder als `egui::Area`) ersetzen `dock.rs` und egui_tiles. Die Kamera bekommt einen Bildmitte-Versatz, damit „Alles einpassen“ und neue Skizzen die freie Fläche treffen, während der Renderer das ganze Fenster füllt. Zeichnung und Skizzen-Overlay behalten ihr Innenlayout und arbeiten im freien Rechteck bzw. mit der Kamera mit Versatz.

**Tech Stack:** Rust, egui/eframe 0.36 (Area, Order, ScrollArea), wgpu-Paint-Callback; Tests im eigenen Harness (`egui::Context::run_ui`, kein egui_kittest).

**Spec:** `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-design.md` (§2–§6 Etappe B, §9 F2/F3/F4/F7/F8, §10); Vorschauen `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-vorschau/` (`modell-graphit.html`, `modell-graphit-hell.html`). Plan 2 von 4; Etappe A ist gemergt (`b155f62`).

## Global Constraints

- Felder sind `egui::Area` (nicht `Panel`/`SidePanel`), Rahmen `theme::panel_frame(dark)` (Fläche `bg_panel`, 1 px `border_subtle`, Radius 14, Innenabstand 12), **kein Schatten, kein Verlauf**; Seitenfelder `theme::PANEL_WIDTH` = 250 (Außenbreite).
- Abstände: Außenrand der Felder `SHELL_INSET` = 16 (`theme::SPACE_4`), Abstand zwischen Feldern `theme::SPACE_3` = 12 (Entscheidung B1).
- Hell und Dunkel je Dokument-Tab (`state.dark`), Rahmen pro Bild aus `dark` ableiten (F7).
- Alle bisherigen Funktionen, Menüeinträge und Tastenkürzel bleiben erreichbar und wirken gleich; keine neue Funktion (Spec §3 Nicht-Ziele). Kein verschiebbares oder andockbares Feldsystem; Ein-/Ausblenden von Projekte und Versionen bleibt im Menü Ansicht.
- Das Zeichenblatt bleibt weißes Papier (`theme::PAPER`, `SHEET_*` unverändert); Zeichnung und Skizzen-Eingabe liegen nicht unter Feldern (Spec F2).
- Kamera-Mathematik wird nicht umgebaut; ein Bildmitte-Versatz (NDC-Verschiebung) ist erlaubt (F3). Standardwert **0**: Tests und Code ohne Versatz verhalten sich unverändert.
- Dialoge bleiben `egui::Window` und liegen **über** den Feldern (Entscheidung B4: Felder `Order::Middle` und nicht verschiebbar, Dialoge `Order::Foreground`); ihre Breitenregel (`wide_window`) bleibt bis Etappe D.
- Tests dürfen mit dem Layout umgeschrieben werden, geprüft wird weiter dasselbe Verhalten (F4); `cargo test --workspace -j 2` ist nach jeder Aufgabe nicht nötig, am Ende Pflicht. Wächter-Tests (`no_hard_coded_colors`, `no_raw_text_edits`, `fonts`) bleiben grün: Farben nur aus `theme::palette`, Textfelder nur über `theme::text_field`.
- Geschützte Pfade unverändert gegenüber 945fa67: `crates/planora-kernel/**`, `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`. `planora-render` darf angepasst werden (Kamera), seine bisherigen Tests bleiben grün.
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests (`cargo ... -j 2`, nie `cargo clean`).

**Festlegungen (Entscheidung Entwurfsverfasser, der Nutzer kann sie kippen):**
- **B1** Rand 16 px, Abstand 12 px (Graphit nennt 24 px Rand; 16 spart Platz, Vorschauen nutzen 16).
- **B2** Freie Fläche wird **deterministisch aus Konstanten** berechnet (kein Messen der Feldgrößen, keine Rückkopplung): links/rechts je `SHELL_INSET + PANEL_WIDTH + SPACE_3` wenn ein Feld dort sichtbar ist, oben `SHELL_INSET + TOP_H + SPACE_3`, unten `SHELL_INSET + DOCK_H + SPACE_3`.
- **B3** Positionen der Felder werden aus `ctx.content_rect()` und diesen Konstanten gesetzt (`fixed_pos`), Höhen der Seitenfelder aus der freien Fläche; Felder mit unbekannter Breite (oben, unten) laufen mit `fade_in(false)` und verwerfen den ersten Größen-Durchgang (`ui.is_sizing_pass()` → `ctx.request_discard`), wie `prompts::window`.
- **B4** Reihenfolge: Felder `Order::Middle`, `movable(false)`; `theme::window` (alle Dialoge, Skizzenfenster) bekommt `.order(egui::Order::Foreground)`.
- **B5** Zeiger-Routing über `response.hovered()`/`hover_pos()` und `ui.rect_contains_pointer`; **nicht** `ctx.is_pointer_over_egui()` verwenden (taugt mit Area-Feldern nicht).
- **B6** Der Modusschalter oben rechts der Vorschau entfällt in Etappe B (es gibt keine Modi dafür, Spec F8: „nichts erfinden“); rechts oben steht direkt das Feld Eigenschaften | Versionen. Das untere Befehlsfeld ist nur eine Abkürzung zu vorhandenen Menüaktionen.
- **B7** Der Hinweis „Alte Version vN – schreibgeschützt …“ steht als eigenes schmales Feld unter dem oberen Feld (Farbe `warning`).
- **B8** Zeichnungsansicht bleibt in Etappe B im freien Rechteck mit ihrer alten Werkzeugleiste (Umbau in Etappe C); sie bekommt einen Hintergrund `theme::viewport_background`. Das Skizzenfenster bleibt ein verschiebbares Fenster (300 px), Startposition rechts neben dem linken Feld.

## Review Focus

1. **Position der Felder im ersten Bild und nach Fenstergrößenänderung:** kein Flackern/Springen, keine Felder außerhalb des Fensters (Fenster 800 × 600 bis 2560 × 1440). → Aufgabe 2 (reine Geometrie-Tests) und Aufgabe 4.
2. **Felder überdecken einander oder das Modell/Blatt/Skizze:** bei kleinen Fenstern (1100 × 700, 800 × 600) dürfen Felder sich nicht überlappen; Seitenfelder schrumpfen und rollen. → Aufgabe 2, 4.
3. **Eingabe:** Klick/Ziehen/Mausrad auf einem Feld darf den 3D-Bereich nicht drehen, wählen oder zoomen; ein im 3D-Bereich begonnenes Ziehen läuft über Felder weiter; Tastenkürzel nicht bei Texteingabe in Feldern. → Aufgabe 3 und 5.
4. **Dialoge verschwinden hinter Feldern** (Klick auf ein Feld hebt es über den Dialog). → Aufgabe 3 (Reihenfolgetest) und 5.
5. **Modell hinter Feldern verloren:** „Alles einpassen“, neuer Tab, Dokument öffnen, Skizze öffnen, Ansichtswechsel zentrieren in der **freien** Mitte; Wiederherstellen der Kamera nach der Skizze behält den Versatz. → Aufgabe 1 und 3.

---

### Task 1: Kamera mit sichtbarem Bereich (`planora-render`, `planora-app/state.rs`)

**Files:**
- Modify: `crates/planora-render/src/camera.rs`, `crates/planora-app/src/state.rs` (`restore_camera`, `frame_all`, `open_sketch`)
- Test: `crates/planora-render/tests/camera_visible.rs` (neu), `crates/planora-app/tests/state.rs` (ergänzen)

**Interfaces:**
- Consumes: bestehende `OrbitCamera` (`target`, `distance`, `yaw`, `pitch`, `fov_y`, `aspect`; `view_proj`, `ray`, `project`, `frame_bounds`, `set_viewport`).
- Produces in `camera.rs`:
```rust
pub struct OrbitCamera { /* bisherige Felder + */ pub ndc_offset: glam::Vec2, pub visible_aspect: f32, pub visible_height_fraction: f32 }
impl OrbitCamera {
    /// full: Größe des ganzen Fensters in Pixeln; visible: freies Rechteck (Pixel, Ursprung oben links des Fensters).
    /// Setzt aspect = full.x/full.y, ndc_offset (x nach rechts, y nach oben, in NDC), visible_aspect = visible.w/visible.h, visible_height_fraction = visible.h/full.y.
    pub fn set_visible(&mut self, full: glam::Vec2, visible: (glam::Vec2 /*min*/, glam::Vec2 /*size*/));
    pub fn clear_visible(&mut self);   // Versatz 0, visible_aspect = aspect, Anteil 1
}
```
`Default` setzt `ndc_offset = ZERO`, `visible_aspect = aspect`, `visible_height_fraction = 1.0`. `set_viewport(w,h)` bleibt unverändert und setzt zusätzlich `visible_aspect = aspect` **nur wenn** der Versatz 0 ist.

- [ ] **Step 1: Failing tests (`camera_visible.rs`):** (a) `target_projects_to_the_centre_of_the_visible_rect`: Fenster 1000 × 600, freies Rechteck min (250, 80) Größe (500, 400): `project(target, full)` liefert genau (500, 280) (Mitte des freien Rechtecks), Abweichung < 1e-3; (b) `ray_and_project_agree_with_offset` (für 20 zufällige Punkte: `project(p)` → `ray` trifft p, Fehler < 1e-4 mm); (c) `zero_offset_changes_nothing` (Default-Kamera: `view_proj` bitgleich zu einer Kopie vor der Änderung, Hash des alten Tests `target_projects_to_screen_center` bleibt grün); (d) `frame_bounds_fits_the_box_inside_the_visible_rect`: Würfel 100 mm, danach liegen alle 8 projizierten Ecken innerhalb des freien Rechtecks (Rand ≥ 0 px) und mindestens eine berührt es bis 15 % Rand; (e) `pan_scale_is_unchanged_by_the_offset` (`pan` bewegt `target` wie ohne Versatz). In `planora-app/tests/state.rs`: (f) `restore_camera_keeps_the_visible_rect` (Skizze öffnen und schließen: `ndc_offset`, `visible_aspect`, `visible_height_fraction` danach gleich), (g) `frame_all_uses_the_visible_rect` (nach `camera.set_visible` liegt die Bounding-Box des Körpers im freien Rechteck).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-render --test camera_visible -j 2` und `cargo test -p planora-app --test state -j 2`.
- [ ] **Step 3: Implement** `projection()` als `Mat4::from_translation(vec3(ndc_offset.x, ndc_offset.y, 0.0)) * perspective(...)` (so ändern sich `view_proj`, `ray`, `project`, GPU-Matrix, `pick_edge` und die Skizzen-Abbildungen automatisch); `frame_bounds` rechnet die Entfernung mit `fov_vis = 2·atan(tan(fov_y/2)·visible_height_fraction)` und `visible_aspect` statt `aspect`; `restore_camera` (state.rs ~1408) übernimmt zusätzlich die drei neuen Felder; `open_sketch` und `frame_all` benutzen weiter `frame_bounds`.
- [ ] **Step 4: Run → PASS**, dazu alle bestehenden `planora-render`-Tests und `cargo test -p planora-app --test sketch_ui --test sketch_dialogs --test state -j 2` (müssen unverändert grün bleiben), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Versatz in `projection()` weglassen → (a) rot; `visible_height_fraction` in `frame_bounds` ignorieren → (d) rot; `restore_camera` ohne die neuen Felder → (f) rot. **Step 6: Commit** — `feat: Kamera mit sichtbarem Bereich (Bildmitte-Versatz)`.

### Task 2: Zustand und Geometrie der Felder (`shell.rs`)

**Files:**
- Create: `crates/planora-app/src/shell.rs`, `crates/planora-app/tests/shell_layout.rs`
- Modify: `crates/planora-app/src/lib.rs` (`pub mod shell;`)

**Interfaces:**
- Consumes: `theme::{PANEL_WIDTH, SPACE_3, SPACE_4, TOOLBAR_H, DOCK_H}`.
- Produces:
```rust
pub const SHELL_INSET: f32 = 16.0;      // theme::SPACE_4
pub const TOP_H: f32 = 48.0;            // theme::TOOLBAR_H
pub const BOTTOM_H: f32 = 56.0;         // theme::DOCK_H
pub const GAP: f32 = 12.0;              // theme::SPACE_3
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum RightTab { Properties, Versions }
#[derive(Clone, Debug)] pub struct ShellState { pub projects_visible: bool, pub versions_visible: bool, pub right_tab: RightTab }
impl Default for ShellState { /* projects_visible=true, versions_visible=true, right_tab=Properties */ }
impl ShellState { pub fn show_projects(&mut self); pub fn set_versions_visible(&mut self, v: bool); pub fn effective_right_tab(&self) -> RightTab; }
/// Rechtecke aller Felder für ein Fenster (content): top (Mitte oben, Breite = `top_width`), left, right, bottom (Mitte unten, Breite = `bottom_width`), status (links unten), banner (unter top, nur wenn read_only), free (freie Fläche).
pub struct ShellRects { pub top: Rect, pub left: Rect, pub right: Rect, pub bottom: Rect, pub status: Rect, pub banner: Option<Rect>, pub free: Rect }
pub fn layout(content: Rect, state: &ShellState, top_width: f32, bottom_width: f32, status_width: f32, read_only: bool) -> ShellRects;
```
Regeln für `layout`: Außenrand `SHELL_INSET`; `left` = Spalte links (Breite 250, Oberkante `SHELL_INSET`, Unterkante = `free.bottom`), nur belegt wenn `projects_visible` **oder** der Baum (der Baum ist immer sichtbar, Spalte also immer vorhanden); `right` = Spalte rechts (250) immer vorhanden (Eigenschaften immer sichtbar, Versionen per Umschalter); `top` mittig, Breite `min(top_width, content.width − 2·(250 + SHELL_INSET + GAP))` (mindestens 240); `bottom` mittig unten, Breite `min(bottom_width, …gleich…)`; `status` links unten unter dem linken Feld: Breite `min(status_width, 250)` und Höhe 32 (das linke Feld endet darüber um `GAP`); `free` = `content` abzüglich links `SHELL_INSET + 250 + GAP`, rechts dasselbe, oben `SHELL_INSET + TOP_H + GAP`, unten `SHELL_INSET + BOTTOM_H + GAP`; hat `free` weniger als 200 × 200, wird es auf Mindestgröße 200 × 200 in der Mitte von `content` gesetzt und die Seitenfelder werden schmaler (Breite = `max(160, (content.width − 200 − 4·GAP) / 2)`, Spalten dürfen mit dem Rest rollen).

- [ ] **Step 1: Failing tests (`shell_layout.rs`):** (a) `no_fields_overlap_at_common_sizes` für 800×600, 1100×700, 1400×900, 1920×1080, 2560×1440: je zwei Rechtecke aus `{top, left, right, bottom, status, banner}` schneiden sich nicht; alle liegen in `content`; (b) `free_rect_matches_the_documented_margins_at_1400x900`: exakte Werte (links `16+250+12 = 278`, oben `16+48+12 = 76`, unten `16+56+12 = 84`, rechts 278); (c) `free_rect_never_smaller_than_200_square` (Fenster 640×480, 400×400); (d) `read_only_banner_sits_below_the_top_field`; (e) `hidden_versions_keep_the_right_column`; (f) `show_projects_makes_projects_visible`; (g) `right_tab_falls_back_when_versions_hidden`: `set_versions_visible(false)` mit `right_tab = Versions` → `effective_right_tab() == Properties`.
- [ ] **Step 2: Run → FAIL.** **Step 3: Implement** nach den Regeln oben (reine Funktionen, keine egui-Zustände). **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Seitenfeld-Breite auf 300 setzen → (b) rot; Überlappungs-Prüfung der Banner-Zeile entfernen → (a)/(d) rot. **Step 6: Commit** — `feat: Zustand und Geometrie der schwebenden Felder`.

### Task 3: 3D-Bereich im Vollbild und Eingabe (`viewport.rs`, `theme.rs`)

**Files:**
- Modify: `crates/planora-app/src/viewport.rs`, `crates/planora-app/src/theme.rs` (`window`: `Order::Foreground`), `crates/planora-app/src/sketch_view.rs` (nur Aufrufstellen, falls nötig)
- Test: `crates/planora-app/tests/viewport_shell.rs` (neu); `tests/sketch_ui.rs`, `tests/sketch_dialogs.rs` bleiben unverändert grün.

**Interfaces:**
- Consumes: `OrbitCamera::set_visible/clear_visible` (Aufgabe 1), `shell::ShellRects` (Aufgabe 2).
- Produces in `viewport.rs`:
```rust
pub fn show(ui: &mut egui::Ui, state: &mut AppState);                       // wie bisher (free = ganzer Bereich)
pub fn show_in(ui: &mut egui::Ui, state: &mut AppState, free: Option<egui::Rect>);   // free = freies Rechteck in Bildschirmkoordinaten
pub fn cube_rect(free: egui::Rect) -> egui::Rect;                           // rechts oben im freien Rechteck, Rand 12 px
```
`show` ruft `show_in(ui, state, None)`. Bei `Some(free)` setzt `show_in` pro Bild `state.camera.set_visible(rect.size, (free.min − rect.min, free.size))` (sonst `clear_visible`), der Würfel liegt in `cube_rect(free)`, Auswahl/Hover/Orbit/Zoom/Pan wie bisher auf dem **ganzen** `rect`. `theme::window` setzt `.order(egui::Order::Foreground)` (Entscheidung B4).

- [ ] **Step 1: Failing tests (`viewport_shell.rs`):** (a) `cube_sits_in_the_free_rect_corner` (`cube_rect(free)` = (free.right − 96 − 12, free.top + 12), Größe 96); (b) `camera_gets_the_visible_rect_each_frame` (nach `show_in` mit `Some(free)` projiziert `camera.project(target, size)` in die Mitte von `free`); (c) `pointer_over_a_floating_area_does_not_orbit_or_select`: ein `egui::Area` mit einem Button über dem Viewport, Ziehen und Klicken **auf dem Area** ändert `yaw/pitch`/Auswahl nicht, das gleiche Ziehen daneben ändert sie; (d) `drag_started_in_the_viewport_continues_over_a_field` (Ziehen beginnt im Viewport, endet über dem Area: `yaw` ändert sich bis zum Ende); (e) `mouse_wheel_over_a_field_does_not_zoom`; (f) `dialogs_stay_above_fields`: ein `Order::Middle`-Area und ein `theme::window` an derselben Stelle: `ctx.layer_id_at(pos)` ist das Fenster, auch nach einem Klick auf das Area; (g) `escape_keeps_working_and_typing_in_a_field_blocks_it` (`typing`).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implement** `show_in`/`cube_rect` (Pointer-Routing über `response.hovered()` wie heute, **kein** `is_pointer_over_egui`); `theme::window` mit Order. **Step 4: Run → PASS**, dazu `cargo test -p planora-app -j 2` (alle bestehenden Viewport- und Skizzen-Tests grün), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Würfel an `rect` statt `free` → (a) rot; `Order::Foreground` entfernen → (f) rot; `set_visible` weglassen → (b) rot. **Step 6: Commit** — `feat: 3D-Bereich im Vollbild mit freier Fläche und Eingaberouting`.

### Task 4: Felder zeichnen (`shell_panels.rs`)

**Files:**
- Create: `crates/planora-app/src/shell_panels.rs`, `crates/planora-app/tests/shell_panels.rs`
- Modify: `crates/planora-app/src/lib.rs`, `crates/planora-app/src/theme.rs` (kleiner Helfer `theme::field_area`), bei Bedarf `projects_panel.rs`/`versions_panel.rs` (nur Überschrift `theme::section_header`)

**Interfaces:**
- Consumes: `shell::{ShellState, ShellRects, layout}`, bestehende `app::menu_bar(ui, ws, ???)`, `tabs::show`, `browser::show_with`, `properties::show`, `versions_panel::show`, `projects_panel::show`, `browser::status_text`, `theme::{panel_frame, section_header, palette}`.
- Produces (Namen sind verbindlich):
```rust
/// Zeichnet alle Felder (Area-basiert) und liefert die freie Fläche; Reihenfolge: Felder vor dem Viewport-Aufruf des Aufrufers.
pub fn show_fields(ctx: &egui::Context, app: &mut PlanoraApp) -> egui::Rect;
fn top_field(ctx, rects: &ShellRects, app: &mut PlanoraApp);     // Menü + Trennstrich + Tabs (Tabs in horizontaler ScrollArea)
fn left_field(ctx, rects: &ShellRects, app: &mut PlanoraApp);    // Projekte (wenn sichtbar) + Browser, eine vertikale ScrollArea, max_height = rects.left.height
fn right_field(ctx, rects: &ShellRects, app: &mut PlanoraApp);   // Umschalter Eigenschaften | Versionen + Inhalt, Rollbereich
fn status_field(ctx, rects: &ShellRects, app: &mut PlanoraApp);  // Meldungslogik wie bisher (Vorrang: message, Skizzenhinweis, Baumfehler, mesh_note/„N Körper“)
fn banner_field(ctx, rects: &ShellRects, app: &mut PlanoraApp);  // nur bei read_only, Farbe warning, Text unverändert
```
Die Bereichsinhalte (`browser::show_with` usw.) bleiben unverändert aufrufbar; `scrolled()` wird durch `ScrollArea::vertical().max_height(h).auto_shrink([false, true])` ersetzt, Breite wird mit `ui.set_width(PANEL_WIDTH − 2·SPACE_3)` fest (Rückkopplung vermeiden). Die Menüsignatur `menu_bar` bekommt in Aufgabe 5 `&mut ShellState` statt `&mut Dock`; bis dahin ruft `top_field` die vorhandene Funktion mit einem Platzhalter-`Dock`-freien Adapter (die Aufgabe 5 räumt auf). Felder nutzen `fixed_pos` aus `ShellRects`, `movable(false)`, `Order::Middle`, `fade_in(false)`; bei unbekannter Breite (top, status) `ui.is_sizing_pass()` → `ctx.request_discard`.

- [ ] **Step 1: Failing tests (`shell_panels.rs`):** (a) `fields_draw_inside_the_window_at_1400x900_and_1100x700` (alle gezeichneten Feld-Rechtecke liegen im `content_rect`, überlappen einander nicht); (b) `top_field_contains_menu_and_tabs` (Texte „Datei“, „Erzeugen“, „Zeichnung“, „Bearbeiten“, „Ansicht“ und Tab-Namen sichtbar); (c) `left_field_shows_projects_and_tree_and_hides_projects_on_request`; (d) `right_field_switches_between_properties_and_versions` (Klick auf „Versionen“ zeigt die Versionsliste, „Eigenschaften“ zurück); (e) `read_only_banner_text_and_colour` (Text unverändert, Farbe `warning`); (f) `status_field_shows_message_with_error_colour_and_counts_bodies` (je ein Fall: Meldung, Skizzenhinweis, Baumfehler, „1 Körper“); (g) `first_frame_has_no_jump` (Rechtecke im ersten und zweiten Bild gleich bzw. erstes Bild wird verworfen); (h) `side_columns_scroll_when_content_is_taller_than_the_window` (viele Features, Fenster 1100×500: Inhalt rollt, Feldhöhe ≤ `rects.left.height`); (i) `dark_and_light_frames` (Rahmenfarbe `border_subtle` je Thema, Radius 14, keine Schatten).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implement** wie in Interfaces. **Step 4: Run → PASS**, clippy, fmt, Wächter-Tests.
- [ ] **Step 5: Mutationsbeweise:** `fixed_pos` des rechten Felds um 10 px nach links schieben → (a) rot; Sizing-Pass-Discard entfernen → (g) rot; `set_width` entfernen → (h)/(a) rot. **Step 6: Commit** — `feat: schwebende Felder für Menü, Baum, Eigenschaften, Versionen und Meldungen`.

### Task 5: Befehlsfeld unten (Abkürzungen)

**Files:**
- Create: `crates/planora-app/tests/shell_commands.rs`
- Modify: `crates/planora-app/src/shell_panels.rs` (`bottom_field`), `crates/planora-app/src/app.rs` (Aktionen als freie Funktionen herausziehen, falls nötig)

**Interfaces:**
- Consumes: die Aktionen aus `create_menu`/`edit_menu` (Erzeugen: Quader, Zylinder, Kugel, Kegel, Extrusion, Drehung; Bearbeiten: Boolesche Operation, Verrunden/Fase, Verschieben, falls im Menü), `icons::{source_button, Icon, sketch_*}`, `theme::{palette, panel_frame}`.
- Produces: `fn bottom_field(ctx, rects: &ShellRects, app: &mut PlanoraApp)` (Fläche `bg_panel`, Höhe `BOTTOM_H`): links drei farbige Chips „Vereinigen“, „Abziehen“, „Schneiden“ (`palette.chip_bridge`/`chip_split`/`chip_boolean`, Text in `.1`, Fläche `.0`; Klick öffnet den Booleschen Dialog mit der passenden Operation vorgewählt, falls der Dialog das hergibt, sonst ohne Vorwahl), danach Trennstrich, danach Symbolknöpfe für die vorhandenen Erzeugen-Aktionen (jeder mit Hinweistext der Menübeschriftung). Dieselben Sperren wie im Menü (schreibgeschützt, Skizzenmodus, offene Rückfrage); jede Aktion bleibt zusätzlich im Menü.

- [ ] **Step 1: Failing tests (`shell_commands.rs`):** (a) `every_bottom_action_matches_a_menu_action` (jede Schaltfläche löst dieselbe Wirkung aus wie der Menüeintrag: z. B. „Quader“ öffnet `tab.dialog` mit demselben Zustand); (b) `chips_open_the_boolean_dialog_with_the_operation` (oder ohne Vorwahl, wenn der Dialog keine hat – Test passt zur tatsächlichen Umsetzung); (c) `bottom_field_is_disabled_in_read_only_tabs_and_while_sketching`; (d) `chip_colours_come_from_the_palette_in_both_themes`; (e) `tooltips_name_the_action_in_german`.
- [ ] **Step 2–4:** FAIL → Implementieren → PASS, clippy, fmt. **Step 5: Mutationsbeweise:** Sperre bei schreibgeschützt weglassen → (c) rot; Chip-Farbe fest verdrahten → Wächter `no_hard_coded_colors` rot. **Step 6: Commit** — `feat: Befehlsfeld mit Abkürzungen`.

### Task 6: Shell verdrahten, egui_tiles entfernen

**Files:**
- Modify: `crates/planora-app/src/app.rs`, `crates/planora-app/src/lib.rs`, `crates/planora-app/Cargo.toml`, `Cargo.toml` (Workspace), `crates/planora-app/src/drawing_view.rs` (nur Hintergrund/Aufruf im freien Rechteck), `crates/planora-app/src/sketch_panel.rs` (Startposition)
- Delete: `crates/planora-app/src/dock.rs`, `crates/planora-app/tests/dock.rs`
- Test: `crates/planora-app/tests/shell_app.rs` (neu); `tests/graphit_style.rs` anpassen

**Interfaces:**
- Consumes: alles aus den Aufgaben 1–5.
- Produces: `PlanoraApp` trägt `shell: ShellState` statt `dock: Dock`; `app::menu_bar(ui, ws, shell: &mut ShellState) -> bool` (Signatur geändert, Test-Aufrufe angepasst); `view_menu` schaltet `shell.projects_visible`/`shell.versions_visible` (Einträge „Projekte“ und „Versionen“ bleiben, `Strg+O` ruft `shell.show_projects()`); `ui()` zeichnet: `theme::apply`-Prüfung, Skizzen-Zwangsbereich, `shell_panels::show_fields(ctx, self)` → freie Fläche `free`, dann `egui::CentralPanel::no_frame()` mit `viewport::show_in(ui, state, Some(free))` (Modell/Skizze) bzw. `drawing_view::show_with` in einem Kind-`Ui` mit `max_rect = free` und Hintergrund `theme::viewport_background(dark)` (Zeichnung), danach die Dialoge wie heute. `sketch_panel::show`: `default_pos` = (`SHELL_INSET + PANEL_WIDTH + SPACE_6`, 90). `Cargo.toml`: `egui_tiles` aus Workspace und `planora-app` entfernen, Lizenz-Kommentar bereinigen; `cargo deny check` bleibt grün.

- [ ] **Step 1: Failing tests (`shell_app.rs`):** soweit `PlanoraApp` ohne `CreationContext` nicht baubar ist, über kleine Hilfsfunktion `PlanoraApp::for_tests(ctx)` (`#[doc(hidden)]`) prüfen: (a) `free_rect_is_what_the_viewport_gets` (nach einem Bild projiziert die Kamera des aktiven Tabs den Zielpunkt in die Mitte der freien Fläche); (b) `view_menu_toggles_projects_and_versions_fields`; (c) `ctrl_o_shows_the_projects_field`; (d) `drawing_area_stays_inside_the_free_rect` (`drawing_view::last_layout(ctx).area` ⊂ freie Fläche, ohne Überlappung mit Feldern); (e) `dialogs_open_above_fields` (Export-Dialog öffnen, Klick auf das linke Feld: `layer_id_at` über dem Dialog bleibt der Dialog); (f) `all_menu_entries_still_present_and_shortcuts_work` (Strg+N, Strg+O, Strg+S, Strg+W, Strg+Umschalt+E wirken); (g) `sketch_window_starts_right_of_the_left_field`; (h) `new_tab_open_document_frame_all_centres_in_the_free_rect`.
- [ ] **Step 2: Run → FAIL.** **Step 3: Implement.** Test-Anpassungen: `tests/graphit_style.rs` (Zeilen mit `dock::Dock::new()` und `menu_bar(ui, &mut ws, &mut dock)` → `ShellState`), `tests/dock.rs` entfällt (ersetzt durch Aufgabe 2 (f)(g)).
- [ ] **Step 4: Run → PASS** (`cargo test -p planora-app -j 2` komplett), `cargo deny check`, clippy, fmt. **Step 5: Mutationsbeweise:** `viewport::show_in` mit `None` aufrufen → (a) rot; Order der Dialoge entfernen → (e) rot; Zeichnung ohne `max_rect` → (d) rot. **Step 6: Commit** — `feat: schwebende Felder im Hauptfenster, egui_tiles entfernt`.

### Task 7: Dokumentation, Gesamtprüfung, Haltepunkt 2

**Files:**
- Modify: `README.md`, `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt 13 – Etappe B“), `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-design.md` (Abschnitt „Festlegungen der Umsetzung Etappe B“ mit B1–B8 und allen Rulings), diesen Plan (falls Abweichungen)

- [ ] **Step 1: Doku** (Laiensprache, README kurz: neues Layout, Felder, was sich nicht verschieben lässt, Ein-/Ausblenden im Menü Ansicht); `offene-punkte.md`: Geliefertes, Entscheidungen B1–B8, Grenzen (Zeichnungsleiste weiter im alten Aufbau bis Etappe C, Skizzenfenster, Dialogbreiten bis Etappe D, kleine Fenster), Handliste (Fenstergrößen 800×600 bis groß, Hell/Dunkel, alle Menüs, Tabs, Felder ein-/ausblenden, Modell dreht/wählt nicht unter Feldern, Dialoge über Feldern, „Alles einpassen“ zentriert, Skizze öffnen und schließen, Zeichnung), Zahlen.
- [ ] **Step 2: Gates** mit Ausgabe im Bericht: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2` (~16 min); `cargo deny check`; `git diff --stat 945fa67 -- crates/planora-kernel crates/planora-mesh/src crates/planora-step crates/planora-exchange` leer; `git status` sauber.
- [ ] **Step 3: Commit** — `docs: Graphit Etappe B, Teststand und Grenzen`.
- [ ] **Step 4 (Controller):** Haltepunkt 2: Nutzer startet `cargo run -p planora-app --release` selbst und arbeitet im Modell-Tab (Fenstergrößen, Felder, Menüs, Tabs, Eingabe über Feldern, Dialoge, Skizze, Zeichnung); Beanstandungen werden gesammelt; danach Gesamtreview des Zweigs (opus), eine Korrekturrunde, Merge-Frage.

---

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung (Etappe B):** §4 Modell-Ansicht (Vollbild-3D, oben mittig Menü+Tabs, links Baumfeld, rechts Eigenschaften|Versionen, unten Befehlsfeld, unten links Meldung, Würfel neben dem rechten Feld) → Aufgaben 2, 3, 4, 5; §5 Stolperstellen 1 (sichtbare Mitte) → Aufgabe 1 und 3; 2 (Eingabe-Routing) → Aufgabe 3; 3 (egui_tiles entfällt, Umschalter, Ein-/Ausblenden) → Aufgaben 2, 4, 6; 5 (Tests) → Aufgabe 6 (und lagenabhängige Tests bleiben grün, weil Signaturen der Bereichsfunktionen gleich bleiben); 9 (Fenster-Sonderheiten: Größen-Durchgang) → Aufgabe 4 (B3); F2 (freie Fläche auch in Zeichnung/Skizze) → Aufgaben 3, 6; F3 (Kamera-Versatz) → Aufgabe 1; F4 (Tests dürfen wandern) → Aufgabe 6; F7 (Hell/Dunkel je Tab) → Global Constraints, Aufgabe 4 (i); F8 (Befehlsfeld nur Abkürzung, Modusschalter nichts erfinden) → B6, Aufgabe 5. **Nicht in diesem Plan:** neues Layout der Zeichnung (Etappe C), Skizzenfelder (C), Dialoge (D), Aufräumen der `pub`-Fläche (D).
**Schritt-Scan:** Tests nennen Name, Eingaben und erwartete Werte; Funktionsrümpfe fehlen absichtlich; Aufgabe 5 (b) hängt an der Fähigkeit des Booleschen Dialogs (Vorwahl) und lässt dem Implementierer die Wahl mit passendem Test.
**Typkonsistenz:** `ShellState`, `ShellRects`, `layout` (Aufgabe 2) werden in 4, 5, 6 benutzt; `show_in`, `cube_rect` (3) in 6; `OrbitCamera::set_visible` (1) in 3; `menu_bar`-Signatur ändert sich nur in 6.
**Proportion:** Der Plan beschreibt Entscheidungen und Tests.
**Bekannte Risiken:** Aufgabe 1 (Kamera: Versatz in `projection` zieht Picking/Skizzen mit, `frame_bounds` braucht den sichtbaren Bereich); Aufgabe 3 (Pointer-Routing, Order); Aufgabe 4 (Area im ersten Bild, kleine Fenster); Aufgabe 6 (viele Tests brechen kurz). Der Controller setzt für 1, 3, 4, 6 opus (Kamera-/Eingabe-/Layout-Urteil), für 2, 5, 7 sonnet; Prüfung durch opus bei 1, 3, 4, 6.
