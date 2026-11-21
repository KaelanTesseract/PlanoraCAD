# Restliche Abweichungen von Fusion – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die fünf letzten Abweichungen von Fusion beheben: Drehgriff an der Tangentialebene, Stückliste ohne ausgeblendete Körper mit Hinweis, Enter in allen Befehlsfenstern, Griffe für Rundung und Fase in der Skizze und berichtigte Doku zu „Skizze erstellen“.

**Architecture:** Alles im Zweig `fix/fusion-reste`, ein Commit je Task. Der Ebenen-Griff nutzt das vorhandene `GripShape::Angle`. Die Stückliste filtert in `planora-feature` mit `Document::body_shown`. Enter läuft über einen gemeinsamen Helfer in `dialog.rs`. Die Skizzengriffe schreiben den gezogenen Wert ins Feld, als wäre er getippt, damit Vorschau, Klick und Enter unverändert weiterlaufen.

**Tech Stack:** Rust, egui 0.36, glam (`DVec2`, `DVec3`), Fluent (`tr!`).

**Spec:** `docs/superpowers/specs/2026-11-22-fusion-reste-design.md`

## Global Constraints

- Verhalten wie Fusion; Quellen sind im Entwurf je Punkt verlinkt.
- Neue Texte nur über `tr!`, in `de` und `en`.
- Gezogene Werte werden gerundet: Längen auf 0,01 mm, Winkel auf 0,1° (wie `plane_dialog::set_dragged`).
- Kein geändertes Dateiformat.
- Bestehende Tests bleiben grün. Ausnahmen sind nur Tests, die das alte Verhalten von Punkt 2 oder 4 verlangen; jede solche Anpassung wird im Commit genannt.
- `cargo fmt -p <crate> -- --check` je Crate. Volle Läufe mit `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`. Nur ein Cargo-Lauf zur Zeit.
- Commits enden mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

- **Alle Körper ausgeblendet:** Die Stückliste ist leer, der Hinweis nennt alle, nichts stürzt ab. Ein Stücklistenblatt (`DrawingKind::PartsList`) zeigt eine leere Tabelle. → Test in Task 2.
- **Positionsnummern-Ballon an einem ausgeblendeten Körper:** Der Ballon verschwindet oder hängt an keiner Zeile, ohne Absturz; nach dem Einblenden stimmt die Nummer wieder. → Test in Task 2.
- **Enter in einem mehrzeiligen oder Formel-Feld** (Parameter, Skizzentext): Es übernimmt nicht das Fenster. → Task 3 lässt diese Fenster aus; Test `enter_in_formula_field_does_not_close`.
- **Griff der Fase bei „zwei Abständen“:** Jeder Griff ändert nur seinen eigenen Abstand. Ziehen über die Linie hinaus bleibt in der Grenze, die der Löser annimmt, und zeigt sonst die bekannte Ablehnung. → Test in Task 4.
- **Tangentialebene an einer Kugel:** Der Drehpfeil dreht um die Achse der Kugel (frame.z), wie `tangent_frame`. → Test in Task 1.

---

### Task 1: Drehgriff an der Tangentialebene

**Files:**
- Modify: `crates/planora-feature/src/planes.rs` (neue öffentliche Funktion)
- Modify: `crates/planora-app/src/plane_dialog.rs` (`grip_shape`, Aufruf)
- Test: `crates/planora-app/tests/plane_grip.rs`

**Interfaces:**
- Produces: `pub fn tangent_axis(doc: &Document, def: &PlaneDef, solids: &[&Solid], tol: &Tolerance) -> Option<(DVec3, DVec3)>`. Liefert Punkt und Einheitsrichtung der Drehachse; das ist `frame.origin` und `frame.z` der Fläche (Zylinder, Kegel, Kugel), wie in `tangent_frame`. Für andere `PlaneDef` liefert sie `None`.
- `grip_shape` bekommt dafür einen Parameter `axis: Option<(DVec3, DVec3)>`. Für `PlaneDef::Tangent { angle, .. }` liefert es `GripShape::Angle { center, axis, handle: mid, value: *angle }`; `center` ist die Projektion von `mid` auf die Achse.

- [ ] Test `dragging_the_turn_arrow_turns_the_tangent_plane`: Zylinder anlegen, `PlaneCommand::Tangent`, Fläche und Bezugsebene wählen. `plane_pick.grip` ist `Some(GripShape::Angle {..})`. Den Griff um 90° um die Zylinderachse ziehen: Danach ist `dialog.value` auf 0,1° gleich 90.0, und das Fenster ist offen.
- [ ] Test `tangent_grip_on_a_sphere_turns_about_its_axis`: Mit einer Kugel liegt `axis` parallel zu `frame.z` der Kugel.
- [ ] `other_plane_commands_have_no_grip` bleibt wie er ist (Mid, ThreePoints).
- [ ] Ausführen: `cargo test -p planora-app --test plane_grip`, erst rot, nach der Umsetzung grün.
- [ ] Commit „Tangentialebene: Drehgriff für den Winkel (wie Fusion)“.

### Task 2: Stückliste ohne ausgeblendete Körper, mit Hinweis

**Files:**
- Modify: `crates/planora-feature/src/parts.rs` (`parts_rows`, neue Zählfunktion)
- Modify: `crates/planora-app/src/drawing_view.rs` (Hinweis über der Zeichnung, nur auf dem Bildschirm)
- Modify: `crates/planora-app/src/sheet_dialog.rs` (Hinweis im Blatt-Fenster)
- Modify: `crates/planora-app/locales/{de,en}/*.ftl` (Datei wie bei den übrigen Zeichnungstexten)
- Test: `crates/planora-feature/tests/parts.rs`, `crates/planora-app/tests/parts_sheet.rs`

**Interfaces:**
- `Document::parts_rows()` zählt nur Körper mit `self.body_shown(v.body)`.
- Produces: `pub fn parts_hidden_count(&self) -> usize`. Liefert die Zahl der sichtbaren Körper (`visible_bodies`, ohne Netz-Körper), deren Auge aus ist.
- Text-ID `drawing-parts-hidden` mit `$count` (de: „{ $count } ausgeblendete Körper sind nicht in der Stückliste.“, mit Pluralform für 1: „1 ausgeblendeter Körper ist nicht in der Stückliste.“). Englisch entsprechend.
- Die Zwischenspeicher hängen an `edit_count`, und `set_visibility` erhöht ihn schon. Hier ist nichts weiter nötig; ein Test sichert das ab.

- [ ] Test `hidden_bodies_are_not_in_the_parts_list`: drei Quader, einen ausblenden. Ergebnis: 2 Zeilen, Positionen 1 und 2, `parts_hidden_count() == 1`. Nach dem Einblenden wieder 3 Zeilen.
- [ ] Test `hidden_bodies_folder_empties_the_parts_list`: Den Ordner „Körper“ ausblenden. Ergebnis: 0 Zeilen, `parts_hidden_count() == 3`.
- [ ] Test in `parts_sheet.rs` `parts_list_hint_names_hidden_bodies`: Zeichnung mit Stückliste, einen Körper ausblenden. Der Hinweis steht im Bild, nicht aber im PDF- bzw. Exportpfad. Ein Ballon am ausgeblendeten Körper führt zu keinem Absturz.
- [ ] Ausführen: `cargo test -p planora-feature --test parts` und `cargo test -p planora-app --test parts_sheet`.
- [ ] Commit „Stückliste ohne ausgeblendete Körper, mit Hinweis“.

### Task 3: Enter in allen Befehlsfenstern

**Files:**
- Modify: `crates/planora-app/src/dialog.rs`
- Modify: die Fenster aus dem Entwurf, Abschnitt 3: `surface_project_dialog.rs`, `curvature_dialog.rs`, `isoparametric_dialog.rs`, `spun_profile_dialog.rs`, `sketch_dxf_dialog.rs`, `blend_dialog.rs`, `boolean_dialog.rs`, `dimension_dialog.rs`, `sheet_dialog.rs`, `drawing_dialog.rs`, `export_dialog.rs`, `step_import_dialog.rs`, `import_dialog.rs`, `insert/svg_dialog.rs`, `insert/dxf_dialog.rs`, `insert/canvas_dialog.rs`. Ein Fenster ohne Übernehmen-Knopf wird ausgelassen und im Commit genannt.
- Test: `crates/planora-app/tests/formula_dialogs.rs` (oder eine neue Datei `tests/dialog_enter.rs`)

**Interfaces:**
- Produces: `pub(crate) fn enter_confirms(ui: &egui::Ui, fields: &[egui::Response], ready: bool) -> bool`. Liefert `true`, wenn eines der Einzeilen-Felder in diesem Bild mit Enter verlassen wurde (`left_with_enter`) oder wenn Enter ohne tippendes Feld kam (bisheriges `surface_project_dialog::keys`) und `ready` gilt.
- `surface_project_dialog::keys` bleibt für Esc; die Enter-Hälfte geht über `enter_confirms`.

- [ ] Test `enter_in_the_field_projects_on_the_first_press`: Fenster „Auf Fläche projizieren“, ein Wert im Zahlenfeld, ein Enter. Ergebnis: Das Feature ist angelegt und das Fenster zu.
- [ ] Je ein gleicher Test für Krümmungskamm und Verrundung (`blend_dialog`).
- [ ] Test `enter_in_formula_field_does_not_close`: Im Parameter-Fenster schließt Enter im Formelfeld das Fenster nicht.
- [ ] Ausführen: `cargo test -p planora-app --test formula_dialogs` und die neuen Tests.
- [ ] Commit „Enter übernimmt in allen Befehlsfenstern direkt im Feld“.

### Task 4: Griffe für Rundung und Fase in der Skizze

**Files:**
- Modify: `crates/planora-app/src/sketch_kinds.rs` (Startlänge der Fase, Griffe, Zug)
- Modify: `crates/planora-app/src/sketch_input.rs` (Wert setzen wie getippt)
- Modify: die Stelle, die die Vorschau der Skizzenwerkzeuge zeichnet und Klicks verteilt (beim Bauen per `graphify query "sketch tool preview draw pointer press"` finden)
- Test: `crates/planora-app/tests/sketch_fillet.rs`

**Interfaces:**
- Produces: `DrawInput::set_dragged(&mut self, kind: FieldKind, v: f64)`. Rundet Längen auf 0,01, schreibt den Wert ins Feld und gilt als `locked`.
- Produces: `pub(crate) fn kind_grips(&self, sk: &Sketch, kind: ToolKind) -> Vec<KindGrip>` mit `pub(crate) struct KindGrip { field: FieldKind, at: DVec2, dir: DVec2 }`. Für `Fillet` gibt es einen Griff in der Mitte des Vorschaubogens, Richtung von der Ecke weg. Für `ChamferEqual` und `ChamferDistanceAngle` einen Griff am Fasenpunkt auf der ersten Linie. Für `ChamferTwoDistances` zwei Griffe, je einen auf ihrer Linie.
- Neue Startlänge der Fase: `chamfer_default_length(&self, sk) -> Option<f64>` nach derselben Regel wie `fillet_default_radius`, mit `FILLET_DEFAULT_SHARE` und Halbieren. `length_at` nimmt das Feld, sonst diese Startlänge. `pointer_to_corner` entfällt; sein Aufruf bei Zeile ~1663 (Feldvorgabe) nimmt die Startlänge.
- Ziehen: Der Zeiger wird auf die Linie bzw. den Strahl des Griffs projiziert. Der neue Wert ist der Abstand des projizierten Punkts zur Ecke (`sk.fillet_corner`). Bei der Rundung ist das der Radius, der zur Lage des Bogenmittelpunkts passt. Ein Druck auf einen Griff legt nichts an; erst Loslassen und ein Klick neben dem Griff oder Enter legt an (wie heute). Esc während des Zugs nimmt nur den Zug zurück.

- [ ] Test `dragging_the_fillet_grip_sets_the_radius`: Ecke, Rundung, beide Linien wählen. Den Griff ziehen: Das Radiusfeld zeigt den gerundeten Wert, und die Vorschau nutzt ihn. Der Klick legt die Rundung mit diesem Radius an.
- [ ] Test `chamfer_starts_with_a_default_length_not_the_pointer`: Nach der Wahl beider Linien ändert eine Zeigerbewegung den Plan nicht. Die Länge ist `FILLET_DEFAULT_SHARE` mal die kürzere Linie.
- [ ] Test `two_distance_chamfer_grips_change_only_their_own_distance`.
- [ ] Tests, die die Länge vom Zeiger verlangen (z. B. `chamfer_equal_distance_has_its_dimension_and_keeps_the_corner`, falls betroffen), werden auf Tippen oder Ziehen umgestellt, ohne die Erwartung an das Ergebnis zu ändern. Jede Umstellung wird im Commit genannt.
- [ ] Ausführen: `cargo test -p planora-app --test sketch_fillet`.
- [ ] Commit „Skizze: Radiusgriff an der Rundung, Fase mit Startlänge und Griffen (wie Fusion)“.

### Task 5: Doku und Abschluss

**Files:** Modify: `docs/superpowers/offene-punkte.md` (Abschnitte „Teilprojekt Skizze erstellen (wie Fusion)“ und „Abweichungen von Fusion“), `docs/funktionen.md`

- [ ] „Skizze erstellen“: Den Wortlaut präzisieren. Eine sichtbare Grundebene gewinnt, wenn sie vorn liegt. Eine gesperrte Vorauswahl (`sketch_on_face_blocked`) gibt nur eine Meldung und keinen Wahlmodus.
- [ ] Abschnitt „Abweichungen von Fusion“: Die erledigten Grenzen abhaken. Neue Grenzen nennen: Maße an ausgeblendeten Körpern bleiben ohne Kante.
- [ ] `docs/funktionen.md`: Griffe und Stückliste nachtragen.
- [ ] Volle Prüfung Windows (fmt je Crate, clippy `-D warnings`, alle Tests, deny, Messtests im Release) und WSL (`~/wsl-s6-full.sh`), danach WSL aufräumen.
- [ ] Frischer Prüfer über den ganzen Zweig; Funde einarbeiten.
- [ ] Handtest erst nach Dennis' Go.
- [ ] Commit, Vault-Notiz.
