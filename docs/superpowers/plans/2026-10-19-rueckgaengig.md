# Rückgängig und Wiederholen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **COMMIT-TRAILER (jeder Commit, ohne Ausnahme, wörtlich als letzte Zeile):**
> `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
> Frühere Agents haben hier wiederholt „Opus“ oder einen anderen Namen eingesetzt. Vor jedem `git commit` die Zeile Zeichen für Zeichen prüfen.
> Immer nur die genannten Pfade einzeln `git add`en (nie `git add -A`/`.`/`-u`).

**Goal:** Jede Änderung am Dokument lässt sich je Tab rückgängig machen und wiederholen (Strg+Z, Strg+Umschalt+Z/Strg+Y, Menü „Bearbeiten“, zwei Knöpfe im Befehlsfeld), bis zu 100 Schritte, mit Aktionsnamen; im Skizzenmodus wirkt es auf die lebende Skizze; der Punkt „ungespeichert“ folgt dem Verlauf.

**Architecture:** Zustandsbilder statt Befehle (Spec §6). Ein reines, generisches Modul `history.rs` hält `History<T>` (Schritte mit Nummern, Grenze 100 Schritte / 200 MB, Gespeichert-Marke) und `Tracker` (Änderungszähler je Bild vergleichen, Gesten zusammenfassen). Jeder `DocTab` hat eine `History<Snapshot>`; am Ende jedes Bildes vergleicht die App `Document::edit_count()` mit dem zuletzt gesehenen Wert und legt bei einer Änderung (nach Loslassen der Maus bzw. Verlassen des Eingabefelds) einen Schritt an, dessen Name aus dem Vergleich von vorher/nachher abgeleitet wird (`undo_labels.rs`). Rückgängig ersetzt nur `document` (neu: `Document::restore`, mit schnellem Weg ohne Neuberechnung, wenn sich nur Zeichnungen/Angaben unterscheiden, und mit weiter steigenden Änderungszählern), danach Namenszähler, Körperliste ohne Neueinpassen, Aufräumen von Auswahl, Dialogen und Werkzeugen. Die offene Skizze hat einen eigenen `History<SketchFeature>` im `AppState`.

**Tech Stack:** Rust (Workspace, keine neuen Abhängigkeiten), egui/eframe 0.36; Tests im eigenen Harness (`egui::Context::run_ui`, `PlanoraApp::for_tests`, `AppState`, `Workspace`), Vorbilder `crates/planora-app/tests/{shell_commands.rs, workspace.rs, tabs_ui.rs, dimension_perf.rs, feature_state.rs, sketch_state.rs}`.

**Spec:** `docs/superpowers/specs/2026-10-19-rueckgaengig-design.md` (vom Nutzer freigegeben samt §6a und den Vorschlägen aus §9: Verlauf je Tab, 100 Schritte / 200 MB, zwei Knöpfe zusätzlich zu Menü und Tasten, ein Schritt je Skizzenelement, Rückgängig auch nach dem Speichern). Vorbild für Stil und Muster: `docs/superpowers/plans/2026-10-19-schnittansichten.md`. Basis: `master` @ `2e283e1`; Arbeitszweig `feature/rueckgaengig` in eigenem Worktree.

## Global Constraints

- Texte deutsch in einfacher Laiensprache, keine technischen Hinweise. Feste Texte: Menü/Hilfetext „Rückgängig: {Aktion}“, „Wiederholen: {Aktion}“; ohne Schritt „Rückgängig“/„Wiederholen“ ausgegraut mit Hilfetext „Es gibt nichts rückgängig zu machen.“ bzw. „Es gibt nichts zu wiederholen.“; Fehler „Rückgängig ist nicht möglich.“ bzw. „Wiederholen ist nicht möglich.“; Sperre beim Export „Während des Exports ist das nicht möglich.“; Tastenkürzel im Menü „Strg+Z“ und „Strg+Y“ (Strg+Umschalt+Z wirkt ebenfalls).
- Werte aus der Spec: höchstens **100 Schritte** je Verlauf (`MAX_STEPS`), höchstens **200 MB** geschätzt (`MAX_BYTES = 200 * 1024 * 1024`), älteste fallen still heraus; Netze zählen nicht (geteilte `Arc`).
- **Nur `planora-app` und `planora-feature` ändern sich.** Ohne Diff gegen `2e283e1`: `crates/planora-kernel/**`, `crates/planora-mesh/**`, `crates/planora-step/**`, `crates/planora-exchange/**`, `crates/planora-project/**` (Dateiformat unverändert, Spec §6), `crates/planora-sketch/**`, `crates/planora-drawing/**`, `crates/planora-render/**`, `crates/planora-math/**`.
- **Keine neuen Abhängigkeiten**: `Cargo.toml`/`Cargo.lock`/`deny.toml` ohne Diff; `cargo deny check` grün mit den in `deny.toml` vermerkten Ausnahmen.
- **Anti-Ziel der Spec:** nie ein halber oder anderer Zustand. Schlägt das Zurückladen fehl, bleibt das Dokument unverändert, nur dieser Schritt wird verworfen, Meldung in `state.message` (Spec §6a.8).
- `AppState::from_snapshot` wird für Rückgängig **nicht** benutzt; Kamera, Hell/Dunkel, `instance`, `fit_pending` bleiben unberührt; die Ansicht wird nach Rückgängig/Wiederholen nie neu eingepasst (Spec §3, §6a.1).
- Farben nur über `theme::` (Wächter `no_hard_coded_colors`), Fenster nur über `dialog::Dialog` (`no_raw_windows`), Textfelder nur über `theme::text_field*` (`no_raw_text_edits`), Zeichen nur aus Montserrat/Roboto/Phosphor (`tests/fonts.rs`); Symbole aus Phosphor Light mit Codepunkt aus `assets/fonts/Phosphor-Light-style.css` (`tests/icons.rs`); gleiche Kartenhöhe in einer Reihe; Oberfläche über die ganze Breite.
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests mit `-j 2` **im Vordergrund** (langsamer Rechner; nie `cargo clean`; keine Hintergrundprozesse zurücklassen).
- **Tests müssen beißen:** Jede Aufgabe endet mit mindestens **10 Mutationen** (einzeln einspielen, roten Test notieren, zurücknehmen); Bericht „Mutation → roter Test“. Bleibt eine Mutation grün, wird der Test schärfer.
- Keine Streudateien: Testdateien in `std::env::temp_dir()`-Unterordnern, am Ende entfernt; `git status` am Ende sauber. Handproben entfallen (keine Ausgabedateien in diesem Teilprojekt).
- Bestehende Tests bleiben unverändert grün, außer den unter „Bestehende Tests, die sich ändern“ genannten.
- Abweichungen von der Spec stehen im Ledger des Controllers, am Ende in `docs/superpowers/offene-punkte.md` und im Abschnitt „Abweichungen bei der Umsetzung“ dieses Plans.

**Festlegungen des Plans (R1–R10; der Nutzer kann sie kippen):**
- **R1 Zählerfalle:** `Document::from_snapshot` setzt `edits`/`drawing_edits` auf 0. Die Zwischenspeicher der Zeichnung hängen an `(instance, edit_count)` bzw. `model_edit_count` (`drawing_state.rs:296`, `drawing_view.rs:695/822/910`, `sheet_dialog.rs:231`, `balloon_tool.rs:257`). Zwei Rückgängig hintereinander träfen sonst einen alten Eintrag mit Zähler 0. `Document::restore` hält deshalb beide Zähler **steigend**: voller Weg `edits = alt + 1`, `drawing_edits = alt + 1`; Zeichnungsweg `edits` unverändert, `drawing_edits = alt + 1`.
- **R2 Zwei Wege beim Zurückladen:** Gleichen sich Features, Marker, `next_id` und Netze (Zeiger) von Bild und aktuellem Dokument (`Snapshot::same_model`), ersetzt `restore` nur Zeichnungen, `next_drawing`, Projektangaben und Stücklistentexte (geprüft wie beim Laden) – keine Neuberechnung, Ansichtsergebnisse bleiben gültig. Sonst wird wie beim Laden gerechnet (`RestoreKind::Full`); Netze, die als derselbe `Arc` schon im Dokument liegen, werden nicht erneut geprüft.
- **R3 Schritt = Bild vorher:** `History.current` ist das Bild nach dem letzten Schritt. Erkennt der `Tracker` eine abgeschlossene Änderung, wird `state.snapshot()` gebildet; ist es inhaltsgleich (`Snapshot::same_content`), entsteht **kein** Schritt (etwa „Verschieben verworfen“, das intern zweimal ändert); sonst `record(after, bytes, describe_change(current, after))`.
- **R4 Geste:** Eine Änderung bleibt offen, solange am Bildende eine Maustaste gedrückt ist (`i.pointer.any_down()`) oder ein Eingabefeld die Tastatur hat (`viewport::typing(ctx)`); sie wird im ersten Bild ohne beides ein Schritt. Ziehen (Position, Maßlinie, Ansicht, Skizzenpunkt) und Tippen bis Enter/Verlassen sind so je **ein** Schritt; Mehrfachaktionen innerhalb eines Bildes ohnehin.
- **R5 Gleichheit von Skizzen:** `impl PartialEq for SketchFeature` vergleicht `sketch.revision()`, `plane`, `copies` (Doku von `Sketch::revision`: gleicher Stand heißt gleicher Inhalt; Kopien teilen den Stand). Damit leiten sich `PartialEq` für `FeatureKind` und `SnapshotFeature` ab. Ungleicher Stand bei gleichem Inhalt zählt als ungleich (vorsichtig: höchstens ein überflüssiger Schritt bzw. eine Neuberechnung).
- **R6 Namen der Schritte** werden aus dem Unterschied vorher/nachher abgeleitet (Tabelle in Aufgabe 5), nicht an den Aufrufstellen gesetzt; so bekommt jede heutige und künftige Aktion einen Namen, mindestens „Ändern“.
- **R7 Gespeichert-Marke:** `History` merkt `saved: Option<StepId>`. `is_dirty = !read_only && (Skizze offen || Tracker hat Ungesehenes/Offenes || current_id ≠ saved)`. Speichern übernimmt vorher offene Änderungen (`observe(false)`) und setzt die Marke; der Verlauf bleibt. Übernommene Autosicherung → `saved = None`. Neuer Zustand im Tab (Autosicherung, Wiederherstellen) → neuer Verlauf.
- **R8 Sperren:** Strg+Z/Strg+Y wirken nicht beim Tippen, bei offener Rückfrage (`ws.prompt`), bei „Über“ und bei offenem Dialog (`DocTab::dialog_open`). Menü und Knöpfe bleiben bei offenem Dialog bedienbar und schließen ihn (Spec §3: „offene Dialoge … werden geschlossen“); gesperrt sind sie nur in alten Versionen, bei offener Rückfrage (wie heute das ganze Menü/Befehlsfeld) und während das Export-Fenster eine Datei schreibt (Spec §7: dieses Fenster darf nicht ohne Meldung enden). Laufende Ansichtsberechnungen brechen über das Verwerfen des Zwischenspeichers ab (Spec §7 „bricht ab und führt dann aus“).
- **R9 Namenszähler** werden nach dem Zurückladen aus den Namen neu aufgebaut (`rebuild_name_counters`): Nach Rückgängig von „Quader 3“ heißt der nächste Quader wieder „Quader 3“.
- **R10 Skizze:** Solange eine Skizze offen ist, wirken alle Wege auf den Skizzenverlauf (`History<SketchFeature>`, Schlüssel `sketch.revision()`); der Dokumentverlauf ruht (das Dokument ändert sich in dieser Zeit nicht). Hat ein Werkzeug noch offene Punkte (`session.pending` nicht leer) und kamen seit dem letzten Schritt nur Punkte hinzu, bleibt der Schritt offen (erster Klick einer Linie gehört zur Linie). Rückgängig/Wiederholen in der Skizze leert `pending`, `pending_pos`, `selection`, `dragging`, `message` und setzt `last_report = Some(sketch.clone().solve())`. „Skizze beenden“ ist danach **ein** Dokumentschritt „Skizze bearbeiten“; ein unveränderter Entwurf ergibt keinen (vorhandene Prüfung in `commit_draft`).

## Review Focus

1. **Rückgängig nach „Löschen mit Folgen“** (Quader → Verschieben → Verrunden → Skizze darauf, Quader samt Folgen gelöscht, auch während genau diese Skizze offen war): ein Rückgängig bringt alle Features mit gleichen Kennungen und Namen zurück, Körper sichtbar, Zeitleistenmarke wie vorher; Wiederholen löscht alles wieder als ein Schritt. → Aufgabe 4 Test (c), Aufgabe 6 Test (h).
2. **Strg+Z beim Tippen oder bei offenem Fenster** (Feld im Fenster „Ändern“, Umbenennen, Positionsfeld, Fenster „Quader erzeugen“, Rückfrage „Löschen“): das Dokument bleibt unverändert, das Textfeld behält sein eigenes Rückgängig, nach Schließen wirkt Strg+Z wieder. → Aufgabe 8 Tests (b, c, d).
3. **Ziehen ist genau ein Schritt** (Position im Eigenschaftenfeld über 30 Bilder, Maßlinie im Blatt, Ansicht verschieben, Skizzenpunkt): ein Rückgängig stellt den Stand vor dem Ziehen her, Wiederholen den Endstand; Rückgängig und danach eine neue Aktion löschen die Wiederholen-Liste. → Aufgabe 1 Tests (d, g), Aufgabe 6 Tests (d, e), Aufgabe 7 Test (c).
4. **Rückgängig bei offener Zeichnung** (Ansicht verschoben, dann zurück; Quader gelöscht, dann zurück; zweimal Rückgängig hintereinander): das Blatt zeigt sofort den zurückgeholten Stand, nie ein altes Ergebnis; ein reiner Zeichnungsschritt startet keine neue Ansichtsberechnung; eine gelöschte Zeichnung schaltet auf das Modell zurück. → Aufgabe 3 Tests (c, d), Aufgabe 4 Tests (f, g).
5. **Punkt „ungespeichert“ und Skizzenmodus** (speichern, Rückgängig → Punkt, Wiederholen → kein Punkt; Rückgängig in offener Skizze trifft nur die Skizze, das Dokument bleibt; „Skizze beenden“ ist ein Schritt, der die ganze Skizze zurücknimmt): → Aufgabe 6 Tests (f, g), Aufgabe 7 Tests (a, e, f).

---

### Task 1: Reiner Verlauf (`history.rs`)

**Files:**
- Create: `crates/planora-app/src/history.rs` (Modulkommentar mit R3, R4, R7; keine egui-Typen)
- Modify: `crates/planora-app/src/lib.rs` (`pub mod history;`)
- Test: `crates/planora-app/tests/history.rs` (neu; `History<u32>` bzw. `History<String>` als Testzustand)

**Interfaces:**
- Consumes: `planora_feature::Snapshot` (nur für `snapshot_bytes`).
- Produces:
```rust
pub const MAX_STEPS: usize = 100;
pub const MAX_BYTES: usize = 200 * 1024 * 1024;
pub type StepId = u64;
pub struct History<T> { /* current, current_id, current_bytes, undo: VecDeque<Entry<T>>, redo: Vec<Entry<T>>, next_id, saved: Option<StepId>, limits */ }
impl<T: Clone> History<T> {
    pub fn new(initial: T, bytes: usize) -> History<T>;                 // current_id 0, saved Some(0)
    pub fn with_limits(initial: T, bytes: usize, max_steps: usize, max_bytes: usize) -> History<T>;
    pub fn current(&self) -> &T;
    pub fn current_id(&self) -> StepId;
    pub fn record(&mut self, after: T, bytes: usize, label: &'static str); // altes current → undo (mit label), neue Nummer, redo leeren, Grenzen
    pub fn undo_label(&self) -> Option<&'static str>;
    pub fn redo_label(&self) -> Option<&'static str>;
    pub fn peek_undo(&self) -> Option<&T>;
    pub fn peek_redo(&self) -> Option<&T>;
    pub fn commit_undo(&mut self) -> Option<&'static str>;   // nach gelungenem Zurückladen: current → redo, oberstes undo → current (mit seiner Nummer)
    pub fn commit_redo(&mut self) -> Option<&'static str>;
    pub fn discard_undo(&mut self);                           // nur den obersten undo-Schritt verwerfen (Zurückladen fehlgeschlagen)
    pub fn discard_redo(&mut self);
    pub fn undo_len(&self) -> usize; pub fn redo_len(&self) -> usize;
    pub fn bytes(&self) -> usize;                             // Summe undo + redo + current
    pub fn mark_saved(&mut self); pub fn forget_saved(&mut self); pub fn is_saved(&self) -> bool;
}
pub struct Tracker { /* seen: u64, pending: bool */ }
impl Tracker {
    pub fn new(count: u64) -> Tracker;
    pub fn observe(&mut self, count: u64, gesture: bool) -> bool; // true = jetzt einen Schritt bilden
    pub fn reset(&mut self, count: u64);                          // nach Zurückladen, ohne Schritt
    pub fn is_pending(&self, count: u64) -> bool;                 // pending || count != seen
}
pub fn snapshot_bytes(s: &planora_feature::Snapshot) -> usize;
```
Grenzen: nach `record` erst die ältesten undo-Einträge entfernen, bis `undo_len ≤ max_steps` und `bytes ≤ max_bytes`; reicht das nicht, die fernsten redo-Einträge. `snapshot_bytes`: `size_of::<Snapshot>()` + je Feature `size_of::<SnapshotFeature>()` + Namenslänge + bei Skizzen `variable_count()·8 + (Punkte + Kurven)·64 + Abhängigkeiten·64 + copies·64`; je Zeichnung `size_of::<Drawing>()` + Name + Ansichten·`size_of::<ViewSpec>()` + Maße·256 + Positionsnummern·`size_of::<Balloon>()` + 1024 für Blattangaben; Stücklistentexte je 256; **Netze 0**.

- [ ] **Step 1: Failing tests:**
  (a) `record_undo_redo_roundtrip` (0 → record 1 „A“ → record 2 „B“: `undo_label == Some("B")`; `peek_undo == Some(&1)`; `commit_undo == Some("B")`, `current == 1`, `redo_label == Some("B")`; `commit_redo` → `current == 2`; Nummern 0, 1, 2 und nach undo/redo wieder genau dieselben);
  (b) `empty_history` (`undo_label`, `redo_label`, `peek_*`, `commit_*` → `None`, `current` unverändert, `discard_*` ohne Wirkung);
  (c) `new_action_clears_redo` (0 → 1 → 2, undo, record 3 „C“: `redo_len == 0`, `current_id == 3` (neu, nicht 2), undo → 1);
  (d) `limit_100_steps` (`new`, 150 Aufnahmen: `undo_len == 100`, ältester erreichbarer Zustand ist 50, `MAX_STEPS == 100`);
  (e) `limit_bytes` (`with_limits(.., 1000, 100)`, je Schritt 30 Bytes: `bytes() ≤ 100`, ältester fällt zuerst; ein Zustand mit 500 Bytes → `undo_len == 0`, `current` ist er selbst; `MAX_BYTES == 209_715_200`);
  (f) `saved_marker` (`new` → `is_saved`; record → nicht; `mark_saved` → ja; undo → nein; redo → ja; undo, record neu → nie wieder ja ohne `mark_saved`; `forget_saved` → nein, auch bei Nummer 0);
  (g) `tracker_coalesces_gestures` (`new(5)`: `observe(5,false) == false`; `observe(6,true) == false`, `observe(9,true) == false`, `observe(9,false) == true`, danach `observe(9,false) == false`; `observe(4,false) == true` (nicht monoton); `is_pending(9)` während offener Geste wahr; `reset(20)` → `observe(20,false) == false`);
  (h) `discard_keeps_rest` (0 → 1 → 2 → 3, `discard_undo`: `current == 3`, `undo_len == 2`, `peek_undo == Some(&1)`; `discard_redo` analog);
  (i) `snapshot_bytes_counts_without_meshes` (leer > 0; ein Quader mehr → größer; Snapshot mit 1-Mio.-Dreiecke-Netz-`Arc` → gleich wie ohne Netz, wenn kein Feature dazukommt; Zeichnung mit 3 Ansichten > mit 1).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test history -j 2`
- [ ] **Step 3: Implement** nach Interfaces.
- [ ] **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: redo bei `record` nicht geleert; neue Nummer = alte + 0; undo stellt `current_id` nicht zurück; Grenze `<` statt `≤`; neueste statt älteste entfernen; Bytes ohne `current`; `saved` nach undo nicht geprüft; Tracker gibt während Geste `true`; Tracker setzt `pending` nicht zurück; `discard_undo` leert alles; Netzgröße mitgezählt.
- [ ] **Step 6: Commit** — `feat: Verlauf für Rückgängig und Wiederholen (rein, ohne Oberfläche)`.

### Task 2: Messung des Zurückladens (Haltepunkt)

**Files:**
- Create: `crates/planora-app/tests/history_perf.rs` (Modulkommentar: Zweck, Ergebnisse, Entscheidung)
- Modify: dieser Plan (Abschnitt „Messergebnis Aufgabe 2“ am Ende)

**Interfaces:**
- Consumes: `AppState` (öffentliche Felder `document`, `bodies`, `refresh`), `Document::from_snapshot`, `snapshot_bytes` (1); Bauhilfen nach Vorbild `tests/dimension_perf.rs` (Platte mit Bohrungen), `tests/nurbs_app.rs` (Spline-Extrusion), `tests/mesh_flow.rs` (Netz), `tests/section_tool.rs` (Zeichnung mit Schnitten).
- Produces: `fn medium_model() -> AppState` (20 Grundkörper, 10 Boolesche Operationen, 5 Verrundungen/Fasen, 1 Skizze mit Extrusion, Zeichnung mit 4 Ansichten), `fn heavy_model() -> AppState` (mittleres Modell + Platte 10 × 10 Bohrungen + Spline-Extrusion (Freiform) + Netz-Körper mit 200 000 Dreiecken + Zeichnung mit 4 Ansichten und 2 Schnitten + 30 Maße). Nur in dieser Testdatei.

- [ ] **Step 1: Tests:** (a) `models_have_the_promised_shape` (nicht ignoriert: mittleres Modell 37 Features, alle `FeatureStatus::Ok`; schweres Modell hat ≥ 1 Freiformfläche, ein Netz mit 200 000 Dreiecken, 6 Ansichten davon 2 mit `section`, 30 Maße); (b) `measure_restore` (`#[ignore]`, Release): je Modell und je 5 Läufen Median in ms für `snapshot()`, `snapshot_bytes`, `Document::from_snapshot`, `AppState::refresh` danach, davon Anteil der Netzprüfung (`ExchangeMesh::validate`); Ausgabe als Tabelle auf stdout.
- [ ] **Step 2: Run** `cargo test -p planora-app --test history_perf -j 2` (PASS für (a)), dann `cargo test --release -p planora-app --test history_perf -j 2 -- --ignored --nocapture` im Vordergrund.
- [ ] **Step 3: Ergebnis festhalten** im Abschnitt „Messergebnis Aufgabe 2“ (Tabelle, Rechner, Build) und im Modulkommentar. **Entscheidung:** Summe `from_snapshot + refresh` beim schweren Modell ≤ 1000 ms → synchron wie geplant (Aufgaben 3–10 unverändert). Größer → **stoppen und dem Controller melden**; Fortschrittsanzeige/Hintergrund (Spec §6a.6) braucht dann eine Planergänzung mit Zustimmung des Nutzers. `snapshot()` des schweren Modells > 50 ms → ebenfalls melden (läuft bei jeder Änderung).
- [ ] **Step 4:** clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)** an den Bauhilfen gegen Test (a): Boolesche Operationen weggelassen; Verrundung auf falscher Kante (Status Fehler); Spline als Linie; Netz halb so groß; Schnitt ohne `section`; Maße nicht angelegt; Ansicht vergessen; Bohrungsraster 9 × 10; Marker vor dem Ende; Netz-Körper nicht sichtbar.
- [ ] **Step 6: Commit** — `test: Aufwand des Zurückladens gemessen (Rückgängig)` (Pfade: `history_perf.rs`, dieser Plan).

### Task 3: Inhaltsgleichheit und Zurückladen im Dokument (`planora-feature`)

**Files:**
- Modify: `crates/planora-feature/src/model.rs` (`impl PartialEq for SketchFeature` nach R5; `#[derive(PartialEq)]` an `FeatureKind`), `crates/planora-feature/src/snapshot.rs` (`#[derive(PartialEq)]` an `SnapshotFeature`; `same_content`, `same_model`, `RestoreKind`, `Document::restore`; `from_snapshot` intern mit „diese Netze nicht erneut prüfen“), `crates/planora-feature/src/lib.rs` (Re-Export `RestoreKind`)
- Test: `crates/planora-feature/tests/restore.rs` (neu)

**Interfaces:**
- Consumes: vorhandene `Document::{snapshot, from_snapshot, set_drawings}`, `validate_project_info`, `parts::check_overrides`.
- Produces:
```rust
impl PartialEq for SketchFeature { /* revision, plane, copies */ }
impl Snapshot {
    pub fn same_content(&self, other: &Snapshot) -> bool; // alle Felder; Netze: gleiche Schlüssel und Arc::ptr_eq
    pub fn same_model(&self, other: &Snapshot) -> bool;   // features, marker, next_id, Netze
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum RestoreKind { Annotations, Full }
impl Document {
    /// R1/R2. Bei Fehler bleibt `self` unverändert.
    pub fn restore(&mut self, s: Snapshot) -> Result<RestoreKind, FeatureError>;
}
#[doc(hidden)] pub fn mesh_validations() -> u64;   // Zähler je Thread (nur für Tests)
```

- [ ] **Step 1: Failing tests:**
  (a) `same_content_detects_each_field` (Quader + Skizze + Zeichnung + Projektangaben + Stücklistentext: Bild mit sich selbst gleich; je ein Feld geändert (Name, unterdrückt, Maß des Quaders, Skizzenpunkt verschoben, Marker, `next_id`, Ansicht verschoben, Projektangabe, Stücklistentext, anderes Netz-`Arc` mit gleichem Inhalt) → ungleich; `same_model` ungleich nur bei den Modellfeldern);
  (b) `restore_full_roundtrip` (Bild A, dann Zylinder anlegen, Quader löschen: `restore(A) == Ok(Full)`, `snapshot().same_content(&A)`, `check_invariants` ok, Körper wie vorher (Zahl und Hüllboxen ±1e-9));
  (c) `counters_keep_rising` (`edit_count` und `model_edit_count` nach `restore` (voll) jeweils größer als davor; zweimal hintereinander auf zwei verschiedene Bilder → drei paarweise verschiedene Wertepaare; nach `from_snapshot` wären sie 0 – Gegenprobe);
  (d) `restore_annotations_only` (nur Ansicht verschoben und Maß angelegt: `Ok(Annotations)`, `model_edit_count` unverändert, `edit_count` + 1, Körper-Ergebnisse unverändert (dieselben `result`-Versionen), Zeichnung gleich dem Bild);
  (e) `restore_failure_leaves_document` (Bild mit doppelter Feature-Kennung, Bild mit ungültigen Projektangaben (Annotations-Weg), Bild mit Marker hinter dem Ende → `Err`, `snapshot()` danach inhaltsgleich mit vorher, Zähler unverändert);
  (f) `known_meshes_not_revalidated` (neuer `#[doc(hidden)] pub fn mesh_validations() -> u64` nach Vorbild `face_probe_calls` in `refs.rs` (Zähler je Thread, zählt jedes `ExchangeMesh::validate` in `from_snapshot`/`restore`): Netz-Körper anlegen, Bild A, Quader anlegen, `restore(A)` → Zähler + 0 (derselbe `Arc`); Bild mit einem *neuen* `Arc` gleichen Inhalts → Zähler + 1; `from_snapshot(A)` → + 1 wie bisher);
  (g) `sketch_equality_by_revision` (Klon gleich; nach `add_point` ungleich; zwei getrennt gebaute, inhaltsgleiche Skizzen ungleich (R5)).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-feature --test restore -j 2`
- [ ] **Step 3: Implement** nach R1, R2, R5 und Interfaces.
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-feature -j 2`, `cargo test -p planora-project -j 2` (unverändert grün), clippy, fmt; `git diff --stat 2e283e1 -- crates/planora-project` leer.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Zähler nach `restore` auf 0; nur `edits` erhöht; Annotations-Weg erhöht `edits`; `same_model` ignoriert Marker; Netze nach Inhalt statt Zeiger verglichen; Projektangaben im Annotations-Weg ungeprüft; Fehler lässt halbes Dokument zurück; `SketchFeature::eq` ohne `copies`; `same_content` ohne Stücklistentexte; Netz trotz gleichem `Arc` geprüft.
- [ ] **Step 6: Commit** — `feat: Dokument aus einem Bild zurückladen, Zähler steigen weiter`.

### Task 4: Dokument im Zustand ersetzen (`AppState::restore_document`)

**Files:**
- Modify: `crates/planora-app/src/state.rs` (`restore_document`, `refresh_following` mit Schalter „nicht einpassen“, Aufräumen), `crates/planora-app/src/drawing_state.rs` (`DrawingUi::forget_missing(&Document)`)
- Test: `crates/planora-app/tests/undo_state.rs` (neu)

**Interfaces:**
- Consumes: `Document::restore`, `RestoreKind` (3).
- Produces:
```rust
pub const UNDO_FAILED: &str = "Rückgängig ist nicht möglich.";
pub const REDO_FAILED: &str = "Wiederholen ist nicht möglich.";
impl AppState {
    /// Ersetzt nur `document` (R1, R2, R9); Kamera, `dark`, `instance`, `fit_pending` bleiben; kein `frame_all`.
    pub fn restore_document(&mut self, s: planora_feature::Snapshot) -> Result<RestoreKind, String>;
}
impl DrawingUi { pub fn forget_missing(&mut self, doc: &Document) } // area → Model, wenn die Zeichnung fehlt; selected_* leeren, wenn ihr Ziel fehlt
```
Bei Erfolg: `rebuild_name_counters`, Körper neu ohne Einpassen (Weg `Full`; bei `Annotations` bleiben `bodies` und `scene` unverändert), Auswahl/Hover/`selected_body`/`selected_feature` wie `refresh` abbilden bzw. leeren, wenn das Ziel fehlt; geleert bzw. auf `Default`: `pending_delete`, `pending_delete_drawing`, `pending_delete_dimension`, `rename_draft`, `rename_drawing_draft`, `edit_request`, `sketch_request`, `marked_edges`, `edge_picking = false`, `drag_move`, `dimension_dialog`, `sheet_dialog`, `dimension_tool`, `balloon_tool`, `section_tool`; `message = None`. Bei Fehler: nichts davon, `Err(e)` mit dem Grund (die Meldung setzt der Aufrufer, Aufgabe 6).

- [ ] **Step 1: Failing tests:**
  (a) `camera_and_theme_stay` (Quader, Kamera gedreht, `dark = false`; Bild vorher; zweiter Körper; `restore_document(bild)`: Kamera-Felder bitgleich, `dark == false`, `instance` gleich, `bodies.len() == 1`);
  (b) `names_continue_from_restored_state` („Quader 1“, „Quader 2“, „Quader 3“, Bild nach 2 zurück → nächster Quader heißt „Quader 3“);
  (c) `delete_with_dependents_comes_back` (Quader, Verschieben, Verrunden, Skizze auf Fläche; Bild; `delete_feature_with_dependents(quader)`; zurück → gleiche `FeatureId`s und Namen, alle `Ok`, Körper sichtbar; auch wenn die Skizze beim Löschen offen war: danach `sketch_session == None`);
  (d) `transient_ui_is_cleared` (jedes der oben genannten Felder gesetzt → danach leer/`Default`, `selection` auf einen nicht mehr vorhandenen Körper → `None`, auf einen noch vorhandenen → bleibt, `message == None`);
  (e) `failure_changes_nothing` (Bild mit Marker > Featurezahl: `Err`, `document.snapshot()` gleich vorher, `bodies`, Auswahl, Dialoge unverändert);
  (f) `drawing_step_keeps_view_results` (Zeichnung mit Vorne/Oben, Ergebnisse fertig (`block`); Ansicht verschieben; zurück → `Ok(Annotations)`, `model_edit_count` gleich, kein neuer Auftrag im `DrawingUi`-Zwischenspeicher, Lage im Blatt = alte Lage);
  (g) `model_step_refreshes_drawing` (Zeichnung offen; Quader 2 anlegen; zurück → `Ok(Full)`; nach `block` zeigt die Vorne-Ansicht nur Kanten von Quader 1; zweimal hintereinander zurück auf zwei verschiedene Bilder → jedes Mal das passende Ergebnis, nie das vorige; gelöschte Zeichnung zurückgeholt bzw. angelegte zurückgenommen → `area == WorkArea::Model`, wenn die geöffnete fehlt).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test undo_state -j 2`
- [ ] **Step 3: Implement** nach Interfaces.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test state --test feature_state --test drawing_state --test section_tool -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: `frame_all` nach dem Zurückladen; `AppState::from_snapshot` benutzt; Namenszähler nicht neu; Werkzeuge nicht zurückgesetzt; `marked_edges` bleiben; `area` bleibt auf gelöschter Zeichnung; Auswahl nicht abgebildet; bei `Annotations` Körper neu tesselliert (Test über `Arc::ptr_eq(&scene)`); Fehler leert trotzdem Dialoge; `message` bleibt.
- [ ] **Step 6: Commit** — `feat: Dokument ersetzen ohne Kamera und Ansicht zu verlieren`.

### Task 5: Namen der Schritte (`undo_labels.rs`)

**Files:**
- Create: `crates/planora-app/src/undo_labels.rs`
- Modify: `crates/planora-app/src/lib.rs` (`pub mod undo_labels;`)
- Test: `crates/planora-app/tests/undo_labels.rs` (neu; jede Aktion über ihren echten Einstieg im `AppState`/`Document`, Bild davor/danach)

**Interfaces:**
- Consumes: `Snapshot` (3), `SketchFeature`.
- Produces:
```rust
pub fn describe_change(before: &Snapshot, after: &Snapshot) -> &'static str;
pub fn describe_sketch_change(before: &SketchFeature, after: &SketchFeature) -> &'static str;
pub const FALLBACK: &str = "Ändern";
```
Regeln `describe_change` in dieser Reihenfolge (erste passende gilt):
1. Genau ein Feature neu: Quader/Zylinder/Kugel/Kegel „… anlegen“ („Quader anlegen“ usw.), Skizze „Skizze anlegen“, Extrusion „Extrudieren“, Drehen „Drehen“, Boolean „Vereinigen“/„Abziehen“/„Schneiden“, Verschieben „Verschieben“, Verrunden „Verrunden“ bzw. bei Fase „Fase“, Netz-Körper „Netz importieren“. Mehrere neu: „Ändern“.
2. Features fehlen (Kennungen weg): „Feature löschen“.
3. Gleiche Kennungen: Name anders „Umbenennen“; `suppressed` wird wahr „Unterdrücken“, falsch „Einblenden“; Marker anders „Zeitleistenmarke setzen“; Art anders: Verschieben oder Netz-Lage „Verschieben“, Skizze „Skizze bearbeiten“, sonst „Feature ändern“.
4. Zeichnungen: neu „Zeichnung anlegen“ (nur Stücklistenblätter neu: „Stücklistenblatt anlegen“); weg „Zeichnung löschen“; Name „Zeichnung umbenennen“; in einer Zeichnung: Ansicht neu mit `section` „Schnitt anlegen“, sonst „Ansicht hinzufügen“; Ansichten weg „Ansicht löschen“; `placement` anders „Ansicht verschieben“; `scale` „Maßstab ändern“; `section` anders „Schnitt ändern“; übrige Ansichtsfelder „Ansicht ändern“; Maß neu „Maß anlegen“, weg „Maß löschen“, anders „Maß ändern“; Positionsnummer neu „Positionsnummer anlegen“, weg „Positionsnummer löschen“, anders „Positionsnummer verschieben“; `sheet`, `projection`, `info` „Blattangaben ändern“.
5. Projektangaben anders „Blattangaben ändern“. 6. Stücklistentexte anders „Stücklistentext ändern“. 7. Sonst `FALLBACK`.

Regeln `describe_sketch_change`: `copies` mehr „Kante übernehmen“; Kurven weniger oder Punkte weniger oder Abhängigkeiten weniger „Löschen“; genau eine Kurve neu „Linie zeichnen“/„Kreis zeichnen“/„Bogen zeichnen“/„Ellipse zeichnen“/„Spline zeichnen“; genau vier Linien neu „Rechteck zeichnen“; mehrere Kurven neu „Zeichnen“; nur Punkte neu „Punkt setzen“; nur Abhängigkeiten neu: Maß (Abstand, waagerechter/senkrechter Abstand, Radius, Durchmesser, Winkel) „Maß setzen“, sonst „Abhängigkeit setzen“; Hilfslinie umgeschaltet „Hilfslinie umschalten“; sonst „Skizze ändern“.

- [ ] **Step 1: Failing tests:** (a) `each_entry_point_has_its_label` (Tabelle mit je einem Fall über den echten Weg: `create_box/cylinder/sphere/cone`, `add_sketch`, `extrude_sketch`, `revolve_sketch`, `add_boolean` ×3, `move_body` (neu und erweitert), `add_blend` (Rundung, Fase), `import_mesh`, `set_mesh_transform`, `delete_feature`, `delete_feature_with_dependents`, `rename_feature`, `Document::set_suppressed` hin und zurück, `set_marker`, `FeatureDialog`-Weg `Document::update` (Quadergröße), Skizze bearbeitet über `close_sketch`, `add_drawing_stamped`+`add_view` (ein Bild), `add_parts_sheets`, `delete_drawing`, `rename_drawing`, `update_drawing` mit `add_view`, `remove_view`, Lage, Maßstab, `add_section`, Schnittlage, verdeckte Kanten, `add_dimension`, Maß löschen, Maß verschoben, `add_balloon`, Positionsnummer verschieben/löschen, `set_sheet_info`, `set_project_info`, `set_parts_override` – jeweils genau der Text aus den Regeln); (b) `priority_and_fallback` (Blattangaben + Projektangaben + Stücklistentext in einem Bild → „Blattangaben ändern“; zwei neue Features → „Ändern“; gleiche Bilder → „Ändern“); (c) `sketch_labels` (Linie, Kreis, Bogen, Ellipse, Spline, Rechteck, Punkt, Abstand, waagerecht (Abhängigkeit), Radius (Maß), Löschen, Hilfslinie, Punkt gezogen „Skizze ändern“, Kante übernehmen); (d) `labels_use_allowed_glyphs` (jeder Text nur aus Zeichen, die `tests/fonts.rs` erlaubt; kein Text endet mit „…“).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test undo_labels -j 2`
- [ ] **Step 3: Implement** nach den Regeln.
- [ ] **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Reihenfolge Löschen/Anlegen vertauscht; Fase als „Verrunden“; Unterdrücken/Einblenden vertauscht; Schnitt als „Ansicht hinzufügen“; Lage und Maßstab vertauscht; Stücklistenblatt als „Zeichnung anlegen“; Rechteck als „Zeichnen“; Maß als Abhängigkeit; `copies` nicht geprüft; Projektangaben vor Zeichnungen geprüft.
- [ ] **Step 6: Commit** — `feat: Namen der rücknehmbaren Aktionen`.

### Task 6: Verlauf im Tab, Erkennen je Bild, Gespeichert-Punkt

**Files:**
- Modify: `crates/planora-app/src/workspace.rs` (`DocTab`: Felder `history`, `tracker`; `saved_edits` entfällt; `observe_history`, `undo`, `redo`, `undo_label`, `redo_label`, `undo_blocked`, `redo_blocked`, `dialog_open`, `close_dialogs`, `replace_state`; `is_dirty`, `mark_clean`, `apply_autosave`, `restore_into` nach R7; `Workspace::observe_history`), `crates/planora-app/src/app.rs` (`ui`: am Bildende vor `remember_keyboard_focus` `self.ws.observe_history(ctx.input(|i| i.pointer.any_down()) || viewport::typing(&ctx))`)
- Test: `crates/planora-app/tests/undo_flow.rs` (neu; `Workspace` direkt und `PlanoraApp::for_tests` mit echten Bildern)

**Interfaces:**
- Consumes: 1, 3, 4, 5.
- Produces:
```rust
impl DocTab {
    pub fn observe_history(&mut self, gesture: bool);          // R3/R4; nur Dokument (Skizze: Aufgabe 7)
    pub fn undo(&mut self) -> Result<(), String>;              // erst observe_history(false); Fehler: discard_undo, state.message = UNDO_FAILED
    pub fn redo(&mut self) -> Result<(), String>;
    pub fn undo_label(&self) -> Option<&'static str>;
    pub fn redo_label(&self) -> Option<&'static str>;
    pub fn undo_blocked(&self) -> Option<&'static str>;        // alte Version, Export schreibt, nichts da (Texte aus Global Constraints)
    pub fn redo_blocked(&self) -> Option<&'static str>;
    pub fn dialog_open(&self) -> bool;                         // alle Option-Dialoge des Tabs + state.{dimension_dialog, sheet_dialog, pending_delete*, rename_*}
    pub fn close_dialogs(&mut self);                           // alle außer einem Export, der gerade schreibt
    pub fn replace_state(&mut self, state: AppState);          // neuer Verlauf, Tracker
}
impl Workspace { pub fn observe_history(&mut self, gesture: bool) } // alle Tabs
```
Nach gelungenem `undo`/`redo`: `tracker.reset(edit_count)`, `close_dialogs()`. `DocTab::new` beginnt mit `History::new(state.snapshot(), snapshot_bytes(..))`. `mark_clean` ruft zuerst `observe_history(false)`, dann `history.mark_saved()`, `autosaved_edits = edit_count`. `apply_autosave`: `replace_state` + `history.forget_saved()`. `restore_into`: `replace_state` + `mark_clean`.

- [ ] **Step 1: Failing tests:**
  (a) `each_action_is_one_step` (Quader anlegen, verschieben, umbenennen, je `observe_history(false)`: `undo_label` nacheinander „Umbenennen“, „Verschieben“, „Quader anlegen“; dreimal `undo` → Bild gleich dem leeren Anfang (`same_content` und `encode_version`-Text byte-gleich); dreimal `redo` → gleich dem Endbild byte-gleich);
  (b) `no_step_without_change` (`observe_history` ohne Änderung, Kamera gedreht, Auswahl geändert, Export-Fenster geöffnet/geschlossen → `undo_len == 0`; abgelehntes Verschieben (Körper läge außerhalb ±MAX) → kein Schritt; `drawing_dialog` mit fehlschlagender Ansicht (angelegt und wieder gelöscht) → kein Schritt);
  (c) `delete_with_dependents_is_one_step` (Quader + 3 Folgen; Löschen mit Folgen; ein `undo` → alle zurück, `undo_label` danach „Verrunden“ o. Ä. des vorigen Schritts);
  (d) `drag_is_one_step` (30 Aufrufe `move_body_dragging` mit `observe_history(true)` dazwischen, `end_drag`, `observe_history(false)`: genau ein neuer Schritt „Verschieben“; `undo` → Lage vor dem Ziehen exakt; `redo` → Endlage exakt);
  (e) `drag_in_real_frames` (App mit Quader, Eigenschaftenfeld; Zeiger auf das Positionsfeld X, Taste gedrückt über 20 Bilder ziehen, loslassen: `undo_len` + 1; während gedrückt `undo_len` unverändert; Zahl in das Feld tippen und Enter → ein Schritt, nicht einer je Taste);
  (f) `dirty_dot_follows_history` (neuer Tab sauber; Quader → schmutzig auch **vor** `observe_history`; speichern (Testbibliothek in `temp_dir`) → sauber; `undo` → schmutzig; `redo` → sauber; `undo`, neuer Kugel → schmutzig, `redo_len == 0`, kein Weg zurück zu sauber ohne Speichern; Verlauf nach Speichern erhalten (`undo_len` gleich); offene Skizze → schmutzig);
  (g) `dirty_dot_in_tab_strip` (echte Bilder: Punkt am Tab sichtbar/unsichtbar wie in (f), über den vorhandenen Test-Helfer aus `tests/tabs_ui.rs`);
  (h) `tabs_have_own_history` (zwei Tabs, je eine Aktion; `undo` im aktiven Tab ändert den anderen nicht);
  (i) `restore_failure_drops_one_step` (drei Schritte; das mittlere Bild im Verlauf künstlich ungültig gemacht über `#[doc(hidden)] pub fn history_mut()`; `undo` (gültig) ok; nächstes `undo` → `Err`, `message == Some(UNDO_FAILED)`, Dokument unverändert, `undo_len` um genau 1 kleiner, nächstes `undo` → erstes Bild);
  (j) `undo_closes_dialogs_and_blocks_while_writing` (Fenster „Quader erzeugen“, „Ändern“, Rückfrage „Löschen“, „Blattangaben“ offen → `dialog_open()`; `undo` schließt alle; Export schreibt → `undo_blocked() == Some("Während des Exports ist das nicht möglich.")`, `undo` ändert nichts; alte Version → gesperrt mit „In einer alten Version lässt sich nichts ändern.“);
  (k) `autosave_and_restore_reset_history` (`apply_autosave` → `undo_len == 0`, schmutzig; `restore_version` → `undo_len == 0`, sauber).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test undo_flow -j 2`
- [ ] **Step 3: Implement** nach Interfaces und R3, R4, R7, R8. `encode_version` nur im Test.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test workspace --test tabs_ui --test close_handling --test versions_ui --test shell_app --test projects_ui -j 2` (unverändert grün), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Beobachtung nur bei steigendem Zähler; Geste nicht abgewartet; Typing nicht als Geste; `same_content`-Prüfung entfernt; `tracker.reset` nach `undo` vergessen (Rückgängig wird selbst ein Schritt); `mark_clean` ohne vorheriges `observe`; `is_dirty` ohne Tracker-Prüfung; Autosicherung lässt `saved` stehen; `close_dialogs` schließt schreibenden Export; Verlauf beim Wiederherstellen nicht neu; nur aktiver Tab beobachtet.
- [ ] **Step 6: Commit** — `feat: Rückgängig je Tab mit Schritten, Gesten und Gespeichert-Punkt`.

### Task 7: Skizzenverlauf und „Skizze beenden“

**Files:**
- Modify: `crates/planora-app/src/state.rs` (Feld `sketch_history: Option<(History<SketchFeature>, Tracker)>`, angelegt in `open_sketch`, entfernt in `commit_draft`-Erfolg und `discard_sketch`; `observe_sketch`, `undo_sketch`, `redo_sketch`, `sketch_undo_label`, `sketch_redo_label`), `crates/planora-app/src/sketch_state.rs` (`SketchSession::after_history_jump(&mut self, sk: &Sketch)`), `crates/planora-app/src/workspace.rs` (`DocTab::{observe_history, undo, redo, undo_label, redo_label, undo_blocked, redo_blocked}` leiten bei offener Skizze an die Skizze weiter)
- Test: `crates/planora-app/tests/undo_sketch.rs` (neu)

**Interfaces:**
- Consumes: 1, 5, 6.
- Produces:
```rust
impl AppState {
    pub fn observe_sketch(&mut self, gesture: bool);   // R10, Schlüssel sketch.revision()
    pub fn undo_sketch(&mut self) -> bool;             // false: nichts da
    pub fn redo_sketch(&mut self) -> bool;
    pub fn sketch_undo_label(&self) -> Option<&'static str>;
    pub fn sketch_redo_label(&self) -> Option<&'static str>;
}
impl SketchSession { pub fn after_history_jump(&mut self, sk: &Sketch) } // R10: pending, pending_pos, selection, dragging, message leeren; last_report = Some(sk.clone().solve())
```

- [ ] **Step 1: Failing tests:**
  (a) `undo_in_sketch_touches_only_the_sketch` (Skizze öffnen, zwei Linien zeichnen (über `SketchSession::click`), `observe` je Bild; `undo_label == "Linie zeichnen"`; `undo` → eine Linie; Dokument-`edit_count` und Dokumentverlauf unverändert; `redo` → zwei Linien);
  (b) `first_click_belongs_to_the_line` (Linienwerkzeug: erster Klick, `observe(false)` → kein Schritt, `undo_label == None`; zweiter Klick → genau ein Schritt; Linienzug mit 3 Klicks → 2 Schritte; Rechteck → 1 Schritt „Rechteck zeichnen“);
  (c) `point_drag_is_one_step` (`begin_drag`, 20× `drag_to` mit `observe(true)`, `end_drag`, `observe(false)` → ein Schritt „Skizze ändern“; `undo` → Punkt an alter Stelle exakt, `dragging == None`);
  (d) `jump_clears_session_bits` (nach `undo`: `pending`, `pending_pos`, `selection` leer, `message == None`, `last_report` gesetzt; `ProfileCache` liefert die Profile des zurückgeholten Stands (Revision gleich));
  (e) `end_sketch_is_one_document_step` (neue Skizze anlegen und öffnen → Dokumentschritt „Skizze anlegen“; drei Elemente; `close_sketch` → genau ein Dokumentschritt „Skizze bearbeiten“; ein `undo` → Skizze im Dokument leer, wie nach dem Anlegen; Skizze ohne Änderung öffnen/schließen → kein Schritt; nach `undo` aller Skizzenschritte und Schließen → kein Dokumentschritt);
  (f) `sketch_history_ends_with_session` (Skizzenschritte, schließen, wieder öffnen → `sketch_undo_label == None`; Löschen der offenen Skizze aus dem Baum → Skizzenverlauf weg, Dokumentschritt „Feature löschen“);
  (g) `labels_route_by_mode` (`DocTab::undo_label` bei offener Skizze = Skizzenname des Schritts, ohne Skizze = Dokumentschritt; `undo` bei offener Skizze ohne Skizzenschritt ändert das Dokument nicht).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test undo_sketch -j 2`
- [ ] **Step 3: Implement** nach R10 und Interfaces; Zurückladen ersetzt `sketch_draft` durch den Klon des Eintrags (Stand bleibt gleich).
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test sketch_state --test sketch_ui --test sketch_view --test sketch_features --test sketch_dialogs --test sketch_fields -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Aufschub bei `pending` fehlt; Aufschub auch bei neuen Kurven; `dragging` nicht geleert; `last_report` nicht erneuert; Skizzenverlauf nach Schließen behalten; Dokumentverlauf statt Skizzenverlauf benutzt; `copies` nicht im Eintrag; Schlüssel `curves().count()` statt Revision (Ziehen unerkannt); `commit_draft` schreibt unveränderten Entwurf; Skizzenverlauf bei `open_sketch`-Wechsel nicht neu.
- [ ] **Step 6: Commit** — `feat: Rückgängig in der offenen Skizze`.

### Task 8: Tastenkürzel mit Sperren

**Files:**
- Modify: `crates/planora-app/src/app.rs` (`shortcuts`: Strg+Z → `undo`, Strg+Umschalt+Z und Strg+Y → `redo`; zusätzlich gesperrt bei `ws.about_open` und `active_tab().dialog_open()`; Kommentar der Funktion nennt die neuen Kürzel)
- Test: `crates/planora-app/tests/undo_keys.rs` (neu; echte Bilder mit `egui::Event::Key { key, modifiers: Modifiers::COMMAND [| SHIFT], pressed: true, .. }`)

**Interfaces:**
- Consumes: 6, 7.
- Produces: keine neuen öffentlichen Namen.

Strg+Umschalt+Z vor Strg+Z abfragen (`consume_shortcut` mit `COMMAND` passt sonst auch bei gedrückter Umschalttaste; egui-Verhalten im Test prüfen).

- [ ] **Step 1: Failing tests:** (a) `keys_undo_and_redo` (Quader, Kugel; Strg+Z → Kugel weg; Strg+Umschalt+Z → wieder da; Strg+Z, Strg+Y → wieder da; jeweils genau ein Schritt je Druck, auch über zwei Durchgänge eines Bildes); (b) `typing_keeps_ctrl_z_for_the_field` (Fenster „Umbenennen“ bzw. Positionsfeld fokussiert, Text getippt, Strg+Z → Dokument unverändert, Feldinhalt per egui-Rückgängig zurück); (c) `dialog_blocks_keys` (je Fenster: „Quader erzeugen“, „Ändern“, „Verrunden“, „Neue Zeichnung“, „Maß bearbeiten“, „Blattangaben“, Rückfrage „Löschen mit Folgen“, „Über“, `ws.prompt` → Strg+Z ändert nichts; nach Schließen wirkt es); (d) `keys_in_sketch_mode` (offene Skizze, Strg+Z nimmt Linie zurück, Dokument unverändert); (e) `other_shortcuts_unchanged` (Strg+N/O/S/W, Strg+Umschalt+E wie vorher, `tests/shell_app.rs` grün).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test undo_keys -j 2`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test shell_app --test dialogs --test feature_dialog -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Typing-Sperre entfernt; Dialogsperre entfernt; `about_open` vergessen; Umschalt+Z als Rückgängig; Strg+Y fehlt; Kürzel in jedem Durchgang (doppelt); `consume` vergessen (Feld bekommt Kürzel zusätzlich); Rückfrage nicht gesperrt; im Skizzenmodus Dokument statt Skizze; ein Dialog fehlt in `dialog_open`.
- [ ] **Step 6: Commit** — `feat: Strg+Z und Strg+Y mit Sperren`.

### Task 9: Menü „Bearbeiten“ und Symbole

**Files:**
- Modify: `crates/planora-app/src/icons.rs` (`Icon::Undo` = Phosphor `arrow-counter-clockwise` `'\u{e038}'`, `Icon::Redo` = `arrow-clockwise` `'\u{e036}'`; Doku-Kommentar, `ALL`, `TABLE`), `crates/planora-app/src/app.rs` (`edit_menu`: oben zwei Einträge über `shortcut_button`, dann Trennlinie, dann das Bisherige)
- Test: `crates/planora-app/tests/undo_menu.rs` (neu); `tests/icons.rs` und `tests/fonts.rs` laufen unverändert mit

**Interfaces:**
- Consumes: 6, 7.
- Produces: `Icon::Undo`, `Icon::Redo`; `pub fn undo_menu_text(label: Option<&str>) -> String` und `pub fn redo_menu_text(label: Option<&str>) -> String` in `app.rs` („Rückgängig: Quader anlegen“ bzw. „Rückgängig“), von Aufgabe 10 für die Hilfetexte benutzt.

- [ ] **Step 1: Failing tests:** (a) `menu_shows_action_names` (Quader → Menü „Bearbeiten“ zeigt „Rückgängig: Quader anlegen“ mit „Strg+Z“ und gesperrt „Wiederholen“ mit „Strg+Y“; Klick → Quader weg, Menü zeigt „Wiederholen: Quader anlegen“; Klick → wieder da); (b) `disabled_entries_explain` (leerer Verlauf: beide gesperrt, Hilfetexte aus Global Constraints; Export schreibt → Sperrtext); (c) `menu_in_sketch_mode` („Rückgängig: Linie zeichnen“); (d) `menu_with_open_dialog_closes_it` (Fenster „Quader erzeugen“ offen, Menüklick → Schritt zurück, Fenster zu); (e) `icons_registered` (`glyph(Icon::Undo) == '\u{e038}'`, `phosphor_name == "arrow-counter-clockwise"`, Redo entsprechend; `tests/icons.rs` grün); (f) `menu_fits_and_themes` (Menü bei 800 × 600 nicht abgeschnitten, längster Eintrag „Rückgängig: Zeitleistenmarke setzen“ ganz sichtbar; Hell und Dunkel, `no_hard_coded_colors` grün).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test undo_menu -j 2`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test icons --test fonts --test no_hard_coded_colors --test shell_commands -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Codepunkt vertauscht; Symbol fehlt in `ALL`; Name ohne Aktion; Doppelpunkt ohne Leerzeichen; Wiederholen zeigt Rückgängig-Namen; gesperrt trotz Schritt; Hilfetext vertauscht; Kürzel „Strg+Umschalt+Z“ statt „Strg+Y“ angezeigt; Menü schließt nicht; Skizzenmodus zeigt Dokumentschritt.
- [ ] **Step 6: Commit** — `feat: Rückgängig und Wiederholen im Menü Bearbeiten`.

### Task 10: Zwei Knöpfe im Befehlsfeld

**Files:**
- Modify: `crates/planora-app/src/shell_panels.rs` (`bottom_field`: vor den Chips zwei `icons::source_icon_button` mit `IconSource::Font(Icon::Undo/Redo)`, Beschriftung in `bottom_buttons` „Rückgängig“/„Wiederholen“, Hilfetext „{undo_menu_text} (Strg+Z)“ bzw. „… (Strg+Y)“, gesperrt mit `explain` über `bottom_lock.or(undo_blocked)`; danach `ui.separator()`; `Command::{Undo, Redo}`; `chips_end` misst weiterhin bis hinter die Chips)
- Modify: `crates/planora-app/tests/shell_commands.rs` (siehe „Bestehende Tests, die sich ändern“)
- Test: `crates/planora-app/tests/undo_buttons.rs` (neu)

**Interfaces:**
- Consumes: 6, 7, 9 (`undo_menu_text`, `redo_menu_text`, `Icon::Undo/Redo`).
- Produces: keine neuen öffentlichen Namen.

- [ ] **Step 1: Failing tests:** (a) `buttons_left_of_the_chips` (`bottom_buttons`: „Rückgängig“, „Wiederholen“ sind die ersten zwei, rechte Kante beider links von „Vereinigen“); (b) `buttons_do_what_the_menu_does` (Klick „Rückgängig“ = Menüeintrag, gleiches Bild danach; „Wiederholen“ ebenso); (c) `tooltips_name_the_action` („Rückgängig: Quader anlegen (Strg+Z)“; gesperrt: Name und Grund wie andere gesperrte Knöpfe); (d) `fits_at_1100_and_scrolls_at_800` (bei 1100 × 700 alle Knöpfe einschließlich der neuen ganz im Befehlsfeld; bei 800 × 600 „Rückgängig“ sofort sichtbar, „Fase …“ nach Rollen erreichbar); (e) `locked_in_old_versions_and_prompts` (alte Version, offene Rückfrage → gesperrt); (f) `themes` (Hell/Dunkel, Symbole in Theme-Farben, `no_hard_coded_colors` grün).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test undo_buttons -j 2`
- [ ] **Step 3: Implement.** Passt bei 1100 × 700 nicht mehr alles (bestehender Test `every_button_is_reachable_at_narrow_widths` oder `status_stays_readable_with_the_real_bottom_field`), **stoppen und dem Controller melden**; Tests nicht aufweichen.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test shell_commands --test shell_layout --test shell_panels --test viewport_shell --test graphit_style -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Knöpfe rechts der Chips; Rückgängig/Wiederholen vertauscht; Sperre ohne `bottom_lock`; Hilfetext ohne Kürzel; `seen` ohne die neuen Knöpfe; Knopf gesperrt trotz Schritt; Klick erreicht die 3D-Ansicht; Trennlinie fehlt (Abstandstest); `chips_end` vor den neuen Knöpfen gemessen; feste Farbe im Symbol.
- [ ] **Step 6: Commit** — `feat: Knöpfe Rückgängig und Wiederholen im Befehlsfeld`.

### Task 11: Dokumentation und Gesamtprüfung

**Files:**
- Modify: `README.md` (neuer Abschnitt „Rückgängig und Wiederholen“ in Laiensprache: Tasten, Menü, Knöpfe, je Tab, 100 Schritte, was nicht rücknehmbar ist, Skizze, Punkt „ungespeichert“; die Sätze „es gibt noch kein Rückgängig“ (Zeile 16) und „Kein Rückgängig.“ (Zeilen 35, 350) entfernen), `docs/superpowers/offene-punkte.md` (Zeilen 48, 69, 118, 159, 206, 561, 586 berichtigen bzw. als erledigt kennzeichnen; neuer Abschnitt „Teilprojekt Rückgängig und Wiederholen“: Geliefertes, R1–R10, Messergebnis aus Aufgabe 2, Grenzen (Verlauf nicht gespeichert; Namen aus dem Unterschied, mitunter allgemein „Ändern“; nach fehlgeschlagenem Zurückladen fasst der nächste Schritt zwei zusammen; Skizzen mit gleichem Inhalt, aber neuem Stand gelten als geändert), Handliste als Abhakliste), `docs/superpowers/specs/2026-10-19-rueckgaengig-design.md` (neuer §10 „Festlegungen der Umsetzung“: R1–R10 kurz), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)

- [ ] **Step 1: Doku** schreiben (Stil nach Gedächtnis `project-page-writing-style`: einfache Sprache, wenig Technik). Handliste: Quader anlegen, Strg+Z, Strg+Y, Knopf und Menü mit Aktionsnamen; Position ziehen → ein Schritt; Löschen mit Folgen zurück; Zeichnung: Ansicht verschieben, Maß anlegen, Schnitt anlegen, je zurück, Blatt stimmt sofort; Skizze: Linien zeichnen, Strg+Z in der Skizze, „Skizze beenden“ zurück; speichern, Rückgängig → Punkt, Wiederholen → kein Punkt; Strg+Z im Textfeld und bei offenem Fenster; zwei Tabs getrennt; Hell/Dunkel; 1100 × 700 und 800 × 600.
- [ ] **Step 2: Gates** mit Ausgabe: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2`; `cargo deny check`; `git diff --stat 2e283e1 -- crates/planora-kernel crates/planora-mesh crates/planora-step crates/planora-exchange crates/planora-project crates/planora-sketch crates/planora-drawing crates/planora-render crates/planora-math` leer; `git diff 2e283e1 -- '**/Cargo.toml' Cargo.lock deny.toml` leer; `git status` sauber.
- [ ] **Step 3: Handproben:** entfallen (keine Ausgabedateien).
- [ ] **Step 4: Mutationsbeweise:** entfällt für reine Doku; dafür die Summenliste aller Mutationen der Aufgaben 1–10 im Bericht (mindestens 100 Einträge).
- [ ] **Step 5: Commit** — `docs: Rückgängig und Wiederholen, Festlegungen, Handliste`.

### Task 12 (Controller): unabhängige Prüfung, echtes Fenster, Haltepunkt

- [ ] **Step 1 (nur mit Freigabe des Nutzers):** Release bauen (`-j 2`, Vordergrund), App starten, mit `tools/gui-drive.ps1` (Gedächtnis `planora-gui-testing`, kein computer-use) den Kernweg fahren und Bildschirmfotos ansehen: Quader anlegen → Strg+Z → Strg+Y; Position ziehen → ein Strg+Z stellt den Stand vor dem Ziehen her; Skizze mit zwei Linien → Strg+Z in der Skizze, „Skizze beenden“, Strg+Z nimmt die ganze Skizze zurück; Zeichnung: Ansicht verschieben → Strg+Z, Blatt zeigt sofort die alte Lage; speichern → Punkt weg, Strg+Z → Punkt, Strg+Y → weg; Strg+Z im Umbenennen-Feld (nur der Text) und bei offenem „Quader erzeugen“ (nichts); Menü- und Knopftexte, 1100 × 700 und 800 × 600, Hell/Dunkel. App beenden, kein Prozess bleibt. Befunde ins Ledger, Korrektur an die zuständige Aufgabe.
- [ ] **Step 2: Haltepunkt:** Nutzer prüft Handliste; Gesamtreview des Zweigs, eine Korrekturrunde, Merge-Frage.

---

## Bestehende Tests, die sich ändern

- **`crates/planora-app/tests/shell_commands.rs`:** nur `all_buttons_sit_inside_the_bottom_field_at_normal_sizes` (Zeile 659) erwartet `ALL_LABELS.len() + 2` Knöpfe. `ALL_LABELS` (Zeile 284) bleibt bei den 11 Dialogknöpfen (die neuen Knöpfe öffnen keinen Dialog; ihre Fälle stehen in `undo_buttons.rs`). `every_button_is_reachable_at_narrow_widths` und `status_stays_readable_with_the_real_bottom_field` bleiben **unverändert** (Stopp-Regel in Aufgabe 10).
- **`DocTab::saved_edits`** (privat) entfällt; kein Test greift darauf zu. Tests, die nach einer Änderung ohne Bild `is_dirty()` prüfen (`tests/workspace.rs`, `tests/close_handling.rs`), bleiben grün über die Tracker-Prüfung (R7).
- **Unverändert (muss so bleiben):** alle Dateiformat-Tests in `crates/planora-project/tests`, alle Golden-Dateien, alle bisherigen Meldungen.

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung:** §1 Zweck → 4, 6. §2 Rücknehmbar: Körper/Netz → 5 (a), 6 (a); Verschieben, Boolesch, Verrunden, Extrudieren, Drehen → 5 (a); Löschen mit Folgen, Umbenennen, Unterdrücken, Marke, Ändern → 4 (c), 5 (a), 6 (c); Zeichnungen samt Maßen, Positionsnummern, Stücklistentexten, Blattangaben, Schnitten → 3 (d), 4 (f, g), 5 (a); Skizze je Element und „Skizze beenden“ → 7. Nicht rücknehmbar (Kamera, Auswahl, Export, Speichern) → 6 (b, f). Nicht-Ziele unberührt (Verlauf nicht gespeichert, je Tab → 6 (h)). Anti-Ziel → 3 (e), 4 (e), 6 (i). §3 Tasten → 8; Menü mit Namen → 9; Knöpfe links der Booleschen, 1100 × 700/800 × 600 → 10; Tastenschutz → 8 (b, c); Ansicht bleibt, Auswahl, Dialoge → 4 (a, d), 6 (j); neue Aktion löscht Wiederholen → 1 (c), 6 (f). §4 Ziehen, Zahlenfeld, Mehrfachaktionen, 100 Schritte/200 MB → R4, 1 (d, e, g), 6 (c, d, e), 7 (c). §5 Gespeichert → R7, 6 (f, g, k). §6/§6a.1–8 → R1–R10, 2, 3, 4, 7, 8. §7 Fehler → 6 (i); lange Berechnung → R8, 6 (j). §8 Tests → 1–10; Byte-Vergleich über `encode_version` → 6 (a); Handliste → 11, 12. §9 → Spec-Kopf.
**Schritt-Scan:** Testschritte nennen Namen, Eingaben und erwartete Werte; Code nur als Signaturen und feste Texte.
**Typkonsistenz:** `History`, `Tracker`, `StepId`, `snapshot_bytes`, `MAX_STEPS`, `MAX_BYTES` (1) → 2, 6, 7; `Snapshot::{same_content, same_model}`, `RestoreKind`, `Document::restore` (3) → 4, 6; `restore_document`, `UNDO_FAILED`, `REDO_FAILED`, `DrawingUi::forget_missing` (4) → 6; `describe_change`, `describe_sketch_change`, `FALLBACK` (5) → 6, 7; `DocTab::{observe_history, undo, redo, undo_label, redo_label, undo_blocked, redo_blocked, dialog_open, close_dialogs, replace_state}`, `Workspace::observe_history` (6) → 7–10; `AppState::{observe_sketch, undo_sketch, redo_sketch, sketch_undo_label, sketch_redo_label}`, `SketchSession::after_history_jump` (7) → 6 (Weiterleitung), 8–10; `Icon::{Undo, Redo}`, `undo_menu_text`, `redo_menu_text` (9) → 10.
**Proportion:** Entscheidungen, Zahlen, Schnittstellen, Tests; kein Funktionsrumpf.
**Bekannte Risiken:** Aufgabe 2 entscheidet synchron/Hintergrund; Aufgabe 6 berührt Speichern und Autosicherung (viele bestehende Tests laufen mit); Aufgabe 10 kann an der Breite bei 1100 px scheitern. Empfehlung Controller: opus für 3, 4, 6, 7; sonnet für 1, 2, 5, 8, 9, 10, 11.

**Fragen an den Nutzer (ändern, was gebaut wird):**
- **Q1 – Namen aus dem Unterschied (R6):** Jede Aktion bekommt ihren Namen aus dem Vergleich vorher/nachher; seltene Fälle heißen nur „Ändern“, Skizzenspiegeln/-muster „Zeichnen“.
- **Q2 – Menü und Knöpfe bei offenem Fenster (R8):** Strg+Z ist bei offenem Fenster gesperrt; Menü und Knöpfe gehen und schließen das Fenster (ungespeicherte Eingaben darin gehen verloren). Während eine Exportdatei geschrieben wird, ist Rückgängig gesperrt statt abzubrechen.
- **Q3 – Namenszähler (R9):** Nach Rückgängig von „Quader 3“ heißt der nächste Quader wieder „Quader 3“.
- **Q4 – Kleine Ergänzungen:** Meldung „Wiederholen ist nicht möglich.“ analog zur Spec; der erste Klick einer Linie ist kein eigener Schritt; eine Skizze mit gleichem Inhalt, aber neuem Stand zählt als geändert.

## Messergebnis Aufgabe 2

Rechner: AMD Ryzen 9 9950X (16 Kerne), 62 GB; Build `cargo test --release -p planora-app --test history_perf -j 2 -- --ignored --nocapture` (Median aus 5 Läufen, Zeiten in ms). Mittleres Modell: 37 Features, 4 Ansichten. Schweres Modell: mittleres Modell + Platte 10 x 10 Bohrungen + Spline-Extrusion + Netz-Körper (200 000 Dreiecke) + 6 Ansichten (2 Schnitte) + 30 Maße.

| Modell | snapshot() | snapshot_bytes | from_snapshot | refresh (mit Netz-Speicher) | refresh (ohne) | davon Netzprüfung | Summe from_snapshot + refresh |
|---|---|---|---|---|---|---|---|
| mittel (Release) | 0,0 | 0,00 | 232,3 | 2,5 | 2,7 | 0,0 | 234,8 |
| schwer (Release) | 0,0 | 0,00 | 250,1 | 39,0 | 42,6 | 0,2 | 289,1 |
| mittel (Debug) | 0,0 | 0,01 | 925,8 | 36,4 | 37,5 | 0,0 | 962,2 |
| schwer (Debug) | 0,1 | 0,01 | 970,1 | 238,7 | 458,9 | 2,4 | 1208,8 |

Den größten Teil von `from_snapshot` macht die Neuberechnung der Features (`recompute_features`); die Netzprüfung ist vernachlässigbar. **Entscheidung (Release):** Summe 289 ms <= 1000 ms und `snapshot()` unter 1 ms <= 50 ms, also synchron wie geplant, Aufgaben 3-10 unverändert. Im Debug-Build liegt das schwere Modell mit 1209 ms über der Grenze (Tests laufen dort, kein Bedienweg). Nicht gemessen: Neuberechnung der Ansichten der Zeichnung nach dem Zurückladen (läuft im Hintergrund, nicht Teil der Grenze).

## Abweichungen bei der Umsetzung

Stand 2026-10-20, nach Aufgabe 10 (Aufgabe 12, die Prüfung im echten Fenster, steht aus). Abweichungen vom Plan und was darüber hinaus festgelegt wurde:

- **Aufgabe 2:** Messung bestanden (Release 289 ms schwer, `snapshot()` 0,0 ms); keine Hintergrundrechnung nötig. Ergänzt gegenüber dem Brief: Marker am Ende, Platte mit mindestens 100 Zylinderwänden.
- **Aufgabe 3:** Feature-Versionen im vollen Weg beginnen bei `next_version` statt bei 1 (offene Dialoge erkennen veraltete Kopien an der Version); Netz-Kennungen werden nie wiederverwendet (`next_mesh` = max); `SketchFeature::eq` vergleicht zusätzlich den Namen; ein NaN-Bild ist nie gleich mit sich selbst (immer voller Weg).
- **Aufgabe 4:** Der Szene-Zwischenspeicher bleibt beim Zeichnungsweg; auf dem vollen Weg werden Flächen-/Kantenwahl und Hover geleert, der gewählte Körper bleibt; `DrawingUi::forget_missing` vergisst Ziele fehlender Zeichnungen (Zeichnungskennungen werden nach Rückgängig wiederverwendet).
- **Aufgabe 5:** „Löschen“ in der Skizze wird über fehlende Kennungen erkannt (strenger als „weniger Elemente“). Das erste Feature in Listenreihenfolge gilt bei mehreren geänderten.
- **Aufgabe 6:** Datei `drawing_dialog.rs` zusätzlich (Stand davor mit `Document::restore(before)` wiederherstellen, wenn die Ansicht einer neuen Zeichnung scheitert). `undo_blocked`/`undo_label` berücksichtigen eine noch nicht übernommene Änderung. Der gewählte Körper wird über Verschieben, Verrunden, Boolesche Operationen mitgeführt. Überholte Autosicherungen werden gelöscht. `history_mut` nur `#[doc(hidden)]`.
- **Aufgabe 7:** Kein `Tracker` im Skizzenverlauf (Vergleich Entwurf gegen aktuelles Bild über den Stand). Aufschub nur für reine Punkte; erstes Rückgängig bei offenem Klick nimmt nur den Punkt zurück. Maßwert 10 auf 20 heißt „Skizze ändern“ (keine Regel „Maß ändern“). `history.rs`, `sketch_panel.rs`, `undo_labels.rs` zusätzlich geändert.
- **Aufgabe 8:** Nach dem Review ein eigenes Feld `AppState::info` für die Erfolgsmeldung in normaler Farbe; die Sperre gilt auch bei gedrückter Maustaste und bei Rückfragen der Felder Projekte und Versionen. Strg+Alt+Z löst auch aus (angenommen).
- **Aufgabe 9:** Wie geplant; Symbole `Icon::Undo` (U+E038) und `Icon::Redo` (U+E036).
- **Aufgabe 10 (wesentliche Abweichung):** Die Knöpfe stehen **im oberen Feld neben den Menüs**, nicht im Befehlsfeld unten. Der erste Anlauf unten (BLOCKED) scheiterte bei 1100 × 700 um etwa 36 px; der Nutzer entschied für das obere Feld. Danach ging die Tab-Leiste bei 1100 auf etwa 100 px zusammen; Option A: Das Symbol „Neu“ ist fest hinter der Leiste, der Trennstrich entfällt, Name und Schließkreuz des aktiven Tabs sind ab 1150 Pixel Breite ganz sichtbar, getestet mit 1 bis 9 Tabs (Strg+W schließt darunter). Der Plan-Hinweis zu `shell_commands.rs` (`+ 2` Knöpfe) entfällt: Die Datei ist unverändert. `bottom_lock` heißt jetzt `edit_lock`. Offene Ideen B bis D stehen in `docs/superpowers/offene-punkte.md`.
- **Aufgabe 11:** Handproben entfallen; Mutationssumme im Bericht (249 Einträge).

