# Graphit – Etappe D (Dialoge, Aufräumen) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Alle Dialoge und Rückfragen schweben schmal und mittig im Graphit-Rahmen (Feldrahmen, Überschrift, Schließen-Symbol), passen bei 800 × 600 ins Fenster und rollen bei Bedarf; dazu die Aufräumarbeiten aus der Abschlussprüfung von Etappe C (öffentliche Schnittstelle des Würfels, Robustheit, Beschriftung, doppelter Skizzenhinweis, Moduswechsel, Werkzeugfeld) und Abschluss von Teilprojekt 13.

**Architecture:** Ein neues Modul `dialog.rs` baut jedes Dialogfenster an einer Stelle (Größenklasse, Mitte, Grenzen, Kopfzeile mit Schließen-Symbol, Verwerfen des ersten Größen-Durchgangs, Rollen); alle Aufrufer von `theme::window` und `export_dialog::wide_window` wandern dorthin, danach entfallen beide samt `DIALOG_TOP`/`DIALOG_MARGIN`. Würfel-Interna werden `pub(crate)`, ihre Tests wandern als Unit-Tests ins Modul; ein Wächtertest verhindert neue rohe Fenster.

**Tech Stack:** Rust, egui/eframe 0.36 (`Window` mit `title_bar(false)`, `pivot`, `constrain_to`, `vscroll`, `request_discard`), wgpu; Tests im eigenen Harness (`egui::Context::run_ui`, `PlanoraApp::for_tests` mit `Frame::_new_kittest()`).

**Spec:** `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-design.md` (§2 Dialoge, §3 Ziele, §4 Absatz „Dialoge“, §5 Punkt 9, §6 Etappe D, §9–§12); offene Punkte: `docs/superpowers/offene-punkte.md` (Teilprojekt 13 Etappe A–C, Abschnitte „Öffentliche Schnittstelle“, „Würfel“, „Zeichnung“, „Skizze“, „Allgemein“, „Folgearbeit“). Plan 4 von 4; Etappe C ist gemergt (`46d3ca9`), Stand `fd99d79`.

## Global Constraints

- Dialoge: mittig schwebend, Breite nach Größenklasse aus `dialog::DialogSize` (Rückfrage 440, Normal 480, Breit 720 px Außenbreite; Spec §4 „etwa 480–720 px, … bis zu etwa 720 px“, Rückfragen „schmal“), Rahmen `theme::panel_frame(dark)`, Titel in `theme::heading_font()` und `text_primary`, Schließen-Symbol `Icon::Close` genau bei den Dialogen, die heute `.open(&mut open)` (also das egui-Kreuz) haben; Rückfragen (`prompts`), Projekte- und Versionen-Dialoge haben heute kein Kreuz und bleiben nur über ihre Knöpfe zu beenden.
- Jeder Dialog liegt bei 800 × 600, 1100 × 700, 1400 × 900, 1920 × 1080 ganz im Fenster (`ctx.content_rect()` minus `shell::SHELL_INSET` 16 rundum); ist der Inhalt höher, rollt er senkrecht im Dialog (Höchsthöhe = Fensterhöhe − 2 · 16).
- Dialoge bleiben `Order::Foreground` über allen Feldern (B4); der erste Größen-Durchgang wird verworfen (`ctx.request_discard`, wie bisher `prompts::window`), damit nichts flackert oder ein Bild zu spät erscheint.
- Karten nebeneinander bleiben nebeneinander und gleich hoch (`cards::card_row`, Nutzerregel); auch in 720 px.
- Tastatur und Fokus wie bisher (Enter in Umbenennen/Neuer Ordner/Kommentar übernimmt, Fokus des Zahlenfelds der Skizze, Esc-Verhalten unverändert; Dialoge schließen nicht mit Esc, weil sie es heute auch nicht tun).
- Texte deutsch in einfacher Laiensprache, keine technischen Hinweise, keine Hinweise auf Datenherkunft (Nutzerregel §2); bestehende Beschriftungen bleiben, wo sie passen (Tests suchen sie).
- Farben nur aus `theme::palette`/`theme::`-Funktionen (Wächter `no_hard_coded_colors`), Einzeilen-Textfelder nur über `theme::text_field` (`no_raw_text_edits`), Zeichen nur aus Montserrat/Symbolschrift (`tests/fonts.rs`). Neu: Fenster nur über `dialog::Dialog` (Wächter `no_raw_windows`, Aufgabe 3).
- Keine neuen Funktionen (Spec §3); Kamera-Mathematik unverändert; `planora-render` nur Zusätze und Sichtbarkeits-Einschränkungen (Aufgabe 4).
- Tests dürfen mit dem Layout umgeschrieben werden, geprüft wird weiter dasselbe Verhalten (F4); Testabdeckung darf beim Verschieben von Tests nicht sinken (gleiche Zusicherungen, nur anderer Ort).
- Geschützte Pfade unverändert gegenüber 945fa67: `crates/planora-kernel/**`, `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`.
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests mit `-j 2` (nie `cargo clean`). Am Ende `cargo test --workspace -j 2` und `cargo deny check`.
- Jeder Commit endet mit der Zeile `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` (Vorgabe des Nutzers für diese Etappe).

**Festlegungen (Entscheidung Entwurfsverfasser, der Nutzer kann sie kippen; Nutzer-Entscheidungen sind markiert):**
- **D1 (Nutzer)** Alle Dialoge schmaler und mittig; die bisher breiten (Exportieren, Netz importieren, Neue Zeichnung, Maß bearbeiten) bis etwa 720 px; Rückfragen mittig und schmal; `wide_window`, `DIALOG_TOP`, `DIALOG_MARGIN` und tote Konstanten entfallen.
- **D2** Feste Breiten je Klasse statt „nach Inhalt“ (deterministisch und prüfbar): `Prompt` 440 (Rückfragen, Umbenennen, Neuer Ordner, Verschieben, Kommentar ändern, Löschen), `Normal` 480 (Körper anlegen, Extrudieren/Drehen, Boolesche Operation, Feature bearbeiten, Rundung/Fase), `Wide` 720 (die vier bisher breiten). Bei schmalerem Fenster: höchstens Fensterbreite − 32.
- **D3** Mitte = Mitte von `ctx.content_rect()` (nicht der freien Fläche). Rückfragen sind fest verankert (wie heute). Alle anderen Dialoge öffnen mittig, lassen sich aber verschieben (an der Fläche des Dialogs außerhalb von Bedienelementen) und bleiben im Fenster (`constrain_to`): Beim Rundungsdialog muss man Kanten im Modell anklicken, die ein fest mittiger Dialog verdecken kann.
- **D4** Eigene Kopfzeile statt egui-Titelleiste (`title_bar(false)`): Titel links in `heading_font`, rechts das Schließen-Symbol als randloser Symbolknopf (16 px, `theme::frameless`, Tooltip und `widget_info` „Schließen“, Fokusring über `theme::focus_ring`), darunter `SPACE_2` Abstand, kein Trennstrich.
- **D5** Würfel-Beschriftung: `LABEL_MIN_FACING` 0,5, `LABEL_FULL_FACING` 0,7, `LABEL_FILL` 0,95. **Nutzerentscheidung: Würfel von 84 auf 100 px (`CUBE_SIZE` 100, ändert Ruling C1; Fläche dann etwa 33 px, „Rechts“ in Montserrat Medium 9 pt etwa 31 px passt); Mindestschrift 9 pt.** Alle Tests und Layoutwerte, die 84 voraussetzen (Rand im Layout, Home-Knopf, Namensstreifen, freie Fläche, `view_cube*.rs`), werden angepasst.
- **D6** Statusfeld im Skizzenmodus zeigt nur „Skizzen-Modus: „Name““; der lange Tastenhinweis steht als Hover-Text des Statusfelds (`SKETCH_KEYS_HINT`), der Fuß zeigt weiter Zustand, Werkzeughinweis und „Skizze beenden“.
- **D7** Werkzeugfeld der Zeichnung so breit wie sein Inhalt (gemerkte Wunschbreite wie beim Fußfeld der Skizze), höchstens so breit wie bisher, mittig; `free` ändert sich dadurch nicht (es hängt nur an der Höhe).
- **D8** Moduswechsel: Ändert sich der Modus (`shell_panels::mode_of`) während eines Bilds, wird das Bild verworfen und wiederholt (`shell_panels::discard`), sodass kein Feld des alten Modus ein Bild lang zu sehen ist.

## Review Focus

1. **Rundungsdialog und Kantenwahl:** Ein mittiger Dialog liegt über der Modellmitte; Kanten müssen weiter anklickbar sein (Dialog verschiebbar, Klick neben den Dialog erreicht das Modell). → Aufgabe 3, Test (h).
2. **Hoher Inhalt bei 800 × 600:** Exportieren mit Dateiwahl (Liste 220 px), Fortschritt und Meldungen, „Maß bearbeiten“ mit Fehlerliste, Speichern unter: Dialog bleibt im Fenster, rollt, der Aktionsknopf ist erreichbar. → Aufgabe 2, Test (d); Aufgabe 3, Test (b).
3. **Wachsender Inhalt und Fenstergröße ändern:** Statuswechsel (Berechnen → Fertig → Fehler) oder Verkleinern des Fensters bei offenem Dialog: Dialog bleibt mittig um seinen Mittelpunkt bzw. im Fenster, springt nicht, flackert nicht. → Aufgabe 1, Test (f); Aufgabe 2, Test (e).
4. **Karten in 720 px:** Lange Beschriftungen („STEP – für andere CAD-Programme (exakt)“, Zeichnungsinfos) brechen in der Karte um statt sie zu verbreitern; Karten bleiben nebeneinander und gleich hoch, nichts ragt aus dem Dialog. → Aufgabe 2, Test (c).
5. **Fokus und Tastatur:** Enter im Textfeld eines Dialogs übernimmt weiter; das neue Schließen-Symbol stiehlt keinen Fokus beim Öffnen; Rückfragen ohne Schließen-Symbol bleiben nur über ihre Knöpfe beendbar. → Aufgabe 3, Test (f).

---

### Task 1: Dialog-Rahmen (`dialog.rs`)

**Files:**
- Create: `crates/planora-app/src/dialog.rs`
- Modify: `crates/planora-app/src/lib.rs` (`pub mod dialog;`)
- Test: `crates/planora-app/tests/dialog_frame.rs` (neu)

**Interfaces:**
- Consumes: `theme::{panel_frame, heading_font, palette, frameless, focus_ring, SPACE_2}`, `shell::SHELL_INSET`, `icons::{Icon, text}`.
- Produces:
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DialogSize { Prompt, Normal, Wide }
impl DialogSize { pub const fn width(self) -> f32; /* 440, 480, 720 */ }
/// Außenbreite und größte Außenhöhe im Fenster `content` (Rand SHELL_INSET rundum).
pub fn limits(content: egui::Rect, size: DialogSize) -> (f32, f32);
pub struct Dialog<'a> { /* title, id, size, open: Option<&'a mut bool>, fixed: bool */ }
impl<'a> Dialog<'a> {
    pub fn new(title: impl Into<String>, size: DialogSize) -> Self;   // Id aus dem Titel wie egui::Window::new
    pub fn id(self, id: egui::Id) -> Self;
    pub fn open(self, open: &'a mut bool) -> Self;                    // zeigt das Schließen-Symbol; Klick setzt *open = false
    pub fn fixed(self) -> Self;                                       // verankert, nicht verschiebbar (Rückfragen)
    pub fn show<R>(self, ctx: &egui::Context, add_contents: impl FnOnce(&mut egui::Ui) -> R)
        -> Option<egui::InnerResponse<Option<R>>>;
}
pub const CLOSE_LABEL: &str = "Schließen";
```
`show`: `egui::Window::new(title).id(..).title_bar(false).frame(panel_frame(dark)).order(Foreground).collapsible(false).resizable(false).fade_in(false).fade_out(false)`, Innenbreite min = max = `limits().0 − frame.total_margin().sum().x`, `max_height` aus `limits().1`, `vscroll(true)`, `constrain_to(content)`; `fixed` → `anchor(CENTER_CENTER, 0)`, sonst `pivot(CENTER_CENTER).default_pos(content.center())`. Im Inhalt zuerst `if ui.is_sizing_pass() { ctx.request_discard(..) }`, dann Kopfzeile (D4), dann `add_contents`. Bei `open == Some(&mut false)` nichts zeigen, `None` liefern.

- [ ] **Step 1: Failing tests (`dialog_frame.rs`, eigener Dialog mit Text und Knopf im Harness, Theme gesetzt, Hell und Dunkel):** (a) `size_classes` (440/480/720; `limits` bei 800 × 600 = (440, 568)/(480, 568)/(720, 568), bei 1920 × 1080 = (…, 1048)); (b) `dialog_is_centred_with_the_class_width` (Fenster 800 × 600 und 1400 × 900: Rechteck des Bereichs `area_rect` Breite = Klasse ± 1, Mitte = Fenstermitte ± 1, ganz in `content.shrink(16)`); (c) `panel_frame_heading_and_close_icon` (gemalte Fläche `bg_panel` mit 1-px-`border_subtle`, Rundung `RADIUS_LG`; Titel in `heading_font` und `text_primary`; mit `.open` ein Knopf mit `widget_info` „Schließen“ rechts oben im Dialog, Klick setzt `open` auf false und im nächsten Bild ist der Dialog weg; ohne `.open` kein solcher Knopf); (d) `order_is_foreground`; (e) `first_frame_is_final` (nach genau einem `ctx.run` mit neu geöffnetem Dialog: Dialog ist gemalt und steht schon mittig, `area_rect` gleich dem nach dem zweiten Bild); (f) `tall_content_scrolls_and_stays_inside` (Inhalt 1500 px hoch bei 800 × 600: Höhe ≤ 568, ganz im Fenster; Mausrad über dem Dialog bringt einen Knopf am Ende ins Bild und er ist klickbar; Inhalt wächst von 100 auf 300 px: Mitte bleibt ± 1); (g) `normal_dialog_moves_fixed_one_does_not` (Ziehen an freier Dialogfläche verschiebt `Normal`, nicht `fixed`; Ziehen über den Rand hinaus: bleibt im Fenster); (h) `close_icon_does_not_take_focus_on_open` (nach dem Öffnen hat kein Widget des Kopfs den Fokus; ein Textfeld mit `request_focus` im Inhalt behält ihn).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test dialog_frame -j 2`
- [ ] **Step 3: Implement** nach Interfaces.
- [ ] **Step 4: Run → PASS**, clippy, fmt, Wächter (`no_hard_coded_colors`, `fonts`).
- [ ] **Step 5: Mutationsbeweise:** Verwerfen des Größen-Durchgangs entfernen → (e) rot; `constrain_to` entfernen → (g) rot; `vscroll(false)` → (f) rot.
- [ ] **Step 6: Commit** — `feat: Dialog-Rahmen im Graphit-Stil (mittig, Größenklassen, Schließen-Symbol)`.

### Task 2: Breite Dialoge mit Karten (Exportieren, Netz importieren, Neue Zeichnung, Maß bearbeiten)

**Files:**
- Modify: `crates/planora-app/src/export_dialog.rs` (`wide_window`, `DIALOG_MARGIN`, `DIALOG_TOP`, `MIN_WIDTH` entfernen), `import_dialog.rs`, `drawing_dialog.rs`, `dimension_dialog.rs`, ggf. `cards.rs` (nur falls Spalten in 720 px nicht umbrechen)
- Test: `tests/export_dialog.rs`, `export_dimensions.rs`, `export_drawing.rs`, `export_step.rs`, `import_dialog.rs`, `dimension_dialog.rs`, `drawing_ui.rs` (soweit Dialoglage), `exchange_infra.rs`, `shell_app.rs` (`dialogs_open_above_fields`)

**Interfaces:**
- Consumes: `dialog::{Dialog, DialogSize::Wide}` (Aufgabe 1), `cards::card_row`.
- Produces: die vier Dialoge über `Dialog::new(title, DialogSize::Wide).open(&mut open).show(..)`; `last_rect`/`last_cards` bleiben (Tests lesen sie). `export_dialog` exportiert `DIALOG_MARGIN`, `DIALOG_TOP` und `wide_window` nicht mehr.

- [ ] **Step 1: Failing tests (umschreiben):** (a) in `export_dialog.rs`, `export_step.rs`, `export_drawing.rs`, `export_dimensions.rs`, `import_dialog.rs` statt „volle Breite“: `the_window_is_centred_720_wide_with_equal_cards_and_plain_texts` (bei 1200-px-Testfenster Breite 720 ± 1, Mitte = Fenstermitte ± 1, Karten wie bisher gleich hoch, alle bisher gesuchten Wörter da); (b) dieselbe Prüfung für `NewDrawingDialog` und `DimensionDialog` (`dimension_dialog.rs`, Testfenster 1600 × 1000 bleibt); (c) `cards_stay_side_by_side_and_inside_at_800x600` für alle vier (Karten gleiche Oberkante, Höhen ± 0,5 nach dem Einschwingen, jede Karte im Dialogrechteck, kein Text ragt aus seiner Karte; Export mit STEP-Format und mit Zeichnungsformat); (d) `export_dialog_scrolls_at_800x600` (mit Dateiwahl offen und Status `Ready`: Dialog ≤ 568 hoch, „Exportieren …“ nach Mausrad sichtbar und klickbar); (e) `growing_status_keeps_the_dialog_in_place` (Export: Berechnen → Fertig → Fehler, Mitte bleibt ± 1, Dialog im Fenster); (f) `shell_app::dialogs_open_above_fields` neu bei 800 × 600 (der 720-px-Dialog liegt über beiden Seitenfeldern; Klick auf ein Feld außerhalb lässt ihn vorn) und bei 1400 × 900 (mittig zwischen den Spalten, über dem 3D-Bereich, Foreground); (g) Quelltext-Prüfung `wide_window_is_gone` in `tests/dialog_frame.rs` (kein `wide_window`, `DIALOG_TOP`, `DIALOG_MARGIN` in `src/`).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test export_dialog --test export_step --test export_drawing --test export_dimensions --test import_dialog --test dimension_dialog --test shell_app --test dialog_frame -j 2`
- [ ] **Step 3: Implement:** Aufrufe umstellen; tote Konstanten entfernen; Zeichenbreiten in Karten (z. B. `ComboBox::width(ui.available_width())`) behalten; Fortschrittsbalken `PROGRESS_WIDTH` 280 bleibt (passt in 696 px).
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test exchange_infra --test drawing_ui -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** `DialogSize::Normal` statt `Wide` beim Export → (a) rot; `card_row` durch senkrechte Folge ersetzen → (c) rot.
- [ ] **Step 6: Commit** — `feat: breite Dialoge mittig in 720 px, wide_window entfernt`.

### Task 3: Übrige Dialoge und Rückfragen, `theme::window` entfernen, Wächter

**Files:**
- Modify: `crates/planora-app/src/create_dialog.rs`, `boolean_dialog.rs`, `feature_dialog.rs`, `blend_dialog.rs` (`Normal`); `browser.rs` (Umbenennen, Rückfrage), `drawing_browser.rs` (Umbenennen, Rückfrage), `dimension_browser.rs` (Rückfrage), `projects_panel.rs` (`dialog_window` → `Dialog … .fixed()`), `versions_panel.rs` (Kommentar ändern, Löschen), `prompts.rs` (`window` über `Dialog::new(.., Prompt).id(..).fixed()`, `set_max_width(460.0)` entfällt) (`Prompt`); `theme.rs` (`window` entfernen)
- Test: `crates/planora-app/tests/dialogs.rs` (neu), `tests/no_raw_windows.rs` (neu), `tests/viewport_shell.rs` (statt `theme::window` den `Dialog`), `graphit_style.rs`, `sketch_fields.rs` (Quelltextprüfung auf `Dialog` erweitern), bestehende `blend_ui.rs`, `boolean_ui.rs`, `feature_dialog.rs`, `projects_ui.rs`, `versions_ui.rs`, `tree_ui.rs`, `drawing_browser.rs`, `dimension_browser.rs`, `close_handling.rs`, `sketch_dialogs.rs` nur soweit Lage/Breite geprüft wird

**Interfaces:**
- Consumes: `dialog::{Dialog, DialogSize}` (Aufgabe 1).
- Produces: kein `theme::window` mehr; Schließen-Symbol genau dort, wo heute `.open(&mut open)` steht (u. a. Umbenennen und Rückfrage in `browser`, `drawing_browser`, Rückfrage in `dimension_browser`); ohne Schließen-Symbol und `fixed` wie heute verankert: `projects_panel`-Dialoge (heute `anchor`, kein `.open`), `versions_panel`-Dialoge (kein `.open`) und alle `prompts`; die übrigen `Prompt`-Klasse-Dialoge öffnen mittig und sind verschiebbar (D3).

- [ ] **Step 1: Failing tests:** `dialogs.rs` (App über `PlanoraApp::for_tests`, Fenster 800 × 600 und 1400 × 900, Hell und Dunkel; jeder Dialog wird über seinen Zustand geöffnet: `Workspace::{dialog (Quader, Zylinder, Extrudieren, Drehen), boolean_dialog, feature_dialog, blend_dialog (Rundung und Fase)}`, `AppState::{pending_delete, rename_draft, pending_delete_drawing, rename_drawing_draft, pending_delete_dimension}`, Projekte-Dialoge, Versionen-Dialoge, alle sechs `Prompt`-Arten): (a) `every_dialog_has_its_size_class` (Breite der Foreground-Ebene unter dem Titel = Klasse nach D2 ± 1); (b) `every_dialog_is_centred_and_inside_the_window` (Mitte ± 1, ganz in `content.shrink(16)`, auch „Speichern unter“ bei 800 × 600); (c) `every_dialog_uses_the_graphit_frame_and_heading` (Fläche `bg_panel`, Titel `heading_font`); (d) `close_icon_where_the_window_had_one` (Liste wie oben: Schließen-Symbol vorhanden ⇔ früher `.open`; Klick schließt und setzt den Zustand auf `None` wie früher das Kreuz); (e) `dialog_texts_are_plain` (alle sichtbaren Texte aller Dialoge enthalten keines von „Spec“, „egui“, „Kernel“, „Autodesk“, „Fusion“, „Graphit“, „Phosphor“, „Montserrat“, „Roboto“, „TODO“, „Debug“, „Datei stammt“); (f) `keyboard_like_before` (Umbenennen im Baum, in Zeichnungen und Neuer Ordner: Enter im Textfeld übernimmt; Kommentar ändern: Enter übernimmt; Rückfrage „Änderungen speichern?“ bleibt ohne Schließen-Symbol); (g) `dialogs_in_all_modes_above_fields` (Modell, Zeichnung, Skizze: Foreground über jedem Feld, das der Dialog überdeckt); (h) `blend_dialog_can_be_moved_and_edges_stay_clickable` (Rundungsdialog öffnen, an der Dialogfläche nach links oben ziehen; danach Klick auf eine Kante des Quaders in der Fenstermitte wählt sie, Zahl „Gewählte Kanten: 1“). `no_raw_windows.rs`: (i) `windows_only_through_dialog` (in `src/*.rs` außer `dialog.rs` kein `egui::Window`, kein `theme::window(`, kein `wide_window`; Selbsttest des Scanners mit einem Beispieltext).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test dialogs --test no_raw_windows -j 2`
- [ ] **Step 3: Implement:** Aufrufe umstellen; feste Inhaltsbreiten (`set_max_width(360.0)`, `460.0`) entfernen; `theme::window` löschen und seine Tests in `tests/theme.rs`/`viewport_shell.rs` auf `Dialog` umstellen.
- [ ] **Step 4: Run → PASS** und `cargo test -p planora-app -j 2` komplett (viele Dialogtests), clippy, fmt, Wächter.
- [ ] **Step 5: Mutationsbeweise:** Rundungsdialog `fixed()` → (h) rot; Schließen-Symbol an einer `prompts`-Rückfrage → (d) rot; ein `egui::Window::new` in `browser.rs` → (i) rot.
- [ ] **Step 6: Commit** — `feat: alle Dialoge und Rückfragen im Graphit-Rahmen, theme::window entfernt`.

### Task 4: Würfel aufräumen (öffentliche Schnittstelle, Robustheit, Beschriftung)

**Files:**
- Modify: `crates/planora-app/src/viewport.rs`, `crates/planora-render/src/viewcube.rs`, `crates/planora-render/src/camera.rs` (`set_view_region`), `crates/planora-render/src/lib.rs` (Re-Exporte)
- Test: `crates/planora-render/tests/viewcube.rs` → Unit-Tests in `src/viewcube.rs` (`#[cfg(test)] mod tests`), `tests/viewcube_regions.rs`, `crates/planora-app/tests/view_cube.rs`, `tests/view_cube_artefacts.rs`; neue Unit-Tests in `crates/planora-app/src/viewport.rs` (`#[cfg(test)] mod cube_tests`, eigene Datei `src/viewport_cube_tests.rs` über `#[path]`)

**Interfaces:**
- Consumes: bestehende `CubeRegion`, `cube_pieces_visible`, `pieces_hit_test`, `current_region`, `face_text_frame`.
- Produces in `planora-render`: `CubeRegion::is_valid(self) -> bool` (Kante: zwei verschiedene Achsen in Reihenfolge Z vor Y vor X; Ecke: `x` ∈ {Links, Rechts}, `y` ∈ {Vorne, Hinten}, `z` ∈ {Oben, Unten}); `normal()` mit `normalize_or_zero`; `OrbitCamera::set_view_region` lässt die Kamera bei ungültigem Bereich unverändert; `current_region` liefert `None` bei nicht endlicher Kamera. Nur noch `#[cfg(test)]`/privat: `CubeFace`, `cube_faces`, `cube_hit_test`, `point_in_quad` (entfällt, `point_in_convex` direkt), `cube_pieces` (Tests nutzen `cube_pieces_visible(cam, 0.02)`), `REGION_COUNT` `pub(crate)` (Test prüft `all_regions().len() == 26`). In `planora-app/viewport.rs` `pub(crate)`: `CUBE_SCALE`, `LABEL_MIN_FACING`, `LABEL_FULL_FACING`, `PIECE_MIN_FACING`, `PIECE_FULL_FACING`, `FaceFrame`, `face_label_transform`, `visible_pieces`, `clickable_pieces`, `cube_region_at`, `piece_opacity`, `label_opacity`, `cube_to_screen`, `name_rect`. Öffentlich bleiben `CUBE_SIZE` (jetzt 100), `cube_rect`, `home_rect`, `show_in`, `show`, `render_palette` und genau ein Testhaken `#[doc(hidden)] pub fn region_point(camera: &OrbitCamera, cube: Rect, region: CubeRegion) -> Option<Pos2>` (Schwerpunkt des anklickbaren Stücks, für Klicktests der Integrationstests). Beschriftung nach D5; `label_font` wird einmal je Bild gerechnet und in `ctx.data` unter (`pixels_per_point`, Schriftatlas-Größe) gemerkt; wächst der Atlas beim Bauen der Netze, wird das Bild verworfen (`ctx.request_discard("Würfelbeschriftung: Schriftatlas gewachsen")`).

- [ ] **Step 1: Failing tests:** `viewcube_regions.rs`: (a) `invalid_regions_are_detected_and_ignored` (`Edge{Top, Bottom}`, `Edge{Front, Top}` (falsche Reihenfolge), `Corner{x: Top, ..}`: `is_valid` false, `normal()` endlich, `set_view_region` ändert Gier/Neigung/Abstand nicht; alle 26 aus `all_regions` gültig); (b) `nan_camera_has_no_current_region_and_no_pieces_panic` (Gier, Neigung oder Abstand NaN/∞: `current_region` = None, `cube_pieces_visible` panikt nicht). App-Unit-Tests (`viewport_cube_tests.rs`): (c) die bisherigen Zusicherungen aus `view_cube.rs`/`view_cube_artefacts.rs`, die Interna brauchen (`face_label_transform`, Deckkraft, `cube_region_at`, Konstanten), unverändert mit neuen Schwellen; (d) `labels_fade_between_0_5_and_0_7` (`label_opacity(0.5) == 0`, `(0.7) == 1`); (e) `label_font_is_as_large_as_fits` (längstes Wort ≤ 0,95 · Flächenbreite bei `CUBE_SIZE` 100; Schriftgröße ≥ 9 pt); (f) `label_font_is_cached_per_scale` (zwei Aufrufe im selben Bild liefern dasselbe, andere `pixels_per_point` ein anderes Ergebnis). Integration `view_cube.rs`: (g) `nan_camera_paints_nothing_invalid` (`show_in` mit NaN-Gier: kein Panik, keine nicht endliche Ecke in den gemalten Formen, kein Name unter dem Würfel); Klicktests über `region_point`. **Würfel zieht die Ansicht (Nutzerentscheidung):** (i) `dragging_on_the_cube_orbits` (Ziehen mit Primärtaste auf dem Würfel, mehr als 4 px Bewegung, ruft `camera.orbit(dx, dy)` auf wie im Viewport, in Modell und Skizze; ein Klick ohne Ziehen wählt weiter die Region; nach einem Zug löst das Loslassen keinen Regionsklick aus; Mausrad über dem Würfel zoomt weiter). Umsetzung: `cube_response.drag_motion()` wird nach Überschreiten der Klickschwelle wie `response.drag_motion()` behandelt, nur Primärtaste. Quelltext-Prüfung (h) `cube_internals_are_not_public` (keine `pub fn`/`pub const` der Liste oben ohne `(crate)` in `viewport.rs`; `cube_faces`, `cube_hit_test`, `cube_pieces`, `REGION_COUNT` nicht im Re-Export von `planora-render/src/lib.rs`).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-render -j 2` und `cargo test -p planora-app --lib --test view_cube --test view_cube_artefacts -j 2`
- [ ] **Step 3: Implement** nach Interfaces; Tests aus `crates/planora-render/tests/viewcube.rs` in den Unit-Test-Block verschieben und die Datei löschen; Testzahl der verschobenen Zusicherungen notieren (vorher/nachher gleich).
- [ ] **Step 4: Run → PASS**, alle Tests beider Pakete, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** `is_valid` immer true → (a) rot; `LABEL_MIN_FACING` zurück auf 0,3 → (d) rot; NaN-Filter in `visible_pieces` entfernen → (g) rot.
- [ ] **Step 6: Commit** — `refactor: Würfel-Schnittstelle verkleinert, robust gegen NaN und ungültige Bereiche, größere Beschriftung`.

### Task 5: Reste der Felder (Skizzenhinweis, Moduswechsel, Werkzeugfeld)

**Files:**
- Modify: `crates/planora-app/src/shell_panels.rs` (`status_line`, Moduswechsel), `crates/planora-app/src/app.rs` (Modus am Bildanfang merken), `crates/planora-app/src/drawing_view.rs` (Werkzeugfeld-Breite), `crates/planora-app/src/shell.rs` (`fit_tool`)
- Test: `tests/shell_panels.rs`, `tests/shell_commands.rs` (Hinweistext), `tests/shell_app.rs` (Moduswechsel), `tests/drawing_fields.rs`, `tests/shell_layout.rs`

**Interfaces:**
- Consumes: `shell_panels::{mode_of, discard, remember_width, last_layout}`, `ShellRects.tool`.
- Produces: `pub const SKETCH_KEYS_HINT: &str = "Esc bricht ab, Eingabe beendet, Entf löscht, mittlere Maustaste oder Umschalt + Ziehen verschiebt";` in `shell_panels.rs`; Statustext im Skizzenmodus `format!("Skizzen-Modus: {}", sk.name)`, Hover-Text `SKETCH_KEYS_HINT`; `pub fn fit_tool(tool: Rect, wanted: f32) -> Rect` in `shell.rs` (Breite `wanted.clamp(MIN_FIELD_W, tool.width())`, gleiche Mitte, gleiche Höhe); Werkzeugfeld merkt seine Wunschbreite (beide Zeilen ohne Rollen plus Rand) unter eigenem Schlüssel.

- [ ] **Step 1: Failing tests:** (a) `shell_panels.rs` `sketch_status_is_short_and_keys_are_in_the_hover` (Text „Skizzen-Modus: Skizze 1“; Hover zeigt `SKETCH_KEYS_HINT`; im ganzen Bild steht „Esc bricht ab“ höchstens einmal sichtbar); `shell_commands.rs` Erwartung angepasst; (b) `shell_app.rs` `no_field_of_the_old_mode_in_the_first_frame` (Skizze per Doppelklick im Baum öffnen und Zeichnung per Menü öffnen und schließen: im ersten Bild nach dem Klick ist keine Ebene des alten Modus gemalt (`field(&c, "links")` fehlt im Skizzenmodus, Werkzeugfeld fehlt im Modell) und die Felder des neuen Modus stehen an ihren Rechtecken aus `layout_in_mode`); läuft (b) ohne Änderung grün, wird das in der Doku als „erledigt, geprüft“ vermerkt und D8 entfällt; (c) `shell_layout.rs` `fit_tool_is_centred_and_capped` (wanted 600 in tool 1000 breit → 600 mittig; wanted 2000 → tool; wanted 50 → 120); (d) `drawing_fields.rs` `tool_field_is_content_wide_on_large_windows` (1920 × 1080 und 2560 × 1440: Breite = Inhaltsbreite ± 2 < bisherige Breite, mittig, kein seitlicher Balken; 800 × 600 und 1100 × 700 unverändert, rollt bei 800 × 600; `free` gleich wie vorher).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test shell_panels --test shell_commands --test shell_app --test shell_layout --test drawing_fields -j 2`
- [ ] **Step 3: Implement** nach Interfaces und D6–D8.
- [ ] **Step 4: Run → PASS**, dazu `--test drawing_ui --test sketch_fields --test graphit_style`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Statustext wieder lang → (a) rot; Werkzeugfeld volle Breite → (d) rot; (falls D8 umgesetzt) Verwerfen beim Moduswechsel entfernen → (b) rot.
- [ ] **Step 6: Commit** — `fix: Skizzenhinweis einmal, Moduswechsel ohne Zwischenbild, Werkzeugfeld nach Inhalt`.

### Task 6: Dokumentation, Gesamtprüfung, Haltepunkt 4

**Files:**
- Modify: `README.md` (Abschnitt „Neues Aussehen (Graphit)“: Dialoge), `docs/superpowers/offene-punkte.md` (neuer Abschnitt „Teilprojekt 13 – Graphit-Oberfläche, Etappe D (Dialoge, Abschluss)“; in Etappe A–C erledigte Punkte als erledigt markieren: `wide_window`/`DIALOG_TOP`, doppelter Skizzenhinweis, Ein-Bild-Moduswechsel, Werkzeugfeld volle Breite, NaN/ungültige Bereiche, Galley-Cache, `cube_faces`/`cube_hit_test`), `docs/superpowers/specs/2026-10-16-graphit-oberflaeche-design.md` (neuer §13 „Festlegungen der Umsetzung Etappe D“ mit D1–D8, Rulings, Abweichungen, verbleibende `pub`-Fläche), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)

- [ ] **Step 1: Tote Konstanten suchen und entfernen:** für jede `pub const`/`pub(crate) const` in `crates/planora-app/src/*.rs` die Fundstellen zählen (`rg -n "\bNAME\b" crates/planora-app`); nur in ihrer Definition vorkommende entfernen, Liste in `offene-punkte.md`.
- [ ] **Step 2: Doku** in Laiensprache; `offene-punkte.md`: Geliefertes, D1–D8, Grenzen (z. B. Beschriftung ~8 pt, verbleibende `pub`-Fläche aus Etappe A/B, falls nicht aufgeräumt), Zahlen, Handliste: alle Dialoge in 800 × 600, 1100 × 700 und groß, Hell und Dunkel (Quader, Zylinder, Kegel, Kugel, Torus, Extrudieren, Drehen, Boolesche Operation, Feature bearbeiten, Rundung, Fase, Exportieren in allen Formaten, Netz importieren, Neue Zeichnung, Maß bearbeiten, Umbenennen/Löschen in Baum, Zeichnungen, Maßen, Projekte (Neuer Ordner, Umbenennen, Verschieben, Löschen), Versionen (Kommentar, Löschen), Rückfragen (Änderungen speichern, Kommentar, Speichern unter, Sperre übernehmen, Autosicherung, Hinweis)); Karten gleich hoch; Texte nicht abgeschnitten; Rollen im Dialog bei 800 × 600; Schließen-Symbol; Dialog verschieben; Rundung: Kanten anklicken neben dem Dialog; Würfel-Beschriftung lesbar; Skizzenstatus kurz, Tastenhinweis beim Überfahren; Werkzeugfeld der Zeichnung im großen Fenster schmal und mittig; Moduswechsel ohne Aufblitzen.
- [ ] **Step 3: Gates** mit Ausgabe: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2` (~16 min); `cargo deny check`; `git diff --stat 945fa67 -- crates/planora-kernel crates/planora-mesh/src crates/planora-step crates/planora-exchange` leer; `git status` sauber.
- [ ] **Step 4: Commit** — `docs: Graphit Etappe D, Teststand und Abschluss Teilprojekt 13`.
- [ ] **Step 5 (Controller):** Haltepunkt 4: Nutzer startet `cargo run -p planora-app --release` selbst und geht alle Dialoge durch; Beanstandungen sammeln; Gesamtreview des Zweigs, eine Korrekturrunde, Merge-Frage.

---

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung (Etappe D):** §4 „Dialoge“ (mittig, 480–720, `panel_frame`, `heading`, Schließen-Symbol, Karten gleich hoch, Rückfragen mittig und schmal) → Aufgaben 1–3, D1–D4; §2 Texte ohne Technik und Datenherkunft → Aufgabe 3 (e); §5 Punkt 9 (Verwerfen des ersten Größen-Durchgangs, neue Breitenregel, Schließen-Kreuz) → Aufgabe 1; §6 Etappe D (Reste `wide_window`, `DIALOG_TOP`, tote Konstanten; Doku; Gesamtprüfung) → Aufgaben 2, 6; Mindestfenster 800 × 600 mit Rollen → Aufgaben 1, 2; Punkte aus der Prüfung von Etappe C (`pub`-Fläche Würfel/Render, NaN, ungültige Bereiche, Galley-Cache, Beschriftung, doppelter Hinweis, Moduswechsel, Werkzeugfeld) → Aufgaben 4, 5. **Nicht in diesem Plan** (offene Frage an den Nutzer): Aufräumen der übrigen `pub`-Fläche aus Etappe A/B (`theme`, `icons`, `app::{CreateKind, …}`, Testhaken in `shell_panels`/`PlanoraApp`), Dialog-Checkboxen und Kontextmenü-Symbole aus der Folgearbeit von Etappe A, Fase-Test ohne Quelltextsuche.
**Schritt-Scan:** Tests nennen Namen, Eingaben und erwartete Werte; Rümpfe fehlen absichtlich. Aufgabe 5 (b) darf ohne Codeänderung grün sein (dann D8 entfällt, dokumentiert).
**Typkonsistenz:** `dialog::{Dialog, DialogSize, limits, CLOSE_LABEL}` (1) in 2 und 3; `CubeRegion::is_valid`, `region_point` (4); `SKETCH_KEYS_HINT`, `shell::fit_tool` (5).
**Proportion:** Entscheidungen und Tests, keine Rümpfe.
**Bekannte Risiken:** egui-`Window` mit `title_bar(false)` und `pivot` beim Wachsen des Inhalts (Aufgabe 1 (f)); viele Dialogtests mit festen Lagen (Aufgabe 3); verschobene Würfeltests ohne Verlust (Aufgabe 4). Empfehlung Controller: opus für 1, 3, 4; sonnet für 2, 5, 6.
