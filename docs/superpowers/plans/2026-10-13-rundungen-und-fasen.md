# PlanoraCAD Rundungen und Fasen – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kanten eines Körpers runden (konstanter und variabler Radius) und fasen (gleicher Abstand, zwei Abstände, Abstand und Winkel), an allen Flächenarten, mit Ketten, Tangentenkette und Ecken im Stil „rollende Kugel“, als Feature `Verrunden` in Zeitleiste, App und Dateiformat 2.

**Architecture:** Neuer Kernel-Ordner `planora-kernel/src/blend/`: Nachbarschaft und Kantenwinkel (`topo`), Querschnitt per Newton aus `implicit`/`project`/`normal` (`section`), analytische Rundungs- und Fasenflächen (`analytic`), Endabschlüsse und Umbau der Nachbarflächen zu `Piece`s mit `sew` (`build`), Ketten (`chain`), Ecken (`corner`) und die Bahn mit Hautfläche für Freiformflächen und variablen Radius (`march`). `planora-math` bekommt die Hautfläche (`nurbs/skin.rs`) und den Radienspline (`radius.rs`). Der Kernel liefert den neuen Körper und eine Schätzung der Volumenänderung; die Volumenprobe mit Netz macht `planora-feature` (der Kernel kann `planora-mesh` nicht benutzen). Feature, App, Szene und Datei folgen dem Muster der Booleschen Operation.

**Tech Stack:** Rust (Edition 2024), bestehende Crates (`glam`, `thiserror`, `serde`, `egui`); **keine neue Abhängigkeit**.

**Spec:** `docs/superpowers/specs/2026-10-13-rundungen-und-fasen-design.md` (bindend). Lernstand: `docs/superpowers/specs/2026-10-11-nurbs-design.md` §13 und `docs/superpowers/offene-punkte.md`.

## Entscheidungen zur Spec (Planungsstand 2026-10-13)

Sie konkretisieren die Spec; Abweichungen werden in Task 20 in die Spec (neuer Abschnitt §12 „Änderungen bei der Umsetzung“) eingetragen.

- **P1 – Kernel-Schnittstelle:** `Kernel::blend(&self, solid: &Solid, request: &BlendRequest) -> Result<Blended, BlendError>` und die freie Funktion `planora_kernel::blend::blend(solid, request, tol)`. `BlendRequest { edges: Vec<BlendEdge>, kind: BlendKind, tangent_chain: bool }`, `BlendEdge { edge: Id<Edge>, first_face: Option<Id<Face>> }`, `Blended { solid: Solid, volume_change: f64, edges: Vec<Id<Edge>> }`.
- **P2 – Volumenprobe (Spec §4.5):** Der Kernel kann nicht vernetzen (`planora-mesh` hängt vom Kernel ab). Er liefert `volume_change`, eine Schätzung aus Querschnittsfläche × Weg (Prisma bzw. Pappus, Ecken mit geschlossenen Formeln). `planora-feature` vernetzt Eingang und Ergebnis mit gemeinsamer Teilung (`Discretizer::for_solids`) und prüft mit `planora_mesh::blend_volume_plausible`: Vorzeichen passend zu konvex/konkav und `|ΔV − Schätzung| ≤ 0,5 · |Schätzung| + 2 · Sehnentoleranz · gekrümmte Oberfläche`. Meldung `FeatureError::ImplausibleBlend`.
- **P3 – Eine Rundungsfläche je Kante:** Entlang einer glatten Kette bekommt jede Kante ihre eigene Rundungsfläche (analytisch: Zylinder bzw. Torus); benachbarte Flächen teilen den Querschnitt an der glatten Ecke als gemeinsame Kante. Das ist die „Fläche je glattem Abschnitt“ aus §4.2, ohne Flächen zusammenzufassen.
- **P4 – Fasenmaße:** Abstand = Sehnenabstand vom Kantenpunkt zum Berührpunkt im Querschnitt (bei Ebenen gleich dem Abstand auf der Fläche). Fläche 1 ist die Fläche der Marke `EdgeRef.a` (Kernel: `BlendEdge::first_face`). Winkel im Feature und in der Datei in Grad, im Kernel im Bogenmaß; erlaubt 1° bis 179°.
- **P5 – Endabschlüsse:** Analytische Rundungsflächen werden an der dritten Fläche über `intersect_surfaces` abgeschlossen; nicht unterstützte Paare ergeben eine benannte Ablehnung. Hautflächen (Stufe 3) schließen nur an Ebenen senkrecht zur Kante, an Nähten, glatten Übergängen und Ecken ab; schräge Abschlüsse benannt abgelehnt.
- **P6 – Ecken:** Zwei gerundete Kanten an einer Ecke (dritte Kante ungerundet) treffen sich an der winkelhalbierenden Ebene (Gehrung); das Ergebnis ist die Schnittmenge der einzeln gerundeten Körper. Drei Kanten mit gleichem Radius: Kugelstück um den Punkt im Abstand r von allen drei Ebenen (Stufe 2 nur rechtwinklig, Stufe 3 beliebige drei Ebenen). Alles andere benannt abgelehnt (Spec R7).
- **P7 – Dateiformat:** `FORMAT_VERSION = 2` ist die höchste lesbare Version. Eine Versionsdatei wird mit der kleinsten passenden Version geschrieben (`1` ohne `Verrunden`, `2` mit). So bleiben die Golden-Dateien für Format 1 unverändert (Spec §7, ihre Tests vergleichen den geschriebenen Text), und ältere Programme lesen Dokumente ohne Rundung weiter; mit Rundung melden sie „zu neu“ (R11). `dokument.json` und `.autosave.json` behalten ihre Struktur und schreiben weiter `format: 1` (`CONTAINER_FORMAT`). Eine Versionsdatei mit `format: 1` und einer Rundung ist beschädigt. Der Test `newer_files_with_changed_structure_report_too_new` nutzt bisher `2` als „zu neu“; er wird mechanisch auf `3` gesetzt (mit Kommentar).
- **P8 – Variabler Radius:** Punkte `(Position in Prozent der Kettenlänge, Radius)`, mindestens 2, höchstens 64, Positionen streng steigend in `[0, 100]`; dazwischen monotone kubische Hermite-Interpolation (Fritsch–Butland), vor dem ersten und nach dem letzten Punkt konstant.
- **P9 – Rollen der neuen Flächen:** „Rundung N“ bzw. „Fase N“, N nach der Kantenliste nach Ausdehnung der Tangentenkette (je gewählte Kante ihre Kette in Laufrichtung, schon enthaltene Kanten entfallen); Eckstücke „Ecke N“ nach Eckpunkt-Id.
- **P10 – Parameterbereiche:** Beschnittene Flächen behalten ihren `patch` (wie bei Booleschen Operationen) und werden nur in nicht geschlossenen Richtungen erweitert, wenn die neue Schleife darüber hinausgeht; neue Flächen bekommen den Bereich ihrer Schleife.
- **P11 – Meldungen:** Die Meldung am Feature nennt den Featurenamen nicht selbst; Baum und Statuszeile zeigen „Name“: davor (wie bei allen Features). Bei Eingangskörpern mit Freiformflächen und Ursachen `SpineLost`, `Unsupported`, `TooExpensive` beginnt sie mit „Bei Körpern mit Freiformflächen:“.
- **P12 – Übergänge:** Stufe 1 lehnt mehrere Kanten und variable Radien benannt ab; die Übergangstests werden in Task 9 und Task 14 ersetzt (wie bei Teilprojekt 7).
- **P13 – Reihenfolge Datei:** Weil `match`-Stellen über `FeatureKind` erschöpfend sind, kommen `KindDto::Verrunden` und die Umwandlung schon in Task 7; Formatnummer, Golden-Datei und feindliche Dateien in Task 18.
- **P14 – Körpername:** Ein gerundeter Körper behält den Namen seines Eingangskörpers (wie beim Verschieben); das Feature heißt „Rundung n“ bzw. „Fase n“.

## Global Constraints

- Reines Rust, Edition 2024; Zahlen `f64`, Einheit Millimeter; zentrale `Tolerance` (linear `1e-6`, angular `1e-9`). Freiformtoleranzen wiederverwenden: `NURBS_FIT_TOL = 1e-5`, `NURBS_EDGE_TOL = 1e-5`, `NURBS_NODE_TOL = 2e-5` (über `pipeline_tol`). Keine freien Epsilon-Zahlen in neuem Code: jede Schranke ist eine benannte `const` mit Kommentar oder aus `Tolerance` abgeleitet.
- **Keine neue externe Abhängigkeit** (Spec R12, §11). `cargo deny check` bleibt grün; `deny.toml` nie selbstständig erweitern – bei Befund anhalten und den Menschen fragen. Projektlizenz GPL-3.0-only. Kein Autodesk-Material. Alle Texte und Meldungen deutsch.
- **Nicht lösbare Fälle werden benannt abgelehnt, nie ein falscher Körper** (R10): `BlendError` mit deutscher Meldung, Dokument unverändert, Folgefeatures `Skipped`. Keine Panics auf Nutzereingaben und Dateiinhalten.
- **Analytische Wege bleiben bitgleich:** Keine Änderung an `crates/planora-kernel/src/boolean/**`, `crates/planora-mesh/src/{tessellate,trimmed,discretize,check}.rs`. Prüfung am Ende jeder Task: `git diff --stat f2f748d -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs` ist leer.
- **Deterministische Arbeitsgrenzen, keine Uhrzeit:** Grenzen zählen Schritte oder NURBS-Auswertungen (`planora_math::nurbs::evaluations`), nie Zeit.
- **Tests:** Alle bisherigen Tests bleiben unverändert grün; einzige erlaubte Änderung ist die mechanische in P7 (mit Kommentar). Tests werden nie abgeschwächt (Schranken, Asserts, `#[ignore]`), um grün zu werden: Schlägt ein Test fehl, Ursache beheben oder anhalten und berichten. Jedes Testprogramm läuft im Debug-Build in höchstens etwa 30 s; längere Reihen bekommen `#[ignore = "… nur mit --release --ignored (etwa N s)"]`.
- **Kernel-Tests, die vernetzen, liegen in `crates/planora-mesh/tests/`** (sonst Abhängigkeitszyklus); reine Kernel-Tests in `crates/planora-kernel/tests/`.
- **Zufallstests** nur mit festem Startwert (`Lcg` wie in `planora-kernel/tests/nurbs_geometry.rs`), gegen geschlossene Formeln oder analytische Zwillinge; Lagetests nur an robusten Punkten (mindestens `0,1 · r` von jedem Rand).
- **Ehrlich berichten:** Jede Task endet mit einem Bericht, was geprüft wurde und was nicht; bekannte Grenzen werden genannt, nicht verschwiegen.
- Plattform Windows (MSVC) und Linux. Befehle in Bash (Git Bash) mit `export PATH="$PATH:/c/Users/denni/.cargo/bin"`; PowerShell 5.1 kennt `&&` nicht. Kein Text mit Apostroph in Bash-Heredocs; Dateien mit dem Write-Werkzeug schreiben.
- Am Ende jeder Task grün: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p <geänderte Crates>`; `cargo test --workspace` und `cargo deny check` mindestens am Ende jeder Stufe (Task 8, 12, 17, 20).
- Commits mit ausdrücklichen Pfaden (nie `git add -A`), Nachricht deutsch, Schlusszeile `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- **Keine Handprüfung durch Implementierer** (der Nutzer sitzt am Rechner); Task 21 nur nach Rücksprache.

## Review Focus

Eingaben, die die Spec andeutet, die aber leicht durchrutschen; jede Zeile hat einen Test in der genannten Task:

1. **Maß zu groß für Fläche oder Kante** (Radius größer als die halbe Flächenbreite, Kante kürzer als die Abschlüsse brauchen, Fase länger als die Nachbarfläche) → `BlendError::TooLarge` mit Flächennamen, Eingangskörper unverändert, kein „gültiger“ Körper mit verdrehten Schleifen. Test `too_large_sizes_are_refused_by_name` in Task 5.
2. **Kanten, an denen keine Rundung möglich ist:** Naht eines Zylinders, fast tangential anschließende Flächen (Langloch), Schneide, Fläche enger gekrümmt als der Radius, Kugelbahn zu nahe an der Drehachse → je eigene benannte Meldung. Tests `seams_and_tangent_edges_are_named` (Task 1), `curvature_and_axis_limits_are_named` (Task 3).
3. **Feindliche Zahlen** (NaN, ±∞, 0, negativ, 1e300, leere Kantenliste, doppelte Kanten, unsortierte oder zu viele Radienpunkte, Fasenwinkel 0°/180°) im Kernel, im Feature und in Dateien → nie Absturz, immer Meldung. Tests `hostile_requests_never_panic` (Task 5), `hostile_blend_parameters_become_error_status` (Task 7), `hostile_blend_files_are_rejected_or_named` (Task 18).
4. **Folgeoperationen und wiederholte Verrundung:** Boolesche Operation durch eine Rundungsfläche, zweite Rundung an einer anderen Kante, Rundung an einer Kante der Rundungsfläche (tangential → benannt), Rundung an einer schon verbrauchten Kante → gültig oder benannt. Tests `follow_up_operations_on_filleted_bodies` (Task 6), `rounding_a_rounded_edge_is_named` (Task 7), `corner_results_survive_follow_ups` (Task 12).
5. **Veraltete Verweise nach Änderungen davor** (Quader größer, Kante durch früheres Feature verschwunden, Feature davor unterdrückt) → Neuberechnung findet die Kante wieder oder meldet `EdgeNotFound` benannt, Folgefeatures `Skipped`. Test `blend_references_follow_upstream_changes` (Task 7).

## Dateistruktur

```
crates/planora-math/
├─ src/lib.rs                         (ändern: pub mod radius)
├─ src/radius.rs                      (neu, Task 14: RadiusLaw)
├─ src/nurbs/mod.rs                   (ändern: mod skin)
├─ src/nurbs/skin.rs                  (neu, Task 13: SkinSection, skin, v_boundary)
└─ tests/{nurbs_skin.rs, radius_law.rs}
crates/planora-kernel/
├─ src/lib.rs, src/kernel.rs          (ändern: pub mod blend, Kernel::blend)
├─ src/blend/mod.rs                   (neu: Typen, Einstieg `blend`)
├─ src/blend/{error,topo,section,analytic,build}.rs   (neu, Stufe 1)
├─ src/blend/{chain,corner}.rs        (neu, Stufe 2)
├─ src/blend/march.rs                 (neu, Stufe 3)
└─ tests/blend_{topo,section,analytic,build,api,chain,corner,march,freeform}.rs
crates/planora-mesh/
├─ src/volume.rs, src/lib.rs          (ändern: blend_volume_plausible, curved_area)
└─ tests/blend_common/mod.rs, tests/blend_{stage1,stage2,stage3}.rs
crates/planora-feature/src/{model,compute,recompute,refs,error,lib}.rs, tests/blend_features.rs
crates/planora-project/src/{format,convert,docstore,lib}.rs, tests/{blend_roundtrip,format2}.rs, tests/data/format2.json
crates/planora-render/src/{scene,lib}.rs, tests/scene_marks.rs
crates/planora-app/src/{blend_dialog.rs (neu), app,lib,workspace,state,viewport,properties,icons,browser,feature_dialog}.rs
crates/planora-app/tests/{blend_state,blend_ui}.rs
docs: README.md, docs/superpowers/offene-punkte.md, Spec §12
```

---

## Stufe 1 – Fase und konstanter Radius, einzelne Kanten, analytische Rundungsflächen

### Task 1: Kantennachbarschaft, Kantenwinkel und Konvexität

**Files:**
- Create: `crates/planora-kernel/src/blend/mod.rs`, `crates/planora-kernel/src/blend/error.rs`, `crates/planora-kernel/src/blend/topo.rs`
- Modify: `crates/planora-kernel/src/lib.rs` (Zeile 3–14: `pub mod blend;` einfügen)
- Test: `crates/planora-kernel/tests/blend_topo.rs`

**Interfaces:**
- Consumes: `Solid`, `Face`, `Edge`, `Vertex`, `Id`, `Solid::half_edge_start`, `Surface::{project, normal}`, `Curve::{point, tangent}` (alle vorhanden).
- Produces (alle `pub` in `planora_kernel::blend`):
  ```rust
  pub enum BlendError { InvalidParameter(String), NoEdges, EdgeNotFound, InvalidInput(String), SeamEdge,
      NearlyTangent { angle_deg: f64 }, TooLarge { face: String }, CurvatureTooSmall { face: String },
      SpineLost, Unsupported(String), Corner(String), TooExpensive, ResultInvalid(String) }
  pub mod topo {
      pub const NEAR_TANGENT_ANGLE: f64;
      pub struct EdgeUse { pub face: Id<Face>, pub loop_index: usize, pub position: usize, pub forward: bool }
      pub struct EdgeSides { pub a: EdgeUse, pub b: EdgeUse }
      pub struct EdgeAngle { pub convex: bool, pub interior: f64 }
      pub fn edge_uses(solid: &Solid, edge: Id<Edge>) -> Vec<EdgeUse>;
      pub fn edge_sides(solid: &Solid, edge: Id<Edge>, first_face: Option<Id<Face>>) -> Result<EdgeSides, BlendError>;
      pub fn outward_normal(face: &Face, p: DVec3) -> DVec3;
      pub fn edge_angle(solid: &Solid, edge: Id<Edge>, sides: &EdgeSides) -> Result<EdgeAngle, BlendError>;
      pub fn inward_directions(solid: &Solid, edge: Id<Edge>, sides: &EdgeSides, t: f64) -> (DVec3, DVec3);
      pub fn vertex_edges(solid: &Solid, v: Id<Vertex>) -> Vec<Id<Edge>>;
      pub fn vertex_faces(solid: &Solid, v: Id<Vertex>) -> Vec<Id<Face>>;
      pub fn face_name(solid: &Solid, face: Id<Face>) -> String;
  }
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-kernel/tests/blend_topo.rs`)

```rust
//! Nachbarschaft, Kantenwinkel und konvex/konkav (Teilprojekt 8, Task 1).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Kante zwischen den Flächen mit den Rollen `a` und `b` (Reihenfolge egal).
fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let names: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            names.len() == 2
                && ((names[0] == a && names[1] == b) || (names[0] == b && names[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn extrude_polygon(points: &[DVec2], h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::polygon(points).unwrap(),
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

/// L-Profil: konkave senkrechte Kante bei (4, 4) zwischen „Wand 3“ und „Wand 4“.
fn l_profile() -> Solid {
    extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    )
}

#[test]
fn a_box_edge_has_two_sides_and_a_right_convex_angle() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let sides = edge_sides(&s, e, None).unwrap();
    assert_ne!(sides.a.face, sides.b.face);
    let angle = edge_angle(&s, e, &sides).unwrap();
    assert!(angle.convex);
    assert!((angle.interior - PI / 2.0).abs() < 1e-12, "{}", angle.interior);
    // Richtungen in die Flächen hinein stehen senkrecht auf der Kante und zeigen ins Material.
    let edge = &s.edges[e];
    let (wa, wb) = inward_directions(&s, e, &sides, 0.5 * (edge.t0 + edge.t1));
    let dir = edge.curve.tangent(edge.t0).normalize();
    assert!(wa.dot(dir).abs() < 1e-12 && wb.dot(dir).abs() < 1e-12);
    let mid = DVec3::new(5.0, 0.0, 10.0);
    for w in [wa, wb] {
        let p = mid + w * 0.1;
        assert!(p.x > 0.0 && p.y >= -1e-12 && p.z <= 10.0 + 1e-12, "{p:?}");
    }
}

#[test]
fn the_first_face_decides_which_side_is_a() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let uses = edge_uses(&s, e);
    let first = uses[1].face;
    let sides = edge_sides(&s, e, Some(first)).unwrap();
    assert_eq!(sides.a.face, first);
    let unchanged = edge_sides(&s, e, None).unwrap();
    assert_eq!(unchanged.a.face, uses[0].face);
}

#[test]
fn the_inner_edge_of_an_l_profile_is_concave() {
    let s = l_profile();
    let e = edge_between(&s, "Wand 3", "Wand 4");
    let sides = edge_sides(&s, e, None).unwrap();
    let angle = edge_angle(&s, e, &sides).unwrap();
    assert!(!angle.convex);
    assert!((angle.interior - 1.5 * PI).abs() < 1e-12, "{}", angle.interior);
    let outer = edge_between(&s, "Wand 1", "Wand 2");
    let sides = edge_sides(&s, outer, None).unwrap();
    assert!(edge_angle(&s, outer, &sides).unwrap().convex);
}

#[test]
fn seams_and_tangent_edges_are_named() {
    // Naht des Zylindermantels: dieselbe Fläche auf beiden Seiten.
    let c = k().make_cylinder(3.0, 5.0).unwrap();
    let seam = c
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Line { .. }))
        .map(|(id, _)| id)
        .unwrap();
    assert_eq!(edge_sides(&c, seam, None), Err(BlendError::SeamEdge));
    // Langloch: Gerade geht tangential in den Bogen über.
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(10.0, 0.0) },
                Segment::Arc { center: DVec2::new(10.0, 2.0), sweep: PI },
                Segment::Line { to: DVec2::new(0.0, 4.0) },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let slot = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let e = edge_between(&slot, "Wand 1", "Wand 2");
    let sides = edge_sides(&slot, e, None).unwrap();
    match edge_angle(&slot, e, &sides) {
        Err(BlendError::NearlyTangent { angle_deg }) => {
            assert!((angle_deg - 180.0).abs() < 1e-6, "{angle_deg}")
        }
        other => panic!("{other:?}"),
    }
    let msg = BlendError::NearlyTangent { angle_deg: 180.0 }.to_string();
    assert!(msg.contains("tangential") && msg.contains("180.0"), "{msg}");
}

#[test]
fn foreign_edges_and_vertices_are_reported() {
    let s = k().make_box(DVec3::ONE).unwrap();
    assert_eq!(edge_sides(&s, Id::from_index(999), None), Err(BlendError::EdgeNotFound));
    // Jede Ecke eines Quaders hat drei Kanten und drei Flächen.
    for (v, _) in s.vertices.iter() {
        assert_eq!(vertex_edges(&s, v).len(), 3);
        assert_eq!(vertex_faces(&s, v).len(), 3);
    }
    let _ = Tolerance::default();
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_topo`
Expected: FAIL – `could not find blend in planora_kernel`.

- [ ] **Step 3: Fehlertyp schreiben** (`crates/planora-kernel/src/blend/error.rs`)

```rust
//! Fehler der Rundungen und Fasen (Meldungen deutsch, für die Oberfläche bestimmt).

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum BlendError {
    #[error("Ungültiges Maß: {0}")]
    InvalidParameter(String),
    #[error("Es ist keine Kante gewählt")]
    NoEdges,
    #[error("Eine gewählte Kante gehört nicht zu diesem Körper")]
    EdgeNotFound,
    #[error("Der Körper ist ungültig: {0}")]
    InvalidInput(String),
    #[error("Die Kante ist die Naht einer gekrümmten Fläche und lässt sich nicht runden")]
    SeamEdge,
    #[error(
        "Die Flächen an der Kante gehen fast tangential ineinander über (Kantenwinkel {angle_deg:.1}°); dort ist keine Rundung oder Fase möglich"
    )]
    NearlyTangent { angle_deg: f64 },
    #[error("Das Maß ist zu groß für die Fläche „{face}“; bitte ein kleineres Maß wählen")]
    TooLarge { face: String },
    #[error(
        "Die Fläche „{face}“ ist enger gekrümmt als der Radius; bitte einen kleineren Radius wählen"
    )]
    CurvatureTooSmall { face: String },
    #[error("Die Kugel findet entlang der Kante keine Lage (die Bahn bricht ab)")]
    SpineLost,
    #[error("{0}")]
    Unsupported(String),
    #[error("Diese Ecke lässt sich nicht runden: {0}")]
    Corner(String),
    #[error(
        "Die Berechnung der Rundung ist zu aufwendig (Arbeitsgrenze erreicht); bitte weniger Kanten oder eine einfachere Form verwenden"
    )]
    TooExpensive,
    #[error("Das Ergebnis der Rundung ist ungültig: {0}")]
    ResultInvalid(String),
}
```

- [ ] **Step 4: Nachbarschaft schreiben** (`crates/planora-kernel/src/blend/topo.rs`)

```rust
//! Nachbarschaft einer Kante (Spec §3.1, §4.1): angrenzende Flächen und ihre Lage in deren
//! Schleifen, Kantenwinkel, konvex/konkav, Kanten und Flächen an einer Ecke.

use super::error::BlendError;
use crate::ids::Id;
use crate::topology::{Edge, Face, Solid, Vertex};
use planora_math::DVec3;
use std::f64::consts::PI;

/// Ist `|nA × nB|` kleiner als der Sinus dieses Winkels (2°), gehen die Nachbarflächen fast
/// tangential ineinander über (Kantenwinkel nahe 180°) oder bilden eine Schneide (nahe 0°).
/// Die Kugel hätte dort keine sichere Lage (Spec §3.4).
pub const NEAR_TANGENT_ANGLE: f64 = 2.0 * PI / 180.0;

/// Stellen (Bruchteile des Kantenbereichs), an denen Kantenwinkel und Konvexität geprüft werden;
/// die mittlere liefert den gemeldeten Winkel.
const ANGLE_PROBES: [f64; 3] = [0.25, 0.5, 0.75];

/// Verwendung einer Kante in einer Schleife einer Fläche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeUse {
    pub face: Id<Face>,
    /// 0 = äußere Schleife, `k + 1` = Loch `k`.
    pub loop_index: usize,
    /// Stelle der Halbkante in der Schleife.
    pub position: usize,
    /// Läuft die Halbkante in Kantenrichtung (`start → end`)?
    pub forward: bool,
}

/// Die beiden Seiten einer Kante: Fläche 1 (`a`) und Fläche 2 (`b`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeSides {
    pub a: EdgeUse,
    pub b: EdgeUse,
}

/// Kantenwinkel an der Kantenmitte.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeAngle {
    /// Konvex: Das Material bildet an der Kante einen Winkel unter 180° (Rundung trägt ab).
    pub convex: bool,
    /// Innenwinkel des Materials (Bogenmaß; unter π konvex, über π konkav).
    pub interior: f64,
}

/// Alle Verwendungen der Kante, nach Fläche, Schleife und Stelle geordnet.
pub fn edge_uses(solid: &Solid, edge: Id<Edge>) -> Vec<EdgeUse> {
    let mut out = Vec::new();
    for (fid, f) in solid.faces.iter() {
        for (li, lp) in std::iter::once(&f.outer).chain(f.inner.iter()).enumerate() {
            for (position, he) in lp.half_edges.iter().enumerate() {
                if he.edge == edge {
                    out.push(EdgeUse {
                        face: fid,
                        loop_index: li,
                        position,
                        forward: he.forward,
                    });
                }
            }
        }
    }
    out
}

/// Die beiden Seiten der Kante. `first_face` legt Fläche 1 fest (sonst die erste Verwendung).
/// Fehler: Kante nicht im Körper, nicht genau zwei Verwendungen, Naht (beide Seiten dieselbe
/// Fläche).
pub fn edge_sides(
    solid: &Solid,
    edge: Id<Edge>,
    first_face: Option<Id<Face>>,
) -> Result<EdgeSides, BlendError> {
    if solid.edges.get(edge).is_none() {
        return Err(BlendError::EdgeNotFound);
    }
    let uses = edge_uses(solid, edge);
    if uses.len() != 2 {
        return Err(BlendError::InvalidInput(format!(
            "Die Kante wird {}-mal statt zweimal verwendet",
            uses.len()
        )));
    }
    if uses[0].face == uses[1].face {
        return Err(BlendError::SeamEdge);
    }
    let (a, b) = match first_face {
        Some(f) if uses[1].face == f => (uses[1], uses[0]),
        _ => (uses[0], uses[1]),
    };
    Ok(EdgeSides { a, b })
}

/// Außennormale der Fläche an einem Punkt auf ihr.
pub fn outward_normal(face: &Face, p: DVec3) -> DVec3 {
    let (u, v) = face.surface.project(p);
    let n = face.surface.normal(u, v);
    if face.same_sense { n } else { -n }
}

/// Einheitstangente der Kante in Kantenrichtung (`start → end`) beim Kurvenparameter `t`.
fn edge_tangent(e: &Edge, t: f64) -> DVec3 {
    (e.curve.tangent(t) * (e.t1 - e.t0).signum()).normalize_or_zero()
}

/// Kantenwinkel und konvex/konkav, geprüft an drei Stellen. Konvex heißt
/// `(nA × nB) · T_A > 0` mit `T_A` = Laufrichtung der Halbkante in Fläche A (Schleifen laufen
/// gegen den Uhrzeigersinn um die Außennormale). Fast tangentiale Flächen oder eine Schneide
/// ergeben `NearlyTangent`, ein Wechsel zwischen konvex und konkav `Unsupported`.
pub fn edge_angle(
    solid: &Solid,
    edge: Id<Edge>,
    sides: &EdgeSides,
) -> Result<EdgeAngle, BlendError> {
    let e = &solid.edges[edge];
    let (fa, fb) = (&solid.faces[sides.a.face], &solid.faces[sides.b.face]);
    let mut first: Option<bool> = None;
    let mut middle: Option<EdgeAngle> = None;
    for f in ANGLE_PROBES {
        let t = e.t0 + (e.t1 - e.t0) * f;
        let p = e.curve.point(t);
        let tangent = edge_tangent(e, t);
        let ta = if sides.a.forward { tangent } else { -tangent };
        let (na, nb) = (outward_normal(fa, p), outward_normal(fb, p));
        let cross = na.cross(nb);
        let phi = na.dot(nb).clamp(-1.0, 1.0).acos();
        let convex = cross.dot(ta) > 0.0;
        let interior = if convex { PI - phi } else { PI + phi };
        // NaN (Tangente null, Normale null) zählt wie „fast tangential“.
        let sharp = cross.length() >= NEAR_TANGENT_ANGLE.sin() && ta.length() > 0.5;
        if !sharp {
            return Err(BlendError::NearlyTangent {
                angle_deg: interior.to_degrees(),
            });
        }
        match first {
            None => first = Some(convex),
            Some(c) if c != convex => {
                return Err(BlendError::Unsupported(
                    "Die Kante wechselt zwischen konvex und konkav; das wird nicht unterstützt"
                        .into(),
                ));
            }
            Some(_) => {}
        }
        if f == 0.5 {
            middle = Some(EdgeAngle { convex, interior });
        }
    }
    middle.ok_or(BlendError::SpineLost)
}

/// Richtungen von der Kante in Fläche A bzw. B hinein (senkrecht zur Kante, in der Tangentialebene
/// der Fläche) beim Kurvenparameter `t`. Das Innere einer Fläche liegt links ihrer Schleife,
/// also in Richtung `n × T`; Fläche B durchläuft die Kante entgegengesetzt.
pub fn inward_directions(solid: &Solid, edge: Id<Edge>, sides: &EdgeSides, t: f64) -> (DVec3, DVec3) {
    let e = &solid.edges[edge];
    let p = e.curve.point(t);
    let tangent = edge_tangent(e, t);
    let ta = if sides.a.forward { tangent } else { -tangent };
    let na = outward_normal(&solid.faces[sides.a.face], p);
    let nb = outward_normal(&solid.faces[sides.b.face], p);
    (
        na.cross(ta).normalize_or_zero(),
        nb.cross(-ta).normalize_or_zero(),
    )
}

/// Kanten mit `v` als Anfang oder Ende (aufsteigend nach Id).
pub fn vertex_edges(solid: &Solid, v: Id<Vertex>) -> Vec<Id<Edge>> {
    solid
        .edges
        .iter()
        .filter(|(_, e)| e.start == v || e.end == v)
        .map(|(id, _)| id)
        .collect()
}

/// Flächen, deren Schleifen durch `v` laufen (aufsteigend, ohne Doppelte).
pub fn vertex_faces(solid: &Solid, v: Id<Vertex>) -> Vec<Id<Face>> {
    solid
        .faces
        .iter()
        .filter(|(_, f)| {
            std::iter::once(&f.outer)
                .chain(f.inner.iter())
                .flat_map(|lp| lp.half_edges.iter())
                .any(|he| solid.half_edge_start(*he) == v)
        })
        .map(|(id, _)| id)
        .collect()
}

/// Name einer Fläche für Meldungen: Rolle der Marke, sonst „Fläche n“.
pub fn face_name(solid: &Solid, face: Id<Face>) -> String {
    solid
        .faces
        .get(face)
        .and_then(|f| f.tag.as_ref())
        .map(|t| t.role.clone())
        .unwrap_or_else(|| format!("Fläche {}", face.index() + 1))
}
```

- [ ] **Step 5: Modul anlegen** (`crates/planora-kernel/src/blend/mod.rs`) und in `crates/planora-kernel/src/lib.rs` nach `pub mod boolean;` die Zeile `pub mod blend;` einfügen.

```rust
//! Rundungen und Fasen (Teilprojekt 8): rollende Kugel, analytische Rundungsflächen, Umbau der
//! Nachbarflächen über `sew` (Spec `2026-10-13-rundungen-und-fasen-design.md`).

pub mod error;
pub mod topo;

pub use error::BlendError;
```

- [ ] **Step 6: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_topo`
Expected: PASS (5 Tests). Der Test zur Meldung prüft, dass `{angle_deg:.1}` mit Punkt formatiert wird (Rust-Format), damit niemand ein Komma erwartet.

- [ ] **Step 7: Prüfen und committen**

Run: `cargo fmt && cargo clippy -p planora-kernel --all-targets -- -D warnings && cargo test -p planora-kernel`
Expected: alles grün, keine Warnung.

```bash
git add crates/planora-kernel/src/lib.rs crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/src/blend/error.rs crates/planora-kernel/src/blend/topo.rs crates/planora-kernel/tests/blend_topo.rs
git commit -m "kernel: Kantennachbarschaft, Kantenwinkel und Konvexität für Rundungen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Querschnitt per Newton (Kugelbahn) und Fasenpunkte

**Files:**
- Create: `crates/planora-kernel/src/blend/section.rs`
- Modify: `crates/planora-kernel/src/blend/mod.rs` (`pub mod section;`)
- Test: `crates/planora-kernel/tests/blend_section.rs`

**Interfaces:**
- Consumes: `Surface::{implicit, project, normal}`, `BlendError` (Task 1).
- Produces:
  ```rust
  pub mod section {
      pub const SECTION_NEWTON_STEPS: usize;           // 60
      pub const SECTION_RESIDUAL_REL: f64;             // 1e-12
      pub struct Side<'a> { pub surface: &'a Surface, pub same_sense: bool }
      impl Side<'_> { pub fn outside(&self, p: DVec3) -> f64; pub fn normal_at(&self, p: DVec3) -> DVec3; }
      pub struct Section { pub center: DVec3, pub pa: DVec3, pub pb: DVec3 }
      pub enum ChamferSpec { Distances { a: f64, b: f64 }, DistanceAngle { distance: f64, angle: f64 } }
      pub fn fillet_section(a: Side, b: Side, o: DVec3, tangent: DVec3, convex: bool, radius: f64, start: Option<DVec3>) -> Result<Section, BlendError>;
      pub fn chamfer_section(a: Side, b: Side, o: DVec3, tangent: DVec3, convex: bool, wa: DVec3, wb: DVec3, spec: ChamferSpec) -> Result<Section, BlendError>;
      pub fn check_curvature(side: Side, p: DVec3, convex: bool, radius: f64, face: &str) -> Result<(), BlendError>;
  }
  ```
  Regeln: `Section::center` ist bei Fasen der Kantenpunkt `o`. Ergebnisse liegen in der Ebene durch `o` senkrecht zu `tangent`. Konvex: Kugel im Material (Außenabstand `−r`), konkav: außerhalb (`+r`).

- [ ] **Step 1: Tests schreiben** (`crates/planora-kernel/tests/blend_section.rs`)

```rust
//! Querschnitt der Rundung und der Fase (Teilprojekt 8, Task 2).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::section::*;
use planora_kernel::Surface;
use planora_math::{DVec3, Frame};

fn plane(origin: DVec3, normal: DVec3) -> Surface {
    Surface::Plane {
        frame: Frame::from_origin_normal(origin, normal),
    }
}

fn side(s: &Surface) -> Side<'_> {
    Side { surface: s, same_sense: true }
}

#[test]
fn two_perpendicular_planes_give_the_textbook_ball() {
    // Deckel z = 10 (außen +z), Vorderwand y = 0 (außen −y), Kante längs x.
    let top = plane(DVec3::new(0.0, 0.0, 10.0), DVec3::Z);
    let front = plane(DVec3::ZERO, -DVec3::Y);
    let o = DVec3::new(3.0, 0.0, 10.0);
    let s = fillet_section(side(&top), side(&front), o, DVec3::X, true, 2.0, None).unwrap();
    assert!(s.center.distance(DVec3::new(3.0, 2.0, 8.0)) < 1e-12, "{:?}", s.center);
    assert!(s.pa.distance(DVec3::new(3.0, 2.0, 10.0)) < 1e-12, "{:?}", s.pa);
    assert!(s.pb.distance(DVec3::new(3.0, 0.0, 8.0)) < 1e-12, "{:?}", s.pb);
    // Konkav: Kugel außerhalb des Materials.
    let c = fillet_section(side(&top), side(&front), o, DVec3::X, false, 2.0, None).unwrap();
    assert!(c.center.distance(DVec3::new(3.0, -2.0, 12.0)) < 1e-12, "{:?}", c.center);
}

#[test]
fn plane_and_cylinder_and_sphere_converge_to_the_exact_center() {
    // Zylinder r = 5 (Achse z), Deckel z = 8: Kugel r = 1 bei ρ = 4, z = 7.
    let cyl = Surface::Cylinder { frame: Frame::WORLD, radius: 5.0 };
    let top = plane(DVec3::new(0.0, 0.0, 8.0), DVec3::Z);
    let o = DVec3::new(5.0, 0.0, 8.0);
    let s = fillet_section(side(&cyl), side(&top), o, DVec3::Y, true, 1.0, None).unwrap();
    assert!(s.center.distance(DVec3::new(4.0, 0.0, 7.0)) < 1e-10, "{:?}", s.center);
    // Halbkugel R = 5 auf der Ebene z = 0 (außen −z): Kugel r = 1 berührt die Kugel innen.
    let sphere = Surface::Sphere { frame: Frame::WORLD, radius: 5.0 };
    let base = plane(DVec3::ZERO, -DVec3::Z);
    let o = DVec3::new(5.0, 0.0, 0.0);
    let s = fillet_section(side(&sphere), side(&base), o, DVec3::Y, true, 1.0, None).unwrap();
    let want = DVec3::new((16.0f64 - 1.0).sqrt(), 0.0, 1.0);
    assert!(s.center.distance(want) < 1e-10, "{:?}", s.center);
    assert!((s.pa.length() - 5.0).abs() < 1e-10);
    assert!(s.pb.z.abs() < 1e-10);
}

#[test]
fn chamfer_points_by_distance_and_by_angle() {
    let top = plane(DVec3::new(0.0, 0.0, 10.0), DVec3::Z);
    let front = plane(DVec3::ZERO, -DVec3::Y);
    let o = DVec3::new(3.0, 0.0, 10.0);
    let (wa, wb) = (DVec3::Y, -DVec3::Z);
    let s = chamfer_section(
        side(&top),
        side(&front),
        o,
        DVec3::X,
        true,
        wa,
        wb,
        ChamferSpec::Distances { a: 1.0, b: 2.5 },
    )
    .unwrap();
    assert!(s.pa.distance(o + DVec3::Y) < 1e-12);
    assert!(s.pb.distance(o - DVec3::Z * 2.5) < 1e-12);
    assert_eq!(s.center, o);
    let angle = 30f64.to_radians();
    let s = chamfer_section(
        side(&top),
        side(&front),
        o,
        DVec3::X,
        true,
        wa,
        wb,
        ChamferSpec::DistanceAngle { distance: 1.0, angle },
    )
    .unwrap();
    assert!(s.pb.distance(o - DVec3::Z * angle.tan()) < 1e-10, "{:?}", s.pb);
    // Ein Winkel, dessen Gerade die zweite Fläche nicht trifft, wird benannt abgelehnt.
    let r = chamfer_section(
        side(&top),
        side(&front),
        o,
        DVec3::X,
        true,
        wa,
        wb,
        ChamferSpec::DistanceAngle { distance: 1.0, angle: 170f64.to_radians() },
    );
    assert!(matches!(r, Err(BlendError::InvalidParameter(m)) if m.contains("Winkel")), "{r:?}");
}

#[test]
fn opposite_normals_lose_the_spine_instead_of_dividing_by_zero() {
    let a = plane(DVec3::ZERO, DVec3::Z);
    let b = plane(DVec3::ZERO, -DVec3::Z);
    let r = fillet_section(side(&a), side(&b), DVec3::ZERO, DVec3::X, true, 1.0, None);
    assert_eq!(r, Err(BlendError::SpineLost));
}

#[test]
fn a_ball_larger_than_the_curvature_is_named() {
    // Zylinder r = 1, Kugel im Material (konvexe Kante, Mantel nach außen): r = 1,5 passt nicht.
    let cyl = Surface::Cylinder { frame: Frame::WORLD, radius: 1.0 };
    let r = check_curvature(side(&cyl), DVec3::new(1.0, 0.0, 0.0), true, 1.5, "Wand 1");
    assert_eq!(r, Err(BlendError::CurvatureTooSmall { face: "Wand 1".into() }));
    assert!(check_curvature(side(&cyl), DVec3::new(1.0, 0.0, 0.0), true, 0.5, "Wand 1").is_ok());
    // Kugel außerhalb des Zylinders (konkave Kante an einem Zapfen): keine Grenze.
    assert!(check_curvature(side(&cyl), DVec3::new(1.0, 0.0, 0.0), false, 5.0, "Wand 1").is_ok());
    // Bohrung (Mantel nach innen, same_sense falsch) und konkave Kante: Kugel liegt in der Bohrung.
    let hole = Side { surface: &cyl, same_sense: false };
    assert!(check_curvature(hole, DVec3::new(1.0, 0.0, 0.0), false, 1.5, "Loch").is_err());
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_section`
Expected: FAIL – `could not find section in blend`.

- [ ] **Step 3: Querschnitt schreiben** (`crates/planora-kernel/src/blend/section.rs`)

```rust
//! Querschnitt der Verrundung an einem Kantenpunkt (Spec §3.1, R3, R5): Der Kugelmittelpunkt
//! wird per Newton aus den vorzeichenbehafteten Abständen beider Flächen bestimmt (keine
//! Versatzfläche als Objekt); die Berührpunkte sind die Fußpunkte. Fasen: Punkt im
//! Sehnenabstand auf jeder Fläche oder Abstand und Winkel.

use super::error::BlendError;
use crate::geometry::Surface;
use planora_math::DVec3;

/// Höchstzahl Newton-Schritte je Querschnitt (deterministische Arbeitsgrenze).
pub const SECTION_NEWTON_STEPS: usize = 60;
/// Restfehler der Abstandsgleichungen relativ zum Maßstab `1 + |o| + r`, unter dem Newton
/// endet. Weit unter `tol.linear`; analytische Flächen erreichen ihn nach wenigen Schritten.
pub const SECTION_RESIDUAL_REL: f64 = 1e-12;
/// Kleinster Betrag der Jacobi-Determinante; darunter sind die Flächen im Querschnitt parallel.
const MIN_JACOBIAN: f64 = 1e-12;
/// Größter Newton-Schritt in Vielfachen des Maßes (Dämpfung gegen Sprünge auf fremde Lagen).
const MAX_STEP_FACTOR: f64 = 0.5;
/// Untergrenze für `1 + nA · nB` beim Startpunkt; darunter stehen die Normalen fast
/// entgegengesetzt (Schneide), der Start wäre unendlich weit weg.
const MIN_BISECTOR: f64 = 1e-6;
/// Kleinster Betrag von `∇B · Richtung` beim Strahl der Winkelfase; darunter läuft der Strahl
/// fast parallel zur Fläche B.
const MIN_RAY_SLOPE: f64 = 1e-9;
/// Spielraum der Krümmungsprüfung (relativ): Ein Radius gleich dem Krümmungsradius ist schon
/// zu groß (die Kugel fiele zu einem Punkt).
const CURVATURE_MARGIN: f64 = 1e-9;

/// Eine Seite der Kante: Trägerfläche mit Außenrichtung.
#[derive(Clone, Copy, Debug)]
pub struct Side<'a> {
    pub surface: &'a Surface,
    pub same_sense: bool,
}

impl Side<'_> {
    fn sign(&self) -> f64 {
        if self.same_sense { 1.0 } else { -1.0 }
    }

    /// Vorzeichenbehafteter Abstand, positiv außerhalb des Materials.
    pub fn outside(&self, p: DVec3) -> f64 {
        self.surface.implicit(p) * self.sign()
    }

    /// Außennormale am Fußpunkt von `p` (Gradient von [`Side::outside`]).
    pub fn normal_at(&self, p: DVec3) -> DVec3 {
        let (u, v) = self.surface.project(p);
        self.surface.normal(u, v) * self.sign()
    }
}

/// Querschnitt: Kugelmittelpunkt (bei Fasen der Kantenpunkt) und Berührpunkte auf A und B.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub center: DVec3,
    pub pa: DVec3,
    pub pb: DVec3,
}

/// Fasenmaße (Bogenmaß beim Winkel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChamferSpec {
    /// Sehnenabstand auf Fläche A und auf Fläche B.
    Distances { a: f64, b: f64 },
    /// Sehnenabstand auf Fläche A und Winkel der Fase zu Fläche A.
    DistanceAngle { distance: f64, angle: f64 },
}

/// Löst das 2×2-System `M · (x, y) = (f, g)`; `None` bei fast singulärer Matrix.
fn solve2(m: [[f64; 2]; 2], f: f64, g: f64) -> Option<(f64, f64)> {
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if !(det.abs() > MIN_JACOBIAN) {
        return None;
    }
    Some((
        (f * m[1][1] - g * m[0][1]) / det,
        (m[0][0] * g - m[1][0] * f) / det,
    ))
}

/// Begrenzt einen Schritt auf `max` (Länge).
fn damp(step: DVec3, max: f64) -> DVec3 {
    let len = step.length();
    if len > max { step * (max / len) } else { step }
}

/// Rundung: Kugelmittelpunkt in der Ebene durch `o` senkrecht zu `tangent` mit Abstand `radius`
/// zu beiden Flächen, im Material bei konvexen und außerhalb bei konkaven Kanten. `start` ist
/// ein Startwert (etwa der Mittelpunkt des vorigen Querschnitts); ohne ihn gilt die
/// Ebenenformel `c = o − s·r·(nA + nB)/(1 + nA·nB)` mit den Normalen an `o`.
pub fn fillet_section(
    a: Side,
    b: Side,
    o: DVec3,
    tangent: DVec3,
    convex: bool,
    radius: f64,
    start: Option<DVec3>,
) -> Result<Section, BlendError> {
    let t = tangent.normalize_or_zero();
    if t == DVec3::ZERO || !(radius > 0.0) {
        return Err(BlendError::SpineLost);
    }
    let (b1, b2) = t.any_orthonormal_pair();
    let target = if convex { -radius } else { radius };
    let mut c = match start {
        Some(c) => c - t * (c - o).dot(t),
        None => {
            let (na, nb) = (a.normal_at(o), b.normal_at(o));
            let denom = 1.0 + na.dot(nb);
            if !(denom > MIN_BISECTOR) {
                return Err(BlendError::SpineLost);
            }
            o + (na + nb) * (target / denom)
        }
    };
    let eps = SECTION_RESIDUAL_REL * (1.0 + o.abs().max_element() + radius);
    for _ in 0..SECTION_NEWTON_STEPS {
        let (da, db) = (a.outside(c), b.outside(c));
        let (fa, fb) = (da - target, db - target);
        if !(fa.is_finite() && fb.is_finite()) {
            return Err(BlendError::SpineLost);
        }
        let (ga, gb) = (a.normal_at(c), b.normal_at(c));
        if fa.abs() <= eps && fb.abs() <= eps {
            return Ok(Section {
                center: c,
                pa: c - ga * da,
                pb: c - gb * db,
            });
        }
        let m = [[ga.dot(b1), ga.dot(b2)], [gb.dot(b1), gb.dot(b2)]];
        let (x, y) = solve2(m, fa, fb).ok_or(BlendError::SpineLost)?;
        c -= damp(b1 * x + b2 * y, MAX_STEP_FACTOR * radius);
    }
    Err(BlendError::SpineLost)
}

/// Punkt auf Fläche `side` in der Ebene durch `o` senkrecht zu `t` im Sehnenabstand `d` von `o`,
/// Startrichtung `w` (in die Fläche hinein).
fn chord_point(side: Side, o: DVec3, t: DVec3, w: DVec3, d: f64) -> Result<DVec3, BlendError> {
    let (b1, b2) = t.any_orthonormal_pair();
    let eps = SECTION_RESIDUAL_REL * (1.0 + o.abs().max_element() + d);
    let mut p = o + w * d;
    for _ in 0..SECTION_NEWTON_STEPS {
        let rel = p - o;
        let (f, g) = (side.outside(p), rel.length() - d);
        if !(f.is_finite() && g.is_finite()) {
            return Err(BlendError::SpineLost);
        }
        if f.abs() <= eps && g.abs() <= eps {
            return Ok(p);
        }
        let n = side.normal_at(p);
        let dir = rel.normalize_or_zero();
        let m = [[n.dot(b1), n.dot(b2)], [dir.dot(b1), dir.dot(b2)]];
        let (x, y) = solve2(m, f, g).ok_or(BlendError::SpineLost)?;
        p -= damp(b1 * x + b2 * y, MAX_STEP_FACTOR * d);
    }
    Err(BlendError::SpineLost)
}

/// Fase: Berührpunkte im Sehnenabstand (Fläche A, Fläche B) oder Punkt auf A im Abstand und
/// Schnitt des Strahls unter dem Winkel zu A mit B. `wa`, `wb` zeigen von der Kante in die
/// Flächen hinein (siehe `topo::inward_directions`).
#[allow(clippy::too_many_arguments)]
pub fn chamfer_section(
    a: Side,
    b: Side,
    o: DVec3,
    tangent: DVec3,
    convex: bool,
    wa: DVec3,
    wb: DVec3,
    spec: ChamferSpec,
) -> Result<Section, BlendError> {
    let t = tangent.normalize_or_zero();
    if t == DVec3::ZERO {
        return Err(BlendError::SpineLost);
    }
    match spec {
        ChamferSpec::Distances { a: da, b: db } => Ok(Section {
            center: o,
            pa: chord_point(a, o, t, wa, da)?,
            pb: chord_point(b, o, t, wb, db)?,
        }),
        ChamferSpec::DistanceAngle { distance, angle } => {
            let pa = chord_point(a, o, t, wa, distance)?;
            let back = o - pa;
            let u = (back - t * back.dot(t)).normalize_or_zero();
            let na = a.normal_at(pa);
            let s = if convex { 1.0 } else { -1.0 };
            let dir = u * angle.cos() - na * (s * angle.sin());
            let misfit = || {
                BlendError::InvalidParameter(
                    "Der Winkel der Fase passt nicht zu dieser Kante (die Fase trifft die zweite Fläche nicht)".into(),
                )
            };
            let eps = SECTION_RESIDUAL_REL * (1.0 + o.abs().max_element() + distance);
            let mut lam = back.length();
            for _ in 0..SECTION_NEWTON_STEPS {
                let q = pa + dir * lam;
                let f = b.outside(q);
                if !f.is_finite() {
                    return Err(misfit());
                }
                if f.abs() <= eps {
                    return if lam > 0.0 {
                        Ok(Section { center: o, pa, pb: q })
                    } else {
                        Err(misfit())
                    };
                }
                let slope = b.normal_at(q).dot(dir);
                if !(slope.abs() > MIN_RAY_SLOPE) {
                    return Err(misfit());
                }
                lam -= f / slope;
            }
            Err(misfit())
        }
    }
}

/// Liegt die Kugel auf der gewölbten Innenseite einer gekrümmten Fläche und ist ihr Radius
/// mindestens so groß wie der Krümmungsradius dort, findet sie keine Lage:
/// `CurvatureTooSmall` (Spec §3.4). Ebenen und Freiformflächen werden hier nicht geprüft (bei
/// Freiformflächen prüft die Bahn die Genauigkeit, Task 14).
pub fn check_curvature(
    side: Side,
    p: DVec3,
    convex: bool,
    radius: f64,
    face: &str,
) -> Result<(), BlendError> {
    // Vorzeichen von `implicit` am Kugelmittelpunkt: < 0 heißt Achsseite (innen gewölbt).
    let ball = if convex { -1.0 } else { 1.0 } * if side.same_sense { 1.0 } else { -1.0 };
    if ball > 0.0 {
        return Ok(());
    }
    let rc = match side.surface {
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => Some(*radius),
        Surface::Torus { minor, .. } => Some(*minor),
        Surface::Cone { frame, slope, .. } => {
            let l = frame.to_local(p);
            Some((l.x * l.x + l.y * l.y).sqrt() * (1.0 + slope * slope).sqrt())
        }
        Surface::Plane { .. } | Surface::Nurbs(_) => None,
    };
    match rc {
        Some(rc) if radius >= rc * (1.0 - CURVATURE_MARGIN) => {
            Err(BlendError::CurvatureTooSmall { face: face.into() })
        }
        _ => Ok(()),
    }
}
```

- [ ] **Step 4: Modul eintragen** – in `crates/planora-kernel/src/blend/mod.rs` nach `pub mod error;` die Zeile `pub mod section;` ergänzen.

- [ ] **Step 5: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_section`
Expected: PASS (5 Tests).

- [ ] **Step 6: Prüfen und committen**

Run: `cargo fmt && cargo clippy -p planora-kernel --all-targets -- -D warnings && cargo test -p planora-kernel`

```bash
git add crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/src/blend/section.rs crates/planora-kernel/tests/blend_section.rs
git commit -m "kernel: Querschnitt der Rundung per Newton und Fasenpunkte

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Analytische Rundungs- und Fasenflächen

**Files:**
- Create: `crates/planora-kernel/src/blend/analytic.rs`
- Modify: `crates/planora-kernel/src/blend/mod.rs` (`pub mod analytic;`)
- Test: `crates/planora-kernel/tests/blend_analytic.rs`

**Interfaces:**
- Consumes: Task 1 (`EdgeSides`, `inward_directions`, `face_name`), Task 2 (`Side`, `Section`, `ChamferSpec`, `fillet_section`, `chamfer_section`, `check_curvature`), `crate::boolean::Seg` (`Seg::new(curve, t0, t1)`, `Seg::at`), `Surface`, `Curve`, `Frame`.
- Produces:
  ```rust
  pub mod analytic {
      pub const COAXIAL_ANGLE: f64;                                   // 1e-9
      pub enum SectionKind { Fillet { radius: f64 }, Chamfer(ChamferSpec) }
      pub enum Sweep { Translate { dir: DVec3, length: f64 }, Rotate { center: DVec3, axis: DVec3, x: DVec3, sweep: f64 } }
      pub enum VolumeModel { Prism { area: f64 }, Pappus { area: f64, centroid_radius: f64 }, Total { volume: f64 } }
      pub struct EdgeGeom { pub edge: Id<Edge>, pub sides: EdgeSides, pub convex: bool, pub surface: Surface,
          pub same_sense: bool, pub contact_a: Curve, pub contact_b: Curve, pub range: (f64, f64),
          pub section_start: Seg, pub section_end: Seg, pub volume: VolumeModel, pub reach: f64,
          pub closed: bool, pub analytic: bool }
      pub fn classify(solid: &Solid, edge: Id<Edge>, sides: &EdgeSides, tol: &Tolerance) -> Option<Sweep>;
      pub fn analytic_geom(solid: &Solid, edge: Id<Edge>, sides: &EdgeSides, convex: bool, kind: SectionKind, tol: &Tolerance) -> Result<Option<EdgeGeom>, BlendError>;
      pub fn section_seg(kind: SectionKind, sec: &Section) -> Seg;            // A → B
      pub fn polygon_area_centroid(pts: &[DVec3]) -> (f64, DVec3);
      pub fn section_polygon(kind: SectionKind, o: DVec3, sec: &Section) -> Vec<DVec3>;
      pub fn outward_sense(natural: DVec3, m: DVec3, o: DVec3, convex: bool) -> bool;
  }
  ```
  Festlegungen: `contact_a`/`contact_b` laufen in Kantenrichtung; `range` sind ihre Parameter am Kantenanfang und -ende (Geraden: Länge ab Kantenanfang, Kreise: Winkel ab dem Kantenanfang, Vorzeichen wie der Kantenparameter). `section_start`/`section_end` laufen von A nach B. `analytic = true` für alle Flächen dieser Task.

- [ ] **Step 1: Tests schreiben** (`crates/planora-kernel/tests/blend_analytic.rs`)

```rust
//! Analytische Rundungs- und Fasenflächen (Teilprojekt 8, Task 3).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::analytic::*;
use planora_kernel::blend::section::ChamferSpec;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn tol() -> Tolerance {
    Tolerance::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn geom(s: &Solid, a: &str, b: &str, kind: SectionKind) -> Result<Option<EdgeGeom>, BlendError> {
    let e = edge_between(s, a, b);
    let first = edge_uses(s, e)
        .into_iter()
        .find(|u| face_name(s, u.face) == a)
        .map(|u| u.face);
    let sides = edge_sides(s, e, first).unwrap();
    let angle = edge_angle(s, e, &sides).unwrap();
    analytic_geom(s, e, &sides, angle.convex, kind, &tol())
}

/// Querschnittsfläche zwischen Kante (Innenwinkel 90°) und Viertelkreis: (1 − π/4)·r².
fn spandrel90(r: f64) -> f64 {
    (1.0 - PI / 4.0) * r * r
}

/// Abstand des Schwerpunkts dieser Fläche von jeder der beiden Seiten (rechter Winkel).
fn spandrel90_offset(r: f64) -> f64 {
    r * (5.0 / 6.0 - PI / 4.0) / (1.0 - PI / 4.0)
}

#[test]
fn a_box_edge_fillet_is_a_cylinder_along_the_edge() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let g = geom(&s, "Deckel", "Wand 1", SectionKind::Fillet { radius: 2.0 }).unwrap().unwrap();
    match &g.surface {
        Surface::Cylinder { frame, radius } => {
            assert!((radius - 2.0).abs() < 1e-12);
            assert!(frame.z.cross(DVec3::X).length() < 1e-12);
            assert!((frame.origin.y - 2.0).abs() < 1e-12 && (frame.origin.z - 8.0).abs() < 1e-12);
        }
        other => panic!("{other:?}"),
    }
    assert!(g.same_sense && g.convex && !g.closed && g.analytic);
    // Berührkurven liegen auf ihren Flächen und laufen über die ganze Kante.
    let (t0, t1) = g.range;
    assert!(((t1 - t0).abs() - 10.0).abs() < 1e-12);
    for f in [0.0, 0.3, 1.0] {
        let t = t0 + (t1 - t0) * f;
        let (pa, pb) = (g.contact_a.point(t), g.contact_b.point(t));
        assert!((pa.z - 10.0).abs() < 1e-12 && (pa.y - 2.0).abs() < 1e-12, "{pa:?}");
        assert!(pb.y.abs() < 1e-12 && (pb.z - 8.0).abs() < 1e-12, "{pb:?}");
    }
    assert!(g.section_start.start().distance(g.contact_a.point(t0)) < 1e-12);
    assert!(g.section_start.end().distance(g.contact_b.point(t0)) < 1e-12);
    assert!(g.section_end.start().distance(g.contact_a.point(t1)) < 1e-12);
    match g.volume {
        VolumeModel::Prism { area } => assert!((area - spandrel90(2.0)).abs() < 1e-3 * spandrel90(2.0)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_concave_fillet_faces_the_other_way() {
    let s = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(10.0, 0.0),
                    DVec2::new(10.0, 4.0),
                    DVec2::new(4.0, 4.0),
                    DVec2::new(4.0, 10.0),
                    DVec2::new(0.0, 10.0),
                ])
                .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            5.0,
        )
        .unwrap();
    let g = geom(&s, "Wand 3", "Wand 4", SectionKind::Fillet { radius: 1.0 }).unwrap().unwrap();
    assert!(!g.convex && !g.same_sense);
    match &g.surface {
        Surface::Cylinder { frame, .. } => {
            assert!(frame.origin.truncate().distance(DVec2::new(5.0, 5.0)) < 1e-12)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_cylinder_top_edge_gives_a_closed_torus_and_a_cone_chamfer() {
    let s = k().make_cylinder(5.0, 8.0).unwrap();
    let g = geom(&s, "Wand 1", "Deckel", SectionKind::Fillet { radius: 1.0 }).unwrap().unwrap();
    match &g.surface {
        Surface::Torus { major, minor, frame } => {
            assert!((major - 4.0).abs() < 1e-10 && (minor - 1.0).abs() < 1e-12);
            assert!((frame.origin.z - 7.0).abs() < 1e-10);
        }
        other => panic!("{other:?}"),
    }
    assert!(g.closed);
    assert!(((g.range.1 - g.range.0).abs() - TAU).abs() < 1e-12);
    assert!(g.section_start.start().distance(g.section_end.start()) < 1e-10);
    match g.volume {
        VolumeModel::Pappus { area, centroid_radius } => {
            assert!((area - spandrel90(1.0)).abs() < 1e-3 * spandrel90(1.0));
            assert!((centroid_radius - (5.0 - spandrel90_offset(1.0))).abs() < 1e-3);
        }
        other => panic!("{other:?}"),
    }
    let c = geom(
        &s,
        "Wand 1",
        "Deckel",
        SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 1.0 }),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(c.surface, Surface::Cone { .. }), "{:?}", c.surface);
    match c.volume {
        VolumeModel::Pappus { area, centroid_radius } => {
            assert!((area - 0.5).abs() < 1e-12);
            assert!((centroid_radius - (5.0 - 1.0 / 3.0)).abs() < 1e-12);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn chamfers_on_a_box_edge_honour_face_one() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let two = geom(
        &s,
        "Deckel",
        "Wand 1",
        SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 2.5 }),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(two.surface, Surface::Plane { .. }));
    let p = two.contact_a.point(two.range.0);
    assert!((p.y - 1.0).abs() < 1e-12 && (p.z - 10.0).abs() < 1e-12, "Fläche 1 = Deckel: {p:?}");
    let q = two.contact_b.point(two.range.0);
    assert!((q.z - 7.5).abs() < 1e-12, "{q:?}");
    let angle = geom(
        &s,
        "Deckel",
        "Wand 1",
        SectionKind::Chamfer(ChamferSpec::DistanceAngle { distance: 1.0, angle: 30f64.to_radians() }),
    )
    .unwrap()
    .unwrap();
    let q = angle.contact_b.point(angle.range.0);
    assert!((10.0 - q.z - 30f64.to_radians().tan()).abs() < 1e-10, "{q:?}");
}

#[test]
fn hemisphere_and_cone_base_edges_are_tori() {
    let r_big = 5.0;
    let hemi = k()
        .revolve(
            &ProfileLoop {
                start: DVec2::ZERO,
                segments: vec![
                    Segment::Line { to: DVec2::new(r_big, 0.0) },
                    Segment::Arc { center: DVec2::ZERO, sweep: PI / 2.0 },
                ],
            },
            &Frame::WORLD,
        )
        .unwrap();
    let g = geom(&hemi, "Fläche 1", "Fläche 2", SectionKind::Fillet { radius: 1.0 }).unwrap().unwrap();
    match &g.surface {
        Surface::Torus { major, .. } => assert!((major - 15f64.sqrt()).abs() < 1e-9, "{major}"),
        other => panic!("{other:?}"),
    }
    let cone = k().make_cone(4.0, 6.0).unwrap();
    let g = geom(&cone, "Fläche 1", "Fläche 2", SectionKind::Fillet { radius: 0.5 }).unwrap().unwrap();
    assert!(matches!(g.surface, Surface::Torus { .. }));
    // Berührpunkt auf dem Kegelmantel liegt auf dem Mantel.
    let fb = &cone.faces[g.sides.b.face];
    let p = g.contact_b.point(g.range.0);
    assert!(fb.surface.implicit(p).abs() < 1e-9);
}

#[test]
fn curvature_and_axis_limits_are_named() {
    let thin = k().make_cylinder(1.0, 5.0).unwrap();
    let r = geom(&thin, "Wand 1", "Deckel", SectionKind::Fillet { radius: 1.5 });
    assert!(matches!(r, Err(BlendError::CurvatureTooSmall { .. })), "{r:?}");
    // Zylinder r = 2, Rundung 1,5: die Kugelbahn (ρ = 0,5) läge näher an der Achse als der Radius.
    let mid = k().make_cylinder(2.0, 5.0).unwrap();
    let r = geom(&mid, "Wand 1", "Deckel", SectionKind::Fillet { radius: 1.5 });
    assert!(matches!(r, Err(BlendError::Unsupported(m)) if m.contains("Drehachse")), "{r:?}");
}

#[test]
fn freeform_edges_are_not_analytic() {
    let ellipse = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 3.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let s = k()
        .extrude(
            &Profile { outer: ProfileLoop::nurbs_loop(ellipse).unwrap(), holes: vec![] },
            &Frame::WORLD,
            4.0,
        )
        .unwrap();
    let g = geom(&s, "Wand 1", "Deckel", SectionKind::Fillet { radius: 0.5 }).unwrap();
    assert!(g.is_none());
}

#[test]
fn polygon_area_and_centroid_of_a_unit_square() {
    let pts = [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y];
    let (a, c) = polygon_area_centroid(&pts);
    assert!((a - 1.0).abs() < 1e-15);
    assert!(c.distance(DVec3::new(0.5, 0.5, 0.0)) < 1e-15);
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_analytic`
Expected: FAIL – `could not find analytic in blend`.

- [ ] **Step 3: Analytische Flächen schreiben** (`crates/planora-kernel/src/blend/analytic.rs`)

```rust
//! Analytische Rundungs- und Fasenflächen (Spec R4, R5): Gerade Kanten zwischen Flächen, die sich
//! längs der Kante nicht ändern (Ebene; Zylinder mit Achse parallel zur Kante) ergeben einen
//! Zylinder bzw. eine Ebene; Kreiskanten zwischen Drehflächen um dieselbe Achse (Ebene senkrecht
//! zur Achse, koaxialer Zylinder, Kegel, Torus, Kugel mit Mittelpunkt auf der Achse) einen Torus
//! bzw. einen Kegel, Zylinder oder eine Ebene. Der Querschnitt wird einmal per Newton gerechnet
//! (Task 2) und dann verschoben bzw. gedreht.

use super::error::BlendError;
use super::section::{ChamferSpec, Section, Side, chamfer_section, check_curvature, fillet_section};
use super::topo::{EdgeSides, face_name, inward_directions};
use crate::boolean::Seg;
use crate::geometry::{Curve, Surface};
use crate::ids::Id;
use crate::topology::{Edge, Solid};
use planora_math::{DVec3, Frame, PI, Tolerance};

/// Winkel (Bogenmaß), bis zu dem Achsen als parallel bzw. Normalen als senkrecht zur Kante
/// gelten (wie `ANG` im analytischen Schnittrechner).
pub const COAXIAL_ANGLE: f64 = 1e-9;
/// Stützpunkte des Kreisbogens im Querschnittspolygon der Volumenschätzung.
const SECTION_ARC_SAMPLES: usize = 64;
/// Spielraum, mit dem die Kugelbahn weiter von der Drehachse entfernt sein muss als der Radius
/// (sonst durchdringt sich der Torus selbst).
const AXIS_MARGIN: f64 = 1e-9;

/// Art des Querschnitts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SectionKind {
    Fillet { radius: f64 },
    Chamfer(ChamferSpec),
}

/// Wie der Querschnitt entlang der Kante wandert.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sweep {
    /// Gerade Kante: Verschiebung um `dir` (Einheitsvektor in Kantenrichtung), Länge `length`.
    Translate { dir: DVec3, length: f64 },
    /// Kreiskante: Drehung um die Achse durch `center` mit Richtung `axis` um `sweep`
    /// (Bogenmaß, Vorzeichen wie der Kantenparameter); `x` zeigt von der Achse zum Kantenanfang.
    Rotate { center: DVec3, axis: DVec3, x: DVec3, sweep: f64 },
}

/// Grundlage der Volumenschätzung einer Kante (P2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VolumeModel {
    /// Querschnittsfläche mal Länge der Berührkurven (gerade Kanten).
    Prism { area: f64 },
    /// Pappus: Querschnittsfläche mal Weg ihres Schwerpunkts um die Achse.
    Pappus { area: f64, centroid_radius: f64 },
    /// Schon aufsummiertes Volumen (Bahn mit Hautfläche, Task 14).
    Total { volume: f64 },
}

/// Geometrie der Verrundung einer Kante.
#[derive(Clone, Debug)]
pub struct EdgeGeom {
    pub edge: Id<Edge>,
    pub sides: EdgeSides,
    pub convex: bool,
    /// Rundungs- bzw. Fasenfläche und ihre Außenrichtung.
    pub surface: Surface,
    pub same_sense: bool,
    /// Berührkurven auf A und B (Parameter wachsen bzw. fallen wie die Kante).
    pub contact_a: Curve,
    pub contact_b: Curve,
    /// Parameter der Berührkurven am Kantenanfang und am Kantenende.
    pub range: (f64, f64),
    /// Querschnitt von A nach B am Kantenanfang und am Kantenende.
    pub section_start: Seg,
    pub section_end: Seg,
    pub volume: VolumeModel,
    /// Größter Abstand Kantenpunkt–Berührpunkt (Suchfenster der Abschlüsse).
    pub reach: f64,
    /// Geschlossene Kante (Anfang = Ende, voller Umlauf): Abschluss an der Naht.
    pub closed: bool,
    /// Analytische Fläche (`true`) oder Hautfläche (`false`, Task 14).
    pub analytic: bool,
}

/// Abstand eines Punkts zur Achse durch `c` mit Richtung `z`.
fn axis_distance(p: DVec3, c: DVec3, z: DVec3) -> f64 {
    let d = p - c;
    (d - z * d.dot(z)).length()
}

/// Bleibt die Fläche bei Verschiebung längs `dir` gleich?
fn translation_invariant(s: &Surface, dir: DVec3) -> bool {
    match s {
        Surface::Plane { frame } => frame.z.dot(dir).abs() <= COAXIAL_ANGLE,
        Surface::Cylinder { frame, .. } => frame.z.cross(dir).length() <= COAXIAL_ANGLE,
        _ => false,
    }
}

/// Ist die Fläche eine Drehfläche um die Achse (`c`, `z`)?
fn revolves_about(s: &Surface, c: DVec3, z: DVec3, tol: &Tolerance) -> bool {
    match s {
        Surface::Plane { frame } => frame.z.cross(z).length() <= COAXIAL_ANGLE,
        Surface::Cylinder { frame, .. } | Surface::Cone { frame, .. } | Surface::Torus { frame, .. } => {
            frame.z.cross(z).length() <= COAXIAL_ANGLE
                && axis_distance(frame.origin, c, z) <= tol.linear
        }
        Surface::Sphere { frame, .. } => axis_distance(frame.origin, c, z) <= tol.linear,
        Surface::Nurbs(_) => false,
    }
}

/// Analytische Klasse der Kante, sonst `None` (Freiformflächen, schräge Achsen, Kegel an
/// geraden Kanten …; diese Kanten rechnet ab Task 15 die Bahn).
pub fn classify(solid: &Solid, edge: Id<Edge>, sides: &EdgeSides, tol: &Tolerance) -> Option<Sweep> {
    let e = solid.edges.get(edge)?;
    let (sa, sb) = (&solid.faces[sides.a.face].surface, &solid.faces[sides.b.face].surface);
    let sign = (e.t1 - e.t0).signum();
    match &e.curve {
        Curve::Line { dir, .. } => {
            let d = *dir * sign;
            (translation_invariant(sa, d) && translation_invariant(sb, d)).then_some(
                Sweep::Translate { dir: d, length: (e.t1 - e.t0).abs() },
            )
        }
        Curve::Circle { frame, .. } => {
            let x = (e.curve.point(e.t0) - frame.origin).normalize_or_zero();
            (revolves_about(sa, frame.origin, frame.z, tol)
                && revolves_about(sb, frame.origin, frame.z, tol))
            .then_some(Sweep::Rotate {
                center: frame.origin,
                axis: frame.z,
                x,
                sweep: e.t1 - e.t0,
            })
        }
        _ => None,
    }
}

/// Kreisbogen um `c` von `pa` nach `pb` (Öffnungswinkel unter π): Kreis mit `x` zu `pa`.
fn arc_curve(c: DVec3, pa: DVec3, pb: DVec3) -> (Curve, f64, Frame) {
    let ex = (pa - c).normalize();
    let w = pb - c;
    let ey = (w - ex * w.dot(ex)).normalize();
    let frame = Frame { origin: c, x: ex, y: ey, z: ex.cross(ey) };
    let angle = w.dot(ey).atan2(w.dot(ex));
    (Curve::Circle { frame, radius: (pa - c).length() }, angle, frame)
}

/// Querschnitt von A nach B als Stück: Kreisbogen (Rundung) oder Strecke (Fase).
pub fn section_seg(kind: SectionKind, sec: &Section) -> Seg {
    match kind {
        SectionKind::Fillet { .. } => {
            let (curve, angle, _) = arc_curve(sec.center, sec.pa, sec.pb);
            Seg::new(curve, 0.0, angle)
        }
        SectionKind::Chamfer(_) => {
            let d = sec.pb - sec.pa;
            Seg::new(Curve::Line { origin: sec.pa, dir: d.normalize() }, 0.0, d.length())
        }
    }
}

/// Polygon des Querschnitts zwischen Kantenpunkt `o` und Rundung bzw. Fase.
pub fn section_polygon(kind: SectionKind, o: DVec3, sec: &Section) -> Vec<DVec3> {
    let mut pts = vec![o, sec.pa];
    if let SectionKind::Fillet { .. } = kind {
        let s = section_seg(kind, sec);
        for i in 1..SECTION_ARC_SAMPLES {
            pts.push(s.at(i as f64 / SECTION_ARC_SAMPLES as f64));
        }
    }
    pts.push(sec.pb);
    pts
}

/// Fläche (≥ 0) und Schwerpunkt eines ebenen Polygons im Raum (Fächer um den ersten Punkt).
pub fn polygon_area_centroid(pts: &[DVec3]) -> (f64, DVec3) {
    let Some(&p0) = pts.first() else {
        return (0.0, DVec3::ZERO);
    };
    let mut normal = DVec3::ZERO;
    for w in pts[1..].windows(2) {
        normal += (w[0] - p0).cross(w[1] - p0);
    }
    let n = normal.normalize_or_zero();
    let (mut area, mut moment) = (0.0, DVec3::ZERO);
    for w in pts[1..].windows(2) {
        let a = 0.5 * (w[0] - p0).cross(w[1] - p0).dot(n);
        area += a;
        moment += (p0 + w[0] + w[1]) * (a / 3.0);
    }
    if area.abs() > 0.0 {
        (area.abs(), moment / area)
    } else {
        (0.0, p0)
    }
}

/// `same_sense` einer neuen Fläche aus ihrer natürlichen Normale `natural` an einem Punkt `m`
/// der Fläche: Bei konvexen Kanten zeigt die Außennormale zur alten Kante `o` hin, bei konkaven
/// von ihr weg.
pub fn outward_sense(natural: DVec3, m: DVec3, o: DVec3, convex: bool) -> bool {
    (natural.dot(o - m) > 0.0) == convex
}

/// Analytische Rundungs- bzw. Fasengeometrie; `Ok(None)`, wenn die Kante keiner analytischen
/// Klasse angehört.
pub fn analytic_geom(
    solid: &Solid,
    edge: Id<Edge>,
    sides: &EdgeSides,
    convex: bool,
    kind: SectionKind,
    tol: &Tolerance,
) -> Result<Option<EdgeGeom>, BlendError> {
    let Some(sweep) = classify(solid, edge, sides, tol) else {
        return Ok(None);
    };
    let e = &solid.edges[edge];
    let (fa, fb) = (&solid.faces[sides.a.face], &solid.faces[sides.b.face]);
    let a = Side { surface: &fa.surface, same_sense: fa.same_sense };
    let b = Side { surface: &fb.surface, same_sense: fb.same_sense };
    let o = e.curve.point(e.t0);
    let tangent = e.curve.tangent(e.t0) * (e.t1 - e.t0).signum();
    let sec = match kind {
        SectionKind::Fillet { radius } => {
            check_curvature(a, o, convex, radius, &face_name(solid, sides.a.face))?;
            check_curvature(b, o, convex, radius, &face_name(solid, sides.b.face))?;
            fillet_section(a, b, o, tangent, convex, radius, None)?
        }
        SectionKind::Chamfer(spec) => {
            let (wa, wb) = inward_directions(solid, edge, sides, e.t0);
            chamfer_section(a, b, o, tangent, convex, wa, wb, spec)?
        }
    };
    let closed = e.start == e.end;
    let geom = match sweep {
        Sweep::Translate { dir, length } => translate_geom(edge, *sides, convex, kind, o, dir, length, sec),
        Sweep::Rotate { center, axis, x, sweep } => {
            rotate_geom(solid, edge, *sides, convex, kind, o, (center, axis, x), sweep, sec, closed, tol)?
        }
    };
    Ok(Some(geom))
}

/// Gerade Kante: Zylinder (Rundung) bzw. Ebene (Fase) längs `dir`.
#[allow(clippy::too_many_arguments)]
fn translate_geom(
    edge: Id<Edge>,
    sides: EdgeSides,
    convex: bool,
    kind: SectionKind,
    o: DVec3,
    dir: DVec3,
    length: f64,
    sec: Section,
) -> EdgeGeom {
    let shifted = |s: &Section, d: f64| Section {
        center: s.center + dir * d,
        pa: s.pa + dir * d,
        pb: s.pb + dir * d,
    };
    let (surface, m, natural) = match kind {
        SectionKind::Fillet { radius } => {
            let (_, angle, frame) = arc_curve(sec.center, sec.pa, sec.pb);
            let m = sec.center + (frame.x * (angle / 2.0).cos() + frame.y * (angle / 2.0).sin()) * radius;
            (Surface::Cylinder { frame, radius }, m, (m - sec.center) / radius)
        }
        SectionKind::Chamfer(_) => {
            let y = (sec.pb - sec.pa).normalize();
            let frame = Frame { origin: sec.pa, x: dir, y, z: dir.cross(y) };
            (Surface::Plane { frame }, 0.5 * (sec.pa + sec.pb), frame.z)
        }
    };
    let (area, _) = polygon_area_centroid(&section_polygon(kind, o, &sec));
    EdgeGeom {
        edge,
        sides,
        convex,
        same_sense: outward_sense(natural, m, o, convex),
        surface,
        contact_a: Curve::Line { origin: sec.pa, dir },
        contact_b: Curve::Line { origin: sec.pb, dir },
        range: (0.0, length),
        section_start: section_seg(kind, &sec),
        section_end: section_seg(kind, &shifted(&sec, length)),
        volume: VolumeModel::Prism { area },
        reach: sec.pa.distance(o).max(sec.pb.distance(o)),
        closed: false,
        analytic: true,
    }
}

/// Kreiskante: Torus (Rundung) bzw. Kegel, Zylinder oder Ebene (Fase) um die Achse.
#[allow(clippy::too_many_arguments)]
fn rotate_geom(
    solid: &Solid,
    edge: Id<Edge>,
    sides: EdgeSides,
    convex: bool,
    kind: SectionKind,
    o: DVec3,
    (center, axis, x): (DVec3, DVec3, DVec3),
    sweep: f64,
    sec: Section,
    closed: bool,
    tol: &Tolerance,
) -> Result<EdgeGeom, BlendError> {
    let y = axis.cross(x);
    let rho = |p: DVec3| (p - center).dot(x);
    let h = |p: DVec3| (p - center).dot(axis);
    // Punkt der Meridianebene (u = 0) um den Winkel `phi` gedreht.
    let rot = |p: DVec3, phi: f64| center + (x * phi.cos() + y * phi.sin()) * rho(p) + axis * h(p);
    for (p, face) in [(sec.pa, sides.a.face), (sec.pb, sides.b.face)] {
        if !(rho(p) > tol.linear) {
            return Err(BlendError::TooLarge { face: face_name(solid, face) });
        }
    }
    let circle = |p: DVec3| Curve::Circle {
        frame: Frame { origin: center + axis * h(p), x, y, z: axis },
        radius: rho(p),
    };
    let at = |phi: f64| Section {
        center: rot(sec.center, phi),
        pa: rot(sec.pa, phi),
        pb: rot(sec.pb, phi),
    };
    let (surface, m, natural) = match kind {
        SectionKind::Fillet { radius } => {
            let rc = rho(sec.center);
            if !(rc > radius * (1.0 + AXIS_MARGIN)) {
                return Err(BlendError::Unsupported(
                    "Die Kugelbahn kommt der Drehachse näher als der Radius; bitte einen kleineren Radius wählen".into(),
                ));
            }
            let frame = Frame { origin: center + axis * h(sec.center), x, y, z: axis };
            let va = (h(sec.pa) - h(sec.center)).atan2(rho(sec.pa) - rc);
            let mut vb = (h(sec.pb) - h(sec.center)).atan2(rho(sec.pb) - rc);
            vb = va + (vb - va - PI).rem_euclid(2.0 * PI) - PI;
            let vm = 0.5 * (va + vb);
            let natural = x * vm.cos() + axis * vm.sin();
            (
                Surface::Torus { frame, major: rc, minor: radius },
                sec.center + natural * radius,
                natural,
            )
        }
        SectionKind::Chamfer(_) => {
            let (ra, ha, rb, hb) = (rho(sec.pa), h(sec.pa), rho(sec.pb), h(sec.pb));
            let surface = if (ha - hb).abs() <= tol.linear {
                Surface::Plane { frame: Frame { origin: center + axis * ha, x, y, z: axis } }
            } else if (ra - rb).abs() <= tol.linear {
                Surface::Cylinder { frame: Frame { origin: center, x, y, z: axis }, radius: ra }
            } else {
                Surface::Cone {
                    frame: Frame { origin: center + axis * ha, x, y, z: axis },
                    radius: ra,
                    slope: (rb - ra) / (hb - ha),
                }
            };
            let m = 0.5 * (sec.pa + sec.pb);
            let (u, v) = surface.project(m);
            let natural = surface.normal(u, v);
            (surface, m, natural)
        }
    };
    let (area, centroid) = polygon_area_centroid(&section_polygon(kind, o, &sec));
    Ok(EdgeGeom {
        edge,
        sides,
        convex,
        same_sense: outward_sense(natural, m, o, convex),
        surface,
        contact_a: circle(sec.pa),
        contact_b: circle(sec.pb),
        range: (0.0, sweep),
        section_start: section_seg(kind, &sec),
        section_end: section_seg(kind, &at(sweep)),
        volume: VolumeModel::Pappus { area, centroid_radius: rho(centroid) },
        reach: sec.pa.distance(o).max(sec.pb.distance(o)),
        closed,
        analytic: true,
    })
}
```

Hinweis zur Umsetzung: `section_seg` für den gedrehten Querschnitt am Ende ergibt bei einer geschlossenen Kante (`sweep = ±2π`) denselben Bogen wie am Anfang (bis auf Rundung ~1e-15); `build` (Task 4) nimmt für die Naht deshalb nur `section_start`.

- [ ] **Step 4: Modul eintragen** – in `crates/planora-kernel/src/blend/mod.rs` `pub mod analytic;` ergänzen (vor `pub mod error;`).

- [ ] **Step 5: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_analytic`
Expected: PASS (8 Tests). Die Flächen der Querschnittspolygone haben einen Sehnenfehler von höchstens `1 − cos(π/128)` relativ (64 Stützpunkte auf ≤ 180°), deshalb prüfen die Tests sie auf `1e-3`.

- [ ] **Step 6: Prüfen und committen**

Run: `cargo fmt && cargo clippy -p planora-kernel --all-targets -- -D warnings && cargo test -p planora-kernel`

```bash
git add crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/src/blend/analytic.rs crates/planora-kernel/tests/blend_analytic.rs
git commit -m "kernel: analytische Rundungs- und Fasenflächen (Zylinder, Torus, Ebene, Kegel)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Endabschlüsse und Umbau der Nachbarflächen zu Stücken

**Files:**
- Create: `crates/planora-kernel/src/blend/build.rs`
- Modify: `crates/planora-kernel/src/blend/mod.rs` (`pub mod build;`)
- Test: `crates/planora-kernel/tests/blend_build.rs`

**Interfaces:**
- Consumes: Task 1–3; aus `crate::boolean`: `Piece`, `Seg`, `SurfaceIntersection`, `CurveSurface`, `curve_surface_roots(curve, t0, t1, surface, tol)`, `face_geom(solid, id) -> FaceGeom`, `intersect_surfaces(a, b, tol)`, `loop_points(&[Seg])`, `oriented_area(&[DVec2], same_sense)`, `points_to_uv(surface, same_sense, &[DVec3])`, `reverse_loop(&[Seg])`, `sew(Vec<Piece>, &Tolerance)`, `pipeline_tol(tol, freeform)` (`pub(crate)`); `crate::validate::validate_with`.
- Produces:
  ```rust
  pub mod build {
      pub const ON_CURVE_FACTOR: f64;      // 10
      pub const END_SEARCH_FACTOR: f64;    // 4
      pub struct EndCut { pub sa: f64, pub sb: f64, pub curve: Seg, pub cap: Option<(Id<Face>, Id<Vertex>)> }
      #[derive(Default)] pub struct EndPlan { pub starts: Vec<EndCut>, pub ends: Vec<EndCut>, pub corners: Vec<Piece>, pub corner_volume: f64 }
      pub fn blend_tol(solid: &Solid, geoms: &[EdgeGeom], tol: &Tolerance) -> Tolerance;
      pub fn param_near(curve: &Curve, p: DVec3, near: f64) -> f64;
      pub fn seam_cut(g: &EdgeGeom, at_end: bool) -> EndCut;
      pub fn cut_with(g: &EdgeGeom, at_end: bool, surface: &Surface, near: DVec3, tol: &Tolerance) -> Option<(f64, f64, Seg)>;
      pub fn cap_cut(solid: &Solid, g: &EdgeGeom, at_end: bool, tol: &Tolerance) -> Result<EndCut, BlendError>;
      pub fn end_cuts(solid: &Solid, geoms: &[EdgeGeom], tol: &Tolerance) -> Result<EndPlan, BlendError>;
      pub fn orient_outer(surface: &Surface, same_sense: bool, segs: Vec<Seg>) -> Vec<Seg>;
      pub fn loop_patch(surface: &Surface, same_sense: bool, lp: &[Seg]) -> Option<UvRect>;
      pub fn chain_loop(segs: Vec<Seg>, weld: f64) -> Option<Vec<Seg>>;
      pub fn assemble(solid: &Solid, geoms: &[EdgeGeom], plan: &EndPlan, roles: &[String], tol: &Tolerance) -> Result<Solid, BlendError>;
  }
  ```
  Regeln: `EndCut::curve` läuft von der Berührkurve A zur Berührkurve B. `cap = Some((C, v))` heißt: Fläche `C` bekommt an der Ecke `v` die Abschlusskurve als neue Kante. Rundungsflächen bekommen die Marke `FaceTag { origin: 0, role: roles[i] }`; beschnittene Flächen behalten ihre Marke. `chain_loop` ordnet lose Stücke zu einer geschlossenen Schleife (für Eckstücke, Task 11).

- [ ] **Step 1: Tests schreiben** (`crates/planora-kernel/tests/blend_build.rs`)

```rust
//! Endabschlüsse und Umbau der Nachbarflächen (Teilprojekt 8, Task 4).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::analytic::*;
use planora_kernel::blend::build::*;
use planora_kernel::blend::section::ChamferSpec;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn tol() -> Tolerance {
    Tolerance::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn parts(s: &Solid, a: &str, b: &str, kind: SectionKind) -> Result<(Vec<EdgeGeom>, EndPlan), BlendError> {
    let e = edge_between(s, a, b);
    let sides = edge_sides(s, e, None)?;
    let angle = edge_angle(s, e, &sides)?;
    let g = analytic_geom(s, e, &sides, angle.convex, kind, &tol())?.expect("analytische Kante");
    let geoms = vec![g];
    let plan = end_cuts(s, &geoms, &tol())?;
    Ok((geoms, plan))
}

fn build(s: &Solid, a: &str, b: &str, kind: SectionKind, role: &str) -> Solid {
    let (geoms, plan) = parts(s, a, b, kind).unwrap();
    let r = assemble(s, &geoms, &plan, &[role.to_string()], &tol()).unwrap();
    validate(&r).unwrap();
    r
}

fn roles(s: &Solid) -> Vec<String> {
    let mut v: Vec<String> = s.faces.iter().map(|(id, _)| face_name(s, id)).collect();
    v.sort();
    v
}

fn loop_len(s: &Solid, role: &str) -> usize {
    let (_, f) = s.faces.iter().find(|(id, _)| face_name(s, *id) == role).unwrap();
    f.outer.half_edges.len()
}

#[test]
fn a_box_edge_fillet_assembles_into_a_valid_solid() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let r = build(&s, "Deckel", "Wand 1", SectionKind::Fillet { radius: 2.0 }, "Rundung 1");
    assert_eq!((r.faces.len(), r.edges.len(), r.vertices.len()), (7, 15, 10));
    assert_eq!(
        roles(&r),
        ["Boden", "Deckel", "Rundung 1", "Wand 1", "Wand 2", "Wand 3", "Wand 4"]
    );
    // Die Seitenwände bekommen je eine Abschlusskante (Bogen) statt der Ecke.
    assert_eq!(loop_len(&r, "Wand 2"), 5);
    assert_eq!(loop_len(&r, "Wand 4"), 5);
    let fillet = r.faces.iter().find(|(id, _)| face_name(&r, *id) == "Rundung 1").unwrap().1;
    assert_eq!(fillet.tag.as_ref().unwrap().origin, 0);
    assert!(fillet.patch.is_some());
}

#[test]
fn a_cylinder_top_fillet_closes_at_the_seam() {
    let s = k().make_cylinder(5.0, 8.0).unwrap();
    let r = build(&s, "Wand 1", "Deckel", SectionKind::Fillet { radius: 1.0 }, "Rundung 1");
    assert_eq!((r.faces.len(), r.edges.len(), r.vertices.len()), (4, 5, 3));
}

#[test]
fn a_concave_edge_extends_the_caps() {
    let profile = Profile {
        outer: ProfileLoop::polygon(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ])
        .unwrap(),
        holes: vec![],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let r = build(&s, "Wand 3", "Wand 4", SectionKind::Fillet { radius: 1.0 }, "Rundung 1");
    assert_eq!(loop_len(&r, "Deckel"), 7);
    assert_eq!(loop_len(&r, "Boden"), 7);
}

#[test]
fn chamfers_assemble_too() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let kind = SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 2.0 });
    let r = build(&s, "Deckel", "Wand 1", kind, "Fase 1");
    let (_, f) = r.faces.iter().find(|(id, _)| face_name(&r, *id) == "Fase 1").unwrap();
    assert!(matches!(f.surface, Surface::Plane { .. }));
    let c = k().make_cylinder(5.0, 8.0).unwrap();
    let kind = SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 1.0 });
    let r = build(&c, "Wand 1", "Deckel", kind, "Fase 1");
    let (_, f) = r.faces.iter().find(|(id, _)| face_name(&r, *id) == "Fase 1").unwrap();
    assert!(matches!(f.surface, Surface::Cone { .. }));
}

#[test]
fn an_end_face_that_cannot_be_cut_is_named() {
    // Langloch: der obere Bogen endet an tangential anschließenden ebenen Wänden; Torus und
    // diese Ebenen schneiden sich nicht (die Berührkurve erreicht die Wand nicht).
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(10.0, 0.0) },
                Segment::Arc { center: DVec2::new(10.0, 2.0), sweep: PI },
                Segment::Line { to: DVec2::new(0.0, 4.0) },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let r = parts(&s, "Deckel", "Wand 2", SectionKind::Fillet { radius: 1.0 });
    match r {
        Err(BlendError::Unsupported(m)) => {
            assert!(m.contains("nicht abschließen") && m.contains("Wand"), "{m}")
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn parameters_unwrap_near_the_reference() {
    let c = Curve::Circle { frame: Frame::WORLD, radius: 2.0 };
    let p = DVec3::new(0.0, -2.0, 0.0);
    assert!((param_near(&c, p, 0.0) + PI / 2.0).abs() < 1e-12);
    assert!((param_near(&c, p, TAU) - 1.5 * PI).abs() < 1e-12);
    let l = Curve::Line { origin: DVec3::ZERO, dir: DVec3::X };
    assert_eq!(param_near(&l, DVec3::new(3.0, 1.0, 0.0), 100.0), 3.0);
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_build`
Expected: FAIL – `could not find build in blend`.

- [ ] **Step 3: Umbau schreiben** (`crates/planora-kernel/src/blend/build.rs`)

```rust
//! Endabschlüsse der Rundungsflächen und Umbau der Nachbarflächen (Spec §4.1, §4.5, R6): Die
//! Nachbarflächen werden bis zu den Berührkurven gekürzt, Abschlussflächen bekommen die
//! Abschlusskurve als neue Kante, alle Flächen gehen als `Piece` in `sew`; danach prüft
//! `validate_with` das Ergebnis.

use super::analytic::EdgeGeom;
use super::error::BlendError;
use super::topo::{face_name, vertex_faces};
use crate::boolean::{
    CurveSurface, Piece, Seg, SurfaceIntersection, curve_surface_roots, face_geom,
    intersect_surfaces, loop_points, oriented_area, pipeline_tol, points_to_uv, reverse_loop,
    sew,
};
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::topology::{Face, FaceTag, Solid, Vertex};
use crate::validate::validate_with;
use planora_math::{DVec3, PI, Tolerance};
use std::collections::HashMap;

/// Vielfaches der (ggf. erweiterten) Längentoleranz, bis zu dem ein Punkt auf einer Kurve liegt
/// oder zwei Punkte zusammenfallen (Abschlusskurven, gekürzte Nachbarkanten, Ecken). Die
/// analytischen Schnitte sind auf etwa 1e-12 genau; der Faktor lässt Spielraum für Newton.
pub const ON_CURVE_FACTOR: f64 = 10.0;
/// Suchfenster für den Schnitt einer Berührkurve mit der Abschlussfläche, in Vielfachen der
/// Reichweite (größter Abstand Kante–Berührpunkt). Schräge Abschlussflächen verschieben den
/// Schnitt höchstens um die Reichweite mal den Kotangens des Schnittwinkels.
pub const END_SEARCH_FACTOR: f64 = 4.0;
/// Kleinster Anteil, den eine gekürzte Nachbarkante von ihrer alten Länge behalten muss; darunter
/// (oder bei Umkehr) ist die Rundung zu groß für diese Fläche.
const MIN_TRIMMED_FRACTION: f64 = 1e-6;
/// Stützpunkte je Stück beim Bestimmen des Parameterbereichs neuer Flächen (dichter als
/// `loop_points`, damit Bögen den Bereich nicht unterschätzen).
const PATCH_SAMPLES_PER_SEG: usize = 64;

/// Abschluss einer Rundungsfläche an einem Ende.
#[derive(Clone, Debug)]
pub struct EndCut {
    /// Parameter der Berührkurven A und B am Abschluss.
    pub sa: f64,
    pub sb: f64,
    /// Abschlusskurve auf der Rundungsfläche, von A nach B.
    pub curve: Seg,
    /// Abschlussfläche und Ecke, an der die Kante endete (`None` bei Naht, glattem Übergang und
    /// Ecken).
    pub cap: Option<(Id<Face>, Id<Vertex>)>,
}

/// Abschlüsse aller Kanten (Reihenfolge wie die Kanten) und Eckstücke.
#[derive(Clone, Debug, Default)]
pub struct EndPlan {
    pub starts: Vec<EndCut>,
    pub ends: Vec<EndCut>,
    pub corners: Vec<Piece>,
    /// Geschätzte Volumenänderung der Eckstücke (mm³, negativ = Material weg).
    pub corner_volume: f64,
}

/// Längentoleranz des Umbaus: mit Freiformflächen, Freiformkanten oder Hautflächen die weitere
/// Knotentoleranz (`pipeline_tol`), sonst unverändert.
pub fn blend_tol(solid: &Solid, geoms: &[EdgeGeom], tol: &Tolerance) -> Tolerance {
    let freeform = geoms.iter().any(|g| !g.analytic)
        || solid.faces.iter().any(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        || solid.edges.iter().any(|(_, e)| matches!(e.curve, Curve::Nurbs(_)));
    pipeline_tol(tol, freeform)
}

/// Parameter des Kurvenpunkts zu `p`; bei periodischen Kurven der Wert, der `near` am nächsten
/// liegt.
pub fn param_near(curve: &Curve, p: DVec3, near: f64) -> f64 {
    let t = curve.project(p);
    match curve.period() {
        Some(per) => t + per * ((near - t) / per).round(),
        None => t,
    }
}

fn on_curve(curve: &Curve, t: f64, p: DVec3, tol: &Tolerance) -> bool {
    curve.point(t).distance(p) <= ON_CURVE_FACTOR * tol.linear
}

/// Parameterbreite, die auf der Kurve bei `s` etwa der Länge `len` entspricht.
fn param_window(curve: &Curve, s: f64, len: f64) -> f64 {
    match curve {
        Curve::Line { .. } => len,
        Curve::Circle { radius, .. } => (len / radius).min(PI),
        _ => {
            let speed = curve.tangent(s).length();
            if speed > 0.0 { len / speed } else { len }
        }
    }
}

/// Schnitt der Kurve mit der Fläche, der `s_ref` am nächsten liegt.
fn end_param(curve: &Curve, s_ref: f64, len: f64, surface: &Surface, tol: &Tolerance) -> Option<f64> {
    let w = param_window(curve, s_ref, len);
    match curve_surface_roots(curve, s_ref - w, s_ref + w, surface, tol) {
        CurveSurface::Roots(rs) => rs
            .iter()
            .map(|r| r.t)
            .min_by(|x, y| (x - s_ref).abs().total_cmp(&(y - s_ref).abs())),
        CurveSurface::Lies => None,
    }
}

/// Stück einer Schnittkurve von `qa` nach `qb`; bei geschlossenen Kurven der Bogen, dessen
/// Mitte näher an `near` (der alten Ecke) liegt.
fn pick_arc(cs: &[Curve], qa: DVec3, qb: DVec3, near: DVec3, tol: &Tolerance) -> Option<Seg> {
    for c in cs {
        let ta = param_near(c, qa, 0.0);
        let tb = param_near(c, qb, ta);
        if !(on_curve(c, ta, qa, tol) && on_curve(c, tb, qb, tol)) {
            continue;
        }
        return Some(match c.period() {
            None => Seg::new(c.clone(), ta, tb),
            Some(per) => {
                let alt = if tb > ta { tb - per } else { tb + per };
                let gap = |t1: f64| Seg::new(c.clone(), ta, t1).at(0.5).distance(near);
                if gap(tb) <= gap(alt) {
                    Seg::new(c.clone(), ta, tb)
                } else {
                    Seg::new(c.clone(), ta, alt)
                }
            }
        });
    }
    None
}

/// Abschluss an einer Naht (geschlossene Kante): an beiden Enden derselbe Querschnitt.
pub fn seam_cut(g: &EdgeGeom, at_end: bool) -> EndCut {
    let s = if at_end { g.range.1 } else { g.range.0 };
    EndCut { sa: s, sb: s, curve: g.section_start.clone(), cap: None }
}

/// Schnitt der Rundung `g` an ihrem Anfang bzw. Ende mit `surface`: Parameter der
/// Berührkurven und Abschlusskurve von A nach B (der Bogen näher an `near`).
pub fn cut_with(
    g: &EdgeGeom,
    at_end: bool,
    surface: &Surface,
    near: DVec3,
    tol: &Tolerance,
) -> Option<(f64, f64, Seg)> {
    let s_ref = if at_end { g.range.1 } else { g.range.0 };
    let len = END_SEARCH_FACTOR * (g.reach + tol.linear);
    let sa = end_param(&g.contact_a, s_ref, len, surface, tol)?;
    let sb = end_param(&g.contact_b, s_ref, len, surface, tol)?;
    let (qa, qb) = (g.contact_a.point(sa), g.contact_b.point(sb));
    let SurfaceIntersection::Curves(cs) = intersect_surfaces(&g.surface, surface, tol) else {
        return None;
    };
    let curve = pick_arc(&cs, qa, qb, near, tol)?;
    Some((sa, sb, curve))
}

/// Abschluss an der dritten Fläche der Ecke (Spec §4.1 „Enden“). An der Ecke müssen genau drei
/// Flächen zusammentreffen.
pub fn cap_cut(solid: &Solid, g: &EdgeGeom, at_end: bool, tol: &Tolerance) -> Result<EndCut, BlendError> {
    let e = &solid.edges[g.edge];
    let v = if at_end { e.end } else { e.start };
    let faces = vertex_faces(solid, v);
    let others: Vec<Id<Face>> = faces
        .iter()
        .copied()
        .filter(|f| *f != g.sides.a.face && *f != g.sides.b.face)
        .collect();
    if faces.len() != 3 || others.len() != 1 {
        return Err(BlendError::Corner(format!(
            "An der Ecke treffen {} Flächen zusammen; unterstützt sind drei",
            faces.len()
        )));
    }
    let c = others[0];
    let vp = solid.vertices[v].point;
    let (sa, sb, curve) = cut_with(g, at_end, &solid.faces[c].surface, vp, tol).ok_or_else(|| {
        BlendError::Unsupported(format!(
            "Die Rundung lässt sich an der Fläche „{}“ nicht abschließen",
            face_name(solid, c)
        ))
    })?;
    Ok(EndCut { sa, sb, curve, cap: Some((c, v)) })
}

/// Abschlüsse aller Kanten: geschlossene Kanten an ihrer Naht, offene an der dritten Fläche ihrer
/// Endecken (Stufe 1; Ketten und Ecken ab Task 9 und 11).
pub fn end_cuts(solid: &Solid, geoms: &[EdgeGeom], tol: &Tolerance) -> Result<EndPlan, BlendError> {
    let mut plan = EndPlan::default();
    for g in geoms {
        if g.closed {
            plan.starts.push(seam_cut(g, false));
            plan.ends.push(seam_cut(g, true));
        } else {
            plan.starts.push(cap_cut(solid, g, false, tol)?);
            plan.ends.push(cap_cut(solid, g, true, tol)?);
        }
    }
    Ok(plan)
}

/// Kürzt das Stück am Ende (`at_end`) bzw. Anfang bis `p`. Liegt `p` nicht auf der Kurve:
/// `Unsupported`; bliebe vom Stück nichts übrig (oder kehrte es sich um): `TooLarge`.
fn retarget(seg: &Seg, p: DVec3, at_end: bool, face: &str, tol: &Tolerance) -> Result<Seg, BlendError> {
    let old = if at_end { seg.t1 } else { seg.t0 };
    let t = param_near(&seg.curve, p, old);
    if !on_curve(&seg.curve, t, p, tol) {
        return Err(BlendError::Unsupported(format!(
            "Eine Kante der Fläche „{face}“ passt nicht zur Rundung"
        )));
    }
    let (t0, t1) = if at_end { (seg.t0, t) } else { (t, seg.t1) };
    let before = seg.t1 - seg.t0;
    if !((t1 - t0) * before.signum() > MIN_TRIMMED_FRACTION * before.abs()) {
        return Err(BlendError::TooLarge { face: face.to_string() });
    }
    Ok(Seg::new(seg.curve.clone(), t0, t1))
}

/// Schließt eine Schleife nach dem Ersetzen: Nachbarn ersetzter Stücke werden bis an deren Enden
/// gekürzt; zwei ersetzte Nachbarn müssen sich schon treffen.
fn reconnect(segs: &mut [Seg], replaced: &[bool], face: &str, tol: &Tolerance) -> Result<(), BlendError> {
    let n = segs.len();
    if n < 2 {
        return Ok(());
    }
    for k in 0..n {
        let prev = (k + n - 1) % n;
        match (replaced[prev], replaced[k]) {
            (false, true) => {
                let p = segs[k].start();
                segs[prev] = retarget(&segs[prev], p, true, face, tol)?;
            }
            (true, false) => {
                let p = segs[prev].end();
                segs[k] = retarget(&segs[k], p, false, face, tol)?;
            }
            (true, true) => {
                if segs[prev].end().distance(segs[k].start()) > ON_CURVE_FACTOR * tol.linear {
                    return Err(BlendError::Corner(format!(
                        "Die Rundungen treffen sich auf der Fläche „{face}“ nicht"
                    )));
                }
            }
            (false, false) => {}
        }
    }
    Ok(())
}

/// Setzt die Abschlusskurve `cap` an der Ecke `vp` in eine Schleife der Abschlussfläche ein und
/// kürzt die beiden Kanten an der Ecke bis zu ihren Enden.
fn insert_cap(loops: &mut [Vec<Seg>], vp: DVec3, cap: &Seg, face: &str, tol: &Tolerance) -> Result<(), BlendError> {
    let weld = ON_CURVE_FACTOR * tol.linear;
    let on = |s: &Seg, p: DVec3, at_end: bool| {
        let t = param_near(&s.curve, p, if at_end { s.t1 } else { s.t0 });
        on_curve(&s.curve, t, p, tol)
    };
    for lp in loops.iter_mut() {
        let n = lp.len();
        for j in 0..n {
            let k = (j + 1) % n;
            if lp[j].end().distance(vp) > weld || lp[k].start().distance(vp) > weld {
                continue;
            }
            let oriented = if on(&lp[j], cap.start(), true) && on(&lp[k], cap.end(), false) {
                cap.clone()
            } else if on(&lp[j], cap.end(), true) && on(&lp[k], cap.start(), false) {
                cap.reversed()
            } else {
                return Err(BlendError::Unsupported(format!(
                    "Die Abschlusskurve passt nicht zur Fläche „{face}“"
                )));
            };
            lp[j] = retarget(&lp[j], oriented.start(), true, face, tol)?;
            lp[k] = retarget(&lp[k], oriented.end(), false, face, tol)?;
            lp.insert(j + 1, oriented);
            return Ok(());
        }
    }
    Err(BlendError::ResultInvalid(format!(
        "Die Ecke der Fläche „{face}“ wurde nicht gefunden"
    )))
}

/// Dreht eine Schleife so, dass sie Außenkontur der Fläche ist (gegen den Uhrzeigersinn um die
/// Außennormale).
pub fn orient_outer(surface: &Surface, same_sense: bool, segs: Vec<Seg>) -> Vec<Seg> {
    let poly = points_to_uv(surface, same_sense, &loop_points(&segs));
    if oriented_area(&poly, same_sense) < 0.0 {
        reverse_loop(&segs)
    } else {
        segs
    }
}

/// Dichte Punkte einer Schleife (für den Parameterbereich).
fn dense_points(lp: &[Seg]) -> Vec<DVec3> {
    let mut pts = Vec::with_capacity(lp.len() * PATCH_SAMPLES_PER_SEG);
    for s in lp {
        for i in 0..PATCH_SAMPLES_PER_SEG {
            pts.push(s.at(i as f64 / PATCH_SAMPLES_PER_SEG as f64));
        }
    }
    pts
}

/// Parameterbereich einer gekrümmten Fläche aus ihrer Schleife (Ebenen: keiner).
pub fn loop_patch(surface: &Surface, same_sense: bool, lp: &[Seg]) -> Option<UvRect> {
    if matches!(surface, Surface::Plane { .. }) {
        return None;
    }
    let poly = points_to_uv(surface, same_sense, &dense_points(lp));
    let mut r = UvRect {
        u0: f64::INFINITY,
        u1: f64::NEG_INFINITY,
        v0: f64::INFINITY,
        v1: f64::NEG_INFINITY,
    };
    for p in &poly {
        r.u0 = r.u0.min(p.x);
        r.u1 = r.u1.max(p.x);
        r.v0 = r.v0.min(p.y);
        r.v1 = r.v1.max(p.y);
    }
    poly.first().map(|_| r)
}

/// Alter Parameterbereich, in nicht geschlossenen Richtungen bis zu den neuen Schleifen erweitert
/// (P10).
fn grow_patch(surface: &Surface, same_sense: bool, old: Option<UvRect>, loops: &[Vec<Seg>]) -> Option<UvRect> {
    let mut r = old?;
    for lp in loops {
        if let Some(p) = loop_patch(surface, same_sense, lp) {
            if surface.u_period().is_none() {
                r.u0 = r.u0.min(p.u0);
                r.u1 = r.u1.max(p.u1);
            }
            if surface.v_period().is_none() {
                r.v0 = r.v0.min(p.v0);
                r.v1 = r.v1.max(p.v1);
            }
        }
    }
    Some(r)
}

/// Ordnet lose Stücke zu einer geschlossenen Schleife (Stücke werden bei Bedarf umgedreht);
/// `None`, wenn sie sich nicht schließen.
pub fn chain_loop(mut segs: Vec<Seg>, weld: f64) -> Option<Vec<Seg>> {
    if segs.is_empty() {
        return None;
    }
    let mut out = vec![segs.remove(0)];
    while !segs.is_empty() {
        let end = out.last()?.end();
        let i = segs
            .iter()
            .position(|s| s.start().distance(end) <= weld || s.end().distance(end) <= weld)?;
        let s = segs.remove(i);
        out.push(if s.start().distance(end) <= weld { s } else { s.reversed() });
    }
    (out.last()?.end().distance(out[0].start()) <= weld).then_some(out)
}

/// Baut den neuen Körper: alle Flächen mit ersetzten und gekürzten Kanten, Abschlusskurven in den
/// Abschlussflächen, die Rundungsflächen (Rollen `roles`) und die Eckstücke des Plans; danach
/// `sew` und `validate_with`.
pub fn assemble(
    solid: &Solid,
    geoms: &[EdgeGeom],
    plan: &EndPlan,
    roles: &[String],
    tol: &Tolerance,
) -> Result<Solid, BlendError> {
    let btol = blend_tol(solid, geoms, tol);
    let mut repl: HashMap<(Id<Face>, usize, usize), Seg> = HashMap::new();
    for (i, g) in geoms.iter().enumerate() {
        let (s, e) = (&plan.starts[i], &plan.ends[i]);
        for (u, curve, s0, s1) in [
            (g.sides.a, &g.contact_a, s.sa, e.sa),
            (g.sides.b, &g.contact_b, s.sb, e.sb),
        ] {
            let seg = Seg::new(curve.clone(), s0, s1);
            repl.insert(
                (u.face, u.loop_index, u.position),
                if u.forward { seg } else { seg.reversed() },
            );
        }
    }
    let mut caps: HashMap<Id<Face>, Vec<(Id<Vertex>, Seg)>> = HashMap::new();
    for cut in plan.starts.iter().chain(&plan.ends) {
        if let Some((f, v)) = cut.cap {
            caps.entry(f).or_default().push((v, cut.curve.clone()));
        }
    }
    let mut pieces: Vec<Piece> = Vec::with_capacity(solid.faces.len() + geoms.len());
    for (fid, face) in solid.faces.iter() {
        let g = face_geom(solid, fid);
        let name = face_name(solid, fid);
        let mut loops: Vec<Vec<Seg>> = Vec::with_capacity(g.loops.len());
        for (li, lp) in g.loops.iter().enumerate() {
            let replaced: Vec<bool> = (0..lp.len()).map(|k| repl.contains_key(&(fid, li, k))).collect();
            let mut segs: Vec<Seg> = lp
                .iter()
                .enumerate()
                .map(|(k, s)| repl.get(&(fid, li, k)).cloned().unwrap_or_else(|| s.clone()))
                .collect();
            reconnect(&mut segs, &replaced, &name, &btol)?;
            loops.push(segs);
        }
        for (v, cap) in caps.get(&fid).map(Vec::as_slice).unwrap_or(&[]) {
            insert_cap(&mut loops, solid.vertices[*v].point, cap, &name, &btol)?;
        }
        let domain = grow_patch(&face.surface, face.same_sense, face.patch, &loops);
        let mut it = loops.into_iter();
        let outer = it.next().unwrap_or_default();
        pieces.push(Piece {
            surface: face.surface.clone(),
            same_sense: face.same_sense,
            domain,
            outer,
            holes: it.collect(),
            tag: face.tag.clone(),
        });
    }
    for (i, g) in geoms.iter().enumerate() {
        let (s, e) = (&plan.starts[i], &plan.ends[i]);
        let segs = vec![
            Seg::new(g.contact_a.clone(), s.sa, e.sa),
            e.curve.clone(),
            Seg::new(g.contact_b.clone(), e.sb, s.sb),
            s.curve.reversed(),
        ];
        let outer = orient_outer(&g.surface, g.same_sense, segs);
        pieces.push(Piece {
            surface: g.surface.clone(),
            same_sense: g.same_sense,
            domain: loop_patch(&g.surface, g.same_sense, &outer),
            outer,
            holes: Vec::new(),
            tag: Some(FaceTag { origin: 0, role: roles[i].clone() }),
        });
    }
    pieces.extend(plan.corners.iter().cloned());
    let result = sew(pieces, tol).map_err(|e| BlendError::ResultInvalid(e.to_string()))?;
    validate_with(&result, tol).map_err(|e| BlendError::ResultInvalid(e.to_string()))?;
    Ok(result)
}
```

- [ ] **Step 4: Modul eintragen** – `pub mod build;` in `crates/planora-kernel/src/blend/mod.rs`.

- [ ] **Step 5: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_build`
Expected: PASS (6 Tests). Schlägt `validate` fehl, die Schleifenrichtung der Rundungsfläche und das Ersetzen in der Halbkantenrichtung prüfen (nicht den Test ändern).

- [ ] **Step 6: Prüfen und committen**

Run: `cargo fmt && cargo clippy -p planora-kernel --all-targets -- -D warnings && cargo test -p planora-kernel`

```bash
git add crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/src/blend/build.rs crates/planora-kernel/tests/blend_build.rs
git commit -m "kernel: Endabschlüsse und Umbau der Nachbarflächen über sew

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Kernel-Operation `blend`, Prüfungen, Marken und Volumenschätzung

**Files:**
- Modify: `crates/planora-kernel/src/blend/mod.rs` (Typen, Einstieg), `crates/planora-kernel/src/kernel.rs:14-30,38-86` (Methode `blend`), `crates/planora-kernel/src/lib.rs` (Re-Exporte)
- Test: `crates/planora-kernel/tests/blend_api.rs`

**Interfaces:**
- Consumes: Task 1–4, `planora_math::nurbs::MAX_COORD` (1e9), `validate_with`, `boolean::{face_geom, point_in_face, Containment}`.
- Produces:
  ```rust
  pub enum BlendKind { Fillet { radius: f64 }, VariableFillet { points: Vec<(f64, f64)> },
      ChamferEqual { distance: f64 }, ChamferTwo { distance1: f64, distance2: f64 },
      ChamferAngle { distance: f64, angle: f64 } }               // Winkel im Bogenmaß, Punkte (Anteil 0..=1, Radius)
  impl BlendKind { pub fn is_chamfer(&self) -> bool }
  pub struct BlendEdge { pub edge: Id<Edge>, pub first_face: Option<Id<Face>> }
  pub struct BlendRequest { pub edges: Vec<BlendEdge>, pub kind: BlendKind, pub tangent_chain: bool }
  pub struct Blended { pub solid: Solid, pub volume_change: f64, pub edges: Vec<Id<Edge>> }
  pub const MAX_BLEND_EDGES: usize = 256;
  pub const MAX_RADIUS_POINTS: usize = 64;
  pub const MIN_CHAMFER_ANGLE: f64;                              // 1° im Bogenmaß
  pub fn check_request(r: &BlendRequest, tol: &Tolerance) -> Result<(), BlendError>;
  pub fn check_contacts(solid: &Solid, geoms: &[EdgeGeom], plan: &EndPlan, tol: &Tolerance) -> Result<(), BlendError>;
  pub fn blend(solid: &Solid, request: &BlendRequest, tol: &Tolerance) -> Result<Blended, BlendError>;
  // Kernel-Trait:
  fn blend(&self, solid: &Solid, request: &BlendRequest) -> Result<Blended, BlendError>;
  // planora_kernel::{BlendEdge, BlendError, BlendKind, BlendRequest, Blended} re-exportiert
  ```
  `volume_change` ist negativ, wenn Material abgetragen wird.

- [ ] **Step 1: Tests schreiben** (`crates/planora-kernel/tests/blend_api.rs`)

```rust
//! Kernel-Operation `blend` (Teilprojekt 8, Task 5).

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn req(edges: &[Id<Edge>], kind: BlendKind) -> BlendRequest {
    BlendRequest {
        edges: edges.iter().map(|&edge| BlendEdge { edge, first_face: None }).collect(),
        kind,
        tangent_chain: false,
    }
}

#[test]
fn a_fillet_through_the_kernel_trait() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 2.0 })).unwrap();
    k().validate(&b.solid).unwrap();
    assert_eq!(b.edges, vec![e]);
    let want = -(1.0 - PI / 4.0) * 4.0 * 10.0;
    assert!((b.volume_change - want).abs() < 1e-3 * want.abs(), "{}", b.volume_change);
    let tags: Vec<_> = b.solid.faces.iter().filter_map(|(_, f)| f.tag.clone()).collect();
    assert!(tags.iter().any(|t| t.role == "Rundung 1" && t.origin == 0));
    let c = k()
        .blend(&s, &req(&[e], BlendKind::ChamferAngle { distance: 1.0, angle: PI / 4.0 }))
        .unwrap();
    assert!(c.solid.faces.iter().any(|(_, f)| f.tag.as_ref().unwrap().role == "Fase 1"));
    assert!((c.volume_change + 0.5 * 10.0).abs() < 1e-9, "{}", c.volume_change);
}

#[test]
fn hostile_requests_never_panic() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let bad_kinds = [
        BlendKind::Fillet { radius: f64::NAN },
        BlendKind::Fillet { radius: f64::INFINITY },
        BlendKind::Fillet { radius: -1.0 },
        BlendKind::Fillet { radius: 0.0 },
        BlendKind::Fillet { radius: 1e300 },
        BlendKind::ChamferEqual { distance: f64::NEG_INFINITY },
        BlendKind::ChamferTwo { distance1: 1.0, distance2: -2.0 },
        BlendKind::ChamferAngle { distance: 1.0, angle: 0.0 },
        BlendKind::ChamferAngle { distance: 1.0, angle: PI },
        BlendKind::ChamferAngle { distance: 1.0, angle: f64::NAN },
        BlendKind::VariableFillet { points: vec![(0.0, 1.0)] },
        BlendKind::VariableFillet { points: vec![(0.5, 1.0), (0.2, 1.0)] },
        BlendKind::VariableFillet { points: vec![(0.0, 1.0), (1.5, 1.0)] },
        BlendKind::VariableFillet { points: vec![(0.0, 1.0), (1.0, -1.0)] },
        BlendKind::VariableFillet { points: (0..65).map(|i| (i as f64 / 64.0, 1.0)).collect() },
    ];
    for kind in bad_kinds {
        let r = k().blend(&s, &req(&[e], kind.clone()));
        assert!(matches!(r, Err(BlendError::InvalidParameter(_))), "{kind:?}: {:?}", r.map(|_| ()));
    }
    assert_eq!(
        k().blend(&s, &req(&[], BlendKind::Fillet { radius: 1.0 })).map(|_| ()),
        Err(BlendError::NoEdges)
    );
    let many: Vec<Id<Edge>> = (0..257).map(|_| e).collect();
    assert!(matches!(
        k().blend(&s, &req(&many, BlendKind::Fillet { radius: 1.0 })),
        Err(BlendError::InvalidParameter(_))
    ));
    assert_eq!(
        k().blend(&s, &req(&[Id::from_index(500)], BlendKind::Fillet { radius: 1.0 })).map(|_| ()),
        Err(BlendError::EdgeNotFound)
    );
    // Doppelte Kanten zählen einmal.
    let twice = k().blend(&s, &req(&[e, e], BlendKind::Fillet { radius: 1.0 })).unwrap();
    assert_eq!(twice.edges, vec![e]);
}

#[test]
fn too_large_sizes_are_refused_by_name() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    match k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 11.0 })) {
        Err(BlendError::TooLarge { face }) => assert!(face == "Deckel" || face == "Wand 1", "{face}"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    match k().blend(&s, &req(&[e], BlendKind::ChamferEqual { distance: 12.0 })) {
        Err(BlendError::TooLarge { .. }) => {}
        other => panic!("{:?}", other.map(|_| ())),
    }
    // Flache Platte: die Rundung bräuchte mehr Höhe, als die Wand hat.
    let flat = k().make_box(DVec3::new(10.0, 10.0, 1.0)).unwrap();
    let e = edge_between(&flat, "Deckel", "Wand 1");
    match k().blend(&flat, &req(&[e], BlendKind::Fillet { radius: 2.0 })) {
        Err(BlendError::TooLarge { face }) => assert_eq!(face, "Wand 1"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    let msg = BlendError::TooLarge { face: "Wand 1".into() }.to_string();
    assert!(msg.contains("„Wand 1“") && msg.contains("kleineres Maß"), "{msg}");
}

#[test]
fn stage_one_refuses_several_edges_and_variable_radii() {
    // Übergang (P12): wird in Task 9 bzw. Task 14 durch echte Ergebnisse ersetzt.
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&s, "Deckel", "Wand 1");
    let e2 = edge_between(&s, "Boden", "Wand 3");
    assert!(matches!(
        k().blend(&s, &req(&[e1, e2], BlendKind::Fillet { radius: 1.0 })),
        Err(BlendError::Unsupported(m)) if m.contains("Mehrere Kanten")
    ));
    assert!(matches!(
        k().blend(&s, &req(&[e1], BlendKind::VariableFillet { points: vec![(0.0, 1.0), (1.0, 2.0)] })),
        Err(BlendError::Unsupported(m)) if m.contains("variable")
    ));
}

#[test]
fn seams_tangents_and_curvature_are_named_through_the_api() {
    let c = k().make_cylinder(1.0, 5.0).unwrap();
    let seam = c.edges.iter().find(|(_, e)| matches!(e.curve, Curve::Line { .. })).unwrap().0;
    assert_eq!(
        k().blend(&c, &req(&[seam], BlendKind::Fillet { radius: 0.2 })).map(|_| ()),
        Err(BlendError::SeamEdge)
    );
    let top = edge_between(&c, "Wand 1", "Deckel");
    assert!(matches!(
        k().blend(&c, &req(&[top], BlendKind::Fillet { radius: 1.5 })),
        Err(BlendError::CurvatureTooSmall { .. })
    ));
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(10.0, 0.0) },
                Segment::Arc { center: DVec2::new(10.0, 2.0), sweep: PI },
                Segment::Line { to: DVec2::new(0.0, 4.0) },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let slot = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let e = edge_between(&slot, "Wand 1", "Wand 2");
    assert!(matches!(
        k().blend(&slot, &req(&[e], BlendKind::Fillet { radius: 0.5 })),
        Err(BlendError::NearlyTangent { .. })
    ));
}

#[test]
fn an_invalid_body_is_reported() {
    let mut s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    s.shells.clear();
    assert!(matches!(
        k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 1.0 })),
        Err(BlendError::InvalidInput(_))
    ));
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_api`
Expected: FAIL – `cannot find type BlendRequest`.

- [ ] **Step 3: Einstieg schreiben** – `crates/planora-kernel/src/blend/mod.rs` vollständig ersetzen:

```rust
//! Rundungen und Fasen (Teilprojekt 8): rollende Kugel, analytische Rundungsflächen, Umbau der
//! Nachbarflächen über `sew` (Spec `2026-10-13-rundungen-und-fasen-design.md`).

pub mod analytic;
pub mod build;
pub mod error;
pub mod section;
pub mod topo;

pub use error::BlendError;

use crate::boolean::{Containment, face_geom, point_in_face};
use crate::ids::Id;
use crate::topology::{Edge, Face, Solid};
use crate::validate::validate_with;
use analytic::{EdgeGeom, SectionKind, VolumeModel, analytic_geom};
use build::{EndCut, EndPlan, assemble, end_cuts};
use planora_math::nurbs::MAX_COORD;
use planora_math::{PI, Tolerance};
use section::ChamferSpec;
use topo::{EdgeSides, edge_angle, edge_sides, face_name};

/// Höchstzahl Kanten je Anfrage (nach Ausdehnung der Tangentenkette).
pub const MAX_BLEND_EDGES: usize = 256;
/// Höchstzahl Punkte eines variablen Radius.
pub const MAX_RADIUS_POINTS: usize = 64;
/// Kleinster (und π minus größter) Fasenwinkel: 1°.
pub const MIN_CHAMFER_ANGLE: f64 = PI / 180.0;
/// Stellen je Berührkurve, an denen geprüft wird, ob sie in ihrer Fläche liegt.
const CONTACT_PROBES: usize = 8;
/// Übergangsmeldung bis Task 9.
const SEVERAL_LATER: &str = "Mehrere Kanten in einem Feature werden noch nicht unterstützt";
/// Übergangsmeldung bis Task 14.
const VARIABLE_LATER: &str = "Der variable Radius wird noch nicht unterstützt";

/// Art der Verrundung (Maße in mm, Winkel im Bogenmaß).
#[derive(Clone, Debug, PartialEq)]
pub enum BlendKind {
    Fillet { radius: f64 },
    /// Punkte (Anteil 0..=1 der Kettenlänge, Radius), Anteile streng steigend.
    VariableFillet { points: Vec<(f64, f64)> },
    ChamferEqual { distance: f64 },
    /// Abstand auf Fläche 1 und auf Fläche 2.
    ChamferTwo { distance1: f64, distance2: f64 },
    /// Abstand auf Fläche 1 und Winkel der Fase zu Fläche 1.
    ChamferAngle { distance: f64, angle: f64 },
}

impl BlendKind {
    pub fn is_chamfer(&self) -> bool {
        matches!(
            self,
            BlendKind::ChamferEqual { .. } | BlendKind::ChamferTwo { .. } | BlendKind::ChamferAngle { .. }
        )
    }
}

/// Gewählte Kante; `first_face` ist Fläche 1 (Seite der Fasenmaße, P4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlendEdge {
    pub edge: Id<Edge>,
    pub first_face: Option<Id<Face>>,
}

/// Anfrage an den Kernel.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendRequest {
    pub edges: Vec<BlendEdge>,
    pub kind: BlendKind,
    /// Glatt anschließende Kanten mitnehmen (R8).
    pub tangent_chain: bool,
}

/// Ergebnis: neuer Körper, geschätzte Volumenänderung (mm³, negativ = Material weg) und die
/// tatsächlich gerundeten Kanten des Eingangskörpers.
#[derive(Clone, Debug)]
pub struct Blended {
    pub solid: Solid,
    pub volume_change: f64,
    pub edges: Vec<Id<Edge>>,
}

fn check_size(x: f64, what: &str, tol: &Tolerance) -> Result<(), BlendError> {
    if x.is_finite() && x > tol.linear && x <= MAX_COORD {
        Ok(())
    } else {
        Err(BlendError::InvalidParameter(format!(
            "{what} muss größer als {} mm und höchstens {MAX_COORD:e} mm sein (war {x})",
            tol.linear
        )))
    }
}

/// Prüft die Zahlen einer Anfrage (endlich, im Bereich, Punkte streng steigend).
pub fn check_request(r: &BlendRequest, tol: &Tolerance) -> Result<(), BlendError> {
    if r.edges.is_empty() {
        return Err(BlendError::NoEdges);
    }
    if r.edges.len() > MAX_BLEND_EDGES {
        return Err(BlendError::InvalidParameter(format!(
            "Zu viele Kanten (höchstens {MAX_BLEND_EDGES})"
        )));
    }
    match &r.kind {
        BlendKind::Fillet { radius } => check_size(*radius, "Der Radius", tol),
        BlendKind::VariableFillet { points } => {
            if points.len() < 2 || points.len() > MAX_RADIUS_POINTS {
                return Err(BlendError::InvalidParameter(format!(
                    "Ein variabler Radius braucht 2 bis {MAX_RADIUS_POINTS} Punkte"
                )));
            }
            for (i, (s, radius)) in points.iter().enumerate() {
                if !(s.is_finite() && (0.0..=1.0).contains(s)) {
                    return Err(BlendError::InvalidParameter(
                        "Die Positionen müssen zwischen 0 und 100 % liegen".into(),
                    ));
                }
                if i > 0 && !(*s > points[i - 1].0) {
                    return Err(BlendError::InvalidParameter(
                        "Die Positionen müssen streng steigen".into(),
                    ));
                }
                check_size(*radius, "Jeder Radius", tol)?;
            }
            Ok(())
        }
        BlendKind::ChamferEqual { distance } => check_size(*distance, "Der Abstand", tol),
        BlendKind::ChamferTwo { distance1, distance2 } => {
            check_size(*distance1, "Abstand 1", tol)?;
            check_size(*distance2, "Abstand 2", tol)
        }
        BlendKind::ChamferAngle { distance, angle } => {
            check_size(*distance, "Der Abstand", tol)?;
            if angle.is_finite() && *angle >= MIN_CHAMFER_ANGLE && *angle <= PI - MIN_CHAMFER_ANGLE {
                Ok(())
            } else {
                Err(BlendError::InvalidParameter(format!(
                    "Der Winkel der Fase muss zwischen 1° und 179° liegen (war {:.3}°)",
                    angle.to_degrees()
                )))
            }
        }
    }
}

/// Querschnittsart einer konstanten Verrundung.
fn section_kind(kind: &BlendKind) -> Result<SectionKind, BlendError> {
    Ok(match kind {
        BlendKind::Fillet { radius } => SectionKind::Fillet { radius: *radius },
        BlendKind::ChamferEqual { distance } => {
            SectionKind::Chamfer(ChamferSpec::Distances { a: *distance, b: *distance })
        }
        BlendKind::ChamferTwo { distance1, distance2 } => {
            SectionKind::Chamfer(ChamferSpec::Distances { a: *distance1, b: *distance2 })
        }
        BlendKind::ChamferAngle { distance, angle } => {
            SectionKind::Chamfer(ChamferSpec::DistanceAngle { distance: *distance, angle: *angle })
        }
        BlendKind::VariableFillet { .. } => {
            return Err(BlendError::Unsupported(VARIABLE_LATER.into()));
        }
    })
}

/// Kanten der Anfrage ohne Doppelte (Stufe 1: genau eine).
fn select_edges(solid: &Solid, r: &BlendRequest) -> Result<Vec<BlendEdge>, BlendError> {
    let mut out: Vec<BlendEdge> = Vec::new();
    for be in &r.edges {
        if solid.edges.get(be.edge).is_none() {
            return Err(BlendError::EdgeNotFound);
        }
        if !out.iter().any(|o| o.edge == be.edge) {
            out.push(*be);
        }
    }
    if out.len() > 1 {
        return Err(BlendError::Unsupported(SEVERAL_LATER.into()));
    }
    Ok(out)
}

fn unsupported_pair(solid: &Solid, sides: &EdgeSides) -> BlendError {
    let label = |f: Id<Face>| solid.faces[f].surface.kind().label();
    BlendError::Unsupported(format!(
        "Die Verrundung zwischen {} und {} an dieser Kante wird noch nicht unterstützt",
        label(sides.a.face),
        label(sides.b.face)
    ))
}

fn edge_geom(solid: &Solid, be: &BlendEdge, kind: SectionKind, tol: &Tolerance) -> Result<EdgeGeom, BlendError> {
    let sides = edge_sides(solid, be.edge, be.first_face)?;
    let angle = edge_angle(solid, be.edge, &sides)?;
    analytic_geom(solid, be.edge, &sides, angle.convex, kind, tol)?
        .ok_or_else(|| unsupported_pair(solid, &sides))
}

/// Liegt jede Berührkurve in ihrer Fläche (Stichproben im Inneren)? Sonst ist das Maß zu groß
/// für diese Fläche (Spec §3.4). Kehren sich die gekürzten Berührkurven um, ebenso.
pub fn check_contacts(
    solid: &Solid,
    geoms: &[EdgeGeom],
    plan: &EndPlan,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    for (i, g) in geoms.iter().enumerate() {
        let (s, e) = (&plan.starts[i], &plan.ends[i]);
        let full = g.range.1 - g.range.0;
        for (u, curve, s0, s1) in [
            (g.sides.a, &g.contact_a, s.sa, e.sa),
            (g.sides.b, &g.contact_b, s.sb, e.sb),
        ] {
            let too_large = || BlendError::TooLarge { face: face_name(solid, u.face) };
            if !((s1 - s0) * full.signum() > 0.0) {
                return Err(too_large());
            }
            let fg = face_geom(solid, u.face);
            for k in 1..CONTACT_PROBES {
                let t = s0 + (s1 - s0) * k as f64 / CONTACT_PROBES as f64;
                if point_in_face(&fg, curve.point(t), tol) == Containment::Outside {
                    return Err(too_large());
                }
            }
        }
    }
    Ok(())
}

/// Geschätzte Volumenänderung einer Kante (P2).
fn edge_volume(g: &EdgeGeom, s: &EndCut, e: &EndCut) -> f64 {
    let sign = if g.convex { -1.0 } else { 1.0 };
    let span = 0.5 * ((e.sa - s.sa).abs() + (e.sb - s.sb).abs());
    sign * match g.volume {
        VolumeModel::Prism { area } => area * span,
        VolumeModel::Pappus { area, centroid_radius } => area * centroid_radius * span,
        VolumeModel::Total { volume } => volume,
    }
}

/// Rollen der neuen Flächen (P9).
fn roles(kind: &BlendKind, n: usize) -> Vec<String> {
    let label = if kind.is_chamfer() { "Fase" } else { "Rundung" };
    (1..=n).map(|i| format!("{label} {i}")).collect()
}

/// Rundet bzw. fast die Kanten der Anfrage. Der Eingangskörper bleibt unverändert; jedes
/// Scheitern ist eine benannte Ablehnung.
pub fn blend(solid: &Solid, request: &BlendRequest, tol: &Tolerance) -> Result<Blended, BlendError> {
    check_request(request, tol)?;
    validate_with(solid, tol).map_err(|e| BlendError::InvalidInput(e.to_string()))?;
    let kind = section_kind(&request.kind)?;
    let edges = select_edges(solid, request)?;
    let geoms = edges
        .iter()
        .map(|be| edge_geom(solid, be, kind, tol))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = end_cuts(solid, &geoms, tol)?;
    check_contacts(solid, &geoms, &plan, tol)?;
    let volume_change = geoms
        .iter()
        .enumerate()
        .map(|(i, g)| edge_volume(g, &plan.starts[i], &plan.ends[i]))
        .sum::<f64>()
        + plan.corner_volume;
    let result = assemble(solid, &geoms, &plan, &roles(&request.kind, geoms.len()), tol)?;
    Ok(Blended {
        solid: result,
        volume_change,
        edges: edges.iter().map(|e| e.edge).collect(),
    })
}
```

- [ ] **Step 4: Kernel-Schnittstelle erweitern**

In `crates/planora-kernel/src/kernel.rs` oben `use crate::blend::{BlendError, BlendRequest, Blended};` ergänzen, im Trait nach `boolean` einfügen:

```rust
    /// Rundet bzw. fast Kanten (Teilprojekt 8); der Eingangskörper bleibt unverändert.
    fn blend(&self, solid: &Solid, request: &BlendRequest) -> Result<Blended, BlendError>;
```

und in `impl Kernel for AnalyticKernel` nach `boolean`:

```rust
    fn blend(&self, solid: &Solid, request: &BlendRequest) -> Result<Blended, BlendError> {
        crate::blend::blend(solid, request, &self.tolerance)
    }
```

In `crates/planora-kernel/src/lib.rs` nach `pub use boolean::{…};` einfügen:

```rust
pub use blend::{BlendEdge, BlendError, BlendKind, BlendRequest, Blended};
```

- [ ] **Step 5: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_api`
Expected: PASS (6 Tests). Die Volumenschätzung der Winkelfase (45°, Abstand 1, Länge 10) ist exakt `−½ · 1 · 1 · 10`.

- [ ] **Step 6: Prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p planora-kernel`

```bash
git add crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/src/kernel.rs crates/planora-kernel/src/lib.rs crates/planora-kernel/tests/blend_api.rs
git commit -m "kernel: Operation blend mit Prüfungen, Marken und Volumenschätzung

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Kern-Gesamttests Stufe 1 (Netz, Volumen, Folgeoperationen) und Volumenprobe

**Files:**
- Modify: `crates/planora-mesh/src/volume.rs` (neue Funktionen), `crates/planora-mesh/src/lib.rs:16` (Re-Export)
- Create: `crates/planora-mesh/tests/blend_common/mod.rs`, `crates/planora-mesh/tests/blend_stage1.rs`

**Interfaces:**
- Consumes: `Kernel::blend` (Task 5), `Discretizer::for_solids`, `tessellate_with`, `mass_properties`, `is_watertight`, `check_face_boundaries`, `boolean::{solid_contains, solid_faces, Location}`.
- Produces:
  ```rust
  // planora_mesh
  pub fn blend_volume_plausible(v_in: f64, v_out: f64, estimate: f64, rel: f64, slack: f64) -> bool;
  pub fn curved_area(solid: &Solid, mesh: &Mesh) -> f64;
  // tests/blend_common (Testhilfen, auch für Task 12 und 16)
  pub fn k() -> AnalyticKernel; pub fn tol() -> Tolerance;
  pub fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge>;
  pub fn blend_edges(s: &Solid, edges: &[Id<Edge>], kind: BlendKind, chain: bool) -> Result<Blended, BlendError>;
  pub fn delta_volume(before: &Solid, after: &Solid, chord: f64) -> f64;
  pub fn assert_rel(got: f64, want: f64, rel: f64, what: &str);
  pub fn extrude_polygon(points: &[DVec2], h: f64) -> Solid;
  pub fn hemisphere(r: f64) -> Solid;
  pub fn spandrel(theta: f64, r: f64) -> (f64, f64);
  pub fn plane_circle_spandrel(big: f64, r: f64) -> (f64, DVec2);
  pub fn plausible(before: &Solid, b: &Blended, chord: f64) -> bool;
  pub struct Lcg(pub u64);  impl Lcg { pub fn next(&mut self) -> f64; pub fn range(&mut self, a: f64, b: f64) -> f64; }
  ```

- [ ] **Step 1: Unit-Tests der Volumenprobe schreiben** – in `crates/planora-mesh/src/volume.rs` im Modul `tests` ergänzen:

```rust
    #[test]
    fn blend_volumes_must_match_sign_and_estimate() {
        // Rundung trägt 30 ab (Schätzung −30), Netz sagt −28: plausibel.
        assert!(blend_volume_plausible(1000.0, 972.0, -30.0, 0.5, 0.5));
        // Falsches Vorzeichen: Material kam hinzu, obwohl abgetragen werden sollte.
        assert!(!blend_volume_plausible(1000.0, 1005.0, -30.0, 0.5, 0.5));
        // Viel zu viel abgetragen.
        assert!(!blend_volume_plausible(1000.0, 900.0, -30.0, 0.5, 0.5));
        // Konkav: Material kommt hinzu.
        assert!(blend_volume_plausible(1000.0, 1010.0, 10.0, 0.5, 0.5));
        assert!(!blend_volume_plausible(1000.0, f64::NAN, 10.0, 0.5, 0.5));
        assert!(!blend_volume_plausible(1000.0, -1.0, -1001.0, 0.5, 0.5));
    }
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-mesh --lib volume`
Expected: FAIL – `cannot find function blend_volume_plausible`.

- [ ] **Step 3: Funktionen schreiben** – in `crates/planora-mesh/src/volume.rs` oben `use crate::mesh::Mesh;` und `use planora_kernel::{Solid, Surface};` ergänzen und nach `boolean_volume_plausible` einfügen:

```rust
/// Volumenprobe einer Verrundung (Spec §4.5, Plan P2): `estimate` ist die Schätzung des
/// Kernels (negativ = Material abgetragen). Die gemessene Änderung `v_out − v_in` muss das
/// Vorzeichen der Schätzung haben (bis `slack`) und höchstens `rel · |estimate| + slack` von ihr
/// abweichen. `slack` deckt den Netzfehler ab (Sehnentoleranz mal gekrümmte Oberfläche).
pub fn blend_volume_plausible(v_in: f64, v_out: f64, estimate: f64, rel: f64, slack: f64) -> bool {
    let d = v_out - v_in;
    if !(d.is_finite() && estimate.is_finite() && v_out > 0.0) {
        return false;
    }
    if estimate < 0.0 && d > slack {
        return false;
    }
    if estimate > 0.0 && d < -slack {
        return false;
    }
    (d - estimate).abs() <= rel * estimate.abs() + slack
}

/// Summe der Dreiecksflächen aller nicht ebenen Flächen des Körpers (ebene Flächen werden exakt
/// vernetzt und tragen keinen Volumenfehler bei).
pub fn curved_area(solid: &Solid, mesh: &Mesh) -> f64 {
    let mut area = 0.0;
    for r in &mesh.face_ranges {
        let curved = solid
            .faces
            .get(r.face)
            .is_some_and(|f| !matches!(f.surface, Surface::Plane { .. }));
        if !curved {
            continue;
        }
        let start = r.first_index as usize;
        let end = start + r.index_count as usize;
        for t in mesh.indices[start..end].chunks_exact(3) {
            let a = mesh.positions[t[0] as usize];
            let b = mesh.positions[t[1] as usize];
            let c = mesh.positions[t[2] as usize];
            area += 0.5 * (b - a).cross(c - a).length();
        }
    }
    area
}
```

und in `crates/planora-mesh/src/lib.rs` die Zeile `pub use volume::boolean_volume_plausible;` ersetzen durch:

```rust
pub use volume::{blend_volume_plausible, boolean_volume_plausible, curved_area};
```

- [ ] **Step 4: Testhilfen schreiben** (`crates/planora-mesh/tests/blend_common/mod.rs`)

```rust
//! Gemeinsame Hilfen der Tests zu Rundungen und Fasen (`blend_stage1.rs`, `blend_stage2.rs`,
//! `blend_stage3.rs`).

#![allow(dead_code)]

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_mesh::{
    Discretizer, blend_volume_plausible, check_face_boundaries, curved_area, is_watertight,
    mass_properties, tessellate_with,
};

pub fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

pub fn tol() -> Tolerance {
    Tolerance::default()
}

/// Kante zwischen den Flächen mit den Rollen `a` und `b` (Reihenfolge egal).
pub fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

pub fn blend_edges(s: &Solid, edges: &[Id<Edge>], kind: BlendKind, chain: bool) -> Result<Blended, BlendError> {
    k().blend(
        s,
        &BlendRequest {
            edges: edges.iter().map(|&edge| BlendEdge { edge, first_face: None }).collect(),
            kind,
            tangent_chain: chain,
        },
    )
}

/// Netzvolumen nachher minus vorher mit gemeinsamer Teilung. Prüft das Ergebnis: `validate`,
/// Rand jeder Fläche aus ihren Kantenpunkten, geschlossenes Netz.
pub fn delta_volume(before: &Solid, after: &Solid, chord: f64) -> f64 {
    validate(after).unwrap_or_else(|e| panic!("validate: {e}"));
    let disc = Discretizer::for_solids(&[before, after], chord);
    let mb = tessellate_with(before, &disc).unwrap();
    let ma = tessellate_with(after, &disc).unwrap_or_else(|e| panic!("Netz: {e}"));
    check_face_boundaries(after, &ma).unwrap_or_else(|e| panic!("{e}"));
    assert!(is_watertight(&ma, 1e-6), "Netz nicht geschlossen");
    mass_properties(&ma).volume - mass_properties(&mb).volume
}

/// Volumenprobe wie im Feature (P2) mit Sehnentoleranz `chord`.
pub fn plausible(before: &Solid, b: &Blended, chord: f64) -> bool {
    let disc = Discretizer::for_solids(&[before, &b.solid], chord);
    let mb = tessellate_with(before, &disc).unwrap();
    let ma = tessellate_with(&b.solid, &disc).unwrap();
    let slack = 2.0 * disc.chord_tolerance() * (curved_area(before, &mb) + curved_area(&b.solid, &ma));
    blend_volume_plausible(mass_properties(&mb).volume, mass_properties(&ma).volume, b.volume_change, 0.5, slack)
}

pub fn assert_rel(got: f64, want: f64, rel: f64, what: &str) {
    assert!(
        (got - want).abs() <= rel * want.abs(),
        "{what}: {got} statt {want} (relativ {})",
        ((got - want) / want).abs()
    );
}

pub fn extrude_polygon(points: &[DVec2], h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::polygon(points).unwrap(),
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

/// Halbkugel über der xy-Ebene: „Fläche 1“ Boden (Kreisscheibe), „Fläche 2“ Kugelfläche.
pub fn hemisphere(r: f64) -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(r, 0.0) },
                Segment::Arc { center: DVec2::ZERO, sweep: PI / 2.0 },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

/// Querschnitt zwischen einer Kante mit Innenwinkel `theta` (zwei Ebenen) und der Rundung mit
/// Radius `r`: (Fläche, Abstand des Schwerpunkts von der Kante längs der Winkelhalbierenden).
/// Drachen (Kante, zwei Berührpunkte, Mittelpunkt) minus Kreissektor mit Öffnung `π − theta`.
pub fn spandrel(theta: f64, r: f64) -> (f64, f64) {
    let half = theta / 2.0;
    let t = r / half.tan();
    let d = r / half.sin();
    let kite = r * t;
    let kite_c = (t * half.cos() + d) / 3.0;
    let alpha = PI - theta;
    let sector = 0.5 * r * r * alpha;
    let sector_c = d - 4.0 * r * (alpha / 2.0).sin() / (3.0 * alpha);
    let area = kite - sector;
    (area, (kite * kite_c - sector * sector_c) / area)
}

/// Fläche und Schwerpunkt (in der Ebene (ρ, h)) des Gebiets zwischen einer Ebene `h = 0`
/// (Material darüber), einem Kreis um den Ursprung mit Radius `big` (Material innen) und der
/// Rundung mit Radius `r`, die beide innen berührt; fein abgetastet (Schnürsenkelformel).
pub fn plane_circle_spandrel(big: f64, r: f64) -> (f64, DVec2) {
    const N: usize = 4000;
    let c = DVec2::new(((big - r) * (big - r) - r * r).sqrt(), r);
    let pp = DVec2::new(c.x, 0.0);
    let ps_angle = c.y.atan2(c.x);
    let mut pts = vec![pp];
    for i in 0..=N {
        let a = ps_angle * i as f64 / N as f64;
        pts.push(DVec2::new(a.cos(), a.sin()) * big);
    }
    // Rundungsbogen von der Kugel (Richtung c) zurück zur Ebene (Richtung −y) über die Ecke.
    let (b0, b1) = (ps_angle, -PI / 2.0);
    for i in 1..N {
        let a = b0 + (b1 - b0) * i as f64 / N as f64;
        pts.push(c + DVec2::new(a.cos(), a.sin()) * r);
    }
    let n = pts.len();
    let (mut area, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let cr = p.x * q.y - q.x * p.y;
        area += cr;
        cx += (p.x + q.x) * cr;
        cy += (p.y + q.y) * cr;
    }
    area *= 0.5;
    (area.abs(), DVec2::new(cx / (6.0 * area), cy / (6.0 * area)))
}

/// Kleiner Zufallsgenerator mit festem Startwert (keine Abhängigkeit).
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

/// Punkt im Abstand `d` von `p` in Richtung `dir` (für Lageprüfungen an robusten Punkten).
pub fn offset(p: DVec3, dir: DVec3, d: f64) -> DVec3 {
    p + dir.normalize() * d
}
```

- [ ] **Step 5: Gesamttests schreiben** (`crates/planora-mesh/tests/blend_stage1.rs`)

```rust
//! Rundungen und Fasen, Stufe 1: geschlossene Formeln, geschlossenes Netz, Folgeoperationen
//! (Teilprojekt 8, Task 6). Netzvolumen mit Sehnentoleranz 1e-3 und gemeinsamer Teilung.

mod blend_common;

use blend_common::*;
use planora_kernel::boolean::{Location, solid_contains, solid_faces};
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, PI};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

const CHORD: f64 = 1e-3;
/// Erlaubter relativer Fehler der Volumenänderung gegen die Formel.
const REL: f64 = 5e-3;

fn box10() -> Solid {
    k().make_box(DVec3::splat(10.0)).unwrap()
}

#[test]
fn a_box_edge_fillet_removes_the_spandrel_prism() {
    let s = box10();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: 2.0 }, false).unwrap();
    let want = -(1.0 - PI / 4.0) * 4.0 * 10.0;
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Rundung");
    assert_rel(b.volume_change, want, 1e-3, "Schätzung");
    assert!(plausible(&s, &b, 0.05));
}

#[test]
fn box_chamfers_match_their_triangles() {
    let s = box10();
    let e = edge_between(&s, "Deckel", "Wand 1");
    for (kind, want) in [
        (BlendKind::ChamferEqual { distance: 1.5 }, -0.5 * 1.5 * 1.5 * 10.0),
        (BlendKind::ChamferTwo { distance1: 1.0, distance2: 2.5 }, -0.5 * 2.5 * 10.0),
        (
            BlendKind::ChamferAngle { distance: 1.0, angle: 30f64.to_radians() },
            -0.5 * 30f64.to_radians().tan() * 10.0,
        ),
    ] {
        let b = blend_edges(&s, &[e], kind.clone(), false).unwrap();
        assert_rel(delta_volume(&s, &b.solid, CHORD), want, 1e-9, &format!("{kind:?}"));
        assert_rel(b.volume_change, want, 1e-9, "Schätzung");
    }
}

#[test]
fn cylinder_top_fillet_and_chamfer_follow_pappus() {
    let (big, h, r) = (5.0, 8.0, 1.0);
    let s = k().make_cylinder(big, h).unwrap();
    let e = edge_between(&s, "Wand 1", "Deckel");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, false).unwrap();
    let (area, dist) = spandrel(PI / 2.0, r);
    let want = -area * 2.0 * PI * (big - dist / 2f64.sqrt());
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Torus");
    assert!(plausible(&s, &b, 0.05));
    let c = blend_edges(&s, &[e], BlendKind::ChamferEqual { distance: r }, false).unwrap();
    let want = -0.5 * r * r * 2.0 * PI * (big - r / 3.0);
    assert_rel(delta_volume(&s, &c.solid, CHORD), want, REL, "Kegelfase");
}

#[test]
fn cone_base_fillet_matches_its_spandrel() {
    let (big, h, r) = (4.0, 6.0, 0.5);
    let s = k().make_cone(big, h).unwrap();
    let e = edge_between(&s, "Fläche 1", "Fläche 2");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, false).unwrap();
    let u1 = DVec2::new(-1.0, 0.0);
    let u2 = DVec2::new(-big, h).normalize();
    let theta = u1.dot(u2).acos();
    let (area, dist) = spandrel(theta, r);
    let bis = (u1 + u2).normalize();
    let want = -area * 2.0 * PI * (big + bis.x * dist);
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Kegel");
}

#[test]
fn hemisphere_base_fillet_matches_a_fine_reference() {
    let (big, r) = (5.0, 1.0);
    let s = hemisphere(big);
    let e = edge_between(&s, "Fläche 1", "Fläche 2");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, false).unwrap();
    let (area, centroid) = plane_circle_spandrel(big, r);
    let want = -area * 2.0 * PI * centroid.x;
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Halbkugel");
}

#[test]
fn extrusions_concave_sixty_degrees_and_plane_cylinder_edges() {
    // Konkave Kante des L-Profils: Material kommt hinzu.
    let l = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    );
    let e = edge_between(&l, "Wand 3", "Wand 4");
    let b = blend_edges(&l, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    assert_rel(delta_volume(&l, &b.solid, CHORD), (1.0 - PI / 4.0) * 5.0, REL, "konkav");
    assert!(b.volume_change > 0.0);
    // Gleichseitiges Dreieck: Innenwinkel 60°.
    let tri = extrude_polygon(
        &[DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0), DVec2::new(5.0, 75f64.sqrt())],
        4.0,
    );
    let e = edge_between(&tri, "Wand 1", "Wand 2");
    let b = blend_edges(&tri, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    let want = -(3f64.sqrt() - PI / 3.0) * 4.0;
    assert_rel(delta_volume(&tri, &b.solid, CHORD), want, REL, "60°");
    // Viertelscheibe: senkrechte Kante zwischen Ebene und Zylinder (gerade Kante, Zylinderrundung).
    let quarter = k()
        .extrude(
            &Profile {
                outer: ProfileLoop {
                    start: DVec2::ZERO,
                    segments: vec![
                        Segment::Line { to: DVec2::new(6.0, 0.0) },
                        Segment::Arc { center: DVec2::ZERO, sweep: PI / 2.0 },
                        Segment::Line { to: DVec2::ZERO },
                    ],
                },
                holes: vec![],
            },
            &planora_math::Frame::WORLD,
            3.0,
        )
        .unwrap();
    let e = edge_between(&quarter, "Wand 1", "Wand 2");
    let b = blend_edges(&quarter, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    let (area, _) = plane_circle_spandrel(6.0, 1.0);
    assert_rel(delta_volume(&quarter, &b.solid, CHORD), -area * 3.0, REL, "Ebene–Zylinder");
}

#[test]
fn follow_up_operations_on_filleted_bodies() {
    let s = box10();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let one = blend_edges(&s, &[e], BlendKind::Fillet { radius: 2.0 }, false).unwrap().solid;
    let spandrel_prism = (1.0 - PI / 4.0) * 4.0 * 10.0;
    // Boolesche Operation durch die Rundungsfläche hindurch.
    let cutter = k()
        .transform(
            &k().make_box(DVec3::new(4.0, 3.0, 12.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(3.0, -1.0, -1.0)),
        )
        .unwrap();
    let cut = k().boolean(&one, &cutter, BoolOp::Difference).unwrap();
    validate(&cut).unwrap();
    let v = mass_properties(&tessellate(&cut, &TessellationOptions { chord_tolerance: CHORD }).unwrap()).volume;
    // Weg fällt der Quaderteil y ∈ [0, 2] × x ∈ [3, 7] × z ∈ [0, 10] ohne den schon fehlenden
    // Zwickel auf diesen 4 mm.
    let want = 1000.0 - spandrel_prism - (4.0 * 2.0 * 10.0 - (1.0 - PI / 4.0) * 4.0 * 4.0);
    assert_rel(v, want, 1e-4, "Abziehen durch die Rundung");
    // Zweite Rundung an einer anderen Kante: die Änderungen addieren sich.
    let e2 = edge_between(&one, "Boden", "Wand 3");
    let two = blend_edges(&one, &[e2], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    assert_rel(delta_volume(&one, &two.solid, CHORD), -(1.0 - PI / 4.0) * 10.0, REL, "zweite Rundung");
    // Rundung an der Kante zwischen Rundungsfläche und Deckel: tangential, benannt.
    let tangent = edge_between(&one, "Rundung 1", "Deckel");
    assert!(matches!(
        blend_edges(&one, &[tangent], BlendKind::Fillet { radius: 0.5 }, false),
        Err(BlendError::NearlyTangent { .. })
    ));
}

#[test]
fn robust_points_lie_on_the_right_side() {
    let s = box10();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: 2.0 }, false).unwrap();
    let faces = solid_faces(&b.solid);
    let at = |p: DVec3| solid_contains(&faces, p, &tol()).unwrap();
    // Im abgetragenen Zwickel (0,3 von der alten Kante längs der Winkelhalbierenden).
    assert_eq!(at(DVec3::new(5.0, 0.2, 9.8)), Location::Outside);
    // Kugelmittelpunkt und Mitte des Körpers: innen.
    assert_eq!(at(DVec3::new(5.0, 2.0, 8.0)), Location::Inside);
    assert_eq!(at(DVec3::new(5.0, 5.0, 5.0)), Location::Inside);
    // Konkav: Punkt im neuen Material.
    let l = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    );
    let e = edge_between(&l, "Wand 3", "Wand 4");
    let b = blend_edges(&l, &[e], BlendKind::Fillet { radius: 1.0 }, false).unwrap();
    let faces = solid_faces(&b.solid);
    assert_eq!(solid_contains(&faces, DVec3::new(4.1, 4.1, 2.5), &tol()).unwrap(), Location::Inside);
    assert_eq!(solid_contains(&faces, DVec3::new(4.9, 4.9, 2.5), &tol()).unwrap(), Location::Outside);
}
```

- [ ] **Step 6: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-mesh --test blend_stage1 && cargo test -p planora-mesh --lib volume`
Expected: PASS (8 + 2 Tests), Laufzeit im Debug-Build unter 30 s. Weicht ein Volumen ab, zuerst die geschlossene Formel und die Kantenwahl im Test prüfen, dann den Kernel; die Schranken `REL = 5e-3` nicht erweitern.

- [ ] **Step 7: Prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p planora-mesh && git diff --stat f2f748d -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs`
Expected: grün; die letzte Ausgabe ist leer.

```bash
git add crates/planora-mesh/src/volume.rs crates/planora-mesh/src/lib.rs crates/planora-mesh/tests/blend_common/mod.rs crates/planora-mesh/tests/blend_stage1.rs
git commit -m "mesh: Volumenprobe der Verrundung und Gesamttests Stufe 1

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Feature `Verrunden` (Zeitleiste, Berechnung, Volumenprobe, alle `match`-Stellen)

**Files:**
- Modify: `crates/planora-feature/src/model.rs` (`BlendArt`, `FeatureKind::Verrunden`, `label`, `dependencies`), `crates/planora-feature/src/compute.rs` (Prüfung, Berechnung, Volumenprobe, `blend_error`), `crates/planora-feature/src/recompute.rs:248-252,263-271`, `crates/planora-feature/src/refs.rs` (`first_face`), `crates/planora-feature/src/error.rs` (`ImplausibleBlend`), `crates/planora-feature/src/lib.rs`
- Modify (erschöpfende `match`-Stellen, sonst baut der Arbeitsbereich nicht): `crates/planora-app/src/properties.rs:63-150`, `crates/planora-app/src/icons.rs:62-94`, `crates/planora-app/src/feature_dialog.rs:32-105,254-263`, `crates/planora-app/src/browser.rs:83-88`, `crates/planora-app/src/state.rs:1347-1356`, `crates/planora-project/src/format.rs:233-274`, `crates/planora-project/src/convert.rs:409-595`
- Test: `crates/planora-feature/tests/blend_features.rs`, `crates/planora-project/tests/blend_roundtrip.rs`

**Interfaces:**
- Consumes: `Kernel::blend`, `BlendEdge`, `BlendKind`, `BlendRequest`, `Blended`, `BlendError` (Task 5); `planora_mesh::{blend_volume_plausible, curved_area, Discretizer, tessellate_with, mass_properties}` (Task 6); `EdgeRef::from_edge`, `resolve_edge`.
- Produces:
  ```rust
  // planora_feature
  pub enum BlendArt { Rundung { radius: f64 }, RundungVariabel { punkte: Vec<(f64, f64)> },
      FaseGleich { abstand: f64 }, FaseZwei { abstand1: f64, abstand2: f64 }, FaseWinkel { abstand: f64, winkel: f64 } }
  impl BlendArt { pub fn is_chamfer(&self) -> bool; pub fn label(&self) -> &'static str;
      pub fn kind_label(&self) -> &'static str; pub fn numbers(&self) -> Vec<f64>; pub fn kernel_kind(&self) -> BlendKind; }
  FeatureKind::Verrunden { body: BodyRef, edges: Vec<EdgeRef>, art: BlendArt, tangentenkette: bool }
  FeatureError::ImplausibleBlend
  pub fn refs::first_face(solid: &Solid, edge: Id<Edge>, r: &FaceRef) -> Option<Id<Face>>;
  // planora_app::icons
  FeatureIcon::Blend
  // planora_project::format
  pub enum BlendArtDto { Rundung { radius: f64 }, RundungVariabel { punkte: Vec<[f64; 2]> },
      FaseGleich { abstand: f64 }, FaseZwei { abstand1: f64, abstand2: f64 }, FaseWinkel { abstand: f64, winkel: f64 } }
  KindDto::Verrunden { koerper: BodyRefDto, kanten: Vec<EdgeRefDto>, art: BlendArtDto, tangentenkette: bool }
  ```
  Einheiten: `winkel` in Grad, `punkte` als (Prozent 0–100, Radius); `kernel_kind` rechnet in Bogenmaß und Anteile um.

- [ ] **Step 1: Feature-Tests schreiben** (`crates/planora-feature/tests/blend_features.rs`)

```rust
//! Feature „Verrunden“: Zeitleiste, Neuberechnung, Verweise, Fehler (Teilprojekt 8, Task 7).

use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DVec3, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

fn volume(s: &Solid) -> f64 {
    mass_properties(&tessellate(s, &TessellationOptions { chord_tolerance: 1e-3 }).unwrap()).volume
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn edge_ref(doc: &Document, body: BodyRef, a: &str, b: &str) -> EdgeRef {
    let s = doc.body(body).unwrap();
    EdgeRef::from_edge(s, edge_between(s, a, b), &Tolerance::default()).unwrap()
}

fn with_box(size: f64) -> (Document, BodyRef) {
    let mut d = Document::new();
    let f = d
        .add_feature("Quader 1", FeatureKind::Box { size: DVec3::splat(size), origin: DVec3::ZERO })
        .unwrap();
    (d, BodyRef { feature: f, index: 0 })
}

fn fillet(body: BodyRef, edges: Vec<EdgeRef>, art: BlendArt) -> FeatureKind {
    FeatureKind::Verrunden { body, edges, art, tangentenkette: true }
}

fn status(d: &Document, id: FeatureId) -> FeatureStatus {
    d.feature(id).unwrap().status.clone()
}

#[test]
fn a_fillet_feature_replaces_its_body_and_follows_edits() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f = d.add_feature("Rundung 1", fillet(b, vec![r], BlendArt::Rundung { radius: 2.0 })).unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    assert_eq!(d.feature(f).unwrap().kind.label(), "Rundung");
    assert_eq!(d.feature(f).unwrap().kind.dependencies(), vec![b.feature]);
    let visible = d.visible_bodies();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].body.feature, f);
    let spandrel = (1.0 - PI / 4.0) * 4.0;
    assert!((volume(visible[0].solid) - (1000.0 - spandrel * 10.0)).abs() < 0.05);
    // Größerer Quader: die Kante wird über Marke und Hinweis wiedergefunden.
    d.update(b.feature, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::splat(12.0);
        }
    })
    .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - (1728.0 - spandrel * 12.0)).abs() < 0.05, "{v}");
    // Neue Flächen tragen die Herkunft des Features.
    let s = d.visible_bodies()[0].solid;
    assert!(s.faces.iter().any(|(_, x)| x.tag.as_ref().is_some_and(|t| t.role == "Rundung 1" && t.origin == f.0)));
    d.check_invariants().unwrap();
}

#[test]
fn chamfer_arts_compute_and_switch() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f = d
        .add_feature("Fase 1", fillet(b, vec![r], BlendArt::FaseZwei { abstand1: 1.0, abstand2: 2.0 }))
        .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    assert_eq!(d.feature(f).unwrap().kind.label(), "Fase");
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - (1000.0 - 0.5 * 2.0 * 10.0)).abs() < 1e-6, "{v}");
    d.update(f, |k| {
        if let FeatureKind::Verrunden { art, .. } = k {
            *art = BlendArt::FaseWinkel { abstand: 1.0, winkel: 45.0 };
        }
    })
    .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - (1000.0 - 0.5 * 10.0)).abs() < 1e-6, "{v}");
}

#[test]
fn hostile_blend_parameters_become_error_status() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let arts = [
        BlendArt::Rundung { radius: f64::NAN },
        BlendArt::Rundung { radius: -1.0 },
        BlendArt::Rundung { radius: 1e300 },
        BlendArt::FaseWinkel { abstand: 1.0, winkel: 0.0 },
        BlendArt::FaseWinkel { abstand: 1.0, winkel: 180.0 },
        BlendArt::RundungVariabel { punkte: vec![(50.0, 1.0), (10.0, 1.0)] },
        BlendArt::RundungVariabel { punkte: vec![] },
    ];
    for art in arts {
        let f = d.add_feature("X", fillet(b, vec![r.clone()], art.clone())).unwrap();
        assert!(matches!(status(&d, f), FeatureStatus::Error(_)), "{art:?}");
        // Ein Folgefeature auf dem Ergebnis wird übersprungen oder scheitert, rechnet aber nie.
        let m = d
            .add_feature("M", FeatureKind::Move { body: BodyRef { feature: f, index: 0 }, delta: DVec3::X })
            .unwrap();
        assert!(!matches!(status(&d, m), FeatureStatus::Ok), "{art:?}");
        d.delete(m).unwrap();
        d.delete(f).unwrap();
        d.check_invariants().unwrap();
    }
    let f = d.add_feature("Leer", fillet(b, vec![], BlendArt::Rundung { radius: 1.0 })).unwrap();
    match status(&d, f) {
        FeatureStatus::Error(e) => assert!(e.contains("keine Kante"), "{e}"),
        s => panic!("{s:?}"),
    }
}

#[test]
fn blend_references_follow_upstream_changes() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f1 = d.add_feature("Rundung 1", fillet(b, vec![r.clone()], BlendArt::Rundung { radius: 1.0 })).unwrap();
    // Dieselbe Kante noch einmal: sie ist durch die erste Rundung verschwunden.
    let f2 = d
        .add_feature("Rundung 2", fillet(BodyRef { feature: f1, index: 0 }, vec![r], BlendArt::Rundung { radius: 1.0 }))
        .unwrap();
    match status(&d, f2) {
        FeatureStatus::Error(e) => assert!(e.contains("nicht mehr vorhanden"), "{e}"),
        s => panic!("{s:?}"),
    }
    // Unterdrücken des Quaders: beide Rundungen werden übersprungen, Dokument bleibt gültig.
    d.set_suppressed(b.feature, true).unwrap();
    assert!(matches!(status(&d, f1), FeatureStatus::Skipped(_)));
    d.set_suppressed(b.feature, false).unwrap();
    assert_eq!(status(&d, f1), FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

#[test]
fn rounding_a_rounded_edge_is_named() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f1 = d.add_feature("Rundung 1", fillet(b, vec![r], BlendArt::Rundung { radius: 2.0 })).unwrap();
    let body = BodyRef { feature: f1, index: 0 };
    let tangent = edge_ref(&d, body, "Rundung 1", "Deckel");
    let f2 = d.add_feature("Rundung 2", fillet(body, vec![tangent], BlendArt::Rundung { radius: 0.5 })).unwrap();
    match status(&d, f2) {
        FeatureStatus::Error(e) => assert!(e.contains("tangential"), "{e}"),
        s => panic!("{s:?}"),
    }
    // Eine andere Kante des Ergebnisses geht.
    let other = edge_ref(&d, body, "Boden", "Wand 3");
    let f3 = d.add_feature("Fase 1", fillet(body, vec![other], BlendArt::FaseGleich { abstand: 1.0 })).unwrap();
    assert_eq!(status(&d, f3), FeatureStatus::Ok);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-feature --test blend_features`
Expected: FAIL – `no variant named Verrunden`.

- [ ] **Step 3: Modell erweitern** – in `crates/planora-feature/src/model.rs` oben `use planora_kernel::BlendKind;` ergänzen, vor `pub enum FeatureKind` einfügen:

```rust
/// Art einer Verrundung (Maße in mm, Winkel in Grad, Positionen in Prozent der Kettenlänge).
#[derive(Clone, Debug, PartialEq)]
pub enum BlendArt {
    Rundung { radius: f64 },
    /// Punkte (Position in % der Kettenlänge, Radius), Positionen streng steigend.
    RundungVariabel { punkte: Vec<(f64, f64)> },
    FaseGleich { abstand: f64 },
    /// Abstand auf Fläche 1 (Marke `EdgeRef.a`) und auf Fläche 2.
    FaseZwei { abstand1: f64, abstand2: f64 },
    /// Abstand auf Fläche 1 und Winkel der Fase zu Fläche 1.
    FaseWinkel { abstand: f64, winkel: f64 },
}

impl BlendArt {
    pub fn is_chamfer(&self) -> bool {
        matches!(
            self,
            BlendArt::FaseGleich { .. } | BlendArt::FaseZwei { .. } | BlendArt::FaseWinkel { .. }
        )
    }

    /// Bezeichnung der Feature-Art: „Rundung“ oder „Fase“.
    pub fn label(&self) -> &'static str {
        if self.is_chamfer() { "Fase" } else { "Rundung" }
    }

    /// Ausführliche Bezeichnung für Eigenschaften und Fenster.
    pub fn kind_label(&self) -> &'static str {
        match self {
            BlendArt::Rundung { .. } => "Rundung",
            BlendArt::RundungVariabel { .. } => "Rundung mit variablem Radius",
            BlendArt::FaseGleich { .. } => "Fase mit gleichem Abstand",
            BlendArt::FaseZwei { .. } => "Fase mit zwei Abständen",
            BlendArt::FaseWinkel { .. } => "Fase mit Abstand und Winkel",
        }
    }

    /// Alle Zahlen (für die Bereichsprüfung).
    pub fn numbers(&self) -> Vec<f64> {
        match self {
            BlendArt::Rundung { radius } => vec![*radius],
            BlendArt::RundungVariabel { punkte } => punkte.iter().flat_map(|(p, r)| [*p, *r]).collect(),
            BlendArt::FaseGleich { abstand } => vec![*abstand],
            BlendArt::FaseZwei { abstand1, abstand2 } => vec![*abstand1, *abstand2],
            BlendArt::FaseWinkel { abstand, winkel } => vec![*abstand, *winkel],
        }
    }

    /// Anfrage-Art des Kernels (Grad → Bogenmaß, Prozent → Anteil).
    pub fn kernel_kind(&self) -> BlendKind {
        match self {
            BlendArt::Rundung { radius } => BlendKind::Fillet { radius: *radius },
            BlendArt::RundungVariabel { punkte } => BlendKind::VariableFillet {
                points: punkte.iter().map(|(p, r)| (p / 100.0, *r)).collect(),
            },
            BlendArt::FaseGleich { abstand } => BlendKind::ChamferEqual { distance: *abstand },
            BlendArt::FaseZwei { abstand1, abstand2 } => BlendKind::ChamferTwo {
                distance1: *abstand1,
                distance2: *abstand2,
            },
            BlendArt::FaseWinkel { abstand, winkel } => BlendKind::ChamferAngle {
                distance: *abstand,
                angle: winkel.to_radians(),
            },
        }
    }
}
```

In `FeatureKind` nach `Move { … }` einfügen:

```rust
    /// Rundung oder Fase an Kanten eines Körpers; ersetzt den Körper (wie Verschieben).
    Verrunden {
        body: BodyRef,
        edges: Vec<EdgeRef>,
        art: BlendArt,
        tangentenkette: bool,
    },
```

In `label`: `FeatureKind::Verrunden { art, .. } => art.label(),`. In `dependencies`:

```rust
            FeatureKind::Verrunden { body, edges, .. } => {
                let mut d = vec![body.feature];
                for e in edges {
                    d.push(FeatureId(e.a.origin));
                    d.push(FeatureId(e.b.origin));
                }
                d
            }
```

- [ ] **Step 4: Fehler, Verweis, Export** – in `crates/planora-feature/src/error.rs` nach `ImplausibleFreeformResult` einfügen:

```rust
    #[error("Das Ergebnis der Rundung oder Fase ist unplausibel (Volumenprobe fehlgeschlagen)")]
    ImplausibleBlend,
```

in `crates/planora-feature/src/refs.rs` am Ende einfügen:

```rust
/// Die an `edge` angrenzende Fläche mit der Marke von `r` (Fläche 1 einer Fase, P4); `None`,
/// wenn keine angrenzende Fläche diese Marke trägt.
pub fn first_face(solid: &Solid, edge: Id<Edge>, r: &FaceRef) -> Option<Id<Face>> {
    let tag = r.tag();
    edge_uses(solid)
        .get(&edge)?
        .iter()
        .copied()
        .find(|f| solid.faces.get(*f).and_then(|x| x.tag.as_ref()) == Some(&tag))
}
```

und in `crates/planora-feature/src/lib.rs` `BlendArt` in `pub use model::{…}` sowie `first_face` in `pub use refs::{…}` aufnehmen.

- [ ] **Step 5: Berechnung** – in `crates/planora-feature/src/compute.rs`:

Importe ergänzen: `use crate::refs::first_face;`, `use planora_kernel::{BlendEdge, BlendError, BlendRequest, Blended};`, in `planora_mesh::{…}` zusätzlich `blend_volume_plausible, curved_area`.

Konstanten nach `VOLUME_REL_TOL`:

```rust
/// Spielraum der Volumenprobe einer Verrundung relativ zur Schätzung des Kernels (P2).
const BLEND_ESTIMATE_REL: f64 = 0.5;
/// Netzfehler je mm² gekrümmter Fläche in Vielfachen der Sehnentoleranz (jedes Dreieck weicht
/// höchstens die Sehnentoleranz von der Fläche ab; Eingang und Ergebnis zusammen).
const BLEND_MESH_SLACK: f64 = 2.0;
```

In `check_parameters` vor dem letzten Arm:

```rust
        FeatureKind::Verrunden { edges, art, .. } => {
            if edges.is_empty() {
                return Err(FeatureError::InvalidInput("Es ist keine Kante gewählt".into()));
            }
            if !art.numbers().iter().all(|v| in_range(*v)) {
                return out("Ein Maß der Rundung oder Fase");
            }
        }
```

In `compute_unchecked` nach dem Arm `Move`:

```rust
        FeatureKind::Verrunden {
            body,
            edges,
            art,
            tangentenkette,
        } => {
            let s = visible_body(doc, visible, *body)?;
            let tol = kernel.tolerance;
            let mut list = Vec::with_capacity(edges.len());
            for r in edges {
                let found = resolve_edge(&[s], r, &tol)?;
                list.push(BlendEdge {
                    edge: found.edge,
                    first_face: first_face(s, found.edge, &r.a),
                });
            }
            let request = BlendRequest {
                edges: list,
                kind: art.kernel_kind(),
                tangent_chain: *tangentenkette,
            };
            let blended = kernel.blend(s, &request).map_err(|e| blend_error(s, &e))?;
            check_blend_volume(doc, s, &blended)?;
            let mut result = blended.solid;
            result.assign_origin(id.0);
            Ok(Computed::bodies(vec![result]))
        }
```

Neue Funktionen nach `is_freeform`:

```rust
/// Meldung einer gescheiterten Verrundung (P11): bei Freiformkörpern und Ursachen aus der Bahn
/// mit dem bekannten Vorsatz.
fn blend_error(input: &Solid, e: &BlendError) -> FeatureError {
    let limit = matches!(
        e,
        BlendError::SpineLost | BlendError::Unsupported(_) | BlendError::TooExpensive
    );
    if is_freeform(input) && limit {
        FeatureError::Kernel(format!("Bei Körpern mit Freiformflächen: {e}"))
    } else {
        FeatureError::Kernel(e.to_string())
    }
}

/// Volumenprobe einer Verrundung (Spec §4.5, P2): Eingang und Ergebnis mit gemeinsamer Teilung
/// vernetzen und die Änderung mit der Schätzung des Kernels vergleichen.
fn check_blend_volume(doc: &Document, input: &Solid, blended: &Blended) -> Result<(), FeatureError> {
    let disc = Discretizer::for_solids(&[input, &blended.solid], doc.chord_tolerance);
    let mesh = |s: &Solid| tessellate_with(s, &disc).map_err(|e| FeatureError::Kernel(e.to_string()));
    let (mi, mo) = (mesh(input)?, mesh(&blended.solid)?);
    let slack = BLEND_MESH_SLACK
        * disc.chord_tolerance()
        * (curved_area(input, &mi) + curved_area(&blended.solid, &mo));
    if blend_volume_plausible(
        mass_properties(&mi).volume,
        mass_properties(&mo).volume,
        blended.volume_change,
        BLEND_ESTIMATE_REL,
        slack,
    ) {
        Ok(())
    } else {
        Err(FeatureError::ImplausibleBlend)
    }
}
```

Im Testmodul von `compute.rs` ergänzen:

```rust
    #[test]
    fn the_blend_volume_probe_rejects_a_wrong_estimate() {
        use planora_kernel::blend::topo::{edge_uses, face_name};
        let doc = Document::new();
        let s = doc.kernel.make_box(DVec3::splat(10.0)).unwrap();
        let e = s
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let n: Vec<String> = edge_uses(&s, id).iter().map(|u| face_name(&s, u.face)).collect();
                n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
            })
            .unwrap();
        let request = BlendRequest {
            edges: vec![BlendEdge { edge: e, first_face: None }],
            kind: planora_kernel::BlendKind::Fillet { radius: 2.0 },
            tangent_chain: false,
        };
        let mut b = doc.kernel.blend(&s, &request).unwrap();
        assert_eq!(check_blend_volume(&doc, &s, &b), Ok(()));
        b.volume_change = 500.0;
        assert_eq!(check_blend_volume(&doc, &s, &b), Err(FeatureError::ImplausibleBlend));
    }
```

- [ ] **Step 6: Neuberechnung** – in `crates/planora-feature/src/recompute.rs` im `consumed`-Closure den Arm `FeatureKind::Verrunden { body, .. } => r == body,` und in `reads_visible` den ersten Arm zu `FeatureKind::Boolean { .. } | FeatureKind::Move { .. } | FeatureKind::Verrunden { .. } => true,` ändern.

- [ ] **Step 7: App-Stellen** (damit der Arbeitsbereich baut):

`crates/planora-app/src/properties.rs`, in `parameter_rows` nach dem Arm `Move`:

```rust
        FeatureKind::Verrunden {
            body,
            edges,
            art,
            tangentenkette,
        } => {
            let mut rows = vec![
                ("Körper", state.body_ref_name(*body)),
                ("Kanten", edges.len().to_string()),
                ("Art", art.kind_label().to_string()),
            ];
            match art {
                BlendArt::Rundung { radius } => rows.push(("Radius", mm(*radius))),
                BlendArt::RundungVariabel { punkte } => rows.push(("Punkte", punkte.len().to_string())),
                BlendArt::FaseGleich { abstand } => rows.push(("Abstand", mm(*abstand))),
                BlendArt::FaseZwei { abstand1, abstand2 } => {
                    rows.push(("Abstand 1", mm(*abstand1)));
                    rows.push(("Abstand 2", mm(*abstand2)));
                }
                BlendArt::FaseWinkel { abstand, winkel } => {
                    rows.push(("Abstand", mm(*abstand)));
                    rows.push(("Winkel", format!("{} °", format_number(*winkel, 2))));
                }
            }
            rows.push(("Tangentenkette", if *tangentenkette { "an" } else { "aus" }.to_string()));
            rows
        }
```

(`use planora_feature::BlendArt;` ergänzen.)

`crates/planora-app/src/icons.rs`: Variante `Blend` in `FeatureIcon`, in `of` den Arm `K::Verrunden { .. } => FeatureIcon::Blend,`, in `paint_feature_icon` den Arm `FeatureIcon::Blend => paint_blend_icon(painter, rect, color),` und die Funktion:

```rust
/// Rundung bzw. Fase: Winkel mit abgerundeter Ecke.
fn paint_blend_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let corner = c + vec2(-r, r);
    let bend = r * 0.9;
    let mut pts: Vec<Pos2> = vec![c + vec2(-r, -r), corner + vec2(0.0, -bend)];
    for i in 1..8 {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
        pts.push(corner + vec2(bend * (1.0 - a.cos()), -bend * (1.0 - a.sin())));
    }
    pts.push(corner + vec2(bend, 0.0));
    pts.push(c + vec2(r, r));
    painter.add(egui::Shape::line(pts, stroke));
}
```

`crates/planora-app/src/feature_dialog.rs`: in `numbers_of` vor dem letzten Arm

```rust
        FeatureKind::Verrunden { art, .. } => match art {
            BlendArt::Rundung { radius } => vec![("Radius", *radius)],
            BlendArt::FaseGleich { abstand } => vec![("Abstand", *abstand)],
            BlendArt::FaseZwei { abstand1, abstand2 } => {
                vec![("Abstand 1", *abstand1), ("Abstand 2", *abstand2)]
            }
            BlendArt::FaseWinkel { abstand, winkel } => vec![("Abstand", *abstand), ("Winkel", *winkel)],
            BlendArt::RundungVariabel { .. } => Vec::new(),
        },
```

in `set_numbers` vor `_ => {}`

```rust
        (FeatureKind::Verrunden { art, .. }, values) => match (art, values) {
            (BlendArt::Rundung { radius }, [r]) => *radius = *r,
            (BlendArt::FaseGleich { abstand }, [a]) => *abstand = *a,
            (BlendArt::FaseZwei { abstand1, abstand2 }, [a, b]) => (*abstand1, *abstand2) = (*a, *b),
            (BlendArt::FaseWinkel { abstand, winkel }, [a, w]) => (*abstand, *winkel) = (*a, *w),
            _ => {}
        },
```

und im Zahlenraster von `show` statt `ui.label("mm");` den Aufruf `ui.label(unit(label));` mit

```rust
/// Einheit eines Zahlenfelds.
fn unit(label: &str) -> &'static str {
    if label == "Winkel" { "°" } else { "mm" }
}
```

(`use planora_feature::BlendArt;` ergänzen.) Der Bereichstext „Betrag höchstens … mm“ bleibt; Winkel über 180° lehnt der Kernel ab.

`crates/planora-app/src/browser.rs` Zeile 84–87: `FeatureKind::Boolean { .. } | FeatureKind::Move { .. } | FeatureKind::Verrunden { .. }`.

`crates/planora-app/src/state.rs` `origin_of`: Arm zu `Some(FeatureKind::Move { body, .. } | FeatureKind::Verrunden { body, .. }) => r = *body,` erweitern und die Doku um „und Verrundungen“ ergänzen (P14).

- [ ] **Step 8: Datei-Typen** – in `crates/planora-project/src/format.rs` vor `KindDto` einfügen:

```rust
/// Art einer Verrundung (Grad, Prozent und Millimeter wie im Feature).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum BlendArtDto {
    Rundung { radius: f64 },
    RundungVariabel { punkte: Vec<[f64; 2]> },
    FaseGleich { abstand: f64 },
    FaseZwei { abstand1: f64, abstand2: f64 },
    FaseWinkel { abstand: f64, winkel: f64 },
}
```

und in `KindDto` nach `Verschieben`:

```rust
    Verrunden {
        koerper: BodyRefDto,
        kanten: Vec<EdgeRefDto>,
        art: BlendArtDto,
        tangentenkette: bool,
    },
```

In `crates/planora-project/src/convert.rs` `BlendArt` importieren und ergänzen:

```rust
fn art_to(a: &BlendArt) -> BlendArtDto {
    match a {
        BlendArt::Rundung { radius } => BlendArtDto::Rundung { radius: *radius },
        BlendArt::RundungVariabel { punkte } => BlendArtDto::RundungVariabel {
            punkte: punkte.iter().map(|(p, r)| [*p, *r]).collect(),
        },
        BlendArt::FaseGleich { abstand } => BlendArtDto::FaseGleich { abstand: *abstand },
        BlendArt::FaseZwei { abstand1, abstand2 } => BlendArtDto::FaseZwei {
            abstand1: *abstand1,
            abstand2: *abstand2,
        },
        BlendArt::FaseWinkel { abstand, winkel } => BlendArtDto::FaseWinkel {
            abstand: *abstand,
            winkel: *winkel,
        },
    }
}

fn art_from(origin: &str, name: &str, d: &BlendArtDto) -> Result<BlendArt> {
    let n = |what: &str, x: f64| num(origin, &format!("„{name}“ {what}"), x);
    Ok(match d {
        BlendArtDto::Rundung { radius } => BlendArt::Rundung { radius: n("Radius", *radius)? },
        BlendArtDto::RundungVariabel { punkte } => {
            check_len(origin, "Radienpunkte", punkte.len())?;
            BlendArt::RundungVariabel {
                punkte: punkte
                    .iter()
                    .map(|[p, r]| Ok((n("Position", *p)?, n("Radius", *r)?)))
                    .collect::<Result<Vec<_>>>()?,
            }
        }
        BlendArtDto::FaseGleich { abstand } => BlendArt::FaseGleich { abstand: n("Abstand", *abstand)? },
        BlendArtDto::FaseZwei { abstand1, abstand2 } => BlendArt::FaseZwei {
            abstand1: n("Abstand 1", *abstand1)?,
            abstand2: n("Abstand 2", *abstand2)?,
        },
        BlendArtDto::FaseWinkel { abstand, winkel } => BlendArt::FaseWinkel {
            abstand: n("Abstand", *abstand)?,
            winkel: n("Winkel", *winkel)?,
        },
    })
}
```

in `kind_to` nach `Move`:

```rust
        FeatureKind::Verrunden {
            body,
            edges,
            art,
            tangentenkette,
        } => KindDto::Verrunden {
            koerper: body_to(body),
            kanten: edges.iter().map(edge_to).collect(),
            art: art_to(art),
            tangentenkette: *tangentenkette,
        },
```

und in `kind_from` nach `Verschieben`:

```rust
        KindDto::Verrunden {
            koerper,
            kanten,
            art,
            tangentenkette,
        } => {
            check_len(origin, "Kanten", kanten.len())?;
            if kanten.is_empty() {
                return Err(ProjectError::corrupt(origin, format!("„{name}“: keine Kante gewählt")));
            }
            FeatureKind::Verrunden {
                body: body_from(koerper),
                edges: kanten
                    .iter()
                    .map(|k| edge_from(origin, name, k))
                    .collect::<Result<Vec<_>>>()?,
                art: art_from(origin, name, art)?,
                tangentenkette: *tangentenkette,
            }
        }
```

- [ ] **Step 9: Rundreise-Test** (`crates/planora-project/tests/blend_roundtrip.rs`)

```rust
//! Verrundungen überstehen Schreiben und Lesen (Teilprojekt 8, Task 7; Formatnummer in Task 18).

use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_math::{DVec3, Tolerance};
use planora_project::{decode_version, encode_version};

#[test]
fn every_blend_art_survives_a_round_trip() {
    let mut d = Document::new();
    let arts = [
        BlendArt::Rundung { radius: 1.5 },
        BlendArt::RundungVariabel { punkte: vec![(0.0, 1.0), (40.0, 1.5), (100.0, 2.0)] },
        BlendArt::FaseGleich { abstand: 1.0 },
        BlendArt::FaseZwei { abstand1: 1.0, abstand2: 2.0 },
        BlendArt::FaseWinkel { abstand: 1.0, winkel: 30.0 },
    ];
    for (i, art) in arts.iter().enumerate() {
        let f = d
            .add_feature(
                format!("Quader {}", i + 1),
                FeatureKind::Box { size: DVec3::splat(10.0), origin: DVec3::new(20.0 * i as f64, 0.0, 0.0) },
            )
            .unwrap();
        let body = BodyRef { feature: f, index: 0 };
        let s = d.body(body).unwrap();
        let e = s
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
                n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
            })
            .unwrap();
        let r = EdgeRef::from_edge(s, e, &Tolerance::default()).unwrap();
        d.add_feature(
            format!("{} {}", art.label(), i + 1),
            FeatureKind::Verrunden { body, edges: vec![r], art: art.clone(), tangentenkette: i % 2 == 0 },
        )
        .unwrap();
    }
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"verrunden\"") && text.contains("\"rundung_variabel\""));
    let back = decode_version(&text, "test").unwrap();
    let e = Document::from_snapshot(back, 0.05).unwrap();
    for (a, b) in d.features().iter().zip(e.features()) {
        match (&a.kind, &b.kind) {
            (
                FeatureKind::Verrunden { art: x, tangentenkette: tx, edges: ex, .. },
                FeatureKind::Verrunden { art: y, tangentenkette: ty, edges: ey, .. },
            ) => {
                assert_eq!((x, tx, ex), (y, ty, ey));
                assert_eq!(a.status, b.status);
            }
            (FeatureKind::Box { .. }, FeatureKind::Box { .. }) => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(text, encode_version(&e.snapshot()).unwrap());
}
```

- [ ] **Step 10: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-feature && cargo test -p planora-project && cargo build -p planora-app`
Expected: PASS; die variable Rundung im Rundreise-Test hat bis Task 14 den Status `Error` (Übergang P12) – der Test vergleicht nur Gleichheit vor und nach der Rundreise. Alle alten Tests grün (auch die Golden-Dateien Format 1: Dokumente ohne Verrundung ändern ihren Text nicht).

- [ ] **Step 11: Prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

```bash
git add crates/planora-feature/src/model.rs crates/planora-feature/src/compute.rs crates/planora-feature/src/recompute.rs crates/planora-feature/src/refs.rs crates/planora-feature/src/error.rs crates/planora-feature/src/lib.rs crates/planora-feature/tests/blend_features.rs crates/planora-app/src/properties.rs crates/planora-app/src/icons.rs crates/planora-app/src/feature_dialog.rs crates/planora-app/src/browser.rs crates/planora-app/src/state.rs crates/planora-project/src/format.rs crates/planora-project/src/convert.rs crates/planora-project/tests/blend_roundtrip.rs
git commit -m "feature: Verrunden in der Zeitleiste mit Volumenprobe, Anzeige und Dateitypen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: App-Fenster „Verrunden …“ und „Fase …“ (eine Kante)

**Files:**
- Create: `crates/planora-app/src/blend_dialog.rs`
- Modify: `crates/planora-app/src/lib.rs` (`pub mod blend_dialog;`), `crates/planora-app/src/workspace.rs:21-60` (`blend_dialog`-Feld), `crates/planora-app/src/app.rs:283-313,536-558` (Menü, Anzeige), `crates/planora-app/src/state.rs` (`add_blend`)
- Test: `crates/planora-app/tests/blend_state.rs`, `crates/planora-app/tests/blend_ui.rs`

**Interfaces:**
- Consumes: `FeatureKind::Verrunden`, `BlendArt`, `planora_feature::EdgeRef::from_edge`, `AppState::{add_checked, try_close_sketch, refresh, body_index, fail}`, `parse_number`.
- Produces:
  ```rust
  impl AppState { pub fn add_blend(&mut self, body: usize, edges: &[Id<Edge>], art: BlendArt, tangentenkette: bool) -> Result<FeatureId, String>; }
  pub mod blend_dialog {
      pub enum ArtChoice { Rundung, FaseGleich, FaseZwei, FaseWinkel }
      impl ArtChoice { pub fn label(self) -> &'static str; pub fn fields(self) -> (&'static str, Option<&'static str>); }
      pub const NO_EDGE: &str;
      pub struct BlendDialog { pub art: ArtChoice, pub first: String, pub second: String, pub tangentenkette: bool, /* error */ }
      impl BlendDialog { pub fn fillet() -> Self; pub fn chamfer() -> Self; pub fn title(&self) -> &'static str;
          pub fn error(&self) -> Option<&str>; pub fn chosen_edges(&self, state: &AppState) -> Option<(usize, Vec<Id<Edge>>)>;
          pub fn art_value(&self) -> Result<BlendArt, String>; pub fn execute(&mut self, state: &mut AppState) -> bool;
          pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool; }
  }
  DocTab::blend_dialog: Option<BlendDialog>
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-app/tests/blend_state.rs`)

```rust
//! Rundungen und Fasen in der App (Teilprojekt 8, Task 8).

use planora_app::blend_dialog::{ArtChoice, BlendDialog, NO_EDGE};
use planora_app::state::*;
use planora_feature::{BlendArt, FeatureKind};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DVec3, PI};
use planora_render::{EdgeRef, Pick};

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn with_box() -> (AppState, Id<Edge>) {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    (s, e)
}

#[test]
fn add_blend_replaces_the_body_and_keeps_its_name() {
    let (mut s, e) = with_box();
    let id = s.add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true).unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.bodies[0].name, "Quader 1");
    assert_eq!(s.document.feature(id).unwrap().name, "Rundung 1");
    assert_eq!(s.selected_body, Some(0));
    let want = 1000.0 - (1.0 - PI / 4.0) * 40.0;
    // Sehnentoleranz der App (0,05): Netzfehler der Rundungsfläche etwa 1 mm³.
    assert!((s.bodies[0].mass.volume - want).abs() / want < 3e-3, "{}", s.bodies[0].mass.volume);
    assert!(matches!(s.document.feature(id).unwrap().kind, FeatureKind::Verrunden { .. }));
}

#[test]
fn a_failing_blend_leaves_the_document_unchanged() {
    let (mut s, e) = with_box();
    let n = s.document.features().len();
    let r = s.add_blend(0, &[e], BlendArt::Rundung { radius: 20.0 }, true);
    let msg = r.unwrap_err();
    assert!(msg.contains("zu groß"), "{msg}");
    assert_eq!(s.document.features().len(), n);
    assert_eq!(s.message.as_deref(), Some(msg.as_str()));
    assert!(s.add_blend(5, &[e], BlendArt::Rundung { radius: 1.0 }, true).is_err());
    assert!(s.add_blend(0, &[], BlendArt::Rundung { radius: 1.0 }, true).is_err());
}

#[test]
fn the_dialog_needs_a_clicked_edge_and_valid_numbers() {
    let (mut s, e) = with_box();
    let mut d = BlendDialog::fillet();
    assert!(d.execute(&mut s), "ohne Kante bleibt das Fenster offen");
    assert_eq!(d.error(), Some(NO_EDGE));
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    d.first = "abc".into();
    assert!(d.execute(&mut s));
    assert!(d.error().unwrap().contains("Radius"));
    d.first = "2,5".into();
    assert_eq!(d.art_value(), Ok(BlendArt::Rundung { radius: 2.5 }));
    assert!(!d.execute(&mut s), "nach Erfolg schließt das Fenster");
    assert_eq!(s.document.features().len(), 2);
}

#[test]
fn chamfer_choices_read_their_fields() {
    let mut d = BlendDialog::chamfer();
    assert_eq!(d.title(), "Fase");
    d.art = ArtChoice::FaseWinkel;
    d.first = "1".into();
    d.second = "30".into();
    assert_eq!(d.art_value(), Ok(BlendArt::FaseWinkel { abstand: 1.0, winkel: 30.0 }));
    d.second = "180".into();
    assert!(d.art_value().unwrap_err().contains("Winkel"));
    d.art = ArtChoice::FaseZwei;
    d.second = "2".into();
    assert_eq!(d.art_value(), Ok(BlendArt::FaseZwei { abstand1: 1.0, abstand2: 2.0 }));
    assert_eq!(ArtChoice::FaseZwei.fields(), ("Abstand 1", Some("Abstand 2")));
}
```

(`crates/planora-app/tests/blend_ui.rs`)

```rust
//! Fenster ohne Bildschirm (Teilprojekt 8, Task 8).

use planora_app::blend_dialog::BlendDialog;
use planora_app::state::*;
use planora_app::{fonts, properties};
use planora_feature::BlendArt;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_math::DVec3;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

#[test]
fn the_blend_dialog_draws_and_changes_nothing_without_input() {
    let ctx = context();
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    for mut d in [BlendDialog::fillet(), BlendDialog::chamfer()] {
        for _ in 0..3 {
            let mut open = false;
            ctx.run_ui(egui::RawInput::default(), |ui| open = d.show(ui.ctx(), &mut s))
                .drop_without_applying_deltas();
            assert!(open);
        }
    }
    assert_eq!(s.document.features().len(), 1);
}

#[test]
fn the_properties_show_the_blend_parameters() {
    let ctx = context();
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let solid = &s.bodies[0].solid;
    let e = solid
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(solid, id).iter().map(|u| face_name(solid, u.face)).collect();
            n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
        })
        .unwrap();
    let id = s.add_blend(0, &[e], BlendArt::FaseWinkel { abstand: 1.0, winkel: 30.0 }, false).unwrap();
    let kind = s.document.feature(id).unwrap().kind.clone();
    let rows = properties::parameter_rows(&s, &kind);
    let labels: Vec<&str> = rows.iter().map(|(l, _)| *l).collect();
    assert_eq!(labels, ["Körper", "Kanten", "Art", "Abstand", "Winkel", "Tangentenkette"]);
    s.selected_body = None;
    s.selected_feature = Some(id);
    ctx.run_ui(egui::RawInput::default(), |ui| properties::show(ui, &mut s))
        .drop_without_applying_deltas();
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test blend_state --test blend_ui`
Expected: FAIL – `could not find blend_dialog`.

- [ ] **Step 3: `add_blend` schreiben** – in `crates/planora-app/src/state.rs` (Importe: `BlendArt` aus `planora_feature`) nach `add_boolean`:

```rust
    /// Legt eine Rundung oder Fase an den Kanten `edges` des Körpers `body` an. Das Ergebnis
    /// ersetzt den Körper (Name bleibt) und wird gewählt. Schlägt sie fehl, wird das Feature
    /// wieder entfernt: Dokument und Körper bleiben unverändert, die Meldung steht in `message`.
    pub fn add_blend(
        &mut self,
        body: usize,
        edges: &[Id<Edge>],
        art: BlendArt,
        tangentenkette: bool,
    ) -> Result<FeatureId, String> {
        let Some(b) = self.bodies.get(body) else {
            return self.fail("Körper nicht gefunden".into());
        };
        if edges.is_empty() {
            return self.fail("Bitte zuerst eine Kante anklicken".into());
        }
        let tol = self.kernel.tolerance;
        let mut refs = Vec::with_capacity(edges.len());
        for &e in edges {
            match planora_feature::EdgeRef::from_edge(&b.solid, e, &tol) {
                Ok(r) => refs.push(r),
                Err(err) => return self.fail(err.to_string()),
            }
        }
        let source = b.source;
        self.try_close_sketch()?;
        let kind = FeatureKind::Verrunden {
            body: source,
            edges: refs,
            art,
            tangentenkette,
        };
        let id = self.add_checked(kind.label(), kind)?;
        self.hover = None;
        self.selection = None;
        self.refresh();
        self.selected_body = self.body_index(BodyRef { feature: id, index: 0 });
        Ok(id)
    }
```

- [ ] **Step 4: Fenster schreiben** (`crates/planora-app/src/blend_dialog.rs`)

```rust
//! Fenster „Verrunden“ bzw. „Fase“ (Menü „Bearbeiten“, Spec §6): Kante im Fenster anklicken,
//! Maße eintragen, Art wählen, „Erzeugen“. Zahlen nehmen Komma und Punkt an.

use crate::browser::ERROR_COLOR;
use crate::sketch_view::parse_number;
use crate::state::{AppState, MAX_COORD_MM};
use planora_feature::BlendArt;
use planora_kernel::{Edge, Id};
use planora_render::{EdgeRef, Pick};

/// Meldung, solange keine Kante angeklickt ist.
pub const NO_EDGE: &str = "Bitte zuerst eine Kante im Fenster anklicken.";

/// Art im Fenster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtChoice {
    Rundung,
    FaseGleich,
    FaseZwei,
    FaseWinkel,
}

impl ArtChoice {
    pub fn label(self) -> &'static str {
        match self {
            ArtChoice::Rundung => "Rundung",
            ArtChoice::FaseGleich => "Fase: gleicher Abstand",
            ArtChoice::FaseZwei => "Fase: zwei Abstände",
            ArtChoice::FaseWinkel => "Fase: Abstand und Winkel",
        }
    }

    /// Bezeichnungen der Maßfelder (das zweite fehlt bei Rundung und gleicher Fase).
    pub fn fields(self) -> (&'static str, Option<&'static str>) {
        match self {
            ArtChoice::Rundung => ("Radius", None),
            ArtChoice::FaseGleich => ("Abstand", None),
            ArtChoice::FaseZwei => ("Abstand 1", Some("Abstand 2")),
            ArtChoice::FaseWinkel => ("Abstand", Some("Winkel")),
        }
    }
}

/// Offenes Fenster „Verrunden“ oder „Fase“.
#[derive(Clone, Debug)]
pub struct BlendDialog {
    pub art: ArtChoice,
    pub first: String,
    pub second: String,
    pub tangentenkette: bool,
    error: Option<String>,
}

/// Liest ein Maß (größer als null, höchstens `MAX_COORD_MM`).
fn size(label: &str, text: &str) -> Result<f64, String> {
    match parse_number(text) {
        Some(v) if v > 0.0 && v <= MAX_COORD_MM => Ok(v),
        _ => Err(format!("„{label}“ muss eine Zahl größer als null sein (eingegeben: „{text}“)")),
    }
}

impl BlendDialog {
    pub fn fillet() -> Self {
        BlendDialog {
            art: ArtChoice::Rundung,
            first: "1".into(),
            second: String::new(),
            tangentenkette: true,
            error: None,
        }
    }

    pub fn chamfer() -> Self {
        BlendDialog {
            art: ArtChoice::FaseGleich,
            first: "1".into(),
            second: "45".into(),
            tangentenkette: true,
            error: None,
        }
    }

    pub fn title(&self) -> &'static str {
        if self.art == ArtChoice::Rundung { "Verrunden" } else { "Fase" }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Gewählte Kanten: (Körperindex, Kanten). Stufe 1: die angeklickte Kante.
    pub fn chosen_edges(&self, state: &AppState) -> Option<(usize, Vec<Id<Edge>>)> {
        match state.selection {
            Some(Pick::Edge(EdgeRef { body, edge })) => Some((body, vec![edge])),
            _ => None,
        }
    }

    /// Art mit den eingegebenen Maßen; ein Fehlertext nennt das ungültige Feld.
    pub fn art_value(&self) -> Result<BlendArt, String> {
        let (l1, l2) = self.art.fields();
        let a = size(l1, &self.first)?;
        Ok(match self.art {
            ArtChoice::Rundung => BlendArt::Rundung { radius: a },
            ArtChoice::FaseGleich => BlendArt::FaseGleich { abstand: a },
            ArtChoice::FaseZwei => BlendArt::FaseZwei {
                abstand1: a,
                abstand2: size(l2.unwrap_or("Abstand 2"), &self.second)?,
            },
            ArtChoice::FaseWinkel => match parse_number(&self.second) {
                Some(w) if w > 0.0 && w < 180.0 => BlendArt::FaseWinkel { abstand: a, winkel: w },
                _ => {
                    return Err(format!(
                        "„Winkel“ muss zwischen 0° und 180° liegen (eingegeben: „{}“)",
                        self.second
                    ));
                }
            },
        })
    }

    /// „Erzeugen“: `true`, wenn das Fenster offen bleiben soll (Fehler stehen dann im Fenster
    /// und in der Statuszeile).
    pub fn execute(&mut self, state: &mut AppState) -> bool {
        state.message = None;
        let Some((body, edges)) = self.chosen_edges(state) else {
            self.error = Some(NO_EDGE.into());
            state.message = Some(NO_EDGE.into());
            return true;
        };
        let art = match self.art_value() {
            Ok(a) => a,
            Err(e) => {
                state.message = Some(e.clone());
                self.error = Some(e);
                return true;
            }
        };
        match state.add_blend(body, &edges, art, self.tangentenkette) {
            Ok(_) => {
                self.error = None;
                false
            }
            Err(e) => {
                self.error = Some(e);
                true
            }
        }
    }

    /// Zeigt das Fenster; `true`, solange es offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let (mut run, mut cancel) = (false, false);
        egui::Window::new(self.title())
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                match self.chosen_edges(state) {
                    Some((_, edges)) => ui.label(format!("Gewählte Kanten: {}", edges.len())),
                    None => ui.weak("Kante im Fenster anklicken."),
                };
                ui.add_space(4.0);
                let choices: &[ArtChoice] = if self.art == ArtChoice::Rundung {
                    &[ArtChoice::Rundung]
                } else {
                    &[ArtChoice::FaseGleich, ArtChoice::FaseZwei, ArtChoice::FaseWinkel]
                };
                for c in choices {
                    ui.radio_value(&mut self.art, *c, c.label());
                }
                let (l1, l2) = self.art.fields();
                egui::Grid::new("blend_numbers").num_columns(3).show(ui, |ui| {
                    ui.label(l1);
                    ui.add(egui::TextEdit::singleline(&mut self.first).desired_width(90.0));
                    ui.label("mm");
                    ui.end_row();
                    if let Some(l2) = l2 {
                        ui.label(l2);
                        ui.add(egui::TextEdit::singleline(&mut self.second).desired_width(90.0));
                        ui.label(if l2 == "Winkel" { "°" } else { "mm" });
                        ui.end_row();
                    }
                });
                ui.checkbox(&mut self.tangentenkette, "Tangentenkette (glatt anschließende Kanten mitnehmen)");
                if let Some(e) = &self.error {
                    ui.colored_label(ERROR_COLOR, e);
                }
                ui.horizontal(|ui| {
                    if ui.button("Erzeugen").clicked() {
                        run = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        cancel = true;
                    }
                });
            });
        if run {
            return self.execute(state);
        }
        open && !cancel
    }
}
```

- [ ] **Step 5: Einbinden** – `crates/planora-app/src/lib.rs`: `pub mod blend_dialog;`. `crates/planora-app/src/workspace.rs`: `use crate::blend_dialog::BlendDialog;`, Feld `pub blend_dialog: Option<BlendDialog>,` nach `feature_dialog`, in `DocTab::new` `blend_dialog: None,`. `crates/planora-app/src/app.rs`: `use crate::blend_dialog::BlendDialog;`; in `edit_menu` vor `ui.separator();` (vor „Zum Ende der Zeitleiste“):

```rust
        ui.separator();
        let any = !tab.state.bodies.is_empty();
        for (label, dialog) in [("Verrunden …", BlendDialog::fillet()), ("Fase …", BlendDialog::chamfer())] {
            if ui
                .add_enabled(any, egui::Button::new(label))
                .on_disabled_hover_text("Dafür wird mindestens ein Körper benötigt.")
                .clicked()
            {
                if tab.state.try_close_sketch().is_ok() {
                    tab.blend_dialog = Some(dialog);
                }
                ui.close();
            }
        }
```

in `ui` beim Zurücksetzen im Skizzen-Modus `tab.blend_dialog = None;` neben `tab.feature_dialog = None;` und nach dem Block des Booleschen Fensters:

```rust
        if let Some(mut dialog) = tab.blend_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.blend_dialog = Some(dialog);
        }
```

- [ ] **Step 6: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test blend_state --test blend_ui`
Expected: PASS (6 Tests).

- [ ] **Step 7: Stufenabschluss prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check`
Expected: alles grün (Abschluss Stufe 1). Ergebnis mit Testzahl und Laufzeiten der neuen Testprogramme im Bericht nennen; danach Review der Stufe 1.

```bash
git add crates/planora-app/src/blend_dialog.rs crates/planora-app/src/lib.rs crates/planora-app/src/workspace.rs crates/planora-app/src/app.rs crates/planora-app/src/state.rs crates/planora-app/tests/blend_state.rs crates/planora-app/tests/blend_ui.rs
git commit -m "app: Fenster Verrunden und Fase im Menü Bearbeiten

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Stufe 2 – Ketten, Tangentenkette, Mehrfachauswahl, Ecken

### Task 9: Kantenketten, Tangentenkette und glatte Übergänge

**Files:**
- Create: `crates/planora-kernel/src/blend/chain.rs`
- Modify: `crates/planora-kernel/src/blend/mod.rs` (`pub mod chain;`, `blend` nutzt `chain::select_edges`, `select_edges`/`SEVERAL_LATER` entfallen), `crates/planora-kernel/src/blend/build.rs` (`end_cuts` nach Ecken gruppiert, glatte Übergänge)
- Modify (Übergangstest ersetzen, P12): `crates/planora-kernel/tests/blend_api.rs` (`stage_one_refuses_several_edges_and_variable_radii` → `variable_radii_wait_for_stage_three`)
- Test: `crates/planora-kernel/tests/blend_chain.rs`

**Interfaces:**
- Consumes: Task 1–5 (`edge_uses`, `vertex_edges`, `vertex_faces`, `EdgeGeom`, `EndCut`, `EndPlan`, `blend_tol`, `seam_cut`, `cap_cut`, `ON_CURVE_FACTOR`), `Seg::length`.
- Produces:
  ```rust
  pub mod chain {
      pub const CHAIN_ANGLE: f64;                                    // 0,5° im Bogenmaß
      pub struct ChainedEdge { pub edge: BlendEdge, pub chain: usize, pub offset: f64, pub length: f64,
                               pub reversed: bool, pub chain_length: f64 }
      impl ChainedEdge { pub fn fraction(&self, s: f64) -> f64; }    // Anteil der Kettenlänge, s = Abstand vom Kantenanfang
      pub fn leaving_direction(solid: &Solid, edge: Id<Edge>, v: Id<Vertex>) -> DVec3;
      pub fn smooth_neighbour(solid: &Solid, edge: Id<Edge>, v: Id<Vertex>) -> Option<Id<Edge>>;
      pub fn tangent_chain(solid: &Solid, edge: Id<Edge>) -> Vec<Id<Edge>>;
      pub fn select_edges(solid: &Solid, r: &BlendRequest) -> Result<Vec<ChainedEdge>, BlendError>;
  }
  // build: end_cuts behält seine Signatur; neu (pub(crate)) section_cut(g, at_end) -> EndCut
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-kernel/tests/blend_chain.rs`)

```rust
//! Kantenketten, Tangentenkette, glatte Übergänge (Teilprojekt 8, Task 9).

use planora_kernel::blend::chain::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

/// Stadion: zwei Geraden (Länge 10) und zwei Halbkreise (r = 2), alles tangential.
fn stadium(h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(10.0, 0.0) },
                Segment::Arc { center: DVec2::new(10.0, 2.0), sweep: PI },
                Segment::Line { to: DVec2::new(0.0, 4.0) },
                Segment::Arc { center: DVec2::new(0.0, 2.0), sweep: PI },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

fn req(edges: &[Id<Edge>], kind: BlendKind, chain: bool) -> BlendRequest {
    BlendRequest {
        edges: edges.iter().map(|&edge| BlendEdge { edge, first_face: None }).collect(),
        kind,
        tangent_chain: chain,
    }
}

#[test]
fn the_top_loop_of_a_stadium_is_one_smooth_chain() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let chain = tangent_chain(&s, e);
    assert_eq!(chain.len(), 4, "{chain:?}");
    assert!(chain.contains(&e));
    // Auf einem Quader schließt keine Kante glatt an.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&b, "Deckel", "Wand 1");
    assert_eq!(tangent_chain(&b, e), vec![e]);
}

#[test]
fn selected_edges_are_grouped_and_measured() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let picked = select_edges(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, true)).unwrap();
    assert_eq!(picked.len(), 4);
    assert!(picked.iter().all(|c| c.chain == 0));
    let total = 20.0 + 4.0 * PI;
    assert!((picked[0].chain_length - total).abs() < 1e-2, "{}", picked[0].chain_length);
    let sum: f64 = picked.iter().map(|c| c.length).sum();
    assert!((sum - picked[0].chain_length).abs() < 1e-9);
    // Anteile wachsen längs der Kette von 0 bis 1.
    let mut ends: Vec<(f64, f64)> = picked.iter().map(|c| (c.fraction(0.0), c.fraction(c.length))).collect();
    ends.iter_mut().for_each(|(a, b)| if a > b { std::mem::swap(a, b) });
    ends.sort_by(|x, y| x.0.total_cmp(&y.0));
    assert!(ends[0].0.abs() < 1e-12 && (ends[3].1 - 1.0).abs() < 1e-12);
    // Ohne Tangentenkette nur die gewählte Kante; zwei getrennte Kanten sind zwei Ketten.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Boden", "Wand 3");
    let picked = select_edges(&b, &req(&[e1, e2, e1], BlendKind::Fillet { radius: 0.5 }, false)).unwrap();
    assert_eq!(picked.len(), 2);
    assert_ne!(picked[0].chain, picked[1].chain);
}

#[test]
fn a_tangent_chain_fillet_runs_around_the_stadium() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, true)).unwrap();
    validate(&b.solid).unwrap();
    assert_eq!(b.edges.len(), 4);
    let roles: Vec<String> = b.solid.faces.iter().map(|(id, _)| face_name(&b.solid, id)).collect();
    for n in 1..=4 {
        assert!(roles.contains(&format!("Rundung {n}")), "{roles:?}");
    }
    // Steiner: Querschnitt mal Weg des Schwerpunkts (Umfang minus 2π·Abstand).
    let area = (1.0 - PI / 4.0) * 0.25;
    let offset = 0.5 * (5.0 / 6.0 - PI / 4.0) / (1.0 - PI / 4.0);
    let want = -area * (20.0 + 4.0 * PI - 2.0 * PI * offset);
    assert!((b.volume_change - want).abs() < 2e-3 * want.abs(), "{} statt {want}", b.volume_change);
}

#[test]
fn without_the_tangent_chain_the_end_at_a_curved_wall_is_named() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    match k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, false)) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("nicht abschließen"), "{m}"),
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn independent_edges_in_one_feature_add_up() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Boden", "Wand 3");
    let r = k().blend(&b, &req(&[e1, e2], BlendKind::Fillet { radius: 1.0 }, false)).unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), 8);
    let want = -2.0 * (1.0 - PI / 4.0) * 10.0;
    assert!((r.volume_change - want).abs() < 1e-3 * want.abs());
}

#[test]
fn two_edges_at_a_corner_are_refused_until_corners_exist() {
    // Übergang (P12): Task 11 ersetzt diesen Test durch Gehrung und Kugelecke.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Deckel", "Wand 2");
    assert!(matches!(
        k().blend(&b, &req(&[e1, e2], BlendKind::Fillet { radius: 1.0 }, false)),
        Err(BlendError::Corner(_))
    ));
}
```

In `crates/planora-kernel/tests/blend_api.rs` den Test `stage_one_refuses_several_edges_and_variable_radii` ersetzen durch:

```rust
#[test]
fn variable_radii_wait_for_stage_three() {
    // Übergang (P12): Task 14 ersetzt diesen Test.
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&s, "Deckel", "Wand 1");
    assert!(matches!(
        k().blend(&s, &req(&[e1], BlendKind::VariableFillet { points: vec![(0.0, 1.0), (1.0, 2.0)] })),
        Err(BlendError::Unsupported(m)) if m.contains("variable")
    ));
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_chain`
Expected: FAIL – `could not find chain in blend`.

- [ ] **Step 3: Ketten schreiben** (`crates/planora-kernel/src/blend/chain.rs`)

```rust
//! Kantenketten (Spec §4.2, R8): glatt anschließende Kanten, Tangentenkette und Gruppierung der
//! gewählten Kanten in Ketten mit Längen (für den variablen Radius, Task 14).

use super::error::BlendError;
use super::topo::{edge_uses, vertex_edges};
use super::{BlendEdge, BlendRequest, MAX_BLEND_EDGES};
use crate::boolean::Seg;
use crate::ids::Id;
use crate::topology::{Edge, Solid, Vertex};
use planora_math::{DVec3, PI};

/// Größter Winkel zwischen den Richtungen zweier Kanten an ihrer gemeinsamen Ecke (0,5°), bei
/// dem sie als glatt anschließend gelten. Tangentiale Übergänge aus Skizzen sind auf 1e-9 genau.
pub const CHAIN_ANGLE: f64 = 0.5 * PI / 180.0;

/// Gewählte Kante mit ihrer Lage in einer Kette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainedEdge {
    pub edge: BlendEdge,
    /// Nummer der Kette (aufeinanderfolgende, glatt verbundene gewählte Kanten).
    pub chain: usize,
    /// Abstand des Kantenanfangs in Kettenrichtung vom Kettenanfang (mm).
    pub offset: f64,
    /// Länge der Kante (mm).
    pub length: f64,
    /// Läuft die Kante gegen die Kettenrichtung?
    pub reversed: bool,
    /// Länge der ganzen Kette (mm).
    pub chain_length: f64,
}

impl ChainedEdge {
    /// Anteil der Kettenlänge am Kantenpunkt im Abstand `s` vom Kantenanfang (Kantenrichtung).
    pub fn fraction(&self, s: f64) -> f64 {
        let along = if self.reversed { self.length - s } else { s };
        if self.chain_length > 0.0 {
            ((self.offset + along) / self.chain_length).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Richtung, in der `edge` die Ecke `v` verlässt (Einheitsvektor).
pub fn leaving_direction(solid: &Solid, edge: Id<Edge>, v: Id<Vertex>) -> DVec3 {
    let e = &solid.edges[edge];
    let sign = (e.t1 - e.t0).signum();
    if e.start == v {
        (e.curve.tangent(e.t0) * sign).normalize_or_zero()
    } else {
        (-e.curve.tangent(e.t1) * sign).normalize_or_zero()
    }
}

fn is_seam(solid: &Solid, edge: Id<Edge>) -> bool {
    let u = edge_uses(solid, edge);
    u.len() == 2 && u[0].face == u[1].face
}

/// Die Kante, die an `v` glatt an `edge` anschließt, wenn es genau eine gibt (Nähte zählen nicht).
pub fn smooth_neighbour(solid: &Solid, edge: Id<Edge>, v: Id<Vertex>) -> Option<Id<Edge>> {
    let e = &solid.edges[edge];
    if e.start == e.end {
        return None;
    }
    let d = leaving_direction(solid, edge, v);
    let found: Vec<Id<Edge>> = vertex_edges(solid, v)
        .into_iter()
        .filter(|&o| o != edge && !is_seam(solid, o) && solid.edges[o].start != solid.edges[o].end)
        .filter(|&o| leaving_direction(solid, o, v).dot(d) <= -CHAIN_ANGLE.cos())
        .collect();
    (found.len() == 1).then(|| found[0])
}

/// Die andere Ecke der Kante.
fn other_end(solid: &Solid, edge: Id<Edge>, v: Id<Vertex>) -> Id<Vertex> {
    let e = &solid.edges[edge];
    if e.start == v { e.end } else { e.start }
}

/// Tangentenkette ab `edge`: die Kante und alle über glatte Ecken anschließenden, in Laufrichtung
/// geordnet (bei geschlossenen Ketten beginnend bei `edge`).
pub fn tangent_chain(solid: &Solid, edge: Id<Edge>) -> Vec<Id<Edge>> {
    let mut forward = vec![edge];
    let (mut cur, mut v) = (edge, solid.edges[edge].end);
    while let Some(next) = smooth_neighbour(solid, cur, v) {
        if forward.contains(&next) {
            break;
        }
        forward.push(next);
        v = other_end(solid, next, v);
        cur = next;
    }
    let mut back: Vec<Id<Edge>> = Vec::new();
    let (mut cur, mut v) = (edge, solid.edges[edge].start);
    while let Some(prev) = smooth_neighbour(solid, cur, v) {
        if forward.contains(&prev) || back.contains(&prev) {
            break;
        }
        back.push(prev);
        v = other_end(solid, prev, v);
        cur = prev;
    }
    back.reverse();
    back.extend(forward);
    back
}

fn edge_length(solid: &Solid, edge: Id<Edge>) -> f64 {
    let e = &solid.edges[edge];
    Seg::new(e.curve.clone(), e.t0, e.t1).length()
}

/// Läuft die Kante an Stelle `pos` des Laufs gegen die Kettenrichtung?
fn run_reversed(solid: &Solid, run: &[Id<Edge>], pos: usize) -> bool {
    let e = &solid.edges[run[pos]];
    let touches = |other: Id<Edge>, v: Id<Vertex>| {
        let o = &solid.edges[other];
        o.start == v || o.end == v
    };
    if pos + 1 < run.len() {
        !touches(run[pos + 1], e.end)
    } else if pos > 0 {
        !touches(run[pos - 1], e.start)
    } else {
        false
    }
}

/// Gewählte Kanten ohne Doppelte, bei eingeschalteter Tangentenkette mit ihren glatt
/// anschließenden Kanten (P9), gruppiert in Ketten. Fläche 1 einer gewählten Kante gilt für ihre
/// Kettenmitglieder, wenn sie an diese angrenzt.
pub fn select_edges(solid: &Solid, r: &BlendRequest) -> Result<Vec<ChainedEdge>, BlendError> {
    let mut picked: Vec<BlendEdge> = Vec::new();
    for be in &r.edges {
        if solid.edges.get(be.edge).is_none() {
            return Err(BlendError::EdgeNotFound);
        }
        let members = if r.tangent_chain { tangent_chain(solid, be.edge) } else { vec![be.edge] };
        for e in members {
            if picked.iter().any(|p| p.edge == e) {
                continue;
            }
            let first_face = be
                .first_face
                .filter(|f| edge_uses(solid, e).iter().any(|u| u.face == *f));
            picked.push(BlendEdge { edge: e, first_face });
        }
    }
    if picked.len() > MAX_BLEND_EDGES {
        return Err(BlendError::InvalidParameter(format!(
            "Zu viele Kanten (höchstens {MAX_BLEND_EDGES})"
        )));
    }
    let mut out: Vec<Option<ChainedEdge>> = vec![None; picked.len()];
    let mut chain = 0;
    for i in 0..picked.len() {
        if out[i].is_some() {
            continue;
        }
        // Läufe aufeinanderfolgender gewählter Kanten in der geometrischen Kette.
        let mut runs: Vec<Vec<Id<Edge>>> = vec![Vec::new()];
        for e in tangent_chain(solid, picked[i].edge) {
            let chosen = picked.iter().position(|p| p.edge == e).filter(|&j| out[j].is_none());
            match (chosen, runs.last_mut()) {
                (Some(_), Some(run)) => run.push(e),
                _ => runs.push(Vec::new()),
            }
        }
        for run in runs.into_iter().filter(|r| !r.is_empty()) {
            let lengths: Vec<f64> = run.iter().map(|&e| edge_length(solid, e)).collect();
            let total: f64 = lengths.iter().sum();
            let mut offset = 0.0;
            for (pos, &e) in run.iter().enumerate() {
                let j = picked.iter().position(|p| p.edge == e).unwrap_or(i);
                out[j] = Some(ChainedEdge {
                    edge: picked[j],
                    chain,
                    offset,
                    length: lengths[pos],
                    reversed: run_reversed(solid, &run, pos),
                    chain_length: total,
                });
                offset += lengths[pos];
            }
            chain += 1;
        }
    }
    Ok(out.into_iter().flatten().collect())
}
```

- [ ] **Step 4: Abschlüsse je Ecke** – in `crates/planora-kernel/src/blend/build.rs` `use super::chain::{CHAIN_ANGLE, leaving_direction};` und `use std::collections::BTreeMap;` ergänzen und `end_cuts` ersetzen durch:

```rust
/// Querschnitt als Abschluss am Anfang bzw. Ende (glatter Übergang, Task 9).
pub(crate) fn section_cut(g: &EdgeGeom, at_end: bool) -> EndCut {
    let s = if at_end { g.range.1 } else { g.range.0 };
    EndCut {
        sa: s,
        sb: s,
        curve: if at_end { g.section_end.clone() } else { g.section_start.clone() },
        cap: None,
    }
}

/// Fallen zwei Abschlusskurven zusammen (Enden in beliebiger Reihenfolge, Mitte)?
pub(crate) fn same_cut(x: &Seg, y: &Seg, weld: f64) -> bool {
    let (a, b, c, d) = (x.start(), x.end(), y.start(), y.end());
    ((a.distance(c) <= weld && b.distance(d) <= weld) || (a.distance(d) <= weld && b.distance(c) <= weld))
        && x.at(0.5).distance(y.at(0.5)) <= weld
}

/// Glatter Übergang an `v` zwischen den Enden `(i, ei)` und `(j, ej)`: beide Rundungsflächen
/// enden an ihrem Querschnitt, der derselbe sein muss. `None`, wenn die Kanten dort nicht glatt
/// anschließen.
fn smooth_pair(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    (i, ei): (usize, bool),
    (j, ej): (usize, bool),
    tol: &Tolerance,
) -> Result<Option<(EndCut, EndCut)>, BlendError> {
    let (gi, gj) = (&geoms[i], &geoms[j]);
    let (di, dj) = (leaving_direction(solid, gi.edge, v), leaving_direction(solid, gj.edge, v));
    if di.dot(dj) > -CHAIN_ANGLE.cos() {
        return Ok(None);
    }
    let faces = vertex_faces(solid, v);
    if faces.len() != 3 {
        return Err(BlendError::Corner(format!(
            "An der Ecke treffen {} Flächen zusammen; unterstützt sind drei",
            faces.len()
        )));
    }
    if gi.convex != gj.convex {
        return Err(BlendError::Unsupported(
            "Am glatten Übergang wechselt die Kante zwischen konvex und konkav".into(),
        ));
    }
    let (ci, cj) = (section_cut(gi, ei), section_cut(gj, ej));
    let weld = ON_CURVE_FACTOR * blend_tol(solid, geoms, tol).linear;
    if !same_cut(&ci.curve, &cj.curve, weld) {
        return Err(BlendError::Unsupported(
            "Die Rundung lässt sich am glatten Übergang nicht fortsetzen".into(),
        ));
    }
    Ok(Some((ci, cj)))
}

/// Zwei gerundete Kanten an einer Ecke: glatter Übergang (Ecken folgen in Task 11).
fn vertex_pair(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    x: (usize, bool),
    y: (usize, bool),
    tol: &Tolerance,
) -> Result<Vec<(usize, bool, EndCut)>, BlendError> {
    if let Some((cx, cy)) = smooth_pair(solid, geoms, v, x, y, tol)? {
        return Ok(vec![(x.0, x.1, cx), (y.0, y.1, cy)]);
    }
    Err(BlendError::Corner(
        "Zwei gerundete Kanten treffen sich an einer Ecke; das wird noch nicht unterstützt".into(),
    ))
}

/// Drei und mehr gerundete Kanten an einer Ecke (Task 11).
fn corner_many(
    _solid: &Solid,
    _geoms: &[EdgeGeom],
    _v: Id<Vertex>,
    list: &[(usize, bool)],
    _plan: &mut EndPlan,
    _tol: &Tolerance,
) -> Result<Vec<(usize, bool, EndCut)>, BlendError> {
    Err(BlendError::Corner(format!(
        "An der Ecke treffen {} gerundete Kanten zusammen; das wird noch nicht unterstützt",
        list.len()
    )))
}

/// Abschlüsse aller Kanten, je Ecke entschieden: geschlossene Kanten an ihrer Naht, eine
/// gerundete Kante an der Ecke → dritte Fläche, zwei → glatter Übergang (oder Ecke), drei → Ecke.
pub fn end_cuts(solid: &Solid, geoms: &[EdgeGeom], tol: &Tolerance) -> Result<EndPlan, BlendError> {
    let n = geoms.len();
    let mut starts: Vec<Option<EndCut>> = vec![None; n];
    let mut ends: Vec<Option<EndCut>> = vec![None; n];
    let mut at: BTreeMap<Id<Vertex>, Vec<(usize, bool)>> = BTreeMap::new();
    for (i, g) in geoms.iter().enumerate() {
        if g.closed {
            starts[i] = Some(seam_cut(g, false));
            ends[i] = Some(seam_cut(g, true));
            continue;
        }
        let e = &solid.edges[g.edge];
        at.entry(e.start).or_default().push((i, false));
        at.entry(e.end).or_default().push((i, true));
    }
    let mut plan = EndPlan::default();
    for (v, list) in at {
        let cuts = match list.as_slice() {
            [(i, end)] => vec![(*i, *end, cap_cut(solid, &geoms[*i], *end, tol)?)],
            [x, y] => vertex_pair(solid, geoms, v, *x, *y, tol)?,
            _ => corner_many(solid, geoms, v, &list, &mut plan, tol)?,
        };
        for (i, end, cut) in cuts {
            if end {
                ends[i] = Some(cut);
            } else {
                starts[i] = Some(cut);
            }
        }
    }
    let missing = || BlendError::ResultInvalid("Ein Abschluss der Rundung fehlt".into());
    plan.starts = starts.into_iter().map(|c| c.ok_or_else(missing)).collect::<Result<_, _>>()?;
    plan.ends = ends.into_iter().map(|c| c.ok_or_else(missing)).collect::<Result<_, _>>()?;
    Ok(plan)
}
```

- [ ] **Step 5: Einstieg anpassen** – in `crates/planora-kernel/src/blend/mod.rs`: `pub mod chain;` ergänzen, `select_edges` und `SEVERAL_LATER` löschen, `use chain::select_edges;` ergänzen, `edge_geom` behält seine Signatur, und `blend` ersetzen durch:

```rust
pub fn blend(solid: &Solid, request: &BlendRequest, tol: &Tolerance) -> Result<Blended, BlendError> {
    check_request(request, tol)?;
    validate_with(solid, tol).map_err(|e| BlendError::InvalidInput(e.to_string()))?;
    let kind = section_kind(&request.kind)?;
    let chained = select_edges(solid, request)?;
    let geoms = chained
        .iter()
        .map(|ce| edge_geom(solid, &ce.edge, kind, tol))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = end_cuts(solid, &geoms, tol)?;
    check_contacts(solid, &geoms, &plan, tol)?;
    let volume_change = geoms
        .iter()
        .enumerate()
        .map(|(i, g)| edge_volume(g, &plan.starts[i], &plan.ends[i]))
        .sum::<f64>()
        + plan.corner_volume;
    let result = assemble(solid, &geoms, &plan, &roles(&request.kind, geoms.len()), tol)?;
    Ok(Blended {
        solid: result,
        volume_change,
        edges: chained.iter().map(|c| c.edge.edge).collect(),
    })
}
```

- [ ] **Step 6: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_chain --test blend_api --test blend_build`
Expected: PASS. Im Stadion entsteht je Kante eine Rundungsfläche (P3): zwei Zylinder, zwei Torusstücke; die Querschnitte an den glatten Ecken sind gemeinsame Kanten.

- [ ] **Step 7: Prüfen und committen**

Run: `cargo fmt && cargo clippy -p planora-kernel --all-targets -- -D warnings && cargo test -p planora-kernel`

```bash
git add crates/planora-kernel/src/blend/chain.rs crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/src/blend/build.rs crates/planora-kernel/tests/blend_chain.rs crates/planora-kernel/tests/blend_api.rs
git commit -m "kernel: Kantenketten, Tangentenkette und glatte Übergänge der Rundung

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Mehrfachauswahl von Kanten in App und Szene

**Files:**
- Modify: `crates/planora-render/src/scene.rs:127-209` (`build_scene_marked`), `crates/planora-render/src/lib.rs` (Export)
- Modify: `crates/planora-app/src/state.rs` (Felder `marked_edges`, `edge_picking`, `chain_marking`; `toggle_edge_mark`, `clear_edge_marks`; `rebuild_scene`, `refresh_following`), `crates/planora-app/src/viewport.rs:99-116`, `crates/planora-app/src/blend_dialog.rs` (Kanten aus der Markierung), `crates/planora-app/src/app.rs` (Markierung beim Schließen löschen)
- Test: `crates/planora-render/tests/scene_marks.rs`, `crates/planora-app/tests/blend_marks.rs`

**Interfaces:**
- Consumes: `planora_kernel::blend::chain::tangent_chain` (Task 9), `Pick`, `EdgeRef` (Render), `AppState::add_blend` (Task 8).
- Produces:
  ```rust
  // planora_render
  pub fn build_scene_marked(meshes: &[&Mesh], highlight: &Highlight, marked: &[Pick], palette: &Palette, grid_half_extent: f32) -> SceneData;
  // planora_app::state::AppState
  pub marked_edges: Vec<EdgeRef>; pub edge_picking: bool; pub chain_marking: bool;
  pub fn toggle_edge_mark(&mut self, e: EdgeRef, extend: bool);
  pub fn clear_edge_marks(&mut self);
  // planora_app::blend_dialog::BlendDialog
  pub fn finish(&self, state: &mut AppState);
  ```
  Regeln: Klick ohne Umschalt ersetzt die Markierung (gleiche Gruppe noch einmal: leeren); mit Umschalt kommt die Gruppe dazu oder wird, wenn schon ganz markiert, entfernt; Markierungen aus einem anderen Körper werden ersetzt. Gruppe = Tangentenkette, wenn `chain_marking`, sonst die Kante.

- [ ] **Step 1: Szene-Test schreiben** (`crates/planora-render/tests/scene_marks.rs`)

```rust
//! Hervorhebung mehrerer Kanten (Teilprojekt 8, Task 10).

use planora_kernel::*;
use planora_math::DVec3;
use planora_mesh::*;
use planora_render::*;

fn box_mesh() -> Mesh {
    let solid = AnalyticKernel::default().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    tessellate(&solid, &TessellationOptions::default()).unwrap()
}

#[test]
fn marked_edges_get_the_selected_colour() {
    let mesh = box_mesh();
    let palette = Palette::dark();
    let (e0, e1) = (mesh.edge_lines[0].edge, mesh.edge_lines[1].edge);
    let marked = [Pick::Edge(EdgeRef { body: 0, edge: e0 }), Pick::Edge(EdgeRef { body: 0, edge: e1 })];
    let scene = build_scene_marked(&[&mesh], &Highlight::default(), &marked, &palette, 0.0);
    let first = |i: usize| -> usize { mesh.edge_lines[..i].iter().map(|l| (l.points.len() - 1) * 2).sum() };
    assert_eq!(scene.line_vertices[first(0)].color, palette.edge_selected);
    assert_eq!(scene.line_vertices[first(1)].color, palette.edge_selected);
    assert_eq!(scene.line_vertices[first(2)].color, palette.edge_base);
    // Ohne Markierung gleich wie bisher.
    let plain = build_scene(&[&mesh], &Highlight::default(), &palette, 0.0);
    let unmarked = build_scene_marked(&[&mesh], &Highlight::default(), &[], &palette, 0.0);
    let colours = |s: &SceneData| s.line_vertices.iter().map(|v| v.color).collect::<Vec<_>>();
    assert_eq!(colours(&plain), colours(&unmarked));
}
```

- [ ] **Step 2: App-Test schreiben** (`crates/planora-app/tests/blend_marks.rs`)

```rust
//! Mehrfachauswahl von Kanten für Rundungen (Teilprojekt 8, Task 10).

use planora_app::blend_dialog::BlendDialog;
use planora_app::state::*;
use planora_feature::FeatureKind;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Kernel, Profile, ProfileLoop, Segment, Solid};
use planora_math::{DVec2, DVec3, PI};
use planora_render::EdgeRef;

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

#[test]
fn clicks_mark_toggle_and_extend() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.edge_picking = true;
    let solid = s.bodies[0].solid.clone();
    let e1 = EdgeRef { body: 0, edge: edge_between(&solid, "Deckel", "Wand 1") };
    let e2 = EdgeRef { body: 0, edge: edge_between(&solid, "Boden", "Wand 3") };
    let v = s.scene_version;
    s.toggle_edge_mark(e1, false);
    assert_eq!(s.marked_edges, vec![e1]);
    assert!(s.scene_version > v, "Szene neu aufgebaut");
    s.toggle_edge_mark(e2, true);
    assert_eq!(s.marked_edges, vec![e1, e2]);
    s.toggle_edge_mark(e1, true);
    assert_eq!(s.marked_edges, vec![e2]);
    s.toggle_edge_mark(e2, false);
    assert!(s.marked_edges.is_empty(), "derselbe Klick ohne Umschalt leert");
    s.toggle_edge_mark(e1, false);
    s.clear_edge_marks();
    assert!(s.marked_edges.is_empty());
}

#[test]
fn the_tangent_chain_marks_the_whole_loop() {
    let mut s = AppState::new();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(10.0, 0.0) },
                Segment::Arc { center: DVec2::new(10.0, 2.0), sweep: PI },
                Segment::Line { to: DVec2::new(0.0, 4.0) },
                Segment::Arc { center: DVec2::new(0.0, 2.0), sweep: PI },
            ],
        },
        holes: vec![],
    };
    // Der Körper kommt über eine Skizze nicht einfacher zustande; hier direkt als Extrusion des
    // Kernels geprüft, die Markierung arbeitet nur auf `bodies`.
    s.create_box(DVec3::splat(1.0)).unwrap();
    let stadium = s.kernel.extrude(&profile, &planora_math::Frame::WORLD, 3.0).unwrap();
    s.bodies[0].solid = stadium;
    s.chain_marking = true;
    let e = EdgeRef { body: 0, edge: edge_between(&s.bodies[0].solid, "Deckel", "Wand 1") };
    s.toggle_edge_mark(e, false);
    assert_eq!(s.marked_edges.len(), 4);
}

#[test]
fn the_dialog_blends_all_marked_edges_and_clears_them() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let solid = s.bodies[0].solid.clone();
    let mut d = BlendDialog::fillet();
    d.tangentenkette = false;
    s.edge_picking = true;
    s.toggle_edge_mark(EdgeRef { body: 0, edge: edge_between(&solid, "Deckel", "Wand 1") }, false);
    s.toggle_edge_mark(EdgeRef { body: 0, edge: edge_between(&solid, "Boden", "Wand 3") }, true);
    assert_eq!(d.chosen_edges(&s).map(|(_, e)| e.len()), Some(2));
    assert!(!d.execute(&mut s));
    d.finish(&mut s);
    assert!(s.marked_edges.is_empty() && !s.edge_picking);
    let f = s.document.features().last().unwrap();
    match &f.kind {
        FeatureKind::Verrunden { edges, .. } => assert_eq!(edges.len(), 2),
        k => panic!("{k:?}"),
    }
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-render --test scene_marks && cargo test -p planora-app --test blend_marks`
Expected: FAIL – `cannot find function build_scene_marked`.

- [ ] **Step 4: Szene** – in `crates/planora-render/src/scene.rs` die bisherige `build_scene` umbenennen in `build_scene_marked` mit zusätzlichem Parameter `marked: &[Pick]` (nach `highlight`), die beiden Farbwahlen auf

```rust
            let color = if highlight.selected == Some(this) || marked.contains(&this) {
                palette.face_selected
            } else if highlight.hover == Some(this) {
                palette.face_hover
            } else {
                continue;
            };
```

bzw.

```rust
            let color = if highlight.selected == Some(this) || marked.contains(&this) {
                palette.edge_selected
            } else if highlight.hover == Some(this) {
                palette.edge_hover
            } else {
                palette.edge_base
            };
```

ändern und davor neu einfügen:

```rust
/// Baut die Szene aus allen Körpern. `grid_half_extent == 0.0` lässt Raster und Achsen weg.
pub fn build_scene(
    meshes: &[&Mesh],
    highlight: &Highlight,
    palette: &Palette,
    grid_half_extent: f32,
) -> SceneData {
    build_scene_marked(meshes, highlight, &[], palette, grid_half_extent)
}
```

Doku von `build_scene_marked`: „Wie `build_scene`; `marked` (Mehrfachauswahl) bekommen die Farbe der Auswahl.“ In `crates/planora-render/src/lib.rs` `build_scene_marked` mit exportieren.

- [ ] **Step 5: Zustand** – in `crates/planora-app/src/state.rs`:

Felder in `AppState` (nach `edit_request`) mit Initialwerten `Vec::new()`, `false`, `true` in `AppState::new`:

```rust
    /// Markierte Kanten (Mehrfachauswahl im Fenster „Verrunden“/„Fase“), alle aus einem Körper.
    pub marked_edges: Vec<EdgeRef>,
    /// Klicks auf Kanten markieren statt zu wählen (solange das Fenster offen ist).
    pub edge_picking: bool,
    /// Ein Klick markiert die ganze Tangentenkette.
    pub chain_marking: bool,
```

Methoden (nach `set_selection`):

```rust
    /// Markiert die Kante `e` (mit `chain_marking` ihre Tangentenkette). Ohne `extend` ersetzt
    /// die Gruppe die Markierung (dieselbe Gruppe noch einmal: leeren); mit `extend` kommt sie
    /// dazu oder wird entfernt, wenn sie schon ganz markiert ist. Kanten eines anderen Körpers
    /// ersetzen die Markierung.
    pub fn toggle_edge_mark(&mut self, e: EdgeRef, extend: bool) {
        let Some(b) = self.bodies.get(e.body) else {
            return;
        };
        let group: Vec<EdgeRef> = if self.chain_marking {
            planora_kernel::blend::chain::tangent_chain(&b.solid, e.edge)
                .into_iter()
                .map(|edge| EdgeRef { body: e.body, edge })
                .collect()
        } else {
            vec![e]
        };
        let all_marked = group.iter().all(|g| self.marked_edges.contains(g));
        if !extend || self.marked_edges.iter().any(|m| m.body != e.body) {
            self.marked_edges = if all_marked && self.marked_edges.len() == group.len() {
                Vec::new()
            } else {
                group
            };
        } else if all_marked {
            self.marked_edges.retain(|m| !group.contains(m));
        } else {
            for g in group {
                if !self.marked_edges.contains(&g) {
                    self.marked_edges.push(g);
                }
            }
        }
        self.selected_body = Some(e.body);
        self.rebuild_scene();
    }

    /// Löscht die Markierung.
    pub fn clear_edge_marks(&mut self) {
        if !self.marked_edges.is_empty() {
            self.marked_edges.clear();
            self.rebuild_scene();
        }
    }
```

In `rebuild_scene` statt `build_scene(&meshes, &highlight, …)`:

```rust
        let marked: Vec<Pick> = self.marked_edges.iter().map(|e| Pick::Edge(*e)).collect();
        self.scene = Arc::new(build_scene_marked(
            &meshes,
            &highlight,
            &marked,
            &self.scene_palette(),
            GRID_HALF_EXTENT,
        ));
```

(Import `build_scene_marked` statt `build_scene`.) In `refresh_following` nach dem Abbilden von `hover`:

```rust
        let marks: Vec<EdgeRef> = self
            .marked_edges
            .iter()
            .filter_map(|m| match self.remap_pick(Pick::Edge(*m), map) {
                Some(Pick::Edge(x)) => Some(x),
                _ => None,
            })
            .collect();
        self.marked_edges = marks;
```

- [ ] **Step 6: Ansicht** – in `crates/planora-app/src/viewport.rs` (Zeile 107–109) den Klick so behandeln:

```rust
                if response.clicked() {
                    match hit {
                        Some(Pick::Edge(e)) if state.edge_picking => {
                            let extend = ui.input(|i| i.modifiers.shift);
                            state.toggle_edge_mark(e, extend);
                        }
                        _ => state.set_selection(hit),
                    }
                }
```

(`use planora_render::Pick;` ergänzen, falls nicht vorhanden.)

- [ ] **Step 7: Fenster** – in `crates/planora-app/src/blend_dialog.rs`:

Feld `seeded: bool` (Initialwert `false` in `fillet` und `chamfer`). `chosen_edges` ersetzen:

```rust
    /// Gewählte Kanten: die markierten (alle aus einem Körper), sonst die angeklickte Kante.
    pub fn chosen_edges(&self, state: &AppState) -> Option<(usize, Vec<Id<Edge>>)> {
        if let Some(first) = state.marked_edges.first() {
            let edges = state
                .marked_edges
                .iter()
                .filter(|m| m.body == first.body)
                .map(|m| m.edge)
                .collect();
            return Some((first.body, edges));
        }
        match state.selection {
            Some(Pick::Edge(EdgeRef { body, edge })) => Some((body, vec![edge])),
            _ => None,
        }
    }

    /// Beendet das Markieren (Fenster geschlossen).
    pub fn finish(&self, state: &mut AppState) {
        state.edge_picking = false;
        state.clear_edge_marks();
    }
```

Am Anfang von `show`:

```rust
        state.edge_picking = true;
        state.chain_marking = self.tangentenkette;
        if !self.seeded {
            self.seeded = true;
            if let Some(Pick::Edge(e)) = state.selection {
                state.toggle_edge_mark(e, false);
            }
        }
```

den Hinweis „Kante im Fenster anklicken.“ zu „Kante im Fenster anklicken (Umschalt: mehrere).“ ändern und am Ende von `show` statt `return self.execute(state);` bzw. `open && !cancel`:

```rust
        let keep = if run { self.execute(state) } else { open && !cancel };
        if !keep {
            self.finish(state);
        }
        keep
```

- [ ] **Step 8: App** – in `crates/planora-app/src/app.rs` nach dem Block des Rundungsfensters:

```rust
        if tab.blend_dialog.is_none() && tab.state.edge_picking {
            tab.state.edge_picking = false;
            tab.state.clear_edge_marks();
        }
```

- [ ] **Step 9: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-render && cargo test -p planora-app`
Expected: PASS, alle alten Szenentests unverändert grün.

- [ ] **Step 10: Prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings`

```bash
git add crates/planora-render/src/scene.rs crates/planora-render/src/lib.rs crates/planora-render/tests/scene_marks.rs crates/planora-app/src/state.rs crates/planora-app/src/viewport.rs crates/planora-app/src/blend_dialog.rs crates/planora-app/src/app.rs crates/planora-app/tests/blend_marks.rs
git commit -m "app: Mehrfachauswahl von Kanten mit Hervorhebung und Tangentenkette

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Ecken im Stil rollende Kugel (Gehrung, Kugelstück)

**Files:**
- Create: `crates/planora-kernel/src/blend/corner.rs`
- Modify: `crates/planora-kernel/src/blend/mod.rs` (`pub mod corner;`), `crates/planora-kernel/src/blend/build.rs` (`vertex_pair`, `corner_many` rufen `corner` auf)
- Modify (Übergangstest ersetzen, P12): `crates/planora-kernel/tests/blend_chain.rs` (`two_edges_at_a_corner_are_refused_until_corners_exist` entfernen; ersetzt durch die Tests unten)
- Test: `crates/planora-kernel/tests/blend_corner.rs`

**Interfaces:**
- Consumes: Task 4/9 (`cut_with`, `chain_loop`, `orient_outer`, `loop_patch`, `same_cut`, `blend_tol`, `ON_CURVE_FACTOR`, `EndCut`, `EndPlan`), `leaving_direction`, `vertex_faces`, `outward_sense`.
- Produces:
  ```rust
  pub mod corner {
      pub const ORTHOGONAL_TOL: f64;     // 1e-9
      pub fn miter_pair(solid: &Solid, geoms: &[EdgeGeom], v: Id<Vertex>, x: (usize, bool), y: (usize, bool), tol: &Tolerance) -> Result<Vec<(usize, bool, EndCut)>, BlendError>;
      pub fn corner_three(solid: &Solid, geoms: &[EdgeGeom], v: Id<Vertex>, list: &[(usize, bool)], index: usize, tol: &Tolerance) -> Result<(Vec<(usize, bool, EndCut)>, Piece, f64), BlendError>;
      pub fn ball_center(normals: [DVec3; 3], vp: DVec3, r: f64) -> Option<DVec3>;
  }
  ```
  Stufe 2: drei Kanten nur bei paarweise rechtwinkligen Ebenen (`|nᵢ · nⱼ| ≤ ORTHOGONAL_TOL`), gleichem Radius, konvex, nur Rundungen; Task 17 lockert die Rechtwinkligkeit.

- [ ] **Step 1: Tests schreiben** (`crates/planora-kernel/tests/blend_corner.rs`)

```rust
//! Ecken: Gehrung zweier Rundungen, Kugelstück an drei Kanten (Teilprojekt 8, Task 11).

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn blend(s: &Solid, edges: &[Id<Edge>], kind: BlendKind) -> Result<Blended, BlendError> {
    k().blend(
        s,
        &BlendRequest {
            edges: edges.iter().map(|&edge| BlendEdge { edge, first_face: None }).collect(),
            kind,
            tangent_chain: false,
        },
    )
}

fn names(s: &Solid) -> Vec<String> {
    s.faces.iter().map(|(id, _)| face_name(s, id)).collect()
}

#[test]
fn two_fillets_at_a_corner_meet_in_a_miter() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Deckel", "Wand 2");
    let r = blend(&b, &[e1, e2], BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), 8);
    let c = blend(&b, &[e1, e2], BlendKind::ChamferEqual { distance: 1.0 }).unwrap();
    validate(&c.solid).unwrap();
}

#[test]
fn three_fillets_at_a_corner_get_a_sphere_patch() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let edges = [
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
        edge_between(&b, "Wand 1", "Wand 2"),
    ];
    let r = blend(&b, &edges, BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&r.solid).unwrap();
    assert!(names(&r.solid).contains(&"Ecke 1".to_string()));
    let (_, corner) = r.solid.faces.iter().find(|(id, _)| face_name(&r.solid, *id) == "Ecke 1").unwrap();
    match &corner.surface {
        Surface::Sphere { frame, radius } => {
            assert!((radius - 1.0).abs() < 1e-12);
            assert!(frame.origin.distance(DVec3::new(9.0, 1.0, 9.0)) < 1e-9, "{:?}", frame.origin);
        }
        other => panic!("{other:?}"),
    }
    // Schätzung: Kanten bis zur Kugelebene, dazu das Eckstück.
    let a = 1.0 - PI / 4.0;
    let want = -(3.0 * a * (10.0 - 1.0) + (1.0 - PI / 6.0));
    assert!((r.volume_change - want).abs() < 2e-3 * want.abs(), "{} statt {want}", r.volume_change);
}

#[test]
fn all_twelve_box_edges_round_into_eight_corners() {
    let b = k().make_box(DVec3::new(10.0, 8.0, 6.0)).unwrap();
    let edges: Vec<Id<Edge>> = b.edges.iter().map(|(id, _)| id).collect();
    let r = blend(&b, &edges, BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), 6 + 12 + 8);
    for n in 1..=8 {
        assert!(names(&r.solid).contains(&format!("Ecke {n}")));
    }
}

#[test]
fn unsupported_corners_are_named() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let three = [
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
        edge_between(&b, "Wand 1", "Wand 2"),
    ];
    assert!(matches!(
        blend(&b, &three, BlendKind::ChamferEqual { distance: 1.0 }),
        Err(BlendError::Corner(m)) if m.contains("Fase")
    ));
    // Konvexe Deckelkante trifft die konkave Innenkante des L-Profils.
    let l = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(10.0, 0.0),
                    DVec2::new(10.0, 4.0),
                    DVec2::new(4.0, 4.0),
                    DVec2::new(4.0, 10.0),
                    DVec2::new(0.0, 10.0),
                ])
                .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            5.0,
        )
        .unwrap();
    let mixed = [edge_between(&l, "Deckel", "Wand 3"), edge_between(&l, "Wand 3", "Wand 4")];
    assert!(matches!(
        blend(&l, &mixed, BlendKind::Fillet { radius: 1.0 }),
        Err(BlendError::Corner(m)) if m.contains("konkav")
    ));
}

#[test]
fn the_ball_center_is_at_radius_from_three_planes() {
    let n = [DVec3::Z, -DVec3::Y, DVec3::X];
    let s = planora_kernel::blend::corner::ball_center(n, DVec3::new(10.0, 0.0, 10.0), 1.0).unwrap();
    assert!(s.distance(DVec3::new(9.0, 1.0, 9.0)) < 1e-12);
    assert!(planora_kernel::blend::corner::ball_center([DVec3::Z, DVec3::Z, DVec3::X], DVec3::ZERO, 1.0).is_none());
}
```

und in `crates/planora-kernel/tests/blend_chain.rs` den Test `two_edges_at_a_corner_are_refused_until_corners_exist` löschen.

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_corner`
Expected: FAIL – `could not find corner in blend`.

- [ ] **Step 3: Ecken schreiben** (`crates/planora-kernel/src/blend/corner.rs`)

```rust
//! Ecken im Stil „rollende Kugel“ (Spec §4.3, R7, P6): Zwei gerundete Kanten an einer Ecke
//! treffen sich an der winkelhalbierenden Ebene (Gehrung, nur bei spiegelgleicher Lage, sonst
//! benannt abgelehnt); drei Rundungen mit gleichem Radius an einer Ecke aus drei Ebenen bekommen
//! ein Kugelstück um den Punkt im Abstand r von allen drei Ebenen.

use super::analytic::{EdgeGeom, outward_sense};
use super::build::{
    EndCut, ON_CURVE_FACTOR, blend_tol, chain_loop, cut_with, loop_patch, orient_outer, same_cut,
};
use super::chain::leaving_direction;
use super::error::BlendError;
use super::topo::vertex_faces;
use crate::boolean::Piece;
use crate::geometry::Surface;
use crate::ids::Id;
use crate::topology::{FaceTag, Solid, Vertex};
use planora_math::{DVec3, Frame, PI, Tolerance};

/// Größter Betrag von `nᵢ · nⱼ`, bei dem drei Flächen als paarweise rechtwinklig gelten
/// (Stufe 2; Task 17 lockert).
pub const ORTHOGONAL_TOL: f64 = 1e-9;
/// Kleinster Betrag der Determinante der drei Normalen: darunter schneiden sich die drei
/// Ebenen nicht sicher in einem Punkt.
const MIN_CORNER_DET: f64 = 1e-6;
/// Spielraum der Pol-Prüfung des Kugelstücks (Baryzentrische Koordinaten).
const POLE_MARGIN: f64 = 1e-6;

fn corner(msg: &str) -> BlendError {
    BlendError::Corner(msg.into())
}

/// Gehrung: zwei gerundete Kanten an einer Ecke mit drei Flächen, die dritte Kante bleibt.
pub fn miter_pair(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    (i, ei): (usize, bool),
    (j, ej): (usize, bool),
    tol: &Tolerance,
) -> Result<Vec<(usize, bool, EndCut)>, BlendError> {
    let (gi, gj) = (&geoms[i], &geoms[j]);
    if gi.convex != gj.convex {
        return Err(corner("An der Ecke treffen eine konvexe und eine konkave Kante zusammen"));
    }
    let faces = vertex_faces(solid, v);
    if faces.len() != 3 {
        return Err(BlendError::Corner(format!(
            "An der Ecke treffen {} Flächen zusammen; unterstützt sind drei",
            faces.len()
        )));
    }
    let fi = [gi.sides.a.face, gi.sides.b.face];
    if [gj.sides.a.face, gj.sides.b.face].iter().filter(|f| fi.contains(f)).count() != 1 {
        return Err(corner("Die beiden Kanten haben keine gemeinsame Fläche"));
    }
    let vp = solid.vertices[v].point;
    let n = (leaving_direction(solid, gi.edge, v) - leaving_direction(solid, gj.edge, v)).normalize_or_zero();
    if n == DVec3::ZERO {
        return Err(corner("Die beiden Kanten laufen in dieselbe Richtung"));
    }
    let plane = Surface::Plane { frame: Frame::from_origin_normal(vp, n) };
    let cut = |g: &EdgeGeom, end: bool| {
        cut_with(g, end, &plane, vp, tol)
            .map(|(sa, sb, curve)| EndCut { sa, sb, curve, cap: None })
            .ok_or_else(|| corner("Die Rundungen lassen sich an der Ecke nicht zusammenführen"))
    };
    let (ci, cj) = (cut(gi, ei)?, cut(gj, ej)?);
    let weld = ON_CURVE_FACTOR * blend_tol(solid, geoms, tol).linear;
    if !same_cut(&ci.curve, &cj.curve, weld) {
        return Err(corner(
            "Die Rundungen treffen sich an der Ecke nicht (verschiedene Maße oder Winkel)",
        ));
    }
    Ok(vec![(i, ei, ci), (j, ej, cj)])
}

/// Punkt im Abstand `r` innerhalb aller drei Ebenen durch `vp` mit den Außennormalen `n`:
/// Lösung von `nₖ · S = nₖ · vp − r` (Cramer); `None`, wenn die Ebenen fast parallel sind.
pub fn ball_center(n: [DVec3; 3], vp: DVec3, r: f64) -> Option<DVec3> {
    let det = n[0].dot(n[1].cross(n[2]));
    if !(det.abs() > MIN_CORNER_DET) {
        return None;
    }
    let d: Vec<f64> = n.iter().map(|x| x.dot(vp) - r).collect();
    Some((n[1].cross(n[2]) * d[0] + n[2].cross(n[0]) * d[1] + n[0].cross(n[1]) * d[2]) / det)
}

/// Baryzentrische Koordinaten von `d` in der Basis `n` (alle ≥ 0: `d` liegt im Kegel der drei
/// Normalen, also im Kugeldreieck der Ecke).
fn inside_cone(n: [DVec3; 3], d: DVec3) -> bool {
    let det = n[0].dot(n[1].cross(n[2]));
    let a = d.dot(n[1].cross(n[2])) / det;
    let b = n[0].dot(d.cross(n[2])) / det;
    let c = n[0].dot(n[1].cross(d)) / det;
    a > -POLE_MARGIN && b > -POLE_MARGIN && c > -POLE_MARGIN
}

/// Drei Rundungen mit gleichem Radius an einer Ecke aus drei Ebenen: jede Rundung endet an der
/// Ebene durch den Kugelmittelpunkt senkrecht zu ihrer Kante; dazwischen ein Kugelstück
/// („Ecke `index`“). Liefert die Abschlüsse, das Stück und seine geschätzte Volumenänderung.
pub fn corner_three(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    list: &[(usize, bool)],
    index: usize,
    tol: &Tolerance,
) -> Result<(Vec<(usize, bool, EndCut)>, Piece, f64), BlendError> {
    if list.len() != 3 {
        return Err(corner("An der Ecke treffen mehr als drei gerundete Kanten zusammen"));
    }
    let mut radius: Option<f64> = None;
    for &(i, _) in list {
        let g = &geoms[i];
        let r = match g.surface {
            Surface::Cylinder { radius, .. } => radius,
            Surface::Plane { .. } => return Err(corner("Ecken mit drei Fasen werden nicht unterstützt")),
            _ => return Err(corner("Nur gerade Kanten zwischen Ebenen bekommen ein Kugelstück")),
        };
        if !g.convex {
            return Err(corner("Ecken mit konkaven Kanten werden nicht unterstützt"));
        }
        match radius {
            Some(r0) if (r0 - r).abs() > tol.linear => {
                return Err(corner("Die Radien der Kanten an der Ecke sind verschieden"));
            }
            _ => radius = Some(r),
        }
    }
    let r = radius.ok_or_else(|| corner("An der Ecke fehlt eine Rundung"))?;
    let faces = vertex_faces(solid, v);
    if faces.len() != 3 {
        return Err(BlendError::Corner(format!(
            "An der Ecke treffen {} Flächen zusammen; unterstützt sind drei",
            faces.len()
        )));
    }
    let mut n = [DVec3::ZERO; 3];
    for (k, f) in faces.iter().enumerate() {
        let face = &solid.faces[*f];
        let Surface::Plane { frame } = &face.surface else {
            return Err(corner("Nur Ecken aus drei Ebenen bekommen ein Kugelstück"));
        };
        n[k] = if face.same_sense { frame.z } else { -frame.z };
    }
    if (0..3).any(|a| n[a].dot(n[(a + 1) % 3]).abs() > ORTHOGONAL_TOL) {
        return Err(corner("Nur rechtwinklige Ecken bekommen bisher ein Kugelstück"));
    }
    let vp = solid.vertices[v].point;
    let center = ball_center(n, vp, r).ok_or_else(|| corner("Die drei Flächen schneiden sich nicht in einem Punkt"))?;
    let mut cuts = Vec::with_capacity(3);
    for &(i, end) in list {
        let g = &geoms[i];
        let plane = Surface::Plane {
            frame: Frame::from_origin_normal(center, leaving_direction(solid, g.edge, v)),
        };
        let (sa, sb, curve) = cut_with(g, end, &plane, vp, tol)
            .ok_or_else(|| corner("Die Rundung lässt sich an der Kugelecke nicht abschließen"))?;
        cuts.push((i, end, EndCut { sa, sb, curve, cap: None }));
    }
    // Kugel so legen, dass beide Pole außerhalb des Kugeldreiecks liegen.
    let z = [n[0] - n[1], n[1] - n[2], n[2] - n[0]]
        .into_iter()
        .map(|d| d.normalize())
        .find(|d| !inside_cone(n, *d) && !inside_cone(n, -*d))
        .ok_or_else(|| corner("Für das Kugelstück gibt es keine Lage ohne Pol"))?;
    let mid = (n[0] + n[1] + n[2]).normalize();
    let x = (mid - z * mid.dot(z)).normalize();
    let frame = Frame { origin: center, x, y: z.cross(x), z };
    let surface = Surface::Sphere { frame, radius: r };
    let weld = ON_CURVE_FACTOR * blend_tol(solid, geoms, tol).linear;
    let segs = chain_loop(cuts.iter().map(|(_, _, c)| c.curve.clone()).collect(), weld)
        .ok_or_else(|| corner("Die Rundungen schließen das Kugelstück nicht"))?;
    let m = center + mid * r;
    let same_sense = outward_sense(mid, m, vp, true);
    let outer = orient_outer(&surface, same_sense, segs);
    let piece = Piece {
        domain: loop_patch(&surface, same_sense, &outer),
        surface,
        same_sense,
        outer,
        holes: Vec::new(),
        tag: Some(FaceTag { origin: 0, role: format!("Ecke {index}") }),
    };
    // Rechtwinklige Ecke: Würfel r³ minus Kugelachtel.
    let volume = -(1.0 - PI / 6.0) * r * r * r;
    Ok((cuts, piece, volume))
}
```

- [ ] **Step 4: Einbinden** – in `crates/planora-kernel/src/blend/mod.rs` `pub mod corner;`; in `crates/planora-kernel/src/blend/build.rs` den Fehlerzweig am Ende von `vertex_pair` ersetzen durch `super::corner::miter_pair(solid, geoms, v, x, y, tol)` und `corner_many` durch:

```rust
/// Drei gerundete Kanten an einer Ecke: Kugelstück (Task 11); mehr: benannt abgelehnt.
fn corner_many(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    list: &[(usize, bool)],
    plan: &mut EndPlan,
    tol: &Tolerance,
) -> Result<Vec<(usize, bool, EndCut)>, BlendError> {
    let index = plan.corners.len() + 1;
    let (cuts, piece, volume) = super::corner::corner_three(solid, geoms, v, list, index, tol)?;
    plan.corners.push(piece);
    plan.corner_volume += volume;
    Ok(cuts)
}
```

- [ ] **Step 5: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_corner --test blend_chain`
Expected: PASS (5 + 5 Tests).

- [ ] **Step 6: Prüfen und committen**

Run: `cargo fmt && cargo clippy -p planora-kernel --all-targets -- -D warnings && cargo test -p planora-kernel`

```bash
git add crates/planora-kernel/src/blend/corner.rs crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/src/blend/build.rs crates/planora-kernel/tests/blend_corner.rs crates/planora-kernel/tests/blend_chain.rs
git commit -m "kernel: Ecken der Rundung mit Gehrung und Kugelstück

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Kern-Gesamttests Stufe 2 (Ketten, Ecken, Zufallsreihe)

**Files:**
- Create: `crates/planora-mesh/tests/blend_stage2.rs`

**Interfaces:**
- Consumes: `tests/blend_common` (Task 6), `Kernel::blend` mit Ketten und Ecken (Task 9, 11).
- Produces: nur Tests. Geschlossene Formeln (Kantenlänge L, Radius r, Abstand d, `A = 1 − π/4`):
  - Kugelecke: `−[Σ A·r²·(Lᵢ − r) + (1 − π/6)·r³]`
  - Gehrung zweier Rundungen: `−[A·r²·(L₁ + L₂) − (5/3 − π/2)·r³]`
  - Gehrung zweier Fasen: `−[d²/2·(L₁ + L₂) − d³/3]`
  - Alle zwölf Kanten (Steiner): `V = a'b'c' + 2(a'b' + b'c' + c'a')·r + π r²(a' + b' + c') + 4/3·π r³` mit `a' = a − 2r`
  - Stadion mit Tangentenkette: `−A·r²·(P − 2π·x̄)`, `P` Umfang, `x̄ = r(5/6 − π/4)/A`

- [ ] **Step 1: Tests schreiben** (`crates/planora-mesh/tests/blend_stage2.rs`)

```rust
//! Rundungen und Fasen, Stufe 2: Ketten, Tangentenkette, Ecken, Zufallsreihe gegen geschlossene
//! Formeln (Teilprojekt 8, Task 12).

mod blend_common;

use blend_common::*;
use planora_kernel::blend::topo::vertex_edges;
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};

const CHORD: f64 = 1e-3;
const REL: f64 = 5e-3;
const A: f64 = 1.0 - PI / 4.0;

fn stadium(h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(10.0, 0.0) },
                Segment::Arc { center: DVec2::new(10.0, 2.0), sweep: PI },
                Segment::Line { to: DVec2::new(0.0, 4.0) },
                Segment::Arc { center: DVec2::new(0.0, 2.0), sweep: PI },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

#[test]
fn a_tangent_chain_around_a_stadium_follows_steiner() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let r = 0.5;
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, true).unwrap();
    let offset = r * (5.0 / 6.0 - PI / 4.0) / A;
    let want = -A * r * r * (20.0 + 4.0 * PI - 2.0 * PI * offset);
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Stadion");
    assert!(plausible(&s, &b, 0.05));
}

#[test]
fn box_corners_follow_their_closed_forms() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let (e1, e2, e3) = (
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
        edge_between(&b, "Wand 1", "Wand 2"),
    );
    let r = 1.5;
    let three = blend_edges(&b, &[e1, e2, e3], BlendKind::Fillet { radius: r }, false).unwrap();
    let want = -(3.0 * A * r * r * (10.0 - r) + (1.0 - PI / 6.0) * r * r * r);
    assert_rel(delta_volume(&b, &three.solid, CHORD), want, REL, "Kugelecke");
    let miter = blend_edges(&b, &[e1, e2], BlendKind::Fillet { radius: r }, false).unwrap();
    let want = -(A * r * r * 20.0 - (5.0 / 3.0 - PI / 2.0) * r * r * r);
    assert_rel(delta_volume(&b, &miter.solid, CHORD), want, REL, "Gehrung");
    let d = 1.5;
    let chamfer = blend_edges(&b, &[e1, e2], BlendKind::ChamferEqual { distance: d }, false).unwrap();
    let want = -(0.5 * d * d * 20.0 - d * d * d / 3.0);
    assert_rel(delta_volume(&b, &chamfer.solid, CHORD), want, 1e-9, "Fasengehrung");
}

#[test]
fn all_twelve_edges_follow_steiner() {
    let (a, b, c, r) = (10.0, 8.0, 6.0, 1.0);
    let s = k().make_box(DVec3::new(a, b, c)).unwrap();
    let edges: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
    let res = blend_edges(&s, &edges, BlendKind::Fillet { radius: r }, false).unwrap();
    let (a1, b1, c1) = (a - 2.0 * r, b - 2.0 * r, c - 2.0 * r);
    let v = a1 * b1 * c1 + 2.0 * (a1 * b1 + b1 * c1 + c1 * a1) * r + PI * r * r * (a1 + b1 + c1)
        + 4.0 / 3.0 * PI * r * r * r;
    assert_rel(delta_volume(&s, &res.solid, CHORD), v - a * b * c, REL, "zwölf Kanten");
}

#[test]
fn corner_results_survive_follow_ups() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let edges = [
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
        edge_between(&b, "Wand 1", "Wand 2"),
    ];
    let one = blend_edges(&b, &edges, BlendKind::Fillet { radius: 1.0 }, false).unwrap().solid;
    let cutter = k()
        .transform(&k().make_box(DVec3::splat(2.0)).unwrap(), &DMat4::from_translation(DVec3::new(-1.0, 4.0, 4.0)))
        .unwrap();
    let cut = k().boolean(&one, &cutter, BoolOp::Difference).unwrap();
    validate(&cut).unwrap();
    let e = edge_between(&one, "Boden", "Wand 3");
    let two = blend_edges(&one, &[e], BlendKind::Fillet { radius: 0.5 }, false).unwrap();
    assert_rel(delta_volume(&one, &two.solid, CHORD), -A * 0.25 * 10.0, REL, "weitere Rundung");
}

/// Erwartete Volumenänderung einer Kantenauswahl an einem Quader (Radius r ≤ 0,45 · kleinste
/// Kante): Kanten bis zu Kugelecken gekürzt, Gehrungen mit ihrer Überlappung, Kugelecken.
fn box_selection_change(s: &Solid, chosen: &[Id<Edge>], r: f64) -> f64 {
    let mut removed = 0.0;
    for &e in chosen {
        let edge = &s.edges[e];
        let len = (edge.t1 - edge.t0).abs();
        let at_corner = |v| vertex_edges(s, v).iter().filter(|x| chosen.contains(x)).count();
        let short = [edge.start, edge.end].iter().filter(|v| at_corner(**v) == 3).count() as f64;
        removed += A * r * r * (len - r * short);
    }
    for (v, _) in s.vertices.iter() {
        match vertex_edges(s, v).iter().filter(|x| chosen.contains(x)).count() {
            2 => removed -= (5.0 / 3.0 - PI / 2.0) * r * r * r,
            3 => removed += (1.0 - PI / 6.0) * r * r * r,
            _ => {}
        }
    }
    -removed
}

fn random_box_selections(cases: usize, seed: u64) {
    let mut rng = Lcg(seed);
    for case in 0..cases {
        let size = DVec3::new(rng.range(4.0, 12.0), rng.range(4.0, 12.0), rng.range(4.0, 12.0));
        let s = k().make_box(size).unwrap();
        let all: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
        let mut chosen: Vec<Id<Edge>> = all.iter().copied().filter(|_| rng.next() < 0.4).collect();
        if chosen.is_empty() {
            chosen.push(all[case % all.len()]);
        }
        let r = rng.range(0.1, 0.45) * size.min_element();
        let b = blend_edges(&s, &chosen, BlendKind::Fillet { radius: r }, false)
            .unwrap_or_else(|e| panic!("Fall {case}: {e}"));
        let want = box_selection_change(&s, &chosen, r);
        assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, &format!("Fall {case}"));
        assert!(plausible(&s, &b, 0.05), "Fall {case}: Volumenprobe");
    }
}

#[test]
fn seeded_box_selections_small() {
    random_box_selections(6, 0x5eed_0001);
}

#[test]
#[ignore = "200 zufällige Kantenauswahlen, nur mit --release --ignored (etwa 60 s)"]
fn seeded_box_selections_200() {
    random_box_selections(200, 0x5eed_0002);
}

#[test]
fn concave_corners_and_mixed_corners_are_named_not_wrong() {
    let l = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    );
    let mixed = [edge_between(&l, "Deckel", "Wand 3"), edge_between(&l, "Wand 3", "Wand 4")];
    assert!(matches!(
        blend_edges(&l, &mixed, BlendKind::Fillet { radius: 1.0 }, false),
        Err(BlendError::Corner(_))
    ));
}
```

- [ ] **Step 2: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-mesh --test blend_stage2`
Expected: PASS (6 Tests, einer ignoriert), Debug-Laufzeit unter 30 s. Danach einmal `cargo test -p planora-mesh --release --test blend_stage2 -- --ignored` laufen lassen und die Laufzeit im Bericht nennen (Zahl im `#[ignore]`-Text anpassen, falls sie deutlich abweicht).

- [ ] **Step 3: Stufenabschluss prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check`
Expected: alles grün (Abschluss Stufe 2); danach Review der Stufe 2.

```bash
git add crates/planora-mesh/tests/blend_stage2.rs
git commit -m "mesh: Gesamttests Stufe 2 (Ketten, Ecken, Zufallsreihe gegen Formeln)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Stufe 3 – Freiformkanten und variabler Radius

### Task 13: Hautfläche in `planora-math`

**Files:**
- Create: `crates/planora-math/src/nurbs/skin.rs`
- Modify: `crates/planora-math/src/nurbs/mod.rs` (`mod skin; pub use skin::{MAX_SECTION_SWEEP, SkinSection, SkinTangent, skin};`)
- Test: `crates/planora-math/tests/nurbs_skin.rs`

**Interfaces:**
- Consumes: `NurbsSurface::{new, counts, control_at, weight_at, degree_u, knots_u}`, `NurbsCurve::new`, `MAX_CONTROL_POINTS`.
- Produces:
  ```rust
  pub const MAX_SECTION_SWEEP: f64;                                   // π − 1e-6
  pub struct SkinSection { pub p0: DVec3, pub p1: DVec3, pub p2: DVec3, pub w1: f64 }
  impl SkinSection { pub fn arc(center: DVec3, from: DVec3, to: DVec3) -> Result<SkinSection, NurbsError>;
                     pub fn line(from: DVec3, to: DVec3) -> SkinSection; pub fn point(&self, v: f64) -> DVec3; }
  /// Ableitungen der homogenen Reihen (p0, w1·p1, p2, w1) nach dem Längsparameter.
  pub struct SkinTangent { pub d0: DVec3, pub d1: DVec3, pub d2: DVec3, pub dw: f64 }
  impl SkinTangent { pub fn between(a: &SkinSection, b: &SkinSection, dx: f64) -> SkinTangent; }
  pub fn skin(sections: &[SkinSection], tangents: &[SkinTangent], params: &[f64]) -> Result<NurbsSurface, NurbsError>;
  impl NurbsSurface { pub fn v_boundary(&self, at_end: bool) -> Result<NurbsCurve, NurbsError>; }
  ```
  Bauart: längs stückweise kubische Hermite-Interpolation der homogenen Reihen (Grad 3, innere Knoten dreifach, also Bézier-Stücke mit gemeinsamen Enden, stetig differenzierbar über die gegebenen Ableitungen), quer Grad 2 (`v ∈ [0, 1]`). An den Stationen liefert die Fläche die Querschnitte exakt. Eine natürliche Spline-Interpolation wurde verworfen: ihre Randbedingung `C'' = 0` erzeugt an den Enden gekrümmter Kanten Fehler der Ordnung `h²`, und der Radienspline (Task 14) ist nur stetig differenzierbar.

- [ ] **Step 1: Tests schreiben** (`crates/planora-math/tests/nurbs_skin.rs`)

```rust
//! Hautfläche aus Kreisbögen und Strecken (Teilprojekt 8, Task 13).

use planora_math::nurbs::*;
use planora_math::{DVec3, PI};

fn quarter(c: DVec3, x: DVec3, y: DVec3, r: f64) -> SkinSection {
    SkinSection::arc(c, c + x * r, c + y * r).unwrap()
}

/// Querschnitte entlang eines Parameters mit numerischen Ableitungen (Differenzenquotient).
fn sections_with_tangents(params: &[f64], f: impl Fn(f64) -> SkinSection) -> (Vec<SkinSection>, Vec<SkinTangent>) {
    let h = 1e-6;
    let secs: Vec<SkinSection> = params.iter().map(|&x| f(x)).collect();
    let tans = params
        .iter()
        .map(|&x| SkinTangent::between(&f(x - h), &f(x + h), 2.0 * h))
        .collect();
    (secs, tans)
}

#[test]
fn arcs_along_a_line_give_an_exact_cylinder() {
    let params: Vec<f64> = (0..5).map(|i| i as f64).collect();
    let (secs, tans) = sections_with_tangents(&params, |x| quarter(DVec3::new(x, 0.0, 0.0), DVec3::Y, DVec3::Z, 1.5));
    let s = skin(&secs, &tans, &params).unwrap();
    for i in 0..=40 {
        for j in 0..=10 {
            let p = s.point(4.0 * i as f64 / 40.0, j as f64 / 10.0);
            assert!(((p.y * p.y + p.z * p.z).sqrt() - 1.5).abs() < 1e-9, "{p:?}");
        }
    }
    for (k, sec) in secs.iter().enumerate() {
        for j in 0..=8 {
            let v = j as f64 / 8.0;
            assert!(s.point(params[k], v).distance(sec.point(v)) < 1e-12);
        }
    }
}

#[test]
fn arcs_around_an_axis_approximate_a_torus_closely() {
    let n = 33;
    let params: Vec<f64> = (0..n).map(|i| 0.5 * PI * i as f64 / (n - 1) as f64).collect();
    let (secs, tans) = sections_with_tangents(&params, |phi| {
        let radial = DVec3::new(phi.cos(), phi.sin(), 0.0);
        quarter(radial * 5.0, radial, DVec3::Z, 1.0)
    });
    let s = skin(&secs, &tans, &params).unwrap();
    for i in 0..=200 {
        for j in 0..=10 {
            let p = s.point(0.5 * PI * i as f64 / 200.0, j as f64 / 10.0);
            let rho = (p.x * p.x + p.y * p.y).sqrt();
            let tube = ((rho - 5.0).powi(2) + p.z * p.z).sqrt();
            assert!((tube - 1.0).abs() < 1e-6, "{tube}");
        }
    }
}

#[test]
fn line_sections_give_a_ruled_surface_and_boundaries_are_exact() {
    let params = [0.0, 1.0, 2.5, 4.0];
    let (secs, tans) = sections_with_tangents(&params, |x| {
        SkinSection::line(DVec3::new(x, 0.0, 0.0), DVec3::new(x, 1.0, 1.0))
    });
    let s = skin(&secs, &tans, &params).unwrap();
    for i in 0..=20 {
        for j in 0..=4 {
            let p = s.point(4.0 * i as f64 / 20.0, j as f64 / 4.0);
            assert!((p.y - p.z).abs() < 1e-9, "{p:?}");
        }
    }
    let a = s.v_boundary(false).unwrap();
    let b = s.v_boundary(true).unwrap();
    for i in 0..=20 {
        let u = 4.0 * i as f64 / 20.0;
        assert!(a.point(u).distance(s.point(u, 0.0)) < 1e-12);
        assert!(b.point(u).distance(s.point(u, 1.0)) < 1e-12);
    }
}

#[test]
fn hostile_sections_are_rejected() {
    let c = DVec3::ZERO;
    assert!(SkinSection::arc(c, DVec3::X, DVec3::Y * 2.0).is_err(), "verschiedene Radien");
    assert!(SkinSection::arc(c, DVec3::X, -DVec3::X).is_err(), "Halbkreis");
    assert!(SkinSection::arc(c, DVec3::X, DVec3::X).is_err(), "Öffnung null");
    assert!(SkinSection::arc(c, DVec3::new(f64::NAN, 0.0, 0.0), DVec3::Y).is_err());
    let sec = SkinSection::line(DVec3::ZERO, DVec3::X);
    let tan = SkinTangent::between(&sec, &sec, 1.0);
    assert!(skin(&[sec, sec], &[tan, tan], &[0.0]).is_err(), "Anzahl");
    assert!(skin(&[sec, sec], &[tan], &[0.0, 1.0]).is_err(), "Ableitungen");
    assert!(skin(&[sec, sec], &[tan, tan], &[1.0, 0.0]).is_err(), "fallend");
    assert!(skin(&[sec], &[tan], &[0.0]).is_err(), "ein Querschnitt");
    // Riesige Gewichtsableitung: negatives Gewicht wird abgelehnt, kein Absturz.
    let arc = SkinSection::arc(c, DVec3::X, DVec3::Y).unwrap();
    let wild = SkinTangent { d0: DVec3::ZERO, d1: DVec3::ZERO, d2: DVec3::ZERO, dw: -1e9 };
    assert!(skin(&[arc, arc], &[wild, wild], &[0.0, 1.0]).is_err());
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-math --test nurbs_skin`
Expected: FAIL – `cannot find type SkinSection`.

- [ ] **Step 3: Hautfläche schreiben** (`crates/planora-math/src/nurbs/skin.rs`)

```rust
//! Hautfläche (Skinning, Spec §3.2) aus rationalen quadratischen Querschnitten: Kreisbögen
//! (Rundung) oder Strecken (Fase). Quer Grad 2 (`v ∈ [0, 1]`); längs stückweise kubische
//! Hermite-Interpolation der homogenen Reihen mit gegebenen Ableitungen (Bézier-Stücke mit
//! gemeinsamen Enden, dreifache innere Knoten). An den Stationen liefert die Fläche die
//! Querschnitte exakt; geradlinige Verschiebungen auch dazwischen.

use super::{MAX_CONTROL_POINTS, NurbsCurve, NurbsError, NurbsSurface};
use crate::DVec3;
use std::f64::consts::PI;

/// Kleinster Öffnungswinkel eines Querschnittsbogens (wie `MIN_ARC_SWEEP` der Kreisbögen).
const MIN_SECTION_SWEEP: f64 = 1e-9;
/// Größter Öffnungswinkel eines einzelnen rationalen quadratischen Stücks (knapp unter π; ein
/// Halbkreis hätte das Gewicht 0).
pub const MAX_SECTION_SWEEP: f64 = PI - 1e-6;
/// Relativer Unterschied der Abstände beider Bogenenden vom Mittelpunkt, bis zu dem sie als
/// gleich gelten (Querschnitte aus dem Newton-Verfahren sind auf etwa 1e-12 genau).
const SAME_RADIUS_REL: f64 = 1e-9;

/// Querschnitt: rationales quadratisches Bézier-Stück `p0 → p2` mit Zwischenpunkt `p1` und
/// dessen Gewicht `w1` (Endgewichte 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinSection {
    pub p0: DVec3,
    pub p1: DVec3,
    pub p2: DVec3,
    pub w1: f64,
}

/// Ableitungen der homogenen Reihen `p0`, `w1 · p1`, `p2` und `w1` nach dem Längsparameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinTangent {
    pub d0: DVec3,
    pub d1: DVec3,
    pub d2: DVec3,
    pub dw: f64,
}

impl SkinSection {
    /// Kreisbogen um `center` von `from` nach `to` (gleicher Abstand, Öffnungswinkel in
    /// `[1e-9, π − 1e-6]`): Zwischenpunkt im Schnitt der Tangenten, Gewicht `cos(θ/2)`.
    pub fn arc(center: DVec3, from: DVec3, to: DVec3) -> Result<SkinSection, NurbsError> {
        let (a, b) = (from - center, to - center);
        let (ra, rb) = (a.length(), b.length());
        if !(ra.is_finite() && rb.is_finite() && ra > 0.0)
            || (ra - rb).abs() > SAME_RADIUS_REL * ra.max(1.0)
        {
            return Err(NurbsError::Input(
                "die Bogenenden haben verschiedene Abstände vom Mittelpunkt".into(),
            ));
        }
        let theta = (a.dot(b) / (ra * rb)).clamp(-1.0, 1.0).acos();
        if !(MIN_SECTION_SWEEP..=MAX_SECTION_SWEEP).contains(&theta) {
            return Err(NurbsError::Input(
                "der Öffnungswinkel des Querschnitts ist zu klein oder zu groß".into(),
            ));
        }
        let w1 = (0.5 * theta).cos();
        let bis = (a / ra + b / rb).normalize();
        Ok(SkinSection { p0: from, p1: center + bis * (ra / w1), p2: to, w1 })
    }

    /// Strecke `from → to` (Fase) als Stück vom Grad 2.
    pub fn line(from: DVec3, to: DVec3) -> SkinSection {
        SkinSection { p0: from, p1: (from + to) * 0.5, p2: to, w1: 1.0 }
    }

    /// Punkt bei `v ∈ [0, 1]` (begrenzt).
    pub fn point(&self, v: f64) -> DVec3 {
        let v = v.clamp(0.0, 1.0);
        let (b0, b1, b2) = ((1.0 - v) * (1.0 - v), 2.0 * v * (1.0 - v) * self.w1, v * v);
        (self.p0 * b0 + self.p1 * b1 + self.p2 * b2) / (b0 + b1 + b2)
    }
}

impl SkinTangent {
    /// Differenzenquotient der homogenen Reihen von `a` nach `b` im Parameterabstand `dx`.
    pub fn between(a: &SkinSection, b: &SkinSection, dx: f64) -> SkinTangent {
        SkinTangent {
            d0: (b.p0 - a.p0) / dx,
            d1: (b.p1 * b.w1 - a.p1 * a.w1) / dx,
            d2: (b.p2 - a.p2) / dx,
            dw: (b.w1 - a.w1) / dx,
        }
    }
}

/// Hautfläche durch die Querschnitte an den streng steigenden Parametern `params` mit den
/// Ableitungen `tangents` (je Station eine; mindestens zwei Stationen, höchstens
/// `(MAX_CONTROL_POINTS − 1)/3 + 1`).
pub fn skin(
    sections: &[SkinSection],
    tangents: &[SkinTangent],
    params: &[f64],
) -> Result<NurbsSurface, NurbsError> {
    let n = sections.len();
    if n < 2 || tangents.len() != n || params.len() != n {
        return Err(NurbsError::Input(
            "Querschnitte, Ableitungen und Parameter passen nicht zusammen".into(),
        ));
    }
    if params.iter().any(|x| !x.is_finite()) || params.windows(2).any(|w| !(w[1] > w[0])) {
        return Err(NurbsError::Input("die Parameter müssen streng steigen".into()));
    }
    let nu = 3 * (n - 1) + 1;
    if nu > MAX_CONTROL_POINTS {
        return Err(NurbsError::TooLarge(format!(
            "höchstens {} Querschnitte",
            (MAX_CONTROL_POINTS - 1) / 3 + 1
        )));
    }
    let rows = |s: &SkinSection| [(s.p0, 1.0), (s.p1 * s.w1, s.w1), (s.p2, 1.0)];
    let drows = |t: &SkinTangent| [(t.d0, 0.0), (t.d1, t.dw), (t.d2, 0.0)];
    let mut hom = vec![(DVec3::ZERO, 0.0); nu * 3];
    for i in 0..n {
        let (r, d) = (rows(&sections[i]), drows(&tangents[i]));
        for j in 0..3 {
            hom[3 * i * 3 + j] = r[j];
            if i + 1 < n {
                let h = (params[i + 1] - params[i]) / 3.0;
                hom[(3 * i + 1) * 3 + j] = (r[j].0 + d[j].0 * h, r[j].1 + d[j].1 * h);
            }
            if i > 0 {
                let h = (params[i] - params[i - 1]) / 3.0;
                hom[(3 * i - 1) * 3 + j] = (r[j].0 - d[j].0 * h, r[j].1 - d[j].1 * h);
            }
        }
    }
    let mut knots = vec![params[0]; 4];
    for &x in &params[1..n - 1] {
        knots.extend([x, x, x]);
    }
    knots.extend([params[n - 1]; 4]);
    let control = hom.iter().map(|(p, w)| *p / *w).collect();
    let weights = hom.iter().map(|(_, w)| *w).collect();
    NurbsSurface::new(3, 2, knots, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], nu, 3, control, weights)
}

impl NurbsSurface {
    /// Randkurve `v = v0` (`at_end = false`) bzw. `v = v1`: die erste bzw. letzte
    /// Kontrollpunktreihe mit ihren Gewichten und dem u-Knotenvektor (exakt, weil jeder
    /// Knotenvektor geklemmt ist).
    pub fn v_boundary(&self, at_end: bool) -> Result<NurbsCurve, NurbsError> {
        let (nu, nv) = self.counts();
        let j = if at_end { nv - 1 } else { 0 };
        NurbsCurve::new(
            self.degree_u(),
            self.knots_u().knots().to_vec(),
            (0..nu).map(|i| self.control_at(i, j)).collect(),
            (0..nu).map(|i| self.weight_at(i, j)).collect(),
        )
    }
}
```

Hinweis: Ein Gewicht ≤ 0 ergibt nach der Division nicht endliche Kontrollpunkte oder ein Gewicht unter `MIN_WEIGHT`; beides lehnt `NurbsSurface::new` ab (Test `hostile_sections_are_rejected`).

- [ ] **Step 4: Modul eintragen** – in `crates/planora-math/src/nurbs/mod.rs` `mod skin;` und `pub use skin::{MAX_SECTION_SWEEP, SkinSection, SkinTangent, skin};` ergänzen.

- [ ] **Step 5: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-math --test nurbs_skin`
Expected: PASS (4 Tests).

- [ ] **Step 6: Prüfen und committen**

Run: `cargo fmt && cargo clippy -p planora-math --all-targets -- -D warnings && cargo test -p planora-math`

```bash
git add crates/planora-math/src/nurbs/skin.rs crates/planora-math/src/nurbs/mod.rs crates/planora-math/tests/nurbs_skin.rs
git commit -m "math: Hautfläche aus rationalen Querschnitten mit Hermite-Übergängen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 14: Radienspline und Bahn mit Hautfläche (variabler Radius)

**Files:**
- Create: `crates/planora-math/src/radius.rs`, `crates/planora-kernel/src/blend/march.rs`
- Modify: `crates/planora-math/src/lib.rs` (`mod radius; pub use radius::{RadiusLaw, RadiusLawError};`), `crates/planora-kernel/src/blend/mod.rs` (`pub mod march;`, Weiche variabler Radius, `section_kind` liefert `Option`, `VARIABLE_LATER` entfällt), `crates/planora-kernel/src/blend/build.rs` (`cap_cut` für Hautflächen)
- Modify (Übergangstest ersetzen, P12): `crates/planora-kernel/tests/blend_api.rs` (`variable_radii_wait_for_stage_three` löschen)
- Test: `crates/planora-math/tests/radius_law.rs`, `crates/planora-kernel/tests/blend_march.rs`

**Interfaces:**
- Consumes: Task 13 (`skin`, `SkinSection`, `SkinTangent`, `v_boundary`), Task 2 (`fillet_section`, `chamfer_section`, `check_curvature`), Task 3 (`section_seg`, `section_polygon`, `polygon_area_centroid`, `outward_sense`, `EdgeGeom`, `VolumeModel::Total`), Task 9 (`ChainedEdge::fraction`, `leaving_direction`, `section_cut`), `Surface::nurbs`, `Curve::nurbs`, `planora_math::nurbs::evaluations`, `crate::boolean::NURBS_FIT_TOL`, `crate::validate::NURBS_EDGE_TOL`.
- Produces:
  ```rust
  // planora_math
  pub struct RadiusLaw;
  impl RadiusLaw { pub fn new(points: &[(f64, f64)]) -> Result<RadiusLaw, RadiusLawError>;
                   pub fn radius(&self, x: f64) -> f64; pub fn positions(&self) -> &[f64]; }
  pub enum RadiusLawError { TooFewPoints, Positions, Radii }
  // planora_kernel::blend::march
  pub const MIN_STATIONS: usize = 9; pub const MAX_STATIONS: usize = 257;
  pub const MAX_BLEND_EVALUATIONS: u64 = 20_000_000; pub const OBLIQUE_END: &str;
  pub enum SectionLaw<'a> { Fillet(f64), Variable(&'a RadiusLaw), Chamfer(ChamferSpec) }
  pub fn march_geom(solid: &Solid, ce: &ChainedEdge, sides: &EdgeSides, convex: bool, law: SectionLaw, tol: &Tolerance) -> Result<EdgeGeom, BlendError>;
  ```
  Festlegungen: `EdgeGeom::analytic = false`, `range = (0, Kantenlänge)`, Berührkurven sind die Randkurven der Hautfläche, Querschnitte an den Enden exakte Kreisbögen bzw. Strecken. Stationen: gleichmäßig verteilt plus die Stützstellen des Radiengesetzes auf dieser Kante; Ableitungen je Station über zentrale Differenzen exakter Querschnitte im Abstand `DERIV_STEP · h`. Hautflächen werden nur an Ebenen senkrecht zur Kante, an Nähten und glatten Übergängen abgeschlossen (P5).

- [ ] **Step 1: Tests schreiben** (`crates/planora-math/tests/radius_law.rs`)

```rust
//! Radius entlang einer Kette (Teilprojekt 8, Task 14).

use planora_math::{RadiusLaw, RadiusLawError};

#[test]
fn the_law_passes_its_points_without_overshoot() {
    let pts = [(0.0, 1.0), (0.3, 3.0), (0.5, 2.9), (1.0, 0.5)];
    let law = RadiusLaw::new(&pts).unwrap();
    assert_eq!(law.positions(), &[0.0, 0.3, 0.5, 1.0]);
    for (s, r) in pts {
        assert!((law.radius(s) - r).abs() < 1e-15);
    }
    for w in pts.windows(2) {
        let (lo, hi) = (w[0].1.min(w[1].1), w[0].1.max(w[1].1));
        for i in 0..=1000 {
            let s = w[0].0 + (w[1].0 - w[0].0) * i as f64 / 1000.0;
            let r = law.radius(s);
            assert!(r >= lo - 1e-12 && r <= hi + 1e-12, "s={s}: {r}");
        }
    }
    assert_eq!(law.radius(-1.0), 1.0);
    assert_eq!(law.radius(2.0), 0.5);
    assert_eq!(law.radius(f64::NAN), 1.0);
    let h = 1e-7;
    for s in [0.3, 0.5] {
        let left = (law.radius(s) - law.radius(s - h)) / h;
        let right = (law.radius(s + h) - law.radius(s)) / h;
        assert!((left - right).abs() < 1e-4, "s={s}: {left} / {right}");
    }
}

#[test]
fn hostile_points_are_rejected() {
    assert_eq!(RadiusLaw::new(&[(0.0, 1.0)]), Err(RadiusLawError::TooFewPoints));
    assert_eq!(RadiusLaw::new(&[(0.5, 1.0), (0.5, 2.0)]), Err(RadiusLawError::Positions));
    assert_eq!(RadiusLaw::new(&[(0.0, 1.0), (f64::NAN, 2.0)]), Err(RadiusLawError::Positions));
    assert_eq!(RadiusLaw::new(&[(0.0, 1.0), (1.0, 0.0)]), Err(RadiusLawError::Radii));
    assert_eq!(RadiusLaw::new(&[(0.0, 1.0), (1.0, f64::INFINITY)]), Err(RadiusLawError::Radii));
}
```

(`crates/planora-kernel/tests/blend_march.rs`)

```rust
//! Bahn mit Hautfläche: variabler Radius, Übereinstimmung mit der analytischen Rundung,
//! Grenzen (Teilprojekt 8, Task 14).

use planora_kernel::blend::analytic::{SectionKind, analytic_geom};
use planora_kernel::blend::chain::select_edges;
use planora_kernel::blend::march::*;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn req(e: Id<Edge>, kind: BlendKind) -> BlendRequest {
    BlendRequest { edges: vec![BlendEdge { edge: e, first_face: None }], kind, tangent_chain: false }
}

#[test]
fn a_constant_march_reproduces_the_analytic_cylinder() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let r = req(e, BlendKind::Fillet { radius: 2.0 });
    let ce = select_edges(&s, &r).unwrap()[0];
    let sides = edge_sides(&s, e, None).unwrap();
    let tol = Tolerance::default();
    let m = march_geom(&s, &ce, &sides, true, SectionLaw::Fillet(2.0), &tol).unwrap();
    let a = analytic_geom(&s, e, &sides, true, SectionKind::Fillet { radius: 2.0 }, &tol).unwrap().unwrap();
    assert!(!m.analytic && matches!(m.surface, Surface::Nurbs(_)));
    for i in 0..=20 {
        let f = i as f64 / 20.0;
        let pm = m.contact_a.point(m.range.0 + (m.range.1 - m.range.0) * f);
        let pa = a.contact_a.point(a.range.0 + (a.range.1 - a.range.0) * f);
        assert!(pm.distance(pa) < 1e-6, "{pm:?} / {pa:?}");
    }
}

#[test]
fn a_variable_radius_on_a_box_edge() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = k()
        .blend(&s, &req(e, BlendKind::VariableFillet { points: vec![(0.0, 1.0), (1.0, 2.0)] }))
        .unwrap();
    validate(&b.solid).unwrap();
    // Zwei Punkte: das Gesetz ist linear, r(s) = 1 + s/10; abgetragen wird
    // (1 − π/4) · ∫₀¹⁰ (1 + s/10)² ds = (1 − π/4) · 10 · 7/3.
    let want = -(1.0 - PI / 4.0) * 10.0 * 7.0 / 3.0;
    assert!((b.volume_change - want).abs() < 3e-3 * want.abs(), "{} statt {want}", b.volume_change);
}

#[test]
fn closed_edges_need_equal_radii_at_the_seam() {
    let c = k().make_cylinder(5.0, 8.0).unwrap();
    let e = edge_between(&c, "Wand 1", "Deckel");
    let r = k().blend(&c, &req(e, BlendKind::VariableFillet { points: vec![(0.0, 1.0), (1.0, 1.5)] }));
    assert!(
        matches!(&r, Err(BlendError::InvalidParameter(m)) if m.contains("geschlossenen")),
        "{:?}",
        r.map(|_| ())
    );
    let ok = k()
        .blend(&c, &req(e, BlendKind::VariableFillet { points: vec![(0.0, 1.0), (0.5, 1.5), (1.0, 1.0)] }))
        .unwrap();
    validate(&ok.solid).unwrap();
}

#[test]
fn oblique_ends_of_a_skin_are_named() {
    let tri = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0), DVec2::new(5.0, 75f64.sqrt())])
                    .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            4.0,
        )
        .unwrap();
    let e = edge_between(&tri, "Deckel", "Wand 1");
    match k().blend(&tri, &req(e, BlendKind::VariableFillet { points: vec![(0.0, 0.5), (1.0, 0.8)] })) {
        Err(BlendError::Unsupported(m)) => assert_eq!(m, OBLIQUE_END),
        other => panic!("{:?}", other.map(|_| ())),
    }
}
```

und in `crates/planora-kernel/tests/blend_api.rs` den Test `variable_radii_wait_for_stage_three` löschen.

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-math --test radius_law && cargo test -p planora-kernel --test blend_march`
Expected: FAIL – `cannot find type RadiusLaw`.

- [ ] **Step 3: Radienspline schreiben** (`crates/planora-math/src/radius.rs`)

```rust
//! Radius entlang einer Kantenkette (Spec §3.3, R9, Plan P8): monotone kubische
//! Hermite-Interpolation (Fritsch–Butland) durch Punkte (Anteil, Radius). Zwischen zwei Punkten
//! liegt der Radius immer zwischen ihren Werten (kein Überschwingen, also nie unter null),
//! außerhalb des ersten und letzten Punkts konstant. Stetig differenzierbar.

/// Fehler beim Anlegen eines Radiusgesetzes.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum RadiusLawError {
    #[error("Ein variabler Radius braucht mindestens zwei Punkte")]
    TooFewPoints,
    #[error("Die Positionen müssen endlich sein und streng steigen")]
    Positions,
    #[error("Jeder Radius muss endlich und größer als null sein")]
    Radii,
}

/// Radius als Funktion des Anteils der Kettenlänge.
#[derive(Clone, Debug, PartialEq)]
pub struct RadiusLaw {
    s: Vec<f64>,
    r: Vec<f64>,
    /// Steigungen an den Punkten.
    m: Vec<f64>,
}

impl RadiusLaw {
    pub fn new(points: &[(f64, f64)]) -> Result<RadiusLaw, RadiusLawError> {
        if points.len() < 2 {
            return Err(RadiusLawError::TooFewPoints);
        }
        let s: Vec<f64> = points.iter().map(|p| p.0).collect();
        let r: Vec<f64> = points.iter().map(|p| p.1).collect();
        if s.iter().any(|x| !x.is_finite()) || s.windows(2).any(|w| !(w[1] > w[0])) {
            return Err(RadiusLawError::Positions);
        }
        if r.iter().any(|x| !(x.is_finite() && *x > 0.0)) {
            return Err(RadiusLawError::Radii);
        }
        let n = s.len();
        let h: Vec<f64> = s.windows(2).map(|w| w[1] - w[0]).collect();
        let d: Vec<f64> = (0..n - 1).map(|k| (r[k + 1] - r[k]) / h[k]).collect();
        let mut m = vec![0.0; n];
        m[0] = d[0];
        m[n - 1] = d[n - 2];
        for k in 1..n - 1 {
            m[k] = if d[k - 1] * d[k] <= 0.0 {
                0.0
            } else {
                // Gewichtetes harmonisches Mittel: höchstens das Dreifache der kleineren
                // Nachbarsteigung, damit bleibt jedes Stück monoton (Fritsch–Carlson-Bereich).
                3.0 * (h[k - 1] + h[k])
                    / ((2.0 * h[k] + h[k - 1]) / d[k - 1] + (h[k] + 2.0 * h[k - 1]) / d[k])
            };
        }
        Ok(RadiusLaw { s, r, m })
    }

    /// Positionen der Punkte (Anteile).
    pub fn positions(&self) -> &[f64] {
        &self.s
    }

    /// Radius beim Anteil `x` (außerhalb konstant; NaN ergibt den ersten Radius).
    pub fn radius(&self, x: f64) -> f64 {
        let n = self.s.len();
        if !(x > self.s[0]) {
            return self.r[0];
        }
        if x >= self.s[n - 1] {
            return self.r[n - 1];
        }
        let k = self.s.partition_point(|&t| t <= x) - 1;
        let h = self.s[k + 1] - self.s[k];
        let t = (x - self.s[k]) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.r[k]
            + (t3 - 2.0 * t2 + t) * h * self.m[k]
            + (-2.0 * t3 + 3.0 * t2) * self.r[k + 1]
            + (t3 - t2) * h * self.m[k + 1]
    }
}
```

In `crates/planora-math/src/lib.rs` `mod radius;` und `pub use radius::{RadiusLaw, RadiusLawError};` ergänzen.

- [ ] **Step 4: Bahn schreiben** (`crates/planora-kernel/src/blend/march.rs`)

```rust
//! Bahn der rollenden Kugel entlang beliebiger Kanten (Spec §3.1, §3.2): Querschnitte an
//! Stationen per Newton (Startwert: voriger Mittelpunkt), Ableitungen über zentrale Differenzen
//! exakter Querschnitte, Hautfläche durch die Querschnitte, Berührkurven als ihre Randkurven.
//! Die Zahl der Stationen verdoppelt sich, bis die Hautfläche an jeder Zwischenstelle den
//! exakten Querschnitt auf `NURBS_FIT_TOL / 2` trifft und die Berührkurven beide Flächen auf
//! `NURBS_EDGE_TOL / 2` (Prüfung gegen beide Flächen wie beim Schnittrechner). Arbeitsgrenzen
//! zählen Stationen und NURBS-Auswertungen, keine Zeit.

use super::analytic::{
    EdgeGeom, SectionKind, VolumeModel, outward_sense, polygon_area_centroid, section_polygon,
    section_seg,
};
use super::chain::ChainedEdge;
use super::error::BlendError;
use super::section::{ChamferSpec, Section, Side, chamfer_section, check_curvature, fillet_section};
use super::topo::{EdgeSides, face_name, inward_directions};
use crate::boolean::NURBS_FIT_TOL;
use crate::geometry::{Curve, Surface};
use crate::topology::{Edge, Solid};
use crate::validate::NURBS_EDGE_TOL;
use planora_math::nurbs::{NurbsError, SkinSection, SkinTangent, evaluations, skin};
use planora_math::{DVec3, RadiusLaw, Tolerance};

/// Anfangszahl der gleichmäßigen Stationen (verdoppelt bis `MAX_STATIONS`: 9, 17, 33, 65, 129, 257).
pub const MIN_STATIONS: usize = 9;
/// Höchstzahl der gleichmäßigen Stationen.
pub const MAX_STATIONS: usize = 257;
/// Höchstzahl NURBS-Auswertungen je Kante (deterministisch, wie `MAX_FREEFORM_EVALUATIONS`).
pub const MAX_BLEND_EVALUATIONS: u64 = 20_000_000;
/// Stützstellen der Bogenlängentabelle einer Kante.
const LENGTH_SAMPLES: usize = 512;
/// Schritt der zentralen Differenzen relativ zum Stationsabstand (Fehler ~ (Schritt)², Rauschen
/// ~ Newton-Rest / Schritt; beide weit unter der Schranke).
const DERIV_STEP: f64 = 1e-3;
/// Stützstellen des Radiengesetzes, die näher als dieser Bruchteil der Kantenlänge an einer
/// Station liegen, werden nicht zusätzlich eingefügt.
const MIN_STATION_GAP: f64 = 1e-6;
/// Erlaubte Abweichung der Hautfläche vom exakten Querschnitt.
const SKIN_TOL: f64 = 0.5 * NURBS_FIT_TOL;
/// Erlaubter Abstand der Berührkurven von ihren Flächen.
const CONTACT_TOL: f64 = 0.5 * NURBS_EDGE_TOL;
/// Meldung bei schrägem Abschluss einer Hautfläche (P5).
pub const OBLIQUE_END: &str = "Eine Freiform- oder variable Rundung lässt sich nur an einer Ebene senkrecht zur Kante abschließen";
const INACCURATE: &str = "Die Rundungsfläche lässt sich nicht genau genug annähern; bitte Maß oder Form leicht ändern";

/// Querschnittsgesetz entlang der Kante.
#[derive(Clone, Copy, Debug)]
pub enum SectionLaw<'a> {
    Fillet(f64),
    Variable(&'a RadiusLaw),
    Chamfer(ChamferSpec),
}

/// Kantenparameter zu Bogenlängen (Streckenzug mit `LENGTH_SAMPLES` Abschnitten).
struct ArcTable {
    t: Vec<f64>,
    s: Vec<f64>,
}

impl ArcTable {
    fn new(e: &Edge) -> ArcTable {
        let mut t = Vec::with_capacity(LENGTH_SAMPLES + 1);
        let mut s = Vec::with_capacity(LENGTH_SAMPLES + 1);
        let mut acc = 0.0;
        let mut prev = e.curve.point(e.t0);
        for i in 0..=LENGTH_SAMPLES {
            let ti = e.t0 + (e.t1 - e.t0) * i as f64 / LENGTH_SAMPLES as f64;
            let p = e.curve.point(ti);
            acc += p.distance(prev);
            prev = p;
            t.push(ti);
            s.push(acc);
        }
        ArcTable { t, s }
    }

    fn length(&self) -> f64 {
        self.s.last().copied().unwrap_or(0.0)
    }

    fn param(&self, x: f64) -> f64 {
        let k = self.s.partition_point(|&v| v < x).clamp(1, self.s.len() - 1);
        let (s0, s1) = (self.s[k - 1], self.s[k]);
        let f = if s1 > s0 { ((x - s0) / (s1 - s0)).clamp(0.0, 1.0) } else { 0.0 };
        self.t[k - 1] + (self.t[k] - self.t[k - 1]) * f
    }
}

fn to_skin(kind: SectionKind, s: &Section) -> Result<SkinSection, BlendError> {
    match kind {
        SectionKind::Fillet { .. } => SkinSection::arc(s.center, s.pa, s.pb).map_err(|e| {
            BlendError::Unsupported(format!("Ein Querschnitt der Rundung ist ungültig: {e}"))
        }),
        SectionKind::Chamfer(_) => Ok(SkinSection::line(s.pa, s.pb)),
    }
}

fn invalid(err: NurbsError) -> BlendError {
    BlendError::Unsupported(format!("Die Rundungsfläche ist ungültig: {err}"))
}

/// Rundung bzw. Fase entlang der Kante `ce` als Hautfläche.
pub fn march_geom(
    solid: &Solid,
    ce: &ChainedEdge,
    sides: &EdgeSides,
    convex: bool,
    law: SectionLaw,
    tol: &Tolerance,
) -> Result<EdgeGeom, BlendError> {
    let edge = ce.edge.edge;
    let e = &solid.edges[edge];
    let (fa, fb) = (&solid.faces[sides.a.face], &solid.faces[sides.b.face]);
    let a = Side { surface: &fa.surface, same_sense: fa.same_sense };
    let b = Side { surface: &fb.surface, same_sense: fb.same_sense };
    let (name_a, name_b) = (face_name(solid, sides.a.face), face_name(solid, sides.b.face));
    let table = ArcTable::new(e);
    let length = table.length();
    let closed = e.start == e.end;
    if let SectionLaw::Variable(l) = law
        && closed
        && (l.radius(ce.fraction(0.0)) - l.radius(ce.fraction(length))).abs() > tol.linear
    {
        return Err(BlendError::InvalidParameter(
            "Auf einer geschlossenen Kante muss der Radius am Anfang und am Ende gleich sein".into(),
        ));
    }
    // Stützstellen des Radiengesetzes auf dieser Kante (Bogenlänge ab Kantenanfang).
    let breaks: Vec<f64> = match law {
        SectionLaw::Variable(l) => l
            .positions()
            .iter()
            .map(|f| {
                let along = f * ce.chain_length - ce.offset;
                if ce.reversed { ce.length - along } else { along }
            })
            .filter(|x| *x > MIN_STATION_GAP * length && *x < length * (1.0 - MIN_STATION_GAP))
            .collect(),
        _ => Vec::new(),
    };
    let start = evaluations();
    let budget = || {
        if evaluations().saturating_sub(start) > MAX_BLEND_EVALUATIONS {
            Err(BlendError::TooExpensive)
        } else {
            Ok(())
        }
    };
    // Querschnitt bei der Bogenlänge `x`; `prev` ist der Mittelpunkt einer Nachbarstation.
    let section_at = |x: f64, prev: Option<DVec3>| -> Result<(Section, DVec3, SectionKind), BlendError> {
        let t = table.param(x);
        let o = e.curve.point(t);
        let tangent = e.curve.tangent(t) * (e.t1 - e.t0).signum();
        let kind = match law {
            SectionLaw::Fillet(r) => SectionKind::Fillet { radius: r },
            SectionLaw::Variable(l) => SectionKind::Fillet { radius: l.radius(ce.fraction(x)) },
            SectionLaw::Chamfer(spec) => SectionKind::Chamfer(spec),
        };
        let sec = match kind {
            SectionKind::Fillet { radius } => {
                check_curvature(a, o, convex, radius, &name_a)?;
                check_curvature(b, o, convex, radius, &name_b)?;
                fillet_section(a, b, o, tangent, convex, radius, prev)?
            }
            SectionKind::Chamfer(spec) => {
                let (wa, wb) = inward_directions(solid, edge, sides, t);
                chamfer_section(a, b, o, tangent, convex, wa, wb, spec)?
            }
        };
        budget()?;
        Ok((sec, o, kind))
    };
    let mut n = MIN_STATIONS;
    loop {
        let mut xs: Vec<f64> = (0..n).map(|i| length * i as f64 / (n - 1) as f64).collect();
        for &x in &breaks {
            if xs.iter().all(|s| (s - x).abs() > MIN_STATION_GAP * length) {
                xs.push(x);
            }
        }
        xs.sort_by(f64::total_cmp);
        let m = xs.len();
        let mut secs: Vec<(Section, DVec3, SectionKind)> = Vec::with_capacity(m);
        let mut prev = None;
        for &x in &xs {
            let s = section_at(x, prev)?;
            prev = Some(s.0.center);
            secs.push(s);
        }
        if closed {
            // Naht: erster und letzter Querschnitt exakt gleich (geschlossene Hautfläche).
            secs[m - 1] = secs[0];
        }
        let sk = secs.iter().map(|(s, _, k)| to_skin(*k, s)).collect::<Result<Vec<_>, _>>()?;
        let delta = DERIV_STEP * length / (n - 1) as f64;
        let mut tangents = Vec::with_capacity(m);
        for (i, &x) in xs.iter().enumerate() {
            let at_seam = closed && (i == 0 || i == m - 1);
            let (lo, hi) = if at_seam {
                (length - delta, delta)
            } else {
                ((x - delta).max(0.0), (x + delta).min(length))
            };
            let span = if at_seam { 2.0 * delta } else { hi - lo };
            let (sl, _, kl) = section_at(lo, Some(secs[i].0.center))?;
            let (sh, _, kh) = section_at(hi, Some(secs[i].0.center))?;
            tangents.push(SkinTangent::between(&to_skin(kl, &sl)?, &to_skin(kh, &sh)?, span));
        }
        let surface = skin(&sk, &tangents, &xs).map_err(|err| {
            BlendError::Unsupported(format!("Die Rundungsfläche lässt sich nicht bilden: {err}"))
        })?;
        let mut accurate = true;
        for (i, w) in xs.windows(2).enumerate() {
            let xm = 0.5 * (w[0] + w[1]);
            let (exact, _, kind) = section_at(xm, Some(secs[i].0.center))?;
            let want = to_skin(kind, &exact)?;
            let off = [(0.0, exact.pa), (0.5, want.point(0.5)), (1.0, exact.pb)]
                .iter()
                .any(|(v, p)| surface.point(xm, *v).distance(*p) > SKIN_TOL);
            let loose = a.outside(surface.point(xm, 0.0)).abs() > CONTACT_TOL
                || b.outside(surface.point(xm, 1.0)).abs() > CONTACT_TOL;
            if off || loose {
                accurate = false;
                break;
            }
        }
        if accurate {
            let contact_a = Curve::nurbs(surface.v_boundary(false).map_err(invalid)?);
            let contact_b = Curve::nurbs(surface.v_boundary(true).map_err(invalid)?);
            let mid = surface.point(xs[0], 0.5);
            let blend_surface = Surface::nurbs(surface, closed, false)
                .map_err(|err| BlendError::Unsupported(format!("Die Rundungsfläche ist ungültig: {err}")))?;
            let natural = blend_surface.normal(xs[0], 0.5);
            let (first, last) = (secs[0], secs[m - 1]);
            let mut volume = 0.0;
            let mut before: Option<(f64, DVec3)> = None;
            for (sec, o, kind) in &secs {
                let (area, c) = polygon_area_centroid(&section_polygon(*kind, *o, sec));
                if let Some((a0, c0)) = before {
                    volume += 0.5 * (a0 + area) * c.distance(c0);
                }
                before = Some((area, c));
            }
            let reach = secs
                .iter()
                .map(|(s, o, _)| s.pa.distance(*o).max(s.pb.distance(*o)))
                .fold(0.0, f64::max);
            return Ok(EdgeGeom {
                edge,
                sides: *sides,
                convex,
                same_sense: outward_sense(natural, mid, first.1, convex),
                surface: blend_surface,
                contact_a,
                contact_b,
                range: (0.0, length),
                section_start: section_seg(first.2, &first.0),
                section_end: section_seg(last.2, &last.0),
                volume: VolumeModel::Total { volume },
                reach,
                closed,
                analytic: false,
            });
        }
        if n >= MAX_STATIONS {
            return Err(BlendError::Unsupported(INACCURATE.into()));
        }
        n = 2 * n - 1;
    }
}
```

- [ ] **Step 5: Abschluss der Hautfläche** – in `crates/planora-kernel/src/blend/build.rs` oben die Konstante

```rust
/// Sinus des größten Winkels zwischen Normale der Abschlussebene und Kante, bei dem eine
/// Hautfläche noch als senkrecht abgeschlossen gilt (Ebenen aus Skizzen sind auf 1e-12 genau).
const PERPENDICULAR_SIN: f64 = 1e-9;
```

und in `cap_cut` nach `let vp = …;`:

```rust
    if !g.analytic {
        let d = leaving_direction(solid, g.edge, v);
        let perpendicular = match &solid.faces[c].surface {
            Surface::Plane { frame } => {
                frame.z.cross(d).length() <= PERPENDICULAR_SIN
                    && frame.z.dot(vp - frame.origin).abs() <= tol.linear
            }
            _ => false,
        };
        if !perpendicular {
            return Err(BlendError::Unsupported(super::march::OBLIQUE_END.into()));
        }
        let mut cut = section_cut(g, at_end);
        cut.cap = Some((c, v));
        return Ok(cut);
    }
```

- [ ] **Step 6: Weiche im Einstieg** – in `crates/planora-kernel/src/blend/mod.rs`: `pub mod march;`, `use march::{SectionLaw, march_geom};`, `use chain::ChainedEdge;`, `use planora_math::RadiusLaw;`; `VARIABLE_LATER` löschen; `section_kind` und `edge_geom` ersetzen; in `blend` das Gesetz bauen:

```rust
/// Querschnittsart einer konstanten Verrundung (`None` beim variablen Radius).
fn section_kind(kind: &BlendKind) -> Option<SectionKind> {
    Some(match kind {
        BlendKind::Fillet { radius } => SectionKind::Fillet { radius: *radius },
        BlendKind::ChamferEqual { distance } => {
            SectionKind::Chamfer(ChamferSpec::Distances { a: *distance, b: *distance })
        }
        BlendKind::ChamferTwo { distance1, distance2 } => {
            SectionKind::Chamfer(ChamferSpec::Distances { a: *distance1, b: *distance2 })
        }
        BlendKind::ChamferAngle { distance, angle } => {
            SectionKind::Chamfer(ChamferSpec::DistanceAngle { distance: *distance, angle: *angle })
        }
        BlendKind::VariableFillet { .. } => return None,
    })
}

fn edge_geom(
    solid: &Solid,
    ce: &ChainedEdge,
    kind: &BlendKind,
    law: Option<&RadiusLaw>,
    tol: &Tolerance,
) -> Result<EdgeGeom, BlendError> {
    let sides = edge_sides(solid, ce.edge.edge, ce.edge.first_face)?;
    let angle = edge_angle(solid, ce.edge.edge, &sides)?;
    if let Some(l) = law {
        return march_geom(solid, ce, &sides, angle.convex, SectionLaw::Variable(l), tol);
    }
    let sk = section_kind(kind).ok_or(BlendError::SpineLost)?;
    analytic_geom(solid, ce.edge.edge, &sides, angle.convex, sk, tol)?
        .ok_or_else(|| unsupported_pair(solid, &sides))
}
```

und in `blend` statt `let kind = section_kind(&request.kind)?;` und der `geoms`-Zeilen:

```rust
    let law = match &request.kind {
        BlendKind::VariableFillet { points } => Some(
            RadiusLaw::new(points).map_err(|e| BlendError::InvalidParameter(e.to_string()))?,
        ),
        _ => None,
    };
    let chained = select_edges(solid, request)?;
    let geoms = chained
        .iter()
        .map(|ce| edge_geom(solid, ce, &request.kind, law.as_ref(), tol))
        .collect::<Result<Vec<_>, _>>()?;
```

- [ ] **Step 7: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-math && cargo test -p planora-kernel`
Expected: PASS. Der Volumentest nutzt, dass Fritsch–Butland bei zwei Punkten linear ist (`m₀ = m₁ = d₀`); die Schätzung (Trapez über die Stationen) liegt dann innerhalb 3e-3.

- [ ] **Step 8: Prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p planora-feature -p planora-project`
Expected: grün; der Rundreise-Test aus Task 7 hat jetzt auch für die variable Rundung `Ok`.

```bash
git add crates/planora-math/src/radius.rs crates/planora-math/src/lib.rs crates/planora-math/tests/radius_law.rs crates/planora-kernel/src/blend/march.rs crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/src/blend/build.rs crates/planora-kernel/tests/blend_march.rs crates/planora-kernel/tests/blend_api.rs
git commit -m "kernel: variabler Radius über Radienspline und Bahn mit Hautfläche

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 15: Rundungen und Fasen an Freiformflächen und allen übrigen Flächenpaaren

**Files:**
- Modify: `crates/planora-kernel/src/blend/mod.rs` (`edge_geom`: ohne analytische Klasse → Bahn; `unsupported_pair` entfällt)
- Test: `crates/planora-kernel/tests/blend_freeform.rs`

**Interfaces:**
- Consumes: Task 14 (`march_geom`, `SectionLaw`), `ProfileLoop::nurbs_loop`, `NurbsCurve::{elliptic_arc, natural_cubic}`, `Segment::Nurbs { curve: Arc<NurbsCurve> }`.
- Produces: `edge_geom` nimmt für jede Kante ohne analytische Klasse die Bahn (konstanter Radius: `SectionLaw::Fillet`, Fase: `SectionLaw::Chamfer`). Meldungen der Bahn bei Freiformkörpern bekommen im Feature den Vorsatz (P11, schon in Task 7).

- [ ] **Step 1: Tests schreiben** (`crates/planora-kernel/tests/blend_freeform.rs`)

```rust
//! Rundungen und Fasen an Freiformflächen (Teilprojekt 8, Task 15).

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU};
use std::sync::Arc;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn req(e: Id<Edge>, kind: BlendKind) -> BlendRequest {
    BlendRequest { edges: vec![BlendEdge { edge: e, first_face: None }], kind, tangent_chain: false }
}

/// Extrudierte Ellipse (a = b: Freiform-Zylinder).
fn ellipse_prism(a: f64, b: f64, h: f64) -> Solid {
    let c = NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::new(a, 0.0, 0.0), DVec3::new(0.0, b, 0.0), 0.0, TAU).unwrap();
    k().extrude(&Profile { outer: ProfileLoop::nurbs_loop(c).unwrap(), holes: vec![] }, &Frame::WORLD, h)
        .unwrap()
}

#[test]
fn a_freeform_cylinder_top_edge_matches_its_analytic_twin() {
    let s = ellipse_prism(5.0, 5.0, 8.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let b = k().blend(&s, &req(e, BlendKind::Fillet { radius: 1.0 })).unwrap();
    validate(&b.solid).unwrap();
    let twin = k().make_cylinder(5.0, 8.0).unwrap();
    let et = edge_between(&twin, "Wand 1", "Deckel");
    let bt = k().blend(&twin, &req(et, BlendKind::Fillet { radius: 1.0 })).unwrap();
    assert!((b.volume_change - bt.volume_change).abs() < 2e-3 * bt.volume_change.abs(), "{} / {}", b.volume_change, bt.volume_change);
    // Die Berührkurve auf dem Deckel liegt auf dem Kreis r = 4.
    let deckel = b.solid.faces.iter().find(|(id, _)| face_name(&b.solid, *id) == "Deckel").unwrap().1;
    for he in &deckel.outer.half_edges {
        let edge = &b.solid.edges[he.edge];
        for i in 0..=16 {
            let p = edge.curve.point(edge.t0 + (edge.t1 - edge.t0) * i as f64 / 16.0);
            assert!(((p.x * p.x + p.y * p.y).sqrt() - 4.0).abs() < 1e-5, "{p:?}");
        }
    }
}

#[test]
fn ellipse_fillets_and_chamfers_give_valid_bodies() {
    let s = ellipse_prism(5.0, 3.0, 4.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    for kind in [BlendKind::Fillet { radius: 0.5 }, BlendKind::ChamferEqual { distance: 0.5 }] {
        let b = k().blend(&s, &req(e, kind.clone())).unwrap_or_else(|err| panic!("{kind:?}: {err}"));
        validate(&b.solid).unwrap();
        assert!(b.volume_change < 0.0);
    }
    // Determinismus: zweimal gleich.
    let x = k().blend(&s, &req(e, BlendKind::Fillet { radius: 0.5 })).unwrap();
    let y = k().blend(&s, &req(e, BlendKind::Fillet { radius: 0.5 })).unwrap();
    assert_eq!(x.volume_change.to_bits(), y.volume_change.to_bits());
}

#[test]
fn the_edge_between_a_spline_wall_and_a_plane_wall_rounds() {
    let spline = NurbsCurve::natural_cubic(
        &[DVec3::new(10.0, 0.0, 0.0), DVec3::new(12.0, 5.0, 0.0), DVec3::new(10.0, 10.0, 0.0)],
        &[0.0, 1.0, 2.0],
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(10.0, 0.0) },
                Segment::Nurbs { curve: Arc::new(spline) },
                Segment::Line { to: DVec2::new(0.0, 10.0) },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let e = edge_between(&s, "Wand 1", "Wand 2");
    let b = k().blend(&s, &req(e, BlendKind::Fillet { radius: 1.0 })).unwrap();
    validate(&b.solid).unwrap();
    assert!(b.volume_change < 0.0 && b.volume_change > -5.0 * 1.0, "{}", b.volume_change);
    let _ = PI;
}

#[test]
fn a_freeform_edge_with_an_oblique_end_is_named() {
    // Obere Kante der Spline-Wand: ihre Enden liegen an schrägen ebenen Wänden.
    let spline = NurbsCurve::natural_cubic(
        &[DVec3::new(10.0, 0.0, 0.0), DVec3::new(12.0, 5.0, 0.0), DVec3::new(10.0, 10.0, 0.0)],
        &[0.0, 1.0, 2.0],
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(10.0, 0.0) },
                Segment::Nurbs { curve: Arc::new(spline) },
                Segment::Line { to: DVec2::new(0.0, 10.0) },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let e = edge_between(&s, "Wand 2", "Deckel");
    assert!(matches!(
        k().blend(&s, &req(e, BlendKind::Fillet { radius: 0.5 })),
        Err(BlendError::Unsupported(m)) if m == planora_kernel::blend::march::OBLIQUE_END
    ));
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_freeform`
Expected: FAIL – Meldung „Die Verrundung zwischen Freiformfläche und Ebene … wird noch nicht unterstützt“.

- [ ] **Step 3: Weiche** – in `crates/planora-kernel/src/blend/mod.rs` `unsupported_pair` löschen (und `EdgeSides` aus `use topo::{…}` entfernen, sonst meldet clippy einen unbenutzten Import) und das Ende von `edge_geom` ersetzen durch:

```rust
    let sk = section_kind(kind).ok_or(BlendError::SpineLost)?;
    if let Some(g) = analytic_geom(solid, ce.edge.edge, &sides, angle.convex, sk, tol)? {
        return Ok(g);
    }
    let law = match sk {
        SectionKind::Fillet { radius } => SectionLaw::Fillet(radius),
        SectionKind::Chamfer(spec) => SectionLaw::Chamfer(spec),
    };
    march_geom(solid, ce, &sides, angle.convex, law, tol)
```

- [ ] **Step 4: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_freeform && cargo test -p planora-kernel`
Expected: PASS. Die Kante zwischen Spline- und Ebenenwand ist senkrecht, ihre Abschlüsse liegen in Deckel und Boden (senkrechte Ebenen, P5). Dauert ein Test im Debug-Build über 30 s, die Ursache (Stationen, Fußpunktsuche) im Bericht nennen und nicht kürzen.

- [ ] **Step 5: Prüfen und committen**

Run: `cargo fmt && cargo clippy -p planora-kernel --all-targets -- -D warnings`

```bash
git add crates/planora-kernel/src/blend/mod.rs crates/planora-kernel/tests/blend_freeform.rs
git commit -m "kernel: Rundungen und Fasen an Freiformflächen über die Bahn

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 16: Kern-Gesamttests Stufe 3 (Integral, Zwillinge, Zufallsreihe)

**Files:**
- Create: `crates/planora-mesh/tests/blend_stage3.rs`

**Interfaces:**
- Consumes: `tests/blend_common`, `planora_math::RadiusLaw`, `boolean::{solid_contains, solid_faces, Location}`.
- Produces: nur Tests. Referenzen: variabler Radius `∫ (1 − π/4) r(s)² ds` (Simpson, 4000 Schritte); Ellipse mit Steiner `−A·r²·(U − 2π·x̄)` (Umfang `U` aus 100 000 Sehnen); Freiform-Zylinder gegen den analytischen Zwilling.

- [ ] **Step 1: Tests schreiben** (`crates/planora-mesh/tests/blend_stage3.rs`)

```rust
//! Rundungen und Fasen, Stufe 3: variabler Radius gegen numerisches Integral, Freiformkanten
//! gegen Zwillinge und Steiner-Formel, Zufallsreihe (Teilprojekt 8, Task 16).

mod blend_common;

use blend_common::*;
use planora_kernel::boolean::{Location, solid_contains, solid_faces};
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec3, Frame, PI, RadiusLaw, TAU};

const CHORD: f64 = 1e-3;
const REL: f64 = 5e-3;
const A: f64 = 1.0 - PI / 4.0;

fn ellipse_prism(a: f64, b: f64, h: f64) -> Solid {
    let c = NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::new(a, 0.0, 0.0), DVec3::new(0.0, b, 0.0), 0.0, TAU).unwrap();
    k().extrude(&Profile { outer: ProfileLoop::nurbs_loop(c).unwrap(), holes: vec![] }, &Frame::WORLD, h)
        .unwrap()
}

/// ∫₀ᴸ (1 − π/4)·r(s/L)² ds nach Simpson.
fn spandrel_integral(law: &RadiusLaw, length: f64) -> f64 {
    let n = 4000;
    let h = length / n as f64;
    let f = |s: f64| A * law.radius(s / length).powi(2);
    let mut sum = f(0.0) + f(length);
    for i in 1..n {
        sum += f(i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

#[test]
fn a_variable_radius_matches_its_integral() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let points = vec![(0.0, 1.0), (0.4, 2.0), (0.7, 1.2), (1.0, 1.6)];
    let law = RadiusLaw::new(&points).unwrap();
    let b = blend_edges(&s, &[e], BlendKind::VariableFillet { points }, false).unwrap();
    assert_rel(delta_volume(&s, &b.solid, CHORD), -spandrel_integral(&law, 10.0), REL, "variabel");
    assert!(plausible(&s, &b, 0.05));
}

#[test]
fn a_freeform_cylinder_rounds_like_its_twin() {
    let s = ellipse_prism(5.0, 5.0, 8.0);
    let twin = k().make_cylinder(5.0, 8.0).unwrap();
    for kind in [BlendKind::Fillet { radius: 1.0 }, BlendKind::ChamferEqual { distance: 1.0 }] {
        let b = blend_edges(&s, &[edge_between(&s, "Wand 1", "Deckel")], kind.clone(), false).unwrap();
        let bt = blend_edges(&twin, &[edge_between(&twin, "Wand 1", "Deckel")], kind.clone(), false).unwrap();
        let (d, dt) = (delta_volume(&s, &b.solid, CHORD), delta_volume(&twin, &bt.solid, CHORD));
        assert_rel(d, dt, 2e-3, &format!("Zwilling {kind:?}"));
    }
}

#[test]
fn an_ellipse_top_fillet_follows_steiner() {
    let (a, b, r) = (5.0, 3.0, 0.5);
    let s = ellipse_prism(a, b, 4.0);
    let res = blend_edges(&s, &[edge_between(&s, "Wand 1", "Deckel")], BlendKind::Fillet { radius: r }, false).unwrap();
    let n = 100_000;
    let perimeter: f64 = (0..n)
        .map(|i| {
            let (t0, t1) = (TAU * i as f64 / n as f64, TAU * (i + 1) as f64 / n as f64);
            let p = |t: f64| DVec3::new(a * t.cos(), b * t.sin(), 0.0);
            p(t0).distance(p(t1))
        })
        .sum();
    let offset = r * (5.0 / 6.0 - PI / 4.0) / A;
    let want = -A * r * r * (perimeter - TAU * offset);
    assert_rel(delta_volume(&s, &res.solid, CHORD), want, REL, "Ellipse");
    let faces = solid_faces(&res.solid);
    assert_eq!(solid_contains(&faces, DVec3::new(0.0, 0.0, 2.0), &tol()).unwrap(), Location::Inside);
    assert_eq!(solid_contains(&faces, DVec3::new(a - 0.05, 0.0, 3.95), &tol()).unwrap(), Location::Outside);
}

/// Zufällige Zylinder (Deckel- und Bodenkante, Rundung und Fase) gegen Pappus und gegen den
/// Freiform-Zwilling.
fn random_cylinders(cases: usize, seed: u64) {
    let mut rng = Lcg(seed);
    for case in 0..cases {
        let (big, h) = (rng.range(2.0, 8.0), rng.range(2.0, 8.0));
        let r = rng.range(0.05, 0.45) * big.min(h);
        let top = rng.next() < 0.5;
        let fillet = rng.next() < 0.5;
        let kind = if fillet { BlendKind::Fillet { radius: r } } else { BlendKind::ChamferEqual { distance: r } };
        let side = if top { "Deckel" } else { "Boden" };
        let s = k().make_cylinder(big, h).unwrap();
        let b = blend_edges(&s, &[edge_between(&s, "Wand 1", side)], kind.clone(), false)
            .unwrap_or_else(|e| panic!("Fall {case}: {e}"));
        let want = if fillet {
            let offset = r * (5.0 / 6.0 - PI / 4.0) / A;
            -A * r * r * TAU * (big - offset)
        } else {
            -0.5 * r * r * TAU * (big - r / 3.0)
        };
        assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, &format!("Fall {case}"));
        let n = ellipse_prism(big, big, h);
        let bn = blend_edges(&n, &[edge_between(&n, "Wand 1", side)], kind, false)
            .unwrap_or_else(|e| panic!("Fall {case} (Freiform): {e}"));
        assert_rel(delta_volume(&n, &bn.solid, CHORD), want, REL, &format!("Fall {case} Freiform"));
    }
}

#[test]
fn seeded_cylinders_small() {
    random_cylinders(3, 0x5eed_0101);
}

#[test]
#[ignore = "100 zufällige Zylinder mit Freiform-Zwilling, nur mit --release --ignored (etwa 2 min)"]
fn seeded_cylinders_100() {
    random_cylinders(100, 0x5eed_0102);
}
```

- [ ] **Step 2: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-mesh --test blend_stage3`
Expected: PASS (5 Tests, einer ignoriert), Debug-Laufzeit unter 30 s; danach `cargo test -p planora-mesh --release --test blend_stage3 -- --ignored` und die Laufzeit im Bericht nennen. Jede Abweichung ist ein Befund (nicht die Schranke erweitern).

- [ ] **Step 3: Prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings`

```bash
git add crates/planora-mesh/tests/blend_stage3.rs
git commit -m "mesh: Gesamttests Stufe 3 (variabler Radius, Freiformkanten, Zufallsreihe)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 17: Eckenfälle lockern (schräge Ecken aus drei Ebenen) und Grenzen benennen

**Files:**
- Modify: `crates/planora-kernel/src/blend/corner.rs` (Rechtwinkligkeit entfällt; Hautflächen an Ecken benannt abgelehnt; Eckvolumen allgemein)
- Modify: `crates/planora-kernel/tests/blend_corner.rs` (neuer Test)
- Test: `crates/planora-mesh/tests/blend_corners_general.rs`

**Interfaces:**
- Consumes: Task 11, Task 14 (`EdgeGeom::analytic`).
- Produces: `corner_three` nimmt beliebige drei Ebenen (`ball_center` mit Determinante, Pole außerhalb geprüft); `miter_pair` und `corner_three` lehnen Hautflächen (`analytic == false`) mit „Ecken an Freiformflächen oder mit variablem Radius werden nicht unterstützt“ ab (bewusst nicht gelockert: der Schnitt einer Hautfläche mit der Gehrungsebene wäre ein numerischer Schnitt ohne sichere Gegenprobe). `ORTHOGONAL_TOL` entfällt. Eckvolumen: Würfel-minus-Kugelachtel-Formel skaliert mit dem Volumen des Spats der drei Normalen (`r³ · (1/|det| − π/6)`; bei rechtwinkligen Ecken unverändert).

- [ ] **Step 1: Tests schreiben** – in `crates/planora-kernel/tests/blend_corner.rs` ergänzen:

```rust
#[test]
fn oblique_corners_of_three_planes_and_skin_corners() {
    let tri = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0), DVec2::new(5.0, 75f64.sqrt())])
                    .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            6.0,
        )
        .unwrap();
    let edges = [
        edge_between(&tri, "Deckel", "Wand 1"),
        edge_between(&tri, "Deckel", "Wand 2"),
        edge_between(&tri, "Wand 1", "Wand 2"),
    ];
    let r = blend(&tri, &edges, BlendKind::Fillet { radius: 0.5 }).unwrap();
    validate(&r.solid).unwrap();
    assert!(names(&r.solid).contains(&"Ecke 1".to_string()));
    // Variabler Radius an einer Ecke: benannt abgelehnt.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let two = [edge_between(&b, "Deckel", "Wand 1"), edge_between(&b, "Deckel", "Wand 2")];
    assert!(matches!(
        blend(&b, &two, BlendKind::VariableFillet { points: vec![(0.0, 1.0), (1.0, 1.0)] }),
        Err(BlendError::Corner(m)) if m.contains("variablem Radius")
    ));
}
```

(`crates/planora-mesh/tests/blend_corners_general.rs`)

```rust
//! Schräge Ecken: gleichseitiges Prisma mit allen neun Kanten gerundet gegen Steiner
//! (Teilprojekt 8, Task 17).

mod blend_common;

use blend_common::*;
use planora_kernel::*;
use planora_math::{DVec2, PI};

#[test]
fn a_fully_rounded_triangular_prism_follows_steiner() {
    let (side, h, r) = (10.0, 6.0, 0.8);
    let height = side * 3f64.sqrt() / 2.0;
    let s = extrude_polygon(&[DVec2::new(0.0, 0.0), DVec2::new(side, 0.0), DVec2::new(side / 2.0, height)], h);
    let edges: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
    let b = blend_edges(&s, &edges, BlendKind::Fillet { radius: r }, false).unwrap();
    // Inneres Prisma: Dreieck um r nach innen versetzt (Seite − 2√3·r), Höhe − 2r.
    let a1 = side - 2.0 * 3f64.sqrt() * r;
    let h1 = h - 2.0 * r;
    let area = 3f64.sqrt() / 4.0 * a1 * a1;
    let v = area * h1;
    let surface = 2.0 * area + 3.0 * a1 * h1;
    // Kanten: 3 senkrechte (Innenwinkel 60°, Außenwinkel 2π/3), 6 waagerechte (90°, π/2).
    let edge_term = 0.5 * (3.0 * h1 * (2.0 * PI / 3.0) + 6.0 * a1 * (PI / 2.0));
    let rounded = v + r * surface + r * r * edge_term + 4.0 / 3.0 * PI * r * r * r;
    let original = 3f64.sqrt() / 4.0 * side * side * h;
    assert_rel(delta_volume(&s, &b.solid, 1e-3), rounded - original, 5e-3, "Steiner-Prisma");
    assert!(plausible(&s, &b, 0.05));
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_corner && cargo test -p planora-mesh --test blend_corners_general`
Expected: FAIL – „Nur rechtwinklige Ecken bekommen bisher ein Kugelstück“ bzw. eine andere Meldung beim variablen Radius.

- [ ] **Step 3: Lockern** – in `crates/planora-kernel/src/blend/corner.rs`:

`ORTHOGONAL_TOL` und die Prüfung `if (0..3).any(|a| n[a].dot(n[(a + 1) % 3]).abs() > ORTHOGONAL_TOL) { … }` löschen. In `miter_pair` direkt nach `let (gi, gj) = (&geoms[i], &geoms[j]);` einfügen:

```rust
    if !gi.analytic || !gj.analytic {
        return Err(corner(SKIN_CORNER));
    }
```

und in `corner_three` in der Schleife `for &(i, _) in list` direkt nach `let g = &geoms[i];`:

```rust
        if !g.analytic {
            return Err(corner(SKIN_CORNER));
        }
```

mit der Konstante

```rust
/// Meldung für Ecken an Hautflächen (Task 17: bewusst nicht gelockert).
const SKIN_CORNER: &str = "Ecken an Freiformflächen oder mit variablem Radius werden nicht unterstützt";
```

und die Eckvolumen-Zeile ersetzen durch:

```rust
    // Spat der drei Einheitsnormalen: bei rechtwinkligen Ecken 1. Das Gebiet zwischen Ecke und
    // Kugel ist ein schiefer Würfel mit Volumen r³/|det| minus das Kugelstück; das Kugelstück
    // ist hier mit dem Achtel angenähert (Schätzung für die Volumenprobe, P2).
    let det = n[0].dot(n[1].cross(n[2])).abs();
    let volume = -(1.0 / det - PI / 6.0) * r * r * r;
```

Die Meldung in `blend_corner.rs`, die vorher „rechtwinklig“ erwartete, gibt es nicht mehr; der Test `three_fillets_at_a_corner_get_a_sphere_patch` bleibt unverändert.

- [ ] **Step 4: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-kernel --test blend_corner && cargo test -p planora-mesh --test blend_corners_general --test blend_stage2`
Expected: PASS. Das Steiner-Prisma prüft Kugelecken an Ecken mit 60°- und 90°-Kanten zugleich.

- [ ] **Step 5: Stufenabschluss prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check && git diff --stat f2f748d -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs`
Expected: alles grün, letzte Ausgabe leer (Abschluss Stufe 3); danach Review der Stufe 3.

```bash
git add crates/planora-kernel/src/blend/corner.rs crates/planora-kernel/tests/blend_corner.rs crates/planora-mesh/tests/blend_corners_general.rs
git commit -m "kernel: Kugelecken an schrägen Ecken, Hautflächen an Ecken benannt abgelehnt

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Stufe 4 – Datei, Eigenschaftenfenster, Dokumentation, Gesamtprüfung, Handprüfung

### Task 18: Dateiformat 2 (Golden-Datei, Format 1 bleibt, feindliche Dateien)

**Files:**
- Modify: `crates/planora-project/src/convert.rs:17` (`FORMAT_VERSION = 2`, `CONTAINER_FORMAT`, `required_format`, Schreiben/Lesen), `crates/planora-project/src/docstore.rs` (`to_file`, `write_autosave` schreiben `CONTAINER_FORMAT`), `crates/planora-project/src/lib.rs:10`
- Modify (mechanisch, P7): `crates/planora-project/tests/docstore.rs:282-308` (Version „zu neu“ von 2 auf 3)
- Create: `crates/planora-project/tests/format2.rs`, `crates/planora-project/tests/data/format2.json` (erzeugt mit `PLANORA_UPDATE_GOLDEN=1`)

**Interfaces:**
- Consumes: Task 7 (`KindDto::Verrunden`, `BlendArtDto`, Umwandlung).
- Produces:
  ```rust
  pub const FORMAT_VERSION: u32 = 2;          // höchste lesbare Version
  pub const CONTAINER_FORMAT: u32 = 1;        // dokument.json, .autosave.json
  pub fn required_format(s: &Snapshot) -> u32;  // 2 mit Verrunden, sonst 1
  ```

- [ ] **Step 1: Tests schreiben** (`crates/planora-project/tests/format2.rs`)

```rust
//! Dateiformat 2: Golden-Datei mit Rundung, variabler Rundung und Fasen; Format 1 bleibt;
//! feindliche Dateien (Teilprojekt 8, Task 18).

use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_math::{DVec3, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_project::{FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format};

fn blend_document() -> Document {
    let mut d = Document::new();
    let arts = [
        BlendArt::Rundung { radius: 1.5 },
        BlendArt::RundungVariabel { punkte: vec![(0.0, 1.0), (50.0, 1.5), (100.0, 1.2)] },
        BlendArt::FaseZwei { abstand1: 1.0, abstand2: 2.0 },
        BlendArt::FaseWinkel { abstand: 1.0, winkel: 30.0 },
    ];
    for (i, art) in arts.into_iter().enumerate() {
        let f = d
            .add_feature(
                format!("Quader {}", i + 1),
                FeatureKind::Box { size: DVec3::splat(10.0), origin: DVec3::new(20.0 * i as f64, 0.0, 0.0) },
            )
            .unwrap();
        let body = BodyRef { feature: f, index: 0 };
        let s = d.body(body).unwrap();
        let e = s
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
                n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
            })
            .unwrap();
        let r = EdgeRef::from_edge(s, e, &Tolerance::default()).unwrap();
        let name = format!("{} {}", art.label(), i + 1);
        d.add_feature(name, FeatureKind::Verrunden { body, edges: vec![r], art, tangentenkette: i % 2 == 0 })
            .unwrap();
    }
    d
}

fn volumes(d: &Document) -> Vec<f64> {
    d.visible_bodies()
        .iter()
        .map(|v| mass_properties(&tessellate(v.solid, &TessellationOptions { chord_tolerance: 1e-3 }).unwrap()).volume)
        .collect()
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

#[test]
fn the_golden_file_pins_format_2() {
    let d = blend_document();
    assert!(d.features().iter().all(|f| f.status == FeatureStatus::Ok), "alle Features berechenbar");
    let text = encode_version(&d.snapshot()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/format2.json");
    if std::env::var("PLANORA_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, &text).unwrap();
    }
    let golden = std::fs::read_to_string(&path)
        .expect("tests/data/format2.json fehlt – mit PLANORA_UPDATE_GOLDEN=1 erzeugen");
    assert_eq!(text, golden.trim_end_matches('\n'));
    assert!(golden.contains("\"format\": 2"));
    let back = Document::from_snapshot(decode_version(&golden, "format2.json").unwrap(), 0.05).unwrap();
    back.check_invariants().unwrap();
    let (a, b) = (volumes(&d), volumes(&back));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() < 1e-9 * x.abs(), "{x} / {y}");
    }
}

#[test]
fn documents_without_blends_still_write_format_1() {
    let mut d = Document::new();
    d.add_feature("Quader 1", FeatureKind::Box { size: DVec3::ONE, origin: DVec3::ZERO }).unwrap();
    assert_eq!(required_format(&d.snapshot()), 1);
    assert!(encode_version(&d.snapshot()).unwrap().contains("\"format\": 1"));
    assert_eq!(required_format(&blend_document().snapshot()), 2);
    assert_eq!(FORMAT_VERSION, 2);
}

#[test]
fn a_format_1_file_with_a_blend_is_corrupt_and_format_3_is_too_new() {
    let good = encode_version(&blend_document().snapshot()).unwrap();
    match bad(&good.replacen("\"format\": 2", "\"format\": 1", 1)) {
        ProjectError::Corrupt { .. } => {}
        other => panic!("{other:?}"),
    }
    let e = bad(&good.replacen("\"format\": 2", "\"format\": 1", 1)).to_string();
    assert!(e.contains("Dateiformat 2"), "{e}");
    assert!(matches!(
        bad(&good.replacen("\"format\": 2", "\"format\": 3", 1)),
        ProjectError::TooNew { found: 3, .. }
    ));
}

#[test]
fn hostile_blend_files_are_rejected_or_named() {
    let good = encode_version(&blend_document().snapshot()).unwrap();
    // Ändert die Art der Verrundung mit dem Typ `typ` im JSON-Baum.
    let edit = |typ: &str, f: &dyn Fn(&mut serde_json::Value)| -> String {
        let mut v: serde_json::Value = serde_json::from_str(&good).unwrap();
        let features = v["features"].as_array_mut().unwrap();
        let blend = features
            .iter_mut()
            .find(|x| x["art"]["typ"] == "verrunden" && x["art"]["art"]["typ"] == typ)
            .unwrap();
        f(&mut blend["art"]);
        serde_json::to_string_pretty(&v).unwrap()
    };
    // Beim Laden abgelehnt: riesige Zahl, NaN (kein gültiges JSON), null, unbekannte Art,
    // leere Kantenliste.
    for text in [
        edit("rundung", &|a| a["art"]["radius"] = serde_json::json!(1e300)),
        good.replacen("\"radius\": 1.5", "\"radius\": NaN", 1),
        edit("fase_zwei", &|a| a["art"]["abstand1"] = serde_json::Value::Null),
        edit("fase_zwei", &|a| a["art"]["typ"] = serde_json::json!("fase_drei")),
        edit("rundung", &|a| a["kanten"] = serde_json::json!([])),
    ] {
        assert_ne!(text, good, "Änderung muss greifen");
        assert!(matches!(bad(&text), ProjectError::Corrupt { .. }), "{text}");
    }
    // Geladen, aber beim Berechnen benannt gemeldet: negativer Radius, Winkel 0, unsortierte
    // Punkte.
    for text in [
        edit("rundung", &|a| a["art"]["radius"] = serde_json::json!(-1.5)),
        edit("fase_winkel", &|a| a["art"]["winkel"] = serde_json::json!(0.0)),
        edit("rundung_variabel", &|a| a["art"]["punkte"][1][0] = serde_json::json!(150.0)),
    ] {
        let d = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
        d.check_invariants().unwrap();
        assert!(
            d.features().iter().any(|f| matches!(f.status, FeatureStatus::Error(_))),
            "ein Feature meldet den Fehler"
        );
    }
}
```

(`planora-project` hat `serde_json` als Abhängigkeit und `planora-mesh` als Entwicklungsabhängigkeit; `planora-kernel` ist Abhängigkeit.)

In `crates/planora-project/tests/docstore.rs` im Test `newer_files_with_changed_structure_report_too_new` die beiden Texte `"format":2` durch `"format":3` und die beiden Muster `found: 2` durch `found: 3` ersetzen und darüber kommentieren:

```rust
    // Seit Teilprojekt 8 ist Format 2 gültig (P7); „zu neu“ ist jetzt 3.
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-project --test format2`
Expected: FAIL – `cannot find function required_format`.

- [ ] **Step 3: Format umstellen** – in `crates/planora-project/src/convert.rs`:

```rust
/// Höchste lesbare Formatnummer der Versionsdateien (P7).
pub const FORMAT_VERSION: u32 = 2;
/// Formatnummer der Rahmendateien `dokument.json` und `.autosave.json`; ihre Struktur ist seit
/// Format 1 unverändert, ältere Programme öffnen das Projekt deshalb weiter (P7).
pub const CONTAINER_FORMAT: u32 = 1;

/// Kleinste Formatnummer, die den Stand darstellt: 2 mit Rundungen oder Fasen, sonst 1 (P7).
pub fn required_format(s: &Snapshot) -> u32 {
    if s.features.iter().any(|f| matches!(f.kind, FeatureKind::Verrunden { .. })) {
        2
    } else {
        1
    }
}
```

in `encode_version` `format: FORMAT_VERSION,` durch `format: required_format(s),` ersetzen und in `decode_version` nach dem Lesen von `file`:

```rust
    if file.format < 2 && file.features.iter().any(|f| matches!(f.art, KindDto::Verrunden { .. })) {
        return Err(ProjectError::corrupt(
            origin,
            "Rundungen und Fasen gibt es erst ab Dateiformat 2",
        ));
    }
```

In `crates/planora-project/src/docstore.rs` `use crate::convert::{CONTAINER_FORMAT, FORMAT_VERSION, decode_version, encode_version};` und in `to_file` sowie `write_autosave` `format: FORMAT_VERSION,` durch `format: CONTAINER_FORMAT,` ersetzen (`check_head` vergleicht weiter mit `FORMAT_VERSION`). In `crates/planora-project/src/lib.rs` `pub use convert::{CONTAINER_FORMAT, FORMAT_VERSION, decode_version, encode_version, required_format};`.

- [ ] **Step 4: Golden-Datei erzeugen und prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && PLANORA_UPDATE_GOLDEN=1 cargo test -p planora-project --test format2 the_golden_file_pins_format_2 && cargo test -p planora-project`
Expected: PASS; `tests/data/format2.json` ist neu, `format1.json` und `format1_voll.json` sind unverändert (`git status --short crates/planora-project/tests/data` zeigt nur `format2.json`). Die Datei von Hand lesen: deutsche Feldnamen `koerper`, `kanten`, `art`, `tangentenkette`, `punkte`, `winkel`.

- [ ] **Step 5: Prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

```bash
git add crates/planora-project/src/convert.rs crates/planora-project/src/docstore.rs crates/planora-project/src/lib.rs crates/planora-project/tests/docstore.rs crates/planora-project/tests/format2.rs crates/planora-project/tests/data/format2.json
git commit -m "project: Dateiformat 2 für Rundungen und Fasen, Format 1 bleibt

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 19: Eigenschaftenfenster und Feature-Dialog für Verrunden (Art, Punkte, Tangentenkette)

**Files:**
- Modify: `crates/planora-app/src/blend_dialog.rs` (`ArtChoice::RundungVariabel`, Punktliste), `crates/planora-app/src/feature_dialog.rs` (Zahlenfelder mit `String`-Bezeichnungen, Art wechseln, Punkte, Tangentenkette, Kantenzahl)
- Test: `crates/planora-app/tests/blend_edit.rs`

**Interfaces:**
- Consumes: `BlendArt`, `FeatureDialog::{new, apply, set_text, reload}`, `STALE_MESSAGE`, `parse_number`, `format_number`.
- Produces:
  ```rust
  // blend_dialog
  ArtChoice::RundungVariabel
  impl ArtChoice { pub fn of(art: &BlendArt) -> ArtChoice; pub fn convert(self, art: &BlendArt) -> BlendArt; }
  impl BlendDialog { pub fn variable() -> Self; pub points: Vec<(String, String)>; pub fn add_point(&mut self); pub fn remove_point(&mut self, i: usize); }
  // feature_dialog
  impl FeatureDialog { pub fn set_blend_art(&mut self, choice: ArtChoice); pub fn add_point(&mut self);
                       pub fn remove_point(&mut self, i: usize); pub fn set_tangent_chain(&mut self, on: bool);
                       pub fn labels(&self) -> Vec<String>; }
  ```
  Regeln: Ein Wechsel der Art übernimmt das erste Maß (Radius ↔ Abstand); variabel aus konstant: zwei Punkte (0 %, r) und (100 %, r); konstant aus variabel: Radius des ersten Punkts. „Punkt hinzufügen“ setzt einen Punkt in die Mitte des größten Abstands; „Punkt entfernen“ lässt mindestens zwei übrig. Die Zahlenfelder heißen „Position k“ (in %) und „Radius k“.

- [ ] **Step 1: Tests schreiben** (`crates/planora-app/tests/blend_edit.rs`)

```rust
//! Verrunden nachträglich ändern (Teilprojekt 8, Task 19).

use planora_app::blend_dialog::{ArtChoice, BlendDialog};
use planora_app::feature_dialog::{FeatureDialog, STALE_MESSAGE};
use planora_app::state::*;
use planora_feature::{BlendArt, FeatureId, FeatureKind, FeatureStatus};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::DVec3;
use planora_render::{EdgeRef, Pick};

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id).iter().map(|u| face_name(s, u.face)).collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn with_fillet() -> (AppState, FeatureId) {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    let id = s.add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true).unwrap();
    (s, id)
}

fn art(s: &AppState, id: FeatureId) -> BlendArt {
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Verrunden { art, .. } => art.clone(),
        k => panic!("{k:?}"),
    }
}

#[test]
fn radius_and_art_change_through_the_dialog() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    assert_eq!(d.labels(), ["Radius"]);
    assert!(d.set_text("Radius", "1,5"));
    d.apply(&mut s).unwrap();
    assert_eq!(art(&s, id), BlendArt::Rundung { radius: 1.5 });
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_blend_art(ArtChoice::FaseWinkel);
    assert_eq!(d.labels(), ["Abstand", "Winkel"]);
    d.set_text("Winkel", "30");
    d.apply(&mut s).unwrap();
    assert_eq!(art(&s, id), BlendArt::FaseWinkel { abstand: 1.5, winkel: 30.0 });
    assert_eq!(s.document.feature(id).unwrap().status, FeatureStatus::Ok);
    assert_eq!(s.document.feature(id).unwrap().name, "Rundung 1", "der Name bleibt");
}

#[test]
fn variable_points_are_added_removed_and_applied() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_blend_art(ArtChoice::RundungVariabel);
    assert_eq!(d.labels(), ["Position 1", "Radius 1", "Position 2", "Radius 2"]);
    d.add_point();
    assert_eq!(d.labels().len(), 6);
    d.set_text("Radius 2", "1,2");
    d.apply(&mut s).unwrap();
    assert_eq!(
        art(&s, id),
        BlendArt::RundungVariabel { punkte: vec![(0.0, 2.0), (50.0, 1.2), (100.0, 2.0)] }
    );
    assert_eq!(s.document.feature(id).unwrap().status, FeatureStatus::Ok);
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.remove_point(1);
    d.remove_point(0);
    assert_eq!(d.labels().len(), 4, "mindestens zwei Punkte bleiben");
}

#[test]
fn the_tangent_chain_and_stale_dialogs() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_tangent_chain(false);
    let stale = FeatureDialog::new(&s, id).unwrap();
    d.apply(&mut s).unwrap();
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Verrunden { tangentenkette, .. } => assert!(!tangentenkette),
        k => panic!("{k:?}"),
    }
    assert_eq!(stale.apply(&mut s), Err(STALE_MESSAGE.to_string()));
}

#[test]
fn the_create_dialog_offers_a_variable_radius() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    let mut d = BlendDialog::variable();
    assert_eq!(d.art, ArtChoice::RundungVariabel);
    d.points = vec![("0".into(), "1".into()), ("100".into(), "2".into())];
    d.add_point();
    assert_eq!(d.points.len(), 3);
    d.remove_point(0);
    d.remove_point(0);
    assert_eq!(d.points.len(), 2);
    d.points = vec![("0".into(), "1".into()), ("100".into(), "2".into())];
    assert!(!d.execute(&mut s));
    let f = s.document.features().last().unwrap();
    assert!(matches!(&f.kind, FeatureKind::Verrunden { art: BlendArt::RundungVariabel { .. }, .. }));
    assert_eq!(f.status, FeatureStatus::Ok);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app --test blend_edit`
Expected: FAIL – `no function set_blend_art`.

- [ ] **Step 3: Art und Punkte im Erzeugen-Fenster** – in `crates/planora-app/src/blend_dialog.rs`:

`ArtChoice` um `RundungVariabel` erweitern (`label`: „Rundung mit variablem Radius“, `fields`: `("Radius", None)` – die Felder dieser Art kommen aus der Punktliste) und ergänzen:

```rust
impl ArtChoice {
    /// Art einer bestehenden Verrundung.
    pub fn of(art: &BlendArt) -> ArtChoice {
        match art {
            BlendArt::Rundung { .. } => ArtChoice::Rundung,
            BlendArt::RundungVariabel { .. } => ArtChoice::RundungVariabel,
            BlendArt::FaseGleich { .. } => ArtChoice::FaseGleich,
            BlendArt::FaseZwei { .. } => ArtChoice::FaseZwei,
            BlendArt::FaseWinkel { .. } => ArtChoice::FaseWinkel,
        }
    }

    /// `art` in diese Art umwandeln: das erste Maß bleibt, fehlende Werte bekommen Vorgaben.
    pub fn convert(self, art: &BlendArt) -> BlendArt {
        let first = match art {
            BlendArt::Rundung { radius } => *radius,
            BlendArt::RundungVariabel { punkte } => punkte.first().map_or(1.0, |p| p.1),
            BlendArt::FaseGleich { abstand } | BlendArt::FaseWinkel { abstand, .. } => *abstand,
            BlendArt::FaseZwei { abstand1, .. } => *abstand1,
        };
        match self {
            ArtChoice::Rundung => BlendArt::Rundung { radius: first },
            ArtChoice::RundungVariabel => BlendArt::RundungVariabel { punkte: vec![(0.0, first), (100.0, first)] },
            ArtChoice::FaseGleich => BlendArt::FaseGleich { abstand: first },
            ArtChoice::FaseZwei => BlendArt::FaseZwei { abstand1: first, abstand2: first },
            ArtChoice::FaseWinkel => BlendArt::FaseWinkel { abstand: first, winkel: 45.0 },
        }
    }
}

/// Neuer Punkt in der Mitte des größten Abstands (Position, Radius als Mittel der Nachbarn).
pub fn insert_point(points: &mut Vec<(f64, f64)>) {
    if points.len() < 2 {
        return;
    }
    let k = (0..points.len() - 1)
        .max_by(|&a, &b| {
            (points[a + 1].0 - points[a].0).total_cmp(&(points[b + 1].0 - points[b].0))
        })
        .unwrap_or(0);
    let (p, q) = (points[k], points[k + 1]);
    points.insert(k + 1, (0.5 * (p.0 + q.0), 0.5 * (p.1 + q.1)));
}
```

In `BlendDialog` das Feld `pub points: Vec<(String, String)>` (Position in %, Radius) ergänzen (in `fillet`/`chamfer` leer), den Konstruktor

```rust
    pub fn variable() -> Self {
        BlendDialog {
            art: ArtChoice::RundungVariabel,
            points: vec![("0".into(), "1".into()), ("100".into(), "1".into())],
            ..BlendDialog::fillet()
        }
    }

    /// Punkt in der Mitte des größten Abstands einfügen (ungültige Felder: ans Ende, 100 %).
    pub fn add_point(&mut self) {
        let parsed: Option<Vec<(f64, f64)>> = self
            .points
            .iter()
            .map(|(p, r)| Some((parse_number(p)?, parse_number(r)?)))
            .collect();
        match parsed {
            Some(mut pts) if pts.len() >= 2 => {
                insert_point(&mut pts);
                self.points = pts.iter().map(|(p, r)| (format_number(*p, 2), format_number(*r, 3))).collect();
            }
            _ => self.points.push(("100".into(), "1".into())),
        }
    }

    /// Punkt `i` entfernen; mindestens zwei bleiben.
    pub fn remove_point(&mut self, i: usize) {
        if self.points.len() > 2 && i < self.points.len() {
            self.points.remove(i);
        }
    }
```

(`use crate::sketch_view::format_number;` ergänzen) und in `art_value` vor dem `match self.art` den Fall

```rust
        if self.art == ArtChoice::RundungVariabel {
            let mut punkte = Vec::with_capacity(self.points.len());
            for (k, (p, r)) in self.points.iter().enumerate() {
                let pos = parse_number(p)
                    .filter(|v| (0.0..=100.0).contains(v))
                    .ok_or_else(|| format!("„Position {}“ muss zwischen 0 und 100 % liegen", k + 1))?;
                punkte.push((pos, size(&format!("Radius {}", k + 1), r)?));
            }
            return Ok(BlendArt::RundungVariabel { punkte });
        }
```

sowie im `match self.art` den Arm `ArtChoice::RundungVariabel => BlendArt::Rundung { radius: a }` ergänzen (wird wegen des frühen `return` nie erreicht; bewusst ohne `panic`). In `show` die Auswahl bei Rundungen auf `&[ArtChoice::Rundung, ArtChoice::RundungVariabel]` erweitern und für `RundungVariabel` statt des Zahlenrasters die Punktliste zeigen:

```rust
                if self.art == ArtChoice::RundungVariabel {
                    let mut remove = None;
                    egui::Grid::new("blend_points").num_columns(5).show(ui, |ui| {
                        for (k, (p, r)) in self.points.iter_mut().enumerate() {
                            ui.label(format!("Punkt {}", k + 1));
                            ui.add(egui::TextEdit::singleline(p).desired_width(60.0));
                            ui.label("%");
                            ui.add(egui::TextEdit::singleline(r).desired_width(60.0));
                            if ui.button("Entfernen").clicked() {
                                remove = Some(k);
                            }
                            ui.end_row();
                        }
                    });
                    if let Some(k) = remove {
                        self.remove_point(k);
                    }
                    if ui.button("Punkt hinzufügen").clicked() {
                        self.add_point();
                    }
                } else {
                    // bisheriges Zahlenraster (Radius bzw. Abstände)
                }
```

(das bisherige `egui::Grid::new("blend_numbers")` steht im `else`-Zweig). Direkt nach den Auswahlknöpfen füllt `show` eine leere Punktliste:

```rust
                if self.art == ArtChoice::RundungVariabel && self.points.is_empty() {
                    self.points = vec![("0".into(), self.first.clone()), ("100".into(), self.first.clone())];
                }
```

- [ ] **Step 4: Feature-Dialog** – in `crates/planora-app/src/feature_dialog.rs`:

Die Zahlenfelder bekommen `String`-Bezeichnungen: Feld `numbers: Vec<(String, String)>`, `numbers_of` liefert `Vec<(String, f64)>` (alle bisherigen Arme mit `.to_string()` bzw. `l.into()` an der Bezeichnung), `set_text` vergleicht `l == label`, das Raster zeigt `ui.label(label.as_str())` und `ui.label(unit(label))`. Der Arm für `RundungVariabel` in `numbers_of`:

```rust
            BlendArt::RundungVariabel { punkte } => punkte
                .iter()
                .enumerate()
                .flat_map(|(k, (p, r))| [(format!("Position {}", k + 1), *p), (format!("Radius {}", k + 1), *r)])
                .collect(),
```

`unit` ergänzen: Bezeichnungen, die mit „Position“ beginnen, haben die Einheit „%“. In `set_numbers` den Arm

```rust
            (BlendArt::RundungVariabel { punkte }, values) if values.len() == 2 * punkte.len() => {
                for (k, p) in punkte.iter_mut().enumerate() {
                    *p = (values[2 * k], values[2 * k + 1]);
                }
            }
```

In `edited_kind` gilt der Bereich „Betrag höchstens `MAX_COORD_MM`“ weiter; Positionen außerhalb 0–100 % meldet der Kernel beim Berechnen (Status `Error`).

Neue Methoden:

```rust
    /// Bezeichnungen der Zahlenfelder (für Tests).
    pub fn labels(&self) -> Vec<String> {
        self.numbers.iter().map(|(l, _)| l.clone()).collect()
    }

    fn renumber(&mut self) {
        self.numbers = numbers_of(&self.kind)
            .into_iter()
            .map(|(l, v)| (l, number_text(v)))
            .collect();
    }

    /// Übernimmt eingegebene Zahlen, soweit gültig, in die Kopie der Art (vor einem Umbau).
    fn absorb(&mut self) {
        if let Ok(kind) = self.edited_kind() {
            self.kind = kind;
        }
    }

    /// Art einer Verrundung wechseln (P: erstes Maß bleibt).
    pub fn set_blend_art(&mut self, choice: ArtChoice) {
        self.absorb();
        if let FeatureKind::Verrunden { art, .. } = &mut self.kind {
            *art = choice.convert(art);
        }
        self.renumber();
    }

    /// Punkt einer variablen Rundung einfügen (Mitte des größten Abstands).
    pub fn add_point(&mut self) {
        self.absorb();
        if let FeatureKind::Verrunden { art: BlendArt::RundungVariabel { punkte }, .. } = &mut self.kind {
            insert_point(punkte);
        }
        self.renumber();
    }

    /// Punkt `i` einer variablen Rundung entfernen; mindestens zwei bleiben.
    pub fn remove_point(&mut self, i: usize) {
        self.absorb();
        if let FeatureKind::Verrunden { art: BlendArt::RundungVariabel { punkte }, .. } = &mut self.kind
            && punkte.len() > 2
            && i < punkte.len()
        {
            punkte.remove(i);
        }
        self.renumber();
    }

    pub fn set_tangent_chain(&mut self, on: bool) {
        if let FeatureKind::Verrunden { tangentenkette, .. } = &mut self.kind {
            *tangentenkette = on;
        }
    }
```

(`use crate::blend_dialog::{ArtChoice, insert_point};`.) In `choices` den Arm für Verrunden ergänzen:

```rust
            FeatureKind::Verrunden { body, edges, art, tangentenkette } => {
                ui.label(format!("Körper: {}", state.body_ref_name(*body)));
                ui.label(format!("Kanten: {}", edges.len()));
                let current = ArtChoice::of(art);
                let choices: &[ArtChoice] = if art.is_chamfer() {
                    &[ArtChoice::FaseGleich, ArtChoice::FaseZwei, ArtChoice::FaseWinkel]
                } else {
                    &[ArtChoice::Rundung, ArtChoice::RundungVariabel]
                };
                for c in choices {
                    if ui.radio(current == *c, c.label()).clicked() && current != *c {
                        switch = Some(*c);
                    }
                }
                ui.checkbox(tangentenkette, "Tangentenkette");
                if matches!(art, BlendArt::RundungVariabel { .. }) && ui.button("Punkt hinzufügen").clicked() {
                    add = true;
                }
            }
```

mit den lokalen Merkern `let (mut switch, mut add) = (None, false);` vor dem `match` und danach

```rust
        if let Some(c) = switch {
            self.set_blend_art(c);
        }
        if add {
            self.add_point();
        }
```

(dazu wird `choices` zu `&mut self` mit den Merkern außerhalb des `match &mut self.kind`). Die Zeile `FeatureKind::Sketch(_) | FeatureKind::Revolve { .. } | FeatureKind::Boolean { .. } => Vec::new()` bleibt.

- [ ] **Step 5: Tests laufen lassen**

Run: `export PATH="$PATH:/c/Users/denni/.cargo/bin" && cargo test -p planora-app`
Expected: PASS, alle alten Dialogtests unverändert grün (`set_box_size`, `set_text` arbeiten weiter über die Bezeichnungen).

- [ ] **Step 6: Prüfen und committen**

Run: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings`

```bash
git add crates/planora-app/src/blend_dialog.rs crates/planora-app/src/feature_dialog.rs crates/planora-app/tests/blend_edit.rs
git commit -m "app: Verrunden nachträglich ändern (Art, Radienpunkte, Tangentenkette)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 20: Dokumentation und Gesamtprüfung

**Files:**
- Modify: `README.md` (neuer Abschnitt „Rundungen und Fasen“ nach „Freiformflächen“), `docs/superpowers/offene-punkte.md` (Kopf „nach Teilprojekt 1–8“, neuer Abschnitt „Teilprojekt 8 – Rundungen und Fasen“), `docs/superpowers/specs/2026-10-13-rundungen-und-fasen-design.md` (neuer §12 „Änderungen bei der Umsetzung“)

**Interfaces:**
- Consumes: die Berichte der Tasks 1–19 (Testzahlen, Laufzeiten der `--ignored`-Reihen, Befunde der Stufenreviews).
- Produces: Dokumentation in einfacher Sprache (Laiensprache, wenig Technik, kein Füllstoff), Spec §12 mit P1–P14 und allen Abweichungen.

- [ ] **Step 1: README** – Abschnitt einfügen (Inhalt, gekürzt nur wo Befunde es verlangen):

```markdown
## Rundungen und Fasen

Kanten eines Körpers lassen sich runden oder abschrägen (Menü „Bearbeiten“ → „Verrunden …“ bzw. „Fase …“). Im Fenster die Kante in der Ansicht anklicken; mit gedrückter Umschalttaste kommen weitere Kanten dazu, ein zweiter Klick nimmt eine Kante wieder heraus. Ist „Tangentenkette“ eingeschaltet, werden Kanten, die glatt anschließen (etwa rund um ein Langloch), automatisch mitgenommen.

- **Rundung:** fester Radius oder variabler Radius über Punkte entlang der Kante (Position in Prozent, Radius); dazwischen ändert sich der Radius glatt und ohne Überschwingen.
- **Fase:** gleicher Abstand auf beiden Flächen, zwei Abstände (Fläche 1 und Fläche 2) oder Abstand und Winkel.
- Treffen zwei gerundete Kanten an einer Ecke zusammen, laufen die Rundungen schräg ineinander; drei gerundete Kanten mit gleichem Radius an einer Ecke aus drei ebenen Flächen bekommen ein Kugelstück.
- Fälle, die das Programm nicht sicher rechnen kann, werden mit einer Meldung abgelehnt, zum Beispiel ein Radius, der größer ist als die Fläche daneben, oder Flächen, die fast glatt ineinander übergehen. Ein falscher Körper entsteht nicht.

Rundungen stehen wie alle Schritte in der Zeitleiste, lassen sich nachträglich ändern, unterdrücken und löschen. Dokumente mit Rundungen oder Fasen speichert das Programm im Dateiformat 2; ältere Programmversionen melden dann „zu neu“. Dokumente ohne Rundung bleiben im Format 1.

Einzelheiten und Grenzen: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 8 – Rundungen und Fasen“.
```

- [ ] **Step 2: Offene Punkte** – Abschnitt „Teilprojekt 8 – Rundungen und Fasen“ anlegen, Stand-Datum, und darin (mit den gemessenen Zahlen aus den Berichten) mindestens: Ecken (nur Gehrung zweier Kanten mit spiegelgleicher Lage, Kugelstück an drei Ebenen mit gleichem Radius; Setback und glatte Ecke kommen später; Ecken an Freiformflächen oder mit variablem Radius abgelehnt); Abschlüsse (Rundung endet an der dritten Fläche nur, wenn der Schnitt unterstützt ist – etwa nicht an einer gewölbten Wand ohne Tangentenkette; Freiform- und variable Rundungen nur an Ebenen senkrecht zur Kante); jede Kante bekommt ihre eigene Rundungsfläche (P3); die Volumenprobe vergleicht mit einer Schätzung (Spielraum 50 %, P2); Laufzeiten (Debug und Release) der Testreihen und die `--ignored`-Reihen mit Dauer; Rundung an Körpern mit mehreren Schalen und Flächenrundungen nicht Teil (Spec §1); Dateiformat (P7) samt der mechanischen Testanpassung.

- [ ] **Step 3: Spec §12** – an das Ende der Spec anfügen: Abschnitt „## 12. Änderungen bei der Umsetzung“ mit Stand-Datum; eine Tabelle P1–P14 („Entscheidung“, „Begründung und Ergebnis“) wie im Plan, ergänzt um das Ergebnis aus der Umsetzung; danach „Weitere Abweichungen von §1–§11“ (mindestens: Hermite-Hautfläche statt natürlichem Spline, Task 13; Volumenprobe im Feature, P2; Ecken nach P6 und Task 17; Abschlüsse P5; Übergangstests P12) und die Befunde der Stufenreviews.

- [ ] **Step 4: Gesamtprüfung**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --release -- --ignored
cargo deny check
git diff --stat f2f748d -- crates/planora-kernel/src/boolean crates/planora-mesh/src/tessellate.rs crates/planora-mesh/src/trimmed.rs crates/planora-mesh/src/discretize.rs crates/planora-mesh/src/check.rs
```
Expected: alles grün; die letzte Ausgabe leer (analytische Wege bitgleich). Testzahl (bestanden/ignoriert) und Laufzeit der Release-Reihen in offene-punkte.md und Spec §12 eintragen. Schlägt eine Release-Reihe fehl, ist das ein Befund für den Bericht (nicht ignorieren, nicht abschwächen).

- [ ] **Step 5: Committen**

```bash
git add README.md docs/superpowers/offene-punkte.md docs/superpowers/specs/2026-10-13-rundungen-und-fasen-design.md
git commit -m "docs: Rundungen und Fasen – README, offene Punkte, Änderungen bei der Umsetzung

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 21: Handprüfung im Fenster (nur nach Rücksprache mit dem Nutzer)

**Files:**
- Modify: `docs/superpowers/offene-punkte.md` (Abschnitt Teilprojekt 8, Absatz „Handprüfung“)

**Interfaces:**
- Consumes: Release-Build der App, `tools/gui-drive.ps1` (Befehle `shot`, `click`, `dclick`, `rclick`, `drag`, `wheel`, `type`; Screenshot nach `%TEMP%\planora_shot.png`).
- Produces: Bericht der Handprüfung (was geprüft, was nicht).

- [ ] **Step 1: Rückfrage** – Den Nutzer fragen, ob die Handprüfung jetzt laufen darf (er sitzt am Rechner, das Fenster übernimmt Maus und Tastatur). Ohne ausdrückliches Ja nicht fortfahren; Implementierer führen diese Task nie aus, nur der Controller.

- [ ] **Step 2: Vorbereiten** (PowerShell, vorübergehende Benutzerordner, damit keine echten Projekte berührt werden)

```powershell
cargo build --release -p planora-app
$tmp = Join-Path $env:TEMP ("planora_hand_" + [guid]::NewGuid())
New-Item -ItemType Directory -Force $tmp | Out-Null
$env:APPDATA = $tmp; $env:USERPROFILE = $tmp
Start-Process -FilePath .\target\release\planora-app.exe -RedirectStandardOutput "$tmp\out.log" -RedirectStandardError "$tmp\err.log"
```

- [ ] **Step 3: Ablauf** (je Schritt `powershell -File tools/gui-drive.ps1 <Befehl> …` und den Screenshot ansehen)

1. Quader 10 × 10 × 10 erzeugen; „Bearbeiten → Verrunden …“; obere vordere Kante anklicken (Hervorhebung prüfen); Radius 2; „Erzeugen“; Volumen im Eigenschaftenfenster ≈ 965,67 mm³.
2. Umschalt + Klick auf zwei weitere Kanten an derselben Ecke; Radius 1; „Erzeugen“ → Kugelecke sichtbar.
3. Im Baum „Rundung 1“ doppelklicken, Radius 1,5 übernehmen → Neuberechnung; Art auf variabel wechseln, Punkt hinzufügen, übernehmen.
4. Fase mit Abstand und Winkel an einer anderen Kante; Radius 20 an einer Kante → Meldung „zu groß“ in Statuszeile und Baum, Dokument unverändert.
5. Speichern (neues Projekt im vorübergehenden Ordner), Programm schließen, neu öffnen, Dokument laden → gleiche Form und gleiches Volumen.

- [ ] **Step 4: Aufräumen und berichten** – Programm beenden, `$tmp` löschen (`Remove-Item -Recurse -Force $tmp -Confirm:$false`). In `docs/superpowers/offene-punkte.md` den Absatz „Handprüfung (Teilprojekt 8)“ ergänzen: Datum, Build, was von Hand geprüft wurde und was nicht.

```bash
git add docs/superpowers/offene-punkte.md
git commit -m "docs: Handprüfung Rundungen und Fasen

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Anhang: Abdeckung der Spec (Selbstprüfung)

| Spec | Inhalt | Task |
|---|---|---|
| §1 | Rundung konstant/variabel, drei Fasenarten | 3, 5, 14 |
| §1 | alle Flächenarten (Ebene, Zylinder, Kegel, Kugel, Torus, Freiform) | 3 (analytisch), 15 (Bahn) |
| §1 | konvex und konkav | 1, 2, 3, 6 |
| §1, §4.2 | mehrere Kanten, Ketten, Tangentenkette (abschaltbar) | 9, 10, 12 |
| §1, §4.3 | Ecken rollende Kugel | 11, 12, 17 |
| §1, §5 | Feature, Zeitleiste, Verweise, Unterdrücken, Speichern | 7, 18 |
| R1, §9 | vier Stufen mit Review | Task 8, 12, 17, 20 (Stufenabschlüsse) |
| R2, R3, §3.1 | Kugelbahn per Newton, keine Versatzfläche | 2, 14 |
| R4, §3.2 | analytisch wo exakt, sonst Hautfläche | 3, 13, 14, 15 |
| R5 | Fase als Gerade im Querschnitt | 2, 3, 14 |
| R6, §4.5 | Stücke + `sew`, `validate_with`, Volumenprobe | 4, 5, 6, 7 |
| R7 | Ecken eingegrenzt, sonst benannt | 11, 17 |
| R8 | Tangentenkette standardmäßig an | 8 (Vorgabe `true`), 9 |
| R9, §3.3 | Punkte in Prozent, glatt ohne Überschwingen | 14, 19 |
| R10, §3.4 | benannte Ablehnungen, Dokument unverändert, Folgefeatures übersprungen | 1, 2, 3, 4, 5, 7, 8 |
| R11, §7 | Format 2, Format 1 lesbar, „zu neu“, Golden-Datei | 18 |
| R12, §11 | keine neue Abhängigkeit, `cargo deny` | alle (Global Constraints), 20 |
| §3.1 | Berührkurven `fit`/geprüft gegen beide Flächen | 14 (Randkurven der Hautfläche, Prüfung gegen beide Flächen) |
| §3.3 | Fase „zwei Abstände“ nach Fläche 1/2, „Abstand und Winkel“ | 2, 3, 7 (`first_face`), 8, 19 |
| §3.4 | Krümmung, fast tangential, Naht/Pol, zu groß, nicht endlich, Arbeitsgrenze | 1, 2, 3, 5, 14 |
| §4.1 | Beschnitt, alte Kante entfällt, Enden an dritter Fläche | 4 |
| §4.4 | Marken „Rundung N“/„Fase N“/„Ecke N“, Verweise auf neue Kanten | 4, 5, 7, 11 |
| §5 | `check_parameters`, `blend_error`, alle `match`-Stellen | 7 |
| §6 | Menü, Fenster, Mehrfachauswahl, Szene, Eigenschaftenfenster, Versionsprüfung | 8, 10, 19 |
| §8 | Tests Mathematik, Kern Stufe 1–3, Zufallstests, Feature/App/Projekt | 13, 14, 6, 12, 16, 7, 8, 18 |
| §8 | Handprüfung nach Rückfrage | 21 |
| §10 | Risiken offen berichten | 20 |
