# PlanoraCAD – Spec Teilprojekt 1+2: Fundament und Kernel-Basis

Datum: 2026-10-09 · Status: Entwurf zur Freigabe

## 1. Vision und Einordnung

PlanoraCAD ist ein Open-Source-CAD in reinem Rust für Windows und Linux, mit Funktionsumfang im Stil von Autodesk Fusion als langfristigem Ziel.

- **Stufe A (erstes Ziel):** nutzbares Alltags-CAD für Hobby und 3D-Druck: Skizze, Extrusion, Bohrungen, Rundungen, Export.
- **Stufe C (langfristig):** schrittweise Feature-Parität (Baugruppen, Zeichnungen, Rendering, Simulation, …).
- Jede Stufe muss für sich benutzbar sein; die Architektur ist auf Wachstum ausgelegt.

### Teilprojekte (Reihenfolge)

| # | Teilprojekt | Ergebnis |
|---|---|---|
| **1** | **Fundament** (diese Spec) | Workspace, CI, Fenster, 3D-Viewport, Theme |
| **2** | **Kernel Teil 1** (diese Spec) | B-Rep, analytische Flächen, Extrusion, Rotation, Tessellierung |
| 3 | Kernel Teil 2 | Boolesche Operationen |
| 4 | Skizzen | 2D-Skizzen + Constraint-Solver |
| 5 | Feature-Baum | Parametrische Historie, Speichern/Laden |
| 6 | Fasen/Rundungen | Chamfer, Fillet |
| 7 | Export | STL, 3MF, später STEP |
| 8+ | NURBS, Loft, Sweep, Baugruppen, Zeichnungen, … | Weg zu Stufe C |

Diese Spec deckt nur 1 und 2 ab. Jedes weitere Teilprojekt bekommt eigene Spec, eigenen Plan und eigene Umsetzung.

## 2. Getroffene Grundentscheidungen

- **Sprache:** reines Rust, keine C/C++-Abhängigkeit (kein OpenCascade).
- **Kernel:** eigener B-Rep-Kernel mit bewährten Bausteinen (`glam` oder `nalgebra`, `robust`, `earcutr`), nicht `truck`. Dahinter liegt eine eigene Fassade, damit Oberfläche und Kernel entkoppelt bleiben.
- **GUI:** `egui` + `wgpu`, mit Docking (`egui_dock`), eigenem Theme, eigener Schrift und eigenen Icons.
- **Lizenz des Projekts:** GPL-3.0. Abhängigkeiten müssen GPL-3-kompatibel sein (MIT, Apache-2.0, ISC, BSD). Nicht erlaubt: GPL-2-only, proprietär, source-available, Code ohne klare Lizenz.
- **Lizenzprüfung ist Pflicht:** jede neue Abhängigkeit und jedes übernommene Asset wird vorab geprüft; `cargo-deny` erzwingt das in der CI.
- **Autodesk-Abgrenzung:** kein Autodesk-Code, keine Autodesk-Icons oder -Texte, kein Autodesk-/Fusion-Branding, kein Reverse-Engineering von `.f3d`. Offene Formate (STEP, STL, 3MF) sind erlaubt. Die Oberfläche bekommt eigene Gestaltung.
- **Name:** PlanoraCAD (Arbeitstitel). Crate-Namen `planoracad`, `planora-cad`, `planora` waren auf crates.io am 2026-10-09 frei. Eine förmliche Markenrecherche (DPMA, EUIPO, WIPO, Klasse 9) steht vor einer Veröffentlichung noch aus.
- **Plattformen:** Windows (MSVC-Toolchain) und Linux.
- **Zahlen und Einheiten:** `f64`, Einheit Millimeter.

## 3. Workspace-Aufbau

Cargo-Workspace mit klaren Schichten. Abhängigkeiten laufen nur nach unten.

| Crate | Aufgabe | Abhängig von |
|---|---|---|
| `planora-math` | Vektoren, Transformationen, Toleranzen, robuste Prädikate | externe Bibliotheken |
| `planora-kernel` | B-Rep, analytische Flächen/Kurven, Extrusion, Rotation, Validator | `math` |
| `planora-mesh` | Tessellierung (Körper → Dreiecke, Kantenlinien) | `kernel` |
| `planora-render` | `wgpu`-Viewport, Kamera, Picking, Raster | `mesh` |
| `planora-app` | Fenster, `egui`-Oberfläche, Theme, Docking | alle |

**Grundregel:** `planora-kernel` kennt weder Oberfläche noch Grafik. Er ist allein testbar und später einbettbar.

Weitere Projektbestandteile: `deny.toml` (cargo-deny), `rustfmt.toml`, Clippy-Konfiguration, `LICENSE` (GPL-3.0), `README`, CI-Workflow.

## 4. Kernel (`planora-kernel`)

### 4.1 Datenstruktur

- Hierarchie: `Solid` → Schalen (`Shell`, Liste von Flächen-IDs) → Flächen (`Face`) → Schleifen (`Loop`) → Halbkanten (`HalfEdge`) → Kanten (`Edge`) → Ecken (`Vertex`).
- Ecken, Kanten und Flächen liegen in Arenen (Vektoren) und verweisen per **typisierter Index-ID** aufeinander, ohne Zeiger oder `Rc`. Schleifen und Halbkanten liegen direkt in der Fläche (`Face.outer`, `Face.inner`); eine Halbkante ist ein Wert aus Kanten-ID und Richtung. Das hält die Struktur einfach und trotzdem halbkantenbasiert (jede Kante wird genau einmal vorwärts und einmal rückwärts durchlaufen).
- **Topologie und Geometrie sind getrennt:** Eine `Face` verweist auf eine `Surface`, eine `Edge` auf eine `Curve`.
  - `Surface` (Enum): Ebene, Zylinder, Kegel, Kugel, Torus.
  - `Curve` (Enum): Gerade, Kreis, Ellipse.
  - Weitere Varianten (NURBS, B-Splines) kommen später als zusätzliche Enum-Fälle, ohne bestehenden Code zu ändern.

### 4.2 Toleranzen und Robustheit

- Eine zentrale Toleranz-Struktur (Standard: 1e-6 mm linear, angulare Toleranz analog) wird jeder Operation durchgereicht; keine verstreuten Epsilon-Konstanten.
- Orientierungstests nutzen exakte Prädikate (`robust`), damit sich Rundungsfehler nicht aufschaukeln.

### 4.3 Umfang

- Grundkörper: Quader, Zylinder, Kugel.
- Extrusion eines ebenen Profils (Polygon mit Kreisbögen, auch mit Löchern).
- Rotation (Revolve) eines Profils um eine Achse.
- Transformationen: verschieben, drehen, spiegeln und gleichmäßig skalieren (Ähnlichkeitstransformationen; andere Matrizen werden mit Fehler abgelehnt).
- Validator (`planora-kernel`): jede Kante wird genau einmal vorwärts und einmal rückwärts durchlaufen, Schleifen sind geschlossen, Euler-Poincaré-Formel liefert ein ganzzahliges, nicht negatives Geschlecht (Zahl der „Henkel“). Dass die Flächennormalen nach außen zeigen, wird in `planora-mesh` über ein **positives vorzeichenbehaftetes Volumen** geprüft.
- Volumen, Oberfläche und Schwerpunkt werden in `planora-mesh` aus dem Dreiecksnetz berechnet (der Kernel bleibt frei von Tessellierung).
- **Einschränkung dieser Stufe:** Nicht-ebene Flächen sind rechteckige Parameter-Ausschnitte (`UvRect`) einer Zylinder-, Kegel-, Kugel- oder Torusfläche. Das deckt alles ab, was durch Extrusion von Linien und Kreisbögen sowie durch Rotation entsteht. Beliebig berandete gekrümmte Flächen kommen mit den Booleschen Operationen (Teilprojekt 3). Ein vollständiger Torus (geschlossenes Profil) ist in dieser Stufe nicht erzeugbar; Torus-Ausschnitte entstehen durch Rotation eines Kreisbogens außerhalb der Achse.

**Nicht im Umfang:** Boolesche Operationen, Fillets, Chamfer, NURBS, Skizzen-Solver, Speichern/Laden, Export.

### 4.4 Fassade

Die Oberfläche spricht nur mit einer kleinen Schnittstelle (`Kernel`-Trait bzw. Fassaden-Funktionen wie `extrude`, `revolve`, `transform`, `box_`, `cylinder`, `sphere`), nie mit den Interna. So bleiben Oberfläche und Kernel unabhängig austauschbar.

## 5. Tessellierung (`planora-mesh`)

- Jede Fläche wird einzeln trianguliert: Randschleifen im Parameterraum der Fläche, bei Löchern inklusive (`earcutr`).
- Gekrümmte Flächen werden anhand einer Sehnenabweichung unterteilt (Standard 0,01 mm, einstellbar).
- Jedes Dreieck behält die Zuordnung zu seiner Fläche, damit die Auswahl weiß, was getroffen wurde.
- Kanten werden als eigene Linienzüge für die Darstellung erzeugt.

## 6. Viewport (`planora-render`)

- `wgpu` rendert in eine Textur, die `egui` als Panel einbindet.
- Navigation: Orbit, Pan, Zoom mit der Maus, Standardansichten per Navigationswürfel.
- Darstellung: Raster, Ursprungsachsen, schattierte Flächen mit einfacher Beleuchtung, Kantenlinien darüber, 4× MSAA.
- Auswahl per **CPU-Strahlentest** gegen das Dreiecksnetz (Flächen) und per Bildschirmabstand zu den Kantenlinien (Kanten, verdeckte Kanten werden ignoriert): Flächen und Kanten sind anklickbar, Hervorhebung beim Überfahren und bei Auswahl. Das ist ohne GPU testbar und braucht keine Rücklesung vom Grafikspeicher. Ein GPU-ID-Puffer bleibt eine spätere Optimierung für sehr große Modelle.
- `egui` gibt die Render-Einstellungen vor: 4× MSAA und ein 32-Bit-Tiefenpuffer werden in `eframe` eingestellt, der Viewport zeichnet über einen `egui_wgpu`-Paint-Callback direkt in dessen Render-Pass.

## 7. Oberfläche (`planora-app`)

- Dunkles Theme als Standard, helles zuschaltbar, mit eigenen Farben, Abständen und Rundungen. Die Icons sind **selbst gezeichnete Vektorformen** im Code (keine Fremdlizenz). Als Schrift dienen vorerst die in `egui` mitgelieferten Schriften (deren Lizenzen `cargo-deny` und die manuelle Prüfung beim Einrichten erfassen); eine eigene Schrift (z. B. Inter) kommt erst nach Lizenzprüfung.
- Docking-Layout: Browser links, Eigenschaften rechts, Viewport in der Mitte.
- Menü „Erzeugen": Quader, Zylinder, Kugel sowie ein einfacher Extrusions-/Rotationsdialog mit festen Beispielprofilen (echte Skizzen folgen in Teilprojekt 4).
- Der Browser zeigt die erzeugten Körper; die Eigenschaften zeigen Maße, Volumen und Schwerpunkt.

## 8. Tests und Qualitätssicherung

- **Kernel:** Eigenschaftstests (`proptest`): Jeder erzeugte Körper besteht den Validator; das Volumen stimmt mit der Formel überein (z. B. π·r²·h beim Zylinder); zufällige Transformationen verändern das Volumen nicht.
- **Mesh:** Das Netz ist geschlossen (jede Kante gehört zu genau zwei Dreiecken); die Dreiecksfläche entspricht der Körperfläche innerhalb der Toleranz.
- **Render/App:** Automatisiert wird nur Logik (Kamera-Mathematik, Picking-Zuordnung). Die Optik wird per Screenshot und von Hand geprüft.
- **CI (Windows + Linux):** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo deny check`.

## 9. Erfolgskriterien

1. Die App startet auf Windows und Linux.
2. Quader, Zylinder und eine extrudierte Form mit Loch lassen sich erzeugen und werden korrekt dargestellt.
3. Die Ansicht lässt sich flüssig drehen, verschieben und zoomen.
4. Flächen und Kanten sind auswählbar und werden hervorgehoben.
5. Alle Tests und die CI-Prüfungen (inkl. `cargo deny`) laufen grün.

## 10. Voraussetzungen und Risiken

- **Toolchain:** Rust ist auf dem Entwicklungsrechner noch nicht installiert. Der erste Schritt des Plans ist `rustup` plus die Visual-Studio-Build-Tools (MSVC-Linker) auf Windows.
- **Hauptrisiko:** Robuste Boolesche Operationen und später Fillets (Teilprojekte 3, 6). Deshalb wird die Datenstruktur schon hier halbkantenbasiert und toleranzbewusst angelegt.
- **Lizenzrisiko:** Abhängigkeiten ändern gelegentlich ihre Lizenz; `cargo-deny` in der CI fängt das ab.
- **Markenrisiko:** „Planora" wird von fachfremden Firmen genutzt; die Markenrecherche steht vor einer Veröffentlichung noch aus.
