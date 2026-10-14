# Zeichnungen – Ansichten (Teilprojekt 11) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aus den exakten Körpern eines Dokuments entstehen Zeichnungen (Normansichten und Isometrie mit sichtbaren und verdeckten Kanten, echte Kurven), die im Dokument gespeichert, live neu berechnet und als SVG und DXF ausgegeben werden.

**Architecture:** Neues Paket `planora-drawing` (reine Berechnung): Kanten und Umrisse der Körper sind „Striche“ (3D-Kurve + Parameterbereich); Parallelprojektion liefert 2D-Kurven, paarweise 2D-Schnitte teilen die Striche, ein exakter Strahl gegen die echten Flächen entscheidet je Stück sichtbar/verdeckt. Datenmodell (Zeichnung, Blatt, Ansichten) lebt im `Document` (`planora-feature`) und in der Versionsdatei (`planora-project`, Format 4). `planora-app` bekommt einen Zeichnungsbereich, Hintergrundberechnung, Browser-Abschnitt und zwei Exportformate.

**Tech Stack:** Rust 2024 (Workspace), `planora-kernel` (B-Rep, `boolean::{FaceGeom, face_geom, point_in_face, Seg}`), `planora-math` (`Frame`, `NurbsCurve`, `Tolerance`), `planora-exchange` (`Progress`), `serde`/`serde_json` (nur Format), egui 0.36. Keine neue externe Abhängigkeit.

**Spec:** `docs/superpowers/specs/2026-10-14-zeichnungen-ansichten-design.md` (Spec). Der Plan argumentiert aus der Spec; bei Widerspruch gilt die Spec, mit den **Festlegungen der Planung** unten (Abschnitt „Festlegungen“).

## Global Constraints

- Branch `feature/zeichnungen-ansichten` (Basis `master` 9a44307, Spec-Commit 945fa67). Nicht auf `master` arbeiten; Merge entscheidet der Nutzer.
- **Geschützte Pfade bleiben Byte für Byte wie in 945fa67:** `crates/planora-kernel/**` (auch `boolean/**`), `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`. Wer etwas aus dem Kernel braucht, nimmt nur dessen **öffentliche** Schnittstelle. Braucht eine Aufgabe zwingend eine Kernel-Änderung, ist das eine **Festlegung** (Ruling mit Begründung ins Protokoll) und eine eigene, kleine Aufgabe mit Rückfrage beim Controller – nie nebenbei.
- Neues Paket `crates/planora-drawing`; Eintrag in `Cargo.toml` (`[workspace.dependencies]`: `planora-drawing = { path = "crates/planora-drawing" }`). Abhängigkeiten von `planora-drawing`: `planora-kernel`, `planora-math`, `planora-exchange` (für `Progress`), `thiserror`. **Nicht** `planora-mesh` in der Bibliothek; `planora-mesh` nur als `[dev-dependencies]` für die Prüfwerkzeuge der Tests. Keine neue externe Abhängigkeit. `cargo deny check` bleibt grün (Lizenzen GPL-3.0-only-kompatibel; kein Autodesk-Material kopieren, DXF-Gruppencodes und SVG sind offene Formate).
- Abhängigkeitsrichtung: `planora-drawing` ← `planora-feature` (Typen im `Document`) ← `planora-project` ← `planora-app`. `planora-drawing` darf `planora-feature` **nicht** kennen.
- Längen in **Millimetern**, Winkel in Radiant (Dateien: SVG in mm, DXF `$INSUNITS = 4`; DXF-Winkel in Grad).
- Toleranzen skalieren mit der Ausdehnung `E` (größte Kante der Hüllbox aller Körper, mindestens 1e-9): `geom_tol(E) = max(1e-9, 64·f64::EPSILON·E)`; `SELF_EPS = 1e-7·E`; `OWN_EPS = 1e-5·E` (für Treffer auf den eigenen Flächen eines Strichs); `SILHOUETTE_TOL = 1e-4` mm (Torus, Freiform); `SMOOTH_ANGLE = 0,5°`; `BISECT_TOL = 1e-9` (relativ zur Parameterbreite). Feste absolute Toleranzen sind verboten (Lehre aus TP10).
- Arbeitsgrenzen (Konstanten in `limits.rs`): `MAX_STROKES = 20_000`, `MAX_PIECES = 200_000`, `MAX_PAIR_TESTS = 50_000_000`, `MAX_RAY_TESTS = 20_000_000`, `MAX_SUBDIVISION_DEPTH = 40`. Überschreiten → `DrawError::TooComplex(zahl)`; nie stillschweigend kürzen.
- Fortschritt und Abbruch über `planora_exchange::Progress` (`set_total`, `advance`, `check`); `check` in jeder Schleife über Striche, Paare und Stücke.
- Alle Meldungen deutsch, Laiensprache, ohne Fachwörter wie „Strahl“ oder „B-Rep“ in Oberflächentexten; keine Hinweise auf Datenherkunft (Gedächtnis: Texte der Oberfläche). Oberflächen: immer volle Fensterbreite, nebeneinanderstehende Karten gleich hoch.
- Quality Gates je Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p <betroffene Pakete>`; vor dem Abschluss (Aufgabe 18): `cargo test --workspace`, `cargo deny check`, Prüfung der geschützten Pfade.
- **Tests dürfen nicht dieselbe Rechnung wiederholen** (Lehre aus TP10): eigene Formelmodelle und ein eigener Netz-Prüfer (Aufgabe 6), Mutationsbeweis für jeden Regressionstest (kurz die Korrektur zurücknehmen, Test muss rot werden, Ergebnis im Bericht).
- Commit-Stil: `feat: …` / `test: …` / `fix: …` / `docs: …` (deutsch, Kleinschreibung nach dem Doppelpunkt), mit der Zeile `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Testcode in diesem Plan ist Vorlage: Namen an die tatsächliche API anpassen (Kernel-Konstruktoren in `planora-kernel/src/primitives.rs`, `extrude.rs`, `revolve.rs`), die geprüften Zahlen **nicht** abschwächen.

## Review Focus

Eingaben, die die Spec nahelegt, die aber in keinem einzelnen Test von selbst vorkommen; jede Zeile hat einen Test in der genannten Aufgabe:

1. **Deckungsgleiche Kanten** (Quader von vorne: vordere und hintere Kante fallen zusammen): genau **eine sichtbare** Linie, nicht sichtbar und gestrichelt übereinander – Aufgabe 6/8.
2. **Ansicht fast parallel zu einer Fläche** (Schrägansicht, Streifstrahlen): richtige Sichtbarkeit oder die benannte Meldung `AmbiguousVisibility`, nie stilles Raten – Aufgabe 6.
3. **Leeres Dokument, nur Netz-Körper, ein Körper mit Fehler**: verständliche Meldung statt Absturz oder leerem Blatt; die Zeichnung bleibt im Dokument erhalten – Aufgabe 14/16.
4. **Modelländerung bei offener Zeichnung** (Feature gelöscht, Maße geändert): Ansichten rechnen neu, alte Linien verschwinden, eine laufende Berechnung wird abgebrochen – Aufgabe 14.
5. **Sehr große oder sehr kleine Körper** (Ausdehnung 1e5 mm bzw. 1e-1 mm) und weit vom Ursprung entfernte Körper: Toleranzen skalieren, Ergebnis gleich – Aufgabe 6.
6. **Datei mit Zeichnungen von fremder Hand** (unbekannte Ansichtsart, NaN, zu große Zahlen, doppelte Kennungen) und **alte Dateien ohne Zeichnungen**: klare Fehlermeldung statt Absturz; alte Dateien bleiben Byte für Byte unverändert – Aufgabe 13.

## Festlegungen der Planung (Rulings gegenüber der Spec)

Diese Punkte präzisieren die Spec; der Controller trägt sie ins Protokoll ein.

- **F1 Fortschritt:** `Progress` kommt aus `planora-exchange` (die Spec nennt versehentlich `planora-mesh`). Die Spec wird in Aufgabe 18 berichtigt.
- **F2 Striche statt 2D-Kurventypen:** Die Spec nennt `curve2.rs` mit `Line2/Arc2/Ellipse2/Spline2`. Der Plan führt stattdessen den **Strich** ein: `Stroke { curve: Curve (3D), t0, t1, origin }`; die Projektion wertet die 3D-Kurve aus und projiziert (`p(t) = (P(t)·ex, P(t)·ey)`). So behält jede Projektion den Parameter der 3D-Kurve (Kreis → Ellipse mit gleichem `t`, Spline → Spline), Teilen heißt „Parameterbereich teilen“, und es gibt keinen zweiten Kurventyp-Satz. 2D-Typen (`Curve2Out`) entstehen **erst für die Ausgabe** (Aufgabe 9).
- **F3 Lage der Ansichten:** `Placement::Free { x, y }` oder `Placement::Aligned { distance }` (Abstand der Ansichtsmitten entlang der Ausrichtungsachse); Spec §3 „Abgeleitete Ansichten dürfen nur längs verschoben werden“ ist damit im Datentyp erzwungen.
- **F4 Selbsttreffer:** Treffer mit `s ≤ SELF_EPS` zählen nie als Verdeckung (alle Flächen, nicht nur anliegende, wie die Spec in §4.3 Schritt 2 knapper sagt); auf den **eigenen** Flächen des Strichs gilt `OWN_EPS`.
- **F5 Torus-Umriss:** wird nicht marschiert, sondern **parametrisch** in `u` berechnet (`v(u) = atan2(−(dx·cos u + dy·sin u), dz)` bzw. `+π`, Sonderfall `dz = 0`), dann als Spline angepasst; nur Freiformflächen marschieren.
- **F6 Doppelte Striche:** deckungsgleiche Stücke (Ort, Art, Länge innerhalb `geom_tol`) werden zu **einem** Stück vereint, sichtbar wenn eines davon sichtbar ist (Spec §4.4).
- **F7 Format:** `FORMAT_VERSION = 4`; Format 4 wird nur geschrieben, wenn das Dokument mindestens eine Zeichnung enthält (wie Format 3 mit Netz-Körpern).
- **F8 Berechnungsschlüssel:** Ein Ansichtsergebnis hängt nur an `(model_edit_count, ViewKind)`; Maßstab, Lage und andere Zeichnungsänderungen ändern es nicht (werden erst beim Zeichnen/Schreiben angewandt). `Document::edit_count()` zählt weiterhin **alle** Änderungen (ungespeichert-Marke), `model_edit_count()` nur die an Körpern.
- **F9 Kippabstimmung statt Einzelstrahl (ersetzt Spec §4.3 Schritte 2–3):** Ein einzelner Strahl scheitert an Linien, die genau auf dem Umriss einer Fläche liegen oder in der Ebene einer von der Seite gesehenen Fläche (Quader von vorne, Zylinderdeckel). Jede Stützstelle bekommt deshalb 8 leicht gekippte Strahlen (`TILT`); **frei** unter irgendeinem Kippstrahl heißt sichtbar (Umrisskonvention), verdeckt nur, wenn alle brauchbaren Strahlen verdeckt sind; unbrauchbare (Randtreffer, `Unsafe`) zählen nicht; ohne einen brauchbaren Strahl weicht die Stelle aus, danach `AmbiguousVisibility`. Edge-on-Flächen verdecken nichts (Fläche mit `|n·d| ≈ 0` hat keine Bildfläche).
- **F10 Entartete Striche:** Striche, die in der Ansicht zu einem Punkt entarten (Kante parallel zur Blickrichtung), werden vor dem Teilen verworfen; ihre Endpunkte sind Endpunkte der Nachbarstriche.
- **F11 Kein Zugriff über zwei Stellen:** Der Zeichnungsbereich (Aufgabe 14) und der Export-Dialog (Aufgabe 17) rechnen Ansichten **getrennt** (je eigener Job); es gibt keinen gemeinsamen veränderlichen Zwischenspeicher.

## Dateistruktur

```
Cargo.toml                                   (Mitglied kommt über crates/*; Eintrag in [workspace.dependencies])
crates/planora-drawing/
  Cargo.toml
  src/lib.rs        // Öffentliche Schnittstelle, compute_view, ViewLines, DrawBody
  src/error.rs      // DrawError (deutsch)
  src/limits.rs     // Konstanten (Toleranzen, Grenzen)
  src/model.rs      // Drawing, DrawingId, Sheet, Paper, Projection, ViewSpec, ViewId, ViewKind, Scale, Placement
  src/basis.rs      // Basis (d, ex, ey) je ViewKind, Projektion von Punkten
  src/layout.rs     // Papiermaße, Maßstabsreihe, Lage abgeleiteter Ansichten, Blatt-Transformation
  src/stroke.rs     // Stroke, StrokeOrigin, Strichliste aus Körpern (Kanten, glatte Kanten ausgeschlossen)
  src/silhouette.rs // Umriss-Striche: Zylinder, Kegel, Kugel (exakt); Torus, Freiform (Spline)
  src/isect.rs      // 2D-Schnitte von Strichen, Teilpunkte
  src/raycast.rs    // Strahl gegen Flächen (analytisch, Freiform), Treffer mit Fläche, Randnähe
  src/visibility.rs // Teilen, Strahlprüfung, Bisektion, Zusammenfassen
  src/sheet.rs      // ViewLines → Blatt-Linien (Maßstab, Lage, Rand), Linienarten
  src/out2d.rs      // Ausgabekurven: Line, Arc, Ellipse (Hauptachsen), Spline
  src/svg.rs        // SVG-Schreiber
  src/dxf.rs        // DXF-Schreiber (R2000)
  tests/common/{mod.rs,model.rs,mesh_ref.rs,check.rs,svg_read.rs,dxf_read.rs}   // eigene Prüfwerkzeuge
  tests/*.rs        // Stufentests
crates/planora-feature/src/drawings.rs       // Zeichnungen im Document (Aufgabe 12)
crates/planora-project/                      // Format 4 (Aufgabe 13)
crates/planora-app/src/drawing_state.rs      // Zeichnungsstand, Hintergrundberechnung (Aufgabe 14)
crates/planora-app/src/drawing_view.rs       // Blattansicht (Aufgabe 15)
crates/planora-app/src/drawing_dialog.rs     // Neue Zeichnung, Ansicht hinzufügen (Aufgabe 15)
```

---

### Task 1: Paket, Datenmodell, Ansichtsbasis und Blattlayout

**Files:**
- Create: `crates/planora-drawing/Cargo.toml`, `src/{lib,error,limits,model,basis,layout}.rs`, `tests/stage1_model.rs`
- Modify: `Cargo.toml` (Workspace-Abhängigkeit)

**Interfaces:**
- Produces (`model.rs`):
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct DrawingId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct ViewId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Paper { A4, A3, A2, A1, A0 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Projection { First, Third }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct Sheet { pub paper: Paper, pub landscape: bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum ViewKind { Front, Back, Left, Right, Top, Bottom, Isometric }
#[derive(Clone, Copy, Debug, PartialEq)] pub struct Scale { pub num: u32, pub den: u32 }   // num:den, beide > 0
#[derive(Clone, Copy, Debug, PartialEq)] pub enum Placement { Free { x: f64, y: f64 }, Aligned { distance: f64 } }
#[derive(Clone, Debug, PartialEq)] pub struct ViewSpec { pub id: ViewId, pub kind: ViewKind, pub scale: Scale,
    pub placement: Placement, pub show_hidden: bool, pub parent: Option<ViewId> }
#[derive(Clone, Debug, PartialEq)] pub struct Drawing { pub id: DrawingId, pub name: String, pub sheet: Sheet,
    pub projection: Projection, pub views: Vec<ViewSpec>, pub next_view: u64 }
```
  Methoden: `Paper::size_mm(self) -> (f64, f64)` (hoch: A4 = 210×297, A3 297×420, A2 420×594, A1 594×841, A0 841×1189); `Sheet::size_mm(&self) -> (f64, f64)` (Breite, Höhe; quer vertauscht); `Scale::factor(self) -> f64`; `Scale::label(self) -> String` (`"1:2"`, `"2:1"`); `Scale::SERIES: [Scale; 10]` = 1:1, 1:2, 1:5, 1:10, 1:20, 1:50, 1:100, 2:1, 5:1, 10:1; `Scale::is_valid(self) -> bool`; `ViewKind::label(self) -> &'static str` (Vorne, Hinten, Links, Rechts, Oben, Unten, Isometrisch); `ViewKind::ALL: [ViewKind; 7]`; `ViewKind::is_normal(self) -> bool` (alles außer Isometrisch); `Drawing::new(id, name, sheet, projection) -> Drawing` (leer, `next_view = 1`); `Drawing::add_view(&mut self, kind, scale, placement, parent) -> ViewId`; `Drawing::view(&self, ViewId) -> Option<&ViewSpec>`; `Drawing::remove_view(&mut self, ViewId) -> Vec<ViewId>` (entfernt die Ansicht und alle von ihr abgeleiteten, gibt die entfernten Kennungen zurück; Kinder werden **mit** gelöscht).
- Produces (`basis.rs`): `pub struct Basis { pub d: DVec3, pub ex: DVec3, pub ey: DVec3 }` mit `Basis::of(kind: ViewKind) -> Basis`, `Basis::project(&self, p: DVec3) -> DVec2` (`(p·ex, p·ey)`), `Basis::depth(&self, p: DVec3) -> f64` (`p·d`), `Basis::eye_dir(&self) -> DVec3` (`−d`). Tabelle:

| Art | `d` | `ey` | `ex = d × ey` |
|---|---|---|---|
| Front | (0,1,0) | (0,0,1) | (1,0,0) |
| Back | (0,−1,0) | (0,0,1) | (−1,0,0) |
| Left | (1,0,0) | (0,0,1) | (0,−1,0) |
| Right | (−1,0,0) | (0,0,1) | (0,1,0) |
| Top | (0,0,−1) | (0,1,0) | (1,0,0) |
| Bottom | (0,0,1) | (0,−1,0) | (1,0,0) |
| Isometric | (−1,1,−1)/√3 | `normalize(Z − (Z·(−d))·(−d))` | `d × ey` |

  Invarianten (Test): `ex`, `ey`, `d` paarweise senkrecht und Länge 1; `ex × ey = −d` (Rechtssystem).
- Produces (`layout.rs`): `pub const GAP_MM: f64 = 20.0` (Abstand zwischen Ansichten), `pub fn align_dir(projection: Projection, parent: ViewKind, child: ViewKind) -> (f64, f64)` – Einheitsrichtung auf dem Blatt, in der das abgeleitete Bild vom Elternbild liegt: Bestimmen von `e = Basis::of(child).eye_dir()`, `(a, b) = (e·ex_parent, e·ey_parent)`; Erstwinkel: `(−a, −b)`, Dritter Winkel: `(a, b)`; ist `a² + b² < 1e-9` (Gegenansicht), dann `(1, 0)` (rechts, beide Projektionsarten). Ergebnis auf die dominante Achse gerundet (genau eine Komponente ±1 für senkrecht benachbarte Ansichten; für Isometrisch als Kind nicht erlaubt: `None`). Signatur daher `-> Option<(f64, f64)>`. `pub fn view_extent(lines_bbox: ((f64,f64),(f64,f64)), scale: Scale) -> (f64, f64)` (Breite, Höhe in mm auf dem Blatt).
- Produces (`error.rs`): `DrawError` mit `thiserror`: `NoBodies` („Es gibt keinen Körper, der sich zeichnen lässt.“), `MeshBodiesOnly` („Alle Körper sind Netze; eine Zeichnung braucht exakte Körper.“), `AmbiguousVisibility(String)` („Bei „{0}“ lässt sich nicht sicher sagen, ob die Linie verdeckt ist. Eine andere Ansicht oder ein anderer Maßstab hilft nicht; bitte melden Sie den Körper.“), `TooComplex(u64)` („Der Körper ist für diese Ansicht zu aufwendig ({0} Teilstücke).“), `NonFinite`, `Cancelled`, `Io(std::io::Error)` (`#[from]`), `InvalidBody(String)`, `Unsupported(String)`. `From<planora_exchange::ExchangeError>` für Abbruch (nur `Cancelled` abbilden, Rest → `Unsupported(text)`).
- Produces (`limits.rs`): alle Konstanten der Global Constraints als `pub const`, dazu `pub fn geom_tol(extent: f64) -> f64`.

- [ ] **Step 1: Paket anlegen.** `crates/planora-drawing/Cargo.toml` (Muster `crates/planora-step/Cargo.toml`: `version.workspace = true` usw., `[lints] workspace = true`), Abhängigkeiten `planora-math`, `planora-kernel`, `planora-exchange`, `thiserror` (alle `.workspace = true`), `[dev-dependencies] planora-mesh.workspace = true`. In der Wurzel-`Cargo.toml` unter `[workspace.dependencies]` ergänzen: `planora-drawing = { path = "crates/planora-drawing" }`. `lib.rs` enthält Modulkopf und `pub use`.

- [ ] **Step 2: Failing tests** (`tests/stage1_model.rs`):

```rust
use planora_drawing::{basis::Basis, layout::*, model::*};
use planora_math::DVec3;

#[test]
fn bases_are_orthonormal_right_handed() {
    for k in ViewKind::ALL {
        let b = Basis::of(k);
        for (a, c) in [(b.d, b.ex), (b.d, b.ey), (b.ex, b.ey)] {
            assert!(a.dot(c).abs() < 1e-14, "{k:?} nicht senkrecht");
        }
        for v in [b.d, b.ex, b.ey] {
            assert!((v.length() - 1.0).abs() < 1e-14);
        }
        assert!((b.ex.cross(b.ey) + b.d).length() < 1e-14, "{k:?} kein Rechtssystem");
    }
}

#[test]
fn front_view_projects_x_right_z_up() {
    let b = Basis::of(ViewKind::Front);
    let p = b.project(DVec3::new(3.0, 5.0, 7.0));
    assert_eq!((p.x, p.y), (3.0, 7.0));
    assert_eq!(b.depth(DVec3::new(3.0, 5.0, 7.0)), 5.0);
}

#[test]
fn first_angle_places_top_below_and_left_right_of_front() {
    let top = align_dir(Projection::First, ViewKind::Front, ViewKind::Top).unwrap();
    let left = align_dir(Projection::First, ViewKind::Front, ViewKind::Left).unwrap();
    assert_eq!(top, (0.0, -1.0));
    assert_eq!(left, (1.0, 0.0));
}

#[test]
fn third_angle_mirrors_first_angle() {
    let top = align_dir(Projection::Third, ViewKind::Front, ViewKind::Top).unwrap();
    let right = align_dir(Projection::Third, ViewKind::Front, ViewKind::Right).unwrap();
    assert_eq!(top, (0.0, 1.0));
    assert_eq!(right, (1.0, 0.0));
}

#[test]
fn back_view_sits_right_of_base_in_both_projections() {
    assert_eq!(align_dir(Projection::First, ViewKind::Front, ViewKind::Back), Some((1.0, 0.0)));
    assert_eq!(align_dir(Projection::Third, ViewKind::Front, ViewKind::Back), Some((1.0, 0.0)));
}

#[test]
fn isometric_cannot_be_derived() {
    assert_eq!(align_dir(Projection::First, ViewKind::Front, ViewKind::Isometric), None);
}

#[test]
fn paper_sizes_and_orientation() {
    assert_eq!(Paper::A4.size_mm(), (210.0, 297.0));
    let s = Sheet { paper: Paper::A3, landscape: true };
    assert_eq!(s.size_mm(), (420.0, 297.0));
}

#[test]
fn scale_series_and_labels() {
    assert_eq!(Scale::SERIES.len(), 10);
    assert_eq!(Scale { num: 1, den: 2 }.label(), "1:2");
    assert_eq!(Scale { num: 5, den: 1 }.label(), "5:1");
    assert!((Scale { num: 1, den: 4 }.factor() - 0.25).abs() < 1e-15);
    assert!(!Scale { num: 0, den: 1 }.is_valid());
}

#[test]
fn removing_a_view_removes_its_children() {
    let mut d = Drawing::new(DrawingId(1), "Z".into(), Sheet { paper: Paper::A4, landscape: true }, Projection::First);
    let f = d.add_view(ViewKind::Front, Scale { num: 1, den: 1 }, Placement::Free { x: 100.0, y: 100.0 }, None);
    let t = d.add_view(ViewKind::Top, Scale { num: 1, den: 1 }, Placement::Aligned { distance: 80.0 }, Some(f));
    let iso = d.add_view(ViewKind::Isometric, Scale { num: 1, den: 1 }, Placement::Free { x: 200.0, y: 50.0 }, None);
    let gone = d.remove_view(f);
    assert_eq!(gone, vec![f, t]);
    assert!(d.view(iso).is_some() && d.view(f).is_none() && d.view(t).is_none());
}
```

- [ ] **Step 3: Run to confirm failure** — `cargo test -p planora-drawing --test stage1_model` → FAIL (Paket/Module fehlen).
- [ ] **Step 4: Implementieren** nach der Schnittstelle oben. `align_dir`: wie beschrieben, dominante Achse runden (`|a| ≥ |b|` → `(sign(a), 0)` sonst `(0, sign(b))`), Erstwinkel negieren. `Drawing::remove_view` entfernt rekursiv Kinder, Reihenfolge der zurückgegebenen Kennungen: erst die Ansicht, dann Kinder in Listenreihenfolge.
- [ ] **Step 5: Run to verify pass** — `cargo test -p planora-drawing`, `cargo clippy -p planora-drawing --all-targets -- -D warnings`, `cargo fmt --all --check`.
- [ ] **Step 6: Commit** — `feat: Paket planora-drawing mit Datenmodell, Ansichtsbasis und Blattlayout`.

---

### Task 2: Striche aus den Körpern (Kanten, glatte Kanten, Nähte)

**Files:**
- Create: `crates/planora-drawing/src/stroke.rs`, `tests/common/{mod.rs,model.rs}` (Anfang), `tests/stage2_strokes.rs`
- Modify: `src/lib.rs` (`DrawBody`, Module)

**Interfaces:**
- Consumes: `planora_kernel::{Solid, Curve, Edge, Id, Face, topology}`, `Basis` (Task 1), `limits`.
- Produces (`lib.rs`): `pub struct DrawBody<'a> { pub name: String, pub solid: &'a Solid }`.
- Produces (`stroke.rs`):
```rust
#[derive(Clone, Debug, PartialEq)]
pub enum StrokeOrigin {
    /// Kante eines Körpers: Körperindex im Eingabefeld und Kantenkennung (Index in `solid.edges`).
    Edge { body: usize, edge: usize },
    /// Umriss einer Fläche (Körperindex, Flächenindex).
    Silhouette { body: usize, face: usize },
}
#[derive(Clone, Debug)]
pub struct Stroke { pub curve: Curve, pub t0: f64, pub t1: f64, pub origin: StrokeOrigin }
impl Stroke {
    pub fn point(&self, t: f64) -> DVec3;                       // 3D
    pub fn point2(&self, basis: &Basis, t: f64) -> DVec2;       // projiziert
    pub fn deriv2(&self, basis: &Basis, t: f64) -> DVec2;       // d/dt des projizierten Punkts (zentrale Differenz oder analytisch)
    pub fn param_len(&self) -> f64;                             // t1 - t0 (positiv)
    pub fn bbox2(&self, basis: &Basis) -> (DVec2, DVec2);       // dichte Probe + Kontrollpunkte bei NURBS, Rand um geom_tol erweitert
    pub fn is_degenerate_2d(&self, basis: &Basis, tol: f64) -> bool;  // projizierte Länge < tol (Kante parallel zu d)
}
pub fn edge_strokes(bodies: &[DrawBody], basis: &Basis, extent: f64) -> Vec<Stroke>;
pub fn extent_of(bodies: &[DrawBody]) -> f64;    // größte Kante der Hüllbox aller Eckpunkte (Kantenpunkte per Probe), mindestens 1e-9
```
  `edge_strokes` liefert je Kante des Körpers einen Strich, **außer**: (a) Kanten, die beide Seiten derselben Fläche begrenzen (Nähte), (b) **glatte** Kanten: Winkel zwischen den Flächennormalen beider angrenzenden Flächen entlang der Kante an mindestens 5 Stützstellen (Kantenparameter gleichverteilt, Enden eingeschlossen) höchstens `SMOOTH_ANGLE` (0,5°). Normalen werden nach `same_sense` ausgerichtet (Außennormale; `Surface::normal(u,v)` mit `Surface::project(p)` für `(u,v)`, bei `Nurbs` `project_near`). (c) Striche, die `is_degenerate_2d` sind, werden **nicht** entfernt (sie liefern Teilpunkte); sie werden in Task 6 beim Ausgeben verworfen (Länge 0 → ein Punkt, nicht zeichnen).
  Zur Zuordnung Kante→Flächen: einmal über alle Flächenschleifen (`outer`, `inner`) alle `HalfEdge { edge, .. }` zählen; Kante mit beiden Halbkanten in **derselben** Fläche = Naht; Kante mit weniger als zwei Halbkanten → `DrawError::InvalidBody("Kante ohne zwei Flächen")` (der Aufrufer reicht den Körpernamen nach).
  Rückgabe daher `Result<Vec<Stroke>, DrawError>` (Signatur oben um `Result` ergänzen).

- [ ] **Step 1: Prüfwerkzeug anlegen** (`tests/common/model.rs`): Formelmodell des Quaders `[0,a]×[0,b]×[0,c]` und Zylinders: Funktionen `box_solid(a,b,c) -> Solid` (Ecke im Ursprung; über `planora_kernel::primitives`/Kernel-Konstruktor, den Muster-Test `crates/planora-kernel/tests/geometry.rs` zeigt), `cylinder_solid(r,h) -> Solid` (Achse Z, Mittelpunkt des Bodens im Ursprung), `sphere_solid(r)` (Mittelpunkt im Ursprung), `cone_solid(r0,r1,h)` (Kegelstumpf, Achse Z, Boden im Ursprung), `torus_solid(major,minor)` (Achse Z, Mittelpunkt im Ursprung), `half_cylinder_solid(r,h)`, `nurbs_slab_solid() -> (Solid, usize)`, `nurbs_slab_center_point_and_normal`, und `total_len(strokes, basis) -> f64` (numerische Längensumme `∑ |p'(t)| dt` mit 4096 Stützstellen je Strich, **unabhängig** von der Bibliothek geschrieben).
- [ ] **Step 2: Failing tests** (`tests/stage2_strokes.rs`):

```rust
mod common;
use common::model::*;
use planora_drawing::{DrawBody, basis::Basis, model::ViewKind, stroke::*};

fn bodies(s: &planora_kernel::Solid) -> Vec<DrawBody<'_>> { vec![DrawBody { name: "K".into(), solid: s }] }

#[test]
fn box_has_twelve_edge_strokes() {
    let s = box_solid(10.0, 20.0, 30.0);
    let b = bodies(&s);
    let e = extent_of(&b);
    let strokes = edge_strokes(&b, &Basis::of(ViewKind::Front), e).unwrap();
    assert_eq!(strokes.len(), 12);
    assert!((e - 30.0).abs() < 1e-9);
}

#[test]
fn front_view_of_box_projects_to_rectangle_lengths() {
    let s = box_solid(10.0, 20.0, 30.0);
    let b = bodies(&s);
    let basis = Basis::of(ViewKind::Front);
    let strokes = edge_strokes(&b, &basis, extent_of(&b)).unwrap();
    // Vorne: 4 Kanten 10 lang (parallel X), 4 Kanten 30 hoch (Z), 4 Kanten längs Y fallen zu Punkten
    let degenerate = strokes.iter().filter(|s| s.is_degenerate_2d(&basis, 1e-9)).count();
    assert_eq!(degenerate, 4);
    let total = total_len(&strokes, &basis);
    assert!((total - (4.0 * 10.0 + 4.0 * 30.0)).abs() < 1e-6, "{total}");
}

#[test]
fn cylinder_has_circle_edges_and_no_seam_stroke() {
    let s = cylinder_solid(5.0, 12.0);
    let b = bodies(&s);
    let strokes = edge_strokes(&b, &Basis::of(ViewKind::Top), extent_of(&b)).unwrap();
    // zwei Kreiskanten; die Naht des Mantels ist keine Strichkante
    assert_eq!(strokes.len(), 2);
    assert!(strokes.iter().all(|s| matches!(s.curve, planora_kernel::Curve::Circle { .. })));
}

#[test]
fn top_view_circle_projects_to_circle_length() {
    let s = cylinder_solid(5.0, 12.0);
    let b = bodies(&s);
    let basis = Basis::of(ViewKind::Top);
    let strokes = edge_strokes(&b, &basis, extent_of(&b)).unwrap();
    let total = total_len(&strokes, &basis);
    assert!((total - 2.0 * 2.0 * std::f64::consts::PI * 5.0).abs() < 1e-6);
}

#[test]
fn filleted_box_does_not_draw_tangent_edges() {
    // Quader mit einer verrundeten Kante: die beiden Tangentenkanten (Rundung ↔ Seitenflächen) sind glatt und entfallen.
    let s = box_with_one_rounded_edge(20.0, 20.0, 20.0, 3.0);
    let b = bodies(&s);
    let all = edge_strokes(&b, &Basis::of(ViewKind::Front), extent_of(&b)).unwrap();
    let smooth = count_smooth_edges(&s);          // eigener Prüfer: Winkel der Flächennormalen an der Kante
    assert_eq!(all.len(), s.edges.iter().count() - smooth - count_seam_edges(&s));
    assert!(smooth >= 2);
}

#[test]
fn extent_scales_with_body() {
    let big = box_solid(1.0e5, 2.0e5, 3.0e5);
    let small = box_solid(0.1, 0.2, 0.3);
    assert!((extent_of(&bodies(&big)) - 3.0e5).abs() < 1e-3);
    assert!((extent_of(&bodies(&small)) - 0.3).abs() < 1e-12);
}
```
  (`box_with_one_rounded_edge`, `count_smooth_edges`, `count_seam_edges` in `tests/common/model.rs`: Rundung über `planora_kernel::blend` wie in `crates/planora-kernel/tests/blend_api.rs`; die Zähler laufen **ohne** die Bibliothek: Normalen über `Surface::normal` an 5 Stellen der Kante.)

- [ ] **Step 3: Run → FAIL.**
- [ ] **Step 4: Implementieren.** `extent_of`: Eckpunkte aller Körper plus je Kante 33 Probepunkte; Hüllbox; größte Seitenlänge, mindestens `1e-9`. `Stroke::bbox2`: 65 Probepunkte plus bei NURBS die projizierten Kontrollpunkte (Konvexe Hülle); Rand um `geom_tol(extent)` – der Strich kennt `extent` nicht, daher bekommt `bbox2` einen Parameter `pad: f64`. `deriv2`: analytisch (Line: `dir`; Circle: `r(−sin t·x + cos t·y)`; Ellipse; NURBS: `curve.derivs(t,1)[1]` mit `nurbs_param`-Faltung über die öffentliche `Curve::point`-Differenz **nicht** nötig, wenn `t` im Bereich bleibt), jeweils projiziert mit `ex`, `ey`.
- [ ] **Step 5: Run → PASS**, clippy, fmt.
- [ ] **Step 6: Commit** — `feat: Striche aus Körperkanten mit Naht- und Glattkanten-Ausschluss`.

---

### Task 3: 2D-Schnitte der Striche (Teilpunkte)

**Files:**
- Create: `crates/planora-drawing/src/isect.rs`, `tests/stage3_isect.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `Stroke`, `Basis`, `limits`, `Progress`.
- Produces (`isect.rs`):
```rust
/// Eine Kreuzung zweier Striche: Parameter auf a und b, Punkt, und `transversal` (false = Berührung).
#[derive(Clone, Copy, Debug)]
pub struct Cross { pub ta: f64, pub tb: f64, pub p: DVec2, pub transversal: bool }
pub fn stroke_cross(a: &Stroke, b: &Stroke, basis: &Basis, extent: f64) -> Result<Vec<Cross>, DrawError>;
/// Für jeden Strich die sortierten, eindeutigen inneren Teilparameter (t0 < t < t1, Abstand ≥ BISECT_TOL·Breite voneinander und vom Rand).
pub fn split_params(strokes: &[Stroke], basis: &Basis, extent: f64, progress: &Progress) -> Result<Vec<Vec<f64>>, DrawError>;
```
  `split_params` sammelt je Strich: (1) alle **transversalen** Kreuzungen mit allen anderen Strichen (Hüllbox-Vorauswahl über Sortierung nach `bbox2.min.x`, Sweep), (2) die Parameter, an denen **Endpunkte anderer Striche** auf diesem Strich liegen (2D-Abstand ≤ `geom_tol(extent)·16`; löst Überdeckung, T-Anschlüsse, Silhouetten, die an Kanten enden), (3) bei geschlossenen Parametern (Kreis, Ellipse, geschlossene NURBS) den Bereichsrand wie üblich nicht. Berührungen (doppelte Wurzel) teilen **nicht**.
  Lösungsweg `stroke_cross`: Schnellfälle Gerade–Gerade (geschlossen, auch parallel → keine Wurzeln; kollinear → keine Wurzeln, Überdeckung regeln die Endpunkte in (2)) und Gerade–Kreis/Ellipse (quadratische Gleichung im Parameter des Kegelschnitts, mit Bereichsprüfung ±`BISECT_TOL`). Alle übrigen Paare (Kegelschnitt–Kegelschnitt, jeder Spline): rekursive Zerlegung beider Parameterbereiche in Abschnitte, deren 2D-Hüllboxen (Probe an 9 Stellen + Kontrollpunkte bei NURBS) sich überlappen, bis Parameterbreite `< 1e-3` der Strichbreite oder Tiefe `MAX_SUBDIVISION_DEPTH`; je überlebendem Paar Newton auf `F(s,t) = pa(s) − pb(t)` (2×2 mit `deriv2`), höchstens 40 Schritte, Abbruch bei `|F| < geom_tol`; Wurzel akzeptieren, wenn `s,t` im Bereich ± `BISECT_TOL`; Duplikate (Abstand < `16·geom_tol` in 2D) vereinigen; `transversal = |cross(da, db)| > 1e-6·|da|·|db|`. Überlauf der Zerlegung über `MAX_PAIR_TESTS`-Zähler (geteilt, übergeben via `&Cell<u64>` im internen Zustand) → `DrawError::TooComplex`.
- [ ] **Step 1: Failing tests** (`tests/stage3_isect.rs`; Striche selbst bauen, `StrokeOrigin::Edge { body: 0, edge: i }`; Basis `Front` (x→x, y→z)):

```rust
mod common;
use common::model::*;
use planora_drawing::{basis::Basis, isect::*, model::ViewKind, stroke::*, DrawError};
use planora_exchange::Progress;
use planora_kernel::Curve;
use planora_math::{DVec3, Frame};

fn line(o: (f64, f64, f64), d: (f64, f64, f64), t1: f64, i: usize) -> Stroke {
    Stroke { curve: Curve::Line { origin: DVec3::new(o.0, o.1, o.2), dir: DVec3::new(d.0, d.1, d.2).normalize() }, t0: 0.0, t1, origin: StrokeOrigin::Edge { body: 0, edge: i } }
}

#[test]
fn two_crossing_lines_split_both() {
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 1.0), 20.0, 0);
    let b = line((10.0, 0.0, 0.0), (-1.0, 0.0, 1.0), 20.0, 1);
    let basis = Basis::of(ViewKind::Front);
    let c = stroke_cross(&a, &b, &basis, 30.0).unwrap();
    assert_eq!(c.len(), 1);
    assert!((c[0].p.x - 5.0).abs() < 1e-9 && (c[0].p.y - 5.0).abs() < 1e-9);
    assert!(c[0].transversal);
}

#[test]
fn lines_with_different_depth_still_cross_in_2d() {
    // y-Tiefe verschieden, 2D-Kreuzung bleibt
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 1.0), 20.0, 0);
    let b = line((10.0, 7.0, 0.0), (-1.0, 0.0, 1.0), 20.0, 1);
    assert_eq!(stroke_cross(&a, &b, &Basis::of(ViewKind::Front), 30.0).unwrap().len(), 1);
}

#[test]
fn line_through_circle_crosses_twice() {
    let circ = Stroke { curve: Curve::Circle { frame: Frame::WORLD, radius: 5.0 }, t0: 0.0, t1: std::f64::consts::TAU, origin: StrokeOrigin::Edge { body: 0, edge: 2 } };
    // Kreis liegt in der XY-Ebene; von vorne (x, z) ist er die Strecke x ∈ [-5, 5] bei z = 0
    let a = line((-10.0, 0.0, -3.0), (1.0, 0.0, 0.0), 20.0, 3); // parallel, keine Kreuzung
    assert!(stroke_cross(&a, &circ, &Basis::of(ViewKind::Front), 20.0).unwrap().is_empty());
    // von oben (x, y) ist der Kreis ein Kreis; eine Gerade durch die Mitte kreuzt zweimal
    let l = line((-10.0, 0.0, 0.0), (1.0, 0.0, 0.0), 20.0, 4);
    let c = stroke_cross(&l, &circ, &Basis::of(ViewKind::Top), 20.0).unwrap();
    assert_eq!(c.len(), 2);
}

#[test]
fn tangent_line_to_circle_does_not_split() {
    let circ = Stroke { curve: Curve::Circle { frame: Frame::WORLD, radius: 5.0 }, t0: 0.0, t1: std::f64::consts::TAU, origin: StrokeOrigin::Edge { body: 0, edge: 5 } };
    let l = line((-10.0, 5.0, 0.0), (1.0, 0.0, 0.0), 20.0, 6);
    let basis = Basis::of(ViewKind::Top);
    let strokes = vec![l, circ];
    let sp = split_params(&strokes, &basis, 20.0, &Progress::new()).unwrap();
    assert!(sp[0].is_empty() && sp[1].is_empty(), "{sp:?}");
}

#[test]
fn endpoint_on_other_stroke_splits_it() {
    // T-Anschluss: Gerade b endet mitten auf a
    let a = line((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 20.0, 0);
    let b = line((8.0, 0.0, 0.0), (0.0, 0.0, 1.0), 10.0, 1);
    let sp = split_params(&[a, b], &Basis::of(ViewKind::Front), 20.0, &Progress::new()).unwrap();
    assert_eq!(sp[0].len(), 1);
    assert!((sp[0][0] - 8.0).abs() < 1e-9);
    assert!(sp[1].is_empty());
}

#[test]
fn ellipse_ellipse_crossings_match_brute_force() {
    // Zwei verdrehte Kreise, von schräg (Isometrisch) projiziert: Ellipsen. Gegenprüfung: dichte Abtastung zählt Vorzeichenwechsel von Abstand.
    let c1 = Stroke { curve: Curve::Circle { frame: Frame::WORLD, radius: 6.0 }, t0: 0.0, t1: std::f64::consts::TAU, origin: StrokeOrigin::Edge { body: 0, edge: 0 } };
    let f2 = Frame { origin: DVec3::new(4.0, 0.0, 0.0), ..Frame::WORLD };
    let c2 = Stroke { curve: Curve::Circle { frame: f2, radius: 6.0 }, t0: 0.0, t1: std::f64::consts::TAU, origin: StrokeOrigin::Edge { body: 0, edge: 1 } };
    let basis = Basis::of(ViewKind::Isometric);
    let got = stroke_cross(&c1, &c2, &basis, 20.0).unwrap().iter().filter(|c| c.transversal).count();
    let want = brute_force_cross_count(&c1, &c2, &basis, 20_000);   // in tests/common/model.rs
    assert_eq!(got, want);
}

#[test]
fn spline_crossing_matches_brute_force() {
    // Kubische Kurve durch (0,0,0), (5,0,12), (10,0,-4), (20,0,10) (Kontrollpunkte, offener Knotenvektor, Grad 3) gegen die Gerade z = 3 (x von -5 bis 25)
    let curve = Curve::nurbs(cubic_through_controls(&[(0.0, 0.0, 0.0), (5.0, 0.0, 12.0), (10.0, 0.0, -4.0), (20.0, 0.0, 10.0)]));
    let (t0, t1) = curve.domain().unwrap();
    let a = Stroke { curve, t0, t1, origin: StrokeOrigin::Edge { body: 0, edge: 7 } };
    let b = line((-5.0, 0.0, 3.0), (1.0, 0.0, 0.0), 30.0, 8);
    let basis = Basis::of(ViewKind::Front);
    let got = stroke_cross(&a, &b, &basis, 30.0).unwrap().iter().filter(|c| c.transversal).count();
    assert_eq!(got, brute_force_cross_count(&a, &b, &basis, 20_000));
    assert!(got >= 2);
}

#[test]
fn cost_limit_reports_too_complex() {
    // Mehr als MAX_STROKES (20 000) Striche → TooComplex.
    let strokes: Vec<Stroke> = (0..25_001).map(|i| line((i as f64, 0.0, 0.0), (0.0, 0.0, 1.0), 1.0, i)).collect();
    let err = split_params(&strokes, &Basis::of(ViewKind::Front), 30_000.0, &Progress::new()).unwrap_err();
    assert!(matches!(err, DrawError::TooComplex(_)));
}
```
  `cubic_through_controls` (offener Knotenvektor `[0,0,0,0,1,1,1,1]`, Gewichte 1, `NurbsCurve::new`, Signatur in `crates/planora-math/src/nurbs/curve.rs:26`) und `brute_force_cross_count` (Abtastung beider Striche mit je 20 000 Punkten, Zählen der Schnittpunkte der beiden Polylinien durch Streckenpaar-Test) stehen in `tests/common/model.rs`.
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** (a) Berührungen als transversal zählen → `tangent_line_to_circle_does_not_split` rot; (b) Endpunkt-Teilung weglassen → `endpoint_on_other_stroke_splits_it` rot.
- [ ] **Step 6: Commit** — `feat: 2D-Schnitte der Striche und Teilpunkte`.

---

### Task 4: Umriss-Striche (Zylinder, Kegel, Kugel exakt)

**Files:**
- Create: `crates/planora-drawing/src/silhouette.rs`, `tests/stage4_silhouette.rs`
- Modify: `src/lib.rs`, `tests/common/model.rs` (Kegel, Torus, Kugel ergänzen)

**Interfaces:**
- Consumes: `Stroke`, `Basis`, `planora_kernel::boolean::{face_geom, point_in_face, Containment, FaceGeom}`, `Tolerance`.
- Produces: `pub fn silhouette_strokes(bodies: &[DrawBody], basis: &Basis, extent: f64, progress: &Progress) -> Result<Vec<Stroke>, DrawError>`. Je Fläche Umriss nach Art:
  - **Plane:** keiner.
  - **Cylinder** (Rahmen `f`, Radius `r`; Achse `a = f.z`): ist `|a·d| ≥ 1 − 1e-12` (Blick entlang der Achse): keiner (der Zylinder erscheint als Kreis, die Randkreise tragen ihn). Sonst Mantellinien bei den Winkeln `u` mit `n(u)·d = 0`, wobei `n(u) = cos u·x + sin u·y`: `d_x cos u + d_y sin u = 0` mit `d_x = d·f.x`, `d_y = d·f.y` → `u₀ = atan2(d_y, d_x) ± π/2` (zwei Lösungen). Mantellinie: `Curve::Line { origin: f.origin + r·n(u), dir: a }` mit Parameter = Höhe `v`; Bereich `[v0, v1]` = Höhenbereich der Fläche (aus `FaceGeom.domain` falls `Some`, sonst aus den Randschleifen: min/max der `v`-Werte der Schleifenpunkte per `Surface::project`).
  - **Cone** (Rahmen, `radius`, `slope`; Radius an `v` = `radius + slope·v`): Mantellinien bei `u` mit `n(u)·d = 0` für die Kegelnormale. Die Normale an `(u,v)` ist `∝ (cos u·x + sin u·y) − slope·z` (Außennormale für `slope`-Vorzeichen beachten: nimm `Surface::normal(u,v)` des Kernels und löse `normal(u, v)·d = 0` numerisch nach `u` mit Vorzeichenwechsel-Suche auf 720 Stützstellen + Bisektion; die Normale hängt bei Kegeln **nicht** von `v` ab (auf `v=0` auswerten, außer an der Spitze)). Mantellinie: `Curve::Line` von `P(u, v0)` in Richtung `normalize(P(u, v1) − P(u, v0))`, Parameter = Länge.
  - **Sphere** (Rahmen, `r`): Großkreis senkrecht zu `d`: `Curve::Circle { frame: Frame { origin: f.origin, x: e1, y: e2, z: d }, radius: r }` mit `e1 ⟂ d`, `e2 = d × e1`.
  - **Torus/Nurbs:** in diesem Task `Ok` ohne Striche + **ausdrückliches Merkblatt-Fehlen**: wird in Task 7 ergänzt; bis dahin liefert `silhouette_strokes` für diese Flächen `Err(DrawError::Unsupported("Umriss von Torus/Freiform folgt".into()))`, damit Körper damit nicht still falsch gezeichnet werden.
  - **Zuschnitt auf die Fläche:** jedes Umrissstück wird auf das tatsächliche Flächengebiet beschnitten: Parameterbereich des Strichs an den Stellen teilen, an denen `point_in_face(&fg, p, &tol)` zwischen `Inside` und `Outside` wechselt (Abtastung 256 Punkte je Strich, Wechselstellen per Bisektion auf `BISECT_TOL` verfeinert; `Boundary` zählt als Innen); behalten werden die `Inside`/`Boundary`-Abschnitte. Der Kreis der Kugel kann dabei mehrere Bögen ergeben; ein Kreis, der ganz drinnen liegt, bleibt voll (`t0 = 0`, `t1 = TAU`).
  - **Zusammenfallen mit Kanten:** Umrisslinie, die genau auf einer Randkante der Fläche liegt (Zylinder mit Mantellinie als Naht, wenn die Naht zufällig auf dem Umriss liegt): bleibt ein eigener Strich; die Doppelung beseitigt Task 8 (F6).
  - `StrokeOrigin::Silhouette { body, face }` mit dem Index der Fläche in `solid.faces`.
- [ ] **Step 1: Failing tests** (`tests/stage4_silhouette.rs`):

```rust
mod common;
use common::model::*;
use planora_drawing::{DrawBody, basis::Basis, model::ViewKind, silhouette::*, stroke::*};
use planora_exchange::Progress;

fn lines_of(s: &planora_kernel::Solid, kind: ViewKind) -> Vec<Stroke> {
    let b = vec![DrawBody { name: "K".into(), solid: s }];
    silhouette_strokes(&b, &Basis::of(kind), extent_of(&b), &Progress::new()).unwrap()
}

#[test]
fn upright_cylinder_front_has_two_silhouette_lines() {
    let s = cylinder_solid(5.0, 12.0);
    let st = lines_of(&s, ViewKind::Front);
    assert_eq!(st.len(), 2);
    let basis = Basis::of(ViewKind::Front);
    // Mantellinien bei x = ±5, z von 0 bis 12
    let mut xs: Vec<f64> = st.iter().map(|s| s.point2(&basis, (s.t0 + s.t1) / 2.0).x).collect();
    xs.sort_by(f64::total_cmp);
    assert!((xs[0] + 5.0).abs() < 1e-9 && (xs[1] - 5.0).abs() < 1e-9);
    for s in &st { assert!((s.point(s.t1) - s.point(s.t0)).length() - 12.0 < 1e-9); }
}

#[test]
fn upright_cylinder_top_has_no_silhouette() {
    assert!(lines_of(&cylinder_solid(5.0, 12.0), ViewKind::Top).is_empty());
}

#[test]
fn tilted_view_of_cylinder_has_two_lines_at_formula_positions() {
    // Isometrisch: Mantellinien im Abstand r senkrecht zur Projektion der Achse
    let s = cylinder_solid(5.0, 12.0);
    let basis = Basis::of(ViewKind::Isometric);
    let st = lines_of(&s, ViewKind::Isometric);
    assert_eq!(st.len(), 2);
    for l in &st {
        let a = l.point2(&basis, l.t0);
        let b = l.point2(&basis, l.t1);
        let axis2 = planora_math::DVec2::new(basis.ex.z, basis.ey.z).normalize(); // projizierte Z-Achse
        let normal = planora_math::DVec2::new(-axis2.y, axis2.x);
        assert!((a - b).normalize().perp_dot(axis2).abs() < 1e-9);       // parallel zur Achse
        let off = a.dot(normal).abs();                                    // Abstand zur Achse durch den Ursprung
        assert!((off - 5.0).abs() < 1e-9);
    }
}

#[test]
fn sphere_has_one_great_circle_in_every_view() {
    let s = sphere_solid(7.0);
    for k in planora_drawing::model::ViewKind::ALL {
        let st = lines_of(&s, k);
        let total: f64 = st.iter().map(|x| (x.t1 - x.t0) * 7.0).sum();
        assert!((total - 2.0 * std::f64::consts::PI * 7.0).abs() < 1e-6, "{k:?}: {total}");
    }
}

#[test]
fn cone_has_two_lines_from_the_side_and_none_from_the_axis() {
    let s = cone_solid(6.0, 2.0, 10.0);   // Radius unten 6, oben 2, Höhe 10 (Kegelstumpf)
    assert_eq!(lines_of(&s, ViewKind::Front).len(), 2);
    assert!(lines_of(&s, ViewKind::Top).is_empty());
}

#[test]
fn half_cylinder_silhouette_is_trimmed_to_the_face() {
    // Halbzylinder: Umriss nur dort, wo `u` im Winkelbereich der Fläche liegt (siehe Hinweis unten).
    let s = half_cylinder_solid(5.0, 12.0);
    assert_eq!(lines_of(&s, ViewKind::Front).len(), 2);
    assert_eq!(lines_of(&s, ViewKind::Left).len(), 1);
}

#[test]
fn torus_and_freeform_report_unsupported_until_task_7() {
    let s = torus_solid(10.0, 3.0);
    let b = vec![DrawBody { name: "T".into(), solid: &s }];
    assert!(silhouette_strokes(&b, &Basis::of(ViewKind::Front), 30.0, &Progress::new()).is_err());
}
```
  Hinweis für den Halbzylinder-Test: `half_cylinder_solid` ist ein Zylinder mit Winkelbereich `u ∈ [0, π]` (Halbkreis-Profil in der Ebene `y = 0` offen nach −Y, Rundung nach +Y). Von vorne (`d = +Y`, Auge bei −Y) liegen die Umrisswinkel `u = 0` und `u = π` genau auf den beiden Randkanten der Rundung: beide Umrissstücke liegen auf dem **Rand** der Fläche (`Boundary` zählt als innen), es entstehen also **zwei** Umriss-Striche, die mit Randkanten zusammenfallen (Task 8 vereint sie). Der Test erwartet daher `2`. Von der Seite (`ViewKind::Left`, `d = +X`) liegen die Umrisswinkel `u = ±π/2`: nur `u = π/2` liegt im Gebiet `[0, π]`, erwartet `1`. (Wer beim Ausrechnen auf andere Zahlen kommt, meldet das als Frage statt den Test anzupassen.)
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Zuschnitt (`point_in_face`) weglassen → Halbzylinder-Test rot; Vorzeichen von `±π/2` vertauschen auf nur eine Lösung → Zylinder-Tests rot.
- [ ] **Step 6: Commit** — `feat: Umrisslinien von Zylinder, Kegel und Kugel`.

---

### Task 5: Strahl gegen Flächen (Treffer mit Randnähe)

**Files:**
- Create: `crates/planora-drawing/src/raycast.rs`, `tests/stage5_ray.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `FaceGeom`, `face_geom`, `point_in_face`, `Containment`, `Surface`, `Tolerance`, `NurbsSurfaceData::ray_hits_checked` (über `Surface::Nurbs(d)`; die exakte Methode auf `d` und ihre Rückgabe `(u, v, s)` in `crates/planora-kernel/src/nurbs_surface/ray.rs` ablesen).
- Produces (`raycast.rs`):
```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitClass { Inside, Boundary }
#[derive(Clone, Copy, Debug)]
pub struct Hit { pub s: f64, pub body: usize, pub face: usize, pub class: HitClass }
/// Alle Flächen aller Körper, vorbereitet für viele Strahlen (FaceGeom, Hüllbox, Randpolygone einmal berechnet).
pub struct Scene { /* privat */ }
impl Scene {
    pub fn new(bodies: &[DrawBody], extent: f64, progress: &Progress) -> Result<Scene, DrawError>;
    /// Alle Treffer von `origin + s·dir` mit `s > min_s` (dir normiert). Treffer auf dem Rand einer Fläche heißen `Boundary`.
    /// `Err(AmbiguousVisibility)` wird NICHT hier erzeugt; unsichere Strahlen liefern `Ray::Unsafe`.
    pub fn cast(&self, origin: DVec3, dir: DVec3, min_s: f64) -> Ray;
}
pub enum Ray { Hits(Vec<Hit>), Unsafe(&'static str) }
```
  `Unsafe` mit kurzem Grund, wenn: Strahl liegt in der Ebene einer Planfläche (`|n·dir| < 1e-9` **und** `|n·(origin − o)| < OWN_EPS`-Ordnung 1e-6·extent → Strahl in der Ebene; Planflächen, die der Strahl nur parallel und außerhalb ihrer Ebene verfehlt, sind harmlos), Zylinder/Kegel mit Strahl fast parallel zur Mantelfläche und Diskriminante ≈ 0 (|disc| < 1e-12·scale²) bei `s > min_s`, `ray_hits_checked` liefert `None`, Treffer exakt an `Boundary` mit `s > min_s`… (Boundary-Treffer sind **keine** Unsicherheit der Fläche, aber der Aufrufer behandelt sie als unsicher, Task 6).
  Analytische Schnitte: Ebene (`s = n·(o − P)/(n·dir)`), Zylinder, Kegel (quadratisch im lokalen Rahmen `to_local`), Kugel (quadratisch), Torus (Quartik: lokal; löse mit der **Wurzelsuche des Kernels**? nein – eigenes Verfahren: Vorzeichenwechsel-Suche der impliziten Funktion `F(s) = Surface::implicit(P(s))` auf 2048 Stützstellen über `[min_s, reach]` + Bisektion/Newton; Doppelwurzeln, bei denen `F` nur berührt, gelten als `Unsafe("Strahl berührt Torus")` wenn `min |F|` unter `1e-9·r²` bleibt), Freiform (`ray_hits_checked`). `reach` = Durchmesser der Hüllbox aller Körper + `extent` (der Strahl zählt nur vor dem Auge innerhalb der Szene).
  Für jeden Schnittpunkt `P(s)`: `class = point_in_face(&fg, P, &tol)` mit `tol = Tolerance { linear: geom_tol(extent)·16, ..Default }` — `Outside` verwirft den Treffer, `Boundary` → `HitClass::Boundary`, `Inside` → `HitClass::Inside`.
  Leistung: je Fläche Hüllbox-Vorabtest des Strahls (Slab-Test gegen die Hüllbox der Fläche aus den Randschleifenpunkten + Probe, aufgebläht um `geom_tol·16`); `Scene::new` zählt `MAX_RAY_TESTS` nicht, der Zähler läuft in Task 6.
- [ ] **Step 1: Failing tests** (`tests/stage5_ray.rs`):

```rust
mod common;
use common::model::*;
use planora_drawing::{DrawBody, raycast::*};
use planora_exchange::Progress;
use planora_math::DVec3;

fn scene(s: &planora_kernel::Solid) -> Scene {
    let b = vec![DrawBody { name: "K".into(), solid: s }];
    Scene::new(&b, 50.0, &Progress::new()).unwrap()
}

#[test]
fn ray_through_box_hits_two_faces_inside() {
    let s = box_solid(10.0, 20.0, 30.0);
    let sc = scene(&s);
    match sc.cast(DVec3::new(5.0, -5.0, 15.0), DVec3::Y, 0.0) {
        Ray::Hits(h) => {
            assert_eq!(h.len(), 2);
            assert!((h[0].s - 5.0).abs() < 1e-9 && (h[1].s - 25.0).abs() < 1e-9);
            assert!(h.iter().all(|x| x.class == HitClass::Inside));
        }
        Ray::Unsafe(r) => panic!("{r}"),
    }
}

#[test]
fn ray_missing_the_box_has_no_hits() {
    let s = box_solid(10.0, 20.0, 30.0);
    assert!(matches!(scene(&s).cast(DVec3::new(50.0, -5.0, 15.0), DVec3::Y, 0.0), Ray::Hits(h) if h.is_empty()));
}

#[test]
fn ray_through_an_edge_reports_boundary() {
    let s = box_solid(10.0, 20.0, 30.0);
    match scene(&s).cast(DVec3::new(0.0, -5.0, 15.0), DVec3::Y, 0.0) {
        Ray::Hits(h) => assert!(h.iter().any(|x| x.class == HitClass::Boundary)),
        Ray::Unsafe(_) => {}  // beides ist zulässig, nie „Inside“ ohne Rand
    }
}

#[test]
fn ray_in_the_plane_of_a_face_is_unsafe() {
    let s = box_solid(10.0, 20.0, 30.0);
    // Strahl entlang +Y genau in der Ebene x = 0 (Fläche x = 0 reicht von y 0..20, z 0..30)
    assert!(matches!(scene(&s).cast(DVec3::new(0.0, -5.0, 15.0), DVec3::Y, 0.0), Ray::Unsafe(_)));
    // dieselbe Richtung weit neben der Box ist harmlos
    assert!(matches!(scene(&s).cast(DVec3::new(-3.0, -5.0, 15.0), DVec3::Y, 0.0), Ray::Hits(h) if h.is_empty()));
}

#[test]
fn min_s_skips_the_own_surface() {
    let s = box_solid(10.0, 20.0, 30.0);
    // Start genau auf der Vorderfläche y=0, nach hinten: mit min_s = 1e-6 nur die hintere Fläche
    match scene(&s).cast(DVec3::new(5.0, 0.0, 15.0), DVec3::Y, 1e-6) {
        Ray::Hits(h) => { assert_eq!(h.len(), 1); assert!((h[0].s - 20.0).abs() < 1e-9); }
        Ray::Unsafe(r) => panic!("{r}"),
    }
}

fn hits(sc: &Scene, o: DVec3, d: DVec3) -> Vec<f64> {
    match sc.cast(o, d, 0.0) { Ray::Hits(h) => h.iter().map(|x| x.s).collect(), Ray::Unsafe(r) => panic!("{r}") }
}

#[test]
fn quadric_hit_distances_match_formulas() {
    // Hilfskörper (tests/common/model.rs): Achse Z, Mittelpunkt des Bodens bzw. der Kugel im Ursprung.
    // Zylinder r=5, h=12: Strahl (-10,0,6) → +X trifft x=-5 und x=+5 → s = 5 und 15
    let c = cylinder_solid(5.0, 12.0);
    let h = hits(&scene(&c), DVec3::new(-10.0, 0.0, 6.0), DVec3::X);
    assert_eq!(h.len(), 2);
    assert!((h[0] - 5.0).abs() < 1e-9 && (h[1] - 15.0).abs() < 1e-9);
    // Kugel r=7 um den Ursprung: (-10,0,0) → +X: x=-7 und x=+7 → s = 3 und 17
    let k = sphere_solid(7.0);
    let h = hits(&scene(&k), DVec3::new(-10.0, 0.0, 0.0), DVec3::X);
    assert!((h[0] - 3.0).abs() < 1e-9 && (h[1] - 17.0).abs() < 1e-9);
    // Kegelstumpf cone_solid(r0=6, r1=2, h=10): Radius bei z=5 ist 6 - 4*5/10 = 4 → s = 6 und 14
    let kg = cone_solid(6.0, 2.0, 10.0);
    let h = hits(&scene(&kg), DVec3::new(-10.0, 0.0, 5.0), DVec3::X);
    assert!((h[0] - 6.0).abs() < 1e-9 && (h[1] - 14.0).abs() < 1e-9);
    // Torus torus_solid(major=10, minor=3), Achse Z: Strahl (-20,0,0) → +X trifft x=-13,-7,7,13 → s = 7,13,27,33
    let t = torus_solid(10.0, 3.0);
    let h = hits(&scene(&t), DVec3::new(-20.0, 0.0, 0.0), DVec3::X);
    assert_eq!(h.len(), 4);
    for (g, w) in h.iter().zip([7.0, 13.0, 27.0, 33.0]) { assert!((g - w).abs() < 1e-7, "{g} vs {w}"); }
}

#[test]
fn freeform_face_ray_matches_surface_point() {
    // `nurbs_slab_solid()` (tests/common/model.rs, gebaut wie die Fixtur in planora-kernel/tests/nurbs_ray.rs): Platte mit einer Freiformfläche oben.
    // Wähle (u,v) in der Mitte, p = Fläche(u,v), n = Normale; Strahl von p + 5n in Richtung -n trifft die Fläche bei s = 5.
    let (solid, face_index) = nurbs_slab_solid();
    let (p, n) = nurbs_slab_center_point_and_normal(&solid, face_index);
    let h = hits(&scene(&solid), p + n * 5.0, -n);
    assert!(h.iter().any(|s| (s - 5.0).abs() < 1e-8), "{h:?}");
}

#[test]
fn hits_are_sorted_by_s() {
    let c = cylinder_solid(5.0, 12.0);
    let th = 0.3_f64;
    let d = DVec3::new(th.cos(), th.sin(), 0.0);
    let h = hits(&scene(&c), DVec3::new(-10.0, -1.0, 6.0), d);
    assert_eq!(h.len(), 2);
    assert!(h[0] < h[1]);
}
```

- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** `point_in_face`-Prüfung weglassen → `ray_missing…`/Trefferzahl-Tests rot (Strahl neben der Fläche trifft die Ebene); `min_s` ignorieren → `min_s_skips_the_own_surface` rot.
- [ ] **Step 6: Commit** — `feat: Strahlprüfung gegen Flächen mit Randklassifikation`.

---

### Task 6: Sichtbarkeit und `compute_view` für analytische Körper

**Files:**
- Create: `crates/planora-drawing/src/visibility.rs`, `tests/common/{mesh_ref.rs,check.rs}`, `tests/stage6_visibility.rs`
- Modify: `src/lib.rs` (`compute_view`, `ViewLines`, `Piece`, `Visibility`), `src/stroke.rs` (`EdgeFaces`, `own_faces`), `src/raycast.rs` (Merkmal `Caster`), `tests/common/mod.rs`, `tests/common/model.rs`

**Interfaces:**
- Consumes: Aufgaben 1–5.
- Produces (`lib.rs`):
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Visibility { Visible, Hidden }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece { pub stroke: usize, pub t0: f64, pub t1: f64, pub vis: Visibility }
#[derive(Clone, Debug)]
pub struct ViewLines { pub kind: ViewKind, pub basis: Basis, pub extent: f64,
    pub strokes: Vec<Stroke>, pub pieces: Vec<Piece>,
    /// Hüllbox aller Stücke (sichtbar und verdeckt) in Ansichtskoordinaten, mm, ungeskaliert.
    pub bbox: ((f64, f64), (f64, f64)) }
pub fn compute_view(bodies: &[DrawBody], kind: ViewKind, progress: &Progress) -> Result<ViewLines, DrawError>;
```
  `compute_view`: (1) `NoBodies` bei leerer Liste; (2) `extent_of`; (3) `edge_strokes` + `silhouette_strokes`; Striche, die in dieser Ansicht zu einem Punkt entarten (`is_degenerate_2d(basis, 16·geom_tol)`), werden **verworfen** (sie tragen keine Linie; ihre Endpunkte sind Endpunkte der Nachbarn); `TooComplex` bei mehr als `MAX_STROKES` Strichen; (4) `split_params`; (5) Stücke je Strich zwischen aufeinanderfolgenden Teilparametern (ein geschlossener Strich ohne Teilpunkt bleibt **ein** Stück `[t0, t1]`); (6) `classify_range` je Stück; (7) `ViewLines` mit `bbox`.
- Produces (`raycast.rs`): `pub trait Caster { fn cast(&self, origin: DVec3, dir: DVec3, min_s: f64) -> Ray; }` und `impl Caster for Scene`.
- Produces (`stroke.rs`): `pub struct EdgeFaces` (einmal je Körper aufgebaut: Kantenindex → Flächenindizes) mit `EdgeFaces::new(solid) -> EdgeFaces`, `pub fn own_faces(&self, stroke: &Stroke) -> Vec<usize>` (`Edge` → anliegende Flächen, `Silhouette { face, .. }` → `[face]`).
- Produces (`visibility.rs`):
```rust
pub const TILT: f64 = 1e-5;                 // Kippwinkel der Prüfstrahlen (rad)
pub const TILT_DIRECTIONS: usize = 8;       // gleichverteilt um die Blickrichtung
pub const SAMPLE_FRACTIONS: [f64; 3] = [0.25, 0.5, 0.75];
pub const SHIFTS: [f64; 6] = [0.0123, -0.0123, 0.037, -0.037, 0.071, -0.071];   // Ausweichstellen (Bruchteil der Stückbreite)
/// Ergebnis an einer Stelle: Some(true) = sichtbar, Some(false) = verdeckt, None = unbrauchbar.
pub fn sample_visibility(caster: &dyn Caster, basis: &Basis, p: DVec3, own: &[usize], body: usize,
                         extent: f64) -> Option<bool>;
/// Beurteilt das Stück [ta, tb] des Strichs; teilt per Bisektion, wenn die Stellen verschieden urteilen.
pub fn classify_range(caster: &dyn Caster, basis: &Basis, stroke: &Stroke, own: &[usize], body: usize,
                      ta: f64, tb: f64, extent: f64, name: &str, counter: &Cell<u64>)
                      -> Result<Vec<(f64, f64, Visibility)>, DrawError>;
```
  **Kippabstimmung (Festlegung F9):** An einer Stelle `P` werden 8 Strahlen `dir_k = normalize(eye + TILT·(cos φ_k·ex + sin φ_k·ey))`, `φ_k = k·45°` mit `min_s = SELF_EPS` geworfen (`eye = basis.eye_dir()`, Strahlen laufen zum Auge). Je Strahl: `Hits(h)`: Treffer auf eigenen Flächen (`own`) mit `s ≤ OWN_EPS` entfallen; bleibt ein Treffer der Klasse `Boundary`, ist der Strahl **unbrauchbar**; sonst ist er **verdeckt**, wenn ein `Inside`-Treffer bleibt, sonst **frei**. `Unsafe` → unbrauchbar. Ergebnis der Stelle: gibt es einen freien Strahl → `Some(true)` (**sichtbar**); sonst gibt es mindestens einen verdeckten → `Some(false)`; sonst `None`. (Sinn: Eine Linie, die genau auf dem Umriss einer Fläche liegt – oder von der Seite betrachtet eine Randkante einer senkrecht stehenden Fläche –, ist nach Zeichenkonvention sichtbar; echt verdeckte Linien sind unter jedem Kippstrahl verdeckt. Unbrauchbare Strahlen werden übergangen.) `TILT` darf der Implementierer im Bereich `1e-7 … 1e-4` anhand der Tests kalibrieren und trägt den gewählten Wert samt Begründung ins Protokoll.
  `classify_range`: Stützstellen `t = ta + f·(tb − ta)` für `f ∈ SAMPLE_FRACTIONS`; jede unbrauchbare Stelle wird nacheinander durch `f + SHIFTS[i]` (innerhalb `(0,1)`) ersetzt, bis eine brauchbare gefunden ist; findet sich keine → `Err(AmbiguousVisibility(name))`. Alle drei Urteile gleich → `[(ta, tb, vis)]`. Sonst: Mitte `tm = (ta+tb)/2`, rekursiv für beide Hälften; Rekursion endet bei `tb − ta < BISECT_TOL·stroke.param_len()` oder Tiefe `MAX_SUBDIVISION_DEPTH` (dann Urteil der mittleren Stelle für die ganze Hälfte). Benachbarte Ergebnisse gleicher Sichtbarkeit werden zusammengefasst. `counter` zählt Strahlwürfe (Summe der `cast`-Aufrufe); über `MAX_RAY_TESTS` → `TooComplex`.
- Produces (Prüfwerkzeuge, nur Tests):
  - `tests/common/mesh_ref.rs`: `ref_visibility(mesh: &Mesh, basis, p: DVec3, extent) -> Option<bool>` – **eigene** Möller–Trumbore-Prüfung gegen das Netz aus `planora_mesh::tessellate` (feine `TessellationOptions`, Sehnentoleranz `1e-3·extent`), mit eigener Kippabstimmung (45°-Schritte, `TILT_REF = 1e-4`, Treffer nur bei `s > 5·chord_tol`); `feature_edges(mesh, basis) -> Vec<(DVec2, DVec2)>`: Netzkanten, deren zwei Dreiecke einen Kantenwinkel von mehr als 20° bilden **oder** eine Vorderseite und eine Rückseite (`n·d` verschiedenen Vorzeichens) haben, projiziert; Netzpunkte werden über quantisierte Koordinaten (`1e-7·extent`) verschweißt.
  - `tests/common/check.rs`: `check_view(solids, kind) -> Report`, das prüft: (a) **Abdeckung:** je Strich überdecken die Stücke `[t0, t1]` lückenlos und ohne Überlappung (Sortierung nach `t0`; geschlossene Striche ebenso); (b) **keine Stücke** mit 2D-Länge unter `64·geom_tol` (vor Aufgabe 8 als Warnung gezählt, danach Fehler); (c) **Vollständigkeit:** jede projizierte Merkmalskante des Netzes (Stichprobe: Mittelpunkt und Viertelpunkte der Kante) liegt innerhalb `4·chord_tol` einer Stückprobe (Abstand Punkt–Stück per dichter Abtastung des Stücks, 64 Punkte); (d) **keine Phantomlinien:** jeder Stückmittelpunkt liegt innerhalb `4·chord_tol` einer projizierten Merkmalskante – **ausgenommen** Kanten glatter Übergänge, die die Bibliothek bewusst weglässt (hier kommt sie nicht vor, glatte Kanten tauchen nicht in den Stücken auf); (e) **Sichtbarkeit:** für jedes Stück die drei Stützpunkte; Abweichung von `ref_visibility` ist nur zulässig, wenn sich `ref_visibility` an Punkten der 2D-Umgebung (Ring mit Radius `4·chord_tol`, 8 Punkte, **auf dem selben 3D-Strich** nicht anwendbar – deshalb: dieselbe Strahlprüfung mit `p` um `±4·chord_tol` entlang der Strichtangente verschoben) ändert; sonst Fehler mit Strich und Stelle.
  - `tests/common/model.rs` ergänzt: `box_with_hole(a,b,c,hole_r, axis)` (Bohrung durch den Quader; Boolesche Differenz mit `planora_kernel::boolean::boolean`, Muster `planora-kernel/tests/boolean_basic.rs`), `l_bracket(…)`, `box_with_pocket(…)`, `cylinder_cross_hole(…)`, `sphere_on_box(…)`, `rounded_box(…)`.

- [ ] **Step 1: Failing tests** (`tests/stage6_visibility.rs`):

```rust
mod common;
use common::{check::*, model::*};
use planora_drawing::{DrawBody, Visibility, compute_view, model::ViewKind};
use planora_exchange::Progress;

fn view(s: &planora_kernel::Solid, k: ViewKind) -> planora_drawing::ViewLines {
    compute_view(&[DrawBody { name: "K".into(), solid: s }], k, &Progress::new()).unwrap()
}
fn len(v: &planora_drawing::ViewLines, vis: Visibility) -> f64 { piece_length_sum(v, vis) } // eigene Längensumme, tests/common/check.rs

#[test]
fn isometric_box_has_nine_visible_and_three_hidden_edges() {
    let (a, b, c) = (10.0, 20.0, 30.0);
    let v = view(&box_solid(a, b, c), ViewKind::Isometric);
    let k = (2.0_f64 / 3.0).sqrt();       // projizierte Länge je Achseneinheit
    assert!((len(&v, Visibility::Visible) - 3.0 * (a + b + c) * k).abs() < 1e-6);
    assert!((len(&v, Visibility::Hidden) - (a + b + c) * k).abs() < 1e-6);
}

#[test]
fn front_view_of_box_has_no_hidden_pieces() {
    let v = view(&box_solid(10.0, 20.0, 30.0), ViewKind::Front);
    assert_eq!(len(&v, Visibility::Hidden), 0.0);
    assert!(len(&v, Visibility::Visible) >= 2.0 * (10.0 + 30.0) - 1e-9);   // Rechteck (hintere Kopien dürfen dazukommen, Aufgabe 8 vereint sie)
}

#[test]
fn through_hole_along_x_shows_hidden_lines_from_the_front() {
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X);   // Bohrung längs X auf Höhe z = 15, y = 10
    let v = view(&s, ViewKind::Front);
    // verdeckt: zwei Mantellinien des Lochs bei z = 15 ± 5, je Länge 40
    assert!((len(&v, Visibility::Hidden) - 80.0).abs() < 1e-6, "{}", len(&v, Visibility::Hidden));
}

#[test]
fn through_hole_along_y_is_all_visible_from_the_front() {
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::Y);
    let v = view(&s, ViewKind::Front);
    // Lochkreis vorn sichtbar, hinterer Kreis fällt darauf (Aufgabe 8 vereint); Mantellinien sind von vorn Punkte
    assert!((len(&v, Visibility::Hidden)).abs() < 1e-6);
}

#[test]
fn mesh_reference_agrees_for_all_shapes_and_views() {
    let shapes: Vec<(&str, planora_kernel::Solid)> = vec![
        ("box", box_solid(10.0, 20.0, 30.0)),
        ("cylinder", cylinder_solid(5.0, 12.0)),
        ("sphere", sphere_solid(7.0)),
        ("cone", cone_solid(6.0, 2.0, 10.0)),
        ("hole_x", box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X)),
        ("hole_y", box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::Y)),
        ("l_bracket", l_bracket(40.0, 30.0, 20.0, 10.0)),
        ("pocket", box_with_pocket(40.0, 30.0, 20.0, 10.0, 8.0, 5.0)),
        ("cross_hole", cylinder_cross_hole(8.0, 30.0, 3.0)),
        ("sphere_on_box", sphere_on_box(20.0, 7.0)),
        ("rounded_box", rounded_box(20.0, 20.0, 20.0, 3.0)),
    ];
    for (name, s) in &shapes {
        for k in planora_drawing::model::ViewKind::ALL {
            let r = check_view(&[s], k);
            assert!(r.errors.is_empty(), "{name} {k:?}: {:?}", r.errors);
        }
    }
}

#[test]
fn scaled_and_shifted_bodies_give_the_same_classification() {
    // Ausdehnung 1e5 mm, weit vom Ursprung (1e6 mm) und 0,1 mm: gleiche Anzahl sichtbarer/verdeckter Stücke je Ansicht
    let base = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X);
    let want: Vec<(usize, usize)> = planora_drawing::model::ViewKind::ALL.iter().map(|k| counts(&view(&base, *k))).collect();
    for (f, shift) in [(2500.0, 0.0), (0.0025, 0.0), (1.0, 1.0e6)] {
        let s = transformed(&base, f, shift);          // tests/common/model.rs: uniform skaliert und verschoben
        for (i, k) in planora_drawing::model::ViewKind::ALL.iter().enumerate() {
            assert_eq!(counts(&view(&s, *k)), want[i], "factor {f} shift {shift} {k:?}");
        }
    }
}

#[test]
fn oblique_view_nearly_parallel_to_a_face_is_classified_or_named() {
    // Quader um 0,01° gedreht gegenüber der Vorderansicht (Fläche fast parallel zu d): entweder korrekt (Mesh-Referenz) oder AmbiguousVisibility, nie falsch.
    let s = rotated(&box_solid(10.0, 20.0, 30.0), planora_math::DVec3::Z, 0.01_f64.to_radians());
    match compute_view(&[DrawBody { name: "K".into(), solid: &s }], ViewKind::Front, &Progress::new()) {
        Ok(_) => assert!(check_view(&[&s], ViewKind::Front).errors.is_empty()),
        Err(planora_drawing::DrawError::AmbiguousVisibility(_)) => {}
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn missing_split_is_found_by_bisection() {
    // classify_range über einen Strich, der hinter der Kante einer Platte hervortritt; Übergang bei bekannter Stelle t*
    let (stroke, scene, t_star) = stroke_behind_plate_edge();       // tests/common/model.rs; t* aus der Geometrie berechnet
    let parts = planora_drawing::visibility::classify_range(&scene, &stroke.basis, &stroke.stroke, &[], 0, stroke.stroke.t0, stroke.stroke.t1, 100.0, "T", &std::cell::Cell::new(0)).unwrap();
    assert_eq!(parts.len(), 2);
    assert!((parts[0].1 - t_star).abs() < 1e-7);
    assert_ne!(parts[0].2, parts[1].2);
}

#[test]
fn all_rays_unsafe_names_the_stroke() {
    struct AlwaysUnsafe;
    impl planora_drawing::raycast::Caster for AlwaysUnsafe {
        fn cast(&self, _: planora_math::DVec3, _: planora_math::DVec3, _: f64) -> planora_drawing::raycast::Ray { planora_drawing::raycast::Ray::Unsafe("test") }
    }
    let st = line_stroke((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 10.0);
    let e = planora_drawing::visibility::classify_range(&AlwaysUnsafe, &planora_drawing::basis::Basis::of(ViewKind::Front), &st, &[], 0, 0.0, 10.0, 10.0, "Kante 3", &std::cell::Cell::new(0)).unwrap_err();
    assert!(matches!(e, planora_drawing::DrawError::AmbiguousVisibility(n) if n == "Kante 3"));
}

#[test]
fn empty_input_and_progress_cancel() {
    assert!(matches!(compute_view(&[], ViewKind::Front, &Progress::new()), Err(planora_drawing::DrawError::NoBodies)));
    let p = Progress::new(); p.cancel();
    let s = box_solid(1.0, 1.0, 1.0);
    assert!(matches!(compute_view(&[DrawBody { name: "K".into(), solid: &s }], ViewKind::Front, &p), Err(planora_drawing::DrawError::Cancelled)));
}
```
  (`counts`, `transformed`, `rotated`, `stroke_behind_plate_edge`, `line_stroke`, `piece_length_sum` in `tests/common`: eigene kleine Hilfen; `transformed` benutzt `planora_kernel`s öffentliche Transformation – `crates/planora-kernel/src/transform.rs` ist privat: dann `Solid` neu über Kernel-Konstruktoren mit skalierten Maßen und verschobenem Rahmen bauen, nicht über Kernelzugriff.)
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren** (Reihenfolge: `Caster`, `EdgeFaces`, `visibility.rs`, `compute_view`, dann die Prüfwerkzeuge). **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise** (Ergebnis im Bericht): (a) nur **einen** ungekippten Strahl statt der Kippabstimmung → `front_view_of_box…`/`mesh_reference_agrees…` rot oder `AmbiguousVisibility`; (b) „Boundary-Treffer zählen als verdeckt“ → `mesh_reference_agrees…` rot; (c) Bisektion durch „Urteil der Mitte für das ganze Stück“ ersetzen → `missing_split_is_found_by_bisection` rot; (d) `OWN_EPS` auf 0 → Silhouetten verdeckt (`mesh_reference_agrees…` rot).
- [ ] **Step 6: Commit** — `feat: Sichtbarkeit der Linienstücke mit Kippstrahlen, compute_view für analytische Körper`.

---

### Task 7: Umrisse von Torus und Freiformflächen

**Files:**
- Modify: `crates/planora-drawing/src/silhouette.rs`
- Create: `tests/stage7_freeform_silhouette.rs`
- Modify: `tests/common/model.rs` (Torusformel, Freiformkörper)

**Interfaces:**
- Consumes: Aufgabe 4 (`silhouette_strokes`, Zuschnitt), `planora_math::nurbs::NurbsCurve::fit(points, tol)`.
- Produces: `silhouette_strokes` liefert jetzt auch für `Torus` und `Nurbs`; die Vorläufer-Fehlermeldung aus Aufgabe 4 entfällt. Jeder Torus-/Freiform-Umriss ist ein `Curve::Nurbs(Arc<NurbsCurve>)` im 3D-Raum, an `SILHOUETTE_TOL = 1e-4` mm an die wahre Umrisskurve angepasst (Abstand der angepassten Kurve zur Fläche **und** Abweichung von `n·d = 0` unten geprüft).
  - **Torus (F5):** Rahmen `f`, `R = major`, `r = minor`; lokal `d_l = (d·f.x, d·f.y, d·f.z)`. Normale an `(u,v)`: `n = (cos v cos u, cos v sin u, sin v)` (lokal). Bedingung `d_l.x cos v cos u + d_l.y cos v sin u + d_l.z sin v = 0`. Für `|d_l.z| > 1e-9`: `v₁(u) = atan2(−(d_l.x cos u + d_l.y sin u), d_l.z)`, `v₂(u) = v₁(u) + π` — zwei geschlossene Zweige, in `u` von 0 bis `TAU` (Punkt `P(u, v) = f.origin + f.x (R + r cos v) cos u + f.y (R + r cos v) sin u + f.z r sin v`). Für `|d_l.z| ≤ 1e-9` (Blick in der Ebene des Torus): `cos v · (d_l.x cos u + d_l.y sin u) = 0` → die Kreise `v = ±π/2` (obere und untere Rohrkante, **Kreise** `Curve::Circle` mit Radius `R` und Mittelpunkt `f.origin ± r·f.z`) und die zwei Meridiankreise bei `u = atan2(d_l.y, d_l.x) ± π/2` (Kreise mit Radius `r`, Mitte `f.origin + R·n_u`, Ebene durch die Achse; `Curve::Circle` mit passendem Rahmen). Jeder Zweig wird in 721 Punkte (`u` gleichverteilt) abgetastet, dann **adaptiv** bis die Anpassung `SILHOUETTE_TOL` hält (Verdopplung der Punktzahl bis Abweichung zum Mittelpunkt zwischen den Stützpunkten ≤ `SILHOUETTE_TOL/2`, höchstens 8 Verdopplungen), `NurbsCurve::fit`. Zuschnitt auf das Flächengebiet wie in Aufgabe 4 (Wechselstellen per Bisektion in `u`; die Stücke werden als Teilbereiche des **Parameters der angepassten Kurve** geführt – daher Teilung der angepassten Kurve an `t` aus dem nächsten Punkt, nicht aus `u`).
  - **Freiform (`Nurbs`):** `g(u,v) = normal(u,v)·d`. Bereich = Flächen-Parameterbereich (`FaceGeom.domain` bzw. `NurbsSurfaceData::surface().domain()`). Raster `64×64` (je Bézier-Spanne mindestens 8 Teile je Richtung, höchstens 256×256), Vorzeichenwechsel von `g` an Rasterkanten suchen, Kurven nach **Marching-Squares-Verfahren** verketten, jeden Punkt per Newton auf `g = 0` in Richtung `∇g` verfeinern (höchstens 8 Schritte, `|g| < 1e-12`), dann `NurbsCurve::fit(punkte, SILHOUETTE_TOL)` je verketteter Kurve. Geschlossene Richtungen (Naht) und Pole: Verkettung über die Naht verbinden (Parameter falten); Pole (Normale nicht definiert) → `DrawError::Unsupported("Umriss an einem Pol der Freiformfläche")` statt Raten. Zuschnitt auf das Gebiet wie oben (`point_in_face`, 256 Proben je Kurve + Bisektion). Rechenaufwand über `MAX_PIECES`/`Progress` begrenzt; Überschreiten → `TooComplex`.
- [ ] **Step 1: Failing tests** (`tests/stage7_freeform_silhouette.rs`):

```rust
mod common;
use common::{check::*, model::*};
use planora_drawing::{DrawBody, basis::Basis, compute_view, model::ViewKind, silhouette::*, stroke::*};
use planora_exchange::Progress;

#[test]
fn torus_silhouette_matches_independent_formula_in_all_views() {
    let s = torus_solid(10.0, 3.0);
    for k in ViewKind::ALL {
        let basis = Basis::of(k);
        let b = vec![DrawBody { name: "T".into(), solid: &s }];
        let st = silhouette_strokes(&b, &basis, extent_of(&b), &Progress::new()).unwrap();
        let pts2: Vec<_> = st.iter().flat_map(|x| sample2(x, &basis, 400)).collect();
        // eigene Formel (tests/common/model.rs: torus_silhouette_points(R, r, d, 4000) -> Vec<DVec3>), projiziert
        let want: Vec<_> = torus_silhouette_points(10.0, 3.0, basis.d, 4000).iter().map(|p| basis.project(*p)).collect();
        assert!(hausdorff(&pts2, &want) <= 1.05e-4, "{k:?}: {}", hausdorff(&pts2, &want));
    }
}

#[test]
fn torus_views_agree_with_the_mesh_reference() {
    let s = torus_solid(10.0, 3.0);
    for k in ViewKind::ALL {
        let r = check_view(&[&s], k);
        assert!(r.errors.is_empty(), "{k:?}: {:?}", r.errors);
    }
}

#[test]
fn freeform_silhouette_stays_on_the_surface_and_satisfies_the_condition() {
    let (s, face) = nurbs_dome_solid();         // gewölbte NURBS-Fläche auf einem Sockel (Fixtur nach planora-kernel/tests/nurbs_geometry.rs)
    for k in [ViewKind::Front, ViewKind::Left, ViewKind::Isometric] {
        let basis = Basis::of(k);
        let b = vec![DrawBody { name: "D".into(), solid: &s }];
        let st = silhouette_strokes(&b, &basis, extent_of(&b), &Progress::new()).unwrap();
        assert!(!st.is_empty(), "{k:?}");
        for stroke in st.iter().filter(|x| matches!(x.origin, StrokeOrigin::Silhouette { face: f, .. } if f == face)) {
            for p in sample3(stroke, 200) {
                let (dist, g) = surface_residual(&s, face, p, basis.d);   // eigene Rechnung: nächster Flächenpunkt, |n·d| dort
                assert!(dist <= 1.2e-4, "Abstand {dist}");
                assert!(g.abs() <= 2e-3, "n·d = {g}");
            }
        }
    }
}

#[test]
fn freeform_views_agree_with_the_mesh_reference() {
    let (s, _) = nurbs_dome_solid();
    for k in ViewKind::ALL {
        let r = check_view(&[&s], k);
        assert!(r.errors.is_empty(), "{k:?}: {:?}", r.errors);
    }
}

#[test]
fn pole_of_a_freeform_face_is_named_not_guessed() {
    let s = nurbs_pole_solid();   // Freiformfläche mit entartetem Rand (Pol), Blick so, dass der Umriss durch den Pol läuft
    let r = compute_view(&[DrawBody { name: "P".into(), solid: &s }], ViewKind::Front, &Progress::new());
    assert!(matches!(r, Ok(_) | Err(planora_drawing::DrawError::Unsupported(_))));
}
```
  (`torus_silhouette_points`: eigene Umsetzung der Formel `v₁,v₂` – **nicht** der Bibliothekscode; `sample2/sample3/hausdorff/surface_residual`: kleine Hilfen in `tests/common`; `nurbs_dome_solid/nurbs_pole_solid`: Körper mit Freiformfläche aus den Kernel-Fixturen.)
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** (a) Torus-Zweig `v₂` weglassen → Hausdorff-Test rot; (b) `SILHOUETTE_TOL` auf `1e-2` → Hausdorff-/Abstandstests rot; (c) Zuschnitt weglassen → Vollständigkeits-/Phantomtest der Netzreferenz rot (Dome-Test).
- [ ] **Step 6: Commit** — `feat: Umrisse von Torus und Freiformflächen als angepasste Splines`.

---

### Task 8: Deckungsgleiche Stücke, Zusammenfassen, Härtung und Prüfreihe

**Files:**
- Modify: `crates/planora-drawing/src/{visibility.rs,lib.rs}` (Zusammenfassen), `tests/common/check.rs` (Warnung (b) wird Fehler)
- Create: `tests/stage8_merge_series.rs`

**Interfaces:**
- Produces: nach `compute_view` (Schritt 7) ein Schritt **Zusammenfassen** in `visibility.rs`: `pub fn merge_pieces(strokes: &[Stroke], pieces: Vec<Piece>, basis: &Basis, extent: f64) -> Vec<Piece>`:
  1. Aufeinanderfolgende Stücke desselben Strichs mit gleicher Sichtbarkeit und gleichem Berührungspunkt (`t1` des einen = `t0` des nächsten, bei geschlossenen Strichen auch Ende an Anfang) werden vereint.
  2. **Deckungsgleiche Stücke** (F6): zwei Stücke gelten als deckungsgleich, wenn ihre 2D-Mittelpunkte, ihre Endpunkte (in beliebiger Reihenfolge) und drei innere Stützpunkte (jeweils bei 1/4 bzw. 3/4 des Parameters) innerhalb `16·geom_tol` übereinstimmen **und** ihre Kurvenart (Gerade/Kegelschnitt/Spline) gleich ist; dann bleibt **ein** Stück, `Visible`, wenn eines davon `Visible` ist. Die Vorauswahl über Hüllboxen; mehr als `MAX_PIECES` → `TooComplex`.
  3. Stücke mit 2D-Länge unter `64·geom_tol` werden entfernt.
  Das Ergebnis ist deterministisch (Sortierung nach Strichindex, dann `t0`).
- [ ] **Step 1: Failing tests** (`tests/stage8_merge_series.rs`):

```rust
mod common;
use common::{check::*, model::*};
use planora_drawing::{DrawBody, Visibility, compute_view, model::ViewKind};
use planora_exchange::Progress;

fn view(s: &planora_kernel::Solid, k: ViewKind) -> planora_drawing::ViewLines {
    compute_view(&[DrawBody { name: "K".into(), solid: s }], k, &Progress::new()).unwrap()
}

#[test]
fn front_view_of_box_is_exactly_four_visible_pieces() {
    let v = view(&box_solid(10.0, 20.0, 30.0), ViewKind::Front);
    assert_eq!(v.pieces.len(), 4);
    assert!(v.pieces.iter().all(|p| p.vis == Visibility::Visible));
    assert!((piece_length_sum(&v, Visibility::Visible) - 80.0).abs() < 1e-6);
}

#[test]
fn top_view_of_cylinder_is_one_circle() {
    let v = view(&cylinder_solid(5.0, 12.0), ViewKind::Top);
    assert_eq!(v.pieces.len(), 1);
    assert!((piece_length_sum(&v, Visibility::Visible) - 2.0 * std::f64::consts::PI * 5.0).abs() < 1e-6);
}

#[test]
fn front_view_of_cylinder_is_rectangle_outline() {
    // Mantellinien (2·12) + obere und untere Kreiskante als Strecken der Länge 10 (zwei Hälften je Seite fallen zusammen)
    let v = view(&cylinder_solid(5.0, 12.0), ViewKind::Front);
    assert!((piece_length_sum(&v, Visibility::Visible) - (2.0 * 12.0 + 2.0 * 10.0)).abs() < 1e-6);
    assert_eq!(piece_length_sum(&v, Visibility::Hidden), 0.0);
}

#[test]
fn visible_wins_over_hidden_when_coincident() {
    // Quader mit Bohrung längs Y: Lochkreis vorne (sichtbar) und hinten (verdeckt, aber deckungsgleich) → ein sichtbarer Kreis
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::Y);
    let v = view(&s, ViewKind::Front);
    assert_eq!(piece_length_sum(&v, Visibility::Hidden), 0.0);
    let circle: f64 = 2.0 * std::f64::consts::PI * 5.0;
    assert!((piece_length_sum(&v, Visibility::Visible) - (2.0 * (40.0 + 30.0) + circle)).abs() < 1e-6);
}

#[test]
fn compute_view_is_deterministic() {
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X);
    let a = view(&s, ViewKind::Isometric);
    let b = view(&s, ViewKind::Isometric);
    assert_eq!(a.pieces, b.pieces);
}

#[test]
fn no_tiny_pieces_and_coverage_hold_in_the_series() {
    // 200 Zufallskörper (feste Startwerte 1..=200): Quader, Zylinder, Kugel, Kegel, mit 0–3 Booleschen (Vereinigung/Differenz) aus Primitiven in zufälliger Lage (nur Vielfache von 0,5 mm, damit nichts in Streifstellung fällt); jede der 7 Ansichten.
    // Erlaubt sind nur: Ok, oder `AmbiguousVisibility`/`TooComplex`/`Unsupported` (gezählt; höchstens 10 % aller Läufe). Bei Ok gelten alle Prüfungen von `check_view` (inkl. Netzreferenz) ohne Fehler.
    let mut ok = 0; let mut named = 0;
    for seed in 1..=200u64 {
        let s = random_body(seed);                     // tests/common/model.rs, deterministisch (eigener kleiner Zufallszahlengenerator, kein neues Paket)
        for k in planora_drawing::model::ViewKind::ALL {
            match compute_view(&[DrawBody { name: "R".into(), solid: &s }], k, &Progress::new()) {
                Ok(_) => { let r = check_view(&[&s], k); assert!(r.errors.is_empty(), "seed {seed} {k:?}: {:?}", r.errors); ok += 1; }
                Err(planora_drawing::DrawError::AmbiguousVisibility(_) | planora_drawing::DrawError::TooComplex(_) | planora_drawing::DrawError::Unsupported(_)) => named += 1,
                Err(e) => panic!("seed {seed} {k:?}: {e}"),
            }
        }
    }
    assert!(named * 10 <= ok + named, "zu viele benannte Fehler: {named} von {}", ok + named);
}

#[test]
fn many_bodies_cost_limit_is_reported_not_exceeded() {
    // 2 000 kleine Quader in einem Raster: entweder Ergebnis oder TooComplex, innerhalb von 60 s; Abbruch über Progress wirkt binnen 2 s.
}
```
  Der letzte Test ist vom Implementierer **auszuschreiben** (Raster aus Quadern über `box_solid` und Verschiebung durch neue Konstruktion; Zeitmessung mit `std::time::Instant`; zweiter Lauf mit einem Thread, der nach 500 ms `Progress::cancel` ruft).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS** (Reihe darf einige Minuten laufen; mit `--release` getestet, Zeit im Bericht), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** (a) „sichtbar gewinnt“ umdrehen (verdeckt gewinnt) → `visible_wins_over_hidden…` rot; (b) Schritt 2 weglassen → `front_view_of_box…` rot; (c) Schritt 1 weglassen → `top_view_of_cylinder_is_one_circle` rot (Kreis in Teilstücken).
- [ ] **Step 6: Commit** — `feat: deckungsgleiche Stücke vereinen, Prüfreihe der Ansichten`.

---

### Task 9: Blatt-Linien und Ausgabekurven

**Files:**
- Create: `crates/planora-drawing/src/{sheet.rs,out2d.rs}`, `tests/stage9_sheet.rs`

**Interfaces:**
- Consumes: `ViewLines`, `Drawing`, `ViewSpec`, `layout`.
- Produces (`out2d.rs`) – Ausgabekurven in Blattkoordinaten (mm, y nach oben):
```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Out2d {
    Line { a: DVec2, b: DVec2 },
    /// Kreisbogen gegen den Uhrzeigersinn von `start` bis `end` (Radiant, `end > start`, höchstens TAU); voller Kreis: end = start + TAU.
    Arc { c: DVec2, r: f64, start: f64, end: f64 },
    /// Ellipse mit Hauptachse `major` (Vektor vom Mittelpunkt zum Hauptscheitel), Nebenachse `ratio·|major|`, Parameter gegen den Uhrzeigersinn (Radiant).
    Ellipse { c: DVec2, major: DVec2, ratio: f64, start: f64, end: f64 },
    /// Rationale B-Spline-Kurve (Grad, Knoten, Kontrollpunkte, Gewichte), Parameterbereich = Knotenbereich.
    Spline { degree: usize, knots: Vec<f64>, ctrl: Vec<DVec2>, weights: Vec<f64> },
}
impl Out2d { pub fn point(&self, t: f64) -> DVec2; pub fn length(&self) -> f64; pub fn bounds(&self) -> (DVec2, DVec2); }
pub fn piece_to_out(stroke: &Stroke, basis: &Basis, t0: f64, t1: f64, xf: &SheetTransform) -> Out2d;
```
  `piece_to_out`: `Line` → `Out2d::Line` (projizierte Endpunkte). `Circle`/`Ellipse` → projizierte konjugierte Halbmesser `p = ex·(…)`; Umwandlung in **Hauptachsen**: `A = proj(frame.x·a)`, `B = proj(frame.y·b)` (konjugiert); Hauptachsen aus der Singulärwertzerlegung der 2×2-Matrix `[A B]`: Winkel `θ = ½·atan2(2(A·B), |A|² − |B|²)`, Hauptachse `major = A cos θ + B sin θ` (Länge `σ₁`), Nebenachse `σ₂`; Parameter der Hauptachsendarstellung: `P(t) = c + A cos t + B sin t = c + major cos(t − t₀) + minor sin(t − t₀)` mit Verschiebung `t₀` – aus `θ` abzuleiten (Test prüft Punktgleichheit). `ratio = σ₂/σ₁`; ist `ratio ≥ 1 − 1e-9`: `Arc` (Kreis), ist `ratio ≤ 1e-9` oder ist die projizierte Ellipse entartet: `Line` (zwischen den Extrempunkten, nur das gezeichnete Stück). Orientierung: die Spiegelung (`ex × ey = −d`) kann die Umlaufrichtung umdrehen: bei negativer Determinante von `[A B]` Parameterbereich und Richtung so wandeln, dass `start < end` und gegen den Uhrzeigersinn. `Nurbs` → `Spline` (Kontrollpunkte projiziert), Teilbereich `[t0, t1]` durch **Knoteneinfügen** (planora-math: `NurbsCurve` bietet das nicht? dann eigenes Knoteneinfügen/-teilen nach dem Boehm-Algorithmus in `out2d.rs`), Ergebnis auf `[t0, t1]` zugeschnitten (offene Knotenvektoren, Randknoten mit Vielfachheit `degree + 1`).
- Produces (`sheet.rs`):
```rust
#[derive(Clone, Copy, Debug)] pub struct SheetTransform { pub scale: f64, pub tx: f64, pub ty: f64, pub bbox_center: DVec2 }  // Blatt = scale·(p − bbox_center) + (tx, ty)
pub struct SheetLine { pub vis: Visibility, pub curve: Out2d }
pub struct SheetView { pub id: ViewId, pub name: String, pub lines: Vec<SheetLine>, pub extent_mm: (f64, f64) /* Breite/Höhe auf dem Blatt */ }
pub struct SheetModel { pub sheet: Sheet, pub frame: (DVec2, DVec2) /* Innenrahmen, 10 mm Rand */, pub views: Vec<SheetView> }
pub fn view_center(drawing: &Drawing, view: ViewId, results: &BTreeMap<ViewId, &ViewLines>) -> Option<DVec2>;   // Mitte der Ansicht auf dem Blatt (Free: x,y; Aligned: Elternmitte + Abstand·align_dir)
pub fn build_sheet(drawing: &Drawing, results: &BTreeMap<ViewId, &ViewLines>) -> SheetModel;
```
  `build_sheet`: je Ansicht mit Ergebnis die Stücke (verdeckte nur wenn `show_hidden`) nach `piece_to_out` umgesetzt; `SheetTransform.scale = scale.factor()`, `bbox_center` = Mitte von `ViewLines.bbox`. Ansichten ohne Ergebnis fehlen in `views` (die Oberfläche zeigt dort ihren Fehler/Fortschritt). `Aligned`: Mitte = Mitte des Elternbildes + `distance · align_dir(projection, parent.kind, kind)`; ist das Elternbild nicht berechnet, entfällt die Ansicht.
  Namen der Ansichten: `ViewKind::label()`; bei mehrfach gleicher Art ` 2`, ` 3` angehängt (Reihenfolge der Ansichten).
- [ ] **Step 1: Failing tests** (`tests/stage9_sheet.rs`): (a) `circle_in_top_view_becomes_arc_with_full_turn`; (b) `circle_in_front_view_becomes_line_of_diameter_length`; (c) `tilted_circle_is_ellipse_with_expected_axes` (Isometrisch eines Kreises: `σ₁ = r`, `σ₂ = r·|d·z|`, Hauptachsen senkrecht zueinander, `Out2d::point` stimmt mit projiziertem 3D-Punkt an 50 Stellen auf 1e-9); (d) `reflected_view_keeps_counter_clockwise_arcs` (Back-Ansicht); (e) `spline_piece_is_cut_exactly` (Teilstück einer NURBS: 100 Punkte gleich der Auswertung der Originalkurve auf 1e-9); (f) `hidden_pieces_follow_show_hidden`; (g) `aligned_view_sits_below_front_in_first_angle` (Mittelpunkte: gleiche x, y um `distance` darunter); (h) `scale_is_applied` (Linie der Länge 10 bei 1:2 wird 5 mm). Alle mit Zahlen aus der Rechnung im Test, nicht aus der Bibliothek.
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Orientierungskorrektur weglassen → (d) rot; `θ`-Formel mit falschem Vorzeichen → (c) rot.
- [ ] **Step 6: Commit** — `feat: Blattmodell und Ausgabekurven (Linien, Bögen, Ellipsen, Splines)`.

---

### Task 10: SVG-Schreiber und eigener Leser

**Files:**
- Create: `crates/planora-drawing/src/svg.rs`, `tests/common/svg_read.rs`, `tests/stage10_svg.rs`

**Interfaces:**
- Produces (`svg.rs`): `pub fn write_svg(model: &SheetModel, out: &mut dyn Write) -> Result<(), DrawError>` und `pub fn svg_string(model: &SheetModel) -> Result<String, DrawError>`. Aufbau:
  - Kopf `<?xml version="1.0" encoding="UTF-8"?>`, `<svg xmlns="http://www.w3.org/2000/svg" width="{W}mm" height="{H}mm" viewBox="0 0 {W} {H}">` (`W`,`H` = `Sheet::size_mm`, Zahlen mit höchstens 6 Nachkommastellen, ohne Exponent).
  - Innenrahmen: `<rect x="10" y="10" width="{W-20}" height="{H-20}" fill="none" stroke="#000" stroke-width="0.5"/>`.
  - Je Ansicht `<g id="{Name}">` mit `transform` nur für die y-Spiegelung `matrix(1 0 0 -1 0 H)` auf der **Wurzelgruppe** (alle Koordinaten bleiben Blattkoordinaten, y oben); darin je Sichtbarkeit ein `<path>`: sichtbar `stroke="#000" stroke-width="0.5" fill="none" stroke-linecap="round"`; verdeckt `stroke-width="0.25" stroke-dasharray="3 1"`.
  - Pfadbefehle: `Line` → `M x y L x y`; `Arc` → `M` Startpunkt, `A r r 0 {large} 1 x y` (sweep 1 = gegen den Uhrzeigersinn im **ungespiegelten** y-aufwärts-Raum erscheint nach der Spiegelung als sweep 0 – **sorgfältig** ableiten und testen; Voll-Kreis als zwei Halbbögen); `Ellipse` → `A rx ry rot large sweep x y` (`rot` in Grad, aus `major`), Voll-Ellipse in zwei Hälften; `Spline` → Zerlegung in **Bézier-Stücke** (Knoteneinfügen bis jeder Knoten Vielfachheit `degree` hat → Bézier-Segmente), Grad 3 → `C`, Grad 2 → `Q`, Grad 1 → `L`, Grad ≥ 4 → Graderniedrigung nicht exakt: stattdessen je Segment Unterteilung in 8 gleiche Teile und Anpassung an kubische Bézier-Kurven (Fehler ≤ `1e-6` mm gemessen; sonst weiter unterteilen, höchstens 64 Teile); **rationale** Splines (Gewichte nicht alle gleich) → Unterteilung wie bei Grad ≥ 4 (rationale Bézier-Kurven vom Grad 2 sind Kegelschnitte: dafür `A`, wenn es ein Kegelschnittstück ist, sonst Näherung wie oben, Fehler ≤ `1e-6` mm).
  - Kein `NaN`/`inf`: `Err(DrawError::NonFinite)`.
- Produces (Tests): `tests/common/svg_read.rs` – **eigener** Leser: zerlegt Pfade in `Out2d`-ähnliche Teile (`M/L/A/C/Q/Z`), bildet y wieder zurück, liefert `Vec<ReadPath { dashed: bool, width: f64, segments: Vec<Seg> }>` mit `length()` (numerisch, 4096 Stützstellen je Segment) und `point_at`; Ellipsenbögen nach der SVG-Implementierungshinweis-Formel (Endpunkt-zu-Mitte-Umrechnung), **unabhängig** von `out2d.rs` geschrieben.
- [ ] **Step 1: Failing tests** (`tests/stage10_svg.rs`): (a) `header_and_size` (A3 quer: `width="420mm" height="297mm"`); (b) `box_front_view_roundtrips` (Vierecklänge 80 mm, vier Pfadsegmente `L`, `stroke-dasharray` fehlt bei sichtbar); (c) `cylinder_top_roundtrips_to_circle` (Umfang 2πr auf 1e-6, Mittelpunkt, Radius); (d) `iso_cylinder_ellipse_roundtrips` (Punkte der Datei vs. Formel); (e) `spline_roundtrips_within_1e6` (NURBS-Kante, gelesene Länge/Punkte vs. Kurve); (f) `hidden_lines_are_dashed` (Quader mit Bohrung längs X: verdeckter Pfad trägt `stroke-dasharray="3 1"`, Breite 0,25); (g) `y_axis_is_flipped_once` (Linie bei Blatt-y = 10 steht bei SVG-Rohkoordinate `H − 10` **oder** über die Gruppentransformation – der Leser rechnet zurück; Test prüft das zurückgerechnete y); (h) `non_finite_is_refused`; (i) `view_group_ids_are_names_and_escaped` (Name mit `<&">` wird als `&lt;&amp;&quot;&gt;` geschrieben, doppelte Namen erhalten ` 2`).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** sweep-Flag vertauschen → (c)/(d) rot; Spiegelung doppelt → (g) rot; Strichelung weglassen → (f) rot.
- [ ] **Step 6: Commit** — `feat: SVG-Schreiber mit eigenem Leser und Rücklesetests`.

---

### Task 11: DXF-Schreiber und eigener Leser

**Files:**
- Create: `crates/planora-drawing/src/dxf.rs`, `tests/common/dxf_read.rs`, `tests/stage11_dxf.rs`

**Interfaces:**
- Produces (`dxf.rs`): `pub fn write_dxf(model: &SheetModel, out: &mut dyn Write) -> Result<(), DrawError>` und `pub fn dxf_string(model: &SheetModel) -> Result<String, DrawError>`. ASCII-DXF, Version **AC1015** (R2000), Zeilenende `\r\n`, Gruppencode/Wert als zwei Zeilen. Abschnitte:
  - `HEADER`: `$ACADVER = AC1015`, `$INSUNITS = 4`, `$MEASUREMENT = 1`, `$HANDSEED`, `$EXTMIN/$EXTMAX` (Blattgröße).
  - `TABLES`: `LTYPE` (`CONTINUOUS`, `HIDDEN` mit Muster 3/−1 mm; Gruppen 73/40/49), `LAYER` (`0`, `SICHTBAR` weiß/schwarz Farbe 7 `CONTINUOUS` Linienstärke 50 [1/100 mm], `VERDECKT` Farbe 7 `HIDDEN` Stärke 25), `STYLE` (`STANDARD`), `VPORT`/`APPID` (`ACAD`) soweit für ein gültiges R2000 nötig (Pflichttabellen nach DXF-Referenz; Implementierer prüft mit Leser **und** Prüfliste in Step 5).
  - `BLOCKS`: je Ansicht ein `BLOCK` mit Namen = Ansichtsname (DXF-Namen: ohne `<>/\":;?*|=\``, ersetzt durch `_`, doppelte mit ` 2`), darin die Objekte; zusätzlich `*MODEL_SPACE`/`*PAPER_SPACE` wie vorgeschrieben.
  - `ENTITIES`: Randlinie (`LWPOLYLINE`, geschlossen, Ebene `SICHTBAR`) und je Ansicht ein `INSERT` des Blocks bei `(0,0)`.
  - Objekte je `Out2d`: `Line` → `LINE` (10/20/11/21), `Arc` → `ARC` bzw. `CIRCLE` (voller Kreis; Winkel in **Grad**, gegen den Uhrzeigersinn, `start < end`, Werte in `[0, 360)`), `Ellipse` → `ELLIPSE` (10/20 Mittelpunkt, 11/21 Hauptachsenendpunkt relativ, 40 Verhältnis, 41/42 Start-/Endparameter in Radiant, 210/220/230 `0,0,1`), `Spline` → `SPLINE` (70 Flags: 8 = planar; rational: +4; 71 Grad, 72 Knotenzahl, 73 Kontrollpunktzahl, 40… Knoten, 41… Gewichte falls rational, 10/20/30… Kontrollpunkte).
  - Ebene je Objekt = `SICHTBAR` oder `VERDECKT`; **Handles** fortlaufend hexadezimal, jeder Eintrag eindeutig, `$HANDSEED` größer als alle; Zahlen mit bis zu 9 Nachkommastellen (`-0` wird `0`), nie `NaN`/`inf` (`NonFinite`). Dateiende `0\nEOF`.
- Produces (Tests): `tests/common/dxf_read.rs` – **eigener** Leser (Gruppencode/Wert-Paare, Abschnitte, Blöcke, Entitäten `LINE/CIRCLE/ARC/ELLIPSE/SPLINE/LWPOLYLINE/INSERT`, Handles auf Eindeutigkeit, Ebenen/Linienarten), liefert `Vec<ReadEntity>` mit Länge und Punktauswertung (Spline: de Boor, unabhängig geschrieben).
- [ ] **Step 1: Failing tests** (`tests/stage11_dxf.rs`): (a) `structure_is_valid` (Abschnittsfolge `HEADER, TABLES, BLOCKS, ENTITIES`, endet mit `EOF`, Handles eindeutig, `$ACADVER`, `$INSUNITS = 4`); (b) `box_front_roundtrips_to_four_lines` (Längen, Endpunkte auf 1e-9); (c) `cylinder_top_is_a_circle_entity` (`CIRCLE`, Mitte/Radius); (d) `half_circle_is_arc_in_degrees` (`ARC` 0…180); (e) `iso_cylinder_is_ellipse_entity` (Verhältnis, Hauptachse, Punkte auf 1e-9 gegen Formel); (f) `spline_roundtrips` (Grad, Knoten, Kontrollpunkte exakt gleich der Quelle nach Teilbereich; Punkte auf 1e-9); (g) `hidden_goes_to_layer_VERDECKT_with_HIDDEN_linetype`; (h) `views_are_blocks_with_inserts`; (i) `names_are_sanitized_and_unique`; (j) `non_finite_is_refused`; (k) `reflected_back_view_has_ccw_arcs` (ARC-Winkel `start < end`, Punkte stimmen).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Winkel in Radiant statt Grad → (d) rot; Handles doppelt → (a) rot; Ebenenzuordnung vertauscht → (g) rot. Zusätzlich: Prüfliste der DXF-Pflichtteile (Tabellenreihenfolge `VPORT, LTYPE, LAYER, STYLE, VIEW, UCS, APPID, DIMSTYLE, BLOCK_RECORD`) gegen die offene DXF-Referenz der Spezifikationstexte des Implementierers abgleichen; fehlende Pflichttabellen ergänzen. (Echter Fremdprogramm-Test beim Handtest, Aufgabe 18.)
- [ ] **Step 6: Commit** — `feat: DXF-Schreiber (R2000) mit eigenem Leser und Rücklesetests`.

---

### Task 12: Zeichnungen im Dokument (`planora-feature`)

**Files:**
- Modify: `crates/planora-feature/Cargo.toml` (`planora-drawing.workspace = true`), `src/lib.rs`, `src/model.rs` (Felder), `src/snapshot.rs`
- Create: `crates/planora-feature/src/drawings.rs`, `crates/planora-feature/tests/drawings.rs`

**Interfaces:**
- Consumes: `planora_drawing::model::*` (Aufgabe 1), `FeatureError`.
- Produces (`Document`, in `drawings.rs`; Felder `drawings: Vec<Drawing>`, `next_drawing: u64` (beginnt bei 1), `drawing_edits: u64` in `model.rs`):
```rust
pub const MAX_DRAWINGS: usize = 1000;
pub const MAX_VIEWS: usize = 64;
pub const MAX_NAME_CHARS: usize = 200;
pub const MAX_POS_MM: f64 = 1.0e5;
impl Document {
    pub fn drawings(&self) -> &[Drawing];
    pub fn drawing(&self, id: DrawingId) -> Option<&Drawing>;
    pub fn add_drawing(&mut self, name: &str, sheet: Sheet, projection: Projection) -> Result<DrawingId, FeatureError>;
    pub fn rename_drawing(&mut self, id: DrawingId, name: &str) -> Result<(), FeatureError>;
    pub fn delete_drawing(&mut self, id: DrawingId) -> Result<(), FeatureError>;
    /// Ändert eine Zeichnung auf einer Kopie und prüft das Ergebnis; bei Fehler bleibt das Dokument unverändert.
    pub fn update_drawing(&mut self, id: DrawingId, f: impl FnOnce(&mut Drawing)) -> Result<(), FeatureError>;
    pub fn add_view(&mut self, id: DrawingId, kind: ViewKind, scale: Scale, placement: Placement, parent: Option<ViewId>) -> Result<ViewId, FeatureError>;
    /// Zähler der Änderungen **an Körpern** (Features); ändert sich bei Zeichnungsänderungen nicht (Schlüssel der Ansichtsberechnung, F8).
    pub fn model_edit_count(&self) -> u64;
}
pub fn validate_drawing(d: &Drawing) -> Result<(), String>;   // auch von from_snapshot benutzt
```
  `edit_count()` liefert künftig `edits + drawing_edits` (jede öffentliche Änderung, auch an Zeichnungen, zählt für „ungespeichert“); `model_edit_count()` liefert nur `edits`.
  `validate_drawing`: Name nicht leer (nach `trim`), höchstens `MAX_NAME_CHARS` Zeichen; höchstens `MAX_VIEWS` Ansichten; Ansichtskennungen eindeutig, `< next_view`, nie 0; jeder `Scale` gültig (`1 ≤ num, den ≤ 1000`); `parent` verweist auf eine **frühere** Ansicht derselben Zeichnung (keine Zyklen), und zwar auf eine Normansicht; `Placement::Aligned` nur mit `parent = Some` und nur für Normansichten (`align_dir` ist `Some`); `Placement::Free` ohne Eltern **oder** mit Eltern (dann gilt es als losgelöst – nicht erlaubt: `parent = Some` verlangt `Aligned`); alle Zahlen endlich, `|x|, |y|, |distance| ≤ MAX_POS_MM`; Grundansicht (`parent = None`) darf jede Art sein.
  Fehler: `FeatureError::InvalidInput(<deutscher Text>)` bzw. `UnknownFeature` für unbekannte Zeichnung (neue Variante nicht nötig: `InvalidInput("Die Zeichnung gibt es nicht")`).
  **Snapshot** (`snapshot.rs`): `Snapshot` bekommt `pub drawings: Vec<Drawing>` und `pub next_drawing: u64`; `Document::snapshot` kopiert sie; `from_snapshot` prüft `next_drawing ≠ u64::MAX`, Kennungen eindeutig und `< next_drawing`, nie 0, höchstens `MAX_DRAWINGS`, jede Zeichnung mit `validate_drawing`, Namen **eindeutig** (Vergleich ohne Beachtung von Groß-/Kleinschreibung nach `trim`); danach `drawing_edits = 0`.
- [ ] **Step 1: Failing tests** (`tests/drawings.rs`): (a) `add_rename_delete_roundtrip` (Kennungen aufsteigend, `edit_count` steigt bei jeder Änderung, `model_edit_count` bleibt); (b) `invalid_drawings_are_refused_and_leave_the_document_unchanged` (leerer Name, 65. Ansicht, Scale 0:1, Parent später/fremd/Isometrisch als Eltern, `Aligned` ohne Eltern, NaN-Position, Position 1e9); (c) `duplicate_names_are_refused_case_insensitively`; (d) `remove_view_removes_children_via_update`; (e) `snapshot_roundtrip_keeps_drawings` (`from_snapshot(snapshot())` gleich, `edit_count == 0`); (f) `snapshot_with_bad_data_is_refused` (doppelte Kennung, Kennung ≥ next, Namen doppelt, Zyklus, Ansicht mit `Aligned` und Elternart Isometrisch); (g) `feature_edit_changes_model_count_drawing_edit_does_not`; (h) `old_snapshot_without_drawings_loads` (`drawings = vec![]`, `next_drawing = 1`).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS** (`cargo test -p planora-feature`; alle bisherigen Tests bleiben grün – Aufrufer von `Snapshot { .. }` (Tests in `planora-project`, `planora-app`) mit `drawings: vec![], next_drawing: 1` ergänzen, mechanisch), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Zyklenprüfung weglassen → (f) rot; `edit_count` ohne `drawing_edits` → (a) rot.
- [ ] **Step 6: Commit** — `feat: Zeichnungen im Dokument mit Prüfung und Snapshot`.

---

### Task 13: Format 4 – Zeichnungen in der Versionsdatei (`planora-project`)

**Files:**
- Modify: `crates/planora-project/src/{format.rs,convert.rs,lib.rs}`, `crates/planora-project/Cargo.toml` (`planora-drawing.workspace = true`)
- Create: `crates/planora-project/tests/drawings_format.rs`

**Interfaces:**
- Produces (`format.rs`), alle ohne `deny_unknown_fields` wie die übrigen:
```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ZeichnungDto { pub id: u64, pub name: String, pub blatt: BlattDto, pub projektion: String /* "erst" | "dritt" */,
    pub naechste_ansicht: u64, pub ansichten: Vec<AnsichtDto> }
pub struct BlattDto { pub papier: String /* "A4".."A0" */, pub quer: bool }
pub struct AnsichtDto { pub id: u64, pub art: String /* "vorne","hinten","links","rechts","oben","unten","iso" */,
    pub massstab: [u32; 2], pub lage: LageDto, pub verdeckt_zeigen: bool, pub eltern: Option<u64> }
#[serde(tag = "art")] pub enum LageDto { #[serde(rename = "frei")] Frei { x: f64, y: f64 }, #[serde(rename = "ausgerichtet")] Ausgerichtet { abstand: f64 } }
```
  `VersionFile` bekommt `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub zeichnungen: Vec<ZeichnungDto>` und `#[serde(default, skip_serializing_if = "is_zero")] pub naechste_zeichnung: u64`.
- Produces (`convert.rs`): `FORMAT_VERSION = 4`; `required_format`: **4**, wenn `!s.drawings.is_empty()`; sonst wie bisher (3, 2, 1). Kodieren/Dekodieren der Zeichnungen mit Prüfung beim Lesen (Fehler als `ProjectError::corrupt(origin, "<Text>")`): Format < 4 mit vorhandenen Zeichnungen → beschädigt; unbekannte Strings für Papier/Projektion/Art → beschädigt; Zahlen nur endlich und `|·| ≤ MAX_POS_MM`; `massstab` beide in `1..=1000`; Listenlängen höchstens `MAX_LIST`; **danach** `Document::from_snapshot`-Prüfung (`validate_drawing`) über `Snapshot` im bisherigen Pfad (Fehler → `ProjectError::Invalid`). `naechste_zeichnung` fehlt oder 0 → `max(id) + 1` bzw. 1; jede Kennung ≥ `naechste_zeichnung` → beschädigt.
- [ ] **Step 1: Failing tests** (`tests/drawings_format.rs`): (a) `snapshot_with_drawings_roundtrips_exactly` (Snapshot mit zwei Zeichnungen, Ansichten aller Arten, `Free` und `Aligned`; `decode_version(encode_version(s))` gleich); (b) `format_is_4_only_with_drawings`; (c) `old_files_stay_byte_for_byte` (Snapshot ohne Zeichnungen: Ausgabe enthält weder `zeichnungen` noch `naechste_zeichnung`; `format` wie früher; Zeichenkette gleich einer aus der **bestehenden** Testfixtur des Pakets codierten – wo keine vorhanden, das Ergebnis vor der Änderung mit `git stash`-freiem Weg erzeugen: Test baut die Zeichenkette aus `master` über `include_str!` einer im Repo abgelegten Datei `tests/fixtures/format3_no_drawings.json`, die der Implementierer **vor** der Änderung mit dem alten Code erzeugt und einchecken lässt); (d) `format_3_file_with_drawings_is_refused`; (e) `hostile_files_are_refused_not_panicking` (NaN als Zahl ist in JSON nicht darstellbar → `1e999` und `null`; unbekannte Art; Maßstab `[0,1]`; Position 1e300; doppelte Ansichts-Kennung; `eltern` auf unbekannte Ansicht; `ausgerichtet` ohne Eltern; Namen leer; 1 000 001 Ansichten über `MAX_LIST`; Kennung ≥ `naechste_zeichnung`); (f) `format_5_is_refused_with_clear_text` (bestehender Pfad `head.format > FORMAT_VERSION`).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS** (`cargo test -p planora-project`, `-p planora-app` nur Kompilierung/Fixtur-Anpassungen), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** `skip_serializing_if` entfernen → (c) rot; Prüfung der Maßstäbe weglassen → (e) rot.
- [ ] **Step 6: Commit** — `feat: Format 4 mit Zeichnungen, alte Dateien unverändert`.

---

### Task 14: Zeichnungsstand der App und Hintergrundberechnung

**Files:**
- Create: `crates/planora-app/src/drawing_state.rs`, `crates/planora-app/tests/drawing_state.rs`
- Modify: `crates/planora-app/src/{lib.rs,state.rs}` (Feld `pub drawing_ui: DrawingUi` in `AppState`, `Default`), `Cargo.toml` (`planora-drawing.workspace = true`)

**Interfaces:**
- Consumes: `Job`/`Poll` (`jobs.rs`), `Body`/`BodyKind` (`state.rs`), `compute_view`, `build_sheet`, `Document::{drawings, model_edit_count, add_view, update_drawing}`.
- Produces (`drawing_state.rs`):
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum WorkArea { Model, Drawing(DrawingId) }
pub enum ViewResult { Pending { fraction: f32 }, Ready(Arc<ViewLines>), Failed(String) }
pub struct DrawingUi { pub area: WorkArea, pub selected_view: Option<ViewId>, pub zoom: f32, pub pan: egui::Vec2, /* intern: cache, jobs, Zähler */ }
impl DrawingUi {
    pub fn spawned(&self) -> u64;                                   // Zahl gestarteter Berechnungen (Tests)
    /// Startet fehlende oder veraltete Berechnungen für die Ansichtsarten der aktuellen Zeichnung; holt fertige ab (mit `block` wartend).
    pub fn tick(&mut self, state_bodies: &[Body], document: &Document, block: bool);
    pub fn result(&self, kind: ViewKind) -> ViewResult;
    pub fn results_for(&self, drawing: &Drawing) -> BTreeMap<ViewId, Arc<ViewLines>>;   // nur fertige
    pub fn note(&self, bodies: &[Body]) -> Option<String>;          // Hinweis „Netz-Körper werden nicht gezeichnet: …“ / „Es gibt keinen Körper …“
}
pub fn estimate_view_size(bodies: &[Body], kind: ViewKind) -> (f64, f64);                      // aus den Hüllboxen (8 Ecken projiziert)
pub fn suggest_scale(bodies: &[Body], sheet: Sheet, projection: Projection, base: ViewKind) -> Scale; // größte Reihenstufe, bei der Grundansicht + Seite + Draufsicht samt `GAP_MM` in den Innenrahmen passen
pub fn default_distance(bodies: &[Body], parent: ViewKind, child: ViewKind, parent_scale: Scale, child_scale: Scale) -> f64;
```
  Verhalten von `tick`: Zeichnungsbereich nicht geöffnet → nichts tun. Schlüssel eines Eintrags: `(document.model_edit_count(), kind)`; passt der Schlüssel nicht, wird der alte Job verworfen (`Drop` bricht ab) und ein neuer gestartet (Kopien der exakten Körper `Vec<(String, Solid)>`, Netz-Körper ausgelassen, kein Zugriff auf das Dokument im Thread); `ViewResult::Pending { fraction }` aus `Job::fraction`. Ergebnis `Err(DrawError)` → `Failed(Anzeigetext)` (`to_string`). Es wird **je Art** höchstens ein Job je Stand gestartet, auch wenn mehrere Ansichten dieselbe Art zeigen. Ansichtsarten, die keine Ansicht der geöffneten Zeichnung braucht, werden aus dem Zwischenspeicher entfernt (Jobs dadurch abgebrochen). Wechsel der Zeichnung oder Löschen der geöffneten Zeichnung → `area = Model`.
  `note`: nur Netz-Körper vorhanden → Text „Alle Körper sind Netze; eine Zeichnung braucht exakte Körper.“; sonst gibt es Netz-Körper → „Netz-Körper werden nicht gezeichnet: A, B.“ (Namen wie beim Export gekürzt, `shorten_for_display`); keine Körper → „Es gibt keinen Körper, der sich zeichnen lässt.“
- [ ] **Step 1: Failing tests** (`tests/drawing_state.rs`, Stil wie `tests/export_step.rs`: Körper über `AnalyticKernel`/Features anlegen):
  (a) `opening_a_drawing_computes_each_kind_once` (Zeichnung mit zwei Vorderansichten verschiedenen Maßstabs + Oben: `spawned() == 2` nach `tick(.., true)`); (b) `moving_or_scaling_a_view_does_not_recompute` (Placement/Scale per `update_drawing` ändern, `tick` → `spawned()` unverändert); (c) `editing_the_model_recomputes_and_drops_old_lines` (Würfel-Kante von 10 auf 20 mm: neues `ViewLines.bbox`); (d) `a_running_job_is_cancelled_on_model_change` (Fortschritt der ersten `Progress` ist nach der Änderung abgebrochen: `Job` verworfen); (e) `no_bodies_and_mesh_only_give_named_failures`; (f) `failed_view_keeps_the_drawing` (Dokument behält Zeichnung und Ansichten); (g) `deleting_the_open_drawing_returns_to_model_area`; (h) `suggest_scale_fits_the_three_views` (Quader 100×60×40 auf A4 quer, Erstwinkel: gewählte Stufe ist die größte, deren drei Ansichten samt Lücken in `297−20 × 210−20` passen; Gegenprobe: nächstgrößere passt nicht); (i) `default_distance_matches_half_extents_plus_gap`; (j) `shared_cache_between_drawings` (zwei Zeichnungen mit Vorderansicht: nach Umschalten kein neuer Job, solange das Modell gleich bleibt – **nur** wenn `tick` den Eintrag nicht verwirft; Eintrag wird beim Wechsel behalten, bis er `MAX_CACHED = 7` übersteigt).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS** (`cargo test -p planora-app`), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Schlüssel auf `edit_count()` statt `model_edit_count()` → (b) rot; Job beim Schlüsselwechsel nicht verwerfen → (d) rot.
- [ ] **Step 6: Commit** — `feat: Zeichnungsstand der App mit Berechnung im Hintergrund`.

---

### Task 15: Zeichnungsbereich – Blattansicht, Werkzeugleiste, Dialoge

**Files:**
- Create: `crates/planora-app/src/{drawing_view.rs,drawing_dialog.rs}`, `crates/planora-app/tests/drawing_ui.rs`
- Modify: `crates/planora-app/src/app.rs` (`Behavior::pane_ui` für `Pane::Viewport`, Menü „Zeichnung“, Aufruf von `tick`), `crates/planora-app/src/lib.rs`, `crates/planora-app/src/workspace.rs` (Feld `drawing_dialog` im Tab bzw. in `AppState`), `crates/planora-app/src/theme.rs` (Blattfarben)

**Interfaces:**
- Produces (`drawing_view.rs`): `pub fn show(ui: &mut egui::Ui, state: &mut AppState)` – füllt die **ganze** Breite des Bereichs (Spec/Gedächtnis: volle Breite): Werkzeugleiste oben (Umschalter „3D-Modell“ / „Zeichnung: {Name}“, „Ansicht hinzufügen“ (Auswahlliste der sieben Arten), „Blatt einpassen“, Lupe +/−, bei gewählter Ansicht: Maßstab (Auswahlliste `Scale::SERIES`), Kontrollkästchen „Verdeckte Kanten zeigen“, „Ansicht löschen“), darunter die Blattfläche. Blatt: weißes Papier mit Rand/Schatten (in dunklem Thema ebenfalls weiß, mit dunklem Umfeld), Innenrahmen; Zoom über Mausrad (um den Zeiger), Verschieben mit mittlerer Maustaste oder Umschalt + Ziehen; „Blatt einpassen“ passt das Papier in den Bereich. Linien: sichtbar 0,5 mm breit (in Pixeln `max(1.0, 0.5·px_per_mm)`), verdeckt 0,25 mm gestrichelt (3 mm Strich, 1 mm Lücke, in Blatt-mm gerechnet); Bögen/Ellipsen/Splines werden für die Anzeige in Polylinien mit höchstens 1 px Abweichung zerlegt (Anzahl aus Länge·Zoom). Auswahl: Klick auf eine Linie (Abstand ≤ 6 px) oder in die Hüllbox einer Ansicht wählt sie (Rahmen aus Strichlinie in Akzentfarbe); Ziehen der gewählten Ansicht verschiebt sie (`Free`: x/y; `Aligned`: nur `distance`, Projektion der Mausbewegung auf `align_dir`), jede Änderung über `Document::update_drawing`. Berechnet eine Ansicht noch (`Pending`), zeigt ihr Platz einen Rahmen mit „wird berechnet … {n} %“ und einem Fortschrittsbalken; `Failed(text)` zeigt den Text rot im Rahmen der Ansicht (mit Zeilenumbruch). Hinweiszeile unten im Bereich: `DrawingUi::note`.
- Produces (`drawing_dialog.rs`): `NewDrawingDialog` (Fenster „Neue Zeichnung“: Name (Vorschlag „Zeichnung N“, N = kleinste freie Zahl), Papier (A4–A0), „Querformat“ (Vorgabe an), Projektion („Erstwinkel (Europa)“ / „Dritter Winkel (USA)“), Grundansicht (Normansichten; Vorgabe Vorne), Maßstab (Vorschlag `suggest_scale`, Auswahlliste), Schaltflächen „Anlegen“/„Abbrechen“; Fehler (leerer oder doppelter Name) als roter Text, Anlegen gesperrt); `AddViewDialog` entfällt: „Ansicht hinzufügen“ wirkt sofort (abgeleitete Ansicht: Eltern = die gewählte Normansicht, sonst die Grundansicht; `Aligned { distance: default_distance(..) }`, Maßstab des Elternteils; Isometrisch: `Free` rechts oberhalb des Blattmittelpunkts, Maßstab = Maßstab der Grundansicht; schon vorhandene Art wird erlaubt (zweite Vorderansicht), aber als abgeleitete Ansicht nur wenn die Ausrichtungsregel eine Richtung liefert; sonst `Free`).
- Menü „Zeichnung“ (nach „Skizze“, deutsche Namen): „Neue Zeichnung …“, Trennlinie, je Zeichnung ein Eintrag „Öffnen: {Name}“, „Zum 3D-Modell“. Im Skizzen-Modus gesperrt (wie die übrigen Erzeugen-Befehle). Beim Umschalten in die Zeichnung wird `tick` aufgerufen; im Zeichnungsbereich ruft jedes Bild `tick(.., false)` und fordert bei `Pending` ein Neuzeichnen an (`ctx.request_repaint_after(100 ms)`).
- Umschalten des Bereichs ändert **nicht** die Kamera/Auswahl des 3D-Modells.
- [ ] **Step 1: Failing tests** (`tests/drawing_ui.rs`, Stil wie `tests/export_step.rs`: `egui::Context` mit `fonts::definitions()`, breite `RawInput`, `collect_texts`): (a) `new_drawing_dialog_creates_a_drawing_with_suggested_scale` (Quader 100×60×40: Anlegen, Zeichnung und Grundansicht im Dokument, Maßstab gleich `suggest_scale`); (b) `empty_and_duplicate_names_block_creation_with_a_clear_text`; (c) `drawing_area_shows_all_texts_in_german` (Texte der Werkzeugleiste, „wird berechnet“, Hinweis, Fehlertexte; keine englischen Wörter wie „View“, „Sheet“); (d) `drawing_area_fills_the_whole_width` (Breite des Bereichs gleich Breite des Panes); (e) `adding_a_top_view_sits_below_front_in_first_angle` (nach `tick(true)`: `build_sheet` – Mitte der Draufsicht unter der Vorderansicht, gleiche x); (f) `dragging_an_aligned_view_moves_only_along_its_axis` (Mausereignis simulieren: Ziehen mit Versatz (30, 40) → nur `distance` ändert sich); (g) `dragging_a_free_view_moves_both_axes`; (h) `selecting_by_click_on_a_line_and_deleting_removes_children`; (i) `failed_view_shows_its_text_and_other_views_still_draw`; (j) `mesh_only_document_shows_the_note_instead_of_lines`; (k) `dark_theme_keeps_the_paper_white`; (l) `menu_lists_drawings_and_is_blocked_in_sketch_mode`; (m) `model_camera_is_unchanged_by_switching` (Kamera vor und nach Umschalten gleich).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS** (`cargo test -p planora-app`), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Projektion der Bewegung auf `align_dir` weglassen → (f) rot; Breite fest statt `available_width` → (d) rot.
- [ ] **Step 6: Commit** — `feat: Zeichnungsbereich mit Blattansicht, Werkzeugleiste und Neue-Zeichnung-Fenster`.

---

### Task 16: Browser-Abschnitt „Zeichnungen“ und Aktionen

**Files:**
- Modify: `crates/planora-app/src/browser.rs` (Abschnitt unter dem Feature-Baum), `crates/planora-app/src/state.rs` (Anforderungen `rename_drawing_draft`, `pending_delete_drawing`), `crates/planora-app/src/prompts.rs` bzw. dort, wo `pending_delete` gezeigt wird (Rückfrage)
- Create: `crates/planora-app/tests/drawing_browser.rs`

**Interfaces:**
- Produces: im Browser unter dem Feature-Baum eine Überschrift „Zeichnungen“; je Zeichnung eine Zeile (Name, Blattgröße und Projektion klein dahinter), darunter eingerückt die Ansichten (`ViewKind::label()`, Maßstab klein dahinter; mehrfach vorkommende Arten mit ` 2`, ` 3`). Leer: „Noch keine Zeichnung. Über „Zeichnung“ im Menü lässt sich eine anlegen.“ Klick auf eine Zeichnung wählt sie, **Doppelklick öffnet** sie (Bereich wird `Drawing(id)`); Klick auf eine Ansicht öffnet die Zeichnung und wählt die Ansicht. Rechtsklick auf eine Zeichnung: „Öffnen“, „Umbenennen“, „Löschen“; auf eine Ansicht: „Löschen“, „Verdeckte Kanten zeigen/ausblenden“. Löschen einer Zeichnung fragt zurück („Zeichnung „{Name}“ mit {n} Ansichten löschen?“, „Löschen“/„Abbrechen“); Löschen einer Ansicht mit abgeleiteten Ansichten nennt sie in der Rückfrage. Umbenennen: dasselbe kleine Fenster wie bei Features (Vorlage `rename_draft`), Fehler bei leerem/doppeltem Namen als roter Text. Zeilenhöhe, Schrift und Einrückung wie die Feature-Zeilen (`row`-Hilfe in `browser.rs`); volle Breite.
- [ ] **Step 1: Failing tests** (`tests/drawing_browser.rs`): (a) `section_lists_drawings_and_views_with_german_labels`; (b) `double_click_opens_the_drawing`; (c) `rename_and_delete_with_confirmation` (Abbrechen lässt alles; Löschen entfernt Zeichnung, Bereich fällt auf `Model` zurück, wenn sie offen war); (d) `delete_view_names_dependents_in_the_prompt`; (e) `empty_state_text`; (f) `rows_use_full_width_and_equal_heights`; (g) `drawings_survive_save_and_load` (über `Workspace`-Speichern/Laden wie die Tests in `tests/versions_ui.rs`/`projects_ui.rs`; nach dem Laden sind Zeichnung und Ansichten gleich und die Berechnung startet neu); (h) `unsaved_marker_after_drawing_edit` (Tab trägt „●“ nach `add_drawing`, nach Speichern nicht mehr).
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS**, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Rückfrage umgehen → (c) rot; Kinder nicht nennen → (d) rot.
- [ ] **Step 6: Commit** — `feat: Browser-Abschnitt Zeichnungen mit Öffnen, Umbenennen und Löschen`.

---

### Task 17: Export als SVG und DXF

**Files:**
- Create: `crates/planora-drawing/src/file.rs` (`DrawingFormat`, `export_drawing_file`, `drawing_to_bytes`), `crates/planora-app/tests/export_drawing.rs`
- Modify: `crates/planora-drawing/src/lib.rs`, `crates/planora-app/src/export_dialog.rs` (`ExportFormat::Drawing(DrawingFormat)`, Karte, Prüfung, Schreiben), `app.rs`/`workspace.rs` (Sperren beim Schreiben wie bei STEP), `crates/planora-app/tests/export_dialog.rs` (eine mechanische Zeile: Länge von `ExportFormat::ALL`)

**Interfaces:**
- Produces (`file.rs`): `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum DrawingFormat { Svg, Dxf }` mit `extension()` (`"svg"`, `"dxf"`) und `label()` („Zeichnung als SVG“, „Zeichnung als DXF“); `pub fn export_drawing_file(path: &Path, model: &SheetModel, format: DrawingFormat, progress: &Progress) -> Result<u64, DrawError>` – erst in Hilfsdatei `.<Name>.planora-tmp` (wie `planora_exchange::temp_path`), `sync_all`, dann umbenennen; bei jedem Fehler bleibt eine vorhandene Zieldatei unverändert, Hilfsdatei weg; Abbruch über `Progress`.
- Produces (App): `ExportFormat::ALL` wird `[ExportFormat; 7]` (Reihenfolge: STL, OBJ, 3MF, glTF, STEP, Zeichnung SVG, Zeichnung DXF). Für die Zeichnungsformate: statt Qualität und Umfang zeigt die Karte eine Auswahlliste „Zeichnung“ (Vorgabe: die geöffnete, sonst die erste) und die Angaben „Ansichten: n“, „Blatt: A3 quer“, „Linien: n sichtbar, m verdeckt“ (aus den berechneten Ansichten); Netz-Körper-Hinweis wie bei STEP; **kein Schreiben ohne Ergebnis**: Status `Ready` erst, wenn **alle** Ansichten der Zeichnung berechnet sind; Ansichten mit Fehler → Status `Failed` mit Text „Die Ansicht „Vorne“ lässt sich nicht berechnen: …“ (Zeichnung wird **nicht** teilweise geschrieben). Die Berechnung läuft im Arbeitsthread des Dialogs (eigener Job mit `Progress`, Schlüssel = `(Zeichnung, edit_count(), Format)`), nutzt dieselben Funktionen wie die Oberfläche (`compute_view`, `build_sheet`), **nicht** den Zwischenspeicher des Zeichnungsbereichs (kein Zugriff von zwei Stellen aus). Zielname-Normalisierung: Endung `.svg`/`.dxf` erzwingen (`normalize_target`); Dateiauswahl (`file_chooser.rs`) listet `.svg`/`.dxf` bei diesen Formaten. Fenster schließen/Programm beenden während des Schreibens: wie bei STEP gesperrt (Implementierer wählt die kleinste Änderung; Ruling ins Protokoll).
- [ ] **Step 1: Failing tests** (`tests/export_drawing.rs`, Stil wie `tests/export_step.rs`): (a) `svg_and_dxf_files_are_written_and_readable` (Quader, Zeichnung mit drei Ansichten; Datei existiert, `<svg` bzw. `SECTION` vorhanden, Größe in der Statuszeile stimmt); (b) `no_drawing_gives_a_clear_hint`; (c) `failed_view_blocks_writing_with_its_text`; (d) `stale_result_is_not_written` (Modell geändert zwischen Prüfung und Schreiben → neue Prüfung, kein altes Ergebnis); (e) `existing_file_stays_on_error_and_cancel`; (f) `extension_is_normalized`; (g) `mesh_bodies_are_named_and_skipped`; (h) `cards_use_full_width_and_equal_heights`; (i) `window_stays_open_while_writing` (wie die STEP-Tests); (j) `format_list_has_seven_entries_in_order`.
  Dazu in `planora-drawing/tests/stage17_file.rs`: `atomic_write_keeps_old_file_on_cancel`, `temp_file_is_removed_on_error`.
- [ ] **Step 2: Run → FAIL.** **Step 3: Implementieren.** **Step 4: Run → PASS** (`cargo test -p planora-app -p planora-drawing`), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise:** Teilweises Schreiben bei Fehler zulassen → (c) rot; Schlüssel ohne `Zeichnung` → (d) rot.
- [ ] **Step 6: Commit** — `feat: Export der Zeichnung als SVG und DXF im Export-Fenster`.

---

### Task 18: Dokumentation, Handtest-Proben und Übergabe

**Files:**
- Modify: `README.md` (Abschnitt Zeichnungen: was geht, was nicht), `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt 11“ mit Grenzen und Nachträgen), `docs/superpowers/specs/2026-10-14-zeichnungen-ansichten-design.md` (Abweichungen F1–F9 als Abschnitt „Festlegungen der Umsetzung“, `Progress` aus `planora-exchange`, Striche statt `curve2.rs`, Kippabstimmung statt Einzelstrahl, Bilder-Platzierung `Placement`)
- Create: `crates/planora-drawing/tests/hand_samples.rs` (`#[ignore]`, schreibt Proben nach `$PLANORA_SAMPLES_DIR` oder `D:\Testumgebung\zeichnung-proben`)

**Interfaces / Schritte:**
- [ ] **Step 1: Proben-Test** `write_samples_for_the_hand_test` (ignoriert, ausdrücklich per `cargo test -p planora-drawing --test hand_samples -- --ignored`): schreibt je Probe SVG und DXF in A3 quer, Erstwinkel, Maßstab 1:1 bzw. passend: `Quader` (10×20×30), `Zylinder` (r 5, h 12), `Kugel` (r 7), `Kegelstumpf`, `Bohrung_X` (40×20×30, Loch Ø10 längs X: zwei gestrichelte Linien), `Bohrung_Y`, `Winkel` (L-Profil), `Tasche`, `Kreuzbohrung`, `Rundung`, `Torus`, `Freiform`; je Probe Vorne/Oben/Links + Isometrisch; dazu `Dritter_Winkel` (Quader mit Bohrung, Dritter Winkel). Zusätzlich eine Textdatei `ERWARTUNG.txt` mit den erwarteten Gesamtlängen sichtbar/verdeckt je Probe (aus den Formeln der Tests).
- [ ] **Step 2: Doku:** README (kurz, Laiensprache, wie im Abschnitt zu STEP), offene-punkte (Grenzen aus Spec §10, außerdem: Umrisse von Torus/Freiform angenähert; Streifstrahlen → benannte Meldung; glatte Kanten ungezeichnet; keine Maße/Mittellinien/Schnitte; Netz-Körper fehlen; Handtest-Stand), Spec-Abschnitt „Festlegungen der Umsetzung“.
- [ ] **Step 3: Alle Gates** vom Controller bzw. Implementierer: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (Dauer notieren), `cargo deny check`, Prüfung der geschützten Pfade: `git diff --stat 945fa67 -- crates/planora-kernel crates/planora-mesh crates/planora-step crates/planora-exchange` muss **leer** sein.
- [ ] **Step 4: Commit** — `docs: Zeichnungen (Ansichten), Proben für den Handtest, Grenzen`.
- [ ] **Step 5 (Controller, nach Rückfrage beim Nutzer):** Handtest – Proben in Fusion 360 (DXF als Skizze einfügen) und im Browser/Inkscape (SVG) öffnen; Maße gegenprüfen (Länge einer Kante, Kreisdurchmesser, Strichelung der verdeckten Linien). Ergebnis in `offene-punkte.md` nachtragen (eigener `docs:`-Commit).
- [ ] **Step 6 (Controller):** Schlussprüfung des gesamten Zweigs (opus), Nacharbeit, dann Meldung „fertig, Merge-Entscheidung beim Nutzer“.

---

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung:** §1 Z1–Z9 → Aufgaben 1–17 (Z1/Z7/Z8: 1, 15; Z2: 2–4, 9–11; Z3: 5–6; Z4: 14, 17; Z5/Z6: 12–16; Z9: Global Constraints). §2 Aufbau → Dateistruktur. §3 Datenmodell → 1, 12, 13. §4.1 Kanten → 2; §4.2 Umrisse → 4, 7; §4.3 Teilen/Sichtbarkeit → 3, 6, 8 (mit F9); §4.4 → Global Constraints, 6, 8. §5 Ausgabe → 9–11. §6 Oberfläche → 14–17. §7 Fehler → 1. §8 Prüfung → 2–11 (eigene Werkzeuge, Mutationsbeweise, Serie in 8), 18. §9 Stufen → Aufgaben. §10 Grenzen → 18.
**Platzhalter-Scan:** Aufgabe 5 und 8 nennen je einen Test, den der Implementierer **ausschreibt** (mit Rechenvorgabe); alle übrigen Tests tragen Code oder eine numerische Vorgabe. Aufgaben 12–17 geben die Tests als genau beschriebene Fälle (Eingabe → erwartetes Ergebnis); die Oberflächentests folgen dem Stil der vorhandenen Dateien.
**Typkonsistenz:** `Stroke`, `StrokeOrigin`, `Basis`, `ViewLines`, `Piece`, `Visibility`, `SheetModel`, `Out2d`, `DrawingFormat`, `WorkArea` heißen in allen Aufgaben gleich; `model_edit_count` (12) ist der Schlüssel in 14; `Placement::Aligned`/`align_dir` (1) wird in 9, 15 benutzt; `FORMAT_VERSION = 4` (13).
**Bekannte Risiken des Plans:** Aufgabe 6 (Kippabstimmung, Kalibrierung `TILT`), Aufgabe 7 (Freiform-Umriss, Pole), Aufgabe 11 (DXF-Pflichtteile ohne Fremdprogramm bis zum Handtest). Der Controller setzt für 3, 4, 5, 6, 7, 8, 13, 14 den fähigsten Implementierer (opus) ein; 1, 2, 9, 10, 11, 12, 15, 16, 17, 18 ein mittleres Modell (sonnet) mit opus-Prüfung bei 11 und 15.
