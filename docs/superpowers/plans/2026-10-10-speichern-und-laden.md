# PlanoraCAD Speichern, Laden, Projektverwaltung und Revisionen – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dokumente (die Feature-Zeitleiste) dauerhaft speichern und laden, mit Projektverwaltung (Projekte → Ordner → Dokumente), Versionsbaum (Speichern = neue Version, Zweige, Wiederherstellen) und mehreren offenen Dokumenten als Tabs.

**Architecture:** Neues Crate `planora-project` (ohne Oberfläche) hält das Dateiformat (serde-Format-Typen, entkoppelt von den internen Strukturen), den Projektbaum auf der Festplatte und den Versionsspeicher je Dokument (atomares Schreiben). `planora-feature` bekommt `Document::snapshot()/from_snapshot()` und einen Änderungszähler, `planora-sketch` bekommt `Sketch::to_data()/from_data()`. Die App führt einen `Workspace` mit Tabs (je Tab eigener `AppState`, eigene Dialoge) sowie zwei andockbare Panels „Projekte“ und „Versionen“.

**Tech Stack:** Rust (Edition 2024), bestehende Crates, neu: `planora-project` (intern) sowie `serde` (derive) und `serde_json`.

**Spec:** `docs/superpowers/specs/2026-10-10-speichern-und-laden-design.md`

## Entscheidungen zur Spec (Planungsstand 2026-10-10)

Sie konkretisieren die Spec; Abweichungen sind begründet. Sie werden dem Projekteigner am Ende genannt und in die Spec (Abschnitt „Änderungen bei der Umsetzung“) eingetragen.

- **P1 – Snapshot statt Zugriff auf Interna:** `Document::snapshot()` liefert reine Daten (`Snapshot { features, marker, next_id }`, Features als `SnapshotFeature { id, name, suppressed, kind }`), `Document::from_snapshot(snapshot, chord_tolerance)` baut ein Dokument neu auf und rechnet einmal. Dateiformat-Typen liegen nur in `planora-project`.
- **P2 – Skizzen als Rohdaten:** `Sketch::to_data()`/`Sketch::from_data()` übertragen **alle** Einträge der Arenen (auch gelöschte, mit `alive = false`), damit Ids unverändert bleiben. `from_data` löst **nicht**; die Neuberechnung des Dokuments löst die Skizzen (Abweichung von Spec §10, die ein Lösen mit Vergleich vorsah: einfacher, derselbe Nutzen).
- **P3 – Änderungszähler:** `Document::edit_count()` steigt bei jeder öffentlichen Änderung (Features hinzufügen, ändern, umbenennen, löschen, unterdrücken, Marker setzen). Ein Tab ist „geändert“, wenn der Zähler vom beim Speichern notierten Wert abweicht oder eine Skizze offen ist.
- **P4 – Sperre nur per Rückfrage:** Ohne neue Abhängigkeit lässt sich nicht prüfen, ob der Prozess einer `.sperre` noch lebt. Eine vorhandene Sperre wird deshalb immer gemeldet; der Nutzer kann sie nach Rückfrage übernehmen (Spec §4 sagte „veraltete Sperren“).
- **P5 – Schreibschutz auf der Oberfläche:** Schreibgeschützte Versions-Tabs sperren Browser, Eigenschaften und die Menüs „Erzeugen“/„Bearbeiten“ (deaktivierte Bedienelemente); `AppState` bekommt kein Schreibschutz-Flag.
- **P6 – Zweig je Tab:** Jeder Tab merkt sich die Version, auf der sein Stand beruht (`based_on`). Beim Speichern entscheidet sie: Ist sie die Spitze ihres Zweigs, wird dort angehängt; sonst muss ein neuer Zweigname angegeben werden. `aktiver_zweig` in `dokument.json` ist nur der Zweig, dessen Spitze beim Öffnen geladen wird.
- **P7 – Szenenstand eindeutig:** `AppState::scene_version` stammt aus einem globalen Zähler, damit der Wechsel zwischen Tabs die GPU-Szene sicher neu hochlädt.
- **P9 – Autosicherung nur für gespeicherte Dokumente:** Unbenannte Dokumente haben noch keinen Ort; für sie gibt es keine Autosicherung (Spec §7 sah den Konfigurationsordner vor). Später nachrüstbar.
- **P10 – Wiederherstellen aus einer Ansicht:** Ist das Dokument schon beschreibbar in einem Tab offen, wirkt *Wiederherstellen* in diesem Tab (nur ohne ungespeicherte Änderungen); sonst wird die Ansicht bearbeitbar (holt die Sperre).
- **P8 – Konfiguration:** `Config { root }` liegt in `planora-project` (dort ist serde), Speicherort `%APPDATA%\PlanoraCAD\config.json` bzw. `$XDG_CONFIG_HOME/planora-cad/config.json` (Rückfall `~/.config`).

## Global Constraints

- Reines Rust; Zahlen `f64`, Einheit Millimeter; zentrale `Tolerance`. Keine freien Epsilon-Zahlen: benannte `const`s mit Kommentar.
- `planora-project` hängt nur von `planora-math`, `planora-kernel` (nur `BoolOp`), `planora-sketch`, `planora-feature`, `serde`, `serde_json` und `thiserror` ab; es kennt keine Oberfläche. `planora-feature`, `planora-sketch` und der Kernel bekommen **kein** `serde`.
- **Jede neue Abhängigkeit wird vorher auf Lizenz geprüft.** Task 1 führt `cargo deny check` nach dem Eintragen aus; meldet es eine Lizenz außerhalb der Erlaubnisliste in `deny.toml`, **anhalten und den Menschen fragen** – die Liste nie selbstständig erweitern. Projektlizenz GPL-3.0-only.
- Kein Autodesk-Material (Code, Icons, Namen, Dateiformate); Oberflächentexte und Fehlermeldungen auf Deutsch.
- Keine Panics auf Dateiinhalten oder Nutzereingaben: Alles an den Grenzen wird geprüft und liefert `Result`; Änderungsoperationen sind atomar (bei Fehler bleibt alles unverändert).
- Alle Schreibvorgänge auf der Festplatte laufen über `atomic_write` (erst `.tmp`, `sync_all`, dann Umbenennen).
- Nichts außerhalb der Wurzel wird gelesen oder geschrieben (kanonische Pfadprüfung).
- Alle bestehenden Tests (596) bleiben grün (ggf. mechanisch angepasst). `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` und `cargo deny check` müssen am Ende jeder Aufgabe grün sein.
- Plattformen: Windows (MSVC) und Linux. Befehle in Bash (Git Bash): `export PATH="$PATH:/c/Users/denni/.cargo/bin"`. Kein Text mit Apostroph in Bash-Heredocs (Write-Werkzeug nutzen).

## Review Focus

Eingaben, die die Spec andeutet, die aber leicht durchrutschen; jede Zeile hat einen Test in der genannten Task:

1. **Beschädigte, abgeschnittene, zu neue oder bösartige Dateien** (NaN/Inf, falsche Typen, Ids außerhalb, Verweise auf spätere Features, doppelte Ids, riesige Zahlen, riesige Listen): Fehlermeldung mit Pfad, nie Panik, Dokument unverändert (Tasks 2–4, 7).
2. **Absturz mitten im Speichern:** alter Stand bleibt lesbar, verwaiste Versionsdatei wird erkannt und gemeldet, nichts wird halb überschrieben (Task 5, 7).
3. **Pfade und Namen:** `..`, absolute Pfade, `CON`/`NUL`, Junctions/Symlinks aus der Wurzel hinaus, leere/zu lange Namen, Namenskollision (Task 5, 6).
4. **Versionsbaum bleibt gültig:** Eltern-Zeiger immer vorhanden, kein Zyklus, Löschen nur ohne Kinder, Speichern auf alter Version ohne Zweignamen wird abgelehnt (Task 7).
5. **Datenverlust in der Oberfläche:** Schließen/Beenden mit ungespeicherten Änderungen fragt nach, Speichern mit offener Skizze nimmt den Entwurf mit, ein Fehler beim Schreiben lässt den Tab „geändert“ (Task 8, 9).

## Hinweis zur Verifikation und zum Code in diesem Plan

Der Code in den Schritten ist die **Referenzimplementierung des beabsichtigten Entwurfs** und wurde noch nicht kompiliert. Er darf zum Kompilieren und zur Fehlerbehebung angepasst und intern umstrukturiert werden, solange die benannten Schnittstellen („Interfaces“) und **alle Tests unverändert** erfüllt werden. Schlägt ein Test fehl, wird der Code korrigiert, nicht der Test – es sei denn, der Test selbst enthält einen Tippfehler (dann berichten). Rust ist installiert.

## Dateistruktur

```
Cargo.toml                          (ändern: planora-project, serde, serde_json)
crates/planora-sketch/
├─ src/data.rs                      (neu: SketchData, to_data/from_data)
├─ src/lib.rs                       (ändern: Export)
└─ tests/data.rs
crates/planora-feature/
├─ src/snapshot.rs                  (neu: Snapshot, snapshot/from_snapshot)
├─ src/model.rs, ops.rs, recompute.rs (ändern: edit_count)
└─ tests/snapshot.rs
crates/planora-project/             (neu)
├─ Cargo.toml
├─ src/ lib.rs error.rs format.rs convert.rs fsutil.rs library.rs docstore.rs config.rs
└─ tests/ data/format1.json  format.rs fsutil.rs library.rs docstore.rs
crates/planora-app/src/
├─ workspace.rs                     (neu: DocTab, Workspace, Speichern/Öffnen/Schließen-Logik)
├─ projects_panel.rs, versions_panel.rs, prompts.rs  (neu)
├─ state.rs, app.rs, lib.rs, viewport.rs (ändern)
└─ tests/ workspace.rs tabs_ui.rs projects_ui.rs versions_ui.rs
tools/gui-drive.ps1                 (vorhanden, Handprüfung)
```

---

### Task 1: Abhängigkeiten, Lizenzprüfung, Crate-Gerüst

**Files:**
- Modify: `Cargo.toml` (Workspace)
- Create: `crates/planora-project/Cargo.toml`, `crates/planora-project/src/lib.rs`

**Interfaces:**
- Produces: Workspace-Abhängigkeiten `serde`, `serde_json`, `planora-project`; leeres Crate `planora_project`.

- [ ] **Step 1: Abhängigkeiten eintragen**

In `Cargo.toml` unter `[workspace.dependencies]` bei den internen Crates ergänzen:

```toml
planora-project = { path = "crates/planora-project" }
```

und bei den externen Crates:

```toml
# Dateiformat (Teilprojekt 6). Lizenzen: serde und serde_json MIT OR Apache-2.0; Unterpakete siehe cargo deny.
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Crate anlegen**

`crates/planora-project/Cargo.toml`:

```toml
[package]
name = "planora-project"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
planora-math.workspace = true
planora-kernel.workspace = true
planora-sketch.workspace = true
planora-feature.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
proptest.workspace = true

[lints]
workspace = true
```

`crates/planora-project/src/lib.rs`:

```rust
//! Dateiformat, Projektverwaltung und Versionsspeicher von PlanoraCAD (ohne Oberfläche).
```

- [ ] **Step 3: Lizenzprüfung (Pflicht)**

Run:
```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin" && cd /d/Testumgebung/PlanoraCAD && cargo build -p planora-project 2>&1 | tail -3 && cargo deny check 2>&1 | tail -15
```
Expected: `advisories ok, bans ok, licenses ok, sources ok`. Meldet `cargo deny` eine Lizenz außerhalb der Erlaubnisliste oder einen Fund bei `sources`/`bans`: **nichts ändern, anhalten, Fund (Paket, Lizenz) berichten.** Hinweis: Warnungen zu mehrfachen Versionen (`multiple-versions = "warn"`) sind erlaubt.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/planora-project
git commit -m "build: Crate planora-project, serde und serde_json (Lizenzen geprüft)"
```

---

### Task 2: Skizzen-Rohdaten (`Sketch::to_data` / `from_data`)

**Files:**
- Create: `crates/planora-sketch/src/data.rs`
- Modify: `crates/planora-sketch/src/lib.rs`, `crates/planora-sketch/src/model.rs` (nur `pub(crate)`-Sichtbarkeit, falls nötig)
- Test: `crates/planora-sketch/tests/data.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct SketchData { pub name: String, pub frame: Frame, pub tolerance: Tolerance,
      pub vars: Vec<f64>, pub points: Vec<PointRecord>, pub curves: Vec<CurveRecord>,
      pub constraints: Vec<ConstraintRecord> }
  pub struct PointRecord { pub x: usize, pub y: usize, pub construction: bool, pub alive: bool }
  pub struct CurveRecord { pub kind: CurveKind, pub construction: bool, pub alive: bool }
  pub struct ConstraintRecord { pub kind: ConstraintKind, pub branch: i8, pub driven: bool,
      pub enabled: bool, pub internal: bool, pub extra: Option<usize>, pub alive: bool }
  impl Sketch { pub fn to_data(&self) -> SketchData;
                pub fn from_data(d: SketchData) -> Result<Sketch, SketchError>; }
  ```
  alle `#[derive(Clone, Debug, PartialEq)]`. Alle Einträge der Arenen werden in Reihenfolge übertragen (auch gelöschte), `from_data` setzt dieselben Ids. `from_data` löst nicht und rechnet nichts.

- [ ] **Step 1: Test schreiben** (`crates/planora-sketch/tests/data.rs`)

```rust
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::*;

fn frame() -> Frame {
    Frame { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Y, z: DVec3::Z }
}

/// Skizze mit allen Kurvenarten und mehreren Abhängigkeiten, ein Objekt gelöscht.
fn rich_sketch() -> Sketch {
    let mut s = Sketch::new(frame(), "Reich", Tolerance::default());
    let r = s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(20.0, 10.0)).unwrap();
    let c = s.add_point(30.0, 5.0).unwrap();
    let circle = s.add_circle(c, 3.0).unwrap();
    let a = s.add_point(33.0, 5.0).unwrap();
    let b = s.add_point(30.0, 8.0).unwrap();
    let arc = s.add_arc(c, a, b).unwrap();
    let ec = s.add_point(50.0, 5.0).unwrap();
    let ellipse = s.add_ellipse(ec, DVec2::new(6.0, 0.0), 3.0).unwrap();
    let sp: Vec<_> = [(0.0, 20.0), (5.0, 24.0), (10.0, 20.0)]
        .iter().map(|&(x, y)| s.add_point(x, y).unwrap()).collect();
    let spline = s.add_spline(&sp).unwrap();
    s.add_constraint(ConstraintKind::Radius(circle, 3.0)).unwrap();
    s.add_constraint(ConstraintKind::Fixed(c, 30.0, 5.0)).unwrap();
    s.add_constraint(ConstraintKind::Distance(r.points[0], r.points[1], 20.0)).unwrap();
    let doomed = s.add_constraint(ConstraintKind::Equal(circle, arc)).unwrap();
    s.remove_constraint(doomed).unwrap();
    s.set_construction(ellipse, true).unwrap();
    let _ = spline;
    s.solve();
    s
}

#[test]
fn a_sketch_survives_the_round_trip_with_identical_data() {
    let s = rich_sketch();
    let d = s.to_data();
    let t = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(t.to_data(), d);
    assert_eq!(t.profiles().profiles.len(), s.profiles().profiles.len());
    // gelöschte Einträge behalten ihren Platz, damit Ids gültig bleiben
    assert!(d.constraints.iter().any(|c| !c.alive));
}

#[test]
fn a_restored_sketch_still_solves_and_edits() {
    let s = rich_sketch();
    let mut t = Sketch::from_data(s.to_data()).unwrap();
    assert!(matches!(t.solve().status, SolveStatus::Solved | SolveStatus::UnderConstrained));
    let p = t.points().next().unwrap().0;
    t.set_point_xy(p, 1.0, 1.0).unwrap();
}

fn broken(f: impl FnOnce(&mut SketchData)) -> SketchError {
    let mut d = rich_sketch().to_data();
    f(&mut d);
    Sketch::from_data(d).unwrap_err()
}

#[test]
fn invalid_data_is_rejected_without_panic() {
    let _ = broken(|d| d.vars[0] = f64::NAN);
    let _ = broken(|d| d.vars[0] = 1e300);
    let _ = broken(|d| d.points[0].x = 10_000);
    let _ = broken(|d| d.curves[0].kind = CurveKind::Line {
        a: PointId::from_index(9_999), b: PointId::from_index(0) });
    let _ = broken(|d| d.constraints[0].extra = Some(10_000));
    let _ = broken(|d| d.constraints[0].branch = 5);
    let _ = broken(|d| d.tolerance.linear = -1.0);
    let _ = broken(|d| d.frame.x = DVec3::ZERO);
    // lebende Kurve verweist auf gelöschten Punkt
    let _ = broken(|d| {
        let idx = match &d.curves[0].kind { CurveKind::Line { a, .. } => a.index(), _ => 0 };
        d.points[idx].alive = false;
    });
    let _ = broken(|d| d.constraints[0].kind = ConstraintKind::Radius(
        CurveId::from_index(0), f64::INFINITY));
}
```

Hinweise: `PointId`/`CurveId` sind `Id<…>` aus `planora_kernel` (mit `from_index`); die Tests nutzen sie über `planora_sketch::{PointId, CurveId}` (Ids sind dort bereits exportiert). `add_rectangle` liefert `RectIds` – die Feldnamen (`points`) vor dem Schreiben im Code prüfen und anpassen, ebenso `SolveStatus`-Varianten. Fehlt ein genannter Helfer, mit den vorhandenen Methoden gleichwertig nachbauen.

- [ ] **Step 2: Test laufen lassen, Fehlschlag prüfen**

Run: `cargo test -p planora-sketch --test data`
Expected: FAIL (`to_data` fehlt).

- [ ] **Step 3: Implementierung** (`crates/planora-sketch/src/data.rs`)

```rust
//! Rohdaten einer Skizze zum Speichern und Laden. Alle Einträge der Arenen werden in ihrer
//! Reihenfolge übertragen (auch gelöschte), damit Ids nach dem Laden dieselben sind.

use crate::constraint::{ConstraintData, ConstraintKind, LineOrPoints};
use crate::error::SketchError;
use crate::model::{CurveData, CurveKind, MAX_COORD, MAX_SPLINE_POINTS, PointData, Sketch, finite};
use crate::solver::SolveCache;
use planora_kernel::Arena;
use planora_math::{Frame, Tolerance};

#[derive(Clone, Debug, PartialEq)]
pub struct PointRecord {
    pub x: usize,
    pub y: usize,
    pub construction: bool,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveRecord {
    pub kind: CurveKind,
    pub construction: bool,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintRecord {
    pub kind: ConstraintKind,
    pub branch: i8,
    pub driven: bool,
    pub enabled: bool,
    pub internal: bool,
    pub extra: Option<usize>,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SketchData {
    pub name: String,
    pub frame: Frame,
    pub tolerance: Tolerance,
    pub vars: Vec<f64>,
    pub points: Vec<PointRecord>,
    pub curves: Vec<CurveRecord>,
    pub constraints: Vec<ConstraintRecord>,
}

fn bad(msg: impl Into<String>) -> SketchError {
    SketchError::InvalidInput(msg.into())
}

impl Sketch {
    pub fn to_data(&self) -> SketchData {
        SketchData {
            name: self.name.clone(),
            frame: *self.frame(),
            tolerance: *self.tolerance(),
            vars: self.vars.clone(),
            points: self.points.iter().map(|(_, p)| PointRecord {
                x: p.x, y: p.y, construction: p.construction, alive: p.alive,
            }).collect(),
            curves: self.curves.iter().map(|(_, c)| CurveRecord {
                kind: c.kind.clone(), construction: c.construction, alive: c.alive,
            }).collect(),
            constraints: self.constraints.iter().map(|(_, c)| ConstraintRecord {
                kind: c.kind.clone(), branch: c.branch, driven: c.driven,
                enabled: c.enabled, internal: c.internal, extra: c.extra, alive: c.alive,
            }).collect(),
        }
    }

    /// Baut eine Skizze aus Rohdaten auf. Prüft alles (Zahlen endlich und im Bereich, Indizes
    /// vorhanden, lebende Objekte verweisen nur auf lebende) und ändert nichts an den Daten.
    pub fn from_data(d: SketchData) -> Result<Sketch, SketchError> {
        if !(d.tolerance.linear.is_finite() && d.tolerance.linear > 0.0
            && d.tolerance.angular.is_finite() && d.tolerance.angular > 0.0)
        {
            return Err(bad("Die Toleranz der Skizze ist ungültig"));
        }
        for v in [d.frame.origin, d.frame.x, d.frame.y, d.frame.z] {
            if !v.is_finite() || v.abs().max_element() > MAX_COORD {
                return Err(bad("Das Koordinatensystem der Skizze ist ungültig"));
            }
        }
        if !d.frame.is_orthonormal(d.tolerance.linear) {
            return Err(bad("Das Koordinatensystem der Skizze ist ungültig"));
        }
        for v in &d.vars {
            finite(*v, "Ein Wert der Skizze")?;
        }
        let nv = d.vars.len();
        let var_ok = |i: usize| i < nv;
        let np = d.points.len();
        let nc = d.curves.len();
        let point_ok = |p: crate::model::PointId| p.index() < np;
        let curve_ok = |c: crate::model::CurveId| c.index() < nc;
        let point_alive = |p: crate::model::PointId| d.points[p.index()].alive;
        let curve_alive = |c: crate::model::CurveId| d.curves[c.index()].alive;

        for p in &d.points {
            if !var_ok(p.x) || !var_ok(p.y) {
                return Err(bad("Ein Punkt verweist auf einen fehlenden Wert"));
            }
        }
        for c in &d.curves {
            let pts = c.kind.points();
            if pts.iter().any(|p| !point_ok(*p)) {
                return Err(bad("Eine Kurve verweist auf einen fehlenden Punkt"));
            }
            match &c.kind {
                CurveKind::Circle { radius, .. } if !var_ok(*radius) => {
                    return Err(bad("Ein Kreis verweist auf einen fehlenden Wert"));
                }
                CurveKind::Ellipse { ax, ay, minor, .. }
                    if !(var_ok(*ax) && var_ok(*ay) && var_ok(*minor)) =>
                {
                    return Err(bad("Eine Ellipse verweist auf einen fehlenden Wert"));
                }
                CurveKind::Spline { points }
                    if points.len() < 2 || points.len() > MAX_SPLINE_POINTS =>
                {
                    return Err(bad("Ein Spline hat eine ungültige Zahl von Stützpunkten"));
                }
                _ => {}
            }
            if c.alive && pts.iter().any(|p| !point_alive(*p)) {
                return Err(bad("Eine Kurve verweist auf einen gelöschten Punkt"));
            }
        }
        for c in &d.constraints {
            if c.branch < -1 || c.branch > 1 {
                return Err(bad("Ungültige Lage einer Abhängigkeit"));
            }
            if let Some(e) = c.extra
                && !var_ok(e)
            {
                return Err(bad("Eine Abhängigkeit verweist auf einen fehlenden Wert"));
            }
            let pts = c.kind.points();
            let curves = c.kind.curves();
            if pts.iter().any(|p| !point_ok(*p)) || curves.iter().any(|k| !curve_ok(*k)) {
                return Err(bad("Eine Abhängigkeit verweist auf ein fehlendes Objekt"));
            }
            if !constraint_numbers_ok(&c.kind) {
                return Err(bad("Eine Abhängigkeit hat einen ungültigen Zahlenwert"));
            }
            if c.alive
                && (pts.iter().any(|p| !point_alive(*p)) || curves.iter().any(|k| !curve_alive(*k)))
            {
                return Err(bad("Eine Abhängigkeit verweist auf ein gelöschtes Objekt"));
            }
        }

        let mut points = Arena::new();
        for p in d.points {
            points.push(PointData { x: p.x, y: p.y, construction: p.construction, alive: p.alive });
        }
        let mut curves = Arena::new();
        for c in d.curves {
            curves.push(CurveData { kind: c.kind, construction: c.construction, alive: c.alive });
        }
        let mut constraints = Arena::new();
        for c in d.constraints {
            constraints.push(ConstraintData {
                kind: c.kind, branch: c.branch, driven: c.driven, enabled: c.enabled,
                internal: c.internal, extra: c.extra, alive: c.alive,
            });
        }
        let mut s = Sketch::new(d.frame, d.name, d.tolerance);
        s.vars = d.vars;
        s.points = points;
        s.curves = curves;
        s.constraints = constraints;
        s.solve_cache = SolveCache::default();
        s.touch_structure();
        Ok(s)
    }
}

/// Alle Zahlen einer Abhängigkeit endlich und im Bereich (Winkel und Maße).
fn constraint_numbers_ok(k: &ConstraintKind) -> bool {
    use ConstraintKind::*;
    let ok = |v: f64| v.is_finite() && v.abs() <= MAX_COORD;
    match k {
        Fixed(_, x, y) => ok(*x) && ok(*y),
        Distance(_, _, v) | DistancePointLine(_, _, v) | Radius(_, v) | Diameter(_, v)
        | Angle(_, _, v) | EllipseMajor(_, v) | EllipseMinor(_, v) | EllipseAngle(_, v) => ok(*v),
        Horizontal(LineOrPoints::Line(_)) | Horizontal(LineOrPoints::Points(..)) => true,
        _ => true,
    }
}
```

In `lib.rs`: `pub mod data;` und `pub use data::{ConstraintRecord, CurveRecord, PointRecord, SketchData};`. `PointId`/`CurveId` kommen aus `crate::model`; `finite`, `MAX_COORD`, `MAX_SPLINE_POINTS` sind dort `pub(crate)`/`pub`. `Frame::is_orthonormal` existiert (wird in `set_frame` benutzt). `DVec3::is_finite`/`abs().max_element()` sind glam-Methoden. Fehlen Sichtbarkeiten (`SolveCache` in `solver`, Felder von `ConstraintData`), nur auf `pub(crate)` erweitern.

- [ ] **Step 4: Tests grün**

Run: `cargo test -p planora-sketch --test data && cargo test -p planora-sketch`
Expected: PASS, alle bisherigen Skizzen-Tests unverändert grün.

- [ ] **Step 5: fmt, clippy, Commit**

```bash
cargo fmt && cargo clippy -p planora-sketch --all-targets -- -D warnings
git add -A && git commit -m "feat: Sketch::to_data und from_data (Rohdaten mit Prüfung)"
```

---

### Task 3: Dokument-Snapshot und Änderungszähler (`planora-feature`)

**Files:**
- Create: `crates/planora-feature/src/snapshot.rs`
- Modify: `crates/planora-feature/src/{lib,model,ops,recompute}.rs`
- Test: `crates/planora-feature/tests/snapshot.rs`

**Interfaces:**
- Consumes: `Document`, `Feature`, `FeatureKind`, `Document::check_references` (vorhanden, `pub(crate)`), `Document::recompute`.
- Produces:
  ```rust
  #[derive(Clone, Debug)] pub struct SnapshotFeature { pub id: FeatureId, pub name: String, pub suppressed: bool, pub kind: FeatureKind }
  #[derive(Clone, Debug)] pub struct Snapshot { pub features: Vec<SnapshotFeature>, pub marker: usize, pub next_id: u64 }
  impl Document {
      pub fn snapshot(&self) -> Snapshot;
      pub fn from_snapshot(s: Snapshot, chord_tolerance: f64) -> Result<Document, FeatureError>;
      pub fn edit_count(&self) -> u64;   // steigt bei jeder öffentlichen Änderung
  }
  ```
  `from_snapshot` prüft: Ids ≠ 0, eindeutig, `< next_id`; Namen nicht leer; `marker <= features.len()`; alle Verweise zeigen auf frühere Features (`check_references`); danach `recompute()` und `edit_count() == 0`. Bei einem Fehler entsteht kein Dokument.

- [ ] **Step 1: Tests schreiben** (`crates/planora-feature/tests/snapshot.rs`)

```rust
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;

fn sample() -> Document {
    let mut d = Document::new();
    let a = d.add_feature("A", FeatureKind::Box { size: DVec3::new(10.0, 10.0, 10.0), origin: DVec3::ZERO }).unwrap();
    let b = d.add_feature("B", FeatureKind::Cylinder { radius: 3.0, height: 12.0, origin: DVec3::new(5.0, 5.0, -1.0) }).unwrap();
    d.add_feature("C", FeatureKind::Boolean {
        target: BodyRef { feature: a, index: 0 }, tool: BodyRef { feature: b, index: 0 }, op: BoolOp::Difference,
    }).unwrap();
    d
}

fn shape(d: &Document) -> Vec<(String, bool, Option<usize>)> {
    d.features().iter().map(|f| (f.name.clone(), f.suppressed, None)).collect()
}

#[test]
fn a_snapshot_rebuilds_an_equivalent_document() {
    let mut d = sample();
    let idx = d.features()[1].id;
    d.set_suppressed(idx, true).unwrap();
    d.set_marker(3).unwrap();
    let s = d.snapshot();
    let e = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(shape(&d), shape(&e));
    assert_eq!(d.marker(), e.marker());
    assert_eq!(e.edit_count(), 0);
    e.check_invariants().unwrap();
    for (x, y) in d.features().iter().zip(e.features()) {
        assert_eq!(x.id, y.id);
        assert_eq!(x.status, y.status);
    }
    // neue Ids setzen die alte Zählung fort
    let mut e = e;
    let id = e.add_feature("D", FeatureKind::Box { size: DVec3::ONE, origin: DVec3::ZERO }).unwrap();
    assert!(d.features().iter().all(|f| f.id != id));
}

#[test]
fn every_public_edit_raises_the_edit_count() {
    let mut d = sample();
    let a = d.features()[0].id;
    let mut last = d.edit_count();
    let mut bumped = |d: &Document, what: &str| {
        let now = d.edit_count();
        assert!(now > last, "{what} muss den Zähler erhöhen");
        last = now;
    };
    d.rename(a, "A2").unwrap();
    bumped(&d, "rename");
    d.update(a, |k| if let FeatureKind::Box { size, .. } = k { size.x = 11.0 }).unwrap();
    bumped(&d, "update");
    d.set_suppressed(a, true).unwrap();
    bumped(&d, "set_suppressed");
    d.set_marker(1).unwrap();
    bumped(&d, "set_marker");
    d.set_marker(3).unwrap();
    d.add_feature("N", FeatureKind::Box { size: DVec3::ONE, origin: DVec3::ZERO }).unwrap();
    bumped(&d, "add_feature");
    let n = d.features().last().unwrap().id;
    d.delete(n).unwrap();
    bumped(&d, "delete");
    let c = d.features()[2].id;
    d.delete_with_dependents(c).unwrap();
    bumped(&d, "delete_with_dependents");
    // keine Änderung → kein Anstieg
    let before = d.edit_count();
    d.set_marker(d.marker()).unwrap();
    d.recompute();
    assert_eq!(d.edit_count(), before);
}

fn snap_with(f: impl FnOnce(&mut Snapshot)) -> FeatureError {
    let mut s = sample().snapshot();
    f(&mut s);
    Document::from_snapshot(s, 0.05).unwrap_err()
}

#[test]
fn inconsistent_snapshots_are_rejected() {
    snap_with(|s| s.features[1].id = s.features[0].id);
    snap_with(|s| s.features[0].id = FeatureId(0));
    snap_with(|s| s.next_id = 2);
    snap_with(|s| s.marker = 99);
    snap_with(|s| s.features[0].name = "  ".into());
    // Boolesche Operation verweist auf ein späteres Feature
    snap_with(|s| s.features.swap(0, 2));
    snap_with(|s| if let FeatureKind::Boolean { target, .. } = &mut s.features[2].kind {
        target.feature = FeatureId(77);
    });
}

#[test]
fn a_snapshot_with_huge_numbers_loads_with_an_error_status_not_a_panic() {
    let mut s = sample().snapshot();
    s.features[0].kind = FeatureKind::Box { size: DVec3::splat(1e300), origin: DVec3::ZERO };
    let d = Document::from_snapshot(s, 0.05).unwrap();
    assert!(matches!(d.features()[0].status, FeatureStatus::Error(_)));
    d.check_invariants().unwrap();
}
```

- [ ] **Step 2: Fehlschlag prüfen**

Run: `cargo test -p planora-feature --test snapshot` → FAIL (Typen fehlen).

- [ ] **Step 3: Implementierung**

In `model.rs` dem `Document` das Feld `pub(crate) edits: u64,` geben (in `with_options` mit `0` initialisieren) und

```rust
    /// Zähler aller öffentlichen Änderungen am Dokument (Hinzufügen, Ändern, Umbenennen,
    /// Löschen, Unterdrücken, Marker). Die Oberfläche erkennt daran ungespeicherte Änderungen.
    pub fn edit_count(&self) -> u64 {
        self.edits
    }
```

Erhöhen (`self.edits += 1;`) an diesen Stellen, jeweils **erst nach** erfolgreicher Prüfung und nur bei tatsächlicher Änderung:
- `add_feature` (`recompute.rs`) nach `self.features.insert`;
- `update_with_report` (`ops.rs`) nach dem Setzen von `feature.kind`;
- `rename` (`ops.rs`) nach dem Setzen des Namens (nur wenn der neue Name vom alten abweicht);
- `delete` und `delete_with_dependents` (`ops.rs`) nach dem Entfernen;
- `set_suppressed_with_report` (`recompute.rs`) nur wenn sich der Wert ändert;
- `set_marker` (`recompute.rs`) nur wenn sich der Marker ändert.
`recompute()` allein ändert den Zähler nicht.

`crates/planora-feature/src/snapshot.rs`:

```rust
//! Reine Daten eines Dokuments zum Speichern und Laden (ohne Ergebnisse und Zwischenspeicher).

use crate::error::FeatureError;
use crate::model::{Document, Feature, FeatureId, FeatureKind, FeatureStatus};

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
}

impl Document {
    pub fn snapshot(&self) -> Snapshot {
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
        }
    }

    /// Baut ein Dokument aus einem Snapshot und berechnet es einmal. Prüft Ids, Namen, Marker
    /// und Verweise; ungültige Zahlen führen erst bei der Berechnung zu einem Fehlerstatus am
    /// betroffenen Feature (kein Panic, kein Abbruch des Ladens).
    pub fn from_snapshot(s: Snapshot, chord_tolerance: f64) -> Result<Document, FeatureError> {
        let bad = |m: &str| FeatureError::InvalidInput(m.to_string());
        if s.marker > s.features.len() {
            return Err(bad("Der Rückrollmarker liegt hinter dem letzten Feature"));
        }
        let mut doc = Document::with_options(chord_tolerance);
        for (i, f) in s.features.iter().enumerate() {
            if f.id.0 == 0 || f.id.0 >= s.next_id {
                return Err(bad("Ein Feature hat eine ungültige Kennung"));
            }
            if s.features[..i].iter().any(|g| g.id == f.id) {
                return Err(bad("Eine Feature-Kennung kommt doppelt vor"));
            }
            if f.name.trim().is_empty() {
                return Err(bad("Ein Feature hat keinen Namen"));
            }
        }
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

In `lib.rs`: `mod snapshot;` und `pub use snapshot::{Snapshot, SnapshotFeature};`. Falls `check_references` einen anderen Parameterweg hat (z. B. `&FeatureKind, usize`), dem Aufruf in `ops.rs` folgen; das Schleifen-Borrowing (`&doc.features[idx].kind` gegen `&doc`) ggf. über `let kind = doc.features[idx].kind.clone();` lösen.

- [ ] **Step 4: Tests grün**

Run: `cargo test -p planora-feature` → alle bisherigen und neuen Tests grün. Zusätzlich: `cargo test --workspace` (die App benutzt `Document`).

- [ ] **Step 5: fmt, clippy, Commit**

```bash
cargo fmt && cargo clippy -p planora-feature --all-targets -- -D warnings
git add -A && git commit -m "feat: Document::snapshot, from_snapshot und edit_count"
```

---

### Task 4: Dateiformat (Format-Typen und Umwandlung)

**Files:**
- Create: `crates/planora-project/src/{error.rs,format.rs,convert.rs}`
- Modify: `crates/planora-project/src/lib.rs`
- Create: `crates/planora-project/tests/data/format1.json` (erzeugt, dann eingecheckt)
- Test: `crates/planora-project/tests/format.rs`

**Interfaces:**
- Consumes: `planora_feature::{Snapshot, SnapshotFeature, FeatureKind, SketchFeature, SketchPlane, EdgeCopy, FaceRef, EdgeRef, BodyRef, StandardPlane, FeatureId}`, `planora_sketch::{Sketch, SketchData, …}`, `planora_kernel::BoolOp`.
- Produces:
  ```rust
  pub const FORMAT_VERSION: u32 = 1;
  pub enum ProjectError { Io{path,msg}, Corrupt{path,msg}, TooNew{path,found}, ... }   // error.rs, siehe unten
  pub fn encode_version(s: &Snapshot) -> Result<String, ProjectError>;               // JSON-Text einer Versionsdatei
  pub fn decode_version(text: &str, origin: &str) -> Result<Snapshot, ProjectError>; // origin = Dateiname für Meldungen
  ```
  `decode_version` prüft: `format <= FORMAT_VERSION` (sonst `TooNew`), alle Zahlen endlich (JSON liefert nie NaN, aber `1e999` wird von serde_json als Fehler abgelehnt – trotzdem prüfen), Listenlängen ≤ `MAX_LIST` (1 000 000), Beträge ≤ `MAX_COORD_MM`, und baut Skizzen über `Sketch::from_data` (deren Fehler werden mit Feature-Name in `Corrupt` verpackt). Es entstehen nie halbe Ergebnisse.

- [ ] **Step 1: Fehlertyp** (`error.rs`)

```rust
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("Dateifehler bei „{path}“: {msg}")]
    Io { path: String, msg: String },
    #[error("Die Datei „{path}“ ist beschädigt: {msg}")]
    Corrupt { path: String, msg: String },
    #[error("Die Datei „{path}“ wurde mit einer neueren Programmversion erstellt (Format {found}).")]
    TooNew { path: String, found: u32 },
    #[error("{0}")]
    InvalidName(String),
    #[error("Der Pfad liegt außerhalb des Projektordners.")]
    OutsideRoot,
    #[error("„{0}“ gibt es schon.")]
    Exists(String),
    #[error("„{0}“ wurde nicht gefunden.")]
    NotFound(String),
    #[error("Das Dokument wird gerade von einem anderen Programm benutzt (gesperrt seit {since}).")]
    Locked { since: String },
    #[error("{0}")]
    Invalid(String),
}

impl ProjectError {
    pub fn io(path: &std::path::Path, e: &std::io::Error) -> Self {
        ProjectError::Io { path: path.display().to_string(), msg: e.to_string() }
    }
    pub fn corrupt(path: impl Into<String>, msg: impl Into<String>) -> Self {
        ProjectError::Corrupt { path: path.into(), msg: msg.into() }
    }
}

pub type Result<T> = std::result::Result<T, ProjectError>;

#[allow(dead_code)]
fn _unused(_: PathBuf) {}
```
(Die letzte Hilfsfunktion nur behalten, falls `PathBuf` sonst unbenutzt ist; sonst Import und Funktion entfernen.)

- [ ] **Step 2: Format-Typen** (`format.rs`). Alle Namen deutsch, `#[serde(deny_unknown_fields)]` **nicht** setzen (Spec: unbekannte Felder in bekannten Versionen werden ignoriert).

```rust
//! Die Typen der Versionsdatei (Format 1). Unabhängig von den internen Strukturen.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct VersionFile {
    pub format: u32,
    pub marker: usize,
    pub naechste_id: u64,
    pub features: Vec<FeatureDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FeatureDto {
    pub id: u64,
    pub name: String,
    pub unterdrueckt: bool,
    pub art: KindDto,
}

pub type V3 = [f64; 3];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BodyRefDto {
    pub feature: u64,
    pub index: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FaceRefDto {
    pub herkunft: u64,
    pub rolle: String,
    pub hinweis: V3,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EdgeRefDto {
    pub a: FaceRefDto,
    pub b: FaceRefDto,
    pub hinweis: V3,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum PlaneDto {
    Standard { ebene: String },
    Flaeche { flaeche: FaceRefDto },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EdgeCopyDto {
    pub kurve: usize,
    pub kante: EdgeRefDto,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FrameDto {
    pub ursprung: V3,
    pub x: V3,
    pub y: V3,
    pub z: V3,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PointDto {
    pub x: usize,
    pub y: usize,
    pub konstruktion: bool,
    pub aktiv: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum CurveKindDto {
    Linie { a: usize, b: usize },
    Kreis { mitte: usize, radius: usize },
    Bogen { mitte: usize, start: usize, ende: usize },
    Ellipse { mitte: usize, ax: usize, ay: usize, klein: usize },
    Spline { punkte: Vec<usize> },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CurveDto {
    pub art: CurveKindDto,
    pub konstruktion: bool,
    pub aktiv: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum LineOrPointsDto {
    Linie { kurve: usize },
    Punkte { a: usize, b: usize },
}

/// Abhängigkeiten; Punkte und Kurven als Indizes, Maße als Zahlen.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum ConstraintKindDto {
    Deckungsgleich { a: usize, b: usize },
    PunktAufKurve { punkt: usize, kurve: usize },
    Waagerecht { ziel: LineOrPointsDto },
    Senkrecht { ziel: LineOrPointsDto },
    Parallel { a: usize, b: usize },
    Rechtwinklig { a: usize, b: usize },
    Tangential { a: usize, b: usize },
    Gleich { a: usize, b: usize },
    Fest { punkt: usize, x: f64, y: f64 },
    Symmetrisch { a: usize, b: usize, achse: usize },
    Mittelpunkt { punkt: usize, kurve: usize },
    Abstand { a: usize, b: usize, wert: f64 },
    AbstandPunktLinie { punkt: usize, kurve: usize, wert: f64 },
    Radius { kurve: usize, wert: f64 },
    Durchmesser { kurve: usize, wert: f64 },
    Winkel { a: usize, b: usize, wert: f64 },
    BogenRadius { kurve: usize },
    EllipseGross { kurve: usize, wert: f64 },
    EllipseKlein { kurve: usize, wert: f64 },
    EllipseWinkel { kurve: usize, wert: f64 },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConstraintDto {
    pub art: ConstraintKindDto,
    pub zweig: i8,
    pub gesteuert: bool,
    pub aktiv: bool,
    pub intern: bool,
    pub extra: Option<usize>,
    pub lebt: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SketchDto {
    pub name: String,
    pub ebene: FrameDto,
    pub linear: f64,
    pub winkel: f64,
    pub werte: Vec<f64>,
    pub punkte: Vec<PointDto>,
    pub kurven: Vec<CurveDto>,
    pub abhaengigkeiten: Vec<ConstraintDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SketchFeatureDto {
    pub skizze: SketchDto,
    pub lage: PlaneDto,
    pub kopien: Vec<EdgeCopyDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum KindDto {
    Skizze(Box<SketchFeatureDto>),
    Quader { groesse: V3, ursprung: V3 },
    Zylinder { radius: f64, hoehe: f64, ursprung: V3 },
    Kugel { radius: f64, ursprung: V3 },
    Kegel { radius: f64, hoehe: f64, ursprung: V3 },
    Extrusion { skizze: u64, profil: usize, hoehe: f64 },
    Rotation { skizze: u64, profil: usize, achse: usize },
    Boolesch { ziel: BodyRefDto, werkzeug: BodyRefDto, operation: String },
    Verschieben { koerper: BodyRefDto, versatz: V3 },
}
```

Hinweis zu `Skizze(Box<…>)`: serde unterstützt interne Tags für Newtype-Varianten mit Struct-Inhalt; wenn die Ableitung das nicht übersetzt, stattdessen die Felder von `SketchFeatureDto` direkt in die Variante schreiben (`Skizze { skizze, lage, kopien }`).

- [ ] **Step 3: Umwandlung** (`convert.rs`) – vollständig, in beide Richtungen, nur mit Prüfungen statt Panics:

```rust
//! Umwandlung zwischen Snapshot und den Format-Typen. `encode_version` schreibt nur, was sich
//! wieder lesen lässt; `decode_version` prüft alles, bevor etwas entsteht.

use crate::error::{ProjectError, Result};
use crate::format::*;
use planora_feature::{
    BodyRef, EdgeCopy, EdgeRef, FaceRef, FeatureId, FeatureKind, MAX_COORD_MM, SketchFeature,
    SketchPlane, Snapshot, SnapshotFeature, StandardPlane,
};
use planora_kernel::BoolOp;
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::{
    ConstraintKind, ConstraintRecord, CurveId, CurveKind, CurveRecord, LineOrPoints, PointId,
    PointRecord, Sketch, SketchData,
};

pub const FORMAT_VERSION: u32 = 1;
/// Längste erlaubte Liste (Features, Punkte, Kurven, Abhängigkeiten, Werte) in einer Datei.
const MAX_LIST: usize = 1_000_000;

fn v3(v: DVec3) -> V3 {
    [v.x, v.y, v.z]
}

fn num(origin: &str, what: &str, x: f64) -> Result<f64> {
    if x.is_finite() && x.abs() <= MAX_COORD_MM * 2.0 {
        Ok(x)
    } else {
        Err(ProjectError::corrupt(origin, format!("{what}: Zahl ungültig oder zu groß")))
    }
}

fn dv3(origin: &str, what: &str, v: V3) -> Result<DVec3> {
    Ok(DVec3::new(num(origin, what, v[0])?, num(origin, what, v[1])?, num(origin, what, v[2])?))
}

fn check_len(origin: &str, what: &str, n: usize) -> Result<()> {
    if n > MAX_LIST {
        Err(ProjectError::corrupt(origin, format!("{what}: zu viele Einträge")))
    } else {
        Ok(())
    }
}

// ---- Verweise ---------------------------------------------------------------------------

fn face_to(r: &FaceRef) -> FaceRefDto {
    FaceRefDto { herkunft: r.origin, rolle: r.role.clone(), hinweis: v3(r.hint) }
}

fn face_from(origin: &str, d: &FaceRefDto) -> Result<FaceRef> {
    Ok(FaceRef { origin: d.herkunft, role: d.rolle.clone(), hint: dv3(origin, "Hinweispunkt", d.hinweis)? })
}

fn edge_to(r: &EdgeRef) -> EdgeRefDto {
    EdgeRefDto { a: face_to(&r.a), b: face_to(&r.b), hinweis: v3(r.hint) }
}

fn edge_from(origin: &str, d: &EdgeRefDto) -> Result<EdgeRef> {
    Ok(EdgeRef { a: face_from(origin, &d.a)?, b: face_from(origin, &d.b)?, hint: dv3(origin, "Hinweispunkt", d.hinweis)? })
}

fn body_to(b: &BodyRef) -> BodyRefDto {
    BodyRefDto { feature: b.feature.0, index: b.index }
}

fn body_from(d: &BodyRefDto) -> BodyRef {
    BodyRef { feature: FeatureId(d.feature), index: d.index }
}

// ---- Skizze -----------------------------------------------------------------------------

fn frame_to(f: &Frame) -> FrameDto {
    FrameDto { ursprung: v3(f.origin), x: v3(f.x), y: v3(f.y), z: v3(f.z) }
}

fn lop_to(l: &LineOrPoints) -> LineOrPointsDto {
    match l {
        LineOrPoints::Line(c) => LineOrPointsDto::Linie { kurve: c.index() },
        LineOrPoints::Points(a, b) => LineOrPointsDto::Punkte { a: a.index(), b: b.index() },
    }
}

fn lop_from(d: &LineOrPointsDto) -> LineOrPoints {
    match d {
        LineOrPointsDto::Linie { kurve } => LineOrPoints::Line(CurveId::from_index(*kurve)),
        LineOrPointsDto::Punkte { a, b } => {
            LineOrPoints::Points(PointId::from_index(*a), PointId::from_index(*b))
        }
    }
}

fn curve_to(k: &CurveKind) -> CurveKindDto {
    match k {
        CurveKind::Line { a, b } => CurveKindDto::Linie { a: a.index(), b: b.index() },
        CurveKind::Circle { center, radius } => CurveKindDto::Kreis { mitte: center.index(), radius: *radius },
        CurveKind::Arc { center, start, end } => CurveKindDto::Bogen {
            mitte: center.index(), start: start.index(), ende: end.index(),
        },
        CurveKind::Ellipse { center, ax, ay, minor } => CurveKindDto::Ellipse {
            mitte: center.index(), ax: *ax, ay: *ay, klein: *minor,
        },
        CurveKind::Spline { points } => CurveKindDto::Spline { punkte: points.iter().map(|p| p.index()).collect() },
    }
}

fn curve_from(d: &CurveKindDto) -> CurveKind {
    let p = PointId::from_index;
    match d {
        CurveKindDto::Linie { a, b } => CurveKind::Line { a: p(*a), b: p(*b) },
        CurveKindDto::Kreis { mitte, radius } => CurveKind::Circle { center: p(*mitte), radius: *radius },
        CurveKindDto::Bogen { mitte, start, ende } => CurveKind::Arc { center: p(*mitte), start: p(*start), end: p(*ende) },
        CurveKindDto::Ellipse { mitte, ax, ay, klein } => CurveKind::Ellipse { center: p(*mitte), ax: *ax, ay: *ay, minor: *klein },
        CurveKindDto::Spline { punkte } => CurveKind::Spline { points: punkte.iter().map(|i| p(*i)).collect() },
    }
}

fn constraint_to(k: &ConstraintKind) -> ConstraintKindDto {
    use ConstraintKind as K;
    use ConstraintKindDto as D;
    let (p, c) = (|x: &PointId| x.index(), |x: &CurveId| x.index());
    match k {
        K::Coincident(a, b) => D::Deckungsgleich { a: p(a), b: p(b) },
        K::PointOnCurve(a, k) => D::PunktAufKurve { punkt: p(a), kurve: c(k) },
        K::Horizontal(l) => D::Waagerecht { ziel: lop_to(l) },
        K::Vertical(l) => D::Senkrecht { ziel: lop_to(l) },
        K::Parallel(a, b) => D::Parallel { a: c(a), b: c(b) },
        K::Perpendicular(a, b) => D::Rechtwinklig { a: c(a), b: c(b) },
        K::Tangent(a, b) => D::Tangential { a: c(a), b: c(b) },
        K::Equal(a, b) => D::Gleich { a: c(a), b: c(b) },
        K::Fixed(a, x, y) => D::Fest { punkt: p(a), x: *x, y: *y },
        K::Symmetric(a, b, axis) => D::Symmetrisch { a: p(a), b: p(b), achse: c(axis) },
        K::Midpoint(a, k) => D::Mittelpunkt { punkt: p(a), kurve: c(k) },
        K::Distance(a, b, v) => D::Abstand { a: p(a), b: p(b), wert: *v },
        K::DistancePointLine(a, k, v) => D::AbstandPunktLinie { punkt: p(a), kurve: c(k), wert: *v },
        K::Radius(k, v) => D::Radius { kurve: c(k), wert: *v },
        K::Diameter(k, v) => D::Durchmesser { kurve: c(k), wert: *v },
        K::Angle(a, b, v) => D::Winkel { a: c(a), b: c(b), wert: *v },
        K::ArcRadius(k) => D::BogenRadius { kurve: c(k) },
        K::EllipseMajor(k, v) => D::EllipseGross { kurve: c(k), wert: *v },
        K::EllipseMinor(k, v) => D::EllipseKlein { kurve: c(k), wert: *v },
        K::EllipseAngle(k, v) => D::EllipseWinkel { kurve: c(k), wert: *v },
    }
}

fn constraint_from(d: &ConstraintKindDto) -> ConstraintKind {
    use ConstraintKind as K;
    use ConstraintKindDto as D;
    let (p, c) = (PointId::from_index, CurveId::from_index);
    match d {
        D::Deckungsgleich { a, b } => K::Coincident(p(*a), p(*b)),
        D::PunktAufKurve { punkt, kurve } => K::PointOnCurve(p(*punkt), c(*kurve)),
        D::Waagerecht { ziel } => K::Horizontal(lop_from(ziel)),
        D::Senkrecht { ziel } => K::Vertical(lop_from(ziel)),
        D::Parallel { a, b } => K::Parallel(c(*a), c(*b)),
        D::Rechtwinklig { a, b } => K::Perpendicular(c(*a), c(*b)),
        D::Tangential { a, b } => K::Tangent(c(*a), c(*b)),
        D::Gleich { a, b } => K::Equal(c(*a), c(*b)),
        D::Fest { punkt, x, y } => K::Fixed(p(*punkt), *x, *y),
        D::Symmetrisch { a, b, achse } => K::Symmetric(p(*a), p(*b), c(*achse)),
        D::Mittelpunkt { punkt, kurve } => K::Midpoint(p(*punkt), c(*kurve)),
        D::Abstand { a, b, wert } => K::Distance(p(*a), p(*b), *wert),
        D::AbstandPunktLinie { punkt, kurve, wert } => K::DistancePointLine(p(*punkt), c(*kurve), *wert),
        D::Radius { kurve, wert } => K::Radius(c(*kurve), *wert),
        D::Durchmesser { kurve, wert } => K::Diameter(c(*kurve), *wert),
        D::Winkel { a, b, wert } => K::Angle(c(*a), c(*b), *wert),
        D::BogenRadius { kurve } => K::ArcRadius(c(*kurve)),
        D::EllipseGross { kurve, wert } => K::EllipseMajor(c(*kurve), *wert),
        D::EllipseKlein { kurve, wert } => K::EllipseMinor(c(*kurve), *wert),
        D::EllipseWinkel { kurve, wert } => K::EllipseAngle(c(*kurve), *wert),
    }
}

fn sketch_to(s: &Sketch) -> SketchDto {
    let d = s.to_data();
    SketchDto {
        name: d.name,
        ebene: frame_to(&d.frame),
        linear: d.tolerance.linear,
        winkel: d.tolerance.angular,
        werte: d.vars,
        punkte: d.points.iter().map(|p| PointDto { x: p.x, y: p.y, konstruktion: p.construction, aktiv: p.alive }).collect(),
        kurven: d.curves.iter().map(|c| CurveDto { art: curve_to(&c.kind), konstruktion: c.construction, aktiv: c.alive }).collect(),
        abhaengigkeiten: d.constraints.iter().map(|c| ConstraintDto {
            art: constraint_to(&c.kind), zweig: c.branch, gesteuert: c.driven,
            aktiv: c.enabled, intern: c.internal, extra: c.extra, lebt: c.alive,
        }).collect(),
    }
}

fn sketch_from(origin: &str, feature: &str, d: &SketchDto) -> Result<Sketch> {
    let ctx = |m: String| ProjectError::corrupt(origin, format!("Skizze „{feature}“: {m}"));
    check_len(origin, "Werte", d.werte.len())?;
    check_len(origin, "Punkte", d.punkte.len())?;
    check_len(origin, "Kurven", d.kurven.len())?;
    check_len(origin, "Abhängigkeiten", d.abhaengigkeiten.len())?;
    let data = SketchData {
        name: d.name.clone(),
        frame: Frame {
            origin: dv3(origin, "Ursprung", d.ebene.ursprung)?,
            x: dv3(origin, "x-Achse", d.ebene.x)?,
            y: dv3(origin, "y-Achse", d.ebene.y)?,
            z: dv3(origin, "z-Achse", d.ebene.z)?,
        },
        tolerance: Tolerance { linear: d.linear, angular: d.winkel },
        vars: d.werte.clone(),
        points: d.punkte.iter().map(|p| PointRecord { x: p.x, y: p.y, construction: p.konstruktion, alive: p.aktiv }).collect(),
        curves: d.kurven.iter().map(|c| CurveRecord { kind: curve_from(&c.art), construction: c.konstruktion, alive: c.aktiv }).collect(),
        constraints: d.abhaengigkeiten.iter().map(|c| ConstraintRecord {
            kind: constraint_from(&c.art), branch: c.zweig, driven: c.gesteuert,
            enabled: c.aktiv, internal: c.intern, extra: c.extra, alive: c.lebt,
        }).collect(),
    };
    Sketch::from_data(data).map_err(|e| ctx(e.to_string()))
}

// ---- Feature-Arten ----------------------------------------------------------------------

fn plane_name(p: StandardPlane) -> &'static str {
    match p { StandardPlane::XY => "xy", StandardPlane::XZ => "xz", StandardPlane::YZ => "yz" }
}

fn kind_to(k: &FeatureKind) -> KindDto {
    match k {
        FeatureKind::Sketch(s) => KindDto::Skizze(Box::new(SketchFeatureDto {
            skizze: sketch_to(&s.sketch),
            lage: match &s.plane {
                SketchPlane::Standard(p) => PlaneDto::Standard { ebene: plane_name(*p).into() },
                SketchPlane::OnFace(r) => PlaneDto::Flaeche { flaeche: face_to(r) },
            },
            kopien: s.copies.iter().map(|c| EdgeCopyDto { kurve: c.curve.index(), kante: edge_to(&c.edge) }).collect(),
        })),
        FeatureKind::Box { size, origin } => KindDto::Quader { groesse: v3(*size), ursprung: v3(*origin) },
        FeatureKind::Cylinder { radius, height, origin } => KindDto::Zylinder { radius: *radius, hoehe: *height, ursprung: v3(*origin) },
        FeatureKind::Sphere { radius, origin } => KindDto::Kugel { radius: *radius, ursprung: v3(*origin) },
        FeatureKind::Cone { radius, height, origin } => KindDto::Kegel { radius: *radius, hoehe: *height, ursprung: v3(*origin) },
        FeatureKind::Extrude { sketch, profile, height } => KindDto::Extrusion { skizze: sketch.0, profil: *profile, hoehe: *height },
        FeatureKind::Revolve { sketch, profile, axis } => KindDto::Rotation { skizze: sketch.0, profil: *profile, achse: axis.index() },
        FeatureKind::Boolean { target, tool, op } => KindDto::Boolesch {
            ziel: body_to(target), werkzeug: body_to(tool),
            operation: match op { BoolOp::Union => "vereinigen", BoolOp::Difference => "abziehen", BoolOp::Intersection => "schneiden" }.into(),
        },
        FeatureKind::Move { body, delta } => KindDto::Verschieben { koerper: body_to(body), versatz: v3(*delta) },
    }
}

fn kind_from(origin: &str, name: &str, d: &KindDto) -> Result<FeatureKind> {
    let n = |what: &str, x: f64| num(origin, &format!("„{name}“ {what}"), x);
    let v = |what: &str, x: V3| dv3(origin, &format!("„{name}“ {what}"), x);
    Ok(match d {
        KindDto::Skizze(s) => {
            check_len(origin, "Kantenkopien", s.kopien.len())?;
            FeatureKind::Sketch(SketchFeature {
                sketch: sketch_from(origin, name, &s.skizze)?,
                plane: match &s.lage {
                    PlaneDto::Standard { ebene } => SketchPlane::Standard(match ebene.as_str() {
                        "xy" => StandardPlane::XY,
                        "xz" => StandardPlane::XZ,
                        "yz" => StandardPlane::YZ,
                        other => return Err(ProjectError::corrupt(origin, format!("Unbekannte Ebene „{other}“"))),
                    }),
                    PlaneDto::Flaeche { flaeche } => SketchPlane::OnFace(face_from(origin, flaeche)?),
                },
                copies: s.kopien.iter().map(|c| Ok(EdgeCopy { curve: CurveId::from_index(c.kurve), edge: edge_from(origin, &c.kante)? }))
                    .collect::<Result<Vec<_>>>()?,
            })
        }
        KindDto::Quader { groesse, ursprung } => FeatureKind::Box { size: v("Größe", *groesse)?, origin: v("Ursprung", *ursprung)? },
        KindDto::Zylinder { radius, hoehe, ursprung } => FeatureKind::Cylinder { radius: n("Radius", *radius)?, height: n("Höhe", *hoehe)?, origin: v("Ursprung", *ursprung)? },
        KindDto::Kugel { radius, ursprung } => FeatureKind::Sphere { radius: n("Radius", *radius)?, origin: v("Ursprung", *ursprung)? },
        KindDto::Kegel { radius, hoehe, ursprung } => FeatureKind::Cone { radius: n("Radius", *radius)?, height: n("Höhe", *hoehe)?, origin: v("Ursprung", *ursprung)? },
        KindDto::Extrusion { skizze, profil, hoehe } => FeatureKind::Extrude { sketch: FeatureId(*skizze), profile: *profil, height: n("Höhe", *hoehe)? },
        KindDto::Rotation { skizze, profil, achse } => FeatureKind::Revolve { sketch: FeatureId(*skizze), profile: *profil, axis: CurveId::from_index(*achse) },
        KindDto::Boolesch { ziel, werkzeug, operation } => FeatureKind::Boolean {
            target: body_from(ziel), tool: body_from(werkzeug),
            op: match operation.as_str() {
                "vereinigen" => BoolOp::Union,
                "abziehen" => BoolOp::Difference,
                "schneiden" => BoolOp::Intersection,
                other => return Err(ProjectError::corrupt(origin, format!("Unbekannte Operation „{other}“"))),
            },
        },
        KindDto::Verschieben { koerper, versatz } => FeatureKind::Move { body: body_from(koerper), delta: v("Versatz", *versatz)? },
    })
}

// ---- Öffentliche Funktionen -------------------------------------------------------------

/// JSON-Text einer Versionsdatei. Enthält der Snapshot eine nicht endliche Zahl, ist das ein
/// Fehler (das Dokument wird dann nicht geschrieben).
pub fn encode_version(s: &Snapshot) -> Result<String> {
    let file = VersionFile {
        format: FORMAT_VERSION,
        marker: s.marker,
        naechste_id: s.next_id,
        features: s.features.iter().map(|f| FeatureDto {
            id: f.id.0, name: f.name.clone(), unterdrueckt: f.suppressed, art: kind_to(&f.kind),
        }).collect(),
    };
    let text = serde_json::to_string_pretty(&file)
        .map_err(|e| ProjectError::Invalid(format!("Das Dokument lässt sich nicht schreiben: {e}")))?;
    // Nur schreiben, was sich wieder lesen lässt (nicht endliche Zahlen werden von serde_json
    // als `null` geschrieben und fallen hier auf).
    decode_version(&text, "Ausgabe").map_err(|e| {
        ProjectError::Invalid(format!("Das Dokument enthält ungültige Werte und wird nicht gespeichert: {e}"))
    })?;
    Ok(text)
}

/// Liest eine Versionsdatei. `origin` ist der Dateiname für Meldungen.
pub fn decode_version(text: &str, origin: &str) -> Result<Snapshot> {
    // Erst nur das Format lesen, damit eine neuere Datei auch bei geänderter Struktur die
    // richtige Meldung bekommt.
    #[derive(serde::Deserialize)]
    struct Head { format: u32 }
    let head: Head = serde_json::from_str(text)
        .map_err(|e| ProjectError::corrupt(origin, e.to_string()))?;
    if head.format > FORMAT_VERSION {
        return Err(ProjectError::TooNew { path: origin.into(), found: head.format });
    }
    let file: VersionFile = serde_json::from_str(text)
        .map_err(|e| ProjectError::corrupt(origin, e.to_string()))?;
    check_len(origin, "Features", file.features.len())?;
    let mut features = Vec::with_capacity(file.features.len());
    for f in &file.features {
        features.push(SnapshotFeature {
            id: FeatureId(f.id),
            name: f.name.clone(),
            suppressed: f.unterdrueckt,
            kind: kind_from(origin, &f.name, &f.art)?,
        });
    }
    Ok(Snapshot { features, marker: file.marker, next_id: file.naechste_id })
}
```

`lib.rs`:

```rust
//! Dateiformat, Projektverwaltung und Versionsspeicher von PlanoraCAD (ohne Oberfläche).
pub mod convert;
pub mod error;
pub mod format;

pub use convert::{FORMAT_VERSION, decode_version, encode_version};
pub use error::{ProjectError, Result};
```

`planora_sketch::{PointId, CurveId, ConstraintRecord, …}` müssen exportiert sein (Task 2 exportiert die Records). `MAX_COORD_MM` kommt aus `planora_feature`. Der Feldzugriff `Snapshot { .. }` und `SnapshotFeature { .. }` ist öffentlich (Task 3).

- [ ] **Step 4: Tests** (`crates/planora-project/tests/format.rs`)

```rust
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::{DVec2, DVec3};
use planora_project::*;

/// Dokument mit allen Feature-Arten: Skizze auf Grundebene, Extrusion, Rotation, Skizze auf
/// einer Fläche mit Kantenkopie, Quader, Zylinder, Kugel, Kegel, Boolesch, Verschieben.
fn full_document() -> Document {
    let mut d = Document::new();
    let b = d.add_feature("Quader 1", FeatureKind::Box { size: DVec3::new(30.0, 20.0, 10.0), origin: DVec3::ZERO }).unwrap();
    let cyl = d.add_feature("Zylinder 1", FeatureKind::Cylinder { radius: 4.0, height: 14.0, origin: DVec3::new(10.0, 10.0, -2.0) }).unwrap();
    d.add_feature("Kugel 1", FeatureKind::Sphere { radius: 3.0, origin: DVec3::new(50.0, 0.0, 0.0) }).unwrap();
    d.add_feature("Kegel 1", FeatureKind::Cone { radius: 3.0, height: 6.0, origin: DVec3::new(60.0, 0.0, 0.0) }).unwrap();
    let diff = d.add_feature("Abzug", FeatureKind::Boolean {
        target: BodyRef { feature: b, index: 0 }, tool: BodyRef { feature: cyl, index: 0 }, op: BoolOp::Difference,
    }).unwrap();
    d.add_feature("Weg", FeatureKind::Move { body: BodyRef { feature: diff, index: 0 }, delta: DVec3::new(0.0, 0.0, 5.0) }).unwrap();
    // Skizze + Extrusion
    let mut sk = planora_sketch::Sketch::new(StandardPlane::XY.frame(), "Skizze 1", Default::default());
    sk.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(8.0, 6.0)).unwrap();
    let sid = d.add_feature("Skizze 1", FeatureKind::Sketch(SketchFeature {
        sketch: sk, plane: SketchPlane::Standard(StandardPlane::XY), copies: vec![] })).unwrap();
    d.add_feature("Extrusion 1", FeatureKind::Extrude { sketch: sid, profile: 0, height: 4.0 }).unwrap();
    d
}

#[test]
fn the_round_trip_keeps_structure_values_and_results() {
    let d = full_document();
    d.check_invariants().unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    let back = decode_version(&text, "test").unwrap();
    let e = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(d.features().len(), e.features().len());
    for (a, b) in d.features().iter().zip(e.features()) {
        assert_eq!((&a.name, a.id, a.suppressed, &a.status), (&b.name, b.id, b.suppressed, &b.status));
        assert_eq!(a.kind.label(), b.kind.label());
    }
    // zweiter Schreibvorgang ergibt denselben Text (stabiles Format)
    assert_eq!(text, encode_version(&e.snapshot()).unwrap());
}

#[test]
fn the_golden_file_pins_format_1() {
    let text = encode_version(&full_document().snapshot()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/format1.json");
    if std::env::var("PLANORA_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let golden = std::fs::read_to_string(&path).expect("tests/data/format1.json fehlt – mit PLANORA_UPDATE_GOLDEN=1 erzeugen");
    // Die Golden-Datei muss sich (a) fehlerfrei lesen lassen und (b) zum selben Text führen.
    let snap = decode_version(&golden, "format1.json").unwrap();
    assert_eq!(encode_version(&snap).unwrap(), golden.trim_end_matches('\n'));
    let doc = Document::from_snapshot(snap, 0.05).unwrap();
    doc.check_invariants().unwrap();
    assert_eq!(doc.features().len(), 8);
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

#[test]
fn damaged_and_unfit_files_give_errors_not_panics() {
    let good = encode_version(&full_document().snapshot()).unwrap();
    assert!(matches!(bad(""), ProjectError::Corrupt { .. }));
    assert!(matches!(bad("{"), ProjectError::Corrupt { .. }));
    assert!(matches!(bad(&good[..good.len() / 2]), ProjectError::Corrupt { .. }));
    assert!(matches!(bad("[]"), ProjectError::Corrupt { .. }));
    assert!(matches!(bad(r#"{"format":99,"marker":0,"naechste_id":1,"features":[]}"#), ProjectError::TooNew { found: 99, .. }));
    assert!(matches!(bad(r#"{"format":1,"marker":"x","naechste_id":1,"features":[]}"#), ProjectError::Corrupt { .. }));
    // Zahl zu groß
    let huge = good.replacen("30.0", "1e300", 1);
    assert!(matches!(bad(&huge), ProjectError::Corrupt { .. }));
    // unbekannte Operation
    assert!(matches!(bad(&good.replace("abziehen", "teilen")), ProjectError::Corrupt { .. }));
    // unbekannte Felder werden ignoriert
    let extra = good.replacen("\"marker\"", "\"zukunft\": 1,\n  \"marker\"", 1);
    decode_version(&extra, "x.json").unwrap();
}

#[test]
fn a_decoded_but_inconsistent_snapshot_is_rejected_by_the_document() {
    let good = encode_version(&full_document().snapshot()).unwrap();
    // Marker hinter dem Ende
    let s = decode_version(&good.replacen("\"marker\": 8", "\"marker\": 80", 1), "x").unwrap();
    assert!(Document::from_snapshot(s, 0.05).is_err());
}

#[test]
fn a_non_finite_number_is_never_written() {
    let mut s = full_document().snapshot();
    if let FeatureKind::Box { size, .. } = &mut s.features[0].kind { size.x = f64::NAN; }
    assert!(encode_version(&s).is_err());
}
```

Das letzte Verhalten liefert `serde_json` nicht von selbst (nicht endliche Zahlen schreibt es als `null`). Deshalb prüft `encode_version` sein Ergebnis selbst: Nach dem Schreiben des Textes wird er mit `decode_version(&text, \"Ausgabe\")` gelesen (ohne Neuberechnung); schlägt das fehl, ist das ein Fehler und es wird nichts geschrieben. So wird nur geschrieben, was sich wieder lesen lässt.

Erzeugen der Golden-Datei: `PLANORA_UPDATE_GOLDEN=1 cargo test -p planora-project --test format the_golden_file` einmal ausführen, `tests/data/format1.json` einchecken, danach ohne die Variable erneut testen. `assert_eq!(doc.features().len(), 8)` entsprechend der Zahl der Features in `full_document` (8 Features plus Skizze und Extrusion = 8 Einträge; bei Abweichung die Zahl im Test auf die tatsächliche Zahl der Features von `full_document` setzen).

- [ ] **Step 5: Tests grün, fmt, clippy, Commit**

```bash
cargo test -p planora-project && cargo fmt && cargo clippy -p planora-project --all-targets -- -D warnings
git add -A && git commit -m "feat: Dateiformat 1 (Versionsdatei) mit Rundlauf und Golden-Datei"
```

---

### Task 5: Dateisystem-Hilfen (`fsutil`)

**Files:**
- Create: `crates/planora-project/src/fsutil.rs`
- Modify: `crates/planora-project/src/lib.rs`
- Test: `crates/planora-project/tests/fsutil.rs`

**Interfaces:**
- Consumes: `ProjectError`, `Result`.
- Produces (alle `pub`):
  ```rust
  pub const MAX_NAME_CHARS: usize = 80;
  pub fn validate_name(name: &str) -> Result<String>;                 // getrimmt und geprüft
  pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()>;       // .tmp, sync_all, rename
  pub fn new_id() -> String;                                           // 32 Hex-Zeichen, eindeutig
  pub fn now_iso() -> String;                                          // "2026-10-10T14:03:09Z"
  pub struct Root { /* kanonischer Pfad */ }
  impl Root { pub fn open(path: &Path) -> Result<Root>;                // legt an, falls fehlt
              pub fn path(&self) -> &Path;
              pub fn resolve(&self, parts: &[String]) -> Result<PathBuf>;   // geprüft, innerhalb der Wurzel
              pub fn to_trash(&self, parts: &[String]) -> Result<()>; }     // nach .papierkorb
  pub struct DocLock { /* gibt die Sperre beim Drop frei */ }
  impl DocLock { pub fn acquire(dir: &Path) -> Result<DocLock>;       // Err(Locked) wenn vorhanden
                 pub fn take_over(dir: &Path) -> Result<DocLock>; }
  pub fn lock_age(dir: &Path) -> Option<String>;                       // Zeitstempel der vorhandenen Sperre
  ```

- [ ] **Step 1: Tests** (`crates/planora-project/tests/fsutil.rs`)

```rust
use planora_project::fsutil::*;
use planora_project::ProjectError;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("planora-test-{name}-{}", new_id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn names_are_trimmed_and_checked() {
    assert_eq!(validate_name("  Gehäuse 2  ").unwrap(), "Gehäuse 2");
    for bad in ["", "   ", ".", "..", "a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b",
        "CON", "con", "NUL", "COM1", "lpt9", "AUX.txt", "name.", ".versteckt", "x\u{0}y",
        &"a".repeat(MAX_NAME_CHARS + 1), "x\ny"] {
        assert!(matches!(validate_name(bad), Err(ProjectError::InvalidName(_))), "{bad:?} muss abgelehnt werden");
    }
    assert!(validate_name(&"ä".repeat(MAX_NAME_CHARS)).is_ok());
}

#[test]
fn atomic_write_replaces_the_file_and_leaves_no_temp_file() {
    let d = tmp("aw");
    let f = d.join("a.json");
    atomic_write(&f, b"eins").unwrap();
    atomic_write(&f, b"zwei").unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"zwei");
    let names: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

#[test]
fn ids_are_unique_and_times_look_like_iso() {
    let ids: std::collections::HashSet<_> = (0..1000).map(|_| new_id()).collect();
    assert_eq!(ids.len(), 1000);
    assert!(ids.iter().all(|i| i.len() == 32 && i.chars().all(|c| c.is_ascii_hexdigit())));
    let t = now_iso();
    assert_eq!(t.len(), 20, "{t}");
    assert!(t.ends_with('Z') && &t[4..5] == "-" && &t[10..11] == "T", "{t}");
}

#[test]
fn resolve_stays_inside_the_root() {
    let d = tmp("root");
    let r = Root::open(&d).unwrap();
    let ok = r.resolve(&["Projekt".into(), "Ordner".into()]).unwrap();
    assert!(ok.starts_with(r.path()));
    for parts in [
        vec!["..".to_string()],
        vec!["a".to_string(), "..".to_string(), "..".to_string()],
        vec!["C:\\Windows".to_string()],
        vec!["/etc".to_string()],
        vec!["CON".to_string()],
        vec![String::new()],
    ] {
        assert!(r.resolve(&parts).is_err(), "{parts:?}");
    }
}

#[test]
fn a_link_leading_out_of_the_root_is_refused() {
    let d = tmp("link");
    let outside = tmp("outside");
    let r = Root::open(&d).unwrap();
    // Symlink bzw. Junction nach außen anlegen; ist das auf diesem System nicht erlaubt
    // (Windows ohne Rechte), wird der Test übersprungen.
    let link = d.join("Aussen");
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&outside, &link).is_ok();
    #[cfg(windows)]
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"]).arg(&link).arg(&outside)
        .output().map(|o| o.status.success()).unwrap_or(false);
    if !made {
        eprintln!("Verknüpfung nicht anlegbar – Test übersprungen");
        return;
    }
    assert!(r.resolve(&["Aussen".into(), "x".into()]).is_err());
}

#[test]
fn trash_moves_instead_of_deleting() {
    let d = tmp("trash");
    let r = Root::open(&d).unwrap();
    let p = r.resolve(&["Projekt".into()]).unwrap();
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join("datei.txt"), "x").unwrap();
    r.to_trash(&["Projekt".into()]).unwrap();
    assert!(!p.exists());
    let trash = r.path().join(".papierkorb");
    let entry = std::fs::read_dir(&trash).unwrap().next().unwrap().unwrap();
    assert!(entry.file_name().to_string_lossy().contains("Projekt"));
    assert!(entry.path().join("datei.txt").exists());
    // zweimal dasselbe löschen kollidiert nicht
    std::fs::create_dir_all(&p).unwrap();
    r.to_trash(&["Projekt".into()]).unwrap();
    assert_eq!(std::fs::read_dir(&trash).unwrap().count(), 2);
    assert!(r.to_trash(&[".papierkorb".into()]).is_err());
}

#[test]
fn a_lock_blocks_a_second_holder_until_it_is_dropped_or_taken_over() {
    let d = tmp("lock");
    let a = DocLock::acquire(&d).unwrap();
    assert!(matches!(DocLock::acquire(&d), Err(ProjectError::Locked { .. })));
    assert!(lock_age(&d).is_some());
    let b = DocLock::take_over(&d).unwrap();
    drop(a);
    // die übernommene Sperre bleibt gültig, auch wenn der alte Halter losließ? Nein: `a` löscht
    // beim Drop nur, was ihm gehört; hier gilt: nach dem Drop beider ist die Sperre weg.
    drop(b);
    assert!(lock_age(&d).is_none());
    let _c = DocLock::acquire(&d).unwrap();
}
```

- [ ] **Step 2: Fehlschlag prüfen** — `cargo test -p planora-project --test fsutil` → FAIL (Modul fehlt).

- [ ] **Step 3: Implementierung** (`fsutil.rs`)

```rust
//! Dateisystem-Hilfen: Namen prüfen, atomar schreiben, Wurzel mit Pfadprüfung, Papierkorb, Sperre.

use crate::error::{ProjectError, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Längster erlaubter Name eines Projekts, Ordners oder Dokuments (in Zeichen).
pub const MAX_NAME_CHARS: usize = 80;
/// Versuche beim Umbenennen (Virenscanner und Indexdienste halten Dateien kurz offen).
const RENAME_TRIES: u32 = 5;
const RENAME_WAIT: Duration = Duration::from_millis(40);
/// Name des Papierkorbs unter der Wurzel.
pub const TRASH: &str = ".papierkorb";
const LOCK_FILE: &str = ".sperre";

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn invalid(msg: &str) -> ProjectError {
    ProjectError::InvalidName(msg.to_string())
}

/// Prüft einen Namen und gibt ihn getrimmt zurück.
pub fn validate_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(invalid("Der Name darf nicht leer sein."));
    }
    if n.chars().count() > MAX_NAME_CHARS {
        return Err(invalid("Der Name ist zu lang (höchstens 80 Zeichen)."));
    }
    if n.starts_with('.') || n.ends_with('.') {
        return Err(invalid("Der Name darf nicht mit einem Punkt beginnen oder enden."));
    }
    if n.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c)) {
        return Err(invalid("Der Name enthält ein unzulässiges Zeichen (/ \\ : * ? \" < > |)."));
    }
    let stem = n.split('.').next().unwrap_or(n).to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(invalid("Dieser Name ist vom Betriebssystem reserviert."));
    }
    Ok(n.to_string())
}

/// Schreibt `bytes` atomar: erst in `<Name>.tmp` im selben Ordner, `sync_all`, dann umbenennen.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = fs::File::create(&tmp).map_err(|e| ProjectError::io(&tmp, &e))?;
        f.write_all(bytes).map_err(|e| ProjectError::io(&tmp, &e))?;
        f.sync_all().map_err(|e| ProjectError::io(&tmp, &e))?;
    }
    rename_with_retries(&tmp, path)
}

fn rename_with_retries(from: &Path, to: &Path) -> Result<()> {
    let mut last = None;
    for _ in 0..RENAME_TRIES {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(RENAME_WAIT);
            }
        }
    }
    let _ = fs::remove_file(from);
    Err(ProjectError::io(to, &last.expect("mindestens ein Versuch")))
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 32 Hexzeichen aus Zeit, Prozess-Id, Zähler und Adresse eines Stapelwerts (keine Sicherheits-
/// funktion, nur Eindeutigkeit).
pub fn new_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let marker = 0u8;
    let addr = &marker as *const u8 as usize as u128;
    let hi = (nanos as u64) ^ (u64::from(std::process::id()) << 32);
    let lo = (n << 20) ^ (addr as u64) ^ (nanos >> 64) as u64;
    format!("{hi:016x}{lo:016x}")
}

/// Aktuelle Zeit (UTC) als „JJJJ-MM-TTThh:mm:ssZ“.
pub fn now_iso() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    iso_from_unix(secs)
}

/// Bürgerliches Datum aus Tagen seit 1970-01-01 (Algorithmus von Howard Hinnant).
fn iso_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Die Wurzel aller Projekte. Jeder Pfad, der gelesen oder geschrieben wird, geht durch
/// [`Root::resolve`].
#[derive(Clone, Debug)]
pub struct Root {
    path: PathBuf,
}

impl Root {
    /// Öffnet die Wurzel (legt sie an, falls sie fehlt) und merkt ihren kanonischen Pfad.
    pub fn open(path: &Path) -> Result<Root> {
        fs::create_dir_all(path).map_err(|e| ProjectError::io(path, &e))?;
        let path = fs::canonicalize(path).map_err(|e| ProjectError::io(path, &e))?;
        Ok(Root { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Pfad zu `parts` (je Teil ein gültiger Name). Verweist ein vorhandener Teil des Pfades
    /// (auch über Symlink oder Junction) aus der Wurzel hinaus, ist das ein Fehler.
    pub fn resolve(&self, parts: &[String]) -> Result<PathBuf> {
        let mut p = self.path.clone();
        for part in parts {
            // Für Dokumente ist die Endung „.pcad“ Teil des Namens; geprüft wird der Teil ohne
            // diese Endung.
            let name = part.strip_suffix(".pcad").unwrap_or(part);
            validate_name(name)?;
            p.push(part);
        }
        // Der längste vorhandene Vorfahre muss innerhalb der Wurzel liegen.
        let mut probe = p.clone();
        while !probe.exists() {
            if !probe.pop() {
                return Err(ProjectError::OutsideRoot);
            }
        }
        let real = fs::canonicalize(&probe).map_err(|e| ProjectError::io(&probe, &e))?;
        if !real.starts_with(&self.path) {
            return Err(ProjectError::OutsideRoot);
        }
        Ok(p)
    }

    /// Verschiebt `parts` in den Papierkorb unter der Wurzel (Name mit Zeitstempel und Id, damit
    /// nichts kollidiert). Der Papierkorb selbst lässt sich nicht löschen.
    pub fn to_trash(&self, parts: &[String]) -> Result<()> {
        if parts.is_empty() || parts[0] == TRASH {
            return Err(ProjectError::OutsideRoot);
        }
        let from = self.resolve(parts)?;
        if !from.exists() {
            return Err(ProjectError::NotFound(parts.join("/")));
        }
        let trash = self.path.join(TRASH);
        fs::create_dir_all(&trash).map_err(|e| ProjectError::io(&trash, &e))?;
        let stamp = now_iso().replace(':', "-");
        let name = format!("{stamp}-{}-{}", &new_id()[..8], parts.last().expect("nicht leer"));
        rename_with_retries(&from, &trash.join(name))
    }
}

/// Sperre eines Dokumentordners; wird beim Drop freigegeben.
#[derive(Debug)]
pub struct DocLock {
    file: PathBuf,
    token: String,
}

impl DocLock {
    pub fn acquire(dir: &Path) -> Result<DocLock> {
        let file = dir.join(LOCK_FILE);
        let token = new_id();
        match fs::OpenOptions::new().write(true).create_new(true).open(&file) {
            Ok(mut f) => {
                let _ = writeln!(f, "{token}\n{}\n{}", now_iso(), std::process::id());
                Ok(DocLock { file, token })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(ProjectError::Locked { since: lock_age(dir).unwrap_or_else(|| "unbekannt".into()) })
            }
            Err(e) => Err(ProjectError::io(&file, &e)),
        }
    }

    /// Übernimmt eine vorhandene Sperre (nach Rückfrage des Nutzers).
    pub fn take_over(dir: &Path) -> Result<DocLock> {
        let file = dir.join(LOCK_FILE);
        let _ = fs::remove_file(&file);
        DocLock::acquire(dir)
    }
}

impl Drop for DocLock {
    fn drop(&mut self) {
        // Nur löschen, was uns gehört (eine übernommene Sperre trägt ein anderes Zeichen).
        if let Ok(text) = fs::read_to_string(&self.file)
            && text.lines().next() == Some(self.token.as_str())
        {
            let _ = fs::remove_file(&self.file);
        }
    }
}

/// Zeitstempel der vorhandenen Sperre (zweite Zeile der Sperrdatei).
pub fn lock_age(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join(LOCK_FILE)).ok()?;
    text.lines().nth(1).map(str::to_string)
}
```

`lib.rs` ergänzen: `pub mod fsutil;`. Im Test `a_lock_blocks…` gilt: `take_over` löscht die Sperre von `a` und legt eine neue (`b`) an; `drop(a)` löscht nicht (anderes Zeichen), `drop(b)` löscht → danach `lock_age == None`. Der Kommentar im Test dort darf vereinfacht werden, die Aussagen bleiben.

- [ ] **Step 4: Tests grün, fmt, clippy, Commit**

```bash
cargo test -p planora-project --test fsutil && cargo fmt && cargo clippy -p planora-project --all-targets -- -D warnings
git add -A && git commit -m "feat: Dateisystem-Hilfen (Namen, atomares Schreiben, Wurzel, Papierkorb, Sperre)"
```

---

### Task 6: Projektbaum (`Library`)

**Files:**
- Create: `crates/planora-project/src/library.rs`, `crates/planora-project/src/config.rs`
- Modify: `crates/planora-project/src/lib.rs`
- Test: `crates/planora-project/tests/library.rs`

**Interfaces:**
- Consumes: `Root`, `validate_name`, `atomic_write`, `new_id`, `ProjectError`.
- Produces:
  ```rust
  pub type NodePath = Vec<String>;                 // Teile ab der Wurzel; Dokumente tragen ".pcad"
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum NodeKind { Project, Folder, Document }
  #[derive(Clone, Debug)] pub struct TreeNode { pub name: String, pub kind: NodeKind, pub path: NodePath, pub children: Vec<TreeNode> }
  pub struct Library { root: Root }
  impl Library {
      pub fn open(root: &Path) -> Result<Library>;
      pub fn root(&self) -> &Root;
      pub fn tree(&self) -> Result<Vec<TreeNode>>;                         // nur Projekte oben, sortiert nach Name
      pub fn create_project(&self, name: &str) -> Result<NodePath>;
      pub fn create_folder(&self, parent: &NodePath, name: &str) -> Result<NodePath>;
      pub fn rename(&self, path: &NodePath, new_name: &str) -> Result<NodePath>;
      pub fn move_node(&self, path: &NodePath, new_parent: &NodePath) -> Result<NodePath>;
      pub fn delete(&self, path: &NodePath) -> Result<()>;                  // in den Papierkorb
      pub fn document_dir(&self, path: &NodePath) -> Result<PathBuf>;
      pub fn is_container(&self, path: &NodePath) -> bool;                  // Projekt oder Ordner (nicht Dokument, nicht Wurzel)
  }
  pub struct Config { pub root: Option<PathBuf> }                           // config.rs
  impl Config { pub fn load() -> Config; pub fn save(&self) -> Result<()>;
                pub fn load_from(path: &Path) -> Config; pub fn save_to(&self, path: &Path) -> Result<()>;
                pub fn default_root() -> PathBuf; pub fn config_path() -> PathBuf; }
  ```
  Regeln: Projekte nur direkt unter der Wurzel (Ordner mit `projekt.json`), Ordner nur in Projekt oder Ordner, Dokumente (Ordner `*.pcad`, innen `dokument.json`) nur in Projekt oder Ordner; ein Dokument wird hier nicht angelegt (das macht Task 7), aber umbenannt/verschoben/gelöscht. Beim Verschieben darf das Ziel kein Nachfahre des Quellknotens sein; Namenskollision im Ziel → `Exists`; Umbenennen eines Dokuments behält `.pcad`.

- [ ] **Step 1: Tests** (`crates/planora-project/tests/library.rs`)

```rust
use planora_project::fsutil::new_id;
use planora_project::library::*;
use planora_project::ProjectError;
use std::path::PathBuf;

fn lib() -> (Library, PathBuf) {
    let p = std::env::temp_dir().join(format!("planora-lib-{}", new_id()));
    (Library::open(&p).unwrap(), p)
}

fn names(nodes: &[TreeNode]) -> Vec<String> {
    nodes.iter().map(|n| n.name.clone()).collect()
}

/// Legt von Hand ein Dokument an (die echte Anlage macht der Versionsspeicher, Task 7).
fn fake_doc(l: &Library, parent: &NodePath, name: &str) -> NodePath {
    let mut path = parent.clone();
    path.push(format!("{name}.pcad"));
    let dir = l.root().resolve(&path).unwrap();
    std::fs::create_dir_all(dir.join("versionen")).unwrap();
    std::fs::write(dir.join("dokument.json"), "{}").unwrap();
    path
}

#[test]
fn projects_and_folders_form_a_sorted_tree() {
    let (l, _) = lib();
    let b = l.create_project("B-Projekt").unwrap();
    let a = l.create_project("a-Projekt").unwrap();
    let f = l.create_folder(&a, "Deckel").unwrap();
    let g = l.create_folder(&f, "Innen").unwrap();
    fake_doc(&l, &g, "Teil");
    fake_doc(&l, &b, "Roh");
    let t = l.tree().unwrap();
    assert_eq!(names(&t), ["a-Projekt", "B-Projekt"]);
    assert_eq!(t[0].kind, NodeKind::Project);
    assert_eq!(t[0].children[0].name, "Deckel");
    assert_eq!(t[0].children[0].children[0].children[0].kind, NodeKind::Document);
    assert_eq!(t[0].children[0].children[0].children[0].name, "Teil");
    assert_eq!(t[1].children[0].name, "Roh");
}

#[test]
fn names_and_places_are_checked() {
    let (l, _) = lib();
    let p = l.create_project("P").unwrap();
    assert!(matches!(l.create_project("P"), Err(ProjectError::Exists(_))));
    assert!(matches!(l.create_project(".."), Err(ProjectError::InvalidName(_))));
    assert!(l.create_folder(&vec![], "X").is_err(), "Ordner nur in Projekten");
    let f = l.create_folder(&p, "F").unwrap();
    assert!(matches!(l.create_folder(&p, "F"), Err(ProjectError::Exists(_))));
    let d = fake_doc(&l, &f, "D");
    assert!(l.create_folder(&d, "In-Dokument").is_err(), "nicht in einem Dokument");
    assert!(l.create_project("Neu").is_ok());
    assert!(l.is_container(&p) && l.is_container(&f) && !l.is_container(&d) && !l.is_container(&vec![]));
}

#[test]
fn rename_and_move_keep_content_and_reject_collisions_and_loops() {
    let (l, _) = lib();
    let p = l.create_project("P").unwrap();
    let a = l.create_folder(&p, "A").unwrap();
    let b = l.create_folder(&p, "B").unwrap();
    let d = fake_doc(&l, &a, "Teil");
    let d2 = l.rename(&d, "Teil neu").unwrap();
    assert_eq!(d2.last().unwrap(), "Teil neu.pcad");
    let d3 = l.move_node(&d2, &b).unwrap();
    assert_eq!(d3, vec!["P".to_string(), "B".to_string(), "Teil neu.pcad".to_string()]);
    assert!(l.document_dir(&d3).unwrap().join("dokument.json").exists());
    // Kollision
    fake_doc(&l, &a, "Teil neu");
    assert!(matches!(l.move_node(&vec!["P".into(), "A".into(), "Teil neu.pcad".into()], &b), Err(ProjectError::Exists(_))));
    // Ordner in sich selbst oder in einen Nachfahren
    let c = l.create_folder(&a, "C").unwrap();
    assert!(l.move_node(&a, &c).is_err());
    assert!(l.move_node(&a, &a).is_err());
    // Projekte lassen sich nicht in Projekte verschieben
    let q = l.create_project("Q").unwrap();
    assert!(l.move_node(&q, &p).is_err());
    // Umbenennen mit schlechtem Namen
    assert!(l.rename(&a, "a/b").is_err());
}

#[test]
fn deleting_moves_to_the_trash_and_hides_the_node() {
    let (l, root) = lib();
    let p = l.create_project("P").unwrap();
    let d = fake_doc(&l, &p, "D");
    l.delete(&d).unwrap();
    assert!(l.tree().unwrap()[0].children.is_empty());
    assert!(root.canonicalize().unwrap().join(".papierkorb").exists());
    l.delete(&p).unwrap();
    assert!(l.tree().unwrap().is_empty());
    assert!(l.delete(&vec![]).is_err());
    assert!(l.delete(&vec!["Gibt es nicht".into()]).is_err());
}

#[test]
fn the_tree_ignores_foreign_files_and_the_trash() {
    let (l, root) = lib();
    std::fs::write(root.join("notiz.txt"), "x").unwrap();
    std::fs::create_dir_all(root.join("Kein Projekt")).unwrap();
    std::fs::create_dir_all(root.join(".papierkorb")).unwrap();
    l.create_project("Echt").unwrap();
    assert_eq!(names(&l.tree().unwrap()), ["Echt"]);
}

#[test]
fn config_round_trips_and_survives_garbage() {
    use planora_project::config::Config;
    let p = std::env::temp_dir().join(format!("planora-cfg-{}.json", new_id()));
    assert!(Config::load_from(&p).root.is_none());
    let c = Config { root: Some(PathBuf::from("D:/Projekte")) };
    c.save_to(&p).unwrap();
    assert_eq!(Config::load_from(&p).root, c.root);
    std::fs::write(&p, "kaputt{").unwrap();
    assert!(Config::load_from(&p).root.is_none());
}
```

- [ ] **Step 2: Fehlschlag prüfen** → FAIL (Module fehlen).

- [ ] **Step 3: Implementierung** (`library.rs`, `config.rs`)

```rust
//! Projektbaum auf der Festplatte: Projekte → Ordner → Dokumente.

use crate::error::{ProjectError, Result};
use crate::fsutil::{Root, TRASH, atomic_write, new_id, validate_name};
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "projekt.json";
pub const DOCUMENT_FILE: &str = "dokument.json";
pub const DOCUMENT_SUFFIX: &str = ".pcad";

pub type NodePath = Vec<String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Project,
    Folder,
    Document,
}

#[derive(Clone, Debug)]
pub struct TreeNode {
    /// Anzeigename (bei Dokumenten ohne `.pcad`).
    pub name: String,
    pub kind: NodeKind,
    pub path: NodePath,
    pub children: Vec<TreeNode>,
}

pub struct Library {
    root: Root,
}

fn display_name(part: &str) -> String {
    part.strip_suffix(DOCUMENT_SUFFIX).unwrap_or(part).to_string()
}

impl Library {
    pub fn open(root: &Path) -> Result<Library> {
        Ok(Library { root: Root::open(root)? })
    }

    pub fn root(&self) -> &Root {
        &self.root
    }

    pub fn document_dir(&self, path: &NodePath) -> Result<PathBuf> {
        let dir = self.root.resolve(path)?;
        if path.last().is_some_and(|p| p.ends_with(DOCUMENT_SUFFIX)) && dir.join(DOCUMENT_FILE).is_file() {
            Ok(dir)
        } else {
            Err(ProjectError::NotFound(path.join("/")))
        }
    }

    fn kind_of(&self, path: &NodePath) -> Result<NodeKind> {
        let dir = self.root.resolve(path)?;
        if !dir.is_dir() {
            return Err(ProjectError::NotFound(path.join("/")));
        }
        if path.last().is_some_and(|p| p.ends_with(DOCUMENT_SUFFIX)) && dir.join(DOCUMENT_FILE).is_file() {
            Ok(NodeKind::Document)
        } else if path.len() == 1 && dir.join(PROJECT_FILE).is_file() {
            Ok(NodeKind::Project)
        } else if path.len() > 1 {
            Ok(NodeKind::Folder)
        } else {
            Err(ProjectError::Invalid("Das ist kein Projekt.".into()))
        }
    }

    pub fn tree(&self) -> Result<Vec<TreeNode>> {
        let mut out = Vec::new();
        for e in fs::read_dir(self.root.path()).map_err(|e| ProjectError::io(self.root.path(), &e))? {
            let e = e.map_err(|e| ProjectError::io(self.root.path(), &e))?;
            let name = e.file_name().to_string_lossy().to_string();
            if name == TRASH || !e.path().join(PROJECT_FILE).is_file() {
                continue;
            }
            out.push(self.node(vec![name], NodeKind::Project)?);
        }
        sort_nodes(&mut out);
        Ok(out)
    }

    fn node(&self, path: NodePath, kind: NodeKind) -> Result<TreeNode> {
        let mut children = Vec::new();
        if kind != NodeKind::Document {
            let dir = self.root.resolve(&path)?;
            for e in fs::read_dir(&dir).map_err(|e| ProjectError::io(&dir, &e))? {
                let e = e.map_err(|e| ProjectError::io(&dir, &e))?;
                if !e.path().is_dir() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let mut p = path.clone();
                p.push(name.clone());
                let k = if name.ends_with(DOCUMENT_SUFFIX) && e.path().join(DOCUMENT_FILE).is_file() {
                    NodeKind::Document
                } else if name.ends_with(DOCUMENT_SUFFIX) {
                    continue; // beschädigter Dokumentordner ohne dokument.json
                } else {
                    NodeKind::Folder
                };
                children.push(self.node(p, k)?);
            }
            sort_nodes(&mut children);
        }
        Ok(TreeNode { name: display_name(path.last().expect("nicht leer")), kind, path, children })
    }

    pub fn create_project(&self, name: &str) -> Result<NodePath> {
        let name = validate_name(name)?;
        let path = vec![name.clone()];
        let dir = self.root.resolve(&path)?;
        if dir.exists() {
            return Err(ProjectError::Exists(name));
        }
        fs::create_dir_all(&dir).map_err(|e| ProjectError::io(&dir, &e))?;
        let text = format!("{{\n  \"format\": 1,\n  \"id\": \"{}\",\n  \"name\": {}\n}}\n", new_id(), json_string(&name));
        atomic_write(&dir.join(PROJECT_FILE), text.as_bytes())?;
        Ok(path)
    }

    pub fn create_folder(&self, parent: &NodePath, name: &str) -> Result<NodePath> {
        if self.kind_of(parent)? == NodeKind::Document {
            return Err(ProjectError::Invalid("In einem Dokument lassen sich keine Ordner anlegen.".into()));
        }
        let name = validate_name(name)?;
        if name.ends_with(DOCUMENT_SUFFIX) {
            return Err(ProjectError::InvalidName("Ordnernamen dürfen nicht auf „.pcad“ enden.".into()));
        }
        let mut path = parent.clone();
        path.push(name.clone());
        let dir = self.root.resolve(&path)?;
        if dir.exists() {
            return Err(ProjectError::Exists(name));
        }
        fs::create_dir(&dir).map_err(|e| ProjectError::io(&dir, &e))?;
        Ok(path)
    }

    pub fn rename(&self, path: &NodePath, new_name: &str) -> Result<NodePath> {
        let kind = self.kind_of(path)?;
        let name = validate_name(new_name)?;
        let part = if kind == NodeKind::Document { format!("{name}{DOCUMENT_SUFFIX}") } else { name.clone() };
        if kind != NodeKind::Document && part.ends_with(DOCUMENT_SUFFIX) {
            return Err(ProjectError::InvalidName("Dieser Name ist nicht erlaubt.".into()));
        }
        let mut to = path.clone();
        *to.last_mut().expect("nicht leer") = part;
        if &to == path {
            return Ok(to);
        }
        self.rename_dir(path, &to)?;
        Ok(to)
    }

    pub fn move_node(&self, path: &NodePath, new_parent: &NodePath) -> Result<NodePath> {
        let kind = self.kind_of(path)?;
        if kind == NodeKind::Project {
            return Err(ProjectError::Invalid("Projekte lassen sich nicht verschieben.".into()));
        }
        if self.kind_of(new_parent)? == NodeKind::Document {
            return Err(ProjectError::Invalid("Das Ziel ist ein Dokument.".into()));
        }
        if new_parent.starts_with(path) {
            return Err(ProjectError::Invalid("Ein Ordner lässt sich nicht in sich selbst verschieben.".into()));
        }
        let mut to = new_parent.clone();
        to.push(path.last().expect("nicht leer").clone());
        if &to == path {
            return Ok(to);
        }
        self.rename_dir(path, &to)?;
        Ok(to)
    }

    fn rename_dir(&self, from: &NodePath, to: &NodePath) -> Result<()> {
        let a = self.root.resolve(from)?;
        let b = self.root.resolve(to)?;
        if b.exists() {
            return Err(ProjectError::Exists(display_name(to.last().expect("nicht leer"))));
        }
        fs::rename(&a, &b).map_err(|e| ProjectError::io(&b, &e))
    }

    /// Ist `path` ein Projekt oder Ordner (also ein Ort für Ordner und Dokumente)?
    pub fn is_container(&self, path: &NodePath) -> bool {
        matches!(self.kind_of(path), Ok(NodeKind::Project | NodeKind::Folder))
    }

    pub fn delete(&self, path: &NodePath) -> Result<()> {
        if path.is_empty() {
            return Err(ProjectError::Invalid("Nichts ausgewählt.".into()));
        }
        self.kind_of(path)?;
        self.root.to_trash(path)
    }
}

fn sort_nodes(v: &mut [TreeNode]) {
    v.sort_by_key(|n| n.name.to_lowercase());
}

/// Text als JSON-Zeichenkette (nur für die kleine Projektdatei).
fn json_string(s: &str) -> String {
    serde_json::to_string(s).expect("Text ist immer darstellbar")
}
```

`config.rs`:

```rust
//! Kleine Konfiguration (gewählte Wurzel), ohne neue Abhängigkeit.

use crate::error::{ProjectError, Result};
use crate::fsutil::atomic_write;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub root: Option<PathBuf>,
}

impl Config {
    /// `%APPDATA%\PlanoraCAD\config.json`, sonst `$XDG_CONFIG_HOME/planora-cad/config.json`,
    /// sonst `~/.config/planora-cad/config.json`.
    pub fn config_path() -> PathBuf {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("PlanoraCAD").join("config.json");
        }
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(x).join("planora-cad").join("config.json");
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        home.join(".config").join("planora-cad").join("config.json")
    }

    /// Vorgabe der Wurzel: `Dokumente\PlanoraCAD` im Benutzerordner.
    pub fn default_root() -> PathBuf {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default();
        home.join("Documents").join("PlanoraCAD")
    }

    pub fn load() -> Config {
        Config::load_from(&Config::config_path())
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Config::config_path())
    }

    /// Fehlende oder unlesbare Dateien ergeben die Vorgabe (kein Fehler).
    pub fn load_from(path: &Path) -> Config {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ProjectError::io(dir, &e))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        atomic_write(path, text.as_bytes())
    }
}
```

`lib.rs`: `pub mod config; pub mod library;`.

- [ ] **Step 4: Tests grün, fmt, clippy, Commit**

```bash
cargo test -p planora-project && cargo fmt && cargo clippy -p planora-project --all-targets -- -D warnings
git add -A && git commit -m "feat: Projektbaum (Library) und Konfiguration"
```

---

### Task 7: Versionsspeicher je Dokument (`DocStore`)

**Files:**
- Create: `crates/planora-project/src/docstore.rs`
- Modify: `crates/planora-project/src/{lib.rs,error.rs}` (Fehler `NeedsBranch`), `crates/planora-project/Cargo.toml` (nichts Neues)
- Test: `crates/planora-project/tests/docstore.rs`

**Interfaces:**
- Consumes: `encode_version`, `decode_version`, `atomic_write`, `now_iso`, `new_id`, `DocLock`, `Snapshot`, `DOCUMENT_FILE` (aus `library`).
- Produces:
  ```rust
  pub const MAIN_BRANCH: &str = "Haupt";
  pub const VERSIONS_DIR: &str = "versionen";
  pub const AUTOSAVE_FILE: &str = ".autosave.json";
  pub const MAX_BRANCH_CHARS: usize = 60;
  pub const MAX_COMMENT_CHARS: usize = 500;
  #[derive(Clone, Debug, PartialEq)] pub struct VersionInfo { pub number: u32, pub parent: Option<u32>, pub branch: String, pub time: String, pub comment: String, pub program: String }
  #[derive(Clone, Debug, PartialEq)] pub struct DocumentInfo { pub id: String, pub versions: Vec<VersionInfo>, pub branches: BTreeMap<String, u32>, pub active_branch: String }
  impl DocumentInfo { pub fn version(&self, n: u32) -> Option<&VersionInfo>; pub fn tip(&self, branch: &str) -> Option<u32>;
                      pub fn children(&self, n: u32) -> Vec<u32>; pub fn latest_time(&self) -> &str; pub fn check(&self) -> Result<(), String>; }
  pub enum LockMode { Acquire, TakeOver }
  pub struct OpenReport { pub orphans: Vec<String>, pub missing: Vec<u32> }
  pub struct Autosave { pub based_on: Option<u32>, pub time: String, pub snapshot: Snapshot }
  pub struct DocStore { /* dir, info, Sperre */ }
  impl DocStore {
      pub fn create(dir: &Path, snapshot: &Snapshot, comment: &str) -> Result<DocStore>;
      pub fn open(dir: &Path, mode: LockMode) -> Result<(DocStore, OpenReport)>;
      pub fn dir(&self) -> &Path;  pub fn info(&self) -> &DocumentInfo;
      pub fn load_version(&self, n: u32) -> Result<Snapshot>;
      pub fn latest_readable(&self, branch: &str) -> Option<u32>;      // Spitze oder nächste lesbare Vorfahrenversion
      pub fn save(&mut self, based_on: Option<u32>, snapshot: &Snapshot, comment: &str, new_branch: Option<&str>) -> Result<u32>;
      pub fn restore(&mut self, version: u32, onto_branch: &str) -> Result<u32>;
      pub fn set_comment(&mut self, n: u32, text: &str) -> Result<()>;
      pub fn delete_version(&mut self, n: u32) -> Result<()>;
      pub fn delete_branch(&mut self, name: &str) -> Result<()>;
      pub fn set_active_branch(&mut self, name: &str) -> Result<()>;
      pub fn write_autosave(&self, based_on: Option<u32>, snapshot: &Snapshot) -> Result<()>;
      pub fn read_autosave(&self) -> Result<Option<Autosave>>;
      pub fn discard_autosave(&self);
  }
  pub fn peek(dir: &Path) -> Result<(DocumentInfo, OpenReport)>;      // nur lesen, ohne Sperre, ändert nichts
  pub fn read_version(dir: &Path, n: u32) -> Result<Snapshot>;        // nur lesen, ohne Sperre
  ```
  Regeln (aus Spec §4): `save` hängt an die Spitze des Zweigs von `based_on` (`None` = aktiver Zweig); ist `based_on` nicht die Spitze ihres Zweigs, ist `new_branch` Pflicht (sonst `ProjectError::NeedsBranch`) und es entsteht dieser neue Zweig ab `based_on`; Zweignamen eindeutig, nicht leer, ≤ 60 Zeichen. Zuerst wird die Versionsdatei, dann `dokument.json` geschrieben; schlägt das Zweite fehl, bleibt der alte Stand gültig, die Datei bleibt verwaist, `self.info` ist unverändert. `delete_version`: nur Spitzen ohne Kinder, nicht die einzige Version. `delete_branch`: nicht der aktive Zweig; keine Version des Zweigs darf ein Kind außerhalb des Zweigs haben. Nach jeder Änderung gilt `DocumentInfo::check() == Ok`.

- [ ] **Step 1: Fehlervariante** in `error.rs` ergänzen:

```rust
    #[error("Diese Version ist nicht die neueste ihres Zweigs. Bitte einen neuen Zweignamen angeben.")]
    NeedsBranch,
```

- [ ] **Step 2: Tests** (`crates/planora-project/tests/docstore.rs`)

```rust
use planora_feature::{Document, FeatureKind, Snapshot};
use planora_math::DVec3;
use planora_project::docstore::*;
use planora_project::fsutil::new_id;
use planora_project::ProjectError;
use std::path::PathBuf;

fn dir() -> PathBuf {
    std::env::temp_dir().join(format!("planora-doc-{}.pcad", new_id()))
}

/// Dokument mit `n` Quadern; unterscheidbar durch die Zahl der Features.
fn snap(n: usize) -> Snapshot {
    let mut d = Document::new();
    for i in 0..n {
        d.add_feature(format!("Q{i}"), FeatureKind::Box { size: DVec3::splat(1.0 + i as f64), origin: DVec3::ZERO }).unwrap();
    }
    d.snapshot()
}

fn count(s: &Snapshot) -> usize {
    s.features.len()
}

#[test]
fn create_save_and_load_back_versions() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "Anfang").unwrap();
    assert_eq!(st.info().versions.len(), 1);
    assert_eq!(st.info().active_branch, MAIN_BRANCH);
    let v2 = st.save(Some(1), &snap(2), "Zwei", None).unwrap();
    let v3 = st.save(Some(v2), &snap(3), "", None).unwrap();
    assert_eq!((v2, v3), (2, 3));
    assert_eq!(st.info().tip(MAIN_BRANCH), Some(3));
    assert_eq!(count(&st.load_version(2).unwrap()), 2);
    st.info().check().unwrap();
    drop(st);
    let (st, report) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert!(report.orphans.is_empty() && report.missing.is_empty());
    assert_eq!(st.info().versions.len(), 3);
    assert_eq!(st.info().version(2).unwrap().comment, "Zwei");
    assert_eq!(count(&st.load_version(3).unwrap()), 3);
}

#[test]
fn saving_on_an_old_version_needs_a_branch_name() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", None).unwrap();
    st.save(Some(2), &snap(3), "", None).unwrap();
    assert!(matches!(st.save(Some(1), &snap(9), "", None), Err(ProjectError::NeedsBranch)));
    assert_eq!(st.info().versions.len(), 3, "nichts geändert");
    let v4 = st.save(Some(1), &snap(9), "Variante", Some("Variante A")).unwrap();
    assert_eq!(st.info().version(v4).unwrap().parent, Some(1));
    assert_eq!(st.info().version(v4).unwrap().branch, "Variante A");
    assert_eq!(st.info().tip("Variante A"), Some(v4));
    assert_eq!(st.info().tip(MAIN_BRANCH), Some(3));
    // Speichern auf der Spitze des Nebenzweigs hängt dort an
    let v5 = st.save(Some(v4), &snap(8), "", None).unwrap();
    assert_eq!(st.info().version(v5).unwrap().branch, "Variante A");
    for bad in ["", "   ", "Haupt", "Variante A", &"x".repeat(MAX_BRANCH_CHARS + 1), "a\u{0}b"] {
        assert!(st.save(Some(1), &snap(1), "", Some(bad)).is_err(), "{bad:?}");
    }
    st.info().check().unwrap();
}

#[test]
fn restore_appends_a_copy_to_the_branch_tip() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", None).unwrap();
    st.save(Some(2), &snap(3), "", None).unwrap();
    let v = st.restore(1, MAIN_BRANCH).unwrap();
    assert_eq!(v, 4);
    assert_eq!(st.info().version(4).unwrap().parent, Some(3));
    assert_eq!(st.info().version(4).unwrap().comment, "Wiederhergestellt aus v1");
    assert_eq!(count(&st.load_version(4).unwrap()), 1);
    assert!(st.restore(99, MAIN_BRANCH).is_err());
    assert!(st.restore(1, "Gibt es nicht").is_err());
}

#[test]
fn comments_and_active_branch() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", Some("B")).unwrap();
    st.set_comment(2, "  neu  \nzweite Zeile").unwrap();
    assert_eq!(st.info().version(2).unwrap().comment, "neu  \nzweite Zeile".trim_start());
    assert!(st.set_comment(2, &"x".repeat(MAX_COMMENT_CHARS + 1)).is_err());
    assert!(st.set_comment(77, "x").is_err());
    st.set_active_branch("B").unwrap();
    assert!(st.set_active_branch("Z").is_err());
    drop(st);
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(st.info().active_branch, "B");
    assert_eq!(st.latest_readable("B"), Some(2));
}

#[test]
fn deletion_rules() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    assert!(st.delete_version(1).is_err(), "die einzige Version bleibt");
    st.save(Some(1), &snap(2), "", None).unwrap(); // v2 Haupt
    st.save(Some(1), &snap(3), "", Some("B")).unwrap(); // v3 Zweig B ab v1
    st.save(Some(3), &snap(4), "", None).unwrap(); // v4 Zweig B
    assert!(st.delete_version(1).is_err(), "hat Kinder");
    assert!(st.delete_version(3).is_err(), "hat Kinder");
    st.delete_version(4).unwrap();
    assert_eq!(st.info().tip("B"), Some(3));
    assert!(st.delete_branch("Haupt").is_err(), "v1 hat ein Kind in Zweig B");
    assert!(st.delete_branch("Gibt es nicht").is_err());
    assert!(st.delete_branch("B").is_err(), "B ist der aktive Zweig");
    st.set_active_branch("Haupt").unwrap();
    st.delete_branch("B").unwrap();
    assert!(st.info().tip("B").is_none());
    assert_eq!(st.info().versions.len(), 2);
    st.info().check().unwrap();
    // Zweig mit Kind außerhalb darf nicht weg: v2 (Haupt) bekommt ein Kind in Zweig C
    st.save(Some(2), &snap(5), "", None).unwrap(); // Haupt
    st.save(Some(2), &snap(6), "", Some("C")).unwrap(); // Zweig C ab v2 (v2 ist nicht mehr Spitze)
    st.set_active_branch("C").unwrap();
    assert!(st.delete_branch("Haupt").is_err(), "v2 hat ein Kind in C");
    drop(st);
    let (st, report) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert!(report.orphans.is_empty(), "Löschen entfernt die Dateien: {:?}", report.orphans);
    st.info().check().unwrap();
}

#[test]
fn a_crash_between_the_files_leaves_a_valid_old_state_and_an_orphan() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    // dokument.json durch einen Ordner ersetzen: das Ersetzen schlägt fehl, wie bei einem Absturz
    let doc = p.join("dokument.json");
    let saved = std::fs::read(&doc).unwrap();
    std::fs::remove_file(&doc).unwrap();
    std::fs::create_dir(&doc).unwrap();
    assert!(st.save(Some(1), &snap(2), "", None).is_err());
    assert_eq!(st.info().versions.len(), 1, "Speicherstand im Arbeitsspeicher unverändert");
    std::fs::remove_dir(&doc).unwrap();
    std::fs::write(&doc, saved).unwrap();
    drop(st);
    let (st, report) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(st.info().versions.len(), 1);
    assert_eq!(report.orphans, ["0002.json"]);
    assert_eq!(count(&st.load_version(1).unwrap()), 1);
    // neue Versionsnummern kollidieren nicht mit der verwaisten Datei
    let mut st = st;
    assert_eq!(st.save(Some(1), &snap(3), "", None).unwrap(), 3);
}

#[test]
fn damaged_documents_give_errors_and_reports() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", None).unwrap();
    drop(st);
    let doc = p.join("dokument.json");
    let good = std::fs::read_to_string(&doc).unwrap();
    for bad in [
        String::new(),
        "{".to_string(),
        good.replace("\"format\": 1", "\"format\": 99"),
        good.replace("\"eltern\": 1", "\"eltern\": 7"),
        good.replace("\"Haupt\"", "\"\""),
    ] {
        std::fs::write(&doc, &bad).unwrap();
        assert!(DocStore::open(&p, LockMode::Acquire).is_err(), "{bad}");
    }
    std::fs::write(&doc, &good).unwrap();
    // fehlende und abgeschnittene Versionsdatei
    std::fs::remove_file(p.join("versionen/0002.json")).unwrap();
    let (st, report) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(report.missing, [2]);
    assert_eq!(st.latest_readable(MAIN_BRANCH), Some(1));
    assert!(st.load_version(2).is_err());
    drop(st);
    std::fs::write(p.join("versionen/0001.json"), "{\"format\":1,\"marker\":0").unwrap();
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert!(matches!(st.load_version(1), Err(ProjectError::Corrupt { .. })));
}

#[test]
fn a_second_opener_is_locked_out_until_taken_over() {
    let p = dir();
    let st = DocStore::create(&p, &snap(1), "").unwrap();
    assert!(matches!(DocStore::open(&p, LockMode::Acquire), Err(ProjectError::Locked { .. })));
    let (b, _) = DocStore::open(&p, LockMode::TakeOver).unwrap();
    drop(st);
    drop(b);
    assert!(DocStore::open(&p, LockMode::Acquire).is_ok());
}

#[test]
fn peek_and_read_version_work_while_the_document_is_locked() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", None).unwrap();
    let (info, report) = peek(&p).unwrap();
    assert_eq!(info.versions.len(), 2);
    assert!(report.orphans.is_empty() && report.missing.is_empty());
    assert_eq!(count(&read_version(&p, 2).unwrap()), 2);
    assert!(read_version(&p, 9).is_err());
    assert_eq!(st.info(), &info);
}

#[test]
fn autosave_round_trips_and_is_discarded() {
    let p = dir();
    let st = DocStore::create(&p, &snap(1), "").unwrap();
    assert!(st.read_autosave().unwrap().is_none());
    st.write_autosave(Some(1), &snap(4)).unwrap();
    let a = st.read_autosave().unwrap().unwrap();
    assert_eq!((a.based_on, count(&a.snapshot)), (Some(1), 4));
    assert!(a.time.as_str() >= st.info().latest_time());
    // es erzeugt keine Version
    assert_eq!(st.info().versions.len(), 1);
    st.discard_autosave();
    assert!(st.read_autosave().unwrap().is_none());
    std::fs::write(p.join(".autosave.json"), "kaputt").unwrap();
    assert!(st.read_autosave().is_err());
}

mod random {
    use super::*;
    use proptest::prelude::*;

    #[derive(Clone, Debug)]
    enum Op {
        Save(usize, Option<u8>),
        Restore(usize, usize),
        Comment(usize),
        DelVersion(usize),
        DelBranch(usize),
        Activate(usize),
        Reopen,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0usize..10, proptest::option::of(0u8..4)).prop_map(|(v, b)| Op::Save(v, b)),
            (0usize..10, 0usize..4).prop_map(|(v, b)| Op::Restore(v, b)),
            (0usize..10).prop_map(Op::Comment),
            (0usize..10).prop_map(Op::DelVersion),
            (0usize..4).prop_map(Op::DelBranch),
            (0usize..4).prop_map(Op::Activate),
            Just(Op::Reopen),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]
        #[test]
        fn random_operations_keep_the_version_tree_valid(ops in proptest::collection::vec(op(), 1..25)) {
            let p = dir();
            let mut st = DocStore::create(&p, &snap(1), "").unwrap();
            for o in ops {
                let nums: Vec<u32> = st.info().versions.iter().map(|v| v.number).collect();
                let branches: Vec<String> = st.info().branches.keys().cloned().collect();
                let pick = |i: usize| nums[i % nums.len()];
                let bpick = |i: usize| branches[i % branches.len()].clone();
                match o {
                    Op::Save(v, b) => { let _ = st.save(Some(pick(v)), &snap(2), "x", b.map(|n| format!("Z{n}")).as_deref()); }
                    Op::Restore(v, b) => { let _ = st.restore(pick(v), &bpick(b)); }
                    Op::Comment(v) => { let _ = st.set_comment(pick(v), "k"); }
                    Op::DelVersion(v) => { let _ = st.delete_version(pick(v)); }
                    Op::DelBranch(b) => { let _ = st.delete_branch(&bpick(b)); }
                    Op::Activate(b) => { let _ = st.set_active_branch(&bpick(b)); }
                    Op::Reopen => {
                        let before = st.info().clone();
                        drop(st);
                        let (s2, rep) = DocStore::open(&p, LockMode::Acquire).unwrap();
                        prop_assert_eq!(&before, s2.info());
                        prop_assert!(rep.missing.is_empty());
                        st = s2;
                    }
                }
                prop_assert!(st.info().check().is_ok(), "{:?}", st.info().check());
                for v in &st.info().versions {
                    prop_assert!(st.load_version(v.number).is_ok());
                }
            }
        }
    }
}
```

- [ ] **Step 3: Fehlschlag prüfen** → FAIL (Modul fehlt).

- [ ] **Step 4: Implementierung** (`docstore.rs`)

```rust
//! Versionsspeicher eines Dokuments: `dokument.json` (Versionsbaum) und `versionen/NNNN.json`.

use crate::convert::{FORMAT_VERSION, decode_version, encode_version};
use crate::error::{ProjectError, Result};
use crate::fsutil::{DocLock, atomic_write, new_id, now_iso};
use crate::library::DOCUMENT_FILE;
use planora_feature::Snapshot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAIN_BRANCH: &str = "Haupt";
pub const VERSIONS_DIR: &str = "versionen";
pub const AUTOSAVE_FILE: &str = ".autosave.json";
pub const MAX_BRANCH_CHARS: usize = 60;
pub const MAX_COMMENT_CHARS: usize = 500;
/// Mehr Versionen in einer `dokument.json` gelten als beschädigt.
const MAX_VERSIONS: usize = 100_000;
const PROGRAM: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq)]
pub struct VersionInfo {
    pub number: u32,
    pub parent: Option<u32>,
    pub branch: String,
    pub time: String,
    pub comment: String,
    pub program: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentInfo {
    pub id: String,
    pub versions: Vec<VersionInfo>,
    pub branches: BTreeMap<String, u32>,
    pub active_branch: String,
}

impl DocumentInfo {
    pub fn version(&self, n: u32) -> Option<&VersionInfo> {
        self.versions.iter().find(|v| v.number == n)
    }

    pub fn tip(&self, branch: &str) -> Option<u32> {
        self.branches.get(branch).copied()
    }

    pub fn children(&self, n: u32) -> Vec<u32> {
        self.versions.iter().filter(|v| v.parent == Some(n)).map(|v| v.number).collect()
    }

    /// Zeitpunkt der jüngsten Version (ISO-Texte sind als Text vergleichbar).
    pub fn latest_time(&self) -> &str {
        self.versions.iter().map(|v| v.time.as_str()).max().unwrap_or("")
    }

    /// Prüft den Versionsbaum: Nummern aufsteigend und eindeutig, genau eine Wurzel, Eltern
    /// früher und vorhanden, jeder Zweig nicht leer mit seiner jüngsten Version als Spitze,
    /// aktiver Zweig vorhanden.
    pub fn check(&self) -> std::result::Result<(), String> {
        if self.versions.is_empty() {
            return Err("Das Dokument hat keine Version".into());
        }
        let mut seen = BTreeSet::new();
        let mut last = 0;
        for (i, v) in self.versions.iter().enumerate() {
            if v.number == 0 || v.number <= last {
                return Err(format!("Versionsnummer {} ist doppelt oder nicht aufsteigend", v.number));
            }
            last = v.number;
            match v.parent {
                None if i != 0 => return Err(format!("Version {} hat keine Elternversion", v.number)),
                Some(p) if p >= v.number || !seen.contains(&p) => {
                    return Err(format!("Version {} verweist auf eine ungültige Elternversion {p}", v.number));
                }
                _ => {}
            }
            seen.insert(v.number);
            if !branch_name_ok(&v.branch) {
                return Err(format!("Version {} hat einen ungültigen Zweignamen", v.number));
            }
        }
        for (name, tip) in &self.branches {
            let newest = self.versions.iter().filter(|v| &v.branch == name).map(|v| v.number).max();
            if newest != Some(*tip) {
                return Err(format!("Die Spitze des Zweigs „{name}“ stimmt nicht"));
            }
        }
        if self.versions.iter().any(|v| !self.branches.contains_key(&v.branch)) {
            return Err("Eine Version gehört zu keinem Zweig".into());
        }
        if !self.branches.contains_key(&self.active_branch) {
            return Err("Der aktive Zweig gibt es nicht".into());
        }
        Ok(())
    }
}

/// Zweigname: getrimmt nicht leer, höchstens 60 Zeichen, keine Steuerzeichen.
pub fn branch_name_ok(n: &str) -> bool {
    !n.trim().is_empty() && n == n.trim() && n.chars().count() <= MAX_BRANCH_CHARS && !n.chars().any(char::is_control)
}

fn clean_comment(t: &str) -> Result<String> {
    let t: String = t.trim().chars().filter(|c| !c.is_control() || *c == '\n').collect();
    if t.chars().count() > MAX_COMMENT_CHARS {
        return Err(ProjectError::Invalid(format!("Der Kommentar ist zu lang (höchstens {MAX_COMMENT_CHARS} Zeichen).")));
    }
    Ok(t)
}

// ---- dokument.json ----------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct DocFile {
    format: u32,
    programm: String,
    id: String,
    versionen: Vec<VersionDto>,
    zweige: BTreeMap<String, u32>,
    aktiver_zweig: String,
}

#[derive(Serialize, Deserialize)]
struct VersionDto {
    nummer: u32,
    eltern: Option<u32>,
    zweig: String,
    zeit: String,
    kommentar: String,
    programm: String,
}

fn to_file(i: &DocumentInfo) -> DocFile {
    DocFile {
        format: FORMAT_VERSION,
        programm: PROGRAM.into(),
        id: i.id.clone(),
        versionen: i.versions.iter().map(|v| VersionDto {
            nummer: v.number, eltern: v.parent, zweig: v.branch.clone(), zeit: v.time.clone(),
            kommentar: v.comment.clone(), programm: v.program.clone(),
        }).collect(),
        zweige: i.branches.clone(),
        aktiver_zweig: i.active_branch.clone(),
    }
}

fn from_file(f: DocFile, origin: &str) -> Result<DocumentInfo> {
    if f.format > FORMAT_VERSION {
        return Err(ProjectError::TooNew { path: origin.into(), found: f.format });
    }
    if f.versionen.len() > MAX_VERSIONS {
        return Err(ProjectError::corrupt(origin, "zu viele Versionen"));
    }
    let info = DocumentInfo {
        id: f.id,
        versions: f.versionen.into_iter().map(|v| VersionInfo {
            number: v.nummer, parent: v.eltern, branch: v.zweig, time: v.zeit,
            comment: v.kommentar, program: v.programm,
        }).collect(),
        branches: f.zweige,
        active_branch: f.aktiver_zweig,
    };
    info.check().map_err(|m| ProjectError::corrupt(origin, m))?;
    Ok(info)
}

fn version_file(n: u32) -> String {
    format!("{n:04}.json")
}

/// Wie die Sperre beim Öffnen behandelt wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Acquire,
    TakeOver,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenReport {
    /// Dateien in `versionen/`, die `dokument.json` nicht kennt (Reste eines abgebrochenen Speicherns).
    pub orphans: Vec<String>,
    /// Versionen, die `dokument.json` nennt, deren Datei aber fehlt.
    pub missing: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Autosave {
    pub based_on: Option<u32>,
    pub time: String,
    pub snapshot: Snapshot,
}

#[derive(Serialize, Deserialize)]
struct AutosaveFile {
    format: u32,
    basiert_auf: Option<u32>,
    zeit: String,
    /// Versionsdatei als Text (derselbe Inhalt wie `versionen/NNNN.json`).
    inhalt: String,
}

/// Liest Versionsbaum und Befund, ohne Sperre und ohne etwas zu ändern (für schreibgeschützte
/// Ansichten und Listen).
pub fn peek(dir: &Path) -> Result<(DocumentInfo, OpenReport)> {
    let doc = dir.join(DOCUMENT_FILE);
    let origin = doc.display().to_string();
    let text = fs::read_to_string(&doc).map_err(|e| ProjectError::io(&doc, &e))?;
    let file: DocFile = serde_json::from_str(&text).map_err(|e| ProjectError::corrupt(origin.clone(), e.to_string()))?;
    let info = from_file(file, &origin)?;
    let mut report = OpenReport::default();
    let mut present = BTreeSet::new();
    if let Ok(rd) = fs::read_dir(dir.join(VERSIONS_DIR)) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".tmp") {
                continue;
            }
            match name.strip_suffix(".json").and_then(|n| n.parse::<u32>().ok()) {
                Some(n) if info.version(n).is_some() => {
                    present.insert(n);
                }
                _ => report.orphans.push(name),
            }
        }
    }
    report.orphans.sort();
    report.missing = info.versions.iter().map(|v| v.number).filter(|n| !present.contains(n)).collect();
    Ok((info, report))
}

/// Liest eine Version, ohne Sperre.
pub fn read_version(dir: &Path, n: u32) -> Result<Snapshot> {
    let (info, _) = peek(dir)?;
    if info.version(n).is_none() {
        return Err(ProjectError::NotFound(format!("Version {n}")));
    }
    let path = dir.join(VERSIONS_DIR).join(version_file(n));
    let text = fs::read_to_string(&path).map_err(|e| ProjectError::io(&path, &e))?;
    decode_version(&text, &path.display().to_string())
}

pub struct DocStore {
    dir: PathBuf,
    info: DocumentInfo,
    _lock: DocLock,
}

impl DocStore {
    /// Legt ein neues Dokument an. `dir` darf noch nicht existieren (Ordnername endet auf `.pcad`).
    pub fn create(dir: &Path, snapshot: &Snapshot, comment: &str) -> Result<DocStore> {
        if dir.exists() {
            return Err(ProjectError::Exists(dir.display().to_string()));
        }
        let comment = clean_comment(comment)?;
        let text = encode_version(snapshot)?;
        fs::create_dir_all(dir.join(VERSIONS_DIR)).map_err(|e| ProjectError::io(dir, &e))?;
        let lock = DocLock::acquire(dir)?;
        let info = DocumentInfo {
            id: new_id(),
            versions: vec![VersionInfo {
                number: 1, parent: None, branch: MAIN_BRANCH.into(), time: now_iso(),
                comment, program: PROGRAM.into(),
            }],
            branches: BTreeMap::from([(MAIN_BRANCH.to_string(), 1)]),
            active_branch: MAIN_BRANCH.into(),
        };
        atomic_write(&dir.join(VERSIONS_DIR).join(version_file(1)), text.as_bytes())?;
        let store = DocStore { dir: dir.to_path_buf(), info, _lock: lock };
        store.persist(&store.info)?;
        Ok(store)
    }

    pub fn open(dir: &Path, mode: LockMode) -> Result<(DocStore, OpenReport)> {
        let lock = match mode {
            LockMode::Acquire => DocLock::acquire(dir)?,
            LockMode::TakeOver => DocLock::take_over(dir)?,
        };
        let (info, report) = peek(dir)?;
        // Reste abgebrochener Schreibvorgänge entfernen (nur mit Sperre)
        if let Ok(rd) = fs::read_dir(dir.join(VERSIONS_DIR)) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().ends_with(".tmp") {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
        Ok((DocStore { dir: dir.to_path_buf(), info, _lock: lock }, report))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn info(&self) -> &DocumentInfo {
        &self.info
    }

    fn persist(&self, info: &DocumentInfo) -> Result<()> {
        let text = serde_json::to_string_pretty(&to_file(info)).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        atomic_write(&self.dir.join(DOCUMENT_FILE), text.as_bytes())
    }

    pub fn load_version(&self, n: u32) -> Result<Snapshot> {
        if self.info.version(n).is_none() {
            return Err(ProjectError::NotFound(format!("Version {n}")));
        }
        let path = self.dir.join(VERSIONS_DIR).join(version_file(n));
        let text = fs::read_to_string(&path).map_err(|e| ProjectError::io(&path, &e))?;
        decode_version(&text, &path.display().to_string())
    }

    /// Spitze des Zweigs oder, wenn deren Datei fehlt, die nächste Vorfahrenversion mit Datei.
    pub fn latest_readable(&self, branch: &str) -> Option<u32> {
        let mut cur = self.info.tip(branch);
        while let Some(n) = cur {
            if self.dir.join(VERSIONS_DIR).join(version_file(n)).is_file() {
                return Some(n);
            }
            cur = self.info.version(n).and_then(|v| v.parent);
        }
        None
    }

    fn next_number(&self) -> u32 {
        let mut max = self.info.versions.iter().map(|v| v.number).max().unwrap_or(0);
        if let Ok(rd) = fs::read_dir(self.dir.join(VERSIONS_DIR)) {
            for e in rd.flatten() {
                if let Some(n) = e.file_name().to_string_lossy().strip_suffix(".json").and_then(|n| n.parse::<u32>().ok()) {
                    max = max.max(n);
                }
            }
        }
        max + 1
    }

    pub fn save(&mut self, based_on: Option<u32>, snapshot: &Snapshot, comment: &str, new_branch: Option<&str>) -> Result<u32> {
        let comment = clean_comment(comment)?;
        let (parent, branch) = match based_on {
            None => {
                let b = self.info.active_branch.clone();
                (self.info.tip(&b), b)
            }
            Some(n) => {
                let v = self.info.version(n).ok_or_else(|| ProjectError::NotFound(format!("Version {n}")))?;
                match new_branch {
                    Some(name) => {
                        if !branch_name_ok(name) || self.info.branches.contains_key(name) {
                            return Err(ProjectError::InvalidName("Der Zweigname ist ungültig oder vergeben.".into()));
                        }
                        (Some(n), name.to_string())
                    }
                    None if self.info.tip(&v.branch) == Some(n) => (Some(n), v.branch.clone()),
                    None => return Err(ProjectError::NeedsBranch),
                }
            }
        };
        self.append(parent, branch, snapshot, comment)
    }

    pub fn restore(&mut self, version: u32, onto_branch: &str) -> Result<u32> {
        let tip = self.info.tip(onto_branch).ok_or_else(|| ProjectError::NotFound(format!("Zweig {onto_branch}")))?;
        let snapshot = self.load_version(version)?;
        self.append(Some(tip), onto_branch.to_string(), &snapshot, format!("Wiederhergestellt aus v{version}"))
    }

    fn append(&mut self, parent: Option<u32>, branch: String, snapshot: &Snapshot, comment: String) -> Result<u32> {
        let text = encode_version(snapshot)?;
        let number = self.next_number();
        let mut info = self.info.clone();
        info.versions.push(VersionInfo {
            number, parent, branch: branch.clone(), time: now_iso(), comment, program: PROGRAM.into(),
        });
        info.branches.insert(branch.clone(), number);
        info.active_branch = branch;
        info.check().map_err(ProjectError::Invalid)?;
        // erst die Versionsdatei, dann der Versionsbaum: bricht es dazwischen ab, bleibt der
        // alte Stand gültig und die Datei verwaist
        atomic_write(&self.dir.join(VERSIONS_DIR).join(version_file(number)), text.as_bytes())?;
        self.persist(&info)?;
        self.info = info;
        Ok(number)
    }

    pub fn set_comment(&mut self, n: u32, text: &str) -> Result<()> {
        let text = clean_comment(text)?;
        let mut info = self.info.clone();
        let v = info.versions.iter_mut().find(|v| v.number == n).ok_or_else(|| ProjectError::NotFound(format!("Version {n}")))?;
        v.comment = text;
        self.persist(&info)?;
        self.info = info;
        Ok(())
    }

    pub fn set_active_branch(&mut self, name: &str) -> Result<()> {
        if !self.info.branches.contains_key(name) {
            return Err(ProjectError::NotFound(format!("Zweig {name}")));
        }
        let mut info = self.info.clone();
        info.active_branch = name.to_string();
        self.persist(&info)?;
        self.info = info;
        Ok(())
    }

    pub fn delete_version(&mut self, n: u32) -> Result<()> {
        let v = self.info.version(n).ok_or_else(|| ProjectError::NotFound(format!("Version {n}")))?.clone();
        if self.info.versions.len() == 1 {
            return Err(ProjectError::Invalid("Die einzige Version lässt sich nicht löschen.".into()));
        }
        if !self.info.children(n).is_empty() {
            return Err(ProjectError::Invalid("Auf dieser Version bauen andere auf.".into()));
        }
        let mut info = self.info.clone();
        info.versions.retain(|x| x.number != n);
        match info.versions.iter().filter(|x| x.branch == v.branch).map(|x| x.number).max() {
            Some(tip) => {
                info.branches.insert(v.branch.clone(), tip);
            }
            None => {
                info.branches.remove(&v.branch);
                if info.active_branch == v.branch {
                    let parent_branch = v.parent.and_then(|p| info.version(p)).map(|p| p.branch.clone());
                    info.active_branch = parent_branch.unwrap_or_else(|| info.branches.keys().next().cloned().unwrap_or_default());
                }
            }
        }
        info.check().map_err(ProjectError::Invalid)?;
        self.persist(&info)?;
        self.info = info;
        let _ = fs::remove_file(self.dir.join(VERSIONS_DIR).join(version_file(n)));
        Ok(())
    }

    pub fn delete_branch(&mut self, name: &str) -> Result<()> {
        if !self.info.branches.contains_key(name) {
            return Err(ProjectError::NotFound(format!("Zweig {name}")));
        }
        if self.info.active_branch == name {
            return Err(ProjectError::Invalid("Der aktive Zweig lässt sich nicht löschen.".into()));
        }
        let own: BTreeSet<u32> = self.info.versions.iter().filter(|v| v.branch == name).map(|v| v.number).collect();
        if own.iter().any(|n| self.info.children(*n).iter().any(|c| !own.contains(c))) {
            return Err(ProjectError::Invalid("Auf diesem Zweig bauen andere Zweige auf.".into()));
        }
        let mut info = self.info.clone();
        info.versions.retain(|v| !own.contains(&v.number));
        info.branches.remove(name);
        info.check().map_err(ProjectError::Invalid)?;
        self.persist(&info)?;
        self.info = info;
        for n in own {
            let _ = fs::remove_file(self.dir.join(VERSIONS_DIR).join(version_file(n)));
        }
        Ok(())
    }

    pub fn write_autosave(&self, based_on: Option<u32>, snapshot: &Snapshot) -> Result<()> {
        let file = AutosaveFile { format: FORMAT_VERSION, basiert_auf: based_on, zeit: now_iso(), inhalt: encode_version(snapshot)? };
        let text = serde_json::to_string(&file).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        atomic_write(&self.dir.join(AUTOSAVE_FILE), text.as_bytes())
    }

    pub fn read_autosave(&self) -> Result<Option<Autosave>> {
        let path = self.dir.join(AUTOSAVE_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        let origin = path.display().to_string();
        let text = fs::read_to_string(&path).map_err(|e| ProjectError::io(&path, &e))?;
        let f: AutosaveFile = serde_json::from_str(&text).map_err(|e| ProjectError::corrupt(origin.clone(), e.to_string()))?;
        if f.format > FORMAT_VERSION {
            return Err(ProjectError::TooNew { path: origin, found: f.format });
        }
        Ok(Some(Autosave { based_on: f.basiert_auf, time: f.zeit, snapshot: decode_version(&f.inhalt, &origin)? }))
    }

    pub fn discard_autosave(&self) {
        let _ = fs::remove_file(self.dir.join(AUTOSAVE_FILE));
    }
}
```

`lib.rs`: `pub mod docstore;`. Der Test `comments_and_active_branch` erwartet, dass `set_comment("  neu  \nzweite Zeile")` zu `"neu  \nzweite Zeile"` wird (führende Leerzeichen entfernt, innere bleiben); `trim()` liefert genau das.

- [ ] **Step 5: Tests grün, fmt, clippy, Commit**

```bash
cargo test -p planora-project && cargo fmt && cargo clippy -p planora-project --all-targets -- -D warnings
git add -A && git commit -m "feat: Versionsspeicher (Versionsbaum, Zweige, Wiederherstellen, Autosicherung)"
```

---

### Task 8: Workspace – Tabs, Speichern, Öffnen (Logik ohne Oberfläche)

**Files:**
- Create: `crates/planora-app/src/workspace.rs`
- Modify: `crates/planora-app/Cargo.toml` (`planora-project`), `crates/planora-app/src/{lib.rs,state.rs}`
- Test: `crates/planora-app/tests/workspace.rs`, außerdem Ergänzung in `crates/planora-app/tests/state.rs`

**Interfaces:**
- Consumes: `AppState`, `CreateDialog`, `BooleanDialog`, `FeatureDialog`, `planora_project::{Library, NodePath, DocStore, LockMode, Config, docstore::{peek, read_version, DocumentInfo, Autosave}}`, `Snapshot`.
- Produces:
  ```rust
  // state.rs
  impl AppState { pub fn snapshot(&self) -> Snapshot;
                  pub fn from_snapshot(s: Snapshot) -> Result<AppState, String>; }
  // workspace.rs
  pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(120);
  pub struct DocTab { pub id: u64, pub state: AppState, pub dialog: Option<CreateDialog>, pub boolean_dialog: Option<BooleanDialog>,
      pub feature_dialog: Option<FeatureDialog>, pub name: String, pub path: Option<NodePath>, pub store: Option<DocStore>,
      pub source_dir: Option<PathBuf>, pub based_on: Option<u32>, pub read_only: bool, pub pending_autosave: Option<Autosave>, /* privat: saved_edits, autosaved_edits, last_autosave */ }
  impl DocTab { pub fn is_dirty(&self) -> bool; pub fn title(&self) -> String; pub fn version_info(&self) -> Option<DocumentInfo>; }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum After { CloseTab, Quit }
  #[derive(Clone, Debug)] pub enum Prompt {
      SaveChanges { tab: u64, then: After },
      SaveComment { tab: u64, then: Option<After>, comment: String, branch: Option<String> },   // branch = Some → Zweigname Pflicht
      SaveAs { tab: u64, then: Option<After>, parent: NodePath, name: String, comment: String },
      TakeOver { path: NodePath, version: Option<u32> },
      AutosaveOffer { tab: u64 },
      Notice(String),
  }
  #[derive(Debug, PartialEq, Eq)] pub enum SaveOutcome { Saved(u32), NeedsLocation, NeedsBranch }
  pub struct OpenResult { pub tab: usize, pub notes: Vec<String>, pub autosave_offered: bool }
  pub struct Workspace { pub tabs: Vec<DocTab>, pub active: usize, pub library: Option<Library>, pub config: Config,
                         pub prompt: Option<Prompt>, pub quit: bool, pub autosave_interval: Duration, /* privat: Zähler */ }
  impl Workspace {
      pub fn new(library: Option<Library>, config: Config) -> Workspace;      // ein leerer Tab „Unbenannt 1“
      pub fn new_tab(&mut self) -> usize;
      pub fn activate(&mut self, i: usize);                                    // außerhalb des Bereichs: nichts
      pub fn active_tab(&self) -> &DocTab;  pub fn active_tab_mut(&mut self) -> &mut DocTab;
      pub fn tab_index(&self, id: u64) -> Option<usize>;
      pub fn tab_for_path(&self, path: &NodePath) -> Option<usize>;            // beschreibbarer Tab dieses Dokuments
      pub fn set_dark(&mut self, dark: bool);                                  // alle Tabs
      pub fn close_tab(&mut self, i: usize);                                   // ohne Rückfrage; nie ohne Tab
      pub fn request_close(&mut self, i: usize) -> bool;                       // true = geschlossen, false = Rückfrage (prompt)
      pub fn request_quit(&mut self) -> bool;                                  // true = darf beenden (setzt quit)
      pub fn discard_and_continue(&mut self);                                  // Prompt::SaveChanges „Verwerfen“
      pub fn save_active(&mut self, comment: &str, new_branch: Option<&str>) -> Result<SaveOutcome, String>;
      pub fn save_as(&mut self, parent: &NodePath, name: &str, comment: &str) -> Result<NodePath, String>;
      pub fn save_for_prompt(&mut self);                                       // „Speichern“ im Prompt SaveChanges
      pub fn finish_after(&mut self, tab: u64, then: Option<After>);
      pub fn open_document(&mut self, path: &NodePath, mode: LockMode) -> Result<OpenResult, ProjectError>;
      pub fn apply_autosave(&mut self, tab: u64);  pub fn discard_autosave(&mut self, tab: u64);
      pub fn open_version(&mut self, path: &NodePath, n: u32) -> Result<usize, ProjectError>;
      pub fn make_editable(&mut self, i: usize) -> Result<(), String>;         // „Abzweigen“ aus einer Ansicht
      pub fn restore_version(&mut self, i: usize, n: u32) -> Result<(), String>;
      pub fn choose_root(&mut self, root: &Path) -> Result<(), String>;
      pub fn persist_config(&self);
      pub fn autosave_tick(&mut self, now: Instant);
  }
  ```

- [ ] **Step 1: Zustand erweitern** (`state.rs`)

```rust
static NEXT_SCENE_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Szenenstände sind über alle Tabs eindeutig, damit ein Tab-Wechsel die GPU-Szene sicher
/// neu hochlädt (siehe viewport.rs).
fn next_scene_version() -> u64 {
    NEXT_SCENE_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
```

In `rebuild_scene`: `self.scene_version += 1;` ersetzen durch `self.scene_version = next_scene_version();`. Neue Methoden:

```rust
impl AppState {
    /// Stand der Zeitleiste zum Speichern (eine offene Skizze wird vorher mit `try_close_sketch`
    /// ins Dokument geschrieben; das ist Sache des Aufrufers).
    pub fn snapshot(&self) -> planora_feature::Snapshot {
        self.document.snapshot()
    }

    /// Neuer Zustand aus einem geladenen Stand: Dokument aufbauen und berechnen, Namenszähler
    /// fortsetzen, Körper ableiten, Ansicht einpassen.
    pub fn from_snapshot(snapshot: planora_feature::Snapshot) -> Result<AppState, String> {
        let mut s = AppState::new();
        s.document = Document::from_snapshot(snapshot, s.chord_tolerance).map_err(|e| e.to_string())?;
        s.rebuild_name_counters();
        s.refresh();
        s.frame_all();
        Ok(s)
    }

    /// Setzt die Zähler je Bezeichnung aus vorhandenen Namen „Bezeichnung N“ fort, damit neue
    /// Namen nach dem Laden nicht doppelt vergeben werden.
    fn rebuild_name_counters(&mut self) {
        self.name_counters.clear();
        for f in self.document.features() {
            if let Some((label, num)) = f.name.rsplit_once(' ')
                && let Ok(n) = num.parse::<u64>()
            {
                let c = self.name_counters.entry(label.to_string()).or_insert(0);
                *c = (*c).max(n);
            }
        }
    }
}
```

Test in `tests/state.rs` ergänzen (Importe der Datei beachten; `DVec3` kommt aus `planora_math`):

```rust
#[test]
fn scene_versions_are_unique_across_states_and_loading_continues_the_names() {
    let a = AppState::new();
    let b = AppState::new();
    assert_ne!(a.scene_version, b.scene_version);
    let mut s = AppState::new();
    s.create_box(planora_math::DVec3::splat(3.0)).unwrap();
    s.create_box(planora_math::DVec3::splat(4.0)).unwrap();
    let mut t = AppState::from_snapshot(s.snapshot()).unwrap();
    assert_eq!(t.bodies.len(), 2);
    t.create_box(planora_math::DVec3::splat(5.0)).unwrap();
    assert!(t.document.features().iter().any(|f| f.name == "Quader 3"));
}
```

- [ ] **Step 2: Workspace-Tests schreiben** (`crates/planora-app/tests/workspace.rs`)

```rust
use planora_app::workspace::*;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::LockMode;
use planora_project::fsutil::new_id;
use planora_project::library::{Library, NodePath};
use std::time::{Duration, Instant};

fn root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("planora-ws-{}", new_id()))
}

fn ws() -> Workspace {
    Workspace::new(Some(Library::open(&root()).unwrap()), Config::default())
}

fn add_box(ws: &mut Workspace) {
    ws.active_tab_mut().state.create_box(DVec3::splat(5.0)).unwrap();
}

/// Speichert den aktiven Tab als Dokument `name` im Projekt „P“ mit `boxes` Quadern.
fn saved_document(w: &mut Workspace, name: &str, boxes: usize) -> NodePath {
    let p = w.library.as_ref().unwrap().create_project("P").unwrap_or_else(|_| vec!["P".to_string()]);
    for _ in 0..boxes {
        add_box(w);
    }
    w.save_as(&p, name, "").unwrap()
}

#[test]
fn a_new_workspace_has_one_clean_untitled_tab() {
    let w = ws();
    assert_eq!(w.tabs.len(), 1);
    assert_eq!(w.active_tab().title(), "Unbenannt 1");
    assert!(!w.active_tab().is_dirty());
}

#[test]
fn tabs_have_independent_states_and_unique_scene_versions() {
    let mut w = ws();
    add_box(&mut w);
    let second = w.new_tab();
    assert_eq!(w.active, second);
    assert_eq!(w.active_tab().title(), "Unbenannt 2");
    assert_eq!(w.active_tab().state.bodies.len(), 0);
    assert_ne!(w.tabs[0].state.scene_version, w.tabs[1].state.scene_version);
    w.activate(0);
    assert_eq!(w.active_tab().state.bodies.len(), 1);
    w.activate(99);
    assert_eq!(w.active, 0);
    w.set_dark(false);
    assert!(w.tabs.iter().all(|t| !t.state.dark));
}

#[test]
fn dirty_follows_edits() {
    let mut w = ws();
    assert!(!w.active_tab().is_dirty());
    add_box(&mut w);
    assert!(w.active_tab().is_dirty());
}

#[test]
fn closing_a_clean_tab_works_and_the_last_tab_is_replaced_by_a_fresh_one() {
    let mut w = ws();
    assert!(w.request_close(0));
    assert_eq!(w.tabs.len(), 1);
    assert!(w.active_tab().title().starts_with("Unbenannt"));
    assert!(!w.active_tab().is_dirty());
}

#[test]
fn closing_a_dirty_tab_asks_first_and_discard_closes_it() {
    let mut w = ws();
    add_box(&mut w);
    w.new_tab();
    assert!(!w.request_close(0));
    assert!(matches!(w.prompt, Some(Prompt::SaveChanges { then: After::CloseTab, .. })));
    assert_eq!(w.tabs.len(), 2);
    w.discard_and_continue();
    assert!(w.prompt.is_none());
    assert_eq!(w.tabs.len(), 1);
    assert!(w.tabs.iter().all(|t| t.state.bodies.is_empty()));
}

#[test]
fn quitting_asks_for_each_dirty_tab_in_turn() {
    let mut w = ws();
    add_box(&mut w);
    w.new_tab();
    add_box(&mut w);
    assert!(!w.request_quit());
    assert!(matches!(w.prompt, Some(Prompt::SaveChanges { then: After::Quit, .. })));
    w.discard_and_continue();
    assert!(matches!(w.prompt, Some(Prompt::SaveChanges { then: After::Quit, .. })), "zweiter Tab fragt");
    w.discard_and_continue();
    assert!(w.quit && w.prompt.is_none());
    let mut clean = ws();
    assert!(clean.request_quit() && clean.quit);
}

#[test]
fn an_untitled_tab_needs_a_location_then_saves_and_is_clean() {
    let mut w = ws();
    add_box(&mut w);
    assert_eq!(w.save_active("", None).unwrap(), SaveOutcome::NeedsLocation);
    let project = w.library.as_ref().unwrap().create_project("P").unwrap();
    let path = w.save_as(&project, "Teil", "Erster Stand").unwrap();
    assert_eq!(path, vec!["P".to_string(), "Teil.pcad".to_string()]);
    assert_eq!(w.active_tab().title(), "Teil");
    assert!(!w.active_tab().is_dirty());
    assert_eq!(w.active_tab().based_on, Some(1));
    add_box(&mut w);
    assert!(w.active_tab().is_dirty());
    assert_eq!(w.save_active("zweiter Stand", None).unwrap(), SaveOutcome::Saved(2));
    assert!(!w.active_tab().is_dirty());
    let info = w.active_tab().version_info().unwrap();
    assert_eq!(info.versions.len(), 2);
    assert_eq!(info.version(2).unwrap().comment, "zweiter Stand");
}

#[test]
fn saving_takes_an_open_sketch_along() {
    let mut w = ws();
    let project = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&project, "S", "").unwrap();
    {
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(planora_feature::SketchPlane::Standard(planora_app::state::StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
    }
    assert!(w.active_tab().is_dirty(), "offene Skizze zählt als geändert");
    assert_eq!(w.save_active("", None).unwrap(), SaveOutcome::Saved(2));
    assert!(w.active_tab().state.sketch_session.is_none());
    assert!(!w.active_tab().is_dirty());
}

#[test]
fn save_as_rejects_bad_places_and_names_without_changing_the_tab() {
    let mut w = ws();
    add_box(&mut w);
    assert!(w.save_as(&vec![], "X", "").is_err(), "kein Projekt");
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    assert!(w.save_as(&p, "a/b", "").is_err());
    assert!(w.save_as(&p, "..", "").is_err());
    assert!(w.active_tab().store.is_none());
    w.save_as(&p, "Teil", "").unwrap();
    // zweites Dokument gleichen Namens
    w.new_tab();
    add_box(&mut w);
    assert!(w.save_as(&p, "Teil", "").is_err());
    assert!(w.active_tab().store.is_none());
}

#[test]
fn the_save_prompt_saves_closes_and_asks_for_missing_places() {
    let mut w = ws();
    let project = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&project, "T", "").unwrap();
    add_box(&mut w);
    assert!(!w.request_close(0));
    w.save_for_prompt();
    assert!(w.prompt.is_none());
    assert!(w.tabs.iter().all(|t| t.store.is_none() || !t.is_dirty()));
    // unbenannt: Prompt wechselt zu SaveAs
    let mut w2 = ws();
    add_box(&mut w2);
    w2.request_close(0);
    w2.save_for_prompt();
    assert!(matches!(w2.prompt, Some(Prompt::SaveAs { .. })));
}

#[test]
fn opening_a_document_loads_the_active_branch_and_reuses_an_open_tab() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 2);
    w.close_tab(0);
    assert_eq!(w.tabs.len(), 1);
    let r = w.open_document(&path, LockMode::Acquire).unwrap();
    assert_eq!(w.tabs.len(), 1, "der leere Tab wird ersetzt");
    assert_eq!(w.tabs[r.tab].state.bodies.len(), 2);
    assert!(!w.tabs[r.tab].is_dirty());
    let again = w.open_document(&path, LockMode::Acquire).unwrap();
    assert_eq!(again.tab, r.tab);
    assert_eq!(w.tabs.len(), 1);
}

#[test]
fn a_locked_document_reports_it_and_can_be_taken_over() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    // zweiter Arbeitsbereich (anderes Programm) auf demselben Ordner
    let mut other = Workspace::new(Some(Library::open(w.library.as_ref().unwrap().root().path()).unwrap()), Config::default());
    assert!(matches!(other.open_document(&path, LockMode::Acquire), Err(planora_project::ProjectError::Locked { .. })));
    assert!(other.open_document(&path, LockMode::TakeOver).is_ok());
}

#[test]
fn old_versions_open_read_only_and_can_be_restored() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    add_box(&mut w);
    w.save_active("zwei", None).unwrap();
    add_box(&mut w);
    w.save_active("drei", None).unwrap();
    let v = w.open_version(&path, 1).unwrap();
    assert!(w.tabs[v].read_only);
    assert_eq!(w.tabs[v].title(), "Teil · v1");
    assert_eq!(w.tabs[v].state.bodies.len(), 1);
    assert!(!w.tabs[v].is_dirty());
    assert!(w.save_active("", None).unwrap_err().contains("schreibgeschützt"));
    // dieselbe Version nicht doppelt öffnen
    assert_eq!(w.open_version(&path, 1).unwrap(), v);
    // Wiederherstellen: neue Version 4 mit dem Stand von v1 im beschreibbaren Tab
    w.restore_version(v, 1).unwrap();
    let t = &w.tabs[w.tab_for_path(&path).unwrap()];
    assert_eq!(t.state.bodies.len(), 1);
    assert_eq!(t.based_on, Some(4));
    assert_eq!(t.version_info().unwrap().versions.len(), 4);
}

#[test]
fn restoring_is_refused_while_the_writable_tab_has_unsaved_changes() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    let v = w.open_version(&path, 1).unwrap();
    let writable = w.tab_for_path(&path).unwrap();
    w.activate(writable);
    add_box(&mut w);
    assert!(w.restore_version(v, 1).unwrap_err().contains("ungespeicherte"));
}

#[test]
fn branching_from_an_old_version_requires_a_branch_name_on_save() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    add_box(&mut w);
    w.save_active("zwei", None).unwrap();
    w.close_tab(w.tab_for_path(&path).unwrap());
    let v = w.open_version(&path, 1).unwrap();
    w.make_editable(v).unwrap();
    assert!(!w.tabs[v].read_only);
    w.activate(v);
    add_box(&mut w);
    assert_eq!(w.save_active("Variante", None).unwrap(), SaveOutcome::NeedsBranch);
    assert_eq!(w.save_active("Variante", Some("Variante A")).unwrap(), SaveOutcome::Saved(3));
    let info = w.active_tab().version_info().unwrap();
    assert_eq!(info.version(3).unwrap().branch, "Variante A");
    assert!(!w.active_tab().is_dirty());
}

#[test]
fn making_a_view_editable_fails_while_another_tab_holds_the_document() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    let v = w.open_version(&path, 1).unwrap();
    assert!(w.make_editable(v).is_err());
    assert!(w.tabs[v].read_only);
}

#[test]
fn an_autosave_is_offered_after_a_crash_and_can_be_applied_or_discarded() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    w.autosave_interval = Duration::ZERO;
    add_box(&mut w);
    w.autosave_tick(Instant::now());
    // „Absturz“: Tab ohne Speichern schließen
    w.close_tab(w.tab_for_path(&path).unwrap());
    let r = w.open_document(&path, LockMode::Acquire).unwrap();
    assert!(r.autosave_offered);
    let id = w.tabs[r.tab].id;
    assert_eq!(w.tabs[r.tab].state.bodies.len(), 1, "zuerst der gespeicherte Stand");
    w.apply_autosave(id);
    assert_eq!(w.tabs[r.tab].state.bodies.len(), 2);
    assert!(w.tabs[r.tab].is_dirty(), "übernommene Sicherung gilt als ungespeichert");
    w.close_tab(r.tab);
    let r2 = w.open_document(&path, LockMode::Acquire).unwrap();
    assert!(r2.autosave_offered);
    let id2 = w.tabs[r2.tab].id;
    w.discard_autosave(id2);
    w.close_tab(r2.tab);
    assert!(!w.open_document(&path, LockMode::Acquire).unwrap().autosave_offered);
}

#[test]
fn saving_removes_the_autosave() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    w.autosave_interval = Duration::ZERO;
    add_box(&mut w);
    w.autosave_tick(Instant::now());
    let dir = w.library.as_ref().unwrap().document_dir(&path).unwrap();
    assert!(dir.join(".autosave.json").exists());
    w.save_active("", None).unwrap();
    assert!(!dir.join(".autosave.json").exists());
}

#[test]
fn a_damaged_document_does_not_open_and_changes_nothing() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    let dir = w.library.as_ref().unwrap().document_dir(&path).unwrap();
    w.close_tab(0);
    std::fs::write(dir.join("dokument.json"), "{").unwrap();
    let before = w.tabs.len();
    assert!(w.open_document(&path, LockMode::Acquire).is_err());
    assert_eq!(w.tabs.len(), before);
}

#[test]
fn choosing_a_root_creates_it_and_remembers_it() {
    let mut w = Workspace::new(None, Config::default());
    assert!(w.library.is_none());
    let r = root();
    w.choose_root(&r).unwrap();
    assert!(w.library.is_some());
    assert_eq!(w.config.root.as_deref(), Some(r.as_path()));
}
```

- [ ] **Step 3: Fehlschlag prüfen** → FAIL (Modul `workspace` fehlt).

- [ ] **Step 4: Implementierung** (`workspace.rs`)

```rust
//! Mehrere offene Dokumente (Tabs) mit Speichern, Öffnen, Versionen und Autosicherung. Die
//! Logik kennt keine Oberfläche; die Fenster dazu stehen in `tabs.rs`, `prompts.rs` und den Panels.

use crate::boolean_dialog::BooleanDialog;
use crate::create_dialog::CreateDialog;
use crate::feature_dialog::FeatureDialog;
use crate::state::AppState;
use planora_project::config::Config;
use planora_project::docstore::{Autosave, DocStore, DocumentInfo, LockMode, peek, read_version};
use planora_project::fsutil::validate_name;
use planora_project::library::{Library, NodePath};
use planora_project::{ProjectError, Result as ProjectResult};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Abstand der Autosicherungen geänderter Dokumente.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(120);

pub struct DocTab {
    pub id: u64,
    pub state: AppState,
    pub dialog: Option<CreateDialog>,
    pub boolean_dialog: Option<BooleanDialog>,
    pub feature_dialog: Option<FeatureDialog>,
    /// Anzeigename ohne `.pcad`, bzw. „Unbenannt n“.
    pub name: String,
    pub path: Option<NodePath>,
    pub store: Option<DocStore>,
    /// Dokumentordner einer schreibgeschützten Ansicht (ohne Sperre).
    pub source_dir: Option<PathBuf>,
    /// Version, auf der der Stand beruht.
    pub based_on: Option<u32>,
    pub read_only: bool,
    pub pending_autosave: Option<Autosave>,
    saved_edits: u64,
    autosaved_edits: u64,
    last_autosave: Instant,
}

impl DocTab {
    fn new(id: u64, name: String, state: AppState) -> DocTab {
        let edits = state.document.edit_count();
        DocTab {
            id, state, dialog: None, boolean_dialog: None, feature_dialog: None, name, path: None,
            store: None, source_dir: None, based_on: None, read_only: false, pending_autosave: None,
            saved_edits: edits, autosaved_edits: edits, last_autosave: Instant::now(),
        }
    }

    /// Ungespeicherte Änderungen: Zähler weicht ab oder eine Skizze ist offen. Schreibgeschützte
    /// Ansichten sind nie „geändert“.
    pub fn is_dirty(&self) -> bool {
        !self.read_only
            && (self.state.document.edit_count() != self.saved_edits || self.state.sketch_session.is_some())
    }

    pub fn title(&self) -> String {
        match (self.read_only, self.based_on) {
            (true, Some(n)) => format!("{} · v{n}", self.name),
            _ => self.name.clone(),
        }
    }

    /// Versionsbaum zum Anzeigen: aus dem Speicher des Tabs, sonst (Ansicht) frisch gelesen.
    pub fn version_info(&self) -> Option<DocumentInfo> {
        match (&self.store, &self.source_dir) {
            (Some(s), _) => Some(s.info().clone()),
            (None, Some(dir)) => peek(dir).ok().map(|(i, _)| i),
            _ => None,
        }
    }

    fn mark_clean(&mut self) {
        self.saved_edits = self.state.document.edit_count();
        self.autosaved_edits = self.saved_edits;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    CloseTab,
    Quit,
}

#[derive(Clone, Debug)]
pub enum Prompt {
    SaveChanges { tab: u64, then: After },
    SaveComment { tab: u64, then: Option<After>, comment: String, branch: Option<String> },
    SaveAs { tab: u64, then: Option<After>, parent: NodePath, name: String, comment: String },
    TakeOver { path: NodePath, version: Option<u32> },
    AutosaveOffer { tab: u64 },
    Notice(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved(u32),
    NeedsLocation,
    NeedsBranch,
}

pub struct OpenResult {
    pub tab: usize,
    pub notes: Vec<String>,
    pub autosave_offered: bool,
}

pub struct Workspace {
    pub tabs: Vec<DocTab>,
    pub active: usize,
    pub library: Option<Library>,
    pub config: Config,
    pub prompt: Option<Prompt>,
    pub quit: bool,
    pub autosave_interval: Duration,
    next_tab_id: u64,
    untitled: usize,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Workspace {
    pub fn new(library: Option<Library>, config: Config) -> Workspace {
        let mut w = Workspace {
            tabs: Vec::new(), active: 0, library, config, prompt: None, quit: false,
            autosave_interval: AUTOSAVE_INTERVAL, next_tab_id: 1, untitled: 0,
        };
        w.new_tab();
        w
    }

    fn tab_id(&mut self) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        id
    }

    pub fn new_tab(&mut self) -> usize {
        self.untitled += 1;
        let id = self.tab_id();
        let dark = self.tabs.get(self.active).map(|t| t.state.dark).unwrap_or(true);
        let mut state = AppState::new();
        state.dark = dark;
        state.rebuild_scene();
        state.frame_all();
        self.tabs.push(DocTab::new(id, format!("Unbenannt {}", self.untitled), state));
        self.active = self.tabs.len() - 1;
        self.active
    }

    pub fn activate(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        }
    }

    pub fn active_tab(&self) -> &DocTab {
        &self.tabs[self.active.min(self.tabs.len() - 1)]
    }

    pub fn active_tab_mut(&mut self) -> &mut DocTab {
        let i = self.active.min(self.tabs.len() - 1);
        &mut self.tabs[i]
    }

    pub fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub fn tab_for_path(&self, path: &NodePath) -> Option<usize> {
        self.tabs.iter().position(|t| !t.read_only && t.path.as_ref() == Some(path))
    }

    pub fn set_dark(&mut self, dark: bool) {
        for t in &mut self.tabs {
            t.state.dark = dark;
            t.state.rebuild_scene();
        }
    }

    /// Schließt einen Tab ohne Rückfrage; nach dem letzten entsteht ein frischer leerer Tab.
    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.new_tab();
        } else if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
    }

    pub fn request_close(&mut self, i: usize) -> bool {
        let Some(t) = self.tabs.get(i) else { return true };
        if t.is_dirty() {
            self.prompt = Some(Prompt::SaveChanges { tab: t.id, then: After::CloseTab });
            false
        } else {
            self.close_tab(i);
            true
        }
    }

    pub fn request_quit(&mut self) -> bool {
        if let Some(t) = self.tabs.iter().find(|t| t.is_dirty()) {
            self.prompt = Some(Prompt::SaveChanges { tab: t.id, then: After::Quit });
            false
        } else {
            self.quit = true;
            true
        }
    }

    /// „Verwerfen“ im Prompt `SaveChanges`.
    pub fn discard_and_continue(&mut self) {
        if let Some(Prompt::SaveChanges { tab, then }) = self.prompt.take() {
            self.finish_after(tab, Some(then));
        }
    }

    /// Schließt `tab` (Verwerfen oder nach dem Speichern) und macht beim Beenden mit dem
    /// nächsten geänderten Tab weiter.
    pub fn finish_after(&mut self, tab: u64, then: Option<After>) {
        let Some(then) = then else { return };
        if let Some(i) = self.tab_index(tab) {
            self.close_tab(i);
        }
        if then == After::Quit {
            self.request_quit();
        }
    }

    // ---- Speichern -------------------------------------------------------------------

    fn save_tab(tab: &mut DocTab, comment: &str, new_branch: Option<&str>) -> Result<SaveOutcome, String> {
        if tab.read_only {
            return Err("Diese Version ist schreibgeschützt. Zuerst abzweigen oder wiederherstellen.".into());
        }
        tab.state.try_close_sketch()?;
        let snapshot = tab.state.snapshot();
        let Some(store) = tab.store.as_mut() else { return Ok(SaveOutcome::NeedsLocation) };
        match store.save(tab.based_on, &snapshot, comment, new_branch) {
            Ok(n) => {
                tab.based_on = Some(n);
                tab.mark_clean();
                store.discard_autosave();
                Ok(SaveOutcome::Saved(n))
            }
            Err(ProjectError::NeedsBranch) => Ok(SaveOutcome::NeedsBranch),
            Err(e) => Err(err(e)),
        }
    }

    pub fn save_active(&mut self, comment: &str, new_branch: Option<&str>) -> Result<SaveOutcome, String> {
        let i = self.active.min(self.tabs.len() - 1);
        Self::save_tab(&mut self.tabs[i], comment, new_branch)
    }

    pub fn save_as(&mut self, parent: &NodePath, name: &str, comment: &str) -> Result<NodePath, String> {
        let lib = self.library.as_ref().ok_or("Es ist noch kein Projektordner gewählt.")?;
        if !lib.is_container(parent) {
            return Err("Bitte ein Projekt oder einen Ordner als Ziel wählen.".into());
        }
        let name = validate_name(name).map_err(err)?;
        let mut path = parent.clone();
        path.push(format!("{name}.pcad"));
        let dir = lib.root().resolve(&path).map_err(err)?;
        let i = self.active.min(self.tabs.len() - 1);
        let tab = &mut self.tabs[i];
        tab.state.try_close_sketch()?;
        let store = DocStore::create(&dir, &tab.state.snapshot(), comment).map_err(err)?;
        tab.store = Some(store);
        tab.path = Some(path.clone());
        tab.name = name;
        tab.based_on = Some(1);
        tab.read_only = false;
        tab.source_dir = None;
        tab.mark_clean();
        Ok(path)
    }

    /// „Speichern“ im Prompt `SaveChanges`: speichert den Tab und macht weiter; fehlen Ort oder
    /// Zweigname, wechselt der Prompt in das passende Fenster.
    pub fn save_for_prompt(&mut self) {
        let Some(Prompt::SaveChanges { tab, then }) = self.prompt.take() else { return };
        let Some(i) = self.tab_index(tab) else { return };
        self.active = i;
        match Self::save_tab(&mut self.tabs[i], "", None) {
            Ok(SaveOutcome::Saved(_)) => self.finish_after(tab, Some(then)),
            Ok(SaveOutcome::NeedsLocation) => {
                self.prompt = Some(Prompt::SaveAs {
                    tab, then: Some(then), parent: Vec::new(), name: self.tabs[i].name.clone(), comment: String::new(),
                });
            }
            Ok(SaveOutcome::NeedsBranch) => {
                self.prompt = Some(Prompt::SaveComment { tab, then: Some(then), comment: String::new(), branch: Some(String::new()) });
            }
            Err(e) => self.prompt = Some(Prompt::Notice(e)),
        }
    }

    // ---- Öffnen ----------------------------------------------------------------------

    fn replace_empty_start_tab(&self) -> bool {
        self.tabs.len() == 1
            && self.tabs[0].store.is_none()
            && !self.tabs[0].read_only
            && !self.tabs[0].is_dirty()
            && self.tabs[0].state.document.features().is_empty()
    }

    fn push_tab(&mut self, tab: DocTab) -> usize {
        if self.replace_empty_start_tab() {
            self.tabs[0] = tab;
            self.active = 0;
        } else {
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
        }
        self.active
    }

    pub fn open_document(&mut self, path: &NodePath, mode: LockMode) -> ProjectResult<OpenResult> {
        if let Some(i) = self.tab_for_path(path) {
            self.active = i;
            return Ok(OpenResult { tab: i, notes: Vec::new(), autosave_offered: false });
        }
        let lib = self
            .library
            .as_ref()
            .ok_or_else(|| ProjectError::Invalid("Es ist noch kein Projektordner gewählt.".into()))?;
        let dir = lib.document_dir(path)?;
        let (store, report) = DocStore::open(&dir, mode)?;
        let branch = store.info().active_branch.clone();
        let v = store
            .latest_readable(&branch)
            .ok_or_else(|| ProjectError::corrupt(dir.display().to_string(), "Es gibt keine lesbare Version."))?;
        let snapshot = store.load_version(v)?;
        let state = AppState::from_snapshot(snapshot).map_err(ProjectError::Invalid)?;
        let mut notes = Vec::new();
        if !report.orphans.is_empty() {
            notes.push(format!("Reste eines abgebrochenen Speicherns gefunden und ignoriert: {}.", report.orphans.join(", ")));
        }
        if !report.missing.is_empty() {
            let list: Vec<String> = report.missing.iter().map(|n| format!("v{n}")).collect();
            notes.push(format!("Versionsdateien fehlen: {}.", list.join(", ")));
        }
        if store.info().tip(&branch) != Some(v) {
            notes.push(format!("Die neueste Version ist nicht lesbar; v{v} wurde geladen."));
        }
        // Eine Autosicherung existiert nur, wenn das letzte Speichern nicht abgeschlossen wurde;
        // „>=“, weil Zeitstempel sekundengenau sind.
        let autosave = match store.read_autosave() {
            Ok(Some(a)) if a.time.as_str() >= store.info().latest_time() => Some(a),
            Ok(_) => None,
            Err(e) => {
                notes.push(format!("Die Autosicherung ist unlesbar und wird ignoriert: {e}"));
                store.discard_autosave();
                None
            }
        };
        let offered = autosave.is_some();
        let id = self.tab_id();
        let name = path.last().map(|p| p.trim_end_matches(".pcad").to_string()).unwrap_or_default();
        let mut tab = DocTab::new(id, name, state);
        tab.path = Some(path.clone());
        tab.store = Some(store);
        tab.based_on = Some(v);
        tab.pending_autosave = autosave;
        let i = self.push_tab(tab);
        if offered {
            self.prompt = Some(Prompt::AutosaveOffer { tab: id });
        }
        Ok(OpenResult { tab: i, notes, autosave_offered: offered })
    }

    pub fn apply_autosave(&mut self, tab: u64) {
        if matches!(self.prompt, Some(Prompt::AutosaveOffer { .. })) {
            self.prompt = None;
        }
        let Some(i) = self.tab_index(tab) else { return };
        let Some(a) = self.tabs[i].pending_autosave.take() else { return };
        match AppState::from_snapshot(a.snapshot) {
            Ok(mut state) => {
                state.dark = self.tabs[i].state.dark;
                state.rebuild_scene();
                let t = &mut self.tabs[i];
                t.state = state;
                t.based_on = a.based_on.or(t.based_on);
                // gilt als geändert gegenüber dem gespeicherten Stand
                t.saved_edits = u64::MAX;
            }
            Err(e) => self.prompt = Some(Prompt::Notice(format!("Die Autosicherung lässt sich nicht laden: {e}"))),
        }
    }

    pub fn discard_autosave(&mut self, tab: u64) {
        if matches!(self.prompt, Some(Prompt::AutosaveOffer { .. })) {
            self.prompt = None;
        }
        if let Some(i) = self.tab_index(tab) {
            self.tabs[i].pending_autosave = None;
            if let Some(s) = &self.tabs[i].store {
                s.discard_autosave();
            }
        }
    }

    /// Öffnet Version `n` schreibgeschützt (ein zweites Mal: nur dorthin springen).
    pub fn open_version(&mut self, path: &NodePath, n: u32) -> ProjectResult<usize> {
        if let Some(i) = self.tabs.iter().position(|t| t.read_only && t.path.as_ref() == Some(path) && t.based_on == Some(n)) {
            self.active = i;
            return Ok(i);
        }
        let lib = self
            .library
            .as_ref()
            .ok_or_else(|| ProjectError::Invalid("Es ist noch kein Projektordner gewählt.".into()))?;
        let dir = lib.document_dir(path)?;
        let snapshot = match self.tab_for_path(path).and_then(|i| self.tabs[i].store.as_ref()) {
            Some(store) => store.load_version(n)?,
            None => read_version(&dir, n)?,
        };
        let state = AppState::from_snapshot(snapshot).map_err(ProjectError::Invalid)?;
        let id = self.tab_id();
        let name = path.last().map(|p| p.trim_end_matches(".pcad").to_string()).unwrap_or_default();
        let mut tab = DocTab::new(id, name, state);
        tab.path = Some(path.clone());
        tab.source_dir = Some(dir);
        tab.based_on = Some(n);
        tab.read_only = true;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        Ok(self.active)
    }

    /// Macht eine schreibgeschützte Ansicht bearbeitbar (Abzweigen): holt die Sperre. Gibt es
    /// das Dokument schon beschreibbar in einem anderen Tab, ist das ein Fehler.
    pub fn make_editable(&mut self, i: usize) -> Result<(), String> {
        let tab = self.tabs.get(i).ok_or("Kein solcher Tab.")?;
        if !tab.read_only {
            return Ok(());
        }
        let path = tab.path.clone().ok_or("Das Dokument hat keinen Ort.")?;
        if self.tab_for_path(&path).is_some() {
            return Err("Das Dokument ist schon in einem anderen Tab geöffnet. Dort weiterarbeiten oder den Tab schließen.".into());
        }
        let dir = tab.source_dir.clone().ok_or("Das Dokument hat keinen Ort.")?;
        let (store, _) = DocStore::open(&dir, LockMode::Acquire).map_err(err)?;
        let tab = &mut self.tabs[i];
        tab.store = Some(store);
        tab.source_dir = None;
        tab.read_only = false;
        tab.mark_clean();
        Ok(())
    }

    /// Stellt Version `n` wieder her: neue Version am Zweig des beschreibbaren Tabs dieses
    /// Dokuments (bei einer Ansicht: im schon offenen beschreibbaren Tab, sonst wird die Ansicht
    /// bearbeitbar). Danach zeigt der beschreibbare Tab den Stand und ist aktiv. Ungespeicherte
    /// Änderungen im Zieltab verhindern das Wiederherstellen.
    pub fn restore_version(&mut self, i: usize, n: u32) -> Result<(), String> {
        let read_only = self.tabs.get(i).ok_or("Kein solcher Tab.")?.read_only;
        let target = if read_only {
            match self.tabs[i].path.as_ref().and_then(|p| self.tab_for_path(p)) {
                Some(j) => j,
                None => {
                    self.make_editable(i)?;
                    i
                }
            }
        } else {
            i
        };
        let tab = &mut self.tabs[target];
        if tab.is_dirty() {
            return Err("Es gibt ungespeicherte Änderungen. Zuerst speichern oder verwerfen.".into());
        }
        let store = tab.store.as_mut().ok_or("Das Dokument ist nicht gespeichert.")?;
        let branch = tab
            .based_on
            .and_then(|b| store.info().version(b))
            .map(|v| v.branch.clone())
            .unwrap_or_else(|| store.info().active_branch.clone());
        let new = store.restore(n, &branch).map_err(err)?;
        let snapshot = store.load_version(new).map_err(err)?;
        let mut state = AppState::from_snapshot(snapshot)?;
        state.dark = tab.state.dark;
        state.rebuild_scene();
        tab.state = state;
        tab.based_on = Some(new);
        tab.mark_clean();
        self.active = target;
        Ok(())
    }

    pub fn choose_root(&mut self, root: &Path) -> Result<(), String> {
        let lib = Library::open(root).map_err(err)?;
        self.library = Some(lib);
        self.config.root = Some(root.to_path_buf());
        Ok(())
    }

    /// Speichert die Konfiguration (gewählte Wurzel); Fehler sind nicht schlimm.
    pub fn persist_config(&self) {
        let _ = self.config.save();
    }

    /// Schreibt für geänderte, gespeicherte Dokumente eine Autosicherung, wenn das Intervall
    /// seit der letzten abgelaufen ist.
    pub fn autosave_tick(&mut self, now: Instant) {
        let interval = self.autosave_interval;
        for t in &mut self.tabs {
            if t.read_only || !t.is_dirty() || t.state.document.edit_count() == t.autosaved_edits {
                continue;
            }
            if now.saturating_duration_since(t.last_autosave) < interval {
                continue;
            }
            let Some(store) = &t.store else { continue };
            if store.write_autosave(t.based_on, &t.state.snapshot()).is_ok() {
                t.autosaved_edits = t.state.document.edit_count();
                t.last_autosave = now;
            }
        }
    }
}
```

`lib.rs`: `pub mod workspace;`. In `crates/planora-app/Cargo.toml`: `planora-project.workspace = true`. `AppState::rebuild_scene` ist bereits `pub`. `Autosave`, `peek` und `read_version` sind in Task 7 `pub` unter `planora_project::docstore`.

- [ ] **Step 5: Tests grün, fmt, clippy, Commit**

```bash
cargo test -p planora-app && cargo fmt && cargo clippy -p planora-app --all-targets -- -D warnings
git add -A && git commit -m "feat: Workspace mit Tabs, Speichern, Öffnen, Versionen und Autosicherung (Logik)"
```

---

### Task 9: Tab-Leiste, Fenster für Rückfragen, Datei-Menü, Einbindung in die App

**Files:**
- Create: `crates/planora-app/src/tabs.rs`, `crates/planora-app/src/prompts.rs`
- Modify: `crates/planora-app/src/{app.rs,lib.rs}`
- Test: `crates/planora-app/tests/tabs_ui.rs`

**Interfaces:**
- Consumes: `Workspace`, `Prompt`, `After`, `SaveOutcome`, bestehende Panels `browser::show`, `properties::show`, `sketch_panel::show`, `viewport::show`.
- Produces:
  ```rust
  // tabs.rs
  pub fn show(ui: &mut egui::Ui, ws: &mut Workspace);                // Tab-Leiste; Klick = aktivieren, × = request_close, + = neuer Tab
  // prompts.rs
  pub fn show(ctx: &egui::Context, ws: &mut Workspace);              // zeigt ws.prompt als Fenster und führt die Knöpfe aus
  // app.rs: PlanoraApp hält `ws: Workspace` statt `state` + Dialogen; Datei-Menü; Tastenkürzel
  ```

- [ ] **Step 1: Tests** (`crates/planora-app/tests/tabs_ui.rs`)

```rust
use planora_app::workspace::*;
use planora_app::{fonts, prompts, tabs, testing::collect_texts};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn ws() -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-tabs-{}", new_id()));
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn frame(c: &egui::Context, f: impl FnMut(&mut egui::Ui)) -> String {
    let out = c.run_ui(egui::RawInput::default(), f);
    let t = collect_texts(&out);
    out.drop_without_applying_deltas();
    t
}

#[test]
fn the_tab_bar_lists_tabs_and_marks_changes() {
    let c = ctx();
    let mut w = ws();
    let t = frame(&c, |ui| tabs::show(ui, &mut w));
    assert!(t.contains("Unbenannt 1"), "{t}");
    assert!(!t.contains('●'));
    w.active_tab_mut().state.create_box(DVec3::splat(4.0)).unwrap();
    w.new_tab();
    let t = frame(&c, |ui| tabs::show(ui, &mut w));
    assert!(t.contains("Unbenannt 1 ●"), "{t}");
    assert!(t.contains("Unbenannt 2"), "{t}");
}

#[test]
fn a_read_only_tab_shows_its_version_in_the_title() {
    let c = ctx();
    let mut w = ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "").unwrap();
    let path = w.active_tab().path.clone().unwrap();
    w.open_version(&path, 1).unwrap();
    let t = frame(&c, |ui| tabs::show(ui, &mut w));
    assert!(t.contains("Teil · v1"), "{t}");
}

#[test]
fn the_save_changes_prompt_names_the_tab_and_offers_three_choices() {
    let c = ctx();
    let mut w = ws();
    w.active_tab_mut().state.create_box(DVec3::splat(4.0)).unwrap();
    assert!(!w.request_close(0));
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    for must in ["Änderungen speichern", "Unbenannt 1", "Speichern", "Verwerfen", "Abbrechen"] {
        assert!(t.contains(must), "„{must}“ fehlt:\n{t}");
    }
}

#[test]
fn the_other_prompts_render_their_fields() {
    let c = ctx();
    let mut w = ws();
    let id = w.active_tab().id;
    w.prompt = Some(Prompt::SaveComment { tab: id, then: None, comment: String::new(), branch: Some(String::new()) });
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("Zweig") && t.contains("Kommentar"), "{t}");
    w.prompt = Some(Prompt::SaveAs { tab: id, then: None, parent: vec![], name: "X".into(), comment: String::new() });
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("Speichern unter") && t.contains("Name"), "{t}");
    w.prompt = Some(Prompt::Notice("Etwas ist schiefgegangen".into()));
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("Etwas ist schiefgegangen"), "{t}");
    w.prompt = Some(Prompt::TakeOver { path: vec!["P".into(), "T.pcad".into()], version: None });
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("gesperrt") || t.contains("benutzt"), "{t}");
    w.prompt = Some(Prompt::AutosaveOffer { tab: id });
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("Wiederherstellen") && t.contains("Verwerfen"), "{t}");
}
```

- [ ] **Step 2: Implementierung `tabs.rs`**

```rust
//! Tab-Leiste über der 3D-Ansicht.

use crate::workspace::Workspace;

pub fn show(ui: &mut egui::Ui, ws: &mut Workspace) {
    let mut activate = None;
    let mut close = None;
    let mut add = false;
    ui.horizontal(|ui| {
        for (i, t) in ws.tabs.iter().enumerate() {
            let label = if t.is_dirty() { format!("{} ●", t.title()) } else { t.title() };
            if ui.selectable_label(i == ws.active, label).clicked() {
                activate = Some(i);
            }
            if ui.small_button("×").on_hover_text("Schließen (Strg+W)").clicked() {
                close = Some(i);
            }
            ui.add_space(6.0);
        }
        if ui.small_button("+").on_hover_text("Neues Dokument (Strg+N)").clicked() {
            add = true;
        }
    });
    if let Some(i) = activate {
        ws.activate(i);
    }
    if let Some(i) = close {
        ws.request_close(i);
    }
    if add {
        ws.new_tab();
    }
}
```

- [ ] **Step 3: Implementierung `prompts.rs`** – ein Fenster je Prompt; die Knöpfe rufen nur Methoden des `Workspace` auf. Muster:

```rust
//! Fenster für Rückfragen des Arbeitsbereichs.

use crate::workspace::{Prompt, SaveOutcome, Workspace};
use planora_project::docstore::LockMode;

pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    let Some(prompt) = ws.prompt.clone() else { return };
    match prompt {
        Prompt::SaveChanges { tab, .. } => {
            let name = ws.tab_index(tab).map(|i| ws.tabs[i].title()).unwrap_or_default();
            let mut choice = None;
            egui::Window::new("Änderungen speichern?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("„{name}“ hat ungespeicherte Änderungen."));
                    ui.horizontal(|ui| {
                        if ui.button("Speichern").clicked() {
                            choice = Some(0);
                        }
                        if ui.button("Verwerfen").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("Abbrechen").clicked() {
                            choice = Some(2);
                        }
                    });
                });
            match choice {
                Some(0) => ws.save_for_prompt(),
                Some(1) => ws.discard_and_continue(),
                Some(2) => ws.prompt = None,
                _ => {}
            }
        }
        Prompt::SaveComment { tab, then, mut comment, mut branch } => {
            let mut done = None;
            egui::Window::new("Version speichern")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    if let Some(b) = branch.as_mut() {
                        ui.label("Diese Version ist nicht die neueste ihres Zweigs. Der Stand wird in einem neuen Zweig gespeichert.");
                        ui.horizontal(|ui| {
                            ui.label("Zweig:");
                            ui.text_edit_singleline(b);
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("Kommentar:");
                        ui.text_edit_singleline(&mut comment);
                    });
                    ui.horizontal(|ui| {
                        let ok = branch.as_ref().is_none_or(|b| !b.trim().is_empty());
                        if ui.add_enabled(ok, egui::Button::new("Speichern")).clicked() {
                            done = Some(true);
                        }
                        if ui.button("Abbrechen").clicked() {
                            done = Some(false);
                        }
                    });
                });
            match done {
                Some(true) => {
                    ws.prompt = None;
                    if let Some(i) = ws.tab_index(tab) {
                        ws.active = i;
                        match ws.save_active(&comment, branch.as_deref().map(str::trim)) {
                            Ok(SaveOutcome::Saved(_)) => ws.finish_after(tab, then),
                            // Stand ist nicht die Spitze: Zweignamen nachfragen
                            Ok(SaveOutcome::NeedsBranch) => {
                                ws.prompt = Some(Prompt::SaveComment { tab, then, comment, branch: Some(String::new()) });
                            }
                            Ok(SaveOutcome::NeedsLocation) => {
                                let name = ws.tabs[i].name.clone();
                                ws.prompt = Some(Prompt::SaveAs { tab, then, parent: Vec::new(), name, comment });
                            }
                            Err(e) => ws.prompt = Some(Prompt::Notice(e)),
                        }
                    }
                }
                Some(false) => ws.prompt = None,
                None => ws.prompt = Some(Prompt::SaveComment { tab, then, comment, branch }),
            }
        }
        // SaveAs, TakeOver, AutosaveOffer, Notice: gleiches Muster.
        other => show_other(ctx, ws, other),
    }
}
```

`show_other` behandelt die übrigen Varianten nach demselben Muster (Fenster mit `anchor(CENTER_CENTER)`, Knöpfe → Workspace-Methoden; Eingabefelder bleiben erhalten, indem der Prompt geklont, bearbeitet und zurückgeschrieben wird, wie oben bei `SaveComment`):
- **SaveAs** („Speichern unter“): Auswahl des Ziels als aufklappbarer Baum aus `ws.library.tree()` (nur `Project`/`Folder`-Knoten sind wählbar, der gewählte Pfad steht in `parent`), Textfelder „Name“ und „Kommentar“, Knöpfe „Speichern“ (aktiv nur mit gewähltem Ziel und nichtleerem Namen; ruft nach `ws.activate(tab_index)` `ws.save_as(&parent, &name, &comment)`; bei Erfolg `ws.finish_after(tab, then)`, bei Fehler `Prompt::Notice(e)`) und „Abbrechen“. Ohne Bibliothek zeigt das Fenster „Zuerst im Projekt-Panel einen Projektordner wählen.“.
- **TakeOver**: Text „Das Dokument wird gerade von einem anderen Programm benutzt (gesperrt). Falls das Programm abgestürzt ist, kannst du die Sperre übernehmen.“, Knöpfe „Sperre übernehmen“ (`ws.open_document(&path, LockMode::TakeOver)`, bei `version = Some(n)` danach `ws.open_version`; Fehler als `Notice`) und „Abbrechen“.
- **AutosaveOffer**: Text „Für dieses Dokument gibt es eine Autosicherung, die neuer ist als der gespeicherte Stand (zum Beispiel nach einem Absturz).“, Knöpfe „Wiederherstellen“ (`ws.apply_autosave(tab)`) und „Verwerfen“ (`ws.discard_autosave(tab)`).
- **Notice**: Text und ein Knopf „OK“.
Die Tests oben prüfen die Texte „Speichern unter“, „Name“, „Zweig“, „Kommentar“, „gesperrt“ bzw. „benutzt“, „Wiederherstellen“, „Verwerfen“.

- [ ] **Step 4: `app.rs` umbauen.** `PlanoraApp` hält `ws: Workspace` statt `state`, `dialog`, `boolean_dialog`, `feature_dialog`. In `new`:

```rust
let config = planora_project::config::Config::load();
let root = config.root.clone().unwrap_or_else(planora_project::config::Config::default_root);
let library = planora_project::library::Library::open(&root).ok();
let ws = Workspace::new(library, config);
```
(`state.frame_all()` entfällt; `Workspace::new_tab` passt die Ansicht an.) `Behavior` hält `state: &mut AppState` des aktiven Tabs und `read_only: bool`; die Bereiche Browser und Eigenschaften laufen in `ui.add_enabled_ui(!read_only, …)`. Im Menü (Methoden `menu`, `sketch_menu`) ersetzt der aktive Tab (`self.ws.active_tab_mut()`) den Zugriff auf `self.state`; die Menüs „Erzeugen“ und „Bearbeiten“ werden bei `read_only` mit `add_enabled_ui(false, …)` gesperrt. Der Dialogcode in `ui` arbeitet auf dem aktiven Tab (`tab.dialog`, `tab.boolean_dialog`, `tab.feature_dialog`). Schreibgeschützte Tabs zeigen unter der Tab-Leiste eine Hinweiszeile „Alte Version v{n} – schreibgeschützt. Im Bereich „Versionen“ lässt sie sich wiederherstellen oder abzweigen.“.

Menü **Datei** (ersetzt den bisherigen Inhalt):

```rust
ui.menu_button("Datei", |ui| {
    if ui.button("Neu              Strg+N").clicked() { self.ws.new_tab(); ui.close(); }
    if ui.button("Öffnen …         Strg+O").clicked() { self.show_projects(); ui.close(); }
    ui.separator();
    if ui.add_enabled(!self.ws.active_tab().read_only, egui::Button::new("Speichern       Strg+S")).clicked() {
        self.begin_save(false); ui.close();
    }
    if ui.add_enabled(!self.ws.active_tab().read_only, egui::Button::new("Speichern unter …")).clicked() {
        self.begin_save(true); ui.close();
    }
    ui.separator();
    if ui.button("Schließen        Strg+W").clicked() { let i = self.ws.active; self.ws.request_close(i); ui.close(); }
    if ui.button("Beenden").clicked() {
        if self.ws.request_quit() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close); }
        ui.close();
    }
});
```

`show_projects()` ist bis Task 10 ein leerer Platzhalter-Aufruf (`fn show_projects(&mut self) {}`), den Task 10 mit `self.dock.set_visible(Pane::Projects, true)` füllt. Mit

```rust
/// Strg+S / Speichern: fragt nach Kommentar (Dialog), Ort oder Zweig.
fn begin_save(&mut self, as_new: bool) {
    let tab = self.ws.active_tab();
    let (id, has_store, name) = (tab.id, tab.store.is_some(), tab.name.clone());
    self.ws.prompt = Some(if as_new || !has_store {
        Prompt::SaveAs { tab: id, then: None, parent: Vec::new(), name, comment: String::new() }
    } else {
        Prompt::SaveComment { tab: id, then: None, comment: String::new(), branch: None }
    });
}
```

**Kürzel** (in `ui`, nur wenn `!viewport::typing(ctx)`; die Funktion ist dort vorhanden): `Strg+N` neuer Tab, `Strg+O` Projekte zeigen, `Strg+S` `begin_save(false)`, `Strg+W` Tab schließen. **Beenden:** `ctx.input(|i| i.viewport().close_requested())` → ist `self.ws.quit` nicht gesetzt und `!self.ws.request_quit()`, dann `ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose)`. **Autosicherung:** am Ende von `ui`: `self.ws.autosave_tick(Instant::now()); ui.ctx().request_repaint_after(Duration::from_secs(30));`. **Design:** `applied_dark` vergleicht gegen `self.ws.active_tab().state.dark`; der Menüpunkt Hell/Dunkel ruft `self.ws.set_dark(!dark)`.

Zusätzlich in `ui`: `prompts::show(ui.ctx(), &mut self.ws)` nach den Dialogen. Die Tab-Leiste steht in einem `egui::Panel::top("tabs")` unter dem Menü (über allen Docking-Bereichen).

- [ ] **Step 5: Tests grün, fmt, clippy; alle Tests der App (`cargo test -p planora-app`) müssen grün bleiben** (sie benutzen `AppState` direkt und sind von der Umstellung der App-Hülle nicht betroffen).

- [ ] **Step 6: Handprüfung (soweit machbar) mit `tools/gui-drive.ps1`**: App starten, Quader anlegen, im Tab-Kopf erscheint „●“; „+“ legt einen zweiten Tab an; Wechsel zeigt jeweils den eigenen Inhalt; Schließen des geänderten Tabs fragt nach. Ergebnis ehrlich berichten (auch was nicht prüfbar war).

- [ ] **Step 7: Commit**

```bash
cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add -A && git commit -m "feat: Tab-Leiste, Rückfragefenster, Datei-Menü und Arbeitsbereich in der App"
```

---

### Task 10: Projekt-Panel (andockbar)

**Files:**
- Create: `crates/planora-app/src/projects_panel.rs`, `crates/planora-app/src/dock.rs`
- Modify: `crates/planora-app/src/{app.rs,lib.rs}` (Docking in `dock.rs` auslagern, Panes `Projects` und `Versions`, Menü „Ansicht“)
- Test: `crates/planora-app/tests/projects_ui.rs`, `crates/planora-app/tests/dock.rs`

**Interfaces:**
- Consumes: `Workspace`, `Library`, `TreeNode`, `NodeKind`, `Prompt`, `LockMode`, `ProjectError`.
- Produces:
  ```rust
  // dock.rs
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum Pane { Projects, Browser, Viewport, Properties, Versions }
  pub struct Dock { pub tree: Tree<Pane>, pub projects: TileId, pub versions: TileId }
  impl Dock { pub fn new() -> Dock;                         // Projekte | Browser | 3D | Eigenschaften+Versionen (Tabs)
              pub fn set_visible(&mut self, which: Pane, visible: bool);   // nur Projects/Versions
              pub fn is_visible(&self, which: Pane) -> bool; }
  // projects_panel.rs
  #[derive(Default)] pub struct ProjectsUi { /* Baum-Zwischenspeicher, Auswahl, Eingaben */ }
  impl ProjectsUi { pub fn invalidate(&mut self); pub fn selected(&self) -> Option<&NodePath>; }
  pub fn show(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut ProjectsUi);
  // Aktionen (auch ohne Oberfläche aufrufbar und getestet); Fehler landen als Prompt::Notice:
  pub fn open_node(ws: &mut Workspace, path: &NodePath);                       // Dokument öffnen (Sperre → Prompt::TakeOver)
  pub fn create_project(ws: &mut Workspace, st: &mut ProjectsUi, name: &str) -> Option<NodePath>;
  pub fn create_folder(ws: &mut Workspace, st: &mut ProjectsUi, parent: &NodePath, name: &str) -> Option<NodePath>;
  pub fn rename_node(ws: &mut Workspace, st: &mut ProjectsUi, path: &NodePath, name: &str) -> Option<NodePath>;
  pub fn move_node(ws: &mut Workspace, st: &mut ProjectsUi, path: &NodePath, to: &NodePath) -> Option<NodePath>;
  pub fn delete_node(ws: &mut Workspace, st: &mut ProjectsUi, path: &NodePath) -> bool;
  ```
  Regeln: Nach jeder Aktion wird der Baum-Zwischenspeicher ungültig. Offene Tabs eines umbenannten, verschobenen oder gelöschten Dokuments bleiben nicht still falsch: `rename_node`/`move_node` für ein Dokument oder einen Ordner, der offene Dokumente enthält, wird mit `Prompt::Notice("Bitte zuerst die betroffenen Dokumente schließen.")` abgelehnt; `delete_node` ebenso. Der Baum wird höchstens bei Änderung oder auf „Aktualisieren“ neu gelesen (nicht jedes Bild).

- [ ] **Step 1: Tests** (`crates/planora-app/tests/projects_ui.rs`)

```rust
use planora_app::projects_panel::*;
use planora_app::workspace::*;
use planora_app::{fonts, testing::collect_texts};
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn ws() -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-pp-{}", new_id()));
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn frame(c: &egui::Context, ws: &mut Workspace, st: &mut ProjectsUi) -> String {
    let out = c.run_ui(egui::RawInput::default(), |ui| show(ui, ws, st));
    let t = collect_texts(&out);
    out.drop_without_applying_deltas();
    t
}

fn notice(w: &Workspace) -> String {
    match &w.prompt {
        Some(Prompt::Notice(m)) => m.clone(),
        other => panic!("Hinweis erwartet, war {other:?}"),
    }
}

#[test]
fn the_panel_shows_the_empty_state_and_then_the_tree() {
    let c = ctx();
    let mut w = ws();
    let mut st = ProjectsUi::default();
    let t = frame(&c, &mut w, &mut st);
    assert!(t.contains("Noch keine Projekte"), "{t}");
    let p = create_project(&mut w, &mut st, "Gehäuse").unwrap();
    let f = create_folder(&mut w, &mut st, &p, "Deckel").unwrap();
    w.save_as(&f, "Oben", "").unwrap();
    let t = frame(&c, &mut w, &mut st);
    for must in ["Gehäuse", "Deckel", "Oben"] {
        assert!(t.contains(must), "„{must}“ fehlt:\n{t}");
    }
}

#[test]
fn a_panel_action_with_a_bad_name_shows_a_notice_and_changes_nothing() {
    let mut w = ws();
    let mut st = ProjectsUi::default();
    assert!(create_project(&mut w, &mut st, "..").is_none());
    assert!(notice(&w).contains("Name"));
    w.prompt = None;
    let p = create_project(&mut w, &mut st, "P").unwrap();
    assert!(create_project(&mut w, &mut st, "P").is_none(), "doppelt");
    assert!(create_folder(&mut w, &mut st, &vec![], "X").is_none());
    assert_eq!(w.library.as_ref().unwrap().tree().unwrap().len(), 1);
    let _ = p;
}

#[test]
fn opening_a_document_creates_a_tab_and_a_locked_one_asks_to_take_over() {
    let mut w = ws();
    let mut st = ProjectsUi::default();
    let p = create_project(&mut w, &mut st, "P").unwrap();
    let doc = w.save_as(&p, "Teil", "").unwrap();
    w.close_tab(0);
    open_node(&mut w, &doc);
    assert_eq!(w.tabs.len(), 1);
    assert_eq!(w.active_tab().title(), "Teil");
    // zweites Programm
    let mut other = Workspace::new(Some(Library::open(w.library.as_ref().unwrap().root().path()).unwrap()), Config::default());
    open_node(&mut other, &doc);
    assert!(matches!(other.prompt, Some(Prompt::TakeOver { .. })), "{:?}", other.prompt);
    assert_eq!(other.tabs.len(), 1);
}

#[test]
fn renaming_moving_and_deleting_documents_in_open_tabs_is_refused() {
    let mut w = ws();
    let mut st = ProjectsUi::default();
    let p = create_project(&mut w, &mut st, "P").unwrap();
    let f = create_folder(&mut w, &mut st, &p, "F").unwrap();
    let doc = w.save_as(&p, "Teil", "").unwrap();
    assert!(rename_node(&mut w, &mut st, &doc, "Neu").is_none());
    assert!(notice(&w).contains("schließen"));
    w.prompt = None;
    assert!(move_node(&mut w, &mut st, &doc, &f).is_none());
    w.prompt = None;
    assert!(!delete_node(&mut w, &mut st, &p), "Projekt enthält ein offenes Dokument");
    w.prompt = None;
    w.close_tab(0);
    let moved = move_node(&mut w, &mut st, &doc, &f).unwrap();
    assert_eq!(moved.len(), 3);
    let renamed = rename_node(&mut w, &mut st, &moved, "Neu").unwrap();
    assert_eq!(renamed.last().unwrap(), "Neu.pcad");
    assert!(delete_node(&mut w, &mut st, &renamed));
    assert!(w.library.as_ref().unwrap().tree().unwrap()[0].children[0].children.is_empty());
}

#[test]
fn the_tree_is_not_reread_every_frame() {
    let c = ctx();
    let mut w = ws();
    let mut st = ProjectsUi::default();
    create_project(&mut w, &mut st, "A").unwrap();
    frame(&c, &mut w, &mut st);
    // direkt auf der Platte angelegt: erscheint erst nach „Aktualisieren“ (invalidate)
    w.library.as_ref().unwrap().create_project("B").unwrap();
    let t = frame(&c, &mut w, &mut st);
    assert!(!t.contains('B') || t.contains("Aktualisieren"), "{t}");
    st.invalidate();
    let t = frame(&c, &mut w, &mut st);
    assert!(t.contains("B"), "{t}");
}

#[test]
fn without_a_library_the_panel_asks_for_a_root() {
    let c = ctx();
    let mut w = Workspace::new(None, Config::default());
    let mut st = ProjectsUi::default();
    let t = frame(&c, &mut w, &mut st);
    assert!(t.contains("Projektordner"), "{t}");
}
```

`crates/planora-app/tests/dock.rs`:

```rust
use planora_app::dock::{Dock, Pane};

#[test]
fn projects_and_versions_can_be_hidden_and_shown() {
    let mut d = Dock::new();
    assert!(d.is_visible(Pane::Projects));
    assert!(d.is_visible(Pane::Versions));
    d.set_visible(Pane::Projects, false);
    assert!(!d.is_visible(Pane::Projects));
    assert!(d.is_visible(Pane::Versions));
    d.set_visible(Pane::Projects, true);
    assert!(d.is_visible(Pane::Projects));
    // die festen Bereiche lassen sich nicht ausblenden
    d.set_visible(Pane::Browser, false);
    assert!(d.is_visible(Pane::Browser));
}
```

- [ ] **Step 2: Implementierung `dock.rs`** (aus `app.rs` verschoben und erweitert)

```rust
//! Anordnung der Bereiche (Docking mit `egui_tiles`).

use egui_tiles::{Container, TileId, Tiles, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pane {
    Projects,
    Browser,
    Viewport,
    Properties,
    Versions,
}

pub struct Dock {
    pub tree: Tree<Pane>,
    pub projects: TileId,
    pub versions: TileId,
}

impl Default for Dock {
    fn default() -> Self {
        Dock::new()
    }
}

impl Dock {
    /// Anfangsanordnung: Projekte (14 %), Browser (16 %), 3D-Ansicht (46 %), rechts Tabs
    /// „Eigenschaften“ und „Versionen“ (24 %).
    pub fn new() -> Dock {
        let mut tiles = Tiles::default();
        let projects_pane = tiles.insert_pane(Pane::Projects);
        let projects = tiles.insert_tab_tile(vec![projects_pane]);
        let browser_pane = tiles.insert_pane(Pane::Browser);
        let browser = tiles.insert_tab_tile(vec![browser_pane]);
        let viewport_pane = tiles.insert_pane(Pane::Viewport);
        let viewport = tiles.insert_tab_tile(vec![viewport_pane]);
        let properties_pane = tiles.insert_pane(Pane::Properties);
        let versions_pane = tiles.insert_pane(Pane::Versions);
        let versions = versions_pane;
        let right = tiles.insert_tab_tile(vec![properties_pane, versions_pane]);
        let root = tiles.insert_horizontal_tile(vec![projects, browser, viewport, right]);
        if let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tiles.get_mut(root) {
            // Anteile; Summe 4: 14 %, 16 %, 46 %, 24 %.
            linear.shares.set_share(projects, 0.56);
            linear.shares.set_share(browser, 0.64);
            linear.shares.set_share(viewport, 1.84);
            linear.shares.set_share(right, 0.96);
        }
        Dock { tree: Tree::new("planora_docking", root, tiles), projects, versions }
    }

    /// Blendet „Projekte“ oder „Versionen“ aus oder ein; andere Bereiche bleiben immer sichtbar.
    pub fn set_visible(&mut self, which: Pane, visible: bool) {
        let id = match which {
            Pane::Projects => self.projects,
            Pane::Versions => self.versions,
            _ => return,
        };
        self.tree.tiles.set_visible(id, visible);
    }

    pub fn is_visible(&self, which: Pane) -> bool {
        match which {
            Pane::Projects => self.tree.tiles.is_visible(self.projects),
            Pane::Versions => self.tree.tiles.is_visible(self.versions),
            _ => true,
        }
    }
}
```

In `app.rs` wird `Pane`/`build_docking` entfernt und durch `use crate::dock::{Dock, Pane};` ersetzt; `PlanoraApp` hält `dock: Dock`, `projects_ui: ProjectsUi`, `versions_ui: VersionsUi` (Task 11); `Behavior::tab_title_for_pane` bekommt „Projekte“ und „Versionen“, `pane_ui` ruft `projects_panel::show(ui, self.ws, self.projects_ui)` bzw. (Task 11) `versions_panel::show`. `self.dock.ui(…)` → `self.dock.tree.ui(…)`. Die Pane-Reihenfolge `Properties` und `Versions` teilt sich einen Tab-Container; `egui_tiles`-API-Namen (`set_visible`, `is_visible`) beim Kompilieren gegen Version 0.17 prüfen und notfalls über `Tiles::set_visible`/`Tiles::is_visible` (Namen können abweichen) anpassen; schlägt das fehl, die Sichtbarkeit selbst führen (Feld `hidden: BTreeSet<Pane>` und beim Zeichnen den Bereich überspringen). Menü **Ansicht**: zwei Schalter „Projekte“ und „Versionen“ (`ui.checkbox`) rufen `dock.set_visible`.

- [ ] **Step 3: Implementierung `projects_panel.rs`.** Zustand und Aktionen:

```rust
//! Panel „Projekte“: Baum Projekt → Ordner → Dokument.

use crate::workspace::{Prompt, Workspace};
use planora_project::ProjectError;
use planora_project::docstore::LockMode;
use planora_project::library::{NodeKind, NodePath, TreeNode};

#[derive(Default)]
pub struct ProjectsUi {
    tree: Option<Vec<TreeNode>>,
    selected: Option<NodePath>,
    new_name: String,
    rename: Option<(NodePath, String)>,
    moving: Option<(NodePath, Option<NodePath>)>,
    confirm_delete: Option<NodePath>,
    root_text: Option<String>,
}

impl ProjectsUi {
    pub fn invalidate(&mut self) {
        self.tree = None;
    }
    pub fn selected(&self) -> Option<&NodePath> {
        self.selected.as_ref()
    }
}

fn notice(ws: &mut Workspace, msg: impl Into<String>) {
    ws.prompt = Some(Prompt::Notice(msg.into()));
}

fn open_tabs_under(ws: &Workspace, path: &NodePath) -> bool {
    ws.tabs.iter().any(|t| t.path.as_ref().is_some_and(|p| p.starts_with(path)))
}

pub fn open_node(ws: &mut Workspace, path: &NodePath) {
    match ws.open_document(path, LockMode::Acquire) {
        Ok(r) => {
            if !r.notes.is_empty() && !r.autosave_offered {
                notice(ws, r.notes.join("\n"));
            }
        }
        Err(ProjectError::Locked { .. }) => ws.prompt = Some(Prompt::TakeOver { path: path.clone(), version: None }),
        Err(e) => notice(ws, e.to_string()),
    }
}

fn lib<'a>(ws: &'a mut Workspace) -> Option<&'a planora_project::library::Library> {
    if ws.library.is_none() {
        ws.prompt = Some(Prompt::Notice("Es ist noch kein Projektordner gewählt.".into()));
    }
    ws.library.as_ref()
}

pub fn create_project(ws: &mut Workspace, st: &mut ProjectsUi, name: &str) -> Option<NodePath> {
    let r = lib(ws)?.create_project(name);
    st.invalidate();
    r.map_err(|e| notice(ws, e.to_string())).ok()
}

pub fn create_folder(ws: &mut Workspace, st: &mut ProjectsUi, parent: &NodePath, name: &str) -> Option<NodePath> {
    let r = lib(ws)?.create_folder(parent, name);
    st.invalidate();
    r.map_err(|e| notice(ws, e.to_string())).ok()
}

pub fn rename_node(ws: &mut Workspace, st: &mut ProjectsUi, path: &NodePath, name: &str) -> Option<NodePath> {
    if open_tabs_under(ws, path) {
        notice(ws, "Bitte zuerst die betroffenen Dokumente schließen.");
        return None;
    }
    let r = lib(ws)?.rename(path, name);
    st.invalidate();
    r.map_err(|e| notice(ws, e.to_string())).ok()
}

pub fn move_node(ws: &mut Workspace, st: &mut ProjectsUi, path: &NodePath, to: &NodePath) -> Option<NodePath> {
    if open_tabs_under(ws, path) {
        notice(ws, "Bitte zuerst die betroffenen Dokumente schließen.");
        return None;
    }
    let r = lib(ws)?.move_node(path, to);
    st.invalidate();
    r.map_err(|e| notice(ws, e.to_string())).ok()
}

pub fn delete_node(ws: &mut Workspace, st: &mut ProjectsUi, path: &NodePath) -> bool {
    if open_tabs_under(ws, path) {
        notice(ws, "Bitte zuerst die betroffenen Dokumente schließen.");
        return false;
    }
    let Some(l) = lib(ws) else { return false };
    let r = l.delete(path);
    st.invalidate();
    match r {
        Ok(()) => true,
        Err(e) => {
            notice(ws, e.to_string());
            false
        }
    }
}
```

`show` zeichnet: Kopf mit dem Wurzelpfad (kleiner Text) und den Knöpfen „Neues Projekt“ (öffnet ein Eingabefeld `new_name` + „Anlegen“), „Aktualisieren“ (`st.invalidate()`), „Wurzel wählen …“ (Textfeld `root_text` mit Pfad + „Übernehmen“: `ws.choose_root(path)`, bei Erfolg `ws.persist_config(); st.invalidate()`, sonst Hinweis). Ohne Bibliothek: Text „Es ist noch kein Projektordner gewählt. Gib unten einen Ordner ein, in dem deine Projekte liegen sollen.“ und das Wurzelfeld. Mit leerem Baum: „Noch keine Projekte. Lege mit „Neues Projekt“ das erste an.“ Baum: für `Project`/`Folder` ein `egui::CollapsingHeader` (id aus dem Pfad), für `Document` ein `selectable_label`; Klick wählt (`st.selected`), Doppelklick auf ein Dokument ruft `open_node`; Rechtsklick-Menü (`response.context_menu`) je nach Art: *Neuer Ordner …* (Projekt/Ordner; Eingabefenster mit Namensfeld), *Umbenennen …*, *Verschieben …* (nicht Projekte; Fenster mit `ComboBox` aller Container außer dem Knoten selbst und seinen Nachfahren, gebildet aus dem Baum), *Löschen …* (Rückfragefenster „In den Papierkorb verschieben?“). Offene Dokumente werden im Baum fett markiert (`ws.tab_for_path`). Der Baum wird in `st.tree` zwischengespeichert und nur bei `None` über `ws.library.tree()` neu gelesen; Lesefehler erscheinen als roter Text im Panel (kein Panic).

- [ ] **Step 4: Tests grün, fmt, clippy, Commit**

```bash
cargo test -p planora-app && cargo fmt && cargo clippy --workspace --all-targets -- -D warnings
git add -A && git commit -m "feat: andockbares Projekt-Panel mit Projekt-, Ordner- und Dokumentaktionen"
```

---

### Task 11: Versionen-Panel

**Files:**
- Create: `crates/planora-app/src/versions_panel.rs`
- Modify: `crates/planora-app/src/{app.rs,lib.rs,dock.rs}` (Pane `Versions` zeigen)
- Test: `crates/planora-app/tests/versions_ui.rs`

**Interfaces:**
- Consumes: `Workspace`, `DocTab::version_info`, `DocumentInfo`, `VersionInfo`, `DocStore` (Löschen, Kommentar).
- Produces:
  ```rust
  #[derive(Default)] pub struct VersionsUi { /* Kommentar-Entwurf */ }
  pub fn show(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut VersionsUi);
  // Aktionen (getestet, ohne Oberfläche); Fehler → Prompt::Notice
  pub fn open_version(ws: &mut Workspace, n: u32);                // schreibgeschützter Tab
  pub fn restore(ws: &mut Workspace, n: u32);
  pub fn branch_from(ws: &mut Workspace, n: u32);                 // Ansicht öffnen und bearbeitbar machen
  pub fn set_comment(ws: &mut Workspace, n: u32, text: &str);
  pub fn delete_version(ws: &mut Workspace, n: u32);
  pub fn delete_branch(ws: &mut Workspace, name: &str);
  pub fn switch_branch(ws: &mut Workspace, name: &str);           // aktiven Zweig setzen und dessen Spitze in einem Tab öffnen
  pub fn rows(info: &DocumentInfo) -> Vec<Row>;                   // Anzeigereihenfolge (neueste oben) mit Einrückung je Zweig
  pub struct Row { pub number: u32, pub branch: String, pub depth: usize, pub is_tip: bool }
  ```
  Regeln: Zeilen sortiert nach Nummer absteigend; `depth` = Position des Zweigs in der Reihenfolge seiner ersten Version (Haupt = 0). Die Aktionen *Wiederherstellen*, *Kommentar*, *Löschen*, *Zweig löschen* arbeiten am beschreibbaren Tab des Dokuments (bei einer Ansicht am schon offenen beschreibbaren Tab, sonst Hinweis „Zum Ändern das Dokument zuerst bearbeitbar öffnen.“). *Löschen* einer Version/eines Zweigs fragt vorher nach, wenn der Tab geändert ist? Nein: der Versionsbaum ist vom Arbeitsstand unabhängig; das Löschen betrifft nur gespeicherte Versionen, verlangt aber eine Bestätigung im Panel („Version vN endgültig löschen?“). Eine geöffnete Ansicht der gelöschten Version bleibt als Tab erhalten (sie ist eine Kopie im Speicher); ihr Titel bleibt.

- [ ] **Step 1: Tests** (`crates/planora-app/tests/versions_ui.rs`)

```rust
use planora_app::versions_panel::*;
use planora_app::workspace::*;
use planora_app::{fonts, testing::collect_texts};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn ws_with_doc(versions: usize) -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-vp-{}", new_id()));
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    for i in 1..versions {
        w.active_tab_mut().state.create_box(DVec3::splat(2.0 + i as f64)).unwrap();
        w.save_active(&format!("Stand {}", i + 1), None).unwrap();
    }
    w
}

fn frame(c: &egui::Context, w: &mut Workspace, st: &mut VersionsUi) -> String {
    let out = c.run_ui(egui::RawInput::default(), |ui| show(ui, w, st));
    let t = collect_texts(&out);
    out.drop_without_applying_deltas();
    t
}

#[test]
fn an_unsaved_document_says_so() {
    let c = ctx();
    let mut w = Workspace::new(None, Config::default());
    let t = frame(&c, &mut w, &mut VersionsUi::default());
    assert!(t.contains("noch nicht gespeichert"), "{t}");
}

#[test]
fn the_list_shows_versions_comments_the_current_one_and_pending_changes() {
    let c = ctx();
    let mut w = ws_with_doc(3);
    let t = frame(&c, &mut w, &mut VersionsUi::default());
    for must in ["v1", "v2", "v3", "Haupt", "Anfang", "Stand 3"] {
        assert!(t.contains(must), "„{must}“ fehlt:\n{t}");
    }
    assert!(!t.contains("Änderungen"), "{t}");
    w.active_tab_mut().state.create_box(DVec3::splat(9.0)).unwrap();
    let t = frame(&c, &mut w, &mut VersionsUi::default());
    assert!(t.contains("Änderungen"), "{t}");
}

#[test]
fn rows_are_newest_first_and_indented_by_branch() {
    let mut w = ws_with_doc(2);
    let path = w.active_tab().path.clone().unwrap();
    open_version(&mut w, 1);
    branch_from(&mut w, 1);
    // jetzt eine Ansicht von v1, bearbeitbar? Nein: das Dokument ist im anderen Tab beschreibbar offen.
    assert!(matches!(w.prompt, Some(Prompt::Notice(_))));
    w.prompt = None;
    let i = w.tab_for_path(&path).unwrap();
    w.activate(i);
    let info = w.active_tab().version_info().unwrap();
    let rows = rows(&info);
    assert_eq!(rows.iter().map(|r| r.number).collect::<Vec<_>>(), [2, 1]);
    assert!(rows.iter().all(|r| r.depth == 0));
}

#[test]
fn restore_comment_and_delete_work_on_the_writable_tab() {
    let mut w = ws_with_doc(3);
    open_version(&mut w, 1);
    restore(&mut w, 1);
    assert!(w.prompt.is_none(), "{:?}", w.prompt);
    let t = w.tabs.iter().find(|t| !t.read_only).unwrap();
    assert_eq!(t.version_info().unwrap().versions.len(), 4);
    assert_eq!(t.state.bodies.len(), 0, "Stand von v1 hat noch keinen Körper");
    set_comment(&mut w, 4, "neu benannt");
    assert_eq!(w.tabs.iter().find(|t| !t.read_only).unwrap().version_info().unwrap().version(4).unwrap().comment, "neu benannt");
    delete_version(&mut w, 4);
    assert_eq!(w.tabs.iter().find(|t| !t.read_only).unwrap().version_info().unwrap().versions.len(), 3);
    delete_version(&mut w, 1);
    assert!(matches!(w.prompt, Some(Prompt::Notice(_))), "v1 hat Kinder");
}

#[test]
fn branching_from_a_closed_document_makes_an_editable_tab_and_saving_asks_for_a_branch() {
    let mut w = ws_with_doc(2);
    let path = w.active_tab().path.clone().unwrap();
    w.close_tab(w.tab_for_path(&path).unwrap());
    w.open_version(&path, 1).unwrap();
    branch_from(&mut w, 1);
    assert!(w.prompt.is_none(), "{:?}", w.prompt);
    assert!(!w.active_tab().read_only);
    w.active_tab_mut().state.create_box(DVec3::splat(7.0)).unwrap();
    assert_eq!(w.save_active("", None).unwrap(), SaveOutcome::NeedsBranch);
    assert_eq!(w.save_active("", Some("Variante")).unwrap(), SaveOutcome::Saved(3));
    delete_branch(&mut w, "Haupt");
    assert!(matches!(w.prompt, Some(Prompt::Notice(_))), "v1 hat Kinder in Variante");
    w.prompt = None;
    switch_branch(&mut w, "Haupt");
    assert!(w.prompt.is_none(), "{:?}", w.prompt);
}
```

(`Prompt` und `SaveOutcome` kommen aus `planora_app::workspace::*`.)

- [ ] **Step 2: Implementierung** (`versions_panel.rs`) – Aktionen:

```rust
//! Panel „Versionen“: Versionsbaum des aktiven Dokuments.

use crate::workspace::{Prompt, Workspace};
use planora_project::docstore::DocumentInfo;

#[derive(Default)]
pub struct VersionsUi {
    comment_edit: Option<(u32, String)>,
    confirm_delete: Option<Confirm>,
}

enum Confirm {
    Version(u32),
    Branch(String),
}

pub struct Row {
    pub number: u32,
    pub branch: String,
    pub depth: usize,
    pub is_tip: bool,
}

pub fn rows(info: &DocumentInfo) -> Vec<Row> {
    // Reihenfolge der Zweige nach ihrer ersten Version
    let mut order: Vec<&str> = Vec::new();
    for v in &info.versions {
        if !order.contains(&v.branch.as_str()) {
            order.push(&v.branch);
        }
    }
    let mut out: Vec<Row> = info
        .versions
        .iter()
        .map(|v| Row {
            number: v.number,
            branch: v.branch.clone(),
            depth: order.iter().position(|b| *b == v.branch).unwrap_or(0),
            is_tip: info.tip(&v.branch) == Some(v.number),
        })
        .collect();
    out.sort_by(|a, b| b.number.cmp(&a.number));
    out
}

fn notice(ws: &mut Workspace, msg: impl Into<String>) {
    ws.prompt = Some(Prompt::Notice(msg.into()));
}

fn active_path(ws: &Workspace) -> Option<planora_project::library::NodePath> {
    ws.active_tab().path.clone()
}

/// Index des beschreibbaren Tabs des aktiven Dokuments (aus einer Ansicht heraus der schon
/// offene beschreibbare Tab).
fn writable(ws: &mut Workspace) -> Option<usize> {
    let path = active_path(ws)?;
    match ws.tab_for_path(&path) {
        Some(i) => Some(i),
        None => {
            notice(ws, "Zum Ändern das Dokument zuerst bearbeitbar öffnen (Rechtsklick auf eine Version → Abzweigen).");
            None
        }
    }
}

pub fn open_version(ws: &mut Workspace, n: u32) {
    let Some(path) = active_path(ws) else { return notice(ws, "Das Dokument ist noch nicht gespeichert.") };
    if let Err(e) = ws.open_version(&path, n) {
        notice(ws, e.to_string());
    }
}

pub fn restore(ws: &mut Workspace, n: u32) {
    let i = ws.active;
    if let Err(e) = ws.restore_version(i, n) {
        notice(ws, e);
    }
}

pub fn branch_from(ws: &mut Workspace, n: u32) {
    let Some(path) = active_path(ws) else { return notice(ws, "Das Dokument ist noch nicht gespeichert.") };
    let view = match ws.open_version(&path, n) {
        Ok(i) => i,
        Err(e) => return notice(ws, e.to_string()),
    };
    if let Err(e) = ws.make_editable(view) {
        notice(ws, e);
    }
}

pub fn set_comment(ws: &mut Workspace, n: u32, text: &str) {
    let Some(i) = writable(ws) else { return };
    if let Some(store) = ws.tabs[i].store.as_mut()
        && let Err(e) = store.set_comment(n, text)
    {
        notice(ws, e.to_string());
    }
}

pub fn delete_version(ws: &mut Workspace, n: u32) {
    let Some(i) = writable(ws) else { return };
    if let Some(store) = ws.tabs[i].store.as_mut()
        && let Err(e) = store.delete_version(n)
    {
        notice(ws, e.to_string());
    }
}

pub fn delete_branch(ws: &mut Workspace, name: &str) {
    let Some(i) = writable(ws) else { return };
    if let Some(store) = ws.tabs[i].store.as_mut()
        && let Err(e) = store.delete_branch(name)
    {
        notice(ws, e.to_string());
    }
}

/// Setzt den aktiven Zweig; der Tab zeigt weiter seinen Stand (nur das nächste Öffnen lädt die
/// Spitze dieses Zweigs).
pub fn switch_branch(ws: &mut Workspace, name: &str) {
    let Some(i) = writable(ws) else { return };
    if let Some(store) = ws.tabs[i].store.as_mut()
        && let Err(e) = store.set_active_branch(name)
    {
        notice(ws, e.to_string());
    }
}
```

Fehlerfall im Test `rows_are_newest_first…`: `branch_from(&mut w, 1)` ruft `open_version` (Ansicht von v1) und danach `make_editable`, das scheitert, weil das Dokument im Original-Tab beschreibbar offen ist → `Prompt::Notice`; die Ansicht bleibt als schreibgeschützter Tab bestehen. Der Test erwartet genau das.

`show` zeichnet: ohne `version_info()`: „Dieses Dokument ist noch nicht gespeichert.“; sonst Überschrift „Versionen“, bei `tab.is_dirty()` oben die Zeile „Änderungen (nicht gespeichert)“; dann je `Row` ein `selectable_label` mit `format!("{}v{}  {}  {}  {}", "  ".repeat(depth), number, branch, zeit_kurz, kommentar)` (Zeit „JJJJ-MM-TT hh:mm“ aus dem ISO-Text), die aktuelle Version (`tab.based_on == Some(number)`) mit „●“ davor, Spitzen von Nebenzweigen mit „▸“; Doppelklick → `open_version`; Rechtsklick-Menü: *Öffnen*, *Wiederherstellen*, *Abzweigen*, *Kommentar ändern …* (kleines Fenster mit Textfeld), *Löschen …* (Rückfrage „Version vN endgültig löschen?“), bei Zweigspitzen zusätzlich *Zweig löschen …* und *Zu diesem Zweig wechseln*. Schreibgeschützte Tabs zeigen oben „Du siehst eine alte Version.“ mit den Knöpfen „Wiederherstellen“ und „Abzweigen“.

- [ ] **Step 3: Tests grün, fmt, clippy, Commit**

```bash
cargo test -p planora-app && cargo fmt && cargo clippy --workspace --all-targets -- -D warnings
git add -A && git commit -m "feat: Versionen-Panel (Liste, Öffnen, Wiederherstellen, Abzweigen, Kommentar, Löschen)"
```

---

### Task 12: Dokumentation, Gesamtprüfung und Handprüfung

**Files:**
- Modify: `README.md`, `docs/superpowers/offene-punkte.md`, `docs/superpowers/specs/2026-10-10-speichern-und-laden-design.md` (neuer Abschnitt „11. Änderungen bei der Umsetzung“), `tools/gui-drive.ps1` (nur falls für die Handprüfung nötig)

- [ ] **Step 1: Vollständige Prüfung**

```bash
export PATH="$PATH:/c/Users/denni/.cargo/bin" && cd /d/Testumgebung/PlanoraCAD
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep -E "^test result|FAILED|failed" | awk '{p+=$4; f+=$6} END{print "passed",p,"failed",f}' && cargo deny check 2>&1 | tail -3
```
Expected: alles grün (mehr als 596 Tests, 0 fehlgeschlagen). **Zusätzlich** gezielt die Review-Fokus-Fälle: `cargo test -p planora-project` (beschädigte Dateien, atomares Schreiben, Pfade, Zufallstest), `cargo test -p planora-app --test workspace`.

- [ ] **Step 2: README** – im Abschnitt „Feature-Baum“ einen Abschnitt „Speichern, Projekte und Versionen“ ergänzen (einfache Laiensprache, wenig Technik, keine KI-Floskeln): wo die Dateien liegen (`Dokumente\PlanoraCAD`), wie ein Projekt, Ordner und Dokument aussehen, dass jedes Speichern eine neue Version ist, wie man eine alte Version ansieht, wiederherstellt oder abzweigt, dass mehrere Dokumente in Tabs offen sein können und dass nach einem Absturz eine Autosicherung angeboten wird. Den Hinweis zur Handprüfung auf den neuen Stand bringen.

- [ ] **Step 3: Offene Punkte** (`docs/superpowers/offene-punkte.md`, neuer Abschnitt „Teilprojekt 6 – Speichern und Projekte“): Zusammenführen von Zweigen fehlt; keine Cloud/Teilen; Versionen sind vollständige Stände (Delta/Komprimierung später); Sperre wird immer per Rückfrage übernommen (kein Prozess-Test ohne neue Abhängigkeit); unbenannte Dokumente haben keine Autosicherung; schreibgeschützte Tabs sperren nur die Oberfläche (nicht jede Zustandsmethode); keine Vorschaubilder; Kamera wird nicht gespeichert; Verschieben per Ziehen im Projektbaum fehlt (nur Auswahldialog); Umbenennen/Verschieben/Löschen verlangt geschlossene Tabs; Wert-Bereichsprüfung beim Laden nur auf Skizzen und Zahlenfelder (Feature-Werte außerhalb des Bereichs führen zu einem Fehlerstatus statt zu einer Ablehnung); große Skizzen erzeugen große Versionsdateien.

- [ ] **Step 4: Spec §11 „Änderungen bei der Umsetzung“** – P1–P8 aus dem Plankopf (mit Begründung) plus tatsächliche Abweichungen aus den Berichten der Aufgaben (jede mit Grund). Spec §5/§6 Wortlaut dort anpassen, wo er vom Gebauten abweicht (z. B. §10: Skizzen werden beim Laden nicht gelöst; §4: Sperre).

- [ ] **Step 5: Handprüfung (ehrlich berichten)** mit `tools/gui-drive.ps1`:
  1. App starten (`./target/release/planora-app.exe` im Hintergrund), Datei → Speichern unter … öffnet das Fenster; ein Projekt anlegen (Panel „Projekte“ → „Neues Projekt“), Dokument „Test“ speichern.
  2. Quader anlegen → Tab zeigt „●“; Strg+S → Kommentar „zwei“ → „●“ verschwindet.
  3. Versionen-Panel zeigt v1 und v2; Doppelklick auf v1 öffnet „Test · v1“, schreibgeschützt (Erzeugen-Menü deaktiviert).
  4. Rechtsklick v1 → Abzweigen funktioniert nicht, solange der Original-Tab offen ist (Hinweis erwartet); Original-Tab schließen, dann nochmals; Änderung machen, Strg+S → Zweigname-Fenster, „Variante“ eingeben.
  5. App beenden und neu starten; im Projekt-Panel das Dokument öffnen: Stand der aktiven Linie ist da.
  6. Eine zweite Instanz auf dasselbe Dokument: Sperrhinweis.
  Alles, was sich nicht bedienen ließ, ausdrücklich als „nicht geprüft“ berichten.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "docs: Speichern, Projekte und Versionen (README, offene Punkte, Spec)"
```
