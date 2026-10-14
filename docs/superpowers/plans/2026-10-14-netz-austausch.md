# PlanoraCAD Netz-Austausch – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Körper als Dreiecksnetz in STL (binär), OBJ, 3MF und glTF (.glb) ausgeben, STL (binär und Text) und OBJ einlesen, eingelesene Netze als „Netz-Körper“ im Dokument (Dateiformat 3) führen und beides über die Fenster „Exportieren …“ und „Netz importieren …“ bedienen.

**Architecture:** Neues Paket `crates/planora-exchange` mit dem Netztyp `ExchangeMesh` (f32-Punkte, u32-Punktnummern), Prüfung (`check_mesh`, `weld_exact`), einheitlicher Quelle `TriSource` → `collect_body` → `ExportBody`, Schreibern und Lesern je Format, eigenem ZIP/CRC-32 und Dateifunktionen mit Hilfsdatei. `planora-feature` bekommt `FeatureKind::MeshBody` und eine Netztabelle im Dokument (Netz-Körper erzeugen keinen Kernel-Körper, sie erscheinen über `visible_meshes`), `planora-project` das Dateiformat 3 mit Base64-Netzdaten, `planora-app` Anzeige, Eigenschaften, Bearbeitungssperre, Arbeitsthreads, eine eigene Dateiauswahl und die beiden Fenster.

**Tech Stack:** Rust (Edition 2024), bestehende Crates (`glam` über `planora-math`, `thiserror`, `earcutr`, `serde`/`serde_json`, `egui`); **keine neue externe Abhängigkeit**.

**Spec:** `docs/superpowers/specs/2026-10-14-netz-austausch-design.md` (bindend, vom Nutzer freigegeben). Lernstand: `docs/superpowers/offene-punkte.md`, Plan des Vorgängers `docs/superpowers/plans/2026-10-13-rundungen-und-fasen.md`.

## Entscheidungen zur Spec (Planungsstand 2026-10-14)

Sie konkretisieren die Spec; Abweichungen werden in Task 18 in die Spec (§12 „Änderungen bei der Umsetzung“) eingetragen.

- **P1 – Schnittstelle der Schreiber:** Alle Quellen laufen über `TriSource` (Punkte in f64 nach der Körpertransformation). `collect_body(&dyn TriSource) -> Result<ExportBody, ExchangeError>` wandelt nach f32, prüft Punktnummern und verschmilzt bitgleiche Punkte. Die Schreiber `write_stl`, `write_obj`, `write_3mf`, `write_glb` nehmen `&[ExportBody]` (`ExportBody { name: String, mesh: ExchangeMesh }`). Quellen: `SolidSource { name, mesh: &planora_mesh::Mesh }` (Kernel-Körper nach `tessellate`) und `PlacedSource { name, mesh: &ExchangeMesh, transform: MeshTransform }` (Netz-Körper).
- **P2 – Bericht:** `MeshReport` hat zusätzlich `flipped_edges` (Kanten, deren zwei Dreiecke in derselben Richtung laufen); `closed` heißt: mindestens eine Kante, keine offene, keine an mehr als zwei Dreiecken, keine falsch herum. Dreiecke mit doppelter Punktnummer zählen als entartet und tragen keine Kanten bei; Dreiecke mit Fläche genau null zählen als entartet, ihre Kanten zählen mit.
- **P3 – Verschmelzen:** `weld_exact` verschmilzt Punkte mit bitgleichen Koordinaten, wobei −0 als +0 gilt, entfernt unbenutzte Punkte und ordnet die Punkte nach ihrer ersten Verwendung. Beim Einlesen werden STL **und** OBJ verschmolzen (OBJ-Dateien anderer Programme verdoppeln Punkte oft an Normalen- oder Texturnähten).
- **P4 – Lesen ohne Speicher nach Kopfzahlen:** Leser bekommen die Dateigröße `len` und einen `Progress`. Reserviert wird höchstens `RESERVE_STEP = 65 536` Dreiecke im Voraus, danach wächst der Speicher mit dem Gelesenen. Weitere benannte Grenzen: `MAX_VERTICES = 3 · MAX_TRIANGLES`, `MAX_LINE_BYTES = 1 MiB` je Textzeile. Zeilenenden `\n`, `\r\n` und `\r`; ein UTF-8-BOM am Dateianfang entfällt; Bytes, die kein UTF-8 sind, werden ersetzt (Kommentare in Latin-1 brechen nichts).
- **P5 – STL als Text:** erkannt, wenn die Datei nach BOM und Leerraum mit `solid` beginnt (Groß-/Kleinschreibung egal) und ihre Größe nicht `84 + 50 · n` ist. Gelesen werden `solid`, `facet`, `outer`, `vertex`, `endloop`, `endfacet`, `endsolid`; jedes andere erste Wort ist ein `Syntax`-Fehler mit Zeilennummer (so fällt eine kaputte Binärdatei mit `solid`-Kopf benannt auf).
- **P6 – OBJ lesen:** Eine Ecke darf nur auf schon gelesene Punkte verweisen (positive oder negative Nummern), sonst `IndexOutOfRange` mit Zeile. Vielecke ab vier Ecken: konvex (jede Ecke biegt im Sinn der Newell-Normalen ab) → Fächer; sonst `earcutr` in der Ebene der größten Normalkomponente, Orientierung je Dreieck an die des Vielecks angeglichen; scheitert das oder fehlen Dreiecke → `Polygon` mit Zeile. `earcutr` ist Abhängigkeit von `planora-mesh` (nicht des Kernels, wie die Spec schreibt) und kommt als Arbeitsbereichs-Abhängigkeit dazu (kein neues Paket).
- **P7 – 3MF:** Jeder ZIP-Eintrag wird zweimal erzeugt (erst Größe und CRC messen, dann schreiben), damit nichts im Speicher liegen muss; ändert sich der Inhalt zwischen beiden Durchgängen → `Corrupt`. Ohne ZIP64: ab 4 GiB je Eintrag oder Archiv → `TooLargeForFormat { format: "3MF" }`. Feste Zeitangabe 1.1.1980 (gleiche Datei bei gleichem Inhalt). Namen werden für XML maskiert, Steuerzeichen durch Leerzeichen ersetzt. Metadaten `Application = PlanoraCAD`.
- **P8 – glTF:** Normalen je Punkt mit harten Kanten über `shade` (Grenzwinkel 30°, dieselbe Funktion wie die Anzeige der Netz-Körper); Punkte an harten Kanten werden verdoppelt. JSON wird selbst geschrieben (Zahlen über `Display` von f32, ohne Exponent). Längen über 4 GiB → `TooLargeForFormat { format: "glTF" }`.
- **P9 – Dateien:** `export_file(path, format, bodies)` schreibt nach `.<Name>.planora-tmp` im Zielordner, ruft `sync_all` und benennt dann um; bei jedem Fehler wird die Hilfsdatei entfernt, eine vorhandene Zieldatei bleibt unverändert. `import_file(path, progress)` wählt den Leser nach der Endung (`stl`/`obj`, Groß-/Kleinschreibung egal), sonst `UnknownFormat`.
- **P10 – Netz-Körper im Feature-Modell:** `FeatureKind::MeshBody { mesh: MeshId, offset: DVec3, rotation: DQuat, scale: f64 }` erzeugt keinen Kernel-Körper (`FeatureResult.bodies` bleibt leer). `visible_bodies` bleibt unverändert (nur Kernel-Körper), neu ist `visible_meshes() -> Vec<VisibleMesh>`. Boolesche Operation, Verschieben, Verrunden/Fase und Skizze auf einer Fläche (auch Kantenkopien) mit einem Netz-Körper als Eingang ergeben `FeatureError::MeshBodyLocked` („Netz-Körper lassen sich nicht bearbeiten“). In der Oberfläche verschiebt „Position“ einen Netz-Körper über seine eigene Lage (kein Feature „Verschieben“).
- **P11 – Netztabelle:** `Document.meshes: BTreeMap<MeshId, Arc<ExchangeMesh>>`; Kennungen ab 1, nie wiederverwendet. `add_mesh_body` legt Netz und Feature nur gemeinsam an (sonst Dokument unverändert, auch Zähler und Änderungszahl). Löschen entfernt nicht mehr benutzte Netze. `Snapshot.meshes` enthält nur benutzte Netze.
- **P12 – Dateiformat 3:** `FORMAT_VERSION = 3`; geschrieben wird die kleinste passende Version (3 mit Netz-Körper, 2 mit Verrunden, sonst 1). Neues Feld `netze: [{ id, daten }]` nur, wenn es Netze gibt (`skip_serializing_if`), daher bleiben Format-1/2-Dateien Byte für Byte gleich. `daten` = Base64 von `[u32 Punktzahl][u32 Dreieckszahl][f32 × 3 × Punkte][u32 × 3 × Dreiecke]`, little endian. Base64 liegt in `planora-project/src/base64.rs` (nur dort gebraucht). Vor dem Dekodieren: Länge der Zeichenkette (dekodierte Länge ≤ `MAX_FILE_BYTES`, auch in Summe aller Netze), Kopf aus den ersten 12 Zeichen gegen die Grenzen und gegen die Gesamtlänge. Netz-Körper in Format 1/2 → beschädigt. Mechanische Testanpassung wie in TP8 (P7): „zu neu“ ist jetzt Format 4 (`tests/docstore.rs`, `tests/format2.rs`, mit Kommentar).
- **P13 – Anzeige:** Ein Netz-Körper ist in der App ein `Body` mit `kind: BodyKind::Mesh`, leerem `Solid`, einem `FaceRange` (Fläche 0) über das ganze Netz und ohne Kantenlinien. Über `display_triangles` (Vorgabe `DISPLAY_TRIANGLES = 5 000 000`) zeigt die Ansicht nur jedes k-te Dreieck (Vorschau), Volumen, Oberfläche und Hülle kommen immer aus dem ganzen Netz. Der Prüfbericht wird je `MeshId` einmal berechnet und gemerkt.
- **P14 – Dateiauswahl:** Ein Dialog des Betriebssystems bräuchte eine neue Abhängigkeit. Stattdessen gibt es eine eigene Dateiauswahl im Fenster (Ordner wechseln, Ordner eintippen, Datei wählen, Dateiname, Ersetzen nach zweitem Klick bestätigen). Abweichung von Spec §8, wird in §12 eingetragen.
- **P15 – Arbeitsthreads:** `std::thread` + `mpsc` + `Progress` (Abbruchflag, erledigter Anteil). Lesen bricht alle 4096 Zeilen oder Dreiecke ab; das Vernetzen eines Kernel-Körpers lässt sich nicht unterbrechen, abgebrochen wird zwischen zwei Körpern. Das Schreiben der Datei läuft ebenfalls im Arbeitsthread, ohne Abbrechen. Ein Thread, der ohne Ergebnis endet (Panik), wird als Meldung angezeigt, nie als Absturz.
- **P16 – Reihenfolge der `match`-Stellen:** Weil `match` über `FeatureKind` erschöpfend ist, kommen `KindDto::NetzKoerper`, die Umwandlung, `BodyKind::Mesh` und alle Arme in App und Projekt schon in Task 10; Formatnummer, Netzdaten, Golden-Datei und feindliche Dateien in Task 11 (wie TP8 P13).
- **P17 – Drehung:** gespeichert als Quaternion `[x, y, z, w]` (Feld `drehung`), angezeigt als Euler-Winkel in Grad in der Reihenfolge XYZ (`glam::EulerRot::XYZ`); „normiert“ heißt `|‖q‖ − 1| ≤ ROTATION_NORM_TOL = 1e-6`.

## Global Constraints

- Reines Rust, Edition 2024, `rust-version = 1.88`; Einheit Millimeter. Keine freien Epsilon-Zahlen in neuem Code: jede Schranke ist eine benannte `const` mit Kommentar.
- Grenzen (Spec E8, §5): `MAX_TRIANGLES = 50_000_000`, `MAX_FILE_BYTES = 8 GiB` (`8 * 1024 * 1024 * 1024`). Kein Speicher nach Zahlen im Dateikopf, nur nach tatsächlich Gelesenem.
- Formate (Spec §3): STL binär mit 80-Byte-Kopf, Text „Planora“, Rest Nullen, u32 Dreieckszahl, je Dreieck Normale, drei Punkte, u16 Attribut 0; OBJ mit `o Name`, `v`, `vn` (gemittelte Flächennormale), `f a//n b//n c//n`; 3MF mit `[Content_Types].xml`, `_rels/.rels`, `3D/3dmodel.model`, `unit="millimeter"`, ein `object` (`type="model"`) und ein `build`/`item` je Körper, Punkte mit mindestens 6 Nachkommastellen; glTF als `.glb`, ein Puffer, je Körper ein Mesh mit `POSITION`, `NORMAL`, `indices` (u32), ein Knoten je Mesh, `asset.version = "2.0"`, `generator = "PlanoraCAD"`, Meter = mm × 0,001.
- Qualität (Spec E5): Grob 0,1 mm, Normal 0,01 mm, Fein 0,001 mm Sehnentoleranz. Netz-Körper: Maßstab endlich, `1e-6 ≤ scale ≤ 1e6`, Drehung normiert, Lage endlich. Glatte Normalen unter 30° (Spec §6). Vorschau ab 5 Millionen Dreiecken (Spec §11). Einheiten beim Import: Millimeter, Zentimeter, Meter, Zoll, Vorgabe Millimeter.
- Oberfläche: „Datei → Exportieren …“ (Strg+Umschalt+E), „Datei → Netz importieren …“. Fenster über die ganze Fensterbreite (Rand `DIALOG_MARGIN = 16`), nebeneinander stehende Karten gleich hoch (`cards::card_row`). Texte deutsch, einfache Laiensprache, keine Hinweise auf die Herkunft von Daten, keine Fahrzeugmarken. Menüpunkte für Netz-Körper ausgegraut mit Hinweis „Netz-Körper lassen sich nicht bearbeiten.“
- **Keine neue externe Abhängigkeit.** `cargo deny check` bleibt grün; `deny.toml` nie selbstständig erweitern – bei Befund anhalten und den Menschen fragen. Projektlizenz GPL-3.0-only. Kein Autodesk-Material.
- **Geschützte Pfade bleiben bitgleich zu `master` 134cde9:** `crates/planora-kernel/src/boolean/**`, `crates/planora-mesh/src/{tessellate,trimmed,discretize,check}.rs`. Prüfung am Ende jeder Task: `git diff --stat 134cde9 -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs` ist leer.
- Golden-Dateien `crates/planora-project/tests/data/format1.json`, `format1_voll.json`, `format2.json` bleiben unverändert; neu ist nur `format3.json`.
- **Tests:** Alle bisherigen Tests bleiben unverändert grün; einzige erlaubte Änderung ist die mechanische in P12 (mit Kommentar). Tests werden nie abgeschwächt (Schranken, Asserts, `#[ignore]`), um grün zu werden: Schlägt ein Test fehl, Ursache beheben oder anhalten und berichten. Jedes Testprogramm läuft im Debug-Build in höchstens etwa 30 s; längere Reihen bekommen `#[ignore = "… nur mit --release --ignored (etwa N s)"]`. Zufallstests nur mit festem Startwert (`Lcg`).
- Keine Panics auf Nutzereingaben und Dateiinhalten; jeder Fehler ist eine benannte `ExchangeError`-Variante mit deutschem Text (bei Textformaten mit Zeilennummer).
- Clippy-Stil des Repos: `cargo clippy --workspace --all-targets -- -D warnings` mit `clippy::all`. Daher u. a.: `is_multiple_of` statt `% n == 0`, Formatargumente inline (`{name}`), `as_chunks::<3>()` statt Indexschleifen, `writeln!` statt `"…\n"`.
- Plattform Windows (MSVC) und Linux. Befehle in Bash (Git Bash) mit `export PATH="$PATH:/c/Users/denni/.cargo/bin"`; PowerShell 5.1 kennt `&&` nicht. Dateien mit dem Write-Werkzeug schreiben, nicht über Heredocs.
- Am Ende jeder Task grün (vorher einmal `cargo fmt` ausführen; der Code im Plan ist nicht zwingend zeichengenau rustfmt-formatiert): `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p <geänderte Crates>`; am Ende jeder Stufe (Task 5, 9, 14, 18) zusätzlich `cargo test --workspace`, `cargo deny check` und die Prüfung der geschützten Pfade.
- Commits mit ausdrücklichen Pfaden (nie `git add -A`), Nachricht deutsch, Schlusszeile `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- **Ehrlich berichten:** Jede Task endet mit einem Bericht, was geprüft wurde und was nicht.
- **Keine Handprüfung durch Implementierer** (der Nutzer sitzt am Rechner); Task 19 nur nach Rücksprache, ausgeführt vom Controller.

## Review Focus

Eingaben, die die Spec andeutet, die aber leicht durchrutschen; jede Zeile hat einen Test in der genannten Task:

1. **STL-Erkennung an der Grenze:** Binärdateien, deren Kopf mit `solid` beginnt (viele CAD-Programme schreiben das), Textdateien mit BOM, `\r\n` oder nur `\r`, mehrere `solid`-Blöcke → richtig erkannt, gleiche Dreiecke. Tests `a_binary_file_whose_header_starts_with_solid_is_read_as_binary`, `ascii_files_are_read_with_all_line_endings_and_a_bom`, `several_solid_blocks_are_merged` (Task 3).
2. **Dateien, die mehr versprechen, als sie enthalten:** Kopfzahl größer als die Datei, falsche Größenangabe, 8-GB-Behauptung, endlose Zeile, Base64-Netz mit falschen Anzahlen → benannter Fehler und keine große Speicheranforderung. Tests `hostile_binary_files_get_named_errors`, `an_endless_line_is_refused` (Task 3), `hostile_files_never_ask_for_much_memory` (Task 5), `hostile_mesh_data_is_rejected_by_name` (Task 11).
3. **OBJ aus fremden Programmen:** negative Nummern, `v/vt/vn`, Tabulatoren, nicht konvexe und entartete Vielecke, Kommentare in Latin-1, Verweis auf Punkt 0 oder auf noch nicht gelesene Punkte → richtiges Netz oder benannter Fehler mit Zeile. Tests `all_corner_forms_and_negative_indices_are_read`, `a_convex_quad_becomes_a_fan_and_a_concave_polygon_is_cut_by_ear_clipping`, `hostile_obj_files_get_named_errors`, `line_endings_bom_tabs_and_foreign_comment_bytes_are_fine` (Task 4).
4. **Nicht geschlossene oder falsch orientierte Netze** (Freiformkörper, fremde Dateien) → Bericht zählt offene, falsch herum liegende und mehrfach benutzte Kanten; Export warnt und schreibt trotzdem; Import legt den Netz-Körper trotzdem an. Tests `a_flipped_triangle_is_reported_as_not_closed`, `a_fin_is_non_manifold` (Task 2), `the_window_warns_about_open_meshes_and_still_exports` (Task 16), `an_open_mesh_is_imported_with_a_warning` (Task 17).
5. **Netz-Körper als Eingang anderer Features und Lagen außerhalb des Bereichs** (auch aus einer Datei) → Feature mit benannter Meldung, Dokument bleibt gültig, Import ändert bei Fehler nichts. Tests `mesh_bodies_cannot_be_edited_by_other_features`, `bad_parameters_become_named_error_states`, `import_is_all_or_nothing` (Task 10), `hostile_mesh_data_is_rejected_by_name` (Teil „geladen, aber benannt“, Task 11), `mesh_bodies_refuse_booleans_blends_and_sketches` (Task 12).

## Dateistruktur

```
Cargo.toml                                   (ändern: planora-exchange in workspace.dependencies)
crates/planora-math/src/lib.rs               (ändern: DQuat, EulerRot weitergeben)
crates/planora-exchange/                     (neu)
├─ Cargo.toml
├─ src/lib.rs                                öffentliche API
├─ src/error.rs                              ExchangeError
├─ src/limits.rs                             MAX_TRIANGLES, MAX_FILE_BYTES, …
├─ src/mesh.rs                               ExchangeMesh, MeshTransform, Bytes für Format 3
├─ src/progress.rs                           Progress
├─ src/check.rs                              MeshReport, check_mesh, weld_exact
├─ src/source.rs                             TriSource, SolidSource, PlacedSource, ExportBody, collect_body
├─ src/text.rs                               Zeilenleser, Byte-Zähler, Zahlen mit Zeile
├─ src/stl.rs, src/obj.rs                    Stufe 1
├─ src/zip.rs, src/threemf.rs                Stufe 2
├─ src/normals.rs, src/gltf.rs               Stufe 2
├─ src/file.rs                               Format, export_file, import_file
└─ tests/{basics,check,stl,obj,stage1,alloc_guard,zip,threemf,shade,gltf,stage2,bytes}.rs, tests/common/mod.rs
crates/planora-feature/src/{model,error,compute,ops,snapshot,lib}.rs, src/meshes.rs (neu), tests/mesh_bodies.rs
crates/planora-project/src/{format,convert,lib}.rs, src/base64.rs (neu)
crates/planora-project/tests/{base64,format3}.rs, tests/data/format3.json; mechanisch: tests/{docstore,format2}.rs
crates/planora-app/src/{state,properties,feature_dialog,icons,boolean_dialog,app,workspace,lib}.rs
crates/planora-app/src/{mesh_body,jobs,file_chooser,cards,export_dialog,import_dialog}.rs (neu)
crates/planora-app/tests/{mesh_body_state,mesh_body_ui,mesh_flow,exchange_infra,export_dialog,import_dialog}.rs
docs: README.md, docs/superpowers/offene-punkte.md, Spec §12
```

Gemeinsame Testnetze (in jeder Testdatei, die sie braucht, als eigene Funktion; Integrationstests verschiedener Crates teilen keinen Code):

```rust
/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}
```

Jede Testdatei, die sie braucht, enthält sie im Plan vollständig (in App-, Feature- und Projekt-Tests als `Arc<ExchangeMesh>` mit `Arc::new(…)` um denselben Körper).

---

## Stufe 1 – Paket `planora-exchange`: Fehler, Grenzen, Prüfung, STL, OBJ

### Task 1: Paketgerüst, Fehler, Grenzen, Netztyp und Fortschritt

**Files:**
- Modify: `Cargo.toml` (Abschnitt `[workspace.dependencies]`, interne Crates)
- Modify: `crates/planora-math/src/lib.rs` (Zeile `pub use glam::{DMat3, DMat4, DVec2, DVec3};`)
- Create: `crates/planora-exchange/Cargo.toml`, `crates/planora-exchange/src/lib.rs`, `src/error.rs`, `src/limits.rs`, `src/mesh.rs`, `src/progress.rs`
- Test: `crates/planora-exchange/tests/basics.rs`

**Interfaces:**
- Consumes: `planora_mesh::{Mesh, MassProperties, mass_properties}`, `planora_math::{DVec3, DQuat}` (DQuat neu weitergegeben).
- Produces (alle `pub` in `planora_exchange`):
  ```rust
  pub enum ExchangeError { Io { msg: String }, FileTooLarge { bytes: u64 }, TooManyTriangles, TooManyVertices,
      Truncated, CountTooLarge { claimed: u64, possible: u64 }, NotFinite { line: Option<usize> },
      BadNumber { line: usize }, IndexOutOfRange { line: usize }, Syntax { line: usize, msg: &'static str },
      Polygon { line: usize }, Empty, TooLargeForFormat { format: &'static str }, Corrupt(String),
      UnknownFormat(String), Cancelled }            // Debug, Clone, PartialEq, Eq, Error; From<std::io::Error>
  pub const MAX_TRIANGLES: usize = 50_000_000;  pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
  pub const MAX_VERTICES: usize = 3 * MAX_TRIANGLES;  pub const MAX_LINE_BYTES: usize = 1024 * 1024;
  pub const RESERVE_STEP: usize = 65_536;  pub const CHECK_EVERY: u64 = 4096;  pub const READ_BUFFER: usize = 64 * 1024;
  pub struct ExchangeMesh { pub positions: Vec<[f32; 3]>, pub indices: Vec<u32> }   // Clone, Debug, Default, PartialEq
  impl ExchangeMesh { pub fn triangle_count(&self) -> usize; pub fn vertex_count(&self) -> usize;
      pub fn point(&self, i: usize) -> DVec3; pub fn validate(&self) -> Result<(), ExchangeError>;
      pub fn mass(&self, t: &MeshTransform) -> MassProperties; pub fn placed(&self, t: &MeshTransform) -> Mesh; }
  pub struct MeshTransform { pub offset: DVec3, pub rotation: DQuat, pub scale: f64 }  // Clone, Copy, Debug, PartialEq, Default
  impl MeshTransform { pub const IDENTITY: MeshTransform; pub fn apply(&self, p: DVec3) -> DVec3; }
  pub struct Progress;  // Debug, Default
  impl Progress { pub fn new() -> Progress; pub fn cancel(&self); pub fn is_cancelled(&self) -> bool;
      pub fn check(&self) -> Result<(), ExchangeError>; pub fn set_total(&self, total: u64);
      pub fn advance(&self, n: u64); pub fn fraction(&self) -> f32; }
  // planora_math: pub use glam::{DMat3, DMat4, DQuat, DVec2, DVec3, EulerRot};
  ```

- [ ] **Step 1: Arbeitsbereich und Paket anlegen**

In `Cargo.toml` unter `# interne Crates` nach `planora-project = { path = "crates/planora-project" }` einfügen:

```toml
planora-exchange = { path = "crates/planora-exchange" }
```

In `crates/planora-math/src/lib.rs` die Zeile `pub use glam::{DMat3, DMat4, DVec2, DVec3};` ersetzen durch:

```rust
pub use glam::{DMat3, DMat4, DQuat, DVec2, DVec3, EulerRot};
```

`crates/planora-exchange/Cargo.toml`:

```toml
[package]
name = "planora-exchange"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
planora-math.workspace = true
planora-mesh.workspace = true
thiserror.workspace = true

[lints]
workspace = true
```

- [ ] **Step 2: Tests schreiben** (`crates/planora-exchange/tests/basics.rs`)

```rust
//! Fehler, Grenzen, Netztyp, Lage und Fortschritt (Teilprojekt 9, Task 1).

use planora_exchange::*;
use planora_math::{DQuat, DVec3};

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}

#[test]
fn limits_match_the_spec() {
    assert_eq!(MAX_TRIANGLES, 50_000_000);
    assert_eq!(MAX_FILE_BYTES, 8 * 1024 * 1024 * 1024);
    assert_eq!(MAX_VERTICES, 150_000_000);
    assert_eq!(MAX_LINE_BYTES, 1 << 20);
    assert_eq!(RESERVE_STEP, 65_536);
}

#[test]
fn error_texts_are_german_and_name_the_line() {
    assert_eq!(
        ExchangeError::BadNumber { line: 7 }.to_string(),
        "Zeile 7: Die Zahl lässt sich nicht lesen"
    );
    assert!(
        ExchangeError::NotFinite { line: Some(3) }
            .to_string()
            .starts_with("Zeile 3: ")
    );
    assert!(
        !ExchangeError::NotFinite { line: None }
            .to_string()
            .contains("Zeile")
    );
    assert_eq!(
        ExchangeError::Syntax {
            line: 12,
            msg: "Unbekanntes Wort"
        }
        .to_string(),
        "Zeile 12: Unbekanntes Wort"
    );
    assert!(
        ExchangeError::IndexOutOfRange { line: 4 }
            .to_string()
            .starts_with("Zeile 4: ")
    );
    assert!(
        ExchangeError::Polygon { line: 9 }
            .to_string()
            .starts_with("Zeile 9: ")
    );
    let big = ExchangeError::FileTooLarge {
        bytes: MAX_FILE_BYTES + 1,
    }
    .to_string();
    assert!(big.contains("8 GB"), "{big}");
    let count = ExchangeError::CountTooLarge {
        claimed: 1000,
        possible: 2,
    }
    .to_string();
    assert!(count.contains("1000") && count.contains('2'), "{count}");
    assert!(
        ExchangeError::TooManyTriangles
            .to_string()
            .contains("50 Millionen")
    );
    assert!(
        ExchangeError::TooLargeForFormat { format: "3MF" }
            .to_string()
            .contains("3MF")
    );
    assert!(
        ExchangeError::UnknownFormat("step".into())
            .to_string()
            .contains("„step“")
    );
    assert_eq!(ExchangeError::Cancelled.to_string(), "Abgebrochen");
    let eof: ExchangeError = std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into();
    assert_eq!(eof, ExchangeError::Truncated);
    let other: ExchangeError = std::io::Error::other("kaputt").into();
    assert!(matches!(other, ExchangeError::Io { .. }));
}

#[test]
fn validation_rejects_broken_meshes_by_name() {
    tetra().validate().unwrap();
    assert_eq!(ExchangeMesh::default().validate(), Err(ExchangeError::Empty));
    let mut m = tetra();
    m.indices.pop();
    assert!(matches!(m.validate(), Err(ExchangeError::Corrupt(_))));
    let mut m = tetra();
    m.indices[0] = 4;
    assert!(matches!(m.validate(), Err(ExchangeError::Corrupt(_))));
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut m = tetra();
        m.positions[1][2] = bad;
        assert_eq!(m.validate(), Err(ExchangeError::NotFinite { line: None }));
    }
}

#[test]
fn the_transform_scales_then_rotates_then_moves() {
    let t = MeshTransform {
        offset: DVec3::new(1.0, 2.0, 3.0),
        rotation: DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
        scale: 2.0,
    };
    let p = t.apply(DVec3::X);
    assert!((p - DVec3::new(1.0, 4.0, 3.0)).length() < 1e-12, "{p}");
    assert_eq!(MeshTransform::default(), MeshTransform::IDENTITY);
    assert_eq!(
        MeshTransform::IDENTITY.apply(DVec3::new(5.0, -1.0, 2.0)),
        DVec3::new(5.0, -1.0, 2.0)
    );
}

#[test]
fn mass_follows_the_transform() {
    let m = tetra();
    let v = m.mass(&MeshTransform::IDENTITY);
    assert!((v.volume - 1000.0 / 6.0).abs() < 1e-9, "{}", v.volume);
    let t = MeshTransform {
        offset: DVec3::new(100.0, 0.0, 0.0),
        rotation: DQuat::from_rotation_x(0.3),
        scale: 2.0,
    };
    let w = m.mass(&t);
    assert!((w.volume - 8.0 * 1000.0 / 6.0).abs() < 1e-6, "{}", w.volume);
    let placed = m.placed(&t);
    assert_eq!(placed.positions.len(), 4);
    assert_eq!(placed.indices, m.indices);
    assert!((placed.positions[0] - DVec3::new(100.0, 0.0, 0.0)).length() < 1e-12);
    assert_eq!(m.point(1), DVec3::new(10.0, 0.0, 0.0));
    assert_eq!((m.vertex_count(), m.triangle_count()), (4, 4));
}

#[test]
fn progress_reports_fraction_and_cancel() {
    let p = Progress::new();
    assert_eq!(p.fraction(), 0.0);
    p.set_total(200);
    p.advance(50);
    assert!((p.fraction() - 0.25).abs() < 1e-6);
    p.advance(500);
    assert_eq!(p.fraction(), 1.0);
    assert_eq!(p.check(), Ok(()));
    assert!(!p.is_cancelled());
    p.cancel();
    assert!(p.is_cancelled());
    assert_eq!(p.check(), Err(ExchangeError::Cancelled));
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test basics`
Expected: FAIL – `src/lib.rs` fehlt bzw. `cannot find type ExchangeMesh`.

- [ ] **Step 4: Implementieren**

`crates/planora-exchange/src/lib.rs`:

```rust
//! Austausch von Dreiecksnetzen (Teilprojekt 9): STL, OBJ, 3MF und glTF schreiben, STL und OBJ
//! lesen, Netze prüfen. Keine neue Abhängigkeit: ZIP und CRC-32 sind selbst geschrieben,
//! Vielecke zerlegt das schon vorhandene `earcutr`.

pub mod error;
pub mod limits;
mod mesh;
mod progress;

pub use error::ExchangeError;
pub use limits::{
    CHECK_EVERY, MAX_FILE_BYTES, MAX_LINE_BYTES, MAX_TRIANGLES, MAX_VERTICES, READ_BUFFER,
    RESERVE_STEP,
};
pub use mesh::{ExchangeMesh, MeshTransform};
pub use progress::Progress;
```

`crates/planora-exchange/src/error.rs`:

```rust
//! Fehler beim Lesen und Schreiben von Netzen (Meldungen deutsch, für die Oberfläche bestimmt).

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExchangeError {
    #[error("Die Datei lässt sich nicht lesen oder schreiben: {msg}")]
    Io { msg: String },
    #[error(
        "Die Datei ist zu groß ({}); erlaubt sind höchstens 8 GB",
        gigabytes(.bytes)
    )]
    FileTooLarge { bytes: u64 },
    #[error("Das Netz hat mehr als 50 Millionen Dreiecke")]
    TooManyTriangles,
    #[error("Das Netz hat mehr als 150 Millionen Punkte")]
    TooManyVertices,
    #[error("Die Datei ist unvollständig: Sie endet, bevor alles gelesen ist")]
    Truncated,
    #[error("Die Datei gibt {claimed} Dreiecke an, ist aber nur groß genug für {possible}")]
    CountTooLarge { claimed: u64, possible: u64 },
    #[error("{}", not_finite(.line))]
    NotFinite { line: Option<usize> },
    #[error("Zeile {line}: Die Zahl lässt sich nicht lesen")]
    BadNumber { line: usize },
    #[error("Zeile {line}: Die Fläche verweist auf einen Punkt, den es nicht gibt")]
    IndexOutOfRange { line: usize },
    #[error("Zeile {line}: {msg}")]
    Syntax { line: usize, msg: &'static str },
    #[error("Zeile {line}: Das Vieleck lässt sich nicht in Dreiecke zerlegen")]
    Polygon { line: usize },
    #[error("Das Netz enthält keine Dreiecke")]
    Empty,
    #[error(
        "Das Netz ist für das Format {format} zu groß; bitte eine gröbere Qualität oder STL wählen"
    )]
    TooLargeForFormat { format: &'static str },
    #[error("Die Netzdaten sind beschädigt: {0}")]
    Corrupt(String),
    #[error("Die Dateiendung „{0}“ wird nicht unterstützt; möglich sind .stl und .obj")]
    UnknownFormat(String),
    #[error("Abgebrochen")]
    Cancelled,
}

/// Größe in Gigabyte mit Komma („9,1 GB“).
fn gigabytes(bytes: &u64) -> String {
    format!("{:.1} GB", *bytes as f64 / 1e9).replace('.', ",")
}

fn not_finite(line: &Option<usize>) -> String {
    match line {
        Some(n) => format!("Zeile {n}: Eine Zahl ist zu groß oder keine gültige Zahl"),
        None => "Die Datei enthält eine Zahl, die zu groß oder keine gültige Zahl ist".into(),
    }
}

impl From<std::io::Error> for ExchangeError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ExchangeError::Truncated
        } else {
            ExchangeError::Io { msg: e.to_string() }
        }
    }
}
```

`crates/planora-exchange/src/limits.rs`:

```rust
//! Grenzen beim Einlesen (Spec E8, §5). Kein Speicher wird nach Zahlen aus dem Dateikopf
//! reserviert, nur nach tatsächlich Gelesenem.

/// Höchstzahl der Dreiecke eines Netzes (Spec E8).
pub const MAX_TRIANGLES: usize = 50_000_000;
/// Größte lesbare Datei: 8 GiB (Spec E8).
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Höchstzahl der Punkte: drei je Dreieck. Mehr kann ein Netz mit [`MAX_TRIANGLES`] Dreiecken
/// nicht benutzen; die Grenze hält OBJ-Dateien mit sehr vielen Punktzeilen im Rahmen.
pub const MAX_VERTICES: usize = 3 * MAX_TRIANGLES;
/// Längste Zeile einer Textdatei (1 MiB). Eine Datei ohne Zeilenumbruch wird so nicht ganz in
/// den Speicher gelesen.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Für so viele Dreiecke wird beim Lesen höchstens im Voraus Speicher reserviert; danach wächst
/// er mit dem Gelesenen.
pub const RESERVE_STEP: usize = 65_536;
/// Alle so viele Zeilen oder Dreiecke prüfen die Leser, ob abgebrochen werden soll.
pub const CHECK_EVERY: u64 = 4096;
/// Puffergröße beim Lesen von Dateien.
pub const READ_BUFFER: usize = 64 * 1024;
```

`crates/planora-exchange/src/mesh.rs`:

```rust
//! Netz für den Austausch: Punkte in `f32` (Millimeter), Dreiecke als Punktnummern; dazu die
//! Lage eines Netz-Körpers.

use crate::error::ExchangeError;
use crate::limits::{MAX_TRIANGLES, MAX_VERTICES};
use planora_math::{DQuat, DVec3};
use planora_mesh::{MassProperties, Mesh, mass_properties};

/// Eingelesenes oder für die Ausgabe vorbereitetes Netz. Dreieck `k` sind die Punkte
/// `indices[3k..3k + 3]`, von außen gesehen gegen den Uhrzeigersinn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExchangeMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl ExchangeMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Punkt `i` in `f64` (`i < vertex_count()`).
    pub fn point(&self, i: usize) -> DVec3 {
        let p = self.positions[i];
        DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
    }

    /// Prüft das Netz: mindestens ein Dreieck, nur ganze Dreiecke, Grenzen eingehalten, jede
    /// Punktnummer vorhanden, alle Zahlen endlich.
    pub fn validate(&self) -> Result<(), ExchangeError> {
        if self.indices.is_empty() {
            return Err(ExchangeError::Empty);
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err(ExchangeError::Corrupt(
                "Die Zahl der Punktnummern ist kein Vielfaches von drei".into(),
            ));
        }
        if self.triangle_count() > MAX_TRIANGLES {
            return Err(ExchangeError::TooManyTriangles);
        }
        if self.positions.len() > MAX_VERTICES {
            return Err(ExchangeError::TooManyVertices);
        }
        let n = self.positions.len();
        if self.indices.iter().any(|&i| i as usize >= n) {
            return Err(ExchangeError::Corrupt(
                "Ein Dreieck verweist auf einen Punkt, den es nicht gibt".into(),
            ));
        }
        if !self
            .positions
            .iter()
            .all(|p| p.iter().all(|c| c.is_finite()))
        {
            return Err(ExchangeError::NotFinite { line: None });
        }
        Ok(())
    }

    /// Volumen, Oberfläche und Schwerpunkt nach der Lage `t` (Netz muss gültig sein).
    pub fn mass(&self, t: &MeshTransform) -> MassProperties {
        mass_properties(&self.placed(t))
    }

    /// Netz mit Punkten nach der Lage `t` in `f64`, ohne Normalen und Flächenbereiche (für
    /// Masseneigenschaften und Hüllquader).
    pub fn placed(&self, t: &MeshTransform) -> Mesh {
        Mesh {
            positions: (0..self.positions.len())
                .map(|i| t.apply(self.point(i)))
                .collect(),
            indices: self.indices.clone(),
            ..Mesh::default()
        }
    }
}

/// Lage eines Netz-Körpers: erst skalieren, dann drehen, dann verschieben (Millimeter).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshTransform {
    pub offset: DVec3,
    pub rotation: DQuat,
    pub scale: f64,
}

impl MeshTransform {
    pub const IDENTITY: MeshTransform = MeshTransform {
        offset: DVec3::ZERO,
        rotation: DQuat::IDENTITY,
        scale: 1.0,
    };

    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.offset + self.rotation * (p * self.scale)
    }
}

impl Default for MeshTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
```

`crates/planora-exchange/src/progress.rs`:

```rust
//! Fortschritt und Abbrechen zwischen Oberfläche und Arbeitsthread.

use crate::error::ExchangeError;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Gemeinsamer Zustand: Abbruchwunsch und erledigter Anteil (`done` von `total`, etwa gelesene
/// Bytes oder fertige Körper).
#[derive(Debug, Default)]
pub struct Progress {
    cancelled: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
}

impl Progress {
    pub fn new() -> Progress {
        Progress::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// `Err(Cancelled)`, wenn abgebrochen werden soll.
    pub fn check(&self) -> Result<(), ExchangeError> {
        if self.is_cancelled() {
            Err(ExchangeError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
    }

    pub fn advance(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }

    /// Erledigter Anteil zwischen 0 und 1 (0, solange die Gesamtmenge unbekannt ist).
    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.done.load(Ordering::Relaxed) as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}
```

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange && cargo test -p planora-math && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (6 Tests in `basics`), keine Warnungen.

```bash
git add Cargo.toml Cargo.lock crates/planora-math/src/lib.rs crates/planora-exchange/Cargo.toml crates/planora-exchange/src/lib.rs crates/planora-exchange/src/error.rs crates/planora-exchange/src/limits.rs crates/planora-exchange/src/mesh.rs crates/planora-exchange/src/progress.rs crates/planora-exchange/tests/basics.rs
git commit -m "exchange: Paket mit Fehlern, Grenzen, Netztyp und Fortschritt

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Netzprüfung, Verschmelzen und einheitliche Quellen

**Files:**
- Create: `crates/planora-exchange/src/check.rs`, `crates/planora-exchange/src/source.rs`
- Modify: `crates/planora-exchange/src/lib.rs`, `crates/planora-exchange/Cargo.toml` (`[dev-dependencies]`)
- Test: `crates/planora-exchange/tests/check.rs`

**Interfaces:**
- Consumes: Task 1 (`ExchangeMesh`, `MeshTransform`, `ExchangeError`, `MAX_TRIANGLES`, `MAX_VERTICES`), `planora_mesh::Mesh`.
- Produces:
  ```rust
  pub struct MeshReport { pub triangles: usize, pub vertices: usize, pub open_edges: usize,
      pub non_manifold_edges: usize, pub flipped_edges: usize, pub degenerate_triangles: usize,
      pub closed: bool }                                     // Clone, Copy, Debug, Default, PartialEq, Eq
  impl MeshReport { pub fn combine(reports: &[MeshReport]) -> MeshReport; }
  pub fn check_mesh(mesh: &ExchangeMesh) -> MeshReport;
  pub fn weld_exact(positions: &[[f32; 3]], indices: &[u32]) -> ExchangeMesh;
  pub trait TriSource { fn name(&self) -> &str; fn vertex_count(&self) -> usize; fn vertex(&self, i: usize) -> DVec3;
      fn triangle_count(&self) -> usize; fn triangle(&self, i: usize) -> [u32; 3]; }
  pub struct SolidSource<'a> { pub name: &'a str, pub mesh: &'a planora_mesh::Mesh }
  pub struct PlacedSource<'a> { pub name: &'a str, pub mesh: &'a ExchangeMesh, pub transform: MeshTransform }
  pub struct ExportBody { pub name: String, pub mesh: ExchangeMesh }   // Clone, Debug, PartialEq
  pub fn collect_body(src: &dyn TriSource) -> Result<ExportBody, ExchangeError>;
  ```

- [ ] **Step 1: Testabhängigkeit eintragen**

In `crates/planora-exchange/Cargo.toml` nach `[dependencies]`-Block einfügen:

```toml
[dev-dependencies]
planora-kernel.workspace = true
```

- [ ] **Step 2: Tests schreiben** (`crates/planora-exchange/tests/check.rs`)

```rust
//! Netzprüfung, Verschmelzen und Quellen (Teilprojekt 9, Task 2).

use planora_exchange::*;
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DQuat, DVec3};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

#[test]
fn a_closed_cube_is_closed() {
    assert_eq!(
        check_mesh(&cube()),
        MeshReport {
            triangles: 12,
            vertices: 8,
            open_edges: 0,
            non_manifold_edges: 0,
            flipped_edges: 0,
            degenerate_triangles: 0,
            closed: true,
        }
    );
}

#[test]
fn a_missing_triangle_opens_three_edges() {
    let mut m = cube();
    m.indices.truncate(33);
    let r = check_mesh(&m);
    assert_eq!((r.triangles, r.open_edges, r.closed), (11, 3, false));
}

#[test]
fn a_flipped_triangle_is_reported_as_not_closed() {
    let mut m = cube();
    m.indices.swap(1, 2); // erstes Dreieck (0, 2, 1) → (0, 1, 2)
    let r = check_mesh(&m);
    assert_eq!((r.flipped_edges, r.open_edges), (3, 0));
    assert!(!r.closed);
}

#[test]
fn a_fin_is_non_manifold() {
    let mut m = cube();
    m.positions.push([0.5, -1.0, 0.0]); // Punkt 8
    m.indices.extend_from_slice(&[0, 1, 8]); // drittes Dreieck an der Kante 0–1
    let r = check_mesh(&m);
    assert_eq!((r.non_manifold_edges, r.open_edges), (1, 2));
    assert!(!r.closed);
}

#[test]
fn degenerate_triangles_are_counted_not_removed() {
    let mut m = cube();
    m.indices.extend_from_slice(&[0, 0, 1]); // doppelter Punkt
    m.positions.push([0.5, 0.0, 0.0]); // Punkt 8 auf der Kante 0–1
    m.indices.extend_from_slice(&[0, 8, 1]); // Fläche genau null
    let r = check_mesh(&m);
    assert_eq!((r.degenerate_triangles, r.triangles), (2, 14));
    assert!(!r.closed);
}

#[test]
fn an_empty_mesh_is_not_closed() {
    assert!(!check_mesh(&ExchangeMesh::default()).closed);
    let only_collapsed = ExchangeMesh {
        positions: vec![[0.0; 3]],
        indices: vec![0, 0, 0],
    };
    assert!(!check_mesh(&only_collapsed).closed);
}

#[test]
fn reports_combine() {
    let a = check_mesh(&cube());
    let mut open = cube();
    open.indices.truncate(33);
    let b = check_mesh(&open);
    let c = MeshReport::combine(&[a, b]);
    assert_eq!(
        (c.triangles, c.vertices, c.open_edges, c.closed),
        (23, 16, 3, false)
    );
    assert!(MeshReport::combine(&[a, a]).closed);
    assert!(!MeshReport::combine(&[]).closed);
}

#[test]
fn weld_merges_only_bit_identical_points() {
    // Zwei Dreiecke mit getrennten Punkten wie in STL; ein Punkt weicht um ein Bit ab, −0 ist +0.
    let x = 1.0f32;
    let x_next = f32::from_bits(x.to_bits() + 1);
    let positions = [
        [0.0, 0.0, 0.0],
        [x, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [x, 0.0, 0.0],
        [-0.0, 0.0, 0.0],
        [x_next, 1.0, 0.0],
        [9.0, 9.0, 9.0],
    ];
    let w = weld_exact(&positions, &[0, 1, 2, 3, 4, 5]);
    assert_eq!(
        w.positions,
        vec![
            [0.0, 0.0, 0.0],
            [x, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [x_next, 1.0, 0.0]
        ]
    );
    assert_eq!(w.indices, vec![0, 1, 2, 1, 0, 3]);
}

#[test]
fn a_triangle_soup_cube_becomes_closed_after_welding() {
    let c = cube();
    let soup: Vec<[f32; 3]> = c.indices.iter().map(|&i| c.positions[i as usize]).collect();
    let idx: Vec<u32> = (0..soup.len() as u32).collect();
    let unwelded = ExchangeMesh {
        positions: soup.clone(),
        indices: idx.clone(),
    };
    assert!(!check_mesh(&unwelded).closed);
    let w = weld_exact(&soup, &idx);
    assert_eq!(w.positions.len(), 8);
    assert!(check_mesh(&w).closed);
}

#[test]
fn a_kernel_box_becomes_a_closed_export_body() {
    let k = AnalyticKernel::default();
    let solid = k.make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let mesh = tessellate(&solid, &TessellationOptions::default()).unwrap();
    let body = collect_body(&SolidSource {
        name: "Quader 1",
        mesh: &mesh,
    })
    .unwrap();
    assert_eq!(body.name, "Quader 1");
    assert_eq!(body.mesh.triangle_count(), mesh.indices.len() / 3);
    assert!(
        body.mesh.positions.len() < mesh.positions.len(),
        "Flächen teilen ihre Ecken"
    );
    let r = check_mesh(&body.mesh);
    assert!(r.closed, "{r:?}");
    let v = body.mesh.mass(&MeshTransform::IDENTITY).volume;
    assert!((v - 6000.0).abs() < 1e-3, "{v}");
    assert!((v - mass_properties(&mesh).volume).abs() < 1e-3);
}

#[test]
fn a_placed_source_applies_the_transform() {
    let c = cube();
    let t = MeshTransform {
        offset: DVec3::new(5.0, 0.0, 0.0),
        rotation: DQuat::IDENTITY,
        scale: 3.0,
    };
    let body = collect_body(&PlacedSource {
        name: "Netz",
        mesh: &c,
        transform: t,
    })
    .unwrap();
    let v = body.mesh.mass(&MeshTransform::IDENTITY).volume;
    assert!((v - 27.0).abs() < 1e-4, "{v}");
    let min_x = body
        .mesh
        .positions
        .iter()
        .map(|p| p[0])
        .fold(f32::MAX, f32::min);
    assert_eq!(min_x, 5.0);
}

#[test]
fn unrepresentable_points_and_bad_indices_are_named() {
    let c = cube();
    let huge = MeshTransform {
        offset: DVec3::new(1e300, 0.0, 0.0),
        ..MeshTransform::IDENTITY
    };
    assert_eq!(
        collect_body(&PlacedSource {
            name: "x",
            mesh: &c,
            transform: huge
        }),
        Err(ExchangeError::NotFinite { line: None })
    );
    let mut bad = cube();
    bad.indices[5] = 99;
    assert!(matches!(
        collect_body(&PlacedSource {
            name: "x",
            mesh: &bad,
            transform: MeshTransform::IDENTITY
        }),
        Err(ExchangeError::Corrupt(_))
    ));
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test check`
Expected: FAIL – `cannot find function check_mesh`.

- [ ] **Step 4: Implementieren**

`crates/planora-exchange/src/check.rs`:

```rust
//! Prüfung eines Netzes (Spec §4) und Verschmelzen bitgleicher Punkte (P2, P3).

use crate::mesh::ExchangeMesh;
use planora_math::DVec3;

/// Ergebnis der Netzprüfung (Spec §4; `flipped_edges` ergänzt, Plan P2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshReport {
    pub triangles: usize,
    pub vertices: usize,
    /// Kanten, die nur zu einem Dreieck gehören.
    pub open_edges: usize,
    /// Kanten, die zu mehr als zwei Dreiecken gehören.
    pub non_manifold_edges: usize,
    /// Kanten zweier Dreiecke, die in derselben Richtung laufen (unstimmige Orientierung).
    pub flipped_edges: usize,
    /// Dreiecke mit doppeltem Punkt oder Flächeninhalt genau null (gezählt, nicht gelöscht).
    pub degenerate_triangles: usize,
    /// Jede Kante gehört zu genau zwei Dreiecken in entgegengesetzter Richtung.
    pub closed: bool,
}

impl MeshReport {
    /// Bericht mehrerer Körper: Zahlen addiert, geschlossen nur, wenn es alle sind (und es
    /// mindestens einen gibt).
    pub fn combine(reports: &[MeshReport]) -> MeshReport {
        let mut sum = MeshReport {
            closed: !reports.is_empty(),
            ..MeshReport::default()
        };
        for r in reports {
            sum.triangles += r.triangles;
            sum.vertices += r.vertices;
            sum.open_edges += r.open_edges;
            sum.non_manifold_edges += r.non_manifold_edges;
            sum.flipped_edges += r.flipped_edges;
            sum.degenerate_triangles += r.degenerate_triangles;
            sum.closed &= r.closed;
        }
        sum
    }
}

/// Prüft das Netz (Punktnummern müssen gültig sein, siehe [`ExchangeMesh::validate`]).
///
/// Jede Kante eines Dreiecks mit drei verschiedenen Punkten kommt in eine von zwei Listen:
/// „vorwärts“ (kleinere Nummer zuerst) oder „rückwärts“. Nach dem Sortieren zählt je Kante:
/// eins → offen, zwei in Gegenrichtung → in Ordnung, zwei in gleicher Richtung → falsch herum,
/// mehr → an mehr als zwei Dreiecken.
pub fn check_mesh(mesh: &ExchangeMesh) -> MeshReport {
    let tris = mesh.indices.as_chunks::<3>().0;
    let mut forward: Vec<u64> = Vec::new();
    let mut backward: Vec<u64> = Vec::new();
    let mut degenerate = 0;
    for t in tris {
        let [a, b, c] = *t;
        if a == b || b == c || a == c {
            degenerate += 1;
            continue;
        }
        let (pa, pb, pc) = (
            mesh.point(a as usize),
            mesh.point(b as usize),
            mesh.point(c as usize),
        );
        if (pb - pa).cross(pc - pa) == DVec3::ZERO {
            degenerate += 1;
        }
        for (p, q) in [(a, b), (b, c), (c, a)] {
            if p < q {
                forward.push(edge_key(p, q));
            } else {
                backward.push(edge_key(q, p));
            }
        }
    }
    forward.sort_unstable();
    backward.sort_unstable();
    let edges = forward.len() + backward.len();
    let (mut open, mut non_manifold, mut flipped) = (0, 0, 0);
    let (mut i, mut j) = (0, 0);
    loop {
        let key = match (forward.get(i), backward.get(j)) {
            (Some(&f), Some(&b)) => f.min(b),
            (Some(&f), None) => f,
            (None, Some(&b)) => b,
            (None, None) => break,
        };
        let nf = run_length(&forward, &mut i, key);
        let nb = run_length(&backward, &mut j, key);
        match (nf + nb, nf) {
            (1, _) => open += 1,
            (2, 1) => {}
            (2, _) => flipped += 1,
            _ => non_manifold += 1,
        }
    }
    MeshReport {
        triangles: tris.len(),
        vertices: mesh.positions.len(),
        open_edges: open,
        non_manifold_edges: non_manifold,
        flipped_edges: flipped,
        degenerate_triangles: degenerate,
        closed: edges > 0 && open == 0 && non_manifold == 0 && flipped == 0,
    }
}

fn edge_key(p: u32, q: u32) -> u64 {
    (u64::from(p) << 32) | u64::from(q)
}

/// Zählt die Einträge gleich `key` ab `*i` und rückt `*i` dahinter.
fn run_length(list: &[u64], i: &mut usize, key: u64) -> usize {
    let start = *i;
    while list.get(*i) == Some(&key) {
        *i += 1;
    }
    *i - start
}

/// Verschmilzt Punkte mit bitgleichen Koordinaten (−0 gilt als +0) und lässt Punkte weg, die
/// kein Dreieck benutzt (Plan P3). Die Punkte stehen danach in der Reihenfolge ihrer ersten
/// Verwendung; Dreiecke und ihre Reihenfolge bleiben. Kein Verschmelzen mit Toleranz (Spec §4).
/// `positions.len()` muss in `u32` passen (Aufrufer halten [`crate::MAX_VERTICES`] ein).
pub fn weld_exact(positions: &[[f32; 3]], indices: &[u32]) -> ExchangeMesh {
    let key = |i: u32| {
        positions[i as usize].map(|c| if c == 0.0 { 0u32 } else { c.to_bits() })
    };
    let mut order: Vec<u32> = (0..positions.len() as u32).collect();
    order.sort_unstable_by_key(|&i| (key(i), i));
    // Vertreter jeder Gruppe gleicher Punkte: die kleinste Nummer (dank der Sortierung vorn).
    let mut representative = vec![0u32; positions.len()];
    for group in order.chunk_by(|&a, &b| key(a) == key(b)) {
        for &i in group {
            representative[i as usize] = group[0];
        }
    }
    let mut new_id = vec![u32::MAX; positions.len()];
    let mut out = Vec::new();
    let mut out_indices = Vec::with_capacity(indices.len());
    for &i in indices {
        let r = representative[i as usize] as usize;
        if new_id[r] == u32::MAX {
            new_id[r] = out.len() as u32;
            out.push(positions[r]);
        }
        out_indices.push(new_id[r]);
    }
    ExchangeMesh {
        positions: out,
        indices: out_indices,
    }
}
```

`crates/planora-exchange/src/source.rs`:

```rust
//! Einheitliche Quelle für die Ausgabe (Spec §2, Plan P1): Netze von Kernel-Körpern und
//! Netz-Körper laufen über [`TriSource`] in ein [`ExportBody`].

use crate::check::weld_exact;
use crate::error::ExchangeError;
use crate::limits::{MAX_TRIANGLES, MAX_VERTICES};
use crate::mesh::{ExchangeMesh, MeshTransform};
use planora_math::DVec3;
use planora_mesh::Mesh;

/// Dreiecke mit Punkten in `f64` (Millimeter), schon nach der Körpertransformation.
pub trait TriSource {
    fn name(&self) -> &str;
    fn vertex_count(&self) -> usize;
    /// Punkt `i` (`i < vertex_count()`).
    fn vertex(&self, i: usize) -> DVec3;
    fn triangle_count(&self) -> usize;
    /// Punktnummern des Dreiecks `i` (`i < triangle_count()`).
    fn triangle(&self, i: usize) -> [u32; 3];
}

/// Netz eines Kernel-Körpers (aus `planora_mesh::tessellate`, Punkte schon an ihrer Lage).
pub struct SolidSource<'a> {
    pub name: &'a str,
    pub mesh: &'a Mesh,
}

impl TriSource for SolidSource<'_> {
    fn name(&self) -> &str {
        self.name
    }

    fn vertex_count(&self) -> usize {
        self.mesh.positions.len()
    }

    fn vertex(&self, i: usize) -> DVec3 {
        self.mesh.positions[i]
    }

    fn triangle_count(&self) -> usize {
        self.mesh.indices.len() / 3
    }

    fn triangle(&self, i: usize) -> [u32; 3] {
        self.mesh.indices.as_chunks::<3>().0[i]
    }
}

/// Netz-Körper: eingelesenes Netz mit seiner Lage.
pub struct PlacedSource<'a> {
    pub name: &'a str,
    pub mesh: &'a ExchangeMesh,
    pub transform: MeshTransform,
}

impl TriSource for PlacedSource<'_> {
    fn name(&self) -> &str {
        self.name
    }

    fn vertex_count(&self) -> usize {
        self.mesh.positions.len()
    }

    fn vertex(&self, i: usize) -> DVec3 {
        self.transform.apply(self.mesh.point(i))
    }

    fn triangle_count(&self) -> usize {
        self.mesh.triangle_count()
    }

    fn triangle(&self, i: usize) -> [u32; 3] {
        self.mesh.indices.as_chunks::<3>().0[i]
    }
}

/// Ein Körper, fertig für die Schreiber: Name und verschmolzenes Netz in `f32`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportBody {
    pub name: String,
    pub mesh: ExchangeMesh,
}

/// Liest eine Quelle in ein [`ExportBody`]: Punkte nach `f32` (nicht darstellbar → `NotFinite`),
/// Punktnummern geprüft, bitgleiche Punkte verschmolzen.
pub fn collect_body(src: &dyn TriSource) -> Result<ExportBody, ExchangeError> {
    let (n, t) = (src.vertex_count(), src.triangle_count());
    if t > MAX_TRIANGLES {
        return Err(ExchangeError::TooManyTriangles);
    }
    if n > MAX_VERTICES {
        return Err(ExchangeError::TooManyVertices);
    }
    let mut positions = Vec::with_capacity(n);
    for i in 0..n {
        let p = src.vertex(i);
        let q = [p.x as f32, p.y as f32, p.z as f32];
        if !q.iter().all(|c| c.is_finite()) {
            return Err(ExchangeError::NotFinite { line: None });
        }
        positions.push(q);
    }
    let mut indices = Vec::with_capacity(3 * t);
    for i in 0..t {
        let tri = src.triangle(i);
        if tri.iter().any(|&k| k as usize >= n) {
            return Err(ExchangeError::Corrupt(
                "Ein Dreieck verweist auf einen Punkt, den es nicht gibt".into(),
            ));
        }
        indices.extend_from_slice(&tri);
    }
    Ok(ExportBody {
        name: src.name().to_string(),
        mesh: weld_exact(&positions, &indices),
    })
}
```

`crates/planora-exchange/src/lib.rs` vollständig ersetzen:

```rust
//! Austausch von Dreiecksnetzen (Teilprojekt 9): STL, OBJ, 3MF und glTF schreiben, STL und OBJ
//! lesen, Netze prüfen. Keine neue Abhängigkeit: ZIP und CRC-32 sind selbst geschrieben,
//! Vielecke zerlegt das schon vorhandene `earcutr`.

mod check;
pub mod error;
pub mod limits;
mod mesh;
mod progress;
mod source;

pub use check::{MeshReport, check_mesh, weld_exact};
pub use error::ExchangeError;
pub use limits::{
    CHECK_EVERY, MAX_FILE_BYTES, MAX_LINE_BYTES, MAX_TRIANGLES, MAX_VERTICES, READ_BUFFER,
    RESERVE_STEP,
};
pub use mesh::{ExchangeMesh, MeshTransform};
pub use progress::Progress;
pub use source::{ExportBody, PlacedSource, SolidSource, TriSource, collect_body};
```

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (12 Tests in `check`), keine Warnungen.

```bash
git add Cargo.lock crates/planora-exchange/Cargo.toml crates/planora-exchange/src/lib.rs crates/planora-exchange/src/check.rs crates/planora-exchange/src/source.rs crates/planora-exchange/tests/check.rs
git commit -m "exchange: Netzprüfung, exaktes Verschmelzen und einheitliche Quellen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: STL schreiben und lesen (binär und Text), feindliche Dateien

**Files:**
- Create: `crates/planora-exchange/src/text.rs`, `crates/planora-exchange/src/stl.rs`
- Modify: `crates/planora-exchange/src/lib.rs`
- Test: `crates/planora-exchange/tests/stl.rs`

**Interfaces:**
- Consumes: Task 1, Task 2 (`weld_exact`, `ExportBody`, `check_mesh` in Tests).
- Produces:
  ```rust
  pub const STL_HEADER_BYTES: u64 = 84;  pub const STL_TRIANGLE_BYTES: u64 = 50;
  pub fn write_stl<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError>;
  pub fn read_stl<R: Read>(r: R, len: u64, progress: &Progress) -> Result<ExchangeMesh, ExchangeError>;
  // crate-intern (text.rs), von Task 4 benutzt:
  pub(crate) struct CountingReader<'a, R>;  impl CountingReader { pub(crate) fn new(inner: R, progress: &'a Progress) -> Self }
  pub(crate) struct LineReader<R>;  impl<R: BufRead> LineReader<R> { pub(crate) fn new(inner: R) -> Self;
      pub(crate) fn next_line(&mut self) -> Result<Option<(usize, &[u8])>, ExchangeError>; }
  pub(crate) fn three_numbers<'a>(tokens: &mut impl Iterator<Item = &'a str>, line: usize) -> Result<[f32; 3], ExchangeError>;
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-exchange/tests/stl.rs`)

```rust
//! STL schreiben und lesen, Erkennung Text/Binär, feindliche Dateien (Teilprojekt 9, Task 3).

use planora_exchange::*;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn body(name: &str, mesh: ExchangeMesh) -> ExportBody {
    ExportBody {
        name: name.into(),
        mesh,
    }
}

fn write(bodies: &[ExportBody]) -> Vec<u8> {
    let mut out = Vec::new();
    write_stl(&mut out, bodies).unwrap();
    out
}

fn read(bytes: &[u8]) -> Result<ExchangeMesh, ExchangeError> {
    read_stl(bytes, bytes.len() as u64, &Progress::new())
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

#[test]
fn binary_layout_matches_the_spec() {
    let bytes = write(&[body("Würfel", cube())]);
    assert_eq!(bytes.len() as u64, STL_HEADER_BYTES + STL_TRIANGLE_BYTES * 12);
    assert_eq!(&bytes[..7], b"Planora");
    assert!(bytes[7..80].iter().all(|&b| b == 0));
    assert_eq!(u32::from_le_bytes(bytes[80..84].try_into().unwrap()), 12);
    // erstes Dreieck (0, 2, 1): Normale −z, zweiter Punkt (1, 1, 0), Attribut 0
    assert_eq!(
        [f32_at(&bytes, 84), f32_at(&bytes, 88), f32_at(&bytes, 92)],
        [0.0, 0.0, -1.0]
    );
    assert_eq!(
        [f32_at(&bytes, 108), f32_at(&bytes, 112), f32_at(&bytes, 116)],
        [1.0, 1.0, 0.0]
    );
    assert_eq!(&bytes[132..134], &[0, 0]);
}

#[test]
fn several_bodies_go_into_one_file() {
    let mut moved = cube();
    for p in &mut moved.positions {
        p[0] += 5.0;
    }
    let bytes = write(&[body("A", cube()), body("B", moved)]);
    assert_eq!(u32::from_le_bytes(bytes[80..84].try_into().unwrap()), 24);
    let back = read(&bytes).unwrap();
    assert_eq!((back.triangle_count(), back.positions.len()), (24, 16));
}

#[test]
fn binary_round_trip_keeps_shape_and_closedness() {
    let back = read(&write(&[body("Würfel", cube())])).unwrap();
    assert_eq!((back.triangle_count(), back.positions.len()), (12, 8));
    let r = check_mesh(&back);
    assert!(r.closed, "{r:?}");
    let v = back.mass(&MeshTransform::IDENTITY);
    assert!((v.volume - 1.0).abs() < 1e-12 && (v.surface_area - 6.0).abs() < 1e-12);
}

#[test]
fn writing_nothing_is_an_error() {
    let mut out = Vec::new();
    assert_eq!(write_stl(&mut out, &[]), Err(ExchangeError::Empty));
    assert_eq!(
        write_stl(&mut out, &[body("leer", ExchangeMesh::default())]),
        Err(ExchangeError::Empty)
    );
}

/// Würfel als STL-Text mit dem Zeilenende `newline`.
fn ascii_cube(newline: &str) -> String {
    let c = cube();
    let mut s = String::from("solid würfel");
    s.push_str(newline);
    for t in c.indices.as_chunks::<3>().0 {
        for line in ["  facet normal 0 0 0", "    outer loop"] {
            s.push_str(line);
            s.push_str(newline);
        }
        for &i in t {
            let p = c.positions[i as usize];
            s.push_str(&format!("      vertex {} {} {}", p[0], p[1], p[2]));
            s.push_str(newline);
        }
        for line in ["    endloop", "  endfacet"] {
            s.push_str(line);
            s.push_str(newline);
        }
    }
    s.push_str("endsolid würfel");
    s.push_str(newline);
    s
}

#[test]
fn ascii_files_are_read_with_all_line_endings_and_a_bom() {
    for nl in ["\n", "\r\n", "\r"] {
        let text = ascii_cube(nl);
        let m = read(text.as_bytes()).unwrap();
        assert_eq!((m.triangle_count(), m.positions.len()), (12, 8), "{nl:?}");
        assert!(check_mesh(&m).closed);
        let with_bom = format!("\u{feff}{text}");
        assert_eq!(read(with_bom.as_bytes()).unwrap().triangle_count(), 12);
        let upper = text.replace("solid", "SOLID");
        assert_eq!(read(upper.as_bytes()).unwrap().triangle_count(), 12);
    }
}

#[test]
fn several_solid_blocks_are_merged() {
    let one = ascii_cube("\n");
    let m = read(format!("{one}{one}").as_bytes()).unwrap();
    assert_eq!((m.triangle_count(), m.positions.len()), (24, 8));
}

#[test]
fn a_binary_file_whose_header_starts_with_solid_is_read_as_binary() {
    let mut bytes = write(&[body("Würfel", cube())]);
    bytes[..7].copy_from_slice(b"solid x");
    assert_eq!(read(&bytes).unwrap().triangle_count(), 12);
}

#[test]
fn hostile_binary_files_get_named_errors() {
    let good = write(&[body("Würfel", cube())]);
    // abgeschnitten, Größe ehrlich: Kopfzahl größer als die Datei hergibt
    let cut = &good[..good.len() - 10];
    assert_eq!(
        read(cut),
        Err(ExchangeError::CountTooLarge {
            claimed: 12,
            possible: 11
        })
    );
    // abgeschnitten, Größenangabe falsch: endet zu früh
    assert_eq!(
        read_stl(cut, good.len() as u64, &Progress::new()),
        Err(ExchangeError::Truncated)
    );
    // kürzer als der Kopf
    assert_eq!(read(&good[..50]), Err(ExchangeError::Truncated));
    // Kopfzahl über der Grenze
    let mut many = good.clone();
    many[80..84].copy_from_slice(&(MAX_TRIANGLES as u32 + 1).to_le_bytes());
    assert_eq!(read(&many), Err(ExchangeError::TooManyTriangles));
    // Datei über 8 GB
    assert_eq!(
        read_stl(&good[..], MAX_FILE_BYTES + 1, &Progress::new()),
        Err(ExchangeError::FileTooLarge {
            bytes: MAX_FILE_BYTES + 1
        })
    );
    // NaN und unendlich im ersten Punkt
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut b = good.clone();
        b[96..100].copy_from_slice(&bad.to_le_bytes());
        assert_eq!(read(&b), Err(ExchangeError::NotFinite { line: None }));
    }
    // keine Dreiecke
    let mut empty = good[..84].to_vec();
    empty[80..84].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(read(&empty), Err(ExchangeError::Empty));
}

#[test]
fn hostile_text_files_get_named_errors_with_line_numbers() {
    let three = "Ein Dreieck braucht genau drei Punkte";
    let cases: [(&str, ExchangeError); 8] = [
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nendloop\nendfacet\nendsolid\n",
            ExchangeError::Syntax { line: 7, msg: three },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nvertex 1 1 0\n",
            ExchangeError::Syntax { line: 7, msg: three },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 nan\n",
            ExchangeError::NotFinite { line: Some(4) },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 1e39\n",
            ExchangeError::NotFinite { line: Some(4) },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 zwei 0\n",
            ExchangeError::BadNumber { line: 4 },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0\n",
            ExchangeError::Syntax {
                line: 4,
                msg: "Es fehlt eine Zahl",
            },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\n",
            ExchangeError::Truncated,
        ),
        (
            "solid x\r\nhallo welt\r\n",
            ExchangeError::Syntax {
                line: 2,
                msg: "Unbekanntes Wort in der STL-Datei",
            },
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(read(text.as_bytes()), Err(expected), "{text:?}");
    }
    assert_eq!(
        read(b"solid leer\nendsolid leer\n"),
        Err(ExchangeError::Empty)
    );
}

#[test]
fn an_endless_line_is_refused() {
    let mut text = b"solid x\n".to_vec();
    text.extend(std::iter::repeat_n(b'7', MAX_LINE_BYTES + 10));
    assert_eq!(
        read(&text),
        Err(ExchangeError::Syntax {
            line: 2,
            msg: "Die Zeile ist zu lang"
        })
    );
}

#[test]
fn reading_stops_when_cancelled_and_counts_its_bytes() {
    let bytes = write(&[body("Würfel", cube())]);
    let p = Progress::new();
    p.cancel();
    assert_eq!(
        read_stl(&bytes[..], bytes.len() as u64, &p),
        Err(ExchangeError::Cancelled)
    );
    let p = Progress::new();
    read_stl(&bytes[..], bytes.len() as u64, &p).unwrap();
    assert_eq!(p.fraction(), 1.0);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test stl`
Expected: FAIL – `cannot find function write_stl`.

- [ ] **Step 3: Implementieren**

`crates/planora-exchange/src/text.rs`:

```rust
//! Gemeinsames für das Lesen: Zeilen mit allen Zeilenenden (P4), Byte-Zähler für den
//! Fortschritt, Zahlen mit Zeilennummer.

use crate::error::ExchangeError;
use crate::limits::MAX_LINE_BYTES;
use crate::progress::Progress;
use std::io::{BufRead, Read};

/// Zählt gelesene Bytes in `progress`.
pub(crate) struct CountingReader<'a, R> {
    inner: R,
    progress: &'a Progress,
}

impl<'a, R> CountingReader<'a, R> {
    pub(crate) fn new(inner: R, progress: &'a Progress) -> Self {
        Self { inner, progress }
    }
}

impl<R: Read> Read for CountingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.progress.advance(n as u64);
        Ok(n)
    }
}

/// Liest Zeilen mit `\n`, `\r\n` oder `\r` als Ende; ein UTF-8-BOM am Dateianfang entfällt.
/// Zeilen über [`MAX_LINE_BYTES`] sind ein Fehler.
pub(crate) struct LineReader<R> {
    inner: R,
    buf: Vec<u8>,
    line: usize,
    /// Das letzte Zeilenende war `\r`; ein unmittelbar folgendes `\n` gehört noch dazu.
    after_cr: bool,
    done: bool,
}

impl<R: BufRead> LineReader<R> {
    pub(crate) fn new(inner: R) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            line: 0,
            after_cr: false,
            done: false,
        }
    }

    /// Nächste Zeile: Nummer (ab 1) und Inhalt ohne Zeilenende; `None` am Dateiende.
    pub(crate) fn next_line(&mut self) -> Result<Option<(usize, &[u8])>, ExchangeError> {
        if self.done {
            return Ok(None);
        }
        self.buf.clear();
        loop {
            let chunk = match self.inner.fill_buf() {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            };
            if chunk.is_empty() {
                self.done = true;
                if self.buf.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(self.finish_line()));
            }
            let start = usize::from(std::mem::take(&mut self.after_cr) && chunk[0] == b'\n');
            let rest = &chunk[start..];
            let end = rest.iter().position(|&b| b == b'\n' || b == b'\r');
            let take = end.unwrap_or(rest.len());
            if self.buf.len() + take > MAX_LINE_BYTES {
                return Err(ExchangeError::Syntax {
                    line: self.line + 1,
                    msg: "Die Zeile ist zu lang",
                });
            }
            self.buf.extend_from_slice(&rest[..take]);
            match end {
                Some(i) => {
                    self.after_cr = rest[i] == b'\r';
                    self.inner.consume(start + i + 1);
                    return Ok(Some(self.finish_line()));
                }
                None => {
                    let used = chunk.len();
                    self.inner.consume(used);
                }
            }
        }
    }

    fn finish_line(&mut self) -> (usize, &[u8]) {
        self.line += 1;
        if self.line == 1 && self.buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.buf.drain(..3);
        }
        (self.line, &self.buf)
    }
}

/// Liest drei Zahlen: fehlende → `Syntax`, unlesbare → `BadNumber`, unendliche oder NaN →
/// `NotFinite`, jeweils mit Zeile.
pub(crate) fn three_numbers<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
    line: usize,
) -> Result<[f32; 3], ExchangeError> {
    let mut out = [0.0f32; 3];
    for v in &mut out {
        let token = tokens.next().ok_or(ExchangeError::Syntax {
            line,
            msg: "Es fehlt eine Zahl",
        })?;
        let x: f32 = token
            .parse()
            .map_err(|_| ExchangeError::BadNumber { line })?;
        if !x.is_finite() {
            return Err(ExchangeError::NotFinite { line: Some(line) });
        }
        *v = x;
    }
    Ok(out)
}
```

`crates/planora-exchange/src/stl.rs`:

```rust
//! STL: binär schreiben; binär und als Text lesen (Spec §3, Plan P4, P5).

use crate::check::weld_exact;
use crate::error::ExchangeError;
use crate::limits::{CHECK_EVERY, MAX_FILE_BYTES, MAX_TRIANGLES, READ_BUFFER, RESERVE_STEP};
use crate::mesh::ExchangeMesh;
use crate::progress::Progress;
use crate::source::ExportBody;
use crate::text::{CountingReader, LineReader, three_numbers};
use planora_math::DVec3;
use std::io::{BufRead, BufReader, Read, Write};

/// Länge des Kopfs einer binären STL-Datei (80 Bytes Text, 4 Bytes Dreieckszahl).
pub const STL_HEADER_BYTES: u64 = 84;
/// Bytes je Dreieck einer binären STL-Datei (Normale, drei Punkte, Attribut).
pub const STL_TRIANGLE_BYTES: u64 = 50;
/// Anfang des Kopftexts geschriebener Dateien (Spec §3); der Rest des Kopfs sind Nullen.
const HEADER_TEXT: &[u8] = b"Planora";
/// UTF-8-Kennung am Anfang mancher Textdateien.
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Schreibt alle Körper als ein binäres STL (Dreiecke aller Körper hintereinander).
pub fn write_stl<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
    let total: usize = bodies.iter().map(|b| b.mesh.triangle_count()).sum();
    if total == 0 {
        return Err(ExchangeError::Empty);
    }
    let count =
        u32::try_from(total).map_err(|_| ExchangeError::TooLargeForFormat { format: "STL" })?;
    let mut header = [0u8; 80];
    header[..HEADER_TEXT.len()].copy_from_slice(HEADER_TEXT);
    w.write_all(&header)?;
    w.write_all(&count.to_le_bytes())?;
    // Normale, drei Punkte, Attribut (bleibt 0)
    let mut record = [0u8; STL_TRIANGLE_BYTES as usize];
    for body in bodies {
        let m = &body.mesh;
        for t in m.indices.as_chunks::<3>().0 {
            let p = t.map(|i| m.positions[i as usize]);
            for (k, v) in [normal(p), p[0], p[1], p[2]].iter().enumerate() {
                for (c, x) in v.iter().enumerate() {
                    let at = 12 * k + 4 * c;
                    record[at..at + 4].copy_from_slice(&x.to_le_bytes());
                }
            }
            w.write_all(&record)?;
        }
    }
    Ok(())
}

/// Einheitsnormale eines Dreiecks aus seinen Punkten (null bei Fläche null).
fn normal(p: [[f32; 3]; 3]) -> [f32; 3] {
    let d = p.map(|q| DVec3::new(f64::from(q[0]), f64::from(q[1]), f64::from(q[2])));
    let n = (d[1] - d[0]).cross(d[2] - d[0]).normalize_or_zero();
    [n.x as f32, n.y as f32, n.z as f32]
}

/// Liest STL, binär oder als Text (Erkennung nach P5). `len` ist die Dateigröße in Bytes (für
/// die Erkennung und die Grenzen); `progress` zählt gelesene Bytes und kann abbrechen. Gleiche
/// Punkte werden verschmolzen (P3).
pub fn read_stl<R: Read>(
    r: R,
    len: u64,
    progress: &Progress,
) -> Result<ExchangeMesh, ExchangeError> {
    if len > MAX_FILE_BYTES {
        return Err(ExchangeError::FileTooLarge { bytes: len });
    }
    progress.set_total(len);
    progress.check()?;
    let mut r = BufReader::with_capacity(READ_BUFFER, CountingReader::new(r, progress));
    let mut head = [0u8; STL_HEADER_BYTES as usize];
    let got = read_up_to(&mut r, &mut head)?;
    if looks_like_text(&head[..got], len) {
        read_text((&head[..got]).chain(r), progress)
    } else if got < head.len() {
        Err(ExchangeError::Truncated)
    } else {
        read_binary(r, &head, len, progress)
    }
}

/// Liest höchstens `buf.len()` Bytes; liefert, wie viele es waren.
fn read_up_to<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<usize, ExchangeError> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(got)
}

/// Text, wenn die Datei (nach BOM und Leerraum) mit `solid` beginnt und ihre Größe nicht zur
/// Dreieckszahl im Binärkopf passt (Spec §3).
fn looks_like_text(head: &[u8], len: u64) -> bool {
    let body = head.strip_prefix(&BOM).unwrap_or(head);
    let start = body
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(body.len());
    let body = &body[start..];
    let solid = body.len() >= 5 && body[..5].eq_ignore_ascii_case(b"solid");
    let binary_fits =
        head.len() == STL_HEADER_BYTES as usize && STL_HEADER_BYTES + STL_TRIANGLE_BYTES * claimed(head) == len;
    solid && !binary_fits
}

/// Dreieckszahl aus dem Binärkopf (Bytes 80 bis 83).
fn claimed(head: &[u8]) -> u64 {
    u64::from(u32::from_le_bytes([head[80], head[81], head[82], head[83]]))
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn read_binary<R: Read>(
    mut r: R,
    head: &[u8],
    len: u64,
    progress: &Progress,
) -> Result<ExchangeMesh, ExchangeError> {
    let claimed = claimed(head);
    if claimed > MAX_TRIANGLES as u64 {
        return Err(ExchangeError::TooManyTriangles);
    }
    let possible = len.saturating_sub(STL_HEADER_BYTES) / STL_TRIANGLE_BYTES;
    if claimed > possible {
        return Err(ExchangeError::CountTooLarge { claimed, possible });
    }
    if claimed == 0 {
        return Err(ExchangeError::Empty);
    }
    // Reserviert wird nur ein Schritt; danach wächst der Speicher mit dem Gelesenen (P4).
    let mut points: Vec<[f32; 3]> = Vec::with_capacity(3 * (claimed as usize).min(RESERVE_STEP));
    let mut record = [0u8; STL_TRIANGLE_BYTES as usize];
    for i in 0..claimed {
        if i.is_multiple_of(CHECK_EVERY) {
            progress.check()?;
        }
        r.read_exact(&mut record)?;
        for k in 0..3 {
            let at = 12 + 12 * k;
            let p = [
                f32_at(&record, at),
                f32_at(&record, at + 4),
                f32_at(&record, at + 8),
            ];
            if !p.iter().all(|c| c.is_finite()) {
                return Err(ExchangeError::NotFinite { line: None });
            }
            points.push(p);
        }
    }
    let indices: Vec<u32> = (0..points.len() as u32).collect();
    Ok(weld_exact(&points, &indices))
}

fn read_text<R: BufRead>(r: R, progress: &Progress) -> Result<ExchangeMesh, ExchangeError> {
    const THREE: &str = "Ein Dreieck braucht genau drei Punkte";
    let mut lines = LineReader::new(r);
    let mut points: Vec<[f32; 3]> = Vec::new();
    let (mut in_facet, mut corners, mut triangles) = (false, 0usize, 0usize);
    while let Some((line, raw)) = lines.next_line()? {
        if (line as u64).is_multiple_of(CHECK_EVERY) {
            progress.check()?;
        }
        let text = String::from_utf8_lossy(raw);
        let mut tokens = text.split_whitespace();
        let Some(word) = tokens.next() else {
            continue;
        };
        match word.to_ascii_lowercase().as_str() {
            "solid" | "endsolid" | "outer" | "endloop" => {}
            "facet" => {
                if in_facet {
                    return Err(ExchangeError::Syntax {
                        line,
                        msg: "Ein Dreieck beginnt, bevor das vorige endet",
                    });
                }
                in_facet = true;
                corners = 0;
            }
            "vertex" => {
                if !in_facet || corners == 3 {
                    return Err(ExchangeError::Syntax { line, msg: THREE });
                }
                points.push(three_numbers(&mut tokens, line)?);
                corners += 1;
            }
            "endfacet" => {
                if !in_facet || corners != 3 {
                    return Err(ExchangeError::Syntax { line, msg: THREE });
                }
                in_facet = false;
                triangles += 1;
                if triangles > MAX_TRIANGLES {
                    return Err(ExchangeError::TooManyTriangles);
                }
            }
            _ => {
                return Err(ExchangeError::Syntax {
                    line,
                    msg: "Unbekanntes Wort in der STL-Datei",
                });
            }
        }
    }
    if in_facet {
        return Err(ExchangeError::Truncated);
    }
    if triangles == 0 {
        return Err(ExchangeError::Empty);
    }
    let indices: Vec<u32> = (0..points.len() as u32).collect();
    Ok(weld_exact(&points, &indices))
}
```

Hinweis zum zweiten Textfall des Tests: Die vierte `vertex`-Zeile (Zeile 7) trifft auf `corners == 3` und liefert den `Syntax`-Fehler mit `THREE`.

`crates/planora-exchange/src/lib.rs` vollständig ersetzen:

```rust
//! Austausch von Dreiecksnetzen (Teilprojekt 9): STL, OBJ, 3MF und glTF schreiben, STL und OBJ
//! lesen, Netze prüfen. Keine neue Abhängigkeit: ZIP und CRC-32 sind selbst geschrieben,
//! Vielecke zerlegt das schon vorhandene `earcutr`.

mod check;
pub mod error;
pub mod limits;
mod mesh;
mod progress;
mod source;
mod stl;
mod text;

pub use check::{MeshReport, check_mesh, weld_exact};
pub use error::ExchangeError;
pub use limits::{
    CHECK_EVERY, MAX_FILE_BYTES, MAX_LINE_BYTES, MAX_TRIANGLES, MAX_VERTICES, READ_BUFFER,
    RESERVE_STEP,
};
pub use mesh::{ExchangeMesh, MeshTransform};
pub use progress::Progress;
pub use source::{ExportBody, PlacedSource, SolidSource, TriSource, collect_body};
pub use stl::{STL_HEADER_BYTES, STL_TRIANGLE_BYTES, read_stl, write_stl};
```

- [ ] **Step 4: Tests prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange`
Expected: PASS (11 Tests in `stl`, alle früheren grün).

- [ ] **Step 5: Prüfungen und Commit**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`

```bash
git add crates/planora-exchange/src/lib.rs crates/planora-exchange/src/text.rs crates/planora-exchange/src/stl.rs crates/planora-exchange/tests/stl.rs
git commit -m "exchange: STL binär schreiben, binär und als Text lesen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: OBJ schreiben und lesen (Vielecke, negative Nummern)

**Files:**
- Create: `crates/planora-exchange/src/obj.rs`
- Modify: `crates/planora-exchange/src/lib.rs`, `crates/planora-exchange/Cargo.toml` (`earcutr`)
- Test: `crates/planora-exchange/tests/obj.rs`

**Interfaces:**
- Consumes: Task 1–3 (`LineReader`, `CountingReader`, `three_numbers`, `weld_exact`, `ExportBody`).
- Produces:
  ```rust
  pub fn write_obj<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError>;
  pub fn read_obj<R: Read>(r: R, len: u64, progress: &Progress) -> Result<ExchangeMesh, ExchangeError>;
  ```

- [ ] **Step 1: Abhängigkeit eintragen**

In `crates/planora-exchange/Cargo.toml` unter `[dependencies]` nach `planora-mesh.workspace = true` einfügen (schon Abhängigkeit von `planora-mesh`, Lizenz ISC, kein neues Paket):

```toml
earcutr.workspace = true
```

- [ ] **Step 2: Tests schreiben** (`crates/planora-exchange/tests/obj.rs`)

```rust
//! OBJ schreiben und lesen, Vielecke, feindliche Dateien (Teilprojekt 9, Task 4).

use planora_exchange::*;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn body(name: &str, mesh: ExchangeMesh) -> ExportBody {
    ExportBody {
        name: name.into(),
        mesh,
    }
}

fn write(bodies: &[ExportBody]) -> String {
    let mut out = Vec::new();
    write_obj(&mut out, bodies).unwrap();
    String::from_utf8(out).unwrap()
}

fn read(text: &str) -> Result<ExchangeMesh, ExchangeError> {
    read_obj(text.as_bytes(), text.len() as u64, &Progress::new())
}

/// Summe der z-Komponenten der Flächennormalen (= doppelte Fläche in der xy-Ebene) und ob
/// alle Dreiecke nach +z zeigen.
fn signed_area_xy(m: &ExchangeMesh) -> (f64, bool) {
    let mut area = 0.0;
    let mut all_up = true;
    for t in m.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| m.point(i as usize));
        let z = (b - a).cross(c - a).z;
        area += z / 2.0;
        all_up &= z > 0.0;
    }
    (area, all_up)
}

#[test]
fn the_written_text_has_objects_points_normals_and_faces() {
    let text = write(&[body("Würfel 1", cube())]);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "# PlanoraCAD");
    assert_eq!(lines[1], "o Würfel 1");
    assert_eq!(lines[2], "v 0 0 0");
    assert_eq!(lines.iter().filter(|l| l.starts_with("v ")).count(), 8);
    assert_eq!(lines.iter().filter(|l| l.starts_with("vn ")).count(), 8);
    assert_eq!(lines.iter().filter(|l| l.starts_with("f ")).count(), 12);
    assert!(lines.contains(&"f 1//1 3//3 2//2"), "{text}");
    // Ecke (1, 1, 1): gemittelte Normale zeigt schräg nach außen
    let vn = lines.iter().filter(|l| l.starts_with("vn ")).nth(6).unwrap();
    let n: Vec<f32> = vn[3..].split(' ').map(|x| x.parse().unwrap()).collect();
    assert!(
        n.iter().all(|&c| (c - 1.0 / 3f32.sqrt()).abs() < 1e-6),
        "{vn}"
    );
}

#[test]
fn several_bodies_continue_the_numbering_and_names_stay_on_one_line() {
    let text = write(&[body("A", cube()), body("B\nzwei", cube())]);
    assert!(text.contains("o A\n") && text.contains("o B zwei\n"), "{text}");
    assert!(text.contains("f 9//9 11//11 10//10"));
    assert_eq!(read(&text).unwrap().triangle_count(), 24);
    let mut out = Vec::new();
    assert_eq!(write_obj(&mut out, &[]), Err(ExchangeError::Empty));
}

#[test]
fn round_trip_is_exact() {
    let mut c = cube();
    for p in &mut c.positions {
        p[0] = p[0] * 0.1 + 1234.5678;
        p[2] *= -3.3;
    }
    let back = read(&write(&[body("x", c.clone())])).unwrap();
    assert_eq!(back, weld_exact(&c.positions, &c.indices));
    assert!(check_mesh(&back).closed);
}

#[test]
fn all_corner_forms_and_negative_indices_are_read() {
    let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\ng gruppe\ns 1\nusemtl stahl\n\
                f 1 2 3\nf 1/1 3/1 4/1\nf -4//1 -2//1 -1//1\nf 1/1/1 2/1/1 3/1/1\n";
    let m = read(text).unwrap();
    assert_eq!(m.indices, vec![0, 1, 2, 0, 2, 3, 0, 2, 3, 0, 1, 2]);
}

#[test]
fn a_convex_quad_becomes_a_fan_and_a_concave_polygon_is_cut_by_ear_clipping() {
    let quad = read("v 0 0 0\nv 2 0 0\nv 2 1 0\nv 0 1 0\nf 1 2 3 4\n").unwrap();
    assert_eq!(quad.indices, vec![0, 1, 2, 0, 2, 3]);
    // U-Form gegen den Uhrzeigersinn (Fläche 3 · 2 − 1 · 1 = 5); ein Fächer von Punkt 1 aus
    // ergäbe falsch herum liegende Dreiecke.
    let u = read(
        "v 0 0 0\nv 3 0 0\nv 3 2 0\nv 2 2 0\nv 2 1 0\nv 1 1 0\nv 1 2 0\nv 0 2 0\nf 1 2 3 4 5 6 7 8\n",
    )
    .unwrap();
    assert_eq!(u.triangle_count(), 6);
    let (area, all_up) = signed_area_xy(&u);
    assert!((area - 5.0).abs() < 1e-12, "Fläche {area}");
    assert!(all_up, "jedes Dreieck behält die Orientierung des Vielecks");
    // dieselbe U-Form im Uhrzeigersinn: alle Dreiecke zeigen nach −z
    let cw = read(
        "v 0 0 0\nv 0 2 0\nv 1 2 0\nv 1 1 0\nv 2 1 0\nv 2 2 0\nv 3 2 0\nv 3 0 0\nf 1 2 3 4 5 6 7 8\n",
    )
    .unwrap();
    let (area, _) = signed_area_xy(&cw);
    assert!((area + 5.0).abs() < 1e-12, "Fläche {area}");
}

#[test]
fn hostile_obj_files_get_named_errors() {
    let cases: [(&str, ExchangeError); 9] = [
        ("v 0 0 0\nv 1 0 0\nf 1 2 3\n", ExchangeError::IndexOutOfRange { line: 3 }),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 0\n",
            ExchangeError::IndexOutOfRange { line: 4 },
        ),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -4 2 3\n",
            ExchangeError::IndexOutOfRange { line: 4 },
        ),
        (
            "f 1 2 3\nv 0 0 0\nv 1 0 0\nv 0 1 0\n",
            ExchangeError::IndexOutOfRange { line: 1 },
        ),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 99999999999999999999\n",
            ExchangeError::Syntax {
                line: 4,
                msg: "Eine Ecke hat keine gültige Punktnummer",
            },
        ),
        ("v 0 0 1e400\n", ExchangeError::NotFinite { line: Some(1) }),
        ("v 0 0 nan\n", ExchangeError::NotFinite { line: Some(1) }),
        ("v 0 0 x\n", ExchangeError::BadNumber { line: 1 }),
        (
            "v 0 0 0\nv 1 0 0\nf 1 2\n",
            ExchangeError::Syntax {
                line: 3,
                msg: "Eine Fläche braucht mindestens drei Ecken",
            },
        ),
    ];
    for (text, e) in cases {
        assert_eq!(read(text), Err(e), "{text:?}");
    }
    assert_eq!(read("# nur ein Kommentar\nv 0 0 0\n"), Err(ExchangeError::Empty));
    assert_eq!(
        read_obj(&b""[..], MAX_FILE_BYTES + 1, &Progress::new()),
        Err(ExchangeError::FileTooLarge {
            bytes: MAX_FILE_BYTES + 1
        })
    );
    // Vieleck ohne Fläche (alle Punkte auf einer Geraden): benannt
    assert_eq!(
        read("v 0 0 0\nv 1 0 0\nv 2 0 0\nv 1 0 0\nf 1 2 3 4\n"),
        Err(ExchangeError::Polygon { line: 5 })
    );
}

#[test]
fn line_endings_bom_tabs_and_foreign_comment_bytes_are_fine() {
    let base = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nf 1 3 2\nf 1 2 4\nf 1 4 3\nf 2 3 4\n";
    for nl in ["\n", "\r\n", "\r"] {
        let text = format!("\u{feff}# Kommentar\n{}", base.replace('\n', nl)).replace("v 1", "v\t1");
        let m = read(&text).unwrap();
        assert_eq!(m.triangle_count(), 4, "{nl:?}");
        assert!(check_mesh(&m).closed);
    }
    let latin1 = b"# Gr\xfc\xdfe\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec();
    assert_eq!(
        read_obj(&latin1[..], latin1.len() as u64, &Progress::new())
            .unwrap()
            .triangle_count(),
        1
    );
}

#[test]
fn reading_stops_when_cancelled() {
    let p = Progress::new();
    p.cancel();
    let text = write(&[body("x", cube())]);
    assert_eq!(
        read_obj(text.as_bytes(), text.len() as u64, &p),
        Err(ExchangeError::Cancelled)
    );
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test obj`
Expected: FAIL – `cannot find function write_obj`.

- [ ] **Step 4: Implementieren**

`crates/planora-exchange/src/obj.rs`:

```rust
//! OBJ: Text schreiben (`o`, `v`, `vn`, `f a//n`); lesen mit Dreiecken und Vielecken (Spec §3,
//! Plan P6).

use crate::check::weld_exact;
use crate::error::ExchangeError;
use crate::limits::{CHECK_EVERY, MAX_FILE_BYTES, MAX_TRIANGLES, MAX_VERTICES, READ_BUFFER};
use crate::mesh::ExchangeMesh;
use crate::progress::Progress;
use crate::source::ExportBody;
use crate::text::{CountingReader, LineReader, three_numbers};
use planora_math::DVec3;
use std::cmp::Ordering;
use std::io::{BufReader, Read, Write};

/// Schreibt alle Körper als OBJ: je Körper `o Name`, Punkte, Normalen je Punkt und Dreiecke
/// (Nummern über alle Körper fortlaufend, ab 1).
pub fn write_obj<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
    if bodies.iter().all(|b| b.mesh.triangle_count() == 0) {
        return Err(ExchangeError::Empty);
    }
    writeln!(w, "# PlanoraCAD")?;
    let mut base: u64 = 1;
    for body in bodies.iter().filter(|b| b.mesh.triangle_count() > 0) {
        let m = &body.mesh;
        writeln!(w, "o {}", object_name(&body.name))?;
        for p in &m.positions {
            writeln!(w, "v {} {} {}", p[0], p[1], p[2])?;
        }
        for n in vertex_normals(m) {
            writeln!(w, "vn {} {} {}", n[0], n[1], n[2])?;
        }
        for t in m.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| base + u64::from(i));
            writeln!(w, "f {a}//{a} {b}//{b} {c}//{c}")?;
        }
        base += m.positions.len() as u64;
    }
    Ok(())
}

/// Name auf einer Zeile: Steuerzeichen werden Leerzeichen; leer → „Körper“.
fn object_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        "Körper".into()
    } else {
        clean.to_string()
    }
}

/// Normale je Punkt: Summe der Flächennormalen der anliegenden Dreiecke (nach Fläche gewichtet),
/// normiert; ohne Fläche (0, 0, 1).
fn vertex_normals(m: &ExchangeMesh) -> Vec<[f32; 3]> {
    let mut sum = vec![DVec3::ZERO; m.positions.len()];
    for t in m.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| m.point(i as usize));
        let n = (b - a).cross(c - a);
        for &i in t {
            sum[i as usize] += n;
        }
    }
    sum.into_iter()
        .map(|n| {
            let u = n.try_normalize().unwrap_or(DVec3::Z);
            [u.x as f32, u.y as f32, u.z as f32]
        })
        .collect()
}

/// Liest OBJ: `v` und `f` (Ecken `v`, `v/vt`, `v//vn`, `v/vt/vn`, auch negative Nummern);
/// alles Übrige wird übergangen. Gleiche Punkte werden verschmolzen (P3).
pub fn read_obj<R: Read>(
    r: R,
    len: u64,
    progress: &Progress,
) -> Result<ExchangeMesh, ExchangeError> {
    if len > MAX_FILE_BYTES {
        return Err(ExchangeError::FileTooLarge { bytes: len });
    }
    progress.set_total(len);
    progress.check()?;
    let mut lines = LineReader::new(BufReader::with_capacity(
        READ_BUFFER,
        CountingReader::new(r, progress),
    ));
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut corners: Vec<u32> = Vec::new();
    while let Some((line, raw)) = lines.next_line()? {
        if (line as u64).is_multiple_of(CHECK_EVERY) {
            progress.check()?;
        }
        let text = String::from_utf8_lossy(raw);
        let mut tokens = text.split_whitespace();
        match tokens.next() {
            Some("v") => {
                if positions.len() == MAX_VERTICES {
                    return Err(ExchangeError::TooManyVertices);
                }
                positions.push(three_numbers(&mut tokens, line)?);
            }
            Some("f") => {
                corners.clear();
                for token in tokens {
                    corners.push(corner(token, positions.len(), line)?);
                }
                if corners.len() < 3 {
                    return Err(ExchangeError::Syntax {
                        line,
                        msg: "Eine Fläche braucht mindestens drei Ecken",
                    });
                }
                triangulate(&positions, &corners, line, &mut indices)?;
                if indices.len() / 3 > MAX_TRIANGLES {
                    return Err(ExchangeError::TooManyTriangles);
                }
            }
            // Kommentare, Gruppen, Materialien, Glättung, Texturkoordinaten, Normalen und alles
            // Übrige werden übergangen (Spec §3).
            _ => {}
        }
    }
    if indices.is_empty() {
        return Err(ExchangeError::Empty);
    }
    Ok(weld_exact(&positions, &indices))
}

/// Punktnummer einer Ecke; negative Nummern zählen vom zuletzt gelesenen Punkt zurück. Nur schon
/// gelesene Punkte sind gültig (P6).
fn corner(token: &str, count: usize, line: usize) -> Result<u32, ExchangeError> {
    let first = token.split('/').next().unwrap_or_default();
    let i: i64 = first.parse().map_err(|_| ExchangeError::Syntax {
        line,
        msg: "Eine Ecke hat keine gültige Punktnummer",
    })?;
    let count = count as i64;
    let index = match i.cmp(&0) {
        Ordering::Greater => i - 1,
        Ordering::Less => count + i,
        Ordering::Equal => -1,
    };
    if !(0..count).contains(&index) {
        return Err(ExchangeError::IndexOutOfRange { line });
    }
    Ok(index as u32)
}

fn point(p: [f32; 3]) -> DVec3 {
    DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
}

/// Zerlegt ein Vieleck in Dreiecke (P6): konvex → Fächer, sonst `earcutr` mit angeglichener
/// Orientierung.
fn triangulate(
    positions: &[[f32; 3]],
    corners: &[u32],
    line: usize,
    out: &mut Vec<u32>,
) -> Result<(), ExchangeError> {
    let n = corners.len();
    if n == 3 {
        out.extend_from_slice(corners);
        return Ok(());
    }
    let pts: Vec<DVec3> = corners.iter().map(|&i| point(positions[i as usize])).collect();
    let normal = newell(&pts);
    if is_convex(&pts, normal) {
        for w in corners[1..].windows(2) {
            out.extend_from_slice(&[corners[0], w[0], w[1]]);
        }
        return Ok(());
    }
    let flat = project(&pts, normal);
    let tris = earcutr::earcut(&flat, &[], 2).map_err(|_| ExchangeError::Polygon { line })?;
    if tris.len() != 3 * (n - 2) {
        return Err(ExchangeError::Polygon { line });
    }
    let at = |k: usize| (flat[2 * k], flat[2 * k + 1]);
    let polygon: f64 = (0..n)
        .map(|i| {
            let ((x0, y0), (x1, y1)) = (at(i), at((i + 1) % n));
            x0 * y1 - x1 * y0
        })
        .sum();
    for t in tris.as_chunks::<3>().0 {
        let [a, b, c] = *t;
        let ((ax, ay), (bx, by), (cx, cy)) = (at(a), at(b), at(c));
        let signed = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if signed * polygon >= 0.0 {
            out.extend_from_slice(&[corners[a], corners[b], corners[c]]);
        } else {
            out.extend_from_slice(&[corners[a], corners[c], corners[b]]);
        }
    }
    Ok(())
}

/// Normale nach Newell (Länge = doppelte Fläche des Vielecks).
fn newell(pts: &[DVec3]) -> DVec3 {
    let n = pts.len();
    pts.iter()
        .enumerate()
        .map(|(i, p)| p.cross(pts[(i + 1) % n]))
        .sum()
}

/// Konvex (gerade Ecken erlaubt), wenn jede Ecke im Sinn der Normalen abbiegt; ohne Normale nie.
fn is_convex(pts: &[DVec3], normal: DVec3) -> bool {
    if normal == DVec3::ZERO {
        return false;
    }
    let n = pts.len();
    (0..n).all(|i| {
        let (a, b, c) = (pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
        (b - a).cross(c - b).dot(normal) >= 0.0
    })
}

/// Punkte in die Ebene senkrecht zur größten Komponente der Normalen (für `earcut`).
fn project(pts: &[DVec3], normal: DVec3) -> Vec<f64> {
    let a = normal.abs();
    let axes: fn(DVec3) -> [f64; 2] = if a.z >= a.x && a.z >= a.y {
        |p| [p.x, p.y]
    } else if a.x >= a.y {
        |p| [p.y, p.z]
    } else {
        |p| [p.z, p.x]
    };
    pts.iter().flat_map(|&p| axes(p)).collect()
}
```

`crates/planora-exchange/src/lib.rs` vollständig ersetzen:

```rust
//! Austausch von Dreiecksnetzen (Teilprojekt 9): STL, OBJ, 3MF und glTF schreiben, STL und OBJ
//! lesen, Netze prüfen. Keine neue Abhängigkeit: ZIP und CRC-32 sind selbst geschrieben,
//! Vielecke zerlegt das schon vorhandene `earcutr`.

mod check;
pub mod error;
pub mod limits;
mod mesh;
mod obj;
mod progress;
mod source;
mod stl;
mod text;

pub use check::{MeshReport, check_mesh, weld_exact};
pub use error::ExchangeError;
pub use limits::{
    CHECK_EVERY, MAX_FILE_BYTES, MAX_LINE_BYTES, MAX_TRIANGLES, MAX_VERTICES, READ_BUFFER,
    RESERVE_STEP,
};
pub use mesh::{ExchangeMesh, MeshTransform};
pub use obj::{read_obj, write_obj};
pub use progress::Progress;
pub use source::{ExportBody, PlacedSource, SolidSource, TriSource, collect_body};
pub use stl::{STL_HEADER_BYTES, STL_TRIANGLE_BYTES, read_stl, write_stl};
```

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (8 Tests in `obj`).

```bash
git add Cargo.lock crates/planora-exchange/Cargo.toml crates/planora-exchange/src/lib.rs crates/planora-exchange/src/obj.rs crates/planora-exchange/tests/obj.rs
git commit -m "exchange: OBJ schreiben und lesen, Vielecke mit earcutr

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Stufentest 1 – Formeln, Rundlauf, Zufallsreihe, Speicherwächter

**Files:**
- Test: `crates/planora-exchange/tests/stage1.rs`, `crates/planora-exchange/tests/alloc_guard.rs`

**Interfaces:**
- Consumes: Task 1–4; `planora_kernel::{AnalyticKernel, Kernel, BlendEdge, BlendKind, BlendRequest, Solid}`, `planora_kernel::blend::topo::{edge_uses, face_name}`, `planora_mesh::{tessellate, mass_properties, TessellationOptions}`.
- Produces: nur Tests; Stufenabschluss 1.

- [ ] **Step 1: Stufentest schreiben** (`crates/planora-exchange/tests/stage1.rs`)

```rust
//! Stufentest 1: Körper aus dem Kernel als STL und OBJ schreiben, lesen und mit Formeln
//! vergleichen; Zufallsreihe gegen die Masseneigenschaften (Teilprojekt 9, Task 5).

use planora_exchange::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{AnalyticKernel, BlendEdge, BlendKind, BlendRequest, Kernel, Solid};
use planora_math::{DMat4, DVec3, PI};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

/// Rundung durch `f32` in der Datei, relativ zum Volumen (Koordinaten bis 30 mm).
const F32_REL: f64 = 1e-5;
/// Wie [`F32_REL`] für die Zufallsreihe mit Lagen bis 500 mm und Körpern ab 0,5 mm.
const RANDOM_REL: f64 = 1e-3;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

fn export_body(name: &str, solid: &Solid, chord: f64) -> (ExportBody, f64) {
    let mesh = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: chord,
        },
    )
    .unwrap();
    let body = collect_body(&SolidSource { name, mesh: &mesh }).unwrap();
    (body, mass_properties(&mesh).volume)
}

/// Schreibt STL und OBJ, liest beide zurück.
fn round_trip(body: &ExportBody) -> [ExchangeMesh; 2] {
    let mut stl = Vec::new();
    write_stl(&mut stl, std::slice::from_ref(body)).unwrap();
    let mut obj = Vec::new();
    write_obj(&mut obj, std::slice::from_ref(body)).unwrap();
    let p = Progress::new();
    [
        read_stl(&stl[..], stl.len() as u64, &p).unwrap(),
        read_obj(&obj[..], obj.len() as u64, &p).unwrap(),
    ]
}

/// Würfel 10 mm mit einer gerundeten Kante (Deckel/Wand 1).
fn filleted_box(r: f64) -> Solid {
    let k = AnalyticKernel::default();
    let s = k.make_box(DVec3::splat(10.0)).unwrap();
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(&s, id)
                .iter()
                .map(|u| face_name(&s, u.face))
                .collect();
            n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
        })
        .unwrap();
    let request = BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face: None,
        }],
        kind: BlendKind::Fillet { radius: r },
        tangent_chain: false,
    };
    k.blend(&s, &request).unwrap().solid
}

#[test]
fn closed_formulas_survive_the_files() {
    let k = AnalyticKernel::default();
    let r = 1.5;
    let cases: [(&str, Solid, f64); 4] = [
        ("Quader", k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap(), 1000.0),
        ("Zylinder", k.make_cylinder(4.0, 10.0).unwrap(), PI * 16.0 * 10.0),
        ("Kugel", k.make_sphere(5.0).unwrap(), 4.0 / 3.0 * PI * 125.0),
        (
            "Gerundeter Quader",
            filleted_box(r),
            1000.0 - 10.0 * r * r * (1.0 - PI / 4.0),
        ),
    ];
    // Grob, Normal, Fein (Spec E5)
    for chord in [0.1, 0.01, 0.001] {
        for (name, solid, volume) in &cases {
            let (body, v_mesh) = export_body(name, solid, chord);
            let area = body.mesh.mass(&MeshTransform::IDENTITY).surface_area;
            let closed = check_mesh(&body.mesh).closed;
            for back in round_trip(&body) {
                assert_eq!(back.triangle_count(), body.mesh.triangle_count(), "{name}");
                let m = back.mass(&MeshTransform::IDENTITY);
                assert!(
                    (m.volume - v_mesh).abs() <= F32_REL * volume,
                    "{name} {chord}: Datei {} / Netz {v_mesh}",
                    m.volume
                );
                assert!(
                    (m.surface_area - area).abs() <= F32_REL * area,
                    "{name} {chord}: Fläche {} / {area}",
                    m.surface_area
                );
                // Das Netz liegt innerhalb der Sehnentoleranz an der Fläche.
                assert!(
                    (m.volume - volume).abs() <= chord * area + F32_REL * volume,
                    "{name} {chord}: Datei {} / Formel {volume}",
                    m.volume
                );
                assert_eq!(check_mesh(&back).closed, closed, "{name} {chord}");
            }
        }
    }
}

fn random_series(n: usize, seed: u64) {
    let k = AnalyticKernel::default();
    let mut rng = Lcg(seed);
    for i in 0..n {
        let solid = match i % 4 {
            0 => k
                .make_box(DVec3::new(
                    rng.range(0.5, 30.0),
                    rng.range(0.5, 30.0),
                    rng.range(0.5, 30.0),
                ))
                .unwrap(),
            1 => k
                .make_cylinder(rng.range(0.5, 15.0), rng.range(0.5, 30.0))
                .unwrap(),
            2 => k.make_sphere(rng.range(0.5, 15.0)).unwrap(),
            _ => k
                .make_cone(rng.range(0.5, 15.0), rng.range(0.5, 30.0))
                .unwrap(),
        };
        let offset = DVec3::new(
            rng.range(-500.0, 500.0),
            rng.range(-500.0, 500.0),
            rng.range(-500.0, 500.0),
        );
        let solid = k
            .transform(&solid, &DMat4::from_translation(offset))
            .unwrap();
        let (body, v_mesh) = export_body("Zufall", &solid, 0.05);
        for back in round_trip(&body) {
            let v = back.mass(&MeshTransform::IDENTITY).volume;
            assert!(
                (v - v_mesh).abs() <= RANDOM_REL * v_mesh.abs(),
                "Fall {i}: {v} / {v_mesh}"
            );
        }
    }
}

#[test]
fn a_fixed_random_series_keeps_the_volume() {
    random_series(24, 0x5EED_0009);
}

#[test]
#[ignore = "lange Zufallsreihe; nur mit --release --ignored (etwa 20 s)"]
fn a_long_random_series_keeps_the_volume() {
    random_series(2000, 0x5EED_0019);
}
```

- [ ] **Step 2: Speicherwächter schreiben** (`crates/planora-exchange/tests/alloc_guard.rs`)

```rust
//! Feindliche Dateien fordern keinen großen Speicher an (Spec §5, §9): Ein eigener Allocator
//! merkt sich die größte einzelne Anforderung. Nur ein Test in dieser Datei, damit nichts
//! anderes gleichzeitig anfordert (Teilprojekt 9, Task 5).

use planora_exchange::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static LARGEST: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LARGEST.fetch_max(new_size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Größte erlaubte einzelne Anforderung beim Lesen einer feindlichen Datei (16 MiB; der
/// Lesepuffer hat 64 KiB, eine Zeile höchstens 1 MiB, die erste Reservierung 65 536 Dreiecke).
const LIMIT: usize = 16 * 1024 * 1024;

/// Führt `f` aus und liefert die größte einzelne Anforderung dabei.
fn largest_during(f: impl FnOnce()) -> usize {
    LARGEST.store(0, Ordering::Relaxed);
    f();
    LARGEST.load(Ordering::Relaxed)
}

#[test]
fn hostile_files_never_ask_for_much_memory() {
    let p = Progress::new();
    // Binärkopf verspricht 50 Millionen Dreiecke.
    let mut claim = b"Planora".to_vec();
    claim.resize(80, 0);
    claim.extend_from_slice(&(MAX_TRIANGLES as u32).to_le_bytes());
    let mut cases: Vec<(&str, Vec<u8>, u64, ExchangeError)> = vec![
        (
            "8-GB-Behauptung ohne Inhalt",
            claim.clone(),
            MAX_FILE_BYTES,
            ExchangeError::Truncated,
        ),
        (
            "Kopfzahl größer als die Datei",
            claim.clone(),
            claim.len() as u64,
            ExchangeError::CountTooLarge {
                claimed: MAX_TRIANGLES as u64,
                possible: 0,
            },
        ),
        (
            "Datei über 8 GB",
            claim.clone(),
            MAX_FILE_BYTES + 1,
            ExchangeError::FileTooLarge {
                bytes: MAX_FILE_BYTES + 1,
            },
        ),
    ];
    let mut endless = b"solid x\n".to_vec();
    endless.extend(std::iter::repeat_n(b'1', 10 * MAX_LINE_BYTES));
    let len = endless.len() as u64;
    cases.push((
        "Zeile ohne Ende",
        endless,
        len,
        ExchangeError::Syntax {
            line: 2,
            msg: "Die Zeile ist zu lang",
        },
    ));
    for (what, bytes, len, expected) in cases {
        let mut result = None;
        let used = largest_during(|| result = Some(read_stl(&bytes[..], len, &p)));
        assert_eq!(result, Some(Err(expected)), "{what}");
        assert!(used <= LIMIT, "{what}: {used} Bytes");
    }
    // OBJ: riesige Nummer, riesige Zahl, Verweis ohne Punkte
    for (text, expected) in [
        (
            "v 0 0 0\nf 1 1 4294967296000\n",
            ExchangeError::IndexOutOfRange { line: 2 },
        ),
        ("v 1e400 0 0\n", ExchangeError::NotFinite { line: Some(1) }),
        ("f 1 2 3\n", ExchangeError::IndexOutOfRange { line: 1 }),
    ] {
        let mut result = None;
        let used = largest_during(|| {
            result = Some(read_obj(text.as_bytes(), MAX_FILE_BYTES, &p));
        });
        assert_eq!(result, Some(Err(expected)), "{text:?}");
        assert!(used <= LIMIT, "{text:?}: {used} Bytes");
    }
}
```

- [ ] **Step 3: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange && cargo test -p planora-exchange --release -- --ignored`
Expected: PASS; Laufzeiten notieren (Debug je Testprogramm unter 30 s, die lange Zufallsreihe im Release). Ein Fehlschlag ist ein Befund: Ursache beheben oder anhalten und berichten (keine Schranke lockern).

- [ ] **Step 4: Stufenabschluss 1**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
git diff --stat 134cde9 -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs
```
Expected: alles grün; die letzte Ausgabe ist leer.

- [ ] **Step 5: Commit**

```bash
git add crates/planora-exchange/tests/stage1.rs crates/planora-exchange/tests/alloc_guard.rs
git commit -m "exchange: Stufentest 1 (Formeln, Rundlauf, Zufallsreihe, Speicherwächter)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Stufe 2 – 3MF (eigenes ZIP) und glTF, geprüft mit eigenen Mini-Lesern

### Task 6: ZIP-Schreiber mit CRC-32 und Mini-Leser

**Files:**
- Create: `crates/planora-exchange/src/zip.rs`, `crates/planora-exchange/tests/common/mod.rs`
- Modify: `crates/planora-exchange/src/lib.rs`
- Test: `crates/planora-exchange/tests/zip.rs`

**Interfaces:**
- Consumes: Task 1 (`ExchangeError`).
- Produces:
  ```rust
  pub struct Crc32;  impl Crc32 { pub fn new() -> Crc32; pub fn update(&mut self, bytes: &[u8]); pub fn finish(self) -> u32; }
  pub fn crc32(bytes: &[u8]) -> u32;
  pub type Content<'a> = &'a dyn Fn(&mut dyn Write) -> io::Result<()>;
  pub struct ZipWriter<W: Write>;
  impl<W: Write> ZipWriter<W> { pub fn new(out: W) -> Self;
      pub fn add_entry(&mut self, name: &str, content: Content<'_>) -> Result<(), ExchangeError>;
      pub fn finish(self) -> Result<W, ExchangeError>; }
  // tests/common/mod.rs (nur Tests): pub struct ZipEntry { pub name: String, pub data: Vec<u8> };
  // pub fn read_zip(b: &[u8]) -> Result<Vec<ZipEntry>, String>; pub fn crc32_slow(data: &[u8]) -> u32;
  ```

- [ ] **Step 1: Mini-Leser schreiben** (`crates/planora-exchange/tests/common/mod.rs`)

```rust
//! Mini-Leser für die Tests der Stufe 2 (unabhängig vom Schreiber, eigene CRC).
#![allow(dead_code)]

/// Ein Eintrag eines ZIP-Archivs.
pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
}

fn u16_at(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([b[at], b[at + 1]]))
}

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

/// CRC-32 Bit für Bit (langsam, aber unabhängig von der Tabelle des Schreibers).
pub fn crc32_slow(data: &[u8]) -> u32 {
    let mut c = u32::MAX;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// Liest ein ZIP-Archiv streng: Endsatz am Ende ohne Kommentar, Verzeichnis lückenlos direkt
/// vor dem Endsatz, jeder lokale Kopf passt zum Verzeichnis, Methode „stored“, CRC und Größen
/// stimmen, Einträge liegen lückenlos hintereinander.
pub fn read_zip(b: &[u8]) -> Result<Vec<ZipEntry>, String> {
    if b.len() < 22 {
        return Err("zu kurz".into());
    }
    let end = b.len() - 22;
    if u32_at(b, end) != 0x0605_4B50 {
        return Err("kein Endsatz".into());
    }
    let count = u16_at(b, end + 10);
    if u16_at(b, end + 8) != count || u16_at(b, end + 20) != 0 {
        return Err("Endsatz unstimmig".into());
    }
    let (dir_len, dir_start) = (u32_at(b, end + 12), u32_at(b, end + 16));
    if dir_start + dir_len != end {
        return Err("Verzeichnis liegt nicht direkt vor dem Endsatz".into());
    }
    let (mut at, mut next_local) = (dir_start, 0usize);
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(b, at) != 0x0201_4B50 {
            return Err(format!("kein Verzeichniseintrag bei {at}"));
        }
        let (method, crc) = (u16_at(b, at + 10), u32_at(b, at + 16) as u32);
        let (packed, size) = (u32_at(b, at + 20), u32_at(b, at + 24));
        let (name_len, extra, comment) = (u16_at(b, at + 28), u16_at(b, at + 30), u16_at(b, at + 32));
        let local = u32_at(b, at + 42);
        let name = String::from_utf8(b[at + 46..at + 46 + name_len].to_vec()).map_err(|e| e.to_string())?;
        if method != 0 || packed != size {
            return Err(format!("{name}: nicht „stored“"));
        }
        if local != next_local {
            return Err(format!("{name}: Lücke vor dem lokalen Kopf"));
        }
        if u32_at(b, local) != 0x0403_4B50 {
            return Err(format!("{name}: kein lokaler Kopf"));
        }
        let (l_name, l_extra) = (u16_at(b, local + 26), u16_at(b, local + 28));
        let same = u16_at(b, local + 8) == 0
            && u32_at(b, local + 14) as u32 == crc
            && u32_at(b, local + 18) == size
            && u32_at(b, local + 22) == size
            && b[local + 30..local + 30 + l_name] == *name.as_bytes();
        if !same {
            return Err(format!("{name}: lokaler Kopf passt nicht zum Verzeichnis"));
        }
        let data_at = local + 30 + l_name + l_extra;
        let data = b[data_at..data_at + size].to_vec();
        if crc32_slow(&data) != crc {
            return Err(format!("{name}: CRC falsch"));
        }
        next_local = data_at + size;
        at += 46 + name_len + extra + comment;
        out.push(ZipEntry { name, data });
    }
    if next_local != dir_start || at != end {
        return Err("Einträge nicht lückenlos".into());
    }
    Ok(out)
}
```

- [ ] **Step 2: Tests schreiben** (`crates/planora-exchange/tests/zip.rs`)

```rust
//! ZIP-Schreiber und CRC-32 (Teilprojekt 9, Task 6).

mod common;

use common::{crc32_slow, read_zip};
use planora_exchange::*;
use std::cell::Cell;
use std::io::Write;

#[test]
fn crc32_matches_the_standard_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
    let mut c = Crc32::new();
    c.update(b"1234");
    c.update(b"56789");
    assert_eq!(c.finish(), 0xCBF4_3926);
    let text = b"PlanoraCAD schreibt 3MF";
    assert_eq!(crc32_slow(text), crc32(text));
}

#[test]
fn entries_are_stored_with_crc_and_directory() {
    let mut z = ZipWriter::new(Vec::new());
    z.add_entry("a.txt", &|w| w.write_all(b"Hallo")).unwrap();
    z.add_entry("ordner/b.bin", &|w| {
        for i in 0..1000u32 {
            w.write_all(&i.to_le_bytes())?;
        }
        Ok(())
    })
    .unwrap();
    z.add_entry("leer", &|_| Ok(())).unwrap();
    let bytes = z.finish().unwrap();
    let entries = read_zip(&bytes).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a.txt", "ordner/b.bin", "leer"]);
    assert_eq!(entries[0].data, b"Hallo");
    assert_eq!(entries[1].data.len(), 4000);
    assert_eq!(&entries[1].data[4..8], &1u32.to_le_bytes());
    assert!(entries[2].data.is_empty());
}

#[test]
fn the_same_content_gives_the_same_file() {
    let make = || {
        let mut z = ZipWriter::new(Vec::new());
        z.add_entry("x", &|w| w.write_all(b"abc")).unwrap();
        z.finish().unwrap()
    };
    assert_eq!(make(), make());
}

#[test]
fn content_that_changes_between_passes_is_refused() {
    let calls = Cell::new(0u8);
    let mut z = ZipWriter::new(Vec::new());
    let e = z
        .add_entry("x", &|w| {
            calls.set(calls.get() + 1);
            w.write_all(&[calls.get()])
        })
        .unwrap_err();
    assert!(matches!(e, ExchangeError::Corrupt(_)), "{e:?}");
}

#[test]
fn an_empty_archive_is_valid() {
    let bytes = ZipWriter::new(Vec::new()).finish().unwrap();
    assert_eq!(bytes.len(), 22);
    assert!(read_zip(&bytes).unwrap().is_empty());
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test zip`
Expected: FAIL – `cannot find struct ZipWriter`.

- [ ] **Step 4: Implementieren**

`crates/planora-exchange/src/zip.rs`:

```rust
//! Kleinster ZIP-Schreiber für 3MF (Spec E7, Plan P7): Methode „stored“ ohne Kompression,
//! CRC-32, feste Zeitangabe (1.1.1980), ohne ZIP64. Jeder Eintrag wird zweimal erzeugt: erst
//! Größe und CRC messen, dann schreiben; so liegt nichts ganz im Speicher.

use crate::error::ExchangeError;
use std::io::{self, Write};

/// Erzeugerpolynom der CRC-32 nach IEEE 802.3 (wie ZIP), bitweise gespiegelt.
const CRC_POLY: u32 = 0xEDB8_8320;
const CRC_TABLE: [u32; 256] = crc_table();
const LOCAL_SIG: u32 = 0x0403_4B50;
const CENTRAL_SIG: u32 = 0x0201_4B50;
const END_SIG: u32 = 0x0605_4B50;
/// ZIP-Version 2.0 („stored“ und Ordner im Namen).
const VERSION: u16 = 20;
/// 00:00 Uhr im DOS-Format.
const DOS_TIME: u16 = 0;
/// 1.1.1980 im DOS-Format (Jahr 0 ab 1980, Monat 1, Tag 1).
const DOS_DATE: u16 = (1 << 5) | 1;
/// Feste Länge eines lokalen Kopfs ohne Namen.
const LOCAL_HEADER: usize = 30;

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { CRC_POLY ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// Laufende CRC-32.
#[derive(Clone, Copy, Debug)]
pub struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Crc32(u32::MAX)
    }
}

impl Crc32 {
    pub fn new() -> Crc32 {
        Crc32::default()
    }

    pub fn update(&mut self, bytes: &[u8]) {
        let mut c = self.0;
        for &b in bytes {
            c = CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
        }
        self.0 = c;
    }

    pub fn finish(self) -> u32 {
        !self.0
    }
}

/// CRC-32 eines Bytefelds.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut c = Crc32::new();
    c.update(bytes);
    c.finish()
}

/// Inhalt eines Eintrags: wird zweimal aufgerufen und muss beide Male dieselben Bytes liefern.
pub type Content<'a> = &'a dyn Fn(&mut dyn Write) -> io::Result<()>;

/// Zählt Bytes und rechnet die CRC, ohne etwas zu speichern.
struct Measure {
    crc: Crc32,
    len: u64,
}

impl Measure {
    fn new() -> Measure {
        Measure {
            crc: Crc32::new(),
            len: 0,
        }
    }
}

impl Write for Measure {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.crc.update(buf);
        self.len += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Schreibt ins Ziel und misst dabei mit.
struct Tee<'a, W> {
    out: &'a mut W,
    measure: Measure,
}

impl<W: Write> Write for Tee<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.out.write(buf)?;
        self.measure.crc.update(&buf[..n]);
        self.measure.len += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// ZIP-Archiv im Entstehen.
pub struct ZipWriter<W: Write> {
    out: W,
    offset: u64,
    entries: Vec<Entry>,
}

fn put16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

/// Größen und Lagen müssen ohne ZIP64 in 32 Bit passen (bis knapp 4 GiB).
fn small(n: u64) -> Result<u32, ExchangeError> {
    u32::try_from(n)
        .ok()
        .filter(|&v| v != u32::MAX)
        .ok_or(ExchangeError::TooLargeForFormat { format: "3MF" })
}

impl<W: Write> ZipWriter<W> {
    pub fn new(out: W) -> Self {
        ZipWriter {
            out,
            offset: 0,
            entries: Vec::new(),
        }
    }

    /// Hängt einen Eintrag an (Name in ASCII, Ordner mit „/“).
    pub fn add_entry(&mut self, name: &str, content: Content<'_>) -> Result<(), ExchangeError> {
        let name_len = u16::try_from(name.len()).map_err(|_| {
            ExchangeError::Corrupt("Der Name eines ZIP-Eintrags ist zu lang".into())
        })?;
        let mut measure = Measure::new();
        content(&mut measure)?;
        let (crc, len) = (measure.crc.finish(), measure.len);
        let size = small(len)?;
        let offset = small(self.offset)?;
        let mut header = Vec::with_capacity(LOCAL_HEADER + name.len());
        put32(&mut header, LOCAL_SIG);
        put16(&mut header, VERSION);
        put16(&mut header, 0); // Kennzeichen
        put16(&mut header, 0); // Methode „stored“
        put16(&mut header, DOS_TIME);
        put16(&mut header, DOS_DATE);
        put32(&mut header, crc);
        put32(&mut header, size); // gepackt
        put32(&mut header, size); // ungepackt
        put16(&mut header, name_len);
        put16(&mut header, 0); // Zusatzfeld
        header.extend_from_slice(name.as_bytes());
        self.out.write_all(&header)?;
        let mut tee = Tee {
            out: &mut self.out,
            measure: Measure::new(),
        };
        content(&mut tee)?;
        if tee.measure.crc.finish() != crc || tee.measure.len != len {
            return Err(ExchangeError::Corrupt(
                "Der Inhalt eines ZIP-Eintrags hat sich beim Schreiben geändert".into(),
            ));
        }
        self.offset += header.len() as u64 + len;
        self.entries.push(Entry {
            name: name.to_string(),
            crc,
            size,
            offset,
        });
        Ok(())
    }

    /// Schreibt Verzeichnis und Endsatz und gibt das Ziel zurück.
    pub fn finish(mut self) -> Result<W, ExchangeError> {
        let start = small(self.offset)?;
        let mut dir = Vec::new();
        for e in &self.entries {
            put32(&mut dir, CENTRAL_SIG);
            put16(&mut dir, VERSION); // erstellt mit
            put16(&mut dir, VERSION); // benötigt
            put16(&mut dir, 0); // Kennzeichen
            put16(&mut dir, 0); // Methode „stored“
            put16(&mut dir, DOS_TIME);
            put16(&mut dir, DOS_DATE);
            put32(&mut dir, e.crc);
            put32(&mut dir, e.size);
            put32(&mut dir, e.size);
            put16(&mut dir, e.name.len() as u16);
            put16(&mut dir, 0); // Zusatzfeld
            put16(&mut dir, 0); // Kommentar
            put16(&mut dir, 0); // Datenträger
            put16(&mut dir, 0); // interne Attribute
            put32(&mut dir, 0); // externe Attribute
            put32(&mut dir, e.offset);
            dir.extend_from_slice(e.name.as_bytes());
        }
        let dir_len = small(dir.len() as u64)?;
        small(self.offset + dir.len() as u64)?;
        let count = u16::try_from(self.entries.len())
            .map_err(|_| ExchangeError::TooLargeForFormat { format: "3MF" })?;
        put32(&mut dir, END_SIG);
        put16(&mut dir, 0); // dieser Datenträger
        put16(&mut dir, 0); // Datenträger des Verzeichnisses
        put16(&mut dir, count);
        put16(&mut dir, count);
        put32(&mut dir, dir_len);
        put32(&mut dir, start);
        put16(&mut dir, 0); // Kommentar
        self.out.write_all(&dir)?;
        self.out.flush()?;
        Ok(self.out)
    }
}
```

In `crates/planora-exchange/src/lib.rs` nach `mod text;` die Zeile `mod zip;` einfügen und am Ende `pub use zip::{Content, Crc32, ZipWriter, crc32};` ergänzen (die übrigen Zeilen bleiben wie nach Task 4).

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (5 Tests in `zip`).

```bash
git add crates/planora-exchange/src/lib.rs crates/planora-exchange/src/zip.rs crates/planora-exchange/tests/common/mod.rs crates/planora-exchange/tests/zip.rs
git commit -m "exchange: eigener ZIP-Schreiber mit CRC-32 (stored, ohne ZIP64)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: 3MF schreiben und mit XML-Mini-Leser prüfen

**Files:**
- Create: `crates/planora-exchange/src/threemf.rs`
- Modify: `crates/planora-exchange/src/lib.rs`, `crates/planora-exchange/tests/common/mod.rs` (XML-Prüfung ergänzen)
- Test: `crates/planora-exchange/tests/threemf.rs`

**Interfaces:**
- Consumes: Task 6 (`ZipWriter`, `Content`), Task 2 (`ExportBody`).
- Produces:
  ```rust
  pub fn write_3mf<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError>;
  // tests/common/mod.rs: pub struct XmlElement { pub name: String, pub attrs: Vec<(String, String)> };
  // pub fn parse_xml(text: &str) -> Result<Vec<XmlElement>, String>;
  ```

- [ ] **Step 1: XML-Prüfung im Mini-Leser ergänzen**

An das Ende von `crates/planora-exchange/tests/common/mod.rs` anfügen:

```rust
/// Element einer XML-Datei: Name und Attribute (Werte schon entmaskiert).
pub struct XmlElement {
    pub name: String,
    pub attrs: Vec<(String, String)>,
}

/// Prüft XML auf Wohlgeformtheit (Kopfzeile, genau ein Wurzelelement, Tags paarweise
/// geschlossen, Attribute in Anführungszeichen und nicht doppelt, nur die fünf
/// Standard-Entitäten, kein Text außerhalb der Wurzel) und liefert alle Elemente in Reihenfolge.
pub fn parse_xml(text: &str) -> Result<Vec<XmlElement>, String> {
    let mut rest = text.strip_prefix("<?xml ").ok_or("keine XML-Kopfzeile")?;
    let close = rest.find("?>").ok_or("Kopfzeile nicht geschlossen")?;
    rest = &rest[close + 2..];
    let mut stack: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let mut seen_root = false;
    while let Some(lt) = rest.find('<') {
        let between = &rest[..lt];
        check_text(between)?;
        if stack.is_empty() && !between.trim().is_empty() {
            return Err("Text außerhalb des Wurzelelements".into());
        }
        rest = &rest[lt + 1..];
        let gt = rest.find('>').ok_or("Tag nicht geschlossen")?;
        let tag = &rest[..gt];
        rest = &rest[gt + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            if stack.pop().as_deref() != Some(name.trim()) {
                return Err(format!("</{name}> passt nicht"));
            }
            continue;
        }
        let (body, self_closing) = match tag.strip_suffix('/') {
            Some(b) => (b, true),
            None => (tag, false),
        };
        let (name, attrs) = parse_tag(body)?;
        if stack.is_empty() {
            if seen_root {
                return Err("zweites Wurzelelement".into());
            }
            seen_root = true;
        }
        if !self_closing {
            stack.push(name.clone());
        }
        out.push(XmlElement { name, attrs });
    }
    if !rest.trim().is_empty() {
        return Err("Text nach dem Wurzelelement".into());
    }
    if !stack.is_empty() {
        return Err(format!("nicht geschlossen: {stack:?}"));
    }
    if !seen_root {
        return Err("kein Element".into());
    }
    Ok(out)
}

fn parse_tag(body: &str) -> Result<(String, Vec<(String, String)>), String> {
    let body = body.trim();
    let end = body.find(char::is_whitespace).unwrap_or(body.len());
    let name = body[..end].to_string();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ':') {
        return Err(format!("ungültiger Name „{name}“"));
    }
    let mut rest = body[end..].trim_start();
    let mut attrs: Vec<(String, String)> = Vec::new();
    while !rest.is_empty() {
        let eq = rest
            .find('=')
            .ok_or_else(|| format!("Attribut ohne Wert in <{name}>"))?;
        let key = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next().ok_or("Wert fehlt")?;
        if quote != '"' && quote != '\'' {
            return Err(format!("Wert von {key} ohne Anführungszeichen"));
        }
        let close = after[1..]
            .find(quote)
            .ok_or_else(|| format!("Wert von {key} nicht geschlossen"))?;
        let value = &after[1..1 + close];
        check_text(value)?;
        if attrs.iter().any(|(k, _)| *k == key) {
            return Err(format!("Attribut {key} doppelt"));
        }
        attrs.push((key, unescape(value)));
        rest = after[close + 2..].trim_start();
    }
    Ok((name, attrs))
}

fn check_text(t: &str) -> Result<(), String> {
    if t.contains('<') {
        return Err("„<“ im Text".into());
    }
    let mut rest = t;
    while let Some(i) = rest.find('&') {
        let tail = &rest[i..];
        let known = ["&amp;", "&lt;", "&gt;", "&quot;", "&apos;"]
            .iter()
            .any(|e| tail.starts_with(e));
        if !known {
            let start: String = tail.chars().take(8).collect();
            return Err(format!("unbekannte Entität bei „{start}“"));
        }
        rest = &tail[1..];
    }
    Ok(())
}

fn unescape(v: &str) -> String {
    v.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
```

- [ ] **Step 2: Tests schreiben** (`crates/planora-exchange/tests/threemf.rs`)

```rust
//! 3MF: Paket, wohlgeformtes XML, Objekte, Punkte und Dreiecke (Teilprojekt 9, Task 7).

mod common;

use common::{XmlElement, parse_xml, read_zip};
use planora_exchange::*;

/// Einheitswürfel, um `shift` in x verschoben.
fn cube(shift: f32) -> ExchangeMesh {
    let mut m = ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    };
    for p in &mut m.positions {
        p[0] += shift;
    }
    m
}

fn bodies() -> [ExportBody; 2] {
    [
        ExportBody {
            name: "Würfel <1> & \"Co\"".into(),
            mesh: cube(0.0),
        },
        ExportBody {
            name: "Zweiter".into(),
            mesh: cube(5.0),
        },
    ]
}

fn write(bodies: &[ExportBody]) -> Vec<u8> {
    let mut out = Vec::new();
    write_3mf(&mut out, bodies).unwrap();
    out
}

fn model_of(bytes: &[u8]) -> Vec<XmlElement> {
    let entries = read_zip(bytes).unwrap();
    let model = entries
        .into_iter()
        .find(|e| e.name == "3D/3dmodel.model")
        .unwrap();
    parse_xml(&String::from_utf8(model.data).unwrap()).unwrap()
}

fn attr<'a>(e: &'a XmlElement, key: &str) -> &'a str {
    &e.attrs.iter().find(|(k, _)| k == key).unwrap().1
}

#[test]
fn the_package_has_the_three_parts_and_valid_xml() {
    let out = write(&bodies());
    let entries = read_zip(&out).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"]);
    for e in &entries {
        parse_xml(std::str::from_utf8(&e.data).unwrap()).unwrap_or_else(|m| panic!("{}: {m}", e.name));
    }
    let rels = parse_xml(std::str::from_utf8(&entries[1].data).unwrap()).unwrap();
    let rel = rels.iter().find(|x| x.name == "Relationship").unwrap();
    assert_eq!(attr(rel, "Target"), "/3D/3dmodel.model");
    let types = parse_xml(std::str::from_utf8(&entries[0].data).unwrap()).unwrap();
    assert!(types.iter().any(|x| x.name == "Default" && attr(x, "Extension") == "model"));
}

#[test]
fn objects_items_points_and_triangles_match_the_bodies() {
    let xml = model_of(&write(&bodies()));
    let root = &xml[0];
    assert_eq!(root.name, "model");
    assert_eq!(attr(root, "unit"), "millimeter");
    let objects: Vec<&XmlElement> = xml.iter().filter(|e| e.name == "object").collect();
    assert_eq!(objects.len(), 2);
    assert_eq!(attr(objects[0], "type"), "model");
    assert_eq!(attr(objects[0], "name"), "Würfel <1> & \"Co\"");
    let items: Vec<&str> = xml
        .iter()
        .filter(|e| e.name == "item")
        .map(|e| attr(e, "objectid"))
        .collect();
    assert_eq!(items, ["1", "2"]);
    let vertices: Vec<&XmlElement> = xml.iter().filter(|e| e.name == "vertex").collect();
    assert_eq!(vertices.len(), 16);
    assert_eq!(xml.iter().filter(|e| e.name == "triangle").count(), 24);
    // zweiter Körper verschoben; Punkte mit sechs Nachkommastellen
    assert_eq!(attr(vertices[9], "x"), "6.000000");
    for v in &vertices {
        for k in ["x", "y", "z"] {
            let s = attr(v, k);
            assert_eq!(s.len() - s.find('.').unwrap() - 1, 6, "{s}");
        }
    }
    for t in xml.iter().filter(|e| e.name == "triangle") {
        for k in ["v1", "v2", "v3"] {
            assert!(attr(t, k).parse::<u32>().unwrap() < 8);
        }
    }
}

#[test]
fn empty_bodies_are_left_out_and_nothing_is_an_error() {
    let mut out = Vec::new();
    assert_eq!(write_3mf(&mut out, &[]), Err(ExchangeError::Empty));
    let b = [
        ExportBody {
            name: "leer".into(),
            mesh: ExchangeMesh::default(),
        },
        ExportBody {
            name: "voll".into(),
            mesh: cube(0.0),
        },
    ];
    let xml = model_of(&write(&b));
    assert_eq!(xml.iter().filter(|e| e.name == "object").count(), 1);
    assert_eq!(xml.iter().filter(|e| e.name == "item").count(), 1);
}

#[test]
fn control_characters_in_names_do_not_break_the_xml() {
    let b = [ExportBody {
        name: "a\u{1}b\nc".into(),
        mesh: cube(0.0),
    }];
    let xml = model_of(&write(&b));
    let object = xml.iter().find(|e| e.name == "object").unwrap();
    assert_eq!(attr(object, "name"), "a b c");
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test threemf`
Expected: FAIL – `cannot find function write_3mf`.

- [ ] **Step 4: Implementieren**

`crates/planora-exchange/src/threemf.rs`:

```rust
//! 3MF schreiben (Spec §3, Plan P7): ZIP mit `[Content_Types].xml`, `_rels/.rels` und
//! `3D/3dmodel.model`; ein `object` je Körper (`type="model"`), ein `item` je Objekt,
//! Millimeter, Punkte mit sechs Nachkommastellen.

use crate::error::ExchangeError;
use crate::source::ExportBody;
use crate::zip::ZipWriter;
use std::io::{self, Write};

const CONTENT_TYPES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "\n",
    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>"#,
    "</Types>\n"
);

const RELS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "\n",
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>"#,
    "</Relationships>\n"
);

/// Schreibt alle Körper mit Dreiecken als 3MF; Körper ohne Dreiecke entfallen.
pub fn write_3mf<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
    let used: Vec<&ExportBody> = bodies
        .iter()
        .filter(|b| b.mesh.triangle_count() > 0)
        .collect();
    if used.is_empty() {
        return Err(ExchangeError::Empty);
    }
    let mut zip = ZipWriter::new(w);
    zip.add_entry("[Content_Types].xml", &|o| o.write_all(CONTENT_TYPES.as_bytes()))?;
    zip.add_entry("_rels/.rels", &|o| o.write_all(RELS.as_bytes()))?;
    zip.add_entry("3D/3dmodel.model", &|o| write_model(o, &used))?;
    zip.finish()?;
    Ok(())
}

fn write_model(out: &mut dyn Write, bodies: &[&ExportBody]) -> io::Result<()> {
    let mut o = io::BufWriter::new(out);
    writeln!(o, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
    writeln!(
        o,
        r#"<model unit="millimeter" xml:lang="de-DE" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">"#
    )?;
    writeln!(o, r#"<metadata name="Application">PlanoraCAD</metadata>"#)?;
    writeln!(o, "<resources>")?;
    for (k, body) in bodies.iter().enumerate() {
        writeln!(
            o,
            r#"<object id="{}" type="model" name="{}">"#,
            k + 1,
            xml_escape(&body.name)
        )?;
        writeln!(o, "<mesh>")?;
        writeln!(o, "<vertices>")?;
        for p in &body.mesh.positions {
            writeln!(
                o,
                r#"<vertex x="{:.6}" y="{:.6}" z="{:.6}"/>"#,
                f64::from(p[0]),
                f64::from(p[1]),
                f64::from(p[2])
            )?;
        }
        writeln!(o, "</vertices>")?;
        writeln!(o, "<triangles>")?;
        for t in body.mesh.indices.as_chunks::<3>().0 {
            writeln!(
                o,
                r#"<triangle v1="{}" v2="{}" v3="{}"/>"#,
                t[0], t[1], t[2]
            )?;
        }
        writeln!(o, "</triangles>")?;
        writeln!(o, "</mesh>")?;
        writeln!(o, "</object>")?;
    }
    writeln!(o, "</resources>")?;
    writeln!(o, "<build>")?;
    for k in 1..=bodies.len() {
        writeln!(o, r#"<item objectid="{k}"/>"#)?;
    }
    writeln!(o, "</build>")?;
    writeln!(o, "</model>")?;
    o.flush()
}

/// Maskiert Text für XML-Attribute; Steuerzeichen (in XML 1.0 verboten) werden Leerzeichen.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
```

In `crates/planora-exchange/src/lib.rs` nach `mod text;` die Zeile `mod threemf;` einfügen und `pub use threemf::write_3mf;` ergänzen.

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (4 Tests in `threemf`, `zip` weiter grün).

```bash
git add crates/planora-exchange/src/lib.rs crates/planora-exchange/src/threemf.rs crates/planora-exchange/tests/common/mod.rs crates/planora-exchange/tests/threemf.rs
git commit -m "exchange: 3MF schreiben, mit eigenem ZIP- und XML-Mini-Leser geprüft

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Normalen mit harten Kanten und glTF (.glb)

**Files:**
- Create: `crates/planora-exchange/src/normals.rs`, `crates/planora-exchange/src/gltf.rs`
- Modify: `crates/planora-exchange/src/lib.rs`, `crates/planora-exchange/Cargo.toml` (`serde_json` als Testabhängigkeit; schon im Arbeitsbereich, Lizenz MIT OR Apache-2.0)
- Test: `crates/planora-exchange/tests/shade.rs`, `crates/planora-exchange/tests/gltf.rs`

**Interfaces:**
- Consumes: Task 1, Task 2 (`ExportBody`).
- Produces:
  ```rust
  pub const SMOOTH_ANGLE_DEG: f64 = 30.0;
  pub struct Shaded { pub positions: Vec<[f32; 3]>, pub normals: Vec<[f32; 3]>, pub indices: Vec<u32> }  // Clone, Debug, Default, PartialEq
  pub fn shade(mesh: &ExchangeMesh, max_angle_deg: f64) -> Shaded;
  pub const GLTF_SCALE: f64 = 0.001;
  pub fn write_glb<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError>;
  ```

- [ ] **Step 1: Testabhängigkeit eintragen**

In `crates/planora-exchange/Cargo.toml` unter `[dev-dependencies]` ergänzen:

```toml
serde_json.workspace = true
```

- [ ] **Step 2: Tests für die Normalen schreiben** (`crates/planora-exchange/tests/shade.rs`)

```rust
//! Normalen je Punkt mit harten Kanten (Teilprojekt 9, Task 8).

use planora_exchange::*;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}

fn is_unit(n: [f32; 3]) -> bool {
    ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-6
}

#[test]
fn a_cube_gets_hard_edges() {
    let c = cube();
    let s = shade(&c, SMOOTH_ANGLE_DEG);
    assert_eq!(
        (s.positions.len(), s.normals.len(), s.indices.len()),
        (24, 24, 36)
    );
    for (t, orig) in s
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .zip(c.indices.as_chunks::<3>().0)
    {
        let [a, b, d] = orig.map(|i| c.point(i as usize));
        let face = (b - a).cross(d - a).normalize();
        for (&i, &o) in t.iter().zip(orig) {
            let n = s.normals[i as usize];
            let diff = (f64::from(n[0]) - face.x).abs()
                + (f64::from(n[1]) - face.y).abs()
                + (f64::from(n[2]) - face.z).abs();
            assert!(diff < 1e-6, "{n:?} / {face}");
            assert_eq!(s.positions[i as usize], c.positions[o as usize], "Lage bleibt");
        }
    }
}

#[test]
fn a_tetrahedron_gets_three_points_per_corner() {
    let s = shade(&tetra(), SMOOTH_ANGLE_DEG);
    assert_eq!(s.positions.len(), 12);
    assert!(s.normals.iter().all(|&n| is_unit(n)));
}

#[test]
fn a_flat_fold_stays_smooth_and_a_sharp_fold_splits() {
    // Zwei Dreiecke an der Kante 0–1; das zweite steigt um den Winkel atan(h) an.
    let fold = |h: f32| ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.5, 1.0, 0.0],
            [0.5, -1.0, h],
        ],
        indices: vec![0, 1, 2, 1, 0, 3],
    };
    let flat = shade(&fold(10f32.to_radians().tan()), SMOOTH_ANGLE_DEG);
    assert_eq!(flat.positions.len(), 4, "10° Knick bleibt glatt");
    let sharp = shade(&fold(60f32.to_radians().tan()), SMOOTH_ANGLE_DEG);
    assert_eq!(sharp.positions.len(), 6, "60° Knick wird hart");
    assert!(flat.normals.iter().chain(&sharp.normals).all(|&n| is_unit(n)));
}

#[test]
fn degenerate_triangles_do_not_change_their_neighbours() {
    let mut m = cube();
    m.indices.extend_from_slice(&[0, 0, 1]);
    let s = shade(&m, SMOOTH_ANGLE_DEG);
    let plain = shade(&cube(), SMOOTH_ANGLE_DEG);
    assert_eq!(s.indices.len(), 39);
    for (a, b) in s.indices[..36].iter().zip(&plain.indices) {
        assert_eq!(s.normals[*a as usize], plain.normals[*b as usize]);
    }
}
```

- [ ] **Step 3: Tests für glTF schreiben** (`crates/planora-exchange/tests/gltf.rs`)

```rust
//! glTF (.glb): Kopf, JSON-Block, Puffergrößen, Zugriffsgrenzen und min/max für POSITION
//! (Teilprojekt 9, Task 8).

use planora_exchange::*;
use serde_json::Value;

/// Einheitswürfel, um `shift` in x verschoben.
fn cube(shift: f32) -> ExchangeMesh {
    let mut m = ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    };
    for p in &mut m.positions {
        p[0] += shift;
    }
    m
}

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}

struct Glb {
    json: Value,
    bin: Vec<u8>,
}

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

/// Liest eine .glb streng: Kopf, ein JSON-Block, ein BIN-Block, Längen passend.
fn parse_glb(b: &[u8]) -> Glb {
    assert_eq!(&b[..4], b"glTF");
    assert_eq!(u32_at(b, 4), 2, "Version");
    assert_eq!(u32_at(b, 8), b.len(), "Gesamtlänge");
    let json_len = u32_at(b, 12);
    assert_eq!(u32_at(b, 16), 0x4E4F_534A, "JSON-Block");
    assert!(json_len.is_multiple_of(4));
    let json: Value = serde_json::from_slice(&b[20..20 + json_len]).unwrap();
    let at = 20 + json_len;
    let bin_len = u32_at(b, at);
    assert_eq!(u32_at(b, at + 4), 0x004E_4942, "BIN-Block");
    assert_eq!(at + 8 + bin_len, b.len());
    Glb {
        json,
        bin: b[at + 8..].to_vec(),
    }
}

fn number(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}

/// Die Bytes, auf die ein Zugriff zeigt.
fn view_bytes<'a>(g: &'a Glb, accessor: &Value) -> &'a [u8] {
    let view = &g.json["bufferViews"][number(&accessor["bufferView"])];
    let start = number(&view["byteOffset"]);
    &g.bin[start..start + number(&view["byteLength"])]
}

fn write(bodies: &[ExportBody]) -> Glb {
    let mut out = Vec::new();
    write_glb(&mut out, bodies).unwrap();
    parse_glb(&out)
}

#[test]
fn header_chunks_buffers_and_accessors_are_consistent() {
    let bodies = [
        ExportBody {
            name: "Würfel \"A\"".into(),
            mesh: cube(0.0),
        },
        ExportBody {
            name: "B".into(),
            mesh: tetra(),
        },
    ];
    let g = write(&bodies);
    let j = &g.json;
    assert_eq!(j["asset"]["version"], "2.0");
    assert_eq!(j["asset"]["generator"], "PlanoraCAD");
    assert_eq!(number(&j["buffers"][0]["byteLength"]), g.bin.len());
    assert_eq!(j["scenes"][0]["nodes"], serde_json::json!([0, 1]));
    assert_eq!(j["nodes"][0]["name"], "Würfel \"A\"");
    assert_eq!(j["nodes"][1]["mesh"], 1);
    for v in j["bufferViews"].as_array().unwrap() {
        assert!(number(&v["byteOffset"]) + number(&v["byteLength"]) <= g.bin.len());
        assert!(number(&v["byteOffset"]).is_multiple_of(4));
    }
    let meshes = j["meshes"].as_array().unwrap();
    assert_eq!(meshes.len(), 2);
    for (k, mesh) in meshes.iter().enumerate() {
        let prim = &mesh["primitives"][0];
        assert_eq!(prim["mode"], 4);
        let pos = &j["accessors"][number(&prim["attributes"]["POSITION"])];
        let nor = &j["accessors"][number(&prim["attributes"]["NORMAL"])];
        let idx = &j["accessors"][number(&prim["indices"])];
        assert_eq!(pos["componentType"], 5126);
        assert_eq!(idx["componentType"], 5125);
        let count = number(&pos["count"]);
        assert_eq!(number(&nor["count"]), count);
        let floats: Vec<f32> = view_bytes(&g, pos)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        assert_eq!(floats.len(), 3 * count);
        for axis in 0..3 {
            let values: Vec<f32> = floats.iter().skip(axis).step_by(3).copied().collect();
            let min = values.iter().copied().fold(f32::MAX, f32::min);
            let max = values.iter().copied().fold(f32::MIN, f32::max);
            assert_eq!(pos["min"][axis].as_f64().unwrap() as f32, min);
            assert_eq!(pos["max"][axis].as_f64().unwrap() as f32, max);
        }
        let normals: Vec<f32> = view_bytes(&g, nor)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        for n in normals.as_chunks::<3>().0 {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "{n:?}");
        }
        let indices: Vec<u32> = view_bytes(&g, idx)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        assert_eq!(indices.len(), number(&idx["count"]));
        assert_eq!(indices.len(), 3 * bodies[k].mesh.triangle_count());
        assert!(indices.iter().all(|&i| (i as usize) < count));
    }
    // Würfel: harte Kanten, drei Punkte je Ecke
    assert_eq!(number(&j["accessors"][0]["count"]), 24);
}

#[test]
fn positions_are_written_in_metres_and_nothing_is_an_error() {
    let g = write(&[ExportBody {
        name: "W".into(),
        mesh: cube(5.0),
    }]);
    let max_x = g.json["accessors"][0]["max"][0].as_f64().unwrap();
    assert!((max_x - 0.006).abs() < 1e-9, "{max_x}");
    assert!((GLTF_SCALE - 0.001).abs() < f64::EPSILON);
    let mut out = Vec::new();
    assert_eq!(write_glb(&mut out, &[]), Err(ExchangeError::Empty));
    let empty = [ExportBody {
        name: "leer".into(),
        mesh: ExchangeMesh::default(),
    }];
    assert_eq!(write_glb(&mut out, &empty), Err(ExchangeError::Empty));
}

#[test]
fn names_are_escaped_for_json() {
    let name = "a\\b\u{1}\"c";
    let g = write(&[ExportBody {
        name: name.into(),
        mesh: tetra(),
    }]);
    assert_eq!(g.json["meshes"][0]["name"], name);
}
```

- [ ] **Step 4: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test shade --test gltf`
Expected: FAIL – `cannot find function shade` / `write_glb`.

- [ ] **Step 5: Implementieren**

`crates/planora-exchange/src/normals.rs`:

```rust
//! Normalen je Punkt mit harten Kanten (Spec §6, Plan P8): An einem Punkt teilen sich die
//! anliegenden Dreiecke eine Normale, solange ihre Flächennormalen weniger als den Grenzwinkel
//! von der des ersten Dreiecks der Gruppe abweichen; sonst bekommt der Punkt eine weitere Kopie.

use crate::mesh::ExchangeMesh;
use planora_math::DVec3;

/// Grenzwinkel für glatte Normalen (Spec §6: unter 30° glatt, sonst harte Kante).
pub const SMOOTH_ANGLE_DEG: f64 = 30.0;

/// Netz mit einer Normale je Punkt; Punkte an harten Kanten sind verdoppelt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shaded {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Normalen je Punkt (Netz muss gültig sein). Gruppen entstehen in der Reihenfolge der Ecken;
/// die Normale einer Gruppe ist die nach Fläche gewichtete Summe, normiert (ohne Fläche +z).
pub fn shade(mesh: &ExchangeMesh, max_angle_deg: f64) -> Shaded {
    let tris = mesh.indices.as_chunks::<3>().0;
    let cos_limit = max_angle_deg.to_radians().cos();
    let weighted: Vec<DVec3> = tris
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.point(i as usize));
            (b - a).cross(c - a)
        })
        .collect();
    let unit: Vec<DVec3> = weighted.iter().map(|n| n.normalize_or_zero()).collect();
    // Ecken je Punkt: Punkt v hat die Ecken corners[start[v]..start[v + 1]].
    let mut start = vec![0usize; mesh.positions.len() + 1];
    for &i in &mesh.indices {
        start[i as usize] += 1;
    }
    let mut sum = 0;
    for s in &mut start {
        let count = *s;
        *s = sum;
        sum += count;
    }
    let mut fill = start.clone();
    let mut corners = vec![0usize; mesh.indices.len()];
    for (c, &i) in mesh.indices.iter().enumerate() {
        corners[fill[i as usize]] = c;
        fill[i as usize] += 1;
    }
    let mut out = Shaded::default();
    let mut corner_vertex = vec![0u32; mesh.indices.len()];
    let mut group_of = vec![0usize; mesh.indices.len()];
    // Gruppen eines Punkts: Einheitsnormale des ersten Dreiecks, Summe der Flächennormalen.
    let mut groups: Vec<(DVec3, DVec3)> = Vec::new();
    for (v, range) in start.windows(2).enumerate() {
        let here = &corners[range[0]..range[1]];
        groups.clear();
        for &c in here {
            let f = c / 3;
            let g = match groups
                .iter()
                .position(|(seed, _)| seed.dot(unit[f]) >= cos_limit)
            {
                Some(g) => g,
                None => {
                    groups.push((unit[f], DVec3::ZERO));
                    groups.len() - 1
                }
            };
            groups[g].1 += weighted[f];
            group_of[c] = g;
        }
        let first = out.positions.len() as u32;
        for (_, n) in &groups {
            let u = n.try_normalize().unwrap_or(DVec3::Z);
            out.positions.push(mesh.positions[v]);
            out.normals.push([u.x as f32, u.y as f32, u.z as f32]);
        }
        for &c in here {
            corner_vertex[c] = first + group_of[c] as u32;
        }
    }
    out.indices = corner_vertex;
    out
}
```

`crates/planora-exchange/src/gltf.rs`:

```rust
//! glTF 2.0 als `.glb` schreiben (Spec §3, Plan P8): ein Puffer, je Körper ein Mesh mit
//! `POSITION`, `NORMAL` und `indices` (u32), ein Knoten je Mesh; Meter (= mm · 0,001).

use crate::error::ExchangeError;
use crate::normals::{SMOOTH_ANGLE_DEG, Shaded, shade};
use crate::source::ExportBody;
use std::fmt::Write as _;
use std::io::Write;

/// Millimeter → Meter (glTF rechnet in Metern, Spec §2).
pub const GLTF_SCALE: f64 = 0.001;
/// „glTF“ als Zahl (little endian).
const GLB_MAGIC: u32 = 0x4654_6C67;
const GLB_VERSION: u32 = 2;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;
/// Länge des Dateikopfs und eines Blockkopfs in Bytes.
const GLB_HEADER: u64 = 12;
const CHUNK_HEADER: u64 = 8;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;

/// Schreibt alle Körper mit Dreiecken als `.glb`; Körper ohne Dreiecke entfallen.
pub fn write_glb<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
    let parts: Vec<(&str, Shaded)> = bodies
        .iter()
        .filter(|b| b.mesh.triangle_count() > 0)
        .map(|b| (b.name.as_str(), in_metres(shade(&b.mesh, SMOOTH_ANGLE_DEG))))
        .collect();
    if parts.is_empty() {
        return Err(ExchangeError::Empty);
    }
    let too_large = || ExchangeError::TooLargeForFormat { format: "glTF" };
    let (mut views, mut accessors, mut meshes, mut nodes) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut offset: u64 = 0;
    for (k, (name, s)) in parts.iter().enumerate() {
        let v = s.positions.len();
        let count = s.indices.len();
        let vbytes = 12 * v as u64;
        let ibytes = 4 * count as u64;
        let normals_at = offset + vbytes;
        let indices_at = offset + 2 * vbytes;
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{vbytes},"target":{ARRAY_BUFFER}}}"#
        ));
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{normals_at},"byteLength":{vbytes},"target":{ARRAY_BUFFER}}}"#
        ));
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{indices_at},"byteLength":{ibytes},"target":{ELEMENT_ARRAY_BUFFER}}}"#
        ));
        offset += 2 * vbytes + ibytes;
        let (a, b, c) = (3 * k, 3 * k + 1, 3 * k + 2);
        let (lo, hi) = bounds(&s.positions);
        accessors.push(format!(
            r#"{{"bufferView":{a},"componentType":{FLOAT},"count":{v},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}}"#,
            lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
        ));
        accessors.push(format!(
            r#"{{"bufferView":{b},"componentType":{FLOAT},"count":{v},"type":"VEC3"}}"#
        ));
        accessors.push(format!(
            r#"{{"bufferView":{c},"componentType":{UNSIGNED_INT},"count":{count},"type":"SCALAR"}}"#
        ));
        let name = json_string(name);
        meshes.push(format!(
            r#"{{"name":{name},"primitives":[{{"attributes":{{"POSITION":{a},"NORMAL":{b}}},"indices":{c},"mode":4}}]}}"#
        ));
        nodes.push(format!(r#"{{"mesh":{k},"name":{name}}}"#));
    }
    let scene: Vec<String> = (0..parts.len()).map(|k| k.to_string()).collect();
    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"PlanoraCAD"}},"scene":0,"scenes":[{{"nodes":[{}]}}],"nodes":[{}],"meshes":[{}],"accessors":[{}],"bufferViews":[{}],"buffers":[{{"byteLength":{offset}}}]}}"#,
        scene.join(","),
        nodes.join(","),
        meshes.join(","),
        accessors.join(","),
        views.join(",")
    );
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let json_len = u32::try_from(json.len()).map_err(|_| too_large())?;
    let bin_len = u32::try_from(offset).map_err(|_| too_large())?;
    let total = u32::try_from(GLB_HEADER + 2 * CHUNK_HEADER + json.len() as u64 + offset)
        .map_err(|_| too_large())?;
    w.write_all(&GLB_MAGIC.to_le_bytes())?;
    w.write_all(&GLB_VERSION.to_le_bytes())?;
    w.write_all(&total.to_le_bytes())?;
    w.write_all(&json_len.to_le_bytes())?;
    w.write_all(&CHUNK_JSON.to_le_bytes())?;
    w.write_all(&json)?;
    w.write_all(&bin_len.to_le_bytes())?;
    w.write_all(&CHUNK_BIN.to_le_bytes())?;
    for (_, s) in &parts {
        for p in s.positions.iter().chain(&s.normals) {
            for c in p {
                w.write_all(&c.to_le_bytes())?;
            }
        }
        for i in &s.indices {
            w.write_all(&i.to_le_bytes())?;
        }
    }
    Ok(())
}

/// Punkte in Meter (Normalen bleiben).
fn in_metres(mut s: Shaded) -> Shaded {
    for p in &mut s.positions {
        *p = p.map(|c| (f64::from(c) * GLTF_SCALE) as f32);
    }
    s
}

/// Kleinste und größte Koordinate je Achse der geschriebenen Werte.
fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in points {
        for ((l, h), &c) in lo.iter_mut().zip(hi.iter_mut()).zip(p) {
            *l = l.min(c);
            *h = h.max(c);
        }
    }
    (lo, hi)
}

/// JSON-Zeichenkette mit Anführungszeichen; Steuerzeichen als `\u00XX`.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
```

Hinweis: Die Reihenfolge im Puffer je Körper ist Punkte, Normalen, Punktnummern (wie die drei `bufferViews`); `s.positions.iter().chain(&s.normals)` schreibt die ersten beiden direkt hintereinander.

In `crates/planora-exchange/src/lib.rs` die Zeilen `mod gltf;` (nach `pub mod error;`) und `mod normals;` (nach `mod mesh;`) einfügen und ergänzen:

```rust
pub use gltf::{GLTF_SCALE, write_glb};
pub use normals::{SMOOTH_ANGLE_DEG, Shaded, shade};
```

- [ ] **Step 6: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (4 Tests in `shade`, 3 in `gltf`).

```bash
git add Cargo.lock crates/planora-exchange/Cargo.toml crates/planora-exchange/src/lib.rs crates/planora-exchange/src/normals.rs crates/planora-exchange/src/gltf.rs crates/planora-exchange/tests/shade.rs crates/planora-exchange/tests/gltf.rs
git commit -m "exchange: Normalen mit harten Kanten und glTF als .glb

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Dateien (Format, Größenschätzung, Hilfsdatei) und Stufentest 2

**Files:**
- Create: `crates/planora-exchange/src/file.rs`
- Modify: `crates/planora-exchange/src/lib.rs` (vollständig)
- Test: `crates/planora-exchange/tests/stage2.rs`

**Interfaces:**
- Consumes: alle Schreiber und Leser aus Task 3, 4, 7, 8.
- Produces:
  ```rust
  pub enum Format { Stl, Obj, ThreeMf, Glb }        // Clone, Copy, Debug, PartialEq, Eq, Hash
  impl Format { pub const ALL: [Format; 4]; pub fn extension(self) -> &'static str; pub fn label(self) -> &'static str;
      pub fn estimate_bytes(self, triangles: u64, vertices: u64) -> u64;
      pub fn write<W: Write>(self, w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError>; }
  pub fn export_file(path: &Path, format: Format, bodies: &[ExportBody]) -> Result<u64, ExchangeError>;
  pub fn import_file(path: &Path, progress: &Progress) -> Result<ExchangeMesh, ExchangeError>;
  pub fn temp_path(path: &Path) -> PathBuf;
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-exchange/tests/stage2.rs`)

```rust
//! Stufentest 2: alle Formate über Dateien, Größenschätzung, Hilfsdatei und Endungen
//! (Teilprojekt 9, Task 9).

mod common;

use planora_exchange::*;
use planora_kernel::{AnalyticKernel, Kernel};
use planora_mesh::{TessellationOptions, tessellate};
use std::fs;
use std::path::{Path, PathBuf};

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn cube_body() -> ExportBody {
    ExportBody {
        name: "Würfel".into(),
        mesh: cube(),
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-exchange-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn every_format_writes_a_readable_file_near_its_estimate() {
    let k = AnalyticKernel::default();
    let solids = [
        ("Kugel", k.make_sphere(6.0).unwrap()),
        ("Zylinder", k.make_cylinder(3.0, 8.0).unwrap()),
    ];
    let bodies: Vec<ExportBody> = solids
        .iter()
        .map(|(name, s)| {
            let m = tessellate(
                s,
                &TessellationOptions {
                    chord_tolerance: 0.01,
                },
            )
            .unwrap();
            collect_body(&SolidSource { name, mesh: &m }).unwrap()
        })
        .collect();
    let t: u64 = bodies.iter().map(|b| b.mesh.triangle_count() as u64).sum();
    let v: u64 = bodies.iter().map(|b| b.mesh.positions.len() as u64).sum();
    let dir = temp_dir("formate");
    for format in Format::ALL {
        let path = dir.join(format!("körper.{}", format.extension()));
        let len = export_file(&path, format, &bodies).unwrap();
        assert_eq!(len, fs::metadata(&path).unwrap().len());
        assert!(!temp_path(&path).exists(), "Hilfsdatei entfernt");
        let estimate = format.estimate_bytes(t, v);
        assert!(
            len as f64 >= 0.5 * estimate as f64 && len as f64 <= 2.0 * estimate as f64,
            "{format:?}: {len} Bytes, geschätzt {estimate}"
        );
        let bytes = fs::read(&path).unwrap();
        match format {
            Format::Stl | Format::Obj => {
                let m = import_file(&path, &Progress::new()).unwrap();
                assert_eq!(m.triangle_count() as u64, t);
            }
            Format::ThreeMf => {
                let entries = common::read_zip(&bytes).unwrap();
                let xml = common::parse_xml(std::str::from_utf8(&entries[2].data).unwrap()).unwrap();
                assert_eq!(xml.iter().filter(|x| x.name == "triangle").count() as u64, t);
                assert_eq!(xml.iter().filter(|x| x.name == "vertex").count() as u64, v);
            }
            Format::Glb => {
                assert_eq!(&bytes[..4], b"glTF");
                let total = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
                assert_eq!(total as usize, bytes.len());
            }
        }
    }
    assert_eq!(Format::Stl.estimate_bytes(12, 8), 84 + 50 * 12);
    let labels: Vec<&str> = Format::ALL.iter().map(|f| f.label()).collect();
    assert_eq!(labels, ["STL", "OBJ", "3MF", "glTF (.glb)"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_export_keeps_the_old_file_and_leaves_no_helper() {
    let dir = temp_dir("fehler");
    let path = dir.join("alt.stl");
    fs::write(&path, b"alter Inhalt").unwrap();
    assert_eq!(export_file(&path, Format::Stl, &[]), Err(ExchangeError::Empty));
    let empty = [ExportBody {
        name: "leer".into(),
        mesh: ExchangeMesh::default(),
    }];
    for format in Format::ALL {
        assert_eq!(export_file(&path, format, &empty), Err(ExchangeError::Empty));
    }
    assert_eq!(fs::read(&path).unwrap(), b"alter Inhalt");
    assert!(!temp_path(&path).exists());
    // Ordner gibt es nicht: Dateifehler, nichts angelegt
    let missing = dir.join("fehlt").join("x.stl");
    assert!(matches!(
        export_file(&missing, Format::Stl, &[cube_body()]),
        Err(ExchangeError::Io { .. })
    ));
    // Ziel ist ein Ordner: Umbenennen scheitert, Hilfsdatei entfernt, Ordner bleibt
    let target_dir = dir.join("ordner.stl");
    fs::create_dir_all(&target_dir).unwrap();
    assert!(export_file(&target_dir, Format::Stl, &[cube_body()]).is_err());
    assert!(target_dir.is_dir());
    assert!(!temp_path(&target_dir).exists());
    // Überschreiben einer vorhandenen Datei klappt
    export_file(&path, Format::Stl, &[cube_body()]).unwrap();
    assert_eq!(fs::read(&path).unwrap().len(), 84 + 50 * 12);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_endings_and_missing_files_are_named() {
    assert_eq!(
        import_file(Path::new("x.step"), &Progress::new()),
        Err(ExchangeError::UnknownFormat("step".into()))
    );
    assert!(matches!(
        import_file(Path::new("gibt-es-nicht.stl"), &Progress::new()),
        Err(ExchangeError::Io { .. })
    ));
    let dir = temp_dir("endung");
    let path = dir.join("W.STL");
    export_file(&path, Format::Stl, &[cube_body()]).unwrap();
    assert_eq!(import_file(&path, &Progress::new()).unwrap().triangle_count(), 12);
    let obj = dir.join("W.Obj");
    export_file(&obj, Format::Obj, &[cube_body()]).unwrap();
    assert!(check_mesh(&import_file(&obj, &Progress::new()).unwrap()).closed);
    assert_eq!(
        temp_path(&path).file_name().unwrap().to_string_lossy(),
        ".W.STL.planora-tmp"
    );
    let _ = fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test stage2`
Expected: FAIL – `cannot find type Format`.

- [ ] **Step 3: Implementieren**

`crates/planora-exchange/src/file.rs`:

```rust
//! Dateien (Plan P9): Formate mit Endung und Größenschätzung, Schreiben über eine Hilfsdatei
//! im Zielordner (Spec §5) und Lesen nach der Endung.

use crate::error::ExchangeError;
use crate::gltf::write_glb;
use crate::limits::MAX_FILE_BYTES;
use crate::mesh::ExchangeMesh;
use crate::obj::{read_obj, write_obj};
use crate::progress::Progress;
use crate::source::ExportBody;
use crate::stl::{STL_HEADER_BYTES, STL_TRIANGLE_BYTES, read_stl, write_stl};
use crate::threemf::write_3mf;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Ausgabeformate (Spec E2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Stl,
    Obj,
    ThreeMf,
    Glb,
}

// Schätzwerte für die Dateigröße (Spec E5, „ca. x MB“), in Bytes je Punkt und je Dreieck.
// Gemessen an Kugel und Zylinder mit sechsstelligen Koordinaten (Stufentest 2) und gerundet;
// die Tests verlangen das Ergebnis innerhalb von Faktor 2.
/// Kopfzeile „# PlanoraCAD“.
const OBJ_BASE: u64 = 13;
/// Zeilen „v …“ und „vn …“ je Punkt.
const OBJ_PER_VERTEX: u64 = 70;
/// Zeile „f a//a b//b c//c“ je Dreieck.
const OBJ_PER_TRIANGLE: u64 = 45;
/// Paketdateien, ZIP-Köpfe und XML-Rahmen.
const THREEMF_BASE: u64 = 1500;
const THREEMF_PER_VERTEX: u64 = 56;
const THREEMF_PER_TRIANGLE: u64 = 50;
/// Köpfe und JSON.
const GLB_BASE: u64 = 1024;
/// Punkt und Normale (24 Bytes), dazu Kopien an harten Kanten.
const GLB_PER_VERTEX: u64 = 36;
/// Drei Punktnummern zu 4 Bytes.
const GLB_PER_TRIANGLE: u64 = 12;

impl Format {
    pub const ALL: [Format; 4] = [Format::Stl, Format::Obj, Format::ThreeMf, Format::Glb];

    /// Dateiendung ohne Punkt.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Stl => "stl",
            Format::Obj => "obj",
            Format::ThreeMf => "3mf",
            Format::Glb => "glb",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Stl => "STL",
            Format::Obj => "OBJ",
            Format::ThreeMf => "3MF",
            Format::Glb => "glTF (.glb)",
        }
    }

    /// Geschätzte Dateigröße in Bytes (STL genau).
    pub fn estimate_bytes(self, triangles: u64, vertices: u64) -> u64 {
        match self {
            Format::Stl => STL_HEADER_BYTES + STL_TRIANGLE_BYTES * triangles,
            Format::Obj => OBJ_BASE + OBJ_PER_VERTEX * vertices + OBJ_PER_TRIANGLE * triangles,
            Format::ThreeMf => {
                THREEMF_BASE + THREEMF_PER_VERTEX * vertices + THREEMF_PER_TRIANGLE * triangles
            }
            Format::Glb => GLB_BASE + GLB_PER_VERTEX * vertices + GLB_PER_TRIANGLE * triangles,
        }
    }

    pub fn write<W: Write>(self, w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
        match self {
            Format::Stl => write_stl(w, bodies),
            Format::Obj => write_obj(w, bodies),
            Format::ThreeMf => write_3mf(w, bodies),
            Format::Glb => write_glb(w, bodies),
        }
    }
}

/// Schreibt `bodies` im Format `format` nach `path`: erst in die Hilfsdatei [`temp_path`],
/// `sync_all`, dann umbenennen. Bei jedem Fehler bleibt eine vorhandene Zieldatei unverändert
/// und die Hilfsdatei wird entfernt (Spec §5). Liefert die Dateigröße in Bytes.
pub fn export_file(
    path: &Path,
    format: Format,
    bodies: &[ExportBody],
) -> Result<u64, ExchangeError> {
    if bodies.iter().all(|b| b.mesh.triangle_count() == 0) {
        return Err(ExchangeError::Empty);
    }
    let tmp = temp_path(path);
    let result = write_to(&tmp, format, bodies)
        .and_then(|len| fs::rename(&tmp, path).map(|()| len).map_err(ExchangeError::from));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_to(tmp: &Path, format: Format, bodies: &[ExportBody]) -> Result<u64, ExchangeError> {
    let mut w = BufWriter::new(File::create(tmp)?);
    format.write(&mut w, bodies)?;
    let file = w
        .into_inner()
        .map_err(|e| ExchangeError::from(e.into_error()))?;
    file.sync_all()?;
    Ok(file.metadata()?.len())
}

/// Hilfsdatei `.<Name>.planora-tmp` im Ordner des Ziels.
pub fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.planora-tmp"))
}

/// Liest ein Netz nach der Dateiendung (`.stl` oder `.obj`, Groß- und Kleinschreibung egal).
pub fn import_file(path: &Path, progress: &Progress) -> Result<ExchangeMesh, ExchangeError> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext != "stl" && ext != "obj" {
        return Err(ExchangeError::UnknownFormat(ext));
    }
    let len = fs::metadata(path)?.len();
    if len > MAX_FILE_BYTES {
        return Err(ExchangeError::FileTooLarge { bytes: len });
    }
    let file = File::open(path)?;
    if ext == "stl" {
        read_stl(file, len, progress)
    } else {
        read_obj(file, len, progress)
    }
}
```

`crates/planora-exchange/src/lib.rs` vollständig ersetzen:

```rust
//! Austausch von Dreiecksnetzen (Teilprojekt 9): STL, OBJ, 3MF und glTF schreiben, STL und OBJ
//! lesen, Netze prüfen. Keine neue Abhängigkeit: ZIP und CRC-32 sind selbst geschrieben,
//! Vielecke zerlegt das schon vorhandene `earcutr`.

mod check;
pub mod error;
mod file;
mod gltf;
pub mod limits;
mod mesh;
mod normals;
mod obj;
mod progress;
mod source;
mod stl;
mod text;
mod threemf;
mod zip;

pub use check::{MeshReport, check_mesh, weld_exact};
pub use error::ExchangeError;
pub use file::{Format, export_file, import_file, temp_path};
pub use gltf::{GLTF_SCALE, write_glb};
pub use limits::{
    CHECK_EVERY, MAX_FILE_BYTES, MAX_LINE_BYTES, MAX_TRIANGLES, MAX_VERTICES, READ_BUFFER,
    RESERVE_STEP,
};
pub use mesh::{ExchangeMesh, MeshTransform};
pub use normals::{SMOOTH_ANGLE_DEG, Shaded, shade};
pub use obj::{read_obj, write_obj};
pub use progress::Progress;
pub use source::{ExportBody, PlacedSource, SolidSource, TriSource, collect_body};
pub use stl::{STL_HEADER_BYTES, STL_TRIANGLE_BYTES, read_stl, write_stl};
pub use threemf::write_3mf;
pub use zip::{Content, Crc32, ZipWriter, crc32};
```

- [ ] **Step 4: Tests prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange`
Expected: PASS (3 Tests in `stage2`). Liegt ein Format außerhalb von Faktor 2 der Schätzung, die gemessene Größe im Bericht nennen und die Konstante dieses Formats in `file.rs` auf den gemessenen Wert je Punkt/Dreieck setzen (das ist die Schätzung, keine Testschranke); die Schranke 0,5 bis 2 bleibt.

- [ ] **Step 5: Stufenabschluss 2 und Commit**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
git diff --stat 134cde9 -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs
```
Expected: alles grün; die letzte Ausgabe ist leer.

```bash
git add crates/planora-exchange/src/lib.rs crates/planora-exchange/src/file.rs crates/planora-exchange/tests/stage2.rs
git commit -m "exchange: Dateien mit Hilfsdatei, Größenschätzung und Stufentest 2

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Stufe 3 – Netz-Körper: Feature, Netztabelle, Dateiformat 3, Eigenschaften, Sperre, Anzeige

### Task 10: Feature `MeshBody`, Netztabelle im Dokument, Bearbeitungssperre und alle `match`-Stellen

**Files:**
- Modify: `crates/planora-feature/Cargo.toml` (`planora-exchange`), `crates/planora-feature/src/model.rs`, `src/error.rs`, `src/compute.rs`, `src/ops.rs`, `src/snapshot.rs` (vollständig), `src/lib.rs` (vollständig)
- Create: `crates/planora-feature/src/meshes.rs`
- Modify (`match`-Stellen, P16): `crates/planora-project/src/format.rs`, `crates/planora-project/src/convert.rs`, `crates/planora-app/src/state.rs`, `crates/planora-app/src/icons.rs`, `crates/planora-app/src/properties.rs`, `crates/planora-app/src/feature_dialog.rs`
- Test: `crates/planora-feature/tests/mesh_bodies.rs`

**Interfaces:**
- Consumes: `planora_exchange::{ExchangeMesh, MeshTransform}` (Task 1), `planora_math::DQuat`.
- Produces:
  ```rust
  // planora_feature
  pub struct MeshId(pub u64);                         // Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord
  FeatureKind::MeshBody { mesh: MeshId, offset: DVec3, rotation: DQuat, scale: f64 }
  impl FeatureKind { pub fn mesh_transform(&self) -> Option<MeshTransform>; }
  pub const MIN_MESH_SCALE: f64 = 1e-6;  pub const MAX_MESH_SCALE: f64 = 1e6;
  pub const ROTATION_NORM_TOL: f64 = 1e-6;  pub const MESH_LOCKED: &str = "Netz-Körper lassen sich nicht bearbeiten";
  FeatureError::MeshBodyLocked;  FeatureError::MeshMissing(u64)
  pub struct VisibleMesh<'a> { pub body: BodyRef, pub mesh_id: MeshId, pub mesh: &'a Arc<ExchangeMesh>,
      pub transform: MeshTransform, pub name: String }
  impl Document {
      pub fn add_mesh(&mut self, mesh: Arc<ExchangeMesh>) -> Result<MeshId, FeatureError>;
      pub fn mesh(&self, id: MeshId) -> Option<&Arc<ExchangeMesh>>;
      pub fn mesh_count(&self) -> usize;
      pub fn mesh_body(&self, id: FeatureId) -> Option<(&Arc<ExchangeMesh>, MeshTransform)>;
      pub fn add_mesh_body(&mut self, name: impl Into<String>, mesh: Arc<ExchangeMesh>, t: MeshTransform) -> Result<FeatureId, FeatureError>;
      pub fn visible_meshes(&self) -> Vec<VisibleMesh<'_>>;
  }
  pub struct Snapshot { pub features, pub marker, pub next_id, pub meshes: BTreeMap<MeshId, Arc<ExchangeMesh>> }
  // planora_project::format: KindDto::NetzKoerper { netz: u64, versatz: V3, drehung: [f64; 4], massstab: f64 }
  // planora_app::state: BodyKind::Mesh
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-feature/tests/mesh_bodies.rs`)

```rust
//! Netz-Körper im Dokument: Sichtbarkeit, gemeinsame Netze, Lage, Unterdrücken, Löschen,
//! Sperre, Import ganz oder gar nicht (Teilprojekt 9, Task 10).

use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::{DQuat, DVec3, Frame, Tolerance};
use planora_sketch::Sketch;
use std::sync::Arc;

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    })
}

fn at(x: f64) -> MeshTransform {
    MeshTransform {
        offset: DVec3::new(x, 0.0, 0.0),
        ..MeshTransform::IDENTITY
    }
}

fn status(d: &Document, id: FeatureId) -> FeatureStatus {
    d.feature(id).unwrap().status.clone()
}

fn mesh_id(d: &Document, id: FeatureId) -> MeshId {
    match &d.feature(id).unwrap().kind {
        FeatureKind::MeshBody { mesh, .. } => *mesh,
        other => panic!("kein Netz-Körper: {other:?}"),
    }
}

#[test]
fn a_mesh_body_is_visible_with_its_transform() {
    let mut d = Document::new();
    let id = d.add_mesh_body("Netz 1", tetra(), at(20.0)).unwrap();
    assert_eq!(status(&d, id), FeatureStatus::Ok);
    assert!(d.visible_bodies().is_empty(), "kein Kernel-Körper");
    let v = d.visible_meshes();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].body, BodyRef { feature: id, index: 0 });
    assert_eq!(v[0].name, "Netz 1");
    assert_eq!(v[0].transform, at(20.0));
    assert_eq!(v[0].mesh_id, mesh_id(&d, id));
    let kind = &d.feature(id).unwrap().kind;
    assert_eq!(kind.label(), "Netz-Körper");
    assert!(kind.dependencies().is_empty());
    assert_eq!(kind.mesh_transform(), Some(at(20.0)));
    assert_eq!(d.mesh_count(), 1);
    d.check_invariants().unwrap();
}

#[test]
fn several_features_can_share_one_mesh() {
    let mut d = Document::new();
    let a = d.add_mesh_body("A", tetra(), at(0.0)).unwrap();
    let mid = mesh_id(&d, a);
    let shared = Arc::clone(d.mesh(mid).unwrap());
    let b = d
        .add_feature(
            "B",
            FeatureKind::MeshBody {
                mesh: mid,
                offset: DVec3::new(30.0, 0.0, 0.0),
                rotation: DQuat::IDENTITY,
                scale: 2.0,
            },
        )
        .unwrap();
    assert_eq!(status(&d, b), FeatureStatus::Ok);
    assert_eq!(d.mesh_count(), 1);
    assert!(Arc::ptr_eq(d.mesh_body(b).unwrap().0, &shared));
    assert_eq!(d.visible_meshes().len(), 2);
    assert_eq!(d.snapshot().meshes.len(), 1);
    // Löschen von A: Netz bleibt (B benutzt es); Löschen von B: Netz entfällt
    d.delete(a).unwrap();
    assert_eq!(d.mesh_count(), 1);
    d.delete(b).unwrap();
    assert_eq!(d.mesh_count(), 0);
    d.check_invariants().unwrap();
}

#[test]
fn transform_suppress_and_delete_follow_the_document() {
    let mut d = Document::new();
    let id = d
        .add_mesh_body("N", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    d.update(id, |k| {
        if let FeatureKind::MeshBody {
            scale, rotation, ..
        } = k
        {
            *scale = 3.0;
            *rotation = DQuat::from_rotation_y(1.0);
        }
    })
    .unwrap();
    let meshes = d.visible_meshes();
    let vol = meshes[0].mesh.mass(&meshes[0].transform).volume;
    assert!((vol - 27.0 * 1000.0 / 6.0).abs() < 1e-6, "{vol}");
    d.set_suppressed(id, true).unwrap();
    assert!(d.visible_meshes().is_empty());
    assert_eq!(status(&d, id), FeatureStatus::Suppressed);
    d.set_suppressed(id, false).unwrap();
    assert_eq!(d.visible_meshes().len(), 1);
    d.delete(id).unwrap();
    assert!(d.visible_meshes().is_empty());
    assert_eq!(d.mesh_count(), 0);
    d.check_invariants().unwrap();
}

#[test]
fn bad_parameters_become_named_error_states() {
    let mut d = Document::new();
    let id = d
        .add_mesh_body("N", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    let cases: [(&str, fn(&mut FeatureKind)); 6] = [
        ("Maßstab", |k| {
            if let FeatureKind::MeshBody { scale, .. } = k {
                *scale = 0.0;
            }
        }),
        ("Maßstab", |k| {
            if let FeatureKind::MeshBody { scale, .. } = k {
                *scale = 2e6;
            }
        }),
        ("Maßstab", |k| {
            if let FeatureKind::MeshBody { scale, .. } = k {
                *scale = f64::NAN;
            }
        }),
        ("Drehung", |k| {
            if let FeatureKind::MeshBody { rotation, .. } = k {
                *rotation = DQuat::from_xyzw(0.0, 0.0, 0.0, 2.0);
            }
        }),
        ("Drehung", |k| {
            if let FeatureKind::MeshBody { rotation, .. } = k {
                *rotation = DQuat::from_xyzw(f64::NAN, 0.0, 0.0, 1.0);
            }
        }),
        ("außerhalb", |k| {
            if let FeatureKind::MeshBody { offset, .. } = k {
                *offset = DVec3::new(2e9, 0.0, 0.0);
            }
        }),
    ];
    for (word, edit) in cases {
        d.update(id, edit).unwrap();
        match status(&d, id) {
            FeatureStatus::Error(e) => assert!(e.contains(word), "{e}"),
            s => panic!("{s:?}"),
        }
        assert!(d.visible_meshes().is_empty());
        d.check_invariants().unwrap();
        d.update(id, |k| {
            if let FeatureKind::MeshBody {
                scale,
                rotation,
                offset,
                ..
            } = k
            {
                *scale = 1.0;
                *rotation = DQuat::IDENTITY;
                *offset = DVec3::ZERO;
            }
        })
        .unwrap();
        assert_eq!(status(&d, id), FeatureStatus::Ok);
    }
    // Lage im Bereich, aber eine Ecke liegt bei 1e9 + 10
    d.update(id, |k| {
        if let FeatureKind::MeshBody { offset, .. } = k {
            *offset = DVec3::new(MAX_COORD_MM, 0.0, 0.0);
        }
    })
    .unwrap();
    assert!(matches!(status(&d, id), FeatureStatus::Error(e) if e.contains("außerhalb")));
}

#[test]
fn import_is_all_or_nothing() {
    let mut d = Document::new();
    d.add_feature(
        "Q",
        FeatureKind::Box {
            size: DVec3::ONE,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let before = (
        d.features().len(),
        d.edit_count(),
        d.mesh_count(),
        d.snapshot().next_id,
    );
    let far = MeshTransform {
        offset: DVec3::new(2e9, 0.0, 0.0),
        ..MeshTransform::IDENTITY
    };
    assert!(matches!(
        d.add_mesh_body("N", tetra(), far),
        Err(FeatureError::OutOfRange(_))
    ));
    let edge = MeshTransform {
        offset: DVec3::new(MAX_COORD_MM, 0.0, 0.0),
        ..MeshTransform::IDENTITY
    };
    assert!(matches!(
        d.add_mesh_body("N", tetra(), edge),
        Err(FeatureError::OutOfRange(_))
    ));
    let tiny = MeshTransform {
        scale: 1e-9,
        ..MeshTransform::IDENTITY
    };
    assert!(matches!(
        d.add_mesh_body("N", tetra(), tiny),
        Err(FeatureError::InvalidInput(_))
    ));
    let broken = Arc::new(ExchangeMesh {
        positions: vec![[0.0; 3]],
        indices: vec![0, 0, 5],
    });
    assert!(matches!(
        d.add_mesh_body("N", broken, MeshTransform::IDENTITY),
        Err(FeatureError::InvalidInput(_))
    ));
    assert_eq!(
        (
            d.features().len(),
            d.edit_count(),
            d.mesh_count(),
            d.snapshot().next_id
        ),
        before
    );
    d.check_invariants().unwrap();
}

#[test]
fn mesh_bodies_cannot_be_edited_by_other_features() {
    let mut d = Document::new();
    let q = d
        .add_feature(
            "Q",
            FeatureKind::Box {
                size: DVec3::splat(5.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let n = d
        .add_mesh_body("N", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    let (qb, nb) = (
        BodyRef { feature: q, index: 0 },
        BodyRef { feature: n, index: 0 },
    );
    let locked = FeatureError::MeshBodyLocked.to_string();
    assert_eq!(locked, MESH_LOCKED);
    let face = FaceRef {
        origin: n.0,
        role: "Netz".into(),
        hint: DVec3::ZERO,
    };
    let edge = EdgeRef {
        a: face.clone(),
        b: face.clone(),
        hint: DVec3::ZERO,
    };
    let kinds = [
        FeatureKind::Boolean {
            target: qb,
            tool: nb,
            op: BoolOp::Union,
        },
        FeatureKind::Boolean {
            target: nb,
            tool: qb,
            op: BoolOp::Difference,
        },
        FeatureKind::Move {
            body: nb,
            delta: DVec3::X,
        },
        FeatureKind::Verrunden {
            body: nb,
            edges: vec![edge.clone()],
            art: BlendArt::Rundung { radius: 1.0 },
            tangentenkette: false,
        },
        FeatureKind::Sketch(SketchFeature {
            sketch: Sketch::new(Frame::WORLD, "S", Tolerance::default()),
            plane: SketchPlane::OnFace(face),
            copies: vec![],
        }),
    ];
    for kind in kinds {
        let id = d.add_feature("X", kind).unwrap();
        assert_eq!(status(&d, id), FeatureStatus::Error(locked.clone()));
        d.check_invariants().unwrap();
        d.delete(id).unwrap();
    }
    assert_eq!(status(&d, n), FeatureStatus::Ok);
    assert_eq!(status(&d, q), FeatureStatus::Ok);
}

#[test]
fn snapshots_carry_only_used_meshes_and_are_checked_on_load() {
    let mut d = Document::new();
    let a = d.add_mesh_body("A", tetra(), at(0.0)).unwrap();
    let unused = d.add_mesh(tetra()).unwrap();
    let s = d.snapshot();
    assert_eq!(s.meshes.len(), 1);
    assert!(!s.meshes.contains_key(&unused));
    let mut back = Document::from_snapshot(s.clone(), 0.05).unwrap();
    assert_eq!(back.visible_meshes()[0].body.feature, a);
    back.check_invariants().unwrap();
    let next = back.add_mesh(tetra()).unwrap();
    assert!(next.0 > mesh_id(&d, a).0, "Kennungen laufen weiter");
    // fehlendes Netz: Verweis ungültig
    let mut missing = s.clone();
    missing.meshes.clear();
    assert!(matches!(
        Document::from_snapshot(missing, 0.05),
        Err(FeatureError::MeshMissing(_))
    ));
    // kaputtes Netz
    let id = *s.meshes.keys().next().unwrap();
    let mut broken = s.clone();
    broken.meshes.insert(
        id,
        Arc::new(ExchangeMesh {
            positions: vec![[f32::NAN; 3]; 3],
            indices: vec![0, 1, 2],
        }),
    );
    assert!(matches!(
        Document::from_snapshot(broken, 0.05),
        Err(FeatureError::InvalidInput(_))
    ));
    // Kennung 0
    let mut zero = s;
    let m = zero.meshes.remove(&id).unwrap();
    zero.meshes.insert(MeshId(0), m);
    assert!(matches!(
        Document::from_snapshot(zero, 0.05),
        Err(FeatureError::InvalidInput(_))
    ));
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-feature --test mesh_bodies`
Expected: FAIL – `unresolved import planora_exchange` bzw. `no variant MeshBody`.

- [ ] **Step 3: Feature-Modell erweitern**

In `crates/planora-feature/Cargo.toml` unter `[dependencies]` nach `planora-mesh.workspace = true` einfügen: `planora-exchange.workspace = true`.

In `crates/planora-feature/src/model.rs`:

Kopf ersetzen (`use crate::refs::…` bis `use planora_sketch::{CurveId, Sketch};`) durch:

```rust
use crate::refs::{EdgeRef, FaceRef};
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_kernel::{AnalyticKernel, BlendKind, BoolOp, Solid};
use planora_math::{DQuat, DVec3, Frame};
use planora_sketch::{CurveId, Sketch};
use std::collections::BTreeMap;
use std::sync::Arc;
```

Nach `pub struct BodyRef { … }` einfügen:

```rust
/// Kennung eines Netzes in der Netztabelle des Dokuments (ab 1); wird nie wiederverwendet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(pub u64);

/// Kleinster und größter Maßstab eines Netz-Körpers (Spec §6).
pub const MIN_MESH_SCALE: f64 = 1e-6;
pub const MAX_MESH_SCALE: f64 = 1e6;
/// Größte Abweichung der Länge der Drehung von 1 (Spec §6 „normiert“, Plan P17); deckt das
/// Runden beim Speichern und beim Umrechnen aus Grad ab.
pub const ROTATION_NORM_TOL: f64 = 1e-6;
/// Meldung der Bearbeitungssperre (Spec §6), gleich dem Text von
/// [`crate::FeatureError::MeshBodyLocked`].
pub const MESH_LOCKED: &str = "Netz-Körper lassen sich nicht bearbeiten";
```

In `enum FeatureKind` nach der Variante `Verrunden { … },` einfügen:

```rust
    /// Eingelesenes Netz („Netz-Körper“, Spec §6, Plan P10): Verweis in die Netztabelle des
    /// Dokuments und Lage (erst skalieren, dann drehen, dann verschieben). Erzeugt keinen
    /// Kernel-Körper.
    MeshBody {
        mesh: MeshId,
        offset: DVec3,
        rotation: DQuat,
        scale: f64,
    },
```

In `fn label` nach `FeatureKind::Verrunden { art, .. } => art.label(),` einfügen: `FeatureKind::MeshBody { .. } => "Netz-Körper",`.

In `fn dependencies` den Arm

```rust
            FeatureKind::Box { .. }
            | FeatureKind::Cylinder { .. }
            | FeatureKind::Sphere { .. }
            | FeatureKind::Cone { .. } => Vec::new(),
```

ersetzen durch:

```rust
            FeatureKind::Box { .. }
            | FeatureKind::Cylinder { .. }
            | FeatureKind::Sphere { .. }
            | FeatureKind::Cone { .. }
            | FeatureKind::MeshBody { .. } => Vec::new(),
```

Nach `fn dependencies` (im selben `impl FeatureKind`) einfügen:

```rust
    /// Lage eines Netz-Körpers; `None` für alle anderen Arten.
    pub fn mesh_transform(&self) -> Option<MeshTransform> {
        match self {
            FeatureKind::MeshBody {
                offset,
                rotation,
                scale,
                ..
            } => Some(MeshTransform {
                offset: *offset,
                rotation: *rotation,
                scale: *scale,
            }),
            _ => None,
        }
    }
```

In `pub struct Document` nach `pub(crate) edits: u64,` einfügen:

```rust
    /// Netze der Netz-Körper (Plan P11); mehrere Features können dasselbe Netz benutzen.
    pub(crate) meshes: BTreeMap<MeshId, Arc<ExchangeMesh>>,
    /// Nächste zu vergebende Netz-Kennung (beginnt bei 1).
    pub(crate) next_mesh: u64,
```

In `Document::with_options` im Strukturausdruck nach `edits: 0,` einfügen: `meshes: BTreeMap::new(),` und `next_mesh: 1,`.

In `crates/planora-feature/src/error.rs` nach der Variante `ImplausibleBlend,` einfügen:

```rust
    #[error("Netz-Körper lassen sich nicht bearbeiten")]
    MeshBodyLocked,
    #[error("Das Netz {0} fehlt im Dokument")]
    MeshMissing(u64),
```

- [ ] **Step 4: Berechnung, Prüfungen und Sperre**

In `crates/planora-feature/src/compute.rs`:

Die Zeile `use crate::model::{` … `};` (erster Import aus `crate::model`) ersetzen durch:

```rust
use crate::model::{
    BlendArt, BodyRef, Document, EdgeCopy, FeatureId, FeatureKind, MAX_MESH_SCALE,
    MIN_MESH_SCALE, ROTATION_NORM_TOL, SketchFeature, SketchPlane,
};
```

und nach `use planora_math::{DMat4, DVec3};` einfügen: `use planora_exchange::{ExchangeMesh, MeshTransform};`.

`fn check_parameters(kind: &FeatureKind)` wird `pub(crate) fn check_parameters(kind: &FeatureKind)`. In seinem `match` vor dem letzten Arm `FeatureKind::Sketch(_) | FeatureKind::Revolve { .. } | FeatureKind::Boolean { .. } => {}` einfügen:

```rust
        FeatureKind::MeshBody {
            offset,
            rotation,
            scale,
            ..
        } => {
            if !offset.to_array().iter().all(|v| in_range(*v)) {
                return out("Die Position des Netz-Körpers");
            }
            if !(scale.is_finite() && (MIN_MESH_SCALE..=MAX_MESH_SCALE).contains(scale)) {
                return Err(FeatureError::InvalidInput(
                    "Der Maßstab muss zwischen 0,000001 und 1 000 000 liegen".into(),
                ));
            }
            if !(rotation.is_finite() && (rotation.length() - 1.0).abs() <= ROTATION_NORM_TOL) {
                return Err(FeatureError::InvalidInput(
                    "Die Drehung des Netz-Körpers ist ungültig".into(),
                ));
            }
        }
```

Nach `fn check_extent` einfügen:

```rust
/// Alle Punkte des Netzes liegen nach der Lage `t` im Bereich ±[`MAX_COORD_MM`].
pub(crate) fn check_mesh_extent(mesh: &ExchangeMesh, t: &MeshTransform) -> Result<(), FeatureError> {
    let inside = (0..mesh.positions.len())
        .all(|i| t.apply(mesh.point(i)).to_array().iter().all(|c| in_range(*c)));
    if inside {
        Ok(())
    } else {
        Err(FeatureError::OutOfRange("Der Netz-Körper".into()))
    }
}

/// Netz-Körper lassen sich nicht weiterverarbeiten (Spec §6, Bearbeitungssperre).
fn ensure_not_mesh(doc: &Document, id: FeatureId) -> Result<(), FeatureError> {
    match doc.feature(id).map(|f| &f.kind) {
        Some(FeatureKind::MeshBody { .. }) => Err(FeatureError::MeshBodyLocked),
        _ => Ok(()),
    }
}
```

In `fn compute_unchecked` nach dem Arm `FeatureKind::Verrunden { … } => { … }` (vor der schließenden Klammer des `match`) einfügen:

```rust
        FeatureKind::MeshBody { mesh, .. } => {
            let m = doc
                .meshes
                .get(mesh)
                .ok_or(FeatureError::MeshMissing(mesh.0))?;
            let t = feature.kind.mesh_transform().unwrap_or_default();
            check_mesh_extent(m, &t)?;
            Ok(Computed::default())
        }
```

In `fn visible_body` als erste Anweisung einfügen: `ensure_not_mesh(doc, r.feature)?;`

In `fn compute_sketch` nach `let tol = doc.kernel.tolerance;` einfügen:

```rust
    if let SketchPlane::OnFace(r) = &sf.plane {
        ensure_not_mesh(doc, FeatureId(r.origin))?;
    }
    for c in &sf.copies {
        ensure_not_mesh(doc, FeatureId(c.edge.a.origin))?;
        ensure_not_mesh(doc, FeatureId(c.edge.b.origin))?;
    }
```

- [ ] **Step 5: Netztabelle, Verweise, Snapshot und Exporte**

`crates/planora-feature/src/meshes.rs`:

```rust
//! Netz-Körper (Teilprojekt 9, Plan P10, P11): Netztabelle des Dokuments, sichtbare
//! Netz-Körper und Anlegen beim Import (ganz oder gar nicht).

use crate::compute::{check_mesh_extent, check_parameters};
use crate::error::FeatureError;
use crate::model::{BodyRef, Document, FeatureId, FeatureKind, FeatureStatus, MeshId};
use planora_exchange::{ExchangeMesh, MeshTransform};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Ein sichtbarer Netz-Körper mit Netz, Lage und Anzeigename.
#[derive(Clone, Debug)]
pub struct VisibleMesh<'a> {
    pub body: BodyRef,
    pub mesh_id: MeshId,
    pub mesh: &'a Arc<ExchangeMesh>,
    pub transform: MeshTransform,
    pub name: String,
}

impl Document {
    /// Legt ein geprüftes Netz in die Tabelle (ohne Feature; zählt nicht als Änderung, nicht
    /// benutzte Netze werden nicht gespeichert).
    pub fn add_mesh(&mut self, mesh: Arc<ExchangeMesh>) -> Result<MeshId, FeatureError> {
        mesh.validate()
            .map_err(|e| FeatureError::InvalidInput(e.to_string()))?;
        let id = MeshId(self.next_mesh);
        self.next_mesh = self.next_mesh.checked_add(1).ok_or_else(|| {
            FeatureError::InvalidInput("Die Netz-Kennungen sind aufgebraucht".into())
        })?;
        self.meshes.insert(id, mesh);
        Ok(id)
    }

    pub fn mesh(&self, id: MeshId) -> Option<&Arc<ExchangeMesh>> {
        self.meshes.get(&id)
    }

    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Netz und Lage des Netz-Körpers `id` (auch wenn er unterdrückt oder fehlerhaft ist).
    pub fn mesh_body(&self, id: FeatureId) -> Option<(&Arc<ExchangeMesh>, MeshTransform)> {
        let kind = &self.feature(id)?.kind;
        let FeatureKind::MeshBody { mesh, .. } = kind else {
            return None;
        };
        Some((self.meshes.get(mesh)?, kind.mesh_transform()?))
    }

    /// Legt Netz und Netz-Körper am Marker an, nur wenn beides gelingt (Spec §5: Import ändert
    /// das Dokument erst nach vollständigem Erfolg). Sonst bleibt das Dokument unverändert, auch
    /// Kennungszähler und Änderungszahl.
    pub fn add_mesh_body(
        &mut self,
        name: impl Into<String>,
        mesh: Arc<ExchangeMesh>,
        t: MeshTransform,
    ) -> Result<FeatureId, FeatureError> {
        mesh.validate()
            .map_err(|e| FeatureError::InvalidInput(e.to_string()))?;
        let probe = FeatureKind::MeshBody {
            mesh: MeshId(0),
            offset: t.offset,
            rotation: t.rotation,
            scale: t.scale,
        };
        check_parameters(&probe)?;
        check_mesh_extent(&mesh, &t)?;
        let saved = (self.next_id, self.next_mesh, self.edits, self.marker);
        let mesh_id = self.add_mesh(mesh)?;
        let kind = FeatureKind::MeshBody {
            mesh: mesh_id,
            offset: t.offset,
            rotation: t.rotation,
            scale: t.scale,
        };
        let id = match self.add_feature(name, kind) {
            Ok(id) => id,
            Err(e) => {
                self.meshes.remove(&mesh_id);
                self.next_mesh = saved.1;
                return Err(e);
            }
        };
        match self.feature(id).map(|f| f.status.clone()) {
            Some(FeatureStatus::Ok) => Ok(id),
            status => {
                self.features.retain(|f| f.id != id);
                self.meshes.remove(&mesh_id);
                (self.next_id, self.next_mesh, self.edits, self.marker) = saved;
                self.recompute();
                let message = match status {
                    Some(FeatureStatus::Error(m) | FeatureStatus::Skipped(m)) => m,
                    _ => "Der Netz-Körper ließ sich nicht anlegen".to_string(),
                };
                Err(FeatureError::InvalidInput(message))
            }
        }
    }

    /// Sichtbare Netz-Körper (berechnet, nicht unterdrückt) in der Reihenfolge der Zeitleiste.
    pub fn visible_meshes(&self) -> Vec<VisibleMesh<'_>> {
        self.features
            .iter()
            .filter(|f| f.status == FeatureStatus::Ok && f.result.is_some())
            .filter_map(|f| {
                let FeatureKind::MeshBody { mesh, .. } = &f.kind else {
                    return None;
                };
                Some(VisibleMesh {
                    body: BodyRef {
                        feature: f.id,
                        index: 0,
                    },
                    mesh_id: *mesh,
                    mesh: self.meshes.get(mesh)?,
                    transform: f.kind.mesh_transform()?,
                    name: f.name.clone(),
                })
            })
            .collect()
    }

    /// Netze, auf die ein Feature verweist.
    pub(crate) fn used_meshes(&self) -> BTreeSet<MeshId> {
        self.features
            .iter()
            .filter_map(|f| match &f.kind {
                FeatureKind::MeshBody { mesh, .. } => Some(*mesh),
                _ => None,
            })
            .collect()
    }

    /// Entfernt Netze, auf die kein Feature mehr verweist.
    pub(crate) fn prune_meshes(&mut self) {
        let used = self.used_meshes();
        self.meshes.retain(|id, _| used.contains(id));
    }
}
```

In `crates/planora-feature/src/ops.rs`:
- In `fn check_references` nach der `for`-Schleife (vor `Ok(())`) einfügen:

```rust
        if let FeatureKind::MeshBody { mesh, .. } = kind
            && !self.meshes.contains_key(mesh)
        {
            return Err(FeatureError::MeshMissing(mesh.0));
        }
```

- In `fn delete_with_dependents` nach `self.marker -= before_marker;` einfügen: `self.prune_meshes();`

`crates/planora-feature/src/snapshot.rs` vollständig ersetzen:

```rust
//! Reine Daten eines Dokuments zum Speichern und Laden (ohne Ergebnisse und Zwischenspeicher).

use crate::error::FeatureError;
use crate::model::{Document, Feature, FeatureId, FeatureKind, FeatureStatus, MeshId};
use planora_exchange::ExchangeMesh;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SnapshotFeature {
    pub id: FeatureId,
    pub name: String,
    pub suppressed: bool,
    pub kind: FeatureKind,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub features: Vec<SnapshotFeature>,
    pub marker: usize,
    pub next_id: u64,
    /// Netze der Netz-Körper; nur die, auf die ein Feature verweist (Plan P11).
    pub meshes: BTreeMap<MeshId, Arc<ExchangeMesh>>,
}

impl Document {
    pub fn snapshot(&self) -> Snapshot {
        let used = self.used_meshes();
        Snapshot {
            features: self
                .features
                .iter()
                .map(|f| SnapshotFeature {
                    id: f.id,
                    name: f.name.clone(),
                    suppressed: f.suppressed,
                    kind: f.kind.clone(),
                })
                .collect(),
            marker: self.marker,
            next_id: self.next_id,
            meshes: self
                .meshes
                .iter()
                .filter(|(id, _)| used.contains(id))
                .map(|(id, m)| (*id, Arc::clone(m)))
                .collect(),
        }
    }

    /// Baut ein Dokument aus einem Snapshot und berechnet es einmal. Prüft Ids, Namen, Marker,
    /// Netze und Verweise; ungültige Zahlen führen erst bei der Berechnung zu einem Fehlerstatus
    /// am betroffenen Feature (kein Panic, kein Abbruch des Ladens).
    pub fn from_snapshot(s: Snapshot, chord_tolerance: f64) -> Result<Document, FeatureError> {
        let bad = |m: &str| FeatureError::InvalidInput(m.to_string());
        if s.next_id == u64::MAX {
            return Err(bad("Der Kennungszähler ist ungültig"));
        }
        if s.marker > s.features.len() {
            return Err(bad("Der Rückrollmarker liegt hinter dem letzten Feature"));
        }
        let mut seen = std::collections::HashSet::with_capacity(s.features.len());
        let mut doc = Document::with_options(chord_tolerance);
        for f in s.features.iter() {
            if f.id.0 == 0 || f.id.0 >= s.next_id {
                return Err(bad("Ein Feature hat eine ungültige Kennung"));
            }
            if !seen.insert(f.id) {
                return Err(bad("Eine Feature-Kennung kommt doppelt vor"));
            }
            if f.name.trim().is_empty() {
                return Err(bad("Ein Feature hat keinen Namen"));
            }
        }
        for (id, mesh) in &s.meshes {
            if id.0 == 0 {
                return Err(bad("Ein Netz hat eine ungültige Kennung"));
            }
            mesh.validate()
                .map_err(|e| FeatureError::InvalidInput(format!("Netz {}: {e}", id.0)))?;
        }
        doc.next_mesh = match s.meshes.last_key_value() {
            Some((id, _)) => id
                .0
                .checked_add(1)
                .ok_or_else(|| bad("Ein Netz hat eine ungültige Kennung"))?,
            None => 1,
        };
        doc.meshes = s.meshes;
        for f in s.features {
            let version = doc.bump();
            doc.features.push(Feature {
                id: f.id,
                name: f.name,
                suppressed: f.suppressed,
                kind: f.kind,
                status: FeatureStatus::NotComputed,
                version,
                result: None,
                failed: None,
            });
        }
        for idx in 0..doc.features.len() {
            doc.check_references(&doc.features[idx].kind, idx)?;
        }
        doc.marker = s.marker;
        doc.next_id = s.next_id;
        doc.recompute();
        doc.edits = 0;
        Ok(doc)
    }
}
```

`crates/planora-feature/src/lib.rs` vollständig ersetzen:

```rust
//! Feature-Baum von PlanoraCAD: Datenmodell, stabile Verweise auf Flächen und Kanten,
//! Berechnung und Neuberechnung mit Zwischenspeicher, Änderungsoperationen und Netz-Körper.

mod compute;
pub mod error;
mod meshes;
pub mod model;
mod ops;
pub mod recompute;
pub mod refs;
mod snapshot;

pub use compute::MAX_COORD_MM;
pub use error::FeatureError;
pub use meshes::VisibleMesh;
pub use model::{
    BlendArt, BodyRef, Document, EdgeCopy, Feature, FeatureId, FeatureKind, FeatureStatus,
    MAX_MESH_SCALE, MESH_LOCKED, MIN_MESH_SCALE, MeshId, ROTATION_NORM_TOL, SketchFeature,
    SketchPlane, StandardPlane,
};
pub use recompute::{RecomputeReport, VisibleBody};
pub use refs::{
    EdgeRef, FaceRef, ResolvedEdge, ResolvedFace, first_face, resolve_edge, resolve_face,
};
pub use snapshot::{Snapshot, SnapshotFeature};
```

- [ ] **Step 6: `match`-Stellen in Projekt und App (P16)**

`crates/planora-project/src/format.rs`, in `enum KindDto` nach der Variante `Verrunden { … },` einfügen:

```rust
    /// Netz-Körper (ab Format 3): Netz aus `netze`, Lage in mm, Drehung als Quaternion
    /// [x, y, z, w], Maßstab.
    NetzKoerper {
        netz: u64,
        versatz: V3,
        drehung: [f64; 4],
        massstab: f64,
    },
```

`crates/planora-project/src/convert.rs`:
- Import `use planora_feature::{ … };` um `MeshId` ergänzen und `use planora_math::{DVec3, Frame, Tolerance};` ersetzen durch `use planora_math::{DQuat, DVec3, Frame, Tolerance};`; zusätzlich `use std::collections::BTreeMap;`.
- In `fn kind_to` nach dem Arm `FeatureKind::Verrunden { … } => KindDto::Verrunden { … },` einfügen:

```rust
        FeatureKind::MeshBody {
            mesh,
            offset,
            rotation,
            scale,
        } => KindDto::NetzKoerper {
            netz: mesh.0,
            versatz: v3(*offset),
            drehung: rotation.to_array(),
            massstab: *scale,
        },
```

- In `fn kind_from` nach dem Arm `KindDto::Verrunden { … } => { … }` einfügen:

```rust
        KindDto::NetzKoerper {
            netz,
            versatz,
            drehung,
            massstab,
        } => {
            let [x, y, z, w] = *drehung;
            FeatureKind::MeshBody {
                mesh: MeshId(*netz),
                offset: v("Position", *versatz)?,
                rotation: DQuat::from_xyzw(
                    n("Drehung", x)?,
                    n("Drehung", y)?,
                    n("Drehung", z)?,
                    n("Drehung", w)?,
                ),
                scale: n("Maßstab", *massstab)?,
            }
        }
```

- Im Strukturausdruck am Ende von `decode_version` nach `next_id: file.naechste_id,` einfügen: `meshes: BTreeMap::new(),` (Task 11 ersetzt das durch die gelesenen Netze).

`crates/planora-app/src/state.rs`:
- In `enum BodyKind` nach `Combined,` einfügen:

```rust
    /// Eingelesenes Netz (Netz-Körper, Teilprojekt 9).
    Mesh,
```

- In `fn body_kind` vor `_ => BodyKind::Combined,` einfügen: `Some(FeatureKind::MeshBody { .. }) => BodyKind::Mesh,`

`crates/planora-app/src/icons.rs`:
- In `FeatureIcon::of` nach `K::Verrunden { .. } => FeatureIcon::Blend,` einfügen: `K::MeshBody { .. } => FeatureIcon::Body(BodyKind::Mesh),`
- In `paint_body_icon` nach dem Arm `BodyKind::Combined => { … }` einfügen:

```rust
        BodyKind::Mesh => {
            // Dreiecksnetz: ein großes Dreieck mit eingeschriebenem Dreieck
            let a = c + vec2(0.0, -r);
            let b = c + vec2(-r, r * 0.8);
            let d = c + vec2(r, r * 0.8);
            painter.add(egui::Shape::closed_line(vec![a, b, d], stroke));
            painter.add(egui::Shape::closed_line(
                vec![a.lerp(b, 0.5), b.lerp(d, 0.5), d.lerp(a, 0.5)],
                stroke,
            ));
        }
```

`crates/planora-app/src/properties.rs`:
- `use planora_math::DVec3;` ersetzen durch `use planora_math::{DVec3, EulerRot};`
- In `fn parameter_rows` nach dem Arm `FeatureKind::Verrunden { … } => { … rows }` einfügen:

```rust
        FeatureKind::MeshBody {
            mesh,
            offset,
            rotation,
            scale,
        } => {
            let triangles = doc.mesh(*mesh).map_or(0, |m| m.triangle_count());
            let (x, y, z) = rotation.to_euler(EulerRot::XYZ);
            vec![
                ("Netz", format!("{triangles} Dreiecke")),
                ("Position", triple(*offset)),
                (
                    "Drehung",
                    format!(
                        "{} / {} / {} °",
                        format_number(x.to_degrees(), 2),
                        format_number(y.to_degrees(), 2),
                        format_number(z.to_degrees(), 2)
                    ),
                ),
                ("Maßstab", format_number(*scale, 4)),
            ]
        }
```

`crates/planora-app/src/feature_dialog.rs`:
- In `fn numbers_fixed` den letzten Arm `FeatureKind::Sketch(_) | FeatureKind::Revolve { .. } | FeatureKind::Boolean { .. } => {` ersetzen durch

```rust
        FeatureKind::Sketch(_)
        | FeatureKind::Revolve { .. }
        | FeatureKind::Boolean { .. }
        | FeatureKind::MeshBody { .. } => {
```

- In `FeatureDialog::new` den Doc-Kommentar zu `/// Dialog für das Feature `id`; `None` für Skizzen, Netz-Körper (Lage im Eigenschaftenfenster) und unbekannte Features.` ändern und `if matches!(f.kind, FeatureKind::Sketch(_)) {` ersetzen durch `if matches!(f.kind, FeatureKind::Sketch(_) | FeatureKind::MeshBody { .. }) {`.

- [ ] **Step 7: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-feature && cargo test -p planora-project && cargo test -p planora-app && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (7 Tests in `mesh_bodies`), alle bisherigen Tests unverändert grün.

```bash
git add Cargo.lock crates/planora-feature/Cargo.toml crates/planora-feature/src/model.rs crates/planora-feature/src/error.rs crates/planora-feature/src/compute.rs crates/planora-feature/src/ops.rs crates/planora-feature/src/snapshot.rs crates/planora-feature/src/lib.rs crates/planora-feature/src/meshes.rs crates/planora-feature/tests/mesh_bodies.rs crates/planora-project/src/format.rs crates/planora-project/src/convert.rs crates/planora-app/src/state.rs crates/planora-app/src/icons.rs crates/planora-app/src/properties.rs crates/planora-app/src/feature_dialog.rs
git commit -m "feature: Netz-Körper mit Netztabelle, Bearbeitungssperre und Import ganz oder gar nicht

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Dateiformat 3 mit Base64-Netzdaten (Golden-Datei, feindliche Dateien)

**Files:**
- Modify: `crates/planora-exchange/src/mesh.rs` (Bytes für das Dokument)
- Test: `crates/planora-exchange/tests/bytes.rs`
- Modify: `crates/planora-project/Cargo.toml` (`planora-exchange`), `crates/planora-project/src/lib.rs`, `src/format.rs`, `src/convert.rs`
- Create: `crates/planora-project/src/base64.rs`, `crates/planora-project/tests/base64.rs`, `crates/planora-project/tests/format3.rs`, `crates/planora-project/tests/data/format3.json` (erzeugt)
- Modify (mechanisch, P12): `crates/planora-project/tests/docstore.rs` (Test `newer_files_with_changed_structure_report_too_new`), `crates/planora-project/tests/format2.rs`

**Interfaces:**
- Consumes: Task 10 (`Snapshot.meshes`, `KindDto::NetzKoerper`, `MeshId`).
- Produces:
  ```rust
  // planora_exchange::ExchangeMesh
  pub fn to_bytes(&self) -> Vec<u8>;
  pub fn byte_len_from_header(head: &[u8]) -> Result<u64, ExchangeError>;
  pub fn from_bytes(bytes: &[u8]) -> Result<ExchangeMesh, ExchangeError>;
  // planora_project::base64
  pub struct Base64Error;  pub fn encode(bytes: &[u8]) -> String;
  pub fn decoded_len(text: &str) -> Result<usize, Base64Error>;
  pub fn decode(text: &str) -> Result<Vec<u8>, Base64Error>;
  pub fn decode_prefix(text: &str, max_bytes: usize) -> Result<Vec<u8>, Base64Error>;
  // planora_project
  pub const FORMAT_VERSION: u32 = 3;  pub fn required_format(s: &Snapshot) -> u32;  // 3, 2 oder 1
  // format.rs: VersionFile { …, pub netze: Vec<MeshDto> }; pub struct MeshDto { pub id: u64, pub daten: String }
  ```

- [ ] **Step 1: Tests für die Netz-Bytes schreiben** (`crates/planora-exchange/tests/bytes.rs`)

```rust
//! Kompakte Netzdaten für das Dokument (Spec §7, Teilprojekt 9, Task 11).

use planora_exchange::*;

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}

#[test]
fn bytes_round_trip() {
    let m = tetra();
    let b = m.to_bytes();
    assert_eq!(b.len(), 8 + 12 * 4 + 12 * 4);
    assert_eq!(&b[..8], &[4, 0, 0, 0, 4, 0, 0, 0]);
    assert_eq!(&b[8 + 12..8 + 16], &10.0f32.to_le_bytes());
    assert_eq!(ExchangeMesh::from_bytes(&b).unwrap(), m);
    assert_eq!(ExchangeMesh::byte_len_from_header(&b[..8]), Ok(b.len() as u64));
}

#[test]
fn damaged_bytes_are_named() {
    let good = tetra().to_bytes();
    assert!(matches!(
        ExchangeMesh::from_bytes(&good[..7]),
        Err(ExchangeError::Corrupt(_))
    ));
    assert!(matches!(
        ExchangeMesh::from_bytes(&good[..good.len() - 4]),
        Err(ExchangeError::Corrupt(_))
    ));
    let mut more = good.clone();
    more[4] = 5;
    assert!(matches!(
        ExchangeMesh::from_bytes(&more),
        Err(ExchangeError::Corrupt(_))
    ));
    let mut huge = good.clone();
    huge[4..8].copy_from_slice(&(MAX_TRIANGLES as u32 + 1).to_le_bytes());
    assert_eq!(ExchangeMesh::from_bytes(&huge), Err(ExchangeError::TooManyTriangles));
    let mut many = good.clone();
    many[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(ExchangeMesh::from_bytes(&many), Err(ExchangeError::TooManyVertices));
    let mut nan = good.clone();
    nan[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(
        ExchangeMesh::from_bytes(&nan),
        Err(ExchangeError::NotFinite { line: None })
    );
    let mut index = good.clone();
    let at = 8 + 48;
    index[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
    assert!(matches!(
        ExchangeMesh::from_bytes(&index),
        Err(ExchangeError::Corrupt(_))
    ));
    let none = [0u8; 8];
    assert_eq!(ExchangeMesh::from_bytes(&none), Err(ExchangeError::Empty));
}
```

- [ ] **Step 2: Bytes implementieren**

In `crates/planora-exchange/src/mesh.rs` im Block `impl ExchangeMesh` nach `fn placed` einfügen:

```rust
    /// Kompakte Form für das Dokument (Spec §7, Plan P12): Punktzahl und Dreieckszahl (u32),
    /// dann die Punkte (f32) und die Punktnummern (u32), alles little endian.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + 12 * self.positions.len() + 4 * self.indices.len());
        out.extend_from_slice(&(self.positions.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.triangle_count() as u32).to_le_bytes());
        for p in &self.positions {
            for c in p {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in &self.indices {
            out.extend_from_slice(&i.to_le_bytes());
        }
        out
    }

    /// Erwartete Länge der Bytes aus ihren ersten acht; prüft die Anzahlen gegen die Grenzen,
    /// bevor etwas reserviert wird.
    pub fn byte_len_from_header(head: &[u8]) -> Result<u64, ExchangeError> {
        let Some(h) = head.get(..8) else {
            return Err(ExchangeError::Corrupt("Die Netzdaten sind zu kurz".into()));
        };
        let vertices = u64::from(u32::from_le_bytes([h[0], h[1], h[2], h[3]]));
        let triangles = u64::from(u32::from_le_bytes([h[4], h[5], h[6], h[7]]));
        if triangles > MAX_TRIANGLES as u64 {
            return Err(ExchangeError::TooManyTriangles);
        }
        if vertices > MAX_VERTICES as u64 {
            return Err(ExchangeError::TooManyVertices);
        }
        Ok(8 + 12 * vertices + 12 * triangles)
    }

    /// Liest die Form aus [`ExchangeMesh::to_bytes`] und prüft das Netz.
    pub fn from_bytes(bytes: &[u8]) -> Result<ExchangeMesh, ExchangeError> {
        let expected = Self::byte_len_from_header(bytes)?;
        if expected != bytes.len() as u64 {
            return Err(ExchangeError::Corrupt(
                "Die Länge der Netzdaten passt nicht zu den Anzahlen".into(),
            ));
        }
        let vertices = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let (points, numbers) = bytes[8..].split_at(12 * vertices);
        let floats: Vec<f32> = points
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        let mesh = ExchangeMesh {
            positions: floats.as_chunks::<3>().0.to_vec(),
            indices: numbers
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect(),
        };
        mesh.validate()?;
        Ok(mesh)
    }
```

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-exchange --test bytes`
Expected: PASS (2 Tests).

- [ ] **Step 3: Tests für Base64 und Format 3 schreiben**

`crates/planora-project/tests/base64.rs`:

```rust
//! Base64 für die Netzdaten (RFC 4648, streng; Teilprojekt 9, Task 11).

use planora_project::base64::{Base64Error, decode, decode_prefix, decoded_len, encode};

#[test]
fn rfc_4648_vectors() {
    for (plain, coded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(encode(plain.as_bytes()), coded);
        assert_eq!(decode(coded).unwrap(), plain.as_bytes());
        assert_eq!(decoded_len(coded), Ok(plain.len()));
    }
}

#[test]
fn all_byte_values_round_trip() {
    let bytes: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
}

#[test]
fn strict_decoding_refuses_everything_else() {
    for bad in [
        "Zg=", "Zg", "Z===", "Zh==", "Zm9=", "Zm 9v", "Zm9v\n", "Zm-v", "Zg==Zg==", "=Zg=",
        "ä000",
    ] {
        assert_eq!(decode(bad), Err(Base64Error), "{bad:?}");
    }
    assert_eq!(decoded_len("abc"), Err(Base64Error));
}

#[test]
fn a_prefix_is_decoded_without_the_rest() {
    let coded = encode(b"Planora ist ein CAD");
    assert_eq!(decode_prefix(&coded, 8).unwrap(), b"Planora i");
    assert_eq!(decode_prefix("Zg==", 8).unwrap(), b"f");
    assert_eq!(decode_prefix("@@@@AAAA", 2), Err(Base64Error));
    assert_eq!(decode_prefix("ä000Zg==", 1), Err(Base64Error));
}
```

`crates/planora-project/tests/format3.rs`:

```rust
//! Dateiformat 3: Netz-Körper (Golden-Datei), kleinste passende Version, alte Dateien,
//! feindliche Netzdaten (Teilprojekt 9, Task 11).

use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::*;
use planora_math::{DQuat, DVec3};
use planora_project::{
    FORMAT_VERSION, ProjectError, base64, decode_version, encode_version, required_format,
};
use std::sync::Arc;

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    })
}

/// Quader, zwei Netz-Körper mit demselben Netz (einer gedreht und vergrößert).
fn mesh_document() -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let t = MeshTransform {
        offset: DVec3::new(20.0, 0.0, 0.0),
        rotation: DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
        scale: 2.0,
    };
    let a = d.add_mesh_body("Netz 1", tetra(), t).unwrap();
    let mesh = match &d.feature(a).unwrap().kind {
        FeatureKind::MeshBody { mesh, .. } => *mesh,
        other => panic!("{other:?}"),
    };
    d.add_feature(
        "Netz 2",
        FeatureKind::MeshBody {
            mesh,
            offset: DVec3::new(40.0, 0.0, 0.0),
            rotation: DQuat::IDENTITY,
            scale: 1.0,
        },
    )
    .unwrap();
    d
}

fn volumes(d: &Document) -> Vec<f64> {
    d.visible_meshes()
        .iter()
        .map(|v| v.mesh.mass(&v.transform).volume)
        .collect()
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

fn edited(good: &str, f: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v: serde_json::Value = serde_json::from_str(good).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

fn daten(good: &str) -> Vec<u8> {
    let v: serde_json::Value = serde_json::from_str(good).unwrap();
    base64::decode(v["netze"][0]["daten"].as_str().unwrap()).unwrap()
}

fn with_bytes(good: &str, bytes: &[u8]) -> String {
    edited(good, |v| {
        v["netze"][0]["daten"] = serde_json::Value::String(base64::encode(bytes));
    })
}

/// Setzt `key` in der Art des ersten Netz-Körpers.
fn set_mesh_field(v: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    let f = v["features"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["art"]["typ"] == "netz_koerper")
        .unwrap();
    f["art"][key] = value;
}

/// Kopfprüfung eines Programms, das höchstens Format `max` lesen kann (wie die bestehende
/// Versionsprüfung `format > FORMAT_VERSION` in `decode_version` und `check_head`).
fn old_program_says_too_new(text: &str, max: u32) -> bool {
    let head: serde_json::Value = serde_json::from_str(text).unwrap();
    head["format"].as_u64().unwrap() > u64::from(max)
}

#[test]
fn the_golden_file_pins_format_3() {
    let d = mesh_document();
    assert!(d.features().iter().all(|f| f.status == FeatureStatus::Ok));
    let text = encode_version(&d.snapshot()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/format3.json");
    if std::env::var("PLANORA_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, &text).unwrap();
    }
    let golden = std::fs::read_to_string(&path)
        .expect("tests/data/format3.json fehlt – mit PLANORA_UPDATE_GOLDEN=1 erzeugen");
    assert_eq!(text, golden.trim_end_matches('\n'));
    assert!(golden.contains("\"format\": 3"));
    assert!(golden.contains("\"typ\": \"netz_koerper\""));
    assert_eq!(golden.matches("\"daten\"").count(), 1, "ein Netz für zwei Netz-Körper");
    let back = Document::from_snapshot(decode_version(&golden, "format3.json").unwrap(), 0.05).unwrap();
    back.check_invariants().unwrap();
    assert_eq!(volumes(&back), volumes(&d));
    assert_eq!(back.visible_bodies().len(), 1);
    assert_eq!(back.visible_meshes().len(), 2);
}

#[test]
fn the_smallest_fitting_version_is_written() {
    let mut d = Document::new();
    d.add_feature(
        "Q",
        FeatureKind::Box {
            size: DVec3::ONE,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    assert_eq!(required_format(&d.snapshot()), 1);
    let mut m = mesh_document();
    assert_eq!(required_format(&m.snapshot()), 3);
    assert_eq!(FORMAT_VERSION, 3);
    // Netz-Körper gelöscht: wieder Format 1, keine Netzdaten mehr
    let ids: Vec<FeatureId> = m
        .features()
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::MeshBody { .. }))
        .map(|f| f.id)
        .collect();
    for id in ids {
        m.delete(id).unwrap();
    }
    let text = encode_version(&m.snapshot()).unwrap();
    assert!(text.contains("\"format\": 1") && !text.contains("netze"), "{text}");
}

#[test]
fn old_files_stay_readable_and_old_programs_report_too_new() {
    for name in ["format1.json", "format1_voll.json", "format2.json"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(name);
        let text = std::fs::read_to_string(&path).unwrap();
        let s = decode_version(&text, name).unwrap();
        assert!(s.meshes.is_empty(), "{name}");
        Document::from_snapshot(s, 0.05).unwrap();
    }
    let good = encode_version(&mesh_document().snapshot()).unwrap();
    // Ein Programm mit Format ≤ 2 lehnt die Datei über seine Versionsprüfung ab (simuliert),
    // dieses Programm liest sie.
    assert!(old_program_says_too_new(&good, 2));
    assert!(!old_program_says_too_new(&good, FORMAT_VERSION));
    // Eine noch neuere Datei meldet dieses Programm genauso als „zu neu“.
    assert!(matches!(
        bad(&good.replacen("\"format\": 3", "\"format\": 4", 1)),
        ProjectError::TooNew { found: 4, .. }
    ));
    // Format 1 oder 2 mit Netz-Körper: beschädigt
    for older in ["\"format\": 1", "\"format\": 2"] {
        let e = bad(&good.replacen("\"format\": 3", older, 1));
        assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
        assert!(e.to_string().contains("Dateiformat 3"), "{e}");
    }
}

#[test]
fn hostile_mesh_data_is_rejected_by_name() {
    let good = encode_version(&mesh_document().snapshot()).unwrap();
    let bytes = daten(&good);
    let mut more = bytes.clone();
    more[4..8].copy_from_slice(&1000u32.to_le_bytes());
    // 50 000 000 Punkte und Dreiecke im Kopf, Inhalt fehlt (8-GB-Behauptung)
    let mut claim = bytes.clone();
    claim[0..8].copy_from_slice(&[0x80, 0xF0, 0xFA, 0x02, 0x80, 0xF0, 0xFA, 0x02]);
    let mut nan = bytes.clone();
    nan[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut index = bytes.clone();
    let at = bytes.len() - 4;
    index[at..].copy_from_slice(&77u32.to_le_bytes());
    let named = [
        edited(&good, |v| v["netze"][0]["daten"] = serde_json::json!("@@@@")),
        edited(&good, |v| v["netze"][0]["daten"] = serde_json::json!("QUJD Q")),
        with_bytes(&good, &more),
        with_bytes(&good, &claim),
        with_bytes(&good, &nan),
        with_bytes(&good, &index),
        with_bytes(&good, &bytes[..bytes.len() - 4]),
        edited(&good, |v| {
            let n = v["netze"][0].clone();
            v["netze"].as_array_mut().unwrap().push(n);
        }),
        edited(&good, |v| v["netze"][0]["id"] = serde_json::json!(0)),
        edited(&good, |v| v["netze"] = serde_json::json!([])),
    ];
    for text in named {
        assert_ne!(text, good, "Änderung muss greifen");
        match bad(&text) {
            ProjectError::Corrupt { msg, .. } => assert!(msg.contains("Netz"), "{msg}"),
            other => panic!("{other:?}"),
        }
    }
    // Falscher Typ und riesige Zahl: beim Laden abgelehnt
    for text in [
        edited(&good, |v| v["netze"][0]["daten"] = serde_json::json!(5)),
        edited(&good, |v| {
            set_mesh_field(v, "versatz", serde_json::json!([1e300, 0.0, 0.0]));
        }),
    ] {
        assert!(matches!(bad(&text), ProjectError::Corrupt { .. }), "{text}");
    }
    // Geladen, aber beim Berechnen benannt gemeldet: Maßstab 0, Drehung nicht normiert
    for text in [
        edited(&good, |v| set_mesh_field(v, "massstab", serde_json::json!(0.0))),
        edited(&good, |v| {
            set_mesh_field(v, "drehung", serde_json::json!([0.0, 0.0, 0.0, 2.0]));
        }),
    ] {
        let d = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
        d.check_invariants().unwrap();
        assert!(d
            .features()
            .iter()
            .any(|f| matches!(f.status, FeatureStatus::Error(_))));
    }
}
```

- [ ] **Step 4: Mechanische Testanpassung (P12)**

In `crates/planora-project/tests/docstore.rs` im Test `newer_files_with_changed_structure_report_too_new` die beiden Texte `"format":3` durch `"format":4` und die beiden Muster `found: 3` durch `found: 4` ersetzen und den Kommentar `// Seit Teilprojekt 8 ist Format 2 gültig (P7); „zu neu“ ist jetzt 3.` ersetzen durch:

```rust
    // Seit Teilprojekt 9 ist Format 3 gültig (Netz-Körper, P12); „zu neu“ ist jetzt 4.
```

In `crates/planora-project/tests/format2.rs`:
- in `documents_without_blends_still_write_format_1` `assert_eq!(FORMAT_VERSION, 2);` ersetzen durch `assert_eq!(FORMAT_VERSION, 3); // seit Teilprojekt 9 (Netz-Körper, P12)`;
- in `a_format_1_file_with_a_blend_is_corrupt_and_format_3_is_too_new` die letzte Prüfung ersetzen durch:

```rust
    // Seit Teilprojekt 9 ist Format 3 gültig (P12); „zu neu“ ist jetzt 4.
    assert!(matches!(
        bad(&good.replacen("\"format\": 2", "\"format\": 4", 1)),
        ProjectError::TooNew { found: 4, .. }
    ));
```

- [ ] **Step 5: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-project --test base64 --test format3`
Expected: FAIL – `could not find base64 in planora_project`.

- [ ] **Step 6: Base64 und Format 3 implementieren**

In `crates/planora-project/Cargo.toml` unter `[dependencies]` nach `planora-feature.workspace = true` einfügen: `planora-exchange.workspace = true`.

`crates/planora-project/src/base64.rs`:

```rust
//! Base64 (RFC 4648, Standardalphabet mit „=“) für die Netzdaten im Dokument (Spec §7, Plan
//! P12); selbst geschrieben, ohne neue Abhängigkeit. Das Lesen ist streng: keine Leerzeichen,
//! richtige Auffüllung, keine gesetzten Füllbits.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
/// Untere sechs Bit eines Zeichens.
const SIX_BITS: u32 = 0x3F;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Die Netzdaten sind keine gültige Base64-Zeichenkette")]
pub struct Base64Error;

fn symbol(n: u32) -> char {
    char::from(ALPHABET[(n & SIX_BITS) as usize])
}

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let (groups, rest) = bytes.as_chunks::<3>();
    for g in groups {
        let n = (u32::from(g[0]) << 16) | (u32::from(g[1]) << 8) | u32::from(g[2]);
        for shift in [18, 12, 6, 0] {
            out.push(symbol(n >> shift));
        }
    }
    match rest {
        [a] => {
            let n = u32::from(*a) << 16;
            out.push(symbol(n >> 18));
            out.push(symbol(n >> 12));
            out.push_str("==");
        }
        [a, b] => {
            let n = (u32::from(*a) << 16) | (u32::from(*b) << 8);
            out.push(symbol(n >> 18));
            out.push(symbol(n >> 12));
            out.push(symbol(n >> 6));
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Länge der dekodierten Bytes, ohne zu dekodieren.
pub fn decoded_len(text: &str) -> Result<usize, Base64Error> {
    let b = text.as_bytes();
    if !b.len().is_multiple_of(4) {
        return Err(Base64Error);
    }
    let pad = b.iter().rev().take(2).take_while(|&&c| c == b'=').count();
    Ok(b.len() / 4 * 3 - pad)
}

pub fn decode(text: &str) -> Result<Vec<u8>, Base64Error> {
    decode_bytes(text.as_bytes())
}

/// Dekodiert nur den Anfang: so viele Vierergruppen, wie für `max_bytes` Bytes nötig sind.
pub fn decode_prefix(text: &str, max_bytes: usize) -> Result<Vec<u8>, Base64Error> {
    let b = text.as_bytes();
    let end = (max_bytes.div_ceil(3) * 4).min(b.len());
    decode_bytes(&b[..end])
}

fn value(c: u8) -> Result<u32, Base64Error> {
    match c {
        b'A'..=b'Z' => Ok(u32::from(c - b'A')),
        b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(Base64Error),
    }
}

fn decode_bytes(b: &[u8]) -> Result<Vec<u8>, Base64Error> {
    if !b.len().is_multiple_of(4) {
        return Err(Base64Error);
    }
    let groups = b.as_chunks::<4>().0;
    let mut out = Vec::with_capacity(groups.len() * 3);
    for (k, g) in groups.iter().enumerate() {
        let pad = if k + 1 == groups.len() {
            g.iter().rev().take_while(|&&c| c == b'=').count()
        } else {
            0
        };
        if pad > 2 {
            return Err(Base64Error);
        }
        let mut n = 0u32;
        for &c in &g[..4 - pad] {
            n = (n << 6) | value(c)?;
        }
        n <<= 6 * pad as u32;
        // Füllbits müssen null sein (eindeutige Darstellung).
        let fill_mask = match pad {
            1 => 0xFF,
            2 => 0xFFFF,
            _ => 0,
        };
        if n & fill_mask != 0 {
            return Err(Base64Error);
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Ok(out)
}
```

`crates/planora-project/src/lib.rs`: nach der Kopfzeile `pub mod base64;` als erste Modulzeile einfügen.

`crates/planora-project/src/format.rs`:
- In `pub struct VersionFile` nach `pub features: Vec<FeatureDto>,` einfügen:

```rust
    /// Netze der Netz-Körper (ab Format 3, Plan P12). Fehlt das Feld oder ist die Liste leer,
    /// bleibt die Datei Byte für Byte wie in Format 1 und 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub netze: Vec<MeshDto>,
```

- Nach `pub struct BodyRefDto { … }` einfügen:

```rust
/// Ein Netz: Kennung und Base64 der Bytes aus `ExchangeMesh::to_bytes`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MeshDto {
    pub id: u64,
    pub daten: String,
}
```

`crates/planora-project/src/convert.rs`:
- Importe ergänzen: `use crate::base64;`, `use planora_exchange::{ExchangeMesh, MAX_FILE_BYTES};`, `use std::sync::Arc;` (BTreeMap ist seit Task 10 importiert).
- `FORMAT_VERSION` und `required_format` ersetzen durch:

```rust
/// Höchste lesbare Formatnummer der Versionsdateien (P7, P12).
pub const FORMAT_VERSION: u32 = 3;
/// Formatnummer der Rahmendateien `dokument.json` und `.autosave.json`; ihre Struktur ist seit
/// Format 1 unverändert, ältere Programme öffnen das Projekt deshalb weiter (P7).
pub const CONTAINER_FORMAT: u32 = 1;
/// Größte Summe der dekodierten Netzdaten eines Dokuments (wie die Dateigrenze beim Import).
const MAX_MESH_BYTES: u64 = MAX_FILE_BYTES;

/// Kleinste Formatnummer, die den Stand darstellt: 3 mit Netz-Körpern, 2 mit Rundungen oder
/// Fasen, sonst 1 (P7, P12).
pub fn required_format(s: &Snapshot) -> u32 {
    let has = |f: fn(&FeatureKind) -> bool| s.features.iter().any(|x| f(&x.kind));
    if has(|k| matches!(k, FeatureKind::MeshBody { .. })) {
        3
    } else if has(|k| matches!(k, FeatureKind::Verrunden { .. })) {
        2
    } else {
        1
    }
}
```

(Die bisherige Zeile `pub const CONTAINER_FORMAT: u32 = 1;` samt Kommentar wird dabei ersetzt, nicht verdoppelt.)

- Vor `// ---- Öffentliche Funktionen` einfügen:

```rust
// ---- Netze ------------------------------------------------------------------------------

/// Liest die Netze einer Datei. Vor dem Dekodieren werden Länge der Zeichenkette (auch in
/// Summe) und der Kopf gegen die Grenzen und gegen die Gesamtlänge geprüft (Spec §7).
fn meshes_from(origin: &str, list: &[MeshDto]) -> Result<BTreeMap<MeshId, Arc<ExchangeMesh>>> {
    let mut total: u64 = 0;
    let mut out = BTreeMap::new();
    for d in list {
        let bad = |m: &str| ProjectError::corrupt(origin, format!("Netz {}: {m}", d.id));
        if d.id == 0 {
            return Err(bad("Die Kennung ist ungültig"));
        }
        let len = base64::decoded_len(&d.daten).map_err(|e| bad(&e.to_string()))? as u64;
        total = total.saturating_add(len);
        if len > MAX_MESH_BYTES || total > MAX_MESH_BYTES {
            return Err(bad("Die Netzdaten sind zu groß"));
        }
        let head = base64::decode_prefix(&d.daten, 8).map_err(|e| bad(&e.to_string()))?;
        let expected = ExchangeMesh::byte_len_from_header(&head).map_err(|e| bad(&e.to_string()))?;
        if expected != len {
            return Err(bad("Die Länge der Netzdaten passt nicht zu den Anzahlen"));
        }
        let bytes = base64::decode(&d.daten).map_err(|e| bad(&e.to_string()))?;
        let mesh = ExchangeMesh::from_bytes(&bytes).map_err(|e| bad(&e.to_string()))?;
        if out.insert(MeshId(d.id), Arc::new(mesh)).is_some() {
            return Err(bad("Die Kennung kommt doppelt vor"));
        }
    }
    Ok(out)
}
```

- In `encode_version` im Strukturausdruck `VersionFile { … }` nach dem Feld `features: …,` einfügen:

```rust
        netze: s
            .meshes
            .iter()
            .map(|(id, m)| MeshDto {
                id: id.0,
                daten: base64::encode(&m.to_bytes()),
            })
            .collect(),
```

- In `decode_version` nach der Prüfung „Rundungen und Fasen gibt es erst ab Dateiformat 2“ einfügen:

```rust
    let has_mesh_body = file
        .features
        .iter()
        .any(|f| matches!(f.art, KindDto::NetzKoerper { .. }));
    if file.format < 3 && (has_mesh_body || !file.netze.is_empty()) {
        return Err(ProjectError::corrupt(
            origin,
            "Netz-Körper gibt es erst ab Dateiformat 3",
        ));
    }
    check_len(origin, "Netze", file.netze.len())?;
    let meshes = meshes_from(origin, &file.netze)?;
    for f in &file.features {
        if let KindDto::NetzKoerper { netz, .. } = &f.art
            && !meshes.contains_key(&MeshId(*netz))
        {
            return Err(ProjectError::corrupt(
                origin,
                format!("„{}“: Das Netz {netz} fehlt in der Datei", f.name),
            ));
        }
    }
```

- Im Strukturausdruck am Ende von `decode_version` `meshes: BTreeMap::new(),` (aus Task 10) ersetzen durch `meshes,`.

- [ ] **Step 7: Golden-Datei erzeugen und prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && PLANORA_UPDATE_GOLDEN=1 cargo test -p planora-project --test format3 the_golden_file_pins_format_3 && cargo test -p planora-project && cargo test -p planora-exchange`
Expected: PASS; `git status --short crates/planora-project/tests/data` zeigt nur `?? crates/planora-project/tests/data/format3.json`. `format1.json`, `format1_voll.json`, `format2.json` sind unverändert. Die Datei von Hand lesen: `"format": 3`, Feld `"netze"` mit genau einem Eintrag, Features mit `"typ": "netz_koerper"`, `netz`, `versatz`, `drehung`, `massstab`.

- [ ] **Step 8: Prüfungen und Commit**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

```bash
git add Cargo.lock crates/planora-exchange/src/mesh.rs crates/planora-exchange/tests/bytes.rs crates/planora-project/Cargo.toml crates/planora-project/src/lib.rs crates/planora-project/src/base64.rs crates/planora-project/src/format.rs crates/planora-project/src/convert.rs crates/planora-project/tests/base64.rs crates/planora-project/tests/format3.rs crates/planora-project/tests/data/format3.json crates/planora-project/tests/docstore.rs crates/planora-project/tests/format2.rs
git commit -m "project: Dateiformat 3 mit Netz-Körpern (Base64), Format 1 und 2 bleiben

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Netz-Körper in der App – Anzeige, Lage, Vorschau, Sperre im Zustand

**Files:**
- Modify: `crates/planora-app/Cargo.toml` (`planora-exchange`), `crates/planora-app/src/lib.rs`, `crates/planora-app/src/state.rs`
- Create: `crates/planora-app/src/mesh_body.rs`
- Test: `crates/planora-app/tests/mesh_body_state.rs`

**Interfaces:**
- Consumes: Task 8 (`shade`, `SMOOTH_ANGLE_DEG`), Task 2 (`check_mesh`, `MeshReport`), Task 10 (`visible_meshes`, `add_mesh_body`, `mesh_body`, `MESH_LOCKED`, `MeshId`).
- Produces:
  ```rust
  // planora_app::mesh_body
  pub const DISPLAY_TRIANGLES: usize = 5_000_000;  pub const PREVIEW_NOTE: &str;  pub const WARNING_COLOR: egui::Color32;
  pub fn display_mesh(mesh: &ExchangeMesh, t: &MeshTransform, max_triangles: usize) -> (Mesh, bool);
  pub fn make_body(name: String, source: BodyRef, mesh: &ExchangeMesh, t: &MeshTransform, report: MeshReport, max_triangles: usize) -> Body;
  pub fn euler_degrees(q: DQuat) -> DVec3;  pub fn rotation_from_degrees(d: DVec3) -> DQuat;
  pub fn report_text(r: &MeshReport) -> String;
  // planora_app::state
  Body { …, pub mesh_report: Option<MeshReport>, pub preview: bool }
  AppState { …, pub display_triangles: usize }
  impl AppState {
      pub fn import_mesh(&mut self, name: &str, mesh: Arc<ExchangeMesh>, unit_scale: f64) -> Result<FeatureId, String>;
      pub fn set_mesh_transform(&mut self, index: usize, t: MeshTransform) -> Result<(), String>;
      pub fn mesh_transform(&self, index: usize) -> Option<MeshTransform>;
      pub fn is_mesh(&self, index: usize) -> bool;
      pub fn solid_body_count(&self) -> usize;
      pub fn selected_is_mesh(&self) -> bool;
  }
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-app/tests/mesh_body_state.rs`)

```rust
//! Netz-Körper in der Oberfläche: Maße, Lage, Sperre, Vorschau, Auswahl (Teilprojekt 9,
//! Task 12).

use planora_app::mesh_body::*;
use planora_app::state::*;
use planora_exchange::{ExchangeMesh, MeshReport, MeshTransform};
use planora_feature::{BlendArt, MESH_LOCKED};
use planora_kernel::Id;
use planora_kernel::boolean::BoolOp;
use planora_math::DVec3;
use planora_render::{FaceRef, Pick};
use std::sync::Arc;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    })
}

/// Quader 10 mm und der Würfel als Netz-Körper in Zentimetern (gewählt).
fn with_mesh() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.import_mesh("Würfel", cube(), 10.0).unwrap();
    s
}

#[test]
fn an_imported_mesh_becomes_a_selected_body_with_its_measures() {
    let s = with_mesh();
    assert_eq!(s.bodies.len(), 2);
    let i = s.selected_body.unwrap();
    let b = &s.bodies[i];
    assert_eq!((b.kind, b.name.as_str()), (BodyKind::Mesh, "Würfel"));
    assert!((b.mass.volume - 1000.0).abs() < 1e-9, "{}", b.mass.volume);
    assert_eq!(b.bounds, (DVec3::ZERO, DVec3::splat(10.0)));
    assert!(b.mesh_report.unwrap().closed);
    assert!(!b.preview);
    assert_eq!(b.mesh.positions.len(), 24, "harte Kanten: drei Punkte je Ecke");
    assert_eq!(b.mesh.normals.len(), 24);
    assert_eq!(b.mesh.face_ranges.len(), 1);
    assert!(b.mesh.edge_lines.is_empty(), "keine Kantenlinien");
    assert!(s.is_mesh(i) && !s.is_mesh(1 - i));
    assert_eq!(s.solid_body_count(), 1);
    assert!(s.selected_is_mesh());
    s.document.check_invariants().unwrap();
}

#[test]
fn moving_rotating_and_scaling_change_the_mesh_body_itself() {
    let mut s = with_mesh();
    let count = s.document.features().len();
    let i = s.selected_body.unwrap();
    s.move_body(i, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    assert_eq!(s.document.features().len(), count, "kein neues Verschieben");
    let i = s.selected_body.unwrap();
    assert_eq!(s.mesh_transform(i).unwrap().offset, DVec3::new(5.0, 0.0, 0.0));
    let t = MeshTransform {
        offset: DVec3::new(5.0, 0.0, 0.0),
        rotation: rotation_from_degrees(DVec3::new(0.0, 0.0, 90.0)),
        scale: 20.0,
    };
    s.set_mesh_transform(i, t).unwrap();
    let b = &s.bodies[s.selected_body.unwrap()];
    assert!((b.mass.volume - 8000.0).abs() < 1e-6, "{}", b.mass.volume);
    assert!(
        (b.bounds.0.x + 15.0).abs() < 1e-9 && (b.bounds.1.y - 20.0).abs() < 1e-9,
        "{:?}",
        b.bounds
    );
    let d = euler_degrees(t.rotation);
    assert!((d - DVec3::new(0.0, 0.0, 90.0)).length() < 1e-9, "{d}");
    // ungültiger Maßstab: abgelehnt, Lage bleibt
    let bad = MeshTransform { scale: 0.0, ..t };
    assert!(s.set_mesh_transform(s.selected_body.unwrap(), bad).is_err());
    assert_eq!(s.mesh_transform(s.selected_body.unwrap()), Some(t));
    assert!(s.message.as_deref().unwrap().contains("Maßstab"));
    // kein Netz-Körper
    let q = 1 - s.selected_body.unwrap();
    assert!(s.set_mesh_transform(q, t).is_err());
    assert_eq!(s.mesh_transform(q), None);
}

#[test]
fn mesh_bodies_refuse_booleans_blends_and_sketches() {
    let mut s = with_mesh();
    let m = s.selected_body.unwrap();
    let q = 1 - m;
    let count = s.document.features().len();
    for (a, b) in [(q, m), (m, q)] {
        assert_eq!(s.add_boolean(a, b, BoolOp::Union), Err(MESH_LOCKED.to_string()));
    }
    assert_eq!(
        s.add_blend(m, &[], BlendArt::Rundung { radius: 1.0 }, false),
        Err(MESH_LOCKED.to_string())
    );
    assert_eq!(
        s.new_sketch_on_face(m, Id::from_index(0)),
        Err(MESH_LOCKED.to_string())
    );
    assert_eq!(s.document.features().len(), count);
    assert_eq!(s.message.as_deref(), Some(MESH_LOCKED));
}

#[test]
fn very_large_meshes_are_shown_as_a_preview() {
    let mut s = AppState::new();
    s.display_triangles = 5;
    s.import_mesh("Groß", cube(), 1.0).unwrap();
    let b = &s.bodies[0];
    assert!(b.preview);
    assert_eq!(b.mesh.indices.len() / 3, 4, "jedes dritte Dreieck: 12 / 3");
    assert!((b.mass.volume - 1.0).abs() < 1e-12, "Maße aus dem ganzen Netz");
    assert_eq!(b.mesh_report.unwrap().triangles, 12);
    assert!(s.mesh_note().unwrap().contains(PREVIEW_NOTE));
    assert_eq!(DISPLAY_TRIANGLES, 5_000_000);
    assert_eq!(AppState::new().display_triangles, DISPLAY_TRIANGLES);
}

#[test]
fn import_with_an_empty_name_counts_and_a_failed_import_changes_nothing() {
    let mut s = AppState::new();
    s.import_mesh("  ", cube(), 1.0).unwrap();
    assert_eq!(s.bodies[0].name, "Netz-Körper 1");
    let before = (s.document.features().len(), s.document.edit_count());
    assert!(s.import_mesh("Weit", cube(), 1e9).is_err(), "Maßstab über 1e6");
    assert_eq!(
        (s.document.features().len(), s.document.edit_count()),
        before
    );
    s.import_mesh("", cube(), 1.0).unwrap();
    assert!(s.bodies.iter().any(|b| b.name == "Netz-Körper 2"));
}

#[test]
fn selection_survives_a_refresh_and_the_state_reloads() {
    let mut s = with_mesh();
    let m = s.selected_body.unwrap();
    s.set_selection(Some(Pick::Face(FaceRef {
        body: m,
        face: Id::from_index(0),
    })));
    s.refresh();
    assert!(s.selection.is_some(), "Auswahl auf dem Netz-Körper bleibt");
    let back = AppState::from_snapshot(s.snapshot()).unwrap();
    assert_eq!(back.bodies.len(), 2);
    assert!(back.bodies.iter().any(|b| b.kind == BodyKind::Mesh));
}

#[test]
fn report_texts_are_plain_german() {
    let closed = MeshReport {
        triangles: 12,
        vertices: 8,
        closed: true,
        ..MeshReport::default()
    };
    assert_eq!(report_text(&closed), "Das Netz ist geschlossen.");
    let open = MeshReport {
        open_edges: 1,
        flipped_edges: 2,
        degenerate_triangles: 1,
        ..MeshReport::default()
    };
    assert_eq!(
        report_text(&open),
        "Das Netz ist nicht ganz geschlossen: 1 offene Kante, 2 Kanten mit falsch herum \
         liegenden Dreiecken. 1 Dreieck hat keine Fläche."
    );
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test mesh_body_state`
Expected: FAIL – `could not find mesh_body in planora_app`.

- [ ] **Step 3: Anzeige-Hilfen schreiben**

In `crates/planora-app/Cargo.toml` unter `[dependencies]` nach `planora-project.workspace = true` einfügen: `planora-exchange.workspace = true`. In `crates/planora-app/src/lib.rs` nach `pub mod icons;` einfügen: `pub mod mesh_body;`.

`crates/planora-app/src/mesh_body.rs`:

```rust
//! Netz-Körper in der Oberfläche (Teilprojekt 9, Plan P13): Anzeige-Netz mit harten Kanten,
//! Vorschau bei sehr großen Netzen, Maße aus dem ganzen Netz, Drehung in Grad, Berichtstext.

use crate::state::{Body, BodyKind};
use planora_exchange::{ExchangeMesh, MeshReport, MeshTransform, SMOOTH_ANGLE_DEG, shade};
use planora_feature::BodyRef;
use planora_kernel::{Id, Solid};
use planora_math::{DQuat, DVec3, EulerRot};
use planora_mesh::{FaceRange, Mesh, mass_properties};

/// Ab so vielen Dreiecken zeigt die Ansicht nur jedes k-te Dreieck (Spec §11); gespeichert und
/// exportiert wird immer das ganze Netz.
pub const DISPLAY_TRIANGLES: usize = 5_000_000;

/// Hinweis in der Statuszeile für einen Netz-Körper in der Vorschau.
pub const PREVIEW_NOTE: &str = "Hinweis: Das Netz ist sehr groß; die Ansicht zeigt nur einen Teil der Dreiecke. Gespeichert und exportiert wird es vollständig.";

/// Farbe für Warnungen (nicht geschlossenes Netz).
pub const WARNING_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 150, 40);

/// Netz für die Ansicht: Punkte nach der Lage `t`, Normalen mit harten Kanten (30°), eine
/// Fläche über das ganze Netz, keine Kantenlinien. Über `max_triangles` Dreiecken nur jedes
/// k-te Dreieck; dann ist der zweite Wert `true` (Vorschau).
pub fn display_mesh(mesh: &ExchangeMesh, t: &MeshTransform, max_triangles: usize) -> (Mesh, bool) {
    let stride = mesh.triangle_count().div_ceil(max_triangles.max(1)).max(1);
    let positions: Vec<[f32; 3]> = (0..mesh.positions.len())
        .map(|i| {
            let p = t.apply(mesh.point(i));
            [p.x as f32, p.y as f32, p.z as f32]
        })
        .collect();
    let indices: Vec<u32> = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .step_by(stride)
        .flatten()
        .copied()
        .collect();
    let shaded = shade(&ExchangeMesh { positions, indices }, SMOOTH_ANGLE_DEG);
    let to_d = |p: &[f32; 3]| DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
    let index_count = shaded.indices.len() as u32;
    let out = Mesh {
        positions: shaded.positions.iter().map(to_d).collect(),
        normals: shaded.normals.iter().map(to_d).collect(),
        indices: shaded.indices,
        face_ranges: vec![FaceRange {
            face: Id::from_index(0),
            first_index: 0,
            index_count,
        }],
        edge_lines: Vec::new(),
        inexact_faces: Vec::new(),
    };
    (out, stride > 1)
}

/// Körper der Oberfläche für einen Netz-Körper: Anzeige-Netz, Volumen, Oberfläche und Hülle
/// aus dem ganzen Netz, leeres `Solid`.
pub fn make_body(
    name: String,
    source: BodyRef,
    mesh: &ExchangeMesh,
    t: &MeshTransform,
    report: MeshReport,
    max_triangles: usize,
) -> Body {
    let placed = mesh.placed(t);
    let mass = mass_properties(&placed);
    let bounds = placed.positions.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let (display, preview) = display_mesh(mesh, t, max_triangles);
    Body {
        name,
        kind: BodyKind::Mesh,
        source,
        solid: Solid::default(),
        mesh: display,
        mass,
        bounds,
        mesh_inexact: false,
        mesh_report: Some(report),
        preview,
    }
}

/// Drehung als Euler-Winkel in Grad (Reihenfolge XYZ, Plan P17).
pub fn euler_degrees(q: DQuat) -> DVec3 {
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    DVec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

/// Drehung aus Euler-Winkeln in Grad (Reihenfolge XYZ), normiert.
pub fn rotation_from_degrees(d: DVec3) -> DQuat {
    DQuat::from_euler(
        EulerRot::XYZ,
        d.x.to_radians(),
        d.y.to_radians(),
        d.z.to_radians(),
    )
    .normalize()
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Prüfbericht in einfachen Worten.
pub fn report_text(r: &MeshReport) -> String {
    let mut parts = Vec::new();
    if r.open_edges > 0 {
        parts.push(count(r.open_edges, "offene Kante", "offene Kanten"));
    }
    if r.flipped_edges > 0 {
        parts.push(count(
            r.flipped_edges,
            "Kante mit falsch herum liegenden Dreiecken",
            "Kanten mit falsch herum liegenden Dreiecken",
        ));
    }
    if r.non_manifold_edges > 0 {
        parts.push(count(
            r.non_manifold_edges,
            "Kante an mehr als zwei Dreiecken",
            "Kanten an mehr als zwei Dreiecken",
        ));
    }
    let mut text = if r.closed {
        "Das Netz ist geschlossen.".to_string()
    } else if parts.is_empty() {
        "Das Netz ist nicht geschlossen.".to_string()
    } else {
        format!("Das Netz ist nicht ganz geschlossen: {}.", parts.join(", "))
    };
    if r.degenerate_triangles > 0 {
        text.push(' ');
        text.push_str(&count(
            r.degenerate_triangles,
            "Dreieck hat keine Fläche.",
            "Dreiecke haben keine Fläche.",
        ));
    }
    text
}
```

- [ ] **Step 4: Zustand erweitern** (`crates/planora-app/src/state.rs`)

Importe: `use planora_feature::{ … };` um `MESH_LOCKED, MeshId` ergänzen; `use planora_math::{DVec2, DVec3, Frame};` ersetzen durch `use planora_math::{DQuat, DVec2, DVec3, Frame};`; neu `use planora_exchange::{ExchangeMesh, MeshReport, MeshTransform, check_mesh};`.

In `pub struct Body` nach `pub mesh_inexact: bool,` einfügen:

```rust
    /// Prüfbericht des Netzes; nur bei Netz-Körpern (Spec §6).
    pub mesh_report: Option<MeshReport>,
    /// Die Ansicht zeigt nur einen Teil der Dreiecke (Netz-Körper über `display_triangles`).
    pub preview: bool,
```

In `pub struct AppState` nach `drag_move: Option<FeatureId>,` einfügen:

```rust
    /// Prüfberichte der Netze, einmal je Netz berechnet (Plan P13).
    mesh_reports: BTreeMap<MeshId, MeshReport>,
    /// Ab so vielen Dreiecken zeigt die Ansicht Netz-Körper als Vorschau (Spec §11).
    pub display_triangles: usize,
```

In `AppState::new` im Strukturausdruck nach `drag_move: None,` einfügen: `mesh_reports: BTreeMap::new(),` und `display_triangles: crate::mesh_body::DISPLAY_TRIANGLES,`.

In `fn refresh_following` zwischen der Schleife `for v in self.document.visible_bodies() { … }` und der Zeile `self.bodies = bodies;` einfügen:

```rust
        for v in self.document.visible_meshes() {
            let report = *self
                .mesh_reports
                .entry(v.mesh_id)
                .or_insert_with(|| check_mesh(v.mesh));
            bodies.push(crate::mesh_body::make_body(
                v.name.clone(),
                v.body,
                v.mesh,
                &v.transform,
                report,
                self.display_triangles,
            ));
        }
        self.mesh_reports
            .retain(|id, _| self.document.mesh(*id).is_some());
```

In `fn remap_pick` den Arm

```rust
            Pick::Face(FaceRef { body, face }) => {
                let body = map(body)?;
                self.bodies[body].solid.faces.get(face)?;
                Some(Pick::Face(FaceRef { body, face }))
            }
```

ersetzen durch:

```rust
            Pick::Face(FaceRef { body, face }) => {
                let body = map(body)?;
                // Ein Netz-Körper hat eine einzige Fläche ohne Kernel-Gegenstück.
                if self.bodies[body].kind != BodyKind::Mesh {
                    self.bodies[body].solid.faces.get(face)?;
                }
                Some(Pick::Face(FaceRef { body, face }))
            }
```

In `fn add_boolean` nach dem ersten `if a == b || … { return self.fail(…); }` einfügen:

```rust
        if self.is_mesh(a) || self.is_mesh(b) {
            return self.fail(MESH_LOCKED.into());
        }
```

In `fn add_blend_first` als erste Anweisung einfügen:

```rust
        if self.is_mesh(body) {
            return self.fail(MESH_LOCKED.into());
        }
```

In `fn new_sketch_on_face` als erste Anweisung einfügen:

```rust
        if self.is_mesh(body) {
            return self.fail(MESH_LOCKED.into());
        }
```

In `fn move_body_with` nach `if delta == DVec3::ZERO { return Ok(()); }` einfügen:

```rust
        // Netz-Körper verschieben sich über ihre eigene Lage (Plan P10), ohne Feature.
        if self.is_mesh(index) {
            return self.move_mesh_body(r, delta);
        }
```

In `fn make_body` im Strukturausdruck `Ok(Body { … })` nach `mesh_inexact,` einfügen: `mesh_report: None,` und `preview: false,`.

`pub fn mesh_note` vollständig ersetzen:

```rust
    /// Hinweis für die Statuszeile, wenn das Netz eines sichtbaren Körpers ungenau ist oder nur
    /// als Vorschau gezeigt wird: mit dem Namen des Körpers (des gewählten, wenn er betroffen
    /// ist, sonst des ersten). Ändert nichts.
    pub fn mesh_note(&self) -> Option<String> {
        let noted = |b: &&Body| b.mesh_inexact || b.preview;
        let chosen = self
            .selected_body
            .and_then(|i| self.bodies.get(i))
            .filter(noted);
        let body = chosen.or_else(|| self.bodies.iter().find(noted))?;
        let note = if body.preview {
            crate::mesh_body::PREVIEW_NOTE
        } else {
            MESH_NOTE
        };
        Some(format!("„{}“: {note}", body.name))
    }
```

Am Ende der Datei (nach `fn body_kind`) einen neuen Block anfügen:

```rust
// ---- Netz-Körper ------------------------------------------------------------------------------

/// Bezeichnung (und Namensstamm) eingelesener Netze.
const MESH_LABEL: &str = "Netz-Körper";

/// Schreibt die Lage `t` in einen Netz-Körper.
fn set_transform(kind: &mut FeatureKind, t: MeshTransform) {
    if let FeatureKind::MeshBody {
        offset,
        rotation,
        scale,
        ..
    } = kind
    {
        (*offset, *rotation, *scale) = (t.offset, t.rotation, t.scale);
    }
}

impl AppState {
    /// Ist der Körper `index` ein Netz-Körper?
    pub fn is_mesh(&self, index: usize) -> bool {
        self.bodies
            .get(index)
            .is_some_and(|b| b.kind == BodyKind::Mesh)
    }

    /// Ist der gewählte Körper ein Netz-Körper?
    pub fn selected_is_mesh(&self) -> bool {
        self.selected_body.is_some_and(|i| self.is_mesh(i))
    }

    /// Anzahl der sichtbaren Körper, die keine Netz-Körper sind.
    pub fn solid_body_count(&self) -> usize {
        self.bodies
            .iter()
            .filter(|b| b.kind != BodyKind::Mesh)
            .count()
    }

    /// Lage des Netz-Körpers `index`; `None` für andere Körper.
    pub fn mesh_transform(&self, index: usize) -> Option<MeshTransform> {
        let b = self.bodies.get(index).filter(|b| b.kind == BodyKind::Mesh)?;
        self.document.mesh_body(b.source.feature).map(|(_, t)| t)
    }

    /// Legt ein eingelesenes Netz als Netz-Körper an (Lage: Ursprung, Maßstab `unit_scale` =
    /// Millimeter je Einheit der Datei) und wählt ihn. Ein leerer Name ergibt „Netz-Körper N“.
    /// Schlägt es fehl, bleibt das Dokument unverändert (Spec §5).
    pub fn import_mesh(
        &mut self,
        name: &str,
        mesh: Arc<ExchangeMesh>,
        unit_scale: f64,
    ) -> Result<FeatureId, String> {
        self.try_close_sketch()?;
        let trimmed = name.trim();
        let (name, counted) = if trimmed.is_empty() {
            (self.next_name(MESH_LABEL), true)
        } else {
            (trimmed.to_string(), false)
        };
        let t = MeshTransform {
            offset: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            scale: unit_scale,
        };
        match self.document.add_mesh_body(name, mesh, t) {
            Ok(id) => {
                if counted {
                    self.count_name(MESH_LABEL);
                }
                self.hover = None;
                self.selection = None;
                self.selected_feature = None;
                self.refresh();
                self.selected_body = self.body_index(BodyRef {
                    feature: id,
                    index: 0,
                });
                Ok(id)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Setzt die Lage des Netz-Körpers `index`. Lässt er sich damit nicht berechnen, wird die
    /// alte Lage zurückgeschrieben und die Meldung steht in `message`.
    pub fn set_mesh_transform(&mut self, index: usize, t: MeshTransform) -> Result<(), String> {
        let Some(r) = self
            .bodies
            .get(index)
            .filter(|b| b.kind == BodyKind::Mesh)
            .map(|b| b.source)
        else {
            return self.fail("Das ist kein Netz-Körper".into());
        };
        self.set_mesh_transform_of(r, t)
    }

    fn move_mesh_body(&mut self, r: BodyRef, delta: DVec3) -> Result<(), String> {
        let Some(mut t) = self.document.mesh_body(r.feature).map(|(_, t)| t) else {
            return self.fail("Körper nicht gefunden".into());
        };
        t.offset += delta;
        self.set_mesh_transform_of(r, t)
    }

    fn set_mesh_transform_of(&mut self, r: BodyRef, t: MeshTransform) -> Result<(), String> {
        self.try_close_sketch()?;
        let Some(old) = self.document.mesh_body(r.feature).map(|(_, t)| t) else {
            return self.fail("Körper nicht gefunden".into());
        };
        if old == t {
            return Ok(());
        }
        if let Err(e) = self.document.update(r.feature, |k| set_transform(k, t)) {
            return self.fail(e.to_string());
        }
        match self.document.feature(r.feature).map(|f| f.status.clone()) {
            Some(FeatureStatus::Ok) => {
                self.refresh();
                Ok(())
            }
            status => {
                let _ = self.document.update(r.feature, |k| set_transform(k, old));
                self.refresh();
                self.fail(status_message(status))
            }
        }
    }
}
```

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (7 Tests in `mesh_body_state`), alle bisherigen App-Tests grün.

```bash
git add Cargo.lock crates/planora-app/Cargo.toml crates/planora-app/src/lib.rs crates/planora-app/src/mesh_body.rs crates/planora-app/src/state.rs crates/planora-app/tests/mesh_body_state.rs
git commit -m "app: Netz-Körper anzeigen, bewegen und sperren, Vorschau bei großen Netzen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Eigenschaften, ausgegraute Menüpunkte und Körperwahl der Booleschen Operation

**Files:**
- Modify: `crates/planora-app/src/properties.rs`, `crates/planora-app/src/state.rs`, `crates/planora-app/src/app.rs` (`edit_menu`, `sketch_menu`), `crates/planora-app/src/boolean_dialog.rs`
- Test: `crates/planora-app/tests/mesh_body_ui.rs`

**Interfaces:**
- Consumes: Task 12.
- Produces:
  ```rust
  // planora_app::state
  pub const MESH_LOCKED_HINT: &str = "Netz-Körper lassen sich nicht bearbeiten.";
  impl AppState { pub fn boolean_blocked(&self) -> Option<&'static str>;
      pub fn blend_blocked(&self) -> Option<&'static str>; pub fn sketch_on_face_blocked(&self) -> Option<&'static str>; }
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-app/tests/mesh_body_ui.rs`)

```rust
//! Netz-Körper im Fenster: Eigenschaften, gesperrte Menüpunkte, Körperwahl, Symbol
//! (Teilprojekt 9, Task 13).

use planora_app::boolean_dialog::BooleanDialog;
use planora_app::icons::FeatureIcon;
use planora_app::state::*;
use planora_app::{fonts, properties};
use planora_exchange::ExchangeMesh;
use planora_kernel::boolean::BoolOp;
use planora_math::DVec3;
use std::sync::Arc;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

#[test]
fn the_properties_of_a_mesh_body_show_measures_transform_and_report() {
    let ctx = context();
    let mut s = AppState::new();
    let mut open = cube();
    open.indices.truncate(33);
    s.import_mesh("Offen", Arc::new(open), 1.0).unwrap();
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| properties::show(ui, &mut s));
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    for word in [
        "Netz-Körper",
        "Volumen",
        "Abmessungen",
        "Position",
        "Drehung",
        "Maßstab",
        "Dreiecke",
        "Geschlossen",
        "nein",
        "3 offene Kanten",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    assert_eq!(s.document.features().len(), 1, "Zeichnen ändert nichts");
    // Feature gewählt: Parameter des Netz-Körpers
    let id = s.document.features()[0].id;
    let kind = s.document.feature(id).unwrap().kind.clone();
    let rows = properties::parameter_rows(&s, &kind);
    let labels: Vec<&str> = rows.iter().map(|(l, _)| *l).collect();
    assert_eq!(labels, ["Netz", "Position", "Drehung", "Maßstab"]);
    assert_eq!(rows[0].1, "11 Dreiecke");
    assert_eq!(FeatureIcon::of(&kind), FeatureIcon::Body(BodyKind::Mesh));
}

#[test]
fn the_edit_menu_rules_name_the_lock() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_box(DVec3::splat(5.0)).unwrap();
    assert_eq!(s.boolean_blocked(), None);
    s.import_mesh("N", Arc::new(cube()), 1.0).unwrap(); // jetzt gewählt
    assert_eq!(s.boolean_blocked(), Some(MESH_LOCKED_HINT));
    assert_eq!(s.blend_blocked(), Some(MESH_LOCKED_HINT));
    assert_eq!(s.sketch_on_face_blocked(), Some(MESH_LOCKED_HINT));
    s.selected_body = Some(0);
    assert_eq!(s.boolean_blocked(), None);
    assert_eq!(s.blend_blocked(), None);
    assert!(s.sketch_on_face_blocked().unwrap().contains("ebene Fläche"));
    let mut only_mesh = AppState::new();
    only_mesh.import_mesh("N", Arc::new(cube()), 1.0).unwrap();
    only_mesh.selected_body = None;
    assert!(only_mesh.boolean_blocked().unwrap().contains("zwei Körper"));
    assert!(only_mesh.blend_blocked().unwrap().contains("ein Körper"));
}

#[test]
fn the_boolean_dialog_offers_only_ordinary_bodies() {
    let mut s = AppState::new();
    s.import_mesh("N", Arc::new(cube()), 1.0).unwrap();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_box(DVec3::splat(5.0)).unwrap();
    s.selected_body = s.bodies.iter().position(|b| b.kind == BodyKind::Mesh);
    let d = BooleanDialog::with_op(&s, BoolOp::Union);
    assert!(!s.is_mesh(d.a) && !s.is_mesh(d.b) && d.a != d.b, "{} {}", d.a, d.b);
    let ctx = context();
    let mut d = d;
    ctx.run_ui(egui::RawInput::default(), |ui| {
        assert!(d.show(ui.ctx(), &mut s));
    })
    .drop_without_applying_deltas();
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test mesh_body_ui`
Expected: FAIL – `cannot find function boolean_blocked` / `MESH_LOCKED_HINT`.

- [ ] **Step 3: Regeln im Zustand** (`crates/planora-app/src/state.rs`)

Nach `const MESH_LABEL: &str = "Netz-Körper";` einfügen:

```rust
/// Hinweis an ausgegrauten Menüpunkten bei gewähltem Netz-Körper (Spec §6).
pub const MESH_LOCKED_HINT: &str = "Netz-Körper lassen sich nicht bearbeiten.";
```

Im Block `impl AppState` des Abschnitts „Netz-Körper“ ergänzen:

```rust
    /// Warum „Vereinigen“, „Abziehen“ und „Schneiden“ gerade nicht gehen (`None`: sie gehen).
    pub fn boolean_blocked(&self) -> Option<&'static str> {
        if self.selected_is_mesh() {
            Some(MESH_LOCKED_HINT)
        } else if self.solid_body_count() < 2 {
            Some("Dafür werden mindestens zwei Körper benötigt.")
        } else {
            None
        }
    }

    /// Warum „Verrunden“ und „Fase“ gerade nicht gehen (`None`: sie gehen).
    pub fn blend_blocked(&self) -> Option<&'static str> {
        if self.selected_is_mesh() {
            Some(MESH_LOCKED_HINT)
        } else if self.solid_body_count() == 0 {
            Some("Dafür wird mindestens ein Körper benötigt.")
        } else {
            None
        }
    }

    /// Warum „Skizze auf gewählter Fläche“ gerade nicht geht (`None`: es geht).
    pub fn sketch_on_face_blocked(&self) -> Option<&'static str> {
        if self.selected_is_mesh() {
            return Some(MESH_LOCKED_HINT);
        }
        let planar = match self.selection {
            Some(Pick::Face(FaceRef { body, face })) => self
                .bodies
                .get(body)
                .is_some_and(|b| plane_of_face(&b.solid, face).is_some()),
            _ => false,
        };
        if planar {
            None
        } else {
            Some("Dafür zuerst eine ebene Fläche eines Körpers anklicken.")
        }
    }
```

- [ ] **Step 4: Menüs und Körperwahl**

In `crates/planora-app/src/app.rs`, `fn edit_menu`: die beiden Schleifen über die Booleschen Operationen und über „Verrunden …“/„Fase …“ samt der Zeilen `let enabled = tab.state.bodies.len() >= 2;` und `let any = !tab.state.bodies.is_empty();` ersetzen durch:

```rust
        // Boolesche Operationen brauchen zwei gewöhnliche Körper; Netz-Körper sind gesperrt.
        let boolean = tab.state.boolean_blocked();
        for (label, op) in [
            ("Vereinigen …", BoolOp::Union),
            ("Abziehen …", BoolOp::Difference),
            ("Schneiden …", BoolOp::Intersection),
        ] {
            if ui
                .add_enabled(boolean.is_none(), egui::Button::new(label))
                .on_disabled_hover_text(boolean.unwrap_or_default())
                .clicked()
            {
                if tab.state.try_close_sketch().is_ok() {
                    tab.boolean_dialog = Some(BooleanDialog::with_op(&tab.state, op));
                }
                ui.close();
            }
        }
        ui.separator();
        let blend = tab.state.blend_blocked();
        for (label, dialog) in [
            ("Verrunden …", BlendDialog::fillet()),
            ("Fase …", BlendDialog::chamfer()),
        ] {
            if ui
                .add_enabled(blend.is_none(), egui::Button::new(label))
                .on_disabled_hover_text(blend.unwrap_or_default())
                .clicked()
            {
                if tab.state.try_close_sketch().is_ok() {
                    tab.blend_dialog = Some(dialog);
                }
                ui.close();
            }
        }
```

In `fn sketch_menu` den Ausdruck

```rust
        if ui
            .add_enabled(
                face.is_some(),
                egui::Button::new("Skizze auf gewählter Fläche"),
            )
            .on_disabled_hover_text("Dafür zuerst eine ebene Fläche eines Körpers anklicken.")
            .clicked()
```

ersetzen durch:

```rust
        let blocked = state.sketch_on_face_blocked();
        if ui
            .add_enabled(
                face.is_some() && blocked.is_none(),
                egui::Button::new("Skizze auf gewählter Fläche"),
            )
            .on_disabled_hover_text(
                blocked.unwrap_or("Dafür zuerst eine ebene Fläche eines Körpers anklicken."),
            )
            .clicked()
```

In `crates/planora-app/src/boolean_dialog.rs` in `with_op` die Berechnung von `a` und `b` ersetzen durch:

```rust
        // Netz-Körper lassen sich nicht verrechnen (Spec §6): nur gewöhnliche Körper anbieten.
        let a = state
            .selected_body
            .filter(|&i| i < state.bodies.len() && !state.is_mesh(i))
            .or_else(|| (0..state.bodies.len()).find(|&i| !state.is_mesh(i)))
            .unwrap_or(0);
        let b = (0..state.bodies.len())
            .find(|&i| i != a && !state.is_mesh(i))
            .unwrap_or(0);
```

und in `show` die Zeile `let names: Vec<String> = state.bodies.iter().map(|b| b.name.clone()).collect();` ersetzen durch

```rust
                let names: Vec<String> = state.bodies.iter().map(|b| b.name.clone()).collect();
                let meshes: Vec<bool> = (0..names.len()).map(|i| state.is_mesh(i)).collect();
```

sowie die innere Schleife `for (i, n) in names.iter().enumerate() { ui.selectable_value(slot, i, n); }` durch

```rust
                            for (i, n) in names.iter().enumerate() {
                                if !meshes[i] {
                                    ui.selectable_value(slot, i, n);
                                }
                            }
```

- [ ] **Step 5: Eigenschaften des Netz-Körpers** (`crates/planora-app/src/properties.rs`)

Importe ergänzen:

```rust
use crate::mesh_body::{WARNING_COLOR, euler_degrees, report_text, rotation_from_degrees};
use planora_exchange::MeshTransform;
use planora_feature::{MAX_MESH_SCALE, MIN_MESH_SCALE};
```

In `fn body(ui, state, index)` als erste Anweisungen einfügen:

```rust
    if state.is_mesh(index) {
        mesh_body(ui, state, index);
        return;
    }
```

Nach `fn body` anfügen:

```rust
/// Eigenschaften eines Netz-Körpers (Spec §6): Name, Maße, Lage, Drehung in Grad, Maßstab und
/// Prüfbericht. Änderungen wirken direkt auf den Netz-Körper (kein neues Feature).
fn mesh_body(ui: &mut egui::Ui, state: &mut AppState, index: usize) {
    let Some(t) = state.mesh_transform(index) else {
        return;
    };
    let shown_degrees = euler_degrees(t.rotation);
    let (mut offset, mut degrees, mut scale) = (t.offset, shown_degrees, t.scale);
    let renamed;
    {
        let body = &state.bodies[index];
        ui.label("Name");
        renamed = name_field(ui, state.naming_feature(index), &body.name);
        ui.add_space(6.0);
        let size = body.bounds.1 - body.bounds.0;
        egui::Grid::new("mesh_props").num_columns(2).show(ui, |ui| {
            ui.label("Art");
            ui.label("Netz-Körper");
            ui.end_row();
            ui.label("Volumen");
            ui.label(format!("≈ {:.2} mm³", body.mass.volume));
            ui.end_row();
            ui.label("Oberfläche");
            ui.label(format!("≈ {:.2} mm²", body.mass.surface_area));
            ui.end_row();
            ui.label("Abmessungen");
            ui.label(format!("{:.2} × {:.2} × {:.2} mm", size.x, size.y, size.z));
            ui.end_row();
            ui.label("Position (x/y/z)");
            ui.horizontal(|ui| {
                for v in [&mut offset.x, &mut offset.y, &mut offset.z] {
                    ui.add(egui::DragValue::new(v).speed(0.5).suffix(" mm"));
                }
            });
            ui.end_row();
            ui.label("Drehung (x/y/z)");
            ui.horizontal(|ui| {
                for v in [&mut degrees.x, &mut degrees.y, &mut degrees.z] {
                    ui.add(egui::DragValue::new(v).speed(1.0).suffix(" °"));
                }
            });
            ui.end_row();
            ui.label("Maßstab");
            ui.add(
                egui::DragValue::new(&mut scale)
                    .speed(0.01)
                    .range(MIN_MESH_SCALE..=MAX_MESH_SCALE),
            );
            ui.end_row();
            if let Some(r) = body.mesh_report {
                ui.label("Dreiecke");
                ui.label(r.triangles.to_string());
                ui.end_row();
                ui.label("Geschlossen");
                ui.label(if r.closed { "ja" } else { "nein" });
                ui.end_row();
            }
        });
        if let Some(r) = body.mesh_report
            && !r.closed
        {
            ui.colored_label(WARNING_COLOR, report_text(&r));
        }
        ui.add_space(4.0);
        ui.weak(
            "Netz-Körper lassen sich verschieben, drehen, vergrößern, messen und exportieren, \
             aber nicht mit anderen Körpern verrechnen, runden oder als Fläche für eine Skizze nutzen.",
        );
    }
    if let (Some(name), Some(id)) = (renamed, state.naming_feature(index)) {
        let _ = state.rename_feature(id, &name);
    }
    // Die Drehung wird nur neu gebildet, wenn ein Winkel geändert wurde (sonst bliebe sie durch
    // das Hin- und Zurückrechnen nicht bitgleich).
    let rotation = if degrees == shown_degrees {
        t.rotation
    } else {
        rotation_from_degrees(degrees)
    };
    let new = MeshTransform {
        offset,
        rotation,
        scale,
    };
    if new != t {
        let _ = state.set_mesh_transform(index, new);
    }
}
```

- [ ] **Step 6: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (3 Tests in `mesh_body_ui`), bisherige Tests (u. a. `boolean_ui`, `boolean_state`, `tree_ui`) grün.

```bash
git add crates/planora-app/src/properties.rs crates/planora-app/src/state.rs crates/planora-app/src/app.rs crates/planora-app/src/boolean_dialog.rs crates/planora-app/tests/mesh_body_ui.rs
git commit -m "app: Eigenschaften der Netz-Körper und ausgegraute Menüpunkte mit Hinweis

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 14: Stufentest 3 – vom Netz in der Datei über das Dokument zurück in eine Datei

**Files:**
- Test: `crates/planora-app/tests/mesh_flow.rs`

**Interfaces:**
- Consumes: Task 9 (`export_file`, `import_file`, `collect_body`, `SolidSource`, `PlacedSource`), Task 11 (`encode_version`, `decode_version`), Task 12.
- Produces: nur Tests; Stufenabschluss 3.

- [ ] **Step 1: Stufentest schreiben** (`crates/planora-app/tests/mesh_flow.rs`)

```rust
//! Stufentest 3: Netz einlesen, als Netz-Körper bewegen, speichern (Format 3), laden und
//! wieder exportieren (Teilprojekt 9, Task 14).

use planora_app::state::*;
use planora_exchange::*;
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DQuat, DVec3, PI};
use planora_mesh::{TessellationOptions, tessellate};
use planora_project::{decode_version, encode_version};
use std::sync::Arc;

#[test]
fn a_mesh_travels_from_file_to_document_and_back() {
    let dir = std::env::temp_dir().join(format!("planora-flow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let k = AnalyticKernel::default();
    let cylinder = k.make_cylinder(5.0, 10.0).unwrap();
    let mesh = tessellate(
        &cylinder,
        &TessellationOptions {
            chord_tolerance: 0.01,
        },
    )
    .unwrap();
    let body = collect_body(&SolidSource {
        name: "Zylinder",
        mesh: &mesh,
    })
    .unwrap();
    let path = dir.join("zylinder.stl");
    export_file(&path, Format::Stl, std::slice::from_ref(&body)).unwrap();
    let read = import_file(&path, &Progress::new()).unwrap();
    assert_eq!(read.triangle_count(), body.mesh.triangle_count());

    let mut s = AppState::new();
    s.import_mesh("Zylinder", Arc::new(read), 1.0).unwrap();
    let i = s.selected_body.unwrap();
    let v0 = s.bodies[i].mass.volume;
    let area = s.bodies[i].mass.surface_area;
    let exact = PI * 25.0 * 10.0;
    assert!((v0 - exact).abs() <= 0.01 * area, "{v0} / {exact}");

    let t = MeshTransform {
        offset: DVec3::new(30.0, 0.0, 0.0),
        rotation: DQuat::from_rotation_x(0.5),
        scale: 2.0,
    };
    s.set_mesh_transform(i, t).unwrap();
    let v1 = s.bodies[s.selected_body.unwrap()].mass.volume;
    assert!((v1 - 8.0 * v0).abs() < 1e-6 * v1, "{v1} / {v0}");

    // Speichern und Laden (Format 3)
    let text = encode_version(&s.snapshot()).unwrap();
    assert!(text.contains("\"format\": 3"));
    let back = AppState::from_snapshot(decode_version(&text, "x.json").unwrap()).unwrap();
    let j = back
        .bodies
        .iter()
        .position(|b| b.kind == BodyKind::Mesh)
        .unwrap();
    assert_eq!(back.bodies[j].mass.volume, v1);
    assert_eq!(back.mesh_transform(j), Some(t));

    // Wieder exportieren: das Netz mit seiner Lage
    let (m, t2) = back
        .document
        .mesh_body(back.bodies[j].source.feature)
        .unwrap();
    let again = collect_body(&PlacedSource {
        name: "Zylinder",
        mesh: m,
        transform: t2,
    })
    .unwrap();
    let path2 = dir.join("wieder.obj");
    export_file(&path2, Format::Obj, std::slice::from_ref(&again)).unwrap();
    let reread = import_file(&path2, &Progress::new()).unwrap();
    let v2 = reread.mass(&MeshTransform::IDENTITY).volume;
    assert!((v2 - v1).abs() < 1e-4 * v1, "{v2} / {v1}");
    assert_eq!(check_mesh(&reread).closed, check_mesh(&again.mesh).closed);
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Test laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test mesh_flow`
Expected: PASS.

- [ ] **Step 3: Stufenabschluss 3**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
git diff --stat 134cde9 -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs
git diff --stat 134cde9 -- crates/planora-project/tests/data/format1.json crates/planora-project/tests/data/format1_voll.json crates/planora-project/tests/data/format2.json
```
Expected: alles grün; beide `git diff`-Ausgaben sind leer.

- [ ] **Step 4: Commit**

```bash
git add crates/planora-app/tests/mesh_flow.rs
git commit -m "app: Stufentest 3 (Netz von der Datei ins Dokument und zurück)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Stufe 4 – Fenster „Exportieren“ und „Netz importieren“, Dokumentation, Gesamtprüfung, Handtest

### Task 15: Arbeitsthreads, eigene Dateiauswahl und gleich hohe Karten

**Files:**
- Create: `crates/planora-app/src/jobs.rs`, `crates/planora-app/src/file_chooser.rs`, `crates/planora-app/src/cards.rs`
- Modify: `crates/planora-app/src/lib.rs`
- Test: `crates/planora-app/tests/exchange_infra.rs`

**Interfaces:**
- Consumes: `planora_exchange::Progress` (Task 1), `crate::browser::ERROR_COLOR`, `crate::mesh_body::WARNING_COLOR` (Task 12).
- Produces:
  ```rust
  // jobs
  pub enum Poll<T> { Pending, Ready(T), Lost }          // Debug, PartialEq
  pub struct Job<T>;
  impl<T: Send + 'static> Job<T> { pub fn spawn(work: impl FnOnce(&Progress) -> T + Send + 'static) -> Job<T>;
      pub fn spawn_with(progress: Arc<Progress>, work: impl FnOnce(&Progress) -> T + Send + 'static) -> Job<T>; }
  impl<T> Job<T> { pub fn poll(&self) -> Poll<T>; pub fn wait(&self) -> Poll<T>; pub fn cancel(&self); pub fn fraction(&self) -> f32; }
  // Drop bricht ab.
  // file_chooser
  pub const MAX_ENTRIES: usize = 2000;
  pub enum ChooserMode { Save, Open }  pub struct Entry { pub name: String, pub is_dir: bool }
  pub enum ChooserAction { None, Chosen(PathBuf), Cancel }
  pub struct FileChooser { pub mode: ChooserMode, pub dir_text: String, pub file_name: String, pub truncated: bool, pub error: Option<String>, … }
  impl FileChooser { pub fn save(dir: PathBuf, file_name: String, extension: &'static str) -> FileChooser;
      pub fn open(dir: PathBuf, extensions: Vec<&'static str>) -> FileChooser; pub fn dir(&self) -> &Path;
      pub fn entries(&self) -> &[Entry]; pub fn reload(&mut self); pub fn enter(&mut self, name: &str); pub fn up(&mut self);
      pub fn go_to_text(&mut self) -> bool; pub fn select(&mut self, name: &str); pub fn target(&self) -> Option<PathBuf>;
      pub fn confirm(&mut self) -> ChooserAction; pub fn asks_to_replace(&self) -> bool; pub fn show(&mut self, ui: &mut egui::Ui) -> ChooserAction; }
  pub fn default_dir() -> PathBuf;
  // cards
  pub fn card_row(ui: &mut egui::Ui, id_salt: &str, cards: &mut [&mut dyn FnMut(&mut egui::Ui)]) -> Vec<egui::Rect>;
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-app/tests/exchange_infra.rs`)

```rust
//! Arbeitsthreads, eigene Dateiauswahl und gleich hohe Karten (Teilprojekt 9, Task 15).

use planora_app::cards::card_row;
use planora_app::file_chooser::*;
use planora_app::fonts;
use planora_app::jobs::{Job, Poll};
use planora_exchange::{ExchangeError, Progress};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier, mpsc};

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-infra-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_job_delivers_its_result() {
    let job = Job::spawn(|p| {
        p.set_total(4);
        p.advance(4);
        42
    });
    assert_eq!(job.wait(), Poll::Ready(42));
}

#[test]
fn a_cancelled_job_sees_the_cancel_flag() {
    let progress = Arc::new(Progress::new());
    progress.cancel();
    let job = Job::spawn_with(progress, |p| p.check());
    assert_eq!(job.wait(), Poll::Ready(Err(ExchangeError::Cancelled)));
}

#[test]
fn a_panicking_job_is_lost_not_a_crash() {
    let job: Job<u8> = Job::spawn(|_| panic!("Absicht: Test für einen abgestürzten Arbeitsthread"));
    assert_eq!(job.wait(), Poll::Lost);
}

#[test]
fn dropping_a_job_cancels_it() {
    let (tx, rx) = mpsc::channel();
    let gate = Arc::new(Barrier::new(2));
    let g = Arc::clone(&gate);
    let job = Job::spawn(move |p: &Progress| {
        g.wait();
        g.wait();
        let _ = tx.send(p.is_cancelled());
    });
    gate.wait();
    drop(job);
    gate.wait();
    assert!(rx.recv().unwrap());
}

#[test]
fn the_chooser_lists_folders_first_and_only_matching_files() {
    let d = temp_dir("liste");
    fs::create_dir_all(d.join("Unterordner")).unwrap();
    for f in ["b.STL", "a.obj", "c.txt", "z.stl"] {
        fs::write(d.join(f), b"x").unwrap();
    }
    let c = FileChooser::open(d.clone(), vec!["stl", "obj"]);
    let names: Vec<(&str, bool)> = c
        .entries()
        .iter()
        .map(|e| (e.name.as_str(), e.is_dir))
        .collect();
    assert_eq!(
        names,
        [
            ("Unterordner", true),
            ("a.obj", false),
            ("b.STL", false),
            ("z.stl", false)
        ]
    );
    assert!(!c.truncated && c.error.is_none());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn saving_adds_the_ending_and_asks_before_replacing() {
    let d = temp_dir("speichern");
    let mut c = FileChooser::save(d.clone(), "Teil".into(), "3mf");
    assert_eq!(c.confirm(), ChooserAction::Chosen(d.join("Teil.3mf")));
    fs::write(d.join("Teil.3mf"), b"alt").unwrap();
    assert_eq!(c.confirm(), ChooserAction::None);
    assert!(c.asks_to_replace());
    assert_eq!(c.confirm(), ChooserAction::Chosen(d.join("Teil.3mf")));
    c.file_name = "Teil.3mf".into();
    assert_eq!(c.target(), Some(d.join("Teil.3mf")), "Endung nicht doppelt");
    c.file_name = "  ".into();
    assert_eq!(c.confirm(), ChooserAction::None);
    assert!(c.error.is_some());
    c.file_name = "../weg".into();
    assert_eq!(c.target(), None);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn navigation_moves_between_folders() {
    let d = temp_dir("wandern");
    fs::create_dir_all(d.join("innen")).unwrap();
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    c.enter("innen");
    assert_eq!(c.dir(), d.join("innen").as_path());
    c.up();
    assert_eq!(c.dir(), d.as_path());
    c.dir_text = d.join("gibt-es-nicht").display().to_string();
    assert!(!c.go_to_text());
    assert_eq!(c.dir(), d.as_path());
    assert!(c.error.is_some());
    fs::write(d.join("x.stl"), b"x").unwrap();
    c.reload();
    c.select("x.stl");
    assert_eq!(c.confirm(), ChooserAction::Chosen(d.join("x.stl")));
    c.select("fehlt.stl");
    assert_eq!(c.confirm(), ChooserAction::None);
    assert!(default_dir().is_dir() || default_dir().as_os_str().is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_chooser_draws_without_input() {
    let d = temp_dir("zeichnen");
    let mut c = FileChooser::save(d.clone(), "Teil".into(), "stl");
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    for _ in 0..2 {
        let mut action = ChooserAction::None;
        ctx.run_ui(egui::RawInput::default(), |ui| action = c.show(ui))
            .drop_without_applying_deltas();
        assert_eq!(action, ChooserAction::None);
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn cards_in_a_row_get_the_same_height_over_the_full_width() {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 600.0),
        )),
        ..Default::default()
    };
    let mut rects = Vec::new();
    for _ in 0..3 {
        ctx.run_ui(input(), |ui| {
            let mut a = |ui: &mut egui::Ui| {
                ui.label("Eine Zeile");
            };
            let mut b = |ui: &mut egui::Ui| {
                for k in 0..5 {
                    ui.label(format!("Zeile {k}"));
                }
            };
            let row: &mut [&mut dyn FnMut(&mut egui::Ui)] = &mut [&mut a, &mut b];
            rects = card_row(ui, "test", row);
        })
        .drop_without_applying_deltas();
    }
    assert_eq!(rects.len(), 2);
    assert!(
        (rects[0].height() - rects[1].height()).abs() < 0.5,
        "{rects:?}"
    );
    assert!(rects[1].min.x > rects[0].max.x, "nebeneinander");
    assert!(rects[1].max.x - rects[0].min.x > 800.0, "über die ganze Breite: {rects:?}");
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test exchange_infra`
Expected: FAIL – `could not find jobs in planora_app`.

- [ ] **Step 3: Implementieren**

In `crates/planora-app/src/lib.rs` die Zeilen `pub mod cards;` (nach `pub mod browser;`), `pub mod file_chooser;` (nach `pub mod feature_dialog;`) und `pub mod jobs;` (nach `pub mod icons;`) einfügen.

`crates/planora-app/src/jobs.rs`:

```rust
//! Arbeitsthreads für lange Rechnungen (Vernetzen, Lesen, Schreiben; Plan P15) mit Fortschritt
//! und Abbrechen. Die Oberfläche fragt jedes Bild nach dem Ergebnis.

use planora_exchange::Progress;
use std::sync::{Arc, mpsc};

/// Zustand eines Auftrags beim Nachfragen.
#[derive(Debug, PartialEq)]
pub enum Poll<T> {
    Pending,
    Ready(T),
    /// Der Thread endete ohne Ergebnis (Panik oder ließ sich nicht starten).
    Lost,
}

/// Ein laufender Auftrag. Fallenlassen bricht ihn ab (der Thread endet beim nächsten Prüfen).
pub struct Job<T> {
    rx: mpsc::Receiver<T>,
    progress: Arc<Progress>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(work: impl FnOnce(&Progress) -> T + Send + 'static) -> Job<T> {
        Job::spawn_with(Arc::new(Progress::new()), work)
    }

    /// Wie [`Job::spawn`] mit vorgegebenem Fortschritt (etwa schon abgebrochen, für Tests).
    pub fn spawn_with(
        progress: Arc<Progress>,
        work: impl FnOnce(&Progress) -> T + Send + 'static,
    ) -> Job<T> {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::clone(&progress);
        // Lässt sich kein Thread starten, ist der Sender schon fort: `poll` meldet `Lost`.
        let _ = std::thread::Builder::new()
            .name("planora-austausch".into())
            .spawn(move || {
                let _ = tx.send(work(&shared));
            });
        Job { rx, progress }
    }
}

impl<T> Job<T> {
    pub fn poll(&self) -> Poll<T> {
        match self.rx.try_recv() {
            Ok(v) => Poll::Ready(v),
            Err(mpsc::TryRecvError::Empty) => Poll::Pending,
            Err(mpsc::TryRecvError::Disconnected) => Poll::Lost,
        }
    }

    /// Wartet auf das Ergebnis (für Tests und Abläufe ohne Fenster).
    pub fn wait(&self) -> Poll<T> {
        match self.rx.recv() {
            Ok(v) => Poll::Ready(v),
            Err(_) => Poll::Lost,
        }
    }

    pub fn cancel(&self) {
        self.progress.cancel();
    }

    pub fn fraction(&self) -> f32 {
        self.progress.fraction()
    }
}

impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.progress.cancel();
    }
}
```

`crates/planora-app/src/file_chooser.rs`:

```rust
//! Eigene Dateiauswahl im Fenster (Plan P14: ohne neue Abhängigkeit statt des Dialogs des
//! Betriebssystems): Ordner wechseln oder eintippen, Datei wählen oder Namen eingeben, Ersetzen
//! einer vorhandenen Datei erst nach einem zweiten Klick.

use crate::browser::ERROR_COLOR;
use crate::mesh_body::WARNING_COLOR;
use std::path::{Path, PathBuf};

/// Höchstzahl der gezeigten Einträge eines Ordners (große Ordner bleiben bedienbar).
pub const MAX_ENTRIES: usize = 2000;
/// Höhe der Liste in Punkten.
const LIST_HEIGHT: f32 = 220.0;
const REPLACE_HINT: &str =
    "Diese Datei gibt es schon. Noch einmal klicken, um sie zu ersetzen.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChooserMode {
    Save,
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChooserAction {
    None,
    Chosen(PathBuf),
    Cancel,
}

pub struct FileChooser {
    pub mode: ChooserMode,
    dir: PathBuf,
    /// Eingabefeld des Ordners (Enter wechselt dorthin).
    pub dir_text: String,
    pub file_name: String,
    /// Endungen klein und ohne Punkt; beim Speichern genau eine.
    extensions: Vec<&'static str>,
    entries: Vec<Entry>,
    /// Der Ordner hat mehr als [`MAX_ENTRIES`] Einträge.
    pub truncated: bool,
    pub error: Option<String>,
    /// Ziel, dessen Ersetzen noch bestätigt werden muss.
    confirm: Option<PathBuf>,
}

/// Startordner: „Documents“ im Benutzerordner, sonst der Benutzerordner, sonst der aktuelle
/// Ordner.
pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    if let Some(h) = home {
        let docs = h.join("Documents");
        if docs.is_dir() {
            return docs;
        }
        if h.is_dir() {
            return h;
        }
    }
    std::env::current_dir().unwrap_or_default()
}

impl FileChooser {
    /// Auswahl zum Speichern mit vorgeschlagenem Namen; die Endung wird ergänzt.
    pub fn save(dir: PathBuf, file_name: String, extension: &'static str) -> FileChooser {
        let mut c = FileChooser::new(ChooserMode::Save, dir, vec![extension]);
        c.file_name = file_name;
        c
    }

    /// Auswahl zum Öffnen einer vorhandenen Datei mit einer der Endungen.
    pub fn open(dir: PathBuf, extensions: Vec<&'static str>) -> FileChooser {
        FileChooser::new(ChooserMode::Open, dir, extensions)
    }

    fn new(mode: ChooserMode, dir: PathBuf, extensions: Vec<&'static str>) -> FileChooser {
        let mut c = FileChooser {
            mode,
            dir_text: dir.display().to_string(),
            dir,
            file_name: String::new(),
            extensions,
            entries: Vec::new(),
            truncated: false,
            error: None,
            confirm: None,
        };
        c.reload();
        c
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Liest den Ordner neu: Unterordner zuerst, dann passende Dateien, je nach Namen ohne
    /// Groß- und Kleinschreibung sortiert.
    pub fn reload(&mut self) {
        self.entries.clear();
        self.truncated = false;
        self.error = None;
        let read = match std::fs::read_dir(&self.dir) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(format!("Der Ordner lässt sich nicht öffnen: {e}"));
                return;
            }
        };
        let (mut dirs, mut files) = (Vec::new(), Vec::new());
        for item in read.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if item.path().is_dir() {
                dirs.push(name);
            } else if self.matches(&name) {
                files.push(name);
            }
        }
        dirs.sort_by_cached_key(|s| s.to_lowercase());
        files.sort_by_cached_key(|s| s.to_lowercase());
        let all: Vec<Entry> = dirs
            .into_iter()
            .map(|name| Entry { name, is_dir: true })
            .chain(files.into_iter().map(|name| Entry {
                name,
                is_dir: false,
            }))
            .collect();
        self.truncated = all.len() > MAX_ENTRIES;
        self.entries = all.into_iter().take(MAX_ENTRIES).collect();
    }

    fn matches(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        self.extensions
            .iter()
            .any(|e| lower.ends_with(&format!(".{e}")))
    }

    fn set_dir(&mut self, dir: PathBuf) {
        self.dir_text = dir.display().to_string();
        self.dir = dir;
        self.confirm = None;
        self.reload();
    }

    pub fn enter(&mut self, name: &str) {
        let next = self.dir.join(name);
        self.set_dir(next);
    }

    pub fn up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            let parent = parent.to_path_buf();
            self.set_dir(parent);
        }
    }

    /// Wechselt in den eingetippten Ordner; `false` (mit Meldung), wenn es ihn nicht gibt.
    pub fn go_to_text(&mut self) -> bool {
        let p = PathBuf::from(self.dir_text.trim());
        if p.is_dir() {
            self.set_dir(p);
            true
        } else {
            self.error = Some("Diesen Ordner gibt es nicht.".into());
            false
        }
    }

    pub fn select(&mut self, name: &str) {
        self.file_name = name.to_string();
        self.confirm = None;
    }

    /// Ziel: beim Speichern im aktuellen Ordner, Endung ergänzt; beim Öffnen nur eine
    /// vorhandene Datei. `None` bei leerem Namen oder Namen mit Ordnerteilen.
    pub fn target(&self) -> Option<PathBuf> {
        let name = self.file_name.trim();
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return None;
        }
        match self.mode {
            ChooserMode::Save => {
                let name = if self.matches(name) {
                    name.to_string()
                } else {
                    format!("{name}.{}", self.extensions[0])
                };
                Some(self.dir.join(name))
            }
            ChooserMode::Open => {
                let p = self.dir.join(name);
                p.is_file().then_some(p)
            }
        }
    }

    /// „Speichern“ oder „Öffnen“: liefert das Ziel. Gibt es die Datei beim Speichern schon,
    /// erst beim zweiten Aufruf mit demselben Ziel.
    pub fn confirm(&mut self) -> ChooserAction {
        let Some(target) = self.target() else {
            self.error = Some(
                match self.mode {
                    ChooserMode::Save => "Bitte einen Dateinamen eingeben.",
                    ChooserMode::Open => "Bitte eine vorhandene Datei wählen.",
                }
                .into(),
            );
            return ChooserAction::None;
        };
        self.error = None;
        if self.mode == ChooserMode::Save
            && target.exists()
            && self.confirm.as_ref() != Some(&target)
        {
            self.confirm = Some(target);
            return ChooserAction::None;
        }
        ChooserAction::Chosen(target)
    }

    /// Wartet die Auswahl auf die Bestätigung zum Ersetzen?
    pub fn asks_to_replace(&self) -> bool {
        self.confirm.is_some()
    }

    /// Zeichnet die Auswahl über die volle Breite.
    pub fn show(&mut self, ui: &mut egui::Ui) -> ChooserAction {
        let mut action = ChooserAction::None;
        ui.horizontal(|ui| {
            if ui.button("Ordner höher").clicked() {
                self.up();
            }
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.dir_text).desired_width(f32::INFINITY),
            );
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.go_to_text();
            }
        });
        let (mut enter, mut pick, mut double) = (None, None, None);
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for e in &self.entries {
                    let text = if e.is_dir {
                        format!("{}  (Ordner)", e.name)
                    } else {
                        e.name.clone()
                    };
                    let selected = !e.is_dir && e.name == self.file_name;
                    let r = ui.selectable_label(selected, text.as_str());
                    if r.double_clicked() && !e.is_dir {
                        double = Some(e.name.clone());
                    } else if r.clicked() {
                        if e.is_dir {
                            enter = Some(e.name.clone());
                        } else {
                            pick = Some(e.name.clone());
                        }
                    }
                }
                if self.entries.is_empty() {
                    ui.weak("Hier gibt es keine passenden Dateien.");
                }
                if self.truncated {
                    ui.weak(format!(
                        "Es werden nur die ersten {MAX_ENTRIES} Einträge gezeigt."
                    ));
                }
            });
        if let Some(n) = enter {
            self.enter(&n);
        }
        if let Some(n) = pick {
            self.select(&n);
        }
        ui.horizontal(|ui| {
            ui.label("Dateiname");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.file_name).desired_width(f32::INFINITY),
            );
            if edit.changed() {
                self.confirm = None;
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(ERROR_COLOR, e);
        }
        if self.asks_to_replace() {
            ui.colored_label(WARNING_COLOR, REPLACE_HINT);
        }
        ui.horizontal(|ui| {
            let label = match self.mode {
                ChooserMode::Save if self.asks_to_replace() => "Ersetzen",
                ChooserMode::Save => "Speichern",
                ChooserMode::Open => "Öffnen",
            };
            if ui.button(label).clicked() {
                action = self.confirm();
            }
            if ui.button("Abbrechen").clicked() {
                action = ChooserAction::Cancel;
            }
        });
        if let Some(n) = double {
            self.select(&n);
            action = self.confirm();
        }
        action
    }
}
```

`crates/planora-app/src/cards.rs`:

```rust
//! Karten nebeneinander über die ganze Breite, alle gleich hoch (Nutzervorgabe).

/// Zeichnet `cards` nebeneinander in gleich breiten Spalten über die volle Breite. Alle Karten
/// bekommen die Höhe der höchsten; sie wird aus dem vorigen Bild gemerkt, nach einer Änderung
/// gleicht das nächste Bild sie an. Liefert die Rechtecke der Karten.
pub fn card_row(
    ui: &mut egui::Ui,
    id_salt: &str,
    cards: &mut [&mut dyn FnMut(&mut egui::Ui)],
) -> Vec<egui::Rect> {
    let mut rects = Vec::with_capacity(cards.len());
    if cards.is_empty() {
        return rects;
    }
    let id = ui.id().with(("card_row", id_salt));
    let min_height: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let mut tallest = 0.0f32;
    ui.columns(cards.len(), |columns| {
        for (column, card) in columns.iter_mut().zip(cards.iter_mut()) {
            let inner = egui::Frame::group(column.style()).show(column, |ui| {
                ui.set_width(ui.available_width());
                (*card)(ui);
                let natural = ui.min_rect().height();
                ui.set_min_height(min_height);
                natural
            });
            tallest = tallest.max(inner.inner);
            rects.push(inner.response.rect);
        }
    });
    if (tallest - min_height).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, tallest));
        ui.ctx().request_repaint();
    }
    rects
}
```

- [ ] **Step 4: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test exchange_infra && cargo test -p planora-app && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (9 Tests in `exchange_infra`; der absichtlich abstürzende Arbeitsthread schreibt eine Panikmeldung in die Testausgabe, der Test ist grün).

```bash
git add crates/planora-app/src/lib.rs crates/planora-app/src/jobs.rs crates/planora-app/src/file_chooser.rs crates/planora-app/src/cards.rs crates/planora-app/tests/exchange_infra.rs
git commit -m "app: Arbeitsthreads mit Abbrechen, eigene Dateiauswahl, gleich hohe Karten

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 16: Fenster „Exportieren …“ (Strg+Umschalt+E)

**Files:**
- Create: `crates/planora-app/src/export_dialog.rs`
- Modify: `crates/planora-app/src/lib.rs`, `crates/planora-app/src/workspace.rs` (`DocTab.export_dialog`, `Workspace.exchange_dir`), `crates/planora-app/src/app.rs` (Menü, Kürzel, Anzeige)
- Test: `crates/planora-app/tests/export_dialog.rs`

**Interfaces:**
- Consumes: Task 9 (`Format`, `export_file`), Task 2 (`collect_body`, `SolidSource`, `PlacedSource`, `check_mesh`, `MeshReport`), Task 12 (`report_text`, `WARNING_COLOR`, `is_mesh`), Task 15.
- Produces:
  ```rust
  pub const DIALOG_MARGIN: f32 = 16.0;  pub const DIALOG_TOP: f32 = 60.0;  pub const LOST: &str;  pub const NOT_CLOSED_WARNING: &str;
  pub enum Quality { Coarse, Normal, Fine }   impl Quality { pub const ALL: [Quality; 3]; pub fn chord_tolerance(self) -> f64; pub fn label(self) -> &'static str; }
  pub enum Scope { Selected, Visible }
  pub enum ExportSource { Solid(Solid), Mesh(Arc<ExchangeMesh>, MeshTransform) }
  pub struct ExportItem { pub name: String, pub source: ExportSource }
  pub struct Prepared { pub bodies: Vec<ExportBody>, pub report: MeshReport }   impl Prepared { pub fn triangles(&self) -> u64; pub fn vertices(&self) -> u64; }
  pub enum ExportStatus { Computing(f32), Ready { triangles: u64, bytes: u64, report: MeshReport }, Failed(String) }
  pub enum WriteState { Idle, Writing, Done(PathBuf, u64), Failed(String) }
  pub fn prepare(items: &[ExportItem], quality: Quality, progress: &Progress) -> Result<Prepared, String>;
  pub fn format_label(f: Format) -> &'static str;  pub fn group_digits(n: u64) -> String;
  pub fn mb_text(bytes: u64) -> String;  pub fn estimate_text(bytes: u64) -> String;  pub(crate) fn file_name(path: &Path) -> String;
  pub struct ExportDialog { pub format: Format, pub quality: Quality, pub scope: Scope, pub last_rect: Option<egui::Rect>, pub last_cards: Vec<egui::Rect>, … }
  impl ExportDialog { pub fn new(state: &AppState) -> ExportDialog; pub fn items(&self, state: &AppState) -> Vec<ExportItem>;
      pub fn tick(&mut self, state: &AppState, block: bool); pub fn status(&self, state: &AppState) -> ExportStatus;
      pub fn write_state(&self) -> WriteState; pub fn cancel(&mut self); pub fn recompute(&mut self);
      pub fn begin_save(&mut self, dir: PathBuf, state: &AppState); pub fn chooser(&self) -> Option<&FileChooser>;
      pub fn write_to(&mut self, path: PathBuf);
      pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, last_dir: &mut Option<PathBuf>) -> bool; }
  // workspace: DocTab { …, pub export_dialog: Option<ExportDialog> }, Workspace { …, pub exchange_dir: Option<PathBuf> }
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-app/tests/export_dialog.rs`)

```rust
//! Fenster „Exportieren“: Umfang, Qualität, Schätzung, Bericht, Schreiben, Abbrechen, volle
//! Breite und gleich hohe Karten (Teilprojekt 9, Task 16).

use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_app::workspace::Workspace;
use planora_exchange::{ExchangeMesh, Format, Progress, import_file};
use planora_math::DVec3;
use planora_project::config::Config;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-export-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn wide() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    }
}

/// Zeichnet das Fenster dreimal und liefert die Texte des letzten Bildes.
fn draw(d: &mut ExportDialog, s: &mut AppState) -> String {
    let ctx = context();
    let mut dir = None;
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show(ui.ctx(), s, &mut dir));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

fn box_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s
}

#[test]
fn the_scope_follows_the_selection_and_suppressed_features_do_not_count() {
    let mut s = box_state();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap(); // gewählt
    let d = ExportDialog::new(&s);
    assert_eq!(d.scope, Scope::Selected);
    let items = d.items(&s);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].source, ExportSource::Mesh(..)));
    s.selected_body = None;
    let d = ExportDialog::new(&s);
    assert_eq!(d.scope, Scope::Visible);
    assert_eq!(d.items(&s).len(), 2);
    let q = s.document.features()[0].id;
    s.document.set_suppressed(q, true).unwrap();
    s.refresh();
    assert_eq!(ExportDialog::new(&s).items(&s).len(), 1);
}

#[test]
fn preparing_shows_triangles_estimate_and_report() {
    let s = box_state();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    match d.status(&s) {
        ExportStatus::Ready {
            triangles,
            bytes,
            report,
        } => {
            assert_eq!(triangles, 12);
            assert_eq!(bytes, 84 + 50 * 12);
            assert!(report.closed);
        }
        other => panic!("{other:?}"),
    }
    d.format = Format::Obj;
    assert!(
        matches!(d.status(&s), ExportStatus::Ready { .. }),
        "das Format ändert das Netz nicht"
    );
    d.quality = Quality::Fine;
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
    let tolerances: Vec<f64> = Quality::ALL.iter().map(|q| q.chord_tolerance()).collect();
    assert_eq!(tolerances, [0.1, 0.01, 0.001]);
}

#[test]
fn finer_quality_gives_more_triangles_on_curved_bodies() {
    let mut s = AppState::new();
    s.create_sphere(10.0).unwrap();
    let mut d = ExportDialog::new(&s);
    let mut counts = Vec::new();
    for q in Quality::ALL {
        d.quality = q;
        d.tick(&s, true);
        if let ExportStatus::Ready { triangles, .. } = d.status(&s) {
            counts.push(triangles);
        }
    }
    assert_eq!(counts.len(), 3);
    assert!(counts[0] < counts[1] && counts[1] < counts[2], "{counts:?}");
}

#[test]
fn writing_goes_to_the_chosen_file_and_names_errors() {
    let s = box_state();
    let dir = temp_dir("schreiben");
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    d.begin_save(dir.clone(), &s);
    assert_eq!(
        d.chooser().unwrap().target(),
        Some(dir.join("Quader 1.stl"))
    );
    let path = dir.join("Quader 1.stl");
    d.write_to(path.clone());
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Done(path.clone(), 84 + 50 * 12));
    assert_eq!(
        import_file(&path, &Progress::new()).unwrap().triangle_count(),
        12
    );
    d.write_to(dir.join("fehlt").join("x.stl"));
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Failed(_)));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_stops_the_computation_and_says_so() {
    let mut s = AppState::new();
    s.create_sphere(10.0).unwrap();
    let mut d = ExportDialog::new(&s);
    d.quality = Quality::Fine;
    d.tick(&s, false);
    d.cancel();
    assert_eq!(d.status(&s), ExportStatus::Failed("Abgebrochen".into()));
    d.recompute();
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
}

#[test]
fn prepare_reports_the_bodies_one_by_one() {
    let s = box_state();
    let items = ExportDialog::new(&s).items(&s);
    let p = Progress::new();
    let prepared = prepare(&items, Quality::Normal, &p).unwrap();
    assert_eq!((prepared.triangles(), prepared.vertices()), (12, 8));
    assert_eq!(p.fraction(), 1.0);
    let cancelled = Progress::new();
    cancelled.cancel();
    assert_eq!(
        prepare(&items, Quality::Normal, &cancelled).unwrap_err(),
        "Abgebrochen"
    );
}

#[test]
fn the_window_spans_the_full_width_with_equal_cards_and_plain_texts() {
    let mut s = box_state();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    let text = draw(&mut d, &mut s);
    let rect = d.last_rect.unwrap();
    assert!(
        rect.width() >= 1200.0 - 2.0 * DIALOG_MARGIN - 1.0 && rect.width() <= 1200.0,
        "{rect:?}"
    );
    assert_eq!(d.last_cards.len(), 3);
    assert!(
        d.last_cards
            .windows(2)
            .all(|w| (w[0].height() - w[1].height()).abs() < 0.5),
        "{:?}",
        d.last_cards
    );
    for word in [
        "Exportieren",
        "Format",
        "Qualität",
        "Umfang",
        "STL",
        "3MF",
        "glTF",
        "Grob",
        "Normal",
        "Fein",
        "12 Dreiecke",
        "unter 0,1 MB",
        "Das Netz ist geschlossen.",
        "Exportieren …",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    assert_eq!(s.document.features().len(), 1, "Zeichnen ändert nichts");
}

#[test]
fn the_window_warns_about_open_meshes_and_still_exports() {
    let mut s = AppState::new();
    let mut open = cube();
    open.indices.truncate(33);
    s.import_mesh("Offen", Arc::new(open), 1.0).unwrap();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    let text = draw(&mut d, &mut s);
    assert!(text.contains("nicht ganz geschlossen"), "{text}");
    assert!(text.contains(NOT_CLOSED_WARNING), "{text}");
    let dir = temp_dir("offen");
    let path = dir.join("offen.stl");
    d.write_to(path.clone());
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Done(..)));
    assert_eq!(
        import_file(&path, &Progress::new()).unwrap().triangle_count(),
        11
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn texts_for_numbers_and_sizes_are_german() {
    assert_eq!(group_digits(12), "12");
    assert_eq!(group_digits(12_345), "12 345");
    assert_eq!(group_digits(1_234_567), "1 234 567");
    assert_eq!(mb_text(50_000), "unter 0,1 MB");
    assert_eq!(mb_text(1_260_000), "1,3 MB");
    assert_eq!(estimate_text(1_260_000), "ca. 1,3 MB");
    assert_eq!(estimate_text(684), "unter 0,1 MB");
    assert!(format_label(Format::Stl).starts_with("STL"));
    let ws = Workspace::new(None, Config::default());
    assert!(ws.exchange_dir.is_none());
    assert!(ws.active_tab().export_dialog.is_none());
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test export_dialog`
Expected: FAIL – `could not find export_dialog in planora_app`.

- [ ] **Step 3: Fenster implementieren**

In `crates/planora-app/src/lib.rs` nach `pub mod dock;` einfügen: `pub mod export_dialog;`.

`crates/planora-app/src/export_dialog.rs`:

```rust
//! Fenster „Exportieren“ (Spec §8): Format, Qualität und Umfang als gleich hohe Karten über die
//! ganze Breite; das Netz entsteht im Arbeitsthread (Fortschritt, Abbrechen); danach Zahl der
//! Dreiecke, geschätzte Größe und Prüfbericht mit Warnung (Spec E6); gespeichert wird über die
//! eigene Dateiauswahl (Plan P14) und eine Hilfsdatei (Spec §5).

use crate::browser::ERROR_COLOR;
use crate::cards::card_row;
use crate::file_chooser::{ChooserAction, FileChooser, default_dir};
use crate::jobs::{Job, Poll};
use crate::mesh_body::{WARNING_COLOR, report_text};
use crate::state::{AppState, BodyKind};
use planora_exchange::{
    ExchangeError, ExchangeMesh, ExportBody, Format, MeshReport, MeshTransform, PlacedSource,
    Progress, SolidSource, check_mesh, collect_body, export_file,
};
use planora_feature::BodyRef;
use planora_kernel::Solid;
use planora_mesh::{TessellationOptions, tessellate};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Abstand des Fensters zum linken und rechten Rand; sonst reicht es über die ganze Breite.
pub const DIALOG_MARGIN: f32 = 16.0;
/// Abstand des Fensters zum oberen Rand (unter Menü und Tab-Leiste).
pub const DIALOG_TOP: f32 = 60.0;
/// Kleinste Breite bei sehr schmalen Fenstern.
const MIN_WIDTH: f32 = 320.0;
/// Breite des Fortschrittsbalkens.
const PROGRESS_WIDTH: f32 = 280.0;
/// So oft wird neu gezeichnet, solange ein Arbeitsthread läuft.
const REPAINT: Duration = Duration::from_millis(100);
/// Meldung, wenn ein Arbeitsthread ohne Ergebnis endet.
pub const LOST: &str = "Die Berechnung wurde unerwartet beendet.";
/// Warnung bei nicht geschlossenem Netz (Spec E6).
pub const NOT_CLOSED_WARNING: &str = "Die Datei wird trotzdem geschrieben. Manche Programme für den 3D-Druck melden solche Netze als fehlerhaft.";

/// Qualität des Netzes (Spec E5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quality {
    Coarse,
    Normal,
    Fine,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Coarse, Quality::Normal, Quality::Fine];

    /// Sehnentoleranz in Millimetern.
    pub fn chord_tolerance(self) -> f64 {
        match self {
            Quality::Coarse => 0.1,
            Quality::Normal => 0.01,
            Quality::Fine => 0.001,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quality::Coarse => "Grob (bis 0,1 mm Abweichung)",
            Quality::Normal => "Normal (bis 0,01 mm)",
            Quality::Fine => "Fein (bis 0,001 mm)",
        }
    }
}

/// Umfang (Spec E4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Selected,
    Visible,
}

/// Ein Körper für den Arbeitsthread (Kopie, damit das Dokument frei bleibt).
#[derive(Clone, Debug)]
pub enum ExportSource {
    Solid(Solid),
    Mesh(Arc<ExchangeMesh>, MeshTransform),
}

#[derive(Clone, Debug)]
pub struct ExportItem {
    pub name: String,
    pub source: ExportSource,
}

/// Fertig vernetzte Körper mit gemeinsamem Bericht.
#[derive(Debug)]
pub struct Prepared {
    pub bodies: Vec<ExportBody>,
    pub report: MeshReport,
}

impl Prepared {
    pub fn triangles(&self) -> u64 {
        self.bodies
            .iter()
            .map(|b| b.mesh.triangle_count() as u64)
            .sum()
    }

    pub fn vertices(&self) -> u64 {
        self.bodies
            .iter()
            .map(|b| b.mesh.positions.len() as u64)
            .sum()
    }
}

/// Was das Fenster zum Netz zeigt.
#[derive(Debug, PartialEq)]
pub enum ExportStatus {
    Computing(f32),
    Ready {
        triangles: u64,
        bytes: u64,
        report: MeshReport,
    },
    Failed(String),
}

/// Stand des Schreibens der Datei.
#[derive(Debug, PartialEq)]
pub enum WriteState {
    Idle,
    Writing,
    Done(PathBuf, u64),
    Failed(String),
}

/// Vernetzt die Körper (läuft im Arbeitsthread); bricht zwischen den Körpern ab, wenn
/// gewünscht (Plan P15).
pub fn prepare(
    items: &[ExportItem],
    quality: Quality,
    progress: &Progress,
) -> Result<Prepared, String> {
    progress.set_total(items.len() as u64);
    let mut bodies = Vec::with_capacity(items.len());
    for item in items {
        progress.check().map_err(|e| e.to_string())?;
        let body = match &item.source {
            ExportSource::Solid(s) => {
                let options = TessellationOptions {
                    chord_tolerance: quality.chord_tolerance(),
                };
                let mesh = tessellate(s, &options)
                    .map_err(|e| format!("„{}“ lässt sich nicht vernetzen: {e}", item.name))?;
                collect_body(&SolidSource {
                    name: &item.name,
                    mesh: &mesh,
                })
            }
            ExportSource::Mesh(m, t) => collect_body(&PlacedSource {
                name: &item.name,
                mesh: m,
                transform: *t,
            }),
        }
        .map_err(|e| format!("„{}“: {e}", item.name))?;
        bodies.push(body);
        progress.advance(1);
    }
    let reports: Vec<MeshReport> = bodies.iter().map(|b| check_mesh(&b.mesh)).collect();
    Ok(Prepared {
        report: MeshReport::combine(&reports),
        bodies,
    })
}

/// Kurzbeschreibung eines Formats für die Auswahl.
pub fn format_label(f: Format) -> &'static str {
    match f {
        Format::Stl => "STL – für den 3D-Druck",
        Format::Obj => "OBJ – für viele 3D-Programme",
        Format::ThreeMf => "3MF – für den 3D-Druck, mit Namen",
        Format::Glb => "glTF (.glb) – zum Ansehen und Zeigen",
    }
}

/// Zahl mit Leerzeichen als Tausendertrenner („12 345“).
pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Größe in Megabyte mit Komma; unter 0,1 MB „unter 0,1 MB“.
pub fn mb_text(bytes: u64) -> String {
    let mb = bytes as f64 / 1e6;
    if mb < 0.1 {
        "unter 0,1 MB".into()
    } else {
        format!("{mb:.1} MB").replace('.', ",")
    }
}

/// Geschätzte Größe: „ca. 1,2 MB“ (sehr kleine Dateien „unter 0,1 MB“).
pub fn estimate_text(bytes: u64) -> String {
    let t = mb_text(bytes);
    if t.starts_with("unter") {
        t
    } else {
        format!("ca. {t}")
    }
}

/// Dateiname ohne Ordner.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

type Key = (Quality, Scope, u64, Option<BodyRef>);
type PrepJob = Job<Result<Prepared, String>>;
type PrepResult = Result<Arc<Prepared>, String>;
type WriteJob = Job<Result<u64, ExchangeError>>;

pub struct ExportDialog {
    pub format: Format,
    pub quality: Quality,
    pub scope: Scope,
    prepared: Option<(Key, PrepResult)>,
    job: Option<(Key, PrepJob)>,
    chooser: Option<FileChooser>,
    writing: Option<(PathBuf, WriteJob)>,
    written: Option<Result<(PathBuf, u64), String>>,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
}

impl ExportDialog {
    /// Voreinstellung: STL, Normal; Umfang „gewählter Körper“, wenn einer gewählt ist.
    pub fn new(state: &AppState) -> ExportDialog {
        let selected = state
            .selected_body
            .is_some_and(|i| i < state.bodies.len());
        ExportDialog {
            format: Format::Stl,
            quality: Quality::Normal,
            scope: if selected {
                Scope::Selected
            } else {
                Scope::Visible
            },
            prepared: None,
            job: None,
            chooser: None,
            writing: None,
            written: None,
            last_rect: None,
            last_cards: Vec::new(),
        }
    }

    /// Indizes der Körper im Umfang (Spec E4: der gewählte, sonst alle sichtbaren).
    fn chosen(&self, state: &AppState) -> Vec<usize> {
        match (self.scope, state.selected_body) {
            (Scope::Selected, Some(i)) if i < state.bodies.len() => vec![i],
            _ => (0..state.bodies.len()).collect(),
        }
    }

    /// Kopien der Körper im Umfang für den Arbeitsthread.
    pub fn items(&self, state: &AppState) -> Vec<ExportItem> {
        self.chosen(state)
            .into_iter()
            .filter_map(|i| {
                let b = &state.bodies[i];
                let source = if b.kind == BodyKind::Mesh {
                    let (mesh, t) = state.document.mesh_body(b.source.feature)?;
                    ExportSource::Mesh(Arc::clone(mesh), t)
                } else {
                    ExportSource::Solid(b.solid.clone())
                };
                Some(ExportItem {
                    name: b.name.clone(),
                    source,
                })
            })
            .collect()
    }

    fn key(&self, state: &AppState) -> Key {
        let selected = match self.scope {
            Scope::Selected => state
                .selected_body
                .and_then(|i| state.bodies.get(i))
                .map(|b| b.source),
            Scope::Visible => None,
        };
        (
            self.quality,
            self.scope,
            state.document.edit_count(),
            selected,
        )
    }

    /// Startet das Vernetzen, wenn sich Qualität, Umfang oder Dokument geändert haben, und holt
    /// fertige Ergebnisse ab. Mit `block` wird gewartet (Tests).
    pub fn tick(&mut self, state: &AppState, block: bool) {
        let key = self.key(state);
        let fresh = self.prepared.as_ref().is_some_and(|(k, _)| *k == key);
        let running = self.job.as_ref().is_some_and(|(k, _)| *k == key);
        if !fresh && !running {
            let items = self.items(state);
            let quality = self.quality;
            // Ein alter Auftrag wird dabei fallengelassen und bricht ab.
            self.job = Some((key, Job::spawn(move |p| prepare(&items, quality, p))));
        }
        let finished = match &self.job {
            Some((k, job)) => match if block { job.wait() } else { job.poll() } {
                Poll::Pending => None,
                Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                Poll::Lost => Some((*k, Err(LOST.to_string()))),
            },
            None => None,
        };
        if let Some(done) = finished {
            self.prepared = Some(done);
            self.job = None;
        }
        let wrote = match &self.writing {
            Some((path, job)) => match if block { job.wait() } else { job.poll() } {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n)).map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.written = Some(w);
            self.writing = None;
        }
    }

    pub fn status(&self, state: &AppState) -> ExportStatus {
        let key = self.key(state);
        if let Some((k, job)) = &self.job
            && *k == key
        {
            return ExportStatus::Computing(job.fraction());
        }
        match &self.prepared {
            Some((k, Ok(p))) if *k == key => ExportStatus::Ready {
                triangles: p.triangles(),
                bytes: self.format.estimate_bytes(p.triangles(), p.vertices()),
                report: p.report,
            },
            Some((k, Err(m))) if *k == key => ExportStatus::Failed(m.clone()),
            _ => ExportStatus::Computing(0.0),
        }
    }

    pub fn write_state(&self) -> WriteState {
        if self.writing.is_some() {
            return WriteState::Writing;
        }
        match &self.written {
            None => WriteState::Idle,
            Some(Ok((p, n))) => WriteState::Done(p.clone(), *n),
            Some(Err(m)) => WriteState::Failed(m.clone()),
        }
    }

    /// Bricht das Vernetzen ab; das Fenster zeigt „Abgebrochen“, bis neu berechnet wird.
    pub fn cancel(&mut self) {
        if let Some((key, _)) = self.job.take() {
            self.prepared = Some((key, Err(ExchangeError::Cancelled.to_string())));
        }
    }

    /// Verwirft das Ergebnis; das nächste `tick` rechnet neu.
    pub fn recompute(&mut self) {
        self.prepared = None;
    }

    /// Öffnet die Dateiauswahl im Ordner `dir` mit dem Namen des Körpers (bei mehreren
    /// „Körper“); Zeichen, die in Dateinamen nicht erlaubt sind, werden „_“.
    pub fn begin_save(&mut self, dir: PathBuf, state: &AppState) {
        let names: Vec<&str> = self
            .chosen(state)
            .into_iter()
            .filter_map(|i| state.bodies.get(i).map(|b| b.name.as_str()))
            .collect();
        let stem = match names.as_slice() {
            [one] => (*one).to_string(),
            _ => "Körper".to_string(),
        };
        let stem: String = stem
            .chars()
            .map(|c| {
                if c.is_control() || "\\/:*?\"<>|".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        self.chooser = Some(FileChooser::save(dir, stem, self.format.extension()));
    }

    pub fn chooser(&self) -> Option<&FileChooser> {
        self.chooser.as_ref()
    }

    /// Schreibt das fertige Netz im Arbeitsthread nach `path` (über eine Hilfsdatei).
    pub fn write_to(&mut self, path: PathBuf) {
        let Some((_, Ok(prepared))) = &self.prepared else {
            return;
        };
        let (prepared, format, target) = (Arc::clone(prepared), self.format, path.clone());
        self.written = None;
        self.writing = Some((
            path,
            Job::spawn(move |_| export_file(&target, format, &prepared.bodies)),
        ));
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll. `last_dir` ist der zuletzt
    /// benutzte Ordner der Dateiauswahl.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
    ) -> bool {
        self.tick(state, false);
        let width = (ctx.content_rect().width() - 2.0 * DIALOG_MARGIN).max(MIN_WIDTH);
        let status = self.status(state);
        let write_state = self.write_state();
        let has_selection = state
            .selected_body
            .is_some_and(|i| i < state.bodies.len());
        let before = self.format;
        let (mut format, mut quality, mut scope) = (self.format, self.quality, self.scope);
        let (mut open, mut close, mut cancel, mut recompute, mut begin, mut drop_chooser) =
            (true, false, false, false, false, false);
        let mut chosen: Option<PathBuf> = None;
        let mut cards = Vec::new();
        let chooser = &mut self.chooser;
        let response = egui::Window::new("Exportieren")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, DIALOG_TOP])
            .min_width(width)
            .max_width(width)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut format_card = |ui: &mut egui::Ui| {
                    ui.strong("Format");
                    for f in Format::ALL {
                        ui.radio_value(&mut format, f, format_label(f));
                    }
                };
                let mut quality_card = |ui: &mut egui::Ui| {
                    ui.strong("Qualität");
                    for q in Quality::ALL {
                        ui.radio_value(&mut quality, q, q.label());
                    }
                    ui.weak("Feiner heißt mehr Dreiecke und größere Dateien.");
                };
                let mut scope_card = |ui: &mut egui::Ui| {
                    ui.strong("Umfang");
                    let selected = ui
                        .add_enabled(
                            has_selection,
                            egui::RadioButton::new(scope == Scope::Selected, "Gewählter Körper"),
                        )
                        .on_disabled_hover_text("Dafür zuerst einen Körper wählen.");
                    if selected.clicked() {
                        scope = Scope::Selected;
                    }
                    ui.radio_value(&mut scope, Scope::Visible, "Alle sichtbaren Körper");
                    ui.weak("Unterdrückte Schritte zählen nicht.");
                };
                let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                    &mut [&mut format_card, &mut quality_card, &mut scope_card];
                cards = card_row(ui, "export", row);
                ui.add_space(8.0);
                match &status {
                    ExportStatus::Computing(f) => {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(*f)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text("Das Netz wird berechnet …"),
                            );
                            if ui.button("Abbrechen").clicked() {
                                cancel = true;
                            }
                        });
                    }
                    ExportStatus::Failed(m) => {
                        ui.colored_label(ERROR_COLOR, m);
                        if ui.button("Neu berechnen").clicked() {
                            recompute = true;
                        }
                    }
                    ExportStatus::Ready {
                        triangles,
                        bytes,
                        report,
                    } => {
                        ui.label(format!(
                            "{} Dreiecke, {}",
                            group_digits(*triangles),
                            estimate_text(*bytes)
                        ));
                        if report.closed {
                            ui.label(report_text(report));
                        } else {
                            ui.colored_label(WARNING_COLOR, report_text(report));
                            ui.colored_label(WARNING_COLOR, NOT_CLOSED_WARNING);
                        }
                    }
                }
                match &write_state {
                    WriteState::Idle => {}
                    WriteState::Writing => {
                        ui.label("Die Datei wird geschrieben …");
                    }
                    WriteState::Done(path, n) => {
                        ui.label(format!(
                            "Fertig: „{}“ geschrieben ({}).",
                            file_name(path),
                            mb_text(*n)
                        ));
                    }
                    WriteState::Failed(m) => {
                        ui.colored_label(ERROR_COLOR, format!("Nicht geschrieben: {m}"));
                    }
                }
                ui.separator();
                match chooser.as_mut() {
                    Some(c) => {
                        ui.strong("Speichern unter");
                        match c.show(ui) {
                            ChooserAction::Chosen(p) => chosen = Some(p),
                            ChooserAction::Cancel => drop_chooser = true,
                            ChooserAction::None => {}
                        }
                    }
                    None => {
                        let ready = matches!(status, ExportStatus::Ready { .. })
                            && write_state != WriteState::Writing;
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(ready, egui::Button::new("Exportieren …"))
                                .on_disabled_hover_text(
                                    "Erst möglich, wenn das Netz fertig berechnet ist.",
                                )
                                .clicked()
                            {
                                begin = true;
                            }
                            if ui.button("Schließen").clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        (self.format, self.quality, self.scope) = (format, quality, scope);
        if self.format != before || drop_chooser {
            self.chooser = None;
        }
        if matches!(status, ExportStatus::Computing(_)) || write_state == WriteState::Writing {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel {
            self.cancel();
        }
        if recompute {
            self.recompute();
        }
        if begin {
            let dir = last_dir.clone().unwrap_or_else(default_dir);
            self.begin_save(dir, state);
        }
        if let Some(path) = chosen {
            *last_dir = path.parent().map(Path::to_path_buf);
            self.chooser = None;
            self.write_to(path);
        }
        open && !close
    }
}
```

- [ ] **Step 4: Arbeitsbereich und App anbinden**

`crates/planora-app/src/workspace.rs`:
- Import ergänzen: `use crate::export_dialog::ExportDialog;`
- In `pub struct DocTab` nach `pub blend_dialog: Option<BlendDialog>,` einfügen: `pub export_dialog: Option<ExportDialog>,`
- In `DocTab::new` nach `blend_dialog: None,` einfügen: `export_dialog: None,`
- In `pub struct Workspace` nach `pub autosave_interval: Duration,` einfügen:

```rust
    /// Zuletzt benutzter Ordner der Dateiauswahl beim Exportieren und Importieren (nicht
    /// gespeichert).
    pub exchange_dir: Option<PathBuf>,
```

- In `Workspace::new` nach `autosave_interval: AUTOSAVE_INTERVAL,` einfügen: `exchange_dir: None,`

`crates/planora-app/src/app.rs`:
- Importe ergänzen: `use crate::export_dialog::ExportDialog;`
- In `fn file_menu` nach dem Block „Speichern unter …“ (vor `ui.separator();` und „Schließen“) einfügen:

```rust
        ui.separator();
        let any = !self.ws.active_tab().state.bodies.is_empty();
        if ui
            .add_enabled(any, egui::Button::new("Exportieren …   Strg+Umschalt+E"))
            .on_disabled_hover_text("Es gibt noch keinen Körper.")
            .clicked()
        {
            self.open_export();
            ui.close();
        }
```

- Nach `fn begin_save` die Methode einfügen:

```rust
    /// Datei → Exportieren … (Strg+Umschalt+E): öffnet das Fenster, wenn es Körper gibt; eine
    /// offene Skizze wird vorher beendet. Auch in schreibgeschützten Ansichten möglich.
    fn open_export(&mut self) {
        let tab = self.ws.active_tab_mut();
        if tab.state.bodies.is_empty() || tab.state.try_close_sketch().is_err() {
            return;
        }
        tab.export_dialog = Some(ExportDialog::new(&tab.state));
    }
```

- In `fn shortcuts` den Kommentar auf „Tastenkürzel Strg+N/O/S/W und Strg+Umschalt+E.“ ändern und nach dem Block `if pressed(egui::Key::W) { … }` einfügen:

```rust
        let export = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::E,
            ))
        });
        if export {
            self.open_export();
        }
```

- In `fn ui` die Zeilen

```rust
        // Das Projekt-Panel kann den aktiven Tab gewechselt haben (Dokument geöffnet).
        let tab = self.ws.active_tab_mut();
```

ersetzen durch

```rust
        // Das Projekt-Panel kann den aktiven Tab gewechselt haben (Dokument geöffnet). Der
        // Ordner der Dateiauswahl wird für die Fenster kurz herausgenommen.
        let mut exchange_dir = self.ws.exchange_dir.take();
        let tab = self.ws.active_tab_mut();
```

- Im Block `if tab.state.sketch_session.is_some() { … }` nach `tab.blend_dialog = None;` einfügen: `tab.export_dialog = None;`
- Nach dem Block `if let Some(mut dialog) = tab.blend_dialog.take() && … { … }` einfügen:

```rust
        if let Some(mut dialog) = tab.export_dialog.take()
            && dialog.show(&ctx, &mut tab.state, &mut exchange_dir)
        {
            tab.export_dialog = Some(dialog);
        }
```

- Unmittelbar vor `self.ws.refresh_prompts();` einfügen: `self.ws.exchange_dir = exchange_dir;`

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo build -p planora-app`
Expected: PASS (9 Tests in `export_dialog`), die App baut.

```bash
git add crates/planora-app/src/lib.rs crates/planora-app/src/export_dialog.rs crates/planora-app/src/workspace.rs crates/planora-app/src/app.rs crates/planora-app/tests/export_dialog.rs
git commit -m "app: Fenster Exportieren mit Arbeitsthread, Schätzung, Bericht und Dateiauswahl

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 17: Fenster „Netz importieren …“

**Files:**
- Create: `crates/planora-app/src/import_dialog.rs`
- Modify: `crates/planora-app/src/lib.rs`, `crates/planora-app/src/workspace.rs` (`DocTab.import_dialog`), `crates/planora-app/src/app.rs` (Menü, Anzeige)
- Test: `crates/planora-app/tests/import_dialog.rs`

**Interfaces:**
- Consumes: Task 9 (`import_file`), Task 2 (`check_mesh`), Task 12 (`import_mesh`, `report_text`), Task 15, Task 16 (`DIALOG_MARGIN`, `DIALOG_TOP`, `LOST`, `group_digits`, `file_name`).
- Produces:
  ```rust
  pub const OPEN_MESH_HINT: &str;
  pub enum Unit { Millimeter, Zentimeter, Meter, Zoll }   impl Unit { pub const ALL: [Unit; 4]; pub fn factor(self) -> f64; pub fn label(self) -> &'static str; }
  pub struct Loaded { pub path: PathBuf, pub mesh: Arc<ExchangeMesh>, pub report: MeshReport }
  pub struct ImportDialog { pub loaded: Option<Loaded>, pub name: String, pub unit: Unit, pub error: Option<String>, pub last_rect: Option<egui::Rect>, pub last_cards: Vec<egui::Rect>, … }
  impl ImportDialog { pub fn new(dir: PathBuf) -> ImportDialog; pub fn load(&mut self, path: PathBuf); pub fn is_loading(&self) -> bool;
      pub fn progress(&self) -> f32; pub fn cancel(&mut self); pub fn tick(&mut self, block: bool);
      pub fn import(&self, state: &mut AppState) -> Result<FeatureId, String>;
      pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, last_dir: &mut Option<PathBuf>) -> bool; }
  // workspace: DocTab { …, pub import_dialog: Option<ImportDialog> }
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-app/tests/import_dialog.rs`)

```rust
//! Fenster „Netz importieren“: lesen im Arbeitsthread, Bericht, Name, Einheit, Abbrechen,
//! Fehler, volle Breite (Teilprojekt 9, Task 17).

use planora_app::export_dialog::{DIALOG_MARGIN, ExportDialog, ExportStatus};
use planora_app::fonts;
use planora_app::import_dialog::*;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_exchange::{ExchangeMesh, ExportBody, Format, export_file};
use std::fs;
use std::path::{Path, PathBuf};

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0,
            7, 3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-import-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Schreibt `mesh` als STL nach `dir/name`.
fn write_stl(dir: &Path, name: &str, mesh: ExchangeMesh) -> PathBuf {
    let path = dir.join(name);
    export_file(
        &path,
        Format::Stl,
        &[ExportBody {
            name: "x".into(),
            mesh,
        }],
    )
    .unwrap();
    path
}

fn wide() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    }
}

fn draw(d: &mut ImportDialog, s: &mut AppState) -> String {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut dir = None;
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show(ui.ctx(), s, &mut dir));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

#[test]
fn a_file_is_read_checked_and_imported_with_its_unit() {
    let dir = temp_dir("einheit");
    let path = write_stl(&dir, "Würfel.stl", cube());
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    d.tick(true);
    let l = d.loaded.as_ref().unwrap();
    assert_eq!(l.report.triangles, 12);
    assert!(l.report.closed);
    assert_eq!(d.name, "Würfel");
    assert_eq!(d.unit, Unit::Millimeter, "Vorgabe Millimeter");
    d.unit = Unit::Zoll;
    d.import(&mut s).unwrap();
    assert!((s.bodies[0].mass.volume - 25.4f64.powi(3)).abs() < 1e-6);
    let factors: Vec<f64> = Unit::ALL.iter().map(|u| u.factor()).collect();
    assert_eq!(factors, [1.0, 10.0, 1000.0, 25.4]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn broken_files_show_a_named_error_and_change_nothing() {
    let dir = temp_dir("kaputt");
    let p = dir.join("kaputt.obj");
    fs::write(&p, "v 0 0 0\nf 1 2 3\n").unwrap();
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(p);
    d.tick(true);
    assert!(d.loaded.is_none());
    assert_eq!(
        d.error.as_deref(),
        Some("Zeile 2: Die Fläche verweist auf einen Punkt, den es nicht gibt")
    );
    assert!(d.import(&mut s).is_err());
    assert!(s.document.features().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_drops_the_reading() {
    let dir = temp_dir("abbrechen");
    let path = write_stl(&dir, "w.stl", cube());
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    assert!(d.is_loading());
    d.cancel();
    assert!(!d.is_loading());
    assert!(d.loaded.is_none());
    assert_eq!(d.error.as_deref(), Some("Abgebrochen"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_open_mesh_is_imported_with_a_warning() {
    let dir = temp_dir("offen");
    let mut open = cube();
    open.indices.truncate(33);
    let path = write_stl(&dir, "offen.stl", open);
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    d.tick(true);
    assert!(!d.loaded.as_ref().unwrap().report.closed);
    let text = draw(&mut d, &mut s);
    assert!(text.contains(OPEN_MESH_HINT), "{text}");
    d.import(&mut s).unwrap();
    assert_eq!(s.bodies[0].kind, BodyKind::Mesh);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_window_spans_the_width_and_shows_equal_cards() {
    let dir = temp_dir("fenster");
    let path = write_stl(&dir, "Teil.stl", cube());
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    let text = draw(&mut d, &mut s);
    assert!(text.contains("STL oder OBJ") && text.contains("Teil.stl"), "{text}");
    d.load(path);
    d.tick(true);
    let text = draw(&mut d, &mut s);
    let rect = d.last_rect.unwrap();
    assert!(
        rect.width() >= 1200.0 - 2.0 * DIALOG_MARGIN - 1.0 && rect.width() <= 1200.0,
        "{rect:?}"
    );
    assert_eq!(d.last_cards.len(), 2);
    assert!((d.last_cards[0].height() - d.last_cards[1].height()).abs() < 0.5);
    for word in [
        "Netz importieren",
        "Prüfung",
        "12 Dreiecke",
        "Das Netz ist geschlossen.",
        "Name",
        "Einheit der Datei",
        "Millimeter",
        "Zentimeter",
        "Meter",
        "Zoll",
        "Importieren",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    assert!(s.document.features().is_empty(), "Zeichnen ändert nichts");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_imported_mesh_body_can_be_exported_again() {
    let dir = temp_dir("wieder");
    let path = write_stl(&dir, "w.stl", cube());
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    d.tick(true);
    d.import(&mut s).unwrap();
    let mut e = ExportDialog::new(&s);
    e.tick(&s, true);
    assert!(matches!(
        e.status(&s),
        ExportStatus::Ready { triangles: 12, .. }
    ));
    let _ = fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test import_dialog`
Expected: FAIL – `could not find import_dialog in planora_app`.

- [ ] **Step 3: Fenster implementieren**

In `crates/planora-app/src/lib.rs` nach `pub mod icons;` einfügen: `pub mod import_dialog;`.

`crates/planora-app/src/import_dialog.rs`:

```rust
//! Fenster „Netz importieren“ (Spec §8): Datei wählen (STL, OBJ), im Arbeitsthread lesen
//! (Fortschritt, Abbrechen), Prüfbericht, Name und Einheit der Datei, „Importieren“. Das
//! Dokument ändert sich erst beim Importieren und nur bei Erfolg (Spec §5).

use crate::browser::ERROR_COLOR;
use crate::cards::card_row;
use crate::export_dialog::{DIALOG_MARGIN, DIALOG_TOP, LOST, file_name, group_digits};
use crate::file_chooser::{ChooserAction, FileChooser};
use crate::jobs::{Job, Poll};
use crate::mesh_body::{WARNING_COLOR, report_text};
use crate::state::AppState;
use planora_exchange::{ExchangeError, ExchangeMesh, MeshReport, check_mesh, import_file};
use planora_feature::FeatureId;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Kleinste Breite bei sehr schmalen Fenstern.
const MIN_WIDTH: f32 = 320.0;
/// Breite des Fortschrittsbalkens.
const PROGRESS_WIDTH: f32 = 280.0;
/// So oft wird neu gezeichnet, solange gelesen wird.
const REPAINT: Duration = Duration::from_millis(100);
/// Hinweis bei nicht geschlossenem Netz.
pub const OPEN_MESH_HINT: &str = "Der Netz-Körper lässt sich trotzdem anlegen und anzeigen; Volumen und Oberfläche stimmen dann nur ungefähr.";

/// Einheit der Quelldatei (Spec §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Millimeter,
    Zentimeter,
    Meter,
    Zoll,
}

impl Unit {
    pub const ALL: [Unit; 4] = [Unit::Millimeter, Unit::Zentimeter, Unit::Meter, Unit::Zoll];

    /// Millimeter je Einheit der Datei (Maßstab des Netz-Körpers).
    pub fn factor(self) -> f64 {
        match self {
            Unit::Millimeter => 1.0,
            Unit::Zentimeter => 10.0,
            Unit::Meter => 1000.0,
            Unit::Zoll => 25.4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Unit::Millimeter => "Millimeter",
            Unit::Zentimeter => "Zentimeter",
            Unit::Meter => "Meter",
            Unit::Zoll => "Zoll",
        }
    }
}

/// Gelesene und geprüfte Datei.
pub struct Loaded {
    pub path: PathBuf,
    pub mesh: Arc<ExchangeMesh>,
    pub report: MeshReport,
}

type ReadJob = Job<Result<(ExchangeMesh, MeshReport), ExchangeError>>;

pub struct ImportDialog {
    chooser: FileChooser,
    job: Option<(PathBuf, ReadJob)>,
    pub loaded: Option<Loaded>,
    pub name: String,
    pub unit: Unit,
    pub error: Option<String>,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
}

/// Dateiname ohne Endung als Vorschlag für den Namen.
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .unwrap_or_default()
}

impl ImportDialog {
    pub fn new(dir: PathBuf) -> ImportDialog {
        ImportDialog {
            chooser: FileChooser::open(dir, vec!["stl", "obj"]),
            job: None,
            loaded: None,
            name: String::new(),
            unit: Unit::Millimeter,
            error: None,
            last_rect: None,
            last_cards: Vec::new(),
        }
    }

    /// Liest `path` im Arbeitsthread und prüft das Netz.
    pub fn load(&mut self, path: PathBuf) {
        self.error = None;
        self.loaded = None;
        let source = path.clone();
        self.job = Some((
            path,
            Job::spawn(move |progress| {
                let mesh = import_file(&source, progress)?;
                let report = check_mesh(&mesh);
                Ok((mesh, report))
            }),
        ));
    }

    pub fn is_loading(&self) -> bool {
        self.job.is_some()
    }

    pub fn progress(&self) -> f32 {
        self.job.as_ref().map_or(0.0, |(_, j)| j.fraction())
    }

    /// Bricht das Lesen ab (der Arbeitsthread endet beim nächsten Prüfen).
    pub fn cancel(&mut self) {
        if self.job.take().is_some() {
            self.error = Some(ExchangeError::Cancelled.to_string());
        }
    }

    /// Holt das Ergebnis des Lesens ab; mit `block` wird gewartet (Tests).
    pub fn tick(&mut self, block: bool) {
        let done = match &self.job {
            Some((path, job)) => match if block { job.wait() } else { job.poll() } {
                Poll::Pending => None,
                Poll::Ready(r) => Some((path.clone(), r.map_err(|e| e.to_string()))),
                Poll::Lost => Some((path.clone(), Err(LOST.to_string()))),
            },
            None => None,
        };
        if let Some((path, result)) = done {
            self.job = None;
            match result {
                Ok((mesh, report)) => {
                    self.name = file_stem(&path);
                    self.loaded = Some(Loaded {
                        path,
                        mesh: Arc::new(mesh),
                        report,
                    });
                }
                Err(e) => self.error = Some(e),
            }
        }
    }

    /// Legt den Netz-Körper an (Name, Einheit); das Dokument ändert sich nur bei Erfolg.
    pub fn import(&self, state: &mut AppState) -> Result<FeatureId, String> {
        let Some(l) = &self.loaded else {
            return Err("Bitte zuerst eine Datei wählen.".into());
        };
        state.import_mesh(&self.name, Arc::clone(&l.mesh), self.unit.factor())
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
    ) -> bool {
        self.tick(false);
        let width = (ctx.content_rect().width() - 2.0 * DIALOG_MARGIN).max(MIN_WIDTH);
        let (mut open, mut close, mut cancel, mut import) = (true, false, false, false);
        let mut chosen = None;
        let mut cards = Vec::new();
        let (mut name, mut unit) = (self.name.clone(), self.unit);
        let loading = self.is_loading().then(|| self.progress());
        let response = egui::Window::new("Netz importieren")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, DIALOG_TOP])
            .min_width(width)
            .max_width(width)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.strong("Datei wählen (STL oder OBJ)");
                match self.chooser.show(ui) {
                    ChooserAction::Chosen(p) => chosen = Some(p),
                    ChooserAction::Cancel => close = true,
                    ChooserAction::None => {}
                }
                if let Some(f) = loading {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::ProgressBar::new(f)
                                .desired_width(PROGRESS_WIDTH)
                                .text("Die Datei wird gelesen …"),
                        );
                        if ui.button("Abbrechen").clicked() {
                            cancel = true;
                        }
                    });
                }
                if let Some(e) = &self.error {
                    ui.colored_label(ERROR_COLOR, e);
                }
                if let Some(l) = &self.loaded {
                    ui.separator();
                    let mut report_card = |ui: &mut egui::Ui| {
                        ui.strong("Prüfung");
                        ui.label(format!("Datei: {}", file_name(&l.path)));
                        ui.label(format!(
                            "{} Dreiecke, {} Punkte",
                            group_digits(l.report.triangles as u64),
                            group_digits(l.report.vertices as u64)
                        ));
                        if l.report.closed {
                            ui.label(report_text(&l.report));
                        } else {
                            ui.colored_label(WARNING_COLOR, report_text(&l.report));
                            ui.colored_label(WARNING_COLOR, OPEN_MESH_HINT);
                        }
                    };
                    let mut name_card = |ui: &mut egui::Ui| {
                        ui.strong("Name");
                        ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
                        ui.add_space(4.0);
                        ui.strong("Einheit der Datei");
                        ui.horizontal_wrapped(|ui| {
                            for u in Unit::ALL {
                                ui.radio_value(&mut unit, u, u.label());
                            }
                        });
                    };
                    let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                        &mut [&mut report_card, &mut name_card];
                    cards = card_row(ui, "import", row);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Importieren").clicked() {
                            import = true;
                        }
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                } else if ui.button("Schließen").clicked() {
                    close = true;
                }
            });
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        (self.name, self.unit) = (name, unit);
        if loading.is_some() {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel {
            self.cancel();
        }
        if let Some(p) = chosen {
            *last_dir = p.parent().map(Path::to_path_buf);
            self.load(p);
        }
        if import {
            match self.import(state) {
                Ok(_) => return false,
                Err(e) => self.error = Some(e),
            }
        }
        open && !close
    }
}
```

- [ ] **Step 4: Arbeitsbereich und App anbinden**

`crates/planora-app/src/workspace.rs`:
- Import ergänzen: `use crate::import_dialog::ImportDialog;`
- In `pub struct DocTab` nach `pub export_dialog: Option<ExportDialog>,` einfügen: `pub import_dialog: Option<ImportDialog>,`
- In `DocTab::new` nach `export_dialog: None,` einfügen: `import_dialog: None,`

`crates/planora-app/src/app.rs`:
- Importe ergänzen: `use crate::file_chooser::default_dir;` und `use crate::import_dialog::ImportDialog;`
- In `fn file_menu` nach dem Eintrag „Exportieren …“ einfügen:

```rust
        if ui
            .add_enabled(!read_only, egui::Button::new("Netz importieren …"))
            .on_disabled_hover_text("In einer alten Version lässt sich nichts hinzufügen.")
            .clicked()
        {
            self.open_import();
            ui.close();
        }
```

- Nach `fn open_export` einfügen:

```rust
    /// Datei → Netz importieren …: öffnet das Fenster im zuletzt benutzten Ordner; eine offene
    /// Skizze wird vorher beendet. Nicht in schreibgeschützten Ansichten.
    fn open_import(&mut self) {
        let dir = self.ws.exchange_dir.clone().unwrap_or_else(default_dir);
        let tab = self.ws.active_tab_mut();
        if tab.read_only || tab.state.try_close_sketch().is_err() {
            return;
        }
        tab.import_dialog = Some(ImportDialog::new(dir));
    }
```

- Im Block `if tab.state.sketch_session.is_some() { … }` nach `tab.export_dialog = None;` einfügen: `tab.import_dialog = None;`
- Nach dem Block für `tab.export_dialog` einfügen:

```rust
        if let Some(mut dialog) = tab.import_dialog.take()
            && dialog.show(&ctx, &mut tab.state, &mut exchange_dir)
        {
            tab.import_dialog = Some(dialog);
        }
```

- [ ] **Step 5: Tests, Prüfungen und Commit**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo build -p planora-app`
Expected: PASS (6 Tests in `import_dialog`), die App baut.

```bash
git add crates/planora-app/src/lib.rs crates/planora-app/src/import_dialog.rs crates/planora-app/src/workspace.rs crates/planora-app/src/app.rs crates/planora-app/tests/import_dialog.rs
git commit -m "app: Fenster Netz importieren mit Arbeitsthread, Prüfbericht, Name und Einheit

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 18: Dokumentation und Gesamtprüfung

**Files:**
- Modify: `README.md` (neuer Abschnitt „Netz-Austausch“ nach „Rundungen und Fasen“; Statuszeile oben um „Netz-Austausch“ ergänzen), `docs/superpowers/offene-punkte.md` (Kopf „nach Teilprojekt 1–9“, neuer Abschnitt „Teilprojekt 9 – Netz-Austausch“ vor „Aufräumen / Qualität“), `docs/superpowers/specs/2026-10-14-netz-austausch-design.md` (§12 ausfüllen)

**Interfaces:**
- Consumes: die Berichte der Tasks 1–17 (Testzahlen, Laufzeiten, gemessene Dateigrößen, Befunde der Stufenreviews).
- Produces: Dokumentation in einfacher Sprache (Laiensprache, wenig Technik, kein Füllstoff), Spec §12 mit P1–P17 und allen Abweichungen.

- [ ] **Step 1: README** – nach dem Abschnitt „Rundungen und Fasen“ einfügen (Inhalt nur kürzen, wo Befunde es verlangen):

```markdown
## Netz-Austausch (Export und Import)

Körper lassen sich als Dreiecksnetz speichern, etwa für den 3D-Druck oder um sie in einem anderen Programm anzusehen: Menü „Datei“ → „Exportieren …“ (Strg+Umschalt+E).

- **Formate:** STL (für den 3D-Druck), OBJ (für viele 3D-Programme), 3MF (für den 3D-Druck, mit dem Namen jedes Körpers) und glTF als .glb (zum Ansehen und Zeigen).
- **Qualität:** Grob, Normal oder Fein. Gewölbte Flächen werden aus kleinen Dreiecken zusammengesetzt. Bei „Fein“ weicht das Netz höchstens ein tausendstel Millimeter von der echten Fläche ab; die Datei wird dafür größer. Das Fenster zeigt vorher, wie viele Dreiecke es werden und wie groß die Datei ungefähr wird.
- **Umfang:** Ist ein Körper gewählt, nur dieser; sonst alle sichtbaren Körper. Unterdrückte Schritte zählen nicht.
- **Prüfung:** Ist das Netz nicht ganz geschlossen (das kann bei Körpern mit Freiformflächen vorkommen), warnt das Fenster. Die Datei wird trotzdem geschrieben.
- Das Programm schreibt zuerst in eine Hilfsdatei. Eine vorhandene Datei wird erst ersetzt, wenn alles geklappt hat.

Netze aus anderen Programmen lassen sich einlesen: „Datei“ → „Netz importieren …“ (STL und OBJ). Das Fenster zeigt vorher, wie viele Dreiecke die Datei hat und ob das Netz geschlossen ist. Die Einheit der Datei lässt sich wählen (Millimeter, Zentimeter, Meter, Zoll). Das Ergebnis ist ein **Netz-Körper**: Er lässt sich anzeigen, verschieben, drehen, vergrößern oder verkleinern, messen und wieder exportieren. Mit anderen Körpern verrechnen, runden oder als Fläche für eine Skizze nutzen lässt er sich nicht; diese Menüpunkte sind dann ausgegraut.

Dokumente mit Netz-Körpern speichert das Programm im Dateiformat 3; das Netz steckt im Dokument selbst. Ältere Programmversionen melden dann „zu neu“. Dokumente ohne Netz-Körper bleiben im Format 1 oder 2.

**Grenzen**
- Höchstens 50 Millionen Dreiecke und 8 GB je Datei. Ab 5 Millionen Dreiecken zeigt die Ansicht nur einen Teil der Dreiecke; gespeichert und exportiert wird alles.
- Dokumente mit großen Netzen werden groß (etwa ein Drittel größer als die eingelesene Datei) und laden langsamer.
- Farben, Materialien und Texturen werden nicht übernommen. 3MF-Dateien über 4 GB kann das Programm nicht schreiben (dann STL wählen).
- In OBJ-Dateien führen Vielecke, die sich nicht sauber in Dreiecke zerlegen lassen, und Flächen, die auf später stehende Punkte verweisen, zu einer Meldung mit Zeilennummer.
- Zum Wählen der Datei hat das Programm ein eigenes Fenster mit Ordnerliste und Dateiname; der Dateidialog des Betriebssystems wird nicht benutzt.
- Während das Netz berechnet oder eine Datei gelesen wird, bleibt das Fenster bedienbar. „Abbrechen“ wirkt beim Export zwischen zwei Körpern.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 9 – Netz-Austausch“.
```

In der Statuszeile oben (`**Status:** frühe Entwicklung (…)`) nach „Freiformflächen“ ergänzen: „, Rundungen und Fasen, Netz-Austausch“ (falls „Rundungen und Fasen“ dort schon steht, nur „, Netz-Austausch“).

- [ ] **Step 2: Offene Punkte** – Kopfzeile `# Offene Punkte nach Teilprojekt 1–8` in `# Offene Punkte nach Teilprojekt 1–9` ändern und vor `## Aufräumen / Qualität` einen Abschnitt `## Teilprojekt 9 – Netz-Austausch` mit Stand-Datum anlegen. Er enthält mindestens diese Punkte, jeweils mit den gemessenen Zahlen aus den Berichten der Tasks (Testanzahl aus der Ausgabe von `cargo test --workspace` in Step 4, Laufzeit der `--ignored`-Reihe aus Task 5, gemessene Dateigrößen aus Task 9):
  - Dateiauswahl: eigenes Fenster statt Dialog des Betriebssystems (P14), weil dafür eine neue Abhängigkeit nötig wäre; Laufwerkswechsel unter Windows nur durch Eintippen des Pfads.
  - Abbrechen: Vernetzen eines einzelnen Körpers lässt sich nicht unterbrechen (P15); Schreiben der Datei ohne Abbrechen.
  - 3MF ohne ZIP64 (bis knapp 4 GiB), glTF bis 4 GiB (P7, P8).
  - Base64 im Dokument (Spec §11): Dokumente werden etwa 4/3 der Netzdaten groß, ein Binärblock wäre ein späteres Teilprojekt; `encode_version` liest jede geschriebene Version zur Kontrolle einmal zurück (doppelte Zeit bei großen Netzen).
  - Vorschau ab 5 Millionen Dreiecken: jedes k-te Dreieck, keine Vereinfachung des Netzes; die Anzeige baut das Netz bei jeder Änderung des Dokuments neu auf.
  - OBJ: nur Verweise auf schon gelesene Punkte (P6); Zeilenfortsetzung mit `\` wird nicht unterstützt; nicht einfache (sich selbst kreuzende) Vielecke, die die Konvexitätsprüfung bestehen, werden als Fächer zerlegt.
  - STL-Text: unbekannte Wörter sind ein Fehler (P5).
  - Netz-Körper: Verschieben über die eigene Lage, kein Feature „Verschieben“ (P10); Bearbeiten-Fenster gibt es für sie nicht, Lage im Eigenschaftenfenster.
  - Spec-Abweichung: `earcutr` ist Abhängigkeit von `planora-mesh`, nicht des Kernels (P6).
  - Dateiformat 3 samt mechanischer Testanpassung (P12).
  - Schätzung der Dateigröße: Faktor 0,5 bis 2 gegenüber der wirklichen Größe (gemessene Verhältnisse je Format aus Task 9 nennen).

- [ ] **Step 3: Spec §12** – den Platzhalter „(wird beim Abschluss des Teilprojekts ergänzt)“ in §12 ersetzen durch: Stand-Datum; eine Tabelle P1–P17 („Entscheidung“, „Begründung und Ergebnis“) wie in diesem Plan, je Zeile ergänzt um das Ergebnis aus der Umsetzung; danach „Weitere Abweichungen von §1–§11“ mit mindestens: eigene Dateiauswahl statt Dialog des Betriebssystems (§8, P14); `flipped_edges` im Bericht (§4, P2); OBJ wird beim Lesen ebenfalls verschmolzen (§4, P3); Base64 in `planora-project` statt in `planora-exchange` (§2, P12); `earcutr` aus `planora-mesh` (§3, P6); Abbrechen nur zwischen Körpern (§8, P15); Grenzen von 3MF und glTF bei 4 GiB (P7, P8); dazu die Befunde der Stufenreviews.

- [ ] **Step 4: Gesamtprüfung**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --release -- --ignored
cargo deny check
git diff --stat 134cde9 -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs
git diff --stat 134cde9 -- crates/planora-project/tests/data/format1.json crates/planora-project/tests/data/format1_voll.json crates/planora-project/tests/data/format2.json
```
Expected: alles grün; beide `git diff`-Ausgaben leer. Testzahl (bestanden/ignoriert) und Laufzeit der Release-Reihen in `offene-punkte.md` und Spec §12 eintragen. Schlägt eine Release-Reihe fehl, ist das ein Befund für den Bericht (nicht ignorieren, nicht abschwächen).

- [ ] **Step 5: Commit**

```bash
git add README.md docs/superpowers/offene-punkte.md docs/superpowers/specs/2026-10-14-netz-austausch-design.md
git commit -m "docs: Netz-Austausch – README, offene Punkte, Änderungen bei der Umsetzung

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 19: Handtest im Fenster (nur nach Rücksprache mit dem Nutzer, vom Controller ausgeführt)

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Abschnitt Teilprojekt 9, Absatz „Handtest“)

**Interfaces:**
- Consumes: Release-Build der App, `tools/gui-drive.ps1` (Befehle `shot`, `click`, `dclick`, `rclick`, `drag`, `wheel`, `type`; Screenshot nach `%TEMP%\planora_shot.png`).
- Produces: Bericht des Handtests (was geprüft, was nicht).

- [ ] **Step 1: Rückfrage** – Den Nutzer fragen, ob der Handtest jetzt laufen darf (er sitzt am Rechner, das Skript übernimmt Maus und Tastatur). Ohne ausdrückliches Ja nicht fortfahren. Implementierer führen diese Task nie aus, nur der Controller.

- [ ] **Step 2: Vorbereiten** (PowerShell, vorübergehende Benutzerordner, damit keine echten Projekte berührt werden)

```powershell
cargo build --release -p planora-app
$tmp = Join-Path $env:TEMP ("planora_hand_" + [guid]::NewGuid())
New-Item -ItemType Directory -Force $tmp | Out-Null
New-Item -ItemType Directory -Force (Join-Path $tmp "Documents") | Out-Null
$env:APPDATA = $tmp; $env:USERPROFILE = $tmp
Start-Process -FilePath .\target\release\planora-app.exe -RedirectStandardOutput "$tmp\out.log" -RedirectStandardError "$tmp\err.log"
```

- [ ] **Step 3: Ablauf** (je Schritt `powershell -File tools/gui-drive.ps1 <Befehl> …` und den Screenshot ansehen)

1. Quader 40 × 30 × 20 und Kugel r = 20 erzeugen; „Datei → Exportieren …“: Fenster reicht über die ganze Breite, die drei Karten Format/Qualität/Umfang sind gleich hoch; Umfang „Alle sichtbaren Körper“, Anzeige „… Dreiecke, ca. … MB“.
2. Qualität „Fein“ wählen: Fortschrittsbalken erscheint, „Abbrechen“ zeigt „Abgebrochen“, „Neu berechnen“ rechnet fertig.
3. Format STL, „Exportieren …“: Dateiauswahl im Ordner `Documents` des vorübergehenden Ordners, Name „Körper.stl“, „Speichern“ → „Fertig: „Körper.stl“ geschrieben …“. Ein zweites Mal speichern → Hinweis „Diese Datei gibt es schon …“, „Ersetzen“.
4. Ebenso 3MF und glTF schreiben (Dateien liegen im Ordner, Größe plausibel zur Schätzung).
5. „Datei → Netz importieren …“: `Körper.stl` wählen → Prüfung „… Dreiecke, … Punkte“, „Das Netz ist geschlossen.“; Einheit Zentimeter; „Importieren“ → Netz-Körper im Feature-Baum mit Netz-Symbol, Ansicht zeigt ihn zehnfach vergrößert.
6. Netz-Körper wählen: Eigenschaften zeigen Volumen, Position, Drehung, Maßstab; Position x um 50 ändern, Drehung z auf 45 → Ansicht folgt. Menü „Bearbeiten“: Vereinigen/Abziehen/Schneiden/Verrunden/Fase ausgegraut mit Hinweis „Netz-Körper lassen sich nicht bearbeiten.“
7. Speichern (neues Projekt im vorübergehenden Ordner), Programm schließen, neu öffnen, Dokument laden → Netz-Körper an gleicher Stelle, gleiches Volumen.
8. Strg+Umschalt+E mit gewähltem Netz-Körper → Umfang „Gewählter Körper“, OBJ exportieren.

- [ ] **Step 4: Aufräumen und berichten** – Programm beenden, `$tmp` löschen (`Remove-Item -Recurse -Force $tmp -Confirm:$false`). In `docs/superpowers/offene-punkte.md` im Abschnitt „Teilprojekt 9 – Netz-Austausch“ den Absatz „Handtest“ ergänzen: Datum, Build, was von Hand geprüft wurde und was nicht (etwa sehr große Netze, Abbrechen beim Lesen, Linux).

```bash
git add docs/superpowers/offene-punkte.md
git commit -m "docs: Handtest Netz-Austausch

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Anhang: Abdeckung der Spec (Selbstprüfung)

| Spec | Inhalt | Task |
|---|---|---|
| §1, E2 | Schreiben STL (binär), OBJ, 3MF, glTF (.glb); Lesen STL (binär und Text), OBJ | 3, 4, 7, 8, 9 |
| §1, E3, §6 | Netz-Körper: anzeigen, verschieben, drehen, skalieren, messen, wieder exportieren; keine Booleans/Rundungen/Skizzen | 10, 12, 13, 14, 16 |
| E4 | Umfang: gewählter Körper, sonst sichtbare; Unterdrücktes zählt nicht | 16 (`the_scope_follows_the_selection_and_suppressed_features_do_not_count`) |
| E5 | Grob 0,1 / Normal 0,01 / Fein 0,001 mm; Dreieckszahl und geschätzte Größe | 5 (alle drei Toleranzen), 9 (Schätzung), 16 |
| E6 | Nicht geschlossen: warnen, trotzdem schreiben | 2, 16 (`the_window_warns_about_open_meshes_and_still_exports`) |
| E7 | eigener ZIP-Schreiber „stored“, keine Abhängigkeit | 6, 7 |
| E8, §5 | 50 Mio. Dreiecke, 8 GB, kein Speicher nach Kopfzahlen | 1, 3, 4, 5 (`alloc_guard`), 11 |
| E9, §7 | Netz im Dokument, Format 3 nur mit Netz-Körper | 10, 11 |
| §2 | Paket `planora-exchange` mit lib/limits/check/stl/obj/zip/threemf/gltf/error, `ExchangeMesh`, `TriSource` | 1–9 (P1; zusätzlich `mesh`, `source`, `text`, `normals`, `file`, `progress`) |
| §2 | Einheiten: mm; 3MF `unit="millimeter"`; glTF Meter | 7, 8 (`positions_are_written_in_metres…`) |
| §2 | Orientierung: Außenseite gegen den Uhrzeigersinn, Import übernimmt, Prüfung meldet unstimmige | 2 (`flipped_edges`), 4 (Orientierung der Vielecke) |
| §3 | STL-Kopf „Planora“, Normale aus Punkten, Attribut 0, alle Körper in einer Datei | 3 (`binary_layout_matches_the_spec`, `several_bodies_go_into_one_file`) |
| §3 | STL-Text: Erkennung über `solid` und Größe; mehrere `solid`-Blöcke | 3 |
| §3 | OBJ `o`/`v`/`vn`/`f a//n`; Lesen `v`, `f` mit allen Formen, Fächer, `earcutr`, negative Indizes, Rest ignoriert | 4 |
| §3 | 3MF Teile, `object` `type="model"`, `build`/`item`, ≥ 6 Nachkommastellen | 7 |
| §3 | glTF ein Puffer, Mesh je Körper, POSITION/NORMAL/indices u32, Knoten, asset | 8 |
| §4 | `MeshReport`, `closed`, entartete gezählt, `weld_exact` bitgleich, Bericht im Exportfenster | 2, 16 |
| §5 | Ablehnungen: > 8 GB, > 50 Mio., NaN/∞, Indizes außerhalb, abgeschnitten, Kopfzahl zu groß | 3, 4, 5 |
| §5 | benannte deutsche Fehler mit Zeilennummer | 1, 3, 4 |
| §5 | Export über Hilfsdatei, Zieldatei bleibt bei Fehlern | 9 (`a_failed_export_keeps_the_old_file_and_leaves_no_helper`) |
| §5 | Import ändert das Dokument erst nach Erfolg | 10 (`import_is_all_or_nothing`), 12, 17 |
| §6 | `FeatureKind::MeshBody`, Tabelle `meshes`, Mehrfachverwendung | 10 |
| §6 | Maßstab 1e-6 … 1e6, Drehung normiert, Lage endlich | 10 (`bad_parameters_become_named_error_states`) |
| §6 | Baum (Symbol „Netz“), Unterdrücken, Löschen; Eigenschaften (Name, Position, Drehung in Grad, Maßstab, Volumen, Oberfläche, Abmessungen, Bericht) | 10 (Symbol), 13 |
| §6 | Bearbeitungssperre: Menüs ausgegraut mit Hinweis; Neuberechnung lehnt Verweise benannt ab | 10, 12, 13 |
| §6 | Anzeige: glatt unter 30°, sonst hart; keine Kantenlinien | 8 (`shade`), 12 |
| §6 | Wiederexport mit Transformation | 2 (`PlacedSource`), 14, 17 |
| §7 | Base64 kompakt (f32/u32, LE, Anzahl vorn); Prüfungen vor dem Dekodieren; „zu neu“ für Format ≤ 2; Golden `format3.json`, alte Golden unverändert | 11, 14 (Prüfung der alten Golden) |
| §8 | Exportieren-Fenster (Strg+Umschalt+E): Format, Qualität, Umfang, Anzeige, Bericht, „Exportieren …“, Arbeitsthread mit Fortschritt und Abbrechen | 15, 16 |
| §8 | Netz-importieren-Fenster: Dateiwahl, Bericht, Name, Einheit (Vorgabe mm), „Importieren“, Arbeitsthread, Abbrechen | 15, 17 |
| §8 | einfache Sprache, volle Breite, gleich hohe Karten | 15, 16, 17 (Tests zu Breite und Kartenhöhe) |
| §9 | Rundlauf STL/OBJ (Dreiecke, Volumen, Fläche, Geschlossenheit) | 5 |
| §9 | Formeln Quader, Zylinder, Kugel, gerundeter Körper | 5 (`closed_formulas_survive_the_files`) |
| §9 | Mini-Leser 3MF (ZIP, CRC, stored, XML, Zahlen) und glTF (Kopf, JSON, Puffer, Zugriffe, min/max) | 6, 7, 8, 9 |
| §9 | Feindliche Dateien inkl. CRLF, BOM, riesige Zahlen, Allocator-Zähler | 3, 4, 5, 11 |
| §9 | Dokument: Format 3 speichern/laden, Format 1/2 lesbar, „zu neu“ simuliert, Mehrfachverwendung, Transformation, Unterdrücken, Löschen, Sperre | 10, 11, 14 |
| §9 | Zufallsreihe gegen Masseneigenschaften | 5 |
| §9 | Handtest nach Rückfrage | 19 |
| §10 | vier Stufen; nach jeder Tests, fmt, clippy, deny | 5, 9, 14, 18 (Stufenabschlüsse) |
| §11 | Risiken: große Netze (Vorschau ab 5 Mio.), Base64-Größe, offene Netze, nicht konvexe OBJ-Vielecke, STL-Erkennung | 12, 18, 2/16, 4, 3 |
| §12 | Änderungen bei der Umsetzung | 18 |

**Platzhalter-Prüfung:** Der Plan enthält keine offenen Stellen im Code. Zahlen, die erst die Umsetzung liefert (Testanzahl, Laufzeiten, gemessene Dateigrößen), werden in Task 18 aus den genannten Ausgaben eingetragen.

**Typkonsistenz (geprüft):** `ExchangeMesh`, `MeshTransform`, `MeshReport`, `ExportBody`, `TriSource`/`SolidSource`/`PlacedSource`, `collect_body`, `Progress` (Task 1–2) werden in Task 3–17 mit denselben Namen und Signaturen benutzt; `ExchangeError::Cancelled` überall gleich geschrieben; `FeatureKind::MeshBody { mesh, offset, rotation, scale }`, `MeshId`, `VisibleMesh { body, mesh_id, mesh, transform, name }`, `Document::{add_mesh, mesh, mesh_count, mesh_body, add_mesh_body, visible_meshes}` (Task 10) in Task 11–17; `KindDto::NetzKoerper { netz, versatz, drehung, massstab }` (Task 10) in Task 11; `Format`, `export_file`, `import_file`, `temp_path` (Task 9) in Task 14–17; `Job`/`Poll`, `FileChooser`/`ChooserAction`, `card_row` (Task 15) in Task 16–17; `DIALOG_MARGIN`, `DIALOG_TOP`, `LOST`, `group_digits`, `file_name` (Task 16) in Task 17.
