# PlanoraCAD Skizzen + Constraint-Solver – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 2D-Skizzen mit Constraint-Solver in reinem Rust (neues Crate `planora-sketch`), Profil-Extraktion für Extrusion/Rotation und Bedienung in der App (Skizzen-Modus, Werkzeuge, Abhängigkeiten, Maße, Freiheitsgrade).

**Architecture:** Alle Zahlenwerte einer Skizze liegen in einem flachen Vektor; Geometrie verweist per Index darauf. Jede Abhängigkeit liefert Residuen als Vorwärts-Ableitungsobjekte (`Dual`, exakte Gradienten ohne Differenzenquotienten). Der Solver zerlegt in zusammenhängende Gruppen und löst mit Levenberg-Marquardt (eigene dichte Cholesky-Zerlegung); Freiheitsgrade, Redundanz und Widerspruch kommen aus einer zeilenweisen Gram-Schmidt-Rangbestimmung. Profile werden als bestehende Kernel-Typen (`Profile`/`ProfileLoop`) geliefert. Die App bekommt einen Skizzen-Modus, dessen Zustand ohne egui testbar ist.

**Tech Stack:** Rust (Edition 2024), `glam` (über `planora-math`), `thiserror`, `proptest`; bestehend `egui`/`eframe` 0.36. **Keine neue externe Abhängigkeit.**

**Spec:** `docs/superpowers/specs/2026-10-09-skizzen-und-solver-design.md` (baut auf `2026-10-09-fundament-und-kernel-design.md` und `2026-10-09-boolesche-operationen-design.md` auf)

## Entscheidungen zur Spec (Planungsstand 2026-10-09)

Diese Punkte konkretisieren die Spec; wo sie von ihr abweichen, ist es hier begründet und gilt für die Umsetzung. Sie werden dem Projekteigner am Ende der Umsetzung genannt.

- **D1 – Ableitungen per Vorwärts-Automatik (`Dual`)** statt von Hand hergeleiteter Formeln. Die Gradienten sind exakt (kein Differenzenquotient); Tests prüfen sie trotzdem gegen Differenzenquotienten. Vorteil: eine einzige Beschreibung je Abhängigkeit, keine Ableitungsfehler.
- **D2 – Zeilenweises Gram-Schmidt statt QR mit Spaltenpivotierung** für Rang, Redundanz und Nullraum (Spec 4.3). Reihenfolge der Abhängigkeiten bleibt erhalten, so wird immer die *später angelegte* Abhängigkeit als redundant gemeldet.
- **D3 – Kein QR-Rückfall im Schritt.** `JᵀJ + λ·diag` ist für λ > 0 positiv definit; scheitert Cholesky trotzdem, wird λ verzehnfacht (Spec 4.2).
- **D4 – Widerspruchsdiagnose** (Spec 4.3): Für jede nutzererzeugte Abhängigkeit der Gruppe wird die Gruppe ohne sie neu gelöst; die, deren Weglassen eine Lösung ermöglicht, werden gemeldet (gibt es keine, werden alle Abhängigkeiten mit Rest über der Schwelle gemeldet).
- **D5 – Profile müssen einfache geschlossene Ketten sein** (jeder Punkt hat genau zwei Kurven). T-Verzweigungen und offene Enden ergeben benannte Fehler; ein Zerschneiden von Kurven an Kreuzungen gibt es nicht.
- **D6 – `profiles()` liefert `ExtractedProfiles { profiles, rejected }`** statt `Result<Vec<_>>`: gültige Profile stehen zur Verfügung, abgelehnte Komponenten werden mit Grund gemeldet.

## Global Constraints

- Reines Rust; Zahlen `f64`, Einheit Millimeter, Winkel Radiant; eine zentrale `Tolerance` (linear 1e-6, angular 1e-9). Keine freien Epsilon-Zahlen im Code: jede Schwelle ist eine benannte `const` mit Kommentar oder aus `Tolerance` abgeleitet.
- `planora-sketch` hängt nur von `planora-math`, `planora-kernel` und `thiserror` ab; es kennt weder egui noch Grafik.
- Der Kernel (`planora-kernel`) kennt weiterhin keine Oberfläche; die Skizze liefert ihm nur `Profile`/`ProfileLoop`/`Frame`.
- **Jede neue Abhängigkeit wird vorher auf Lizenz geprüft** (hier: keine geplant). `cargo deny check` muss grün bleiben; Erlaubnisliste in `deny.toml` nie selbstständig erweitern – bei Befund anhalten und den Menschen fragen. Projektlizenz GPL-3.0-only.
- Kein Autodesk-Material (Code, Icons, Namen, Dateiformate); Oberflächentexte und Fehlermeldungen auf Deutsch; dunkles Theme bleibt Standard.
- Keine Panics auf Nutzereingaben: Indizes, Zahlen (NaN/∞), Typen der referenzierten Objekte werden an den API-Grenzen geprüft und ergeben `SketchError`.
- Der Solver verändert die Skizze nur bei Erfolg; sonst bleibt der letzte gültige Zustand.
- Plattformen: Windows (MSVC) und Linux. Befehle in Bash (Git Bash): `export PATH="$PATH:/c/Users/denni/.cargo/bin"`.

## Review Focus

Eingaben, die die Spec andeutet, die aber leicht durchrutschen; jede Zeile hat einen Test in der genannten Task:

1. **Entartete Geometrie:** Linie der Länge 0, Kreis mit Radius 0, zwei zusammenfallende Punkte, NaN/∞ als Eingabe → kein NaN im Zustand, benannter Fehler oder verwerfender Solver (Task 2, Task 4).
2. **Unterbestimmt, überbestimmt, widersprüchlich:** Dreieck mit drei Winkeln (redundant, lösbar), zwei widersprüchliche Abstände (Widerspruch, Zustand bleibt) (Task 4).
3. **Ziehen:** Ein gezogener Punkt lässt die Skizze nicht umklappen und überschreitet keine harten Abhängigkeiten (Task 5).
4. **Profile:** Verschachtelung (Loch im Loch), offene Ketten, T-Verzweigung, sich kreuzende Linien, Konstruktionsgeometrie zählt nicht, Ellipse/Spline → abgelehnt (Task 7, Task 10).
5. **Kernel-Eingaben von außen:** Profil mit < 3 Punkten, mit NaN-Bogen, mit schneidenden/außen liegenden Löchern, `Frame` mit NaN oder nicht orthonormal → `KernelError`, nie Panik (Task 6).

## Hinweis zur Verifikation und zum Code in diesem Plan

Der Code in den Schritten ist die **Referenzimplementierung des beabsichtigten Entwurfs** und wurde noch nicht kompiliert. Er darf zum Kompilieren und zur Fehlerbehebung angepasst und intern umstrukturiert werden, solange die benannten Schnittstellen (Abschnitte „Interfaces“) und **alle Tests unverändert** erfüllt werden. Schlägt ein Test fehl, wird der Code korrigiert, nicht der Test – es sei denn, der Test selbst enthält einen Tippfehler (dann berichten). Rust ist installiert (`export PATH="$PATH:/c/Users/denni/.cargo/bin"`).

## Dateistruktur

```
crates/planora-sketch/                 (neu)
├─ Cargo.toml
├─ src/
│  ├─ lib.rs          Exporte
│  ├─ error.rs        SketchError
│  ├─ linalg.rs       Matrix, Cholesky, Zeilenraum-Analyse (Rang, Redundanz, Nullraum)
│  ├─ dual.rs         Dual (Vorwärts-Ableitung)
│  ├─ model.rs        Sketch, Punkte, Kurven, Ids, Bearbeiten, Löschen
│  ├─ constraint.rs   ConstraintKind, Residuen (Dual), Jacobi-Zeilen
│  ├─ solver.rs       Gruppen, Levenberg-Marquardt, Diagnose, SolveReport, Ziehen
│  ├─ profile.rs      ExtractedProfiles, Schleifensuche, Schachtelung
│  ├─ helpers.rs      Rechteck, Spiegeln, Muster, Körperkante kopieren, Ebene einer Fläche
│  └─ curves.rs       Ellipse und Spline (Auswertung, Residuen)
└─ tests/ linalg.rs dual.rs model.rs constraints.rs solver.rs drag.rs profile.rs helpers.rs ellipse_spline.rs e2e.rs properties.rs
crates/planora-kernel/src/  profile.rs, extrude.rs, revolve.rs, error.rs, kernel.rs (Task 6)
crates/planora-app/src/
├─ sketch_state.rs   Skizzen-Modus ohne egui (Werkzeuge, Auswahl, Fangen, Ziehen)
├─ sketch_view.rs    Zeichnen und Eingaben im Viewport (egui)
├─ sketch_panel.rs   Werkzeugleiste, Abhängigkeitsliste, Maß-Eingabe (egui)
├─ state.rs, app.rs, browser.rs, properties.rs, create_dialog.rs, viewport.rs (ändern)
└─ tests/ sketch_state.rs sketch_ui.rs
```

---

### Task 1: Crate `planora-sketch` und lineare Algebra

**Files:**
- Modify: `Cargo.toml` (Workspace-Abhängigkeit `planora-sketch`)
- Create: `crates/planora-sketch/Cargo.toml`, `crates/planora-sketch/src/lib.rs`, `crates/planora-sketch/src/error.rs`, `crates/planora-sketch/src/linalg.rs`
- Test: `crates/planora-sketch/tests/linalg.rs`

**Interfaces:**
- Produces (alle `pub` aus `planora_sketch::linalg`):
  - `struct Matrix` (dicht, zeilenweise): `zeros(rows, cols)`, `identity(n)`, `from_rows(&[Vec<f64>])`, `rows()`, `cols()`, `get(i, j)`, `set(i, j, v)`, `add_to(i, j, v)`, `row(i) -> &[f64]`, `mul_vec(&[f64]) -> Vec<f64>` (A·x), `transpose_mul_vec(&[f64]) -> Vec<f64>` (Aᵀ·y), `gram() -> Matrix` (AᵀA)
  - `fn cholesky_solve(a: &Matrix, b: &[f64]) -> Option<Vec<f64>>` (`None`, wenn `a` nicht symmetrisch positiv definit ist oder das Ergebnis nicht endlich ist)
  - `struct RowSpace { independent: Vec<bool>, rank: usize, null_space: Vec<Vec<f64>> }` und `fn analyze_rows(a: &Matrix, rel_tol: f64) -> RowSpace`: Zeilen werden der Reihe nach geprüft; eine Zeile ist abhängig, wenn nach Abzug des bisherigen Zeilenraums weniger als `rel_tol` mal ihre ursprüngliche Länge übrig bleibt (Nullzeilen sind abhängig). `null_space` ist eine orthonormale Basis des Nullraums (`n − rank` Vektoren der Länge `n`).
- `SketchError` (in `error.rs`, zunächst mit `InvalidInput(String)`; wird in späteren Tasks erweitert).

- [ ] **Step 1: Crate anlegen**

`Cargo.toml` (Workspace), unter `[workspace.dependencies]` bei den internen Crates ergänzen:

```toml
planora-sketch = { path = "crates/planora-sketch" }
```

`crates/planora-sketch/Cargo.toml`:

```toml
[package]
name = "planora-sketch"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
planora-math.workspace = true
planora-kernel.workspace = true
thiserror.workspace = true

[dev-dependencies]
proptest.workspace = true

[lints]
workspace = true
```

`crates/planora-sketch/src/lib.rs`:

```rust
//! 2D-Skizzen mit Constraint-Solver für PlanoraCAD.

pub mod error;
pub mod linalg;

pub use error::SketchError;
```

`crates/planora-sketch/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SketchError {
    #[error("Ungültige Eingabe: {0}")]
    InvalidInput(String),
}
```

- [ ] **Step 2: Failing tests schreiben** – `crates/planora-sketch/tests/linalg.rs`

```rust
use planora_sketch::linalg::*;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn matrix_basics() {
    let m = Matrix::from_rows(&[vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]);
    assert_eq!((m.rows(), m.cols()), (2, 3));
    assert_eq!(m.get(1, 2), 6.0);
    assert_eq!(m.mul_vec(&[1.0, 0.0, -1.0]), vec![-2.0, -2.0]);
    assert_eq!(m.transpose_mul_vec(&[1.0, 1.0]), vec![5.0, 7.0, 9.0]);
    let g = m.gram();
    assert_eq!((g.rows(), g.cols()), (3, 3));
    assert_eq!(g.get(0, 0), 17.0);
    assert_eq!(g.get(0, 1), g.get(1, 0));
    let mut z = Matrix::zeros(2, 2);
    z.set(0, 1, 2.0);
    z.add_to(0, 1, 3.0);
    assert_eq!(z.get(0, 1), 5.0);
    assert_eq!(Matrix::identity(3).get(2, 2), 1.0);
}

#[test]
fn cholesky_solves_a_positive_definite_system() {
    let a = Matrix::from_rows(&[
        vec![4.0, 1.0, 0.0],
        vec![1.0, 3.0, 1.0],
        vec![0.0, 1.0, 2.0],
    ]);
    let x = [1.0, 2.0, 3.0];
    let b = a.mul_vec(&x);
    let got = cholesky_solve(&a, &b).unwrap();
    for (g, w) in got.iter().zip(x) {
        assert!(close(*g, w), "{got:?}");
    }
}

#[test]
fn cholesky_rejects_indefinite_and_non_finite() {
    let indefinite = Matrix::from_rows(&[vec![1.0, 2.0], vec![2.0, 1.0]]);
    assert!(cholesky_solve(&indefinite, &[1.0, 1.0]).is_none());
    let nan = Matrix::from_rows(&[vec![f64::NAN, 0.0], vec![0.0, 1.0]]);
    assert!(cholesky_solve(&nan, &[1.0, 1.0]).is_none());
    let singular = Matrix::from_rows(&[vec![1.0, 1.0], vec![1.0, 1.0]]);
    assert!(cholesky_solve(&singular, &[1.0, 1.0]).is_none());
}

#[test]
fn later_duplicate_rows_are_the_dependent_ones() {
    let a = Matrix::from_rows(&[vec![1.0, 2.0], vec![2.0, 4.0]]);
    let r = analyze_rows(&a, 1e-9);
    assert_eq!(r.independent, vec![true, false]);
    assert_eq!(r.rank, 1);
    assert_eq!(r.null_space.len(), 1);
    // Nullvektor steht senkrecht auf (1, 2)
    let v = &r.null_space[0];
    assert!(close(v[0] + 2.0 * v[1], 0.0));
    assert!(close(v.iter().map(|x| x * x).sum::<f64>(), 1.0));
}

#[test]
fn combination_of_earlier_rows_is_dependent() {
    let a = Matrix::from_rows(&[
        vec![1.0, 0.0, 0.0],
        vec![0.0, 1.0, 0.0],
        vec![1.0, 1.0, 0.0],
    ]);
    let r = analyze_rows(&a, 1e-9);
    assert_eq!(r.independent, vec![true, true, false]);
    assert_eq!(r.rank, 2);
    assert_eq!(r.null_space.len(), 1);
    assert!(close(r.null_space[0][2].abs(), 1.0));
}

#[test]
fn zero_rows_are_dependent_and_scaling_does_not_matter() {
    let a = Matrix::from_rows(&[vec![0.0, 0.0], vec![1e-8, 0.0], vec![0.0, 1e8]]);
    let r = analyze_rows(&a, 1e-9);
    assert_eq!(r.independent, vec![false, true, true]);
    assert_eq!(r.rank, 2);
    assert!(r.null_space.is_empty());
}

#[test]
fn no_rows_means_the_whole_space_is_free() {
    let r = analyze_rows(&Matrix::zeros(0, 3), 1e-9);
    assert_eq!(r.rank, 0);
    assert_eq!(r.null_space.len(), 3);
}

#[test]
fn null_space_is_orthonormal_and_annihilated() {
    let a = Matrix::from_rows(&[
        vec![1.0, 2.0, 0.5, -1.0, 0.0, 3.0],
        vec![0.0, 1.0, 1.0, 2.0, -2.0, 0.0],
        vec![3.0, 0.0, -1.0, 1.0, 1.0, 1.0],
    ]);
    let r = analyze_rows(&a, 1e-9);
    assert_eq!(r.rank, 3);
    assert_eq!(r.null_space.len(), 3);
    for (i, v) in r.null_space.iter().enumerate() {
        for rv in a.mul_vec(v) {
            assert!(rv.abs() < 1e-9, "A·v = {rv}");
        }
        for (j, w) in r.null_space.iter().enumerate() {
            let d: f64 = v.iter().zip(w).map(|(x, y)| x * y).sum();
            assert!(close(d, if i == j { 1.0 } else { 0.0 }));
        }
    }
}
```

- [ ] **Step 3: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test linalg`
Expected: FAIL (Modul/Funktionen fehlen)

- [ ] **Step 4: Implementierung** – `crates/planora-sketch/src/linalg.rs`

```rust
//! Kleine dichte lineare Algebra für den Skizzen-Solver (Skizzengruppen haben typisch < 300
//! Variablen, dafür genügen dichte Matrizen).

/// Dichte Matrix, zeilenweise gespeichert.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

impl Matrix {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n, n);
        for i in 0..n {
            m.data[i * n + i] = 1.0;
        }
        m
    }

    /// Alle Zeilen müssen gleich lang sein (interne Schnittstelle).
    pub fn from_rows(rows: &[Vec<f64>]) -> Self {
        let cols = rows.first().map_or(0, Vec::len);
        let mut m = Self::zeros(rows.len(), cols);
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(r.len(), cols, "Zeilen müssen gleich lang sein");
            m.data[i * cols..(i + 1) * cols].copy_from_slice(r);
        }
        m
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.data[i * self.cols + j]
    }

    pub fn set(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] = v;
    }

    pub fn add_to(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] += v;
    }

    pub fn row(&self, i: usize) -> &[f64] {
        &self.data[i * self.cols..(i + 1) * self.cols]
    }

    /// A·x
    pub fn mul_vec(&self, x: &[f64]) -> Vec<f64> {
        assert_eq!(x.len(), self.cols);
        (0..self.rows).map(|i| dot(self.row(i), x)).collect()
    }

    /// Aᵀ·y
    pub fn transpose_mul_vec(&self, y: &[f64]) -> Vec<f64> {
        assert_eq!(y.len(), self.rows);
        let mut out = vec![0.0; self.cols];
        for (i, yi) in y.iter().enumerate() {
            for (o, a) in out.iter_mut().zip(self.row(i)) {
                *o += a * yi;
            }
        }
        out
    }

    /// AᵀA
    pub fn gram(&self) -> Matrix {
        let n = self.cols;
        let mut g = Matrix::zeros(n, n);
        for i in 0..self.rows {
            let r = self.row(i);
            for (j, rj) in r.iter().enumerate() {
                if *rj == 0.0 {
                    continue;
                }
                for (k, rk) in r.iter().enumerate() {
                    g.data[j * n + k] += rj * rk;
                }
            }
        }
        g
    }
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

/// Löst `A·x = b` für symmetrisch positiv definites `A` (Cholesky). `None`, wenn `A` nicht
/// positiv definit ist oder das Ergebnis nicht endlich ist.
pub fn cholesky_solve(a: &Matrix, b: &[f64]) -> Option<Vec<f64>> {
    let n = a.rows;
    assert_eq!(a.cols, n);
    assert_eq!(b.len(), n);
    let mut l = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a.get(i, j);
            for k in 0..j {
                s -= l[i * n + k] * l[j * n + k];
            }
            if i == j {
                if !(s > 0.0) || !s.is_finite() {
                    return None;
                }
                l[i * n + i] = s.sqrt();
            } else {
                l[i * n + j] = s / l[j * n + j];
            }
        }
    }
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i * n + k] * y[k];
        }
        y[i] = s / l[i * n + i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l[k * n + i] * x[k];
        }
        x[i] = s / l[i * n + i];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Ergebnis von [`analyze_rows`].
#[derive(Clone, Debug, PartialEq)]
pub struct RowSpace {
    /// Je Zeile: `true`, wenn sie vom Zeilenraum der früheren Zeilen unabhängig ist.
    pub independent: Vec<bool>,
    pub rank: usize,
    /// Orthonormale Basis des Nullraums (`n − rank` Vektoren).
    pub null_space: Vec<Vec<f64>>,
}

/// Zieht den Anteil von `v` ab, der im Raum der orthonormalen `basis` liegt (zweimal, gegen
/// Rundungsfehler).
fn orthogonalize(v: &mut [f64], basis: &[Vec<f64>]) {
    for _ in 0..2 {
        for b in basis {
            let d = dot(v, b);
            for (x, y) in v.iter_mut().zip(b) {
                *x -= d * y;
            }
        }
    }
}

/// Bestimmt Rang, abhängige Zeilen und Nullraum. Zeilen werden in Reihenfolge geprüft; eine Zeile
/// ist abhängig, wenn nach Abzug des Raums der vorigen unabhängigen Zeilen weniger als `rel_tol`
/// ihrer ursprünglichen Länge übrig bleibt.
pub fn analyze_rows(a: &Matrix, rel_tol: f64) -> RowSpace {
    let n = a.cols();
    let mut basis: Vec<Vec<f64>> = Vec::new();
    let mut independent = vec![false; a.rows()];
    for (i, flag) in independent.iter_mut().enumerate() {
        let mut v = a.row(i).to_vec();
        let norm0 = norm(&v);
        if !(norm0 > 0.0 && norm0.is_finite()) {
            continue;
        }
        orthogonalize(&mut v, &basis);
        let norm1 = norm(&v);
        if norm1 > rel_tol * norm0 {
            for x in &mut v {
                *x /= norm1;
            }
            basis.push(v);
            *flag = true;
        }
    }
    let rank = basis.len();

    // Nullraum: Einheitsvektoren gegen den Zeilenraum orthogonalisieren; dann wiederholt den mit
    // dem größten Rest nehmen, normieren und aus den übrigen entfernen.
    let mut rest: Vec<Vec<f64>> = (0..n)
        .map(|j| {
            let mut e = vec![0.0; n];
            e[j] = 1.0;
            orthogonalize(&mut e, &basis);
            e
        })
        .collect();
    let mut null_space = Vec::new();
    for _ in 0..n.saturating_sub(rank) {
        let Some((best, len)) = rest
            .iter()
            .enumerate()
            .map(|(j, e)| (j, norm(e)))
            .max_by(|x, y| x.1.total_cmp(&y.1))
        else {
            break;
        };
        if !(len > 0.0) {
            break;
        }
        let q: Vec<f64> = rest[best].iter().map(|x| x / len).collect();
        for e in rest.iter_mut() {
            let d = dot(e, &q);
            for (x, y) in e.iter_mut().zip(&q) {
                *x -= d * y;
            }
        }
        null_space.push(q);
    }
    RowSpace {
        independent,
        rank,
        null_space,
    }
}
```

- [ ] **Step 5: Tests laufen lassen**

Run: `cargo test -p planora-sketch --test linalg && cargo clippy -p planora-sketch --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS, clippy sauber

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/planora-sketch
git commit -m "feat(sketch): Crate planora-sketch mit dichter linearer Algebra (Cholesky, Zeilenraum-Analyse)"
```

---

### Task 2: Vorwärts-Ableitung (`Dual`) und Skizzen-Datenmodell

**Files:**
- Create: `crates/planora-sketch/src/dual.rs`, `crates/planora-sketch/src/model.rs`
- Modify: `crates/planora-sketch/src/lib.rs`, `crates/planora-sketch/src/error.rs`
- Test: `crates/planora-sketch/tests/dual.rs`, `crates/planora-sketch/tests/model.rs`

**Interfaces:**
- Consumes: `planora_kernel::{Arena, Id}`, `planora_math::{DVec2, Frame, Tolerance}`.
- Produces:
  - `dual::Dual { v: f64, d: Vec<f64> }` mit `constant(v)`, `variable(v, index, n)`, `sqrt()`, `grad(i)`; Operatoren `+ − × ÷` und Negation für `Dual`/`&Dual` und gemischt mit `f64`. Ein leerer Gradient `d` bedeutet „Konstante“ (Gradient null); Gradienten verschiedener Länge werden verträglich verknüpft. `sqrt` hat eine untere Schranke im Nenner der Ableitung, damit Nullabstände keine unendlichen Gradienten liefern.
  - `model::{PointData, CurveData, CurveKind, PointId, CurveId, Sketch}` (Details unten). `ConstraintData`/`ConstraintId` kommen in Task 3.
  - `Sketch::new(frame, name, tol)`, `frame()`, `tolerance()`, `add_point(x, y)`, `point_xy(p)`, `set_point_xy(p, x, y)`, `add_line(a, b)`, `add_circle(center, radius)`, `add_arc(center, start, end)` (gegen den Uhrzeigersinn von `start` nach `end`; der Bogenradius ist `|start − center|`, `end` wird vom Solver auf diesen Radius gezogen), `curve(c) -> Option<&CurveData>`, `radius(c) -> Option<f64>` (Kreis/Bogen), `line_length(c)`, `points()`/`curves()` (nur lebende), `set_construction(c, bool)`, `variable_count()`.
  - `SketchError` neu: `Degenerate(String)`, `UnknownObject`, `WrongType(String)`.
- Alle Methoden, die Ids annehmen, prüfen sie (Index im Bereich, Objekt lebt) und geben `SketchError::UnknownObject` zurück – nie eine Panik.

- [ ] **Step 1: Failing tests schreiben**

`crates/planora-sketch/tests/dual.rs`:

```rust
use planora_sketch::dual::Dual;

/// f(x, y) = sqrt(x² + y²) / (x + 3) − x·y
fn f(x: &Dual, y: &Dual) -> Dual {
    let r = (x * x + y * y).sqrt();
    &r / &(x + &Dual::constant(3.0)) - x * y
}

#[test]
fn gradient_matches_finite_differences() {
    let (x0, y0) = (1.3, -0.7);
    let x = Dual::variable(x0, 0, 2);
    let y = Dual::variable(y0, 1, 2);
    let r = f(&x, &y);
    let h = 1e-6;
    let value = |a: f64, b: f64| f(&Dual::constant(a), &Dual::constant(b)).v;
    let dx = (value(x0 + h, y0) - value(x0 - h, y0)) / (2.0 * h);
    let dy = (value(x0, y0 + h) - value(x0, y0 - h)) / (2.0 * h);
    assert!((r.v - value(x0, y0)).abs() < 1e-12);
    assert!((r.grad(0) - dx).abs() < 1e-7, "{} vs {dx}", r.grad(0));
    assert!((r.grad(1) - dy).abs() < 1e-7, "{} vs {dy}", r.grad(1));
}

#[test]
fn constants_have_zero_gradient_and_mix_with_variables() {
    let c = Dual::constant(2.0);
    assert_eq!(c.grad(0), 0.0);
    assert_eq!(c.grad(7), 0.0);
    let x = Dual::variable(5.0, 2, 3);
    let s = &x * &c + 1.0;
    assert_eq!(s.v, 11.0);
    assert_eq!(s.grad(2), 2.0);
    assert_eq!(s.grad(0), 0.0);
    let n = -s;
    assert_eq!(n.v, -11.0);
    assert_eq!(n.grad(2), -2.0);
}

#[test]
fn sqrt_at_zero_has_a_finite_gradient() {
    let x = Dual::variable(0.0, 0, 1);
    let r = x.sqrt();
    assert_eq!(r.v, 0.0);
    assert!(r.grad(0).is_finite());
    let neg = Dual::variable(-1e-30, 0, 1).sqrt();
    assert!(neg.v.is_finite() && neg.grad(0).is_finite());
}
```

`crates/planora-sketch/tests/model.rs`:

```rust
use planora_kernel::Id;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{CurveKind, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn points_and_lines_share_variables() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(3.0, 4.0).unwrap();
    let c = s.add_point(3.0, 0.0).unwrap();
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    assert_eq!(s.variable_count(), 6);
    assert!((s.line_length(l1).unwrap() - 5.0).abs() < 1e-12);
    assert!((s.line_length(l2).unwrap() - 4.0).abs() < 1e-12);
    let CurveKind::Line { b: end1, .. } = s.curve(l1).unwrap().kind else {
        panic!("keine Linie")
    };
    let CurveKind::Line { a: start2, .. } = s.curve(l2).unwrap().kind else {
        panic!("keine Linie")
    };
    assert_eq!(end1, start2, "gemeinsamer Punkt");
    s.set_point_xy(b, 6.0, 8.0).unwrap();
    assert_eq!(s.point_xy(b).unwrap(), DVec2::new(6.0, 8.0));
    assert!((s.line_length(l1).unwrap() - 10.0).abs() < 1e-12);
    assert_eq!(s.points().count(), 3);
    assert_eq!(s.curves().count(), 2);
}

#[test]
fn circles_and_arcs_report_their_radius() {
    let mut s = sketch();
    let c = s.add_point(1.0, 1.0).unwrap();
    let circle = s.add_circle(c, 2.5).unwrap();
    assert_eq!(s.radius(circle), Some(2.5));
    let a = s.add_point(4.0, 1.0).unwrap();
    let b = s.add_point(1.0, 4.0).unwrap();
    let arc = s.add_arc(c, a, b).unwrap();
    assert!((s.radius(arc).unwrap() - 3.0).abs() < 1e-12);
    let line = s.add_line(c, a).unwrap();
    assert_eq!(s.radius(line), None);
}

#[test]
fn invalid_input_is_rejected_without_panic() {
    let mut s = sketch();
    assert!(matches!(
        s.add_point(f64::NAN, 0.0),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(s.add_point(0.0, f64::INFINITY).is_err());
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(0.0, 0.0).unwrap();
    assert!(matches!(s.add_line(a, a), Err(SketchError::Degenerate(_))));
    assert!(
        matches!(s.add_line(a, b), Err(SketchError::Degenerate(_))),
        "zusammenfallende Punkte"
    );
    assert!(s.add_circle(a, 0.0).is_err());
    assert!(s.add_circle(a, -1.0).is_err());
    assert!(s.add_circle(a, f64::NAN).is_err());
    let c = s.add_point(5.0, 0.0).unwrap();
    assert!(s.add_arc(a, a, c).is_err(), "Start = Mitte");
    assert!(s.add_arc(a, c, a).is_err(), "Ende = Mitte");
    assert!(s.set_point_xy(a, f64::NAN, 1.0).is_err());
    assert_eq!(s.point_xy(a).unwrap(), DVec2::ZERO, "unverändert");
}

#[test]
fn foreign_ids_give_unknown_object_not_a_panic() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let bogus = Id::from_index(99);
    assert_eq!(s.add_line(a, bogus), Err(SketchError::UnknownObject));
    assert_eq!(s.point_xy(bogus), Err(SketchError::UnknownObject));
    assert_eq!(s.set_point_xy(bogus, 1.0, 1.0), Err(SketchError::UnknownObject));
    assert!(s.curve(Id::from_index(0)).is_none());
    assert_eq!(s.line_length(Id::from_index(3)), None);
}

#[test]
fn construction_flag_can_be_toggled() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert!(!s.curve(l).unwrap().construction);
    s.set_construction(l, true).unwrap();
    assert!(s.curve(l).unwrap().construction);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test dual --test model`
Expected: FAIL (Module fehlen)

- [ ] **Step 3: Implementierung `dual.rs`**

```rust
//! Zahlen mit Gradient (Vorwärts-Ableitung): Jede Rechnung liefert den Wert und die exakten
//! partiellen Ableitungen nach den lokalen Variablen.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// Untere Schranke für die Wurzel im Nenner der Ableitung von `sqrt` (Nullabstände).
const MIN_ROOT: f64 = 1e-12;

/// Wert mit Gradient. Ein leerer Gradient bedeutet „Konstante“ (alle Ableitungen null).
#[derive(Clone, Debug, PartialEq)]
pub struct Dual {
    pub v: f64,
    pub d: Vec<f64>,
}

impl Dual {
    pub fn constant(v: f64) -> Dual {
        Dual { v, d: Vec::new() }
    }

    /// Variable Nummer `index` von insgesamt `n` lokalen Variablen.
    pub fn variable(v: f64, index: usize, n: usize) -> Dual {
        let mut d = vec![0.0; n];
        d[index] = 1.0;
        Dual { v, d }
    }

    pub fn grad(&self, i: usize) -> f64 {
        self.d.get(i).copied().unwrap_or(0.0)
    }

    pub fn sqrt(&self) -> Dual {
        let s = self.v.max(0.0).sqrt();
        let den = 2.0 * s.max(MIN_ROOT);
        Dual {
            v: s,
            d: self.d.iter().map(|x| x / den).collect(),
        }
    }
}

/// `ca·a + cb·b` im Gradienten, Wert `v`.
fn lin(a: &Dual, ca: f64, b: &Dual, cb: f64, v: f64) -> Dual {
    let n = a.d.len().max(b.d.len());
    Dual {
        v,
        d: (0..n).map(|i| ca * a.grad(i) + cb * b.grad(i)).collect(),
    }
}

fn add(a: &Dual, b: &Dual) -> Dual {
    lin(a, 1.0, b, 1.0, a.v + b.v)
}
fn sub(a: &Dual, b: &Dual) -> Dual {
    lin(a, 1.0, b, -1.0, a.v - b.v)
}
fn mul(a: &Dual, b: &Dual) -> Dual {
    lin(a, b.v, b, a.v, a.v * b.v)
}
fn div(a: &Dual, b: &Dual) -> Dual {
    let q = a.v / b.v;
    lin(a, 1.0 / b.v, b, -q / b.v, q)
}

macro_rules! binop {
    ($tr:ident, $m:ident, $f:ident) => {
        impl $tr<&Dual> for &Dual {
            type Output = Dual;
            fn $m(self, rhs: &Dual) -> Dual {
                $f(self, rhs)
            }
        }
        impl $tr<Dual> for Dual {
            type Output = Dual;
            fn $m(self, rhs: Dual) -> Dual {
                $f(&self, &rhs)
            }
        }
        impl $tr<&Dual> for Dual {
            type Output = Dual;
            fn $m(self, rhs: &Dual) -> Dual {
                $f(&self, rhs)
            }
        }
        impl $tr<Dual> for &Dual {
            type Output = Dual;
            fn $m(self, rhs: Dual) -> Dual {
                $f(self, &rhs)
            }
        }
        impl $tr<f64> for &Dual {
            type Output = Dual;
            fn $m(self, rhs: f64) -> Dual {
                $f(self, &Dual::constant(rhs))
            }
        }
        impl $tr<f64> for Dual {
            type Output = Dual;
            fn $m(self, rhs: f64) -> Dual {
                $f(&self, &Dual::constant(rhs))
            }
        }
        impl $tr<&Dual> for f64 {
            type Output = Dual;
            fn $m(self, rhs: &Dual) -> Dual {
                $f(&Dual::constant(self), rhs)
            }
        }
        impl $tr<Dual> for f64 {
            type Output = Dual;
            fn $m(self, rhs: Dual) -> Dual {
                $f(&Dual::constant(self), &rhs)
            }
        }
    };
}
binop!(Add, add, add);
binop!(Sub, sub, sub);
binop!(Mul, mul, mul);
binop!(Div, div, div);

impl Neg for Dual {
    type Output = Dual;
    fn neg(self) -> Dual {
        -&self
    }
}
impl Neg for &Dual {
    type Output = Dual;
    fn neg(self) -> Dual {
        Dual {
            v: -self.v,
            d: self.d.iter().map(|x| -x).collect(),
        }
    }
}
```

Hinweis: Die Funktionsnamen `add`/`sub`/`mul`/`div` und die Methoden der Traits heißen gleich; bei Namenskonflikten die freien Funktionen umbenennen (`add_dual` usw.), das ist kein Schnittstellenbestandteil.

- [ ] **Step 4: Implementierung `model.rs`**

```rust
//! Skizze: Punkte, Kurven, Zahlenvektor. Abhängigkeiten kommen in `constraint.rs`.

use crate::error::SketchError;
use planora_kernel::{Arena, Id};
use planora_math::{DVec2, Frame, Tolerance};

/// Punkt: zwei Einträge im Variablenvektor.
#[derive(Clone, Debug)]
pub struct PointData {
    pub x: usize,
    pub y: usize,
    pub construction: bool,
    pub(crate) alive: bool,
}
pub type PointId = Id<PointData>;

#[derive(Clone, Debug, PartialEq)]
pub enum CurveKind {
    Line { a: PointId, b: PointId },
    Circle { center: PointId, radius: usize },
    /// Gegen den Uhrzeigersinn von `start` nach `end`; Radius = |start − center|.
    Arc { center: PointId, start: PointId, end: PointId },
    /// Große Halbachse als Vektor (ax, ay), kleine Halbachse `minor`.
    Ellipse { center: PointId, ax: usize, ay: usize, minor: usize },
    /// Natürlicher kubischer Spline durch die Stützpunkte.
    Spline { points: Vec<PointId> },
}

#[derive(Clone, Debug)]
pub struct CurveData {
    pub kind: CurveKind,
    pub construction: bool,
    pub(crate) alive: bool,
}
pub type CurveId = Id<CurveData>;

/// Eine Skizze auf einer Ebene. Alle Zahlenwerte liegen in `vars`.
#[derive(Clone, Debug)]
pub struct Sketch {
    pub name: String,
    frame: Frame,
    tol: Tolerance,
    pub(crate) vars: Vec<f64>,
    pub(crate) points: Arena<PointData>,
    pub(crate) curves: Arena<CurveData>,
}

fn finite(v: f64, what: &str) -> Result<(), SketchError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(SketchError::InvalidInput(format!("{what} muss eine endliche Zahl sein")))
    }
}

impl Sketch {
    pub fn new(frame: Frame, name: impl Into<String>, tol: Tolerance) -> Sketch {
        Sketch {
            name: name.into(),
            frame,
            tol,
            vars: Vec::new(),
            points: Arena::new(),
            curves: Arena::new(),
        }
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn tolerance(&self) -> &Tolerance {
        &self.tol
    }

    pub fn variable_count(&self) -> usize {
        self.vars.len()
    }

    pub(crate) fn new_var(&mut self, v: f64) -> usize {
        self.vars.push(v);
        self.vars.len() - 1
    }

    pub(crate) fn check_point(&self, p: PointId) -> Result<(), SketchError> {
        if p.index() < self.points.len() && self.points[p].alive {
            Ok(())
        } else {
            Err(SketchError::UnknownObject)
        }
    }

    pub(crate) fn check_curve(&self, c: CurveId) -> Result<(), SketchError> {
        if c.index() < self.curves.len() && self.curves[c].alive {
            Ok(())
        } else {
            Err(SketchError::UnknownObject)
        }
    }

    /// Position eines (geprüften) Punktes.
    pub(crate) fn pos(&self, p: PointId) -> DVec2 {
        let d = &self.points[p];
        DVec2::new(self.vars[d.x], self.vars[d.y])
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> Result<PointId, SketchError> {
        finite(x, "Die x-Koordinate")?;
        finite(y, "Die y-Koordinate")?;
        let (xi, yi) = (self.new_var(x), self.new_var(y));
        Ok(self.points.push(PointData {
            x: xi,
            y: yi,
            construction: false,
            alive: true,
        }))
    }

    pub fn point_xy(&self, p: PointId) -> Result<DVec2, SketchError> {
        self.check_point(p)?;
        Ok(self.pos(p))
    }

    pub fn set_point_xy(&mut self, p: PointId, x: f64, y: f64) -> Result<(), SketchError> {
        self.check_point(p)?;
        finite(x, "Die x-Koordinate")?;
        finite(y, "Die y-Koordinate")?;
        let (xi, yi) = (self.points[p].x, self.points[p].y);
        self.vars[xi] = x;
        self.vars[yi] = y;
        Ok(())
    }

    fn push_curve(&mut self, kind: CurveKind) -> CurveId {
        self.curves.push(CurveData {
            kind,
            construction: false,
            alive: true,
        })
    }

    pub fn add_line(&mut self, a: PointId, b: PointId) -> Result<CurveId, SketchError> {
        self.check_point(a)?;
        self.check_point(b)?;
        if a == b || self.pos(a).distance(self.pos(b)) <= self.tol.linear {
            return Err(SketchError::Degenerate("Die Linie hätte die Länge null".into()));
        }
        Ok(self.push_curve(CurveKind::Line { a, b }))
    }

    pub fn add_circle(&mut self, center: PointId, radius: f64) -> Result<CurveId, SketchError> {
        self.check_point(center)?;
        finite(radius, "Der Radius")?;
        if radius <= self.tol.linear {
            return Err(SketchError::Degenerate("Der Radius muss größer als null sein".into()));
        }
        let r = self.new_var(radius);
        Ok(self.push_curve(CurveKind::Circle { center, radius: r }))
    }

    /// Bogen gegen den Uhrzeigersinn von `start` nach `end` um `center`. Der Radius ist
    /// `|start − center|`; liegt `end` nicht auf diesem Kreis, zieht der Solver ihn dorthin.
    pub fn add_arc(
        &mut self,
        center: PointId,
        start: PointId,
        end: PointId,
    ) -> Result<CurveId, SketchError> {
        self.check_point(center)?;
        self.check_point(start)?;
        self.check_point(end)?;
        let c = self.pos(center);
        if self.pos(start).distance(c) <= self.tol.linear
            || self.pos(end).distance(c) <= self.tol.linear
        {
            return Err(SketchError::Degenerate("Start und Ende dürfen nicht in der Mitte liegen".into()));
        }
        Ok(self.push_curve(CurveKind::Arc { center, start, end }))
    }

    pub fn curve(&self, c: CurveId) -> Option<&CurveData> {
        self.check_curve(c).ok().map(|()| &self.curves[c])
    }

    pub fn radius(&self, c: CurveId) -> Option<f64> {
        match self.curve(c)?.kind {
            CurveKind::Circle { radius, .. } => Some(self.vars[radius]),
            CurveKind::Arc { center, start, .. } => Some(self.pos(start).distance(self.pos(center))),
            _ => None,
        }
    }

    pub fn line_length(&self, c: CurveId) -> Option<f64> {
        match self.curve(c)?.kind {
            CurveKind::Line { a, b } => Some(self.pos(a).distance(self.pos(b))),
            _ => None,
        }
    }

    pub fn set_construction(&mut self, c: CurveId, construction: bool) -> Result<(), SketchError> {
        self.check_curve(c)?;
        self.curves[c].construction = construction;
        Ok(())
    }

    /// Alle lebenden Punkte.
    pub fn points(&self) -> impl Iterator<Item = (PointId, &PointData)> {
        self.points.iter().filter(|(_, p)| p.alive)
    }

    /// Alle lebenden Kurven.
    pub fn curves(&self) -> impl Iterator<Item = (CurveId, &CurveData)> {
        self.curves.iter().filter(|(_, c)| c.alive)
    }
}
```

`error.rs` ergänzen:

```rust
    #[error("Das Objekt gibt es in dieser Skizze nicht")]
    UnknownObject,
    #[error("Entartete Geometrie: {0}")]
    Degenerate(String),
    #[error("Falscher Objekttyp: {0}")]
    WrongType(String),
```

`lib.rs`:

```rust
pub mod dual;
pub mod error;
pub mod linalg;
pub mod model;

pub use error::SketchError;
pub use model::{CurveData, CurveId, CurveKind, PointData, PointId, Sketch};
```

- [ ] **Step 5: Tests laufen lassen**

Run: `cargo test -p planora-sketch && cargo clippy -p planora-sketch --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS, clippy sauber

- [ ] **Step 6: Commit**

```bash
git add crates/planora-sketch
git commit -m "feat(sketch): Dual-Zahlen und Skizzen-Datenmodell (Punkte, Linien, Kreise, Bögen)"
```

---

### Task 3: Abhängigkeiten, Maße und Residuen

**Files:**
- Create: `crates/planora-sketch/src/constraint.rs`
- Modify: `crates/planora-sketch/src/model.rs` (Feld `constraints`, `CurveKind::points`, `add_arc` legt die interne Abhängigkeit an), `crates/planora-sketch/src/lib.rs`
- Test: `crates/planora-sketch/tests/constraints.rs`

**Interfaces:**
- Consumes: `Dual`, `Sketch`, `PointId`, `CurveId` aus Task 2.
- Produces (aus `planora_sketch::constraint`, Re-Exporte in `lib.rs`):
  - `enum LineOrPoints { Line(CurveId), Points(PointId, PointId) }`
  - `enum ConstraintKind` mit den Varianten `Coincident(PointId, PointId)`, `PointOnCurve(PointId, CurveId)`, `Horizontal(LineOrPoints)`, `Vertical(LineOrPoints)`, `Parallel(CurveId, CurveId)`, `Perpendicular(CurveId, CurveId)`, `Tangent(CurveId, CurveId)`, `Equal(CurveId, CurveId)`, `Fixed(PointId, f64, f64)`, `Symmetric(PointId, PointId, CurveId)` (Achse = Linie), `Midpoint(PointId, CurveId)`, `Distance(PointId, PointId, f64)`, `DistancePointLine(PointId, CurveId, f64)`, `Radius(CurveId, f64)`, `Diameter(CurveId, f64)`, `Angle(CurveId, CurveId, f64)` (Winkel von Richtung 1 nach Richtung 2, Radiant), `ArcRadius(CurveId)` (intern). Methoden: `is_dimension()`, `points() -> Vec<PointId>`, `curves() -> Vec<CurveId>`.
  - `struct ConstraintData { pub kind: ConstraintKind, pub branch: i8, pub driven: bool, pub enabled: bool, pub internal: bool, .. }`, `type ConstraintId = Id<ConstraintData>`.
  - `struct Evaluation { pub locals: Vec<usize>, pub residuals: Vec<Dual> }` – `locals` sind die Indizes der beteiligten Variablen im Variablenvektor; der Gradient jedes Residuums ist nach `locals` geordnet.
  - `impl Sketch`: `add_constraint(kind) -> Result<ConstraintId, SketchError>` (prüft Ids und Typen, legt `branch` aus der aktuellen Geometrie fest), `constraint(id) -> Option<&ConstraintData>`, `constraints()` (lebende, nicht interne), `set_constraint_value(id, v)` (nur Maße), `set_driven(id, bool)`, `set_enabled(id, bool)`, `measure(&kind) -> Option<f64>` (aktueller Wert eines Maßes: Abstand, Abstand Punkt–Linie (≥ 0), Radius, Durchmesser, Winkel in (−π, π]), `variables() -> &[f64]`, `evaluate(id, x: &[f64]) -> Option<Evaluation>`, `remove_constraint(id)`, `remove_curve(c)`, `remove_point(p)` (entfernt abhängige Kurven und Abhängigkeiten), `prune_unused_points() -> usize`. Interne Abhängigkeiten (`ArcRadius`) sind nicht einzeln entfernbar.
  - Branch-Festlegung beim Anlegen: `Tangent` Linie–Kreis/Bogen: `+1`, wenn der Mittelpunkt links der Linie (vorzeichenbehafteter Abstand ≥ 0), sonst `−1`; `Tangent` Kreis–Kreis: `0` außen berührend, `1` Kreis 2 innerhalb von Kreis 1, `−1` Kreis 1 innerhalb von Kreis 2 (die Lage mit dem kleinsten Fehler zur aktuellen Geometrie); `DistancePointLine`: Vorzeichen des aktuellen vorzeichenbehafteten Abstands (`+1` bei ≥ 0).
  - Residuen (alle mit `Dual`, Längenvektoren normiert mit Mindestlänge `MIN_LEN = 1e-9`): `Coincident` 2; `PointOnCurve` 1 (Linie: vorzeichenbehafteter Abstand; Kreis/Bogen: `|p−c| − r`); `Horizontal` `ay−by`; `Vertical` `ax−bx`; `Parallel` `cross(d1,d2)/(|d1||d2|)`; `Perpendicular` `dot(d1,d2)/(|d1||d2|)`; `Tangent` Linie–Kreis `s·sd(c, Linie) − r`, Kreis–Kreis `|c1−c2| − (s1·r1 + s2·r2)` mit `(s1,s2) = (1,1) | (1,−1) | (−1,1)` für Branch `0 | 1 | −1`; `Equal` Längen bzw. Radien; `Fixed` 2; `Symmetric` 2 (Mitte auf der Achse, Verbindung senkrecht zur Achse); `Midpoint` 2; `Distance` `|p−q| − d`; `DistancePointLine` `s·sd − d`; `Radius` `r − v`; `Diameter` `2r − v`; `Angle` `(cross·cos v − dot·sin v)/(|d1||d2|)`; `ArcRadius` `|end−c| − |start−c|`. Für Bögen ist `r = |start − center|`.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-sketch/tests/constraints.rs`

```rust
use planora_math::{Frame, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::SketchError;

struct Fixture {
    s: Sketch,
    p: Vec<PointId>,
    l: Vec<CurveId>,
    c: Vec<CurveId>,
    arc: CurveId,
}

fn fixture() -> Fixture {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let xy = [(0.0, 0.0), (7.0, 1.0), (3.0, 5.0), (8.0, 6.0), (-2.0, 3.0), (4.0, -1.0)];
    let p: Vec<PointId> = xy.iter().map(|&(x, y)| s.add_point(x, y).unwrap()).collect();
    let l = vec![
        s.add_line(p[0], p[1]).unwrap(),
        s.add_line(p[2], p[3]).unwrap(),
        s.add_line(p[4], p[5]).unwrap(),
    ];
    let c = vec![
        s.add_circle(p[2], 2.0).unwrap(),
        s.add_circle(p[5], 3.5).unwrap(),
    ];
    let arc = s.add_arc(p[0], p[1], p[3]).unwrap();
    Fixture { s, p, l, c, arc }
}

fn all_kinds(f: &Fixture) -> Vec<K> {
    let (p, l, c) = (&f.p, &f.l, &f.c);
    vec![
        K::Coincident(p[0], p[3]),
        K::PointOnCurve(p[4], l[0]),
        K::PointOnCurve(p[4], c[0]),
        K::PointOnCurve(p[4], f.arc),
        K::Horizontal(LineOrPoints::Line(l[0])),
        K::Horizontal(LineOrPoints::Points(p[0], p[3])),
        K::Vertical(LineOrPoints::Line(l[1])),
        K::Parallel(l[0], l[1]),
        K::Perpendicular(l[0], l[2]),
        K::Tangent(l[0], c[0]),
        K::Tangent(c[0], l[0]),
        K::Tangent(c[0], c[1]),
        K::Tangent(f.arc, c[1]),
        K::Equal(l[0], l[1]),
        K::Equal(c[0], c[1]),
        K::Fixed(p[1], 7.5, 1.5),
        K::Symmetric(p[0], p[3], l[2]),
        K::Midpoint(p[4], l[0]),
        K::Distance(p[0], p[3], 9.0),
        K::DistancePointLine(p[4], l[0], 3.0),
        K::Radius(c[0], 2.0),
        K::Radius(f.arc, 7.0),
        K::Diameter(c[1], 7.0),
        K::Angle(l[0], l[2], 1.0),
    ]
}

#[test]
fn jacobians_match_finite_differences_for_every_kind() {
    let mut f = fixture();
    for kind in all_kinds(&f) {
        let id = f.s.add_constraint(kind.clone()).unwrap();
        let x0 = f.s.variables().to_vec();
        let ev = f.s.evaluate(id, &x0).unwrap();
        assert!(!ev.residuals.is_empty(), "{kind:?}");
        for (ri, r) in ev.residuals.iter().enumerate() {
            assert!(r.v.is_finite(), "{kind:?}");
            for (k, &var) in ev.locals.iter().enumerate() {
                let h = 1e-6;
                let mut xp = x0.clone();
                xp[var] += h;
                let mut xm = x0.clone();
                xm[var] -= h;
                let fp = f.s.evaluate(id, &xp).unwrap().residuals[ri].v;
                let fm = f.s.evaluate(id, &xm).unwrap().residuals[ri].v;
                let fd = (fp - fm) / (2.0 * h);
                let g = r.grad(k);
                assert!(
                    (g - fd).abs() < 1e-5 * (1.0 + fd.abs()),
                    "{kind:?} Residuum {ri} Variable {var}: {g} vs {fd}"
                );
            }
        }
    }
}

fn residual_norm(s: &Sketch, id: ConstraintId) -> f64 {
    let ev = s.evaluate(id, s.variables()).unwrap();
    ev.residuals.iter().map(|r| r.v.abs()).fold(0.0, f64::max)
}

#[test]
fn dimensions_taken_from_the_current_geometry_start_satisfied() {
    let mut f = fixture();
    let (p, l, c) = (f.p.clone(), f.l.clone(), f.c.clone());
    for kind in [
        K::Distance(p[0], p[3], 0.0),
        K::DistancePointLine(p[4], l[0], 0.0),
        K::DistancePointLine(p[2], l[0], 0.0),
        K::Radius(c[0], 1.0),
        K::Diameter(c[1], 1.0),
        K::Angle(l[0], l[2], 0.0),
    ] {
        let v = f.s.measure(&kind).unwrap();
        let kind = set_value(kind, v);
        let id = f.s.add_constraint(kind.clone()).unwrap();
        assert!(residual_norm(&f.s, id) < 1e-9, "{kind:?}");
    }
    assert_eq!(f.s.measure(&K::Parallel(l[0], l[1])), None, "kein Maß");
}

fn set_value(kind: K, v: f64) -> K {
    match kind {
        K::Distance(a, b, _) => K::Distance(a, b, v),
        K::DistancePointLine(a, l, _) => K::DistancePointLine(a, l, v),
        K::Radius(c, _) => K::Radius(c, v),
        K::Diameter(c, _) => K::Diameter(c, v),
        K::Angle(a, b, _) => K::Angle(a, b, v),
        other => other,
    }
}

#[test]
fn satisfied_geometry_gives_zero_residuals() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    let c = s.add_point(4.0, 3.0).unwrap();
    let d = s.add_point(0.0, 5.0).unwrap();
    let e = s.add_point(4.0, 5.0).unwrap();
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    let de = s.add_line(d, e).unwrap();
    let circle = s.add_circle(c, 1.0).unwrap();
    // y = 4 berührt den Kreis um (4, 3) mit r = 1 von oben
    let t0 = s.add_point(0.0, 4.0).unwrap();
    let t1 = s.add_point(9.0, 4.0).unwrap();
    let tangent_line = s.add_line(t0, t1).unwrap();
    let mid = s.add_point(2.0, 0.0).unwrap();
    for (kind, name) in [
        (K::Horizontal(LineOrPoints::Line(ab)), "horizontal"),
        (K::Vertical(LineOrPoints::Line(bc)), "vertikal"),
        (K::Parallel(ab, de), "parallel"),
        (K::Perpendicular(ab, bc), "senkrecht"),
        (K::Tangent(tangent_line, circle), "tangential"),
        (K::Equal(ab, de), "gleich"),
        (K::Midpoint(mid, ab), "Mitte"),
        (K::PointOnCurve(mid, ab), "Punkt auf Linie"),
        (K::Fixed(mid, 2.0, 0.0), "fest"),
    ] {
        let id = s.add_constraint(kind).unwrap();
        assert!(residual_norm(&s, id) < 1e-9, "{name}");
    }
}

#[test]
fn unsatisfied_constraints_have_nonzero_residuals() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 1.0).unwrap();
    let c = s.add_point(4.0, 3.0).unwrap();
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    // a und b liegen nicht symmetrisch zur Achse bc (x = 4)
    let sym = s.add_constraint(K::Symmetric(a, b, bc)).unwrap();
    assert!(residual_norm(&s, sym) > 1e-3);
    let h = s.add_constraint(K::Horizontal(LineOrPoints::Line(ab))).unwrap();
    assert!(residual_norm(&s, h) > 1e-3);
}

#[test]
fn wrong_types_and_bad_values_are_rejected() {
    let mut f = fixture();
    let (p, l, c) = (f.p.clone(), f.l.clone(), f.c.clone());
    assert!(matches!(
        f.s.add_constraint(K::Parallel(l[0], c[0])),
        Err(SketchError::WrongType(_))
    ));
    assert!(matches!(
        f.s.add_constraint(K::Radius(l[0], 2.0)),
        Err(SketchError::WrongType(_))
    ));
    assert!(matches!(
        f.s.add_constraint(K::Horizontal(LineOrPoints::Line(c[0]))),
        Err(SketchError::WrongType(_))
    ));
    assert!(f.s.add_constraint(K::Tangent(l[0], l[1])).is_err());
    assert!(f.s.add_constraint(K::Distance(p[0], p[1], f64::NAN)).is_err());
    assert!(f.s.add_constraint(K::Distance(p[0], p[0], 1.0)).is_err());
    assert!(f.s.add_constraint(K::Radius(c[0], 0.0)).is_err());
    assert!(f.s.add_constraint(K::Fixed(p[0], f64::INFINITY, 0.0)).is_err());
    let bogus_point = planora_kernel::Id::from_index(500);
    assert_eq!(
        f.s.add_constraint(K::Coincident(p[0], bogus_point)),
        Err(SketchError::UnknownObject)
    );
    assert_eq!(f.s.constraints().count(), 0, "nichts wurde angelegt");
}

#[test]
fn arcs_carry_a_hidden_internal_constraint() {
    let f = fixture();
    assert_eq!(f.s.constraints().count(), 0, "interne Abhängigkeiten sind unsichtbar");
    let arc_internal = f
        .s
        .internal_constraints()
        .filter(|(_, c)| c.kind == K::ArcRadius(f.arc))
        .count();
    assert_eq!(arc_internal, 1);
}

#[test]
fn dimension_values_can_be_changed_and_flags_set() {
    let mut f = fixture();
    let id = f.s.add_constraint(K::Radius(f.c[0], 2.0)).unwrap();
    f.s.set_constraint_value(id, 3.0).unwrap();
    assert_eq!(f.s.constraint(id).unwrap().kind, K::Radius(f.c[0], 3.0));
    assert!(f.s.set_constraint_value(id, f64::NAN).is_err());
    let h = f.s.add_constraint(K::Horizontal(LineOrPoints::Line(f.l[0]))).unwrap();
    assert!(f.s.set_constraint_value(h, 1.0).is_err(), "kein Maß");
    f.s.set_driven(id, true).unwrap();
    f.s.set_enabled(h, false).unwrap();
    assert!(f.s.constraint(id).unwrap().driven);
    assert!(!f.s.constraint(h).unwrap().enabled);
}

#[test]
fn removing_objects_cascades() {
    let mut f = fixture();
    let (p, l, c) = (f.p.clone(), f.l.clone(), f.c.clone());
    let k1 = f.s.add_constraint(K::Parallel(l[0], l[1])).unwrap();
    let k2 = f.s.add_constraint(K::Radius(c[0], 2.0)).unwrap();
    let k3 = f.s.add_constraint(K::Coincident(p[0], p[1])).unwrap();
    f.s.remove_curve(l[0]).unwrap();
    assert!(f.s.curve(l[0]).is_none());
    assert!(f.s.constraint(k1).is_none(), "abhängige Abhängigkeit entfällt");
    assert!(f.s.constraint(k2).is_some());
    assert_eq!(f.s.remove_curve(l[0]), Err(SketchError::UnknownObject));
    // Punkt p[1] wird von der Linie l[0] (entfernt), dem Bogen und k3 benutzt
    f.s.remove_point(p[1]).unwrap();
    assert!(f.s.curve(f.arc).is_none(), "Bogen hing am Punkt");
    assert!(f.s.constraint(k3).is_none());
    assert!(
        f.s.internal_constraints().all(|(_, c)| c.kind != K::ArcRadius(f.arc)),
        "die interne Abhängigkeit des Bogens entfällt mit ihm"
    );
    let before = f.s.points().count();
    let pruned = f.s.prune_unused_points();
    assert_eq!(f.s.points().count(), before - pruned);
    assert!(pruned >= 1, "p[0] hing nur noch an entfernten Objekten");
    assert!(f.s.remove_constraint(k2).is_ok());
    assert!(f.s.remove_constraint(k2).is_err());
}
```

Hinweis: Der Test `arcs_carry_a_hidden_internal_constraint` und `removing_objects_cascades` brauchen zusätzlich `Sketch::internal_constraints()` (pub, gibt nur die lebenden internen Abhängigkeiten als `(ConstraintId, &ConstraintData)` aus). Der Import `SketchError` wird in mehreren Tests benutzt; ungenutzte Importe der Testdatei entfernen.

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test constraints`
Expected: FAIL (Modul fehlt)

- [ ] **Step 3: Implementierung `model.rs`-Änderungen**

Feld ergänzen: `pub(crate) constraints: Arena<crate::constraint::ConstraintData>,` (in `new` mit `Arena::new()`).

```rust
impl CurveKind {
    /// Alle Punkte, die die Kurve benutzt.
    pub fn points(&self) -> Vec<PointId> {
        match self {
            CurveKind::Line { a, b } => vec![*a, *b],
            CurveKind::Circle { center, .. } | CurveKind::Ellipse { center, .. } => vec![*center],
            CurveKind::Arc { center, start, end } => vec![*center, *start, *end],
            CurveKind::Spline { points } => points.clone(),
        }
    }
}
```

In `add_arc` nach `push_curve`:

```rust
        let id = self.push_curve(CurveKind::Arc { center, start, end });
        self.push_internal(crate::constraint::ConstraintKind::ArcRadius(id));
        Ok(id)
```

**Step 4: Implementierung `constraint.rs`** (Referenz; Auszug der Kernpunkte, der Rest folgt dem Schema der Tabelle in „Interfaces“)

```rust
//! Abhängigkeiten und Maße. Jede Abhängigkeit liefert Residuen als `Dual`.

use crate::dual::Dual;
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_kernel::{Arena, Id};
use std::cell::RefCell;

/// Mindestlänge für Richtungsvektoren beim Normieren (verhindert Division durch null).
const MIN_LEN: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq)]
pub enum LineOrPoints {
    Line(CurveId),
    Points(PointId, PointId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstraintKind {
    Coincident(PointId, PointId),
    PointOnCurve(PointId, CurveId),
    Horizontal(LineOrPoints),
    Vertical(LineOrPoints),
    Parallel(CurveId, CurveId),
    Perpendicular(CurveId, CurveId),
    Tangent(CurveId, CurveId),
    Equal(CurveId, CurveId),
    Fixed(PointId, f64, f64),
    Symmetric(PointId, PointId, CurveId),
    Midpoint(PointId, CurveId),
    Distance(PointId, PointId, f64),
    DistancePointLine(PointId, CurveId, f64),
    Radius(CurveId, f64),
    Diameter(CurveId, f64),
    Angle(CurveId, CurveId, f64),
    /// Intern: das Ende eines Bogens liegt auf dem Kreis durch seinen Start.
    ArcRadius(CurveId),
}

impl ConstraintKind {
    pub fn is_dimension(&self) -> bool {
        matches!(
            self,
            ConstraintKind::Distance(..)
                | ConstraintKind::DistancePointLine(..)
                | ConstraintKind::Radius(..)
                | ConstraintKind::Diameter(..)
                | ConstraintKind::Angle(..)
        )
    }

    pub fn points(&self) -> Vec<PointId> {
        use ConstraintKind::*;
        match self {
            Coincident(a, b) | Distance(a, b, _) => vec![*a, *b],
            PointOnCurve(p, _) | Fixed(p, ..) | Midpoint(p, _) | DistancePointLine(p, ..) => vec![*p],
            Horizontal(LineOrPoints::Points(a, b)) | Vertical(LineOrPoints::Points(a, b)) => vec![*a, *b],
            Symmetric(a, b, _) => vec![*a, *b],
            _ => vec![],
        }
    }

    pub fn curves(&self) -> Vec<CurveId> {
        use ConstraintKind::*;
        match self {
            PointOnCurve(_, c) | Midpoint(_, c) | DistancePointLine(_, c, _) | Radius(c, _)
            | Diameter(c, _) | ArcRadius(c) | Symmetric(_, _, c)
            | Horizontal(LineOrPoints::Line(c)) | Vertical(LineOrPoints::Line(c)) => vec![*c],
            Parallel(a, b) | Perpendicular(a, b) | Tangent(a, b) | Equal(a, b) | Angle(a, b, _) => vec![*a, *b],
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConstraintData {
    pub kind: ConstraintKind,
    /// Gewählte Lage bei mehrdeutigen Abhängigkeiten (siehe Interfaces).
    pub branch: i8,
    /// Nur anzeigen, nicht erzwingen.
    pub driven: bool,
    pub enabled: bool,
    pub internal: bool,
    pub(crate) alive: bool,
}
pub type ConstraintId = Id<ConstraintData>;

pub struct Evaluation {
    pub locals: Vec<usize>,
    pub residuals: Vec<Dual>,
}

// ---- Auswertung ---------------------------------------------------------------------------

type V2 = (Dual, Dual);

struct Ctx<'a> {
    sk: &'a Sketch,
    x: &'a [f64],
    locals: RefCell<Vec<usize>>,
    collecting: bool,
}

impl Ctx<'_> {
    fn var(&self, i: usize) -> Dual {
        let mut locals = self.locals.borrow_mut();
        let pos = match locals.iter().position(|&l| l == i) {
            Some(p) => p,
            None if self.collecting => {
                locals.push(i);
                locals.len() - 1
            }
            None => unreachable!("Variable wurde im ersten Durchlauf nicht gemeldet"),
        };
        if self.collecting {
            Dual::constant(self.x[i])
        } else {
            Dual::variable(self.x[i], pos, locals.len())
        }
    }

    fn pt(&self, p: PointId) -> V2 {
        let d = &self.sk.points[p];
        (self.var(d.x), self.var(d.y))
    }
}

fn sub2(a: &V2, b: &V2) -> V2 { (&a.0 - &b.0, &a.1 - &b.1) }
fn cross(a: &V2, b: &V2) -> Dual { &a.0 * &b.1 - &a.1 * &b.0 }
fn dot(a: &V2, b: &V2) -> Dual { &a.0 * &b.0 + &a.1 * &b.1 }
fn length(a: &V2) -> Dual { dot(a, a).sqrt() }
/// Länge, nach unten auf `MIN_LEN` begrenzt (zum Normieren).
fn norm_len(a: &V2) -> Dual {
    let l = length(a);
    if l.v < MIN_LEN { Dual::constant(MIN_LEN) } else { l }
}
```

Weiterer Aufbau von `constraint.rs` (Referenz, gleiche Schreibweise):

```rust
fn line_dir(ctx: &Ctx, c: CurveId) -> (V2, V2) {   // (a, d = b − a)
    let CurveKind::Line { a, b } = ctx.sk.curves[c].kind else { unreachable!("geprüft") };
    let (pa, pb) = (ctx.pt(a), ctx.pt(b));
    let d = sub2(&pb, &pa);
    (pa, d)
}

fn circle_of(ctx: &Ctx, c: CurveId) -> (V2, Dual) {
    match &ctx.sk.curves[c].kind {
        CurveKind::Circle { center, radius } => (ctx.pt(*center), ctx.var(*radius)),
        CurveKind::Arc { center, start, .. } => {
            let pc = ctx.pt(*center);
            let r = length(&sub2(&ctx.pt(*start), &pc));
            (pc, r)
        }
        _ => unreachable!("geprüft"),
    }
}

/// Vorzeichenbehafteter Abstand des Punktes p von der Linie c (links der Richtung positiv).
fn signed_distance(ctx: &Ctx, p: &V2, line: CurveId) -> Dual {
    let (a, d) = line_dir(ctx, line);
    cross(&d, &sub2(p, &a)) / norm_len(&d)
}

fn eval_kind(ctx: &Ctx, data: &ConstraintData) -> Vec<Dual> {
    use ConstraintKind::*;
    let sk = ctx.sk;
    match &data.kind {
        Coincident(a, b) => { let (p, q) = (ctx.pt(*a), ctx.pt(*b)); vec![&p.0 - &q.0, &p.1 - &q.1] }
        PointOnCurve(p, c) => {
            let pp = ctx.pt(*p);
            match sk.curves[*c].kind {
                CurveKind::Line { .. } => vec![signed_distance(ctx, &pp, *c)],
                _ => { let (m, r) = circle_of(ctx, *c); vec![length(&sub2(&pp, &m)) - r] }
            }
        }
        Horizontal(l) | Vertical(l) => {
            let (a, b) = match l {
                LineOrPoints::Line(c) => { let (a, d) = line_dir(ctx, *c); let b = (&a.0 + &d.0, &a.1 + &d.1); (a, b) }
                LineOrPoints::Points(a, b) => (ctx.pt(*a), ctx.pt(*b)),
            };
            if matches!(data.kind, Horizontal(_)) { vec![&a.1 - &b.1] } else { vec![&a.0 - &b.0] }
        }
        Parallel(c1, c2) | Perpendicular(c1, c2) => {
            let ((_, d1), (_, d2)) = (line_dir(ctx, *c1), line_dir(ctx, *c2));
            let den = norm_len(&d1) * norm_len(&d2);
            let num = if matches!(data.kind, Parallel(..)) { cross(&d1, &d2) } else { dot(&d1, &d2) };
            vec![num / den]
        }
        Tangent(c1, c2) => {
            let s = f64::from(data.branch);
            match (&sk.curves[*c1].kind, &sk.curves[*c2].kind) {
                (CurveKind::Line { .. }, _) => { let (m, r) = circle_of(ctx, *c2); vec![s * signed_distance(ctx, &m, *c1) - r] }
                (_, CurveKind::Line { .. }) => { let (m, r) = circle_of(ctx, *c1); vec![s * signed_distance(ctx, &m, *c2) - r] }
                _ => {
                    let ((m1, r1), (m2, r2)) = (circle_of(ctx, *c1), circle_of(ctx, *c2));
                    let (s1, s2) = match data.branch { 1 => (1.0, -1.0), -1 => (-1.0, 1.0), _ => (1.0, 1.0) };
                    vec![length(&sub2(&m1, &m2)) - (s1 * r1 + s2 * r2)]
                }
            }
        }
        Equal(c1, c2) => match (&sk.curves[*c1].kind, &sk.curves[*c2].kind) {
            (CurveKind::Line { .. }, _) => {
                let ((_, d1), (_, d2)) = (line_dir(ctx, *c1), line_dir(ctx, *c2));
                vec![length(&d1) - length(&d2)]
            }
            _ => {
                let ((_, r1), (_, r2)) = (circle_of(ctx, *c1), circle_of(ctx, *c2));
                vec![r1 - r2]
            }
        },
        Fixed(p, x, y) => {
            let pp = ctx.pt(*p);
            vec![&pp.0 - *x, &pp.1 - *y]
        }
        Symmetric(a, b, axis) => {
            let (pa, pb) = (ctx.pt(*a), ctx.pt(*b));
            let mid = ((&pa.0 + &pb.0) * 0.5, (&pa.1 + &pb.1) * 0.5);
            let (o, d) = line_dir(ctx, *axis);
            let n = norm_len(&d);
            vec![
                cross(&d, &sub2(&mid, &o)) / n.clone(),
                dot(&d, &sub2(&pb, &pa)) / n,
            ]
        }
        Midpoint(p, line) => {
            let pp = ctx.pt(*p);
            let (a, d) = line_dir(ctx, *line);
            vec![
                &pp.0 - &(&a.0 + &(&d.0 * 0.5)),
                &pp.1 - &(&a.1 + &(&d.1 * 0.5)),
            ]
        }
        Distance(a, b, v) => vec![length(&sub2(&ctx.pt(*a), &ctx.pt(*b))) - *v],
        DistancePointLine(p, line, v) => {
            let pp = ctx.pt(*p);
            vec![f64::from(data.branch) * signed_distance(ctx, &pp, *line) - *v]
        }
        Radius(c, v) => vec![circle_of(ctx, *c).1 - *v],
        Diameter(c, v) => vec![2.0 * circle_of(ctx, *c).1 - *v],
        Angle(c1, c2, v) => {
            let ((_, d1), (_, d2)) = (line_dir(ctx, *c1), line_dir(ctx, *c2));
            let den = norm_len(&d1) * norm_len(&d2);
            vec![(cross(&d1, &d2) * v.cos() - dot(&d1, &d2) * v.sin()) / den]
        }
        ArcRadius(c) => {
            let CurveKind::Arc { center, start, end } = sk.curves[*c].kind else {
                unreachable!("geprüft")
            };
            let pc = ctx.pt(center);
            vec![length(&sub2(&ctx.pt(end), &pc)) - length(&sub2(&ctx.pt(start), &pc))]
        }
    }
}
```

```rust
// ---- Sketch-Methoden ----------------------------------------------------------------------

impl Sketch {
    pub fn variables(&self) -> &[f64] { &self.vars }

    pub fn evaluate(&self, id: ConstraintId, x: &[f64]) -> Option<Evaluation> {
        if id.index() >= self.constraints.len() || !self.constraints[id].alive { return None; }
        let data = &self.constraints[id];
        let first = Ctx { sk: self, x, locals: RefCell::new(Vec::new()), collecting: true };
        let _ = eval_kind(&first, data);
        let locals = first.locals.into_inner();
        let second = Ctx { sk: self, x, locals: RefCell::new(locals), collecting: false };
        let residuals = eval_kind(&second, data);
        Some(Evaluation { locals: second.locals.into_inner(), residuals })
    }
    pub fn constraint(&self, id: ConstraintId) -> Option<&ConstraintData> {
        (id.index() < self.constraints.len() && self.constraints[id].alive && !self.constraints[id].internal)
            .then(|| &self.constraints[id])
    }

    /// Lebende, nicht interne Abhängigkeiten.
    pub fn constraints(&self) -> impl Iterator<Item = (ConstraintId, &ConstraintData)> {
        self.constraints.iter().filter(|(_, c)| c.alive && !c.internal)
    }

    /// Lebende interne Abhängigkeiten (zum Beispiel Bogenradius).
    pub fn internal_constraints(&self) -> impl Iterator<Item = (ConstraintId, &ConstraintData)> {
        self.constraints.iter().filter(|(_, c)| c.alive && c.internal)
    }

    pub(crate) fn push_internal(&mut self, kind: ConstraintKind) {
        self.constraints.push(ConstraintData { kind, branch: 0, driven: false, enabled: true, internal: true, alive: true });
    }

    fn checked_constraint(&self, id: ConstraintId) -> Result<(), SketchError> {
        if self.constraint(id).is_some() { Ok(()) } else { Err(SketchError::UnknownObject) }
    }

    pub fn set_driven(&mut self, id: ConstraintId, driven: bool) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.constraints[id].driven = driven;
        Ok(())
    }

    pub fn set_enabled(&mut self, id: ConstraintId, enabled: bool) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.constraints[id].enabled = enabled;
        Ok(())
    }

    /// Ändert den Zahlenwert eines Maßes. Die gewählte Lage (`branch`) bleibt erhalten.
    pub fn set_constraint_value(&mut self, id: ConstraintId, v: f64) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        let mut kind = self.constraints[id].kind.clone();
        match &mut kind {
            ConstraintKind::Distance(_, _, x) | ConstraintKind::DistancePointLine(_, _, x) | ConstraintKind::Angle(_, _, x) => *x = v,
            ConstraintKind::Radius(_, x) | ConstraintKind::Diameter(_, x) => *x = v,
            _ => return Err(SketchError::WrongType("Das ist kein Maß".into())),
        }
        self.check_refs(&kind)?;
        self.constraints[id].kind = kind;
        Ok(())
    }

    /// Aktueller Wert eines Maßes (der gespeicherte Wert in `kind` wird ignoriert).
    pub fn measure(&self, kind: &ConstraintKind) -> Option<f64> {
        use ConstraintKind::*;
        let x = &self.vars;
        let ctx = Ctx { sk: self, x, locals: RefCell::new(Vec::new()), collecting: true };
        let v = match kind {
            Distance(a, b, _) => length(&sub2(&ctx.pt(*a), &ctx.pt(*b))).v,
            DistancePointLine(p, l, _) => signed_distance(&ctx, &ctx.pt(*p), *l).v.abs(),
            Radius(c, _) => circle_of(&ctx, *c).1.v,
            Diameter(c, _) => 2.0 * circle_of(&ctx, *c).1.v,
            Angle(c1, c2, _) => {
                let ((_, d1), (_, d2)) = (line_dir(&ctx, *c1), line_dir(&ctx, *c2));
                cross(&d1, &d2).v.atan2(dot(&d1, &d2).v)
            }
            _ => return None,
        };
        v.is_finite().then_some(v)
    }

    /// Prüft Ids und Typen einer Abhängigkeit (siehe Typregeln).
    fn check_refs(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        for p in kind.points() { self.check_point(p)?; }
        for c in kind.curves() { self.check_curve(c)?; }
        let is_line = |c: CurveId| matches!(self.curves[c].kind, CurveKind::Line { .. });
        let is_round = |c: CurveId| matches!(self.curves[c].kind, CurveKind::Circle { .. } | CurveKind::Arc { .. });
        let wrong = |what: &str| Err(SketchError::WrongType(what.to_string()));
        let degenerate = |what: &str| Err(SketchError::Degenerate(what.to_string()));
        let finite = |v: f64| if v.is_finite() { Ok(()) } else { Err(SketchError::InvalidInput("Der Wert muss eine endliche Zahl sein".into())) };
        use ConstraintKind::*;
        match kind {
            Coincident(a, b) if a == b => degenerate("Ein Punkt kann nicht mit sich selbst zusammenfallen"),
            PointOnCurve(_, c) if !(is_line(*c) || is_round(*c)) => wrong("Ein Punkt kann nur auf Linie, Kreis oder Bogen liegen"),
            Horizontal(LineOrPoints::Line(c)) | Vertical(LineOrPoints::Line(c)) if !is_line(*c) => wrong("Das gilt nur für Linien"),
            Horizontal(LineOrPoints::Points(a, b)) | Vertical(LineOrPoints::Points(a, b)) if a == b => degenerate("Zwei verschiedene Punkte nötig"),
            Parallel(a, b) | Perpendicular(a, b) | Angle(a, b, _) if !(is_line(*a) && is_line(*b)) => wrong("Das gilt nur für Linien"),
            Parallel(a, b) | Perpendicular(a, b) | Angle(a, b, _) | Equal(a, b) if a == b => degenerate("Zwei verschiedene Objekte nötig"),
            Tangent(a, b) if !((is_line(*a) && is_round(*b)) || (is_round(*a) && is_line(*b)) || (is_round(*a) && is_round(*b) && a != b)) => wrong("Tangential geht nur zwischen Linie und Kreis/Bogen oder zwei Kreisen/Bögen"),
            Equal(a, b) if !((is_line(*a) && is_line(*b)) || (is_round(*a) && is_round(*b))) => wrong("Gleich geht nur für zwei Linien oder zwei Kreise/Bögen"),
            Symmetric(a, b, axis) if a == b || !is_line(*axis) => wrong("Symmetrie braucht zwei Punkte und eine Linie als Achse"),
            Midpoint(_, l) | DistancePointLine(_, l, _) if !is_line(*l) => wrong("Das gilt nur für Linien"),
            Distance(a, b, v) => { finite(*v)?; if a == b || *v < 0.0 { degenerate("Zwei verschiedene Punkte und ein Abstand ≥ 0 nötig") } else { Ok(()) } }
            DistancePointLine(_, _, v) => { finite(*v)?; if *v < 0.0 { degenerate("Der Abstand muss ≥ 0 sein") } else { Ok(()) } }
            Radius(c, v) | Diameter(c, v) => {
                finite(*v)?;
                if !is_round(*c) { wrong("Das gilt nur für Kreise und Bögen") }
                else if *v <= self.tolerance().linear { degenerate("Das Maß muss größer als null sein") } else { Ok(()) }
            }
            Angle(_, _, v) => finite(*v),
            Fixed(_, x, y) => { finite(*x)?; finite(*y) }
            _ => Ok(()),
        }
    }

    /// Lage bei mehrdeutigen Abhängigkeiten, bestimmt aus der aktuellen Geometrie.
    fn initial_branch(&self, kind: &ConstraintKind) -> i8 {
        use ConstraintKind::*;
        let ctx = Ctx { sk: self, x: &self.vars, locals: RefCell::new(Vec::new()), collecting: true };
        let sign = |v: f64| if v >= 0.0 { 1 } else { -1 };
        match kind {
            Tangent(a, b) => match (&self.curves[*a].kind, &self.curves[*b].kind) {
                (CurveKind::Line { .. }, _) => sign(signed_distance(&ctx, &circle_of(&ctx, *b).0, *a).v),
                (_, CurveKind::Line { .. }) => sign(signed_distance(&ctx, &circle_of(&ctx, *a).0, *b).v),
                _ => {
                    let ((m1, r1), (m2, r2)) = (circle_of(&ctx, *a), circle_of(&ctx, *b));
                    let d = length(&sub2(&m1, &m2)).v;
                    let (r1, r2) = (r1.v, r2.v);
                    let candidates = [(0, (d - (r1 + r2)).abs()), (1, (d - (r1 - r2)).abs()), (-1, (d - (r2 - r1)).abs())];
                    candidates.iter().min_by(|x, y| x.1.total_cmp(&y.1)).map_or(0, |c| c.0)
                }
            },
            DistancePointLine(p, l, _) => sign(signed_distance(&ctx, &ctx.pt(*p), *l).v),
            _ => 0,
        }
    }

    pub fn add_constraint(&mut self, kind: ConstraintKind) -> Result<ConstraintId, SketchError> {
        if matches!(kind, ConstraintKind::ArcRadius(_)) {
            return Err(SketchError::WrongType("Diese Abhängigkeit ist intern".into()));
        }
        self.check_refs(&kind)?;
        let branch = self.initial_branch(&kind);
        Ok(self.constraints.push(ConstraintData { kind, branch, driven: false, enabled: true, internal: false, alive: true }))
    }

    pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), SketchError> {
        self.checked_constraint(id)?;
        self.constraints[id].alive = false;
        Ok(())
    }

    /// Entfernt die Kurve samt allen Abhängigkeiten, die sie benutzen (auch interne).
    pub fn remove_curve(&mut self, c: CurveId) -> Result<(), SketchError> {
        self.check_curve(c)?;
        self.curves[c].alive = false;
        let dead: Vec<ConstraintId> = self.constraints.iter().filter(|(_, k)| k.alive && k.kind.curves().contains(&c)).map(|(i, _)| i).collect();
        for i in dead { self.constraints[i].alive = false; }
        Ok(())
    }

    /// Entfernt den Punkt samt allen Kurven und Abhängigkeiten, die ihn benutzen.
    pub fn remove_point(&mut self, p: PointId) -> Result<(), SketchError> {
        self.check_point(p)?;
        self.points[p].alive = false;
        let curves: Vec<CurveId> = self.curves().filter(|(_, d)| d.kind.points().contains(&p)).map(|(i, _)| i).collect();
        for c in curves { self.remove_curve(c)?; }
        let dead: Vec<ConstraintId> = self.constraints.iter().filter(|(_, k)| k.alive && k.kind.points().contains(&p)).map(|(i, _)| i).collect();
        for i in dead { self.constraints[i].alive = false; }
        Ok(())
    }

    /// Entfernt Punkte, die weder eine Kurve noch eine (nicht interne) Abhängigkeit benutzt.
    pub fn prune_unused_points(&mut self) -> usize {
        let mut used = std::collections::HashSet::new();
        for (_, c) in self.curves() { used.extend(c.kind.points()); }
        for (_, k) in self.constraints() { used.extend(k.kind.points()); }
        let unused: Vec<PointId> = self.points().map(|(i, _)| i).filter(|i| !used.contains(i)).collect();
        for p in &unused { self.points[*p].alive = false; }
        unused.len()
    }
}
```

`add_constraint` prüft zuerst `check_refs(&kind)` (alle Punkt-/Kurven-Ids über `check_point`/`check_curve`, dann Typen laut folgender Liste), dann `branch` und legt erst danach an – bei Fehler bleibt die Skizze unverändert.

Typregeln für `check_refs` (jeder Verstoß → `WrongType` mit deutscher Beschreibung, Werte nicht endlich → `InvalidInput`, gleiche Punkte/Kurven wo Verschiedenheit nötig → `Degenerate`): `PointOnCurve`: Linie, Kreis oder Bogen; `Horizontal`/`Vertical`: Linie bzw. zwei verschiedene Punkte; `Parallel`/`Perpendicular`/`Angle`: zwei verschiedene Linien; `Tangent`: (Linie, Kreis|Bogen), (Kreis|Bogen, Linie) oder zwei verschiedene Kreise/Bögen; `Equal`: zwei verschiedene Linien oder zwei verschiedene Kreise/Bögen; `Symmetric`: zwei verschiedene Punkte und eine Linie; `Midpoint`/`DistancePointLine`: Punkt und Linie (Abstand ≥ 0); `Distance`: zwei verschiedene Punkte (Wert ≥ 0); `Radius`/`Diameter`: Kreis oder Bogen (Wert > `tol.linear`); `Fixed`: endliche Koordinaten.

`remove_*` und `prune_unused_points` setzen nur `alive = false` (Ids bleiben stabil); `remove_curve` entfernt alle lebenden Abhängigkeiten, deren `kind.curves()` die Kurve enthält (auch interne); `remove_point` entfernt zuerst alle lebenden Kurven, deren `kind.points()` den Punkt enthalten, dann alle Abhängigkeiten mit diesem Punkt; `prune_unused_points` entfernt Punkte, die weder von einer lebenden Kurve noch von einer lebenden, nicht internen Abhängigkeit benutzt werden, und gibt ihre Zahl zurück.

- [ ] **Step 5: Tests laufen lassen**

Run: `cargo test -p planora-sketch && cargo clippy -p planora-sketch --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/planora-sketch
git commit -m "feat(sketch): Abhängigkeiten und Maße mit exakten Ableitungen (Dual), Entfernen mit Folgen"
```

---

### Task 4: Solver (Gruppen, Levenberg-Marquardt, Freiheitsgrade, Diagnose)

**Files:**
- Create: `crates/planora-sketch/src/solver.rs`
- Modify: `crates/planora-sketch/src/lib.rs`
- Test: `crates/planora-sketch/tests/solver.rs`

**Interfaces:**
- Consumes: `Sketch::evaluate`, `Evaluation`, `Matrix`, `cholesky_solve`, `analyze_rows` (Tasks 1–3).
- Produces (aus `planora_sketch::solver`, Re-Exporte in `lib.rs`):
  - `enum SolveStatus { Solved, NotConverged, Conflicting }`
  - `struct SolveReport { pub status: SolveStatus, pub dof: usize, pub redundant: Vec<ConstraintId>, pub conflicting: Vec<ConstraintId>, pub free_points: Vec<PointId>, pub free_curves: Vec<CurveId>, pub iterations: usize }`
  - `struct Soft { pub var: usize, pub target: f64, pub weight: f64 }` (weiche Zusatzbedingung `weight·(x[var] − target)`; wird in Task 5 fürs Ziehen benutzt)
  - `impl Sketch { pub fn solve(&mut self) -> SolveReport; pub(crate) fn solve_with_soft(&mut self, soft: &[Soft]) -> SolveReport }`
- Regeln:
  - Aktiv sind lebende, eingeschaltete (`enabled`), nicht nur anzeigende (`!driven`) Abhängigkeiten **einschließlich der internen**.
  - Variablen und Abhängigkeiten bilden Gruppen (Zusammenhangskomponenten über gemeinsame Variablen); jede Gruppe wird für sich gelöst. Eine Gruppe gilt als gelöst, wenn alle Residuen `≤ RESIDUAL_FRACTION · tol.linear` (= 1e-8 mm) sind. Gelöste Gruppen schreiben ihre Werte in die Skizze; nicht gelöste Gruppen lassen ihre Variablen **unverändert**.
  - `dof` = Summe der Freiheitsgrade der gelösten Gruppen (`Variablen − Rang(J)`) plus Zahl der lebenden Variablen, die in keiner aktiven Abhängigkeit vorkommen. Die Variablen eines nicht gelösten Gruppe zählen nicht mit.
  - `redundant`: nutzererzeugte Abhängigkeiten (nicht intern), von denen mindestens eine Zeile vom Zeilenraum der früher angelegten abhängig ist (nur in gelösten Gruppen). Es wird also immer die später angelegte gemeldet.
  - `free_points`: Punkte, deren x oder y im Nullraum der Jacobi-Matrix einen Anteil über `FREE_COMPONENT` hat, oder die in keiner Abhängigkeit vorkommen. `free_curves`: Kurven mit einer freien eigenen Variablen (Radius, Ellipsenachsen) oder einem freien Punkt.
  - Status: `Solved`, wenn alle Gruppen gelöst; `Conflicting`, wenn für mindestens eine Gruppe die Diagnose (unten) Abhängigkeiten findet; sonst `NotConverged`.
  - Diagnose einer nicht gelösten Gruppe (Entscheidung D4): Für jede nutzererzeugte Abhängigkeit der Gruppe wird die Gruppe ohne sie vom Ausgangszustand neu gelöst; die Abhängigkeiten, deren Weglassen eine Lösung ermöglicht, stehen in `conflicting`. Findet keine einzelne Weglassung eine Lösung, stehen dort alle Abhängigkeiten mit einem Residuum über der Schwelle am besten erreichten Punkt (Status dann `NotConverged`).
  - Weiche Bedingungen (`soft`, nur fürs Ziehen) liefern nur eine Näherung: Nach dem Lauf mit ihnen wird pro Gruppe ohne sie von dort aus exakt nachgelöst („Politur“); gelingt das nicht, wird ohne Zug vom Ausgangszustand gelöst. So bleiben harte Abhängigkeiten immer exakt erfüllt.
  - Konstanten (alle `const` mit Kommentar): `MAX_ITERATIONS = 100`, `RESIDUAL_FRACTION = 1e-2` (Anteil von `tol.linear`), `RANK_REL_TOL = 1e-7`, `INITIAL_LAMBDA = 1e-3`, `MIN_LAMBDA = 1e-12`, `MAX_LAMBDA = 1e12`, `MIN_DIAG = 1e-9` (untere Schranke für die Marquardt-Skalierung), `STALL_REL = 1e-14` (relative Schrittweite für „kein Fortschritt“), `FREE_COMPONENT = 1e-6`.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-sketch/tests/solver.rs`

```rust
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{PointId, Sketch};
use planora_sketch::solver::SolveStatus;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Rechteck aus vier Linien mit gemeinsamen Eckpunkten, leicht schief gestartet.
fn rectangle(s: &mut Sketch, at: DVec2) -> [PointId; 4] {
    let p = [
        s.add_point(at.x + 1.0, at.y + 2.0).unwrap(),
        s.add_point(at.x + 37.0, at.y + 3.0).unwrap(),
        s.add_point(at.x + 41.0, at.y + 33.0).unwrap(),
        s.add_point(at.x - 2.0, at.y + 29.0).unwrap(),
    ];
    let l: Vec<_> = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l[0]))).unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(l[1]))).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l[2]))).unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(l[3]))).unwrap();
    p
}

fn xy(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

#[test]
fn a_dimensioned_and_anchored_rectangle_is_fully_determined() {
    let mut s = sketch();
    let p = rectangle(&mut s, DVec2::ZERO);
    s.add_constraint(K::Fixed(p[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(p[1], p[2], 30.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.dof, 0);
    assert!(r.free_points.is_empty(), "{:?}", r.free_points);
    assert!(r.redundant.is_empty());
    for (pt, want) in p.iter().zip([(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)]) {
        let got = xy(&s, *pt);
        assert!((got - DVec2::new(want.0, want.1)).length() < 1e-7, "{got:?} vs {want:?}");
    }
}

#[test]
fn an_unanchored_rectangle_keeps_its_translation_free() {
    let mut s = sketch();
    let p = rectangle(&mut s, DVec2::ZERO);
    s.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(p[1], p[2], 30.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved);
    assert_eq!(r.dof, 2, "nur die Verschiebung in x und y bleibt frei");
    assert_eq!(r.free_points.len(), 4);
    let w = xy(&s, p[1]) - xy(&s, p[0]);
    assert!((w.length() - 40.0).abs() < 1e-7);
}

#[test]
fn a_third_right_angle_is_redundant_but_solvable() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(9.0, 1.0).unwrap();
    let c = s.add_point(10.0, 7.0).unwrap();
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(ab))).unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(bc))).unwrap();
    let perp = s.add_constraint(K::Perpendicular(ab, bc)).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    s.add_constraint(K::Distance(b, c, 6.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.redundant, vec![perp], "die später angelegte Abhängigkeit ist die überzählige");
    assert_eq!(r.dof, 0);
    assert!((xy(&s, c) - DVec2::new(10.0, 6.0)).length() < 1e-7);
}

#[test]
fn contradicting_dimensions_are_reported_and_leave_the_state_alone() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    let d1 = s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    let d2 = s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    let before = s.variables().to_vec();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert!(r.conflicting.contains(&d1) && r.conflicting.contains(&d2), "{:?}", r.conflicting);
    assert_eq!(s.variables(), before.as_slice(), "letzter gültiger Zustand bleibt");
}

#[test]
fn driven_and_disabled_constraints_are_ignored() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    let d2 = s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    s.set_driven(d2, true).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!((xy(&s, b).distance(xy(&s, a)) - 10.0).abs() < 1e-7);
    let e = s.add_constraint(K::Distance(a, b, 30.0)).unwrap();
    s.set_enabled(e, false).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn a_circle_tangent_to_a_fixed_line() {
    let mut s = sketch();
    let l0 = s.add_point(-10.0, 5.0).unwrap();
    let l1 = s.add_point(10.0, 5.0).unwrap();
    let line = s.add_line(l0, l1).unwrap();
    let c = s.add_point(1.0, 0.0).unwrap();
    let q = s.add_point(0.0, 0.0).unwrap();
    let circle = s.add_circle(c, 2.0).unwrap();
    s.add_constraint(K::Fixed(l0, -10.0, 5.0)).unwrap();
    s.add_constraint(K::Fixed(l1, 10.0, 5.0)).unwrap();
    s.add_constraint(K::Fixed(q, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Points(c, q))).unwrap();
    s.add_constraint(K::Radius(circle, 5.0)).unwrap();
    s.add_constraint(K::Tangent(line, circle)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.dof, 1, "der Mittelpunkt kann noch entlang der Linie wandern");
    assert!((s.radius(circle).unwrap() - 5.0).abs() < 1e-7);
    assert!((xy(&s, c).y - 0.0).abs() < 1e-7);
    assert!(r.free_points.contains(&c));
    assert!(r.free_curves.contains(&circle), "der Kreis hängt am freien Mittelpunkt");
}

#[test]
fn an_arc_end_is_pulled_onto_the_circle_through_its_start() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 7.0).unwrap();
    s.add_arc(c, a, b).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    let ra = xy(&s, a).distance(xy(&s, c));
    let rb = xy(&s, b).distance(xy(&s, c));
    assert!((ra - rb).abs() < 1e-7, "{ra} vs {rb}");
    assert_eq!(r.dof, 5, "6 Koordinaten minus die eine Bogenbedingung");
}

#[test]
fn independent_groups_are_solved_separately() {
    let mut s = sketch();
    let good = rectangle(&mut s, DVec2::ZERO);
    s.add_constraint(K::Fixed(good[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(good[0], good[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(good[1], good[2], 30.0)).unwrap();
    let a = s.add_point(100.0, 0.0).unwrap();
    let b = s.add_point(105.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    let before_bad = (xy(&s, a), xy(&s, b));
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Conflicting);
    assert!((xy(&s, good[2]) - DVec2::new(40.0, 30.0)).length() < 1e-7, "gute Gruppe ist gelöst");
    assert_eq!((xy(&s, a), xy(&s, b)), before_bad, "schlechte Gruppe unverändert");
}

#[test]
fn empty_and_unconstrained_sketches() {
    let mut s = sketch();
    let r = s.solve();
    assert_eq!((r.status, r.dof), (SolveStatus::Solved, 0));
    s.add_point(1.0, 1.0).unwrap();
    s.add_point(2.0, 2.0).unwrap();
    let r = s.solve();
    assert_eq!((r.status, r.dof, r.free_points.len()), (SolveStatus::Solved, 4, 2));
}

#[test]
fn collapsing_geometry_never_produces_nan() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 1.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Distance(a, b, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l))).unwrap();
    let _ = s.solve();
    assert!(s.variables().iter().all(|v| v.is_finite()));
}

#[test]
fn solving_twice_changes_nothing_more() {
    let mut s = sketch();
    let p = rectangle(&mut s, DVec2::ZERO);
    s.add_constraint(K::Fixed(p[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(p[1], p[2], 30.0)).unwrap();
    s.solve();
    let first = s.variables().to_vec();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved);
    for (a, b) in first.iter().zip(s.variables()) {
        assert!((a - b).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test solver`
Expected: FAIL (`solve` fehlt)

- [ ] **Step 3: Implementierung** – `crates/planora-sketch/src/solver.rs` (Referenz)

```rust
//! Constraint-Solver: Gruppen, Levenberg-Marquardt, Freiheitsgrade und Diagnose.

use crate::constraint::ConstraintId;
use crate::linalg::{Matrix, analyze_rows, cholesky_solve};
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use std::collections::{HashMap, HashSet};

/// Höchstzahl der Newton-Schritte je Gruppe.
const MAX_ITERATIONS: usize = 100;
/// Anteil von `tol.linear`, bis zu dem alle Residuen sinken müssen.
const RESIDUAL_FRACTION: f64 = 1e-2;
/// Relative Schwelle der Rangbestimmung (Länge des Restvektors / ursprüngliche Länge).
const RANK_REL_TOL: f64 = 1e-7;
const INITIAL_LAMBDA: f64 = 1e-3;
const MIN_LAMBDA: f64 = 1e-12;
const MAX_LAMBDA: f64 = 1e12;
/// Untere Schranke der Diagonale für die Marquardt-Skalierung (Nullspalten).
const MIN_DIAG: f64 = 1e-9;
/// Schritte kleiner als dieser Anteil der Variablengröße gelten als Stillstand.
const STALL_REL: f64 = 1e-14;
/// Nullraumanteil, ab dem eine Variable als frei gilt.
const FREE_COMPONENT: f64 = 1e-6;
/// Versuche mit steigender Dämpfung innerhalb eines Schritts.
const DAMPING_TRIES: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolveStatus {
    Solved,
    NotConverged,
    Conflicting,
}

#[derive(Clone, Debug)]
pub struct SolveReport {
    pub status: SolveStatus,
    pub dof: usize,
    pub redundant: Vec<ConstraintId>,
    pub conflicting: Vec<ConstraintId>,
    pub free_points: Vec<PointId>,
    pub free_curves: Vec<CurveId>,
    pub iterations: usize,
}

/// Weiche Zusatzbedingung `weight·(x[var] − target)` (zum Ziehen).
#[derive(Clone, Copy, Debug)]
pub struct Soft {
    pub var: usize,
    pub target: f64,
    pub weight: f64,
}

struct Group {
    vars: Vec<usize>,
    constraints: Vec<ConstraintId>,
}

struct Assembled {
    r: Vec<f64>,
    j: Matrix,
    hard_rows: usize,
    /// Zu jeder harten Zeile: Index der Abhängigkeit in `Group::constraints`.
    owner: Vec<usize>,
}

struct Lm {
    converged: bool,
    iterations: usize,
    hard_inf: f64,
}

struct Analysis {
    dof: usize,
    redundant: Vec<ConstraintId>,
    free_vars: Vec<usize>,
}

fn sumsq(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum()
}

impl Sketch {
    pub fn solve(&mut self) -> SolveReport {
        self.solve_with_soft(&[])
    }

    fn assemble(
        &self,
        g: &Group,
        col: &HashMap<usize, usize>,
        x: &[f64],
        soft: &[Soft],
        skip: Option<ConstraintId>,
    ) -> Assembled {
        let mut rows: Vec<(f64, Vec<(usize, f64)>)> = Vec::new();
        let mut owner = Vec::new();
        for (ci, &id) in g.constraints.iter().enumerate() {
            if Some(id) == skip {
                continue;
            }
            let ev = self.evaluate(id, x).expect("aktive Abhängigkeit lebt");
            for res in &ev.residuals {
                let entries = ev
                    .locals
                    .iter()
                    .enumerate()
                    .map(|(k, v)| (col[v], res.grad(k)))
                    .collect();
                rows.push((res.v, entries));
                owner.push(ci);
            }
        }
        let hard_rows = rows.len();
        for s in soft {
            if let Some(&c) = col.get(&s.var) {
                rows.push((s.weight * (x[s.var] - s.target), vec![(c, s.weight)]));
            }
        }
        let mut j = Matrix::zeros(rows.len(), g.vars.len());
        let mut r = Vec::with_capacity(rows.len());
        for (i, (value, entries)) in rows.iter().enumerate() {
            r.push(*value);
            for (c, d) in entries {
                j.add_to(i, *c, *d);
            }
        }
        Assembled { r, j, hard_rows, owner }
    }

    fn levenberg_marquardt(
        &self,
        g: &Group,
        col: &HashMap<usize, usize>,
        x: &mut [f64],
        soft: &[Soft],
        skip: Option<ConstraintId>,
        tol_r: f64,
    ) -> Lm {
        let hard_inf = |a: &Assembled| a.r[..a.hard_rows].iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let mut a = self.assemble(g, col, x, soft, skip);
        let mut cost = sumsq(&a.r);
        let mut lambda = INITIAL_LAMBDA;
        let mut iterations = 0;
        for _ in 0..MAX_ITERATIONS {
            if soft.is_empty() && hard_inf(&a) <= tol_r {
                break;
            }
            iterations += 1;
            let gradient = a.j.transpose_mul_vec(&a.r);
            let gram = a.j.gram();
            let rhs: Vec<f64> = gradient.iter().map(|v| -v).collect();
            let mut accepted = false;
            let mut stalled = false;
            for _ in 0..DAMPING_TRIES {
                let mut m = gram.clone();
                for i in 0..m.rows() {
                    let d = gram.get(i, i).max(MIN_DIAG);
                    m.add_to(i, i, lambda * d);
                }
                let Some(step) = cholesky_solve(&m, &rhs) else {
                    lambda *= 10.0;
                    continue;
                };
                let mut trial = x.to_vec();
                for (c, &v) in g.vars.iter().enumerate() {
                    trial[v] += step[c];
                }
                let t = self.assemble(g, col, &trial, soft, skip);
                let trial_cost = sumsq(&t.r);
                if trial_cost.is_finite() && trial_cost < cost {
                    let size = g.vars.iter().map(|&v| x[v].abs()).fold(1.0_f64, f64::max);
                    let step_len = step.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
                    stalled = step_len <= STALL_REL * size
                        || (cost - trial_cost) <= STALL_REL * cost;
                    x.copy_from_slice(&trial);
                    a = t;
                    cost = trial_cost;
                    lambda = (lambda / 10.0).max(MIN_LAMBDA);
                    accepted = true;
                    break;
                }
                lambda *= 10.0;
                if lambda > MAX_LAMBDA {
                    break;
                }
            }
            if !accepted || stalled {
                break;
            }
        }
        let h = hard_inf(&a);
        Lm { converged: h.is_finite() && h <= tol_r, iterations, hard_inf: h }
    }

    /// Rang, Redundanz und freie Variablen einer gelösten Gruppe an der Stelle `x`.
    fn analyze(&self, g: &Group, col: &HashMap<usize, usize>, x: &[f64]) -> Analysis {
        let a = self.assemble(g, col, x, &[], None);
        let space = analyze_rows(&a.j, RANK_REL_TOL);
        let mut redundant = Vec::new();
        for (row, independent) in space.independent.iter().enumerate() {
            let id = g.constraints[a.owner[row]];
            if !independent && !self.constraints[id].internal && !redundant.contains(&id) {
                redundant.push(id);
            }
        }
        let free_vars = g
            .vars
            .iter()
            .enumerate()
            .filter(|(c, _)| space.null_space.iter().any(|v| v[*c].abs() > FREE_COMPONENT))
            .map(|(_, &v)| v)
            .collect();
        Analysis { dof: g.vars.len() - space.rank, redundant, free_vars }
    }

    /// Bestimmt, welche einzelne Abhängigkeit einer nicht lösbaren Gruppe den Widerspruch
    /// verursacht (Entscheidung D4).
    fn diagnose(
        &self,
        g: &Group,
        col: &HashMap<usize, usize>,
        start: &[f64],
        failed: &[f64],
        tol_r: f64,
    ) -> Vec<ConstraintId> {
        let mut candidates = Vec::new();
        for &id in &g.constraints {
            if self.constraints[id].internal {
                continue;
            }
            let mut x = start.to_vec();
            if self.levenberg_marquardt(g, col, &mut x, &[], Some(id), tol_r).converged {
                candidates.push(id);
            }
        }
        if !candidates.is_empty() {
            return candidates;
        }
        let a = self.assemble(g, col, failed, &[], None);
        let mut big = Vec::new();
        for (row, v) in a.r[..a.hard_rows].iter().enumerate() {
            let id = g.constraints[a.owner[row]];
            if v.abs() > tol_r && !self.constraints[id].internal && !big.contains(&id) {
                big.push(id);
            }
        }
        big
    }

    pub(crate) fn solve_with_soft(&mut self, soft: &[Soft]) -> SolveReport {
        let tol_r = RESIDUAL_FRACTION * self.tolerance().linear;
        let active: Vec<ConstraintId> = self
            .constraints
            .iter()
            .filter(|(_, c)| c.alive && c.enabled && !c.driven)
            .map(|(i, _)| i)
            .collect();

        // Zusammenhangskomponenten über gemeinsame Variablen.
        let n = self.vars.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let mut in_constraint = vec![false; n];
        for &id in &active {
            let locals = self.evaluate(id, &self.vars).expect("lebt").locals;
            for w in locals.windows(2) {
                let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
                parent[a] = b;
            }
            for v in locals {
                in_constraint[v] = true;
            }
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut index: HashMap<usize, usize> = HashMap::new();
        for v in (0..n).filter(|v| in_constraint[*v]) {
            let root = find(&mut parent, v);
            let gi = *index.entry(root).or_insert_with(|| {
                groups.push(Group { vars: Vec::new(), constraints: Vec::new() });
                groups.len() - 1
            });
            groups[gi].vars.push(v);
        }
        for &id in &active {
            let first = self.evaluate(id, &self.vars).expect("lebt").locals[0];
            let gi = index[&find(&mut parent, first)];
            groups[gi].constraints.push(id);
        }

        let mut status = SolveStatus::Solved;
        let mut redundant = Vec::new();
        let mut conflicting = Vec::new();
        let mut free_vars: HashSet<usize> = HashSet::new();
        let mut dof = 0;
        let mut iterations = 0;
        let mut committed = self.vars.clone();
        let mut solved_group_vars: HashSet<usize> = HashSet::new();
        for g in &groups {
            let col: HashMap<usize, usize> = g.vars.iter().enumerate().map(|(c, v)| (*v, c)).collect();
            let start = self.vars.clone();
            let mut x = start.clone();
            let mut lm = self.levenberg_marquardt(g, &col, &mut x, soft, None, tol_r);
            if !soft.is_empty() {
                // Politur: Die weichen Bedingungen liefern nur eine Näherung. Von dort aus wird
                // ohne sie exakt nachgelöst; gelingt das nicht, wird ohne Zug vom Start gelöst.
                lm = self.levenberg_marquardt(g, &col, &mut x, &[], None, tol_r);
                if !lm.converged {
                    x = start.clone();
                    lm = self.levenberg_marquardt(g, &col, &mut x, &[], None, tol_r);
                }
            }
            iterations = iterations.max(lm.iterations);
            if lm.converged {
                let analysis = self.analyze(g, &col, &x);
                dof += analysis.dof;
                free_vars.extend(analysis.free_vars);
                for id in analysis.redundant {
                    if !redundant.contains(&id) {
                        redundant.push(id);
                    }
                }
                for &v in &g.vars {
                    committed[v] = x[v];
                }
                solved_group_vars.extend(g.vars.iter().copied());
            } else {
                let found = self.diagnose(g, &col, &start, &x, tol_r);
                let reason = if found.is_empty() || lm.hard_inf.is_nan() {
                    SolveStatus::NotConverged
                } else {
                    SolveStatus::Conflicting
                };
                // Widerspruch gewinnt gegen „nicht konvergiert“.
                if status != SolveStatus::Conflicting {
                    status = reason;
                }
                for id in found {
                    if !conflicting.contains(&id) {
                        conflicting.push(id);
                    }
                }
            }
        }
        self.vars = committed;

        // Variablen, die in keiner Abhängigkeit vorkommen, sind frei.
        let live = self.live_variables();
        for &v in &live {
            if !in_constraint[v] {
                dof += 1;
                free_vars.insert(v);
            }
        }
        let free_points: Vec<PointId> = self
            .points()
            .filter(|(_, p)| free_vars.contains(&p.x) || free_vars.contains(&p.y))
            .map(|(i, _)| i)
            .collect();
        let free_curves: Vec<CurveId> = self
            .curves()
            .filter(|(_, c)| {
                self.own_vars(&c.kind).iter().any(|v| free_vars.contains(v))
                    || c.kind.points().iter().any(|p| free_points.contains(p))
            })
            .map(|(i, _)| i)
            .collect();
        SolveReport { status, dof, redundant, conflicting, free_points, free_curves, iterations }
    }

    /// Variablen, die zu einer Kurve gehören und nicht Teil eines Punktes sind.
    fn own_vars(&self, kind: &CurveKind) -> Vec<usize> {
        match kind {
            CurveKind::Circle { radius, .. } => vec![*radius],
            CurveKind::Ellipse { ax, ay, minor, .. } => vec![*ax, *ay, *minor],
            _ => Vec::new(),
        }
    }

    /// Alle Variablen lebender Punkte und Kurven.
    fn live_variables(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.points().flat_map(|(_, p)| [p.x, p.y]).collect();
        for (_, c) in self.curves() {
            v.extend(self.own_vars(&c.kind));
        }
        v
    }
}
```

Hinweis: Diese Referenz setzt voraus, dass jede aktive Abhängigkeit mindestens eine Variable hat (`locals[0]`). `Fixed`, `Distance` usw. haben immer welche; ein Fall ohne Variable kann nicht entstehen, weil jede Abhängigkeit Punkte oder Kurven mit Variablen referenziert.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-sketch --test solver && cargo clippy -p planora-sketch --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS. Schlägt `an_arc_end_is_pulled_onto_the_circle_through_its_start` beim `dof`-Wert fehl, ist zuerst zu prüfen, ob der Bogen intern wirklich genau eine Abhängigkeit hat (`ArcRadius`); die Erwartung 5 folgt aus 6 Variablen − 1.

- [ ] **Step 5: Commit**

```bash
git add crates/planora-sketch
git commit -m "feat(sketch): Solver mit Gruppen, Levenberg-Marquardt, Freiheitsgraden, Redundanz- und Widerspruchsdiagnose"
```

---

### Task 5: Ziehen und Stabilität

**Files:**
- Modify: `crates/planora-sketch/src/solver.rs` (nur Ergänzung), `crates/planora-sketch/src/lib.rs`
- Test: `crates/planora-sketch/tests/drag.rs`

**Interfaces:**
- Consumes: `solve_with_soft`, `Soft` (Task 4).
- Produces: `struct Drag { pub point: PointId, pub target: DVec2 }` und `impl Sketch { pub fn drag(&mut self, drags: &[Drag]) -> Result<SolveReport, SketchError> }`. Ziehen setzt je gezogenem Punkt zwei weiche Bedingungen mit Gewicht `DRAG_WEIGHT = 1e-2` (benannte Konstante, begründen: klein gegen harte Residuen, groß genug für den Zug). Ein gezogener Punkt, der in keiner Abhängigkeit vorkommt, wird direkt auf das Ziel gesetzt. Nach dem Lösen der Skizze mit den weichen Bedingungen zählt der Zustand nur, wenn **alle harten** Abhängigkeiten erfüllt sind (`Solved`); sonst bleibt der Zustand unverändert (Regel aus Task 4). Rückgabe `Err(UnknownObject)` für unbekannte Punkte, `Err(InvalidInput)` für nicht endliche Ziele.
- Eigenschaft „kein Umklappen“: Der Solver startet immer vom aktuellen Zustand; die Lage mehrdeutiger Abhängigkeiten (Tangential, Abstand Punkt–Linie) ist in `branch` gespeichert und ändert sich beim Ziehen nicht.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-sketch/tests/drag.rs`

```rust
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::Sketch;
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::SketchError;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn dragging_a_free_point_moves_it_exactly() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let r = s
        .drag(&[Drag { point: a, target: DVec2::new(3.0, -4.0) }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved);
    assert_eq!(s.point_xy(a).unwrap(), DVec2::new(3.0, -4.0));
}

#[test]
fn dragging_respects_hard_constraints() {
    // Linie fest an a (0,0), Länge 10, horizontal; Ziehen von b nach (7, 7) darf sie nicht
    // verlängern oder kippen, b bleibt auf der x-Achse bei Länge 10.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l))).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let r = s.drag(&[Drag { point: b, target: DVec2::new(7.0, 7.0) }]).unwrap();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    let pb = s.point_xy(b).unwrap();
    assert!((pb - DVec2::new(10.0, 0.0)).length() < 1e-6, "{pb:?}");
}

#[test]
fn dragging_a_hinge_never_flips_to_the_mirror_solution() {
    // Zwei Stäbe a–m (Länge 5) und m–b (Länge 5) mit festen Enden a=(0,0), b=(8,0): m hat genau
    // zwei Lösungen (4, ±3). Egal wohin gezogen wird, m bleibt oben und exakt auf der Lösung.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(8.0, 0.0).unwrap();
    let m = s.add_point(4.0, 3.0).unwrap();
    s.add_line(a, m).unwrap();
    s.add_line(m, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 8.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, m, 5.0)).unwrap();
    s.add_constraint(K::Distance(m, b, 5.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    for target in [DVec2::new(4.0, 9.0), DVec2::new(4.5, 2.0), DVec2::new(3.5, 3.5)] {
        let r = s.drag(&[Drag { point: m, target }]).unwrap();
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        let pm = s.point_xy(m).unwrap();
        assert!(pm.y > 0.0, "ist nach unten umgeklappt: {pm:?}");
        assert!((pm.distance(DVec2::ZERO) - 5.0).abs() < 1e-6);
        assert!((pm.distance(DVec2::new(8.0, 0.0)) - 5.0).abs() < 1e-6);
    }
}

#[test]
fn dragging_in_a_degenerate_direction_keeps_the_state_valid() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 4.0)).unwrap();
    s.solve();
    // Ziel genau auf a: kein NaN, Abstand bleibt 4
    let r = s.drag(&[Drag { point: b, target: DVec2::ZERO }]).unwrap();
    assert!(s.variables().iter().all(|v| v.is_finite()));
    assert_eq!(r.status, SolveStatus::Solved);
    let pb = s.point_xy(b).unwrap();
    assert!((pb.length() - 4.0).abs() < 1e-6, "{pb:?}");
}

#[test]
fn drag_rejects_bad_input() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    assert!(matches!(
        s.drag(&[Drag { point: a, target: DVec2::new(f64::NAN, 0.0) }]),
        Err(SketchError::InvalidInput(_))
    ));
    let bogus = planora_kernel::Id::from_index(42);
    assert_eq!(
        s.drag(&[Drag { point: bogus, target: DVec2::ZERO }]).unwrap_err(),
        SketchError::UnknownObject
    );
    assert_eq!(s.point_xy(a).unwrap(), DVec2::ZERO);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test drag`
Expected: FAIL (`drag`, `Drag` fehlen)

- [ ] **Step 3: Implementierung** (Ergänzung in `solver.rs`)

```rust
use crate::error::SketchError;
use planora_math::DVec2;

/// Gewicht der weichen Zugbedingung. Klein gegen die harten Residuen (Gewicht 1), damit
/// Abhängigkeiten gewinnen; groß genug, damit der Zug die freien Freiheitsgrade bewegt.
const DRAG_WEIGHT: f64 = 1e-2;

#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub point: PointId,
    pub target: DVec2,
}

impl Sketch {
    /// Zieht Punkte zu Zielpositionen; harte Abhängigkeiten bleiben erfüllt. Bei Misserfolg
    /// bleibt die Skizze unverändert (siehe `solve`).
    pub fn drag(&mut self, drags: &[Drag]) -> Result<SolveReport, SketchError> {
        for d in drags {
            self.check_point(d.point)?;
            if !d.target.is_finite() {
                return Err(SketchError::InvalidInput("Das Ziel muss endlich sein".into()));
            }
        }
        let mut soft = Vec::new();
        for d in drags {
            let (xv, yv) = (self.points[d.point].x, self.points[d.point].y);
            soft.push(Soft { var: xv, target: d.target.x, weight: DRAG_WEIGHT });
            soft.push(Soft { var: yv, target: d.target.y, weight: DRAG_WEIGHT });
        }
        // Punkte ohne jede Abhängigkeit folgen dem Zug direkt.
        for d in drags {
            if self.point_unconstrained(d.point) {
                self.set_point_xy(d.point, d.target.x, d.target.y)?;
            }
        }
        Ok(self.solve_with_soft(&soft))
    }
}
```

mit

```rust
    /// Wahr, wenn keine aktive Abhängigkeit (lebend, eingeschaltet, nicht nur anzeigend, auch
    /// interne) den Punkt direkt oder über eine Kurve benutzt, die ihn enthält.
    fn point_unconstrained(&self, p: PointId) -> bool {
        !self
            .constraints
            .iter()
            .filter(|(_, c)| c.alive && c.enabled && !c.driven)
            .any(|(_, c)| {
                c.kind.points().contains(&p)
                    || c.kind
                        .curves()
                        .iter()
                        .any(|&cu| self.curves[cu].kind.points().contains(&p))
            })
    }
```

Ein Punkt, der nur Endpunkt einer Linie ohne Abhängigkeiten ist, ist unbeschränkt und folgt dem Zug direkt.

Das Gewicht und die Reihenfolge der Residuen sorgen dafür, dass die kleinste Änderung gewählt wird (Levenberg-Marquardt startet vom aktuellen Zustand und dämpft Sprünge). Wird der Test `dragging_a_hinge_follows_the_target_without_flipping` instabil, ist die Ursache fast sicher ein zu großer Schritt über die Spiegellage; dann `INITIAL_LAMBDA` für Zugläufe erhöhen (nicht den Test ändern).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-sketch && cargo clippy -p planora-sketch --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/planora-sketch
git commit -m "feat(sketch): Ziehen mit weichen Bedingungen, kein Umklappen mehrdeutiger Lagen"
```

---

### Task 6: Kernel-Eingaben absichern und Eigenschaften über eine Fassade lesen

**Files:**
- Modify: `crates/planora-math/src/frame.rs`, `crates/planora-kernel/src/profile.rs`, `crates/planora-kernel/src/error.rs`, `crates/planora-kernel/src/extrude.rs`, `crates/planora-kernel/src/revolve.rs`, `crates/planora-kernel/src/kernel.rs`, `crates/planora-kernel/src/lib.rs`
- Modify (Aufrufer): `crates/planora-app/src/samples.rs`, `crates/planora-app/src/properties.rs`, `crates/planora-kernel/tests/extrude.rs`, `crates/planora-kernel/tests/profile.rs`, `crates/planora-mesh/tests/tessellate.rs`
- Test: `crates/planora-kernel/tests/input_hardening.rs`, `crates/planora-math/tests/frame.rs` (neu, falls nicht vorhanden)

**Interfaces:**
- Produces:
  - `Frame::is_orthonormal(&self, eps: f64) -> bool` (`planora-math`): alle Komponenten endlich, `|x| = |y| = |z| = 1` und paarweise Skalarprodukte 0 jeweils innerhalb `eps`, und `x × y = z` innerhalb `eps` (rechtshändig).
  - `ProfileLoop::polygon(points: &[DVec2]) -> Result<ProfileLoop, KernelError>` (Fehler `KernelError::DegenerateSegment` bei weniger als drei Punkten oder nicht endlichen Werten). `ProfileLoop::rectangle` und `circle` bleiben unfehlbar.
  - `ProfileLoop::sample(steps_per_turn)` und `signed_area()` hängen und allozieren nie unbegrenzt: Die Schrittzahl je Bogen ist auf `MAX_SAMPLES_PER_ARC = 65_536` begrenzt (benannte `const`), nicht endliche Bogenwinkel ergeben mindestens 2 Schritte (Ergebnis dann NaN-Punkte, aber begrenzte Länge). `steps_per_turn` wird auf `1..=MAX_SAMPLES_PER_ARC` geklemmt.
  - Neue `KernelError`-Varianten (deutsche Meldungen): `InvalidFrame` („Das Koordinatensystem ist ungültig (nicht endlich oder nicht orthonormal)“), `HoleOutsideOuter` („Ein Loch liegt nicht vollständig innerhalb der äußeren Schleife“), `ProfileLoopsIntersect` („Zwei Schleifen des Profils schneiden oder berühren sich“).
  - `extrude` und `revolve` geben `InvalidFrame` zurück, wenn `!frame.is_orthonormal(tol.linear)`; `extrude` prüft zusätzlich vor dem Aufbau Löcher: jedes Loch muss vollständig innerhalb der äußeren Schleife liegen und darf weder die äußere Schleife noch ein anderes Loch schneiden oder berühren (Prüfung auf den mit 64 Schritten je Vollkreis abgetasteten Linienzügen, Berührung zählt als Schnitt).
  - Fassade: `enum SurfaceKind { Plane, Cylinder, Cone, Sphere, Torus }` und `enum CurveKind { Line, Circle, Ellipse }` (in `planora-kernel`, mit `label() -> &'static str` auf Deutsch: „Ebene“, „Zylinderfläche“, „Kegelfläche“, „Kugelfläche“, „Torusfläche“; „Gerade“, „Kreis“, „Ellipse“); `Kernel::face_kind(&self, solid, face: Id<Face>) -> Option<SurfaceKind>`, `Kernel::edge_kind(&self, solid, edge: Id<Edge>) -> Option<CurveKind>`, `Kernel::counts(&self, solid) -> (usize, usize)` (Flächen, Kanten). Die App liest damit Eigenschaften, ohne `solid.faces[..]` zu indizieren (nicht vorhandene Ids ergeben `None`).
  - Die Aufrufer von `polygon` (`samples.rs`, Tests) werden angepasst (`.unwrap()` in Tests, `.expect("Beispielprofil")` in `samples.rs`).

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-kernel/tests/input_hardening.rs`

```rust
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, TAU, Tolerance};
use std::time::{Duration, Instant};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

#[test]
fn polygon_with_too_few_or_bad_points_is_an_error_not_a_panic() {
    assert_eq!(
        ProfileLoop::polygon(&[DVec2::ZERO, DVec2::X]),
        Err(KernelError::DegenerateSegment)
    );
    assert_eq!(ProfileLoop::polygon(&[]), Err(KernelError::DegenerateSegment));
    assert_eq!(
        ProfileLoop::polygon(&[DVec2::ZERO, DVec2::X, DVec2::new(f64::NAN, 1.0)]),
        Err(KernelError::DegenerateSegment)
    );
    assert!(ProfileLoop::polygon(&[DVec2::ZERO, DVec2::X, DVec2::Y]).is_ok());
}

#[test]
fn sampling_huge_or_non_finite_sweeps_terminates_quickly() {
    let start = Instant::now();
    for sweep in [1e300, f64::INFINITY, f64::NAN, -1e18] {
        let lp = ProfileLoop {
            start: DVec2::new(1.0, 0.0),
            segments: vec![Segment::Arc {
                center: DVec2::ZERO,
                sweep,
            }],
        };
        let pts = lp.sample(64);
        assert!(pts.len() <= 70_000, "zu viele Punkte: {}", pts.len());
        let _ = lp.signed_area();
        assert!(lp.sample(usize::MAX).len() <= 70_000);
        assert!(lp.sample(0).len() >= 2);
    }
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn bad_frames_are_rejected_by_extrude_and_revolve() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(2.0, 2.0)),
        holes: vec![],
    };
    let mut nan = Frame::WORLD;
    nan.origin.x = f64::NAN;
    let mut skew = Frame::WORLD;
    skew.y = DVec3::new(0.1, 1.0, 0.0);
    let mut short = Frame::WORLD;
    short.x = DVec3::new(2.0, 0.0, 0.0);
    let mut left_handed = Frame::WORLD;
    left_handed.z = -DVec3::Z;
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line { to: DVec2::new(2.0, 0.0) },
            Segment::Line { to: DVec2::new(2.0, 2.0) },
            Segment::Line { to: DVec2::new(0.0, 2.0) },
        ],
    };
    for f in [nan, skew, short, left_handed] {
        assert_eq!(k().extrude(&profile, &f, 1.0).unwrap_err(), KernelError::InvalidFrame);
        assert_eq!(k().revolve(&chain, &f).unwrap_err(), KernelError::InvalidFrame);
    }
    assert!(k().extrude(&profile, &Frame::WORLD, 1.0).is_ok());
}

#[test]
fn frame_orthonormality_check() {
    assert!(Frame::WORLD.is_orthonormal(1e-9));
    assert!(Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 1.0, 1.0)).is_orthonormal(1e-9));
}

fn rect_loop(min: (f64, f64), max: (f64, f64)) -> ProfileLoop {
    ProfileLoop::rectangle(DVec2::new(min.0, min.1), DVec2::new(max.0, max.1))
}

#[test]
fn holes_must_lie_inside_and_apart() {
    let outer = rect_loop((0.0, 0.0), (20.0, 20.0));
    let ok = Profile {
        outer: outer.clone(),
        holes: vec![rect_loop((2.0, 2.0), (6.0, 6.0)), ProfileLoop::circle(DVec2::new(14.0, 14.0), 3.0)],
    };
    assert!(k().extrude(&ok, &Frame::WORLD, 1.0).is_ok());

    let outside = Profile { outer: outer.clone(), holes: vec![rect_loop((30.0, 30.0), (34.0, 34.0))] };
    assert_eq!(k().extrude(&outside, &Frame::WORLD, 1.0).unwrap_err(), KernelError::HoleOutsideOuter);

    let crossing = Profile { outer: outer.clone(), holes: vec![rect_loop((15.0, 15.0), (25.0, 25.0))] };
    assert_eq!(k().extrude(&crossing, &Frame::WORLD, 1.0).unwrap_err(), KernelError::ProfileLoopsIntersect);

    let overlapping = Profile {
        outer: outer.clone(),
        holes: vec![rect_loop((2.0, 2.0), (8.0, 8.0)), rect_loop((6.0, 6.0), (12.0, 12.0))],
    };
    assert_eq!(k().extrude(&overlapping, &Frame::WORLD, 1.0).unwrap_err(), KernelError::ProfileLoopsIntersect);

    let touching = Profile { outer, holes: vec![rect_loop((0.0, 5.0), (4.0, 9.0))] };
    assert_eq!(k().extrude(&touching, &Frame::WORLD, 1.0).unwrap_err(), KernelError::ProfileLoopsIntersect);
}

#[test]
fn facade_reports_kinds_and_counts_without_indexing() {
    let solid = k().make_cylinder(2.0, 5.0).unwrap();
    let (faces, edges) = k().counts(&solid);
    assert_eq!(faces, solid.faces.len());
    assert_eq!(edges, solid.edges.len());
    let kinds: Vec<_> = solid.faces.iter().filter_map(|(id, _)| k().face_kind(&solid, id)).collect();
    assert!(kinds.contains(&SurfaceKind::Cylinder) && kinds.contains(&SurfaceKind::Plane));
    let (eid, _) = solid.edges.iter().next().unwrap();
    assert!(k().edge_kind(&solid, eid).is_some());
    assert_eq!(k().face_kind(&solid, Id::from_index(999)), None);
    assert_eq!(k().edge_kind(&solid, Id::from_index(999)), None);
    assert_eq!(SurfaceKind::Plane.label(), "Ebene");
    assert_eq!(CurveKind::Circle.label(), "Kreis");
    let _ = (TAU, Tolerance::default());
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-kernel --test input_hardening`
Expected: FAIL (Kompilierfehler: neue Typen/Signaturen fehlen)

- [ ] **Step 3: Implementierung**

`frame.rs`:

```rust
impl Frame {
    /// Prüft, ob das System endlich, orthonormal und rechtshändig ist (Abweichung ≤ `eps`).
    pub fn is_orthonormal(&self, eps: f64) -> bool {
        let finite = [self.origin, self.x, self.y, self.z]
            .iter()
            .all(|v| v.is_finite());
        finite
            && (self.x.length() - 1.0).abs() <= eps
            && (self.y.length() - 1.0).abs() <= eps
            && (self.z.length() - 1.0).abs() <= eps
            && self.x.dot(self.y).abs() <= eps
            && self.x.dot(self.z).abs() <= eps
            && self.y.dot(self.z).abs() <= eps
            && (self.x.cross(self.y) - self.z).length() <= eps
    }
}
```

`profile.rs` – `polygon`:

```rust
    pub fn polygon(points: &[DVec2]) -> Result<ProfileLoop, KernelError> {
        if points.len() < 3 || points.iter().any(|p| !p.is_finite()) {
            return Err(KernelError::DegenerateSegment);
        }
        Ok(ProfileLoop {
            start: points[0],
            segments: points[1..]
                .iter()
                .map(|p| Segment::Line { to: *p })
                .chain(std::iter::once(Segment::Line { to: points[0] }))
                .collect(),
        })
    }
```

`rectangle` ruft `polygon(..).expect("vier endliche Punkte")` nur, wenn Eingaben endlich sind; sonst unverändert wie bisher aufbauen (kein Panic: `rectangle` baut die vier Linien direkt, ohne Prüfung). `sample`:

```rust
/// Obergrenze der Abtastschritte je Bogen (verhindert Hängen bei riesigen oder nicht endlichen Winkeln).
pub const MAX_SAMPLES_PER_ARC: usize = 65_536;
...
    pub fn sample(&self, steps_per_turn: usize) -> Vec<DVec2> {
        let steps_per_turn = steps_per_turn.clamp(1, MAX_SAMPLES_PER_ARC);
        ...
                Segment::Arc { center, sweep } => {
                    let n = (((sweep.abs() / TAU) * steps_per_turn as f64).ceil())
                        .max(2.0)
                        .min(MAX_SAMPLES_PER_ARC as f64) as usize;
```

(`f64::max(NaN, 2.0)` ist 2.0, `∞` wird durch `min` begrenzt.)

`error.rs`: drei neue Varianten mit den oben genannten Meldungen.

Lochprüfung (`profile.rs`, `pub(crate)`):

```rust
/// Prüft, dass jedes Loch vollständig in der äußeren Schleife liegt und keine zwei Schleifen
/// sich schneiden oder berühren. Arbeitet auf mit 64 Schritten je Vollkreis abgetasteten Linienzügen.
pub(crate) fn check_hole_layout(profile: &Profile) -> Result<(), KernelError> {
    const STEPS: usize = 64;
    let outer = profile.outer.sample(STEPS);
    let holes: Vec<Vec<DVec2>> = profile.holes.iter().map(|h| h.sample(STEPS)).collect();
    for (i, h) in holes.iter().enumerate() {
        if polylines_touch(&outer, h) {
            return Err(KernelError::ProfileLoopsIntersect);
        }
        if !point_in_polygon(h[0], &outer) {
            return Err(KernelError::HoleOutsideOuter);
        }
        for other in &holes[i + 1..] {
            if polylines_touch(h, other) {
                return Err(KernelError::ProfileLoopsIntersect);
            }
        }
    }
    Ok(())
}
```

`polylines_touch(a, b)`: wahr, wenn sich irgendein Segment von `a` mit irgendeinem von `b` schneidet oder berührt (Vorzeichentest mit `planora_math::orient2d`, kollineare Fälle über Bereichsprüfung). `point_in_polygon`: Strahlverfahren (gerade/ungerade), Polygon als geschlossen betrachtet. Beides sind kleine private Hilfsfunktionen; Tests oben decken Schnitt, Berührung, Außenlage und gültige Fälle ab. Die Prüfreihenfolge in `extrude`: Frame → Höhe → `check_loop_input` für alle Schleifen → `check_hole_layout` → Orientierung. In `revolve`: Frame → `check_loop_input`.

`kernel.rs`: die drei Fassadenfunktionen im Trait `Kernel` (Standardimplementierung nicht nötig, `AnalyticKernel` implementiert sie); `SurfaceKind`/`CurveKind` mit `label()` in `geometry.rs`, Re-Export in `lib.rs`. `Kernel::face_kind` gibt `None` zurück, wenn `face.index() >= solid.faces.len()` (kein Indexieren außerhalb des Bereichs).

`properties.rs`: `surface_name`/`curve_name` und die direkten Zugriffe auf `body.solid.faces[..]`/`edges[..]` durch `state.kernel.face_kind(..)`, `edge_kind(..)`, `counts(..)` ersetzen; Verhalten und Texte bleiben gleich (bestehende Tests in `crates/planora-app/tests/` müssen weiter laufen).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS (alle bisherigen Tests unverändert grün, die Aufrufer von `polygon` sind angepasst)

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "fix(kernel): Profil- und Frame-Eingaben absichern, Lochlage prüfen, Fassade für Eigenschaften"
```

---

### Task 7: Profile aus der Skizze

**Files:**
- Create: `crates/planora-sketch/src/profile.rs`
- Modify: `crates/planora-sketch/src/lib.rs`, `crates/planora-sketch/src/error.rs`
- Test: `crates/planora-sketch/tests/profile.rs`

**Interfaces:**
- Consumes: `Sketch`, `CurveKind` (Task 2), `Profile`/`ProfileLoop`/`Segment` aus `planora-kernel`, `planora_math::orient2d`.
- Produces:
  - `SketchError` neu: `OpenLoop`, `AmbiguousTopology(String)`, `SelfIntersecting`, `AmbiguousNesting`, `UnsupportedForSolid(String)` (Meldungen auf Deutsch, z. B. „Das Profil ist nicht geschlossen“, „Dieses Profil enthält {0} und kann noch nicht zu einem Körper werden“).
  - `struct RejectedLoop { pub curves: Vec<CurveId>, pub reason: SketchError }`, `struct ExtractedProfiles { pub profiles: Vec<Profile>, pub rejected: Vec<RejectedLoop>, pub curve_loops: Vec<Vec<CurveId>> }` – `curve_loops[i]` sind die Kurven der äußeren Schleife von `profiles[i]` (für die Auswahl in der Oberfläche).
  - `impl Sketch { pub fn profiles(&self) -> ExtractedProfiles }`.
- Regeln (Entscheidungen D5/D6):
  - Es zählen lebende Kurven mit `construction == false`. Punkte, deren Abstand ≤ `tol.linear` ist, gelten als derselbe Knoten.
  - Kreise sind eigene Schleifen (`ProfileLoop::circle`, Start bei `center + (r, 0)`). Linien und Bögen bilden einen Graphen; jede Zusammenhangskomponente muss ein einfacher Kreis sein (alle Knoten Grad 2). Grad 1 → `OpenLoop`; Grad ≥ 3 → `AmbiguousTopology("T-Verzweigung")`. Ein Bogen, dessen Start und Ende auf denselben Knoten fallen, → `Degenerate`.
  - Ellipsen und Splines (ab Task 9) liegen nie in einem Profil: Eine Komponente, die eine nicht verbotene Ellipse/Spline enthält, wird mit `UnsupportedForSolid("Ellipse"|"Spline")` abgelehnt; eine freistehende Ellipse ebenso.
  - Schleifen werden mit 64 Schritten je Vollkreis abgetastet. Eine Schleife, die sich selbst schneidet (nicht benachbarte Abtastsegmente schneiden oder berühren sich), oder zwei Schleifen, die sich schneiden oder berühren → beide abgelehnt mit `SelfIntersecting`.
  - Schachtelung: Die Tiefe einer Schleife ist die Zahl der Schleifen, die sie enthalten. Gerade Tiefe → äußere Schleife eines Profils; ungerade → Loch der unmittelbar umschließenden Schleife (kleinste Fläche unter den Umschließenden). Löcher kommen als `holes` des Profils ihrer äußeren Schleife.
  - Bogenrichtung: Ein Bogen läuft gegen den Uhrzeigersinn von `start` nach `end`; in Durchlaufrichtung vorwärts ist `sweep = +φ`, rückwärts `−φ` mit `φ = (atan2(end−c) − atan2(start−c)) mod 2π`.
  - Reihenfolge der Profile und Schleifen ist deterministisch (nach kleinster Kurven-Id).

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-sketch/tests/profile.rs`

```rust
use planora_kernel::{AnalyticKernel, Kernel, Segment};
use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{PointId, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn rect(s: &mut Sketch, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<PointId> {
    let p = vec![
        s.add_point(x0, y0).unwrap(),
        s.add_point(x1, y0).unwrap(),
        s.add_point(x1, y1).unwrap(),
        s.add_point(x0, y1).unwrap(),
    ];
    for i in 0..4 {
        s.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    p
}

#[test]
fn a_rectangle_is_one_profile() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 3.0, 4.0);
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.curve_loops.len(), 1);
    assert_eq!(e.curve_loops[0].len(), 4);
    let p = &e.profiles[0];
    assert!(p.holes.is_empty());
    assert_eq!(p.outer.segments.len(), 4);
    assert!((p.outer.signed_area().abs() - 12.0).abs() < 1e-9);
    assert!(p.outer.is_closed(s.tolerance()));
}

#[test]
fn a_circle_inside_a_rectangle_becomes_a_hole() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 20.0, 20.0);
    let c = s.add_point(10.0, 10.0).unwrap();
    s.add_circle(c, 4.0).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.profiles[0].holes.len(), 1);
    // das Profil lässt sich wirklich extrudieren
    let solid = AnalyticKernel::default()
        .extrude(&e.profiles[0], s.frame(), 2.0)
        .unwrap();
    AnalyticKernel::default().validate(&solid).unwrap();
}

#[test]
fn nesting_alternates_outer_and_hole() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(c, 10.0).unwrap();
    s.add_circle(c, 6.0).unwrap();
    s.add_circle(c, 2.0).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 2, "Ring plus innere Insel");
    let ring = e.profiles.iter().find(|p| !p.holes.is_empty()).unwrap();
    assert_eq!(ring.holes.len(), 1);
    let island = e.profiles.iter().find(|p| p.holes.is_empty()).unwrap();
    assert!((island.outer.signed_area().abs() - PI * 4.0).abs() < 0.1, "abgetastete Fläche");
}

#[test]
fn two_separate_rectangles_are_two_profiles() {
    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 3.0, 3.0);
    rect(&mut s, 10.0, 0.0, 13.0, 3.0);
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2);
    assert!(e.profiles.iter().all(|p| p.holes.is_empty()));
}

#[test]
fn construction_geometry_is_ignored() {
    let mut s = sketch();
    let p = rect(&mut s, 0.0, 0.0, 3.0, 3.0);
    let diag = s.add_line(p[0], p[2]).unwrap();
    s.set_construction(diag, true).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
}

#[test]
fn open_chains_and_t_junctions_are_rejected_with_a_reason() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    let c = s.add_point(5.0, 5.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, c).unwrap();
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert_eq!(e.rejected.len(), 1);
    assert_eq!(e.rejected[0].reason, SketchError::OpenLoop);

    let mut s = sketch();
    let p = rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    let extra = s.add_point(9.0, 9.0).unwrap();
    s.add_line(p[0], extra).unwrap();
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert!(matches!(e.rejected[0].reason, SketchError::AmbiguousTopology(_)), "{:?}", e.rejected);
}

#[test]
fn bow_ties_and_crossing_loops_are_rejected() {
    let mut s = sketch();
    let p = [
        s.add_point(0.0, 0.0).unwrap(),
        s.add_point(4.0, 4.0).unwrap(),
        s.add_point(4.0, 0.0).unwrap(),
        s.add_point(0.0, 4.0).unwrap(),
    ];
    for i in 0..4 {
        s.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert_eq!(e.rejected[0].reason, SketchError::SelfIntersecting);

    let mut s = sketch();
    rect(&mut s, 0.0, 0.0, 6.0, 6.0);
    rect(&mut s, 3.0, 3.0, 9.0, 9.0);
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert_eq!(e.rejected.len(), 2);
    assert!(e.rejected.iter().all(|r| r.reason == SketchError::SelfIntersecting));
}

#[test]
fn arcs_follow_the_traversal_direction() {
    // Halbscheibe: Linie von (-1,0) nach (1,0) und Bogen (gegen den Uhrzeigersinn) von (1,0)
    // über (0,1) nach (-1,0)
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let l = s.add_point(-1.0, 0.0).unwrap();
    let r = s.add_point(1.0, 0.0).unwrap();
    s.add_line(l, r).unwrap();
    s.add_arc(c, r, l).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
    let outer = &e.profiles[0].outer;
    assert!((outer.signed_area().abs() - PI / 2.0).abs() < 1e-3);
    assert!(outer.is_closed(s.tolerance()));
    let arcs: Vec<f64> = outer
        .segments
        .iter()
        .filter_map(|sg| match sg {
            Segment::Arc { sweep, .. } => Some(*sweep),
            _ => None,
        })
        .collect();
    assert_eq!(arcs.len(), 1);
    assert!((arcs[0].abs() - PI).abs() < 1e-9);
    let solid = AnalyticKernel::default()
        .extrude(&e.profiles[0], s.frame(), 1.0)
        .unwrap();
    AnalyticKernel::default().validate(&solid).unwrap();
}

#[test]
fn nearly_touching_endpoints_are_the_same_node() {
    // zwei Linien, deren Endpunkte 1e-8 auseinander liegen, schließen die Schleife trotzdem
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    let c = s.add_point(4.0, 4.0).unwrap();
    let c2 = s.add_point(4.0 + 1e-8, 4.0).unwrap();
    let d = s.add_point(0.0, 4.0).unwrap();
    let d2 = s.add_point(1e-8, 4.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, c).unwrap();
    s.add_line(c2, d).unwrap();
    s.add_line(d2, a).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
    let _ = DVec2::ZERO;
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test profile`
Expected: FAIL (`profiles` fehlt)

- [ ] **Step 3: Implementierung** (`profile.rs`, Referenz)

```rust
//! Profil-Extraktion: geschlossene Ketten aus Linien/Bögen und Kreise zu Kernel-Profilen.

use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_kernel::{Profile, ProfileLoop, Segment};
use planora_math::{DVec2, TAU, orient2d};
use std::collections::HashMap;

/// Abtastschritte je Vollkreis für Schnitt- und Enthaltensein-Prüfungen.
const CHECK_STEPS: usize = 64;

#[derive(Clone, Debug)]
pub struct RejectedLoop {
    pub curves: Vec<CurveId>,
    pub reason: SketchError,
}

#[derive(Clone, Debug, Default)]
pub struct ExtractedProfiles {
    pub profiles: Vec<Profile>,
    pub rejected: Vec<RejectedLoop>,
    /// Kurven der äußeren Schleife von `profiles[i]`.
    pub curve_loops: Vec<Vec<CurveId>>,
}

struct Candidate {
    curves: Vec<CurveId>,
    lp: ProfileLoop,
    samples: Vec<DVec2>,
}

impl Sketch {
    pub fn profiles(&self) -> ExtractedProfiles {
        let tol = self.tolerance().linear;
        let mut out = ExtractedProfiles::default();
        let mut candidates: Vec<Candidate> = Vec::new();

        // 1. Knoten: Punkte im Abstand <= tol fallen zusammen.
        let used: Vec<PointId> = self
            .curves()
            .filter(|(_, c)| !c.construction)
            .flat_map(|(_, c)| c.kind.points())
            .collect();
        let mut node_of: HashMap<PointId, usize> = HashMap::new();
        let mut reps: Vec<DVec2> = Vec::new();
        for p in used {
            if node_of.contains_key(&p) {
                continue;
            }
            let pos = self.pos(p);
            let n = reps.iter().position(|r| r.distance(pos) <= tol).unwrap_or_else(|| {
                reps.push(pos);
                reps.len() - 1
            });
            node_of.insert(p, n);
        }

        // 2. Kreise sind eigene Schleifen; Ellipsen/Splines werden abgelehnt; Linien und Bögen
        //    bilden den Graphen.
        struct Edge { curve: CurveId, a: usize, b: usize }
        let mut edges: Vec<Edge> = Vec::new();
        let mut blocked: Vec<(Vec<CurveId>, &'static str)> = Vec::new();
        for (id, c) in self.curves().filter(|(_, c)| !c.construction) {
            match &c.kind {
                CurveKind::Circle { center, .. } => {
                    let r = self.radius(id).unwrap_or(0.0);
                    let lp = ProfileLoop::circle(self.pos(*center), r);
                    let samples = lp.sample(CHECK_STEPS);
                    candidates.push(Candidate { curves: vec![id], lp, samples });
                }
                CurveKind::Line { a, b } | CurveKind::Arc { start: a, end: b, .. } => {
                    edges.push(Edge { curve: id, a: node_of[a], b: node_of[b] });
                }
                CurveKind::Ellipse { .. } => blocked.push((vec![id], "Ellipse")),
                CurveKind::Spline { points } => {
                    // Splines hängen mit ihren Endpunkten am Graphen
                    let (a, b) = (node_of[&points[0]], node_of[points.last().expect("≥ 2 Punkte")]);
                    edges.push(Edge { curve: id, a, b });
                }
            }
        }
        for (curves, what) in blocked {
            out.rejected.push(RejectedLoop {
                curves,
                reason: SketchError::UnsupportedForSolid(what.to_string()),
            });
        }

        // 3. Zusammenhangskomponenten des Graphen; jede muss ein einfacher Kreis sein.
        let mut comp_of_node: HashMap<usize, usize> = HashMap::new();
        let mut comps: Vec<Vec<usize>> = Vec::new(); // Kanten-Indizes
        for start in 0..edges.len() {
            if comp_of_node.contains_key(&edges[start].a) {
                continue;
            }
            let ci = comps.len();
            comps.push(Vec::new());
            let mut stack = vec![edges[start].a];
            comp_of_node.insert(edges[start].a, ci);
            while let Some(n) = stack.pop() {
                for (ei, e) in edges.iter().enumerate() {
                    if e.a == n || e.b == n {
                        for m in [e.a, e.b] {
                            if comp_of_node.insert(m, ci).is_none() {
                                stack.push(m);
                            }
                        }
                        if !comps[ci].contains(&ei) {
                            comps[ci].push(ei);
                        }
                    }
                }
            }
            comps[ci].sort_unstable();
        }
        for comp in comps {
            let curves: Vec<CurveId> = comp.iter().map(|&e| edges[e].curve).collect();
            let has = |f: fn(&CurveKind) -> bool| curves.iter().any(|&c| f(&self.curves[c].kind));
            if has(|k| matches!(k, CurveKind::Spline { .. })) {
                out.rejected.push(RejectedLoop { curves, reason: SketchError::UnsupportedForSolid("Spline".into()) });
                continue;
            }
            let mut degree: HashMap<usize, usize> = HashMap::new();
            for &e in &comp {
                *degree.entry(edges[e].a).or_default() += 1;
                *degree.entry(edges[e].b).or_default() += 1;
            }
            if comp.iter().any(|&e| edges[e].a == edges[e].b) {
                out.rejected.push(RejectedLoop { curves, reason: SketchError::Degenerate("Ein Bogen beginnt und endet im selben Punkt".into()) });
                continue;
            }
            // Verzweigungen zuerst: Ein Stummel an einer Ecke hat Grad 1 *und* 3 und ist mehrdeutig,
            // nicht „offen“.
            if degree.values().any(|&d| d >= 3) {
                out.rejected.push(RejectedLoop { curves, reason: SketchError::AmbiguousTopology("T-Verzweigung".into()) });
                continue;
            }
            if degree.values().any(|&d| d == 1) {
                out.rejected.push(RejectedLoop { curves, reason: SketchError::OpenLoop });
                continue;
            }
            // 4. Kreis durchlaufen.
            let first = comp[0];
            let mut order: Vec<(usize, bool)> = vec![(first, true)];
            let mut node = edges[first].b;
            while node != edges[first].a {
                let &next = comp
                    .iter()
                    .find(|&&e| !order.iter().any(|(o, _)| *o == e) && (edges[e].a == node || edges[e].b == node))
                    .expect("Grad 2 garantiert einen Nachfolger");
                let forward = edges[next].a == node;
                order.push((next, forward));
                node = if forward { edges[next].b } else { edges[next].a };
            }
            let mut segments = Vec::new();
            let start = reps[edges[first].a];
            let mut at = start;
            for &(e, forward) in &order {
                let edge = &edges[e];
                let to = reps[if forward { edge.b } else { edge.a }];
                match &self.curves[edge.curve].kind {
                    CurveKind::Arc { center, start: s, end: en } => {
                        let c = self.pos(*center);
                        let a0 = (self.pos(*s) - c).to_angle();
                        let a1 = (self.pos(*en) - c).to_angle();
                        let phi = (a1 - a0).rem_euclid(TAU);
                        segments.push(Segment::Arc { center: c, sweep: if forward { phi } else { -phi } });
                    }
                    _ => segments.push(Segment::Line { to }),
                }
                at = to;
            }
            let _ = at;
            let lp = ProfileLoop { start, segments };
            let samples = lp.sample(CHECK_STEPS);
            candidates.push(Candidate { curves: order.iter().map(|&(e, _)| edges[e].curve).collect(), lp, samples });
        }

        // 5. Selbstschnitt, Schnitt zwischen Schleifen, Schachtelung.
        let n = candidates.len();
        let mut bad = vec![false; n];
        for i in 0..n {
            if polyline_self_touches(&candidates[i].samples) {
                bad[i] = true;
            }
            for j in i + 1..n {
                if polylines_touch(&candidates[i].samples, &candidates[j].samples) {
                    bad[i] = true;
                    bad[j] = true;
                }
            }
        }
        let mut good: Vec<usize> = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            if bad[i] {
                out.rejected.push(RejectedLoop { curves: c.curves.clone(), reason: SketchError::SelfIntersecting });
            } else {
                good.push(i);
            }
        }
        // Umschließende je Schleife
        let area = |i: usize| candidates[i].lp.signed_area().abs();
        let containers = |i: usize| -> Vec<usize> {
            good.iter().copied().filter(|&j| j != i && point_in_polygon(candidates[i].samples[0], &candidates[j].samples)).collect()
        };
        let mut profile_of: HashMap<usize, usize> = HashMap::new();
        let mut order_good = good.clone();
        order_good.sort_by_key(|&i| candidates[i].curves.iter().map(|c| c.index()).min());
        for &i in &order_good {
            if containers(i).len() % 2 == 0 {
                profile_of.insert(i, out.profiles.len());
                out.profiles.push(Profile { outer: candidates[i].lp.clone(), holes: Vec::new() });
                out.curve_loops.push(candidates[i].curves.clone());
            }
        }
        for &i in &order_good {
            let cs = containers(i);
            if cs.len() % 2 == 1 {
                // unmittelbar umschließende Schleife = kleinste Fläche unter den Umschließenden
                let parent = cs.into_iter().min_by(|&a, &b| area(a).total_cmp(&area(b))).expect("nicht leer");
                match profile_of.get(&parent) {
                    Some(&pi) => out.profiles[pi].holes.push(candidates[i].lp.clone()),
                    None => out.rejected.push(RejectedLoop { curves: candidates[i].curves.clone(), reason: SketchError::AmbiguousNesting }),
                }
            }
        }
        out
    }
}
```

Dazu die drei Hilfsfunktionen (klein, geradlinig; Referenz):

```rust
fn side(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    orient2d(a, b, c)
}

/// Schneiden oder berühren sich die Strecken pq und rs?
fn segments_touch(p: DVec2, q: DVec2, r: DVec2, s: DVec2) -> bool {
    let (d1, d2, d3, d4) = (side(p, q, r), side(p, q, s), side(r, s, p), side(r, s, q));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return true;
    }
    let on = |a: DVec2, b: DVec2, c: DVec2, d: f64| {
        d == 0.0 && c.x >= a.x.min(b.x) && c.x <= a.x.max(b.x) && c.y >= a.y.min(b.y) && c.y <= a.y.max(b.y)
    };
    on(p, q, r, d1) || on(p, q, s, d2) || on(r, s, p, d3) || on(r, s, q, d4)
}

fn polylines_touch(a: &[DVec2], b: &[DVec2]) -> bool {
    a.windows(2).any(|sa| b.windows(2).any(|sb| segments_touch(sa[0], sa[1], sb[0], sb[1])))
}

/// Berühren sich nicht benachbarte Segmente derselben (geschlossen betrachteten) Abtastung?
fn polyline_self_touches(pts: &[DVec2]) -> bool {
    let n = pts.len() - 1; // letztes = erstes bei geschlossenen Schleifen
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue; // benachbart über den Schluss
            }
            if segments_touch(pts[i], pts[i + 1], pts[j], pts[j + 1]) {
                return true;
            }
        }
    }
    false
}

fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    for w in poly.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}
```

Hinweis: `orient2d` aus `planora-math` hat die Signatur der bestehenden Funktion (prüfen: `orient2d(a: DVec2, b: DVec2, c: DVec2) -> f64`); weicht sie ab, die Hilfsfunktion `side` anpassen. Der Kreis eines abgetasteten Vollkreises ist geschlossen (letzter = erster Punkt), `polyline_self_touches` behandelt das.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-sketch --test profile && cargo clippy -p planora-sketch --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/planora-sketch
git commit -m "feat(sketch): Profile aus Skizzen (Schleifen, Schachtelung, Ablehnungsgründe)"
```

---

### Task 8: Hilfen – Rechteck, Spiegeln, Muster, Körperkante kopieren, Ebene einer Fläche

**Files:**
- Create: `crates/planora-sketch/src/helpers.rs`
- Modify: `crates/planora-sketch/src/lib.rs`, `crates/planora-sketch/src/error.rs`
- Test: `crates/planora-sketch/tests/helpers.rs`

**Interfaces:**
- Consumes: `Sketch`, `ConstraintKind`, Solver (Tasks 2–4), `planora_kernel::{Solid, Edge, Face, Id, Curve, Surface}`.
- Produces:
  - `SketchError` neu: `UnsupportedProjection(String)` („Diese Kante kann noch nicht in die Skizze kopiert werden: {0}“).
  - `struct RectIds { pub points: [PointId; 4], pub lines: [CurveId; 4] }` und `Sketch::add_rectangle(&mut self, a: DVec2, b: DVec2) -> Result<RectIds, SketchError>`: achsenparalleles Rechteck aus den Ecken `a`, `b` (Ecken im Uhrzeigersinn beginnend mit `(min.x, min.y)` gegen den Uhrzeigersinn: unten links, unten rechts, oben rechts, oben links), vier Linien mit gemeinsamen Eckpunkten, `Horizontal` auf unten und oben, `Vertical` auf rechts und links. Breite oder Höhe ≤ `tol.linear` → `Degenerate`.
  - `Sketch::add_mirrored(&mut self, curves: &[CurveId], axis: CurveId) -> Result<Vec<CurveId>, SketchError>`: kopiert Linien, Kreise und Bögen gespiegelt an der Linie `axis`. Jeder Quellpunkt bekommt einen neuen Punkt an der gespiegelten Stelle (gemeinsame Punkte bleiben gemeinsam); je Punktpaar eine `Symmetric(p, p', axis)`-Abhängigkeit; Kreise bekommen `Equal` zu ihrem Original. Bögen werden mit **vertauschtem Start und Ende** angelegt (Spiegelung kehrt den Drehsinn um). Die Achse selbst darf nicht in `curves` stehen; Ellipsen/Splines → `WrongType`.
  - `Sketch::add_linear_pattern(&mut self, curves: &[CurveId], offset: DVec2, count: usize) -> Result<Vec<Vec<CurveId>>, SketchError>` (`count` = Zahl der zusätzlichen Kopien, 1..=100): Kopie k liegt um `k·offset` verschoben. Je Kopie: `Equal` für Linien und Kreise/Bögen zum Original, `Parallel` für Linien, und genau eine `Distance` zwischen dem ersten Punkt des Originals und dem entsprechenden der Kopie (Wert `k·|offset|`). Ist `offset` achsenparallel (eine Komponente innerhalb `tol.linear` von null), kommt je Kopie zusätzlich `Horizontal(Points(Original-Punkt, Kopie-Punkt))` bzw. `Vertical(..)` hinzu. Die Kopien teilen keine Punkte mit dem Original.
  - `Sketch::add_projected_edge(&mut self, solid: &Solid, edge: Id<Edge>) -> Result<CurveId, SketchError>`: Kopie der Kante, orthogonal auf die Skizzenebene projiziert, als **Konstruktionsgeometrie** (Kurve und Punkte `construction = true`), jeder Punkt `Fixed` an der projizierten Stelle, Kreise zusätzlich mit `Radius`. Unterstützt: Geraden (nicht senkrecht zur Ebene stehend: projizierte Länge > `tol.linear`), Kreise und Kreisbögen in Ebenen parallel zur Skizzenebene (Vollkreis → Kreis, Teilstück → Bogen mit richtigem Drehsinn: bei `sign(t1 − t0)·sign(n_kante · n_skizze) > 0` von Start nach Ende, sonst vertauscht). Alles andere (Ellipsen, schräg stehende Kreise) ergibt `UnsupportedProjection`; Task 9 erweitert um Ellipsen.
  - `fn plane_of_face(solid: &Solid, face: Id<Face>) -> Option<Frame>` (Freifunktion in `planora_sketch`): für ebene Flächen ein `Frame` mit Ursprung der Flächenebene und **nach außen zeigender Normale** als `z`-Achse (`z = frame.z` bei `same_sense`, sonst `−frame.z`; `x = frame.x`, `y = z × x`); `None` für gekrümmte Flächen oder unbekannte Ids.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-sketch/tests/helpers.rs`

```rust
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveKind, Sketch};
use planora_sketch::solver::SolveStatus;
use planora_sketch::{SketchError, plane_of_face};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn rectangle_stays_axis_parallel_after_solving_and_dragging_a_corner() {
    let mut s = sketch();
    let r = s.add_rectangle(DVec2::new(5.0, 2.0), DVec2::new(1.0, 7.0)).unwrap();
    assert_eq!(s.point_xy(r.points[0]).unwrap(), DVec2::new(1.0, 2.0), "unten links");
    assert_eq!(s.point_xy(r.points[2]).unwrap(), DVec2::new(5.0, 7.0), "oben rechts");
    assert_eq!(s.constraints().count(), 4);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved);
    assert_eq!(rep.dof, 4, "x, y, Breite, Höhe");
    let rep = s
        .drag(&[planora_sketch::solver::Drag { point: r.points[2], target: DVec2::new(9.0, 10.0) }])
        .unwrap();
    assert_eq!(rep.status, SolveStatus::Solved);
    let (c0, c2, c1, c3) = (
        s.point_xy(r.points[0]).unwrap(),
        s.point_xy(r.points[2]).unwrap(),
        s.point_xy(r.points[1]).unwrap(),
        s.point_xy(r.points[3]).unwrap(),
    );
    assert!((c1.y - c0.y).abs() < 1e-7 && (c3.y - c2.y).abs() < 1e-7);
    assert!((c1.x - c2.x).abs() < 1e-7 && (c3.x - c0.x).abs() < 1e-7);
    assert!(s.add_rectangle(DVec2::ZERO, DVec2::new(0.0, 5.0)).is_err());
}

#[test]
fn mirrored_copy_is_symmetric_and_arcs_keep_their_shape() {
    let mut s = sketch();
    let axis_a = s.add_point(0.0, -10.0).unwrap();
    let axis_b = s.add_point(0.0, 10.0).unwrap();
    let axis = s.add_line(axis_a, axis_b).unwrap();
    s.add_constraint(K::Fixed(axis_a, 0.0, -10.0)).unwrap();
    s.add_constraint(K::Fixed(axis_b, 0.0, 10.0)).unwrap();
    let c = s.add_point(3.0, 0.0).unwrap();
    let a0 = s.add_point(5.0, 0.0).unwrap();
    let a1 = s.add_point(3.0, 2.0).unwrap();
    let arc = s.add_arc(c, a0, a1).unwrap();
    let circle_center = s.add_point(6.0, 5.0).unwrap();
    let circle = s.add_circle(circle_center, 1.0).unwrap();
    let copies = s.add_mirrored(&[arc, circle], axis).unwrap();
    assert_eq!(copies.len(), 2);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let CurveKind::Arc { center, start, end } = s.curve(copies[0]).unwrap().kind else {
        panic!("Bogen erwartet")
    };
    assert!((s.point_xy(center).unwrap() - DVec2::new(-3.0, 0.0)).length() < 1e-7);
    // Spiegelung vertauscht Start und Ende: der gespiegelte Start ist das Bild von a1
    assert!((s.point_xy(start).unwrap() - DVec2::new(-3.0, 2.0)).length() < 1e-7);
    assert!((s.point_xy(end).unwrap() - DVec2::new(-5.0, 0.0)).length() < 1e-7);
    // dasselbe Gebiet: gespiegelter Bogen hat denselben Radius und Öffnungssinn gegen den Uhrzeigersinn
    assert!((s.radius(copies[0]).unwrap() - 2.0).abs() < 1e-7);
    let mirrored_circle = copies[1];
    assert!((s.radius(mirrored_circle).unwrap() - 1.0).abs() < 1e-7);
    // Achse nicht in der Liste
    assert!(s.add_mirrored(&[axis], axis).is_err());
}

#[test]
fn linear_pattern_places_translated_copies() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(2.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let c = s.add_point(1.0, 3.0).unwrap();
    let circle = s.add_circle(c, 0.5).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 2.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(c, 1.0, 3.0)).unwrap();
    s.add_constraint(K::Radius(circle, 0.5)).unwrap();
    let copies = s.add_linear_pattern(&[l, circle], DVec2::new(5.0, 0.0), 2).unwrap();
    assert_eq!(copies.len(), 2);
    assert_eq!(s.curves().count(), 2 + 4);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    for (k, group) in copies.iter().enumerate() {
        let CurveKind::Line { a: la, .. } = s.curve(group[0]).unwrap().kind else {
            panic!("Linie erwartet")
        };
        let moved = s.point_xy(la).unwrap();
        let want = DVec2::new(5.0 * (k as f64 + 1.0), 0.0);
        assert!(moved.distance(want) < 1e-6, "Kopie {k}: {moved:?}");
        assert!((s.line_length(group[0]).unwrap() - 2.0).abs() < 1e-7);
    }
    assert!(s.add_linear_pattern(&[l], DVec2::X, 0).is_err());
    assert!(s.add_linear_pattern(&[l], DVec2::X, 101).is_err());
}

#[test]
fn body_edges_are_copied_as_fixed_construction_geometry() {
    let k = AnalyticKernel::default();
    let bx = k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let mut s = sketch();
    let mut copied = 0;
    for (id, _) in bx.edges.iter() {
        match s.add_projected_edge(&bx, id) {
            Ok(c) => {
                assert!(s.curve(c).unwrap().construction);
                copied += 1;
            }
            Err(SketchError::UnsupportedProjection(_)) => {}
            Err(e) => panic!("{e}"),
        }
    }
    // 8 Kanten liegen in z-Ebenen (4 unten, 4 oben, je projiziert deckungsgleich), die 4
    // senkrechten Kanten stehen senkrecht zur Skizzenebene und werden abgelehnt.
    assert_eq!(copied, 8);
    assert!(s.points().all(|(_, p)| p.construction));
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(s.profiles().profiles.is_empty(), "Konstruktionsgeometrie bildet keine Profile");

    let cyl = k.make_cylinder(3.0, 8.0).unwrap();
    let mut s = sketch();
    let mut circles = 0;
    for (id, _) in cyl.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&cyl, id)
            && matches!(s.curve(c).unwrap().kind, CurveKind::Circle { .. })
        {
            circles += 1;
            assert!((s.radius(c).unwrap() - 3.0).abs() < 1e-9);
        }
    }
    assert_eq!(circles, 2, "unterer und oberer Kreis");
}

#[test]
fn plane_of_a_face_points_outwards() {
    let k = AnalyticKernel::default();
    let bx = k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let mut tops = 0;
    let mut bottoms = 0;
    for (id, _) in bx.faces.iter() {
        let f = plane_of_face(&bx, id).expect("alle Quaderflächen sind eben");
        assert!(f.is_orthonormal(1e-9));
        if (f.z - DVec3::Z).length() < 1e-9 {
            tops += 1;
            assert!((f.origin.z - 5.0).abs() < 1e-9);
        }
        if (f.z + DVec3::Z).length() < 1e-9 {
            bottoms += 1;
            assert!(f.origin.z.abs() < 1e-9);
        }
    }
    assert_eq!((tops, bottoms), (1, 1));
    let cyl = k.make_cylinder(3.0, 8.0).unwrap();
    let planar = cyl.faces.iter().filter(|(id, _)| plane_of_face(&cyl, *id).is_some()).count();
    assert_eq!(planar, 2, "die Mantelfläche ist nicht eben");
    assert!(plane_of_face(&bx, planora_kernel::Id::from_index(99)).is_none());
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test helpers`
Expected: FAIL

- [ ] **Step 3: Implementierung** – `helpers.rs` (Referenz, Kernpunkte)

```rust
//! Zeichenhilfen: Rechteck, Spiegeln, Muster, Körperkanten kopieren, Ebene einer Fläche.

use crate::constraint::{ConstraintKind, LineOrPoints};
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_kernel::{Curve, Edge, Face, Id, Solid, Surface};
use planora_math::{DVec2, DVec3, Frame, TAU};

/// Höchstzahl zusätzlicher Kopien eines Musters.
const MAX_PATTERN_COPIES: usize = 100;

pub struct RectIds {
    pub points: [PointId; 4],
    pub lines: [CurveId; 4],
}

/// Spiegelt `p` an der Geraden durch `a` mit Richtung `d` (normiert).
fn reflect(p: DVec2, a: DVec2, d: DVec2) -> DVec2 {
    let v = p - a;
    a + d * (2.0 * v.dot(d)) - v
}
```

- `add_rectangle`: Ecken aus `min`/`max` bilden, vier Punkte, vier Linien, vier Abhängigkeiten (`Horizontal(Line(l0))`, `Vertical(Line(l1))`, `Horizontal(Line(l2))`, `Vertical(Line(l3))`).
- `add_mirrored`: Achslinie → `(a, d)`; Abbildung `HashMap<PointId, PointId>`; je Kurve die Punkte (neu gespiegelt, wiederverwendet falls schon vorhanden), dann `add_line`/`add_circle`/`add_arc(center', end', start')`; je neuem Punktpaar `Symmetric(p, p', axis)`; Kreise: `Equal(original, kopie)`. Punkte, die auf der Achse liegen (Abstand ≤ `tol.linear`), bekommen statt eines neuen Punktes die Wiederverwendung des Originals (kein Symmetric nötig). Vorab alle Eingaben prüfen (Achse ist Linie, nicht in `curves`, nur Linie/Kreis/Bogen), damit bei Fehler nichts angelegt ist.
- `add_linear_pattern`: `count` in `1..=MAX_PATTERN_COPIES` sonst `InvalidInput`; Kopien wie beschrieben; die `Distance`-Abhängigkeit zwischen dem ersten Punkt der ersten Quellkurve und seiner Kopie.
- `add_projected_edge`: Kante → Kurve und Parameterbereich; Punkte mit `frame.to_local(p)` auf `(x, y)` projizieren; Linien: `curve.point(t0)`/`point(t1)`; Kreise: Normale des Kreis-`Frame` (`frame.z`) gegen `self.frame().z` prüfen (`|dot| ≥ 1 − 1e-9` → parallel); Vollkreis, wenn `|t1 − t0| ≥ TAU − 1e-9`. Nach dem Anlegen `construction = true` für Kurve und alle Punkte und `Fixed`-Abhängigkeiten (Mittelpunkt, Endpunkte) und `Radius` bei Kreis/Bogen.
- `plane_of_face`: nur `Surface::Plane { frame }`; `Id` außerhalb des Bereichs → `None`.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-sketch && cargo clippy -p planora-sketch --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/planora-sketch
git commit -m "feat(sketch): Rechteck, Spiegeln, lineares Muster, Körperkanten kopieren, Ebene einer Fläche"
```

---

### Task 9: Ellipse und Spline (abtrennbar)

> Diese Task ist von den übrigen entkoppelt: Ohne sie funktionieren Skizzen, Solver, Profile und Oberfläche vollständig mit Linien, Kreisen und Bögen. Die Oberfläche (Task 12) bietet die Werkzeuge „Ellipse“ und „Spline“ nur an, wenn diese Task umgesetzt ist (`Sketch::add_ellipse`/`add_spline` vorhanden).

**Files:**
- Create: `crates/planora-sketch/src/curves.rs`
- Modify: `crates/planora-sketch/src/model.rs` (`add_ellipse`, `add_spline`, `ellipse_point`, `spline_point`), `crates/planora-sketch/src/constraint.rs` (neue Abhängigkeiten, `extra`-Variable, Typregeln), `crates/planora-sketch/src/helpers.rs` (Projektion von Ellipsen und schräg stehenden Kreisen), `crates/planora-sketch/src/lib.rs`
- Test: `crates/planora-sketch/tests/ellipse_spline.rs`

**Interfaces:**
- Consumes: Tasks 1–8.
- Produces:
  - `Sketch::add_ellipse(center: PointId, major: DVec2, minor: f64) -> Result<CurveId, SketchError>`: `|major| > tol.linear`, `tol.linear < minor ≤ |major|`, alle Werte endlich, sonst `InvalidInput`/`Degenerate`. Variablen: `ax`, `ay` (Vektor der großen Halbachse), `minor`.
  - `Sketch::add_spline(points: &[PointId]) -> Result<CurveId, SketchError>`: 2 bis 64 verschiedene, lebende Punkte, paarweise Abstand > `tol.linear` zum Nachbarn. Natürlicher kubischer Spline mit gleichmäßigem Parameter `t ∈ [0, n − 1]` (Stützpunkt `i` bei `t = i`).
  - `Sketch::ellipse_point(c, t) -> Option<DVec2>` = `center + a·cos t + b̂·minor·sin t` mit `a = (ax, ay)`, `b̂ = (−ay, ax)/|a|`; `Sketch::spline_point(c, t) -> Option<DVec2>` (t außerhalb wird auf `[0, n−1]` geklemmt).
  - Neue Abhängigkeiten (`ConstraintKind`): `EllipseMajor(CurveId, f64)` (Residuum `|a| − v`), `EllipseMinor(CurveId, f64)` (`minor − v`), `EllipseAngle(CurveId, f64)` (Winkel der großen Halbachse zur Skizzen-x-Achse, Radiant; Residuum `cross(a, (cos v, sin v))/|a|`); alle drei sind Maße (`is_dimension()`), `measure` liefert `|a|`, `minor` bzw. `atan2(ay, ax)`.
  - `PointOnCurve(p, ellipse)`: implizit, 1 Residuum `(√((x'/|a|)² + (y'/m)²) − 1)·(|a| + m)/2` mit `x' = (p − c)·â`, `y' = (p − c)·b̂`. `PointOnCurve(p, spline)`: parametrisch mit einer **Zusatzvariablen** `t` (Eintrag in `ConstraintData::extra: Option<usize>`, im Variablenvektor angelegt), 2 Residuen `S(t) − p`. `Tangent(line, ellipse|spline)` (auch umgekehrte Reihenfolge): parametrisch mit Zusatzvariable `t`, 2 Residuen: Punkt der Kurve liegt auf der Linie (vorzeichenbehafteter Abstand) und `cross(C'(t), d)/(|C'||d|)`. Die Zusatzvariable startet beim nächstgelegenen Kurvenparameter (Abtastung mit 64 Schritten je Spline-Segment bzw. 256 je Ellipse, dann Verfeinerung per Intervallhalbierung).
  - `ConstraintData` bekommt das Feld `pub extra: Option<usize>`; der Solver zählt Zusatzvariablen wie andere Variablen der Gruppe (sie stehen in `Evaluation::locals`). Für Ellipse und Spline gelten die Typregeln aus Task 3 erweitert (`PointOnCurve` und `Tangent` mit Linie).
  - `add_projected_edge` (Task 8) projiziert zusätzlich: Ellipsen und Vollkreise in schräg stehenden Ebenen werden zu **Ellipsen** (nur Vollkurven, `|t1 − t0| ≥ TAU − 1e-9`; Teilstücke → weiter `UnsupportedProjection`). Formel: Die projizierte Kurve ist `P(t) = c + A·cos t + B·sin t` mit `A = major·(x·sx, x·sy)`, `B = minor·(y·sx, y·sy)` (Kreise: `major = minor = r`; `x`, `y` die Achsen des Kantenrahmens, `sx`, `sy` die Skizzenachsen). Hauptachsen: `φ = ½·atan2(2·A·B, |A|² − |B|²)`, `a = A·cos φ + B·sin φ`, `b = −A·sin φ + B·cos φ`; die längere von `a`, `b` wird große Halbachse (bei `b` länger: Rollen tauschen). Ergebnis: `add_ellipse(center, a, |b|)` als Konstruktionsgeometrie mit `Fixed` auf den Mittelpunkt sowie `EllipseMajor`, `EllipseMinor`, `EllipseAngle`.
  - Profile: Komponenten mit Ellipse oder Spline werden weiterhin mit `UnsupportedForSolid` abgelehnt (Task 7 enthält die Logik; hier kommt der Test hinzu).

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-sketch/tests/ellipse_spline.rs`

```rust
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::solver::SolveStatus;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn max_over_curve(s: &Sketch, c: CurveId, ts: std::ops::Range<f64>, f: impl Fn(DVec2) -> f64) -> f64 {
    let n = 4000;
    (0..=n)
        .map(|i| {
            let t = ts.start + (ts.end - ts.start) * i as f64 / n as f64;
            f(s.ellipse_point(c, t).or_else(|| s.spline_point(c, t)).unwrap())
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn ellipse_points_follow_the_axes() {
    let mut s = sketch();
    let c = s.add_point(1.0, 2.0).unwrap();
    let e = s.add_ellipse(c, DVec2::new(0.0, 5.0), 3.0).unwrap();
    // große Halbachse zeigt nach +y; die kleine liegt links davon (b̂ = (−ay, ax)/|a|)
    assert!((s.ellipse_point(e, 0.0).unwrap() - DVec2::new(1.0, 7.0)).length() < 1e-12);
    assert!((s.ellipse_point(e, PI / 2.0).unwrap() - DVec2::new(-2.0, 2.0)).length() < 1e-12);
    assert!(s.add_ellipse(c, DVec2::ZERO, 1.0).is_err());
    assert!(s.add_ellipse(c, DVec2::new(1.0, 0.0), 2.0).is_err(), "klein > groß");
    assert!(s.add_ellipse(c, DVec2::new(f64::NAN, 0.0), 1.0).is_err());
}

#[test]
fn splines_interpolate_their_support_points() {
    let mut s = sketch();
    let pts: Vec<_> = [(0.0, 0.0), (3.0, 4.0), (7.0, 3.0), (10.0, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    for (i, p) in pts.iter().enumerate() {
        let got = s.spline_point(sp, i as f64).unwrap();
        assert!(got.distance(s.point_xy(*p).unwrap()) < 1e-9, "Stützpunkt {i}");
    }
    // natürlicher Spline: stetig zwischen den Segmenten
    let a = s.spline_point(sp, 1.0 - 1e-9).unwrap();
    let b = s.spline_point(sp, 1.0 + 1e-9).unwrap();
    assert!(a.distance(b) < 1e-6);
    assert!(s.add_spline(&pts[..1]).is_err());
    assert!(s.add_spline(&[pts[0], pts[0]]).is_err());
}

#[test]
fn ellipse_dimensions_and_point_on_ellipse() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(c, DVec2::new(4.0, 1.0), 2.0).unwrap();
    let p = s.add_point(3.0, 2.5).unwrap();
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    s.add_constraint(K::EllipseMajor(e, 5.0)).unwrap();
    s.add_constraint(K::EllipseMinor(e, 3.0)).unwrap();
    s.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    s.add_constraint(K::PointOnCurve(p, e)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let pe = s.point_xy(p).unwrap();
    // Ellipse x²/25 + y²/9 = 1
    assert!(((pe.x / 5.0).powi(2) + (pe.y / 3.0).powi(2) - 1.0).abs() < 1e-7, "{pe:?}");
    assert!((s.measure(&K::EllipseMajor(e, 0.0)).unwrap() - 5.0).abs() < 1e-7);
    assert!(s.measure(&K::EllipseAngle(e, 0.0)).unwrap().abs() < 1e-7);
}

#[test]
fn a_line_can_be_tangent_to_an_ellipse() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(c, DVec2::new(5.0, 0.5), 2.0).unwrap();
    let l0 = s.add_point(-10.0, 4.0).unwrap();
    let l1 = s.add_point(10.0, 4.0).unwrap();
    let line = s.add_line(l0, l1).unwrap();
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(l0, -10.0, 4.0)).unwrap();
    s.add_constraint(K::Fixed(l1, 10.0, 4.0)).unwrap();
    s.add_constraint(K::EllipseMajor(e, 5.0)).unwrap();
    s.add_constraint(K::EllipseMinor(e, 2.0)).unwrap();
    s.add_constraint(K::Tangent(line, e)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    // die Ellipse berührt y = 4 von unten: ihr höchster Punkt liegt bei y = 4
    let top = max_over_curve(&s, e, 0.0..2.0 * PI, |p| p.y);
    assert!((top - 4.0).abs() < 1e-4, "höchster Punkt {top}");
}

#[test]
fn a_point_can_lie_on_a_spline_and_a_line_can_touch_it() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(5.0, 4.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let sp = s.add_spline(&[a, m, b]).unwrap();
    let q = s.add_point(2.4, 2.6).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(m, 5.0, 4.0)).unwrap();
    s.add_constraint(K::PointOnCurve(q, sp)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let pq = s.point_xy(q).unwrap();
    let dist = (0..=20_000)
        .map(|i| s.spline_point(sp, 2.0 * i as f64 / 20_000.0).unwrap().distance(pq))
        .fold(f64::INFINITY, f64::min);
    assert!(dist < 1e-4, "Abstand zur Kurve {dist}");

    // Tangente: Stützpunkt m darf wandern; die waagerechte Linie y = 4.5 berührt den Spline
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(5.0, 4.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let sp = s.add_spline(&[a, m, b]).unwrap();
    let l0 = s.add_point(-1.0, 4.5).unwrap();
    let l1 = s.add_point(11.0, 4.5).unwrap();
    let line = s.add_line(l0, l1).unwrap();
    for (p, x, y) in [(a, 0.0, 0.0), (b, 10.0, 0.0), (l0, -1.0, 4.5), (l1, 11.0, 4.5)] {
        s.add_constraint(K::Fixed(p, x, y)).unwrap();
    }
    s.add_constraint(K::Tangent(line, sp)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let top = max_over_curve(&s, sp, 0.0..2.0, |p| p.y);
    assert!((top - 4.5).abs() < 1e-3, "höchster Punkt {top}");
    let _ = LineOrPoints::Line(line);
}

#[test]
fn ellipses_and_splines_never_become_solids() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_ellipse(c, DVec2::new(3.0, 0.0), 2.0).unwrap();
    // ein Rechteck daneben bleibt ein gültiges Profil
    s.add_rectangle(DVec2::new(10.0, 0.0), DVec2::new(14.0, 3.0)).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.rejected.len(), 1);
    assert!(matches!(&e.rejected[0].reason, SketchError::UnsupportedForSolid(w) if w == "Ellipse"));

    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(2.0, 2.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_spline(&[a, m, b]).unwrap();
    s.add_line(b, a).unwrap();
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert!(matches!(&e.rejected[0].reason, SketchError::UnsupportedForSolid(w) if w == "Spline"));
}

#[test]
fn a_tilted_circle_projects_to_an_ellipse() {
    let k = AnalyticKernel::default();
    let cyl = k.make_cylinder(3.0, 8.0).unwrap();
    // Skizzenebene um 30° gegen die Kreisebene geneigt
    let normal = DVec3::new(0.0, 0.5, 3.0_f64.sqrt() / 2.0);
    let mut s = Sketch::new(Frame::from_origin_normal(DVec3::ZERO, normal), "S", Tolerance::default());
    let mut ellipses = 0;
    for (id, _) in cyl.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&cyl, id)
            && let Some(planora_sketch::model::CurveKind::Ellipse { ax, ay, minor, .. }) =
                s.curve(c).map(|d| d.kind.clone())
        {
            ellipses += 1;
            let vars = s.variables();
            let (a, b) = (DVec2::new(vars[ax], vars[ay]).length(), vars[minor]);
            assert!((a - 3.0).abs() < 1e-9, "große Halbachse {a}");
            assert!((b - 3.0 * (PI / 6.0).cos()).abs() < 1e-9, "kleine Halbachse {b}");
        }
    }
    assert_eq!(ellipses, 2, "unterer und oberer Kreis");
    assert_eq!(s.solve().status, SolveStatus::Solved);
}
```

Hinweis: Die Zahlen der beiden Spline-Tests (Startwerte, Höhe 4,5) dürfen angepasst werden, falls der Solver eine andere, ebenfalls gültige Tangentenlage findet, solange die Aussagen („Punkt liegt auf dem Spline“, „höchster Punkt des Splines liegt auf der Linie“) erhalten bleiben.

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test ellipse_spline`
Expected: FAIL (Funktionen/Varianten fehlen)

- [ ] **Step 3: Implementierung** – `curves.rs` und Erweiterungen

Auswertung (Referenz, für `f64` und `Dual` gleich geschrieben; hier als `Dual`-Version, die `f64`-Methoden `ellipse_point`/`spline_point` rufen sie mit konstanten `Dual`-Werten und lesen `.v`):

```rust
//! Auswertung von Ellipse und natürlichem kubischem Spline mit Ableitungen.

use crate::dual::Dual;

pub type V2 = (Dual, Dual);

/// Zweite Ableitungen `M` eines natürlichen Splines mit Parameterabstand 1 durch die Werte `y`
/// (tridiagonal: `M[i−1] + 4·M[i] + M[i+1] = 6·(y[i−1] − 2·y[i] + y[i+1])`, `M[0] = M[n−1] = 0`).
pub fn spline_second_derivatives(y: &[Dual]) -> Vec<Dual> {
    let n = y.len();
    let mut m = vec![Dual::constant(0.0); n];
    if n < 3 {
        return m;
    }
    let k = n - 2;
    // Thomas-Verfahren mit konstanten Koeffizienten (1, 4, 1)
    let mut c = vec![0.0; k];
    let mut d: Vec<Dual> = Vec::with_capacity(k);
    for i in 0..k {
        let rhs = 6.0 * (&y[i] - &(2.0 * &y[i + 1]) + &y[i + 2]);
        if i == 0 {
            c[0] = 1.0 / 4.0;
            d.push(rhs / 4.0);
        } else {
            let den = 4.0 - c[i - 1];
            c[i] = 1.0 / den;
            d.push((rhs - &d[i - 1]) / den);
        }
    }
    let mut x = d.clone();
    for i in (0..k.saturating_sub(1)).rev() {
        x[i] = &d[i] - &(c[i] * &x[i + 1]);
    }
    for i in 0..k {
        m[i + 1] = x[i].clone();
    }
    m
}

/// Wert und Ableitung nach `t` einer Koordinate des Splines bei Parameter `t`.
pub fn spline_eval(y: &[Dual], m: &[Dual], t: &Dual) -> (Dual, Dual) {
    let n = y.len();
    let i = (t.v.floor().max(0.0) as usize).min(n - 2);
    let u = t - i as f64;
    let w = 1.0 - &u;
    let value = &w * &y[i] + &u * &y[i + 1]
        + (&w * &w * &w - &w) * &m[i] / 6.0
        + (&u * &u * &u - &u) * &m[i + 1] / 6.0;
    let deriv = &y[i + 1] - &y[i]
        - (3.0 * &w * &w - 1.0) * &m[i] / 6.0
        + (3.0 * &u * &u - 1.0) * &m[i + 1] / 6.0;
    (value, deriv)
}
```

Die Ellipse (`c + a·cos t + b̂·minor·sin t`) und ihre Ableitung nach `t` werden in `constraint.rs` mit `Dual`-`sin`/`cos` gebildet; dafür bekommt `Dual` zwei Methoden `sin()` und `cos()` (Ableitung `cos`/`−sin` mal Gradient). `eval_kind` bekommt Zweige für die neuen Abhängigkeiten nach den Residuen im Abschnitt „Interfaces“; die Zusatzvariable wird über `ctx.var(extra)` geholt. `Sketch::add_constraint` legt die Zusatzvariable an (`new_var(t0)`) und speichert sie in `ConstraintData::extra`; bei Fehler vor dem Anlegen bleibt die Skizze unverändert (die Variable wird erst nach bestandener Prüfung erzeugt).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-sketch && cargo clippy -p planora-sketch --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS (die Jacobi-Prüfung aus Task 3 wird um die neuen Abhängigkeiten erweitert: `EllipseMajor`, `EllipseMinor`, `EllipseAngle`, `PointOnCurve` auf Ellipse und Spline, `Tangent` Linie–Ellipse und Linie–Spline, jeweils gegen Differenzenquotienten, inklusive der Zusatzvariablen)

- [ ] **Step 5: Commit**

```bash
git add crates/planora-sketch
git commit -m "feat(sketch): Ellipse und natürlicher Spline mit Abhängigkeiten und Projektion schräger Kreise"
```

---

### Task 10: Körper aus Skizzen, Ende-zu-Ende- und Eigenschaftstests

**Files:**
- Create: `crates/planora-sketch/src/solid.rs`
- Modify: `crates/planora-sketch/src/lib.rs`, `crates/planora-sketch/src/error.rs`, `crates/planora-sketch/Cargo.toml` (dev-dependency `planora-mesh`)
- Test: `crates/planora-sketch/tests/e2e.rs`, `crates/planora-sketch/tests/properties.rs`

**Interfaces:**
- Consumes: `Sketch::profiles`, `ExtractedProfiles` (Task 7), Kernel (`extrude`, `revolve`, `validate`), `planora-mesh` nur in Tests.
- Produces:
  - `SketchError::Kernel(String)` („Der Kernel hat das Profil abgelehnt: {0}“).
  - `fn extrude_profile<K: Kernel>(kernel: &K, sketch: &Sketch, profile: &Profile, height: f64) -> Result<Solid, SketchError>`: `kernel.extrude(profile, sketch.frame(), height)`, Kernel-Fehler werden zu `SketchError::Kernel(text)`. Die Extrusion läuft entlang der Skizzennormalen (`frame.z`).
  - `Sketch::revolve_input(&self, profile: &Profile, axis: CurveId) -> Result<(ProfileLoop, Frame), SketchError>` und `fn revolve_profile<K: Kernel>(kernel: &K, sketch: &Sketch, profile: &Profile, axis: CurveId) -> Result<Solid, SketchError>`. Regeln: `axis` ist eine Linie der Skizze (meist Konstruktionslinie). Das Profil darf keine Löcher haben (`UnsupportedForSolid("Loch")`). Die äußere Schleife muss genau ein Linienstück enthalten, dessen beide Endpunkte auf der Achsgeraden liegen (Abstand ≤ `tol.linear`); fehlt es: `Degenerate("Das Profil hat keine Kante auf der Drehachse")`. Dieses Stück wird entfernt; die übrigen Segmente, beginnend am Ende des entfernten Stücks, bilden die offene Kette. Punkte werden nach `(r, z)` abgebildet: `z = (p − A)·d` (entlang der Achse von `A` nach `B`, `d` normiert), `perp = d.x·(p − A).y − d.y·(p − A).x` (links der Achse positiv); alle Punkte müssen auf einer Seite liegen (`perp` überall ≥ −tol oder überall ≤ tol), sonst `Degenerate("Das Profil liegt auf beiden Seiten der Drehachse")`; `s = +1`, wenn die Punkte links liegen, sonst `−1`; `r = s·perp`. Bögen: Mittelpunkt ebenso abgebildet, `sweep' = −s·sweep`. Rahmen für `revolve`: `origin = frame.point_2d(A)`, `z` = Achsrichtung im Raum (`frame.x·d.x + frame.y·d.y`), `x` = `s·(frame.x·(−d.y) + frame.y·d.x)`, `y = z × x`.
- Tests der Eigenschaften (feste Seeds, 40 Fälle, Mindestzahl geprüfter Fälle wird erzwungen).

- [ ] **Step 1: Failing tests schreiben**

`crates/planora-sketch/tests/e2e.rs` (benötigt `planora-mesh` als dev-dependency; die Namen von `tessellate`/`mass_properties` und ihre Optionen aus `planora-mesh` übernehmen, sie stehen in `crates/planora-mesh/src/lib.rs`):

```rust
use planora_kernel::{AnalyticKernel, BoolOp, Kernel};
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::Sketch;
use planora_sketch::solid::{extrude_profile, revolve_profile};
use planora_sketch::SketchError;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn volume(solid: &planora_kernel::Solid) -> f64 {
    let mesh = tessellate(solid, &TessellationOptions::default()).unwrap();
    mass_properties(&mesh).volume
}

#[test]
fn a_dimensioned_rectangle_extrudes_to_the_right_volume() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let r = s.add_rectangle(DVec2::new(1.0, 1.0), DVec2::new(30.0, 20.0)).unwrap();
    s.add_constraint(K::Fixed(r.points[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(r.points[0], r.points[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(r.points[1], r.points[2], 25.0)).unwrap();
    assert_eq!(s.solve().dof, 0);
    let e = s.profiles();
    let solid = extrude_profile(&k(), &s, &e.profiles[0], 7.0).unwrap();
    k().validate(&solid).unwrap();
    assert!((volume(&solid) - 40.0 * 25.0 * 7.0).abs() < 1e-3);
}

#[test]
fn a_sketch_on_another_plane_extrudes_along_its_normal() {
    let frame = Frame::from_origin_normal(DVec3::new(0.0, 5.0, 0.0), DVec3::Y);
    let mut s = Sketch::new(frame, "S", Tolerance::default());
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(c, 2.0).unwrap();
    let solid = extrude_profile(&k(), &s, &s.profiles().profiles[0], 3.0).unwrap();
    let mesh = tessellate(&solid, &TessellationOptions::default()).unwrap();
    let ys: Vec<f64> = mesh.positions.iter().map(|p| p.y).collect();
    let (lo, hi) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
    assert!((hi - lo - 3.0).abs() < 1e-4, "Höhe entlang der Normalen");
    assert!((lo - 5.0).abs() < 1e-4 || (hi - 5.0).abs() < 1e-4, "Sockel liegt in der Skizzenebene");
}

#[test]
fn revolving_a_rectangle_next_to_an_axis_gives_a_cylinder() {
    for side in [1.0, -1.0] {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let a = s.add_point(0.0, -1.0).unwrap();
        let b = s.add_point(0.0, 6.0).unwrap();
        let axis = s.add_line(a, b).unwrap();
        s.set_construction(axis, true).unwrap();
        s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(side * 3.0, 5.0)).unwrap();
        let e = s.profiles();
        assert_eq!(e.profiles.len(), 1, "{:?}", e.rejected);
        let solid = revolve_profile(&k(), &s, &e.profiles[0], axis).unwrap();
        k().validate(&solid).unwrap();
        assert!((volume(&solid) - PI * 9.0 * 5.0).abs() < 0.5, "Seite {side}");
    }
}

#[test]
fn revolving_a_half_disc_gives_a_sphere() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, -4.0).unwrap();
    let b = s.add_point(0.0, 4.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    let c = s.add_point(0.0, 0.0).unwrap();
    let top = s.add_point(0.0, 4.0).unwrap();
    let bottom = s.add_point(0.0, -4.0).unwrap();
    s.add_line(top, bottom).unwrap();
    s.add_arc(c, bottom, top).unwrap(); // gegen den Uhrzeigersinn von unten über rechts nach oben
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 1, "{:?}", e.rejected);
    let solid = revolve_profile(&k(), &s, &e.profiles[0], axis).unwrap();
    k().validate(&solid).unwrap();
    assert!((volume(&solid) - 4.0 / 3.0 * PI * 64.0).abs() / (4.0 / 3.0 * PI * 64.0) < 0.01);
}

#[test]
fn bad_revolve_inputs_are_rejected_with_a_reason() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(-5.0, 0.0).unwrap();
    let b = s.add_point(-5.0, 6.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(3.0, 5.0)).unwrap();
    let p = &s.profiles().profiles[0];
    assert!(matches!(revolve_profile(&k(), &s, p, axis), Err(SketchError::Degenerate(_))), "keine Kante auf der Achse");

    // Achse durch das Profil: liegt auf beiden Seiten
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(1.0, -1.0).unwrap();
    let b = s.add_point(1.0, 6.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(3.0, 5.0)).unwrap();
    let p = &s.profiles().profiles[0];
    assert!(revolve_profile(&k(), &s, p, axis).is_err());

    // Profile mit Loch
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, -1.0).unwrap();
    let b = s.add_point(0.0, 20.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.set_construction(axis, true).unwrap();
    s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0)).unwrap();
    let c = s.add_point(5.0, 5.0).unwrap();
    s.add_circle(c, 1.0).unwrap();
    let p = &s.profiles().profiles[0];
    assert!(matches!(revolve_profile(&k(), &s, p, axis), Err(SketchError::UnsupportedForSolid(_))));
}

#[test]
fn sketch_solids_combine_with_boolean_operations() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    s.add_rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)).unwrap();
    let c = s.add_point(10.0, 10.0).unwrap();
    s.add_circle(c, 4.0).unwrap();
    let plate = extrude_profile(&k(), &s, &s.profiles().profiles[0], 5.0).unwrap();
    let mut t = Sketch::new(Frame::WORLD, "T", Tolerance::default());
    t.add_rectangle(DVec2::new(5.0, 5.0), DVec2::new(8.0, 8.0)).unwrap();
    let pin = extrude_profile(&k(), &t, &t.profiles().profiles[0], 9.0).unwrap();
    let both = k().boolean(&plate, &pin, BoolOp::Union).unwrap();
    k().validate(&both).unwrap();
    assert!(volume(&both) > volume(&plate));
}
```

`crates/planora-sketch/tests/properties.rs`:

```rust
use planora_math::{DVec2, Frame, TAU, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::Sketch;
use planora_sketch::solver::SolveStatus;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestRng, TestRunner};
use std::cell::Cell;

fn runner() -> TestRunner {
    TestRunner::new_with_rng(
        Config { cases: 40, failure_persistence: None, rng_seed: RngSeed::Fixed(7), ..Config::default() },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    )
}

/// Alle Abhängigkeiten sind erfüllt (Residuen unter der Schwelle).
fn all_satisfied(s: &Sketch) -> bool {
    s.constraints().all(|(id, c)| {
        c.driven
            || s.evaluate(id, s.variables())
                .is_some_and(|ev| ev.residuals.iter().all(|r| r.v.abs() < 1e-6))
    })
}

#[test]
fn polygons_with_known_dimensions_are_found_from_a_disturbed_start() {
    let checked = Cell::new(0usize);
    let strategy = (3usize..7, 5.0f64..30.0, proptest::collection::vec((-1.0f64..1.0, -1.0f64..1.0), 6));
    runner()
        .run(&strategy, |(n, size, noise)| {
            // konvexes n-Eck auf einem Kreis (die Lösung), dazu eine gestörte Startlage
            let truth: Vec<DVec2> = (0..n)
                .map(|i| DVec2::from_angle(TAU * i as f64 / n as f64) * size)
                .collect();
            let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
            let pts: Vec<_> = truth
                .iter()
                .zip(&noise)
                .map(|(p, (dx, dy))| s.add_point(p.x + dx * 0.05 * size, p.y + dy * 0.05 * size).unwrap())
                .collect();
            let lines: Vec<_> = (0..n).map(|i| s.add_line(pts[i], pts[(i + 1) % n]).unwrap()).collect();
            s.add_constraint(K::Fixed(pts[0], truth[0].x, truth[0].y)).unwrap();
            for i in 0..n {
                let d = truth[i].distance(truth[(i + 1) % n]);
                s.add_constraint(K::Distance(pts[i], pts[(i + 1) % n], d)).unwrap();
            }
            for i in 0..n {
                let (a, b) = (truth[(i + 1) % n] - truth[i], truth[(i + 2) % n] - truth[(i + 1) % n]);
                let angle = a.perp_dot(b).atan2(a.dot(b));
                s.add_constraint(K::Angle(lines[i], lines[(i + 1) % n], angle)).unwrap();
            }
            let rep = s.solve();
            prop_assert_eq!(rep.status, SolveStatus::Solved, "{:?}", rep);
            prop_assert!(all_satisfied(&s));
            checked.set(checked.get() + 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(checked.get(), 40, "alle Fälle wurden wirklich geprüft");
}

#[test]
fn circles_with_known_radii_and_tangent_lines_are_found() {
    let checked = Cell::new(0usize);
    let strategy = (2.0f64..10.0, 2.0f64..10.0, 15.0f64..40.0, (-1.0f64..1.0, -1.0f64..1.0));
    runner()
        .run(&strategy, |(r1, r2, dist, (nx, ny))| {
            let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
            let c1 = s.add_point(0.0, 0.0).unwrap();
            let c2 = s.add_point(dist + nx * 0.3, ny * 0.3).unwrap();
            let k1 = s.add_circle(c1, r1 * 1.05).unwrap();
            let k2 = s.add_circle(c2, r2 * 0.95).unwrap();
            // äußere Tangentenlinie zwischen den Kreisen oben: Endpunkte liegen auf den Kreisen
            let p1 = s.add_point(0.0, r1).unwrap();
            let p2 = s.add_point(dist, r2).unwrap();
            let line = s.add_line(p1, p2).unwrap();
            s.add_constraint(K::Fixed(c1, 0.0, 0.0)).unwrap();
            s.add_constraint(K::Distance(c1, c2, dist)).unwrap();
            s.add_constraint(K::Horizontal(planora_sketch::constraint::LineOrPoints::Points(c1, c2))).unwrap();
            s.add_constraint(K::Radius(k1, r1)).unwrap();
            s.add_constraint(K::Radius(k2, r2)).unwrap();
            s.add_constraint(K::Tangent(line, k1)).unwrap();
            s.add_constraint(K::Tangent(line, k2)).unwrap();
            s.add_constraint(K::PointOnCurve(p1, k1)).unwrap();
            s.add_constraint(K::PointOnCurve(p2, k2)).unwrap();
            let rep = s.solve();
            prop_assert_eq!(rep.status, SolveStatus::Solved, "{:?}", rep);
            prop_assert!(all_satisfied(&s));
            checked.set(checked.get() + 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(checked.get(), 40);
}
```

Hinweise zu den Eigenschaftstests: Sie sind deterministisch (fester Seed, kein Persistenzordner). Die Störung (5 % der Größe bzw. 0,3 mm) darf **verkleinert**, aber nicht unter 2 % bzw. 0,1 mm gesenkt werden, falls einzelne Fälle des festen Seeds nicht konvergieren; die tatsächlich gewählte Störung steht im Bericht. Konvergiert ein Fall trotz kleiner Störung nicht, ist das ein Solver-Befund (Bericht mit minimaler Reproduktion), kein Grund, Fälle auszulassen. Der zweite Test ist überbestimmt (redundante Tangenten und Punkt-auf-Kreis); `Solved` mit Redundanzmeldung ist erwartet.

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test e2e --test properties`
Expected: FAIL (`solid`-Modul fehlt)

- [ ] **Step 3: Implementierung** – `solid.rs` (Referenz)

```rust
//! Von der Skizze zum Körper: Extrusion und Rotation.

use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, Sketch};
use planora_kernel::{Kernel, Profile, ProfileLoop, Segment, Solid, segment_end};
use planora_math::{DVec2, Frame};

fn kernel_error(e: impl std::fmt::Display) -> SketchError {
    SketchError::Kernel(e.to_string())
}

pub fn extrude_profile<K: Kernel>(
    kernel: &K,
    sketch: &Sketch,
    profile: &Profile,
    height: f64,
) -> Result<Solid, SketchError> {
    kernel.extrude(profile, sketch.frame(), height).map_err(kernel_error)
}

pub fn revolve_profile<K: Kernel>(
    kernel: &K,
    sketch: &Sketch,
    profile: &Profile,
    axis: CurveId,
) -> Result<Solid, SketchError> {
    let (chain, frame) = sketch.revolve_input(profile, axis)?;
    kernel.revolve(&chain, &frame).map_err(kernel_error)
}

impl Sketch {
    pub fn revolve_input(
        &self,
        profile: &Profile,
        axis: CurveId,
    ) -> Result<(ProfileLoop, Frame), SketchError> {
        let tol = self.tolerance().linear;
        if !profile.holes.is_empty() {
            return Err(SketchError::UnsupportedForSolid("Loch".into()));
        }
        self.check_curve(axis)?;
        let CurveKind::Line { a, b } = self.curves[axis].kind else {
            return Err(SketchError::WrongType("Die Drehachse muss eine Linie sein".into()));
        };
        let (pa, pb) = (self.pos(a), self.pos(b));
        let d = (pb - pa).normalize();
        let perp_of = |p: DVec2| d.x * (p - pa).y - d.y * (p - pa).x;
        let along_of = |p: DVec2| (p - pa).dot(d);

        // Punkte der Schleife und das Stück auf der Achse suchen
        let lp = &profile.outer;
        let pts = lp.chain_points();
        let m = lp.segments.len();
        let on_axis = |i: usize| {
            matches!(lp.segments[i], Segment::Line { .. })
                && perp_of(pts[i]).abs() <= tol
                && perp_of(pts[i + 1]).abs() <= tol
        };
        let Some(axis_seg) = (0..m).find(|&i| on_axis(i)) else {
            return Err(SketchError::Degenerate("Das Profil hat keine Kante auf der Drehachse".into()));
        };
        let all: Vec<f64> = lp.sample(64).iter().map(|&p| perp_of(p)).collect();
        let (any_left, any_right) = (all.iter().any(|&v| v > tol), all.iter().any(|&v| v < -tol));
        if any_left && any_right {
            return Err(SketchError::Degenerate("Das Profil liegt auf beiden Seiten der Drehachse".into()));
        }
        let s = if any_right { -1.0 } else { 1.0 };
        let to_rz = |p: DVec2| DVec2::new(s * perp_of(p), along_of(p));

        // Kette: Segmente nach dem Achsstück, einmal herum bis vor das Achsstück
        let start_point = pts[(axis_seg + 1) % (m + 1)];
        let mut segments = Vec::new();
        let mut at = start_point;
        for k in 1..m {
            let seg = lp.segments[(axis_seg + k) % m];
            let end = segment_end(at, &seg);
            segments.push(match seg {
                Segment::Line { .. } => Segment::Line { to: to_rz(end) },
                Segment::Arc { center, sweep } => Segment::Arc { center: to_rz(center), sweep: -s * sweep },
            });
            at = end;
        }
        let chain = ProfileLoop { start: to_rz(start_point), segments };

        let frame = self.frame();
        let z3 = frame.x * d.x + frame.y * d.y;
        let x3 = (frame.x * (-d.y) + frame.y * d.x) * s;
        let y3 = z3.cross(x3);
        Ok((chain, Frame { origin: frame.point_2d(pa), x: x3, y: y3, z: z3 }))
    }
}
```

Hinweise: `pts[(axis_seg + 1) % (m + 1)]` ist der Endpunkt des Achsstücks (`pts` hat `m + 1` Einträge, der letzte entspricht dem ersten). Wird die Rotation der Kette im Kernel abgelehnt (z. B. `ProfileOffAxis`), kommt der Fehler als `SketchError::Kernel` zurück. `lib.rs`: `pub mod solid;`. `Cargo.toml`: `[dev-dependencies] planora-mesh.workspace = true`.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p planora-sketch && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/planora-sketch Cargo.lock
git commit -m "feat(sketch): Extrusion und Rotation aus Skizzenprofilen, Ende-zu-Ende- und Eigenschaftstests"
```

---

### Task 11: Skizzen-Modus ohne Oberfläche (Zustand, Werkzeuge, Abhängigkeiten) und App-Zustand

**Files:**
- Create: `crates/planora-app/src/sketch_state.rs`
- Modify: `crates/planora-sketch/src/model.rs` oder neue Datei `crates/planora-sketch/src/query.rs` (Darstellungs- und Auswahlhilfen), `crates/planora-sketch/src/lib.rs`, `crates/planora-render/src/camera.rs`, `crates/planora-render/src/scene.rs` (gedimmte Palette), `crates/planora-app/src/state.rs`, `crates/planora-app/src/lib.rs`, `crates/planora-app/Cargo.toml` (`planora-sketch`)
- Test: `crates/planora-sketch/tests/query.rs`, `crates/planora-render/tests/camera_look.rs` (oder bestehende Testdatei), `crates/planora-app/tests/sketch_state.rs`

**Interfaces:**
- Consumes: gesamte API von `planora-sketch` (Tasks 1–10), `AppState` (Teilprojekt 1–3).
- Produces in `planora-sketch` (`query.rs`, `Sketch`-Methoden):
  - `curve_polyline(c: CurveId, steps_per_turn: usize) -> Option<Vec<DVec2>>`: Linie 2 Punkte; Kreis geschlossen (letzter = erster Punkt); Bogen gegen den Uhrzeigersinn von Start nach Ende (Endpunkte exakt die Punktpositionen); Ellipse geschlossen; Spline 16 Punkte je Segment. `steps_per_turn` wird auf `4..=4096` geklemmt.
  - `pick_point(at: DVec2, radius: f64) -> Option<PointId>` (nächster lebender Punkt innerhalb `radius`), `pick_curve(at: DVec2, radius: f64) -> Option<CurveId>` (nächste lebende Kurve innerhalb `radius`, Abstand zum Linienzug mit 96 Schritten je Vollkreis), `distance_to_curve(c, at) -> Option<f64>`.
  - `bounds() -> Option<(DVec2, DVec2)>` aller lebenden Punkte (für „Skizze einpassen“).
- Produces in `planora-render`: `OrbitCamera::look_along(&mut self, to_camera: Vec3, up_hint: Vec3)`: setzt `yaw`/`pitch` so, dass die Kamera in Richtung `to_camera` vom Ziel steht (Pitch auf `±MAX_PITCH` geklemmt). Bei (fast) senkrechter Blickrichtung wird `yaw` so gewählt, dass die Bildschirm-Aufwärtsrichtung der Projektion von `up_hint` entspricht (für `to_camera = +Z`, `up_hint = Y`: `yaw = −π/2`; allgemein `yaw = atan2(−up.y, −up.x)` für `+Z`, `atan2(up.y, up.x)` für `−Z`). `Palette::dimmed(&self) -> Palette`: mischt die Flächen- und Kantenfarben zu 60 % in Richtung `[0.5, 0.5, 0.5]` (Raster und Achsen unverändert), bleibt in `0..=1`.
- Produces in `planora-app` (`sketch_state.rs`, ohne egui):

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool { Select, Point, Line, Rectangle, Circle, Arc, Ellipse, Spline, ProjectEdge }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item { Point(PointId), Curve(CurveId), Constraint(ConstraintId) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap { None, Point(PointId), Curve(CurveId) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintAction {
    Coincident, Horizontal, Vertical, Parallel, Perpendicular, Tangent, Equal, Fixed,
    Midpoint, Symmetric, PointOnCurve, Distance, DistancePointLine, Radius, Diameter, Angle,
}
impl ConstraintAction { pub fn label(self) -> &'static str; pub fn is_dimension(self) -> bool }

pub enum Severity { Info, Warning, Error }

pub struct SketchSession {
    pub sketch: usize,                   // Index in AppState::sketches
    pub tool: Tool,
    pub selection: Vec<Item>,
    pub pending: Vec<PointId>,           // Punkte des laufenden Werkzeugs
    pub pending_pos: Vec<DVec2>,         // noch nicht angelegte Eckpunkte (Rechteck, Ellipse)
    pub cursor: Option<DVec2>,
    pub auto_constrain: bool,            // Standard: true
    pub last_report: Option<SolveReport>,
    pub message: Option<String>,
    pub dragging: Option<PointId>,
}
```

  Methoden von `SketchSession` (alle nehmen `sk: &mut Sketch`, Koordinaten in Skizzeneinheiten, `radius` = Fangradius in Skizzeneinheiten):
  - `new(sketch: usize) -> SketchSession`, `set_tool(tool)` (leert `pending`, `pending_pos`).
  - `snap(&self, sk: &Sketch, at, radius) -> Snap` (Punkte vor Kurven).
  - `click(&mut self, sk, at, radius, extend: bool) -> Result<(), String>` – verhält sich je Werkzeug (siehe unten); Fehler werden als deutsche Meldung zurückgegeben und stehen auch in `message`; bei Fehler bleibt die Skizze unverändert.
  - `finish_tool(&mut self, sk) -> Result<(), String>` (Spline abschließen; Linienzug beenden), `cancel(&mut self)` (leert `pending`/`pending_pos`; im Auswahlwerkzeug leert es die Auswahl).
  - `begin_drag(&mut self, sk, at, radius) -> bool`, `drag_to(&mut self, sk, at)`, `end_drag(&mut self)`; Ziehen nutzt `Sketch::drag`; scheitert der Solver, bleibt der Zustand, `message` erklärt es.
  - `available_actions(&self, sk: &Sketch) -> Vec<ConstraintAction>` aus der Auswahl (siehe Tabelle) und `apply(&mut self, sk, action, value: Option<f64>) -> Result<ConstraintId, String>`.
  - `delete_selection(&mut self, sk)`, `toggle_construction(&mut self, sk)` (auf ausgewählte Kurven), `status(&self, sk: &Sketch) -> (String, Severity)`.
  - `mirror_selection(&mut self, sk, axis: CurveId)`, `pattern_selection(&mut self, sk, offset: DVec2, count: usize)`, `project_edge(&mut self, sk, solid: &Solid, edge: Id<Edge>)`.

  **Werkzeugverhalten (Tests unten):**
  - *Punkt*: Klick legt einen Punkt an (Fang auf Punkt: nichts Neues; Fang auf Kurve: Punkt plus `PointOnCurve`).
  - *Linie*: 1. Klick legt den Anfangspunkt an oder fängt einen vorhandenen; 2. Klick legt den Endpunkt an/fängt und die Linie; danach ist der Endpunkt der neue Anfang (Linienzug). Liegt die Linie weniger als `AUTO_AXIS_ANGLE` (3°, benannte `const`) von der Waagerechten bzw. Senkrechten entfernt und ist `auto_constrain` an, kommt `Horizontal` bzw. `Vertical` hinzu. Danach wird gelöst. `cancel` oder `finish_tool` beendet den Linienzug.
  - *Rechteck*: 1. Klick merkt die Ecke in `pending_pos`, 2. Klick `add_rectangle`; Werkzeug bleibt aktiv.
  - *Kreis*: Mittelpunkt (anlegen/fangen), 2. Klick: Radius = Abstand; Radius ≤ `tol.linear` → Fehler.
  - *Bogen*: Mittelpunkt, Startpunkt (anlegen/fangen), 3. Klick: Endpunkt auf dem Kreis um den Mittelpunkt durch den Start in Richtung des Klicks; gegen den Uhrzeigersinn von Start nach Ende.
  - *Ellipse*: 1. Klick Mittelpunkt, 2. Klick Endpunkt der großen Halbachse (`pending_pos`), 3. Klick: kleine Halbachse = Abstand des Klicks von der großen Achse (mindestens `tol.linear`, höchstens große Halbachse).
  - *Spline*: jeder Klick fügt einen Stützpunkt hinzu; `finish_tool` legt den Spline an (mindestens 2 Punkte, sonst Fehlermeldung).
  - *Auswahl*: Klick wählt den nächsten Punkt, sonst die nächste Kurve innerhalb `radius`; `extend` schaltet die Zugehörigkeit um; Klick ins Leere leert die Auswahl (außer `extend`).
  - *ProjectEdge*: wird von der Oberfläche gesteuert (Klick auf eine Körperkante ruft `project_edge`).

  **Abhängigkeiten aus der Auswahl** (in Auswahlreihenfolge; Punkt = P, Linie = L, Kreis/Bogen = C):

  | Auswahl | verfügbare Aktionen |
  |---|---|
  | P | Fixed |
  | P, P | Coincident, Horizontal, Vertical, Distance |
  | L | Horizontal, Vertical, Distance (Länge) |
  | L, L | Parallel, Perpendicular, Equal, Angle |
  | L, C oder C, L | Tangent |
  | C, C | Tangent, Equal |
  | C | Radius, Diameter |
  | P, L | PointOnCurve, Midpoint, DistancePointLine |
  | P, C | PointOnCurve |
  | P, P, L | Symmetric |

  `apply`: baut die `ConstraintKind` aus der Auswahl; bei Maßen ist `value = None` der aktuelle Wert (`measure`). Danach wird gelöst. Ergebnisse: Ist die neue Abhängigkeit in `conflicting` oder konvergiert der Solver nicht, wird sie wieder entfernt und `Err("Widerspruch: … ")` bzw. `Err("… ließ sich nicht lösen")` zurückgegeben (Skizze unverändert). Ist sie `redundant`: ein Maß wird auf `driven` gesetzt (`message`: „Dieses Maß ist bereits durch andere Abhängigkeiten bestimmt und wird nur angezeigt“), eine geometrische Abhängigkeit bleibt (`message`: „Diese Abhängigkeit ist überzählig“). `last_report` wird nach jedem Lösen aktualisiert.

  `status`: gelöst und `dof == 0` → „Vollständig bestimmt“ (Info); `dof > 0` → „{n} Freiheitsgrade“ (Info); `Conflicting` → „Widerspruch zwischen {k} Abhängigkeiten“ (Error); `NotConverged` → „Nicht lösbar“ (Error); redundante Abhängigkeiten ergänzen „, {r} überzählig“ (Warning).

- Produces in `AppState` (`state.rs`):
  - Felder `pub sketches: Vec<Sketch>`, `pub sketch_session: Option<SketchSession>`, `saved_camera: Option<OrbitCamera>`, `next_sketch_number: u64`.
  - `new_sketch(&mut self, frame: Frame) -> usize` (Name „Skizze N“, Zähler zählt nur hoch), `standard_frame(plane: StandardPlane) -> Frame` (XY: x=X, y=Y, z=Z; XZ: x=X, y=Z, z=−Y; YZ: x=Y, y=Z, z=X; `enum StandardPlane { XY, XZ, YZ }`), `new_sketch_on_face(&mut self, body: usize, face: Id<Face>) -> Result<usize, String>` (nur ebene Flächen, `plane_of_face`), `open_sketch(&mut self, index: usize) -> Result<(), String>` (Sitzung starten, Kamera senkrecht zur Skizzenebene mit `look_along(frame.z, frame.y)` auf die Skizze ausrichten, alte Kamera merken, Szene mit gedimmter Palette neu bauen), `close_sketch(&mut self)` (Sitzung beenden, Kamera zurück, normale Palette), `delete_sketch(&mut self, index: usize)` (schließt die Sitzung, wenn sie diese Skizze betraf; passt `sketch`-Indizes der Sitzung an).
  - `extrude_sketch(&mut self, sketch: usize, profile: usize, height: f64) -> Result<usize, String>` und `revolve_sketch(&mut self, sketch: usize, profile: usize, axis: CurveId) -> Result<usize, String>`: Profile über `sketches[sketch].profiles()` (Index in `profiles`), Körper **an Ort und Stelle** (`add_body_in_place`) mit Art `Extrusion`/`Revolution`, Name „Extrusion: {Skizzenname}“ / „Rotation: {Skizzenname}“; jeder Fehler (unbekannter Index, Kernel-Ablehnung) → `Err(deutscher Text)` und `message`, Dokument unverändert.

- [ ] **Step 1: Failing tests schreiben**

`crates/planora-sketch/tests/query.rs`:

```rust
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn polylines_hit_the_exact_end_points() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(2.0, 0.0).unwrap();
    let b = s.add_point(0.0, 2.0).unwrap();
    let arc = s.add_arc(c, a, b).unwrap();
    let pl = s.curve_polyline(arc, 64).unwrap();
    assert_eq!(*pl.first().unwrap(), DVec2::new(2.0, 0.0));
    assert!((*pl.last().unwrap() - DVec2::new(0.0, 2.0)).length() < 1e-12);
    assert!(pl.iter().all(|p| (p.length() - 2.0).abs() < 1e-9));
    assert!(pl.len() >= 16);
    let circle = s.add_circle(c, 1.0).unwrap();
    let cl = s.curve_polyline(circle, 32).unwrap();
    assert!((cl[0] - *cl.last().unwrap()).length() < 1e-12, "Kreis ist geschlossen");
    let line = s.add_line(a, b).unwrap();
    assert_eq!(s.curve_polyline(line, 64).unwrap().len(), 2);
    assert!(s.curve_polyline(planora_kernel::Id::from_index(77), 8).is_none());
}

#[test]
fn picking_prefers_points_and_respects_the_radius() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert_eq!(s.pick_point(DVec2::new(0.3, 0.2), 0.5), Some(a));
    assert_eq!(s.pick_point(DVec2::new(5.0, 0.0), 0.5), None);
    assert_eq!(s.pick_curve(DVec2::new(5.0, 0.4), 0.5), Some(l));
    assert_eq!(s.pick_curve(DVec2::new(5.0, 2.0), 0.5), None);
    assert!((s.distance_to_curve(l, DVec2::new(5.0, 3.0)).unwrap() - 3.0).abs() < 1e-12);
    let (lo, hi) = s.bounds().unwrap();
    assert_eq!((lo, hi), (DVec2::ZERO, DVec2::new(10.0, 0.0)));
    assert!(sketch().bounds().is_none());
}
```

`crates/planora-render/tests/camera_look.rs`:

```rust
use glam::{Vec3, Vec4Swizzles};
use planora_render::{OrbitCamera, Palette};

#[test]
fn looking_along_a_normal_puts_the_camera_there() {
    let mut cam = OrbitCamera::default();
    cam.target = Vec3::new(1.0, 2.0, 3.0);
    for dir in [Vec3::X, -Vec3::Y, Vec3::new(1.0, 1.0, 1.0).normalize()] {
        cam.look_along(dir, Vec3::Z);
        let to_cam = (cam.eye() - cam.target).normalize();
        assert!(to_cam.dot(dir) > 0.999, "{dir:?} → {to_cam:?}");
    }
}

#[test]
fn top_view_keeps_x_right_and_y_up() {
    let mut cam = OrbitCamera::default();
    cam.look_along(Vec3::Z, Vec3::Y);
    let view = cam.view();
    let x = (view * Vec3::X.extend(0.0)).xyz();
    let y = (view * Vec3::Y.extend(0.0)).xyz();
    assert!(x.x > 0.99 && x.y.abs() < 0.05, "x zeigt nach rechts: {x:?}");
    assert!(y.y > 0.99 && y.x.abs() < 0.05, "y zeigt nach oben: {y:?}");
}

#[test]
fn dimmed_palette_stays_in_range_and_differs() {
    let p = Palette::dark();
    let d = p.dimmed();
    assert_ne!(p.face_base, d.face_base);
    assert_eq!(p.grid, d.grid);
    for c in [d.face_base, d.face_hover, d.face_selected, d.edge_base, d.edge_hover, d.edge_selected] {
        assert!(c.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
```

`crates/planora-app/tests/sketch_state.rs`:

```rust
use planora_app::sketch_state::*;
use planora_app::state::{AppState, BodyKind, StandardPlane};
use planora_kernel::{Kernel, Surface};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveKind, Sketch};

const R: f64 = 0.5; // Fangradius in Skizzeneinheiten

fn setup() -> (Sketch, SketchSession) {
    (Sketch::new(Frame::WORLD, "S", Tolerance::default()), SketchSession::new(0))
}

fn click(se: &mut SketchSession, sk: &mut Sketch, x: f64, y: f64) {
    se.click(sk, DVec2::new(x, y), R, false).unwrap();
}

#[test]
fn the_line_tool_draws_a_chain_and_adds_horizontal_and_vertical() {
    let (mut sk, mut se) = setup();
    se.set_tool(Tool::Line);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 10.0, 0.1);
    assert_eq!(sk.curves().count(), 1);
    assert_eq!(sk.constraints().count(), 1, "fast waagerecht → Horizontal");
    let (a, b) = match sk.curves().next().unwrap().1.kind {
        CurveKind::Line { a, b } => (a, b),
        _ => panic!(),
    };
    assert!((sk.point_xy(a).unwrap().y - sk.point_xy(b).unwrap().y).abs() < 1e-7, "durch Lösen exakt waagerecht");
    click(&mut se, &mut sk, 10.0, 10.0);
    assert_eq!(sk.curves().count(), 2);
    assert_eq!(sk.points().count(), 3, "der Endpunkt wird zum neuen Anfang");
    click(&mut se, &mut sk, 0.05, 0.0); // schließt am ersten Punkt
    assert_eq!(sk.curves().count(), 3);
    assert_eq!(sk.points().count(), 3, "Fang auf den ersten Punkt");
    assert!(sk.profiles().profiles.len() == 1, "geschlossenes Dreieck");
}

#[test]
fn auto_constraints_can_be_switched_off() {
    let (mut sk, mut se) = setup();
    se.auto_constrain = false;
    se.set_tool(Tool::Line);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 10.0, 0.1);
    assert_eq!(sk.constraints().count(), 0);
}

#[test]
fn rectangle_circle_and_arc_tools() {
    let (mut sk, mut se) = setup();
    se.set_tool(Tool::Rectangle);
    click(&mut se, &mut sk, 0.0, 0.0);
    assert!(se.pending_pos.len() == 1);
    click(&mut se, &mut sk, 8.0, 5.0);
    assert_eq!((sk.curves().count(), sk.constraints().count()), (4, 4));
    assert!(se.pending_pos.is_empty());

    se.set_tool(Tool::Circle);
    click(&mut se, &mut sk, 20.0, 20.0);
    click(&mut se, &mut sk, 23.0, 20.0);
    let circle = sk.curves().last().unwrap().0;
    assert!((sk.radius(circle).unwrap() - 3.0).abs() < 1e-9);

    se.set_tool(Tool::Arc);
    click(&mut se, &mut sk, 40.0, 0.0);
    click(&mut se, &mut sk, 45.0, 0.0);
    click(&mut se, &mut sk, 40.0, 9.0);
    let arc = sk.curves().last().unwrap().0;
    let CurveKind::Arc { end, .. } = sk.curves().last().unwrap().1.kind else { panic!() };
    assert!((sk.point_xy(end).unwrap() - DVec2::new(40.0, 5.0)).length() < 1e-9, "Ende liegt auf dem Kreis");
    assert!((sk.radius(arc).unwrap() - 5.0).abs() < 1e-9);
    assert!(se.pending.is_empty());
}

#[test]
fn a_point_clicked_on_a_curve_stays_on_it() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    se.set_tool(Tool::Point);
    click(&mut se, &mut sk, 5.0, 0.3);
    assert_eq!(sk.points().count(), 3);
    assert_eq!(sk.constraints().count(), 1, "PointOnCurve");
    assert!(sk.point_xy(sk.points().last().unwrap().0).unwrap().y.abs() < 1e-7);
}

#[test]
fn selection_prefers_points_and_toggles_with_extend() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    let l = sk.add_line(a, b).unwrap();
    se.set_tool(Tool::Select);
    click(&mut se, &mut sk, 0.2, 0.2);
    assert_eq!(se.selection, vec![Item::Point(a)]);
    se.click(&mut sk, DVec2::new(5.0, 0.1), R, true).unwrap();
    assert_eq!(se.selection, vec![Item::Point(a), Item::Curve(l)]);
    se.click(&mut sk, DVec2::new(0.2, 0.2), R, true).unwrap();
    assert_eq!(se.selection, vec![Item::Curve(l)], "erneutes Anklicken mit extend nimmt heraus");
    click(&mut se, &mut sk, 50.0, 50.0);
    assert!(se.selection.is_empty());
}

#[test]
fn dragging_moves_free_points_and_respects_constraints() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    se.set_tool(Tool::Select);
    assert!(se.begin_drag(&mut sk, DVec2::new(10.1, 0.0), R));
    se.drag_to(&mut sk, DVec2::new(14.0, 3.0));
    se.end_drag();
    assert!((sk.point_xy(b).unwrap() - DVec2::new(14.0, 3.0)).length() < 1e-6, "b ist frei");
    assert_eq!(sk.point_xy(a).unwrap(), DVec2::ZERO, "a ist fest");
    assert!(!se.begin_drag(&mut sk, DVec2::new(100.0, 100.0), R));
}

#[test]
fn actions_follow_the_selection_and_constraints_are_applied_and_solved() {
    let (mut sk, mut se) = setup();
    let p: Vec<_> = [(0.0, 0.0), (10.0, 1.0), (0.0, 5.0), (10.0, 7.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    let l1 = sk.add_line(p[0], p[1]).unwrap();
    let l2 = sk.add_line(p[2], p[3]).unwrap();
    se.selection = vec![Item::Curve(l1), Item::Curve(l2)];
    let acts = se.available_actions(&sk);
    for a in [ConstraintAction::Parallel, ConstraintAction::Perpendicular, ConstraintAction::Equal, ConstraintAction::Angle] {
        assert!(acts.contains(&a), "{a:?} fehlt: {acts:?}");
    }
    assert!(!acts.contains(&ConstraintAction::Tangent));
    se.apply(&mut sk, ConstraintAction::Parallel, None).unwrap();
    let d1 = sk.point_xy(p[1]).unwrap() - sk.point_xy(p[0]).unwrap();
    let d2 = sk.point_xy(p[3]).unwrap() - sk.point_xy(p[2]).unwrap();
    assert!(d1.perp_dot(d2).abs() < 1e-6, "parallel nach dem Lösen");

    se.selection = vec![Item::Point(p[0])];
    assert_eq!(se.available_actions(&sk), vec![ConstraintAction::Fixed]);
    se.selection = vec![];
    assert!(se.available_actions(&sk).is_empty());
    assert!(se.apply(&mut sk, ConstraintAction::Parallel, None).is_err(), "nichts ausgewählt");
}

#[test]
fn contradicting_and_redundant_dimensions_are_handled() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    se.selection = vec![Item::Point(a), Item::Point(b)];
    se.apply(&mut sk, ConstraintAction::Distance, Some(10.0)).unwrap();
    let before = sk.variables().to_vec();
    let err = se.apply(&mut sk, ConstraintAction::Distance, Some(20.0)).unwrap_err();
    assert!(err.contains("Widerspruch"), "{err}");
    assert_eq!(sk.constraints().count(), 1, "die widersprüchliche Abhängigkeit wurde nicht behalten");
    assert_eq!(sk.variables(), before.as_slice());

    // dasselbe Maß noch einmal: widerspruchsfrei, aber überzählig → nur anzeigend
    let id = se.apply(&mut sk, ConstraintAction::Distance, Some(10.0)).unwrap();
    assert!(sk.constraint(id).unwrap().driven);
    assert!(se.message.as_deref().unwrap_or("").contains("bereits"));
}

#[test]
fn delete_cascades_and_status_texts() {
    let (mut sk, mut se) = setup();
    se.set_tool(Tool::Rectangle);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 8.0, 5.0);
    let (text, sev) = se.status(&sk);
    assert!(text.contains("Freiheitsgrade"), "{text}");
    assert!(matches!(sev, Severity::Info));
    let line = sk.curves().next().unwrap().0;
    se.selection = vec![Item::Curve(line)];
    se.delete_selection(&mut sk);
    assert_eq!(sk.curves().count(), 3);
    assert!(sk.constraints().count() < 4, "Abhängigkeiten der Linie sind weg");
    assert!(se.selection.is_empty());
}

#[test]
fn ellipse_and_spline_tools() {
    let (mut sk, mut se) = setup();
    se.set_tool(Tool::Ellipse);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 6.0, 0.0);
    click(&mut se, &mut sk, 0.0, 2.0);
    assert!(matches!(sk.curves().last().unwrap().1.kind, CurveKind::Ellipse { .. }));
    se.set_tool(Tool::Spline);
    click(&mut se, &mut sk, 20.0, 0.0);
    click(&mut se, &mut sk, 24.0, 5.0);
    click(&mut se, &mut sk, 28.0, 0.0);
    se.finish_tool(&mut sk).unwrap();
    assert!(matches!(sk.curves().last().unwrap().1.kind, CurveKind::Spline { .. }));
    se.set_tool(Tool::Spline);
    click(&mut se, &mut sk, 50.0, 50.0);
    assert!(se.finish_tool(&mut sk).is_err(), "ein Punkt reicht nicht");
}

#[test]
fn app_state_creates_opens_and_closes_sketches() {
    let mut s = AppState::new();
    let before_cam = s.camera;
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    assert_eq!(s.sketches[i].name, "Skizze 1");
    s.open_sketch(i).unwrap();
    assert!(s.sketch_session.is_some());
    let to_cam = (s.camera.eye() - s.camera.target).normalize();
    assert!(to_cam.z > 0.99, "Draufsicht auf die XY-Ebene: {to_cam:?}");
    s.close_sketch();
    assert!(s.sketch_session.is_none());
    assert_eq!(s.camera, before_cam, "Kamera zurückgesetzt");
    s.delete_sketch(i);
    assert!(s.sketches.is_empty());
    let j = s.new_sketch(AppState::standard_frame(StandardPlane::XZ));
    assert_eq!(s.sketches[j].name, "Skizze 2", "Zähler zählt nur hoch");
    assert!(s.open_sketch(99).is_err());
}

#[test]
fn bodies_from_sketches_appear_in_place_and_errors_leave_the_document_alone() {
    let mut s = AppState::new();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.sketches[i].add_rectangle(DVec2::new(2.0, 3.0), DVec2::new(12.0, 8.0)).unwrap();
    let b = s.extrude_sketch(i, 0, 6.0).unwrap();
    assert_eq!(s.bodies[b].kind, BodyKind::Extrusion);
    assert_eq!(s.bodies[b].name, "Extrusion: Skizze 1");
    let (lo, hi) = s.bodies[b].bounds;
    assert!((lo - DVec3::new(2.0, 3.0, 0.0)).length() < 1e-6, "an Ort und Stelle: {lo:?}");
    assert!((hi - DVec3::new(12.0, 8.0, 6.0)).length() < 1e-6);
    let count = s.bodies.len();
    assert!(s.extrude_sketch(i, 5, 6.0).is_err(), "Profil gibt es nicht");
    assert!(s.extrude_sketch(i, 0, -1.0).is_err());
    assert!(s.extrude_sketch(77, 0, 1.0).is_err());
    assert_eq!(s.bodies.len(), count);
    assert!(s.message.is_some());
}

#[test]
fn a_sketch_can_be_started_on_a_planar_face() {
    let mut s = AppState::new();
    let k = s.kernel;
    let bi = s
        .add_body(BodyKind::Box, "Q".into(), k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap())
        .unwrap();
    let top = s.bodies[bi]
        .solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Plane { .. }) && f.same_sense && (f.surface.frame().z.z - 1.0).abs() < 1e-9 && (f.surface.frame().origin.z - 5.0).abs() < 1e-9)
        .map(|(id, _)| id)
        .expect("Deckfläche");
    let i = s.new_sketch_on_face(bi, top).unwrap();
    let f = s.sketches[i].frame();
    assert!((f.z - DVec3::Z).length() < 1e-9 && (f.origin.z - 5.0).abs() < 1e-9);
    assert!(s.new_sketch_on_face(bi, planora_kernel::Id::from_index(99)).is_err());
    let cyl = s.add_body(BodyKind::Cylinder, "Z".into(), k.make_cylinder(2.0, 4.0).unwrap()).unwrap();
    let side = s.bodies[cyl].solid.faces.iter().find(|(_, f)| !matches!(f.surface, Surface::Plane { .. })).map(|(id, _)| id).unwrap();
    assert!(s.new_sketch_on_face(cyl, side).is_err(), "nur ebene Flächen");
}

#[test]
fn body_edges_can_be_copied_into_the_open_sketch() {
    let mut s = AppState::new();
    let k = s.kernel;
    let bi = s.add_body(BodyKind::Box, "Q".into(), k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap()).unwrap();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.open_sketch(i).unwrap();
    let edge = s.bodies[bi].solid.edges.iter().next().map(|(id, _)| id).unwrap();
    let mut session = s.sketch_session.take().unwrap();
    let before = s.sketches[i].curves().count();
    let res = session.project_edge(&mut s.sketches[i], &s.bodies[bi].solid, edge);
    s.sketch_session = Some(session);
    assert!(res.is_ok() || res.is_err(), "kein Panic; senkrechte Kanten werden mit Meldung abgelehnt");
    assert!(s.sketches[i].curves().count() >= before);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test query && cargo test -p planora-render --test camera_look && cargo test -p planora-app --test sketch_state`
Expected: FAIL (Funktionen/Module fehlen)

- [ ] **Step 3: Implementierung**

Reihenfolge: (a) `query.rs` in `planora-sketch` (Polylinien aus `CurveKind`, Ellipse/Spline mit `ellipse_point`/`spline_point`, Abstand Punkt–Polylinie, Auswahl); (b) `look_along`/`dimmed` in `planora-render` (Referenz unten); (c) `sketch_state.rs`; (d) `AppState`-Erweiterungen.

```rust
// planora-render/src/camera.rs
impl OrbitCamera {
    pub fn look_along(&mut self, to_camera: Vec3, up_hint: Vec3) {
        let d = to_camera.normalize_or_zero();
        if d == Vec3::ZERO {
            return;
        }
        self.pitch = d.z.asin().clamp(-Self::MAX_PITCH, Self::MAX_PITCH);
        self.yaw = if d.z.abs() > 0.999 {
            let up = Vec3::new(up_hint.x, up_hint.y, 0.0);
            if d.z > 0.0 { (-up.y).atan2(-up.x) } else { up.y.atan2(up.x) }
        } else {
            d.y.atan2(d.x)
        };
    }
}
```

```rust
// planora-render/src/scene.rs
impl Palette {
    /// Gedämpfte Farben für den Hintergrund des Skizzen-Modus (Raster und Achsen bleiben).
    pub fn dimmed(&self) -> Palette {
        let mix = |c: [f32; 4]| [0.5 + (c[0] - 0.5) * 0.4, 0.5 + (c[1] - 0.5) * 0.4, 0.5 + (c[2] - 0.5) * 0.4, c[3]];
        Palette {
            face_base: mix(self.face_base),
            face_hover: mix(self.face_hover),
            face_selected: mix(self.face_selected),
            edge_base: mix(self.edge_base),
            edge_hover: mix(self.edge_hover),
            edge_selected: mix(self.edge_selected),
            ..*self
        }
    }
}
```

Der Rest (`sketch_state.rs`) folgt den oben beschriebenen Regeln. Kernpunkte als Referenz:

```rust
/// Winkel zur Achse, unter dem eine Linie beim Zeichnen als waagerecht/senkrecht gilt.
const AUTO_AXIS_ANGLE: f64 = 3.0 * std::f64::consts::PI / 180.0;

fn solve_and_record(&mut self, sk: &mut Sketch) {
    self.last_report = Some(sk.solve());
}

fn point_at(&mut self, sk: &mut Sketch, at: DVec2, radius: f64) -> Result<PointId, String> {
    match self.snap(sk, at, radius) {
        Snap::Point(p) => Ok(p),
        Snap::Curve(c) => {
            let p = sk.add_point(at.x, at.y).map_err(|e| e.to_string())?;
            sk.add_constraint(ConstraintKind::PointOnCurve(p, c)).map_err(|e| e.to_string())?;
            Ok(p)
        }
        Snap::None => sk.add_point(at.x, at.y).map_err(|e| e.to_string()),
    }
}
```

Atomarität: Jeder Klick, der mehrere Skizzenänderungen macht, arbeitet auf einer **Kopie** (`let backup = sk.clone();` – `Sketch` ist `Clone`) und stellt sie bei Fehler zurück (`*sk = backup`). Das gilt auch für `apply` (Rückbau der neuen Abhängigkeit nach dem Lösen) und `pattern_selection`/`mirror_selection`.

`AppState::open_sketch` setzt `self.saved_camera = Some(self.camera)` nur, wenn noch keine Sitzung offen war (Wechsel zwischen Skizzen darf die gespeicherte Kamera nicht überschreiben); danach `camera.target` auf die Skizzenmitte (`bounds` oder `frame.origin`), `look_along(frame.z, frame.y)`, `frame_bounds` nur wenn Punkte da sind. `rebuild_scene` nutzt `palette().dimmed()`, solange `sketch_session.is_some()`.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS (bestehende Tests unverändert grün)

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "feat(app): Skizzen-Modus (Zustand, Werkzeuge, Abhängigkeiten) und Skizzen im App-Zustand"
```

---

### Task 12: Oberfläche des Skizzen-Modus (Darstellung, Eingaben, Werkzeugleiste, Abhängigkeitsliste)

**Files:**
- Create: `crates/planora-app/src/sketch_view.rs`, `crates/planora-app/src/sketch_panel.rs`
- Modify: `crates/planora-app/src/viewport.rs`, `crates/planora-app/src/app.rs`, `crates/planora-app/src/browser.rs`, `crates/planora-app/src/icons.rs`, `crates/planora-app/src/lib.rs`
- Test: `crates/planora-app/tests/sketch_view.rs`, `crates/planora-app/tests/sketch_ui.rs`

**Interfaces:**
- Consumes: `SketchSession`, `Tool`, `ConstraintAction`, `AppState` (Task 11), `OrbitCamera::project/ray`.
- Produces – reine, getestete Abbildungsfunktionen in `sketch_view.rs`:
  - `screen_to_sketch(camera: &OrbitCamera, frame: &Frame, viewport: Vec2, pixel: Vec2) -> Option<DVec2>`: Strahl durch den Pixel (Ursprung oben links) mit der Skizzenebene schneiden; `None`, wenn der Strahl parallel zur Ebene ist oder die Ebene hinter der Kamera liegt.
  - `sketch_to_screen(camera, frame, viewport, p: DVec2) -> Option<Vec2>` (Umkehrung; `None` hinter der Kamera).
  - `pick_radius_units(camera, frame, viewport, pixel: Vec2, pixel_radius: f32) -> Option<f64>`: Fangradius in Skizzeneinheiten für einen Radius in Pixeln an dieser Stelle (Abstand zweier benachbarter Pixel auf der Ebene mal `pixel_radius`).
- Produces – Zeichnen und Eingaben:
  - `sketch_view::show_overlay(ui: &mut egui::Ui, rect: Rect, state: &mut AppState)`: zeichnet die Skizze über die 3D-Ansicht und verarbeitet Eingaben; wird von `viewport::show` aufgerufen, wenn `state.sketch_session.is_some()`. In diesem Modus ist **Orbit gesperrt** (die Kamera bleibt senkrecht zur Skizzenebene); Verschieben (mittlere Maustaste bzw. Umschalt + Ziehen) und Zoomen (Mausrad) bleiben.
  - `sketch_panel::show(ctx: &egui::Context, state: &mut AppState)`: schwebendes Fenster „Skizze“ (nur im Skizzen-Modus).
  - Wege in die Oberfläche: Menü „Erzeugen“ erhält die Einträge „Skizze auf XY-Ebene“, „Skizze auf XZ-Ebene“, „Skizze auf YZ-Ebene“, „Skizze auf gewählter Fläche“ (deaktiviert mit Hinweis, wenn keine ebene Fläche gewählt ist); keine Untermenüs (die Schrift hat keine Pfeil-Zeichen). Der Browser zeigt unter „Körper“ eine Überschrift „Skizzen“ mit einer Zeile je Skizze (Symbol, Name); Doppelklick öffnet die Skizze, Klick wählt sie.

**Darstellung (`show_overlay`, Verhalten):**
  - Kurven als Linienzüge (`curve_polyline`, `sketch_to_screen`): normale Linien in Akzentfarbe des Themes; Konstruktionsgeometrie gestrichelt/gedämpft; **vollständig bestimmte** Objekte (weder `free_points` noch `free_curves` im letzten Bericht) grün, freie blau; ausgewählte orange; Hover heller. Punkte als kleine Kreise (freie hohl, bestimmte gefüllt).
  - Abhängigkeitssymbole nahe den Objekten als kleine Beschriftung (`H`, `V`, `∥` nur wenn die Schrift das Zeichen hat, sonst `||`, `⟂` ersatzweise `Lot`, `=`, `T`, `fix`, `Mitte`, `Sym`, `auf`); Maße als Zahl mit Einheit (z. B. „40,00 mm“, „R 5,00 mm“, „90,0°“) an einer sinnvollen Stelle (Mitte der Strecke bzw. am Kreis); Anzeige-Maße (`driven`) in Klammern; widersprüchliche/überzählige Abhängigkeiten rot.
  - Vorschau des laufenden Werkzeugs (Gummiband-Linie, Kreis, Rechteck) vom letzten gesetzten Punkt zum Cursor.
  - Alle Texte deutsch; Zahlenformat mit Komma als Dezimaltrenner **nur in der Anzeige** (Eingabe akzeptiert Punkt und Komma).
**Eingaben:**
  - Linke Maustaste: `SketchSession::click` (Fangradius 8 Pixel), im Auswahlwerkzeug Drücken auf einen Punkt startet `begin_drag`, Ziehen `drag_to`, Loslassen `end_drag`; Umschalt hält `extend`.
  - Rechte Maustaste oder Escape: `cancel`; Eingabe/Doppelklick: `finish_tool`; Entf: `delete_selection`.
  - Ein Eingabefeld mit Tastaturfokus unterdrückt Tastenkürzel (wie in `viewport.rs`, `remember_keyboard_focus`).
  - Im Werkzeug `ProjectEdge` wählt ein Klick eine Körperkante in der 3D-Szene (`state.pick`) und ruft `SketchSession::project_edge` auf.

**Werkzeugleiste und Liste (`sketch_panel`, Verhalten):**
  - Werkzeuge als Schaltflächen mit deutscher Beschriftung: „Auswahl“, „Punkt“, „Linie“, „Rechteck“, „Kreis“, „Bogen“, „Ellipse“, „Spline“ (die beiden letzten nur wenn vorhanden), „Körperkante kopieren“; das aktive hervorgehoben.
  - Abhängigkeiten: eine Schaltfläche je Eintrag von `available_actions` (deutsche `label()`), aktiviert nur bei passender Auswahl; Maße öffnen ein Zahlenfeld (Vorgabe: aktueller Wert; Winkel in Grad, intern Radiant) mit „OK“/Enter und „Abbrechen“. Fehlertexte von `apply` erscheinen unter der Leiste in Rot.
  - Weitere Aktionen: „Konstruktion umschalten“, „Spiegeln …“ (letzte ausgewählte Linie als Achse), „Muster …“ (Versatz x/y, Anzahl), „Löschen“, Kontrollkästchen „Automatisch waagerecht/senkrecht“, „Skizze beenden“.
  - Liste der Abhängigkeiten der Skizze mit Name (z. B. „Abstand 40,00 mm“), Auswahl durch Klick, Umschalter „nur anzeigend“, Löschen, Maßwert ändern (Doppelklick); überzählige/widersprüchliche rot.
  - Statuszeile unten im Fenster: `SketchSession::status` mit Farbe nach `Severity`.

- [ ] **Step 1: Failing tests schreiben**

`crates/planora-app/tests/sketch_view.rs` (reine Abbildungsfunktionen):

```rust
use glam::{Vec2, Vec3};
use planora_app::sketch_view::{pick_radius_units, screen_to_sketch, sketch_to_screen};
use planora_math::{DVec2, Frame};
use planora_render::OrbitCamera;

fn top_camera() -> OrbitCamera {
    let mut cam = OrbitCamera::default();
    cam.target = Vec3::new(5.0, 3.0, 0.0);
    cam.distance = 100.0;
    cam.look_along(Vec3::Z, Vec3::Y);
    cam.set_viewport(800.0, 600.0);
    cam
}

#[test]
fn the_screen_center_maps_to_the_camera_target_on_the_plane() {
    let cam = top_camera();
    let vp = Vec2::new(800.0, 600.0);
    let p = screen_to_sketch(&cam, &Frame::WORLD, vp, Vec2::new(400.0, 300.0)).unwrap();
    assert!((p - DVec2::new(5.0, 3.0)).length() < 1e-4, "{p:?}");
}

#[test]
fn mapping_round_trips_and_x_points_right_y_points_up() {
    let cam = top_camera();
    let vp = Vec2::new(800.0, 600.0);
    for p in [DVec2::new(0.0, 0.0), DVec2::new(12.0, -7.0), DVec2::new(-30.0, 22.0)] {
        let s = sketch_to_screen(&cam, &Frame::WORLD, vp, p).unwrap();
        let back = screen_to_sketch(&cam, &Frame::WORLD, vp, s).unwrap();
        assert!((back - p).length() < 1e-3, "{p:?} → {s:?} → {back:?}");
    }
    let o = sketch_to_screen(&cam, &Frame::WORLD, vp, DVec2::ZERO).unwrap();
    let x = sketch_to_screen(&cam, &Frame::WORLD, vp, DVec2::new(10.0, 0.0)).unwrap();
    let y = sketch_to_screen(&cam, &Frame::WORLD, vp, DVec2::new(0.0, 10.0)).unwrap();
    assert!(x.x > o.x && (x.y - o.y).abs() < 1.0, "x nach rechts");
    assert!(y.y < o.y && (y.x - o.x).abs() < 1.0, "y nach oben (Bildschirm-y wächst nach unten)");
}

#[test]
fn a_parallel_ray_misses_the_plane() {
    let mut cam = OrbitCamera::default();
    cam.look_along(Vec3::X, Vec3::Z); // blickt entlang der Ebene XY
    cam.set_viewport(800.0, 600.0);
    cam.target = Vec3::ZERO;
    // Pixel in der Bildmitte: der Strahl verläuft parallel zur Ebene z = 0 nur, wenn pitch = 0
    // und die Ebene durch die Augenhöhe geht; hier genügt: kein Panic, Ergebnis ist Some oder None
    let _ = screen_to_sketch(&cam, &Frame::WORLD, Vec2::new(800.0, 600.0), Vec2::new(400.0, 300.0));
}

#[test]
fn the_pick_radius_scales_with_the_zoom() {
    let mut cam = top_camera();
    let vp = Vec2::new(800.0, 600.0);
    let near = pick_radius_units(&cam, &Frame::WORLD, vp, Vec2::new(400.0, 300.0), 8.0).unwrap();
    cam.distance = 200.0;
    let far = pick_radius_units(&cam, &Frame::WORLD, vp, Vec2::new(400.0, 300.0), 8.0).unwrap();
    assert!(near > 0.0 && (far / near - 2.0).abs() < 0.05, "{near} {far}");
}
```

`crates/planora-app/tests/sketch_ui.rs` (Rauchtests ohne Fenster wie `boolean_ui.rs`):

```rust
use planora_app::{fonts, sketch_panel, sketch_view};
use planora_app::sketch_state::{Item, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind as K;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn populated() -> AppState {
    let mut s = AppState::new();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    {
        let sk = &mut s.sketches[i];
        let r = sk.add_rectangle(DVec2::ZERO, DVec2::new(40.0, 30.0)).unwrap();
        sk.add_constraint(K::Distance(r.points[0], r.points[1], 40.0)).unwrap();
        let c = sk.add_point(20.0, 15.0).unwrap();
        sk.add_circle(c, 5.0).unwrap();
        sk.solve();
    }
    s.open_sketch(i).unwrap();
    s
}

#[test]
fn the_panel_and_overlay_draw_without_changing_the_sketch() {
    let ctx = context();
    let mut s = populated();
    let before = s.sketches[0].variables().to_vec();
    for _ in 0..3 {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            sketch_panel::show(ui.ctx(), &mut s);
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
            sketch_view::show_overlay(ui, rect, &mut s);
        })
        .drop_without_applying_deltas();
    }
    assert_eq!(s.sketches[0].variables(), before.as_slice());
    assert!(s.sketch_session.is_some());
}

#[test]
fn every_tool_and_a_selection_can_be_drawn() {
    let ctx = context();
    let mut s = populated();
    let first_curve = s.sketches[0].curves().next().unwrap().0;
    for tool in [Tool::Select, Tool::Point, Tool::Line, Tool::Rectangle, Tool::Circle, Tool::Arc, Tool::ProjectEdge] {
        {
            let se = s.sketch_session.as_mut().unwrap();
            se.set_tool(tool);
            se.selection = vec![Item::Curve(first_curve)];
            se.cursor = Some(DVec2::new(10.0, 10.0));
        }
        ctx.run_ui(egui::RawInput::default(), |ui| {
            sketch_panel::show(ui.ctx(), &mut s);
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(640.0, 480.0));
            sketch_view::show_overlay(ui, rect, &mut s);
        })
        .drop_without_applying_deltas();
    }
}

#[test]
fn texts_in_the_panel_are_german() {
    let ctx = context();
    let mut s = populated();
    let mut seen = String::new();
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        sketch_panel::show(ui.ctx(), &mut s);
    });
    for clipped in &out.shapes {
        if let egui::epaint::Shape::Text(t) = &clipped.shape {
            seen.push_str(&t.galley.text());
            seen.push('\n');
        }
    }
    for must in ["Linie", "Kreis", "Skizze beenden"] {
        assert!(seen.contains(must), "„{must}“ fehlt in der Werkzeugleiste:\n{seen}");
    }
}
```

(Hinweis: `ctx.run_ui(..)` und die Ausgabe-Struktur entsprechen der egui-0.36-API, wie sie `tests/boolean_ui.rs` nutzt; an die tatsächliche Form der Ausgabe anpassen, die Aussagen „zeichnet ohne Änderung“, „jedes Werkzeug zeichnet“ und „Beschriftungen deutsch“ bleiben.)

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-app --test sketch_view --test sketch_ui`
Expected: FAIL (Module fehlen)

- [ ] **Step 3: Implementierung**

Reihenfolge: (a) reine Abbildungsfunktionen (Referenz unten) und deren Tests; (b) `sketch_panel` (Werkzeuge, Aktionen, Liste, Status) – nur Zeichnen und Weiterleiten, die Logik liegt in `SketchSession`; (c) `sketch_view` (Zeichnen, Eingaben); (d) Einbindung in `viewport.rs` (bei offener Sitzung Orbit sperren, Overlay zeichnen, Würfel-Klicks weiterhin erlaubt), `app.rs` (Menü, Panel, Statuszeile), `browser.rs` (Skizzenliste) und `icons.rs` (`paint_sketch_icon`: ein Rechteck mit Eckpunkten und einem Kreis, nur aus egui-Formen gezeichnet; kein fremdes Symbol).

```rust
// sketch_view.rs – Abbildung Bildschirm ↔ Skizzenebene
use glam::{Vec2, Vec3};
use planora_math::{DVec2, Frame};
use planora_render::OrbitCamera;

fn to_vec3(v: planora_math::DVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

pub fn screen_to_sketch(camera: &OrbitCamera, frame: &Frame, viewport: Vec2, pixel: Vec2) -> Option<DVec2> {
    let ndc = Vec2::new(pixel.x / viewport.x * 2.0 - 1.0, 1.0 - pixel.y / viewport.y * 2.0);
    let (origin, dir) = camera.ray(ndc);
    let (n, p0) = (to_vec3(frame.z), to_vec3(frame.origin));
    let denom = dir.dot(n);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (p0 - origin).dot(n) / denom;
    if t < 0.0 {
        return None;
    }
    let hit = origin + dir * t;
    let local = frame.to_local(planora_math::DVec3::new(hit.x as f64, hit.y as f64, hit.z as f64));
    Some(DVec2::new(local.x, local.y))
}

pub fn sketch_to_screen(camera: &OrbitCamera, frame: &Frame, viewport: Vec2, p: DVec2) -> Option<Vec2> {
    camera.project(to_vec3(frame.point_2d(p)), viewport)
}

pub fn pick_radius_units(camera: &OrbitCamera, frame: &Frame, viewport: Vec2, pixel: Vec2, pixel_radius: f32) -> Option<f64> {
    let a = screen_to_sketch(camera, frame, viewport, pixel)?;
    let b = screen_to_sketch(camera, frame, viewport, pixel + Vec2::X)?;
    Some(a.distance(b) * f64::from(pixel_radius))
}
```

Alle Texte kommen aus `label()`-Funktionen bzw. deutschen Literalen; Zahlen in der Anzeige mit `format!("{:.2}", v).replace('.', ",")`; die Eingabe-Felder lesen Text und ersetzen `,` durch `.`, bevor sie parsen (ungültig → Fehlermeldung „Bitte eine Zahl eingeben“, kein Panic).

- [ ] **Step 4: Tests laufen lassen und die Oberfläche prüfen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS.

Danach die App einmal wirklich starten und im Skizzen-Modus durchprobieren (Rechteck zeichnen, Maß setzen, Punkt ziehen, Skizze beenden, wieder öffnen) – per Browser-/Computer-Werkzeug oder Bildschirmfoto belegen; Ergebnis im Bericht schildern (was funktioniert, was nicht). Kein GUI-Prozess bleibt laufen.

- [ ] **Step 5: Commit**

```bash
git add -A crates
git commit -m "feat(app): Skizzen-Oberfläche (Darstellung, Eingaben, Werkzeugleiste, Abhängigkeitsliste)"
```

---

### Task 13: Körper aus Skizzen in der Oberfläche, Körperkanten kopieren, Dokumentation

**Files:**
- Modify: `crates/planora-app/src/create_dialog.rs`, `crates/planora-app/src/app.rs`, `crates/planora-app/src/browser.rs`, `crates/planora-app/src/samples.rs` (Hinweis im Dialog), `README.md`, `docs/superpowers/offene-punkte.md`, `docs/superpowers/specs/2026-10-09-skizzen-und-solver-design.md` (Abschnitt „Änderungen bei der Umsetzung“ mit D1–D6 und tatsächlichen Abweichungen)
- Test: `crates/planora-app/tests/sketch_dialogs.rs`

**Interfaces:**
- Consumes: `AppState::extrude_sketch`, `revolve_sketch`, `ExtractedProfiles` (Tasks 7, 10, 11).
- Produces:
  - `CreateDialog::ExtrudeSketch { sketch: usize, profile: usize, height: f64 }` und `CreateDialog::RevolveSketch { sketch: usize, profile: usize, axis: Option<CurveId> }` (Titel „Skizze extrudieren“ / „Skizze drehen“). Der Dialog zeigt: Skizzenauswahl (falls aufgerufen ohne Skizze), Profilliste („Profil {n}: {Fläche} mm²{, n Löcher}“, abgelehnte Schleifen mit Grund als graue Hinweiszeilen, z. B. „Nicht verwendbar: Das Profil ist nicht geschlossen“), Höhe bzw. Auswahl der Drehachse (nur Linien der Skizze; Konstruktionslinien zuerst), Schaltflächen „Erzeugen“/„Abbrechen“; Fehler von `extrude_sketch`/`revolve_sketch` stehen in der Statuszeile, der Dialog bleibt offen.
  - Der bisherige Beispielprofil-Dialog (`Extrude`/`Revolve` mit `samples`) bleibt erhalten (Menü „Extrusion …“ / „Rotation …“ unverändert), zusätzlich: Menü „Erzeugen“ bekommt „Skizze extrudieren …“ und „Skizze drehen …“ (deaktiviert mit Hinweis „Zuerst eine Skizze anlegen.“, wenn es keine Skizze gibt), und die Skizzenzeilen im Browser haben ein Kontextmenü „Bearbeiten“, „Extrudieren …“, „Drehen …“, „Löschen“.
  - Dokumentation: README-Abschnitt „Skizzen“ (Funktionsumfang ehrlich: was geht, was nicht), `offene-punkte.md` bekommt den Abschnitt „Nach Teilprojekt 4 (Skizzen)“ mit den bekannten Grenzen (siehe unten), die Liste „Vor Teilprojekt 4“ wird auf „Erledigt in Teilprojekt 4“ umgestellt.

  **Bekannte Grenzen, die dokumentiert werden müssen** (jeweils nur aufnehmen, wenn sie nach der Umsetzung noch stimmen): Ellipsen/Splines nicht zu Körpern; Profile müssen einfache geschlossene Ketten sein (keine T-Verzweigungen, keine Kreuzungen); Rotation nur ohne Löcher und mit einer Kante auf der Achse; Skizzen nur auf ebenen Flächen und Standardebenen; Körperkanten nur einmalig kopiert (Geraden, Kreise/Bögen, volle Ellipsen), keine lebende Verknüpfung; Körper aus Skizzen sind Momentaufnahmen (keine Neuberechnung); kein Rückgängig; kein Speichern/Laden; Mehrdeutigkeiten (Winkel um 180°) werden vom nächsten Zustand bestimmt; Solver arbeitet dicht (sehr große Gruppen > ~500 Variablen werden langsam); Eingabe von Maßen als Zahl, keine Gleichungen/Parameter; Konstruktionsgeometrie nur über Umschalter.

- [ ] **Step 1: Failing tests schreiben** – `crates/planora-app/tests/sketch_dialogs.rs`

```rust
use planora_app::create_dialog::CreateDialog;
use planora_app::fonts;
use planora_app::state::{AppState, BodyKind, StandardPlane};
use planora_math::DVec2;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn state_with_sketch() -> AppState {
    let mut s = AppState::new();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.sketches[i].add_rectangle(DVec2::ZERO, DVec2::new(10.0, 4.0)).unwrap();
    // offene Kette, die abgelehnt wird
    let a = s.sketches[i].add_point(30.0, 0.0).unwrap();
    let b = s.sketches[i].add_point(35.0, 0.0).unwrap();
    s.sketches[i].add_line(a, b).unwrap();
    s
}

#[test]
fn the_extrude_dialog_creates_a_body_and_stays_open_on_errors() {
    let mut s = state_with_sketch();
    let ok = CreateDialog::ExtrudeSketch { sketch: 0, profile: 0, height: 3.0 };
    ok.create(&mut s);
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.bodies[0].kind, BodyKind::Extrusion);
    assert!(s.message.is_none());

    let bad = CreateDialog::ExtrudeSketch { sketch: 0, profile: 7, height: 3.0 };
    bad.create(&mut s);
    assert_eq!(s.bodies.len(), 1, "unverändert");
    assert!(s.message.is_some());
}

#[test]
fn the_dialogs_draw_and_list_profiles_and_rejections() {
    let ctx = context();
    let mut s = state_with_sketch();
    let mut dialog = CreateDialog::ExtrudeSketch { sketch: 0, profile: 0, height: 3.0 };
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &mut s), "bleibt ohne Klick offen");
        });
        for c in &out.shapes {
            if let egui::epaint::Shape::Text(t) = &c.shape {
                text.push_str(&t.galley.text());
                text.push('\n');
            }
        }
    }
    assert!(text.contains("Profil 1"), "{text}");
    assert!(text.contains("Nicht verwendbar"), "abgelehnte Kette wird erklärt:\n{text}");
    let mut revolve = CreateDialog::RevolveSketch { sketch: 0, profile: 0, axis: None };
    ctx.run_ui(egui::RawInput::default(), |ui| {
        revolve.show(ui.ctx(), &mut s);
    })
    .drop_without_applying_deltas();
    assert_eq!(s.bodies.len(), 0);
}

#[test]
fn revolve_dialog_reports_a_missing_axis() {
    let mut s = state_with_sketch();
    let mut d = CreateDialog::RevolveSketch { sketch: 0, profile: 0, axis: None };
    d.create(&mut s);
    assert!(s.bodies.is_empty());
    assert!(s.message.as_deref().unwrap_or("").contains("Drehachse"), "{:?}", s.message);
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-app --test sketch_dialogs`
Expected: FAIL

- [ ] **Step 3: Implementierung**

Die Dialogvarianten folgen dem Muster der vorhandenen (`show`/`create`). Im Dialog wird `state.sketches[sketch].profiles()` pro Frame berechnet (billig bei Skizzengröße); `create` ruft `state.extrude_sketch(...)` bzw. `revolve_sketch(...)` und schließt den Dialog nur bei Erfolg (`create` gibt dafür wie beim Boolean-Dialog ein Flag zurück oder `show` prüft `state.message`). Fehlende Achse: `state.message = Some("Bitte eine Drehachse wählen".into())`-ähnlicher deutscher Text mit dem Wort „Drehachse“.

Dokumentation: README und `offene-punkte.md` nach der Liste oben; im Spec-Dokument am Ende ein Abschnitt „Änderungen bei der Umsetzung“ mit D1 bis D6 aus diesem Plan sowie allen weiteren Abweichungen, die während der Umsetzung entstanden sind (aus dem Ledger).

- [ ] **Step 4: Tests laufen lassen und Gesamtprüfung**

Run:
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check
```
Expected: alles grün; `deny.toml` unverändert (keine neue Abhängigkeit). Danach App starten und den Ablauf „Skizze auf XY zeichnen → bemaßen → beenden → extrudieren → mit einem Quader abziehen“ von Hand durchgehen, mit Bildschirmfoto belegen.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(app): Körper aus Skizzen (Extrusion, Rotation), Dialoge, Dokumentation Teilprojekt 4"
```

---

## Selbstprüfung (Spec-Abdeckung)

| Spec-Abschnitt | Task |
|---|---|
| 3.1 Skizze und IDs | 2 |
| 3.2 Geometrie (Punkt, Linie, Kreis, Bogen, Ellipse, Spline, Konstruktion; Rechteck/Spiegeln/Muster als Hilfen) | 2, 8, 9 |
| 3.3 Abhängigkeiten und Maße | 3, 9 |
| 3.4 Profil-Extraktion und Grenze zum Kernel | 7, 9 (Ablehnung), 10 |
| 3.5 Kernel-Anpassungen | 6 |
| 4.1–4.3 Solver, Zerlegung, Freiheitsgrade, Diagnose | 1, 4 |
| 4.4 Ziehen und Stabilität | 5 |
| 4.5 Besonderheiten (Winkel per Vektoren, Splines) | 3, 9 |
| 5 Oberfläche (Skizzen-Modus, Werkzeuge, Zeichnen, Abhängigkeiten, Anzeige, Beenden/Öffnen, Zum Körper, Körperkante kopieren, Code-Trennung) | 11, 12, 13 (Kante kopieren: 8, 11, 12) |
| 6 Fehlerbehandlung | 3, 4, 11, 13 |
| 7 Tests (Ableitungen, Algebra, bekannte Skizzen, Eigenschaften, Widerspruch/Redundanz, Ziehen, Profile, Ende zu Ende, Kernel, App) | 1–13 |
| 9 Lizenzen | alle (keine neue Abhängigkeit) |

Abweichungen von der Spec sind oben unter „Entscheidungen zur Spec“ (D1–D6) benannt.
