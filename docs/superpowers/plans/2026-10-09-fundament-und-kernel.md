# PlanoraCAD Fundament und Kernel-Basis – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ein lauffähiges PlanoraCAD für Windows und Linux, in dem man Quader, Zylinder, Kugel, Kegel sowie extrudierte und rotierte Profile erzeugt, im 3D-Viewport betrachtet und Flächen/Kanten auswählt.

**Architecture:** Cargo-Workspace aus fünf Crates mit Abhängigkeiten nur nach unten (`math` → `kernel` → `mesh` → `render` → `app`). Der Kernel ist ein eigener B-Rep-Kernel in reinem Rust mit analytischen Flächen; nicht ebene Flächen sind rechteckige Parameter-Ausschnitte (`UvRect`). Der Viewport zeichnet über einen `egui_wgpu`-Paint-Callback; Auswahl läuft per CPU-Strahlentest.

**Tech Stack:** Rust (Edition 2024, aktuelles stable), `glam` 0.34, `robust` 1.2, `earcutr` 0.5, `thiserror` 2, `proptest` 1, `bytemuck` 1, `eframe`/`egui`/`egui-wgpu` 0.36, `wgpu` 30 (über `egui_wgpu::wgpu`), `egui_dock` 0.21.

**Spec:** `docs/superpowers/specs/2026-10-09-fundament-und-kernel-design.md`

## Global Constraints

- Reines Rust, keine C/C++-Abhängigkeit im Projekt (kein OpenCascade).
- Zahlen sind `f64` (Kernel/Mesh), Einheit Millimeter. Die GPU-Schicht (`render`) nutzt `f32`.
- Projektlizenz **GPL-3.0-only**; jede Abhängigkeit muss GPL-3-kompatibel sein (MIT, Apache-2.0, ISC, BSD, Zlib …). Nicht erlaubt: GPL-2-only, proprietär, source-available, ohne Lizenz.
- **Lizenzen werden immer geprüft:** vor jeder neuen Abhängigkeit und jedem übernommenen Asset; `cargo deny check` muss grün sein. Findet `cargo deny` eine Lizenz außerhalb der Erlaubnisliste, **anhalten und den Menschen fragen** – nichts selbstständig freigeben.
- Kein Autodesk-Code, keine Autodesk-Icons/-Texte, kein „Fusion“-Branding, kein `.f3d`. Icons sind selbst gezeichnete Vektorformen im Code.
- Name: PlanoraCAD (Arbeitstitel). Crate-Präfix `planora-`.
- Plattformen: Windows (MSVC-Toolchain) und Linux.
- Eine zentrale `Tolerance` (linear 1e-6 mm, angular 1e-9) wird durchgereicht; keine verstreuten Epsilon-Konstanten in Kernel-Operationen.
- Der Kernel (`planora-kernel`) kennt weder Oberfläche noch Grafik noch Tessellierung.
- Oberflächentexte auf Deutsch. Dunkles Theme ist Standard.
- Jede Eingabe, die ein Maß übergibt, muss NaN, 0 und negative Werte mit einem Fehler (`KernelError`) ablehnen und darf nicht panicken.

## Review Focus

Eingaben/Bedingungen, die die Spec nur andeutet und die einem Benutzer am ehesten wehtun; jede Zeile hat einen Test in der genannten Task:

1. **NaN, 0 oder negative Maße** (Quader, Zylinder, Kugel, Kegel, Höhe) → `Err(KernelError::InvalidDimension)`, kein Panic (Task 4, Task 5).
2. **Profil mit Fläche 0 oder im falschen Drehsinn** (Rechteck mit Breite 0; Profil im Uhrzeigersinn; Loch gegen den Uhrzeigersinn) → Fehler bzw. automatisch korrekt orientiert (Task 4).
3. **Sehr große und sehr kleine Körper** (Radius 1e5 mm, Radius 0,01 mm) → Tessellierung bleibt in vernünftigen Grenzen (≤ 100 000 Indizes bzw. ≥ 16 Segmente pro Kreis) (Task 7).
4. **Gespiegelte, skalierte und schief verzerrte Matrizen** → gespiegelte Körper behalten ein positives Volumen und ein geschlossenes Netz; nicht-ähnliche Matrizen liefern `NonSimilarityTransform` (Task 6, Task 7).
5. **Leere Szene und Viewport der Größe 0** → Picking/Einpassen/Kamera liefern `None`/bleiben stabil, kein NaN, kein Panic (Task 8, Task 10).

## Hinweis zur Verifikation

Rust ist auf dem Entwicklungsrechner **noch nicht installiert** (Task 1 installiert es). Der Code in diesem Plan wurde deshalb noch **nicht kompiliert**. Die Drittanbieter-APIs wurden gegen die Quellen geprüft: egui/eframe/egui-wgpu 0.36.2, egui_dock 0.21.1, wgpu-types 30.0.1, earcutr 0.5.0, robust 1.2.0. Kleine Kompilierfehler sind trotzdem möglich. Regel: Schlägt der Schritt „Tests laufen lassen“ wegen eines Kompilierfehlers fehl, wird der **Implementierungscode** korrigiert (bei API-Abweichungen gegen docs.rs der festgelegten Version prüfen), nicht der Test – außer der Test selbst enthält einen Tippfehler. Der Test-Soll-Zustand („PASS“) ist verbindlich.

## Dateistruktur

```
PlanoraCAD/
├─ Cargo.toml                  Workspace, gemeinsame Versionen/Abhängigkeiten/Lints
├─ rustfmt.toml  deny.toml  LICENSE  README.md  .gitignore
├─ .github/workflows/ci.yml
├─ crates/
│  ├─ planora-math/            Frame, Tolerance, orient2d
│  │  ├─ src/{lib,frame,tolerance,predicates}.rs
│  │  └─ tests/math.rs
│  ├─ planora-kernel/
│  │  ├─ src/{lib,error,ids,geometry,topology,validate,profile,extrude,revolve,primitives,transform,kernel}.rs
│  │  └─ tests/{ids,geometry,topology,profile,extrude,revolve,transform}.rs
│  ├─ planora-mesh/
│  │  ├─ src/{lib,mesh,discretize,tessellate,mass,check}.rs
│  │  └─ tests/{tessellate,transforms}.rs
│  ├─ planora-render/
│  │  ├─ src/{lib,camera,pick,scene,viewcube,gpu}.rs  src/shader.wgsl
│  │  └─ tests/{camera,pick,scene,viewcube}.rs
│  └─ planora-app/
│     ├─ src/{lib,main,state,samples,theme,icons,create_dialog,browser,properties,viewport,app}.rs
│     └─ tests/{state,samples}.rs
```

---

### Task 1: Toolchain, Workspace, Lizenz, CI

**Files:**
- Create: `Cargo.toml`, `rustfmt.toml`, `deny.toml`, `.gitignore`, `LICENSE`, `README.md`, `.github/workflows/ci.yml`
- Create: `crates/planora-math/Cargo.toml`, `crates/planora-math/src/lib.rs`

**Interfaces:**
- Produces: Workspace mit `[workspace.dependencies]` (alle späteren Crates verwenden `xyz.workspace = true`), Crate `planora-math` (leer bis Task 2).

- [ ] **Step 1: Rust und Build-Werkzeuge installieren (der Mensch bestätigt vorher)**

Das installiert Software auf dem Rechner – vorher beim Menschen nachfragen. In PowerShell:

```powershell
winget install --id Rustlang.Rustup -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Danach eine **neue** Shell öffnen (oder `$env:Path += ";$env:USERPROFILE\.cargo\bin"`), dann:

```powershell
rustup default stable
rustup component add rustfmt clippy
cargo install cargo-deny --locked
rustc --version
cargo --version
cargo deny --version
```

Expected: Versionsausgaben ohne Fehler (`rustc` ≥ 1.88).

Linux (nur Hinweis, nicht auf diesem Rechner): `sudo apt-get install -y build-essential pkg-config libclang-dev libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev` und Rust über `rustup`.

- [ ] **Step 2: Git-Identität im Projekt setzen**

```powershell
cd D:\Testumgebung\PlanoraCAD
git config user.name "Dennis Guse"
git config user.email "info@dennis-guse.de"
```

- [ ] **Step 3: Workspace-Dateien schreiben**

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "GPL-3.0-only"
repository = ""
authors = ["Dennis Guse"]

[workspace.dependencies]
# interne Crates
planora-math = { path = "crates/planora-math" }
planora-kernel = { path = "crates/planora-kernel" }
planora-mesh = { path = "crates/planora-mesh" }
planora-render = { path = "crates/planora-render" }

# externe Crates (Lizenzen am 2026-10-09 geprüft: alle MIT und/oder Apache-2.0, earcutr ISC, egui_dock MIT)
glam = "0.34"
robust = "1.2"
earcutr = "0.5"
thiserror = "2"
bytemuck = { version = "1", features = ["derive"] }
proptest = "1"
eframe = { version = "0.36", features = ["wgpu"] }
egui = "0.36"
egui-wgpu = "0.36"
egui_dock = "0.21"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
```

`rustfmt.toml`:

```toml
edition = "2024"
```

`.gitignore`:

```
/target
```

`deny.toml`:

```toml
[graph]
all-features = true

[advisories]
version = 2

[licenses]
version = 2
# Nur freizügige, GPL-3-kompatible Lizenzen. Alles andere: anhalten und den Menschen fragen.
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "ISC",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "Zlib",
    "Unicode-3.0",
    "0BSD",
    "CC0-1.0",
    "BSL-1.0",
    "GPL-3.0-only",
]
confidence-threshold = 0.9

[bans]
multiple-versions = "warn"
wildcards = "deny"
allow-wildcard-paths = true

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

`LICENSE` (offizieller GPL-3.0-Text):

```powershell
curl.exe -L -o LICENSE https://www.gnu.org/licenses/gpl-3.0.txt
```

Expected: `LICENSE` beginnt mit „GNU GENERAL PUBLIC LICENSE Version 3, 29 June 2007“. Falls der Download scheitert (z. B. wegen TLS-Abfang durch den NordVPN-Bedrohungsschutz, siehe Memory `nordvpn-tls-interception`), den Menschen bitten, den Text von gnu.org manuell einzufügen – nichts erfinden.

`README.md`:

```markdown
# PlanoraCAD

Ein quelloffenes, parametrisches 3D-CAD in reinem Rust für Windows und Linux.

**Status:** frühe Entwicklung (Fundament und Kernel-Basis).

Lizenz: GPL-3.0-only (siehe `LICENSE`).

## Bauen

    cargo run -p planora-app --release

Unter Linux werden zusätzlich die System-Bibliotheken aus `.github/workflows/ci.yml` benötigt.

## Entwicklung

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check
```

`.github/workflows/ci.yml`:

```yaml
name: CI
on:
  push:
  pull_request:

jobs:
  check:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - name: Rust installieren
        run: rustup toolchain install stable --profile minimal -c rustfmt,clippy
      - name: Linux-Bibliotheken
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y build-essential pkg-config libclang-dev libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      - name: Lizenzen prüfen
        run: cargo install cargo-deny --locked && cargo deny check
```

`crates/planora-math/Cargo.toml`:

```toml
[package]
name = "planora-math"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
glam.workspace = true
robust.workspace = true

[lints]
workspace = true
```

`crates/planora-math/src/lib.rs`:

```rust
//! Grundlegende Mathematik für PlanoraCAD: Vektoren, Koordinatensysteme, Toleranzen.
```

- [ ] **Step 4: Bauen und Lizenzen prüfen**

```powershell
cargo build
cargo deny check
```

Expected: `cargo build` erfolgreich. `cargo deny check` meldet `licenses ok`, `bans ok`, `sources ok`. Meldet es eine Lizenz außerhalb der Erlaubnisliste oder Sicherheitswarnungen: **anhalten und den Menschen fragen.** Warnungen zu doppelten Versionen (`multiple-versions`) sind erlaubt.

- [ ] **Step 5: Commit**

```powershell
git add .
git commit -m "chore: Workspace, GPL-3.0, cargo-deny und CI" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `planora-math`

**Files:**
- Create: `crates/planora-math/src/{frame,tolerance,predicates}.rs`
- Modify: `crates/planora-math/src/lib.rs`
- Test: `crates/planora-math/tests/math.rs`

**Interfaces:**
- Produces (öffentlich aus `planora_math`):
  - `Tolerance { linear: f64, angular: f64 }` mit `Default` (1e-6 / 1e-9), `points_equal(&self, DVec3, DVec3) -> bool`
  - `Frame { origin, x, y, z: DVec3 }` (`Copy`), `Frame::WORLD`, `from_origin_normal(DVec3, DVec3)`, `point(&self, f64, f64, f64) -> DVec3`, `point_2d(&self, DVec2) -> DVec3`, `to_local(&self, DVec3) -> DVec3`, `transformed(&self, &DMat4) -> Frame`
  - `orient2d(DVec2, DVec2, DVec2) -> f64` (> 0 = gegen den Uhrzeigersinn)
  - Re-exports: `DVec2, DVec3, DMat3, DMat4, PI, TAU`

- [ ] **Step 1: Tests schreiben** – `crates/planora-math/tests/math.rs`

```rust
use planora_math::*;

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}

fn close3(a: DVec3, b: DVec3) {
    assert!(a.distance(b) < 1e-12, "{a:?} != {b:?}");
}

#[test]
fn default_tolerance_values() {
    let t = Tolerance::default();
    close(t.linear, 1e-6);
    close(t.angular, 1e-9);
}

#[test]
fn tolerance_compares_points() {
    let t = Tolerance::default();
    assert!(t.points_equal(DVec3::ZERO, DVec3::new(5e-7, 0.0, 0.0)));
    assert!(!t.points_equal(DVec3::ZERO, DVec3::new(2e-6, 0.0, 0.0)));
}

#[test]
fn frame_from_normal_is_orthonormal_and_right_handed() {
    let normals = [
        DVec3::X,
        DVec3::Y,
        DVec3::Z,
        -DVec3::Z,
        DVec3::new(1.0, 2.0, 3.0),
    ];
    for n in normals {
        let f = Frame::from_origin_normal(DVec3::new(1.0, 1.0, 1.0), n);
        close(f.x.length(), 1.0);
        close(f.y.length(), 1.0);
        close(f.z.length(), 1.0);
        close(f.x.dot(f.y), 0.0);
        close(f.x.dot(f.z), 0.0);
        close(f.y.dot(f.z), 0.0);
        close3(f.x.cross(f.y), f.z);
        close3(f.z, n.normalize());
    }
}

#[test]
fn frame_local_roundtrip() {
    let f = Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.3, -0.2, 0.9));
    let local = DVec3::new(0.5, -1.5, 2.0);
    let p = f.point(local.x, local.y, local.z);
    close3(f.to_local(p), local);
}

#[test]
fn frame_point_2d_uses_x_and_y_axes() {
    let f = Frame::WORLD;
    close3(f.point_2d(DVec2::new(2.0, 3.0)), DVec3::new(2.0, 3.0, 0.0));
}

#[test]
fn rotation_keeps_frame_right_handed() {
    let m = DMat4::from_translation(DVec3::new(1.0, 0.0, 0.0)) * DMat4::from_rotation_z(0.7);
    let f = Frame::WORLD.transformed(&m);
    close3(f.x.cross(f.y), f.z);
    close3(f.origin, m.transform_point3(DVec3::ZERO));
}

#[test]
fn mirror_keeps_frame_right_handed() {
    let m = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    let f = Frame::WORLD.transformed(&m);
    close3(f.x.cross(f.y), f.z);
}

#[test]
fn orient2d_sign_matches_winding() {
    let a = DVec2::new(0.0, 0.0);
    let b = DVec2::new(1.0, 0.0);
    assert!(orient2d(a, b, DVec2::new(0.0, 1.0)) > 0.0);
    assert!(orient2d(a, b, DVec2::new(0.0, -1.0)) < 0.0);
    assert_eq!(orient2d(a, b, DVec2::new(2.0, 0.0)), 0.0);
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-math`
Expected: FAIL (Kompilierfehler: `Tolerance`, `Frame`, `orient2d` nicht gefunden).

- [ ] **Step 3: Implementieren**

`crates/planora-math/src/lib.rs`:

```rust
//! Grundlegende Mathematik für PlanoraCAD: Vektoren, Koordinatensysteme, Toleranzen.

mod frame;
mod predicates;
mod tolerance;

pub use frame::Frame;
pub use glam::{DMat3, DMat4, DVec2, DVec3};
pub use predicates::orient2d;
pub use std::f64::consts::{PI, TAU};
pub use tolerance::Tolerance;
```

`crates/planora-math/src/tolerance.rs`:

```rust
use glam::DVec3;

/// Zentrale Toleranzen. Wird jeder Kernel-Operation durchgereicht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    /// Längentoleranz in Millimetern.
    pub linear: f64,
    /// Winkeltoleranz in Radiant.
    pub angular: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            linear: 1e-6,
            angular: 1e-9,
        }
    }
}

impl Tolerance {
    pub fn points_equal(&self, a: DVec3, b: DVec3) -> bool {
        a.distance(b) <= self.linear
    }
}
```

`crates/planora-math/src/predicates.rs`:

```rust
use glam::DVec2;
use robust::Coord;

/// Exakter Orientierungstest: > 0, wenn `a → b → c` gegen den Uhrzeigersinn läuft,
/// < 0 im Uhrzeigersinn, 0 bei kollinearen Punkten.
pub fn orient2d(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    robust::orient2d(
        Coord { x: a.x, y: a.y },
        Coord { x: b.x, y: b.y },
        Coord { x: c.x, y: c.y },
    )
}
```

`crates/planora-math/src/frame.rs`:

```rust
use glam::{DMat4, DVec2, DVec3};

/// Rechtshändiges, orthonormales Koordinatensystem (`x × y = z`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
    pub z: DVec3,
}

impl Frame {
    pub const WORLD: Frame = Frame {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    };

    /// Koordinatensystem mit gegebenem Ursprung, dessen z-Achse `normal` ist.
    pub fn from_origin_normal(origin: DVec3, normal: DVec3) -> Frame {
        let z = normal.normalize();
        let x = z.any_orthonormal_vector();
        let y = z.cross(x);
        Frame { origin, x, y, z }
    }

    pub fn point(&self, x: f64, y: f64, z: f64) -> DVec3 {
        self.origin + self.x * x + self.y * y + self.z * z
    }

    pub fn point_2d(&self, p: DVec2) -> DVec3 {
        self.origin + self.x * p.x + self.y * p.y
    }

    pub fn to_local(&self, p: DVec3) -> DVec3 {
        let d = p - self.origin;
        DVec3::new(d.dot(self.x), d.dot(self.y), d.dot(self.z))
    }

    /// Wendet eine Ähnlichkeitstransformation an. Die z-Achse wird immer als `x × y`
    /// neu berechnet, damit das System auch bei Spiegelungen rechtshändig bleibt.
    pub fn transformed(&self, m: &DMat4) -> Frame {
        let origin = m.transform_point3(self.origin);
        let x = m.transform_vector3(self.x).normalize();
        let y = m.transform_vector3(self.y).normalize();
        let z = x.cross(y);
        Frame { origin, x, y, z }
    }
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-math`
Expected: PASS (8 Tests).

- [ ] **Step 5: Commit**

```powershell
git add crates/planora-math
git commit -m "feat(math): Tolerance, Frame und orient2d" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Kernel – IDs, Geometrie, Topologie

**Files:**
- Create: `crates/planora-kernel/Cargo.toml`, `src/{lib,error,ids,geometry,topology}.rs`
- Test: `crates/planora-kernel/tests/{ids,geometry,topology}.rs`

**Interfaces:**
- Consumes: `planora_math::{DVec3, Frame, Tolerance}`
- Produces:
  - `Id<T>` (`Copy, Eq, Hash, Ord, Debug`), `Id::from_index(usize)`, `id.index() -> usize`; `Arena<T>` mit `new()`, `push(T) -> Id<T>`, `len()`, `is_empty()`, `iter() -> impl Iterator<Item=(Id<T>, &T)>`, `Index/IndexMut<Id<T>>`
  - `Curve::{Line{origin,dir}, Circle{frame,radius}, Ellipse{frame,major,minor}}` mit `point(t) -> DVec3`
  - `Surface::{Plane{frame}, Cylinder{frame,radius}, Cone{frame,radius,slope}, Sphere{frame,radius}, Torus{frame,major,minor}}` mit `point(u,v)`, `normal(u,v)` (Normale = ∂u × ∂v)
  - `UvRect { u0, u1, v0, v1 }`
  - `Vertex { point }`, `Edge { curve, t0, t1, start, end }`, `HalfEdge { edge: Id<Edge>, forward: bool }`, `Loop { half_edges: Vec<HalfEdge> }` mit `reversed()`, `Face { surface, same_sense, outer, inner, patch }`, `Shell { faces }`, `Solid { vertices, edges, faces, shells }` mit `new()`, `add_vertex`, `add_edge`, `add_face`, `seal`, `half_edge_start`, `half_edge_end`
  - `KernelError` (siehe unten)

- [ ] **Step 1: Cargo.toml und Tests schreiben**

`crates/planora-kernel/Cargo.toml`:

```toml
[package]
name = "planora-kernel"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
planora-math.workspace = true
thiserror.workspace = true

[dev-dependencies]
proptest.workspace = true

[lints]
workspace = true
```

`crates/planora-kernel/src/lib.rs` (zunächst minimal, damit die Tests kompilieren können):

```rust
//! Geometrie-Kernel von PlanoraCAD (B-Rep mit analytischen Flächen).

pub mod error;
pub mod geometry;
pub mod ids;
pub mod topology;

pub use error::KernelError;
pub use geometry::{Curve, Surface, UvRect};
pub use ids::{Arena, Id};
pub use topology::{Edge, Face, HalfEdge, Loop, Shell, Solid, Vertex};
```

`crates/planora-kernel/tests/ids.rs`:

```rust
use planora_kernel::{Arena, Id};
use std::collections::HashSet;

#[test]
fn arena_push_index_and_iter() {
    let mut a: Arena<&str> = Arena::new();
    assert!(a.is_empty());
    let x = a.push("x");
    let y = a.push("y");
    assert_eq!(a.len(), 2);
    assert_eq!(a[x], "x");
    assert_eq!(a[y], "y");
    let all: Vec<_> = a.iter().map(|(id, v)| (id.index(), *v)).collect();
    assert_eq!(all, vec![(0, "x"), (1, "y")]);
}

#[test]
fn arena_index_mut() {
    let mut a: Arena<i32> = Arena::new();
    let id = a.push(1);
    a[id] = 5;
    assert_eq!(a[id], 5);
}

#[test]
fn ids_are_copy_eq_hash_ord() {
    let a: Id<String> = Id::from_index(3);
    let b = a;
    assert_eq!(a, b);
    assert!(Id::<String>::from_index(1) < Id::<String>::from_index(2));
    let mut set = HashSet::new();
    set.insert(a);
    assert!(set.contains(&b));
}
```

`crates/planora-kernel/tests/geometry.rs`:

```rust
use planora_kernel::*;
use planora_math::*;

fn frame() -> Frame {
    Frame::from_origin_normal(DVec3::new(1.0, -2.0, 3.0), DVec3::new(0.2, 0.5, 0.8))
}

fn numeric_normal(s: &Surface, u: f64, v: f64) -> DVec3 {
    let h = 1e-6;
    let du = (s.point(u + h, v) - s.point(u - h, v)) / (2.0 * h);
    let dv = (s.point(u, v + h) - s.point(u, v - h)) / (2.0 * h);
    du.cross(dv).normalize()
}

#[test]
fn analytic_normals_match_numeric_cross_product() {
    let f = frame();
    let surfaces = [
        Surface::Plane { frame: f },
        Surface::Cylinder { frame: f, radius: 2.5 },
        Surface::Cone { frame: f, radius: 3.0, slope: 0.4 },
        Surface::Cone { frame: f, radius: 3.0, slope: -0.4 },
        Surface::Sphere { frame: f, radius: 4.0 },
        Surface::Torus { frame: f, major: 5.0, minor: 1.5 },
    ];
    for s in &surfaces {
        for &(u, v) in &[(0.3, 0.2), (1.9, -0.7), (4.4, 0.9), (6.0, 0.1)] {
            let n = s.normal(u, v);
            assert!((n.length() - 1.0).abs() < 1e-12);
            assert!(
                n.distance(numeric_normal(s, u, v)) < 1e-5,
                "{s:?} at ({u},{v})"
            );
        }
    }
}

#[test]
fn sphere_and_cylinder_points_have_expected_distance() {
    let f = Frame::WORLD;
    let sphere = Surface::Sphere { frame: f, radius: 4.0 };
    assert!((sphere.point(0.7, 0.3).length() - 4.0).abs() < 1e-12);
    let cyl = Surface::Cylinder { frame: f, radius: 2.0 };
    let p = cyl.point(1.1, 5.0);
    assert!((DVec2::new(p.x, p.y).length() - 2.0).abs() < 1e-12);
    assert!((p.z - 5.0).abs() < 1e-12);
}

#[test]
fn circle_curve_point_is_on_circle() {
    let c = Curve::Circle { frame: frame(), radius: 3.0 };
    for t in [0.0, 1.0, 2.5, 6.0] {
        assert!((c.point(t).distance(frame().origin) - 3.0).abs() < 1e-12);
    }
}

#[test]
fn line_curve_point_is_linear() {
    let l = Curve::Line { origin: DVec3::new(1.0, 1.0, 1.0), dir: DVec3::X };
    assert!(l.point(2.0).distance(DVec3::new(3.0, 1.0, 1.0)) < 1e-12);
}
```

`crates/planora-kernel/tests/topology.rs`:

```rust
use planora_kernel::*;
use planora_math::*;

fn tri_solid() -> (Solid, Id<Edge>, Id<Edge>) {
    let mut s = Solid::new();
    let a = s.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_vertex(DVec3::new(1.0, 0.0, 0.0));
    let c = s.add_vertex(DVec3::new(0.0, 1.0, 0.0));
    let line = |from: DVec3, to: DVec3| Curve::Line {
        origin: from,
        dir: (to - from).normalize(),
    };
    let e1 = s.add_edge(line(DVec3::ZERO, DVec3::X), 0.0, 1.0, a, b);
    let e2 = s.add_edge(line(DVec3::X, DVec3::Y), 0.0, 2f64.sqrt(), b, c);
    (s, e1, e2)
}

#[test]
fn half_edge_endpoints_follow_direction() {
    let (s, e1, _) = tri_solid();
    let fwd = HalfEdge { edge: e1, forward: true };
    let bwd = HalfEdge { edge: e1, forward: false };
    assert_eq!(s.half_edge_start(fwd), s.edges[e1].start);
    assert_eq!(s.half_edge_end(fwd), s.edges[e1].end);
    assert_eq!(s.half_edge_start(bwd), s.edges[e1].end);
    assert_eq!(s.half_edge_end(bwd), s.edges[e1].start);
}

#[test]
fn loop_reversed_flips_order_and_direction() {
    let (_, e1, e2) = tri_solid();
    let l = Loop {
        half_edges: vec![
            HalfEdge { edge: e1, forward: true },
            HalfEdge { edge: e2, forward: true },
        ],
    };
    let r = l.reversed();
    assert_eq!(
        r.half_edges,
        vec![
            HalfEdge { edge: e2, forward: false },
            HalfEdge { edge: e1, forward: false },
        ]
    );
    assert_eq!(r.reversed(), l);
}

#[test]
fn seal_puts_all_faces_into_one_shell() {
    let (mut s, e1, _) = tri_solid();
    let face = Face {
        surface: Surface::Plane { frame: Frame::WORLD },
        same_sense: true,
        outer: Loop { half_edges: vec![HalfEdge { edge: e1, forward: true }] },
        inner: vec![],
        patch: None,
    };
    let f1 = s.add_face(face.clone());
    let f2 = s.add_face(face);
    s.seal();
    assert_eq!(s.shells.len(), 1);
    assert_eq!(s.shells[0].faces, vec![f1, f2]);
}
```

`Loop` muss dafür `PartialEq` ableiten (siehe Implementierung).

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel`
Expected: FAIL (Kompilierfehler, Module leer bzw. nicht vorhanden).

- [ ] **Step 3: Implementieren**

`crates/planora-kernel/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum KernelError {
    #[error("Das Profil ist nicht geschlossen")]
    ProfileNotClosed,
    #[error("Ein Profilsegment hat die Länge null")]
    DegenerateSegment,
    #[error("Das Profil hat die Fläche null")]
    ZeroAreaProfile,
    #[error("Das Maß muss eine Zahl größer als die Toleranz sein (war {0})")]
    InvalidDimension(f64),
    #[error("Das Rotationsprofil muss auf der Achse beginnen und enden")]
    ProfileOffAxis,
    #[error("Das Rotationsprofil darf die Achse nur am Anfang und am Ende berühren")]
    ProfileCrossesAxis,
    #[error("Dieses Rotationssegment wird noch nicht unterstützt: {0}")]
    UnsupportedSegment(&'static str),
    #[error("Die Matrix ist keine Ähnlichkeitstransformation (verschieben, drehen, spiegeln, gleichmäßig skalieren)")]
    NonSimilarityTransform,
}
```

`crates/planora-kernel/src/ids.rs`:

```rust
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// Typisierter Index in eine [`Arena`].
pub struct Id<T> {
    index: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    pub fn from_index(index: usize) -> Self {
        Self {
            index: index as u32,
            marker: PhantomData,
        }
    }

    pub fn index(self) -> usize {
        self.index as usize
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Id<T> {}
impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}
impl<T> Eq for Id<T> {}
impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}
impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.index.cmp(&other.index)
    }
}
impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.index)
    }
}

/// Einfacher Speicher, der Elemente per [`Id`] adressiert.
#[derive(Clone, Debug)]
pub struct Arena<T> {
    items: Vec<T>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: T) -> Id<T> {
        let id = Id::from_index(self.items.len());
        self.items.push(item);
        id
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.items
            .iter()
            .enumerate()
            .map(|(i, item)| (Id::from_index(i), item))
    }
}

impl<T> Index<Id<T>> for Arena<T> {
    type Output = T;
    fn index(&self, id: Id<T>) -> &T {
        &self.items[id.index()]
    }
}

impl<T> IndexMut<Id<T>> for Arena<T> {
    fn index_mut(&mut self, id: Id<T>) -> &mut T {
        &mut self.items[id.index()]
    }
}
```

`crates/planora-kernel/src/geometry.rs`:

```rust
use planora_math::{DVec3, Frame};

/// Parametrische Kurve. Die Parameterbereiche stehen an der [`Edge`](crate::Edge).
#[derive(Clone, Debug)]
pub enum Curve {
    /// `origin + dir * t`, `dir` hat die Länge 1 (t = Länge entlang der Geraden).
    Line { origin: DVec3, dir: DVec3 },
    /// `origin + radius * (cos t * x + sin t * y)`
    Circle { frame: Frame, radius: f64 },
    /// `origin + major * cos t * x + minor * sin t * y`
    Ellipse { frame: Frame, major: f64, minor: f64 },
}

impl Curve {
    pub fn point(&self, t: f64) -> DVec3 {
        match self {
            Curve::Line { origin, dir } => *origin + *dir * t,
            Curve::Circle { frame, radius } => {
                frame.origin + (frame.x * t.cos() + frame.y * t.sin()) * *radius
            }
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => frame.origin + frame.x * (major * t.cos()) + frame.y * (minor * t.sin()),
        }
    }
}

/// Rechteckiger Parameterbereich einer gekrümmten Fläche.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvRect {
    pub u0: f64,
    pub u1: f64,
    pub v0: f64,
    pub v1: f64,
}

/// Analytische Fläche. Die natürliche Normale ist `∂P/∂u × ∂P/∂v`.
#[derive(Clone, Debug)]
pub enum Surface {
    /// `P = origin + u * x + v * y`, Normale `z`.
    Plane { frame: Frame },
    /// `u` = Winkel um `z`, `v` = Höhe entlang `z`.
    Cylinder { frame: Frame, radius: f64 },
    /// `u` = Winkel, `v` = Höhe; Radius an der Stelle `v` ist `radius + slope * v`.
    Cone { frame: Frame, radius: f64, slope: f64 },
    /// `u` = Länge, `v` = Breite in [-π/2, π/2].
    Sphere { frame: Frame, radius: f64 },
    /// `u` = Winkel um `z`, `v` = Winkel um das Rohr.
    Torus { frame: Frame, major: f64, minor: f64 },
}

fn radial(frame: &Frame, u: f64) -> DVec3 {
    frame.x * u.cos() + frame.y * u.sin()
}

impl Surface {
    pub fn frame(&self) -> &Frame {
        match self {
            Surface::Plane { frame }
            | Surface::Cylinder { frame, .. }
            | Surface::Cone { frame, .. }
            | Surface::Sphere { frame, .. }
            | Surface::Torus { frame, .. } => frame,
        }
    }

    pub fn point(&self, u: f64, v: f64) -> DVec3 {
        match self {
            Surface::Plane { frame } => frame.origin + frame.x * u + frame.y * v,
            Surface::Cylinder { frame, radius } => {
                frame.origin + radial(frame, u) * *radius + frame.z * v
            }
            Surface::Cone {
                frame,
                radius,
                slope,
            } => frame.origin + radial(frame, u) * (*radius + *slope * v) + frame.z * v,
            Surface::Sphere { frame, radius } => {
                frame.origin + (radial(frame, u) * v.cos() + frame.z * v.sin()) * *radius
            }
            Surface::Torus {
                frame,
                major,
                minor,
            } => {
                frame.origin
                    + radial(frame, u) * (*major + *minor * v.cos())
                    + frame.z * (*minor * v.sin())
            }
        }
    }

    /// Natürliche Einheitsnormale `∂P/∂u × ∂P/∂v`.
    pub fn normal(&self, u: f64, v: f64) -> DVec3 {
        match self {
            Surface::Plane { frame } => frame.z,
            Surface::Cylinder { frame, .. } => radial(frame, u),
            Surface::Cone { frame, slope, .. } => {
                (radial(frame, u) - frame.z * *slope) / (1.0 + slope * slope).sqrt()
            }
            Surface::Sphere { frame, .. } | Surface::Torus { frame, .. } => {
                radial(frame, u) * v.cos() + frame.z * v.sin()
            }
        }
    }
}
```

`crates/planora-kernel/src/topology.rs`:

```rust
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::{Arena, Id};
use planora_math::DVec3;

#[derive(Clone, Debug)]
pub struct Vertex {
    pub point: DVec3,
}

/// Kante: `curve` über den Parameterbereich `t0 → t1`, von `start` nach `end`.
#[derive(Clone, Debug)]
pub struct Edge {
    pub curve: Curve,
    pub t0: f64,
    pub t1: f64,
    pub start: Id<Vertex>,
    pub end: Id<Vertex>,
}

/// Durchlaufrichtung einer Kante innerhalb einer Schleife.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HalfEdge {
    pub edge: Id<Edge>,
    pub forward: bool,
}

/// Geschlossene Folge von Halbkanten. Eine äußere Schleife läuft gegen den Uhrzeigersinn,
/// gesehen von außen auf die Fläche (Rechte-Hand-Regel um die nach außen zeigende Normale).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loop {
    pub half_edges: Vec<HalfEdge>,
}

impl Loop {
    pub fn reversed(&self) -> Loop {
        Loop {
            half_edges: self
                .half_edges
                .iter()
                .rev()
                .map(|he| HalfEdge {
                    edge: he.edge,
                    forward: !he.forward,
                })
                .collect(),
        }
    }
}

/// Fläche. Die nach außen zeigende Normale ist die natürliche Flächennormale,
/// wenn `same_sense` wahr ist, sonst deren Gegenteil.
///
/// Ebene Flächen haben `patch == None` und werden über ihre Schleifen begrenzt.
/// Gekrümmte Flächen müssen einen `patch` (Parameterbereich) haben.
#[derive(Clone, Debug)]
pub struct Face {
    pub surface: Surface,
    pub same_sense: bool,
    pub outer: Loop,
    pub inner: Vec<Loop>,
    pub patch: Option<UvRect>,
}

#[derive(Clone, Debug, Default)]
pub struct Shell {
    pub faces: Vec<Id<Face>>,
}

#[derive(Clone, Debug, Default)]
pub struct Solid {
    pub vertices: Arena<Vertex>,
    pub edges: Arena<Edge>,
    pub faces: Arena<Face>,
    pub shells: Vec<Shell>,
}

impl Solid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_vertex(&mut self, point: DVec3) -> Id<Vertex> {
        self.vertices.push(Vertex { point })
    }

    pub fn add_edge(
        &mut self,
        curve: Curve,
        t0: f64,
        t1: f64,
        start: Id<Vertex>,
        end: Id<Vertex>,
    ) -> Id<Edge> {
        self.edges.push(Edge {
            curve,
            t0,
            t1,
            start,
            end,
        })
    }

    pub fn add_face(&mut self, face: Face) -> Id<Face> {
        self.faces.push(face)
    }

    /// Fasst alle Flächen in einer einzigen Schale zusammen.
    pub fn seal(&mut self) {
        self.shells = vec![Shell {
            faces: self.faces.iter().map(|(id, _)| id).collect(),
        }];
    }

    pub fn half_edge_start(&self, he: HalfEdge) -> Id<Vertex> {
        let e = &self.edges[he.edge];
        if he.forward { e.start } else { e.end }
    }

    pub fn half_edge_end(&self, he: HalfEdge) -> Id<Vertex> {
        let e = &self.edges[he.edge];
        if he.forward { e.end } else { e.start }
    }
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel`
Expected: PASS (10 Tests).

- [ ] **Step 5: Commit**

```powershell
git add crates/planora-kernel
git commit -m "feat(kernel): IDs, Geometrie und Topologie" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Kernel – Validator, Profile, Extrusion, Quader/Zylinder

**Files:**
- Create: `crates/planora-kernel/src/{validate,profile,extrude,primitives,kernel}.rs`
- Modify: `crates/planora-kernel/src/lib.rs`
- Test: `crates/planora-kernel/tests/{profile,extrude}.rs`

**Interfaces:**
- Consumes: Task 2 und Task 3.
- Produces:
  - `Segment::{Line{to: DVec2}, Arc{center: DVec2, sweep: f64}}`, `ProfileLoop { start, segments }` mit `polygon(&[DVec2])`, `rectangle(min,max)`, `circle(center, radius)`, `chain_points() -> Vec<DVec2>`, `is_closed(&Tolerance)`, `sample(steps_per_turn) -> Vec<DVec2>`, `signed_area() -> f64`, `reversed()`; `Profile { outer, holes }` mit `oriented()`; `segment_end(DVec2, &Segment) -> DVec2`
  - `extrude(&Profile, &Frame, height: f64, &Tolerance) -> Result<Solid, KernelError>` (Profil liegt in der xy-Ebene des Frames, Extrusion entlang `frame.z`)
  - `primitives::{make_box(DVec3,&Tolerance), make_cylinder(radius,height,&Tolerance)}`
  - `validate(&Solid) -> Result<ValidationReport, ValidationError>`; `ValidationReport { vertices, edges, faces, inner_loops, genus }` (alle `usize`)
  - `Kernel`-Trait und `AnalyticKernel { tolerance }` (in dieser Task nur die schon vorhandenen Operationen; Task 5/6 ergänzen)

- [ ] **Step 1: Tests schreiben**

`crates/planora-kernel/tests/profile.rs`:

```rust
use planora_kernel::*;
use planora_math::*;

fn close(a: f64, b: f64, eps: f64) {
    assert!((a - b).abs() < eps, "{a} != {b}");
}

#[test]
fn rectangle_area_and_orientation() {
    let r = ProfileLoop::rectangle(DVec2::new(0.0, 0.0), DVec2::new(2.0, 3.0));
    close(r.signed_area(), 6.0, 1e-12);
    close(r.reversed().signed_area(), -6.0, 1e-12);
}

#[test]
fn circle_area_is_close_to_pi_r_squared() {
    let c = ProfileLoop::circle(DVec2::new(1.0, 1.0), 2.0);
    close(c.signed_area() / (PI * 4.0), 1.0, 0.005);
}

#[test]
fn arc_end_point_is_rotated_start() {
    let end = segment_end(
        DVec2::new(1.0, 0.0),
        &Segment::Arc { center: DVec2::ZERO, sweep: PI / 2.0 },
    );
    close(end.x, 0.0, 1e-12);
    close(end.y, 1.0, 1e-12);
}

#[test]
fn reversed_twice_visits_same_points() {
    let r = ProfileLoop::polygon(&[
        DVec2::new(0.0, 0.0),
        DVec2::new(4.0, 0.0),
        DVec2::new(4.0, 2.0),
        DVec2::new(0.0, 2.0),
    ]);
    let rr = r.reversed().reversed();
    assert_eq!(r.chain_points().len(), rr.chain_points().len());
    for (a, b) in r.chain_points().iter().zip(rr.chain_points().iter()) {
        assert!(a.distance(*b) < 1e-12);
    }
}

#[test]
fn closed_and_open_loops_are_detected() {
    let tol = Tolerance::default();
    let closed = ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(1.0, 1.0));
    assert!(closed.is_closed(&tol));
    let open = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line { to: DVec2::new(1.0, 0.0) },
            Segment::Line { to: DVec2::new(1.0, 1.0) },
        ],
    };
    assert!(!open.is_closed(&tol));
}

#[test]
fn oriented_profile_has_ccw_outer_and_cw_holes() {
    let cw_outer = ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0)).reversed();
    let ccw_hole = ProfileLoop::circle(DVec2::new(5.0, 5.0), 1.0);
    let p = Profile { outer: cw_outer, holes: vec![ccw_hole] }.oriented();
    assert!(p.outer.signed_area() > 0.0);
    assert!(p.holes[0].signed_area() < 0.0);
}
```

`crates/planora-kernel/tests/extrude.rs`:

```rust
use planora_kernel::*;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn counts(s: &Solid) -> (usize, usize, usize, usize, usize) {
    let r = validate(s).expect("Körper muss gültig sein");
    (r.vertices, r.edges, r.faces, r.inner_loops, r.genus)
}

fn rect_profile(w: f64, h: f64) -> Profile {
    Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(w, h)),
        holes: vec![],
    }
}

#[test]
fn box_is_valid_genus_zero() {
    let s = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    assert_eq!(counts(&s), (8, 12, 6, 0, 0));
}

#[test]
fn box_face_normals_point_outward() {
    let s = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    let center = DVec3::new(1.0, 1.5, 2.0);
    for (_, f) in s.faces.iter() {
        let Surface::Plane { frame } = &f.surface else {
            panic!("Quader hat nur ebene Flächen")
        };
        let n = if f.same_sense { frame.z } else { -frame.z };
        assert!(n.dot(frame.origin - center) > 0.0, "Normale zeigt nach innen");
    }
}

#[test]
fn cylinder_is_valid_genus_zero() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    assert_eq!(counts(&s), (2, 3, 3, 0, 0));
}

#[test]
fn plate_with_round_hole_has_genus_one() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&p, &Frame::WORLD, 5.0).unwrap();
    assert_eq!(counts(&s), (10, 15, 7, 2, 1));
}

#[test]
fn plate_with_two_holes_has_genus_two() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(40.0, 20.0)),
        holes: vec![
            ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0),
            ProfileLoop::circle(DVec2::new(30.0, 10.0), 3.0),
        ],
    };
    let s = k().extrude(&p, &Frame::WORLD, 5.0).unwrap();
    assert_eq!(counts(&s).4, 2);
}

#[test]
fn clockwise_outer_and_ccw_hole_are_fixed_automatically() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)).reversed(),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&p, &Frame::WORLD, 5.0).unwrap();
    assert_eq!(counts(&s), (10, 15, 7, 2, 1));
}

#[test]
fn concave_l_shape_is_valid() {
    let p = Profile {
        outer: ProfileLoop::polygon(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 3.0),
            DVec2::new(0.0, 3.0),
        ]),
        holes: vec![],
    };
    let s = k().extrude(&p, &Frame::WORLD, 2.0).unwrap();
    assert_eq!(counts(&s), (12, 18, 8, 0, 0));
}

#[test]
fn notched_bar_with_concave_arc_is_valid() {
    let outer = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line { to: DVec2::new(4.0, 0.0) },
            Segment::Line { to: DVec2::new(4.0, 2.0) },
            Segment::Line { to: DVec2::new(3.0, 2.0) },
            Segment::Arc { center: DVec2::new(2.0, 2.0), sweep: -PI },
            Segment::Line { to: DVec2::new(0.0, 2.0) },
            Segment::Line { to: DVec2::new(0.0, 0.0) },
        ],
    };
    let s = k().extrude(&Profile { outer, holes: vec![] }, &Frame::WORLD, 3.0).unwrap();
    assert_eq!(counts(&s), (12, 18, 8, 0, 0));
}

#[test]
fn extrusion_in_a_tilted_frame_is_valid() {
    let frame = Frame::from_origin_normal(DVec3::new(5.0, 6.0, 7.0), DVec3::new(1.0, 2.0, 3.0));
    let s = k().extrude(&rect_profile(2.0, 3.0), &frame, 4.0).unwrap();
    assert_eq!(counts(&s), (8, 12, 6, 0, 0));
}

#[test]
fn invalid_dimensions_are_rejected_without_panic() {
    let bad = [0.0, -1.0, f64::NAN, f64::INFINITY * 0.0];
    for d in bad {
        assert!(matches!(
            k().make_box(DVec3::new(d, 1.0, 1.0)),
            Err(KernelError::InvalidDimension(_))
        ));
        assert!(matches!(
            k().make_cylinder(d, 1.0),
            Err(KernelError::InvalidDimension(_))
        ));
        assert!(matches!(
            k().make_cylinder(1.0, d),
            Err(KernelError::InvalidDimension(_))
        ));
        assert!(matches!(
            k().extrude(&rect_profile(1.0, 1.0), &Frame::WORLD, d),
            Err(KernelError::InvalidDimension(_))
        ));
    }
}

#[test]
fn zero_area_profile_is_rejected() {
    let r = k().extrude(&rect_profile(0.0, 5.0), &Frame::WORLD, 1.0);
    assert!(matches!(
        r,
        Err(KernelError::ZeroAreaProfile) | Err(KernelError::DegenerateSegment)
    ));
}

#[test]
fn open_profile_is_rejected() {
    let open = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line { to: DVec2::new(1.0, 0.0) },
                Segment::Line { to: DVec2::new(1.0, 1.0) },
            ],
        },
        holes: vec![],
    };
    assert_eq!(
        k().extrude(&open, &Frame::WORLD, 1.0).unwrap_err(),
        KernelError::ProfileNotClosed
    );
}

#[test]
fn validator_detects_a_flipped_half_edge() {
    let mut s = k().make_box(DVec3::ONE).unwrap();
    let id = Id::<Face>::from_index(0);
    let he = &mut s.faces[id].outer.half_edges[0];
    he.forward = !he.forward;
    assert!(validate(&s).is_err());
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel`
Expected: FAIL (Kompilierfehler: `ProfileLoop`, `validate`, `AnalyticKernel`, … nicht gefunden).

- [ ] **Step 3: Implementieren**

`crates/planora-kernel/src/profile.rs`:

```rust
use planora_math::{DVec2, TAU, Tolerance};

/// Profilsegment. Der Startpunkt ist immer das Ende des vorigen Segments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Line { to: DVec2 },
    /// Kreisbogen um `center`; `sweep` in Radiant, positiv = gegen den Uhrzeigersinn.
    Arc { center: DVec2, sweep: f64 },
}

fn rotate(v: DVec2, angle: f64) -> DVec2 {
    let (s, c) = angle.sin_cos();
    DVec2::new(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// Endpunkt eines Segments, das bei `from` beginnt.
pub fn segment_end(from: DVec2, seg: &Segment) -> DVec2 {
    match seg {
        Segment::Line { to } => *to,
        Segment::Arc { center, sweep } => *center + rotate(from - *center, *sweep),
    }
}

/// Folge von Segmenten. Als Schleife (Extrusion) muss sie geschlossen sein,
/// als Kette (Rotation) darf sie offen sein.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileLoop {
    pub start: DVec2,
    pub segments: Vec<Segment>,
}

impl ProfileLoop {
    /// Geschlossenes Polygon durch die gegebenen Punkte.
    pub fn polygon(points: &[DVec2]) -> ProfileLoop {
        assert!(points.len() >= 3, "ein Polygon braucht mindestens drei Punkte");
        ProfileLoop {
            start: points[0],
            segments: points[1..]
                .iter()
                .map(|p| Segment::Line { to: *p })
                .chain(std::iter::once(Segment::Line { to: points[0] }))
                .collect(),
        }
    }

    pub fn rectangle(min: DVec2, max: DVec2) -> ProfileLoop {
        ProfileLoop::polygon(&[
            min,
            DVec2::new(max.x, min.y),
            max,
            DVec2::new(min.x, max.y),
        ])
    }

    /// Vollkreis gegen den Uhrzeigersinn, beginnend bei `center + (radius, 0)`.
    pub fn circle(center: DVec2, radius: f64) -> ProfileLoop {
        ProfileLoop {
            start: center + DVec2::new(radius, 0.0),
            segments: vec![Segment::Arc { center, sweep: TAU }],
        }
    }

    /// Alle Segmentgrenzen: Startpunkt und das Ende jedes Segments (`segments.len() + 1` Punkte).
    pub fn chain_points(&self) -> Vec<DVec2> {
        let mut pts = vec![self.start];
        let mut p = self.start;
        for s in &self.segments {
            p = segment_end(p, s);
            pts.push(p);
        }
        pts
    }

    pub fn is_closed(&self, tol: &Tolerance) -> bool {
        self.chain_points()
            .last()
            .is_some_and(|end| end.distance(self.start) <= tol.linear)
    }

    /// Punktfolge entlang der Kurve; Bögen werden mit `steps_per_turn` Schritten pro Vollkreis genähert.
    pub fn sample(&self, steps_per_turn: usize) -> Vec<DVec2> {
        let mut out = vec![self.start];
        let mut p = self.start;
        for s in &self.segments {
            match s {
                Segment::Line { to } => out.push(*to),
                Segment::Arc { center, sweep } => {
                    let n = ((sweep.abs() / TAU) * steps_per_turn as f64)
                        .ceil()
                        .max(2.0) as usize;
                    for k in 1..=n {
                        out.push(*center + rotate(p - *center, sweep * k as f64 / n as f64));
                    }
                }
            }
            p = segment_end(p, s);
        }
        out
    }

    /// Vorzeichenbehaftete Fläche (positiv = gegen den Uhrzeigersinn). Offene Ketten
    /// werden dabei gedanklich geradlinig geschlossen.
    pub fn signed_area(&self) -> f64 {
        let pts = self.sample(64);
        let n = pts.len();
        (0..n).map(|i| pts[i].perp_dot(pts[(i + 1) % n])).sum::<f64>() / 2.0
    }

    /// Dieselbe Kurve in umgekehrter Richtung.
    pub fn reversed(&self) -> ProfileLoop {
        let pts = self.chain_points();
        let m = self.segments.len();
        let segments = (0..m)
            .rev()
            .map(|i| match self.segments[i] {
                Segment::Line { .. } => Segment::Line { to: pts[i] },
                Segment::Arc { center, sweep } => Segment::Arc {
                    center,
                    sweep: -sweep,
                },
            })
            .collect();
        ProfileLoop {
            start: pts[m],
            segments,
        }
    }
}

/// Ebenes Profil: eine äußere Schleife und beliebig viele Löcher.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub outer: ProfileLoop,
    pub holes: Vec<ProfileLoop>,
}

impl Profile {
    /// Äußere Schleife gegen, Löcher im Uhrzeigersinn.
    pub fn oriented(&self) -> Profile {
        let outer = if self.outer.signed_area() < 0.0 {
            self.outer.reversed()
        } else {
            self.outer.clone()
        };
        let holes = self
            .holes
            .iter()
            .map(|h| {
                if h.signed_area() > 0.0 {
                    h.reversed()
                } else {
                    h.clone()
                }
            })
            .collect();
        Profile { outer, holes }
    }
}
```

`crates/planora-kernel/src/validate.rs`:

```rust
use crate::topology::Solid;
use crate::geometry::Surface;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("Kante {0} wird {1}-mal verwendet (erwartet: 2)")]
    EdgeUseCount(usize, usize),
    #[error("Kante {0} wird nicht genau einmal vorwärts und einmal rückwärts durchlaufen")]
    EdgeOrientation(usize),
    #[error("Eine Schleife der Fläche {0} ist nicht geschlossen")]
    OpenLoop(usize),
    #[error("Eine Schleife der Fläche {0} ist leer")]
    EmptyLoop(usize),
    #[error("Die gekrümmte Fläche {0} hat keinen Parameterbereich")]
    MissingPatch(usize),
    #[error("Die Euler-Poincaré-Formel liefert kein ganzzahliges Geschlecht (V-E+F-R = {0})")]
    InvalidEulerCharacteristic(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Zahl der Ecken, die von mindestens einer Kante benutzt werden.
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    /// Zahl der inneren Schleifen (Löcher in Flächen).
    pub inner_loops: usize,
    /// Zahl der „Henkel“ (Torus = 1).
    pub genus: usize,
}

/// Prüft Topologie und Orientierung. Dass die Normalen nach außen zeigen, wird nicht hier,
/// sondern in `planora-mesh` über ein positives Volumen geprüft.
pub fn validate(solid: &Solid) -> Result<ValidationReport, ValidationError> {
    let n_edges = solid.edges.len();
    let mut forward = vec![0usize; n_edges];
    let mut backward = vec![0usize; n_edges];
    let mut inner_loops = 0;

    for (fid, face) in solid.faces.iter() {
        if !matches!(face.surface, Surface::Plane { .. }) && face.patch.is_none() {
            return Err(ValidationError::MissingPatch(fid.index()));
        }
        inner_loops += face.inner.len();
        for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
            if lp.half_edges.is_empty() {
                return Err(ValidationError::EmptyLoop(fid.index()));
            }
            for (i, he) in lp.half_edges.iter().enumerate() {
                if he.forward {
                    forward[he.edge.index()] += 1;
                } else {
                    backward[he.edge.index()] += 1;
                }
                let next = lp.half_edges[(i + 1) % lp.half_edges.len()];
                if solid.half_edge_end(*he) != solid.half_edge_start(next) {
                    return Err(ValidationError::OpenLoop(fid.index()));
                }
            }
        }
    }

    for e in 0..n_edges {
        let total = forward[e] + backward[e];
        if total != 2 {
            return Err(ValidationError::EdgeUseCount(e, total));
        }
        if forward[e] != 1 || backward[e] != 1 {
            return Err(ValidationError::EdgeOrientation(e));
        }
    }

    let used: HashSet<_> = solid
        .edges
        .iter()
        .flat_map(|(_, e)| [e.start, e.end])
        .collect();
    let characteristic =
        used.len() as i64 - n_edges as i64 + solid.faces.len() as i64 - inner_loops as i64;
    let twice_genus = 2 - characteristic;
    if twice_genus < 0 || twice_genus % 2 != 0 {
        return Err(ValidationError::InvalidEulerCharacteristic(characteristic));
    }

    Ok(ValidationReport {
        vertices: used.len(),
        edges: n_edges,
        faces: solid.faces.len(),
        inner_loops,
        genus: (twice_genus / 2) as usize,
    })
}
```

`crates/planora-kernel/src/extrude.rs`:

```rust
use crate::error::KernelError;
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::profile::{Profile, ProfileLoop, Segment};
use crate::topology::{Edge, Face, HalfEdge, Loop, Solid};
use planora_math::{Frame, Tolerance};

/// Kanten einer Schleife auf der Unter- und Oberseite.
struct LoopEdges {
    bottom: Vec<Id<Edge>>,
    top: Vec<Id<Edge>>,
}

/// Extrudiert ein ebenes Profil (in der xy-Ebene von `frame`) um `height` entlang `frame.z`.
/// Äußere Schleife und Löcher werden bei Bedarf automatisch richtig orientiert.
pub fn extrude(
    profile: &Profile,
    frame: &Frame,
    height: f64,
    tol: &Tolerance,
) -> Result<Solid, KernelError> {
    if !(height > tol.linear) {
        return Err(KernelError::InvalidDimension(height));
    }
    let profile = profile.oriented();
    if !(profile.outer.signed_area().abs() > 1e-12) {
        return Err(KernelError::ZeroAreaProfile);
    }

    let mut solid = Solid::new();
    let outer = build_loop(&mut solid, frame, height, &profile.outer, tol)?;
    let holes = profile
        .holes
        .iter()
        .map(|h| build_loop(&mut solid, frame, height, h, tol))
        .collect::<Result<Vec<_>, _>>()?;

    let forward = |edges: &[Id<Edge>]| Loop {
        half_edges: edges
            .iter()
            .map(|&edge| HalfEdge { edge, forward: true })
            .collect(),
    };
    let backward_reversed = |edges: &[Id<Edge>]| Loop {
        half_edges: edges
            .iter()
            .rev()
            .map(|&edge| HalfEdge { edge, forward: false })
            .collect(),
    };

    // Deckel: Normale +z, äußere Schleife gegen den Uhrzeigersinn (von oben gesehen).
    let top_frame = Frame {
        origin: frame.origin + frame.z * height,
        ..*frame
    };
    solid.add_face(Face {
        surface: Surface::Plane { frame: top_frame },
        same_sense: true,
        outer: forward(&outer.top),
        inner: holes.iter().map(|h| forward(&h.top)).collect(),
        patch: None,
    });
    // Boden: Normale -z, alle Schleifen in umgekehrter Richtung.
    solid.add_face(Face {
        surface: Surface::Plane { frame: *frame },
        same_sense: false,
        outer: backward_reversed(&outer.bottom),
        inner: holes.iter().map(|h| backward_reversed(&h.bottom)).collect(),
        patch: None,
    });

    solid.seal();
    Ok(solid)
}

fn side_loop(
    bottom: Id<Edge>,
    top: Id<Edge>,
    vertical_start: Id<Edge>,
    vertical_end: Id<Edge>,
) -> Loop {
    Loop {
        half_edges: vec![
            HalfEdge { edge: bottom, forward: true },
            HalfEdge { edge: vertical_end, forward: true },
            HalfEdge { edge: top, forward: false },
            HalfEdge { edge: vertical_start, forward: false },
        ],
    }
}

fn build_loop(
    solid: &mut Solid,
    frame: &Frame,
    height: f64,
    lp: &ProfileLoop,
    tol: &Tolerance,
) -> Result<LoopEdges, KernelError> {
    if !lp.is_closed(tol) {
        return Err(KernelError::ProfileNotClosed);
    }
    let pts = lp.chain_points();
    let m = lp.segments.len();
    let up = frame.z * height;

    let bottom_v: Vec<_> = (0..m)
        .map(|i| solid.add_vertex(frame.point_2d(pts[i])))
        .collect();
    let top_v: Vec<_> = (0..m)
        .map(|i| solid.add_vertex(frame.point_2d(pts[i]) + up))
        .collect();
    let vertical: Vec<_> = (0..m)
        .map(|i| {
            solid.add_edge(
                Curve::Line {
                    origin: frame.point_2d(pts[i]),
                    dir: frame.z,
                },
                0.0,
                height,
                bottom_v[i],
                top_v[i],
            )
        })
        .collect();

    let mut edges = LoopEdges {
        bottom: Vec::with_capacity(m),
        top: Vec::with_capacity(m),
    };

    for i in 0..m {
        let next = (i + 1) % m;
        match lp.segments[i] {
            Segment::Line { .. } => {
                let a = frame.point_2d(pts[i]);
                let b = frame.point_2d(pts[next]);
                let len = a.distance(b);
                if !(len > tol.linear) {
                    return Err(KernelError::DegenerateSegment);
                }
                let dir = (b - a) / len;
                let be = solid.add_edge(
                    Curve::Line { origin: a, dir },
                    0.0,
                    len,
                    bottom_v[i],
                    bottom_v[next],
                );
                let te = solid.add_edge(
                    Curve::Line { origin: a + up, dir },
                    0.0,
                    len,
                    top_v[i],
                    top_v[next],
                );
                // Normale = dir × z zeigt bei gegen den Uhrzeigersinn laufender Schleife nach außen.
                let side_frame = Frame {
                    origin: a,
                    x: dir,
                    y: frame.z,
                    z: dir.cross(frame.z),
                };
                solid.add_face(Face {
                    surface: Surface::Plane { frame: side_frame },
                    same_sense: true,
                    outer: side_loop(be, te, vertical[i], vertical[next]),
                    inner: vec![],
                    patch: None,
                });
                edges.bottom.push(be);
                edges.top.push(te);
            }
            Segment::Arc { center, sweep } => {
                let radius = pts[i].distance(center);
                if !(radius > tol.linear) || !(sweep.abs() > tol.angular) {
                    return Err(KernelError::DegenerateSegment);
                }
                let rel = pts[i] - center;
                let a0 = rel.y.atan2(rel.x);
                let a1 = a0 + sweep;
                let circle_frame = Frame {
                    origin: frame.point_2d(center),
                    x: frame.x,
                    y: frame.y,
                    z: frame.z,
                };
                let be = solid.add_edge(
                    Curve::Circle { frame: circle_frame, radius },
                    a0,
                    a1,
                    bottom_v[i],
                    bottom_v[next],
                );
                let top_circle = Frame {
                    origin: circle_frame.origin + up,
                    ..circle_frame
                };
                let te = solid.add_edge(
                    Curve::Circle { frame: top_circle, radius },
                    a0,
                    a1,
                    top_v[i],
                    top_v[next],
                );
                // Gegen den Uhrzeigersinn laufende Bögen zeigen nach außen (+radial), sonst nach innen.
                solid.add_face(Face {
                    surface: Surface::Cylinder { frame: circle_frame, radius },
                    same_sense: sweep > 0.0,
                    outer: side_loop(be, te, vertical[i], vertical[next]),
                    inner: vec![],
                    patch: Some(UvRect {
                        u0: a0.min(a1),
                        u1: a0.max(a1),
                        v0: 0.0,
                        v1: height,
                    }),
                });
                edges.bottom.push(be);
                edges.top.push(te);
            }
        }
    }
    Ok(edges)
}
```

`crates/planora-kernel/src/primitives.rs` (Quader und Zylinder; Kugel/Kegel folgen in Task 5):

```rust
use crate::error::KernelError;
use crate::extrude::extrude;
use crate::profile::{Profile, ProfileLoop};
use crate::topology::Solid;
use planora_math::{DVec2, DVec3, Frame, Tolerance};

fn check(dimension: f64, tol: &Tolerance) -> Result<(), KernelError> {
    if dimension > tol.linear {
        Ok(())
    } else {
        Err(KernelError::InvalidDimension(dimension))
    }
}

/// Quader mit einer Ecke im Ursprung, ausgedehnt in +x, +y, +z.
pub fn make_box(size: DVec3, tol: &Tolerance) -> Result<Solid, KernelError> {
    check(size.x, tol)?;
    check(size.y, tol)?;
    check(size.z, tol)?;
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(size.x, size.y)),
        holes: vec![],
    };
    extrude(&profile, &Frame::WORLD, size.z, tol)
}

/// Zylinder um die z-Achse, Boden im Ursprung.
pub fn make_cylinder(radius: f64, height: f64, tol: &Tolerance) -> Result<Solid, KernelError> {
    check(radius, tol)?;
    check(height, tol)?;
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, radius),
        holes: vec![],
    };
    extrude(&profile, &Frame::WORLD, height, tol)
}
```

`crates/planora-kernel/src/kernel.rs` (wird in Task 5/6 erweitert):

```rust
use crate::error::KernelError;
use crate::extrude::extrude;
use crate::primitives;
use crate::profile::Profile;
use crate::topology::Solid;
use crate::validate::{ValidationError, ValidationReport, validate};
use planora_math::{DVec3, Frame, Tolerance};

/// Die kleine Schnittstelle, über die Oberfläche und andere Crates mit dem Kernel sprechen.
pub trait Kernel {
    fn make_box(&self, size: DVec3) -> Result<Solid, KernelError>;
    fn make_cylinder(&self, radius: f64, height: f64) -> Result<Solid, KernelError>;
    fn extrude(&self, profile: &Profile, frame: &Frame, height: f64)
    -> Result<Solid, KernelError>;
    fn validate(&self, solid: &Solid) -> Result<ValidationReport, ValidationError>;
}

/// Kernel mit analytischen Flächen (Ebene, Zylinder, Kegel, Kugel, Torus).
#[derive(Clone, Copy, Debug, Default)]
pub struct AnalyticKernel {
    pub tolerance: Tolerance,
}

impl Kernel for AnalyticKernel {
    fn make_box(&self, size: DVec3) -> Result<Solid, KernelError> {
        primitives::make_box(size, &self.tolerance)
    }

    fn make_cylinder(&self, radius: f64, height: f64) -> Result<Solid, KernelError> {
        primitives::make_cylinder(radius, height, &self.tolerance)
    }

    fn extrude(
        &self,
        profile: &Profile,
        frame: &Frame,
        height: f64,
    ) -> Result<Solid, KernelError> {
        extrude(profile, frame, height, &self.tolerance)
    }

    fn validate(&self, solid: &Solid) -> Result<ValidationReport, ValidationError> {
        validate(solid)
    }
}
```

`crates/planora-kernel/src/lib.rs` ersetzen durch:

```rust
//! Geometrie-Kernel von PlanoraCAD (B-Rep mit analytischen Flächen).

pub mod error;
pub mod extrude;
pub mod geometry;
pub mod ids;
pub mod kernel;
pub mod primitives;
pub mod profile;
pub mod topology;
pub mod validate;

pub use error::KernelError;
pub use extrude::extrude;
pub use geometry::{Curve, Surface, UvRect};
pub use ids::{Arena, Id};
pub use kernel::{AnalyticKernel, Kernel};
pub use profile::{Profile, ProfileLoop, Segment, segment_end};
pub use topology::{Edge, Face, HalfEdge, Loop, Shell, Solid, Vertex};
pub use validate::{ValidationError, ValidationReport, validate};
```

Hinweis: `pub mod extrude;` und `pub use extrude::extrude;` benennen Modul und Funktion gleich; das ist in Rust erlaubt (verschiedene Namensräume). Falls Clippy/Compiler sich beschwert, das Modul in `extrusion` umbenennen und nur die Funktion exportieren.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel`
Expected: PASS (alle Tests aus Task 3 und 4).

- [ ] **Step 5: Commit**

```powershell
git add crates/planora-kernel
git commit -m "feat(kernel): Validator, Profile, Extrusion, Quader und Zylinder" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Kernel – Rotation, Kugel, Kegel

**Files:**
- Create: `crates/planora-kernel/src/revolve.rs`
- Modify: `src/primitives.rs`, `src/kernel.rs`, `src/lib.rs`
- Test: `crates/planora-kernel/tests/revolve.rs`

**Interfaces:**
- Consumes: Task 3 und 4 (`ProfileLoop`, `Segment`, `segment_end`, `Solid`, `Face`, `Loop`, `HalfEdge`).
- Produces:
  - `revolve(chain: &ProfileLoop, frame: &Frame, tol: &Tolerance) -> Result<Solid, KernelError>`. Die Kette liegt in der Halbebene `(r, z)` des Frames: `DVec2.x` = Abstand von der Achse (≥ 0), `DVec2.y` = Höhe entlang `frame.z` (Drehachse), Winkel 0 liegt in Richtung `frame.x`. Anfangs- und Endpunkt müssen auf der Achse (`x ≈ 0`) liegen; innere Punkte müssen `x > tol` haben.
  - `primitives::{make_sphere(radius,&Tolerance), make_cone(radius,height,&Tolerance)}`
  - `Kernel::{make_sphere, make_cone, revolve}`

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/revolve.rs`

```rust
use planora_kernel::*;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn counts(s: &Solid) -> (usize, usize, usize, usize, usize) {
    let r = validate(s).expect("Körper muss gültig sein");
    (r.vertices, r.edges, r.faces, r.inner_loops, r.genus)
}

fn chain(start: DVec2, segments: Vec<Segment>) -> ProfileLoop {
    ProfileLoop { start, segments }
}

#[test]
fn sphere_is_valid() {
    let s = k().make_sphere(5.0).unwrap();
    assert_eq!(counts(&s), (2, 1, 1, 0, 0));
    assert!(matches!(s.faces.iter().next().unwrap().1.surface, Surface::Sphere { .. }));
}

#[test]
fn cone_is_valid() {
    let s = k().make_cone(3.0, 6.0).unwrap();
    assert_eq!(counts(&s), (2, 2, 2, 0, 0));
}

#[test]
fn cylinder_by_revolution_is_valid() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Line { to: DVec2::new(2.0, 0.0) },
            Segment::Line { to: DVec2::new(2.0, 5.0) },
            Segment::Line { to: DVec2::new(0.0, 5.0) },
        ],
    );
    let s = k().revolve(&c, &Frame::WORLD).unwrap();
    assert_eq!(counts(&s), (2, 3, 3, 0, 0));
}

#[test]
fn capsule_with_sphere_caps_is_valid() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Arc { center: DVec2::new(0.0, 10.0), sweep: PI / 2.0 },
            Segment::Line { to: DVec2::new(10.0, 30.0) },
            Segment::Arc { center: DVec2::new(0.0, 30.0), sweep: PI / 2.0 },
        ],
    );
    let s = k().revolve(&c, &Frame::WORLD).unwrap();
    validate(&s).unwrap();
    assert_eq!(s.faces.len(), 3);
}

#[test]
fn puck_with_rounded_edge_uses_a_torus_patch() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Line { to: DVec2::new(3.0, 0.0) },
            Segment::Arc { center: DVec2::new(3.0, 1.0), sweep: PI / 2.0 },
            Segment::Line { to: DVec2::new(0.0, 1.0) },
        ],
    );
    let s = k().revolve(&c, &Frame::WORLD).unwrap();
    validate(&s).unwrap();
    assert!(s.faces.iter().any(|(_, f)| matches!(f.surface, Surface::Torus { .. })));
}

#[test]
fn clockwise_chain_is_reoriented() {
    let c = chain(
        DVec2::new(0.0, 5.0),
        vec![
            Segment::Line { to: DVec2::new(2.0, 5.0) },
            Segment::Line { to: DVec2::new(2.0, 0.0) },
            Segment::Line { to: DVec2::new(0.0, 0.0) },
        ],
    );
    let s = k().revolve(&c, &Frame::WORLD).unwrap();
    assert_eq!(counts(&s), (2, 3, 3, 0, 0));
}

#[test]
fn chain_not_starting_on_the_axis_is_rejected() {
    let c = chain(
        DVec2::new(1.0, 0.0),
        vec![
            Segment::Line { to: DVec2::new(2.0, 0.0) },
            Segment::Line { to: DVec2::new(0.0, 3.0) },
        ],
    );
    assert_eq!(
        k().revolve(&c, &Frame::WORLD).unwrap_err(),
        KernelError::ProfileOffAxis
    );
}

#[test]
fn chain_touching_the_axis_in_the_middle_is_rejected() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Line { to: DVec2::new(2.0, 0.0) },
            Segment::Line { to: DVec2::new(0.0, 2.0) },
            Segment::Line { to: DVec2::new(2.0, 4.0) },
            Segment::Line { to: DVec2::new(0.0, 4.0) },
        ],
    );
    assert_eq!(
        k().revolve(&c, &Frame::WORLD).unwrap_err(),
        KernelError::ProfileCrossesAxis
    );
}

#[test]
fn zero_area_chain_is_rejected() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![Segment::Line { to: DVec2::new(0.0, 5.0) }],
    );
    assert!(k().revolve(&c, &Frame::WORLD).is_err());
}

#[test]
fn invalid_dimensions_are_rejected_without_panic() {
    for d in [0.0, -1.0, f64::NAN] {
        assert!(matches!(k().make_sphere(d), Err(KernelError::InvalidDimension(_))));
        assert!(matches!(k().make_cone(d, 1.0), Err(KernelError::InvalidDimension(_))));
        assert!(matches!(k().make_cone(1.0, d), Err(KernelError::InvalidDimension(_))));
    }
}

#[test]
fn revolution_around_a_tilted_axis_is_valid() {
    let frame = Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 1.0, 0.5));
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Line { to: DVec2::new(2.0, 0.0) },
            Segment::Line { to: DVec2::new(0.0, 4.0) },
        ],
    );
    let s = k().revolve(&c, &frame).unwrap();
    assert_eq!(counts(&s), (2, 2, 2, 0, 0));
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test revolve`
Expected: FAIL (Kompilierfehler: `make_sphere`, `revolve` fehlen).

- [ ] **Step 3: Implementieren**

`crates/planora-kernel/src/revolve.rs`:

```rust
use crate::error::KernelError;
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::profile::{ProfileLoop, Segment};
use crate::topology::{Edge, Face, HalfEdge, Loop, Solid};
use planora_math::{DVec2, Frame, TAU, Tolerance};

/// Beschreibung eines gekrümmten Flächenstücks, das durch ein Segment entsteht.
struct Curved {
    surface: Surface,
    /// Parameter `v` am Anfang und am Ende des Segments (in Laufrichtung).
    v_start: f64,
    v_end: f64,
    /// Meridiankurve (Bild des Segments bei u = 0) samt Parameterbereich in Laufrichtung.
    meridian: Curve,
    t0: f64,
    t1: f64,
    /// Laufrichtung in der (r, z)-Halbebene in der Segmentmitte.
    tangent: DVec2,
}

/// Rotiert eine Kette in der (r, z)-Halbebene um die z-Achse von `frame` (360°).
///
/// `chain` muss auf der Achse beginnen und enden; innere Punkte müssen echt neben der Achse liegen.
/// Die Kette wird bei Bedarf so gedreht, dass das Material links der Laufrichtung liegt.
pub fn revolve(
    chain: &ProfileLoop,
    frame: &Frame,
    tol: &Tolerance,
) -> Result<Solid, KernelError> {
    if chain.segments.is_empty() {
        return Err(KernelError::DegenerateSegment);
    }
    let area = chain.signed_area();
    if !(area.abs() > 1e-12) {
        return Err(KernelError::ZeroAreaProfile);
    }
    let chain = if area < 0.0 {
        chain.reversed()
    } else {
        chain.clone()
    };

    let pts = chain.chain_points();
    let m = chain.segments.len();
    if pts[0].x.abs() > tol.linear || pts[m].x.abs() > tol.linear {
        return Err(KernelError::ProfileOffAxis);
    }
    if pts[1..m].iter().any(|p| p.x <= tol.linear) {
        return Err(KernelError::ProfileCrossesAxis);
    }
    if chain.sample(64).iter().any(|p| p.x < -tol.linear) {
        return Err(KernelError::ProfileCrossesAxis);
    }

    let to3 = |p: DVec2| frame.origin + frame.x * p.x + frame.z * p.y;
    let mut solid = Solid::new();
    let vertices: Vec<_> = pts.iter().map(|p| solid.add_vertex(to3(*p))).collect();

    // Parallelkreise an den inneren Punkten.
    let mut circles: Vec<Option<Id<Edge>>> = vec![None; m + 1];
    for i in 1..m {
        let p = pts[i];
        let circle_frame = Frame {
            origin: frame.origin + frame.z * p.y,
            ..*frame
        };
        circles[i] = Some(solid.add_edge(
            Curve::Circle {
                frame: circle_frame,
                radius: p.x,
            },
            0.0,
            TAU,
            vertices[i],
            vertices[i],
        ));
    }

    for i in 0..m {
        let (p, q) = (pts[i], pts[i + 1]);
        let seg = chain.segments[i];

        let curved = match seg {
            Segment::Line { .. } => {
                let d = q - p;
                let len = d.length();
                if !(len > tol.linear) {
                    return Err(KernelError::DegenerateSegment);
                }
                if d.y.abs() <= tol.linear {
                    add_planar_face(&mut solid, frame, &circles, i, p, q);
                    continue;
                }
                let meridian = Curve::Line {
                    origin: to3(p),
                    dir: (to3(q) - to3(p)) / len,
                };
                let surface = if d.x.abs() <= tol.linear {
                    Surface::Cylinder {
                        frame: *frame,
                        radius: p.x,
                    }
                } else {
                    let slope = d.x / d.y;
                    Surface::Cone {
                        frame: *frame,
                        radius: p.x - slope * p.y,
                        slope,
                    }
                };
                Curved {
                    surface,
                    v_start: p.y,
                    v_end: q.y,
                    meridian,
                    t0: 0.0,
                    t1: len,
                    tangent: d / len,
                }
            }
            Segment::Arc { center, sweep } => {
                let radius = (p - center).length();
                if !(radius > tol.linear)
                    || !(sweep.abs() > tol.angular)
                    || sweep.abs() > TAU + 1e-9
                {
                    return Err(KernelError::DegenerateSegment);
                }
                let a0 = (p.y - center.y).atan2(p.x - center.x);
                let a1 = a0 + sweep;
                let a_mid = a0 + sweep / 2.0;
                let tangent = DVec2::new(-a_mid.sin(), a_mid.cos()) * sweep.signum();
                let shifted = Frame {
                    origin: frame.origin + frame.z * center.y,
                    ..*frame
                };
                if center.x.abs() <= tol.linear {
                    // Mittelpunkt auf der Achse → Kugel
                    Curved {
                        surface: Surface::Sphere {
                            frame: shifted,
                            radius,
                        },
                        v_start: a0,
                        v_end: a1,
                        meridian: Curve::Circle {
                            frame: Frame {
                                origin: shifted.origin,
                                x: frame.x,
                                y: frame.z,
                                z: frame.x.cross(frame.z),
                            },
                            radius,
                        },
                        t0: a0,
                        t1: a1,
                        tangent,
                    }
                } else if center.x > tol.linear {
                    // Mittelpunkt neben der Achse → Torus
                    Curved {
                        surface: Surface::Torus {
                            frame: shifted,
                            major: center.x,
                            minor: radius,
                        },
                        v_start: a0,
                        v_end: a1,
                        meridian: Curve::Circle {
                            frame: Frame {
                                origin: frame.origin
                                    + frame.x * center.x
                                    + frame.z * center.y,
                                x: frame.x,
                                y: frame.z,
                                z: frame.x.cross(frame.z),
                            },
                            radius,
                        },
                        t0: a0,
                        t1: a1,
                        tangent,
                    }
                } else {
                    return Err(KernelError::UnsupportedSegment(
                        "Kreisbogen mit Mittelpunkt auf der negativen Seite der Achse",
                    ));
                }
            }
        };

        // Welche Kreise liegen am unteren / oberen Ende des v-Bereichs?
        let along_v = curved.v_end > curved.v_start;
        let (lo, hi) = if along_v { (i, i + 1) } else { (i + 1, i) };
        let meridian = solid.add_edge(
            curved.meridian,
            curved.t0,
            curved.t1,
            vertices[i],
            vertices[i + 1],
        );

        // Kanonische Schleife (gegen den Uhrzeigersinn im (u, v)-Rechteck, also um die natürliche Normale).
        let mut half_edges = Vec::new();
        if let Some(e) = circles[lo] {
            half_edges.push(HalfEdge { edge: e, forward: true });
        }
        half_edges.push(HalfEdge { edge: meridian, forward: along_v });
        if let Some(e) = circles[hi] {
            half_edges.push(HalfEdge { edge: e, forward: false });
        }
        half_edges.push(HalfEdge { edge: meridian, forward: !along_v });
        let mut outer = Loop { half_edges };

        // Nach außen = rechts der Laufrichtung (das Material liegt links).
        let v_mid = 0.5 * (curved.v_start + curved.v_end);
        let u_mid = std::f64::consts::PI;
        let natural = curved.surface.normal(u_mid, v_mid);
        let outward_rz = DVec2::new(curved.tangent.y, -curved.tangent.x);
        let radial = frame.x * u_mid.cos() + frame.y * u_mid.sin();
        let outward = radial * outward_rz.x + frame.z * outward_rz.y;
        let same_sense = natural.dot(outward) > 0.0;
        if !same_sense {
            outer = outer.reversed();
        }

        solid.add_face(Face {
            surface: curved.surface,
            same_sense,
            outer,
            inner: vec![],
            patch: Some(UvRect {
                u0: 0.0,
                u1: TAU,
                v0: curved.v_start.min(curved.v_end),
                v1: curved.v_start.max(curved.v_end),
            }),
        });
    }

    solid.seal();
    Ok(solid)
}

/// Segment senkrecht zur Achse → Kreisscheibe bzw. Kreisring als ebene Fläche.
fn add_planar_face(
    solid: &mut Solid,
    frame: &Frame,
    circles: &[Option<Id<Edge>>],
    i: usize,
    p: DVec2,
    q: DVec2,
) {
    let plane_frame = Frame {
        origin: frame.origin + frame.z * p.y,
        ..*frame
    };
    // Läuft das Segment nach außen (q.x > p.x), zeigt die Außennormale nach -z.
    let same_sense = q.x < p.x;
    let (big, small) = if p.x >= q.x { (i, i + 1) } else { (i + 1, i) };
    let outer_edge = circles[big].expect("der größere Kreis liegt nie auf der Achse");
    let mut outer = Loop {
        half_edges: vec![HalfEdge { edge: outer_edge, forward: true }],
    };
    let mut inner = Vec::new();
    if let Some(e) = circles[small] {
        inner.push(Loop {
            half_edges: vec![HalfEdge { edge: e, forward: false }],
        });
    }
    if !same_sense {
        outer = outer.reversed();
        inner = inner.iter().map(Loop::reversed).collect();
    }
    solid.add_face(Face {
        surface: Surface::Plane { frame: plane_frame },
        same_sense,
        outer,
        inner,
        patch: None,
    });
}
```

`crates/planora-kernel/src/primitives.rs` – oben die Imports erweitern und am Ende anhängen:

```rust
use crate::profile::Segment;
use crate::revolve::revolve;
use planora_math::PI;

/// Kugel um den Ursprung (Pole auf der z-Achse).
pub fn make_sphere(radius: f64, tol: &Tolerance) -> Result<Solid, KernelError> {
    check(radius, tol)?;
    let chain = ProfileLoop {
        start: DVec2::new(0.0, -radius),
        segments: vec![Segment::Arc {
            center: DVec2::ZERO,
            sweep: PI,
        }],
    };
    revolve(&chain, &Frame::WORLD, tol)
}

/// Kegel um die z-Achse: Grundkreis (Radius `radius`) im Ursprung, Spitze bei `z = height`.
pub fn make_cone(radius: f64, height: f64, tol: &Tolerance) -> Result<Solid, KernelError> {
    check(radius, tol)?;
    check(height, tol)?;
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(radius, 0.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, height),
            },
        ],
    };
    revolve(&chain, &Frame::WORLD, tol)
}
```

`crates/planora-kernel/src/kernel.rs` – Trait und Implementierung erweitern: im Trait ergänzen

```rust
    fn make_sphere(&self, radius: f64) -> Result<Solid, KernelError>;
    fn make_cone(&self, radius: f64, height: f64) -> Result<Solid, KernelError>;
    fn revolve(&self, chain: &ProfileLoop, frame: &Frame) -> Result<Solid, KernelError>;
```

in `impl Kernel for AnalyticKernel` ergänzen

```rust
    fn make_sphere(&self, radius: f64) -> Result<Solid, KernelError> {
        primitives::make_sphere(radius, &self.tolerance)
    }

    fn make_cone(&self, radius: f64, height: f64) -> Result<Solid, KernelError> {
        primitives::make_cone(radius, height, &self.tolerance)
    }

    fn revolve(&self, chain: &ProfileLoop, frame: &Frame) -> Result<Solid, KernelError> {
        revolve(chain, frame, &self.tolerance)
    }
```

und die Imports `use crate::profile::{Profile, ProfileLoop};` sowie `use crate::revolve::revolve;`.

`crates/planora-kernel/src/lib.rs` ergänzen: `pub mod revolve;` und `pub use revolve::revolve;` (gleiche Namenshinweis wie bei `extrude`).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel`
Expected: PASS (alle Kernel-Tests).

- [ ] **Step 5: Commit**

```powershell
git add crates/planora-kernel
git commit -m "feat(kernel): Rotation sowie Kugel und Kegel" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Kernel – Transformationen

**Files:**
- Create: `crates/planora-kernel/src/transform.rs`
- Modify: `src/kernel.rs`, `src/lib.rs`
- Test: `crates/planora-kernel/tests/transform.rs`

**Interfaces:**
- Consumes: alle Kernel-Typen.
- Produces: `Solid::transformed(&self, m: &DMat4) -> Result<Solid, KernelError>` (Ähnlichkeitstransformationen: verschieben, drehen, spiegeln, gleichmäßig skalieren); `Kernel::transform(&self, &Solid, &DMat4) -> Result<Solid, KernelError>`. Vertex-, Kanten- und Flächen-IDs bleiben erhalten.

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/transform.rs`

```rust
use planora_kernel::*;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn all_primitives() -> Vec<Solid> {
    let puck = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line { to: DVec2::new(3.0, 0.0) },
            Segment::Arc { center: DVec2::new(3.0, 1.0), sweep: PI / 2.0 },
            Segment::Line { to: DVec2::new(0.0, 1.0) },
        ],
    };
    vec![
        k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap(),
        k().make_cylinder(2.0, 5.0).unwrap(),
        k().make_sphere(3.0).unwrap(),
        k().make_cone(2.0, 4.0).unwrap(),
        k().revolve(&puck, &Frame::WORLD).unwrap(),
    ]
}

fn matrices() -> Vec<DMat4> {
    vec![
        DMat4::from_translation(DVec3::new(10.0, -5.0, 2.0)),
        DMat4::from_axis_angle(DVec3::new(1.0, 2.0, 3.0).normalize(), 0.9),
        DMat4::from_scale(DVec3::splat(2.5)),
        DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)),
        DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0))
            * DMat4::from_axis_angle(DVec3::Y, 1.1)
            * DMat4::from_scale(DVec3::new(-2.0, 2.0, 2.0)),
    ]
}

#[test]
fn every_primitive_stays_valid_under_every_similarity() {
    for solid in all_primitives() {
        let before = validate(&solid).unwrap();
        for m in matrices() {
            let t = k().transform(&solid, &m).unwrap();
            let after = validate(&t).unwrap();
            assert_eq!(before, after);
        }
    }
}

#[test]
fn vertices_move_with_the_matrix() {
    let solid = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    let m = DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0))
        * DMat4::from_axis_angle(DVec3::Z, 0.5);
    let t = k().transform(&solid, &m).unwrap();
    for ((_, a), (_, b)) in solid.vertices.iter().zip(t.vertices.iter()) {
        assert!(m.transform_point3(a.point).distance(b.point) < 1e-12);
    }
}

#[test]
fn identity_is_a_valid_similarity() {
    let solid = k().make_cylinder(1.0, 2.0).unwrap();
    assert!(k().transform(&solid, &DMat4::IDENTITY).is_ok());
}

#[test]
fn non_similarity_matrices_are_rejected() {
    let solid = k().make_box(DVec3::ONE).unwrap();
    let bad = [
        DMat4::from_scale(DVec3::new(1.0, 2.0, 1.0)),
        DMat4::from_scale(DVec3::ZERO),
        DMat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, //
            0.5, 1.0, 0.0, 0.0, // Scherung
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ]),
        DMat4::from_scale(DVec3::splat(f64::NAN)),
    ];
    for m in bad {
        assert_eq!(
            k().transform(&solid, &m).unwrap_err(),
            KernelError::NonSimilarityTransform
        );
    }
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test transform`
Expected: FAIL (Kompilierfehler: `Kernel::transform` fehlt).

- [ ] **Step 3: Implementieren**

`crates/planora-kernel/src/transform.rs`:

```rust
use crate::error::KernelError;
use crate::geometry::{Curve, Surface, UvRect};
use crate::topology::{Face, Loop, Solid};
use planora_math::DMat4;

impl Solid {
    /// Wendet eine Ähnlichkeitstransformation an (verschieben, drehen, spiegeln,
    /// gleichmäßig skalieren). IDs von Ecken, Kanten und Flächen bleiben erhalten.
    pub fn transformed(&self, m: &DMat4) -> Result<Solid, KernelError> {
        let det = m.determinant();
        let s = det.abs().cbrt();
        let cols = [
            m.x_axis.truncate(),
            m.y_axis.truncate(),
            m.z_axis.truncate(),
        ];
        let eps = 1e-9 * s.max(1.0);
        let is_similarity = s.is_finite()
            && s > 1e-12
            && cols.iter().all(|c| (c.length() - s).abs() <= eps)
            && cols[0].dot(cols[1]).abs() <= eps * s
            && cols[0].dot(cols[2]).abs() <= eps * s
            && cols[1].dot(cols[2]).abs() <= eps * s
            && m.x_axis.w == 0.0
            && m.y_axis.w == 0.0
            && m.z_axis.w == 0.0
            && m.w_axis.w == 1.0;
        if !is_similarity {
            return Err(KernelError::NonSimilarityTransform);
        }
        let mirrored = det < 0.0;

        let mut out = Solid::new();
        for (_, v) in self.vertices.iter() {
            out.add_vertex(m.transform_point3(v.point));
        }
        for (_, e) in self.edges.iter() {
            let (curve, t0, t1) = transform_curve(&e.curve, e.t0, e.t1, m, s);
            out.add_edge(curve, t0, t1, e.start, e.end);
        }
        for (_, f) in self.faces.iter() {
            let (surface, same_sense, patch) = transform_surface(f, m, s, mirrored);
            let fix = |l: &Loop| if mirrored { l.reversed() } else { l.clone() };
            out.add_face(Face {
                surface,
                same_sense,
                outer: fix(&f.outer),
                inner: f.inner.iter().map(fix).collect(),
                patch,
            });
        }
        out.shells = self.shells.clone();
        Ok(out)
    }
}

fn transform_curve(c: &Curve, t0: f64, t1: f64, m: &DMat4, s: f64) -> (Curve, f64, f64) {
    match c {
        Curve::Line { origin, dir } => (
            Curve::Line {
                origin: m.transform_point3(*origin),
                dir: m.transform_vector3(*dir).normalize(),
            },
            t0 * s,
            t1 * s,
        ),
        Curve::Circle { frame, radius } => (
            Curve::Circle {
                frame: frame.transformed(m),
                radius: radius * s,
            },
            t0,
            t1,
        ),
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => (
            Curve::Ellipse {
                frame: frame.transformed(m),
                major: major * s,
                minor: minor * s,
            },
            t0,
            t1,
        ),
    }
}

/// Bei einer Spiegelung bleibt die Parametrisierung `(u, v)` erhalten, außer dass `v` (die Achse
/// bzw. der Breitenwinkel) das Vorzeichen wechselt. Ebenen behalten `(u, v)`, ihre natürliche
/// Normale `x × y` zeigt aber nach der Spiegelung entgegengesetzt, daher kippt `same_sense`.
fn transform_surface(
    f: &Face,
    m: &DMat4,
    s: f64,
    mirrored: bool,
) -> (Surface, bool, Option<UvRect>) {
    let length_range = |r: UvRect| {
        if mirrored {
            UvRect {
                v0: -s * r.v1,
                v1: -s * r.v0,
                ..r
            }
        } else {
            UvRect {
                v0: s * r.v0,
                v1: s * r.v1,
                ..r
            }
        }
    };
    let angle_range = |r: UvRect| {
        if mirrored {
            UvRect {
                v0: -r.v1,
                v1: -r.v0,
                ..r
            }
        } else {
            r
        }
    };
    match &f.surface {
        Surface::Plane { frame } => (
            Surface::Plane {
                frame: frame.transformed(m),
            },
            if mirrored { !f.same_sense } else { f.same_sense },
            None,
        ),
        Surface::Cylinder { frame, radius } => (
            Surface::Cylinder {
                frame: frame.transformed(m),
                radius: radius * s,
            },
            f.same_sense,
            f.patch.map(length_range),
        ),
        Surface::Cone {
            frame,
            radius,
            slope,
        } => (
            Surface::Cone {
                frame: frame.transformed(m),
                radius: radius * s,
                slope: if mirrored { -slope } else { *slope },
            },
            f.same_sense,
            f.patch.map(length_range),
        ),
        Surface::Sphere { frame, radius } => (
            Surface::Sphere {
                frame: frame.transformed(m),
                radius: radius * s,
            },
            f.same_sense,
            f.patch.map(angle_range),
        ),
        Surface::Torus {
            frame,
            major,
            minor,
        } => (
            Surface::Torus {
                frame: frame.transformed(m),
                major: major * s,
                minor: minor * s,
            },
            f.same_sense,
            f.patch.map(angle_range),
        ),
    }
}
```

`crates/planora-kernel/src/kernel.rs`: im Trait `fn transform(&self, solid: &Solid, matrix: &DMat4) -> Result<Solid, KernelError>;`, in der Implementierung

```rust
    fn transform(&self, solid: &Solid, matrix: &DMat4) -> Result<Solid, KernelError> {
        solid.transformed(matrix)
    }
```

und `DMat4` in den `planora_math`-Import aufnehmen. In `lib.rs`: `mod transform;` (Methode hängt an `Solid`, kein Re-Export nötig).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel`
Expected: PASS. Hinweis: Dass gespiegelte Körper auch ein **positives Volumen** haben (Orientierung stimmt), prüft Task 7.

- [ ] **Step 5: Commit**

```powershell
git add crates/planora-kernel
git commit -m "feat(kernel): Ähnlichkeitstransformationen inklusive Spiegelung" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: `planora-mesh` – Tessellierung, Masseneigenschaften, Geschlossenheitsprüfung

**Files:**
- Create: `crates/planora-mesh/Cargo.toml`, `src/{lib,mesh,discretize,tessellate,mass,check}.rs`
- Test: `crates/planora-mesh/tests/{tessellate,transforms}.rs`

**Interfaces:**
- Consumes: `planora_kernel::{Solid, Face, Edge, Id, Surface, Curve, UvRect, AnalyticKernel, Kernel, …}`, `planora_math::DVec3`
- Produces:
  - `Mesh { positions: Vec<DVec3>, normals: Vec<DVec3>, indices: Vec<u32>, face_ranges: Vec<FaceRange>, edge_lines: Vec<EdgeLine> }` (`Default, Clone, Debug`)
  - `FaceRange { face: Id<Face>, first_index: u32, index_count: u32 }`, `EdgeLine { edge: Id<Edge>, points: Vec<DVec3> }`
  - `TessellationOptions { chord_tolerance: f64 }` (`Default` = 0.01)
  - `tessellate(&Solid, &TessellationOptions) -> Result<Mesh, MeshError>`
  - `mass_properties(&Mesh) -> MassProperties { volume, surface_area, centroid }`
  - `is_watertight(&Mesh, eps: f64) -> bool` (verschweißt Punkte, prüft, dass jede gerichtete Kante genau einmal vorkommt und ihre Gegenkante genau einmal)

- [ ] **Step 1: Cargo.toml und Tests schreiben**

`crates/planora-mesh/Cargo.toml`:

```toml
[package]
name = "planora-mesh"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
planora-math.workspace = true
planora-kernel.workspace = true
earcutr.workspace = true
thiserror.workspace = true

[dev-dependencies]
proptest.workspace = true

[lints]
workspace = true
```

`crates/planora-mesh/tests/tessellate.rs`:

```rust
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn opts() -> TessellationOptions {
    TessellationOptions { chord_tolerance: 0.001 }
}

fn mesh_of(s: &Solid) -> Mesh {
    tessellate(s, &opts()).unwrap()
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn puck() -> ProfileLoop {
    ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line { to: DVec2::new(3.0, 0.0) },
            Segment::Arc { center: DVec2::new(3.0, 1.0), sweep: PI / 2.0 },
            Segment::Line { to: DVec2::new(0.0, 1.0) },
        ],
    }
}

#[test]
fn box_has_exact_volume_area_and_centroid() {
    let s = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, 24.0) < 1e-12);
    assert!(rel(p.surface_area, 52.0) < 1e-12);
    assert!(p.centroid.distance(DVec3::new(1.0, 1.5, 2.0)) < 1e-9);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn cylinder_volume_matches_formula() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, PI * 25.0 * 10.0) < 0.005, "{}", p.volume);
    assert!(p.centroid.distance(DVec3::new(0.0, 0.0, 5.0)) < 1e-6);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn sphere_volume_matches_formula() {
    let s = k().make_sphere(5.0).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, 4.0 / 3.0 * PI * 125.0) < 0.005, "{}", p.volume);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn cone_volume_matches_formula() {
    let s = k().make_cone(3.0, 6.0).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, PI * 9.0 * 6.0 / 3.0) < 0.005, "{}", p.volume);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn rounded_puck_volume_matches_pappus() {
    let s = k().revolve(&puck(), &Frame::WORLD).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    // V = 2π ∫ x dA = 2π (4,5 + 3π/4 + 1/3)
    let expected = 2.0 * PI * (4.5 + 3.0 * PI / 4.0 + 1.0 / 3.0);
    assert!(rel(p.volume, expected) < 0.005, "{} vs {expected}", p.volume);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn plate_with_hole_is_watertight_with_correct_volume() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, 2000.0 - PI * 9.0 * 5.0) < 0.005, "{}", p.volume);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn notched_bar_volume_matches_formula() {
    let outer = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line { to: DVec2::new(4.0, 0.0) },
            Segment::Line { to: DVec2::new(4.0, 2.0) },
            Segment::Line { to: DVec2::new(3.0, 2.0) },
            Segment::Arc { center: DVec2::new(2.0, 2.0), sweep: -PI },
            Segment::Line { to: DVec2::new(0.0, 2.0) },
            Segment::Line { to: DVec2::new(0.0, 0.0) },
        ],
    };
    let s = k()
        .extrude(&Profile { outer, holes: vec![] }, &Frame::WORLD, 3.0)
        .unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, 3.0 * (8.0 - PI / 2.0)) < 0.005, "{}", p.volume);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn concave_l_shape_is_watertight() {
    let p = Profile {
        outer: ProfileLoop::polygon(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 3.0),
            DVec2::new(0.0, 3.0),
        ]),
        holes: vec![],
    };
    let s = k().extrude(&p, &Frame::WORLD, 2.0).unwrap();
    let m = mesh_of(&s);
    assert!(rel(mass_properties(&m).volume, 2.0 * (4.0 + 2.0)) < 1e-12);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn face_ranges_cover_every_face_and_normals_are_unit() {
    let s = k().make_cylinder(2.0, 3.0).unwrap();
    let m = mesh_of(&s);
    assert_eq!(m.face_ranges.len(), s.faces.len());
    assert_eq!(m.indices.len() % 3, 0);
    let total: u32 = m.face_ranges.iter().map(|r| r.index_count).sum();
    assert_eq!(total as usize, m.indices.len());
    assert_eq!(m.positions.len(), m.normals.len());
    for n in &m.normals {
        assert!((n.length() - 1.0).abs() < 1e-9);
    }
    assert_eq!(m.edge_lines.len(), s.edges.len());
}

#[test]
fn huge_and_tiny_bodies_keep_reasonable_triangle_counts() {
    let big = k().make_cylinder(1.0e5, 10.0).unwrap();
    let m = tessellate(&big, &TessellationOptions { chord_tolerance: 0.01 }).unwrap();
    assert!(m.indices.len() < 100_000, "{}", m.indices.len());

    let tiny = k().make_cylinder(0.01, 0.02).unwrap();
    let m = tessellate(&tiny, &TessellationOptions { chord_tolerance: 0.01 }).unwrap();
    let side = m
        .face_ranges
        .iter()
        .map(|r| r.index_count)
        .max()
        .unwrap();
    assert!(side >= 16 * 3, "{side}");
}
```

`crates/planora-mesh/tests/transforms.rs`:

```rust
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;
use proptest::prelude::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn opts() -> TessellationOptions {
    TessellationOptions { chord_tolerance: 0.001 }
}

fn solids() -> Vec<Solid> {
    let puck = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line { to: DVec2::new(3.0, 0.0) },
            Segment::Arc { center: DVec2::new(3.0, 1.0), sweep: PI / 2.0 },
            Segment::Line { to: DVec2::new(0.0, 1.0) },
        ],
    };
    vec![
        k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap(),
        k().make_cylinder(2.0, 5.0).unwrap(),
        k().make_sphere(3.0).unwrap(),
        k().make_cone(2.0, 4.0).unwrap(),
        k().revolve(&puck, &Frame::WORLD).unwrap(),
    ]
}

#[test]
fn mirrored_bodies_keep_positive_volume_and_closed_meshes() {
    let mirrors = [
        DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)),
        DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0))
            * DMat4::from_axis_angle(DVec3::Y, 1.1)
            * DMat4::from_scale(DVec3::new(-2.0, 2.0, 2.0)),
    ];
    for solid in solids() {
        let original = mass_properties(&tessellate(&solid, &opts()).unwrap());
        for m in mirrors {
            let t = k().transform(&solid, &m).unwrap();
            let mesh = tessellate(&t, &opts()).unwrap();
            let p = mass_properties(&mesh);
            let scale = m.determinant().abs();
            assert!(p.volume > 0.0, "Volumen negativ: Orientierung falsch");
            assert!(
                ((p.volume - original.volume * scale) / (original.volume * scale)).abs() < 0.01,
                "{} vs {}",
                p.volume,
                original.volume * scale
            );
            assert!(is_watertight(&mesh, 1e-6));
        }
    }
}

#[test]
fn rotated_and_translated_bodies_keep_volume() {
    let m = DMat4::from_translation(DVec3::new(10.0, -5.0, 2.0))
        * DMat4::from_axis_angle(DVec3::new(1.0, 2.0, 3.0).normalize(), 0.9);
    for solid in solids() {
        let original = mass_properties(&tessellate(&solid, &opts()).unwrap());
        let t = k().transform(&solid, &m).unwrap();
        let mesh = tessellate(&t, &opts()).unwrap();
        let p = mass_properties(&mesh);
        assert!(((p.volume - original.volume) / original.volume).abs() < 1e-6);
        assert!(is_watertight(&mesh, 1e-6));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn rigid_transforms_preserve_box_volume(
        sx in 0.1f64..50.0, sy in 0.1f64..50.0, sz in 0.1f64..50.0,
        ax in -1.0f64..1.0, ay in -1.0f64..1.0, az in 0.2f64..1.0,
        angle in 0.0f64..6.28,
        tx in -100.0f64..100.0, ty in -100.0f64..100.0, tz in -100.0f64..100.0,
    ) {
        let axis = DVec3::new(ax, ay, az).normalize();
        let m = DMat4::from_translation(DVec3::new(tx, ty, tz)) * DMat4::from_axis_angle(axis, angle);
        let solid = k().make_box(DVec3::new(sx, sy, sz)).unwrap();
        let t = k().transform(&solid, &m).unwrap();
        prop_assert!(k().validate(&t).is_ok());
        let mesh = tessellate(&t, &opts()).unwrap();
        let p = mass_properties(&mesh);
        let expected = sx * sy * sz;
        prop_assert!(((p.volume - expected) / expected).abs() < 1e-6);
        prop_assert!(is_watertight(&mesh, 1e-6));
    }
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-mesh`
Expected: FAIL (Kompilierfehler: `tessellate`, `mass_properties`, … fehlen).

- [ ] **Step 3: Implementieren**

`crates/planora-mesh/src/lib.rs`:

```rust
//! Tessellierung: Körper (B-Rep) → Dreiecksnetz, plus Masseneigenschaften und Netzprüfung.

mod check;
mod discretize;
mod mass;
mod mesh;
mod tessellate;

pub use check::is_watertight;
pub use mass::{MassProperties, mass_properties};
pub use mesh::{EdgeLine, FaceRange, Mesh, TessellationOptions};
pub use tessellate::{MeshError, tessellate};
```

`crates/planora-mesh/src/mesh.rs`:

```rust
use planora_kernel::{Edge, Face, Id};
use planora_math::DVec3;

/// Dreiecksnetz eines Körpers. Jede Fläche besitzt eigene Eckpunkte (kein Teilen über Flächen
/// hinweg), damit Normalen und Zuordnung zur Fläche eindeutig bleiben.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub indices: Vec<u32>,
    pub face_ranges: Vec<FaceRange>,
    pub edge_lines: Vec<EdgeLine>,
}

/// Indexbereich `first_index .. first_index + index_count` gehört zu `face`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceRange {
    pub face: Id<Face>,
    pub first_index: u32,
    pub index_count: u32,
}

/// Linienzug einer Kante für die Darstellung.
#[derive(Clone, Debug)]
pub struct EdgeLine {
    pub edge: Id<Edge>,
    pub points: Vec<DVec3>,
}

#[derive(Clone, Copy, Debug)]
pub struct TessellationOptions {
    /// Größte erlaubte Abweichung der Sehne vom Kreisbogen in Millimetern.
    pub chord_tolerance: f64,
}

impl Default for TessellationOptions {
    fn default() -> Self {
        Self {
            chord_tolerance: 0.01,
        }
    }
}
```

`crates/planora-mesh/src/discretize.rs`:

```rust
use planora_kernel::{Curve, Edge, Id, Solid, Surface};
use planora_math::DVec3;
use std::f64::consts::PI;

/// Legt fest, in wie viele Segmente Bögen unterteilt werden. Der Winkelschritt hängt nur vom
/// größten Radius des Körpers ab; dadurch bekommen benachbarte Flächen und Kanten mit gleicher
/// Winkelspanne immer dieselben Teilpunkte und das Netz bleibt geschlossen.
#[derive(Clone, Copy, Debug)]
pub struct Discretizer {
    dtheta: f64,
}

impl Discretizer {
    pub fn new(solid: &Solid, chord_tolerance: f64) -> Self {
        let mut r_max = 0.0_f64;
        for (_, e) in solid.edges.iter() {
            r_max = r_max.max(match &e.curve {
                Curve::Line { .. } => 0.0,
                Curve::Circle { radius, .. } => *radius,
                Curve::Ellipse { major, minor, .. } => major.max(*minor),
            });
        }
        for (_, f) in solid.faces.iter() {
            r_max = r_max.max(match &f.surface {
                Surface::Plane { .. } => 0.0,
                Surface::Cylinder { radius, .. } => *radius,
                Surface::Cone { radius, slope, .. } => match f.patch {
                    Some(p) => (radius + slope * p.v0).abs().max((radius + slope * p.v1).abs()),
                    None => *radius,
                },
                Surface::Sphere { radius, .. } => *radius,
                Surface::Torus { major, minor, .. } => major + minor,
            });
        }
        let tol = chord_tolerance.max(1e-9);
        let r = r_max.max(tol);
        let c = (1.0 - tol / r).clamp(-1.0, 1.0);
        // mindestens 16 Segmente pro Vollkreis, höchstens etwa 6300
        let dtheta = (2.0 * c.acos()).clamp(1e-3, PI / 8.0);
        Self { dtheta }
    }

    /// Anzahl der Segmente für eine Winkelspanne (im Bogenmaß), mindestens 1.
    pub fn segments(&self, span: f64) -> usize {
        ((span.abs() / self.dtheta) - 1e-9).ceil().max(1.0) as usize
    }

    /// Linienzug einer Kante, beginnend und endend exakt an den Eckpunkten der Kante.
    pub fn edge_polyline(&self, solid: &Solid, id: Id<Edge>) -> Vec<DVec3> {
        let e = &solid.edges[id];
        let a = solid.vertices[e.start].point;
        let b = solid.vertices[e.end].point;
        match &e.curve {
            Curve::Line { .. } => vec![a, b],
            curve => {
                let n = self.segments(e.t1 - e.t0);
                let mut pts: Vec<DVec3> = (0..=n)
                    .map(|k| curve.point(e.t0 + (e.t1 - e.t0) * k as f64 / n as f64))
                    .collect();
                pts[0] = a;
                pts[n] = b;
                pts
            }
        }
    }
}
```

`crates/planora-mesh/src/tessellate.rs`:

```rust
use crate::discretize::Discretizer;
use crate::mesh::{EdgeLine, FaceRange, Mesh, TessellationOptions};
use planora_kernel::{Face, Id, Loop, Solid, Surface, UvRect};
use planora_math::{DVec3, Frame};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MeshError {
    #[error("Fläche {0}: Triangulation fehlgeschlagen")]
    Triangulation(usize),
    #[error("Fläche {0}: gekrümmte Fläche ohne Parameterbereich")]
    MissingPatch(usize),
}

/// Zerlegt einen Körper in Dreiecke. Alle Dreiecke sind so orientiert, dass sie nach außen zeigen.
pub fn tessellate(solid: &Solid, options: &TessellationOptions) -> Result<Mesh, MeshError> {
    let disc = Discretizer::new(solid, options.chord_tolerance);
    let polylines: Vec<Vec<DVec3>> = (0..solid.edges.len())
        .map(|i| disc.edge_polyline(solid, Id::from_index(i)))
        .collect();

    let mut mesh = Mesh::default();
    for (fid, face) in solid.faces.iter() {
        let first_index = mesh.indices.len() as u32;
        match &face.surface {
            Surface::Plane { frame } => {
                tessellate_planar(face, frame, &polylines, &mut mesh)
                    .map_err(|_| MeshError::Triangulation(fid.index()))?;
            }
            _ => {
                let patch = face
                    .patch
                    .ok_or(MeshError::MissingPatch(fid.index()))?;
                tessellate_patch(face, &patch, &disc, &mut mesh);
            }
        }
        mesh.face_ranges.push(FaceRange {
            face: fid,
            first_index,
            index_count: mesh.indices.len() as u32 - first_index,
        });
    }
    mesh.edge_lines = polylines
        .into_iter()
        .enumerate()
        .map(|(i, points)| EdgeLine {
            edge: Id::from_index(i),
            points,
        })
        .collect();
    Ok(mesh)
}

fn loop_points(lp: &Loop, polylines: &[Vec<DVec3>]) -> Vec<DVec3> {
    let mut pts = Vec::new();
    for he in &lp.half_edges {
        let poly = &polylines[he.edge.index()];
        if he.forward {
            pts.extend_from_slice(&poly[..poly.len() - 1]);
        } else {
            pts.extend(poly[1..].iter().rev());
        }
    }
    pts
}

fn tessellate_planar(
    face: &Face,
    frame: &Frame,
    polylines: &[Vec<DVec3>],
    mesh: &mut Mesh,
) -> Result<(), ()> {
    let normal = if face.same_sense { frame.z } else { -frame.z };
    let rings: Vec<Vec<DVec3>> = std::iter::once(&face.outer)
        .chain(face.inner.iter())
        .map(|lp| loop_points(lp, polylines))
        .collect();

    let mut flat: Vec<f64> = Vec::new();
    let mut holes: Vec<usize> = Vec::new();
    for (i, ring) in rings.iter().enumerate() {
        if ring.len() < 3 {
            return Err(());
        }
        if i > 0 {
            holes.push(flat.len() / 2);
        }
        for p in ring {
            let local = frame.to_local(*p);
            flat.push(local.x);
            flat.push(local.y);
        }
    }
    let triangles = earcutr::earcut(&flat, &holes, 2).map_err(|_| ())?;

    let base = mesh.positions.len() as u32;
    for p in rings.iter().flatten() {
        mesh.positions.push(*p);
        mesh.normals.push(normal);
    }
    for t in triangles.chunks_exact(3) {
        let (a, mut b, mut c) = (t[0], t[1], t[2]);
        let area2 = (flat[2 * b] - flat[2 * a]) * (flat[2 * c + 1] - flat[2 * a + 1])
            - (flat[2 * c] - flat[2 * a]) * (flat[2 * b + 1] - flat[2 * a + 1]);
        // Gegen den Uhrzeigersinn in der Ebene = Normale +z des Frames.
        if area2 < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        if !face.same_sense {
            std::mem::swap(&mut b, &mut c);
        }
        mesh.indices
            .extend_from_slice(&[base + a as u32, base + b as u32, base + c as u32]);
    }
    Ok(())
}

fn tessellate_patch(face: &Face, patch: &UvRect, disc: &Discretizer, mesh: &mut Mesh) {
    let nu = disc.segments(patch.u1 - patch.u0);
    let nv = match face.surface {
        Surface::Sphere { .. } | Surface::Torus { .. } => disc.segments(patch.v1 - patch.v0),
        _ => 1,
    };
    let sign = if face.same_sense { 1.0 } else { -1.0 };
    let base = mesh.positions.len() as u32;
    for j in 0..=nv {
        let v = patch.v0 + (patch.v1 - patch.v0) * j as f64 / nv as f64;
        for i in 0..=nu {
            let u = patch.u0 + (patch.u1 - patch.u0) * i as f64 / nu as f64;
            mesh.positions.push(face.surface.point(u, v));
            mesh.normals.push(face.surface.normal(u, v) * sign);
        }
    }
    let idx = |i: usize, j: usize| base + (j * (nu + 1) + i) as u32;
    for j in 0..nv {
        for i in 0..nu {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            for tri in [[a, b, c], [a, c, d]] {
                push_triangle(mesh, tri, face.same_sense);
            }
        }
    }
}

/// Fügt ein Dreieck hinzu und lässt entartete Dreiecke (an den Polen) weg.
fn push_triangle(mesh: &mut Mesh, tri: [u32; 3], keep_order: bool) {
    const EPS: f64 = 1e-12;
    let p = [
        mesh.positions[tri[0] as usize],
        mesh.positions[tri[1] as usize],
        mesh.positions[tri[2] as usize],
    ];
    if p[0].distance(p[1]) < EPS || p[1].distance(p[2]) < EPS || p[0].distance(p[2]) < EPS {
        return;
    }
    let t = if keep_order { tri } else { [tri[0], tri[2], tri[1]] };
    mesh.indices.extend_from_slice(&t);
}
```

`crates/planora-mesh/src/mass.rs`:

```rust
use crate::mesh::Mesh;
use planora_math::DVec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProperties {
    /// Vorzeichenbehaftetes Volumen; positiv, wenn alle Dreiecke nach außen zeigen.
    pub volume: f64,
    pub surface_area: f64,
    pub centroid: DVec3,
}

/// Volumen, Oberfläche und Schwerpunkt (Divergenzsatz über die Dreiecke).
pub fn mass_properties(mesh: &Mesh) -> MassProperties {
    let mut volume = 0.0;
    let mut area = 0.0;
    let mut weighted = DVec3::ZERO;
    for t in mesh.indices.chunks_exact(3) {
        let a = mesh.positions[t[0] as usize];
        let b = mesh.positions[t[1] as usize];
        let c = mesh.positions[t[2] as usize];
        let v = a.dot(b.cross(c)) / 6.0;
        volume += v;
        weighted += v * (a + b + c) / 4.0;
        area += 0.5 * (b - a).cross(c - a).length();
    }
    let centroid = if volume.abs() > 1e-12 {
        weighted / volume
    } else {
        DVec3::ZERO
    };
    MassProperties {
        volume,
        surface_area: area,
        centroid,
    }
}
```

`crates/planora-mesh/src/check.rs`:

```rust
use crate::mesh::Mesh;
use std::collections::HashMap;

/// Wahr, wenn das Netz nach dem Verschweißen nahe beieinanderliegender Punkte (Abstand < `eps`)
/// geschlossen und durchgängig orientiert ist: Jede gerichtete Kante kommt genau einmal vor,
/// ebenso ihre Gegenkante. Entartete Dreiecke werden ignoriert.
pub fn is_watertight(mesh: &Mesh, eps: f64) -> bool {
    let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let remap: Vec<u32> = mesh
        .positions
        .iter()
        .map(|p| {
            let key = (
                (p.x / eps).round() as i64,
                (p.y / eps).round() as i64,
                (p.z / eps).round() as i64,
            );
            let next = ids.len() as u32;
            *ids.entry(key).or_insert(next)
        })
        .collect();

    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]);
        if a == b || b == c || a == c {
            continue;
        }
        for edge in [(a, b), (b, c), (c, a)] {
            *directed.entry(edge).or_insert(0) += 1;
        }
    }
    !directed.is_empty()
        && directed
            .iter()
            .all(|(&(p, q), &n)| n == 1 && directed.get(&(q, p)) == Some(&1))
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-mesh`
Expected: PASS. Falls `earcutr::earcut` nicht `Result` zurückgibt oder die Löcher-Indizes anders erwartet (laut Quelle 0.5.0: `Result<Vec<usize>, Error>`, Lochindizes zählen **Punkte**, nicht Koordinaten), nur `tessellate_planar` anpassen. Falls ein Volumentest knapp scheitert, zuerst prüfen, ob die Rechnung (nicht die Toleranz) stimmt.

- [ ] **Step 5: Commit**

```powershell
git add crates/planora-mesh
git commit -m "feat(mesh): Tessellierung, Masseneigenschaften und Geschlossenheitsprüfung" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: `planora-render` – Kamera, Picking, Szenendaten, ViewCube (ohne GPU)

**Files:**
- Create: `crates/planora-render/Cargo.toml`, `src/{lib,camera,pick,scene,viewcube}.rs`
- Test: `crates/planora-render/tests/{camera,pick,scene,viewcube}.rs`

(Die GPU-Datei `gpu.rs` und `shader.wgsl` kommen in Task 9. In dieser Task enthält `lib.rs` nur die vier CPU-Module.)

**Interfaces:**
- Consumes: `planora_mesh::{Mesh, FaceRange, EdgeLine, tessellate, TessellationOptions}`, `planora_kernel::{Id, Face, Edge, AnalyticKernel, Kernel}`, `glam` (f32).
- Produces:
  - `OrbitCamera { target: Vec3, distance, yaw, pitch, fov_y, aspect: f32 }` (`Copy`) mit `Default`, `eye()`, `view()`, `projection()`, `view_proj()`, `orbit(dx,dy)`, `pan(dx,dy,viewport_height)`, `zoom(scroll)`, `set_viewport(w,h)`, `ray(ndc: Vec2) -> (Vec3, Vec3)`, `project(p, viewport: Vec2) -> Option<Vec2>`, `frame_bounds(min,max)`, `light_dir() -> Vec3`, `set_view(ViewFace)` (aus `viewcube`), `MAX_PITCH`
  - `pick_face(&Mesh, origin: Vec3, dir: Vec3) -> Option<FaceHit {face, distance}>`; `pick_edge(&Mesh, &OrbitCamera, viewport: Vec2, cursor: Vec2, max_px: f32, occluder: Option<f32>) -> Option<EdgeHit {edge, pixel_distance}>`
  - `FaceRef { body: usize, face: Id<Face> }`, `EdgeRef { body: usize, edge: Id<Edge> }`, `Pick::{Face(FaceRef), Edge(EdgeRef)}`, `Highlight { hover: Option<Pick>, selected: Option<Pick> }`, `Palette::{dark(), light()}` (Felder `[f32;4]`: `face_base, face_hover, face_selected, edge_base, edge_hover, edge_selected, grid, axis_x, axis_y, axis_z`), `FaceVertex { position:[f32;3], normal:[f32;3], color:[f32;4] }`, `LineVertex { position:[f32;3], color:[f32;4] }` (beide `bytemuck::Pod`), `SceneData { face_vertices, face_indices, line_vertices }`, `build_scene(&[&Mesh], &Highlight, &Palette, grid_half_extent: f32) -> SceneData` (`grid_half_extent == 0.0` → kein Raster/keine Achsen)
  - `ViewFace::{Front, Back, Left, Right, Top, Bottom}`, `ViewFace::view() -> (yaw, pitch)`, `ISO_VIEW: (f32, f32)`, `CubeFace { face: ViewFace, corners: [Vec2; 4], depth: f32 }`, `cube_faces(&OrbitCamera) -> Vec<CubeFace>` (nur sichtbare Seiten, Ecken in Einheitskoordinaten mit y nach oben), `cube_hit_test(&[CubeFace], Vec2) -> Option<ViewFace>`

- [ ] **Step 1: Cargo.toml und Tests schreiben**

`crates/planora-render/Cargo.toml`:

```toml
[package]
name = "planora-render"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
planora-math.workspace = true
planora-kernel.workspace = true
planora-mesh.workspace = true
glam = { workspace = true, features = ["bytemuck"] }
bytemuck.workspace = true
egui.workspace = true
egui-wgpu.workspace = true

[lints]
workspace = true
```

`crates/planora-render/tests/camera.rs`:

```rust
use glam::{Vec2, Vec3};
use planora_render::OrbitCamera;
use std::f32::consts::FRAC_PI_2;

fn cam() -> OrbitCamera {
    OrbitCamera {
        target: Vec3::new(10.0, 20.0, 5.0),
        distance: 100.0,
        yaw: 0.6,
        pitch: 0.4,
        aspect: 1.5,
        ..OrbitCamera::default()
    }
}

#[test]
fn target_projects_to_screen_center() {
    let c = cam();
    let ndc = c.view_proj().project_point3(c.target);
    assert!(ndc.x.abs() < 1e-4 && ndc.y.abs() < 1e-4);
    assert!(ndc.z > 0.0 && ndc.z < 1.0);
}

#[test]
fn center_ray_passes_through_target() {
    let c = cam();
    let (origin, dir) = c.ray(Vec2::ZERO);
    assert!((dir.length() - 1.0).abs() < 1e-4);
    let to_target = c.target - origin;
    let off_axis = to_target - dir * to_target.dot(dir);
    assert!(off_axis.length() < 1e-2, "{off_axis:?}");
}

#[test]
fn project_matches_ray() {
    let c = cam();
    let viewport = Vec2::new(900.0, 600.0);
    let px = c.project(c.target, viewport).unwrap();
    assert!((px - viewport / 2.0).length() < 1e-2, "{px:?}");
}

#[test]
fn points_behind_the_camera_are_not_projected() {
    let c = cam();
    let behind = c.eye() + (c.eye() - c.target);
    assert!(c.project(behind, Vec2::new(800.0, 600.0)).is_none());
}

#[test]
fn orbit_clamps_pitch() {
    let mut c = cam();
    c.orbit(0.0, 1.0e6);
    assert!(c.pitch <= OrbitCamera::MAX_PITCH + 1e-6);
    c.orbit(0.0, -2.0e6);
    assert!(c.pitch >= -OrbitCamera::MAX_PITCH - 1e-6);
    assert!(OrbitCamera::MAX_PITCH < FRAC_PI_2);
}

#[test]
fn zoom_stays_within_bounds() {
    let mut c = cam();
    for _ in 0..1000 {
        c.zoom(500.0);
    }
    assert!(c.distance >= 0.01);
    for _ in 0..1000 {
        c.zoom(-500.0);
    }
    assert!(c.distance <= 1.0e6);
}

#[test]
fn pan_moves_target_parallel_to_the_screen() {
    let mut c = cam();
    let before = c.target;
    c.pan(30.0, -12.0, 600.0);
    let delta = c.target - before;
    assert!(delta.length() > 0.0);
    let view_dir = (c.target - c.eye()).normalize();
    assert!(delta.dot(view_dir).abs() < 1e-3);
}

#[test]
fn zero_sized_viewport_is_ignored() {
    let mut c = cam();
    let aspect = c.aspect;
    c.set_viewport(0.0, 0.0);
    c.set_viewport(100.0, 0.0);
    assert_eq!(c.aspect, aspect);
    c.set_viewport(300.0, 100.0);
    assert!((c.aspect - 3.0).abs() < 1e-6);
    assert!(c.view_proj().to_cols_array().iter().all(|v| v.is_finite()));
}

#[test]
fn frame_bounds_puts_box_in_view() {
    let mut c = OrbitCamera::default();
    c.frame_bounds(Vec3::new(-5.0, -5.0, -5.0), Vec3::new(5.0, 5.0, 5.0));
    assert!(c.target.length() < 1e-5);
    for corner in [
        Vec3::new(-5.0, -5.0, -5.0),
        Vec3::new(5.0, 5.0, 5.0),
        Vec3::new(-5.0, 5.0, -5.0),
    ] {
        let ndc = c.view_proj().project_point3(corner);
        assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0, "{ndc:?}");
    }
}
```

`crates/planora-render/tests/viewcube.rs`:

```rust
use glam::Vec2;
use planora_render::*;

fn visible(cam: &OrbitCamera) -> Vec<ViewFace> {
    let mut v: Vec<_> = cube_faces(cam).iter().map(|f| f.face).collect();
    v.sort_by_key(|f| format!("{f:?}"));
    v
}

#[test]
fn front_view_shows_only_the_front_face() {
    let mut cam = OrbitCamera::default();
    cam.set_view(ViewFace::Front);
    assert_eq!(visible(&cam), vec![ViewFace::Front]);
}

#[test]
fn top_view_shows_only_the_top_face() {
    let mut cam = OrbitCamera::default();
    cam.set_view(ViewFace::Top);
    assert_eq!(visible(&cam), vec![ViewFace::Top]);
}

#[test]
fn isometric_view_shows_three_faces() {
    let mut cam = OrbitCamera::default();
    cam.set_iso_view();
    assert_eq!(
        visible(&cam),
        vec![ViewFace::Front, ViewFace::Right, ViewFace::Top]
    );
}

#[test]
fn clicking_the_center_of_the_front_face_selects_it() {
    let mut cam = OrbitCamera::default();
    cam.set_view(ViewFace::Front);
    let faces = cube_faces(&cam);
    assert_eq!(cube_hit_test(&faces, Vec2::ZERO), Some(ViewFace::Front));
    assert_eq!(cube_hit_test(&faces, Vec2::new(5.0, 5.0)), None);
}

#[test]
fn every_standard_view_looks_at_the_expected_side() {
    // Die Kamera steht auf der Seite der Fläche, die sie ansieht.
    let expectations = [
        (ViewFace::Front, glam::Vec3::NEG_Y),
        (ViewFace::Back, glam::Vec3::Y),
        (ViewFace::Right, glam::Vec3::X),
        (ViewFace::Left, glam::Vec3::NEG_X),
        (ViewFace::Top, glam::Vec3::Z),
        (ViewFace::Bottom, glam::Vec3::NEG_Z),
    ];
    for (face, side) in expectations {
        let mut cam = OrbitCamera::default();
        cam.set_view(face);
        let dir = (cam.eye() - cam.target).normalize();
        assert!(dir.dot(side) > 0.99, "{face:?}: {dir:?}");
    }
}
```

`crates/planora-render/tests/pick.rs`:

```rust
use glam::{Vec2, Vec3};
use planora_kernel::*;
use planora_math::DVec3;
use planora_mesh::*;
use planora_render::*;

fn box_mesh() -> (Solid, Mesh) {
    let solid = AnalyticKernel::default()
        .make_box(DVec3::new(2.0, 2.0, 2.0))
        .unwrap();
    let mesh = tessellate(&solid, &TessellationOptions::default()).unwrap();
    (solid, mesh)
}

#[test]
fn ray_from_above_hits_the_top_face() {
    let (solid, mesh) = box_mesh();
    let hit = pick_face(&mesh, Vec3::new(1.0, 1.0, 10.0), Vec3::new(0.0, 0.0, -1.0)).unwrap();
    assert!((hit.distance - 8.0).abs() < 1e-4);
    let Surface::Plane { frame } = &solid.faces[hit.face].surface else {
        panic!("Quader hat nur ebene Flächen")
    };
    assert!((frame.origin.z - 2.0).abs() < 1e-9);
}

#[test]
fn ray_that_misses_returns_none() {
    let (_, mesh) = box_mesh();
    assert!(pick_face(&mesh, Vec3::new(50.0, 50.0, 10.0), Vec3::new(0.0, 0.0, -1.0)).is_none());
}

#[test]
fn empty_mesh_never_picks() {
    let mesh = Mesh::default();
    assert!(pick_face(&mesh, Vec3::ZERO, Vec3::Z).is_none());
    let cam = OrbitCamera::default();
    assert!(pick_edge(&mesh, &cam, Vec2::new(800.0, 600.0), Vec2::new(400.0, 300.0), 6.0, None).is_none());
}

#[test]
fn cursor_near_an_edge_picks_that_edge() {
    let (_, mesh) = box_mesh();
    let mut cam = OrbitCamera::default();
    cam.frame_bounds(Vec3::ZERO, Vec3::splat(2.0));
    let viewport = Vec2::new(800.0, 600.0);
    cam.set_viewport(viewport.x, viewport.y);
    let line = &mesh.edge_lines[0];
    let a = line.points[0];
    let b = *line.points.last().unwrap();
    let mid = Vec3::new(
        ((a.x + b.x) / 2.0) as f32,
        ((a.y + b.y) / 2.0) as f32,
        ((a.z + b.z) / 2.0) as f32,
    );
    let cursor = cam.project(mid, viewport).unwrap() + Vec2::new(2.0, 1.0);
    let hit = pick_edge(&mesh, &cam, viewport, cursor, 6.0, None).unwrap();
    assert!(hit.pixel_distance <= 3.0);
}

#[test]
fn cursor_far_from_all_edges_picks_nothing() {
    let (_, mesh) = box_mesh();
    let mut cam = OrbitCamera::default();
    cam.frame_bounds(Vec3::ZERO, Vec3::splat(2.0));
    let viewport = Vec2::new(800.0, 600.0);
    cam.set_viewport(viewport.x, viewport.y);
    assert!(pick_edge(&mesh, &cam, viewport, Vec2::new(2.0, 2.0), 6.0, None).is_none());
}

#[test]
fn occluded_edges_are_ignored() {
    let (_, mesh) = box_mesh();
    let mut cam = OrbitCamera::default();
    cam.frame_bounds(Vec3::ZERO, Vec3::splat(2.0));
    let viewport = Vec2::new(800.0, 600.0);
    cam.set_viewport(viewport.x, viewport.y);
    let line = &mesh.edge_lines[0];
    let a = line.points[0];
    let mid = Vec3::new(a.x as f32, a.y as f32, a.z as f32);
    let cursor = cam.project(mid, viewport).unwrap();
    // Eine Fläche ganz nah an der Kamera verdeckt alles dahinter.
    assert!(pick_edge(&mesh, &cam, viewport, cursor, 6.0, Some(0.1)).is_none());
}
```

`crates/planora-render/tests/scene.rs`:

```rust
use planora_kernel::*;
use planora_math::DVec3;
use planora_mesh::*;
use planora_render::*;

fn box_mesh() -> Mesh {
    let solid = AnalyticKernel::default()
        .make_box(DVec3::new(2.0, 3.0, 4.0))
        .unwrap();
    tessellate(&solid, &TessellationOptions::default()).unwrap()
}

#[test]
fn scene_copies_all_vertices_and_indices() {
    let mesh = box_mesh();
    let scene = build_scene(&[&mesh], &Highlight::default(), &Palette::dark(), 0.0);
    assert_eq!(scene.face_vertices.len(), mesh.positions.len());
    assert_eq!(scene.face_indices, mesh.indices);
    let expected_line_vertices: usize = mesh
        .edge_lines
        .iter()
        .map(|l| (l.points.len() - 1) * 2)
        .sum();
    assert_eq!(scene.line_vertices.len(), expected_line_vertices);
}

#[test]
fn selected_face_gets_selected_color_and_others_stay_base() {
    let mesh = box_mesh();
    let palette = Palette::dark();
    let face = mesh.face_ranges[0].face;
    let hl = Highlight {
        hover: None,
        selected: Some(Pick::Face(FaceRef { body: 0, face })),
    };
    let scene = build_scene(&[&mesh], &hl, &palette, 0.0);
    let color_of = |range: &FaceRange| {
        let i = mesh.indices[range.first_index as usize] as usize;
        scene.face_vertices[i].color
    };
    assert_eq!(color_of(&mesh.face_ranges[0]), palette.face_selected);
    assert_eq!(color_of(&mesh.face_ranges[1]), palette.face_base);
}

#[test]
fn hovered_edge_gets_hover_color() {
    let mesh = box_mesh();
    let palette = Palette::dark();
    let edge = mesh.edge_lines[0].edge;
    let hl = Highlight {
        hover: Some(Pick::Edge(EdgeRef { body: 0, edge })),
        selected: None,
    };
    let scene = build_scene(&[&mesh], &hl, &palette, 0.0);
    assert_eq!(scene.line_vertices[0].color, palette.edge_hover);
    let other = (mesh.edge_lines[0].points.len() - 1) * 2;
    assert_eq!(scene.line_vertices[other].color, palette.edge_base);
}

#[test]
fn highlight_only_applies_to_its_own_body() {
    let mesh = box_mesh();
    let palette = Palette::dark();
    let face = mesh.face_ranges[0].face;
    let hl = Highlight {
        hover: None,
        selected: Some(Pick::Face(FaceRef { body: 1, face })),
    };
    let scene = build_scene(&[&mesh, &mesh], &hl, &palette, 0.0);
    let n = mesh.positions.len();
    let first = mesh.indices[mesh.face_ranges[0].first_index as usize] as usize;
    assert_eq!(scene.face_vertices[first].color, palette.face_base);
    assert_eq!(scene.face_vertices[n + first].color, palette.face_selected);
    // Indizes des zweiten Körpers sind um die Eckenzahl des ersten verschoben.
    assert_eq!(scene.face_indices.len(), mesh.indices.len() * 2);
    assert_eq!(scene.face_indices[mesh.indices.len()], mesh.indices[0] + n as u32);
}

#[test]
fn grid_adds_lines_and_empty_scene_is_fine() {
    let empty = build_scene(&[], &Highlight::default(), &Palette::dark(), 100.0);
    assert!(empty.face_vertices.is_empty());
    assert!(!empty.line_vertices.is_empty());
    assert_eq!(empty.line_vertices.len() % 2, 0);
    let none = build_scene(&[], &Highlight::default(), &Palette::dark(), 0.0);
    assert!(none.line_vertices.is_empty());
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-render`
Expected: FAIL (Kompilierfehler, Module fehlen).

- [ ] **Step 3: Implementieren**

`crates/planora-render/src/lib.rs`:

```rust
//! Darstellung und Auswahl: Kamera, Picking, Szenendaten. (Die GPU-Anbindung folgt in `gpu`.)

mod camera;
mod pick;
mod scene;
mod viewcube;

pub use camera::OrbitCamera;
pub use pick::{EdgeHit, FaceHit, pick_edge, pick_face};
pub use scene::{
    EdgeRef, FaceRef, FaceVertex, Highlight, LineVertex, Palette, Pick, SceneData, build_scene,
};
pub use viewcube::{CubeFace, ViewFace, cube_faces, cube_hit_test};
```

`crates/planora-render/src/camera.rs`:

```rust
use crate::viewcube::ViewFace;
use glam::{Mat4, Vec2, Vec3};

/// Umlaufkamera um einen Zielpunkt. Z zeigt nach oben.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    /// Drehung um die z-Achse (0 = Kamera steht auf der +x-Seite).
    pub yaw: f32,
    /// Höhenwinkel (0 = waagerecht, positiv = von oben).
    pub pitch: f32,
    pub fov_y: f32,
    pub aspect: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 150.0,
            yaw: -0.8,
            pitch: 0.5,
            fov_y: 45.0_f32.to_radians(),
            aspect: 1.0,
        }
    }
}

impl OrbitCamera {
    pub const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.01;

    pub fn eye(&self) -> Vec3 {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        self.target + self.distance * Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Z)
    }

    fn near_far(&self) -> (f32, f32) {
        ((self.distance * 0.01).max(0.001), self.distance * 100.0 + 10_000.0)
    }

    pub fn projection(&self) -> Mat4 {
        let (near, far) = self.near_far();
        Mat4::perspective_rh(self.fov_y, self.aspect, near, far)
    }

    pub fn view_proj(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Setzt das Seitenverhältnis. Zu kleine (oder nicht endliche) Größen werden ignoriert.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        if width >= 1.0 && height >= 1.0 && width.is_finite() && height.is_finite() {
            self.aspect = width / height;
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.01;
        self.pitch = (self.pitch + dy * 0.01).clamp(-Self::MAX_PITCH, Self::MAX_PITCH);
    }

    /// Verschiebt das Ziel parallel zum Bildschirm; `dx`, `dy` in Pixeln.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let height = viewport_height.max(1.0);
        let scale = 2.0 * self.distance * (self.fov_y * 0.5).tan() / height;
        let inv = self.view().inverse();
        let right = inv.x_axis.truncate();
        let up = inv.y_axis.truncate();
        self.target += (-right * dx + up * dy) * scale;
    }

    /// `scroll` in Punkten; positiv = hineinzoomen.
    pub fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (-scroll * 0.002).exp()).clamp(0.01, 1.0e6);
    }

    /// Strahl durch einen Punkt in normierten Gerätekoordinaten (x, y ∈ [-1, 1], y nach oben).
    pub fn ray(&self, ndc: Vec2) -> (Vec3, Vec3) {
        let inv = self.view_proj().inverse();
        let near = inv.project_point3(Vec3::new(ndc.x, ndc.y, 0.0));
        let far = inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
        (near, (far - near).normalize())
    }

    /// Bildschirmposition in Pixeln (Ursprung oben links) oder `None` hinter der Kamera.
    pub fn project(&self, p: Vec3, viewport: Vec2) -> Option<Vec2> {
        let clip = self.view_proj() * p.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(Vec2::new(
            (ndc.x * 0.5 + 0.5) * viewport.x,
            (0.5 - ndc.y * 0.5) * viewport.y,
        ))
    }

    /// Richtet die Kamera so aus, dass der Quader `min..max` ins Bild passt.
    pub fn frame_bounds(&mut self, min: Vec3, max: Vec3) {
        self.target = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(1e-3);
        let half = (self.fov_y * 0.5).min((self.fov_y * self.aspect).atan() * 0.5).max(0.05);
        self.distance = (radius / half.sin() * 1.15).clamp(0.01, 1.0e6);
    }

    /// Richtung zur Lichtquelle (leicht versetzt vom Betrachter, damit Flächen Tiefe bekommen).
    pub fn light_dir(&self) -> Vec3 {
        let inv = self.view().inverse();
        (inv.z_axis.truncate() + inv.y_axis.truncate() * 0.5 + inv.x_axis.truncate() * 0.3)
            .normalize()
    }

    /// Standardansicht von der gegebenen Seite (Blickrichtung auf das Ziel bleibt erhalten).
    pub fn set_view(&mut self, face: ViewFace) {
        let (yaw, pitch) = face.view();
        self.yaw = yaw;
        self.pitch = pitch;
    }

    pub fn set_iso_view(&mut self) {
        self.yaw = -std::f32::consts::FRAC_PI_4;
        self.pitch = (1.0_f32 / 2.0_f32.sqrt()).atan();
    }
}
```

Hinweis zu `frame_bounds`: Die Formel soll einfach eine Distanz liefern, bei der die Bounding-Kugel sicher ins Bild passt. Der Test `frame_bounds_puts_box_in_view` prüft drei Ecken; falls er an der Formel scheitert, `half` durch `(self.fov_y * 0.5)` ersetzen und mit `radius / half.sin() * 1.3` rechnen – der Test ist verbindlich, die Formel nicht.

`crates/planora-render/src/viewcube.rs`:

```rust
use crate::camera::OrbitCamera;
use glam::{Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewFace {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

impl ViewFace {
    pub const ALL: [ViewFace; 6] = [
        ViewFace::Front,
        ViewFace::Back,
        ViewFace::Left,
        ViewFace::Right,
        ViewFace::Top,
        ViewFace::Bottom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewFace::Front => "Vorne",
            ViewFace::Back => "Hinten",
            ViewFace::Left => "Links",
            ViewFace::Right => "Rechts",
            ViewFace::Top => "Oben",
            ViewFace::Bottom => "Unten",
        }
    }

    /// Außennormale der Würfelseite (Vorne = -y, Rechts = +x, Oben = +z).
    fn normal(self) -> Vec3 {
        match self {
            ViewFace::Front => Vec3::NEG_Y,
            ViewFace::Back => Vec3::Y,
            ViewFace::Left => Vec3::NEG_X,
            ViewFace::Right => Vec3::X,
            ViewFace::Top => Vec3::Z,
            ViewFace::Bottom => Vec3::NEG_Z,
        }
    }

    /// Zwei Tangenten, sodass `t1 × t2 = normal` (Ecken laufen gegen den Uhrzeigersinn von außen).
    fn tangents(self) -> (Vec3, Vec3) {
        match self {
            ViewFace::Right => (Vec3::Y, Vec3::Z),
            ViewFace::Left => (Vec3::Z, Vec3::Y),
            ViewFace::Front => (Vec3::X, Vec3::Z),
            ViewFace::Back => (Vec3::Z, Vec3::X),
            ViewFace::Top => (Vec3::X, Vec3::Y),
            ViewFace::Bottom => (Vec3::Y, Vec3::X),
        }
    }

    /// `(yaw, pitch)` einer Kamera, die diese Seite frontal ansieht.
    pub fn view(self) -> (f32, f32) {
        use std::f32::consts::{FRAC_PI_2, PI};
        match self {
            ViewFace::Front => (-FRAC_PI_2, 0.0),
            ViewFace::Back => (FRAC_PI_2, 0.0),
            ViewFace::Right => (0.0, 0.0),
            ViewFace::Left => (PI, 0.0),
            ViewFace::Top => (-FRAC_PI_2, OrbitCamera::MAX_PITCH),
            ViewFace::Bottom => (-FRAC_PI_2, -OrbitCamera::MAX_PITCH),
        }
    }
}

/// Sichtbare Seite des Navigationswürfels in Einheitskoordinaten (x rechts, y oben, Würfel ±1).
#[derive(Clone, Debug)]
pub struct CubeFace {
    pub face: ViewFace,
    pub corners: [Vec2; 4],
    /// Größer = näher an der Kamera.
    pub depth: f32,
}

/// Projiziert den Würfel orthografisch mit der Drehung der Kamera und liefert die sichtbaren
/// Seiten, die am weitesten hinten liegende zuerst.
pub fn cube_faces(camera: &OrbitCamera) -> Vec<CubeFace> {
    let view = camera.view();
    let to_view = |v: Vec3| view.transform_vector3(v);
    let mut faces: Vec<CubeFace> = ViewFace::ALL
        .iter()
        .filter_map(|&face| {
            let n = face.normal();
            let facing = to_view(n).z;
            if facing <= 1e-4 {
                return None;
            }
            let (t1, t2) = face.tangents();
            let corner = |a: f32, b: f32| {
                let p = to_view(n + t1 * a + t2 * b);
                Vec2::new(p.x, p.y)
            };
            Some(CubeFace {
                face,
                corners: [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)],
                depth: facing,
            })
        })
        .collect();
    faces.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    faces
}

fn point_in_quad(corners: &[Vec2; 4], p: Vec2) -> bool {
    let mut sign = 0.0_f32;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b - a).perp_dot(p - a);
        if cross.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// Welche (vorderste) Seite liegt unter dem Punkt `p` (Einheitskoordinaten)?
pub fn cube_hit_test(faces: &[CubeFace], p: Vec2) -> Option<ViewFace> {
    faces
        .iter()
        .rev()
        .find(|f| point_in_quad(&f.corners, p))
        .map(|f| f.face)
}
```

Hinweis: In `tests/viewcube.rs` wird `ViewFace::Top` mit `visible == [Top]` geprüft. Bei `pitch = MAX_PITCH` (0,01 rad vor der Senkrechten) ist `facing` der Vorderseite ≈ 0,01·… und könnte über der Schwelle 1e-4 liegen. Falls der Test dadurch zwei Seiten findet, die Schwelle in `cube_faces` auf `0.02` erhöhen.

`crates/planora-render/src/pick.rs`:

```rust
use crate::camera::OrbitCamera;
use glam::{Vec2, Vec3};
use planora_kernel::{Edge, Face, Id};
use planora_math::DVec3;
use planora_mesh::Mesh;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceHit {
    pub face: Id<Face>,
    pub distance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeHit {
    pub edge: Id<Edge>,
    pub pixel_distance: f32,
}

fn to_vec3(p: DVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
}

/// Möller–Trumbore, beidseitig. Liefert die Strahllänge bis zum Treffer.
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let t_vec = o - a;
    let u = t_vec.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t_vec.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-6).then_some(t)
}

/// Nächstgelegene Fläche, die der Strahl trifft.
pub fn pick_face(mesh: &Mesh, origin: Vec3, dir: Vec3) -> Option<FaceHit> {
    let mut best: Option<FaceHit> = None;
    for range in &mesh.face_ranges {
        let start = range.first_index as usize;
        let end = start + range.index_count as usize;
        for tri in mesh.indices[start..end].chunks_exact(3) {
            let a = to_vec3(mesh.positions[tri[0] as usize]);
            let b = to_vec3(mesh.positions[tri[1] as usize]);
            let c = to_vec3(mesh.positions[tri[2] as usize]);
            if let Some(t) = ray_triangle(origin, dir, a, b, c) {
                if best.is_none_or(|h| t < h.distance) {
                    best = Some(FaceHit {
                        face: range.face,
                        distance: t,
                    });
                }
            }
        }
    }
    best
}

fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> (Vec2, f32) {
    let ab = b - a;
    let len2 = ab.length_squared();
    let s = if len2 > 1e-12 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * s, s)
}

/// Kante, die der Mausposition (in Pixeln, Ursprung oben links) am nächsten liegt, höchstens `max_px` entfernt.
/// `occluder` ist die Entfernung der vordersten getroffenen Fläche; weiter entfernte Kanten gelten als verdeckt.
pub fn pick_edge(
    mesh: &Mesh,
    camera: &OrbitCamera,
    viewport: Vec2,
    cursor: Vec2,
    max_px: f32,
    occluder: Option<f32>,
) -> Option<EdgeHit> {
    let eye = camera.eye();
    let mut best: Option<EdgeHit> = None;
    for line in &mesh.edge_lines {
        for seg in line.points.windows(2) {
            let (wa, wb) = (to_vec3(seg[0]), to_vec3(seg[1]));
            let (Some(a), Some(b)) = (camera.project(wa, viewport), camera.project(wb, viewport))
            else {
                continue;
            };
            let (closest, s) = closest_on_segment(cursor, a, b);
            let dist = (cursor - closest).length();
            if dist > max_px || best.is_some_and(|h| dist >= h.pixel_distance) {
                continue;
            }
            if let Some(od) = occluder {
                let world = wa + (wb - wa) * s;
                if (world - eye).length() > od + 0.01 * od + 0.05 {
                    continue;
                }
            }
            best = Some(EdgeHit {
                edge: line.edge,
                pixel_distance: dist,
            });
        }
    }
    best
}
```

`crates/planora-render/src/scene.rs`:

```rust
use bytemuck::{Pod, Zeroable};
use planora_kernel::{Edge, Face, Id};
use planora_mesh::Mesh;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FaceRef {
    pub body: usize,
    pub face: Id<Face>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EdgeRef {
    pub body: usize,
    pub edge: Id<Edge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pick {
    Face(FaceRef),
    Edge(EdgeRef),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Highlight {
    pub hover: Option<Pick>,
    pub selected: Option<Pick>,
}

/// Farben in sRGB-Werten (0..1); egui zeichnet ohne zusätzliche Gamma-Umrechnung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub face_base: [f32; 4],
    pub face_hover: [f32; 4],
    pub face_selected: [f32; 4],
    pub edge_base: [f32; 4],
    pub edge_hover: [f32; 4],
    pub edge_selected: [f32; 4],
    pub grid: [f32; 4],
    pub axis_x: [f32; 4],
    pub axis_y: [f32; 4],
    pub axis_z: [f32; 4],
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            face_base: [0.62, 0.68, 0.78, 1.0],
            face_hover: [0.45, 0.72, 1.0, 1.0],
            face_selected: [1.0, 0.62, 0.20, 1.0],
            edge_base: [0.06, 0.07, 0.09, 1.0],
            edge_hover: [0.30, 0.80, 1.0, 1.0],
            edge_selected: [1.0, 0.55, 0.10, 1.0],
            grid: [0.24, 0.26, 0.30, 1.0],
            axis_x: [0.85, 0.28, 0.28, 1.0],
            axis_y: [0.35, 0.75, 0.35, 1.0],
            axis_z: [0.30, 0.50, 0.95, 1.0],
        }
    }

    pub fn light() -> Self {
        Self {
            face_base: [0.70, 0.76, 0.86, 1.0],
            face_hover: [0.40, 0.65, 0.95, 1.0],
            face_selected: [0.95, 0.55, 0.15, 1.0],
            edge_base: [0.12, 0.13, 0.16, 1.0],
            edge_hover: [0.10, 0.50, 0.85, 1.0],
            edge_selected: [0.90, 0.45, 0.05, 1.0],
            grid: [0.78, 0.80, 0.84, 1.0],
            axis_x: [0.80, 0.20, 0.20, 1.0],
            axis_y: [0.25, 0.65, 0.25, 1.0],
            axis_z: [0.20, 0.40, 0.90, 1.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FaceVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LineVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

/// Fertig aufbereitete Daten für die Grafikkarte.
#[derive(Clone, Debug, Default)]
pub struct SceneData {
    pub face_vertices: Vec<FaceVertex>,
    pub face_indices: Vec<u32>,
    /// Linienliste: je zwei aufeinanderfolgende Einträge bilden eine Linie.
    pub line_vertices: Vec<LineVertex>,
}

fn p3(p: planora_math::DVec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

/// Baut die Szene aus allen Körpern. `grid_half_extent == 0.0` lässt Raster und Achsen weg.
pub fn build_scene(
    meshes: &[&Mesh],
    highlight: &Highlight,
    palette: &Palette,
    grid_half_extent: f32,
) -> SceneData {
    let mut scene = SceneData::default();

    for (body, mesh) in meshes.iter().enumerate() {
        let vertex_base = scene.face_vertices.len() as u32;
        scene
            .face_vertices
            .extend(mesh.positions.iter().zip(mesh.normals.iter()).map(|(p, n)| FaceVertex {
                position: p3(*p),
                normal: p3(*n),
                color: palette.face_base,
            }));
        for range in &mesh.face_ranges {
            let this = Pick::Face(FaceRef {
                body,
                face: range.face,
            });
            let color = if highlight.selected == Some(this) {
                palette.face_selected
            } else if highlight.hover == Some(this) {
                palette.face_hover
            } else {
                continue;
            };
            let start = range.first_index as usize;
            let end = start + range.index_count as usize;
            for &i in &mesh.indices[start..end] {
                scene.face_vertices[(vertex_base + i) as usize].color = color;
            }
        }
        scene
            .face_indices
            .extend(mesh.indices.iter().map(|i| i + vertex_base));

        for line in &mesh.edge_lines {
            let this = Pick::Edge(EdgeRef {
                body,
                edge: line.edge,
            });
            let color = if highlight.selected == Some(this) {
                palette.edge_selected
            } else if highlight.hover == Some(this) {
                palette.edge_hover
            } else {
                palette.edge_base
            };
            for seg in line.points.windows(2) {
                for p in seg {
                    scene.line_vertices.push(LineVertex {
                        position: p3(*p),
                        color,
                    });
                }
            }
        }
    }

    if grid_half_extent > 0.0 {
        add_grid(&mut scene, palette, grid_half_extent);
    }
    scene
}

fn add_grid(scene: &mut SceneData, palette: &Palette, half: f32) {
    let step = 10.0_f32;
    let n = (half / step).floor() as i32;
    let line = |scene: &mut SceneData, a: [f32; 3], b: [f32; 3], color: [f32; 4]| {
        scene.line_vertices.push(LineVertex { position: a, color });
        scene.line_vertices.push(LineVertex { position: b, color });
    };
    for i in -n..=n {
        if i == 0 {
            continue;
        }
        let t = i as f32 * step;
        line(scene, [-half, t, 0.0], [half, t, 0.0], palette.grid);
        line(scene, [t, -half, 0.0], [t, half, 0.0], palette.grid);
    }
    line(scene, [-half, 0.0, 0.0], [half, 0.0, 0.0], palette.axis_x);
    line(scene, [0.0, -half, 0.0], [0.0, half, 0.0], palette.axis_y);
    line(scene, [0.0, 0.0, 0.0], [0.0, 0.0, half], palette.axis_z);
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-render`
Expected: PASS (Kamera, ViewCube, Picking, Szene). Die Hinweise an `frame_bounds` und `cube_faces` (Schwellen/Formeln) gelten nur, falls ein Test knapp scheitert.

- [ ] **Step 5: Commit**

```powershell
git add crates/planora-render
git commit -m "feat(render): Kamera, Picking, Szenendaten und Navigationswürfel" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: `planora-render` – GPU-Anbindung (wgpu, egui-Paint-Callback)

**Files:**
- Create: `crates/planora-render/src/gpu.rs`, `crates/planora-render/src/shader.wgsl`
- Modify: `crates/planora-render/src/lib.rs`

**Interfaces:**
- Consumes: `SceneData`, `FaceVertex`, `LineVertex` (Task 8); `egui_wgpu::{self, wgpu}`.
- Produces: `gpu::{GpuScene, ViewportCallback, MSAA_SAMPLES: u32, DEPTH_FORMAT}`; `GpuScene::install(&egui_wgpu::RenderState)` legt die GPU-Ressourcen in den Callback-Ressourcen ab; `ViewportCallback { view_proj: [[f32;4];4], light_dir: [f32;3], scene: Arc<SceneData>, scene_version: u64 }` implementiert `egui_wgpu::CallbackTrait`.

Diese Datei lässt sich nicht automatisiert testen (braucht eine GPU und ein Fenster). Verifiziert wird per Compilerlauf jetzt und per Sichtprüfung in Task 10. Die API-Formen stammen aus dem offiziellen egui-0.36.2-Beispiel `custom3d_wgpu.rs`.

- [ ] **Step 1: Shader schreiben** – `crates/planora-render/src/shader.wgsl`

```wgsl
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct FaceOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_face(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
) -> FaceOut {
    var out: FaceOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.color = color;
    return out;
}

@fragment
fn fs_face(input: FaceOut) -> @location(0) vec4<f32> {
    let n = normalize(input.normal);
    let light = normalize(u.light_dir.xyz);
    let diffuse = abs(dot(n, light));
    let intensity = 0.30 + 0.70 * diffuse;
    return vec4<f32>(input.color.rgb * intensity, input.color.a);
}

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
) -> LineOut {
    var out: LineOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_line(input: LineOut) -> @location(0) vec4<f32> {
    return input.color;
}
```

- [ ] **Step 2: `gpu.rs` schreiben**

```rust
use crate::scene::{FaceVertex, LineVertex, SceneData};
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::util::DeviceExt as _;
use egui_wgpu::{self, wgpu};
use std::sync::Arc;

/// Muss zu `NativeOptions::multisampling` und `NativeOptions::depth_buffer` passen.
pub const MSAA_SAMPLES: u32 = 4;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
}

/// GPU-Ressourcen des Viewports. Liegen in `egui_wgpu::CallbackResources`.
pub struct GpuScene {
    face_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    face_vertex_buffer: Option<wgpu::Buffer>,
    face_index_buffer: Option<wgpu::Buffer>,
    face_index_count: u32,
    line_vertex_buffer: Option<wgpu::Buffer>,
    line_vertex_count: u32,
    uploaded_version: Option<u64>,
}

impl GpuScene {
    /// Legt die Pipelines an und hinterlegt sie in den Callback-Ressourcen von egui.
    pub fn install(render_state: &egui_wgpu::RenderState) {
        let gpu = Self::new(&render_state.device, render_state.target_format);
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(gpu);
    }

    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("planora-viewport"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("planora-viewport"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("planora-viewport"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("planora-uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                view_proj: [[0.0; 4]; 4],
                light_dir: [0.0, 0.0, 1.0, 0.0],
            }),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("planora-viewport"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let multisample = wgpu::MultisampleState {
            count: MSAA_SAMPLES,
            ..Default::default()
        };

        let face_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("planora-faces"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_face"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<FaceVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x3,
                        2 => Float32x4
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_face"),
                targets: &[Some(target_format.into())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                // Flächen leicht nach hinten schieben, damit Kanten darüber sauber erscheinen.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample,
            multiview_mask: None,
            cache: None,
        });

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("planora-lines"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_line"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                targets: &[Some(target_format.into())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample,
            multiview_mask: None,
            cache: None,
        });

        Self {
            face_pipeline,
            line_pipeline,
            uniform_buffer,
            bind_group,
            face_vertex_buffer: None,
            face_index_buffer: None,
            face_index_count: 0,
            line_vertex_buffer: None,
            line_vertex_count: 0,
            uploaded_version: None,
        }
    }

    fn upload(&mut self, device: &wgpu::Device, scene: &SceneData) {
        let buffer = |label: &str, bytes: &[u8], usage| {
            (!bytes.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage,
                })
            })
        };
        self.face_vertex_buffer = buffer(
            "planora-face-vertices",
            bytemuck::cast_slice(&scene.face_vertices),
            wgpu::BufferUsages::VERTEX,
        );
        self.face_index_buffer = buffer(
            "planora-face-indices",
            bytemuck::cast_slice(&scene.face_indices),
            wgpu::BufferUsages::INDEX,
        );
        self.face_index_count = scene.face_indices.len() as u32;
        self.line_vertex_buffer = buffer(
            "planora-line-vertices",
            bytemuck::cast_slice(&scene.line_vertices),
            wgpu::BufferUsages::VERTEX,
        );
        self.line_vertex_count = scene.line_vertices.len() as u32;
    }

    fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, cb: &ViewportCallback) {
        let uniforms = Uniforms {
            view_proj: cb.view_proj,
            light_dir: [cb.light_dir[0], cb.light_dir[1], cb.light_dir[2], 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        if self.uploaded_version != Some(cb.scene_version) {
            self.upload(device, &cb.scene);
            self.uploaded_version = Some(cb.scene_version);
        }
    }

    fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        if let (Some(vb), Some(ib)) = (&self.face_vertex_buffer, &self.face_index_buffer) {
            render_pass.set_pipeline(&self.face_pipeline);
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.face_index_count, 0, 0..1);
        }
        if let Some(lvb) = &self.line_vertex_buffer {
            render_pass.set_pipeline(&self.line_pipeline);
            render_pass.set_vertex_buffer(0, lvb.slice(..));
            render_pass.draw(0..self.line_vertex_count, 0..1);
        }
    }
}

/// Wird pro Frame mit `egui_wgpu::Callback::new_paint_callback(rect, …)` in egui eingehängt.
pub struct ViewportCallback {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub scene: Arc<SceneData>,
    /// Erhöht sich, wenn sich `scene` geändert hat; steuert das Neuladen der Puffer.
    pub scene_version: u64,
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(gpu) = resources.get_mut::<GpuScene>() {
            gpu.update(device, queue, self);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(gpu) = resources.get::<GpuScene>() {
            gpu.paint(render_pass);
        }
    }
}
```

`crates/planora-render/src/lib.rs` ergänzen:

```rust
pub mod gpu;
```

- [ ] **Step 3: Kompilieren und Lint prüfen**

Run:

```powershell
cargo build -p planora-render
cargo clippy -p planora-render --all-targets -- -D warnings
cargo test -p planora-render
```

Expected: Build und Clippy ohne Fehler, Tests aus Task 8 weiterhin PASS. Bei Kompilierfehlern in `gpu.rs`: gegen `crates/egui_demo_app/src/apps/custom3d_wgpu.rs` (egui 0.36.2) und docs.rs von `wgpu` 30 abgleichen.

- [ ] **Step 4: Commit**

```powershell
git add crates/planora-render
git commit -m "feat(render): wgpu-Pipelines und egui-Paint-Callback" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: `planora-app` – Zustand, Beispielprofile, Oberfläche, Abnahme

**Files:**
- Create: `crates/planora-app/Cargo.toml`, `src/{lib,main,state,samples,theme,icons,create_dialog,browser,properties,viewport,app}.rs`
- Test: `crates/planora-app/tests/{state,samples}.rs`

**Interfaces:**
- Consumes: alle früheren Crates.
- Produces:
  - `samples::{ExtrudeSample, RevolveSample}` mit `ALL`, `label()`, `profile()` bzw. `chain()`
  - `state::{AppState, Body, BodyKind}`; `AppState::new()`, `add_body(kind, name, solid) -> Result<usize, String>`, `remove_body(index)`, `rebuild_scene()`, `frame_all()`, `set_hover(Option<Pick>)`, `set_selection(Option<Pick>)`, `pick(cursor: Vec2, viewport: Vec2) -> Option<Pick>`; öffentliche Felder `bodies`, `camera`, `scene`, `scene_version`, `hover`, `selection`, `dark`, `kernel`, `chord_tolerance`
  - Programm `planora-app` (Binärdatei) mit Dock-Layout, Menüs, Theme, Viewport, Browser, Eigenschaften, Navigationswürfel

- [ ] **Step 1: Cargo.toml und Tests schreiben**

`crates/planora-app/Cargo.toml`:

```toml
[package]
name = "planora-app"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
name = "planora_app"
path = "src/lib.rs"

[[bin]]
name = "planora-app"
path = "src/main.rs"

[dependencies]
planora-math.workspace = true
planora-kernel.workspace = true
planora-mesh.workspace = true
planora-render.workspace = true
glam.workspace = true
eframe.workspace = true
egui.workspace = true
egui_dock.workspace = true

[lints]
workspace = true
```

`crates/planora-app/tests/samples.rs`:

```rust
use planora_app::samples::*;
use planora_kernel::*;
use planora_math::*;

#[test]
fn every_extrude_sample_builds_a_valid_solid() {
    let k = AnalyticKernel::default();
    for sample in ExtrudeSample::ALL {
        let solid = k
            .extrude(&sample.profile(), &Frame::WORLD, sample.default_height())
            .unwrap_or_else(|e| panic!("{}: {e}", sample.label()));
        validate(&solid).unwrap_or_else(|e| panic!("{}: {e}", sample.label()));
    }
}

#[test]
fn every_revolve_sample_builds_a_valid_solid() {
    let k = AnalyticKernel::default();
    for sample in RevolveSample::ALL {
        let solid = k
            .revolve(&sample.chain(), &Frame::WORLD)
            .unwrap_or_else(|e| panic!("{}: {e}", sample.label()));
        validate(&solid).unwrap_or_else(|e| panic!("{}: {e}", sample.label()));
    }
}

#[test]
fn sample_labels_are_unique_and_not_empty() {
    let mut labels: Vec<_> = ExtrudeSample::ALL.iter().map(|s| s.label()).collect();
    labels.extend(RevolveSample::ALL.iter().map(|s| s.label()));
    assert!(labels.iter().all(|l| !l.is_empty()));
    let mut sorted = labels.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), labels.len());
}
```

`crates/planora-app/tests/state.rs`:

```rust
use glam::Vec2;
use planora_app::state::*;
use planora_kernel::*;
use planora_math::DVec3;
use planora_render::*;

fn boxed(size: f64) -> Solid {
    AnalyticKernel::default()
        .make_box(DVec3::new(size, size, size))
        .unwrap()
}

#[test]
fn new_state_is_empty_and_framing_it_does_not_panic() {
    let mut s = AppState::new();
    assert!(s.bodies.is_empty());
    s.frame_all();
    assert!(s.camera.view_proj().to_cols_array().iter().all(|v| v.is_finite()));
    assert!(s.pick(Vec2::new(100.0, 100.0), Vec2::new(800.0, 600.0)).is_none());
    assert!(!s.scene.line_vertices.is_empty(), "das Raster ist auch ohne Körper sichtbar");
    assert!(s.scene.face_vertices.is_empty());
}

#[test]
fn bodies_are_placed_next_to_each_other() {
    let mut s = AppState::new();
    let a = s.add_body(BodyKind::Box, "A".into(), boxed(10.0)).unwrap();
    let b = s.add_body(BodyKind::Box, "B".into(), boxed(10.0)).unwrap();
    assert_eq!((a, b), (0, 1));
    assert!(s.bodies[1].bounds.0.x > s.bodies[0].bounds.1.x, "Körper überlappen sich");
    assert_eq!(
        s.scene.face_vertices.len(),
        s.bodies[0].mesh.positions.len() + s.bodies[1].mesh.positions.len()
    );
}

#[test]
fn adding_a_body_fills_mass_properties_and_bumps_the_scene_version() {
    let mut s = AppState::new();
    let v0 = s.scene_version;
    s.add_body(BodyKind::Box, "Würfel".into(), boxed(10.0)).unwrap();
    assert!(s.scene_version > v0);
    assert!((s.bodies[0].mass.volume - 1000.0).abs() < 1e-6);
}

#[test]
fn invalid_solids_are_rejected_with_a_message() {
    let mut s = AppState::new();
    let mut bad = boxed(1.0);
    let id = Id::<Face>::from_index(0);
    bad.faces[id].outer.half_edges[0].forward = !bad.faces[id].outer.half_edges[0].forward;
    let err = s.add_body(BodyKind::Box, "kaputt".into(), bad).unwrap_err();
    assert!(!err.is_empty());
    assert!(s.bodies.is_empty());
}

#[test]
fn removing_a_body_clears_references_to_it() {
    let mut s = AppState::new();
    s.add_body(BodyKind::Box, "A".into(), boxed(10.0)).unwrap();
    s.add_body(BodyKind::Box, "B".into(), boxed(10.0)).unwrap();
    let face = s.bodies[1].mesh.face_ranges[0].face;
    s.set_selection(Some(Pick::Face(FaceRef { body: 1, face })));
    s.remove_body(1);
    assert_eq!(s.bodies.len(), 1);
    assert!(s.selection.is_none());
    s.remove_body(5); // ungültiger Index → ignoriert
    assert_eq!(s.bodies.len(), 1);
}

#[test]
fn picking_hits_a_face_in_the_middle_of_the_screen() {
    let mut s = AppState::new();
    s.add_body(BodyKind::Box, "A".into(), boxed(10.0)).unwrap();
    s.camera.set_viewport(800.0, 600.0);
    let hit = s.pick(Vec2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
    assert!(matches!(hit, Some(Pick::Face(_))));
    assert!(s.pick(Vec2::new(2.0, 2.0), Vec2::new(800.0, 600.0)).is_none());
}

#[test]
fn hover_and_selection_changes_rebuild_the_scene_only_when_needed() {
    let mut s = AppState::new();
    s.add_body(BodyKind::Box, "A".into(), boxed(10.0)).unwrap();
    let face = s.bodies[0].mesh.face_ranges[0].face;
    let v = s.scene_version;
    s.set_hover(None);
    assert_eq!(s.scene_version, v);
    s.set_hover(Some(Pick::Face(FaceRef { body: 0, face })));
    assert!(s.scene_version > v);
    let v = s.scene_version;
    s.set_hover(Some(Pick::Face(FaceRef { body: 0, face })));
    assert_eq!(s.scene_version, v);
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-app`
Expected: FAIL (Kompilierfehler, Module fehlen).

- [ ] **Step 3: Logik implementieren (ohne Oberfläche)**

`crates/planora-app/src/lib.rs`:

```rust
//! PlanoraCAD – Anwendungslogik und Oberfläche.

pub mod app;
pub mod browser;
pub mod create_dialog;
pub mod icons;
pub mod properties;
pub mod samples;
pub mod state;
pub mod theme;
pub mod viewport;
```

`crates/planora-app/src/samples.rs`:

```rust
use planora_kernel::{Profile, ProfileLoop, Segment};
use planora_math::{DVec2, PI};

/// Fest eingebaute Beispielprofile für die Extrusion (echte Skizzen folgen später).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtrudeSample {
    PlateWithHole,
    LShape,
    NotchedBar,
    TwoHoles,
}

impl ExtrudeSample {
    pub const ALL: [ExtrudeSample; 4] = [
        ExtrudeSample::PlateWithHole,
        ExtrudeSample::LShape,
        ExtrudeSample::NotchedBar,
        ExtrudeSample::TwoHoles,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExtrudeSample::PlateWithHole => "Platte mit rundem Loch",
            ExtrudeSample::LShape => "L-Profil",
            ExtrudeSample::NotchedBar => "Leiste mit runder Kerbe",
            ExtrudeSample::TwoHoles => "Platte mit zwei Löchern",
        }
    }

    pub fn default_height(self) -> f64 {
        match self {
            ExtrudeSample::PlateWithHole | ExtrudeSample::TwoHoles => 5.0,
            ExtrudeSample::LShape => 20.0,
            ExtrudeSample::NotchedBar => 15.0,
        }
    }

    pub fn profile(self) -> Profile {
        match self {
            ExtrudeSample::PlateWithHole => Profile {
                outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(40.0, 40.0)),
                holes: vec![ProfileLoop::circle(DVec2::new(20.0, 20.0), 8.0)],
            },
            ExtrudeSample::LShape => Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(40.0, 0.0),
                    DVec2::new(40.0, 10.0),
                    DVec2::new(10.0, 10.0),
                    DVec2::new(10.0, 30.0),
                    DVec2::new(0.0, 30.0),
                ]),
                holes: vec![],
            },
            ExtrudeSample::NotchedBar => Profile {
                outer: ProfileLoop {
                    start: DVec2::new(0.0, 0.0),
                    segments: vec![
                        Segment::Line { to: DVec2::new(40.0, 0.0) },
                        Segment::Line { to: DVec2::new(40.0, 20.0) },
                        Segment::Line { to: DVec2::new(30.0, 20.0) },
                        Segment::Arc { center: DVec2::new(20.0, 20.0), sweep: -PI },
                        Segment::Line { to: DVec2::new(0.0, 20.0) },
                        Segment::Line { to: DVec2::new(0.0, 0.0) },
                    ],
                },
                holes: vec![],
            },
            ExtrudeSample::TwoHoles => Profile {
                outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(70.0, 30.0)),
                holes: vec![
                    ProfileLoop::circle(DVec2::new(20.0, 15.0), 6.0),
                    ProfileLoop::circle(DVec2::new(50.0, 15.0), 6.0),
                ],
            },
        }
    }
}

/// Fest eingebaute Beispielketten für die Rotation (Halbebene: x = Abstand von der Achse, y = Höhe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevolveSample {
    Vase,
    Capsule,
    Puck,
}

impl RevolveSample {
    pub const ALL: [RevolveSample; 3] =
        [RevolveSample::Vase, RevolveSample::Capsule, RevolveSample::Puck];

    pub fn label(self) -> &'static str {
        match self {
            RevolveSample::Vase => "Vase",
            RevolveSample::Capsule => "Kapsel",
            RevolveSample::Puck => "Scheibe mit gerundeter Kante",
        }
    }

    pub fn chain(self) -> ProfileLoop {
        match self {
            RevolveSample::Vase => ProfileLoop {
                start: DVec2::new(0.0, 0.0),
                segments: vec![
                    Segment::Line { to: DVec2::new(15.0, 0.0) },
                    Segment::Line { to: DVec2::new(10.0, 25.0) },
                    Segment::Line { to: DVec2::new(14.0, 45.0) },
                    Segment::Line { to: DVec2::new(0.0, 45.0) },
                ],
            },
            RevolveSample::Capsule => ProfileLoop {
                start: DVec2::new(0.0, 0.0),
                segments: vec![
                    Segment::Arc { center: DVec2::new(0.0, 10.0), sweep: PI / 2.0 },
                    Segment::Line { to: DVec2::new(10.0, 30.0) },
                    Segment::Arc { center: DVec2::new(0.0, 30.0), sweep: PI / 2.0 },
                ],
            },
            RevolveSample::Puck => ProfileLoop {
                start: DVec2::new(0.0, 0.0),
                segments: vec![
                    Segment::Line { to: DVec2::new(30.0, 0.0) },
                    Segment::Arc { center: DVec2::new(30.0, 10.0), sweep: PI / 2.0 },
                    Segment::Line { to: DVec2::new(0.0, 10.0) },
                ],
            },
        }
    }
}
```

`crates/planora-app/src/state.rs`:

```rust
use glam::{Vec2, Vec3};
use planora_kernel::{AnalyticKernel, Kernel, Solid};
use planora_math::{DMat4, DVec3};
use planora_mesh::{MassProperties, Mesh, TessellationOptions, mass_properties, tessellate};
use planora_render::{
    EdgeRef, FaceRef, Highlight, OrbitCamera, Palette, Pick, SceneData, build_scene, pick_edge,
    pick_face,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Extrusion,
    Revolution,
}

pub struct Body {
    pub name: String,
    pub kind: BodyKind,
    pub solid: Solid,
    pub mesh: Mesh,
    pub mass: MassProperties,
    /// Achsenparallele Hülle `(min, max)` in Millimetern.
    pub bounds: (DVec3, DVec3),
}

/// Alles, was die Oberfläche anzeigt. Enthält keine egui-Typen und ist deshalb testbar.
pub struct AppState {
    pub bodies: Vec<Body>,
    pub selected_body: Option<usize>,
    pub selection: Option<Pick>,
    pub hover: Option<Pick>,
    pub camera: OrbitCamera,
    pub scene: Arc<SceneData>,
    pub scene_version: u64,
    pub dark: bool,
    pub kernel: AnalyticKernel,
    pub chord_tolerance: f64,
    /// Letzte Fehlermeldung für die Statuszeile.
    pub message: Option<String>,
}

const GRID_HALF_EXTENT: f32 = 500.0;
const PICK_EDGE_PX: f32 = 6.0;
const BODY_GAP_MM: f64 = 10.0;

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let mut state = Self {
            bodies: Vec::new(),
            selected_body: None,
            selection: None,
            hover: None,
            camera: OrbitCamera::default(),
            scene: Arc::new(SceneData::default()),
            scene_version: 0,
            dark: true,
            kernel: AnalyticKernel::default(),
            chord_tolerance: 0.05,
            message: None,
        };
        state.rebuild_scene();
        state
    }

    pub fn palette(&self) -> Palette {
        if self.dark { Palette::dark() } else { Palette::light() }
    }

    /// Prüft den Körper, stellt ihn neben die vorhandenen und nimmt ihn auf.
    pub fn add_body(&mut self, kind: BodyKind, name: String, solid: Solid) -> Result<usize, String> {
        self.kernel
            .validate(&solid)
            .map_err(|e| format!("Ungültiger Körper: {e}"))?;
        let solid = match self.bodies.iter().map(|b| b.bounds.1.x).reduce(f64::max) {
            None => solid,
            Some(max_x) => {
                let mesh = self.mesh_of(&solid)?;
                let (min, _) = bounds_of(&mesh);
                let shift = DMat4::from_translation(DVec3::new(max_x + BODY_GAP_MM - min.x, 0.0, 0.0));
                self.kernel
                    .transform(&solid, &shift)
                    .map_err(|e| format!("Platzieren fehlgeschlagen: {e}"))?
            }
        };
        let mesh = self.mesh_of(&solid)?;
        let mass = mass_properties(&mesh);
        let bounds = bounds_of(&mesh);
        self.bodies.push(Body { name, kind, solid, mesh, mass, bounds });
        self.rebuild_scene();
        self.frame_all();
        Ok(self.bodies.len() - 1)
    }

    fn mesh_of(&self, solid: &Solid) -> Result<Mesh, String> {
        tessellate(solid, &TessellationOptions { chord_tolerance: self.chord_tolerance })
            .map_err(|e| format!("Tessellierung fehlgeschlagen: {e}"))
    }

    pub fn remove_body(&mut self, index: usize) {
        if index >= self.bodies.len() {
            return;
        }
        self.bodies.remove(index);
        self.hover = None;
        self.selection = None;
        self.selected_body = match self.selected_body {
            Some(i) if i == index => None,
            Some(i) if i > index => Some(i - 1),
            other => other,
        };
        self.rebuild_scene();
    }

    pub fn rebuild_scene(&mut self) {
        let meshes: Vec<&Mesh> = self.bodies.iter().map(|b| &b.mesh).collect();
        let highlight = Highlight { hover: self.hover, selected: self.selection };
        self.scene = Arc::new(build_scene(&meshes, &highlight, &self.palette(), GRID_HALF_EXTENT));
        self.scene_version += 1;
    }

    /// Richtet die Kamera auf alle Körper aus (ohne Körper: Standardansicht auf den Ursprung).
    pub fn frame_all(&mut self) {
        let mut iter = self.bodies.iter().map(|b| b.bounds);
        let Some(first) = iter.next() else {
            self.camera.frame_bounds(Vec3::splat(-25.0), Vec3::splat(25.0));
            return;
        };
        let (min, max) = iter.fold(first, |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
        self.camera.frame_bounds(
            Vec3::new(min.x as f32, min.y as f32, min.z as f32),
            Vec3::new(max.x as f32, max.y as f32, max.z as f32),
        );
    }

    pub fn set_hover(&mut self, hover: Option<Pick>) {
        if self.hover != hover {
            self.hover = hover;
            self.rebuild_scene();
        }
    }

    pub fn set_selection(&mut self, selection: Option<Pick>) {
        if self.selection != selection {
            self.selection = selection;
            self.selected_body = match selection {
                Some(Pick::Face(FaceRef { body, .. })) | Some(Pick::Edge(EdgeRef { body, .. })) => {
                    Some(body)
                }
                None => self.selected_body,
            };
            self.rebuild_scene();
        }
    }

    /// Was liegt unter dem Mauszeiger? `cursor` in Pixeln (Ursprung oben links) im Viewport der Größe `viewport`.
    /// Kanten haben Vorrang, wenn sie nahe genug und nicht verdeckt sind.
    pub fn pick(&self, cursor: Vec2, viewport: Vec2) -> Option<Pick> {
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }
        let ndc = Vec2::new(
            2.0 * cursor.x / viewport.x - 1.0,
            1.0 - 2.0 * cursor.y / viewport.y,
        );
        let (origin, dir) = self.camera.ray(ndc);

        let mut nearest_face: Option<(usize, planora_render::FaceHit)> = None;
        for (body, b) in self.bodies.iter().enumerate() {
            if let Some(hit) = pick_face(&b.mesh, origin, dir) {
                if nearest_face.is_none_or(|(_, h)| hit.distance < h.distance) {
                    nearest_face = Some((body, hit));
                }
            }
        }
        let occluder = nearest_face.map(|(_, h)| h.distance);

        let mut best_edge: Option<(usize, planora_render::EdgeHit)> = None;
        for (body, b) in self.bodies.iter().enumerate() {
            if let Some(hit) =
                pick_edge(&b.mesh, &self.camera, viewport, cursor, PICK_EDGE_PX, occluder)
            {
                if best_edge.is_none_or(|(_, h)| hit.pixel_distance < h.pixel_distance) {
                    best_edge = Some((body, hit));
                }
            }
        }
        if let Some((body, hit)) = best_edge {
            return Some(Pick::Edge(EdgeRef { body, edge: hit.edge }));
        }
        nearest_face.map(|(body, hit)| Pick::Face(FaceRef { body, face: hit.face }))
    }
}

fn bounds_of(mesh: &Mesh) -> (DVec3, DVec3) {
    mesh.positions.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}
```

Hinweis: `AppState` nutzt `glam::{Vec2, Vec3}` (f32); `planora_math::DVec3` ist `glam::DVec3` derselben glam-Version, die Typen kommen also aus derselben Bibliothek.

- [ ] **Step 4: Logik-Tests laufen lassen**

Run: `cargo test -p planora-app`
Expected: PASS für `samples` und `state`. (Die Oberflächen-Module aus Step 5 müssen dafür schon als Dateien existieren, weil `lib.rs` sie einbindet. Falls die Tests vor Step 5 laufen sollen, die Zeilen `pub mod app; browser; create_dialog; icons; properties; theme; viewport;` in `lib.rs` vorübergehend auskommentieren und in Step 5 wieder einschalten.)

- [ ] **Step 5: Oberfläche implementieren**

`crates/planora-app/src/theme.rs`:

```rust
use egui::{Color32, CornerRadius, Visuals};

fn base(dark: bool) -> Visuals {
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    let radius = CornerRadius::same(6);
    let (panel, window, extreme, faint, inactive, hovered, accent) = if dark {
        (
            Color32::from_rgb(0x1e, 0x21, 0x27),
            Color32::from_rgb(0x25, 0x28, 0x30),
            Color32::from_rgb(0x15, 0x17, 0x1b),
            Color32::from_rgb(0x26, 0x29, 0x31),
            Color32::from_rgb(0x2c, 0x30, 0x39),
            Color32::from_rgb(0x38, 0x3d, 0x49),
            Color32::from_rgb(0x4c, 0x9a, 0xff),
        )
    } else {
        (
            Color32::from_rgb(0xf3, 0xf4, 0xf7),
            Color32::from_rgb(0xff, 0xff, 0xff),
            Color32::from_rgb(0xff, 0xff, 0xff),
            Color32::from_rgb(0xea, 0xec, 0xf1),
            Color32::from_rgb(0xe3, 0xe6, 0xed),
            Color32::from_rgb(0xd5, 0xda, 0xe4),
            Color32::from_rgb(0x1f, 0x6f, 0xe0),
        )
    };
    v.panel_fill = panel;
    v.window_fill = window;
    v.extreme_bg_color = extreme;
    v.faint_bg_color = faint;
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = radius;
    v.widgets.inactive.weak_bg_fill = inactive;
    v.widgets.inactive.bg_fill = inactive;
    v.widgets.hovered.weak_bg_fill = hovered;
    v.widgets.hovered.bg_fill = hovered;
    v.widgets.active.weak_bg_fill = accent;
    v.widgets.active.bg_fill = accent;
    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.inactive.corner_radius = radius;
    v.widgets.hovered.corner_radius = radius;
    v.widgets.active.corner_radius = radius;
    v.selection.bg_fill = accent.linear_multiply(0.55);
    v.hyperlink_color = accent;
    v
}

/// Setzt Farben, Rundungen und Abstände für das gewünschte Theme.
pub fn apply(ctx: &egui::Context, dark: bool) {
    ctx.set_visuals_of(egui::Theme::Dark, base(true));
    ctx.set_visuals_of(egui::Theme::Light, base(false));
    ctx.set_theme(if dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.menu_margin = egui::Margin::same(6);
    });
}

/// Hintergrundfarbe des 3D-Viewports.
pub fn viewport_background(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x12, 0x14, 0x18)
    } else {
        Color32::from_rgb(0xf8, 0xf9, 0xfb)
    }
}
```

`crates/planora-app/src/icons.rs` (selbst gezeichnete Vektor-Icons, keine Fremdlizenz):

```rust
use crate::state::BodyKind;
use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};

/// Zeichnet ein kleines Körpersymbol in `rect`.
pub fn paint_body_icon(painter: &Painter, rect: Rect, kind: BodyKind, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    match kind {
        BodyKind::Box => {
            let front = Rect::from_center_size(c + vec2(-r * 0.15, r * 0.15), vec2(r * 1.4, r * 1.4));
            let off = vec2(r * 0.4, -r * 0.4);
            painter.rect_stroke(front, 1.0, stroke, egui::StrokeKind::Inside);
            painter.line_segment([front.left_top(), front.left_top() + off], stroke);
            painter.line_segment([front.right_top(), front.right_top() + off], stroke);
            painter.line_segment([front.right_bottom(), front.right_bottom() + off], stroke);
            painter.line_segment(
                [front.left_top() + off, front.right_top() + off],
                stroke,
            );
            painter.line_segment(
                [front.right_top() + off, front.right_bottom() + off],
                stroke,
            );
        }
        BodyKind::Cylinder | BodyKind::Extrusion => {
            let top = c + vec2(0.0, -r * 0.7);
            let bottom = c + vec2(0.0, r * 0.7);
            let rx = r * 0.8;
            let ry = r * 0.3;
            painter.add(egui::Shape::ellipse_stroke(top, vec2(rx, ry), stroke));
            painter.add(egui::Shape::ellipse_stroke(bottom, vec2(rx, ry), stroke));
            painter.line_segment([pos2(c.x - rx, top.y), pos2(c.x - rx, bottom.y)], stroke);
            painter.line_segment([pos2(c.x + rx, top.y), pos2(c.x + rx, bottom.y)], stroke);
        }
        BodyKind::Sphere | BodyKind::Revolution => {
            painter.circle_stroke(c, r, stroke);
            painter.add(egui::Shape::ellipse_stroke(c, vec2(r, r * 0.35), stroke));
        }
        BodyKind::Cone => {
            let apex: Pos2 = c + vec2(0.0, -r);
            let base_y = c.y + r * 0.7;
            let rx = r * 0.85;
            painter.line_segment([apex, pos2(c.x - rx, base_y)], stroke);
            painter.line_segment([apex, pos2(c.x + rx, base_y)], stroke);
            painter.add(egui::Shape::ellipse_stroke(
                pos2(c.x, base_y),
                vec2(rx, r * 0.28),
                stroke,
            ));
        }
    }
}
```

`crates/planora-app/src/create_dialog.rs`:

```rust
use crate::samples::{ExtrudeSample, RevolveSample};
use crate::state::{AppState, BodyKind};
use planora_kernel::Kernel;
use planora_math::{DVec3, Frame};

/// Offener Erzeugen-Dialog samt Eingabewerten.
#[derive(Clone, Debug, PartialEq)]
pub enum CreateDialog {
    Box { x: f64, y: f64, z: f64 },
    Cylinder { radius: f64, height: f64 },
    Sphere { radius: f64 },
    Cone { radius: f64, height: f64 },
    Extrude { sample: ExtrudeSample, height: f64 },
    Revolve { sample: RevolveSample },
}

impl CreateDialog {
    pub fn title(&self) -> &'static str {
        match self {
            CreateDialog::Box { .. } => "Quader erzeugen",
            CreateDialog::Cylinder { .. } => "Zylinder erzeugen",
            CreateDialog::Sphere { .. } => "Kugel erzeugen",
            CreateDialog::Cone { .. } => "Kegel erzeugen",
            CreateDialog::Extrude { .. } => "Extrusion erzeugen",
            CreateDialog::Revolve { .. } => "Rotation erzeugen",
        }
    }

    fn drag(ui: &mut egui::Ui, label: &str, value: &mut f64) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::DragValue::new(value)
                    .speed(0.5)
                    .range(0.001..=100_000.0)
                    .suffix(" mm"),
            );
        });
    }

    /// Zeigt den Dialog. Gibt `true` zurück, solange er offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let mut create = false;
        let mut cancel = false;
        egui::Window::new(self.title())
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                match self {
                    CreateDialog::Box { x, y, z } => {
                        Self::drag(ui, "Breite (x)", x);
                        Self::drag(ui, "Tiefe (y)", y);
                        Self::drag(ui, "Höhe (z)", z);
                    }
                    CreateDialog::Cylinder { radius, height } => {
                        Self::drag(ui, "Radius", radius);
                        Self::drag(ui, "Höhe", height);
                    }
                    CreateDialog::Sphere { radius } => Self::drag(ui, "Radius", radius),
                    CreateDialog::Cone { radius, height } => {
                        Self::drag(ui, "Radius", radius);
                        Self::drag(ui, "Höhe", height);
                    }
                    CreateDialog::Extrude { sample, height } => {
                        ui.label("Beispielprofil");
                        for s in ExtrudeSample::ALL {
                            ui.selectable_value(sample, s, s.label());
                        }
                        Self::drag(ui, "Höhe", height);
                    }
                    CreateDialog::Revolve { sample } => {
                        ui.label("Beispielprofil");
                        for s in RevolveSample::ALL {
                            ui.selectable_value(sample, s, s.label());
                        }
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Erzeugen").clicked() {
                        create = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        cancel = true;
                    }
                });
            });
        if create {
            self.create(state);
            return false;
        }
        open && !cancel
    }

    fn create(&self, state: &mut AppState) {
        let k = state.kernel;
        let (kind, name, solid) = match self {
            CreateDialog::Box { x, y, z } => (
                BodyKind::Box,
                "Quader".to_string(),
                k.make_box(DVec3::new(*x, *y, *z)),
            ),
            CreateDialog::Cylinder { radius, height } => (
                BodyKind::Cylinder,
                "Zylinder".to_string(),
                k.make_cylinder(*radius, *height),
            ),
            CreateDialog::Sphere { radius } => {
                (BodyKind::Sphere, "Kugel".to_string(), k.make_sphere(*radius))
            }
            CreateDialog::Cone { radius, height } => {
                (BodyKind::Cone, "Kegel".to_string(), k.make_cone(*radius, *height))
            }
            CreateDialog::Extrude { sample, height } => (
                BodyKind::Extrusion,
                format!("Extrusion: {}", sample.label()),
                k.extrude(&sample.profile(), &Frame::WORLD, *height),
            ),
            CreateDialog::Revolve { sample } => (
                BodyKind::Revolution,
                format!("Rotation: {}", sample.label()),
                k.revolve(&sample.chain(), &Frame::WORLD),
            ),
        };
        state.message = None;
        match solid {
            Ok(solid) => {
                let number = state.bodies.len() + 1;
                if let Err(e) = state.add_body(kind, format!("{name} {number}"), solid) {
                    state.message = Some(e);
                }
            }
            Err(e) => state.message = Some(format!("Erzeugen fehlgeschlagen: {e}")),
        }
    }
}
```

`crates/planora-app/src/browser.rs`:

```rust
use crate::icons::paint_body_icon;
use crate::state::AppState;
use egui::{Sense, vec2};

/// Liste der Körper mit Symbol; Klick wählt einen Körper aus.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Körper");
    ui.add_space(4.0);
    if state.bodies.is_empty() {
        ui.weak("Noch keine Körper. Über „Erzeugen“ im Menü lässt sich einer anlegen.");
        return;
    }
    let mut clicked = None;
    for (i, body) in state.bodies.iter().enumerate() {
        let selected = state.selected_body == Some(i);
        let row_height = 28.0;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::click());
        if selected {
            ui.painter()
                .rect_filled(rect, 6.0, ui.visuals().selection.bg_fill);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, 6.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let color = ui.visuals().text_color();
        let icon_rect = egui::Rect::from_min_size(rect.min + vec2(4.0, 2.0), vec2(24.0, 24.0));
        paint_body_icon(ui.painter(), icon_rect, body.kind, color);
        ui.painter().text(
            rect.min + vec2(36.0, row_height / 2.0),
            egui::Align2::LEFT_CENTER,
            &body.name,
            egui::FontId::proportional(14.0),
            color,
        );
        if response.clicked() {
            clicked = Some(i);
        }
    }
    if let Some(i) = clicked {
        state.selected_body = Some(i);
    }
}
```

`crates/planora-app/src/properties.rs`:

```rust
use crate::state::AppState;
use planora_kernel::{Curve, Surface};
use planora_render::Pick;

fn surface_name(s: &Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "Ebene",
        Surface::Cylinder { .. } => "Zylinderfläche",
        Surface::Cone { .. } => "Kegelfläche",
        Surface::Sphere { .. } => "Kugelfläche",
        Surface::Torus { .. } => "Torusfläche",
    }
}

fn curve_name(c: &Curve) -> &'static str {
    match c {
        Curve::Line { .. } => "Gerade",
        Curve::Circle { .. } => "Kreis",
        Curve::Ellipse { .. } => "Ellipse",
    }
}

/// Maße des gewählten Körpers sowie Angaben zur gewählten Fläche oder Kante.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Eigenschaften");
    ui.add_space(4.0);
    let Some(index) = state.selected_body.filter(|&i| i < state.bodies.len()) else {
        ui.weak("Nichts ausgewählt.");
        return;
    };

    let mut delete = false;
    {
        let body = &mut state.bodies[index];
        ui.label("Name");
        ui.text_edit_singleline(&mut body.name);
        ui.add_space(6.0);
        let (min, max) = body.bounds;
        let size = max - min;
        egui::Grid::new("body_props").num_columns(2).show(ui, |ui| {
            ui.label("Volumen");
            ui.label(format!("{:.2} mm³", body.mass.volume));
            ui.end_row();
            ui.label("Oberfläche");
            ui.label(format!("{:.2} mm²", body.mass.surface_area));
            ui.end_row();
            ui.label("Schwerpunkt");
            ui.label(format!(
                "{:.2} / {:.2} / {:.2} mm",
                body.mass.centroid.x, body.mass.centroid.y, body.mass.centroid.z
            ));
            ui.end_row();
            ui.label("Abmessungen");
            ui.label(format!("{:.2} × {:.2} × {:.2} mm", size.x, size.y, size.z));
            ui.end_row();
            ui.label("Flächen / Kanten");
            ui.label(format!("{} / {}", body.solid.faces.len(), body.solid.edges.len()));
            ui.end_row();
        });
        ui.add_space(8.0);
        if ui.button("Körper löschen").clicked() {
            delete = true;
        }
    }

    ui.separator();
    match state.selection {
        Some(Pick::Face(f)) if f.body == index => {
            let face = &state.bodies[index].solid.faces[f.face];
            ui.label(format!("Ausgewählte Fläche: {}", surface_name(&face.surface)));
        }
        Some(Pick::Edge(e)) if e.body == index => {
            let body = &state.bodies[index];
            let edge = &body.solid.edges[e.edge];
            let length: f64 = body.mesh.edge_lines[e.edge.index()]
                .points
                .windows(2)
                .map(|w| w[0].distance(w[1]))
                .sum();
            ui.label(format!(
                "Ausgewählte Kante: {}, Länge ≈ {:.2} mm",
                curve_name(&edge.curve),
                length
            ));
        }
        _ => {
            ui.weak("Klicke im Viewport eine Fläche oder Kante an.");
        }
    }

    if delete {
        state.remove_body(index);
    }
}
```

`crates/planora-app/src/viewport.rs`:

```rust
use crate::state::AppState;
use crate::theme;
use egui::{Align2, Color32, FontId, PointerButton, Rect, Sense, Shape, Stroke, pos2, vec2};
use glam::Vec2;
use planora_render::gpu::ViewportCallback;
use planora_render::{ViewFace, cube_faces, cube_hit_test};

const CUBE_SIZE: f32 = 96.0;

fn cube_rect(viewport: Rect) -> Rect {
    Rect::from_min_size(
        pos2(viewport.right() - CUBE_SIZE - 12.0, viewport.top() + 12.0),
        vec2(CUBE_SIZE, CUBE_SIZE),
    )
}

/// 3D-Ansicht: Navigation, Auswahl, Zeichnen und Navigationswürfel.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::viewport_background(state.dark));
    state.camera.set_viewport(rect.width(), rect.height());

    let cube = cube_rect(rect);
    let pointer = response.hover_pos();
    let over_cube = pointer.is_some_and(|p| cube.contains(p));

    // Navigation
    let pan = response.dragged_by(PointerButton::Middle)
        || (response.dragged_by(PointerButton::Primary) && ui.input(|i| i.modifiers.shift));
    if pan {
        let d = response.drag_motion();
        state.camera.pan(d.x, d.y, rect.height());
    } else if response.dragged_by(PointerButton::Primary) {
        let d = response.drag_motion();
        state.camera.orbit(d.x, d.y);
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta().y);
        if scroll != 0.0 {
            state.camera.zoom(scroll);
        }
    }

    // Navigationswürfel
    let faces = cube_faces(&state.camera);
    let cube_center = cube.center();
    let half = CUBE_SIZE * 0.28;
    let to_screen = |v: Vec2| pos2(cube_center.x + v.x * half, cube_center.y - v.y * half);
    let mut cube_hover: Option<ViewFace> = None;
    if let (true, Some(p)) = (over_cube, pointer) {
        let local = Vec2::new((p.x - cube_center.x) / half, -(p.y - cube_center.y) / half);
        cube_hover = cube_hit_test(&faces, local);
    }
    if response.clicked() {
        if let Some(face) = cube_hover {
            state.camera.set_view(face);
        }
    }

    // Auswahl (nicht über dem Würfel)
    if let (false, Some(p)) = (over_cube, pointer) {
        let cursor = Vec2::new(p.x - rect.left(), p.y - rect.top());
        let viewport = Vec2::new(rect.width(), rect.height());
        let hit = state.pick(cursor, viewport);
        state.set_hover(hit);
        if response.clicked() {
            state.set_selection(hit);
        }
    } else if pointer.is_none() || over_cube {
        state.set_hover(None);
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.set_selection(None);
    }

    // 3D-Szene
    let cam = state.camera;
    painter.add(eframe::egui_wgpu::Callback::new_paint_callback(
        rect,
        ViewportCallback {
            view_proj: cam.view_proj().to_cols_array_2d(),
            light_dir: cam.light_dir().to_array(),
            scene: state.scene.clone(),
            scene_version: state.scene_version,
        },
    ));

    // Würfel zeichnen
    let (fill, hover_fill, line, text) = if state.dark {
        (
            Color32::from_rgba_unmultiplied(70, 76, 90, 235),
            Color32::from_rgba_unmultiplied(76, 154, 255, 235),
            Color32::from_rgb(150, 158, 175),
            Color32::from_rgb(235, 238, 245),
        )
    } else {
        (
            Color32::from_rgba_unmultiplied(225, 229, 238, 240),
            Color32::from_rgba_unmultiplied(31, 111, 224, 240),
            Color32::from_rgb(90, 98, 115),
            Color32::from_rgb(30, 34, 44),
        )
    };
    for f in &faces {
        let points: Vec<_> = f.corners.iter().map(|c| to_screen(*c)).collect();
        let color = if cube_hover == Some(f.face) { hover_fill } else { fill };
        painter.add(Shape::convex_polygon(points.clone(), color, Stroke::new(1.0, line)));
        let center = points.iter().fold(pos2(0.0, 0.0), |a, p| pos2(a.x + p.x / 4.0, a.y + p.y / 4.0));
        painter.text(center, Align2::CENTER_CENTER, f.face.label(), FontId::proportional(11.0), text);
    }
}
```

Hinweis: `eframe::egui_wgpu::Callback` wird hier über `eframe` angesprochen; die `planora-render`-Crate verwendet dieselbe `egui-wgpu`-Version (0.36), die Typen stimmen also überein.

`crates/planora-app/src/app.rs`:

```rust
use crate::create_dialog::CreateDialog;
use crate::samples::{ExtrudeSample, RevolveSample};
use crate::state::AppState;
use crate::{browser, properties, theme, viewport};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use planora_render::ViewFace;
use planora_render::gpu::GpuScene;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Tab {
    Browser,
    Viewport,
    Properties,
}

struct Viewer<'a> {
    state: &'a mut AppState,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Browser => "Browser",
            Tab::Viewport => "3D-Ansicht",
            Tab::Properties => "Eigenschaften",
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Browser => browser::show(ui, self.state),
            Tab::Viewport => viewport::show(ui, self.state),
            Tab::Properties => properties::show(ui, self.state),
        }
    }

    fn is_closeable(&self, _tab: &Tab) -> bool {
        false
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        // Der Viewport füllt den Platz selbst aus und braucht keine Rollbalken.
        if *tab == Tab::Viewport { [false, false] } else { [false, true] }
    }
}

pub struct PlanoraApp {
    state: AppState,
    dock: DockState<Tab>,
    dialog: Option<CreateDialog>,
    applied_dark: Option<bool>,
}

impl PlanoraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("PlanoraCAD benötigt den wgpu-Renderer");
        GpuScene::install(render_state);

        let mut dock = DockState::new(vec![Tab::Viewport]);
        let tree = dock.main_surface_mut();
        let [center, _left] = tree.split_left(NodeIndex::root(), 0.2, vec![Tab::Browser]);
        let [_center, _right] = tree.split_right(center, 0.75, vec![Tab::Properties]);

        let mut state = AppState::new();
        state.frame_all();
        Self {
            state,
            dock,
            dialog: None,
            applied_dark: None,
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Datei", |ui| {
                if ui.button("Beenden").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Erzeugen", |ui| {
                let mut open = |dialog: CreateDialog, ui: &mut egui::Ui| {
                    self.dialog = Some(dialog);
                    ui.close();
                };
                if ui.button("Quader …").clicked() {
                    open(CreateDialog::Box { x: 40.0, y: 30.0, z: 20.0 }, ui);
                }
                if ui.button("Zylinder …").clicked() {
                    open(CreateDialog::Cylinder { radius: 15.0, height: 30.0 }, ui);
                }
                if ui.button("Kugel …").clicked() {
                    open(CreateDialog::Sphere { radius: 20.0 }, ui);
                }
                if ui.button("Kegel …").clicked() {
                    open(CreateDialog::Cone { radius: 20.0, height: 40.0 }, ui);
                }
                ui.separator();
                if ui.button("Extrusion …").clicked() {
                    let sample = ExtrudeSample::PlateWithHole;
                    open(CreateDialog::Extrude { sample, height: sample.default_height() }, ui);
                }
                if ui.button("Rotation …").clicked() {
                    open(CreateDialog::Revolve { sample: RevolveSample::Vase }, ui);
                }
            });
            ui.menu_button("Ansicht", |ui| {
                for face in ViewFace::ALL {
                    if ui.button(face.label()).clicked() {
                        self.state.camera.set_view(face);
                        ui.close();
                    }
                }
                if ui.button("Isometrisch").clicked() {
                    self.state.camera.set_iso_view();
                    ui.close();
                }
                ui.separator();
                if ui.button("Alles einpassen").clicked() {
                    self.state.frame_all();
                    ui.close();
                }
                ui.separator();
                let label = if self.state.dark { "Helles Design" } else { "Dunkles Design" };
                if ui.button(label).clicked() {
                    self.state.dark = !self.state.dark;
                    self.state.rebuild_scene();
                    ui.close();
                }
            });
        });
    }
}

impl eframe::App for PlanoraApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.applied_dark != Some(self.state.dark) {
            theme::apply(ui.ctx(), self.state.dark);
            self.applied_dark = Some(self.state.dark);
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                match &self.state.message {
                    Some(m) => ui.colored_label(egui::Color32::from_rgb(230, 90, 90), m),
                    None => ui.weak(format!("{} Körper", self.state.bodies.len())),
                };
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let mut viewer = Viewer { state: &mut self.state };
            DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
        });

        if let Some(mut dialog) = self.dialog.take() {
            if dialog.show(ui.ctx(), &mut self.state) {
                self.dialog = Some(dialog);
            }
        }
    }
}
```

`crates/planora-app/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use planora_app::app::PlanoraApp;
use planora_render::gpu::MSAA_SAMPLES;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PlanoraCAD")
            .with_inner_size([1400.0, 900.0]),
        renderer: eframe::Renderer::Wgpu,
        multisampling: MSAA_SAMPLES as u16,
        depth_buffer: 32,
        ..Default::default()
    };
    eframe::run_native(
        "PlanoraCAD",
        options,
        Box::new(|cc| Ok(Box::new(PlanoraApp::new(cc)))),
    )
}
```

Zur API-Abweichung: Wenn `egui::Panel::top(...).show(ui, …)` oder `egui::MenuBar::new().ui(...)` in 0.36.2 anders heißt, den Namen gegen die Quelle `crates/egui/src/containers/panel.rs` bzw. `menu.rs` prüfen (die Namen wurden vorab dort verifiziert). Die Tests (Step 4) berühren diese Datei nicht.

- [ ] **Step 6: Bauen, Lint und alle Tests**

Run:

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

Expected: Alles grün. **`cargo deny check` prüft jetzt erstmals den kompletten egui/wgpu-Abhängigkeitsbaum.** Meldet es Lizenzen außerhalb der Erlaubnisliste (zu erwarten sind u. a. MPL-2.0, OFL-1.1 und Ubuntu-Font-Lizenz bei den in egui mitgelieferten Schriften sowie evtl. weitere): **anhalten, die betroffenen Crates mit Lizenz auflisten und den Menschen entscheiden lassen**, ob eine Ausnahme in `deny.toml` eingetragen wird (mit Begründung im Kommentar). Nichts selbstständig freigeben.

- [ ] **Step 7: Programm starten und von Hand abnehmen**

```powershell
cargo run -p planora-app --release
```

Abnahmeliste (alle Punkte müssen stimmen; bei Fehlern erst beheben, dann weiter):

1. Das Fenster „PlanoraCAD“ öffnet sich mit dunklem Design; links „Browser“, in der Mitte „3D-Ansicht“ mit Raster und farbigen Achsen (x rot, y grün, z blau), rechts „Eigenschaften“.
2. Menü *Erzeugen → Quader …* → Dialog → *Erzeugen*: Ein Quader erscheint, die Kamera passt sich an, der Browser zeigt „Quader 1“.
3. Zylinder, Kugel, Kegel erscheinen daneben, ohne sich zu überlappen; Kugel und Zylinder sind glatt (keine sichtbaren Löcher oder Risse).
4. *Extrusion → Platte mit rundem Loch* zeigt ein durchgehendes Loch; *Leiste mit runder Kerbe* zeigt die Kerbe; *Rotation → Vase*, *Kapsel* und *Scheibe mit gerundeter Kante* sehen richtig aus.
5. Drehen mit linker Maustaste, Verschieben mit Mittelmaus (oder Umschalt + links), Zoomen mit dem Mausrad – flüssig.
6. Der Navigationswürfel oben rechts folgt der Drehung; Klick auf „Oben“/„Vorne“ stellt die Ansicht um.
7. Über eine Fläche fahren hebt sie hell hervor; Klick wählt sie (orange), die Eigenschaften nennen den Flächentyp. Über eine Kante fahren/klicken markiert die Kante, die Eigenschaften zeigen Kurventyp und Länge. Esc hebt die Auswahl auf. Kanten hinter dem Körper werden nicht gewählt.
8. Eigenschaften eines Körpers zeigen Volumen, Oberfläche, Schwerpunkt, Abmessungen; der Quader 40×30×20 zeigt 24000,00 mm³.
9. „Körper löschen“ entfernt ihn; das Programm läuft mit leerer Szene weiter. Fenster sehr klein ziehen: kein Absturz.
10. *Ansicht → Helles Design* wechselt Oberfläche und 3D-Hintergrund und wieder zurück.
11. Ungültige Eingaben (Maß 0 im Dialog ist durch den Eingabebereich nicht möglich) – über *Datei → Beenden* schließt das Programm sauber.

Wiederholung unter Linux (mit den Paketen aus Task 1) oder über die CI: `cargo build -p planora-app` muss dort ebenfalls erfolgreich sein.

- [ ] **Step 8: README und Commit**

In `README.md` unter „Status“ ergänzen: „Erste Version: Quader, Zylinder, Kugel, Kegel, Extrusion und Rotation von Beispielprofilen, 3D-Ansicht mit Auswahl.“

```powershell
git add .
git commit -m "feat(app): Oberfläche mit Dock-Layout, Viewport, Browser, Eigenschaften und Navigationswürfel" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Selbstprüfung gegen die Spec

- **§2 Grundentscheidungen:** reines Rust, eigener Kernel mit `glam`/`robust`/`earcutr` (Task 2–7), `egui`+`wgpu`+`egui_dock` (Task 9–10), GPL-3.0 und `cargo-deny` (Task 1, Prüfung in Task 10), eigene Icons (Task 10), Windows+Linux (Task 1 CI, Task 10 Step 7).
- **§3 Workspace:** fünf Crates mit den vorgesehenen Abhängigkeiten (Task 1–10).
- **§4 Kernel:** Datenstruktur (Task 3), Toleranzen/`robust` (Task 2; `orient2d` ist vorhanden, der Kernel nutzt für die Orientierung der Profile die Flächenformel – exakte Prädikate werden erst mit den Booleschen Operationen in Teilprojekt 3 gebraucht), Umfang Quader/Zylinder/Kugel/Kegel/Extrusion/Rotation/Transformationen/Validator (Task 4–6), Fassade (Task 4–6), Volumen/Schwerpunkt in `planora-mesh` (Task 7, in der Spec so festgehalten).
- **§5 Tessellierung:** Task 7 (Sehnenabweichung, Flächenzuordnung, Kantenlinien).
- **§6 Viewport:** Kamera, Navigationswürfel, CPU-Picking (Task 8), wgpu/MSAA/Tiefe/Kanten über Flächen (Task 9), Raster/Achsen (Task 8 `build_scene`).
- **§7 Oberfläche:** Theme, Dock, Menü „Erzeugen“, Browser, Eigenschaften (Task 10).
- **§8 Tests:** Eigenschaftstests (Task 7 `proptest`), Netzgeschlossenheit (Task 7), Logiktests Kamera/Picking (Task 8), CI mit `fmt`, `clippy`, `test`, `deny` (Task 1).
- **§9 Erfolgskriterien:** Task 10 Step 6 und 7.
- **Typkonsistenz:** `Kernel`-Methoden (`make_box`, `make_cylinder`, `make_sphere`, `make_cone`, `extrude`, `revolve`, `transform`, `validate`) werden in Task 4–6 definiert und in Task 7–10 so verwendet; `Pick`/`FaceRef`/`EdgeRef` kommen aus Task 8 und werden in Task 10 verwendet; `ViewportCallback`/`GpuScene` aus Task 9 in Task 10.
- **Bekannte, bewusst offene Feinjustierung:** Bei `frame_bounds` (Task 8) und der Sichtbarkeitsschwelle in `cube_faces` (Task 8) sind in den Hinweisen Ersatzwerte genannt, die nur dann einzusetzen sind, wenn der zugehörige Test knapp scheitert. Die Tests sind verbindlich, die Formeln nicht.
- **Clippy:** `cargo clippy … -D warnings` läuft erst in Task 10 Step 6 und in der CI über den ganzen Workspace. Dort gemeldete Hinweise (z. B. `needless_range_loop`) werden verhaltensneutral behoben; in den früheren Tasks genügt `cargo test`.
