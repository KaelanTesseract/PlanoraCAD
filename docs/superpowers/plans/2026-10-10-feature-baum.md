# PlanoraCAD Feature-Baum – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine parametrische Zeitleiste (Feature-Baum): Skizzen, Grundkörper, Extrusion, Rotation, Boolesche Operationen und Verschieben als Features, die nach jeder Änderung ab dort neu berechnet werden, mit stabilen Verweisen auf Flächen und Kanten („Rollen + Herkunft“) und einer Baum-Oberfläche.

**Architecture:** Der Kernel markiert jede Fläche mit `FaceTag { origin, role }`; Extrusion und Rotation vergeben Rollen, Boolesche Operationen reichen Marken weiter. Das neue Crate `planora-feature` hält `Document` (lineare Zeitleiste mit Rückrollmarker), berechnet Features in Reihenfolge, merkt Ergebnisse je Feature (Schlüssel aus Feature-Version und Versionen der Eingaben) und löst `FaceRef`/`EdgeRef` auf. Die App leitet ihre Körperliste aus dem `Document` ab; Skizzen werden Features, der Skizzen-Modus arbeitet auf einer Entwurfskopie und schreibt beim Beenden zurück.

**Tech Stack:** Rust (Edition 2024), bestehende Crates (`planora-math/-kernel/-mesh/-sketch/-render/-app`), neu: `planora-feature` (intern). **Keine neue externe Abhängigkeit.**

**Spec:** `docs/superpowers/specs/2026-10-10-feature-baum-design.md` (baut auf den Specs zu Fundament/Kernel, Booleschen Operationen und Skizzen auf)

## Entscheidungen zur Spec (Planungsstand 2026-10-10)

Sie konkretisieren die Spec; Abweichungen sind begründet. Sie werden dem Projekteigner am Ende der Umsetzung genannt und in die Spec (Abschnitt „Änderungen bei der Umsetzung“) eingetragen.

- **F1 – Rollen der Grundkörper:** Quader, Zylinder, Kegel und Kugel werden im Kernel über `extrude`/`revolve` gebaut und erben deren Rollen (`Boden`, `Deckel`, `Wand k`, `Fläche k`). Eigene Rollen wie „Seite +x“ gibt es nicht (Spec §4.1 wird angepasst).
- **F2 – Herkunft nachträglich:** Der Kernel erzeugt Marken mit `origin = 0` („noch nicht zugewiesen“); `Solid::assign_origin(feature_id)` setzt die Herkunft aller noch unzugewiesenen Marken. Der Kernel-Aufruf braucht also keinen zusätzlichen Parameter, und `FeatureId`s beginnen bei 1.
- **F3 – Zwischenspeicher über Versionen statt Hashes:** Jedes Feature hat eine `version` (zählt bei jeder Änderung hoch) und ein berechnetes Ergebnis mit eigener `result_version`. Der Schlüssel eines Features ist `[eigene version, result_version aller gelesenen Eingaben …]`. Gleicher Schlüssel → Ergebnis wird übernommen. `Document::recompute()` scannt immer die ganze Zeitleiste bis zum Marker und rechnet nur dort neu, wo der Schlüssel abweicht (statt `recompute(from)` der Spec).
- **F4 – Volumenprobe der Booleschen Operation im Feature:** `planora-feature` hängt von `planora-mesh` ab und führt `boolean_volume_plausible` beim Berechnen eines `Boolean`-Features aus (bisher in der App).
- **F5 – Körperverbrauch:** Ein `Boolean`-Feature verbraucht Ziel und Werkzeug, `Move` ersetzt den Körper; sichtbar sind alle Körper, die kein späteres Feature verbraucht hat. `Move` behält keine Körperidentität: Der verschobene Körper ist `BodyRef { feature: move_id, index: 0 }`.
- **F6 – Skizzen-Entwurf:** Der Skizzen-Modus arbeitet auf einer Kopie der Skizze (`AppState::sketch_draft`); „Skizze beenden“ schreibt sie per `Document::update` zurück und rechnet neu; während des Zeichnens wird nicht neu berechnet.
- **F7 – Kantenkopien folgen:** Kopierte Körperkanten werden als `EdgeCopy { curve, edge: EdgeRef }` im Skizzen-Feature geführt; beim Neuberechnen werden sie mit `Sketch::reproject_edge` an die neue Kante angepasst (Fixwerte und Radius werden aktualisiert, die Skizzenobjekte bleiben erhalten). Ändert die Kante ihre Art, meldet das Skizzen-Feature einen Fehler.
- **F8 – Profil-Index:** `Extrude`/`Revolve` verweisen mit `profile: usize` auf das Profil in `Sketch::profiles()`; ändert sich die Reihenfolge der Profile, ändert sich das Ziel (dokumentierte Grenze).
- **F10 – Status:** `FeatureStatus` hat fünf Varianten (`Ok`, `Error`, `Skipped`, `Suppressed`, `NotComputed`); `Suppressed` zusätzlich zum Flag `suppressed`, damit die Oberfläche den Zustand direkt lesen kann.
- **F11 – Rückrollmarker per Menü:** Der Marker wird in der Oberfläche über das Kontextmenü („Zurückrollen bis hier“, „Zum Ende“) gesetzt; Ziehen mit der Maus ist optional und nicht Teil der Abnahme.
- **F12 – Beispielprofile entfallen:** Die festen Beispielprofile der alten Extrusions-/Rotationsdialoge (`samples.rs`) werden samt Dialogen entfernt; Skizzen ersetzen sie.
- **F13 – Übergang:** Task 7 lässt `AppState::sketches` unverändert; `extrude_sketch`/`revolve_sketch` legen dort übergangsweise je ein Skizzen-Feature (Kopie) und das Extrusions-/Rotationsfeature an; Task 8 löst die Skizzenliste ab.
- **F9 – Position:** Position und Verschieben laufen über `Move`-Features; das Positionsfeld im Eigenschaftenfenster ändert das neueste `Move` desselben Körpers, wenn es das letzte Feature ist, das diesen Körper liest, sonst wird ein neues `Move` am Marker angelegt.

## Global Constraints

- Reines Rust; Zahlen `f64`, Einheit Millimeter; eine zentrale `Tolerance` (linear 1e-6, angular 1e-9). Keine freien Epsilon-Zahlen in neuem Code: benannte `const`s mit Kommentar oder aus `Tolerance` abgeleitet.
- `planora-feature` hängt nur von `planora-math`, `planora-kernel`, `planora-mesh`, `planora-sketch` und `thiserror` ab; es kennt weder egui noch Grafik. Der Kernel kennt weder Features noch Oberfläche (die Herkunft ist nur eine Zahl `u64`).
- **Jede neue Abhängigkeit wird vorher auf Lizenz geprüft** (hier: keine geplant). `cargo deny check` muss grün bleiben; Erlaubnisliste in `deny.toml` nie selbstständig erweitern – bei Befund anhalten und den Menschen fragen. Projektlizenz GPL-3.0-only.
- Kein Autodesk-Material (Code, Icons, Namen, Dateiformate); Oberflächentexte und Fehlermeldungen auf Deutsch.
- Keine Panics auf Nutzereingaben: Ids, Indizes, Profile, Verweise werden an den API-Grenzen geprüft; Änderungsoperationen sind atomar (bei Fehler bleibt das Dokument unverändert).
- Die Kernel-Erweiterung ändert keine Geometrie; alle bestehenden Kernel-, Netz-, Skizzen- und App-Tests bleiben (ggf. mechanisch angepasst) grün.
- Plattformen: Windows (MSVC) und Linux. Befehle in Bash (Git Bash): `export PATH="$PATH:/c/Users/denni/.cargo/bin"`.

## Review Focus

Eingaben, die die Spec andeutet, die aber leicht durchrutschen; jede Zeile hat einen Test in der genannten Task:

1. **Marken überleben Boolesche Operationen in allen Auswahlfällen** (Vereinigen, Abziehen mit Umdrehen, Schneiden, bündige Flächen, zerteilte Flächen): jede Ergebnisfläche trägt die Marke ihrer Quellfläche, keine geht verloren (Task 2).
2. **Verlorene und mehrdeutige Verweise:** Fläche verschwindet → klarer Fehler am verursachenden Feature, keine Panik; zerteilte Fläche löst über den `hint` auf die nächste Teilfläche (Task 4, Task 6).
3. **Fehlerausbreitung:** Fehler in der Mitte der Zeitleiste → abhängige Features „ausgesetzt“, unabhängige rechnen weiter; Unterdrücken wirkt wie ein Fehler für Abhängige (Task 5).
4. **Zwischenspeicher liefert nie Veraltetes:** Änderung an Eingaben (auch Skizzenebene auf Fläche, die von einem früheren Körper abhängt) invalidiert genau die richtigen Nachfolger; ein zweiter Lauf ohne Änderung rechnet nichts neu (Task 5).
5. **Löschen und Einfügen:** Löschen mit Folgen verlangt Bestätigung der Liste, Einfügen am Marker erzeugt nur Verweise auf frühere Features, Zyklen sind unmöglich; zufällige Folgen von Änderungen halten das Dokument gültig (Task 6).

## Hinweis zur Verifikation und zum Code in diesem Plan

Der Code in den Schritten ist die **Referenzimplementierung des beabsichtigten Entwurfs** und wurde noch nicht kompiliert. Er darf zum Kompilieren und zur Fehlerbehebung angepasst und intern umstrukturiert werden, solange die benannten Schnittstellen (Abschnitte „Interfaces“) und **alle Tests unverändert** erfüllt werden. Schlägt ein Test fehl, wird der Code korrigiert, nicht der Test – es sei denn, der Test selbst enthält einen Tippfehler (dann berichten). Rust ist installiert.

## Dateistruktur

```
crates/planora-kernel/
├─ src/topology.rs                 (ändern: FaceTag, Face.tag, Solid::assign_origin/faces_with_tag)
├─ src/extrude.rs, revolve.rs      (ändern: Rollen vergeben)
├─ src/transform.rs                (ändern: Marken übernehmen)
├─ src/boolean/{uv,select,sew,classify,mod}.rs  (ändern: Marken durchreichen, face_probe_point/edge_probe_point)
└─ tests/ tags.rs, tags_boolean.rs
crates/planora-sketch/
├─ src/model.rs, helpers.rs        (ändern: Sketch::set_frame, reproject_edge)
└─ tests/ reproject.rs
crates/planora-feature/            (neu)
├─ Cargo.toml
├─ src/ lib.rs error.rs model.rs refs.rs compute.rs recompute.rs ops.rs
└─ tests/ refs.rs compute.rs recompute.rs ops.rs chain.rs properties.rs
crates/planora-app/src/            (ändern)
├─ state.rs, sketch_state.rs, sketch_view.rs, sketch_panel.rs, browser.rs (Baum), properties.rs,
│  create_dialog.rs, boolean_dialog.rs, app.rs, lib.rs
└─ tests/ (anpassen) feature_state.rs tree_ui.rs
```

---

### Task 1: Kernel – Flächenmarken (`FaceTag`) für Extrusion, Rotation und Verschieben

**Files:**
- Modify: `crates/planora-kernel/src/topology.rs`, `crates/planora-kernel/src/extrude.rs`, `crates/planora-kernel/src/revolve.rs`, `crates/planora-kernel/src/transform.rs`, `crates/planora-kernel/src/lib.rs`
- Modify (nur `tag: None` an allen Stellen, die `Face { .. }` bauen): `crates/planora-kernel/src/boolean/sew.rs` (wird in Task 2 richtig gefüllt) und alle Tests/Crates, die `Face { .. }` direkt bauen (`cargo build --workspace --all-targets` zeigt sie)
- Test: `crates/planora-kernel/tests/tags.rs`

**Interfaces:**
- Produces (alle `pub`, Re-Export aus `planora_kernel`):
  - `struct FaceTag { pub origin: u64, pub role: String }` (`Clone, Debug, PartialEq, Eq, Hash`). `origin == 0` heißt „noch keinem Feature zugewiesen“.
  - `Face` bekommt `pub tag: Option<FaceTag>`. Das Feld beschreibt nur; es fließt weder in die Geometrie noch in den Validator ein.
  - `Solid::assign_origin(&mut self, origin: u64)`: setzt `tag.origin = origin` bei allen Flächen, deren Marke `origin == 0` hat (andere bleiben unverändert; Flächen ohne Marke bleiben ohne).
  - `Solid::faces_with_tag(&self, tag: &FaceTag) -> Vec<Id<Face>>`: alle Flächen mit genau dieser Marke, aufsteigend nach Id.
  - Rollen (alle als `String`, deutsch, 1-basierte Zähler):
    - `extrude`: Deckel `Deckel`, Boden `Boden`; Wand je Segment der **äußeren** Schleife `Wand k`; je Segment des Lochs mit Nummer `j` (Position in `profile.holes`) `Loch j, Wand k`. `k` zählt die Segmente der **eingegebenen** Schleife. Wird die Schleife intern zur Orientierung umgedreht (`Profile::oriented`), wird zurückgerechnet (Segment `i` der umgedrehten Schleife mit `m` Segmenten entspricht Segment `m − 1 − i` der Eingabe).
    - `revolve`: `Fläche k` je Segment `k` der **eingegebenen** Kette (ebenso zurückgerechnet, wenn die Kette intern umgedreht wird).
    - Alle diese Marken haben `origin = 0`. Quader/Zylinder/Kegel/Kugel (`primitives`) erben sie, weil sie über `extrude`/`revolve` gebaut werden (Entscheidung F1).
  - `Solid::transformed` (Verschieben/Spiegeln/Skalieren) übernimmt die Marken unverändert.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-kernel/tests/tags.rs`

```rust
use planora_kernel::*;
use planora_math::{DVec2, DVec3, DMat4};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn roles(s: &Solid) -> Vec<String> {
    let mut v: Vec<String> = s
        .faces
        .iter()
        .map(|(_, f)| f.tag.as_ref().map(|t| t.role.clone()).unwrap_or_default())
        .collect();
    v.sort();
    v
}

fn outward_normal(s: &Solid, role: &str) -> DVec3 {
    let (_, f) = s
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role))
        .unwrap_or_else(|| panic!("Rolle {role} fehlt"));
    let n = f.surface.normal(0.0, 0.0);
    if f.same_sense { n } else { -n }
}

#[test]
fn a_box_carries_floor_ceiling_and_four_walls() {
    let s = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    assert_eq!(roles(&s), ["Boden", "Deckel", "Wand 1", "Wand 2", "Wand 3", "Wand 4"]);
    assert!(s.faces.iter().all(|(_, f)| f.tag.as_ref().unwrap().origin == 0));
    k().validate(&s).unwrap();
}

#[test]
fn wall_numbers_follow_the_input_order_of_the_profile() {
    let rect = |pts: [DVec2; 4]| Profile { outer: ProfileLoop::polygon(&pts).unwrap(), holes: vec![] };
    let (a, b, c, d) = (DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0), DVec2::new(0.0, 3.0));
    // gegen den Uhrzeigersinn: Segment 1 ist die untere Kante (a→b), Wand zeigt nach −y
    let ccw = k().extrude(&rect([a, b, c, d]), &planora_math::Frame::WORLD, 2.0).unwrap();
    assert!((outward_normal(&ccw, "Wand 1") - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-9);
    // im Uhrzeigersinn eingegeben (wird intern umgedreht): Segment 1 ist a→d, Wand zeigt nach −x
    let cw = k().extrude(&rect([a, d, c, b]), &planora_math::Frame::WORLD, 2.0).unwrap();
    assert!((outward_normal(&cw, "Wand 1") - DVec3::new(-1.0, 0.0, 0.0)).length() < 1e-9);
    assert!((outward_normal(&cw, "Wand 2") - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9);
}

#[test]
fn holes_get_their_own_numbering() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(5.0, 5.0), 2.0), ProfileLoop::circle(DVec2::new(14.0, 14.0), 3.0)],
    };
    let s = k().extrude(&p, &planora_math::Frame::WORLD, 3.0).unwrap();
    let r = roles(&s);
    for want in ["Boden", "Deckel", "Wand 1", "Wand 4", "Loch 1, Wand 1", "Loch 2, Wand 1"] {
        assert!(r.iter().any(|x| x == want), "{want} fehlt in {r:?}");
    }
    assert_eq!(r.len(), 8);
}

#[test]
fn cylinder_cone_and_sphere_roles() {
    assert_eq!(roles(&k().make_cylinder(2.0, 5.0).unwrap()), ["Boden", "Deckel", "Wand 1"]);
    assert_eq!(roles(&k().make_cone(2.0, 5.0).unwrap()), ["Fläche 1", "Fläche 2"]);
    assert_eq!(roles(&k().make_sphere(3.0).unwrap()), ["Fläche 1"]);
}

#[test]
fn revolve_roles_follow_the_input_order_of_the_chain() {
    let up = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line { to: DVec2::new(2.0, 0.0) },
            Segment::Line { to: DVec2::new(0.0, 5.0) },
        ],
    };
    let a = k().revolve(&up, &planora_math::Frame::WORLD).unwrap();
    let kind = |s: &Solid, role: &str| {
        let (id, _) = s.faces.iter().find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role)).unwrap();
        k().face_kind(s, id).unwrap()
    };
    assert_eq!(kind(&a, "Fläche 1"), SurfaceKind::Plane, "Segment 1 ist die Grundscheibe");
    assert_eq!(kind(&a, "Fläche 2"), SurfaceKind::Cone);
    let down = up.reversed();
    let b = k().revolve(&down, &planora_math::Frame::WORLD).unwrap();
    assert_eq!(kind(&b, "Fläche 1"), SurfaceKind::Cone, "umgekehrt eingegeben: Segment 1 ist der Mantel");
    assert_eq!(kind(&b, "Fläche 2"), SurfaceKind::Plane);
}

#[test]
fn assign_origin_only_fills_unassigned_tags_and_faces_are_found_by_tag() {
    let mut s = k().make_box(DVec3::splat(2.0)).unwrap();
    s.assign_origin(7);
    assert!(s.faces.iter().all(|(_, f)| f.tag.as_ref().unwrap().origin == 7));
    s.assign_origin(9);
    assert!(s.faces.iter().all(|(_, f)| f.tag.as_ref().unwrap().origin == 7), "bereits zugewiesen");
    let tag = FaceTag { origin: 7, role: "Deckel".into() };
    assert_eq!(s.faces_with_tag(&tag).len(), 1);
    assert!(s.faces_with_tag(&FaceTag { origin: 8, role: "Deckel".into() }).is_empty());
}

#[test]
fn transforms_keep_the_tags() {
    let mut s = k().make_box(DVec3::splat(2.0)).unwrap();
    s.assign_origin(3);
    let moved = k().transform(&s, &DMat4::from_translation(DVec3::new(5.0, 0.0, 0.0))).unwrap();
    assert_eq!(roles(&moved), roles(&s));
    assert!(moved.faces.iter().all(|(_, f)| f.tag.as_ref().unwrap().origin == 3));
    let mirrored = k().transform(&s, &DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0))).unwrap();
    assert_eq!(roles(&mirrored), roles(&s));
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test tags`
Expected: FAIL (Kompilierfehler: `FaceTag`, `tag`, `assign_origin` fehlen)

- [ ] **Step 3: Implementierung**

`topology.rs`:

```rust
/// Beschreibende Marke einer Fläche (siehe Spec Feature-Baum §4). `origin == 0`: noch keinem
/// Feature zugewiesen.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FaceTag {
    pub origin: u64,
    pub role: String,
}

// in struct Face: pub tag: Option<FaceTag>,

impl Solid {
    /// Weist allen noch nicht zugewiesenen Marken (`origin == 0`) die Herkunft `origin` zu.
    pub fn assign_origin(&mut self, origin: u64) {
        let ids: Vec<Id<Face>> = self.faces.iter().map(|(id, _)| id).collect();
        for id in ids {
            if let Some(tag) = self.faces[id].tag.as_mut()
                && tag.origin == 0
            {
                tag.origin = origin;
            }
        }
    }

    pub fn faces_with_tag(&self, tag: &FaceTag) -> Vec<Id<Face>> {
        self.faces
            .iter()
            .filter(|(_, f)| f.tag.as_ref() == Some(tag))
            .map(|(id, _)| id)
            .collect()
    }
}
```

`extrude.rs`: `build_loop` bekommt zwei neue Parameter: das Rollenpräfix (`""` für die äußere Schleife, `"Loch j, "` für das `j`-te Loch) und `reversed: bool` (wurde die Schleife durch `oriented()` umgedreht?). Für Segment `i` einer Schleife mit `m` Segmenten ist `k = if reversed { m - i } else { i + 1 }` (1-basiert; bei `reversed` entspricht Index `i` dem Eingabesegment `m − 1 − i`, also `k = m − i`). Die Wandfläche bekommt `tag: Some(FaceTag { origin: 0, role: format!("{prefix}Wand {k}") })`. `reversed` wird in `extrude` vor dem Aufruf von `oriented()` berechnet: `signed_area() < 0.0` für die äußere Schleife (sie wird gedreht, wenn negativ), bzw. `> 0.0` für ein Loch (Löcher werden im Uhrzeigersinn erwartet). Deckel/Boden: `Deckel`/`Boden`.

`revolve.rs`: Beim Umdrehen der Kette (`if area < 0.0 { chain.reversed() }`) merkt sich die Funktion `reversed = area < 0.0`; jede erzeugte Fläche des Segments `i` (von `m` Segmenten) bekommt `role: format!("Fläche {}", if reversed { m - i } else { i + 1 })`.

`transform.rs`: in `Face { .. }` `tag: f.tag.clone()`.

`lib.rs`: `pub use topology::{.., FaceTag}`.

Alle anderen `Face { .. }`-Literale bekommen vorerst `tag: None` (in `boolean/sew.rs` wird es in Task 2 echt gefüllt).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS (alle bisherigen Tests unverändert grün)

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "feat(kernel): Flächenmarken (FaceTag) für Extrusion, Rotation und Verschieben"
```

---

### Task 2: Kernel – Marken durch Boolesche Operationen, Prüfpunkte für Flächen und Kanten

**Files:**
- Modify: `crates/planora-kernel/src/boolean/uv.rs` (`FaceGeom.tag`), `crates/planora-kernel/src/boolean/select.rs` (`Piece.tag`), `crates/planora-kernel/src/boolean/sew.rs`, `crates/planora-kernel/src/boolean/classify.rs` (Prüfpunkte), `crates/planora-kernel/src/boolean/mod.rs`, `crates/planora-kernel/src/lib.rs`
- Test: `crates/planora-kernel/tests/tags_boolean.rs`

**Interfaces:**
- Consumes: `FaceTag`, `Face.tag` (Task 1).
- Produces:
  - `FaceGeom` bekommt `pub tag: Option<FaceTag>` (gefüllt von `face_geom`); `Piece` bekommt `pub tag: Option<FaceTag>` (kopiert aus der `FaceGeom` der Quellfläche in `select_piece`); `sew` schreibt `tag: p.tag.clone()` in jede erzeugte Fläche. Zerteilte Flächen tragen alle dieselbe Marke; umgedrehte Flächen (Abziehen) behalten die Marke.
  - `pub fn face_probe_point(solid: &Solid, face: Id<Face>, tol: &Tolerance) -> Option<DVec3>`: ein Punkt im Inneren der Fläche (nicht auf dem Rand, nicht auf einer Naht), `None` bei unbekannter Id oder wenn keiner gefunden wird. Nutzt dieselbe Suche wie die Einordnung (`interior_point`) über die ganze Fläche.
  - `pub fn edge_probe_point(solid: &Solid, edge: Id<Edge>) -> Option<DVec3>`: Kurvenpunkt in der Mitte des Parameterbereichs (`None` bei unbekannter Id).
  - Beide aus `planora_kernel` re-exportiert.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-kernel/tests/tags_boolean.rs`

```rust
use planora_kernel::*;
use planora_math::{DMat4, DVec3, Tolerance};
use std::collections::HashSet;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn boxed(size: DVec3, at: DVec3, origin: u64) -> Solid {
    let mut s = k().make_box(size).unwrap();
    s = k().transform(&s, &DMat4::from_translation(at)).unwrap();
    s.assign_origin(origin);
    s
}

fn origins(s: &Solid) -> HashSet<u64> {
    s.faces.iter().map(|(_, f)| f.tag.as_ref().expect("jede Fläche trägt eine Marke").origin).collect()
}

#[test]
fn every_result_face_keeps_the_tag_of_its_source_face() {
    let a = boxed(DVec3::splat(10.0), DVec3::ZERO, 1);
    let b = boxed(DVec3::splat(10.0), DVec3::splat(5.0), 2);
    for (op, want) in [
        (BoolOp::Union, HashSet::from([1, 2])),
        (BoolOp::Difference, HashSet::from([1, 2])),
        (BoolOp::Intersection, HashSet::from([1, 2])),
    ] {
        let r = k().boolean(&a, &b, op).unwrap();
        assert_eq!(origins(&r), want, "{op:?}");
        k().validate(&r).unwrap();
    }
}

#[test]
fn flipped_tool_faces_keep_their_tag_and_a_split_face_keeps_one_tag_for_all_pieces() {
    let plate = boxed(DVec3::new(20.0, 20.0, 5.0), DVec3::ZERO, 1);
    // Nut quer über die ganze Breite: zerteilt die Deckfläche in zwei Teile
    let groove = boxed(DVec3::new(4.0, 30.0, 2.0), DVec3::new(8.0, -5.0, 3.0), 2);
    let r = k().boolean(&plate, &groove, BoolOp::Difference).unwrap();
    let lid = FaceTag { origin: 1, role: "Deckel".into() };
    assert_eq!(r.faces_with_tag(&lid).len(), 2, "die Deckfläche wurde in zwei Stücke geteilt");
    // die Nutwände stammen vom Werkzeug (origin 2), umgedreht
    assert!(r.faces.iter().any(|(_, f)| f.tag.as_ref().is_some_and(|t| t.origin == 2)));
}

#[test]
fn flush_and_curved_cases_keep_their_tags() {
    let a = boxed(DVec3::new(10.0, 10.0, 5.0), DVec3::ZERO, 1);
    let mut cyl = k().make_cylinder(2.0, 5.0).unwrap();
    cyl = k().transform(&cyl, &DMat4::from_translation(DVec3::new(5.0, 5.0, 0.0))).unwrap();
    cyl.assign_origin(2);
    let r = k().boolean(&a, &cyl, BoolOp::Difference).unwrap();
    assert!(r.faces.iter().all(|(_, f)| f.tag.is_some()));
    let bore = FaceTag { origin: 2, role: "Wand 1".into() };
    assert_eq!(r.faces_with_tag(&bore).len(), 1, "die Bohrungswand ist eine Fläche");
}

#[test]
fn probe_points_lie_on_their_face_or_edge() {
    let s = boxed(DVec3::new(4.0, 6.0, 8.0), DVec3::new(1.0, 2.0, 3.0), 1);
    let tol = Tolerance::default();
    for (id, f) in s.faces.iter() {
        let p = face_probe_point(&s, id, &tol).expect("Prüfpunkt");
        assert!(f.surface.implicit(p).abs() < 1e-9, "Punkt liegt auf der Trägerfläche");
    }
    for (id, e) in s.edges.iter() {
        let p = edge_probe_point(&s, id).unwrap();
        assert!((p - e.curve.point(0.5 * (e.t0 + e.t1))).length() < 1e-12);
    }
    assert!(face_probe_point(&s, Id::from_index(99), &tol).is_none());
    assert!(edge_probe_point(&s, Id::from_index(99)).is_none());
}

#[test]
fn probe_points_exist_for_curved_faces() {
    let s = k().make_cylinder(3.0, 4.0).unwrap();
    for (id, f) in s.faces.iter() {
        let p = face_probe_point(&s, id, &Tolerance::default()).expect("Prüfpunkt");
        assert!(f.surface.implicit(p).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test tags_boolean`
Expected: FAIL (Funktionen/Felder fehlen)

- [ ] **Step 3: Implementierung**

- `uv.rs`: `FaceGeom { …, tag: Option<FaceTag> }`, in `face_geom` aus `solid.faces[id].tag.clone()`; alle weiteren Stellen, die `FaceGeom { .. }` bauen (Tests), bekommen `tag: None`.
- `select.rs`: `Piece { …, tag: Option<FaceTag> }`, in `select_piece` aus `geom.tag.clone()` (auch bei `flip`).
- `sew.rs`: im `Face { .. }`-Literal `tag: p.tag.clone()`.
- `classify.rs`: `face_probe_point(solid, face, tol)`: `let geom = face_geom(solid, face)` (nach Prüfung `face.index() < solid.faces.len()`), ein `Region` über die ganze Fläche bauen (äußere Schleife und Löcher so, wie `split_face` es für eine Fläche ohne Schnitte liefern würde: am einfachsten `split_face(&geom, &[], tol)` und den ersten `Region`), dann `interior_point(&geom, &region, tol)`.
- `edge_probe_point`: `solid.edges.get(edge)` → `curve.point(0.5 * (t0 + t1))`.
- `lib.rs` und `boolean/mod.rs`: Re-Exporte.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "feat(kernel): Flächenmarken durch Boolesche Operationen; Prüfpunkte für Flächen und Kanten"
```

---

### Task 3: Skizzen – Ebene nachträglich setzen und Kantenkopien nachführen

**Files:**
- Modify: `crates/planora-sketch/src/model.rs` (`set_frame`), `crates/planora-sketch/src/helpers.rs` (`reproject_edge`), `crates/planora-sketch/src/lib.rs`
- Test: `crates/planora-sketch/tests/reproject.rs`

**Interfaces:**
- Consumes: `Sketch`, `add_projected_edge` (Skizzen-Teilprojekt), `Solid`, `Edge`.
- Produces:
  - `Sketch::set_frame(&mut self, frame: Frame) -> Result<(), SketchError>`: ersetzt die Skizzenebene (`InvalidInput`, wenn `!frame.is_orthonormal(tol.linear)`); die 2D-Werte bleiben unverändert; erhöht die `revision`. Bei Fehler bleibt die Skizze unverändert.
  - `Sketch::reproject_edge(&mut self, curve: CurveId, solid: &Solid, edge: Id<Edge>) -> Result<(), SketchError>`: projiziert die Kante wie `add_projected_edge` und **aktualisiert** dabei die schon vorhandene kopierte Kurve: Position der Punkte, Radius, Ellipsenachsen und die zugehörigen `Fixed`-/`Radius`-/`EllipseMajor|Minor|Angle`-Werte. Die Kurve behält ihre Id und alle Abhängigkeiten, die der Nutzer an sie geknüpft hat. Ändert die Kante ihre Art (Linie ↔ Kreis ↔ Bogen ↔ Ellipse) oder lässt sich nicht projizieren, gibt es `UnsupportedProjection` mit dem Hinweis „Die Kante hat ihre Art geändert“ (bzw. dem Grund der Projektion) und die Skizze bleibt unverändert (atomar). `UnknownObject` für unbekannte Kurven oder Kanten-Ids.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-sketch/tests/reproject.rs`

```rust
use planora_kernel::{AnalyticKernel, Id, Kernel};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{CurveKind, Sketch};
use planora_sketch::solver::SolveStatus;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn set_frame_replaces_the_plane_and_rejects_bad_frames() {
    let mut s = sketch();
    let rev = s.revision();
    let f = Frame::from_origin_normal(DVec3::new(0.0, 0.0, 5.0), DVec3::Z);
    s.set_frame(f).unwrap();
    assert_eq!(*s.frame(), f);
    assert_ne!(s.revision(), rev);
    let mut bad = f;
    bad.x = DVec3::new(f64::NAN, 0.0, 0.0);
    assert!(matches!(s.set_frame(bad), Err(SketchError::InvalidInput(_))));
    assert_eq!(*s.frame(), f, "unverändert");
}

#[test]
fn a_projected_line_follows_when_the_body_changes() {
    let a = k().make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let mut s = sketch();
    let mut copied = None;
    for (id, _) in a.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&a, id)
            && (s.line_length(c).unwrap_or(0.0) - 10.0).abs() < 1e-9
        {
            copied = Some((c, id));
            break;
        }
    }
    let (curve, edge) = copied.expect("eine Kante der Länge 10");
    let b = k().make_box(DVec3::new(12.0, 20.0, 5.0)).unwrap();
    s.reproject_edge(curve, &b, edge).unwrap();
    assert!((s.line_length(curve).unwrap() - 12.0).abs() < 1e-9);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!((s.line_length(curve).unwrap() - 12.0).abs() < 1e-7, "die Fixwerte wurden mit aktualisiert");
    assert!(s.curve(curve).unwrap().construction);
}

#[test]
fn a_projected_circle_follows_the_radius() {
    let a = k().make_cylinder(3.0, 8.0).unwrap();
    let mut s = sketch();
    let mut copied = None;
    for (id, _) in a.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&a, id)
            && matches!(s.curve(c).unwrap().kind, CurveKind::Circle { .. })
        {
            copied = Some((c, id));
            break;
        }
    }
    let (curve, edge) = copied.expect("Kreis");
    let b = k().make_cylinder(5.0, 8.0).unwrap();
    s.reproject_edge(curve, &b, edge).unwrap();
    assert!((s.radius(curve).unwrap() - 5.0).abs() < 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!((s.radius(curve).unwrap() - 5.0).abs() < 1e-7);
}

#[test]
fn a_change_of_kind_or_a_bad_id_is_an_error_and_leaves_the_sketch_alone() {
    let a = k().make_cylinder(3.0, 8.0).unwrap();
    let mut s = sketch();
    let mut line_curve = None;
    let mut circle_edge = None;
    for (id, _) in a.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&a, id) {
            match s.curve(c).unwrap().kind {
                CurveKind::Circle { .. } => circle_edge = Some(id),
                _ => line_curve = Some(c),
            }
        }
    }
    // Der Zylinder hat eine senkrechte Naht (wird nicht kopiert) – also nur Kreise.
    let c0 = s.curves().next().unwrap().0;
    let before = s.variables().to_vec();
    let box_b = k().make_box(DVec3::splat(5.0)).unwrap();
    let line_edge = box_b.edges.iter().find(|(_, e)| matches!(e.curve, planora_kernel::Curve::Line { .. })).map(|(i, _)| i).unwrap();
    let r = s.reproject_edge(c0, &box_b, line_edge);
    assert!(matches!(r, Err(SketchError::UnsupportedProjection(_))), "{r:?}");
    assert_eq!(s.variables(), before.as_slice());
    assert!(matches!(s.reproject_edge(c0, &a, Id::from_index(999)), Err(SketchError::UnknownObject)));
    assert!(matches!(s.reproject_edge(Id::from_index(999), &a, circle_edge.unwrap()), Err(SketchError::UnknownObject)));
    let _ = line_curve;
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test reproject`
Expected: FAIL (Methoden fehlen)

- [ ] **Step 3: Implementierung**

`model.rs`:

```rust
    pub fn set_frame(&mut self, frame: Frame) -> Result<(), SketchError> {
        if !frame.is_orthonormal(self.tol.linear) {
            return Err(SketchError::InvalidInput("Das Koordinatensystem der Skizze ist ungültig".into()));
        }
        self.frame = frame;
        self.touch_structure(); // erhöht die Revision, leert Zwischenspeicher
        Ok(())
    }
```

`helpers.rs` (Referenz): `reproject_edge` baut mit der vorhandenen Projektionslogik die neue Geometrie (Variante `Projected { Line(a,b), Circle(c,r), Arc(c,start,end,r), Ellipse(c,a,minor) }`, dieselbe Funktion, die `add_projected_edge` benutzt – sie wird in eine private Funktion `project_edge(&self, solid, edge) -> Result<Projected, SketchError>` herausgelöst), prüft `Projected`-Variante gegen `CurveKind` der Zielkurve (Linie↔Line, Kreis↔Circle, Bogen↔Arc, Ellipse↔Ellipse), und schreibt im selben `atomically`-Block: Punktkoordinaten über `set_point_xy`, bei Kreis den Radiusvariablenwert, bei Ellipse `ax`, `ay`, `minor`; danach für jede lebende Abhängigkeit `Fixed(p, ..)` mit `p` in den Punkten der Kurve den Wert auf die neue Position, `Radius(c, ..)` auf den neuen Radius, `EllipseMajor/Minor/Angle(c, ..)` auf die neuen Werte (Zugriff direkt auf `constraints[id].kind`, gleiche Crate). Alle anderen Abhängigkeiten der Kurve bleiben unverändert.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-sketch && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "feat(sketch): Ebene nachträglich setzen und kopierte Körperkanten nachführen"
```

---

### Task 4: Crate `planora-feature` – Datenmodell und stabile Verweise

**Files:**
- Modify: `Cargo.toml` (Workspace-Abhängigkeit `planora-feature`)
- Create: `crates/planora-feature/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/model.rs`, `src/refs.rs`
- Test: `crates/planora-feature/tests/refs.rs`

**Interfaces:**
- Consumes: `FaceTag`, `Solid::faces_with_tag`, `face_probe_point`, `edge_probe_point` (Tasks 1–2), `planora_sketch::{Sketch, model::CurveId}`.
- Produces (alle `pub`, Re-Exporte in `lib.rs`):
  - `FeatureId(pub u64)` (`Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord`), `BodyRef { pub feature: FeatureId, pub index: usize }` (`Clone, Copy, Debug, PartialEq, Eq, Hash`).
  - `enum StandardPlane { XY, XZ, YZ }` mit `fn frame(self) -> Frame` (XY: x=X, y=Y, z=Z; XZ: x=X, y=Z, z=−Y; YZ: x=Y, y=Z, z=X, alle rechtshändig) – sie ersetzt die gleichnamige Aufzählung der App.
  - `enum SketchPlane { Standard(StandardPlane), OnFace(FaceRef) }`, `struct EdgeCopy { pub curve: CurveId, pub edge: EdgeRef }`, `struct SketchFeature { pub sketch: Sketch, pub plane: SketchPlane, pub copies: Vec<EdgeCopy> }`.
  - `enum FeatureKind { Sketch(SketchFeature), Box { size: DVec3, origin: DVec3 }, Cylinder { radius: f64, height: f64, origin: DVec3 }, Sphere { radius: f64, origin: DVec3 }, Cone { radius: f64, height: f64, origin: DVec3 }, Extrude { sketch: FeatureId, profile: usize, height: f64 }, Revolve { sketch: FeatureId, profile: usize, axis: CurveId }, Boolean { target: BodyRef, tool: BodyRef, op: BoolOp }, Move { body: BodyRef, delta: DVec3 } }` mit `fn label(&self) -> &'static str` (deutsch: „Skizze“, „Quader“, „Zylinder“, „Kugel“, „Kegel“, „Extrusion“, „Rotation“, „Boolesche Operation“, „Verschieben“) und `fn dependencies(&self) -> Vec<FeatureId>` (alle Features, auf die sie verweisen: `sketch`, `target.feature`, `tool.feature`, `body.feature`, `FaceRef.origin` der Ebene und der Kantenkopien (beide Flächen); sortiert, ohne Doppelte).
  - `enum FeatureStatus { Ok, Error(String), Skipped(String), Suppressed, NotComputed }` (Entscheidung F10: `Suppressed` als fünfte Variante neben dem Flag `suppressed`, damit die Oberfläche den Zustand direkt lesen kann).
  - `struct Feature { pub id: FeatureId, pub name: String, pub suppressed: bool, pub kind: FeatureKind, pub status: FeatureStatus, .. }` (weitere Felder `pub(crate)`: `version: u64`, `result: Option<FeatureResult>`).
  - `FeatureError` (in `error.rs`, `thiserror`, deutsche Meldungen): `UnknownFeature`, `ForwardReference`, `FaceNotFound { origin: u64, role: String }` („Fläche „{role}“ von Feature {origin} ist nicht mehr vorhanden“), `EdgeNotFound { a: String, b: String }`, `UntaggedFace`, `UntaggedEdge`, `BodyNotFound { feature: u64, index: usize }`, `Kernel(String)`, `Sketch(String)`, `HasDependents(Vec<u64>)`, `InvalidInput(String)`.
  - `FaceRef { pub origin: u64, pub role: String, pub hint: DVec3 }` (`Clone, Debug, PartialEq`): `FaceRef::from_face(solid, face, tol) -> Result<FaceRef, FeatureError>` (`UntaggedFace` bei fehlender Marke oder `origin == 0`; `hint` = `face_probe_point`; gibt es keinen Prüfpunkt, `InvalidInput("Die Fläche hat keinen Prüfpunkt")`), `fn tag(&self) -> FaceTag`.
  - `ResolvedFace { pub body: usize, pub face: Id<Face>, pub hint: DVec3 }` und `resolve_face(solids: &[&Solid], r: &FaceRef, tol: &Tolerance) -> Result<ResolvedFace, FeatureError>`: sammelt in allen Körpern die Flächen mit `r.tag()`; keine → `FaceNotFound`; eine → sie; mehrere → die, deren `face_probe_point` dem `hint` am nächsten liegt (Gleichstand: kleinere Körper-/Flächen-Id). `hint` im Ergebnis = Prüfpunkt der gewählten Fläche (bei `None` der alte).
  - `EdgeRef { pub a: FaceRef, pub b: FaceRef, pub hint: DVec3 }`: `from_edge(solid, edge, tol)` nimmt die ersten zwei Flächen-Verwendungen der Kante (bei einer Naht dieselbe Fläche zweimal; `UntaggedEdge`, wenn es weniger als zwei Verwendungen gibt oder eine Fläche unmarkiert ist), `hint = edge_probe_point`. `ResolvedEdge { body, edge, hint }` und `resolve_edge(solids, r, tol)`: Kandidaten sind Kanten, die von einer Fläche mit Marke `a` **und** einer mit Marke `b` benutzt werden (bei `a.tag == b.tag` zählt auch eine Kante, die dieselbe Fläche zweimal benutzt); mehrere → kleinster Abstand des Kantenpunkts zum `hint`; keine → `EdgeNotFound`.
  - `Document` (Skelett, `model.rs`): `Document::new() -> Document` (leer, `marker = 0`, nächste Id 1), `features() -> &[Feature]`, `feature(id) -> Option<&Feature>`, `index_of(id) -> Option<usize>`, `marker() -> usize`.

- [ ] **Step 1: Crate anlegen**

Workspace-`Cargo.toml`: `planora-feature = { path = "crates/planora-feature" }`. `crates/planora-feature/Cargo.toml`: Paket wie die anderen Crates (`version.workspace = true` usw.); `[dependencies]` `planora-math`, `planora-kernel`, `planora-mesh`, `planora-sketch`, `thiserror` (alle `.workspace = true`); `[dev-dependencies]` `proptest.workspace = true`; `[lints] workspace = true`. `lib.rs`: Module `error`, `model`, `refs` (weitere kommen) und Re-Exporte der Typen oben.

- [ ] **Step 2: Failing tests schreiben** – `crates/planora-feature/tests/refs.rs`

```rust
use planora_feature::*;
use planora_kernel::*;
use planora_math::{DMat4, DVec3, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn tol() -> Tolerance {
    Tolerance::default()
}

fn tagged_box(size: DVec3, origin: u64) -> Solid {
    let mut s = k().make_box(size).unwrap();
    s.assign_origin(origin);
    s
}

fn face_with_role(s: &Solid, role: &str) -> Id<Face> {
    s.faces.iter().find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role)).map(|(i, _)| i).unwrap()
}

#[test]
fn a_face_reference_is_found_again_after_the_body_moved() {
    let a = tagged_box(DVec3::new(4.0, 6.0, 8.0), 5);
    let r = FaceRef::from_face(&a, face_with_role(&a, "Deckel"), &tol()).unwrap();
    assert_eq!((r.origin, r.role.as_str()), (5, "Deckel"));
    let moved = k().transform(&a, &DMat4::from_translation(DVec3::new(10.0, 0.0, 0.0))).unwrap();
    let hit = resolve_face(&[&moved], &r, &tol()).unwrap();
    assert_eq!(hit.body, 0);
    assert_eq!(moved.faces[hit.face].tag.as_ref().unwrap().role, "Deckel");
    assert!(hit.hint.x > 5.0, "der Hinweispunkt ist mitgewandert: {:?}", hit.hint);
}

#[test]
fn missing_untagged_and_unassigned_faces_are_errors() {
    let a = tagged_box(DVec3::splat(3.0), 5);
    let r = FaceRef::from_face(&a, face_with_role(&a, "Deckel"), &tol()).unwrap();
    let other = tagged_box(DVec3::splat(3.0), 6);
    assert!(matches!(resolve_face(&[&other], &r, &tol()), Err(FeatureError::FaceNotFound { origin: 5, .. })));
    assert!(matches!(resolve_face(&[], &r, &tol()), Err(FeatureError::FaceNotFound { .. })));
    let unassigned = k().make_box(DVec3::splat(3.0)).unwrap(); // origin == 0
    assert!(matches!(FaceRef::from_face(&unassigned, face_with_role(&unassigned, "Deckel"), &tol()), Err(FeatureError::UntaggedFace)));
    assert!(matches!(FaceRef::from_face(&a, Id::from_index(99), &tol()), Err(FeatureError::UntaggedFace | FeatureError::InvalidInput(_))));
}

#[test]
fn the_right_body_is_chosen_among_several() {
    let a = tagged_box(DVec3::splat(2.0), 1);
    let b = tagged_box(DVec3::splat(2.0), 2);
    let r = FaceRef::from_face(&b, face_with_role(&b, "Boden"), &tol()).unwrap();
    let hit = resolve_face(&[&a, &b], &r, &tol()).unwrap();
    assert_eq!(hit.body, 1);
}

#[test]
fn a_split_face_is_resolved_by_its_hint() {
    let mut plate = k().make_box(DVec3::new(20.0, 20.0, 5.0)).unwrap();
    plate.assign_origin(1);
    let mut groove = k().make_box(DVec3::new(4.0, 30.0, 2.0)).unwrap();
    groove = k().transform(&groove, &DMat4::from_translation(DVec3::new(8.0, -5.0, 3.0))).unwrap();
    groove.assign_origin(2);
    let cut = k().boolean(&plate, &groove, BoolOp::Difference).unwrap();
    let tag = FaceTag { origin: 1, role: "Deckel".into() };
    let pieces = cut.faces_with_tag(&tag);
    assert_eq!(pieces.len(), 2);
    let probes: Vec<DVec3> = pieces.iter().map(|&f| face_probe_point(&cut, f, &tol()).unwrap()).collect();
    for (i, p) in probes.iter().enumerate() {
        let r = FaceRef { origin: 1, role: "Deckel".into(), hint: *p + DVec3::new(0.01, 0.0, 0.0) };
        let hit = resolve_face(&[&cut], &r, &tol()).unwrap();
        assert_eq!(hit.face, pieces[i], "Hinweis {i}");
    }
}

#[test]
fn edge_references_use_the_two_adjacent_faces() {
    let a = tagged_box(DVec3::new(4.0, 6.0, 8.0), 3);
    let (ea, _) = a
        .edges
        .iter()
        .find(|(id, _)| {
            let r = EdgeRef::from_edge(&a, *id, &tol());
            r.is_ok_and(|r| {
                let roles = [r.a.role.as_str(), r.b.role.as_str()];
                roles.contains(&"Deckel") && roles.contains(&"Wand 1")
            })
        })
        .expect("die Kante zwischen Deckel und Wand 1");
    let r = EdgeRef::from_edge(&a, ea, &tol()).unwrap();
    let moved = k().transform(&a, &DMat4::from_translation(DVec3::new(0.0, 0.0, 7.0))).unwrap();
    let hit = resolve_edge(&[&moved], &r, &tol()).unwrap();
    let p = edge_probe_point(&moved, hit.edge).unwrap();
    let p0 = edge_probe_point(&a, ea).unwrap();
    assert!((p - p0 - DVec3::new(0.0, 0.0, 7.0)).length() < 1e-9);
}

#[test]
fn seam_edges_reference_the_same_face_twice() {
    let mut c = k().make_cylinder(3.0, 5.0).unwrap();
    c.assign_origin(4);
    let seam = c
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Line { .. }))
        .map(|(i, _)| i)
        .expect("Naht");
    let r = EdgeRef::from_edge(&c, seam, &tol()).unwrap();
    assert_eq!(r.a.tag(), r.b.tag());
    let hit = resolve_edge(&[&c], &r, &tol()).unwrap();
    assert_eq!(hit.edge, seam);
    let missing = tagged_box(DVec3::splat(1.0), 9);
    assert!(matches!(resolve_edge(&[&missing], &r, &tol()), Err(FeatureError::EdgeNotFound { .. })));
}

#[test]
fn feature_kinds_report_their_dependencies_and_labels() {
    let a = FeatureId(3);
    let b = FeatureId(5);
    let k1 = FeatureKind::Boolean { target: BodyRef { feature: b, index: 0 }, tool: BodyRef { feature: a, index: 0 }, op: BoolOp::Union };
    assert_eq!(k1.dependencies(), vec![a, b]);
    assert_eq!(k1.label(), "Boolesche Operation");
    let k2 = FeatureKind::Box { size: DVec3::ONE, origin: DVec3::ZERO };
    assert!(k2.dependencies().is_empty());
    let f = StandardPlane::XZ.frame();
    assert!((f.x.cross(f.y) - f.z).length() < 1e-12);
    assert!((f.z - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-12);
}

#[test]
fn a_new_document_is_empty() {
    let d = Document::new();
    assert!(d.features().is_empty());
    assert_eq!(d.marker(), 0);
    assert!(d.feature(FeatureId(1)).is_none());
}
```

- [ ] **Step 3: Fehlschlag prüfen**

Run: `cargo test -p planora-feature --test refs`
Expected: FAIL (Crate/Typen fehlen)

- [ ] **Step 4: Implementierung**

`refs.rs` (Referenz):

```rust
//! Stabile Verweise auf Flächen und Kanten (Rollen + Herkunft, Hinweispunkt als zweite Stufe).

use crate::error::FeatureError;
use planora_kernel::{Edge, Face, FaceTag, Id, Solid, edge_probe_point, face_probe_point};
use planora_math::{DVec3, Tolerance};

#[derive(Clone, Debug, PartialEq)]
pub struct FaceRef {
    pub origin: u64,
    pub role: String,
    pub hint: DVec3,
}

impl FaceRef {
    pub fn tag(&self) -> FaceTag {
        FaceTag { origin: self.origin, role: self.role.clone() }
    }

    pub fn from_face(solid: &Solid, face: Id<Face>, tol: &Tolerance) -> Result<FaceRef, FeatureError> {
        let f = solid.faces.get(face).ok_or(FeatureError::UntaggedFace)?;
        let tag = f.tag.as_ref().filter(|t| t.origin != 0).ok_or(FeatureError::UntaggedFace)?;
        let hint = face_probe_point(solid, face, tol)
            .ok_or_else(|| FeatureError::InvalidInput("Die Fläche hat keinen Prüfpunkt".into()))?;
        Ok(FaceRef { origin: tag.origin, role: tag.role.clone(), hint })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ResolvedFace {
    pub body: usize,
    pub face: Id<Face>,
    pub hint: DVec3,
}

pub fn resolve_face(solids: &[&Solid], r: &FaceRef, tol: &Tolerance) -> Result<ResolvedFace, FeatureError> {
    let tag = r.tag();
    let mut candidates: Vec<(usize, Id<Face>)> = Vec::new();
    for (bi, s) in solids.iter().enumerate() {
        candidates.extend(s.faces_with_tag(&tag).into_iter().map(|f| (bi, f)));
    }
    let probe = |&(bi, f): &(usize, Id<Face>)| face_probe_point(solids[bi], f, tol);
    let best = match candidates.len() {
        0 => {
            return Err(FeatureError::FaceNotFound { origin: r.origin, role: r.role.clone() });
        }
        1 => candidates[0],
        _ => *candidates
            .iter()
            .min_by(|a, b| {
                let da = probe(a).map_or(f64::INFINITY, |p| p.distance(r.hint));
                let db = probe(b).map_or(f64::INFINITY, |p| p.distance(r.hint));
                da.total_cmp(&db)
            })
            .expect("nicht leer"),
    };
    let hint = probe(&best).unwrap_or(r.hint);
    Ok(ResolvedFace { body: best.0, face: best.1, hint })
}
```

`EdgeRef`/`resolve_edge` analog: Hilfsfunktion `edge_uses(solid) -> HashMap<Id<Edge>, Vec<Id<Face>>>` (jede Halbkante jeder Schleife jeder Fläche, auch doppelt derselben Fläche). `from_edge`: `uses[edge]` hat ≥ 2 Einträge, die ersten beiden Flächen sind markiert (`origin != 0`) → `EdgeRef`. `resolve_edge`: je Körper `uses`; Kandidaten: Kanten, deren Verwendungsliste eine Fläche mit Marke `a` und (davon verschiedenen Eintrag) eine mit Marke `b` enthält; Auswahl nach Abstand von `edge_probe_point` zum `hint`.

`model.rs`: Typen wie unter „Interfaces“; `FeatureKind::dependencies` sammelt die Ids (`Sketch`: `OnFace(r)` → `r.origin`, Kantenkopien → `a.origin`, `b.origin`), sortiert und dedupliziert; `StandardPlane::frame` wie angegeben (`Frame { origin: ZERO, x, y, z }`).

- [ ] **Step 5: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all && cargo deny check`
Expected: PASS, `deny.toml` unverändert

- [ ] **Step 6: Commit**

```bash
git add -A Cargo.toml Cargo.lock crates
git commit -m "feat(feature): Crate planora-feature mit Datenmodell und stabilen Flächen- und Kantenverweisen"
```

---

### Task 5: Berechnung der Features, Neuberechnung mit Zwischenspeicher, Fehlerausbreitung

**Files:**
- Create: `crates/planora-feature/src/compute.rs`, `crates/planora-feature/src/recompute.rs`
- Modify: `crates/planora-feature/src/model.rs`, `crates/planora-feature/src/lib.rs`
- Test: `crates/planora-feature/tests/compute.rs`, `crates/planora-feature/tests/recompute.rs`

**Interfaces:**
- Consumes: Task 4, Kernel (`make_*`, `extrude`, `revolve`, `boolean`, `transform`, `validate`), `planora_sketch::{extrude_profile, revolve_profile, plane_of_face}`, `planora_mesh::{tessellate, mass_properties, boolean_volume_plausible, TessellationOptions}`.
- Produces:
  - `Document::with_options(chord_tolerance: f64) -> Document` (Standard `0.05`, wie die App; `Document::new()` nutzt ihn), `kernel() -> &AnalyticKernel`.
  - `add_feature(&mut self, name: impl Into<String>, kind: FeatureKind) -> Result<FeatureId, FeatureError>`: fügt am Rückrollmarker ein (`marker += 1`), prüft **vor** dem Einfügen, dass alle `kind.dependencies()` existieren und **vor der Einfügestelle** liegen (`UnknownFeature`/`ForwardReference`), vergibt die nächste Id (ab 1) und die erste `version`, ruft `recompute()` auf.
  - `set_suppressed(&mut self, id, suppressed: bool) -> Result<(), FeatureError>` (bumpt `version`, ruft `recompute()`), `set_marker(&mut self, n: usize) -> Result<(), FeatureError>` (`n <= features.len()`, sonst `InvalidInput`; ruft `recompute()`).
  - `struct RecomputeReport { pub computed: Vec<FeatureId>, pub reused: Vec<FeatureId> }` und `recompute(&mut self) -> RecomputeReport`.
  - `struct VisibleBody<'a> { pub body: BodyRef, pub solid: &'a Solid, pub name: String }`, `Document::visible_bodies(&self) -> Vec<VisibleBody<'_>>` (Reihenfolge der Zeitleiste; Name = Name des Features, bei mehreren Körpern eines Features ergänzt um „ {index+1}“), `Document::body(&self, r: BodyRef) -> Option<&Solid>` (Ergebnis des Features, unabhängig von Sichtbarkeit), `Document::sketch_of(&self, id: FeatureId) -> Option<&Sketch>` (berechnete Skizze des Skizzen-Features: mit aufgelöster Ebene und nachgeführten Kantenkopien).
  - Regeln der Berechnung (`compute.rs`):
    - **Skizze:** Ebene `Standard(p)` → `p.frame()`; `OnFace(r)` → `resolve_face` in den **sichtbaren Körpern vor dem Feature** → `plane_of_face`. `sketch.set_frame(frame)`. Jede `EdgeCopy`: `resolve_edge` in denselben Körpern → `reproject_edge`. Danach `solve()` (Ergebnisstatus wird ignoriert). Hinweispunkte der Verweise werden ohne Versionsänderung im Feature aktualisiert. Ergebnis: keine Körper, aber die berechnete Skizze.
    - **Grundkörper:** Kernel-Aufruf, bei `origin != 0` Verschiebung um `origin`, `validate`, `assign_origin(id.0)`.
    - **Extrusion/Rotation:** Profile der **berechneten** Skizze des Eingangsfeatures; `profile` außerhalb → Fehler „Profil {n} gibt es nicht“; `extrude_profile`/`revolve_profile`; `validate`; `assign_origin`.
    - **Boolesche Operation:** Ziel und Werkzeug müssen unter den sichtbaren Körpern vor dem Feature sein (sonst `BodyNotFound`); `kernel.boolean`; Volumenprobe `boolean_volume_plausible(op, va, vb, v, 0.02)` mit Tessellierung bei `chord_tolerance` → bei Misserfolg Fehler „Das Ergebnis ist unplausibel (Volumenprobe fehlgeschlagen)“. Das Ergebnis ersetzt Ziel und Werkzeug (Entscheidung F5).
    - **Verschieben:** Körper unter den sichtbaren; `transform` um `delta`; ersetzt den Körper.
  - **Abhängigkeitsstatus:** Vor der Berechnung wird jedes Feature aus `kind.dependencies()` geprüft: unterdrückt → `Skipped("Feature „X“ ist unterdrückt")`; `Error`/`Skipped` → `Skipped("abhängig von Feature „X“")`.
  - **Sichtbarkeit:** `visible_at(end)` spielt die Zeitleiste bis `end` nach: Ein Feature mit Status `Ok` fügt seine Körper hinzu; `Boolean` entfernt Ziel und Werkzeug und fügt das Ergebnis hinzu; `Move` ersetzt den Körper. Features hinter dem Marker, unterdrückte, fehlerhafte oder ausgesetzte tragen nichts bei und verbrauchen nichts.
  - **Zwischenspeicher (F3):** Schlüssel eines Features = `[feature.version]` + `result_version` jedes Features aus `dependencies()` + bei Features, die sichtbare Körper lesen (`Boolean`, `Move`, `Sketch` mit `OnFace` oder Kantenkopien) die `result_version`s aller dafür sichtbaren Körper. Weicht der Schlüssel vom gespeicherten ab (oder gibt es kein Ergebnis), wird neu berechnet und eine neue `result_version` vergeben; sonst wird das Ergebnis übernommen (`reused`).
  - Features hinter dem Marker erhalten `NotComputed` (Ergebnis bleibt im Speicher, damit das Zurückholen des Markers billig ist), unterdrückte `Suppressed`.

- [ ] **Step 1: Failing tests schreiben**

`crates/planora-feature/tests/compute.rs`:

```rust
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::model::Sketch;

fn volume(s: &planora_kernel::Solid) -> f64 {
    mass_properties(&tessellate(s, &TessellationOptions { chord_tolerance: 1e-3 }).unwrap()).volume
}

pub fn rect_sketch(w: f64, h: f64) -> SketchFeature {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(w, h)).unwrap();
    SketchFeature { sketch: sk, plane: SketchPlane::Standard(StandardPlane::XY), copies: vec![] }
}

#[test]
fn primitives_and_extrusion_produce_bodies_with_origin_tags() {
    let mut d = Document::new();
    let b = d.add_feature("Quader", FeatureKind::Box { size: DVec3::new(2.0, 3.0, 4.0), origin: DVec3::new(10.0, 0.0, 0.0) }).unwrap();
    let s = d.add_feature("Skizze 1", FeatureKind::Sketch(rect_sketch(40.0, 30.0))).unwrap();
    let e = d.add_feature("Extrusion 1", FeatureKind::Extrude { sketch: s, profile: 0, height: 10.0 }).unwrap();
    for id in [b, s, e] {
        assert_eq!(d.feature(id).unwrap().status, FeatureStatus::Ok, "{id:?}");
    }
    let vis = d.visible_bodies();
    assert_eq!(vis.len(), 2, "die Skizze liefert keinen Körper");
    assert!((volume(vis[0].solid) - 24.0).abs() < 1e-6);
    assert!((volume(vis[1].solid) - 12000.0).abs() < 1e-3);
    let bounds_x = vis[0].solid.vertices.iter().map(|(_, v)| v.point.x).fold(f64::MAX, f64::min);
    assert!((bounds_x - 10.0).abs() < 1e-9, "Position");
    assert!(vis[1].solid.faces.iter().all(|(_, f)| f.tag.as_ref().unwrap().origin == e.0));
    assert_eq!(vis[1].name, "Extrusion 1");
    assert!(d.sketch_of(s).is_some());
}

#[test]
fn a_boolean_consumes_its_inputs_and_checks_the_volume() {
    let mut d = Document::new();
    let a = d.add_feature("A", FeatureKind::Box { size: DVec3::splat(10.0), origin: DVec3::ZERO }).unwrap();
    let c = d.add_feature("Z", FeatureKind::Cylinder { radius: 2.0, height: 10.0, origin: DVec3::new(5.0, 5.0, 0.0) }).unwrap();
    let diff = d
        .add_feature("Abzug", FeatureKind::Boolean { target: BodyRef { feature: a, index: 0 }, tool: BodyRef { feature: c, index: 0 }, op: BoolOp::Difference })
        .unwrap();
    assert_eq!(d.feature(diff).unwrap().status, FeatureStatus::Ok);
    let vis = d.visible_bodies();
    assert_eq!(vis.len(), 1);
    assert_eq!(vis[0].body.feature, diff);
    assert!((volume(vis[0].solid) - (1000.0 - std::f64::consts::PI * 4.0 * 10.0)).abs() < 0.5);
    assert!(d.body(BodyRef { feature: a, index: 0 }).is_some(), "Ergebnisse früherer Features bleiben lesbar");
}

#[test]
fn a_move_replaces_its_body_and_keeps_the_tags() {
    let mut d = Document::new();
    let a = d.add_feature("A", FeatureKind::Box { size: DVec3::splat(2.0), origin: DVec3::ZERO }).unwrap();
    let m = d.add_feature("Verschieben", FeatureKind::Move { body: BodyRef { feature: a, index: 0 }, delta: DVec3::new(0.0, 0.0, 5.0) }).unwrap();
    let vis = d.visible_bodies();
    assert_eq!(vis.len(), 1);
    assert_eq!(vis[0].body, BodyRef { feature: m, index: 0 });
    let zmin = vis[0].solid.vertices.iter().map(|(_, v)| v.point.z).fold(f64::MAX, f64::min);
    assert!((zmin - 5.0).abs() < 1e-9);
    assert!(vis[0].solid.faces.iter().all(|(_, f)| f.tag.as_ref().unwrap().origin == a.0), "Marken bleiben beim Verschieben");
}

#[test]
fn revolve_from_a_sketch_axis() {
    let mut d = Document::new();
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let (p, q) = (sk.add_point(0.0, -1.0).unwrap(), sk.add_point(0.0, 6.0).unwrap());
    let axis = sk.add_line(p, q).unwrap();
    sk.set_construction(axis, true).unwrap();
    sk.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(3.0, 5.0)).unwrap();
    let s = d.add_feature("S", FeatureKind::Sketch(SketchFeature { sketch: sk, plane: SketchPlane::Standard(StandardPlane::XY), copies: vec![] })).unwrap();
    let r = d.add_feature("Rotation", FeatureKind::Revolve { sketch: s, profile: 0, axis }).unwrap();
    assert_eq!(d.feature(r).unwrap().status, FeatureStatus::Ok, "{:?}", d.feature(r).unwrap().status);
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - std::f64::consts::PI * 9.0 * 5.0).abs() < 0.2);
}

#[test]
fn adding_features_validates_references() {
    let mut d = Document::new();
    assert!(matches!(
        d.add_feature("X", FeatureKind::Extrude { sketch: FeatureId(7), profile: 0, height: 1.0 }),
        Err(FeatureError::UnknownFeature)
    ));
    assert!(d.features().is_empty(), "nichts wurde angelegt");
}
```

`crates/planora-feature/tests/recompute.rs`:

```rust
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;

fn boxed(d: &mut Document, name: &str) -> FeatureId {
    d.add_feature(name, FeatureKind::Box { size: DVec3::splat(4.0), origin: DVec3::ZERO }).unwrap()
}

#[test]
fn a_second_run_without_changes_computes_nothing() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
    d.add_feature("U", FeatureKind::Boolean { target: BodyRef { feature: a, index: 0 }, tool: BodyRef { feature: b, index: 0 }, op: BoolOp::Union }).unwrap();
    let report = d.recompute();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    assert_eq!(report.reused.len(), 3);
}

#[test]
fn errors_skip_dependents_but_not_independent_features() {
    let mut d = Document::new();
    let bad = d.add_feature("Zu groß", FeatureKind::Cylinder { radius: -1.0, height: 2.0, origin: DVec3::ZERO });
    // der Kernel lehnt den Zylinder ab; das Feature wird angelegt und trägt den Fehler
    let bad = bad.unwrap();
    assert!(matches!(d.feature(bad).unwrap().status, FeatureStatus::Error(_)));
    let ok = boxed(&mut d, "Quader");
    let dep = d
        .add_feature("Verschieben", FeatureKind::Move { body: BodyRef { feature: bad, index: 0 }, delta: DVec3::X })
        .unwrap();
    assert_eq!(d.feature(ok).unwrap().status, FeatureStatus::Ok);
    assert!(matches!(d.feature(dep).unwrap().status, FeatureStatus::Skipped(_)));
    assert_eq!(d.visible_bodies().len(), 1, "nur der Quader");
}

#[test]
fn suppressing_a_feature_skips_its_dependents() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
    let u = d.add_feature("U", FeatureKind::Boolean { target: BodyRef { feature: a, index: 0 }, tool: BodyRef { feature: b, index: 0 }, op: BoolOp::Union }).unwrap();
    d.set_suppressed(a, true).unwrap();
    assert_eq!(d.feature(a).unwrap().status, FeatureStatus::Suppressed);
    assert!(matches!(&d.feature(u).unwrap().status, FeatureStatus::Skipped(m) if m.contains("unterdrückt")), "{:?}", d.feature(u).unwrap().status);
    let names: Vec<String> = d.visible_bodies().into_iter().map(|v| v.name).collect();
    assert_eq!(names, ["B"]);
    d.set_suppressed(a, false).unwrap();
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
    assert_eq!(d.visible_bodies().len(), 1);
}

#[test]
fn a_boolean_with_a_consumed_body_reports_it() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
    let u = d.add_feature("U", FeatureKind::Boolean { target: BodyRef { feature: a, index: 0 }, tool: BodyRef { feature: b, index: 0 }, op: BoolOp::Union }).unwrap();
    let again = d.add_feature("V", FeatureKind::Move { body: BodyRef { feature: a, index: 0 }, delta: DVec3::X }).unwrap();
    assert!(matches!(&d.feature(again).unwrap().status, FeatureStatus::Error(m) if m.contains("nicht")), "{:?}", d.feature(again).unwrap().status);
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
}

#[test]
fn the_marker_hides_later_features_and_inserts_at_its_position() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
    d.set_marker(1).unwrap();
    assert_eq!(d.feature(b).unwrap().status, FeatureStatus::NotComputed);
    assert_eq!(d.visible_bodies().len(), 1);
    let c = boxed(&mut d, "C");
    assert_eq!(d.index_of(c), Some(1), "wird am Marker eingefügt");
    assert_eq!(d.index_of(b), Some(2));
    assert_eq!(d.marker(), 2);
    assert_eq!(d.feature(a).unwrap().status, FeatureStatus::Ok);
    assert!(d.set_marker(99).is_err());
    d.set_marker(3).unwrap();
    assert_eq!(d.visible_bodies().len(), 3);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-feature --test compute --test recompute`
Expected: FAIL

- [ ] **Step 3: Implementierung** – Kern in `recompute.rs` (Referenz, Skeleton; die Einzelberechnungen stehen in `compute.rs`)

```rust
//! Neuberechnung der Zeitleiste mit Zwischenspeicher (Versionen statt Hashes, Entscheidung F3).

use crate::compute::{Computed, compute_feature};
use crate::error::FeatureError;
use crate::model::*;

#[derive(Clone, Debug, Default)]
pub struct RecomputeReport {
    pub computed: Vec<FeatureId>,
    pub reused: Vec<FeatureId>,
}

impl Document {
    pub fn recompute(&mut self) -> RecomputeReport {
        let mut report = RecomputeReport::default();
        for idx in 0..self.features.len() {
            let id = self.features[idx].id;
            if idx >= self.marker {
                self.features[idx].status = FeatureStatus::NotComputed;
                continue;
            }
            if self.features[idx].suppressed {
                self.features[idx].status = FeatureStatus::Suppressed;
                self.features[idx].result = None;
                continue;
            }
            if let Some(why) = self.blocked_by_dependency(idx) {
                self.features[idx].status = FeatureStatus::Skipped(why);
                self.features[idx].result = None;
                continue;
            }
            let visible = self.visible_at(idx);
            let key = self.cache_key(idx, &visible);
            if self.features[idx].result.as_ref().is_some_and(|r| r.key == key) {
                self.features[idx].status = FeatureStatus::Ok;
                report.reused.push(id);
                continue;
            }
            match compute_feature(self, idx, &visible) {
                Ok(computed) => {
                    let result_version = self.bump();
                    self.store(idx, computed, key, result_version);
                    self.features[idx].status = FeatureStatus::Ok;
                }
                Err(e) => {
                    self.features[idx].result = None;
                    self.features[idx].status = FeatureStatus::Error(e.to_string());
                }
            }
            report.computed.push(id);
        }
        report
    }
}
```

Wichtig:
- `cache_key`: `[version]` + `result_version` jedes Features aus `dependencies()` (0, wenn kein Ergebnis) + bei lesenden Features die `result_version`s der Features der sichtbaren Körper.
- `blocked_by_dependency` liefert den Skip-Text („Feature „X“ ist unterdrückt“ / „abhängig von Feature „X“ (Fehler)“), wenn ein Eingangsfeature unterdrückt ist oder `Error`/`Skipped` trägt; für Eingänge, die nicht vor `idx` liegen, den Fehler `ForwardReference` als `Error`.
- `Computed { bodies: Vec<Solid>, sketch: Option<Sketch>, hints: Option<…> }` – Hinweisaktualisierungen werden nach `store` ohne `version`-Änderung in die Definition geschrieben.
- Fehler in `compute_feature` sind `FeatureError`, der Statustext ist ihre deutsche Meldung (`Kernel(..)`/`Sketch(..)` tragen den Text des unterliegenden Fehlers).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "feat(feature): Berechnung der Features, Neuberechnung mit Zwischenspeicher und Fehlerausbreitung"
```

---

### Task 6: Änderungsoperationen, Löschen mit Folgen, Ende-zu-Ende- und Eigenschaftstests

**Files:**
- Create: `crates/planora-feature/src/ops.rs`
- Modify: `crates/planora-feature/src/lib.rs`
- Test: `crates/planora-feature/tests/ops.rs`, `crates/planora-feature/tests/chain.rs`, `crates/planora-feature/tests/properties.rs`

**Interfaces:**
- Consumes: Tasks 4–5.
- Produces (`impl Document`):
  - `update(&mut self, id: FeatureId, f: impl FnOnce(&mut FeatureKind)) -> Result<(), FeatureError>`: wendet `f` auf eine **Kopie** der Art an; die Variante darf sich nicht ändern (`InvalidInput`), alle `dependencies()` müssen existieren und vor dem Feature liegen (`UnknownFeature`/`ForwardReference`); danach Ersetzen, `version` hochzählen, `recompute()`. Bei Fehler bleibt das Dokument unverändert.
  - `rename(&mut self, id, name: impl Into<String>) -> Result<(), FeatureError>` (leerer Name → `InvalidInput`; ändert keine Version).
  - `dependents(&self, id) -> Vec<FeatureId>`: alle Features (transitiv, in Zeitleistenreihenfolge), die über `dependencies()` auf `id` oder ein abhängiges Feature verweisen; `Boolean`/`Move` zählen auch, wenn sie ein Ergebnis eines abhängigen Features lesen.
  - `delete(&mut self, id) -> Result<(), FeatureError>`: `HasDependents(ids)` (mit `u64`-Ids), wenn `dependents(id)` nicht leer ist; sonst entfernt es das Feature, passt den Marker an (`marker -= 1`, falls das Feature vor dem Marker lag) und rechnet neu. `delete_with_dependents(&mut self, id) -> Result<Vec<FeatureId>, FeatureError>`: entfernt das Feature und alle abhängigen und gibt die entfernten Ids zurück.
  - `check_invariants(&self) -> Result<(), String>` (für Tests): alle Verweise zeigen auf frühere Features; Features mit Status `Ok` haben ein Ergebnis, alle anderen keines (außer `NotComputed`/`Suppressed` dürfen ein altes behalten); Features ab dem Marker sind `NotComputed`; alle sichtbaren Körper bestehen den Kernel-Validator; `FeatureId`s sind eindeutig.

- [ ] **Step 1: Failing tests schreiben**

`crates/planora-feature/tests/ops.rs`:

```rust
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;

fn boxed(d: &mut Document, name: &str, size: f64) -> FeatureId {
    d.add_feature(name, FeatureKind::Box { size: DVec3::splat(size), origin: DVec3::ZERO }).unwrap()
}

#[test]
fn update_changes_parameters_and_recomputes_only_the_followers() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    let u = d.add_feature("U", FeatureKind::Boolean { target: BodyRef { feature: a, index: 0 }, tool: BodyRef { feature: b, index: 0 }, op: BoolOp::Union }).unwrap();
    // B ändern → B und U neu, A nicht
    let before = d.recompute();
    assert!(before.computed.is_empty());
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::splat(6.0);
        }
    })
    .unwrap();
    // recompute lief schon in update; ein weiterer Lauf ändert nichts mehr
    assert!(d.recompute().computed.is_empty());
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
}

#[test]
fn update_count_of_recomputed_features_is_exactly_the_followers() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    let _c = boxed(&mut d, "C", 4.0);
    let u = d.add_feature("U", FeatureKind::Boolean { target: BodyRef { feature: a, index: 0 }, tool: BodyRef { feature: b, index: 0 }, op: BoolOp::Union }).unwrap();
    let report = d.update_with_report(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::splat(5.0);
        }
    })
    .unwrap();
    let mut ids = report.computed.clone();
    ids.sort();
    assert_eq!(ids, vec![b, u], "nur B und das davon abhängige U");
    assert_eq!(report.reused.len(), 2, "A und C");
}

#[test]
fn update_rejects_variant_changes_and_forward_references_without_touching_the_document() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let m = d.add_feature("M", FeatureKind::Move { body: BodyRef { feature: a, index: 0 }, delta: DVec3::X }).unwrap();
    let r = d.update(a, |k| *k = FeatureKind::Sphere { radius: 1.0, origin: DVec3::ZERO });
    assert!(matches!(r, Err(FeatureError::InvalidInput(_))));
    let r = d.update(a, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::splat(2.0);
        }
    });
    assert!(r.is_ok());
    let r = d.update(m, |k| {
        if let FeatureKind::Move { body, .. } = k {
            body.feature = FeatureId(99);
        }
    });
    assert!(matches!(r, Err(FeatureError::UnknownFeature)));
    let r = d.update(a, |_| {});
    assert!(r.is_ok());
    assert!(d.update(FeatureId(77), |_| {}).is_err());
}

#[test]
fn rename_and_dependents_and_delete_with_consequences() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    let u = d.add_feature("U", FeatureKind::Boolean { target: BodyRef { feature: a, index: 0 }, tool: BodyRef { feature: b, index: 0 }, op: BoolOp::Union }).unwrap();
    let m = d.add_feature("M", FeatureKind::Move { body: BodyRef { feature: u, index: 0 }, delta: DVec3::X }).unwrap();
    d.rename(a, "Grundplatte").unwrap();
    assert_eq!(d.feature(a).unwrap().name, "Grundplatte");
    assert!(d.rename(a, "").is_err());
    assert_eq!(d.dependents(a), vec![u, m]);
    assert_eq!(d.dependents(m), Vec::<FeatureId>::new());
    match d.delete(a) {
        Err(FeatureError::HasDependents(ids)) => assert_eq!(ids, vec![u.0, m.0]),
        other => panic!("{other:?}"),
    }
    assert_eq!(d.features().len(), 4, "unverändert");
    d.delete(m).unwrap();
    let removed = d.delete_with_dependents(a).unwrap();
    assert_eq!(removed, vec![a, u]);
    assert_eq!(d.features().len(), 1);
    assert_eq!(d.marker(), 1);
    d.check_invariants().unwrap();
    assert_eq!(d.visible_bodies().len(), 1);
}

#[test]
fn inserting_at_the_marker_only_allows_earlier_references() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    d.set_marker(1).unwrap();
    let r = d.add_feature("M", FeatureKind::Move { body: BodyRef { feature: b, index: 0 }, delta: DVec3::X });
    assert!(matches!(r, Err(FeatureError::ForwardReference)), "{r:?}");
    let m = d.add_feature("M", FeatureKind::Move { body: BodyRef { feature: a, index: 0 }, delta: DVec3::X }).unwrap();
    assert_eq!(d.index_of(m), Some(1));
    d.check_invariants().unwrap();
}
```

(`update_with_report` ist die Variante von `update`, die den `RecomputeReport` des Laufs zurückgibt; `update` ruft sie auf und verwirft ihn.)

`crates/planora-feature/tests/chain.rs`:

```rust
use planora_feature::*;
use planora_kernel::{BoolOp, Kernel};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::model::{CurveKind, Sketch};

fn volume(s: &planora_kernel::Solid) -> f64 {
    mass_properties(&tessellate(s, &TessellationOptions { chord_tolerance: 1e-3 }).unwrap()).volume
}

fn rect(w: f64, h: f64) -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(w, h)).unwrap();
    sk
}

fn sketch_feature(sk: Sketch) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature { sketch: sk, plane: SketchPlane::Standard(StandardPlane::XY), copies: vec![] })
}

fn z_range(s: &planora_kernel::Solid) -> (f64, f64) {
    let zs: Vec<f64> = s.vertices.iter().map(|(_, v)| v.point.z).collect();
    (zs.iter().cloned().fold(f64::MAX, f64::min), zs.iter().cloned().fold(f64::MIN, f64::max))
}

#[test]
fn changing_a_sketch_or_a_height_updates_the_whole_chain() {
    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", sketch_feature(rect(40.0, 30.0))).unwrap();
    let e = d.add_feature("Extrusion 1", FeatureKind::Extrude { sketch: s, profile: 0, height: 10.0 }).unwrap();
    assert!((volume(d.visible_bodies()[0].solid) - 12000.0).abs() < 1e-3);
    d.update(e, |k| {
        if let FeatureKind::Extrude { height, .. } = k {
            *height = 20.0;
        }
    })
    .unwrap();
    assert!((volume(d.visible_bodies()[0].solid) - 24000.0).abs() < 1e-3);
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch = rect(50.0, 30.0);
        }
    })
    .unwrap();
    assert!((volume(d.visible_bodies()[0].solid) - 30000.0).abs() < 1e-3);
}

fn deckel_ref(d: &Document, extrude: FeatureId) -> FaceRef {
    let solid = d.body(BodyRef { feature: extrude, index: 0 }).unwrap();
    let (id, _) = solid.faces.iter().find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == "Deckel")).unwrap();
    FaceRef::from_face(solid, id, &Tolerance::default()).unwrap()
}

fn circle_sketch() -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let c = sk.add_point(20.0, 15.0).unwrap();
    sk.add_circle(c, 5.0).unwrap();
    sk
}

#[test]
fn a_sketch_on_a_face_follows_the_body() {
    let mut d = Document::new();
    let s1 = d.add_feature("Skizze 1", sketch_feature(rect(40.0, 30.0))).unwrap();
    let e1 = d.add_feature("Extrusion 1", FeatureKind::Extrude { sketch: s1, profile: 0, height: 10.0 }).unwrap();
    let face = deckel_ref(&d, e1);
    let s2 = d
        .add_feature("Skizze 2", FeatureKind::Sketch(SketchFeature { sketch: circle_sketch(), plane: SketchPlane::OnFace(face), copies: vec![] }))
        .unwrap();
    let e2 = d.add_feature("Extrusion 2", FeatureKind::Extrude { sketch: s2, profile: 0, height: 5.0 }).unwrap();
    assert_eq!(d.feature(e2).unwrap().status, FeatureStatus::Ok, "{:?}", d.feature(e2).unwrap().status);
    let (z0, z1) = z_range(d.body(BodyRef { feature: e2, index: 0 }).unwrap());
    assert!((z0 - 10.0).abs() < 1e-9 && (z1 - 15.0).abs() < 1e-9, "{z0} {z1}");
    // erste Extrusion höher → die Skizze auf der Deckelfläche folgt
    d.update(e1, |k| {
        if let FeatureKind::Extrude { height, .. } = k {
            *height = 20.0;
        }
    })
    .unwrap();
    let (z0, z1) = z_range(d.body(BodyRef { feature: e2, index: 0 }).unwrap());
    assert!((z0 - 20.0).abs() < 1e-9 && (z1 - 25.0).abs() < 1e-9, "{z0} {z1}");
    // und beide lassen sich vereinigen
    let u = d.add_feature("Vereinigung", FeatureKind::Boolean { target: BodyRef { feature: e1, index: 0 }, tool: BodyRef { feature: e2, index: 0 }, op: BoolOp::Union }).unwrap();
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok, "{:?}", d.feature(u).unwrap().status);
    let want = 40.0 * 30.0 * 20.0 + std::f64::consts::PI * 25.0 * 5.0;
    assert!((volume(d.visible_bodies()[0].solid) - want).abs() < 1.0);
}

#[test]
fn a_lost_face_is_reported_at_its_feature_and_dependents_are_skipped() {
    let mut d = Document::new();
    let s1 = d.add_feature("Skizze 1", sketch_feature(rect(40.0, 30.0))).unwrap();
    let e1 = d.add_feature("Extrusion 1", FeatureKind::Extrude { sketch: s1, profile: 0, height: 10.0 }).unwrap();
    let mut face = deckel_ref(&d, e1);
    face.role = "Wand 9".into();
    let s2 = d
        .add_feature("Skizze 2", FeatureKind::Sketch(SketchFeature { sketch: circle_sketch(), plane: SketchPlane::OnFace(face), copies: vec![] }))
        .unwrap();
    let e2 = d.add_feature("Extrusion 2", FeatureKind::Extrude { sketch: s2, profile: 0, height: 5.0 }).unwrap();
    assert!(matches!(&d.feature(s2).unwrap().status, FeatureStatus::Error(m) if m.contains("Wand 9")), "{:?}", d.feature(s2).unwrap().status);
    assert!(matches!(d.feature(e2).unwrap().status, FeatureStatus::Skipped(_)));
    assert_eq!(d.feature(e1).unwrap().status, FeatureStatus::Ok, "der Rest bleibt benutzbar");
    assert_eq!(d.visible_bodies().len(), 1);
}

#[test]
fn copied_body_edges_follow_a_resized_body() {
    let mut d = Document::new();
    let b = d.add_feature("Quader", FeatureKind::Box { size: DVec3::new(10.0, 20.0, 5.0), origin: DVec3::ZERO }).unwrap();
    let solid = d.body(BodyRef { feature: b, index: 0 }).unwrap().clone();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let tol = Tolerance::default();
    let mut copy = None;
    for (id, _) in solid.edges.iter() {
        if let Ok(c) = sk.add_projected_edge(&solid, id)
            && matches!(sk.curve(c).unwrap().kind, CurveKind::Line { .. })
            && (sk.line_length(c).unwrap() - 10.0).abs() < 1e-9
        {
            copy = Some((c, EdgeRef::from_edge(&solid, id, &tol).unwrap()));
            break;
        }
    }
    let (curve, edge) = copy.expect("eine Kante der Länge 10");
    let s = d
        .add_feature("Skizze", FeatureKind::Sketch(SketchFeature { sketch: sk, plane: SketchPlane::Standard(StandardPlane::XY), copies: vec![EdgeCopy { curve, edge }] }))
        .unwrap();
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = 12.0;
        }
    })
    .unwrap();
    assert_eq!(d.feature(s).unwrap().status, FeatureStatus::Ok, "{:?}", d.feature(s).unwrap().status);
    let sk = d.sketch_of(s).unwrap();
    assert!((sk.line_length(curve).unwrap() - 12.0).abs() < 1e-7, "die Kantenkopie ist mitgewachsen");
}
```

`crates/planora-feature/tests/properties.rs`:

```rust
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestRng, TestRunner};
use std::cell::Cell;

#[derive(Clone, Debug)]
enum Op {
    AddBox(f64),
    AddCylinder(f64, f64),
    AddBoolean(usize, usize, u8),
    AddMove(usize, f64),
    Resize(usize, f64),
    Rename(usize),
    Suppress(usize),
    Marker(usize),
    Delete(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1.0f64..10.0).prop_map(Op::AddBox),
        (1.0f64..5.0, 1.0f64..10.0).prop_map(|(r, h)| Op::AddCylinder(r, h)),
        (0usize..8, 0usize..8, 0u8..3).prop_map(|(a, b, o)| Op::AddBoolean(a, b, o)),
        (0usize..8, -5.0f64..5.0).prop_map(|(i, x)| Op::AddMove(i, x)),
        (0usize..8, 1.0f64..10.0).prop_map(|(i, s)| Op::Resize(i, s)),
        (0usize..8).prop_map(Op::Rename),
        (0usize..8).prop_map(Op::Suppress),
        (0usize..9).prop_map(Op::Marker),
        (0usize..8).prop_map(Op::Delete),
    ]
}

fn apply(d: &mut Document, op: &Op) {
    let nth = |d: &Document, i: usize| d.features().get(i % d.features().len().max(1)).map(|f| f.id);
    match op {
        Op::AddBox(s) => {
            let _ = d.add_feature("Quader", FeatureKind::Box { size: DVec3::splat(*s), origin: DVec3::ZERO });
        }
        Op::AddCylinder(r, h) => {
            let _ = d.add_feature("Zylinder", FeatureKind::Cylinder { radius: *r, height: *h, origin: DVec3::new(2.0, 2.0, 0.0) });
        }
        Op::AddBoolean(a, b, o) => {
            let vis: Vec<BodyRef> = d.visible_bodies().iter().map(|v| v.body).collect();
            if vis.len() >= 2 {
                let op = [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][*o as usize];
                let _ = d.add_feature("Boolesch", FeatureKind::Boolean { target: vis[a % vis.len()], tool: vis[(b + 1) % vis.len()], op });
            }
        }
        Op::AddMove(i, x) => {
            let vis: Vec<BodyRef> = d.visible_bodies().iter().map(|v| v.body).collect();
            if !vis.is_empty() {
                let _ = d.add_feature("Verschieben", FeatureKind::Move { body: vis[i % vis.len()], delta: DVec3::new(*x, 0.0, 0.0) });
            }
        }
        Op::Resize(i, s) => {
            if let Some(id) = nth(d, *i) {
                let _ = d.update(id, |k| {
                    if let FeatureKind::Box { size, .. } = k {
                        *size = DVec3::splat(*s);
                    }
                });
            }
        }
        Op::Rename(i) => {
            if let Some(id) = nth(d, *i) {
                let _ = d.rename(id, "Neuer Name");
            }
        }
        Op::Suppress(i) => {
            if let Some(id) = nth(d, *i) {
                let now = d.feature(id).unwrap().suppressed;
                let _ = d.set_suppressed(id, !now);
            }
        }
        Op::Marker(n) => {
            let _ = d.set_marker((*n).min(d.features().len()));
        }
        Op::Delete(i) => {
            if let Some(id) = nth(d, *i) {
                let _ = d.delete_with_dependents(id);
            }
        }
    }
}

#[test]
fn random_edit_sequences_keep_the_document_valid() {
    let checked = Cell::new(0usize);
    let mut runner = TestRunner::new_with_rng(
        Config { cases: 40, failure_persistence: None, rng_seed: RngSeed::Fixed(11), ..Config::default() },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    runner
        .run(&proptest::collection::vec(op(), 1..12), |ops| {
            let mut d = Document::new();
            for o in &ops {
                apply(&mut d, o);
                prop_assert!(d.check_invariants().is_ok(), "{:?} nach {:?}", d.check_invariants(), o);
            }
            let again = d.recompute();
            prop_assert!(again.computed.is_empty(), "zweiter Lauf rechnet {:?}", again.computed);
            checked.set(checked.get() + 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(checked.get(), 40);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-feature`
Expected: FAIL

- [ ] **Step 3: Implementierung** – `ops.rs` (Referenz, Kern)

```rust
impl Document {
    pub fn update_with_report(
        &mut self,
        id: FeatureId,
        f: impl FnOnce(&mut FeatureKind),
    ) -> Result<RecomputeReport, FeatureError> {
        let idx = self.index_of(id).ok_or(FeatureError::UnknownFeature)?;
        let mut kind = self.features[idx].kind.clone();
        let before = std::mem::discriminant(&kind);
        f(&mut kind);
        if std::mem::discriminant(&kind) != before {
            return Err(FeatureError::InvalidInput("Die Art des Features lässt sich nicht ändern".into()));
        }
        self.check_references(&kind, idx)?; // alle Abhängigkeiten existieren und liegen vor `idx`
        let version = self.bump();
        let feature = &mut self.features[idx];
        feature.kind = kind;
        feature.version = version;
        Ok(self.recompute())
    }

    pub fn update(&mut self, id: FeatureId, f: impl FnOnce(&mut FeatureKind)) -> Result<(), FeatureError> {
        self.update_with_report(id, f).map(|_| ())
    }

    pub fn dependents(&self, id: FeatureId) -> Vec<FeatureId> {
        let mut found: Vec<FeatureId> = Vec::new();
        for f in &self.features {
            let direct = f.kind.dependencies().iter().any(|d| *d == id || found.contains(d));
            if direct {
                found.push(f.id);
            }
        }
        found
    }

    pub fn delete_with_dependents(&mut self, id: FeatureId) -> Result<Vec<FeatureId>, FeatureError> {
        let idx = self.index_of(id).ok_or(FeatureError::UnknownFeature)?;
        let mut removed = vec![id];
        removed.extend(self.dependents(id));
        let before_marker = self.features[..self.marker.min(self.features.len())]
            .iter()
            .filter(|f| removed.contains(&f.id))
            .count();
        self.features.retain(|f| !removed.contains(&f.id));
        self.marker -= before_marker;
        let _ = idx;
        self.recompute();
        Ok(removed)
    }
}
```

(`dependents` liefert wegen der Zeitleistenreihenfolge und weil Verweise nur nach vorn zeigen, die transitive Hülle in einem Durchlauf. `delete` ruft `dependents` und gibt `HasDependents` zurück, sonst `delete_with_dependents`.) `check_invariants` prüft die Punkte der Schnittstelle (Validator über `planora_kernel::validate`).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all && cargo deny check`
Expected: PASS (Tests laufen deterministisch, `properties` unter 30 s im Debug-Build)

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "feat(feature): Änderungsoperationen, Löschen mit Folgen, Ende-zu-Ende- und Eigenschaftstests"
```

---

### Task 7: App – Körper aus dem `Document` ableiten (Grundkörper, Boolesche Operation, Verschieben)

**Files:**
- Modify: `crates/planora-feature/src/recompute.rs`/`model.rs` (`visible_bodies_before`), `crates/planora-app/Cargo.toml` (`planora-feature`), `crates/planora-app/src/state.rs`, `crates/planora-app/src/create_dialog.rs`, `crates/planora-app/src/boolean_dialog.rs`, `crates/planora-app/src/properties.rs`, `crates/planora-app/src/browser.rs` (nur Anpassung an die abgeleitete Körperliste), `crates/planora-app/src/app.rs`, `crates/planora-app/src/icons.rs` (`BodyKind::Combined`), `crates/planora-app/src/lib.rs`
- Delete: `crates/planora-app/src/samples.rs`, `crates/planora-app/tests/samples.rs` (Entscheidung F12)
- Modify (Tests mechanisch auf die neue API portieren, Absicht erhalten): `crates/planora-app/tests/state.rs`, `boolean_state.rs`, `boolean_ui.rs`, `sketch_dialogs.rs`, weitere, die `cargo test --workspace` als rot zeigt
- Test: `crates/planora-app/tests/feature_state.rs`

**Interfaces:**
- Consumes: `planora-feature` (Tasks 4–6).
- Produces in `planora-feature`: `Document::visible_bodies_before(&self, id: FeatureId) -> Vec<VisibleBody<'_>>` (die Körper, die zum Zeitpunkt vor dem Feature `id` sichtbar sind; `UnknownFeature` → leere Liste).
- Produces in `AppState` (`state.rs`):
  - `pub document: Document` (`Document::with_options(chord_tolerance)`), `pub bodies: Vec<Body>` ist **abgeleitet**: `Body { name, kind: BodyKind, source: BodyRef, solid, mesh, mass, bounds }` (`source` neu; `kind` aus der Art des erzeugenden Features: Box/Cylinder/Sphere/Cone/Extrusion/Revolution, `Boolean` und `Move` → `BodyKind::Combined` (neu, mit einfachem Symbol), `Move` übernimmt die Art des verschobenen Körpers, wenn bekannt).
  - `pub fn refresh(&mut self)`: baut `bodies`, Netz, Maße und Szene aus `document.visible_bodies()` neu auf (Tessellierung bei `chord_tolerance`; schlägt sie fehl, bleibt der Körper ohne Netz weg und die Meldung steht in `message`); behält `selected_body`/`selection`/`hover`, wenn der Körper (`BodyRef`) weiter sichtbar ist (Körperindizes in `Pick`-Werten werden auf die neuen Indizes abgebildet), sonst werden sie gelöscht. `frame_all()` wird nur aufgerufen, wenn sich die Zahl der sichtbaren Körper geändert hat.
  - `pub fn add_feature(&mut self, base_name: Option<&str>, kind: FeatureKind) -> Result<FeatureId, String>`: Name `{Bezeichnung} {Nummer}` mit einem Zähler je Bezeichnung (`Quader 1`, `Quader 2`, `Zylinder 1`, …; zählt nur hoch), `document.add_feature`, `refresh()`, Fehler als deutscher Text (und in `message`). Hat das neue Feature den Status `Error`, wird es trotzdem angelegt und der Text als Meldung gezeigt.
  - `pub fn free_origin(&self, extent_x: f64) -> DVec3`: Ursprung für einen neuen Grundkörper neben den vorhandenen (rechts von der größten `bounds.1.x` plus 10 mm Abstand, `y = z = 0`; ohne Körper `ZERO`).
  - `pub fn create_box(&mut self, size: DVec3)`, `create_cylinder(radius, height)`, `create_sphere(radius)`, `create_cone(radius, height)` → `Result<FeatureId, String>`: bestimmen `origin` über `free_origin(…)` (Kugel: Mittelpunkt `origin + (radius, 0, 0)` bzw. so, dass die linke Kante am Rand liegt) und rufen `add_feature`.
  - `pub fn add_boolean(&mut self, a: usize, b: usize, op: BoolOp) -> Result<FeatureId, String>` (Körperindizes; gleiche Körper/unbekannte Indizes → „Bitte zwei verschiedene Körper wählen“, Dokument unverändert); nach Erfolg `selected_body` = Ergebnis. Die Volumenprobe läuft im Feature (F4).
  - `pub fn move_body(&mut self, index: usize, delta: DVec3) -> Result<(), String>` (Entscheidung F9): `delta` ist die **zusätzliche** Verschiebung gegenüber dem derzeitigen Stand (die Oberfläche rechnet „neue Position − alte Position“); ist das letzte Feature, das diesen Körper liest, ein `Move` desselben Körpers, wird dessen `delta` um den Wert erhöht (kein neues Feature); sonst wird ein `Move` am Marker angelegt. Nicht endliche Werte → Fehler, Dokument unverändert.
  - `pub fn rename_feature`, `delete_feature(id) -> Result<(), String>` (liefert bei Folgen einen Fehlertext mit der Liste der abhängigen Namen), `delete_feature_with_dependents(id) -> Result<Vec<FeatureId>, String>`, jeweils mit `refresh()`.
  - Entfernt: `add_body`, `add_body_in_place`, `boolean`, `translate_body`, `remove_body`, `next_body_number`. Die Körperliste wird nirgends mehr direkt verändert.
  - `extrude_sketch`/`revolve_sketch` bleiben für die noch vorhandenen `AppState::sketches` und legen übergangsweise (F13) ein Skizzen-Feature (Kopie der Skizze, `SketchPlane::Standard(XY)` bzw. die Ebene der Skizze, sofern sie einer Standardebene gleicht, sonst `Standard(XY)` mit gesetztem `Frame`; die berechnete Skizze trägt das Frame ohnehin) und das Extrusions-/Rotationsfeature an.
- Oberfläche: Die Erzeugen-Dialoge (Quader, Zylinder, Kugel, Kegel) rufen `create_*`; die Beispielprofil-Dialoge entfallen; der Boolesche Dialog ruft `add_boolean`; das Eigenschaftenfenster schreibt Name über `rename_feature` des erzeugenden Features, Position über `move_body`, „Löschen“ über `delete_feature` (bei Folgen Hinweis mit den Namen statt Löschen; die Rückfrage-Oberfläche folgt in Task 9); Maße (Quader-Größe etc.) werden hier nur angezeigt.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-app/tests/feature_state.rs`

```rust
use planora_app::state::{AppState, BodyKind};
use planora_feature::{FeatureKind, FeatureStatus};
use planora_kernel::BoolOp;
use planora_math::DVec3;
use planora_mesh::{mass_properties, tessellate, TessellationOptions};

fn volume(s: &AppState, i: usize) -> f64 {
    mass_properties(&tessellate(&s.bodies[i].solid, &TessellationOptions { chord_tolerance: 1e-3 }).unwrap()).volume
}

#[test]
fn primitives_become_features_and_stand_side_by_side() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(10.0)).unwrap();
    let b = s.create_box(DVec3::splat(10.0)).unwrap();
    assert_ne!(a, b);
    assert_eq!(s.document.features().len(), 2);
    assert_eq!(s.bodies.len(), 2);
    assert_eq!(s.bodies[0].name, "Quader 1");
    assert_eq!(s.bodies[1].name, "Quader 2");
    assert!(s.bodies[1].bounds.0.x > s.bodies[0].bounds.1.x, "nebeneinander");
    assert_eq!(s.bodies[0].kind, BodyKind::Box);
    s.create_cylinder(2.0, 5.0).unwrap();
    s.create_sphere(3.0).unwrap();
    s.create_cone(2.0, 4.0).unwrap();
    assert_eq!(s.bodies.len(), 5);
    assert_eq!(s.bodies[2].name, "Zylinder 1");
}

#[test]
fn a_boolean_replaces_its_inputs_and_errors_leave_the_document_alone() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_cylinder(2.0, 14.0).unwrap();
    // Zylinder in den Quader schieben
    s.move_body(1, DVec3::new(-7.0, 5.0, -2.0)).unwrap();
    let features_before = s.document.features().len();
    assert!(s.add_boolean(0, 0, BoolOp::Union).is_err());
    assert!(s.add_boolean(0, 9, BoolOp::Union).is_err());
    assert_eq!(s.document.features().len(), features_before);
    let f = s.add_boolean(0, 1, BoolOp::Difference).unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.selected_body, Some(0));
    assert_eq!(s.bodies[0].source.feature, f);
    assert!((volume(&s, 0) - (1000.0 - std::f64::consts::PI * 4.0 * 10.0)).abs() < 0.6);
    assert_eq!(s.bodies[0].kind, BodyKind::Combined);
}

#[test]
fn moving_a_body_edits_the_last_move_feature_instead_of_stacking_new_ones() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    let x0 = s.bodies[0].bounds.0.x;
    s.move_body(0, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    assert_eq!(s.document.features().len(), 2);
    s.move_body(0, DVec3::new(2.0, 0.0, 0.0)).unwrap();
    assert_eq!(s.document.features().len(), 2, "kein weiteres Feature");
    assert!((s.bodies[0].bounds.0.x - (x0 + 7.0)).abs() < 1e-6);
    match &s.document.features()[1].kind {
        FeatureKind::Move { delta, .. } => assert!((delta.x - 7.0).abs() < 1e-9),
        k => panic!("{k:?}"),
    }
    assert!(s.move_body(0, DVec3::new(f64::NAN, 0.0, 0.0)).is_err());
    assert!(s.move_body(5, DVec3::X).is_err());
    assert_eq!(s.document.features().len(), 2);
}

#[test]
fn moving_after_a_foreign_feature_read_the_body_adds_a_new_move() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    s.move_body(0, DVec3::X).unwrap(); // Move für Körper 0 (Feature 3)
    s.add_boolean(0, 1, BoolOp::Union).unwrap_or_default(); // liest beide (ob erfolgreich oder nicht)
    let n = s.document.features().len();
    s.move_body(0, DVec3::Y).unwrap();
    assert!(s.document.features().len() >= n, "ein weiteres Feature entsteht oder der letzte Move wird erweitert, nie beides verloren");
}

#[test]
fn editing_the_document_and_refreshing_updates_the_bodies_and_keeps_the_selection() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(4.0)).unwrap();
    s.selected_body = Some(0);
    s.document
        .update(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                *size = DVec3::splat(6.0);
            }
        })
        .unwrap();
    s.refresh();
    assert!((volume(&s, 0) - 216.0).abs() < 1e-6);
    assert_eq!(s.selected_body, Some(0));
}

#[test]
fn deleting_features_removes_their_bodies_and_reports_dependents() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    s.add_boolean(0, 1, BoolOp::Union).unwrap_or_default();
    let e = s.delete_feature(a).unwrap_err();
    assert!(e.contains("Quader 1") || e.contains("Boolesche"), "{e}");
    let removed = s.delete_feature_with_dependents(a).unwrap();
    assert!(removed.len() >= 1);
    assert!(s.selected_body.is_none() || s.selected_body.unwrap() < s.bodies.len());
    for f in s.document.features() {
        assert!(!matches!(f.status, FeatureStatus::Error(_)), "{f:?}");
    }
}

#[test]
fn rename_goes_through_the_feature() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.rename_feature(a, "Grundplatte").unwrap();
    assert_eq!(s.bodies[0].name, "Grundplatte");
    assert!(s.rename_feature(a, "  ").is_err());
}
```

(`add_boolean(..).unwrap_or_default()` setzt `Result<FeatureId, String>: Default` nicht voraus – stattdessen `let _ = s.add_boolean(..)` verwenden; die Zeilen sind Ausdrucksform, nicht Vorgabe.)

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-app --test feature_state`
Expected: FAIL

- [ ] **Step 3: Implementierung**

Reihenfolge: (a) `visible_bodies_before` in `planora-feature` (+ Test in `tests/recompute.rs`: vor einer Booleschen Operation sind beide Eingaben sichtbar, danach nur das Ergebnis); (b) `AppState`-Umbau (`document`, `refresh`, `add_feature`, `create_*`, `add_boolean`, `move_body`, `rename/delete`), Entfernen der alten Körperfunktionen; (c) Dialoge, Eigenschaften, Browser, Menüs anpassen; (d) Tests portieren. Portierungsregel: Wo Tests bisher `state.add_body(kind, name, solid)` mit einem fertigen Körper aufrufen, legen sie jetzt das passende Feature an (`create_*`, oder `document.add_feature` mit `FeatureKind::Box { .. }`); Tests, die genau die entfernten Beispielprofil-Dialoge prüfen, entfallen mit `samples.rs`. Verhalten, Meldungstexte und Reihenfolgen der Körper bleiben (Namen „Quader 1“ statt „Quader 1“ unverändert). `BodyKind::Combined` bekommt in `icons.rs` ein einfaches, selbst gezeichnetes Symbol (zwei überlappende Quadrate).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all && cargo deny check`
Expected: PASS

- [ ] **Step 5: App einmal starten** (Schnelltest): Quader erzeugen, Zylinder erzeugen, Position ändern, Abziehen; Ergebnis kurz beschreiben (Bildschirmfoto optional). Kein GUI-Prozess bleibt laufen.

- [ ] **Step 6: Commit**

```bash
git add -A crates
git commit -m "feat(app): Körperliste aus dem Feature-Dokument ableiten (Grundkörper, Boolesche Operation, Verschieben)"
```

---

### Task 8: App – Skizzen als Features (Entwurf, Skizze auf Fläche, Kantenkopien)

**Files:**
- Modify: `crates/planora-app/src/state.rs`, `crates/planora-app/src/sketch_state.rs`, `crates/planora-app/src/sketch_view.rs`, `crates/planora-app/src/sketch_panel.rs`, `crates/planora-app/src/browser.rs`, `crates/planora-app/src/create_dialog.rs`, `crates/planora-app/src/app.rs`
- Modify (Tests): `crates/planora-app/tests/sketch_state.rs`, `sketch_ui.rs`, `sketch_dialogs.rs`, `sketch_view.rs` (nur wo `AppState::sketches`/Indizes benutzt werden)
- Test: `crates/planora-app/tests/sketch_features.rs`

**Interfaces:**
- Consumes: Tasks 3, 4–7.
- Produces in `AppState`:
  - **Entfernt:** `sketches: Vec<Sketch>`, `selected_sketch: Option<usize>`, `next_sketch_number`, alle Index-API (`new_sketch(frame)`, `delete_sketch(index)`, `new_sketch_on_face(body, face)` mit Index-Rückgabe, `extrude_sketch(index, …)`, `revolve_sketch(index, …)`, `SketchRequest` mit Indizes). Ersatz (alle mit `FeatureId`):
  - `pub selected_feature: Option<FeatureId>` (löst `selected_sketch` ab).
  - `add_sketch(&mut self, plane: SketchPlane) -> Result<FeatureId, String>` (Name `Skizze {n}`; leere Skizze mit dem Frame der Ebene; `OnFace` → Ebene löst das Dokument auf), `new_sketch_on_face(&mut self, body: usize, face: Id<Face>) -> Result<FeatureId, String>` (nur ebene markierte Flächen; baut `FaceRef::from_face` aus dem **sichtbaren** Körper), `open_sketch(&mut self, id: FeatureId) -> Result<(), String>`, `close_sketch(&mut self)`, `delete_sketch`: ersetzt durch `delete_feature*` aus Task 7.
  - **Entwurf (F6):** `pub sketch_draft: Option<SketchFeature>`. `open_sketch` klont das Skizzen-Feature: `sketch` = **berechnete** Skizze (`document.sketch_of(id)`, enthält die aufgelöste Ebene), `plane` und `copies` aus der Definition. Alle Skizzen-Modus-Funktionen arbeiten auf `sketch_draft.sketch`. `close_sketch` schreibt den Entwurf mit `document.update(id, |k| *k = FeatureKind::Sketch(draft))` zurück (Ebene und Kopien bleiben) und ruft `refresh()`. Während des Zeichnens wird nicht neu berechnet.
  - `SketchSession.sketch: FeatureId` (statt Index); `AppState::session_sketch(&self) -> Option<&Sketch>` und `session_sketch_mut(&mut self) -> Option<(&mut SketchSession, &mut Sketch)>` liefern Sitzung und Entwurf gemeinsam (statt `sketches.get_mut(index)`).
  - `extrude_sketch(&mut self, sketch: FeatureId, profile: usize, height: f64) -> Result<FeatureId, String>` und `revolve_sketch(&mut self, sketch: FeatureId, profile: usize, axis: CurveId) -> Result<FeatureId, String>` legen die Features an (`add_feature`), die Profile für die Dialoge kommen aus `document.sketch_of(sketch)`. Sie beenden vorher eine offene Skizze (R12).
  - **Kantenkopien:** `project_edge_into_sketch(&mut self, body: usize, edge: Id<Edge>) -> Result<(), String>`: nur Körper, die **vor** dem Skizzen-Feature sichtbar sind (`visible_bodies_before(id)`), sonst Fehler „Dieser Körper entsteht erst nach der Skizze“; legt die Kante mit `SketchSession::project_edge` an und trägt `EdgeCopy { curve, edge: EdgeRef::from_edge(..) }` in `sketch_draft.copies` ein.
  - Skizzenliste im Browser (bis Task 9): Zeilen aus den Skizzen-Features der Zeitleiste (`kind == Sketch`), Auswahl über `selected_feature`, Doppelklick → `open_sketch`.
- **Solver-/Sitzungs-Verhalten bleibt unverändert** (nur die Quelle der Skizze wechselt). Die Kamera-Ausrichtung beim Öffnen nimmt den Frame der berechneten Skizze.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-app/tests/sketch_features.rs`

```rust
use planora_app::sketch_state::Tool;
use planora_app::state::AppState;
use planora_feature::{FeatureKind, FeatureStatus, SketchPlane, StandardPlane};
use planora_kernel::Surface;
use planora_math::{DVec2, DVec3};

fn volume(s: &AppState, i: usize) -> f64 {
    use planora_mesh::{TessellationOptions, mass_properties, tessellate};
    mass_properties(&tessellate(&s.bodies[i].solid, &TessellationOptions { chord_tolerance: 1e-3 }).unwrap()).volume
}

fn draw_rectangle(s: &mut AppState, w: f64, h: f64) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, Tool::Rectangle);
    se.click(sk, DVec2::new(0.0, 0.0), 0.5, false).unwrap();
    se.click(sk, DVec2::new(w, h), 0.5, false).unwrap();
}

#[test]
fn a_sketch_becomes_a_feature_and_closing_writes_the_draft_back() {
    let mut s = AppState::new();
    let id = s.add_sketch(SketchPlane::Standard(StandardPlane::XY)).unwrap();
    assert_eq!(s.document.feature(id).unwrap().name, "Skizze 1");
    s.open_sketch(id).unwrap();
    assert!(s.sketch_draft.is_some());
    let to_cam = (s.camera.eye() - s.camera.target).normalize();
    assert!(to_cam.z > 0.99, "Draufsicht: {to_cam:?}");
    draw_rectangle(&mut s, 40.0, 30.0);
    assert!(s.document.sketch_of(id).unwrap().curves().count() == 0, "vor dem Beenden unverändert");
    s.close_sketch();
    assert!(s.sketch_session.is_none() && s.sketch_draft.is_none());
    assert_eq!(s.document.sketch_of(id).unwrap().curves().count(), 4);
}

#[test]
fn extruding_and_editing_the_sketch_updates_the_body() {
    let mut s = AppState::new();
    let id = s.add_sketch(SketchPlane::Standard(StandardPlane::XY)).unwrap();
    s.open_sketch(id).unwrap();
    draw_rectangle(&mut s, 40.0, 30.0);
    s.close_sketch();
    let e = s.extrude_sketch(id, 0, 10.0).unwrap();
    assert_eq!(s.document.feature(e).unwrap().status, FeatureStatus::Ok);
    assert!((volume(&s, 0) - 12000.0).abs() < 1e-3);
    // Skizze wieder öffnen, die rechte obere Ecke verschieben, beenden → Körper folgt
    s.open_sketch(id).unwrap();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let p = sk.points().map(|(i, _)| i).nth(2).unwrap();
        sk.set_point_xy(p, 50.0, 30.0).unwrap();
        let _ = se;
    }
    s.close_sketch();
    assert!(volume(&s, 0) > 12000.0 + 1.0, "{}", volume(&s, 0));
}

#[test]
fn a_sketch_on_a_face_follows_a_resized_body() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let top = s.bodies[0]
        .solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Plane { .. }) && f.tag.as_ref().is_some_and(|t| t.role == "Deckel"))
        .map(|(i, _)| i)
        .unwrap();
    let id = s.new_sketch_on_face(0, top).unwrap();
    let z0 = s.document.sketch_of(id).unwrap().frame().origin.z;
    assert!((z0 - 5.0).abs() < 1e-9);
    s.document
        .update(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.z = 9.0;
            }
        })
        .unwrap();
    s.refresh();
    let z1 = s.document.sketch_of(id).unwrap().frame().origin.z;
    assert!((z1 - 9.0).abs() < 1e-9, "die Skizzenebene ist mitgewandert: {z1}");
    // nicht ebene Flächen sind abgelehnt
    s.create_cylinder(2.0, 4.0).unwrap();
    let side = s.bodies[1].solid.faces.iter().find(|(_, f)| !matches!(f.surface, Surface::Plane { .. })).map(|(i, _)| i).unwrap();
    assert!(s.new_sketch_on_face(1, side).is_err());
}

#[test]
fn copied_edges_are_recorded_and_follow_the_body() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let id = s.add_sketch(SketchPlane::Standard(StandardPlane::XY)).unwrap();
    s.open_sketch(id).unwrap();
    let solid = s.bodies[0].solid.clone();
    let mut done = None;
    for (eid, _) in solid.edges.iter() {
        if s.project_edge_into_sketch(0, eid).is_ok() {
            done = Some(eid);
            break;
        }
    }
    assert!(done.is_some());
    assert_eq!(s.sketch_draft.as_ref().unwrap().copies.len(), 1);
    s.close_sketch();
    s.document
        .update(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.x = 14.0;
                size.y = 24.0;
            }
        })
        .unwrap();
    s.refresh();
    assert_eq!(s.document.feature(id).unwrap().status, FeatureStatus::Ok);
    let sk = s.document.sketch_of(id).unwrap();
    let longest = sk.curves().filter_map(|(c, _)| sk.line_length(c)).fold(0.0_f64, f64::max);
    assert!(longest > 13.9, "die Kopie ist mitgewachsen: {longest}");
}

#[test]
fn bodies_created_after_a_sketch_cannot_be_projected_into_it() {
    let mut s = AppState::new();
    let id = s.add_sketch(SketchPlane::Standard(StandardPlane::XY)).unwrap();
    s.create_box(DVec3::splat(5.0)).unwrap(); // entsteht NACH der Skizze
    s.open_sketch(id).unwrap();
    let e = s.bodies[0].solid.edges.iter().next().map(|(i, _)| i).unwrap();
    let err = s.project_edge_into_sketch(0, e).unwrap_err();
    assert!(err.contains("nach der Skizze"), "{err}");
}

#[test]
fn deleting_an_open_sketch_feature_closes_the_session() {
    let mut s = AppState::new();
    let id = s.add_sketch(SketchPlane::Standard(StandardPlane::XZ)).unwrap();
    s.open_sketch(id).unwrap();
    s.delete_feature_with_dependents(id).unwrap();
    assert!(s.sketch_session.is_none() && s.sketch_draft.is_none());
    assert!(s.document.features().is_empty());
}
```

(`se.set_tool(sk, …)`: In der Sitzung bleibt die bestehende Signatur (`set_tool(&mut self, sk: &mut Sketch, tool)`), die Hilfsfunktion `session_sketch_mut` liefert beides; wenn die tatsächliche Signatur abweicht, die Tests entsprechend der vorhandenen API schreiben – die Aussagen bleiben.)

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-app --test sketch_features`
Expected: FAIL

- [ ] **Step 3: Implementierung**

Reihenfolge: (a) `AppState`-Felder und Methoden wie beschrieben; (b) `sketch_view.rs`/`sketch_panel.rs`/`sketch_state.rs`: überall `state.sketches.get_mut(index)` durch `session_sketch_mut()` ersetzen (Sitzung und Entwurf gemeinsam geborgt); `SketchSession.sketch: FeatureId`; die Skizzenfarben/Overlay lesen die berechneten Skizzen der **anderen** (nicht geöffneten) Skizzen-Features aus `document.sketch_of(id)` (Ersatz für die alte Liste `state.sketches`); (c) `create_dialog.rs` (Skizzen-Extrusion/Rotation) mit `FeatureId` statt Index; der Profil-Zwischenspeicher bleibt (Schlüssel: `Sketch::revision`); (d) `app.rs`: Menü „Skizze auf XY/XZ/YZ“ → `add_sketch(Standard(..))` und öffnen; „Skizze auf gewählter Fläche“ → `new_sketch_on_face`; (e) Browser: Skizzenzeilen aus den Features; (f) Tests anpassen: alle `AppState`-nahen Tests (Indizes → `FeatureId`); reine `SketchSession`-Tests auf einer eigenen `Sketch` bleiben.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all && cargo deny check`
Expected: PASS

- [ ] **Step 5: App von Hand prüfen** (Pflicht, mit Bildschirmfotos unter `.superpowers/sdd/<plan>/task-8-screens/`): Skizze auf XY zeichnen, beenden, extrudieren; Skizze wieder öffnen, Maß ändern, beenden → Körper folgt; Quader erzeugen, Skizze auf dessen Deckfläche anlegen, Quader später in den Eigenschaften verschieben (Position ändern) → Skizze folgt. Ehrlich berichten, was nicht ging.

- [ ] **Step 6: Commit**

```bash
git add -A crates
git commit -m "feat(app): Skizzen als Features (Entwurf, Skizze auf Fläche, Kantenkopien)"
```

---

### Task 9: App – Feature-Baum in der Oberfläche

**Files:**
- Create: `crates/planora-app/src/feature_dialog.rs` (Bearbeiten-Dialog)
- Modify: `crates/planora-app/src/browser.rs` (Baum), `crates/planora-app/src/properties.rs`, `crates/planora-app/src/app.rs`, `crates/planora-app/src/icons.rs`, `crates/planora-app/src/state.rs`, `crates/planora-app/src/lib.rs`
- Test: `crates/planora-app/tests/tree_ui.rs`, `crates/planora-app/tests/feature_dialog.rs`

**Interfaces:**
- Consumes: Tasks 4–8.
- Produces:
  - **Baum** (`browser::show`): eine Zeile je Feature der Zeitleiste (Symbol der Art, Name, Statusanzeige), eingerückte Unterzeilen für die **sichtbaren Körper**, die das Feature erzeugt (`Körper`-Name, Klick wählt den Körper: `selected_body`). Statusfarben: `Error` rot (Tooltip mit dem Text), `Skipped`/`Suppressed`/`NotComputed` grau (Tooltip mit Grund), `Ok` normal. Unter der Zeile an der Stelle des Markers eine Trennlinie „Berechnung endet hier“, wenn der Marker nicht am Ende steht. Klick auf eine Zeile setzt `selected_feature`; Doppelklick: Skizze → `open_sketch`; alle anderen Arten → Bearbeiten-Dialog.
  - **Kontextmenü je Zeile:** „Bearbeiten“ (wie Doppelklick), „Umbenennen“ (eigenes kleines Fenster mit Textfeld), „Unterdrücken“/„Aktivieren“, „Zurückrollen bis hier“ (`set_marker(index + 1)`), „Zum Ende“ (`set_marker(len)`), bei Skizzen zusätzlich „Extrudieren …“ und „Drehen …“ (öffnen die vorhandenen Dialoge, R12: beenden eine offene Skizze), „Löschen“. **Löschen** ohne Folgen löscht sofort; mit Folgen öffnet es eine Rückfrage „„{Name}“ und {n} davon abhängige Feature(s) werden gelöscht: {Namen}“ mit „Löschen“/„Abbrechen“.
  - `FeatureDialog` (`feature_dialog.rs`): `FeatureDialog::new(state: &AppState, id: FeatureId) -> Option<FeatureDialog>` (nicht für Skizzen), `show(&mut self, ctx, state: &mut AppState) -> bool` (offen), `apply(&self, state) -> Result<(), String>`. Felder je Art: Quader (Breite/Tiefe/Höhe, Position x/y/z), Zylinder/Kegel (Radius, Höhe, Position), Kugel (Radius, Position), Extrusion (Skizze, Profil, Höhe), Rotation (Skizze, Profil, Achse), Boolesche Operation (Ziel, Werkzeug, Operation – wählbar unter `document.visible_bodies_before(id)`), Verschieben (Δx/Δy/Δz). „Übernehmen“ ruft `document.update` und `refresh()`; ein anschließender Fehlerstatus des Features wird als Meldung gezeigt, der Dialog schließt trotzdem (der Fehler steht im Baum). Zahlen nehmen Komma und Punkt an (wie im Skizzen-Modus).
  - **Eigenschaften:** Für das gewählte Feature: Art, Name (umbenennbar), Status (Fehlertext rot), Parameter als Text; für den gewählten Körper wie bisher Maße, Fläche/Kante, Position (`move_body`), „Körper löschen“ entfällt (Löschen läuft über das Feature).
  - **Menü „Bearbeiten“:** „Vereinigen/Abziehen/Schneiden …“ wie bisher (Boolean-Dialog legt ein Feature an); neu: „Zum Ende der Zeitleiste“.
  - Statuszeile: zeigt den Text des ersten Features mit `Error` (rot), sonst „{n} Körper“.

- [ ] **Step 1: Failing tests schreiben**

`crates/planora-app/tests/feature_dialog.rs`:

```rust
use planora_app::feature_dialog::FeatureDialog;
use planora_app::state::AppState;
use planora_feature::{FeatureKind, FeatureStatus};
use planora_math::DVec3;

#[test]
fn the_dialog_edits_parameters_and_the_bodies_follow() {
    let mut s = AppState::new();
    let id = s.create_box(DVec3::splat(4.0)).unwrap();
    let mut d = FeatureDialog::new(&s, id).expect("Dialog für Quader");
    d.set_box_size(DVec3::new(5.0, 6.0, 7.0));
    d.apply(&mut s).unwrap();
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Box { size, .. } => assert_eq!(*size, DVec3::new(5.0, 6.0, 7.0)),
        k => panic!("{k:?}"),
    }
    assert!((s.bodies[0].bounds.1.z - s.bodies[0].bounds.0.z - 7.0).abs() < 1e-6);
}

#[test]
fn invalid_values_become_an_error_status_not_a_panic() {
    let mut s = AppState::new();
    let id = s.create_box(DVec3::splat(4.0)).unwrap();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_box_size(DVec3::new(-1.0, 6.0, 7.0));
    let r = d.apply(&mut s);
    assert!(r.is_err() || matches!(s.document.feature(id).unwrap().status, FeatureStatus::Error(_)));
    assert!(s.bodies.is_empty() || r.is_err());
}

#[test]
fn sketches_have_no_dialog_and_unknown_ids_none() {
    let mut s = AppState::new();
    let sk = s.add_sketch(planora_feature::SketchPlane::Standard(planora_feature::StandardPlane::XY)).unwrap();
    assert!(FeatureDialog::new(&s, sk).is_none());
    assert!(FeatureDialog::new(&s, planora_feature::FeatureId(99)).is_none());
}
```

`crates/planora-app/tests/tree_ui.rs` (Muster wie `sketch_dialogs.rs`/`boolean_ui.rs`: Fenster-lose egui-Läufe, Rechtsklick auf Zeilen über Textpositionen):

```rust
use planora_app::state::AppState;
use planora_app::{browser, fonts};
use planora_feature::FeatureStatus;
use planora_kernel::BoolOp;
use planora_math::DVec3;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn texts(out: &egui::FullOutput) -> String {
    // sammelt alle Texte der gezeichneten Formen (rekursiv, wie in sketch_dialogs.rs)
    planora_app::testing::collect_texts(out)
}

#[test]
fn the_tree_lists_features_bodies_and_the_marker() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    s.add_boolean(0, 1, BoolOp::Union).unwrap();
    let c = ctx();
    let mut text = String::new();
    for _ in 0..3 {
        let out = c.run_ui(egui::RawInput::default(), |ui| browser::show(ui, &mut s));
        text = texts(&out);
        out.drop_without_applying_deltas();
    }
    for must in ["Quader 1", "Zylinder 1", "Boolesche Operation 1"] {
        assert!(text.contains(must), "„{must}“ fehlt:\n{text}");
    }
    s.document.set_marker(2).unwrap();
    s.refresh();
    let out = c.run_ui(egui::RawInput::default(), |ui| browser::show(ui, &mut s));
    assert!(texts(&out).contains("Berechnung endet hier"));
}

#[test]
fn an_error_row_carries_its_reason_and_dependents_are_marked() {
    let mut s = AppState::new();
    let bad = s.add_feature(None, planora_feature::FeatureKind::Cylinder { radius: -1.0, height: 2.0, origin: DVec3::ZERO }).unwrap();
    assert!(matches!(s.document.feature(bad).unwrap().status, FeatureStatus::Error(_)));
    assert!(s.message.is_some(), "Fehler steht in der Statuszeile");
}

#[test]
fn delete_with_consequences_asks_first_and_then_removes_everything() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    s.add_boolean(0, 1, BoolOp::Union).unwrap();
    let plan = browser::delete_plan(&s, a);
    assert_eq!(plan.dependents.len(), 1);
    assert!(plan.text().contains("Quader 1") && plan.text().contains("Boolesche Operation"));
    browser::apply_delete(&mut s, &plan).unwrap();
    assert!(s.document.feature(a).is_none());
    assert!(s.document.features().iter().all(|f| f.name != "Boolesche Operation 1"));
}

#[test]
fn context_actions_work_on_the_state_level() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_box(DVec3::splat(2.0)).unwrap();
    browser::toggle_suppress(&mut s, a).unwrap();
    assert!(s.document.feature(a).unwrap().suppressed);
    assert_eq!(s.bodies.len(), 1);
    browser::toggle_suppress(&mut s, a).unwrap();
    assert_eq!(s.bodies.len(), 2);
    browser::roll_back_to(&mut s, a).unwrap();
    assert_eq!(s.document.marker(), 1);
    assert_eq!(s.bodies.len(), 1);
    browser::roll_to_end(&mut s).unwrap();
    assert_eq!(s.document.marker(), 2);
}
```

(`planora_app::testing::collect_texts` ist ein kleiner, in `src/lib.rs` hinter `#[doc(hidden)]` bereitgestellter Helfer, der die Texte der Formen eines `FullOutput` sammelt; die vorhandenen Tests (`sketch_dialogs.rs`) haben denselben Code lokal – er wird dorthin zurückgeführt. Ersatz: lokale Funktion in der Testdatei.)

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-app --test feature_dialog --test tree_ui`
Expected: FAIL

- [ ] **Step 3: Implementierung**

Reihenfolge: (a) Zustandsfunktionen in `browser.rs` (`delete_plan`, `apply_delete`, `toggle_suppress`, `roll_back_to`, `roll_to_end`, `DeletePlan { target, dependents, names }` mit `text()`); (b) `FeatureDialog` mit den Feldern je Art (Felder als Struktur-Kopie der `FeatureKind`, `set_box_size` & Co. als kleine öffentliche Setter für Tests); (c) Baum-Zeichnen und Kontextmenü; (d) Eigenschaften; (e) Menüs/Statuszeile; (f) Tests. Verhalten bei offener Skizze: Alle Dokument-ändernden Aktionen (Unterdrücken, Löschen, Zurückrollen, Bearbeiten) beenden zuerst die Skizze (`close_sketch`, R12), außer sie betreffen die geöffnete Skizze selbst (dann Hinweis „Zuerst die Skizze beenden“).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all && cargo deny check`
Expected: PASS

- [ ] **Step 5: App von Hand prüfen** (Pflicht, Bildschirmfotos): Baum mit mehreren Features; Rechtsklick → Unterdrücken, Zurückrollen bis hier, Zum Ende; Doppelklick auf eine Extrusion ändert die Höhe → Körper folgt; Löschen mit Rückfrage; Umbenennen. Ehrlich berichten (auch was nicht per Automation prüfbar war).

- [ ] **Step 6: Commit**

```bash
git add -A crates
git commit -m "feat(app): Feature-Baum in der Oberfläche (Baum, Kontextmenü, Bearbeiten-Dialog, Löschen mit Folgen)"
```

---

### Task 10: Dokumentation, Spec-Nachträge, Abschlussprüfung

**Files:**
- Modify: `README.md`, `docs/superpowers/offene-punkte.md`, `docs/superpowers/specs/2026-10-10-feature-baum-design.md` (Abschnitt „Änderungen bei der Umsetzung“)

**Interfaces:** keine neuen; Dokumentation und Gesamtprüfung.

- [ ] **Step 1: Dokumentation**
  - README: Abschnitt „Feature-Baum“ (Was geht: Zeitleiste, Neuberechnung, stabile Verweise, Unterdrücken, Zurückrollen, Löschen mit Folgen; was nicht: Speichern/Laden, benannte Parameter, Rückgängig, Muster-/Spiegel-Features, Ziehen des Markers, Skizzenebene nachträglich ändern, Körper vor die Skizze projizieren).
  - `offene-punkte.md`: Abschnitt „Nach Teilprojekt 5 (Feature-Baum)“ mit den echten Grenzen (aus dem Ledger: Profilindex-Zuordnung, Verweise bei vertauschten gleichartigen Flächen, `Move` ohne Körperidentität, Kantenkopien nur bei gleicher Art der Kante, Zwischenspeicher nur im Speicher, kein Ziehen des Markers, Beispielprofile entfernt, `BodyKind::Combined`). „Bekannte Grenzen“ der früheren Abschnitte aktualisieren, wo sie sich geändert haben (Skizzen sind jetzt Features).
  - Spec: Abschnitt „10. Änderungen bei der Umsetzung“ mit F1–F13 und allen weiteren tatsächlichen Abweichungen aus dem Ledger (z. B. Rollen der Grundkörper, `Suppressed`-Status, `recompute()` ohne `from`, Volumenprobe im Feature, Skizzen-Entwurf, Marker per Menü, Entfernen der Beispielprofile).
- [ ] **Step 2: Gesamtprüfung**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check
```
Expected: alles grün; `deny.toml` unverändert; kein neuer externer Eintrag in `Cargo.lock`.
- [ ] **Step 3: Gesamter Ablauf von Hand** (mit Bildschirmfotos): Skizze auf XY → Extrusion → Skizze auf der Deckfläche → zweite Extrusion → Boolesche Operation (Vereinigen) → Maß der ersten Skizze und Höhe der ersten Extrusion ändern → alle Körper passen sich an; Fläche absichtlich verlieren (z. B. erste Extrusion auf ein anderes Profil umstellen) → Fehler am richtigen Feature, Rest bleibt benutzbar. Ehrlich beschreiben.
- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "docs: Feature-Baum (README, offene Punkte, Änderungen bei der Umsetzung)"
```

---

## Selbstprüfung (Spec-Abdeckung)

| Spec-Abschnitt | Task |
|---|---|
| 3 Datenmodell (Features, Status, Document, BodyRef) | 4, 5 |
| 3.1 Neuberechnung, Zwischenspeicher, Fehlerausbreitung, Rückrollen, Unterdrücken | 5 |
| 3.1 Änderungsoperationen, Löschen mit Folgen, Einfügen am Marker | 6 |
| 4.1 Marken im Kernel (Extrusion, Rotation, Verschieben, Boolesche Operationen) | 1, 2 |
| 4.2 Verweise (FaceRef/EdgeRef, hint) | 4 (Auflösen), 6 (Kette), 8 (Oberfläche) |
| 4.3 Grenzen der Verweise | 10 (Dokumentation) |
| 5 Oberfläche (Baum, Aktionen, Dialoge, Skizzen, Eigenschaften, abgeleitete Körperliste) | 7, 8, 9 |
| 6 Fehlerbehandlung (atomar, Fehler am Feature, keine Panik) | 5, 6, 9 |
| 7 Tests (Neuberechnung, Beispielketten, Verweise, Kernel-Marken, Löschen/Umbenennen/Einfügen, Eigenschaftstests, App) | 1–9 |
| 9 Lizenzen | alle (keine neue Abhängigkeit) |

Abweichungen von der Spec sind oben unter „Entscheidungen zur Spec“ (F1–F13) benannt.
