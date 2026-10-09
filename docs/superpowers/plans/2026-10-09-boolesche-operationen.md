# PlanoraCAD Boolesche Operationen – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Vereinigen, Abziehen und Schneiden zweier Körper im reinen Rust-Kernel (Stufe A: Schnitte aus Geraden, Kreisen und Ellipsen), mit Netz für beschnittene Flächen und Bedienung im Programm.

**Architecture:** Neues Modul `planora-kernel/src/boolean/`: Schnittkurven (analytisch) → Randpunkte per Nullstellensuche (implizite Flächenfunktionen) → Flächen über einen ebenen Graphen mit Drehsystem auftrennen → Stücke per Strahlverfahren einordnen → nach Tabelle auswählen → zu einem `Solid` mit eventuell mehreren Schalen zusammennähen. Kanten behalten nur ihre 3D-Kurve; Parameterform (u,v) wird über exakte Umkehrabbildungen berechnet. `planora-mesh` triangulierte beschnittene gekrümmte Flächen im (u,v)-Raum mit `spade`.

**Tech Stack:** Rust (Edition 2024), `glam`, `robust`, `thiserror`, `proptest`, neu: `spade` 2.15 (MIT OR Apache-2.0, am 2026-10-09 geprüft), bestehend `egui`/`eframe` 0.36.

**Spec:** `docs/superpowers/specs/2026-10-09-boolesche-operationen-design.md` (baut auf `2026-10-09-fundament-und-kernel-design.md` auf)

## Global Constraints

- Reines Rust; Zahlen `f64`, Einheit Millimeter; eine zentrale `Tolerance` (linear 1e-6, angular 1e-9), keine neuen Epsilon-Konstanten im Boolean-Code außer den in diesem Plan ausdrücklich benannten Abtast-/Rechengrenzen.
- Der Kernel (`planora-kernel`) kennt weder Oberfläche noch Grafik noch Tessellierung (die Volumenprobe liegt in `planora-mesh` und in der App).
- Projektlizenz GPL-3.0-only; **jede neue Abhängigkeit wird vorher auf Lizenz geprüft**: einzig neu ist `spade` (MIT OR Apache-2.0). `cargo deny check` muss grün bleiben; Erlaubnisliste in `deny.toml` nie selbstständig erweitern – bei Befund anhalten und den Menschen fragen.
- Kein Autodesk-Material; Oberflächentexte und Fehlermeldungen auf Deutsch; dunkles Theme bleibt Standard.
- Nicht unterstützte Flächenpaare liefern `BooleanError::UnsupportedIntersection`, niemals ein falsches Ergebnis. Panik für gültige Eingaben ist ausgeschlossen.
- Alle Eingangskörper werden zuerst mit `validate` geprüft; jedes Ergebnis geht durch `validate`.
- Plattformen: Windows (MSVC) und Linux.

## Review Focus

Eingaben, die die Spec andeutet, die aber leicht durchrutschen; jede Zeile hat einen Test in der genannten Task:

1. **Bündige Stapelung und bündige Werkzeuge:** Quader auf Quader (Gegenseiten bündig), Zylinder gleicher Höhe wie der Quader beim Abziehen (Deckflächen koplanar) → gültiges Ergebnis (Task 8, Task 9).
2. **Identische Operanden** (A op A): Vereinigen = A, Schneiden = A, Abziehen = `EmptyResult` (Task 8).
3. **Enthaltensein und Getrenntheit:** B vollständig in A (Hohlraum, zwei Schalen), A in B, beide getrennt, nur Fläche berührend (Task 8).
4. **Kanten-/Eckenberührung ohne Fläche** (Quader berühren sich nur an einer Kante): kein Panik, sondern gültiges Ergebnis oder benannter Fehler (Task 8).
5. **Nicht unterstütztes Paar** (zwei sich schräg durchdringende Zylinder) und **Folgeoperationen** (Ergebnis wieder Eingabe): Fehler `UnsupportedIntersection` bzw. korrektes Kettenergebnis (Task 9, Task 11).

## Hinweis zur Verifikation und zum Code in diesem Plan

Der Code in den Schritten ist die **Referenzimplementierung des beabsichtigten Entwurfs** und wurde noch nicht kompiliert. Er darf zum Kompilieren und zur Fehlerbehebung angepasst und intern umstrukturiert werden, solange die benannten Schnittstellen (Abschnitte „Interfaces“) und **alle Tests unverändert** erfüllt werden. Schlägt ein Test fehl, wird der Code korrigiert, nicht der Test – es sei denn, der Test selbst enthält einen Tippfehler (dann berichten). Für `spade`, `egui` usw. gilt die API der in `Cargo.lock` aufgelösten Version; im Zweifel gegen den vendorierten Quelltext in `%USERPROFILE%\.cargo\registry\src` prüfen. Rust ist installiert (`export PATH="$PATH:/c/Users/denni/.cargo/bin"`).

## Dateistruktur

```
crates/planora-kernel/
├─ src/
│  ├─ geometry.rs            (ändern: implicit, project, pole_vs, Curve::project/tangent)
│  ├─ validate.rs            (ändern: Euler-Poincaré je Schale)
│  ├─ kernel.rs, lib.rs      (ändern: boolean in Trait/Exporte)
│  └─ boolean/
│     ├─ mod.rs              boolean(), BoolOp, Re-Exporte
│     ├─ error.rs            BooleanError
│     ├─ seg.rs              Seg (3D-Kurvenstück), reverse_loop
│     ├─ roots.rs            Kurve↔Fläche-Nullstellen, Stück↔Stück-Schnitte
│     ├─ surfsurf.rs         Fläche↔Fläche-Schnitte (Stufe A)
│     ├─ uv.rs               FaceGeom, Schleifen im Parameterraum, Punkt-in-Fläche
│     ├─ split.rs            Flächen auftrennen (ebener Graph, Drehsystem)
│     ├─ imprint.rs          Schnittsegmente je Fläche sammeln (Beschnitt)
│     ├─ classify.rs         Strahlverfahren, innen/außen/bündig
│     ├─ select.rs           Auswahltabelle, Piece
│     └─ sew.rs              Zusammennähen zu Solid (Schalen)
└─ tests/ surface_ops.rs roots.rs surfsurf.rs uv.rs split.rs imprint.rs classify.rs boolean_basic.rs
crates/planora-mesh/
├─ Cargo.toml                (spade)
├─ src/tessellate.rs         (ändern: beschnittene Flächen)  src/trimmed.rs (neu)  src/volume.rs (neu)
└─ tests/ boolean_planar.rs boolean_curved.rs trimmed.rs
crates/planora-app/
└─ src/ state.rs boolean_dialog.rs properties.rs app.rs   tests/ boolean_state.rs
```

---

### Task 1: Kernel-Grundlagen für Flächen und Kurven

**Files:**
- Modify: `crates/planora-kernel/src/geometry.rs`
- Test: `crates/planora-kernel/tests/surface_ops.rs`

**Interfaces:**
- Produces:
  - `Surface::implicit(&self, p: DVec3) -> f64` (> 0 auf der Seite der natürlichen Normale, 0 auf der Fläche, näherungsweise Abstand)
  - `Surface::project(&self, p: DVec3) -> (f64, f64)` (Umkehrabbildung; `u` in (−π, π] bei periodischen Flächen, Ebene: lokale x/y)
  - `Surface::is_periodic(&self) -> bool`, `Surface::pole_vs(&self) -> Vec<f64>`
  - `Curve::project(&self, p: DVec3) -> f64` (Parameter t), `Curve::tangent(&self, t: f64) -> DVec3` (nicht normiert, Richtung wachsendes t)

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/surface_ops.rs`

```rust
use planora_kernel::*;
use planora_math::*;

fn frame() -> Frame {
    Frame::from_origin_normal(DVec3::new(1.0, -2.0, 3.0), DVec3::new(0.2, 0.5, 0.8))
}

fn samples() -> Vec<(Surface, f64, f64)> {
    let f = frame();
    vec![
        (Surface::Plane { frame: f }, 0.7, -1.2),
        (Surface::Cylinder { frame: f, radius: 2.5 }, 1.1, 0.8),
        (Surface::Cone { frame: f, radius: 3.0, slope: 0.4 }, 2.0, 0.5),
        (Surface::Cone { frame: f, radius: 3.0, slope: -0.4 }, 2.0, 0.5),
        (Surface::Sphere { frame: f, radius: 4.0 }, 0.9, 0.4),
        (Surface::Torus { frame: f, major: 5.0, minor: 1.5 }, 2.2, 1.1),
    ]
}

#[test]
fn implicit_is_zero_on_the_surface() {
    for (s, u, v) in samples() {
        assert!(s.implicit(s.point(u, v)).abs() < 1e-12, "{s:?}");
    }
}

#[test]
fn implicit_is_positive_on_the_normal_side() {
    for (s, u, v) in samples() {
        let p = s.point(u, v);
        let n = s.normal(u, v);
        assert!(s.implicit(p + n * 0.1) > 0.05, "{s:?}");
        assert!(s.implicit(p - n * 0.1) < -0.05, "{s:?}");
    }
}

#[test]
fn project_inverts_point() {
    for (s, u, v) in samples() {
        let p = s.point(u, v);
        let (pu, pv) = s.project(p);
        assert!(s.point(pu, pv).distance(p) < 1e-9, "{s:?}");
    }
}

#[test]
fn periodicity_and_poles() {
    let f = Frame::WORLD;
    assert!(!Surface::Plane { frame: f }.is_periodic());
    assert!(Surface::Cylinder { frame: f, radius: 1.0 }.is_periodic());
    assert!(Surface::Cylinder { frame: f, radius: 1.0 }.pole_vs().is_empty());
    let poles = Surface::Sphere { frame: f, radius: 2.0 }.pole_vs();
    assert_eq!(poles.len(), 2);
    assert!((poles[0] + PI / 2.0).abs() < 1e-12 && (poles[1] - PI / 2.0).abs() < 1e-12);
    // Kegel mit radius 3 und slope -0.5: Spitze bei v = 6
    let apex = Surface::Cone { frame: f, radius: 3.0, slope: -0.5 }.pole_vs();
    assert_eq!(apex.len(), 1);
    assert!((apex[0] - 6.0).abs() < 1e-12);
    assert!(Surface::Torus { frame: f, major: 3.0, minor: 1.0 }.pole_vs().is_empty());
}

#[test]
fn curve_project_inverts_point() {
    let f = frame();
    let curves = [
        Curve::Line { origin: DVec3::new(1.0, 2.0, 3.0), dir: DVec3::new(0.0, 0.6, 0.8) },
        Curve::Circle { frame: f, radius: 2.0 },
        Curve::Ellipse { frame: f, major: 3.0, minor: 1.5 },
    ];
    for c in &curves {
        for t in [0.3, 1.2, 2.5] {
            let p = c.point(t);
            let back = c.project(p);
            assert!(c.point(back).distance(p) < 1e-9, "{c:?} t={t}");
        }
    }
}

#[test]
fn curve_tangent_matches_finite_difference() {
    let f = frame();
    let curves = [
        Curve::Line { origin: DVec3::ZERO, dir: DVec3::X },
        Curve::Circle { frame: f, radius: 2.0 },
        Curve::Ellipse { frame: f, major: 3.0, minor: 1.5 },
    ];
    let h = 1e-6;
    for c in &curves {
        for t in [0.3, 1.2, 2.5] {
            let num = (c.point(t + h) - c.point(t - h)) / (2.0 * h);
            assert!(c.tangent(t).distance(num) < 1e-5, "{c:?} t={t}");
        }
    }
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test surface_ops`
Expected: FAIL (Kompilierfehler: `implicit`, `project`, … fehlen).

- [ ] **Step 3: Implementieren** – in `geometry.rs`, Import um `PI` ergänzen (`use planora_math::{DVec3, Frame, PI};`) und anhängen:

```rust
impl Curve {
    /// Parameter `t` des Kurvenpunkts, der `p` am nächsten liegt (für Punkte auf der Kurve exakt).
    pub fn project(&self, p: DVec3) -> f64 {
        match self {
            Curve::Line { origin, dir } => (p - *origin).dot(*dir),
            Curve::Circle { frame, .. } => {
                let l = frame.to_local(p);
                l.y.atan2(l.x)
            }
            Curve::Ellipse { frame, major, minor } => {
                let l = frame.to_local(p);
                (l.y / minor).atan2(l.x / major)
            }
        }
    }

    /// Ableitung nach `t` (nicht normiert).
    pub fn tangent(&self, t: f64) -> DVec3 {
        match self {
            Curve::Line { dir, .. } => *dir,
            Curve::Circle { frame, radius } => (frame.y * t.cos() - frame.x * t.sin()) * *radius,
            Curve::Ellipse { frame, major, minor } => {
                frame.y * (minor * t.cos()) - frame.x * (major * t.sin())
            }
        }
    }
}

impl Surface {
    /// Vorzeichenbehaftete Abstandsfunktion: > 0 auf der Seite der natürlichen Normale,
    /// 0 auf der Fläche (Zylinder, Kegel, Kugel, Torus: außen positiv).
    pub fn implicit(&self, p: DVec3) -> f64 {
        match self {
            Surface::Plane { frame } => (p - frame.origin).dot(frame.z),
            Surface::Cylinder { frame, radius } => {
                let d = p - frame.origin;
                let h = d.dot(frame.z);
                (d - frame.z * h).length() - radius
            }
            Surface::Cone { frame, radius, slope } => {
                let d = p - frame.origin;
                let h = d.dot(frame.z);
                let rho = (d - frame.z * h).length();
                (rho - (radius + slope * h)) / (1.0 + slope * slope).sqrt()
            }
            Surface::Sphere { frame, radius } => (p - frame.origin).length() - radius,
            Surface::Torus { frame, major, minor } => {
                let d = p - frame.origin;
                let h = d.dot(frame.z);
                let rho = (d - frame.z * h).length();
                ((rho - major).powi(2) + h * h).sqrt() - minor
            }
        }
    }

    /// Umkehrabbildung Punkt → (u, v). Bei periodischen Flächen liegt `u` in (−π, π].
    pub fn project(&self, p: DVec3) -> (f64, f64) {
        let l = self.frame().to_local(p);
        let u = l.y.atan2(l.x);
        let rho = (l.x * l.x + l.y * l.y).sqrt();
        match self {
            Surface::Plane { .. } => (l.x, l.y),
            Surface::Cylinder { .. } | Surface::Cone { .. } => (u, l.z),
            Surface::Sphere { .. } => (u, l.z.atan2(rho)),
            Surface::Torus { major, .. } => (u, l.z.atan2(rho - major)),
        }
    }

    pub fn is_periodic(&self) -> bool {
        !matches!(self, Surface::Plane { .. })
    }

    /// Werte von `v`, bei denen die u-Linie zu einem Punkt zusammenfällt (Kugelpole, Kegelspitze).
    pub fn pole_vs(&self) -> Vec<f64> {
        match self {
            Surface::Sphere { .. } => vec![-PI / 2.0, PI / 2.0],
            Surface::Cone { radius, slope, .. } if slope.abs() > 1e-12 => vec![-radius / slope],
            _ => vec![],
        }
    }
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel` und `cargo clippy -p planora-kernel --all-targets -- -D warnings`
Expected: PASS, clippy sauber.

- [ ] **Step 5: Commit**

```bash
git add crates/planora-kernel
git commit -m "feat(kernel): implizite Flächenfunktionen und Umkehrabbildungen" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Kurvenstücke und Nullstellen

**Files:**
- Create: `crates/planora-kernel/src/boolean/{mod,error,seg,roots}.rs`
- Modify: `crates/planora-kernel/src/lib.rs` (`pub mod boolean;`)
- Test: `crates/planora-kernel/tests/roots.rs`

**Interfaces:**
- Consumes: `Curve`, `Surface` (Task 1).
- Produces (alles aus `planora_kernel::boolean`):
  - `BooleanError` (siehe Code), `BoolOp::{Union, Difference, Intersection}`
  - `Seg { curve: Curve, t0: f64, t1: f64 }` mit `new`, `param(f)`, `at(f)`, `start()`, `end()`, `reversed()`, `sub(f0, f1)`, `tangent_at(f)`, `length()`, `closest(p) -> (f, dist)`; `reverse_loop(&[Seg]) -> Vec<Seg>`
  - `Root { t: f64, touching: bool }`, `CurveSurface::{Roots(Vec<Root>), Lies}`, `curve_surface_roots(&Curve, t0, t1, &Surface, &Tolerance) -> CurveSurface`
  - `seg_intersections(&Seg, &Seg, &Tolerance) -> Vec<(f64, f64)>` (Bruchteile `f_a`, `f_b` der Schnittpunkte; bei Überlappung die Enden der überlappenden Strecke)

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/roots.rs`

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn roots(c: CurveSurface) -> Vec<Root> {
    match c {
        CurveSurface::Roots(r) => r,
        CurveSurface::Lies => panic!("Kurve liegt auf der Fläche"),
    }
}

#[test]
fn line_hits_a_plane_once() {
    let line = Curve::Line { origin: DVec3::new(0.0, 0.0, -2.0), dir: DVec3::Z };
    let plane = Surface::Plane { frame: Frame::WORLD };
    let r = roots(curve_surface_roots(&line, 0.0, 4.0, &plane, &tol()));
    assert_eq!(r.len(), 1);
    assert!((r[0].t - 2.0).abs() < 1e-9 && !r[0].touching);
}

#[test]
fn line_hits_a_sphere_twice() {
    let line = Curve::Line { origin: DVec3::new(-3.0, 0.0, 0.0), dir: DVec3::X };
    let sphere = Surface::Sphere { frame: Frame::WORLD, radius: 1.0 };
    let mut r = roots(curve_surface_roots(&line, 0.0, 6.0, &sphere, &tol()));
    r.sort_by(|a, b| a.t.total_cmp(&b.t));
    assert_eq!(r.len(), 2);
    assert!((r[0].t - 2.0).abs() < 1e-9 && (r[1].t - 4.0).abs() < 1e-9);
}

#[test]
fn circle_crosses_a_plane_twice_and_touches_once() {
    let plane = Surface::Plane { frame: Frame::WORLD };
    // Kreis in der xz-Ebene, Mittelpunkt im Ursprung: schneidet z = 0 zweimal
    let f = Frame { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Z, z: -DVec3::Y };
    let c = Curve::Circle { frame: f, radius: 1.0 };
    let r = roots(curve_surface_roots(&c, 0.0, TAU, &plane, &tol()));
    assert_eq!(r.len(), 2, "{r:?}");
    // Kreis, dessen tiefster Punkt die Ebene berührt
    let f2 = Frame { origin: DVec3::new(0.0, 0.0, 1.0), ..f };
    let c2 = Curve::Circle { frame: f2, radius: 1.0 };
    let r2 = roots(curve_surface_roots(&c2, 0.0, TAU, &plane, &tol()));
    assert_eq!(r2.len(), 1);
    assert!(r2[0].touching);
}

#[test]
fn curve_lying_on_the_surface_is_reported() {
    let plane = Surface::Plane { frame: Frame::WORLD };
    let c = Curve::Circle { frame: Frame::WORLD, radius: 2.0 };
    assert!(matches!(
        curve_surface_roots(&c, 0.0, TAU, &plane, &tol()),
        CurveSurface::Lies
    ));
}

#[test]
fn endpoint_on_the_surface_counts_as_a_root() {
    let line = Curve::Line { origin: DVec3::ZERO, dir: DVec3::Z };
    let plane = Surface::Plane { frame: Frame::WORLD };
    let r = roots(curve_surface_roots(&line, 0.0, 3.0, &plane, &tol()));
    assert_eq!(r.len(), 1);
    assert!(r[0].t.abs() < 1e-9);
}

fn line_seg(o: DVec3, d: DVec3, len: f64) -> Seg {
    Seg::new(Curve::Line { origin: o, dir: d.normalize() }, 0.0, len)
}

#[test]
fn crossing_lines_meet_once() {
    let a = line_seg(DVec3::new(-1.0, 0.0, 0.0), DVec3::X, 2.0);
    let b = line_seg(DVec3::new(0.0, -1.0, 0.0), DVec3::Y, 2.0);
    let r = seg_intersections(&a, &b, &tol());
    assert_eq!(r.len(), 1, "{r:?}");
    assert!((r[0].0 - 0.5).abs() < 1e-6 && (r[0].1 - 0.5).abs() < 1e-6);
}

#[test]
fn line_and_circle_meet_twice() {
    let circle = Seg::new(Curve::Circle { frame: Frame::WORLD, radius: 1.0 }, 0.0, TAU);
    let line = line_seg(DVec3::new(-2.0, 0.0, 0.0), DVec3::X, 4.0);
    let r = seg_intersections(&line, &circle, &tol());
    assert_eq!(r.len(), 2, "{r:?}");
}

#[test]
fn two_circles_meet_twice() {
    let a = Seg::new(Curve::Circle { frame: Frame::WORLD, radius: 1.0 }, 0.0, TAU);
    let f = Frame { origin: DVec3::new(1.0, 0.0, 0.0), ..Frame::WORLD };
    let b = Seg::new(Curve::Circle { frame: f, radius: 1.0 }, 0.0, TAU);
    assert_eq!(seg_intersections(&a, &b, &tol()).len(), 2);
}

#[test]
fn disjoint_segments_do_not_meet() {
    let a = line_seg(DVec3::ZERO, DVec3::X, 1.0);
    let b = line_seg(DVec3::new(0.0, 1.0, 0.0), DVec3::X, 1.0);
    assert!(seg_intersections(&a, &b, &tol()).is_empty());
}

#[test]
fn overlapping_collinear_segments_report_the_overlap_ends() {
    let a = line_seg(DVec3::ZERO, DVec3::X, 4.0);
    let b = line_seg(DVec3::new(2.0, 0.0, 0.0), DVec3::X, 4.0);
    let r = seg_intersections(&a, &b, &tol());
    assert!(r.len() >= 2, "{r:?}");
    let fa: Vec<f64> = r.iter().map(|x| x.0).collect();
    assert!(fa.iter().any(|f| (f - 0.5).abs() < 1e-3), "Überlappungsanfang bei 2/4: {fa:?}");
    assert!(fa.iter().any(|f| (f - 1.0).abs() < 1e-3), "Überlappungsende bei 4/4: {fa:?}");
}

#[test]
fn seg_closest_finds_nearest_point() {
    let c = Seg::new(Curve::Circle { frame: Frame::WORLD, radius: 2.0 }, 0.0, PI);
    let (f, d) = c.closest(DVec3::new(0.0, 5.0, 0.0));
    assert!((f - 0.5).abs() < 1e-6 && (d - 3.0).abs() < 1e-6);
    // Punkt hinter dem Bogenende: nächster Punkt ist ein Endpunkt
    let (f2, d2) = c.closest(DVec3::new(0.0, -2.5, 0.0));
    assert!(f2 < 1e-6 || (f2 - 1.0).abs() < 1e-6);
    assert!(d2 > 0.4);
}

#[test]
fn reverse_loop_flips_order_and_direction() {
    let a = line_seg(DVec3::ZERO, DVec3::X, 1.0);
    let b = line_seg(DVec3::new(1.0, 0.0, 0.0), DVec3::Y, 1.0);
    let r = reverse_loop(&[a.clone(), b.clone()]);
    assert!(r[0].start().distance(b.end()) < 1e-12);
    assert!(r[1].end().distance(a.start()) < 1e-12);
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test roots`
Expected: FAIL (Modul `boolean` fehlt).

- [ ] **Step 3: Implementieren**

`crates/planora-kernel/src/lib.rs` ergänzen: `pub mod boolean;`

`crates/planora-kernel/src/boolean/mod.rs` (wird in späteren Tasks erweitert):

```rust
//! Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit analytischen Flächen.

mod error;
mod roots;
mod seg;

pub use error::{BoolOp, BooleanError};
pub use roots::{CurveSurface, Root, curve_surface_roots, seg_intersections};
pub use seg::{Seg, reverse_loop};
```

`crates/planora-kernel/src/boolean/error.rs`:

```rust
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum BooleanError {
    #[error("Der Schnitt von {a} mit {b} wird noch nicht unterstützt")]
    UnsupportedIntersection { a: &'static str, b: &'static str },
    #[error("Die Körper berühren sich so, dass die Toleranz keine sichere Entscheidung zulässt")]
    AmbiguousContact,
    #[error("Die Lage eines Flächenstücks zum anderen Körper ließ sich nicht sicher bestimmen")]
    AmbiguousClassification,
    #[error("Das Ergebnis ist nicht geschlossen")]
    NotClosed,
    #[error("Das Ergebnis ist ungültig: {0}")]
    ResultInvalid(String),
    #[error("Das Ergebnis ist leer")]
    EmptyResult,
    #[error("Ein Eingangskörper ist ungültig: {0}")]
    InvalidInput(String),
}
```

`crates/planora-kernel/src/boolean/seg.rs`:

```rust
use crate::geometry::Curve;
use planora_math::{DVec3, TAU};

/// Orientiertes Stück einer 3D-Kurve: Parameter `t0 → t1` (auch absteigend erlaubt).
#[derive(Clone, Debug)]
pub struct Seg {
    pub curve: Curve,
    pub t0: f64,
    pub t1: f64,
}

impl Seg {
    pub fn new(curve: Curve, t0: f64, t1: f64) -> Self {
        Self { curve, t0, t1 }
    }

    /// Kurvenparameter am Bruchteil `f` ∈ [0, 1] des Stücks.
    pub fn param(&self, f: f64) -> f64 {
        self.t0 + (self.t1 - self.t0) * f
    }

    pub fn at(&self, f: f64) -> DVec3 {
        self.curve.point(self.param(f))
    }

    pub fn start(&self) -> DVec3 {
        self.at(0.0)
    }

    pub fn end(&self) -> DVec3 {
        self.at(1.0)
    }

    pub fn reversed(&self) -> Seg {
        Seg { curve: self.curve.clone(), t0: self.t1, t1: self.t0 }
    }

    /// Teilstück zwischen den Bruchteilen `f0` und `f1`.
    pub fn sub(&self, f0: f64, f1: f64) -> Seg {
        Seg { curve: self.curve.clone(), t0: self.param(f0), t1: self.param(f1) }
    }

    /// Tangente in Laufrichtung des Stücks (nicht normiert).
    pub fn tangent_at(&self, f: f64) -> DVec3 {
        let d = self.curve.tangent(self.param(f));
        if self.t1 >= self.t0 { d } else { -d }
    }

    /// Länge, aus einem Streckenzug mit 48 Abschnitten geschätzt.
    pub fn length(&self) -> f64 {
        let n = 48;
        (0..n)
            .map(|i| self.at(i as f64 / n as f64).distance(self.at((i + 1) as f64 / n as f64)))
            .sum()
    }

    /// Nächster Punkt des Stücks zu `p`: `(Bruchteil f, Abstand)`.
    pub fn closest(&self, p: DVec3) -> (f64, f64) {
        if let Curve::Line { origin, dir } = &self.curve {
            let (lo, hi) = (self.t0.min(self.t1), self.t0.max(self.t1));
            let t = (p - *origin).dot(*dir).clamp(lo, hi);
            let span = self.t1 - self.t0;
            let f = if span.abs() < 1e-300 { 0.0 } else { (t - self.t0) / span };
            return (f, (self.curve.point(t) - p).length());
        }
        let n = (((self.t1 - self.t0).abs() / (TAU / 96.0)).ceil() as usize).max(8);
        let mut best = (0usize, f64::MAX);
        for i in 0..=n {
            let d = (self.at(i as f64 / n as f64) - p).length();
            if d < best.1 {
                best = (i, d);
            }
        }
        let lo = best.0.saturating_sub(1) as f64 / n as f64;
        let hi = (best.0 + 1).min(n) as f64 / n as f64;
        golden_min(lo, hi, |f| (self.at(f) - p).length())
    }
}

/// Dreht eine Schleife um: Reihenfolge umkehren und jedes Stück umdrehen.
pub fn reverse_loop(lp: &[Seg]) -> Vec<Seg> {
    lp.iter().rev().map(Seg::reversed).collect()
}

/// Goldener Schnitt: Minimum von `f` auf `[a, b]` → `(x, f(x))`.
pub(crate) fn golden_min(mut a: f64, mut b: f64, f: impl Fn(f64) -> f64) -> (f64, f64) {
    const R: f64 = 0.618_033_988_749_894_9;
    let mut c = b - R * (b - a);
    let mut d = a + R * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..80 {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - R * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + R * (b - a);
            fd = f(d);
        }
    }
    let x = 0.5 * (a + b);
    (x, f(x))
}
```

`crates/planora-kernel/src/boolean/roots.rs`:

```rust
use super::seg::{Seg, golden_min};
use crate::geometry::{Curve, Surface};
use planora_math::{TAU, Tolerance};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Root {
    pub t: f64,
    /// Nur Berührung (kein Vorzeichenwechsel).
    pub touching: bool,
}

#[derive(Clone, Debug)]
pub enum CurveSurface {
    Roots(Vec<Root>),
    /// Die ganze Kurve liegt (innerhalb der Toleranz) auf der Fläche.
    Lies,
}

fn bisect(mut a: f64, mut b: f64, g: &impl Fn(f64) -> f64) -> f64 {
    let (mut ga, _) = (g(a), g(b));
    for _ in 0..200 {
        let m = 0.5 * (a + b);
        let gm = g(m);
        if gm == 0.0 || (b - a).abs() < 1e-15 {
            return m;
        }
        if ga * gm < 0.0 {
            b = m;
        } else {
            a = m;
            ga = gm;
        }
    }
    0.5 * (a + b)
}

/// Schnittpunkte der Kurve auf `[t0, t1]` mit einer Fläche (Nullstellen von `F(c(t))`).
pub fn curve_surface_roots(
    curve: &Curve,
    t0: f64,
    t1: f64,
    surface: &Surface,
    tol: &Tolerance,
) -> CurveSurface {
    let n = (((t1 - t0).abs() / (TAU / 64.0)).ceil() as usize).max(64);
    let g = |t: f64| surface.implicit(curve.point(t));
    let ts: Vec<f64> = (0..=n).map(|i| t0 + (t1 - t0) * i as f64 / n as f64).collect();
    let gs: Vec<f64> = ts.iter().map(|&t| g(t)).collect();
    if gs.iter().all(|v| v.abs() <= tol.linear) {
        return CurveSurface::Lies;
    }

    let mut roots: Vec<Root> = Vec::new();
    let add = |roots: &mut Vec<Root>, t: f64, touching: bool| {
        if !roots.iter().any(|r| (r.t - t).abs() < 1e-7) {
            roots.push(Root { t, touching });
        }
    };

    // Vorzeichenwechsel
    for i in 0..n {
        if gs[i] * gs[i + 1] < 0.0 {
            add(&mut roots, bisect(ts[i], ts[i + 1], &g), false);
        } else if gs[i] == 0.0 && i > 0 && gs[i - 1] * gs[i + 1] < 0.0 {
            add(&mut roots, ts[i], false);
        }
    }
    // Endpunkte, die innerhalb der Toleranz auf der Fläche liegen
    for &i in &[0usize, n] {
        if gs[i].abs() <= tol.linear {
            add(&mut roots, ts[i], false);
        }
    }
    // Berührungen: lokale Minima von |g| ohne Vorzeichenwechsel
    for i in 1..n {
        let (p, c, q) = (gs[i - 1].abs(), gs[i].abs(), gs[i + 1].abs());
        if c <= p && c <= q && gs[i - 1] * gs[i + 1] >= 0.0 {
            let (t, m) = golden_min(ts[i - 1], ts[i + 1], |t| g(t).abs());
            if m <= tol.linear && !roots.iter().any(|r| (r.t - t).abs() < 1e-6) {
                add(&mut roots, t, true);
            }
        }
    }
    roots.sort_by(|a, b| a.t.total_cmp(&b.t));
    CurveSurface::Roots(roots)
}

/// Schnittpunkte zweier Kurvenstücke als Bruchteile `(f_a, f_b)`. Überlappen die Stücke
/// auf einer Strecke, werden die Enden der Überlappung geliefert.
pub fn seg_intersections(a: &Seg, b: &Seg, tol: &Tolerance) -> Vec<(f64, f64)> {
    let n = (((a.t1 - a.t0).abs() / (TAU / 64.0)).ceil() as usize).max(64);
    let dist = |f: f64| b.closest(a.at(f)).1;
    let ds: Vec<f64> = (0..=n).map(|i| dist(i as f64 / n as f64)).collect();
    let near = tol.linear;
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut push = |out: &mut Vec<(f64, f64)>, fa: f64| {
        let fb = b.closest(a.at(fa)).0;
        if !out.iter().any(|(x, _)| (x - fa).abs() < 1e-6) {
            out.push((fa, fb));
        }
    };

    // Überlappungsstrecken: Läufe von Stützstellen, die der anderen Kurve näher als tol liegen
    let mut i = 0;
    while i <= n {
        if ds[i] <= near {
            let mut j = i;
            while j < n && ds[j + 1] <= near {
                j += 1;
            }
            if j - i >= 2 {
                // Ende des Laufs verfeinern (Übergang zu > tol suchen)
                let lo = if i > 0 { bisect_dist(&dist, (i - 1) as f64 / n as f64, i as f64 / n as f64, near) } else { 0.0 };
                let hi = if j < n { bisect_dist(&dist, (j + 1) as f64 / n as f64, j as f64 / n as f64, near) } else { 1.0 };
                push(&mut out, lo);
                push(&mut out, hi);
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    // einzelne Schnittpunkte: lokale Minima des Abstands
    for i in 0..=n {
        let prev = if i > 0 { ds[i - 1] } else { f64::MAX };
        let next = if i < n { ds[i + 1] } else { f64::MAX };
        if ds[i] <= prev && ds[i] <= next && ds[i] < (a.length() / n as f64) * 2.0 + near {
            let lo = if i > 0 { (i - 1) as f64 / n as f64 } else { 0.0 };
            let hi = if i < n { (i + 1) as f64 / n as f64 } else { 1.0 };
            let (fa, m) = golden_min(lo, hi, &dist);
            if m <= near && !out.iter().any(|(x, _)| (x - fa).abs() < 1e-4) {
                push(&mut out, fa);
            }
        }
    }
    out.sort_by(|x, y| x.0.total_cmp(&y.0));
    out
}

/// Findet den Übergang des Abstands durch `level` zwischen `inside` (≤ level) und `outside` (> level).
fn bisect_dist(dist: &impl Fn(f64) -> f64, outside: f64, inside: f64, level: f64) -> f64 {
    let (mut a, mut b) = (outside, inside);
    for _ in 0..60 {
        let m = 0.5 * (a + b);
        if dist(m) <= level {
            b = m;
        } else {
            a = m;
        }
    }
    b
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel --test roots` und `cargo clippy -p planora-kernel --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/planora-kernel
git commit -m "feat(kernel): Kurvenstücke, Kurve-Fläche-Nullstellen und Stück-Schnitte" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Flächen-Flächen-Schnitte (Stufe A)

**Files:**
- Create: `crates/planora-kernel/src/boolean/surfsurf.rs`
- Modify: `crates/planora-kernel/src/boolean/mod.rs`
- Test: `crates/planora-kernel/tests/surfsurf.rs`

**Interfaces:**
- Consumes: `Surface`, `Curve`, `Frame`, `Tolerance`.
- Produces: `SurfaceIntersection::{None, Touch, Coincident, Curves(Vec<Curve>), Unsupported}`, `intersect_surfaces(&Surface, &Surface, &Tolerance) -> SurfaceIntersection` (symmetrisch in den Argumenten), `surface_name(&Surface) -> &'static str` ("Ebene", "Zylinder", "Kegel", "Kugel", "Torus"). Zurückgegebene Kurven sind vollständig (Gerade unbeschränkt, Kreis/Ellipse mit Parameter 0..2π); der Beschnitt folgt in Task 6.

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/surfsurf.rs`

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn plane(o: DVec3, n: DVec3) -> Surface {
    Surface::Plane { frame: Frame::from_origin_normal(o, n) }
}

fn cyl(o: DVec3, axis: DVec3, r: f64) -> Surface {
    Surface::Cylinder { frame: Frame::from_origin_normal(o, axis), radius: r }
}

fn cone(o: DVec3, axis: DVec3, r: f64, slope: f64) -> Surface {
    Surface::Cone { frame: Frame::from_origin_normal(o, axis), radius: r, slope }
}

fn sphere(c: DVec3, r: f64) -> Surface {
    Surface::Sphere { frame: Frame::from_origin_normal(c, DVec3::Z), radius: r }
}

fn torus(o: DVec3, axis: DVec3, major: f64, minor: f64) -> Surface {
    Surface::Torus { frame: Frame::from_origin_normal(o, axis), major, minor }
}

fn curves(r: SurfaceIntersection) -> Vec<Curve> {
    match r {
        SurfaceIntersection::Curves(c) => c,
        other => panic!("Kurven erwartet, bekam {other:?}"),
    }
}

/// Alle abgetasteten Punkte der Kurven liegen auf beiden Flächen. `ts` bestimmt den Bereich bei Geraden.
fn check_on_both(cs: &[Curve], a: &Surface, b: &Surface, line_range: (f64, f64)) {
    for c in cs {
        let n = 24;
        for i in 0..n {
            let t = match c {
                Curve::Line { .. } => line_range.0 + (line_range.1 - line_range.0) * i as f64 / n as f64,
                _ => TAU * i as f64 / n as f64,
            };
            let p = c.point(t);
            assert!(a.implicit(p).abs() < 1e-8, "{c:?} t={t} nicht auf A: {}", a.implicit(p));
            assert!(b.implicit(p).abs() < 1e-8, "{c:?} t={t} nicht auf B: {}", b.implicit(p));
        }
    }
}

fn both_orders(a: &Surface, b: &Surface) -> (SurfaceIntersection, SurfaceIntersection) {
    (intersect_surfaces(a, b, &tol()), intersect_surfaces(b, a, &tol()))
}

#[test]
fn plane_plane() {
    let a = plane(DVec3::ZERO, DVec3::Z);
    let b = plane(DVec3::new(1.0, 0.0, 0.0), DVec3::X);
    let (ab, ba) = both_orders(&a, &b);
    let c = curves(ab);
    assert_eq!(c.len(), 1);
    check_on_both(&c, &a, &b, (-5.0, 5.0));
    assert_eq!(curves(ba).len(), 1);
    // parallel getrennt, identisch
    let p2 = plane(DVec3::new(0.0, 0.0, 1.0), DVec3::Z);
    assert!(matches!(intersect_surfaces(&a, &p2, &tol()), SurfaceIntersection::None));
    assert!(matches!(intersect_surfaces(&a, &a, &tol()), SurfaceIntersection::Coincident));
    // entgegengesetzte Normale, gleiche Ebene
    let flipped = plane(DVec3::ZERO, -DVec3::Z);
    assert!(matches!(intersect_surfaces(&a, &flipped, &tol()), SurfaceIntersection::Coincident));
}

#[test]
fn plane_cylinder_all_orientations() {
    let cy = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    // Achse senkrecht zur Ebene → Kreis
    let p = plane(DVec3::new(0.0, 0.0, 3.0), DVec3::Z);
    let (ab, ba) = both_orders(&p, &cy);
    let c = curves(ab);
    assert_eq!(c.len(), 1);
    assert!(matches!(&c[0], Curve::Circle { radius, .. } if (radius - 2.0).abs() < 1e-9));
    check_on_both(&c, &p, &cy, (0.0, 0.0));
    assert_eq!(curves(ba).len(), 1);
    // Achse parallel zur Ebene → zwei Geraden
    let pv = plane(DVec3::new(1.0, 0.0, 0.0), DVec3::X);
    let c = curves(intersect_surfaces(&pv, &cy, &tol()));
    assert_eq!(c.len(), 2);
    assert!(c.iter().all(|k| matches!(k, Curve::Line { .. })));
    check_on_both(&c, &pv, &cy, (-4.0, 4.0));
    // tangential und weit weg
    let tan = plane(DVec3::new(2.0, 0.0, 0.0), DVec3::X);
    assert!(matches!(intersect_surfaces(&tan, &cy, &tol()), SurfaceIntersection::Touch));
    let far = plane(DVec3::new(5.0, 0.0, 0.0), DVec3::X);
    assert!(matches!(intersect_surfaces(&far, &cy, &tol()), SurfaceIntersection::None));
    // schräg → Ellipse mit großer Halbachse r / cos(θ)
    let n = DVec3::new(0.0, 0.5, 1.0).normalize();
    let obl = plane(DVec3::ZERO, n);
    let c = curves(intersect_surfaces(&obl, &cy, &tol()));
    assert_eq!(c.len(), 1);
    let Curve::Ellipse { major, minor, .. } = &c[0] else { panic!("Ellipse erwartet") };
    assert!((minor - 2.0).abs() < 1e-9);
    assert!((major - 2.0 / n.z).abs() < 1e-9);
    check_on_both(&c, &obl, &cy, (0.0, 0.0));
}

#[test]
fn plane_cone_circle_and_generators() {
    let co = cone(DVec3::ZERO, DVec3::Z, 3.0, -0.5); // Spitze bei z = 6
    let p = plane(DVec3::new(0.0, 0.0, 2.0), DVec3::Z);
    let c = curves(intersect_surfaces(&p, &co, &tol()));
    assert_eq!(c.len(), 1);
    assert!(matches!(&c[0], Curve::Circle { radius, .. } if (radius - 2.0).abs() < 1e-9));
    check_on_both(&c, &p, &co, (0.0, 0.0));
    // Ebene enthält die Achse → zwei Erzeugende (nur der Ast mit positivem Radius wird geprüft)
    let pa = plane(DVec3::ZERO, DVec3::Y);
    let c = curves(intersect_surfaces(&pa, &co, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &pa, &co, (0.0, 2.0));
    // schräge Ebene: noch nicht unterstützt
    let obl = plane(DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.3, 0.0, 1.0));
    assert!(matches!(intersect_surfaces(&obl, &co, &tol()), SurfaceIntersection::Unsupported));
}

#[test]
fn plane_sphere() {
    let s = sphere(DVec3::new(1.0, 2.0, 3.0), 5.0);
    let p = plane(DVec3::new(0.0, 0.0, 6.0), DVec3::new(0.0, 0.0, 1.0));
    let c = curves(intersect_surfaces(&p, &s, &tol()));
    assert!(matches!(&c[0], Curve::Circle { radius, .. } if (radius - 4.0).abs() < 1e-9));
    check_on_both(&c, &p, &s, (0.0, 0.0));
    let tangent = plane(DVec3::new(0.0, 0.0, 8.0), DVec3::Z);
    assert!(matches!(intersect_surfaces(&tangent, &s, &tol()), SurfaceIntersection::Touch));
    let far = plane(DVec3::new(0.0, 0.0, 20.0), DVec3::Z);
    assert!(matches!(intersect_surfaces(&far, &s, &tol()), SurfaceIntersection::None));
    // beliebig geneigte Ebene durch die Kugel
    let tilted = plane(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 1.0, 1.0));
    let c = curves(intersect_surfaces(&tilted, &s, &tol()));
    check_on_both(&c, &tilted, &s, (0.0, 0.0));
}

#[test]
fn plane_torus() {
    let t = torus(DVec3::ZERO, DVec3::Z, 5.0, 1.5);
    let p = plane(DVec3::new(0.0, 0.0, 0.5), DVec3::Z);
    let c = curves(intersect_surfaces(&p, &t, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &p, &t, (0.0, 0.0));
    let through = plane(DVec3::ZERO, DVec3::Y);
    let c = curves(intersect_surfaces(&through, &t, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &through, &t, (0.0, 0.0));
    let obl = plane(DVec3::ZERO, DVec3::new(0.3, 0.0, 1.0));
    assert!(matches!(intersect_surfaces(&obl, &t, &tol()), SurfaceIntersection::Unsupported));
}

#[test]
fn sphere_sphere() {
    let a = sphere(DVec3::ZERO, 3.0);
    let b = sphere(DVec3::new(4.0, 0.0, 0.0), 2.0);
    let c = curves(intersect_surfaces(&a, &b, &tol()));
    assert_eq!(c.len(), 1);
    check_on_both(&c, &a, &b, (0.0, 0.0));
    assert!(matches!(intersect_surfaces(&a, &sphere(DVec3::new(5.0, 0.0, 0.0), 2.0), &tol()), SurfaceIntersection::Touch));
    assert!(matches!(intersect_surfaces(&a, &sphere(DVec3::new(9.0, 0.0, 0.0), 2.0), &tol()), SurfaceIntersection::None));
    assert!(matches!(intersect_surfaces(&a, &a, &tol()), SurfaceIntersection::Coincident));
    assert!(matches!(intersect_surfaces(&a, &sphere(DVec3::ZERO, 1.0), &tol()), SurfaceIntersection::None));
}

#[test]
fn coaxial_surfaces_intersect_in_circles() {
    let cy = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    let sp = sphere(DVec3::new(0.0, 0.0, 1.0), 3.0);
    let c = curves(intersect_surfaces(&cy, &sp, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &cy, &sp, (0.0, 0.0));
    let co = cone(DVec3::ZERO, DVec3::Z, 4.0, -0.5);
    let c = curves(intersect_surfaces(&cy, &co, &tol()));
    assert_eq!(c.len(), 1);
    check_on_both(&c, &cy, &co, (0.0, 0.0));
    let c = curves(intersect_surfaces(&co, &sp, &tol()));
    check_on_both(&c, &co, &sp, (0.0, 0.0));
    let to = torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 5.0, 1.5);
    let c = curves(intersect_surfaces(&cy, &torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0), &tol()));
    check_on_both(&c, &cy, &torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0), (0.0, 0.0));
    let c = curves(intersect_surfaces(&sp, &torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0), &tol()));
    check_on_both(&c, &sp, &torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0), (0.0, 0.0));
    let c = curves(intersect_surfaces(&to, &torus(DVec3::new(0.0, 0.0, 2.0), DVec3::Z, 5.0, 1.5), &tol()));
    check_on_both(&c, &to, &torus(DVec3::new(0.0, 0.0, 2.0), DVec3::Z, 5.0, 1.5), (0.0, 0.0));
    // gleiche Zylinder / unterschiedliche Radien
    assert!(matches!(intersect_surfaces(&cy, &cy, &tol()), SurfaceIntersection::Coincident));
    assert!(matches!(intersect_surfaces(&cy, &cyl(DVec3::ZERO, DVec3::Z, 3.0), &tol()), SurfaceIntersection::None));
    // umgekehrte Achsrichtung ändert nichts
    let cy_down = cyl(DVec3::ZERO, -DVec3::Z, 2.0);
    let c = curves(intersect_surfaces(&cy_down, &sp, &tol()));
    check_on_both(&c, &cy_down, &sp, (0.0, 0.0));
}

#[test]
fn parallel_cylinders_give_lines() {
    let a = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    let b = cyl(DVec3::new(3.0, 0.0, 0.0), DVec3::Z, 2.0);
    let c = curves(intersect_surfaces(&a, &b, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &a, &b, (-3.0, 3.0));
    assert!(matches!(
        intersect_surfaces(&a, &cyl(DVec3::new(4.0, 0.0, 0.0), DVec3::Z, 2.0), &tol()),
        SurfaceIntersection::Touch
    ));
    assert!(matches!(
        intersect_surfaces(&a, &cyl(DVec3::new(9.0, 0.0, 0.0), DVec3::Z, 2.0), &tol()),
        SurfaceIntersection::None
    ));
}

#[test]
fn unsupported_pairs_are_reported() {
    let a = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    let b = cyl(DVec3::ZERO, DVec3::X, 2.0);
    assert!(matches!(intersect_surfaces(&a, &b, &tol()), SurfaceIntersection::Unsupported));
    let off_sphere = sphere(DVec3::new(0.5, 0.0, 0.0), 3.0);
    assert!(matches!(intersect_surfaces(&a, &off_sphere, &tol()), SurfaceIntersection::Unsupported));
}

#[test]
fn surface_names_are_german() {
    assert_eq!(surface_name(&plane(DVec3::ZERO, DVec3::Z)), "Ebene");
    assert_eq!(surface_name(&sphere(DVec3::ZERO, 1.0)), "Kugel");
    assert_eq!(surface_name(&cyl(DVec3::ZERO, DVec3::Z, 1.0)), "Zylinder");
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test surfsurf`
Expected: FAIL (Kompilierfehler).

- [ ] **Step 3: Implementieren**

`mod.rs` ergänzen: `mod surfsurf;` und `pub use surfsurf::{SurfaceIntersection, intersect_surfaces, surface_name};`

`crates/planora-kernel/src/boolean/surfsurf.rs`:

```rust
use crate::geometry::{Curve, Surface};
use planora_math::{DVec3, Frame, Tolerance};

#[derive(Clone, Debug)]
pub enum SurfaceIntersection {
    /// Kein gemeinsamer Punkt.
    None,
    /// Nur Berührung ohne Durchdringung.
    Touch,
    /// Dieselbe Fläche (innerhalb der Toleranz).
    Coincident,
    Curves(Vec<Curve>),
    /// Paar außerhalb von Stufe A.
    Unsupported,
}

const ANG: f64 = 1e-9;

pub fn surface_name(s: &Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "Ebene",
        Surface::Cylinder { .. } => "Zylinder",
        Surface::Cone { .. } => "Kegel",
        Surface::Sphere { .. } => "Kugel",
        Surface::Torus { .. } => "Torus",
    }
}

fn line(origin: DVec3, dir: DVec3) -> Curve {
    Curve::Line { origin, dir: dir.normalize() }
}

fn circle(center: DVec3, normal: DVec3, radius: f64) -> Curve {
    Curve::Circle { frame: Frame::from_origin_normal(center, normal), radius }
}

/// Schnitt zweier Flächen (symmetrisch in den Argumenten).
pub fn intersect_surfaces(a: &Surface, b: &Surface, tol: &Tolerance) -> SurfaceIntersection {
    use Surface::*;
    match (a, b) {
        (Plane { frame: fa }, Plane { frame: fb }) => plane_plane(fa, fb, tol),
        (Plane { frame: p }, Cylinder { frame, radius })
        | (Cylinder { frame, radius }, Plane { frame: p }) => plane_cylinder(p, frame, *radius, tol),
        (Plane { frame: p }, Cone { frame, radius, slope })
        | (Cone { frame, radius, slope }, Plane { frame: p }) => {
            plane_cone(p, frame, *radius, *slope, tol)
        }
        (Plane { frame: p }, Sphere { frame, radius })
        | (Sphere { frame, radius }, Plane { frame: p }) => {
            plane_sphere(p, frame.origin, *radius, tol)
        }
        (Plane { frame: p }, Torus { frame, major, minor })
        | (Torus { frame, major, minor }, Plane { frame: p }) => {
            plane_torus(p, frame, *major, *minor, tol)
        }
        (Sphere { frame: fa, radius: ra }, Sphere { frame: fb, radius: rb }) => {
            sphere_sphere(fa.origin, *ra, fb.origin, *rb, tol)
        }
        (Cylinder { frame: fa, radius: ra }, Cylinder { frame: fb, radius: rb }) => {
            if coaxial(fa, fb, tol) {
                revolve_pair(a, b, tol)
            } else if fa.z.cross(fb.z).length() < ANG {
                cylinder_cylinder_parallel(fa, *ra, fb, *rb, tol)
            } else {
                SurfaceIntersection::Unsupported
            }
        }
        _ => revolve_pair(a, b, tol),
    }
}

fn plane_plane(f1: &Frame, f2: &Frame, tol: &Tolerance) -> SurfaceIntersection {
    let d = f1.z.cross(f2.z);
    if d.length() < ANG {
        let dist = (f2.origin - f1.origin).dot(f1.z);
        return if dist.abs() <= tol.linear {
            SurfaceIntersection::Coincident
        } else {
            SurfaceIntersection::None
        };
    }
    let c1 = f1.z.dot(f1.origin);
    let c2 = f2.z.dot(f2.origin);
    let p = (f2.z.cross(d) * c1 + d.cross(f1.z) * c2) / d.dot(d);
    SurfaceIntersection::Curves(vec![line(p, d)])
}

fn plane_cylinder(pf: &Frame, cf: &Frame, r: f64, tol: &Tolerance) -> SurfaceIntersection {
    let n = pf.z;
    let a = cf.z;
    let cos = n.dot(a);
    let dist = n.dot(cf.origin - pf.origin);
    if (cos.abs() - 1.0).abs() < ANG {
        let center = cf.origin + a * (-dist / cos);
        return SurfaceIntersection::Curves(vec![circle(center, n, r)]);
    }
    if cos.abs() < ANG {
        let h = dist.abs();
        if h > r + tol.linear {
            return SurfaceIntersection::None;
        }
        if (h - r).abs() <= tol.linear {
            return SurfaceIntersection::Touch;
        }
        let foot = cf.origin - n * dist;
        let m = a.cross(n).normalize();
        let w = (r * r - h * h).sqrt();
        return SurfaceIntersection::Curves(vec![line(foot + m * w, a), line(foot - m * w, a)]);
    }
    let center = cf.origin + a * (-dist / cos);
    let x = (a - n * cos).normalize();
    let y = n.cross(x);
    let frame = Frame { origin: center, x, y, z: n };
    SurfaceIntersection::Curves(vec![Curve::Ellipse { frame, major: r / cos.abs(), minor: r }])
}

fn plane_cone(pf: &Frame, cf: &Frame, radius: f64, slope: f64, tol: &Tolerance) -> SurfaceIntersection {
    let n = pf.z;
    let a = cf.z;
    let cos = n.dot(a);
    let dist = n.dot(cf.origin - pf.origin);
    if (cos.abs() - 1.0).abs() < ANG {
        let h = -dist / cos;
        let rho = radius + slope * h;
        if rho <= tol.linear {
            return SurfaceIntersection::None;
        }
        return SurfaceIntersection::Curves(vec![circle(cf.origin + a * h, n, rho)]);
    }
    if cos.abs() < ANG && dist.abs() <= tol.linear {
        let m = a.cross(n).normalize();
        return SurfaceIntersection::Curves(vec![
            line(cf.origin + m * radius, a + m * slope),
            line(cf.origin - m * radius, a - m * slope),
        ]);
    }
    SurfaceIntersection::Unsupported
}

fn plane_sphere(pf: &Frame, c: DVec3, r: f64, tol: &Tolerance) -> SurfaceIntersection {
    let d = pf.z.dot(c - pf.origin);
    if d.abs() > r + tol.linear {
        return SurfaceIntersection::None;
    }
    if (d.abs() - r).abs() <= tol.linear {
        return SurfaceIntersection::Touch;
    }
    SurfaceIntersection::Curves(vec![circle(c - pf.z * d, pf.z, (r * r - d * d).sqrt())])
}

fn plane_torus(pf: &Frame, tf: &Frame, major: f64, minor: f64, tol: &Tolerance) -> SurfaceIntersection {
    let n = pf.z;
    let a = tf.z;
    let cos = n.dot(a);
    if (cos.abs() - 1.0).abs() < ANG {
        let h = (pf.origin - tf.origin).dot(a);
        if h.abs() > minor + tol.linear {
            return SurfaceIntersection::None;
        }
        if (h.abs() - minor).abs() <= tol.linear {
            return SurfaceIntersection::Touch;
        }
        let w = (minor * minor - h * h).sqrt();
        let center = tf.origin + a * h;
        let mut cs = vec![circle(center, n, major + w)];
        if major - w > tol.linear {
            cs.push(circle(center, n, major - w));
        }
        return SurfaceIntersection::Curves(cs);
    }
    if cos.abs() < ANG && n.dot(tf.origin - pf.origin).abs() <= tol.linear {
        let m = a.cross(n).normalize();
        return SurfaceIntersection::Curves(vec![
            circle(tf.origin + m * major, n, minor),
            circle(tf.origin - m * major, n, minor),
        ]);
    }
    SurfaceIntersection::Unsupported
}

fn sphere_sphere(c1: DVec3, r1: f64, c2: DVec3, r2: f64, tol: &Tolerance) -> SurfaceIntersection {
    let d = (c2 - c1).length();
    if d <= tol.linear {
        return if (r1 - r2).abs() <= tol.linear {
            SurfaceIntersection::Coincident
        } else {
            SurfaceIntersection::None
        };
    }
    if d > r1 + r2 + tol.linear || d < (r1 - r2).abs() - tol.linear {
        return SurfaceIntersection::None;
    }
    if (d - (r1 + r2)).abs() <= tol.linear || (d - (r1 - r2).abs()).abs() <= tol.linear {
        return SurfaceIntersection::Touch;
    }
    let n = (c2 - c1) / d;
    let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
    SurfaceIntersection::Curves(vec![circle(c1 + n * x, n, (r1 * r1 - x * x).sqrt())])
}

fn cylinder_cylinder_parallel(f1: &Frame, r1: f64, f2: &Frame, r2: f64, tol: &Tolerance) -> SurfaceIntersection {
    let a = f1.z;
    let c1 = f1.origin;
    let c2 = f2.origin - a * (f2.origin - f1.origin).dot(a);
    let dv = c2 - c1;
    let d = dv.length();
    if d <= tol.linear {
        return SurfaceIntersection::Unsupported; // koaxial: wird vorher behandelt
    }
    if d > r1 + r2 + tol.linear || d < (r1 - r2).abs() - tol.linear {
        return SurfaceIntersection::None;
    }
    if (d - (r1 + r2)).abs() <= tol.linear || (d - (r1 - r2).abs()).abs() <= tol.linear {
        return SurfaceIntersection::Touch;
    }
    let e = dv / d;
    let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
    let h = (r1 * r1 - x * x).sqrt();
    let w = a.cross(e);
    let q = c1 + e * x;
    SurfaceIntersection::Curves(vec![line(q + w * h, a), line(q - w * h, a)])
}

// ---- Drehflächen mit gemeinsamer Achse: Meridiankurven in der (ρ, h)-Halbebene ----

fn axis_of(s: &Surface) -> Option<(DVec3, DVec3)> {
    match s {
        Surface::Cylinder { frame, .. } | Surface::Cone { frame, .. } | Surface::Torus { frame, .. } => {
            Some((frame.origin, frame.z))
        }
        _ => None,
    }
}

fn dist_to_axis(p: DVec3, o: DVec3, z: DVec3) -> f64 {
    let d = p - o;
    (d - z * d.dot(z)).length()
}

fn coaxial(f1: &Frame, f2: &Frame, tol: &Tolerance) -> bool {
    f1.z.cross(f2.z).length() < ANG && dist_to_axis(f2.origin, f1.origin, f1.z) <= tol.linear
}

enum Mer {
    /// ρ = rho0 + slope · h
    Line { rho0: f64, slope: f64 },
    /// Kreis um (rc, hc) mit Radius r
    Circle { rc: f64, hc: f64, r: f64 },
}

fn meridian(s: &Surface, o: DVec3, z: DVec3) -> Mer {
    let f = s.frame();
    let h0 = (f.origin - o).dot(z);
    let sigma = if f.z.dot(z) >= 0.0 { 1.0 } else { -1.0 };
    match s {
        Surface::Cylinder { radius, .. } => Mer::Line { rho0: *radius, slope: 0.0 },
        Surface::Cone { radius, slope, .. } => {
            Mer::Line { rho0: radius - slope * sigma * h0, slope: slope * sigma }
        }
        Surface::Sphere { radius, .. } => Mer::Circle { rc: 0.0, hc: h0, r: *radius },
        Surface::Torus { major, minor, .. } => Mer::Circle { rc: *major, hc: h0, r: *minor },
        Surface::Plane { .. } => unreachable!("Ebenen werden vorher behandelt"),
    }
}

enum MerResult {
    None,
    Coincident,
    /// Punkte (ρ, h); `true` = alle Punkte sind Berührpunkte
    Points(Vec<(f64, f64)>, bool),
}

fn mer_intersect(a: &Mer, b: &Mer, tol: &Tolerance) -> MerResult {
    match (a, b) {
        (Mer::Line { rho0: ra, slope: sa }, Mer::Line { rho0: rb, slope: sb }) => {
            if (sa - sb).abs() < 1e-12 {
                return if (ra - rb).abs() <= tol.linear { MerResult::Coincident } else { MerResult::None };
            }
            let h = (rb - ra) / (sa - sb);
            MerResult::Points(vec![(ra + sa * h, h)], false)
        }
        (Mer::Line { rho0, slope }, Mer::Circle { rc, hc, r })
        | (Mer::Circle { rc, hc, r }, Mer::Line { rho0, slope }) => {
            let (s, k) = (*slope, rho0 - rc);
            let qa = s * s + 1.0;
            let qb = 2.0 * (k * s - hc);
            let qc = k * k + hc * hc - r * r;
            let disc = qb * qb - 4.0 * qa * qc;
            let scale = qb * qb + 1.0;
            if disc < -1e-12 * scale {
                return MerResult::None;
            }
            let pt = |h: f64| (rho0 + s * h, h);
            if disc.abs() <= 1e-12 * scale {
                return MerResult::Points(vec![pt(-qb / (2.0 * qa))], true);
            }
            let sq = disc.sqrt();
            MerResult::Points(vec![pt((-qb + sq) / (2.0 * qa)), pt((-qb - sq) / (2.0 * qa))], false)
        }
        (Mer::Circle { rc: c1, hc: h1, r: r1 }, Mer::Circle { rc: c2, hc: h2, r: r2 }) => {
            let (dx, dy) = (c2 - c1, h2 - h1);
            let d = (dx * dx + dy * dy).sqrt();
            if d <= tol.linear {
                return if (r1 - r2).abs() <= tol.linear { MerResult::Coincident } else { MerResult::None };
            }
            if d > r1 + r2 + tol.linear || d < (r1 - r2).abs() - tol.linear {
                return MerResult::None;
            }
            let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
            let hh = (r1 * r1 - x * x).max(0.0).sqrt();
            let (ex, ey) = (dx / d, dy / d);
            let base = (c1 + ex * x, h1 + ey * x);
            if hh <= tol.linear {
                return MerResult::Points(vec![base], true);
            }
            MerResult::Points(
                vec![(base.0 - ey * hh, base.1 + ex * hh), (base.0 + ey * hh, base.1 - ex * hh)],
                false,
            )
        }
    }
}

fn revolve_pair(a: &Surface, b: &Surface, tol: &Tolerance) -> SurfaceIntersection {
    // gemeinsame Achse bestimmen
    let (o, z) = match (axis_of(a), axis_of(b)) {
        (Some((oa, za)), Some((ob, zb))) => {
            if za.cross(zb).length() >= ANG || dist_to_axis(ob, oa, za) > tol.linear {
                return SurfaceIntersection::Unsupported;
            }
            (oa, za)
        }
        (Some((o, z)), None) => {
            if dist_to_axis(b.frame().origin, o, z) > tol.linear {
                return SurfaceIntersection::Unsupported;
            }
            (o, z)
        }
        (None, Some((o, z))) => {
            if dist_to_axis(a.frame().origin, o, z) > tol.linear {
                return SurfaceIntersection::Unsupported;
            }
            (o, z)
        }
        (None, None) => return SurfaceIntersection::Unsupported,
    };
    match mer_intersect(&meridian(a, o, z), &meridian(b, o, z), tol) {
        MerResult::None => SurfaceIntersection::None,
        MerResult::Coincident => SurfaceIntersection::Coincident,
        MerResult::Points(points, all_tangent) => {
            let circles: Vec<Curve> = points
                .into_iter()
                .filter(|(rho, _)| *rho > tol.linear)
                .map(|(rho, h)| circle(o + z * h, z, rho))
                .collect();
            if circles.is_empty() {
                SurfaceIntersection::None
            } else if all_tangent {
                SurfaceIntersection::Touch
            } else {
                SurfaceIntersection::Curves(circles)
            }
        }
    }
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel --test surfsurf` und clippy.
Expected: PASS. Hinweis: Der Test „coaxial“ nutzt einen Torus-Torus-Fall mit gleichem Haupt- und Nebenradius, aber verschobener Höhe; erwartet werden zwei Kreise (Meridiankreise schneiden sich in zwei Punkten).

- [ ] **Step 5: Commit**

```bash
git add crates/planora-kernel
git commit -m "feat(kernel): Flächen-Flächen-Schnitte für Stufe A" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Randschleifen im Parameterraum und Punkt-in-Fläche

**Files:**
- Create: `crates/planora-kernel/src/boolean/uv.rs`
- Modify: `crates/planora-kernel/src/boolean/mod.rs`
- Test: `crates/planora-kernel/tests/uv.rs`

**Interfaces:**
- Consumes: `Seg`, `Surface::{project, pole_vs, is_periodic}`, `Solid`, `Face`, `Id`.
- Produces:
  - `FaceGeom { surface: Surface, same_sense: bool, domain: Option<UvRect>, loops: Vec<Vec<Seg>> }` (`loops[0]` = äußere Schleife, danach Löcher), `face_geom(&Solid, Id<Face>) -> FaceGeom`
  - `loop_points(&[Seg]) -> Vec<DVec3>` (Abtastpunkte einer Schleife ohne Wiederholung des Anfangs)
  - `points_to_uv(&Surface, same_sense: bool, &[DVec3]) -> Vec<DVec2>` (Polygon im Parameterraum; Naht aufgerollt, Pole als Strecke) und `points_to_uv_indexed(...) -> Vec<(DVec2, Option<usize>)>` (zusätzlich der Index des Eingabepunkts, `None` für eingefügte Polpunkte)
  - `oriented_area(&[DVec2], same_sense: bool) -> f64` (positiv für Außenkonturen)
  - `winding(&[DVec2], DVec2) -> i32`
  - `Containment::{Inside, Boundary, Outside}`, `point_in_face(&FaceGeom, DVec3, &Tolerance) -> Containment`

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/uv.rs`

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn face_where(s: &Solid, f: impl Fn(&Face) -> bool) -> Id<Face> {
    s.faces.iter().find(|(_, face)| f(face)).map(|(id, _)| id).expect("Fläche nicht gefunden")
}

fn plane_face_at_z(s: &Solid, z: f64, up: bool) -> Id<Face> {
    face_where(s, |f| match &f.surface {
        Surface::Plane { frame } => {
            let n = if f.same_sense { frame.z } else { -frame.z };
            (frame.origin.z - z).abs() < 1e-9 && (n.z > 0.5) == up && n.z.abs() > 0.5
        }
        _ => false,
    })
}

#[test]
fn oriented_area_of_a_planar_face_is_positive() {
    let s = k().make_box(DVec3::new(10.0, 6.0, 4.0)).unwrap();
    for (id, f) in s.faces.iter() {
        let g = face_geom(&s, id);
        let outer = points_to_uv(&g.surface, g.same_sense, &loop_points(&g.loops[0]));
        assert!(oriented_area(&outer, g.same_sense) > 1.0, "{:?}", f.surface);
    }
}

#[test]
fn point_in_planar_face_with_hole() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let g = face_geom(&s, plane_face_at_z(&s, 5.0, true));
    assert_eq!(g.loops.len(), 2);
    assert_eq!(point_in_face(&g, DVec3::new(2.0, 2.0, 5.0), &tol()), Containment::Inside);
    assert_eq!(point_in_face(&g, DVec3::new(10.0, 10.0, 5.0), &tol()), Containment::Outside);
    assert_eq!(point_in_face(&g, DVec3::new(25.0, 10.0, 5.0), &tol()), Containment::Outside);
    assert_eq!(point_in_face(&g, DVec3::new(0.0, 7.0, 5.0), &tol()), Containment::Boundary);
    assert_eq!(point_in_face(&g, DVec3::new(13.0, 10.0, 5.0), &tol()), Containment::Boundary);
}

#[test]
fn point_in_a_cylinder_wall_across_the_seam() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let id = face_where(&s, |f| matches!(f.surface, Surface::Cylinder { .. }));
    let g = face_geom(&s, id);
    for angle in [0.1, 1.0, 3.0, 5.0, 6.2] {
        let p = DVec3::new(5.0 * f64::cos(angle), 5.0 * f64::sin(angle), 4.0);
        assert_eq!(point_in_face(&g, p, &tol()), Containment::Inside, "u = {angle}");
    }
    // oberhalb der Höhe: außerhalb der Wand (nicht auf der Fläche), Punkt auf dem Rand
    assert_eq!(point_in_face(&g, DVec3::new(5.0, 0.0, 4.0), &tol()), Containment::Boundary);
    assert_eq!(point_in_face(&g, DVec3::new(0.0, 5.0, 11.0), &tol()), Containment::Outside);
}

#[test]
fn point_in_a_sphere_face_with_poles() {
    let s = k().make_sphere(5.0).unwrap();
    let g = face_geom(&s, Id::<Face>::from_index(0));
    let area = {
        let poly = points_to_uv(&g.surface, g.same_sense, &loop_points(&g.loops[0]));
        oriented_area(&poly, g.same_sense)
    };
    assert!((area - TAU * PI).abs() / (TAU * PI) < 0.01, "UV-Fläche der ganzen Kugel: {area}");
    for (u, v) in [(0.3, 0.2), (2.0, -1.2), (5.5, 1.4), (3.1, 0.0)] {
        let p = g.surface.point(u, v);
        assert_eq!(point_in_face(&g, p, &tol()), Containment::Inside, "u={u} v={v}");
    }
}

#[test]
fn point_in_cone_and_torus_faces() {
    let cone = k().make_cone(3.0, 6.0).unwrap();
    let cid = face_where(&cone, |f| matches!(f.surface, Surface::Cone { .. }));
    let g = face_geom(&cone, cid);
    for angle in [0.4, 2.0, 4.5, 6.0] {
        let p = DVec3::new(1.5 * f64::cos(angle), 1.5 * f64::sin(angle), 3.0);
        assert_eq!(point_in_face(&g, p, &tol()), Containment::Inside, "u = {angle}");
    }
    let puck = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line { to: DVec2::new(3.0, 0.0) },
            Segment::Arc { center: DVec2::new(3.0, 1.0), sweep: PI / 2.0 },
            Segment::Line { to: DVec2::new(0.0, 1.0) },
        ],
    };
    let solid = k().revolve(&puck, &Frame::WORLD).unwrap();
    let tid = face_where(&solid, |f| matches!(f.surface, Surface::Torus { .. }));
    let gt = face_geom(&solid, tid);
    let p = gt.surface.point(2.5, -PI / 4.0);
    assert_eq!(point_in_face(&gt, p, &tol()), Containment::Inside);
    // Punkt der Kegelfläche, der nicht auf dem Torusstück liegt
    let q = gt.surface.point(1.0, 1.5);
    assert_ne!(point_in_face(&gt, q, &tol()), Containment::Inside);
}

#[test]
fn mirrored_faces_work_too() {
    let m = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    for solid in [k().make_cylinder(5.0, 10.0).unwrap(), k().make_sphere(4.0).unwrap()] {
        let mirrored = k().transform(&solid, &m).unwrap();
        for (id, f) in mirrored.faces.iter() {
            if matches!(f.surface, Surface::Plane { .. }) {
                continue;
            }
            let g = face_geom(&mirrored, id);
            let patch = f.patch.unwrap();
            let p = g.surface.point(0.5 * (patch.u0 + patch.u1) + 0.1, 0.5 * (patch.v0 + patch.v1));
            assert_eq!(point_in_face(&g, p, &tol()), Containment::Inside, "{:?}", f.surface);
        }
    }
}

#[test]
fn winding_counts_correctly() {
    let sq = [DVec2::new(0.0, 0.0), DVec2::new(2.0, 0.0), DVec2::new(2.0, 2.0), DVec2::new(0.0, 2.0)];
    assert_eq!(winding(&sq, DVec2::new(1.0, 1.0)), 1);
    assert_eq!(winding(&sq, DVec2::new(3.0, 1.0)), 0);
    let rev: Vec<DVec2> = sq.iter().rev().cloned().collect();
    assert_eq!(winding(&rev, DVec2::new(1.0, 1.0)), -1);
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test uv`
Expected: FAIL (Kompilierfehler).

- [ ] **Step 3: Implementieren**

`mod.rs` ergänzen: `mod uv;` und

```rust
pub use uv::{
    Containment, FaceGeom, face_geom, loop_points, oriented_area, point_in_face, points_to_uv,
    points_to_uv_indexed, winding,
};
```

`crates/planora-kernel/src/boolean/uv.rs`:

```rust
use super::seg::Seg;
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::topology::{Face, Loop, Solid};
use planora_math::{DVec2, DVec3, TAU, Tolerance, orient2d};

/// Geometrische Beschreibung einer (ggf. beschnittenen) Fläche, unabhängig vom `Solid`.
#[derive(Clone, Debug)]
pub struct FaceGeom {
    pub surface: Surface,
    pub same_sense: bool,
    pub domain: Option<UvRect>,
    /// `loops[0]` ist die äußere Schleife, danach folgen Löcher.
    pub loops: Vec<Vec<Seg>>,
}

pub fn face_geom(solid: &Solid, id: Id<Face>) -> FaceGeom {
    let f = &solid.faces[id];
    let to_loop = |l: &Loop| -> Vec<Seg> {
        l.half_edges
            .iter()
            .map(|he| {
                let e = &solid.edges[he.edge];
                let s = Seg::new(e.curve.clone(), e.t0, e.t1);
                if he.forward { s } else { s.reversed() }
            })
            .collect()
    };
    FaceGeom {
        surface: f.surface.clone(),
        same_sense: f.same_sense,
        domain: f.patch,
        loops: std::iter::once(&f.outer).chain(f.inner.iter()).map(to_loop).collect(),
    }
}

fn samples(s: &Seg) -> usize {
    match s.curve {
        Curve::Line { .. } => 1,
        _ => (((s.t1 - s.t0).abs() / (TAU / 72.0)).ceil() as usize).max(2),
    }
}

/// Abtastpunkte einer Schleife (jedes Stück ohne seinen Endpunkt).
pub fn loop_points(lp: &[Seg]) -> Vec<DVec3> {
    let mut pts = Vec::new();
    for s in lp {
        let n = samples(s);
        for k in 0..n {
            pts.push(s.at(k as f64 / n as f64));
        }
    }
    pts
}

/// Polygon im Parameterraum. Bei periodischen Flächen wird `u` entlang der Schleife stetig
/// aufgerollt; Pole (Kugelpole, Kegelspitze) werden als Strecke entlang `v = const` eingefügt, deren
/// Richtung aus dem Umlaufsinn folgt.
///
/// `points_to_uv_indexed` liefert zu jedem Polygonpunkt den Index des zugehörigen Eingabepunkts
/// (`None` für die eingefügten Polpunkte); `points_to_uv` gibt nur die Polygonpunkte zurück.
pub fn points_to_uv_indexed(surface: &Surface, same_sense: bool, pts: &[DVec3]) -> Vec<(DVec2, Option<usize>)> {
    if !surface.is_periodic() {
        return pts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (u, v) = surface.project(*p);
                (DVec2::new(u, v), Some(i))
            })
            .collect();
    }
    let s = if same_sense { 1.0 } else { -1.0 };
    let poles = surface.pole_vs();
    let pole_of = |v: f64| poles.iter().copied().find(|pv| (v - pv).abs() < 1e-7 * (1.0 + pv.abs()));

    // Schleife so drehen, dass sie nicht an einem Pol beginnt
    let proj: Vec<(f64, f64)> = pts.iter().map(|p| surface.project(*p)).collect();
    let start = proj.iter().position(|(_, v)| pole_of(*v).is_none()).unwrap_or(0);
    let order: Vec<usize> = (0..pts.len()).map(|i| (start + i) % pts.len()).collect();

    let mut out: Vec<(DVec2, Option<usize>)> = Vec::with_capacity(pts.len() + 4);
    let mut u_prev: Option<f64> = None;
    let mut v_prev = 0.0;
    let mut pole: Option<(f64, f64, f64)> = None; // (v_pol, v_vor_dem_Pol, u_vor_dem_Pol)
    for &i in &order {
        let (u0, v) = proj[i];
        if let Some(pv) = pole_of(v) {
            if pole.is_none() {
                pole = Some((pv, v_prev, u_prev.unwrap_or(0.0)));
            }
            continue;
        }
        let mut u = match u_prev {
            Some(up) => u0 + TAU * ((up - u0) / TAU).round(),
            None => u0,
        };
        if let Some((vp, v_before, u_at)) = pole.take() {
            let from_below = v_before < vp;
            let dir = if from_below { -s } else { s };
            let mut delta = ((u0 - u_at) * dir).rem_euclid(TAU);
            if delta < 1e-9 {
                delta = TAU;
            }
            let u_new = u_at + dir * delta;
            out.push((DVec2::new(u_at, vp), None));
            out.push((DVec2::new(u_new, vp), None));
            u = u_new;
        }
        out.push((DVec2::new(u, v), Some(i)));
        u_prev = Some(u);
        v_prev = v;
    }
    if let Some((vp, v_before, u_at)) = pole {
        // Schleife endet am Pol: zum Anfangspunkt zurückspringen
        let first_u = out.first().map(|p| p.0.x).unwrap_or(0.0);
        let from_below = v_before < vp;
        let dir = if from_below { -s } else { s };
        let mut delta = ((first_u - u_at) * dir).rem_euclid(TAU);
        if delta < 1e-9 {
            delta = TAU;
        }
        out.push((DVec2::new(u_at, vp), None));
        out.push((DVec2::new(u_at + dir * delta, vp), None));
    }
    out
}

pub fn points_to_uv(surface: &Surface, same_sense: bool, pts: &[DVec3]) -> Vec<DVec2> {
    points_to_uv_indexed(surface, same_sense, pts).into_iter().map(|(p, _)| p).collect()
}

/// Vorzeichenbehaftete Fläche des Polygons, positiv für Außenkonturen
/// (gegen den Uhrzeigersinn bei `same_sense`, sonst im Uhrzeigersinn).
pub fn oriented_area(poly: &[DVec2], same_sense: bool) -> f64 {
    let n = poly.len();
    let a: f64 = (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>() / 2.0;
    if same_sense { a } else { -a }
}

/// Windungszahl des Polygons um `q` (exakte Orientierungstests).
pub fn winding(poly: &[DVec2], q: DVec2) -> i32 {
    let n = poly.len();
    let mut w = 0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if a.y <= q.y {
            if b.y > q.y && orient2d(a, b, q) > 0.0 {
                w += 1;
            }
        } else if b.y <= q.y && orient2d(a, b, q) < 0.0 {
            w -= 1;
        }
    }
    w
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Containment {
    Inside,
    Boundary,
    Outside,
}

/// Liegt `p` (auf der Fläche) innerhalb der beschnittenen Fläche, auf ihrem Rand oder außerhalb?
pub fn point_in_face(f: &FaceGeom, p: DVec3, tol: &Tolerance) -> Containment {
    for lp in &f.loops {
        for s in lp {
            if s.closest(p).1 <= tol.linear {
                return Containment::Boundary;
            }
        }
    }
    let (u, v) = f.surface.project(p);
    let shifts: &[f64] = if f.surface.is_periodic() { &[-TAU, 0.0, TAU] } else { &[0.0] };
    let mut total = 0;
    for lp in &f.loops {
        let poly = points_to_uv(&f.surface, f.same_sense, &loop_points(lp));
        for sh in shifts {
            total += winding(&poly, DVec2::new(u + sh, v));
        }
    }
    let s = if f.same_sense { 1 } else { -1 };
    if total * s >= 1 { Containment::Inside } else { Containment::Outside }
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel --test uv` und clippy.
Expected: PASS. Falls die Kugel-Fläche nicht ≈ 2π² ergibt, ist die Pol-Logik in `points_to_uv` zu korrigieren (Pole müssen als Strecke von `u_vorher` nach `u_vorher ± 2π` laufen); der Test ist verbindlich.

- [ ] **Step 5: Commit**

```bash
git add crates/planora-kernel
git commit -m "feat(kernel): Randschleifen im Parameterraum und Punkt-in-Fläche" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Flächen auftrennen (ebener Graph mit Drehsystem)

**Files:**
- Create: `crates/planora-kernel/src/boolean/split.rs`
- Modify: `crates/planora-kernel/src/boolean/mod.rs`
- Test: `crates/planora-kernel/tests/split.rs`

**Interfaces:**
- Consumes: `FaceGeom`, `Seg`, `points_to_uv`, `loop_points`, `oriented_area`, `winding`, `BooleanError`.
- Produces: `Region { outer: Vec<Seg>, holes: Vec<Vec<Seg>> }`, `split_face(&FaceGeom, sections: &[Seg], &Tolerance) -> Result<Vec<Region>, BooleanError>`. `sections` sind Segmente, die im Inneren der Fläche liegen und deren Enden auf dem Rand bzw. an anderen Schnittsegmenten liegen (geschlossene Schleifen erlaubt). Ohne Schnitte liefert die Funktion genau eine Region (die ursprüngliche Fläche).

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/split.rs`

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn find_face(s: &Solid, f: impl Fn(&Face) -> bool) -> FaceGeom {
    let (id, _) = s.faces.iter().find(|(_, face)| f(face)).expect("Fläche");
    face_geom(s, id)
}

fn area(g: &FaceGeom, lp: &[Seg]) -> f64 {
    let poly = points_to_uv(&g.surface, g.same_sense, &loop_points(lp));
    oriented_area(&poly, g.same_sense)
}

fn region_area(g: &FaceGeom, r: &Region) -> f64 {
    area(g, &r.outer) + r.holes.iter().map(|h| area(g, h)).sum::<f64>()
}

fn line(o: DVec3, d: DVec3, len: f64) -> Seg {
    Seg::new(Curve::Line { origin: o, dir: d.normalize() }, 0.0, len)
}

fn top_face(s: &Solid, z: f64) -> FaceGeom {
    find_face(s, |f| match &f.surface {
        Surface::Plane { frame } => {
            (frame.origin.z - z).abs() < 1e-9 && frame.z.z.abs() > 0.5 && f.same_sense == (frame.z.z > 0.0)
        }
        _ => false,
    })
}

fn sorted_areas(g: &FaceGeom, regions: &[Region]) -> Vec<f64> {
    let mut a: Vec<f64> = regions.iter().map(|r| region_area(g, r)).collect();
    a.sort_by(|x, y| x.total_cmp(y));
    a
}

fn close(a: f64, b: f64, rel: f64) {
    assert!((a - b).abs() <= rel * b.abs().max(1e-9), "{a} != {b}");
}

#[test]
fn untouched_face_gives_itself() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let r = split_face(&g, &[], &tol()).unwrap();
    assert_eq!(r.len(), 1);
    assert!(r[0].holes.is_empty());
    close(region_area(&g, &r[0]), 100.0, 1e-9);
}

#[test]
fn existing_hole_stays_a_hole() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let g = top_face(&s, 5.0);
    let r = split_face(&g, &[], &tol()).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].holes.len(), 1);
}

#[test]
fn a_line_splits_a_planar_face_in_two() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let cut = line(DVec3::new(4.0, 0.0, 10.0), DVec3::Y, 10.0);
    let r = split_face(&g, &[cut], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], 40.0, 1e-6);
    close(a[1], 60.0, 1e-6);
}

#[test]
fn a_closed_circle_makes_a_disk_and_a_face_with_a_hole() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let f = Frame::from_origin_normal(DVec3::new(5.0, 5.0, 10.0), DVec3::Z);
    let circle = Seg::new(Curve::Circle { frame: f, radius: 2.0 }, 0.0, TAU);
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let with_hole = r.iter().find(|x| x.holes.len() == 1).expect("Region mit Loch");
    let disk = r.iter().find(|x| x.holes.is_empty()).expect("Scheibe");
    close(region_area(&g, with_hole), 100.0 - PI * 4.0, 0.01);
    close(region_area(&g, disk), PI * 4.0, 0.01);
}

#[test]
fn two_crossing_cuts_make_four_pieces() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    // beide Schnitte enden am Rand und treffen sich in (5,5): jeweils in zwei Hälften
    let secs = [
        line(DVec3::new(5.0, 0.0, 10.0), DVec3::Y, 5.0),
        line(DVec3::new(5.0, 5.0, 10.0), DVec3::Y, 5.0),
        line(DVec3::new(0.0, 5.0, 10.0), DVec3::X, 5.0),
        line(DVec3::new(5.0, 5.0, 10.0), DVec3::X, 5.0),
    ];
    let r = split_face(&g, &secs, &tol()).unwrap();
    assert_eq!(r.len(), 4);
    for x in sorted_areas(&g, &r) {
        close(x, 25.0, 1e-6);
    }
}

#[test]
fn a_dangling_cut_is_an_error() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let dangling = line(DVec3::new(5.0, 0.0, 10.0), DVec3::Y, 4.0); // endet frei im Inneren
    assert!(split_face(&g, &[dangling], &tol()).is_err());
}

#[test]
fn a_cylinder_wall_is_split_by_a_circle_across_the_seam() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let g = find_face(&s, |f| matches!(f.surface, Surface::Cylinder { .. }));
    let f = Frame::from_origin_normal(DVec3::new(0.0, 0.0, 4.0), DVec3::Z);
    let circle = Seg::new(Curve::Circle { frame: f, radius: 5.0 }, 0.0, TAU);
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], TAU * 4.0, 0.01);
    close(a[1], TAU * 6.0, 0.01);
}

#[test]
fn a_cylinder_wall_is_split_by_two_axial_lines_with_the_seam_inside_one_piece() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let g = find_face(&s, |f| matches!(f.surface, Surface::Cylinder { .. }));
    let l1 = line(DVec3::new(0.0, 5.0, 0.0), DVec3::Z, 10.0); // u = π/2
    let l2 = line(DVec3::new(-5.0, 0.0, 0.0), DVec3::Z, 10.0); // u = π
    let r = split_face(&g, &[l1, l2], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], (PI / 2.0) * 10.0, 0.01);
    close(a[1], (3.0 * PI / 2.0) * 10.0, 0.01);
}

#[test]
fn a_sphere_is_split_by_a_latitude_circle() {
    let s = k().make_sphere(5.0).unwrap();
    let g = face_geom(&s, Id::<Face>::from_index(0));
    let f = Frame::from_origin_normal(DVec3::new(0.0, 0.0, 3.0), DVec3::Z);
    let circle = Seg::new(Curve::Circle { frame: f, radius: 4.0 }, 0.0, TAU);
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    let lat = (3.0_f64 / 5.0).asin();
    close(a[0], TAU * (PI / 2.0 - lat), 0.02);
    close(a[1], TAU * (PI / 2.0 + lat), 0.02);
}

#[test]
fn a_small_closed_curve_on_a_sphere_makes_a_hole_and_a_cap() {
    let s = k().make_sphere(5.0).unwrap();
    let g = face_geom(&s, Id::<Face>::from_index(0));
    // kleiner Kreis um (0, 5, 0): Ebene y = 4.5, Radius sqrt(25 - 20.25)
    let f = Frame::from_origin_normal(DVec3::new(0.0, 4.5, 0.0), DVec3::Y);
    let circle = Seg::new(Curve::Circle { frame: f, radius: (25.0f64 - 20.25).sqrt() }, 0.0, TAU);
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    assert!(r.iter().any(|x| x.holes.len() == 1));
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test split`
Expected: FAIL (Kompilierfehler).

- [ ] **Step 3: Implementieren**

`mod.rs` ergänzen: `mod split;` und `pub use split::{Region, split_face};`

`crates/planora-kernel/src/boolean/split.rs`:

```rust
use super::error::BooleanError;
use super::seg::Seg;
use super::uv::{FaceGeom, loop_points, oriented_area, points_to_uv, winding};
use planora_math::{DVec2, DVec3, TAU, Tolerance};

/// Ein zusammenhängendes Flächenstück: eine äußere Schleife und beliebig viele Löcher.
#[derive(Clone, Debug)]
pub struct Region {
    pub outer: Vec<Seg>,
    pub holes: Vec<Vec<Seg>>,
}

struct He {
    seg: Seg,
    from: usize,
    to: usize,
}

/// Teilt Segmente an fremden Knotenpunkten, die in ihrem Inneren liegen.
fn split_at_points(segs: &[Seg], pts: &[DVec3], tol: &Tolerance) -> Vec<Seg> {
    let mut out = Vec::new();
    for s in segs {
        let (a, b) = (s.start(), s.end());
        let mut fs: Vec<f64> = Vec::new();
        for p in pts {
            // nur Punkte im Inneren des Stücks (nicht an seinen Enden)
            if p.distance(a) <= tol.linear || p.distance(b) <= tol.linear {
                continue;
            }
            let (f, d) = s.closest(*p);
            if d <= tol.linear && f > 1e-9 && f < 1.0 - 1e-9 && !fs.iter().any(|x| (x - f).abs() < 1e-7) {
                fs.push(f);
            }
        }
        fs.sort_by(|x, y| x.total_cmp(y));
        let mut prev = 0.0;
        for f in fs.into_iter().chain(std::iter::once(1.0)) {
            out.push(s.sub(prev, f));
            prev = f;
        }
    }
    out
}

fn node_of(nodes: &mut Vec<DVec3>, p: DVec3, tol: &Tolerance) -> usize {
    if let Some(i) = nodes.iter().position(|q| q.distance(p) <= tol.linear * 4.0) {
        return i;
    }
    nodes.push(p);
    nodes.len() - 1
}

/// Außennormale der Fläche und eine Bezugsrichtung `e1` in der Tangentialebene am Knoten.
fn node_frame(face: &FaceGeom, p: DVec3, probe: DVec3) -> (DVec3, DVec3) {
    let sgn = if face.same_sense { 1.0 } else { -1.0 };
    let (u, v) = face.surface.project(p);
    let at_pole = face
        .surface
        .pole_vs()
        .iter()
        .any(|pv| (v - pv).abs() < 1e-7 * (1.0 + pv.abs()));
    let mut n = face.surface.normal(u, v) * sgn;
    if at_pole {
        let (pu, pv) = face.surface.project(probe);
        let np = face.surface.normal(pu, pv) * sgn;
        let z = face.surface.frame().z;
        n = z * if np.dot(z) >= 0.0 { 1.0 } else { -1.0 };
    }
    (n, n.any_orthonormal_vector())
}

fn angle(n: DVec3, e1: DVec3, t: DVec3) -> f64 {
    let e2 = n.cross(e1);
    t.dot(e2).atan2(t.dot(e1))
}

/// Teilt eine Fläche entlang von Schnittsegmenten. Algorithmus: ebener Graph aus Randstücken
/// (nur in Flächenrichtung) und Schnittstücken (beide Richtungen); Umläufe werden über das
/// Drehsystem an jedem Knoten gebildet (nächste Halbkante = im Uhrzeigersinn erste nach der
/// Gegenkante). Positive Umläufe sind Außenkonturen, negative Löcher.
pub fn split_face(face: &FaceGeom, sections: &[Seg], tol: &Tolerance) -> Result<Vec<Region>, BooleanError> {
    let boundary: Vec<Seg> = face.loops.iter().flatten().cloned().collect();
    let all_pts: Vec<DVec3> = boundary
        .iter()
        .chain(sections.iter())
        .flat_map(|s| [s.start(), s.end()])
        .collect();
    let boundary = split_at_points(&boundary, &all_pts, tol);
    let mut secs = split_at_points(sections, &all_pts, tol);

    // doppelte Schnittsegmente entfernen (gleiche Knoten und gleicher Mittelpunkt)
    let mut unique: Vec<Seg> = Vec::new();
    for s in secs.drain(..) {
        let dup = unique.iter().any(|u| {
            let same_ends = (u.start().distance(s.start()) <= tol.linear && u.end().distance(s.end()) <= tol.linear)
                || (u.start().distance(s.end()) <= tol.linear && u.end().distance(s.start()) <= tol.linear);
            same_ends && u.at(0.5).distance(s.at(0.5)) <= tol.linear * 4.0
        });
        if !dup {
            unique.push(s);
        }
    }
    // Schnittsegmente, die mit einem Randstück zusammenfallen, entfallen
    let secs: Vec<Seg> = unique
        .into_iter()
        .filter(|s| {
            !boundary.iter().any(|b| {
                let ends = (b.start().distance(s.start()) <= tol.linear && b.end().distance(s.end()) <= tol.linear)
                    || (b.start().distance(s.end()) <= tol.linear && b.end().distance(s.start()) <= tol.linear);
                ends && b.at(0.5).distance(s.at(0.5)) <= tol.linear * 4.0
            })
        })
        .collect();

    let mut nodes: Vec<DVec3> = Vec::new();
    let mut hes: Vec<He> = Vec::new();
    for s in &boundary {
        let (from, to) = (node_of(&mut nodes, s.start(), tol), node_of(&mut nodes, s.end(), tol));
        hes.push(He { seg: s.clone(), from, to });
    }
    for s in &secs {
        let (from, to) = (node_of(&mut nodes, s.start(), tol), node_of(&mut nodes, s.end(), tol));
        hes.push(He { seg: s.clone(), from, to });
        hes.push(He { seg: s.reversed(), from: to, to: from });
    }
    let mut out_of: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (i, h) in hes.iter().enumerate() {
        out_of[h.from].push(i);
    }

    // Nachfolger jeder Halbkante
    let mut next = vec![usize::MAX; hes.len()];
    for (h, he) in hes.iter().enumerate() {
        let v = he.to;
        let (n, e1) = node_frame(face, nodes[v], he.seg.at(0.5));
        let ang_twin = angle(n, e1, -he.seg.tangent_at(1.0));
        let mut best: Option<(usize, f64)> = None;
        let mut tie = false;
        for &o in &out_of[v] {
            let ang_o = angle(n, e1, hes[o].seg.tangent_at(0.0));
            let mut d = (ang_twin - ang_o).rem_euclid(TAU);
            if d < 1e-9 {
                d = TAU;
            }
            match best {
                None => best = Some((o, d)),
                Some((_, bd)) => {
                    if (d - bd).abs() < 1e-9 {
                        tie = true;
                    } else if d < bd {
                        best = Some((o, d));
                        tie = false;
                    }
                }
            }
        }
        match best {
            Some((o, d)) if !tie && d < TAU - 1e-9 => next[h] = o,
            // freies Ende eines Schnitts im Inneren der Fläche oder nicht auflösbare Gleichlage
            _ => return Err(BooleanError::AmbiguousContact),
        }
    }

    // Umläufe
    let mut visited = vec![false; hes.len()];
    let mut walks: Vec<Vec<Seg>> = Vec::new();
    for start in 0..hes.len() {
        if visited[start] {
            continue;
        }
        let mut walk = Vec::new();
        let mut h = start;
        let mut guard = 0;
        loop {
            visited[h] = true;
            walk.push(hes[h].seg.clone());
            h = next[h];
            guard += 1;
            if h == start {
                break;
            }
            if visited[h] || guard > hes.len() {
                return Err(BooleanError::AmbiguousContact);
            }
        }
        walks.push(walk);
    }

    // Umläufe klassifizieren: positive = Außenkontur, negative = Loch
    let polys: Vec<Vec<DVec2>> = walks
        .iter()
        .map(|w| points_to_uv(&face.surface, face.same_sense, &loop_points(w)))
        .collect();
    let areas: Vec<f64> = polys.iter().map(|p| oriented_area(p, face.same_sense)).collect();
    let positive: Vec<usize> = (0..walks.len()).filter(|&i| areas[i] > 0.0).collect();
    if positive.is_empty() {
        return Err(BooleanError::AmbiguousContact);
    }
    let mut regions: Vec<Region> = positive
        .iter()
        .map(|&i| Region { outer: walks[i].clone(), holes: Vec::new() })
        .collect();

    let shifts: &[f64] = if face.surface.is_periodic() { &[-TAU, 0.0, TAU] } else { &[0.0] };
    let sgn = if face.same_sense { 1 } else { -1 };
    for i in (0..walks.len()).filter(|&i| areas[i] <= 0.0) {
        let rep = walks[i][0].at(0.5);
        let (u, v) = face.surface.project(rep);
        let mut owner: Option<(usize, f64)> = None;
        for (ri, &pi) in positive.iter().enumerate() {
            let w: i32 = shifts.iter().map(|sh| winding(&polys[pi], DVec2::new(u + sh, v))).sum();
            if w * sgn >= 1 && owner.is_none_or(|(_, a)| areas[pi] < a) {
                owner = Some((ri, areas[pi]));
            }
        }
        match owner {
            Some((ri, _)) => regions[ri].holes.push(walks[i].clone()),
            None => return Err(BooleanError::AmbiguousContact),
        }
    }
    Ok(regions)
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel --test split` und clippy.
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/planora-kernel
git commit -m "feat(kernel): Flächen über einen ebenen Graphen mit Drehsystem auftrennen" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Beschnitt sammeln (Imprint)

**Files:**
- Create: `crates/planora-kernel/src/boolean/imprint.rs`
- Modify: `crates/planora-kernel/src/boolean/mod.rs`
- Test: `crates/planora-kernel/tests/imprint.rs`

**Interfaces:**
- Consumes: `intersect_surfaces`, `curve_surface_roots`, `seg_intersections`, `FaceGeom`, `face_geom`, `point_in_face`, `Seg`, `BooleanError`.
- Produces: `FaceWork { geom: FaceGeom, sections: Vec<Seg> }`, `imprint(a: &Solid, b: &Solid, &Tolerance) -> Result<(Vec<FaceWork>, Vec<FaceWork>), BooleanError>` (Reihenfolge der Listen = Reihenfolge der Flächen im jeweiligen `Solid`). Ein Paar mit `Unsupported` und überlappenden Hüllquadern liefert `Err(UnsupportedIntersection{a,b})`.

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/imprint.rs`

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn total(w: &[FaceWork]) -> usize {
    w.iter().map(|f| f.sections.len()).sum()
}

#[test]
fn overlapping_boxes_get_six_sections_each() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = moved(&a, DVec3::splat(5.0));
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    assert_eq!(fa.len(), 6);
    assert_eq!(total(&fa), 6);
    assert_eq!(total(&fb), 6);
}

#[test]
fn disjoint_boxes_get_no_sections() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = moved(&a, DVec3::new(30.0, 0.0, 0.0));
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    assert_eq!(total(&fa) + total(&fb), 0);
}

#[test]
fn a_cylinder_through_a_box() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = k().make_cylinder(2.0, 14.0).unwrap();
    let b = moved(&cyl, DVec3::new(5.0, 5.0, -2.0));
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    // Boden und Deckel des Quaders bekommen je einen Kreis, sonst nichts
    assert_eq!(total(&fa), 2);
    // die Zylinderwand bekommt zwei Kreise (z = 0 und z = 10), die Kappen nichts
    assert_eq!(total(&fb), 2);
    let wall = fb
        .iter()
        .find(|f| matches!(f.geom.surface, Surface::Cylinder { .. }))
        .unwrap();
    assert_eq!(wall.sections.len(), 2);
}

#[test]
fn a_box_stacked_flush_on_a_box_imprints_its_footprint() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let small = k().make_box(DVec3::new(6.0, 6.0, 10.0)).unwrap();
    let b = moved(&small, DVec3::new(2.0, 2.0, 10.0)); // steht bündig auf der Deckfläche
    let (fa, fb) = imprint(&a, &b, &tol()).unwrap();
    // die Deckfläche von A bekommt die 4 Randkanten des B-Bodens
    let top = fa
        .iter()
        .find(|f| matches!(&f.geom.surface, Surface::Plane { frame } if (frame.origin.z - 10.0).abs() < 1e-9 && f.geom.same_sense == (frame.z.z > 0.0)))
        .unwrap();
    assert_eq!(top.sections.len(), 4);
    assert_eq!(total(&fb), 0);
}

#[test]
fn perpendicular_cylinders_are_unsupported() {
    let a = k().make_cylinder(2.0, 10.0).unwrap();
    let rot = DMat4::from_translation(DVec3::new(-5.0, 0.0, 5.0)) * DMat4::from_rotation_y(PI / 2.0);
    let b = k().transform(&k().make_cylinder(2.0, 10.0).unwrap(), &rot).unwrap();
    match imprint(&a, &b, &tol()) {
        Err(BooleanError::UnsupportedIntersection { a, b }) => {
            assert_eq!((a, b), ("Zylinder", "Zylinder"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_order_of_faces_matches_the_solid() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = moved(&a, DVec3::splat(5.0));
    let (fa, _) = imprint(&a, &b, &tol()).unwrap();
    for (i, (id, face)) in a.faces.iter().enumerate() {
        assert_eq!(fa[i].geom.same_sense, face.same_sense, "Fläche {id:?}");
    }
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test imprint`
Expected: FAIL.

- [ ] **Step 3: Implementieren**

`mod.rs`: `mod imprint;` und `pub use imprint::{FaceWork, imprint};`

`crates/planora-kernel/src/boolean/imprint.rs`:

```rust
use super::error::BooleanError;
use super::roots::{CurveSurface, curve_surface_roots, seg_intersections};
use super::seg::Seg;
use super::surfsurf::{SurfaceIntersection, intersect_surfaces, surface_name};
use super::uv::{Containment, FaceGeom, face_geom, loop_points, point_in_face};
use crate::geometry::Curve;
use crate::topology::Solid;
use planora_math::{DVec3, TAU, Tolerance};

/// Eine Fläche samt der Schnittsegmente, die in ihrem Inneren liegen.
#[derive(Clone, Debug)]
pub struct FaceWork {
    pub geom: FaceGeom,
    pub sections: Vec<Seg>,
}

fn bbox(g: &FaceGeom) -> (DVec3, DVec3) {
    let pts: Vec<DVec3> = g.loops.iter().flat_map(|l| loop_points(l)).collect();
    pts.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

fn overlaps(a: &(DVec3, DVec3), b: &(DVec3, DVec3), margin: f64) -> bool {
    a.0.x <= b.1.x + margin && b.0.x <= a.1.x + margin
        && a.0.y <= b.1.y + margin && b.0.y <= a.1.y + margin
        && a.0.z <= b.1.z + margin && b.0.z <= a.1.z + margin
}

fn is_closed_curve(c: &Curve) -> bool {
    !matches!(c, Curve::Line { .. })
}

/// Parameter von `p` auf der Kurve, falls `p` (innerhalb der Toleranz) auf ihr liegt.
fn param_on_curve(c: &Curve, p: DVec3, tol: &Tolerance) -> Option<f64> {
    let t = c.project(p);
    let t = if is_closed_curve(c) { t.rem_euclid(TAU) } else { t };
    (c.point(t).distance(p) <= tol.linear * 10.0).then_some(t)
}

/// Haltepunkte der Schnittkurve `c`: dort, wo Randkanten der beiden Flächen die jeweils andere Fläche treffen.
fn breakpoints(c: &Curve, fa: &FaceGeom, fb: &FaceGeom, tol: &Tolerance) -> Vec<f64> {
    let mut ts: Vec<f64> = Vec::new();
    let mut add = |t: f64| {
        if !ts.iter().any(|x| (x - t).abs() < 1e-9) {
            ts.push(t);
        }
    };
    for (own, other) in [(fa, fb), (fb, fa)] {
        for e in own.loops.iter().flatten() {
            match curve_surface_roots(&e.curve, e.t0, e.t1, &other.surface, tol) {
                CurveSurface::Roots(rs) => {
                    for r in rs {
                        if let Some(t) = param_on_curve(c, e.curve.point(r.t), tol) {
                            add(t);
                        }
                    }
                }
                CurveSurface::Lies => {
                    for p in [e.start(), e.end()] {
                        if let Some(t) = param_on_curve(c, p, tol) {
                            add(t);
                        }
                    }
                }
            }
        }
    }
    ts
}

/// Zerlegt die Schnittkurve an den Haltepunkten und liefert die Stücke samt Lage ihrer Mitte
/// in beiden Flächen.
fn pieces(c: &Curve, ts: &mut Vec<f64>, fa: &FaceGeom, fb: &FaceGeom, tol: &Tolerance) -> Vec<(Seg, Containment, Containment)> {
    ts.sort_by(|x, y| x.total_cmp(y));
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    if is_closed_curve(c) {
        if ts.is_empty() {
            intervals.push((0.0, TAU));
        } else {
            for w in ts.windows(2) {
                intervals.push((w[0], w[1]));
            }
            intervals.push((ts[ts.len() - 1], ts[0] + TAU));
        }
    } else {
        for w in ts.windows(2) {
            intervals.push((w[0], w[1]));
        }
    }
    intervals
        .into_iter()
        .filter(|(a, b)| (b - a).abs() > 1e-9)
        .map(|(a, b)| {
            let seg = Seg::new(c.clone(), a, b);
            let mid = seg.at(0.5);
            if seg.length() <= tol.linear {
                return (seg, Containment::Outside, Containment::Outside);
            }
            let (ca, cb) = (point_in_face(fa, mid, tol), point_in_face(fb, mid, tol));
            (seg, ca, cb)
        })
        .collect()
}

/// Randstücke von `own`, die im Inneren von `target` liegen (beide Flächen liegen auf derselben Fläche).
fn coincident_sections(own: &FaceGeom, target: &FaceGeom, tol: &Tolerance) -> Vec<Seg> {
    let mut out = Vec::new();
    for e in own.loops.iter().flatten() {
        let mut fs = vec![0.0, 1.0];
        for t in target.loops.iter().flatten() {
            for (fe, _) in seg_intersections(e, t, tol) {
                fs.push(fe);
            }
        }
        fs.sort_by(|x, y| x.total_cmp(y));
        fs.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
        for w in fs.windows(2) {
            let piece = e.sub(w[0], w[1]);
            if piece.length() <= tol.linear {
                continue;
            }
            if point_in_face(target, piece.at(0.5), tol) == Containment::Inside {
                out.push(piece);
            }
        }
    }
    out
}

/// Berechnet für jede Fläche beider Körper die Schnittsegmente in ihrem Inneren.
pub fn imprint(a: &Solid, b: &Solid, tol: &Tolerance) -> Result<(Vec<FaceWork>, Vec<FaceWork>), BooleanError> {
    let mut wa: Vec<FaceWork> = a.faces.iter().map(|(id, _)| FaceWork { geom: face_geom(a, id), sections: vec![] }).collect();
    let mut wb: Vec<FaceWork> = b.faces.iter().map(|(id, _)| FaceWork { geom: face_geom(b, id), sections: vec![] }).collect();
    let boxes_a: Vec<_> = wa.iter().map(|w| bbox(&w.geom)).collect();
    let boxes_b: Vec<_> = wb.iter().map(|w| bbox(&w.geom)).collect();

    for i in 0..wa.len() {
        for j in 0..wb.len() {
            if !overlaps(&boxes_a[i], &boxes_b[j], tol.linear * 10.0) {
                continue;
            }
            let (ga, gb) = (wa[i].geom.clone(), wb[j].geom.clone());
            match intersect_surfaces(&ga.surface, &gb.surface, tol) {
                SurfaceIntersection::None | SurfaceIntersection::Touch => {}
                SurfaceIntersection::Unsupported => {
                    return Err(BooleanError::UnsupportedIntersection {
                        a: surface_name(&ga.surface),
                        b: surface_name(&gb.surface),
                    });
                }
                SurfaceIntersection::Coincident => {
                    wa[i].sections.extend(coincident_sections(&gb, &ga, tol));
                    wb[j].sections.extend(coincident_sections(&ga, &gb, tol));
                }
                SurfaceIntersection::Curves(cs) => {
                    for c in cs {
                        let mut ts = breakpoints(&c, &ga, &gb, tol);
                        for (seg, ca, cb) in pieces(&c, &mut ts, &ga, &gb, tol) {
                            if ca == Containment::Inside && cb != Containment::Outside {
                                wa[i].sections.push(seg.clone());
                            }
                            if cb == Containment::Inside && ca != Containment::Outside {
                                wb[j].sections.push(seg);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok((wa, wb))
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel --test imprint` und clippy.
Expected: PASS. Hinweis: `a_box_stacked_flush...` ist ein Test für den Fall `Coincident` (Deckfläche von A und Boden von B liegen auf einer Ebene mit entgegengesetzter Normale).

- [ ] **Step 5: Commit**

```bash
git add crates/planora-kernel
git commit -m "feat(kernel): Schnittsegmente je Fläche sammeln (Imprint)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Einordnen (innen/außen/bündig)

**Files:**
- Create: `crates/planora-kernel/src/boolean/classify.rs`
- Modify: `crates/planora-kernel/src/boolean/mod.rs`
- Test: `crates/planora-kernel/tests/classify.rs`

**Interfaces:**
- Consumes: `FaceGeom`, `Region`, `point_in_face`, `curve_surface_roots`, `points_to_uv`, `winding`, `loop_points`.
- Produces: `Class::{Inside, Outside, OnSame, OnOpposite}`, `Location::{Inside, Outside, OnBoundary}`, `classify_region(face: &FaceGeom, region: &Region, other: &[FaceGeom], &Tolerance) -> Result<Class, BooleanError>`, `solid_contains(other: &[FaceGeom], p: DVec3, &Tolerance) -> Result<Location, BooleanError>`, `solid_faces(&Solid) -> Vec<FaceGeom>`.

- [ ] **Step 1: Tests schreiben** – `crates/planora-kernel/tests/classify.rs`

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn contains(s: &Solid, p: DVec3) -> Location {
    solid_contains(&solid_faces(s), p, &tol()).unwrap()
}

#[test]
fn box_contains_points() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    assert_eq!(contains(&b, DVec3::splat(5.0)), Location::Inside);
    assert_eq!(contains(&b, DVec3::new(15.0, 5.0, 5.0)), Location::Outside);
    assert_eq!(contains(&b, DVec3::new(10.0, 5.0, 5.0)), Location::OnBoundary);
    assert_eq!(contains(&b, DVec3::new(-3.0, -3.0, -3.0)), Location::Outside);
}

#[test]
fn curved_solids_contain_points() {
    let cyl = k().make_cylinder(3.0, 8.0).unwrap();
    assert_eq!(contains(&cyl, DVec3::new(1.0, 1.0, 4.0)), Location::Inside);
    assert_eq!(contains(&cyl, DVec3::new(4.0, 0.0, 4.0)), Location::Outside);
    assert_eq!(contains(&cyl, DVec3::new(0.0, 0.0, 9.0)), Location::Outside);
    let sph = k().make_sphere(4.0).unwrap();
    assert_eq!(contains(&sph, DVec3::new(1.0, -1.0, 2.0)), Location::Inside);
    assert_eq!(contains(&sph, DVec3::new(4.0, 4.0, 0.0)), Location::Outside);
    assert_eq!(contains(&sph, DVec3::new(0.0, 0.0, 4.0)), Location::OnBoundary);
    let cone = k().make_cone(3.0, 6.0).unwrap();
    assert_eq!(contains(&cone, DVec3::new(0.5, 0.0, 1.0)), Location::Inside);
    assert_eq!(contains(&cone, DVec3::new(2.5, 0.0, 3.0)), Location::Outside);
    let puck = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line { to: DVec2::new(3.0, 0.0) },
            Segment::Arc { center: DVec2::new(3.0, 1.0), sweep: PI / 2.0 },
            Segment::Line { to: DVec2::new(0.0, 1.0) },
        ],
    };
    let t = k().revolve(&puck, &Frame::WORLD).unwrap();
    assert_eq!(contains(&t, DVec3::new(2.0, 0.0, 0.5)), Location::Inside);
    assert_eq!(contains(&t, DVec3::new(3.9, 0.0, 0.05)), Location::Outside); // wegen der Rundung außerhalb
    assert_eq!(contains(&t, DVec3::new(6.0, 0.0, 0.5)), Location::Outside);
}

#[test]
fn mirrored_and_rotated_solids_work_too() {
    let m = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    let cyl = k().transform(&k().make_cylinder(3.0, 8.0).unwrap(), &m).unwrap();
    assert_eq!(contains(&cyl, DVec3::new(1.0, 1.0, -4.0)), Location::Inside);
    assert_eq!(contains(&cyl, DVec3::new(1.0, 1.0, 4.0)), Location::Outside);
    let rot = DMat4::from_axis_angle(DVec3::new(1.0, 2.0, 3.0).normalize(), 0.7) * DMat4::from_translation(DVec3::new(1.0, 1.0, 1.0));
    let b = k().transform(&k().make_box(DVec3::splat(4.0)).unwrap(), &rot).unwrap();
    let c = rot.transform_point3(DVec3::splat(2.0));
    assert_eq!(contains(&b, c), Location::Inside);
}

fn whole_face_class(a: &Solid, b: &Solid, face_index: usize) -> Class {
    let fa = solid_faces(a);
    let fb = solid_faces(b);
    let g = &fa[face_index];
    let region = Region { outer: g.loops[0].clone(), holes: g.loops[1..].to_vec() };
    classify_region(g, &region, &fb, &tol()).unwrap()
}

#[test]
fn whole_faces_are_classified_against_the_other_solid() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let far = moved(&a, DVec3::new(30.0, 0.0, 0.0));
    for i in 0..6 {
        assert_eq!(whole_face_class(&a, &far, i), Class::Outside, "Fläche {i}");
    }
    let big = moved(&k().make_box(DVec3::splat(20.0)).unwrap(), DVec3::splat(-5.0));
    for i in 0..6 {
        assert_eq!(whole_face_class(&a, &big, i), Class::Inside, "Fläche {i}");
    }
    for i in 0..6 {
        assert_eq!(whole_face_class(&a, &a, i), Class::OnSame, "Fläche {i}");
    }
}

#[test]
fn touching_boxes_have_opposite_coincident_faces() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = moved(&a, DVec3::new(10.0, 0.0, 0.0));
    let fa = solid_faces(&a);
    let on: Vec<usize> = (0..6).filter(|&i| whole_face_class(&a, &b, i) == Class::OnOpposite).collect();
    assert_eq!(on.len(), 1, "genau die Fläche x = 10");
    let g = &fa[on[0]];
    assert!(matches!(&g.surface, Surface::Plane { frame } if (frame.origin.x - 10.0).abs() < 1e-9));
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test classify`
Expected: FAIL.

- [ ] **Step 3: Implementieren**

`mod.rs`: `mod classify;` und `pub use classify::{Class, Location, classify_region, solid_contains, solid_faces};`

`crates/planora-kernel/src/boolean/classify.rs`:

```rust
use super::error::BooleanError;
use super::roots::{CurveSurface, curve_surface_roots};
use super::split::Region;
use super::uv::{Containment, FaceGeom, face_geom, loop_points, point_in_face, points_to_uv, winding};
use crate::geometry::Curve;
use crate::topology::Solid;
use planora_math::{DVec2, DVec3, Tolerance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Inside,
    Outside,
    /// Auf einer Fläche des anderen Körpers, gleiche Außenrichtung.
    OnSame,
    /// Auf einer Fläche des anderen Körpers, entgegengesetzte Außenrichtung.
    OnOpposite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Inside,
    Outside,
    OnBoundary,
}

pub fn solid_faces(s: &Solid) -> Vec<FaceGeom> {
    s.faces.iter().map(|(id, _)| face_geom(s, id)).collect()
}

/// Feste, „unrunde“ Strahlrichtungen für das Strahlverfahren.
fn directions() -> [DVec3; 6] {
    [
        DVec3::new(0.7321, 0.4123, 0.5471),
        DVec3::new(-0.3911, 0.8312, 0.3977),
        DVec3::new(0.2214, -0.5731, 0.7893),
        DVec3::new(-0.6123, -0.2987, 0.7311),
        DVec3::new(0.9120, -0.2210, -0.3412),
        DVec3::new(-0.1731, 0.3490, -0.9205),
    ]
    .map(|d| d.normalize())
}

fn outward_normal(g: &FaceGeom, p: DVec3) -> DVec3 {
    let (u, v) = g.surface.project(p);
    g.surface.normal(u, v) * if g.same_sense { 1.0 } else { -1.0 }
}

/// Liegt `p` auf einer Fläche von `other`? Liefert die Außennormale dieser Fläche.
fn on_surface(other: &[FaceGeom], p: DVec3, tol: &Tolerance) -> Option<DVec3> {
    other
        .iter()
        .find(|g| g.surface.implicit(p).abs() <= tol.linear && point_in_face(g, p, tol) != Containment::Outside)
        .map(|g| outward_normal(g, p))
}

/// Strahlverfahren: `Ok(Some(true))` = innen, `Ok(Some(false))` = außen, `Ok(None)` = mehrdeutig.
fn cast(other: &[FaceGeom], p: DVec3, dir: DVec3, tol: &Tolerance) -> Option<bool> {
    let reach = other
        .iter()
        .flat_map(|g| g.loops.iter().flat_map(|l| loop_points(l)))
        .map(|q| q.distance(p))
        .fold(0.0_f64, f64::max)
        * 2.0
        + 1.0;
    let ray = Curve::Line { origin: p, dir };
    let mut hits = 0;
    for g in other {
        match curve_surface_roots(&ray, 0.0, reach, &g.surface, tol) {
            CurveSurface::Lies => return None,
            CurveSurface::Roots(rs) => {
                for r in rs {
                    if r.t <= tol.linear {
                        continue;
                    }
                    let hit = p + dir * r.t;
                    match point_in_face(g, hit, tol) {
                        Containment::Boundary => return None,
                        Containment::Inside => {
                            if r.touching {
                                return None;
                            }
                            hits += 1;
                        }
                        Containment::Outside => {}
                    }
                }
            }
        }
    }
    Some(hits % 2 == 1)
}

/// Lage eines Punkts relativ zu einem Körper (gegeben durch seine Flächen).
pub fn solid_contains(other: &[FaceGeom], p: DVec3, tol: &Tolerance) -> Result<Location, BooleanError> {
    if on_surface(other, p, tol).is_some() {
        return Ok(Location::OnBoundary);
    }
    for d in directions() {
        if let Some(inside) = cast(other, p, d, tol) {
            return Ok(if inside { Location::Inside } else { Location::Outside });
        }
    }
    Err(BooleanError::AmbiguousClassification)
}

/// Ein Punkt im Inneren der Region mit möglichst großem Abstand zum Rand.
fn interior_point(face: &FaceGeom, region: &Region, tol: &Tolerance) -> Option<DVec3> {
    let outer = points_to_uv(&face.surface, face.same_sense, &loop_points(&region.outer));
    let holes: Vec<Vec<DVec2>> = region
        .holes
        .iter()
        .map(|h| points_to_uv(&face.surface, face.same_sense, &loop_points(h)))
        .collect();
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in &outer {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let sgn = if face.same_sense { 1 } else { -1 };
    let n = 24;
    let mut best: Option<(DVec3, f64)> = None;
    for i in 0..n {
        for j in 0..n {
            let uv = DVec2::new(lo.x + (hi.x - lo.x) * (i as f64 + 0.5) / n as f64, lo.y + (hi.y - lo.y) * (j as f64 + 0.5) / n as f64);
            let w = winding(&outer, uv) + holes.iter().map(|h| winding(h, uv)).sum::<i32>();
            if w * sgn != 1 {
                continue;
            }
            let q = face.surface.point(uv.x, uv.y);
            let d = region
                .outer
                .iter()
                .chain(region.holes.iter().flatten())
                .map(|s| s.closest(q).1)
                .fold(f64::MAX, f64::min);
            if best.is_none_or(|(_, bd)| d > bd) {
                best = Some((q, d));
            }
        }
    }
    best.filter(|(_, d)| *d > tol.linear * 2.0).map(|(q, _)| q)
}

/// Ordnet ein Flächenstück relativ zum anderen Körper ein.
pub fn classify_region(face: &FaceGeom, region: &Region, other: &[FaceGeom], tol: &Tolerance) -> Result<Class, BooleanError> {
    let q = interior_point(face, region, tol).ok_or(BooleanError::AmbiguousClassification)?;
    let n = outward_normal(face, q);
    if let Some(n2) = on_surface(other, q, tol) {
        return Ok(if n.dot(n2) > 0.0 { Class::OnSame } else { Class::OnOpposite });
    }
    for d in directions() {
        if let Some(inside) = cast(other, q, d, tol) {
            return Ok(if inside { Class::Inside } else { Class::Outside });
        }
    }
    Err(BooleanError::AmbiguousClassification)
}
```

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel --test classify` und clippy.
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/planora-kernel
git commit -m "feat(kernel): Flächenstücke und Punkte innen/außen/bündig einordnen" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Auswählen, Zusammennähen, `boolean()` und Validator je Schale

**Files:**
- Create: `crates/planora-kernel/src/boolean/{select,sew}.rs`
- Modify: `crates/planora-kernel/src/boolean/mod.rs`, `crates/planora-kernel/src/validate.rs`, `crates/planora-kernel/src/kernel.rs`, `crates/planora-kernel/src/lib.rs`
- Test: `crates/planora-kernel/tests/boolean_basic.rs`, `crates/planora-mesh/tests/boolean_planar.rs`

**Interfaces:**
- Consumes: alles aus Task 2–7, `validate`, `Solid`.
- Produces:
  - `Side::{A, B}`, `Piece { surface, same_sense, domain: Option<UvRect>, outer: Vec<Seg>, holes: Vec<Vec<Seg>> }`, `select_piece(op: BoolOp, side: Side, class: Class, geom: &FaceGeom, region: Region) -> Option<Piece>`
  - `sew(Vec<Piece>, &Tolerance) -> Result<Solid, BooleanError>` (mehrere Schalen möglich)
  - `boolean(a: &Solid, b: &Solid, op: BoolOp, &Tolerance) -> Result<Solid, BooleanError>` und `Kernel::boolean(&self, a, b, op)`
  - `validate` prüft Euler-Poincaré je Schale; `ValidationReport.genus` ist die Summe der Henkel über alle Schalen

- [ ] **Step 1: Tests schreiben**

`crates/planora-kernel/tests/boolean_basic.rs` (nur Topologie/Fehler, Volumen folgen im Mesh-Crate):

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn cube(size: f64) -> Solid {
    k().make_box(DVec3::splat(size)).unwrap()
}

#[test]
fn overlapping_boxes_union_difference_intersection_are_valid() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::splat(5.0));
    for op in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
        let r = k().boolean(&a, &b, op).unwrap_or_else(|e| panic!("{op:?}: {e}"));
        let report = validate(&r).unwrap_or_else(|e| panic!("{op:?}: {e}"));
        assert_eq!(report.genus, 0, "{op:?}");
        assert_eq!(r.shells.len(), 1, "{op:?}");
    }
}

#[test]
fn contained_box_gives_a_cavity_with_two_shells() {
    let a = cube(10.0);
    let b = moved(&cube(2.0), DVec3::splat(4.0));
    let r = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    assert_eq!(r.shells.len(), 2);
    validate(&r).unwrap();
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    assert_eq!(u.shells.len(), 1);
    let i = k().boolean(&a, &b, BoolOp::Intersection).unwrap();
    assert_eq!(i.shells.len(), 1);
    assert_eq!(i.faces.len(), 6);
}

#[test]
fn disjoint_boxes() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::new(30.0, 0.0, 0.0));
    assert_eq!(k().boolean(&a, &b, BoolOp::Union).unwrap().shells.len(), 2);
    assert_eq!(k().boolean(&a, &b, BoolOp::Difference).unwrap().faces.len(), 6);
    assert_eq!(k().boolean(&a, &b, BoolOp::Intersection).unwrap_err(), BooleanError::EmptyResult);
}

#[test]
fn boxes_touching_at_a_face() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::new(10.0, 0.0, 0.0));
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    validate(&u).unwrap();
    assert_eq!(u.shells.len(), 1);
    assert_eq!(k().boolean(&a, &b, BoolOp::Intersection).unwrap_err(), BooleanError::EmptyResult);
    let d = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    assert_eq!(d.faces.len(), 6);
}

#[test]
fn identical_operands() {
    let a = cube(10.0);
    assert_eq!(k().boolean(&a, &a, BoolOp::Union).unwrap().faces.len(), 6);
    assert_eq!(k().boolean(&a, &a, BoolOp::Intersection).unwrap().faces.len(), 6);
    assert_eq!(k().boolean(&a, &a, BoolOp::Difference).unwrap_err(), BooleanError::EmptyResult);
}

#[test]
fn boxes_touching_only_at_an_edge_do_not_panic() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::new(10.0, 10.0, 0.0));
    let r = k().boolean(&a, &b, BoolOp::Union);
    // entweder ein gültiger Körper oder ein benannter Fehler – niemals eine Panik
    if let Ok(s) = r {
        let _ = validate(&s);
    }
}

#[test]
fn box_stacked_flush_on_a_box() {
    let a = cube(10.0);
    let small = k().make_box(DVec3::new(6.0, 6.0, 10.0)).unwrap();
    let b = moved(&small, DVec3::new(2.0, 2.0, 10.0));
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    validate(&u).unwrap();
    assert_eq!(u.shells.len(), 1);
}

#[test]
fn invalid_input_is_rejected() {
    let a = cube(10.0);
    let mut bad = cube(10.0);
    let id = Id::<Face>::from_index(0);
    bad.faces[id].outer.half_edges[0].forward = !bad.faces[id].outer.half_edges[0].forward;
    assert!(matches!(k().boolean(&a, &bad, BoolOp::Union), Err(BooleanError::InvalidInput(_))));
}

#[test]
fn validator_checks_euler_per_shell() {
    let a = cube(10.0);
    let b = moved(&cube(2.0), DVec3::splat(4.0));
    let r = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    let report = validate(&r).unwrap();
    assert_eq!(report.genus, 0);
    assert_eq!(report.faces, 12);
}
```

`crates/planora-mesh/tests/boolean_planar.rs` (Volumenbilanzen für ebene Körper):

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn cube(size: f64) -> Solid {
    k().make_box(DVec3::splat(size)).unwrap()
}

fn vol(s: &Solid) -> f64 {
    let m = tessellate(s, &TessellationOptions { chord_tolerance: 0.001 }).unwrap();
    assert!(is_watertight(&m, 1e-6), "Netz nicht geschlossen");
    mass_properties(&m).volume
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-6 * b.abs().max(1.0), "{a} != {b}");
}

#[test]
fn overlapping_boxes_have_the_expected_volumes() {
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::splat(5.0));
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    let d = k().boolean(&a, &b, BoolOp::Difference).unwrap();
    let i = k().boolean(&a, &b, BoolOp::Intersection).unwrap();
    close(vol(&u), 1875.0);
    close(vol(&d), 875.0);
    close(vol(&i), 125.0);
    close(vol(&u) + vol(&i), vol(&a) + vol(&b));
    close(vol(&k().boolean(&b, &a, BoolOp::Difference).unwrap()), 875.0);
}

#[test]
fn box_with_a_cavity_and_contained_box() {
    let a = cube(10.0);
    let b = moved(&cube(2.0), DVec3::splat(4.0));
    close(vol(&k().boolean(&a, &b, BoolOp::Difference).unwrap()), 992.0);
    close(vol(&k().boolean(&a, &b, BoolOp::Union).unwrap()), 1000.0);
    close(vol(&k().boolean(&a, &b, BoolOp::Intersection).unwrap()), 8.0);
}

#[test]
fn through_hole_by_a_box() {
    let a = cube(10.0);
    let tool = moved(&k().make_box(DVec3::new(4.0, 4.0, 12.0)).unwrap(), DVec3::new(3.0, 3.0, -1.0));
    let d = k().boolean(&a, &tool, BoolOp::Difference).unwrap();
    close(vol(&d), 1000.0 - 160.0);
    assert_eq!(validate(&d).unwrap().genus, 1);
}

#[test]
fn flush_and_touching_boxes() {
    let a = cube(10.0);
    let on_top = moved(&k().make_box(DVec3::new(6.0, 6.0, 10.0)).unwrap(), DVec3::new(2.0, 2.0, 10.0));
    close(vol(&k().boolean(&a, &on_top, BoolOp::Union).unwrap()), 1360.0);
    let next_to = moved(&cube(10.0), DVec3::new(10.0, 0.0, 0.0));
    close(vol(&k().boolean(&a, &next_to, BoolOp::Union).unwrap()), 2000.0);
    close(vol(&k().boolean(&a, &next_to, BoolOp::Difference).unwrap()), 1000.0);
}

#[test]
fn identical_operands_volumes() {
    let a = cube(10.0);
    close(vol(&k().boolean(&a, &a, BoolOp::Union).unwrap()), 1000.0);
    close(vol(&k().boolean(&a, &a, BoolOp::Intersection).unwrap()), 1000.0);
}

#[test]
fn volume_plausibility_helper_accepts_and_rejects() {
    // implementiert in Task 9; hier nur die Bilanz als Vorgriff
    let a = cube(10.0);
    let b = moved(&cube(10.0), DVec3::splat(5.0));
    let (va, vb) = (vol(&a), vol(&b));
    let u = vol(&k().boolean(&a, &b, BoolOp::Union).unwrap());
    assert!(u >= va.max(vb) - 1e-6 && u <= va + vb + 1e-6);
}

#[test]
fn chained_operations() {
    let a = cube(20.0);
    let h1 = moved(&k().make_box(DVec3::new(4.0, 4.0, 24.0)).unwrap(), DVec3::new(3.0, 3.0, -2.0));
    let h2 = moved(&k().make_box(DVec3::new(4.0, 4.0, 24.0)).unwrap(), DVec3::new(12.0, 12.0, -2.0));
    let step1 = k().boolean(&a, &h1, BoolOp::Difference).unwrap();
    let step2 = k().boolean(&step1, &h2, BoolOp::Difference).unwrap();
    close(vol(&step2), 8000.0 - 2.0 * 16.0 * 20.0);
    assert_eq!(validate(&step2).unwrap().genus, 2);
}
```

(`planora-mesh` hat `planora-kernel` bereits als Abhängigkeit.)

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test boolean_basic`
Expected: FAIL (Kompilierfehler: `boolean`, `Kernel::boolean`).

- [ ] **Step 3: Implementieren**

`mod.rs` ergänzen/ersetzen durch vollständige Fassung:

```rust
//! Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit analytischen Flächen.

mod classify;
mod error;
mod imprint;
mod roots;
mod select;
mod seg;
mod sew;
mod split;
mod surfsurf;
mod uv;

pub use classify::{Class, Location, classify_region, solid_contains, solid_faces};
pub use error::{BoolOp, BooleanError};
pub use imprint::{FaceWork, imprint};
pub use roots::{CurveSurface, Root, curve_surface_roots, seg_intersections};
pub use select::{Piece, Side, select_piece};
pub use seg::{Seg, reverse_loop};
pub use sew::sew;
pub use split::{Region, split_face};
pub use surfsurf::{SurfaceIntersection, intersect_surfaces, surface_name};
pub use uv::{
    Containment, FaceGeom, face_geom, loop_points, oriented_area, point_in_face, points_to_uv,
    points_to_uv_indexed, winding,
};

use crate::topology::Solid;
use crate::validate::validate;
use planora_math::Tolerance;

/// Boolesche Operation `a op b`. Das Ergebnis besteht immer den Validator; sonst ein benannter Fehler.
pub fn boolean(a: &Solid, b: &Solid, op: BoolOp, tol: &Tolerance) -> Result<Solid, BooleanError> {
    validate(a).map_err(|e| BooleanError::InvalidInput(format!("Körper A: {e}")))?;
    validate(b).map_err(|e| BooleanError::InvalidInput(format!("Körper B: {e}")))?;

    let (work_a, work_b) = imprint(a, b, tol)?;
    let faces_a = solid_faces(a);
    let faces_b = solid_faces(b);

    let mut pieces: Vec<Piece> = Vec::new();
    for (side, works, other) in [(Side::A, &work_a, &faces_b), (Side::B, &work_b, &faces_a)] {
        for w in works {
            for region in split_face(&w.geom, &w.sections, tol)? {
                let class = classify_region(&w.geom, &region, other, tol)?;
                if let Some(p) = select_piece(op, side, class, &w.geom, region) {
                    pieces.push(p);
                }
            }
        }
    }
    if pieces.is_empty() {
        return Err(BooleanError::EmptyResult);
    }
    let solid = sew(pieces, tol)?;
    validate(&solid).map_err(|e| BooleanError::ResultInvalid(e.to_string()))?;
    Ok(solid)
}
```

`crates/planora-kernel/src/boolean/select.rs`:

```rust
use super::classify::Class;
use super::error::BoolOp;
use super::seg::{Seg, reverse_loop};
use super::split::Region;
use super::uv::FaceGeom;
use crate::geometry::{Surface, UvRect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// Ein ausgewähltes Flächenstück für den Ergebniskörper.
#[derive(Clone, Debug)]
pub struct Piece {
    pub surface: Surface,
    pub same_sense: bool,
    pub domain: Option<UvRect>,
    pub outer: Vec<Seg>,
    pub holes: Vec<Vec<Seg>>,
}

/// Auswahltabelle (siehe Spec §6). Bündige Stücke werden nur von Körper A übernommen.
pub fn select_piece(op: BoolOp, side: Side, class: Class, geom: &FaceGeom, region: Region) -> Option<Piece> {
    use BoolOp::*;
    use Class::*;
    use Side::*;
    // (behalten, umdrehen)
    let (keep, flip) = match (op, side, class) {
        (Union, _, Outside) => (true, false),
        (Union, A, OnSame) => (true, false),
        (Union, B, OnSame) => (false, false),
        (Union, _, OnOpposite) => (false, false),
        (Union, _, Inside) => (false, false),

        (Difference, A, Outside) => (true, false),
        (Difference, A, OnOpposite) => (true, false),
        (Difference, A, OnSame) => (false, false),
        (Difference, A, Inside) => (false, false),
        (Difference, B, Inside) => (true, true),
        (Difference, B, _) => (false, false),

        (Intersection, _, Inside) => (true, false),
        (Intersection, A, OnSame) => (true, false),
        (Intersection, B, OnSame) => (false, false),
        (Intersection, _, OnOpposite) => (false, false),
        (Intersection, _, Outside) => (false, false),
    };
    if !keep {
        return None;
    }
    let (outer, holes) = if flip {
        (reverse_loop(&region.outer), region.holes.iter().map(|h| reverse_loop(h)).collect())
    } else {
        (region.outer, region.holes)
    };
    Some(Piece {
        surface: geom.surface.clone(),
        same_sense: geom.same_sense != flip,
        domain: geom.domain,
        outer,
        holes,
    })
}
```

`crates/planora-kernel/src/boolean/sew.rs`:

```rust
use super::error::BooleanError;
use super::select::Piece;
use super::seg::Seg;
use crate::ids::Id;
use crate::topology::{Edge, Face, HalfEdge, Loop, Shell, Solid, Vertex};
use planora_math::{DVec3, Tolerance};
use std::collections::HashMap;

struct EdgeRec {
    id: Id<Edge>,
    a: Id<Vertex>,
    b: Id<Vertex>,
    mid: DVec3,
    q1: DVec3,
    q3: DVec3,
}

struct Sewer<'t> {
    solid: Solid,
    verts: Vec<(DVec3, Id<Vertex>)>,
    edges: Vec<EdgeRec>,
    uses: HashMap<Id<Edge>, Vec<usize>>,
    tol: &'t Tolerance,
}

impl Sewer<'_> {
    fn vertex(&mut self, p: DVec3) -> Id<Vertex> {
        if let Some((_, id)) = self.verts.iter().find(|(q, _)| q.distance(p) <= self.tol.linear * 4.0) {
            return *id;
        }
        let id = self.solid.add_vertex(p);
        self.verts.push((p, id));
        id
    }

    /// Findet die gemeinsame Kante zu `seg` oder legt sie an; liefert die Halbkante in Richtung von `seg`.
    fn half_edge(&mut self, seg: &Seg, face: usize) -> HalfEdge {
        let a = self.vertex(seg.start());
        let b = self.vertex(seg.end());
        let (m, q1, q3) = (seg.at(0.5), seg.at(0.25), seg.at(0.75));
        let tol = self.tol.linear * 8.0;
        let found = self.edges.iter().find(|r| {
            ((r.a == a && r.b == b) || (r.a == b && r.b == a)) && r.mid.distance(m) <= tol
        });
        let edge = match found {
            Some(r) => {
                let forward = r.q1.distance(q1) + r.q3.distance(q3) <= r.q1.distance(q3) + r.q3.distance(q1);
                let id = r.id;
                self.uses.entry(id).or_default().push(face);
                return HalfEdge { edge: id, forward };
            }
            None => self.solid.add_edge(seg.curve.clone(), seg.t0, seg.t1, a, b),
        };
        self.edges.push(EdgeRec { id: edge, a, b, mid: m, q1, q3 });
        self.uses.entry(edge).or_default().push(face);
        HalfEdge { edge, forward: true }
    }

    fn loop_of(&mut self, segs: &[Seg], face: usize) -> Loop {
        Loop { half_edges: segs.iter().map(|s| self.half_edge(s, face)).collect() }
    }
}

/// Näht Flächenstücke zu einem `Solid` zusammen: Ecken und Kanten werden verschweißt, die Schalen
/// ergeben sich aus dem Zusammenhang über gemeinsame Kanten.
pub fn sew(pieces: Vec<Piece>, tol: &Tolerance) -> Result<Solid, BooleanError> {
    let mut s = Sewer { solid: Solid::new(), verts: Vec::new(), edges: Vec::new(), uses: HashMap::new(), tol };
    let mut face_ids: Vec<Id<Face>> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        let outer = s.loop_of(&p.outer, i);
        let inner: Vec<Loop> = p.holes.iter().map(|h| s.loop_of(h, i)).collect();
        let patch = if matches!(p.surface, crate::geometry::Surface::Plane { .. }) { None } else { p.domain };
        face_ids.push(s.solid.add_face(Face { surface: p.surface.clone(), same_sense: p.same_sense, outer, inner, patch }));
    }

    // jede Kante muss von genau zwei Halbkanten benutzt werden (die Prüfung der Richtung folgt im Validator)
    let mut parent: Vec<usize> = (0..pieces.len()).collect();
    fn find(p: &mut Vec<usize>, x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let n = p[c];
            p[c] = r;
            c = n;
        }
        r
    }
    for faces in s.uses.values() {
        if faces.len() < 2 {
            return Err(BooleanError::NotClosed);
        }
        let first = faces[0];
        for &f in &faces[1..] {
            let (ra, rb) = (find(&mut parent, first), find(&mut parent, f));
            parent[ra] = rb;
        }
    }
    let mut groups: HashMap<usize, Vec<Id<Face>>> = HashMap::new();
    for (i, id) in face_ids.iter().enumerate() {
        groups.entry(find(&mut parent, i)).or_default().push(*id);
    }
    let mut shells: Vec<(usize, Vec<Id<Face>>)> = groups.into_iter().collect();
    shells.sort_by_key(|(k, _)| *k);
    s.solid.shells = shells.into_iter().map(|(_, faces)| Shell { faces }).collect();
    Ok(s.solid)
}
```

`crates/planora-kernel/src/validate.rs`: Euler-Poincaré je Schale. Ersetze den Abschnitt ab `let used: HashSet<_> = …` bis zum Ende von `validate` durch:

```rust
    // Euler-Poincaré je Schale: V − E + F − R = 2 − 2g (Henkel g); ohne Schalen gilt das Solid als eine Schale
    let shell_faces: Vec<Vec<crate::ids::Id<crate::topology::Face>>> = if solid.shells.is_empty() {
        vec![solid.faces.iter().map(|(id, _)| id).collect()]
    } else {
        solid.shells.iter().map(|s| s.faces.clone()).collect()
    };
    let mut total_genus = 0usize;
    let mut used_all: HashSet<crate::ids::Id<crate::topology::Vertex>> = HashSet::new();
    for faces in &shell_faces {
        let mut edges: HashSet<crate::ids::Id<crate::topology::Edge>> = HashSet::new();
        let mut rings = 0usize;
        for &fid in faces {
            let face = &solid.faces[fid];
            rings += face.inner.len();
            for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
                for he in &lp.half_edges {
                    edges.insert(he.edge);
                }
            }
        }
        let verts: HashSet<_> = edges
            .iter()
            .flat_map(|&e| [solid.edges[e].start, solid.edges[e].end])
            .collect();
        used_all.extend(verts.iter().copied());
        let characteristic = verts.len() as i64 - edges.len() as i64 + faces.len() as i64 - rings as i64;
        let twice_genus = 2 - characteristic;
        if twice_genus < 0 || twice_genus % 2 != 0 {
            return Err(ValidationError::InvalidEulerCharacteristic(characteristic));
        }
        total_genus += (twice_genus / 2) as usize;
    }

    Ok(ValidationReport {
        vertices: used_all.len(),
        edges: n_edges,
        faces: solid.faces.len(),
        inner_loops,
        genus: total_genus,
    })
}
```

(die frühere Berechnung von `used`, `characteristic`, `twice_genus` entfällt; `HashSet` bleibt importiert.)

`crates/planora-kernel/src/kernel.rs`: im Trait `fn boolean(&self, a: &Solid, b: &Solid, op: BoolOp) -> Result<Solid, BooleanError>;` und in der Implementierung `crate::boolean::boolean(a, b, op, &self.tolerance)`; Imports `use crate::boolean::{BoolOp, BooleanError};`.

`crates/planora-kernel/src/lib.rs`: `pub use boolean::{BoolOp, BooleanError};`

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-kernel --test boolean_basic` und `cargo test -p planora-mesh --test boolean_planar` (ebene Körper brauchen keine neue Tessellierung) und `cargo clippy --workspace --all-targets -- -D warnings`.
Expected: PASS. Falls Fälle scheitern, zuerst eine Region/Auswahl in `boolean_basic` per `dbg!` auf die beteiligten Klassen prüfen (häufigste Ursache: falsche Zuordnung bündiger Stücke).

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat(kernel): boolean() mit Auswahl, Zusammennähen und Euler je Schale" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Netz für beschnittene Flächen und gekrümmte Fälle

**Files:**
- Create: `crates/planora-mesh/src/trimmed.rs`, `crates/planora-mesh/src/volume.rs`
- Modify: `crates/planora-mesh/Cargo.toml` (+ `spade = "2"`), root `Cargo.toml` (`[workspace.dependencies] spade = "2"`), `crates/planora-mesh/src/{lib,tessellate}.rs`
- Test: `crates/planora-mesh/tests/trimmed.rs`, `crates/planora-mesh/tests/boolean_curved.rs`

**Interfaces:**
- Consumes: `planora_kernel::boolean::{face_geom, points_to_uv, loop_points, oriented_area, winding, FaceGeom}`, `Discretizer` (bestehend).
- Produces:
  - `tessellate` triangulert beschnittene gekrümmte Flächen (gleiche Signatur wie bisher)
  - `boolean_volume_plausible(op: BoolOp, v_a: f64, v_b: f64, v_result: f64, rel_tol: f64) -> bool` (aus `planora_mesh`)
  - Erkennung „beschnitten“: Fläche mit nicht-ebener Oberfläche, deren Schleifen nicht die Rechteck-Domäne füllen (UV-Fläche weicht um mehr als 1e-6 relativ von der Domänenfläche ab) oder die innere Schleifen hat.

- [ ] **Step 1: Lizenz und Abhängigkeit**

Vor dem Hinzufügen: `spade` 2.15.1 ist MIT OR Apache-2.0 (Prüfung am 2026-10-09 über crates.io-API: Lizenz-Feld und Abhängigkeiten `hashbrown`, `num-traits`, `robust`, `smallvec`). Eintragen: root `Cargo.toml` → `spade = "2"`; `crates/planora-mesh/Cargo.toml` → `spade.workspace = true`. Danach `cargo deny check` — muss grün bleiben (Befund → anhalten und fragen).

- [ ] **Step 2: Tests schreiben**

`crates/planora-mesh/tests/trimmed.rs`:

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn opts() -> TessellationOptions {
    TessellationOptions { chord_tolerance: 0.001 }
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn checked(s: &Solid) -> f64 {
    validate(s).unwrap();
    let m = tessellate(s, &opts()).unwrap();
    assert!(is_watertight(&m, 1e-6), "Netz nicht geschlossen");
    for t in m.indices.chunks_exact(3) {
        let (a, b, c) = (m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]);
        let n = (b - a).cross(c - a);
        if n.length() < 1e-14 {
            continue;
        }
        let avg = m.normals[t[0] as usize] + m.normals[t[1] as usize] + m.normals[t[2] as usize];
        assert!(n.dot(avg) > 0.0, "Dreieck entgegen der Normalen orientiert");
    }
    mass_properties(&m).volume
}

#[test]
fn box_minus_cylinder_through() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = moved(&k().make_cylinder(2.0, 14.0).unwrap(), DVec3::new(5.0, 5.0, -2.0));
    let d = k().boolean(&a, &cyl, BoolOp::Difference).unwrap();
    let v = checked(&d);
    assert!(rel(v, 1000.0 - PI * 4.0 * 10.0) < 0.005, "{v}");
    assert_eq!(validate(&d).unwrap().genus, 1);
}

#[test]
fn box_minus_cylinder_blind_hole() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = moved(&k().make_cylinder(2.0, 10.0).unwrap(), DVec3::new(5.0, 5.0, 5.0));
    let d = k().boolean(&a, &cyl, BoolOp::Difference).unwrap();
    let v = checked(&d);
    assert!(rel(v, 1000.0 - PI * 4.0 * 5.0) < 0.005, "{v}");
}

#[test]
fn box_minus_flush_cylinder_of_equal_height() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = moved(&k().make_cylinder(2.0, 10.0).unwrap(), DVec3::new(5.0, 5.0, 0.0));
    let d = k().boolean(&a, &cyl, BoolOp::Difference).unwrap();
    let v = checked(&d);
    assert!(rel(v, 1000.0 - PI * 4.0 * 10.0) < 0.005, "{v}");
}

#[test]
fn cylinder_and_box_union_and_intersection() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = moved(&k().make_cylinder(2.0, 14.0).unwrap(), DVec3::new(5.0, 5.0, -2.0));
    let u = k().boolean(&a, &cyl, BoolOp::Union).unwrap();
    let i = k().boolean(&a, &cyl, BoolOp::Intersection).unwrap();
    let (vu, vi) = (checked(&u), checked(&i));
    assert!(rel(vi, PI * 4.0 * 10.0) < 0.005, "{vi}");
    assert!(rel(vu, 1000.0 + PI * 4.0 * 4.0) < 0.005, "{vu}");
}

#[test]
fn half_sphere_by_a_box() {
    let sph = moved(&k().make_sphere(5.0).unwrap(), DVec3::new(5.0, 5.0, 5.0));
    let boxx = moved(&k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap(), DVec3::new(5.0, 0.0, 0.0));
    let i = k().boolean(&sph, &boxx, BoolOp::Intersection).unwrap();
    let v = checked(&i);
    assert!(rel(v, 2.0 / 3.0 * PI * 125.0) < 0.01, "{v}");
}

#[test]
fn sphere_inside_a_box_makes_a_cavity() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let sph = moved(&k().make_sphere(3.0).unwrap(), DVec3::splat(5.0));
    let d = k().boolean(&a, &sph, BoolOp::Difference).unwrap();
    assert_eq!(d.shells.len(), 2);
    let v = checked(&d);
    assert!(rel(v, 1000.0 - 4.0 / 3.0 * PI * 27.0) < 0.01, "{v}");
}

#[test]
fn coaxial_cylinders_union() {
    let a = k().make_cylinder(3.0, 10.0).unwrap();
    let b = moved(&k().make_cylinder(2.0, 10.0).unwrap(), DVec3::new(0.0, 0.0, 6.0));
    let u = k().boolean(&a, &b, BoolOp::Union).unwrap();
    let v = checked(&u);
    assert!(rel(v, PI * 9.0 * 10.0 + PI * 4.0 * 6.0) < 0.005, "{v}");
}

#[test]
fn cone_cut_by_a_box() {
    let cone = k().make_cone(4.0, 8.0).unwrap();
    let boxx = moved(&k().make_box(DVec3::new(20.0, 20.0, 10.0)).unwrap(), DVec3::new(-10.0, -10.0, 4.0));
    let d = k().boolean(&cone, &boxx, BoolOp::Difference).unwrap();
    // Stumpf von z = 0 bis 4: Kegelstumpf mit r = 4 → 2, h = 4
    let expect = PI * 4.0 / 3.0 * (16.0 + 8.0 + 4.0);
    let v = checked(&d);
    assert!(rel(v, expect) < 0.01, "{v} vs {expect}");
}

#[test]
fn unsupported_pair_is_a_clear_error() {
    let a = k().make_cylinder(2.0, 10.0).unwrap();
    let rot = DMat4::from_translation(DVec3::new(-5.0, 0.0, 5.0)) * DMat4::from_rotation_y(PI / 2.0);
    let b = k().transform(&k().make_cylinder(2.0, 10.0).unwrap(), &rot).unwrap();
    assert!(matches!(
        k().boolean(&a, &b, BoolOp::Union),
        Err(BooleanError::UnsupportedIntersection { .. })
    ));
}

#[test]
fn volume_balance_holds_for_curved_operands() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = moved(&k().make_cylinder(3.0, 14.0).unwrap(), DVec3::new(4.0, 6.0, -2.0));
    let (va, vb) = (checked(&a), checked(&cyl));
    let vu = checked(&k().boolean(&a, &cyl, BoolOp::Union).unwrap());
    let vi = checked(&k().boolean(&a, &cyl, BoolOp::Intersection).unwrap());
    let vd = checked(&k().boolean(&a, &cyl, BoolOp::Difference).unwrap());
    assert!((vu + vi - (va + vb)).abs() < 0.01 * (va + vb), "{vu} + {vi} != {va} + {vb}");
    assert!((vd - (va - vi)).abs() < 0.01 * va, "{vd} != {va} - {vi}");
    for (op, v) in [(BoolOp::Union, vu), (BoolOp::Difference, vd), (BoolOp::Intersection, vi)] {
        assert!(boolean_volume_plausible(op, va, vb, v, 0.01), "{op:?}");
    }
    assert!(!boolean_volume_plausible(BoolOp::Union, va, vb, 0.5 * va, 0.01));
    assert!(!boolean_volume_plausible(BoolOp::Intersection, va, vb, va + vb, 0.01));
    assert!(!boolean_volume_plausible(BoolOp::Difference, va, vb, va * 1.2, 0.01));
}
```

- [ ] **Step 3: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-mesh --test trimmed`
Expected: FAIL (Kompilierfehler `boolean_volume_plausible`; beschnittene Flächen werden noch falsch trianguliert).

- [ ] **Step 4: Implementieren**

`crates/planora-mesh/src/volume.rs`:

```rust
use planora_kernel::boolean::BoolOp;

/// Prüft, ob das Ergebnisvolumen einer Booleschen Operation in den möglichen Grenzen liegt
/// (Spielraum `rel_tol` relativ zur Summe der Eingangsvolumen).
pub fn boolean_volume_plausible(op: BoolOp, v_a: f64, v_b: f64, v_result: f64, rel_tol: f64) -> bool {
    let eps = rel_tol * (v_a + v_b).max(1e-12);
    if !(v_result > -eps) {
        return false;
    }
    match op {
        BoolOp::Union => v_result >= v_a.max(v_b) - eps && v_result <= v_a + v_b + eps,
        BoolOp::Difference => v_result >= v_a - v_b - eps && v_result <= v_a + eps,
        BoolOp::Intersection => v_result <= v_a.min(v_b) + eps,
    }
}
```

`lib.rs`: `mod trimmed; mod volume;` und `pub use volume::boolean_volume_plausible;`

`crates/planora-mesh/src/trimmed.rs` (mit `spade`; API gegen die aufgelöste Version prüfen):

```rust
use crate::discretize::Discretizer;
use crate::mesh::Mesh;
use planora_kernel::Surface;
use planora_kernel::boolean::{FaceGeom, loop_points, oriented_area, points_to_uv, points_to_uv_indexed, winding};
use planora_math::{DVec2, DVec3, TAU};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

/// Ist die Fläche gegenüber ihrer Rechteck-Domäne beschnitten (Löcher oder fehlende Anteile)?
pub fn is_trimmed(g: &FaceGeom) -> bool {
    let Some(d) = g.domain else { return false };
    if g.loops.len() > 1 {
        return true;
    }
    let poly = points_to_uv(&g.surface, g.same_sense, &loop_points(&g.loops[0]));
    let full = (d.u1 - d.u0) * (d.v1 - d.v0);
    let a = oriented_area(&poly, g.same_sense);
    (a - full).abs() > 1e-6 * full.abs().max(1e-12)
}

/// Triangulierung einer beschnittenen gekrümmten Fläche im Parameterraum.
/// `rings` sind die gemeinsamen 3D-Randpunkte je Schleife (aus den Kantenpolylinien, Reihenfolge wie
/// `g.loops`), damit Nachbarflächen exakt dieselben Randpunkte haben.
pub fn tessellate_trimmed(g: &FaceGeom, rings: &[Vec<DVec3>], disc: &Discretizer, mesh: &mut Mesh) -> Result<(), ()> {
    let sign = if g.same_sense { 1.0 } else { -1.0 };
    let indexed: Vec<Vec<(DVec2, Option<usize>)>> = rings
        .iter()
        .map(|r| points_to_uv_indexed(&g.surface, g.same_sense, r))
        .collect();
    let polys: Vec<Vec<DVec2>> = indexed.iter().map(|r| r.iter().map(|(p, _)| *p).collect()).collect();

    let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    let mut pts3: Vec<DVec3> = Vec::new();
    let mut uv: Vec<DVec2> = Vec::new();

    // Punkt einfügen (gleiche UV-Lage ergibt denselben Punkt) und seine 3D-Lage merken
    fn add(
        cdt: &mut ConstrainedDelaunayTriangulation<Point2<f64>>,
        pts3: &mut Vec<DVec3>,
        uv: &mut Vec<DVec2>,
        p2: DVec2,
        p3: DVec3,
    ) -> Result<spade::handles::FixedVertexHandle, ()> {
        let h = cdt.insert(Point2::new(p2.x, p2.y)).map_err(|_| ())?;
        if h.index() == pts3.len() {
            pts3.push(p3);
            uv.push(p2);
        }
        Ok(h)
    }

    // Randpunkte: exakte 3D-Lage aus den gemeinsamen Polylinien; eingefügte Polpunkte werten die Fläche aus
    for (ring, pts) in indexed.iter().zip(rings.iter()) {
        let mut hs = Vec::with_capacity(ring.len());
        for (p2, src) in ring {
            let p3 = match src {
                Some(i) => pts[*i],
                None => g.surface.point(p2.x, p2.y),
            };
            hs.push(add(&mut cdt, &mut pts3, &mut uv, *p2, p3)?);
        }
        for i in 0..hs.len() {
            let (a, b) = (hs[i], hs[(i + 1) % hs.len()]);
            if a != b {
                cdt.add_constraint(a, b);
            }
        }
    }

    let inside = |q: DVec2| -> bool {
        let w: i32 = polys.iter().map(|p| winding(p, q)).sum();
        (w as f64) * sign >= 1.0
    };

    // Innenpunkte als Raster nur bei in beiden Richtungen gekrümmten Flächen (Kugel, Torus);
    // Zylinder und Kegel sind in v gerade, dort genügen die Randpunkte.
    if matches!(g.surface, Surface::Sphere { .. } | Surface::Torus { .. }) {
        let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
        for p in polys.iter().flatten() {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let step = TAU / disc.segments(TAU).max(8) as f64;
        let nu = (((hi.x - lo.x) / step).ceil() as usize).max(1);
        let nv = (((hi.y - lo.y) / step).ceil() as usize).max(1);
        for i in 1..nu {
            for j in 1..nv {
                let q = DVec2::new(
                    lo.x + (hi.x - lo.x) * i as f64 / nu as f64,
                    lo.y + (hi.y - lo.y) * j as f64 / nv as f64,
                );
                if inside(q) {
                    add(&mut cdt, &mut pts3, &mut uv, q, g.surface.point(q.x, q.y))?;
                }
            }
        }
    }

    // Dreiecke übernehmen, deren Schwerpunkt in der Fläche liegt
    let base = mesh.positions.len() as u32;
    for (i, p) in pts3.iter().enumerate() {
        mesh.positions.push(*p);
        mesh.normals.push(g.surface.normal(uv[i].x, uv[i].y) * sign);
    }
    for f in cdt.inner_faces() {
        let v = f.vertices();
        let idx = [v[0].fix().index(), v[1].fix().index(), v[2].fix().index()];
        let centroid = (uv[idx[0]] + uv[idx[1]] + uv[idx[2]]) / 3.0;
        if !inside(centroid) {
            continue;
        }
        let (a, b, c) = (pts3[idx[0]], pts3[idx[1]], pts3[idx[2]]);
        if (b - a).cross(c - a).length() < 1e-14 {
            continue; // entartet (an einem Pol)
        }
        // gegen den Uhrzeigersinn im Parameterraum = natürliche Normale; bei !same_sense umdrehen
        let area2 = (uv[idx[1]] - uv[idx[0]]).perp_dot(uv[idx[2]] - uv[idx[0]]);
        let tri = if (area2 > 0.0) == g.same_sense { [idx[0], idx[1], idx[2]] } else { [idx[0], idx[2], idx[1]] };
        mesh.indices.extend(tri.iter().map(|&i| base + i as u32));
    }
    Ok(())
}
```

In `tessellate.rs` im Zweig für nicht ebene Flächen (bestehende Größenprüfungen wie `ensure_room`/`TooLarge` beibehalten):

```rust
_ => {
    let g = planora_kernel::boolean::face_geom(solid, fid);
    if crate::trimmed::is_trimmed(&g) {
        let rings: Vec<Vec<DVec3>> = std::iter::once(&face.outer)
            .chain(face.inner.iter())
            .map(|lp| loop_points(lp, &polylines)) // bestehende Hilfsfunktion in tessellate.rs
            .collect();
        crate::trimmed::tessellate_trimmed(&g, &rings, &disc, &mut mesh)
            .map_err(|_| MeshError::Triangulation(fid.index()))?;
    } else {
        let patch = face.patch.ok_or(MeshError::MissingPatch(fid.index()))?;
        tessellate_patch(face, &patch, &disc, &mut mesh);
    }
}
```

Hinweis: Verbindlich sind die Tests (geschlossene, durchgängig orientierte Netze, richtige Volumen, Normalen im Einklang mit der Dreiecksorientierung). Die Quelltexte oben sind der Entwurf; `spade`-Typen und -Methoden (`FixedVertexHandle`, `add_constraint`, `inner_faces`) gegen die aufgelöste Version prüfen. An der Naht liegen doppelte UV-Punkte im Raum an derselben Stelle (Verschweißen über Position, wie bisher).

- [ ] **Step 5: Tests laufen lassen**

Run: `cargo test -p planora-mesh` (alle Tests, inklusive der bisherigen), `cargo test -p planora-kernel`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo deny check`.
Expected: PASS. Falls ein Volumentest knapp scheitert, zuerst die Dichtheitsprüfung (`is_watertight`) ansehen: Lücken weisen auf nicht übereinstimmende Randpunkte zwischen Nachbarflächen hin.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates
git commit -m "feat(mesh): beschnittene gekrümmte Flächen (spade) und Volumenprobe für Boolesche Operationen" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Oberfläche – Vereinigen, Abziehen, Schneiden und Körper verschieben

**Files:**
- Create: `crates/planora-app/src/boolean_dialog.rs`
- Modify: `crates/planora-app/src/{state,properties,app,lib}.rs`
- Test: `crates/planora-app/tests/boolean_state.rs`

**Interfaces:**
- Consumes: `Kernel::{boolean, transform}`, `BoolOp`, `BooleanError`, `boolean_volume_plausible`, `AppState` (bestehend).
- Produces:
  - `AppState::boolean(&mut self, a: usize, b: usize, op: BoolOp) -> Result<usize, String>` (ersetzt die Körper `a` und `b` durch das Ergebnis, ohne neu zu platzieren; liefert den Index des Ergebnisses; Fehlertext deutsch)
  - `AppState::translate_body(&mut self, index: usize, delta: DVec3) -> Result<(), String>`
  - `AppState::add_body_in_place(&mut self, kind, name, solid) -> Result<usize, String>` (wie `add_body`, aber ohne Verschieben neben die anderen Körper)
  - `BooleanDialog` (egui-Fenster mit Auswahl A, B, Operation), Menü „Bearbeiten“

- [ ] **Step 1: Tests schreiben** – `crates/planora-app/tests/boolean_state.rs`

```rust
use planora_app::state::*;
use planora_kernel::boolean::BoolOp;
use planora_kernel::*;
use planora_math::DVec3;

fn state_with_box_and_cylinder() -> AppState {
    let mut s = AppState::new();
    let k = s.kernel;
    s.add_body(BodyKind::Box, "Quader".into(), k.make_box(DVec3::splat(10.0)).unwrap()).unwrap();
    s.add_body(BodyKind::Cylinder, "Zylinder".into(), k.make_cylinder(2.0, 14.0).unwrap()).unwrap();
    s
}

#[test]
fn translating_a_body_moves_bounds_and_scene() {
    let mut s = state_with_box_and_cylinder();
    let before = s.bodies[1].bounds;
    let v = s.scene_version;
    s.translate_body(1, DVec3::new(1.0, 2.0, 3.0)).unwrap();
    let after = s.bodies[1].bounds;
    assert!((after.0 - before.0 - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-6);
    assert!(s.scene_version > v);
    assert!(s.translate_body(9, DVec3::ZERO).is_err());
}

#[test]
fn boolean_replaces_both_bodies_with_the_result() {
    let mut s = state_with_box_and_cylinder();
    // Zylinder mittig durch den Quader legen
    let b = s.bodies[0].bounds;
    let c = s.bodies[1].bounds;
    let target = DVec3::new(5.0, 5.0, -2.0) + b.0;
    s.translate_body(1, target - c.0 - DVec3::new(2.0, 2.0, 0.0)).unwrap();
    let idx = s.boolean(0, 1, BoolOp::Difference).unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(idx, 0);
    let vol = s.bodies[0].mass.volume;
    assert!((vol - (1000.0 - std::f64::consts::PI * 4.0 * 10.0)).abs() / 1000.0 < 0.01, "{vol}");
    assert!(s.bodies[0].name.contains("Abzug") || s.bodies[0].name.contains("Quader"));
}

#[test]
fn boolean_errors_are_german_messages_and_keep_the_bodies() {
    let mut s = AppState::new();
    let k = s.kernel;
    s.add_body(BodyKind::Box, "A".into(), k.make_box(DVec3::splat(10.0)).unwrap()).unwrap();
    s.add_body(BodyKind::Box, "B".into(), k.make_box(DVec3::splat(10.0)).unwrap()).unwrap();
    // die Körper liegen nebeneinander: Schneiden ist leer
    let err = s.boolean(0, 1, BoolOp::Intersection).unwrap_err();
    assert!(err.to_lowercase().contains("leer"), "{err}");
    assert_eq!(s.bodies.len(), 2);
    assert!(s.boolean(0, 0, BoolOp::Union).is_err(), "A und B müssen verschieden sein");
    assert!(s.boolean(0, 7, BoolOp::Union).is_err());
}

#[test]
fn unsupported_pairs_report_a_clear_message() {
    let mut s = AppState::new();
    let k = s.kernel;
    s.add_body(BodyKind::Cylinder, "A".into(), k.make_cylinder(2.0, 10.0).unwrap()).unwrap();
    let rot = planora_math::DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0)) * planora_math::DMat4::from_rotation_y(std::f64::consts::FRAC_PI_2);
    let b = k.transform(&k.make_cylinder(2.0, 10.0).unwrap(), &rot).unwrap();
    s.add_body_in_place(BodyKind::Cylinder, "B".into(), b).unwrap();
    // Körper A ist verschoben worden? Nein – beide liegen am Ursprung (in place)
    let err = s.boolean(0, 1, BoolOp::Union).unwrap_err();
    assert!(err.contains("nicht unterstützt"), "{err}");
}
```

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-app --test boolean_state`
Expected: FAIL.

- [ ] **Step 3: Implementieren**

`state.rs`: ergänzen

```rust
use planora_kernel::boolean::BoolOp;
use planora_mesh::boolean_volume_plausible;

impl AppState {
    /// Wie `add_body`, aber ohne das Verschieben neben die vorhandenen Körper.
    pub fn add_body_in_place(&mut self, kind: BodyKind, name: String, solid: Solid) -> Result<usize, String> {
        self.kernel.validate(&solid).map_err(|e| format!("Ungültiger Körper: {e}"))?;
        let body = self.make_body(kind, name, solid)?;
        self.bodies.push(body);
        self.rebuild_scene();
        self.frame_all();
        Ok(self.bodies.len() - 1)
    }

    pub fn translate_body(&mut self, index: usize, delta: DVec3) -> Result<(), String> {
        let body = self.bodies.get(index).ok_or_else(|| "Körper nicht gefunden".to_string())?;
        let moved = self
            .kernel
            .transform(&body.solid, &DMat4::from_translation(delta))
            .map_err(|e| format!("Verschieben fehlgeschlagen: {e}"))?;
        let (kind, name) = (body.kind, body.name.clone());
        let new = self.make_body(kind, name, moved)?;
        self.bodies[index] = new;
        self.rebuild_scene();
        Ok(())
    }

    /// Führt `a op b` aus; ersetzt beide Körper durch das Ergebnis (an der Stelle des ersten).
    pub fn boolean(&mut self, a: usize, b: usize, op: BoolOp) -> Result<usize, String> {
        if a == b || a >= self.bodies.len() || b >= self.bodies.len() {
            return Err("Bitte zwei verschiedene Körper wählen".into());
        }
        let result = self
            .kernel
            .boolean(&self.bodies[a].solid, &self.bodies[b].solid, op)
            .map_err(|e| e.to_string())?;
        let name = match op {
            BoolOp::Union => format!("Vereinigung: {} + {}", self.bodies[a].name, self.bodies[b].name),
            BoolOp::Difference => format!("Abzug: {} − {}", self.bodies[a].name, self.bodies[b].name),
            BoolOp::Intersection => format!("Schnitt: {} ∩ {}", self.bodies[a].name, self.bodies[b].name),
        };
        let kind = self.bodies[a].kind;
        let body = self.make_body(kind, name, result)?;
        let (va, vb) = (self.bodies[a].mass.volume, self.bodies[b].mass.volume);
        if !boolean_volume_plausible(op, va, vb, body.mass.volume, 0.02) {
            return Err("Das Ergebnis ist unplausibel (Volumenprobe fehlgeschlagen) und wurde verworfen".into());
        }
        let (lo, hi) = (a.min(b), a.max(b));
        self.bodies.remove(hi);
        self.bodies[lo] = body;
        self.hover = None;
        self.selection = None;
        self.selected_body = Some(lo);
        self.rebuild_scene();
        Ok(lo)
    }

    /// Netz, Masse und Hülle eines Körpers berechnen (gemeinsam für Platzieren/Verschieben/Ergebnis).
    fn make_body(&self, kind: BodyKind, name: String, solid: Solid) -> Result<Body, String> {
        let mesh = self.mesh_of(&solid)?;
        let mass = mass_properties(&mesh);
        let bounds = bounds_of(&mesh);
        Ok(Body { name, kind, solid, mesh, mass, bounds })
    }
}
```

(`add_body` kann `make_body` ebenfalls verwenden; `bounds_of` und `mesh_of` sind bestehend. Der Fehlertext `NotClosed`/`EmptyResult` kommt aus `BooleanError::to_string()` und ist deutsch; der Test erwartet „leer“ bzw. „nicht unterstützt“, beides steht in den Meldungen.)

`boolean_dialog.rs`: Fenster „Boolesche Operation“ mit zwei `egui::ComboBox` (Körper A, Körper B aus `state.bodies`), drei Radioauswahlen (Vereinigen / Abziehen (A − B) / Schneiden), Schaltflächen „Ausführen“ und „Abbrechen“; „Ausführen“ ruft `state.boolean(a, b, op)` und setzt bei Fehler `state.message = Some(text)`:

```rust
use crate::state::AppState;
use planora_kernel::boolean::BoolOp;

#[derive(Clone, Debug, PartialEq)]
pub struct BooleanDialog {
    pub a: usize,
    pub b: usize,
    pub op: BoolOp,
}

impl BooleanDialog {
    pub fn new(state: &AppState) -> Self {
        let a = state.selected_body.unwrap_or(0);
        let b = (0..state.bodies.len()).find(|&i| i != a).unwrap_or(0);
        Self { a, b, op: BoolOp::Difference }
    }

    /// Zeigt den Dialog; `true`, solange er offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let (mut run, mut cancel) = (false, false);
        egui::Window::new("Boolesche Operation").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
            let names: Vec<String> = state.bodies.iter().map(|b| b.name.clone()).collect();
            for (label, slot) in [("Körper A", &mut self.a), ("Körper B", &mut self.b)] {
                egui::ComboBox::from_label(label)
                    .selected_text(names.get(*slot).cloned().unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(slot, i, n);
                        }
                    });
            }
            ui.radio_value(&mut self.op, BoolOp::Union, "Vereinigen");
            ui.radio_value(&mut self.op, BoolOp::Difference, "Abziehen (A − B)");
            ui.radio_value(&mut self.op, BoolOp::Intersection, "Schneiden");
            ui.horizontal(|ui| {
                if ui.button("Ausführen").clicked() {
                    run = true;
                }
                if ui.button("Abbrechen").clicked() {
                    cancel = true;
                }
            });
        });
        if run {
            state.message = None;
            if let Err(e) = state.boolean(self.a, self.b, self.op) {
                state.message = Some(e);
            }
            return false;
        }
        open && !cancel
    }
}
```

`app.rs`: neues Feld `boolean_dialog: Option<BooleanDialog>`; Menü „Bearbeiten“ mit „Vereinigen …“, „Abziehen …“, „Schneiden …“ (je öffnet den Dialog mit der entsprechenden Voreinstellung; nur aktiv bei ≥ 2 Körpern, sonst grau); Aufruf von `dialog.show(...)` analog zum Erzeugen-Dialog (`take()`-Muster).

`properties.rs`: unter „Abmessungen“ eine Zeile „Position (x/y/z)“ mit drei `DragValue` (mm, Schrittweite 0,5), initial `bounds.0`; bei Änderung `state.translate_body(index, neu − alt)`; Fehler in `state.message`. (Die Änderung der Position wird nach dem Zeichnen der Felder angewendet, nicht während die Körper-Referenz geliehen ist.)

`lib.rs`: `pub mod boolean_dialog;`.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-app` (alle Tests), `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build -p planora-app --release`.
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/planora-app
git commit -m "feat(app): Boolesche Operationen und Körper verschieben" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Eigenschafts-, Ketten- und Randfalltests; Abschluss

**Files:**
- Create: `crates/planora-mesh/tests/boolean_properties.rs`
- Modify: `docs/superpowers/offene-punkte.md`, `README.md`
- Test: `crates/planora-mesh/tests/boolean_properties.rs`

**Interfaces:**
- Consumes: alles bisherige.
- Produces: Property-Tests (Volumenbilanzen für zufällige Quader und Zylinder), Kettenoperationen, Dokumentation.

- [ ] **Step 1: Tests schreiben** – `crates/planora-mesh/tests/boolean_properties.rs`

```rust
use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;
use proptest::prelude::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn vol(s: &Solid) -> f64 {
    validate(s).expect("Ergebnis muss gültig sein");
    let m = tessellate(s, &TessellationOptions { chord_tolerance: 0.001 }).unwrap();
    assert!(is_watertight(&m, 1e-6));
    mass_properties(&m).volume
}

/// Operation ausführen; „nicht unterstützt“, „leer“ und Berührungsfälle sind erlaubte Ergebnisse.
fn try_op(a: &Solid, b: &Solid, op: BoolOp) -> Option<Solid> {
    match k().boolean(a, b, op) {
        Ok(s) => Some(s),
        Err(BooleanError::EmptyResult) | Err(BooleanError::UnsupportedIntersection { .. }) => None,
        Err(BooleanError::AmbiguousContact | BooleanError::AmbiguousClassification) => None,
        Err(e) => panic!("unerwarteter Fehler {e}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn box_box_volume_balance(
        sx in 2.0f64..12.0, sy in 2.0f64..12.0, sz in 2.0f64..12.0,
        dx in -9.0f64..9.0, dy in -9.0f64..9.0, dz in -9.0f64..9.0,
    ) {
        // allgemeine Lage: Versätze nie genau 0, damit keine bündigen Kanten entstehen
        let (dx, dy, dz) = (dx + 0.0137, dy + 0.0291, dz + 0.0113);
        let a = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
        let b = moved(&k().make_box(DVec3::new(sx, sy, sz)).unwrap(), DVec3::new(dx, dy, dz));
        let (va, vb) = (vol(&a), vol(&b));
        let u = try_op(&a, &b, BoolOp::Union);
        let i = try_op(&a, &b, BoolOp::Intersection);
        let d = try_op(&a, &b, BoolOp::Difference);
        let vi = i.as_ref().map(vol).unwrap_or(0.0);
        let vu = u.as_ref().map(vol).unwrap_or(va + vb);
        let vd = d.as_ref().map(vol).unwrap_or(if i.is_none() { va } else { 0.0 });
        prop_assert!((vu + vi - (va + vb)).abs() < 1e-6 * (va + vb), "V(A∪B)+V(A∩B) = {} ≠ {}", vu + vi, va + vb);
        prop_assert!((vd - (va - vi)).abs() < 1e-6 * va, "V(A−B) = {vd} ≠ {}", va - vi);
    }

    #[test]
    fn box_cylinder_volume_balance(
        r in 0.8f64..3.5, h in 3.0f64..14.0,
        x in 0.5f64..9.5, y in 0.5f64..9.5, z in -3.0f64..8.0,
    ) {
        let a = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
        let c = moved(&k().make_cylinder(r, h).unwrap(), DVec3::new(x + 0.0071, y + 0.0043, z + 0.0087));
        let (va, vb) = (vol(&a), vol(&c));
        let u = try_op(&a, &c, BoolOp::Union);
        let i = try_op(&a, &c, BoolOp::Intersection);
        let d = try_op(&a, &c, BoolOp::Difference);
        // nicht unterstützte Lagen (Zylinder schneidet eine Kante tangential usw.) werden übersprungen
        prop_assume!(u.is_some() && i.is_some() && d.is_some());
        let (vu, vi, vd) = (vol(&u.unwrap()), vol(&i.unwrap()), vol(&d.unwrap()));
        let tol = 0.01 * (va + vb);
        prop_assert!((vu + vi - (va + vb)).abs() < tol, "{vu} + {vi} ≠ {va} + {vb}");
        prop_assert!((vd - (va - vi)).abs() < tol, "{vd} ≠ {va} − {vi}");
        for (op, v) in [(BoolOp::Union, vu), (BoolOp::Difference, vd), (BoolOp::Intersection, vi)] {
            prop_assert!(boolean_volume_plausible(op, va, vb, v, 0.02));
        }
    }
}

#[test]
fn plate_with_three_holes_chain() {
    let mut s = k().make_box(DVec3::new(30.0, 12.0, 4.0)).unwrap();
    let mut expected = 30.0 * 12.0 * 4.0;
    for cx in [5.0, 15.0, 25.0] {
        let hole = moved(&k().make_cylinder(2.0, 8.0).unwrap(), DVec3::new(cx, 6.0, -2.0));
        s = k().boolean(&s, &hole, BoolOp::Difference).unwrap();
        expected -= PI * 4.0 * 4.0;
        let v = vol(&s);
        assert!(((v - expected) / expected).abs() < 0.01, "{v} vs {expected}");
    }
    assert_eq!(validate(&s).unwrap().genus, 3);
}

#[test]
fn union_then_difference_chain() {
    let base = k().make_box(DVec3::new(20.0, 20.0, 5.0)).unwrap();
    let post = moved(&k().make_cylinder(4.0, 15.0).unwrap(), DVec3::new(10.0, 10.0, 0.0));
    let s1 = k().boolean(&base, &post, BoolOp::Union).unwrap();
    let bore = moved(&k().make_cylinder(2.0, 20.0).unwrap(), DVec3::new(10.0, 10.0, -2.0));
    let s2 = k().boolean(&s1, &bore, BoolOp::Difference).unwrap();
    let expected = 20.0 * 20.0 * 5.0 + PI * 16.0 * 10.0 - PI * 4.0 * 15.0;
    let v = vol(&s2);
    assert!(((v - expected) / expected).abs() < 0.01, "{v} vs {expected}");
}

#[test]
fn tangent_cylinder_beside_a_box_does_not_panic() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let c = moved(&k().make_cylinder(2.0, 10.0).unwrap(), DVec3::new(12.0, 5.0, 0.0)); // berührt die Seitenfläche
    for op in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
        let _ = k().boolean(&a, &c, op); // Ok oder benannter Fehler, aber keine Panik
    }
}
```

- [ ] **Step 2: Test laufen lassen**

Run: `cargo test -p planora-mesh --test boolean_properties`
Expected: Die Tests laufen; Fehlschläge deuten auf echte Robustheitslücken. Behebe sie im Kernel (nicht im Test), außer der Test selbst enthält einen Fehler. Zufällig auftretende, nicht unterstützte oder mehrdeutige Lagen werden im Test bewusst übersprungen bzw. als erlaubter Fehler behandelt.

- [ ] **Step 3: Dokumentation und Abschluss**

`docs/superpowers/offene-punkte.md`: Abschnitt „Vor Teilprojekt 3 (Boolesche Operationen)“ als erledigt entfernen; neuer Abschnitt „Nach Teilprojekt 3“ mit den bewusst offen gelassenen Punkten: nicht unterstützte Flächenpaare (Raumkurven, schräge Kegel-/Torus-Ebenenschnitte), kein automatisches Zusammenfassen bündiger Flächen, Kanten-/Eckenberührung ohne Fläche liefert Fehler, Mehrdeutigkeiten bei Tangentialkontakt, Pick-Performance bei großen Netzen.
`README.md`: Statussatz um „Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit Flächen aus Ebenen, Zylindern, Kegeln, Kugeln und Tori (Stufe A)“ ergänzen.

Abschlussprüfung:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo build -p planora-app --release
```

Expected: alles grün (`cargo deny`: advisories/bans/licenses/sources ok).

- [ ] **Step 4: Commit**

```bash
git add .
git commit -m "test(mesh): Eigenschafts- und Kettentests für Boolesche Operationen; Doku" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Selbstprüfung gegen die Spec

- **§1/§2 Ziel, Stufe A, Ansatz „schlank“, `spade`:** Task 1–11; `spade` mit Lizenzprüfung in Task 9 Schritt 1.
- **§3 Architektur:** Module `intersect` (Task 2/3 `roots`, `surfsurf`), `split` (Task 5), `classify` (Task 7), `select`/`sew` (Task 8), Kernel-Grundlagen (Task 1), Netz (Task 9), Oberfläche (Task 10). Die Datei `intersect` aus der Spec ist hier in `roots.rs` (Kurve↔Fläche) und `surfsurf.rs` (Fläche↔Fläche) aufgeteilt; `imprint.rs` (Task 6) ist neu und fasst den Beschnitt der Schnittkurven zusammen.
- **§4 Schnitte, Tabelle und Sonderfälle:** Task 3 (alle Paare der Tabelle, `Touch`, `Coincident`, `Unsupported`) und Task 6 (Beschnitt, bündige Paare).
- **§5 Auftrennen inkl. Naht/Pole:** Task 4 (Parameterraum mit Naht und Pol-Strecken) und Task 5 (Graph mit Drehsystem, Tests für Naht, Kugel, Löcher).
- **§6 Einordnen, Auswahltabelle, Zusammennähen:** Task 7, Task 8; Volumenprobe in `planora-mesh` (Task 9) und in der App (Task 10), wie in der Spec festgehalten.
- **§7 Validator und Netz:** Task 8 (Euler je Schale), Task 9 (beschnittene Flächen).
- **§8 Toleranz:** durchgängig `Tolerance`; exakte Prädikate über `orient2d` in `winding`.
- **§9 Oberfläche:** Task 10 (Dialog, Verschieben, Fehlertexte; Ergebnis bleibt an der Stelle, ein Körper mit mehreren Schalen).
- **§10 Tests:** Eigenschaftstests und Volumenbilanzen (Task 11), gezielte Fälle (Task 8/9), Ketten (Task 8/11), Schnittfunktionen (Task 3), Netz (Task 9).
- **Typkonsistenz:** `FaceGeom`, `Region`, `Seg`, `Piece`, `FaceWork`, `Class`, `Location`, `BoolOp`, `BooleanError` werden in Task 2–10 mit denselben Namen und Signaturen verwendet; `classify_region(&FaceGeom, &Region, &[FaceGeom], &Tolerance)` in Task 7 und 8 identisch; `Kernel::boolean(&self, a, b, op)` in Task 8 und 10 identisch.
- **Review Focus:** (1) Task 8 `box_stacked_flush_on_a_box`, `flush_and_touching_boxes`, Task 9 `box_minus_flush_cylinder_of_equal_height`; (2) Task 8 `identical_operands`; (3) Task 8 `contained_box...`, `disjoint_boxes`, `boxes_touching_at_a_face`, Task 9 `sphere_inside_a_box_makes_a_cavity`; (4) Task 8 `boxes_touching_only_at_an_edge_do_not_panic`, Task 11 `tangent_cylinder_beside_a_box_does_not_panic`; (5) Task 9 `unsupported_pair_is_a_clear_error`, Task 8/11 Ketten.
- **Bewusst offene Feinheiten (nicht Spec-widrig):** Die Abtast- und Rechengrenzen in `roots.rs`, `imprint.rs` und `classify.rs` (z. B. 64 Stützstellen pro 2π, 24×24 Innenpunkte) sind benannte Konstanten im jeweiligen Modul und gehören nicht zur zentralen `Tolerance`; sie dürfen beim Implementieren nachgeschärft werden, wenn ein Test es verlangt.
